//! Read-only capture, drain and finalization lifecycle.
use super::rows::{CaptureStart, MarketCoverage, WindowEvent};
use super::*;
use crate::status_tap::{CapturePhase, StatusTap};
use std::path::Path;

// ------------------------------------------------------------------------------------------
// Production run
// ------------------------------------------------------------------------------------------

pub(super) fn print_banner(cfg: &MarketConfig) {
    println!("PROJECT 0 — UNFILTERED LIVE MARKET OBSERVER");
    println!("mode=market");
    println!("read_only=true");
    println!("signing=false");
    println!("simulation=false");
    println!("submission=false");
    println!("health_filter=false");
    println!("candidate_filter=false");
    println!("economics_filter=false");
    println!("routing_filter=false");
    println!("funding_filter=false");
    println!("source={}", cfg.source.as_str());
    println!("commitment={}", cfg.commitment);
    println!(
        "source_coverage_verified={}",
        cfg.source.coverage_verified()
    );
    println!("heartbeat_seconds={}", cfg.heartbeat_secs);
    println!(
        "enrichment_available={}",
        cfg.source == MarketSource::Transactions || cfg.rpc_url.is_some()
    );
    println!("duration_seconds={}", cfg.duration_secs);
    println!("program={}", cfg.program);
    println!("ws_host={}", redact_url(&cfg.ws_url));
    if let Some(rpc) = &cfg.rpc_url {
        println!("rpc_host={}", redact_url(rpc));
    }
    println!("output={}", cfg.run_dir.display());
}

/// Longest wait for the reporter to deliver the final capture state.
const STATUS_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// Status-tap view of the run configuration; endpoints only as scheme://host.
fn status_run_snapshot(
    cfg: &MarketConfig,
    start: &CaptureStart,
    process_started_unix_s: u64,
) -> crate::status_tap::RunSnapshot {
    crate::status_tap::RunSnapshot {
        run_id: cfg.run_id.clone(),
        program_id: cfg.program.to_string(),
        source: cfg.source.as_str(),
        commitment: cfg.commitment,
        ws_host: redact_url(&cfg.ws_url),
        rpc_host: cfg.rpc_url.as_deref().map(redact_url),
        configured_duration_seconds: cfg.duration_secs,
        ready_timeout_seconds: cfg.ready_timeout_secs,
        drain_timeout_seconds: cfg.drain_timeout_secs,
        heartbeat_seconds: cfg.heartbeat_secs,
        min_sample_liquidations: cfg.min_sample_liquidations,
        ingress_buffer_events: cfg.ingress_buffer as u64,
        run_dir: cfg.run_dir.display().to_string(),
        enrich_max_pending: cfg.enrich.max_pending as u64,
        enrich_max_attempts: u64::from(cfg.enrich.max_attempts),
        enrich_retry_ms: cfg.enrich.retry_ms,
        enrich_timeout_ms: cfg.enrich.timeout_ms,
        enrich_phase_max_seconds: cfg.enrich.phase_max_secs,
        source_coverage_verified: cfg.source.coverage_verified(),
        enrichment_available: cfg.source == MarketSource::Transactions || cfg.rpc_url.is_some(),
        recovery_budget_ms: start.recovery_budget_ms,
        recovery_attempts: start.recovery_attempts,
        idle_timeout_ms: start.idle_timeout_ms,
        healthy_reset_ms: start.healthy_reset_ms,
        readiness: start.readiness.clone(),
        process_started_unix_s,
    }
}

/// The one classification of a finished capture: what `run_embedded_with` returns and what the
/// ENDED phase reports.
fn capture_end(
    cfg: &MarketConfig,
    quiescent: &Result<(), String>,
    stop_reason: &str,
    stream_ready: bool,
    write_error: Option<&str>,
    ingress_end_error: Option<&str>,
) -> Result<(), String> {
    if let Err(e) = quiescent {
        return Err(e.clone());
    }
    if stop_reason == "OPERATOR_STOP" {
        // Requested stop (also before StreamReady): sealed INCOMPLETE, not a provider failure.
        return Err(format!("MARKET_CAPTURE_INCOMPLETE: {stop_reason}"));
    }
    if !stream_ready {
        let hint = if cfg.source == MarketSource::Transactions {
            " MARKET_OBSERVER_UNSUPPORTED_TRANSACTION_SUBSCRIBE suspected: Helius transactionSubscribe needs a Developer/Business/Professional plan; on the Free plan use P0_MARKET_SOURCE=logs."
        } else {
            ""
        };
        return Err(format!(
            "MARKET_OBSERVER_STREAM_NOT_READY: no StreamReady within {}s (see ingress errors above).{hint}",
            cfg.ready_timeout_secs
        ));
    }
    if let Some(e) = write_error {
        return Err(format!("market evidence write failed: {e}"));
    }
    if stop_reason != "TIMER" {
        return Err(format!("MARKET_CAPTURE_INCOMPLETE: {stop_reason}"));
    }
    if let Some(code) = ingress_end_error {
        return Err(format!("MARKET_CAPTURE_INCOMPLETE: {code}"));
    }
    Ok(())
}

/// Facts handed from the capture/drain phases to the shared finishing path.
pub struct CaptureOutcome {
    pub process_started_unix_s: u64,
    pub ready_unix_s: Option<u64>,
    /// `(start_t_rx_ns, cutoff_t_rx_ns)` if the stream became ready.
    pub window: Option<(u64, u64)>,
    pub capture_end_ns: u64,
    pub observation_end_unix_s: u64,
    pub stop_reason: &'static str,
    pub drain_complete: bool,
    pub write_error: Option<String>,
}

/// Production finishing path used by `run()`: phase 3 (enrichment, logs only), summary and
/// report. A missing `P0_RPC_URL` or a failing enrichment client is recorded
/// (`ENRICHMENT_INIT_FAILED`, `enrichment_init_error`) and never prevents summary/report.
/// Returns `(summary, report, write_error)`.
/// Waits up to `budget` for a stopped ingress thread and joins it (D8). `join` itself cannot
/// time out, so the thread is polled first; one still running is `INGRESS_NOT_QUIESCENT`.
pub(super) fn quiesce<T>(
    ingress: std::thread::JoinHandle<T>,
    budget: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + budget;
    while !ingress.is_finished() {
        if Instant::now() >= deadline {
            return Err("INGRESS_NOT_QUIESCENT".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    ingress
        .join()
        .map(drop)
        .map_err(|_| "INGRESS_THREAD_PANIC".to_string())
}

/// The market ingress thread's result: `Err` holds a fixed, secret-free error class.
pub(super) type IngressThread = std::thread::JoinHandle<Result<(), &'static str>>;

/// R2: how a finished ingress thread ended. A requested stop ends the WSS source with `Ok`; an
/// error return or a panic is never a clean end of the stream.
fn ingress_end(ingress: IngressThread) -> Result<(), &'static str> {
    match ingress.join() {
        Ok(result) => result,
        Err(_) => Err("INGRESS_THREAD_PANIC"),
    }
}

/// Phase 2 of a capture: drain the queue after the stop request until the ingress thread has
/// ended (or the deadline passes). R2: a finished thread is joined here, before the journal is
/// sealed, so a panic or an error return marks the capture INCOMPLETE instead of counting as a
/// complete drain. Returns `(drain_complete, write_error, ingress_end_error)`; `ingress` is
/// `None` afterwards if the thread was joined.
pub(super) fn drain_after_stop(
    consumer: &darkbot_common::channel::Consumer<IngressEvent>,
    ingress: &mut Option<IngressThread>,
    core: &mut ObserverCore,
    sink: &mut dyn MarketSink,
    clock: Clock,
    deadline: Instant,
    last_overflow: &mut u64,
) -> (bool, Option<String>, Option<&'static str>) {
    let mut write_error: Option<String> = None;
    let mut drain_complete = false;
    let mut end_error = None;
    while write_error.is_none() {
        let now_s = unix_now_s();
        match consumer.pop_timeout(Duration::from_millis(100)) {
            Some(ev) => {
                let event_unix_s = (clock.unix_ns(ev.t_rx_ns()) / 1_000_000_000) as u64;
                if let Err(e) = core.on_ingress_event(ev, event_unix_s, sink) {
                    write_error = Some(e.to_string());
                }
            }
            None if ingress.as_ref().is_none_or(|t| t.is_finished()) => {
                while let Some(ev) = consumer.try_pop() {
                    let event_unix_s = (clock.unix_ns(ev.t_rx_ns()) / 1_000_000_000) as u64;
                    if let Err(e) = core.on_ingress_event(ev, event_unix_s, sink) {
                        write_error = Some(e.to_string());
                        break;
                    }
                }
                if let Some(Err(code)) = ingress.take().map(ingress_end) {
                    core.mark_incomplete(code);
                    end_error = Some(code);
                }
                drain_complete = write_error.is_none() && end_error.is_none();
                break;
            }
            None => {}
        }
        let overflow = consumer.stats().overflow;
        if overflow > *last_overflow {
            *last_overflow = overflow;
            let _ = core.coverage(
                sink,
                now_s,
                None,
                WindowEvent::IngressOverflow {
                    overflow_total: overflow,
                    drain: true,
                },
            );
        }
        if Instant::now() >= deadline {
            break;
        }
    }
    (drain_complete, write_error, end_error)
}

pub fn finish_capture(
    cfg: &MarketConfig,
    core: &mut ObserverCore,
    sink: &mut dyn MarketSink,
    outcome: CaptureOutcome,
) -> Result<(MarketFinal, String, Option<String>), String> {
    let write_error = outcome.write_error;
    // Outage abort is not a request for a lengthy repair scan. Preserve evidence
    // for explicit resumable verification; stop capture/Shadow first.
    let verify_allowed = outcome.stop_reason == "TIMER";
    if !verify_allowed {
        core.mark_incomplete("TRANSPORT_CAPTURE_ABORT");
        core.reconciliation.error = Some("VERIFICATION_DEFERRED_AFTER_ABORT");
    }
    // Capture finalization is local-only. Historical repair/enrichment belongs
    // to the separately budgeted verifier after the immutable raw manifest.
    // Keep every pending gap/truncated record as evidence, never mark repaired.
    if verify_allowed {
        core.reconciliation.error = Some("VERIFICATION_NOT_RUN");
    }
    let ready = outcome.ready_unix_s;
    let ctx = SummaryContext {
        run_id: cfg.run_id.clone(),
        commitment: cfg.commitment.to_string(),
        process_started_at: utc_iso(outcome.process_started_unix_s),
        stream_ready_at: ready.map(utc_iso),
        observation_started_at: ready.map(utc_iso),
        observation_ended_at: ready.map(|_| utc_iso(outcome.observation_end_unix_s)),
        configured_duration_seconds: cfg.duration_secs as f64,
        actual_observation_seconds: outcome
            .window
            .map(|(start, cutoff)| {
                (outcome.capture_end_ns.min(cutoff).saturating_sub(start)) as f64 / 1e9
            })
            .unwrap_or(0.0),
        stream_ready: outcome.window.is_some(),
        full_duration_observed: outcome.stop_reason == "TIMER" && outcome.window.is_some(),
        drain_complete: outcome.drain_complete,
        min_sample_liquidations: cfg.min_sample_liquidations,
        enrichment_available: cfg.source == MarketSource::Transactions || cfg.rpc_url.is_some(),
    };
    if core.deployment_before.is_some() {
        core.deployment_error = Some("POST_CAPTURE_DEPLOYMENT_VERIFICATION_NOT_RUN");
    }
    let (summary, report) = finalize(
        core,
        sink,
        &ctx,
        &cfg.run_dir,
        unix_now_s(),
        WindowEvent::Stopped {
            reason: outcome.stop_reason.into(),
            drain_complete: outcome.drain_complete,
            write_error: write_error.clone(),
        },
    )?;
    Ok((summary, report, write_error))
}

pub fn run_from_env() -> Result<(), String> {
    run_from_env_with(None, |_, _| Ok(()))
}
pub fn run_from_env_with(
    status_tap: Option<StatusTap>,
    before_start: impl FnOnce(&str, &Path) -> Result<(), String>,
) -> Result<(), String> {
    let cfg = MarketConfig::from_lookup(&|k| std::env::var(k).ok(), unix_now_s())?;
    before_start(&cfg.run_id, &cfg.run_dir)?;
    run_embedded_with(cfg, Clock::new(), None, status_tap)
}

pub fn run(cfg: MarketConfig) -> Result<(), String> {
    run_embedded_with(cfg, Clock::new(), None, None)
}

/// Existing C0-MO capture/drain/enrichment with an optional typed evidence tap.
pub fn run_embedded(
    cfg: MarketConfig,
    clock: Clock,
    econ: Option<crate::analytics::econ::Handle>,
) -> Result<(), String> {
    run_embedded_with(cfg, clock, econ, None)
}
pub fn run_embedded_with(
    cfg: MarketConfig,
    clock: Clock,
    econ: Option<crate::analytics::econ::Handle>,
    status_tap: Option<StatusTap>,
) -> Result<(), String> {
    if let Some(tap) = &status_tap {
        tap.publish_operational(crate::status_tap::operational_snapshot());
        tap.publish_phase(|p| p.phase = CapturePhase::WaitingForStream);
    }
    // One terminal publication for every return path, including initialization and
    // evidence-write failures before the capture loop begins.
    let result = run_embedded_inner(cfg, clock, econ, status_tap.clone());
    if let Some(tap) = &status_tap {
        tap.publish_phase(|p| {
            p.phase = CapturePhase::Ended;
            p.end_result = result
                .as_ref()
                .err()
                .cloned()
                .unwrap_or_else(|| "CAPTURE_COMPLETE".into());
        });
        let _ = tap.flush(STATUS_FLUSH_TIMEOUT);
    }
    result
}

fn run_embedded_inner(
    cfg: MarketConfig,
    clock: Clock,
    econ: Option<crate::analytics::econ::Handle>,
    status_tap: Option<StatusTap>,
) -> Result<(), String> {
    let process_started = unix_now_s();
    fs::create_dir_all(&cfg.run_dir)
        .map_err(|e| format!("create {}: {e}", cfg.run_dir.display()))?;
    let mut sink = JournalSink::create(&cfg.run_dir)?;
    if econ.is_none() {
        print_banner(&cfg);
    }

    if let Some(h) = &econ {
        // No measurement clock or market ingress yet: initialization is separately
        // bounded and recorded, so a short run measures a running Shadow bot.
        eprintln!(
            "C0_E1_WARMUP_WAIT timeout_s={}",
            h.cfg.plan.budgets.bootstrap_seconds
        );
        sink.coverage(&MarketCoverage::ShadowWarmupWait {
            observed_unix_s: unix_now_s(),
        })
        .map_err(|_| "WARMUP_EVIDENCE_WRITE".to_string())?;
        if let Err(e) =
            h.wait_shadow_ready(Duration::from_secs(h.cfg.plan.budgets.bootstrap_seconds))
        {
            let _ = sink.coverage(&MarketCoverage::ShadowWarmupFailed { error: e.clone() });
            let _ = sink.flush();
            eprintln!("C0_E1_WARMUP_FAILED code={e}");
            return Err(e);
        }
        sink.coverage(&MarketCoverage::ShadowWarmupReady {
            at_ns: clock.now_ns(),
            observed_unix_s: unix_now_s(),
        })
        .map_err(|_| "WARMUP_EVIDENCE_WRITE".to_string())?;
        crate::analytics::measurement::journal(
            &h.cfg.paths(),
            clock,
            crate::analytics::measurement::Phase::WaitStreamReady {
                budget_seconds: h.cfg.plan.budgets.stream_ready_seconds,
            },
        )?;
        eprintln!("C0_E1_WARMUP_READY; starting market stream and measurement window");
    }

    let deployment_before = paths::inspect(cfg.rpc_url.as_deref(), &cfg.program);
    let (producer, consumer) = bounded::<IngressEvent>("market-ingress", cfg.ingress_buffer);
    let halt = IngressHalt::new();
    let slots = SlotWatch::new();
    let session = Shutdown::new();
    let ws = market_ws_config(&cfg)?;
    let start = CaptureStart {
        source: cfg.source.as_str().into(),
        commitment: cfg.commitment.into(),
        program: cfg.program,
        configured_duration_seconds: cfg.duration_secs,
        ws_host: redact_url(&cfg.ws_url),
        recovery_budget_ms: ws.recovery_budget.map(|d| d.as_millis() as u64),
        recovery_attempts: ws.recovery_attempts,
        idle_timeout_ms: ws.idle_timeout.as_millis() as u64,
        healthy_reset_ms: darkbot_solana::ingress::ws_rpc::HEALTHY_RESET.as_millis() as u64,
        readiness: "ALL_ACKS_AND_NEW_SLOT_PROGRESS".into(),
        provider_budget: darkbot_solana::provider::shared().snapshot(),
    };
    let src = WsRpcControlSource::new(ws, clock);
    let session_for_thread = session.clone();
    let ingress: IngressThread = std::thread::Builder::new()
        .name("market-ingress-ws".into())
        .spawn(move || {
            src.run(producer, session_for_thread, halt, slots)
                .map_err(|e| {
                    let class = market_ingress_error_class(&e);
                    // R6: bounded console, fixed class (never a provider text or URL).
                    crate::console::line(format!("market ingress ended: {class}"));
                    class
                })
        })
        .map_err(|e| format!("spawn ingress: {e}"))?;
    let mut ingress = Some(ingress);

    let mut core = ObserverCore::new(cfg.program, cfg.source, cfg.enrich);
    match deployment_before {
        Ok(v) => core.deployment_before = Some(v),
        Err(e) => core.deployment_error = Some(e),
    }
    core.econ = econ.clone();
    core.status_tap = status_tap.clone();
    core.gap_cfg = cfg.gap;
    core.reconcile_cfg = cfg.reconcile;
    if let Some(tap) = &status_tap {
        tap.publish_run(status_run_snapshot(&cfg, &start, process_started));
    }
    // Provisional all-path counts come from the existing program-attributed
    // WebSocket decoder. Canonical RPC happens only after Shadow is quiescent.
    let mut last_live_display = Instant::now() - Duration::from_secs(5);
    let mut checkpoint_index = 0usize;
    core.coverage(
        &mut sink,
        process_started,
        None,
        WindowEvent::Start(Box::new(start)),
    )
    .map_err(|e| format!("market evidence write failed: {e}"))?;

    let started = Instant::now();
    let ready_timeout = Duration::from_secs(cfg.ready_timeout_secs);
    let duration_ns = if cfg.duration_secs == 0 {
        u64::MAX
    } else {
        cfg.duration_secs.saturating_mul(1_000_000_000)
    };
    let mut ready_unix_s: Option<u64> = None;
    let mut window: Option<(u64, u64)> = None;
    let mut last_overflow = 0u64;
    let mut last_flush = Instant::now();
    let mut stop_reason = "TIMER";
    let mut write_error: Option<String> = None;

    // ---- phase 1: CAPTURE (no RPC) ----
    loop {
        if econ
            .as_ref()
            .is_some_and(|h| h.shared.abort.load(std::sync::atomic::Ordering::Acquire))
        {
            let operator = econ.as_ref().is_some_and(|h| {
                h.shared
                    .operator_stop
                    .load(std::sync::atomic::Ordering::Acquire)
            });
            stop_reason = if operator {
                "OPERATOR_STOP"
            } else {
                "C0_E1_ABORT"
            };
            break;
        }
        if econ.is_none() && crate::orchestration::control::stop_requested() {
            stop_reason = "OPERATOR_STOP";
            break;
        }
        if let (Some(h), Some((start, cutoff))) = (&econ, window) {
            let due = h
                .cfg
                .plan
                .due(clock.now_ns().min(cutoff).saturating_sub(start));
            while checkpoint_index < due {
                checkpoint_index += 1;
                let seconds = h.cfg.plan.seconds(checkpoint_index);
                let summary = core.summary(&SummaryContext {
                    run_id: cfg.run_id.clone(),
                    commitment: cfg.commitment.into(),
                    configured_duration_seconds: seconds,
                    actual_observation_seconds: seconds,
                    stream_ready: true,
                    full_duration_observed: true,
                    drain_complete: false,
                    min_sample_liquidations: cfg.min_sample_liquidations,
                    ..Default::default()
                });
                h.emit(crate::analytics::econ::Evidence::MarketCheckpoint {
                    index: checkpoint_index,
                    summary: Box::new(summary),
                });
            }
        }
        if window.is_none() && started.elapsed() >= ready_timeout {
            stop_reason = "STREAM_NOT_READY";
            break;
        }
        if window.is_some_and(|(_, cutoff)| clock.now_ns() >= cutoff) {
            break;
        }
        if ingress.as_ref().is_some_and(|t| t.is_finished()) {
            if let Err(e) = core.coverage(
                &mut sink,
                unix_now_s(),
                Some(clock.now_ns()),
                WindowEvent::IngressThreadEnded,
            ) {
                write_error = Some(e.to_string());
            }
            stop_reason = "INGRESS_THREAD_ENDED";
            break;
        }
        let wait = window
            .map(|(_, cutoff)| Duration::from_nanos(cutoff.saturating_sub(clock.now_ns())))
            .unwrap_or(Duration::from_secs(1))
            .min(Duration::from_millis(250));
        let now_s = unix_now_s();
        let step = match consumer.pop_timeout(wait) {
            Some(IngressEvent::Reconnected {
                attempt, t_rx_ns, ..
            }) if window.is_none() => {
                let cutoff = t_rx_ns.saturating_add(duration_ns);
                window = Some((t_rx_ns, cutoff));
                ready_unix_s = Some((clock.unix_ns(t_rx_ns) / 1_000_000_000) as u64);
                core.start_observation(t_rx_ns, cutoff);
                if let Some(tap) = &status_tap {
                    tap.publish_phase(|p| {
                        p.phase = CapturePhase::Capture;
                        p.stream_ready_unix_s = ready_unix_s;
                    });
                }
                if let Some(h) = &econ {
                    let evidence = h.cfg.plan.capture_window(clock, t_rx_ns)?;
                    crate::analytics::measurement::journal(
                        &h.cfg.paths(),
                        clock,
                        crate::analytics::measurement::Phase::Capture(evidence),
                    )?;
                    h.start(t_rx_ns, cutoff);
                }
                core.coverage(
                    &mut sink,
                    now_s,
                    Some(t_rx_ns),
                    WindowEvent::StreamReady {
                        attempt,
                        observation_start_t_rx_ns: t_rx_ns,
                        observation_cutoff_t_rx_ns: cutoff,
                    },
                )
            }
            Some(ev) => {
                let event_unix_s = (clock.unix_ns(ev.t_rx_ns()) / 1_000_000_000) as u64;
                core.on_ingress_event(ev, event_unix_s, &mut sink)
            }
            None => Ok(()),
        };
        if let Err(e) = step {
            write_error = Some(e.to_string());
            stop_reason = "WRITE_ERROR";
            break;
        }
        if last_live_display.elapsed() >= Duration::from_secs(2) {
            last_live_display = Instant::now();
            let gross: f64 = core.edges.iter().sum();
            crate::console::line(format!(
                "[MARKET/LIVE] phase={} tx={} success={} provisional_classic={} provisional_receivership={} provisional_deleverage={} provisional_bankruptcy={} receiver_gross_usd={:.6} gaps={} decoder_errors={} canonical_status=POST_CAPTURE_ONLY",
                if window.is_some() { "CAPTURE" } else { "WAITING_FOR_MARKET_STREAM" },
                core.stats.unique_transactions,
                core.stats.successful_transactions,
                core.stats.provisional_classic_events,
                core.stats.liquidation_events,
                core.stats.provisional_deleverage_events,
                core.stats.provisional_bankruptcy_events,
                gross,
                core.stats.gap_count,
                core.stats.decoder_error_count
            ));
            // SK-05: queue, rates, lag and exhaustion forecasts of the writer lane.
            if let Some(status) = sink.storage_status() {
                crate::console::line(status.terminal());
                if let Some(tap) = &status_tap {
                    tap.publish_storage(status);
                }
            }
            if let Some(tap) = &status_tap {
                tap.publish_assessment(core.assessment_snapshot());
            }
        }
        let overflow = consumer.stats().overflow;
        if overflow > last_overflow {
            last_overflow = overflow;
            if let Err(e) = core.coverage(
                &mut sink,
                now_s,
                None,
                WindowEvent::IngressOverflow {
                    overflow_total: overflow,
                    drain: false,
                },
            ) {
                write_error = Some(e.to_string());
                stop_reason = "WRITE_ERROR";
                break;
            }
        }
        if last_flush.elapsed() >= Duration::from_secs(5) {
            last_flush = Instant::now();
            if let Err(e) = sink.flush() {
                write_error = Some(e.to_string());
                stop_reason = "WRITE_ERROR";
                break;
            }
        }
    }
    let capture_end_ns = clock.now_ns();
    // Freeze an open-ended window at the operator stop before draining queued ingress.
    if cfg.duration_secs == 0 {
        if let Some((start, _)) = window {
            window = Some((start, capture_end_ns));
            core.start_observation(start, capture_end_ns);
        }
    }
    if let Some(tap) = &status_tap {
        tap.publish_phase(|p| {
            p.phase = CapturePhase::Drain;
            p.stop_reason = stop_reason;
        });
    }
    if let Some(h) = &econ {
        crate::analytics::measurement::journal(
            &h.cfg.paths(),
            clock,
            crate::analytics::measurement::Phase::Drain {
                stop_reason: stop_reason.into(),
                actual_capture_end_ns: capture_end_ns,
                actual_capture_elapsed_ns: window
                    .map(|(start, cutoff)| capture_end_ns.min(cutoff).saturating_sub(start)),
                shadow_budget_seconds: h.cfg.plan.budgets.shadow_drain_seconds,
                market_budget_seconds: h.cfg.plan.budgets.market_drain_seconds,
            },
        )?;
    }
    if stop_reason != "TIMER" {
        core.mark_incomplete("TRANSPORT_CAPTURE_ABORT");
        if let Some(h) = &econ {
            h.shared
                .market_failed
                .store(true, std::sync::atomic::Ordering::Release);
            h.shared
                .abort
                .store(true, std::sync::atomic::Ordering::Release);
        }
    }
    let observation_end_s = window
        .map(|(_, cutoff)| (clock.unix_ns(capture_end_ns.min(cutoff)) / 1_000_000_000) as u64)
        .unwrap_or_else(unix_now_s);

    // ---- phase 2: STOP ingress + DRAIN (events with t_rx <= cutoff still count) ----
    let _ = core.coverage(
        &mut sink,
        observation_end_s,
        None,
        WindowEvent::StopRequested {
            reason: stop_reason.into(),
        },
    );
    session.request();
    let drain_deadline = Instant::now() + Duration::from_secs(cfg.drain_timeout_secs);
    let (drain_complete, drain_write_error, ingress_end_error) = if write_error.is_none() {
        drain_after_stop(
            &consumer,
            &mut ingress,
            &mut core,
            &mut sink,
            clock,
            drain_deadline,
            &mut last_overflow,
        )
    } else {
        (false, None, None)
    };
    if drain_write_error.is_some() {
        write_error = drain_write_error;
    }
    core.stop_observation();
    let events_after_cutoff = core.stats.events_after_cutoff;
    let _ = core.coverage(
        &mut sink,
        unix_now_s(),
        None,
        WindowEvent::Drained {
            drain_complete,
            events_after_cutoff,
        },
    );

    // A separate capture drain is insufficient: Shadow keeps winner RPCs alive
    // through its own drain. Release the heavy scan only after ALL workers exit.
    if let Some(h) = &econ {
        let barrier = h.wait_shadow_quiescent(Duration::from_secs(
            h.cfg.plan.budgets.shadow_drain_seconds + h.cfg.plan.budgets.quiescence_grace_seconds,
        ));
        let at_ns = clock.now_ns();
        let event = match &barrier {
            Ok(_) => WindowEvent::ShadowRpcQuiescent { at_ns },
            Err(e) => WindowEvent::ShadowRpcBarrierFailed {
                at_ns,
                error: e.clone(),
            },
        };
        if let Err(e) = core.coverage(&mut sink, unix_now_s(), None, event) {
            write_error = Some(e.to_string());
        }
        if let Err(e) = barrier {
            write_error = Some(e);
        }
    }

    if let Some(h) = &econ {
        crate::analytics::measurement::journal(
            &h.cfg.paths(),
            clock,
            crate::analytics::measurement::Phase::Verification {
                strict_seconds: h.cfg.plan.budgets.strict_seconds,
                gap_seconds: h.cfg.plan.budgets.gap_seconds,
                enrichment_seconds: h.cfg.plan.budgets.enrichment_seconds,
                shadow_quiescent: write_error.is_none(),
            },
        )?;
    }
    if let Some(tap) = &status_tap {
        let error = write_error.clone();
        tap.publish_phase(|p| {
            p.phase = CapturePhase::Finalize;
            p.drain_complete = Some(drain_complete);
            p.write_error = error;
        });
    }

    // ---- phase 3 + summary + report: one shared production path ----
    let outcome = CaptureOutcome {
        process_started_unix_s: process_started,
        ready_unix_s,
        window,
        capture_end_ns,
        observation_end_unix_s: observation_end_s,
        stop_reason,
        drain_complete,
        write_error,
    };
    let (summary, report, write_error) = finish_capture(&cfg, &mut core, &mut sink, outcome)?;
    // D8: the session was asked to stop; its thread is joined within the drain budget. One still
    // alive afterwards is reported, never declared quiescent (the journal is INCOMPLETE then,
    // since its drain did not complete).
    let quiescent = match ingress.take() {
        Some(thread) => quiesce(thread, Duration::from_secs(cfg.drain_timeout_secs)),
        None => Ok(()),
    };
    let end = capture_end(
        &cfg,
        &quiescent,
        stop_reason,
        window.is_some(),
        write_error.as_deref(),
        ingress_end_error,
    );
    if let Some(tap) = &status_tap {
        tap.publish_market(core.market_snapshot());
        tap.publish_assessment(core.assessment_snapshot());
        tap.publish_phase(|p| p.write_error = write_error.clone());
    }
    if let Some(h) = &econ {
        h.emit(crate::analytics::econ::Evidence::MarketFinal(Box::new(
            summary,
        )));
    } else {
        print!("{report}");
    }
    end
}

/// A fixed, secret-free class of a market ingress error (R2/R6).
fn market_ingress_error_class(e: &darkbot_solana::ingress::IngressError) -> &'static str {
    use darkbot_solana::ingress::IngressError as E;
    match e {
        E::Connect(c) if c == "WSS_RECOVERY_EXHAUSTED" => "WSS_RECOVERY_EXHAUSTED",
        E::Connect(_) => "INGRESS_CONNECT_ERROR",
        E::Protocol(_) => "INGRESS_PROTOCOL_ERROR",
        E::Config(_) => "INGRESS_CONFIG_ERROR",
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;

    fn config() -> MarketConfig {
        MarketConfig::from_lookup(
            &|key| match key {
                "P0_PROGRAM_ID" => Some("11111111111111111111111111111111".into()),
                "P0_WS_URL" => Some("wss://example.invalid/private/key?api-key=TEST_SECRET".into()),
                "P0_MARKET_SOURCE" => Some("logs".into()),
                _ => None,
            },
            1,
        )
        .unwrap()
    }

    #[test]
    fn initialization_failure_publishes_the_exact_terminal_result() {
        let mut cfg = config();
        let path = std::env::temp_dir().join(format!(
            "p0-init-failure-{}-{}",
            std::process::id(),
            crate::status_tap::unix_ms_now()
        ));
        fs::write(&path, b"not a directory").unwrap();
        cfg.run_dir = path.clone();
        let tap = StatusTap::new();
        let error = run_embedded_with(cfg, Clock::new(), None, Some(tap.clone())).unwrap_err();
        let state = tap.reader().wait_changed(Duration::ZERO).unwrap();
        let phase = state.phase.unwrap();
        assert_eq!(phase.phase, CapturePhase::Ended);
        assert_eq!(phase.end_result, error);
        assert!(state.operational.is_some());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn terminal_classification_preserves_error_precedence() {
        let cfg = config();
        assert_eq!(
            capture_end(
                &cfg,
                &Err("INGRESS_TIMEOUT".into()),
                "OPERATOR_STOP",
                false,
                Some("WRITE"),
                Some("END")
            ),
            Err("INGRESS_TIMEOUT".into())
        );
        assert_eq!(
            capture_end(&cfg, &Ok(()), "OPERATOR_STOP", false, None, None),
            Err("MARKET_CAPTURE_INCOMPLETE: OPERATOR_STOP".into())
        );
        assert!(
            capture_end(&cfg, &Ok(()), "TIMER", false, Some("WRITE"), None)
                .unwrap_err()
                .starts_with("MARKET_OBSERVER_STREAM_NOT_READY:")
        );
        assert_eq!(
            capture_end(&cfg, &Ok(()), "TIMER", true, Some("WRITE"), None),
            Err("market evidence write failed: WRITE".into())
        );
        assert_eq!(
            capture_end(&cfg, &Ok(()), "TIMER", true, None, Some("END")),
            Err("MARKET_CAPTURE_INCOMPLETE: END".into())
        );
        assert_eq!(
            capture_end(&cfg, &Ok(()), "TIMER", true, None, None),
            Ok(())
        );
    }
}

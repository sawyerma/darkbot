//! Read-only capture, drain and finalization lifecycle.
use super::rows::{CaptureStart, MarketCoverage, WindowEvent};
use super::*;

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
pub fn finish_capture(
    cfg: &MarketConfig,
    core: &mut ObserverCore,
    sink: &mut dyn MarketSink,
    outcome: CaptureOutcome,
) -> Result<(Value, String, Option<String>), String> {
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
    let cfg = MarketConfig::from_lookup(&|k| std::env::var(k).ok(), unix_now_s())?;
    run(cfg)
}

pub fn run(cfg: MarketConfig) -> Result<(), String> {
    run_embedded(cfg, Clock::new(), None)
}

/// Existing C0-MO capture/drain/enrichment with an optional typed evidence tap.
pub fn run_embedded(
    cfg: MarketConfig,
    clock: Clock,
    econ: Option<crate::analytics::econ::Handle>,
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
            "WAIT_STREAM_READY",
            json!({"budget_seconds":h.cfg.plan.budgets.stream_ready_seconds}),
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
    let ingress = std::thread::Builder::new()
        .name("market-ingress-ws".into())
        .spawn(move || {
            if let Err(e) = src.run(producer, session_for_thread, halt, slots) {
                eprintln!("market ingress ended: {e}");
            }
        })
        .map_err(|e| format!("spawn ingress: {e}"))?;

    let mut core = ObserverCore::new(cfg.program, cfg.source, cfg.enrich);
    match deployment_before {
        Ok(v) => core.deployment_before = Some(v),
        Err(e) => core.deployment_error = Some(e),
    }
    core.econ = econ.clone();
    core.gap_cfg = cfg.gap;
    core.reconcile_cfg = cfg.reconcile;
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
    let duration_ns = cfg.duration_secs.saturating_mul(1_000_000_000);
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
                    summary,
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
        if ingress.is_finished() {
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
                if let Some(h) = &econ {
                    let evidence = h.cfg.plan.capture_window(clock, t_rx_ns)?;
                    crate::analytics::measurement::journal(
                        &h.cfg.paths(),
                        clock,
                        "CAPTURE",
                        evidence,
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
            eprintln!(
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
            );
            // SK-05: queue, rates, lag and exhaustion forecasts of the writer lane.
            if let Some(status) = sink.storage_status() {
                eprintln!("{}", status.terminal());
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
    if let Some(h) = &econ {
        crate::analytics::measurement::journal(
            &h.cfg.paths(),
            clock,
            "DRAIN",
            json!({
            "stop_reason":stop_reason,"actual_capture_end_ns":capture_end_ns,
            "actual_capture_elapsed_ns":window.map(|(start,cutoff)|capture_end_ns.min(cutoff).saturating_sub(start)),
            "shadow_budget_seconds":h.cfg.plan.budgets.shadow_drain_seconds,
            "market_budget_seconds":h.cfg.plan.budgets.market_drain_seconds}),
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
    let mut drain_complete = false;
    let drain_deadline = Instant::now() + Duration::from_secs(cfg.drain_timeout_secs);
    while write_error.is_none() {
        let now_s = unix_now_s();
        match consumer.pop_timeout(Duration::from_millis(100)) {
            Some(ev) => {
                let event_unix_s = (clock.unix_ns(ev.t_rx_ns()) / 1_000_000_000) as u64;
                if let Err(e) = core.on_ingress_event(ev, event_unix_s, &mut sink) {
                    write_error = Some(e.to_string());
                }
            }
            None if ingress.is_finished() => {
                while let Some(ev) = consumer.try_pop() {
                    let event_unix_s = (clock.unix_ns(ev.t_rx_ns()) / 1_000_000_000) as u64;
                    if let Err(e) = core.on_ingress_event(ev, event_unix_s, &mut sink) {
                        write_error = Some(e.to_string());
                        break;
                    }
                }
                drain_complete = write_error.is_none();
                break;
            }
            None => {}
        }
        let overflow = consumer.stats().overflow;
        if overflow > last_overflow {
            last_overflow = overflow;
            let _ = core.coverage(
                &mut sink,
                now_s,
                None,
                WindowEvent::IngressOverflow {
                    overflow_total: overflow,
                    drain: true,
                },
            );
        }
        if Instant::now() >= drain_deadline {
            break;
        }
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
            "VERIFICATION",
            json!({
            "strict_seconds":h.cfg.plan.budgets.strict_seconds,"gap_seconds":h.cfg.plan.budgets.gap_seconds,
            "enrichment_seconds":h.cfg.plan.budgets.enrichment_seconds,"shadow_quiescent":write_error.is_none()}),
        )?;
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
    if ingress.is_finished() {
        let _ = ingress.join();
    }
    if let Some(h) = &econ {
        h.emit(crate::analytics::econ::Evidence::MarketFinal(summary));
    } else {
        print!("{report}");
    }
    if stop_reason == "OPERATOR_STOP" {
        // Requested stop (also before StreamReady): sealed INCOMPLETE, not a provider failure.
        return Err(format!("MARKET_CAPTURE_INCOMPLETE: {stop_reason}"));
    }
    if window.is_none() {
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
    Ok(())
}

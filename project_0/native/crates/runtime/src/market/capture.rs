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
/// ENDED phase reports. A capture whose drain did not complete is sealed INCOMPLETE and never
/// returns `Ok` — also when its ingress thread ends after the seal and is joined quiescent (R1).
pub(super) fn capture_end(
    cfg: &MarketConfig,
    quiescent: &Result<(), String>,
    stop_reason: &str,
    stream_ready: bool,
    write_error: Option<&str>,
    ingress_end_error: Option<&str>,
    drain_complete: bool,
) -> Result<(), String> {
    if let Err(e) = quiescent {
        return Err(e.clone());
    }
    // DEC-16: an unbounded capture ends only by its stop, its maximum runtime or the end of its
    // standing approval — an ordered end only with nothing failed and its drain complete.
    let unbounded_end =
        cfg.duration_secs == 0 && crate::orchestration::until_stopped::ordered_reason(stop_reason);
    if (stop_reason == "OPERATOR_STOP" && !unbounded_end) || (unbounded_end && !stream_ready) {
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
    if stop_reason != "TIMER" && !unbounded_end {
        return Err(format!("MARKET_CAPTURE_INCOMPLETE: {stop_reason}"));
    }
    if let Some(code) = ingress_end_error {
        return Err(format!("MARKET_CAPTURE_INCOMPLETE: {code}"));
    }
    if !drain_complete {
        return Err("MARKET_CAPTURE_INCOMPLETE: DRAIN_INCOMPLETE".into());
    }
    if unbounded_end {
        // The ordered end of an unbounded capture: complete, by its stop (never "full duration").
        return Err(format!("MARKET_CAPTURE_INCOMPLETE: {stop_reason}"));
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
    /// When the capture finished, on its validated clock (review 11: finalization never reads a
    /// wall clock that could fail before the journal is sealed).
    pub finished_unix_s: u64,
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

/// D8/R1: the ingress thread is joined within what is left of the drain budget — never a second
/// full budget after the seal. A thread still alive at the drain deadline is reported at once
/// (`INGRESS_NOT_QUIESCENT`); one whose drain was skipped (write error) keeps the remaining budget.
pub(super) fn join_after_seal<T>(
    ingress: Option<std::thread::JoinHandle<T>>,
    drain_deadline: Instant,
) -> Result<(), String> {
    match ingress {
        Some(thread) => quiesce(
            thread,
            drain_deadline.saturating_duration_since(Instant::now()),
        ),
        None => Ok(()),
    }
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
        let now_s = clock_unix_s(&clock);
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
        outcome.finished_unix_s,
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
    // D08: the capture starts only with a wall clock (CLOCK_UNAVAILABLE, never time 0); the
    // refusal is the capture's terminal result in the Capture tab.
    let clock = match Clock::try_new() {
        Ok(clock) => clock,
        Err(code) => return refuse_start(code, status_tap.as_ref()),
    };
    // DEC-07: a standalone raw capture reads Mainnet only admitted to its approved boot flow
    // (mode market); every other start (autostart, terminal, an unapproved command) ends here,
    // before any read.
    let admitted =
        match crate::orchestration::boot_audit::market_admission(&|k| std::env::var(k).ok()) {
            Ok(admitted) => admitted,
            Err(code) => return refuse_start(code, status_tap.as_ref()),
        };
    let cfg = MarketConfig::from_lookup(&|k| std::env::var(k).ok(), clock_unix_s(&clock))?;
    before_start(&cfg.run_id, &cfg.run_dir)?;
    // DEC-16: an unbounded capture rolls its windows under its admission.
    run_published(cfg, clock, None, status_tap, admitted.rolling)
}

pub fn run(cfg: MarketConfig) -> Result<(), String> {
    run_embedded_with(cfg, Clock::try_new()?, None, None)
}

/// Now on the capture's clock: a validated wall origin plus monotonic time (D08, review 11) —
/// after the start no capture time can fail or be 0.
fn clock_unix_s(clock: &Clock) -> u64 {
    (clock.unix_ns(clock.now_ns()) / 1_000_000_000) as u64
}

/// A start refused before the capture exists: the code is the terminal result (ENDED).
fn refuse_start(code: &'static str, status_tap: Option<&StatusTap>) -> Result<(), String> {
    if let Some(tap) = status_tap {
        tap.publish_phase(|p| {
            p.phase = CapturePhase::Ended;
            p.end_result = code.into();
        });
        let _ = tap.flush(STATUS_FLUSH_TIMEOUT);
    }
    Err(code.into())
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
    run_published(cfg, clock, econ, status_tap, None)
}

pub(crate) fn run_published(
    cfg: MarketConfig,
    clock: Clock,
    econ: Option<crate::analytics::econ::Handle>,
    status_tap: Option<StatusTap>,
    rolling: Option<crate::orchestration::until_stopped::Rolling>,
) -> Result<(), String> {
    if let Some(tap) = &status_tap {
        tap.publish_operational(crate::status_tap::operational_snapshot());
        tap.publish_phase(|p| p.phase = CapturePhase::WaitingForStream);
    }
    // One terminal publication for every return path, including initialization and
    // evidence-write failures before the capture loop begins.
    // DEC-16: an unbounded econ4h run's windows check against its admission.
    let rolling = rolling.or_else(|| econ.as_ref().and_then(|h| h.cfg.rolling.clone()));
    let result = run_embedded_inner(cfg, clock, econ, status_tap.clone(), rolling);
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

/// DEC-16: one rolling window's boundary of an unbounded capture: its segment sealed, its
/// provider window ended, its gates re-checked against the newest qualifying evidence.
fn roll_window(
    windows: &mut crate::orchestration::until_stopped::RollingWindows,
    rolling: &crate::orchestration::until_stopped::Rolling,
    sink: &mut JournalSink,
    cfg: &MarketConfig,
    now_s: u64,
) -> Result<crate::orchestration::until_stopped::WindowCheck, String> {
    use crate::orchestration::until_stopped as us;
    let rotated = sink.rotate_window().map_err(|e| e.to_string())?;
    let provider = darkbot_solana::provider::shared().roll_window();
    let next_end = now_s.saturating_add(rolling.binding.checkpoint_seconds);
    let check = windows
        .close(
            &rolling.config,
            cfg.source.as_str(),
            us::Measured {
                now: now_s,
                bytes: rotated.bytes,
                run_volume: &cfg.run_dir,
                environment: crate::storage::preflight::environment(&cfg.run_dir),
                provider,
                evidence: us::newest_evidence(
                    &rolling.project,
                    &rolling.config,
                    &rolling.binding,
                    next_end,
                ),
                sealed: Some((&rotated.sealed, rotated.root_sha256)),
            },
        )
        .map_err(|e| e.to_string())?;
    crate::console::line(format!(
        "[UNTIL_STOPPED] window={} verdict={} {}",
        check.window, check.verdict, check.detail
    ));
    Ok(check)
}

/// One C0-E1 market checkpoint (boundary `index` of the measurement plan) to the analytics.
fn emit_checkpoint(
    h: &crate::analytics::econ::Handle,
    core: &ObserverCore,
    cfg: &MarketConfig,
    index: usize,
) {
    let seconds = h.cfg.plan.seconds(index);
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
        index,
        summary: Box::new(summary),
    });
}

/// DEC-16: the window boundaries of an unbounded econ4h run's capture.
struct EconWindows<'a> {
    h: &'a crate::analytics::econ::Handle,
    rolling: &'a crate::orchestration::until_stopped::Rolling,
    /// The window now open (1-based, the econ grid's).
    open: u64,
}

impl EconWindows<'_> {
    fn width(&self) -> u64 {
        self.h
            .shared
            .window_ns
            .load(std::sync::atomic::Ordering::Acquire)
    }
    /// The open window's end on the grid.
    fn end(&self) -> u64 {
        self.h
            .shared
            .start_ns
            .load(std::sync::atomic::Ordering::Acquire)
            + self.open * self.width()
    }

    /// DEC-16: closes every window whose end `t` has passed — its remaining checkpoints, the
    /// journals' rotation with the window's checks (`roll_window`), then `WindowClosed` to the
    /// analytics, in that order. `true` once a window's checks halted the run.
    #[allow(clippy::too_many_arguments)]
    fn close_passed(
        &mut self,
        t: u64,
        windows: &mut crate::orchestration::until_stopped::RollingWindows,
        sink: &mut JournalSink,
        cfg: &MarketConfig,
        core: &ObserverCore,
        checkpoint_index: &mut usize,
        clock: &Clock,
    ) -> Result<bool, String> {
        while t > self.end() {
            let end = self.end();
            while *checkpoint_index < crate::analytics::measurement::SNAPSHOT_COUNT {
                *checkpoint_index += 1;
                emit_checkpoint(self.h, core, cfg, *checkpoint_index);
            }
            let end_s = (clock.unix_ns(end) / 1_000_000_000) as u64;
            let check = roll_window(windows, self.rolling, sink, cfg, end_s)?;
            // Lossless within the writer stall budget: a lost close would stop the window grid.
            let within = Duration::from_millis(
                darkbot_solana::runtime_config::active()
                    .limits
                    .market_writer_stall_ms,
            );
            if !self.h.emit_control(
                crate::analytics::econ::Evidence::WindowClosed {
                    window: self.open,
                    boundary_ns: end,
                    verdict: check.verdict.clone(),
                },
                within,
            ) {
                return Err("UNTIL_STOPPED_ANALYTICS_BACKPRESSURE: the window's close did not reach the analytics within the writer stall budget".into());
            }
            *checkpoint_index = 0;
            self.open += 1;
            if check.halt_code().is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// DEC-16: the windows of a crashed unbounded run, continued: the interrupted window (from its
/// last recorded end to now) checked like any window before the capture reads anything; a HALT
/// there ends the run (its windows journal sealed, never resumed again).
fn resume_windows(
    rolling: &crate::orchestration::until_stopped::Rolling,
    cfg: &MarketConfig,
    now_s: u64,
    bytes: u64,
) -> Result<crate::orchestration::until_stopped::RollingWindows, String> {
    use crate::orchestration::until_stopped as us;
    let (windows, check) = us::RollingWindows::resume(
        &cfg.run_dir,
        rolling.binding.clone(),
        &rolling.qualification_id,
        &rolling.config,
        cfg.source.as_str(),
        us::Measured {
            now: now_s,
            bytes,
            run_volume: &cfg.run_dir,
            environment: crate::storage::preflight::environment(&cfg.run_dir),
            // What this process has used so far (nothing read yet); the crashed one's use ended
            // with it.
            provider: darkbot_solana::provider::shared().roll_window(),
            evidence: us::newest_evidence(
                &rolling.project,
                &rolling.config,
                &rolling.binding,
                now_s.saturating_add(rolling.binding.checkpoint_seconds),
            ),
            sealed: None,
        },
    )
    .map_err(|e| format!("UNTIL_STOPPED_RESUME: {e}"))?;
    crate::console::line(format!(
        "[UNTIL_STOPPED] window={} verdict={} {}",
        check.window, check.verdict, check.detail
    ));
    if let Some(verdict) = windows.halted().map(str::to_string) {
        rolling.suspend_autostart(&verdict, now_s);
        if let Err(e) = windows.finish(false) {
            crate::console::line(format!("UNTIL_STOPPED_WINDOWS_INCOMPLETE {e}"));
        }
        return Err("MARKET_CAPTURE_INCOMPLETE: UNTIL_STOPPED_HALT".into());
    }
    Ok(windows)
}

fn run_embedded_inner(
    cfg: MarketConfig,
    clock: Clock,
    econ: Option<crate::analytics::econ::Handle>,
    status_tap: Option<StatusTap>,
    rolling: Option<crate::orchestration::until_stopped::Rolling>,
) -> Result<(), String> {
    let process_started = clock_unix_s(&clock);
    fs::create_dir_all(&cfg.run_dir)
        .map_err(|e| format!("create {}: {e}", cfg.run_dir.display()))?;
    // DEC-16: an approved start continues the crashed unbounded run it names — its journals by
    // their SK-07 recovery, the window the crash interrupted checked again before any read.
    let (mut sink, mut windows) = match rolling
        .as_ref()
        .and_then(|r| r.resume.as_ref().map(|id| (r, id)))
    {
        None => (JournalSink::create(&cfg.run_dir)?, None),
        Some((r, id)) => {
            if id != &cfg.run_id {
                return Err(format!(
                    "UNTIL_STOPPED_RESUME_DRIFT: the start continues {id}, the capture is {}",
                    cfg.run_id
                ));
            }
            let (mut sink, how, bytes) = JournalSink::resume(&cfg.run_dir)?;
            crate::console::line(format!(
                "[UNTIL_STOPPED] continues run {id}: market journal {how:?}"
            ));
            match resume_windows(r, &cfg, process_started, bytes) {
                Ok(w) => (sink, Some(w)),
                Err(code) => {
                    let _ = sink.finish(false);
                    return Err(code);
                }
            }
        }
    };
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
            observed_unix_s: clock_unix_s(&clock),
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
            observed_unix_s: clock_unix_s(&clock),
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
        healthy_reset_ms: darkbot_solana::ingress::ws_rpc::healthy_reset().as_millis() as u64,
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
    core.retention = cfg.retention;
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
    // DEC-16: an unbounded capture (`until-stopped`, duration 0) has no cutoff until its stop.
    let duration_ns =
        (cfg.duration_secs > 0).then(|| cfg.duration_secs.saturating_mul(1_000_000_000));
    let mut ready_unix_s: Option<u64> = None;
    let mut window: Option<(u64, Option<u64>)> = None;
    let mut last_overflow = 0u64;
    let mut last_flush = Instant::now();
    let mut stop_reason = "TIMER";
    let mut write_error: Option<String> = None;
    let mut econ_windows: Option<EconWindows> = None;

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
        // DEC-16: an unbounded econ4h run ends at its stop, the cutoff its supervisor sets.
        if econ
            .as_ref()
            .is_some_and(|h| h.cfg.unbounded && h.cutoff().is_some())
        {
            stop_reason = "OPERATOR_STOP";
            break;
        }
        if let (Some(h), Some((start, Some(cutoff)))) = (&econ, window) {
            let due = h
                .cfg
                .plan
                .due(clock.now_ns().min(cutoff).saturating_sub(start));
            while checkpoint_index < due {
                checkpoint_index += 1;
                emit_checkpoint(h, &core, &cfg, checkpoint_index);
            }
        }
        // DEC-16: the open window's checkpoints of an unbounded econ4h run; its boundary closes
        // it once the stream passed it (ingress below) or nothing is queued after it.
        if let (Some(e), Some(w)) = (econ_windows.as_mut(), windows.as_mut()) {
            let now = clock.now_ns();
            let from = e.end() - e.width();
            let due = e.h.cfg.plan.due(now.min(e.end()).saturating_sub(from));
            while checkpoint_index < due {
                checkpoint_index += 1;
                emit_checkpoint(e.h, &core, &cfg, checkpoint_index);
            }
            if now > e.end() && !consumer.pending() {
                match e.close_passed(
                    now,
                    w,
                    &mut sink,
                    &cfg,
                    &core,
                    &mut checkpoint_index,
                    &clock,
                ) {
                    Ok(false) => {}
                    Ok(true) => {
                        stop_reason = "UNTIL_STOPPED_HALT";
                        break;
                    }
                    Err(err) => {
                        write_error = Some(err);
                        stop_reason = "WRITE_ERROR";
                        break;
                    }
                }
            }
        }
        if window.is_none() && started.elapsed() >= ready_timeout {
            stop_reason = "STREAM_NOT_READY";
            break;
        }
        if window.is_some_and(|(_, cutoff)| cutoff.is_some_and(|c| clock.now_ns() >= c)) {
            break;
        }
        // DEC-16: an unbounded capture's window boundary; its first HALT stops the capture. An
        // autostarted one ends in order at its standing approval's maximum runtime.
        if let (Some(w), Some(r), None) = (windows.as_mut(), &rolling, &econ) {
            let now_s = clock_unix_s(&clock);
            // The boundary first: a window that ended is checked before the run may end.
            if w.due(now_s) {
                match roll_window(w, r, &mut sink, &cfg, now_s) {
                    Ok(check) => {
                        if let Some(reason) = r.after_window(&check) {
                            stop_reason = reason;
                            break;
                        }
                    }
                    Err(e) => {
                        write_error = Some(e);
                        stop_reason = "WRITE_ERROR";
                        break;
                    }
                }
            }
            if r.max_runtime_reached(w, now_s) {
                stop_reason = crate::orchestration::autostart::MAX_RUNTIME;
                break;
            }
        }
        if ingress.as_ref().is_some_and(|t| t.is_finished()) {
            if let Err(e) = core.coverage(
                &mut sink,
                clock_unix_s(&clock),
                Some(clock.now_ns()),
                WindowEvent::IngressThreadEnded,
            ) {
                write_error = Some(e.to_string());
            }
            stop_reason = "INGRESS_THREAD_ENDED";
            break;
        }
        let wait = window
            .and_then(|(_, cutoff)| cutoff)
            .map(|cutoff| Duration::from_nanos(cutoff.saturating_sub(clock.now_ns())))
            .unwrap_or(Duration::from_secs(1))
            .min(Duration::from_millis(250));
        let now_s = clock_unix_s(&clock);
        let step =
            match consumer.pop_timeout(wait) {
                Some(IngressEvent::Reconnected {
                    attempt, t_rx_ns, ..
                }) if window.is_none() => {
                    let cutoff = duration_ns.map(|d| t_rx_ns.saturating_add(d));
                    window = Some((t_rx_ns, cutoff));
                    ready_unix_s = Some((clock.unix_ns(t_rx_ns) / 1_000_000_000) as u64);
                    core.start_observation(t_rx_ns, cutoff);
                    if let (Some(r), None) = (&rolling, &windows) {
                        windows = Some(
                            crate::orchestration::until_stopped::RollingWindows::start(
                                &cfg.run_dir,
                                r.binding.clone(),
                                &r.qualification_id,
                                clock_unix_s(&clock),
                                0,
                            )
                            .map_err(|e| format!("UNTIL_STOPPED_WINDOWS: {e}"))?,
                        );
                    }
                    if let Some(tap) = &status_tap {
                        tap.publish_phase(|p| {
                            p.phase = CapturePhase::Capture;
                            p.stream_ready_unix_s = ready_unix_s;
                        });
                    }
                    if let Some(h) = &econ {
                        match (cutoff, &rolling) {
                            (Some(cutoff), _) => {
                                let evidence = h.cfg.plan.capture_window(clock, t_rx_ns)?;
                                crate::analytics::measurement::journal(
                                    &h.cfg.paths(),
                                    clock,
                                    crate::analytics::measurement::Phase::Capture(evidence),
                                )?;
                                h.start(t_rx_ns, cutoff);
                            }
                            // DEC-16: an admitted unbounded run measures its windows from here.
                            (None, Some(r)) if h.cfg.unbounded => {
                                h.start_rolling(t_rx_ns);
                                econ_windows = Some(EconWindows {
                                    h,
                                    rolling: r,
                                    open: 1,
                                });
                            }
                            (None, _) => return Err(
                                "C0_E1_UNBOUNDED: an unbounded economics run needs its admission"
                                    .into(),
                            ),
                        }
                    }
                    core.coverage(
                        &mut sink,
                        now_s,
                        Some(t_rx_ns),
                        match cutoff {
                            Some(cutoff) => WindowEvent::StreamReady {
                                attempt,
                                observation_start_t_rx_ns: t_rx_ns,
                                observation_cutoff_t_rx_ns: cutoff,
                            },
                            None => WindowEvent::StreamReadyUntilStopped {
                                attempt,
                                observation_start_t_rx_ns: t_rx_ns,
                            },
                        },
                    )
                }
                Some(ev) => {
                    // DEC-16: a window closes before the first event past its boundary.
                    if let (Some(e), Some(w)) = (econ_windows.as_mut(), windows.as_mut()) {
                        match e.close_passed(
                            ev.t_rx_ns(),
                            w,
                            &mut sink,
                            &cfg,
                            &core,
                            &mut checkpoint_index,
                            &clock,
                        ) {
                            Ok(false) => {}
                            Ok(true) => {
                                stop_reason = "UNTIL_STOPPED_HALT";
                                break;
                            }
                            Err(err) => {
                                write_error = Some(err);
                                stop_reason = "WRITE_ERROR";
                                break;
                            }
                        }
                    }
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
                if let Some(assessment) = core.assessment_snapshot() {
                    tap.publish_assessment(assessment);
                }
                // F-01b: the provider budget every RPC of this process runs through.
                tap.publish_provider(&darkbot_solana::provider::shared());
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
    // Freeze an open-ended window at its stop before draining queued ingress.
    let window: Option<(u64, u64)> = window.map(|(start, cutoff)| match cutoff {
        Some(cutoff) => (start, cutoff),
        None => {
            core.start_observation(start, Some(capture_end_ns));
            (start, capture_end_ns)
        }
    });
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
    // DEC-16: an unbounded econ4h run's stop is its ordered end, never an abort.
    let rolling_stop =
        econ.as_ref().is_some_and(|h| h.cfg.unbounded) && stop_reason == "OPERATOR_STOP";
    if stop_reason != "TIMER" {
        core.mark_incomplete("TRANSPORT_CAPTURE_ABORT");
        if let (Some(h), false) = (&econ, rolling_stop) {
            h.shared
                .market_failed
                .store(true, std::sync::atomic::Ordering::Release);
            h.shared.request_abort();
        }
    }
    let observation_end_s = window
        .map(|(_, cutoff)| (clock.unix_ns(capture_end_ns.min(cutoff)) / 1_000_000_000) as u64)
        // Without a window: now on the capture's validated clock (D08, never 0).
        .unwrap_or_else(|| (clock.unix_ns(clock.now_ns()) / 1_000_000_000) as u64);

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
        clock_unix_s(&clock),
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
        if let Err(e) = core.coverage(&mut sink, clock_unix_s(&clock), None, event) {
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
        finished_unix_s: clock_unix_s(&clock),
    };
    let (summary, report, write_error) = finish_capture(&cfg, &mut core, &mut sink, outcome)?;
    // D8: the session was asked to stop; its thread is joined within the drain budget. One still
    // alive afterwards is reported, never declared quiescent (the journal is INCOMPLETE then,
    // since its drain did not complete).
    let quiescent = join_after_seal(ingress.take(), drain_deadline);
    // DEC-16: the windows journal ends with the capture — complete only for a run that ended in
    // order (drained, quiescent, nothing failed) and never halted; an autostarted run's HALT
    // suspends its standing approval.
    if let Some(w) = windows.take() {
        let in_order = crate::orchestration::until_stopped::ended_in_order(
            stop_reason,
            drain_complete,
            write_error.as_deref(),
            ingress_end_error,
            &quiescent,
        );
        if let (Some(verdict), Some(r)) = (w.halted().map(str::to_string), &rolling) {
            r.suspend_autostart(&verdict, clock_unix_s(&clock));
        }
        if let Err(e) = w.finish(in_order) {
            crate::console::line(format!("UNTIL_STOPPED_WINDOWS_INCOMPLETE {e}"));
        }
    }
    let end = capture_end(
        &cfg,
        &quiescent,
        stop_reason,
        window.is_some(),
        write_error.as_deref(),
        ingress_end_error,
        drain_complete,
    );
    if let Some(tap) = &status_tap {
        tap.publish_market(core.market_snapshot());
        if let Some(assessment) = core.assessment_snapshot() {
            tap.publish_assessment(assessment);
        }
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
            crate::status_tap::unix_ms_now().unwrap()
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

    /// D08: a capture refused for an unavailable wall clock ends with exactly that code in the
    /// Capture projection (phase ENDED), and starts nothing.
    #[test]
    fn d08_clock_refusal_is_the_terminal_capture_result() {
        let tap = StatusTap::new();
        let code = darkbot_common::timing::CLOCK_UNAVAILABLE;
        assert_eq!(refuse_start(code, Some(&tap)), Err(code.to_string()));
        let state = tap.reader().wait_changed(Duration::ZERO).unwrap();
        let phase = state.phase.unwrap();
        assert_eq!(
            (phase.phase, phase.end_result.as_str()),
            (CapturePhase::Ended, code)
        );
        assert!(state.run.is_none() && state.market.is_none());
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
                Some("END"),
                false
            ),
            Err("INGRESS_TIMEOUT".into())
        );
        assert_eq!(
            capture_end(&cfg, &Ok(()), "OPERATOR_STOP", false, None, None, false),
            Err("MARKET_CAPTURE_INCOMPLETE: OPERATOR_STOP".into())
        );
        assert!(
            capture_end(&cfg, &Ok(()), "TIMER", false, Some("WRITE"), None, false)
                .unwrap_err()
                .starts_with("MARKET_OBSERVER_STREAM_NOT_READY:")
        );
        assert_eq!(
            capture_end(&cfg, &Ok(()), "TIMER", true, Some("WRITE"), None, false),
            Err("market evidence write failed: WRITE".into())
        );
        assert_eq!(
            capture_end(&cfg, &Ok(()), "TIMER", true, None, Some("END"), false),
            Err("MARKET_CAPTURE_INCOMPLETE: END".into())
        );
        assert_eq!(
            capture_end(&cfg, &Ok(()), "TIMER", true, None, None, false),
            Err("MARKET_CAPTURE_INCOMPLETE: DRAIN_INCOMPLETE".into())
        );
        assert_eq!(
            capture_end(&cfg, &Ok(()), "TIMER", true, None, None, true),
            Ok(())
        );
    }
}

//! CLI resolution and mode lifecycle; market dispatch precedes trading startup.
use super::state_owner::run_shadow;
use darkbot_common::{config, timing::Clock};
use std::time::Duration;

/// Operational-configuration rejection from `runtime_config`: fixed local texts that never
/// contain endpoint values or keys, so the start report may show them (N5).
#[derive(Debug)]
pub(crate) struct OperationalConfigError(pub String);
impl std::fmt::Display for OperationalConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for OperationalConfigError {}

/// N5: the one-line start failure report. Only classified errors carry detail: configuration
/// keys (never their values), operational-config rejections, owner/evidence codes; transport,
/// RPC and IO errors keep their category; every other error is the generic category, its
/// `Display` is never printed (it may carry URLs, keys, paths or bodies).
pub fn failure_report(error: &(dyn std::error::Error + 'static)) -> String {
    use darkbot_common::config::ConfigError;
    if let Some(e) = error.downcast_ref::<ConfigError>() {
        return match e {
            ConfigError::Missing(key) => format!("CONFIG_MISSING {key}"),
            ConfigError::Invalid { key, .. } => format!("CONFIG_INVALID {key}"),
        };
    }
    if let Some(e) = error.downcast_ref::<OperationalConfigError>() {
        return format!("OPERATIONAL_CONFIG: {e}");
    }
    shadow_failure_code(error).to_string()
}

/// The operational configuration as process environment (incl. `RAYON_NUM_THREADS` =
/// `limits.workers`). D13: applied on the single startup thread before anything uses Rayon.
pub(crate) fn apply_operational_environment(
    config: &darkbot_solana::runtime_config::RuntimeConfig,
) {
    for (k, v) in config.environment() {
        std::env::set_var(k, v);
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|s| s == "--help" || s == "-h") {
        print!("{}", darkbot_solana::runtime_config::Resolved::help());
        return Ok(());
    }
    if !args.is_empty()
        || std::env::var_os("P0_PROFILE").is_some()
        || std::env::var_os("P0_OPERATIONAL_CONFIG_FILE").is_some()
    {
        let resolved =
            darkbot_solana::runtime_config::Resolved::parse(&args, &|k| std::env::var(k).ok())
                .map_err(OperationalConfigError)?;
        if resolved.print {
            print!("{}", resolved.text());
            return Ok(());
        }
        if resolved.config.verification != "off"
            && std::env::var_os("P0_MEASUREMENT_PLAN").is_none()
        {
            return Err("verification requires capture.sh/verify.sh post-capture lifecycle".into());
        }
        // Single-threaded startup only. Existing constructors keep their typed
        // gates while obtaining operational values from the shared contract.
        apply_operational_environment(&resolved.config);
        resolved.activate()?;
        darkbot_solana::provider::enable_measurement_budget();
    }
    // FEUI-01: one runtime owner per project (launchers set P0_RUNTIME_LOCK) and an ordered
    // operator stop for the observation modes that poll it (econ4h, market). Shadow keeps the
    // default signal behaviour because it has no stop poll.
    let mode = config::require("P0_MODE")?;
    let _owner = match std::env::var_os("P0_RUNTIME_LOCK") {
        Some(path) => {
            // S1-02: `p0ctl source set` signals any lock holder; the doorbell exists first.
            super::source_control::install_signal()?;
            let run_id = ["P0_ECON_RUN_ID", "P0_MARKET_RUN_ID"]
                .iter()
                .find_map(|k| std::env::var(k).ok())
                .unwrap_or_else(|| "-".into());
            Some(super::control::acquire_owner(
                std::path::Path::new(&path),
                &run_id,
                &mode,
            )?)
        }
        None => None,
    };
    if mode == "market" || mode == "econ4h" {
        super::control::install_stop_handler()?;
    }
    // [C0-MO] Market mode branches off before any trading-stack initialisation: no RPC URL,
    // keypair, signer, health engine, planner, simulation or submission is constructed.
    if mode == "market" {
        crate::run_layout::RunLayout::standalone_market(&|k| std::env::var(k).ok())?;
        return crate::market_observer::run_from_env().map_err(|e| e.into());
    }
    // X1: a start by `p0ctl run start` (or with a record) is bound to its approved network
    // audit before any owner thread starts or anything is read from the network; a missing,
    // expired, foreign or drifted record refuses the start (econ4h: after its local manifest, so
    // the refused run stays diagnosable). The owner binds the audited accounts to its own
    // bootstrap state at the first Ready, before admission.
    let admission = super::boot_audit::admission_record(
        &|k| std::env::var(k).ok(),
        std::env::var_os("P0_CTL_LAUNCH_RUN_ID").is_some(),
        &|| {
            let project = std::env::var("P0_PROJECT_DIR")
                .map_err(|_| "BOOT_NETWORK_AUDIT_MISSING: P0_PROJECT_DIR".to_string())?;
            super::boot_audit::launched_identity(
                std::path::Path::new(&project),
                super::boot_audit::SafetyFlags::from_env(),
                &args,
                &|k| std::env::var(k).ok(),
            )
        },
        &super::boot_audit::runtime_binary_identity,
    )
    .map_err(|e| {
        eprintln!("BOOT admission refused: {e}");
        super::boot_audit::admission_code(&e)
    });
    let refused = |code| Box::new(super::state_owner::OwnerTerminal(code));
    if mode == "econ4h" {
        let code = admission.as_ref().err().copied();
        return run_econ4h(admission).map_err(|e| match code {
            Some(code) => refused(code) as Box<dyn std::error::Error>,
            None => e.into(),
        });
    }
    run_shadow(Clock::new(), None, admission.map_err(refused)?)
}

/// The Shadow owner of an econ run has ended. An owner error (e.g. an `OwnerTerminal` such as
/// STATE_RECOVERY_EXHAUSTED) or an end before admission closed marks the run failed and aborts
/// it, so the market capture seals INCOMPLETE (C1/C7, D1 case "abort").
pub(crate) fn record_shadow_end(
    shared: &crate::analytics::econ::Shared,
    failed: bool,
    admission_closed: bool,
) {
    use std::sync::atomic::Ordering;
    if failed || !admission_closed {
        shared.shadow_failed.store(true, Ordering::Release);
        shared.request_abort();
    }
    shared.shadow_done.store(true, Ordering::Release);
}

// Keep the first Shadow failure diagnosable without exposing provider URLs,
// response payloads or wallet paths from library Display implementations.
pub fn shadow_failure_code(error: &(dyn std::error::Error + 'static)) -> &'static str {
    use solana_client::client_error::{ClientError, ClientErrorKind};
    if let Some(e) = error.downcast_ref::<crate::analytics::evidence::EvidenceFailure>() {
        return e.0;
    }
    if let Some(e) = error.downcast_ref::<super::state_owner::OwnerTerminal>() {
        return e.0;
    }
    if let Some(e) = error.downcast_ref::<ClientError>() {
        return match e.kind() {
            ClientErrorKind::Reqwest(e) if e.is_timeout() => "RPC_TIMEOUT",
            ClientErrorKind::Reqwest(e) if e.is_connect() => "RPC_CONNECT",
            ClientErrorKind::Reqwest(e) if e.status().is_some_and(|s| s.as_u16() == 429) => {
                "RPC_RATE_LIMIT"
            }
            ClientErrorKind::Reqwest(_) => "RPC_HTTP_OR_TRANSPORT",
            ClientErrorKind::Io(_) => "RPC_IO",
            ClientErrorKind::RpcError(_) => "RPC_RESPONSE_ERROR",
            ClientErrorKind::SerdeJson(_) => "RPC_DECODE",
            _ => "RPC_OTHER",
        };
    }
    if error.is::<std::io::Error>() {
        "LOCAL_IO"
    } else {
        "SHADOW_RUNTIME_ERROR"
    }
}

fn run_econ4h(
    admission: Result<Option<super::boot_audit::NetworkAudit>, &'static str>,
) -> Result<(), String> {
    darkbot_solana::provider::enable_measurement_budget();
    use crate::analytics::econ;
    use crate::market::observer as market_observer;
    let clock = Clock::new();
    let now_s = (clock.unix_ns(clock.now_ns()) / 1_000_000_000) as u64;
    let cfg = econ::EconConfig::from_lookup(&|k| std::env::var(k).ok(), now_s)?;
    let mut market = market_observer::MarketConfig::from_lookup(&|k| std::env::var(k).ok(), now_s)?;
    if let Ok(url) = std::env::var("P0_MARKET_WS_URL") {
        market.ws_url = url;
    }
    let owner = admission.map(|admission| {
        move |handle: econ::Handle| {
            run_shadow(clock, Some(handle), admission).map_err(|e| {
                let report = failure_report(e.as_ref());
                eprintln!("C0_E1_SHADOW_FAILED category={report}");
                report
            })
        }
    });
    run_econ4h_with(cfg, market, clock, owner)
}

/// The C0-E1 run after configuration: the market capture bound to the measurement plan, the
/// analytics worker, both owners under supervision (or the refusal before them) and the sealed
/// phase journal. `owner` is the Shadow owner (`run_shadow` in production).
pub(crate) fn run_econ4h_with<S>(
    cfg: crate::analytics::econ::EconConfig,
    mut market: crate::market::observer::MarketConfig,
    clock: Clock,
    owner: Result<S, &'static str>,
) -> Result<(), String>
where
    S: FnOnce(crate::analytics::econ::Handle) -> Result<(), String> + Send + 'static,
{
    use crate::analytics::econ;
    market.duration_secs = cfg.plan.duration_seconds;
    market.ready_timeout_secs = cfg.plan.budgets.stream_ready_seconds;
    market.drain_timeout_secs = cfg.plan.budgets.market_drain_seconds;
    market.enrich.phase_max_secs = cfg.plan.budgets.enrichment_seconds;
    market.reconcile.phase_max_secs = cfg.plan.budgets.strict_seconds;
    market.run_id = cfg.run_id.clone();
    market.run_dir = cfg.paths().market_dir();
    let timeout = cfg.plan.owner_budget_seconds();
    let (handle, analytics) = econ::spawn(cfg, clock, &market)?;
    let paths = handle.cfg.paths();
    // Every exit after BOOTSTRAP seals the phase journal and renders its JSONL view.
    let result = match owner {
        Ok(owner) => supervise(handle, analytics, market, clock, timeout, owner),
        Err(code) => refuse_before_owners(handle, analytics, clock, code),
    };
    let rendered = crate::analytics::measurement::finalize_phases(&paths, result.is_ok());
    if let Err(e) = &rendered {
        eprintln!("C0_E1_PHASE_JOURNAL_INCOMPLETE {e}");
    }
    result.and(rendered)
}

/// X1: a refused admission ends the econ run before either owner starts (no owner thread, no
/// network contact): both owners count as failed, the run is aborted and the analytics worker
/// ends in order, so the local run evidence records the refusal.
fn refuse_before_owners(
    handle: crate::analytics::econ::Handle,
    analytics: std::thread::JoinHandle<Result<(), String>>,
    clock: Clock,
    code: &'static str,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    eprintln!("C0_E1_ADMISSION_REFUSED {code}: no owner started; live=false submission=false");
    let shared = &handle.shared;
    for flag in [
        &shared.abort,
        &shared.shadow_failed,
        &shared.market_failed,
        &shared.shadow_done,
        &shared.market_done,
        &shared.finish,
    ] {
        flag.store(true, Ordering::Release);
    }
    crate::analytics::measurement::journal(
        &handle.cfg.paths(),
        clock,
        crate::analytics::measurement::Phase::OwnersFinished {
            aborted: true,
            shadow_failed: true,
            market_failed: true,
        },
    )?;
    analytics
        .join()
        .map_err(|_| "C0-E1 analytics panic".to_string())??;
    Err(code.into())
}

fn supervise<S>(
    handle: crate::analytics::econ::Handle,
    analytics: std::thread::JoinHandle<Result<(), String>>,
    market: crate::market::observer::MarketConfig,
    clock: Clock,
    timeout: u64,
    owner: S,
) -> Result<(), String>
where
    S: FnOnce(crate::analytics::econ::Handle) -> Result<(), String> + Send + 'static,
{
    use crate::market::observer as market_observer;
    use std::sync::atomic::Ordering;
    let shadow_handle = handle.clone();
    let shadow = std::thread::Builder::new()
        .name("econ-shadow-owner".into())
        .spawn(move || {
            let result = owner(shadow_handle.clone());
            record_shadow_end(
                &shadow_handle.shared,
                result.is_err(),
                shadow_handle.admission_closed(),
            );
            result
        })
        .map_err(|e| e.to_string())?;
    let market_handle = handle.clone();
    let observer = std::thread::Builder::new()
        .name("econ-market".into())
        .spawn(move || {
            let result = market_observer::run_embedded(market, clock, Some(market_handle.clone()));
            if result.is_err() {
                market_handle
                    .shared
                    .market_failed
                    .store(true, Ordering::Release);
                market_handle.shared.request_abort();
            }
            market_handle
                .shared
                .market_done
                .store(true, Ordering::Release);
            result
        })
        .map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout);
    // Report state transitions instead of repeating a finished Shadow state
    // throughout the (potentially long) finalized block reconciliation.
    let mut last_status = None;
    while !shadow.is_finished() || !observer.is_finished() {
        let status = (
            handle.shared.shadow_ready.load(Ordering::Acquire),
            handle.shared.shadow_done.load(Ordering::Acquire),
            handle.shared.shadow_failed.load(Ordering::Acquire),
            handle.shared.market_done.load(Ordering::Acquire),
            handle.shared.market_failed.load(Ordering::Acquire),
        );
        if last_status != Some(status) {
            if status.1 && !status.3 {
                eprintln!(
                    "[MARKET/STATUS] phase=POST_CAPTURE_FINALIZATION shadow_done={} shadow_failed={} market_done={} market_failed={} live=false submission=false",
                    status.1, status.2, status.3, status.4
                );
            } else {
                eprintln!(
                    "[SHADOW/STATUS] ready={} done={} failed={} market_done={} market_failed={} live=false submission=false",
                    status.0, status.1, status.2, status.3, status.4
                );
            }
            last_status = Some(status);
        }
        if super::control::stop_requested() && !handle.shared.operator_stop.load(Ordering::Acquire)
        {
            handle.shared.operator_stop.store(true, Ordering::Release);
            handle.shared.abort.store(true, Ordering::Release);
            eprintln!("[C0_E1/STOP] operator stop requested: admission closed, capture drains, journals sealed, coverage INCOMPLETE (OPERATOR_STOP); live=false submission=false");
        }
        if analytics.is_finished() || std::time::Instant::now() >= deadline {
            handle.shared.abort.store(true, Ordering::Release);
            handle.shared.shadow_failed.store(true, Ordering::Release);
            handle.shared.market_failed.store(true, Ordering::Release);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // A5: after an abort (analytics ended, deadline) the owners stop on the abort flag; the
    // process ends only after they did (the market capture seals INCOMPLETE), bounded by the
    // shutdown grace. One still running then is reported, never awaited without bound.
    let grace = Duration::from_secs(handle.cfg.plan.budgets.shutdown_grace_seconds);
    let quiescent = await_owners(&shadow, &observer, grace);
    if !quiescent {
        handle.shared.shadow_failed.store(true, Ordering::Release);
        handle.shared.market_failed.store(true, Ordering::Release);
        eprintln!(
            "C0_E1_OWNERS_NOT_QUIESCENT shadow_finished={} market_finished={} grace_s={}; live=false submission=false",
            shadow.is_finished(),
            observer.is_finished(),
            grace.as_secs()
        );
    }
    if shadow.is_finished() && !matches!(shadow.join(), Ok(Ok(()))) {
        handle.shared.shadow_failed.store(true, Ordering::Release);
    }
    if observer.is_finished() && !matches!(observer.join(), Ok(Ok(()))) {
        handle.shared.market_failed.store(true, Ordering::Release);
    }
    handle.shared.finish.store(true, Ordering::Release);
    crate::analytics::measurement::journal(
        &handle.cfg.paths(),
        clock,
        crate::analytics::measurement::Phase::OwnersFinished {
            aborted: handle.shared.abort.load(Ordering::Acquire),
            shadow_failed: handle.shared.shadow_failed.load(Ordering::Acquire),
            market_failed: handle.shared.market_failed.load(Ordering::Acquire),
        },
    )?;
    let result = analytics
        .join()
        .map_err(|_| "C0-E1 analytics panic".to_string())?;
    if !quiescent {
        return result.and(Err("C0_E1_OWNERS_NOT_QUIESCENT".into()));
    }
    result
}

/// A5: waits until both owner threads have ended, at most `grace`; `false` if one still runs.
pub(crate) fn await_owners<A, B>(
    shadow: &std::thread::JoinHandle<A>,
    observer: &std::thread::JoinHandle<B>,
    grace: Duration,
) -> bool {
    let end = std::time::Instant::now() + grace;
    while !(shadow.is_finished() && observer.is_finished()) {
        let left = end.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50).min(left));
    }
    true
}

#[cfg(test)]
mod supervise_tests {
    use super::*;
    use crate::market_observer::rows::MarketCoverage;
    use crate::market_observer::{JournalSink, MarketSink, MARKET_JOURNAL};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// A5: after the supervisor's abort (analytics ended) the process waits for the owners: the
    /// market owner drains and seals its journal INCOMPLETE before the wait returns (without the
    /// wait the journal is still unsealed at that point); a stuck owner is reported within the
    /// grace instead of being awaited without bound.
    #[test]
    fn analytics_abort_waits_for_the_owners_until_the_market_journal_is_sealed() {
        let dir = std::env::temp_dir().join(format!("p0_a5_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let abort = Arc::new(AtomicBool::new(false));
        let (market_abort, shadow_abort) = (abort.clone(), abort.clone());
        let market_dir = dir.clone();
        let observer = std::thread::spawn(move || {
            let mut sink = JournalSink::create(&market_dir).unwrap();
            sink.coverage(&MarketCoverage::ShadowWarmupWait { observed_unix_s: 0 })
                .unwrap();
            while !market_abort.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
            // Drain, then seal INCOMPLETE (the capture's abort path).
            std::thread::sleep(Duration::from_millis(300));
            sink.finish(false).unwrap();
        });
        let shadow = std::thread::spawn(move || {
            while !shadow_abort.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        std::thread::sleep(Duration::from_millis(50));
        abort.store(true, Ordering::Release);
        let journal = dir.join(MARKET_JOURNAL);
        assert!(
            crate::storage::segments::verify_chain(&journal).map_or(true, |c| !c.sealed),
            "right after the abort the journal is not sealed yet"
        );
        let started = std::time::Instant::now();
        assert!(await_owners(&shadow, &observer, Duration::from_secs(10)));
        assert!(started.elapsed() >= Duration::from_millis(200));
        let chain = crate::storage::segments::verify_chain(&journal).unwrap();
        assert_eq!((chain.sealed, chain.complete), (true, false));

        let done = std::thread::spawn(|| {});
        let stuck = std::thread::spawn(|| std::thread::sleep(Duration::from_secs(1)));
        let started = std::time::Instant::now();
        assert!(!await_owners(&done, &stuck, Duration::from_millis(200)));
        assert!(started.elapsed() < Duration::from_secs(1));
        stuck.join().unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A loopback WebSocket peer (plain `ws://`, never Mainnet): acknowledges every subscription
    /// and delivers slot progress every 50 ms; with `drop_after` it drops its first session after
    /// that many slots (a transport gap) and serves the reconnect until the client closes.
    fn loopback_peer(drop_after: Option<u64>) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        use tungstenite::Message;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let accepted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = accepted.clone();
        let sessions = if drop_after.is_some() { 2 } else { 1 };
        std::thread::spawn(move || {
            let mut slot = 1_000u64;
            for session in 0..sessions {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                count.fetch_add(1, Ordering::Release);
                stream
                    .set_read_timeout(Some(Duration::from_millis(50)))
                    .unwrap();
                let mut ws = tungstenite::accept(stream).unwrap();
                let (mut acked, mut slot_subscription, mut sent) = (0, None, 0u64);
                loop {
                    match ws.read() {
                        Ok(Message::Text(text)) => {
                            let request: serde_json::Value = serde_json::from_str(&text).unwrap();
                            let id = request["id"].as_u64().unwrap();
                            if request["method"] == "slotSubscribe" {
                                slot_subscription = Some(id);
                            }
                            acked += 1;
                            let ack = serde_json::json!({"jsonrpc": "2.0", "id": id, "result": id});
                            if ws.send(Message::Text(ack.to_string())).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(tungstenite::Error::Io(e))
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) => {}
                        Err(_) => break,
                    }
                    if let (2, Some(subscription)) = (acked, slot_subscription) {
                        if session == 0 && drop_after.is_some_and(|n| sent >= n) {
                            break; // dropped without a close frame: a transport gap
                        }
                        slot += 1;
                        sent += 1;
                        let notification = serde_json::json!({"jsonrpc": "2.0",
                            "method": "slotNotification", "params": {"subscription": subscription,
                            "result": {"slot": slot, "parent": slot - 1, "root": slot - 2}}});
                        if ws.send(Message::Text(notification.to_string())).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        (url, accepted)
    }

    /// The production C0-E1 run (`run_econ4h_with`: plan-bound market capture, analytics worker,
    /// supervision, phase journal) over the loopback peer. The Shadow owner needs an RPC boot, so
    /// a stand-in publishes its readiness exactly as `ShadowOwner` does (`lifecycle.rs`) and ends
    /// at the drain expiry like its loop; it evaluates nothing.
    fn unit_e_run(gap: bool) -> (std::path::PathBuf, Result<(), String>, usize) {
        let (url, accepted) = loopback_peer(gap.then_some(20));
        let out = std::env::temp_dir().join(format!("p0_unit_e_{}_{gap}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let env: std::collections::HashMap<&str, String> = [
            ("P0_ECON_RUN_ID", "unit_e".to_string()),
            ("P0_ECON_OUT_DIR", out.to_string_lossy().to_string()),
            ("P0_RUN_LAYOUT", "v2".into()),
            ("P0_ECON_DURATION_SECS", "4".into()),
            ("P0_PROVIDER_CLASS", "test".into()),
            ("P0_MARKET_READY_TIMEOUT_SECS", "10".into()),
            ("P0_ECON_DRAIN_SECS", "1".into()),
            ("P0_MARKET_DRAIN_TIMEOUT_SECS", "5".into()),
            ("P0_WS_URL", url),
            (
                "P0_PROGRAM_ID",
                darkbot_project0::protocol::PROGRAM_ID_MAINNET.into(),
            ),
            ("P0_MARKET_SOURCE", "logs".into()),
            ("P0_MARKET_HEARTBEAT_SECS", "1".into()),
        ]
        .into_iter()
        .collect();
        let get = |k: &str| env.get(k).cloned();
        let clock = Clock::new();
        let now_s = (clock.unix_ns(clock.now_ns()) / 1_000_000_000) as u64;
        let cfg = crate::analytics::econ::EconConfig::from_lookup(&get, now_s).unwrap();
        let market = crate::market_observer::MarketConfig::from_lookup(&get, now_s).unwrap();
        let run = cfg.run_dir.clone();
        let owner = |h: crate::analytics::econ::Handle| -> Result<(), String> {
            h.shared.shadow_ready.store(true, Ordering::Release);
            let _ = h.shared.shadow_ready_ns.compare_exchange(
                0,
                h.clock.now_ns(),
                Ordering::AcqRel,
                Ordering::Acquire,
            );
            while !h.drain_expired() {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(())
        };
        let result = run_econ4h_with(cfg, market, clock, Ok::<_, &'static str>(owner));
        (run, result, accepted.load(Ordering::Acquire))
    }

    /// Readiness unit E (PHASENPLAN L.2): capture, optional post-capture verification, report and
    /// qualification stay separate axes end to end on the production C0-E1 path. A capture that
    /// ran its full clock is a finished capture (market owner without failure,
    /// `capture_clock_complete`) with or without a transport gap; the gap stays
    /// `LIVE_COVERAGE_INCOMPLETE` through every verification level; `off` is `NOT_RUN`, an offline
    /// strict attempt without chain data is never canonical, the report stays PROVISIONAL and never
    /// claims the C0-E1 master, and the technical qualification — the run result — is INCOMPLETE
    /// (no canonical evidence, no evaluation), never PASS.
    #[test]
    fn unit_e_capture_verification_report_and_qualification_stay_separate() {
        use crate::analytics::report::{self, ReportQuery};
        use crate::market::observer::strict_driver::{drive, DriverRequest, StrictBudget};
        for gap in [false, true] {
            let (run, result, sessions) = unit_e_run(gap);
            assert_eq!(
                result,
                Err("C0-E1 technical qualification incomplete; see economics_final.json".into()),
                "gap={gap}"
            );
            assert_eq!(sessions, if gap { 2 } else { 1 }, "gap={gap}");
            // Both owners ended without failure: the capture itself finished (also with the gap).
            let paths = crate::run_layout::RunPaths::new(&run, crate::run_layout::RunLayout::V2);
            let mut r = crate::analytics::journal::Reader::open(
                &crate::analytics::journal::binary_path(&paths.phases()),
            )
            .unwrap();
            let mut owners = None;
            while let Ok(Some(f)) = r.next_frame() {
                if f.kind != crate::analytics::journal::kind::MEASUREMENT_PHASE {
                    continue;
                }
                let row = f
                    .typed::<crate::analytics::measurement::PhaseRecord>()
                    .unwrap();
                if let crate::analytics::measurement::Phase::OwnersFinished {
                    aborted,
                    shadow_failed,
                    market_failed,
                } = row.phase
                {
                    owners = Some((aborted, shadow_failed, market_failed));
                }
            }
            assert_eq!(owners, Some((false, false, false)), "gap={gap}");
            let live = if gap {
                "LIVE_COVERAGE_INCOMPLETE"
            } else {
                "LIVE_COVERAGE_COMPLETE"
            };
            // Verification off: the report is provisional and binds the capture axes only.
            let q = ReportQuery::default();
            let off = report::report(&run, &report::Options::default(), &q).unwrap();
            let ds = &off.dataset;
            assert_eq!(ds.verification_status, "NOT_RUN", "gap={gap}");
            assert_eq!(ds.report_status, "PROVISIONAL/INCOMPLETE", "gap={gap}");
            assert!(ds.capture_clock_complete, "gap={gap}");
            assert_eq!(ds.live_coverage_status, live, "gap={gap}");
            assert!(!ds.c0_e1_master_pass);
            // Strict, offline (no provider, no cached chain data): never canonical, and the live
            // coverage of the capture is unchanged.
            let budget = StrictBudget {
                rps: 2,
                max_calls: 10,
                max_seconds: 30,
                max_bytes: 1 << 20,
                max_disk_bytes: 1 << 24,
                max_ram_bytes: 1 << 34,
                min_free_bytes: 1,
            };
            let strict = drive(&DriverRequest {
                run: run.clone(),
                budget,
                offline: true,
                resume: false,
                mode: crate::market::observer::strict_driver::VerifyMode::Strict,
                provider_evidence: None,
            })
            .unwrap();
            assert_ne!(strict.status, "CANONICAL_VERIFIED", "gap={gap}");
            let after = report::report(&run, &report::Options::default(), &q).unwrap();
            let ds = &after.dataset;
            assert_eq!(ds.verification_status, strict.status, "gap={gap}");
            assert_eq!(
                ds.identity.verification_result_id,
                strict.verification_result_id
            );
            assert_eq!(ds.report_status, "PROVISIONAL/INCOMPLETE", "gap={gap}");
            assert_eq!(ds.live_coverage_status, live, "gap={gap}");
            assert!(ds.capture_clock_complete && !ds.c0_e1_master_pass);
            // The technical qualification of the run is never PASS here.
            let mut r = crate::analytics::journal::Reader::open(
                &run.join("snapshots")
                    .join(crate::analytics::econ::REPORT_JOURNAL),
            )
            .unwrap();
            let mut last = None;
            while let Ok(Some(f)) = r.next_frame() {
                if f.kind == crate::analytics::journal::kind::ECON_FINAL {
                    last = Some(f.typed::<crate::analytics::econ::SnapshotReport>().unwrap());
                }
            }
            let technical = last.expect("ECON_FINAL").technical_qualification;
            assert_eq!(technical.verdict, "INCOMPLETE", "gap={gap}");
            assert!(!technical.market_capture_complete, "gap={gap}");
            // C0E1-P4-VC: the separate master qualification of this production run is never a
            // PASS — no canonical result, no evaluation, no Shadow telemetry from the stand-in
            // (and with the gap no live coverage).
            let (q, _) =
                crate::analytics::qualification::qualify(&run, &Default::default()).unwrap();
            assert_eq!(q.verdict, crate::analytics::qualification::NOT_QUALIFIED);
            // No RPC: the audited deployment continuity is never proven offline.
            let mut expected = vec![];
            if gap {
                expected.extend(["LIVE_COVERAGE_INCOMPLETE", "CAPTURE_JOURNAL_INCOMPLETE"]);
            }
            expected.extend([
                "VERIFICATION_NOT_CANONICAL",
                "DEPLOYMENT_CONTINUITY_UNPROVEN",
                "TECHNICAL_NO_EVALUATIONS",
                "TECHNICAL_SHADOW_STATE",
            ]);
            assert_eq!(q.failed, expected, "gap={gap}");
            assert_eq!(q.inputs.owners.map(|o| o.aborted), Some(false));
            if !gap {
                // C0E1-P5-AUTO: the production chain after the capture — verification, report and
                // qualification as one entry; a refused or incomplete verification never
                // suppresses the report or the qualification.
                use crate::analytics::post_capture::post_capture;
                let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
                let off = post_capture(&run, "off", &[]).unwrap();
                assert!(off.verification_ok && off.report_ok, "{}", off.text);
                assert!(off.text.contains("status=NOT_RUN reason=VERIFICATION_OFF"));
                assert_eq!(off.verdict.as_deref(), Some("C0_E1_NOT_QUALIFIED"));
                assert!(off.text.contains("verdict=C0_E1_NOT_QUALIFIED"));
                // Strict offline with the configured budgets (its own spec): incomplete without
                // chain data, report and qualification still run; the same spec again needs resume.
                let first = post_capture(&run, "strict", &args(&["--offline"])).unwrap();
                assert!(!first.verification_ok && first.report_ok, "{}", first.text);
                assert!(first.text.contains("status=HISTORICAL_INCOMPLETE"));
                let again = post_capture(&run, "strict", &args(&["--offline"])).unwrap();
                assert!(!again.verification_ok && again.report_ok);
                assert!(again.text.contains("SPEC_EXISTS_USE_RESUME_VERIFICATION"));
                let resumed = post_capture(
                    &run,
                    "strict",
                    &args(&["--offline", "--resume-verification"]),
                )
                .unwrap();
                assert!(!resumed.verification_ok && resumed.report_ok);
                assert!(resumed.text.contains("status=HISTORICAL_INCOMPLETE"));
                assert!(resumed.verdict.is_some());
                // Online strict without provider evidence: refused before any chain RPC.
                let online = post_capture(&run, "strict", &[]).unwrap();
                assert!(!online.verification_ok && online.report_ok);
                assert!(online.text.contains("STRICT_PROVIDER_EVIDENCE_REQUIRED"));
                assert!(online.verdict.is_some());
                assert!(post_capture(&run, "index", &[])
                    .err()
                    .is_some_and(|e| e.starts_with("CHAIN_VERIFY_index_BLOCKED")));
            }
            let _ = std::fs::remove_dir_all(run.parent().unwrap());
        }
    }
}

#[cfg(test)]
mod d13_tests {
    /// D13: the configured worker count sizes the global Rayon pool. `run` applies the
    /// operational environment (`RAYON_NUM_THREADS` = `limits.workers`) on the single startup
    /// thread before anything uses Rayon (only the state owner's sweep does); each count is
    /// checked in a fresh process, where no earlier pool can exist.
    /// D13: only the state owner (constructed after `apply_operational_environment` in `run`)
    /// uses Rayon; no other runtime source can initialize the global pool earlier.
    #[test]
    fn d13_rayon_is_used_only_by_the_state_owner() {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&root, &mut files);
        let users: Vec<_> = files
            .iter()
            .filter(|p| {
                let text = std::fs::read_to_string(p).unwrap();
                text.contains("rayon::") || text.contains("par_iter")
            })
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        let allowed = [
            "orchestration/state_owner.rs",
            "orchestration/state_owner/sweep.rs",
            "orchestration/state_owner/tests.rs",
            "orchestration/startup.rs",
        ];
        assert!(
            users.iter().all(|u| allowed.contains(&u.as_str())),
            "{users:?}"
        );
    }

    #[test]
    fn d13_configured_workers_size_the_global_rayon_pool() {
        if let Ok(n) = std::env::var("P0_D13_CHILD") {
            let workers: u64 = n.parse().unwrap();
            let mut c = darkbot_solana::runtime_config::RuntimeConfig::default();
            c.limits.workers = workers;
            super::apply_operational_environment(&c);
            assert_eq!(rayon::current_num_threads() as u64, workers);
            return;
        }
        for workers in [1u64, 2, 4] {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "orchestration::startup::d13_tests::d13_configured_workers_size_the_global_rayon_pool",
                    "--test-threads=1",
                ])
                .env("P0_D13_CHILD", workers.to_string())
                .env_remove("RAYON_NUM_THREADS")
                .output()
                .unwrap();
            assert!(
                out.status.success() && String::from_utf8_lossy(&out.stdout).contains("1 passed"),
                "workers={workers}: {}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
    }
}

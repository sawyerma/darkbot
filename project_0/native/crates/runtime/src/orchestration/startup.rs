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
        // Legacy CLI contract of the pinned Python bridge (`operational_config.py`, PERSIST-04):
        // private file plus the machine view on stdout. The native launcher never uses it.
        if let Some(path) = &resolved.export {
            resolved.export_private(path)?;
            println!("{}", serde_json::to_string_pretty(&resolved.json())?);
            return Ok(());
        }
        if resolved.print {
            print!("{}", resolved.text());
            return Ok(());
        }
        let resolved = resolved.native_only().map_err(OperationalConfigError)?;
        if resolved.config.verification != "off"
            && std::env::var_os("P0_MEASUREMENT_PLAN").is_none()
        {
            return Err("verification requires capture.sh/verify.sh post-capture lifecycle".into());
        }
        // Single-threaded startup only. Existing constructors keep their typed
        // gates while obtaining operational values from the shared contract.
        for (k, v) in resolved.config.environment() {
            std::env::set_var(k, v);
        }
        resolved.activate()?;
        darkbot_solana::provider::enable_measurement_budget();
    }
    // FEUI-01: one runtime owner per project (launchers set P0_RUNTIME_LOCK) and an ordered
    // operator stop for the observation modes that poll it (econ4h, market). Shadow keeps the
    // default signal behaviour because it has no stop poll.
    let mode = config::require("P0_MODE")?;
    let _owner = match std::env::var_os("P0_RUNTIME_LOCK") {
        Some(path) => {
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
        shared.abort.store(true, Ordering::Release);
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
    let result = match admission {
        Ok(admission) => supervise(handle, analytics, market, clock, timeout, admission),
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

fn supervise(
    handle: crate::analytics::econ::Handle,
    analytics: std::thread::JoinHandle<Result<(), String>>,
    market: crate::market::observer::MarketConfig,
    clock: Clock,
    timeout: u64,
    admission: Option<super::boot_audit::NetworkAudit>,
) -> Result<(), String> {
    use crate::market::observer as market_observer;
    use std::sync::atomic::Ordering;
    let shadow_handle = handle.clone();
    let shadow = std::thread::Builder::new()
        .name("econ-shadow-owner".into())
        .spawn(move || {
            let result = run_shadow(clock, Some(shadow_handle.clone()), admission).map_err(|e| {
                let report = failure_report(e.as_ref());
                eprintln!("C0_E1_SHADOW_FAILED category={report}");
                report
            });
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
                market_handle.shared.abort.store(true, Ordering::Release);
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
}

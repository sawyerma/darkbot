//! CLI resolution and mode lifecycle; market dispatch precedes trading startup.
use super::state_owner::run_shadow;
use darkbot_common::{config, timing::Clock};
use std::time::Duration;

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
            darkbot_solana::runtime_config::Resolved::parse(&args, &|k| std::env::var(k).ok())?;
        if let Some(path) = &resolved.export {
            resolved.export_private(path)?;
            println!("{}", serde_json::to_string_pretty(&resolved.json())?);
            return Ok(());
        }
        if resolved.print {
            println!("{}", serde_json::to_string_pretty(&resolved.json())?);
            return Ok(());
        }
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
    if mode == "econ4h" {
        return run_econ4h().map_err(|e| e.into());
    }
    run_shadow(Clock::new(), None)
}

// Keep the first Shadow failure diagnosable without exposing provider URLs,
// response payloads or wallet paths from library Display implementations.
pub(super) fn shadow_failure_code(error: &(dyn std::error::Error + 'static)) -> &'static str {
    use solana_client::client_error::{ClientError, ClientErrorKind};
    if let Some(e) = error.downcast_ref::<crate::analytics::evidence::EvidenceFailure>() {
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

fn run_econ4h() -> Result<(), String> {
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
    let result = supervise(handle, analytics, market, clock, timeout);
    let rendered = crate::analytics::measurement::finalize_phases(&paths, result.is_ok());
    if let Err(e) = &rendered {
        eprintln!("C0_E1_PHASE_JOURNAL_INCOMPLETE {e}");
    }
    result.and(rendered)
}

fn supervise(
    handle: crate::analytics::econ::Handle,
    analytics: std::thread::JoinHandle<Result<(), String>>,
    market: crate::market::observer::MarketConfig,
    clock: Clock,
    timeout: u64,
) -> Result<(), String> {
    use crate::market::observer as market_observer;
    use std::sync::atomic::Ordering;
    let shadow_handle = handle.clone();
    let shadow = std::thread::Builder::new()
        .name("econ-shadow-owner".into())
        .spawn(move || {
            let result = run_shadow(clock, Some(shadow_handle.clone())).map_err(|e| {
                let code = shadow_failure_code(e.as_ref());
                eprintln!("C0_E1_SHADOW_FAILED category={code}");
                code.to_string()
            });
            if result.is_err() || !shadow_handle.admission_closed() {
                shadow_handle
                    .shared
                    .shadow_failed
                    .store(true, Ordering::Release);
                shadow_handle.shared.abort.store(true, Ordering::Release);
            }
            shadow_handle
                .shared
                .shadow_done
                .store(true, Ordering::Release);
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
        if super::control::stop_requested() && !handle.shared.operator_stop.load(Ordering::Acquire) {
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
        "OWNERS_FINISHED",
        serde_json::json!({"aborted":handle.shared.abort.load(Ordering::Acquire),
            "shadow_failed":handle.shared.shadow_failed.load(Ordering::Acquire),
            "market_failed":handle.shared.market_failed.load(Ordering::Acquire)}),
    )?;
    analytics
        .join()
        .map_err(|_| "C0-E1 analytics panic".to_string())?
}

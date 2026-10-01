//! `p0-longtail`: Long-Tail-Runner. Konfiguration ausschließlich aus Env (kein Wert im Code):
//!   P0_WS_URL, P0_RPC_URL, P0_PROGRAM_ID, P0_GROUP_ID (aus P1A-Manifest), P0_KEYPAIR_PATH,
//!   P0_MODE=shadow|live, P0_MIN_PROFIT, P0_MAX_*, P0_RECORD_PATH
//! Ohne Health-Engine (Feature crate_health) läuft der Prozess im Shadow-Modus bis Detection
//! und schreibt `Unavailable`-Records — damit ist die Kette messbar, bevor P3 angebunden ist.
//!
//! Status: UNTESTED / VERIFY. Threads: Ingress (WSS) → bounded channel → State/Detection-Loop;
//! Blockhash-Poller separat. Kein RPC im Entscheidungs-Loop.

use darkbot_common::channel::bounded;
use darkbot_common::config;
use darkbot_common::lifecycle::Shutdown;
use darkbot_common::timing::Clock;
use darkbot_project0::health::{AccountSource, HealthEngine, HealthError};
use darkbot_project0::protocol::{classify, AccountType};
use darkbot_solana::ingress::ws_rpc::{WsRpcConfig, WsRpcControlSource};
use darkbot_solana::ingress::{IngressEvent, IngressHalt, IngressSource, Subscription};
use darkbot_solana::state::recovery::{RecoveryDriver, SnapshotData};
use darkbot_solana::state::snapshot;
use darkbot_solana::state::StateCore;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::time::Duration;

/// S2-State als AccountSource für die Health-Engine (keine Kopie, keine RPC).
struct StateView<'a>(&'a StateCore);
impl<'a> AccountSource for StateView<'a> {
    fn account(&self, key: &solana_sdk::pubkey::Pubkey) -> Option<(solana_sdk::pubkey::Pubkey, u64, &[u8])> {
        self.0.get(key).map(|r| (r.owner, r.lamports, r.data.as_slice()))
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("p0-longtail: {}", e);
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let ws_url = config::require("P0_WS_URL")?;
    let rpc_url = config::require("P0_RPC_URL")?;
    let program = Pubkey::from_str(&config::require("P0_PROGRAM_ID")?)?;
    let mode = config::require("P0_MODE")?;
    if mode != "shadow" && mode != "live" {
        return Err("P0_MODE must be shadow|live".into());
    }
    if mode == "live" {
        return Err("live mode requires P8-LT qualification in MEASUREMENT_LEDGER; not enabled".into());
    }
    let buffer_cap: usize = config::parsed_or("P0_INGRESS_BUFFER", 65_536)?;

    let group = Pubkey::from_str(&config::require("P0_GROUP_ID")?)?;
    let clock = Clock::new();
    let shutdown = Shutdown::new();
    let halt = IngressHalt::new();
    let (producer, consumer) = bounded::<IngressEvent>("ingress", buffer_cap);

    // Ingress-Thread (WSS). Provider nur als URL.
    let sub = Subscription { programs: vec![program], accounts: vec![], slots: true, roots: true, transactions_mentioning: vec![] };
    let src = WsRpcControlSource::new(WsRpcConfig::new(ws_url, sub), clock);
    let sd = shutdown.clone();
    let h = halt.clone();
    std::thread::Builder::new().name("ingress-ws".into()).spawn(move || {
        if let Err(e) = src.run(producer, sd, h) {
            eprintln!("ingress ended: {}", e);
        }
    })?;

    // State + Recovery (Snapshot via RPC, außerhalb des Decision-Loops aufgerufen).
    let rpc = RpcClient::new(rpc_url);
    let mut st = StateCore::new(buffer_cap);
    let mut drv = RecoveryDriver::new(3);
    let mut fetch = || -> Result<SnapshotData, String> {
        let s = snapshot::program_accounts(&rpc, &program, &clock).map_err(|e| e.to_string())?;
        Ok(SnapshotData { slot: s.slot, accounts: s.accounts })
    };
    // Barrier: während der RPC-Fetches gepufferte Events in den State ziehen.
    let mut drain = |s: &mut StateCore| {
        while let Some(ev) = consumer.try_pop() {
            s.on_event(ev);
        }
    };
    // Reihenfolge: Stream läuft (puffert) → Snapshot → drain → Snapshot B → drain → reconcile → ready
    std::thread::sleep(Duration::from_millis(500));
    let out = drv.recover(&mut st, &mut fetch, &mut drain);
    eprintln!("bootstrap: {:?}, accounts={}, run_state={:?}", out, st.len(), st.run_state());
    let record_path = config::parsed_or::<String>("P0_RECORD_PATH", "data/project_0/shadow/opportunities.jsonl".into())?;
    std::fs::create_dir_all(std::path::Path::new(&record_path).parent().unwrap_or(std::path::Path::new(".")))?;
    let mut records = std::fs::OpenOptions::new().create(true).append(true).open(&record_path)?;
    use std::io::Write;

    let mut counts = [0u64; 6];
    loop {
        if shutdown.is_requested() {
            break;
        }
        if halt.is_raised() {
            st.halt(darkbot_common::error::HaltReason::FeedGap);
        }
        let Some(ev) = consumer.pop_timeout(Duration::from_millis(250)) else { continue };
        // Trigger (a) Long-Tail: jedes MarginfiAccount-Update → Detection im Shadow-Modus.
        let detect = match &ev {
            IngressEvent::Account(a) if classify(&a.data) == AccountType::MarginfiAccount && st.run_state().is_tradable() => Some((a.pubkey, a.t_rx_ns, a.slot)),
            _ => None,
        };
        if let IngressEvent::Account(a) = &ev {
            let i = match classify(&a.data) {
                AccountType::MarginfiAccount => 0,
                AccountType::Bank => 1,
                AccountType::MarginfiGroup => 2,
                AccountType::FeeState => 3,
                AccountType::LiquidationRecord => 4,
                AccountType::Unknown => 5,
            };
            counts[i] += 1;
        }
        if let Some(o) = drv.on_event(&mut st, ev, &mut fetch, &mut drain) {
            eprintln!("recovery: {:?}, run_state={:?}", o, st.run_state());
            if matches!(o, darkbot_solana::state::recovery::RecoveryOutcome::Ready) {
                halt.clear();
            }
        }
        if let Some((acct, t_rx, slot)) = detect {
            // Health über das Crate (Feature crate_health), sonst fail-closed NoHealth → Unavailable-Record.
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let view = StateView(&st);
            #[cfg(feature = "crate_health")]
            let engine = darkbot_project0::health::crate_impl::CrateHealth { state: &view, group };
            #[cfg(not(feature = "crate_health"))]
            let engine = darkbot_project0::health::NoHealth;
            let _ = &view;
            let line = match engine.health(&acct, now as i64, slot) {
                Ok(h) => format!("{{\"account\":\"{}\",\"trigger\":\"account_update\",\"t_rx_ns\":{},\"slot\":{},\"liquidatable\":{},\"maint_health_bits\":{},\"equity_assets_bits\":{},\"bad_debt\":{}}}", acct, t_rx, slot, h.liquidatable, h.maintenance_health, h.equity_assets, h.is_bad_debt),
                Err(HealthError::Unavailable) => format!("{{\"account\":\"{}\",\"trigger\":\"account_update\",\"t_rx_ns\":{},\"slot\":{},\"decision\":\"Unavailable\"}}", acct, t_rx, slot),
                Err(e) => format!("{{\"account\":\"{}\",\"trigger\":\"account_update\",\"t_rx_ns\":{},\"slot\":{},\"error\":\"{}\"}}", acct, t_rx, slot, e),
            };
            let _ = writeln!(records, "{}", line);
        }
        // Detection/Pipeline (P2→P5) wird hier eingehängt, sobald HealthEngine (P3) verfügbar ist.
        // Bis dahin: nur State + Zähler, damit S1-LT/S2 messbar sind (Ledger).
        if counts.iter().sum::<u64>() % 1000 == 0 {
            eprintln!("updates: accounts={} banks={} groups={} fee={} liqrec={} unknown={} channel={:?}",
                counts[0], counts[1], counts[2], counts[3], counts[4], counts[5], consumer.stats());
        }
    }
    Ok(())
}

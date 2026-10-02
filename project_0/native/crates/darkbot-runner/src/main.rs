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
use darkbot_project0::economics::{FailedCostMatrix, Path};
use darkbot_project0::execution::Freshness;
use darkbot_project0::planner::Allowlist;
use darkbot_project0::protocol;
use darkbot_project0::safety::{Allow, Limits, Safety};
use darkbot_runner::pipeline::{Mode, Pipeline, ReceivershipBuilder, ReceivershipPlan};
use darkbot_runner::pending::{try_reconcile, PendingSubmission};
use darkbot_runner::record::Decision;
use darkbot_runner::shadow_ledger::ShadowLedger;
use darkbot_runner::winner::{find_winner, race_outcome, RaceOutcome};
use darkbot_solana::ingress::SlotWatch;
use darkbot_solana::state::blockhash::BlockhashCache;
use darkbot_solana::submit::rpc::RpcSubmitter;
use darkbot_solana::tx::{self, Resources, TxLimits};
use darkbot_project0::protocol::{classify as classify_account, AccountType}; // [FIX-47] Alias
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

/// [FIX-1] Ohne P4A-03-Sizing gibt es keine Pläne — fail closed, kein geschätzter Betrag.
struct NoPlans;
impl ReceivershipBuilder for NoPlans {
    fn plans(&self, _: &solana_sdk::pubkey::Pubkey, _: u64) -> Result<Vec<ReceivershipPlan>, String> {
        Err("unavailable: P4A-03 sizing provider not configured".into())
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
    let ws_url_for_spawn = ws_url.clone();
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
    let slot_watch = SlotWatch::new(); // [FIX-40]
    let (producer, consumer) = bounded::<IngressEvent>("ingress", buffer_cap);

    // [FIX-26] Externe Abhängigkeiten (Oracles, Integrationen, Mints) aus den Banks ableiten — vor dem
    // Stream-Start, damit accountSubscribe + Snapshot sie von Anfang an halten. Ein zusätzlicher gPA.
    let dep_rpc = RpcClient::new(config::require("P0_RPC_URL")?);
    let pre = snapshot::program_accounts(&dep_rpc, &program, &clock)?;
    let mut pre_state = StateCore::new(1);
    pre_state.install_snapshot(pre.slot, pre.accounts);
    let bank_keys: Vec<Pubkey> = pre_state.iter_owned_by(&program).filter(|(_, r)| classify_account(&r.data) == AccountType::Bank).map(|(k, _)| *k).collect();
    #[cfg(feature = "crate_health")]
    let mut dependency_keys = darkbot_project0::health::dependency_keys(&StateView(&pre_state), &program, &bank_keys);
    #[cfg(not(feature = "crate_health"))]
    let mut dependency_keys: Vec<Pubkey> = Vec::new();
    eprintln!("dependencies: banks={} external_accounts={}", bank_keys.len(), dependency_keys.len());
    drop(pre_state);
    // Ingress-Thread (WSS). Provider nur als URL.
    let sub = Subscription { programs: vec![program], accounts: dependency_keys.clone(), slots: true, roots: true, transactions_mentioning: vec![] };
    // [FIX-34] Ingress als neu startbare Session: bei Änderung der Dependency-Menge wird die alte
    // Session beendet, eine neue mit erweiterter Subscription gestartet, dann HALT → Recovery → READY.
    let slot_watch_for_spawn = slot_watch.clone();
    let spawn_ingress = |sub: Subscription, producer: darkbot_common::channel::Producer<IngressEvent>, halt: IngressHalt| -> std::io::Result<Shutdown> {
        let session = Shutdown::new();
        let src = WsRpcControlSource::new(WsRpcConfig::new(ws_url_for_spawn.clone(), sub), clock);
        let sd = session.clone();
        let sw = slot_watch_for_spawn.clone();
        std::thread::Builder::new().name("ingress-ws".into()).spawn(move || {
            if let Err(e) = src.run(producer, sd, halt, sw) { eprintln!("ingress ended: {}", e); }
        })?;
        Ok(session)
    };
    let mut subscribed: std::collections::HashSet<Pubkey> = dependency_keys.iter().copied().collect();
    let mut ingress_session = spawn_ingress(sub.clone(), producer.clone(), halt.clone())?;

    // State + Recovery (Snapshot via RPC, außerhalb des Decision-Loops aufgerufen).
    let rpc = RpcClient::new(rpc_url.clone());
    let rpc_recon = RpcClient::new(rpc_url); // [FIX-5] eigener Client für Reconciliation (fetch-Closure besitzt `rpc`)
    let mut st = StateCore::new(buffer_cap);
    let mut drv = RecoveryDriver::new(3);
    let dep_shared = std::sync::Arc::new(std::sync::Mutex::new(dependency_keys.clone()));
    let dep_for_fetch = dep_shared.clone();
    let mut fetch = move || -> Result<SnapshotData, String> {
        // [FIX-26] Snapshot = P0-Programm + externe Abhängigkeiten, gemeinsam reconciled.
        let s = snapshot::program_accounts(&rpc, &program, &clock).map_err(|e| e.to_string())?;
        let keys = dep_for_fetch.lock().map_err(|_| "dep lock".to_string())?.clone();
        let d = snapshot::multiple_accounts(&rpc, &keys, &clock).map_err(|e| e.to_string())?;
        let mut accounts = s.accounts;
        accounts.extend(d.accounts);
        // [FIX-37] alle Dependency-Keys sind required (Oracles/Integrationen/Mints): fehlend → nicht sauber.
        Ok(SnapshotData { slot: s.slot.min(if d.slot == 0 { s.slot } else { d.slot }), accounts, missing_required: d.missing })
    };
    // Barrier: während der RPC-Fetches gepufferte Events in den State ziehen.
    let mut drain = |s: &mut StateCore| {
        while let Some(ev) = consumer.try_pop() {
            s.on_event(ev);
        }
    };
    // [FIX-35] Bootstrap läuft über die StreamReady-Barriere: das erste `Reconnected` (alle ACKs)
    // löst im Event-Loop die Recovery aus. Kein sleep, kein vorzeitiger Snapshot.
    eprintln!("bootstrap: waiting for StreamReady (all subscription ACKs)");
    // [FIX-1] Pipeline-Bausteine: Keypair (nur signieren, Shadow sendet nicht), Blockhash-Cache,
    // Safety aus Env (Limits sind Konfiguration), Allowlist aus Source, Submitter nur für Simulation.
    let payer = tx::load_keypair_json(&config::require("P0_KEYPAIR_PATH")?)?;
    let blockhash = BlockhashCache::new();
    let bh_rpc = RpcClient::new(config::require("P0_RPC_URL")?);
    let bh_cache = blockhash.clone();
    let bh_clock = clock;
    std::thread::Builder::new().name("blockhash".into()).spawn(move || loop {
        if let Err(e) = bh_cache.refresh(&bh_rpc, bh_clock.now_ns()) { eprintln!("blockhash refresh: {}", e); }
        std::thread::sleep(Duration::from_secs(10));
    })?;
    let limits = Limits {
        max_total_capital: config::parsed_or("P0_MAX_TOTAL_CAPITAL", 0)?,
        max_outstanding_inventory: config::parsed_or("P0_MAX_INVENTORY", 0)?,
        max_concurrent_positions: config::parsed_or("P0_MAX_CONCURRENT", 1)?,
        max_exposure_per_attempt: config::parsed_or("P0_MAX_EXPOSURE", 0)?,
        max_daily_loss: config::parsed_or("P0_MAX_DAILY_LOSS", 0)?,
        max_failed_attempts_per_day: config::parsed_or("P0_MAX_FAILED", 0)?,
        max_tip: config::parsed_or("P0_MAX_TIP", 0)?,
        max_slippage_bps: config::parsed_or("P0_MAX_SLIPPAGE_BPS", 0)?,
        max_outstanding_attempts: config::parsed_or("P0_MAX_OUTSTANDING", 1)?,
    };
    let mut allow = Allow::default();
    allow.execution_classes.insert("longtail");
    let mut safety = Safety::new(limits, allow, false);
    // [FIX-25] Semantische Kategorien getrennt; die globale Top-Level-Allowlist prüft die Pipeline separat.
    let mut allowlist = Allowlist::default();
    for s in [protocol::JUP_SWAP_PROGRAM_ID, protocol::TITAN_SWAP_PROGRAM_ID] { allowlist.swap_programs.insert(Pubkey::from_str(s)?); }
    for s in [protocol::KAMINO_PROGRAM_ID, protocol::DRIFT_PROGRAM_ID, protocol::JUPLEND_PROGRAM_ID] {
        let p = Pubkey::from_str(s)?;
        allowlist.integration_programs.insert(p);
        allowlist.refresh_programs.insert(p); // Pre-Start-Refreshes nur dieser Programme (liquidate_start.rs)
    }
    allowlist.refresh_programs.insert(program); // init_liquidation_record vor Start
    let global_allowed = protocol::receivership_allowed_programs();
    // [FIX-30] Prerequisite-TXs (JIT-Buy): Swap-Programme + System/Token/Token-2022/ATA/ComputeBudget.
    let prereq_allowed: Vec<Pubkey> = [protocol::JUP_SWAP_PROGRAM_ID, protocol::TITAN_SWAP_PROGRAM_ID, protocol::ASSOCIATED_TOKEN_PROGRAM_ID, protocol::COMPUTE_BUDGET_PROGRAM_ID,
        darkbot_project0::receivership::SYSTEM_PROGRAM, "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"].iter().map(|s| Pubkey::from_str(s)).collect::<Result<_, _>>()?;
    let submitter = RpcSubmitter::new(config::require("P0_RPC_URL")?, true);
    let failed_costs = FailedCostMatrix::default(); // alle None → Unknown → nicht handelbar, bis gemessen
    let builder = NoPlans;
    let resources = Resources { compute_unit_limit: config::parsed_or("P0_CU_LIMIT", 1_400_000)?, priority_micro_lamports_per_cu: config::parsed_or("P0_PRIORITY_MICRO", 0)?, loaded_accounts_data_size: Some(config::parsed_or("P0_LOADED_DATA", 64 * 1024 * 1024)?) };
    let freshness = Freshness { max_opportunity_age_ns: config::parsed_or("P0_MAX_AGE_NS", 2_000_000_000)?, execution_deadline_ns: config::parsed_or("P0_DEADLINE_NS", 2_000_000_000)? };
    let min_profit: i128 = config::parsed_or::<i128>("P0_MIN_PROFIT", 0)?;
    let mut pending: Vec<PendingSubmission> = Vec::new();
    // [M2][M3] Shadow-Evidence: Kandidaten-Watchlist für den Winner-Vergleich und Tages-Ledger.
    let mut ledger = ShadowLedger::new();
    let mut watch: Vec<(Pubkey, u64, u64, Option<u64>, Option<i128>, bool, Option<darkbot_runner::record::LossReason>, u64)> = Vec::new(); // (account, rx_slot, detect_slot, sign_slot, expected_net, shadow_ready, our_loss, created_unix)
    let summary_path = config::parsed_or::<String>("P0_SUMMARY_PATH", "project_0/data/shadow/daily_summary.jsonl".into())?;
    let mut summaries = std::fs::OpenOptions::new().create(true).append(true).open(&summary_path)?;
    let _ = Path::Rpc;
    let record_path = config::parsed_or::<String>("P0_RECORD_PATH", "project_0/data/shadow/opportunities.jsonl".into())?;
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
            IngressEvent::Account(a) if classify_account(&a.data) == AccountType::MarginfiAccount && st.run_state().is_tradable() => Some((a.pubkey, a.t_rx_ns, a.slot)),
            _ => None,
        };
        // [FIX-35] Bank-Pubkey merken; die Dependency-Prüfung läuft NACH dem Anwenden des Updates.
        let updated_bank: Option<Pubkey> = match &ev {
            IngressEvent::Account(a) if classify_account(&a.data) == AccountType::Bank => Some(a.pubkey),
            _ => None,
        };
        if let IngressEvent::Account(a) = &ev {
            let i = match classify_account(&a.data) {
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
        // [FIX-34][FIX-35] Nach dem Anwenden: zeigt die NEUE Bank-Konfiguration auf nicht abonnierte Accounts?
        #[cfg(feature = "crate_health")]
        if let Some(bk) = updated_bank {
            let new_keys: Vec<Pubkey> = {
                let view = StateView(&st);
                darkbot_project0::health::dependency_keys(&view, &program, &[bk]).into_iter().filter(|k| !subscribed.contains(k)).collect()
            };
            if !new_keys.is_empty() {
                eprintln!("dependency set changed: +{} accounts → HALT, resubscribe; Recovery folgt auf StreamReady", new_keys.len());
                st.halt(darkbot_common::error::HaltReason::StateGap);
                ingress_session.request();
                subscribed.extend(new_keys.iter().copied());
                dependency_keys = subscribed.iter().copied().collect();
                if let Ok(mut g) = dep_shared.lock() { *g = dependency_keys.clone(); }
                let sub2 = Subscription { programs: vec![program], accounts: dependency_keys.clone(), slots: true, roots: true, transactions_mentioning: vec![] };
                ingress_session = spawn_ingress(sub2, producer.clone(), halt.clone())?;
                // kein sleep, keine direkte Recovery: das `Reconnected` der neuen Session (alle ACKs) startet sie.
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
            if let Some(day) = ledger.on_account_update(now) { if let Ok(j) = serde_json::to_string(&day) { let _ = writeln!(summaries, "{}", j); } }
            let line = match engine.health(&acct, now as i64, slot) {
                Ok(h) => { ledger.on_health(h.liquidatable, false); format!("{{\"account\":\"{}\",\"trigger\":\"account_update\",\"t_rx_ns\":{},\"slot\":{},\"liquidatable\":{},\"maint_health_bits\":{},\"equity_assets_bits\":{},\"bad_debt\":{}}}", acct, t_rx, slot, h.liquidatable, h.maintenance_health, h.equity_assets, h.is_bad_debt) },
                Err(HealthError::Unavailable) => { ledger.on_health(false, false); format!("{{\"account\":\"{}\",\"trigger\":\"account_update\",\"t_rx_ns\":{},\"slot\":{},\"decision\":\"Unavailable\"}}", acct, t_rx, slot) },
                Err(e) => { ledger.on_health(false, matches!(e, HealthError::Unsupported(_))); format!("{{\"account\":\"{}\",\"trigger\":\"account_update\",\"t_rx_ns\":{},\"slot\":{},\"error\":\"{}\"}}", acct, t_rx, slot, e) },
            };
            let _ = writeln!(records, "{}", line);
            // [FIX-1] Pipeline (Shadow): Health → Pläne → Economics → Build → Sign → Send-Boundary.
            let mut pipe = Pipeline {
                mode: Mode::Shadow, clock: &clock, health: &engine, builder: &builder, submitter: &submitter,
                safety: &mut safety, blockhash: &blockhash, payer: &payer, allowlist: &allowlist, failed_costs: &failed_costs,
                limits: TxLimits::default(), resources, min_profit, blockhash_max_age_ns: 30_000_000_000, p7_passed: false,
                freshness, tip_account: None, global_allowed: global_allowed.clone(), prereq_allowed: prereq_allowed.clone(), slot_watch: &slot_watch,
            };
            let rec = pipe.evaluate(acct, "account_update", t_rx, now, slot, st.run_state().is_tradable());
            if let Ok(j) = serde_json::to_string(&rec) { let _ = writeln!(records, "{}", j); }
            // [M3] Ledger + [M2] Watchlist (nur Shadow-relevante Entscheidungen).
            let sim_ok = match rec.decision { Decision::ShadowReady => Some(true), Decision::ShadowSimFailed => Some(false), _ => None };
            ledger.on_record(&rec, sim_ok);
            // [FIX-43] strukturierter Verlustgrund aus dem Record; [FIX-45] nur ShadowReady kann ein Win werden.
            if matches!(rec.decision, Decision::ShadowReady | Decision::ShadowSimFailed | Decision::Skip) {
                let shadow_ready = matches!(rec.decision, Decision::ShadowReady);
                let our_loss = if shadow_ready { None } else { Some(rec.loss.unwrap_or(darkbot_runner::record::LossReason::Build)) };
                watch.push((acct, slot, slot, rec.sign_slot, rec.net_profit, shadow_ready, our_loss, now));
            }
            // [FIX-18][FIX-23] Submitted und AmbiguousSubmission landen beide in der Reconciliation.
            if let (Some(sig), Some(exp)) = (rec.signature.as_deref(), rec.exposure) {
                if let Ok(s) = sig.parse() {
                    let sigs = rec.all_signatures.iter().filter_map(|x| x.parse().ok()).collect();
                    // [FIX-33] tatsächlich verwendeter Pfad aus dem Record, nicht die Adapter-Default.
                    let path = match rec.path.as_deref() { Some("JITO_BUNDLE") => darkbot_solana::submit::SubmissionPath::JitoBundle, Some("JITO_SINGLE") => darkbot_solana::submit::SubmissionPath::JitoSingle, _ => darkbot_solana::submit::SubmissionPath::Rpc };
                    pending.push(PendingSubmission { signature: s, signatures: sigs, bundle_id: rec.bundle_id.clone(), path, exposure: exp, submitted_unix_s: now });
                }
            }
        }
        // [M2] Winner-Vergleich: nach ≥ 60 s nachsehen, wer den Account tatsächlich liquidiert hat; nach 15 min aufgeben.
        if !watch.is_empty() {
            let now_s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let mut keep = Vec::with_capacity(watch.len());
            for w in watch.drain(..) {
                let (acct, rx_slot, det_slot, sign_slot, net, shadow_ready, our_loss, created) = w;
                if now_s.saturating_sub(created) < 60 { keep.push(w); continue; }
                match find_winner(&rpc_recon, &program, &acct, rx_slot.saturating_sub(2)) {
                    Ok(Some(win)) => {
                        let out = race_outcome(win.slot, rx_slot, det_slot, sign_slot, shadow_ready, our_loss);
                        match out {
                            RaceOutcome::ReadyBeforeWinner => ledger.on_race(true, None, net),
                            RaceOutcome::SameSlotUnknown => ledger.on_same_slot(),
                            RaceOutcome::Loss(l) => ledger.on_race(false, Some(l), None),
                        }
                        let line = serde_json::json!({"race": {"account": acct.to_string(), "winner": win, "our_sign_slot": sign_slot, "outcome": out, "expected_net": net}});
                        let _ = writeln!(records, "{}", line);
                    }
                    Ok(None) if now_s.saturating_sub(created) < 900 => keep.push(w),
                    Ok(None) => { /* niemand hat liquidiert: kein Rennen, keine Zählung */ }
                    Err(_) => keep.push(w),
                }
            }
            watch = keep;
        }
        // [FIX-5] Offene Submissions reconciliieren → Safety-Buchhaltung schließen.
        if !pending.is_empty() {
            let now_s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let mut still = Vec::with_capacity(pending.len());
            for p in pending.drain(..) {
                match try_reconcile(&rpc_recon, None, &p, now_s, clock.now_ns(), 90) {
                    Some(r) => {
                        safety.record_outcome(now_s, r.landed_success, r.realized_pnl, p.exposure, 0); // [FIX-24] None = UNKNOWN → HALT
                        if let Ok(j) = serde_json::to_string(&r.record) { let _ = writeln!(records, "{}", j); }
                    }
                    None => still.push(p),
                }
            }
            pending = still;
        }
        // Detection/Pipeline (P2→P5) wird hier eingehängt, sobald HealthEngine (P3) verfügbar ist.
        // Bis dahin: nur State + Zähler, damit S1-LT/S2 messbar sind (Ledger).
        if counts.iter().sum::<u64>() % 1000 == 0 {
            eprintln!("updates: accounts={} banks={} groups={} fee={} liqrec={} unknown={} channel={:?}",
                counts[0], counts[1], counts[2], counts[3], counts[4], counts[5], consumer.stats());
        }
    }
    // [FIX-46] Letzten (Teil-)Tag beim regulären Shutdown schreiben, damit das Ledger vollständig ist.
    let final_summary = ledger.finish();
    if let Ok(j) = serde_json::to_string(&final_summary) { let _ = writeln!(summaries, "{}", j); }
    let _ = summaries.flush();
    let _ = records.flush();
    Ok(())
}

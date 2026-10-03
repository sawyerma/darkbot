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
use darkbot_runner::pipeline::{Mode, Pipeline};
#[cfg(not(feature = "crate_health"))]
use darkbot_runner::pipeline::{ReceivershipBuilder, ReceivershipPlan};
#[cfg(feature = "crate_health")]
use darkbot_runner::builder::p0::{P0ReceivershipBuilder, ProtocolSizingProvider};
#[cfg(feature = "crate_health")]
use darkbot_runner::route::JupiterV2RouteProvider;
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
use darkbot_solana::ingress::{IngressEvent, IngressHalt, IngressSource, SourceKind, Subscription};
use darkbot_solana::state::recovery::{RecoveryDriver, SnapshotData};
use darkbot_solana::state::snapshot;
use darkbot_solana::state::StateCore;
use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
#[cfg(feature = "crate_health")]
use marginfi_type_crate::{
    constants::{ASSET_TAG_DEFAULT, ASSET_TAG_SOL},
    types::{Bank, BankOperationalState, MarginfiAccount},
};
use std::str::FromStr;
use std::time::Duration;

/// S2-State als AccountSource für die Health-Engine (keine Kopie, keine RPC).
struct StateView<'a>(&'a StateCore);
impl<'a> AccountSource for StateView<'a> {
    fn account(&self, key: &solana_sdk::pubkey::Pubkey) -> Option<(solana_sdk::pubkey::Pubkey, u64, &[u8])> {
        self.0.get(key).map(|r| (r.owner, r.lamports, r.data.as_slice()))
    }
}

#[cfg(not(feature = "crate_health"))]
struct NoPlans;
#[cfg(not(feature = "crate_health"))]
impl ReceivershipBuilder for NoPlans {
    fn plans(&self, _: &solana_sdk::pubkey::Pubkey, _: u64, _: u64) -> Result<Vec<ReceivershipPlan>, String> {
        Err("unavailable: crate_health disabled".into())
    }
}

#[cfg(feature = "crate_health")]
fn bank_from_state<S: AccountSource + ?Sized>(state: &S, program: &Pubkey, bank_key: &Pubkey) -> Option<Bank> {
    let (owner, _, data) = state.account(bank_key)?;
    if owner != *program || classify_account(data) != AccountType::Bank {
        return None;
    }
    let need = 8 + std::mem::size_of::<Bank>();
    if data.len() < need {
        return None;
    }
    bytemuck::try_from_bytes::<Bank>(&data[8..need]).ok().copied()
}

#[cfg(feature = "crate_health")]
fn associated_token_address(owner: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
    let ata_program = Pubkey::from_str(protocol::ASSOCIATED_TOKEN_PROGRAM_ID).expect("static ATA program");
    Pubkey::find_program_address(
        &[owner.as_ref(), token_program.as_ref(), mint.as_ref()],
        &ata_program,
    ).0
}

#[cfg(feature = "crate_health")]
fn ata_candidates_for_bank<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
    bank_key: &Pubkey,
    liquidator: &Pubkey,
) -> Vec<Pubkey> {
    let Some(bank) = bank_from_state(state, program, bank_key) else { return Vec::new() };
    [protocol::TOKEN_PROGRAM_ID, protocol::TOKEN_2022_PROGRAM_ID]
        .iter()
        .filter_map(|s| Pubkey::from_str(s).ok())
        .map(|tp| associated_token_address(liquidator, &bank.mint, &tp))
        .collect()
}

#[cfg(feature = "crate_health")]
fn validated_liquidator_ata<S: AccountSource + ?Sized>(
    state: &S,
    mint: &Pubkey,
    liquidator: &Pubkey,
) -> Option<(Pubkey, u64)> {
    for token_program in [protocol::TOKEN_PROGRAM_ID, protocol::TOKEN_2022_PROGRAM_ID]
        .iter()
        .filter_map(|s| Pubkey::from_str(s).ok())
    {
        let ata = associated_token_address(liquidator, mint, &token_program);
        let Some((owner, _, data)) = state.account(&ata) else { continue };
        if owner != token_program || data.len() < 72 {
            continue;
        }
        let account_mint = Pubkey::new_from_array(data[0..32].try_into().ok()?);
        let account_owner = Pubkey::new_from_array(data[32..64].try_into().ok()?);
        if account_mint != *mint || account_owner != *liquidator {
            continue;
        }
        let amount = u64::from_le_bytes(data[64..72].try_into().ok()?);
        return Some((ata, amount));
    }
    None
}

#[cfg(feature = "crate_health")]
fn find_supported_cost_bank_by_mint<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
    banks: &[Pubkey],
    mint: &Pubkey,
) -> Option<(Pubkey, Bank)> {
    // [FIX-94] Cost valuation must not accidentally select an integration bank merely because
    // it shares the same mint. Mirror ProtocolSizer's supported-generic-bank policy and pick
    // deterministically by pubkey.
    let mut candidates: Vec<(Pubkey, Bank)> = banks
        .iter()
        .filter_map(|bk| {
            let bank = bank_from_state(state, program, bk)?;
            let supported_tag = matches!(bank.config.asset_tag, ASSET_TAG_DEFAULT | ASSET_TAG_SOL);
            let supported_state = matches!(
                bank.config.operational_state,
                BankOperationalState::Operational
                    | BankOperationalState::ReduceOnly
                    | BankOperationalState::ReduceOnlyWithBorrowingPower
            );
            (bank.mint == *mint && supported_tag && supported_state).then_some((*bk, bank))
        })
        .collect();
    candidates.sort_unstable_by_key(|(pk, _)| pk.to_bytes());
    candidates.into_iter().next()
}

#[cfg(feature = "crate_health")]
fn marginfi_account_in_group(data: &[u8], group: &Pubkey) -> bool {
    if classify_account(data) != AccountType::MarginfiAccount {
        return false;
    }
    let need = 8 + std::mem::size_of::<MarginfiAccount>();
    if data.len() < need {
        return false;
    }
    bytemuck::try_from_bytes::<MarginfiAccount>(&data[8..need])
        .map(|a| a.group == *group)
        .unwrap_or(false)
}

#[cfg(feature = "crate_health")]
fn bootstrap_probe_candidate(data: &[u8], group: &Pubkey) -> bool {
    if classify_account(data) != AccountType::MarginfiAccount {
        return false;
    }
    let need = 8 + std::mem::size_of::<MarginfiAccount>();
    if data.len() < need {
        return false;
    }
    bytemuck::try_from_bytes::<MarginfiAccount>(&data[8..need])
        .map(|a| a.group == *group && a.indexer_flags.was_liquidatable == 1)
        .unwrap_or(false)
}

#[cfg(feature = "crate_health")]
fn bank_filters(group: &Pubkey) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"dataSize": 8 + std::mem::size_of::<Bank>()}),
        serde_json::json!({"memcmp": {"offset": 41, "bytes": group.to_string()}}),
    ]
}

#[cfg(feature = "crate_health")]
fn liability_account_filters(group: &Pubkey) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"dataSize": 8 + std::mem::size_of::<MarginfiAccount>()}),
        serde_json::json!({"memcmp": {"offset": 8, "bytes": group.to_string()}}),
        // IndexerFlags.is_lending_only == 0; one zero byte is base58 "1".
        serde_json::json!({"memcmp": {"offset": 8 + 2248, "bytes": "1"}}),
    ]
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
    let payer = tx::load_keypair_json(&config::require("P0_KEYPAIR_PATH")?)?;
    let liquidator = tx::payer_of(&payer);
    let clock = Clock::new();
    let shutdown = Shutdown::new();
    let halt = IngressHalt::new();
    let slot_watch = SlotWatch::new(); // [FIX-40]
    let (producer, consumer) = bounded::<IngressEvent>("ingress", buffer_cap);

    // [FIX-78] Bootstrap only the configured group's Banks first. Full-program GPA was ~387 MB
    // in C0 and is unnecessary for dependency discovery.
    let dep_rpc = RpcClient::new(config::require("P0_RPC_URL")?);

    // [FIX-98] A signed Shadow simulation uses the real fee payer. If that pubkey has never
    // been funded on-chain, Solana rejects the transaction during account loading with
    // TransactionError::AccountNotFound before any program executes (empty logs). Fail here
    // with the exact payer instead of misclassifying the first real Build/Sign/Sim candidate.
    let payer_account = dep_rpc
        .get_account_with_commitment(&liquidator, CommitmentConfig::confirmed())?
        .value;
    let Some(payer_account) = payer_account else {
        return Err(format!(
            "PAYER_ACCOUNT_NOT_FOUND: liquidator={} has no confirmed on-chain account; fund this exact fee-payer pubkey with SOL before Shadow Build/Sign/Simulation",
            liquidator
        ).into());
    };
    eprintln!(
        "payer_preflight: pubkey={} lamports={} owner={} executable={}",
        liquidator,
        payer_account.lamports,
        payer_account.owner,
        payer_account.executable
    );
    #[cfg(feature = "crate_health")]
    let pre = snapshot::program_accounts_filtered(&dep_rpc, &program, bank_filters(&group), &clock)?;
    #[cfg(not(feature = "crate_health"))]
    let pre = snapshot::program_accounts(&dep_rpc, &program, &clock)?;
    let mut pre_state = StateCore::new(1);
    pre_state.install_snapshot(pre.slot, pre.accounts);
    let bank_keys: Vec<Pubkey> = pre_state
        .iter_owned_by(&program)
        .filter_map(|(k, r)| {
            if classify_account(&r.data) != AccountType::Bank { return None; }
            #[cfg(feature = "crate_health")]
            {
                let view = StateView(&pre_state);
                return bank_from_state(&view, &program, k)
                    .filter(|b| b.group == group)
                    .map(|_| *k);
            }
            #[cfg(not(feature = "crate_health"))]
            { Some(*k) }
        })
        .collect();
    #[cfg(feature = "crate_health")]
    let mut required_dependency_keys = {
        let mut v = darkbot_project0::health::dependency_keys(&StateView(&pre_state), &program, &bank_keys);
        v.push(group);
        v.push(protocol::fee_state_pda(&program));
        v.sort_unstable_by_key(|k| k.to_bytes());
        v.dedup();
        v
    };
    #[cfg(not(feature = "crate_health"))]
    let mut required_dependency_keys: Vec<Pubkey> = Vec::new();

    #[cfg(feature = "crate_health")]
    let mut optional_dependency_keys: Vec<Pubkey> = bank_keys.iter()
        .flat_map(|bk| ata_candidates_for_bank(&StateView(&pre_state), &program, bk, &liquidator))
        .collect();
    #[cfg(not(feature = "crate_health"))]
    let mut optional_dependency_keys: Vec<Pubkey> = Vec::new();
    optional_dependency_keys.sort_unstable_by_key(|k| k.to_bytes());
    optional_dependency_keys.dedup();

    #[cfg(feature = "crate_health")]
    let native_sol_mint = Pubkey::from_str(protocol::NATIVE_SOL_MINT)?;
    #[cfg(feature = "crate_health")]
    let (sol_bank, sol_cost_bank) =
        find_supported_cost_bank_by_mint(&StateView(&pre_state), &program, &bank_keys, &native_sol_mint)
            .ok_or("no supported generic Marginfi WSOL bank found; cannot value SOL-denominated liquidation costs")?;
    #[cfg(feature = "crate_health")]
    eprintln!(
        "sol_cost_bank: {} asset_tag={} oracle_setup={:?} operational_state={:?}",
        sol_bank,
        sol_cost_bank.config.asset_tag,
        sol_cost_bank.config.oracle_setup,
        sol_cost_bank.config.operational_state
    );

    let mut subscription_keys = required_dependency_keys.clone();
    subscription_keys.extend(optional_dependency_keys.iter().copied());
    subscription_keys.sort_unstable_by_key(|k| k.to_bytes());
    subscription_keys.dedup();

    eprintln!(
        "dependencies: banks={} required={} optional_atas={}",
        bank_keys.len(),
        required_dependency_keys.len(),
        optional_dependency_keys.len()
    );
    drop(pre_state);
    // Ingress-Thread (WSS). Provider nur als URL.
    let sub = Subscription {
        programs: vec![],
        program_filters: vec![
            (program, liability_account_filters(&group)),
            (program, bank_filters(&group)),
        ],
        accounts: subscription_keys.clone(),
        slots: true,
        roots: true,
        transactions_mentioning: vec![],
    };
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
    let mut subscribed: std::collections::HashSet<Pubkey> = subscription_keys.iter().copied().collect();
    let mut required_set: std::collections::HashSet<Pubkey> = required_dependency_keys.iter().copied().collect();
    let mut optional_set: std::collections::HashSet<Pubkey> = optional_dependency_keys.iter().copied().collect();
    let mut ingress_session = spawn_ingress(sub.clone(), producer.clone(), halt.clone())?;

    // State + Recovery (Snapshot via RPC, außerhalb des Decision-Loops aufgerufen).
    let rpc = RpcClient::new(rpc_url.clone());
    let rpc_recon = RpcClient::new(rpc_url); // [FIX-5] eigener Client für Reconciliation (fetch-Closure besitzt `rpc`)
    let mut st = StateCore::new(buffer_cap);
    // [FIX-96] Production recovery waits for confirmed WSS/root catch-up before
    // Snapshot-B reconciliation. This prevents false dirty reconciles when RPC Snapshot B
    // is ahead of account notifications still in flight on the same confirmed stream.
    let mut drv = RecoveryDriver::new(3).with_root_catchup(Duration::from_secs(30));
    let dep_shared = std::sync::Arc::new(std::sync::Mutex::new(required_dependency_keys.clone()));
    let opt_shared = std::sync::Arc::new(std::sync::Mutex::new(optional_dependency_keys.clone()));
    let dep_for_fetch = dep_shared.clone();
    let opt_for_fetch = opt_shared.clone();
    let mut fetch = move || -> Result<SnapshotData, String> {
        let t0 = std::time::Instant::now();
        eprintln!("snapshot: start");
        // [FIX-78] Recovery snapshot = target-group liability accounts + target-group banks +
        // explicit required/optional dependencies. Lending-only accounts are excluded.
        #[cfg(feature = "crate_health")]
        let a = snapshot::program_accounts_filtered(&rpc, &program, liability_account_filters(&group), &clock)
            .map_err(|e| format!("stage=liability_gpa {e}"))?;
        #[cfg(feature = "crate_health")]
        let b = snapshot::program_accounts_filtered(&rpc, &program, bank_filters(&group), &clock)
            .map_err(|e| format!("stage=bank_gpa {e}"))?;
        #[cfg(not(feature = "crate_health"))]
        let a = snapshot::program_accounts(&rpc, &program, &clock).map_err(|e| e.to_string())?;
        #[cfg(not(feature = "crate_health"))]
        let b = darkbot_solana::state::snapshot::Snapshot { slot: a.slot, accounts: Vec::new(), missing: Vec::new() };
        let required = dep_for_fetch.lock().map_err(|_| "dep lock".to_string())?.clone();
        let optional = opt_for_fetch.lock().map_err(|_| "opt dep lock".to_string())?.clone();
        let d = snapshot::multiple_accounts(&rpc, &required, &clock)
            .map_err(|e| format!("stage=required_accounts {e}"))?;
        let o = snapshot::multiple_accounts(&rpc, &optional, &clock)
            .map_err(|e| format!("stage=optional_accounts {e}"))?;
        let liability_count = a.accounts.len();
        let bank_count = b.accounts.len();
        let required_count = d.accounts.len();
        let optional_count = o.accounts.len();
        let missing_required_count = d.missing.len();
        let mut accounts = a.accounts;
        accounts.extend(b.accounts);
        accounts.extend(d.accounts);
        accounts.extend(o.accounts);
        // Required dependencies stay fail-closed; missing optional ATAs only disable that candidate.
        let mut slot = a.slot;
        if b.slot != 0 { slot = slot.min(b.slot); }
        if d.slot != 0 { slot = slot.min(d.slot); }
        if o.slot != 0 { slot = slot.min(o.slot); }
        eprintln!(
            "snapshot: done slot={} liability_accounts={} banks={} required={} optional={} missing_required={} total={} elapsed_ms={}",
            slot,
            liability_count,
            bank_count,
            required_count,
            optional_count,
            missing_required_count,
            accounts.len(),
            t0.elapsed().as_millis()
        );
        Ok(SnapshotData { slot, accounts, missing_required: d.missing })
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
    let blockhash = BlockhashCache::new();
    let bh_rpc = RpcClient::new(config::require("P0_RPC_URL")?);
    let bh_cache = blockhash.clone();
    let bh_clock = clock;
    std::thread::Builder::new().name("blockhash".into()).spawn(move || {
        let mut degraded = false;
        loop {
            match bh_cache.refresh(&bh_rpc, bh_clock.now_ns()) {
                Ok(_) => {
                    if degraded {
                        eprintln!("blockhash refresh: recovered");
                        degraded = false;
                    }
                }
                Err(e) => {
                    if !degraded {
                        // [FIX-82] Never print provider URLs/API keys embedded in transport errors.
                        let safe = e.split(" for url ").next().unwrap_or("rpc transport error");
                        eprintln!("blockhash refresh: {}", safe);
                        degraded = true;
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(10));
        }
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
    allowlist.other_programs.insert(Pubkey::from_str(protocol::ASSOCIATED_TOKEN_PROGRAM_ID)?);
    let global_allowed = protocol::receivership_allowed_programs();
    // [FIX-30] Prerequisite-TXs (JIT-Buy): Swap-Programme + System/Token/Token-2022/ATA/ComputeBudget.
    let prereq_allowed: Vec<Pubkey> = [protocol::JUP_SWAP_PROGRAM_ID, protocol::TITAN_SWAP_PROGRAM_ID, protocol::ASSOCIATED_TOKEN_PROGRAM_ID, protocol::COMPUTE_BUDGET_PROGRAM_ID,
        darkbot_project0::receivership::SYSTEM_PROGRAM, "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"].iter().map(|s| Pubkey::from_str(s)).collect::<Result<_, _>>()?;
    let submitter = RpcSubmitter::new(config::require("P0_RPC_URL")?, true);
    let failed_costs = FailedCostMatrix::default(); // alle None → Unknown → nicht handelbar, bis gemessen
    #[cfg(feature = "crate_health")]
    let routes = JupiterV2RouteProvider::new(
        liquidator,
        std::env::var("P0_JUPITER_API_KEY").ok().filter(|s| !s.trim().is_empty()),
        config::parsed_or("P0_MAX_SLIPPAGE_BPS", 0)?,
        config::parsed_or("P0_JUPITER_MAX_ACCOUNTS", 64u8)?,
        true,
        false,
    ).map_err(|e| format!("route provider: {e}"))?;
    #[cfg(not(feature = "crate_health"))]
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
    let mut last_reported_update_total = 0u64; // [FIX-81]
    let mut bootstrap_probe_done = false; // [FIX-84]
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
            IngressEvent::Account(a)
                if st.run_state().is_tradable()
                    && marginfi_account_in_group(&a.data, &group) =>
            {
                let trigger = if a.source == SourceKind::Snapshot { "bootstrap_snapshot_probe" } else { "account_update" };
                Some((a.pubkey, a.t_rx_ns, a.slot, trigger))
            },
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
                #[cfg(feature = "crate_health")]
                if mode == "shadow" && !bootstrap_probe_done {
                    bootstrap_probe_done = true;
                    let mut probes = Vec::new();
                    for (pk, r) in st.iter_owned_by(&program) {
                        if bootstrap_probe_candidate(&r.data, &group) {
                            probes.push(darkbot_solana::ingress::AccountUpdate {
                                pubkey: *pk,
                                owner: r.owner,
                                lamports: r.lamports,
                                data: r.data.clone(),
                                slot: r.slot,
                                write_version: r.write_version,
                                source: SourceKind::Snapshot,
                                t_rx_ns: clock.now_ns(),
                            });
                        }
                    }
                    probes.sort_unstable_by_key(|a| a.pubkey.to_bytes());
                    let mut queued = 0usize;
                    for a in probes {
                        if producer.try_push(IngressEvent::Account(a)).is_ok() {
                            queued += 1;
                        } else {
                            break;
                        }
                    }
                    eprintln!("bootstrap_probe: queued={} candidates from was_liquidatable", queued);
                }
            }
        }
        // [FIX-34][FIX-35] Nach dem Anwenden: zeigt die NEUE Bank-Konfiguration auf nicht abonnierte Accounts?
        #[cfg(feature = "crate_health")]
        if let Some(bk) = updated_bank {
            let view = StateView(&st);
            let required_now = darkbot_project0::health::dependency_keys(&view, &program, &[bk]);
            let optional_now = ata_candidates_for_bank(&view, &program, &bk, &liquidator);
            let new_required: Vec<Pubkey> = required_now.into_iter().filter(|k| !required_set.contains(k)).collect();
            let new_optional: Vec<Pubkey> = optional_now.into_iter().filter(|k| !subscribed.contains(k)).collect();
            if !new_required.is_empty() || !new_optional.is_empty() {
                eprintln!(
                    "dependency set changed: +{} required +{} optional → HALT, resubscribe; Recovery follows StreamReady",
                    new_required.len(),
                    new_optional.len()
                );
                st.halt(darkbot_common::error::HaltReason::StateGap);
                ingress_session.request();
                required_set.extend(new_required.iter().copied());
                optional_set.extend(new_optional.iter().copied());
                required_dependency_keys = required_set.iter().copied().collect();
                optional_dependency_keys = optional_set.iter().copied().collect();
                required_dependency_keys.sort_unstable_by_key(|k| k.to_bytes());
                optional_dependency_keys.sort_unstable_by_key(|k| k.to_bytes());
                subscribed = required_set.union(&optional_set).copied().collect();
                if let Ok(mut g) = dep_shared.lock() { *g = required_dependency_keys.clone(); }
                if let Ok(mut g) = opt_shared.lock() { *g = optional_dependency_keys.clone(); }
                let mut all_keys: Vec<Pubkey> = subscribed.iter().copied().collect();
                all_keys.sort_unstable_by_key(|k| k.to_bytes());
                let sub2 = Subscription {
                    programs: vec![],
                    program_filters: vec![
                        (program, liability_account_filters(&group)),
                        (program, bank_filters(&group)),
                        (program, liquidation_record_filters()),
                    ],
                    accounts: all_keys,
                    slots: true,
                    roots: true,
                    transactions_mentioning: vec![],
                };
                ingress_session = spawn_ingress(sub2, producer.clone(), halt.clone())?;
            }
        }
        if let Some((acct, t_rx, slot, trigger)) = detect {
            // Health über das Crate (Feature crate_health), sonst fail-closed NoHealth → Unavailable-Record.
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let view = StateView(&st);
            #[cfg(feature = "crate_health")]
            let engine = darkbot_project0::health::crate_impl::CrateHealth { state: &view, group };
            #[cfg(not(feature = "crate_health"))]
            let engine = darkbot_project0::health::NoHealth;
            #[cfg(feature = "crate_health")]
            let inventory = |bank_key: &Pubkey| -> u64 {
                let Some(bank) = bank_from_state(&view, &program, bank_key) else { return 0 };
                validated_liquidator_ata(&view, &bank.mint, &liquidator).map(|(_, amount)| amount).unwrap_or(0)
            };
            #[cfg(feature = "crate_health")]
            let ata = |mint: &Pubkey| -> Option<Pubkey> {
                validated_liquidator_ata(&view, mint, &liquidator).map(|(pk, _)| pk)
            };
            #[cfg(feature = "crate_health")]
            let sizing = ProtocolSizingProvider {
                state: &view,
                program,
                group,
                sol_bank,
                inventory: &inventory,
                slippage_bps: config::parsed_or("P0_MAX_SLIPPAGE_BPS", 0).unwrap_or(0),
            };
            #[cfg(feature = "crate_health")]
            let sizer_for_cost = darkbot_project0::sizing::ProtocolSizer {
                state: &view,
                program,
                group,
                headroom_bps: darkbot_project0::sizing::DEFAULT_HEADROOM_BPS,
            };
            #[cfg(feature = "crate_health")]
            let priority_fee_lamports = resources.priority_total_lamports();
            #[cfg(feature = "crate_health")]
            let priority_fee_usd_micro = sizer_for_cost
                .native_amount_value_usd_micro(&sol_bank, priority_fee_lamports, now as i64, slot)
                .unwrap_or(u64::MAX);
            #[cfg(feature = "crate_health")]
            let builder = P0ReceivershipBuilder {
                state: &view,
                program,
                group,
                liquidator,
                global_fee_wallet: match sizer_for_cost.fee_snapshot() {
                    Ok(f) => f.global_fee_wallet,
                    Err(_) => Pubkey::default(),
                },
                sizing: &sizing,
                routes: &routes,
                inventory: &inventory,
                ata: &ata,
                priority_fee_usd_micro,
                jito_tip_usd_micro: 0,
                jito_tip_lamports: 0,
                zero_prefund_qualified: config::parsed_or("P0_ZERO_PREFUND_QUALIFIED", false).unwrap_or(false)
                    || (mode == "shadow" && config::parsed_or("P0_ZERO_PREFUND_SHADOW_PROBE", false).unwrap_or(false)),
                path: Path::Rpc,
            };
            let _ = &view;
            if let Some(day) = ledger.on_account_update(now) { if let Ok(j) = serde_json::to_string(&day) { let _ = writeln!(summaries, "{}", j); } }
            let health_result = engine.health(&acct, now as i64, slot);
            // [FIX-87] Health evidence as structured JSON. Do not hand-build JSON with format!:
            // braces/escaping are error-prone and error strings may contain quotes.
            let line = match &health_result {
                Ok(h) => {
                    ledger.on_health(h.liquidatable, false);
                    serde_json::json!({
                        "account": acct.to_string(),
                        "trigger": trigger,
                        "t_rx_ns": t_rx,
                        "slot": slot,
                        "liquidatable": h.liquidatable,
                        "maint_health_bits": h.maintenance_health,
                        "equity_assets_bits": h.equity_assets,
                        "bad_debt": h.is_bad_debt,
                    }).to_string()
                }
                Err(HealthError::Unavailable) => {
                    ledger.on_health(false, false);
                    serde_json::json!({
                        "account": acct.to_string(),
                        "trigger": trigger,
                        "t_rx_ns": t_rx,
                        "slot": slot,
                        "decision": "Unavailable",
                    }).to_string()
                }
                Err(e) => {
                    ledger.on_health(false, matches!(e, HealthError::Unsupported(_)));
                    serde_json::json!({
                        "account": acct.to_string(),
                        "trigger": trigger,
                        "t_rx_ns": t_rx,
                        "slot": slot,
                        "error": e.to_string(),
                    }).to_string()
                }
            };
            let _ = writeln!(records, "{}", line);
            // [FIX-1] Pipeline (Shadow): Health → Pläne → Economics → Build → Sign → Send-Boundary.
            let mut pipe = Pipeline {
                mode: Mode::Shadow, clock: &clock, health: &engine, builder: &builder, submitter: &submitter,
                safety: &mut safety, blockhash: &blockhash, payer: &payer, allowlist: &allowlist, failed_costs: &failed_costs,
                limits: TxLimits::default(), resources, min_profit, blockhash_max_age_ns: 30_000_000_000, p7_passed: false,
                freshness, tip_account: None, global_allowed: global_allowed.clone(), prereq_allowed: prereq_allowed.clone(), slot_watch: &slot_watch,
            };
            let mut rec = pipe.evaluate(acct, trigger, t_rx, now, slot, st.run_state().is_tradable());
            if let Ok(h) = &health_result {
                // [FIX-91] Bankruptcy/no-collateral is a protocol state, not a routing/sizing
                // failure. Reclassify independently of the stage that first reported "no plan".
                if h.is_bad_debt && h.equity_assets <= 0 {
                    rec.loss = Some(darkbot_runner::record::LossReason::Bankruptcy);
                    rec.reason = Some("bad debt with zero equity assets: bankruptcy/no-collateral case, not a normal receivership opportunity".into());
                }
            }
            if rec.reason.as_deref().map(|s| s.contains("ReserveStale")).unwrap_or(false) {
                rec.loss = Some(darkbot_runner::record::LossReason::IntegrationRefresh);
                rec.reason = Some("integration reserve stale: required reserve refresh prerequisite missing before exact sizing".into());
            }
            if let Ok(j) = serde_json::to_string(&rec) { let _ = writeln!(records, "{}", j); }
            // [M3] Ledger + [M2] Watchlist (nur Shadow-relevante Entscheidungen).
            let sim_ok = match rec.decision {
                Decision::ShadowReady | Decision::ShadowTechnicalReady => Some(true),
                Decision::ShadowSimFailed | Decision::ShadowTechnicalSimFailed => Some(false),
                _ => None,
            };
            ledger.on_record(&rec, sim_ok);
            // [FIX-43][FIX-71] Nur wirtschaftlich bekannter ShadowReady darf als virtueller Win/PnL zählen.
            // Technische Probes liefern ausschließlich Build/Sign/Simulation-Evidence.
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
        let update_total = counts.iter().sum::<u64>();
        if update_total > 0 && update_total % 1000 == 0 && update_total != last_reported_update_total {
            last_reported_update_total = update_total;
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

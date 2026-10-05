//! `p0-longtail`: Long-Tail-Runner. Konfiguration ausschließlich aus Env (kein Wert im Code):
//!   P0_WS_URL, P0_RPC_URL, P0_PROGRAM_ID, P0_GROUP_ID (aus P1A-Manifest), P0_KEYPAIR_PATH,
//!   P0_MODE=shadow|live, P0_MIN_PROFIT, P0_MAX_*, P0_RECORD_PATH
//! Ohne Health-Engine (Feature crate_health) läuft der Prozess im Shadow-Modus bis Detection
//! und schreibt `Unavailable`-Records — damit ist die Kette messbar, bevor P3 angebunden ist.
//!
//! Status: UNTESTED / VERIFY. Threads: Ingress (WSS) → bounded channel → State/Detection-Loop;
//! Blockhash, Winner-Matcher und Submission-Reconciliation separat. Kein Remote-RPC im Entscheidungs-Loop.

use darkbot_common::channel::{bounded, ChannelError};
use darkbot_common::config;
use darkbot_common::lifecycle::Shutdown;
use darkbot_common::timing::Clock;
use darkbot_project0::health::{AccountSource, HealthEngine, HealthError};
#[cfg(feature = "crate_health")]
use darkbot_project0::health::crate_impl::bank_observation_keys;
#[cfg(feature = "crate_health")]
use darkbot_project0::index::{ReverseIndex, ThresholdIndex};
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
use darkbot_runner::pending::PendingSubmission;
use darkbot_runner::record::Decision;
use darkbot_runner::shadow_ledger::ShadowLedger;
use darkbot_runner::winner::{race_outcome, RaceOutcome};
use darkbot_runner::remote_workers::{
    OraclePriceProjectionWork, ProjectionWork, RemoteWorkers, ReconcileWork, WinnerWatch,
};
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
use solana_sdk::{account::Account, hash::{hash, Hash}, pubkey::Pubkey};
#[cfg(feature = "crate_health")]
use marginfi_type_crate::{
    constants::{ASSET_TAG_DEFAULT, ASSET_TAG_SOL},
    types::{BalanceSide, Bank, BankOperationalState, MarginfiAccount},
};
#[cfg(feature = "crate_health")]
use rayon::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::str::FromStr;
use std::time::Duration;

/// S2-State als AccountSource für die Health-Engine (keine Kopie, keine RPC).
struct StateView<'a>(&'a StateCore);
impl<'a> AccountSource for StateView<'a> {
    fn account(&self, key: &solana_sdk::pubkey::Pubkey) -> Option<(solana_sdk::pubkey::Pubkey, u64, &[u8])> {
        self.0.get(key).map(|r| (r.owner, r.lamports, r.data.as_slice()))
    }
}


#[cfg(feature = "crate_health")]
struct CachedProjection {
    base_account_hash: Hash,
    projection_slot: u64,
    projection_unix_s: i64,
    accounts: Vec<(Pubkey, Account)>,
}

#[cfg(feature = "crate_health")]
type DetectWork = (Pubkey, u64, u64, &'static str);

// [FIX-121][R6] Qualification targets wait as unsigned identities, not signed RPC jobs.
// This timestamp is the start of an explicit synthetic probe; real ingress timestamps
// and projection latency remain untouched and are still subject to freshness guards.
#[cfg(feature = "crate_health")]
fn next_integration_probe(
    queue: &mut VecDeque<(Pubkey, u64)>,
    projection_busy: bool,
    now_ns: u64,
) -> Option<DetectWork> {
    if projection_busy {
        return None;
    }
    queue.pop_front().map(|(account, slot)|
        (account, now_ns, slot, "integration_target_probe"))
}

#[cfg(feature = "crate_health")]
fn next_oracle_detect(
    order: &mut VecDeque<Pubkey>,
    pending: &mut HashMap<Pubkey, DetectWork>,
) -> Option<DetectWork> {
    while let Some(account) = order.pop_front() {
        if let Some(work) = pending.remove(&account) {
            return Some(work);
        }
    }
    None
}

#[cfg(feature = "crate_health")]
fn enqueue_oracle_accounts(
    accounts: impl IntoIterator<Item = Pubkey>,
    t_rx_ns: u64,
    event_slot: u64,
    trigger: &'static str,
    order: &mut VecDeque<Pubkey>,
    pending: &mut HashMap<Pubkey, DetectWork>,
    enqueued: &mut u64,
    coalesced: &mut u64,
    max_pending: &mut usize,
) -> usize {
    let mut unique = HashSet::new();
    unique.extend(accounts);
    let count = unique.len();
    let mut accounts: Vec<Pubkey> = unique.into_iter().collect();
    accounts.sort_unstable_by_key(|pk| pk.to_bytes());
    for account in accounts {
        let work = (account, t_rx_ns, event_slot, trigger);
        if let Some(existing) = pending.get_mut(&account) {
            *existing = work;
            *coalesced = coalesced.saturating_add(1);
        } else {
            order.push_back(account);
            pending.insert(account, work);
            *enqueued = enqueued.saturating_add(1);
        }
    }
    *max_pending = (*max_pending).max(pending.len());
    count
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
fn active_banks_in_group_account(data: &[u8], group: &Pubkey) -> Option<Vec<Pubkey>> {
    if classify_account(data) != AccountType::MarginfiAccount {
        return None;
    }
    let need = 8 + std::mem::size_of::<MarginfiAccount>();
    if data.len() < need {
        return None;
    }
    let account = bytemuck::try_from_bytes::<MarginfiAccount>(&data[8..need]).ok()?;
    if account.group != *group {
        return None;
    }
    let mut banks: Vec<Pubkey> = account
        .lending_account
        .balances
        .iter()
        .filter(|b| b.is_active())
        .map(|b| b.bank_pk)
        .collect();
    banks.sort_unstable_by_key(|pk| pk.to_bytes());
    banks.dedup();
    Some(banks)
}

#[cfg(feature = "crate_health")]
fn liability_account_in_group(data: &[u8], group: &Pubkey) -> bool {
    if classify_account(data) != AccountType::MarginfiAccount {
        return false;
    }
    let need = 8 + std::mem::size_of::<MarginfiAccount>();
    if data.len() < need {
        return false;
    }
    bytemuck::try_from_bytes::<MarginfiAccount>(&data[8..need])
        .map(|account| {
            account.group == *group
                && account
                    .lending_account
                    .balances
                    .iter()
                    .any(|b| b.is_active() && matches!(b.get_side(), Some(BalanceSide::Liabilities)))
        })
        .unwrap_or(false)
}

#[cfg(feature = "crate_health")]
fn rebuild_reverse_index(
    st: &StateCore,
    program: &Pubkey,
    group: &Pubkey,
) -> (ReverseIndex, usize, usize, usize) {
    let view = StateView(st);
    let mut index = ReverseIndex::new();
    let mut bank_count = 0usize;
    let mut account_count = 0usize;
    let mut unsupported_banks = 0usize;

    for (pk, record) in st.iter_owned_by(program) {
        if classify_account(&record.data) != AccountType::Bank {
            continue;
        }
        let Some(bank) = bank_from_state(&view, program, pk) else {
            continue;
        };
        if bank.group != *group {
            continue;
        }
        bank_count += 1;
        match bank_observation_keys(&bank) {
            Ok(_) => {
                // [R127b] Long-Tail admission must react to every subscribed dependency that
                // can change effective bank pricing/health, not only the primary oracle key.
                // Reuse the same source-backed dependency derivation as snapshot/subscription.
                let deps = darkbot_project0::health::dependency_keys(&view, program, &[*pk]);
                index.set_bank_oracles(*pk, &deps);
            }
            Err(_) => {
                unsupported_banks += 1;
                index.set_bank_oracles(*pk, &[]);
            }
        }
    }

    for (pk, record) in st.iter_owned_by(program) {
        if let Some(banks) = active_banks_in_group_account(&record.data, group) {
            index.set_account_banks(*pk, &banks);
            account_count += 1;
        }
    }

    (index, bank_count, account_count, unsupported_banks)
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
        // [FIX-130] Do not pin Bank account size in RPC/WSS filters. 0.1.12 grows the
        // Bank payload from 1856 to 3904 bytes while preserving the legacy prefix.
        // Exact supported lengths remain enforced by protocol classification/decoding.
        serde_json::json!({"memcmp": {
            "offset": 0,
            "bytes": bs58::encode(protocol::DISC_BANK).into_string()
        }}),
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

    // [FIX-131] Loader-v3 upgrades mutate the ProgramData account, not the executable
    // Program account. Pin its current content hash at startup and subscribe it as a required
    // dependency. Any later mismatch is a latched PROGRAM_UPGRADE halt; reconnect/recovery
    // must never auto-promote new protocol code.
    let upgradeable_loader = Pubkey::from_str("BPFLoaderUpgradeab1e11111111111111111111111")?;
    let program_data =
        Pubkey::find_program_address(&[program.as_ref()], &upgradeable_loader).0;
    let program_data_account = dep_rpc
        .get_account_with_commitment(&program_data, CommitmentConfig::confirmed())?
        .value
        .ok_or_else(|| format!("PROGRAM_DATA_NOT_FOUND: {}", program_data))?;
    if program_data_account.owner != upgradeable_loader {
        return Err(format!(
            "PROGRAM_DATA_OWNER_MISMATCH: {} owner={} expected={}",
            program_data, program_data_account.owner, upgradeable_loader
        ).into());
    }
    let program_data_baseline_hash = hash(&program_data_account.data);
    eprintln!(
        "program_upgrade_watch: program={} program_data={} baseline_hash={}",
        program, program_data, program_data_baseline_hash
    );

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

    // ProgramData is required in every build: protocol upgrades are a lifecycle concern,
    // independent of whether crate_health is enabled.
    required_dependency_keys.push(program_data);
    required_dependency_keys.sort_unstable_by_key(|k| k.to_bytes());
    required_dependency_keys.dedup();

    #[cfg(feature = "crate_health")]
    let mut optional_dependency_keys: Vec<Pubkey> = bank_keys.iter()
        .flat_map(|bk| ata_candidates_for_bank(&StateView(&pre_state), &program, bk, &liquidator))
        .collect();
    #[cfg(not(feature = "crate_health"))]
    let mut optional_dependency_keys: Vec<Pubkey> = Vec::new();
    #[cfg(feature = "crate_health")]
    optional_dependency_keys.extend(darkbot_runner::lookup_tables::published_keys(&group));
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
    let mut subscribed: HashSet<Pubkey> = subscription_keys.iter().copied().collect();
    let mut required_set: HashSet<Pubkey> = required_dependency_keys.iter().copied().collect();
    let mut optional_set: HashSet<Pubkey> = optional_dependency_keys.iter().copied().collect();
    let mut ingress_session = spawn_ingress(sub.clone(), producer.clone(), halt.clone())?;

    // State + Recovery (Snapshot via RPC, außerhalb des Decision-Loops aufgerufen).
    let rpc = RpcClient::new(rpc_url.clone());
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

    #[cfg(feature = "crate_health")]
    let integration_probe_enabled: bool =
        config::parsed_or("P0_INTEGRATION_PROBE", false)?;
    #[cfg(feature = "crate_health")]
    let integration_probe_limit_per_kind: usize =
        config::parsed_or("P0_INTEGRATION_PROBE_LIMIT_PER_KIND", 8)?;
    #[cfg(feature = "crate_health")]
    if integration_probe_enabled && integration_probe_limit_per_kind == 0 {
        return Err("P0_INTEGRATION_PROBE_LIMIT_PER_KIND must be > 0 when probe is enabled".into());
    }

    // [FIX-108][R4] All remote winner/reconciliation RPC is owned by bounded workers.
    let remote_worker_capacity: usize = config::parsed_or("P0_REMOTE_WORKER_BUFFER", 256)?;
    if remote_worker_capacity == 0 {
        return Err("P0_REMOTE_WORKER_BUFFER must be > 0".into());
    }
    let remote = RemoteWorkers::start(
        rpc_url.clone(),
        program,
        &shutdown,
        remote_worker_capacity,
    )?;
    let mut pending: Vec<ReconcileWork> = Vec::new();

    #[cfg(feature = "crate_health")]
    let mut projection_cache: HashMap<Pubkey, CachedProjection> = HashMap::new();
    #[cfg(feature = "crate_health")]
    let mut projection_pending: HashSet<Pubkey> = HashSet::new();
    #[cfg(feature = "crate_health")]
    let mut detect_queue: VecDeque<DetectWork> = VecDeque::new();
    #[cfg(feature = "crate_health")]
    let mut oracle_detect_order: VecDeque<Pubkey> = VecDeque::new();
    #[cfg(feature = "crate_health")]
    let mut oracle_detect_pending: HashMap<Pubkey, DetectWork> = HashMap::new();
    #[cfg(feature = "crate_health")]
    let mut oracle_dependency_updates: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut oracle_candidate_hits: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut oracle_enqueued: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut oracle_coalesced: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut oracle_price_fallbacks: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut oracle_max_pending: usize = 0;
    #[cfg(feature = "crate_health")]
    let mut oracle_bank_projection_pending: HashMap<Pubkey, (u64, u64)> = HashMap::new();
    #[cfg(feature = "crate_health")]
    let mut oracle_bank_projection_submitted: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut oracle_bank_projection_ready: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut oracle_bank_projection_failed: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut oracle_bank_projection_stale: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut integration_probe_expected: HashSet<Pubkey> = HashSet::new();
    #[cfg(feature = "crate_health")]
    let mut integration_probe_queue: VecDeque<(Pubkey, u64)> = VecDeque::new();
    #[cfg(feature = "crate_health")]
    let mut integration_probe_terminal: HashSet<Pubkey> = HashSet::new();
    #[cfg(feature = "crate_health")]
    let mut integration_probe_projection_ready = 0usize;
    #[cfg(feature = "crate_health")]
    let mut integration_probe_projection_failed = 0usize;
    #[cfg(feature = "crate_health")]
    let mut integration_probe_projection_stale = 0usize;
    #[cfg(feature = "crate_health")]
    let mut integration_probe_completion_reported = false;

    // [M2][M3] Shadow-Evidence: Kandidaten-Watchlist für den Winner-Vergleich und Tages-Ledger.
    let mut ledger = ShadowLedger::new();
    let mut watch: Vec<WinnerWatch> = Vec::new();
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
    let mut program_upgrade_latched = false; // [FIX-131]
    #[cfg(feature = "crate_health")]
    let mut reverse_index = ReverseIndex::new(); // [FIX-133a]
    #[cfg(feature = "crate_health")]
    let mut threshold_index = ThresholdIndex::new(); // [FIX-133d]
    #[cfg(feature = "crate_health")]
    let mut threshold_fallback: HashSet<Pubkey> = HashSet::new();
    #[cfg(feature = "crate_health")]
    let mut threshold_bootstrap: VecDeque<Pubkey> = VecDeque::new();
    #[cfg(feature = "crate_health")]
    let threshold_bootstrap_batch: usize =
        config::parsed_or("P0_THRESHOLD_BOOTSTRAP_BATCH", 16)?;
    #[cfg(feature = "crate_health")]
    if threshold_bootstrap_batch == 0 {
        return Err("P0_THRESHOLD_BOOTSTRAP_BATCH must be > 0".into());
    }
    #[cfg(feature = "crate_health")]
    let use_threshold_trigger: bool =
        config::parsed_or("P0_USE_THRESHOLD_TRIGGER", false)?;
    #[cfg(feature = "crate_health")]
    let mut threshold_ready = !use_threshold_trigger;
    #[cfg(feature = "crate_health")]
    let mut health_sweep_checks: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut health_sweep_healthy: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut health_sweep_liquidatable: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut health_sweep_errors: u64 = 0;
    // [R127c] Dependency/Bank updates are first coalesced at Bank level. Only after the
    // current ingress burst is drained (or this hard event cap is reached) do we fan out
    // once to unique accounts and run exact Health in parallel.
    #[cfg(feature = "crate_health")]
    let sweep_batch_max_events: usize =
        config::parsed_or("P0_SWEEP_BATCH_MAX_EVENTS", 64)?;
    #[cfg(feature = "crate_health")]
    if sweep_batch_max_events == 0 {
        return Err("P0_SWEEP_BATCH_MAX_EVENTS must be > 0".into());
    }
    #[cfg(feature = "crate_health")]
    let sweep_batch_max_age_ns: u64 =
        config::parsed_or("P0_SWEEP_BATCH_MAX_AGE_NS", 5_000_000)?;
    #[cfg(feature = "crate_health")]
    if sweep_batch_max_age_ns == 0 {
        return Err("P0_SWEEP_BATCH_MAX_AGE_NS must be > 0".into());
    }
    #[cfg(feature = "crate_health")]
    let mut sweep_dirty_banks: HashMap<Pubkey, (u64, u64)> = HashMap::new();
    #[cfg(feature = "crate_health")]
    let mut sweep_batch_events: usize = 0;
    #[cfg(feature = "crate_health")]
    let mut sweep_batch_started_ns: Option<u64> = None;
    #[cfg(feature = "crate_health")]
    let mut sweep_batches: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut sweep_account_refs: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut sweep_unique_accounts: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut sweep_forwarded: u64 = 0;
    #[cfg(feature = "crate_health")]
    let mut sweep_max_unique_batch: usize = 0;
    #[cfg(feature = "crate_health")]
    let mut sweep_max_dirty_banks: usize = 0;
    // [R126b] Preserve canonical ingress under detection bursts. When both ingress and
    // detection work are available, drain at most this many ingress events before forcing
    // one detection item. This prevents either side from starving the other.
    #[cfg(feature = "crate_health")]
    let ingress_burst_before_detect: usize =
        config::parsed_or("P0_INGRESS_BURST_BEFORE_DETECT", 64)?;
    #[cfg(feature = "crate_health")]
    if ingress_burst_before_detect == 0 {
        return Err("P0_INGRESS_BURST_BEFORE_DETECT must be > 0".into());
    }
    #[cfg(feature = "crate_health")]
    let mut ingress_since_detect: usize = 0;
    loop {
        if shutdown.is_requested() {
            break;
        }
        if halt.is_raised() {
            st.halt(darkbot_common::error::HaltReason::FeedGap);
        }

        #[cfg(feature = "crate_health")]
        if use_threshold_trigger
            && st.run_state().is_tradable()
            && !threshold_ready
            && !threshold_bootstrap.is_empty()
        {
            let now_s = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let slot = slot_watch.get();
            let view = StateView(&st);
            let engine = darkbot_project0::health::crate_impl::CrateHealth {
                state: &view,
                group,
            };
            for _ in 0..threshold_bootstrap_batch {
                let Some(account) = threshold_bootstrap.pop_front() else { break };
                match engine.thresholds(&account, now_s, slot) {
                    Ok(t) => {
                        threshold_index.set_account(account, &t.thresholds);
                        threshold_fallback.remove(&account);
                    }
                    Err(e) => {
                        threshold_index.remove_account(&account);
                        threshold_fallback.insert(account);
                        eprintln!(
                            "threshold_bootstrap_fallback: account={} error={}",
                            account, e
                        );
                    }
                }
            }
            if threshold_bootstrap.is_empty() {
                threshold_ready = true;
                eprintln!(
                    "threshold_index_ready: tracked={} fallback={}",
                    threshold_index.tracked_accounts(),
                    threshold_fallback.len()
                );
            }
        }

        // [FIX-108][R4] Apply completed remote work on the state thread, but never perform RPC here.
        let worker_now_s = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        while let Some(done) = remote.winner_results.try_pop() {
            let mut w = done.watch;
            match done.result {
                Ok(Some(win)) => {
                    let out = race_outcome(
                        win.slot,
                        w.rx_slot,
                        w.detect_slot,
                        w.sign_slot,
                        w.shadow_ready,
                        w.our_loss,
                    );
                    match out {
                        RaceOutcome::ReadyBeforeWinner => ledger.on_race(true, None, w.expected_net),
                        RaceOutcome::SameSlotUnknown => ledger.on_same_slot(),
                        RaceOutcome::Loss(l) => ledger.on_race(false, Some(l), None),
                    }
                    let line = serde_json::json!({
                        "race": {
                            "account": w.account.to_string(),
                            "winner": win,
                            "our_sign_slot": w.sign_slot,
                            "outcome": out,
                            "expected_net": w.expected_net
                        }
                    });
                    let _ = writeln!(records, "{}", line);
                }
                Ok(None) if worker_now_s.saturating_sub(w.created_unix_s) < 900 => {
                    w.next_check_unix_s = worker_now_s.saturating_add(5);
                    watch.push(w);
                }
                Ok(None) => {}
                Err(_) => {
                    w.next_check_unix_s = worker_now_s.saturating_add(5);
                    watch.push(w);
                }
            }
        }

        while let Some(done) = remote.reconcile_results.try_pop() {
            let mut work = done.work;
            match done.result {
                Some(r) => {
                    safety.record_outcome(
                        worker_now_s,
                        r.landed_success,
                        r.realized_pnl,
                        work.submission.exposure,
                        0,
                    );
                    if let Ok(j) = serde_json::to_string(&r.record) {
                        let _ = writeln!(records, "{}", j);
                    }
                }
                None => {
                    work.next_check_unix_s = worker_now_s.saturating_add(1);
                    pending.push(work);
                }
            }
        }


        #[cfg(feature = "crate_health")]
        while let Some(done) = remote.oracle_price_projection_results.try_pop() {
            let bank = done.bank;
            let base_slot = done.base_slot;
            let base_bank_hash = done.base_bank_hash;
            let result = done.result;
            let latest = oracle_bank_projection_pending.remove(&bank);
            let all_accounts: Vec<Pubkey> =
                reverse_index.accounts_for_bank(&bank).copied().collect();
            let fallback_all = |reason: &str,
                                oracle_price_fallbacks: &mut u64,
                                oracle_candidate_hits: &mut u64,
                                oracle_enqueued: &mut u64,
                                oracle_coalesced: &mut u64,
                                oracle_max_pending: &mut usize,
                                oracle_detect_order: &mut VecDeque<Pubkey>,
                                oracle_detect_pending: &mut HashMap<Pubkey, DetectWork>| {
                *oracle_price_fallbacks = oracle_price_fallbacks.saturating_add(1);
                let (t_rx_ns, event_slot) = latest.unwrap_or((clock.now_ns(), base_slot));
                let count = enqueue_oracle_accounts(
                    all_accounts.iter().copied(),
                    t_rx_ns,
                    event_slot,
                    "oracle_threshold",
                    oracle_detect_order,
                    oracle_detect_pending,
                    oracle_enqueued,
                    oracle_coalesced,
                    oracle_max_pending,
                );
                *oracle_candidate_hits = oracle_candidate_hits.saturating_add(count as u64);
                eprintln!(
                    "oracle_bank_projection_fallback: bank={} reason={} accounts={}",
                    bank, reason, count
                );
            };

            match result {
                Ok((projection_slot, projection_unix_s, accounts, _units)) => {
                    let canonical_matches = st
                        .get(&done.bank)
                        .map(|r| r.slot == base_slot && hash(&r.data) == base_bank_hash)
                        .unwrap_or(false);
                    let newest_event_slot = latest.map(|(_, slot)| slot).unwrap_or(base_slot);
                    if !canonical_matches || projection_slot < newest_event_slot {
                        oracle_bank_projection_stale =
                            oracle_bank_projection_stale.saturating_add(1);
                        fallback_all(
                            if !canonical_matches {
                                "canonical bank changed"
                            } else {
                                "projection older than newest dependency update"
                            },
                            &mut oracle_price_fallbacks,
                            &mut oracle_candidate_hits,
                            &mut oracle_enqueued,
                            &mut oracle_coalesced,
                            &mut oracle_max_pending,
                            &mut oracle_detect_order,
                            &mut oracle_detect_pending,
                        );
                        continue;
                    }

                    let view = StateView(&st);
                    match darkbot_project0::projection::ProjectedState::new(
                        &view,
                        projection_slot,
                        projection_unix_s,
                        accounts,
                    ) {
                        Ok(projected) => {
                            let engine = darkbot_project0::health::crate_impl::CrateHealth {
                                state: &projected,
                                group,
                            };
                            match engine.maintenance_risk_price_bits(
                                &bank,
                                projection_unix_s,
                                projection_slot,
                            ) {
                                Ok((asset_price, liability_price)) => {
                                    let mut candidates: HashSet<Pubkey> = threshold_index
                                        .candidates(
                                            &bank,
                                            asset_price,
                                            liability_price,
                                            0,
                                        )
                                        .into_iter()
                                        .collect();
                                    candidates.extend(
                                        all_accounts
                                            .iter()
                                            .copied()
                                            .filter(|a| threshold_fallback.contains(a)),
                                    );
                                    let (t_rx_ns, event_slot) =
                                        latest.unwrap_or((clock.now_ns(), projection_slot));
                                    let count = enqueue_oracle_accounts(
                                        candidates,
                                        t_rx_ns,
                                        event_slot,
                                        "oracle_threshold",
                                        &mut oracle_detect_order,
                                        &mut oracle_detect_pending,
                                        &mut oracle_enqueued,
                                        &mut oracle_coalesced,
                                        &mut oracle_max_pending,
                                    );
                                    oracle_candidate_hits =
                                        oracle_candidate_hits.saturating_add(count as u64);
                                    oracle_bank_projection_ready =
                                        oracle_bank_projection_ready.saturating_add(1);
                                }
                                Err(e) => {
                                    oracle_bank_projection_failed =
                                        oracle_bank_projection_failed.saturating_add(1);
                                    fallback_all(
                                        &format!("projected price error: {e}"),
                                        &mut oracle_price_fallbacks,
                                        &mut oracle_candidate_hits,
                                        &mut oracle_enqueued,
                                        &mut oracle_coalesced,
                                        &mut oracle_max_pending,
                                        &mut oracle_detect_order,
                                        &mut oracle_detect_pending,
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            oracle_bank_projection_failed =
                                oracle_bank_projection_failed.saturating_add(1);
                            fallback_all(
                                &format!("projected state invalid: {e}"),
                                &mut oracle_price_fallbacks,
                                &mut oracle_candidate_hits,
                                &mut oracle_enqueued,
                                &mut oracle_coalesced,
                                &mut oracle_max_pending,
                                &mut oracle_detect_order,
                                &mut oracle_detect_pending,
                            );
                        }
                    }
                }
                Err(e) => {
                    oracle_bank_projection_failed =
                        oracle_bank_projection_failed.saturating_add(1);
                    fallback_all(
                        &format!("simulation failed: {e}"),
                        &mut oracle_price_fallbacks,
                        &mut oracle_candidate_hits,
                        &mut oracle_enqueued,
                        &mut oracle_coalesced,
                        &mut oracle_max_pending,
                        &mut oracle_detect_order,
                        &mut oracle_detect_pending,
                    );
                }
            }
        }

        #[cfg(feature = "crate_health")]
        while let Some(done) = remote.projection_results.try_pop() {
            projection_pending.remove(&done.account);
            let targeted_probe_result = integration_probe_expected.contains(&done.account);
            let mut targeted_status: Option<&'static str> = None;
            match done.result {
                Ok((projection_slot, projection_unix_s, accounts, units)) => {
                    let canonical = st.get(&done.account);
                    let canonical_matches = canonical
                        .map(|r| r.slot == done.base_slot && hash(&r.data) == done.base_account_hash)
                        .unwrap_or(false);

                    // [FIX-116b] The candidate MarginfiAccount is readonly during the external
                    // refresh prefix and is therefore intentionally not requested as a post-account.
                    // Freshness is proven against canonical StateCore via exact slot+hash identity.
                    if canonical_matches {
                        if targeted_probe_result {
                            targeted_status = Some("ready");
                        }
                        projection_cache.insert(
                            done.account,
                            CachedProjection {
                                base_account_hash: done.base_account_hash,
                                projection_slot,
                                projection_unix_s,
                                accounts,
                            },
                        );
                        detect_queue.push_front((
                            done.account,
                            done.t_rx_ns,
                            done.base_slot,
                            "integration_projection",
                        ));
                        let (integration_kinds, pricing_integration_kinds) = {
                            let view = StateView(&st);
                            let withdraw = darkbot_project0::integration::kinds_for_account(
                                &view,
                                &program,
                                &done.account,
                            )
                            .unwrap_or_default()
                            .into_iter()
                            .map(darkbot_project0::integration::kind_name)
                            .collect::<Vec<_>>();
                            let pricing = darkbot_project0::integration::pricing_kinds_for_account(
                                &view,
                                &program,
                                &done.account,
                            )
                            .unwrap_or_default()
                            .into_iter()
                            .map(darkbot_project0::integration::kind_name)
                            .collect::<Vec<_>>();
                            (withdraw, pricing)
                        };
                        let line = serde_json::json!({
                            "projection": {
                                "account": done.account.to_string(),
                                "base_slot": done.base_slot,
                                "projection_slot": projection_slot,
                                "projection_unix_s": projection_unix_s,
                                "status": "ready",
                                "integration_kinds": integration_kinds,
                                "pricing_integration_kinds": pricing_integration_kinds,
                                "simulated_cu": units
                            }
                        });
                        let _ = writeln!(records, "{}", line);
                    } else {
                        if targeted_probe_result {
                            targeted_status = Some("stale");
                        }
                        projection_cache.remove(&done.account);
                        let line = serde_json::json!({
                            "projection": {
                                "account": done.account.to_string(),
                                "base_slot": done.base_slot,
                                "projection_slot": projection_slot,
                                "status": "stale",
                                "canonical_matches": canonical_matches
                            }
                        });
                        let _ = writeln!(records, "{}", line);
                    }
                }
                Err(error) => {
                    if targeted_probe_result {
                        targeted_status = Some("failed");
                    }
                    projection_cache.remove(&done.account);
                    let line = serde_json::json!({
                        "projection": {
                            "account": done.account.to_string(),
                            "base_slot": done.base_slot,
                            "status": "failed",
                            "error": error
                        }
                    });
                    let _ = writeln!(records, "{}", line);
                }
            }

            if targeted_probe_result && integration_probe_terminal.insert(done.account) {
                match targeted_status {
                    Some("ready") => integration_probe_projection_ready += 1,
                    Some("failed") => integration_probe_projection_failed += 1,
                    Some("stale") => integration_probe_projection_stale += 1,
                    _ => {}
                }
            }
            if integration_probe_enabled
                && !integration_probe_completion_reported
                && !integration_probe_expected.is_empty()
                && integration_probe_terminal.len() == integration_probe_expected.len()
            {
                integration_probe_completion_reported = true;
                eprintln!(
                    "integration_target_projection_complete: total={} ready={} failed={} stale={}",
                    integration_probe_expected.len(),
                    integration_probe_projection_ready,
                    integration_probe_projection_failed,
                    integration_probe_projection_stale
                );
            }
        }

        // Bounded non-blocking dispatch. Full queues retain the work item for the next loop.
        if !watch.is_empty() {
            let mut keep = Vec::with_capacity(watch.len());
            for w in watch.drain(..) {
                if worker_now_s < w.next_check_unix_s {
                    keep.push(w);
                    continue;
                }
                match remote.winner_jobs.try_push(w) {
                    Ok(()) => {}
                    Err(ChannelError::Full(w)) => keep.push(w),
                    Err(ChannelError::Disconnected(_)) => {
                        return Err("winner RPC worker disconnected".into());
                    }
                }
            }
            watch = keep;
        }

        if !pending.is_empty() {
            let mut keep = Vec::with_capacity(pending.len());
            for p in pending.drain(..) {
                if worker_now_s < p.next_check_unix_s {
                    keep.push(p);
                    continue;
                }
                match remote.reconcile_jobs.try_push(p) {
                    Ok(()) => {}
                    Err(ChannelError::Full(p)) => keep.push(p),
                    Err(ChannelError::Disconnected(_)) => {
                        return Err("reconcile RPC worker disconnected".into());
                    }
                }
            }
            pending = keep;
        }

        // [R127c] Flush the accumulated Bank set only after an ingress burst is drained,
        // or after a bounded number of dependency events. This mirrors eva01's "drain updates,
        // dedupe affected accounts, then evaluate" behavior and prevents per-update fanout.
        #[cfg(feature = "crate_health")]
        if !use_threshold_trigger
            && st.run_state().is_tradable()
            && !sweep_dirty_banks.is_empty()
        {
            let batch_age_ns = sweep_batch_started_ns
                .map(|t| clock.now_ns().saturating_sub(t))
                .unwrap_or(0);
            if sweep_batch_events >= sweep_batch_max_events
                || batch_age_ns >= sweep_batch_max_age_ns
            {
                let dirty = std::mem::take(&mut sweep_dirty_banks);
                sweep_batch_events = 0;
                sweep_batch_started_ns = None;
                sweep_batches = sweep_batches.saturating_add(1);

                let mut account_work: HashMap<Pubkey, (u64, u64)> = HashMap::new();
                let mut refs = 0usize;
                for (bank, (t_rx_ns, event_slot)) in dirty {
                    for account in reverse_index.accounts_for_bank(&bank).copied() {
                        refs = refs.saturating_add(1);
                        account_work
                            .entry(account)
                            .and_modify(|w| {
                                if event_slot > w.1 || (event_slot == w.1 && t_rx_ns > w.0) {
                                    *w = (t_rx_ns, event_slot);
                                }
                            })
                            .or_insert((t_rx_ns, event_slot));
                    }
                }

                let unique = account_work.len();
                sweep_account_refs = sweep_account_refs.saturating_add(refs as u64);
                sweep_unique_accounts =
                    sweep_unique_accounts.saturating_add(unique as u64);
                sweep_max_unique_batch = sweep_max_unique_batch.max(unique);
                oracle_candidate_hits =
                    oracle_candidate_hits.saturating_add(unique as u64);
                health_sweep_checks =
                    health_sweep_checks.saturating_add(unique as u64);

                let now_s = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let evaluation_slot = slot_watch.get();
                let view = StateView(&st);

                // CPU-only and independent per account. No RPC, no state mutation.
                let evaluated: Vec<(Pubkey, u64, u64, u8)> = account_work
                    .into_par_iter()
                    .map(|(account, (t_rx_ns, event_slot))| {
                        let engine = darkbot_project0::health::crate_impl::CrateHealth {
                            state: &view,
                            group,
                        };
                        let status = match engine.health(&account, now_s, evaluation_slot) {
                            Ok(h) if !h.liquidatable => 0u8,
                            Ok(_) => 1u8,
                            Err(_) => 2u8,
                        };
                        (account, t_rx_ns, event_slot, status)
                    })
                    .collect();

                for (account, t_rx_ns, event_slot, status) in evaluated {
                    match status {
                        0 => {
                            health_sweep_healthy =
                                health_sweep_healthy.saturating_add(1);
                        }
                        1 => {
                            health_sweep_liquidatable =
                                health_sweep_liquidatable.saturating_add(1);
                            sweep_forwarded = sweep_forwarded.saturating_add(1);
                            let _ = enqueue_oracle_accounts(
                                [account],
                                t_rx_ns,
                                event_slot,
                                "oracle_health_ready",
                                &mut oracle_detect_order,
                                &mut oracle_detect_pending,
                                &mut oracle_enqueued,
                                &mut oracle_coalesced,
                                &mut oracle_max_pending,
                            );
                        }
                        _ => {
                            // Fail closed: stale/unknown Health must reach the existing
                            // integration refresh/projection path, not disappear as "healthy".
                            health_sweep_errors =
                                health_sweep_errors.saturating_add(1);
                            sweep_forwarded = sweep_forwarded.saturating_add(1);
                            let _ = enqueue_oracle_accounts(
                                [account],
                                t_rx_ns,
                                event_slot,
                                "oracle_health_error",
                                &mut oracle_detect_order,
                                &mut oracle_detect_pending,
                                &mut oracle_enqueued,
                                &mut oracle_coalesced,
                                &mut oracle_max_pending,
                            );
                        }
                    }
                }
            }
        }

        #[cfg(feature = "crate_health")]
        let queued_detection_available =
            !detect_queue.is_empty()
                || !oracle_detect_pending.is_empty()
                || (!integration_probe_queue.is_empty() && projection_pending.is_empty());
        #[cfg(feature = "crate_health")]
        let force_detection =
            queued_detection_available && ingress_since_detect >= ingress_burst_before_detect;
        #[cfg(feature = "crate_health")]
        let maybe_ev = if force_detection {
            None
        } else if queued_detection_available {
            consumer.try_pop()
        } else if !sweep_dirty_banks.is_empty() {
            consumer.pop_timeout(Duration::from_millis(1))
        } else {
            consumer.pop_timeout(Duration::from_millis(250))
        };
        #[cfg(not(feature = "crate_health"))]
        let maybe_ev = consumer.pop_timeout(Duration::from_millis(250));

        let mut detect = None;

        if let Some(ev) = maybe_ev {
            #[cfg(feature = "crate_health")]
            {
                ingress_since_detect = ingress_since_detect.saturating_add(1);
            }
            // Trigger (a) Long-Tail: jedes MarginfiAccount-Update → Detection im Shadow-Modus.
            detect = match &ev {
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
        let updated_bank_event: Option<(Pubkey, u64, u64)> = match &ev {
            IngressEvent::Account(a) if classify_account(&a.data) == AccountType::Bank => {
                Some((a.pubkey, a.t_rx_ns, a.slot))
            }
            _ => None,
        };
        let updated_bank: Option<Pubkey> = updated_bank_event.map(|(pk, _, _)| pk);
        #[cfg(feature = "crate_health")]
        let updated_marginfi_account: Option<Pubkey> = match &ev {
            IngressEvent::Account(a) if classify_account(&a.data) == AccountType::MarginfiAccount => Some(a.pubkey),
            _ => None,
        };
        #[cfg(feature = "crate_health")]
        let dependency_update: Option<(Pubkey, u64, u64)> = match &ev {
            IngressEvent::Account(a)
                if reverse_index.banks_for_oracle(&a.pubkey).next().is_some() =>
            {
                Some((a.pubkey, a.t_rx_ns, a.slot))
            }
            _ => None,
        };
        if let IngressEvent::Account(a) = &ev {
            if a.pubkey == program_data && hash(&a.data) != program_data_baseline_hash {
                if !program_upgrade_latched {
                    eprintln!(
                        "PROGRAM_UPGRADE detected slot={} program_data={} — latched HALT until explicit P1B promotion",
                        a.slot, program_data
                    );
                }
                program_upgrade_latched = true;
            }
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

                // A program upgrade can race startup/recovery before its WSS notification is
                // observed. Re-check the canonical recovered ProgramData bytes against the
                // startup baseline before allowing any detection work.
                if st.get(&program_data)
                    .map(|r| hash(&r.data) != program_data_baseline_hash)
                    .unwrap_or(true)
                {
                    program_upgrade_latched = true;
                }
                if program_upgrade_latched {
                    st.halt(darkbot_common::error::HaltReason::ProgramUpgrade);
                    eprintln!(
                        "PROGRAM_UPGRADE HALT active program_data={} — no automatic resume",
                        program_data
                    );
                    continue;
                }

                #[cfg(feature = "crate_health")]
                {
                    let (rebuilt, indexed_banks, indexed_accounts, unsupported_banks) =
                        rebuild_reverse_index(&st, &program, &group);
                    reverse_index = rebuilt;
                    sweep_dirty_banks.clear();
                    sweep_batch_events = 0;
                    sweep_batch_started_ns = None;
                    threshold_index = ThresholdIndex::new();
                    threshold_fallback.clear();
                    threshold_bootstrap.clear();
                    if use_threshold_trigger {
                        for (pk, record) in st.iter_owned_by(&program) {
                            if liability_account_in_group(&record.data, &group) {
                                threshold_bootstrap.push_back(*pk);
                            }
                        }
                        threshold_ready = threshold_bootstrap.is_empty();
                    } else {
                        threshold_ready = true;
                    }
                    eprintln!(
                        "reverse_index_ready: banks={} accounts={} unsupported_bank_oracle_layouts={} threshold_bootstrap={} threshold_trigger={}",
                        indexed_banks, indexed_accounts, unsupported_banks, threshold_bootstrap.len(), use_threshold_trigger
                    );
                    if !use_threshold_trigger {
                        // [R127c] eva01 parity: one full debtor scan at startup so an account
                        // already underwater before StreamReady cannot wait forever for a future
                        // unrelated write. Reuse the same batched/parallel Health admission path
                        // by marking every current group Bank dirty exactly once.
                        let startup_t_rx_ns = clock.now_ns();
                        let startup_slot = slot_watch.get();
                        let view = StateView(&st);
                        let mut startup_banks = 0usize;
                        for (pk, record) in st.iter_owned_by(&program) {
                            let Some(bank) = bank_from_state(&view, &program, pk) else {
                                continue;
                            };
                            if bank.group != group || classify_account(&record.data) != AccountType::Bank {
                                continue;
                            }
                            sweep_dirty_banks.insert(*pk, (startup_t_rx_ns, startup_slot));
                            startup_banks = startup_banks.saturating_add(1);
                        }
                        if startup_banks > 0 {
                            // Force one immediate startup batch; runtime batches use the normal
                            // event-count/time bounds.
                            sweep_batch_events = sweep_batch_max_events;
                            sweep_batch_started_ns = Some(startup_t_rx_ns);
                            sweep_max_dirty_banks =
                                sweep_max_dirty_banks.max(sweep_dirty_banks.len());
                        }
                        eprintln!(
                            "r127_health_sweep_ready: reverse_accounts={} startup_banks={} threshold_trigger=false",
                            indexed_accounts, startup_banks
                        );
                    }
                }

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

                    let view = StateView(&st);
                    let mut kamino = 0usize;
                    let mut drift = 0usize;
                    let mut juplend = 0usize;
                    let mut pricing_kamino = 0usize;
                    let mut pricing_drift = 0usize;
                    let mut pricing_juplend = 0usize;
                    for (pk, r) in st.iter_owned_by(&program) {
                        if !bootstrap_probe_candidate(&r.data, &group) {
                            continue;
                        }
                        if let Ok(kinds) =
                            darkbot_project0::integration::kinds_for_account(&view, &program, pk)
                        {
                            for k in kinds {
                                match k {
                                    darkbot_project0::integration::IntegrationKind::Kamino => kamino += 1,
                                    darkbot_project0::integration::IntegrationKind::Drift => drift += 1,
                                    darkbot_project0::integration::IntegrationKind::JupLend => juplend += 1,
                                }
                            }
                        }
                        if let Ok(kinds) =
                            darkbot_project0::integration::pricing_kinds_for_account(&view, &program, pk)
                        {
                            for k in kinds {
                                match k {
                                    darkbot_project0::integration::IntegrationKind::Kamino => pricing_kamino += 1,
                                    darkbot_project0::integration::IntegrationKind::Drift => pricing_drift += 1,
                                    darkbot_project0::integration::IntegrationKind::JupLend => pricing_juplend += 1,
                                }
                            }
                        }
                    }
                    eprintln!(
                        "bootstrap_probe_integrations: withdraw_kamino={} withdraw_drift={} withdraw_juplend={} pricing_kamino={} pricing_drift={} pricing_juplend={}",
                        kamino, drift, juplend, pricing_kamino, pricing_drift, pricing_juplend
                    );


                    // [FIX-115][R6] Optional targeted qualification probe. The historical
                    // was_liquidatable set can legitimately contain zero integration accounts.
                    // Scan every liability-bearing account already present in canonical RAM,
                    // report the actual integration population, then queue a small deterministic
                    // sample per venue through the existing projection -> Health/Sizing pipeline.
                    if integration_probe_enabled {
                        let mut all_withdraw = [0usize; 3];
                        let mut all_pricing = [0usize; 3];
                        let mut scan_errors = 0usize;
                        let mut kamino_accounts: Vec<(Pubkey, u64)> = Vec::new();
                        let mut drift_accounts: Vec<(Pubkey, u64)> = Vec::new();
                        let mut juplend_accounts: Vec<(Pubkey, u64)> = Vec::new();

                        for (pk, r) in st.iter_owned_by(&program) {
                            if !marginfi_account_in_group(&r.data, &group) {
                                continue;
                            }

                            let withdraw = match darkbot_project0::integration::kinds_for_account(
                                &view,
                                &program,
                                pk,
                            ) {
                                Ok(v) => v,
                                Err(_) => {
                                    scan_errors += 1;
                                    continue;
                                }
                            };
                            let pricing = match darkbot_project0::integration::pricing_kinds_for_account(
                                &view,
                                &program,
                                pk,
                            ) {
                                Ok(v) => v,
                                Err(_) => {
                                    scan_errors += 1;
                                    continue;
                                }
                            };

                            let mut has_kamino = false;
                            let mut has_drift = false;
                            let mut has_juplend = false;
                            for k in withdraw {
                                match k {
                                    darkbot_project0::integration::IntegrationKind::Kamino => {
                                        all_withdraw[0] += 1;
                                        has_kamino = true;
                                    }
                                    darkbot_project0::integration::IntegrationKind::Drift => {
                                        all_withdraw[1] += 1;
                                        has_drift = true;
                                    }
                                    darkbot_project0::integration::IntegrationKind::JupLend => {
                                        all_withdraw[2] += 1;
                                        has_juplend = true;
                                    }
                                }
                            }
                            for k in pricing {
                                match k {
                                    darkbot_project0::integration::IntegrationKind::Kamino => {
                                        all_pricing[0] += 1;
                                        has_kamino = true;
                                    }
                                    darkbot_project0::integration::IntegrationKind::Drift => {
                                        all_pricing[1] += 1;
                                        has_drift = true;
                                    }
                                    darkbot_project0::integration::IntegrationKind::JupLend => {
                                        all_pricing[2] += 1;
                                        has_juplend = true;
                                    }
                                }
                            }

                            if has_kamino {
                                kamino_accounts.push((*pk, r.slot));
                            }
                            if has_drift {
                                drift_accounts.push((*pk, r.slot));
                            }
                            if has_juplend {
                                juplend_accounts.push((*pk, r.slot));
                            }
                        }

                        for v in [&mut kamino_accounts, &mut drift_accounts, &mut juplend_accounts] {
                            v.sort_unstable_by_key(|(pk, _)| pk.to_bytes());
                            v.dedup_by_key(|(pk, _)| *pk);
                        }

                        eprintln!(
                            "integration_target_census: withdraw_kamino={} withdraw_drift={} withdraw_juplend={} pricing_kamino={} pricing_drift={} pricing_juplend={} scan_errors={}",
                            all_withdraw[0],
                            all_withdraw[1],
                            all_withdraw[2],
                            all_pricing[0],
                            all_pricing[1],
                            all_pricing[2],
                            scan_errors
                        );

                        let mut selected = HashSet::new();
                        let mut queued_kamino = 0usize;
                        let mut queued_drift = 0usize;
                        let mut queued_juplend = 0usize;

                        for (kind_idx, accounts) in [
                            (0usize, &kamino_accounts),
                            (1usize, &drift_accounts),
                            (2usize, &juplend_accounts),
                        ] {
                            for (pk, slot) in accounts.iter().take(integration_probe_limit_per_kind) {
                                match kind_idx {
                                    0 => queued_kamino += 1,
                                    1 => queued_drift += 1,
                                    _ => queued_juplend += 1,
                                }
                                if selected.insert(*pk) {
                                    integration_probe_queue.push_back((*pk, *slot));
                                }
                            }
                        }

                        integration_probe_expected = selected.clone();
                        integration_probe_terminal.clear();
                        integration_probe_projection_ready = 0;
                        integration_probe_projection_failed = 0;
                        integration_probe_projection_stale = 0;
                        integration_probe_completion_reported = false;

                        eprintln!(
                            "integration_target_probe: queued_unique={} kamino_sample={} drift_sample={} juplend_sample={} limit_per_kind={}",
                            selected.len(),
                            queued_kamino,
                            queued_drift,
                            queued_juplend,
                            integration_probe_limit_per_kind
                        );
                    }
                }
            }
        }
        // [FIX-133a] Keep reverse indices in lockstep with the canonical post-event state.
        // This does not trigger detection yet; FIX-133b will consume it only after threshold
        // qualification. Unknown oracle layouts are removed from the trigger map fail-closed.
        #[cfg(feature = "crate_health")]
        if let Some(account_pk) = updated_marginfi_account {
            match st.get(&account_pk)
                .and_then(|r| active_banks_in_group_account(&r.data, &group))
            {
                Some(banks) => {
                    reverse_index.set_account_banks(account_pk, &banks);
                    if !use_threshold_trigger {
                        // R127 Long-Tail admission uses targeted exact-health sweeps.
                    } else {
                    let now_s = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let slot = st.get(&account_pk).map(|r| r.slot).unwrap_or(slot_watch.get());
                    let view = StateView(&st);
                    let engine = darkbot_project0::health::crate_impl::CrateHealth {
                        state: &view,
                        group,
                    };
                    match engine.thresholds(&account_pk, now_s, slot) {
                        Ok(t) => {
                            threshold_index.set_account(account_pk, &t.thresholds);
                            threshold_fallback.remove(&account_pk);
                        }
                        Err(e) => {
                            threshold_index.remove_account(&account_pk);
                            threshold_fallback.insert(account_pk);
                            eprintln!(
                                "threshold_account_fallback: account={} error={}",
                                account_pk, e
                            );
                        }
                    }
                    }
                }
                None => {
                    reverse_index.remove_account(&account_pk);
                    if use_threshold_trigger {
                        threshold_index.remove_account(&account_pk);
                        threshold_fallback.remove(&account_pk);
                    }
                }
            }
        }
        #[cfg(feature = "crate_health")]
        if let Some(bank_pk) = updated_bank {
            let affected_accounts: Vec<Pubkey> =
                reverse_index.accounts_for_bank(&bank_pk).copied().collect();
            if use_threshold_trigger {
                for account in affected_accounts.iter().copied() {
                    threshold_index.remove_account(&account);
                    threshold_fallback.insert(account);
                    threshold_bootstrap.push_back(account);
                }
                threshold_ready = threshold_bootstrap.is_empty();
            } else if st.run_state().is_tradable() {
                if let Some((_, t_rx_ns, event_slot)) = updated_bank_event {
                    if sweep_batch_started_ns.is_none() {
                        sweep_batch_started_ns = Some(clock.now_ns());
                    }
                    sweep_dirty_banks
                        .entry(bank_pk)
                        .and_modify(|w| {
                            if event_slot > w.1 || (event_slot == w.1 && t_rx_ns > w.0) {
                                *w = (t_rx_ns, event_slot);
                            }
                        })
                        .or_insert((t_rx_ns, event_slot));
                    sweep_batch_events = sweep_batch_events.saturating_add(1);
                    sweep_max_dirty_banks = sweep_max_dirty_banks.max(sweep_dirty_banks.len());
                }
            }

            let view = StateView(&st);
            match bank_from_state(&view, &program, &bank_pk).filter(|b| b.group == group) {
                Some(bank) => match bank_observation_keys(&bank) {
                    Ok(_) => {
                        let deps =
                            darkbot_project0::health::dependency_keys(&view, &program, &[bank_pk]);
                        reverse_index.set_bank_oracles(bank_pk, &deps);
                    }
                    Err(e) => {
                        reverse_index.set_bank_oracles(bank_pk, &[]);
                        eprintln!(
                            "reverse_index_bank_unsupported: bank={} error={}",
                            bank_pk, e
                        );
                    }
                },
                None => reverse_index.set_bank_oracles(bank_pk, &[]),
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
                    ],
                    accounts: all_keys,
                    slots: true,
                    roots: true,
                    transactions_mentioning: vec![],
                };
                ingress_session = spawn_ingress(sub2, producer.clone(), halt.clone())?;
            }
        }
        #[cfg(feature = "crate_health")]
        if st.run_state().is_tradable() {
            if let Some((dependency, t_rx_ns, event_slot)) = dependency_update {
                oracle_dependency_updates = oracle_dependency_updates.saturating_add(1);
                let now_s = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let view = StateView(&st);
                let engine = darkbot_project0::health::crate_impl::CrateHealth {
                    state: &view,
                    group,
                };
                let banks: Vec<Pubkey> =
                    reverse_index.banks_for_oracle(&dependency).copied().collect();

                if !use_threshold_trigger {
                    if sweep_batch_started_ns.is_none() {
                        sweep_batch_started_ns = Some(clock.now_ns());
                    }
                    for bank in banks {
                        sweep_dirty_banks
                            .entry(bank)
                            .and_modify(|w| {
                                if event_slot > w.1 || (event_slot == w.1 && t_rx_ns > w.0) {
                                    *w = (t_rx_ns, event_slot);
                                }
                            })
                            .or_insert((t_rx_ns, event_slot));
                    }
                    sweep_batch_events = sweep_batch_events.saturating_add(1);
                    sweep_max_dirty_banks = sweep_max_dirty_banks.max(sweep_dirty_banks.len());
                } else {
                let mut candidates: HashSet<Pubkey> = HashSet::new();
                for bank in banks {
                    let all_accounts: Vec<Pubkey> =
                        reverse_index.accounts_for_bank(&bank).copied().collect();
                    if threshold_ready {
                        match engine.maintenance_risk_price_bits(&bank, now_s, event_slot) {
                            Ok((asset_price, liability_price)) => {
                                candidates.extend(
                                    threshold_index
                                        .candidates(&bank, asset_price, liability_price, 0)
                                );
                                candidates.extend(
                                    all_accounts
                                        .iter()
                                        .copied()
                                        .filter(|a| threshold_fallback.contains(a))
                                );
                            }
                            Err(e) => {
                                // [R126] Integration pricing may be stale in canonical state even
                                // though the exact pre-Start refresh is deterministic. Project one
                                // Bank refresh once, then threshold-select accounts from that fresh
                                // pricing state instead of waking the whole Bank immediately.
                                if let Some(existing) = oracle_bank_projection_pending.get_mut(&bank) {
                                    *existing = (t_rx_ns, event_slot);
                                    eprintln!(
                                        "oracle_bank_projection_coalesced: dependency={} bank={} error={}",
                                        dependency, bank, e
                                    );
                                } else {
                                    let Some(bank_record) = st.get(&bank) else {
                                        oracle_price_fallbacks =
                                            oracle_price_fallbacks.saturating_add(1);
                                        candidates.extend(all_accounts);
                                        continue;
                                    };
                                    let Some(bank_state) = bank_from_state(&view, &program, &bank) else {
                                        oracle_price_fallbacks =
                                            oracle_price_fallbacks.saturating_add(1);
                                        candidates.extend(all_accounts);
                                        continue;
                                    };
                                    let refreshes =
                                        match darkbot_project0::integration::refresh_plan_for_bank(
                                            &view,
                                            &bank_state,
                                        ) {
                                            Ok(v) if !v.is_empty() => v,
                                            Ok(_) => {
                                                oracle_price_fallbacks =
                                                    oracle_price_fallbacks.saturating_add(1);
                                                candidates.extend(all_accounts);
                                                continue;
                                            }
                                            Err(refresh_error) => {
                                                oracle_price_fallbacks =
                                                    oracle_price_fallbacks.saturating_add(1);
                                                eprintln!(
                                                    "oracle_bank_projection_plan_fallback: dependency={} bank={} price_error={} refresh_error={}",
                                                    dependency, bank, e, refresh_error
                                                );
                                                candidates.extend(all_accounts);
                                                continue;
                                            }
                                        };
                                    let targets =
                                        darkbot_project0::projection::projection_targets(&refreshes);
                                    if targets.is_empty() {
                                        oracle_price_fallbacks =
                                            oracle_price_fallbacks.saturating_add(1);
                                        candidates.extend(all_accounts);
                                        continue;
                                    }
                                    let Some(bh) = blockhash.fresh(clock.now_ns(), 30_000_000_000) else {
                                        oracle_price_fallbacks =
                                            oracle_price_fallbacks.saturating_add(1);
                                        candidates.extend(all_accounts);
                                        continue;
                                    };
                                    let projection_plan = tx::TxPlan {
                                        version: tx::BuildVersion::V0,
                                        payer: liquidator,
                                        instructions: refreshes,
                                        lookup_tables: vec![],
                                        resources,
                                    };
                                    let mut built = match tx::build(
                                        &projection_plan,
                                        bh.hash,
                                        &TxLimits::default(),
                                    ) {
                                        Ok(v) => v,
                                        Err(build_error) => {
                                            oracle_price_fallbacks =
                                                oracle_price_fallbacks.saturating_add(1);
                                            eprintln!(
                                                "oracle_bank_projection_build_fallback: dependency={} bank={} error={}",
                                                dependency, bank, build_error
                                            );
                                            candidates.extend(all_accounts);
                                            continue;
                                        }
                                    };
                                    if let Err(sign_error) = tx::sign(&mut built, &[&payer]) {
                                        oracle_price_fallbacks =
                                            oracle_price_fallbacks.saturating_add(1);
                                        eprintln!(
                                            "oracle_bank_projection_sign_fallback: dependency={} bank={} error={}",
                                            dependency, bank, sign_error
                                        );
                                        candidates.extend(all_accounts);
                                        continue;
                                    }
                                    let work = OraclePriceProjectionWork {
                                        bank,
                                        base_slot: bank_record.slot,
                                        base_bank_hash: hash(&bank_record.data),
                                        tx: built.tx,
                                        targets,
                                    };
                                    match remote.oracle_price_projection_jobs.try_push(work) {
                                        Ok(()) => {
                                            oracle_bank_projection_pending
                                                .insert(bank, (t_rx_ns, event_slot));
                                            oracle_bank_projection_submitted =
                                                oracle_bank_projection_submitted.saturating_add(1);
                                            eprintln!(
                                                "oracle_bank_projection_pending: dependency={} bank={} price_error={}",
                                                dependency, bank, e
                                            );
                                        }
                                        Err(ChannelError::Full(_)) => {
                                            oracle_price_fallbacks =
                                                oracle_price_fallbacks.saturating_add(1);
                                            candidates.extend(all_accounts);
                                        }
                                        Err(ChannelError::Disconnected(_)) => {
                                            return Err(
                                                "oracle price projection RPC worker disconnected"
                                                    .into(),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        candidates.extend(all_accounts);
                    }
                }

                let count = enqueue_oracle_accounts(
                    candidates,
                    t_rx_ns,
                    event_slot,
                    "oracle_threshold",
                    &mut oracle_detect_order,
                    &mut oracle_detect_pending,
                    &mut oracle_enqueued,
                    &mut oracle_coalesced,
                    &mut oracle_max_pending,
                );
                oracle_candidate_hits =
                    oracle_candidate_hits.saturating_add(count as u64);
                }
            }
        }

        if program_upgrade_latched {
            st.halt(darkbot_common::error::HaltReason::ProgramUpgrade);
            detect = None;
        }
        } else {
            #[cfg(feature = "crate_health")]
            {
                detect = detect_queue
                    .pop_front()
                    .or_else(|| {
                        next_oracle_detect(
                            &mut oracle_detect_order,
                            &mut oracle_detect_pending,
                        )
                    })
                    .or_else(|| {
                        next_integration_probe(
                            &mut integration_probe_queue,
                            !projection_pending.is_empty(),
                            clock.now_ns(),
                        )
                    });
                if detect.is_some() {
                    ingress_since_detect = 0;
                }
            }
            if detect.is_none() {
                continue;
            }
        } // normal ingress event path; projected rechecks skip state mutation

        if let Some((acct, t_rx, slot, trigger)) = detect {
            // Health über das Crate (Feature crate_health), sonst fail-closed NoHealth → Unavailable-Record.
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let view = StateView(&st);

            // [FIX-111][R6] Integration refreshes are projected by an isolated RPC worker first.
            // The returned account set is one-shot: only the exact candidate account state that
            // requested it may consume it, and canonical StateCore is never mutated.
            #[cfg(feature = "crate_health")]
            let projected_owned = {
                let refreshes = match darkbot_project0::integration::refresh_plan_for_account(
                    &view,
                    &program,
                    &acct,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        let line = serde_json::json!({
                            "account": acct.to_string(),
                            "trigger": trigger,
                            "slot": slot,
                            "decision": "Skip",
                            "loss": "IntegrationRefresh",
                            "reason": format!("integration refresh plan: {e}")
                        });
                        let _ = writeln!(records, "{}", line);
                        continue;
                    }
                };

                if refreshes.is_empty() {
                    None
                } else {
                    let Some(base_record) = st.get(&acct) else {
                        let _ = writeln!(records, "{}", serde_json::json!({
                            "account": acct.to_string(),
                            "decision": "Skip",
                            "loss": "IntegrationRefresh",
                            "reason": "candidate account disappeared before projection"
                        }));
                        continue;
                    };
                    let base_account_hash = hash(&base_record.data);

                    let cached = if trigger == "integration_projection" {
                        projection_cache.remove(&acct).filter(|c| {
                            c.base_account_hash == base_account_hash && base_record.slot == slot
                        })
                    } else {
                        projection_cache.remove(&acct);
                        None
                    };

                    if let Some(cached) = cached {
                        match darkbot_project0::projection::ProjectedState::new(
                            &view,
                            cached.projection_slot,
                            cached.projection_unix_s,
                            cached.accounts,
                        ) {
                            Ok(p) => Some(p),
                            Err(e) => {
                                let _ = writeln!(records, "{}", serde_json::json!({
                                    "account": acct.to_string(),
                                    "decision": "Skip",
                                    "loss": "IntegrationRefresh",
                                    "reason": format!("projected state invalid: {e}")
                                }));
                                continue;
                            }
                        }
                    } else {
                        if !projection_pending.contains(&acct) {
                            let targets = match darkbot_project0::integration::projection_targets_for_account(
                                &view,
                                &program,
                                &acct,
                                &refreshes,
                            ) {
                                Ok(v) if !v.is_empty() => v,
                                Ok(_) => {
                                    let _ = writeln!(records, "{}", serde_json::json!({
                                        "account": acct.to_string(),
                                        "decision": "Skip",
                                        "loss": "IntegrationRefresh",
                                        "reason": "integration projection produced zero targets"
                                    }));
                                    continue;
                                }
                                Err(e) => {
                                    let _ = writeln!(records, "{}", serde_json::json!({
                                        "account": acct.to_string(),
                                        "decision": "Skip",
                                        "loss": "IntegrationRefresh",
                                        "reason": format!("projection targets: {e}")
                                    }));
                                    continue;
                                }
                            };

                            let Some(bh) = blockhash.fresh(clock.now_ns(), 30_000_000_000) else {
                                let _ = writeln!(records, "{}", serde_json::json!({
                                    "account": acct.to_string(),
                                    "decision": "Skip",
                                    "loss": "IntegrationRefresh",
                                    "reason": "projection blockhash stale"
                                }));
                                continue;
                            };

                            let projection_plan = tx::TxPlan {
                                version: tx::BuildVersion::V0,
                                payer: liquidator,
                                instructions: refreshes,
                                lookup_tables: vec![],
                                resources,
                            };
                            let mut built = match tx::build(
                                &projection_plan,
                                bh.hash,
                                &TxLimits::default(),
                            ) {
                                Ok(v) => v,
                                Err(e) => {
                                    let _ = writeln!(records, "{}", serde_json::json!({
                                        "account": acct.to_string(),
                                        "decision": "Skip",
                                        "loss": "IntegrationRefresh",
                                        "reason": format!("projection tx build: {e}")
                                    }));
                                    continue;
                                }
                            };
                            if let Err(e) = tx::sign(&mut built, &[&payer]) {
                                let _ = writeln!(records, "{}", serde_json::json!({
                                    "account": acct.to_string(),
                                    "decision": "Skip",
                                    "loss": "IntegrationRefresh",
                                    "reason": format!("projection tx sign: {e}")
                                }));
                                continue;
                            }

                            let work = ProjectionWork {
                                account: acct,
                                base_slot: base_record.slot,
                                base_account_hash,
                                t_rx_ns: t_rx,
                                tx: built.tx,
                                targets,
                            };
                            match remote.projection_jobs.try_push(work) {
                                Ok(()) => {
                                    projection_pending.insert(acct);
                                }
                                Err(ChannelError::Full(_)) => {
                                    let _ = writeln!(records, "{}", serde_json::json!({
                                        "account": acct.to_string(),
                                        "decision": "Skip",
                                        "loss": "IntegrationRefresh",
                                        "reason": "projection worker queue full"
                                    }));
                                }
                                Err(ChannelError::Disconnected(_)) => {
                                    return Err("projection RPC worker disconnected".into());
                                }
                            }
                        }
                        let integration_kinds = darkbot_project0::integration::kinds_for_account(
                            &view,
                            &program,
                            &acct,
                        )
                        .unwrap_or_default()
                        .into_iter()
                        .map(darkbot_project0::integration::kind_name)
                        .collect::<Vec<_>>();
                        let pricing_integration_kinds =
                            darkbot_project0::integration::pricing_kinds_for_account(
                                &view,
                                &program,
                                &acct,
                            )
                            .unwrap_or_default()
                            .into_iter()
                            .map(darkbot_project0::integration::kind_name)
                            .collect::<Vec<_>>();
                        let _ = writeln!(records, "{}", serde_json::json!({
                            "account": acct.to_string(),
                            "trigger": trigger,
                            "slot": slot,
                            "decision": "Skip",
                            "loss": "IntegrationRefresh",
                            "integration_kinds": integration_kinds,
                            "pricing_integration_kinds": pricing_integration_kinds,
                            "reason": "integration projection pending"
                        }));
                        continue;
                    }
                }
            };

            #[cfg(feature = "crate_health")]
            let eval_state: &dyn AccountSource = projected_owned
                .as_ref()
                .map(|p| p as &dyn AccountSource)
                .unwrap_or(&view as &dyn AccountSource);

            // [FIX-119][R6] Use both coordinates of the exact simulation Clock for every
            // projected protocol calculation. The original slot remains trigger provenance.
            #[cfg(feature = "crate_health")]
            let (protocol_now, protocol_slot) = projected_owned.as_ref()
                .map(|p| p.evaluation_time()).unwrap_or((now, slot));
            #[cfg(not(feature = "crate_health"))]
            let (protocol_now, protocol_slot) = (now, slot);

            #[cfg(feature = "crate_health")]
            let engine = darkbot_project0::health::crate_impl::CrateHealth { state: eval_state, group };
            #[cfg(not(feature = "crate_health"))]
            let engine = darkbot_project0::health::NoHealth;
            #[cfg(feature = "crate_health")]
            let inventory = |bank_key: &Pubkey| -> u64 {
                let Some(bank) = bank_from_state(eval_state, &program, bank_key) else { return 0 };
                validated_liquidator_ata(eval_state, &bank.mint, &liquidator).map(|(_, amount)| amount).unwrap_or(0)
            };
            #[cfg(feature = "crate_health")]
            let ata = |mint: &Pubkey| -> Option<Pubkey> {
                validated_liquidator_ata(eval_state, mint, &liquidator).map(|(pk, _)| pk)
            };
            #[cfg(feature = "crate_health")]
            let sizing = ProtocolSizingProvider {
                state: eval_state,
                program,
                group,
                sol_bank,
                inventory: &inventory,
                slippage_bps: config::parsed_or("P0_MAX_SLIPPAGE_BPS", 0).unwrap_or(0),
            };
            #[cfg(feature = "crate_health")]
            let sizer_for_cost = darkbot_project0::sizing::ProtocolSizer {
                state: eval_state,
                program,
                group,
                headroom_bps: darkbot_project0::sizing::DEFAULT_HEADROOM_BPS,
            };
            #[cfg(feature = "crate_health")]
            let lamports_to_usd_micro = |lamports| sizer_for_cost
                .native_amount_cost_usd_micro(&sol_bank, lamports, protocol_now as i64, protocol_slot).ok();
            #[cfg(feature = "crate_health")]
            let builder = P0ReceivershipBuilder {
                state: eval_state,
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
                lamports_to_usd_micro: &lamports_to_usd_micro,
                jito_tip_usd_micro: 0,
                jito_tip_lamports: 0,
                zero_prefund_qualified: config::parsed_or("P0_ZERO_PREFUND_QUALIFIED", false).unwrap_or(false)
                    || (mode == "shadow" && config::parsed_or("P0_ZERO_PREFUND_SHADOW_PROBE", false).unwrap_or(false)),
                path: Path::Rpc,
                resources,
                limits: TxLimits::default(),
            };
            let _ = &view;
            if let Some(day) = ledger.on_account_update(now) { if let Ok(j) = serde_json::to_string(&day) { let _ = writeln!(summaries, "{}", j); } }
            let health_result = engine.health(&acct, protocol_now as i64, protocol_slot);
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
                        "evaluation_slot": protocol_slot,
                        "evaluation_unix_s": protocol_now,
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
                        "evaluation_slot": protocol_slot,
                        "evaluation_unix_s": protocol_now,
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
                        "evaluation_slot": protocol_slot,
                        "evaluation_unix_s": protocol_now,
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
            let mut rec = pipe.evaluate(acct, trigger, t_rx, protocol_now, protocol_slot, st.run_state().is_tradable());
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
                watch.push(WinnerWatch {
                    account: acct,
                    rx_slot: slot,
                    detect_slot: slot,
                    sign_slot: rec.sign_slot,
                    expected_net: rec.net_profit,
                    shadow_ready,
                    our_loss,
                    created_unix_s: now,
                    next_check_unix_s: now.saturating_add(60),
                });
            }
            // [FIX-18][FIX-23] Submitted und AmbiguousSubmission landen beide in der Reconciliation.
            if let (Some(sig), Some(exp)) = (rec.signature.as_deref(), rec.exposure) {
                if let Ok(s) = sig.parse() {
                    let sigs = rec.all_signatures.iter().filter_map(|x| x.parse().ok()).collect();
                    // [FIX-33] tatsächlich verwendeter Pfad aus dem Record, nicht die Adapter-Default.
                    let path = match rec.path.as_deref() { Some("JITO_BUNDLE") => darkbot_solana::submit::SubmissionPath::JitoBundle, Some("JITO_SINGLE") => darkbot_solana::submit::SubmissionPath::JitoSingle, _ => darkbot_solana::submit::SubmissionPath::Rpc };
                    pending.push(ReconcileWork {
                        submission: PendingSubmission {
                            signature: s,
                            signatures: sigs,
                            bundle_id: rec.bundle_id.clone(),
                            path,
                            exposure: exp,
                            submitted_unix_s: now,
                            tip: rec.submitted_tip.clone(),
                        },
                        next_check_unix_s: now,
                    });
                }
            }
        }
        // [FIX-108][R4] Winner/Reconcile RPC is handled only by bounded workers above.
        // Detection/Pipeline (P2→P5) wird hier eingehängt, sobald HealthEngine (P3) verfügbar ist.
        // Bis dahin: nur State + Zähler, damit S1-LT/S2 messbar sind (Ledger).
        let update_total = counts.iter().sum::<u64>();
        if update_total > 0 && update_total % 1000 == 0 && update_total != last_reported_update_total {
            last_reported_update_total = update_total;
            eprintln!("updates: accounts={} banks={} groups={} fee={} liqrec={} unknown={} channel={:?}",
                counts[0], counts[1], counts[2], counts[3], counts[4], counts[5], consumer.stats());
            #[cfg(feature = "crate_health")]
            eprintln!(
                "oracle_trigger_stats: threshold_ready={} tracked={} fallback={} bootstrap_remaining={} dependency_updates={} candidate_hits={} enqueued={} coalesced={} price_fallbacks={} pending={} max_pending={} detect_queue={} bank_projection_pending={} bank_projection_submitted={} bank_projection_ready={} bank_projection_failed={} bank_projection_stale={} ingress_backlog={} ingress_since_detect={} threshold_trigger={} sweep_checks={} sweep_healthy={} sweep_liquidatable={} sweep_errors={} sweep_batches={} sweep_dirty_banks={} sweep_account_refs={} sweep_unique_accounts={} sweep_forwarded={} sweep_max_unique_batch={} sweep_max_dirty_banks={}",
                threshold_ready,
                threshold_index.tracked_accounts(),
                threshold_fallback.len(),
                threshold_bootstrap.len(),
                oracle_dependency_updates,
                oracle_candidate_hits,
                oracle_enqueued,
                oracle_coalesced,
                oracle_price_fallbacks,
                oracle_detect_pending.len(),
                oracle_max_pending,
                detect_queue.len(),
                oracle_bank_projection_pending.len(),
                oracle_bank_projection_submitted,
                oracle_bank_projection_ready,
                oracle_bank_projection_failed,
                oracle_bank_projection_stale,
                consumer.stats().sent.saturating_sub(consumer.stats().received),
                ingress_since_detect,
                use_threshold_trigger,
                health_sweep_checks,
                health_sweep_healthy,
                health_sweep_liquidatable,
                health_sweep_errors,
                sweep_batches,
                sweep_dirty_banks.len(),
                sweep_account_refs,
                sweep_unique_accounts,
                sweep_forwarded,
                sweep_max_unique_batch,
                sweep_max_dirty_banks
            );
        }
    }
    // [FIX-46] Letzten (Teil-)Tag beim regulären Shutdown schreiben, damit das Ledger vollständig ist.
    let final_summary = ledger.finish();
    if let Ok(j) = serde_json::to_string(&final_summary) { let _ = writeln!(summaries, "{}", j); }
    let _ = summaries.flush();
    let _ = records.flush();
    Ok(())
}

#[cfg(all(test, feature = "crate_health"))]
mod integration_probe_tests {
    use super::*;

    #[test]
    fn target_waits_unsigned_until_worker_is_idle() {
        let account = Pubkey::new_unique();
        let mut queue = VecDeque::from([(account, 42)]);
        assert!(next_integration_probe(&mut queue, true, 100).is_none());
        assert_eq!(queue.len(), 1);
        // A long qualification backlog does not backdate a newly started probe.
        assert_eq!(next_integration_probe(&mut queue, false, 9_000),
            Some((account, 9_000, 42, "integration_target_probe")));
        assert!(queue.is_empty());
    }

    #[test]
    fn oracle_detect_returns_latest_coalesced_work_once() {
        let a = Pubkey::new_unique();
        let mut order = VecDeque::from([a]);
        let mut pending = HashMap::from([
            (a, (a, 200, 9, "oracle_threshold"))
        ]);
        assert_eq!(
            next_oracle_detect(&mut order, &mut pending),
            Some((a, 200, 9, "oracle_threshold"))
        );
        assert!(pending.is_empty());
        assert!(next_oracle_detect(&mut order, &mut pending).is_none());
    }

    #[test]
    fn oracle_detect_skips_stale_order_entries() {
        let stale = Pubkey::new_unique();
        let live = Pubkey::new_unique();
        let mut order = VecDeque::from([stale, live]);
        let mut pending = HashMap::from([
            (live, (live, 300, 10, "oracle_threshold"))
        ]);
        assert_eq!(next_oracle_detect(&mut order, &mut pending).unwrap().0, live);
        assert!(order.is_empty());
    }

    #[test]
    fn only_one_target_is_dispatched_per_idle_turn() {
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        let mut queue = VecDeque::from([(a, 1), (b, 2)]);
        assert_eq!(next_integration_probe(&mut queue, false, 100).unwrap().0, a);
        assert!(next_integration_probe(&mut queue, true, 101).is_none());
        assert_eq!(queue.front(), Some(&(b, 2)));
        assert_eq!(next_integration_probe(&mut queue, false, 200).unwrap().0, b);
        assert!(next_integration_probe(&mut queue, false, 201).is_none());
    }
}

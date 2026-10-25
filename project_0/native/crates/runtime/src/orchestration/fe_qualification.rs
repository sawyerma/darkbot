//! FE-01 scope expansion over the existing pinned decoders, Health and Sizing.
//! No signer, swap client or submission adapter. Observation is not source parity.
use crate::analytics::payload::{
    Fe01Account, Fe01AccountDigest, Fe01Observation, Fe01Read, Fe01ReadOutcome, Fe01Report,
    Fe01Size, Fe01Smoke, Fe01SmokeTarget, Fe01Target,
};
use base64::Engine;
use darkbot_project0::{
    health::{
        crate_impl::{bank_observation_keys, CrateHealth},
        AccountSource, HealthEngine,
    },
    protocol,
    sizing::ProtocolSizer,
};
use darkbot_solana::runtime_config::RuntimeConfig;
use marginfi_type_crate::types::{Bank, FeeState, MarginfiAccount, MarginfiGroup};
use solana_sdk::{account::Account, clock::Clock, pubkey::Pubkey};
use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

pub use super::provider_evidence::{validate_provider_evidence, ProviderEvidence, ProviderProof};

pub const SOURCE_PIN: &str = protocol::SOURCE_COMMIT;
pub const LOADER: &str = protocol::BPF_LOADER_UPGRADEABLE_ID;
pub fn sha(bytes: &[u8]) -> String {
    solana_sdk::hash::hash(bytes)
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[derive(Default, Debug)]
pub struct ObservedState(pub BTreeMap<Pubkey, Account>);
impl AccountSource for ObservedState {
    fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
        self.0
            .get(key)
            .map(|a| (a.owner, a.lamports, a.data.as_slice()))
    }
}
fn key(s: &str) -> Result<Pubkey, String> {
    Pubkey::from_str(s).map_err(|_| "FE_PUBKEY_INVALID".into())
}
pub(crate) fn typed<T: bytemuck::Pod>(
    state: &ObservedState,
    k: &Pubkey,
    program: &Pubkey,
    disc: [u8; 8],
) -> Result<T, String> {
    let a = state
        .0
        .get(k)
        .ok_or_else(|| format!("FE_ACCOUNT_MISSING:{k}"))?;
    if a.owner != *program
        || a.executable
        || a.data.get(..8) != Some(disc.as_slice())
        || a.data.len() != 8 + std::mem::size_of::<T>()
    {
        return Err(format!(
            "FE_OWNER_OR_EXACT_LAYOUT_MISMATCH:{k}:{}",
            a.data.len()
        ));
    }
    bytemuck::try_pod_read_unaligned(&a.data[8..]).map_err(|_| "FE_POD_LAYOUT".into())
}
/// The `getMultipleAccounts` answer (base64 encoding) as the concrete wire type (JFI-02): the
/// JSON-RPC edge decodes into it directly, nothing behind it sees JSON. Protocol layouts stay
/// owned by the pinned Rust crates.
#[derive(serde::Deserialize)]
pub struct WireMultipleAccounts {
    context: WireContext,
    value: Vec<Option<WireAccount>>,
}
#[derive(serde::Deserialize)]
struct WireContext {
    slot: u64,
}
#[derive(serde::Deserialize)]
struct WireAccount {
    owner: String,
    lamports: u64,
    executable: bool,
    data: (String, String),
}
impl WireMultipleAccounts {
    /// The typed outcome saved as the FE01_READ frame; an owner or encoding the wire type
    /// cannot carry is a failed read, never a guessed account.
    pub fn outcome(self) -> Fe01ReadOutcome {
        let accounts = self
            .value
            .into_iter()
            .map(|a| {
                a.map(|a| {
                    if a.data.1 != "base64" {
                        return Err("FE_ACCOUNT_ENCODING".to_string());
                    }
                    Ok(Fe01Account {
                        owner: key(&a.owner)?,
                        lamports: a.lamports,
                        executable: a.executable,
                        data: base64::engine::general_purpose::STANDARD
                            .decode(a.data.0)
                            .map_err(|_| "FE_ACCOUNT_ENCODING".to_string())?,
                    })
                })
                .transpose()
            })
            .collect::<Result<Vec<_>, String>>();
        match accounts {
            Ok(accounts) => Fe01ReadOutcome::Accounts {
                context_slot: self.context.slot,
                accounts,
            },
            Err(reason) => Fe01ReadOutcome::Failed { reason },
        }
    }
}
/// The observed state of one saved read: its context must not regress below the requested
/// floor, and every requested key has exactly one account.
pub fn observed(read: &Fe01Read) -> Result<(u64, ObservedState), String> {
    let (slot, accounts) = match &read.outcome {
        Fe01ReadOutcome::Accounts {
            context_slot,
            accounts,
        } => (*context_slot, accounts),
        Fe01ReadOutcome::Failed { reason } => return Err(reason.clone()),
    };
    if slot == 0 || slot < read.min_context_slot {
        return Err("FE_CONTEXT_REGRESSION".into());
    }
    if accounts.len() != read.keys.len() {
        return Err("FE_BATCH_LENGTH".into());
    }
    let mut state = ObservedState::default();
    for (k, a) in read.keys.iter().zip(accounts) {
        let a = a
            .as_ref()
            .ok_or_else(|| format!("FE_ACCOUNT_MISSING:{k}"))?;
        let account = Account {
            owner: a.owner,
            lamports: a.lamports,
            data: a.data.clone(),
            executable: a.executable,
            rent_epoch: 0,
        };
        if state.0.insert(*k, account).is_some() {
            return Err("FE_DUPLICATE_ACCOUNT".into());
        }
    }
    Ok((slot, state))
}
fn program_data(state: &ObservedState, program: &Pubkey) -> Result<Pubkey, String> {
    let a = state.0.get(program).ok_or("FE_PROGRAM_MISSING")?;
    protocol::decode_program(program, &a.owner, a.executable, &a.data).map_err(|e| match e {
        protocol::DeploymentError::Address => "FE_PROGRAMDATA_ADDRESS".into(),
        _ => "FE_PROGRAM_LAYOUT".into(),
    })
}
fn deployment(state: &ObservedState, pd: &Pubkey) -> Result<(u64, String), String> {
    let a = state.0.get(pd).ok_or("FE_PROGRAMDATA_MISSING")?;
    let d = protocol::decode_program_data(pd, &a.owner, a.executable, &a.data)
        .map_err(|_| "FE_PROGRAMDATA_LAYOUT")?;
    Ok((d.deployment_slot, sha(&a.data)))
}
fn dependencies(
    state: &ObservedState,
    program: &Pubkey,
    group: &Pubkey,
    targets: &[Pubkey],
) -> Result<BTreeSet<Pubkey>, String> {
    let mut banks = BTreeSet::new();
    for target in targets {
        let account: MarginfiAccount =
            typed(state, target, program, protocol::DISC_MARGINFI_ACCOUNT)?;
        if account.group != *group {
            return Err("FE_TARGET_GROUP_MISMATCH".into());
        }
        banks.extend(
            account
                .lending_account
                .balances
                .iter()
                .filter(|b| b.is_active())
                .map(|b| b.bank_pk),
        );
    }
    let mut required = banks.clone();
    for bk in banks {
        if !state.0.contains_key(&bk) {
            continue;
        }
        let bank: Bank = typed(state, &bk, program, protocol::DISC_BANK)?;
        if bank.group != *group {
            return Err("FE_BANK_GROUP_MISMATCH".into());
        }
        bank_observation_keys(&bank).map_err(|e| format!("FE_ORACLE_UNSUPPORTED:{e}"))?;
        required.extend(darkbot_project0::health::dependency_keys(
            state,
            program,
            &[bk],
        ));
        required.insert(bank.liquidity_vault);
    }
    if required.contains(&Pubkey::default()) {
        return Err("FE_ZERO_DEPENDENCY".into());
    }
    Ok(required)
}
/// FE-01 transaction version pin: what this bot builds, not what the cluster accepts. The
/// liquidation path compiles v0 messages with address lookup tables; v1 exists only as a
/// size/capability measurement that refuses to sign. Cluster support is not observed here.
pub const TRANSACTION_VERSION_PIN: &str =
    "BUILD_V0_WITH_ADDRESS_LOOKUP_TABLES; V1_CAPABILITY_MEASUREMENT_ONLY; CLUSTER_SUPPORT_UNKNOWN_NOT_OBSERVED";
/// Each round refreshes the complete discovered scope. New balances/banks expand
/// the next round; an inconsistent/moving snapshot never becomes qualified.
pub struct CollectedScope {
    pub state: ObservedState,
    pub clock: Clock,
    pub program: Pubkey,
    pub group: Pubkey,
    pub targets: Vec<Pubkey>,
    pub pd: Pubkey,
    pub deployment: (u64, String),
    pub round: u64,
}
/// Keys every scope round reads besides the targets: program, ProgramData, group, clock, fee state.
const FIXED_SCOPE_KEYS: u64 = 5;
/// Account reads [`collect_scope`] needs to run every configured round: the program and
/// ProgramData reads, then each round over at least the initial scope ([`FIXED_SCOPE_KEYS`] plus
/// the targets) in `fe_batch_size` batches. Dependencies discovered on chain only add batches.
pub fn required_scope_reads(config: &RuntimeConfig) -> Result<u64, String> {
    let batch = config.limits.fe_batch_size;
    if batch == 0 || config.fe_targets.is_empty() {
        return Err("FE_SCOPE_OR_BATCH_BUDGET_INVALID".into());
    }
    let initial = FIXED_SCOPE_KEYS + config.fe_targets.len() as u64;
    Ok(2 + config.limits.fe_snapshot_rounds * initial.div_ceil(batch))
}
/// R4: scope keys one bank can add at most — the bank, its liquidity vault, its mint, at most four
/// observation keys (`bank_observation_keys`, StakedWithPythPush) and at most three integration
/// accounts (`health::dependency_keys`); overlaps only lower the count.
pub const MAX_SCOPE_KEYS_PER_BANK: u64 = 10;
/// R4: upper bound of the scope collector's reads. The scope closes after at most two expansions
/// (targets ⇒ their banks ⇒ each bank's dependencies; nothing deeper is followed), so a round
/// never reads more than `FIXED + T·(1 + MAX_LENDING_ACCOUNT_BALANCES · MAX_SCOPE_KEYS_PER_BANK)`
/// keys while the targets' bank sets stay unchanged during the audit (keys are only added across
/// rounds; a target that changes its banks mid-audit can exceed it and then runs into the hard
/// limit). [`required_scope_reads`] is the minimum (initial scope only); a budget between both may
/// run out on chain and then blocks the audit (fail-closed), never passes it.
pub fn worst_case_scope_reads(config: &RuntimeConfig) -> Result<u64, String> {
    let batch = config.limits.fe_batch_size;
    if batch == 0 || config.fe_targets.is_empty() {
        return Err("FE_SCOPE_OR_BATCH_BUDGET_INVALID".into());
    }
    let per_target = 1 + marginfi_type_crate::types::MAX_LENDING_ACCOUNT_BALANCES as u64
        * MAX_SCOPE_KEYS_PER_BANK;
    let max_keys = FIXED_SCOPE_KEYS + config.fe_targets.len() as u64 * per_target;
    Ok(2 + config.limits.fe_snapshot_rounds * max_keys.div_ceil(batch))
}
/// The same canonical scope collector, reusable by boot without constructing JSON reports.
pub fn collect_scope<F>(
    config: &RuntimeConfig,
    mut read: F,
    boot: bool,
) -> Result<CollectedScope, String>
where
    F: FnMut(&[Pubkey], u64) -> Result<(u64, ObservedState), String>,
{
    if config.fe_targets.is_empty()
        || config.limits.fe_batch_size == 0
        || config.limits.fe_batch_size > 100
        || config.limits.fe_snapshot_rounds == 0
    {
        return Err("FE_SCOPE_OR_BATCH_BUDGET_INVALID".into());
    }
    let mut read = |keys: &[Pubkey], floor: u64| -> Result<(u64, ObservedState), String> {
        let (slot, state) = read(keys, floor)?;
        if slot == 0 || slot < floor {
            return Err("FE_CONTEXT_REGRESSION".into());
        }
        if state.0.len() != keys.len() || keys.iter().any(|k| !state.0.contains_key(k)) {
            return Err("FE_BATCH_SCOPE_MISMATCH".into());
        }
        Ok((slot, state))
    };
    let program = key(protocol::PROGRAM_ID_MAINNET)?;
    let group = key(&config.fe_group)?;
    let targets = config
        .fe_targets
        .iter()
        .map(|s| key(s))
        .collect::<Result<Vec<_>, _>>()?;
    if group == Pubkey::default()
        || targets.contains(&Pubkey::default())
        || targets.iter().collect::<BTreeSet<_>>().len() != targets.len()
    {
        return Err("FE_TARGET_SCOPE_INVALID".into());
    }
    let (mut floor, initial) = read(&[program], 0)?;
    let pd = program_data(&initial, &program)?;
    let (s, initial_pd) = read(&[pd], floor)?;
    floor = s;
    let baseline = deployment(&initial_pd, &pd)?;
    if baseline.0 > floor {
        return Err("FE_DEPLOYMENT_FROM_FUTURE".into());
    }
    let clock_key = solana_sdk::sysvar::clock::id();
    let fee = protocol::fee_state_pda(&program);
    let fixed = [program, pd, group, clock_key, fee];
    debug_assert_eq!(fixed.len() as u64, FIXED_SCOPE_KEYS);
    let mut keys: BTreeSet<_> = fixed.into_iter().chain(targets.iter().copied()).collect();
    let mut last_reason = "FE_SCOPE_NOT_CLOSED";
    for round in 1..=config.limits.fe_snapshot_rounds {
        let mut state = ObservedState::default();
        let mut slots = BTreeSet::new();
        for chunk in keys
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .chunks(config.limits.fe_batch_size as usize)
        {
            let (slot, batch) = read(chunk, floor)?;
            floor = slot;
            slots.insert(slot);
            state.0.extend(batch.0);
        }
        if program_data(&state, &program)? != pd || deployment(&state, &pd)? != baseline {
            return Err("FE_PROGRAM_UPGRADE_DURING_OBSERVATION".into());
        }
        let _: MarginfiGroup = typed(&state, &group, &program, protocol::DISC_MARGINFI_GROUP)?;
        let _: FeeState = typed(&state, &fee, &program, protocol::DISC_FEE_STATE)?;
        let mut required = dependencies(&state, &program, &group, &targets)?;
        if boot {
            for a in state.0.values() {
                if a.owner == program && a.data.get(..8) == Some(protocol::DISC_BANK.as_slice()) {
                    let bank: Bank = bytemuck::try_pod_read_unaligned(&a.data[8..])
                        .map_err(|_| "FE_BANK_LAYOUT")?;
                    required.insert(bank.mint);
                }
            }
        }

        if !required.is_subset(&keys) {
            keys.extend(required);
            last_reason = "FE_SCOPE_NOT_CLOSED";
            continue;
        }
        if slots.len() != 1 {
            last_reason = "FE_CONTEXT_SLOTS_NOT_COHERENT";
            continue;
        }
        let clock_account = state.0.get(&clock_key).ok_or("FE_CLOCK_MISSING")?;
        if clock_account.owner != solana_sdk::sysvar::id()
            || clock_account.executable
            || clock_account.data.len() != 40
        {
            return Err("FE_CLOCK_LAYOUT".into());
        }
        let clock: Clock =
            bincode::deserialize(&clock_account.data).map_err(|_| "FE_CLOCK_LAYOUT")?;
        if clock.slot != floor || clock.unix_timestamp < 0 {
            last_reason = "FE_CLOCK_CONTEXT_MISMATCH";
            continue;
        }
        return Ok(CollectedScope {
            state,
            clock,
            program,
            group,
            targets,
            pd,
            deployment: baseline,
            round,
        });
    }
    Err(last_reason.into())
}
pub fn collect<F>(config: &RuntimeConfig, read: F) -> Result<Fe01Report, String>
where
    F: FnMut(&[Pubkey], u64) -> Result<(u64, ObservedState), String>,
{
    Ok(report(&collect_scope(config, read, false)?))
}

/// FE-01: the observation report and the one-class freeze of the same closed scope (the freeze
/// error, if any, is the reason it is blocked; the report is still evidence).
pub fn collect_frozen<F>(
    config: &RuntimeConfig,
    read: F,
    class: darkbot_project0::funding::FundingClass,
    historical: bool,
) -> Result<
    (
        Fe01Report,
        Result<crate::analytics::payload::Fe01Freeze, String>,
    ),
    String,
>
where
    F: FnMut(&[Pubkey], u64) -> Result<(u64, ObservedState), String>,
{
    let scope = collect_scope(config, read, false)?;
    let freeze = super::fe_freeze::freeze(&scope, class, historical);
    Ok((report(&scope), freeze))
}

/// FE-01 smoke status (never a qualification) and its own journal beside the typed reads.
pub const SMOKE_OBSERVED: &str = "SMOKE_OBSERVED_NOT_QUALIFIED";
pub const FE01_SMOKE_JOURNAL: &str = "fe01_smoke.p0j";

/// One read of the smoke: the typed answer, its context slot not below `floor` and one entry
/// per key; counted in `s`.
fn smoke_read<F>(
    read: &mut F,
    s: &mut Fe01Smoke,
    keys: &[Pubkey],
    floor: u64,
) -> Result<(u64, Vec<Option<Fe01Account>>), String>
where
    F: FnMut(&[Pubkey], u64) -> Result<Fe01Read, String>,
{
    s.logical_reads += 1;
    let r = read(keys, floor)?;
    match r.outcome {
        Fe01ReadOutcome::Accounts {
            context_slot,
            accounts,
        } => {
            if context_slot == 0 || context_slot < floor {
                return Err("FE_CONTEXT_REGRESSION".into());
            }
            if accounts.len() != keys.len() {
                return Err("FE_BATCH_LENGTH".into());
            }
            s.context_slots.push(context_slot);
            Ok((context_slot, accounts))
        }
        Fe01ReadOutcome::Failed { reason } => Err(reason),
    }
}

/// FE-01 smoke (`--fe-mode smoke`, the P1A deployment/target observation): the program, its
/// ProgramData and deployment (hash, slot, pin against the qualified reference), then every
/// configured target (present, owner, size, hash) — three typed reads whose context slots never
/// regress. A missing program or ProgramData, a regression or a failed read makes it INCOMPLETE
/// (never a partial pass); targets report MISSING/FOREIGN_OWNER as observed facts.
pub fn smoke<F>(config: &RuntimeConfig, mut read: F) -> Fe01Smoke
where
    F: FnMut(&[Pubkey], u64) -> Result<Fe01Read, String>,
{
    let mut s = Fe01Smoke {
        status: INCOMPLETE.into(),
        reason: None,
        program: Pubkey::default(),
        program_data: None,
        program_data_sha256: None,
        deployment_slot: None,
        deployment_pin: None,
        context_slots: vec![],
        targets: vec![],
        logical_reads: 0,
        provider_actual: None,
        pilot_ready: false,
        live_enabled: false,
        submission_enabled: false,
    };
    let result = (|| -> Result<(), String> {
        let program = key(protocol::PROGRAM_ID_MAINNET)?;
        s.program = program;
        let targets = config
            .fe_targets
            .iter()
            .map(|t| key(t))
            .collect::<Result<Vec<_>, _>>()?;
        if targets.is_empty()
            || targets.len() as u64 > config.limits.fe_batch_size
            || targets.contains(&Pubkey::default())
            || targets.iter().collect::<BTreeSet<_>>().len() != targets.len()
        {
            return Err("FE_TARGET_SCOPE_INVALID".into());
        }
        let (slot, accounts) = smoke_read(&mut read, &mut s, &[program], 0)?;
        let a = accounts[0].as_ref().ok_or("FE_PROGRAM_MISSING")?;
        let pd = protocol::decode_program(&program, &a.owner, a.executable, &a.data)
            .map_err(|_| "FE_PROGRAM_LAYOUT")?;
        s.program_data = Some(pd);
        let (slot, accounts) = smoke_read(&mut read, &mut s, &[pd], slot)?;
        let a = accounts[0].as_ref().ok_or("FE_PROGRAMDATA_MISSING")?;
        let d = protocol::decode_program_data(&pd, &a.owner, a.executable, &a.data)
            .map_err(|_| "FE_PROGRAMDATA_LAYOUT")?;
        if d.deployment_slot > slot {
            return Err("FE_DEPLOYMENT_FROM_FUTURE".into());
        }
        s.program_data_sha256 = Some(sha(&a.data));
        s.deployment_slot = Some(d.deployment_slot);
        s.deployment_pin = Some(
            match protocol::deployment_pin(&program, &d) {
                protocol::DeploymentPin::Reference => "REFERENCE",
                protocol::DeploymentPin::NotReference => "NOT_REFERENCE",
                protocol::DeploymentPin::Unpinned => "UNPINNED",
            }
            .into(),
        );
        let (_, accounts) = smoke_read(&mut read, &mut s, &targets, slot)?;
        s.targets = targets
            .iter()
            .zip(accounts)
            .map(|(account, a)| match a {
                None => Fe01SmokeTarget {
                    account: *account,
                    status: "MISSING".into(),
                    bytes: None,
                    sha256: None,
                },
                Some(a) => Fe01SmokeTarget {
                    account: *account,
                    status: if a.owner == program {
                        "OBSERVED_NOT_HEALTH_QUALIFIED"
                    } else {
                        "FOREIGN_OWNER"
                    }
                    .into(),
                    bytes: Some(a.data.len() as u64),
                    sha256: Some(sha(&a.data)),
                },
            })
            .collect();
        // Review 6: every configured target must exist and belong to the program (as the
        // removed Python smoke required); the per-target statuses stay recorded.
        if s.targets
            .iter()
            .any(|t| t.status != "OBSERVED_NOT_HEALTH_QUALIFIED")
        {
            return Err("ACCOUNT_OWNER_MISMATCH".into());
        }
        Ok(())
    })();
    match result {
        Ok(()) => s.status = SMOKE_OBSERVED.into(),
        Err(e) => s.reason = Some(e),
    }
    s
}

/// FE-01 report status of a closed, observed scope (never a qualification).
pub const OBSERVED: &str = "STATE_SCOPE_OBSERVED_NOT_QUALIFIED";
pub const INCOMPLETE: &str = "INCOMPLETE";

/// The report of an observation that ended before a scope closed (or ran out of time).
pub fn incomplete(reason: String) -> Fe01Report {
    Fe01Report {
        status: INCOMPLETE.into(),
        reason: Some(reason),
        observation: None,
        scope: None,
        freeze: None,
        logical_reads: None,
        provider_actual: None,
        provider_cooldown: None,
        replay: None,
        source_parity: "UNKNOWN".into(),
        fe01: "INCOMPLETE".into(),
        fe02: "NOT_RUN".into(),
        pilot_ready: false,
        live_enabled: false,
        submission_enabled: false,
    }
}

fn report(scope: &CollectedScope) -> Fe01Report {
    let CollectedScope {
        state,
        clock,
        program,
        group,
        targets,
        pd,
        deployment: baseline,
        round,
    } = scope;
    let (program, group) = (*program, *group);
    let health = CrateHealth { state, group };
    let sizer = ProtocolSizer {
        state,
        program,
        group,
        headroom_bps: darkbot_project0::sizing::DEFAULT_HEADROOM_BPS,
    };
    let mut rows = Vec::new();
    for target in targets {
        let h = health.health(target, clock.unix_timestamp, clock.slot);
        let eligibility = health.receivership_eligibility(target, clock.unix_timestamp, clock.slot);
        let mut row = Fe01Target {
            account: *target,
            health: "UNKNOWN".into(),
            liquidatable: None,
            maintenance_health_bits: None,
            receivership_eligibility: if eligibility.is_ok() { "PASS" } else { "FAIL" }.into(),
            eligibility_reason: eligibility.as_ref().err().map(|e| e.to_string()),
            funding: "UNKNOWN".into(),
            net_edge_usd_micro: None,
            simulation: "NOT_RUN".into(),
            collateral_exit: "UNKNOWN".into(),
            source_parity: "UNKNOWN".into(),
            protocol_sizes: None,
            sizing_error: None,
            health_error: None,
        };
        match h {
            Ok(h) => {
                row.health = "EVALUATED_WITH_PINNED_CORE".into();
                row.liquidatable = Some(h.liquidatable);
                row.maintenance_health_bits = Some(h.maintenance_health);
                if h.liquidatable && eligibility.is_ok() {
                    match sizer.sizes(target, clock.unix_timestamp, clock.slot) {
                        Ok(sizes) => {
                            row.protocol_sizes = Some(
                                sizes
                                    .into_iter()
                                    .map(|s| Fe01Size {
                                        asset_bank: s.asset_bank,
                                        liability_bank: s.liab_bank,
                                        withdraw_native: s.withdraw_amount,
                                        repay_native: s.repay_amount,
                                        repay_all: s.repay_all,
                                        flat_sol_fee_lamports: s.flat_sol_fee_lamports,
                                        executable_value: "UNKNOWN".into(),
                                    })
                                    .collect(),
                            )
                        }
                        Err(e) => row.sizing_error = Some(e.text),
                    }
                }
            }
            Err(e) => row.health_error = Some(e.to_string()),
        }
        rows.push(row);
    }
    let accounts = state
        .0
        .iter()
        .map(|(key, a)| Fe01AccountDigest {
            pubkey: *key,
            owner: a.owner,
            lamports: a.lamports,
            bytes: a.data.len() as u64,
            sha256: sha(&a.data),
        })
        .collect();
    Fe01Report {
        status: OBSERVED.into(),
        reason: None,
        observation: Some(Fe01Observation {
            rounds: *round,
            slot: clock.slot,
            unix_timestamp: clock.unix_timestamp,
            clock: "CHAIN_SYSVAR_SAME_CONTEXT".into(),
            program_data: *pd,
            program_data_sha256: baseline.1.clone(),
            deployment_slot: baseline.0,
            source_pin: SOURCE_PIN.into(),
            source_parity_reason: "CURRENT_PROGRAM_BYTES_NOT_A_REPRODUCIBLE_SOURCE_BUILD_PROOF"
                .into(),
            transaction_version: TRANSACTION_VERSION_PIN.into(),
            accounts,
            targets: rows,
        }),
        ..incomplete(String::new())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use bytemuck::Zeroable;
    #[test]
    fn provider_evidence_requires_fresh_free_capacity_for_every_fallback() {
        let mut c = RuntimeConfig::default();
        let primary = "http://primary.invalid";
        let proof = |url: &str| ProviderProof {
            policy: "FREE_ONLY".into(),
            passed: true,
            provider_binding: sha(url.as_bytes()),
            measured_at_unix_s: 1000.0,
            evidence_sha256: Some(sha(b"fixture")),
            allowed_rps: 5.0,
            available_calls: 3,
        };
        let evidence = |providers: Vec<ProviderProof>| ProviderEvidence {
            providers,
            wss_provider_binding: None,
            wss_free_only: false,
            archive_qualified: false,
            source_sha256: sha(b"measured"),
        };
        let p = evidence(vec![proof(primary)]);
        assert!(validate_provider_evidence(&c, primary, &p, 1001.0).is_ok());
        type Mutation = (&'static str, fn(&mut ProviderProof));
        let mutations: [Mutation; 9] = [
            ("policy", |p| p.policy = "PAID".into()),
            ("passed", |p| p.passed = false),
            ("allowed_rps", |p| p.allowed_rps = 0.0),
            ("allowed_rps NaN", |p| p.allowed_rps = f64::NAN),
            ("available_calls", |p| p.available_calls = 2),
            ("evidence_sha256", |p| {
                p.evidence_sha256 = Some("missing".into())
            }),
            ("evidence_sha256 null", |p| p.evidence_sha256 = None),
            ("provider_binding", |p| p.provider_binding = "wrong".into()),
            ("measured_at_unix_s", |p| p.measured_at_unix_s = 1002.0),
        ];
        for (field, mutate) in mutations {
            let mut invalid = p.clone();
            mutate(&mut invalid.providers[0]);
            assert!(
                validate_provider_evidence(&c, primary, &invalid, 1001.0).is_err(),
                "{field}"
            );
        }
        assert!(validate_provider_evidence(
            &c,
            primary,
            &p,
            1001.0 + c.limits.fe_qualification_age_seconds as f64
        )
        .is_err());
        c.rpc_endpoints = vec![primary.into(), "http://backup.invalid".into()];
        c.fallback = true;
        assert!(validate_provider_evidence(&c, primary, &p, 1001.0).is_err());
        let proofs = evidence(vec![proof(primary), proof("http://backup.invalid")]);
        assert!(validate_provider_evidence(&c, primary, &proofs, 1001.0).is_ok());
        c.fallback = false;
        assert!(validate_provider_evidence(&c, primary, &p, 1001.0).is_ok());
    }
    /// X2: the measured JSON is imported once, offline, into the typed record; the record round
    /// trips through the journal and fails closed on schema, corruption and truncation.
    #[test]
    fn provider_evidence_round_trips_as_typed_journal() {
        use crate::analytics::{journal, payload::Payload};
        let example = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../config/examples/provider_qualification.example.json"),
        )
        .unwrap();
        let single = ProviderEvidence::from_measured_json(&example).unwrap();
        assert_eq!(single.providers.len(), 1);
        assert!(!single.providers[0].passed && !single.wss_free_only);
        assert_eq!(single.source_sha256, sha(&example));
        let bundle = br#"{"providers":[{"policy":"FREE_ONLY","passed":true,"provider_binding":"b","measured_at_unix_s":1,"evidence_sha256":"e","allowed_rps":5,"available_calls":3}],"wss_provider_binding":"w","wss_free_only":true,"archive_qualified":false}"#;
        let typed = ProviderEvidence::from_measured_json(bundle).unwrap();
        assert_eq!(typed.wss_provider_binding.as_deref(), Some("w"));
        for bad in [
            br#"{"providers":[],"wss_free_only":true}"#.as_slice(),
            br#"{"policy":"FREE_ONLY","passed":true}"#,
            br#"{"providers":[{"policy":"FREE_ONLY","passed":true,"provider_binding":"b","measured_at_unix_s":1,"evidence_sha256":"e","allowed_rps":5,"available_calls":3,"extra":1}]}"#,
            br#"{"unknown":1}"#,
            b"[]",
        ] {
            assert_eq!(
                ProviderEvidence::from_measured_json(bad).unwrap_err(),
                "PROVIDER_EVIDENCE_FORMAT"
            );
        }
        let dir = std::env::temp_dir().join(format!(
            "p0_provider_evidence_{}_{}",
            std::process::id(),
            Pubkey::new_unique()
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("provider.p0j");
        let mut w = journal::Writer::create(&path).unwrap();
        w.append_typed(journal::Provenance::default(), &typed)
            .unwrap();
        w.seal(true).unwrap();
        assert_eq!(
            journal::read_typed_value::<ProviderEvidence>(&path).unwrap(),
            typed
        );
        let bytes = crate::analytics::payload::encode(&typed).unwrap();
        assert!(crate::analytics::payload::decode::<ProviderEvidence>(
            ProviderEvidence::KIND,
            ProviderEvidence::SCHEMA + 1,
            &bytes
        )
        .is_err());
        let good = std::fs::read(&path).unwrap();
        let mut flipped = good.clone();
        let at = good.len() / 2;
        flipped[at] ^= 1;
        for damaged in [flipped, good[..good.len() - 1].to_vec()] {
            std::fs::write(&path, damaged).unwrap();
            assert!(journal::read_typed_value::<ProviderEvidence>(&path).is_err());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    /// The required read count is the collector's own: one round over the initial scope in
    /// single-key batches reads exactly that many accounts; a further round over the discovered
    /// dependencies reads more (dynamic, bounded by the caller's hard limit).
    #[test]
    fn required_scope_reads_match_the_collector() {
        let (mut c, all) = fixture();
        c.limits.fe_batch_size = 1;
        let count = |c: &RuntimeConfig| {
            let mut reads = 0u64;
            let result = collect_scope(
                c,
                |keys, _| {
                    reads += 1;
                    Ok((
                        100,
                        ObservedState(keys.iter().map(|k| (*k, all[k].clone())).collect()),
                    ))
                },
                true,
            );
            (reads, result.err())
        };
        c.limits.fe_snapshot_rounds = 1;
        let (reads, err) = count(&c);
        assert_eq!(err.as_deref(), Some("FE_SCOPE_NOT_CLOSED"));
        assert_eq!(reads, required_scope_reads(&c).unwrap());
        assert_eq!(reads, 2 + FIXED_SCOPE_KEYS + c.fe_targets.len() as u64);
        c.limits.fe_snapshot_rounds = 2;
        let (reads, _) = count(&c);
        assert!(reads > required_scope_reads(&c).unwrap(), "{reads}");
        c.limits.fe_batch_size = 0;
        assert!(required_scope_reads(&c).is_err());
    }
    fn wrapped<T: bytemuck::Pod>(v: T, disc: [u8; 8], owner: Pubkey) -> Account {
        Account {
            owner,
            lamports: 1,
            data: [disc.as_slice(), bytemuck::bytes_of(&v)].concat(),
            ..Account::default()
        }
    }
    /// FE-01 smoke (P1A): program, ProgramData and targets over three typed reads; the deployment
    /// pin against the qualified reference; MISSING/FOREIGN_OWNER targets are observed facts; a
    /// missing ProgramData or a regressing context slot is INCOMPLETE, never a partial pass.
    /// U-8 (PERSIST-04): the binary layout of `FE01_SMOKE` (704) is pinned. A failing pin is a
    /// layout change: bump `SCHEMA`, keep old frames readable, renew the pin — never silently.
    #[test]
    fn fe01_smoke_layout_is_pinned() {
        use crate::analytics::payload::{encode, Fe01Smoke, Fe01SmokeTarget, Payload};
        let key = |b: u8| Pubkey::new_from_array([b; 32]);
        let s = Fe01Smoke {
            status: "INCOMPLETE".into(),
            reason: Some("FE01_SMOKE_TARGET_MISSING".into()),
            program: key(1),
            program_data: Some(key(2)),
            program_data_sha256: Some("sha".into()),
            deployment_slot: Some(7),
            deployment_pin: Some("UNPINNED".into()),
            context_slots: vec![100, 101],
            targets: vec![Fe01SmokeTarget {
                account: key(3),
                status: "MISSING".into(),
                bytes: None,
                sha256: None,
            }],
            logical_reads: 2,
            provider_actual: None,
            pilot_ready: false,
            live_enabled: false,
            submission_enabled: false,
        };
        let hash = solana_sdk::hash::hash(&encode(&s).unwrap()).to_string();
        assert_eq!(
            (Fe01Smoke::KIND, Fe01Smoke::SCHEMA, hash.as_str()),
            (704, 2, "5M2J1tN1RGCRqhYeUCnCAEH5rVXTegizcByUvRxY6c2E")
        );
    }

    #[test]
    fn smoke_observes_deployment_and_targets_and_fails_closed() {
        let (mut c, mut state) = fixture();
        c.fe_mode = "smoke".into();
        let foreign = Pubkey::new_unique();
        state.insert(
            foreign,
            Account {
                owner: Pubkey::new_unique(),
                data: vec![1, 2, 3],
                ..Account::default()
            },
        );
        c.fe_targets.push(foreign.to_string());
        let missing = key(&c.fe_targets[1]).unwrap();
        state.remove(&missing);
        let reads = |state: &BTreeMap<Pubkey, Account>, slots: Vec<u64>| {
            let state = state.clone();
            let mut slots = slots.into_iter();
            move |keys: &[Pubkey], floor: u64| -> Result<Fe01Read, String> {
                Ok(Fe01Read {
                    keys: keys.to_vec(),
                    min_context_slot: floor,
                    outcome: Fe01ReadOutcome::Accounts {
                        context_slot: slots.next().ok_or("NO_MORE_READS")?,
                        accounts: keys
                            .iter()
                            .map(|k| {
                                state.get(k).map(|a| Fe01Account {
                                    owner: a.owner,
                                    lamports: a.lamports,
                                    executable: a.executable,
                                    data: a.data.clone(),
                                })
                            })
                            .collect(),
                    },
                })
            }
        };
        // Review 6 (parity with the removed Python smoke): a missing or foreign-owned target fails
        // the smoke (ACCOUNT_OWNER_MISMATCH); every target's status stays recorded.
        let s = smoke(&c, reads(&state, vec![100, 101, 102]));
        assert_eq!(
            (s.status.as_str(), s.reason.as_deref()),
            (INCOMPLETE, Some("ACCOUNT_OWNER_MISMATCH"))
        );
        let mut owned = c.clone();
        owned.fe_targets.truncate(1);
        let ok = smoke(&owned, reads(&state, vec![100, 101, 102]));
        assert_eq!(
            (ok.status.as_str(), ok.reason.as_deref()),
            (SMOKE_OBSERVED, None)
        );
        assert_eq!((s.deployment_slot, s.logical_reads), (Some(80), 3));
        assert_eq!(s.context_slots, [100, 101, 102]);
        assert_eq!(s.deployment_pin.as_deref(), Some("NOT_REFERENCE"));
        let statuses: Vec<_> = s.targets.iter().map(|t| t.status.as_str()).collect();
        assert_eq!(
            statuses,
            ["OBSERVED_NOT_HEALTH_QUALIFIED", "MISSING", "FOREIGN_OWNER"]
        );
        assert!(!(s.pilot_ready || s.live_enabled || s.submission_enabled));
        let regressed = smoke(&c, reads(&state, vec![100, 99, 102]));
        assert_eq!(
            (regressed.status.as_str(), regressed.reason.as_deref()),
            (INCOMPLETE, Some("FE_CONTEXT_REGRESSION"))
        );
        let pd = s.program_data.unwrap();
        state.remove(&pd);
        let no_pd = smoke(&c, reads(&state, vec![100, 101, 102]));
        assert_eq!(no_pd.reason.as_deref(), Some("FE_PROGRAMDATA_MISSING"));
        assert_eq!(no_pd.status, INCOMPLETE);
    }

    pub(crate) fn fixture() -> (RuntimeConfig, BTreeMap<Pubkey, Account>) {
        let c = RuntimeConfig {
            fe_mode: "full".into(),
            ..RuntimeConfig::default()
        };
        let program = key(protocol::PROGRAM_ID_MAINNET).unwrap();
        let group = key(&c.fe_group).unwrap();
        let loader = key(LOADER).unwrap();
        let pd = Pubkey::find_program_address(&[program.as_ref()], &loader).0;
        let bank_key = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let vault = Pubkey::new_unique();
        let oracle = Pubkey::new_unique();
        let mut state = BTreeMap::new();
        state.insert(
            program,
            Account {
                owner: loader,
                executable: true,
                data: [2u32.to_le_bytes().as_slice(), pd.as_ref()].concat(),
                ..Account::default()
            },
        );
        let mut data = vec![0; 45];
        data[..4].copy_from_slice(&3u32.to_le_bytes());
        data[4..12].copy_from_slice(&80u64.to_le_bytes());
        state.insert(
            pd,
            Account {
                owner: loader,
                data,
                ..Account::default()
            },
        );
        state.insert(
            group,
            wrapped(
                MarginfiGroup::zeroed(),
                protocol::DISC_MARGINFI_GROUP,
                program,
            ),
        );
        state.insert(
            protocol::fee_state_pda(&program),
            wrapped(FeeState::zeroed(), protocol::DISC_FEE_STATE, program),
        );
        for target in &c.fe_targets {
            let mut a = MarginfiAccount::zeroed();
            a.group = group;
            a.lending_account.balances[0].active = 1;
            a.lending_account.balances[0].bank_pk = bank_key;
            state.insert(
                key(target).unwrap(),
                wrapped(a, protocol::DISC_MARGINFI_ACCOUNT, program),
            );
        }
        let mut bank = Bank::zeroed();
        bank.group = group;
        bank.mint = mint;
        bank.liquidity_vault = vault;
        bank.config.oracle_setup = marginfi_type_crate::types::OracleSetup::PythPushOracle;
        bank.config.oracle_keys[0] = oracle;
        state.insert(bank_key, wrapped(bank, protocol::DISC_BANK, program));
        for k in [mint, vault, oracle] {
            state.insert(
                k,
                Account {
                    owner: key(protocol::TOKEN_PROGRAM_ID).unwrap(),
                    lamports: 1,
                    data: vec![0; 82],
                    ..Account::default()
                },
            );
        }
        state.insert(
            solana_sdk::sysvar::clock::id(),
            Account {
                owner: solana_sdk::sysvar::id(),
                data: bincode::serialize(&Clock {
                    slot: 100,
                    unix_timestamp: 1000,
                    ..Clock::default()
                })
                .unwrap(),
                ..Account::default()
            },
        );
        (c, state)
    }
    fn batch(all: &BTreeMap<Pubkey, Account>, keys: &[Pubkey]) -> Result<ObservedState, String> {
        Ok(ObservedState(
            keys.iter()
                .map(|k| {
                    all.get(k)
                        .cloned()
                        .map(|v| (*k, v))
                        .ok_or("FE_ACCOUNT_MISSING".to_string())
                })
                .collect::<Result<_, _>>()?,
        ))
    }
    #[test]
    fn scope_expands_beyond_three_calls_using_pinned_dependencies() {
        let (c, all) = fixture();
        let mut calls = 0;
        let mut seen = BTreeSet::new();
        let report = collect(&c, |keys, floor| {
            assert!(floor <= 100);
            calls += 1;
            seen.extend(keys.iter().copied());
            Ok((100, batch(&all, keys)?))
        })
        .unwrap();
        assert!(calls > 3);
        assert_eq!(report.status, OBSERVED);
        assert_eq!(seen.len(), all.len());
        assert_eq!(report.source_parity, "UNKNOWN");
        assert_eq!(report.fe01, "INCOMPLETE");
        assert!(!report.pilot_ready && !report.live_enabled && !report.submission_enabled);
        // Zero-share balances may evaluate to zero health even with unused oracles.
        // That does not establish an executable economic candidate.
        let observation = report.observation.unwrap();
        assert_eq!(observation.targets[0].liquidatable, Some(false));
        assert_eq!(observation.targets[0].net_edge_usd_micro, None);
    }
    #[test]
    fn integration_accounts_of_target_banks_are_observed_in_the_same_scope() {
        let (c, mut all) = fixture();
        let program = key(protocol::PROGRAM_ID_MAINNET).unwrap();
        let bank_key = *all
            .iter()
            .find(|(_, a)| {
                a.owner == program && protocol::classify(&a.data) == protocol::AccountType::Bank
            })
            .unwrap()
            .0;
        // The bank names one integration account (for example a Kamino reserve).
        let reserve = Pubkey::new_unique();
        let size = std::mem::size_of::<Bank>();
        let mut bank: Bank = bytemuck::pod_read_unaligned(&all[&bank_key].data[8..8 + size]);
        bank.integration_acc_1 = reserve;
        all.insert(bank_key, wrapped(bank, protocol::DISC_BANK, program));
        all.insert(
            reserve,
            Account {
                owner: Pubkey::new_unique(),
                lamports: 1,
                data: vec![0; 64],
                ..Account::default()
            },
        );
        let mut seen = BTreeSet::new();
        let report = collect(&c, |keys, _| {
            seen.extend(keys.iter().copied());
            Ok((100, batch(&all, keys)?))
        })
        .unwrap();
        assert!(seen.contains(&reserve));
        let observation = report.observation.unwrap();
        assert!(observation.accounts.iter().any(|a| a.pubkey == reserve));
        assert_eq!(observation.transaction_version, TRANSACTION_VERSION_PIN);
        // Without the integration account the scope never closes: fail closed, never skipped.
        all.remove(&reserve);
        assert!(collect(&c, |keys, _| Ok((100, batch(&all, keys)?))).is_err());
    }
    #[test]
    fn changed_deployment_and_missing_accounts_fail_closed() {
        let (c, all) = fixture();
        let mut calls = 0;
        let result = collect(&c, |keys, _| {
            calls += 1;
            let mut s = batch(&all, keys)?;
            if calls > 2 {
                for a in s.0.values_mut() {
                    if a.owner == key(LOADER).unwrap() && !a.executable {
                        a.data.push(0);
                    }
                }
            }
            Ok((100, s))
        });
        assert_eq!(result.unwrap_err(), "FE_PROGRAM_UPGRADE_DURING_OBSERVATION");
        let mut missing = all;
        missing.remove(&key(&c.fe_targets[0]).unwrap());
        assert!(collect(&c, |keys, _| Ok((100, batch(&missing, keys)?)))
            .unwrap_err()
            .contains("MISSING"));
    }
    #[test]
    fn mixed_contexts_and_round_budget_do_not_invent_coherent_state() {
        let (mut c, all) = fixture();
        c.limits.fe_snapshot_rounds = 1;
        assert_eq!(
            collect(&c, |keys, _| Ok((100, batch(&all, keys)?))).unwrap_err(),
            "FE_SCOPE_NOT_CLOSED"
        );
        c.limits.fe_snapshot_rounds = 5;
        c.limits.fe_batch_size = 2;
        let mut slot = 100;
        assert_eq!(
            collect(&c, |keys, _| {
                slot += 1;
                Ok((slot, batch(&all, keys)?))
            })
            .unwrap_err(),
            "FE_CONTEXT_SLOTS_NOT_COHERENT"
        );
    }
    /// Only the pinned exact Bank layout (0.1.12: 8 + 3904 B) is collected: a non-resized
    /// 0.1.11 bank (8 + 1856 B) or any compatibility padding is a layout mismatch.
    #[test]
    fn exact_bank_layout_rejects_compatibility_padding() {
        for len in [8 + protocol::BANK_PAYLOAD_LEN_0_1_11, 3920] {
            let (c, mut all) = fixture();
            let b = all
                .values_mut()
                .find(|a| a.data.get(..8) == Some(protocol::DISC_BANK.as_slice()))
                .unwrap();
            assert_eq!(b.data.len(), 8 + protocol::BANK_PAYLOAD_LEN_0_1_12);
            b.data.resize(len, 0);
            assert!(
                collect(&c, |keys, _| Ok((100, batch(&all, keys)?)))
                    .unwrap_err()
                    .contains("EXACT_LAYOUT_MISMATCH"),
                "{len}"
            );
        }
    }
    /// JFI-02: the `getMultipleAccounts` answer decodes into the wire type at the edge; the typed
    /// read keeps the exact bytes and fails closed on a regressed context, a missing or
    /// non-base64 account, an invalid owner and a length mismatch.
    #[test]
    fn wire_decoder_preserves_bytes_and_rejects_regressed_or_missing_context() {
        let k = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let raw = vec![1, 2, 3];
        let wire = |text: String| -> WireMultipleAccounts { serde_json::from_str(&text).unwrap() };
        let answer = |owner: &str, encoding: &str| {
            format!(
                r#"{{"context":{{"slot":12}},"value":[{{"owner":"{owner}","lamports":45,"executable":false,"rentEpoch":0,"data":["{}","{encoding}"]}}]}}"#,
                base64::engine::general_purpose::STANDARD.encode(&raw)
            )
        };
        let read = |floor, outcome| Fe01Read {
            keys: vec![k],
            min_context_slot: floor,
            outcome,
        };
        let outcome = wire(answer(&owner.to_string(), "base64")).outcome();
        let (slot, state) = observed(&read(11, outcome.clone())).unwrap();
        assert_eq!(slot, 12);
        assert_eq!(state.0[&k].data, raw);
        assert_eq!(state.0[&k].owner, owner);
        assert_eq!(
            observed(&read(13, outcome)).unwrap_err(),
            "FE_CONTEXT_REGRESSION"
        );
        for (answer, code) in [
            (answer(&owner.to_string(), "base58"), "FE_ACCOUNT_ENCODING"),
            (answer("not-a-key", "base64"), "FE_PUBKEY_INVALID"),
            (
                r#"{"context":{"slot":12},"value":[null]}"#.to_string(),
                "FE_ACCOUNT_MISSING",
            ),
            (
                r#"{"context":{"slot":12},"value":[]}"#.to_string(),
                "FE_BATCH_LENGTH",
            ),
        ] {
            let e = observed(&read(11, wire(answer).outcome())).unwrap_err();
            assert!(e.starts_with(code), "{e}");
        }
        assert!(serde_json::from_str::<WireMultipleAccounts>(r#"{"value":[]}"#).is_err());
    }
    #[test]
    fn invalid_scope_and_callback_context_fail_closed_before_qualification() {
        let (c, all) = fixture();
        let mut bad = c.clone();
        bad.fe_targets.clear();
        assert!(collect(&bad, |_, _| panic!("must not read")).is_err());
        bad = c.clone();
        bad.fe_targets.push(bad.fe_targets[0].clone());
        assert!(collect(&bad, |_, _| panic!("must not read")).is_err());
        bad = c.clone();
        bad.limits.fe_batch_size = 0;
        assert!(collect(&bad, |_, _| panic!("must not read")).is_err());
        assert_eq!(
            collect(&c, |keys, _| Ok((0, batch(&all, keys)?))).unwrap_err(),
            "FE_CONTEXT_REGRESSION"
        );
        assert_eq!(
            collect(&c, |_, _| Ok((100, ObservedState::default()))).unwrap_err(),
            "FE_BATCH_SCOPE_MISMATCH"
        );
    }
    /// FE-01 replay of one saved observation from each source — legacy `read_NNNNNN.json` files,
    /// a historical schema-1 journal (FE-01 smoke era) and the typed journal (JFI-04) — yields
    /// the identical observation, stays historical, unqualified and bound to its exact reads.
    #[test]
    fn saved_full_observation_replays_identically_but_stays_historical_and_incomplete() {
        use super::super::fe_replay;
        use crate::analytics::journal::{kind, Writer};
        use crate::analytics::payload::Fe01Config;
        let (mut c, all) = fixture();
        c.limits.fe_calls = 100;
        c.limits.max_requests = 100;
        let dir = std::env::temp_dir().join(format!("p0-fe-replay-{}", Pubkey::new_unique()));
        std::fs::create_dir(&dir).unwrap();
        // Historical format of a saved read, as the pre-JFI-04 writer saved it.
        let mut saved = Vec::new();
        let original = collect(&c, |keys, floor| {
            let state = batch(&all, keys)?;
            let values: Vec<_> = keys
                .iter()
                .map(|k| {
                    let a = &state.0[k];
                    serde_json::json!({"owner":a.owner.to_string(),"lamports":a.lamports,"executable":a.executable,"data":[base64::engine::general_purpose::STANDARD.encode(&a.data),"base64"]})
                })
                .collect();
            let entry = serde_json::json!({"method":"getMultipleAccounts","params":[keys.iter().map(ToString::to_string).collect::<Vec<_>>(),{"encoding":"base64","commitment":"finalized","minContextSlot":floor}],"result":{"context":{"slot":100},"value":values}});
            saved.push((entry, keys.to_vec(), floor, state.0.clone()));
            let n = saved.len();
            std::fs::write(dir.join(format!("read_{n:06}.json")), serde_json::to_vec(&saved[n - 1].0).unwrap()).unwrap();
            Ok((100, state))
        })
        .unwrap();
        let calls = saved.len();
        let class = fe_replay::first_class("ZERO_PREFUND_SAME_TX").unwrap();
        // A14: the pre-PERSIST-01 read files are a historical source, read only on request.
        assert!(fe_replay::replay(&c, &dir, class, false)
            .unwrap_err()
            .starts_with("FE_REPLAY_JOURNAL_MISSING"));
        let replayed = fe_replay::replay(&c, &dir, class, true).unwrap();
        assert_eq!(replayed.observation, original.observation);
        let replay = replayed.replay.as_ref().unwrap();
        assert_eq!(
            (replay.freshness.as_str(), replay.source.as_str()),
            ("HISTORICAL_NOT_CURRENT", "LEGACY_READ_FILES")
        );
        assert!(!replayed.pilot_ready);
        assert_eq!(
            replayed.scope.as_ref().unwrap().funding_class,
            "ZERO_PREFUND_SAME_TX"
        );
        let effective = darkbot_solana::runtime_config::Resolved::parse(&[], &|_| None)
            .unwrap()
            .effective();
        // The same observation as a historical schema-1 journal and as the typed journal.
        for typed in [false, true] {
            let binary = dir.with_extension(if typed { "typed" } else { "historical" });
            std::fs::create_dir(&binary).unwrap();
            let mut w = Writer::create(&binary.join(fe_replay::FE01_JOURNAL)).unwrap();
            if typed {
                let config = Fe01Config {
                    requested: effective.clone(),
                    effective: effective.clone(),
                    qualification_sha256: Some("q".into()),
                    execution: "READ_ONLY_NO_SIGNER".into(),
                };
                w.append_typed(Default::default(), &config).unwrap();
                for (_, keys, floor, state) in &saved {
                    let read = Fe01Read {
                        keys: keys.clone(),
                        min_context_slot: *floor,
                        outcome: Fe01ReadOutcome::Accounts {
                            context_slot: 100,
                            accounts: keys
                                .iter()
                                .map(|k| {
                                    let a = &state[k];
                                    Some(Fe01Account {
                                        owner: a.owner,
                                        lamports: a.lamports,
                                        executable: a.executable,
                                        data: a.data.clone(),
                                    })
                                })
                                .collect(),
                        },
                    };
                    w.append_typed(Default::default(), &read).unwrap();
                }
            } else {
                w.test_append_historical(
                    kind::FE01_CONFIG,
                    &serde_json::json!({"execution":"READ_ONLY_NO_SIGNER"}),
                )
                .unwrap();
                for (entry, ..) in &saved {
                    w.test_append_historical(kind::FE01_READ, entry).unwrap();
                }
            }
            w.sync().unwrap();
            assert_eq!(
                fe_replay::replay(&c, &binary, class, false).unwrap_err(),
                "FE_REPLAY_INCOMPLETE"
            );
            if typed {
                let mut result = original.clone();
                result.logical_reads = Some(calls as u64);
                w.append_typed(Default::default(), &result).unwrap();
            } else {
                w.test_append_historical(
                    kind::FE01_RESULT,
                    &serde_json::json!({"status":OBSERVED,"logical_reads":calls}),
                )
                .unwrap();
            }
            w.seal(true).unwrap();
            let from_journal = fe_replay::replay(&c, &binary, class, false).unwrap();
            assert_eq!(
                from_journal.observation, original.observation,
                "typed={typed}"
            );
            let replay = from_journal.replay.unwrap();
            assert_eq!(replay.source, "BINARY_JOURNAL");
            assert_eq!(replay.reads.len(), calls);
            // A second primary source is refused, never merged into the sealed archive.
            std::fs::write(binary.join("read_000001.json"), b"{}").unwrap();
            assert_eq!(
                fe_replay::replay(&c, &binary, class, false).unwrap_err(),
                "FE_REPLAY_AMBIGUOUS_SOURCE"
            );
            drop(w);
            std::fs::remove_dir_all(binary).unwrap();
        }
        c.limits.fe_calls = 1;
        assert_eq!(
            fe_replay::replay(&c, &dir, class, true).unwrap_err(),
            "FE_REPLAY_CALL_LIMIT"
        );
        c.limits.fe_calls = 100;
        std::fs::write(dir.join("read_999999.json"), b"{}").unwrap();
        assert_eq!(
            fe_replay::replay(&c, &dir, class, true).unwrap_err(),
            "FE_REPLAY_UNUSED_READS"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

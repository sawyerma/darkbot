use darkbot_bsc_record::schema::g07::contract;
use darkbot_bsc_record::{Record, RecordError, decode, hex, sha256};
use std::{fs, path::Path};

/// The v3 proposal of G0.7 attempt 1, bound to the Portal v5.24.0 DRY. It
/// is superseded by v4 and can only be inspected, never authorize.
pub const V3_RELATIVE_PATH: &str = "bsc/config/g07_attempt1_acceptance_v3.dbrec";

/// The binary acceptance contract of the requalified G0.7 attempt 1
/// (DBREC001, v4): the only possible execution authority. The v3 proposal,
/// the JSON v1/v2 contracts and the JSON v2 draft never authorize.
pub const V4_RELATIVE_PATH: &str = "bsc/config/g07_attempt1_acceptance_v4.dbrec";

// Deliberately UNARMED. Only a dedicated, explicitly approved arming commit may
// replace this with the SHA-256 of the exact approved v4 record bytes.
// Environment variables and CLI flags are never execution authority.
pub const FROZEN_APPROVED_V4_SHA256: &str = "UNARMED";

/// The hashes binding one input at runtime: the SHA-256 of the binary
/// record file G0.7 read and, for an audited import, the legacy SHA-256
/// that record carries (`None` for a native record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputHashes {
    pub record_sha256: String,
    pub legacy_sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LiveGuardContext {
    pub attempt_id: String,
    pub launcher_address: String,
    pub buyer_address: String,
    pub chain_id: u64,
    pub portal_address: String,
    pub launch_value_wei: u128,
    pub buy_value_wei: u128,
    pub gas_price_wei: u128,
    pub launch_gas_limit: u64,
    pub buy_gas_limit: u64,
    pub direct_builder_tip_wei: u128,
    pub spent_budget_wei: u128,
    pub reserved_attempt_wei: u128,
    pub attempt_number: u64,
    pub dry_pass_all: bool,
    pub capture_prearmed: bool,
    pub producer_witness_contract_bound: bool,
    pub dry_evidence: InputHashes,
    pub preflight_evidence: InputHashes,
    pub builder_policy_evidence: InputHashes,
    pub producer_witness_source: InputHashes,
}

#[derive(Debug, Clone)]
pub struct ContractSnapshot {
    pub sha256: String,
    pub approved: bool,
    pub execution_ready: bool,
    pub attempt_id: String,
    pub launcher_address: String,
    pub buyer_address: String,
    pub chain_id: u64,
    pub portal_address: String,
    pub max_attempts: u64,
    pub max_buy_value_per_attempt: u128,
    pub max_launch_value_per_attempt: u128,
    pub max_launch_gas_limit: u64,
    pub max_buy_gas_limit: u64,
    pub max_gas_price_wei: u128,
    pub max_total_per_attempt: u128,
    pub absolute_budget_wei: u128,
    pub max_direct_builder_tip_wei: u128,
    pub capture_required: bool,
    pub producer_witness_contract_required: bool,
    pub dry_evidence: InputHashes,
    pub preflight_evidence: InputHashes,
    pub builder_policy_evidence: InputHashes,
    pub producer_witness_source: InputHashes,
    /// v4 only: the further bound evidence.
    pub local_simulation: Option<InputHashes>,
    pub economics: Option<InputHashes>,
    pub portal_upgrade_record_sha256: Option<String>,
    pub supersedes_contract_sha256: Option<String>,
}

fn norm_addr(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&sha256(bytes))
}

fn address(r: &Record, tag: u16) -> Result<String, RecordError> {
    Ok(format!("0x{}", hex(&r.address(tag)?)))
}

fn word_hex(r: &Record, tag: u16) -> Result<String, RecordError> {
    Ok(hex(&r.b256(tag)?))
}

/// A U256 field that must fit a u128 (every budget value does).
fn u128_of(r: &Record, tag: u16) -> Result<u128, RecordError> {
    let word = r.u256(tag)?;
    if word[..16].iter().any(|b| *b != 0) {
        return Err(RecordError(format!(
            "acceptance contract tag {tag} exceeds u128"
        )));
    }
    let mut low = [0u8; 16];
    low.copy_from_slice(&word[16..]);
    Ok(u128::from_be_bytes(low))
}

fn hashes(r: &Record, record: u16, legacy: u16) -> Result<InputHashes, RecordError> {
    Ok(InputHashes {
        record_sha256: word_hex(r, record)?,
        legacy_sha256: r.opt_b256(legacy)?.map(|w| hex(&w)),
    })
}

fn opt_word_hex(r: &Record, tag: u16) -> Result<Option<String>, RecordError> {
    if r.schema().field(tag).is_none() {
        return Ok(None);
    }
    Ok(r.opt_b256(tag)?.map(|w| hex(&w)))
}

/// The further evidence a v4 contract binds (absent in v3).
fn v4_hashes(r: &Record, record: u16) -> Result<Option<InputHashes>, RecordError> {
    Ok(opt_word_hex(r, record)?.map(|record_sha256| InputHashes {
        record_sha256,
        legacy_sha256: None,
    }))
}

fn snapshot(r: &Record, sha256: String) -> Result<ContractSnapshot, RecordError> {
    Ok(ContractSnapshot {
        local_simulation: v4_hashes(r, contract::LOCAL_SIMULATION_RECORD_SHA256)?,
        economics: v4_hashes(r, contract::ECONOMICS_RECORD_SHA256)?,
        portal_upgrade_record_sha256: opt_word_hex(r, contract::PORTAL_UPGRADE_RECORD_SHA256)?,
        supersedes_contract_sha256: opt_word_hex(r, contract::SUPERSEDES_CONTRACT_SHA256)?,
        sha256,
        approved: r.bool(contract::APPROVED)?,
        execution_ready: r.bool(contract::EXECUTION_READY)?,
        attempt_id: r.text(contract::ATTEMPT_ID)?.to_string(),
        launcher_address: address(r, contract::LAUNCHER)?,
        buyer_address: address(r, contract::BUYER)?,
        chain_id: r.u64(contract::CHAIN_ID)?,
        portal_address: address(r, contract::PORTAL)?,
        max_attempts: r.u64(contract::MAX_ATTEMPTS)?,
        max_buy_value_per_attempt: u128_of(r, contract::MAX_BUY_VALUE)?,
        max_launch_value_per_attempt: u128_of(r, contract::MAX_LAUNCH_VALUE)?,
        max_launch_gas_limit: r.u64(contract::MAX_LAUNCH_GAS_LIMIT)?,
        max_buy_gas_limit: r.u64(contract::MAX_BUY_GAS_LIMIT)?,
        max_gas_price_wei: u128_of(r, contract::MAX_GAS_PRICE_WEI)?,
        max_total_per_attempt: u128_of(r, contract::MAX_TOTAL_PER_ATTEMPT)?,
        absolute_budget_wei: u128_of(r, contract::ABSOLUTE_BUDGET)?,
        max_direct_builder_tip_wei: u128_of(r, contract::MAX_DIRECT_BUILDER_TIP_WEI)?,
        capture_required: r.bool(contract::CAPTURE_REQUIRED)?,
        producer_witness_contract_required: r.bool(contract::WITNESS_CONTRACT_REQUIRED)?,
        dry_evidence: hashes(r, contract::DRY_RECORD_SHA256, contract::DRY_LEGACY_SHA256)?,
        preflight_evidence: hashes(
            r,
            contract::PREFLIGHT_RECORD_SHA256,
            contract::PREFLIGHT_LEGACY_SHA256,
        )?,
        builder_policy_evidence: hashes(
            r,
            contract::BUILDER_POLICY_RECORD_SHA256,
            contract::BUILDER_POLICY_LEGACY_SHA256,
        )?,
        producer_witness_source: hashes(
            r,
            contract::WITNESS_SOURCE_RECORD_SHA256,
            contract::WITNESS_SOURCE_LEGACY_SHA256,
        )?,
    })
}

/// The superseded v3 proposal, for inspection only.
pub fn inspect_v3(path: &Path) -> Result<ContractSnapshot, String> {
    if path != Path::new(V3_RELATIVE_PATH) {
        return Err(format!(
            "wrong acceptance contract path: expected exactly {V3_RELATIVE_PATH}, got {}",
            path.display()
        ));
    }
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    decode(&contract::SCHEMA, &bytes)
        .and_then(|r| snapshot(&r, sha256_hex(&bytes)))
        .map_err(|e| format!("acceptance contract {}: {e}", path.display()))
}

/// A v4 contract from its exact bytes (fail-closed decode).
pub fn parse_v4(bytes: &[u8]) -> Result<ContractSnapshot, String> {
    decode(&contract::V4, bytes)
        .and_then(|r| snapshot(&r, sha256_hex(bytes)))
        .map_err(|e| format!("acceptance contract v4: {e}"))
}

pub fn inspect_v4(path: &Path) -> Result<ContractSnapshot, String> {
    if path != Path::new(V4_RELATIVE_PATH) {
        return Err(format!(
            "wrong acceptance contract path: expected exactly {V4_RELATIVE_PATH}, got {}",
            path.display()
        ));
    }
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_v4(&bytes)
}

fn verify_bound_sha256(label: &str, frozen: &str, runtime: &str) -> Result<(), String> {
    let valid = frozen.len() == 64
        && frozen.bytes().all(|b| b.is_ascii_hexdigit())
        && runtime.len() == 64
        && runtime.bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        return Err(format!(
            "invalid SHA-256 encoding for evidence binding {label}"
        ));
    }
    if !frozen.eq_ignore_ascii_case(runtime) {
        return Err(format!(
            "evidence binding mismatch for {label}: runtime={runtime} frozen={frozen}"
        ));
    }
    Ok(())
}

fn verify_input(label: &str, frozen: &InputHashes, runtime: &InputHashes) -> Result<(), String> {
    verify_bound_sha256(
        &format!("{label}_record_sha256"),
        &frozen.record_sha256,
        &runtime.record_sha256,
    )?;
    match (&frozen.legacy_sha256, &runtime.legacy_sha256) {
        (None, None) => Ok(()),
        (Some(frozen), Some(runtime)) => {
            verify_bound_sha256(&format!("{label}_legacy_sha256"), frozen, runtime)
        }
        _ => Err(format!(
            "evidence binding mismatch for {label}_legacy_sha256: one side is an import, the other native"
        )),
    }
}

pub fn verify_live_authorization(
    path: &Path,
    ctx: &LiveGuardContext,
) -> Result<ContractSnapshot, String> {
    if path != Path::new(V4_RELATIVE_PATH) {
        return Err(format!(
            "wrong acceptance contract path: expected exactly {V4_RELATIVE_PATH}, got {}",
            path.display()
        ));
    }
    // Refused before anything is read: no contract file can stand in for the
    // compiled authority.
    if FROZEN_APPROVED_V4_SHA256 == "UNARMED" {
        return Err(
            "live guard is deliberately UNARMED: no approved v4 SHA-256 is compiled in".into(),
        );
    }
    let snap = inspect_v4(path)?;
    if snap.sha256 != FROZEN_APPROVED_V4_SHA256 {
        return Err(format!(
            "v4 SHA-256 mismatch: file={} frozen={}",
            snap.sha256, FROZEN_APPROVED_V4_SHA256
        ));
    }
    if !snap.approved {
        return Err("v4 authorization.approved is not true".into());
    }
    if !snap.execution_ready {
        return Err("v4 authorization.execution_ready is not true".into());
    }
    if !ctx.dry_pass_all {
        return Err("DRY-1..DRY-4 are not all PASS".into());
    }
    if snap.capture_required && !ctx.capture_prearmed {
        return Err("required bounded capture is not pre-armed".into());
    }
    if snap.producer_witness_contract_required && !ctx.producer_witness_contract_bound {
        return Err("producer-witness schema/source contract is not bound for this attempt; actual producer access may arrive asynchronously".into());
    }

    verify_attempt_id(&snap.attempt_id, &ctx.attempt_id)?;
    verify_input("dry_evidence", &snap.dry_evidence, &ctx.dry_evidence)?;
    verify_input(
        "preflight_evidence",
        &snap.preflight_evidence,
        &ctx.preflight_evidence,
    )?;
    verify_input(
        "builder_policy_evidence",
        &snap.builder_policy_evidence,
        &ctx.builder_policy_evidence,
    )?;
    verify_input(
        "producer_witness_source",
        &snap.producer_witness_source,
        &ctx.producer_witness_source,
    )?;

    if norm_addr(&snap.launcher_address) != norm_addr(&ctx.launcher_address) {
        return Err("runtime launcher does not equal frozen v4 launcher".into());
    }
    if norm_addr(&snap.buyer_address) != norm_addr(&ctx.buyer_address) {
        return Err("runtime buyer does not equal frozen v4 buyer".into());
    }
    if norm_addr(&snap.launcher_address) == norm_addr(&snap.buyer_address) {
        return Err("launcher and buyer must be distinct".into());
    }
    if ctx.chain_id != snap.chain_id {
        return Err(format!(
            "wrong chain: runtime={} approved={}",
            ctx.chain_id, snap.chain_id
        ));
    }
    if norm_addr(&ctx.portal_address) != norm_addr(&snap.portal_address) {
        return Err("runtime Portal does not equal approved Portal".into());
    }
    if ctx.launch_value_wei > snap.max_launch_value_per_attempt {
        return Err("launch value exceeds approved ceiling".into());
    }
    if ctx.buy_value_wei > snap.max_buy_value_per_attempt {
        return Err("BUY value exceeds approved ceiling".into());
    }
    if ctx.gas_price_wei > snap.max_gas_price_wei {
        return Err("signed gas price exceeds approved ceiling".into());
    }
    if ctx.launch_gas_limit > snap.max_launch_gas_limit {
        return Err("launch gas limit exceeds approved ceiling".into());
    }
    if ctx.buy_gas_limit > snap.max_buy_gas_limit {
        return Err("BUY gas limit exceeds approved ceiling".into());
    }
    if ctx.direct_builder_tip_wei > snap.max_direct_builder_tip_wei {
        return Err("direct builder tip exceeds approved ceiling".into());
    }
    if ctx.reserved_attempt_wei > snap.max_total_per_attempt {
        return Err(format!(
            "attempt reserve {} exceeds approved per-attempt maximum {}",
            ctx.reserved_attempt_wei, snap.max_total_per_attempt
        ));
    }
    if ctx.attempt_number != 1 {
        return Err(format!(
            "single-attempt freeze: only G0.7 attempt 1 is permitted, got {}",
            ctx.attempt_number
        ));
    }
    if ctx.attempt_number > snap.max_attempts {
        return Err(format!(
            "attempt {} exceeds approved maximum {}",
            ctx.attempt_number, snap.max_attempts
        ));
    }

    let next_total = ctx
        .spent_budget_wei
        .checked_add(ctx.reserved_attempt_wei)
        .ok_or("budget addition overflow")?;
    if next_total > snap.absolute_budget_wei {
        return Err(format!(
            "budget guard: spent {} + reserve {} > absolute {}",
            ctx.spent_budget_wei, ctx.reserved_attempt_wei, snap.absolute_budget_wei
        ));
    }

    Ok(snap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn legacy_hashes_bind_imports_and_native_inputs_alike() {
        let h = |c: char| c.to_string().repeat(64);
        let native = InputHashes {
            record_sha256: h('a'),
            legacy_sha256: None,
        };
        let import = InputHashes {
            record_sha256: h('a'),
            legacy_sha256: Some(h('b')),
        };
        assert!(verify_input("x", &native, &native).is_ok());
        assert!(verify_input("x", &import, &import).is_ok());
        assert!(verify_input("x", &native, &import).is_err());
        assert!(verify_input("x", &import, &native).is_err());
        let other = InputHashes {
            record_sha256: h('c'),
            legacy_sha256: None,
        };
        assert!(verify_input("x", &native, &other).is_err());
        let wrong_legacy = InputHashes {
            record_sha256: h('a'),
            legacy_sha256: Some(h('d')),
        };
        assert!(verify_input("x", &import, &wrong_legacy).is_err());
    }

    #[test]
    fn frozen_hash_is_unarmed_or_sha256() {
        assert!(
            FROZEN_APPROVED_V4_SHA256 == "UNARMED"
                || (FROZEN_APPROVED_V4_SHA256.len() == 64
                    && FROZEN_APPROVED_V4_SHA256
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit()))
        );
    }
}

/// Exact runtime identity binding; no normalization or empty identity.
pub fn verify_attempt_id(expected: &str, actual: &str) -> Result<(), String> {
    if expected.is_empty() || expected != actual { return Err("runtime attempt_id differs from frozen v4".into()); }
    Ok(())
}

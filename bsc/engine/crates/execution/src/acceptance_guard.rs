use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub const V2_RELATIVE_PATH: &str = "bsc/config/g01_mainnet_acceptance_proposal_v2.json";

// Deliberately UNARMED. Only a dedicated post-DRY, explicitly approved commit may
// replace this with the SHA-256 of the exact final v2 bytes. Environment variables
// and CLI flags are never execution authority.
pub const FROZEN_APPROVED_V2_SHA256: &str = "UNARMED";

#[derive(Debug, Clone)]
pub struct LiveGuardContext {
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
    pub dry_evidence_sha256: String,
    pub preflight_evidence_sha256: String,
    pub builder_policy_evidence_sha256: String,
    pub producer_witness_source_contract_sha256: String,
}

#[derive(Debug, Clone)]
pub struct ContractSnapshot {
    pub sha256: String,
    pub approved: bool,
    pub execution_ready: bool,
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
    pub dry_evidence_sha256: Option<String>,
    pub preflight_evidence_sha256: Option<String>,
    pub builder_policy_evidence_sha256: Option<String>,
    pub producer_witness_source_contract_sha256: Option<String>,
}

fn norm_addr(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn req_bool(v: &Value, ptr: &str) -> Result<bool, String> {
    v.pointer(ptr)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("missing/bad bool at {ptr}"))
}

fn req_u64(v: &Value, ptr: &str) -> Result<u64, String> {
    v.pointer(ptr)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing/bad u64 at {ptr}"))
}

fn req_string(v: &Value, ptr: &str) -> Result<String, String> {
    v.pointer(ptr)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("missing/bad string at {ptr}"))
}

fn opt_string(v: &Value, ptr: &str) -> Option<String> {
    v.pointer(ptr)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn req_u128_string(v: &Value, ptr: &str) -> Result<u128, String> {
    req_string(v, ptr)?
        .parse::<u128>()
        .map_err(|e| format!("bad u128 at {ptr}: {e}"))
}

fn req_u64_string(v: &Value, ptr: &str) -> Result<u64, String> {
    req_string(v, ptr)?
        .parse::<u64>()
        .map_err(|e| format!("bad u64 at {ptr}: {e}"))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn inspect_v2(path: &Path) -> Result<ContractSnapshot, String> {
    if path != Path::new(V2_RELATIVE_PATH) {
        return Err(format!(
            "wrong acceptance contract path: expected exactly {V2_RELATIVE_PATH}, got {}",
            path.display()
        ));
    }

    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let sha256 = sha256_hex(&bytes);
    let v: Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))?;

    if v.get("schema_version").and_then(Value::as_u64) != Some(2) {
        return Err("acceptance contract schema_version must be 2".into());
    }

    Ok(ContractSnapshot {
        sha256,
        approved: req_bool(&v, "/authorization/approved")?,
        execution_ready: req_bool(&v, "/authorization/execution_ready")?,
        launcher_address: req_string(&v, "/authorization/launcher_address")?,
        buyer_address: req_string(&v, "/authorization/buyer_address")?,
        chain_id: req_u64(&v, "/scope/chain_id")?,
        portal_address: req_string(&v, "/scope/portal_expected_from_current_evidence")?,
        max_attempts: req_u64(&v, "/attempts/maximum")?,
        max_buy_value_per_attempt: req_u128_string(
            &v,
            "/budget_wei_proposal/max_buy_value_per_attempt",
        )?,
        max_launch_value_per_attempt: req_u128_string(
            &v,
            "/budget_wei_proposal/max_launch_call_value_per_attempt",
        )?,
        max_launch_gas_limit: req_u64_string(&v, "/budget_wei_proposal/max_launch_gas_limit")?,
        max_buy_gas_limit: req_u64_string(&v, "/budget_wei_proposal/max_buy_gas_limit")?,
        max_gas_price_wei: req_u128_string(&v, "/budget_wei_proposal/max_gas_price_wei")?,
        max_total_per_attempt: req_u128_string(&v, "/budget_wei_proposal/max_total_per_attempt")?,
        absolute_budget_wei: req_u128_string(
            &v,
            "/budget_wei_proposal/absolute_max_total_all_four_attempts",
        )?,
        max_direct_builder_tip_wei: req_u128_string(
            &v,
            "/economics_validation/bid_policy/max_direct_builder_tip_wei",
        )?,
        // v2 predates the explicit machine-readable capture flags. Until the
        // final arming revision adds them, the safe/default interpretation is
        // that both are mandatory, matching the master-plan §§9/11 contract.
        capture_required: v
            .pointer("/retention_observability/pre_arm_bounded_capture_required")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        producer_witness_contract_required: v
            .pointer("/retention_observability/producer_witness_contract_required_before_execution")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        dry_evidence_sha256: opt_string(&v, "/authorization/evidence_bindings/dry_evidence_sha256"),
        preflight_evidence_sha256: opt_string(
            &v,
            "/authorization/evidence_bindings/preflight_evidence_sha256",
        ),
        builder_policy_evidence_sha256: opt_string(
            &v,
            "/authorization/evidence_bindings/builder_policy_evidence_sha256",
        ),
        producer_witness_source_contract_sha256: opt_string(
            &v,
            "/authorization/evidence_bindings/producer_witness_source_contract_sha256",
        ),
    })
}

fn verify_bound_sha256(label: &str, frozen: Option<&str>, runtime: &str) -> Result<(), String> {
    let frozen = frozen.ok_or_else(|| format!("v2 evidence binding missing: {label}"))?;
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

pub fn verify_live_authorization(
    path: &Path,
    ctx: &LiveGuardContext,
) -> Result<ContractSnapshot, String> {
    let snap = inspect_v2(path)?;

    if FROZEN_APPROVED_V2_SHA256 == "UNARMED" {
        return Err(
            "live guard is deliberately UNARMED: no approved v2 SHA-256 is compiled in".into(),
        );
    }
    if snap.sha256 != FROZEN_APPROVED_V2_SHA256 {
        return Err(format!(
            "v2 SHA-256 mismatch: file={} frozen={}",
            snap.sha256, FROZEN_APPROVED_V2_SHA256
        ));
    }
    if !snap.approved {
        return Err("v2 authorization.approved is not true".into());
    }
    if !snap.execution_ready {
        return Err("v2 authorization.execution_ready is not true".into());
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

    verify_bound_sha256(
        "dry_evidence_sha256",
        snap.dry_evidence_sha256.as_deref(),
        &ctx.dry_evidence_sha256,
    )?;
    verify_bound_sha256(
        "preflight_evidence_sha256",
        snap.preflight_evidence_sha256.as_deref(),
        &ctx.preflight_evidence_sha256,
    )?;
    verify_bound_sha256(
        "builder_policy_evidence_sha256",
        snap.builder_policy_evidence_sha256.as_deref(),
        &ctx.builder_policy_evidence_sha256,
    )?;
    verify_bound_sha256(
        "producer_witness_source_contract_sha256",
        snap.producer_witness_source_contract_sha256.as_deref(),
        &ctx.producer_witness_source_contract_sha256,
    )?;

    if norm_addr(&snap.launcher_address) != norm_addr(&ctx.launcher_address) {
        return Err("runtime launcher does not equal frozen v2 launcher".into());
    }
    if norm_addr(&snap.buyer_address) != norm_addr(&ctx.buyer_address) {
        return Err("runtime buyer does not equal frozen v2 buyer".into());
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
    fn frozen_hash_is_unarmed_or_sha256() {
        assert!(
            FROZEN_APPROVED_V2_SHA256 == "UNARMED"
                || (FROZEN_APPROVED_V2_SHA256.len() == 64
                    && FROZEN_APPROVED_V2_SHA256
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit()))
        );
    }
}

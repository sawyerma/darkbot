use sha2::{Digest, Sha256};
use serde_json::Value;
use std::{fs, path::Path};

pub const V2_RELATIVE_PATH: &str =
    "bsc/gates/gate0/evidence/g01_mainnet_acceptance_proposal_v2.json";

// Deliberately UNARMED. After DRY-1..DRY-4 pass and the user explicitly approves
// the finalized v2 contract, replace this with the SHA-256 of the exact approved
// v2 bytes in a dedicated commit. A CLI flag or environment variable cannot arm it.
pub const FROZEN_APPROVED_V2_SHA256: &str = "UNARMED";

#[derive(Debug, Clone)]
pub struct LiveGuardContext {
    pub launcher_address: String,
    pub buyer_address: String,
    pub spent_budget_wei: u128,
    pub reserved_attempt_wei: u128,
    pub attempt_number: u64,
    pub dry_pass_all: bool,
}

#[derive(Debug, Clone)]
pub struct ContractSnapshot {
    pub sha256: String,
    pub approved: bool,
    pub execution_ready: bool,
    pub launcher_address: Option<String>,
    pub buyer_address: Option<String>,
    pub approved_contract_sha256: Option<String>,
    pub max_attempts: u64,
    pub absolute_budget_wei: u128,
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

fn req_u128_string(v: &Value, ptr: &str) -> Result<u128, String> {
    v.pointer(ptr)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing/bad decimal string at {ptr}"))?
        .parse::<u128>()
        .map_err(|e| format!("bad u128 at {ptr}: {e}"))
}

fn opt_string(v: &Value, ptr: &str) -> Option<String> {
    v.pointer(ptr).and_then(Value::as_str).map(ToOwned::to_owned)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn inspect_v2(path: &Path) -> Result<ContractSnapshot, String> {
    if !path.to_string_lossy().ends_with(V2_RELATIVE_PATH) {
        return Err(format!(
            "wrong acceptance contract path: expected suffix {V2_RELATIVE_PATH}, got {}",
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

    let max_attempts = req_u64(&v, "/attempts/maximum")?;
    let absolute_budget_wei =
        req_u128_string(&v, "/budget_wei_proposal/absolute_max_total_all_four_attempts")?;

    Ok(ContractSnapshot {
        sha256,
        approved: req_bool(&v, "/authorization/approved")?,
        execution_ready: req_bool(&v, "/authorization/execution_ready")?,
        launcher_address: opt_string(&v, "/authorization/launcher_address"),
        buyer_address: opt_string(&v, "/authorization/buyer_address"),
        approved_contract_sha256: opt_string(&v, "/authorization/approved_contract_sha256"),
        max_attempts,
        absolute_budget_wei,
    })
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

    let launcher = snap
        .launcher_address
        .as_deref()
        .ok_or("v2 launcher_address is not frozen")?;
    let buyer = snap
        .buyer_address
        .as_deref()
        .ok_or("v2 buyer_address is not frozen")?;

    if norm_addr(launcher) != norm_addr(&ctx.launcher_address) {
        return Err("runtime launcher does not equal v2 launcher_address".into());
    }
    if norm_addr(buyer) != norm_addr(&ctx.buyer_address) {
        return Err("runtime buyer does not equal v2 buyer_address".into());
    }
    if norm_addr(launcher) == norm_addr(buyer) {
        return Err("launcher and buyer must be distinct".into());
    }

    let contract_hash = snap
        .approved_contract_sha256
        .as_deref()
        .ok_or("v2 approved_contract_sha256 is not frozen")?;
    if contract_hash != FROZEN_APPROVED_V2_SHA256 {
        return Err("v2 approved_contract_sha256 does not equal compiled frozen hash".into());
    }

    if ctx.attempt_number == 0 || ctx.attempt_number > snap.max_attempts {
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
    fn live_guard_is_unarmed_at_source() {
        assert_eq!(FROZEN_APPROVED_V2_SHA256, "UNARMED");
    }
}

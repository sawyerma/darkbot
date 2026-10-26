//! BOOT-03..08: bounded read-only qualification through existing provider/ingress and FE decoders.
use super::{
    boot_audit::{
        self, AccountProof, AuditVerdict, BootAudit, BootCheck, CheckState, NetworkAudit,
    },
    fe_qualification as fe,
};
use crate::analytics::journal;
use darkbot_project0::{
    health::{crate_impl::CrateHealth, HealthEngine},
    protocol,
};
use darkbot_solana::{provider, runtime_config::RuntimeConfig};
use marginfi_type_crate::types::{Bank, MarginfiAccount};
use solana_client::rpc_request::RpcRequest;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{hash::Hash, pubkey::Pubkey};
use std::{
    path::Path,
    str::FromStr,
    time::{Duration, Instant},
};

/// D08: the audit's wall clock (`CLOCK_UNAVAILABLE`, never 0).
pub fn now() -> Result<u64, String> {
    darkbot_common::timing::wall_now()
        .map(|v| v.as_secs())
        .map_err(String::from)
}
fn hash(bytes: &[u8]) -> String {
    fe::sha(bytes)
}
fn check(id: &str, name: &str, result: Result<String, &str>, slot: u64) -> BootCheck {
    let (state, reason) = match result {
        Ok(s) => (CheckState::Pass, s),
        Err(s) => (CheckState::Blocked, s.into()),
    };
    BootCheck {
        id: id.into(),
        name: name.into(),
        critical: true,
        state,
        expected: None,
        observed: Some(format!("finalized_slot={slot}")),
        source: "bounded read-only provider; pinned Rust decoders".into(),
        reason,
    }
}
/// Actual expectations: supplied genesis pin; immutable checked-in ProgramData pin, never re-pinned.
#[derive(Clone)]
pub struct Spec {
    pub genesis: String,
    pub transactions: bool,
    /// DEC-07: the registry's `boot_token_ttl_seconds` and its source.
    pub ttl_seconds: u64,
    pub ttl_source: String,
}
impl Spec {
    pub fn validate(&self) -> Result<(), String> {
        Hash::from_str(&self.genesis).map_err(|_| "BOOT_GENESIS_PIN_REQUIRED")?;
        // BOOT-12: operator input and the checked-in pin are independent sources and must agree.
        if self.genesis != protocol::GENESIS_HASH_MAINNET {
            return Err("BOOT_GENESIS_PIN_MISMATCH".into());
        }
        use darkbot_solana::runtime_config::defaults::{
            BOOT_TOKEN_TTL_MAX_SECONDS, BOOT_TOKEN_TTL_MIN_SECONDS,
        };
        if !(BOOT_TOKEN_TTL_MIN_SECONDS..=BOOT_TOKEN_TTL_MAX_SECONDS).contains(&self.ttl_seconds) {
            return Err("BOOT_TOKEN_TTL".into());
        }
        Ok(())
    }
}
/// Narrow, typed transport interface (X2): no signer/submission operation exists and no JSON
/// value leaves the wire adapter. Fixtures use the same collector.
pub trait Reads {
    fn genesis_hash(&mut self) -> Result<String, String>;
    /// `solana-core` version string.
    fn version(&mut self) -> Result<String, String>;
    fn health_ok(&mut self) -> Result<bool, String>;
    fn slot(&mut self, finalized: bool) -> Result<u64, String>;
    fn multiple_accounts(
        &mut self,
        keys: &[Pubkey],
        min_context_slot: u64,
    ) -> Result<(u64, fe::ObservedState), String>;
    fn wss(&mut self, program: Pubkey, transactions: bool) -> Result<(u64, u64), String>;
}
pub struct NetworkReads {
    /// N6: one client per audit instance (one transport runtime, same provider budget).
    rpc: solana_client::rpc_client::RpcClient,
    call_timeout: Duration,
    wss: String,
    end: Instant,
    calls: u64,
    limit: u64,
}
/// RPC reads of `cluster`: genesis hash, version, health, finalized and confirmed slot.
pub const CLUSTER_READS: u64 = 5;
/// Required BOOT read budget (N1), derived from the read paths the audit runs: [`CLUSTER_READS`]
/// plus `fe::required_scope_reads` (program and ProgramData, then every configured round over
/// the initial scope). Necessary, not sufficient: dependencies discovered on chain add batches
/// known only at run time; they stay bounded by the hard limit (an exhausted budget blocks the
/// audit, it never passes).
pub fn required_reads(c: &RuntimeConfig) -> Result<u64, String> {
    Ok(CLUSTER_READS + fe::required_scope_reads(c)?)
}
/// R4: upper bound of the BOOT reads for the configured scope ([`CLUSTER_READS`] plus
/// `fe::worst_case_scope_reads`). A `boot_calls` budget at or above it cannot run out on any
/// dynamic dependency set; below it (but at least [`required_reads`]) the audit may block.
pub fn worst_case_reads(c: &RuntimeConfig) -> Result<u64, String> {
    Ok(CLUSTER_READS + fe::worst_case_scope_reads(c)?)
}
/// The audit's own call and time budget `(calls, seconds)` (N1): `boot_calls` capped by the
/// global `max_requests`, and `boot_seconds`; independent of the FE-01 observation limits. A
/// budget below [`required_reads`] is refused before any network access.
fn boot_budget(c: &RuntimeConfig) -> Result<(u64, u64), String> {
    let calls = c.limits.boot_calls.min(c.limits.max_requests);
    if calls < required_reads(c)? {
        return Err("BOOT_BUDGET_TOO_SMALL".into());
    }
    let seconds = c.limits.boot_seconds;
    if seconds == 0 || seconds > 300 {
        return Err("BOOT_TIME_BUDGET".into());
    }
    Ok((calls, seconds))
}
impl NetworkReads {
    pub fn new(c: &RuntimeConfig) -> Result<Self, String> {
        if !darkbot_solana::runtime_config::configured() {
            return Err("BOOT_PROVIDER_NOT_INITIALIZED".into());
        }
        Self::from_config(c)
    }
    /// Endpoint scope and budget checks; nothing is contacted.
    fn from_config(c: &RuntimeConfig) -> Result<Self, String> {
        if c.fallback || c.rpc_endpoints.len() != 1 || c.wss_endpoints.len() != 1 {
            return Err("BOOT_PROVIDER_SCOPE_REQUIRES_SINGLE_RPC_WSS".into());
        }
        if !c.rpc_endpoints[0].starts_with("https://") || !c.wss_endpoints[0].starts_with("wss://")
        {
            return Err("BOOT_TLS_REQUIRED".into());
        }
        let (limit, seconds) = boot_budget(c)?;
        Ok(Self::open(
            c.rpc_endpoints[0].clone(),
            c.wss_endpoints[0].clone(),
            Duration::from_millis(c.limits.rpc_timeout_ms),
            limit,
            seconds,
        ))
    }
    fn open(rpc_url: String, wss: String, timeout: Duration, limit: u64, seconds: u64) -> Self {
        let budget = Duration::from_secs(seconds);
        // N6: the cached client's per-call timeout is bounded (a quarter of the audit budget, at
        // least 1 s), and a call starts only while that much time remains, so no call can end
        // after the absolute deadline.
        let call_timeout = timeout.min(budget / 4).max(Duration::from_secs(1));
        Self {
            rpc: provider::rpc_client(
                rpc_url,
                call_timeout,
                CommitmentConfig::finalized(),
                provider::Workload::Bootstrap,
            ),
            call_timeout,
            wss,
            end: Instant::now() + budget,
            calls: 0,
            limit,
        }
    }
}
impl NetworkReads {
    /// The Solana JSON-RPC wire edge (JFI-02): method filter, call and time budget; every
    /// response is decoded into native values by the typed `Reads` methods right here.
    fn send(
        &mut self,
        request: RpcRequest,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        if !matches!(
            request,
            RpcRequest::GetGenesisHash
                | RpcRequest::GetVersion
                | RpcRequest::GetHealth
                | RpcRequest::GetSlot
                | RpcRequest::GetMultipleAccounts
        ) {
            return Err("BOOT_METHOD_REFUSED".into());
        }
        if self.end.saturating_duration_since(Instant::now()) < self.call_timeout {
            return Err("BOOT_TIME_BUDGET".into());
        }
        if self.calls >= self.limit {
            return Err("BOOT_RPC_BUDGET".into());
        }
        self.calls += 1;
        let value = self
            .rpc
            .send(request, params)
            .map_err(|e| darkbot_solana::submit::rpc::safe_rpc_error(&e))?;
        if Instant::now() >= self.end {
            return Err("BOOT_TIME_BUDGET".into());
        }
        Ok(value)
    }
}
impl Reads for NetworkReads {
    fn genesis_hash(&mut self) -> Result<String, String> {
        let v = self.send(RpcRequest::GetGenesisHash, serde_json::json!([]))?;
        v.as_str()
            .map(str::to_owned)
            .ok_or_else(|| "BOOT_GENESIS_UNKNOWN".into())
    }
    fn version(&mut self) -> Result<String, String> {
        let v = self.send(RpcRequest::GetVersion, serde_json::json!([]))?;
        v["solana-core"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "BOOT_CLUSTER_VERSION_UNKNOWN".into())
    }
    fn health_ok(&mut self) -> Result<bool, String> {
        Ok(self
            .send(RpcRequest::GetHealth, serde_json::json!([]))?
            .as_str()
            == Some("ok"))
    }
    fn slot(&mut self, finalized: bool) -> Result<u64, String> {
        let commitment = if finalized { "finalized" } else { "confirmed" };
        self.send(
            RpcRequest::GetSlot,
            serde_json::json!([{ "commitment": commitment }]),
        )?
        .as_u64()
        .ok_or_else(|| "BOOT_SLOT_UNKNOWN".into())
    }
    fn multiple_accounts(
        &mut self,
        keys: &[Pubkey],
        min_context_slot: u64,
    ) -> Result<(u64, fe::ObservedState), String> {
        let response = self.send(
            RpcRequest::GetMultipleAccounts,
            serde_json::json!([
                keys.iter().map(ToString::to_string).collect::<Vec<_>>(),
                {"encoding": "base64", "commitment": "finalized", "minContextSlot": min_context_slot}
            ]),
        )?;
        // JFI-02: decoded at this wire edge into the typed answer, then the FE-01 check.
        let wire = <fe::WireMultipleAccounts as serde::Deserialize>::deserialize(&response)
            .map_err(|_| "FE_RPC_RESULT_DECODE")?;
        fe::observed(&crate::analytics::payload::Fe01Read {
            keys: keys.to_vec(),
            min_context_slot,
            outcome: wire.outcome(),
        })
    }
    fn wss(&mut self, program: Pubkey, transactions: bool) -> Result<(u64, u64), String> {
        let remaining = self
            .end
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(30));
        if remaining.is_zero() {
            return Err("BOOT_TIME_BUDGET".into());
        }
        darkbot_solana::ingress::ws_rpc::audit_liveness(
            self.wss.clone(),
            program,
            transactions,
            remaining,
        )
        .map_err(str::to_owned)
    }
}
fn cluster(io: &mut impl Reads, spec: &Spec) -> Result<(u64, u64), String> {
    if io.genesis_hash()? != spec.genesis {
        return Err("BOOT_CLUSTER_MISMATCH".into());
    }
    let version = io.version()?;
    if version.is_empty()
        || version.len() > 64
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b))
    {
        return Err("BOOT_CLUSTER_VERSION_UNKNOWN".into());
    }
    if !io.health_ok()? {
        return Err("BOOT_CLUSTER_UNHEALTHY".into());
    }
    let finalized = Some(io.slot(true)?)
        .filter(|s| *s > 0)
        .ok_or("BOOT_SLOT_UNKNOWN")?;
    let current = Some(io.slot(false)?)
        .filter(|s| *s >= finalized)
        .ok_or("BOOT_SLOT_CONTRADICTION")?;
    if current - finalized > 150 {
        return Err("BOOT_CLUSTER_LAG".into());
    }
    Ok((finalized, current))
}
fn program(scope: &fe::CollectedScope) -> Result<String, &'static str> {
    let a = scope
        .state
        .0
        .get(&scope.pd)
        .ok_or("PROGRAM_VERSION_UNKNOWN")?;
    let d = protocol::decode_program_data(&scope.pd, &a.owner, a.executable, &a.data)
        .map_err(|_| "PROGRAM_IDENTITY_MISMATCH")?;
    if protocol::deployment_pin(&scope.program, &d) != protocol::DeploymentPin::Reference {
        return Err("PROGRAM_IDENTITY_MISMATCH");
    }
    Ok(format!(
        "program={} program_data={} sha256={}",
        scope.program, scope.pd, scope.deployment.1
    ))
}
/// Standard SPL custody is independently tied to its bank/mint/PDA. Unsupported extension or
/// integration ownership remains blocked until its expected schema/program pins are qualified.
fn relationships(scope: &fe::CollectedScope) -> Result<String, &'static str> {
    let token = Pubkey::from_str(protocol::TOKEN_PROGRAM_ID).unwrap();
    for key in &scope.targets {
        let target: MarginfiAccount = fe::typed(
            &scope.state,
            key,
            &scope.program,
            protocol::DISC_MARGINFI_ACCOUNT,
        )
        .map_err(|_| "BOOT_TARGET_LAYOUT")?;
        if target.group != scope.group {
            return Err("BOOT_TARGET_GROUP");
        }
        for balance in target
            .lending_account
            .balances
            .iter()
            .filter(|b| b.is_active())
        {
            let bank: Bank = fe::typed(
                &scope.state,
                &balance.bank_pk,
                &scope.program,
                protocol::DISC_BANK,
            )
            .map_err(|_| "BOOT_BANK_LAYOUT")?;
            if bank.group != scope.group {
                return Err("BOOT_BANK_GROUP");
            }
            if [
                bank.integration_acc_1,
                bank.integration_acc_2,
                bank.integration_acc_3,
            ]
            .iter()
            .any(|k| *k != Pubkey::default())
            {
                return Err("BOOT_INTEGRATION_PROGRAM_PIN_UNQUALIFIED");
            }
            let mint = scope.state.0.get(&bank.mint).ok_or("BOOT_MINT_MISSING")?;
            let vault = scope
                .state
                .0
                .get(&bank.liquidity_vault)
                .ok_or("BOOT_VAULT_MISSING")?;
            if mint.owner != token
                || vault.owner != token
                || mint.executable
                || vault.executable
                || mint.data.len() != 82
                || vault.data.len() != 165
            {
                return Err("BOOT_TOKEN_OWNER_OR_SCHEMA_UNQUALIFIED");
            }
            if mint.data[45] != 1 || mint.data[44] != bank.mint_decimals || vault.data[108] != 1 {
                return Err("BOOT_TOKEN_STATE");
            }
            if &vault.data[..32] != bank.mint.as_ref()
                || &vault.data[32..64]
                    != protocol::liquidity_vault_authority_pda(&scope.program, &balance.bank_pk)
                        .as_ref()
            {
                return Err("BOOT_VAULT_PDA_OR_MINT_MISMATCH");
            }
        }
    }
    Ok(
        "target/group/bank/mint/custody authority and canonical dynamic dependency scope verified"
            .into(),
    )
}
fn oracles(scope: &fe::CollectedScope) -> Result<String, &'static str> {
    let h = CrateHealth {
        state: &scope.state,
        group: scope.group,
    };
    for target in &scope.targets {
        h.health(target, scope.clock.unix_timestamp, scope.clock.slot)
            .map_err(|_| "BOOT_ORACLE_HEALTH_UNKNOWN")?;
        h.receivership_cb_gate(target, scope.clock.unix_timestamp, scope.clock.slot)
            .map_err(|_| "BOOT_ORACLE_CONFIDENCE_OR_STALENESS")?;
        let a: MarginfiAccount = fe::typed(
            &scope.state,
            target,
            &scope.program,
            protocol::DISC_MARGINFI_ACCOUNT,
        )
        .map_err(|_| "BOOT_TARGET_LAYOUT")?;
        for b in a.lending_account.balances.iter().filter(|b| b.is_active()) {
            h.maintenance_risk_price_bits(&b.bank_pk, scope.clock.unix_timestamp, scope.clock.slot)
                .map_err(|_| "BOOT_ORACLE_PRICE_UNQUALIFIED")?;
        }
    }
    Ok("pinned per-bank oracle identity, freshness, confidence and price gates passed; no quote or funding qualification".into())
}
fn run(
    mut audit: BootAudit,
    c: &RuntimeConfig,
    spec: &Spec,
    io: &mut impl Reads,
    observed_at: u64,
    provider_proof_sha256: String,
    binary_sha256: String,
) -> Result<NetworkAudit, String> {
    spec.validate()?;
    if audit.identity.live_enabled || audit.identity.submission_enabled {
        return Err("BOOT_OBSERVE_ONLY_REQUIRED".into());
    }
    audit.scope = "network".into();
    audit.checks.retain(|c| {
        !matches!(
            c.id.as_str(),
            "BOOT-03" | "BOOT-04" | "BOOT-05" | "BOOT-06" | "BOOT-07" | "BOOT-08"
        )
    });
    let mut slot = 0;
    let mut accounts = Vec::new();
    let mut state_hash = String::new();
    let cluster_result = cluster(io, spec);
    let cluster_ok = cluster_result.is_ok();
    audit.checks.push(check(
        "BOOT-03",
        "cluster",
        match cluster_result {
            Ok((f, s)) => {
                slot = f;
                Ok(format!(
                    "genesis={} finalized={f} confirmed={s}",
                    spec.genesis
                ))
            }
            Err(ref e) => Err(e.as_str()),
        },
        slot,
    ));
    let program_key = Pubkey::from_str(protocol::PROGRAM_ID_MAINNET).unwrap();
    if cluster_ok {
        let scope = fe::collect_scope(
            c,
            |keys, floor| io.multiple_accounts(keys, floor.max(slot)),
            true,
        );
        match scope {
            Ok(s) => {
                slot = s.clock.slot;
                let clock_ok = s.clock.unix_timestamp >= 0
                    && observed_at.abs_diff(s.clock.unix_timestamp as u64) <= 60;
                audit
                    .checks
                    .push(check("BOOT-05", "program_identity", program(&s), slot));
                let relation = relationships(&s);
                audit.checks.push(check(
                    "BOOT-06",
                    "coherent_protocol_state",
                    if clock_ok {
                        relation.clone()
                    } else {
                        Err("BOOT_CHAIN_CLOCK_STALE")
                    },
                    slot,
                ));
                audit
                    .checks
                    .push(check("BOOT-07", "dynamic_relationships", relation, slot));
                audit.checks.push(check(
                    "BOOT-08",
                    "oracles",
                    if clock_ok {
                        oracles(&s)
                    } else {
                        Err("BOOT_CHAIN_CLOCK_STALE")
                    },
                    slot,
                ));
                accounts = s
                    .state
                    .0
                    .iter()
                    .map(|(key, a)| AccountProof {
                        address: key.to_string(),
                        owner: a.owner.to_string(),
                        data_sha256: hash(&a.data),
                        bytes: a.data.len() as u64,
                        executable: a.executable,
                    })
                    .collect();
                state_hash =
                    hash(&bincode::serialize(&accounts).map_err(|_| "BOOT_STATE_ENCODING")?);
            }
            Err(ref e) => {
                for id in ["BOOT-05", "BOOT-06", "BOOT-07", "BOOT-08"] {
                    audit
                        .checks
                        .push(check(id, "state_observation", Err(e.as_str()), slot));
                }
            }
        }
        let wss = io.wss(program_key, spec.transactions);
        audit.checks.push(check("BOOT-04","provider_liveness",match wss{Ok((a,b)) if a>=slot&&b>a=>Ok(format!("subscriptions ACKed, source data and increasing slots {a}..{b}; measured FREE_ONLY proof required before transport")),_=>Err("BOOT_WSS_LIVENESS_UNQUALIFIED")},slot));
    } else {
        for id in ["BOOT-04", "BOOT-05", "BOOT-06", "BOOT-07", "BOOT-08"] {
            audit.checks.push(check(
                id,
                "network_dependency",
                Err("BOOT_CLUSTER_BLOCKED"),
                slot,
            ));
        }
    }
    audit.verdict = if audit
        .checks
        .iter()
        .any(|c| c.critical && !matches!(c.state, CheckState::Pass | CheckState::NotApplicable))
    {
        AuditVerdict::Blocked
    } else {
        AuditVerdict::Ready
    };
    let rpc_binding = c
        .rpc_endpoints
        .first()
        .map(|s| hash(s.as_bytes()))
        .ok_or("BOOT_RPC_REQUIRED")?;
    let wss_binding = c
        .wss_endpoints
        .first()
        .map(|s| hash(s.as_bytes()))
        .ok_or("BOOT_WSS_REQUIRED")?;
    let mut result = NetworkAudit {
        audit,
        observed_at,
        expires_at: observed_at
            .checked_add(spec.ttl_seconds)
            .ok_or("BOOT_TIME")?,
        slot,
        genesis: spec.genesis.clone(),
        rpc_binding,
        wss_binding,
        binary_sha256,
        provider_proof_sha256,
        capture_source: if spec.transactions {
            "transactions"
        } else {
            "logs"
        }
        .into(),
        accounts,
        state_hash,
        ttl_seconds: spec.ttl_seconds,
        ttl_source: spec.ttl_source.clone(),
    };
    result.audit.readiness_token.clear();
    result.audit.readiness_token = hash(&bincode::serialize(&result).map_err(|_| "BOOT_TOKEN")?);
    Ok(result)
}
/// Read permission and FREE_ONLY proof are checked before constructing clients or opening WSS.
/// `mode`: the operating mode `run start` will launch (DEC-05: its profile).
pub fn execute(project: &Path, mode: &str, args: &[String]) -> Result<bool, String> {
    let val = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let operational = args.split(|a| a == "--").nth(1).unwrap_or(&[]);
    let resolved = darkbot_solana::runtime_config::Resolved::for_mode(mode, operational, &|k| {
        std::env::var(k).ok()
    })?;
    // DEC-07: the TTL's source, as the status projection names it.
    let ttl_source = resolved
        .sources
        .get("boot_token_ttl_seconds")
        .cloned()
        .unwrap_or_else(|| "builtin_default".into());
    resolved.activate()?;
    let c = darkbot_solana::runtime_config::active();
    let offline = boot_audit::offline(&boot_audit::OfflineInputs::for_project(project))?;
    if val("--network-approved").as_deref() != Some(&offline.readiness_token[..16]) {
        print!("{}", offline.terminal());
        return Err("BOOT_NETWORK_APPROVAL_REQUIRED: explicit approval of the printed offline token required; no network request made".into());
    }
    if offline.verdict == AuditVerdict::Blocked {
        return Err("BOOT_OFFLINE_BLOCKED".into());
    }
    let spec = Spec {
        genesis: val("--genesis-hash").ok_or("BOOT_GENESIS_PIN_REQUIRED")?,
        transactions: match val("--capture-source").as_deref() {
            None | Some("logs") => false,
            Some("transactions") => true,
            _ => return Err("BOOT_CAPTURE_SOURCE".into()),
        },
        ttl_seconds: c.limits.boot_token_ttl_seconds,
        ttl_source,
    };
    spec.validate()?;
    // X2: the typed PROVIDER_EVIDENCE record of the explicit offline import
    // (`p0-journal import-provider-evidence`); its bytes are bound into the audit record.
    let proof_path = val("--provider-evidence").ok_or("BOOT_PROVIDER_EVIDENCE_REQUIRED")?;
    let read_bounded = || -> Result<Vec<u8>, String> {
        let mut data = Vec::new();
        use std::io::Read;
        std::fs::File::open(&proof_path)
            .map_err(|_| "BOOT_PROVIDER_EVIDENCE_IO")?
            .take(65537)
            .read_to_end(&mut data)
            .map_err(|_| "BOOT_PROVIDER_EVIDENCE_IO")?;
        if data.len() > 65536 {
            return Err("BOOT_PROVIDER_EVIDENCE_LIMIT".into());
        }
        Ok(data)
    };
    let data = read_bounded()?;
    let proof: fe::ProviderEvidence = journal::read_typed_value(Path::new(&proof_path))
        .map_err(|_| "BOOT_PROVIDER_EVIDENCE_FORMAT")?;
    if read_bounded()? != data {
        return Err("BOOT_PROVIDER_EVIDENCE_CHANGED".into());
    }
    fe::validate_provider_evidence(
        c,
        c.rpc_endpoints.first().ok_or("BOOT_RPC_REQUIRED")?,
        &proof,
        now()? as f64,
    )?;
    if proof.wss_provider_binding.as_deref()
        != c.wss_endpoints
            .first()
            .map(|s| hash(s.as_bytes()))
            .as_deref()
        || !proof.wss_free_only
    {
        return Err("BOOT_WSS_FREE_ONLY_UNQUALIFIED".into());
    }
    if c.verification != "off" && !proof.archive_qualified {
        return Err("BOOT_ARCHIVE_UNQUALIFIED".into());
    }
    let output = val("--output").ok_or("BOOT_OUTPUT_REQUIRED")?;
    let mut writer =
        journal::Writer::create(Path::new(&output)).map_err(|_| "BOOT_EVIDENCE_CREATE")?;
    let mut io = NetworkReads::new(c)?;
    let result = run(
        offline,
        c,
        &spec,
        &mut io,
        now()?,
        hash(&data),
        boot_audit::binary_identity()?,
    )?;
    writer
        .append_typed(journal::Provenance::default(), &result)
        .map_err(|_| "BOOT_EVIDENCE_WRITE")?;
    writer.seal(true).map_err(|_| "BOOT_EVIDENCE_SYNC")?;
    print!("{}", result.audit.terminal());
    Ok(result.audit.verdict == AuditVerdict::Ready)
}
#[cfg(test)]
mod tests {
    use super::super::boot_audit::{AccountProof, AuditedIdentity};
    use super::*;
    use serde_json::{json, Value};
    use solana_sdk::account::Account;
    use std::collections::BTreeMap;
    struct Fixture {
        genesis: String,
        all: BTreeMap<Pubkey, Account>,
        reads: usize,
        wss: bool,
        wrong_slot: bool,
    }
    impl Reads for Fixture {
        fn genesis_hash(&mut self) -> Result<String, String> {
            self.reads += 1;
            Ok(self.genesis.clone())
        }
        fn version(&mut self) -> Result<String, String> {
            self.reads += 1;
            Ok("3.1.12".into())
        }
        fn health_ok(&mut self) -> Result<bool, String> {
            self.reads += 1;
            Ok(true)
        }
        fn slot(&mut self, finalized: bool) -> Result<u64, String> {
            self.reads += 1;
            Ok(if self.wrong_slot && !finalized {
                99
            } else {
                100
            })
        }
        fn multiple_accounts(
            &mut self,
            keys: &[Pubkey],
            _: u64,
        ) -> Result<(u64, fe::ObservedState), String> {
            self.reads += 1;
            let mut state = fe::ObservedState::default();
            for k in keys {
                let a = self
                    .all
                    .get(k)
                    .ok_or_else(|| format!("FE_ACCOUNT_MISSING:{k}"))?;
                state.0.insert(*k, a.clone());
            }
            Ok((100, state))
        }
        fn wss(&mut self, _: Pubkey, _: bool) -> Result<(u64, u64), String> {
            if self.wss {
                Ok((100, 101))
            } else {
                Err("WSS_NO_PROGRESS".into())
            }
        }
    }
    fn audit() -> BootAudit {
        BootAudit {
            scope: "offline".into(),
            checks: vec![],
            identity: AuditedIdentity {
                source_head: Some("fixture".into()),
                source_dirty_sha256: Some("fixture".into()),
                runtime_config_sha256: "fixture".into(),
                journal_version: journal::VERSION,
                discriminator_table_sha256: "fixture".into(),
                live_enabled: false,
                submission_enabled: false,
            },
            verdict: AuditVerdict::OfflineReadyNetworkNotRun,
            readiness_token: "fixture".into(),
        }
    }
    fn spec() -> Spec {
        Spec {
            genesis: protocol::GENESIS_HASH_MAINNET.into(),
            transactions: false,
            ttl_seconds: 60,
            ttl_source: "builtin_default".into(),
        }
    }
    /// BOOT-12: the operator's `--genesis-hash` and the checked-in pin are two independent
    /// sources; the audit runs only when they agree.
    #[test]
    fn spec_rejects_genesis_that_differs_from_the_pin() {
        assert_eq!(spec().validate(), Ok(()));
        for genesis in [Hash::new_unique().to_string(), Hash::default().to_string()] {
            let s = Spec { genesis, ..spec() };
            assert_eq!(s.validate().unwrap_err(), "BOOT_GENESIS_PIN_MISMATCH");
        }
        let s = Spec {
            genesis: "not-a-hash".into(),
            ..spec()
        };
        assert_eq!(s.validate().unwrap_err(), "BOOT_GENESIS_PIN_REQUIRED");
    }
    fn state() -> (RuntimeConfig, fe::CollectedScope) {
        let (c, mut all) = fe::tests::fixture();
        let program = Pubkey::from_str(protocol::PROGRAM_ID_MAINNET).unwrap();
        let banks: Vec<_> = all
            .iter()
            .filter(|(_, a)| {
                a.owner == program && a.data.get(..8) == Some(protocol::DISC_BANK.as_slice())
            })
            .map(|(k, a)| (*k, bytemuck::pod_read_unaligned::<Bank>(&a.data[8..])))
            .collect();
        for (k, b) in banks {
            let mint = all.get_mut(&b.mint).unwrap();
            mint.data[45] = 1;
            mint.data[44] = b.mint_decimals;
            let v = all.get_mut(&b.liquidity_vault).unwrap();
            v.data = vec![0; 165];
            v.data[..32].copy_from_slice(b.mint.as_ref());
            v.data[32..64]
                .copy_from_slice(protocol::liquidity_vault_authority_pda(&program, &k).as_ref());
            v.data[108] = 1;
        }
        let scope = fe::collect_scope(
            &c,
            |keys, _| {
                Ok((
                    100,
                    fe::ObservedState(keys.iter().map(|k| (*k, all[k].clone())).collect()),
                ))
            },
            true,
        )
        .unwrap();
        (c, scope)
    }
    /// Local JSON-RPC stub (no network): answers every request with `result` and counts them.
    fn rpc_stub(
        result: Value,
    ) -> (
        String,
        std::sync::Arc<std::sync::atomic::AtomicU64>,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
        std::thread::JoinHandle<()>,
    ) {
        use std::io::{Read, Write};
        use std::sync::atomic::Ordering::SeqCst;
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let url = format!("http://{}", socket.local_addr().unwrap());
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (n, halt) = (count.clone(), stop.clone());
        let t = std::thread::spawn(move || {
            while !halt.load(SeqCst) {
                let Ok((mut c, _)) = socket.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                c.set_nonblocking(false).unwrap();
                c.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let (mut data, mut b) = (Vec::new(), [0u8; 4096]);
                let body = loop {
                    let Ok(k) = c.read(&mut b) else { break None };
                    if k == 0 {
                        break None;
                    }
                    data.extend_from_slice(&b[..k]);
                    let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") else {
                        continue;
                    };
                    let len = String::from_utf8_lossy(&data[..end])
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if data.len() >= end + 4 + len {
                        break Some(data[end + 4..end + 4 + len].to_vec());
                    }
                };
                let Some(body) = body else { continue };
                let id =
                    serde_json::from_slice::<Value>(&body).map_or(json!(1), |v| v["id"].clone());
                n.fetch_add(1, SeqCst);
                let reply = json!({"jsonrpc":"2.0","id":id,"result":result}).to_string();
                let _ = c.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                        reply.len()
                    )
                    .as_bytes(),
                );
            }
        });
        (url, count, stop, t)
    }
    fn endpoints(c: &mut RuntimeConfig) {
        c.rpc_endpoints = vec!["https://rpc.invalid".into()];
        c.wss_endpoints = vec!["wss://rpc.invalid".into()];
    }
    /// N1: the audit has its own budget, derived from the reads it runs and checked before any
    /// network access; the FE-01 observation limits do not shape it.
    #[test]
    fn network_reads_refuses_budget_below_minimum() {
        let mut c = RuntimeConfig::default();
        endpoints(&mut c);
        // Cluster reads, program + ProgramData, every round over program/ProgramData/group/
        // clock/fee state and the two default targets (7 keys, one batch of 100).
        let need = required_reads(&c).unwrap();
        assert_eq!(need, CLUSTER_READS + 2 + c.limits.fe_snapshot_rounds);
        let refused = |c: &RuntimeConfig| NetworkReads::from_config(c).err();
        c.limits.boot_calls = need - 1;
        assert_eq!(refused(&c).as_deref(), Some("BOOT_BUDGET_TOO_SMALL"));
        c.limits.boot_calls = need;
        assert_eq!(refused(&c), None);
        // The global request ceiling still caps the audit.
        c.limits.max_requests = need - 1;
        assert_eq!(refused(&c).as_deref(), Some("BOOT_BUDGET_TOO_SMALL"));
        c.limits.max_requests = RuntimeConfig::default().limits.max_requests;
        // Smaller batches need more reads per round: the requirement follows the scope.
        c.limits.fe_batch_size = 2;
        assert_eq!(
            required_reads(&c).unwrap(),
            CLUSTER_READS + 2 + c.limits.fe_snapshot_rounds * 4
        );
        assert_eq!(refused(&c).as_deref(), Some("BOOT_BUDGET_TOO_SMALL"));
        // The default profile (FE smoke: 3 calls, 30 s) covers the audit.
        let mut d = RuntimeConfig::default();
        endpoints(&mut d);
        assert_eq!((d.fe_mode.as_str(), d.limits.fe_calls), ("smoke", 3));
        assert_eq!(refused(&d), None);
    }
    #[test]
    fn network_reads_counts_only_boot_budget() {
        let mut c = RuntimeConfig::default();
        c.limits.fe_calls = 1;
        c.limits.fe_seconds = 1;
        let need = required_reads(&c).unwrap();
        c.limits.boot_calls = need;
        let (limit, seconds) = boot_budget(&c).unwrap();
        assert_eq!((limit, seconds), (need, c.limits.boot_seconds));
        let (url, count, stop, server) = rpc_stub(json!(100));
        let mut io = NetworkReads::open(
            url.clone(),
            "wss://unused.invalid".into(),
            Duration::from_secs(2),
            limit,
            seconds,
        );
        for _ in 0..need {
            assert_eq!(io.slot(true).unwrap(), 100);
        }
        assert_eq!(io.slot(true).unwrap_err(), "BOOT_RPC_BUDGET");
        assert_eq!(
            io.send(RpcRequest::SendTransaction, json!([])).unwrap_err(),
            "BOOT_METHOD_REFUSED"
        );
        // An expired deadline refuses before any request.
        let mut late = NetworkReads::open(
            url.clone(),
            "wss://unused.invalid".into(),
            Duration::from_secs(2),
            limit,
            0,
        );
        // N6: no call starts unless its full timeout still fits before the deadline.
        assert_eq!(late.slot(true).unwrap_err(), "BOOT_TIME_BUDGET");
        let mut tight = NetworkReads::open(
            url.clone(),
            "wss://unused.invalid".into(),
            Duration::from_secs(30),
            limit,
            4,
        );
        assert_eq!(tight.call_timeout, Duration::from_secs(1));
        assert_eq!(tight.slot(true).unwrap(), 100);
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        server.join().unwrap();
        // `need` budgeted reads plus the one of the tight-deadline instance.
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), need + 1);
    }
    /// R4: a maximal dynamic scope — two targets, 16 distinct banks each, every bank with four
    /// observation keys and three integration accounts (327 keys) — against the read bounds. A
    /// coherent chain closes it in round 3 with fewer reads than the all-rounds minimum; a chain
    /// that never becomes slot-coherent reads every round over the full scope: more than the
    /// minimum, never more than `worst_case_reads`. With only the minimum budget the same audit
    /// stops at the limit with BOOT_RPC_BUDGET after exactly `limit` reads (no retry).
    #[test]
    fn dynamic_boot_scope_is_bounded_and_blocks_at_the_budget() {
        use bytemuck::Zeroable;
        use marginfi_type_crate::types::{MarginfiAccount, OracleSetup};
        use std::cell::Cell;
        let (c, mut all) = fe::tests::fixture();
        let program = Pubkey::from_str(protocol::PROGRAM_ID_MAINNET).unwrap();
        let group = Pubkey::from_str(&c.fe_group).unwrap();
        let wrapped = |disc: [u8; 8], payload: &[u8]| Account {
            owner: program,
            lamports: 1,
            data: [disc.as_slice(), payload].concat(),
            ..Account::default()
        };
        let plain = || Account {
            owner: Pubkey::new_unique(),
            lamports: 1,
            data: vec![0; 82],
            ..Account::default()
        };
        for target in &c.fe_targets {
            let mut a = MarginfiAccount::zeroed();
            a.group = group;
            let mut banks: Vec<Pubkey> = (0..16).map(|_| Pubkey::new_unique()).collect();
            banks.sort_by(|x, y| y.cmp(x));
            for (i, bank_key) in banks.iter().enumerate() {
                a.lending_account.balances[i].active = 1;
                a.lending_account.balances[i].bank_pk = *bank_key;
                let mut b = Bank::zeroed();
                b.group = group;
                b.mint = Pubkey::new_unique();
                b.liquidity_vault = Pubkey::new_unique();
                b.config.oracle_setup = OracleSetup::StakedWithPythPush;
                for k in b.config.oracle_keys.iter_mut().take(4) {
                    *k = Pubkey::new_unique();
                }
                b.integration_acc_1 = Pubkey::new_unique();
                b.integration_acc_2 = Pubkey::new_unique();
                b.integration_acc_3 = Pubkey::new_unique();
                let deps = [
                    b.mint,
                    b.liquidity_vault,
                    b.config.oracle_keys[0],
                    b.config.oracle_keys[1],
                    b.config.oracle_keys[2],
                    b.config.oracle_keys[3],
                    b.integration_acc_1,
                    b.integration_acc_2,
                    b.integration_acc_3,
                ];
                for k in deps {
                    all.insert(k, plain());
                }
                all.insert(
                    *bank_key,
                    wrapped(protocol::DISC_BANK, bytemuck::bytes_of(&b)),
                );
            }
            all.insert(
                Pubkey::from_str(target).unwrap(),
                wrapped(protocol::DISC_MARGINFI_ACCOUNT, bytemuck::bytes_of(&a)),
            );
        }
        let min = fe::required_scope_reads(&c).unwrap();
        let worst = fe::worst_case_scope_reads(&c).unwrap();
        assert_eq!(
            (min + CLUSTER_READS, worst + CLUSTER_READS),
            (required_reads(&c).unwrap(), worst_case_reads(&c).unwrap())
        );
        let serve = |keys: &[Pubkey], slot: u64| {
            Ok((
                slot,
                fe::ObservedState(keys.iter().map(|k| (*k, all[k].clone())).collect()),
            ))
        };
        // Coherent chain: the scope closes in round 3 (7, 39, then all 327 keys).
        let reads = Cell::new(0u64);
        let scope = fe::collect_scope(
            &c,
            |keys, _| {
                reads.set(reads.get() + 1);
                serve(keys, 100)
            },
            true,
        )
        .unwrap();
        assert_eq!(scope.state.0.len(), 327);
        let coherent_reads = reads.get();
        assert!(coherent_reads <= worst, "{coherent_reads} > {worst}");
        // Never slot-coherent: every round over the full scope, beyond the minimum, within the
        // worst case; the audit ends unqualified, never passes.
        let reads = Cell::new(0u64);
        let incoherent = fe::collect_scope(
            &c,
            |keys, _| {
                reads.set(reads.get() + 1);
                serve(keys, 100 + reads.get())
            },
            true,
        );
        assert!(incoherent.is_err());
        println!(
            "R4_BOOT_SCOPE keys=327 coherent_rounds={} coherent_reads={coherent_reads} incoherent_reads={} minimum={min} worst_case={worst} (scope reads, plus {CLUSTER_READS} cluster reads)",
            scope.round,
            reads.get()
        );
        assert!(
            reads.get() > min && reads.get() <= worst,
            "min {min} < {} <= worst {worst}",
            reads.get()
        );
        // Only the minimum budget: blocked at the limit, exactly `limit` reads, no retry.
        let attempts = Cell::new(0u64);
        let blocked = fe::collect_scope(
            &c,
            |keys, _| {
                if attempts.get() >= min {
                    return Err("BOOT_RPC_BUDGET".to_string());
                }
                attempts.set(attempts.get() + 1);
                serve(keys, 100 + attempts.get())
            },
            true,
        );
        assert_eq!(blocked.err().as_deref(), Some("BOOT_RPC_BUDGET"));
        assert_eq!(attempts.get(), min);
        // Default profile: the configured BOOT budget covers this worst case.
        assert!(c.limits.boot_calls >= worst_case_reads(&c).unwrap());
    }
    /// X2: responses are decoded into native values at the wire edge; malformed ones fail there.
    #[test]
    fn network_reads_decode_at_the_wire_edge() {
        let key = Pubkey::new_unique();
        let decode = |result: Value| {
            let (url, _, stop, server) = rpc_stub(result);
            let mut io = NetworkReads::open(
                url,
                "wss://unused.invalid".into(),
                Duration::from_secs(2),
                5,
                30,
            );
            let out = (
                io.slot(true),
                io.multiple_accounts(&[key], 100),
                io.genesis_hash(),
            );
            stop.store(true, std::sync::atomic::Ordering::SeqCst);
            server.join().unwrap();
            out
        };
        let (slot, accounts, genesis) = decode(json!("not-a-number"));
        assert_eq!(slot.unwrap_err(), "BOOT_SLOT_UNKNOWN");
        assert!(accounts.is_err());
        assert_eq!(genesis.unwrap(), "not-a-number");
        let (_, regressed, _) = decode(json!({"context":{"slot":50},"value":[null]}));
        assert_eq!(regressed.err().unwrap(), "FE_CONTEXT_REGRESSION");
        let (_, owner, _) = decode(json!({"context":{"slot":100},"value":[
            {"owner":"bad","lamports":1,"executable":false,"data":["","base64"]}
        ]}));
        assert_eq!(owner.err().unwrap(), "FE_PUBKEY_INVALID");
        let (_, missing, genesis) = decode(json!({"context":{"slot":100},"value":[null]}));
        assert!(missing.err().unwrap().starts_with("FE_ACCOUNT_MISSING"));
        assert_eq!(genesis.unwrap_err(), "BOOT_GENESIS_UNKNOWN");
    }
    /// DEC-07: the registry's TTL is the record's — 10 s and 120 s are issued, 9 s and 121 s are
    /// refused before any read. The TTL counts from the oldest network read (`observed_at`, taken
    /// before the first read), however long the reads take, never from the token's issue.
    #[test]
    fn dec07_the_record_carries_the_registry_ttl_from_its_oldest_read() {
        let (mut c, all) = fe::tests::fixture();
        c.rpc_endpoints = vec!["https://rpc.invalid".into()];
        c.wss_endpoints = vec!["wss://wss.invalid".into()];
        let io = || Fixture {
            genesis: spec().genesis,
            all: all.clone(),
            reads: 0,
            wss: true,
            wrong_slot: false,
        };
        for ttl in [10, 120] {
            let s = Spec {
                ttl_seconds: ttl,
                ttl_source: "cli".into(),
                ..spec()
            };
            let mut reads = io();
            let out = run(audit(), &c, &s, &mut reads, 1000, "p".into(), "b".into()).unwrap();
            assert!(reads.reads > 1, "the reads follow the TTL's start");
            assert_eq!(
                (out.observed_at, out.expires_at, out.ttl_seconds),
                (1000, 1000 + ttl, ttl)
            );
            assert_eq!(out.ttl_source, "cli");
        }
        for ttl in [9, 121] {
            let s = Spec {
                ttl_seconds: ttl,
                ..spec()
            };
            let mut reads = io();
            let refused = run(audit(), &c, &s, &mut reads, 1000, "p".into(), "b".into());
            assert_eq!(refused.unwrap_err(), "BOOT_TOKEN_TTL");
            assert_eq!(reads.reads, 0, "refused before any read");
        }
    }
    #[test]
    fn cluster_pin_and_slot_contradictions_block_without_followup_reads() {
        let s = spec();
        let (mut c, all) = fe::tests::fixture();
        c.rpc_endpoints = vec!["https://private.invalid/?api-key=SECRET".into()];
        c.wss_endpoints = vec!["wss://private.invalid/?api-key=SECRET".into()];
        let mut io = Fixture {
            genesis: "wrong".into(),
            all,
            reads: 0,
            wss: true,
            wrong_slot: false,
        };
        let out = run(
            audit(),
            &c,
            &s,
            &mut io,
            1000,
            "proof".into(),
            "fixture-binary".into(),
        )
        .unwrap();
        assert_eq!(io.reads, 1);
        assert_eq!(out.audit.verdict, AuditVerdict::Blocked);
        assert_eq!(out.audit.checks.len(), 6);
        assert!(!format!("{out:?}").contains("SECRET"));
        io.genesis = s.genesis.clone();
        io.wrong_slot = true;
        assert!(cluster(&mut io, &s).unwrap_err().contains("CONTRADICTION"));
    }
    #[test]
    fn boot_state_uses_pinned_decoders_dynamic_pda_and_oracle_gates() {
        let (_, mut s) = state();
        assert!(relationships(&s).is_ok());
        assert_eq!(program(&s), Err("PROGRAM_IDENTITY_MISMATCH"));
        assert!(oracles(&s).is_err());
        let target: MarginfiAccount = fe::typed(
            &s.state,
            &s.targets[0],
            &s.program,
            protocol::DISC_MARGINFI_ACCOUNT,
        )
        .unwrap();
        let bk = target.lending_account.balances[0].bank_pk;
        let bank: Bank = fe::typed(&s.state, &bk, &s.program, protocol::DISC_BANK).unwrap();
        s.state.0.get_mut(&bank.liquidity_vault).unwrap().data[32] ^= 1;
        assert_eq!(relationships(&s), Err("BOOT_VAULT_PDA_OR_MINT_MISMATCH"));
        s.state.0.get_mut(&bank.mint).unwrap().owner = Pubkey::new_unique();
        assert_eq!(
            relationships(&s),
            Err("BOOT_TOKEN_OWNER_OR_SCHEMA_UNQUALIFIED")
        );
    }
    #[test]
    fn network_fixture_never_promotes_unpinned_deployment_or_unknown_oracle() {
        let (mut c, s) = state();
        c.rpc_endpoints = vec!["https://rpc.invalid".into()];
        c.wss_endpoints = vec!["wss://rpc.invalid".into()];
        let spec = spec();
        let mut io = Fixture {
            genesis: spec.genesis.clone(),
            all: s.state.0,
            reads: 0,
            wss: true,
            wrong_slot: false,
        };
        let out = run(
            audit(),
            &c,
            &spec,
            &mut io,
            1000,
            "proof".into(),
            "fixture-binary".into(),
        )
        .unwrap();
        for id in ["BOOT-03", "BOOT-04", "BOOT-06", "BOOT-07"] {
            assert_eq!(
                out.audit.checks.iter().find(|c| c.id == id).unwrap().state,
                CheckState::Pass,
                "{id}"
            );
        }
        for id in ["BOOT-05", "BOOT-08"] {
            assert_eq!(
                out.audit.checks.iter().find(|c| c.id == id).unwrap().state,
                CheckState::Blocked
            );
        }
        assert_eq!(out.audit.verdict, AuditVerdict::Blocked);
        assert!(!out.accounts.is_empty());
    }
    #[test]
    fn native_network_record_and_start_token_reject_expiry_drift_and_tampering() {
        let mut a = audit();
        a.scope = "network".into();
        a.verdict = AuditVerdict::Ready;
        a.checks = [
            "BOOT-03", "BOOT-04", "BOOT-05", "BOOT-06", "BOOT-07", "BOOT-08",
        ]
        .iter()
        .map(|id| check(id, "fixture", Ok("fixture".into()), 100))
        .collect();
        let mut n = NetworkAudit {
            audit: a,
            observed_at: 1000,
            expires_at: 1060,
            slot: 100,
            genesis: spec().genesis,
            rpc_binding: "rpc".into(),
            wss_binding: "wss".into(),
            binary_sha256: "binary".into(),
            provider_proof_sha256: "proof".into(),
            capture_source: "logs".into(),
            accounts: Vec::<AccountProof>::new(),
            state_hash: "state".into(),
            ttl_seconds: 60,
            ttl_source: "builtin_default".into(),
        };
        n.audit.readiness_token.clear();
        n.audit.readiness_token = hash(&bincode::serialize(&n).unwrap());
        let id = n.audit.identity.clone();
        assert!(n.validate_start(&id, "binary", 1001, "logs").is_ok());
        assert_eq!(
            n.validate_start(&id, "binary", 1060, "logs").unwrap_err(),
            "BOOT_NETWORK_EXPIRED"
        );
        assert!(n.validate_start(&id, "changed", 1001, "logs").is_err());
        assert!(n
            .validate_start(&id, "binary", 1001, "transactions")
            .is_err());
        let dir = std::env::temp_dir().join(format!("boot_record_{}", Pubkey::new_unique()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("boot.p0j");
        let mut w = journal::Writer::create(&path).unwrap();
        w.append_typed(journal::Provenance::default(), &n).unwrap();
        w.seal(true).unwrap();
        assert_eq!(journal::read_typed_value::<NetworkAudit>(&path).unwrap(), n);
        n.genesis = "tampered".into();
        assert_eq!(
            n.validate_start(&id, "binary", 1001, "logs").unwrap_err(),
            "BOOT_NETWORK_TOKEN"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

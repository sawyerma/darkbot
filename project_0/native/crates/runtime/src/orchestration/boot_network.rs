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
use serde_json::{json, Value};
use solana_client::rpc_request::RpcRequest;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{hash::Hash, pubkey::Pubkey};
use std::{
    path::Path,
    str::FromStr,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .unwrap_or(0)
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
    pub ttl_seconds: u64,
}
impl Spec {
    pub fn validate(&self) -> Result<(), String> {
        Hash::from_str(&self.genesis).map_err(|_| "BOOT_GENESIS_PIN_REQUIRED")?;
        if self.ttl_seconds == 0 || self.ttl_seconds > 60 {
            return Err("BOOT_TOKEN_TTL".into());
        }
        Ok(())
    }
}
/// Narrow transport interface: no signer/submission operation exists. Fixtures use the same collector.
pub trait Reads {
    fn rpc(&mut self, request: RpcRequest, params: Value) -> Result<Value, String>;
    fn wss(&mut self, program: Pubkey, transactions: bool) -> Result<(u64, u64), String>;
}
pub struct NetworkReads {
    rpc_url: String,
    timeout: Duration,
    wss: String,
    end: Instant,
    calls: u64,
    limit: u64,
}
impl NetworkReads {
    pub fn new(c: &RuntimeConfig) -> Result<Self, String> {
        if !darkbot_solana::runtime_config::configured() || !provider::enabled() {
            return Err("BOOT_PROVIDER_NOT_INITIALIZED".into());
        }
        if c.fallback || c.rpc_endpoints.len() != 1 || c.wss_endpoints.len() != 1 {
            return Err("BOOT_PROVIDER_SCOPE_REQUIRES_SINGLE_RPC_WSS".into());
        }
        if !c.rpc_endpoints[0].starts_with("https://") || !c.wss_endpoints[0].starts_with("wss://")
        {
            return Err("BOOT_TLS_REQUIRED".into());
        }
        let seconds = c.limits.fe_seconds.min(300);
        if seconds == 0 {
            return Err("BOOT_TIME_BUDGET".into());
        }
        Ok(Self {
            rpc_url: c.rpc_endpoints[0].clone(),
            timeout: Duration::from_millis(c.limits.rpc_timeout_ms),
            wss: c.wss_endpoints[0].clone(),
            end: Instant::now() + Duration::from_secs(seconds),
            calls: 0,
            limit: c.limits.fe_calls.min(c.limits.max_requests),
        })
    }
}
impl Reads for NetworkReads {
    fn rpc(&mut self, request: RpcRequest, params: Value) -> Result<Value, String> {
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
        if Instant::now() >= self.end || self.calls >= self.limit {
            return Err("BOOT_RPC_BUDGET".into());
        }
        self.calls += 1;
        let rpc = provider::rpc_client(
            self.rpc_url.clone(),
            self.timeout
                .min(self.end.saturating_duration_since(Instant::now())),
            CommitmentConfig::finalized(),
            provider::Workload::Bootstrap,
        );
        let value = rpc
            .send(request, params)
            .map_err(|e| darkbot_solana::submit::rpc::safe_rpc_error(&e))?;
        if Instant::now() >= self.end {
            return Err("BOOT_TIME_BUDGET".into());
        }
        Ok(value)
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
    if io.rpc(RpcRequest::GetGenesisHash, json!([]))?.as_str() != Some(&spec.genesis) {
        return Err("BOOT_CLUSTER_MISMATCH".into());
    }
    let version = io.rpc(RpcRequest::GetVersion, json!([]))?;
    if version["solana-core"].as_str().is_none_or(|s| {
        s.is_empty()
            || s.len() > 64
            || !s
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b))
    }) {
        return Err("BOOT_CLUSTER_VERSION_UNKNOWN".into());
    }
    if io.rpc(RpcRequest::GetHealth, json!([]))?.as_str() != Some("ok") {
        return Err("BOOT_CLUSTER_UNHEALTHY".into());
    }
    let finalized = io
        .rpc(RpcRequest::GetSlot, json!([{"commitment":"finalized"}]))?
        .as_u64()
        .filter(|s| *s > 0)
        .ok_or("BOOT_SLOT_UNKNOWN")?;
    let current = io
        .rpc(RpcRequest::GetSlot, json!([{"commitment":"confirmed"}]))?
        .as_u64()
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
            |keys, floor| {
                let response=io.rpc(RpcRequest::GetMultipleAccounts,json!([keys.iter().map(ToString::to_string).collect::<Vec<_>>(),{"encoding":"base64","commitment":"finalized","minContextSlot":floor.max(slot)}]))?;
                fe::decode_batch(response, keys, floor.max(slot))
            },
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
    };
    result.audit.readiness_token.clear();
    result.audit.readiness_token = hash(&bincode::serialize(&result).map_err(|_| "BOOT_TOKEN")?);
    Ok(result)
}
/// Read permission and FREE_ONLY proof are checked before constructing clients or opening WSS.
pub fn execute(project: &Path, args: &[String]) -> Result<bool, String> {
    let val = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let operational = args.split(|a| a == "--").nth(1).unwrap_or(&[]);
    let resolved =
        darkbot_solana::runtime_config::Resolved::parse(operational, &|k| std::env::var(k).ok())?
            .native_only()?;
    resolved.activate()?;
    provider::enable_measurement_budget();
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
        ttl_seconds: 60,
    };
    spec.validate()?;
    let proof = val("--provider-evidence").ok_or("BOOT_PROVIDER_EVIDENCE_REQUIRED")?;
    let mut data = Vec::new();
    use std::io::Read;
    std::fs::File::open(proof)
        .map_err(|_| "BOOT_PROVIDER_EVIDENCE_IO")?
        .take(65537)
        .read_to_end(&mut data)
        .map_err(|_| "BOOT_PROVIDER_EVIDENCE_IO")?;
    if data.len() > 65536 {
        return Err("BOOT_PROVIDER_EVIDENCE_LIMIT".into());
    }
    let proof: Value =
        serde_json::from_slice(&data).map_err(|_| "BOOT_PROVIDER_EVIDENCE_FORMAT")?;
    fe::validate_provider_evidence(
        c,
        c.rpc_endpoints.first().ok_or("BOOT_RPC_REQUIRED")?,
        &proof,
        now() as f64,
    )?;
    if proof["wss_provider_binding"].as_str()
        != c.wss_endpoints
            .first()
            .map(|s| hash(s.as_bytes()))
            .as_deref()
        || proof["wss_free_only"] != true
    {
        return Err("BOOT_WSS_FREE_ONLY_UNQUALIFIED".into());
    }
    if c.verification != "off" && proof["archive_qualified"] != true {
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
        now(),
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
    use base64::Engine;
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
        fn rpc(&mut self, r: RpcRequest, p: Value) -> Result<Value, String> {
            self.reads += 1;
            Ok(match r {
                RpcRequest::GetGenesisHash => json!(self.genesis),
                RpcRequest::GetVersion => json!({"solana-core":"3.1.12"}),
                RpcRequest::GetHealth => json!("ok"),
                RpcRequest::GetSlot => {
                    json!(if self.wrong_slot && p[0]["commitment"] == "confirmed" {
                        99
                    } else {
                        100
                    })
                }
                RpcRequest::GetMultipleAccounts => {
                    json!({"context":{"slot":100},"value":p[0].as_array().unwrap().iter().map(|key|{
          self.all.get(&Pubkey::from_str(key.as_str().unwrap()).unwrap()).map(|a|json!({"owner":a.owner.to_string(),"lamports":a.lamports,"executable":a.executable,"data":[base64::engine::general_purpose::STANDARD.encode(&a.data),"base64"]})).unwrap_or(Value::Null)
       }).collect::<Vec<_>>()})
                }
                _ => panic!("non-read operation"),
            })
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
            genesis: Hash::new_unique().to_string(),
            transactions: false,
            ttl_seconds: 60,
        }
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

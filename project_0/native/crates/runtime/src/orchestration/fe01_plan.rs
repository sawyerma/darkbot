//! DEC-18 §6 (C-01 step 6): the FE-01 full observation's read plan. Offline, `p0-fe-qualify
//! --fe-mode full --plan --provider-evidence ID --output DIR` binds the selected RPC provider
//! (registry version and binding), the evidence record, the effective configuration with its
//! bounds and the executable into `DIR/fe01_plan.p0j` and prints its token; nothing is read and
//! no credential is opened. `--execute --approve TOKEN --output DIR` observes exactly that plan,
//! once: another token, a second run, or a provider, credential revision, configuration,
//! evidence or executable changed since the plan is refused before any read.
pub use super::c01_records::Fe01Plan;
use super::fe_qualification as fe;
use super::provider_evidence::ProviderEvidence;
use super::provider_measurement::ProviderRef;
use crate::analytics::journal;
use darkbot_solana::providers::{ProviderKind, ProviderRegistry};
use darkbot_solana::runtime_config::{EffectiveConfig, Resolved};
use std::path::Path;

pub const PLAN: &str = "fe01_plan.p0j";
const STARTED: &str = "run.started";

/// The observation's configuration: bound to the project's registry, its limits within the
/// FE-01 bounds (`fe_calls`, `fe_max_mib`). Returns the registry, the RPC URL and the
/// configuration as requested (before the bounds).
pub fn shape(
    project: &Path,
    resolved: &mut Resolved,
) -> Result<(ProviderRegistry, String, EffectiveConfig), String> {
    let registry = super::provider_registry::bind(project, resolved)?;
    let url = resolved.config.rpc_url().ok_or("RPC_URL_MISSING")?;
    let requested = resolved.effective();
    let l = &mut resolved.config.limits;
    l.max_requests = l.max_requests.min(l.fe_calls);
    l.max_total_mib = l.max_total_mib.min(l.fe_max_mib);
    l.max_response_mib = l.max_response_mib.min(l.max_total_mib);
    l.wss_message_mib = l.wss_message_mib.min(l.max_total_mib);
    Ok((registry, url, requested))
}

fn now_s() -> Result<f64, String> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "CLOCK_INVALID")?
        .as_secs_f64())
}

fn rpc_ref(resolved: &Resolved, url: &str) -> Result<ProviderRef, String> {
    let b = resolved.config.bound(ProviderKind::Rpc, url)?;
    Ok(ProviderRef {
        id: b.id.clone(),
        binding: b.binding.clone(),
    })
}

fn digest(resolved: &Resolved) -> Result<String, String> {
    Ok(fe::sha(
        &bincode::serialize(&resolved.effective()).map_err(|_| "FE_PLAN_CONFIG")?,
    ))
}

/// The evidence record (its bytes and proof), admitted for this configuration now.
fn evidence(
    project: &Path,
    resolved: &Resolved,
    url: &str,
    id: &str,
) -> Result<(Vec<u8>, ProviderEvidence), String> {
    let (bytes, proof) = super::provider_evidence::load(project, id)?;
    fe::validate_provider_evidence(&resolved.config, url, &proof, now_s()?)?;
    Ok((bytes, proof))
}

/// `--plan`: the offline step. `resolved` is shaped as the observation runs; the plan is
/// refused when the configured calls cannot even cover the initial scope.
pub fn plan(
    project: &Path,
    resolved: &mut Resolved,
    evidence_id: &str,
    funding_class: &str,
    output: &Path,
) -> Result<(Fe01Plan, String), String> {
    let (registry, url, _) = shape(project, resolved)?;
    let (bytes, _) = evidence(project, resolved, &url, evidence_id)?;
    let c = &resolved.config;
    let l = &c.limits;
    let required_reads = fe::required_scope_reads(c)?;
    if required_reads > l.max_requests {
        return Err(format!(
            "FE_PLAN_EXCEEDS_BOUNDS: the initial scope alone needs {required_reads} reads, the bound is {}",
            l.max_requests
        ));
    }
    let plan = Fe01Plan {
        plan_id: fe::sha(super::provider_registry::fresh_nonce()?.as_bytes())[..16].to_string(),
        created_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_millis() as u64),
        registry_version: registry.version,
        rpc: rpc_ref(resolved, &url)?,
        evidence_id: evidence_id.into(),
        evidence_sha256: fe::sha(&bytes),
        config_digest: digest(resolved)?,
        binary_sha256: super::provider_measure::executable_sha256()?,
        funding_class: funding_class.into(),
        group: c.fe_group.clone(),
        targets: c.fe_targets.clone(),
        calls: l.max_requests,
        seconds: l.fe_seconds,
        mib: l.max_total_mib,
        rps: l.rpc_rps,
        batch: l.fe_batch_size,
        rounds: l.fe_snapshot_rounds,
        required_reads,
        worst_case_reads: fe::worst_case_scope_reads(c)?,
    };
    std::fs::create_dir(output).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => "FE_OUTPUT_EXISTS_PRESERVE_EVIDENCE".to_string(),
        _ => format!("FE_PLAN_STORE: {e}"),
    })?;
    let path = output.join(PLAN);
    journal::create_typed_value(&path, journal::source::FE01, &plan)
        .map_err(|e| format!("FE_PLAN_STORE: {e}"))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("FE_PLAN_STORE: {e}"))?;
    Ok((plan, super::provider_measure::token(&bytes)))
}

/// What an approved plan runs with: the plan, the bound registry, the RPC URL, the configuration
/// as requested, the evidence record's bytes and proof.
pub struct Approved {
    pub plan: Fe01Plan,
    pub registry: ProviderRegistry,
    pub url: String,
    pub requested: EffectiveConfig,
    pub record: Vec<u8>,
    pub proof: ProviderEvidence,
}

/// `--execute --approve TOKEN`: the checks of exactly the approved plan; `resolved` is shaped as
/// planned. [`start`] then marks it used.
pub fn approve(
    project: &Path,
    resolved: &mut Resolved,
    output: &Path,
    approve: &str,
) -> Result<Approved, String> {
    let path = output.join(PLAN);
    let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            "FE_PLAN_REQUIRED: an observation runs an approved plan (--plan, then --approve TOKEN)"
                .to_string()
        }
        _ => format!("FE_PLAN_STORE: {e}"),
    })?;
    if approve.len() != 16 || super::provider_measure::token(&bytes) != approve {
        return Err(
            "FE_PLAN_APPROVAL_MISMATCH: the approval names another plan's token; nothing read"
                .into(),
        );
    }
    let plan: Fe01Plan =
        journal::read_typed_value(&path).map_err(|_| "FE_PLAN_FORMAT".to_string())?;
    let (registry, url, requested) = shape(project, resolved)?;
    let drift = |what: &str| {
        format!(
            "FE_PLAN_DRIFT: {what} changed since the plan; a new plan is required (nothing read)"
        )
    };
    if registry.version != plan.registry_version || rpc_ref(resolved, &url)? != plan.rpc {
        return Err(drift("the provider or its credential revision"));
    }
    if digest(resolved)? != plan.config_digest {
        return Err(drift("the configuration"));
    }
    if super::provider_measure::executable_sha256()? != plan.binary_sha256 {
        return Err(drift("the observing executable"));
    }
    let (record, proof) = evidence(project, resolved, &url, &plan.evidence_id)?;
    if fe::sha(&record) != plan.evidence_sha256 {
        return Err(drift("the evidence record"));
    }
    if output.join(STARTED).exists() {
        return Err(used());
    }
    Ok(Approved {
        plan,
        registry,
        url,
        requested,
        record,
        proof,
    })
}

/// Whether the plan in `output` started reading.
pub fn started(output: &Path) -> bool {
    output.join(STARTED).is_file()
}

fn used() -> String {
    "FE_PLAN_USED: this plan ran before (a new attempt is a new plan)".into()
}

/// The approved plan starts reading: once (of concurrent starts exactly one wins), after every
/// check and the credentials, right before the first read.
pub fn start(output: &Path) -> Result<(), String> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join(STARTED))
        .map(|_| ())
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => used(),
            _ => format!("FE_PLAN_STORE: {e}"),
        })
}

/// The operator's view of a plan.
pub fn plan_text(plan: &Fe01Plan, token: &str, output: &Path) -> String {
    format!(
        "FE01_PLAN id={} token={token}\nprovider rpc={} ({}) registry v{} evidence {} ({})\nscope group {} targets {} funding_class {}\nbounds calls={} seconds={} mib={} rps={} batch={} rounds={}\nreads: the program, its ProgramData (base64+zstd), every round over the scope without the ProgramData, then the ProgramData once more; {} reads for the initial scope, {} in the theoretical worst case (the bounds end an observation INCOMPLETE, never beyond them)\napprove: p0-fe-qualify --fe-mode full --execute --approve {token} --output {}   (C-01 step 6: reads Mainnet through the selected provider)\n",
        plan.plan_id,
        plan.rpc.id,
        &plan.rpc.binding[..16.min(plan.rpc.binding.len())],
        plan.registry_version,
        plan.evidence_id,
        &plan.evidence_sha256[..16.min(plan.evidence_sha256.len())],
        plan.group,
        plan.targets.join(","),
        plan.funding_class,
        plan.calls,
        plan.seconds,
        plan.mib,
        plan.rps,
        plan.batch,
        plan.rounds,
        plan.required_reads,
        plan.worst_case_reads,
        output.display()
    )
}

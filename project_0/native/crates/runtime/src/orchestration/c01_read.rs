//! DEC-18 §2 (P0-025, approval C-01 steps 5–6): the deployment read `p0ctl c01 plan|run`. Read
//! once, finalized, through the selected RPC provider, inside the plan's approved budget (the
//! FE-01 scope's `fe_calls`, `fe_max_mib`, `fe_seconds`) and only after the operator approved
//! exactly the plan's token:
//! * the program and its ProgramData — executable, deployment slot, upgrade authority, the ELF;
//! * its Anchor IDL;
//! * the group and its FeeState;
//! * every bank of the group (the resize census);
//! * the oracles and integrations those banks observe;
//! * the token programs' bytecode.
//!
//! The parity verdict (`P1A_PARITY`) is derived from the typed reads: PASS only when every mandatory
//! gate passes; the build (`p0ctl c01 build`, offline) and the FE-01 observation are gates of
//! their own steps, each recorded once beside the reads and the parity derived again.
//!
//! `<project>/data/c01/<plan_id>/`: `plan.p0j`, `run.started`, `reads.p0j`, `parity.p0j`,
//! `programs/<name>.so`, `build.p0j`, `fe01.p0j` and the parity of the recorded steps
//! (`parity-build.p0j`, `parity-fe01.p0j`, `parity-build-fe01.p0j`).
pub use super::c01_records::*;
use crate::analytics::journal;
use crate::analytics::payload::{Fe01Account, Fe01ReadOutcome};
use darkbot_project0::protocol;
use darkbot_solana::providers::ProviderKind;
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, Instant};

const PLAN: &str = "plan.p0j";
const STARTED: &str = "run.started";
const READS: &str = "reads.p0j";
const PARITY: &str = "parity.p0j";
const BUILD: &str = "build.p0j";
const FE01: &str = "fe01.p0j";

/// The evidence of the steps after the reads, each recorded once by its own step.
#[derive(Default)]
pub struct Steps {
    pub build: Option<C01Build>,
    pub fe01: Option<C01Fe01>,
}
impl Steps {
    /// The parity record of exactly these steps: `parity.p0j` for the reads alone.
    fn parity_file(&self) -> String {
        match (&self.build, &self.fe01) {
            (None, None) => PARITY.into(),
            (Some(_), None) => "parity-build.p0j".into(),
            (None, Some(_)) => "parity-fe01.p0j".into(),
            (Some(_), Some(_)) => "parity-build-fe01.p0j".into(),
        }
    }

    /// The steps recorded beside a run's reads.
    fn recorded(dir: &Path) -> Result<Self, String> {
        fn load<P: crate::analytics::payload::Payload>(path: &Path) -> Result<Option<P>, String> {
            match journal::read_typed_value(path) {
                Ok(v) => Ok(Some(v)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("C01_STORE: {}: {e}", path.display())),
            }
        }
        Ok(Self {
            build: load(&dir.join(BUILD))?,
            fe01: load(&dir.join(FE01))?,
        })
    }
}

/// Records a step once and derives the parity of every step recorded.
fn record_step<P: crate::analytics::payload::Payload>(
    dir: &Path,
    file: &str,
    value: &P,
    reads: &C01Reads,
    reads_sha256: &str,
) -> Result<P1aParity, String> {
    journal::create_typed_value(
        &dir.join(file),
        journal::source::PROVIDER_MEASUREMENT,
        value,
    )
    .map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => {
            format!("C01_STEP_RECORDED: {file} of this plan is recorded (a new one is a new plan)")
        }
        _ => format!("C01_STORE: {e}"),
    })?;
    let steps = Steps::recorded(dir)?;
    let parity = evaluate(reads, reads_sha256, &steps);
    journal::write_typed_value(
        &dir.join(steps.parity_file()),
        journal::source::PROVIDER_MEASUREMENT,
        &parity,
    )
    .map_err(|e| format!("C01_STORE: {e}"))?;
    Ok(parity)
}

fn sha(bytes: &[u8]) -> String {
    journal::hex(&solana_sdk::hash::hash(bytes).to_bytes())
}

/// The token programs whose bytecode C-10 and the EXIT-WSOL proof read (DEC-17 d).
const PROGRAMS: [(&str, &str); 3] = [
    ("token", protocol::TOKEN_PROGRAM_ID),
    ("token-2022", protocol::TOKEN_2022_PROGRAM_ID),
    ("associated-token", protocol::ASSOCIATED_TOKEN_PROGRAM_ID),
];

/// The read's configuration: the project's selected RPC provider (FREE_ONLY, no fallback), its
/// calls and bytes the FE-01 scope's.
fn configure(
    project: &Path,
    get: &dyn Fn(&str) -> Option<String>,
) -> Result<
    (
        darkbot_solana::runtime_config::Resolved,
        darkbot_solana::providers::ProviderRegistry,
    ),
    String,
> {
    let mut resolved = darkbot_solana::runtime_config::Resolved::parse(&[], get)?;
    let registry = super::provider_registry::bind(project, &mut resolved)?;
    let c = &mut resolved.config;
    if c.fallback || c.rpc_endpoints.len() != 1 {
        return Err("C01_PROVIDER_REQUIRED: exactly one selected RPC provider, no fallback (p0ctl provider select)".into());
    }
    super::provider_evidence::free_registry_provider(
        c.bound(ProviderKind::Rpc, &c.rpc_endpoints[0])?,
    )?;
    let l = &mut c.limits;
    l.max_requests = l.max_requests.min(l.fe_calls);
    l.max_total_mib = l.max_total_mib.min(l.fe_max_mib);
    l.max_response_mib = l.max_response_mib.min(l.max_total_mib);
    l.wss_message_mib = l.wss_message_mib.min(l.max_total_mib);
    Ok((resolved, registry))
}

fn rpc_ref(
    c: &darkbot_solana::runtime_config::RuntimeConfig,
) -> Result<super::provider_measurement::ProviderRef, String> {
    let b = c.bound(ProviderKind::Rpc, &c.rpc_endpoints[0])?;
    Ok(super::provider_measurement::ProviderRef {
        id: b.id.clone(),
        binding: b.binding.clone(),
    })
}

fn program_and_group(
    c: &darkbot_solana::runtime_config::RuntimeConfig,
) -> Result<(Pubkey, Pubkey), String> {
    Ok((
        Pubkey::from_str(protocol::PROGRAM_ID_MAINNET).map_err(|_| "C01_PROGRAM")?,
        Pubkey::from_str(&c.fe_group).map_err(|_| "C01_GROUP: fe_group")?,
    ))
}

fn now_ms() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

fn plan_dir(project: &Path, plan_id: &str) -> Result<PathBuf, String> {
    if !super::provider_evidence::is_id(plan_id) {
        return Err("C01_PLAN_ID_INVALID: a plan id is 16 lowercase hex digits".into());
    }
    Ok(project.join(STORE).join(plan_id))
}

/// `p0ctl c01 plan`: the offline step — the provider, the budget and the reads the operator
/// approves; nothing is read and no credential is opened. Returns the plan and its token.
pub fn plan(
    project: &Path,
    get: &dyn Fn(&str) -> Option<String>,
) -> Result<(C01Plan, String), String> {
    let (resolved, registry) = configure(project, get)?;
    let c = &resolved.config;
    let (program, group) = program_and_group(c)?;
    let l = &c.limits;
    let batch = l.fe_batch_size;
    let plan = C01Plan {
        plan_id: sha(super::provider_registry::fresh_nonce()?.as_bytes())[..16].to_string(),
        created_unix_ms: now_ms(),
        registry_version: registry.version,
        rpc: rpc_ref(c)?,
        config_digest: sha(&bincode::serialize(&resolved.effective()).map_err(|_| "C01_CONFIG")?),
        binary_sha256: super::provider_measure::executable_sha256()?,
        program: program.to_string(),
        group: group.to_string(),
        budget: C01Budget {
            calls: l.max_requests,
            mib: l.max_total_mib,
            seconds: l.fe_seconds,
            rps: l.rpc_rps,
            batch,
        },
        reads: vec![
            "getMultipleAccounts finalized base64: the program, its IDL account, the group, the FeeState".into(),
            "getMultipleAccounts finalized base64: the ProgramData (the whole deployed binary)".into(),
            "getProgramAccounts finalized base64 withContext: every bank of the group (bank discriminator, group at 41)".into(),
            format!("getMultipleAccounts finalized base64, at most {batch} keys each: every oracle and integration account the banks observe"),
            "getMultipleAccounts finalized base64: the Token, Token-2022 and Associated Token programs, then the ProgramData of an upgradeable one".into(),
        ],
    };
    let dir = plan_dir(project, &plan.plan_id)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("C01_STORE: {e}"))?;
    let path = dir.join(PLAN);
    journal::create_typed_value(&path, journal::source::PROVIDER_MEASUREMENT, &plan)
        .map_err(|e| format!("C01_STORE: {e}"))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("C01_STORE: {e}"))?;
    Ok((plan, super::provider_measure::token(&bytes)))
}

fn load_plan(project: &Path, plan_id: &str) -> Result<(C01Plan, Vec<u8>), String> {
    let path = plan_dir(project, plan_id)?.join(PLAN);
    let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "C01_PLAN_UNKNOWN".to_string(),
        _ => format!("C01_STORE: {e}"),
    })?;
    let plan: C01Plan =
        journal::read_typed_value(&path).map_err(|_| "C01_PLAN_FORMAT".to_string())?;
    if std::fs::read(&path).map_err(|e| format!("C01_STORE: {e}"))? != bytes
        || plan.plan_id != plan_id
    {
        return Err("C01_PLAN_CHANGED".into());
    }
    Ok((plan, bytes))
}

/// `p0ctl c01 run --plan ID --approve TOKEN`: the reads of exactly the approved plan, once.
/// Refused before any read on another plan's token, a second run, or a provider, credential
/// revision, configuration or executable changed since the plan. Writes the reads and their
/// parity verdict.
pub fn run(
    project: &Path,
    plan_id: &str,
    approve: &str,
    get: &dyn Fn(&str) -> Option<String>,
) -> Result<(C01Reads, P1aParity), String> {
    let (plan, bytes) = load_plan(project, plan_id)?;
    if approve.len() != 16 || super::provider_measure::token(&bytes) != approve {
        return Err(
            "C01_APPROVAL_MISMATCH: the approval names another plan's token; nothing read".into(),
        );
    }
    let (resolved, registry) = configure(project, get)?;
    let drift = |what: &str| {
        format!(
            "C01_PLAN_DRIFT: {what} changed since the plan; a new plan is required (nothing read)"
        )
    };
    if registry.version != plan.registry_version || rpc_ref(&resolved.config)? != plan.rpc {
        return Err(drift("the provider or its credential revision"));
    }
    if sha(&bincode::serialize(&resolved.effective()).map_err(|_| "C01_CONFIG")?)
        != plan.config_digest
    {
        return Err(drift("the configuration"));
    }
    if super::provider_measure::executable_sha256()? != plan.binary_sha256 {
        return Err(drift("the reading executable"));
    }
    let dir = plan_dir(project, plan_id)?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(STARTED))
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => {
                "C01_PLAN_USED: this plan ran before (a new attempt is a new plan)".to_string()
            }
            _ => format!("C01_STORE: {e}"),
        })?;
    resolved.activate()?;
    let c = darkbot_solana::runtime_config::active();
    super::provider_registry::install_credentials(&registry, c)?;
    let reads = read(&plan, &bytes, c, &dir)?;
    journal::create_typed_value(
        &dir.join(READS),
        journal::source::PROVIDER_MEASUREMENT,
        &reads,
    )
    .map_err(|e| format!("C01_STORE: {e}"))?;
    let reads_bytes = std::fs::read(dir.join(READS)).map_err(|e| format!("C01_STORE: {e}"))?;
    let parity = evaluate(&reads, &sha(&reads_bytes), &Steps::default());
    journal::create_typed_value(
        &dir.join(PARITY),
        journal::source::PROVIDER_MEASUREMENT,
        &parity,
    )
    .map_err(|e| format!("C01_STORE: {e}"))?;
    Ok((reads, parity))
}

/// A run's reads and the SHA-256 of their record.
fn load_reads(dir: &Path) -> Result<(C01Reads, String), String> {
    let path = dir.join(READS);
    let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "C01_READS_MISSING: the plan has not run".to_string(),
        _ => format!("C01_STORE: {e}"),
    })?;
    let reads: C01Reads =
        journal::read_typed_value(&path).map_err(|_| "C01_READS_FORMAT".to_string())?;
    Ok((reads, sha(&bytes)))
}

/// `p0ctl c01 build --plan ID --source DIR --verifier PATH --image NAME@sha256:DIGEST`: offline —
/// runs, once per plan, the pinned source's Mainnet build recipe (`protocol::SOURCE_LIBRARY`,
/// `SOURCE_BUILD_FEATURES`, `SOURCE_BUILD_ENV`) through the verifier in the image named by its
/// digest, on a clean clone of the source, hashes the ELF that build produced and derives the
/// parity again. Refused before anything runs: a clone whose `HEAD` is not the pinned commit or
/// that holds any other file (tracked change, untracked or ignored — no ELF planted beside the
/// build), an image not named by its digest. A build whose ELF is not the deployed one is a
/// recorded BUILD FAIL; a failed build records nothing.
pub fn build(
    project: &Path,
    plan_id: &str,
    source: &Path,
    verifier: &Path,
    image: &str,
) -> Result<(C01Reads, P1aParity), String> {
    let dir = plan_dir(project, plan_id)?;
    let (reads, reads_sha) = load_reads(&dir)?;
    if dir.join(BUILD).exists() {
        return Err(format!(
            "C01_STEP_RECORDED: {BUILD} of this plan is recorded (a new one is a new plan)"
        ));
    }
    let git = |args: &[&str]| -> Result<String, String> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(source)
            .args(args)
            .output()
            .map_err(|e| format!("C01_BUILD_SOURCE: git: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "C01_BUILD_SOURCE: {} is no git checkout",
                source.display()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let head = git(&["rev-parse", "HEAD"])?;
    if head != protocol::SOURCE_COMMIT {
        return Err(format!(
            "C01_BUILD_SOURCE: HEAD {head} is not the pinned {} ({})",
            protocol::SOURCE_COMMIT,
            protocol::SOURCE_TAG
        ));
    }
    if !git(&[
        "status",
        "--porcelain",
        "--ignored",
        "--untracked-files=all",
    ])?
    .is_empty()
    {
        return Err("C01_BUILD_SOURCE_NOT_CLEAN: build from a clean clone of the pinned commit (no changed, untracked or ignored file)".into());
    }
    let digest = image.rsplit_once("@sha256:").map(|(_, d)| d);
    if !digest.is_some_and(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err(
            "C01_BUILD_IMAGE: the build image by its digest (<name>@sha256:<64 hex>)".into(),
        );
    }
    let verifier_bytes = std::fs::read(verifier).map_err(|e| format!("C01_BUILD_VERIFIER: {e}"))?;
    let run = |args: &[&str]| {
        let mut c = std::process::Command::new(verifier);
        c.args(args).env_clear();
        // The verifier drives docker: its search path, home and docker settings, nothing else.
        for k in ["PATH", "HOME", "TMPDIR", "DOCKER_HOST", "DOCKER_CONFIG"] {
            if let Some(v) = std::env::var_os(k) {
                c.env(k, v);
            }
        }
        c.envs(protocol::SOURCE_BUILD_ENV);
        c.output().map_err(|e| format!("C01_BUILD_VERIFIER: {e}"))
    };
    let version = run(&["--version"])?;
    let verifier_version = String::from_utf8_lossy(&version.stdout).trim().to_string();
    let source_arg = source.to_string_lossy().to_string();
    let args = [
        "build",
        "-b",
        image,
        "--library-name",
        protocol::SOURCE_LIBRARY,
        source_arg.as_str(),
        "--",
        "--features",
        protocol::SOURCE_BUILD_FEATURES,
    ];
    let out = run(&args)?;
    if !out.status.success() {
        let tail = String::from_utf8_lossy(&out.stderr);
        let tail: String = tail.lines().rev().take(5).collect::<Vec<_>>().join(" | ");
        return Err(format!(
            "C01_BUILD_FAILED: the verifier exited {} ({tail}); nothing recorded",
            out.status
        ));
    }
    let elf = source
        .join("target/deploy")
        .join(format!("{}.so", protocol::SOURCE_LIBRARY));
    let bytes =
        std::fs::read(&elf).map_err(|e| format!("C01_BUILD_ELF: {}: {e}", elf.display()))?;
    if !bytes.starts_with(b"\x7fELF") {
        return Err("C01_BUILD_ELF: the build produced no ELF".into());
    }
    let built = protocol::trimmed_elf(&bytes);
    let record = C01Build {
        plan_id: plan_id.into(),
        source_commit: head,
        source_modified: false,
        image: image.into(),
        verifier_sha256: sha(&verifier_bytes),
        verifier_version,
        command: format!(
            "{} {} {}",
            protocol::SOURCE_BUILD_ENV
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(" "),
            verifier.display(),
            args.join(" ")
        ),
        elf_sha256: sha(built),
        elf_len: built.len() as u64,
        recorded_unix_ms: now_ms(),
    };
    let parity = record_step(&dir, BUILD, &record, &reads, &reads_sha)?;
    Ok((reads, parity))
}

/// `p0ctl c01 fe01 --plan ID --observation DIR`: offline — records, once per plan, what the
/// FE-01 full observation in `DIR` (`p0-fe-qualify --fe-mode full --execute`, C-01 step 6)
/// configured and observed, and derives the parity again. Its journal is read strictly (chain
/// and manifest); the FE01 gate compares the observed deployment with the read one.
pub fn fe01(
    project: &Path,
    plan_id: &str,
    observation: &Path,
) -> Result<(C01Reads, P1aParity), String> {
    use crate::analytics::payload::{Fe01Config, Fe01Report, Manifest};
    let dir = plan_dir(project, plan_id)?;
    let (reads, reads_sha) = load_reads(&dir)?;
    let path = observation.join(super::fe_replay::FE01_JOURNAL);
    let bad = |e: std::io::Error| format!("C01_FE01_JOURNAL: {e}");
    let bytes = std::fs::read(&path).map_err(bad)?;
    let mut reader = journal::Reader::open(&path).map_err(bad)?;
    let (mut config, mut report, mut complete) = (None, None, false);
    while let Some(f) = reader.next_frame().map_err(bad)? {
        match f.kind {
            journal::kind::FE01_CONFIG => config = Some(f.typed::<Fe01Config>().map_err(bad)?),
            journal::kind::FE01_RESULT => report = Some(f.typed::<Fe01Report>().map_err(bad)?),
            journal::MANIFEST => complete = f.typed::<Manifest>().map_err(bad)?.complete,
            _ => {}
        }
    }
    let config = config.ok_or("C01_FE01_JOURNAL: no configuration frame")?;
    let report = report.ok_or("C01_FE01_JOURNAL: no result frame")?;
    // An observation that did not complete is no evidence about the deployment: refused before
    // anything is recorded, the plan's step stays open for a complete one.
    if !complete || report.status != super::fe_qualification::OBSERVED {
        return Err(format!(
            "C01_FE01_INCOMPLETE: {} {} (sealed complete: {complete}); nothing recorded",
            report.status,
            report.reason.as_deref().unwrap_or("-")
        ));
    }
    // The observation of an approved FE-01 plan of this C-01 scope: the plan beside it, started,
    // its evidence and its configuration exactly the journal's.
    let fe01_plan: super::fe01_plan::Fe01Plan =
        journal::read_typed_value(&observation.join(super::fe01_plan::PLAN)).map_err(|_| {
            "C01_FE01_UNPLANNED: no FE-01 plan beside the observation; nothing recorded"
        })?;
    if !super::fe01_plan::started(observation)
        || config.qualification_sha256.as_deref() != Some(fe01_plan.evidence_sha256.as_str())
        || sha(&bincode::serialize(&config.effective).map_err(|_| "C01_FE01_CONFIG")?)
            != fe01_plan.config_digest
    {
        return Err("C01_FE01_UNPLANNED: the observation is not its plan's (start, evidence or configuration differ); nothing recorded".into());
    }
    let (c01_plan, _) = load_plan(project, plan_id)?;
    if fe01_plan.group != c01_plan.group {
        return Err(format!(
            "C01_FE01_SCOPE: the observation's group {} is not the C-01 plan's {}; nothing recorded",
            fe01_plan.group, c01_plan.group
        ));
    }
    let o = report.observation.as_ref();
    let record = C01Fe01 {
        plan_id: plan_id.into(),
        fe01_plan_id: fe01_plan.plan_id.clone(),
        group: fe01_plan.group.clone(),
        scope_targets: fe01_plan.targets.clone(),
        evidence_sha256: fe01_plan.evidence_sha256.clone(),
        journal_sha256: sha(&bytes),
        sealed_complete: complete,
        execution: config.execution,
        status: report.status.clone(),
        reason: report.reason.clone(),
        slot: o.map(|o| o.slot),
        program_data: o.map(|o| o.program_data.to_string()),
        program_data_sha256: o.map(|o| o.program_data_sha256.clone()),
        deployment_slot: o.map(|o| o.deployment_slot),
        accounts: o.map_or(0, |o| o.accounts.len() as u64),
        targets: o.map_or(0, |o| o.targets.len() as u64),
        recorded_unix_ms: now_ms(),
    };
    let parity = record_step(&dir, FE01, &record, &reads, &reads_sha)?;
    Ok((reads, parity))
}

/// The finalized reads of one run, their context never regressing.
struct Reader {
    rpc: darkbot_solana::provider::DeadlineRpc,
    deadline: Instant,
    floor: u64,
    slots: Vec<u64>,
}
impl Reader {
    fn call(
        &mut self,
        request: solana_client::rpc_request::RpcRequest,
        params: Value,
    ) -> Result<Value, String> {
        use darkbot_solana::provider::CallError;
        self.rpc
            .call(request, params, self.deadline)
            .map_err(|e| match e {
                CallError::Remote(e) => format!("C01_READ_REMOTE: {e}"),
                CallError::Budget(c) => format!("C01_BUDGET: {c}"),
                CallError::Deadline(c) => format!("C01_DEADLINE: {c}"),
                CallError::Transport(c) => format!("C01_TRANSPORT: {c}"),
            })
    }
    fn context(&mut self, slot: u64) -> Result<(), String> {
        if slot < self.floor {
            return Err(format!("C01_CONTEXT_REGRESSED: {slot} < {}", self.floor));
        }
        self.floor = slot;
        self.slots.push(slot);
        Ok(())
    }
    fn multiple(&mut self, keys: &[Pubkey]) -> Result<Vec<Option<Fe01Account>>, String> {
        let params = json!([
            keys.iter().map(ToString::to_string).collect::<Vec<_>>(),
            {"encoding":"base64","commitment":"finalized","minContextSlot":self.floor}
        ]);
        let v = self.call(
            solana_client::rpc_request::RpcRequest::GetMultipleAccounts,
            params,
        )?;
        let wire: super::fe_qualification::WireMultipleAccounts =
            serde_json::from_value(v).map_err(|_| "C01_WIRE")?;
        match wire.outcome() {
            Fe01ReadOutcome::Accounts {
                context_slot,
                accounts,
            } if accounts.len() == keys.len() => {
                self.context(context_slot)?;
                Ok(accounts)
            }
            Fe01ReadOutcome::Accounts { .. } => Err("C01_WIRE: account count".into()),
            Fe01ReadOutcome::Failed { reason } => Err(reason),
        }
    }
    /// The group's banks (`getProgramAccounts`, the bank filters).
    fn banks(
        &mut self,
        program: &Pubkey,
        group: &Pubkey,
    ) -> Result<Vec<(Pubkey, Fe01Account)>, String> {
        #[derive(serde::Deserialize)]
        struct Wire {
            context: Ctx,
            value: Vec<Keyed>,
        }
        #[derive(serde::Deserialize)]
        struct Ctx {
            slot: u64,
        }
        #[derive(serde::Deserialize)]
        struct Keyed {
            pubkey: String,
            account: Account,
        }
        #[derive(serde::Deserialize)]
        struct Account {
            owner: String,
            lamports: u64,
            executable: bool,
            data: (String, String),
        }
        use base64::Engine;
        let filters: Vec<Value> = super::state_owner::bank_filters(group)
            .iter()
            .map(darkbot_solana::ingress::ProgramFilter::wire)
            .collect();
        let params = json!([program.to_string(), {"encoding":"base64","commitment":"finalized","withContext":true,"minContextSlot":self.floor,"filters":filters}]);
        let v = self.call(
            solana_client::rpc_request::RpcRequest::GetProgramAccounts,
            params,
        )?;
        let wire: Wire = serde_json::from_value(v).map_err(|_| "C01_WIRE")?;
        self.context(wire.context.slot)?;
        wire.value
            .into_iter()
            .map(|k| {
                if k.account.data.1 != "base64" {
                    return Err("C01_ACCOUNT_ENCODING".to_string());
                }
                Ok((
                    Pubkey::from_str(&k.pubkey).map_err(|_| "C01_WIRE")?,
                    Fe01Account {
                        owner: Pubkey::from_str(&k.account.owner).map_err(|_| "C01_WIRE")?,
                        lamports: k.account.lamports,
                        executable: k.account.executable,
                        data: base64::engine::general_purpose::STANDARD
                            .decode(k.account.data.0)
                            .map_err(|_| "C01_ACCOUNT_ENCODING")?,
                    },
                ))
            })
            .collect()
    }
}

/// The IDL account (Anchor: discriminator, authority, length, zlib-compressed JSON).
fn idl(address: Pubkey, program: &Pubkey, account: Option<&Fe01Account>, limit: u64) -> C01Idl {
    let mut out = C01Idl {
        address: address.to_string(),
        present: account.is_some(),
        authority: None,
        compressed_sha256: None,
        sha256: None,
        instructions: vec![],
        error: None,
    };
    let Some(a) = account else {
        return out;
    };
    let parsed = (|| -> Result<(), String> {
        if a.owner != *program || a.data.len() < 44 {
            return Err("C01_IDL_LAYOUT".into());
        }
        out.authority = Some(
            Pubkey::new_from_array(a.data[8..40].try_into().map_err(|_| "C01_IDL_LAYOUT")?)
                .to_string(),
        );
        let len =
            u32::from_le_bytes(a.data[40..44].try_into().map_err(|_| "C01_IDL_LAYOUT")?) as usize;
        let compressed = a.data.get(44..44 + len).ok_or("C01_IDL_LAYOUT")?;
        out.compressed_sha256 = Some(sha(compressed));
        use std::io::Read;
        let mut json = Vec::new();
        flate2::read::ZlibDecoder::new(compressed)
            .take(limit + 1)
            .read_to_end(&mut json)
            .map_err(|_| "C01_IDL_INFLATE")?;
        if json.len() as u64 > limit {
            return Err("C01_IDL_LIMIT".into());
        }
        out.sha256 = Some(sha(&json));
        // The IDL is the external Anchor format: read at this edge, its instruction names kept.
        let v: Value = serde_json::from_slice(&json).map_err(|_| "C01_IDL_FORMAT")?;
        out.instructions = v["instructions"]
            .as_array()
            .map(|i| {
                i.iter()
                    .filter_map(|x| x["name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(())
    })();
    out.error = parsed.err();
    out
}

fn read(
    plan: &C01Plan,
    plan_bytes: &[u8],
    c: &darkbot_solana::runtime_config::RuntimeConfig,
    dir: &Path,
) -> Result<C01Reads, String> {
    let program = Pubkey::from_str(&plan.program).map_err(|_| "C01_PROGRAM")?;
    let group = Pubkey::from_str(&plan.group).map_err(|_| "C01_GROUP")?;
    let mut r = Reader {
        rpc: darkbot_solana::provider::DeadlineRpc::new(
            c.rpc_endpoints[0].clone(),
            darkbot_solana::provider::Workload::Bootstrap,
        )?,
        deadline: Instant::now() + Duration::from_secs(plan.budget.seconds),
        floor: 0,
        slots: vec![],
    };
    // The Anchor IDL account: `create_with_seed(find_program_address([]), "anchor:idl")`.
    let idl_address = Pubkey::create_with_seed(
        &Pubkey::find_program_address(&[], &program).0,
        "anchor:idl",
        &program,
    )
    .map_err(|_| "C01_IDL_ADDRESS")?;
    let fee = protocol::fee_state_pda(&program);
    let mut reads = C01Reads {
        plan_id: plan.plan_id.clone(),
        plan_sha256: sha(plan_bytes),
        started_unix_ms: now_ms(),
        finished_unix_ms: None,
        context_slots: vec![],
        deployment: C01Deployment {
            program_data: None,
            decode_error: None,
            executable: false,
            deployment_slot: None,
            upgrade_authority: None,
            account_hash: None,
            elf_sha256: None,
            elf_len: None,
            pin: None,
        },
        idl: idl(idl_address, &program, None, 0),
        group_decodes: false,
        fee_state_decodes: false,
        banks: vec![],
        dependencies: vec![],
        programs: vec![],
        error: None,
        calls: 0,
        bytes: 0,
    };
    let limit = c.limits.max_response_mib * 1024 * 1024;
    let outcome = (|| -> Result<(), String> {
        let first = r.multiple(&[program, idl_address, group, fee])?;
        let decoded = |a: &Option<Fe01Account>, f: fn(&[u8]) -> bool| {
            a.as_ref()
                .is_some_and(|a| a.owner == program && !a.executable && f(&a.data))
        };
        reads.group_decodes = decoded(&first[2], |d| protocol::decode_group(d).is_ok());
        reads.fee_state_decodes = decoded(&first[3], |d| protocol::decode_fee_state(d).is_ok());
        reads.idl = idl(idl_address, &program, first[1].as_ref(), limit);
        let pd = match &first[0] {
            None => Err(protocol::DeploymentError::Owner),
            Some(a) => {
                reads.deployment.executable = a.executable;
                protocol::decode_program(&program, &a.owner, a.executable, &a.data)
            }
        };
        match pd {
            Err(e) => reads.deployment.decode_error = Some(format!("{e:?}")),
            Ok(pd) => {
                reads.deployment.program_data = Some(pd.to_string());
                let a = r
                    .multiple(&[pd])?
                    .remove(0)
                    .ok_or("C01_PROGRAM_DATA_MISSING")?;
                match protocol::decode_program_data(&pd, &a.owner, a.executable, &a.data) {
                    Err(e) => reads.deployment.decode_error = Some(format!("{e:?}")),
                    Ok(d) => {
                        reads.deployment.deployment_slot = Some(d.deployment_slot);
                        reads.deployment.upgrade_authority =
                            d.upgrade_authority.map(|k| k.to_string());
                        reads.deployment.account_hash = Some(d.hash.to_string());
                        reads.deployment.pin =
                            Some(protocol::deployment_pin(&program, &d).name().into());
                        if let Some(elf) = protocol::program_data_elf(&a.data) {
                            reads.deployment.elf_sha256 = Some(sha(elf));
                            reads.deployment.elf_len = Some(elf.len() as u64);
                        }
                    }
                }
            }
        }
        // The resize census: every bank of the group, its layout and what it observes.
        let mut observed: Vec<Pubkey> = Vec::new();
        for (key, a) in r.banks(&program, &group)? {
            let mut row = C01Bank {
                key: key.to_string(),
                len: a.data.len() as u64,
                layout: protocol::bank_layout(a.data.len()).map(String::from),
                decodes: false,
                oracle_setup: None,
                observation_keys: vec![],
                observation_error: None,
            };
            if let Ok(bank) = protocol::decode_bank(&a.data) {
                row.decodes = a.owner == program;
                row.oracle_setup = Some(bank.config.oracle_setup as u8);
                match darkbot_project0::health::crate_impl::bank_observation_keys(&bank) {
                    Ok(keys) => {
                        for k in keys.into_iter().filter(|k| *k != Pubkey::default()) {
                            row.observation_keys.push(k.to_string());
                            if !observed.contains(&k) {
                                observed.push(k);
                            }
                        }
                    }
                    Err(e) => row.observation_error = Some(e.to_string()),
                }
            }
            reads.banks.push(row);
        }
        for chunk in observed.chunks(plan.budget.batch.max(1) as usize) {
            for (k, a) in chunk.iter().zip(r.multiple(chunk)?) {
                reads.dependencies.push(C01Dependency {
                    key: k.to_string(),
                    owner: a.as_ref().map(|a| a.owner.to_string()),
                    len: a.as_ref().map_or(0, |a| a.data.len() as u64),
                    sha256: a.as_ref().map(|a| sha(&a.data)),
                });
            }
        }
        // The token programs' bytecode: a loader-v2 program holds its ELF, a loader-v3 one its
        // ProgramData.
        let keys: Vec<Pubkey> = PROGRAMS
            .iter()
            .map(|(_, k)| Pubkey::from_str(k).map_err(|_| "C01_PROGRAM"))
            .collect::<Result<_, _>>()?;
        let accounts = r.multiple(&keys)?;
        let programs = dir.join("programs");
        std::fs::create_dir_all(&programs).map_err(|e| format!("C01_STORE: {e}"))?;
        let mut upgradeable = Vec::new();
        for (((name, _), key), a) in PROGRAMS.iter().zip(&keys).zip(accounts) {
            let mut row = C01Program {
                name: (*name).into(),
                key: key.to_string(),
                owner: a.as_ref().map(|a| a.owner.to_string()),
                executable: a.as_ref().is_some_and(|a| a.executable),
                elf_sha256: None,
                elf_len: None,
                file: None,
                error: None,
            };
            match a {
                Some(a) if a.owner.to_string() == protocol::BPF_LOADER_2_ID && a.executable => {
                    store_elf(&programs, &mut row, &a.data)?;
                }
                Some(a) => match protocol::decode_program(key, &a.owner, a.executable, &a.data) {
                    Ok(pd) => upgradeable.push((reads.programs.len(), pd)),
                    Err(e) => row.error = Some(format!("{e:?}")),
                },
                None => row.error = Some("C01_PROGRAM_MISSING".into()),
            }
            reads.programs.push(row);
        }
        if !upgradeable.is_empty() {
            let pds: Vec<Pubkey> = upgradeable.iter().map(|(_, pd)| *pd).collect();
            for ((i, _), a) in upgradeable.iter().zip(r.multiple(&pds)?) {
                match a.as_ref().and_then(|a| protocol::program_data_elf(&a.data)) {
                    Some(elf) => store_elf(&programs, &mut reads.programs[*i], elf)?,
                    None => reads.programs[*i].error = Some("C01_PROGRAM_DATA_MISSING".into()),
                }
            }
        }
        Ok(())
    })();
    reads.error = outcome.err();
    reads.context_slots = r.slots.clone();
    reads.calls = r.rpc.request_count() as u64;
    reads.bytes = r.rpc.bytes();
    reads.finished_unix_ms = now_ms();
    Ok(reads)
}

fn store_elf(dir: &Path, row: &mut C01Program, elf: &[u8]) -> Result<(), String> {
    let file = format!("{}.so", row.name);
    std::fs::write(dir.join(&file), elf).map_err(|e| format!("C01_STORE: {e}"))?;
    row.elf_sha256 = Some(sha(elf));
    row.elf_len = Some(elf.len() as u64);
    row.file = Some(format!("programs/{file}"));
    Ok(())
}

/// DEC-18 §2.2–2.5: the parity gates of a run's reads and the recorded steps. PASS only when
/// every mandatory gate passes; a step not recorded is NOT_RUN.
pub fn evaluate(reads: &C01Reads, reads_sha256: &str, steps: &Steps) -> P1aParity {
    let gate = |name: &str, verdict: &str, mandatory: bool, detail: String| ParityGate {
        name: name.into(),
        verdict: verdict.into(),
        mandatory,
        detail,
    };
    let pass = |ok: bool| if ok { "PASS" } else { "FAIL" };
    let mut gates = Vec::new();
    if let Some(e) = &reads.error {
        gates.push(gate("READS", "BLOCKED", true, e.clone()));
    }
    let d = &reads.deployment;
    gates.push(gate(
        "DEPLOYMENT",
        pass(d.executable && d.deployment_slot.is_some() && d.elf_sha256.is_some()),
        true,
        format!(
            "program_data={} slot={} upgrade_authority={} elf_sha256={} elf_len={} {}",
            d.program_data.as_deref().unwrap_or("-"),
            d.deployment_slot.map_or("-".into(), |s| s.to_string()),
            d.upgrade_authority.as_deref().unwrap_or("none"),
            d.elf_sha256.as_deref().unwrap_or("-"),
            d.elf_len.map_or("-".into(), |l| l.to_string()),
            d.decode_error.as_deref().unwrap_or("")
        ),
    ));
    // The pin of the read hash against the reference of this build (the read's own verdict
    // was against the reference of its build).
    let pin = d
        .account_hash
        .as_deref()
        .and_then(|h| solana_sdk::hash::Hash::from_str(h).ok())
        .zip(Pubkey::from_str(protocol::PROGRAM_ID_MAINNET).ok())
        .map(|(h, program)| protocol::hash_pin(&program, &h).name().to_string())
        .or_else(|| d.pin.clone());
    gates.push(gate(
        "PIN",
        pass(pin.as_deref() == Some("REFERENCE")),
        false,
        format!(
            "{} (reference {} of {}; read as {}; NOT_REFERENCE is a promotion candidate)",
            pin.as_deref().unwrap_or("-"),
            protocol::PROGRAM_DATA_REFERENCE_HASH,
            protocol::PROGRAM_DATA_REFERENCE_SOURCE_TAG,
            d.pin.as_deref().unwrap_or("-")
        ),
    ));
    let build = match &steps.build {
        None => gate(
            "BUILD",
            "NOT_RUN",
            true,
            format!(
                "no reproducible build of {} ({}) recorded (p0ctl c01 build): source/binary parity of the ELF is not shown",
                protocol::SOURCE_TAG,
                protocol::SOURCE_COMMIT
            ),
        ),
        Some(b) => gate(
            "BUILD",
            pass(
                b.plan_id == reads.plan_id
                    && b.source_commit == protocol::SOURCE_COMMIT
                    && !b.source_modified
                    && d.elf_sha256.as_deref() == Some(b.elf_sha256.as_str())
                    && d.elf_len == Some(b.elf_len),
            ),
            true,
            format!(
                "built elf_sha256={} elf_len={} from {} ({}) with {}: {}; deployed elf_sha256={}",
                b.elf_sha256,
                b.elf_len,
                protocol::SOURCE_TAG,
                b.source_commit,
                b.image,
                b.command,
                d.elf_sha256.as_deref().unwrap_or("-")
            ),
        ),
    };
    // The user's decision of 02.11.2026: a build of the pinned source identical to the deployed
    // ELF proves the code; the on-chain IDL is metadata its authority uploads apart from the
    // program, informational then. Without that build it stays mandatory.
    let built = build.verdict == "PASS";
    let idl = &reads.idl;
    let mut idl_gate = match (&idl.sha256, idl.present) {
        (Some(s), _) => gate(
            "IDL",
            pass(s == protocol::SOURCE_IDL_SHA256),
            true,
            format!(
                "sha256={s} instructions={} (pinned {} release IDL {})",
                idl.instructions.len(),
                protocol::SOURCE_TAG,
                protocol::SOURCE_IDL_SHA256
            ),
        ),
        (None, true) => gate("IDL", "FAIL", true, idl.error.clone().unwrap_or_default()),
        (None, false) => gate(
            "IDL",
            "NOT_RUN",
            true,
            format!("no on-chain IDL at {}", idl.address),
        ),
    };
    if built {
        idl_gate.mandatory = false;
        idl_gate.detail += "; informational: the BUILD gate proves the deployed bytecode";
    }
    gates.push(idl_gate);
    gates.push(gate(
        "GROUP",
        pass(reads.group_decodes && reads.fee_state_decodes),
        true,
        format!(
            "group={} fee_state={}",
            reads.group_decodes, reads.fee_state_decodes
        ),
    ));
    let mut layouts = std::collections::BTreeMap::<String, u64>::new();
    for b in &reads.banks {
        *layouts
            .entry(match (&b.layout, b.decodes) {
                (_, true) => "decodes".into(),
                (Some(l), false) => format!("layout {l}"),
                (None, false) => format!("length {}", b.len),
            })
            .or_default() += 1;
    }
    gates.push(gate(
        "BANKS",
        pass(!reads.banks.is_empty() && reads.banks.iter().all(|b| b.decodes)),
        true,
        format!("{} banks: {layouts:?}", reads.banks.len()),
    ));
    let unsupported: Vec<String> = reads
        .banks
        .iter()
        .filter(|b| b.observation_error.is_some())
        .map(|b| format!("{} setup {:?}", b.key, b.oracle_setup))
        .collect();
    let missing = reads
        .dependencies
        .iter()
        .filter(|d| d.owner.is_none())
        .count();
    gates.push(gate(
        "ORACLES",
        pass(unsupported.is_empty() && missing == 0),
        true,
        format!(
            "{} observed accounts, {missing} missing, unsupported setups {unsupported:?}",
            reads.dependencies.len()
        ),
    ));
    gates.push(gate(
        "PROGRAMS",
        pass(
            reads.programs.len() == PROGRAMS.len()
                && reads.programs.iter().all(|p| p.elf_sha256.is_some()),
        ),
        false,
        reads
            .programs
            .iter()
            .map(|p| format!("{}={}", p.name, p.elf_sha256.as_deref().unwrap_or("-")))
            .collect::<Vec<_>>()
            .join(" "),
    ));
    gates.push(build);
    gates.push(match &steps.fe01 {
        None => gate(
            "FE01",
            "NOT_RUN",
            true,
            "the FE-01 observation on this deployment (p0-fe-qualify --fe-mode full, C-01 step 6; p0ctl c01 fe01)"
                .into(),
        ),
        Some(f) => {
            // The read deployment's account hash (base58) as the observation states it (hex).
            let account = d
                .account_hash
                .as_deref()
                .and_then(|h| solana_sdk::hash::Hash::from_str(h).ok())
                .map(|h| journal::hex(&h.to_bytes()));
            gate(
                "FE01",
                pass(
                    f.plan_id == reads.plan_id
                        && f.sealed_complete
                        && f.execution == super::fe_qualification::READ_ONLY
                        && f.status == super::fe_qualification::OBSERVED
                        && f.program_data.is_some()
                        && f.program_data == d.program_data
                        && f.deployment_slot == d.deployment_slot
                        && account.is_some()
                        && f.program_data_sha256 == account
                        && f.slot >= reads.context_slots.iter().max().copied(),
                ),
                true,
                format!(
                    "{} {} slot={} program_data={} sha256={} deployment_slot={} accounts={} targets={} sealed_complete={} journal={}",
                    f.status,
                    f.execution,
                    f.slot.map_or("-".into(), |s| s.to_string()),
                    f.program_data.as_deref().unwrap_or("-"),
                    f.program_data_sha256.as_deref().unwrap_or("-"),
                    f.deployment_slot.map_or("-".into(), |s| s.to_string()),
                    f.accounts,
                    f.targets,
                    f.sealed_complete,
                    f.journal_sha256
                ),
            )
        }
    });
    let overall = gates
        .iter()
        .find(|g| g.mandatory && g.verdict != "PASS")
        .map_or("PASS".to_string(), |g| {
            format!("BLOCKED: {} {}", g.name, g.verdict)
        });
    P1aParity {
        plan_id: reads.plan_id.clone(),
        reads_sha256: reads_sha256.into(),
        source_tag: protocol::SOURCE_TAG.into(),
        gates,
        overall,
    }
}

/// The operator's view of a plan.
pub fn plan_text(plan: &C01Plan, token: &str) -> String {
    let b = &plan.budget;
    let mut out = format!(
        "C01_READ_PLAN id={} token={token}\nprovider rpc={} ({}) registry v{}\nbudget calls={} mib={} seconds={} rps={} batch={}\nprogram {} group {}\n",
        plan.plan_id,
        plan.rpc.id,
        &plan.rpc.binding[..16.min(plan.rpc.binding.len())],
        plan.registry_version,
        b.calls,
        b.mib,
        b.seconds,
        b.rps,
        b.batch,
        plan.program,
        plan.group
    );
    for r in &plan.reads {
        out += &format!("read {r}\n");
    }
    out += &format!(
        "approve: p0ctl c01 run --plan {} --approve {token}   (C-01 steps 5–6: reads Mainnet through the selected provider)\n",
        plan.plan_id
    );
    out
}

/// The operator's view of a run.
pub fn run_text(reads: &C01Reads, parity: &P1aParity) -> String {
    let mut out = format!(
        "C01_READS plan={} calls={} bytes={} context_slots={:?} banks={} observed={} error={}\n",
        reads.plan_id,
        reads.calls,
        reads.bytes,
        reads.context_slots,
        reads.banks.len(),
        reads.dependencies.len(),
        reads.error.as_deref().unwrap_or("-")
    );
    for g in &parity.gates {
        out += &format!(
            "gate {} {}{} {}\n",
            g.name,
            g.verdict,
            if g.mandatory { "" } else { " (informational)" },
            g.detail
        );
    }
    out += &format!("P1A_PARITY {}\n", parity.overall);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reads() -> C01Reads {
        C01Reads {
            plan_id: "p".into(),
            plan_sha256: "s".into(),
            started_unix_ms: None,
            finished_unix_ms: None,
            context_slots: vec![1],
            deployment: C01Deployment {
                program_data: Some("pd".into()),
                decode_error: None,
                executable: true,
                deployment_slot: Some(1),
                upgrade_authority: None,
                account_hash: Some("h".into()),
                elf_sha256: Some("e".into()),
                elf_len: Some(1),
                pin: Some("NOT_REFERENCE".into()),
            },
            idl: C01Idl {
                address: "a".into(),
                present: true,
                authority: None,
                compressed_sha256: None,
                sha256: Some(protocol::SOURCE_IDL_SHA256.into()),
                instructions: vec![],
                error: None,
            },
            group_decodes: true,
            fee_state_decodes: true,
            banks: vec![C01Bank {
                key: "b".into(),
                len: 3912,
                layout: Some("0.1.12".into()),
                decodes: true,
                oracle_setup: Some(0),
                observation_keys: vec![],
                observation_error: None,
            }],
            dependencies: vec![],
            programs: vec![],
            error: None,
            calls: 1,
            bytes: 1,
        }
    }

    /// DEC-18 §2.2–2.5 (P0-025): the release IDL's bytes pass the IDL gate, a pin that is not the
    /// reference is informational; parity is PASS only with every mandatory gate — here the
    /// build (no reproducible build) blocks first, never a self-declared PASS. An unsupported
    /// oracle setup or a missing observed account fails the oracle gate.
    #[test]
    fn p0_025_parity_passes_only_with_every_mandatory_gate() {
        let p = evaluate(&reads(), "x", &Steps::default());
        let verdict = |name: &str| {
            p.gates
                .iter()
                .find(|g| g.name == name)
                .unwrap()
                .verdict
                .clone()
        };
        assert_eq!(verdict("IDL"), "PASS");
        assert_eq!(verdict("PIN"), "FAIL");
        // The pin follows the read hash against this build's reference (C-01 step 7).
        let mut promoted = reads();
        promoted.deployment.account_hash = Some(protocol::PROGRAM_DATA_REFERENCE_HASH.into());
        let p2 = evaluate(&promoted, "x", &Steps::default());
        let pin = p2.gates.iter().find(|g| g.name == "PIN").unwrap();
        assert!(
            pin.verdict == "PASS" && pin.detail.contains("read as NOT_REFERENCE"),
            "{}",
            pin.detail
        );
        assert_eq!(p.overall, "BLOCKED: BUILD NOT_RUN");
        let mut r = reads();
        r.banks[0].observation_error = Some("unsupported".into());
        assert_eq!(
            evaluate(&r, "x", &Steps::default())
                .gates
                .iter()
                .find(|g| g.name == "ORACLES")
                .unwrap()
                .verdict,
            "FAIL"
        );
        let mut r = reads();
        r.dependencies.push(C01Dependency {
            key: "k".into(),
            owner: None,
            len: 0,
            sha256: None,
        });
        assert_eq!(
            evaluate(&r, "x", &Steps::default())
                .gates
                .iter()
                .find(|g| g.name == "ORACLES")
                .unwrap()
                .verdict,
            "FAIL"
        );
        let mut r = reads();
        r.error = Some("C01_BUDGET: x".into());
        assert_eq!(
            evaluate(&r, "x", &Steps::default()).overall,
            "BLOCKED: READS BLOCKED"
        );
        // The ELF of a ProgramData account: after the metadata, padding trimmed.
        let mut pd = vec![0u8; protocol::PROGRAM_DATA_METADATA_LEN];
        pd.extend_from_slice(b"ELF\0x");
        pd.extend_from_slice(&[0; 9]);
        assert_eq!(protocol::program_data_elf(&pd), Some(&b"ELF\0x"[..]));
    }

    /// C-01 BUILD gate: a recorded build of the pinned source passes only with the deployed
    /// ELF's hash and length; the parity then waits for the FE-01 observation.
    #[test]
    fn p0_025_the_build_gate_is_the_recorded_build_against_the_deployed_elf() {
        let build = |sha: &str, len: u64| Steps {
            fe01: None,
            build: Some(C01Build {
                plan_id: "p".into(),
                source_commit: protocol::SOURCE_COMMIT.into(),
                source_modified: false,
                image: "i@sha256:0".into(),
                verifier_sha256: "v".into(),
                verifier_version: "solana-verify 0".into(),
                command: "c".into(),
                elf_sha256: sha.into(),
                elf_len: len,
                recorded_unix_ms: None,
            }),
        };
        let verdict = |steps: &Steps| {
            let p = evaluate(&reads(), "x", steps);
            let g = p.gates.iter().find(|g| g.name == "BUILD").unwrap();
            (g.verdict.clone(), p.overall)
        };
        assert_eq!(
            verdict(&build("e", 1)),
            ("PASS".into(), "BLOCKED: FE01 NOT_RUN".into())
        );
        assert_eq!(verdict(&build("other", 1)).0, "FAIL");
        assert_eq!(verdict(&build("e", 2)).0, "FAIL");
        let mut modified = build("e", 1);
        modified.build.as_mut().unwrap().source_modified = true;
        assert_eq!(verdict(&modified).0, "FAIL");
        // A stale on-chain IDL: mandatory without the build, informational with it.
        let mut stale = reads();
        stale.idl.sha256 = Some("stale".into());
        assert_eq!(
            evaluate(&stale, "x", &Steps::default()).overall,
            "BLOCKED: IDL FAIL"
        );
        let p = evaluate(&stale, "x", &build("e", 1));
        let idl = p.gates.iter().find(|g| g.name == "IDL").unwrap();
        assert_eq!((idl.verdict.as_str(), idl.mandatory), ("FAIL", false));
        assert_eq!(p.overall, "BLOCKED: FE01 NOT_RUN");
        assert_eq!(
            evaluate(&stale, "x", &build("other", 1)).overall,
            "BLOCKED: IDL FAIL"
        );
        assert_eq!(Steps::default().parity_file(), "parity.p0j");
        assert_eq!(build("e", 1).parity_file(), "parity-build.p0j");
    }

    /// C-01 step 6, FE01 gate: the sealed read-only observation of exactly the read deployment
    /// (ProgramData, deployment slot, account hash), not before the reads; with the build the
    /// parity is PASS.
    #[test]
    fn p0_025_the_fe01_gate_is_the_observation_of_the_read_deployment() {
        let hash = solana_sdk::hash::hash(b"pd");
        let mut r = reads();
        r.deployment.account_hash = Some(hash.to_string());
        let observed = C01Fe01 {
            plan_id: "p".into(),
            fe01_plan_id: "f".into(),
            group: "g".into(),
            scope_targets: vec!["t".into()],
            evidence_sha256: "e".into(),
            journal_sha256: "j".into(),
            sealed_complete: true,
            execution: crate::orchestration::fe_qualification::READ_ONLY.into(),
            status: crate::orchestration::fe_qualification::OBSERVED.into(),
            reason: None,
            slot: Some(1),
            program_data: Some("pd".into()),
            program_data_sha256: Some(journal::hex(&hash.to_bytes())),
            deployment_slot: Some(1),
            accounts: 1,
            targets: 1,
            recorded_unix_ms: None,
        };
        let built = C01Build {
            plan_id: "p".into(),
            source_commit: protocol::SOURCE_COMMIT.into(),
            source_modified: false,
            image: "i".into(),
            verifier_sha256: "v".into(),
            verifier_version: "solana-verify 0".into(),
            command: "c".into(),
            elf_sha256: "e".into(),
            elf_len: 1,
            recorded_unix_ms: None,
        };
        let verdict = |f: C01Fe01| {
            let p = evaluate(
                &r,
                "x",
                &Steps {
                    build: Some(built.clone()),
                    fe01: Some(f),
                },
            );
            let g = p.gates.iter().find(|g| g.name == "FE01").unwrap();
            (g.verdict.clone(), p.overall)
        };
        assert_eq!(verdict(observed.clone()), ("PASS".into(), "PASS".into()));
        for f in [
            C01Fe01 {
                program_data_sha256: Some("00".into()),
                ..observed.clone()
            },
            C01Fe01 {
                deployment_slot: Some(2),
                ..observed.clone()
            },
            C01Fe01 {
                sealed_complete: false,
                ..observed.clone()
            },
            C01Fe01 {
                status: crate::orchestration::fe_qualification::INCOMPLETE.into(),
                ..observed.clone()
            },
            C01Fe01 {
                slot: Some(0),
                ..observed.clone()
            },
            C01Fe01 {
                program_data: None,
                program_data_sha256: None,
                deployment_slot: None,
                slot: None,
                ..observed.clone()
            },
        ] {
            assert_eq!(verdict(f).0, "FAIL");
        }
    }
}

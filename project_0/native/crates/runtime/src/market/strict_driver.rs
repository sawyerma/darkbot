//! Ticket 3 A3: native driver of the strict verifier (PHASENPLAN L.2/L.3).
//!
//! Capture, verification and report stay separate lifecycles. The driver reads the run's typed
//! pre-capture manifest and its typed CAPTURE phase, binds the immutable capture evidence (every
//! capture-time file with size and SHA-256), derives the stable `verification_spec_id` from the
//! capture identity, window, mode, decoder binary, protocol pin, provider policy/binding and
//! budgets, records every attempt separately and points at the strict engine's content-addressed
//! result (`verification_result_id`). Resume reuses the same spec and its checkpoints; a changed
//! capture, manifest, window or decoder is a different spec. Nothing here is JSON: every record is
//! a typed single-value journal. Layout V2 runs only; historical runs keep the Python driver.
//! DEC-08: INDEX and CROSSCHECK run through the same binding, spec, attempt and lock
//! (`drive_history`).
use super::verification::{read_report, run as run_strict, Job, StrictResult};
use crate::analytics::journal::{self, hex, kind, source};
use crate::analytics::measurement::{Phase, PhaseRecord, RunManifest, NS, RUN_MANIFEST_JOURNAL};
use crate::analytics::payload::{self, Payload};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

pub const SPEC_SCHEMA: &str = "Project0/StrictVerificationSpec/1";
/// Directory prefix of native specs under `verification/` (Python specs use bare ids).
pub const NATIVE_PREFIX: &str = "native-";

/// C0E1-P4-RESUME: the audited deployment continuity of the capture window, proven after the
/// capture for one canonical result — the ProgramData read at capture start (market summary) and
/// a finalized read at or after the result's last slot carry the audited reference hash and the
/// same ProgramData (`paths::verified`). The strict slot scan does not prove it. Immutable,
/// bound to its result (`deployment-<result>.p0j` beside the spec).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentProof {
    pub verification_result_id: String,
    pub end_slot: u64,
    pub before: Option<super::paths::Deployment>,
    pub after: Option<super::paths::Deployment>,
    pub error: Option<String>,
    pub verified: bool,
}
impl Payload for DeploymentProof {
    const KIND: u16 = kind::DEPLOYMENT_PROOF;
}

/// A stored proof re-checked on read (C0E1-P4-VC): bound to `result_id`, its slot bound equal to
/// the result's last scoped slot, its before-read equal to the capture's own, and continuity judged
/// anew from the stored ProgramData, hashes and slots — a stored `verified` alone proves nothing.
pub fn proof_holds(
    proof: &DeploymentProof,
    result_id: &str,
    scope_end: Option<u64>,
    capture_before: Option<&super::paths::Deployment>,
) -> bool {
    proof.verification_result_id == result_id
        && scope_end == Some(proof.end_slot)
        && capture_before.is_some()
        && proof.before.as_ref() == capture_before
        && super::paths::verified(
            proof.before.as_ref(),
            proof.after.as_ref(),
            Some(proof.end_slot),
        )
}

/// Where the deployment proof of `result_id` under spec `spec_id` lives.
pub fn deployment_proof_path(run: &Path, spec_id: &str, result_id: &str) -> PathBuf {
    run.join("verification")
        .join(format!("{NATIVE_PREFIX}{spec_id}"))
        .join(format!("deployment-{result_id}.p0j"))
}

/// Reads the deployment after the window (`url`, finalized, context slot ≥ `end_slot`) and
/// records the proof of `result_id` (spec `spec_id`) next to its spec; never reads without an
/// RPC (`DEPLOYMENT_RPC_MISSING`).
pub fn prove_deployment(
    run: &Path,
    spec_id: &str,
    result_id: &str,
    end_slot: u64,
    url: Option<&str>,
) -> Result<DeploymentProof, String> {
    let summary: super::MarketFinal = journal::read_typed_value(
        &run.join("capture/market")
            .join(super::MARKET_SUMMARY_JOURNAL),
    )
    .map_err(|_| "DEPLOYMENT_BEFORE_UNREADABLE")?;
    let before = summary.summary.deployment_continuity.before;
    use std::str::FromStr;
    let program =
        solana_sdk::pubkey::Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET)
            .map_err(|e| e.to_string())?;
    let (after, mut error) = match super::paths::inspect(url, &program) {
        Ok(d) => (Some(d), None),
        Err(e) => (None, Some(e.to_string())),
    };
    let verified = super::paths::verified(before.as_ref(), after.as_ref(), Some(end_slot));
    if before.is_none() && error.is_none() {
        error = Some("DEPLOYMENT_BEFORE_MISSING".into());
    }
    let proof = DeploymentProof {
        verification_result_id: result_id.into(),
        end_slot,
        before,
        after,
        error,
        verified,
    };
    let path = deployment_proof_path(run, spec_id, result_id);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    journal::write_typed_value(&path, source::STRICT, &proof)
        .map_err(|e| format!("DEPLOYMENT_PROOF: {e}"))?;
    Ok(proof)
}

/// Verifier budget of one spec (part of its identity).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrictBudget {
    pub rps: u64,
    pub max_calls: u64,
    pub max_seconds: u64,
    pub max_bytes: u64,
    pub max_disk_bytes: u64,
    pub max_ram_bytes: u64,
    pub min_free_bytes: u64,
}

impl StrictBudget {
    /// DEC-08: the typed FREE_ONLY budget of a verification method — requests, bytes and RPS
    /// per method (`verify_*` strict, `index_*`, `crosscheck_*` per source), time, disk, RAM and
    /// free space the verifiers' shared bounds.
    pub fn of_mode(mode: VerifyMode, l: &darkbot_solana::runtime_config::Limits) -> Self {
        let (rps, calls, mib) = match mode {
            VerifyMode::Off | VerifyMode::Strict => (l.verify_rps, l.verify_calls, l.verify_mib),
            VerifyMode::Index { .. } => (l.index_rps, l.index_calls, l.index_mib),
            VerifyMode::Crosscheck { .. } => {
                (l.crosscheck_rps, l.crosscheck_calls, l.crosscheck_mib)
            }
        };
        let mib_bytes = 1024 * 1024;
        StrictBudget {
            rps,
            max_calls: calls,
            max_seconds: l.verify_seconds,
            max_bytes: mib * mib_bytes,
            max_disk_bytes: l.verify_disk_mib * mib_bytes,
            max_ram_bytes: l.verify_ram_mib * mib_bytes,
            min_free_bytes: l.verify_free_mib * mib_bytes,
        }
    }
}

/// Stable identity of one strict verification definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationSpec {
    pub schema: String,
    pub capture_run_id: String,
    pub manifest_sha256: [u8; 32],
    pub window_start_unix_ns: u64,
    pub window_end_unix_ns: u64,
    pub mode: String,
    pub program: String,
    /// The verifier binary is the decoder pin (any decoder change is a new spec).
    pub decoder_binary_sha256: [u8; 32],
    pub protocol_pin: String,
    pub journal_version: u16,
    pub provider_policy: String,
    /// SHA-256 of the RPC endpoint (never the endpoint); `None` offline.
    pub provider_binding_sha256: Option<[u8; 32]>,
    pub budget: StrictBudget,
}
impl Payload for VerificationSpec {
    const KIND: u16 = kind::STRICT_SPEC;
}
impl VerificationSpec {
    pub fn id(&self) -> Result<String, String> {
        payload::encode(self)
            .map(|b| hex(&solana_sdk::hash::hash(&b).to_bytes()))
            .map_err(|e| e.to_string())
    }
}

/// Immutable capture evidence: every capture-time file of the run (verification, reports and
/// qualification excluded) with its size and SHA-256, bound to the capture identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureEvidence {
    pub capture_run_id: String,
    pub files: BTreeMap<String, (u64, [u8; 32])>,
}
impl Payload for CaptureEvidence {
    const KIND: u16 = kind::CAPTURE_EVIDENCE;
}
impl CaptureEvidence {
    pub fn digest(&self) -> Result<String, String> {
        payload::encode(self)
            .map(|b| hex(&solana_sdk::hash::hash(&b).to_bytes()))
            .map_err(|e| e.to_string())
    }
}

/// One attempt of a spec: start and end are separate immutable records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptRecord {
    pub verification_spec_id: String,
    pub verification_attempt_id: String,
    pub at_unix_ns: u64,
    pub offline: bool,
    pub resume: bool,
    /// Set on the end record.
    pub verification_result_id: Option<String>,
    pub status: Option<String>,
    pub reason: Option<String>,
    pub capture_evidence_digest: String,
}
impl Payload for AttemptRecord {
    const KIND: u16 = kind::STRICT_ATTEMPT;
}

pub struct DriverRequest {
    pub run: PathBuf,
    pub budget: StrictBudget,
    pub offline: bool,
    pub resume: bool,
    pub mode: VerifyMode,
    /// Typed measured FREE_ONLY provider evidence (`PROVIDER_EVIDENCE`): required for an online
    /// strict verification (PHASENPLAN: strict only after an explicit provider preflight).
    pub provider_evidence: Option<PathBuf>,
    /// The job controller's cancellation (`job cancel`): the attempt ends resumable.
    pub cancelled: Option<fn() -> bool>,
}

/// The provider gate of an online strict verification: measured, recent FREE_ONLY evidence for
/// the endpoint (`validate_provider_evidence`) that also qualifies archive reads.
pub fn strict_provider_gate(
    config: &darkbot_solana::runtime_config::RuntimeConfig,
    url: &str,
    evidence: &Path,
    now_unix_s: f64,
) -> Result<(), String> {
    let proof: crate::orchestration::provider_evidence::ProviderEvidence =
        journal::read_typed_value(evidence).map_err(|_| "STRICT_PROVIDER_EVIDENCE_INVALID")?;
    crate::orchestration::provider_evidence::validate_provider_evidence(
        config, url, &proof, now_unix_s,
    )?;
    if !proof.archive_qualified {
        return Err("STRICT_ARCHIVE_UNQUALIFIED".into());
    }
    Ok(())
}

/// Post-capture verification mode of the native driver (PHASENPLAN L.2, DEC-08).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyMode {
    /// No historical chain RPC: spec and attempt identities, status `NOT_RUN`, no result.
    Off,
    Strict,
    /// DEC-08: the finalized signature index of the window (`index::run`), at most `max_pages`
    /// new pages per attempt (a per-attempt bound recorded in its result, not in the spec:
    /// a resume continues the same index).
    Index {
        max_pages: u64,
    },
    /// DEC-08: the same index from two independently qualified FREE_ONLY sources and their
    /// agreement (`index::crosscheck`); without a second source NOT_RUN (C-02).
    Crosscheck {
        max_pages: u64,
    },
}
impl VerifyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            VerifyMode::Off => "off",
            VerifyMode::Strict => "strict",
            VerifyMode::Index { .. } => "index",
            VerifyMode::Crosscheck { .. } => "crosscheck",
        }
    }
    /// The status a successful attempt of this mode reaches (none for `off`).
    pub fn verified_status(self) -> Option<&'static str> {
        match self {
            VerifyMode::Off => None,
            VerifyMode::Strict => Some("CANONICAL_VERIFIED"),
            VerifyMode::Index { .. } => Some("INDEX_VERIFIED"),
            VerifyMode::Crosscheck { .. } => Some("CROSSCHECK_VERIFIED"),
        }
    }
}

#[derive(Debug)]
pub struct DriverOutcome {
    pub verification_spec_id: String,
    pub verification_attempt_id: String,
    pub verification_result_id: Option<String>,
    pub status: String,
    pub reason: Option<String>,
    pub terminal: String,
}

/// D08: attempt and gate times never fall back to 0 (`CLOCK_UNAVAILABLE`).
fn now_unix_ns() -> Result<u64, String> {
    darkbot_common::timing::wall_now()
        .map(|d| d.as_nanos() as u64)
        .map_err(String::from)
}

fn sha_file(path: &Path) -> Result<[u8; 32], String> {
    journal::file_sha256(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The run's capture-time files: everything except the post-capture layers (`verification/`,
/// `reports/`, `qualification/`), dotfiles and the legacy views the explicit offline export may
/// add later (derived from the journals, JFI-03).
fn capture_files(run: &Path) -> Result<BTreeMap<String, (u64, [u8; 32])>, String> {
    let derived: BTreeSet<PathBuf> = crate::analytics::renders::detect(run)
        .and_then(|paths| crate::analytics::renders::views(&paths))
        .map(|views| views.into_iter().map(|v| v.output).collect())
        .unwrap_or_default();
    let mut out = BTreeMap::new();
    let mut stack = vec![run.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let p = e.path();
            let rel = p
                .strip_prefix(run)
                .map_err(|e| e.to_string())?
                .to_path_buf();
            let first = rel
                .components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().into_owned());
            if e.file_name().to_string_lossy().starts_with('.')
                || matches!(
                    first.as_deref(),
                    Some("verification")
                        | Some("reports")
                        | Some(crate::analytics::qualification::QUALIFICATION_DIR)
                )
                || derived.contains(&p)
            {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.is_file() {
                let before = p.metadata().map_err(|e| e.to_string())?;
                let sha = sha_file(&p)?;
                let after = p.metadata().map_err(|e| e.to_string())?;
                if (before.len(), before.modified().ok()) != (after.len(), after.modified().ok()) {
                    return Err("CAPTURE_STILL_MUTATING".into());
                }
                out.insert(rel.to_string_lossy().into_owned(), (before.len(), sha));
            }
        }
    }
    Ok(out)
}

/// The run's single CAPTURE window from its typed phase journal (unix ns, half-open).
/// A window as `(start, end)` nanoseconds.
type Span = (u64, u64);

/// The unique CAPTURE window: unix ns (half-open) and the monotone process-clock start/cutoff.
fn capture_window(run: &Path, manifest: &RunManifest) -> Result<(Span, Span), String> {
    let path = run.join("capture/measurement_phases.p0j");
    let mut r = crate::storage::segments::ChainReader::open(&path)
        .map_err(|e| format!("PHASE_JOURNAL: {e}"))?;
    let mut windows = Vec::new();
    while let Some(f) = r.next_frame().map_err(|e| format!("PHASE_JOURNAL: {e}"))? {
        if f.kind != kind::MEASUREMENT_PHASE {
            continue;
        }
        let rec: PhaseRecord = f.typed().map_err(|_| "PHASE_JOURNAL_NOT_TYPED")?;
        if let Phase::Capture(w) = rec.phase {
            windows.push(w);
        }
    }
    let [w] = windows.as_slice() else {
        return Err("CAPTURE_WINDOW_NOT_UNIQUE".into());
    };
    let start = u64::try_from(w.capture_start_unix_ns).map_err(|_| "INVALID_WINDOW")?;
    let end = u64::try_from(w.capture_cutoff_unix_ns).map_err(|_| "INVALID_WINDOW")?;
    if end.checked_sub(start) != Some(manifest.plan.duration_seconds * NS) {
        return Err("PLAN_WINDOW_MISMATCH".into());
    }
    Ok(((start, end), (w.capture_start_ns, w.capture_cutoff_ns)))
}

/// Exclusive, non-blocking writer lock of one spec directory (held while the guard lives).
fn lock(dir: &Path) -> Result<File, String> {
    let f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("writer.lock"))
        .map_err(|e| e.to_string())?;
    // SAFETY: flock on an owned descriptor.
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("VERIFIER_ALREADY_RUNNING".into());
    }
    Ok(f)
}

/// A V2 run's capture identity as every post-capture consumer (strict driver, native report)
/// binds it: typed manifest, the unique CAPTURE window and the immutable capture evidence.
#[derive(Debug)]
pub struct CaptureBinding {
    pub run: PathBuf,
    pub manifest: RunManifest,
    pub manifest_path: PathBuf,
    /// Unix ns, half-open.
    pub window: (u64, u64),
    /// A3: monotone process-clock `[capture_start_ns, capture_cutoff_ns]` (evaluation window).
    pub monotonic_window: (u64, u64),
    pub evidence: CaptureEvidence,
    pub evidence_digest: String,
}

/// Two-stage capture finalize: the capture evidence is fixed by the first consumer and every
/// later one re-checks it (`CAPTURE_EVIDENCE_CHANGED`).
pub fn bind_capture(run: &Path) -> Result<CaptureBinding, String> {
    let run = run.canonicalize().map_err(|e| format!("RUN: {e}"))?;
    let manifest_path = run.join("capture").join(RUN_MANIFEST_JOURNAL);
    let manifest: RunManifest = journal::read_typed_value(&manifest_path)
        .map_err(|_| "NATIVE_MANIFEST_MISSING: the run has no native capture records (a historical run is read with project_0/scripts/verify.sh)")?;
    if manifest.layout.as_deref() != Some(crate::run_layout::RunLayout::V2.tag()) {
        return Err("NATIVE_DRIVER_REQUIRES_LAYOUT_V2".into());
    }
    let (window, monotonic_window) = capture_window(&run, &manifest)?;
    let native = run.join("verification/native");
    std::fs::create_dir_all(&native).map_err(|e| e.to_string())?;
    let evidence = CaptureEvidence {
        capture_run_id: manifest.capture_run_id.clone(),
        files: capture_files(&run)?,
    };
    let evidence_path = native.join("capture_evidence.p0j");
    if evidence_path.exists() {
        let kept: CaptureEvidence =
            journal::read_typed_value(&evidence_path).map_err(|_| "CAPTURE_EVIDENCE_INVALID")?;
        if kept != evidence {
            return Err("CAPTURE_EVIDENCE_CHANGED".into());
        }
    } else {
        journal::write_typed_value(&evidence_path, source::STRICT, &evidence)
            .map_err(|e| format!("CAPTURE_EVIDENCE: {e}"))?;
    }
    let evidence_digest = evidence.digest()?;
    Ok(CaptureBinding {
        run,
        manifest,
        manifest_path,
        window,
        monotonic_window,
        evidence,
        evidence_digest,
    })
}

impl CaptureBinding {
    /// The capture must not have changed while it was consumed.
    pub fn recheck(&self, code: &str) -> Result<(), String> {
        if capture_files(&self.run)? != self.evidence.files {
            return Err(code.into());
        }
        Ok(())
    }
}

/// Runs (or resumes) one strict verification of a V2 run and records spec, attempt and result.
pub fn drive(req: &DriverRequest) -> Result<DriverOutcome, String> {
    let CaptureBinding {
        run,
        manifest,
        manifest_path,
        window: (start, end),
        evidence,
        evidence_digest,
        ..
    } = bind_capture(&req.run)?;
    let binary = std::env::current_exe().map_err(|e| e.to_string())?;
    // DEC-08: an online CROSSCHECK's second source (or why it cannot run).
    let mut second: Result<Option<String>, &'static str> = Ok(None);
    let provider = if req.offline || req.mode == VerifyMode::Off {
        None
    } else {
        // Online strict only after the provider preflight, before any chain RPC.
        let evidence = req
            .provider_evidence
            .as_deref()
            .ok_or("STRICT_PROVIDER_EVIDENCE_REQUIRED")?;
        let config = darkbot_solana::runtime_config::active();
        let url = config
            .rpc_url(&|k| std::env::var(k).ok())
            .ok_or("RPC_URL_MISSING")?;
        let now = now_unix_ns()? as f64 / 1e9;
        strict_provider_gate(config, &url, evidence, now)?;
        if let VerifyMode::Index { .. } | VerifyMode::Crosscheck { .. } = req.mode {
            method_capacity(evidence, &url, &req.budget)?;
        }
        if let VerifyMode::Crosscheck { .. } = req.mode {
            second = crosscheck_source(config, &url, evidence, now)?.map(Some);
            if let Ok(Some(s)) = &second {
                method_capacity(evidence, s, &req.budget)?;
            }
        }
        Some(provider_binding(
            &url,
            second.as_ref().ok().and_then(|s| s.as_deref()),
        ))
    };
    let spec = VerificationSpec {
        schema: SPEC_SCHEMA.into(),
        capture_run_id: manifest.capture_run_id.clone(),
        manifest_sha256: sha_file(&manifest_path)?,
        window_start_unix_ns: start,
        window_end_unix_ns: end,
        mode: req.mode.as_str().into(),
        program: darkbot_project0::protocol::PROGRAM_ID_MAINNET.into(),
        decoder_binary_sha256: sha_file(&binary)?,
        protocol_pin: manifest.protocol_pin.clone(),
        journal_version: journal::VERSION,
        provider_policy: "FREE_ONLY".into(),
        provider_binding_sha256: provider,
        budget: req.budget.clone(),
    };
    // An offline history read continues the operator's online checkpoint: the newest spec of
    // this definition that bound a provider (an offline spec of its own would hold an empty
    // checkpoint; the offline attempt reads the cache only, its time/RAM bounds its own).
    let spec = match (req.offline, req.mode) {
        (true, VerifyMode::Index { .. } | VerifyMode::Crosscheck { .. } | VerifyMode::Strict) => {
            online_spec(&run, &spec).unwrap_or(spec)
        }
        _ => spec,
    };
    let spec_id = spec.id()?;
    let out = run
        .join("verification")
        .join(format!("{NATIVE_PREFIX}{spec_id}"));
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let _lock = lock(&out)?;
    let spec_path = out.join("spec.p0j");
    if spec_path.exists() {
        let kept: VerificationSpec =
            journal::read_typed_value(&spec_path).map_err(|_| "SPEC_INVALID")?;
        if kept != spec {
            return Err("RESUME_IDENTITY_MISMATCH".into());
        }
        if !req.resume {
            return Err("SPEC_EXISTS_USE_RESUME_VERIFICATION".into());
        }
    } else {
        journal::write_typed_value(&spec_path, source::STRICT, &spec)
            .map_err(|e| format!("SPEC: {e}"))?;
    }
    let mut id = [0u8; 16];
    std::io::Read::read_exact(
        &mut File::open("/dev/urandom").map_err(|e| e.to_string())?,
        &mut id,
    )
    .map_err(|e| e.to_string())?;
    let attempt_id = hex(&id);
    let attempt_dir = out.join("attempts").join(&attempt_id);
    std::fs::create_dir_all(&attempt_dir).map_err(|e| e.to_string())?;
    let mut record = AttemptRecord {
        verification_spec_id: spec_id.clone(),
        verification_attempt_id: attempt_id.clone(),
        at_unix_ns: now_unix_ns()?,
        offline: req.offline,
        resume: req.resume,
        verification_result_id: None,
        status: None,
        reason: None,
        capture_evidence_digest: evidence_digest.clone(),
    };
    journal::write_typed_value(&attempt_dir.join("start.p0j"), source::STRICT, &record)
        .map_err(|e| format!("ATTEMPT: {e}"))?;
    if req.mode == VerifyMode::Off {
        // Off: the evidence stays bound and identified, no chain proof is attempted.
        record.at_unix_ns = now_unix_ns()?;
        record.status = Some("NOT_RUN".into());
        record.reason = Some("VERIFICATION_OFF".into());
        journal::write_typed_value(&attempt_dir.join("end.p0j"), source::STRICT, &record)
            .map_err(|e| format!("ATTEMPT_END: {e}"))?;
        if capture_files(&run)? != evidence.files {
            return Err("CAPTURE_EVIDENCE_CHANGED_DURING_VERIFICATION".into());
        }
        return Ok(DriverOutcome {
            verification_spec_id: spec_id,
            verification_attempt_id: attempt_id,
            verification_result_id: None,
            status: "NOT_RUN".into(),
            reason: record.reason,
            terminal: "verification off: no historical chain RPC, no chain proof\n".into(),
        });
    }
    if let VerifyMode::Index { .. } | VerifyMode::Crosscheck { .. } = req.mode {
        if let Err(block) = second {
            // C-02: without a second independently qualified source the crosscheck is NOT_RUN.
            record.at_unix_ns = now_unix_ns()?;
            record.status = Some("NOT_RUN".into());
            record.reason = Some(block.into());
            journal::write_typed_value(&attempt_dir.join("end.p0j"), source::STRICT, &record)
                .map_err(|e| format!("ATTEMPT_END: {e}"))?;
            if capture_files(&run)? != evidence.files {
                return Err("CAPTURE_EVIDENCE_CHANGED_DURING_VERIFICATION".into());
            }
            return Ok(DriverOutcome {
                verification_spec_id: spec_id,
                verification_attempt_id: attempt_id,
                verification_result_id: None,
                status: "NOT_RUN".into(),
                terminal: format!("crosscheck not run: {block}; no chain read\n"),
                reason: record.reason,
            });
        }
        let base = Job {
            verification_spec_id: spec_id.clone(),
            output: out.clone(),
            start_ns: start,
            end_ns: end,
            program: spec.program.clone(),
            max_calls: req.budget.max_calls,
            max_seconds: req.budget.max_seconds,
            max_bytes: req.budget.max_bytes,
            max_disk_bytes: req.budget.max_disk_bytes,
            max_ram_bytes: req.budget.max_ram_bytes,
            min_free_bytes: req.budget.min_free_bytes,
            rps: req.budget.rps,
            offline: req.offline,
            cancelled: req.cancelled,
        };
        return drive_history(
            req,
            &run,
            &evidence,
            base,
            second.ok().flatten(),
            record,
            &attempt_dir,
        );
    }
    let job = Job {
        verification_spec_id: spec_id.clone(),
        output: out.join("strict"),
        start_ns: start,
        end_ns: end,
        program: spec.program.clone(),
        max_calls: req.budget.max_calls,
        max_seconds: req.budget.max_seconds,
        max_bytes: req.budget.max_bytes,
        max_disk_bytes: req.budget.max_disk_bytes,
        max_ram_bytes: req.budget.max_ram_bytes,
        min_free_bytes: req.budget.min_free_bytes,
        rps: req.budget.rps,
        offline: req.offline,
        cancelled: req.cancelled,
    };
    let outcome: Result<StrictResult, String> = run_strict(job);
    record.at_unix_ns = now_unix_ns()?;
    match &outcome {
        Ok(result) => {
            let bytes = payload::encode(result).map_err(|e| e.to_string())?;
            record.verification_result_id = Some(hex(&solana_sdk::hash::hash(&bytes).to_bytes()));
            record.status = Some(result.status.as_str().into());
            record.reason = result.reason.clone();
        }
        Err(e) => {
            record.status = Some("HISTORICAL_INCOMPLETE".into());
            record.reason = Some(e.clone());
        }
    }
    journal::write_typed_value(&attempt_dir.join("end.p0j"), source::STRICT, &record)
        .map_err(|e| format!("ATTEMPT_END: {e}"))?;
    // A canonical result online also proves the deployment continuity of its window.
    if let (Ok(result), Some(rid), false) = (&outcome, &record.verification_result_id, req.offline)
    {
        if record.status.as_deref() == Some("CANONICAL_VERIFIED") {
            if let Some(end_slot) = result.scope.as_ref().map(|s| s.hi) {
                let url =
                    darkbot_solana::runtime_config::active().rpc_url(&|k| std::env::var(k).ok());
                prove_deployment(&run, &spec_id, rid, end_slot, url.as_deref())?;
            }
        }
    }
    // The capture must not have changed while it was verified.
    if capture_files(&run)? != evidence.files {
        return Err("CAPTURE_EVIDENCE_CHANGED_DURING_VERIFICATION".into());
    }
    Ok(DriverOutcome {
        verification_spec_id: spec_id,
        verification_attempt_id: attempt_id,
        verification_result_id: record.verification_result_id,
        status: record.status.unwrap_or_default(),
        reason: record.reason,
        terminal: read_report(&out.join("strict")).terminal(),
    })
}

/// The spec's provider binding: SHA-256 of the endpoint, and of both a CROSSCHECK reads (never
/// the endpoint itself).
fn provider_binding(primary: &str, second: Option<&str>) -> [u8; 32] {
    let mut bound = primary.to_string();
    if let Some(second) = second {
        bound.push('\n');
        bound.push_str(second);
    }
    solana_sdk::hash::hash(bound.as_bytes()).to_bytes()
}

/// DEC-08: the newest spec of `run` that equals `offline` but for its provider binding and
/// budget and bound a provider (the operator's online verification of the same capture,
/// window, mode, program, decoder binary and pin); `None` when there is none.
fn online_spec(run: &Path, offline: &VerificationSpec) -> Option<VerificationSpec> {
    let same = |s: &VerificationSpec| {
        s.provider_binding_sha256.is_some()
            && s.schema == offline.schema
            && s.capture_run_id == offline.capture_run_id
            && s.manifest_sha256 == offline.manifest_sha256
            && s.window_start_unix_ns == offline.window_start_unix_ns
            && s.window_end_unix_ns == offline.window_end_unix_ns
            && s.mode == offline.mode
            && s.program == offline.program
            && s.decoder_binary_sha256 == offline.decoder_binary_sha256
            && s.protocol_pin == offline.protocol_pin
            && s.journal_version == offline.journal_version
            && s.provider_policy == offline.provider_policy
    };
    let newest_attempt = |dir: &Path| {
        std::fs::read_dir(dir.join("attempts"))
            .map(|d| {
                d.flatten()
                    .filter_map(|a| {
                        journal::read_typed_value::<AttemptRecord>(&a.path().join("start.p0j")).ok()
                    })
                    .map(|a| a.at_unix_ns)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    };
    std::fs::read_dir(run.join("verification"))
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let id = name.strip_prefix(NATIVE_PREFIX)?.to_string();
            let spec: VerificationSpec =
                journal::read_typed_value(&e.path().join("spec.p0j")).ok()?;
            (same(&spec) && spec.id().ok()? == id).then(|| (newest_attempt(&e.path()), spec))
        })
        .max_by_key(|(at, _)| *at)
        .map(|(_, spec)| spec)
}

/// DEC-08: a history read plans no more than its provider proved — the endpoint's measured
/// FREE_ONLY proof covers the method's typed RPS and requests.
fn method_capacity(evidence: &Path, url: &str, budget: &StrictBudget) -> Result<(), String> {
    let proof: crate::orchestration::provider_evidence::ProviderEvidence =
        journal::read_typed_value(evidence).map_err(|_| "STRICT_PROVIDER_EVIDENCE_INVALID")?;
    let binding = hex(&solana_sdk::hash::hash(url.as_bytes()).to_bytes());
    let p = proof
        .providers
        .iter()
        .find(|p| p.provider_binding == binding)
        .ok_or("FE_PROVIDER_EVIDENCE_MISSING")?;
    if p.allowed_rps < budget.rps as f64 || p.available_calls < budget.max_calls {
        return Err(format!(
            "PROVIDER_CAPACITY_BELOW_METHOD_BUDGET: proved {} rps / {} requests, the method's budget {} rps / {} requests",
            p.allowed_rps, p.available_calls, budget.rps, budget.max_calls
        ));
    }
    Ok(())
}

/// DEC-08: the second source of an online CROSSCHECK — the second configured RPC endpoint, on
/// another host, with its own measured FREE_ONLY archive proof (`strict_provider_gate`).
/// Missing or on the primary's host: `Err` (the crosscheck is NOT_RUN, C-02); present but
/// unqualified: refused before any chain RPC.
fn crosscheck_source(
    config: &darkbot_solana::runtime_config::RuntimeConfig,
    primary: &str,
    evidence: &Path,
    now_unix_s: f64,
) -> Result<Result<String, &'static str>, String> {
    let Some(second) = config
        .rpc_endpoints
        .get(1)
        .filter(|s| s.as_str() != primary)
    else {
        return Ok(Err("CROSSCHECK_SECOND_PROVIDER_MISSING"));
    };
    let host = |u: &str| {
        reqwest::Url::parse(u)
            .ok()
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
    };
    if host(second).is_none() || host(second) == host(primary) {
        return Ok(Err("CROSSCHECK_PROVIDER_NOT_INDEPENDENT"));
    }
    strict_provider_gate(config, second, evidence, now_unix_s)?;
    Ok(Ok(second.clone()))
}

/// DEC-08: one INDEX or CROSSCHECK attempt of a bound capture — the capture side from the run's
/// typed records, each index through the verifier's budgeted chain client (its cache = its
/// checkpoint) under `index/` or `crosscheck/{primary,secondary}/`, the result content-addressed;
/// the attempt end binds it like a strict result.
fn drive_history(
    req: &DriverRequest,
    run: &Path,
    evidence: &CaptureEvidence,
    base: Job,
    second: Option<String>,
    mut record: AttemptRecord,
    attempt_dir: &Path,
) -> Result<DriverOutcome, String> {
    use super::index;
    use std::str::FromStr;
    let url = if req.offline {
        None
    } else {
        Some(
            darkbot_solana::runtime_config::active()
                .rpc_url(&|k| std::env::var(k).ok())
                .ok_or("RPC_URL_MISSING")?,
        )
    };
    let job = |dir: &str| Job {
        output: base.output.join(dir),
        ..base.clone()
    };
    let program =
        solana_sdk::pubkey::Pubkey::from_str(&base.program).map_err(|_| "PROGRAM_INVALID")?;
    let bot = index::BotEvidence::from_run(run, &program);
    let encode = |bytes: std::io::Result<Vec<u8>>| bytes.map_err(|e| e.to_string());
    // (encoded result, status, reason, terminal)
    let outcome: Result<(Vec<u8>, String, Option<String>, String), String> = match req.mode {
        VerifyMode::Index { max_pages } => bot
            .and_then(|bot| index::run(&job("index"), max_pages, url, &bot))
            .and_then(|r| {
                Ok((
                    encode(payload::encode(&r))?,
                    r.status.clone(),
                    r.reason.clone(),
                    r.terminal(),
                ))
            }),
        VerifyMode::Crosscheck { max_pages } => {
            let (a, b) = (job("crosscheck/primary"), job("crosscheck/secondary"));
            bot.and_then(|bot| {
                index::crosscheck(
                    [&a, &b],
                    max_pages,
                    [url, second],
                    &bot,
                    &base.output.join("crosscheck"),
                )
            })
            .and_then(|r| {
                Ok((
                    encode(payload::encode(&r))?,
                    r.status.clone(),
                    r.reason.clone(),
                    r.terminal(),
                ))
            })
        }
        VerifyMode::Off | VerifyMode::Strict => Err("VERIFY_MODE_NOT_A_HISTORY_READ".into()),
    };
    record.at_unix_ns = now_unix_ns()?;
    match &outcome {
        Ok((bytes, status, reason, _)) => {
            record.verification_result_id = Some(hex(&solana_sdk::hash::hash(bytes).to_bytes()));
            record.status = Some(status.clone());
            record.reason = reason.clone();
        }
        Err(e) => {
            record.status = Some("HISTORICAL_INCOMPLETE".into());
            record.reason = Some(e.clone());
        }
    }
    journal::write_typed_value(&attempt_dir.join("end.p0j"), source::STRICT, &record)
        .map_err(|e| format!("ATTEMPT_END: {e}"))?;
    if capture_files(run)? != evidence.files {
        return Err("CAPTURE_EVIDENCE_CHANGED_DURING_VERIFICATION".into());
    }
    Ok(DriverOutcome {
        verification_spec_id: record.verification_spec_id,
        verification_attempt_id: record.verification_attempt_id,
        verification_result_id: record.verification_result_id,
        status: record.status.unwrap_or_default(),
        reason: record.reason,
        terminal: match outcome {
            Ok((_, _, _, terminal)) => terminal,
            Err(e) => format!("PROJECT 0 {} VERIFICATION: {e}\n", req.mode.as_str()),
        },
    })
}

/// A3/C0E1-P5-CLI: the one typed request of a native verification (`p0-strict-verify --run`,
/// `p0ctl verify`): budgets default to the mode's typed budget of the operational config
/// (DEC-08: per method); every online history read needs `--provider-evidence`.
pub fn cli(args: &[String]) -> Result<(String, bool), String> {
    cli_with(args, None)
}

/// `cli` under the job controller's cancellation (`job cancel` of a post-capture job).
pub fn cli_with(
    args: &[String],
    cancelled: Option<fn() -> bool>,
) -> Result<(String, bool), String> {
    if std::env::var_os("P0_OPERATIONAL_CONFIG_FILE").is_some()
        || std::env::var_os("P0_PROFILE").is_some()
    {
        let resolved =
            darkbot_solana::runtime_config::Resolved::parse(&[], &|k| std::env::var(k).ok())?;
        for (k, v) in resolved.config.environment() {
            std::env::set_var(k, v);
        }
        resolved.activate()?;
    }
    let l = &darkbot_solana::runtime_config::active().limits;
    let (mut run, mut offline, mut resume, mut mode) = (None, false, false, VerifyMode::Strict);
    let mut pages = l.verify_pages;
    let mut provider_evidence = None;
    // Explicit bounds override the mode's typed budget (`StrictBudget::of_mode`).
    let mut set: Vec<(&'static str, u64)> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut number = || -> Result<u64, String> {
            it.next()
                .and_then(|v| v.parse().ok())
                .filter(|n| *n > 0)
                .ok_or(format!("{a} needs a positive integer"))
        };
        match a.as_str() {
            "--run" => run = Some(it.next().ok_or("--run needs a directory")?.clone()),
            "--offline" => offline = true,
            "--provider-evidence" => {
                provider_evidence = Some(PathBuf::from(
                    it.next().ok_or("--provider-evidence needs a file")?,
                ))
            }
            "--mode" => {
                mode = match it.next().map(String::as_str) {
                    Some("strict") => VerifyMode::Strict,
                    Some("off") => VerifyMode::Off,
                    Some("index") => VerifyMode::Index { max_pages: pages },
                    Some("crosscheck") => VerifyMode::Crosscheck { max_pages: pages },
                    _ => return Err("--mode strict|index|crosscheck|off".into()),
                }
            }
            "--verify-max-pages" => {
                pages = number()?;
                if let VerifyMode::Index { max_pages } | VerifyMode::Crosscheck { max_pages } =
                    &mut mode
                {
                    *max_pages = pages;
                }
            }
            "--resume-verification" => resume = true,
            flag @ ("--verify-rps"
            | "--verify-calls"
            | "--verify-seconds"
            | "--verify-bytes"
            | "--verify-disk-bytes"
            | "--verify-ram-bytes"
            | "--verify-min-free-bytes") => {
                let n = number()?;
                set.push((
                    match flag {
                        "--verify-rps" => "rps",
                        "--verify-calls" => "calls",
                        "--verify-seconds" => "seconds",
                        "--verify-bytes" => "bytes",
                        "--verify-disk-bytes" => "disk",
                        "--verify-ram-bytes" => "ram",
                        _ => "free",
                    },
                    n,
                ))
            }
            other => return Err(format!("UNKNOWN_ARGUMENT: {other}")),
        }
    }
    let mut budget = StrictBudget::of_mode(mode, l);
    for (field, n) in set {
        *match field {
            "rps" => &mut budget.rps,
            "calls" => &mut budget.max_calls,
            "seconds" => &mut budget.max_seconds,
            "bytes" => &mut budget.max_bytes,
            "disk" => &mut budget.max_disk_bytes,
            "ram" => &mut budget.max_ram_bytes,
            _ => &mut budget.min_free_bytes,
        } = n;
    }
    let outcome = drive(&DriverRequest {
        run: run.ok_or("--run required")?.into(),
        budget,
        offline,
        resume,
        mode,
        provider_evidence,
        cancelled,
    })?;
    let text = format!(
        "{}verification_spec_id={}\nverification_attempt_id={}\nverification_result_id={}\nstatus={} reason={}\n",
        outcome.terminal,
        outcome.verification_spec_id,
        outcome.verification_attempt_id,
        outcome.verification_result_id.as_deref().unwrap_or("none"),
        outcome.status,
        outcome.reason.as_deref().unwrap_or("none")
    );
    Ok((
        text,
        mode.verified_status() == Some(outcome.status.as_str()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C0E1-P4-RESUME/P5-CLI: online strict needs measured, recent FREE_ONLY provider evidence
    /// for its endpoint that also qualifies archive reads; anything else is refused.
    #[test]
    fn strict_provider_gate_requires_fresh_free_archive_evidence() {
        use crate::orchestration::provider_evidence::{ProviderEvidence, ProviderProof};
        let dir = std::env::temp_dir().join(format!("p0_gate_{}_{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config = darkbot_solana::runtime_config::RuntimeConfig::default();
        let url = "http://archive.invalid";
        let sha = |b: &[u8]| hex(&solana_sdk::hash::hash(b).to_bytes());
        let evidence = |archive_qualified| ProviderEvidence {
            providers: vec![ProviderProof {
                policy: "FREE_ONLY".into(),
                passed: true,
                provider_binding: sha(url.as_bytes()),
                measured_at_unix_s: 1000.0,
                evidence_sha256: Some(sha(b"fixture")),
                allowed_rps: 5.0,
                available_calls: 3,
            }],
            wss_provider_binding: None,
            wss_free_only: false,
            archive_qualified,
            source_sha256: sha(b"measured"),
        };
        let write = |name: &str, e: &ProviderEvidence| {
            let path = dir.join(name);
            journal::write_typed_value(&path, source::FE01, e).unwrap();
            path
        };
        let qualified = write("archive.p0j", &evidence(true));
        let unqualified = write("no_archive.p0j", &evidence(false));
        assert_eq!(
            strict_provider_gate(&config, url, &qualified, 1001.0),
            Ok(())
        );
        assert_eq!(
            strict_provider_gate(&config, url, &unqualified, 1001.0),
            Err("STRICT_ARCHIVE_UNQUALIFIED".into())
        );
        assert!(strict_provider_gate(&config, "http://other.invalid", &qualified, 1001.0).is_err());
        let stale = 1001.0 + config.limits.fe_qualification_age_seconds as f64;
        assert!(strict_provider_gate(&config, url, &qualified, stale).is_err());
        assert_eq!(
            strict_provider_gate(&config, url, &dir.join("missing.p0j"), 1001.0),
            Err("STRICT_PROVIDER_EVIDENCE_INVALID".into())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// DEC-08: a crosscheck's spec binds both endpoints (never one alone, never the endpoints).
    #[test]
    fn dec08_the_provider_binding_holds_both_crosscheck_endpoints() {
        let (a, b) = ("http://a.invalid", "http://b.invalid");
        assert_eq!(
            provider_binding(a, None),
            solana_sdk::hash::hash(a.as_bytes()).to_bytes()
        );
        let both = provider_binding(a, Some(b));
        for other in [
            provider_binding(a, None),
            provider_binding(b, None),
            provider_binding(b, Some(a)),
        ] {
            assert_ne!(both, other);
        }
    }

    /// DEC-08 (review): a history read plans no more than its provider proved — the measured
    /// proof of the endpoint covers the method's RPS and requests.
    #[test]
    fn dec08_a_history_read_plans_within_its_providers_proof() {
        use crate::orchestration::provider_evidence::{ProviderEvidence, ProviderProof};
        let dir = std::env::temp_dir().join(format!("p0_cap_{}_{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let url = "http://archive.invalid";
        let sha = |b: &[u8]| hex(&solana_sdk::hash::hash(b).to_bytes());
        let path = dir.join("evidence.p0j");
        journal::write_typed_value(
            &path,
            source::FE01,
            &ProviderEvidence {
                providers: vec![ProviderProof {
                    policy: "FREE_ONLY".into(),
                    passed: true,
                    provider_binding: sha(url.as_bytes()),
                    measured_at_unix_s: 1000.0,
                    evidence_sha256: Some(sha(b"fixture")),
                    allowed_rps: 5.0,
                    available_calls: 300,
                }],
                wss_provider_binding: None,
                wss_free_only: false,
                archive_qualified: true,
                source_sha256: sha(b"measured"),
            },
        )
        .unwrap();
        let budget = |rps, max_calls| StrictBudget {
            rps,
            max_calls,
            max_seconds: 1,
            max_bytes: 1,
            max_disk_bytes: 1,
            max_ram_bytes: 1,
            min_free_bytes: 1,
        };
        assert_eq!(method_capacity(&path, url, &budget(5, 300)), Ok(()));
        for over in [budget(6, 300), budget(5, 301)] {
            assert!(method_capacity(&path, url, &over)
                .unwrap_err()
                .starts_with("PROVIDER_CAPACITY_BELOW_METHOD_BUDGET"));
        }
        assert_eq!(
            method_capacity(&path, "http://other.invalid", &budget(1, 1)).unwrap_err(),
            "FE_PROVIDER_EVIDENCE_MISSING"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// DEC-08: each verification method has its own typed requests/bytes/RPS budget; time, disk,
    /// RAM and free space are shared.
    #[test]
    fn dec08_each_method_has_its_own_typed_budget() {
        let mut l = darkbot_solana::runtime_config::Limits::default();
        (l.verify_rps, l.verify_calls, l.verify_mib) = (1, 10, 100);
        (l.index_rps, l.index_calls, l.index_mib) = (2, 20, 200);
        (l.crosscheck_rps, l.crosscheck_calls, l.crosscheck_mib) = (3, 30, 300);
        (
            l.verify_seconds,
            l.verify_disk_mib,
            l.verify_ram_mib,
            l.verify_free_mib,
        ) = (7, 8, 9, 6);
        let mib = 1024 * 1024;
        let of = |mode| {
            let b = StrictBudget::of_mode(mode, &l);
            (b.rps, b.max_calls, b.max_bytes / mib)
        };
        assert_eq!(of(VerifyMode::Strict), (1, 10, 100));
        assert_eq!(of(VerifyMode::Off), (1, 10, 100));
        assert_eq!(of(VerifyMode::Index { max_pages: 1 }), (2, 20, 200));
        assert_eq!(of(VerifyMode::Crosscheck { max_pages: 1 }), (3, 30, 300));
        let b = StrictBudget::of_mode(VerifyMode::Crosscheck { max_pages: 1 }, &l);
        assert_eq!(
            (
                b.max_seconds,
                b.max_disk_bytes,
                b.max_ram_bytes,
                b.min_free_bytes
            ),
            (7, 8 * mib, 9 * mib, 6 * mib)
        );
        use crate::storage::preflight::{VerifyBudget, VerifyMode as P};
        let pre = |m| {
            let v = VerifyBudget::for_mode(m, &l);
            (v.rps, v.calls, v.bytes / mib, v.max_pages)
        };
        assert_eq!(pre(P::Strict), (1, 10, 100, l.verify_pages));
        assert_eq!(pre(P::Index), (2, 20, 200, l.verify_pages));
        assert_eq!(pre(P::Crosscheck), (3, 30, 300, l.verify_pages));
    }

    /// DEC-08 (C-02): the second source of a CROSSCHECK is the second configured endpoint on
    /// another host with its own measured FREE_ONLY archive proof; missing, the primary itself
    /// or on the primary's host it is NOT_RUN; present without its proof it is refused.
    #[test]
    fn dec08_crosscheck_needs_a_second_independent_qualified_source() {
        use crate::orchestration::provider_evidence::{ProviderEvidence, ProviderProof};
        let dir = std::env::temp_dir().join(format!("p0_cc_{}_{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sha = |b: &[u8]| hex(&solana_sdk::hash::hash(b).to_bytes());
        let (primary, second) = ("http://archive-a.invalid", "http://archive-b.invalid");
        let proof = |url: &str| ProviderProof {
            policy: "FREE_ONLY".into(),
            passed: true,
            provider_binding: sha(url.as_bytes()),
            measured_at_unix_s: 1000.0,
            evidence_sha256: Some(sha(b"fixture")),
            allowed_rps: 5.0,
            available_calls: 3,
        };
        let evidence = |urls: &[&str]| {
            let path = dir.join(format!("evidence_{}.p0j", urls.len()));
            let e = ProviderEvidence {
                providers: urls.iter().map(|u| proof(u)).collect(),
                wss_provider_binding: None,
                wss_free_only: false,
                archive_qualified: true,
                source_sha256: sha(b"measured"),
            };
            journal::write_typed_value(&path, source::FE01, &e).unwrap();
            path
        };
        let both = evidence(&[primary, second]);
        let config = |endpoints: &[&str]| darkbot_solana::runtime_config::RuntimeConfig {
            rpc_endpoints: endpoints.iter().map(|s| s.to_string()).collect(),
            fallback: false,
            ..Default::default()
        };
        let check = |endpoints: &[&str], evidence: &Path| {
            crosscheck_source(&config(endpoints), primary, evidence, 1001.0)
        };
        assert_eq!(
            check(&[primary], &both),
            Ok(Err("CROSSCHECK_SECOND_PROVIDER_MISSING"))
        );
        assert_eq!(
            check(&[primary, primary], &both),
            Ok(Err("CROSSCHECK_SECOND_PROVIDER_MISSING"))
        );
        assert_eq!(
            check(&[primary, "http://archive-a.invalid/other"], &both),
            Ok(Err("CROSSCHECK_PROVIDER_NOT_INDEPENDENT"))
        );
        assert_eq!(check(&[primary, second], &both), Ok(Ok(second.to_string())));
        assert!(
            check(&[primary, second], &evidence(&[primary])).is_err(),
            "an unqualified second source is refused"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// C0E1-P4-RESUME: the post-capture deployment proof reads the program after the window
    /// through the production inspector (loopback RPC, never Mainnet) and records it, bound to
    /// its result, next to the spec. Bytes that do not carry the audited reference hash are no
    /// continuity; no RPC is no read; continuity needs the reference hash before and after, the
    /// same ProgramData and an after-read at or past the window's last slot.
    #[test]
    fn deployment_proof_is_read_after_the_window_and_bound_to_its_result() {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::str::FromStr;
        let run =
            std::env::temp_dir().join(format!("p0_deploy_{}_{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&run);
        let market = run.join("capture/market");
        std::fs::create_dir_all(&market).unwrap();
        let program =
            solana_sdk::pubkey::Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET)
                .unwrap();
        let program_data = darkbot_project0::protocol::program_data_address(&program).to_string();
        let reference = darkbot_project0::protocol::PROGRAM_DATA_REFERENCE_HASH.to_string();
        let before = super::super::paths::Deployment {
            context_slot: 5,
            hash: reference.clone(),
            program_data: program_data.clone(),
        };
        let mut summary = crate::analytics::econ::tests::market_summary(4.0);
        summary.deployment_continuity.before = Some(before.clone());
        journal::write_typed_value(
            &market.join(super::super::MARKET_SUMMARY_JOURNAL),
            source::MARKET,
            &super::super::MarketFinal::checkpoint(summary),
        )
        .unwrap();
        // A loopback RPC with a ProgramData account of other bytes (not the audited build).
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let mut data = vec![3u8, 0, 0, 0];
            data.extend(7u64.to_le_bytes());
            data.resize(45, 0);
            data.extend(b"not the audited program");
            let body = serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": {
                "context": {"slot": 100},
                "value": {"owner": darkbot_project0::protocol::BPF_LOADER_UPGRADEABLE_ID,
                    "executable": false, "lamports": 1, "rentEpoch": 0,
                    "data": [base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data), "base64"]}}})
            .to_string();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut request = vec![0u8; length];
                let _ = reader.read_exact(&mut request);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            }
        });
        let proof = prove_deployment(&run, "spec", "result", 80, Some(&url)).unwrap();
        let after = proof.after.clone().expect("after read");
        assert_eq!(
            (after.context_slot, after.program_data.as_str()),
            (100, program_data.as_str())
        );
        assert_ne!(after.hash, reference);
        assert_eq!((proof.verified, proof.error.as_deref()), (false, None));
        assert_eq!(proof.before, Some(before.clone()));
        let path = deployment_proof_path(&run, "spec", "result");
        assert_eq!(
            journal::read_typed_value::<DeploymentProof>(&path).unwrap(),
            proof
        );
        // No RPC: no read, never continuity.
        let none = prove_deployment(&run, "spec", "other", 80, None).unwrap();
        assert_eq!(
            (none.after, none.verified, none.error.as_deref()),
            (None, false, Some("DEPLOYMENT_RPC_MISSING"))
        );
        // Continuity itself (the audited build before and after, same ProgramData, read at or past
        // the window's last slot).
        let audited = |slot| super::super::paths::Deployment {
            context_slot: slot,
            hash: reference.clone(),
            program_data: program_data.clone(),
        };
        let verified = super::super::paths::verified;
        assert!(verified(Some(&before), Some(&audited(100)), Some(80)));
        assert!(!verified(Some(&before), Some(&audited(79)), Some(80)));
        assert!(!verified(Some(&before), Some(&after), Some(80)));
        assert!(!verified(None, Some(&audited(100)), Some(80)));
        let moved = super::super::paths::Deployment {
            program_data: "other".into(),
            ..audited(100)
        };
        assert!(!verified(Some(&before), Some(&moved), Some(80)));
        std::fs::remove_dir_all(&run).unwrap();
    }

    /// The spec identity is stable for the same definition and changes with any bound input;
    /// evidence and attempt records round-trip as typed journals.
    #[test]
    fn spec_identity_is_stable_and_bound_to_its_inputs() {
        let budget = StrictBudget {
            rps: 2,
            max_calls: 10,
            max_seconds: 30,
            max_bytes: 1 << 20,
            max_disk_bytes: 1 << 20,
            max_ram_bytes: 1 << 30,
            min_free_bytes: 1,
        };
        let spec = VerificationSpec {
            schema: SPEC_SCHEMA.into(),
            capture_run_id: "run".into(),
            manifest_sha256: [1; 32],
            window_start_unix_ns: 10,
            window_end_unix_ns: 20,
            mode: "strict".into(),
            program: "p".into(),
            decoder_binary_sha256: [2; 32],
            protocol_pin: "pin".into(),
            journal_version: 1,
            provider_policy: "FREE_ONLY".into(),
            provider_binding_sha256: None,
            budget,
        };
        let id = spec.id().unwrap();
        assert_eq!(id, spec.clone().id().unwrap());
        for changed in [
            VerificationSpec {
                capture_run_id: "other".into(),
                ..spec.clone()
            },
            VerificationSpec {
                window_end_unix_ns: 21,
                ..spec.clone()
            },
            VerificationSpec {
                decoder_binary_sha256: [3; 32],
                ..spec.clone()
            },
            VerificationSpec {
                provider_binding_sha256: Some([4; 32]),
                ..spec.clone()
            },
        ] {
            assert_ne!(changed.id().unwrap(), id);
        }
        let bytes = payload::encode(&spec).unwrap();
        let back: VerificationSpec =
            payload::decode(VerificationSpec::KIND, VerificationSpec::SCHEMA, &bytes).unwrap();
        assert_eq!(back, spec);
    }

    /// Offline end to end on a layout-V2 run with a real manifest and CAPTURE phase: spec,
    /// evidence and attempt records are typed and kept; a second run without resume is refused;
    /// resume keeps the spec and adds an attempt; a changed capture file fails closed.
    #[test]
    fn driver_records_identities_and_refuses_changed_capture() {
        use crate::analytics::measurement::{journal as phase, persist_manifest, MeasurementPlan};
        use crate::run_layout::{RunLayout, RunPaths};
        let d = std::env::temp_dir().join(format!("p0_drv_{}_{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&d);
        let root = d.join("run_x");
        let paths = RunPaths::new(&root, RunLayout::V2);
        paths.create().unwrap();
        let plan = MeasurementPlan::new(60).unwrap();
        let clock = darkbot_common::timing::Clock::new();
        persist_manifest(&paths, "run_x", "test", &plan, clock).unwrap();
        let window = plan.capture_window(clock, 1_000 * NS).unwrap();
        phase(&paths, clock, Phase::Capture(window)).unwrap();
        let budget = StrictBudget {
            rps: 2,
            max_calls: 10,
            max_seconds: 30,
            max_bytes: 1 << 20,
            max_disk_bytes: 1 << 24,
            max_ram_bytes: 1 << 34,
            min_free_bytes: 1,
        };
        let req = |resume| DriverRequest {
            run: root.clone(),
            budget: budget.clone(),
            offline: true,
            resume,
            mode: VerifyMode::Strict,
            provider_evidence: None,
            cancelled: None,
        };
        let first = drive(&req(false)).unwrap();
        assert_ne!(first.status, "CANONICAL_VERIFIED");
        let out = root
            .join("verification")
            .join(format!("{NATIVE_PREFIX}{}", first.verification_spec_id));
        let spec: VerificationSpec = journal::read_typed_value(&out.join("spec.p0j")).unwrap();
        assert_eq!(
            (spec.capture_run_id.as_str(), spec.provider_binding_sha256),
            ("run_x", None)
        );
        assert_eq!(spec.window_end_unix_ns - spec.window_start_unix_ns, 60 * NS);
        let end: AttemptRecord = journal::read_typed_value(
            &out.join("attempts")
                .join(&first.verification_attempt_id)
                .join("end.p0j"),
        )
        .unwrap();
        assert_eq!(end.status.as_deref(), Some(first.status.as_str()));
        assert_eq!(
            drive(&req(false)).unwrap_err(),
            "SPEC_EXISTS_USE_RESUME_VERIFICATION"
        );
        let second = drive(&req(true)).unwrap();
        assert_eq!(second.verification_spec_id, first.verification_spec_id);
        assert_ne!(
            second.verification_attempt_id,
            first.verification_attempt_id
        );
        // Off (C0E1-P4-RESUME): its own stable spec (the mode is part of the identity), an
        // attempt NOT_RUN without any chain RPC and no result; resume keeps the spec.
        let off = |resume| DriverRequest {
            mode: VerifyMode::Off,
            offline: false,
            ..req(resume)
        };
        let o1 = drive(&off(false)).unwrap();
        assert_eq!(
            (
                o1.status.as_str(),
                o1.reason.as_deref(),
                &o1.verification_result_id
            ),
            ("NOT_RUN", Some("VERIFICATION_OFF"), &None)
        );
        assert_ne!(o1.verification_spec_id, first.verification_spec_id);
        let off_dir = root
            .join("verification")
            .join(format!("{NATIVE_PREFIX}{}", o1.verification_spec_id));
        let off_spec: VerificationSpec =
            journal::read_typed_value(&off_dir.join("spec.p0j")).unwrap();
        assert_eq!(
            (off_spec.mode.as_str(), off_spec.provider_binding_sha256),
            ("off", None)
        );
        assert!(!off_dir.join("strict").exists(), "off reads no chain data");
        assert_eq!(
            drive(&off(false)).unwrap_err(),
            "SPEC_EXISTS_USE_RESUME_VERIFICATION"
        );
        let o2 = drive(&off(true)).unwrap();
        assert_eq!(o2.verification_spec_id, o1.verification_spec_id);
        assert_ne!(o2.verification_attempt_id, o1.verification_attempt_id);
        // DEC-08: the run's methods as shown — off NOT_RUN, strict incomplete with its scope
        // unresolved (its kinds and markers unknown, never 0).
        let methods = super::super::verification_view::methods(&root);
        let [off_state, strict_state] = &methods[..] else {
            panic!("{methods:?}")
        };
        assert_eq!(
            (
                off_state.method.as_str(),
                off_state.status.as_str(),
                off_state.reason.as_deref()
            ),
            ("off", "NOT_RUN", Some("VERIFICATION_OFF"))
        );
        assert_eq!(
            (strict_state.method.as_str(), strict_state.status.as_str()),
            ("strict", "HISTORICAL_INCOMPLETE")
        );
        assert!(strict_state.qualified.is_empty() && strict_state.markers.is_empty());
        assert_eq!(
            (strict_state.progress_unit, strict_state.progress_total),
            ("slots", None)
        );
        std::fs::write(root.join("capture/late_file.bin"), b"x").unwrap();
        assert_eq!(drive(&req(true)).unwrap_err(), "CAPTURE_EVIDENCE_CHANGED");
        std::fs::remove_dir_all(d).unwrap();
    }
}

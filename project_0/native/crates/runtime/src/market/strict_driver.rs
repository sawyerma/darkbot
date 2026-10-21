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
}

/// Post-capture verification mode of the native driver (PHASENPLAN L.2). `index`/`crosscheck`
/// stay with the pinned Python verifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyMode {
    /// No historical chain RPC: spec and attempt identities, status `NOT_RUN`, no result.
    Off,
    Strict,
}
impl VerifyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            VerifyMode::Off => "off",
            VerifyMode::Strict => "strict",
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

fn now_unix_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
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
        .map_err(|_| "NATIVE_MANIFEST_MISSING: use the Python driver for historical runs")?;
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
    let provider = if req.offline || req.mode == VerifyMode::Off {
        None
    } else {
        let url = darkbot_solana::runtime_config::active()
            .rpc_url(&|k| std::env::var(k).ok())
            .ok_or("RPC_URL_MISSING")?;
        Some(solana_sdk::hash::hash(url.as_bytes()).to_bytes())
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
        at_unix_ns: now_unix_ns(),
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
        record.at_unix_ns = now_unix_ns();
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
    };
    let outcome: Result<StrictResult, String> = run_strict(job);
    record.at_unix_ns = now_unix_ns();
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

#[cfg(test)]
mod tests {
    use super::*;

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
        std::fs::write(root.join("capture/late_file.bin"), b"x").unwrap();
        assert_eq!(drive(&req(true)).unwrap_err(), "CAPTURE_EVIDENCE_CHANGED");
        std::fs::remove_dir_all(d).unwrap();
    }
}

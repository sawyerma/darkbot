//! DEC-16: unbounded runs (`RunHorizon::UntilStopped`) — the typed, persistent, identity-bound
//! qualification `RunHorizon::admit` requires, its store and its producer
//! (`p0ctl horizon qualify`).
//!
//! A qualification binds one mode, the run's identity (code, configuration including its
//! horizon, safety flags), its executables and the horizon's window and runway, and records five
//! parts: the mode's runtime runs rolling windows; SK-07 rolling segments and crash recovery on
//! this storage; the rolling capacity for the runway (the capacity preflight over the measured
//! data rate); the provider budget of a window; and the provider evidence it rests on, with the
//! time it stays fresh. Each part is PASS, FAIL or NOT_RUN with its reason; nothing missing is a
//! PASS. A qualification admits nothing by itself: the run still needs its network, start and
//! operation approvals.
use super::boot_audit::{self, AuditedIdentity, SafetyFlags};
use super::provider_evidence;
use crate::analytics::journal::{self, kind, Provenance};
use crate::analytics::payload::Payload;
use crate::storage::{preflight, segments};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SCHEMA: u32 = 1;
/// The controller's store of qualifications, content-addressed like the provider evidence:
/// `<id>.p0j`, the id the first 16 hex digits of the record's SHA-256.
pub const STORE: &str = "data/until_stopped";
/// The modes whose runtime runs rolling windows. A mode outside stays gated whatever else is
/// qualified: econ4h measures its windows (DEC-16 stage 2) but stays gated until its Shadow
/// owner's windows exist (stage 3: owner windows, drains, owner journals).
pub const ROLLING_MODES: &[&str] = &["market"];
/// The modes an unbounded run may be qualified for.
pub const MODES: &[&str] = &["econ4h", "market", "shadow"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartState {
    Pass,
    Fail,
    NotRun,
}
impl PartState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::NotRun => "NOT_RUN",
        }
    }
}

/// One qualification part with the reason for its state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub state: PartState,
    pub reason: String,
}
impl Part {
    fn of(state: PartState, reason: impl Into<String>) -> Self {
        Self {
            state,
            reason: reason.into(),
        }
    }
}

/// What a qualification is bound to; recomputed for the run that would start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub mode: String,
    pub identity: AuditedIdentity,
    pub binary_sha256: String,
    pub checkpoint_seconds: u64,
    pub minimum_runway_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Qualification {
    pub schema: u32,
    pub binding: Binding,
    pub runtime: Part,
    pub sk07: Part,
    pub capacity: Part,
    /// Bytes the runway needs on the run volume (the capacity preflight), when measurable.
    pub capacity_need_bytes: Option<u64>,
    pub provider_budget: Part,
    pub provider_evidence: Part,
    pub evidence_id: Option<String>,
    /// The last second the provider evidence stays fresh: its oldest measurement plus the
    /// qualification age of the configuration (`validate_provider_evidence`).
    pub evidence_valid_until_unix_s: Option<u64>,
    pub qualified_at_unix_s: u64,
}
impl Payload for Qualification {
    const KIND: u16 = kind::UNTIL_STOPPED_QUALIFICATION;
}

impl Qualification {
    /// The parts with their names, in the order they are reported.
    pub fn parts(&self) -> [(&'static str, &Part); 5] {
        [
            ("runtime", &self.runtime),
            ("sk07", &self.sk07),
            ("capacity", &self.capacity),
            ("provider_budget", &self.provider_budget),
            ("provider_evidence", &self.provider_evidence),
        ]
    }

    /// What keeps an unbounded run bound to `current` from starting at `now` (empty: nothing).
    pub fn gaps(&self, current: &Binding, now: u64) -> Vec<String> {
        let mut gaps = Vec::new();
        if self.schema != SCHEMA {
            gaps.push(format!("qualification schema {}", self.schema));
        }
        let (a, b) = (&self.binding, current);
        let mut drift = Vec::new();
        if a.mode != b.mode {
            drift.push("mode");
        }
        if a.identity.source_head != b.identity.source_head
            || a.identity.source_dirty_sha256 != b.identity.source_dirty_sha256
        {
            drift.push("source");
        }
        if a.identity.runtime_config_sha256 != b.identity.runtime_config_sha256 {
            drift.push("runtime_config");
        }
        if a.identity.journal_version != b.identity.journal_version
            || a.identity.discriminator_table_sha256 != b.identity.discriminator_table_sha256
        {
            drift.push("schema");
        }
        if a.identity.live_enabled != b.identity.live_enabled
            || a.identity.submission_enabled != b.identity.submission_enabled
        {
            drift.push("safety_flags");
        }
        if a.binary_sha256 != b.binary_sha256 {
            drift.push("binary");
        }
        if (a.checkpoint_seconds, a.minimum_runway_seconds)
            != (b.checkpoint_seconds, b.minimum_runway_seconds)
        {
            drift.push("horizon");
        }
        if !drift.is_empty() {
            gaps.push(format!("qualification drift: {}", drift.join(",")));
        }
        for (name, part) in self.parts() {
            if part.state != PartState::Pass {
                gaps.push(format!("{name} {}: {}", part.state.as_str(), part.reason));
            }
        }
        if self.provider_evidence.state == PartState::Pass
            && self
                .evidence_valid_until_unix_s
                .is_none_or(|until| now > until)
        {
            gaps.push("provider evidence expired".into());
        }
        gaps
    }

    /// Operator text: one line per part.
    pub fn terminal(&self, id: &str) -> String {
        let b = &self.binding;
        let mut out = format!(
            "PROJECT 0 UNTIL-STOPPED QUALIFICATION {id} mode={} window={}s runway={}s\n",
            b.mode, b.checkpoint_seconds, b.minimum_runway_seconds
        );
        for (name, part) in self.parts() {
            out.push_str(&format!(
                "  {name:<18} {:<8} {}\n",
                part.state.as_str(),
                part.reason
            ));
        }
        out
    }
}

/// The binding of an unbounded run of `mode` with these operational arguments and this horizon
/// (`until-stopped:<checkpoint>:<runway>`): its resolved identity and executables.
pub fn binding(
    project: &Path,
    mode: &str,
    operational: &[String],
    horizon: &str,
    env: &dyn Fn(&str) -> Option<String>,
    binary: &dyn Fn(&str) -> Result<String, String>,
) -> Result<(Binding, darkbot_solana::runtime_config::Resolved), String> {
    if !MODES.contains(&mode) {
        return Err(format!("UNTIL_STOPPED_MODE: {mode} (econ4h|market|shadow)"));
    }
    let darkbot_solana::runtime_config::Horizon::UntilStopped {
        checkpoint_seconds,
        minimum_runway_seconds,
    } = darkbot_solana::runtime_config::horizon(horizon)?
    else {
        return Err("UNTIL_STOPPED_HORIZON: a fixed duration is no unbounded run".into());
    };
    let resolved = boot_audit::start_config(mode, operational, horizon, env)?;
    let identity = boot_audit::identity_with(project, SafetyFlags::from_env(), &resolved.config)?;
    Ok((
        Binding {
            mode: mode.into(),
            identity,
            binary_sha256: binary(mode)?,
            checkpoint_seconds,
            minimum_runway_seconds,
        },
        resolved,
    ))
}

/// SK-07 on this storage: rolling segments rotated at window boundaries, a crash with a torn
/// tail, the recovery continuing the chain, and its verification — all on the real writer in
/// `dir` (removed afterwards).
pub fn sk07_selftest(dir: &Path) -> Part {
    use crate::market::observer::rows::MarketCoverage;
    let run = || -> Result<String, String> {
        let io = |e: std::io::Error| e.to_string();
        std::fs::create_dir_all(dir).map_err(io)?;
        let first = dir.join("sk07.p0j");
        let row = |slot| MarketCoverage::SlotFailed {
            slot,
            error: String::new(),
        };
        let mut w = segments::SegmentedWriter::create(&first, 0).map_err(io)?;
        for slot in 0..2 {
            w.append_typed(Provenance::default(), &row(slot))
                .map_err(io)?;
        }
        w.rotate_now().map_err(io)?;
        for slot in 2..4 {
            w.append_typed(Provenance::default(), &row(slot))
                .map_err(io)?;
        }
        w.sync().map_err(io)?;
        drop(w);
        // The crash tore the last frame (slot 3).
        let crashed = segments::segment_path(&first, 2);
        let bytes = std::fs::read(&crashed).map_err(io)?;
        std::fs::write(&crashed, &bytes[..bytes.len() - 1]).map_err(io)?;
        let (mut w, how) = segments::SegmentedWriter::recover(&first, 0).map_err(io)?;
        if !matches!(how, segments::Recovered::Crash { segment: 2, torn_bytes } if torn_bytes > 0) {
            return Err(format!("recovery {how:?}"));
        }
        for slot in 4..6 {
            w.append_typed(Provenance::default(), &row(slot))
                .map_err(io)?;
        }
        w.rotate_now().map_err(io)?;
        w.append_typed(Provenance::default(), &row(6)).map_err(io)?;
        w.seal(true).map_err(io)?;
        if std::fs::read(&crashed).map_err(io)? != bytes[..bytes.len() - 1] {
            return Err("the crashed segment changed".into());
        }
        let s = segments::verify_chain(&first)?;
        // Sealed, never complete: a chain continued after a crash may have lost frames.
        if (s.segments, s.events, s.sealed, s.complete, s.recovered) != (4, 6, true, false, 1) {
            return Err(format!("chain {s:?}"));
        }
        let mut r = segments::ChainReader::open(&first).map_err(io)?;
        let mut slots = Vec::new();
        while let Some(f) = r.next_frame().map_err(io)? {
            if f.kind != journal::MANIFEST {
                let MarketCoverage::SlotFailed { slot, .. } = f.typed().map_err(io)? else {
                    return Err("frame".into());
                };
                slots.push(slot);
            }
        }
        if slots != [0, 1, 2, 4, 5, 6] {
            return Err(format!("replay {slots:?}"));
        }
        Ok(format!(
            "rolling segments and crash recovery verified on {} (4 segments, 1 recovery, {} torn bytes)",
            dir.display(),
            s.torn_bytes
        ))
    };
    let part = match run() {
        Ok(reason) => Part::of(PartState::Pass, reason),
        Err(e) => Part::of(PartState::Fail, format!("SK07_SELFTEST: {e}")),
    };
    let _ = std::fs::remove_dir_all(dir);
    part
}

/// The rolling capacity for the runway: the capacity preflight of a run of `runway_seconds`
/// on the run volume over the measured data rate; its provider and verifier checks are parts
/// of their own. Every check QUALIFIED (or not applicable) passes; a BLOCKED one fails; one not
/// measured leaves the part NOT_RUN.
pub fn capacity(report: &preflight::CapacityReport) -> Part {
    use preflight::Verdict::*;
    let relevant: Vec<_> = report
        .checks
        .iter()
        .filter(|c| c.name != "provider" && c.name != "verification")
        .collect();
    let list = |want: &[preflight::Verdict]| -> Vec<String> {
        relevant
            .iter()
            .filter(|c| want.contains(&c.verdict))
            .map(|c| format!("{} ({})", c.name, c.detail))
            .collect()
    };
    let blocked = list(&[Blocked]);
    if !blocked.is_empty() {
        return Part::of(PartState::Fail, blocked.join("; "));
    }
    let open = list(&[NotRun, CapacityUnverified]);
    if !open.is_empty() {
        return Part::of(PartState::NotRun, open.join("; "));
    }
    Part::of(
        PartState::Pass,
        format!(
            "runway {} s: needs {} bytes",
            report.request.duration_s,
            report
                .disk_need_bytes
                .map_or("UNKNOWN".to_string(), |b| b.to_string())
        ),
    )
}

/// The provider evidence and the provider budget of the runway. The evidence must qualify every
/// usable endpoint (FREE_ONLY, measured, fresh: `validate_provider_evidence`); the budget must
/// fit each endpoint's proof: `rpc_rps` within its allowed rate, and its available calls (a
/// quota of no stated period) must cover every window of the runway at the window cap
/// `max_requests` — one window's cap covers no continuous run.
pub fn provider(
    config: &darkbot_solana::runtime_config::RuntimeConfig,
    evidence: Option<&provider_evidence::ProviderEvidence>,
    now: u64,
    binding: &Binding,
) -> (Part, Part, Option<u64>) {
    let Some(proof) = evidence else {
        let missing = "no provider evidence (p0ctl horizon qualify --provider-evidence ID|FILE)";
        return (
            Part::of(PartState::NotRun, missing),
            Part::of(PartState::NotRun, missing),
            None,
        );
    };
    let Some(primary) = config.rpc_endpoints.first() else {
        let none = "no RPC endpoint configured";
        return (
            Part::of(PartState::Fail, none),
            Part::of(PartState::Fail, none),
            None,
        );
    };
    let evidence_part =
        match provider_evidence::validate_provider_evidence(config, primary, proof, now as f64) {
            Ok(()) => Part::of(
                PartState::Pass,
                format!("{} provider proof(s), FREE_ONLY", proof.providers.len()),
            ),
            Err(e) => Part::of(PartState::Fail, e),
        };
    let l = &config.limits;
    let windows = binding
        .minimum_runway_seconds
        .div_ceil(binding.checkpoint_seconds.max(1))
        .max(1);
    let need_calls = l.max_requests.saturating_mul(windows);
    let short: Vec<String> = proof
        .providers
        .iter()
        .filter(|p| p.allowed_rps < l.rpc_rps as f64 || p.available_calls < need_calls)
        .map(|p| {
            format!(
                "{}: allowed_rps {} / rpc_rps {}, available_calls {} / {need_calls} ({windows} window(s) × max_requests {})",
                &p.provider_binding[..p.provider_binding.len().min(12)],
                p.allowed_rps,
                l.rpc_rps,
                p.available_calls,
                l.max_requests
            )
        })
        .collect();
    let budget_part = if short.is_empty() {
        Part::of(
            PartState::Pass,
            format!(
                "rpc_rps {} and {need_calls} calls ({windows} window(s) × max_requests {}) within every proof",
                l.rpc_rps, l.max_requests
            ),
        )
    } else {
        Part::of(PartState::Fail, short.join("; "))
    };
    let oldest = proof
        .providers
        .iter()
        .map(|p| p.measured_at_unix_s)
        .filter(|m| m.is_finite() && *m >= 0.0)
        .fold(f64::INFINITY, f64::min);
    let valid_until = oldest
        .is_finite()
        .then(|| oldest as u64 + l.fe_qualification_age_seconds);
    (evidence_part, budget_part, valid_until)
}

/// Inputs of a qualification beyond its binding.
pub struct Inputs<'a> {
    pub project: &'a Path,
    pub runs_root: PathBuf,
    pub evidence_id: Option<String>,
    pub now: u64,
}

/// Qualifies an unbounded run of `binding` (resolved configuration `config`).
pub fn qualify(
    binding: Binding,
    config: &darkbot_solana::runtime_config::RuntimeConfig,
    capture_source: &str,
    inputs: &Inputs,
) -> Result<Qualification, String> {
    // The SK-07 self-test, the write probe and the host facts on the run volume.
    let work = inputs
        .runs_root
        .join(format!(".until-stopped-{}", std::process::id()));
    let runtime = if ROLLING_MODES.contains(&binding.mode.as_str()) {
        Part::of(
            PartState::Pass,
            format!("the {} runtime runs rolling windows", binding.mode),
        )
    } else {
        Part::of(
            PartState::NotRun,
            format!(
                "the {} runtime's Shadow owner has no windows yet (DEC-16 stage 3: owner windows, drains, owner journals)",
                binding.mode
            ),
        )
    };
    let sk07 = sk07_selftest(&work.join("sk07"));
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let request = preflight::Request::from_limits(
        &config.limits,
        binding.minimum_runway_seconds,
        &inputs.runs_root,
        capture_source,
    );
    let report = preflight::evaluate(
        request,
        preflight::measure_basis(
            &preflight::discover_runs(&inputs.runs_root, None),
            capture_source,
        ),
        None,
        preflight::probe_write_rate(&work, preflight::PROBE_BYTES),
        preflight::environment(&work),
    );
    let _ = std::fs::remove_dir_all(&work);
    let evidence = match &inputs.evidence_id {
        Some(id) => Some(
            journal::read_typed_value::<provider_evidence::ProviderEvidence>(
                &provider_evidence::resolve(inputs.project, id)?,
            )
            .map_err(|_| "BOOT_PROVIDER_EVIDENCE_FORMAT")?,
        ),
        None => None,
    };
    let (provider_evidence, provider_budget, evidence_valid_until_unix_s) =
        provider(config, evidence.as_ref(), inputs.now, &binding);
    Ok(Qualification {
        schema: SCHEMA,
        binding,
        runtime,
        sk07,
        capacity: capacity(&report),
        capacity_need_bytes: report.disk_need_bytes,
        provider_budget,
        provider_evidence,
        evidence_id: inputs.evidence_id.clone(),
        evidence_valid_until_unix_s,
        qualified_at_unix_s: inputs.now,
    })
}

/// Stores a qualification under its id (every record is its own: a journal header is unique;
/// a different record under a stored id is refused).
pub fn put(project: &Path, q: &Qualification) -> Result<String, String> {
    let io = |e: std::io::Error| format!("UNTIL_STOPPED_STORE: {e}");
    let store = project.join(STORE);
    std::fs::create_dir_all(&store).map_err(io)?;
    let temp = store.join(format!(".record.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&temp);
    let stored = (|| {
        journal::write_typed_value(&temp, journal::source::UNTIL_STOPPED, q).map_err(io)?;
        let bytes = std::fs::read(&temp).map_err(io)?;
        let id = journal::hex(&solana_sdk::hash::hash(&bytes).to_bytes())[..16].to_string();
        let target = store.join(format!("{id}.p0j"));
        match std::fs::hard_link(&temp, &target) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if std::fs::read(&target).map_err(io)? != bytes {
                    return Err("UNTIL_STOPPED_ID_COLLISION".to_string());
                }
            }
            Err(e) => return Err(io(e)),
        }
        std::fs::File::open(&store)
            .and_then(|d| d.sync_all())
            .map_err(io)?;
        Ok(id)
    })();
    let _ = std::fs::remove_file(&temp);
    stored
}

/// The stored qualification of `id`: a malformed id or one never stored is refused, and the
/// bytes must still be the ones the id names.
pub fn get(project: &Path, id: &str) -> Result<Qualification, String> {
    if !provider_evidence::is_id(id) {
        return Err(
            "UNTIL_STOPPED_ID_INVALID: a qualification id is 16 lowercase hex digits".into(),
        );
    }
    let path = project.join(STORE).join(format!("{id}.p0j"));
    let read = || -> Result<Vec<u8>, String> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(&path)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => "UNTIL_STOPPED_QUALIFICATION_UNKNOWN".to_string(),
                _ => format!("UNTIL_STOPPED_STORE: {e}"),
            })?
            .take(provider_evidence::RECORD_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("UNTIL_STOPPED_STORE: {e}"))?;
        if bytes.len() > provider_evidence::RECORD_LIMIT {
            return Err("UNTIL_STOPPED_QUALIFICATION_LIMIT".into());
        }
        Ok(bytes)
    };
    let bytes = read()?;
    if !provider_evidence::names(id, &bytes) {
        return Err("UNTIL_STOPPED_QUALIFICATION_CHANGED".into());
    }
    let q = journal::read_typed_value(&path)
        .map_err(|_| "UNTIL_STOPPED_QUALIFICATION_FORMAT".to_string())?;
    // The record decoded is the one the id names (read again after the decode).
    if read()? != bytes {
        return Err("UNTIL_STOPPED_QUALIFICATION_CHANGED".into());
    }
    Ok(q)
}

/// What an admitted unbounded run's rolling windows check against: its project (the stores),
/// its binding, its qualification and its resolved configuration — and, for an autostarted run,
/// the maximum runtime of its standing approval (`autostart::Grant`).
#[derive(Clone, Debug)]
pub struct Rolling {
    pub project: PathBuf,
    pub binding: Binding,
    pub qualification_id: String,
    pub config: darkbot_solana::runtime_config::RuntimeConfig,
    pub max_runtime_seconds: Option<u64>,
    /// The crashed run this start continues (its run id; `until_stopped_view::resumable`).
    pub resume: Option<String>,
    /// The standing approval (its digest) an autostarted run runs under.
    pub autostart: Option<String>,
}

/// DEC-16: whether the run's `--duration` (`P0_MEASUREMENT_DURATION`) is an unbounded horizon.
pub fn unbounded_duration(get: &dyn Fn(&str) -> Option<String>) -> bool {
    get("P0_MEASUREMENT_DURATION").is_some_and(|d| {
        matches!(
            darkbot_solana::runtime_config::horizon(&d),
            Ok(darkbot_solana::runtime_config::Horizon::UntilStopped { .. })
        )
    })
}

/// DEC-16: the bytes an unbounded run's journals hold below `run_dir` (every `.p0j` file,
/// continuation segments included): the data its rolling runway is measured by.
pub fn run_bytes(run_dir: &Path) -> u64 {
    std::fs::read_dir(run_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => run_bytes(&e.path()),
            Ok(t) if t.is_file() && e.path().extension().is_some_and(|x| x == "p0j") => {
                e.metadata().map_or(0, |m| m.len())
            }
            _ => 0,
        })
        .sum()
}

/// DEC-16: the crashed unbounded run an approved start continues (its run id), decided by the
/// controller and admitted again by the capture.
pub const ENV_RESUME: &str = "P0_UNTIL_STOPPED_RESUME";

/// The verdict of a window whose gates all held.
pub const CONTINUE: &str = "CONTINUE";
/// The stop reason of an unbounded run one of whose windows halted it.
pub const HALT_STOP: &str = "UNTIL_STOPPED_HALT";
/// The operator's stop of a run.
pub const OPERATOR_STOP: &str = "OPERATOR_STOP";

/// DEC-16: whether a window's verdict is a HALT (neither `CONTINUE` nor the verdict of a window a
/// crash interrupted whose gates held).
pub fn halts(verdict: &str) -> bool {
    verdict != CONTINUE && verdict != CRASH_RECOVERED
}

/// DEC-16: the stop reasons that end an unbounded run in order, in every mode.
pub fn ordered_reason(stop_reason: &str) -> bool {
    matches!(
        stop_reason,
        OPERATOR_STOP | super::autostart::MAX_RUNTIME | super::autostart::ENDED
    )
}

/// DEC-16: whether an unbounded run ended in order — by an ordered reason with its drain
/// complete, its ingress quiescent and nothing failed (its windows journal is complete only then).
pub fn ended_in_order(
    stop_reason: &str,
    drain_complete: bool,
    write_error: Option<&str>,
    ingress_end_error: Option<&str>,
    quiescent: &Result<(), String>,
) -> bool {
    ordered_reason(stop_reason)
        && drain_complete
        && write_error.is_none()
        && ingress_end_error.is_none()
        && quiescent.is_ok()
}

impl Rolling {
    /// DEC-16: a HALT of an autostarted run suspends its standing approval (no further autostart
    /// until the operator grants again).
    pub fn suspend_autostart(&self, verdict: &str, now_s: u64) {
        if let Some(digest) = &self.autostart {
            match super::autostart::suspend(&self.project, digest, verdict, now_s) {
                Ok(()) => crate::console::line(format!(
                    "[UNTIL_STOPPED] standing approval {digest} suspended by {verdict}"
                )),
                Err(e) => crate::console::line(format!("AUTOSTART_SUSPEND_FAILED {e}")),
            }
        }
    }

    /// DEC-16: why the run stops after the window `check` closed — its HALT, or (autostarted)
    /// the end of its standing approval — or `None` while it goes on.
    pub fn after_window(&self, check: &WindowCheck) -> Option<&'static str> {
        if check.halt_code().is_some() {
            return Some(HALT_STOP);
        }
        // An autostarted run goes on only while its standing approval is in force.
        if let Some(digest) = &self.autostart {
            if let Err(e) = super::autostart::in_force(&self.project, digest) {
                crate::console::line(format!("[UNTIL_STOPPED] {e}"));
                return Some(super::autostart::ENDED);
            }
        }
        None
    }

    /// DEC-16: whether an autostarted run has run its standing approval's maximum runtime.
    pub fn max_runtime_reached(&self, windows: &RollingWindows, now_s: u64) -> bool {
        self.max_runtime_seconds
            .is_some_and(|max| windows.ran(max, now_s))
    }
}

/// The segment a window sealed, as its writer sealed it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedSegment {
    pub file: String,
    pub root_sha256: [u8; 32],
}

/// DEC-16: the gates one window of an unbounded run re-checked (STARTUP §3: the periodic
/// re-preflight) and its verdict: `CONTINUE` or the HALT code of the first violated gate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowCheck {
    pub window: u64,
    pub started_unix_s: u64,
    pub ended_unix_s: u64,
    /// Bytes the run's journals grew by in the window (its measured data rate).
    pub bytes_written: u64,
    /// The run journals' size at the window's end.
    pub bytes_total: u64,
    pub free_bytes: Option<u64>,
    /// Bytes the minimum runway needs at that rate (the preflight's disk need).
    pub runway_need_bytes: u64,
    pub memory_pressure: Option<i32>,
    pub provider: darkbot_solana::provider::WindowUsage,
    pub evidence_id: Option<String>,
    pub evidence_valid_until_unix_s: Option<u64>,
    pub sealed: Option<SealedSegment>,
    pub verdict: String,
    pub detail: String,
}
impl Payload for WindowCheck {
    const KIND: u16 = kind::UNTIL_STOPPED_WINDOW;
}
impl WindowCheck {
    /// The HALT code of a window whose gates did not hold.
    pub fn halt_code(&self) -> Option<&str> {
        halts(&self.verdict).then_some(self.verdict.as_str())
    }
}

/// What a window measured, handed to [`check_window`].
pub struct WindowInputs<'a> {
    pub window: u64,
    pub started_unix_s: u64,
    pub ended_unix_s: u64,
    pub bytes_written: u64,
    pub bytes_total: u64,
    pub run_volume: &'a Path,
    pub environment: preflight::Environment,
    pub provider: darkbot_solana::provider::WindowUsage,
    /// The newest provider evidence valid at the window's end ([`newest_evidence`]).
    pub evidence: Option<(String, provider_evidence::ProviderEvidence)>,
    /// The segment this window's boundary sealed (path, sealed chain root).
    pub sealed: Option<(&'a Path, [u8; 32])>,
}

/// DEC-16: whether the segment at `path` is intact as its writer sealed it — readable to its
/// end, sealed, with the chain head `root` (every mode's window integrity gate).
pub fn segment_intact(path: &Path, root: [u8; 32]) -> Result<(), String> {
    let intact = (|| -> std::io::Result<bool> {
        let mut r = journal::Reader::open(path)?;
        while r.next_frame()?.is_some() {}
        Ok(r.sealed && r.root() == root)
    })();
    match intact {
        Ok(true) => Ok(()),
        other => Err(format!("{}: {other:?}", path.display())),
    }
}

/// DEC-16: the gates every window of an unbounded run re-checks, mode-neutral, in order: the
/// integrity of the segment the window sealed (re-read from disk, its sealed root); the rolling
/// runway on the run volume at the window's measured data rate; the host's memory pressure;
/// the window's provider budget (an exhausted window lost reads); the provider evidence still
/// valid at the window's end. The first violation is the run's safe HALT with its code.
pub fn check_window(
    config: &darkbot_solana::runtime_config::RuntimeConfig,
    binding: &Binding,
    capture_source: &str,
    inputs: WindowInputs,
) -> WindowCheck {
    let span = inputs
        .ended_unix_s
        .saturating_sub(inputs.started_unix_s)
        .max(1);
    let next_end = inputs
        .ended_unix_s
        .saturating_add(binding.checkpoint_seconds);
    let request = preflight::Request::from_limits(
        &config.limits,
        binding.minimum_runway_seconds,
        inputs.run_volume,
        capture_source,
    );
    let runway_need_bytes =
        preflight::disk_need_bytes(&request, inputs.bytes_written as f64 / span as f64);
    let env = &inputs.environment;
    let l = &config.limits;
    let sealed = inputs.sealed.map(|(path, root)| SealedSegment {
        file: path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string(),
        root_sha256: root,
    });
    let halt = (|| -> Result<String, (&'static str, String)> {
        if let Some((path, root)) = inputs.sealed {
            segment_intact(path, root).map_err(|e| ("UNTIL_STOPPED_SEGMENT_INTEGRITY", e))?;
        }
        match env.free_disk_bytes {
            None => return Err(("UNTIL_STOPPED_RUNWAY_UNKNOWN", "free disk unknown".into())),
            Some(free) if free < runway_need_bytes => {
                return Err((
                    "UNTIL_STOPPED_RUNWAY_EXHAUSTED",
                    format!(
                        "{free} bytes free < {runway_need_bytes} for {} s",
                        binding.minimum_runway_seconds
                    ),
                ))
            }
            Some(_) => {}
        }
        // An OS that reports no pressure level leaves this gate open, as in the preflight
        // (unknown is not BLOCKED there); the memory budgets of the writers still bound RAM.
        if let Some(level) = env
            .memory_pressure
            .filter(|l| *l >= crate::storage::budget::PRESSURE_CRITICAL)
        {
            return Err((
                "UNTIL_STOPPED_MEMORY_CRITICAL",
                format!("memory pressure level {level}"),
            ));
        }
        if inputs.provider.requests >= l.max_requests
            || inputs.provider.bytes >= l.max_total_mib * 1024 * 1024
        {
            return Err((
                "UNTIL_STOPPED_PROVIDER_BUDGET_EXHAUSTED",
                format!(
                    "{} calls / {} bytes in the window (caps {} / {} MiB)",
                    inputs.provider.requests,
                    inputs.provider.bytes,
                    l.max_requests,
                    l.max_total_mib
                ),
            ));
        }
        let Some((id, evidence)) = &inputs.evidence else {
            return Err((
                "UNTIL_STOPPED_PROVIDER_EVIDENCE_MISSING",
                "no valid provider evidence".into(),
            ));
        };
        // Valid through the window to come, with the budget its admission demanded.
        let (part, budget, until) = provider(config, Some(evidence), next_end, binding);
        if part.state != PartState::Pass {
            let code = if until.is_some_and(|u| u < next_end) {
                "UNTIL_STOPPED_PROVIDER_EVIDENCE_EXPIRED"
            } else {
                "UNTIL_STOPPED_PROVIDER_EVIDENCE_UNQUALIFIED"
            };
            return Err((code, format!("{id}: {}", part.reason)));
        }
        if budget.state != PartState::Pass {
            return Err((
                "UNTIL_STOPPED_PROVIDER_BUDGET_UNQUALIFIED",
                format!("{id}: {}", budget.reason),
            ));
        }
        Ok(format!(
            "runway {runway_need_bytes} of {} bytes; evidence {id}",
            env.free_disk_bytes.unwrap_or_default()
        ))
    })();
    let (_, _, evidence_valid_until_unix_s) = provider(
        config,
        inputs.evidence.as_ref().map(|(_, e)| e),
        next_end,
        binding,
    );
    let (verdict, detail) = match halt {
        Ok(detail) => (CONTINUE.to_string(), detail),
        Err((code, detail)) => (code.to_string(), detail),
    };
    WindowCheck {
        window: inputs.window,
        started_unix_s: inputs.started_unix_s,
        ended_unix_s: inputs.ended_unix_s,
        bytes_written: inputs.bytes_written,
        bytes_total: inputs.bytes_total,
        free_bytes: env.free_disk_bytes,
        runway_need_bytes,
        memory_pressure: env.memory_pressure,
        provider: inputs.provider,
        evidence_id: inputs.evidence.as_ref().map(|(id, _)| id.clone()),
        evidence_valid_until_unix_s,
        sealed,
        verdict,
        detail,
    }
}

/// The newest provider evidence in the controller's store that qualifies this run (evidence and
/// budget, `binding`) through `at` (an operator imports fresh evidence while an unbounded run
/// goes on).
pub fn newest_evidence(
    project: &Path,
    config: &darkbot_solana::runtime_config::RuntimeConfig,
    binding: &Binding,
    at: u64,
) -> Option<(String, provider_evidence::ProviderEvidence)> {
    let dir = std::fs::read_dir(project.join(provider_evidence::STORE)).ok()?;
    let mut best: Option<(u64, String, provider_evidence::ProviderEvidence)> = None;
    for entry in dir.flatten() {
        let name = entry.file_name();
        let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".p0j")) else {
            continue;
        };
        let Ok(path) = provider_evidence::resolve(project, id) else {
            continue;
        };
        let Ok(evidence) = journal::read_typed_value::<provider_evidence::ProviderEvidence>(&path)
        else {
            continue;
        };
        let (part, budget, until) = provider(config, Some(&evidence), at, binding);
        if let (PartState::Pass, PartState::Pass, Some(until)) = (part.state, budget.state, until) {
            if best.as_ref().is_none_or(|(b, _, _)| until > *b) {
                best = Some((until, id.to_string(), evidence));
            }
        }
    }
    best.map(|(_, id, e)| (id, e))
}

/// The verdict recorded for a window a crash interrupted whose gates held at the restart.
pub const CRASH_RECOVERED: &str = "UNTIL_STOPPED_CRASH_RECOVERED";
/// The refusal of a window after the run halted (a HALT is final: a new run is a new start).
pub const HALTED: &str = "UNTIL_STOPPED_HALTED";
/// The journal of an unbounded run's windows, in its run directory.
pub const WINDOWS_JOURNAL: &str = "windows.p0j";

/// The first frame of an unbounded run's windows journal: what the run was admitted as.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    pub binding: Binding,
    pub qualification_id: String,
    pub started_unix_s: u64,
}
impl Payload for RunRecord {
    const KIND: u16 = kind::UNTIL_STOPPED_RUN;
}

/// What a window boundary measured (the runtime rotated its segmented journals and ended its
/// provider window just before).
pub struct Measured<'a> {
    pub now: u64,
    /// The run journals' size now.
    pub bytes: u64,
    pub run_volume: &'a Path,
    /// The host facts (`preflight::environment(run_volume)` in production).
    pub environment: preflight::Environment,
    pub provider: darkbot_solana::provider::WindowUsage,
    /// The newest valid provider evidence ([`newest_evidence`]).
    pub evidence: Option<(String, provider_evidence::ProviderEvidence)>,
    /// The segment this boundary sealed (path, sealed chain root).
    pub sealed: Option<(&'a Path, [u8; 32])>,
}

fn coded(code: &'static str) -> std::io::Error {
    journal::coded(code)
}

/// DEC-16: the rolling windows of an unbounded run, the one mode-neutral driver every mode's
/// runtime calls: `due` tells a window's end; at it the runtime rotates its segmented journals
/// (`SegmentedWriter::rotate_now`) and ends its provider window (`Budget::roll_window`), and
/// `close` re-checks the gates ([`check_window`]) and journals the verdict durably. A verdict
/// other than `CONTINUE` is the run's safe HALT and final: the driver refuses every further
/// window (`HALTED`). After a crash `resume` continues only the same admitted run that never
/// halted — the windows journal by its SK-07 recovery — and re-checks every gate for the
/// window the crash interrupted before the next one opens.
pub struct RollingWindows {
    binding: Binding,
    journal: segments::SegmentedWriter,
    window: u64,
    started_unix_s: u64,
    run_started_unix_s: u64,
    bytes_at_start: u64,
    halted: Option<String>,
    /// How the chain continued, for the window a crash interrupted (closed first after resume).
    recovery: Option<String>,
}

impl RollingWindows {
    /// Opens the first window of a new run admitted as `binding` with `qualification_id`.
    pub fn start(
        run_dir: &Path,
        binding: Binding,
        qualification_id: &str,
        now: u64,
        bytes: u64,
    ) -> std::io::Result<Self> {
        let mut journal = segments::SegmentedWriter::create(&run_dir.join(WINDOWS_JOURNAL), 0)?;
        journal.append_typed(
            Self::provenance(0),
            &RunRecord {
                binding: binding.clone(),
                qualification_id: qualification_id.into(),
                started_unix_s: now,
            },
        )?;
        journal.sync()?;
        Ok(Self {
            binding,
            journal,
            window: 1,
            started_unix_s: now,
            run_started_unix_s: now,
            bytes_at_start: bytes,
            halted: None,
            recovery: None,
        })
    }

    /// Continues a run after a crash: only the same admitted run (its run record's binding and
    /// qualification), only one that never halted. The windows journal continues by its
    /// recovery link; the window the crash interrupted (from the last recorded end to now) is
    /// checked like any window — `CRASH_RECOVERED` when its gates hold, else its HALT code.
    pub fn resume(
        run_dir: &Path,
        binding: Binding,
        qualification_id: &str,
        config: &darkbot_solana::runtime_config::RuntimeConfig,
        capture_source: &str,
        m: Measured,
    ) -> std::io::Result<(Self, WindowCheck)> {
        let first = run_dir.join(WINDOWS_JOURNAL);
        let (mut run, mut last): (Option<RunRecord>, Option<WindowCheck>) = (None, None);
        let mut r = segments::ChainReader::open(&first)?;
        while let Some(f) = r.next_frame()? {
            match f.kind {
                kind::UNTIL_STOPPED_RUN if run.is_none() && last.is_none() => {
                    run = Some(f.typed()?)
                }
                kind::UNTIL_STOPPED_WINDOW if run.is_some() => last = Some(f.typed()?),
                _ => return Err(coded("UNTIL_STOPPED_JOURNAL_INVALID")),
            }
        }
        let run = run.ok_or_else(|| coded("UNTIL_STOPPED_JOURNAL_INVALID"))?;
        if run.binding != binding || run.qualification_id != qualification_id {
            return Err(coded("UNTIL_STOPPED_RESUME_DRIFT"));
        }
        if let Some(w) = &last {
            if halts(&w.verdict) {
                return Err(coded(HALTED));
            }
        }
        let (journal, how) = segments::SegmentedWriter::recover(&first, 0)?;
        let mut this = Self {
            binding,
            journal,
            window: last.as_ref().map_or(1, |w| w.window + 1),
            started_unix_s: last.as_ref().map_or(run.started_unix_s, |w| w.ended_unix_s),
            run_started_unix_s: run.started_unix_s,
            bytes_at_start: last.as_ref().map_or(0, |w| w.bytes_total),
            halted: None,
            recovery: Some(format!("{how:?}")),
        };
        let check = this.close(config, capture_source, m)?;
        Ok((this, check))
    }

    fn provenance(window: u64) -> Provenance {
        Provenance {
            source: journal::source::UNTIL_STOPPED,
            source_seq: window,
            ..Default::default()
        }
    }

    /// The window now open.
    pub fn window(&self) -> u64 {
        self.window
    }

    /// The HALT code once a window halted the run.
    pub fn halted(&self) -> Option<&str> {
        self.halted.as_deref()
    }

    /// Whether the run, started at its run record, has run `max_seconds` at `now`.
    pub fn ran(&self, max_seconds: u64, now: u64) -> bool {
        now >= self.run_started_unix_s.saturating_add(max_seconds)
    }

    /// Whether the open window ended at `now`.
    pub fn due(&self, now: u64) -> bool {
        now >= self
            .started_unix_s
            .saturating_add(self.binding.checkpoint_seconds)
    }

    /// Ends the open window at `m.now`: re-checks its gates, journals the verdict durably and
    /// opens the next window — or, on a HALT, refuses every further one.
    pub fn close(
        &mut self,
        config: &darkbot_solana::runtime_config::RuntimeConfig,
        capture_source: &str,
        m: Measured,
    ) -> std::io::Result<WindowCheck> {
        if self.halted.is_some() {
            return Err(coded(HALTED));
        }
        let mut check = check_window(
            config,
            &self.binding,
            capture_source,
            WindowInputs {
                window: self.window,
                started_unix_s: self.started_unix_s,
                ended_unix_s: m.now,
                bytes_written: m.bytes.saturating_sub(self.bytes_at_start),
                bytes_total: m.bytes,
                run_volume: m.run_volume,
                environment: m.environment,
                provider: m.provider,
                evidence: m.evidence,
                sealed: m.sealed,
            },
        );
        // A resumed run journals its interrupted window under its recovery verdict.
        if let Some(how) = self.recovery.take() {
            if check.verdict == CONTINUE {
                check.verdict = CRASH_RECOVERED.into();
            }
            check.detail = format!("{how}; {}", check.detail);
        }
        self.journal
            .append_typed(Self::provenance(self.window), &check)?;
        self.journal.sync()?;
        if halts(&check.verdict) {
            self.halted = Some(check.verdict.clone());
        }
        self.window += 1;
        self.started_unix_s = m.now;
        self.bytes_at_start = m.bytes;
        Ok(check)
    }

    /// Seals the windows journal at the run's end; complete only for a run that ended in order
    /// (an operator stop, or the maximum runtime of its standing approval) and never halted.
    pub fn finish(mut self, ended_in_order: bool) -> std::io::Result<()> {
        let complete = ended_in_order && self.halted.is_none();
        self.journal.seal(complete).map(|_| ())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::analytics::measurement::{RunHorizon, UNTIL_STOPPED_GATED};
    use provider_evidence::{ProviderEvidence, ProviderProof};

    pub(crate) fn binding(checkpoint_seconds: u64, minimum_runway_seconds: u64) -> Binding {
        Binding {
            mode: "market".into(),
            identity: AuditedIdentity {
                source_head: Some("head".into()),
                source_dirty_sha256: Some("dirty".into()),
                runtime_config_sha256: "config".into(),
                journal_version: journal::VERSION,
                discriminator_table_sha256: "table".into(),
                live_enabled: false,
                submission_enabled: false,
            },
            binary_sha256: "binary".into(),
            checkpoint_seconds,
            minimum_runway_seconds,
        }
    }
    fn pass(reason: &str) -> Part {
        Part::of(PartState::Pass, reason)
    }
    pub(crate) fn qualified(b: &Binding) -> Qualification {
        Qualification {
            schema: SCHEMA,
            binding: b.clone(),
            runtime: pass("rolling"),
            sk07: pass("sk07"),
            capacity: pass("runway"),
            capacity_need_bytes: Some(1),
            provider_budget: pass("budget"),
            provider_evidence: pass("evidence"),
            evidence_id: Some("0123456789abcdef".into()),
            evidence_valid_until_unix_s: Some(2_000),
            qualified_at_unix_s: 1_000,
        }
    }
    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "p0_until_stopped_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// DEC-16 (review M1): an unbounded `--duration` is recognised (the Shadow runtime refuses it).
    #[test]
    fn dec16_an_unbounded_duration_is_recognised() {
        let with = |d: &'static str| {
            move |k: &str| (k == "P0_MEASUREMENT_DURATION").then(|| d.to_string())
        };
        assert!(unbounded_duration(&with("until-stopped:1h:1d")));
        assert!(!unbounded_duration(&with("4h")));
        assert!(!unbounded_duration(&with("until-stopped")));
        assert!(!unbounded_duration(&|_| None));
    }

    /// DEC-16: `RunHorizon::admit` admits an unbounded run only with its complete
    /// qualification bound to exactly this run; every gap is named.
    #[test]
    fn dec16_admit_requires_a_complete_qualification_bound_to_the_run() {
        let h = RunHorizon::until_stopped("1h", "24h").unwrap();
        let b = binding(3_600, 86_400);
        let q = qualified(&b);
        assert_eq!(h.admit(Some(&q), &b, 1_500), Ok(()));
        let gated = |q: &Qualification, current: &Binding, now: u64| {
            let e = h.admit(Some(q), current, now).unwrap_err();
            assert!(e.starts_with(UNTIL_STOPPED_GATED), "{e}");
            e
        };
        for name in [
            "runtime",
            "sk07",
            "capacity",
            "provider_budget",
            "provider_evidence",
        ] {
            for state in [PartState::NotRun, PartState::Fail] {
                let mut open = q.clone();
                let part = Part::of(state, "why");
                match name {
                    "runtime" => open.runtime = part,
                    "sk07" => open.sk07 = part,
                    "capacity" => open.capacity = part,
                    "provider_budget" => open.provider_budget = part,
                    _ => open.provider_evidence = part,
                }
                let e = gated(&open, &b, 1_500);
                assert!(
                    e.contains(&format!("{name} {}: why", state.as_str())),
                    "{e}"
                );
            }
        }
        let drifted = |f: &dyn Fn(&mut Binding)| {
            let mut current = b.clone();
            f(&mut current);
            gated(&q, &current, 1_500)
        };
        // Another mode: drifted — and econ4h, whose Shadow owner has no windows yet, gated as such.
        let econ = drifted(&|c| c.mode = "econ4h".into());
        assert!(
            econ.contains("drift: mode") && econ.contains("DEC-16 stage 3"),
            "{econ}"
        );
        assert!(
            drifted(&|c| c.identity.runtime_config_sha256 = "other".into())
                .contains("drift: runtime_config")
        );
        assert!(drifted(&|c| c.identity.live_enabled = true).contains("drift: safety_flags"));
        assert!(drifted(&|c| c.binary_sha256 = "other".into()).contains("drift: binary"));
        assert!(drifted(&|c| c.identity.source_head = None).contains("drift: source"));
        // The run's horizon and the qualification's must agree.
        let shorter = RunHorizon::until_stopped("1h", "12h").unwrap();
        let mut current = b.clone();
        current.minimum_runway_seconds = 43_200;
        let e = shorter.admit(Some(&q), &current, 1_500).unwrap_err();
        assert!(e.contains("drift: horizon"), "{e}");
        assert!(h
            .admit(Some(&q), &current, 1_500)
            .unwrap_err()
            .contains("another horizon"));
        assert_eq!(
            h.admit(Some(&q), &b, 2_000),
            Ok(()),
            "fresh through its last second"
        );
        assert!(gated(&q, &b, 2_001).contains("provider evidence expired"));
        let mut old = q.clone();
        old.schema = 0;
        assert!(gated(&old, &b, 1_500).contains("qualification schema 0"));
    }

    /// DEC-16: the store names a qualification by its bytes; an unknown, malformed or changed
    /// id is refused.
    #[test]
    fn dec16_the_store_names_a_qualification_by_its_bytes() {
        let d = temp("store");
        let q = qualified(&binding(3_600, 86_400));
        let id = put(&d, &q).unwrap();
        assert!(provider_evidence::is_id(&id));
        assert_eq!(get(&d, &id), Ok(q.clone()));
        // Each stored record is its own (a journal header is unique): both resolve.
        let again = put(&d, &q).unwrap();
        assert_ne!(again, id);
        assert_eq!(get(&d, &again), Ok(q.clone()));
        assert_eq!(
            get(&d, "0123456789abcdef").unwrap_err(),
            "UNTIL_STOPPED_QUALIFICATION_UNKNOWN"
        );
        assert!(get(&d, "/etc/q.p0j")
            .unwrap_err()
            .starts_with("UNTIL_STOPPED_ID_INVALID"));
        let stored = d.join(STORE).join(format!("{id}.p0j"));
        std::fs::write(&stored, b"other").unwrap();
        assert_eq!(
            get(&d, &id).unwrap_err(),
            "UNTIL_STOPPED_QUALIFICATION_CHANGED"
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    /// DEC-16: the SK-07 part runs the real writer: rotation, a torn crash, recovery and the
    /// chain's verification; its directory is removed.
    #[test]
    fn dec16_the_sk07_selftest_passes_on_the_real_writer() {
        let d = temp("sk07");
        let part = sk07_selftest(&d.join("sk07"));
        assert_eq!(part.state, PartState::Pass, "{}", part.reason);
        assert!(!d.join("sk07").exists());
        std::fs::remove_dir_all(d).unwrap();
    }

    /// DEC-16: the capacity part maps the preflight's checks; its provider and verifier checks
    /// are parts of their own.
    #[test]
    fn dec16_capacity_maps_the_preflight_checks() {
        use preflight::Verdict::*;
        let report = |checks: &[(&str, preflight::Verdict)]| preflight::CapacityReport {
            request: preflight::Request::from_limits(
                &darkbot_solana::runtime_config::RuntimeConfig::default().limits,
                86_400,
                Path::new("/r"),
                "logs",
            ),
            basis: preflight::RateBasis::default(),
            strict_basis: None,
            probe: Err("not run".into()),
            environment: preflight::Environment::default(),
            disk_need_bytes: Some(42),
            checks: checks
                .iter()
                .map(|(name, verdict)| preflight::Check {
                    name: (*name).into(),
                    verdict: *verdict,
                    detail: "d".into(),
                })
                .collect(),
            verdict: CapacityUnverified,
            start_allowed: false,
        };
        let all = [
            ("data_rate", Qualified),
            ("disk", Qualified),
            ("provider", NotRun),
            ("verification", NotApplicable),
        ];
        let p = capacity(&report(&all));
        assert!(
            p.state == PartState::Pass && p.reason.contains("42"),
            "{p:?}"
        );
        let mut unmeasured = all;
        unmeasured[0].1 = NotRun;
        assert_eq!(capacity(&report(&unmeasured)).state, PartState::NotRun);
        let mut full = all;
        full[1].1 = Blocked;
        let p = capacity(&report(&full));
        assert!(
            p.state == PartState::Fail && p.reason.starts_with("disk"),
            "{p:?}"
        );
    }

    /// DEC-16: the provider evidence must qualify every usable endpoint and stay fresh; its
    /// available calls must cover every window of the runway (one window's cap is no continuous
    /// run). Without evidence both parts are NOT_RUN.
    #[test]
    fn dec16_provider_parts_need_fresh_evidence_that_covers_the_runway() {
        let c = darkbot_solana::runtime_config::RuntimeConfig {
            rpc_endpoints: vec!["https://rpc.invalid".into()],
            ..Default::default()
        };
        let run = binding(3_600, 86_400);
        let (e, b, until) = provider(&c, None, 1_000, &run);
        assert_eq!(
            (e.state, b.state, until),
            (PartState::NotRun, PartState::NotRun, None)
        );
        let binding = journal::hex(&solana_sdk::hash::hash(b"https://rpc.invalid").to_bytes());
        let proof = |rps: f64, calls: u64, measured: f64| ProviderEvidence {
            providers: vec![ProviderProof {
                policy: "FREE_ONLY".into(),
                passed: true,
                provider_binding: binding.clone(),
                measured_at_unix_s: measured,
                evidence_sha256: Some("a".repeat(64)),
                allowed_rps: rps,
                available_calls: calls,
            }],
            wss_provider_binding: None,
            wss_free_only: true,
            archive_qualified: false,
            source_sha256: "s".into(),
        };
        let l = &c.limits;
        let runway_calls = l.max_requests * 24;
        let enough = proof(l.rpc_rps as f64, runway_calls, 900.0);
        let (e, b, until) = provider(&c, Some(&enough), 1_000, &run);
        assert_eq!(
            (e.state, b.state),
            (PartState::Pass, PartState::Pass),
            "{e:?} {b:?}"
        );
        assert_eq!(until, Some(900 + l.fe_qualification_age_seconds));
        // One window's calls, or one call less than the runway, are not enough.
        for calls in [l.max_requests, runway_calls - 1] {
            let (_, b, _) = provider(
                &c,
                Some(&proof(l.rpc_rps as f64, calls, 900.0)),
                1_000,
                &run,
            );
            assert_eq!(b.state, PartState::Fail, "{calls}: {b:?}");
        }
        let (_, b, _) = provider(
            &c,
            Some(&proof(l.rpc_rps as f64 - 1.0, runway_calls, 900.0)),
            1_000,
            &run,
        );
        assert_eq!(b.state, PartState::Fail, "{b:?}");
        let stale = 1_001 + l.fe_qualification_age_seconds;
        let (e, _, _) = provider(&c, Some(&enough), stale, &run);
        assert_eq!(e.state, PartState::Fail, "{e:?}");
    }

    /// DEC-16: a qualification of a mode whose runtime has no rolling windows stays gated on
    /// that part, whatever else it holds; without measured runs and evidence those stay NOT_RUN.
    #[test]
    fn dec16_qualify_records_every_part_and_stays_gated_without_rolling_windows() {
        let d = temp("qualify");
        let c = darkbot_solana::runtime_config::RuntimeConfig::default();
        let mut b = binding(3_600, 86_400);
        b.mode = "econ4h".into();
        let q = qualify(
            b.clone(),
            &c,
            "logs",
            &Inputs {
                project: &d,
                runs_root: d.join("data/runs"),
                evidence_id: None,
                now: 1_000,
            },
        )
        .unwrap();
        assert_eq!(q.runtime.state, PartState::NotRun, "{:?}", q.runtime);
        assert_eq!(q.sk07.state, PartState::Pass, "{:?}", q.sk07);
        assert_eq!(q.capacity.state, PartState::NotRun, "{:?}", q.capacity);
        assert_eq!(q.provider_evidence.state, PartState::NotRun);
        let h = RunHorizon::until_stopped("1h", "24h").unwrap();
        let e = h.admit(Some(&q), &b, 1_000).unwrap_err();
        for part in [
            "runtime NOT_RUN",
            "capacity NOT_RUN",
            "provider_evidence NOT_RUN",
        ] {
            assert!(e.contains(part), "{e}");
        }
        assert!(!e.contains("sk07"), "{e}");
        // The self-test and the probe ran on the run volume and left nothing.
        assert!(!d
            .join("data/runs")
            .join(format!(".until-stopped-{}", std::process::id()))
            .exists());
        std::fs::remove_dir_all(d).unwrap();
    }

    /// FREE_ONLY evidence for the configured endpoint that covers `run`'s runway.
    pub(crate) fn evidence_for(
        c: &darkbot_solana::runtime_config::RuntimeConfig,
        run: &Binding,
        measured: f64,
    ) -> ProviderEvidence {
        let binding =
            journal::hex(&solana_sdk::hash::hash(c.rpc_endpoints[0].as_bytes()).to_bytes());
        let windows = run
            .minimum_runway_seconds
            .div_ceil(run.checkpoint_seconds)
            .max(1);
        ProviderEvidence {
            providers: vec![ProviderProof {
                policy: "FREE_ONLY".into(),
                passed: true,
                provider_binding: binding,
                measured_at_unix_s: measured,
                evidence_sha256: Some("a".repeat(64)),
                allowed_rps: c.limits.rpc_rps as f64,
                available_calls: c.limits.max_requests * windows,
            }],
            wss_provider_binding: None,
            wss_free_only: true,
            archive_qualified: false,
            source_sha256: "s".into(),
        }
    }

    /// DEC-16: every window re-checks its gates, mode-neutral: the sealed segment's integrity,
    /// the runway at the window's data rate, memory pressure, the window's provider budget and
    /// the provider evidence. All held: CONTINUE; the first violation: its HALT code.
    #[test]
    fn dec16_a_window_continues_only_while_every_gate_holds() {
        use crate::market::observer::rows::MarketCoverage;
        let d = temp("window");
        let c = darkbot_solana::runtime_config::RuntimeConfig {
            rpc_endpoints: vec!["https://rpc.invalid".into()],
            ..Default::default()
        };
        let b = binding(3_600, 86_400);
        // A real window boundary: one sealed segment and its root.
        let first = d.join("market.p0j");
        let mut w = segments::SegmentedWriter::create(&first, 0).unwrap();
        w.append_typed(
            Provenance::default(),
            &MarketCoverage::SlotFailed {
                slot: 1,
                error: String::new(),
            },
        )
        .unwrap();
        let sealed = w.rotate_now().unwrap();
        let sealed_path = segments::segment_path(&first, sealed.index);
        let env = preflight::Environment {
            free_disk_bytes: Some(u64::MAX / 4),
            memory_pressure: Some(1),
            ..Default::default()
        };
        let inputs = |env: preflight::Environment,
                      provider: darkbot_solana::provider::WindowUsage,
                      evidence: Option<ProviderEvidence>,
                      root: [u8; 32]| WindowInputs {
            window: 3,
            started_unix_s: 1_000,
            ended_unix_s: 4_600,
            bytes_written: 3_600 * 1_000,
            bytes_total: 9 * 3_600 * 1_000,
            run_volume: &d,
            environment: env,
            provider,
            evidence: evidence.map(|e| ("0123456789abcdef".to_string(), e)),
            sealed: Some((&sealed_path, root)),
        };
        let used = darkbot_solana::provider::WindowUsage {
            requests: 5,
            bytes: 50,
        };
        let ok = evidence_for(&c, &b, 4_000.0);
        let check = |i| check_window(&c, &b, "logs", i);
        let w0 = check(inputs(
            env.clone(),
            used,
            Some(ok.clone()),
            sealed.root_sha256,
        ));
        assert_eq!(w0.verdict, CONTINUE, "{}", w0.detail);
        assert_eq!(
            w0.runway_need_bytes,
            preflight::disk_need_bytes(
                &preflight::Request::from_limits(&c.limits, 86_400, &d, "logs"),
                1_000.0
            )
        );
        assert_eq!(
            w0.evidence_valid_until_unix_s,
            Some(4_000 + c.limits.fe_qualification_age_seconds)
        );
        let halt = |i| check(i).verdict;
        assert_eq!(
            halt(inputs(env.clone(), used, Some(ok.clone()), [7; 32])),
            "UNTIL_STOPPED_SEGMENT_INTEGRITY"
        );
        let low = preflight::Environment {
            free_disk_bytes: Some(w0.runway_need_bytes - 1),
            ..env.clone()
        };
        assert_eq!(
            halt(inputs(low, used, Some(ok.clone()), sealed.root_sha256)),
            "UNTIL_STOPPED_RUNWAY_EXHAUSTED"
        );
        let unknown = preflight::Environment {
            free_disk_bytes: None,
            ..env.clone()
        };
        assert_eq!(
            halt(inputs(unknown, used, Some(ok.clone()), sealed.root_sha256)),
            "UNTIL_STOPPED_RUNWAY_UNKNOWN"
        );
        let pressed = preflight::Environment {
            memory_pressure: Some(crate::storage::budget::PRESSURE_CRITICAL),
            ..env.clone()
        };
        assert_eq!(
            halt(inputs(pressed, used, Some(ok.clone()), sealed.root_sha256)),
            "UNTIL_STOPPED_MEMORY_CRITICAL"
        );
        let spent = darkbot_solana::provider::WindowUsage {
            requests: c.limits.max_requests,
            bytes: 0,
        };
        assert_eq!(
            halt(inputs(
                env.clone(),
                spent,
                Some(ok.clone()),
                sealed.root_sha256
            )),
            "UNTIL_STOPPED_PROVIDER_BUDGET_EXHAUSTED"
        );
        assert_eq!(
            halt(inputs(env.clone(), used, None, sealed.root_sha256)),
            "UNTIL_STOPPED_PROVIDER_EVIDENCE_MISSING"
        );
        // Valid at the window's end but not through the window to come (ends 8,200).
        let mut one_window = ok.clone();
        one_window.providers[0].available_calls = c.limits.max_requests;
        assert_eq!(
            halt(inputs(
                env.clone(),
                used,
                Some(one_window),
                sealed.root_sha256
            )),
            "UNTIL_STOPPED_PROVIDER_BUDGET_UNQUALIFIED"
        );
        let mut short_age = c.clone();
        short_age.limits.fe_qualification_age_seconds = 4_000;
        // Measured at 4,000: fresh through 8,000 — not through the window to come (8,200).
        let expired = check_window(
            &short_age,
            &b,
            "logs",
            inputs(env.clone(), used, Some(ok.clone()), sealed.root_sha256),
        );
        assert_eq!(
            expired.verdict, "UNTIL_STOPPED_PROVIDER_EVIDENCE_EXPIRED",
            "{}",
            expired.detail
        );
        let mut free = ok.clone();
        free.providers[0].policy = "PAID".into();
        assert_eq!(
            halt(inputs(env.clone(), used, Some(free), sealed.root_sha256)),
            "UNTIL_STOPPED_PROVIDER_EVIDENCE_UNQUALIFIED"
        );
        // A changed sealed segment on disk.
        let mut bytes = std::fs::read(&sealed_path).unwrap();
        let at = bytes.len() / 2;
        bytes[at] ^= 0xff;
        std::fs::write(&sealed_path, bytes).unwrap();
        assert_eq!(
            halt(inputs(env.clone(), used, Some(ok), sealed.root_sha256)),
            "UNTIL_STOPPED_SEGMENT_INTEGRITY"
        );
        drop(w);
        std::fs::remove_dir_all(d).unwrap();
    }

    /// DEC-16: a running window takes the newest provider evidence the operator imported that
    /// qualifies the run (evidence and runway budget) through the given time; an expired, a
    /// short or an unreadable record is never taken.
    #[test]
    fn dec16_a_window_takes_the_newest_qualifying_provider_evidence() {
        let d = temp("newest");
        let c = darkbot_solana::runtime_config::RuntimeConfig {
            rpc_endpoints: vec!["https://rpc.invalid".into()],
            ..Default::default()
        };
        let run = binding(3_600, 86_400);
        assert!(newest_evidence(&d, &c, &run, 5_000).is_none());
        let import = |e: &ProviderEvidence, tag: &str| {
            let file = d.join(format!("e{tag}.p0j"));
            journal::write_typed_value(&file, 0, e).unwrap();
            provider_evidence::import(&d, &file).unwrap()
        };
        let older = import(&evidence_for(&c, &run, 1_000.0), "older");
        let newer = import(&evidence_for(&c, &run, 3_000.0), "newer");
        let mut short = evidence_for(&c, &run, 4_000.0);
        short.providers[0].available_calls = c.limits.max_requests;
        import(&short, "short");
        let age = c.limits.fe_qualification_age_seconds;
        assert_eq!(newest_evidence(&d, &c, &run, 5_000).unwrap().0, newer);
        assert_eq!(newest_evidence(&d, &c, &run, 1_001 + age).unwrap().0, newer);
        assert!(newest_evidence(&d, &c, &run, 3_001 + age).is_none());
        std::fs::write(
            d.join(provider_evidence::STORE)
                .join(format!("{newer}.p0j")),
            b"x",
        )
        .unwrap();
        assert_eq!(newest_evidence(&d, &c, &run, 5_000).unwrap().0, older);
        std::fs::remove_dir_all(d).unwrap();
    }

    /// A journaled window: index, verdict, start, end, bytes written.
    type Seen = (u64, String, u64, u64, u64);

    struct Run {
        dir: PathBuf,
        config: darkbot_solana::runtime_config::RuntimeConfig,
        binding: Binding,
    }
    impl Run {
        fn new(tag: &str) -> Self {
            Self {
                dir: temp(tag),
                config: darkbot_solana::runtime_config::RuntimeConfig {
                    rpc_endpoints: vec!["https://rpc.invalid".into()],
                    ..Default::default()
                },
                binding: binding(60, 3_600),
            }
        }
        fn at(&self, now: u64, bytes: u64, requests: u64, free: u64) -> Measured<'_> {
            Measured {
                now,
                bytes,
                run_volume: &self.dir,
                environment: preflight::Environment {
                    free_disk_bytes: Some(free),
                    memory_pressure: Some(1),
                    ..Default::default()
                },
                provider: darkbot_solana::provider::WindowUsage { requests, bytes: 0 },
                evidence: Some((
                    "0123456789abcdef".into(),
                    evidence_for(&self.config, &self.binding, 1_000.0),
                )),
                sealed: None,
            }
        }
        fn windows(&self) -> (Vec<Seen>, segments::ChainSummary) {
            let journal = self.dir.join(WINDOWS_JOURNAL);
            let mut r = segments::ChainReader::open(&journal).unwrap();
            let mut seen = Vec::new();
            while let Some(f) = r.next_frame().unwrap() {
                if f.kind == kind::UNTIL_STOPPED_WINDOW {
                    let w: WindowCheck = f.typed().unwrap();
                    seen.push((
                        w.window,
                        w.verdict,
                        w.started_unix_s,
                        w.ended_unix_s,
                        w.bytes_written,
                    ));
                }
            }
            (seen, segments::verify_chain(&journal).unwrap())
        }
    }
    impl Drop for Run {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    const FREE: u64 = u64::MAX / 4;

    /// DEC-16: the windows of an unbounded run are due at their end and journaled durably; after
    /// a crash the same run resumes — the journal continues by its SK-07 recovery, the window
    /// the crash interrupted is checked like any window (CRASH_RECOVERED, its data rate since
    /// the last recorded window) — and the run is never complete afterwards.
    #[test]
    fn dec16_rolling_windows_resume_the_same_run_after_a_crash() {
        let r = Run::new("windows_resume");
        let mut w = RollingWindows::start(&r.dir, r.binding.clone(), "q1", 1_000, 0).unwrap();
        assert!(!w.due(1_059) && w.due(1_060));
        let first = w
            .close(&r.config, "logs", r.at(1_060, 6_000, 1, FREE))
            .unwrap();
        assert_eq!(first.verdict, CONTINUE, "{}", first.detail);
        drop(w); // crash: never sealed
        let (mut w, interrupted) = RollingWindows::resume(
            &r.dir,
            r.binding.clone(),
            "q1",
            &r.config,
            "logs",
            r.at(1_300, 9_000, 0, FREE),
        )
        .unwrap();
        assert_eq!(
            interrupted.verdict, CRASH_RECOVERED,
            "{}",
            interrupted.detail
        );
        assert!(
            interrupted.detail.starts_with("Crash"),
            "{}",
            interrupted.detail
        );
        let after = w
            .close(&r.config, "logs", r.at(1_360, 9_500, 1, FREE))
            .unwrap();
        assert_eq!(after.verdict, CONTINUE);
        w.finish(true).unwrap();
        let (seen, chain) = r.windows();
        assert_eq!(
            seen,
            [
                (1, CONTINUE.into(), 1_000, 1_060, 6_000),
                (2, CRASH_RECOVERED.into(), 1_060, 1_300, 3_000),
                (3, CONTINUE.into(), 1_300, 1_360, 500),
            ]
        );
        assert!(
            chain.sealed && !chain.complete && chain.recovered == 1,
            "{chain:?}"
        );
        // A run that ended in order is never resumed.
        assert!(RollingWindows::resume(
            &r.dir,
            r.binding.clone(),
            "q1",
            &r.config,
            "logs",
            r.at(1_400, 9_500, 0, FREE)
        )
        .is_err());
    }

    /// DEC-16: a HALT is final — no further window, no resume after a crash.
    #[test]
    fn dec16_a_halt_is_final_even_across_a_crash() {
        let r = Run::new("windows_halt");
        let mut w = RollingWindows::start(&r.dir, r.binding.clone(), "q1", 1_000, 0).unwrap();
        let halted = w
            .close(
                &r.config,
                "logs",
                r.at(1_060, 6_000, r.config.limits.max_requests, FREE),
            )
            .unwrap();
        assert_eq!(halted.verdict, "UNTIL_STOPPED_PROVIDER_BUDGET_EXHAUSTED");
        assert_eq!(w.halted(), Some("UNTIL_STOPPED_PROVIDER_BUDGET_EXHAUSTED"));
        let refused = w
            .close(&r.config, "logs", r.at(1_120, 6_000, 0, FREE))
            .unwrap_err();
        assert_eq!(refused.to_string(), HALTED);
        drop(w); // crash before the HALT was sealed
        let refused = RollingWindows::resume(
            &r.dir,
            r.binding.clone(),
            "q1",
            &r.config,
            "logs",
            r.at(1_300, 6_000, 0, FREE),
        )
        .err()
        .unwrap();
        assert_eq!(refused.to_string(), HALTED);
        assert_eq!(
            r.windows().0.len(),
            1,
            "nothing was journaled after the HALT"
        );
    }

    /// DEC-16: only the admitted run resumes (its run record's binding and qualification), and
    /// a resumed run's interrupted window halts it when a gate no longer holds.
    #[test]
    fn dec16_a_resume_binds_its_run_and_rechecks_every_gate() {
        let r = Run::new("windows_drift");
        let mut w = RollingWindows::start(&r.dir, r.binding.clone(), "q1", 1_000, 0).unwrap();
        w.close(&r.config, "logs", r.at(1_060, 6_000, 1, FREE))
            .unwrap();
        drop(w);
        let mut other = r.binding.clone();
        other.binary_sha256 = "other".into();
        for (b, q) in [(other, "q1"), (r.binding.clone(), "q2")] {
            let e = RollingWindows::resume(
                &r.dir,
                b,
                q,
                &r.config,
                "logs",
                r.at(1_300, 6_000, 0, FREE),
            )
            .err()
            .unwrap();
            assert_eq!(e.to_string(), "UNTIL_STOPPED_RESUME_DRIFT");
        }
        let (mut w, interrupted) = RollingWindows::resume(
            &r.dir,
            r.binding.clone(),
            "q1",
            &r.config,
            "logs",
            r.at(1_300, 6_000, 0, 1),
        )
        .unwrap();
        assert_eq!(interrupted.verdict, "UNTIL_STOPPED_RUNWAY_EXHAUSTED");
        assert_eq!(
            w.close(&r.config, "logs", r.at(1_360, 6_000, 0, FREE))
                .unwrap_err()
                .to_string(),
            HALTED
        );
    }

    /// DEC-16 (stage 3, shared by every mode): a window halts a run only by a gate code — never by
    /// `CONTINUE` or the recovery verdict — and the run stops after it by `UNTIL_STOPPED_HALT`; a
    /// run without a standing approval goes on after a window whose gates held, and only an
    /// autostarted run has a maximum runtime.
    #[test]
    fn dec16_a_window_halts_a_run_only_by_a_gate_code_in_every_mode() {
        assert!(!halts(CONTINUE));
        assert!(!halts(CRASH_RECOVERED));
        assert!(halts("UNTIL_STOPPED_RUNWAY_EXHAUSTED"));
        let d = std::env::temp_dir().join(format!("p0_us_after_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let b = binding(60, 120);
        let mut rolling = Rolling {
            project: d.clone(),
            binding: b.clone(),
            qualification_id: "0123456789abcdef".into(),
            config: darkbot_solana::runtime_config::RuntimeConfig::default(),
            max_runtime_seconds: None,
            resume: None,
            autostart: None,
        };
        let windows = RollingWindows::start(&d, b, "0123456789abcdef", 1_000, 0).unwrap();
        let check = |verdict: &str| WindowCheck {
            window: 1,
            started_unix_s: 1_000,
            ended_unix_s: 1_060,
            bytes_written: 0,
            bytes_total: 0,
            free_bytes: None,
            runway_need_bytes: 0,
            memory_pressure: None,
            provider: darkbot_solana::provider::WindowUsage {
                requests: 0,
                bytes: 0,
            },
            evidence_id: None,
            evidence_valid_until_unix_s: None,
            sealed: None,
            verdict: verdict.into(),
            detail: String::new(),
        };
        for code in [
            "UNTIL_STOPPED_SEGMENT_INTEGRITY",
            "UNTIL_STOPPED_PROVIDER_BUDGET_EXHAUSTED",
        ] {
            assert_eq!(check(code).halt_code(), Some(code));
            assert_eq!(rolling.after_window(&check(code)), Some(HALT_STOP));
        }
        assert_eq!(check(CRASH_RECOVERED).halt_code(), None);
        assert_eq!(rolling.after_window(&check(CONTINUE)), None);
        assert_eq!(rolling.after_window(&check(CRASH_RECOVERED)), None);
        assert!(
            !rolling.max_runtime_reached(&windows, u64::MAX),
            "no approval, no maximum"
        );
        rolling.max_runtime_seconds = Some(300);
        assert!(!rolling.max_runtime_reached(&windows, 1_299));
        assert!(rolling.max_runtime_reached(&windows, 1_300));
        assert!(!ordered_reason(HALT_STOP));
        assert!(ordered_reason(OPERATOR_STOP));
        windows.finish(false).unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }
}

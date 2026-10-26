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
/// The modes whose runtime runs rolling windows (DEC-16 stage 2: econ4h and market; stage 3:
/// the Shadow owner). A mode outside stays gated whatever else is qualified.
pub const ROLLING_MODES: &[&str] = &[];
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
    /// Until when the provider evidence stays fresh: its oldest measurement plus the
    /// qualification age of the configuration.
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
                .is_none_or(|until| now >= until)
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
        if (s.segments, s.events, s.sealed, s.complete, s.recovered) != (4, 6, true, true, 1) {
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

/// The provider evidence and the provider budget of one window. The evidence must qualify every
/// usable endpoint (FREE_ONLY, measured, fresh: `validate_provider_evidence`); the window budget
/// of a rolling run (`max_requests`, `rpc_rps` per window) must fit each endpoint's proof.
pub fn provider(
    config: &darkbot_solana::runtime_config::RuntimeConfig,
    evidence: Option<&provider_evidence::ProviderEvidence>,
    now: u64,
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
    let short: Vec<String> = proof
        .providers
        .iter()
        .filter(|p| p.allowed_rps < l.rpc_rps as f64 || p.available_calls < l.max_requests)
        .map(|p| {
            format!(
                "{}: allowed_rps {} / rpc_rps {}, available_calls {} / max_requests {}",
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
                "window budget rpc_rps {} and max_requests {} within every proof",
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
    let store = inputs.project.join(STORE);
    let work = store.join(format!(".work.{}", std::process::id()));
    let runtime = if ROLLING_MODES.contains(&binding.mode.as_str()) {
        Part::of(
            PartState::Pass,
            format!("the {} runtime runs rolling windows", binding.mode),
        )
    } else {
        Part::of(
            PartState::NotRun,
            format!(
                "the {} runtime has no rolling windows yet (DEC-16 stage 2: econ4h, market; stage 3: shadow)",
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
        provider(config, evidence.as_ref(), inputs.now);
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
    let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "UNTIL_STOPPED_QUALIFICATION_UNKNOWN".to_string(),
        _ => format!("UNTIL_STOPPED_STORE: {e}"),
    })?;
    if !provider_evidence::names(id, &bytes) {
        return Err("UNTIL_STOPPED_QUALIFICATION_CHANGED".into());
    }
    journal::read_typed_value(&path).map_err(|_| "UNTIL_STOPPED_QUALIFICATION_FORMAT".into())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::analytics::measurement::{RunHorizon, UNTIL_STOPPED_GATED};
    use provider_evidence::{ProviderEvidence, ProviderProof};

    pub(crate) fn binding(checkpoint_seconds: u64, minimum_runway_seconds: u64) -> Binding {
        Binding {
            mode: "econ4h".into(),
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
    fn qualified(b: &Binding) -> Qualification {
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
        assert!(drifted(&|c| c.mode = "market".into()).contains("drift: mode"));
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
        assert!(gated(&q, &b, 2_000).contains("provider evidence expired"));
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

    /// DEC-16: the provider evidence must qualify every usable endpoint and stay fresh; a
    /// window's budget must fit each proof. Without evidence both parts are NOT_RUN.
    #[test]
    fn dec16_provider_parts_need_fresh_evidence_that_covers_a_window() {
        let c = darkbot_solana::runtime_config::RuntimeConfig {
            rpc_endpoints: vec!["https://rpc.invalid".into()],
            ..Default::default()
        };
        let (e, b, until) = provider(&c, None, 1_000);
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
        let enough = proof(l.rpc_rps as f64, l.max_requests, 900.0);
        let (e, b, until) = provider(&c, Some(&enough), 1_000);
        assert_eq!(
            (e.state, b.state),
            (PartState::Pass, PartState::Pass),
            "{e:?} {b:?}"
        );
        assert_eq!(until, Some(900 + l.fe_qualification_age_seconds));
        let (_, b, _) = provider(
            &c,
            Some(&proof(l.rpc_rps as f64, l.max_requests - 1, 900.0)),
            1_000,
        );
        assert_eq!(b.state, PartState::Fail, "{b:?}");
        let stale = 1_000 + l.fe_qualification_age_seconds;
        let (e, _, _) = provider(&c, Some(&enough), stale);
        assert_eq!(e.state, PartState::Fail, "{e:?}");
    }

    /// DEC-16: a qualification of a mode whose runtime has no rolling windows stays gated on
    /// that part, whatever else it holds; without measured runs and evidence those stay NOT_RUN.
    #[test]
    fn dec16_qualify_records_every_part_and_stays_gated_without_rolling_windows() {
        let d = temp("qualify");
        let c = darkbot_solana::runtime_config::RuntimeConfig::default();
        let b = binding(3_600, 86_400);
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
        assert!(!d
            .join(STORE)
            .join(format!(".work.{}", std::process::id()))
            .exists());
        std::fs::remove_dir_all(d).unwrap();
    }
}

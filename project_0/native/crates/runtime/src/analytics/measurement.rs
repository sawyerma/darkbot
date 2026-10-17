//! One time plan for the launcher, capture, attribution and reports. No RPC or trading.
use darkbot_common::timing::Clock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::Path};

pub const SNAPSHOT_COUNT: usize = 4;

/// Run horizon (frontend STARTUP §3). Only `Fixed` exists: an open-ended capture
/// (`UntilStopped` with rolling windows, re-preflight and segment policy) is FEUI follow-up
/// scope and is refused by the parser instead of being emulated by a huge duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunHorizon {
    Fixed { duration_seconds: u64 },
}
pub const NS: u64 = 1_000_000_000;
pub const MAX_SECONDS: u64 = u64::MAX / NS / SNAPSHOT_COUNT as u64;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MeasurementPlan {
    pub schema_version: u32,
    pub duration_seconds: u64,
    pub snapshot_offsets_ns: [u64; SNAPSHOT_COUNT],
    pub budgets: PhaseBudgets,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PhaseBudgets {
    pub bootstrap_seconds: u64,
    pub stream_ready_seconds: u64,
    pub shadow_drain_seconds: u64,
    pub market_drain_seconds: u64,
    pub quiescence_grace_seconds: u64,
    pub strict_seconds: u64,
    pub gap_seconds: u64,
    pub enrichment_seconds: u64,
    pub shutdown_grace_seconds: u64,
}
impl Default for PhaseBudgets {
    fn default() -> Self {
        Self {
            bootstrap_seconds: 600,
            stream_ready_seconds: 600,
            shadow_drain_seconds: 180,
            market_drain_seconds: 120,
            quiescence_grace_seconds: 150,
            strict_seconds: 14400,
            gap_seconds: 180,
            enrichment_seconds: 900,
            shutdown_grace_seconds: 180,
        }
    }
}
impl super::payload::Payload for MeasurementPlan {
    const KIND: u16 = super::journal::kind::MEASUREMENT_PLAN;
}
impl MeasurementPlan {
    pub fn new(seconds: u64) -> Result<Self, String> {
        if !(1..=MAX_SECONDS).contains(&seconds) {
            return Err("duration is zero or exceeds exact nanosecond representation".into());
        }
        Ok(Self {
            schema_version: 1,
            duration_seconds: seconds,
            snapshot_offsets_ns: std::array::from_fn(|i| {
                seconds * NS * (i as u64 + 1) / SNAPSHOT_COUNT as u64
            }),
            budgets: PhaseBudgets::default(),
        })
    }
    /// Delegates to the one duration parser (`runtime_config::duration_seconds`, s/m/h/d) and
    /// checks the plan bounds.
    pub fn parse_duration(s: &str) -> Result<u64, String> {
        let seconds = darkbot_solana::runtime_config::duration_seconds(s)?;
        Self::new(seconds).map(|p| p.duration_seconds)
    }
    /// Typed run horizon of this plan (always fixed: finite measurement windows only).
    pub fn horizon(&self) -> RunHorizon {
        RunHorizon::Fixed {
            duration_seconds: self.duration_seconds,
        }
    }
    pub fn from_lookup(get: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        if let Some(path) = get("P0_MEASUREMENT_PLAN") {
            // JFI-04: the launcher hands over the typed plan journal; a JSON plan file is only
            // read for historical/manual invocations.
            let plan: Self = if path.ends_with(".p0j") {
                super::journal::read_typed_value(Path::new(&path)).map_err(|e| e.to_string())?
            } else {
                serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?
            };
            plan.validate()?;
            // A persisted plan is authoritative. Reject conflicting legacy overrides.
            plan.check_legacy(get)?;
            return Ok(plan);
        }
        let seconds = if let Some(s) = get("P0_MEASUREMENT_DURATION") {
            Self::parse_duration(&s)?
        } else {
            number(get, "P0_ECON_DURATION_SECS", 14400, MAX_SECONDS)?
        };
        let mut plan = Self::new(seconds)?;
        let b = &mut plan.budgets;
        b.stream_ready_seconds = number(
            get,
            "P0_MARKET_READY_TIMEOUT_SECS",
            b.stream_ready_seconds,
            MAX_SECONDS,
        )?;
        b.bootstrap_seconds = number(
            get,
            "P0_ECON_BOOTSTRAP_SECS",
            b.stream_ready_seconds,
            MAX_SECONDS,
        )?;
        b.shadow_drain_seconds = number(
            get,
            "P0_ECON_DRAIN_SECS",
            b.shadow_drain_seconds,
            MAX_SECONDS,
        )?;
        b.market_drain_seconds = number(
            get,
            "P0_MARKET_DRAIN_TIMEOUT_SECS",
            b.market_drain_seconds,
            MAX_SECONDS,
        )?;
        b.strict_seconds = number(
            get,
            "P0_MARKET_RECONCILE_PHASE_MAX_SECS",
            b.strict_seconds,
            MAX_SECONDS,
        )?;
        b.gap_seconds = number(
            get,
            "P0_MARKET_GAP_PHASE_MAX_SECS",
            b.gap_seconds,
            MAX_SECONDS,
        )?;
        b.enrichment_seconds = number(
            get,
            "P0_MARKET_ENRICH_PHASE_MAX_SECS",
            b.enrichment_seconds,
            MAX_SECONDS,
        )?;
        b.quiescence_grace_seconds = number(
            get,
            "P0_QUIESCENCE_SECS",
            b.quiescence_grace_seconds,
            MAX_SECONDS,
        )?;
        b.shutdown_grace_seconds = number(
            get,
            "P0_SHUTDOWN_SECS",
            b.shutdown_grace_seconds,
            MAX_SECONDS,
        )?;
        plan.check_legacy(get)?;
        Ok(plan)
    }
    fn check_legacy(&self, get: &dyn Fn(&str) -> Option<String>) -> Result<(), String> {
        if let Some(duration) = get("P0_MEASUREMENT_DURATION") {
            if Self::parse_duration(&duration)? != self.duration_seconds {
                return Err("duration conflicts with MeasurementPlan".into());
            }
        }
        for (key, expected) in [
            ("P0_ECON_DURATION_SECS", self.duration_seconds),
            ("P0_MARKET_DURATION_SECS", self.duration_seconds),
            ("P0_ECON_BOOTSTRAP_SECS", self.budgets.bootstrap_seconds),
            (
                "P0_MARKET_READY_TIMEOUT_SECS",
                self.budgets.stream_ready_seconds,
            ),
            ("P0_ECON_DRAIN_SECS", self.budgets.shadow_drain_seconds),
            ("P0_QUIESCENCE_SECS", self.budgets.quiescence_grace_seconds),
            ("P0_SHUTDOWN_SECS", self.budgets.shutdown_grace_seconds),
            (
                "P0_MARKET_DRAIN_TIMEOUT_SECS",
                self.budgets.market_drain_seconds,
            ),
            (
                "P0_MARKET_RECONCILE_PHASE_MAX_SECS",
                self.budgets.strict_seconds,
            ),
            ("P0_MARKET_GAP_PHASE_MAX_SECS", self.budgets.gap_seconds),
            (
                "P0_MARKET_ENRICH_PHASE_MAX_SECS",
                self.budgets.enrichment_seconds,
            ),
        ] {
            if get(key).is_some_and(|v| v.parse::<u64>().ok() != Some(expected)) {
                return Err(format!("{key} conflicts with MeasurementPlan"));
            }
        }
        if let Some(s) = get("P0_ECON_SNAPSHOT_SECS") {
            if s.parse::<u64>()
                .ok()
                .and_then(|s| s.checked_mul(SNAPSHOT_COUNT as u64))
                != Some(self.duration_seconds)
            {
                return Err(
                    "P0_ECON_SNAPSHOT_SECS conflicts with automatic quarter snapshots; unset it"
                        .into(),
                );
            }
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        let expected = Self::new(self.duration_seconds)?;
        let b = &self.budgets;
        if self.schema_version != expected.schema_version
            || self.snapshot_offsets_ns != expected.snapshot_offsets_ns
            || [
                b.bootstrap_seconds,
                b.stream_ready_seconds,
                b.shadow_drain_seconds,
                b.market_drain_seconds,
                b.enrichment_seconds,
            ]
            .iter()
            .any(|n| !(1..=MAX_SECONDS).contains(n))
            || !(1..=MAX_SECONDS).contains(&b.strict_seconds)
            || !(1..=MAX_SECONDS).contains(&b.gap_seconds)
            || !(1..=MAX_SECONDS).contains(&b.quiescence_grace_seconds)
            || !(1..=MAX_SECONDS).contains(&b.shutdown_grace_seconds)
        {
            return Err("invalid MeasurementPlan schema, boundaries or budgets".into());
        }
        Ok(())
    }
    pub fn offset_ns(&self, boundary: usize) -> u64 {
        if boundary == 0 {
            0
        } else {
            self.snapshot_offsets_ns[boundary - 1]
        }
    }
    pub fn seconds(&self, boundary: usize) -> f64 {
        self.offset_ns(boundary) as f64 / NS as f64
    }
    pub fn span_seconds(&self, from: usize, through: usize) -> f64 {
        (self.offset_ns(through) - self.offset_ns(from)) as f64 / NS as f64
    }
    pub fn due(&self, elapsed_ns: u64) -> usize {
        self.snapshot_offsets_ns
            .partition_point(|n| *n <= elapsed_ns)
    }
    /// Preserve live evidence's original [start, first_end], (previous_end, end] convention.
    pub fn live_bucket(&self, elapsed_ns: u64) -> Option<usize> {
        (elapsed_ns <= self.duration_seconds * NS).then(|| {
            self.snapshot_offsets_ns
                .partition_point(|n| *n < elapsed_ns)
        })
    }
    /// Chain block time has a separate [start, end) convention, unchanged from legacy reports.
    pub fn chain_bucket(&self, elapsed_ns: u64) -> Option<usize> {
        (elapsed_ns < self.duration_seconds * NS).then(|| self.due(elapsed_ns))
    }
    pub fn cutoff(&self, start: u64) -> Result<u64, String> {
        start
            .checked_add(self.duration_seconds * NS)
            .ok_or("capture clock overflow".into())
    }
    pub fn owner_budget_seconds(&self) -> u64 {
        let b = &self.budgets;
        b.bootstrap_seconds
            + b.stream_ready_seconds
            + self.duration_seconds
            + b.shadow_drain_seconds
            + b.market_drain_seconds
            + b.quiescence_grace_seconds
            + b.strict_seconds
            + b.gap_seconds
            + b.enrichment_seconds
            + b.shutdown_grace_seconds
    }
    pub fn capture_window(&self, clock: Clock, start: u64) -> Result<CaptureWindow, String> {
        let cutoff = self.cutoff(start)?;
        Ok(CaptureWindow {
            capture_start_ns: start,
            capture_cutoff_ns: cutoff,
            capture_start_unix_ns: clock.unix_ns(start),
            capture_cutoff_unix_ns: clock.unix_ns(cutoff),
            capture_start_utc: utc_ns(clock.unix_ns(start)),
            capture_cutoff_utc: utc_ns(clock.unix_ns(cutoff)),
            snapshot_monotonic_ns: self.snapshot_offsets_ns.map(|n| start + n),
            provenance: "SHADOW_READY_THEN_MARKET_STREAM_READY; shared process Clock; UTC projected from fixed origin".into(),
        })
    }
}

/// The measurement window as journaled at StreamReady (one shared process clock).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureWindow {
    pub capture_start_ns: u64,
    pub capture_cutoff_ns: u64,
    pub capture_start_unix_ns: u128,
    pub capture_cutoff_unix_ns: u128,
    pub capture_start_utc: String,
    pub capture_cutoff_utc: String,
    pub snapshot_monotonic_ns: [u64; SNAPSHOT_COUNT],
    pub provenance: String,
}

/// One lifecycle phase of an econ4h run (JFI-04: typed `MEASUREMENT_PHASE` frame).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Bootstrap {
        budget_seconds: u64,
    },
    WaitStreamReady {
        budget_seconds: u64,
    },
    Capture(CaptureWindow),
    Drain {
        stop_reason: String,
        actual_capture_end_ns: u64,
        actual_capture_elapsed_ns: Option<u64>,
        shadow_budget_seconds: u64,
        market_budget_seconds: u64,
    },
    Verification {
        strict_seconds: u64,
        gap_seconds: u64,
        enrichment_seconds: u64,
        shadow_quiescent: bool,
    },
    OwnersFinished {
        aborted: bool,
        shadow_failed: bool,
        market_failed: bool,
    },
    ReportsFinalized {
        technical_qualification: String,
        snapshot_count: u64,
        report: String,
    },
}
impl Phase {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Bootstrap { .. } => "BOOTSTRAP",
            Self::WaitStreamReady { .. } => "WAIT_STREAM_READY",
            Self::Capture(_) => "CAPTURE",
            Self::Drain { .. } => "DRAIN",
            Self::Verification { .. } => "VERIFICATION",
            Self::OwnersFinished { .. } => "OWNERS_FINISHED",
            Self::ReportsFinalized { .. } => "REPORTS_FINALIZED",
        }
    }
}

/// `MEASUREMENT_PHASE` frame: a phase with the shared clock's monotonic and UTC time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhaseRecord {
    pub phase: Phase,
    pub at_ns: u64,
    pub unix_ns: u128,
}
impl super::payload::Payload for PhaseRecord {
    const KIND: u16 = super::journal::kind::MEASUREMENT_PHASE;
}
impl PhaseRecord {
    /// Historical `measurement_phases.jsonl` row (offline render for the pinned readers).
    pub fn legacy_json(&self) -> Value {
        let details = match &self.phase {
            Phase::Bootstrap { budget_seconds } | Phase::WaitStreamReady { budget_seconds } => {
                json!({ "budget_seconds": budget_seconds })
            }
            Phase::Capture(w) => json!(w),
            Phase::Drain {
                stop_reason,
                actual_capture_end_ns,
                actual_capture_elapsed_ns,
                shadow_budget_seconds,
                market_budget_seconds,
            } => json!({
                "stop_reason": stop_reason,
                "actual_capture_end_ns": actual_capture_end_ns,
                "actual_capture_elapsed_ns": actual_capture_elapsed_ns,
                "shadow_budget_seconds": shadow_budget_seconds,
                "market_budget_seconds": market_budget_seconds,
            }),
            Phase::Verification {
                strict_seconds,
                gap_seconds,
                enrichment_seconds,
                shadow_quiescent,
            } => json!({
                "strict_seconds": strict_seconds,
                "gap_seconds": gap_seconds,
                "enrichment_seconds": enrichment_seconds,
                "shadow_quiescent": shadow_quiescent,
            }),
            Phase::OwnersFinished {
                aborted,
                shadow_failed,
                market_failed,
            } => json!({
                "aborted": aborted,
                "shadow_failed": shadow_failed,
                "market_failed": market_failed,
            }),
            Phase::ReportsFinalized {
                technical_qualification,
                snapshot_count,
                report,
            } => json!({
                "technical_qualification": technical_qualification,
                "snapshot_count": snapshot_count,
                "report": report,
            }),
        };
        json!({
            "phase": self.phase.name(),
            "at_ns": self.at_ns,
            "unix_ns": self.unix_ns,
            "details": details,
        })
    }
}
/// Preserve fractional snapshot boundaries in new UTC fields. Legacy second fields remain readable.
pub fn utc_ns(unix_ns: u128) -> String {
    let seconds = crate::market_observer::utc_iso((unix_ns / NS as u128) as u64);
    format!(
        "{}.{:09}Z",
        seconds.trim_end_matches('Z'),
        unix_ns % NS as u128
    )
}

fn number(
    get: &dyn Fn(&str) -> Option<String>,
    key: &str,
    default: u64,
    max: u64,
) -> Result<u64, String> {
    let value = get(key)
        .map(|v| v.parse::<u64>())
        .transpose()
        .map_err(|_| format!("invalid {key}"))?
        .unwrap_or(default);
    if value == 0 || value > max {
        return Err(format!("out of range {key}"));
    }
    Ok(value)
}
/// Phase rows of all C0-E1 threads go through one lock; owner timeout and observer
/// completion may race, and every row must be a whole, durable frame.
static PHASE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Appends one phase row to `measurement_phases.p0j` and fsyncs it (PERSIST-01). The JSONL
/// file the pinned readers consume is rendered from this journal by `finalize_phases`.
pub fn journal(
    paths: &crate::run_layout::RunPaths,
    clock: Clock,
    phase: Phase,
) -> Result<(), String> {
    use super::journal::{self as binary, source, Provenance, Writer};
    let _guard = PHASE_LOCK
        .lock()
        .map_err(|_| "measurement journal lock poisoned")?;
    let path = binary::binary_path(&paths.phases());
    let mut writer = if path.exists() {
        Writer::resume(&path)
    } else {
        Writer::create(&path)
    }
    .map_err(|e| format!("PHASE_JOURNAL: {e}"))?;
    let now = clock.now_ns();
    let unix = clock.unix_ns(now);
    let provenance = Provenance {
        source: source::MEASUREMENT,
        source_seq: writer.next_seq(),
        rx_ns: Some(now),
        unix_ns: u64::try_from(unix).ok(),
        ..Default::default()
    };
    let row = PhaseRecord {
        phase,
        at_ns: now,
        unix_ns: unix,
    };
    writer
        .append_typed(provenance, &row)
        .and_then(|_| writer.sync())
        .map(|_| ())
        .map_err(|e| format!("PHASE_JOURNAL: {e}"))
}
/// Seals the phase journal (COMPLETE only for a run that ended without error) and renders
/// `measurement_phases.jsonl` as an explicit export. No phase can be added afterwards.
pub fn finalize_phases(paths: &crate::run_layout::RunPaths, complete: bool) -> Result<(), String> {
    use super::journal::{self as binary, kind, Writer};
    let _guard = PHASE_LOCK
        .lock()
        .map_err(|_| "measurement journal lock poisoned")?;
    let path = binary::binary_path(&paths.phases());
    Writer::resume(&path)
        .and_then(|mut writer| writer.seal(complete))
        .map_err(|e| format!("PHASE_JOURNAL_SEAL: {e}"))?;
    binary::export(&path, &paths.phases(), Some(kind::MEASUREMENT_PHASE), false)
        .map_err(|e| format!("PHASE_RENDER: {e}"))
}
pub fn persist_manifest(
    paths: &crate::run_layout::RunPaths,
    run_id: &str,
    provider_class: &str,
    plan: &MeasurementPlan,
    clock: Clock,
) -> Result<(), String> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let binary = std::env::current_exe().map_err(|e| e.to_string())?;
    let sha = |bytes: &[u8]| {
        solana_sdk::hash::hash(bytes)
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let git = |args: &[&str]| -> Result<Vec<u8>, String> {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err("manifest git identity unavailable".into());
        }
        Ok(out.stdout)
    };
    let head = String::from_utf8(git(&["rev-parse", "HEAD"])?).map_err(|e| e.to_string())?;
    let mut sources = std::collections::BTreeMap::new();
    for name in git(&[
        "ls-files",
        "-z",
        "-co",
        "--exclude-standard",
        "--",
        "project_0/native",
        "project_0/operations",
        "project_0/scripts",
    ])?
    .split(|b| *b == 0)
    .filter(|s| !s.is_empty())
    {
        let name = std::str::from_utf8(name).map_err(|e| e.to_string())?;
        let p = repo.join(name);
        if p.is_file()
            && p.extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| matches!(s, "rs" | "toml" | "lock" | "sh" | "py"))
        {
            sources.insert(
                name.to_string(),
                sha(&fs::read(p).map_err(|e| e.to_string())?),
            );
        }
    }
    let mut risk = serde_json::Map::new();
    for key in [
        "P0_MAX_SLIPPAGE_BPS",
        "P0_MIN_PROFIT",
        "P0_MAX_CONCURRENT",
        "P0_MAX_OUTSTANDING",
        "P0_MAX_AGE_NS",
        "P0_DEADLINE_NS",
    ] {
        // Only numeric risk configuration. Never persist arbitrary environment contents.
        risk.insert(
            key.into(),
            std::env::var(key)
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .map_or(Value::Null, |v| json!(v)),
        );
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(paths.manifest())
        .map_err(|e| e.to_string())?;
    let mut manifest = json!({"schema":"C0-E1/MeasurementManifest/2","run_id":run_id,"capture_run_id":run_id,
        "plan":plan,"created_unix_ns":clock.unix_ns(clock.now_ns()),"clock_origin_unix_ns":clock.unix_ns(0),
        "clock_definition":"process monotonic ns; UTC = fixed origin + monotonic ns; capture boundaries journaled after StreamReady",
        "head":head.trim(),"binary_sha256":sha(&fs::read(binary).map_err(|e|e.to_string())?),"source_sha256":sources,
        "provider_class":if provider_class.bytes().all(|c|c.is_ascii_alphanumeric() || b"_-".contains(&c)){provider_class}else{"UNKNOWN"},
        "operational_config":darkbot_solana::runtime_config::report(),"risk_limits":risk,"permitted_modes":["SHADOW_OBSERVE_ONLY"],"live_enabled":false,"submission_enabled":false,
        "verification_mode":"SEPARATE_POST_CAPTURE","capture_window":null,"protocol_pin":"33c67987a661f59c6683a65485ebb4f0ef030b77",
        "phase_journal":"measurement_phases.jsonl","compatibility":"hour_01..04 names retained; use plan offsets, not filename as hours"});
    if paths.layout == crate::run_layout::RunLayout::V2 {
        // Layout V2 is a separately versioned manifest; V1 output stays byte-identical.
        manifest["schema"] = json!("C0-E1/MeasurementManifest/3");
        manifest["layout"] = json!(paths.layout.tag());
        manifest["phase_journal"] = json!("capture/measurement_phases.jsonl");
    }
    serde_json::to_writer_pretty(&mut f, &manifest).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    journal(
        paths,
        clock,
        Phase::Bootstrap {
            budget_seconds: plan.budgets.bootstrap_seconds,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durations_and_exact_quarters() {
        for (s, n) in [
            ("10m", 600),
            ("1h", 3600),
            ("5h", 18000),
            ("24h", 86400),
            ("25h", 90000),
            ("86401s", 86401),
            ("61s", 61),
            ("1s", 1),
            ("2d", 172800),
            ("75m", 4500),
        ] {
            let p = MeasurementPlan::new(MeasurementPlan::parse_duration(s).unwrap()).unwrap();
            assert_eq!(
                p.horizon(),
                RunHorizon::Fixed {
                    duration_seconds: n
                }
            );
            assert_eq!(p.duration_seconds, n);
            assert_eq!(p.offset_ns(SNAPSHOT_COUNT), n * NS);
            for i in 1..=SNAPSHOT_COUNT {
                let end = p.offset_ns(i);
                assert_eq!(p.due(end - 1), i - 1);
                assert_eq!(p.due(end), i);
                assert_eq!(p.live_bucket(end), Some(i - 1));
                assert_eq!(p.chain_bucket(end - 1), Some(i - 1));
                if i < SNAPSHOT_COUNT {
                    assert_eq!(p.live_bucket(end + 1), Some(i));
                    assert_eq!(p.chain_bucket(end), Some(i));
                }
            }
            assert_eq!(p.live_bucket(n * NS + 1), None);
            assert_eq!(p.chain_bucket(n * NS), None);
            assert_eq!(
                (0..SNAPSHOT_COUNT)
                    .map(|i| p.span_seconds(i, i + 1))
                    .sum::<f64>(),
                n as f64
            );
            assert_eq!(
                serde_json::from_str::<MeasurementPlan>(&serde_json::to_string(&p).unwrap())
                    .unwrap(),
                p
            );
        }
    }
    #[test]
    fn reject_invalid_duration() {
        for s in [
            "",
            "0s",
            "-1h",
            "+1h",
            "1.5h",
            "1",
            "1w",
            "0d",
            "inf",
            "until-stopped",
            "é",
            "1🦀",
            "999999999999999999999999h",
        ] {
            assert!(MeasurementPlan::parse_duration(s).is_err(), "{s}");
        }
    }
    /// JFI-04: the launcher's typed plan journal is the runtime input; conflicting legacy
    /// overrides are still refused, and a tampered journal fails closed.
    #[test]
    fn typed_plan_journal_is_the_runtime_input() {
        let d = std::env::temp_dir().join(format!("p0_plan_{}_{}", std::process::id(), line!()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let path = d.join("measurement_plan.p0j");
        let plan = MeasurementPlan::new(4500).unwrap();
        super::super::journal::write_typed_value(
            &path,
            super::super::journal::source::MEASUREMENT,
            &plan,
        )
        .unwrap();
        let p = path.to_string_lossy().into_owned();
        let get = |k: &str| (k == "P0_MEASUREMENT_PLAN").then(|| p.clone());
        assert_eq!(MeasurementPlan::from_lookup(&get).unwrap(), plan);
        let conflict = |k: &str| match k {
            "P0_MEASUREMENT_PLAN" => Some(p.clone()),
            "P0_MEASUREMENT_DURATION" => Some("10m".into()),
            _ => None,
        };
        assert!(MeasurementPlan::from_lookup(&conflict).is_err());
        let mut bytes = fs::read(&path).unwrap();
        let n = bytes.len();
        bytes[n / 2] ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(MeasurementPlan::from_lookup(&get).is_err());
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn bootstrap_does_not_consume_capture_budget() {
        let p = MeasurementPlan::new(600).unwrap();
        let start = 599 * NS;
        assert_eq!(p.cutoff(start).unwrap() - start, 600 * NS);
        assert!(p.owner_budget_seconds() > 1200);
        assert!(p.cutoff(u64::MAX).is_err());
    }
    #[test]
    fn legacy_conflicts_and_tampered_plan_fail_closed() {
        assert!(MeasurementPlan::from_lookup(&|k| match k {
            "P0_ECON_DURATION_SECS" => Some("600".into()),
            "P0_ECON_SNAPSHOT_SECS" => Some("3600".into()),
            _ => None,
        })
        .is_err());
        let p =
            MeasurementPlan::from_lookup(&|k| (k == "P0_ECON_DURATION_SECS").then(|| "61".into()))
                .unwrap();
        assert_eq!(p.seconds(1), 15.25);
        let mut bad = p;
        bad.snapshot_offsets_ns[0] += 1;
        assert!(bad.validate().is_err());
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn manifest_is_exclusive_and_phase_journal_preserves_exact_window() {
        use crate::run_layout::{RunLayout, RunPaths};
        let dir = std::env::temp_dir().join(format!("p0-plan-manifest-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        // Unique child; never overwrite earlier evidence, even after a failed test.
        let clock = Clock::new();
        let dir = dir.join(clock.unix_ns(0).to_string());
        fs::create_dir(&dir).unwrap();
        let plan = MeasurementPlan::new(61).unwrap();
        for (layout, manifest_name, phases_name, schema) in [
            (
                RunLayout::V1,
                "measurement_manifest.json",
                "measurement_phases.jsonl",
                "C0-E1/MeasurementManifest/2",
            ),
            (
                RunLayout::V2,
                "manifest.json",
                "capture/measurement_phases.jsonl",
                "C0-E1/MeasurementManifest/3",
            ),
        ] {
            let root = dir.join(layout.tag().replace('/', "_"));
            let paths = RunPaths::new(&root, layout);
            paths.create().unwrap();
            persist_manifest(&paths, "test", "https://secret/?key=hidden", &plan, clock).unwrap();
            let before = fs::read(root.join(manifest_name)).unwrap();
            let manifest: Value = serde_json::from_slice(&before).unwrap();
            assert_eq!(manifest["schema"], schema);
            assert_eq!(manifest["phase_journal"], phases_name);
            assert_eq!(manifest["capture_run_id"], "test");
            match layout {
                RunLayout::V1 => assert!(manifest.get("layout").is_none()),
                RunLayout::V2 => {
                    assert_eq!(manifest["layout"], "Project0/RunLayout/2");
                    assert!(!root.join("measurement_manifest.json").exists());
                    assert!(!root.join("measurement_phases.jsonl").exists());
                }
            }
            assert_eq!(manifest["provider_class"], "UNKNOWN");
            assert_eq!(manifest["binary_sha256"].as_str().unwrap().len(), 64);
            assert!(!manifest["source_sha256"].as_object().unwrap().is_empty());
            assert!(persist_manifest(&paths, "test", "UNKNOWN", &plan, clock).is_err());
            assert_eq!(before, fs::read(root.join(manifest_name)).unwrap());
            let window = plan.capture_window(clock, 600 * NS).unwrap();
            journal(&paths, clock, Phase::Capture(window.clone())).unwrap();
            let drain = Phase::Drain {
                stop_reason: "TIMER".into(),
                actual_capture_end_ns: 1,
                actual_capture_elapsed_ns: None,
                shadow_budget_seconds: 2,
                market_budget_seconds: 3,
            };
            journal(&paths, clock, drain).unwrap();
            // The pinned readers see the JSONL render only after the journal was sealed.
            assert!(!root.join(phases_name).exists());
            finalize_phases(&paths, true).unwrap();
            assert!(journal(&paths, clock, Phase::Bootstrap { budget_seconds: 1 }).is_err());
            let mut sealed = crate::analytics::journal::Reader::open(
                &crate::analytics::journal::binary_path(&root.join(phases_name)),
            )
            .unwrap();
            while sealed.next_frame().unwrap().is_some() {}
            assert!(sealed.sealed && sealed.complete);
            assert!(finalize_phases(&paths, true).is_err()); // never a second render
            let rows: Vec<Value> = fs::read_to_string(root.join(phases_name))
                .unwrap()
                .lines()
                .map(|s| serde_json::from_str(s).unwrap())
                .collect();
            assert_eq!(rows.len(), 3);
            // The typed CAPTURE phase renders the same window object the JSON phase carried.
            assert_eq!(rows[1]["details"], serde_json::to_value(&window).unwrap());
            assert_eq!(rows[1]["phase"], "CAPTURE");
            assert_eq!(rows[2]["details"]["stop_reason"], "TIMER");
            assert!(rows[2]["details"]["actual_capture_elapsed_ns"].is_null());
            assert_eq!(rows[1]["details"]["capture_cutoff_ns"], 661 * NS);
            assert_eq!(
                rows[1]["details"]["snapshot_monotonic_ns"][0],
                615_250_000_000u64
            );
        }
        assert_eq!(utc_ns(15_250_000_001), "1970-01-01T00:00:15.250000001Z");
    }
}

/// Standalone Shadow uses the same duration and readiness rule as a measurement.
/// No capture time is spent while initial recovery is incomplete.
pub struct ReadyWindow {
    plan: MeasurementPlan,
    capture_start: Option<u64>,
}
impl ReadyWindow {
    pub fn new(plan: MeasurementPlan) -> Self {
        Self {
            plan,
            capture_start: None,
        }
    }
    /// elapsed_ns is monotonic elapsed since entering Shadow bootstrap.
    pub fn poll(&mut self, elapsed_ns: u64, ready: bool) -> Result<bool, &'static str> {
        if self.capture_start.is_none() {
            if elapsed_ns >= self.plan.budgets.bootstrap_seconds.saturating_mul(NS) {
                return Err("SHADOW_BOOTSTRAP_BUDGET");
            }
            if ready {
                self.capture_start = Some(elapsed_ns);
            }
        }
        Ok(self.capture_start.is_some_and(|start| {
            elapsed_ns.saturating_sub(start) >= self.plan.duration_seconds * NS
        }))
    }
}
#[cfg(test)]
mod operational_window_tests {
    use super::*;
    #[test]
    fn bootstrap_does_not_consume_capture_and_reconnect_does_not_extend_it() {
        let mut w = ReadyWindow::new(MeasurementPlan::new(61).unwrap());
        assert_eq!(w.poll(59 * NS, false), Ok(false));
        assert_eq!(w.poll(60 * NS, true), Ok(false));
        assert_eq!(w.poll(120 * NS, false), Ok(false));
        assert_eq!(w.poll(121 * NS, false), Ok(true));
        let mut w = ReadyWindow::new(MeasurementPlan::new(600).unwrap());
        assert_eq!(w.poll(600 * NS, false), Err("SHADOW_BOOTSTRAP_BUDGET"));
    }
}

//! SK-06: capacity preflight before a capture of a given duration.
//!
//! Every input is measured or configured, never assumed. The data rates come from sealed
//! journals of earlier real runs in exactly the frame format the current binary writes (every
//! lane: market, Shadow evaluations, economics reports, owner evidence, MT0, phases), the
//! sustained write rate and sync cost are measured now on the target volume through the journal
//! codec, disk, memory, swap, pressure and CPU come from the OS, budgets from the operational
//! config and the selected verification mode. The verifier budget is sized for the mode
//! actually selected: `off` needs none, `index`/`crosscheck` scale with the measured program
//! transaction rate, `strict` with the measured slot rate and the per-slot cost of earlier
//! strict verifications. Missing measurements are CAPACITY_UNVERIFIED or NOT_RUN, never PASS.
//! A capture longer than `CALIBRATION_MAX_S` starts only when QUALIFIED; a short calibration
//! capture may start UNVERIFIED (it produces the basis). The result is kept as a typed journal.
use super::budget::{
    free_disk_bytes, physical_memory_bytes, MemorySample, MIB, PRESSURE_CRITICAL, PRESSURE_WARN,
};
use super::writer::WriterConfig;
use crate::analytics::journal::{self, kind, source, Provenance};
use crate::analytics::payload::{self, Payload};
use crate::market::observer::rows::{MarketCoverage, WindowEvent};
use crate::market::observer::verification::{StrictResult, BATCH_SLOTS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Longest capture that may start without a measured capacity basis (it calibrates).
pub const CALIBRATION_MAX_S: u64 = 600;
/// Name of the native report in the capture directory of a run.
pub const REPORT_JOURNAL: &str = "capacity_preflight.p0j";
/// Bytes of the write/sync probe of an operator preflight (`p0ctl run preflight`, the DEC-16
/// qualification; `p0-capacity --probe-mib` defaults to the same).
pub const PROBE_BYTES: u64 = 64 * MIB;
/// Burst window over the frames' clock.
const BURST_WINDOW_NS: u64 = 10_000_000_000;
/// Nominal Solana slot time: only printed as context, never used for a verdict.
const NOMINAL_SLOT_MS: u64 = 400;
/// `getSignaturesForAddress` page size of the index verifier.
const SIGNATURES_PER_PAGE: u64 = 1000;
/// Newest valid basis runs taken into account.
pub const MAX_BASIS_RUNS: usize = 3;
/// Skip reasons kept verbatim (the total is always counted).
const MAX_SKIP_REASONS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    Qualified,
    CapacityUnverified,
    Blocked,
    /// A required measurement was not taken (counts like CAPACITY_UNVERIFIED).
    NotRun,
    /// Not needed for this request (e.g. verifier budget with `--chain-verify off`).
    NotApplicable,
}
impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Qualified => "QUALIFIED",
            Verdict::CapacityUnverified => "CAPACITY_UNVERIFIED",
            Verdict::Blocked => "BLOCKED",
            Verdict::NotRun => "NOT_RUN",
            Verdict::NotApplicable => "NOT_APPLICABLE",
        }
    }
}

/// Post-capture chain verification selected for the run (PHASENPLAN L.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerifyMode {
    Off,
    Index,
    Strict,
    Crosscheck,
}
impl VerifyMode {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "off" => Ok(Self::Off),
            "index" => Ok(Self::Index),
            "strict" => Ok(Self::Strict),
            "crosscheck" => Ok(Self::Crosscheck),
            _ => Err(format!("VERIFY_MODE_INVALID: {s}")),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Index => "index",
            Self::Strict => "strict",
            Self::Crosscheck => "crosscheck",
        }
    }
}

impl VerifyBudget {
    /// The typed budget of a verification method (DEC-08: requests, bytes and RPS per method,
    /// the same as the verifier's `StrictBudget::of_mode`; pages, time and disk shared).
    pub fn for_mode(mode: VerifyMode, l: &darkbot_solana::runtime_config::Limits) -> Self {
        let (rps, calls, mib) = match mode {
            VerifyMode::Off | VerifyMode::Strict => (l.verify_rps, l.verify_calls, l.verify_mib),
            VerifyMode::Index => (l.index_rps, l.index_calls, l.index_mib),
            VerifyMode::Crosscheck => (l.crosscheck_rps, l.crosscheck_calls, l.crosscheck_mib),
        };
        VerifyBudget {
            mode,
            rps,
            calls,
            seconds: l.verify_seconds,
            max_pages: l.verify_pages,
            bytes: mib * MIB,
            disk_bytes: l.verify_disk_mib * MIB,
        }
    }
}

/// The verifier budget the run will actually use (measurement config, else operational config).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerifyBudget {
    pub mode: VerifyMode,
    pub rps: u64,
    pub calls: u64,
    pub seconds: u64,
    pub max_pages: u64,
    pub bytes: u64,
    pub disk_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub verdict: Verdict,
    pub detail: String,
}

/// Bytes of one journal lane of the basis runs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LaneRate {
    pub lane: String,
    pub journals: u64,
    pub frames: u64,
    pub bytes: u64,
    pub avg_bytes_per_s: f64,
    /// Highest 10 s window; `None` when the lane's frames carry no clock.
    pub burst_bytes_per_s: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BasisRun {
    pub root: PathBuf,
    pub source: String,
    pub span_s: f64,
    pub bytes: u64,
    pub program_transactions: u64,
    pub slots: Option<(u64, u64)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: String,
}

/// Data rates measured from earlier sealed runs (empty `runs` = no basis).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RateBasis {
    pub runs: Vec<BasisRun>,
    pub lanes: Vec<LaneRate>,
    pub span_s: f64,
    pub bytes: u64,
    pub avg_bytes_per_s: f64,
    /// Sum of the lanes' bursts (conservative: bursts need not coincide); `None` if a lane
    /// carries no clock.
    pub burst_bytes_per_s: Option<f64>,
    /// Burst of the market lane alone (the lane behind the capture byte budget).
    pub market_burst_bytes_per_s: Option<f64>,
    pub program_tx_per_s: Option<f64>,
    pub slots_per_s: Option<f64>,
    pub skipped: Vec<Skipped>,
    pub skipped_total: u64,
}
impl RateBasis {
    pub fn measured(&self) -> bool {
        !self.runs.is_empty()
    }
    fn skip(&mut self, path: &Path, reason: impl Into<String>) {
        self.skipped_total += 1;
        if self.skipped.len() < MAX_SKIP_REASONS {
            self.skipped.push(Skipped {
                path: path.into(),
                reason: reason.into(),
            });
        }
    }
}

/// Measured cost of earlier strict verifications (their kept attempt results).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StrictBasis {
    pub results: u64,
    pub slots: u64,
    pub calls: u64,
    pub response_bytes: u64,
}

/// Write probe on the target volume through the journal codec.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WriteProbe {
    pub bytes: u64,
    pub seconds: f64,
    pub bytes_per_s: f64,
    pub syncs: u64,
    pub sync_mean_ms: f64,
    pub sync_max_ms: f64,
}

/// OS facts at preflight time.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub free_disk_bytes: Option<u64>,
    pub physical_bytes: Option<u64>,
    pub memory_pressure: Option<i32>,
    pub swap_used_bytes: Option<u64>,
    pub cores: u64,
    pub load_1m: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub duration_s: u64,
    pub out_dir: PathBuf,
    pub capture_source: String,
    pub verify: VerifyBudget,
    pub initial_bytes: u64,
    pub max_bytes: u64,
    pub system_reserve_bytes: u64,
    pub disk_reserve_bytes: u64,
    pub stall_budget_s: f64,
    pub safety_percent: u64,
    /// SK-03: byte budget of the economics/winner/Shadow retained collections.
    pub econ_retention_bytes: u64,
}

impl Request {
    /// Budgets from the active operational config; the caller overrides the verifier budget
    /// with the run's measurement config.
    pub fn from_active(duration_s: u64, out_dir: &Path, capture_source: &str) -> Self {
        Self::from_limits(
            &darkbot_solana::runtime_config::active().limits,
            duration_s,
            out_dir,
            capture_source,
        )
    }
    /// Budgets of a run with these limits (DEC-16: an unbounded run's runway).
    pub fn from_limits(
        l: &darkbot_solana::runtime_config::Limits,
        duration_s: u64,
        out_dir: &Path,
        capture_source: &str,
    ) -> Self {
        let w = WriterConfig::from_limits(l);
        Self {
            duration_s,
            out_dir: out_dir.into(),
            capture_source: capture_source.into(),
            verify: VerifyBudget::for_mode(VerifyMode::Off, l),
            initial_bytes: w.initial_bytes,
            max_bytes: w.max_bytes,
            system_reserve_bytes: w.system_reserve_bytes,
            disk_reserve_bytes: w.disk_reserve_bytes,
            stall_budget_s: w.stall_budget.as_secs_f64(),
            safety_percent: l.capacity_safety_percent,
            econ_retention_bytes: l.econ_retention_mib * MIB,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CapacityReport {
    pub request: Request,
    pub basis: RateBasis,
    pub strict_basis: Option<StrictBasis>,
    pub probe: Result<WriteProbe, String>,
    pub environment: Environment,
    /// Bytes this run needs on its volume (journals × safety + reserves), when measurable.
    pub disk_need_bytes: Option<u64>,
    pub checks: Vec<Check>,
    pub verdict: Verdict,
    /// Whether this capture may start (QUALIFIED, or a calibration capture UNVERIFIED).
    pub start_allowed: bool,
}
impl Payload for CapacityReport {
    const KIND: u16 = kind::CAPACITY_PREFLIGHT;
}

impl CapacityReport {
    pub fn terminal(&self) -> String {
        let r = &self.request;
        let mut out = format!(
            "PROJECT 0 CAPACITY PREFLIGHT duration={}s source={} chain_verify={} verdict={} start_allowed={}\n",
            r.duration_s,
            r.capture_source,
            r.verify.mode.as_str(),
            self.verdict.as_str(),
            self.start_allowed
        );
        let b = &self.basis;
        out.push_str(&format!(
            "  basis: {} run(s), {:.0}s, {} skipped\n",
            b.runs.len(),
            b.span_s,
            b.skipped_total
        ));
        for l in &b.lanes {
            out.push_str(&format!(
                "    lane {:<12} {:>10.1} KiB/s avg  {:>10} burst  {} frame(s)\n",
                l.lane,
                l.avg_bytes_per_s / 1024.0,
                l.burst_bytes_per_s
                    .map(|v| format!("{:.1} KiB/s", v / 1024.0))
                    .unwrap_or_else(|| "UNKNOWN".into()),
                l.frames
            ));
        }
        for s in &b.skipped {
            out.push_str(&format!("    skipped {}: {}\n", s.path.display(), s.reason));
        }
        for c in &self.checks {
            out.push_str(&format!(
                "  {:<20} {:<20} {}\n",
                c.name,
                c.verdict.as_str(),
                c.detail
            ));
        }
        if !self.start_allowed {
            out.push_str("  START BLOCKED: resolve the checks above or run a calibration capture (≤ 600 s, explicit Mainnet permission)\n");
        } else if self.verdict != Verdict::Qualified {
            out.push_str("  CALIBRATION CAPTURE: start allowed without a qualified basis (≤ 600 s); its sealed journals become the basis\n");
        }
        out
    }
}

/// Lane of a journal file (by its stem).
fn lane(path: &Path) -> &'static str {
    match path.file_stem().and_then(|s| s.to_str()).unwrap_or("") {
        "market" => "market",
        "shadow_evaluations" => "shadow",
        "economics_reports" => "economics",
        "opportunities" => "owner",
        "mt0" => "mt0",
        "measurement_phases" => "phases",
        _ => "other",
    }
}

/// Journals of a run (capture-time lanes only: no verification, reports, probes or preflights).
fn run_journals(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, top: bool, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                if !(top && (name == "verification" || name == "reports")) {
                    walk(&p, false, out);
                }
            } else if p.extension().and_then(|x| x.to_str()) == Some("p0j")
                && name != REPORT_JOURNAL
                && !super::segments::is_continuation(&p)
            {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, true, &mut out);
    out.sort();
    out
}

/// One journal of a basis run: frames in the current write format, sealed complete.
struct JournalScan {
    frames: u64,
    bytes: u64,
    /// `(clock, t_ns, bytes)`: clock 0 = receive clock, 1 = wall clock.
    timed: Vec<(u8, u64, u64)>,
    rx_span: Option<(u64, u64)>,
    slots: Option<(u64, u64)>,
    program_transactions: u64,
    source: Option<String>,
}

fn scan(path: &Path) -> Result<JournalScan, String> {
    // SK-07: a lane journal is its whole segment chain.
    let mut r = super::segments::ChainReader::open(path).map_err(|e| format!("UNREADABLE: {e}"))?;
    let mut s = JournalScan {
        frames: 0,
        bytes: 0,
        timed: Vec::new(),
        rx_span: None,
        slots: None,
        program_transactions: 0,
        source: None,
    };
    let span =
        |v: Option<(u64, u64)>, x: u64| Some(v.map_or((x, x), |(a, b)| (a.min(x), b.max(x))));
    while let Some(f) = r.next_frame().map_err(|e| format!("UNREADABLE: {e}"))? {
        if f.kind == journal::MANIFEST {
            continue;
        }
        // Only the byte format the current binary writes for this kind is a valid basis.
        payload::check_write(f.kind, f.schema)
            .map_err(|_| format!("FOREIGN_FORMAT kind={} schema={}", f.kind, f.schema))?;
        s.frames += 1;
        s.bytes += f.bytes;
        match (f.provenance.rx_ns, f.provenance.unix_ns) {
            (Some(t), _) => {
                s.timed.push((0, t, f.bytes));
                s.rx_span = span(s.rx_span, t);
            }
            (None, Some(t)) => s.timed.push((1, t, f.bytes)),
            _ => {}
        }
        if let Some(slot) = f.provenance.slot {
            s.slots = span(s.slots, slot);
        }
        if f.kind == kind::MARKET_TRANSACTION {
            s.program_transactions += 1;
        } else if f.kind == kind::MARKET_COVERAGE && s.source.is_none() {
            if let Ok(MarketCoverage::Window {
                event: WindowEvent::Start(start),
                ..
            }) = f.typed::<MarketCoverage>()
            {
                s.source = Some(start.source.clone());
            }
        }
    }
    if !r.sealed {
        return Err("UNSEALED (crashed or still written)".into());
    }
    if !r.complete {
        return Err("SEALED_INCOMPLETE".into());
    }
    Ok(s)
}

/// Highest 10 s window rate per clock, summed over clocks (conservative).
fn burst(timed: &mut [(u8, u64, u64)]) -> Option<f64> {
    if timed.is_empty() {
        return None;
    }
    timed.sort_unstable();
    let mut total = 0.0;
    for clock in [0u8, 1] {
        let series: Vec<_> = timed.iter().filter(|t| t.0 == clock).collect();
        let (mut lo, mut window, mut best) = (0usize, 0u64, 0u64);
        for hi in 0..series.len() {
            window += series[hi].2;
            while series[hi].1 - series[lo].1 > BURST_WINDOW_NS {
                window -= series[lo].2;
                lo += 1;
            }
            best = best.max(window);
        }
        total += best as f64 / (BURST_WINDOW_NS as f64 / 1e9);
    }
    Some(total)
}

/// Rates of the newest valid basis runs (`candidates` newest first). A run counts only if it has
/// a market journal of the requested capture source, every journal is sealed complete in the
/// current write format, and its receive-clock span is at least one burst window.
pub fn measure_basis(candidates: &[PathBuf], capture_source: &str) -> RateBasis {
    let mut b = RateBasis::default();
    let mut lanes: BTreeMap<&'static str, LaneRate> = BTreeMap::new();
    let (mut program_txs, mut slot_count, mut slot_span) = (0u64, 0u64, 0.0f64);
    for root in candidates {
        if b.runs.len() >= MAX_BASIS_RUNS {
            break;
        }
        let journals = run_journals(root);
        if journals.is_empty() {
            b.skip(root, "NO_JOURNAL");
            continue;
        }
        let scans: Result<Vec<_>, _> = journals
            .iter()
            .map(|p| scan(p).map(|s| (p, s)).map_err(|e| (p, e)))
            .collect();
        let scans = match scans {
            Ok(s) => s,
            Err((p, e)) => {
                b.skip(p, e);
                continue;
            }
        };
        let Some((_, market)) = scans.iter().find(|(p, _)| lane(p) == "market") else {
            b.skip(root, "NO_MARKET_JOURNAL");
            continue;
        };
        let Some(source) = market.source.clone() else {
            b.skip(root, "CAPTURE_SOURCE_UNKNOWN");
            continue;
        };
        if source != capture_source {
            b.skip(
                root,
                format!("CAPTURE_SOURCE_MISMATCH basis={source} requested={capture_source}"),
            );
            continue;
        }
        let Some((first, last)) = market.rx_span.filter(|(a, z)| z - a >= BURST_WINDOW_NS) else {
            b.skip(root, "SPAN_SHORTER_THAN_BURST_WINDOW");
            continue;
        };
        let span_s = (last - first) as f64 / 1e9;
        let mut run_bytes = 0;
        for (p, s) in scans.iter() {
            let mut timed = s.timed.clone();
            let l = lanes.entry(lane(p)).or_insert_with(|| LaneRate {
                lane: lane(p).into(),
                burst_bytes_per_s: Some(0.0),
                ..Default::default()
            });
            l.journals += 1;
            l.frames += s.frames;
            l.bytes += s.bytes;
            l.burst_bytes_per_s = match (l.burst_bytes_per_s, burst(&mut timed)) {
                (Some(a), Some(z)) => Some(a.max(z)),
                _ => None,
            };
            run_bytes += s.bytes;
        }
        program_txs += market.program_transactions;
        if let Some((lo, hi)) = market.slots.filter(|(lo, hi)| hi > lo) {
            slot_count += hi - lo;
            slot_span += span_s;
        }
        b.span_s += span_s;
        b.bytes += run_bytes;
        b.runs.push(BasisRun {
            root: root.clone(),
            source,
            span_s,
            bytes: run_bytes,
            program_transactions: market.program_transactions,
            slots: market.slots,
        });
    }
    if !b.measured() {
        return b;
    }
    b.avg_bytes_per_s = b.bytes as f64 / b.span_s;
    b.lanes = lanes
        .into_values()
        .map(|mut l| {
            l.avg_bytes_per_s = l.bytes as f64 / b.span_s;
            l
        })
        .collect();
    b.burst_bytes_per_s = b.lanes.iter().try_fold(0.0, |acc, l| {
        l.burst_bytes_per_s.map(|v| acc + v.max(l.avg_bytes_per_s))
    });
    b.market_burst_bytes_per_s = b
        .lanes
        .iter()
        .find(|l| l.lane == "market")
        .and_then(|l| l.burst_bytes_per_s.map(|v| v.max(l.avg_bytes_per_s)));
    b.program_tx_per_s = Some(program_txs as f64 / b.span_s);
    b.slots_per_s = (slot_span > 0.0).then(|| slot_count as f64 / slot_span);
    b
}

/// Run directories under `root`, newest first, without `exclude`.
pub fn discover_runs(root: &Path, exclude: Option<&Path>) -> Vec<PathBuf> {
    let exclude = exclude.and_then(|e| e.canonicalize().ok());
    let mut runs: Vec<_> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
        .filter(|e| e.path().canonicalize().ok() != exclude)
        .map(|e| {
            let at = e.metadata().and_then(|m| m.modified()).ok();
            (at, e.path())
        })
        .collect();
    runs.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    runs.into_iter().map(|(_, p)| p).collect()
}

/// Per-slot cost of earlier strict verifications: kept attempt results under `verification/`
/// of the given directories (slots counted from their completed batches).
pub fn measure_strict(dirs: &[PathBuf]) -> Option<StrictBasis> {
    fn results(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                results(&p, out);
            } else if p.parent().and_then(|d| d.file_name())
                == Some(std::ffi::OsStr::new("results"))
                && p.extension().and_then(|x| x.to_str()) == Some("p0j")
            {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    for d in dirs {
        results(d, &mut files);
    }
    let mut s = StrictBasis::default();
    for f in files {
        let Ok(r) = journal::read_typed_value::<StrictResult>(&f) else {
            continue;
        };
        let slots: u64 = r
            .batches
            .iter()
            .filter_map(|b| {
                let name = b.checkpoint.strip_prefix("batch_")?.strip_suffix(".p0j")?;
                let (lo, hi) = name.split_once('_')?;
                Some(hi.parse::<u64>().ok()? - lo.parse::<u64>().ok()? + 1)
            })
            .sum();
        if slots == 0 || r.rpc_calls == 0 {
            continue;
        }
        s.results += 1;
        s.slots += slots;
        s.calls += r.rpc_calls;
        s.response_bytes += r.successful_response_bytes;
    }
    (s.results > 0).then_some(s)
}

/// Sustained write rate and sync cost measured on `dir`'s volume: `bytes` of real journal frames
/// in batches, synced every 8 batches and at seal. The probe journal is removed afterwards.
pub fn probe_write_rate(dir: &Path, bytes: u64) -> Result<WriteProbe, String> {
    let path = dir.join(format!(".capacity-probe-{}.p0j", std::process::id()));
    let result = (|| -> Result<WriteProbe, String> {
        let mut w = journal::Writer::create(&path).map_err(|e| e.to_string())?;
        let row = MarketCoverage::SlotFailed {
            slot: 0,
            error: "x".repeat(256),
        };
        let encoded = payload::encode(&row).map_err(|e| e.to_string())?;
        let p = Provenance::default();
        let batch: Vec<_> = (0..256)
            .map(|_| {
                (
                    MarketCoverage::KIND,
                    MarketCoverage::SCHEMA,
                    &p,
                    encoded.as_slice(),
                )
            })
            .collect();
        let started = Instant::now();
        let (mut n, mut syncs, mut sync_total, mut sync_max) = (0u64, 0u64, 0.0f64, 0.0f64);
        let mut timed_sync = |w: &mut journal::Writer, seal: bool| -> Result<(), String> {
            let t = Instant::now();
            if seal {
                w.seal(true).map_err(|e| e.to_string())?;
            } else {
                w.sync().map_err(|e| e.to_string())?;
            }
            let ms = t.elapsed().as_secs_f64() * 1e3;
            syncs += 1;
            sync_total += ms;
            sync_max = sync_max.max(ms);
            Ok(())
        };
        while w.position() < bytes {
            w.append_batch(batch.iter().copied())
                .map_err(|e| e.to_string())?;
            n += 1;
            if n % 8 == 0 {
                timed_sync(&mut w, false)?;
            }
        }
        timed_sync(&mut w, true)?;
        let seconds = started.elapsed().as_secs_f64().max(1e-6);
        Ok(WriteProbe {
            bytes: w.position(),
            seconds,
            bytes_per_s: w.position() as f64 / seconds,
            syncs,
            sync_mean_ms: sync_total / syncs as f64,
            sync_max_ms: sync_max,
        })
    })();
    let _ = std::fs::remove_file(&path);
    result
}

pub fn environment(dir: &Path) -> Environment {
    let mut load = [0f64; 1];
    // SAFETY: getloadavg writes at most one sample into the array we own.
    let samples = unsafe { libc::getloadavg(load.as_mut_ptr(), 1) };
    let memory = MemorySample::now();
    Environment {
        free_disk_bytes: free_disk_bytes(dir),
        physical_bytes: physical_memory_bytes(),
        memory_pressure: memory.pressure_level,
        swap_used_bytes: memory.swap_used_bytes,
        cores: std::thread::available_parallelism()
            .map(|n| n.get() as u64)
            .unwrap_or(1),
        load_1m: (samples == 1).then_some(load[0]),
    }
}

/// Verifier budget needed for the selected mode, from measured rates.
fn verification_check(
    req: &Request,
    basis: &RateBasis,
    strict: Option<&StrictBasis>,
    safety: f64,
) -> (Verdict, String) {
    use Verdict::*;
    let v = &req.verify;
    let duration = req.duration_s as f64;
    let fits = |calls: u64, bytes: Option<u64>, pages: Option<u64>| {
        let seconds = calls.div_ceil(v.rps.max(1));
        let mut short = Vec::new();
        if calls > v.calls {
            short.push(format!("calls {calls} > {}", v.calls));
        }
        if seconds > v.seconds {
            short.push(format!(
                "walltime {seconds}s > {}s at {} rps",
                v.seconds, v.rps
            ));
        }
        if let Some(p) = pages.filter(|p| *p > v.max_pages) {
            short.push(format!("pages {p} > {}", v.max_pages));
        }
        if let Some(b) = bytes.filter(|b| *b > v.bytes.min(v.disk_bytes)) {
            short.push(format!(
                "bytes {:.0} MiB > {} MiB",
                b as f64 / MIB as f64,
                v.bytes.min(v.disk_bytes) / MIB
            ));
        }
        (short, seconds)
    };
    let insufficient = |mode: &str, short: Vec<String>| {
        format!(
            "{mode} needs more than configured: {}; raise --verify-max-calls/--verify-max-seconds/--verify-rps \
             or capture with --chain-verify off and verify later with --resume-verification",
            short.join(", ")
        )
    };
    match v.mode {
        VerifyMode::Off => (
            NotApplicable,
            "chain verification off: verification NOT_RUN by choice, no chain proof for this run"
                .into(),
        ),
        VerifyMode::Index | VerifyMode::Crosscheck => {
            let Some(rate) = basis.program_tx_per_s else {
                return (
                    CapacityUnverified,
                    "index sizing needs the measured program transaction rate (no basis)".into(),
                );
            };
            let sources = if v.mode == VerifyMode::Crosscheck {
                2
            } else {
                1
            };
            let signatures = (rate * duration * safety).ceil() as u64;
            let pages = signatures / SIGNATURES_PER_PAGE + 1;
            // DEC-08: each CROSSCHECK source reads within its own budget and deadline.
            let calls = signatures + pages;
            let (short, seconds) = fits(calls, None, Some(pages));
            if short.is_empty() {
                (
                    Qualified,
                    format!(
                        "{}: ~{signatures} signatures ({rate:.3}/s × {}s × {safety:.2}), {calls} calls, {seconds}s at {} rps per source × {sources}; provider quota checked by the verification preflight",
                        v.mode.as_str(), req.duration_s, v.rps
                    ),
                )
            } else {
                (Blocked, insufficient(v.mode.as_str(), short))
            }
        }
        VerifyMode::Strict => {
            let nominal = req.duration_s * 1000 / NOMINAL_SLOT_MS;
            let Some(slot_rate) = basis.slots_per_s else {
                return (
                    CapacityUnverified,
                    format!(
                        "strict sizing needs the measured slot rate (no basis); context only: ~{nominal} slots at a nominal {NOMINAL_SLOT_MS} ms vs {} configured calls — never strict-qualified without measurement",
                        v.calls
                    ),
                );
            };
            let slots = (slot_rate * duration * safety).ceil() as u64;
            // getBlock per produced slot, getBlocks and the re-fetched parent anchor per batch.
            let calls_floor = slots + 2 * slots.div_ceil(BATCH_SLOTS);
            let (calls, bytes) = match strict {
                Some(s) => (
                    ((s.calls as f64 / s.slots as f64) * slots as f64).ceil() as u64,
                    Some(((s.response_bytes as f64 / s.slots as f64) * slots as f64).ceil() as u64),
                ),
                None => (calls_floor, None),
            };
            let calls = calls.max(calls_floor);
            let (short, seconds) = fits(calls, bytes, None);
            if !short.is_empty() {
                return (
                    Blocked,
                    format!(
                        "CANONICAL_VERIFIED not reachable within the budget: {}",
                        insufficient("strict", short)
                    ),
                );
            }
            match bytes {
                None => (
                    CapacityUnverified,
                    format!("strict: ~{slots} slots, ≥{calls} calls, {seconds}s; response bytes per slot unmeasured (no earlier strict result)"),
                ),
                Some(b) => (
                    Qualified,
                    format!(
                        "strict: ~{slots} slots ({slot_rate:.2}/s), {calls} calls, {seconds}s, {:.0} MiB responses; archive availability is a provider check",
                        b as f64 / MIB as f64
                    ),
                ),
            }
        }
    }
}

/// The capacity decision from measured inputs (pure).
/// Bytes a run of `req` needs on its volume at `avg_bytes_per_s`: all journal lanes for its
/// duration (with safety) plus the disk reserve and the verifier's disk (DEC-16: the runway of
/// an unbounded run, re-checked every window).
pub fn disk_need_bytes(req: &Request, avg_bytes_per_s: f64) -> u64 {
    let safety = 1.0 + req.safety_percent as f64 / 100.0;
    // DEC-08: a CROSSCHECK keeps one checkpoint per source.
    let verify_disk = match req.verify.mode {
        VerifyMode::Off => 0,
        VerifyMode::Crosscheck => req.verify.disk_bytes.saturating_mul(2),
        _ => req.verify.disk_bytes,
    };
    (avg_bytes_per_s * req.duration_s as f64 * safety) as u64 + req.disk_reserve_bytes + verify_disk
}

pub fn evaluate(
    req: Request,
    basis: RateBasis,
    strict_basis: Option<StrictBasis>,
    probe: Result<WriteProbe, String>,
    env: Environment,
) -> CapacityReport {
    use Verdict::*;
    let mut checks = Vec::new();
    let mut check = |name: &str, verdict: Verdict, detail: String| {
        checks.push(Check {
            name: name.into(),
            verdict,
            detail,
        })
    };
    let safety = 1.0 + req.safety_percent as f64 / 100.0;
    let gib = |b: f64| b / (1024.0 * MIB as f64);
    let mib = |b: f64| b / MIB as f64;
    let measured = basis.measured();
    if measured {
        check(
            "data_rate",
            Qualified,
            format!(
                "avg {:.1} KiB/s over {:.0}s of {} run(s), {} lane(s)",
                basis.avg_bytes_per_s / 1024.0,
                basis.span_s,
                basis.runs.len(),
                basis.lanes.len()
            ),
        );
    } else {
        check(
            "data_rate",
            CapacityUnverified,
            format!(
                "no sealed earlier run of source {} in the current journal format ({} candidate(s) skipped); run a calibration capture first",
                req.capture_source, basis.skipped_total
            ),
        );
    }
    let disk_need = measured.then(|| disk_need_bytes(&req, basis.avg_bytes_per_s));
    match (disk_need, env.free_disk_bytes) {
        (_, None) => check("disk", Blocked, "free disk space unknown".into()),
        (None, Some(free)) => check(
            "disk",
            CapacityUnverified,
            format!("{:.1} GiB free; need unknown without a data-rate basis", gib(free as f64)),
        ),
        (Some(need), Some(free)) => check(
            "disk",
            if free >= need { Qualified } else { Blocked },
            format!(
                "{:.1} GiB free, need {:.1} GiB (all lanes × {safety:.2} over {}s + disk reserve + verifier disk)",
                gib(free as f64),
                gib(need as f64),
                req.duration_s
            ),
        ),
    }
    // Sustained write rate must carry the summed burst with safety.
    match (&probe, measured, basis.burst_bytes_per_s) {
        (Err(e), _, _) => check("write_rate", Blocked, format!("write probe failed: {e}")),
        (Ok(p), false, _) => check(
            "write_rate",
            CapacityUnverified,
            format!(
                "{:.1} MiB/s sustained (measured); data rate unknown",
                mib(p.bytes_per_s)
            ),
        ),
        (Ok(p), true, None) => check(
            "write_rate",
            if p.bytes_per_s >= basis.avg_bytes_per_s * safety {
                CapacityUnverified
            } else {
                Blocked
            },
            format!(
                "{:.1} MiB/s sustained; burst of a lane without clock unmeasured",
                mib(p.bytes_per_s)
            ),
        ),
        (Ok(p), true, Some(burst)) => check(
            "write_rate",
            if p.bytes_per_s >= burst * safety {
                Qualified
            } else if p.bytes_per_s >= basis.avg_bytes_per_s * safety {
                CapacityUnverified
            } else {
                Blocked
            },
            format!(
                "{:.1} MiB/s sustained vs burst {:.1} KiB/s, avg {:.1} KiB/s (× {safety:.2})",
                mib(p.bytes_per_s),
                burst / 1024.0,
                basis.avg_bytes_per_s / 1024.0
            ),
        ),
    }
    // Sync cost: one sync must finish well inside the writer's stall budget.
    match &probe {
        Err(_) => check("sync_latency", Blocked, "write probe failed".into()),
        Ok(p) => check(
            "sync_latency",
            if p.sync_max_ms * safety <= req.stall_budget_s * 1e3 {
                Qualified
            } else {
                Blocked
            },
            format!(
                "{} sync(s): mean {:.2} ms, max {:.2} ms vs stall budget {:.0}s",
                p.syncs, p.sync_mean_ms, p.sync_max_ms, req.stall_budget_s
            ),
        ),
    }
    // RAM: a market burst during a full stall must fit the hard capture budget.
    match basis.market_burst_bytes_per_s {
        None => check(
            "ram_backlog",
            CapacityUnverified,
            format!(
                "max {:.0} MiB; market burst unmeasured",
                mib(req.max_bytes as f64)
            ),
        ),
        Some(b) => {
            let backlog = 2.0 * b * req.stall_budget_s * safety;
            check(
                "ram_backlog",
                if backlog <= req.max_bytes as f64 { Qualified } else { Blocked },
                format!(
                    "stall {:.0}s × market burst needs {:.1} MiB (payload + batch copy), max {:.0} MiB",
                    req.stall_budget_s,
                    mib(backlog),
                    mib(req.max_bytes as f64)
                ),
            )
        }
    }
    // Economics/winner/Shadow collections: one hard byte budget (overflow → INCOMPLETE).
    check(
        "bounded_collections",
        Qualified,
        format!(
            "econ/winner/Shadow retention ≤ {:.0} MiB (byte budget, overflow is INCOMPLETE); capture lane ≤ {:.0} MiB",
            mib(req.econ_retention_bytes as f64),
            mib(req.max_bytes as f64)
        ),
    );
    let bounded = req.max_bytes + req.econ_retention_bytes + req.system_reserve_bytes;
    match env.physical_bytes {
        Some(p) if bounded <= p => check(
            "physical_memory",
            Qualified,
            format!(
                "{:.1} GiB physical ≥ capture max + econ retention + system reserve ({:.1} GiB)",
                gib(p as f64),
                gib(bounded as f64)
            ),
        ),
        Some(p) => check(
            "physical_memory",
            Blocked,
            format!(
                "{:.1} GiB physical < capture max + econ retention + system reserve ({:.1} GiB)",
                gib(p as f64),
                gib(bounded as f64)
            ),
        ),
        None => check("physical_memory", Blocked, "physical memory unknown".into()),
    }
    match env.memory_pressure {
        Some(l) if l >= PRESSURE_CRITICAL => {
            check("memory_pressure", Blocked, format!("level {l} (critical)"))
        }
        Some(l) if l >= PRESSURE_WARN => check(
            "memory_pressure",
            CapacityUnverified,
            format!("level {l} (warning)"),
        ),
        Some(l) => check("memory_pressure", Qualified, format!("level {l}")),
        None => check(
            "memory_pressure",
            CapacityUnverified,
            "not reported by the OS".into(),
        ),
    }
    match env.swap_used_bytes {
        Some(s) if s <= req.system_reserve_bytes => check(
            "swap",
            Qualified,
            format!("{:.2} GiB in use ≤ system reserve", gib(s as f64)),
        ),
        Some(s) => check(
            "swap",
            CapacityUnverified,
            format!(
                "{:.2} GiB in use > system reserve: host under memory load",
                gib(s as f64)
            ),
        ),
        None => check(
            "swap",
            CapacityUnverified,
            "swap usage not reported by the OS".into(),
        ),
    }
    match env.load_1m {
        Some(load) if load <= env.cores as f64 * 0.75 => check(
            "cpu",
            Qualified,
            format!("load {load:.2} on {} cores", env.cores),
        ),
        Some(load) => check(
            "cpu",
            CapacityUnverified,
            format!(
                "load {load:.2} on {} cores: other active processes",
                env.cores
            ),
        ),
        None => check("cpu", CapacityUnverified, "load average unavailable".into()),
    }
    let (verdict, detail) = verification_check(&req, &basis, strict_basis.as_ref(), safety);
    check("verification", verdict, detail);
    check(
        "provider",
        NotRun,
        "capture provider capacity (WSS liveness, 429, FREE_ONLY quota, archive) not probed: BOOT-04 network audit needs explicit permission".into(),
    );
    let verdict = if checks.iter().any(|c| c.verdict == Blocked) {
        Blocked
    } else if checks
        .iter()
        .any(|c| matches!(c.verdict, CapacityUnverified | NotRun))
    {
        CapacityUnverified
    } else {
        Qualified
    };
    let start_allowed = verdict == Qualified
        || (verdict == CapacityUnverified && req.duration_s <= CALIBRATION_MAX_S);
    CapacityReport {
        request: req,
        basis,
        strict_basis,
        probe,
        environment: env,
        disk_need_bytes: disk_need,
        checks,
        verdict,
        start_allowed,
    }
}

/// Measure, evaluate and keep the report (`capacity_preflight.p0j` in `out_dir`, immutable).
pub fn run(
    req: Request,
    basis_runs: &[PathBuf],
    probe_bytes: u64,
) -> Result<CapacityReport, String> {
    std::fs::create_dir_all(&req.out_dir).map_err(|e| e.to_string())?;
    let basis = measure_basis(basis_runs, &req.capture_source);
    let strict_dirs: Vec<_> = basis
        .runs
        .iter()
        .map(|r| r.root.join("verification"))
        .collect();
    let strict = measure_strict(&strict_dirs);
    let probe = probe_write_rate(&req.out_dir, probe_bytes);
    let env = environment(&req.out_dir);
    let report = evaluate(req, basis, strict, probe, env);
    let path = report.request.out_dir.join(REPORT_JOURNAL);
    journal::write_typed_value(&path, source::STORAGE, &report).map_err(|e| e.to_string())?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::observer::rows::{CaptureStart, MarketTransaction, TxOrigin};
    use darkbot_solana::chain::{ChainLogs, TxStatus};

    fn req(duration_s: u64, mode: VerifyMode) -> Request {
        Request {
            duration_s,
            out_dir: std::env::temp_dir(),
            capture_source: "logs".into(),
            verify: VerifyBudget {
                mode,
                rps: 2,
                calls: 2000,
                seconds: 300,
                max_pages: 50,
                bytes: 256 * MIB,
                disk_bytes: 8192 * MIB,
            },
            initial_bytes: 64 * MIB,
            max_bytes: 512 * MIB,
            system_reserve_bytes: 4096 * MIB,
            disk_reserve_bytes: 2048 * MIB,
            stall_budget_s: 30.0,
            safety_percent: 50,
            econ_retention_bytes: 1024 * MIB,
        }
    }
    fn env() -> Environment {
        Environment {
            free_disk_bytes: Some(500 * 1024 * MIB),
            physical_bytes: Some(32 * 1024 * MIB),
            memory_pressure: Some(1),
            swap_used_bytes: Some(0),
            cores: 8,
            load_1m: Some(1.0),
        }
    }
    fn probe(bytes_per_s: f64) -> Result<WriteProbe, String> {
        Ok(WriteProbe {
            bytes: 64 * MIB,
            seconds: 1.0,
            bytes_per_s,
            syncs: 4,
            sync_mean_ms: 2.0,
            sync_max_ms: 5.0,
        })
    }
    fn basis(avg: f64, burst: f64) -> RateBasis {
        RateBasis {
            runs: vec![BasisRun {
                root: "/r".into(),
                source: "logs".into(),
                span_s: 600.0,
                bytes: (avg * 600.0) as u64,
                program_transactions: 60,
                slots: Some((1, 1501)),
            }],
            lanes: vec![LaneRate {
                lane: "market".into(),
                journals: 1,
                frames: 1000,
                bytes: (avg * 600.0) as u64,
                avg_bytes_per_s: avg,
                burst_bytes_per_s: Some(burst),
            }],
            span_s: 600.0,
            bytes: (avg * 600.0) as u64,
            avg_bytes_per_s: avg,
            burst_bytes_per_s: Some(burst),
            market_burst_bytes_per_s: Some(burst),
            program_tx_per_s: Some(0.1),
            slots_per_s: Some(2.5),
            ..Default::default()
        }
    }
    fn verdict_of(r: &CapacityReport, name: &str) -> Verdict {
        r.checks.iter().find(|c| c.name == name).unwrap().verdict
    }

    /// No basis: a long run never starts, a calibration run may. The verifier budget is sized
    /// for the selected mode only: off is never blocked by a strict budget, and 2,000 calls for
    /// a four-hour strict verification (~36,000 slots) are refused as not canonical-qualifiable.
    #[test]
    fn verdicts_follow_measured_inputs_and_mode() {
        let r = evaluate(
            req(14_400, VerifyMode::Off),
            RateBasis::default(),
            None,
            probe(200.0 * MIB as f64),
            env(),
        );
        assert_eq!(
            (r.verdict, r.start_allowed),
            (Verdict::CapacityUnverified, false)
        );
        let r = evaluate(
            req(600, VerifyMode::Off),
            RateBasis::default(),
            None,
            probe(200.0 * MIB as f64),
            env(),
        );
        assert_eq!(
            (r.verdict, r.start_allowed),
            (Verdict::CapacityUnverified, true)
        );
        let b = || basis(50_000.0, 400_000.0);
        let r = evaluate(
            req(14_400, VerifyMode::Off),
            b(),
            None,
            probe(200.0 * MIB as f64),
            env(),
        );
        assert_eq!(verdict_of(&r, "verification"), Verdict::NotApplicable);
        assert_eq!(verdict_of(&r, "disk"), Verdict::Qualified);
        // Open capability and the missing network probe keep a long run unverified (fail closed).
        assert_eq!(
            (r.verdict, r.start_allowed),
            (Verdict::CapacityUnverified, false),
            "{}",
            r.terminal()
        );
        let r = evaluate(
            req(14_400, VerifyMode::Strict),
            b(),
            None,
            probe(200.0 * MIB as f64),
            env(),
        );
        assert_eq!(verdict_of(&r, "verification"), Verdict::Blocked);
        assert!(r.terminal().contains("CANONICAL_VERIFIED not reachable"));
        assert!(!r.start_allowed);
        // Index over 4 h: 0.1 tx/s × 14,400 s × 1.5 = 2,160 signatures > 2,000 calls → refused;
        // with a sufficient budget it qualifies.
        let r = evaluate(
            req(14_400, VerifyMode::Index),
            b(),
            None,
            probe(200.0 * MIB as f64),
            env(),
        );
        assert_eq!(verdict_of(&r, "verification"), Verdict::Blocked);
        let mut index = req(14_400, VerifyMode::Index);
        (index.verify.calls, index.verify.seconds) = (5_000, 3_000);
        let r = evaluate(index.clone(), b(), None, probe(200.0 * MIB as f64), env());
        assert_eq!(
            verdict_of(&r, "verification"),
            Verdict::Qualified,
            "{}",
            r.terminal()
        );
        // DEC-08: a crosscheck reads each source within that source's own budget (exactly the
        // index sizing, 2,160 signatures + 3 pages, qualifies it; one call less does not), and
        // keeps a checkpoint per source (twice the verifier's disk).
        let mut crosscheck = Request {
            verify: VerifyBudget {
                mode: VerifyMode::Crosscheck,
                calls: 2_162,
                ..index.verify.clone()
            },
            ..index.clone()
        };
        let r = evaluate(
            crosscheck.clone(),
            b(),
            None,
            probe(200.0 * MIB as f64),
            env(),
        );
        assert_eq!(verdict_of(&r, "verification"), Verdict::Blocked);
        crosscheck.verify.calls = 2_163;
        let r = evaluate(
            crosscheck.clone(),
            b(),
            None,
            probe(200.0 * MIB as f64),
            env(),
        );
        assert_eq!(
            verdict_of(&r, "verification"),
            Verdict::Qualified,
            "{}",
            r.terminal()
        );
        assert_eq!(
            disk_need_bytes(&crosscheck, 1.0) - disk_need_bytes(&index, 1.0),
            index.verify.disk_bytes
        );
        // Strict with measured per-slot cost and a large budget: qualified sizing.
        let mut strict = req(14_400, VerifyMode::Strict);
        (
            strict.verify.calls,
            strict.verify.seconds,
            strict.verify.rps,
        ) = (100_000, 100_000, 5);
        (strict.verify.bytes, strict.verify.disk_bytes) = (200 * 1024 * MIB, 200 * 1024 * MIB);
        let sb = StrictBasis {
            results: 1,
            slots: 64,
            calls: 70,
            response_bytes: 64 * MIB,
        };
        let r = evaluate(strict, b(), Some(sb), probe(200.0 * MIB as f64), env());
        assert_eq!(
            verdict_of(&r, "verification"),
            Verdict::Qualified,
            "{}",
            r.terminal()
        );
        // Disk too small for 4 h at the measured rate; burst backlog over a stall above max;
        // write rate below the average: each blocks.
        let mut small = env();
        small.free_disk_bytes = Some(2 * 1024 * MIB);
        let r = evaluate(
            req(14_400, VerifyMode::Off),
            b(),
            None,
            probe(200.0 * MIB as f64),
            small,
        );
        assert_eq!(
            (verdict_of(&r, "disk"), r.verdict),
            (Verdict::Blocked, Verdict::Blocked)
        );
        let r = evaluate(
            req(14_400, VerifyMode::Off),
            basis(50_000.0, 20.0 * MIB as f64),
            None,
            probe(400.0 * MIB as f64),
            env(),
        );
        assert_eq!(verdict_of(&r, "ram_backlog"), Verdict::Blocked);
        let r = evaluate(
            req(14_400, VerifyMode::Off),
            b(),
            None,
            probe(40_000.0),
            env(),
        );
        assert_eq!(verdict_of(&r, "write_rate"), Verdict::Blocked);
        let r = evaluate(
            req(14_400, VerifyMode::Off),
            b(),
            None,
            Err("EIO".into()),
            env(),
        );
        assert_eq!(verdict_of(&r, "sync_latency"), Verdict::Blocked);
    }

    /// The basis is measured from sealed journals of a run directory in the current format and
    /// of the requested capture source; unsealed or foreign-source runs are skipped with their
    /// reason; the write probe measures sync cost and leaves no file behind.
    #[test]
    fn basis_and_write_probe_are_measured() {
        let d = std::env::temp_dir().join(format!(
            "p0_cap_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let write_run = |name: &str, source: &str, seal: bool| {
            let dir = d.join(name).join("capture/market");
            std::fs::create_dir_all(&dir).unwrap();
            let mut w = journal::Writer::create(&dir.join("market.p0j")).unwrap();
            let start = MarketCoverage::Window {
                observed_unix_s: 0,
                t_rx_ns: Some(0),
                observing: true,
                counted: false,
                event: WindowEvent::Start(Box::new(CaptureStart {
                    source: source.into(),
                    commitment: "confirmed".into(),
                    program: Default::default(),
                    configured_duration_seconds: 60,
                    ws_host: "h".into(),
                    recovery_budget_ms: None,
                    recovery_attempts: 0,
                    idle_timeout_ms: 0,
                    healthy_reset_ms: 0,
                    readiness: "r".into(),
                    provider_budget: darkbot_solana::provider::ProviderSnapshot {
                        scope: "s".into(),
                        requests: 0,
                        http_429_count: 0,
                        in_flight: 0,
                        queued: 0,
                        max_rps: 0,
                        effective_rps: 0.0,
                        max_concurrency: 0,
                        burst: 0,
                        bytes: 0,
                        quota_qualified: false,
                    },
                })),
            };
            w.append_typed(
                Provenance {
                    rx_ns: Some(0),
                    ..Default::default()
                },
                &start,
            )
            .unwrap();
            for i in 1..=60u64 {
                let p = Provenance {
                    rx_ns: Some(i * 1_000_000_000),
                    slot: Some(100 + i * 3),
                    ..Default::default()
                };
                let tx = MarketTransaction {
                    signature: format!("s{i}"),
                    slot: 100 + i * 3,
                    origin: TxOrigin::LogsSubscribe {
                        observed_unix_s: i,
                        t_rx_ns: i * 1_000_000_000,
                        logs: ChainLogs {
                            slot: 100 + i * 3,
                            signature: format!("s{i}"),
                            status: TxStatus::Success,
                            logs: Some(vec![]),
                        },
                    },
                };
                w.append_typed(p, &tx).unwrap();
            }
            if seal {
                w.seal(true).unwrap();
            }
        };
        write_run("a_logs", "logs", true);
        write_run("b_tx", "transactions", true);
        write_run("c_crashed", "logs", false);
        let candidates = [
            d.join("c_crashed"),
            d.join("b_tx"),
            d.join("a_logs"),
            d.join("empty"),
        ];
        let b = measure_basis(&candidates, "logs");
        assert_eq!(b.runs.len(), 1, "{:?}", b.skipped);
        assert_eq!(b.skipped_total, 3);
        assert!(b.skipped.iter().any(|s| s.reason.starts_with("UNSEALED")));
        assert!(b
            .skipped
            .iter()
            .any(|s| s.reason.starts_with("CAPTURE_SOURCE_MISMATCH")));
        assert_eq!(b.runs[0].program_transactions, 60);
        assert!((b.span_s - 60.0).abs() < 1e-9);
        assert!((b.program_tx_per_s.unwrap() - 1.0).abs() < 1e-9);
        assert!((b.slots_per_s.unwrap() - 177.0 / 60.0).abs() < 1e-9);
        assert!(b.burst_bytes_per_s.unwrap() >= b.avg_bytes_per_s);
        assert_eq!(measure_basis(&[], "logs"), RateBasis::default());
        let p = probe_write_rate(&d, 2 * MIB).unwrap();
        assert!(p.bytes_per_s > 0.0 && p.syncs >= 1 && p.sync_max_ms >= p.sync_mean_ms);
        assert!(
            std::fs::read_dir(&d).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with('.')),
            "probe removed"
        );
        // The kept report is immutable and the record; its configuration render is derived,
        // byte-stable and never overwritten.
        let mut r = req(600, VerifyMode::Off);
        r.out_dir = d.join("new/capture");
        let report = run(r, &[d.join("a_logs")], 2 * MIB).unwrap();
        assert!(
            report.start_allowed && report.basis.measured(),
            "{}",
            report.terminal()
        );
        let kept = d.join("new/capture").join(REPORT_JOURNAL);
        assert_eq!(
            journal::read_typed_value::<CapacityReport>(&kept).unwrap(),
            report
        );
        std::fs::remove_dir_all(d).unwrap();
    }
}

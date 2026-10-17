//! SK-06: capacity preflight before a capture of a given duration.
//!
//! Every input is measured or configured, never assumed: the data rate (average and the
//! highest 10 s burst) comes from sealed typed market journals of earlier real captures; the
//! sustained write rate is measured now on the target volume through the journal codec with
//! fsync; disk, memory, pressure and CPU come from the OS; budgets from the operational config.
//! Without a measured data-rate basis the verdict is CAPACITY_UNVERIFIED. A capture longer than
//! `CALIBRATION_MAX_S` starts only when QUALIFIED; a short calibration capture may start
//! UNVERIFIED (it produces the basis). The result is printed and kept as a typed journal.
use super::budget::{free_disk_bytes, physical_memory_bytes, MemorySample, MIB, PRESSURE_WARN};
use super::writer::WriterConfig;
use crate::analytics::journal::{self, kind, source, Provenance};
use crate::analytics::payload::{self, Payload};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Longest capture that may start without a measured capacity basis (it calibrates).
pub const CALIBRATION_MAX_S: u64 = 600;
/// Burst window over the frames' receive clock.
const BURST_WINDOW_NS: u64 = 10_000_000_000;
/// Nominal Solana slot time used only to size the strict verifier's call budget (an estimate;
/// the result says so).
const NOMINAL_SLOT_MS: u64 = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    Qualified,
    CapacityUnverified,
    Blocked,
}
impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Qualified => "QUALIFIED",
            Verdict::CapacityUnverified => "CAPACITY_UNVERIFIED",
            Verdict::Blocked => "BLOCKED",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub verdict: Verdict,
    pub detail: String,
}

/// Data rate measured from sealed typed market journals of earlier captures.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RateBasis {
    pub journals: u64,
    pub frames: u64,
    pub bytes: u64,
    pub span_s: f64,
    pub avg_bytes_per_s: f64,
    pub burst_bytes_per_s: f64,
    /// Journals not counted: unsealed/incomplete (crashed), unreadable, or historical
    /// schema-1 frames (another byte format).
    pub skipped: u64,
}

/// OS facts at preflight time.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub free_disk_bytes: Option<u64>,
    pub physical_bytes: Option<u64>,
    pub memory_pressure: Option<i32>,
    pub cores: u64,
    pub load_1m: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub duration_s: u64,
    pub out_dir: PathBuf,
    pub initial_bytes: u64,
    pub max_bytes: u64,
    pub system_reserve_bytes: u64,
    pub disk_reserve_bytes: u64,
    pub stall_budget_s: f64,
    pub safety_percent: u64,
    pub verify_disk_bytes: u64,
    pub verify_calls: u64,
}

impl Request {
    pub fn from_active(duration_s: u64, out_dir: &Path) -> Self {
        let w = WriterConfig::from_active();
        let l = &darkbot_solana::runtime_config::active().limits;
        Self {
            duration_s,
            out_dir: out_dir.into(),
            initial_bytes: w.initial_bytes,
            max_bytes: w.max_bytes,
            system_reserve_bytes: w.system_reserve_bytes,
            disk_reserve_bytes: w.disk_reserve_bytes,
            stall_budget_s: w.stall_budget.as_secs_f64(),
            safety_percent: l.capacity_safety_percent,
            verify_disk_bytes: l.verify_disk_mib * MIB,
            verify_calls: l.verify_calls,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CapacityReport {
    pub request: Request,
    pub basis: Option<RateBasis>,
    pub write_bytes_per_s: Option<f64>,
    pub environment: Environment,
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
        let mut out = format!(
            "PROJECT 0 CAPACITY PREFLIGHT duration={}s verdict={} start_allowed={}\n",
            self.request.duration_s,
            self.verdict.as_str(),
            self.start_allowed
        );
        for c in &self.checks {
            out.push_str(&format!(
                "  {:<22} {:<20} {}\n",
                c.name,
                c.verdict.as_str(),
                c.detail
            ));
        }
        out
    }
}

/// Average and highest 10 s burst rate of sealed typed market journals (receive-clock span of
/// their live frames; frames without a receive time count to the byte total only). Journals that
/// are unreadable, unsealed/incomplete or hold schema-1 frames are skipped and counted.
pub fn measure_rate(paths: &[PathBuf]) -> Option<RateBasis> {
    let mut b = RateBasis::default();
    let mut burst: f64 = 0.0;
    for p in paths {
        let Some((frames, bytes, timed)) = typed_frames(p) else {
            b.skipped += 1;
            continue;
        };
        let (Some(first), Some(last)) = (timed.first(), timed.last()) else {
            b.skipped += 1;
            continue;
        };
        let span = last.0.saturating_sub(first.0);
        if span < BURST_WINDOW_NS {
            b.skipped += 1; // too short to measure a rate
            continue;
        }
        let (mut lo, mut window) = (0, 0u64);
        for hi in 0..timed.len() {
            window += timed[hi].1;
            while timed[hi].0 - timed[lo].0 > BURST_WINDOW_NS {
                window -= timed[lo].1;
                lo += 1;
            }
            burst = burst.max(window as f64 / (BURST_WINDOW_NS as f64 / 1e9));
        }
        b.journals += 1;
        b.frames += frames;
        b.bytes += bytes;
        b.span_s += span as f64 / 1e9;
    }
    if b.journals == 0 || b.span_s <= 0.0 {
        return None;
    }
    b.avg_bytes_per_s = b.bytes as f64 / b.span_s;
    b.burst_bytes_per_s = burst.max(b.avg_bytes_per_s);
    Some(b)
}

/// `(frames, bytes, sorted (rx_ns, bytes))` of a sealed complete typed journal.
fn typed_frames(p: &Path) -> Option<(u64, u64, Vec<(u64, u64)>)> {
    let mut r = journal::Reader::open(p).ok()?;
    let (mut frames, mut bytes, mut timed) = (0, 0, Vec::new());
    while let Some(f) = r.next_frame().ok()? {
        if f.kind == journal::MANIFEST {
            continue;
        }
        if f.schema < payload::TYPED {
            return None;
        }
        frames += 1;
        bytes += f.bytes;
        if let Some(t) = f.provenance.rx_ns {
            timed.push((t, f.bytes));
        }
    }
    if !(r.sealed && r.complete) {
        return None;
    }
    timed.sort_unstable();
    Some((frames, bytes, timed))
}

/// Sustained write rate measured on `dir`'s volume: `bytes` of real journal frames in batches,
/// fsynced every 8 batches and at seal. The probe journal is removed afterwards.
pub fn probe_write_rate(dir: &Path, bytes: u64) -> Result<f64, String> {
    let path = dir.join(format!(".capacity-probe-{}.p0j", std::process::id()));
    let result = (|| -> Result<f64, String> {
        let mut w = journal::Writer::create(&path).map_err(|e| e.to_string())?;
        let row = crate::market::observer::rows::MarketCoverage::SlotFailed {
            slot: 0,
            error: "x".repeat(256),
        };
        let encoded = payload::encode(&row).map_err(|e| e.to_string())?;
        let p = Provenance::default();
        let batch: Vec<_> = (0..256)
            .map(|_| {
                (
                    crate::market::observer::rows::MarketCoverage::KIND,
                    crate::market::observer::rows::MarketCoverage::SCHEMA,
                    &p,
                    encoded.as_slice(),
                )
            })
            .collect();
        let started = Instant::now();
        let mut n = 0u64;
        while w.position() < bytes {
            w.append_batch(batch.iter().copied())
                .map_err(|e| e.to_string())?;
            n += 1;
            if n % 8 == 0 {
                w.sync().map_err(|e| e.to_string())?;
            }
        }
        w.seal(true).map_err(|e| e.to_string())?;
        let secs = started.elapsed().as_secs_f64().max(1e-6);
        Ok(w.position() as f64 / secs)
    })();
    let _ = std::fs::remove_file(&path);
    result
}

pub fn environment(dir: &Path) -> Environment {
    let mut load = [0f64; 1];
    // SAFETY: getloadavg writes at most one sample into the array we own.
    let samples = unsafe { libc::getloadavg(load.as_mut_ptr(), 1) };
    Environment {
        free_disk_bytes: free_disk_bytes(dir),
        physical_bytes: physical_memory_bytes(),
        memory_pressure: MemorySample::now().pressure_level,
        cores: std::thread::available_parallelism()
            .map(|n| n.get() as u64)
            .unwrap_or(1),
        load_1m: (samples == 1).then_some(load[0]),
    }
}

/// The capacity decision from measured inputs (pure).
pub fn evaluate(
    req: Request,
    basis: Option<RateBasis>,
    write_bytes_per_s: Option<f64>,
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
    match &basis {
        None => check(
            "data_rate",
            CapacityUnverified,
            "no sealed typed market journal of an earlier capture; run a calibration capture first"
                .into(),
        ),
        Some(b) => check(
            "data_rate",
            Qualified,
            format!(
                "avg {:.1} KiB/s, burst {:.1} KiB/s over {:.0}s of {} journal(s)",
                b.avg_bytes_per_s / 1024.0,
                b.burst_bytes_per_s / 1024.0,
                b.span_s,
                b.journals
            ),
        ),
    }
    // Disk: journal bytes for the duration (with safety) + disk reserve + verification lane.
    match (&basis, env.free_disk_bytes) {
        (_, None) => check("disk", Blocked, "free disk space unknown".into()),
        (None, Some(free)) => check(
            "disk",
            CapacityUnverified,
            format!(
                "{:.1} GiB free; need unknown without a data-rate basis",
                gib(free as f64)
            ),
        ),
        (Some(b), Some(free)) => {
            let need = b.avg_bytes_per_s * req.duration_s as f64 * safety
                + req.disk_reserve_bytes as f64
                + req.verify_disk_bytes as f64;
            check(
                "disk",
                if free as f64 >= need { Qualified } else { Blocked },
                format!("{:.1} GiB free, need {:.1} GiB (journal × {:.2} + disk reserve + verifier disk)", gib(free as f64), gib(need), safety),
            )
        }
    }
    // Sustained write rate must carry the burst with safety; otherwise the backlog must fit max.
    match (&basis, write_bytes_per_s) {
        (_, None) => check("write_rate", Blocked, "write probe failed".into()),
        (None, Some(w)) => check(
            "write_rate",
            CapacityUnverified,
            format!(
                "{:.1} MiB/s sustained (measured); burst unknown",
                w / MIB as f64
            ),
        ),
        (Some(b), Some(w)) => {
            let verdict = if w >= b.burst_bytes_per_s * safety {
                Qualified
            } else if w >= b.avg_bytes_per_s * safety {
                CapacityUnverified
            } else {
                Blocked
            };
            check(
                "write_rate",
                verdict,
                format!(
                    "{:.1} MiB/s sustained vs burst {:.1} KiB/s, avg {:.1} KiB/s (× {safety:.2})",
                    w / MIB as f64,
                    b.burst_bytes_per_s / 1024.0,
                    b.avg_bytes_per_s / 1024.0
                ),
            )
        }
    }
    // RAM: a burst during a full stall must fit the hard capture budget.
    match &basis {
        None => check(
            "ram_backlog",
            CapacityUnverified,
            format!(
                "max {:.0} MiB; burst backlog unknown",
                req.max_bytes as f64 / MIB as f64
            ),
        ),
        Some(b) => {
            let backlog = 2.0 * b.burst_bytes_per_s * req.stall_budget_s * safety;
            check(
                "ram_backlog",
                if backlog <= req.max_bytes as f64 {
                    Qualified
                } else {
                    Blocked
                },
                format!(
                    "stall {:.0}s × burst needs {:.1} MiB (payload + batch copy), max {:.0} MiB",
                    req.stall_budget_s,
                    backlog / MIB as f64,
                    req.max_bytes as f64 / MIB as f64
                ),
            )
        }
    }
    match env.physical_bytes {
        Some(p) if req.max_bytes + req.system_reserve_bytes <= p => check(
            "physical_memory",
            Qualified,
            format!("{:.1} GiB physical ≥ max + system reserve", gib(p as f64)),
        ),
        Some(p) => check(
            "physical_memory",
            Blocked,
            format!("{:.1} GiB physical < max + system reserve", gib(p as f64)),
        ),
        None => check("physical_memory", Blocked, "physical memory unknown".into()),
    }
    match env.memory_pressure {
        Some(l) if l >= 4 => check("memory_pressure", Blocked, format!("level {l} (critical)")),
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
    // Strict verifier: one getBlock per produced slot at a nominal slot time (estimate).
    let slots = req.duration_s * 1000 / NOMINAL_SLOT_MS;
    check(
        "strict_call_budget",
        if req.verify_calls >= slots { Qualified } else { CapacityUnverified },
        format!(
            "{} configured calls vs ~{} slots (estimate, {} ms nominal); provider archive qualification separate",
            req.verify_calls, slots, NOMINAL_SLOT_MS
        ),
    );
    let verdict = if checks.iter().any(|c| c.verdict == Blocked) {
        Blocked
    } else if checks.iter().any(|c| c.verdict == CapacityUnverified) {
        CapacityUnverified
    } else {
        Qualified
    };
    let start_allowed = verdict == Qualified
        || (verdict == CapacityUnverified && req.duration_s <= CALIBRATION_MAX_S);
    CapacityReport {
        request: req,
        basis,
        write_bytes_per_s,
        environment: env,
        checks,
        verdict,
        start_allowed,
    }
}

/// Measure, evaluate and keep the report (`capacity_preflight.p0j` in `out_dir`).
pub fn run(
    req: Request,
    basis_journals: &[PathBuf],
    probe_bytes: u64,
) -> Result<CapacityReport, String> {
    std::fs::create_dir_all(&req.out_dir).map_err(|e| e.to_string())?;
    let basis = measure_rate(basis_journals);
    let write = probe_write_rate(&req.out_dir, probe_bytes).ok();
    let env = environment(&req.out_dir);
    let report = evaluate(req, basis, write, env);
    let path = report.request.out_dir.join("capacity_preflight.p0j");
    journal::write_typed_value(&path, source::STORAGE, &report).map_err(|e| e.to_string())?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(duration_s: u64) -> Request {
        Request {
            duration_s,
            out_dir: std::env::temp_dir(),
            initial_bytes: 64 * MIB,
            max_bytes: 512 * MIB,
            system_reserve_bytes: 4096 * MIB,
            disk_reserve_bytes: 2048 * MIB,
            stall_budget_s: 30.0,
            safety_percent: 50,
            verify_disk_bytes: 8192 * MIB,
            verify_calls: 2000,
        }
    }
    fn env() -> Environment {
        Environment {
            free_disk_bytes: Some(500 * 1024 * MIB),
            physical_bytes: Some(32 * 1024 * MIB),
            memory_pressure: Some(1),
            cores: 8,
            load_1m: Some(1.0),
        }
    }
    fn basis(avg: f64, burst: f64) -> RateBasis {
        RateBasis {
            journals: 1,
            frames: 1000,
            bytes: (avg * 600.0) as u64,
            span_s: 600.0,
            avg_bytes_per_s: avg,
            burst_bytes_per_s: burst,
            skipped: 0,
        }
    }

    /// Without a measured basis a long run never starts; a calibration run may. Measured inputs
    /// qualify or block per check; the strict call budget is reported as an estimate.
    #[test]
    fn verdicts_follow_measured_inputs() {
        let r = evaluate(req(14_400), None, Some(200.0 * MIB as f64), env());
        assert_eq!(
            (r.verdict, r.start_allowed),
            (Verdict::CapacityUnverified, false)
        );
        let r = evaluate(req(600), None, Some(200.0 * MIB as f64), env());
        assert_eq!(
            (r.verdict, r.start_allowed),
            (Verdict::CapacityUnverified, true)
        );
        let mut long = req(14_400);
        long.verify_calls = 40_000;
        let r = evaluate(
            long.clone(),
            Some(basis(50_000.0, 400_000.0)),
            Some(200.0 * MIB as f64),
            env(),
        );
        assert_eq!(r.verdict, Verdict::Qualified, "{}", r.terminal());
        assert!(r.start_allowed);
        // Disk too small for 4 h at the measured rate: blocked.
        let mut small = env();
        small.free_disk_bytes = Some(11 * 1024 * MIB);
        let r = evaluate(
            long.clone(),
            Some(basis(50_000.0, 400_000.0)),
            Some(200.0 * MIB as f64),
            small,
        );
        assert_eq!(r.verdict, Verdict::Blocked);
        // Burst backlog over a full stall exceeding max: blocked.
        let r = evaluate(
            long.clone(),
            Some(basis(50_000.0, 20.0 * MIB as f64)),
            Some(400.0 * MIB as f64),
            env(),
        );
        assert!(r
            .checks
            .iter()
            .any(|c| c.name == "ram_backlog" && c.verdict == Verdict::Blocked));
        // Write rate below the average: blocked; between average and burst: unverified.
        let r = evaluate(
            long.clone(),
            Some(basis(50_000.0, 400_000.0)),
            Some(40_000.0),
            env(),
        );
        assert!(r
            .checks
            .iter()
            .any(|c| c.name == "write_rate" && c.verdict == Verdict::Blocked));
        let r = evaluate(
            long,
            Some(basis(50_000.0, 400_000.0)),
            Some(100_000.0),
            env(),
        );
        assert!(r
            .checks
            .iter()
            .any(|c| c.name == "write_rate" && c.verdict == Verdict::CapacityUnverified));
        assert!(r.terminal().contains("write_rate"));
    }

    /// The rate basis is measured from a sealed typed journal's receive clock; the write probe
    /// measures the volume and leaves no file behind.
    #[test]
    fn rate_basis_and_write_probe_are_measured() {
        let d = std::env::temp_dir().join(format!(
            "p0_cap_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        let path = d.join("market.p0j");
        let mut w = journal::Writer::create(&path).unwrap();
        let row = crate::market::observer::rows::MarketCoverage::SlotFailed {
            slot: 1,
            error: "e".into(),
        };
        for i in 0..60u64 {
            let p = Provenance {
                rx_ns: Some(i * 1_000_000_000),
                ..Default::default()
            };
            w.append_typed(p, &row).unwrap();
        }
        w.seal(true).unwrap();
        let b = measure_rate(&[path.clone(), d.join("missing.p0j")]).unwrap();
        assert_eq!(b.skipped, 1);
        assert_eq!((b.journals, b.frames), (1, 60));
        assert!((b.span_s - 59.0).abs() < 1e-9 && b.burst_bytes_per_s >= b.avg_bytes_per_s);
        assert_eq!(measure_rate(&[]), None);
        let rate = probe_write_rate(&d, 2 * MIB).unwrap();
        assert!(rate > 0.0);
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1, "probe removed");
        std::fs::remove_dir_all(d).unwrap();
    }
}

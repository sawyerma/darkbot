//! The capacity preflight of a planned scoped capture (master plan §11,
//! storage contract; Phase F contract §3/§4): before a run of the planned
//! duration starts, its journal must fit the free disk above the reserve,
//! the writer must sustain the capture rate and a burst must fit the
//! capture's byte budget. Every input is a measurement of this host:
//!
//! - the rates of the newest run sealed by a `COMPLETE` manifest: its
//!   journal bytes over its record time (average), its records read again
//!   and bucketed per second of record time (the peak second, the longest
//!   run of seconds above the average as the burst), and the manifest
//!   bytes against the journal bytes (overhead);
//! - the writer throughput just measured on the capture volume: records of
//!   that run's average size through the production `SegmentWriter`
//!   (append, synced by the configured sync policy, sealed with its
//!   read-back hash), timed, in bytes and in records per second;
//! - the backlog that trace builds when drained at that writer rate, in
//!   bytes against the byte budget and in records against the queue;
//! - the volume's free bytes (`df -Pk`).
//!
//! Without a sealed run there is no rate and the run is `UNMEASURED`. The
//! verdict is the journal crate's `preflight`.

use crate::runtime_journal::RUN_MANIFEST;
use darkbot_bsc_journal::storage::preflight::{
    CapacityInputs, Preflight, Resource, measured_backlog, preflight,
};
use darkbot_bsc_journal::storage::run::RunReader;
use darkbot_bsc_journal::storage::segments::{SegmentPolicy, SegmentWriter};
use darkbot_bsc_journal::storage::writer::SyncPolicy;
use darkbot_bsc_journal::{CaptureRecord, RecordKind};
use darkbot_bsc_record::Draft;
use darkbot_bsc_record::schema::bsc_capture::{capture_preflight as cp, capture_run as cr};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

/// Capture rates measured from one sealed run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rates {
    pub run_id: String,
    pub average_bytes_per_s: u64,
    pub peak_bytes_per_s: u64,
    pub burst_s: u64,
    pub overhead_ppm: u64,
    /// Journal bytes per frame.
    pub average_record_bytes: u64,
    /// The run's frame bytes and frames per second of record time.
    pub bytes_per_second: Vec<u64>,
    pub records_per_second: Vec<u64>,
}

/// The frame bytes and frames of `run`'s records per second of record time.
fn per_second(run: &Path, max_payload: usize) -> Result<(Vec<u64>, Vec<u64>), String> {
    let mut reader =
        RunReader::open(run, max_payload).map_err(|e| format!("{}: {e}", run.display()))?;
    let (mut bytes, mut records): (Vec<u64>, Vec<u64>) = (Vec::new(), Vec::new());
    let mut first: Option<u64> = None;
    while let Some(r) = reader
        .next_record()
        .map_err(|e| format!("{}: {e}", run.display()))?
    {
        // Gap spans carry no time; every other record its own.
        if r.timestamp_ns == 0 {
            continue;
        }
        let start = *first.get_or_insert(r.timestamp_ns);
        let second = usize::try_from(r.timestamp_ns.saturating_sub(start) / 1_000_000_000)
            .map_err(|_| "record time beyond the address space".to_string())?;
        if second >= bytes.len() {
            bytes.resize(second + 1, 0);
            records.resize(second + 1, 0);
        }
        bytes[second] += (darkbot_bsc_journal::HEADER_LEN + r.payload.len()) as u64;
        records[second] += 1;
    }
    Ok((bytes, records))
}

fn rate_of(bytes: u64, span_ns: u64) -> u64 {
    u64::try_from((u128::from(bytes) * 1_000_000_000).div_ceil(u128::from(span_ns.max(1))))
        .unwrap_or(u64::MAX)
}

/// The rates of the newest run of `journal_dir` sealed by a `COMPLETE`
/// manifest whose records span time; `None` when there is none.
pub fn rates(journal_dir: &Path, max_payload: usize) -> Result<Option<Rates>, String> {
    let mut runs: Vec<u64> = Vec::new();
    for entry in fs::read_dir(journal_dir).map_err(|e| format!("{}: {e}", journal_dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", journal_dir.display()))?;
        if entry.path().is_dir()
            && let Ok(id) = entry.file_name().to_string_lossy().parse::<u64>()
        {
            runs.push(id);
        }
    }
    runs.sort_unstable_by(|a, b| b.cmp(a));
    for id in runs {
        let path = journal_dir.join(id.to_string()).join(RUN_MANIFEST);
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(m) = cr::decode_run(&bytes) else {
            continue;
        };
        if m.text(cr::STATE).ok() != Some("COMPLETE") {
            continue;
        }
        let segments = m
            .nested_all(cr::SEGMENTS)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let span = |s: &darkbot_bsc_record::Record| match (
            s.opt_u64(cr::segment::FIRST_TIMESTAMP_NS).ok().flatten(),
            s.opt_u64(cr::segment::LAST_TIMESTAMP_NS).ok().flatten(),
        ) {
            (Some(a), Some(b)) if b > a => Some((a, b)),
            _ => None,
        };
        let journal: u64 = segments
            .iter()
            .filter_map(|s| s.u64(cr::segment::BYTES).ok())
            .sum();
        let records: u64 = segments
            .iter()
            .filter_map(|s| s.u64(cr::segment::RECORDS).ok())
            .sum();
        let first = segments.iter().filter_map(span).map(|(a, _)| a).min();
        let last = segments.iter().filter_map(span).map(|(_, b)| b).max();
        let (Some(first), Some(last)) = (first, last) else {
            continue;
        };
        if journal == 0 || records == 0 {
            continue;
        }
        let average = rate_of(journal, last - first);
        let (bytes_per_second, records_per_second) =
            per_second(&journal_dir.join(id.to_string()), max_payload)?;
        let peak = bytes_per_second.iter().copied().max().unwrap_or(average);
        // The longest run of seconds above the average rate.
        let (mut burst, mut run) = (1u64, 0u64);
        for b in &bytes_per_second {
            run = if *b > average { run + 1 } else { 0 };
            burst = burst.max(run);
        }
        return Ok(Some(Rates {
            run_id: id.to_string(),
            average_bytes_per_s: average,
            peak_bytes_per_s: peak.max(average),
            burst_s: burst,
            overhead_ppm: u64::try_from(
                (u128::from(bytes.len() as u64) * 1_000_000).div_ceil(u128::from(journal)),
            )
            .unwrap_or(u64::MAX),
            average_record_bytes: journal.div_ceil(records),
            bytes_per_second,
            records_per_second,
        }));
    }
    Ok(None)
}

/// Bytes and records per second the production segment writer sustains on
/// the volume of `volume_dir`: `probe_bytes` of records of `record_bytes`
/// each appended, synced by `sync` like the capture's worker, and sealed
/// (fsync, read-back hash, rename, directory sync), timed; the probe's
/// directory is removed afterwards.
pub fn writer_throughput(
    volume_dir: &Path,
    probe_bytes: u64,
    record_bytes: usize,
    sync: SyncPolicy,
) -> Result<(u64, u64), String> {
    let header = darkbot_bsc_journal::HEADER_LEN;
    let payload = record_bytes.saturating_sub(header).max(1);
    let dir = volume_dir.join(format!(
        "{PROBE_PREFIX}{}-{}",
        std::process::id(),
        darkbot_bsc_record::observed_now()?
    ));
    let run = || -> Result<(u64, u64), String> {
        let mut writer = SegmentWriter::create(
            &dir,
            SegmentPolicy {
                max_segment_bytes: probe_bytes.max(1),
                max_segment_ns: u64::MAX,
            },
        )
        .map_err(|e| format!("{}: {e}", dir.display()))?;
        let body = vec![0xA5u8; payload].into_boxed_slice();
        let started = Instant::now();
        let (mut written, mut sequence) = (0u64, 0u64);
        let (mut since_sync, mut synced_at) = (0u64, Instant::now());
        while written < probe_bytes {
            writer
                .append(&CaptureRecord {
                    sequence,
                    kind: RecordKind::Runtime,
                    timestamp_ns: sequence,
                    payload: body.clone(),
                })
                .map_err(|e| format!("preflight probe: {e}"))?;
            sequence += 1;
            written += (header + payload) as u64;
            since_sync += 1;
            if since_sync >= sync.every_records || synced_at.elapsed() >= sync.every {
                writer.sync().map_err(|e| format!("preflight probe: {e}"))?;
                since_sync = 0;
                synced_at = Instant::now();
            }
        }
        writer.sync().map_err(|e| format!("preflight probe: {e}"))?;
        writer
            .finish()
            .map_err(|e| format!("preflight probe: {e}"))?;
        let ns = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        Ok((rate_of(written, ns), rate_of(sequence, ns)))
    };
    let result = run();
    let _ = fs::remove_dir_all(&dir);
    result
}

/// The prefix of a writer probe's directory.
const PROBE_PREFIX: &str = ".capture-preflight-";

/// Removes probe directories a killed preflight left in `volume_dir`.
fn remove_stale_probes(volume_dir: &Path) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(volume_dir) else {
        return Ok(());
    };
    for e in entries.flatten() {
        if e.file_name().to_string_lossy().starts_with(PROBE_PREFIX) {
            fs::remove_dir_all(e.path()).map_err(|err| format!("{}: {err}", e.path().display()))?;
        }
    }
    Ok(())
}

/// Free bytes of the volume holding `path` (POSIX `df -Pk`).
pub fn free_disk(path: &Path) -> Result<u64, String> {
    let out = Command::new("df")
        .arg("-Pk")
        .arg(path)
        .output()
        .map_err(|e| format!("df: {e}"))?;
    if !out.status.success() {
        return Err(format!("df {}: {}", path.display(), out.status));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .nth(1)
        .ok_or_else(|| format!("df {}: no filesystem line", path.display()))?;
    let available: u64 = line
        .split_whitespace()
        .nth(3)
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("df {}: no available blocks in {line:?}", path.display()))?;
    available
        .checked_mul(1024)
        .ok_or_else(|| "df: available bytes overflow".to_string())
}

/// The bounds of one preflight (`BSC_RUNTIME_CAPTURE_DURATION_S`,
/// `BSC_RUNTIME_PREFLIGHT_PROBE_BYTES`, `BSC_RUNTIME_DISK_RESERVE_BYTES`,
/// `BSC_RUNTIME_JOURNAL_BUDGET_BYTES`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreflightSetup {
    pub duration_s: u64,
    pub probe_bytes: u64,
    pub disk_reserve_bytes: u64,
    pub memory_budget_bytes: u64,
    /// The recorder's queue in records, the journal's payload bound and its
    /// sync policy (the probe syncs like the worker).
    pub queue_records: u64,
    pub max_payload: usize,
    pub sync: SyncPolicy,
}

/// One preflight: its measured inputs and the verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub rates: Option<Rates>,
    pub probe_bytes: u64,
    pub writer_bytes_per_s: u64,
    pub writer_records_per_s: u64,
    pub inputs: CapacityInputs,
    pub verdict: Preflight,
}

/// Measures and checks the planned capture into `journal_dir`.
pub fn check(journal_dir: &Path, setup: PreflightSetup) -> Result<Report, String> {
    fs::create_dir_all(journal_dir).map_err(|e| format!("{}: {e}", journal_dir.display()))?;
    let rates = rates(journal_dir, setup.max_payload)?;
    // Without a sealed run nothing is measured; the probe would have no
    // record size either.
    // The probe writes next to the journal directory, never inside it (the
    // census reads every run there); a probe a killed preflight left
    // behind is removed first (Phase F gate M2).
    let volume = journal_dir.parent().unwrap_or(journal_dir);
    remove_stale_probes(volume)?;
    let ((writer, writer_records), probe) = match &rates {
        Some(r) => (
            writer_throughput(
                volume,
                setup.probe_bytes,
                usize::try_from(r.average_record_bytes).unwrap_or(usize::MAX),
                setup.sync,
            )?,
            setup.probe_bytes,
        ),
        None => ((0, 0), 0),
    };
    let inputs = CapacityInputs {
        duration_s: setup.duration_s,
        average_bytes_per_s: rates.as_ref().map_or(0, |r| r.average_bytes_per_s),
        peak_bytes_per_s: rates.as_ref().map_or(0, |r| r.peak_bytes_per_s),
        burst_s: rates.as_ref().map_or(0, |r| r.burst_s),
        writer_bytes_per_s: writer,
        overhead_ppm: rates.as_ref().map_or(0, |r| r.overhead_ppm),
        free_disk_bytes: free_disk(journal_dir)?,
        disk_reserve_bytes: setup.disk_reserve_bytes,
        memory_budget_bytes: setup.memory_budget_bytes,
        // The recorded trace drained at the measured writer rate.
        measured_backlog_bytes: rates
            .as_ref()
            .map(|r| measured_backlog(&r.bytes_per_second, writer)),
        measured_backlog_records: rates
            .as_ref()
            .map(|r| measured_backlog(&r.records_per_second, writer_records)),
        queue_records: setup.queue_records,
    };
    Ok(Report {
        rates,
        probe_bytes: probe,
        writer_bytes_per_s: writer,
        writer_records_per_s: writer_records,
        verdict: preflight(&inputs),
        inputs,
    })
}

impl Report {
    pub fn qualified(&self) -> bool {
        matches!(self.verdict, Preflight::Qualified { .. })
    }

    pub fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let i = &self.inputs;
        let mut d = Draft::new(&cp::V2);
        d.set(cp::DURATION_S, i.duration_s)
            .set(cp::CHAIN_ID, 56u64)
            .set(cp::PROVENANCE_TOOL, "bsc_runtime capture-preflight")
            .set(cp::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
            .set(cp::AVERAGE_BYTES_PER_S, i.average_bytes_per_s)
            .set(cp::PEAK_BYTES_PER_S, i.peak_bytes_per_s)
            .set(cp::BURST_S, i.burst_s)
            .set(cp::OVERHEAD_PPM, i.overhead_ppm)
            .set(cp::PROBE_BYTES, self.probe_bytes)
            .set(cp::WRITER_BYTES_PER_S, self.writer_bytes_per_s)
            .set(cp::FREE_DISK_BYTES, i.free_disk_bytes)
            .set(cp::DISK_RESERVE_BYTES, i.disk_reserve_bytes)
            .set(cp::MEMORY_BUDGET_BYTES, i.memory_budget_bytes)
            .set(cp::QUEUE_RECORDS, i.queue_records)
            .set(cp::WRITER_RECORDS_PER_S, self.writer_records_per_s)
            .push(
                cp::NON_CLAIMS,
                "LOCAL_MEASUREMENT: this host's capture volume and its last sealed run; the source rate of a native geth capture may differ",
            );
        if let Some(r) = &self.rates {
            d.set(cp::RATES_RUN_ID, r.run_id.as_str());
        }
        if let Some(b) = i.measured_backlog_bytes {
            d.set(cp::BACKLOG_BYTES, b);
        }
        if let Some(r) = i.measured_backlog_records {
            d.set(cp::BACKLOG_RECORDS, r);
        }
        match self.verdict {
            Preflight::Qualified {
                expected_bytes,
                burst_backlog_bytes,
            } => {
                d.set(cp::VERDICT, "QUALIFIED")
                    .set(cp::EXPECTED_BYTES, expected_bytes)
                    .set(cp::BURST_BACKLOG_BYTES, burst_backlog_bytes);
            }
            Preflight::Blocked {
                resource,
                achievable_s,
            } => {
                d.set(cp::VERDICT, "BLOCKED")
                    .set(cp::BLOCKING_RESOURCE, resource_name(resource))
                    .set(cp::ACHIEVABLE_S, achievable_s);
            }
        }
        d.encode().map_err(|e| format!("capture preflight: {e}"))
    }

    pub fn summary(&self) -> String {
        let i = &self.inputs;
        let verdict = match self.verdict {
            Preflight::Qualified {
                expected_bytes,
                burst_backlog_bytes,
            } => format!(
                "QUALIFIED: {expected_bytes} journal bytes expected, burst backlog {burst_backlog_bytes} bytes"
            ),
            Preflight::Blocked {
                resource,
                achievable_s,
            } => format!(
                "BLOCKED by {}: {achievable_s} s would qualify",
                resource_name(resource)
            ),
        };
        format!(
            "capture preflight for {} s: {verdict}\nrates of run {}: average {} B/s, peak {} B/s for {} s, overhead {} ppm; writer {} B/s ({} probe bytes); free disk {} B (reserve {} B); memory budget {} B",
            i.duration_s,
            self.rates
                .as_ref()
                .map_or("none (unmeasured)", |r| r.run_id.as_str()),
            i.average_bytes_per_s,
            i.peak_bytes_per_s,
            i.burst_s,
            i.overhead_ppm,
            self.writer_bytes_per_s,
            self.probe_bytes,
            i.free_disk_bytes,
            i.disk_reserve_bytes,
            i.memory_budget_bytes
        )
    }
}

fn resource_name(r: Resource) -> &'static str {
    match r {
        Resource::Unmeasured => "UNMEASURED",
        Resource::WriterThroughput => "WRITER_THROUGHPUT",
        Resource::Memory => "MEMORY",
        Resource::Disk => "DISK",
        Resource::Queue => "QUEUE",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_journal::{EXIT_PAYLOAD, JournalSetup, RuntimeJournal};

    fn setup(duration_s: u64, reserve: u64) -> PreflightSetup {
        PreflightSetup {
            duration_s,
            probe_bytes: 1 << 20,
            disk_reserve_bytes: reserve,
            memory_budget_bytes: 64 << 20,
            queue_records: 256,
            max_payload: 4096,
            sync: SyncPolicy {
                every_records: 64,
                every: std::time::Duration::from_millis(1_000),
            },
        }
    }

    /// A sealed run of 1 s record time over two segments.
    fn write(dir: &Path, started: u64) {
        let s = JournalSetup {
            segment: SegmentPolicy {
                max_segment_bytes: 4_000,
                max_segment_ns: u64::MAX,
            },
            ..JournalSetup::whole(256, 4096)
        };
        let mut j = RuntimeJournal::open(dir, started, 1, s).unwrap();
        for i in 0..=100u64 {
            assert!(j.record(RecordKind::Runtime, i * 10_000_000, vec![7u8; 64]));
        }
        assert!(j.record(RecordKind::Runtime, 1_000_000_000, EXIT_PAYLOAD.to_vec()));
        j.finish().unwrap();
    }

    #[test]
    fn rates_come_from_the_newest_sealed_run_and_qualify_a_fitting_run() {
        let base = std::env::temp_dir().join(format!("bsc-preflight-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let dir = base.join("journal");
        // A probe a killed preflight left behind goes first.
        fs::create_dir_all(base.join(".capture-preflight-1-1")).unwrap();
        // Nothing sealed yet: unmeasured, blocked.
        let r = check(&dir, setup(1_800, 0)).unwrap();
        assert_eq!(
            r.verdict,
            Preflight::Blocked {
                resource: Resource::Unmeasured,
                achievable_s: 0
            }
        );
        write(&dir, 5);
        // A newer run without a manifest is not a measurement.
        fs::create_dir_all(dir.join("9")).unwrap();
        let rates = rates(&dir, 4096).unwrap().unwrap();
        assert_eq!(rates.run_id, "5");
        // 101 frames of 64 payload bytes and the 1-byte end record, each
        // with its frame header, in 1 s.
        let h = darkbot_bsc_journal::HEADER_LEN as u64;
        assert_eq!(
            rates.average_record_bytes,
            (101 * (h + 64) + h + 1).div_ceil(102)
        );
        assert!(
            rates.average_bytes_per_s >= 10_000
                && rates.peak_bytes_per_s >= rates.average_bytes_per_s
        );
        assert!(rates.overhead_ppm > 0 && rates.burst_s >= 1);
        let r = check(&dir, setup(1_800, 0)).unwrap();
        assert!(r.qualified(), "{}", r.summary());
        assert!(r.writer_bytes_per_s > 0 && r.inputs.free_disk_bytes > 0);
        let record = cp::decode_preflight(&r.encode(1).unwrap()).unwrap();
        assert_eq!(record.text(cp::VERDICT).unwrap(), "QUALIFIED");
        assert_eq!(record.u64(cp::QUEUE_RECORDS).unwrap(), 256);
        assert!(record.u64(cp::WRITER_RECORDS_PER_S).unwrap() > 0);
        assert_eq!(
            record.u64(cp::BACKLOG_BYTES).ok(),
            r.inputs.measured_backlog_bytes
        );
        assert_eq!(record.text(cp::RATES_RUN_ID).unwrap(), "5");
        // The probe leaves nothing behind, in the journal directory or next
        // to it, and the stale one is gone.
        for d in [&dir, &base] {
            assert!(fs::read_dir(d).unwrap().all(|e| {
                !e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".capture")
            }));
        }
        // A reserve above the free disk blocks on DISK with no duration.
        let r = check(&dir, setup(1_800, u64::MAX)).unwrap();
        assert_eq!(
            r.verdict,
            Preflight::Blocked {
                resource: Resource::Disk,
                achievable_s: 0
            }
        );
        fs::remove_dir_all(&base).unwrap();
    }
}

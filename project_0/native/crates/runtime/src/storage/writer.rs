//! SK-04: the market journal's one ordered writer, off the capture thread.
//!
//! The capture thread reserves the exact bytes of a typed row (payload and its frame copy in the
//! batch buffer) in the `MemoryBudget` before encoding it (bincode, no JSON) and hands the frame
//! to a dedicated writer over the existing bounded channel; it never writes, flushes or syncs.
//! Queue slots and the batch vector are reserved once at start, so every capture-owned byte of
//! this lane is inside the budget and `max` bounds it. The writer appends admitted frames in order in bounded batches (one
//! sequential write per batch, `Writer::append_batch`) and syncs at the configured interval and at
//! seal. OS-buffered frames are not crash-safe before a sync; an unsealed tail is INCOMPLETE.
//! A writer failure is sticky: every later submit and the seal fail, the capture stops
//! INCOMPLETE. Financial FE-04/P8 durability is a separate writer lane and is not relaxed here.
//!
//! SK-05: under backlog the writer adapts only within its reserved capacity (batches up to
//! `ADAPTIVE_BATCH`× the configured size, the periodic sync deferred at most to twice its
//! interval); it never grows beyond `max` and never drops. Before every batch the free disk space
//! is checked against the configured reserve: falling below it fails the lane (INCOMPLETE)
//! before the filesystem is full. `status()` reports queue bytes, rates, lag and exhaustion
//! forecasts for the terminal.
use super::budget::{free_disk_bytes, BudgetSnapshot, BytePermit, MemoryBudget, MemorySample, MIB};
use crate::analytics::journal::{self, Provenance};
use crate::analytics::payload::{self, Payload};
use darkbot_common::channel::{bounded, ChannelError, Consumer, Producer};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Writer configuration from the typed operational config (`runtime_config` limits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriterConfig {
    pub initial_bytes: u64,
    pub max_bytes: u64,
    pub system_reserve_bytes: u64,
    pub batch_frames: usize,
    pub flush_interval: Duration,
    pub sync_interval: Duration,
    /// Longest wait of the capture thread for budget or queue space before INCOMPLETE.
    pub stall_budget: Duration,
    /// Free disk space that must remain on the journal's filesystem.
    pub disk_reserve_bytes: u64,
    /// SK-07: segment size; the journal rotates to a linked successor at this size (0 = never).
    pub segment_bytes: u64,
    /// Test-only I/O fault injection: the writer fails at this batch.
    #[cfg(test)]
    pub fail_at_batch: Option<u64>,
}

impl WriterConfig {
    pub fn from_active() -> Self {
        let l = &darkbot_solana::runtime_config::active().limits;
        Self {
            initial_bytes: l.market_capture_initial_mib * MIB,
            max_bytes: l.market_capture_max_mib * MIB,
            system_reserve_bytes: l.system_reserve_mib * MIB,
            batch_frames: l.market_writer_batch_frames as usize,
            flush_interval: Duration::from_millis(l.market_writer_flush_ms),
            sync_interval: Duration::from_millis(l.market_writer_sync_ms),
            stall_budget: Duration::from_millis(l.market_writer_stall_ms),
            disk_reserve_bytes: l.market_disk_reserve_mib * MIB,
            segment_bytes: l.market_segment_mib * MIB,
            #[cfg(test)]
            fail_at_batch: None,
        }
    }
}

/// Largest adaptive batch, in multiples of the configured batch size (reserved at start).
const ADAPTIVE_BATCH: usize = 4;

/// SK-05 control decision: under backlog (queue depth of at least two batches) write larger
/// batches, up to `ADAPTIVE_BATCH`× the configured size, and defer the periodic sync while the
/// backlog lasts, but never beyond twice its interval (a bounded unsynced tail).
fn adapt(
    queue_depth: u64,
    batch_frames: usize,
    since_sync: Duration,
    sync_interval: Duration,
) -> (usize, bool) {
    let backlog = queue_depth >= 2 * batch_frames as u64;
    let limit = if backlog {
        (batch_frames * ADAPTIVE_BATCH).min(batch_frames + queue_depth as usize)
    } else {
        batch_frames
    };
    let sync_due = since_sync >= sync_interval && (!backlog || since_sync >= 2 * sync_interval);
    (limit, sync_due)
}

/// Counters shared by the capture thread (admission) and the writer thread (writes).
#[derive(Default)]
struct LaneCounters {
    admitted_bytes: AtomicU64,
    admitted_frames: AtomicU64,
    written_bytes: AtomicU64,
    written_frames: AtomicU64,
}

/// Live state of the writer lane (SK-05), sampled by the capture thread for the terminal.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct StorageStatus {
    pub queue_bytes: u64,
    pub queue_peak_bytes: u64,
    pub admission_limit_bytes: u64,
    pub max_bytes: u64,
    pub free_budget_bytes: u64,
    pub ingress_bytes_per_s: f64,
    pub write_bytes_per_s: f64,
    /// Admitted but not yet written frames.
    pub lag_frames: u64,
    /// Seconds until the byte budget is exhausted at the current rates (`None`: not draining
    /// slower than arriving).
    pub budget_exhaustion_s: Option<f64>,
    pub disk_free_bytes: Option<u64>,
    pub disk_reserve_bytes: u64,
    /// Seconds until the disk reserve is reached at the current write rate.
    pub disk_exhaustion_s: Option<f64>,
}

impl StorageStatus {
    pub fn terminal(&self) -> String {
        let mib = |b: u64| b as f64 / MIB as f64;
        let eta = |s: Option<f64>| s.map_or("none".into(), |s| format!("{s:.0}s"));
        format!(
            "[MARKET/STORAGE] queue={:.1}/{:.1}MiB (max {:.1}) peak={:.1}MiB ingress={:.1}KiB/s write={:.1}KiB/s lag={} budget_exhaustion={} disk_free={} disk_exhaustion={}",
            mib(self.queue_bytes),
            mib(self.admission_limit_bytes),
            mib(self.max_bytes),
            mib(self.queue_peak_bytes),
            self.ingress_bytes_per_s / 1024.0,
            self.write_bytes_per_s / 1024.0,
            self.lag_frames,
            eta(self.budget_exhaustion_s),
            self.disk_free_bytes
                .map_or("unknown".into(), |b| format!("{:.1}GiB", b as f64 / (1024.0 * MIB as f64))),
            eta(self.disk_exhaustion_s),
        )
    }
}

/// Measured writer and budget behaviour of one journal (reported with the seal).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct StorageMetrics {
    pub frames: u64,
    pub batches: u64,
    pub bytes_written: u64,
    pub max_batch_frames: u64,
    pub max_write_ns: u64,
    pub syncs: u64,
    pub max_sync_ns: u64,
    pub queue_full_waits: u64,
    /// Bytes reserved once for queue slots and the batch vector.
    pub fixed_reserved_bytes: u64,
    /// Worst process/OS memory state sampled at each batch and at seal.
    pub memory: MemorySample,
    pub budget: BudgetSnapshot,
    /// SK-05: batches written larger than configured (backlog) and syncs deferred by backlog.
    pub backlog_batches: u64,
    pub deferred_syncs: u64,
    pub min_disk_free_bytes: Option<u64>,
    /// Highest rates seen by `status()` (bytes/s, rounded).
    pub max_ingress_bytes_per_s: u64,
    pub max_write_bytes_per_s: u64,
    /// SK-07: segments of the journal (1 = never rotated).
    pub segments: u32,
}

struct Pending {
    kind: u16,
    schema: u16,
    provenance: Provenance,
    payload: Vec<u8>,
    _permit: BytePermit,
}

enum Msg {
    Frame(Pending),
    Seal(bool),
}

/// Reserved bytes of one frame: the encoded payload plus its copy in the batch buffer (payload
/// and frame header/CRC/SHA). The queue slot itself is part of the fixed reservation.
fn frame_bytes(payload_len: u64) -> u64 {
    2 * payload_len + journal::FRAME_WIRE as u64
}

pub struct Sealed {
    pub durable: journal::Durable,
    pub frames: u64,
    pub metrics: StorageMetrics,
}

pub struct MarketJournalWorker {
    tx: Option<Producer<Msg>>,
    handle: Option<JoinHandle<io::Result<Sealed>>>,
    budget: MemoryBudget,
    failure: Arc<Mutex<Option<String>>>,
    stall_budget: Duration,
    queue_full_waits: u64,
    fixed: Option<BytePermit>,
    fixed_bytes: u64,
    counters: Arc<LaneCounters>,
    last_sample: Option<(Instant, u64, u64)>,
    max_rates: (f64, f64),
    disk_reserve_bytes: u64,
    pub path: PathBuf,
}

impl MarketJournalWorker {
    /// Creates the journal (never overwrites) and starts its writer thread.
    pub fn start(path: &Path, cfg: WriterConfig) -> Result<Self, String> {
        let budget = MemoryBudget::new(cfg.initial_bytes, cfg.max_bytes, cfg.system_reserve_bytes)
            .map_err(str::to_string)?;
        if cfg.batch_frames == 0 {
            return Err("MARKET_WRITER_BATCH_INVALID".into());
        }
        // Queue slots (count-bounded channel) and the writer's batch vector are capture-owned
        // memory: reserved once, inside the initial admission limit.
        let slots = cfg.batch_frames * 4;
        let batch_capacity = cfg.batch_frames * ADAPTIVE_BATCH;
        let fixed_bytes = ((slots + batch_capacity) * std::mem::size_of::<Msg>()) as u64;
        if fixed_bytes >= cfg.initial_bytes {
            return Err("MARKET_WRITER_SLOTS_EXCEED_INITIAL_BUDGET".into());
        }
        let fixed = budget
            .acquire(fixed_bytes, Duration::ZERO)
            .map_err(str::to_string)?;
        let writer = super::segments::SegmentedWriter::create(path, cfg.segment_bytes)
            .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        let (tx, rx) = bounded::<Msg>("market-journal-writer", slots);
        let failure = Arc::new(Mutex::new(None));
        let counters = Arc::new(LaneCounters::default());
        let (thread_failure, thread_budget) = (failure.clone(), budget.clone());
        let thread_counters = counters.clone();
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let handle = std::thread::Builder::new()
            .name("market-journal-writer".into())
            .spawn(move || {
                // The receiver outlives the failure record: a submit never sees a closed lane
                // without its reason.
                let result = write_loop(writer, &rx, cfg, &thread_budget, &thread_counters, &dir);
                if let Err(e) = &result {
                    *thread_failure.lock().unwrap_or_else(|p| p.into_inner()) =
                        Some(format!("MARKET_WRITER_FAILED: {e}"));
                }
                drop(rx);
                result
            })
            .map_err(|e| format!("spawn market journal writer: {e}"))?;
        Ok(Self {
            tx: Some(tx),
            handle: Some(handle),
            budget,
            failure,
            stall_budget: cfg.stall_budget,
            queue_full_waits: 0,
            fixed: Some(fixed),
            fixed_bytes,
            counters,
            last_sample: None,
            max_rates: (0.0, 0.0),
            disk_reserve_bytes: cfg.disk_reserve_bytes,
            path: path.into(),
        })
    }

    /// Live lane state; rates are measured since the previous call (the capture loop calls it
    /// at its display cadence).
    pub fn status(&mut self) -> StorageStatus {
        let b = self.budget.snapshot();
        let now = Instant::now();
        let admitted = self.counters.admitted_bytes.load(Ordering::Relaxed);
        let written = self.counters.written_bytes.load(Ordering::Relaxed);
        let (ingress, write) = match self.last_sample {
            Some((t, a, w)) if now > t => {
                let dt = (now - t).as_secs_f64();
                (
                    admitted.saturating_sub(a) as f64 / dt,
                    written.saturating_sub(w) as f64 / dt,
                )
            }
            _ => (0.0, 0.0),
        };
        self.last_sample = Some((now, admitted, written));
        self.max_rates = (self.max_rates.0.max(ingress), self.max_rates.1.max(write));
        let queue = b.used_bytes.saturating_sub(self.fixed_bytes);
        let free_budget = b.max_bytes.saturating_sub(b.used_bytes);
        let disk = self.path.parent().and_then(free_disk_bytes);
        StorageStatus {
            queue_bytes: queue,
            queue_peak_bytes: b.peak_bytes.saturating_sub(self.fixed_bytes),
            admission_limit_bytes: b.limit_bytes,
            max_bytes: b.max_bytes,
            free_budget_bytes: free_budget,
            ingress_bytes_per_s: ingress,
            write_bytes_per_s: write,
            lag_frames: self
                .counters
                .admitted_frames
                .load(Ordering::Relaxed)
                .saturating_sub(self.counters.written_frames.load(Ordering::Relaxed)),
            budget_exhaustion_s: (ingress > write).then(|| free_budget as f64 / (ingress - write)),
            disk_free_bytes: disk,
            disk_reserve_bytes: self.disk_reserve_bytes,
            disk_exhaustion_s: disk
                .filter(|_| write > 0.0)
                .map(|free| free.saturating_sub(self.disk_reserve_bytes) as f64 / write),
        }
    }

    fn failed(&self) -> Option<io::Error> {
        self.failure
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(|e| io::Error::other(e.clone()))
    }

    /// Encodes `row` on the calling thread and admits it to the writer, waiting at most the
    /// stall budget for bytes and queue space. Never drops: a refusal is an error.
    pub fn submit<P: Payload>(&mut self, provenance: Provenance, row: &P) -> io::Result<()> {
        if let Some(e) = self.failed() {
            return Err(e);
        }
        // Reserve first: the encoded payload is allocated only after admission.
        let len = payload::encoded_len(row)?;
        let deadline = Instant::now() + self.stall_budget;
        let permit = self
            .budget
            .acquire(frame_bytes(len), self.stall_budget)
            .map_err(io::Error::other)?;
        let bytes = payload::encode(row)?;
        let wire = bytes.len() as u64 + journal::FRAME_WIRE as u64;
        let mut msg = Msg::Frame(Pending {
            kind: P::KIND,
            schema: P::SCHEMA,
            provenance,
            payload: bytes,
            _permit: permit,
        });
        let tx = self
            .tx
            .as_ref()
            .ok_or_else(|| io::Error::other("MARKET_JOURNAL_ALREADY_FINISHED"))?;
        loop {
            match tx.try_push(msg) {
                Ok(()) => {
                    self.counters
                        .admitted_bytes
                        .fetch_add(wire, Ordering::Relaxed);
                    self.counters
                        .admitted_frames
                        .fetch_add(1, Ordering::Relaxed);
                    return Ok(());
                }
                Err(ChannelError::Disconnected(_)) => {
                    return Err(self
                        .failed()
                        .unwrap_or_else(|| io::Error::other("MARKET_WRITER_GONE")))
                }
                Err(ChannelError::Full(back)) => {
                    msg = back;
                    self.queue_full_waits += 1;
                    if Instant::now() >= deadline {
                        return Err(io::Error::other("MARKET_WRITER_QUEUE_STALLED"));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }

    /// Writes every admitted frame, seals (manifest + sync) and stops the writer.
    pub fn seal(mut self, complete: bool) -> io::Result<Sealed> {
        self.finish(complete)
    }

    fn finish(&mut self, complete: bool) -> io::Result<Sealed> {
        let tx = self
            .tx
            .take()
            .ok_or_else(|| io::Error::other("MARKET_JOURNAL_ALREADY_FINISHED"))?;
        let mut msg = Msg::Seal(complete);
        // The seal waits for queue space without a stall limit: frames already admitted
        // must reach the journal before its manifest.
        loop {
            match tx.try_push(msg) {
                Ok(()) => break,
                Err(ChannelError::Full(back)) => {
                    msg = back;
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(ChannelError::Disconnected(_)) => break,
            }
        }
        drop(tx);
        let handle = self
            .handle
            .take()
            .ok_or_else(|| io::Error::other("MARKET_JOURNAL_ALREADY_FINISHED"))?;
        let mut sealed = handle
            .join()
            .map_err(|_| io::Error::other("MARKET_WRITER_PANICKED"))??;
        sealed.metrics.queue_full_waits = self.queue_full_waits;
        sealed.metrics.fixed_reserved_bytes = self.fixed_bytes;
        sealed.metrics.max_ingress_bytes_per_s = self.max_rates.0.round() as u64;
        sealed.metrics.max_write_bytes_per_s = self.max_rates.1.round() as u64;
        // The writer has stopped: its slots are released before the final snapshot.
        self.fixed = None;
        sealed.metrics.budget = self.budget.snapshot();
        Ok(sealed)
    }
}

impl Drop for MarketJournalWorker {
    /// An unfinished worker (error path) still seals INCOMPLETE.
    fn drop(&mut self) {
        if self.tx.is_some() {
            if let Err(e) = self.finish(false) {
                eprintln!("MARKET_JOURNAL_INCOMPLETE {e}");
            }
        }
    }
}

fn write_loop(
    mut writer: super::segments::SegmentedWriter,
    rx: &Consumer<Msg>,
    cfg: WriterConfig,
    budget: &MemoryBudget,
    counters: &LaneCounters,
    dir: &Path,
) -> io::Result<Sealed> {
    let mut m = StorageMetrics::default();
    let mut batch: Vec<Pending> = Vec::with_capacity(cfg.batch_frames * ADAPTIVE_BATCH);
    let mut first_pending: Option<Instant> = None;
    let mut last_sync = Instant::now();
    let mut seal = None;
    loop {
        let wait = first_pending
            .map(|t| cfg.flush_interval.saturating_sub(t.elapsed()))
            .unwrap_or(cfg.flush_interval);
        match rx.pop_timeout_or_closed(wait) {
            Ok(Some(Msg::Frame(f))) => {
                first_pending.get_or_insert_with(Instant::now);
                batch.push(f);
            }
            Ok(Some(Msg::Seal(complete))) => seal = Some(complete),
            Ok(None) => {}
            // Closed without a seal request: the owner vanished; seal INCOMPLETE.
            Err(()) => seal = Some(false),
        }
        let depth = {
            let s = rx.stats();
            s.sent.saturating_sub(s.received)
        };
        let (batch_limit, sync_due) = adapt(
            depth,
            cfg.batch_frames,
            last_sync.elapsed(),
            cfg.sync_interval,
        );
        let due = first_pending.is_some_and(|t| t.elapsed() >= cfg.flush_interval);
        // Under backlog keep collecting from the queue up to the adaptive batch size.
        if depth > 0 && batch.len() < batch_limit && seal.is_none() {
            continue;
        }
        if !batch.is_empty() && (batch.len() >= cfg.batch_frames || due || seal.is_some()) {
            #[cfg(test)]
            if cfg.fail_at_batch == Some(m.batches) {
                return Err(io::Error::other("injected write failure"));
            }
            // Fail before the filesystem fills: the reserve is never written into.
            let free = free_disk_bytes(dir);
            m.min_disk_free_bytes = match (m.min_disk_free_bytes, free) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            if free.is_some_and(|f| f < cfg.disk_reserve_bytes) {
                return Err(io::Error::other("DISK_RESERVE_EXHAUSTED"));
            }
            if batch.len() > cfg.batch_frames {
                m.backlog_batches += 1;
            }
            let before = writer.bytes();
            let started = Instant::now();
            let n = writer.append_batch(
                batch
                    .iter()
                    .map(|f| (f.kind, f.schema, &f.provenance, f.payload.as_slice())),
            )?;
            m.max_write_ns = m.max_write_ns.max(started.elapsed().as_nanos() as u64);
            m.frames += n;
            m.batches += 1;
            m.max_batch_frames = m.max_batch_frames.max(n);
            m.bytes_written = writer.bytes();
            counters
                .written_bytes
                .fetch_add(writer.bytes() - before, Ordering::Relaxed);
            counters.written_frames.fetch_add(n, Ordering::Relaxed);
            m.memory = m.memory.worst(MemorySample::now());
            // Written: release the reserved bytes of the whole batch.
            batch.clear();
            first_pending = None;
            // SK-07: rotate between batches only (the successor is linked and synced).
            if writer.rotate_if_due()? {
                m.syncs += 2;
                last_sync = Instant::now();
            }
            m.segments = writer.segments();
        }
        if seal.is_none() && last_sync.elapsed() >= cfg.sync_interval && !sync_due && m.frames > 0 {
            m.deferred_syncs += 1;
        }
        if seal.is_none() && sync_due && m.frames > 0 {
            let started = Instant::now();
            writer.sync()?;
            m.syncs += 1;
            m.max_sync_ns = m.max_sync_ns.max(started.elapsed().as_nanos() as u64);
            last_sync = Instant::now();
        }
        if let Some(complete) = seal {
            m.segments = writer.segments();
            let set = writer.seal(complete)?;
            let durable = set.durable;
            m.syncs += 1;
            m.bytes_written = set.bytes;
            m.memory = m.memory.worst(MemorySample::now());
            m.budget = budget.snapshot();
            return Ok(Sealed {
                durable,
                frames: m.frames,
                metrics: m,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::observer::rows::MarketCoverage;

    fn cfg(max: u64) -> WriterConfig {
        WriterConfig {
            initial_bytes: max,
            max_bytes: max,
            system_reserve_bytes: 0,
            batch_frames: 4,
            flush_interval: Duration::from_millis(5),
            sync_interval: Duration::from_millis(20),
            stall_budget: Duration::from_millis(50),
            disk_reserve_bytes: 0,
            segment_bytes: 0,
            fail_at_batch: None,
        }
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "p0_writer_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn row(i: u64) -> MarketCoverage {
        MarketCoverage::SlotFailed {
            slot: i,
            error: "X".into(),
        }
    }

    /// Frames are written in order and in batches by the writer thread; the sealed journal
    /// verifies; metrics and the budget are reported; nothing remains reserved.
    #[test]
    fn worker_writes_ordered_batches_and_seals() {
        let d = dir("ordered");
        let path = d.join("market.p0j");
        let mut w = MarketJournalWorker::start(&path, cfg(1 << 20)).unwrap();
        for i in 0..10 {
            w.submit(
                Provenance {
                    slot: Some(i),
                    ..Default::default()
                },
                &row(i),
            )
            .unwrap();
        }
        let sealed = w.seal(true).unwrap();
        assert_eq!(sealed.frames, 10);
        assert!(sealed.metrics.batches >= 3 && sealed.metrics.max_batch_frames <= 4);
        assert_eq!(sealed.metrics.budget.used_bytes, 0);
        let mut r = journal::Reader::open(&path).unwrap();
        let mut slots = Vec::new();
        while let Some(f) = r.next_frame().unwrap() {
            if f.kind == journal::kind::MARKET_COVERAGE {
                let MarketCoverage::SlotFailed { slot, .. } = f.typed::<MarketCoverage>().unwrap()
                else {
                    panic!()
                };
                slots.push(slot);
            }
        }
        assert_eq!(slots, (0..10).collect::<Vec<_>>());
        assert!(r.sealed && r.complete);
        assert_eq!(journal::hex(&r.root()), journal::hex(&sealed.durable.hash));
        assert!(MarketJournalWorker::start(&path, cfg(1 << 20)).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }

    fn fixed(c: &WriterConfig) -> u64 {
        ((c.batch_frames * (4 + ADAPTIVE_BATCH)) * std::mem::size_of::<Msg>()) as u64
    }

    fn frames_in(path: &Path) -> (u64, bool, bool) {
        let mut r = journal::Reader::open(path).unwrap();
        let mut n = 0;
        while let Ok(Some(f)) = r.next_frame() {
            n += u64::from(f.kind == journal::kind::MARKET_COVERAGE);
        }
        (n, r.sealed, r.complete)
    }

    /// The payload is reserved before it is encoded: a row whose frame (payload + its batch copy)
    /// exceeds `max` is refused without allocating; slots beyond `initial` refuse the start;
    /// a dropped worker seals INCOMPLETE with every admitted frame.
    #[test]
    fn reservation_precedes_encoding_and_slots_are_budgeted() {
        let d = dir("budget");
        let mut c = cfg(0);
        let f = fixed(&c);
        c.initial_bytes = f;
        c.max_bytes = f;
        assert_eq!(
            MarketJournalWorker::start(&d.join("x.p0j"), c)
                .err()
                .as_deref(),
            Some("MARKET_WRITER_SLOTS_EXCEED_INITIAL_BUDGET")
        );
        c.initial_bytes = f + 2_000;
        c.max_bytes = f + 2_000;
        let mut w = MarketJournalWorker::start(&d.join("a.p0j"), c).unwrap();
        let big = MarketCoverage::SlotFailed {
            slot: 1,
            error: "e".repeat(1_000),
        };
        let len = payload::encoded_len(&big).unwrap();
        assert_eq!(len, payload::encode(&big).unwrap().len() as u64);
        assert!(frame_bytes(len) > 2_000);
        let e = w.submit(Provenance::default(), &big).unwrap_err();
        assert_eq!(e.to_string(), "CAPTURE_MEMORY_BUDGET_EXHAUSTED");
        w.submit(Provenance::default(), &row(1)).unwrap();
        drop(w);
        assert_eq!(frames_in(&d.join("a.p0j")), (1, true, false));
        std::fs::remove_dir_all(d).unwrap();
    }

    /// Overload while the writer holds a batch: admission stops at `max` (refused after the
    /// stall budget, never dropped); every admitted frame reaches the journal at seal and the
    /// budget is fully released.
    #[test]
    fn overload_refuses_at_max_and_keeps_every_admitted_frame() {
        let d = dir("overload");
        let mut c = cfg(0);
        c.batch_frames = 64;
        c.flush_interval = Duration::from_secs(30);
        let one = frame_bytes(payload::encoded_len(&row(0)).unwrap());
        c.initial_bytes = fixed(&c) + 3 * one;
        c.max_bytes = c.initial_bytes;
        let mut w = MarketJournalWorker::start(&d.join("o.p0j"), c).unwrap();
        let mut admitted = 0;
        let refusal = loop {
            match w.submit(Provenance::default(), &row(admitted)) {
                Ok(()) => admitted += 1,
                Err(e) => break e.to_string(),
            }
            assert!(admitted <= 3, "admission beyond max");
        };
        assert_eq!(
            (admitted, refusal.as_str()),
            (3, "CAPTURE_MEMORY_BUDGET_EXHAUSTED")
        );
        let sealed = w.seal(false).unwrap();
        assert_eq!(sealed.frames, 3);
        let b = sealed.metrics.budget;
        assert!(b.peak_bytes <= b.max_bytes && b.refused == 1);
        assert_eq!(b.used_bytes, 0);
        assert!(sealed.metrics.memory.rss_bytes.is_some());
        assert_eq!(frames_in(&d.join("o.p0j")), (3, true, false));
        std::fs::remove_dir_all(d).unwrap();
    }

    /// A write failure is sticky: later submits and the seal report it, and the journal keeps
    /// the frames written before the failure, unsealed (INCOMPLETE for every reader).
    #[test]
    fn writer_failure_is_sticky_and_keeps_the_written_prefix() {
        let d = dir("failure");
        let mut c = cfg(1 << 20);
        c.batch_frames = 1;
        c.flush_interval = Duration::from_millis(1);
        c.fail_at_batch = Some(2);
        let path = d.join("f.p0j");
        let mut w = MarketJournalWorker::start(&path, c).unwrap();
        for i in 0..2 {
            w.submit(Provenance::default(), &row(i)).unwrap();
            // One batch per frame: wait until the writer drained it (not a fixed sleep).
            let deadline = Instant::now() + Duration::from_secs(5);
            while w.status().lag_frames > 0 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        // The third batch fails on the writer thread.
        let _ = w.submit(Provenance::default(), &row(2));
        let deadline = Instant::now() + Duration::from_secs(5);
        let sticky = loop {
            match w.submit(Provenance::default(), &row(9)) {
                Err(e) => break e.to_string(),
                Ok(()) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
                Ok(()) => panic!("failure not reported"),
            }
        };
        assert!(sticky.contains("injected write failure"), "{sticky}");
        assert!(w.seal(true).is_err());
        assert_eq!(frames_in(&path), (2, false, false));
        std::fs::remove_dir_all(d).unwrap();
    }

    /// SK-05: larger batches and a deferred sync only under backlog, bounded by the reserved
    /// batch capacity and twice the sync interval.
    #[test]
    fn adaptive_control_stays_within_reserved_bounds() {
        let ms = Duration::from_millis;
        assert_eq!(adapt(0, 8, ms(5), ms(10)), (8, false));
        assert_eq!(adapt(3, 8, ms(10), ms(10)), (8, true));
        assert_eq!(adapt(16, 8, ms(10), ms(10)), (24, false));
        assert_eq!(adapt(16, 8, ms(19), ms(10)).1, false);
        assert_eq!(adapt(16, 8, ms(20), ms(10)).1, true);
        assert_eq!(adapt(1_000, 8, ms(0), ms(10)).0, 8 * ADAPTIVE_BATCH);
        assert_eq!(adapt(17, 8, ms(0), ms(10)).0, 25);
    }

    /// The disk reserve is checked before every batch: below it the lane fails sticky
    /// (INCOMPLETE) without writing; the status reports queue, lag and disk state.
    #[test]
    fn disk_reserve_fails_closed_and_status_reports_the_lane() {
        let d = dir("disk");
        let mut c = cfg(1 << 20);
        c.disk_reserve_bytes = u64::MAX / 2;
        c.batch_frames = 1;
        c.flush_interval = Duration::from_millis(1);
        let path = d.join("d.p0j");
        let mut w = MarketJournalWorker::start(&path, c).unwrap();
        let first = w.status();
        assert_eq!((first.lag_frames, first.queue_bytes), (0, 0));
        assert!(first
            .disk_free_bytes
            .is_some_and(|f| f < first.disk_reserve_bytes));
        w.submit(Provenance::default(), &row(1)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let err = loop {
            match w.submit(Provenance::default(), &row(2)) {
                Err(e) => break e.to_string(),
                Ok(()) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
                Ok(()) => panic!("reserve not enforced"),
            }
        };
        assert!(err.contains("DISK_RESERVE_EXHAUSTED"), "{err}");
        let s = w.status();
        assert!(s.lag_frames >= 1, "{s:?}");
        assert!(s.terminal().starts_with("[MARKET/STORAGE] queue="));
        assert!(w.seal(true).is_err());
        assert_eq!(frames_in(&path), (0, false, false));
        std::fs::remove_dir_all(d).unwrap();
    }

    /// SK-07: the lane rotates into linked segments between batches; the chain replays every
    /// admitted frame once and in order, and the seal reports the segment count.
    #[test]
    fn lane_rotates_segments_and_chain_replays_every_frame() {
        let d = dir("segments");
        let mut c = cfg(64 * MIB);
        c.segment_bytes = 2048;
        // Every rotation seals and syncs a segment; not a stall test (parallel test load).
        c.stall_budget = Duration::from_secs(10);
        let path = d.join("market.p0j");
        let mut w = MarketJournalWorker::start(&path, c).unwrap();
        for i in 0..200u64 {
            let p = Provenance {
                source_seq: i,
                ..Default::default()
            };
            w.submit(p, &row(i)).unwrap();
        }
        let sealed = w.seal(true).unwrap();
        let chain = super::super::segments::verify_chain(&path).unwrap();
        assert!(chain.segments > 2, "{chain:?}");
        assert_eq!(
            (
                chain.events,
                chain.sealed,
                chain.complete,
                sealed.metrics.segments
            ),
            (200, true, true, chain.segments)
        );
        let mut r = super::super::segments::ChainReader::open(&path).unwrap();
        let mut seqs = Vec::new();
        while let Some(f) = r.next_frame().unwrap() {
            if f.kind != journal::MANIFEST {
                seqs.push(f.provenance.source_seq);
            }
        }
        assert_eq!(seqs, (0..200).collect::<Vec<_>>());
        std::fs::remove_dir_all(d).unwrap();
    }
}

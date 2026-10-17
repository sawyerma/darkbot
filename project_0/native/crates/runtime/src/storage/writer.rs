//! SK-04: the market journal's one ordered writer, off the capture thread.
//!
//! The capture thread encodes a typed row (no JSON), reserves its bytes in the `MemoryBudget`
//! and hands the frame to a dedicated writer over the existing bounded channel; it never writes,
//! flushes or syncs. The writer appends admitted frames in order in bounded batches (one
//! sequential write per batch, `Writer::append_batch`) and syncs at the configured interval and at
//! seal. OS-buffered frames are not crash-safe before a sync; an unsealed tail is INCOMPLETE.
//! A writer failure is sticky: every later submit and the seal fail, the capture stops
//! INCOMPLETE. Financial FE-04/P8 durability is a separate writer lane and is not relaxed here.
use super::budget::{peak_rss_bytes, BudgetSnapshot, BytePermit, MemoryBudget, MIB};
use crate::analytics::journal::{self, Provenance};
use crate::analytics::payload::{self, Payload};
use darkbot_common::channel::{bounded, ChannelError, Consumer, Producer};
use std::io;
use std::path::{Path, PathBuf};
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
        }
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
    pub peak_rss_bytes: Option<u64>,
    pub budget: BudgetSnapshot,
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

/// Per-frame bytes beyond the payload: frame header, chain hash, CRC, SHA and queue slot.
const FRAME_OVERHEAD: u64 = 160 + std::mem::size_of::<Pending>() as u64;

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
        let writer = journal::Writer::create(path)
            .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        // Queue slots are bounded twice: by count here and by bytes through the budget.
        let (tx, rx) = bounded::<Msg>("market-journal-writer", cfg.batch_frames * 4);
        let failure = Arc::new(Mutex::new(None));
        let (thread_failure, thread_budget) = (failure.clone(), budget.clone());
        let handle = std::thread::Builder::new()
            .name("market-journal-writer".into())
            .spawn(move || {
                let result = write_loop(writer, rx, cfg, &thread_budget);
                if let Err(e) = &result {
                    *thread_failure.lock().unwrap_or_else(|p| p.into_inner()) =
                        Some(format!("MARKET_WRITER_FAILED: {e}"));
                }
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
            path: path.into(),
        })
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
        let bytes = payload::encode(row)?;
        let deadline = Instant::now() + self.stall_budget;
        let permit = self
            .budget
            .acquire(bytes.len() as u64 + FRAME_OVERHEAD, self.stall_budget)
            .map_err(io::Error::other)?;
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
                Ok(()) => return Ok(()),
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
    mut writer: journal::Writer,
    rx: Consumer<Msg>,
    cfg: WriterConfig,
    budget: &MemoryBudget,
) -> io::Result<Sealed> {
    let mut m = StorageMetrics::default();
    let mut batch: Vec<Pending> = Vec::with_capacity(cfg.batch_frames);
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
        let due = first_pending.is_some_and(|t| t.elapsed() >= cfg.flush_interval);
        if !batch.is_empty() && (batch.len() >= cfg.batch_frames || due || seal.is_some()) {
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
            m.bytes_written = writer.position();
            // Written: release the reserved bytes of the whole batch.
            batch.clear();
            first_pending = None;
        }
        if seal.is_none() && last_sync.elapsed() >= cfg.sync_interval && m.frames > 0 {
            let started = Instant::now();
            writer.sync()?;
            m.syncs += 1;
            m.max_sync_ns = m.max_sync_ns.max(started.elapsed().as_nanos() as u64);
            last_sync = Instant::now();
        }
        if let Some(complete) = seal {
            let durable = writer.seal(complete)?;
            m.syncs += 1;
            m.bytes_written = writer.position();
            m.peak_rss_bytes = peak_rss_bytes();
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
    use crate::market::observer::rows::{MarketCoverage, MarketTransaction};

    fn cfg(max: u64) -> WriterConfig {
        WriterConfig {
            initial_bytes: max,
            max_bytes: max,
            system_reserve_bytes: 0,
            batch_frames: 4,
            flush_interval: Duration::from_millis(5),
            sync_interval: Duration::from_millis(20),
            stall_budget: Duration::from_millis(50),
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

    /// A row larger than the hard maximum is refused, never dropped silently; a dropped worker
    /// seals INCOMPLETE; a write failure is sticky for every later submit.
    #[test]
    fn worker_refuses_over_budget_and_failures_are_sticky() {
        let d = dir("budget");
        let mut w = MarketJournalWorker::start(&d.join("a.p0j"), cfg(400)).unwrap();
        let big = MarketTransaction {
            signature: "s".repeat(1_000),
            slot: 1,
            origin: crate::market::observer::rows::TxOrigin::TransactionSubscribe {
                observed_unix_s: 0,
                t_rx_ns: 0,
                tx: Err("TX_DECODE".into()),
            },
        };
        let e = w.submit(Provenance::default(), &big).unwrap_err();
        assert_eq!(e.to_string(), "BYTE_PERMIT_EXCEEDS_MAX");
        w.submit(Provenance::default(), &row(1)).unwrap();
        drop(w);
        let mut r = journal::Reader::open(&d.join("a.p0j")).unwrap();
        while r.next_frame().unwrap().is_some() {}
        assert!(r.sealed && !r.complete);
        // Sticky failure: once the writer thread failed, submit and seal report it.
        let mut w = MarketJournalWorker::start(&d.join("b.p0j"), cfg(1 << 20)).unwrap();
        *w.failure.lock().unwrap() = Some("MARKET_WRITER_FAILED: injected".into());
        assert!(w
            .submit(Provenance::default(), &row(2))
            .unwrap_err()
            .to_string()
            .contains("injected"));
        std::fs::remove_dir_all(d).unwrap();
    }
}

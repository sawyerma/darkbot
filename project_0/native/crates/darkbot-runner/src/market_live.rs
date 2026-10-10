//! Bounded read-only live canonical market projection.
//!
//! The full-window final reconciliation remains authoritative. This worker merely
//! provides independently tracked FINALIZED block counters during capture, reusing
//! the exact existing full-slot reconciliation + four-path classifier.
//! Each batch is published only after its entire slot range has reconciled cleanly.
//! Failed/429 batches never advance the cursor and are retried without duplicate output.

use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

const BATCH_SLOTS: u64 = 24;
const POLL_MS: u64 = 750;

#[derive(Clone, Debug)]
pub(super) struct Snapshot {
    pub events: u64,
    pub classic: u64,
    pub receivership: u64,
    pub deleverage: u64,
    pub bankruptcy: u64,
    pub known_gross_events: u64,
    pub gross_usd: f64,
    pub scanned_through: u64,
    pub target_slot: u64,
    pub errors: u64,
    pub status: &'static str,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            events: 0, classic: 0, receivership: 0, deleverage: 0, bankruptcy: 0,
            known_gross_events: 0, gross_usd: 0.0, scanned_through: 0,
            target_slot: 0, errors: 0, status: "WAITING_FOR_MARKET_SLOTS",
        }
    }
}

pub(super) struct LiveScan {
    first: Arc<AtomicU64>,
    last: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<Snapshot>>,
    worker: Option<JoinHandle<()>>,
}

impl LiveScan {
    pub fn start(cfg: &MarketConfig) -> Result<Self, String> {
        let first = Arc::new(AtomicU64::new(0));
        let last = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let worker = if let Some(url) = cfg.rpc_url.clone() {
            let first_bg = first.clone();
            let last_bg = last.clone();
            let stop_bg = stop.clone();
            let state_bg = state.clone();
            let filename = cfg.run_dir.join("live_finalized.jsonl");
            let program = cfg.program;
            let reconcile_cfg = cfg.reconcile;
            Some(std::thread::Builder::new()
                .name("market-live-finalized".into())
                .spawn(move || scan_loop(&url, &filename, program, reconcile_cfg,
                    &first_bg, &last_bg, &stop_bg, &state_bg))
                .map_err(|e| format!("market live scanner spawn: {e}"))?)
        } else {
            state.lock().unwrap().status = "NO_RPC_FINALIZED_SCAN_UNAVAILABLE";
            None
        };
        Ok(Self { first, last, stop, state, worker })
    }

    pub fn observe_slots(&self, first: Option<u64>, last: Option<u64>) {
        if let (Some(lo), Some(hi)) = (first, last) {
            if lo > 0 && hi >= lo {
                let _ = self.first.compare_exchange(0, lo, Ordering::AcqRel, Ordering::Acquire);
                self.last.fetch_max(hi, Ordering::Release);
                self.state.lock().unwrap().target_slot = hi;
            }
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                eprintln!("[MARKET/FINALIZED] live_scanner_thread_panicked=true final_scan_remains_authoritative");
            }
        }
    }
}

impl Drop for LiveScan {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Default)]
struct CountSink;
impl MarketSink for CountSink {
    fn transaction(&mut self, _: &Value) -> std::io::Result<()> { Ok(()) }
    fn liquidation(&mut self, _: &Value) -> std::io::Result<()> { Ok(()) }
    fn coverage(&mut self, _: &Value) -> std::io::Result<()> { Ok(()) }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

fn note_failure(state: &Mutex<Snapshot>, why: &'static str) {
    let mut s = state.lock().unwrap();
    s.errors += 1;
    s.status = why;
    eprintln!("[MARKET/FINALIZED] status=RETRY reason={} last_valid_slot={} total_errors={}",
        why, s.scanned_through, s.errors);
}

fn scan_loop(
    url: &str,
    path: &Path,
    program: Pubkey,
    cfg: ReconcileConfig,
    first: &AtomicU64,
    last: &AtomicU64,
    stop: &AtomicBool,
    state: &Mutex<Snapshot>,
) {
    let file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(f) => f,
        Err(_) => {
            note_failure(state, "LIVE_JOURNAL_OPEN_FAILED");
            return;
        }
    };
    let mut journal = BufWriter::new(file);
    let rpc = match RpcFetcher::new(url.to_string(), 8_000, "finalized") {
        Ok(v) => v,
        Err(_) => {
            note_failure(state, "LIVE_RPC_INIT_FAILED");
            return;
        }
    };
    let mut cursor = 0_u64;
    let mut retry_ms = 750_u64;
    while !stop.load(Ordering::Acquire) {
        let low = first.load(Ordering::Acquire);
        let high = last.load(Ordering::Acquire);
        if low == 0 || high < low {
            std::thread::sleep(Duration::from_millis(POLL_MS));
            continue;
        }
        if cursor == 0 {
            cursor = low;
        }
        if cursor > high {
            state.lock().unwrap().status = "FINALIZED_CAUGHT_UP_TO_OBSERVED";
            std::thread::sleep(Duration::from_millis(POLL_MS));
            continue;
        }
        let tip = rpc.gap_request(
            "getSlot", json!([{"commitment":"finalized"}]),
            Instant::now() + Duration::from_secs(8), 8_000,
        );
        let tip = match tip {
            Ok(v) => match v.as_u64() {
                Some(n) => n,
                None => {
                    note_failure(state, "LIVE_FINALIZED_TIP_INVALID");
                    std::thread::sleep(Duration::from_millis(retry_ms));
                    retry_ms = (retry_ms * 2).min(15_000);
                    continue;
                }
            },
            Err(e) => {
                note_failure(state, e);
                std::thread::sleep(Duration::from_millis(retry_ms));
                retry_ms = (retry_ms * 2).min(15_000);
                continue;
            }
        };
        let upper = high.min(tip).min(cursor.saturating_add(BATCH_SLOTS - 1));
        if upper < cursor {
            state.lock().unwrap().status = "WAITING_FOR_FINALITY";
            std::thread::sleep(Duration::from_millis(POLL_MS));
            continue;
        }
        let mut core = ObserverCore::new(program, MarketSource::Logs, EnrichConfig::default());
        core.reconcile_cfg = cfg;
        core.reconcile_cfg.requests_per_second = cfg.requests_per_second.min(4);
        core.reconcile_cfg.max_attempts = cfg.max_attempts.min(3);
        core.reconcile_cfg.phase_max_secs = cfg.phase_max_secs.min(120);
        core.reconcile_cfg.finality_wait_secs = cfg.finality_wait_secs.min(2);
        let bound = |slot| reconcile::SlotBoundary {
            slot, t_rx_ns: 0, observed_unix_s: unix_now_s(),
            provenance: "LIVE_FINALIZED_BATCH",
        };
        core.reconciliation.market_slot_start = Some(bound(cursor));
        core.reconciliation.market_slot_end = Some(bound(upper));
        state.lock().unwrap().status = "FINALIZED_SCAN_RUNNING";
        let mut sink = CountSink;
        let run = core.full_reconciliation_phase(Some(url), &mut sink);
        if run.is_err() || !core.reconciliation.complete {
            let reason = core.reconciliation.error.unwrap_or("LIVE_SLOT_SCAN_IO");
            note_failure(state, reason);
            std::thread::sleep(Duration::from_millis(retry_ms));
            retry_ms = (retry_ms * 2).min(15_000);
            continue;
        }
        // Commit a whole verified batch atomically to the live counter: no
        // duplicate increments from incomplete/retried block scans.
        for event in &core.truth.events {
            let row = json!({"source":"live_finalized_slot_scan", "event":event});
            if writeln!(journal, "{row}").is_err() {
                note_failure(state, "LIVE_JOURNAL_WRITE_FAILED");
                return;
            }
        }
        if journal.flush().is_err() {
            note_failure(state, "LIVE_JOURNAL_FLUSH_FAILED");
            return;
        }
        {
            let mut s = state.lock().unwrap();
            for event in &core.truth.events {
                s.events += 1;
                match event.liquidation_type {
                    truth::LiquidationType::Classic => s.classic += 1,
                    truth::LiquidationType::Receivership => s.receivership += 1,
                    truth::LiquidationType::ForcedDeleverage => s.deleverage += 1,
                    truth::LiquidationType::BankruptcySettlement => s.bankruptcy += 1,
                }
                if let Some(gross) = event.gross_edge_usd {
                    s.known_gross_events += 1;
                    s.gross_usd += gross;
                }
                eprintln!(
                    "[MARKET/FINALIZED] type={} slot={} signature={} event={} target={} gross_usd={}",
                    event.liquidation_type.as_str(), event.slot, event.signature,
                    event.event_index, event.target,
                    event.gross_edge_usd.map(|v|format!("{v:.6}")).unwrap_or_else(||"UNKNOWN".into())
                );
            }
            s.scanned_through = upper;
            s.status = "FINALIZED_BATCH_VERIFIED";
        }
        cursor = upper.saturating_add(1);
        retry_ms = 750;
    }
}

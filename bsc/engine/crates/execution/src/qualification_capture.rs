//! G0.7 bounded qualification capture session: the background owner of the
//! `darkbot-bsc-journal` recorder and journal writer. Migrated unchanged from the
//! retired Gate-0 harness, which compiled the capture crate in by `#[path]`;
//! it now uses the real `darkbot-bsc-journal` crate. Built only with the
//! `g07-mainnet` feature; not part of the production runner.
// Moved unchanged from Gate-0: the invariant u32 field-length expect remains as
// qualified. Since the G0.7 hardening the journal file is created with mode
// 0600, the wall-clock stamp saturates instead of panicking (records are also
// written after the release) and the writer reports what it has appended.
#![allow(clippy::expect_used)]

pub use darkbot_bsc_journal::{CaptureRecord, JournalManifest, JournalWriter, RecordKind};
use darkbot_bsc_journal::{Recorder, RecorderStats};
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const G07_CAPTURE_SCHEMA: &str = "darkbot.g07.capture.v1";
pub const DEFAULT_CAPTURE_CAPACITY: usize = 256;
pub const DEFAULT_CAPTURE_MAX_PAYLOAD: usize = 4 * 1024 * 1024;

#[derive(Clone)]
pub struct CaptureClock {
    epoch: Instant,
}

impl CaptureClock {
    pub fn now_ns(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}

#[derive(Debug, Clone)]
pub struct CaptureSummary {
    pub path: PathBuf,
    pub records: u64,
    pub first_sequence: Option<u64>,
    pub last_sequence: Option<u64>,
    pub bytes: u64,
    pub sha256_hex: String,
    pub next_sequence: u64,
    pub enqueued: u64,
    pub dropped_full: u64,
    pub rejected_oversize: u64,
    pub disconnected: u64,
    pub complete_local_journal: bool,
}

pub struct CaptureSession {
    path: PathBuf,
    recorder: Option<Recorder>,
    writer: Option<JoinHandle<Result<JournalManifest, String>>>,
    clock: CaptureClock,
    failed: Arc<AtomicBool>,
    /// Records the writer has appended to the file (write, not yet fsync).
    appended: Arc<AtomicU64>,
}

fn unix_time_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn text_payload(event: &str, body: &str) -> Vec<u8> {
    format!(
        "schema={G07_CAPTURE_SCHEMA}\nevent={event}\nunix_time_ns={}\n{body}",
        unix_time_ns()
    )
    .into_bytes()
}

fn push_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("capture field exceeds u32");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

fn raw_tx_payload(label: &str, tx_hash: &str, raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(24 + label.len() + tx_hash.len() + raw.len());
    out.extend_from_slice(b"DBRTX001");
    push_len_prefixed(&mut out, label.as_bytes());
    push_len_prefixed(&mut out, tx_hash.as_bytes());
    push_len_prefixed(&mut out, raw);
    out
}

impl CaptureSession {
    pub fn prearm(
        path: &Path,
        attempt_id: &str,
        clock_id: &str,
        boot_id: &str,
        producer_witness_source: &str,
    ) -> Result<Self, String> {
        if attempt_id.trim().is_empty()
            || clock_id.trim().is_empty()
            || boot_id.trim().is_empty()
            || producer_witness_source.trim().is_empty()
        {
            return Err("capture identity fields must be non-empty".into());
        }

        let mut options = OpenOptions::new();
        options.create_new(true).read(true).write(true);
        // The journal holds broadcastable signed bytes: owner read/write only.
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let file = options
            .open(path)
            .map_err(|e| format!("create-new capture {}: {e}", path.display()))?;
        file.sync_all()
            .map_err(|e| format!("sync capture fence {}: {e}", path.display()))?;

        let (recorder, drain) =
            Recorder::bounded(DEFAULT_CAPTURE_CAPACITY, DEFAULT_CAPTURE_MAX_PAYLOAD)
                .map_err(|e| format!("create bounded capture recorder: {e}"))?;

        let failed = Arc::new(AtomicBool::new(false));
        let writer_failed = Arc::clone(&failed);
        let appended = Arc::new(AtomicU64::new(0));
        let writer_appended = Arc::clone(&appended);
        let (prearm_ack_tx, prearm_ack_rx) = mpsc::sync_channel::<Result<(), String>>(1);
        let writer = thread::Builder::new()
            .name("g07-capture-writer".into())
            .spawn(move || {
                let mut journal = JournalWriter::new(file);
                let mut prearm_ack = Some(prearm_ack_tx);
                loop {
                    match drain.recv() {
                        Ok(record) => {
                            if let Err(e) = journal.append(&record) {
                                writer_failed.store(true, Ordering::Release);
                                if let Some(tx) = prearm_ack.take() {
                                    let _ = tx.send(Err(format!("capture append failed: {e}")));
                                }
                                return Err(format!("capture append failed: {e}"));
                            }
                            writer_appended.fetch_add(1, Ordering::Release);
                            if let Some(tx) = prearm_ack.take() {
                                if let Err(e) = journal.sync_file() {
                                    writer_failed.store(true, Ordering::Release);
                                    let msg = format!("capture pre-arm sync failed: {e}");
                                    let _ = tx.send(Err(msg.clone()));
                                    return Err(msg);
                                }
                                let _ = tx.send(Ok(()));
                            }
                        }
                        Err(_) => break,
                    }
                }
                journal.commit_file().map_err(|e| format!("capture final commit failed: {e}"))
            })
            .map_err(|e| format!("spawn capture writer: {e}"))?;

        let mut session = Self {
            path: path.to_path_buf(),
            recorder: Some(recorder),
            writer: Some(writer),
            clock: CaptureClock {
                epoch: Instant::now(),
            },
            failed,
            appended,
        };

        let body = format!(
            "attempt_id={attempt_id}\nclock_id={clock_id}\nboot_id={boot_id}\nproducer_witness_source={producer_witness_source}\nprimary_timing_authority=PRODUCER_WITNESS_NOT_LOCAL_OBSERVER"
        );
        if let Err(e) = session.record_text("SESSION_PREARMED", session.clock.now_ns(), &body) {
            let _ = session.finish();
            return Err(e);
        }
        match prearm_ack_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(session),
            Ok(Err(e)) => {
                let _ = session.finish();
                Err(e)
            }
            Err(e) => {
                session.failed.store(true, Ordering::Release);
                let _ = session.finish();
                Err(format!("capture writer did not acknowledge pre-arm: {e}"))
            }
        }
    }

    pub fn clock(&self) -> CaptureClock {
        self.clock.clone()
    }

    pub fn healthy(&self) -> bool {
        !self.failed.load(Ordering::Acquire)
    }

    /// Waits, bounded, until the writer has appended every record enqueued so
    /// far (written to the file, no fsync): what is journaled before the
    /// release is on file before any bundle byte leaves.
    pub fn wait_appended(&self, timeout: Duration) -> Result<(), String> {
        let Some(recorder) = self.recorder.as_ref() else {
            return Err("capture recorder already closed".into());
        };
        let target = recorder.stats().enqueued;
        let deadline = Instant::now() + timeout;
        loop {
            if self.failed.load(Ordering::Acquire) {
                return Err("capture writer failed".into());
            }
            if self.appended.load(Ordering::Acquire) >= target {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "capture writer appended {} of {target} records within {} ms",
                    self.appended.load(Ordering::Acquire),
                    timeout.as_millis()
                ));
            }
            thread::sleep(Duration::from_micros(100));
        }
    }

    pub fn stats(&self) -> Option<RecorderStats> {
        self.recorder.as_ref().map(Recorder::stats)
    }

    fn record(
        &mut self,
        kind: RecordKind,
        timestamp_ns: u64,
        payload: &[u8],
    ) -> Result<(), String> {
        let Some(recorder) = self.recorder.as_ref() else {
            self.failed.store(true, Ordering::Release);
            return Err("capture recorder already closed".into());
        };
        recorder
            .try_record(kind, timestamp_ns, payload)
            .map(|_| ())
            .map_err(|e| {
                self.failed.store(true, Ordering::Release);
                format!("capture enqueue failed: {e:?}")
            })
    }

    pub fn record_text(
        &mut self,
        event: &str,
        timestamp_ns: u64,
        body: &str,
    ) -> Result<(), String> {
        self.record(
            RecordKind::Runtime,
            timestamp_ns,
            &text_payload(event, body),
        )
    }

    pub fn record_raw_tx(
        &mut self,
        label: &str,
        timestamp_ns: u64,
        tx_hash: &str,
        raw: &[u8],
    ) -> Result<(), String> {
        self.record(
            RecordKind::RawSignedTx,
            timestamp_ns,
            &raw_tx_payload(label, tx_hash, raw),
        )
    }

    pub fn finish(mut self) -> Result<CaptureSummary, String> {
        let stats = self
            .recorder
            .as_ref()
            .map(Recorder::stats)
            .unwrap_or_default();
        drop(self.recorder.take());

        let writer = self.writer.take().ok_or("capture writer already joined")?;
        let manifest = writer
            .join()
            .map_err(|_| "capture writer panicked".to_string())??;

        let failed = self.failed.load(Ordering::Acquire);
        let complete_local_journal = !failed
            && stats.dropped_full == 0
            && stats.rejected_oversize == 0
            && stats.disconnected == 0
            && stats.enqueued == manifest.records;

        Ok(CaptureSummary {
            path: self.path,
            records: manifest.records,
            first_sequence: manifest.first_sequence,
            last_sequence: manifest.last_sequence,
            bytes: manifest.bytes,
            sha256_hex: hex::encode(manifest.sha256),
            next_sequence: stats.next_sequence,
            enqueued: stats.enqueued,
            dropped_full: stats.dropped_full,
            rejected_oversize: stats.rejected_oversize,
            disconnected: stats.disconnected,
            complete_local_journal,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn prearmed_capture_finishes_with_integrity_manifest() {
        let path = std::env::temp_dir().join(format!(
            "darkbot-g07-capture-test-{}-{}.bin",
            std::process::id(),
            unix_time_ns()
        ));
        let mut session = CaptureSession::prearm(
            &path,
            "attempt-test",
            "local-test-clock",
            "test-boot",
            "test producer witness source",
        )
        .unwrap();
        let clock = session.clock();
        session
            .record_text("TEST", clock.now_ns(), "value=1")
            .unwrap();
        session.wait_appended(Duration::from_secs(2)).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let summary = session.finish().unwrap();
        assert!(summary.complete_local_journal);
        assert_eq!(summary.records, 2);
        assert_eq!(summary.enqueued, 2);
        assert_ne!(summary.sha256_hex, "0".repeat(64));
        fs::remove_file(path).unwrap();
    }
}

//! The BSC runtime's binary journal (`DBJNL001`, V2 framing of
//! `darkbot-bsc-journal`): the runtime thread enqueues without waiting into a
//! bounded recorder; a writer thread appends to `<dir>/<started_ms>.dbjnl`
//! (create-new, 0600). A full queue loses the record visibly (counted, and the
//! sequence gap is proven on replay), never silently.
//!
//! What it holds (binary payloads, no JSON):
//! - `RawSignedTx`: the first verified full sighting of a Flap launch,
//!   `source u8 | hash [32] | canonical signed bytes`.
//! - `Runtime` `b'L'`: a later sighting of a launch from another source,
//!   `b'L' | source u8 | hash [32] | lag_ns u64 LE`.
//! - `Runtime` `b'S'`: a source transition, `b'S' | source u8 | event u8`
//!   (see [`SourceEvent`]).
//!
//! Other pending transactions are only counted (runtime status), not
//! journaled: the journal stays bounded over long read-only runs.

use crate::sources::SourceId;
use darkbot_bsc_journal::{JournalError, JournalManifest, JournalWriter, RecordKind, Recorder};
use std::fs;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};

/// A source transition recorded in the journal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SourceEvent {
    Started = 1,
    Stopped = 2,
    Connected = 3,
    Disconnected = 4,
    RateLimited = 5,
}

fn source_byte(source: SourceId) -> u8 {
    SourceId::ALL.iter().position(|s| *s == source).unwrap_or(0) as u8
}

pub fn first_sighting_payload(source: SourceId, hash: &[u8; 32], raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(33 + raw.len());
    out.push(source_byte(source));
    out.extend_from_slice(hash);
    out.extend_from_slice(raw);
    out
}

pub fn later_sighting_payload(source: SourceId, hash: &[u8; 32], lag_ns: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(42);
    out.push(b'L');
    out.push(source_byte(source));
    out.extend_from_slice(hash);
    out.extend_from_slice(&lag_ns.to_le_bytes());
    out
}

pub fn source_event_payload(source: SourceId, event: SourceEvent) -> Vec<u8> {
    vec![b'S', source_byte(source), event as u8]
}

pub struct RuntimeJournal {
    path: PathBuf,
    recorder: Option<Recorder>,
    writer: Option<JoinHandle<Result<JournalManifest, String>>>,
    appended: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
    lost: u64,
}

impl RuntimeJournal {
    /// Creates `<dir>/<started_ms>.dbjnl` (dir 0700) and its writer.
    pub fn open(
        dir: &Path,
        started_ms: u64,
        capacity: usize,
        max_payload: usize,
    ) -> Result<Self, String> {
        match fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(dir)
        {
            Ok(()) => {}
            Err(e) => return Err(format!("{}: {e}", dir.display())),
        }
        let path = dir.join(format!("{started_ms}.dbjnl"));
        // Read too: the commit hashes the exact bytes written.
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("create-new {}: {e}", path.display()))?;
        let (recorder, mut drain) = Recorder::bounded_spanned(capacity, max_payload)
            .map_err(|e| format!("journal recorder: {e}"))?;
        let appended = Arc::new(AtomicU64::new(0));
        let failed = Arc::new(AtomicBool::new(false));
        let (w_appended, w_failed) = (Arc::clone(&appended), Arc::clone(&failed));
        let writer = thread::Builder::new()
            .name("bsc-runtime-journal".into())
            .spawn(move || {
                let mut journal = JournalWriter::new_v2(file);
                loop {
                    match drain.recv_spanned() {
                        Ok(record) => {
                            if let Err(e) = journal.append(&record) {
                                w_failed.store(true, Ordering::Release);
                                return Err(format!("journal append failed: {e}"));
                            }
                            w_appended.fetch_add(1, Ordering::Release);
                        }
                        Err(JournalError::QueueDisconnected) => break,
                        Err(e) => {
                            w_failed.store(true, Ordering::Release);
                            return Err(format!("journal drain failed: {e}"));
                        }
                    }
                }
                journal
                    .commit_file()
                    .map_err(|e| format!("journal commit failed: {e}"))
            })
            .map_err(|e| format!("spawn journal writer: {e}"))?;
        Ok(RuntimeJournal {
            path,
            recorder: Some(recorder),
            writer: Some(writer),
            appended,
            failed,
            lost: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Enqueues without waiting; `false` when the record was lost (counted).
    pub fn record(&mut self, kind: RecordKind, timestamp_ns: u64, payload: &[u8]) -> bool {
        let ok = self
            .recorder
            .as_mut()
            .is_some_and(|r| r.try_record(kind, timestamp_ns, payload).is_ok());
        if !ok {
            self.lost += 1;
        }
        ok
    }

    pub fn appended(&self) -> u64 {
        self.appended.load(Ordering::Acquire)
    }

    pub fn lost(&self) -> u64 {
        self.lost
    }

    pub fn healthy(&self) -> bool {
        !self.failed.load(Ordering::Acquire)
    }

    /// Closes the queue, lets the writer append what is queued and commit.
    pub fn finish(mut self) -> Result<JournalManifest, String> {
        drop(self.recorder.take());
        self.writer
            .take()
            .ok_or("journal writer already joined")?
            .join()
            .map_err(|_| "journal writer panicked".to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_bsc_journal::JournalReader;

    #[test]
    fn sightings_and_transitions_replay_in_order_with_their_payloads() {
        let dir = std::env::temp_dir().join(format!("bsc-runtime-journal-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut j = RuntimeJournal::open(&dir, 7, 64, 4096).unwrap();
        let h = [5u8; 32];
        assert!(j.record(
            RecordKind::Runtime,
            1,
            &source_event_payload(SourceId::Drpc, SourceEvent::Connected)
        ));
        assert!(j.record(
            RecordKind::RawSignedTx,
            2,
            &first_sighting_payload(SourceId::Drpc, &h, b"raw")
        ));
        assert!(j.record(
            RecordKind::Runtime,
            3,
            &later_sighting_payload(SourceId::PublicNode, &h, 40)
        ));
        let path = j.path().to_path_buf();
        let manifest = j.finish().unwrap();
        assert_eq!(manifest.logical_records, 3);
        let mut reader = JournalReader::new(fs::File::open(&path).unwrap(), 4096).unwrap();
        let mut got = Vec::new();
        while let Some(r) = reader.next_record().unwrap() {
            got.push((r.kind, r.timestamp_ns, r.payload.to_vec()));
        }
        assert_eq!(got[0], (RecordKind::Runtime, 1, vec![b'S', 2, 3]));
        assert_eq!(got[1].0, RecordKind::RawSignedTx);
        assert_eq!(&got[1].2[..33], &[&[2u8][..], &h[..]].concat()[..]);
        assert_eq!(&got[1].2[33..], b"raw");
        let mut later = vec![b'L', 1];
        later.extend_from_slice(&h);
        later.extend_from_slice(&40u64.to_le_bytes());
        assert_eq!(got[2], (RecordKind::Runtime, 3, later));
        // create-new: the same start time never overwrites a journal.
        assert!(RuntimeJournal::open(&dir, 7, 64, 4096).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_full_queue_loses_visibly_never_silently() {
        let dir =
            std::env::temp_dir().join(format!("bsc-runtime-journal-full-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut j = RuntimeJournal::open(&dir, 1, 1, 64).unwrap();
        // Oversize payloads are refused by the bounded recorder: counted.
        assert!(!j.record(RecordKind::Runtime, 1, &[0u8; 65]));
        assert_eq!(j.lost(), 1);
        j.finish().unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }
}

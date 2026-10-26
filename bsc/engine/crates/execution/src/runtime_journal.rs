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
//! - `Canonical` `b'P'`: a canonical launch received (not joined yet),
//!   `b'P' | venue u8 | block u64 | block_hash [32] | tx_hash [32]`; a `P`
//!   without its `C` or `O` was lost (crash) and is counted as unjoined.
//! - `Canonical` `b'C'`: a canonical launch joined with the sightings,
//!   `b'C' | venue u8 | block u64 | block_hash [32] | tx_index u64 | tx_hash
//!   [32] | token? u8 [20] | buys? u8 | external_buys u32 | class u8 | source
//!   u8 | delta_us u64 | block_ms u64 | tx u8 | fetch_lag? u8 i64` (integers
//!   LE; venue 1 Flap, 2 Four; class per
//!   [`Visibility::code`](crate::canonical_join::Visibility::code); source 0xFF
//!   and delta 0 for a miss; delta = lead or lag against the block time;
//!   fetch_lag = local wall ms at the header fetch minus block_ms; tx 0 verified
//!   outside the C1 scope, 1 verified in the C1 scope, 2 unverifiable).
//! - `Canonical` `b'O'`: a launch of an orphaned block taken out again,
//!   `b'O' | venue u8 | block u64 | block_hash [32] | tx_hash [32]`.
//! - `Reorg` `b'R'`: a reorg, `b'R' | depth u32 | new head u64 | hash [32]`.
//! - `Reorg` `b'G'`: a resync (gap or reorg deeper than the window),
//!   `b'G' | from u64 | to u64`.
//! - `Canonical` `b'H'` / `b'E'` / `b'K'`: the first / last canonical block
//!   of this process and periodic checkpoints of the head,
//!   `b'H'|b'E'|b'K' | number u64 | hash [32]`: the covered window, also
//!   after a crash (last `K`).
//! - `Runtime` `b'X'`: the clean end of this process (journal committed
//!   after it); its absence marks a killed or crashed process.
//!
//! Other pending transactions are only counted (runtime status), not
//! journaled: the journal stays bounded over long read-only runs.

use crate::canonical_join::{Joined, LaunchRef, Venue};
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

fn venue_byte(venue: Venue) -> u8 {
    match venue {
        Venue::Flap => 1,
        Venue::Four => 2,
    }
}

pub fn canonical_launch_payload(j: &Joined) -> Vec<u8> {
    let l = &j.launch;
    let mut out = Vec::with_capacity(152);
    out.push(b'C');
    out.push(venue_byte(l.venue));
    out.extend_from_slice(&l.block_number.to_le_bytes());
    out.extend_from_slice(&l.block_hash);
    out.extend_from_slice(&l.tx_index.to_le_bytes());
    out.extend_from_slice(&l.tx_hash);
    out.push(u8::from(l.token.is_some()));
    out.extend_from_slice(&l.token.unwrap_or([0; 20]));
    out.push(u8::from(l.external_buys.is_some()));
    out.extend_from_slice(&l.external_buys.unwrap_or(0).to_le_bytes());
    out.push(j.visibility.code());
    let (source, delta) = match j.visibility.source_and_delta() {
        Some((s, d)) => (source_byte(s), d),
        None => (0xFF, 0),
    };
    out.push(source);
    out.extend_from_slice(&delta.to_le_bytes());
    out.extend_from_slice(&l.block_ms.to_le_bytes());
    out.push(if l.tx_unverifiable {
        2
    } else {
        u8::from(l.c1_scope)
    });
    let lag = l.fetch_lag_ms();
    out.push(u8::from(lag.is_some()));
    out.extend_from_slice(&lag.unwrap_or(0).to_le_bytes());
    out
}

/// The length of a `C` record.
pub const CANONICAL_LAUNCH_LEN: usize = 136;

pub fn pending_launch_payload(l: &LaunchRef) -> Vec<u8> {
    let mut out = Vec::with_capacity(74);
    out.push(b'P');
    out.push(venue_byte(l.venue));
    out.extend_from_slice(&l.block_number.to_le_bytes());
    out.extend_from_slice(&l.block_hash);
    out.extend_from_slice(&l.tx_hash);
    out
}

/// The clean end of the process (`Runtime` kind).
pub const EXIT_PAYLOAD: [u8; 1] = [b'X'];

pub fn orphaned_launch_payload(l: &LaunchRef) -> Vec<u8> {
    let mut out = Vec::with_capacity(74);
    out.push(b'O');
    out.push(venue_byte(l.venue));
    out.extend_from_slice(&l.block_number.to_le_bytes());
    out.extend_from_slice(&l.block_hash);
    out.extend_from_slice(&l.tx_hash);
    out
}

pub fn reorg_payload(depth: u32, head: u64, hash: &[u8; 32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(45);
    out.push(b'R');
    out.extend_from_slice(&depth.to_le_bytes());
    out.extend_from_slice(&head.to_le_bytes());
    out.extend_from_slice(hash);
    out
}

pub fn resync_payload(from: u64, to: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(17);
    out.push(b'G');
    out.extend_from_slice(&from.to_le_bytes());
    out.extend_from_slice(&to.to_le_bytes());
    out
}

/// A window mark: `b'H'` first, `b'E'` last, `b'K'` checkpoint.
pub fn window_payload(tag: u8, number: u64, hash: &[u8; 32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(41);
    out.push(tag);
    out.extend_from_slice(&number.to_le_bytes());
    out.extend_from_slice(hash);
    out
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
    fn canonical_payloads_have_their_fixed_layout() {
        use crate::canonical_join::{Joined, LaunchRef, Venue, Visibility};
        let launch = LaunchRef {
            venue: Venue::Flap,
            block_number: 7,
            block_hash: [1; 32],
            tx_index: 3,
            tx_hash: [2; 32],
            token: Some([4; 20]),
            external_buys: Some(5),
            c1_scope: true,
            tx_unverifiable: false,
            block_ms: 1_000,
            block_seen_ns: 0,
            block_seen_unix_ms: Some(990),
        };
        let joined = Joined {
            launch,
            visibility: Visibility::FullBefore {
                source: SourceId::PublicNode,
                lead_us: 9,
            },
        };
        let c = canonical_launch_payload(&joined);
        assert_eq!(c.len(), CANONICAL_LAUNCH_LEN);
        assert_eq!(&c[..2], &[b'C', 1]);
        assert_eq!(&c[2..10], &7u64.to_le_bytes());
        assert_eq!(&c[42..50], &3u64.to_le_bytes());
        assert_eq!(
            &c[82..108],
            &[&[1u8][..], &[4u8; 20][..], &[1, 5, 0, 0, 0][..]].concat()[..]
        );
        assert_eq!(
            &c[108..118],
            &[&[1u8, 1][..], &9u64.to_le_bytes()[..]].concat()[..]
        );
        assert_eq!(&c[118..126], &1_000u64.to_le_bytes());
        // C1 scope, then a signed fetch lag: the local clock read 10 ms
        // before the block time (it is behind).
        assert_eq!(
            &c[126..],
            &[&[1u8, 1][..], &(-10i64).to_le_bytes()[..]].concat()[..]
        );
        let missed = Joined {
            launch,
            visibility: Visibility::Missed,
        };
        let m = canonical_launch_payload(&missed);
        assert_eq!(
            &m[108..118],
            &[&[4u8, 0xFF][..], &[0u8; 8][..]].concat()[..]
        );
        assert_eq!(orphaned_launch_payload(&launch).len(), 74);
        assert_eq!(pending_launch_payload(&launch)[..2], [b'P', 1]);
        assert_eq!(pending_launch_payload(&launch).len(), 74);
        assert_eq!(reorg_payload(2, 9, &[3; 32])[..5], [b'R', 2, 0, 0, 0]);
        assert_eq!(resync_payload(1, 2).len(), 17);
        assert_eq!(window_payload(b'K', 1, &[0; 32])[0], b'K');
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

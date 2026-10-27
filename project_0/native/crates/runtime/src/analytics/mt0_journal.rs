//! MT0 snapshots into the one binary journal (PERSIST-01). Runs only on the reporter thread:
//! the owner records into lock-free atomics and never sees JSON, files or this writer.
//! JFI-04: a snapshot is a typed `MT0_SNAPSHOT` frame (`Mt0Telemetry`, schema 2) — the same typed
//! form the econ reports carry; its JSON row exists only as the explicit offline view
//! (`Mt0Telemetry::legacy_json`). Historical schema-1 value-tree frames stay readable.
use super::econ::Mt0Telemetry;
use super::journal::{self, Provenance};
use crate::storage::segments::SegmentedWriter;
use darkbot_common::mt0::{Snapshot, SnapshotSink};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct Mt0Journal {
    writer: SegmentedWriter,
    lost: bool,
    /// DEC-16: the window the owner opened last, and the window this journal's segment is for.
    rotate: Option<(Arc<AtomicU64>, u64)>,
}
impl Mt0Journal {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        Ok(Self {
            writer: SegmentedWriter::create(path, 0)?,
            lost: false,
            rotate: None,
        })
    }
    /// DEC-16: the MT0 journal of an unbounded run, a segment chain sealed at the first snapshot
    /// after each window boundary the owner announces in `window` (the window it opened) and at
    /// `max_segment_bytes`; `recover` continues the chain of the crashed run a start resumes.
    pub fn rolling(
        path: &Path,
        max_segment_bytes: u64,
        window: Arc<AtomicU64>,
        recover: bool,
    ) -> std::io::Result<Self> {
        let writer = if recover {
            SegmentedWriter::recover(path, max_segment_bytes)?.0
        } else {
            SegmentedWriter::create(path, max_segment_bytes)?
        };
        let open = window.load(Ordering::Acquire);
        Ok(Self {
            writer,
            lost: false,
            rotate: Some((window, open)),
        })
    }
}
impl SnapshotSink for Mt0Journal {
    /// A typed frame of the snapshot; no JSON on the way.
    fn store(&mut self, unix_ns: u64, mono_ns: u64, snapshot: &Snapshot<'_>) -> bool {
        if self.lost {
            return false;
        }
        if let Some((window, segment_for)) = &mut self.rotate {
            let open = window.load(Ordering::Acquire);
            if open > *segment_for {
                if self.writer.rotate_now().is_err() {
                    self.lost = true;
                    return false;
                }
                *segment_for = open;
            }
        }
        let provenance = Provenance {
            source: journal::source::MT0,
            source_seq: self.writer.events(),
            rx_ns: Some(mono_ns),
            unix_ns: Some(unix_ns),
            ..Default::default()
        };
        let stored = self
            .writer
            .append_typed(provenance, &Mt0Telemetry::from(snapshot))
            .is_ok()
            && self.writer.rotate_if_due().is_ok();
        self.lost |= !stored;
        stored
    }
    /// A lost snapshot seals the journal INCOMPLETE; a failed seal is reported, never hidden. A
    /// chain continued after a crash is never complete (DEC-16).
    fn finish(&mut self, complete: bool) -> bool {
        let complete = complete && !self.lost && !self.writer.recovered();
        self.writer.seal(complete).is_ok() && complete
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_common::lifecycle::Shutdown;
    use std::time::Duration;

    fn dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "p0-mt0-{}-{}",
            std::process::id(),
            solana_sdk::pubkey::Pubkey::new_unique()
        ));
        std::fs::create_dir(&dir).unwrap();
        dir
    }

    #[test]
    fn reporter_writes_final_snapshot_and_seals_complete() {
        let dir = dir();
        let path = dir.join("mt0.p0j");
        let sink = Mt0Journal::create(&path).unwrap();
        let reporter =
            darkbot_common::mt0::spawn_reporter(Duration::from_secs(3600), Shutdown::new(), sink)
                .unwrap();
        assert!(reporter.finish());
        let mut reader = journal::Reader::open(&path).unwrap();
        let first = reader.next_frame().unwrap().unwrap();
        assert_eq!((first.kind, first.schema), (journal::kind::MT0_SNAPSHOT, 2));
        let telemetry: super::super::econ::Mt0Telemetry = first.typed().unwrap();
        assert!(telemetry.unix_s > 0);
        assert!(first.provenance.unix_ns.is_some() && first.provenance.rx_ns.is_some());
        assert_eq!(
            reader.next_frame().unwrap().unwrap().kind,
            journal::MANIFEST
        );
        assert!(reader.sealed && reader.complete);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lost_snapshot_seals_incomplete_and_is_reported() {
        let dir = dir();
        let path = dir.join("mt0.p0j");
        let mut sink = Mt0Journal::create(&path).unwrap();
        // A snapshot beyond the frame limit cannot be written: lost, and every later one too.
        let none = darkbot_common::mt0::HistSnapshot::default();
        let mut snapshot = darkbot_common::mt0::MT0.snapshot(1, 2, &none);
        let huge = "x".repeat(journal::MAX_FRAME);
        snapshot.counters.push((huge.as_str(), 1));
        assert!(!sink.store(1, 2, &snapshot));
        let small = darkbot_common::mt0::MT0.snapshot(1, 2, &none);
        assert!(!sink.store(1, 2, &small));
        assert!(!sink.finish(true));
        let mut reader = journal::Reader::open(&path).unwrap();
        assert_eq!(
            reader.next_frame().unwrap().unwrap().kind,
            journal::MANIFEST
        );
        assert!(reader.sealed && !reader.complete);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "historical")]
    /// JFI-04: a typed MT0 frame and the historical schema-1 frame of the same snapshot (the
    /// former writer: the snapshot text as a value tree) give the same offline row — the
    /// registry's `Snapshot::json` text — and the typed frame decodes to the snapshot's telemetry.
    #[test]
    fn jfi04_typed_mt0_frame_renders_the_historical_row() {
        let dir = dir();
        let path = dir.join("mt0.p0j");
        let duty = darkbot_common::mt0::HistSnapshot {
            count: 3,
            sum: 30,
            max: Some(20),
            p50: Some(10),
            p95: Some(20),
            p99: Some(20),
        };
        let snapshot = darkbot_common::mt0::MT0.snapshot(1_700_000_000, 5, &duty);
        let historical: serde_json::Value = serde_json::from_str(&snapshot.json()).unwrap();
        let mut sink = Mt0Journal::create(&path).unwrap();
        assert!(sink.store(7, 8, &snapshot));
        sink.writer
            .test_writer()
            .test_append_historical(journal::kind::MT0_SNAPSHOT, &historical)
            .unwrap();
        assert!(sink.finish(true));
        let mut reader = journal::Reader::open(&path).unwrap();
        let typed = reader.next_frame().unwrap().unwrap();
        let old = reader.next_frame().unwrap().unwrap();
        assert_eq!((typed.schema, old.schema), (2, 1));
        let view = |f: &journal::Frame| super::super::payload::legacy_view(f).unwrap();
        assert_eq!(view(&typed), historical);
        assert_eq!(view(&old), historical);
        assert_eq!(
            typed.typed::<Mt0Telemetry>().unwrap(),
            Mt0Telemetry::from(&snapshot)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// DEC-16: an unbounded run's MT0 journal seals its segment at the first snapshot after the
    /// owner opened the next window, so each window's snapshots stand in their own sealed
    /// segment; after a crash the chain continues (never complete) and a sealed one is refused.
    #[test]
    fn dec16_mt0_rotates_at_the_first_snapshot_after_a_boundary_and_recovers() {
        let dir = dir();
        let path = dir.join("mt0.p0j");
        let window = Arc::new(AtomicU64::new(1));
        let none = darkbot_common::mt0::HistSnapshot::default();
        let snapshot = darkbot_common::mt0::MT0.snapshot(1, 2, &none);
        let mut sink = Mt0Journal::rolling(&path, 0, window.clone(), false).unwrap();
        assert!(sink.store(10, 1, &snapshot));
        assert!(sink.store(11, 2, &snapshot));
        window.store(2, Ordering::Release);
        assert!(sink.store(12, 3, &snapshot));
        let unix = |path: &Path| {
            let mut r = journal::Reader::open(path).unwrap();
            let mut at = Vec::new();
            while let Some(f) = r.next_frame().unwrap() {
                if f.kind == journal::kind::MT0_SNAPSHOT {
                    at.push(f.provenance.unix_ns.unwrap());
                }
            }
            (at, r.sealed)
        };
        assert_eq!(unix(&path), (vec![10, 11], true), "window 1 alone, sealed");
        // A crash: the process ends without sealing.
        drop(sink);
        let mut again = Mt0Journal::rolling(&path, 0, window.clone(), true).unwrap();
        assert!(again.store(13, 4, &snapshot));
        assert!(!again.finish(true), "a recovered chain is never complete");
        let mut chain = crate::storage::segments::ChainReader::open(&path).unwrap();
        let mut at = Vec::new();
        while let Some(f) = chain.next_frame().unwrap() {
            if f.kind == journal::kind::MT0_SNAPSHOT {
                at.push(f.provenance.unix_ns.unwrap());
            }
        }
        assert_eq!(at, [10, 11, 12, 13]);
        assert!(chain.sealed && !chain.complete);
        assert!(Mt0Journal::rolling(&path, 0, window, true).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn existing_journal_is_never_overwritten() {
        let dir = dir();
        let path = dir.join("mt0.p0j");
        drop(Mt0Journal::create(&path).unwrap());
        assert!(Mt0Journal::create(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

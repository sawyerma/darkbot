//! MT0 snapshots into the one binary journal (PERSIST-01). Runs only on the reporter thread:
//! the owner records into lock-free atomics and never sees JSON, files or this writer.
//! JFI-04: a snapshot is a typed `MT0_SNAPSHOT` frame (`Mt0Telemetry`, schema 2) — the same typed
//! form the econ reports carry; its JSON row exists only as the explicit offline view
//! (`Mt0Telemetry::legacy_json`). Historical schema-1 value-tree frames stay readable.
use super::econ::Mt0Telemetry;
use super::journal::{self, Provenance, Writer};
use darkbot_common::mt0::{Snapshot, SnapshotSink};
use std::path::Path;

pub struct Mt0Journal {
    writer: Writer,
    lost: bool,
}
impl Mt0Journal {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        Ok(Self {
            writer: Writer::create(path)?,
            lost: false,
        })
    }
}
impl SnapshotSink for Mt0Journal {
    /// A typed frame of the snapshot; no JSON on the way.
    fn store(&mut self, unix_ns: u64, mono_ns: u64, snapshot: &Snapshot<'_>) -> bool {
        if self.lost {
            return false;
        }
        let provenance = Provenance {
            source: journal::source::MT0,
            source_seq: self.writer.next_seq(),
            rx_ns: Some(mono_ns),
            unix_ns: Some(unix_ns),
            ..Default::default()
        };
        let stored = self
            .writer
            .append_typed(provenance, &Mt0Telemetry::from(snapshot))
            .is_ok();
        self.lost |= !stored;
        stored
    }
    /// A lost snapshot seals the journal INCOMPLETE; a failed seal is reported, never hidden.
    fn finish(&mut self, complete: bool) -> bool {
        let complete = complete && !self.lost;
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
        let view = super::super::payload::legacy_view(&first).unwrap();
        assert_eq!(view["mt0_version"], 2);
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

    #[test]
    fn existing_journal_is_never_overwritten() {
        let dir = dir();
        let path = dir.join("mt0.p0j");
        drop(Mt0Journal::create(&path).unwrap());
        assert!(Mt0Journal::create(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

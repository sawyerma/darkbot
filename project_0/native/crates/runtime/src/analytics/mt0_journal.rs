//! MT0 snapshots into the one binary journal (PERSIST-01). Runs only on the reporter thread:
//! the owner records into lock-free atomics and never sees JSON, files or this writer.
use super::journal::{self, Provenance, Writer};
use darkbot_common::mt0::SnapshotSink;
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
    /// The snapshot text is the legacy `mt0.jsonl` row; the frame keeps its value losslessly.
    fn store(&mut self, unix_ns: u64, mono_ns: u64, snapshot: &str) -> bool {
        if self.lost {
            return false;
        }
        let stored = serde_json::from_str(snapshot).is_ok_and(|value| {
            let provenance = Provenance {
                source: journal::source::MT0,
                source_seq: self.writer.next_seq(),
                rx_ns: Some(mono_ns),
                unix_ns: Some(unix_ns),
                ..Default::default()
            };
            self.writer
                .append(journal::kind::MT0_SNAPSHOT, 1, provenance, &value)
                .is_ok()
        });
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
        assert_eq!(first.kind, journal::kind::MT0_SNAPSHOT);
        assert_eq!(first.value["mt0_version"], 2);
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
        assert!(!sink.store(1, 2, "{not json"));
        assert!(!sink.store(1, 2, r#"{"mt0_version":2}"#));
        assert!(!sink.finish(true));
        let mut reader = journal::Reader::open(&path).unwrap();
        assert_eq!(
            reader.next_frame().unwrap().unwrap().kind,
            journal::MANIFEST
        );
        assert!(reader.sealed && !reader.complete);
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

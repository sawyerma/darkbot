//! Append-only raw market evidence writers.
use super::*;
use crate::analytics::journal;

// ------------------------------------------------------------------------------------------
// Output sink
// ------------------------------------------------------------------------------------------

pub trait MarketSink {
    fn transaction(&mut self, row: &Value) -> std::io::Result<()>;
    fn liquidation(&mut self, row: &Value) -> std::io::Result<()>;
    fn coverage(&mut self, row: &Value) -> std::io::Result<()>;
    fn flush(&mut self) -> std::io::Result<()>;
    /// Called once by `finalize` after the last row. A journal-backed sink seals its journal,
    /// renders the legacy views and returns the binding recorded in `summary.json`.
    fn finish(&mut self) -> std::io::Result<Option<Value>> {
        self.flush().map(|_| None)
    }
}

/// Compatibility name used by the immutable strict verifier. Its only primary
/// storage is JournalSink; flush is the pinned verifier's completed-batch boundary.
/// The legacy three files are materialized once, after the journal is sealed.
pub struct JsonlSink {
    inner: JournalSink,
    exported: bool,
}
impl JsonlSink {
    pub fn create(run_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            inner: JournalSink::create(run_dir)?,
            exported: false,
        })
    }
}
impl MarketSink for JsonlSink {
    fn transaction(&mut self, row: &Value) -> std::io::Result<()> {
        self.inner.transaction(row)
    }
    fn liquidation(&mut self, row: &Value) -> std::io::Result<()> {
        self.inner.liquidation(row)
    }
    fn coverage(&mut self, row: &Value) -> std::io::Result<()> {
        self.inner.coverage(row)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if !self.exported {
            self.inner.seal_and_render(true)?;
            self.exported = true;
        }
        Ok(())
    }
}

/// The binary market journal of a capture (PERSIST-01), in the market directory.
pub const MARKET_JOURNAL: &str = "market.p0j";
/// Legacy JSONL views the pinned verifiers and reports read; renders of the journal kinds.
pub const MARKET_VIEWS: [(u16, &str); 3] = [
    (journal::kind::MARKET_TRANSACTION, "transactions.jsonl"),
    (journal::kind::MARKET_LIQUIDATION, "liquidations.jsonl"),
    (journal::kind::MARKET_COVERAGE, "coverage.jsonl"),
];

/// Production market sink: every row is one frame of `market.p0j`, written unbuffered on
/// the capture thread exactly where `JsonlSink` wrote its line. The three JSONL files are
/// rendered from the sealed journal (explicit export), never written during the capture.
pub struct JournalSink {
    writer: journal::Writer,
    run_dir: PathBuf,
    rows: u64,
    finished: bool,
}
impl JournalSink {
    /// Never overwrites: neither the journal nor any of its views may exist yet.
    pub fn create(run_dir: &Path) -> Result<Self, String> {
        for (_, name) in MARKET_VIEWS {
            let view = run_dir.join(name);
            if view.exists() {
                return Err(format!("cannot create {}: exists", view.display()));
            }
        }
        let path = run_dir.join(MARKET_JOURNAL);
        let writer = journal::Writer::create(&path)
            .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        Ok(Self {
            writer,
            run_dir: run_dir.into(),
            rows: 0,
            finished: false,
        })
    }
    fn append(&mut self, kind: u16, row: &Value) -> std::io::Result<()> {
        let provenance = journal::Provenance {
            source: journal::source::MARKET,
            source_seq: self.rows,
            slot: row
                .get("slot")
                .or_else(|| row.pointer("/event/slot"))
                .and_then(Value::as_u64),
            rx_ns: row.get("t_rx_ns").and_then(Value::as_u64),
            ..Default::default()
        };
        self.writer.append_legacy(kind, provenance, row)?;
        self.rows += 1;
        Ok(())
    }
    fn seal_and_render(&mut self, complete: bool) -> std::io::Result<Value> {
        self.finished = true;
        let durable = self.writer.seal(complete)?;
        let views: Vec<(Option<u16>, PathBuf)> = MARKET_VIEWS
            .iter()
            .map(|(kind, name)| (Some(*kind), self.run_dir.join(name)))
            .collect();
        let refs: Vec<(Option<u16>, &Path)> =
            views.iter().map(|(k, p)| (*k, p.as_path())).collect();
        let rows = journal::export_views(&self.writer.path, &refs, false)?;
        let views: serde_json::Map<String, Value> = MARKET_VIEWS
            .iter()
            .zip(rows)
            .map(|((kind, name), rows)| (name.to_string(), json!({"kind":kind,"rows":rows})))
            .collect();
        Ok(
            json!({"schema":"Project0/MarketJournalBinding/1","journal":MARKET_JOURNAL,
            "event_frames":self.rows,"root_sha256":journal::hex(&durable.hash),
            "status":if complete {"COMPLETE"} else {"INCOMPLETE"},"views":views,
            "views_are":"explicit exports of the journal kinds; the journal is primary"}),
        )
    }
}
impl MarketSink for JournalSink {
    fn transaction(&mut self, row: &Value) -> std::io::Result<()> {
        self.append(journal::kind::MARKET_TRANSACTION, row)
    }
    fn liquidation(&mut self, row: &Value) -> std::io::Result<()> {
        self.append(journal::kind::MARKET_LIQUIDATION, row)
    }
    fn coverage(&mut self, row: &Value) -> std::io::Result<()> {
        self.append(journal::kind::MARKET_COVERAGE, row)
    }
    /// Frames are written unbuffered, so every row already left the process (as a flushed
    /// `JsonlSink` line did); durability is the fsync at seal.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
    fn finish(&mut self) -> std::io::Result<Option<Value>> {
        if self.finished {
            return Err(std::io::Error::other("MARKET_JOURNAL_ALREADY_FINISHED"));
        }
        self.seal_and_render(true).map(Some)
    }
}
impl Drop for JournalSink {
    /// An early exit (warmup failure, error path) still seals INCOMPLETE and renders the views.
    fn drop(&mut self) {
        if !self.finished {
            if let Err(e) = self.seal_and_render(false) {
                eprintln!("MARKET_JOURNAL_INCOMPLETE {e}");
            }
        }
    }
}

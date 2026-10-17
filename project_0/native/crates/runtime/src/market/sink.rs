//! Append-only raw market evidence writers.
use super::rows::{MarketCoverage, MarketLiquidation, MarketTransaction};
use super::*;
use crate::analytics::journal;
use crate::storage::writer::{MarketJournalWorker, StorageMetrics, WriterConfig};

// ------------------------------------------------------------------------------------------
// Output sink
// ------------------------------------------------------------------------------------------

/// Typed market evidence (JFI-FIX-03): producers hand over native rows, never JSON.
pub trait MarketSink {
    fn transaction(&mut self, row: &MarketTransaction) -> std::io::Result<()>;
    fn liquidation(&mut self, row: &MarketLiquidation) -> std::io::Result<()>;
    fn coverage(&mut self, row: &MarketCoverage) -> std::io::Result<()>;
    fn flush(&mut self) -> std::io::Result<()>;
    /// Called once by `finalize` after the last row. A journal-backed sink seals its journal
    /// and returns the binding recorded in the summary.
    fn finish(&mut self) -> std::io::Result<Option<MarketJournalBinding>> {
        self.flush().map(|_| None)
    }
}

/// Seal result of a market journal: what the summary binds.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct MarketJournalBinding {
    pub schema: &'static str,
    pub journal: &'static str,
    pub event_frames: u64,
    pub root_sha256: String,
    pub status: &'static str,
    pub views_are: &'static str,
    /// SK-03/SK-04: measured writer lane and capture byte budget of this journal.
    pub storage: StorageMetrics,
}

/// Batch sink of the strict verifier: the batch journal plus the three legacy views the pinned
/// report reader consumes, rendered once from the sealed batch journal at the batch boundary.
/// Capture never renders; this compatibility output ends with the native report reader.
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
    fn transaction(&mut self, row: &MarketTransaction) -> std::io::Result<()> {
        self.inner.transaction(row)
    }
    fn liquidation(&mut self, row: &MarketLiquidation) -> std::io::Result<()> {
        self.inner.liquidation(row)
    }
    fn coverage(&mut self, row: &MarketCoverage) -> std::io::Result<()> {
        self.inner.coverage(row)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if !self.exported {
            self.inner.seal(true)?;
            let views: Vec<(Option<u16>, PathBuf)> = MARKET_VIEWS
                .iter()
                .map(|(kind, name)| (Some(*kind), self.inner.run_dir.join(name)))
                .collect();
            let refs: Vec<(Option<u16>, &Path)> =
                views.iter().map(|(k, p)| (*k, p.as_path())).collect();
            journal::export_views(&self.inner.run_dir.join(MARKET_JOURNAL), &refs, false)?;
            self.exported = true;
        }
        Ok(())
    }
}

/// The binary market journal of a capture (PERSIST-01), in the market directory.
pub const MARKET_JOURNAL: &str = "market.p0j";
/// Legacy JSONL views of the pinned readers: explicit offline renders of the journal kinds.
pub const MARKET_VIEWS: [(u16, &str); 3] = [
    (journal::kind::MARKET_TRANSACTION, "transactions.jsonl"),
    (journal::kind::MARKET_LIQUIDATION, "liquidations.jsonl"),
    (journal::kind::MARKET_COVERAGE, "coverage.jsonl"),
];

/// Production market sink: every row is one typed frame of `market.p0j`. The capture thread
/// only encodes and admits rows within the byte budget; the dedicated writer lane writes them
/// (SK-03/SK-04). No view is rendered during or after capture; `p0-journal renders` exports
/// them explicitly offline.
pub struct JournalSink {
    writer: Option<MarketJournalWorker>,
    run_dir: PathBuf,
    rows: u64,
}
impl JournalSink {
    /// Never overwrites: neither the journal nor any of its views may exist yet. Budgets and
    /// batching come from the active operational configuration.
    pub fn create(run_dir: &Path) -> Result<Self, String> {
        Self::create_with(run_dir, WriterConfig::from_active())
    }
    pub fn create_with(run_dir: &Path, cfg: WriterConfig) -> Result<Self, String> {
        for (_, name) in MARKET_VIEWS {
            let view = run_dir.join(name);
            if view.exists() {
                return Err(format!("cannot create {}: exists", view.display()));
            }
        }
        let writer = MarketJournalWorker::start(&run_dir.join(MARKET_JOURNAL), cfg)?;
        Ok(Self {
            writer: Some(writer),
            run_dir: run_dir.into(),
            rows: 0,
        })
    }
    fn append<P: crate::analytics::payload::Payload>(
        &mut self,
        row: &P,
        slot: Option<u64>,
        rx_ns: Option<u64>,
    ) -> std::io::Result<()> {
        let provenance = journal::Provenance {
            source: journal::source::MARKET,
            source_seq: self.rows,
            slot,
            rx_ns,
            ..Default::default()
        };
        self.writer
            .as_mut()
            .ok_or_else(|| std::io::Error::other("MARKET_JOURNAL_ALREADY_FINISHED"))?
            .submit(provenance, row)?;
        self.rows += 1;
        Ok(())
    }
    /// SK-05 live lane state for the terminal (`None` once sealed).
    pub fn storage_status(&mut self) -> Option<crate::storage::writer::StorageStatus> {
        self.writer.as_mut().map(|w| w.status())
    }
    fn seal(&mut self, complete: bool) -> std::io::Result<MarketJournalBinding> {
        let writer = self
            .writer
            .take()
            .ok_or_else(|| std::io::Error::other("MARKET_JOURNAL_ALREADY_FINISHED"))?;
        let sealed = writer.seal(complete)?;
        Ok(MarketJournalBinding {
            schema: "Project0/MarketJournalBinding/3",
            journal: MARKET_JOURNAL,
            event_frames: self.rows,
            root_sha256: journal::hex(&sealed.durable.hash),
            status: if complete { "COMPLETE" } else { "INCOMPLETE" },
            views_are: "typed journal frames; legacy JSONL views only by explicit offline export",
            storage: sealed.metrics,
        })
    }
}
impl MarketSink for JournalSink {
    fn transaction(&mut self, row: &MarketTransaction) -> std::io::Result<()> {
        self.append(row, Some(row.slot), row.t_rx_ns())
    }
    fn liquidation(&mut self, row: &MarketLiquidation) -> std::io::Result<()> {
        self.append(row, Some(row.slot()), row.t_rx_ns())
    }
    fn coverage(&mut self, row: &MarketCoverage) -> std::io::Result<()> {
        self.append(row, row.slot(), row.t_rx_ns())
    }
    /// The writer lane flushes on its batch/interval bounds and syncs on its interval; durability
    /// of the whole journal is the sync at seal (OS-buffered frames are not crash-safe before).
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
    fn finish(&mut self) -> std::io::Result<Option<MarketJournalBinding>> {
        self.seal(true).map(Some)
    }
}
// An early exit (warmup failure, error path) drops the worker, which seals INCOMPLETE.

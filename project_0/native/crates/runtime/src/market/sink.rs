//! Append-only raw market evidence writers.
use super::*;

// ------------------------------------------------------------------------------------------
// Output sink
// ------------------------------------------------------------------------------------------

pub trait MarketSink {
    fn transaction(&mut self, row: &Value) -> std::io::Result<()>;
    fn liquidation(&mut self, row: &Value) -> std::io::Result<()>;
    fn coverage(&mut self, row: &Value) -> std::io::Result<()>;
    fn flush(&mut self) -> std::io::Result<()>;
}

pub struct JsonlSink {
    transactions: BufWriter<File>,
    liquidations: BufWriter<File>,
    coverage: BufWriter<File>,
}

pub(super) fn create_new(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("cannot create {}: {e}", path.display()))
}

pub(super) fn write_line(w: &mut BufWriter<File>, row: &Value) -> std::io::Result<()> {
    serde_json::to_writer(&mut *w, row).map_err(std::io::Error::other)?;
    w.write_all(b"\n")
}

impl JsonlSink {
    /// Never overwrites: every file is created with `create_new`.
    pub fn create(run_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            transactions: BufWriter::new(create_new(&run_dir.join("transactions.jsonl"))?),
            liquidations: BufWriter::new(create_new(&run_dir.join("liquidations.jsonl"))?),
            coverage: BufWriter::new(create_new(&run_dir.join("coverage.jsonl"))?),
        })
    }
}

impl MarketSink for JsonlSink {
    fn transaction(&mut self, row: &Value) -> std::io::Result<()> {
        write_line(&mut self.transactions, row)
    }
    fn liquidation(&mut self, row: &Value) -> std::io::Result<()> {
        write_line(&mut self.liquidations, row)
    }
    fn coverage(&mut self, row: &Value) -> std::io::Result<()> {
        write_line(&mut self.coverage, row)?;
        self.coverage.flush() // coverage events are rare and must survive a kill
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.transactions.flush()?;
        self.liquidations.flush()?;
        self.coverage.flush()
    }
}

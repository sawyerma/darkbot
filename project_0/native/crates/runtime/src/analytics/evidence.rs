//! Typed owner evidence. Serialization and file IO run only on the writer thread.
//! A full queue closes admission, retains the refused event, and drains in order.
use super::journal::{self, Provenance, Writer};
use super::{record::OpportunityRecord, shadow_ledger::DaySummary};
use crate::execution::pending::ReconcileRecord;
use crate::market::winner::{RaceOutcome, Winner};
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::JoinHandle,
};

/// Fixed bound, plus one refused event and one terminal summary. No blocking send
/// is used during admission. This is a capacity bound, not a latency guarantee.
pub const OWNER_EVIDENCE_CAPACITY: usize = 1024;

pub enum HealthEvidence {
    Known {
        liquidatable: bool,
        maintenance_health: i128,
        equity_assets: i128,
        bad_debt: bool,
    },
    Unavailable,
    Error(String),
}

pub enum RecordEvent {
    IntegrationPlan {
        account: Pubkey,
        trigger: &'static str,
        slot: u64,
        reason: String,
    },
    IntegrationSkip {
        account: Pubkey,
        reason: String,
    },
    IntegrationPending {
        account: Pubkey,
        trigger: &'static str,
        slot: u64,
        integration_kinds: Vec<&'static str>,
        pricing_integration_kinds: Vec<&'static str>,
    },
    Health {
        account: Pubkey,
        trigger: &'static str,
        t_rx_ns: u64,
        slot: u64,
        evaluation_slot: u64,
        evaluation_unix_s: u64,
        result: HealthEvidence,
    },
    Race {
        account: Pubkey,
        winner: Winner,
        our_sign_slot: Option<u64>,
        outcome: RaceOutcome,
        expected_net: Option<i128>,
    },
    ProjectionReady {
        account: Pubkey,
        base_slot: u64,
        projection_slot: u64,
        projection_unix_s: i64,
        integration_kinds: Vec<&'static str>,
        pricing_integration_kinds: Vec<&'static str>,
        simulated_cu: Option<u64>,
    },
    ProjectionStale {
        account: Pubkey,
        base_slot: u64,
        projection_slot: u64,
        canonical_matches: bool,
    },
    ProjectionFailed {
        account: Pubkey,
        base_slot: u64,
        error: String,
    },
}
impl RecordEvent {
    fn value(self) -> Value {
        match self {
            Self::IntegrationPlan {
                account,
                trigger,
                slot,
                reason,
            } => {
                json!({"account":account.to_string(),"trigger":trigger,"slot":slot,"decision":"Skip","loss":"IntegrationRefresh","reason":reason})
            }
            Self::IntegrationSkip { account, reason } => {
                json!({"account":account.to_string(),"decision":"Skip","loss":"IntegrationRefresh","reason":reason})
            }
            Self::IntegrationPending {
                account,
                trigger,
                slot,
                integration_kinds,
                pricing_integration_kinds,
            } => {
                json!({"account":account.to_string(),"trigger":trigger,"slot":slot,"decision":"Skip","loss":"IntegrationRefresh","integration_kinds":integration_kinds,"pricing_integration_kinds":pricing_integration_kinds,"reason":"integration projection pending"})
            }
            Self::Health {
                account,
                trigger,
                t_rx_ns,
                slot,
                evaluation_slot,
                evaluation_unix_s,
                result,
            } => {
                let mut v = json!({"account":account.to_string(),"trigger":trigger,"t_rx_ns":t_rx_ns,"slot":slot,"evaluation_slot":evaluation_slot,"evaluation_unix_s":evaluation_unix_s});
                match result {
                    HealthEvidence::Known {
                        liquidatable,
                        maintenance_health,
                        equity_assets,
                        bad_debt,
                    } => {
                        v["liquidatable"] = json!(liquidatable);
                        v["maint_health_bits"] = json!(maintenance_health);
                        v["equity_assets_bits"] = json!(equity_assets);
                        v["bad_debt"] = json!(bad_debt);
                    }
                    HealthEvidence::Unavailable => v["decision"] = json!("Unavailable"),
                    HealthEvidence::Error(error) => v["error"] = json!(error),
                }
                v
            }
            Self::Race {
                account,
                winner,
                our_sign_slot,
                outcome,
                expected_net,
            } => {
                json!({"race":{"account":account.to_string(),"winner":winner,"our_sign_slot":our_sign_slot,"outcome":outcome,"expected_net":expected_net}})
            }
            Self::ProjectionReady {
                account,
                base_slot,
                projection_slot,
                projection_unix_s,
                integration_kinds,
                pricing_integration_kinds,
                simulated_cu,
            } => {
                json!({"projection":{"account":account.to_string(),"base_slot":base_slot,"projection_slot":projection_slot,"projection_unix_s":projection_unix_s,"status":"ready","integration_kinds":integration_kinds,"pricing_integration_kinds":pricing_integration_kinds,"simulated_cu":simulated_cu}})
            }
            Self::ProjectionStale {
                account,
                base_slot,
                projection_slot,
                canonical_matches,
            } => {
                json!({"projection":{"account":account.to_string(),"base_slot":base_slot,"projection_slot":projection_slot,"status":"stale","canonical_matches":canonical_matches}})
            }
            Self::ProjectionFailed {
                account,
                base_slot,
                error,
            } => {
                json!({"projection":{"account":account.to_string(),"base_slot":base_slot,"status":"failed","error":error}})
            }
        }
    }
}

pub enum OwnerEvidence {
    Record(RecordEvent),
    Opportunity(Box<OpportunityRecord>),
    Reconcile(ReconcileRecord),
    Summary(DaySummary),
    /// A formatted owner diagnostic. The owner never writes stderr itself (PERSIST-02).
    Diagnostic(String),
}
impl OwnerEvidence {
    fn encode(self) -> Result<(u16, Value), serde_json::Error> {
        match self {
            Self::Record(v) => Ok((journal::kind::OWNER_RECORD, v.value())),
            Self::Opportunity(v) => {
                Ok((journal::kind::OWNER_OPPORTUNITY, serde_json::to_value(v)?))
            }
            Self::Reconcile(v) => Ok((journal::kind::OWNER_RECONCILE, serde_json::to_value(v)?)),
            Self::Summary(v) => Ok((journal::kind::OWNER_SUMMARY, serde_json::to_value(v)?)),
            Self::Diagnostic(line) => Ok((journal::kind::OWNER_DIAGNOSTIC, json!({"line": line}))),
        }
    }
}
struct Terminal {
    refused: Option<OwnerEvidence>,
    summary: Option<DaySummary>,
    incomplete: bool,
}

/// Static category only: safe to surface without formatting provider payloads.
#[derive(Debug)]
pub struct EvidenceFailure(pub(crate) &'static str);
impl std::fmt::Display for EvidenceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for EvidenceFailure {}
impl From<&'static str> for EvidenceFailure {
    fn from(code: &'static str) -> Self {
        Self(code)
    }
}
pub struct OwnerEvidenceWriter {
    tx: Option<mpsc::SyncSender<OwnerEvidence>>,
    terminal: Option<mpsc::Sender<Terminal>>,
    worker: Option<JoinHandle<Result<(), &'static str>>>,
    failed: Arc<AtomicBool>,
    refused: Option<OwnerEvidence>,
    admission_error: Option<&'static str>,
}
impl OwnerEvidenceWriter {
    pub fn start(path: PathBuf, recovery: PathBuf, capacity: usize) -> std::io::Result<Self> {
        if capacity == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "zero evidence capacity",
            ));
        }
        let journal = Writer::create(&path)?;
        let (tx, rx) = mpsc::sync_channel(capacity);
        let (terminal, terminal_rx) = mpsc::channel();
        let failed = Arc::new(AtomicBool::new(false));
        let signal = failed.clone();
        let worker = std::thread::Builder::new()
            .name("p0-owner-evidence".into())
            .spawn(move || run_writer(journal, recovery, signal, rx, terminal_rx))?;
        Ok(Self {
            tx: Some(tx),
            terminal: Some(terminal),
            worker: Some(worker),
            failed,
            refused: None,
            admission_error: None,
        })
    }
    pub fn check(&self) -> Result<(), EvidenceFailure> {
        if let Some(e) = self.admission_error {
            return Err(e.into());
        }
        if self.failed.load(Ordering::Acquire) {
            return Err("OWNER_EVIDENCE_WRITER_FAILED".into());
        }
        if self.tx.is_none() {
            return Err("OWNER_EVIDENCE_CLOSED".into());
        }
        Ok(())
    }
    /// On error the caller MUST leave admission immediately and call finish.
    /// The refused event remains owned here, never overwritten or silently dropped.
    pub fn emit(&mut self, event: OwnerEvidence) -> Result<(), EvidenceFailure> {
        if let Err(e) = self.check() {
            if self.refused.is_none() {
                self.refused = Some(event);
            } else {
                return Err("OWNER_EVIDENCE_EMIT_AFTER_FAILURE".into());
            }
            self.admission_error = Some(e.0);
            return Err(e);
        }
        let failure = match self.tx.as_ref().expect("checked open").try_send(event) {
            Ok(()) => return Ok(()),
            Err(mpsc::TrySendError::Full(event)) => ("OWNER_EVIDENCE_OVERFLOW", event),
            Err(mpsc::TrySendError::Disconnected(event)) => ("OWNER_EVIDENCE_DISCONNECTED", event),
        };
        self.admission_error = Some(failure.0);
        self.refused = Some(failure.1);
        Err(failure.0.into())
    }
    /// Owner diagnostics go through the same bounded queue; the writer journals the line and
    /// only then prints it. A full queue closes admission like any other evidence.
    pub fn diagnostic(&mut self, line: String) -> Result<(), EvidenceFailure> {
        self.emit(OwnerEvidence::Diagnostic(line))
    }
    pub fn finish(&mut self, summary: Option<DaySummary>) -> Result<(), EvidenceFailure> {
        let Some(worker) = self.worker.take() else {
            return self.check();
        };
        // The separate one-message terminal channel cannot be blocked by a full data queue.
        let terminal = self.terminal.take().expect("terminal");
        let terminal_ok = terminal
            .send(Terminal {
                refused: self.refused.take(),
                summary,
                incomplete: self.admission_error.is_some() || self.failed.load(Ordering::Acquire),
            })
            .is_ok();
        drop(terminal);
        drop(self.tx.take());
        let result = worker
            .join()
            .map_err(|_| "OWNER_EVIDENCE_WRITER_PANICKED")?;
        result?;
        if !terminal_ok {
            return Err("OWNER_EVIDENCE_TERMINAL_LOST".into());
        }
        if let Some(e) = self.admission_error {
            return Err(e.into());
        }
        Ok(())
    }
}
impl Drop for OwnerEvidenceWriter {
    fn drop(&mut self) {
        if self.worker.is_some() {
            if let Err(e) = self.finish(None) {
                eprintln!("{e}");
            }
        }
    }
}

struct Sink {
    journal: Writer,
    recovery: PathBuf,
    recovery_writer: Option<Writer>,
    failed: Arc<AtomicBool>,
    io_failed: bool,
    recovery_failed: bool,
}
impl Sink {
    fn write(&mut self, event: OwnerEvidence) {
        let (kind, value) = match event.encode() {
            Ok(v) => v,
            Err(_) => {
                self.failed.store(true, Ordering::Release);
                self.recovery_failed = true;
                return;
            }
        };
        let console = (kind == journal::kind::OWNER_DIAGNOSTIC)
            .then(|| value["line"].as_str().map(str::to_owned))
            .flatten();
        self.store(kind, value);
        // The console copy of an owner diagnostic, printed here, after its frame.
        if let Some(line) = console {
            eprintln!("{line}");
        }
    }
    fn store(&mut self, kind: u16, value: Value) {
        let offset = self.journal.position();
        let p = Provenance {
            source: journal::source::OWNER,
            source_seq: self.journal.next_seq(),
            slot: value
                .get("slot")
                .or_else(|| value.get("rx_slot"))
                .and_then(Value::as_u64),
            rx_ns: value
                .get("t_rx_ns")
                .or_else(|| value.pointer("/timing/t_rx_ns"))
                .and_then(Value::as_u64),
            ..Default::default()
        };
        if !self.io_failed && self.journal.append(kind, 1, p, &value).is_ok() {
            return;
        }
        self.io_failed = true;
        self.failed.store(true, Ordering::Release);
        if self.recovery_writer.is_none() && !self.recovery_failed {
            match Writer::create(&self.recovery) {
                Ok(w) => self.recovery_writer = Some(w),
                Err(_) => self.recovery_failed = true,
            }
        }
        if let Some(w) = &mut self.recovery_writer {
            let value = json!({"original_journal_offset":offset,"event_kind":kind,"event":value});
            if w.append_value(journal::kind::OWNER_RECOVERY, &value)
                .is_err()
            {
                self.recovery_failed = true;
            }
        }
    }
    fn finish(mut self, incomplete: bool) -> Result<(), &'static str> {
        if let Some(w) = &mut self.recovery_writer {
            if w.seal(false).is_err() {
                self.recovery_failed = true;
            }
        }
        if self.recovery_failed {
            return Err("OWNER_EVIDENCE_RECOVERY_FAILED");
        }
        if self.io_failed {
            return Err("OWNER_EVIDENCE_WRITE_FAILED_RECOVERY_SAVED");
        }
        self.journal
            .seal(!incomplete)
            .map_err(|_| "OWNER_EVIDENCE_SYNC_FAILED")?;
        Ok(())
    }
}
fn run_writer(
    journal: Writer,
    recovery: PathBuf,
    signal: Arc<AtomicBool>,
    rx: mpsc::Receiver<OwnerEvidence>,
    terminal_rx: mpsc::Receiver<Terminal>,
) -> Result<(), &'static str> {
    let mut sink = Sink {
        journal,
        recovery,
        recovery_writer: None,
        failed: signal,
        io_failed: false,
        recovery_failed: false,
    };
    for event in rx {
        sink.write(event);
    }
    let mut incomplete = true;
    if let Ok(end) = terminal_rx.recv() {
        incomplete = end.incomplete;
        if let Some(event) = end.refused {
            sink.write(event);
        }
        if let Some(day) = end.summary {
            sink.write(OwnerEvidence::Summary(day));
        }
    }
    sink.finish(incomplete)
}
#[cfg(test)]
#[path = "evidence_tests.rs"]
mod tests;

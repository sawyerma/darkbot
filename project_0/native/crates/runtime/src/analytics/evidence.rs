//! Typed owner evidence. Serialization and file IO run only on the writer thread.
//! A full queue closes admission, retains the refused event, and drains in order.
use super::journal::{self, Provenance, Writer};
use super::payload::{self, OwnerDiagnostic, Payload};
use super::{record::OpportunityRecord, shadow_ledger::DaySummary};
use crate::execution::pending::ReconcileRecord;
use crate::market::winner::{RaceOutcome, Winner, WinnerIdentity};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::borrow::Cow;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// Fixed bound, plus one refused event and one terminal summary. No blocking send
/// is used during admission. This is a capacity bound, not a latency guarantee.
pub const OWNER_EVIDENCE_CAPACITY: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// Race winner as a native payload type (the JSON `Winner` is internally tagged for the
/// pinned JSON readers and cannot be decoded from a binary frame).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceWinner {
    pub slot: u64,
    pub identity: RaceWinnerIdentity,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RaceWinnerIdentity {
    Unique {
        signature: String,
        receiver: String,
        tx_version: String,
    },
    /// (signature, receiver, tx_version, transaction_index) of every same-slot candidate.
    SameSlotAmbiguous {
        candidates: Vec<(String, String, String, Option<u64>)>,
    },
}
impl From<&Winner> for RaceWinner {
    fn from(w: &Winner) -> Self {
        Self {
            slot: w.slot,
            identity: match &w.identity {
                WinnerIdentity::Unique {
                    signature,
                    receiver,
                    tx_version,
                } => RaceWinnerIdentity::Unique {
                    signature: signature.clone(),
                    receiver: receiver.clone(),
                    tx_version: tx_version.clone(),
                },
                WinnerIdentity::SameSlotAmbiguous { candidates } => {
                    RaceWinnerIdentity::SameSlotAmbiguous {
                        candidates: candidates
                            .iter()
                            .map(|c| {
                                (
                                    c.signature.clone(),
                                    c.receiver.clone(),
                                    c.tx_version.clone(),
                                    c.transaction_index,
                                )
                            })
                            .collect(),
                    }
                }
            },
        }
    }
}

/// Owner record payload (`kind::OWNER_RECORD`). Static strings stay borrowed on the owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordEvent {
    IntegrationPlan {
        account: Pubkey,
        trigger: Cow<'static, str>,
        slot: u64,
        reason: String,
    },
    IntegrationSkip {
        account: Pubkey,
        reason: String,
    },
    IntegrationPending {
        account: Pubkey,
        trigger: Cow<'static, str>,
        slot: u64,
        integration_kinds: Vec<Cow<'static, str>>,
        pricing_integration_kinds: Vec<Cow<'static, str>>,
    },
    Health {
        account: Pubkey,
        trigger: Cow<'static, str>,
        t_rx_ns: u64,
        slot: u64,
        evaluation_slot: u64,
        evaluation_unix_s: u64,
        result: HealthEvidence,
    },
    Race {
        account: Pubkey,
        winner: RaceWinner,
        our_sign_slot: Option<u64>,
        outcome: RaceOutcome,
        expected_net: Option<i128>,
    },
    ProjectionReady {
        account: Pubkey,
        base_slot: u64,
        projection_slot: u64,
        projection_unix_s: i64,
        integration_kinds: Vec<Cow<'static, str>>,
        pricing_integration_kinds: Vec<Cow<'static, str>>,
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
    /// DEC-01: a lane candidate refused before planning (loss `Build`, cause BLOCKHASH_STALE)
    /// was parked or coalesced into its parked entry, or a later evaluation of the account
    /// with a fresh blockhash superseded the entry; `cache_generation` is the blockhash
    /// cache's at the event.
    BlockhashPark {
        event: BlockhashParkEvent,
        entry: BlockhashParkEntry,
        cache_generation: u64,
    },
    /// DEC-01: the parked candidates a newer fresh blockhash (`cache_generation`) returned to
    /// their lanes, in one record per re-arm.
    BlockhashRearm {
        cache_generation: u64,
        entries: Vec<BlockhashParkEntry>,
    },
    /// DEC-03 (N-2): one applied settlement read of the FE-04 attempt (`exit: None`) or an exit:
    /// the subject's result class after it (`None`: not handed off, or released before N-2),
    /// the non-landing read it carried (every B source, C, calls and bytes) or why none was
    /// made. An open contradiction (`VetoContradiction`) is this record and an alarm.
    NonLanding {
        exit: Option<String>,
        class: Option<crate::execution::one_shot::non_landing::Verdict>,
        read: Option<crate::execution::one_shot::non_landing::NonLandingRead>,
        read_error: Option<String>,
    },
}
/// DEC-01: what happened to a parked candidate (`RecordEvent::BlockhashPark`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockhashParkEvent {
    Parked,
    Merged,
    Superseded,
}
/// DEC-01: a parked candidate's lane entry: work (`trigger`, `slot`, `t_rx_ns`), first receive
/// time, lane, input revision and the cache generation it waits beyond.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockhashParkEntry {
    pub account: Pubkey,
    pub trigger: Cow<'static, str>,
    pub slot: u64,
    pub t_rx_ns: u64,
    pub t_rx_first_ns: u64,
    pub lane: Cow<'static, str>,
    pub revision: u64,
    pub generation: u64,
}
pub type OwnerRecord = RecordEvent;
impl Payload for RecordEvent {
    const KIND: u16 = journal::kind::OWNER_RECORD;
}
impl RecordEvent {
    /// Frame provenance: the trigger slot and receive time where the event has them.
    fn provenance(&self) -> (Option<u64>, Option<u64>) {
        match self {
            Self::IntegrationPlan { slot, .. } | Self::IntegrationPending { slot, .. } => {
                (Some(*slot), None)
            }
            Self::Health { slot, t_rx_ns, .. } => (Some(*slot), Some(*t_rx_ns)),
            Self::BlockhashPark { entry, .. } => (Some(entry.slot), Some(entry.t_rx_ns)),
            _ => (None, None),
        }
    }
}

/// A primary-journal write that failed, saved verbatim in the recovery journal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerRecovery {
    pub original_journal_offset: u64,
    pub event_kind: u16,
    pub event_schema: u16,
    pub event: Vec<u8>,
}
impl Payload for OwnerRecovery {
    const KIND: u16 = journal::kind::OWNER_RECOVERY;
}

pub enum OwnerEvidence {
    Record(RecordEvent),
    Opportunity(Box<OpportunityRecord>),
    Reconcile(ReconcileRecord),
    Summary(DaySummary),
    /// A formatted owner diagnostic. The owner never writes stderr itself (PERSIST-02).
    Diagnostic(String),
    /// F1: bounded simulation logs of a failed candidate.
    SimulationFailure(Box<super::record::SimulationFailureRecord>),
}
/// One encoded owner event: kind, payload version, bytes and frame provenance.
struct Encoded {
    kind: u16,
    schema: u16,
    bytes: Vec<u8>,
    slot: Option<u64>,
    rx_ns: Option<u64>,
}
impl Encoded {
    fn of<P: Payload>(p: &P, slot: Option<u64>, rx_ns: Option<u64>) -> std::io::Result<Self> {
        Ok(Self {
            kind: P::KIND,
            schema: P::SCHEMA,
            bytes: payload::encode(p)?,
            slot,
            rx_ns,
        })
    }
}
impl OwnerEvidence {
    /// Typed binary encoding on the writer thread (JFI-02); no JSON in between.
    fn encode(&self) -> std::io::Result<Encoded> {
        match self {
            Self::Record(v) => {
                let (slot, rx_ns) = v.provenance();
                Encoded::of(v, slot, rx_ns)
            }
            Self::Opportunity(v) => Encoded::of(v.as_ref(), v.rx_slot, v.timing.t_rx_ns),
            Self::Reconcile(v) => Encoded::of(v, None, None),
            Self::Summary(v) => Encoded::of(v, None, None),
            Self::Diagnostic(line) => {
                Encoded::of(&OwnerDiagnostic { line: line.clone() }, None, None)
            }
            Self::SimulationFailure(v) => Encoded::of(v.as_ref(), v.rx_slot, None),
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
    syncs: Arc<AtomicU64>,
}
impl OwnerEvidenceWriter {
    /// `sync_interval` (M2): the journal is synced at least this often, also without new events,
    /// so a power loss loses at most one interval of owner evidence.
    pub fn start(
        path: PathBuf,
        recovery: PathBuf,
        capacity: usize,
        sync_interval: Duration,
    ) -> std::io::Result<Self> {
        Self::start_with(Writer::create(&path)?, recovery, capacity, sync_interval)
    }
    pub(crate) fn start_with(
        journal: Writer,
        recovery: PathBuf,
        capacity: usize,
        sync_interval: Duration,
    ) -> std::io::Result<Self> {
        if capacity == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "zero evidence capacity",
            ));
        }
        let (tx, rx) = mpsc::sync_channel(capacity);
        let (terminal, terminal_rx) = mpsc::channel();
        let failed = Arc::new(AtomicBool::new(false));
        let signal = failed.clone();
        let syncs = Arc::new(AtomicU64::new(0));
        let counted = syncs.clone();
        let worker = std::thread::Builder::new()
            .name("p0-owner-evidence".into())
            .spawn(move || {
                run_writer(
                    journal,
                    recovery,
                    signal,
                    rx,
                    terminal_rx,
                    sync_interval,
                    counted,
                )
            })?;
        Ok(Self {
            tx: Some(tx),
            terminal: Some(terminal),
            worker: Some(worker),
            failed,
            refused: None,
            admission_error: None,
            syncs,
        })
    }
    /// Periodic journal syncs completed so far (M2).
    pub fn syncs(&self) -> u64 {
        self.syncs.load(Ordering::Acquire)
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
                // R6: optional diagnostics only; the failure itself stays sticky in the writer.
                crate::console::line(e.to_string());
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
    syncs: Arc<AtomicU64>,
    max_sync_ns: u64,
}
impl Sink {
    /// M2: periodic durability point. A failed sync is a sticky writer failure like a failed
    /// append (the journal is blocked; later frames go to the recovery journal).
    fn sync(&mut self) {
        if self.io_failed {
            return;
        }
        let started = Instant::now();
        match self.journal.sync() {
            Ok(_) => {
                self.syncs.fetch_add(1, Ordering::Release);
                self.max_sync_ns = self
                    .max_sync_ns
                    .max(started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
            }
            Err(_) => {
                self.io_failed = true;
                self.failed.store(true, Ordering::Release);
            }
        }
    }
    fn write(&mut self, event: OwnerEvidence) {
        let encoded = match event.encode() {
            Ok(v) => v,
            Err(_) => {
                self.failed.store(true, Ordering::Release);
                self.recovery_failed = true;
                return;
            }
        };
        self.store(encoded);
        // The console copy of an owner diagnostic, after its frame; non-blocking (M2): a blocked
        // stderr drops console lines, never evidence.
        if let OwnerEvidence::Diagnostic(line) = event {
            crate::console::line(line);
        }
    }
    fn store(&mut self, e: Encoded) {
        let offset = self.journal.position();
        let p = Provenance {
            source: journal::source::OWNER,
            source_seq: self.journal.next_seq(),
            slot: e.slot,
            rx_ns: e.rx_ns,
            ..Default::default()
        };
        if !self.io_failed && self.journal.append(e.kind, e.schema, p, &e.bytes).is_ok() {
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
            let saved = OwnerRecovery {
                original_journal_offset: offset,
                event_kind: e.kind,
                event_schema: e.schema,
                event: e.bytes,
            };
            let p = Provenance {
                source: journal::source::OWNER,
                source_seq: w.next_seq(),
                ..Default::default()
            };
            if w.append_typed(p, &saved).is_err() {
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
    sync_interval: Duration,
    syncs: Arc<AtomicU64>,
) -> Result<(), &'static str> {
    let mut sink = Sink {
        journal,
        recovery,
        recovery_writer: None,
        failed: signal,
        io_failed: false,
        recovery_failed: false,
        syncs,
        max_sync_ns: 0,
    };
    let mut next_sync = Instant::now() + sync_interval;
    loop {
        match rx.recv_timeout(next_sync.saturating_duration_since(Instant::now())) {
            Ok(event) => sink.write(event),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if Instant::now() >= next_sync {
            sink.sync();
            next_sync = Instant::now() + sync_interval;
        }
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
    crate::console::line(format!(
        "[owner-evidence] periodic_syncs={} max_sync_ms={} console_dropped={}",
        sink.syncs.load(Ordering::Acquire),
        sink.max_sync_ns / 1_000_000,
        crate::console::dropped()
    ));
    sink.finish(incomplete)
}
#[cfg(test)]
#[path = "evidence_tests.rs"]
mod tests;

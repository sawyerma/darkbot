//! The BSC runtime's binary journal (`DBJNL001`, V2 framing of
//! `darkbot-bsc-journal`; Phase F storage): the runtime thread enqueues
//! without waiting into a recorder bounded by records and by bytes (the
//! capture `MemoryBudget`); the one `JournalWorker` appends to the run
//! directory `<dir>/<started_ms>/` as sealed segments (create-new, 0600),
//! syncs by the configured policy and, at the end, the run manifest
//! `capture-run.dbrec` (`darkbot.bsc.capture-run.v1`) names every segment
//! with its sequence range and exact SHA-256. A full queue or budget loses
//! the record visibly (counted, and the sequence gap is proven on replay),
//! never silently. Journals of older builds (`<dir>/<started_ms>.dbjnl`,
//! one file) stay readable by the census.
//!
//! What it holds (binary payloads, no JSON):
//! - `RawSignedTx`: the first verified full sighting of a launch (Flap C1
//!   scope or a Four.meme `createToken`, Phase F), `source u8 | hash [32] |
//!   canonical signed bytes`: a replay decodes them again.
//! - `TxpoolAccepted`: the first verified full sighting of another pending
//!   transaction, as far as the capture scope keeps them
//!   (`BSC_RUNTIME_CAPTURE_PENDING`: `VENUES` those addressed to a venue
//!   contract, the competitor flow; `ALL` every one), the same layout.
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
//!   u8 | delta_us u64 | block_ms u64 | tx u8 | fetch_lag? u8 i64 | four u8`
//!   (integers
//!   LE; venue 1 Flap, 2 Four Classic, 3 OpenFour; class per
//!   [`Visibility::code`](crate::canonical_join::Visibility::code); source 0xFF
//!   and delta 0 for a miss; delta = lead or lag against the block time;
//!   fetch_lag = local wall ms at the header fetch minus block_ms; tx 0 verified
//!   outside the C1 scope, 1 verified in the C1 scope (Four.meme: the pending
//!   decoder names the launch's token before execution), 2 a type/form the
//!   verifier does not rebuild, 3 provider answers refused every time; four:
//!   the Four.meme decoder's pre-execution verdict, 0 Flap, 1 not a decoded
//!   launch, 2 no proven derivation, 3 derived = canonical, 4 derived ≠
//!   canonical; absent in the 136-byte records of older builds).
//! - `Canonical` `b'V'`: a launch call included and reverted (no token),
//!   `b'V' | venue u8 | block u64 | block_hash [32] | tx_hash [32]`; an `O`
//!   of the same block and transaction takes it out again.
//! - `Canonical` `b'O'`: a launch of an orphaned block taken out again,
//!   `b'O' | venue u8 | block u64 | block_hash [32] | tx_hash [32]`.
//! - `Reorg` `b'R'`: a reorg, `b'R' | depth u32 | new head u64 | hash [32]`.
//! - `Reorg` `b'G'`: a resync (gap or reorg deeper than the window),
//!   `b'G' | from u64 | to u64`.
//! - `Canonical` `b'H'` / `b'E'` / `b'K'`: the first / last canonical block
//!   of this process and periodic checkpoints of the head,
//!   `b'H'|b'E'|b'K' | number u64 | hash [32]`: the covered window, also
//!   after a crash (last `K`).
//! - `Runtime` `b'D'`: pending events of a source the bounded ingress queue
//!   dropped before this journal, `b'D' | source u8 | total u64` (the
//!   source's cumulative count, written when it grows and at the end).
//! - `Runtime` `b'X'`: the clean end of this process (journal committed
//!   after it); its absence marks a killed or crashed process.
//!
//! With `LAUNCHES` (the default) other pending transactions are only
//! counted (runtime status), not journaled: the journal stays small over
//! long read-only runs; a scoped Phase-F capture chooses `VENUES` or `ALL`
//! (the byte budget and the preflight bound it).

use crate::canonical_join::{FourToken, Joined, LaunchRef, LaunchTx, Venue};
use crate::sources::SourceId;
use darkbot_bsc_journal::storage::metrics::{MetricsView, StorageMetrics};
use darkbot_bsc_journal::storage::segments::{SealedSegment, SegmentPolicy, SegmentWriter};
use darkbot_bsc_journal::storage::writer::{self, SyncPolicy, WorkerEnd};
use darkbot_bsc_journal::{JournalError, MemoryBudget, RECORD_OVERHEAD, RecordKind, Recorder};
use darkbot_bsc_record::schema::bsc_capture::capture_run as cr;
use darkbot_bsc_record::{Draft, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;

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

/// One PAPER BUY taken by the PAPER sink (C3): `Q`, the sink sequence,
/// the launch and BUY hashes, the token, the stage times (ingress, decoded,
/// admitted, signed, sink) and the signed bytes (a local artefact, never
/// sent).
pub fn paper_handoff_payload(buy: &crate::paper_exec::PaperBuy) -> Vec<u8> {
    let t = buy.times;
    let mut out = Vec::with_capacity(1 + 8 + 32 + 32 + 20 + 5 * 8 + buy.raw.len());
    out.push(b'Q');
    out.extend_from_slice(&buy.handoff.sequence.to_le_bytes());
    out.extend_from_slice(&buy.launch_hash);
    out.extend_from_slice(&buy.handoff.tx_hash);
    out.extend_from_slice(&buy.token);
    for ns in [
        t.ingress_ns,
        t.decoded_ns,
        t.admitted_ns,
        t.signed_ns,
        t.sink_ns,
    ] {
        out.extend_from_slice(&ns.to_le_bytes());
    }
    out.extend_from_slice(&buy.raw);
    out
}

/// The cumulative count of a source's events the ingress queue dropped.
pub fn ingress_drop_payload(source: SourceId, total: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(10);
    out.push(b'D');
    out.push(source_byte(source));
    out.extend_from_slice(&total.to_le_bytes());
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
        Venue::OpenFour => 3,
    }
}

/// The venue of a launch record's venue byte (the inverse of the writer's).
pub fn venue_of_byte(byte: u8) -> Option<Venue> {
    [Venue::Flap, Venue::Four, Venue::OpenFour]
        .into_iter()
        .find(|v| venue_byte(*v) == byte)
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
    out.push(match l.tx {
        LaunchTx::Verified => u8::from(l.c1_scope),
        LaunchTx::Unrebuildable => 2,
        LaunchTx::AnswerRefused => 3,
    });
    let lag = l.fetch_lag_ms();
    out.push(u8::from(lag.is_some()));
    out.extend_from_slice(&lag.unwrap_or(0).to_le_bytes());
    out.push(four_token_byte(l.four_token));
    out
}

/// The length of a `C` record…
pub const CANONICAL_LAUNCH_LEN: usize = 137;
/// …and of one of an older build (without the Four verdict).
pub const CANONICAL_LAUNCH_LEN_V1: usize = 136;

pub fn four_token_byte(t: FourToken) -> u8 {
    match t {
        FourToken::NotApplicable => 0,
        FourToken::NotRecognized => 1,
        FourToken::Unknown => 2,
        FourToken::Matched => 3,
        FourToken::Mismatch => 4,
    }
}

pub fn four_token_of_byte(byte: u8) -> Option<FourToken> {
    [
        FourToken::NotApplicable,
        FourToken::NotRecognized,
        FourToken::Unknown,
        FourToken::Matched,
        FourToken::Mismatch,
    ]
    .into_iter()
    .find(|t| four_token_byte(*t) == byte)
}

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
    launch_id_payload(b'O', l.venue, l.block_number, &l.block_hash, &l.tx_hash)
}

/// A launch call included and reverted (`V`), or one of an orphaned block
/// taken out again (`O`): `tag | venue u8 | block u64 | block_hash [32] |
/// tx_hash [32]`.
pub fn launch_id_payload(
    tag: u8,
    venue: Venue,
    block: u64,
    block_hash: &[u8; 32],
    tx_hash: &[u8; 32],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(74);
    out.push(tag);
    out.push(venue_byte(venue));
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(block_hash);
    out.extend_from_slice(tx_hash);
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

/// Which pending transactions besides launches the journal keeps
/// (`BSC_RUNTIME_CAPTURE_PENDING`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureScope {
    Launches,
    /// Also every one addressed to a venue contract (`ingress::venue_target`).
    Venues,
    /// Also every other one.
    All,
}

impl CaptureScope {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "LAUNCHES" => Ok(Self::Launches),
            "VENUES" => Ok(Self::Venues),
            "ALL" => Ok(Self::All),
            other => Err(format!(
                "BSC_RUNTIME_CAPTURE_PENDING={other} is not LAUNCHES, VENUES or ALL"
            )),
        }
    }

    /// Whether the first full sighting of a pending transaction that is no
    /// launch is journaled.
    pub fn keeps(
        self,
        four: &darkbot_bsc_trading::venues::fourmeme::deployment::FourDeployment,
        raw: &[u8],
    ) -> bool {
        match self {
            Self::Launches => false,
            Self::Venues => crate::ingress::venue_target(four, raw),
            Self::All => true,
        }
    }
}

/// The run manifest's file name inside a run directory.
pub const RUN_MANIFEST: &str = "capture-run.dbrec";

/// The bounds of the runtime's segmented capture (`BSC_RUNTIME_JOURNAL_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JournalSetup {
    /// Records the recorder queues at most…
    pub queue: usize,
    /// …and the bytes they may hold together (payload plus overhead).
    pub budget_bytes: usize,
    pub max_payload: usize,
    pub segment: SegmentPolicy,
    pub sync: SyncPolicy,
}

impl JournalSetup {
    /// One unrotated segment, a budget that holds a full queue of the
    /// largest records, synced at the end only: the bounds of a bounded
    /// harness or test capture (the runtime takes its keys,
    /// `runtime::journal_setup`).
    pub fn whole(queue: usize, max_payload: usize) -> Self {
        JournalSetup {
            queue,
            budget_bytes: max_payload
                .saturating_add(RECORD_OVERHEAD)
                .saturating_mul(queue.max(1)),
            max_payload,
            segment: SegmentPolicy {
                max_segment_bytes: u64::MAX,
                max_segment_ns: u64::MAX,
            },
            sync: SyncPolicy {
                every_records: u64::MAX,
                every: std::time::Duration::MAX,
            },
        }
    }

    /// The largest record must fit the budget, or every such record would
    /// be lost.
    pub fn validate(&self) -> Result<(), String> {
        let largest = self.max_payload.saturating_add(RECORD_OVERHEAD);
        if self.budget_bytes < largest {
            return Err(format!(
                "BSC_RUNTIME_JOURNAL_BUDGET_BYTES {} is below the largest record ({largest} bytes)",
                self.budget_bytes
            ));
        }
        Ok(())
    }
}

/// What a finished capture wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureRun {
    pub dir: PathBuf,
    pub segments: usize,
    /// Frames written; records lost: the gap spans among them plus the
    /// oversize records refused (which take no sequence).
    pub records: u64,
    pub lost_records: u64,
    pub complete: bool,
}

/// What wrote a run (Phase F contract §4): the host its latencies are of,
/// the build and the SHA-256 of the runtime configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunIdentity {
    pub host: String,
    /// Crate version and the checkout's commit (`checkout <sha>[+dirty]`).
    pub build: String,
    pub config_sha256: [u8; 32],
    /// SHA-256 of the running binary itself, if it could be read.
    pub build_sha256: Option<[u8; 32]>,
    /// Whether the checkout had uncommitted engine changes, if known.
    pub build_dirty: Option<bool>,
    /// The pending capture scope.
    pub capture_pending: &'static str,
}

impl CaptureScope {
    pub fn name(self) -> &'static str {
        match self {
            Self::Launches => "LAUNCHES",
            Self::Venues => "VENUES",
            Self::All => "ALL",
        }
    }
}

/// SHA-256 of the running binary (`std::env::current_exe`).
pub fn own_binary_sha256() -> Option<[u8; 32]> {
    let exe = std::env::current_exe().ok()?;
    Some(darkbot_bsc_record::sha256(&fs::read(exe).ok()?))
}

/// The capture's storage health of this moment (runtime status).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureStatus {
    pub writer: MetricsView,
    pub budget_max: u64,
    pub budget_in_use: u64,
    pub budget_peak: u64,
    pub budget_refused: u64,
}

pub struct RuntimeJournal {
    dir: PathBuf,
    started_ms: u64,
    clock_id: u64,
    setup: JournalSetup,
    recorder: Option<Recorder>,
    worker: Option<JoinHandle<Result<WorkerEnd, JournalError>>>,
    sealed: mpsc::Receiver<SealedSegment>,
    metrics: Arc<StorageMetrics>,
    budget: Arc<MemoryBudget>,
    lost: u64,
    identity: Option<RunIdentity>,
    /// Events the ingress queue dropped before this journal (all sources).
    ingress_dropped: u64,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

impl RuntimeJournal {
    /// Creates the run directory `<dir>/<started_ms>/` (parents 0700; the
    /// run itself create-new) and its writer. `clock_id` names the clock
    /// of every record timestamp.
    pub fn open(
        dir: &Path,
        started_ms: u64,
        clock_id: u64,
        setup: JournalSetup,
    ) -> Result<Self, String> {
        setup.validate()?;
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        let run = dir.join(started_ms.to_string());
        let segments = SegmentWriter::create(&run, setup.segment)
            .map_err(|e| format!("create-new {}: {e}", run.display()))?;
        let budget =
            MemoryBudget::new(setup.budget_bytes).map_err(|e| format!("journal budget: {e}"))?;
        let (recorder, drain) =
            Recorder::bounded_spanned_budgeted(setup.queue, setup.max_payload, Arc::clone(&budget))
                .map_err(|e| format!("journal recorder: {e}"))?;
        let metrics = Arc::new(StorageMetrics::default());
        let (tx, sealed) = mpsc::channel();
        let worker = writer::spawn(drain, segments, setup.sync, Arc::clone(&metrics), Some(tx))
            .map_err(|e| format!("spawn journal writer: {e}"))?;
        Ok(RuntimeJournal {
            dir: run,
            started_ms,
            clock_id,
            setup,
            recorder: Some(recorder),
            worker: Some(worker),
            sealed,
            metrics,
            budget,
            lost: 0,
            identity: None,
            ingress_dropped: 0,
        })
    }

    /// Names the host, build and configuration in the run manifest.
    pub fn set_identity(&mut self, identity: RunIdentity) {
        self.identity = Some(identity);
    }

    /// The events the ingress queue dropped before this journal so far (all
    /// sources): the manifest's loss count besides the gap spans.
    pub fn set_ingress_dropped(&mut self, total: u64) {
        self.ingress_dropped = total;
    }

    /// The run directory.
    pub fn path(&self) -> &Path {
        &self.dir
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
        self.metrics.view().records
    }

    pub fn lost(&self) -> u64 {
        self.lost
    }

    pub fn healthy(&self) -> bool {
        !self.writer_view().failed
    }

    /// The writer's metrics; a worker that ended while the recorder is
    /// open failed (it panicked or returned an error: it only ends when the
    /// recorder is closed), whatever the metrics say (Phase F gate L1).
    fn writer_view(&self) -> MetricsView {
        let mut view = self.metrics.view();
        if self.recorder.is_some() && self.worker.as_ref().is_some_and(|w| w.is_finished()) {
            view.failed = true;
        }
        view
    }

    pub fn capture(&self) -> CaptureStatus {
        CaptureStatus {
            writer: self.writer_view(),
            budget_max: self.budget.max() as u64,
            budget_in_use: self.budget.in_use() as u64,
            budget_peak: self.budget.peak() as u64,
            budget_refused: self.budget.refused(),
        }
    }

    /// Closes the queue, lets the worker append what is queued and seal
    /// the last segment, then writes the run manifest (also after a write
    /// failure, as `INCOMPLETE`). A write failure is returned after that.
    pub fn finish(mut self) -> Result<CaptureRun, String> {
        // Oversize records take no sequence; every sequence the recorder
        // handed out lies in a sealed segment (a record or a gap span) or
        // never reached one (a failed writer's open segment, its dropped
        // queue, refusals after it ended).
        let (oversize, next_sequence) = self.recorder.as_ref().map_or((0, 0), |r| {
            let s = r.stats();
            (s.rejected_oversize, s.next_sequence)
        });
        drop(self.recorder.take());
        // A failed or panicked writer still gets its manifest (INCOMPLETE).
        let end: Result<(), String> = match self
            .worker
            .take()
            .ok_or("journal writer already joined")?
            .join()
        {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => Err("journal writer panicked".to_string()),
        };
        let open_segments: Vec<String> = fs::read_dir(&self.dir)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.ends_with(".dbjnl.open"))
                    .collect()
            })
            .unwrap_or_default();
        let sealed: Vec<SealedSegment> = self.sealed.try_iter().collect();
        let view = self.metrics.view();
        let records: u64 = sealed.iter().map(|s| s.manifest.records).sum();
        let lost_records: u64 = sealed.iter().map(|s| s.manifest.lost_records).sum();
        let sealed_next = sealed
            .iter()
            .filter_map(|s| s.manifest.last_sequence)
            .max()
            .map_or(0, |l| l.saturating_add(1));
        let unsealed = next_sequence.saturating_sub(sealed_next);
        let mut reasons = Vec::new();
        if let Err(e) = &end {
            reasons.push(format!("journal write failed: {e}"));
        }
        if lost_records > 0 {
            reasons.push(format!("{lost_records} records lost (gap spans)"));
        }
        if oversize > 0 {
            reasons.push(format!("{oversize} oversize records refused"));
        }
        if unsealed > 0 {
            reasons.push(format!("{unsealed} records never reached a sealed segment"));
        }
        for open in &open_segments {
            reasons.push(format!("segment {open} left open (not sealed)"));
        }
        if self.ingress_dropped > 0 {
            reasons.push(format!(
                "{} source events dropped by the ingress queue before the journal",
                self.ingress_dropped
            ));
        }
        let complete = reasons.is_empty();
        let mut d = Draft::new(&cr::V3);
        d.set(cr::RUN_ID, self.started_ms.to_string())
            .set(cr::CHAIN_ID, 56u64)
            .set(cr::PROVENANCE_TOOL, "bsc_runtime")
            .set(cr::PROVENANCE_OBSERVED_UNIX_MS, now_ms())
            .set(cr::STARTED_UNIX_MS, self.started_ms)
            .set(cr::ENDED_UNIX_MS, now_ms())
            .set(cr::CLOCK_ID, self.clock_id)
            .set(cr::BUDGET_BYTES, self.setup.budget_bytes as u64)
            .set(cr::QUEUE_RECORDS, self.setup.queue as u64)
            .set(cr::MAX_PAYLOAD, self.setup.max_payload as u64)
            .set(cr::SEGMENT_BYTES, self.setup.segment.max_segment_bytes)
            .set(cr::SEGMENT_NS, self.setup.segment.max_segment_ns)
            .set(cr::SYNC_RECORDS, self.setup.sync.every_records)
            .set(
                cr::SYNC_MS,
                u64::try_from(self.setup.sync.every.as_millis()).unwrap_or(u64::MAX),
            )
            .set(cr::RECORDS, records)
            .set(
                cr::LOST_RECORDS,
                lost_records
                    .saturating_add(oversize)
                    .saturating_add(unsealed),
            )
            .set(cr::PEAK_BYTES, self.budget.peak() as u64)
            .set(cr::BUDGET_REFUSED, self.budget.refused())
            .set(cr::WRITE_ERRORS, view.write_errors)
            .set(cr::INGRESS_DROPPED, self.ingress_dropped)
            .set(cr::STATE, if complete { "COMPLETE" } else { "INCOMPLETE" })
            .push(
                cr::NON_CLAIMS,
                "LOCAL_CAPTURE: a runtime journal of this host; no Phase-F quality verdict",
            );
        if let Some(id) = &self.identity {
            d.set(cr::HOST, id.host.as_str())
                .set(cr::BUILD, id.build.as_str())
                .set(cr::CONFIG_SHA256, Value::B256(id.config_sha256))
                .set(cr::CAPTURE_PENDING, id.capture_pending);
            if let Some(h) = id.build_sha256 {
                d.set(cr::BUILD_SHA256, Value::B256(h));
            }
            if let Some(dirty) = id.build_dirty {
                d.set(cr::BUILD_DIRTY, dirty);
            }
        }
        if let Some(last) = sealed.iter().rev().find_map(|s| s.manifest.last_sequence) {
            d.set(cr::LAST_SEQUENCE, last);
        }
        for reason in &reasons {
            d.push(cr::REASONS, reason.as_str());
        }
        for s in &sealed {
            let mut seg = Draft::new(&cr::segment::SCHEMA);
            let file = s
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            seg.set(cr::segment::INDEX, u64::from(s.index))
                .set(cr::segment::FILE, file)
                .set(cr::segment::RECORDS, s.manifest.records)
                .set(cr::segment::LOST_RECORDS, s.manifest.lost_records)
                .set(cr::segment::BYTES, s.manifest.bytes)
                .set(cr::segment::SHA256, Value::B256(s.manifest.sha256));
            if let Some(v) = s.manifest.first_sequence {
                seg.set(cr::segment::FIRST_SEQUENCE, v);
            }
            if let Some(v) = s.manifest.last_sequence {
                seg.set(cr::segment::LAST_SEQUENCE, v);
            }
            if let Some(v) = s.first_timestamp_ns {
                seg.set(cr::segment::FIRST_TIMESTAMP_NS, v);
            }
            if let Some(v) = s.last_timestamp_ns {
                seg.set(cr::segment::LAST_TIMESTAMP_NS, v);
            }
            d.push(cr::SEGMENTS, seg.into_nested());
        }
        for open in &open_segments {
            d.push(cr::OPEN_SEGMENTS, open.as_str());
        }
        let bytes = d.encode().map_err(|e| format!("run manifest: {e}"))?;
        write_durable(&self.dir.join(RUN_MANIFEST), &bytes)?;
        end.map_err(|e| format!("journal write failed: {e}"))?;
        Ok(CaptureRun {
            dir: self.dir.clone(),
            segments: sealed.len(),
            records,
            lost_records: lost_records
                .saturating_add(oversize)
                .saturating_add(unsealed),
            complete,
        })
    }
}

/// Create-new 0600, written, synced, then the directory synced.
fn write_durable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("create-new {}: {e}", path.display()))?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(parent) = path.parent() {
        fs::File::open(parent)
            .and_then(|d| d.sync_all())
            .map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    /// The pending capture scope: nothing besides launches, the venue
    /// flow, or everything; anything else is refused at parse.
    #[test]
    fn the_capture_scope_keeps_what_it_names() {
        let four = darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_g01_129842711();
        let calls = include_str!("../../trading/fixtures/fourmeme/freeze_20261026_calls.txt");
        let row: Vec<&str> = calls
            .lines()
            .find(|l| l.starts_with("CLASSIC BUY"))
            .unwrap()
            .split_whitespace()
            .collect();
        let venue = alloy_primitives::hex::decode(row[10]).unwrap();
        let other = b"not a venue transaction";
        let keeps = |s: &str, raw: &[u8]| CaptureScope::parse(s).unwrap().keeps(&four, raw);
        assert!(!keeps("LAUNCHES", &venue));
        assert!(keeps("VENUES", &venue) && !keeps("VENUES", other));
        assert!(keeps("ALL", other));
        assert!(CaptureScope::parse("SOME").is_err());
    }

    /// Every Four verdict's byte reads back as that verdict; no other does.
    #[test]
    fn every_four_verdict_byte_reads_back() {
        for t in [
            FourToken::NotApplicable,
            FourToken::NotRecognized,
            FourToken::Unknown,
            FourToken::Matched,
            FourToken::Mismatch,
        ] {
            assert_eq!(four_token_of_byte(four_token_byte(t)), Some(t));
        }
        assert_eq!(four_token_of_byte(5), None);
    }

    /// Every venue's byte reads back as that venue; no other byte does.
    #[test]
    fn every_venue_byte_reads_back() {
        for v in [Venue::Flap, Venue::Four, Venue::OpenFour] {
            assert_eq!(venue_of_byte(venue_byte(v)), Some(v));
        }
        assert_eq!(venue_of_byte(0), None);
        assert_eq!(venue_of_byte(4), None);
    }

    use super::*;
    use darkbot_bsc_journal::storage::run::RunReader;

    #[test]
    fn sightings_and_transitions_replay_in_order_with_their_payloads() {
        let dir = std::env::temp_dir().join(format!("bsc-runtime-journal-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut j = RuntimeJournal::open(&dir, 7, 1, JournalSetup::whole(64, 4096)).unwrap();
        j.set_identity(RunIdentity {
            host: "test-host".into(),
            build: "test-build".into(),
            config_sha256: [3; 32],
            build_sha256: Some([4; 32]),
            build_dirty: Some(false),
            capture_pending: "VENUES",
        });
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
        assert_eq!(path, dir.join("7"));
        let run = j.finish().unwrap();
        assert_eq!((run.segments, run.records, run.complete), (1, 3, true));
        let mut reader = RunReader::open(&path, 4096).unwrap();
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
        // The run manifest names the sealed segment by its exact hash.
        let m = cr::decode_run(&fs::read(path.join(RUN_MANIFEST)).unwrap()).unwrap();
        assert_eq!(m.text(cr::STATE).unwrap(), "COMPLETE");
        assert_eq!(
            (
                m.text(cr::HOST).unwrap(),
                m.b256(cr::CONFIG_SHA256).unwrap()
            ),
            ("test-host", [3; 32])
        );
        let segments = m.nested_all(cr::SEGMENTS).unwrap();
        assert_eq!(
            segments[0].b256(cr::segment::SHA256).unwrap(),
            reader.segments()[0].sha256
        );
        assert_eq!(m.u64(cr::RECORDS).unwrap(), 3);
        // create-new: the same start time never overwrites a journal.
        assert!(RuntimeJournal::open(&dir, 7, 1, JournalSetup::whole(64, 4096)).is_err());
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
            four_token: crate::canonical_join::FourToken::NotApplicable,
            tx: LaunchTx::Verified,
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
            &c[126..136],
            &[&[1u8, 1][..], &(-10i64).to_le_bytes()[..]].concat()[..]
        );
        // A Flap launch has no Four verdict.
        assert_eq!(c[136], 0);
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

    /// Phase F gate L1: a writer that fails (here: it cannot seal a
    /// segment in a read-only run directory) turns the capture FAILED at
    /// once, records refused after it count as lost, and the manifest says
    /// INCOMPLETE with the failure and the segment left open.
    #[test]
    fn a_failed_writer_is_reported_and_its_manifest_still_written() {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("bsc-runtime-journal-f-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let setup = JournalSetup {
            segment: SegmentPolicy {
                max_segment_bytes: 100,
                max_segment_ns: u64::MAX,
            },
            ..JournalSetup::whole(64, 256)
        };
        let mut j = RuntimeJournal::open(&dir, 1, 1, setup).unwrap();
        let run = j.path().to_path_buf();
        assert!(j.record(RecordKind::Runtime, 1, &[1u8; 80]));
        let started = std::time::Instant::now();
        while j.appended() < 1 {
            assert!(started.elapsed() < std::time::Duration::from_secs(5));
            std::thread::yield_now();
        }
        fs::set_permissions(&run, fs::Permissions::from_mode(0o500)).unwrap();
        // The next record makes the writer rotate: it cannot seal.
        assert!(j.record(RecordKind::Runtime, 2, &[2u8; 80]));
        while j.healthy() {
            assert!(started.elapsed() < std::time::Duration::from_secs(5));
            std::thread::yield_now();
        }
        assert!(j.capture().writer.failed);
        // Queued into a dead writer or refused: never sealed either way.
        j.record(RecordKind::Runtime, 3, &[3u8; 8]);
        fs::set_permissions(&run, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(j.finish().is_err());
        let m = cr::decode_run(&fs::read(run.join(RUN_MANIFEST)).unwrap()).unwrap();
        assert_eq!(m.text(cr::STATE).unwrap(), "INCOMPLETE");
        let reasons = m.texts(cr::REASONS).unwrap();
        assert!(
            reasons
                .iter()
                .any(|r| r.starts_with("journal write failed")),
            "{reasons:?}"
        );
        // Records 1 (in the open segment), 2 and 3.
        assert!(
            reasons
                .iter()
                .any(|r| *r == "3 records never reached a sealed segment"),
            "{reasons:?}"
        );
        assert_eq!(
            m.texts(cr::OPEN_SEGMENTS).unwrap(),
            ["segment_000000.dbjnl.open"]
        );
        assert_eq!(m.u64(cr::LOST_RECORDS).unwrap(), 3);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Phase F gate H1: events the ingress queue dropped before the journal
    /// make the run INCOMPLETE and are named in its manifest.
    #[test]
    fn ingress_drops_make_the_run_incomplete() {
        let dir =
            std::env::temp_dir().join(format!("bsc-runtime-journal-d-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut j = RuntimeJournal::open(&dir, 1, 1, JournalSetup::whole(8, 64)).unwrap();
        assert!(j.record(
            RecordKind::Runtime,
            1,
            &ingress_drop_payload(SourceId::Drpc, 3)
        ));
        j.set_ingress_dropped(3);
        let path = j.path().to_path_buf();
        assert!(!j.finish().unwrap().complete);
        let m = cr::decode_run(&fs::read(path.join(RUN_MANIFEST)).unwrap()).unwrap();
        assert_eq!(
            (
                m.text(cr::STATE).unwrap(),
                m.u64(cr::INGRESS_DROPPED).unwrap()
            ),
            ("INCOMPLETE", 3)
        );
        assert_eq!(
            m.texts(cr::REASONS).unwrap(),
            ["3 source events dropped by the ingress queue before the journal"]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A budget below the largest record would lose every such record:
    /// refused before anything is created.
    #[test]
    fn a_budget_below_the_largest_record_is_refused() {
        let dir =
            std::env::temp_dir().join(format!("bsc-runtime-journal-b-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let setup = JournalSetup {
            budget_bytes: 64 + RECORD_OVERHEAD - 1,
            ..JournalSetup::whole(4, 64)
        };
        assert!(RuntimeJournal::open(&dir, 1, 1, setup).is_err());
        assert!(!dir.exists());
    }

    #[test]
    fn a_full_queue_loses_visibly_never_silently() {
        let dir =
            std::env::temp_dir().join(format!("bsc-runtime-journal-full-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut j = RuntimeJournal::open(&dir, 1, 1, JournalSetup::whole(1, 64)).unwrap();
        // Oversize payloads are refused by the bounded recorder: counted.
        assert!(!j.record(RecordKind::Runtime, 1, &[0u8; 65]));
        assert_eq!(j.lost(), 1);
        let path = j.path().to_path_buf();
        let run = j.finish().unwrap();
        assert!(!run.complete);
        // The run manifest says why the capture is not complete.
        let m = cr::decode_run(&fs::read(path.join(RUN_MANIFEST)).unwrap()).unwrap();
        assert_eq!(m.text(cr::STATE).unwrap(), "INCOMPLETE");
        assert_eq!(m.u64(cr::LOST_RECORDS).unwrap(), 1);
        assert_eq!(
            m.texts(cr::REASONS).unwrap(),
            ["1 oversize records refused"]
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}

//! Post-capture strict verification. Reuses the canonical slot/parent/skip engine.
//! Raw capture files are never opened for writing; only this specification's directory.
//!
//! Evidence is native (Ticket 3 A3): the resolved scope, every cached chain read, the batch
//! market journals, the batch checkpoints and the result are typed `.p0j` journals. JSON exists
//! only in `fetch`, the JSON-RPC wire edge (JFI-02): the answer is decoded there at once and
//! cached typed (`ChainCacheEntry`, F10); no provider JSON is persisted or handed on.
//! The liquidation truth comes from the finalized blocks themselves (`reconcile_slots` →
//! `truth::classify`), never from the market observer's rows.
use super::reconcile::{ChainAnswer, ChainRead, RequestPacer};
use super::rows::{MarketCoverage, MarketLiquidation, ReconciliationRow};
use super::*;
use crate::analytics::journal::{self, kind, source};
use crate::analytics::payload::Payload;
use crate::storage::budget::{current_rss_bytes, free_disk_bytes, MIB};
use crate::storage::writer::WriterConfig;
use darkbot_solana::chain::WIRE_DECODER_VERSION;
use serde::{Deserialize, Serialize};
use solana_sdk::hash::hash;

/// The verification job (from the verification driver). Budgets are hard limits.
#[derive(Clone, Deserialize)]
pub struct Job {
    pub verification_spec_id: String,
    pub output: PathBuf,
    pub start_ns: u64,
    pub end_ns: u64,
    pub program: String,
    pub max_calls: u64,
    pub max_seconds: u64,
    pub max_bytes: u64,
    pub max_disk_bytes: u64,
    pub max_ram_bytes: u64,
    pub min_free_bytes: u64,
    pub rps: u64,
    #[serde(default)]
    pub offline: bool,
    /// The job controller's cancellation (`job cancel`, DEC-08): checked with every budget, the
    /// attempt ends HISTORICAL_INCOMPLETE `VERIFICATION_CANCELLED`, resumable from its cache.
    #[serde(skip)]
    pub cancelled: Option<fn() -> bool>,
}

impl Job {
    /// An empty window or a zero/out-of-range budget (every verifier refuses it before a read).
    pub(super) fn invalid(&self) -> bool {
        self.start_ns >= self.end_ns
            || self.max_calls == 0
            || self.max_seconds == 0
            || self.max_seconds > u64::from(u32::MAX)
            || self.rps == 0
            || self.rps > u64::from(u32::MAX)
            || self.max_ram_bytes == 0
            || self.max_bytes == 0
    }
}

/// Slots per batch (one checkpoint each).
/// Slots per strict batch (also sizes the verifier budget in the capacity preflight).
pub const BATCH_SLOTS: u64 = 32;
/// Bounded read-ahead: produced slots fetched together, never more responses retained.
const PREFETCH: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrictScope {
    pub verification_spec_id: String,
    pub start_ns: u64,
    pub end_ns: u64,
    pub lo: u64,
    pub hi: u64,
}
impl Payload for StrictScope {
    const KIND: u16 = kind::STRICT_SCOPE;
}

/// One cached chain read (F10, JFI-01/02): the typed answer decoded at the JSON-RPC wire edge,
/// bound to its read and the wire decoder version. The provider's JSON is never persisted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainCacheEntry {
    pub read: ChainRead,
    /// `darkbot_solana::chain::WIRE_DECODER_VERSION` of the decode; another version is refused.
    pub decoder: u16,
    pub answer: ChainAnswer,
}
impl Payload for ChainCacheEntry {
    const KIND: u16 = kind::STRICT_CHAIN_CACHE;
}

/// HISTORICAL (before F10, 25.10.): a cached JSON-RPC exchange as wire text. Never written; kept
/// only so the caches of earlier strict runs stay verifiable read-only (`p0-journal`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcCacheEntry {
    pub method: String,
    pub params: String,
    pub result: Vec<u8>,
    pub result_sha256: [u8; 32],
}
impl Payload for RpcCacheEntry {
    const KIND: u16 = kind::STRICT_RPC_CACHE;
}

/// The sealed market journal of one batch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchJournal {
    pub directory: String,
    pub file_sha256: [u8; 32],
    pub frames: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrictCheckpoint {
    pub spec: String,
    pub start: u64,
    pub end: u64,
    pub previous_anchor: Option<(u64, String)>,
    pub anchor: Option<(u64, String)>,
    pub journal: BatchJournal,
    pub proof: ReconciliationRow,
}
impl Payload for StrictCheckpoint {
    const KIND: u16 = kind::STRICT_CHECKPOINT;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrictStatus {
    CanonicalVerified,
    HistoricalIncomplete,
}
impl StrictStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            StrictStatus::CanonicalVerified => "CANONICAL_VERIFIED",
            StrictStatus::HistoricalIncomplete => "HISTORICAL_INCOMPLETE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchRef {
    pub checkpoint: String,
    pub sha256: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrictResult {
    pub verification_spec_id: String,
    pub status: StrictStatus,
    pub reason: Option<String>,
    pub scope: Option<StrictScope>,
    pub batches: Vec<BatchRef>,
    pub rpc_calls: u64,
    pub successful_response_bytes: u64,
    /// Always false: a strict verification alone is never the C0-E1 master gate.
    pub c0_e1_master_pass: bool,
}
impl Payload for StrictResult {
    const KIND: u16 = kind::STRICT_RESULT;
}

/// The typed cache file of one read (F10): named by the read itself, never by provider JSON.
fn cache_path(output: &Path, read: &ChainRead) -> PathBuf {
    let name = match *read {
        ChainRead::FinalizedSlot => "slot_finalized".to_string(),
        ChainRead::FirstAvailableBlock => "first_available_block".to_string(),
        ChainRead::Blocks { start, end } => format!("blocks_{start}_{end}"),
        ChainRead::BlockHeader { slot } => format!("header_{slot}"),
        ChainRead::FullBlock { slot } => format!("block_{slot}"),
        // DEC-08: the INDEX/CROSSCHECK reads (a page by its cursor, a transaction by its id).
        ChainRead::Signatures { address, before } => format!(
            "signatures_{address}_{}",
            before.map_or("head".to_string(), |b| b.to_string())
        ),
        ChainRead::Transaction { signature } => format!("transaction_{signature}"),
    };
    output.join("cache").join(format!("{name}.p0j"))
}
/// The single JSON-RPC exchange of a read (JFI-02 wire edge): typed request to wire params, wire
/// answer decoded at once; P2 admission, Retry-After/circuit and the bounded body reader of
/// `gap_request_metered` stay authoritative. The provider's JSON never leaves this function.
fn fetch(
    rpc: &RpcFetcher,
    read: &ChainRead,
    deadline: Instant,
    max_bytes: u64,
) -> Result<(ChainAnswer, u64), &'static str> {
    let (method, params) = reconcile::wire_params(read);
    let (wire, bytes) = rpc.gap_request_metered(method, params, deadline, 10_000, max_bytes)?;
    Ok((reconcile::wire_answer(read, &wire), bytes))
}
fn io_code(_: std::io::Error) -> &'static str {
    "EVIDENCE_IO"
}
fn disk_bytes(dir: &Path) -> Result<u64, &'static str> {
    let mut total = 0;
    for e in fs::read_dir(dir).map_err(|_| "RESOURCE_IO")? {
        let e = e.map_err(|_| "RESOURCE_IO")?;
        let m = e.metadata().map_err(|_| "RESOURCE_IO")?;
        total += if m.is_dir() {
            disk_bytes(&e.path())?
        } else {
            m.len()
        };
    }
    Ok(total)
}
fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Slots of the next read-ahead group for `slot`: produced slots from `slot` on, at most
/// `limit`, without responses already held or cached. A retry of the same slot fetches it alone
/// (no read-ahead expansion while the provider rejects requests).
fn read_ahead_group(
    slot: u64,
    produced: &[u64],
    held: &BTreeMap<u64, Result<ChainAnswer, &'static str>>,
    cached: &dyn Fn(u64) -> bool,
    limit: usize,
) -> Vec<u64> {
    match produced.iter().position(|s| *s == slot) {
        Some(i) => std::iter::once(slot)
            .chain(
                produced[i + 1..]
                    .iter()
                    .copied()
                    .filter(|s| !held.contains_key(s) && !cached(*s)),
            )
            .take(limit)
            .collect(),
        None => vec![slot],
    }
}

/// The budgeted chain client of the post-capture verifiers (STRICT, and DEC-08 INDEX and
/// CROSSCHECK): one wire edge, the typed cache of every read, the call/byte/time/disk/RAM budgets.
pub(super) struct Client<'a> {
    job: &'a Job,
    rpc: Option<RpcFetcher>,
    deadline: Instant,
    pacer: RequestPacer,
    pub(super) calls: u64,
    pub(super) bytes: u64,
    disk: u64,
    produced: Vec<u64>,
    held: BTreeMap<u64, Result<ChainAnswer, &'static str>>,
    previous_block: Option<u64>,
    completed_batches: Vec<BatchRef>,
}
impl<'a> Client<'a> {
    /// The client of `job` against `url` (`None` offline: cache only).
    pub(super) fn new(job: &'a Job, url: Option<String>) -> Result<Self, &'static str> {
        Ok(Client {
            rpc: match url {
                None => None,
                Some(url) => Some(
                    RpcFetcher::new(
                        url,
                        darkbot_solana::runtime_config::active()
                            .limits
                            .rpc_timeout_ms,
                        "finalized",
                    )
                    .map_err(|_| "RPC_INIT")?,
                ),
            },
            deadline: Instant::now() + Duration::from_secs(job.max_seconds),
            pacer: RequestPacer::new(job.rps),
            calls: 0,
            bytes: 0,
            disk: disk_bytes(&job.output).map_err(|_| "RESOURCE_IO")?,
            produced: Vec::new(),
            held: BTreeMap::new(),
            previous_block: None,
            completed_batches: Vec::new(),
            job,
        })
    }
    /// The read is answered from the cache, without the network.
    pub(super) fn is_cached(&self, read: &ChainRead) -> bool {
        cache_path(&self.job.output, read).exists()
    }
    /// The client reads the network (`false`: the cache only).
    pub(super) fn online(&self) -> bool {
        self.rpc.is_some()
    }
    /// DEC-08: re-reads `read` from the network and replaces its cache entry — the one mutable
    /// read of an INDEX checkpoint (an address's newest page); a failed re-read keeps the old one.
    pub(super) fn refresh(&mut self, read: &ChainRead) -> Result<ChainAnswer, &'static str> {
        self.guard()?;
        let path = cache_path(&self.job.output, read);
        let kept = path.with_extension("p0j.replaced");
        if path.exists() {
            fs::rename(&path, &kept).map_err(io_code)?;
        }
        match self.network(read) {
            Ok(answer) => {
                let _ = fs::remove_file(&kept);
                Ok(answer)
            }
            Err(e) => {
                if kept.exists() && !path.exists() {
                    fs::rename(&kept, &path).map_err(io_code)?;
                }
                Err(e)
            }
        }
    }
    /// Time, disk, RAM and free-space budgets (no subprocess; disk usage tracked incrementally).
    pub(super) fn guard(&self) -> Result<(), &'static str> {
        if self.job.cancelled.is_some_and(|c| c()) {
            return Err("VERIFICATION_CANCELLED");
        }
        if Instant::now() >= self.deadline {
            return Err("VERIFICATION_TIME_BUDGET");
        }
        if self.disk >= self.job.max_disk_bytes {
            return Err("VERIFICATION_DISK_BUDGET");
        }
        let rss = current_rss_bytes().ok_or("RAM_PREFLIGHT_UNAVAILABLE")?;
        // Headroom for the capped response, its decode and the read-ahead group.
        if rss.saturating_add(512 * MIB) > self.job.max_ram_bytes {
            return Err("VERIFICATION_RAM_BUDGET");
        }
        let free = free_disk_bytes(&self.job.output).ok_or("DISK_PREFLIGHT_UNAVAILABLE")?;
        if free < self.job.min_free_bytes.saturating_add(64 * MIB) {
            return Err("VERIFICATION_DISK_FREE");
        }
        Ok(())
    }
    fn cached(&self, read: &ChainRead) -> Result<Option<ChainAnswer>, &'static str> {
        let path = cache_path(&self.job.output, read);
        if !path.exists() {
            return Ok(None);
        }
        let e: ChainCacheEntry = journal::read_typed_value(&path).map_err(|_| "CACHE_CORRUPT")?;
        if e.read != *read {
            return Err("CACHE_CORRUPT");
        }
        if e.decoder != WIRE_DECODER_VERSION {
            return Err("CACHE_DECODER_VERSION");
        }
        Ok(Some(e.answer))
    }
    fn store(
        &mut self,
        read: &ChainRead,
        answer: ChainAnswer,
    ) -> Result<ChainAnswer, &'static str> {
        let path = cache_path(&self.job.output, read);
        let entry = ChainCacheEntry {
            read: *read,
            decoder: WIRE_DECODER_VERSION,
            answer,
        };
        journal::write_typed_value(&path, source::STRICT, &entry).map_err(io_code)?;
        self.disk += file_len(&path);
        Ok(entry.answer)
    }
    fn network(&mut self, read: &ChainRead) -> Result<ChainAnswer, &'static str> {
        let remaining_bytes = self.job.max_bytes.saturating_sub(self.bytes);
        if remaining_bytes == 0 {
            return Err("VERIFICATION_NETWORK_BUDGET");
        }
        let rpc = self.rpc.as_ref().ok_or("OFFLINE_CACHE_MISS")?;
        // A transient refusal (429, timeout, transport) is retried within the attempt up to the
        // configured attempts; the shared provider waits out its cooldown before each one.
        let attempts = darkbot_solana::runtime_config::active()
            .limits
            .rpc_attempts
            .max(1);
        let mut tried = 0;
        let (answer, bytes) = loop {
            if self.calls >= self.job.max_calls {
                return Err("VERIFICATION_CALL_BUDGET");
            }
            self.pacer.wait(self.deadline)?;
            self.calls += 1;
            tried += 1;
            match fetch(rpc, read, self.deadline, remaining_bytes) {
                Err("RPC_RATE_LIMIT" | "RPC_TIMEOUT" | "RPC_TRANSPORT") if tried < attempts => {
                    self.guard()?;
                }
                other => break other?,
            }
        };
        self.bytes += bytes;
        let answer = self.store(read, answer)?;
        self.guard()?;
        Ok(answer)
    }
    /// One typed chain read (F10): JSON exists only inside `fetch`; the cache, the read-ahead and
    /// every caller see the typed answer.
    pub(super) fn read(&mut self, read: &ChainRead) -> Result<ChainAnswer, &'static str> {
        self.guard()?;
        if let ChainRead::FullBlock { slot } = *read {
            return self.block(slot);
        }
        let answer = match self.cached(read)? {
            Some(answer) => answer,
            None => self.network(read)?,
        };
        if let ChainAnswer::Blocks(list) = &answer {
            self.produced = list.clone().unwrap_or_default();
        }
        Ok(answer)
    }
    /// A full block with bounded read-ahead over the last produced-slot list. Only full-block
    /// reads are read ahead and held (a header probe never serves a full read). Validation and
    /// ledger order stay strictly canonical in `reconcile_slots`; every answer is cached.
    fn block(&mut self, slot: u64) -> Result<ChainAnswer, &'static str> {
        let read = ChainRead::FullBlock { slot };
        let retry = self.previous_block == Some(slot);
        self.previous_block = Some(slot);
        if retry {
            self.held.remove(&slot);
        } else if let Some(answer) = self.held.remove(&slot) {
            return answer;
        }
        if let Some(answer) = self.cached(&read)? {
            return Ok(answer);
        }
        let group = if retry {
            vec![slot]
        } else {
            let output = &self.job.output;
            let cached = |s: u64| cache_path(output, &ChainRead::FullBlock { slot: s }).exists();
            read_ahead_group(slot, &self.produced, &self.held, &cached, PREFETCH)
        };
        if group.len() <= 1 || self.rpc.is_none() {
            return self.network(&read);
        }
        self.prefetch(&group)?;
        self.held
            .remove(&slot)
            .unwrap_or(Err("PREFETCH_SLOT_MISSING"))
    }
    fn prefetch(&mut self, group: &[u64]) -> Result<(), &'static str> {
        let budget = self.job.max_calls.saturating_sub(self.calls) as usize;
        let group = &group[..group.len().min(budget)];
        if group.is_empty() {
            return Err("VERIFICATION_CALL_BUDGET");
        }
        let remaining = self.job.max_bytes.saturating_sub(self.bytes);
        if remaining == 0 {
            return Err("VERIFICATION_NETWORK_BUDGET");
        }
        let rpc = self.rpc.as_ref().ok_or("OFFLINE_CACHE_MISS")?;
        let (pacer, deadline) = (&self.pacer, self.deadline);
        self.calls += group.len() as u64;
        type Fetched = Vec<(u64, Result<(ChainAnswer, u64), &'static str>)>;
        let fetched: Fetched = std::thread::scope(|scope| {
            let jobs: Vec<_> = group
                .iter()
                .map(|&s| {
                    (
                        s,
                        scope.spawn(move || {
                            pacer.wait(deadline)?;
                            fetch(rpc, &ChainRead::FullBlock { slot: s }, deadline, remaining)
                        }),
                    )
                })
                .collect();
            jobs.into_iter()
                .map(|(s, j)| (s, j.join().unwrap_or(Err("BLOCK_FETCH_WORKER_FAILED"))))
                .collect()
        });
        // Accounted in slot order; answers beyond the byte budget are refused, not kept.
        for (s, r) in fetched {
            let r = match r {
                Ok((answer, bytes)) if self.bytes + bytes <= self.job.max_bytes => {
                    self.bytes += bytes;
                    self.store(&ChainRead::FullBlock { slot: s }, answer)
                }
                Ok(_) => Err("VERIFICATION_NETWORK_BUDGET"),
                Err(e) => Err(e),
            };
            self.held.insert(s, r);
        }
        self.guard()
    }
    fn header_at_or_after(&mut self, slot: u64, tip: u64) -> Result<(u64, i64), &'static str> {
        use reconcile::{ChainAnswer, ChainRead};
        let slots = self.read(&ChainRead::Blocks {
            start: slot,
            end: slot.saturating_add(512).min(tip),
        })?;
        let s = match slots {
            ChainAnswer::Blocks(Some(list)) => list.first().copied(),
            _ => None,
        }
        .filter(|s| *s >= slot && *s <= tip)
        .ok_or("UTC_BOUNDARY_SLOT_UNAVAILABLE")?;
        let header = match self.read(&ChainRead::BlockHeader { slot: s })? {
            ChainAnswer::Header(Ok(header)) => header,
            _ => return Err("UTC_BLOCK_HEADER_INVALID"),
        };
        Ok((s, header.block_time.ok_or("UTC_BLOCK_TIME_UNKNOWN")?))
    }
    fn resolve_window(&mut self) -> Result<(u64, u64), &'static str> {
        use reconcile::{ChainAnswer, ChainRead};
        let ChainAnswer::Slot(Some(first)) = self.read(&ChainRead::FirstAvailableBlock)? else {
            return Err("ARCHIVE_FIRST_SLOT_UNKNOWN");
        };
        let ChainAnswer::Slot(Some(tip)) = self.read(&ChainRead::FinalizedSlot)? else {
            return Err("FINALIZED_TIP_INVALID");
        };
        let mut lower = self.header_at_or_after(first, tip)?;
        let upper = self.header_at_or_after(tip, tip)?;
        let start = self.job.start_ns as i128;
        let end = self.job.end_ns as i128;
        let ns = |t: i64| t as i128 * 1_000_000_000;
        if ns(lower.1) >= start || ns(upper.1) < end {
            return Err("UTC_WINDOW_NOT_BRACKETED");
        }
        // Lower bound strictly before start. Missing/pruned probe fails closed.
        let mut hi = upper;
        while hi.0 > lower.0 + 1 {
            let mid = lower.0 + (hi.0 - lower.0) / 2;
            let probe = self.header_at_or_after(mid, hi.0)?;
            if probe.0 == hi.0 {
                break;
            }
            if ns(probe.1) < start {
                lower = probe;
            } else {
                hi = probe;
            }
        }
        let lo = lower.0;
        hi = upper;
        while hi.0 > lower.0 + 1 {
            let probe = self.header_at_or_after(lower.0 + (hi.0 - lower.0) / 2, hi.0)?;
            if probe.0 == hi.0 {
                break;
            }
            if ns(probe.1) < end {
                lower = probe;
            } else {
                hi = probe;
            }
        }
        Ok((lo, hi.0))
    }
}

/// What the sealed market journal of one batch proves.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BatchFacts {
    pub frames: u64,
    pub file_sha256: [u8; 32],
    /// Last produced-and-fetched block `(slot, blockhash)` (the next batch's parent anchor).
    pub anchor: Option<(u64, String)>,
    pub canonical: Vec<truth::TruthEvent>,
    /// Produced-and-fetched slots with their scanned transaction count.
    pub produced: Vec<(u64, u64)>,
    pub skipped: Vec<u64>,
    /// Program-relevant transactions `(signature, slot)` of the full-slot scan.
    pub program_transactions: Vec<(String, u64)>,
    /// RELATED_NON_LIQUIDATION instructions (never liquidations).
    pub related: Vec<super::related::MarketRelated>,
    /// The full-slot program transactions themselves; only kept on request (report reader).
    pub transactions: Vec<super::rows::MarketTransaction>,
}

/// Minimal native reader of one batch journal: verifies sequence, CRC, hash chain and seal and
/// decodes every typed frame (a schema-1 or foreign frame fails the batch).
pub fn read_batch(dir: &Path) -> Result<BatchFacts, &'static str> {
    read_batch_kept(dir, false)
}

fn read_batch_kept(dir: &Path, keep_transactions: bool) -> Result<BatchFacts, &'static str> {
    let path = dir.join(MARKET_JOURNAL);
    if crate::storage::segments::segment_path(&path, 2).exists() {
        return Err("BATCH_JOURNAL_SEGMENTED");
    }
    let mut r =
        crate::storage::segments::ChainReader::open(&path).map_err(|_| "BATCH_JOURNAL_INVALID")?;
    let mut f = BatchFacts::default();
    while let Some(frame) = r.next_frame().map_err(|_| "BATCH_JOURNAL_INVALID")? {
        match frame.kind {
            journal::MANIFEST => continue,
            kind::MARKET_COVERAGE => {
                match frame
                    .typed::<MarketCoverage>()
                    .map_err(|_| "BATCH_FRAME_INVALID")?
                {
                    MarketCoverage::SlotProduced {
                        slot,
                        blockhash,
                        transactions_scanned,
                        ..
                    } => {
                        f.produced.push((slot, transactions_scanned));
                        f.anchor = Some((slot, blockhash));
                    }
                    MarketCoverage::SlotSkipped { slot, .. } => f.skipped.push(slot),
                    _ => {}
                }
            }
            kind::MARKET_LIQUIDATION => {
                if let MarketLiquidation::Canonical(e) = frame
                    .typed::<MarketLiquidation>()
                    .map_err(|_| "BATCH_FRAME_INVALID")?
                {
                    f.canonical.push(*e);
                }
            }
            kind::MARKET_TRANSACTION => {
                let row = frame
                    .typed::<super::rows::MarketTransaction>()
                    .map_err(|_| "BATCH_FRAME_INVALID")?;
                if matches!(row.origin, super::rows::TxOrigin::FullSlot { .. }) {
                    f.program_transactions
                        .push((row.signature.clone(), row.slot));
                    if keep_transactions {
                        f.transactions.push(row);
                    }
                }
            }
            kind::MARKET_RELATED => f.related.push(
                frame
                    .typed::<super::related::MarketRelated>()
                    .map_err(|_| "BATCH_FRAME_INVALID")?,
            ),
            _ => return Err("BATCH_FRAME_KIND"),
        }
        f.frames += 1;
    }
    if !(r.sealed && r.complete) {
        return Err("BATCH_JOURNAL_INCOMPLETE");
    }
    f.file_sha256 = journal::file_sha256(&path).map_err(|_| "BATCH_JOURNAL_INVALID")?;
    Ok(f)
}

fn checkpoint_name(start: u64, end: u64) -> String {
    format!("batch_{start}_{end}.p0j")
}

/// A saved checkpoint must bind this spec, range and parent anchor, a complete proof and an
/// unchanged, verifiable batch journal.
fn verified_checkpoint(
    output: &Path,
    spec: &str,
    start: u64,
    end: u64,
    anchor: &Option<(u64, String)>,
    keep_transactions: bool,
) -> Result<Option<(StrictCheckpoint, BatchFacts)>, &'static str> {
    let path = output.join(checkpoint_name(start, end));
    if !path.exists() {
        return Ok(None);
    }
    let c: StrictCheckpoint = journal::read_typed_value(&path).map_err(|_| "CHECKPOINT_CORRUPT")?;
    if c.spec != spec
        || c.start != start
        || c.end != end
        || c.previous_anchor != *anchor
        || !c.proof.complete
    {
        return Err("CHECKPOINT_CORRUPT");
    }
    let d = &c.journal.directory;
    if d.contains('/') || d.contains('\\') || !d.starts_with("batch_") {
        return Err("CHECKPOINT_PATH");
    }
    let facts = read_batch_kept(&output.join(d), keep_transactions)
        .map_err(|_| "CHECKPOINT_EVIDENCE_CHANGED")?;
    if facts.file_sha256 != c.journal.file_sha256
        || facts.frames != c.journal.frames
        || facts.anchor != c.anchor
    {
        return Err("CHECKPOINT_EVIDENCE_CHANGED");
    }
    Ok(Some((c, facts)))
}

/// Single job, no wallet or send methods. Each committed batch is immutable.
/// A killed batch is replayed from verified cached RPC responses on resume.
pub fn run(job: Job) -> Result<StrictResult, String> {
    if job.invalid() {
        return Err("VERIFICATION_CONFIG_INVALID".into());
    }
    let program = Pubkey::from_str(&job.program).map_err(|_| "PROGRAM_INVALID")?;
    fs::create_dir_all(job.output.join("cache")).map_err(|_| "VERIFICATION_IO")?;
    let url = if job.offline {
        None
    } else {
        Some(
            darkbot_solana::runtime_config::active()
                .rpc_url(&|k| std::env::var(k).ok())
                .ok_or("RPC_URL_MISSING")?,
        )
    };
    let mut c = Client::new(&job, url)?;
    let mut scope_seen = None;
    let work = (|| -> Result<(), &'static str> {
        c.guard()?;
        let scope_path = job.output.join("scope.p0j");
        let scope = if scope_path.exists() {
            journal::read_typed_value::<StrictScope>(&scope_path)
                .map_err(|_| "CHECKPOINT_CORRUPT")?
        } else {
            let (lo, hi) = c.resolve_window()?;
            let s = StrictScope {
                verification_spec_id: job.verification_spec_id.clone(),
                start_ns: job.start_ns,
                end_ns: job.end_ns,
                lo,
                hi,
            };
            journal::write_typed_value(&scope_path, source::STRICT, &s).map_err(io_code)?;
            c.disk += file_len(&scope_path);
            s
        };
        if scope.verification_spec_id != job.verification_spec_id
            || scope.start_ns != job.start_ns
            || scope.end_ns != job.end_ns
        {
            return Err("RESUME_SCOPE_MISMATCH");
        }
        if scope.hi < scope.lo {
            return Err("CHECKPOINT_CORRUPT");
        }
        scope_seen = Some(scope.clone());
        let mut next = scope.lo;
        let mut anchor: Option<(u64, String)> = None;
        let mut first_time = None;
        let mut last_time = None;
        while next <= scope.hi {
            c.guard()?;
            let end = next.saturating_add(BATCH_SLOTS - 1).min(scope.hi);
            let saved = match verified_checkpoint(
                &job.output,
                &job.verification_spec_id,
                next,
                end,
                &anchor,
                false,
            )? {
                Some((saved, _)) => saved,
                None => {
                    let stamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|_| "CLOCK")?
                        .as_nanos();
                    let dirname = format!("batch_{next}_{end}_{stamp}");
                    let dir = job.output.join(&dirname);
                    fs::create_dir(&dir).map_err(|_| "EVIDENCE_IO")?;
                    // One file per batch: the checkpoint binds the batch journal by its file hash.
                    let mut sink = JournalSink::create_with(
                        &dir,
                        WriterConfig {
                            segment_bytes: 0,
                            ..WriterConfig::from_active()
                        },
                    )
                    .map_err(|_| "EVIDENCE_IO")?;
                    let mut core = ObserverCore::new(
                        program,
                        MarketSource::Transactions,
                        EnrichConfig::DEFAULT,
                    );
                    core.reconcile_cfg.max_attempts = 1;
                    core.reconcile_cfg.phase_max_secs = c
                        .deadline
                        .saturating_duration_since(Instant::now())
                        .as_secs()
                        .max(1);
                    let start = anchor.as_ref().map(|a| a.0).unwrap_or(next);
                    for (side, slot) in [(0, start), (1, end)] {
                        let b = reconcile::SlotBoundary {
                            slot,
                            t_rx_ns: 0,
                            observed_unix_s: 0,
                            provenance: "UTC_BRACKET_RESUMABLE_STRICT",
                        };
                        if side == 0 {
                            core.reconciliation.market_slot_start = Some(b);
                        } else {
                            core.reconciliation.market_slot_end = Some(b);
                        }
                    }
                    core.reconcile_slots(
                        &mut |read, _| {
                            let answer = c.read(read)?;
                            // The previous batch's last block must be the same block again.
                            if let (
                                reconcile::ChainRead::FullBlock { slot },
                                reconcile::ChainAnswer::Block { header, .. },
                                Some((s, h)),
                            ) = (read, &answer, &anchor)
                            {
                                if slot == s
                                    && header.as_ref().map(|x| x.blockhash.as_str()).ok()
                                        != Some(h.as_str())
                                {
                                    return Err("BATCH_ANCHOR_MISMATCH");
                                }
                            }
                            Ok(answer)
                        },
                        &mut sink,
                    )
                    .map_err(|_| "EVIDENCE_IO")?;
                    // D1: a batch is sealed COMPLETE only with a complete reconciliation.
                    sink.finish(core.reconciliation.complete)
                        .map_err(|_| "EVIDENCE_IO")?;
                    drop(sink);
                    if !core.reconciliation.complete {
                        return Err(core.reconciliation.error.unwrap_or("STRICT_INCOMPLETE"));
                    }
                    let facts = read_batch(&dir)?;
                    c.disk += file_len(&dir.join(MARKET_JOURNAL));
                    let checkpoint = StrictCheckpoint {
                        spec: job.verification_spec_id.clone(),
                        start: next,
                        end,
                        previous_anchor: anchor.clone(),
                        anchor: facts.anchor.clone().or_else(|| anchor.clone()),
                        journal: BatchJournal {
                            directory: dirname,
                            file_sha256: facts.file_sha256,
                            frames: facts.frames,
                        },
                        proof: (&core.reconciliation).into(),
                    };
                    let path = job.output.join(checkpoint_name(next, end));
                    journal::write_typed_value(&path, source::STRICT, &checkpoint)
                        .map_err(io_code)?;
                    c.disk += file_len(&path);
                    checkpoint
                }
            };
            anchor = saved.anchor.clone();
            if first_time.is_none() {
                first_time = saved.proof.first_block_time;
            }
            last_time = saved.proof.last_block_time.or(last_time);
            let name = checkpoint_name(next, end);
            let sha256 =
                journal::file_sha256(&job.output.join(&name)).map_err(|_| "EVIDENCE_IO")?;
            c.completed_batches.push(BatchRef {
                checkpoint: name,
                sha256,
            });
            next = end.checked_add(1).ok_or("SLOT_OVERFLOW")?;
        }
        if first_time.is_none_or(|v| (v as i128) * 1_000_000_000 >= job.start_ns as i128)
            || last_time.is_none_or(|v| (v as i128) * 1_000_000_000 < job.end_ns as i128)
        {
            return Err("UTC_WINDOW_NOT_BRACKETED");
        }
        Ok(())
    })();
    let result = StrictResult {
        verification_spec_id: job.verification_spec_id.clone(),
        status: if work.is_ok() {
            StrictStatus::CanonicalVerified
        } else {
            StrictStatus::HistoricalIncomplete
        },
        reason: work.err().map(str::to_owned),
        scope: scope_seen,
        batches: std::mem::take(&mut c.completed_batches),
        rpc_calls: c.calls,
        successful_response_bytes: c.bytes,
        c0_e1_master_pass: false,
    };
    // Every attempt's result is kept, content-addressed and immutable.
    let bytes = crate::analytics::payload::encode(&result).map_err(|e| e.to_string())?;
    let dir = job.output.join("results");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    journal::write_typed_value(
        &dir.join(format!("{}.p0j", journal::hex(&hash(&bytes).to_bytes()))),
        source::STRICT,
        &result,
    )
    .map_err(|e| e.to_string())?;
    Ok(result)
}

/// The verified strict evidence of an output directory, read natively (no JSON, no SQLite).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StrictReport {
    pub scope: Option<StrictScope>,
    pub batches_verified: u64,
    /// First slot without a verified checkpoint, `None` when the whole scope is verified.
    pub next_unverified_slot: Option<u64>,
    pub reason: Option<&'static str>,
    pub slots_expected: u64,
    pub slots_fetched: u64,
    pub slots_skipped_confirmed: u64,
    pub transactions_scanned: u64,
    pub marginfi_transactions: u64,
    pub canonical: Vec<truth::TruthEvent>,
    /// RELATED_NON_LIQUIDATION instructions, once per signature and position.
    pub related: Vec<super::related::MarketRelated>,
}

impl StrictReport {
    pub fn complete(&self) -> bool {
        self.scope.is_some() && self.next_unverified_slot.is_none() && self.reason.is_none()
    }
    pub fn by_type(&self) -> BTreeMap<truth::LiquidationType, u64> {
        let mut m = BTreeMap::new();
        for e in &self.canonical {
            *m.entry(e.liquidation_type).or_insert(0) += 1;
        }
        m
    }
    /// Human-readable terminal summary.
    pub fn terminal(&self) -> String {
        let mut out = String::from("PROJECT 0 STRICT VERIFICATION (native evidence)\n");
        match &self.scope {
            None => out.push_str("scope=NOT_RESOLVED\n"),
            Some(s) => out.push_str(&format!(
                "spec={} window_ns=[{},{}) slots=[{},{}]\n",
                s.verification_spec_id, s.start_ns, s.end_ns, s.lo, s.hi
            )),
        }
        out.push_str(&format!(
            "status={} batches_verified={} next_unverified_slot={} reason={}\n",
            if self.complete() {
                "CANONICAL_VERIFIED"
            } else {
                "HISTORICAL_INCOMPLETE"
            },
            self.batches_verified,
            self.next_unverified_slot
                .map_or("none".into(), |s| s.to_string()),
            self.reason.unwrap_or("none")
        ));
        out.push_str(&format!(
            "slots expected={} fetched={} skipped_confirmed={} transactions_scanned={} marginfi_transactions={}\n",
            self.slots_expected, self.slots_fetched, self.slots_skipped_confirmed,
            self.transactions_scanned, self.marginfi_transactions
        ));
        let by = self.by_type();
        for t in [
            truth::LiquidationType::Classic,
            truth::LiquidationType::Receivership,
            truth::LiquidationType::ForcedDeleverage,
            truth::LiquidationType::BankruptcySettlement,
        ] {
            out.push_str(&format!(
                "canonical {}={}\n",
                t.as_str(),
                by.get(&t).copied().unwrap_or(0)
            ));
        }
        for k in super::related::RelatedKind::ALL {
            let of = |status: &str| {
                self.related
                    .iter()
                    .filter(|x| x.kind == k && x.status.as_str() == status)
                    .count()
            };
            out.push_str(&format!(
                "related_non_liquidation {} success={} failed={} unknown={}\n",
                k.name(),
                of("SUCCESS"),
                of("FAILED"),
                of("UNKNOWN")
            ));
        }
        out.push_str("non_claims: strict proves finalized chain facts only; not DarkBot executability, not realized PnL, not the C0-E1 master gate\n");
        out
    }
}

/// Reads and verifies the native strict evidence of `output`: the scope, then every batch
/// checkpoint in slot order with its parent anchor and batch journal. Stops at the first missing
/// or invalid batch (reported, never skipped).
pub fn read_report(output: &Path) -> StrictReport {
    read_report_inner(output, None)
}

/// `read_report` that also hands each full-slot program transaction of every verified batch to
/// `visit` (inside the scope, once per signature and slot, in slot order) after its checkpoint
/// and batch journal verified; nothing of a rejected batch is ever visited.
pub fn read_report_with(
    output: &Path,
    visit: &mut dyn FnMut(&super::rows::MarketTransaction),
) -> StrictReport {
    read_report_inner(output, Some(visit))
}

fn read_report_inner(
    output: &Path,
    mut visit: Option<&mut dyn FnMut(&super::rows::MarketTransaction)>,
) -> StrictReport {
    let mut r = StrictReport::default();
    let scope = match journal::read_typed_value::<StrictScope>(&output.join("scope.p0j")) {
        Ok(s) => s,
        Err(_) => {
            r.reason = Some("SCOPE_MISSING_OR_INVALID");
            return r;
        }
    };
    let mut next = scope.lo;
    let mut anchor = None;
    // Each batch re-fetches its parent anchor block as proof: count every slot, transaction
    // and canonical event once, and only inside the scope.
    let (mut slots, mut skipped) = (BTreeSet::new(), BTreeSet::new());
    let (mut program, mut events, mut related) =
        (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let mut visited = BTreeSet::new();
    r.slots_expected = scope.hi - scope.lo + 1;
    while next <= scope.hi {
        let end = next.saturating_add(BATCH_SLOTS - 1).min(scope.hi);
        match verified_checkpoint(
            output,
            &scope.verification_spec_id,
            next,
            end,
            &anchor,
            visit.is_some(),
        ) {
            Ok(Some((c, facts))) => {
                r.batches_verified += 1;
                let inside = |s: &u64| (scope.lo..=scope.hi).contains(s);
                for (slot, txs) in facts.produced.iter().filter(|(s, _)| inside(s)) {
                    if slots.insert(*slot) {
                        r.slots_fetched += 1;
                        r.transactions_scanned += txs;
                    }
                }
                for slot in facts.skipped.iter().filter(|s| inside(s)) {
                    if skipped.insert(*slot) {
                        r.slots_skipped_confirmed += 1;
                    }
                }
                for key in facts
                    .program_transactions
                    .into_iter()
                    .filter(|k| inside(&k.1))
                {
                    if program.insert(key) {
                        r.marginfi_transactions += 1;
                    }
                }
                if let Some(visit) = visit.as_mut() {
                    for tx in facts.transactions.iter().filter(|t| inside(&t.slot)) {
                        if visited.insert((tx.signature.clone(), tx.slot)) {
                            visit(tx);
                        }
                    }
                }
                for e in facts.canonical.into_iter().filter(|e| inside(&e.slot)) {
                    if events.insert((e.signature.clone(), e.slot, e.event_index)) {
                        r.canonical.push(e);
                    }
                }
                for x in facts.related.into_iter().filter(|x| inside(&x.slot)) {
                    if related.insert((x.signature.clone(), x.position())) {
                        r.related.push(x);
                    }
                }
                anchor = c.anchor;
            }
            Ok(None) => {
                r.next_unverified_slot = Some(next);
                break;
            }
            Err(e) => {
                r.next_unverified_slot = Some(next);
                r.reason = Some(e);
                break;
            }
        }
        match end.checked_add(1) {
            Some(n) => next = n,
            None => break,
        }
    }
    r.scope = Some(scope);
    r
}

/// Test fixture: a JSON-RPC wire answer cached as the typed entry the verifier itself would
/// write — decoded through the same `wire_answer` edge (no test-only decode).
#[cfg(test)]
pub(crate) fn cache_fixture(output: &Path, read: ChainRead, wire: &Value) {
    let path = cache_path(output, &read);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _ = fs::remove_file(&path);
    let entry = ChainCacheEntry {
        read,
        decoder: WIRE_DECODER_VERSION,
        answer: reconcile::wire_answer(&read, wire),
    };
    journal::write_typed_value(&path, source::STRICT, &entry).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    fn job() -> Job {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Job {
            verification_spec_id: "fixture".into(),
            output: std::env::temp_dir().join(format!("p4-{stamp}")),
            start_ns: 11_000_000_000,
            end_ns: 79_000_000_000,
            program: darkbot_project0::protocol::PROGRAM_ID_MAINNET.into(),
            max_calls: 100,
            max_seconds: 60,
            max_bytes: 1024 * 1024,
            max_disk_bytes: 1024 * 1024 * 1024,
            max_ram_bytes: 64 * 1024 * 1024 * 1024,
            min_free_bytes: 1024 * 1024,
            rps: 2,
            offline: true,
            cancelled: None,
        }
    }
    fn cache(j: &Job, read: ChainRead, wire: Value) {
        cache_fixture(&j.output, read, &wire);
    }
    fn fixture(j: &Job, stop: Option<u64>) {
        fs::create_dir_all(&j.output).unwrap();
        let scope = StrictScope {
            verification_spec_id: "fixture".into(),
            start_ns: j.start_ns,
            end_ns: j.end_ns,
            lo: 10,
            hi: 80,
        };
        let path = j.output.join("scope.p0j");
        if !path.exists() {
            journal::write_typed_value(&path, source::STRICT, &scope).unwrap();
        }
        cache(j, ChainRead::FinalizedSlot, json!(90));
        for lo in [10, 41, 73] {
            cache(
                j,
                ChainRead::Blocks { start: lo, end: 90 },
                json!((lo..=90)
                    .filter(|s| *s != 15 && *s != 79)
                    .collect::<Vec<_>>()),
            );
        }
        let mut parent = 9;
        for slot in 10..=90 {
            if slot == 15 || slot == 79 {
                continue;
            }
            if stop.is_none_or(|stop| slot <= stop) {
                cache(
                    j,
                    ChainRead::FullBlock { slot },
                    json!({"parentSlot":parent,"previousBlockhash":format!("h{parent}"),"blockhash":format!("h{slot}"),"blockTime":slot,"transactions":[]}),
                );
            }
            parent = slot;
        }
    }
    fn reload(j: &Job) -> Job {
        Job {
            verification_spec_id: j.verification_spec_id.clone(),
            output: j.output.clone(),
            start_ns: j.start_ns,
            end_ns: j.end_ns,
            program: j.program.clone(),
            max_calls: j.max_calls,
            max_seconds: j.max_seconds,
            max_bytes: j.max_bytes,
            max_disk_bytes: j.max_disk_bytes,
            max_ram_bytes: j.max_ram_bytes,
            min_free_bytes: j.min_free_bytes,
            rps: j.rps,
            offline: true,
            cancelled: None,
        }
    }
    /// Interrupted batches resume identically from native checkpoints with parent/skip proofs;
    /// the native reader reports the same verified scope; a corrupt checkpoint never passes.
    #[test]
    fn interrupted_batches_resume_identically_with_parent_skip_proofs() {
        let j = job();
        fixture(&j, Some(43));
        let first = run(reload(&j)).unwrap();
        assert_eq!(first.status, StrictStatus::HistoricalIncomplete);
        let checkpoint = j.output.join("batch_10_41.p0j");
        assert!(checkpoint.exists());
        assert_eq!(first.batches.len(), 1);
        let partial = read_report(&j.output);
        assert_eq!(
            (partial.batches_verified, partial.next_unverified_slot),
            (1, Some(42))
        );
        let before = journal::file_sha256(&checkpoint).unwrap();
        fixture(&j, None);
        let resumed = run(reload(&j)).unwrap();
        assert_eq!(
            resumed.status,
            StrictStatus::CanonicalVerified,
            "{resumed:?}"
        );
        assert_eq!(resumed.rpc_calls, 0);
        assert_eq!(journal::file_sha256(&checkpoint).unwrap(), before);
        let repeated = run(reload(&j)).unwrap();
        assert_eq!(resumed, repeated);
        assert_eq!(resumed.batches.len(), 3);
        let report = read_report(&j.output);
        assert!(report.complete(), "{report:?}");
        assert_eq!(report.batches_verified, 3);
        // 71 slots: 69 produced, 2 proven skipped (15 and 79).
        assert_eq!(
            (report.slots_fetched, report.slots_skipped_confirmed),
            (69, 2)
        );
        assert!(report.terminal().contains("status=CANONICAL_VERIFIED"));
        // No JSON evidence exists (JFI-01: the compatibility render was removed).
        assert!(fs::read_dir(&j.output).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".json")));
        fs::write(&checkpoint, b"broken").unwrap();
        let bad = run(reload(&j)).unwrap();
        assert_eq!(bad.reason.as_deref(), Some("CHECKPOINT_CORRUPT"));
        assert_eq!(read_report(&j.output).reason, Some("CHECKPOINT_CORRUPT"));
        fs::remove_dir_all(j.output).unwrap();
    }
    #[test]
    fn disk_budget_and_changed_scope_fail_closed() {
        let mut j = job();
        fixture(&j, None);
        j.max_disk_bytes = 1;
        assert_eq!(
            run(reload(&j)).unwrap().reason.as_deref(),
            Some("VERIFICATION_DISK_BUDGET")
        );
        j.max_disk_bytes = 1024 * 1024 * 1024;
        j.start_ns += 1;
        assert_eq!(
            run(reload(&j)).unwrap().reason.as_deref(),
            Some("RESUME_SCOPE_MISMATCH")
        );
        fs::remove_dir_all(j.output).unwrap();
    }
    #[test]
    fn wrong_parent_and_corrupt_cached_response_never_pass() {
        let j = job();
        fixture(&j, None);
        let read = ChainRead::FullBlock { slot: 42 };
        let p = cache_path(&j.output, &read);
        let original: ChainCacheEntry = journal::read_typed_value(&p).unwrap();
        let rewrite = |e: &ChainCacheEntry| {
            fs::remove_file(&p).unwrap();
            journal::write_typed_value(&p, source::STRICT, e).unwrap();
        };
        // A damaged file, an entry of another read and another decoder version never pass.
        let mut bytes = fs::read(&p).unwrap();
        let at = bytes.len() / 2;
        bytes[at] ^= 0xff;
        fs::write(&p, &bytes).unwrap();
        assert_eq!(
            run(reload(&j)).unwrap().reason.as_deref(),
            Some("CACHE_CORRUPT")
        );
        let mut e = original.clone();
        e.read = ChainRead::FullBlock { slot: 43 };
        rewrite(&e);
        assert_eq!(
            run(reload(&j)).unwrap().reason.as_deref(),
            Some("CACHE_CORRUPT")
        );
        let mut e = original.clone();
        e.decoder = WIRE_DECODER_VERSION + 1;
        rewrite(&e);
        assert_eq!(
            run(reload(&j)).unwrap().reason.as_deref(),
            Some("CACHE_DECODER_VERSION")
        );
        // A consistent but wrong answer is caught by the canonical parent chain.
        let mut e = original;
        let ChainAnswer::Block {
            header: Ok(header),
            block: Ok(block),
        } = &mut e.answer
        else {
            panic!("fixture block");
        };
        header.parent_slot = 40;
        block.parent_slot = 40;
        rewrite(&e);
        assert_eq!(
            run(reload(&j)).unwrap().reason.as_deref(),
            Some("FINALIZED_PARENT_CHAIN_MISMATCH")
        );
        fs::remove_dir_all(j.output).unwrap();
    }
    /// U-8 (PERSIST-04): the binary layout of `STRICT_CHAIN_CACHE` (318) is pinned over a
    /// decoded block entry. A failing pin is a layout change: bump `SCHEMA` (or, for a changed
    /// decode, `WIRE_DECODER_VERSION`) and renew the pin — never silently.
    #[test]
    fn chain_cache_layout_is_pinned() {
        let read = ChainRead::FullBlock { slot: 42 };
        let wire = json!({"parentSlot": 41, "previousBlockhash": "h41", "blockhash": "h42",
            "blockTime": 42, "blockHeight": 40, "transactions": [{"transaction": 1}]});
        let e = ChainCacheEntry {
            read,
            decoder: WIRE_DECODER_VERSION,
            answer: reconcile::wire_answer(&read, &wire),
        };
        let hash = hash(&crate::analytics::payload::encode(&e).unwrap()).to_string();
        assert_eq!(
            (
                ChainCacheEntry::KIND,
                ChainCacheEntry::SCHEMA,
                hash.as_str()
            ),
            (318, 2, "29UrVy3axv8swUmRGy7hWGz5musWJkwafSkNs571ydSx")
        );
    }

    /// Loopback JSON-RPC endpoint answering from the request body: `getBlocks` lists every slot,
    /// `getBlock` with `transactionDetails = none` is a header-only block, `full` carries its
    /// (empty) transaction list.
    fn loopback_chain() -> String {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut request = vec![0u8; length];
                let _ = reader.read_exact(&mut request);
                let request: Value = serde_json::from_slice(&request).unwrap_or(Value::Null);
                let params = &request["params"];
                let result = match request["method"].as_str() {
                    Some("getBlocks") => json!((params[0].as_u64().unwrap()
                        ..=params[1].as_u64().unwrap())
                        .collect::<Vec<_>>()),
                    Some("getBlock") => {
                        let slot = params[0].as_u64().unwrap();
                        let mut block = json!({"parentSlot": slot - 1,
                            "previousBlockhash": format!("h{}", slot - 1),
                            "blockhash": format!("h{slot}"), "blockTime": slot});
                        if params[1]["transactionDetails"] == "full" {
                            block["transactions"] = json!([]);
                        }
                        block
                    }
                    _ => json!(90),
                };
                let body = json!({"jsonrpc": "2.0", "id": 1, "result": result}).to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            }
        });
        url
    }

    /// F10: header probes of the window resolution never fill the full-block read-ahead. A full
    /// read of a slot whose header was probed (and read ahead) gets the block with its
    /// transactions, and every answer is cached typed under its own read.
    #[test]
    fn header_probes_never_serve_a_full_block_read() {
        use reconcile::{ChainAnswer, ChainRead};
        let mut j = job();
        j.offline = false;
        fs::create_dir_all(j.output.join("cache")).unwrap();
        let mut c = Client {
            job: &j,
            rpc: Some(RpcFetcher::new(loopback_chain(), 5_000, "finalized").unwrap()),
            deadline: Instant::now() + Duration::from_secs(60),
            pacer: RequestPacer::new(1_000),
            calls: 0,
            bytes: 0,
            disk: 0,
            produced: Vec::new(),
            held: BTreeMap::new(),
            previous_block: None,
            completed_batches: Vec::new(),
        };
        assert!(matches!(
            c.read(&ChainRead::Blocks { start: 10, end: 40 }),
            Ok(ChainAnswer::Blocks(Some(_)))
        ));
        assert!(matches!(
            c.read(&ChainRead::BlockHeader { slot: 10 }),
            Ok(ChainAnswer::Header(Ok(_)))
        ));
        let Ok(ChainAnswer::Block { block, .. }) = c.read(&ChainRead::FullBlock { slot: 11 })
        else {
            panic!("full block read failed");
        };
        assert_eq!(block.map(|b| (b.slot, b.transactions.len())), Ok((11, 0)));
        fs::remove_dir_all(&j.output).unwrap();
    }

    /// The bounded read-ahead takes at most `limit` produced slots without held or cached
    /// responses; an unknown slot is fetched alone.
    #[test]
    fn read_ahead_group_is_bounded_and_skips_known_slots() {
        let produced: Vec<u64> = (10..30).collect();
        let mut held = BTreeMap::new();
        held.insert(12, Ok(ChainAnswer::Slot(Some(12))));
        let cached = |s: u64| s == 13;
        assert_eq!(
            read_ahead_group(10, &produced, &held, &cached, 8),
            vec![10, 11, 14, 15, 16, 17, 18, 19]
        );
        assert_eq!(read_ahead_group(5, &produced, &held, &cached, 8), vec![5]);
        assert_eq!(read_ahead_group(29, &produced, &held, &cached, 8), vec![29]);
    }
}

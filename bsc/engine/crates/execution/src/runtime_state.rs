//! The C2 chain-state owner inside the runtime (component `BSC_STATE_C2`,
//! effective `SHADOW`).
//!
//! The one native [`ChainState`] receives every canonical input of the
//! follower (blocks with all their transactions, reorgs, resyncs, gaps) and
//! the pending sightings of launch candidates and of the watched senders.
//! It never reads a clock: a sighting carries its time on the runtime's one
//! monotonic clock.
//!
//! Restart: the retained canonical window is checkpointed atomically in the
//! runtime pass every `checkpoint_ms`, when the component is switched off
//! and at stop (never on an entry path). At start it is restored and every
//! watched sender is quarantined until a nonce fact reconciles it; the
//! follower then resumes after that window, so the blocks mined while the
//! process was down are followed (or reported as a gap). Pending state is
//! not persisted. An unreadable or contradicting checkpoint is refused,
//! visibly, and the state starts empty.

use crate::chain_follower::CanonicalEvent;
use crate::wss::BSC_MAINNET_CHAIN_ID;
use alloy_primitives::{Address, B256};
use darkbot_bsc_record::Draft;
use darkbot_bsc_record::schema::bsc_runtime::runtime_status::state as e;
use darkbot_bsc_trading::chain_state::canonical::{
    ExecutionStatus, FinalityProof, InclusionProof, OrphanProof,
};
use darkbot_bsc_trading::chain_state::engine::{ChainState, Conflict, StateLimits};
use darkbot_bsc_trading::chain_state::input::{
    CanonicalBlock, CanonicalTx, ChainInput, FirstSeen, PendingObservation,
};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

/// The C2 bounds and the watched senders (`BSC_STATE_*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateConfig {
    /// `reorg_window` is the follower's `reorg_depth`: a reorg the follower
    /// reports exactly is applied exactly.
    pub limits: StateLimits,
    /// The own accounts whose transactions get proofs; none in READ_ONLY.
    pub watched: Vec<[u8; 20]>,
    pub checkpoint_ms: u64,
    /// The follower reads the watched senders' nonces at every this-many
    /// blocks (and after a start, reorg or resync).
    pub nonce_fact_blocks: u64,
}

impl StateConfig {
    /// `BSC_STATE_WATCHED`: comma-separated `0x` addresses, possibly none.
    pub fn parse_watched(text: &str) -> Result<Vec<[u8; 20]>, String> {
        let mut out = Vec::new();
        for item in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let hex = item
                .strip_prefix("0x")
                .filter(|h| h.len() == 40)
                .ok_or_else(|| format!("BSC_STATE_WATCHED: {item} is not a 0x address"))?;
            let mut a = [0u8; 20];
            alloy_primitives::hex::decode_to_slice(hex, &mut a)
                .map_err(|_| format!("BSC_STATE_WATCHED: {item} is not hex"))?;
            out.push(a);
        }
        Ok(out)
    }

    /// The owner's own rules (bounds, watched count, duplicates).
    pub fn validate(&self) -> Result<(), String> {
        self.new_state().map(|_| ())
    }

    fn watched(&self) -> Vec<Address> {
        self.watched.iter().map(|a| Address::from(*a)).collect()
    }

    fn new_state(&self) -> Result<ChainState, String> {
        ChainState::new(BSC_MAINNET_CHAIN_ID, self.limits, &self.watched())
            .map_err(|e| format!("chain state: {e}"))
    }
}

/// C2's evidence about a watched sender's nonce (plan 28.10.): what the
/// LIVE path's lanes resolve their intents by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneEvidence {
    Inclusion(InclusionProof),
    Finality(FinalityProof),
    Orphan(OrphanProof),
}

/// The owner and what the runtime adds around it.
pub struct StateOwner {
    state: ChainState,
    /// The watched senders, ascending.
    watched: Vec<[u8; 20]>,
    path: PathBuf,
    restore: &'static str,
    restored_blocks: u64,
    checkpoint_writes: u64,
    checkpoint_errors: u64,
    last_conflict: Option<&'static str>,
    /// The latest refused input's kind (a later non-refusing conflict never
    /// hides it).
    last_refused: Option<&'static str>,
    /// Evidence about the watched senders since the runtime last took it
    /// ([`StateOwner::take_evidence`], after every canonical event).
    evidence: Vec<LaneEvidence>,
}

impl StateOwner {
    /// At process start: restores the checkpoint at `path` if there is one.
    /// Either way this is a restart: every watched sender is quarantined.
    pub fn restore(config: &StateConfig, path: PathBuf) -> Result<Self, String> {
        let (state, restore) = match std::fs::read(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (config.new_state()?, "NONE"),
            Err(e) => {
                eprintln!("c2 checkpoint {}: {e}", path.display());
                (config.new_state()?, "REFUSED")
            }
            Ok(bytes) => match ChainState::restore(
                BSC_MAINNET_CHAIN_ID,
                config.limits,
                &config.watched(),
                &bytes,
            ) {
                Ok(state) => (state, "RESTORED"),
                Err(e) => {
                    eprintln!("c2 checkpoint {} refused: {e}", path.display());
                    (config.new_state()?, "REFUSED")
                }
            },
        };
        let mut owner = Self::with(state, config, path, restore);
        if restore != "RESTORED" {
            owner.state.quarantine_all();
        }
        owner.restored_blocks = owner.state.counters().restored_blocks;
        Ok(owner)
    }

    /// Applies the component's effective mode while running: switched on,
    /// a fresh owner (the follower is past any checkpoint); switched off,
    /// the last window is checkpointed for the next start and the owner
    /// ends.
    pub fn switch(
        owner: &mut Option<StateOwner>,
        wanted: bool,
        config: &StateConfig,
        path: &Path,
        replace: impl Fn(&Path, &[u8]) -> Result<(), String>,
    ) -> Result<(), String> {
        match (wanted, owner.is_some()) {
            (true, false) => *owner = Some(Self::fresh(config, path.to_path_buf())?),
            (false, true) => {
                if let Some(mut o) = owner.take() {
                    o.checkpoint(replace);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Switched on while running: the follower is past any checkpoint, so
    /// the state starts empty (and quarantined, as after a restart).
    pub fn fresh(config: &StateConfig, path: PathBuf) -> Result<Self, String> {
        let mut owner = Self::with(config.new_state()?, config, path, "NONE");
        owner.state.quarantine_all();
        Ok(owner)
    }

    fn with(state: ChainState, config: &StateConfig, path: PathBuf, restore: &'static str) -> Self {
        let mut watched = config.watched.clone();
        watched.sort_unstable();
        StateOwner {
            state,
            watched,
            path,
            restore,
            restored_blocks: 0,
            checkpoint_writes: 0,
            checkpoint_errors: 0,
            last_conflict: None,
            last_refused: None,
            evidence: Vec::new(),
        }
    }

    /// The window the follower resumes after (empty: start at the head).
    pub fn resume_window(&self) -> Vec<(u64, [u8; 32])> {
        self.state
            .retained()
            .into_iter()
            .map(|(n, h)| (n, h.0))
            .collect()
    }

    /// The coherent snapshot for `candidate` under the given generations
    /// (C2 decides; `None` when it is not coherent or the candidate is not
    /// an eligible held candidate).
    pub fn snapshot(
        &self,
        candidate: B256,
        policy_generation: u64,
        codehash_generation: u64,
    ) -> Option<darkbot_bsc_trading::chain_state::SnapshotId> {
        self.state
            .snapshot_id(candidate, policy_generation, codehash_generation)
    }

    /// The coherent snapshot of the canonical state alone (Phase I: a
    /// Strategy C intent on a curve replayed from canonical blocks).
    pub fn canonical_snapshot(
        &self,
        policy_generation: u64,
        codehash_generation: u64,
    ) -> Option<darkbot_bsc_trading::chain_state::SnapshotId> {
        self.state
            .canonical_snapshot_id(policy_generation, codehash_generation)
    }

    /// The canonical head as a submission plan sees it: number and
    /// timestamp in unix seconds.
    pub fn head(&self) -> Option<darkbot_bsc_trading::submission_policy::plan::ChainHead> {
        self.state.head_block().map(|(number, _, timestamp_ms)| {
            darkbot_bsc_trading::submission_policy::plan::ChainHead {
                number,
                timestamp: timestamp_ms / 1_000,
            }
        })
    }

    /// The hash of the canonical block `number` C2 holds (none: outside
    /// its retained window).
    pub fn canonical_hash(&self, number: u64) -> Option<[u8; 32]> {
        self.state.retained_hash(number).map(|h| h.0)
    }

    /// The next canonical nonce of `sender` when C2 knows it exactly.
    pub fn clean_next_nonce(&self, sender: &[u8; 20]) -> Option<u64> {
        match self.state.sender(Address::from(*sender)) {
            darkbot_bsc_trading::chain_state::engine::SenderView::Clean { next_nonce } => {
                Some(next_nonce)
            }
            _ => None,
        }
    }

    /// Whether `sender` is one of the watched (own) accounts.
    pub fn watches(&self, sender: &[u8; 20]) -> bool {
        self.watched.binary_search(sender).is_ok()
    }

    /// The evidence of the inputs applied since the last call, in order:
    /// inclusions, finalities and orphans as the owner produced them.
    pub fn take_evidence(&mut self) -> Vec<LaneEvidence> {
        std::mem::take(&mut self.evidence)
    }

    fn apply(&mut self, input: &ChainInput) {
        let applied = self.state.apply(input);
        // Orphans first: a reorg's orphaned nonces before the new chain's
        // inclusions (the engine's order within one input).
        self.evidence
            .extend(applied.orphans.iter().copied().map(LaneEvidence::Orphan));
        self.evidence.extend(
            applied
                .inclusions
                .iter()
                .copied()
                .map(LaneEvidence::Inclusion),
        );
        self.evidence.extend(
            applied
                .finalities
                .iter()
                .copied()
                .map(LaneEvidence::Finality),
        );
        if let Some(c) = applied.conflicts.last() {
            self.last_conflict = Some(conflict_kind(c));
        }
        if let Some(c) = applied.conflicts.iter().rev().find(|c| c.refused()) {
            self.last_refused = Some(conflict_kind(c));
        }
    }

    /// One canonical event of the follower, before the G0.4 join sees it.
    pub fn canonical(&mut self, event: &CanonicalEvent) {
        let input = match event {
            CanonicalEvent::Block {
                number,
                hash,
                parent_hash,
                timestamp_ms,
                txs,
                ..
            } => ChainInput::Block(CanonicalBlock {
                number: *number,
                hash: B256::from(*hash),
                parent_hash: B256::from(*parent_hash),
                timestamp_ms: *timestamp_ms,
                txs: txs
                    .iter()
                    .map(|t| CanonicalTx {
                        hash: B256::from(t.hash),
                        // Beyond u32 the block is malformed for the owner,
                        // which refuses it (index = position).
                        index: u32::try_from(t.index).unwrap_or(u32::MAX),
                        sender: Address::from(t.sender),
                        nonce: t.nonce,
                        status: if t.succeeded {
                            ExecutionStatus::Succeeded
                        } else {
                            ExecutionStatus::Reverted
                        },
                    })
                    .collect(),
            }),
            CanonicalEvent::Reorg { orphaned, new_head } => ChainInput::Reorg {
                orphaned: ids(orphaned),
                new_head: (new_head.0, B256::from(new_head.1)),
            },
            CanonicalEvent::ReorgTooDeep { orphaned, new_head } => ChainInput::ReorgTooDeep {
                orphaned: ids(orphaned),
                new_head: (new_head.0, B256::from(new_head.1)),
            },
            CanonicalEvent::Gap { from, to } => ChainInput::Gap {
                from: *from,
                to: *to,
            },
            CanonicalEvent::NonceFact {
                sender,
                next_nonce,
                block_number,
                block_hash,
            } => ChainInput::NonceFact {
                sender: Address::from(*sender),
                next_nonce: *next_nonce,
                block_number: *block_number,
                block_hash: B256::from(*block_hash),
            },
            CanonicalEvent::Launch(_)
            | CanonicalEvent::RevertedLaunch { .. }
            | CanonicalEvent::FlapTrade(_)
            | CanonicalEvent::FlapTerm(_)
            | CanonicalEvent::FlapMigrated { .. }
            | CanonicalEvent::FlapTradeUndecodable { .. }
            | CanonicalEvent::BlockComplete { .. }
            | CanonicalEvent::Fault(_) => return,
        };
        self.apply(&input);
    }

    /// A verified full pending sighting of a launch candidate or of a
    /// watched sender's transaction, at `at_ns` on the runtime clock.
    pub fn pending(
        &mut self,
        hash: [u8; 32],
        sender: [u8; 20],
        nonce: u64,
        source: u8,
        at_ns: u64,
    ) {
        self.apply(&ChainInput::Pending(PendingObservation {
            hash: B256::from(hash),
            sender: Address::from(sender),
            nonce,
            first_seen: FirstSeen { source, at_ns },
        }));
    }

    /// Writes the retained window atomically; a failure is counted and
    /// logged, the owner goes on.
    pub fn checkpoint(&mut self, replace: impl Fn(&Path, &[u8]) -> Result<(), String>) {
        let written = replace(&self.path, &self.checkpoint_bytes());
        self.checkpoint_written(written);
    }

    /// The retained window as a checkpoint: an immutable copy another
    /// thread may write ([`CheckpointWriter`]).
    pub fn checkpoint_bytes(&self) -> Vec<u8> {
        self.state.encode_checkpoint()
    }

    /// Tests: the checkpoint writes and failures counted.
    #[cfg(test)]
    pub(crate) fn checkpoint_counts(&self) -> (u64, u64) {
        (self.checkpoint_writes, self.checkpoint_errors)
    }

    /// Counts one checkpoint write; a failure is logged, the owner goes on.
    pub fn checkpoint_written(&mut self, written: Result<(), String>) {
        match written {
            Ok(()) => self.checkpoint_writes += 1,
            Err(e) => {
                self.checkpoint_errors += 1;
                eprintln!("c2 checkpoint: {e}");
            }
        }
    }

    pub fn draft(&self) -> Draft {
        let c = self.state.counters();
        let mut d = Draft::new(&e::SCHEMA);
        d.set(e::COHERENT, self.state.coherent())
            .set(e::WATCHED, self.watched.len() as u64)
            .set(e::RETAINED_BLOCKS, c.retained_blocks)
            .set(e::HELD_TXS, c.held_txs)
            .set(e::HELD_CANDIDATES, c.held_candidates)
            .set(e::HELD_UNCERTAIN, c.held_uncertain)
            .set(e::HELD_SENDERS, c.held_senders)
            .set(e::UNKNOWN_SENDERS, c.unknown_senders)
            .set(e::QUARANTINED_SENDERS, c.quarantined_senders)
            .set(e::INPUTS, c.inputs)
            .set(e::EFFECTS, c.effects)
            .set(e::PENDING_OBSERVATIONS, c.pending_observations)
            .set(e::REPEAT_SIGHTINGS, c.repeat_sightings)
            .set(e::EARLIER_FIRST_SEEN, c.earlier_first_seen)
            .set(e::LATE_SIGHTINGS, c.late_sightings)
            .set(e::STALE_PENDING, c.stale_pending)
            .set(e::COMPETING_CANDIDATES, c.competing_candidates)
            .set(e::BLOCKS, c.blocks)
            .set(e::INCLUDED, c.included)
            .set(e::REVERTED, c.reverted)
            .set(e::REPLACED, c.replaced)
            .set(e::REORGS, c.reorgs)
            .set(e::REORGS_BEYOND_WINDOW, c.reorgs_beyond_window)
            .set(e::REORGS_TOO_DEEP, c.reorgs_too_deep)
            .set(e::UNMATCHED_ORPHANED_BLOCKS, c.unmatched_orphaned_blocks)
            .set(e::ORPHANED_TXS, c.orphaned_txs)
            .set(e::GAPS, c.gaps)
            .set(e::UNCERTAIN_MARKED, c.uncertain_marked)
            .set(e::FINALIZED_BLOCKS, c.finalized_blocks)
            .set(e::FINALIZED_TXS, c.finalized_txs)
            .set(e::NONCE_FACTS, c.nonce_facts)
            .set(e::IGNORED_NONCE_FACTS, c.ignored_nonce_facts)
            .set(e::RECONCILIATIONS, c.reconciliations)
            .set(e::RECONCILIATION_REFUSALS, c.reconciliation_refusals)
            .set(e::QUARANTINES, c.quarantines)
            .set(e::CONFLICTS, c.conflicts)
            .set(e::REFUSED_INPUTS, c.refused_inputs)
            .set(e::OVERFLOW_TXS, c.overflow_txs)
            .set(e::OVERFLOW_CANDIDATES, c.overflow_candidates)
            .set(e::EVICTED, c.evicted)
            .set(e::RESTORED_BLOCKS, self.restored_blocks)
            .set(e::CHECKPOINT_RESTORE, self.restore)
            .set(e::CHECKPOINT_WRITES, self.checkpoint_writes)
            .set(e::CHECKPOINT_ERRORS, self.checkpoint_errors)
            .set(e::DISCONTINUITIES, c.discontinuities)
            .set(e::CONSUMED_NOT_SEEN, c.consumed_not_seen)
            .set(e::CONSUMED_UNATTRIBUTED, c.consumed_unattributed)
            .set(
                e::RESOLVED_BY_LATER_INCLUSION,
                c.resolved_by_later_inclusion,
            )
            .set(e::RESOLUTIONS_REVERTED, c.resolutions_reverted);
        if let Some((n, _)) = self.state.high_water() {
            d.set(e::HEAD, n);
        }
        if let Some(kind) = self.last_conflict {
            d.set(e::LAST_CONFLICT, kind);
        }
        if let Some(kind) = self.last_refused {
            d.set(e::LAST_REFUSED, kind);
        }
        d
    }
}

fn ids(blocks: &[(u64, [u8; 32])]) -> Vec<(u64, B256)> {
    blocks.iter().map(|(n, h)| (*n, B256::from(*h))).collect()
}

fn conflict_kind(c: &Conflict) -> &'static str {
    match c {
        Conflict::Discontinuity { .. } => "DISCONTINUITY",
        Conflict::MalformedGap { .. } => "MALFORMED_GAP",
        Conflict::MalformedBlock { .. } => "MALFORMED_BLOCK",
        Conflict::OversizeBlock { .. } => "OVERSIZE_BLOCK",
        Conflict::DoubleInclusion { .. } => "DOUBLE_INCLUSION",
        Conflict::NonceConsumedTwice { .. } => "NONCE_CONSUMED_TWICE",
        Conflict::IdentityMismatch { .. } => "IDENTITY_MISMATCH",
        Conflict::ReorgMismatch => "REORG_MISMATCH",
        Conflict::GapMismatch { .. } => "GAP_MISMATCH",
        Conflict::NonceDiscontinuity { .. } => "NONCE_DISCONTINUITY",
    }
}

/// How a checkpoint reaches its file (the runtime's durable `replace`).
pub type Replace = Arc<dyn Fn(&Path, &[u8]) -> Result<(), String> + Send + Sync>;

/// One finished write: its window's generation and how it went.
type Written = (u64, Result<(), String>);

/// BSC3-039: the C2 checkpoint written off the decision loop. The loop
/// encodes the retained window (an immutable copy, 1.3 ms for 1.25 MB
/// measured) and hands it over; one writer thread replaces the file
/// durably (10–34 ms measured for 479 KB). At most one write runs and one
/// waits: a newer window replaces a waiting older one, so the memory is
/// bounded and the newest wins. Every finished write returns with its
/// window's generation to the loop, which counts it on the owner
/// ([`CheckpointWriter::finished`]). The file has one writer at a time:
/// the loop writes the last window (the component switched off, the stop)
/// through [`CheckpointWriter::write_now`], which waits until nothing
/// waits or runs and holds the writer meanwhile. A writer thread that
/// ended (it never should) leaves the writes to the loop.
pub struct CheckpointWriter {
    shared: Arc<(Mutex<Slot>, Condvar)>,
    done: Receiver<Written>,
    handle: Option<JoinHandle<()>>,
    path: PathBuf,
    replace: Replace,
    /// The generation of the next window handed over.
    next: Cell<u64>,
    /// The newest generation counted.
    counted: Cell<u64>,
    /// Writes the loop made because the writer thread had ended.
    here: RefCell<Vec<Written>>,
}

#[derive(Default)]
struct Slot {
    waiting: Option<(u64, Vec<u8>)>,
    busy: bool,
    stop: bool,
}

impl CheckpointWriter {
    pub fn spawn(path: PathBuf, replace: Replace) -> Result<CheckpointWriter, String> {
        let shared = Arc::new((Mutex::new(Slot::default()), Condvar::new()));
        let (tx, done) = channel();
        let theirs = Arc::clone(&shared);
        let (their_path, their_replace) = (path.clone(), Arc::clone(&replace));
        let handle = thread::Builder::new()
            .name("bsc-c2-checkpoint".into())
            .spawn(move || {
                let (slot, changed) = &*theirs;
                loop {
                    let (generation, bytes) = {
                        let mut s = slot.lock().unwrap_or_else(PoisonError::into_inner);
                        while s.waiting.is_none() && !s.stop {
                            s = changed.wait(s).unwrap_or_else(PoisonError::into_inner);
                        }
                        let Some(window) = s.waiting.take() else {
                            return;
                        };
                        s.busy = true;
                        window
                    };
                    let written = their_replace(&their_path, &bytes);
                    let _ = tx.send((generation, written));
                    slot.lock().unwrap_or_else(PoisonError::into_inner).busy = false;
                    changed.notify_all();
                }
            })
            .map_err(|e| format!("c2 checkpoint writer: {e}"))?;
        Ok(CheckpointWriter {
            shared,
            done,
            handle: Some(handle),
            path,
            replace,
            next: Cell::new(1),
            counted: Cell::new(0),
            here: RefCell::new(Vec::new()),
        })
    }

    fn alive(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| !h.is_finished())
    }

    /// Hands `bytes` to the writer; it never waits for a write (a writer
    /// that ended: written here).
    pub fn submit(&self, bytes: Vec<u8>) {
        let generation = self.next.replace(self.next.get() + 1);
        if !self.alive() {
            let written = (self.replace)(&self.path, &bytes);
            self.here.borrow_mut().push((generation, written));
            return;
        }
        let (slot, changed) = &*self.shared;
        slot.lock().unwrap_or_else(PoisonError::into_inner).waiting = Some((generation, bytes));
        changed.notify_all();
    }

    /// The writes finished since the last call, oldest first, never
    /// waiting; one older than a write already counted is refused.
    pub fn finished(&self) -> Vec<Result<(), String>> {
        let mut writes: Vec<Written> = self.done.try_iter().collect();
        writes.append(&mut self.here.borrow_mut());
        writes.sort_by_key(|(generation, _)| *generation);
        writes
            .into_iter()
            .map(|(generation, written)| {
                if generation <= self.counted.get() {
                    return Err(format!(
                        "window {generation} finished after window {}",
                        self.counted.get()
                    ));
                }
                self.counted.set(generation);
                written
            })
            .collect()
    }

    /// Waits until no write waits or runs: every write finished since the
    /// last call.
    pub fn idle(&self) -> Vec<Result<(), String>> {
        drop(self.held());
        self.finished()
    }

    /// Writes `bytes` with `replace` on the calling thread once no write
    /// waits or runs, holding the writer meanwhile: never beside one of its
    /// writes, always after them.
    pub fn write_now(
        &self,
        path: &Path,
        bytes: &[u8],
        replace: impl Fn(&Path, &[u8]) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut s = self.held();
        s.busy = true;
        drop(s);
        let written = replace(path, bytes);
        let (slot, changed) = &*self.shared;
        slot.lock().unwrap_or_else(PoisonError::into_inner).busy = false;
        changed.notify_all();
        written
    }

    /// The slot once nothing waits or runs (a writer that ended leaves
    /// nothing to wait for).
    fn held(&self) -> std::sync::MutexGuard<'_, Slot> {
        let (slot, changed) = &*self.shared;
        let mut s = slot.lock().unwrap_or_else(PoisonError::into_inner);
        while (s.waiting.is_some() || s.busy) && self.alive() {
            s = changed
                .wait_timeout(s, std::time::Duration::from_millis(100))
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        s
    }
}

impl Drop for CheckpointWriter {
    fn drop(&mut self) {
        let (slot, changed) = &*self.shared;
        slot.lock().unwrap_or_else(PoisonError::into_inner).stop = true;
        changed.notify_all();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain_follower::CanonicalTx as FollowerTx;
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status::state as e;
    use darkbot_bsc_trading::chain_state::engine::{SenderReason, SenderView, TxState};
    use std::time::Instant;

    const W: [u8; 20] = [0xAA; 20];
    const F: [u8; 20] = [0xFF; 20];

    fn config(watched: Vec<[u8; 20]>) -> StateConfig {
        StateConfig {
            limits: StateLimits {
                max_txs: 1_024,
                max_candidates_per_nonce: 4,
                max_watched_senders: 4,
                reorg_window: 8,
                finality_depth: 2,
            },
            watched,
            checkpoint_ms: 1_000,
            nonce_fact_blocks: 1,
        }
    }

    /// Block `n` of fork `fork`.
    fn id(n: u64, fork: u8) -> [u8; 32] {
        let mut h = [0u8; 32];
        h[0] = fork;
        h[24..].copy_from_slice(&n.to_be_bytes());
        h
    }

    fn block(
        n: u64,
        fork: u8,
        parent_fork: u8,
        txs: &[([u8; 32], [u8; 20], u64)],
    ) -> CanonicalEvent {
        CanonicalEvent::Block {
            number: n,
            hash: id(n, fork),
            parent_hash: id(n - 1, parent_fork),
            timestamp_s: 1_800_000_000 + n,
            timestamp_ms: (1_800_000_000 + n) * 1_000,
            txs: txs
                .iter()
                .enumerate()
                .map(|(i, (hash, sender, nonce))| FollowerTx {
                    hash: *hash,
                    index: i as u64,
                    sender: *sender,
                    nonce: *nonce,
                    succeeded: true,
                    cost: None,
                })
                .collect(),
            observed: Instant::now(),
            observed_unix_ms: None,
        }
    }

    fn status(o: &StateOwner) -> darkbot_bsc_record::Record {
        darkbot_bsc_record::decode(&e::SCHEMA, &o.draft().encode().unwrap()).unwrap()
    }

    /// The follower's events and the launch sightings reach the one owner;
    /// a checkpoint restores the window after a restart (quarantined, the
    /// follower resuming after it); a corrupt checkpoint is refused and the
    /// state starts empty; contradicting input is refused and visible.
    #[test]
    fn the_owner_follows_checkpoints_and_restores_its_window() {
        let dir = std::env::temp_dir().join(format!("bsc-c2-owner-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c2-checkpoint.bin");
        let cfg = config(vec![W]);
        let mut o = StateOwner::restore(&cfg, path.clone()).unwrap();
        assert_eq!(o.restore, "NONE");
        assert!(o.watches(&W) && !o.watches(&F));
        assert_eq!(
            o.state.sender(Address::from(W)),
            SenderView::Quarantined {
                reason: SenderReason::Restart
            }
        );
        // A launch candidate seen pending, then included with an own tx.
        let launch = [0x11; 32];
        o.pending(launch, F, 0, 1, 5);
        assert_eq!(
            o.state.tx(&B256::from(launch)).unwrap().state,
            TxState::Candidate
        );
        o.canonical(&block(1, 0xA, 0xA, &[]));
        o.canonical(&block(2, 0xA, 0xA, &[([0x22; 32], W, 7), (launch, F, 0)]));
        o.canonical(&block(3, 0xA, 0xA, &[]));
        assert!(matches!(
            o.state.tx(&B256::from(launch)).unwrap().state,
            TxState::Included {
                block_number: 2,
                ..
            }
        ));
        // Block 3 left the chain; the new chain's block 3 follows.
        o.canonical(&CanonicalEvent::Reorg {
            orphaned: vec![(3, id(3, 0xA))],
            new_head: (3, id(3, 0xB)),
        });
        o.canonical(&block(3, 0xB, 0xA, &[]));
        assert_eq!(
            o.resume_window(),
            vec![(1, id(1, 0xA)), (2, id(2, 0xA)), (3, id(3, 0xB))]
        );
        let r = status(&o);
        assert_eq!(
            (
                r.u64(e::HEAD).unwrap(),
                r.u64(e::BLOCKS).unwrap(),
                r.u64(e::REORGS).unwrap(),
                r.u64(e::INCLUDED).unwrap(),
                r.u64(e::PENDING_OBSERVATIONS).unwrap(),
                r.u64(e::WATCHED).unwrap(),
                r.u64(e::QUARANTINED_SENDERS).unwrap(),
            ),
            (3, 4, 1, 2, 1, 1, 1)
        );
        // The own account is quarantined until a nonce fact reconciles it.
        assert!(!r.bool(e::COHERENT).unwrap());
        assert_eq!(r.opt_text(e::LAST_CONFLICT).unwrap(), None);
        o.checkpoint(super::super::replace);
        assert_eq!(status(&o).u64(e::CHECKPOINT_WRITES).unwrap(), 1);
        let back = StateOwner::restore(&cfg, path.clone()).unwrap();
        assert_eq!((back.restore, back.restored_blocks), ("RESTORED", 3));
        assert_eq!(back.resume_window(), o.resume_window());
        assert!(matches!(
            back.state.tx(&B256::from(launch)).unwrap().state,
            TxState::Included {
                block_number: 2,
                ..
            }
        ));
        assert_eq!(
            status(&back).text(e::CHECKPOINT_RESTORE).unwrap(),
            "RESTORED"
        );
        assert!(matches!(
            back.state.sender(Address::from(W)),
            SenderView::Quarantined { .. }
        ));

        // A block not extending the head is a discontinuity: the chain is
        // dropped and the block re-anchors (never a permanent stall).
        o.canonical(&block(5, 0xC, 0xC, &[]));
        let r = status(&o);
        assert_eq!(
            (
                r.text(e::LAST_CONFLICT).unwrap(),
                r.u64(e::DISCONTINUITIES).unwrap(),
                r.u64(e::REFUSED_INPUTS).unwrap(),
                r.opt_text(e::LAST_REFUSED).unwrap()
            ),
            ("DISCONTINUITY", 1, 0, None)
        );
        assert_eq!(o.state.high_water(), Some((5, B256::from(id(5, 0xC)))));
        // A malformed block (one sender, nonce 4 after 5) is refused; its child
        // re-anchors, and the refusal stays visible as the last refused kind.
        o.canonical(&block(
            6,
            0xC,
            0xC,
            &[([0x61; 32], F, 5), ([0x62; 32], F, 4)],
        ));
        o.canonical(&block(7, 0xC, 0xC, &[]));
        let r = status(&o);
        assert_eq!(
            (
                r.text(e::LAST_CONFLICT).unwrap(),
                r.text(e::LAST_REFUSED).unwrap(),
                r.u64(e::REFUSED_INPUTS).unwrap(),
                r.u64(e::DISCONTINUITIES).unwrap()
            ),
            ("DISCONTINUITY", "MALFORMED_BLOCK", 1, 2)
        );
        assert_eq!(o.state.high_water(), Some((7, B256::from(id(7, 0xC)))));

        let mut bytes = std::fs::read(&path).unwrap();
        bytes[20] ^= 1;
        std::fs::write(&path, &bytes).unwrap();
        let refused = StateOwner::restore(&cfg, path.clone()).unwrap();
        assert_eq!(refused.restore, "REFUSED");
        assert!(refused.resume_window().is_empty());
        assert_eq!(refused.state.high_water(), None);
        assert!(matches!(
            refused.state.sender(Address::from(W)),
            SenderView::Quarantined { .. }
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Without own accounts the state is coherent once anchored; a gap
    /// makes it incoherent until the next block re-anchors it.
    #[test]
    fn without_watched_senders_the_state_is_coherent_on_an_anchored_head() {
        let path = std::env::temp_dir().join(format!("bsc-c2-fresh-{}.bin", std::process::id()));
        let mut o = StateOwner::fresh(&config(Vec::new()), path).unwrap();
        assert!(!o.state.coherent(), "no head yet");
        o.canonical(&block(10, 1, 1, &[]));
        assert!(o.state.coherent());
        o.canonical(&CanonicalEvent::Gap { from: 11, to: 12 });
        assert!(!o.state.coherent());
        assert_eq!(status(&o).u64(e::GAPS).unwrap(), 1);
        o.canonical(&block(13, 1, 9, &[]));
        assert!(o.state.coherent());
        assert_eq!(status(&o).u64(e::HEAD).unwrap(), 13);
    }

    /// The restart quarantine of an own account ends only with a nonce
    /// fact (pinned to a retained block) that the canonical effects
    /// confirm; a contradicting fact is refused and counted.
    #[test]
    fn a_confirmed_nonce_fact_reconciles_the_quarantined_own_account() {
        let path = std::env::temp_dir().join(format!("bsc-c2-fact-{}.bin", std::process::id()));
        let mut o = StateOwner::fresh(&config(vec![W]), path).unwrap();
        o.canonical(&block(1, 0xA, 0xA, &[]));
        o.canonical(&block(2, 0xA, 0xA, &[([0x22; 32], W, 7)]));
        assert!(!o.state.coherent(), "own account quarantined");
        let fact = |next_nonce| CanonicalEvent::NonceFact {
            sender: W,
            next_nonce,
            block_number: 2,
            block_hash: id(2, 0xA),
        };
        o.canonical(&fact(9));
        assert!(!o.state.coherent());
        assert!(matches!(
            o.state.sender(Address::from(W)),
            SenderView::Quarantined { .. }
        ));
        o.canonical(&fact(8));
        assert!(o.state.coherent());
        assert_eq!(
            o.state.sender(Address::from(W)),
            SenderView::Clean { next_nonce: 8 }
        );
        let r = status(&o);
        assert_eq!(
            (
                r.u64(e::NONCE_FACTS).unwrap(),
                r.u64(e::RECONCILIATIONS).unwrap(),
                r.u64(e::RECONCILIATION_REFUSALS).unwrap(),
                r.u64(e::QUARANTINED_SENDERS).unwrap()
            ),
            (2, 1, 1, 0)
        );
    }

    /// The lifecycle the runtime drives: OFF→SHADOW starts a fresh owner,
    /// SHADOW→OFF checkpoints its window and ends it, a second switch-on
    /// starts fresh again (the follower is past the checkpoint), and the
    /// next process start restores the window, quarantined.
    #[test]
    fn switching_on_and_off_checkpoints_and_the_next_start_restores() {
        let dir = std::env::temp_dir().join(format!("bsc-c2-switch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c2-checkpoint.bin");
        let cfg = config(vec![W]);
        let mut owner: Option<StateOwner> = None;
        StateOwner::switch(&mut owner, false, &cfg, &path, super::super::replace).unwrap();
        assert!(
            owner.is_none() && !path.exists(),
            "OFF stays off, writes nothing"
        );
        StateOwner::switch(&mut owner, true, &cfg, &path, super::super::replace).unwrap();
        let o = owner.as_mut().unwrap();
        assert_eq!(o.restore, "NONE");
        for n in 1..=3 {
            o.canonical(&block(n, 0xA, 0xA, &[]));
        }
        StateOwner::switch(&mut owner, true, &cfg, &path, super::super::replace).unwrap();
        assert_eq!(
            owner.as_ref().unwrap().state.high_water().unwrap().0,
            3,
            "kept"
        );
        StateOwner::switch(&mut owner, false, &cfg, &path, super::super::replace).unwrap();
        assert!(owner.is_none());
        StateOwner::switch(&mut owner, true, &cfg, &path, super::super::replace).unwrap();
        assert_eq!(
            owner.as_ref().unwrap().state.high_water(),
            None,
            "fresh, not restored"
        );
        let next = StateOwner::restore(&cfg, path.clone()).unwrap();
        assert_eq!(next.restore, "RESTORED");
        assert_eq!(
            next.resume_window(),
            vec![(1, id(1, 0xA)), (2, id(2, 0xA)), (3, id(3, 0xA))]
        );
        assert!(matches!(
            next.state.sender(Address::from(W)),
            SenderView::Quarantined { .. }
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn watched_accounts_parse_exactly() {
        assert_eq!(
            StateConfig::parse_watched("").unwrap(),
            Vec::<[u8; 20]>::new()
        );
        assert_eq!(
            StateConfig::parse_watched(" 0xffffffffffffffffffffffffffffffffffffffff ,").unwrap(),
            vec![[0xFF; 20]]
        );
        for bad in ["ff", "0xff", "0xgfffffffffffffffffffffffffffffffffffffff"] {
            assert!(StateConfig::parse_watched(bad).is_err(), "{bad}");
        }
        let mut twice = config(vec![W, W]);
        assert!(twice.validate().is_err());
        twice.watched = vec![W];
        assert!(twice.validate().is_ok());
    }

    /// BSC3-039: a write never holds up whoever hands the window over; a
    /// newer window replaces one still waiting (at most one runs, one
    /// waits) and the newest is the file; `idle` returns once nothing
    /// waits or runs, with every result; a failure returns as it failed.
    #[test]
    fn a_checkpoint_is_written_off_the_loop_and_the_newest_wins() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::{Duration, Instant};
        let written = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let started = Arc::new(AtomicBool::new(false));
        let (w, st) = (Arc::clone(&written), Arc::clone(&started));
        let slow: Replace = Arc::new(move |_: &Path, bytes: &[u8]| {
            st.store(true, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(600));
            w.lock().unwrap().push(bytes.to_vec());
            Ok(())
        });
        let writer = CheckpointWriter::spawn(PathBuf::from("/nonexistent/c2"), slow).unwrap();
        let t = Instant::now();
        writer.submit(b"A".to_vec());
        while !started.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }
        writer.submit(b"B".to_vec());
        writer.submit(b"C".to_vec());
        // A liveness margin, not a timing claim: three hand-overs while
        // the first write takes 600 ms.
        assert!(
            t.elapsed() < Duration::from_millis(500),
            "{:?}",
            t.elapsed()
        );
        assert!(writer.finished().is_empty());
        assert_eq!(writer.idle(), [Ok(()), Ok(())]);
        assert_eq!(*written.lock().unwrap(), [b"A".to_vec(), b"C".to_vec()]);
        let failing: Replace = Arc::new(|_: &Path, _: &[u8]| Err("disk full".into()));
        let writer = CheckpointWriter::spawn(PathBuf::from("/nonexistent/c2"), failing).unwrap();
        writer.submit(b"D".to_vec());
        assert_eq!(writer.idle(), [Err("disk full".to_string())]);
    }

    /// BSC3-039 review: the file has one writer at a time. The loop's own
    /// write waits for the writer's (one running, one waiting) and holds
    /// the writer meanwhile, so it is the last; a window waiting when the
    /// writer is dropped is written first; a writer thread that ended
    /// leaves its writes to the caller, counted in order.
    #[test]
    fn the_checkpoint_file_has_one_writer_at_a_time() {
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
        use std::time::Duration;
        let written = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let (writing, overlapped) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        let flagging = {
            let (w, writing, overlapped) = (
                Arc::clone(&written),
                Arc::clone(&writing),
                Arc::clone(&overlapped),
            );
            move |_: &Path, bytes: &[u8]| -> Result<(), String> {
                if writing.swap(true, Ordering::SeqCst) {
                    overlapped.store(true, Ordering::SeqCst);
                }
                thread::sleep(Duration::from_millis(300));
                w.lock().unwrap().push(bytes.to_vec());
                writing.store(false, Ordering::SeqCst);
                Ok(())
            }
        };
        let flagging = Arc::new(flagging);
        let theirs: Replace = flagging.clone();
        let writer = CheckpointWriter::spawn(PathBuf::from("/nonexistent/c2"), theirs).unwrap();
        writer.submit(b"A".to_vec());
        while !writing.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }
        writer.submit(b"B".to_vec());
        writer
            .write_now(
                Path::new("/nonexistent/c2"),
                b"LAST",
                |p: &Path, b: &[u8]| flagging(p, b),
            )
            .unwrap();
        assert!(!overlapped.load(Ordering::SeqCst), "two writes at once");
        assert_eq!(
            *written.lock().unwrap(),
            [b"A".to_vec(), b"B".to_vec(), b"LAST".to_vec()]
        );
        assert_eq!(writer.finished(), [Ok(()), Ok(())]);
        // Dropped while a window waits behind a running write: both written.
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let held = {
            let (w, gate, writing) = (
                Arc::clone(&written),
                Arc::clone(&gate),
                Arc::clone(&writing),
            );
            move |_: &Path, bytes: &[u8]| -> Result<(), String> {
                writing.store(true, Ordering::SeqCst);
                let (open, opened) = &*gate;
                let mut o = open.lock().unwrap();
                while !*o {
                    o = opened.wait(o).unwrap();
                }
                w.lock().unwrap().push(bytes.to_vec());
                Ok(())
            }
        };
        writing.store(false, Ordering::SeqCst);
        let writer =
            CheckpointWriter::spawn(PathBuf::from("/nonexistent/c2"), Arc::new(held)).unwrap();
        writer.submit(b"D".to_vec());
        while !writing.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }
        writer.submit(b"E".to_vec());
        let opener = Arc::clone(&gate);
        let open = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            *opener.0.lock().unwrap() = true;
            opener.1.notify_all();
        });
        drop(writer);
        open.join().unwrap();
        assert_eq!(written.lock().unwrap()[3..], [b"D".to_vec(), b"E".to_vec()]);
        // A writer thread that ended: the next windows are written by the
        // caller, in order.
        let calls = Arc::new(AtomicU32::new(0));
        let dying = {
            let calls = Arc::clone(&calls);
            move |_: &Path, _: &[u8]| -> Result<(), String> {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("the writer thread ends (test)");
                }
                Ok(())
            }
        };
        let writer =
            CheckpointWriter::spawn(PathBuf::from("/nonexistent/c2"), Arc::new(dying)).unwrap();
        writer.submit(b"F".to_vec());
        assert_eq!(writer.idle(), []);
        writer.submit(b"G".to_vec());
        writer.submit(b"H".to_vec());
        assert_eq!(writer.finished(), [Ok(()), Ok(())]);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
}

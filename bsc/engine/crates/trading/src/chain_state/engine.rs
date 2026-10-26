//! The C2 chain-state owner: the one native canonical, pending and
//! reconciliation state (master plan §7 "single C2 chain-state/reconciler
//! owner", §12, §13 C2; PHASENPLAN §10; retirement contract §3).
//!
//! # Model
//!
//! [`ChainState`] is a pure, deterministic state machine. It reads no clock,
//! performs no I/O and uses no randomness; every collection iterates in key
//! order. The same [`ChainInput`]s applied in the same order produce the
//! same state and the same [`Applied`] outputs. The caller journals every
//! input ([`ChainInput::encode`]) and rebuilds the state after a restart by
//! replaying the journal, then calls [`ChainState::quarantine_all`].
//! Instead of journaling every block, the caller may write a periodic
//! [`ChainState::encode_checkpoint`] of the retained canonical window (never
//! on an entry path) and rebuild from it with [`ChainState::restore`]; the
//! follower then re-fetches the blocks after the checkpoint's head or
//! reports them as a gap.
//!
//! # Event order (follower contract)
//!
//! A [`ChainInput::Reorg`] lists the orphaned blocks ascending and is
//! followed by the new chain's blocks `fork + 1 ..= new_head`. A
//! [`ChainInput::ReorgTooDeep`] is followed by the block `new_head`. A
//! [`ChainInput::Gap`] `{from, to}` is followed by the block `to + 1`. A
//! block that is not the one expected is a counted
//! [`Conflict::Discontinuity`] (rule 3); the owner never stays stalled on
//! canonical input.
//!
//! It keeps three things apart (master plan §13 C2):
//!
//! - **Observed candidates.** Every pending sighting of a transaction hash,
//!   with its earliest first-seen, under its `(sender, nonce)` slot. A slot
//!   may hold several candidates; observation order never decides between
//!   them.
//! - **Canonical effects.** The retained canonical chain (at most
//!   `reorg_window` blocks) with the complete transaction list of every
//!   block. Canonical effects exist whether or not a transaction was ever
//!   seen pending.
//! - **Confidence** per sender: `Clean` (its next canonical nonce is known
//!   exactly), `Unknown` or `Quarantined`, always with a reason. Nothing is
//!   ever assumed: an unknown nonce is never zero. EIP-7702 is active on
//!   BSC: an authorization in another transaction can raise an account's
//!   nonce without a transaction of that account. Such a raise shows as a
//!   skipped nonce at the account's next inclusion and costs it `Clean`
//!   (rule 3). For a foreign sender `Clean` therefore means "known from the
//!   last inclusion"; watched senders are assumed to sign no
//!   authorizations (the signer signs legacy transactions only), so a skip
//!   of theirs needs a nonce fact.
//!
//! Proofs ([`InclusionProof`], [`FinalityProof`], [`OrphanProof`],
//! [`ReconciliationProof`]) and [`Replaced`] records are produced only for
//! the watched senders given at construction (the own accounts). Foreign
//! senders are state and counters only.
//!
//! # Rules
//!
//! 1. **First-seen.** A transaction keeps the sighting with the smallest
//!    `at_ns`; a sighting with an equal or later time never replaces it. A
//!    sighting after the transaction was included, replaced or became
//!    uncertain is counted as late and changes nothing else.
//! 2. **Replacement needs canonical evidence.** Only the inclusion of one
//!    candidate in an accepted block marks the others of its slot
//!    `Replaced` with that winner. Nonces only rise, so an accepted
//!    inclusion of a sender's nonce `m` also proves every lower nonce of
//!    that sender consumed at or before that block: its candidates below
//!    `m` become `NonceConsumedNotSeen` and its uncertain transactions below
//!    `m` `ConsumedUnattributed`, with that inclusion as evidence (whether a
//!    candidate consumed its nonce itself, where the state could not see
//!    it, is unknown). Such a resolution emits no proof, reverts if the
//!    evidence block leaves the chain or becomes uncertain, and settles
//!    once it leaves the window final. A candidate is snapshot-eligible
//!    only while unresolved and sighted in the current sighting epoch
//!    (rule 5).
//! 3. **Blocks extend the head.** A block whose parent is the head and
//!    whose number is the next one extends the chain. Any other block
//!    (also, while unanchored, one that is not the expected block) is a
//!    discontinuity of unknown extent: it is reported as the non-refusing
//!    [`Conflict::Discontinuity`], the retained chain is dropped as for a
//!    gap (rule 5, reason `Discontinuity`), and the block becomes the new
//!    anchor. Every block's transaction list must be complete (`index` =
//!    position), free of repeated hashes, and give each sender strictly
//!    increasing nonces; it need not give consecutive ones, because an
//!    EIP-7702 authorization can raise a nonce inside the block. A skipped
//!    nonce is a non-refusing [`Conflict::NonceDiscontinuity`] and costs the
//!    sender `Clean`. A malformed or oversize block, a transaction already
//!    included in a held canonical block, or a nonce whose consumption the
//!    state holds (an inclusion, or a resolution of rules 2 and 8 whose
//!    evidence still holds) is refused: nothing is overwritten and the state
//!    reports no snapshot until an accepted canonical input follows. The
//!    follower's next block then re-anchors through the discontinuity rule.
//!    A nonce below the sender's known next nonce without such a held
//!    consumption is evidence that the known nonce was wrong (an orphaned
//!    EIP-7702 authorization can leave the reorg fallback too high): it is
//!    applied as a non-refusing `NonceDiscontinuity`, and the sender's next
//!    nonce continues from it.
//! 4. **Reorg.** A [`ChainInput::Reorg`] must name exactly a suffix of the
//!    retained chain ending at the head. Its transactions return to
//!    candidates (never dropped), the candidates they had replaced return
//!    as well, watched senders get one [`OrphanProof`] per consumed nonce,
//!    and every sender's next nonce falls back to the smallest orphaned
//!    one. A reorg reaching below the retained window is handled as too
//!    deep. A reorg naming blocks the state does not hold is refused; the
//!    new chain's next block re-anchors (rule 3).
//! 5. **Too deep, gaps, discontinuities.** [`ChainInput::ReorgTooDeep`],
//!    [`ChainInput::Gap`] and a discontinuous block drop the retained
//!    chain: listed blocks are orphaned as in rule 4, the not yet final
//!    transactions of the other retained blocks become `Uncertain`, every
//!    sender becomes `Unknown` (a quarantined one stays quarantined) and
//!    the state is unanchored until the next block, which anchors it. A gap
//!    that does not start right after the head is applied all the same and
//!    reported as the non-refusing [`Conflict::GapMismatch`]. Watched
//!    senders need a [`ChainInput::NonceFact`] to become `Clean` again;
//!    foreign senders become `Clean` with their next inclusion that skips
//!    no nonce. Dropping the chain starts a new sighting epoch: a candidate
//!    sighted before it may have been mined in the heights the state did
//!    not see, so it is snapshot-eligible again only once it is sighted
//!    after it (any sighting counts, also a late or repeated one; its
//!    first-seen is unchanged).
//! 6. **Finality.** A retained block becomes final once the head is
//!    `finality_depth` blocks above it; each watched transaction in it gets
//!    one [`FinalityProof`]. `finality_depth <= reorg_window`, so a block is
//!    final before it leaves the window; a final block that is still in the
//!    window can be orphaned.
//! 7. **Reverted transactions consume their nonce** like successful ones.
//! 8. **Reconciliation.** A `NonceFact` for a watched sender is checked
//!    against the canonical effects: its block must be on the retained
//!    chain (or be the head), the sender's inclusions after it must have
//!    strictly increasing nonces from the fact's next nonce on, and the
//!    resulting next nonce must equal the one the effects imply. A fact
//!    that passes the block check proves every nonce below its next nonce
//!    consumed at or before its block, and every nonce from it on not yet
//!    consumed there: a candidate of a consumed nonce becomes
//!    `NonceConsumedNotSeen`, an uncertain transaction of a consumed nonce
//!    `ConsumedUnattributed` (resolved, no proof: only a receipt could
//!    attribute it), and an
//!    uncertain transaction of an unconsumed nonce at or below the fact's
//!    block was orphaned. The sender becomes `Clean` and a
//!    [`ReconciliationProof`] is emitted only if, in addition, its
//!    inclusions after the fact's block skip no nonce, none of its
//!    transactions is a candidate or uncertain, and every overflowed nonce
//!    of it is consumed. Otherwise it stays `Unknown` or `Quarantined` with
//!    the reason. A fact resolution, like one of rule 2, settles once its
//!    evidence block leaves the window final, and is reverted if that
//!    block leaves the chain or becomes uncertain first.
//! 9. **Bounds.** At most `max_txs` transactions and, per slot, at most
//!    `max_candidates_per_nonce` records besides the canonical one are
//!    held. When full, only *settled* records are evicted: included in a
//!    final block that left the reorg window, replaced by such a
//!    transaction, or resolved (rules 2 and 8) by evidence whose block did.
//!    Otherwise the record is refused with a counted
//!    [`Overflow`] and the sender becomes `Unknown`; nothing is lost
//!    silently. Sender records exist for the watched senders and for
//!    senders with at least one held transaction.
//!
//! # Output order
//!
//! A caller that forwards proofs to the intent lanes applies, per
//! [`Applied`], first `orphans`, then `inclusions`, then `finalities`, then
//! `reconciliations`. One input never needs another order.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use alloy_primitives::{Address, B256};

use super::SnapshotId;
use super::canonical::{
    ExecutionStatus, FinalityProof, InclusionProof, OrphanProof, ReconciliationProof,
};
use super::input::{
    BLOCK_PREFIX_SIZE, CanonicalBlock, CanonicalTx, ChainInput, FirstSeen, PendingObservation,
    TX_SIZE,
};

/// Largest accepted `max_txs`: the encoding of a block of that many
/// transactions still fits the `u32` length of a checkpoint entry.
pub const MAX_TXS_BOUND: usize = (u32::MAX as usize - BLOCK_PREFIX_SIZE) / TX_SIZE;

#[path = "checkpoint.rs"]
mod checkpoint;
#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;

pub use checkpoint::{CHECKPOINT_FORMAT_VERSION, CHECKPOINT_MAGIC, CheckpointError};

/// Every bound of the chain state. The caller fills all of them; there is
/// no default. [`StateLimits::validate`] states the rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateLimits {
    /// Transactions held at most (candidates, included, replaced,
    /// uncertain). Also the largest accepted block.
    pub max_txs: usize,
    /// Records per `(sender, nonce)` slot besides the canonical one.
    pub max_candidates_per_nonce: usize,
    /// Watched senders at most.
    pub max_watched_senders: usize,
    /// Retained canonical blocks: the deepest reorg handled exactly.
    pub reorg_window: u64,
    /// Blocks above a block until it is final.
    pub finality_depth: u64,
}

impl StateLimits {
    /// Every bound is non-zero, `max_txs` is at most [`MAX_TXS_BOUND`], a
    /// slot cannot exceed the whole table, and a block is final before it
    /// leaves the reorg window.
    pub fn validate(&self) -> Result<(), LimitsError> {
        if self.max_txs == 0 {
            return Err(LimitsError::ZeroMaxTxs);
        }
        if self.max_txs > MAX_TXS_BOUND {
            return Err(LimitsError::MaxTxsAboveBound);
        }
        if self.max_candidates_per_nonce == 0 {
            return Err(LimitsError::ZeroMaxCandidatesPerNonce);
        }
        if self.max_candidates_per_nonce > self.max_txs {
            return Err(LimitsError::CandidatesPerNonceAboveMaxTxs);
        }
        if self.max_watched_senders == 0 {
            return Err(LimitsError::ZeroMaxWatchedSenders);
        }
        if self.reorg_window == 0 {
            return Err(LimitsError::ZeroReorgWindow);
        }
        if self.finality_depth == 0 {
            return Err(LimitsError::ZeroFinalityDepth);
        }
        if self.finality_depth > self.reorg_window {
            return Err(LimitsError::FinalityDepthAboveReorgWindow);
        }
        Ok(())
    }
}

/// Reason [`ChainState::new`] refused its configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitsError {
    /// `max_txs` is zero.
    ZeroMaxTxs,
    /// `max_txs` is above [`MAX_TXS_BOUND`].
    MaxTxsAboveBound,
    /// `max_candidates_per_nonce` is zero.
    ZeroMaxCandidatesPerNonce,
    /// `max_candidates_per_nonce` is above `max_txs`.
    CandidatesPerNonceAboveMaxTxs,
    /// `max_watched_senders` is zero.
    ZeroMaxWatchedSenders,
    /// `reorg_window` is zero.
    ZeroReorgWindow,
    /// `finality_depth` is zero.
    ZeroFinalityDepth,
    /// `finality_depth` is above `reorg_window`.
    FinalityDepthAboveReorgWindow,
    /// More watched senders than `max_watched_senders`.
    TooManyWatchedSenders {
        /// Watched senders given.
        count: usize,
        /// The bound.
        max: usize,
    },
    /// A watched sender is listed twice.
    DuplicateWatchedSender(Address),
}

impl core::fmt::Display for LimitsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ZeroMaxTxs => f.write_str("chain state: max_txs is zero"),
            Self::MaxTxsAboveBound => f.write_str("chain state: max_txs is above its bound"),
            Self::ZeroMaxCandidatesPerNonce => {
                f.write_str("chain state: max_candidates_per_nonce is zero")
            }
            Self::CandidatesPerNonceAboveMaxTxs => {
                f.write_str("chain state: max_candidates_per_nonce is above max_txs")
            }
            Self::ZeroMaxWatchedSenders => f.write_str("chain state: max_watched_senders is zero"),
            Self::ZeroReorgWindow => f.write_str("chain state: reorg_window is zero"),
            Self::ZeroFinalityDepth => f.write_str("chain state: finality_depth is zero"),
            Self::FinalityDepthAboveReorgWindow => {
                f.write_str("chain state: finality_depth is above reorg_window")
            }
            Self::TooManyWatchedSenders { count, max } => {
                write!(f, "chain state: {count} watched senders, at most {max}")
            }
            Self::DuplicateWatchedSender(a) => {
                write!(f, "chain state: watched sender {a} listed twice")
            }
        }
    }
}

impl std::error::Error for LimitsError {}

/// Why a sender's next nonce is not known, or why it is quarantined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SenderReason {
    /// No nonce fact (watched) or canonical inclusion (foreign) has fixed
    /// the next nonce yet.
    NotReconciled,
    /// The state holds nothing about this sender.
    NotTracked,
    /// The process restarted ([`ChainState::quarantine_all`]).
    Restart,
    /// Canonical blocks were not followed.
    Gap,
    /// A block did not extend the head (rule 3).
    Discontinuity,
    /// A reorg reached below the retained chain.
    ReorgTooDeep,
    /// A transaction of the sender was refused by a bound.
    Overflow,
    /// A canonical inclusion skipped the known next nonce (an EIP-7702
    /// authorization, or a consumption the state did not see).
    NonceDiscontinuity {
        /// The next nonce the state held.
        expected: u64,
        /// The nonce the block consumed.
        observed: u64,
    },
    /// The sender consumed the largest nonce.
    NonceExhausted,
    /// The state has no head to check a nonce fact against.
    ChainUnanchored,
    /// A nonce fact names a block that is not on the retained chain.
    NonceFactNotCanonical {
        /// The fact's block number.
        block_number: u64,
        /// The fact's block hash.
        block_hash: B256,
    },
    /// A nonce fact contradicts the canonical effects.
    NonceFactMismatch {
        /// The fact's next nonce.
        fact_next: u64,
        /// The next nonce at the fact's block implied by the effects, if
        /// any is implied.
        implied_next: Option<u64>,
    },
    /// A transaction of the sender is still a candidate or uncertain.
    UnresolvedCandidate {
        /// Its nonce (the smallest such).
        nonce: u64,
    },
}

/// Confidence in a sender's next canonical nonce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SenderView {
    /// The next nonce at the head is known exactly.
    Clean {
        /// The next nonce.
        next_nonce: u64,
    },
    /// The next nonce is not known.
    Unknown {
        /// Why.
        reason: SenderReason,
    },
    /// The sender is quarantined after a restart until a nonce fact
    /// reconciles it.
    Quarantined {
        /// The restart, or the latest reason the reconciliation failed.
        reason: SenderReason,
    },
}

/// What proved a nonce consumed for a transaction that was not seen
/// consuming it on the canonical chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsumptionEvidence {
    /// A nonce fact at the evidence block reported a higher next nonce.
    NonceFact,
    /// The sender's transaction `tx` with the higher nonce `nonce` was
    /// included in the evidence block.
    LaterInclusion {
        /// The included transaction.
        tx: B256,
        /// Its nonce.
        nonce: u64,
    },
}

/// State of one held transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxState {
    /// Pending, or returned to pending by a reorg. Not resolved.
    Candidate,
    /// Executed in a block of the canonical chain.
    Included {
        /// Number of the including block.
        block_number: u64,
        /// Hash of the including block.
        block_hash: B256,
        /// Position in that block.
        index: u32,
        /// Execution result; a revert consumed the nonce too.
        status: ExecutionStatus,
        /// The block satisfies the finality depth.
        finalized: bool,
    },
    /// Executed in a block whose canonicity became unknown (gap or reorg
    /// too deep) before it was final. Not resolved.
    Uncertain {
        /// Number of that block.
        block_number: u64,
        /// Hash of that block.
        block_hash: B256,
        /// Position in that block.
        index: u32,
        /// Execution result in that block.
        status: ExecutionStatus,
    },
    /// Another transaction consumed the nonce on the canonical chain.
    Replaced {
        /// The consuming transaction.
        winner: B256,
    },
    /// The nonce was consumed at or before the evidence block, and this
    /// transaction was never seen included on the canonical chain. Whether
    /// another transaction consumed it, or this one did where the state
    /// could not see it (skipped heights), is unknown. Resolved; no proof;
    /// never snapshot-eligible.
    NonceConsumedNotSeen {
        /// What proved the consumption.
        evidence: ConsumptionEvidence,
        /// Number of the evidence block.
        evidence_block_number: u64,
        /// Hash of the evidence block.
        evidence_block_hash: B256,
    },
    /// Executed in a block of unknown canonicity, and the nonce was proven
    /// consumed at or before the evidence block; whether by this
    /// transaction is unknown without a receipt. Resolved; no proof.
    ConsumedUnattributed {
        /// Number of the uncertain block.
        block_number: u64,
        /// Hash of the uncertain block.
        block_hash: B256,
        /// Position in that block.
        index: u32,
        /// Execution result in that block.
        status: ExecutionStatus,
        /// What proved the consumption.
        evidence: ConsumptionEvidence,
        /// Number of the evidence block.
        evidence_block_number: u64,
        /// Hash of the evidence block.
        evidence_block_hash: B256,
    },
}

/// What the state holds about one transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxView {
    /// Transaction hash.
    pub hash: B256,
    /// Sender.
    pub sender: Address,
    /// Nonce.
    pub nonce: u64,
    /// Earliest pending sighting before resolution; `None` for a
    /// transaction known only from a block.
    pub first_seen: Option<FirstSeen>,
    /// Current state.
    pub state: TxState,
    /// Pending sightings applied, including the first.
    pub sightings: u64,
    /// Pending sightings applied while included, replaced or uncertain.
    pub late_sightings: u64,
    /// Times a block holding it left the canonical chain.
    pub orphaned: u64,
}

/// How a nonce of a sender was consumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Consumption {
    /// By `tx` on the canonical chain.
    By {
        /// The consuming transaction.
        tx: B256,
        /// Its block number.
        block_number: u64,
        /// Its block hash.
        block_hash: B256,
        /// Its execution result.
        status: ExecutionStatus,
        /// The block is final.
        finalized: bool,
    },
    /// By `tx` in a block of unknown canonicity.
    Uncertain {
        /// The transaction.
        tx: B256,
        /// Its block number.
        block_number: u64,
        /// Its block hash.
        block_hash: B256,
    },
    /// Consumed at or before the evidence block (a nonce fact, or a later
    /// nonce of the sender included there); by which transaction is unknown
    /// (the slot holds `NonceConsumedNotSeen` or `ConsumedUnattributed`
    /// records).
    Unattributed {
        /// Number of the evidence block.
        evidence_block_number: u64,
        /// Hash of the evidence block.
        evidence_block_hash: B256,
    },
    /// Below the sender's known next nonce, by a transaction the state does
    /// not hold.
    ByUntracked,
    /// Not consumed: at or above the sender's known next nonce.
    Open,
    /// The sender's next nonce is not known.
    Unknown,
}

/// What the state holds about one `(sender, nonce)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonceView {
    /// Unresolved candidates, in the order they were first held.
    pub candidates: Vec<B256>,
    /// Replaced transactions, in the order they were first held.
    pub replaced: Vec<B256>,
    /// `NonceConsumedNotSeen` and `ConsumedUnattributed` transactions, in
    /// the order they were first held.
    pub unattributed: Vec<B256>,
    /// Consumption of the nonce.
    pub consumption: Consumption,
}

/// A candidate of a watched sender lost its nonce to a canonical winner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Replaced {
    /// The sender.
    pub sender: Address,
    /// The nonce.
    pub nonce: u64,
    /// The replaced candidate.
    pub loser: B256,
    /// The canonical transaction.
    pub winner: B256,
}

/// Contradicting or malformed input. [`Conflict::Discontinuity`],
/// [`Conflict::GapMismatch`] and [`Conflict::NonceDiscontinuity`] are
/// reported for inputs that were applied ([`Conflict::refused`] is false).
/// Every other variant means the input was refused and changed nothing but
/// counters; a refused canonical input also withholds the snapshot until an
/// accepted canonical input follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    /// Applied, not refused: the block's parent is not the head or its
    /// number not the next (or, unanchored, it is not the expected block).
    /// The retained chain was dropped and the block is the new anchor.
    Discontinuity {
        /// Block number.
        number: u64,
        /// Block hash.
        hash: B256,
        /// Its parent hash.
        parent_hash: B256,
    },
    /// Incomplete or repeated transaction list, or equal or decreasing
    /// nonces of a sender within the block.
    MalformedBlock {
        /// Block number.
        number: u64,
        /// Block hash.
        hash: B256,
    },
    /// More transactions than `max_txs` in one block.
    OversizeBlock {
        /// Block number.
        number: u64,
        /// Block hash.
        hash: B256,
        /// Its transaction count.
        txs: usize,
    },
    /// A transaction included in an accepted block appears in another one
    /// without an orphaning reorg.
    DoubleInclusion {
        /// The transaction.
        tx: B256,
        /// Block that already holds it.
        included_in: B256,
        /// The refused block.
        block_hash: B256,
    },
    /// A nonce the state knows to be consumed is consumed again.
    NonceConsumedTwice {
        /// The sender.
        sender: Address,
        /// The nonce.
        nonce: u64,
        /// The held transaction that consumed it; `None` when a nonce fact
        /// or the sender's known next nonce proves the consumption.
        consumed_by: Option<B256>,
        /// The second transaction.
        tx: B256,
    },
    /// A transaction hash with another sender or nonce than recorded.
    IdentityMismatch {
        /// The transaction.
        tx: B256,
    },
    /// The reorg does not name a suffix of the retained chain ending at the
    /// head, or the state has no head.
    ReorgMismatch,
    /// Applied, not refused: the gap does not start right after the head
    /// (or at the block an unanchored state expected).
    GapMismatch {
        /// First block of the gap.
        from: u64,
        /// Last block of the gap.
        to: u64,
    },
    /// `from` is above `to`, or `to` is the largest block number.
    MalformedGap {
        /// First block of the gap.
        from: u64,
        /// Last block of the gap.
        to: u64,
    },
    /// Applied, not refused: an accepted block skipped the sender's known
    /// next nonce, across blocks or inside one (EIP-7702).
    NonceDiscontinuity {
        /// The sender.
        sender: Address,
        /// The next nonce the state held.
        expected: u64,
        /// The consumed nonce.
        observed: u64,
    },
}

impl Conflict {
    /// Whether the input was refused.
    pub fn refused(&self) -> bool {
        !matches!(
            self,
            Self::Discontinuity { .. } | Self::GapMismatch { .. } | Self::NonceDiscontinuity { .. }
        )
    }
}

/// Which bound refused a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverflowKind {
    /// `max_txs`, with no settled record to evict.
    Txs,
    /// `max_candidates_per_nonce`.
    CandidatesPerNonce,
}

/// A transaction the state could not hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Overflow {
    /// The bound.
    pub kind: OverflowKind,
    /// Sender of the transaction.
    pub sender: Address,
    /// Its nonce.
    pub nonce: u64,
    /// The transaction.
    pub tx: B256,
}

/// Everything one [`ChainState::apply`] produced. See the module
/// documentation for the order in which proofs are forwarded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// Watched transactions included in an accepted block, in block order.
    pub inclusions: Vec<InclusionProof>,
    /// Watched nonces whose consuming block became final.
    pub finalities: Vec<FinalityProof>,
    /// Watched nonces whose consuming block left the canonical chain.
    pub orphans: Vec<OrphanProof>,
    /// Watched senders reconciled by a nonce fact.
    pub reconciliations: Vec<ReconciliationProof>,
    /// Watched candidates replaced by a canonical winner.
    pub replaced: Vec<Replaced>,
    /// Contradicting or malformed input.
    pub conflicts: Vec<Conflict>,
    /// Records refused by a bound.
    pub overflow: Vec<Overflow>,
}

/// Counters of everything applied, refused and held. Every refused,
/// overflowed, late or unknown item is counted here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StateCounters {
    /// Inputs applied, refused or not.
    pub inputs: u64,
    /// Applied effects; the snapshot cursor.
    pub effects: u64,
    /// Pending observations applied.
    pub pending_observations: u64,
    /// Repeated sightings that kept the first-seen.
    pub repeat_sightings: u64,
    /// Sightings that moved the first-seen to an earlier time.
    pub earlier_first_seen: u64,
    /// Sightings of included, replaced or uncertain transactions.
    pub late_sightings: u64,
    /// New pending transactions below the sender's known next nonce with
    /// no held consumer: counted, not held.
    pub stale_pending: u64,
    /// Candidates that joined a slot already holding a candidate.
    pub competing_candidates: u64,
    /// Blocks accepted.
    pub blocks: u64,
    /// Transactions included by accepted blocks.
    pub included: u64,
    /// Of those, reverted.
    pub reverted: u64,
    /// Candidates replaced by a canonical winner.
    pub replaced: u64,
    /// Exact reorgs applied.
    pub reorgs: u64,
    /// Reorgs that reached below the retained chain.
    pub reorgs_beyond_window: u64,
    /// `ReorgTooDeep` inputs applied.
    pub reorgs_too_deep: u64,
    /// Listed orphaned blocks the state did not hold.
    pub unmatched_orphaned_blocks: u64,
    /// Transactions whose block left the canonical chain.
    pub orphaned_txs: u64,
    /// Gaps applied.
    pub gaps: u64,
    /// Blocks that did not extend the head and re-anchored the state.
    pub discontinuities: u64,
    /// Transactions that became uncertain.
    pub uncertain_marked: u64,
    /// Blocks that became final.
    pub finalized_blocks: u64,
    /// Transactions whose block became final.
    pub finalized_txs: u64,
    /// Nonce facts applied.
    pub nonce_facts: u64,
    /// Nonce facts for senders that are not watched; ignored.
    pub ignored_nonce_facts: u64,
    /// Reconciliations completed.
    pub reconciliations: u64,
    /// Nonce facts that did not reconcile.
    pub reconciliation_refusals: u64,
    /// Candidates resolved `NonceConsumedNotSeen` (rules 2 and 8).
    pub consumed_not_seen: u64,
    /// Uncertain transactions resolved `ConsumedUnattributed`.
    pub consumed_unattributed: u64,
    /// Of both, resolved by a later nonce's inclusion (rule 2) rather than
    /// a nonce fact.
    pub resolved_by_later_inclusion: u64,
    /// Resolutions reverted because their evidence block left the chain or
    /// became uncertain.
    pub resolutions_reverted: u64,
    /// Restart quarantines.
    pub quarantines: u64,
    /// Conflicts, refused or not.
    pub conflicts: u64,
    /// Inputs refused by a conflict.
    pub refused_inputs: u64,
    /// Records refused by `max_txs`.
    pub overflow_txs: u64,
    /// Records refused by `max_candidates_per_nonce`.
    pub overflow_candidates: u64,
    /// Settled records evicted.
    pub evicted: u64,
    /// Blocks replayed from a checkpoint ([`ChainState::restore`]).
    pub restored_blocks: u64,
    /// Gauge: transactions held.
    pub held_txs: u64,
    /// Gauge: of those, candidates.
    pub held_candidates: u64,
    /// Gauge: of those, uncertain.
    pub held_uncertain: u64,
    /// Gauge: sender records held.
    pub held_senders: u64,
    /// Gauge: senders that are `Unknown`.
    pub unknown_senders: u64,
    /// Gauge: senders that are `Quarantined`.
    pub quarantined_senders: u64,
    /// Gauge: retained canonical blocks.
    pub retained_blocks: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Confidence {
    Clean,
    Unknown(SenderReason),
    Quarantined(SenderReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SenderRecord {
    watched: bool,
    /// Held transactions of this sender.
    refs: usize,
    /// Exact next nonce at the head implied by canonical effects and nonce
    /// facts, when derivable. `Clean` implies `Some`.
    implied: Option<u64>,
    confidence: Confidence,
    /// Largest nonce of a refused (overflowed) transaction not yet shown
    /// consumed by a held inclusion.
    lost_max: Option<u64>,
}

impl SenderRecord {
    fn new(watched: bool) -> Self {
        Self {
            watched,
            refs: 0,
            implied: None,
            confidence: Confidence::Unknown(SenderReason::NotReconciled),
            lost_max: None,
        }
    }

    /// Loses `Clean`; a quarantine stays a quarantine.
    fn degrade(&mut self, reason: SenderReason) {
        self.confidence = match self.confidence {
            Confidence::Quarantined(_) => Confidence::Quarantined(reason),
            Confidence::Clean | Confidence::Unknown(_) => Confidence::Unknown(reason),
        };
    }

    /// A refused reconciliation: the reason of a sender that is not clean
    /// is updated; a clean sender stays clean.
    fn note_refusal(&mut self, reason: SenderReason) {
        if self.confidence != Confidence::Clean {
            self.degrade(reason);
        }
    }

    fn view(&self) -> SenderView {
        match (self.confidence, self.implied) {
            (Confidence::Clean, Some(next_nonce)) => SenderView::Clean { next_nonce },
            // Unreachable by construction; fail closed.
            (Confidence::Clean, None) => SenderView::Unknown {
                reason: SenderReason::NotReconciled,
            },
            (Confidence::Unknown(reason), _) => SenderView::Unknown { reason },
            (Confidence::Quarantined(reason), _) => SenderView::Quarantined { reason },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TxRecord {
    sender: Address,
    nonce: u64,
    first_seen: Option<FirstSeen>,
    sightings: u64,
    late_sightings: u64,
    orphaned: u64,
    state: TxState,
    /// Evictable: final and out of the reorg window, or replaced by such a
    /// transaction.
    settled: bool,
    /// Sighting epoch of the latest pending sighting; `None` if never
    /// sighted pending.
    seen_epoch: Option<u64>,
}

/// One transaction of a retained block, complete enough to re-encode the
/// block for a checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BlockTx {
    hash: B256,
    index: u32,
    sender: Address,
    nonce: u64,
    status: ExecutionStatus,
    /// The state holds a record of it (it did not overflow).
    recorded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BlockRecord {
    number: u64,
    hash: B256,
    parent_hash: B256,
    timestamp_ms: u64,
    /// The block's complete transaction list, overflowed ones included.
    txs: Vec<BlockTx>,
    finalized: bool,
}

impl BlockRecord {
    /// The complete block as it was accepted.
    fn to_input(&self) -> CanonicalBlock {
        CanonicalBlock {
            number: self.number,
            hash: self.hash,
            parent_hash: self.parent_hash,
            timestamp_ms: self.timestamp_ms,
            txs: self
                .txs
                .iter()
                .map(|tx| CanonicalTx {
                    hash: tx.hash,
                    index: tx.index,
                    sender: tx.sender,
                    nonce: tx.nonce,
                    status: tx.status,
                })
                .collect(),
        }
    }
}

/// The block an unanchored state accepts next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Expect {
    number: u64,
    hash: Option<B256>,
}

/// The single canonical/pending state and reconciliation owner (module
/// documentation).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainState {
    chain_id: u64,
    limits: StateLimits,
    /// Watched senders, ascending.
    watched: Vec<Address>,
    head: Option<(u64, B256)>,
    expect: Option<Expect>,
    /// A canonical input was refused since the last accepted one.
    diverged: bool,
    /// Retained canonical chain, ascending, ending at the head.
    blocks: VecDeque<BlockRecord>,
    txs: BTreeMap<B256, TxRecord>,
    slots: BTreeMap<(Address, u64), Vec<B256>>,
    senders: BTreeMap<Address, SenderRecord>,
    /// Settled records in the order they settled; eviction order.
    settle_queue: VecDeque<B256>,
    /// Sighting epoch; every dropped chain starts a new one (rule 5).
    epoch: u64,
    /// Resolutions (rules 2 and 8) by the evidence block they rest on, while
    /// that block is retained.
    resolutions: BTreeMap<B256, Vec<B256>>,
    /// The open records (`Candidate`, `Uncertain`) by sender and nonce: what
    /// an inclusion or a nonce fact may resolve, without visiting a
    /// sender's settled history.
    open: BTreeSet<(Address, u64, B256)>,
    /// Records `resolve_consumed` examined; a test-only cost probe.
    #[cfg(test)]
    resolve_visits: u64,
    counters: StateCounters,
}

impl ChainState {
    /// An empty, unanchored state for `chain_id`. Every watched sender is
    /// `Unknown` until a nonce fact reconciles it.
    pub fn new(
        chain_id: u64,
        limits: StateLimits,
        watched: &[Address],
    ) -> Result<Self, LimitsError> {
        limits.validate()?;
        if watched.len() > limits.max_watched_senders {
            return Err(LimitsError::TooManyWatchedSenders {
                count: watched.len(),
                max: limits.max_watched_senders,
            });
        }
        let mut senders = BTreeMap::new();
        for sender in watched {
            if senders.insert(*sender, SenderRecord::new(true)).is_some() {
                return Err(LimitsError::DuplicateWatchedSender(*sender));
            }
        }
        Ok(Self {
            chain_id,
            limits,
            watched: senders.keys().copied().collect(),
            head: None,
            expect: None,
            diverged: false,
            blocks: VecDeque::new(),
            txs: BTreeMap::new(),
            slots: BTreeMap::new(),
            senders,
            settle_queue: VecDeque::new(),
            epoch: 0,
            resolutions: BTreeMap::new(),
            open: BTreeSet::new(),
            #[cfg(test)]
            resolve_visits: 0,
            counters: StateCounters::default(),
        })
    }

    /// Chain of this state.
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// The bounds of this state.
    pub fn limits(&self) -> StateLimits {
        self.limits
    }

    /// Applies one input. Deterministic: see the module documentation.
    pub fn apply(&mut self, input: &ChainInput) -> Applied {
        let mut out = Applied::default();
        bump(&mut self.counters.inputs);
        match input {
            ChainInput::Pending(obs) => self.apply_pending(obs, &mut out),
            ChainInput::Block(block) => self.apply_block(block, &mut out),
            ChainInput::Reorg { orphaned, new_head } => {
                self.apply_reorg(orphaned, *new_head, &mut out)
            }
            ChainInput::ReorgTooDeep { orphaned, new_head } => {
                bump(&mut self.counters.reorgs_too_deep);
                self.drop_chain(
                    orphaned,
                    Some(Expect {
                        number: new_head.0,
                        hash: Some(new_head.1),
                    }),
                    SenderReason::ReorgTooDeep,
                    &mut out,
                );
            }
            ChainInput::Gap { from, to } => self.apply_gap(*from, *to, &mut out),
            ChainInput::NonceFact {
                sender,
                next_nonce,
                block_number,
                block_hash,
            } => self.apply_nonce_fact(*sender, *next_nonce, *block_number, *block_hash, &mut out),
        }
        for conflict in &out.conflicts {
            bump(&mut self.counters.conflicts);
            if conflict.refused() {
                bump(&mut self.counters.refused_inputs);
            }
        }
        for overflow in &out.overflow {
            match overflow.kind {
                OverflowKind::Txs => bump(&mut self.counters.overflow_txs),
                OverflowKind::CandidatesPerNonce => bump(&mut self.counters.overflow_candidates),
            }
        }
        out
    }

    /// Restart: every watched sender is `Quarantined` until a nonce fact
    /// reconciles it (master plan §7). The canonical effects replayed from
    /// the journal are kept as the basis of that check.
    pub fn quarantine_all(&mut self) {
        for record in self.senders.values_mut() {
            if record.watched {
                record.confidence = Confidence::Quarantined(SenderReason::Restart);
            }
        }
        bump(&mut self.counters.quarantines);
    }

    // -----------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------

    /// What the state holds about transaction `hash`.
    pub fn tx(&self, hash: &B256) -> Option<TxView> {
        self.txs.get(hash).map(|r| TxView {
            hash: *hash,
            sender: r.sender,
            nonce: r.nonce,
            first_seen: r.first_seen,
            state: r.state,
            sightings: r.sightings,
            late_sightings: r.late_sightings,
            orphaned: r.orphaned,
        })
    }

    /// What the state holds about `nonce` of `sender`.
    pub fn nonce(&self, sender: Address, nonce: u64) -> NonceView {
        let mut candidates = Vec::new();
        let mut replaced = Vec::new();
        let mut unattributed = Vec::new();
        let mut consumption = None;
        for hash in self.slot(sender, nonce) {
            let Some(record) = self.txs.get(hash) else {
                continue;
            };
            match record.state {
                TxState::Candidate => candidates.push(*hash),
                TxState::Replaced { .. } => replaced.push(*hash),
                TxState::Included {
                    block_number,
                    block_hash,
                    status,
                    finalized,
                    ..
                } => {
                    consumption = Some(Consumption::By {
                        tx: *hash,
                        block_number,
                        block_hash,
                        status,
                        finalized,
                    })
                }
                TxState::Uncertain {
                    block_number,
                    block_hash,
                    ..
                } => {
                    consumption.get_or_insert(Consumption::Uncertain {
                        tx: *hash,
                        block_number,
                        block_hash,
                    });
                }
                TxState::NonceConsumedNotSeen {
                    evidence_block_number,
                    evidence_block_hash,
                    ..
                }
                | TxState::ConsumedUnattributed {
                    evidence_block_number,
                    evidence_block_hash,
                    ..
                } => {
                    unattributed.push(*hash);
                    consumption.get_or_insert(Consumption::Unattributed {
                        evidence_block_number,
                        evidence_block_hash,
                    });
                }
            }
        }
        let consumption = consumption.unwrap_or_else(|| match self.sender(sender) {
            SenderView::Clean { next_nonce } if nonce < next_nonce => Consumption::ByUntracked,
            SenderView::Clean { .. } => Consumption::Open,
            SenderView::Unknown { .. } | SenderView::Quarantined { .. } => Consumption::Unknown,
        });
        NonceView {
            candidates,
            replaced,
            unattributed,
            consumption,
        }
    }

    /// Confidence in the next canonical nonce of `sender`.
    pub fn sender(&self, sender: Address) -> SenderView {
        self.senders.get(&sender).map_or(
            SenderView::Unknown {
                reason: SenderReason::NotTracked,
            },
            SenderRecord::view,
        )
    }

    /// Whether the state is coherent: the head is known (not at start, not
    /// after a gap or a reorg too deep), no canonical input was refused since
    /// the last accepted one, and every watched sender is `Clean`.
    /// [`ChainState::snapshot_id`] returns a snapshot exactly when this
    /// holds and its candidate is a held, unresolved candidate.
    pub fn coherent(&self) -> bool {
        self.head.is_some()
            && !self.diverged
            && self.watched.iter().all(|sender| {
                self.senders
                    .get(sender)
                    .is_some_and(|r| r.confidence == Confidence::Clean)
            })
    }

    /// The retained canonical window, ascending: the follower's resume
    /// window after a restore.
    pub fn retained(&self) -> Vec<(u64, B256)> {
        self.blocks.iter().map(|b| (b.number, b.hash)).collect()
    }

    /// Identity of the current coherent snapshot for `pending_candidate`.
    ///
    /// `None` unless [`ChainState::coherent`], or when `pending_candidate`
    /// is not a held, unresolved candidate sighted in the current sighting
    /// epoch: after a gap, a reorg too deep or a discontinuity, a candidate
    /// is eligible again only once sighted after it (rule 5), and a
    /// candidate whose nonce a later inclusion or a nonce fact proved
    /// consumed never is (rule 2).
    /// The cursor counts applied effects: accepted blocks, reorgs, gaps,
    /// competing candidates and orphans proven by nonce facts; pending
    /// sightings of uncontested transactions do not move it.
    pub fn snapshot_id(
        &self,
        pending_candidate: B256,
        policy_generation: u64,
        codehash_generation: u64,
    ) -> Option<SnapshotId> {
        if !self.coherent() {
            return None;
        }
        let (_, canonical_block_hash) = self.head?;
        let candidate = self.txs.get(&pending_candidate)?;
        if candidate.state != TxState::Candidate || candidate.seen_epoch != Some(self.epoch) {
            return None;
        }
        Some(SnapshotId {
            canonical_block_hash,
            ordered_state_cursor: self.counters.effects,
            pending_candidate,
            policy_generation,
            codehash_generation,
        })
    }

    /// The head of the retained canonical chain, if anchored.
    pub fn high_water(&self) -> Option<(u64, B256)> {
        self.head
    }

    /// All counters and gauges.
    pub fn counters(&self) -> StateCounters {
        let mut c = self.counters;
        c.held_txs = len_u64(self.txs.len());
        for record in self.txs.values() {
            match record.state {
                TxState::Candidate => bump(&mut c.held_candidates),
                TxState::Uncertain { .. } => bump(&mut c.held_uncertain),
                TxState::Included { .. }
                | TxState::Replaced { .. }
                | TxState::NonceConsumedNotSeen { .. }
                | TxState::ConsumedUnattributed { .. } => {}
            }
        }
        c.held_senders = len_u64(self.senders.len());
        for record in self.senders.values() {
            match record.confidence {
                Confidence::Clean => {}
                Confidence::Unknown(_) => bump(&mut c.unknown_senders),
                Confidence::Quarantined(_) => bump(&mut c.quarantined_senders),
            }
        }
        c.retained_blocks = len_u64(self.blocks.len());
        c
    }

    // -----------------------------------------------------------------
    // Pending
    // -----------------------------------------------------------------

    fn apply_pending(&mut self, obs: &PendingObservation, out: &mut Applied) {
        bump(&mut self.counters.pending_observations);
        if let Some(record) = self.txs.get_mut(&obs.hash) {
            if record.sender != obs.sender || record.nonce != obs.nonce {
                out.conflicts
                    .push(Conflict::IdentityMismatch { tx: obs.hash });
                return;
            }
            bump(&mut record.sightings);
            // Sighted in this epoch, for snapshot eligibility only (rule 5).
            record.seen_epoch = Some(self.epoch);
            if record.state != TxState::Candidate {
                bump(&mut record.late_sightings);
                bump(&mut self.counters.late_sightings);
                return;
            }
            match record.first_seen {
                Some(first) if obs.first_seen.at_ns < first.at_ns => {
                    record.first_seen = Some(obs.first_seen);
                    bump(&mut self.counters.earlier_first_seen);
                }
                Some(_) => bump(&mut self.counters.repeat_sightings),
                None => record.first_seen = Some(obs.first_seen),
            }
            return;
        }

        let key = (obs.sender, obs.nonce);
        let winner = self.slot_winner(key);
        if winner.is_none() {
            let below_next = self
                .senders
                .get(&obs.sender)
                .and_then(|r| r.implied)
                .is_some_and(|next| obs.nonce < next);
            if below_next {
                bump(&mut self.counters.stale_pending);
                return;
            }
        }
        let others = self
            .slot(obs.sender, obs.nonce)
            .iter()
            .filter(|h| Some(**h) != winner.map(|w| w.0))
            .count();
        if others >= self.limits.max_candidates_per_nonce {
            self.overflow(
                OverflowKind::CandidatesPerNonce,
                obs.sender,
                obs.nonce,
                obs.hash,
                out,
            );
            return;
        }
        if !self.make_room() {
            self.overflow(OverflowKind::Txs, obs.sender, obs.nonce, obs.hash, out);
            return;
        }
        let (state, settled) = match winner {
            Some((winner, settled)) => (TxState::Replaced { winner }, settled),
            None => (TxState::Candidate, false),
        };
        let competing = winner.is_none()
            && self.slot(obs.sender, obs.nonce).iter().any(|h| {
                self.txs
                    .get(h)
                    .is_some_and(|r| r.state == TxState::Candidate)
            });
        self.insert_record(
            obs.hash,
            TxRecord {
                sender: obs.sender,
                nonce: obs.nonce,
                first_seen: Some(obs.first_seen),
                sightings: 1,
                late_sightings: 0,
                orphaned: 0,
                state,
                settled: false,
                seen_epoch: Some(self.epoch),
            },
        );
        if settled {
            self.settle(obs.hash);
        }
        if let TxState::Replaced { winner } = state {
            bump(&mut self.counters.replaced);
            if self.is_watched(obs.sender) {
                out.replaced.push(Replaced {
                    sender: obs.sender,
                    nonce: obs.nonce,
                    loser: obs.hash,
                    winner,
                });
            }
        }
        if competing {
            bump(&mut self.counters.competing_candidates);
            bump(&mut self.counters.effects);
        }
    }

    // -----------------------------------------------------------------
    // Canonical blocks
    // -----------------------------------------------------------------

    fn apply_block(&mut self, block: &CanonicalBlock, out: &mut Applied) {
        let linked = match (self.head, self.expect) {
            (Some((number, hash)), _) => {
                block.parent_hash == hash && number.checked_add(1) == Some(block.number)
            }
            (None, Some(expect)) => {
                block.number == expect.number && expect.hash.is_none_or(|h| h == block.hash)
            }
            (None, None) => true,
        };
        // A block that does not extend the head is a discontinuity of
        // unknown extent (rule 3): it is checked against the state as it
        // will be once the retained chain is dropped.
        let dropping = !linked && self.head.is_some();
        if let Some(conflict) = self.check_block(block, dropping) {
            self.refuse(conflict, out);
            return;
        }
        if !linked {
            out.conflicts.push(Conflict::Discontinuity {
                number: block.number,
                hash: block.hash,
                parent_hash: block.parent_hash,
            });
            bump(&mut self.counters.discontinuities);
            if dropping {
                self.drop_chain(&[], None, SenderReason::Discontinuity, out);
            } else {
                // Unanchored already: nothing is retained to drop.
                bump(&mut self.counters.effects);
            }
            self.expect = None;
        }
        let mut record = BlockRecord {
            number: block.number,
            hash: block.hash,
            parent_hash: block.parent_hash,
            timestamp_ms: block.timestamp_ms,
            txs: Vec::with_capacity(block.txs.len()),
            finalized: false,
        };
        for tx in &block.txs {
            let recorded = self.include_tx(block.hash, block.number, tx, out);
            record.txs.push(BlockTx {
                hash: tx.hash,
                index: tx.index,
                sender: tx.sender,
                nonce: tx.nonce,
                status: tx.status,
                recorded,
            });
        }
        self.blocks.push_back(record);
        self.head = Some((block.number, block.hash));
        self.expect = None;
        self.diverged = false;
        bump(&mut self.counters.blocks);
        bump(&mut self.counters.effects);
        self.finalize(out);
        self.trim_window();
    }

    /// Every reason to refuse `block`, checked before anything changes.
    ///
    /// With `dropping` the records are judged as they will be once the
    /// retained chain is dropped (rule 5): an inclusion in a retained block
    /// that is not final is then uncertain, a fact resolution at such a
    /// block is reverted, and no sender has a known next nonce.
    fn check_block(&self, block: &CanonicalBlock, dropping: bool) -> Option<Conflict> {
        if block.txs.len() > self.limits.max_txs {
            return Some(Conflict::OversizeBlock {
                number: block.number,
                hash: block.hash,
                txs: block.txs.len(),
            });
        }
        let malformed = Some(Conflict::MalformedBlock {
            number: block.number,
            hash: block.hash,
        });
        let mut hashes = BTreeSet::new();
        let mut last_nonce: BTreeMap<Address, u64> = BTreeMap::new();
        for (position, tx) in block.txs.iter().enumerate() {
            if usize::try_from(tx.index).ok() != Some(position) || !hashes.insert(tx.hash) {
                return malformed;
            }
            // Strictly increasing, not consecutive: an EIP-7702
            // authorization can raise the nonce inside the block.
            if let Some(previous) = last_nonce.insert(tx.sender, tx.nonce)
                && previous >= tx.nonce
            {
                return malformed;
            }
        }
        for tx in &block.txs {
            let consumed = |consumed_by: Option<B256>| {
                Some(Conflict::NonceConsumedTwice {
                    sender: tx.sender,
                    nonce: tx.nonce,
                    consumed_by,
                    tx: tx.hash,
                })
            };
            if let Some(record) = self.txs.get(&tx.hash) {
                if record.sender != tx.sender || record.nonce != tx.nonce {
                    return Some(Conflict::IdentityMismatch { tx: tx.hash });
                }
                match record.state {
                    TxState::Included { block_hash, .. }
                        if stays_canonical(&record.state, dropping) =>
                    {
                        return Some(Conflict::DoubleInclusion {
                            tx: tx.hash,
                            included_in: block_hash,
                            block_hash: block.hash,
                        });
                    }
                    TxState::Replaced { winner } => {
                        let winner_open = self.txs.get(&winner).is_some_and(|w| {
                            matches!(w.state, TxState::Candidate | TxState::Uncertain { .. })
                                || (matches!(w.state, TxState::Included { .. })
                                    && !stays_canonical(&w.state, dropping))
                        });
                        if !winner_open {
                            return consumed(Some(winner));
                        }
                    }
                    _ if self.resolution_holds(&record.state, dropping) => return consumed(None),
                    _ => {}
                }
            }
            for other in self.slot(tx.sender, tx.nonce) {
                if *other == tx.hash {
                    continue;
                }
                let Some(record) = self.txs.get(other) else {
                    continue;
                };
                if stays_canonical(&record.state, dropping) {
                    return consumed(Some(*other));
                }
                if self.resolution_holds(&record.state, dropping) {
                    return consumed(None);
                }
            }
            // A nonce below the sender's known next nonce without a held
            // consumption is applied (rule 3, `note_inclusion`).
        }
        None
    }

    /// Whether a resolution (`NonceConsumedNotSeen`, `ConsumedUnattributed`)
    /// holds, with `dropping` also once the retained chain is dropped.
    fn resolution_holds(&self, state: &TxState, dropping: bool) -> bool {
        match state {
            TxState::NonceConsumedNotSeen {
                evidence_block_hash,
                ..
            }
            | TxState::ConsumedUnattributed {
                evidence_block_hash,
                ..
            } => {
                !dropping
                    || !self
                        .blocks
                        .iter()
                        .any(|b| b.hash == *evidence_block_hash && !b.finalized)
            }
            _ => false,
        }
    }

    /// Applies one transaction of an accepted block. Returns whether the
    /// state holds a record of it.
    fn include_tx(
        &mut self,
        block_hash: B256,
        block_number: u64,
        tx: &CanonicalTx,
        out: &mut Applied,
    ) -> bool {
        let chain_id = self.chain_id;
        let watched = self.is_watched(tx.sender);
        let slot: Vec<B256> = self.slot(tx.sender, tx.nonce).to_vec();

        // A transaction of this nonce in a block of unknown canonicity: a
        // canonical block consuming the nonce proves that block left the
        // chain.
        for hash in &slot {
            if let Some(record) = self.txs.get_mut(hash)
                && let TxState::Uncertain {
                    block_hash: old, ..
                } = record.state
            {
                record.state = TxState::Candidate;
                bump(&mut record.orphaned);
                bump(&mut self.counters.orphaned_txs);
                if watched {
                    out.orphans
                        .push(OrphanProof::new(chain_id, tx.sender, tx.nonce, old));
                }
                self.sync_open(*hash);
            }
        }

        let recorded = if self.txs.contains_key(&tx.hash) {
            true
        } else if self.make_room() {
            self.insert_record(
                tx.hash,
                TxRecord {
                    sender: tx.sender,
                    nonce: tx.nonce,
                    first_seen: None,
                    sightings: 0,
                    late_sightings: 0,
                    orphaned: 0,
                    state: TxState::Candidate,
                    settled: false,
                    seen_epoch: None,
                },
            );
            true
        } else {
            self.overflow(OverflowKind::Txs, tx.sender, tx.nonce, tx.hash, out);
            false
        };
        if let Some(record) = self.txs.get_mut(&tx.hash) {
            record.state = TxState::Included {
                block_number,
                block_hash,
                index: tx.index,
                status: tx.status,
                finalized: false,
            };
            self.sync_open(tx.hash);
        }

        for hash in self.slot(tx.sender, tx.nonce).to_vec() {
            if hash == tx.hash {
                continue;
            }
            if let Some(record) = self.txs.get_mut(&hash)
                && record.state == TxState::Candidate
            {
                record.state = TxState::Replaced { winner: tx.hash };
                bump(&mut self.counters.replaced);
                if watched {
                    out.replaced.push(Replaced {
                        sender: tx.sender,
                        nonce: tx.nonce,
                        loser: hash,
                        winner: tx.hash,
                    });
                }
                self.sync_open(hash);
            }
        }

        self.note_inclusion(tx.sender, tx.nonce, recorded, out);
        // Nonces only rise: every lower nonce of the sender is consumed at
        // or before this block (rule 2).
        self.resolve_consumed(
            tx.sender,
            tx.nonce,
            ConsumptionEvidence::LaterInclusion {
                tx: tx.hash,
                nonce: tx.nonce,
            },
            (block_number, block_hash),
            true,
        );
        bump(&mut self.counters.included);
        if tx.status == ExecutionStatus::Reverted {
            bump(&mut self.counters.reverted);
        }
        if recorded && watched {
            out.inclusions.push(InclusionProof::new(
                chain_id, tx.sender, tx.nonce, tx.hash, block_hash, tx.status,
            ));
        }
        recorded
    }

    /// The sender's next nonce after a canonical inclusion of `nonce`. A
    /// foreign sender becomes `Clean` by an inclusion that skips no nonce, a
    /// watched one only by a nonce fact; a skip costs either `Clean`.
    fn note_inclusion(&mut self, sender: Address, nonce: u64, recorded: bool, out: &mut Applied) {
        let Some(record) = self.senders.get_mut(&sender) else {
            return;
        };
        let skipped = record.implied.filter(|expected| *expected != nonce);
        if let Some(expected) = skipped {
            // Above: something the state does not hold consumed the nonces
            // in between (an EIP-7702 authorization). Below, with no held
            // consumption (refused before): the known next nonce was wrong,
            // e.g. a reorg fallback past an orphaned authorization.
            out.conflicts.push(Conflict::NonceDiscontinuity {
                sender,
                expected,
                observed: nonce,
            });
            record.degrade(SenderReason::NonceDiscontinuity {
                expected,
                observed: nonce,
            });
        }
        if recorded && record.lost_max == Some(nonce) {
            // The largest overflowed nonce was consumed by a held inclusion.
            record.lost_max = None;
        }
        record.implied = nonce.checked_add(1);
        if record.implied.is_none() {
            record.degrade(SenderReason::NonceExhausted);
        } else if !record.watched && recorded && skipped.is_none() {
            record.confidence = Confidence::Clean;
        }
    }

    /// Marks blocks final that are `finality_depth` below the head.
    fn finalize(&mut self, out: &mut Applied) {
        let Some((head, _)) = self.head else {
            return;
        };
        let chain_id = self.chain_id;
        for position in 0..self.blocks.len() {
            let Some(block) = self.blocks.get(position) else {
                break;
            };
            if block.finalized {
                continue;
            }
            let due = block
                .number
                .checked_add(self.limits.finality_depth)
                .is_some_and(|n| n <= head);
            if !due {
                break;
            }
            let hash = block.hash;
            let txs = block.txs.clone();
            for tx in txs.iter().filter(|t| t.recorded) {
                let watched = self.is_watched(tx.sender);
                let Some(record) = self.txs.get_mut(&tx.hash) else {
                    continue;
                };
                if let TxState::Included {
                    block_hash,
                    finalized,
                    ..
                } = &mut record.state
                    && *block_hash == hash
                    && !*finalized
                {
                    *finalized = true;
                    bump(&mut self.counters.finalized_txs);
                    if watched {
                        out.finalities
                            .push(FinalityProof::new(chain_id, tx.sender, tx.nonce, hash));
                    }
                }
            }
            if let Some(block) = self.blocks.get_mut(position) {
                block.finalized = true;
            }
            bump(&mut self.counters.finalized_blocks);
        }
    }

    /// Blocks that leave the reorg window settle.
    fn trim_window(&mut self) {
        while len_u64(self.blocks.len()) > self.limits.reorg_window {
            let Some(block) = self.blocks.pop_front() else {
                break;
            };
            self.retire_block(&block);
        }
    }

    /// A block leaves the retained chain without being orphaned: a final
    /// block settles; the transactions of a block that is not final become
    /// uncertain and the candidates they had replaced are open again.
    fn retire_block(&mut self, block: &BlockRecord) {
        for tx in &block.txs {
            if block.finalized {
                if tx.recorded {
                    self.settle(tx.hash);
                }
                for loser in self.losers_of(tx.sender, tx.nonce, tx.hash) {
                    self.settle(loser);
                }
                continue;
            }
            if tx.recorded
                && let Some(record) = self.txs.get_mut(&tx.hash)
                && let TxState::Included {
                    block_number,
                    block_hash,
                    index,
                    status,
                    ..
                } = record.state
                && block_hash == block.hash
            {
                record.state = TxState::Uncertain {
                    block_number,
                    block_hash,
                    index,
                    status,
                };
                bump(&mut self.counters.uncertain_marked);
                self.sync_open(tx.hash);
            }
            self.reopen_losers(tx.sender, tx.nonce, tx.hash);
        }
        if block.finalized {
            for hash in self.take_resolutions(block.hash) {
                self.settle(hash);
            }
        } else {
            self.revert_resolutions(block.hash);
        }
    }

    /// The records still resolved by evidence at `evidence_block`, removed
    /// from the index.
    fn take_resolutions(&mut self, evidence_block: B256) -> Vec<B256> {
        let mut hashes = self.resolutions.remove(&evidence_block).unwrap_or_default();
        hashes.retain(|hash| {
            self.txs.get(hash).is_some_and(|r| {
                matches!(
                    r.state,
                    TxState::NonceConsumedNotSeen { evidence_block_hash, .. }
                        | TxState::ConsumedUnattributed { evidence_block_hash, .. }
                        if evidence_block_hash == evidence_block
                )
            })
        });
        hashes
    }

    /// The evidence block left the chain or became uncertain: what it
    /// resolved is open again.
    fn revert_resolutions(&mut self, evidence_block: B256) {
        for hash in self.take_resolutions(evidence_block) {
            let Some(record) = self.txs.get_mut(&hash) else {
                continue;
            };
            record.state = match record.state {
                TxState::ConsumedUnattributed {
                    block_number,
                    block_hash,
                    index,
                    status,
                    ..
                } => TxState::Uncertain {
                    block_number,
                    block_hash,
                    index,
                    status,
                },
                _ => TxState::Candidate,
            };
            bump(&mut self.counters.resolutions_reverted);
            self.sync_open(hash);
        }
    }

    // -----------------------------------------------------------------
    // Reorgs and gaps
    // -----------------------------------------------------------------

    fn apply_reorg(&mut self, orphaned: &[(u64, B256)], new_head: (u64, B256), out: &mut Applied) {
        let Some(head) = self.head else {
            self.refuse(Conflict::ReorgMismatch, out);
            return;
        };
        let (Some(first), Some(last)) = (orphaned.first(), orphaned.last()) else {
            self.refuse(Conflict::ReorgMismatch, out);
            return;
        };
        let contiguous = orphaned
            .windows(2)
            .all(|pair| matches!(pair, [a, b] if a.0.checked_add(1) == Some(b.0)));
        let Some(fork) = first.0.checked_sub(1) else {
            self.refuse(Conflict::ReorgMismatch, out);
            return;
        };
        if *last != head || !contiguous || new_head.0 <= fork {
            self.refuse(Conflict::ReorgMismatch, out);
            return;
        }
        let oldest = match self.blocks.front() {
            Some(block) if first.0 >= block.number => block.number,
            _ => {
                // Below the retained chain: the effects there cannot be
                // undone exactly.
                bump(&mut self.counters.reorgs_beyond_window);
                self.drop_chain(
                    orphaned,
                    Some(Expect {
                        number: first.0,
                        hash: None,
                    }),
                    SenderReason::ReorgTooDeep,
                    out,
                );
                return;
            }
        };
        let mut parent = None;
        for (number, hash) in orphaned {
            let held = number
                .checked_sub(oldest)
                .and_then(|offset| usize::try_from(offset).ok())
                .and_then(|position| self.blocks.get(position));
            match held {
                Some(block) if block.hash == *hash => {
                    if parent.is_none() {
                        parent = Some(block.parent_hash);
                    }
                }
                _ => {
                    self.refuse(Conflict::ReorgMismatch, out);
                    return;
                }
            }
        }
        let Some(parent) = parent else {
            self.refuse(Conflict::ReorgMismatch, out);
            return;
        };

        let mut orphans = Vec::new();
        let mut fallback = BTreeMap::new();
        for _ in 0..orphaned.len() {
            let Some(block) = self.blocks.pop_back() else {
                break;
            };
            self.unwind_block(&block, &mut orphans, &mut fallback);
        }
        orphans.reverse();
        out.orphans.extend(orphans);
        for (sender, nonce) in fallback {
            if let Some(record) = self.senders.get_mut(&sender)
                && record.implied.is_some()
            {
                record.implied = Some(nonce);
            }
        }
        self.head = Some((fork, parent));
        self.expect = None;
        self.diverged = false;
        bump(&mut self.counters.reorgs);
        bump(&mut self.counters.effects);
    }

    /// Undoes the effects of an orphaned block: its transactions and the
    /// candidates they replaced become candidates again. Proofs are pushed
    /// in descending order; `fallback` collects each sender's smallest
    /// orphaned nonce.
    fn unwind_block(
        &mut self,
        block: &BlockRecord,
        orphans: &mut Vec<OrphanProof>,
        fallback: &mut BTreeMap<Address, u64>,
    ) {
        let chain_id = self.chain_id;
        for tx in block.txs.iter().rev() {
            let watched = self.is_watched(tx.sender);
            if tx.recorded
                && let Some(record) = self.txs.get_mut(&tx.hash)
                && matches!(
                    record.state,
                    TxState::Included { block_hash, .. } if block_hash == block.hash
                )
            {
                record.state = TxState::Candidate;
                bump(&mut record.orphaned);
                if watched {
                    orphans.push(OrphanProof::new(chain_id, tx.sender, tx.nonce, block.hash));
                }
                self.sync_open(tx.hash);
            }
            bump(&mut self.counters.orphaned_txs);
            self.reopen_losers(tx.sender, tx.nonce, tx.hash);
            fallback
                .entry(tx.sender)
                .and_modify(|n| *n = (*n).min(tx.nonce))
                .or_insert(tx.nonce);
        }
        self.revert_resolutions(block.hash);
    }

    /// The retained chain is dropped (reorg too deep, gap, discontinuity):
    /// listed blocks are orphaned, the others retire, every sender loses its
    /// nonce, and the state is unanchored, expecting `expect` (any block
    /// anchors it; another one is a counted discontinuity).
    fn drop_chain(
        &mut self,
        orphaned: &[(u64, B256)],
        expect: Option<Expect>,
        reason: SenderReason,
        out: &mut Applied,
    ) {
        let listed: BTreeSet<(u64, B256)> = orphaned.iter().copied().collect();
        let blocks: Vec<BlockRecord> = self.blocks.drain(..).collect();
        let mut matched = 0usize;
        let mut orphans = Vec::new();
        let mut fallback = BTreeMap::new();
        for block in blocks.iter().rev() {
            if listed.contains(&(block.number, block.hash)) {
                matched = matched.saturating_add(1);
                self.unwind_block(block, &mut orphans, &mut fallback);
            } else {
                self.retire_block(block);
            }
        }
        orphans.reverse();
        out.orphans.extend(orphans);
        let unmatched = len_u64(listed.len().saturating_sub(matched));
        self.counters.unmatched_orphaned_blocks = self
            .counters
            .unmatched_orphaned_blocks
            .saturating_add(unmatched);
        for record in self.senders.values_mut() {
            record.implied = None;
            record.degrade(reason);
        }
        self.head = None;
        self.expect = expect;
        self.diverged = false;
        // Candidates sighted so far may have been mined where the state did
        // not see it (rule 5).
        self.epoch = self.epoch.saturating_add(1);
        bump(&mut self.counters.effects);
    }

    fn apply_gap(&mut self, from: u64, to: u64, out: &mut Applied) {
        let Some(next) = to.checked_add(1).filter(|_| from <= to) else {
            self.refuse(Conflict::MalformedGap { from, to }, out);
            return;
        };
        let starts_right = match (self.head, self.expect) {
            (Some((number, _)), _) => number.checked_add(1) == Some(from),
            (None, Some(expect)) => expect.number == from,
            (None, None) => true,
        };
        if !starts_right {
            // Applied all the same: whatever lies between the head and the
            // gap was not followed either.
            out.conflicts.push(Conflict::GapMismatch { from, to });
        }
        bump(&mut self.counters.gaps);
        self.drop_chain(
            &[],
            Some(Expect {
                number: next,
                hash: None,
            }),
            SenderReason::Gap,
            out,
        );
    }

    // -----------------------------------------------------------------
    // Reconciliation
    // -----------------------------------------------------------------

    fn apply_nonce_fact(
        &mut self,
        sender: Address,
        next_nonce: u64,
        block_number: u64,
        block_hash: B256,
        out: &mut Applied,
    ) {
        bump(&mut self.counters.nonce_facts);
        if !self.is_watched(sender) {
            bump(&mut self.counters.ignored_nonce_facts);
            return;
        }
        if self.head.is_none() {
            self.refuse_fact(sender, SenderReason::ChainUnanchored);
            return;
        }
        if !self.on_chain(block_number, block_hash) {
            self.refuse_fact(
                sender,
                SenderReason::NonceFactNotCanonical {
                    block_number,
                    block_hash,
                },
            );
            return;
        }

        self.resolve_uncertain(sender, next_nonce, block_number, out);
        let retained = self.blocks.iter().any(|b| b.hash == block_hash);
        self.resolve_consumed(
            sender,
            next_nonce,
            ConsumptionEvidence::NonceFact,
            (block_number, block_hash),
            retained,
        );

        let mut after = Vec::new();
        let mut before_max: Option<u64> = None;
        for block in &self.blocks {
            for tx in block.txs.iter().filter(|t| t.sender == sender) {
                if block.number > block_number {
                    after.push(tx.nonce);
                } else {
                    before_max = Some(before_max.map_or(tx.nonce, |m| m.max(tx.nonce)));
                }
            }
        }
        let implied_now = self.senders.get(&sender).and_then(|r| r.implied);
        let increasing = after.windows(2).all(|pair| matches!(pair, [a, b] if a < b));
        let starts = after.first().is_none_or(|first| *first >= next_nonce);
        let head_next = match after.last() {
            Some(last) => last.checked_add(1),
            None => Some(next_nonce),
        };
        let consistent = increasing
            && starts
            && before_max.is_none_or(|m| m < next_nonce)
            && head_next.is_some()
            && implied_now.is_none_or(|c| Some(c) == head_next);
        let Some(record) = self.senders.get_mut(&sender) else {
            return;
        };
        let Some(head_next) = head_next.filter(|_| consistent) else {
            let implied_next = after.first().copied().or(implied_now);
            record.implied = None;
            record.degrade(SenderReason::NonceFactMismatch {
                fact_next: next_nonce,
                implied_next,
            });
            bump(&mut self.counters.reconciliation_refusals);
            return;
        };
        record.implied = Some(head_next);

        // A nonce skipped after the fact's block: consistent, but the sender
        // is reconciled only by a fact at or after the skip.
        let mut expected = next_nonce;
        for observed in &after {
            if *observed != expected {
                self.refuse_fact(
                    sender,
                    SenderReason::NonceDiscontinuity {
                        expected,
                        observed: *observed,
                    },
                );
                return;
            }
            expected = observed.saturating_add(1);
        }

        if let Some(nonce) = self.min_unresolved_nonce(sender) {
            self.refuse_fact(sender, SenderReason::UnresolvedCandidate { nonce });
            return;
        }
        let lost = self.senders.get(&sender).and_then(|r| r.lost_max);
        // A lost transaction can no longer run once its nonce is consumed.
        if let Some(lost) = lost
            && lost >= next_nonce
            && self.slot_winner((sender, lost)).is_none()
        {
            self.refuse_fact(sender, SenderReason::Overflow);
            return;
        }
        if let Some(record) = self.senders.get_mut(&sender) {
            record.lost_max = None;
            record.confidence = Confidence::Clean;
        }
        bump(&mut self.counters.reconciliations);
        out.reconciliations
            .push(ReconciliationProof::new(self.chain_id, sender, head_next));
    }

    /// A fact at block `fact_block` with `next_nonce` proves that an
    /// uncertain transaction of `sender` at or below that block whose nonce
    /// is not yet consumed is not canonical: its block left the chain.
    fn resolve_uncertain(
        &mut self,
        sender: Address,
        next_nonce: u64,
        fact_block: u64,
        out: &mut Applied,
    ) {
        let chain_id = self.chain_id;
        let open: Vec<(u64, B256)> = self
            .open
            .range((sender, next_nonce, B256::ZERO)..)
            .take_while(|(s, _, _)| *s == sender)
            .map(|(_, nonce, hash)| (*nonce, *hash))
            .collect();
        let mut resolved = false;
        for (nonce, hash) in open {
            let Some(record) = self.txs.get_mut(&hash) else {
                continue;
            };
            if let TxState::Uncertain {
                block_number,
                block_hash,
                ..
            } = record.state
                && block_number <= fact_block
            {
                // Uncertain to candidate: open either way.
                record.state = TxState::Candidate;
                bump(&mut record.orphaned);
                bump(&mut self.counters.orphaned_txs);
                out.orphans
                    .push(OrphanProof::new(chain_id, sender, nonce, block_hash));
                resolved = true;
            }
        }
        if resolved {
            bump(&mut self.counters.effects);
        }
    }

    /// Evidence that every nonce of `sender` below `below` is consumed at or
    /// before `evidence_block` (rule 2: a later nonce included there; rule
    /// 8: a nonce fact there). A candidate of such a nonce becomes
    /// `NonceConsumedNotSeen`, an uncertain transaction
    /// `ConsumedUnattributed`; both without a proof. While the evidence
    /// block is retained they are indexed under it, to settle or revert
    /// with it; otherwise (a fact at a head that is no longer retained,
    /// hence final) they settle at once.
    fn resolve_consumed(
        &mut self,
        sender: Address,
        below: u64,
        evidence: ConsumptionEvidence,
        evidence_block: (u64, B256),
        evidence_retained: bool,
    ) {
        let (evidence_block_number, evidence_block_hash) = evidence_block;
        // Only the sender's open records below `below`: O(log n + k), never
        // its settled history (review 4c L-1).
        let hashes: Vec<B256> = self
            .open
            .range((sender, 0, B256::ZERO)..(sender, below, B256::ZERO))
            .map(|(_, _, hash)| *hash)
            .collect();
        #[cfg(test)]
        {
            self.resolve_visits = self.resolve_visits.saturating_add(len_u64(hashes.len()));
        }
        let mut resolved = Vec::new();
        for hash in hashes {
            let Some(record) = self.txs.get_mut(&hash) else {
                continue;
            };
            record.state = match record.state {
                TxState::Candidate => {
                    bump(&mut self.counters.consumed_not_seen);
                    TxState::NonceConsumedNotSeen {
                        evidence,
                        evidence_block_number,
                        evidence_block_hash,
                    }
                }
                TxState::Uncertain {
                    block_number,
                    block_hash,
                    index,
                    status,
                } => {
                    bump(&mut self.counters.consumed_unattributed);
                    TxState::ConsumedUnattributed {
                        block_number,
                        block_hash,
                        index,
                        status,
                        evidence,
                        evidence_block_number,
                        evidence_block_hash,
                    }
                }
                _ => continue,
            };
            if matches!(evidence, ConsumptionEvidence::LaterInclusion { .. }) {
                bump(&mut self.counters.resolved_by_later_inclusion);
            }
            self.sync_open(hash);
            resolved.push(hash);
        }
        if resolved.is_empty() {
            return;
        }
        bump(&mut self.counters.effects);
        if evidence_retained {
            self.resolutions
                .entry(evidence_block_hash)
                .or_default()
                .extend(resolved);
        } else {
            for hash in resolved {
                self.settle(hash);
            }
        }
    }

    fn refuse_fact(&mut self, sender: Address, reason: SenderReason) {
        if let Some(record) = self.senders.get_mut(&sender) {
            record.note_refusal(reason);
        }
        bump(&mut self.counters.reconciliation_refusals);
    }

    /// Whether `(number, hash)` is the head or a retained block.
    fn on_chain(&self, number: u64, hash: B256) -> bool {
        self.head == Some((number, hash))
            || self
                .blocks
                .iter()
                .any(|b| b.number == number && b.hash == hash)
    }

    fn min_unresolved_nonce(&self, sender: Address) -> Option<u64> {
        self.open
            .range((sender, 0, B256::ZERO)..)
            .next()
            .filter(|(s, _, _)| *s == sender)
            .map(|(_, nonce, _)| *nonce)
    }

    // -----------------------------------------------------------------
    // Records and bounds
    // -----------------------------------------------------------------

    fn is_watched(&self, sender: Address) -> bool {
        self.watched.binary_search(&sender).is_ok()
    }

    fn slot(&self, sender: Address, nonce: u64) -> &[B256] {
        self.slots.get(&(sender, nonce)).map_or(&[], Vec::as_slice)
    }

    /// The included transaction of a slot and whether it is settled.
    fn slot_winner(&self, key: (Address, u64)) -> Option<(B256, bool)> {
        self.slot(key.0, key.1).iter().find_map(|h| {
            self.txs.get(h).and_then(|r| match r.state {
                TxState::Included { .. } => Some((*h, r.settled)),
                _ => None,
            })
        })
    }

    /// Replaced records of a slot whose winner is `winner`.
    fn losers_of(&self, sender: Address, nonce: u64, winner: B256) -> Vec<B256> {
        self.slot(sender, nonce)
            .iter()
            .copied()
            .filter(|h| {
                self.txs
                    .get(h)
                    .is_some_and(|r| r.state == TxState::Replaced { winner })
            })
            .collect()
    }

    /// Candidates replaced by `winner` are candidates again.
    fn reopen_losers(&mut self, sender: Address, nonce: u64, winner: B256) {
        for loser in self.losers_of(sender, nonce, winner) {
            if let Some(record) = self.txs.get_mut(&loser) {
                record.state = TxState::Candidate;
                self.sync_open(loser);
            }
        }
    }

    fn insert_record(&mut self, hash: B256, record: TxRecord) {
        let sender = record.sender;
        let nonce = record.nonce;
        self.txs.insert(hash, record);
        self.sync_open(hash);
        self.slots.entry((sender, nonce)).or_default().push(hash);
        let sender_record = self
            .senders
            .entry(sender)
            .or_insert_with(|| SenderRecord::new(false));
        sender_record.refs = sender_record.refs.saturating_add(1);
    }

    /// Keeps the open index in step with the record of `hash` after its
    /// state changed: `Candidate` and `Uncertain` are open.
    fn sync_open(&mut self, hash: B256) {
        let Some(record) = self.txs.get(&hash) else {
            return;
        };
        let key = (record.sender, record.nonce, hash);
        if matches!(record.state, TxState::Candidate | TxState::Uncertain { .. }) {
            self.open.insert(key);
        } else {
            self.open.remove(&key);
        }
    }

    fn settle(&mut self, hash: B256) {
        if let Some(record) = self.txs.get_mut(&hash)
            && !record.settled
        {
            record.settled = true;
            self.settle_queue.push_back(hash);
        }
    }

    /// Room for one more record: free capacity, or one settled record
    /// evicted.
    fn make_room(&mut self) -> bool {
        if self.txs.len() < self.limits.max_txs {
            return true;
        }
        while let Some(hash) = self.settle_queue.pop_front() {
            if self.txs.get(&hash).is_some_and(|r| r.settled) {
                self.remove_record(hash);
                bump(&mut self.counters.evicted);
                return true;
            }
        }
        false
    }

    fn remove_record(&mut self, hash: B256) {
        let Some(record) = self.txs.remove(&hash) else {
            return;
        };
        self.open.remove(&(record.sender, record.nonce, hash));
        let key = (record.sender, record.nonce);
        if let Some(slot) = self.slots.get_mut(&key) {
            slot.retain(|h| *h != hash);
            if slot.is_empty() {
                self.slots.remove(&key);
            }
        }
        let remove_sender = match self.senders.get_mut(&record.sender) {
            Some(sender) => {
                sender.refs = sender.refs.saturating_sub(1);
                sender.refs == 0 && !sender.watched
            }
            None => false,
        };
        if remove_sender {
            self.senders.remove(&record.sender);
        }
    }

    /// A refused record: counted, and its sender (if held) is `Unknown`.
    fn overflow(
        &mut self,
        kind: OverflowKind,
        sender: Address,
        nonce: u64,
        tx: B256,
        out: &mut Applied,
    ) {
        out.overflow.push(Overflow {
            kind,
            sender,
            nonce,
            tx,
        });
        if let Some(record) = self.senders.get_mut(&sender) {
            record.degrade(SenderReason::Overflow);
            record.lost_max = Some(record.lost_max.map_or(nonce, |m| m.max(nonce)));
        }
    }

    /// A refused input: counted, and no snapshot until an accepted
    /// canonical input follows.
    fn refuse(&mut self, conflict: Conflict, out: &mut Applied) {
        out.conflicts.push(conflict);
        self.diverged = true;
    }
}

/// Whether `state` is a canonical inclusion, with `dropping` also once the
/// retained chain is dropped (a not yet final one is then uncertain).
fn stays_canonical(state: &TxState, dropping: bool) -> bool {
    matches!(state, TxState::Included { finalized, .. } if !dropping || *finalized)
}

fn bump(counter: &mut u64) {
    *counter = counter.saturating_add(1);
}

fn len_u64(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}

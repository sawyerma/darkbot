//! Economic intents, the single-sender nonce lane and the adapter permits
//! (master plan Unit 4, §7; adoption rows SN06 and SN08).
//!
//! # Model
//!
//! One economic BUY is one intent with one identity: chain, sender, nonce,
//! token and the exact signed transaction. The sender is also the
//! recipient, because the Portal BUY has no recipient argument.
//!
//! Sending that intent through several inclusion adapters is *fan-out of
//! one intent*, never several purchases. The lane holds the one signed
//! byte string of the intent; every adapter writes those bytes, with the
//! one nonce. Nothing in this module can sign, re-sign or assign a second
//! nonce for a redundant submission.
//!
//! Intent state and submission state are separate:
//!
//! ```text
//! intent      Reserved → Prepared → Sending ─┬→ Included ─┐
//!                                            ├→ Reverted ─┼→ Canonical
//!                                            ├→ Replaced ─┘
//!                                            └→ Halted  (outcome unknown)
//!             Included | Reverted | Replaced | Canonical → Orphaned → included again
//!             Halted → Included | Reverted | Replaced
//!             Every step after Sending except Halted needs evidence.
//!
//! submission  NotStarted → Writing → HandedOff → Acked | Rejected
//! (per                       └→ Unknown   (write or flush failed after the
//!  adapter)                                permit was consumed)
//!             NotStarted → Suppressed     (lane stopped before the adapter
//!                                          started)
//! ```
//!
//! # Invariants
//!
//! 1. **One nonce per intent.** Nonces are assigned contiguously by the
//!    single owner of the lane (`&mut self`); there is no shared access,
//!    hence no race and no lock.
//! 2. **Economic dedup for the whole session.** A token is reserved at
//!    most once. The record is never evicted, also not after the intent
//!    resolved or was withdrawn. Hash uniqueness is not dedup.
//! 3. **Dispatch in nonce order.** Only the head of the lane can be
//!    dispatched. The head moves only when canonical evidence shows its
//!    nonce consumed. A blocked follower is counted, not hidden.
//! 4. **One economic admission, then permits.** [`IntentLane::begin_send`]
//!    is consumed once per intent, immediately before egress, and mints one
//!    permit per adapter of the immutable selected set. An adapter consumes
//!    its permit with [`IntentLane::start_write`] before its first transport
//!    write; a permit cannot be consumed twice.
//! 5. **Bytes only against a permit.** The lane holds the one signed byte
//!    string of an intent and releases it through exactly one call: a
//!    successful [`IntentLane::start_write`], which has just moved that
//!    adapter's permit from `NotStarted` to `Writing`. There is no other
//!    way to read the bytes from the lane, in no state.
//! 6. **ACK is not inclusion.** Adapter responses change submission state
//!    only. Included, Reverted, Replaced, Canonical and Orphaned are reached
//!    solely through proofs of the chain-state owner
//!    ([`crate::chain_state::canonical`]).
//! 7. **A failed write is UNKNOWN.** After a permit was consumed, a write or
//!    flush failure means bytes may have left the process. The submission
//!    becomes `Unknown`, the intent `Halted`, the lane stops admitting, and
//!    every permit that has not started is suppressed. Already started
//!    submissions keep their state; bookkeeping cannot cancel them.
//! 8. **No retry, no reuse, no replacement.** There is no API to resend,
//!    to free a nonce that may have been disclosed, or to replace a
//!    transaction. `Replaced` only records that evidence showed another
//!    transaction on the nonce.
//! 9. **Preparations are bound to their snapshot.** A preparation is
//!    accepted only after the final binding check of the signed bytes
//!    against the admitted expectation, and it is dispatched only while the
//!    current snapshot identity (candidate, policy, code, chain state)
//!    equals the one it was prepared for.
//! 10. **Restart is fail-closed.** State lives in memory and is lost with
//!     the process. A lane starts either from an [`ArmedFence`] or
//!     quarantined. A quarantined lane has forgotten the previous run: its
//!     dedup records, its intents and any signed bytes an adapter or relay
//!     may still hold and execute later. A nonce reconciliation alone
//!     cannot make that safe, so this unit offers **no way to release a
//!     restart quarantine**, and none to go around it: an [`ArmedFence`]
//!     has no production constructor until the runner's durable startup
//!     fence exists. Releasing a quarantine needs a stronger recovery proof
//!     built from durably recovered intents, disclosed bytes, pending and
//!     canonical nonce state and the adapters' late-execution semantics;
//!     that proof does not exist yet (runner, C2, Unit 5).
//! 11. **In-process stops can be reconciled.** A lane that stopped while
//!     running still holds its intents. It resumes with a reconciliation
//!     proof, and only when every consumed nonce is `Canonical` and no
//!     dispatched intent is unresolved.
//!
//! # Hot path
//!
//! All storage of the lane is allocated when it is created. No operation
//! locks, blocks or performs I/O. No operation allocates either:
//! [`IntentLane::prepare`], the only one that needs working memory,
//! refuses a scratch buffer smaller than [`PREPARE_SCRATCH_CAPACITY`]
//! instead of growing it. The statement covers this crate's code; the
//! allocation behaviour of the signature library is measured in the
//! integrated runtime (C3). Dedup is a scan over the declared capacity.
//! The lane has no clock: waiting between actionable and permit is
//! measured by the caller around these calls.
//!
//! # Not in this unit
//!
//! Transport, framing and provider behaviour (Unit 5); the durable fence
//! and its I/O, configuration and budgets (runner, Unit 6); canonical,
//! pending and reorg determination and the production of proofs (C2); the
//! recovery proof that could release a restart quarantine (runner, C2,
//! Unit 5); a replacement policy (none is qualified).

mod lane;

#[cfg(test)]
mod tests;

pub use lane::{IntentLane, PREPARE_SCRATCH_CAPACITY};

use alloy_primitives::{Address, B256};

use crate::venues::flap::binding::BindingError;

/// Upper bound of adapters one intent can fan out to.
pub const MAX_ADAPTERS: usize = 8;

/// Upper bound of the declared lane capacity.
pub const MAX_LANE_CAPACITY: usize = 65_536;

/// Opaque index of an inclusion adapter. The lane attaches no provider
/// meaning to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdapterId(u8);

impl AdapterId {
    /// Adapter with index `index`, which must be below [`MAX_ADAPTERS`].
    pub fn new(index: u8) -> Option<Self> {
        if usize::from(index) < MAX_ADAPTERS {
            Some(Self(index))
        } else {
            None
        }
    }

    /// Index of the adapter.
    #[inline]
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

/// The immutable, non-empty set of adapters selected for one intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdapterSet(u8);

impl AdapterSet {
    /// Set of the given adapters. Returns `None` for an empty selection.
    pub fn new(adapters: &[AdapterId]) -> Option<Self> {
        let mut mask = 0u8;
        for adapter in adapters {
            mask |= 1u8 << adapter.0;
        }
        if mask == 0 { None } else { Some(Self(mask)) }
    }

    /// Whether `adapter` is selected.
    #[inline]
    pub fn contains(self, adapter: AdapterId) -> bool {
        self.has_index(adapter.index())
    }

    /// Number of selected adapters.
    #[inline]
    pub fn count(self) -> usize {
        self.0.count_ones() as usize
    }

    #[inline]
    fn has_index(self, index: usize) -> bool {
        index < MAX_ADAPTERS && ((self.0 >> index) & 1) == 1
    }
}

/// State of one economic intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentState {
    /// A nonce is assigned; no signed bytes are bound yet.
    Reserved,
    /// Verified signed bytes are bound; nothing was dispatched.
    Prepared,
    /// The economic admission was consumed and permits were minted.
    Sending,
    /// Evidence: the intent's transaction executed successfully.
    Included,
    /// Evidence: the intent's transaction was included and reverted.
    Reverted,
    /// Evidence: another transaction consumed the nonce.
    Replaced,
    /// Evidence: the consuming block satisfies the acceptance rule.
    Canonical,
    /// Evidence: the consuming block left the canonical chain.
    Orphaned,
    /// A submission outcome is unknown; unresolved until evidence arrives.
    Halted,
}

/// What consumed the nonce of an intent, according to evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The intent's own transaction, successfully.
    Included,
    /// The intent's own transaction, reverted.
    Reverted,
    /// Another transaction.
    Replaced,
}

/// State of one adapter's submission of an intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionState {
    /// The adapter is not part of the selected set.
    NotSelected,
    /// A permit exists and has not been consumed.
    NotStarted,
    /// The permit was consumed; a transport write may be in progress.
    Writing,
    /// The transport handoff completed.
    HandedOff,
    /// The adapter acknowledged the request. Not an inclusion.
    Acked,
    /// The adapter rejected the request. Not a proof of non-execution.
    Rejected,
    /// A write or flush failed after the permit was consumed.
    Unknown,
    /// The permit was withdrawn before the adapter started.
    Suppressed,
}

/// Why the lane stopped admitting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HaltReason {
    /// The lane was created quarantined after a restart.
    RestartQuarantine,
    /// A submission outcome is unknown.
    UnknownSubmission,
    /// An adapter presented another transaction hash than the intent's.
    WrongHash,
    /// Evidence: the intent's transaction reverted.
    Reverted,
    /// Evidence: a consuming block was orphaned.
    Reorg,
    /// Evidence: a transaction the lane did not dispatch consumed a nonce.
    NonceConsumedByOtherTx,
    /// Evidence contradicts the lane's recorded state.
    ConflictingEvidence,
    /// Stopped by the runtime or the operator: timeout, drop, stale state.
    External,
}

/// Reason a lane operation was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneError {
    /// The declared capacity is zero or above [`MAX_LANE_CAPACITY`].
    InvalidCapacity,
    /// The lane is stopped.
    Halted,
    /// The token already has an intent in this session.
    Duplicate,
    /// The declared capacity is used up.
    CapacityExceeded,
    /// No further nonce can be assigned.
    NonceExhausted,
    /// The ticket does not belong to an intent of this lane.
    UnknownTicket,
    /// The operation is not valid in the current state.
    InvalidState,
    /// The head of the lane cannot be dispatched yet.
    NotReady,
    /// Only the newest undisclosed intent can be withdrawn.
    NotTail,
    /// The expectation does not describe this ticket on this lane.
    BindingMismatch,
    /// The signed bytes exceed the per-intent buffer.
    Oversize,
    /// The scratch buffer is smaller than [`PREPARE_SCRATCH_CAPACITY`]; it
    /// is not grown.
    ScratchTooSmall,
    /// The signed bytes are not the expected BUY.
    Binding(BindingError),
    /// The preparation belongs to another snapshot than the current one.
    StalePreparation,
    /// The adapter is not in the selected set of the intent.
    AdapterNotSelected,
    /// The adapter's permit was already consumed.
    PermitConsumed,
    /// The adapter's permit was suppressed.
    PermitSuppressed,
    /// The adapter presented another transaction hash.
    WrongHash,
    /// The evidence is about another chain or sender.
    ForeignEvidence,
    /// The evidence is about a nonce the lane did not assign.
    UnknownNonce,
    /// The evidence contradicts the recorded state.
    ConflictingEvidence,
    /// A disclosed intent is unresolved, or a consumed nonce is not final.
    UnresolvedIntents,
    /// The reconciled nonce differs from the lane's consumed nonces.
    NonceMismatch,
    /// The lane is quarantined after a restart; no proof of this unit can
    /// release it.
    QuarantineNotReleasable,
}

impl core::fmt::Display for LaneError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidCapacity => f.write_str("intent lane: invalid capacity"),
            Self::Halted => f.write_str("intent lane: halted"),
            Self::Duplicate => f.write_str("intent lane: duplicate economic intent"),
            Self::CapacityExceeded => f.write_str("intent lane: capacity exceeded"),
            Self::NonceExhausted => f.write_str("intent lane: nonce exhausted"),
            Self::UnknownTicket => f.write_str("intent lane: unknown ticket"),
            Self::InvalidState => f.write_str("intent lane: invalid state"),
            Self::NotReady => f.write_str("intent lane: head is not ready"),
            Self::NotTail => f.write_str("intent lane: not the newest intent"),
            Self::BindingMismatch => f.write_str("intent lane: expectation does not match"),
            Self::Oversize => f.write_str("intent lane: signed bytes too long"),
            Self::ScratchTooSmall => f.write_str("intent lane: scratch buffer too small"),
            Self::Binding(e) => write!(f, "intent lane: {e}"),
            Self::StalePreparation => f.write_str("intent lane: preparation is stale"),
            Self::AdapterNotSelected => f.write_str("intent lane: adapter not selected"),
            Self::PermitConsumed => f.write_str("intent lane: permit already consumed"),
            Self::PermitSuppressed => f.write_str("intent lane: permit suppressed"),
            Self::WrongHash => f.write_str("intent lane: wrong transaction hash"),
            Self::ForeignEvidence => f.write_str("intent lane: evidence for another lane"),
            Self::UnknownNonce => f.write_str("intent lane: evidence for an unknown nonce"),
            Self::ConflictingEvidence => f.write_str("intent lane: conflicting evidence"),
            Self::UnresolvedIntents => f.write_str("intent lane: unresolved intents"),
            Self::NonceMismatch => f.write_str("intent lane: reconciled nonce mismatch"),
            Self::QuarantineNotReleasable => {
                f.write_str("intent lane: restart quarantine cannot be released")
            }
        }
    }
}

impl std::error::Error for LaneError {}

/// Identity of one reserved intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    chain_id: u64,
    sender: Address,
    nonce: u64,
    token: Address,
}

impl Ticket {
    /// Chain of the lane.
    #[inline]
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Sender of the lane; also the recipient of the tokens.
    #[inline]
    pub fn sender(&self) -> Address {
        self.sender
    }

    /// The one nonce of this intent.
    #[inline]
    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    /// Token of this intent.
    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }
}

/// Result of the one economic admission of an intent: what may be sent
/// and through which adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dispatch {
    /// The dispatched intent.
    pub ticket: Ticket,
    /// Hash of the intent's signed transaction.
    pub tx_hash: B256,
    /// Adapters that hold a permit.
    pub adapters: AdapterSet,
}

/// Counters that keep refused and blocked work visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaneStats {
    /// Reservations refused because the token already had an intent.
    pub duplicates: u64,
    /// Reservations refused because the capacity was used up.
    pub capacity_rejections: u64,
    /// Dispatch attempts refused while an intent waited behind the head.
    pub head_of_line_blocks: u64,
    /// Undisclosed intents that were withdrawn.
    pub withdrawn: u64,
}

/// Attestation that a new fence for this chain and sender was durably
/// established before trading, and that `first_nonce` is the sender's
/// reconciled next nonce. It is consumed by [`IntentLane::armed`]: one
/// attestation arms one lane.
///
/// The fence, its I/O and its recovery check belong to the runner (Unit 6)
/// and do not exist yet. Until they do, this type has **no production
/// constructor**: nothing outside this crate's tests can create an
/// admitting lane, and in particular nothing can replace a restart
/// quarantine by arming a fresh lane. The production constructor will be
/// added together with the durable startup fence and bound to its
/// successful check.
#[derive(Debug, PartialEq, Eq)]
pub struct ArmedFence {
    chain_id: u64,
    sender: Address,
    first_nonce: u64,
}

impl ArmedFence {
    /// Test-only attestation. Production code cannot arm a lane yet.
    /// Also compiled for the tests of dependent crates behind the
    /// `test-support` feature, which is enabled only under
    /// `[dev-dependencies]` and never in a production build.
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_tests(chain_id: u64, sender: Address, first_nonce: u64) -> Self {
        Self {
            chain_id,
            sender,
            first_nonce,
        }
    }
}

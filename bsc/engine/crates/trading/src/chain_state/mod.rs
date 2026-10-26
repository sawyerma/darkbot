//! Chain state: the single C2 canonical/pending/reorg owner
//! ([`engine::ChainState`]), its typed inputs ([`input`]), the proofs it
//! alone constructs ([`canonical`]) and the identity that every
//! state-dependent value carries, so that a quote, a prepared BUY and the
//! state it was computed from can be checked for compatibility (master plan
//! §5, §12).

pub mod canonical;
pub mod engine;
pub mod input;

use alloy_primitives::B256;

/// Identity of one coherent, immutable state snapshot.
///
/// Two values are compatible only when their snapshot identities are equal.
/// A reorg, a candidate replacement, a policy change or a code change
/// produces a different identity and thereby invalidates everything derived
/// from the old one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapshotId {
    /// Hash of the canonical block the snapshot builds on.
    pub canonical_block_hash: B256,
    /// Position in the ordered stream of applied effects; not a block number.
    pub ordered_state_cursor: u64,
    /// Transaction hash of the pending launch candidate the snapshot assumes.
    pub pending_candidate: B256,
    /// Generation of the validated operating policy.
    pub policy_generation: u64,
    /// Generation of the validated deployment code fingerprints.
    pub codehash_generation: u64,
}

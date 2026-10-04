//! Canonical evidence: the interface between the chain-state owner and the
//! intent lane.
//!
//! The chain-state owner (phase C2) is the only component that decides what
//! is canonical, included, final or orphaned. The intent lane never derives
//! such facts itself; it only consumes the proofs defined here and moves
//! its intents accordingly (master plan §7, §12).
//!
//! The proofs have no public constructor. C2 will construct them inside
//! this module tree once it exists; until then no runtime code can create
//! one, so no intent can be marked included, final or reconciled by
//! anything but tests. In particular an adapter ACK cannot be turned into
//! an inclusion.

use alloy_primitives::{Address, B256};

/// Result of executing a transaction in a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// The transaction executed successfully.
    Succeeded,
    /// The transaction was included and reverted. Its nonce is consumed.
    Reverted,
}

/// A transaction of `sender` with `nonce` was executed in a block of the
/// chain that is currently canonical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InclusionProof {
    chain_id: u64,
    sender: Address,
    nonce: u64,
    tx_hash: B256,
    block_hash: B256,
    status: ExecutionStatus,
}

impl InclusionProof {
    /// Chain of the block.
    #[inline]
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Sender of the transaction.
    #[inline]
    pub fn sender(&self) -> Address {
        self.sender
    }

    /// Nonce the transaction consumed.
    #[inline]
    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    /// Hash of the included transaction.
    #[inline]
    pub fn tx_hash(&self) -> B256 {
        self.tx_hash
    }

    /// Hash of the including block.
    #[inline]
    pub fn block_hash(&self) -> B256 {
        self.block_hash
    }

    /// Execution result.
    #[inline]
    pub fn status(&self) -> ExecutionStatus {
        self.status
    }

    #[cfg(test)]
    pub(crate) fn for_tests(
        chain_id: u64,
        sender: Address,
        nonce: u64,
        tx_hash: B256,
        block_hash: B256,
        status: ExecutionStatus,
    ) -> Self {
        Self {
            chain_id,
            sender,
            nonce,
            tx_hash,
            block_hash,
            status,
        }
    }
}

/// The block that consumed `nonce` of `sender` satisfies the reviewed
/// canonical acceptance rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FinalityProof {
    chain_id: u64,
    sender: Address,
    nonce: u64,
    block_hash: B256,
}

impl FinalityProof {
    /// Chain of the block.
    #[inline]
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Sender whose nonce the block consumed.
    #[inline]
    pub fn sender(&self) -> Address {
        self.sender
    }

    /// The consumed nonce.
    #[inline]
    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    /// Hash of the accepted block.
    #[inline]
    pub fn block_hash(&self) -> B256 {
        self.block_hash
    }

    #[cfg(test)]
    pub(crate) fn for_tests(chain_id: u64, sender: Address, nonce: u64, block_hash: B256) -> Self {
        Self {
            chain_id,
            sender,
            nonce,
            block_hash,
        }
    }
}

/// The block that consumed `nonce` of `sender` left the canonical chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrphanProof {
    chain_id: u64,
    sender: Address,
    nonce: u64,
    block_hash: B256,
}

impl OrphanProof {
    /// Chain of the block.
    #[inline]
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Sender whose nonce the block had consumed.
    #[inline]
    pub fn sender(&self) -> Address {
        self.sender
    }

    /// The nonce that is no longer consumed.
    #[inline]
    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    /// Hash of the orphaned block.
    #[inline]
    pub fn block_hash(&self) -> B256 {
        self.block_hash
    }

    #[cfg(test)]
    pub(crate) fn for_tests(chain_id: u64, sender: Address, nonce: u64, block_hash: B256) -> Self {
        Self {
            chain_id,
            sender,
            nonce,
            block_hash,
        }
    }
}

/// Result of a completed canonical, pending and nonce reconciliation for
/// one sender: its next canonical nonce, with no transaction of the sender
/// left pending or unresolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconciliationProof {
    chain_id: u64,
    sender: Address,
    next_nonce: u64,
}

impl ReconciliationProof {
    /// Chain that was reconciled.
    #[inline]
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Sender that was reconciled.
    #[inline]
    pub fn sender(&self) -> Address {
        self.sender
    }

    /// Next nonce of the sender on the canonical chain.
    #[inline]
    pub fn next_nonce(&self) -> u64 {
        self.next_nonce
    }

    #[cfg(test)]
    pub(crate) fn for_tests(chain_id: u64, sender: Address, next_nonce: u64) -> Self {
        Self {
            chain_id,
            sender,
            next_nonce,
        }
    }
}

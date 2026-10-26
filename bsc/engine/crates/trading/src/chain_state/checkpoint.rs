//! Binary checkpoint of the retained canonical window.
//!
//! The runtime does not journal every full block. It writes a periodic
//! checkpoint of the retained canonical window instead (never on an entry
//! path) and, after a crash, rebuilds the state from it with
//! [`ChainState::restore`]; the follower re-fetches the blocks after the
//! checkpoint's head or reports them as a [`ChainInput::Gap`].
//!
//! # Layout (version 1)
//!
//! All integers are little-endian.
//!
//! ```text
//! magic     b"DBC2CKP1"                                   8 bytes
//! format    u8 (CHECKPOINT_FORMAT_VERSION)
//! chain_id  u64
//! count     u64
//! count ×   (len u32 | ChainInput::Block encoding, len bytes), ascending
//! checksum  keccak256 of every byte before it              32 bytes
//! ```
//!
//! Each block is the complete block as it was accepted: every transaction
//! with its index and execution status, also one the state could not hold
//! (overflow), and the header's `timestamp_ms`.
//!
//! # What a restore restores
//!
//! Only the canonical window: the blocks are applied in order to a fresh
//! state (the first one anchors it, as any first block of an unanchored
//! state without an expectation does), and then every watched sender is
//! quarantined ([`ChainState::quarantine_all`]). Pending candidates and
//! their first-seen, records of blocks that already left the window, and
//! uncertain transactions are not part of a checkpoint. The proofs the
//! replayed blocks produce are discarded: after a restart every intent lane
//! is quarantined anyway.
//!
//! Decoding is fail-closed: magic, format version, checksum, chain id,
//! every count and length against the remaining bytes before anything is
//! allocated, trailing bytes, each entry decoding to a block, and each block
//! being accepted as an extension of the one before it.

use alloy_primitives::{Address, keccak256};

use super::super::input::{ChainInput, DecodeError};
use super::{ChainState, Conflict, LimitsError, StateLimits, bump, len_u64};

/// First bytes of every checkpoint.
pub const CHECKPOINT_MAGIC: [u8; 8] = *b"DBC2CKP1";

/// Format version written by [`ChainState::encode_checkpoint`].
pub const CHECKPOINT_FORMAT_VERSION: u8 = 1;

/// magic | format | chain_id | count.
const HEADER_LEN: usize = 8 + 1 + 8 + 8;
const CHECKSUM_LEN: usize = 32;
/// The `u32` length that precedes each entry.
const ENTRY_LEN_SIZE: usize = 4;

/// Reason a checkpoint could not be restored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointError {
    /// The bytes end before a field, an entry or the checksum.
    Truncated,
    /// The first bytes are not [`CHECKPOINT_MAGIC`].
    BadMagic,
    /// The format byte is not [`CHECKPOINT_FORMAT_VERSION`].
    UnsupportedVersion(u8),
    /// The keccak256 checksum does not match.
    ChecksumMismatch,
    /// The checkpoint is of another chain.
    WrongChain {
        /// The chain of the state being restored.
        expected: u64,
        /// The chain in the checkpoint.
        found: u64,
    },
    /// A count does not fit this platform's address space.
    LengthOutOfRange,
    /// Bytes remain between the last entry and the checksum.
    TrailingBytes(usize),
    /// An entry does not decode.
    Entry {
        /// Position of the entry.
        position: u64,
        /// The decoding error.
        error: DecodeError,
    },
    /// An entry decodes to another input than a block.
    NotABlock {
        /// Position of the entry.
        position: u64,
    },
    /// The state refused a block, or the block does not extend the one
    /// before it.
    BlockRefused {
        /// Position of the entry.
        position: u64,
        /// Why.
        conflict: Conflict,
    },
    /// The configuration of the restored state is invalid.
    Limits(LimitsError),
}

impl core::fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => f.write_str("chain checkpoint: truncated"),
            Self::BadMagic => f.write_str("chain checkpoint: bad magic"),
            Self::UnsupportedVersion(v) => {
                write!(f, "chain checkpoint: unsupported version {v}")
            }
            Self::ChecksumMismatch => f.write_str("chain checkpoint: checksum mismatch"),
            Self::WrongChain { expected, found } => {
                write!(f, "chain checkpoint: chain {found}, expected {expected}")
            }
            Self::LengthOutOfRange => f.write_str("chain checkpoint: count out of range"),
            Self::TrailingBytes(n) => write!(f, "chain checkpoint: {n} trailing bytes"),
            Self::Entry { position, error } => {
                write!(f, "chain checkpoint: entry {position}: {error}")
            }
            Self::NotABlock { position } => {
                write!(f, "chain checkpoint: entry {position} is not a block")
            }
            Self::BlockRefused { position, conflict } => {
                write!(
                    f,
                    "chain checkpoint: block {position} refused: {conflict:?}"
                )
            }
            Self::Limits(e) => write!(f, "chain checkpoint: {e}"),
        }
    }
}

impl std::error::Error for CheckpointError {}

impl ChainState {
    /// The retained canonical window as a checkpoint (module documentation).
    /// Not for an entry path: it allocates and hashes the whole window.
    pub fn encode_checkpoint(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&CHECKPOINT_MAGIC);
        out.push(CHECKPOINT_FORMAT_VERSION);
        out.extend_from_slice(&self.chain_id.to_le_bytes());
        out.extend_from_slice(&len_u64(self.blocks.len()).to_le_bytes());
        for block in &self.blocks {
            let bytes = ChainInput::Block(block.to_input()).encode();
            // `StateLimits::validate` bounds `max_txs` so that the length
            // fits; a length that did not would fail the restore, not wrap.
            let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&bytes);
        }
        let checksum = keccak256(&out);
        out.extend_from_slice(checksum.as_slice());
        out
    }

    /// Rebuilds a state from a checkpoint: a fresh state of `chain_id`,
    /// `limits` and `watched`, the checkpoint's blocks applied in order,
    /// then every watched sender quarantined.
    pub fn restore(
        chain_id: u64,
        limits: StateLimits,
        watched: &[Address],
        bytes: &[u8],
    ) -> Result<ChainState, CheckpointError> {
        let mut state =
            ChainState::new(chain_id, limits, watched).map_err(CheckpointError::Limits)?;
        let body_len = bytes
            .len()
            .checked_sub(CHECKSUM_LEN)
            .filter(|n| *n >= HEADER_LEN)
            .ok_or(CheckpointError::Truncated)?;
        let (body, checksum) = bytes
            .split_at_checked(body_len)
            .ok_or(CheckpointError::Truncated)?;
        let mut r = Cursor {
            bytes: body,
            pos: 0,
        };
        if r.take(CHECKPOINT_MAGIC.len())? != CHECKPOINT_MAGIC.as_slice() {
            return Err(CheckpointError::BadMagic);
        }
        let version = r.u8()?;
        if version != CHECKPOINT_FORMAT_VERSION {
            return Err(CheckpointError::UnsupportedVersion(version));
        }
        if keccak256(body).as_slice() != checksum {
            return Err(CheckpointError::ChecksumMismatch);
        }
        let found = r.u64()?;
        if found != chain_id {
            return Err(CheckpointError::WrongChain {
                expected: chain_id,
                found,
            });
        }
        let count = r.u64()?;
        let entries = usize::try_from(count).map_err(|_| CheckpointError::LengthOutOfRange)?;
        let needed = entries
            .checked_mul(ENTRY_LEN_SIZE)
            .ok_or(CheckpointError::LengthOutOfRange)?;
        if needed > r.remaining() {
            return Err(CheckpointError::Truncated);
        }
        for position in 0..count {
            let len = usize::try_from(r.u32()?).map_err(|_| CheckpointError::LengthOutOfRange)?;
            let entry = r.take(len)?;
            let input = ChainInput::decode(entry)
                .map_err(|error| CheckpointError::Entry { position, error })?;
            if !matches!(input, ChainInput::Block(_)) {
                return Err(CheckpointError::NotABlock { position });
            }
            let out = state.apply(&input);
            // Inside a checkpoint every block extends the previous one; a
            // discontinuity there is corruption, not a follower event.
            let broken = out
                .conflicts
                .iter()
                .find(|c| c.refused() || matches!(c, Conflict::Discontinuity { .. }));
            if let Some(conflict) = broken {
                return Err(CheckpointError::BlockRefused {
                    position,
                    conflict: *conflict,
                });
            }
            bump(&mut state.counters.restored_blocks);
        }
        let rest = r.remaining();
        if rest != 0 {
            return Err(CheckpointError::TrailingBytes(rest));
        }
        state.quarantine_all();
        Ok(state)
    }
}

/// Bounds-checked cursor over the checkpoint body.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], CheckpointError> {
        let end = self.pos.checked_add(n).ok_or(CheckpointError::Truncated)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(CheckpointError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], CheckpointError> {
        <[u8; N]>::try_from(self.take(N)?).map_err(|_| CheckpointError::Truncated)
    }

    fn u8(&mut self) -> Result<u8, CheckpointError> {
        Ok(self.array::<1>()?[0])
    }

    fn u32(&mut self) -> Result<u32, CheckpointError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, CheckpointError> {
        Ok(u64::from_le_bytes(self.array()?))
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::B256;

    use super::*;
    use crate::chain_state::canonical::ExecutionStatus;
    use crate::chain_state::engine::{Consumption, SenderReason, SenderView, TxState};
    use crate::chain_state::input::{CanonicalBlock, CanonicalTx, FirstSeen, PendingObservation};

    const CHAIN: u64 = 56;
    const BASE: u64 = 500;

    fn own() -> Address {
        Address::repeat_byte(0x11)
    }

    fn foreign() -> Address {
        Address::repeat_byte(0x21)
    }

    fn t(id: u64) -> B256 {
        let mut b = [0u8; 32];
        b[0] = 0x7c;
        b[24..].copy_from_slice(&id.to_be_bytes());
        B256::from(b)
    }

    fn bh(number: u64, fork: u8) -> B256 {
        let mut b = [0u8; 32];
        b[0] = 0xc0;
        b[1] = fork;
        b[24..].copy_from_slice(&number.to_be_bytes());
        B256::from(b)
    }

    fn limits() -> StateLimits {
        StateLimits {
            max_txs: 64,
            max_candidates_per_nonce: 3,
            max_watched_senders: 2,
            reorg_window: 5,
            finality_depth: 2,
        }
    }

    fn block(
        number: u64,
        fork: u8,
        parent_fork: u8,
        txs: &[(B256, Address, u64, ExecutionStatus)],
    ) -> ChainInput {
        ChainInput::Block(CanonicalBlock {
            number,
            hash: bh(number, fork),
            parent_hash: bh(number - 1, parent_fork),
            timestamp_ms: number * 750 + 3,
            txs: txs
                .iter()
                .enumerate()
                .map(|(i, (hash, sender, nonce, status))| CanonicalTx {
                    hash: *hash,
                    index: i as u32,
                    sender: *sender,
                    nonce: *nonce,
                    status: *status,
                })
                .collect(),
        })
    }

    /// Nine blocks (four beyond the window of five) with own, foreign and
    /// reverted transactions; built from blocks only.
    fn original() -> ChainState {
        let ok = ExecutionStatus::Succeeded;
        let mut s = ChainState::new(CHAIN, limits(), &[own()]).unwrap();
        for n in 0..9u64 {
            let txs = [
                (t(2 * n), own(), n, ok),
                (
                    t(2 * n + 1),
                    foreign(),
                    10 + n,
                    if n % 3 == 0 {
                        ExecutionStatus::Reverted
                    } else {
                        ok
                    },
                ),
            ];
            let out = s.apply(&block(BASE + n, 0, 0, &txs));
            assert!(out.conflicts.is_empty(), "{:?}", out.conflicts);
        }
        s.apply(&ChainInput::NonceFact {
            sender: own(),
            next_nonce: 9,
            block_number: BASE + 8,
            block_hash: bh(BASE + 8, 0),
        });
        assert_eq!(s.sender(own()), SenderView::Clean { next_nonce: 9 });
        s
    }

    fn reseal(body: &[u8]) -> Vec<u8> {
        let mut bytes = body.to_vec();
        bytes.extend_from_slice(keccak256(body).as_slice());
        bytes
    }

    fn body(bytes: &[u8]) -> Vec<u8> {
        bytes[..bytes.len() - CHECKSUM_LEN].to_vec()
    }

    fn restore(bytes: &[u8]) -> Result<ChainState, CheckpointError> {
        ChainState::restore(CHAIN, limits(), &[own()], bytes)
    }

    /// Round trip: the retained window, the head and the views of every
    /// replayed transaction are restored; watched senders are quarantined.
    #[test]
    fn checkpoint_round_trip_restores_the_window_quarantined() {
        let s = original();
        let bytes = s.encode_checkpoint();
        assert_eq!(&bytes[..8], b"DBC2CKP1");
        let r = restore(&bytes).unwrap();

        assert_eq!(r.high_water(), s.high_water());
        assert_eq!(r.blocks, s.blocks);
        assert_eq!(r.counters().restored_blocks, 5);
        assert_eq!(r.counters().retained_blocks, 5);
        for block in &s.blocks {
            for tx in &block.txs {
                assert_eq!(r.tx(&tx.hash), s.tx(&tx.hash));
                assert_eq!(r.nonce(tx.sender, tx.nonce), s.nonce(tx.sender, tx.nonce));
            }
        }
        assert_eq!(r.sender(foreign()), s.sender(foreign()));
        assert_eq!(
            r.sender(own()),
            SenderView::Quarantined {
                reason: SenderReason::Restart
            }
        );
        assert_eq!(r.encode_checkpoint(), bytes);

        // The replayed effects are the basis of the reconciliation.
        let mut r = r;
        let out = r.apply(&ChainInput::NonceFact {
            sender: own(),
            next_nonce: 9,
            block_number: BASE + 8,
            block_hash: bh(BASE + 8, 0),
        });
        assert_eq!(out.reconciliations.len(), 1);
    }

    /// A transaction the state could not hold is still part of its block in
    /// the checkpoint: the block list stays complete.
    #[test]
    fn overflowed_transactions_are_kept_in_the_checkpoint() {
        let small = StateLimits {
            max_txs: 3,
            ..limits()
        };
        let mut s = ChainState::new(CHAIN, small, &[own()]).unwrap();
        for id in 0..3u64 {
            s.apply(&ChainInput::Pending(PendingObservation {
                hash: t(100 + id),
                sender: Address::repeat_byte(0x40 + id as u8),
                nonce: 0,
                first_seen: FirstSeen {
                    source: 0,
                    at_ns: id,
                },
            }));
        }
        let ok = ExecutionStatus::Succeeded;
        let out = s.apply(&block(BASE, 0, 0, &[(t(1), foreign(), 4, ok)]));
        assert_eq!(out.overflow.len(), 1);
        assert_eq!(s.tx(&t(1)), None);

        let r = ChainState::restore(CHAIN, small, &[own()], &s.encode_checkpoint()).unwrap();
        assert!(matches!(
            r.tx(&t(1)).unwrap().state,
            TxState::Included {
                block_number: BASE,
                ..
            }
        ));
        assert_eq!(r.sender(foreign()), SenderView::Clean { next_nonce: 5 });
    }

    /// Every single-byte corruption and every truncation is refused.
    #[test]
    fn every_corruption_and_truncation_is_refused() {
        let bytes = original().encode_checkpoint();
        for i in 0..bytes.len() {
            let mut corrupt = bytes.clone();
            corrupt[i] ^= 0x01;
            assert!(restore(&corrupt).is_err(), "flip at {i} restored");
        }
        for len in 0..bytes.len() {
            assert!(restore(&bytes[..len]).is_err(), "prefix {len} restored");
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(restore(&longer).is_err());
    }

    /// Chain, version, trailing bytes, entry kind, counts and refused blocks
    /// are checked even under a valid checksum.
    #[test]
    fn resealed_malformed_checkpoints_are_refused() {
        let s = original();
        let bytes = s.encode_checkpoint();
        assert_eq!(
            ChainState::restore(97, limits(), &[own()], &bytes),
            Err(CheckpointError::WrongChain {
                expected: 97,
                found: CHAIN
            })
        );

        let mut b = body(&bytes);
        b[8] = 2;
        assert_eq!(
            restore(&reseal(&b)),
            Err(CheckpointError::UnsupportedVersion(2))
        );

        let mut b = body(&bytes);
        b.push(0);
        assert_eq!(restore(&reseal(&b)), Err(CheckpointError::TrailingBytes(1)));

        let mut b = body(&bytes);
        b[0] = b'X';
        assert_eq!(restore(&reseal(&b)), Err(CheckpointError::BadMagic));

        let mut b = body(&bytes);
        b[17..25].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(
            restore(&reseal(&b)),
            Err(CheckpointError::Truncated) | Err(CheckpointError::LengthOutOfRange)
        ));

        // An entry that is not a block.
        let gap = ChainInput::Gap { from: 1, to: 2 }.encode();
        let mut b = bytes[..HEADER_LEN].to_vec();
        b[17..25].copy_from_slice(&1u64.to_le_bytes());
        b.extend_from_slice(&(gap.len() as u32).to_le_bytes());
        b.extend_from_slice(&gap);
        assert_eq!(
            restore(&reseal(&b)),
            Err(CheckpointError::NotABlock { position: 0 })
        );

        // Blocks that do not link are refused by the state.
        let ok = ExecutionStatus::Succeeded;
        let mut b = bytes[..HEADER_LEN].to_vec();
        b[17..25].copy_from_slice(&2u64.to_le_bytes());
        for input in [
            block(BASE, 0, 0, &[]),
            block(BASE + 2, 0, 0, &[(t(1), own(), 0, ok)]),
        ] {
            let e = input.encode();
            b.extend_from_slice(&(e.len() as u32).to_le_bytes());
            b.extend_from_slice(&e);
        }
        assert!(matches!(
            restore(&reseal(&b)),
            Err(CheckpointError::BlockRefused {
                position: 1,
                conflict: Conflict::Discontinuity { .. }
            })
        ));

        assert!(matches!(
            ChainState::restore(
                CHAIN,
                StateLimits {
                    max_txs: 0,
                    ..limits()
                },
                &[own()],
                &bytes
            ),
            Err(CheckpointError::Limits(LimitsError::ZeroMaxTxs))
        ));
    }

    /// An empty checkpoint (unanchored state) restores an unanchored state.
    #[test]
    fn empty_checkpoint_restores_an_unanchored_state() {
        let fresh = ChainState::new(CHAIN, limits(), &[own()]).unwrap();
        let bytes = fresh.encode_checkpoint();
        assert_eq!(bytes.len(), HEADER_LEN + CHECKSUM_LEN);
        let mut r = restore(&bytes).unwrap();
        assert_eq!(r.high_water(), None);
        assert_eq!(r.counters().restored_blocks, 0);
        assert_eq!(
            r.sender(own()),
            SenderView::Quarantined {
                reason: SenderReason::Restart
            }
        );
        assert_eq!(r.nonce(own(), 0).consumption, Consumption::Unknown);
        // Any first block anchors it.
        let out = r.apply(&block(BASE, 3, 9, &[]));
        assert!(out.conflicts.is_empty());
    }

    /// After a restore the follower continues at the checkpoint's head: a
    /// block extending it is accepted; any other is a counted
    /// discontinuity that re-anchors the state (the owner never stalls).
    #[test]
    fn restored_state_follows_the_checkpoint_head_and_re_anchors_otherwise() {
        let s = original();
        let ok = ExecutionStatus::Succeeded;
        let mut r = restore(&s.encode_checkpoint()).unwrap();
        let out = r.apply(&block(BASE + 9, 0, 0, &[(t(99), own(), 9, ok)]));
        assert!(out.conflicts.is_empty());
        assert_eq!(out.inclusions.len(), 1);
        assert_eq!(r.high_water(), Some((BASE + 9, bh(BASE + 9, 0))));

        for (number, parent_fork) in [(BASE + 9, 7), (BASE + 10, 0)] {
            let mut r = restore(&s.encode_checkpoint()).unwrap();
            let out = r.apply(&block(number, 0, parent_fork, &[]));
            assert_eq!(
                out.conflicts,
                vec![Conflict::Discontinuity {
                    number,
                    hash: bh(number, 0),
                    parent_hash: bh(number - 1, parent_fork)
                }]
            );
            assert_eq!(r.high_water(), Some((number, bh(number, 0))));
            assert_eq!(r.retained(), vec![(number, bh(number, 0))]);
            assert_eq!(
                r.sender(own()),
                SenderView::Quarantined {
                    reason: SenderReason::Discontinuity
                }
            );
            assert_eq!(r.counters().discontinuities, 1);
        }
    }
}

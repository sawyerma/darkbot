//! Typed inputs of the chain-state owner and their fixed binary encoding.
//!
//! Every input the chain-state owner ([`super::engine::ChainState`]) applies
//! is one [`ChainInput`]. The caller journals each input with
//! [`ChainInput::encode`] before it applies it and rebuilds the state after a
//! restart by decoding and applying the journaled inputs in their order
//! (master plan §7, §12; retirement contract §3 "capture/replay deterministic
//! ordering"). There is no JSON and no text form.
//!
//! # Encoding (version 1)
//!
//! All integers are little-endian and of fixed width. Hashes are their 32
//! raw bytes, addresses their 20 raw bytes. A list is a `u64` item count
//! followed by its items.
//!
//! ```text
//! byte 0        format version (1)
//! byte 1        tag
//! tag 1 Pending      hash | sender | nonce u64 | source u8 | at_ns u64
//! tag 2 Block        number u64 | hash | parent_hash | timestamp_ms u64 |
//!                    count u64 | count × (hash | index u32 | sender |
//!                                         nonce u64 | status u8)
//!                    status: 0 = Succeeded, 1 = Reverted
//! tag 3 Reorg        count u64 | count × (number u64 | hash) |
//!                    new_head number u64 | new_head hash
//! tag 4 ReorgTooDeep as tag 3
//! tag 5 Gap          from u64 | to u64
//! tag 6 NonceFact    sender | next_nonce u64 | block_number u64 | block_hash
//! ```
//!
//! Decoding is fail-closed: an unknown version, an unknown tag, an unknown
//! execution status, a count that the remaining bytes cannot hold, a
//! truncated field or a single trailing byte is an error. A count is checked
//! against the remaining bytes before anything is allocated for it.

use alloy_primitives::{Address, B256};

use super::canonical::ExecutionStatus;

/// Version byte of the encoding produced by [`ChainInput::encode`].
pub const INPUT_FORMAT_VERSION: u8 = 1;

const TAG_PENDING: u8 = 1;
const TAG_BLOCK: u8 = 2;
const TAG_REORG: u8 = 3;
const TAG_REORG_TOO_DEEP: u8 = 4;
const TAG_GAP: u8 = 5;
const TAG_NONCE_FACT: u8 = 6;

const STATUS_SUCCEEDED: u8 = 0;
const STATUS_REVERTED: u8 = 1;

/// Encoded size of one [`CanonicalTx`].
pub(super) const TX_SIZE: usize = 32 + 4 + 20 + 8 + 1;
/// Encoded size of a block input without its transactions.
pub(super) const BLOCK_PREFIX_SIZE: usize = 2 + 8 + 32 + 32 + 8 + 8;
/// Encoded size of one `(number, hash)` pair.
const BLOCK_ID_SIZE: usize = 8 + 32;

/// When and where a pending transaction was seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirstSeen {
    /// Opaque index of the observing source (peer lane, provider, …).
    pub source: u8,
    /// Native observation time in nanoseconds of the caller's one clock
    /// domain. The state compares these values; it never reads a clock.
    pub at_ns: u64,
}

/// One sighting of a pending (not yet canonical) transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingObservation {
    /// Transaction hash.
    pub hash: B256,
    /// Recovered sender.
    pub sender: Address,
    /// Nonce of the transaction.
    pub nonce: u64,
    /// The sighting.
    pub first_seen: FirstSeen,
}

/// One transaction of a canonical block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanonicalTx {
    /// Transaction hash.
    pub hash: B256,
    /// Position in the block. A block's list must be complete: the `i`-th
    /// entry has index `i`.
    pub index: u32,
    /// Sender.
    pub sender: Address,
    /// Nonce the transaction consumed.
    pub nonce: u64,
    /// Execution result from the receipt.
    pub status: ExecutionStatus,
}

/// A block of the canonical chain with its complete transaction list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalBlock {
    /// Block number.
    pub number: u64,
    /// Block hash.
    pub hash: B256,
    /// Hash of the parent block.
    pub parent_hash: B256,
    /// Header time in milliseconds (bsc-geth `MilliTimestamp`). Kept with
    /// the retained block for checkpoints; no state rule uses it.
    pub timestamp_ms: u64,
    /// Every transaction of the block, in index order.
    pub txs: Vec<CanonicalTx>,
}

/// One input of the chain-state owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainInput {
    /// A pending transaction was seen.
    Pending(PendingObservation),
    /// A block extends the canonical chain.
    Block(CanonicalBlock),
    /// The listed blocks (ascending, ending at the head) left the canonical
    /// chain; the new chain's blocks up to `new_head` follow.
    Reorg {
        /// Orphaned blocks, ascending.
        orphaned: Vec<(u64, B256)>,
        /// Head of the new chain.
        new_head: (u64, B256),
    },
    /// The fork point is below every block the follower kept: the listed
    /// blocks left the chain, the fork point is unknown, following resumes
    /// at `new_head`.
    ReorgTooDeep {
        /// Orphaned blocks.
        orphaned: Vec<(u64, B256)>,
        /// The block following resumes at.
        new_head: (u64, B256),
    },
    /// The canonical blocks `from..=to` are not followed.
    Gap {
        /// First block not followed.
        from: u64,
        /// Last block not followed.
        to: u64,
    },
    /// An independent `eth_getTransactionCount(sender, block)` result.
    NonceFact {
        /// The account.
        sender: Address,
        /// Its next nonce after `block`.
        next_nonce: u64,
        /// Number of the block the count was taken at.
        block_number: u64,
        /// Hash of that block.
        block_hash: B256,
    },
}

/// Reason a journaled input could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// No bytes.
    Empty,
    /// The version byte is not [`INPUT_FORMAT_VERSION`].
    UnsupportedVersion(u8),
    /// The tag byte names no input.
    UnknownTag(u8),
    /// A field or the items of a count do not fit in the remaining bytes.
    Truncated,
    /// Bytes remain after the input.
    TrailingBytes(usize),
    /// The execution status byte is neither 0 nor 1.
    InvalidExecutionStatus(u8),
    /// A count does not fit this platform's address space.
    LengthOutOfRange,
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => f.write_str("chain input: empty"),
            Self::UnsupportedVersion(v) => write!(f, "chain input: unsupported version {v}"),
            Self::UnknownTag(t) => write!(f, "chain input: unknown tag {t}"),
            Self::Truncated => f.write_str("chain input: truncated"),
            Self::TrailingBytes(n) => write!(f, "chain input: {n} trailing bytes"),
            Self::InvalidExecutionStatus(s) => {
                write!(f, "chain input: invalid execution status {s}")
            }
            Self::LengthOutOfRange => f.write_str("chain input: count out of range"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl ChainInput {
    /// The fixed binary form of this input (module documentation).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.encoded_len());
        out.push(INPUT_FORMAT_VERSION);
        match self {
            Self::Pending(obs) => {
                out.push(TAG_PENDING);
                out.extend_from_slice(obs.hash.as_slice());
                out.extend_from_slice(obs.sender.as_slice());
                out.extend_from_slice(&obs.nonce.to_le_bytes());
                out.push(obs.first_seen.source);
                out.extend_from_slice(&obs.first_seen.at_ns.to_le_bytes());
            }
            Self::Block(block) => {
                out.push(TAG_BLOCK);
                out.extend_from_slice(&block.number.to_le_bytes());
                out.extend_from_slice(block.hash.as_slice());
                out.extend_from_slice(block.parent_hash.as_slice());
                out.extend_from_slice(&block.timestamp_ms.to_le_bytes());
                out.extend_from_slice(&(block.txs.len() as u64).to_le_bytes());
                for tx in &block.txs {
                    out.extend_from_slice(tx.hash.as_slice());
                    out.extend_from_slice(&tx.index.to_le_bytes());
                    out.extend_from_slice(tx.sender.as_slice());
                    out.extend_from_slice(&tx.nonce.to_le_bytes());
                    out.push(match tx.status {
                        ExecutionStatus::Succeeded => STATUS_SUCCEEDED,
                        ExecutionStatus::Reverted => STATUS_REVERTED,
                    });
                }
            }
            Self::Reorg { orphaned, new_head } => {
                out.push(TAG_REORG);
                encode_reorg(&mut out, orphaned, *new_head);
            }
            Self::ReorgTooDeep { orphaned, new_head } => {
                out.push(TAG_REORG_TOO_DEEP);
                encode_reorg(&mut out, orphaned, *new_head);
            }
            Self::Gap { from, to } => {
                out.push(TAG_GAP);
                out.extend_from_slice(&from.to_le_bytes());
                out.extend_from_slice(&to.to_le_bytes());
            }
            Self::NonceFact {
                sender,
                next_nonce,
                block_number,
                block_hash,
            } => {
                out.push(TAG_NONCE_FACT);
                out.extend_from_slice(sender.as_slice());
                out.extend_from_slice(&next_nonce.to_le_bytes());
                out.extend_from_slice(&block_number.to_le_bytes());
                out.extend_from_slice(block_hash.as_slice());
            }
        }
        out
    }

    /// Decodes one input encoded by [`ChainInput::encode`]. `bytes` must
    /// hold exactly one input.
    pub fn decode(bytes: &[u8]) -> Result<ChainInput, DecodeError> {
        let mut r = Reader { bytes, pos: 0 };
        let version = r.u8().map_err(|_| DecodeError::Empty)?;
        if version != INPUT_FORMAT_VERSION {
            return Err(DecodeError::UnsupportedVersion(version));
        }
        let tag = r.u8()?;
        let input = match tag {
            TAG_PENDING => ChainInput::Pending(PendingObservation {
                hash: r.b256()?,
                sender: r.address()?,
                nonce: r.u64()?,
                first_seen: FirstSeen {
                    source: r.u8()?,
                    at_ns: r.u64()?,
                },
            }),
            TAG_BLOCK => {
                let number = r.u64()?;
                let hash = r.b256()?;
                let parent_hash = r.b256()?;
                let timestamp_ms = r.u64()?;
                let count = r.count(TX_SIZE)?;
                let mut txs = Vec::with_capacity(count);
                for _ in 0..count {
                    txs.push(CanonicalTx {
                        hash: r.b256()?,
                        index: r.u32()?,
                        sender: r.address()?,
                        nonce: r.u64()?,
                        status: match r.u8()? {
                            STATUS_SUCCEEDED => ExecutionStatus::Succeeded,
                            STATUS_REVERTED => ExecutionStatus::Reverted,
                            other => return Err(DecodeError::InvalidExecutionStatus(other)),
                        },
                    });
                }
                ChainInput::Block(CanonicalBlock {
                    number,
                    hash,
                    parent_hash,
                    timestamp_ms,
                    txs,
                })
            }
            TAG_REORG => {
                let (orphaned, new_head) = decode_reorg(&mut r)?;
                ChainInput::Reorg { orphaned, new_head }
            }
            TAG_REORG_TOO_DEEP => {
                let (orphaned, new_head) = decode_reorg(&mut r)?;
                ChainInput::ReorgTooDeep { orphaned, new_head }
            }
            TAG_GAP => ChainInput::Gap {
                from: r.u64()?,
                to: r.u64()?,
            },
            TAG_NONCE_FACT => ChainInput::NonceFact {
                sender: r.address()?,
                next_nonce: r.u64()?,
                block_number: r.u64()?,
                block_hash: r.b256()?,
            },
            other => return Err(DecodeError::UnknownTag(other)),
        };
        let rest = r.remaining();
        if rest != 0 {
            return Err(DecodeError::TrailingBytes(rest));
        }
        Ok(input)
    }

    /// Exact length of [`ChainInput::encode`]'s output.
    fn encoded_len(&self) -> usize {
        let body = match self {
            Self::Pending(_) => 32 + 20 + 8 + 1 + 8,
            Self::Block(block) => {
                (8usize + 32 + 32 + 8 + 8).saturating_add(block.txs.len().saturating_mul(TX_SIZE))
            }
            Self::Reorg { orphaned, .. } | Self::ReorgTooDeep { orphaned, .. } => 8usize
                .saturating_add(orphaned.len().saturating_mul(BLOCK_ID_SIZE))
                .saturating_add(BLOCK_ID_SIZE),
            Self::Gap { .. } => 16,
            Self::NonceFact { .. } => 20 + 8 + 8 + 32,
        };
        body.saturating_add(2)
    }
}

fn encode_reorg(out: &mut Vec<u8>, orphaned: &[(u64, B256)], new_head: (u64, B256)) {
    out.extend_from_slice(&(orphaned.len() as u64).to_le_bytes());
    for (number, hash) in orphaned {
        out.extend_from_slice(&number.to_le_bytes());
        out.extend_from_slice(hash.as_slice());
    }
    out.extend_from_slice(&new_head.0.to_le_bytes());
    out.extend_from_slice(new_head.1.as_slice());
}

type ReorgFields = (Vec<(u64, B256)>, (u64, B256));

fn decode_reorg(r: &mut Reader<'_>) -> Result<ReorgFields, DecodeError> {
    let count = r.count(BLOCK_ID_SIZE)?;
    let mut orphaned = Vec::with_capacity(count);
    for _ in 0..count {
        orphaned.push((r.u64()?, r.b256()?));
    }
    let new_head = (r.u64()?, r.b256()?);
    Ok((orphaned, new_head))
}

/// Bounds-checked cursor over one encoded input.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::Truncated)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(DecodeError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let slice = self.take(N)?;
        <[u8; N]>::try_from(slice).map_err(|_| DecodeError::Truncated)
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.array::<1>()?[0])
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn b256(&mut self) -> Result<B256, DecodeError> {
        Ok(B256::from(self.array::<32>()?))
    }

    fn address(&mut self) -> Result<Address, DecodeError> {
        Ok(Address::from(self.array::<20>()?))
    }

    /// A list count whose items of `item_size` bytes must fit in the
    /// remaining bytes; checked before anything is allocated.
    fn count(&mut self, item_size: usize) -> Result<usize, DecodeError> {
        let count = usize::try_from(self.u64()?).map_err(|_| DecodeError::LengthOutOfRange)?;
        let needed = count
            .checked_mul(item_size)
            .ok_or(DecodeError::LengthOutOfRange)?;
        if needed > self.remaining() {
            return Err(DecodeError::Truncated);
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(byte: u8) -> B256 {
        B256::repeat_byte(byte)
    }

    fn samples() -> Vec<ChainInput> {
        vec![
            ChainInput::Pending(PendingObservation {
                hash: b(0x01),
                sender: Address::repeat_byte(0x02),
                nonce: u64::MAX,
                first_seen: FirstSeen {
                    source: 7,
                    at_ns: 0x0102_0304_0506_0708,
                },
            }),
            ChainInput::Block(CanonicalBlock {
                number: 100,
                hash: b(0x10),
                parent_hash: b(0x0f),
                timestamp_ms: 1_700_000_000_123,
                txs: vec![
                    CanonicalTx {
                        hash: b(0x20),
                        index: 0,
                        sender: Address::repeat_byte(0x21),
                        nonce: 5,
                        status: ExecutionStatus::Succeeded,
                    },
                    CanonicalTx {
                        hash: b(0x22),
                        index: 1,
                        sender: Address::repeat_byte(0x23),
                        nonce: 0,
                        status: ExecutionStatus::Reverted,
                    },
                ],
            }),
            ChainInput::Block(CanonicalBlock {
                number: 0,
                hash: b(0x30),
                parent_hash: B256::ZERO,
                timestamp_ms: 0,
                txs: Vec::new(),
            }),
            ChainInput::Reorg {
                orphaned: vec![(101, b(0x41)), (102, b(0x42))],
                new_head: (103, b(0x43)),
            },
            ChainInput::ReorgTooDeep {
                orphaned: Vec::new(),
                new_head: (u64::MAX, b(0x50)),
            },
            ChainInput::Gap { from: 7, to: 9 },
            ChainInput::NonceFact {
                sender: Address::repeat_byte(0x60),
                next_nonce: 3,
                block_number: 104,
                block_hash: b(0x61),
            },
        ]
    }

    /// Every input survives encode/decode unchanged, and the encoding has
    /// the documented length (journal round trip, retirement §3
    /// "capture/replay deterministic ordering").
    #[test]
    fn every_input_round_trips() {
        for input in samples() {
            let bytes = input.encode();
            assert_eq!(bytes.len(), input.encoded_len());
            assert_eq!(bytes[0], INPUT_FORMAT_VERSION);
            assert_eq!(ChainInput::decode(&bytes), Ok(input));
        }
    }

    /// The layout is fixed little-endian (a frozen byte vector).
    #[test]
    fn gap_layout_is_fixed_little_endian() {
        let bytes = ChainInput::Gap {
            from: 1,
            to: 0x0203,
        }
        .encode();
        assert_eq!(
            bytes,
            [1, TAG_GAP, 1, 0, 0, 0, 0, 0, 0, 0, 3, 2, 0, 0, 0, 0, 0, 0]
        );
    }

    /// Every strict prefix of every encoding is refused: nothing decodes
    /// from a truncated journal record (fail-closed).
    #[test]
    fn every_truncation_is_refused() {
        for input in samples() {
            let bytes = input.encode();
            for len in 0..bytes.len() {
                let result = ChainInput::decode(&bytes[..len]);
                assert!(result.is_err(), "prefix {len} of {input:?} decoded");
                if len == 0 {
                    assert_eq!(result, Err(DecodeError::Empty));
                }
            }
        }
    }

    #[test]
    fn trailing_bytes_are_refused() {
        for input in samples() {
            let mut bytes = input.encode();
            bytes.push(0);
            assert_eq!(
                ChainInput::decode(&bytes),
                Err(DecodeError::TrailingBytes(1))
            );
        }
    }

    #[test]
    fn unknown_version_tag_and_status_are_refused() {
        let mut bytes = ChainInput::Gap { from: 1, to: 2 }.encode();
        bytes[0] = 0;
        assert_eq!(
            ChainInput::decode(&bytes),
            Err(DecodeError::UnsupportedVersion(0))
        );
        bytes[0] = 2;
        assert_eq!(
            ChainInput::decode(&bytes),
            Err(DecodeError::UnsupportedVersion(2))
        );
        bytes[0] = INPUT_FORMAT_VERSION;
        for tag in [0u8, 7, 0xff] {
            bytes[1] = tag;
            assert_eq!(
                ChainInput::decode(&bytes),
                Err(DecodeError::UnknownTag(tag))
            );
        }

        let block = &samples()[1];
        let mut bytes = block.encode();
        let status_at = 2 + 8 + 32 + 32 + 8 + 8 + TX_SIZE - 1;
        bytes[status_at] = 2;
        assert_eq!(
            ChainInput::decode(&bytes),
            Err(DecodeError::InvalidExecutionStatus(2))
        );
    }

    /// A count larger than the remaining bytes can hold is refused before
    /// any allocation for it.
    #[test]
    fn oversized_counts_are_refused_before_allocation() {
        let mut bytes = vec![INPUT_FORMAT_VERSION, TAG_REORG];
        bytes.extend_from_slice(&u64::MAX.to_le_bytes());
        let result = ChainInput::decode(&bytes);
        assert!(matches!(
            result,
            Err(DecodeError::LengthOutOfRange) | Err(DecodeError::Truncated)
        ));

        let mut bytes = vec![INPUT_FORMAT_VERSION, TAG_BLOCK];
        bytes.extend_from_slice(&[0u8; 8 + 32 + 32 + 8]);
        bytes.extend_from_slice(&1_000_000u64.to_le_bytes());
        assert_eq!(ChainInput::decode(&bytes), Err(DecodeError::Truncated));
    }
}

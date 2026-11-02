//! REV.1.0 R3b (BSC3-024/041): what the chain says of one LIVE transaction
//! the follower never saw resolved (a gap, a restart, a block it did not
//! follow, a window it never saw end). Only final blocks count: at least
//! [`FollowerLimits::reorg_depth`] below the followed head, the block the
//! primary serves equal to the independent second source's (BSC3-056).
//!
//! A block is a word on a transaction only when it holds it: by its hash
//! (its receipt names the height), or the sender's transaction at its
//! nonce (the block where the sender's next nonce passes it, as both
//! sources read it at the last final block; found by a search read at the
//! primary, the block then fetched and confirmed). Such a block is
//! reported as the follower reports one ([`super::follow::block_events`]);
//! its consumer resolves it as any other: the transaction itself included
//! or reverted (also when both nodes dropped its receipt), or another one
//! of the sender at its nonce (which replaced it). Nothing else is a word:
//! no receipt, a window over or one source's reading decide nothing.

use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;

use super::follow::{FirstHeld, Rpc, block_events, receipts_match};
use super::jsonrpc::Header;
use super::{
    Call, CanonicalEvent, Field, FollowerFault, FollowerLimits, RequestError, is_unrebuildable,
};
use crate::wss::DisconnectReason;

/// The transaction asked about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryTarget {
    /// A transaction whose hash is known; `window_end` the last block its
    /// bundle may land in (none: not known, or it bounds nothing after its
    /// block left the chain).
    Hash {
        tx_hash: [u8; 32],
        sender: [u8; 20],
        nonce: u64,
        window_end: Option<u64>,
    },
    /// A lease's BUY (its hash never known): the sender's transaction at
    /// `nonce`, its bundle landing up to `to`; `from` where the search for
    /// it starts (the head the lease was taken at, as the configuration
    /// says now: a start, never a bound).
    Window {
        sender: [u8; 20],
        nonce: u64,
        from: u64,
        to: u64,
    },
}

/// What the chain says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recovered {
    /// The final block that holds the transaction, or the sender's
    /// transaction at its nonce, confirmed by the second source: its
    /// events, as the follower reports a block.
    Block(Vec<CanonicalEvent>),
    /// Nothing to decide yet.
    Pending(RecoveryPending),
}

/// Why a recovery decided nothing (yet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryPending {
    /// The block it names (or the window's end) is not final at the
    /// followed head.
    NotFinal { block: u64 },
    /// Its nonce not taken at the last final block, as both sources read
    /// it (and no receipt names a final block).
    NotFound,
    /// The two sources hold different blocks at this height, or read the
    /// sender's next nonce at it on either side of the transaction's.
    SourcesDisagree { number: u64 },
    /// The block at this height does not hold the transaction (nor the
    /// sender's at its nonce).
    NotInBlock { number: u64 },
    /// The sender's next nonce is past the transaction's at every height
    /// searched, down to `from` (the search's bound).
    BeyondSearch { from: u64 },
}

impl RecoveryPending {
    pub fn name(self) -> &'static str {
        match self {
            Self::NotFinal { .. } => "RECOVERY_NOT_FINAL",
            Self::NotFound => "RECOVERY_NOT_FOUND",
            Self::SourcesDisagree { .. } => "RECOVERY_SOURCES_DISAGREE",
            Self::NotInBlock { .. } => "RECOVERY_NOT_IN_BLOCK",
            Self::BeyondSearch { .. } => "RECOVERY_BEYOND_SEARCH",
        }
    }
}

/// The two sources: the primary answers, the second confirms.
pub(super) struct Sources<'a, S, C, S2, C2> {
    pub(super) primary: &'a mut Rpc<S, C>,
    pub(super) second: &'a mut Rpc<S2, C2>,
    pub(super) stop: &'a AtomicBool,
}

/// What a block must hold to be a word on the transaction.
#[derive(Clone, Copy)]
enum Holds {
    Hash([u8; 32]),
    Nonce([u8; 20], u64),
}

impl<S, C, S2, C2> Sources<'_, S, C, S2, C2>
where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
    S2: Read + Write,
    C2: FnMut(&AtomicBool) -> Result<S2, DisconnectReason>,
{
    /// The header of `number` both sources hold (none: they differ).
    fn agreed(&mut self, number: u64) -> Result<Option<Header>, FollowerFault> {
        let ours = self.primary.header(number, self.stop)?;
        let theirs = self.second.header(number, self.stop)?;
        Ok((ours.hash == theirs.hash).then_some(ours))
    }

    /// Whether the sender's next nonce at `number` is past `nonce`, as both
    /// sources read it pinned to the block both hold there; none when they
    /// hold different blocks or read it on either side of `nonce` (one
    /// source's reading never decides).
    fn past_agreed(
        &mut self,
        sender: [u8; 20],
        nonce: u64,
        number: u64,
    ) -> Result<Option<bool>, FollowerFault> {
        let Some(at) = self.agreed(number)? else {
            return Ok(None);
        };
        let ours = self.primary.transaction_count(sender, at.hash, self.stop)? > nonce;
        let theirs = self.second.transaction_count(sender, at.hash, self.stop)? > nonce;
        Ok((ours == theirs).then_some(ours))
    }

    /// The sender's next nonce at `number`, read at the primary pinned to
    /// its block there (the search's reading; what it finds is confirmed).
    fn next_nonce_at(&mut self, sender: [u8; 20], number: u64) -> Result<u64, FollowerFault> {
        let h = self.primary.header(number, self.stop)?;
        self.primary.transaction_count(sender, h.hash, self.stop)
    }

    /// The final block `number` if it holds `holds`, confirmed by the
    /// second source, as the follower reports it. A provider-side refusal
    /// of a launch's transaction object fetches the block again, up to
    /// `tx_attempts` times, then is carried by the launch (as the
    /// follower's).
    fn block_at(
        &mut self,
        limits: &FollowerLimits,
        number: u64,
        holds: Holds,
    ) -> Result<Recovered, FollowerFault> {
        let confirmed = self.second.header(number, self.stop)?;
        let attempts = limits.tx_attempts.max(1);
        let mut attempt = 0;
        loop {
            attempt += 1;
            let block = self.primary.block(number, self.stop)?;
            if block.head.header.hash != confirmed.hash {
                return Ok(Recovered::Pending(RecoveryPending::SourcesDisagree {
                    number,
                }));
            }
            let receipts = self.primary.receipts(number, self.stop)?;
            if !receipts_match(&block.head, &receipts) {
                return Err(FollowerFault::ReceiptsMismatch(number));
            }
            let held = block.head.txs.iter().any(|t| match holds {
                Holds::Hash(h) => t.hash == h,
                Holds::Nonce(sender, nonce) => t.from == sender && t.nonce == nonce,
            });
            if !held {
                return Ok(Recovered::Pending(RecoveryPending::NotInBlock { number }));
            }
            let last = attempt >= attempts;
            let mut refused = |_: [u8; 32], reason| {
                if is_unrebuildable(reason) || last {
                    Ok(())
                } else {
                    Err(FollowerFault::Request {
                        call: Call::Block(number),
                        error: RequestError::Unverifiable(reason),
                    })
                }
            };
            match block_events(
                block,
                &receipts,
                FirstHeld::now(),
                limits.max_launch_calldata,
                &mut refused,
            ) {
                Ok(events) => return Ok(Recovered::Block(events)),
                Err(FollowerFault::Request {
                    error: RequestError::Unverifiable(_),
                    ..
                }) if !last => {}
                Err(fault) => return Err(fault),
            }
        }
    }

    /// The block below `top` (the sender's next nonce past `nonce` there)
    /// where it passes: a height where it is not past yet, from `hint` (the
    /// height the search probes first: a window's start or end; none: one
    /// below `top`) with the step doubled while it is past, down to `floor`
    /// at most; then bisected. None when it is past at every height down
    /// to that bound.
    fn taker(
        &mut self,
        sender: [u8; 20],
        nonce: u64,
        top: u64,
        hint: Option<u64>,
        floor: u64,
    ) -> Result<Option<u64>, FollowerFault> {
        let mut step = hint.map_or(1, |h| top.saturating_sub(h)).max(1);
        let below = loop {
            let at = top.saturating_sub(step).max(floor);
            if self.next_nonce_at(sender, at)? <= nonce {
                break at;
            }
            if at == floor {
                return Ok(None);
            }
            step = step.saturating_mul(2);
        };
        // Not past at `lo - 1`, past at `hi`.
        let (mut lo, mut hi) = (below + 1, top);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.next_nonce_at(sender, mid)? > nonce {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        Ok(Some(hi))
    }
}

/// What the chain says of `target` at the followed head `head`; the search
/// for the block where a nonce passes goes at most `search_blocks` below
/// the window's end (none: the last final block).
pub(super) fn recover<S, C, S2, C2>(
    sources: &mut Sources<'_, S, C, S2, C2>,
    limits: &FollowerLimits,
    search_blocks: u64,
    head: u64,
    target: RecoveryTarget,
) -> Result<Recovered, FollowerFault>
where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
    S2: Read + Write,
    C2: FnMut(&AtomicBool) -> Result<S2, DisconnectReason>,
{
    let Some(last_final) = head.checked_sub(limits.reorg_depth) else {
        return Ok(Recovered::Pending(RecoveryPending::NotFinal {
            block: head,
        }));
    };
    let not_final = |number: u64| {
        (number > last_final).then_some(Recovered::Pending(RecoveryPending::NotFinal {
            block: number,
        }))
    };
    // The sender, its nonce, where its bundle may land last (none: not
    // known or bounding nothing) and where the search probes first.
    let (sender, nonce, window_end, hint) = match target {
        RecoveryTarget::Hash {
            tx_hash,
            sender,
            nonce,
            window_end,
        } => {
            let receipt = match sources.primary.receipt(tx_hash, sources.stop)? {
                Some(r) => Some(r),
                None => sources.second.receipt(tx_hash, sources.stop)?,
            };
            if let Some(r) = receipt {
                // A receipt of another transaction or sender is the
                // provider's error.
                let shape = |field| FollowerFault::Request {
                    call: Call::Receipt(tx_hash),
                    error: RequestError::Shape(field),
                };
                if r.tx_hash != tx_hash {
                    return Err(shape(Field::TxHash));
                }
                if r.from != sender {
                    return Err(shape(Field::From));
                }
                if let Some(pending) = not_final(r.block_number) {
                    return Ok(pending);
                }
                // A receipt whose height's block does not hold it (an index
                // stale after a reorg): the search decides.
                match sources.block_at(limits, r.block_number, Holds::Hash(tx_hash))? {
                    Recovered::Pending(RecoveryPending::NotInBlock { .. }) => {}
                    found => return Ok(found),
                }
            }
            (sender, nonce, window_end, window_end)
        }
        RecoveryTarget::Window {
            sender,
            nonce,
            from,
            to,
        } => (sender, nonce, Some(to), Some(from)),
    };
    // Its bundle may still land in a block not final yet.
    if let Some(pending) = window_end.and_then(not_final) {
        return Ok(pending);
    }
    match sources.past_agreed(sender, nonce, last_final)? {
        None => {
            return Ok(Recovered::Pending(RecoveryPending::SourcesDisagree {
                number: last_final,
            }));
        }
        Some(false) => return Ok(Recovered::Pending(RecoveryPending::NotFound)),
        Some(true) => {}
    }
    // The search reaches `search_blocks` below its window's end (a lease's
    // whole window, however long ago it ended), else below the last final
    // block; it bisects up to the last final block (bytes landing late).
    let floor = window_end
        .unwrap_or(last_final)
        .saturating_sub(search_blocks);
    match sources.taker(sender, nonce, last_final, hint, floor)? {
        Some(number) => sources.block_at(limits, number, Holds::Nonce(sender, nonce)),
        None => Ok(Recovered::Pending(RecoveryPending::BeyondSearch {
            from: floor,
        })),
    }
}

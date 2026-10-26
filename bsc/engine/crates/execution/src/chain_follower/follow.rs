//! The follower: the typed RPC calls, the kept canonical window, reorg
//! resolution and the step loop with its backoff.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::http::Client;
use super::jsonrpc::{self, BlockHead, FullBlock, Receipt};
use super::launches;
use super::{
    Call, CanonicalEvent, CanonicalTx, FollowerFault, FollowerLimits, NonceWatch, RequestError,
    is_unrebuildable, stopped,
};
use crate::wss::DisconnectReason;

/// The typed JSON-RPC calls over one HTTP client.
pub(super) struct Rpc<S, C> {
    http: Client<S, C>,
    next_id: u64,
}

impl<S, C> Rpc<S, C>
where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
{
    pub(super) fn new(http: Client<S, C>) -> Self {
        Self { http, next_id: 1 }
    }

    fn post(&mut self, call: Call, stop: &AtomicBool) -> Result<(Vec<u8>, u64), FollowerFault> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let body = jsonrpc::request(id, call);
        self.http
            .post(body.as_bytes(), stop)
            .map(|answer| (answer, id))
            .map_err(|error| FollowerFault::Request { call, error })
    }

    fn block_number(&mut self, stop: &AtomicBool) -> Result<u64, FollowerFault> {
        let call = Call::BlockNumber;
        let (answer, id) = self.post(call, stop)?;
        jsonrpc::parse_quantity(&answer, id).map_err(|error| FollowerFault::Request { call, error })
    }

    fn transaction_count(
        &mut self,
        sender: [u8; 20],
        block_hash: [u8; 32],
        stop: &AtomicBool,
    ) -> Result<u64, FollowerFault> {
        let call = Call::TransactionCount { sender, block_hash };
        let (answer, id) = self.post(call, stop)?;
        jsonrpc::parse_quantity(&answer, id).map_err(|error| FollowerFault::Request { call, error })
    }

    fn block(&mut self, number: u64, stop: &AtomicBool) -> Result<FullBlock, FollowerFault> {
        let call = Call::Block(number);
        let (answer, id) = self.post(call, stop)?;
        jsonrpc::parse_block(&answer, id, number)
            .map_err(|error| FollowerFault::Request { call, error })?
            .ok_or(FollowerFault::Unavailable(call))
    }

    fn receipts(&mut self, number: u64, stop: &AtomicBool) -> Result<Vec<Receipt>, FollowerFault> {
        let call = Call::Receipts(number);
        let (answer, id) = self.post(call, stop)?;
        jsonrpc::parse_receipts(&answer, id)
            .map_err(|error| FollowerFault::Request { call, error })?
            .ok_or(FollowerFault::Unavailable(call))
    }
}

/// When the follower first held a header: the monotonic clock and, read
/// right after it, the wall clock in Unix milliseconds (`None` when the
/// system clock is unreadable or before 1970).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FirstHeld {
    at: Instant,
    unix_ms: Option<u64>,
}

impl FirstHeld {
    fn now() -> Self {
        let at = Instant::now();
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|d| u64::try_from(d.as_millis()).ok());
        Self { at, unix_ms }
    }
}

/// Runs steps until `stop`: after a completed step wait `poll`, after a
/// fault report it and wait the backoff (doubling from `backoff_initial`
/// to `backoff_max`; `backoff_max` at once after a rate-limit signal).
pub(super) fn run<S, C>(
    rpc: &mut Rpc<S, C>,
    limits: &FollowerLimits,
    resume: &[(u64, [u8; 32])],
    watch: &NonceWatch,
    stop: &AtomicBool,
    sink: &mut dyn FnMut(CanonicalEvent),
) where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
{
    let mut follower = Follower::new(limits, resume, watch);
    let mut next_wait = limits.backoff_initial;
    while !stopped(stop) {
        let wait = match follower.step(rpc, stop, sink) {
            Ok(()) => {
                next_wait = limits.backoff_initial;
                limits.poll
            }
            Err(FollowerFault::Request {
                error: RequestError::Transport(DisconnectReason::Stopped),
                ..
            }) => return,
            Err(fault) => {
                let rate_limited = matches!(
                    fault,
                    FollowerFault::Request {
                        error: RequestError::RateLimited(_),
                        ..
                    }
                );
                let wait = if rate_limited {
                    limits.backoff_max
                } else {
                    next_wait
                };
                next_wait = wait.saturating_mul(2).min(limits.backoff_max);
                sink(CanonicalEvent::Fault(fault));
                wait
            }
        };
        if !wait_unless_stopped(wait, limits.read_tick, stop) {
            return;
        }
    }
}

/// Sleeps `wait` in slices of at most `tick`; `false` when `stop` was set.
fn wait_unless_stopped(wait: Duration, tick: Duration, stop: &AtomicBool) -> bool {
    let start = Instant::now();
    loop {
        if stopped(stop) {
            return false;
        }
        let remaining = wait.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            return true;
        }
        thread::sleep(remaining.min(tick));
    }
}

/// The canonical window and the follower's position.
pub(super) struct Follower {
    reorg_depth: u64,
    max_catch_up: u64,
    /// The last followed blocks (number, hash), contiguous and ascending:
    /// at most `reorg_depth + 1` (the deepest provable fork point
    /// included).
    kept: VecDeque<(u64, [u8; 32])>,
    /// The lowest height not yet followed; `None` before the first block.
    next: Option<u64>,
    /// When a header (number, hash) was first held, for blocks not yet
    /// followed; at most `reorg_depth + 2` entries.
    seen: VecDeque<(u64, [u8; 32], FirstHeld)>,
    tx_attempts: u32,
    max_launch_calldata: usize,
    /// Refused answers per launch transaction of the block being accepted;
    /// cleared when a block is accepted (bounded by one block's launches).
    refusals: Vec<([u8; 32], u32)>,
    watch: NonceWatch,
    /// The next followed block carries nonce facts regardless of its number.
    facts_due: bool,
}

impl Follower {
    /// `resume`: the blocks the caller applied last (ascending, consecutive
    /// heights), kept as the window until the next block proves them
    /// canonical or orphans some of them. Only the last `reorg_depth + 1`
    /// are kept; a list with a height step other than one keeps only its
    /// last block.
    pub(super) fn new(
        limits: &FollowerLimits,
        resume: &[(u64, [u8; 32])],
        watch: &NonceWatch,
    ) -> Self {
        let consecutive = resume
            .windows(2)
            .all(|w| w[0].0.checked_add(1) == Some(w[1].0));
        let keep = if consecutive {
            usize::try_from(limits.reorg_depth.saturating_add(1)).unwrap_or(usize::MAX)
        } else {
            1
        };
        let kept: VecDeque<_> = resume[resume.len().saturating_sub(keep)..]
            .iter()
            .copied()
            .collect();
        Self {
            reorg_depth: limits.reorg_depth,
            max_catch_up: limits.max_catch_up,
            next: kept.back().map(|(number, _)| number.saturating_add(1)),
            kept,
            seen: VecDeque::new(),
            tx_attempts: limits.tx_attempts,
            max_launch_calldata: limits.max_launch_calldata,
            refusals: Vec::new(),
            watch: watch.clone(),
            facts_due: true,
        }
    }

    /// One head report and every block up to it.
    fn step<S, C>(
        &mut self,
        rpc: &mut Rpc<S, C>,
        stop: &AtomicBool,
        sink: &mut dyn FnMut(CanonicalEvent),
    ) -> Result<(), FollowerFault>
    where
        S: Read + Write,
        C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
    {
        let head = rpc.block_number(stop)?;
        let Some(last) = self.kept.back().map(|(number, _)| *number) else {
            return self.restart(rpc, head, stop, sink);
        };
        if head <= last {
            return Ok(());
        }
        if head - last > self.max_catch_up {
            self.kept.clear();
            return self.restart(rpc, head, stop, sink);
        }
        for number in last + 1..=head {
            if stopped(stop) {
                return Ok(());
            }
            self.advance(rpc, number, stop, sink)?;
        }
        Ok(())
    }

    /// Follows from `head` with no kept window: the first block, or a
    /// resync. Heights skipped since the last followed one are a gap.
    fn restart<S, C>(
        &mut self,
        rpc: &mut Rpc<S, C>,
        head: u64,
        stop: &AtomicBool,
        sink: &mut dyn FnMut(CanonicalEvent),
    ) -> Result<(), FollowerFault>
    where
        S: Read + Write,
        C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
    {
        if let Some(next) = self.next {
            if head < next {
                // The provider reports less than was followed: wait.
                return Ok(());
            }
            if head > next {
                sink(CanonicalEvent::Gap {
                    from: next,
                    to: head - 1,
                });
                self.next = Some(head);
                self.facts_due = true;
            }
        }
        let (block, observed) = self.fetch_block(rpc, head, stop)?;
        self.accept(rpc, block, observed, stop, sink)
    }

    /// Follows block `number`, the kept head's successor.
    fn advance<S, C>(
        &mut self,
        rpc: &mut Rpc<S, C>,
        number: u64,
        stop: &AtomicBool,
        sink: &mut dyn FnMut(CanonicalEvent),
    ) -> Result<(), FollowerFault>
    where
        S: Read + Write,
        C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
    {
        let (block, observed) = self.fetch_block(rpc, number, stop)?;
        let extends = self
            .kept
            .back()
            .is_none_or(|(_, hash)| *hash == block.head.header.parent_hash);
        if extends {
            self.accept(rpc, block, observed, stop, sink)
        } else {
            self.reorg(rpc, block, observed, stop, sink)
        }
    }

    /// `tip` does not extend the kept head: walk its parent links down to
    /// a kept block (the fork point), report the orphaned blocks and follow
    /// the new chain. Without a kept ancestor, report every kept block as
    /// orphaned and follow from `tip`.
    fn reorg<S, C>(
        &mut self,
        rpc: &mut Rpc<S, C>,
        tip: FullBlock,
        tip_observed: FirstHeld,
        stop: &AtomicBool,
        sink: &mut dyn FnMut(CanonicalEvent),
    ) -> Result<(), FollowerFault>
    where
        S: Read + Write,
        C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
    {
        let new_head = (tip.head.header.number, tip.head.header.hash);
        // The new chain from the tip downwards. Bounded: each round goes
        // one height down and ends below the kept window.
        let mut chain = vec![(tip, tip_observed)];
        let fork = loop {
            let Some(child) = chain.last().map(|(block, _)| block.head.header) else {
                break None;
            };
            let Some((kept_number, kept_hash)) =
                child.number.checked_sub(1).and_then(|h| self.kept_at(h))
            else {
                break None;
            };
            if kept_hash == child.parent_hash {
                break Some(kept_number);
            }
            let (parent, observed) = self.fetch_block(rpc, kept_number, stop)?;
            if parent.head.header.hash != child.parent_hash {
                return Err(FollowerFault::ChainMoved(kept_number));
            }
            chain.push((parent, observed));
        };
        match fork {
            Some(fork) => {
                let mut orphaned = Vec::new();
                while let Some(head) = self.kept.back().filter(|(number, _)| *number > fork) {
                    orphaned.push(*head);
                    self.kept.pop_back();
                }
                orphaned.reverse();
                self.facts_due = true;
                // The new chain is followed from the fork point on: should
                // its first block fail and a gap follow, the gap starts here.
                self.next = Some(fork.saturating_add(1));
                sink(CanonicalEvent::Reorg { orphaned, new_head });
            }
            None => {
                let orphaned = self.kept.drain(..).collect();
                self.facts_due = true;
                self.next = Some(new_head.0);
                sink(CanonicalEvent::ReorgTooDeep { orphaned, new_head });
                chain.truncate(1);
            }
        }
        for (block, observed) in chain.into_iter().rev() {
            self.accept(rpc, block, observed, stop, sink)?;
        }
        Ok(())
    }

    /// Fetches the receipts of `block`, verifies them against it and the
    /// launch transactions from the block's own objects, then reports the
    /// block (with all its transactions) and its launches and keeps it.
    /// Nothing is reported unless all of it succeeded.
    fn accept<S, C>(
        &mut self,
        rpc: &mut Rpc<S, C>,
        block: FullBlock,
        observed: FirstHeld,
        stop: &AtomicBool,
        sink: &mut dyn FnMut(CanonicalEvent),
    ) -> Result<(), FollowerFault>
    where
        S: Read + Write,
        C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
    {
        let FullBlock { head, objects } = block;
        let header = head.header;
        let receipts = rpc.receipts(header.number, stop)?;
        if !receipts_match(&head, &receipts) {
            return Err(FollowerFault::ReceiptsMismatch(header.number));
        }
        let mut launches = Vec::new();
        for launch in launches::detect(&head, &receipts) {
            let object = usize::try_from(launch.tx_index)
                .ok()
                .and_then(|index| objects.get(index))
                .ok_or(FollowerFault::ReceiptsMismatch(header.number))?;
            let tx = match launches::verify_object(object) {
                Ok(tx) => Ok(tx),
                Err(reason) => {
                    // A refusal for a property of the transaction is final
                    // at once; one that may be the provider's is fetched
                    // again with the block up to `tx_attempts` times, then
                    // carried by the launch.
                    let tried = match self.refusals.iter_mut().find(|(h, _)| *h == launch.tx_hash) {
                        Some((_, n)) => {
                            *n += 1;
                            *n
                        }
                        None => {
                            self.refusals.push((launch.tx_hash, 1));
                            1
                        }
                    };
                    if !is_unrebuildable(reason) && tried < self.tx_attempts {
                        return Err(FollowerFault::Request {
                            call: Call::Block(header.number),
                            error: RequestError::Unverifiable(reason),
                        });
                    }
                    Err(reason)
                }
            };
            launches.push(
                launch.with_transaction(tx.as_ref().map_err(|r| *r), self.max_launch_calldata),
            );
        }
        let txs = head
            .txs
            .iter()
            .zip(&receipts)
            .enumerate()
            .map(|(index, (tx, receipt))| CanonicalTx {
                hash: tx.hash,
                index: index as u64,
                sender: tx.from,
                nonce: tx.nonce,
                succeeded: receipt.succeeded,
            })
            .collect();
        sink(CanonicalEvent::Block {
            number: header.number,
            hash: header.hash,
            parent_hash: header.parent_hash,
            timestamp_s: header.timestamp_s,
            timestamp_ms: header.block_ms,
            observed: observed.at,
            observed_unix_ms: observed.unix_ms,
            txs,
        });
        for launch in launches {
            sink(CanonicalEvent::Launch(launch));
        }
        self.kept.push_back((header.number, header.hash));
        while self.kept.len() as u64 > self.reorg_depth.saturating_add(1) {
            self.kept.pop_front();
        }
        self.next = Some(header.number.saturating_add(1));
        self.seen.retain(|(number, _, _)| *number > header.number);
        self.refusals.clear();
        self.nonce_facts(rpc, header.number, header.hash, stop, sink)
    }

    /// The watched senders' next nonces at the block just reported, when
    /// due. A failed read is a fault; the next block carries the facts.
    fn nonce_facts<S, C>(
        &mut self,
        rpc: &mut Rpc<S, C>,
        number: u64,
        hash: [u8; 32],
        stop: &AtomicBool,
        sink: &mut dyn FnMut(CanonicalEvent),
    ) -> Result<(), FollowerFault>
    where
        S: Read + Write,
        C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
    {
        let every = self.watch.every_blocks.max(1);
        if self.watch.senders.is_empty() || !(self.facts_due || number.is_multiple_of(every)) {
            return Ok(());
        }
        self.facts_due = true;
        for i in 0..self.watch.senders.len() {
            let sender = self.watch.senders[i];
            let next_nonce = rpc.transaction_count(sender, hash, stop)?;
            sink(CanonicalEvent::NonceFact {
                sender,
                next_nonce,
                block_number: number,
                block_hash: hash,
            });
        }
        self.facts_due = false;
        Ok(())
    }

    /// The block `number` and when this follower first held its header.
    fn fetch_block<S, C>(
        &mut self,
        rpc: &mut Rpc<S, C>,
        number: u64,
        stop: &AtomicBool,
    ) -> Result<(FullBlock, FirstHeld), FollowerFault>
    where
        S: Read + Write,
        C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
    {
        let block = rpc.block(number, stop)?;
        let now = FirstHeld::now();
        let id = (block.head.header.number, block.head.header.hash);
        let observed = match self.seen.iter().find(|(n, h, _)| (*n, *h) == id) {
            Some((_, _, first)) => *first,
            None => {
                self.seen.push_back((id.0, id.1, now));
                while self.seen.len() as u64 > self.reorg_depth.saturating_add(2) {
                    self.seen.pop_front();
                }
                now
            }
        };
        Ok((block, observed))
    }

    fn kept_at(&self, number: u64) -> Option<(u64, [u8; 32])> {
        let front = self.kept.front()?.0;
        let index = usize::try_from(number.checked_sub(front)?).ok()?;
        self.kept.get(index).copied()
    }
}

/// The receipts are exactly the block's transactions: same count, in
/// order, each with its transaction's hash and sender, bound to the block's
/// hash and number, with the block's log indices contiguous from 0.
pub(super) fn receipts_match(block: &BlockHead, receipts: &[Receipt]) -> bool {
    if receipts.len() != block.txs.len() {
        return false;
    }
    let mut next_log = 0u64;
    for (index, (receipt, tx)) in receipts.iter().zip(&block.txs).enumerate() {
        let bound = receipt.tx_index == index as u64
            && receipt.tx_hash == tx.hash
            && receipt.from == tx.from
            && receipt.block_hash == block.header.hash
            && receipt.block_number == block.header.number;
        if !bound {
            return false;
        }
        for log in &receipt.logs {
            if log.log_index != next_log {
                return false;
            }
            next_log += 1;
        }
    }
    true
}

//! Joins what landed (the canonical chain follower) with what the signal
//! sources delivered before it (the [`IngressCore`](crate::ingress::IngressCore)
//! sightings): the G0.4 denominator and its visibility classes.
//!
//! "Before the block" means before the block's own time: the header's
//! millisecond timestamp (`Header.MilliTimestamp()` of bsc-geth, the
//! validator's wall clock). A sighting's time is the runtime's wall-clock
//! estimate of the moment it was ingested (a monotonic reading mapped through
//! a monotonic/wall pair refreshed at least once per runtime pass). The two
//! are different clocks: the comparison carries the local clock's offset to
//! the validators, which the census bounds with the header fetch lag (local
//! wall clock when the follower first held the header minus the block time;
//! a negative lag proves the local clock is behind). A sighting without a
//! wall estimate is `ClockUnknown`, never guessed.
//!
//! The monotonic lead before the follower's first header fetch is kept as a
//! diagnostic only (it includes the follower's own lag).
//!
//! A launch is joined `delay_ns` after its block was fetched, so sightings
//! still queued behind the block in the runtime's channel are counted. A
//! reorg takes the launches of every orphaned block out of the counts again
//! (they stay visible as `orphaned`); the window that can still be orphaned
//! is the follower's reorg depth.

pub use crate::chain_follower::{FourToken, Venue};
use crate::ingress::{FirstSeen, Sighting};
use crate::sources::SourceId;
use std::collections::VecDeque;

/// How a canonical launch was visible before its block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    /// A verified full (actionable) sighting at or before the block time.
    FullBefore { source: SourceId, lead_us: u64 },
    /// Only its hash before the block time: not actionable.
    HashOnlyBefore { source: SourceId, lead_us: u64 },
    /// First delivered pending only after the block time.
    After { source: SourceId, lag_us: u64 },
    /// No source delivered it while it was remembered.
    Missed,
    /// A deciding sighting has no wall-clock estimate.
    ClockUnknown { source: SourceId },
}

impl Visibility {
    /// The journal byte of the class.
    pub fn code(self) -> u8 {
        match self {
            Visibility::FullBefore { .. } => 1,
            Visibility::HashOnlyBefore { .. } => 2,
            Visibility::After { .. } => 3,
            Visibility::Missed => 4,
            Visibility::ClockUnknown { .. } => 5,
        }
    }

    /// The source and the time between sighting and block (either way, µs).
    pub fn source_and_delta(self) -> Option<(SourceId, u64)> {
        match self {
            Visibility::FullBefore { source, lead_us }
            | Visibility::HashOnlyBefore { source, lead_us } => Some((source, lead_us)),
            Visibility::After { source, lag_us } => Some((source, lag_us)),
            Visibility::ClockUnknown { source } => Some((source, 0)),
            Visibility::Missed => None,
        }
    }
}

/// The class of a sighting relative to the block time `block_ms`.
pub fn classify(sighting: Option<Sighting>, block_ms: u64) -> Visibility {
    let Some(s) = sighting else {
        return Visibility::Missed;
    };
    let block_us = block_ms.saturating_mul(1_000);
    let before = |seen: FirstSeen| seen.wall_us.map(|w| (w <= block_us, w));
    if let Some(full) = s.first_full {
        match before(full) {
            Some((true, wall)) => {
                return Visibility::FullBefore {
                    source: full.source,
                    lead_us: block_us - wall,
                };
            }
            Some((false, _)) => {}
            None => {
                return Visibility::ClockUnknown {
                    source: full.source,
                };
            }
        }
    }
    let any = s.first_any;
    match before(any) {
        Some((true, wall)) => Visibility::HashOnlyBefore {
            source: any.source,
            lead_us: block_us - wall,
        },
        Some((false, wall)) => Visibility::After {
            source: any.source,
            lag_us: wall - block_us,
        },
        None => Visibility::ClockUnknown { source: any.source },
    }
}

/// What a launch transaction is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchTx {
    /// Verified; `c1_scope` holds.
    Verified,
    /// A type or form the verifier does not rebuild (EIP-4844/7702,
    /// pre-EIP-155): never actionable by the C1 path.
    Unrebuildable,
    /// The provider's answers were refused (malformed, a hash or signer
    /// mismatch) every time it was fetched: unknown, not C1 scope.
    AnswerRefused,
}

/// One canonical launch as the join needs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchRef {
    pub venue: Venue,
    pub block_number: u64,
    pub block_hash: [u8; 32],
    pub tx_index: u64,
    pub tx_hash: [u8; 32],
    /// The created token, where the venue's event layout is proven.
    pub token: Option<[u8; 20]>,
    /// Buys of the token after the launch in its block; `None` unknown.
    pub external_buys: Option<u32>,
    /// The launch transaction is a direct call of the C1 scope (Portal
    /// `newTokenV6`): what the pending path can act on.
    pub c1_scope: bool,
    /// Four.meme: the pending decoder's token against the canonical one.
    pub four_token: FourToken,
    /// What the launch transaction is (verified, or why not).
    pub tx: LaunchTx,
    /// The block's time (header milliseconds, validator wall clock).
    pub block_ms: u64,
    /// When the follower first held the block's header: monotonic ns of the
    /// runtime clock (diagnostic) and local wall ms.
    pub block_seen_ns: u64,
    pub block_seen_unix_ms: Option<u64>,
}

impl LaunchRef {
    /// Local wall clock at the header fetch minus the block time (ms); a
    /// negative value proves the local clock is behind the validator's.
    pub fn fetch_lag_ms(&self) -> Option<i64> {
        let seen = i64::try_from(self.block_seen_unix_ms?).ok()?;
        let block = i64::try_from(self.block_ms).ok()?;
        seen.checked_sub(block)
    }
}

/// A launch whose join is decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Joined {
    pub launch: LaunchRef,
    pub visibility: Visibility,
}

/// The counters of the runtime status (`runtime_status::canonical`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JoinCounters {
    pub flap_launches: u64,
    /// Four.meme launches (Classic and OpenFour), and those whose verified
    /// signed bytes the production Four decoder recognizes as a launch with
    /// the canonical token derived before execution (Phase E).
    pub four_launches: u64,
    pub four_pending_decoded: u64,
    /// Four.meme launches decoded pending without a proven derivation (an
    /// unfrozen OpenFour preset), and with a derived token that is not the
    /// canonical one.
    pub four_pending_unknown: u64,
    pub four_pending_mismatch: u64,
    pub orphaned_launches: u64,
    pub seen_full_before: u64,
    pub seen_hash_only_before: u64,
    pub seen_after: u64,
    pub missed: u64,
    pub clock_unknown: u64,
    /// Launches (any venue) whose transaction type/form is not rebuilt, and
    /// those whose provider answers were refused every time.
    pub tx_unverifiable: u64,
    pub tx_answer_refused: u64,
    /// Flap launches of the C1 scope, and those seen full before the block.
    pub c1_scope_launches: u64,
    pub c1_scope_full_before: u64,
    /// Sum over the Flap launches whose buys are known.
    pub external_buys: u64,
    /// Flap launches whose external buys are unknown (never counted as 0).
    pub buys_unknown: u64,
}

impl JoinCounters {
    fn apply(&mut self, j: &Joined, add: bool) {
        let step = |c: &mut u64| {
            *c = if add {
                c.saturating_add(1)
            } else {
                c.saturating_sub(1)
            }
        };
        match j.launch.tx {
            LaunchTx::Verified => {}
            LaunchTx::Unrebuildable => step(&mut self.tx_unverifiable),
            LaunchTx::AnswerRefused => step(&mut self.tx_answer_refused),
        }
        if j.launch.venue != Venue::Flap {
            step(&mut self.four_launches);
            match j.launch.four_token {
                FourToken::Matched => step(&mut self.four_pending_decoded),
                FourToken::Unknown => step(&mut self.four_pending_unknown),
                FourToken::Mismatch => step(&mut self.four_pending_mismatch),
                FourToken::NotApplicable | FourToken::NotRecognized => {}
            }
            return;
        }
        step(&mut self.flap_launches);
        match j.visibility {
            Visibility::FullBefore { .. } => step(&mut self.seen_full_before),
            Visibility::HashOnlyBefore { .. } => step(&mut self.seen_hash_only_before),
            Visibility::After { .. } => step(&mut self.seen_after),
            Visibility::Missed => step(&mut self.missed),
            Visibility::ClockUnknown { .. } => step(&mut self.clock_unknown),
        }
        if j.launch.c1_scope {
            step(&mut self.c1_scope_launches);
            if matches!(j.visibility, Visibility::FullBefore { .. }) {
                step(&mut self.c1_scope_full_before);
            }
        }
        let Some(buys) = j.launch.external_buys.map(u64::from) else {
            step(&mut self.buys_unknown);
            return;
        };
        self.external_buys = if add {
            self.external_buys.saturating_add(buys)
        } else {
            self.external_buys.saturating_sub(buys)
        };
    }
}

/// Launches waiting for their join, and the joined ones a reorg can still
/// orphan.
pub struct Ledger {
    delay_ns: u64,
    reorg_depth: u64,
    waiting: VecDeque<LaunchRef>,
    recent: VecDeque<Joined>,
    counters: JoinCounters,
}

impl Ledger {
    pub fn new(delay_ns: u64, reorg_depth: u64) -> Self {
        Ledger {
            delay_ns,
            reorg_depth,
            waiting: VecDeque::new(),
            recent: VecDeque::new(),
            counters: JoinCounters::default(),
        }
    }

    pub fn counters(&self) -> JoinCounters {
        self.counters
    }

    /// Launches received but not joined yet.
    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// A launch of a canonical block, joined once `delay_ns` passed.
    pub fn push(&mut self, launch: LaunchRef) {
        self.waiting.push_back(launch);
    }

    /// Joins every launch whose delay passed at `now_ns`; returns them in
    /// block order. `head` bounds the window a reorg can still reach.
    pub fn join_due(
        &mut self,
        now_ns: u64,
        head: u64,
        sighting: impl Fn(&[u8; 32]) -> Option<Sighting>,
    ) -> Vec<Joined> {
        let mut out = Vec::new();
        while let Some(l) = self.waiting.front() {
            if now_ns.saturating_sub(l.block_seen_ns) < self.delay_ns {
                break;
            }
            let Some(launch) = self.waiting.pop_front() else {
                break;
            };
            let joined = Joined {
                launch,
                visibility: classify(sighting(&launch.tx_hash), launch.block_ms),
            };
            self.counters.apply(&joined, true);
            self.recent.push_back(joined);
            out.push(joined);
        }
        let floor = head.saturating_sub(self.reorg_depth);
        while self
            .recent
            .front()
            .is_some_and(|j| j.launch.block_number < floor)
        {
            self.recent.pop_front();
        }
        out
    }

    /// Takes the launches of the orphaned blocks out of the counts; returns
    /// them (joined or still waiting).
    pub fn orphan(&mut self, orphaned: &[[u8; 32]]) -> Vec<LaunchRef> {
        let mut out = Vec::new();
        let hit = |hash: &[u8; 32]| orphaned.contains(hash);
        self.waiting.retain(|l| {
            let gone = hit(&l.block_hash);
            if gone {
                out.push(*l);
            }
            !gone
        });
        let mut kept = VecDeque::with_capacity(self.recent.len());
        for j in self.recent.drain(..) {
            if hit(&j.launch.block_hash) {
                self.counters.apply(&j, false);
                out.push(j.launch);
            } else {
                kept.push_back(j);
            }
        }
        self.recent = kept;
        self.counters.orphaned_launches = self
            .counters
            .orphaned_launches
            .saturating_add(out.len() as u64);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK_MS: u64 = 1_000;

    fn launch(
        venue: Venue,
        block: u64,
        hash: u8,
        tx: u8,
        seen: u64,
        buys: Option<u32>,
    ) -> LaunchRef {
        LaunchRef {
            venue,
            block_number: block,
            block_hash: [hash; 32],
            tx_index: u64::from(tx),
            tx_hash: [tx; 32],
            token: None,
            external_buys: buys,
            c1_scope: tx % 2 == 1,
            four_token: match (venue, tx % 2 == 1) {
                (Venue::Flap, _) => FourToken::NotApplicable,
                (_, true) => FourToken::Matched,
                (_, false) => FourToken::Unknown,
            },
            tx: LaunchTx::Verified,
            block_ms: BLOCK_MS,
            block_seen_ns: seen,
            block_seen_unix_ms: Some(BLOCK_MS + 300),
        }
    }

    fn at(source: SourceId, at_ns: u64, wall_us: Option<u64>) -> FirstSeen {
        FirstSeen {
            source,
            at_ns,
            wall_us,
        }
    }

    fn seen(any: FirstSeen, full: Option<FirstSeen>) -> Option<Sighting> {
        Some(Sighting {
            first_any: any,
            first_full: full,
        })
    }

    #[test]
    fn the_class_is_decided_against_the_block_time_never_the_fetch() {
        let p = SourceId::PublicNode;
        let d = SourceId::Drpc;
        let block_us = BLOCK_MS * 1_000;
        assert_eq!(classify(None, BLOCK_MS), Visibility::Missed);
        assert_eq!(
            classify(
                seen(
                    at(d, 1, Some(block_us - 90)),
                    Some(at(p, 2, Some(block_us - 60)))
                ),
                BLOCK_MS
            ),
            Visibility::FullBefore {
                source: p,
                lead_us: 60
            }
        );
        // Equal time counts as before.
        assert_eq!(
            classify(
                seen(at(p, 1, Some(block_us)), Some(at(p, 1, Some(block_us)))),
                BLOCK_MS
            ),
            Visibility::FullBefore {
                source: p,
                lead_us: 0
            }
        );
        // Seen after the block existed, although maybe before the follower
        // fetched it: after (the review's failure case).
        assert_eq!(
            classify(
                seen(
                    at(p, 9, Some(block_us + 200)),
                    Some(at(p, 9, Some(block_us + 200)))
                ),
                BLOCK_MS
            ),
            Visibility::After {
                source: p,
                lag_us: 200
            }
        );
        // A hash before, the full transaction only after: not actionable.
        assert_eq!(
            classify(
                seen(
                    at(d, 1, Some(block_us - 90)),
                    Some(at(p, 2, Some(block_us + 40)))
                ),
                BLOCK_MS
            ),
            Visibility::HashOnlyBefore {
                source: d,
                lead_us: 90
            }
        );
        // No wall estimate for the deciding sighting: unknown, never a class.
        assert_eq!(
            classify(
                seen(at(d, 1, Some(block_us - 90)), Some(at(p, 2, None))),
                BLOCK_MS
            ),
            Visibility::ClockUnknown { source: p }
        );
        assert_eq!(
            classify(seen(at(d, 1, None), None), BLOCK_MS),
            Visibility::ClockUnknown { source: d }
        );
    }

    #[test]
    fn the_fetch_lag_is_signed_and_unknown_without_a_clock() {
        let mut l = launch(Venue::Flap, 1, 1, 1, 0, Some(0));
        assert_eq!(l.fetch_lag_ms(), Some(300));
        l.block_seen_unix_ms = Some(BLOCK_MS - 20);
        assert_eq!(l.fetch_lag_ms(), Some(-20));
        l.block_seen_unix_ms = None;
        assert_eq!(l.fetch_lag_ms(), None);
    }

    #[test]
    fn launches_join_after_the_delay_and_a_reorg_takes_them_out_again() {
        let p = SourceId::PublicNode;
        let block_us = BLOCK_MS * 1_000;
        let mut ledger = Ledger::new(50, 3);
        ledger.push(launch(Venue::Flap, 10, 0xA, 1, 100, Some(2)));
        ledger.push(launch(Venue::Four, 10, 0xA, 2, 100, None));
        // Phase E: an OpenFour launch whose pending bytes the Four decoder
        // recognized (odd tx: `c1_scope`) counts with the Four family.
        ledger.push(launch(Venue::OpenFour, 10, 0xA, 5, 100, None));
        ledger.push(launch(Venue::Flap, 11, 0xB, 3, 200, Some(1)));
        let sighting = |tx: &[u8; 32]| match tx[0] {
            1 => seen(
                at(p, 60, Some(block_us - 40)),
                Some(at(p, 60, Some(block_us - 40))),
            ),
            3 => seen(
                at(p, 230, Some(block_us + 30)),
                Some(at(p, 230, Some(block_us + 30))),
            ),
            _ => None,
        };
        assert!(ledger.join_due(120, 11, sighting).is_empty());
        assert_eq!(ledger.waiting(), 4);
        let first = ledger.join_due(160, 11, sighting);
        assert_eq!(first.len(), 3);
        assert_eq!(
            first[0].visibility,
            Visibility::FullBefore {
                source: p,
                lead_us: 40
            }
        );
        let second = ledger.join_due(260, 11, sighting);
        assert_eq!(
            second[0].visibility,
            Visibility::After {
                source: p,
                lag_us: 30
            }
        );
        let c = ledger.counters();
        assert_eq!(
            (
                c.flap_launches,
                c.four_launches,
                c.seen_full_before,
                c.seen_after,
                c.external_buys,
                c.c1_scope_launches,
                c.c1_scope_full_before,
                c.four_pending_decoded
            ),
            (2, 2, 1, 1, 3, 2, 1, 1)
        );
        // Block 11 is orphaned: its launch leaves every count.
        let gone = ledger.orphan(&[[0xB; 32]]);
        assert_eq!(gone.len(), 1);
        let c = ledger.counters();
        assert_eq!(
            (
                c.flap_launches,
                c.seen_after,
                c.external_buys,
                c.orphaned_launches,
                c.c1_scope_launches
            ),
            (1, 0, 2, 1, 1)
        );
        // A waiting launch of an orphaned block never joins.
        ledger.push(launch(Venue::Flap, 12, 0xC, 4, 300, None));
        assert_eq!(ledger.orphan(&[[0xC; 32]]).len(), 1);
        assert!(ledger.join_due(10_000, 12, sighting).is_empty());
        assert_eq!(ledger.counters().orphaned_launches, 2);
    }

    #[test]
    fn only_the_reorg_window_is_kept() {
        let mut ledger = Ledger::new(0, 2);
        for b in 0..10u64 {
            ledger.push(launch(Venue::Flap, b, b as u8, b as u8, b, Some(0)));
            ledger.join_due(b, b, |_| None);
        }
        assert!(ledger.recent.len() <= 3);
        assert!(ledger.orphan(&[[0; 32]]).is_empty());
        assert_eq!(ledger.counters().missed, 10);
    }
}

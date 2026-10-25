//! Joins what landed (the canonical chain follower) with what the signal
//! sources delivered before it (the [`IngressCore`](crate::ingress::IngressCore)
//! sightings): the G0.4 denominator and its visibility classes.
//!
//! Both times are readings of the runtime's one monotonic clock: the first
//! sighting of the launch transaction, and the moment the follower first had
//! the block's header. No wall-clock time is subtracted from them.
//!
//! A launch is joined `delay_ns` after its block was seen, so sightings still
//! queued behind the block in the runtime's channel are counted. A reorg
//! takes the launches of every orphaned block out of the counts again (they
//! stay visible as `orphaned`); the window that can still be orphaned is the
//! follower's reorg depth.

pub use crate::chain_follower::Venue;
use crate::ingress::Sighting;
use crate::sources::SourceId;
use std::collections::VecDeque;

/// How a canonical launch was visible before its block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    /// A verified full (actionable) sighting at or before the block.
    FullBefore { source: SourceId, lead_ns: u64 },
    /// Only its hash before the block: not actionable.
    HashOnlyBefore { source: SourceId, lead_ns: u64 },
    /// First delivered pending only after the block.
    After { source: SourceId, lag_ns: u64 },
    /// No source delivered it while it was remembered.
    Missed,
}

impl Visibility {
    /// The journal byte of the class.
    pub fn code(self) -> u8 {
        match self {
            Visibility::FullBefore { .. } => 1,
            Visibility::HashOnlyBefore { .. } => 2,
            Visibility::After { .. } => 3,
            Visibility::Missed => 4,
        }
    }

    /// The source and the time between sighting and block (either way).
    pub fn source_and_delta(self) -> Option<(SourceId, u64)> {
        match self {
            Visibility::FullBefore { source, lead_ns }
            | Visibility::HashOnlyBefore { source, lead_ns } => Some((source, lead_ns)),
            Visibility::After { source, lag_ns } => Some((source, lag_ns)),
            Visibility::Missed => None,
        }
    }
}

/// The class of a sighting relative to the block seen at `block_seen_ns`.
pub fn classify(sighting: Option<Sighting>, block_seen_ns: u64) -> Visibility {
    let Some(s) = sighting else {
        return Visibility::Missed;
    };
    if let Some((source, at)) = s.first_full
        && at <= block_seen_ns
    {
        return Visibility::FullBefore {
            source,
            lead_ns: block_seen_ns - at,
        };
    }
    let (source, at) = s.first_any;
    if at <= block_seen_ns {
        return Visibility::HashOnlyBefore {
            source,
            lead_ns: block_seen_ns - at,
        };
    }
    Visibility::After {
        source,
        lag_ns: at - block_seen_ns,
    }
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
    /// When the follower first had the block's header (monotonic ns).
    pub block_seen_ns: u64,
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
    pub four_launches: u64,
    pub orphaned_launches: u64,
    pub seen_full_before: u64,
    pub seen_hash_only_before: u64,
    pub seen_after: u64,
    pub missed: u64,
    /// Sum over the launches whose buys are known.
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
        if j.launch.venue == Venue::Four {
            step(&mut self.four_launches);
            return;
        }
        step(&mut self.flap_launches);
        match j.visibility {
            Visibility::FullBefore { .. } => step(&mut self.seen_full_before),
            Visibility::HashOnlyBefore { .. } => step(&mut self.seen_hash_only_before),
            Visibility::After { .. } => step(&mut self.seen_after),
            Visibility::Missed => step(&mut self.missed),
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
                visibility: classify(sighting(&launch.tx_hash), launch.block_seen_ns),
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

    fn launch(venue: Venue, block: u64, hash: u8, tx: u8, seen: u64, buys: u32) -> LaunchRef {
        LaunchRef {
            venue,
            block_number: block,
            block_hash: [hash; 32],
            tx_index: u64::from(tx),
            tx_hash: [tx; 32],
            token: None,
            external_buys: Some(buys),
            block_seen_ns: seen,
        }
    }

    fn seen(any: (SourceId, u64), full: Option<(SourceId, u64)>) -> Option<Sighting> {
        Some(Sighting {
            first_any: any,
            first_full: full,
        })
    }

    #[test]
    fn the_class_follows_the_first_sightings_on_one_clock() {
        let p = SourceId::PublicNode;
        let d = SourceId::Drpc;
        assert_eq!(classify(None, 100), Visibility::Missed);
        assert_eq!(
            classify(seen((d, 10), Some((p, 40))), 100),
            Visibility::FullBefore {
                source: p,
                lead_ns: 60
            }
        );
        // Equal time counts as before (the sighting was there with the block).
        assert_eq!(
            classify(seen((p, 100), Some((p, 100))), 100),
            Visibility::FullBefore {
                source: p,
                lead_ns: 0
            }
        );
        // A hash before, the full transaction only after: not actionable.
        assert_eq!(
            classify(seen((d, 10), Some((p, 140))), 100),
            Visibility::HashOnlyBefore {
                source: d,
                lead_ns: 90
            }
        );
        assert_eq!(
            classify(seen((d, 130), None), 100),
            Visibility::After {
                source: d,
                lag_ns: 30
            }
        );
    }

    #[test]
    fn launches_join_after_the_delay_and_a_reorg_takes_them_out_again() {
        let p = SourceId::PublicNode;
        let mut ledger = Ledger::new(50, 3);
        ledger.push(launch(Venue::Flap, 10, 0xA, 1, 100, 2));
        ledger.push(launch(Venue::Four, 10, 0xA, 2, 100, 0));
        ledger.push(launch(Venue::Flap, 11, 0xB, 3, 200, 1));
        let sighting = |tx: &[u8; 32]| match tx[0] {
            1 => seen((p, 60), Some((p, 60))),
            3 => seen((p, 230), Some((p, 230))),
            _ => None,
        };
        // Not yet due.
        assert!(ledger.join_due(120, 11, sighting).is_empty());
        let first = ledger.join_due(160, 11, sighting);
        assert_eq!(first.len(), 2);
        assert_eq!(
            first[0].visibility,
            Visibility::FullBefore {
                source: p,
                lead_ns: 40
            }
        );
        // A sighting that arrived before the join (queued) but after the block.
        let second = ledger.join_due(260, 11, sighting);
        assert_eq!(
            second[0].visibility,
            Visibility::After {
                source: p,
                lag_ns: 30
            }
        );
        let c = ledger.counters();
        assert_eq!(
            (
                c.flap_launches,
                c.four_launches,
                c.seen_full_before,
                c.seen_after,
                c.external_buys
            ),
            (2, 1, 1, 1, 3)
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
                c.orphaned_launches
            ),
            (1, 0, 2, 1)
        );
        // A waiting launch of an orphaned block never joins.
        ledger.push(launch(Venue::Flap, 12, 0xC, 4, 300, 0));
        assert_eq!(ledger.orphan(&[[0xC; 32]]).len(), 1);
        assert!(ledger.join_due(10_000, 12, sighting).is_empty());
        assert_eq!(ledger.counters().orphaned_launches, 2);
    }

    #[test]
    fn only_the_reorg_window_is_kept() {
        let mut ledger = Ledger::new(0, 2);
        for b in 0..10u64 {
            ledger.push(launch(Venue::Flap, b, b as u8, b as u8, b, 0));
            ledger.join_due(b, b, |_| None);
        }
        assert!(ledger.recent.len() <= 3);
        // Too old to be orphaned by the follower: the count stays.
        assert!(ledger.orphan(&[[0; 32]]).is_empty());
        assert_eq!(ledger.counters().missed, 10);
    }
}

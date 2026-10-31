//! BSC3-056: the followed canonical chain against an independent second
//! source. Both are kept as rings of recent (height, hash); a LIVE BUY is
//! released only while they agree: at the highest height both hold the
//! hashes are equal, the second source's newest header is fresh and at
//! most `max_lag_blocks` below the followed head. The second source is
//! read off the hot path (its own thread); the verdict is read in O(1).

use std::collections::VecDeque;

/// Recent headers of one chain, ascending, consecutive as reported.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ring {
    headers: VecDeque<(u64, [u8; 32])>,
    /// The newest header's millisecond timestamp.
    newest_ms: Option<u64>,
    capacity: usize,
}

impl Ring {
    pub fn new(capacity: usize) -> Self {
        Self {
            headers: VecDeque::with_capacity(capacity),
            newest_ms: None,
            capacity: capacity.max(1),
        }
    }

    /// Header `number` with `hash` on `parent`: what was kept at or above
    /// `number` left the chain; a parent other than the kept one below it
    /// means the source reorganized below what is kept: only this header
    /// is known.
    pub fn push(&mut self, number: u64, hash: [u8; 32], parent: [u8; 32], block_ms: u64) {
        while self.headers.back().is_some_and(|(n, _)| *n >= number) {
            self.headers.pop_back();
        }
        let linked = self
            .headers
            .back()
            .is_some_and(|(n, h)| n.checked_add(1) == Some(number) && *h == parent);
        if !linked {
            self.headers.clear();
        }
        self.headers.push_back((number, hash));
        while self.headers.len() > self.capacity {
            self.headers.pop_front();
        }
        self.newest_ms = Some(block_ms);
    }

    /// The followed chain orphaned these blocks (a reorg).
    pub fn orphan(&mut self, orphaned: &[(u64, [u8; 32])]) {
        self.headers.retain(|b| !orphaned.contains(b));
    }

    pub fn head(&self) -> Option<u64> {
        self.headers.back().map(|(n, _)| *n)
    }

    fn at(&self, number: u64) -> Option<[u8; 32]> {
        let first = self.headers.front()?.0;
        let i = usize::try_from(number.checked_sub(first)?).ok()?;
        self.headers
            .get(i)
            .filter(|(n, _)| *n == number)
            .map(|(_, h)| *h)
    }
}

/// Whether the second source confirms the followed chain now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Equal hashes at `height`, the highest both hold.
    Agreed { height: u64 },
    /// No header of the second source yet, or its newest is `age_ms` old.
    Stale { age_ms: Option<u64> },
    /// The second source's newest is `by` blocks below the followed head.
    Behind { by: u64 },
    /// Different hashes at `height`.
    Disagrees { height: u64 },
    /// No height both hold (one of them reorganized or jumped).
    NoCommonHeight,
}

impl Verdict {
    pub fn name(self) -> &'static str {
        match self {
            Self::Agreed { .. } => "AGREED",
            Self::Stale { .. } => "STALE",
            Self::Behind { .. } => "BEHIND",
            Self::Disagrees { .. } => "DISAGREES",
            Self::NoCommonHeight => "NO_COMMON_HEIGHT",
        }
    }
}

/// A verdict with the lag it was judged under: what a BUY planned on a
/// head reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Confirmation {
    pub verdict: Verdict,
    pub max_lag_blocks: u64,
}

impl Confirmation {
    /// Whether a BUY planned on canonical head `head` may go: AGREED at a
    /// height at most `max_lag_blocks` below it (a newer head than the
    /// verdict's is not confirmed beyond that); otherwise why not.
    pub fn confirms(&self, head: u64) -> Result<(), String> {
        match self.verdict {
            Verdict::Agreed { height } if height.saturating_add(self.max_lag_blocks) >= head => {
                Ok(())
            }
            Verdict::Agreed { height } => Err(format!(
                "SECOND_SOURCE_BEHIND_HEAD: agreed at {height}, the BUY plans on head {head} (at most {} below)",
                self.max_lag_blocks
            )),
            v => Err(format!("SECOND_SOURCE_{}: {v:?}", v.name())),
        }
    }
}

/// The verdict of `followed` against `second` at `now_ms` (wall clock):
/// the second source's newest header at most `max_age_ms` old and at most
/// `max_lag_blocks` below the followed head, the hashes equal at the
/// highest height both hold.
pub fn verdict(
    followed: &Ring,
    second: &Ring,
    now_ms: u64,
    max_age_ms: u64,
    max_lag_blocks: u64,
) -> Verdict {
    let (Some(theirs), Some(newest_ms)) = (second.head(), second.newest_ms) else {
        return Verdict::Stale { age_ms: None };
    };
    let age_ms = now_ms.saturating_sub(newest_ms);
    if age_ms > max_age_ms {
        return Verdict::Stale {
            age_ms: Some(age_ms),
        };
    }
    let Some(ours) = followed.head() else {
        return Verdict::NoCommonHeight;
    };
    if ours.saturating_sub(theirs) > max_lag_blocks {
        return Verdict::Behind { by: ours - theirs };
    }
    let mut height = ours.min(theirs);
    loop {
        match (followed.at(height), second.at(height)) {
            (Some(a), Some(b)) if a == b => return Verdict::Agreed { height },
            (Some(_), Some(_)) => return Verdict::Disagrees { height },
            _ => match height.checked_sub(1) {
                Some(h) if followed.at(h).is_some() || second.at(h).is_some() => height = h,
                _ => return Verdict::NoCommonHeight,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(n: u64, fork: u8) -> [u8; 32] {
        let mut h = [fork; 32];
        h[..8].copy_from_slice(&n.to_be_bytes());
        h
    }

    /// Heights `from..=to` of fork `fork` (parents of the same fork).
    fn ring(from: u64, to: u64, fork: u8, ms: u64) -> Ring {
        let mut r = Ring::new(64);
        for n in from..=to {
            r.push(n, h(n, fork), h(n - 1, fork), ms);
        }
        r
    }

    #[test]
    fn the_chains_agree_only_fresh_close_and_equal() {
        let ours = ring(10, 20, 0, 1_000);
        // Equal at the second source's head, one block behind.
        assert_eq!(
            verdict(&ours, &ring(10, 19, 0, 1_000), 1_500, 3_000, 2),
            Verdict::Agreed { height: 19 }
        );
        // Ahead of ours: compared at our head.
        assert_eq!(
            verdict(&ours, &ring(10, 22, 0, 1_000), 1_500, 3_000, 2),
            Verdict::Agreed { height: 20 }
        );
        // Too old, too far behind, none yet.
        assert_eq!(
            verdict(&ours, &ring(10, 20, 0, 1_000), 5_000, 3_000, 2),
            Verdict::Stale {
                age_ms: Some(4_000)
            }
        );
        assert_eq!(
            verdict(&ours, &ring(10, 17, 0, 1_000), 1_500, 3_000, 2),
            Verdict::Behind { by: 3 }
        );
        assert_eq!(
            verdict(&ours, &Ring::new(64), 1_500, 3_000, 2),
            Verdict::Stale { age_ms: None }
        );
        // Another fork from 18: different at the highest common height.
        let mut fork = ring(10, 17, 0, 1_000);
        fork.push(18, h(18, 7), h(17, 0), 1_000);
        fork.push(19, h(19, 7), h(18, 7), 1_000);
        assert_eq!(
            verdict(&ours, &fork, 1_500, 3_000, 2),
            Verdict::Disagrees { height: 19 }
        );
    }

    #[test]
    fn a_buy_reads_the_verdict_against_its_own_head() {
        let c = |verdict| Confirmation {
            verdict,
            max_lag_blocks: 1,
        };
        assert!(c(Verdict::Agreed { height: 10 }).confirms(10).is_ok());
        assert!(c(Verdict::Agreed { height: 10 }).confirms(11).is_ok());
        // A newer head than the verdict covers: not confirmed.
        let e = c(Verdict::Agreed { height: 10 }).confirms(12).unwrap_err();
        assert!(e.starts_with("SECOND_SOURCE_BEHIND_HEAD"), "{e}");
        let e = c(Verdict::Disagrees { height: 10 })
            .confirms(10)
            .unwrap_err();
        assert!(e.starts_with("SECOND_SOURCE_DISAGREES"), "{e}");
        let zero = Confirmation {
            verdict: Verdict::Agreed { height: 10 },
            max_lag_blocks: 0,
        };
        assert!(zero.confirms(10).is_ok() && zero.confirms(11).is_err());
    }

    #[test]
    fn a_reorg_or_a_jump_keeps_only_what_is_linked() {
        let mut r = ring(10, 15, 0, 1_000);
        // The same height again on another parent: the source reorganized.
        r.push(15, h(15, 1), h(14, 1), 1_100);
        assert_eq!(
            (r.head(), r.at(14), r.at(15)),
            (Some(15), None, Some(h(15, 1)))
        );
        let ours = ring(10, 15, 0, 1_000);
        assert_eq!(
            verdict(&ours, &r, 1_200, 3_000, 2),
            Verdict::Disagrees { height: 15 }
        );
        // A jump: only the new header is known; ours holds it.
        let mut jumped = ring(10, 12, 0, 1_000);
        jumped.push(15, h(15, 0), h(14, 0), 1_000);
        assert_eq!(jumped.at(12), None);
        assert_eq!(
            verdict(&ours, &jumped, 1_200, 3_000, 2),
            Verdict::Agreed { height: 15 }
        );
        // The followed chain orphaned its last two: compared below.
        let mut o = ring(10, 15, 0, 1_000);
        o.orphan(&[(14, h(14, 0)), (15, h(15, 0))]);
        assert_eq!(o.head(), Some(13));
        assert_eq!(
            verdict(&o, &ring(10, 15, 0, 1_000), 1_200, 3_000, 2),
            Verdict::Agreed { height: 13 }
        );
        // Bounded.
        let long = ring(1, 200, 0, 1_000);
        assert_eq!(long.headers.len(), 64);
        assert_eq!(long.at(100), None);
        assert_eq!(long.at(200), Some(h(200, 0)));
    }
}

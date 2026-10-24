//! P6-MT.8 (MT4): the route-cap cache — a hint, never quote truth.
//!
//! Keyed by (input mint, output mint, [`RouteFootprintClass`]), it remembers only the Jupiter
//! `maxAccounts` cap at which the REAL compiled receivership TX last fit. It never holds a
//! route or a quote: every route is still quoted afresh and proven by its own compile
//! (P6-MT.14 #31). The cache lives in the owner's memory only and starts empty on every run.
use solana_sdk::pubkey::Pubkey;
use std::collections::{HashMap, VecDeque};

/// The footprint of the non-route part of the receivership TX — the TX compiled with an empty
/// route at the same version, payer, resources and LUT setup — as its exact locked-account count
/// and serialized length. Two revisions with the same class leave a route exactly the same room,
/// so a cap proven for one is a meaningful hint for the other. Coarser buckets have no basis in
/// existing data: a fit is decided at one account / one byte, and a bucket mixing different
/// headrooms would carry a "fit" or "too large" between TXs it was never proven for. The class
/// therefore repeats exactly for the repeated evaluations of one candidate (the hot case) and
/// across accounts only with an identical non-route TX.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RouteFootprintClass {
    pub locked_accounts: u8,
    pub serialized_len: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RouteCapKey {
    pub input_mint: Pubkey,
    pub output_mint: Pubkey,
    pub class: RouteFootprintClass,
}

/// What one route-fit search proved: the cap at which the compiled TX fit and the smallest cap
/// above it whose quote was rejected by the compile in the same search (`None`: the first quote
/// fit, or the search was already hinted below every rejected cap).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FitTrace {
    pub fit_cap: u8,
    pub rejected_above: Option<u8>,
    /// Quote requests the search made.
    pub attempts: u32,
}

/// The hint for one search: the cap to start from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hint {
    /// No usable entry (or a periodic re-probe): the configured maximum.
    Miss { start_cap: u8 },
    /// The cap the real TX last fit at for this key, never above the configured maximum.
    Hit { start_cap: u8 },
}

impl Hint {
    pub fn start_cap(self) -> u8 {
        match self {
            Hint::Miss { start_cap } | Hint::Hit { start_cap } => start_cap,
        }
    }
}

/// Every `ROUTE_CAP_HINT_REPROBE_HITS`-th hit of a key searches from the configured maximum
/// again: a hinted search can only confirm or lower the fit cap, so without a re-probe a cap
/// lowered during a congested market would never rise again (a smaller cap can yield a worse
/// quote). The bound equals the attempt bound of one search (`ROUTE_FIT_MAX_ATTEMPTS`): a
/// re-probe costs at most that many requests, so the amortized cost of re-probing stays at most
/// one request per hinted evaluation.
pub const ROUTE_CAP_HINT_REPROBE_HITS: u8 = crate::execution::builder::p0::ROUTE_FIT_MAX_ATTEMPTS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    fit_cap: u8,
    rejected_above: Option<u8>,
    hits_since_probe: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// P6-MT.13 `route_cap_cache_hits`.
    pub hits: u64,
    /// P6-MT.13 `route_cap_cache_misses` (re-probes included).
    pub misses: u64,
    pub reprobes: u64,
    pub evictions: u64,
    pub invalidations: u64,
}

pub struct RouteCapCache {
    capacity: usize,
    /// The configured Jupiter `maxAccounts` the entries were proven under.
    max_accounts: u8,
    entries: HashMap<RouteCapKey, Entry>,
    /// Insertion order: the oldest key is evicted first (deterministic).
    order: VecDeque<RouteCapKey>,
    pub stats: Stats,
}

impl RouteCapCache {
    /// `capacity` keys at most; `max_accounts` is the configured cap the hints are bound to.
    pub fn new(capacity: usize, max_accounts: u8) -> Self {
        Self {
            capacity: capacity.max(1),
            max_accounts: max_accounts.clamp(1, 64),
            entries: HashMap::new(),
            order: VecDeque::new(),
            stats: Stats::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn max_accounts(&self) -> u8 {
        self.max_accounts
    }

    /// Drops every entry when the configured maximum differs from the one they were proven
    /// under (a cap proven under another bound says nothing about this one).
    fn bind(&mut self, max_accounts: u8) {
        let max_accounts = max_accounts.clamp(1, 64);
        if self.max_accounts != max_accounts {
            self.max_accounts = max_accounts;
            self.entries.clear();
            self.order.clear();
            self.stats.invalidations += 1;
        }
    }

    /// The start cap of a search under the configured `max_accounts`: the cap the real TX last
    /// fit at for `key` (a hit), else `max_accounts`. Only a number — the search still quotes
    /// and compiles every route itself.
    pub fn hint(&mut self, key: &RouteCapKey, max_accounts: u8) -> Hint {
        self.bind(max_accounts);
        let max = self.max_accounts;
        let Some(e) = self.entries.get_mut(key) else {
            self.stats.misses += 1;
            return Hint::Miss { start_cap: max };
        };
        if e.hits_since_probe >= ROUTE_CAP_HINT_REPROBE_HITS {
            e.hits_since_probe = 0;
            self.stats.reprobes += 1;
            self.stats.misses += 1;
            return Hint::Miss { start_cap: max };
        }
        e.hits_since_probe += 1;
        self.stats.hits += 1;
        Hint::Hit {
            start_cap: e.fit_cap.min(max),
        }
    }

    /// Records a search whose compiled TX fit at `trace.fit_cap` under `max_accounts`. A search
    /// without a fit (provider error, no route, budget exhausted) is never recorded.
    pub fn record_fit(&mut self, key: RouteCapKey, max_accounts: u8, trace: FitTrace) {
        self.bind(max_accounts);
        let fit_cap = trace.fit_cap.clamp(1, self.max_accounts);
        match self.entries.get_mut(&key) {
            Some(e) => {
                // A hinted search that fit at once proved no cap above; the bound it was
                // hinted below stays known.
                e.rejected_above = trace
                    .rejected_above
                    .or(e.rejected_above.filter(|r| *r > fit_cap));
                e.fit_cap = fit_cap;
            }
            None => {
                self.entries.insert(
                    key,
                    Entry {
                        fit_cap,
                        rejected_above: trace.rejected_above,
                        hits_since_probe: 0,
                    },
                );
                self.order.push_back(key);
                while self.entries.len() > self.capacity {
                    let Some(old) = self.order.pop_front() else {
                        break;
                    };
                    if self.entries.remove(&old).is_some() {
                        self.stats.evictions += 1;
                    }
                }
            }
        }
    }

    /// The remembered (fit cap, smallest rejected cap above) of `key`, for diagnostics.
    pub fn remembered(&self, key: &RouteCapKey) -> Option<(u8, Option<u8>)> {
        self.entries.get(key).map(|e| (e.fit_cap, e.rejected_above))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: u8) -> RouteCapKey {
        RouteCapKey {
            input_mint: Pubkey::new_from_array([n; 32]),
            output_mint: Pubkey::new_from_array([n + 100; 32]),
            class: RouteFootprintClass {
                locked_accounts: 30,
                serialized_len: 700,
            },
        }
    }
    fn trace(fit_cap: u8, rejected_above: Option<u8>) -> FitTrace {
        FitTrace {
            fit_cap,
            rejected_above,
            attempts: 1,
        }
    }

    /// (d) FIFO eviction at the bound and invalidation on a changed configured maximum; a miss
    /// starts at the configured maximum, a hit at the proven fit cap (never above the maximum).
    #[test]
    fn mt4_bound_eviction_and_invalidation() {
        let mut c = RouteCapCache::new(2, 64);
        assert_eq!(c.hint(&key(1), 64), Hint::Miss { start_cap: 64 });
        c.record_fit(key(1), 64, trace(30, Some(40)));
        c.record_fit(key(2), 64, trace(20, None));
        c.record_fit(key(3), 64, trace(10, Some(64)));
        assert_eq!((c.len(), c.stats.evictions), (2, 1));
        assert_eq!(c.remembered(&key(1)), None, "the oldest key is evicted");
        assert_eq!(c.hint(&key(2), 64), Hint::Hit { start_cap: 20 });
        assert_eq!(c.hint(&key(3), 64), Hint::Hit { start_cap: 10 });
        // A smaller configured maximum than the fit cap is never exceeded.
        c.record_fit(key(2), 64, trace(50, None));
        assert_eq!(c.max_accounts(), 64);
        // A changed configured maximum invalidates everything.
        assert_eq!(c.hint(&key(2), 40), Hint::Miss { start_cap: 40 });
        assert!(c.is_empty());
        assert_eq!(c.stats.invalidations, 1);
        assert_eq!(c.max_accounts(), 40);
        assert_eq!(c.hint(&key(3), 40), Hint::Miss { start_cap: 40 });
        assert_eq!(
            c.stats,
            Stats {
                hits: 2,
                misses: 3,
                reprobes: 0,
                evictions: 1,
                invalidations: 1,
            }
        );
    }

    /// A key is re-probed from the configured maximum every `ROUTE_CAP_HINT_REPROBE_HITS` hits,
    /// so a lowered fit cap can rise again; the fit recorded by the re-probe replaces it.
    #[test]
    fn mt4_periodic_reprobe_lets_the_fit_cap_rise() {
        let mut c = RouteCapCache::new(4, 64);
        c.record_fit(key(1), 64, trace(30, Some(40)));
        for _ in 0..ROUTE_CAP_HINT_REPROBE_HITS {
            assert_eq!(c.hint(&key(1), 64), Hint::Hit { start_cap: 30 });
        }
        assert_eq!(c.hint(&key(1), 64), Hint::Miss { start_cap: 64 });
        assert_eq!(c.stats.reprobes, 1);
        c.record_fit(key(1), 64, trace(64, None));
        assert_eq!(c.hint(&key(1), 64), Hint::Hit { start_cap: 64 });
        assert_eq!(c.remembered(&key(1)), Some((64, None)));
    }

    /// A hinted search that fit at once keeps the bound it was hinted below; a lower fit after
    /// a rejection records its own bound.
    #[test]
    fn mt4_rejected_bound_is_kept_across_hinted_fits() {
        let mut c = RouteCapCache::new(4, 64);
        c.record_fit(key(1), 64, trace(30, Some(40)));
        c.record_fit(key(1), 64, trace(30, None));
        assert_eq!(c.remembered(&key(1)), Some((30, Some(40))));
        c.record_fit(key(1), 64, trace(20, Some(30)));
        assert_eq!(c.remembered(&key(1)), Some((20, Some(30))));
    }
}

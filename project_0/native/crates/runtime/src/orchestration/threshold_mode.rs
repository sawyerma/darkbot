//! DEC-06 (ENTSCHEIDUNGEN §6, PHASENPLAN DEC-06): the threshold mode of the state owner. The
//! operator requests OFF, OBSERVE or ACTIVE through the one controller (`p0ctl capability set
//! THRESHOLD_TRIGGER …`, CAS on the capability configuration version); the owner alone decides
//! the effective mode:
//! - OFF: the R127 sweep is authoritative, no threshold index is kept;
//! - SWEEP: a threshold mode is requested, the sweep stays authoritative — the index is still
//!   bootstrapping (also after every recovery and restart) or the state is not tradable;
//! - OBSERVE: the sweep stays authoritative; the threshold trigger runs beside it on the same
//!   state and input revisions, admits nothing and is compared with the sweep;
//! - ACTIVE: the threshold trigger admits; a sampled sweep cross-check runs off the owner.
//!
//! ACTIVE is effective only from OBSERVE: with the user's N (B-8) comparable oracle ticks without
//! a miss, no latched miss fallback and an empty sweep batch (the barrier). ACTIVE is no LIVE.

/// The capability of the threshold mode.
pub const THRESHOLD_TRIGGER: &str = "THRESHOLD_TRIGGER";
/// The values the threshold mode may be requested with.
pub const VALUES: [&str; 3] = ["OFF", "OBSERVE", "ACTIVE"];

/// The effective mode follows the index: the index is (re)built after the start, a recovery or a
/// restart, the sweep stays authoritative until it is.
pub const INDEX_BOOTSTRAP: &str = "INDEX_BOOTSTRAP";
/// Gap or unknown state: no new admission, the sweep is the fallback.
pub const STATE_NOT_TRADABLE: &str = "STATE_NOT_TRADABLE";
/// ACTIVE is not effective: the user has not decided N (B-8, `threshold_min_ticks` unset).
pub const B8_UNDECIDED: &str = "B8_UNDECIDED";
/// ACTIVE is not effective: fewer than N comparable oracle ticks since the last miss.
pub const EVIDENCE_SHORT: &str = "THRESHOLD_EVIDENCE_SHORT";
/// ACTIVE is not effective: a sweep cross-check found a miss in ACTIVE; only a new ACTIVE request
/// lifts it.
pub const MISS_FALLBACK: &str = "THRESHOLD_MISS_FALLBACK";
/// ACTIVE is not effective yet: the open sweep batch is evaluated first.
pub const BARRIER: &str = "THRESHOLD_BARRIER";
/// A refused request: ACTIVE is reachable from OBSERVE only.
pub const OFF_TO_ACTIVE: &str =
    "THRESHOLD_TRANSITION_INVALID: OFF -> ACTIVE, request OBSERVE first";

/// The operator's threshold mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Requested {
    #[default]
    Off,
    Observe,
    Active,
}

impl Requested {
    pub fn code(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Observe => "OBSERVE",
            Self::Active => "ACTIVE",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "OFF" => Some(Self::Off),
            "OBSERVE" => Some(Self::Observe),
            "ACTIVE" => Some(Self::Active),
            _ => None,
        }
    }
    /// The start value of the configuration (`threshold_trigger`, `P0_USE_THRESHOLD_TRIGGER`):
    /// `on` requests the threshold trigger, which becomes effective only through OBSERVE.
    pub fn from_config(threshold_trigger: bool) -> Self {
        if threshold_trigger {
            Self::Active
        } else {
            Self::Off
        }
    }
}

/// The owner's effective threshold mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effective {
    Off,
    Sweep,
    Observe,
    Active,
}

impl Effective {
    pub fn code(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Sweep => "SWEEP",
            Self::Observe => "OBSERVE",
            Self::Active => "ACTIVE",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "OFF" => Some(Self::Off),
            "SWEEP" => Some(Self::Sweep),
            "OBSERVE" => Some(Self::Observe),
            "ACTIVE" => Some(Self::Active),
            _ => None,
        }
    }
}

/// What the effective mode is decided from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inputs {
    pub requested: Requested,
    /// The state is tradable (no gap, no recovery).
    pub tradable: bool,
    /// The index was bootstrapped after its last (re)build.
    pub bootstrapped: bool,
    /// N (B-8); `None`: undecided.
    pub min_ticks: Option<u64>,
    /// Comparable oracle ticks since the last miss.
    pub streak: u64,
    /// A miss in ACTIVE took ACTIVE back.
    pub latched: bool,
    /// A sweep batch is open (dirty banks or a pending flush).
    pub batch_open: bool,
}

/// The effective mode and why it is not the requested one ("" when it is).
pub fn decide(i: &Inputs) -> (Effective, &'static str) {
    if i.requested == Requested::Off {
        return (Effective::Off, "");
    }
    if !i.tradable {
        return (Effective::Sweep, STATE_NOT_TRADABLE);
    }
    if !i.bootstrapped {
        return (Effective::Sweep, INDEX_BOOTSTRAP);
    }
    if i.requested == Requested::Observe {
        return (Effective::Observe, "");
    }
    match i.min_ticks {
        _ if i.latched => (Effective::Observe, MISS_FALLBACK),
        None => (Effective::Observe, B8_UNDECIDED),
        Some(n) if i.streak < n => (Effective::Observe, EVIDENCE_SHORT),
        Some(_) if i.batch_open => (Effective::Observe, BARRIER),
        Some(_) => (Effective::Active, ""),
    }
}

/// A request of the operator from the current one: ACTIVE only from OBSERVE (or ACTIVE).
pub fn request(current: Requested, next: Requested) -> Result<Requested, &'static str> {
    if current == Requested::Off && next == Requested::Active {
        return Err(OFF_TO_ACTIVE);
    }
    Ok(next)
}

/// B-9: the owner time OBSERVE may use — `ppm` of the owner's wall time, at most one second's
/// share banked. A unit of OBSERVE work runs only while the bank is positive and is charged its
/// measured time afterwards (a unit is never cut short), so over any interval OBSERVE uses at most
/// its share plus one second's share plus one unit; without budget OBSERVE is sampled, the sweep
/// is never slowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObserveBudget {
    ppm: u64,
    bank_ns: i128,
    last_ns: u64,
}

impl ObserveBudget {
    const PPM: i128 = 1_000_000;
    const SECOND_NS: i128 = 1_000_000_000;

    /// A full bank at `now_ns`.
    pub fn new(ppm: u64, now_ns: u64) -> Self {
        let mut b = Self {
            ppm,
            bank_ns: 0,
            last_ns: now_ns,
        };
        b.bank_ns = b.cap();
        b
    }
    fn cap(&self) -> i128 {
        Self::SECOND_NS * i128::from(self.ppm) / Self::PPM
    }
    /// Whether a unit may run now (the bank refilled to `now_ns`).
    pub fn allows(&mut self, now_ns: u64) -> bool {
        let elapsed = i128::from(now_ns.saturating_sub(self.last_ns));
        self.last_ns = self.last_ns.max(now_ns);
        self.bank_ns = (self.bank_ns + elapsed * i128::from(self.ppm) / Self::PPM).min(self.cap());
        self.bank_ns > 0
    }
    /// Charges a unit's measured owner time.
    pub fn charge(&mut self, ns: u64) {
        self.bank_ns -= i128::from(ns);
    }
    pub fn ppm(&self) -> u64 {
        self.ppm
    }
}

/// The comparison and cost counters of the threshold mode, cumulative over the owner's run.
#[derive(Clone, Debug, Default)]
pub struct ThresholdStats {
    /// Comparable oracle ticks (OBSERVE, same state and input revision as the sweep).
    pub ticks: u64,
    /// Comparable oracle ticks since the last miss (B-8's N is reached by this).
    pub streak: u64,
    /// Oracle ticks of OBSERVE batches that were not comparable (sampled, price error, index
    /// incomplete, generation change).
    pub uncomparable_ticks: u64,
    /// Liquidatable accounts found by the sweep and woken by the threshold trigger.
    pub hits: u64,
    /// Liquidatable accounts found by the sweep (or its cross-check) but not woken.
    pub misses: u64,
    /// Accounts woken by the threshold trigger that the sweep found healthy.
    pub extra: u64,
    /// OBSERVE work skipped for want of budget (B-9).
    pub sampled_out: u64,
    /// Times the effective mode left ACTIVE.
    pub fallbacks: u64,
    /// Exact threshold evaluations of the index (`thresholds()`).
    pub index_evaluations: u64,
    /// ACTIVE cross-check samples sent, skipped (queue full), stale (state changed) and missed.
    pub crosscheck_samples: u64,
    pub crosscheck_skipped: u64,
    pub crosscheck_stale: u64,
    pub crosscheck_misses: u64,
    /// t_rx → exact sweep evaluation of a liquidatable account.
    pub sweep_detect_ns: darkbot_common::histogram::Histogram,
    /// t_rx → threshold wake-up of a liquidatable account.
    pub threshold_detect_ns: darkbot_common::histogram::Histogram,
    /// Owner time of one unit of OBSERVE work (lookups, comparison, index upkeep).
    pub observe_owner_ns: darkbot_common::histogram::Histogram,
}

impl ThresholdStats {
    /// One comparable or uncomparable batch of `ticks` oracle ticks with `misses` misses.
    pub fn batch(&mut self, ticks: u64, comparable: bool, misses: u64) {
        if comparable {
            self.ticks = self.ticks.saturating_add(ticks);
        } else {
            self.uncomparable_ticks = self.uncomparable_ticks.saturating_add(ticks);
        }
        if misses > 0 {
            self.streak = 0;
        } else if comparable {
            self.streak = self.streak.saturating_add(ticks);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(requested: Requested) -> Inputs {
        Inputs {
            requested,
            tradable: true,
            bootstrapped: true,
            min_ticks: Some(10),
            streak: 10,
            latched: false,
            batch_open: false,
        }
    }

    /// DEC-06: every effective mode and its block reason, in precedence order: OFF, the state,
    /// the index bootstrap, OBSERVE, then ACTIVE's gates (latch, N, streak, barrier).
    #[test]
    fn decide_follows_state_index_evidence_and_barrier() {
        use Effective as E;
        let active = inputs(Requested::Active);
        assert_eq!(decide(&active), (E::Active, ""));
        assert_eq!(decide(&inputs(Requested::Off)), (E::Off, ""));
        assert_eq!(decide(&inputs(Requested::Observe)), (E::Observe, ""));
        let cases: [(Inputs, (Effective, &str)); 7] = [
            (
                Inputs {
                    tradable: false,
                    bootstrapped: false,
                    ..active
                },
                (E::Sweep, STATE_NOT_TRADABLE),
            ),
            (
                Inputs {
                    bootstrapped: false,
                    latched: true,
                    ..active
                },
                (E::Sweep, INDEX_BOOTSTRAP),
            ),
            (
                Inputs {
                    requested: Requested::Observe,
                    bootstrapped: false,
                    ..active
                },
                (E::Sweep, INDEX_BOOTSTRAP),
            ),
            (
                Inputs {
                    latched: true,
                    min_ticks: None,
                    ..active
                },
                (E::Observe, MISS_FALLBACK),
            ),
            (
                Inputs {
                    min_ticks: None,
                    ..active
                },
                (E::Observe, B8_UNDECIDED),
            ),
            (
                Inputs {
                    streak: 9,
                    batch_open: true,
                    ..active
                },
                (E::Observe, EVIDENCE_SHORT),
            ),
            (
                Inputs {
                    batch_open: true,
                    ..active
                },
                (E::Observe, BARRIER),
            ),
        ];
        for (i, expected) in cases {
            assert_eq!(decide(&i), expected, "{i:?}");
        }
        assert_eq!(
            decide(&Inputs {
                requested: Requested::Off,
                tradable: false,
                ..active
            }),
            (E::Off, "")
        );
    }

    /// DEC-06: OFF → ACTIVE is refused; every other request is accepted as it is.
    #[test]
    fn active_is_requested_from_observe_only() {
        use Requested::*;
        assert_eq!(request(Off, Active), Err(OFF_TO_ACTIVE));
        for (from, to) in [
            (Off, Off),
            (Off, Observe),
            (Observe, Active),
            (Observe, Off),
            (Active, Active),
            (Active, Observe),
            (Active, Off),
        ] {
            assert_eq!(request(from, to), Ok(to), "{from:?} -> {to:?}");
        }
        for v in VALUES {
            assert_eq!(Requested::parse(v).map(Requested::code), Some(v));
        }
        assert_eq!(Requested::parse("SWEEP"), None);
        assert_eq!(
            (Requested::from_config(false), Requested::from_config(true)),
            (Off, Active)
        );
        for e in [
            Effective::Off,
            Effective::Sweep,
            Effective::Observe,
            Effective::Active,
        ] {
            assert_eq!(Effective::parse(e.code()), Some(e));
        }
    }

    /// B-9: a full bank holds one second's share; an exhausted bank refuses until the owner's
    /// wall time refilled it, and a unit is charged after it ran (the share bounds the use).
    #[test]
    fn observe_budget_samples_once_its_share_is_used() {
        // 2 % of the owner: 20 ms per second.
        let mut b = ObserveBudget::new(20_000, 0);
        assert!(b.allows(0));
        b.charge(15_000_000);
        assert!(b.allows(0), "5 ms left");
        b.charge(15_000_000);
        assert!(!b.allows(0), "10 ms over: sampled");
        assert!(
            !b.allows(400_000_000),
            "0.4 s refill 8 ms: still 2 ms short"
        );
        assert!(b.allows(600_000_000), "0.6 s refill 12 ms: 2 ms left");
        // The bank never holds more than one second's share.
        let mut idle = ObserveBudget::new(20_000, 0);
        assert!(idle.allows(3_600_000_000_000));
        idle.charge(20_000_000);
        assert!(
            !idle.allows(3_600_000_000_000),
            "an idle hour banks no more than 20 ms"
        );
        // Time never runs backwards in the bank.
        let mut back = ObserveBudget::new(20_000, 1_000);
        back.charge(25_000_000);
        assert!(!back.allows(0));
        assert_eq!(back.ppm(), 20_000);
    }

    /// B-8: only comparable ticks count, a miss resets the streak (also in an uncomparable
    /// batch), an uncomparable batch without a miss leaves it.
    #[test]
    fn the_streak_counts_comparable_ticks_since_the_last_miss() {
        let mut s = ThresholdStats::default();
        s.batch(3, true, 0);
        s.batch(2, false, 0);
        assert_eq!((s.ticks, s.streak, s.uncomparable_ticks), (3, 3, 2));
        s.batch(4, true, 1);
        assert_eq!((s.ticks, s.streak), (7, 0));
        s.batch(5, true, 0);
        s.batch(1, false, 1);
        assert_eq!((s.ticks, s.streak, s.uncomparable_ticks), (12, 0, 3));
    }
}

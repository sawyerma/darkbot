//! Timing Contract (PHASENPLAN §6).
//!
//! Alle Stufen messen auf derselben monotonen Uhr in Nanosekunden.
//! `T_observed_landed` ist ein von uns beobachteter Zeitpunkt, kein exakter
//! Chain-Ausführungszeitpunkt (Landing-Evidence liegt separat in Slot/Index).

use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// D08 (P0-PROD-CTRL-08): the code of a host wall clock before the Unix epoch. A wall time is
/// never folded into 0: callers refuse the start or end with this code.
pub const CLOCK_UNAVAILABLE: &str = "CLOCK_UNAVAILABLE";

/// Time since the Unix epoch of `t`; `CLOCK_UNAVAILABLE` before the epoch.
pub fn since_epoch(t: SystemTime) -> Result<std::time::Duration, &'static str> {
    t.duration_since(UNIX_EPOCH).map_err(|_| CLOCK_UNAVAILABLE)
}

/// The host wall clock since the Unix epoch (`CLOCK_UNAVAILABLE`, never 0).
pub fn wall_now() -> Result<std::time::Duration, &'static str> {
    since_epoch(SystemTime::now())
}

/// Messpunkte in der verbindlichen Reihenfolge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stage {
    /// Rohupdate im Ingress empfangen (vor Business-Decode).
    Rx = 0,
    /// State-Update angewendet.
    State = 1,
    /// Kandidat als liquidierbar + profitabel erkannt.
    Detect = 2,
    /// Vollständiger Execution-Plan / TX gebaut.
    Ready = 3,
    /// Signatur abgeschlossen.
    Sign = 4,
    /// An Submission-Adapter übergeben (ACK ≠ landed).
    Handoff = 5,
    /// Landing von uns beobachtet/reconciled.
    ObservedLanded = 6,
}

pub const STAGES: [Stage; 7] = [
    Stage::Rx,
    Stage::State,
    Stage::Detect,
    Stage::Ready,
    Stage::Sign,
    Stage::Handoff,
    Stage::ObservedLanded,
];

impl Stage {
    pub const fn name(self) -> &'static str {
        match self {
            Stage::Rx => "T_rx",
            Stage::State => "T_state",
            Stage::Detect => "T_detect",
            Stage::Ready => "T_ready",
            Stage::Sign => "T_sign",
            Stage::Handoff => "T_handoff",
            Stage::ObservedLanded => "T_observed_landed",
        }
    }
}

/// Monotone Prozessuhr mit einmalig festgehaltenem Unix-Bezug.
///
/// `now_ns()` ist monoton und wird für alle Latenzen verwendet.
/// `unix_ns()` dient nur der Provenienz in Journal/Ledger.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    origin: Instant,
    origin_unix_ns: u128,
}

impl Clock {
    /// A clock whose wall origin is the host clock now; production entries use it and refuse
    /// to start with `CLOCK_UNAVAILABLE` (D08).
    pub fn try_new() -> Result<Self, &'static str> {
        Self::at(Instant::now(), SystemTime::now())
    }

    /// Test and fixture clock: `try_new` that never yields an origin of 0 (it panics on a host
    /// clock before the Unix epoch instead). Production entries call `try_new`.
    pub fn new() -> Self {
        Self::try_new().expect(CLOCK_UNAVAILABLE)
    }

    /// The clock of a monotonic `origin` whose wall time is `wall`; `CLOCK_UNAVAILABLE` before
    /// the Unix epoch (never an origin of 0).
    pub fn at(origin: Instant, wall: SystemTime) -> Result<Self, &'static str> {
        Ok(Clock {
            origin,
            origin_unix_ns: since_epoch(wall)?.as_nanos(),
        })
    }

    /// Nanosekunden seit Clock-Erzeugung, monoton.
    #[inline]
    pub fn now_ns(&self) -> u64 {
        // as_nanos passt für > 500 Jahre in u64.
        self.origin.elapsed().as_nanos() as u64
    }

    /// Unix-Zeit (ns) für einen monotonen Zeitstempel dieser Uhr.
    pub fn unix_ns(&self, mono_ns: u64) -> u128 {
        self.origin_unix_ns + mono_ns as u128
    }
}

impl Default for Clock {
    fn default() -> Self {
        Clock::new()
    }
}

/// PHASENPLAN §6 Ableitungen: the core stages between consecutive normative points (S4-04
/// baseline: per stage count, p50/p95/p99/max, allocations, queue wait).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CoreStage {
    IngressToState = 0,
    StateToDetect = 1,
    DetectToReady = 2,
    ReadyToSign = 3,
    SignToHandoff = 4,
}

pub const CORE_STAGES: [CoreStage; 5] = [
    CoreStage::IngressToState,
    CoreStage::StateToDetect,
    CoreStage::DetectToReady,
    CoreStage::ReadyToSign,
    CoreStage::SignToHandoff,
];

impl CoreStage {
    pub const fn name(self) -> &'static str {
        match self {
            CoreStage::IngressToState => "ingress_to_state",
            CoreStage::StateToDetect => "state_to_detect",
            CoreStage::DetectToReady => "detect_to_ready",
            CoreStage::ReadyToSign => "ready_to_sign",
            CoreStage::SignToHandoff => "sign_to_handoff",
        }
    }

    /// The normative points the stage lies between.
    pub const fn bounds(self) -> (Stage, Stage) {
        match self {
            CoreStage::IngressToState => (Stage::Rx, Stage::State),
            CoreStage::StateToDetect => (Stage::State, Stage::Detect),
            CoreStage::DetectToReady => (Stage::Detect, Stage::Ready),
            CoreStage::ReadyToSign => (Stage::Ready, Stage::Sign),
            CoreStage::SignToHandoff => (Stage::Sign, Stage::Handoff),
        }
    }
}

/// S4-04: a thread's allocation count at one instant (`crate::alloc`), with the thread that read
/// it — counts of two threads are never subtracted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationSample {
    pub allocations: u64,
    thread: std::thread::ThreadId,
}

impl AllocationSample {
    /// The calling thread's count now; `None` in a build that counts none.
    pub fn now() -> Option<Self> {
        crate::alloc::thread_allocations().map(|allocations| AllocationSample {
            allocations,
            thread: std::thread::current().id(),
        })
    }
}

/// S4-04: a point read at once where it happens — its time on a `Clock` and the reading thread's
/// allocation sample (the handoff, read in the FE-04 journal immediately before the adapter
/// call, after its durable writes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub ns: u64,
    pub allocations: Option<AllocationSample>,
}

impl Point {
    pub fn now(clock: &Clock) -> Self {
        let allocations = AllocationSample::now();
        Point {
            ns: clock.now_ns(),
            allocations,
        }
    }
}

/// Zeitleiste einer Opportunity/Transaktion. Allokationsfrei.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timeline {
    marks: [Option<u64>; 7],
    /// S4-04: the marking thread's allocation sample at each point; `None` for a point measured
    /// elsewhere (`mark_elsewhere`) or in a build that counts none.
    allocations: [Option<AllocationSample>; 7],
}

impl Timeline {
    pub const fn new() -> Self {
        Timeline {
            marks: [None; 7],
            allocations: [None; 7],
        }
    }

    /// Setzt einen Messpunkt. Ein bereits gesetzter Punkt wird nicht überschrieben
    /// (Rückgabe `false`), damit Provenienz nicht stillschweigend verloren geht.
    pub fn mark(&mut self, stage: Stage, mono_ns: u64) -> bool {
        self.mark_with(stage, mono_ns, AllocationSample::now())
    }

    /// `mark` of a point read where it happened (`Point::now`).
    pub fn mark_point(&mut self, stage: Stage, point: Point) -> bool {
        self.mark_with(stage, point.ns, point.allocations)
    }

    /// `mark` of a point measured on another thread (its time, e.g. the ingress `T_rx`): no
    /// allocation count of this thread belongs to it.
    pub fn mark_elsewhere(&mut self, stage: Stage, mono_ns: u64) -> bool {
        self.mark_with(stage, mono_ns, None)
    }

    fn mark_with(
        &mut self,
        stage: Stage,
        mono_ns: u64,
        allocations: Option<AllocationSample>,
    ) -> bool {
        let slot = &mut self.marks[stage as usize];
        if slot.is_some() {
            return false;
        }
        *slot = Some(mono_ns);
        self.allocations[stage as usize] = allocations;
        true
    }

    pub fn get(&self, stage: Stage) -> Option<u64> {
        self.marks[stage as usize]
    }

    /// Intervall `to - from` in ns. `None`, wenn ein Punkt fehlt oder die
    /// Reihenfolge verletzt ist (nie negative Latenzen als 0 ausgeben).
    pub fn interval(&self, from: Stage, to: Stage) -> Option<u64> {
        let a = self.get(from)?;
        let b = self.get(to)?;
        if b < a {
            return None;
        }
        Some(b - a)
    }

    /// S4-04: the core stage's duration (`interval` of its bounds) and the allocations made
    /// within it — `None` unless both points carry a sample of the same thread (a counting build;
    /// neither point measured elsewhere).
    pub fn core_stage(&self, stage: CoreStage) -> (Option<u64>, Option<u64>) {
        let (from, to) = stage.bounds();
        let allocations = match (
            self.allocations[from as usize],
            self.allocations[to as usize],
        ) {
            (Some(a), Some(b)) if a.thread == b.thread && b.allocations >= a.allocations => {
                Some(b.allocations - a.allocations)
            }
            _ => None,
        };
        (self.interval(from, to), allocations)
    }

    /// Alle Standardintervalle des Ledgers (§1) in fester Reihenfolge.
    pub fn standard_intervals(&self) -> [(&'static str, Option<u64>); 5] {
        [
            ("T_rx→T_detect", self.interval(Stage::Rx, Stage::Detect)),
            (
                "T_detect→T_ready",
                self.interval(Stage::Detect, Stage::Ready),
            ),
            ("T_ready→T_sign", self.interval(Stage::Ready, Stage::Sign)),
            (
                "T_sign→T_handoff",
                self.interval(Stage::Sign, Stage::Handoff),
            ),
            ("T_rx→T_handoff", self.interval(Stage::Rx, Stage::Handoff)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D08 (P0-PROD-CTRL-08): a host clock before the Unix epoch is CLOCK_UNAVAILABLE — never a
    /// clock whose wall origin is 0 (every timestamp derived from it would be invented).
    #[test]
    fn d08_a_clock_before_the_epoch_is_refused_never_zero() {
        let before = UNIX_EPOCH - std::time::Duration::from_secs(1);
        assert_eq!(
            Clock::at(Instant::now(), before).err(),
            Some(CLOCK_UNAVAILABLE)
        );
        assert_eq!(since_epoch(before), Err(CLOCK_UNAVAILABLE));
        let later = UNIX_EPOCH + std::time::Duration::from_secs(5);
        assert_eq!(since_epoch(later), Ok(std::time::Duration::from_secs(5)));
        let c = Clock::at(Instant::now(), later).unwrap();
        assert!(c.unix_ns(c.now_ns()) >= 5_000_000_000);
    }

    #[test]
    fn clock_is_monotonic() {
        let c = Clock::new();
        let a = c.now_ns();
        let b = c.now_ns();
        assert!(b >= a);
        assert!(c.unix_ns(b) >= c.unix_ns(a));
    }

    #[test]
    fn timeline_intervals_and_no_overwrite() {
        let mut t = Timeline::new();
        assert!(t.mark(Stage::Rx, 100));
        assert!(t.mark(Stage::Detect, 250));
        assert!(
            !t.mark(Stage::Rx, 999),
            "erster Mark darf nicht überschrieben werden"
        );
        assert_eq!(t.get(Stage::Rx), Some(100));
        assert_eq!(t.interval(Stage::Rx, Stage::Detect), Some(150));
        assert_eq!(
            t.interval(Stage::Rx, Stage::Ready),
            None,
            "fehlender Punkt → None, nie 0"
        );
    }

    #[test]
    fn timeline_rejects_negative_interval() {
        let mut t = Timeline::new();
        t.mark(Stage::Ready, 500);
        t.mark(Stage::Sign, 400);
        assert_eq!(t.interval(Stage::Ready, Stage::Sign), None);
    }

    /// S4-04: the core stages are the §6 derivations between consecutive points; a build without
    /// the counting allocator counts no allocations (unknown, never 0), a point measured
    /// elsewhere never carries this thread's count.
    #[test]
    fn s4_04_core_stages_are_the_derivations_and_count_only_what_is_counted() {
        let names: Vec<_> = CORE_STAGES.iter().map(|s| s.name()).collect();
        assert_eq!(
            names,
            [
                "ingress_to_state",
                "state_to_detect",
                "detect_to_ready",
                "ready_to_sign",
                "sign_to_handoff"
            ]
        );
        for pair in CORE_STAGES.windows(2) {
            assert_eq!(pair[0].bounds().1, pair[1].bounds().0, "consecutive");
        }
        // A build that counts none (`thread_allocations()` is `None`); a point measured elsewhere.
        let mut t = Timeline::new();
        t.mark_elsewhere(Stage::Rx, 100);
        t.mark_with(Stage::State, 400, None);
        t.mark_with(Stage::Detect, 450, None);
        assert_eq!(t.core_stage(CoreStage::IngressToState), (Some(300), None));
        assert_eq!(t.core_stage(CoreStage::StateToDetect), (Some(50), None));
        assert_eq!(t.core_stage(CoreStage::DetectToReady), (None, None));
        let mut e = Timeline::new();
        e.mark_with(
            Stage::Rx,
            1,
            Some(AllocationSample {
                allocations: 4,
                thread: std::thread::current().id(),
            }),
        );
        e.mark_elsewhere(Stage::State, 2);
        assert_eq!(e.core_stage(CoreStage::IngressToState), (Some(1), None));
        // Samples as a counting build takes them.
        let here = std::thread::current().id();
        let sample = |allocations| {
            Some(AllocationSample {
                allocations,
                thread: here,
            })
        };
        let mut c = Timeline::new();
        c.mark_with(Stage::Ready, 10, sample(7));
        c.mark_with(Stage::Sign, 30, sample(19));
        c.mark_with(Stage::Handoff, 35, sample(3));
        assert_eq!(c.core_stage(CoreStage::ReadyToSign), (Some(20), Some(12)));
        assert_eq!(
            c.core_stage(CoreStage::SignToHandoff),
            (Some(5), None),
            "a count that went back is no count"
        );
        assert!(
            !c.mark_with(Stage::Ready, 11, sample(8)),
            "never overwritten"
        );
        assert_eq!(c.core_stage(CoreStage::ReadyToSign), (Some(20), Some(12)));
        // Another thread's count is never subtracted (review L3).
        let elsewhere = std::thread::spawn(|| std::thread::current().id())
            .join()
            .unwrap();
        let mut t2 = Timeline::new();
        t2.mark_with(Stage::Ready, 1, sample(5));
        t2.mark_point(
            Stage::Sign,
            Point {
                ns: 4,
                allocations: Some(AllocationSample {
                    allocations: 9,
                    thread: elsewhere,
                }),
            },
        );
        assert_eq!(t2.core_stage(CoreStage::ReadyToSign), (Some(3), None));
    }

    #[test]
    fn stage_names_are_ledger_vocabulary() {
        let names: Vec<&str> = STAGES.iter().map(|s| s.name()).collect();
        assert_eq!(
            names,
            vec![
                "T_rx",
                "T_state",
                "T_detect",
                "T_ready",
                "T_sign",
                "T_handoff",
                "T_observed_landed"
            ]
        );
    }
}

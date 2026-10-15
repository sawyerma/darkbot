//! Timing Contract (PHASENPLAN §6).
//!
//! Alle Stufen messen auf derselben monotonen Uhr in Nanosekunden.
//! `T_observed_landed` ist ein von uns beobachteter Zeitpunkt, kein exakter
//! Chain-Ausführungszeitpunkt (Landing-Evidence liegt separat in Slot/Index).

use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
    pub fn new() -> Self {
        let origin_unix_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Clock {
            origin: Instant::now(),
            origin_unix_ns,
        }
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

/// Zeitleiste einer Opportunity/Transaktion. Allokationsfrei.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timeline {
    marks: [Option<u64>; 7],
}

impl Timeline {
    pub const fn new() -> Self {
        Timeline { marks: [None; 7] }
    }

    /// Setzt einen Messpunkt. Ein bereits gesetzter Punkt wird nicht überschrieben
    /// (Rückgabe `false`), damit Provenienz nicht stillschweigend verloren geht.
    pub fn mark(&mut self, stage: Stage, mono_ns: u64) -> bool {
        let slot = &mut self.marks[stage as usize];
        if slot.is_some() {
            return false;
        }
        *slot = Some(mono_ns);
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

    /// Alle Standardintervalle des Ledgers (§1) in fester Reihenfolge.
    pub fn standard_intervals(&self) -> [(&'static str, Option<u64>); 5] {
        [
            ("T_rx→T_detect", self.interval(Stage::Rx, Stage::Detect)),
            ("T_detect→T_ready", self.interval(Stage::Detect, Stage::Ready)),
            ("T_ready→T_sign", self.interval(Stage::Ready, Stage::Sign)),
            ("T_sign→T_handoff", self.interval(Stage::Sign, Stage::Handoff)),
            ("T_rx→T_handoff", self.interval(Stage::Rx, Stage::Handoff)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(!t.mark(Stage::Rx, 999), "erster Mark darf nicht überschrieben werden");
        assert_eq!(t.get(Stage::Rx), Some(100));
        assert_eq!(t.interval(Stage::Rx, Stage::Detect), Some(150));
        assert_eq!(t.interval(Stage::Rx, Stage::Ready), None, "fehlender Punkt → None, nie 0");
    }

    #[test]
    fn timeline_rejects_negative_interval() {
        let mut t = Timeline::new();
        t.mark(Stage::Ready, 500);
        t.mark(Stage::Sign, 400);
        assert_eq!(t.interval(Stage::Ready, Stage::Sign), None);
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

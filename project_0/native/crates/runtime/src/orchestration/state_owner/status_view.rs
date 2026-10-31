//! The FE-04 accounting view the owner status projection carries (read at the journal refresh
//! cadence, never on the hot path).
use crate::execution::accounting::{Ledger, Totals, Verdict};

/// Totals and verdict of the liquidator's FE-04 journals (`execution/accounting.rs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LedgerView {
    pub(crate) totals: Totals,
    pub(crate) verdict: Verdict,
    /// P8-CTRL.5: every attempt's FE-06 grant booking.
    pub(crate) bookings: Vec<crate::execution::one_shot::Fe06Attempt>,
    /// P8-CTRL.5: every exit's FE-06 grant booking.
    pub(crate) exit_bookings: Vec<crate::execution::one_shot::Fe06Attempt>,
}

impl LedgerView {
    pub(crate) fn of(ledger: &Ledger) -> Self {
        Self {
            totals: ledger.totals(),
            verdict: ledger.verdict(),
            bookings: ledger
                .attempts
                .iter()
                .filter_map(|a| a.fe06.clone())
                .collect(),
            exit_bookings: ledger
                .attempts
                .iter()
                .flat_map(|a| a.exits.iter())
                .filter_map(|e| e.fe06.clone())
                .collect(),
        }
    }

    /// Reads the wallet's journals; an unreadable journal is an error, never an empty ledger.
    pub(crate) fn load(dir: &std::path::Path, wallet: &str) -> Result<Self, String> {
        Ledger::load(dir, wallet).map(|l| Self::of(&l))
    }
}

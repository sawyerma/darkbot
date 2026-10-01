//! Fail-closed Fehlertaxonomie (S0). Jeder Grund führt zu HALT der betroffenen
//! Execution-Klasse; keiner wird stillschweigend ignoriert oder als 0 gewertet.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HaltReason {
    FeedGap,
    StateGap,
    ReconcileFailure,
    ProgramUpgrade,
    UnknownLayout,
    UnknownOracle,
    UnknownIntegration,
    UnknownEconomics,
    UnknownVersion,
    /// [FIX-12] Safety-Limit verletzt (Daily Loss / Failed Attempts) — kein Protokollfehler.
    LimitBreached,
    Shutdown,
}

impl HaltReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            HaltReason::FeedGap => "FEED_GAP",
            HaltReason::StateGap => "STATE_GAP",
            HaltReason::ReconcileFailure => "RECONCILE_FAILURE",
            HaltReason::ProgramUpgrade => "PROGRAM_UPGRADE",
            HaltReason::UnknownLayout => "UNKNOWN_LAYOUT",
            HaltReason::UnknownOracle => "UNKNOWN_ORACLE",
            HaltReason::UnknownIntegration => "UNKNOWN_INTEGRATION",
            HaltReason::UnknownEconomics => "UNKNOWN_ECONOMICS",
            HaltReason::UnknownVersion => "UNKNOWN_VERSION",
            HaltReason::LimitBreached => "LIMIT_BREACHED",
            HaltReason::Shutdown => "SHUTDOWN",
        }
    }
}

impl fmt::Display for HaltReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Fehler, der eine Execution-Klasse fail-closed anhält.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailClosed {
    pub reason: HaltReason,
    pub detail: String,
}

impl FailClosed {
    pub fn new(reason: HaltReason, detail: impl Into<String>) -> Self {
        FailClosed { reason, detail: detail.into() }
    }
}

impl fmt::Display for FailClosed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HALT[{}]: {}", self.reason, self.detail)
    }
}

impl std::error::Error for FailClosed {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_carries_reason_and_detail() {
        let e = FailClosed::new(HaltReason::UnknownLayout, "bank size 3904 not in manifest");
        assert_eq!(e.to_string(), "HALT[UNKNOWN_LAYOUT]: bank size 3904 not in manifest");
    }
}

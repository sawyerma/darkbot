//! Freshness of an opportunity (Drift keep-rs pattern, adapted; no code copied): a stale
//! opportunity is not executed, and build, sign and submit stay within a hard deadline per
//! attempt. Times in nanoseconds from `T_rx`; both windows are configuration.
//!
//! F-14 (CTRL-09): the former eva01 execution helpers of this module (route choice, priority-fee
//! percentile, no-route quarantine, target backoff, simulate gate, Jito chain confirmation,
//! sequential fallback) had no productive caller; the pipeline, the submission adapters and the
//! FE-04 owner hold those decisions, so they were removed instead of kept as a second policy.

/// Drift-keep-rs-Muster (adaptiert): stale Opportunities verwerfen, harte Deadline pro Versuch.
#[derive(Clone, Copy, Debug)]
pub struct Freshness {
    /// Opportunity älter als das (ab T_rx, ns) → nicht ausführen.
    pub max_opportunity_age_ns: u64,
    /// V2-04: Build+Sign+Submit müssen innerhalb dieser Frist ab T_rx liegen (nicht ab T_detect,
    /// das erst nach der Ökonomie markiert wird).
    pub execution_deadline_ns: u64,
}

impl Freshness {
    pub fn is_stale(&self, t_rx_ns: u64, now_ns: u64) -> bool {
        now_ns.saturating_sub(t_rx_ns) > self.max_opportunity_age_ns
    }
    pub fn deadline_passed(&self, t_rx_ns: u64, now_ns: u64) -> bool {
        now_ns.saturating_sub(t_rx_ns) > self.execution_deadline_ns
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_and_deadline_count_from_t_rx() {
        let f = Freshness {
            max_opportunity_age_ns: 1_000_000_000,
            execution_deadline_ns: 2_000_000_000,
        };
        assert!(!f.is_stale(0, 500_000_000));
        assert!(f.is_stale(0, 1_500_000_000));
        assert!(!f.deadline_passed(0, 1_500_000_000));
        assert!(f.deadline_passed(0, 2_500_000_000));
        // A clock behind T_rx never makes an opportunity stale.
        assert!(!f.is_stale(10, 5));
    }
}

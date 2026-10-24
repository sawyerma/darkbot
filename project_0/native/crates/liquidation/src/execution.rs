//! Execution-Härtung — Verhalten aus eva01 adaptiert (Merge-Entscheidung 8), kein Code kopiert:
//! - No-Route-Quarantäne pro Mint (verhindert sinnlose Router-/Build-Versuche)
//! - exponentielles Backoff pro Ziel-Account nach Fehlschlägen
//! - Simulate-first: senden nur, wenn die Simulation nicht fehlschlägt
//! - Jito-Status unklar → Signatur direkt on-chain prüfen (authoritative)
//! - Sequenzieller RPC-Fallback nur, wenn keine Exposure entstehen kann (Entscheidung 9)
//!
//! Zeit in Unix-Sekunden; alle Fenster sind Konfiguration.

use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;

/// Drift-keep-rs-Muster (adaptiert): stale Opportunities verwerfen, harte Deadline pro Versuch.
#[derive(Clone, Copy, Debug)]
pub struct Freshness {
    /// Opportunity älter als das (ab T_detect, ns) → nicht ausführen.
    pub max_opportunity_age_ns: u64,
    /// Build+Sign+Submit müssen innerhalb dieser Frist ab T_detect liegen.
    pub execution_deadline_ns: u64,
}

impl Freshness {
    pub fn is_stale(&self, t_detect_ns: u64, now_ns: u64) -> bool {
        now_ns.saturating_sub(t_detect_ns) > self.max_opportunity_age_ns
    }
    pub fn deadline_passed(&self, t_detect_ns: u64, now_ns: u64) -> bool {
        now_ns.saturating_sub(t_detect_ns) > self.execution_deadline_ns
    }
}

/// Parallel angefragte Routen (Jupiter || Titan): Auswahl nach **realisierbarem** Output nach Kosten,
/// nicht nach Quote-Brutto. `None`, wenn keine Route den Mindestoutput erreicht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteQuote {
    pub provider: &'static str,
    pub out_amount: u64,
    pub route_cost: u64,
    pub extra_accounts: usize,
    pub extra_bytes: usize,
}

pub fn select_route(quotes: &[RouteQuote], min_out: u64) -> Option<RouteQuote> {
    quotes
        .iter()
        .filter(|q| q.out_amount.saturating_sub(q.route_cost) >= min_out)
        .max_by_key(|q| q.out_amount.saturating_sub(q.route_cost))
        .copied()
}

/// Dynamische Priority-Fee: Perzentil über jüngste beobachtete Fees (µLamports/CU), gedeckelt.
pub fn priority_fee_percentile(samples: &mut [u64], p: f64, cap: u64) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    samples.sort_unstable();
    let idx = ((p.clamp(0.0, 1.0) * (samples.len() - 1) as f64).round()) as usize;
    Some(samples[idx].min(cap))
}

#[derive(Clone, Copy, Debug)]
pub struct ExecPolicy {
    pub quarantine_s: u64,
    pub backoff_base_s: u64,
    pub backoff_max_s: u64,
    pub max_failures_before_drop: u32,
    /// Sequenzieller RPC-Fallback nur erlaubt, wenn der Plan atomar bleibt (eine TX) —
    /// bei `[buy, liquidate]` wäre ein halb gelandeter Plan Exposure.
    pub allow_sequential_fallback_single_tx_only: bool,
}

#[derive(Default)]
pub struct NoRouteQuarantine {
    until: HashMap<Pubkey, u64>,
}

impl NoRouteQuarantine {
    pub fn quarantine(&mut self, mint: Pubkey, now: u64, policy: &ExecPolicy) {
        // A23-05: saturating — an end beyond u64 is "quarantined", never a wrap into the past.
        self.until
            .insert(mint, now.saturating_add(policy.quarantine_s));
    }
    pub fn is_quarantined(&mut self, mint: &Pubkey, now: u64) -> bool {
        match self.until.get(mint) {
            Some(u) if *u > now => true,
            Some(_) => {
                self.until.remove(mint);
                false
            }
            None => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct TargetState {
    failures: u32,
    next_allowed: u64,
}

#[derive(Default)]
pub struct TargetBackoff {
    targets: HashMap<Pubkey, TargetState>,
}

impl TargetBackoff {
    pub fn allowed(&self, account: &Pubkey, now: u64) -> bool {
        self.targets
            .get(account)
            .is_none_or(|t| now >= t.next_allowed)
    }
    /// Fehlschlag: 2^n × base, gedeckelt. Rückgabe `false`, wenn das Ziel nach `max_failures` fallengelassen wird.
    pub fn record_failure(&mut self, account: Pubkey, now: u64, policy: &ExecPolicy) -> bool {
        let t = self.targets.entry(account).or_default();
        t.failures += 1;
        let delay = policy
            .backoff_base_s
            .saturating_mul(1u64 << (t.failures.min(16) - 1))
            .min(policy.backoff_max_s);
        t.next_allowed = now.saturating_add(delay); // A23-05: never wraps into the past

        t.failures < policy.max_failures_before_drop
    }
    pub fn record_success(&mut self, account: &Pubkey) {
        self.targets.remove(account);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimOutcome {
    Ok { units_consumed: Option<u64> },
    Failed,
    Unavailable,
}

/// Gate: nur `Ok` darf gesendet werden. `Unavailable` (RPC-Fehler) ist kein Freifahrtschein.
pub fn simulate_gate(sim: SimOutcome) -> bool {
    matches!(sim, SimOutcome::Ok { .. })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundleView {
    Landed,
    Failed,
    Pending,
    Unknown,
}

/// Entscheidung nach Jito-Status: `Unknown`/`Pending` nach N Polls → on-chain Signaturprüfung ist authoritative.
pub fn needs_chain_confirmation(view: BundleView, polls: u32, max_polls: u32) -> bool {
    match view {
        BundleView::Landed | BundleView::Failed => false,
        BundleView::Pending | BundleView::Unknown => polls >= max_polls,
    }
}

/// Darf bei Bundle-Problemen sequenziell per RPC gesendet werden?
pub fn sequential_fallback_allowed(tx_count: usize, policy: &ExecPolicy) -> bool {
    if policy.allow_sequential_fallback_single_tx_only {
        tx_count == 1
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pol() -> ExecPolicy {
        ExecPolicy {
            quarantine_s: 600,
            backoff_base_s: 5,
            backoff_max_s: 300,
            max_failures_before_drop: 4,
            allow_sequential_fallback_single_tx_only: true,
        }
    }

    #[test]
    fn quarantine_expires() {
        let mut q = NoRouteQuarantine::default();
        let m = Pubkey::new_unique();
        q.quarantine(m, 1000, &pol());
        assert!(q.is_quarantined(&m, 1500));
        assert!(!q.is_quarantined(&m, 1700));
    }

    /// A23-05: quarantine and backoff ends beyond `u64` saturate (still blocked) instead of
    /// overflowing — a debug panic, a release wrap into the past that would unblock at once.
    #[test]
    fn a23_05_quarantine_and_backoff_ends_saturate() {
        let now = u64::MAX - 10;
        let mut q = NoRouteQuarantine::default();
        let m = Pubkey::new_unique();
        q.quarantine(m, now, &pol());
        assert!(q.is_quarantined(&m, now + 5));
        let (mut b, late) = (TargetBackoff::default(), u64::MAX - 2);
        let a = Pubkey::new_unique();
        assert!(b.record_failure(a, late, &pol())); // 5 s past u64::MAX
        assert!(!b.allowed(&a, u64::MAX - 1));
    }

    #[test]
    fn backoff_grows_and_drops() {
        let mut b = TargetBackoff::default();
        let a = Pubkey::new_unique();
        assert!(b.allowed(&a, 0));
        assert!(b.record_failure(a, 0, &pol())); // 5 s
        assert!(!b.allowed(&a, 4));
        assert!(b.allowed(&a, 5));
        assert!(b.record_failure(a, 5, &pol())); // 10 s
        assert!(!b.allowed(&a, 14));
        assert!(b.record_failure(a, 20, &pol())); // 20 s
        assert!(!b.record_failure(a, 40, &pol()), "4. Fehler → fallenlassen");
        b.record_success(&a);
        assert!(b.allowed(&a, 0));
    }

    #[test]
    fn freshness_route_fee() {
        let f = Freshness {
            max_opportunity_age_ns: 1_000_000_000,
            execution_deadline_ns: 1_000_000_000,
        };
        assert!(!f.is_stale(0, 500_000_000));
        assert!(f.is_stale(0, 1_500_000_000));
        let q = [
            RouteQuote {
                provider: "jup",
                out_amount: 1000,
                route_cost: 50,
                extra_accounts: 20,
                extra_bytes: 400,
            },
            RouteQuote {
                provider: "titan",
                out_amount: 990,
                route_cost: 10,
                extra_accounts: 12,
                extra_bytes: 300,
            },
        ];
        assert_eq!(
            select_route(&q, 900).unwrap().provider,
            "titan",
            "Netto nach Kosten entscheidet"
        );
        assert!(select_route(&q, 2000).is_none());
        let mut s = vec![5, 1, 9, 3, 7];
        assert_eq!(priority_fee_percentile(&mut s, 0.5, 100), Some(5));
        assert_eq!(priority_fee_percentile(&mut s, 1.0, 8), Some(8), "Cap");
        assert_eq!(priority_fee_percentile(&mut [], 0.5, 8), None);
    }

    #[test]
    fn gates() {
        assert!(simulate_gate(SimOutcome::Ok {
            units_consumed: Some(1)
        }));
        assert!(!simulate_gate(SimOutcome::Unavailable));
        assert!(needs_chain_confirmation(BundleView::Unknown, 3, 3));
        assert!(!needs_chain_confirmation(BundleView::Landed, 0, 3));
        assert!(sequential_fallback_allowed(1, &pol()));
        assert!(!sequential_fallback_allowed(2, &pol()));
    }
}

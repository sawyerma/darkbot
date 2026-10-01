//! P5: Receivership-Planner — Regeln, die unabhängig vom Instruction-Bau prüfbar sind.
//!
//! - Allowlist (P1A): zwischen StartLiquidation und EndLiquidation nur erlaubte Programme.
//! - Sequenz: [erlaubte Refreshes] → Start → Withdraw… → (Swap | Repay aus Inventar) → Repay → End.
//! - `EndLiquidation` ist die **letzte Instruction der Transaktion** — ohne Ausnahme.
//! - Bundle: nur Prerequisite-TXs + eine Receivership-TX; Start/End nie getrennt.
//! - Route-Reihenfolge: NO_SWAP → allowlisted vorberechnet → freie Quelle → paid (nach P7).
//!
//! Der eigentliche Instruction-Bau (marginfi-Ixs, Kamino/JupLend-Refresh, Jupiter/Titan-Swap)
//! kommt aus dem Crate bzw. den gepinnten IDLs (VERIFY, P5-05). Dieses Modul validiert Pläne.

use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Refresh(Pubkey),        // Programm, das refreshed wird (Kamino, Drift, JupLend, LiqRecord-Init)
    PriceUpdate(Pubkey),    // Oracle-Programm (falls Pull-Oracle, P0-03)
    StartLiquidation,
    Withdraw,
    IntegrationWithdraw(Pubkey),
    Swap(Pubkey),           // Swap-Programm
    RepayFromInventory,
    Repay,
    EndLiquidation,
    Other(Pubkey),          // alles andere (ATA-Close, Transfer, …)
}

#[derive(Clone, Debug, Default)]
pub struct Allowlist {
    pub refresh_programs: HashSet<Pubkey>,
    pub integration_programs: HashSet<Pubkey>,
    pub swap_programs: HashSet<Pubkey>,
    pub oracle_programs: HashSet<Pubkey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteKind {
    NoSwap,
    LocalPrecomputedAllowed,
    FreePublicAllowed,
    PaidAllowed,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PlanError {
    NoStart,
    NoEnd,
    EndNotLast,
    StepBeforeStartNotAllowed(usize),
    ProgramNotAllowlisted(usize),
    OrderViolation(usize, &'static str),
    RepayMissing,
    PaidRouteBeforeP7,
}

/// Validiert eine Receivership-TX (eine Transaktion). Indizes im Fehler = Position im Plan.
pub fn validate_receivership(steps: &[Step], allow: &Allowlist, route: RouteKind, p7_passed: bool) -> Result<(), PlanError> {
    let start = steps.iter().position(|s| *s == Step::StartLiquidation).ok_or(PlanError::NoStart)?;
    let end = steps.iter().position(|s| *s == Step::EndLiquidation).ok_or(PlanError::NoEnd)?;
    if end != steps.len() - 1 {
        return Err(PlanError::EndNotLast);
    }
    if route == RouteKind::PaidAllowed && !p7_passed {
        return Err(PlanError::PaidRouteBeforeP7);
    }
    // vor Start: nur erlaubte Refreshes / Preis-Updates
    for (i, s) in steps[..start].iter().enumerate() {
        match s {
            Step::Refresh(p) if allow.refresh_programs.contains(p) => {}
            Step::PriceUpdate(p) if allow.oracle_programs.contains(p) => {}
            _ => return Err(PlanError::StepBeforeStartNotAllowed(i)),
        }
    }
    // zwischen Start und End
    let mut seen_withdraw = false;
    let mut seen_repay = false;
    for (i, s) in steps.iter().enumerate().take(end).skip(start + 1) {
        match s {
            Step::Withdraw => seen_withdraw = true,
            Step::IntegrationWithdraw(p) => {
                if !allow.integration_programs.contains(p) {
                    return Err(PlanError::ProgramNotAllowlisted(i));
                }
                seen_withdraw = true;
            }
            Step::Swap(p) => {
                if route == RouteKind::NoSwap {
                    return Err(PlanError::OrderViolation(i, "swap in NO_SWAP plan"));
                }
                if !allow.swap_programs.contains(p) {
                    return Err(PlanError::ProgramNotAllowlisted(i));
                }
                if !seen_withdraw {
                    return Err(PlanError::OrderViolation(i, "swap before withdraw"));
                }
            }
            Step::RepayFromInventory | Step::Repay => seen_repay = true,
            Step::StartLiquidation => return Err(PlanError::OrderViolation(i, "duplicate start")),
            Step::Refresh(p) if allow.refresh_programs.contains(p) => {}
            Step::PriceUpdate(p) if allow.oracle_programs.contains(p) => {}
            Step::Other(_) | Step::Refresh(_) | Step::PriceUpdate(_) => return Err(PlanError::ProgramNotAllowlisted(i)),
            Step::EndLiquidation => unreachable!(),
        }
    }
    if !seen_repay {
        return Err(PlanError::RepayMissing);
    }
    Ok(())
}

/// Bundle-Regel: genau eine TX enthält Start und End; alle anderen TXs sind Prerequisites ohne beides.
pub fn validate_bundle(txs: &[Vec<Step>]) -> Result<usize, &'static str> {
    if txs.is_empty() || txs.len() > 5 {
        return Err("bundle size must be 1..=5");
    }
    let mut receivership_idx = None;
    for (i, tx) in txs.iter().enumerate() {
        let has_start = tx.contains(&Step::StartLiquidation);
        let has_end = tx.contains(&Step::EndLiquidation);
        match (has_start, has_end) {
            (true, true) => {
                if receivership_idx.is_some() {
                    return Err("more than one receivership tx");
                }
                receivership_idx = Some(i);
            }
            (false, false) => {}
            _ => return Err("start/end split across transactions"),
        }
    }
    receivership_idx.ok_or("no receivership tx in bundle")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allow() -> (Allowlist, Pubkey, Pubkey, Pubkey) {
        let (kamino, jup, other) = (Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique());
        let mut a = Allowlist::default();
        a.refresh_programs.insert(kamino);
        a.integration_programs.insert(kamino);
        a.swap_programs.insert(jup);
        (a, kamino, jup, other)
    }

    #[test]
    fn valid_swap_plan() {
        let (a, kamino, jup, _) = allow();
        let plan = vec![Step::Refresh(kamino), Step::StartLiquidation, Step::Withdraw, Step::Swap(jup), Step::Repay, Step::EndLiquidation];
        assert_eq!(validate_receivership(&plan, &a, RouteKind::FreePublicAllowed, false), Ok(()));
    }

    #[test]
    fn end_must_be_last_without_exception() {
        let (a, _, _, other) = allow();
        let plan = vec![Step::StartLiquidation, Step::Withdraw, Step::Repay, Step::EndLiquidation, Step::Other(other)];
        assert_eq!(validate_receivership(&plan, &a, RouteKind::NoSwap, false), Err(PlanError::EndNotLast));
    }

    #[test]
    fn non_allowlisted_swap_and_paid_route() {
        let (a, _, _, other) = allow();
        let plan = vec![Step::StartLiquidation, Step::Withdraw, Step::Swap(other), Step::Repay, Step::EndLiquidation];
        assert_eq!(validate_receivership(&plan, &a, RouteKind::FreePublicAllowed, false), Err(PlanError::ProgramNotAllowlisted(2)));
        let plan = vec![Step::StartLiquidation, Step::Withdraw, Step::Repay, Step::EndLiquidation];
        assert_eq!(validate_receivership(&plan, &a, RouteKind::PaidAllowed, false), Err(PlanError::PaidRouteBeforeP7));
        assert_eq!(validate_receivership(&plan, &a, RouteKind::NoSwap, false), Ok(()));
    }

    #[test]
    fn no_swap_plan_rejects_swap_and_requires_repay() {
        let (a, _, jup, _) = allow();
        let plan = vec![Step::StartLiquidation, Step::Withdraw, Step::Swap(jup), Step::Repay, Step::EndLiquidation];
        assert!(matches!(validate_receivership(&plan, &a, RouteKind::NoSwap, false), Err(PlanError::OrderViolation(2, _))));
        let plan = vec![Step::StartLiquidation, Step::Withdraw, Step::EndLiquidation];
        assert_eq!(validate_receivership(&plan, &a, RouteKind::NoSwap, false), Err(PlanError::RepayMissing));
    }

    #[test]
    fn bundle_rules() {
        let pre = vec![Step::Other(Pubkey::new_unique())];
        let recv = vec![Step::StartLiquidation, Step::Withdraw, Step::Repay, Step::EndLiquidation];
        assert_eq!(validate_bundle(&[pre.clone(), recv.clone()]), Ok(1));
        let split_a = vec![Step::StartLiquidation, Step::Withdraw];
        let split_b = vec![Step::Repay, Step::EndLiquidation];
        assert!(validate_bundle(&[split_a, split_b]).is_err());
        assert!(validate_bundle(&[pre.clone(); 6]).is_err());
    }
}

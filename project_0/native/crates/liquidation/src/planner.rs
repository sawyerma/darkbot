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

use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Refresh(Pubkey), // Programm, das refreshed wird (Kamino, Drift, JupLend)
    /// E2: the receivership program's LiquidationRecord initialization (only before Start).
    InitLiquidationRecord,
    PriceUpdate(Pubkey), // Oracle-Programm (falls Pull-Oracle, P0-03)
    StartLiquidation,
    Withdraw,
    IntegrationWithdraw(Pubkey),
    Swap(Pubkey), // Swap-Programm
    RepayFromInventory,
    Repay,
    EndLiquidation,
    Other(Pubkey), // alles andere (ATA-Close, Transfer, …)
}

#[derive(Clone, Debug, Default)]
pub struct Allowlist {
    pub refresh_programs: HashSet<Pubkey>,
    pub integration_programs: HashSet<Pubkey>,
    pub swap_programs: HashSet<Pubkey>,
    pub other_programs: HashSet<Pubkey>,
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
    /// E2: an instruction of the receivership program or of a pinned integration program whose
    /// discriminator is not one this plan may contain.
    UnknownInstruction(usize),
}

/// E2: how built instructions are recognized — the receivership program with its pinned
/// instruction discriminators and every pinned integration program with its refresh
/// discriminators (both from the pinned crates/IDLs, see `receivership::builders`).
#[derive(Clone, Debug, Default)]
pub struct InstructionTable {
    pub program: Pubkey,
    pub start: [u8; 8],
    pub end: [u8; 8],
    /// Plain and integration withdraws of the receivership program.
    pub withdraw: Vec<[u8; 8]>,
    pub repay: [u8; 8],
    pub init_record: [u8; 8],
    /// Integration program → its pinned refresh discriminators.
    pub refreshes: HashMap<Pubkey, Vec<[u8; 8]>>,
    pub swap_programs: HashSet<Pubkey>,
}

/// E2: the steps of an actually built transaction, from each instruction's program and 8-byte
/// discriminator. The receivership program and the pinned integration programs only appear
/// with a discriminator of the table (anything else is `UnknownInstruction`); a swap program
/// is a `Swap`; every other program is `Other`, left to the allowlist. The steps are a pure
/// function of the bytes, never an independently written list.
pub fn steps_of(ixs: &[Instruction], t: &InstructionTable) -> Result<Vec<Step>, PlanError> {
    ixs.iter()
        .enumerate()
        .map(|(i, ix)| {
            let disc: Option<[u8; 8]> = ix.data.get(..8).and_then(|d| d.try_into().ok());
            if ix.program_id == t.program {
                let d = disc.ok_or(PlanError::UnknownInstruction(i))?;
                return if d == t.start {
                    Ok(Step::StartLiquidation)
                } else if d == t.end {
                    Ok(Step::EndLiquidation)
                } else if t.withdraw.contains(&d) {
                    Ok(Step::Withdraw)
                } else if d == t.repay {
                    Ok(Step::Repay)
                } else if d == t.init_record {
                    Ok(Step::InitLiquidationRecord)
                } else {
                    Err(PlanError::UnknownInstruction(i))
                };
            }
            if let Some(refreshes) = t.refreshes.get(&ix.program_id) {
                return match disc {
                    Some(d) if refreshes.contains(&d) => Ok(Step::Refresh(ix.program_id)),
                    _ => Err(PlanError::UnknownInstruction(i)),
                };
            }
            if t.swap_programs.contains(&ix.program_id) {
                return Ok(Step::Swap(ix.program_id));
            }
            Ok(Step::Other(ix.program_id))
        })
        .collect()
}

/// Validiert eine Receivership-TX (eine Transaktion). Indizes im Fehler = Position im Plan.
pub fn validate_receivership(
    steps: &[Step],
    allow: &Allowlist,
    route: RouteKind,
    p7_passed: bool,
) -> Result<(), PlanError> {
    let start = steps
        .iter()
        .position(|s| *s == Step::StartLiquidation)
        .ok_or(PlanError::NoStart)?;
    let end = steps
        .iter()
        .position(|s| *s == Step::EndLiquidation)
        .ok_or(PlanError::NoEnd)?;
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
            Step::InitLiquidationRecord => {}
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
            // E2: a repay settles what a withdraw seized, never before it.
            Step::RepayFromInventory | Step::Repay => {
                if !seen_withdraw {
                    return Err(PlanError::OrderViolation(i, "repay before withdraw"));
                }
                seen_repay = true
            }
            Step::InitLiquidationRecord => {
                return Err(PlanError::OrderViolation(i, "init record after start"))
            }
            Step::StartLiquidation => return Err(PlanError::OrderViolation(i, "duplicate start")),
            Step::Refresh(p) if allow.refresh_programs.contains(p) => {}
            Step::PriceUpdate(p) if allow.oracle_programs.contains(p) => {}
            Step::Other(p) if allow.other_programs.contains(p) => {}
            Step::Other(_) | Step::Refresh(_) | Step::PriceUpdate(_) => {
                return Err(PlanError::ProgramNotAllowlisted(i))
            }
            Step::EndLiquidation => unreachable!(),
        }
    }
    if !seen_repay {
        return Err(PlanError::RepayMissing);
    }
    Ok(())
}

/// Bundle-Regel: genau eine TX enthält Start und End; alle anderen TXs sind Prerequisites ohne beides.
/// E2: a prerequisite transaction (no Start/End) carries only pinned swap steps — the JIT buy;
/// any other instruction (System/Token/ATA, a receivership step outside the receivership) needs
/// its own classification first and is refused, never allowed as a generic `Other`.
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
            (false, false) => {
                if tx.iter().any(|s| !matches!(s, Step::Swap(_))) {
                    return Err("prerequisite step is not a pinned swap");
                }
            }
            _ => return Err("start/end split across transactions"),
        }
    }
    receivership_idx.ok_or("no receivership tx in bundle")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> InstructionTable {
        let kamino = Pubkey::new_unique();
        InstructionTable {
            program: Pubkey::new_unique(),
            start: [1; 8],
            end: [2; 8],
            withdraw: vec![[3; 8], [4; 8]],
            repay: [5; 8],
            init_record: [6; 8],
            refreshes: HashMap::from([(kamino, vec![[7; 8]])]),
            swap_programs: HashSet::from([Pubkey::new_unique()]),
        }
    }
    fn ix(program: Pubkey, disc: [u8; 8]) -> Instruction {
        let mut data = disc.to_vec();
        data.extend_from_slice(&[9, 9]);
        Instruction {
            program_id: program,
            accounts: vec![],
            data,
        }
    }

    /// E2: steps come from program + discriminator of the built instructions: order, End last,
    /// duplicate Start, foreign program with a Start discriminator, unknown discriminators.
    #[test]
    fn e2_steps_are_derived_from_built_instructions() {
        let t = table();
        let kamino = *t.refreshes.keys().next().unwrap();
        let swap = *t.swap_programs.iter().next().unwrap();
        let p = t.program;
        let built = [
            ix(kamino, [7; 8]),
            ix(p, [6; 8]),
            ix(p, [1; 8]),
            ix(p, [4; 8]),
            ix(swap, [42; 8]),
            ix(p, [5; 8]),
            ix(p, [2; 8]),
        ];
        assert_eq!(
            steps_of(&built, &t).unwrap(),
            [
                Step::Refresh(kamino),
                Step::InitLiquidationRecord,
                Step::StartLiquidation,
                Step::Withdraw,
                Step::Swap(swap),
                Step::Repay,
                Step::EndLiquidation,
            ]
        );
        let mut a = Allowlist::default();
        a.refresh_programs.insert(kamino);
        a.swap_programs.insert(swap);
        let check = |ixs: &[Instruction]| {
            validate_receivership(&steps_of(ixs, &t)?, &a, RouteKind::FreePublicAllowed, false)
        };
        assert_eq!(check(&built), Ok(()));
        // End not last: anything after the real End instruction.
        let mut after = built.to_vec();
        after.push(ix(Pubkey::new_unique(), [0; 8]));
        assert_eq!(check(&after), Err(PlanError::EndNotLast));
        // Second Start.
        let mut twice = built.to_vec();
        twice.insert(3, ix(p, [1; 8]));
        assert_eq!(
            check(&twice),
            Err(PlanError::OrderViolation(3, "duplicate start"))
        );
        // A foreign program carrying the Start discriminator is no Start.
        let mut foreign = built.to_vec();
        foreign[2] = ix(Pubkey::new_unique(), [1; 8]);
        assert_eq!(check(&foreign), Err(PlanError::NoStart));
        // Unknown discriminator on the receivership program or on a pinned integration program.
        let mut unknown = built.to_vec();
        unknown[3] = ix(p, [8; 8]);
        assert_eq!(check(&unknown), Err(PlanError::UnknownInstruction(3)));
        let mut refresh = built.to_vec();
        refresh[0] = ix(kamino, [8; 8]);
        assert_eq!(check(&refresh), Err(PlanError::UnknownInstruction(0)));
        let short = [Instruction {
            program_id: p,
            accounts: vec![],
            data: vec![1, 1],
        }];
        assert_eq!(steps_of(&short, &t), Err(PlanError::UnknownInstruction(0)));
        // Init record only before Start; repay only after a withdraw.
        let mut late_init = built.to_vec();
        late_init.swap(1, 2);
        assert_eq!(
            check(&late_init),
            Err(PlanError::OrderViolation(2, "init record after start"))
        );
        let mut early_repay = built.to_vec();
        early_repay.swap(3, 5);
        assert_eq!(
            check(&early_repay),
            Err(PlanError::OrderViolation(3, "repay before withdraw"))
        );
        // A prerequisite containing Start/End is not a prerequisite.
        let pre = steps_of(&[ix(swap, [42; 8])], &t).unwrap();
        let recv = steps_of(&built, &t).unwrap();
        assert_eq!(validate_bundle(&[pre.clone(), recv.clone()]), Ok(1));
        assert!(validate_bundle(&[recv.clone(), recv]).is_err());
    }

    fn allow() -> (Allowlist, Pubkey, Pubkey, Pubkey) {
        let (kamino, jup, other) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let mut a = Allowlist::default();
        a.refresh_programs.insert(kamino);
        a.integration_programs.insert(kamino);
        a.swap_programs.insert(jup);
        a.other_programs.insert(other);
        (a, kamino, jup, other)
    }

    #[test]
    fn valid_swap_plan() {
        let (a, kamino, jup, _) = allow();
        let plan = vec![
            Step::Refresh(kamino),
            Step::StartLiquidation,
            Step::Withdraw,
            Step::Swap(jup),
            Step::Repay,
            Step::EndLiquidation,
        ];
        assert_eq!(
            validate_receivership(&plan, &a, RouteKind::FreePublicAllowed, false),
            Ok(())
        );
    }

    #[test]
    fn end_must_be_last_without_exception() {
        let (a, _, _, other) = allow();
        let plan = vec![
            Step::StartLiquidation,
            Step::Withdraw,
            Step::Repay,
            Step::EndLiquidation,
            Step::Other(other),
        ];
        assert_eq!(
            validate_receivership(&plan, &a, RouteKind::NoSwap, false),
            Err(PlanError::EndNotLast)
        );
    }

    #[test]
    fn non_allowlisted_swap_and_paid_route() {
        let (a, _, _, other) = allow();
        let plan = vec![
            Step::StartLiquidation,
            Step::Withdraw,
            Step::Swap(other),
            Step::Repay,
            Step::EndLiquidation,
        ];
        assert_eq!(
            validate_receivership(&plan, &a, RouteKind::FreePublicAllowed, false),
            Err(PlanError::ProgramNotAllowlisted(2))
        );
        let plan = vec![
            Step::StartLiquidation,
            Step::Withdraw,
            Step::Repay,
            Step::EndLiquidation,
        ];
        assert_eq!(
            validate_receivership(&plan, &a, RouteKind::PaidAllowed, false),
            Err(PlanError::PaidRouteBeforeP7)
        );
        assert_eq!(
            validate_receivership(&plan, &a, RouteKind::NoSwap, false),
            Ok(())
        );
    }

    #[test]
    fn no_swap_plan_rejects_swap_and_requires_repay() {
        let (a, _, jup, _) = allow();
        let plan = vec![
            Step::StartLiquidation,
            Step::Withdraw,
            Step::Swap(jup),
            Step::Repay,
            Step::EndLiquidation,
        ];
        assert!(matches!(
            validate_receivership(&plan, &a, RouteKind::NoSwap, false),
            Err(PlanError::OrderViolation(2, _))
        ));
        let plan = vec![Step::StartLiquidation, Step::Withdraw, Step::EndLiquidation];
        assert_eq!(
            validate_receivership(&plan, &a, RouteKind::NoSwap, false),
            Err(PlanError::RepayMissing)
        );
    }

    #[test]
    fn bundle_rules() {
        let pre = vec![Step::Swap(Pubkey::new_unique())];
        // E2: a prerequisite is swaps only — no generic `Other`, no receivership step outside.
        for bad in [
            Step::Other(Pubkey::new_unique()),
            Step::Withdraw,
            Step::Repay,
        ] {
            let recv = vec![
                Step::StartLiquidation,
                Step::Withdraw,
                Step::Repay,
                Step::EndLiquidation,
            ];
            assert_eq!(
                validate_bundle(&[vec![Step::Swap(Pubkey::new_unique()), bad], recv]),
                Err("prerequisite step is not a pinned swap")
            );
        }
        let recv = vec![
            Step::StartLiquidation,
            Step::Withdraw,
            Step::Repay,
            Step::EndLiquidation,
        ];
        assert_eq!(validate_bundle(&[pre.clone(), recv.clone()]), Ok(1));
        let split_a = vec![Step::StartLiquidation, Step::Withdraw];
        let split_b = vec![Step::Repay, Step::EndLiquidation];
        assert!(validate_bundle(&[split_a, split_b]).is_err());
        assert!(validate_bundle(&vec![pre.clone(); 6]).is_err()); // [FIX-55] Vec<T>: Clone, not Copy
    }
}

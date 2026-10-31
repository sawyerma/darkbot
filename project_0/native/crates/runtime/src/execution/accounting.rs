//! FE-04/FE-07 finalized accounting: the cumulative realized result of every own attempt of a
//! wallet, read only from its FE-04 journals (archived attempts and the active one) — the single
//! financial authority. Realized is the attempt's SOL delta and token outflows plus its exits'
//! proceeds; open inventory is a mark, never realized; anything not settled is UNKNOWN. No
//! profit is claimed without every attempt and exit settled, no position left and a positive
//! cumulative sum. The treasury view (FEUI-10A, W-G02/W-G03) is read-only: no transfer exists.
use crate::execution::one_shot::{exit::ExitSettlement, exit::Position, settlement::Settlement};
use std::collections::BTreeMap;

/// The state of one attempt or exit in its journal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    /// Durably reserved, not handed off (a restart makes it UNKNOWN).
    Reserved,
    /// Handed off; the outcome is not finalized.
    Unknown,
    /// Finalized and consistent; the booking still waits (e.g. a value).
    Finalized,
    Settled,
}
impl StepState {
    pub fn name(self) -> &'static str {
        match self {
            StepState::Reserved => "RESERVED",
            StepState::Unknown => "UNKNOWN",
            StepState::Finalized => "FINALIZED_UNBOOKED",
            StepState::Settled => "SETTLED",
        }
    }
}

/// One exit of an attempt's open inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExitRecord {
    pub approval_id: String,
    pub mint: String,
    pub state: StepState,
    pub settlement: Option<ExitSettlement>,
    /// P8-CTRL.5: the FE-06 grant the exit is booked against (`None`: none, or before it).
    pub fe06: Option<crate::execution::one_shot::Fe06Attempt>,
    /// The exit's bound network fee — what it may spend.
    pub fee_lamports: u64,
}

/// One own attempt with its exits, as its journal holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttemptRecord {
    pub approval_id: String,
    pub target: String,
    pub funding_class: String,
    pub archived: bool,
    pub state: StepState,
    pub settlement: Option<Settlement>,
    pub exits: Vec<ExitRecord>,
    /// The open inventory after the settled exits.
    pub open_positions: BTreeMap<String, Position>,
    /// DEC-04: the holds the attempt was admitted with (`None`: OFF or before DEC-04).
    pub hold: Option<crate::execution::hold_b::HoldEvidence>,
    /// P8-CTRL.5: the FE-06 grant it is booked against (`None`: none, or before P8-CTRL).
    pub fe06: Option<crate::execution::one_shot::Fe06Attempt>,
    /// P8-CTRL.5: what its intent committed — its capital, worst-case loss and SOL debit (the
    /// intent's SOL hold; `None` without one).
    pub capital_usd_micro: u64,
    pub max_loss_usd_micro: u64,
    pub sol_debit_lamports: Option<u64>,
}

/// FE-07: the cumulative decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// No attempt landed (none made, or every one proven not landed): nothing was paid.
    NoTrade,
    FirstRealizedProfitEvidenced,
    Loss,
    /// Something is not settled, a position is open or a sum overflowed.
    Unknown(Vec<String>),
}
impl Verdict {
    pub fn name(&self) -> &'static str {
        match self {
            Verdict::NoTrade => "NO_TRADE",
            Verdict::FirstRealizedProfitEvidenced => "FIRST_REALIZED_PROFIT_EVIDENCED",
            Verdict::Loss => "LOSS",
            Verdict::Unknown(_) => "UNKNOWN",
        }
    }
}

/// Sums over the settled steps only (an unsettled step is in `Verdict::Unknown`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub attempts: usize,
    pub archived: usize,
    pub landed_attempts: usize,
    pub settled_exits: usize,
    /// `None` on overflow.
    pub realized_net_usd_micro: Option<i128>,
    pub fee_lamports: Option<u64>,
    pub open_inventory_mark_usd_micro: Option<i128>,
}

/// Every own attempt of one wallet, oldest archived first, the active one last.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    pub attempts: Vec<AttemptRecord>,
}

impl Ledger {
    /// Reads the archive and the active journal (read-only, no wallet lock). A journal that
    /// cannot be read is an error, never an empty ledger.
    pub fn load(directory: &std::path::Path, wallet: &str) -> Result<Self, String> {
        crate::execution::one_shot::archive::records(directory, wallet)
            .map(|attempts| Self { attempts })
    }

    /// DEC-04: the B evidence of the landed, settled attempts — each B against its finalized
    /// wallet deltas and the inventory it left (only a successful one counts as exact).
    pub fn b_evidence(&self) -> crate::execution::hold_b::BEvidence {
        let mut e = crate::execution::hold_b::BEvidence::default();
        for a in &self.attempts {
            if let (Some(hold), Some(s)) = (&a.hold, &a.settlement) {
                if s.landed {
                    e.add(
                        hold,
                        &crate::execution::hold_b::Finalized {
                            funding_class: &a.funding_class,
                            success: s.landed_success,
                            deltas: &s.wallet_deltas,
                            inventory_usd_micro: s.unrealized_inventory_usd_micro,
                        },
                    );
                }
            }
        }
        e
    }

    pub fn totals(&self) -> Totals {
        let mut t = Totals {
            attempts: self.attempts.len(),
            archived: self.attempts.iter().filter(|a| a.archived).count(),
            realized_net_usd_micro: Some(0),
            fee_lamports: Some(0),
            open_inventory_mark_usd_micro: Some(0),
            ..Default::default()
        };
        for a in &self.attempts {
            if let Some(s) = &a.settlement {
                t.landed_attempts += usize::from(s.landed);
                t.realized_net_usd_micro = t
                    .realized_net_usd_micro
                    .and_then(|r| r.checked_add(s.realized_net_usd_micro));
                t.fee_lamports = t.fee_lamports.and_then(|f| f.checked_add(s.fee_lamports));
            }
            for e in &a.exits {
                if let Some(x) = &e.settlement {
                    t.settled_exits += 1;
                    t.realized_net_usd_micro = t
                        .realized_net_usd_micro
                        .and_then(|r| r.checked_add(x.realized_usd_micro));
                    t.fee_lamports = t.fee_lamports.and_then(|f| f.checked_add(x.fee_lamports));
                }
            }
            // F-07: one unvalued position makes the open inventory's mark Unknown (never 0).
            for p in a.open_positions.values() {
                t.open_inventory_mark_usd_micro = t
                    .open_inventory_mark_usd_micro
                    .and_then(|m| p.mark_usd_micro.and_then(|x| m.checked_add(x)));
            }
        }
        t
    }

    /// FE-07: `FIRST_REALIZED_PROFIT_EVIDENCED` only with every attempt and exit settled, no
    /// position left and a positive cumulative realized net; `NO_TRADE` when nothing landed.
    pub fn verdict(&self) -> Verdict {
        let mut unknown = Vec::new();
        for a in &self.attempts {
            if a.state != StepState::Settled {
                unknown.push(format!("ATTEMPT_{}:{}", a.state.name(), a.approval_id));
            }
            for e in &a.exits {
                if e.state != StepState::Settled {
                    unknown.push(format!("EXIT_{}:{}", e.state.name(), e.approval_id));
                }
            }
            for (mint, p) in &a.open_positions {
                if p.units != 0 {
                    unknown.push(format!("OPEN_INVENTORY:{}:{mint}", a.approval_id));
                }
            }
        }
        let totals = self.totals();
        let Some(realized) = totals.realized_net_usd_micro else {
            unknown.push("REALIZED_OVERFLOW".into());
            return Verdict::Unknown(unknown);
        };
        if !unknown.is_empty() {
            return Verdict::Unknown(unknown);
        }
        if totals.landed_attempts == 0 && realized == 0 {
            return Verdict::NoTrade;
        }
        if realized > 0 {
            Verdict::FirstRealizedProfitEvidenced
        } else {
            Verdict::Loss
        }
    }

    /// The terminal report (text lines, offline): every attempt and exit, the sums, the verdict.
    pub fn report(&self) -> Vec<String> {
        let opt = |v: Option<i128>| v.map_or("UNKNOWN".to_string(), |v| v.to_string());
        let mut out = Vec::new();
        for a in &self.attempts {
            let s = a.settlement.as_ref();
            out.push(format!(
                "attempt={} archived={} state={} class={} target={} landed={} success={} realized_net_usd_micro={} fee_lamports={} exits={} open_positions={}",
                a.approval_id,
                a.archived,
                a.state.name(),
                a.funding_class,
                a.target,
                s.map_or("-".into(), |s| s.landed.to_string()),
                s.map_or("-".into(), |s| s.landed_success.to_string()),
                opt(s.map(|s| s.realized_net_usd_micro)),
                s.map_or("UNKNOWN".into(), |s| s.fee_lamports.to_string()),
                a.exits.len(),
                a.open_positions.values().filter(|p| p.units != 0).count(),
            ));
            for e in &a.exits {
                let x = e.settlement.as_ref();
                out.push(format!(
                    "exit={} attempt={} mint={} state={} sold_units={} realized_usd_micro={}",
                    e.approval_id,
                    a.approval_id,
                    e.mint,
                    e.state.name(),
                    x.map_or("-".into(), |x| x.sold_units.to_string()),
                    opt(x.map(|x| x.realized_usd_micro)),
                ));
            }
        }
        let t = self.totals();
        out.push(format!(
            "attempts={} archived={} landed_attempts={} settled_exits={}",
            t.attempts, t.archived, t.landed_attempts, t.settled_exits
        ));
        out.push(format!(
            "cumulative_realized_net_usd_micro={} fee_lamports={} open_inventory_mark_usd_micro={}",
            opt(t.realized_net_usd_micro),
            t.fee_lamports.map_or("UNKNOWN".into(), |f| f.to_string()),
            opt(t.open_inventory_mark_usd_micro),
        ));
        out.push(match self.realized_deltas() {
            Some(d) => format!(
                "realized_deltas={}",
                d.iter()
                    .map(|(k, v)| format!("{k}:{v}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            None => "realized_deltas=UNKNOWN".into(),
        });
        let verdict = self.verdict();
        out.push(match &verdict {
            Verdict::Unknown(reasons) => format!("verdict=UNKNOWN reasons={}", reasons.join(",")),
            v => format!("verdict={}", v.name()),
        });
        out
    }
}

/// FEUI-10A/W-G02 (read-only, no transfer exists): what of one asset could at most leave the
/// wallet as realized profit — `min(realized profit in this asset, spendable after holds and
/// the never-spent reserve)`. No sweep ledger exists, so nothing was swept yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Withdrawable {
    Known(u64),
    Unknown(String),
}

impl Ledger {
    /// Per asset key (`"SOL"` or the mint): the net realized delta of every settled attempt and
    /// exit, `None` when a step is not settled or a sum overflows.
    pub fn realized_deltas(&self) -> Option<BTreeMap<String, i128>> {
        if matches!(self.verdict(), Verdict::Unknown(_)) {
            return None;
        }
        let mut out: BTreeMap<String, i128> = BTreeMap::new();
        for a in &self.attempts {
            let steps = a.settlement.iter().map(|s| &s.wallet_deltas).chain(
                a.exits
                    .iter()
                    .filter_map(|e| e.settlement.as_ref())
                    .map(|x| &x.wallet_deltas),
            );
            for deltas in steps {
                for (asset, d) in deltas {
                    let v = out.entry(asset.clone()).or_default();
                    *v = v.checked_add(*d)?;
                }
            }
        }
        Some(out)
    }

    /// `asset`'s withdrawable realized profit given the reconciled `balances`, the holds of
    /// every attempt (`held`) and the policy `reserve`. UNKNOWN unless the accounting is
    /// settled with a positive verdict, the balance, the hold and the reserve of `asset` are given
    /// (an absent entry is UNKNOWN, D09; an explicit 0 is known) and no other asset was consumed
    /// (a cross-asset profit needs a qualified price, which this ledger does not have).
    pub fn withdrawable(
        &self,
        asset: &str,
        balances: &BTreeMap<String, u64>,
        held: &BTreeMap<String, u64>,
        reserve: &BTreeMap<String, u64>,
    ) -> Withdrawable {
        let unknown = |r: &str| Withdrawable::Unknown(r.into());
        if self.verdict() != Verdict::FirstRealizedProfitEvidenced {
            return unknown("NO_SETTLED_REALIZED_PROFIT");
        }
        let Some(deltas) = self.realized_deltas() else {
            return unknown("ACCOUNTING_UNKNOWN");
        };
        if deltas.iter().any(|(k, d)| k != asset && *d < 0) {
            return unknown("CROSS_ASSET_PROFIT_NEEDS_PRICE");
        }
        let profit = deltas.get(asset).copied().unwrap_or(0).max(0);
        let Some(balance) = balances.get(asset) else {
            return unknown("BALANCE_UNKNOWN");
        };
        // D09: a hold or reserve the caller did not provide is Unknown (an explicit 0 is known).
        let Some(held) = held.get(asset) else {
            return unknown("HOLD_UNKNOWN");
        };
        let Some(reserve) = reserve.get(asset) else {
            return unknown("RESERVE_UNKNOWN");
        };
        let spendable = balance.saturating_sub(*held).saturating_sub(*reserve);
        Withdrawable::Known(u64::try_from(profit).unwrap_or(u64::MAX).min(spendable))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(deltas: &[(&str, i128)], realized: i128) -> AttemptRecord {
        AttemptRecord {
            approval_id: "a-1".into(),
            target: "t".into(),
            funding_class: "NO_SWAP_INVENTORY".into(),
            archived: true,
            state: StepState::Settled,
            settlement: Some(Settlement {
                landed: true,
                landed_success: true,
                fee_lamports: 5_000,
                wallet_deltas: deltas.iter().map(|(k, d)| (k.to_string(), *d)).collect(),
                valuation: BTreeMap::new(),
                realized_net_usd_micro: realized,
                open_inventory: BTreeMap::new(),
                unrealized_inventory_usd_micro: 0,
                booked_unix_s: Some(1),
            }),
            exits: vec![],
            open_positions: BTreeMap::new(),
            hold: None,
            fe06: None,
            capital_usd_micro: 0,
            max_loss_usd_micro: 0,
            sol_debit_lamports: Some(0),
        }
    }

    /// DEC-04 (review H1): only landed attempts are evidence, a failed one within B is none; an
    /// underestimation counts from any landed attempt.
    #[test]
    fn dec04_b_evidence_counts_landed_attempts_only() {
        use crate::execution::hold_b::{AssetFlow, BHold, HoldEvidence, HoldMode, PlanFlows};
        let hold = HoldEvidence {
            mode: HoldMode::Observe,
            a_inventory_usd_micro: 1_100,
            b: BHold::Exact(PlanFlows {
                flows: [(
                    "SOL".to_string(),
                    AssetFlow {
                        reserve: 5_000,
                        residual_max: Some(0),
                    },
                )]
                .into(),
                inventory_usd_micro: 0,
                kind: "PLAIN".into(),
            }),
            reserved_inventory_usd_micro: 1_100,
        };
        let with = |landed: bool, success: bool, sol: i128| {
            let mut a = settled(&[("SOL", sol)], 0);
            a.hold = Some(hold.clone());
            let s = a.settlement.as_mut().unwrap();
            (s.landed, s.landed_success) = (landed, success);
            a
        };
        let ledger = |attempts| Ledger { attempts };
        let e = ledger(vec![with(false, false, 0), with(true, false, -5_000)]).b_evidence();
        assert_eq!((e.exact, e.underestimated), (0, 0));
        let e = ledger(vec![with(true, true, -5_000), with(true, false, -6_000)]).b_evidence();
        assert_eq!((e.exact, e.underestimated), (1, 1));
    }

    /// F-07: an unvalued open position makes the open inventory's total mark Unknown, never 0;
    /// a valued one adds its mark.
    #[test]
    fn f07_open_inventory_mark_is_unknown_with_an_unvalued_position() {
        let mut a = settled(&[("SOL", -5_000), ("MINT", 1_000)], -1);
        a.open_positions.insert(
            "MINT".into(),
            Position {
                units: 1_000,
                mark_usd_micro: Some(700),
            },
        );
        let valued = Ledger {
            attempts: vec![a.clone()],
        };
        assert_eq!(valued.totals().open_inventory_mark_usd_micro, Some(700));
        a.open_positions.get_mut("MINT").unwrap().mark_usd_micro = None;
        let unvalued = Ledger { attempts: vec![a] };
        assert_eq!(unvalued.totals().open_inventory_mark_usd_micro, None);
    }

    /// D09 (P0-PROD-CTRL-08): a hold or reserve entry the caller did not provide is Unknown,
    /// never 0 — an explicit 0 is a known "nothing held / no reserve".
    #[test]
    fn d09_missing_hold_or_reserve_entry_is_unknown_never_zero() {
        let m = |v: &[(&str, u64)]| -> BTreeMap<String, u64> {
            v.iter().map(|(k, n)| (k.to_string(), *n)).collect()
        };
        let sol_only = Ledger {
            attempts: vec![settled(&[("SOL", 15_000), ("MINT", 0)], 1_500)],
        };
        let balance = m(&[("SOL", 100_000)]);
        assert_eq!(
            sol_only.withdrawable("SOL", &balance, &m(&[]), &m(&[("SOL", 0)])),
            Withdrawable::Unknown("HOLD_UNKNOWN".into())
        );
        assert_eq!(
            sol_only.withdrawable("SOL", &balance, &m(&[("SOL", 0)]), &m(&[])),
            Withdrawable::Unknown("RESERVE_UNKNOWN".into())
        );
        assert_eq!(
            sol_only.withdrawable("SOL", &balance, &m(&[("SOL", 0)]), &m(&[("SOL", 0)])),
            Withdrawable::Known(15_000)
        );
    }

    /// W-G02/W-G03 (read-only): withdrawable = min(realized profit in the asset, balance minus
    /// every hold and the never-spent reserve); UNKNOWN without a settled positive verdict, a
    /// known balance, or when another asset was consumed (that profit needs a price).
    #[test]
    fn withdrawable_is_the_settled_profit_in_the_asset_after_holds_and_reserve() {
        let m = |v: &[(&str, u64)]| -> BTreeMap<String, u64> {
            v.iter().map(|(k, n)| (k.to_string(), *n)).collect()
        };
        let sol_only = Ledger {
            attempts: vec![settled(&[("SOL", 15_000), ("MINT", 0)], 1_500)],
        };
        assert_eq!(sol_only.realized_deltas().unwrap()["SOL"], 15_000);
        assert_eq!(
            sol_only.withdrawable(
                "SOL",
                &m(&[("SOL", 100_000)]),
                &m(&[("SOL", 0)]),
                &m(&[("SOL", 0)])
            ),
            Withdrawable::Known(15_000)
        );
        assert_eq!(
            sol_only.withdrawable(
                "SOL",
                &m(&[("SOL", 100_000)]),
                &m(&[("SOL", 40_000)]),
                &m(&[("SOL", 50_000)])
            ),
            Withdrawable::Known(10_000),
            "holds and the reserve are never withdrawable"
        );
        assert_eq!(
            sol_only.withdrawable("SOL", &m(&[]), &m(&[]), &m(&[])),
            Withdrawable::Unknown("BALANCE_UNKNOWN".into())
        );
        assert_eq!(
            sol_only.withdrawable(
                "MINT",
                &m(&[("MINT", 7)]),
                &m(&[("MINT", 0)]),
                &m(&[("MINT", 0)])
            ),
            Withdrawable::Known(0),
            "no realized profit in this asset"
        );
        let cross = Ledger {
            attempts: vec![settled(&[("SOL", 15_000), ("USDC", -5_000)], 1)],
        };
        assert_eq!(
            cross.withdrawable("SOL", &m(&[("SOL", 100_000)]), &m(&[]), &m(&[])),
            Withdrawable::Unknown("CROSS_ASSET_PROFIT_NEEDS_PRICE".into())
        );
        let loss = Ledger {
            attempts: vec![settled(&[("SOL", -5_000)], -500)],
        };
        assert_eq!(
            loss.withdrawable("SOL", &m(&[("SOL", 100_000)]), &m(&[]), &m(&[])),
            Withdrawable::Unknown("NO_SETTLED_REALIZED_PROFIT".into())
        );
        let mut unsettled = sol_only.clone();
        unsettled.attempts[0].state = StepState::Unknown;
        assert!(unsettled.realized_deltas().is_none());
    }
}

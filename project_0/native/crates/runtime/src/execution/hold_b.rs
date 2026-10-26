//! DEC-04 (V7): the two holds of an FE-04 attempt and the exact flows of its signed plan.
//!
//! * **A** (authoritative): the wallet reservation — only the spendable units the plan debits,
//!   per asset (SOL: network fee, tip, flat protocol fee, record rent; every inventory it spends)
//!   against the balance minus other holds minus the policy reserve — and the inventory limit —
//!   the conservative (high) value of the whole seizure against the Safety inventory limit, never
//!   against a balance (`builder::inventory_hold_usd_micro`).
//! * **B** (built completely): the per-asset flows decoded from the signed instructions — the
//!   withdraw and repay amounts (`built_amounts`, parity with the sizing), the integration
//!   conversion (Kamino, Drift, JupLend: the credited underlying, recomputed from state), the
//!   signed swap terms (ExactIn input, floor, quote), Token-2022 transfer fees (a fee-bearing or
//!   unmodelled mint has no fee model: unqualified) and the flash repayment (FLASH_FUNDED is
//!   blocked) — what the wallet must hold before the attempt (`reserve`) and keeps at most after it
//!   (`residual_max`; a swap's liability inflow has no bound in units, its value is bounded by the
//!   swapped input at the high price less the repay at the low price), the residual inventory
//!   valued conservatively. What B cannot derive is UNQUALIFIED, and A applies, visibly.
//! * **Modes** (`HOLD_B`): OFF computes nothing; OBSERVE (the default) records A, B and the
//!   finalized flows per attempt; ACTIVE holds B's inventory value for a plan of a qualified class
//!   (funding class and integration) — effective only on a recorded request with the user's N
//!   (B-8, `hold_b_min_attempts`, no default) and N successful finalized attempts of that class
//!   without any underestimation. Otherwise OBSERVE, with the reason; never promoted unrequested.
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::collections::BTreeMap;

/// The capability of the B hold mode.
pub const HOLD_B: &str = "HOLD_B";
/// The values HOLD_B may be requested with.
pub const HOLD_B_VALUES: [&str; 3] = ["OFF", "OBSERVE", "ACTIVE"];
/// ACTIVE is not effective: the user has not decided N (B-8).
pub const B8_UNDECIDED: &str = "B-8: N not decided (hold_b_min_attempts unset)";
/// ACTIVE is not effective: B underestimated a finalized attempt.
pub const B_UNDERESTIMATED: &str = "B underestimated a finalized attempt";
/// ACTIVE is not effective: fewer than N successful attempts with an exact B.
pub const B_EVIDENCE_SHORT: &str = "fewer than N finalized attempts with an exact B";
/// ACTIVE is not effective: the owner's decision on the request was not recorded.
pub const B_DECISION_UNRECORDED: &str = "the ACTIVE decision was not recorded";

/// The B hold mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HoldMode {
    Off,
    #[default]
    Observe,
    Active,
}

impl HoldMode {
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
}

/// The sizing facts of a plan its B is checked against (from the builder, never re-sized).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoldFacts {
    pub asset_bank: Pubkey,
    pub liab_bank: Pubkey,
    pub asset_mint: Pubkey,
    pub liab_mint: Pubkey,
    /// The withdrawal in bank units (Marginfi balance accounting).
    pub withdraw_bank_units: u64,
    pub repay_amount: u64,
    pub repay_all: bool,
    /// A's inventory hold: the conservative value of the whole seizure.
    pub seized_hold_value: u64,
    /// The repay amount at the low-biased liability price (`None`: unknown).
    pub repay_low_value: Option<u64>,
    /// JIT: the mint the buy spends.
    pub funding_mint: Option<Pubkey>,
}

/// One asset's flow in the signed plan, base units: what the wallet must hold before the attempt
/// (spent before an in-plan inflow covers it) and what it keeps at most after it (`None`: no bound
/// in units — a swap's output — the value is bounded in B's inventory hold).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetFlow {
    pub reserve: u64,
    pub residual_max: Option<u64>,
}

/// B: the exact flows of one signed plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanFlows {
    /// Per asset ("SOL" or the mint).
    pub flows: BTreeMap<String, AssetFlow>,
    /// The residual inventory at its conservative value: B's inventory hold.
    pub inventory_usd_micro: u64,
    /// The plan's kind: PLAIN, KAMINO, DRIFT or JUPLEND (the withdrawal's integration).
    pub kind: String,
}

/// B of one plan, or why it has none (A applies).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BHold {
    Exact(PlanFlows),
    Unqualified(String),
}

/// What an attempt's journal keeps of its holds: the mode it was admitted in, A's inventory hold
/// (A's wallet reservation is the intent's holds), B, and the inventory hold Safety reserved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldEvidence {
    pub mode: HoldMode,
    pub a_inventory_usd_micro: u64,
    pub b: BHold,
    pub reserved_inventory_usd_micro: u64,
}

/// B's computation from a signed plan (the builder's crates: integration conversion, swap
/// terms, the receivership decoder).
#[cfg(feature = "crate_health")]
mod compute {
    use super::*;
    use crate::execution::pipeline::ReceivershipPlan;
    use darkbot_project0::funding::FundingClass;
    use darkbot_project0::integration::{IntegrationExecutionAmount, IntegrationKind};

    /// What B is derived from besides the plan: the program and the taker of the signed
    /// instructions, the withdrawal's integration conversion recomputed from state (`None`: a plain
    /// bank), an unmodelled mint of the plan and the plan's SOL debit.
    pub struct BInputs<'a> {
        pub plan: &'a ReceivershipPlan,
        pub program: &'a Pubkey,
        pub taker: &'a Pubkey,
        pub exec: Result<Option<IntegrationExecutionAmount>, String>,
        pub unmodelled_mint: Option<&'static str>,
        pub sol_debit: Option<u64>,
    }

    fn kind_code(kind: Option<IntegrationKind>) -> &'static str {
        match kind {
            None => "PLAIN",
            Some(IntegrationKind::Kamino) => "KAMINO",
            Some(IntegrationKind::Drift) => "DRIFT",
            Some(IntegrationKind::JupLend) => "JUPLEND",
        }
    }

    /// `ceil(value · part / whole)`; `None` on a zero whole or an overflow.
    fn ceil_share(value: u64, part: u64, whole: u64) -> Option<u64> {
        if part == 0 {
            return Some(0);
        }
        if whole == 0 {
            return None;
        }
        let n = u128::from(value).checked_mul(u128::from(part))?;
        u64::try_from(n.div_ceil(u128::from(whole))).ok()
    }

    /// Whether `ixs` carries an instruction of an allowlisted swap program.
    fn swaps(ixs: &[solana_sdk::instruction::Instruction]) -> bool {
        let programs = [
            darkbot_project0::protocol::JUP_SWAP_PROGRAM_ID,
            darkbot_project0::protocol::TITAN_SWAP_PROGRAM_ID,
        ];
        ixs.iter()
            .any(|ix| programs.contains(&ix.program_id.to_string().as_str()))
    }

    /// The one swap instruction of `ixs` (the pinned Jupiter program), its terms from its bytes.
    fn swap_of(
        ixs: &[solana_sdk::instruction::Instruction],
        taker: &Pubkey,
        from: &Pubkey,
        to: &Pubkey,
    ) -> Result<crate::execution::route::SwapTerms, String> {
        let titan = darkbot_project0::protocol::TITAN_SWAP_PROGRAM_ID;
        let jup = darkbot_project0::protocol::JUP_SWAP_PROGRAM_ID;
        if ixs.iter().any(|ix| ix.program_id.to_string() == titan) {
            return Err("B_SWAP_PROGRAM_UNMODELLED".into());
        }
        let swaps: Vec<_> = ixs
            .iter()
            .filter(|ix| ix.program_id.to_string() == jup)
            .collect();
        match swaps.as_slice() {
            [swap] => crate::execution::route::swap_terms(swap, taker, from, to)
                .map_err(|e| format!("B_SWAP:{e}")),
            _ => Err(format!("B_SWAP_SHAPE:{}", swaps.len())),
        }
    }

    /// B of `plan` over `state`: the withdrawal's integration conversion recomputed, every mint of
    /// the plan checked for an unmodelled transfer (Token-2022 fee or hook, unknown mint).
    pub fn hold_b_of<S: darkbot_project0::health::AccountSource + ?Sized>(
        state: &S,
        program: &Pubkey,
        taker: &Pubkey,
        plan: &ReceivershipPlan,
        sol_debit: Option<u64>,
    ) -> BHold {
        let exec = match &plan.hold_facts {
            None => Ok(None),
            Some(f) => state
                .account(&f.asset_bank)
                .filter(|(owner, _, _)| owner == program)
                .ok_or_else(|| "B_ASSET_BANK_UNKNOWN".to_string())
                .and_then(|(_, _, data)| {
                    darkbot_project0::protocol::decode_bank(data).map_err(|e| e.to_string())
                })
                .and_then(|bank| {
                    darkbot_project0::integration::execution_amount_for_asset(
                        state,
                        &bank,
                        f.withdraw_bank_units,
                    )
                }),
        };
        let funding = plan.hold_facts.as_ref().and_then(|f| f.funding_mint);
        let unmodelled_mint = plan
            .assets
            .iter()
            .chain(funding.as_ref())
            .find_map(|m| match state.account(m) {
                None => Some("MINT_STATE_UNKNOWN"),
                Some((owner, lamports, data)) => {
                    darkbot_project0::sizing::mint_transfer_unmodeled(m, &owner, lamports, data)
                }
            });
        b_hold(BInputs {
            plan,
            program,
            taker,
            exec,
            unmodelled_mint,
            sol_debit,
        })
    }

    /// B of one plan (DEC-04 §4).
    pub fn b_hold(i: BInputs<'_>) -> BHold {
        match flows(i) {
            Ok(f) => BHold::Exact(f),
            Err(reason) => BHold::Unqualified(reason),
        }
    }

    fn flows(i: BInputs<'_>) -> Result<PlanFlows, String> {
        let p = i.plan;
        let f = p.hold_facts.as_ref().ok_or("B_HOLD_FACTS_MISSING")?;
        if p.input.funding == FundingClass::FlashFunded {
            return Err("B_FLASH_FUNDED_BLOCKED".into());
        }
        if let Some(reason) = i.unmodelled_mint {
            return Err(format!("B_MINT_UNMODELLED:{reason}"));
        }
        if f.asset_mint == f.liab_mint {
            return Err("B_SAME_MINT_UNMODELLED".into());
        }
        // A's unknown-price sentinel is no value B could share.
        if f.seized_hold_value == u64::MAX {
            return Err("B_SEIZURE_PRICE_UNKNOWN".into());
        }
        let sol = i.sol_debit.ok_or("B_SOL_DEBIT_UNKNOWN")?;
        let exec = i.exec.map_err(|e| format!("B_INTEGRATION:{e}"))?;
        // Mint/amount parity: the signed withdraw (the converted instruction amount of an
        // integration bank, never withdraw-all) and repay are the sizing's.
        let built =
            darkbot_project0::receivership::builders::built_amounts(&p.instructions, i.program)
                .map_err(|e| format!("B_PARITY:{e}"))?;
        let expected = exec.map_or(f.withdraw_bank_units, |e| e.instruction_amount);
        if built.withdraw != (expected, false) {
            return Err("B_PARITY_WITHDRAW".into());
        }
        if built.repay != (f.repay_amount, f.repay_all) {
            return Err("B_PARITY_REPAY".into());
        }
        // The collateral the withdrawal credits: the integration's underlying, else the bank units.
        let credited = exec.map_or(f.withdraw_bank_units, |e| e.underlying_out);
        if credited == 0 {
            return Err("B_CREDIT_ZERO".into());
        }
        let mut flows: BTreeMap<String, AssetFlow> = BTreeMap::new();
        flows.insert(
            "SOL".into(),
            AssetFlow {
                reserve: sol,
                residual_max: Some(0),
            },
        );
        // (collateral the swap spends, the liability's floor, the value the swap or buy spends
        // at the high price: what its output can be worth at most; `None`: no swap)
        let (swapped, liability_min, input_value) = match p.input.funding {
            FundingClass::ZeroPrefundSameTx => {
                if !p.prerequisite_txs.is_empty() {
                    return Err("B_ZERO_PREFUND_PREREQUISITES".into());
                }
                let t = swap_of(&p.instructions, i.taker, &f.asset_mint, &f.liab_mint)?;
                if t.in_amount > credited {
                    return Err("B_SWAP_INPUT_EXCEEDS_CREDIT".into());
                }
                let value = ceil_share(f.seized_hold_value, t.in_amount, credited)
                    .ok_or("B_VALUE_OVERFLOW")?;
                (t.in_amount, t.minimum_out, Some(value))
            }
            FundingClass::NoSwapInventory | FundingClass::PreheldInventory => {
                if !p.prerequisite_txs.is_empty() || swaps(&p.instructions) {
                    return Err("B_INVENTORY_PLAN_SWAPS".into());
                }
                (0, 0, None)
            }
            FundingClass::JitShortfall => {
                let funding = f.funding_mint.ok_or("B_JIT_FUNDING_UNKNOWN")?;
                let [buy] = p.prerequisite_txs.as_slice() else {
                    return Err(format!("B_JIT_SHAPE:{}", p.prerequisite_txs.len()));
                };
                if swaps(&p.instructions) {
                    return Err("B_JIT_RECEIVERSHIP_SWAPS".into());
                }
                let t = swap_of(buy, i.taker, &funding, &f.liab_mint)?;
                flows.insert(
                    funding.to_string(),
                    AssetFlow {
                        reserve: t.in_amount,
                        residual_max: Some(0),
                    },
                );
                // The buy spends the plan's capital: the funding at its conservative cost.
                (0, t.minimum_out, Some(p.input.required_capital_usd_micro))
            }
            FundingClass::FlashFunded => return Err("B_FLASH_FUNDED_BLOCKED".into()),
        };
        // The liability: the repay debits it, a swap or buy credits at least its floor; what it
        // credits beyond the repay has no bound in units — its value at most the input's high
        // value less the repay's low value.
        flows.insert(
            f.liab_mint.to_string(),
            AssetFlow {
                reserve: f.repay_amount.saturating_sub(liability_min),
                residual_max: if input_value.is_some() { None } else { Some(0) },
            },
        );
        // The collateral: credited by the withdrawal, spent by the swap.
        let residual = credited - swapped;
        flows.insert(
            f.asset_mint.to_string(),
            AssetFlow {
                reserve: 0,
                residual_max: Some(residual),
            },
        );
        // Its residual is its share of the seizure's conservative value.
        let collateral_value =
            ceil_share(f.seized_hold_value, residual, credited).ok_or("B_VALUE_OVERFLOW")?;
        let surplus_value = match input_value {
            None => 0,
            Some(input) => {
                let low = f.repay_low_value.ok_or("B_LIABILITY_PRICE_UNKNOWN")?;
                input.saturating_sub(low)
            }
        };
        Ok(PlanFlows {
            flows,
            inventory_usd_micro: collateral_value
                .checked_add(surplus_value)
                .ok_or("B_VALUE_OVERFLOW")?,
            kind: kind_code(exec.map(|e| e.kind)).into(),
        })
    }
}
#[cfg(feature = "crate_health")]
pub use compute::{b_hold, hold_b_of, BInputs};

/// One asset of a finalized attempt against a prediction: the predicted reserve and residual,
/// the finalized wallet delta, and whether the prediction underestimated it (a debit beyond the
/// reserve or an inflow beyond the residual).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowCheck {
    pub asset: String,
    pub reserve: u64,
    pub residual_max: Option<u64>,
    pub actual: i128,
    pub underestimated: bool,
}

/// Every asset of the prediction or of the finalized deltas, checked.
pub fn compare(
    predicted: &BTreeMap<String, AssetFlow>,
    actual: &BTreeMap<String, i128>,
) -> Vec<FlowCheck> {
    let assets: std::collections::BTreeSet<&String> =
        predicted.keys().chain(actual.keys()).collect();
    assets
        .into_iter()
        .map(|asset| {
            // An asset the prediction does not name: nothing held, nothing kept.
            let p = predicted.get(asset).copied().unwrap_or(AssetFlow {
                reserve: 0,
                residual_max: Some(0),
            });
            let d = actual.get(asset).copied().unwrap_or(0);
            FlowCheck {
                asset: asset.clone(),
                reserve: p.reserve,
                residual_max: p.residual_max,
                actual: d,
                underestimated: d < -i128::from(p.reserve)
                    || p.residual_max.is_some_and(|m| d > i128::from(m)),
            }
        })
        .collect()
}

/// A's prediction as flows: the wallet reservation as reserve, nothing kept (A holds the
/// whole seizure in the inventory limit instead).
pub fn a_flows(holds: &BTreeMap<String, u64>) -> BTreeMap<String, AssetFlow> {
    holds
        .iter()
        .map(|(asset, units)| {
            (
                asset.clone(),
                AssetFlow {
                    reserve: *units,
                    residual_max: Some(0),
                },
            )
        })
        .collect()
}

/// The class B's evidence is kept by: the attempt's funding class and the withdrawal's kind.
pub fn class_of(funding_class: &str, kind: &str) -> String {
    format!("{funding_class}/{kind}")
}

/// The B evidence of the landed, finalized attempts per class (`class_of`): exact predictions
/// of successful attempts, underestimations (of any landed attempt) and attempts without an
/// exact B. A failed attempt within its prediction exercised no swap or repay flow: no evidence.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BEvidence {
    pub exact: u64,
    pub underestimated: u64,
    pub unqualified: u64,
    pub per_kind: BTreeMap<String, (u64, u64)>,
}

/// One landed, finalized attempt as evidence: its funding class, whether it succeeded, its
/// finalized wallet deltas and the conservative mark of the inventory it left.
pub struct Finalized<'a> {
    pub funding_class: &'a str,
    pub success: bool,
    pub deltas: &'a BTreeMap<String, i128>,
    pub inventory_usd_micro: i128,
}

impl BEvidence {
    /// One landed, finalized attempt with its holds: B underestimated it when a flow left its
    /// prediction or the inventory it left is worth more than B's inventory hold.
    pub fn add(&mut self, hold: &HoldEvidence, done: &Finalized<'_>) {
        match &hold.b {
            BHold::Unqualified(_) => self.unqualified += 1,
            BHold::Exact(f) => {
                let under = compare(&f.flows, done.deltas)
                    .iter()
                    .any(|c| c.underestimated)
                    || done.inventory_usd_micro > i128::from(f.inventory_usd_micro);
                let class = self
                    .per_kind
                    .entry(class_of(done.funding_class, &f.kind))
                    .or_default();
                if under {
                    self.underestimated += 1;
                    class.1 += 1;
                } else if done.success {
                    self.exact += 1;
                    class.0 += 1;
                }
            }
        }
    }
    /// A class is qualified with N exact attempts and no underestimation at all.
    pub fn kind_qualified(&self, class: &str, n: u64) -> bool {
        self.underestimated == 0 && self.per_kind.get(class).is_some_and(|(ok, _)| *ok >= n)
    }
}

/// The effective mode of a request: ACTIVE only with N decided and N exact attempts without any
/// underestimation (else OBSERVE and why); OFF and OBSERVE as requested.
pub fn effective(requested: HoldMode, n: Option<u64>, e: &BEvidence) -> (HoldMode, &'static str) {
    match (requested, n) {
        (HoldMode::Active, None) => (HoldMode::Observe, B8_UNDECIDED),
        (HoldMode::Active, Some(_)) if e.underestimated > 0 => {
            (HoldMode::Observe, B_UNDERESTIMATED)
        }
        (HoldMode::Active, Some(n)) if e.exact < n => (HoldMode::Observe, B_EVIDENCE_SHORT),
        (mode, _) => (mode, ""),
    }
}

/// What the pipeline evaluates the holds with: the effective mode, the user's N and the evidence
/// (the owner's, refreshed with its FE-04 journal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoldGate {
    pub mode: HoldMode,
    pub n: Option<u64>,
    pub evidence: BEvidence,
}

impl HoldGate {
    /// OBSERVE without N or evidence: B is computed and recorded, A holds.
    pub const OBSERVE: HoldGate = HoldGate {
        mode: HoldMode::Observe,
        n: None,
        evidence: BEvidence {
            exact: 0,
            underestimated: 0,
            unqualified: 0,
            per_kind: BTreeMap::new(),
        },
    };
    /// The inventory hold of a plan of `funding_class` with A's value `a` and B `b`.
    pub fn inventory_hold(&self, a: u64, b: Option<&BHold>, funding_class: &str) -> u64 {
        inventory_hold(a, b, funding_class, self.mode, self.n, &self.evidence)
    }
}

/// The inventory hold Safety checks: B's for an exact plan of a qualified class (its funding
/// class and kind) in an effective ACTIVE, A's otherwise.
pub fn inventory_hold(
    a: u64,
    b: Option<&BHold>,
    funding_class: &str,
    mode: HoldMode,
    n: Option<u64>,
    e: &BEvidence,
) -> u64 {
    match (mode, b, n) {
        (HoldMode::Active, Some(BHold::Exact(f)), Some(n))
            if e.kind_qualified(&class_of(funding_class, &f.kind), n) =>
        {
            f.inventory_usd_micro
        }
        _ => a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flows(entries: &[(&str, u64, Option<u64>)]) -> BTreeMap<String, AssetFlow> {
        entries
            .iter()
            .map(|(a, reserve, residual_max)| {
                (
                    a.to_string(),
                    AssetFlow {
                        reserve: *reserve,
                        residual_max: *residual_max,
                    },
                )
            })
            .collect()
    }
    fn exact(kind: &str, f: BTreeMap<String, AssetFlow>) -> HoldEvidence {
        HoldEvidence {
            mode: HoldMode::Observe,
            a_inventory_usd_micro: 1_100,
            b: BHold::Exact(PlanFlows {
                flows: f,
                inventory_usd_micro: 100,
                kind: kind.into(),
            }),
            reserved_inventory_usd_micro: 1_100,
        }
    }
    fn done<'a>(
        funding_class: &'a str,
        success: bool,
        deltas: &'a BTreeMap<String, i128>,
        inventory: i128,
    ) -> Finalized<'a> {
        Finalized {
            funding_class,
            success,
            deltas,
            inventory_usd_micro: inventory,
        }
    }
    const ZP: &str = "ZERO_PREFUND_SAME_TX";

    /// DEC-04 (§5): a finalized attempt against a prediction — a debit within the reserve and an
    /// inflow within the residual hold (an unbounded residual: any inflow); anything beyond, or a
    /// flow of an unpredicted asset, is an underestimation.
    #[test]
    fn dec04_finalized_flows_against_the_prediction() {
        let predicted = flows(&[
            ("SOL", 10, Some(0)),
            ("MINT", 0, Some(100)),
            ("LIAB", 5, None),
        ]);
        let actual = |d: &[(&str, i128)]| d.iter().map(|(a, v)| (a.to_string(), *v)).collect();
        let under = |d: &[(&str, i128)]| {
            compare(&predicted, &actual(d))
                .into_iter()
                .filter(|c| c.underestimated)
                .map(|c| c.asset)
                .collect::<Vec<_>>()
        };
        assert!(under(&[("SOL", -10), ("MINT", 100), ("LIAB", 1_000_000)]).is_empty());
        assert!(under(&[("SOL", -3), ("LIAB", -5)]).is_empty());
        assert_eq!(
            under(&[("SOL", -11), ("MINT", 101), ("LIAB", -6)]),
            ["LIAB", "MINT", "SOL"]
        );
        assert_eq!(under(&[("OTHER", 1)]), ["OTHER"]);
        // A predicts the wallet reservation and keeps nothing in the wallet.
        assert_eq!(
            a_flows(&[("SOL".to_string(), 10)].into()),
            flows(&[("SOL", 10, Some(0))])
        );
    }

    /// DEC-04 (§6, review H1/M1/M4): evidence — only a successful attempt within its prediction
    /// is exact; a failed one within it is no evidence; an underestimation counts from any
    /// landed attempt, also an inventory left worth more than B's hold; the evidence is kept per
    /// funding class and kind.
    #[test]
    fn dec04_evidence_counts_successful_exact_attempts_per_class() {
        let plain = exact("PLAIN", flows(&[("SOL", 10, Some(0))]));
        let ok: BTreeMap<String, i128> = [("SOL".to_string(), -10)].into();
        let over: BTreeMap<String, i128> = [("SOL".to_string(), -11)].into();
        let mut e = BEvidence::default();
        e.add(&plain, &done(ZP, false, &ok, 0));
        assert_eq!(
            (e.exact, e.underestimated),
            (0, 0),
            "a lost race is no evidence"
        );
        e.add(&plain, &done(ZP, true, &ok, 100));
        e.add(&plain, &done("NO_SWAP_INVENTORY", true, &ok, 0));
        assert_eq!(e.per_kind[&class_of(ZP, "PLAIN")], (1, 0));
        assert_eq!(e.per_kind[&class_of("NO_SWAP_INVENTORY", "PLAIN")], (1, 0));
        assert!(e.kind_qualified(&class_of(ZP, "PLAIN"), 1));
        assert!(!e.kind_qualified(&class_of(ZP, "PLAIN"), 2));
        let mut worth = e.clone();
        worth.add(&plain, &done(ZP, true, &ok, 101));
        assert_eq!(
            worth.underestimated, 1,
            "an inventory worth more than B's hold"
        );
        let mut failed = e.clone();
        failed.add(&plain, &done(ZP, false, &over, 0));
        assert_eq!(
            failed.underestimated, 1,
            "a failed attempt can still be underestimated"
        );
        assert!(!failed.kind_qualified(&class_of("NO_SWAP_INVENTORY", "PLAIN"), 1));
        let mut unq = e.clone();
        unq.add(
            &HoldEvidence {
                b: BHold::Unqualified("B_SWAP_PROGRAM_UNMODELLED".into()),
                ..plain.clone()
            },
            &done(ZP, true, &ok, 0),
        );
        assert_eq!((unq.exact, unq.unqualified), (2, 1));
    }

    /// DEC-04 (§6): the gate — ACTIVE is effective only with N decided, N exact attempts and no
    /// underestimation at all; the inventory hold is B's only then and only for a funding class
    /// and kind with N exact attempts of its own; otherwise A.
    #[test]
    fn dec04_active_is_gated_by_n_and_the_evidence() {
        let mut e = BEvidence::default();
        let plain = exact("PLAIN", flows(&[("SOL", 10, Some(0))]));
        let ok: BTreeMap<String, i128> = [("SOL".to_string(), -10)].into();
        assert_eq!(
            effective(HoldMode::Active, None, &e),
            (HoldMode::Observe, B8_UNDECIDED)
        );
        assert_eq!(
            effective(HoldMode::Active, Some(2), &e),
            (HoldMode::Observe, B_EVIDENCE_SHORT)
        );
        e.add(&plain, &done(ZP, true, &ok, 0));
        e.add(&plain, &done(ZP, true, &ok, 0));
        assert_eq!(
            effective(HoldMode::Active, Some(2), &e),
            (HoldMode::Active, "")
        );
        for m in [HoldMode::Off, HoldMode::Observe] {
            assert_eq!(effective(m, None, &e), (m, ""));
        }
        let b = plain.b.clone();
        let hold =
            |b: Option<&BHold>, class: &str, mode, n| inventory_hold(1_100, b, class, mode, n, &e);
        assert_eq!(hold(Some(&b), ZP, HoldMode::Active, Some(2)), 100);
        assert_eq!(hold(Some(&b), ZP, HoldMode::Observe, Some(2)), 1_100);
        assert_eq!(hold(Some(&b), ZP, HoldMode::Active, None), 1_100);
        assert_eq!(hold(None, ZP, HoldMode::Active, Some(2)), 1_100);
        // Another funding class or kind without its own evidence stays A.
        assert_eq!(
            hold(Some(&b), "NO_SWAP_INVENTORY", HoldMode::Active, Some(2)),
            1_100
        );
        let drift = exact("DRIFT", flows(&[("SOL", 10, Some(0))])).b;
        assert_eq!(hold(Some(&drift), ZP, HoldMode::Active, Some(2)), 1_100);
        let unq = BHold::Unqualified("B_SWAP_PROGRAM_UNMODELLED".into());
        assert_eq!(hold(Some(&unq), ZP, HoldMode::Active, Some(2)), 1_100);
        // One underestimation blocks ACTIVE and every class.
        let mut under = e.clone();
        under.add(
            &plain,
            &done(ZP, true, &[("SOL".to_string(), -11)].into(), 0),
        );
        assert_eq!(
            effective(HoldMode::Active, Some(2), &under),
            (HoldMode::Observe, B_UNDERESTIMATED)
        );
        assert_eq!(
            inventory_hold(1_100, Some(&b), ZP, HoldMode::Active, Some(2), &under),
            1_100
        );
    }
}

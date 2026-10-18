//! P4A: Deterministische Profit-Engine.
//!
//! - Realisierbarer Wert, nicht Oracle-Wert (Aufrufer liefert `gross_seized_realizable`).
//! - Alle Kosten explizit; unbekannte Kosten sind `None` und machen das Ergebnis
//!   `Unknown` — nie 0.
//! - Premium-Kurve (Release 0.1.12, BELEGT): erste Stunde Basis = max(FeeState.liquidation_max_fee, 5 %),
//!   danach linear bis 100 % nach 7 Tagen; kein Wachstum bei Bad Debt / EMODE_BOOSTED.
//!   Zusätzlich Deckel durch Receivership-Health-Regel (P3 liefert `premium_cap_rule`).
//! - Kein p(win), kein Warten: Handlungsregel V1 = net > 0 ∧ valid.
//!
//! Beträge in Lamports bzw. in USD-Micro (1e-6) als u64/i128 — kein f64.
//! Prämien in Basispunkten (bps, 10 000 = 100 %).

pub const BPS: u64 = 10_000;
pub const PREMIUM_BASE_BPS: u64 = 500; // 5 %
pub const PREMIUM_MAX_BPS: u64 = 10_000; // 100 %
pub const PREMIUM_GRACE_S: u64 = 3_600; // 1 h
/// Rampe nach der Grace-Stunde, so dass 100 % genau 7 Tage **nach Tagging** erreicht sind.
/// VERIFY gegen `liquidate_start.rs` (Bezugspunkt der Woche: Tagging vs. Ende der Grace-Stunde).
pub const PREMIUM_RAMP_S: u64 = 7 * 24 * 3_600 - PREMIUM_GRACE_S;

/// B5: `FeeState.liquidation_max_fee` über 100 % ist kein gültiger Quellzustand. Jede Prämien-
/// und Seize-Rechnung damit ist UNKNOWN (`None`), nie still auf 100 % normalisiert.
pub fn fee_state_valid(fee_state_max_bps: u64) -> bool {
    fee_state_max_bps <= BPS
}

/// Maximale Prämie zum Zeitpunkt `now` für einen Account, der bei `tagged_at` getaggt wurde.
/// `fee_state_max_bps` = FeeState.liquidation_max_fee (P1A). Nicht getaggt (None) → Basis.
/// `growth_blocked` = Bad Debt oder EMODE_BOOSTED → Basis. `None` bei ungültigem FeeState (B5).
pub fn premium_bps(
    now_unix_s: u64,
    tagged_at: Option<u64>,
    fee_state_max_bps: u64,
    growth_blocked: bool,
) -> Option<u64> {
    if !fee_state_valid(fee_state_max_bps) {
        return None;
    }
    let base = fee_state_max_bps.max(PREMIUM_BASE_BPS);
    let Some(t0) = tagged_at else {
        return Some(base);
    };
    if growth_blocked || now_unix_s <= t0.saturating_add(PREMIUM_GRACE_S) {
        return Some(base);
    }
    let elapsed = now_unix_s - t0 - PREMIUM_GRACE_S;
    if elapsed >= PREMIUM_RAMP_S {
        return Some(PREMIUM_MAX_BPS);
    }
    // linear von base auf 100 % (base ≤ 100 % durch fee_state_valid)
    let span = PREMIUM_MAX_BPS - base;
    Some(base + (span as u128 * elapsed as u128 / PREMIUM_RAMP_S as u128) as u64)
}

/// Frühester Zeitpunkt, an dem premium_bps ≥ `needed_bps` (für den Tag-Scheduler).
/// None, wenn nie erreichbar (Wachstum blockiert und Basis reicht nicht, Bedarf über 100 %,
/// Zeitüberlauf) oder der FeeState ungültig ist (B5).
pub fn time_for_premium(
    tagged_at: u64,
    fee_state_max_bps: u64,
    needed_bps: u64,
    growth_blocked: bool,
) -> Option<u64> {
    if !fee_state_valid(fee_state_max_bps) {
        return None;
    }
    let base = fee_state_max_bps.max(PREMIUM_BASE_BPS);
    if needed_bps <= base {
        return Some(tagged_at);
    }
    if growth_blocked || needed_bps > PREMIUM_MAX_BPS {
        return None;
    }
    // base < needed ≤ 100 %: span > 0. elapsed = ceil((needed-base) * RAMP / span)
    let span = PREMIUM_MAX_BPS - base;
    let num = (needed_bps - base) as u128 * PREMIUM_RAMP_S as u128;
    let elapsed = num.div_ceil(span as u128) as u64;
    tagged_at.checked_add(PREMIUM_GRACE_S)?.checked_add(elapsed)
}

/// Kandidatenwerte des [`t_profitable`]-Solvers (unveränderte Bedeutung der früheren Einzelparameter).
#[derive(Clone, Copy, Debug)]
pub struct ProfitInputs {
    pub fee_state_max_bps: u64,
    pub growth_blocked: bool,
    pub premium_cap_bps: u64,
    pub repay_value: u64,
    pub realizable_bps: u64,
    pub fixed_costs: u64,
    pub min_profit: i128,
}

/// `t_profitable`-Solver (Stufe-1-Kern): frühestes t mit net(t) > min_profit, wobei
/// gross_realizable(t) = repay_value × (BPS + premium_cap(t)) / BPS × realizable_bps / BPS.
/// `premium_cap_bps` deckelt die Kurve durch die Receivership-Health-Regel (P3).
/// Rückgabe None, wenn selbst bei der Kappe kein Profit entsteht → Kandidat verwerfen.
pub fn t_profitable(now_unix_s: u64, tagged_at: Option<u64>, inputs: &ProfitInputs) -> Option<u64> {
    let ProfitInputs {
        fee_state_max_bps,
        growth_blocked,
        premium_cap_bps,
        repay_value,
        realizable_bps,
        fixed_costs,
        min_profit,
    } = *inputs;
    // net(p) = repay × (BPS + p)/BPS × r/BPS − repay − costs  >  min
    // ⇔ p > ((min + costs + repay) × BPS² / (repay × r)) − BPS
    if repay_value == 0 || realizable_bps == 0 || !fee_state_valid(fee_state_max_bps) {
        return None;
    }
    // B5: checked; a non-positive requirement (negative min_profit) is met by any premium.
    let required = min_profit
        .checked_add(fixed_costs as i128)?
        .checked_add(repay_value as i128)?;
    if required <= 0 {
        return Some(now_unix_s);
    }
    let num = (required as u128).checked_mul(BPS as u128 * BPS as u128)?;
    let den = repay_value as u128 * realizable_bps as u128;
    let p_needed = num.checked_add(den)? / den; // ceil + 1 für striktes ">"
    if p_needed <= BPS as u128 {
        return Some(now_unix_s); // schon ohne Prämie profitabel (realizable > repay) – sofort
    }
    // B5: a need beyond u64 bps is unreachable, never wrapped into a small premium.
    let needed_bps = u64::try_from(p_needed - BPS as u128).ok()?;
    let cap = premium_cap_bps.min(PREMIUM_MAX_BPS);
    if needed_bps > cap {
        return None;
    }
    match tagged_at {
        None => {
            let base = fee_state_max_bps.max(PREMIUM_BASE_BPS);
            if needed_bps <= base {
                Some(now_unix_s)
            } else {
                None
            } // nicht getaggt: nur Basis verfügbar
        }
        Some(t0) => time_for_premium(t0, fee_state_max_bps, needed_bps, growth_blocked)
            .map(|t| t.max(now_unix_s)),
    }
}

/// Seize-Grenze aus `liquidate_end.rs` (BELEGT, 0.1.11): in **Equity-(EMA-)Werten**
/// `seized ≤ repaid × (1 + max(FeeState.liquidation_max_fee, 0.05))`; außerdem Post-Health
/// (Maintenance, Cached) ≥ Pre-Health. Bei Equity-Assets < 5 USD entfällt die Grenze (Closeout).
/// Rückgabe: maximal beschlagnahmbarer Equity-Wert für einen gegebenen Repay-Wert (gleiche Einheit);
/// `None` bei ungültigem FeeState oder Überlauf (B5).
pub fn max_seizable_for_repay(
    repaid_equity: u128,
    fee_state_max_bps: u64,
    closeout: bool,
) -> Option<u128> {
    if !fee_state_valid(fee_state_max_bps) {
        return None;
    }
    if closeout {
        return Some(u128::MAX);
    }
    let fee = fee_state_max_bps.max(crate::protocol::LIQUIDATION_BONUS_FEE_MINIMUM_BPS) as u128;
    Some(repaid_equity.checked_mul(BPS as u128 + fee)? / BPS as u128)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Path {
    Rpc,
    JitoSingle,
    JitoBundle,
}

/// Pfadabhängige Kosten eines **fehlgeschlagenen** Versuchs (Ledger §4).
/// Monetary unit: USD-micro, after explicit conversion; never raw lamports.
/// `None` = ungemessen. Ein Kandidat mit `None` in seiner Pfadzelle ist nicht bewertbar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FailedCostMatrix {
    pub rpc_not_landed: Option<u64>,
    pub rpc_landed_revert: Option<u64>,
    pub jito_single_not_landed: Option<u64>,
    pub jito_single_landed_revert: Option<u64>,
    pub jito_bundle_not_landed: Option<u64>,
    pub jito_bundle_failed: Option<u64>,
}

impl FailedCostMatrix {
    /// Erwartete Failed-Cost für einen Versuch auf `path` ohne Wahrscheinlichkeiten:
    /// V1 nimmt konservativ das Maximum der bekannten Fehlerfälle des Pfads.
    pub fn conservative(&self, path: Path) -> Option<u64> {
        let (a, b) = match path {
            Path::Rpc => (self.rpc_not_landed, self.rpc_landed_revert),
            Path::JitoSingle => (self.jito_single_not_landed, self.jito_single_landed_revert),
            Path::JitoBundle => (self.jito_bundle_not_landed, self.jito_bundle_failed),
        };
        match (a, b) {
            (Some(x), Some(y)) => Some(x.max(y)),
            _ => None,
        }
    }
}

/// Candidate-specific scenario pricing, not a historical landing classification.
/// RPC genuinely not landed pays no network fee; a landed execution revert pays
/// the exact message fee. A null quote cannot qualify either prospective cell.
/// Jito lifecycle/tip cells are deliberately untouched. Amounts are USD-micro.
pub fn rpc_message_failure_costs(message_fee_usd_micro: Option<u64>) -> FailedCostMatrix {
    match message_fee_usd_micro {
        Some(fee) => FailedCostMatrix {
            rpc_not_landed: Some(0),
            rpc_landed_revert: Some(fee),
            ..Default::default()
        },
        None => FailedCostMatrix::default(),
    }
}

/// Eingaben pro Kandidat. **Jedes Geldfeld ist explizit USD-Micro.**
/// Lamports/native token units dürfen hier niemals direkt eingetragen werden; die Umrechnung
/// muss vorher über einen expliziten, ausführbaren Preis-/Kostenprovider erfolgt sein. [FIX-61]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateInput {
    /// Realisierbarer Wert des beschlagnahmten Collaterals über den gewählten Pfad.
    /// Executable realizable value. None means market realization is not proven for this path.
    /// Never substitute oracle/protocol equity or zero for an unknown market value. [FIX-97]
    pub gross_seized_realizable_usd_micro: Option<u64>,
    pub repay_usd_micro: u64,
    pub protocol_fees_usd_micro: u64,
    pub flat_sol_fee_usd_micro: u64,
    pub swap_fees_and_impact_usd_micro: u64,
    /// Exact-message total network fee (base + requested priority), once per success.
    pub transaction_fee_usd_micro: Option<u64>,
    pub jito_tip_usd_micro: u64,
    pub rent_and_token_usd_micro: Option<u64>,
    pub required_capital_usd_micro: u64,
    pub path: Path,
    pub funding: crate::funding::FundingClass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evaluation {
    /// Netto-Profit deterministisch bekannt (kann negativ sein).
    Known {
        net: i128,
        profit_per_capital_bps: i128,
    },
    /// Eine Kostenzelle ist ungemessen → nicht handelbar.
    Unknown,
}

pub fn evaluate(c: &CandidateInput, failed: &FailedCostMatrix) -> Evaluation {
    let Some(gross_seized_realizable_usd_micro) = c.gross_seized_realizable_usd_micro else {
        return Evaluation::Unknown;
    };
    let Some(failed_cost) = failed.conservative(c.path) else {
        return Evaluation::Unknown;
    };
    let (Some(transaction_fee), Some(rent_and_token)) =
        (c.transaction_fee_usd_micro, c.rent_and_token_usd_micro)
    else {
        return Evaluation::Unknown;
    };
    let costs: u128 = c.repay_usd_micro as u128
        + c.protocol_fees_usd_micro as u128
        + c.flat_sol_fee_usd_micro as u128
        + c.swap_fees_and_impact_usd_micro as u128
        + transaction_fee as u128
        + c.jito_tip_usd_micro as u128
        + rent_and_token as u128
        + failed_cost as u128;
    let net = gross_seized_realizable_usd_micro as i128 - costs as i128;
    let ppc = if c.required_capital_usd_micro == 0 {
        i128::MAX
    } else {
        net * BPS as i128 / c.required_capital_usd_micro as i128
    };
    Evaluation::Known {
        net,
        profit_per_capital_bps: ppc,
    }
}

/// Vergleich zweier bekannter Economics-Ergebnisse. Höherer deterministischer Netto-Profit
/// gewinnt; bei Gleichstand höhere Kapital-Effizienz. Das Funding-Enum selbst ist **kein**
/// Ranking-Signal.
pub fn compare_evaluations(a: Evaluation, b: Evaluation) -> std::cmp::Ordering {
    match (a, b) {
        (
            Evaluation::Known {
                net: na,
                profit_per_capital_bps: pa,
            },
            Evaluation::Known {
                net: nb,
                profit_per_capital_bps: pb,
            },
        ) => na.cmp(&nb).then(pa.cmp(&pb)),
        (Evaluation::Known { .. }, Evaluation::Unknown) => std::cmp::Ordering::Greater,
        (Evaluation::Unknown, Evaluation::Known { .. }) => std::cmp::Ordering::Less,
        (Evaluation::Unknown, Evaluation::Unknown) => std::cmp::Ordering::Equal,
    }
}

/// Wählt aus bereits technisch machbaren Kandidaten die beste deterministische Variante.
/// Unknown oder `net <= min_profit` werden verworfen. TX-/State-/Safety-Feasibility muss der
/// Aufrufer vorher prüfen; diese Funktion ersetzt bewusst **keine** Execution-Gates.
pub fn choose_best_candidate(
    candidates: &[CandidateInput],
    failed: &FailedCostMatrix,
    min_profit: i128,
) -> Option<(CandidateInput, Evaluation)> {
    candidates
        .iter()
        .copied()
        .filter_map(|input| {
            let evaluation = evaluate(&input, failed);
            match evaluation {
                Evaluation::Known { net, .. } if net > min_profit => Some((input, evaluation)),
                _ => None,
            }
        })
        .max_by(|(_, a), (_, b)| compare_evaluations(*a, *b))
}

/// Handlungsregel V1: net > min_profit ∧ Ausführung/State/Safety gültig (Flags vom Aufrufer).
pub fn actionable(
    e: Evaluation,
    min_profit: i128,
    execution_valid: bool,
    state_valid: bool,
    safety_valid: bool,
) -> bool {
    match e {
        Evaluation::Known { net, .. } => {
            net > min_profit && execution_valid && state_valid && safety_valid
        }
        Evaluation::Unknown => false,
    }
}

/// Ranking: zuerst höchster Netto, bei Gleichstand höchster Profit/Kapital. Unknown wird ausgelassen.
pub fn rank(
    mut items: Vec<(solana_sdk::pubkey::Pubkey, Evaluation)>,
) -> Vec<(solana_sdk::pubkey::Pubkey, i128)> {
    items.retain(|(_, e)| matches!(e, Evaluation::Known { .. }));
    items.sort_by(|(_, a), (_, b)| compare_evaluations(*b, *a));
    items
        .into_iter()
        .map(|(k, e)| match e {
            Evaluation::Known { net, .. } => (k, net),
            Evaluation::Unknown => (k, i128::MIN),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;

    #[test]
    fn exact_rpc_fee_qualifies_only_rpc_scenarios() {
        assert_eq!(
            rpc_message_failure_costs(Some(73)).conservative(Path::Rpc),
            Some(73)
        );
        assert_eq!(
            rpc_message_failure_costs(None).conservative(Path::Rpc),
            None
        );
        assert_eq!(
            rpc_message_failure_costs(Some(73)).conservative(Path::JitoBundle),
            None
        );
    }

    #[test]
    fn premium_curve_matches_release_notes() {
        let t0 = 1_000_000;
        assert_eq!(premium_bps(t0, Some(t0), 0, false), Some(500), "Basis 5 %");
        assert_eq!(
            premium_bps(t0, Some(t0), 800, false),
            Some(800),
            "FeeState-Max gewinnt"
        );
        assert_eq!(
            premium_bps(t0 + 3_599, Some(t0), 0, false),
            Some(500),
            "Grace-Stunde"
        );
        let half = t0 + PREMIUM_GRACE_S + PREMIUM_RAMP_S / 2;
        let p = premium_bps(half, Some(t0), 0, false).unwrap();
        assert!(
            (5_249..=5_251).contains(&p),
            "Mitte der Rampe ≈ 52,5 %: {}",
            p
        );
        assert_eq!(
            premium_bps(t0 + 7 * 24 * 3_600, Some(t0), 0, false),
            Some(10_000),
            "100 % genau 7 d nach Tagging"
        );
        assert!(premium_bps(t0 + 7 * 24 * 3_600 - 1, Some(t0), 0, false).unwrap() < 10_000);
        assert_eq!(
            premium_bps(t0 + 10 * PREMIUM_RAMP_S, Some(t0), 0, true),
            Some(500),
            "Bad Debt/EMODE: kein Wachstum"
        );
        assert_eq!(premium_bps(t0, None, 0, false), Some(500), "nicht getaggt");
    }

    #[test]
    fn time_for_premium_inverts_curve() {
        let t0 = 5_000;
        let t = time_for_premium(t0, 0, 2_000, false).unwrap(); // 20 %
        let p = premium_bps(t, Some(t0), 0, false).unwrap();
        assert!((2_000..2_010).contains(&p), "{}", p);
        assert!(premium_bps(t - 1, Some(t0), 0, false).unwrap() < 2_000);
        assert_eq!(
            time_for_premium(t0, 0, 400, false),
            Some(t0),
            "unter Basis sofort"
        );
        assert_eq!(time_for_premium(t0, 0, 3_000, true), None, "blockiert: nie");
    }

    /// B5: FeeState above 100 %, zero span, time overflow, negative MinProfit and multiplication
    /// overflow never panic or wrap; invalid inputs are UNKNOWN (None), valid ones keep their
    /// values.
    #[test]
    fn invalid_fee_state_and_extreme_inputs_fail_closed_without_panic() {
        let t0 = 1_000_000;
        let late = t0 + PREMIUM_GRACE_S + PREMIUM_RAMP_S / 2;
        for fee in [0u64, 500, 9_999, 10_000] {
            assert!(premium_bps(late, Some(t0), fee, false).unwrap() <= PREMIUM_MAX_BPS);
            assert!(max_seizable_for_repay(10_000, fee, false).is_some());
        }
        assert_eq!(premium_bps(late, Some(t0), 10_000, false), Some(10_000));
        // 100 % already at base: any need above it is unreachable, not a division by zero.
        assert_eq!(time_for_premium(t0, 10_000, 10_000, false), Some(t0));
        assert_eq!(time_for_premium(t0, 10_000, 20_000, false), None);
        assert_eq!(time_for_premium(t0, 0, 10_001, false), None);
        for fee in [10_001u64, u64::MAX] {
            for now in [t0, late] {
                assert_eq!(premium_bps(now, Some(t0), fee, false), None, "{fee}");
                assert_eq!(premium_bps(now, None, fee, true), None, "{fee}");
            }
            assert_eq!(time_for_premium(t0, fee, 20_000, false), None);
            assert_eq!(max_seizable_for_repay(10_000, fee, false), None);
            assert_eq!(
                t_profitable(
                    t0,
                    Some(t0),
                    &inputs(fee, false, 10_000, 1_000, 9_700, 20, 0)
                ),
                None
            );
        }
        // Time overflow near u64::MAX and seize overflow near u128::MAX are UNKNOWN.
        assert_eq!(time_for_premium(u64::MAX - 10, 0, 2_000, false), None);
        assert_eq!(max_seizable_for_repay(u128::MAX, 0, false), None);
        // Negative MinProfit: requirement met by any premium, no u128 wrap.
        assert_eq!(
            t_profitable(
                t0,
                None,
                &inputs(0, false, 10_000, 1_000, 9_700, 20, -10_000)
            ),
            Some(t0)
        );
        assert_eq!(
            t_profitable(
                t0,
                None,
                &inputs(0, false, 10_000, 1_000, 9_700, 20, i128::MIN)
            ),
            Some(t0)
        );
        assert_eq!(
            t_profitable(t0, None, &inputs(0, false, 10_000, 1, 1, 0, i128::MAX)),
            None
        );
        // A need beyond u64 bps must not wrap into a small, reachable premium (review B5 rest).
        assert_eq!(
            t_profitable(
                t0,
                Some(t0),
                &inputs(0, false, 10_000, 1, 10_000, 0, 1_844_674_407_370_956)
            ),
            None
        );
    }

    #[test]
    fn seize_bound_from_source() {
        assert_eq!(
            max_seizable_for_repay(10_000, 0, false),
            Some(10_500),
            "Minimum 5 %"
        );
        assert_eq!(
            max_seizable_for_repay(10_000, 800, false),
            Some(10_800),
            "FeeState 8 % gewinnt"
        );
        assert_eq!(
            max_seizable_for_repay(10_000, 0, true),
            Some(u128::MAX),
            "Closeout < 5 USD"
        );
    }

    fn inputs(
        fee: u64,
        blocked: bool,
        cap: u64,
        repay: u64,
        real: u64,
        costs: u64,
        min: i128,
    ) -> ProfitInputs {
        ProfitInputs {
            fee_state_max_bps: fee,
            growth_blocked: blocked,
            premium_cap_bps: cap,
            repay_value: repay,
            realizable_bps: real,
            fixed_costs: costs,
            min_profit: min,
        }
    }

    #[test]
    fn t_profitable_solver() {
        let t0 = 10_000;
        // repay 1000, realisierbar 97 %, Kosten 20, min 0: braucht p mit 1000*(1+p)*0.97 > 1020 → p > 5,15 % → ~516 bps
        let t = t_profitable(t0, Some(t0), &inputs(0, false, 10_000, 1_000, 9_700, 20, 0)).unwrap();
        let p = premium_bps(t, Some(t0), 0, false).unwrap();
        assert!((516..=530).contains(&p), "p={}", p);
        assert!(t > t0 + PREMIUM_GRACE_S, "liegt nach der Grace-Stunde");
        // Kappe durch Receivership-Regel unter Bedarf → None
        assert_eq!(
            t_profitable(t0, Some(t0), &inputs(0, false, 500, 1_000, 9_700, 20, 0)),
            None
        );
        // nicht getaggt, Basis reicht nicht → None
        assert_eq!(
            t_profitable(t0, None, &inputs(0, false, 10_000, 1_000, 9_700, 20, 0)),
            None
        );
        // sofort profitabel
        assert_eq!(
            t_profitable(t0, None, &inputs(0, false, 10_000, 1_000, 10_000, 0, 0)),
            Some(t0)
        );
    }

    #[test]
    fn unknown_realizable_value_is_unknown() {
        let c = CandidateInput {
            gross_seized_realizable_usd_micro: None,
            repay_usd_micro: 900,
            protocol_fees_usd_micro: 0,
            flat_sol_fee_usd_micro: 0,
            swap_fees_and_impact_usd_micro: 0,
            transaction_fee_usd_micro: Some(0),
            jito_tip_usd_micro: 0,
            rent_and_token_usd_micro: Some(0),
            required_capital_usd_micro: 900,
            path: Path::Rpc,
            funding: crate::funding::FundingClass::NoSwapInventory,
        };
        let failed = FailedCostMatrix {
            rpc_not_landed: Some(0),
            rpc_landed_revert: Some(0),
            ..Default::default()
        };
        assert_eq!(evaluate(&c, &failed), Evaluation::Unknown);
    }

    #[test]
    fn unknown_cost_is_not_zero() {
        let c = CandidateInput {
            gross_seized_realizable_usd_micro: Some(1_000),
            repay_usd_micro: 900,
            protocol_fees_usd_micro: 0,
            flat_sol_fee_usd_micro: 0,
            swap_fees_and_impact_usd_micro: 0,
            transaction_fee_usd_micro: Some(0),
            jito_tip_usd_micro: 0,
            rent_and_token_usd_micro: Some(0),
            required_capital_usd_micro: 900,
            path: Path::JitoBundle,
            funding: crate::funding::FundingClass::NoSwapInventory,
        };
        assert_eq!(
            evaluate(&c, &FailedCostMatrix::default()),
            Evaluation::Unknown
        );
        let m = FailedCostMatrix {
            jito_bundle_not_landed: Some(0),
            jito_bundle_failed: Some(0),
            ..Default::default()
        };
        match evaluate(&c, &m) {
            Evaluation::Known {
                net,
                profit_per_capital_bps,
            } => {
                assert_eq!(net, 100);
                assert_eq!(profit_per_capital_bps, 100 * 10_000 / 900);
            }
            _ => panic!(),
        }
        assert_eq!(
            evaluate(
                &CandidateInput {
                    transaction_fee_usd_micro: None,
                    ..c
                },
                &m
            ),
            Evaluation::Unknown
        );
        assert_eq!(
            evaluate(
                &CandidateInput {
                    rent_and_token_usd_micro: None,
                    ..c
                },
                &m
            ),
            Evaluation::Unknown
        );
        assert_eq!(
            evaluate(
                &CandidateInput {
                    transaction_fee_usd_micro: Some(23),
                    ..c
                },
                &m
            ),
            Evaluation::Known {
                net: 77,
                profit_per_capital_bps: 77 * 10_000 / 900
            }
        );
        assert!(actionable(evaluate(&c, &m), 0, true, true, true));
        assert!(!actionable(evaluate(&c, &m), 100, true, true, true));
        assert!(!actionable(evaluate(&c, &m), 0, true, false, true));
    }

    #[test]
    fn funding_class_is_not_a_hardcoded_preference() {
        let base = CandidateInput {
            gross_seized_realizable_usd_micro: Some(1_100),
            repay_usd_micro: 1_000,
            protocol_fees_usd_micro: 0,
            flat_sol_fee_usd_micro: 0,
            swap_fees_and_impact_usd_micro: 0,
            transaction_fee_usd_micro: Some(0),
            jito_tip_usd_micro: 0,
            rent_and_token_usd_micro: Some(0),
            required_capital_usd_micro: 0,
            path: Path::Rpc,
            funding: crate::funding::FundingClass::ZeroPrefundSameTx,
        };
        let zero = CandidateInput {
            swap_fees_and_impact_usd_micro: 60,
            ..base
        }; // net 40
        let no_swap = CandidateInput {
            funding: crate::funding::FundingClass::NoSwapInventory,
            required_capital_usd_micro: 1_000,
            swap_fees_and_impact_usd_micro: 0,
            ..base
        }; // net 100
        let failed = FailedCostMatrix {
            rpc_not_landed: Some(0),
            rpc_landed_revert: Some(0),
            ..Default::default()
        };
        let (chosen, eval) = choose_best_candidate(&[zero, no_swap], &failed, 0).unwrap();
        assert_eq!(
            chosen.funding,
            crate::funding::FundingClass::NoSwapInventory
        );
        assert!(matches!(eval, Evaluation::Known { net: 100, .. }));
    }

    #[test]
    fn equal_net_prefers_capital_efficiency() {
        let zero = CandidateInput {
            gross_seized_realizable_usd_micro: Some(1_100),
            repay_usd_micro: 1_000,
            protocol_fees_usd_micro: 0,
            flat_sol_fee_usd_micro: 0,
            swap_fees_and_impact_usd_micro: 0,
            transaction_fee_usd_micro: Some(0),
            jito_tip_usd_micro: 0,
            rent_and_token_usd_micro: Some(0),
            required_capital_usd_micro: 0,
            path: Path::Rpc,
            funding: crate::funding::FundingClass::ZeroPrefundSameTx,
        };
        let preheld = CandidateInput {
            required_capital_usd_micro: 1_000,
            funding: crate::funding::FundingClass::PreheldInventory,
            ..zero
        };
        let failed = FailedCostMatrix {
            rpc_not_landed: Some(0),
            rpc_landed_revert: Some(0),
            ..Default::default()
        };
        let (chosen, _) = choose_best_candidate(&[preheld, zero], &failed, 0).unwrap();
        assert_eq!(
            chosen.funding,
            crate::funding::FundingClass::ZeroPrefundSameTx
        );
    }

    #[test]
    fn ranking_by_net_then_capital_efficiency() {
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        let u = Pubkey::new_unique();
        let r = rank(vec![
            (
                a,
                Evaluation::Known {
                    net: 50,
                    profit_per_capital_bps: 10,
                },
            ),
            (u, Evaluation::Unknown),
            (
                b,
                Evaluation::Known {
                    net: 50,
                    profit_per_capital_bps: 20,
                },
            ),
        ]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].0, b);
        assert_eq!(r[1].0, a);
    }
}

// Native backtest/optimizer: the production admission over typed rows (exact amounts, no
// look-ahead, deterministic search). End-to-end runs over `.p0j` evidence: report_tests.rs.
use super::*;

const MICRO: i128 = 1_000_000;

/// A verified liquidation joined with one evaluation: `net` (USD-micro) executable when `Some`,
/// signed before the winner's slot when `early`, needing `budget` (USD-micro).
fn row(slot: u64, net: Option<i128>, early: bool, budget: u64, funding: &str) -> BacktestRow {
    BacktestRow {
        signature: format!("sig-{slot}"),
        slot,
        transaction_index: 0,
        event_index: 0,
        liquidation_type: "RECEIVERSHIP".into(),
        target: format!("target-{slot}"),
        liquidator: "winner".into(),
        block_time: Some(slot as i64),
        protocol_fee_lamports: Some(5_000),
        transaction_fee_lamports: None,
        priority_fee_lamports: None,
        facts: JoinedFacts {
            identity_status: "TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT".into(),
            race_class: if early {
                "PRE_WINNER_SLOT_PROVEN"
            } else {
                "SAME_SLOT_UNKNOWN"
            }
            .into(),
            local_observed_race_margin_ns: None,
            local_slot_race_margin_ns: None,
            loss_reason: "NO_PROVEN_TECHNICAL_LOSS".into(),
            loss_provenance: "EXACT_PLAN_MATCH".into(),
            signed: Some(true),
            simulation_result: Some("PASS".into()),
            economics_known: Some(net.is_some()),
            executable_net_usd_micro: net,
            required_working_capital_usd_micro: Some(budget),
            required_total_budget_usd_micro: Some(budget),
            record_digest: Some([slot as u8; 32]),
        },
        funding: Some(funding.into()),
        profit_per_capital_bps: net.map(|n| n * 10_000 / i128::from(budget)),
    }
}

fn unmatched(slot: u64) -> BacktestRow {
    let mut r = row(slot, None, false, 1, "NO_SWAP_INVENTORY");
    r.facts.record_digest = None;
    r.facts.loss_reason = "NOT_DETECTED".into();
    r
}

fn params(min_usd: i128, capital_usd: &[u64], funding: &[&str]) -> Params {
    Params {
        min_profit_usd_micro: min_usd * MICRO,
        capital_usd_micro: capital_usd.iter().map(|c| c * 1_000_000).collect(),
        funding: funding.iter().map(|f| f.to_string()).collect(),
    }
}

/// Every admission path of the production rule: only an exact-match, executable, known,
/// pre-winner-slot evaluation above the minimum (strictly, `economics::actionable`) whose
/// budget fits a scenario counts; sums are exact integers; UNKNOWN is never 0.
#[test]
fn counterfactual_applies_the_production_admission_exactly() {
    let rows = [
        row(
            1,
            Some(3 * MICRO),
            true,
            100 * 1_000_000,
            "NO_SWAP_INVENTORY",
        ),
        row(
            2,
            Some(5 * MICRO),
            true,
            900 * 1_000_000,
            "ZERO_PREFUND_SAME_TX",
        ),
        row(
            3,
            Some(2 * MICRO),
            true,
            100 * 1_000_000,
            "NO_SWAP_INVENTORY",
        ), // == minimum
        row(
            4,
            Some(9 * MICRO),
            false,
            100 * 1_000_000,
            "NO_SWAP_INVENTORY",
        ), // late
        row(5, None, true, 100 * 1_000_000, "NO_SWAP_INVENTORY"), // UNKNOWN
        unmatched(6),
    ];
    let refs: Vec<_> = rows.iter().collect();
    let c = counterfactual(&refs, &params(2, &[500, 1_000], &[]), "M");
    assert_eq!(
        (c.matched, c.actionable, c.below_minimum, c.late, c.unknown),
        (5, 2, 1, 1, 1)
    );
    assert_eq!(c.scenarios[0].executions, 1, "900 USD does not fit 500");
    assert_eq!(c.scenarios[0].over_budget, 1);
    assert_eq!(c.scenarios[0].net_usd_micro, 3 * MICRO);
    assert_eq!(c.scenarios[1].executions, 2);
    assert_eq!(c.scenarios[1].net_usd_micro, 8 * MICRO);
    assert_eq!(c.landing, "UNPROVEN");
    assert!(!c.concurrent_capital_allocation_proven);
    assert_eq!(c.loss_attribution.get("NOT_DETECTED"), Some(&1));
    // Funding admission: only the selected candidate's class counts.
    let only = counterfactual(&refs, &params(2, &[1_000], &["NO_SWAP_INVENTORY"]), "M");
    assert_eq!((only.actionable, only.funding_excluded), (1, 1));
    // An unknown budget is never fundable.
    let mut no_budget = rows[0].clone();
    no_budget.facts.required_total_budget_usd_micro = None;
    assert_eq!(
        outcome(&no_budget, &params(0, &[1_000], &[])),
        Outcome::Unknown
    );
    let o = observed(&refs);
    assert_eq!((o.liquidations, o.protocol_fee_lamports), (6, 30_000));
    assert_eq!(
        o.transaction_fee_unknown, 6,
        "unknown fees are counted, never 0"
    );
}

/// Parameters are validated (no negative minimum, ascending capital, known classes) and the
/// command-line amounts are exact decimals.
#[test]
fn params_and_amounts_are_validated_exactly() {
    assert_eq!(params(0, &[1], &["NO_SWAP_INVENTORY"]).validate(), Ok(()));
    for bad in [
        params(-1, &[1], &[]),
        params(0, &[], &[]),
        params(0, &[2, 1], &[]),
        params(0, &[1], &["FORGED"]),
        params(0, &[1], &["ZERO_PREFUND_SAME_TX", "NO_SWAP_INVENTORY"]),
    ] {
        assert!(bad.validate().is_err(), "{bad:?}");
    }
    assert_eq!(parse_min_profit("0.000001"), Ok(1));
    assert_eq!(parse_min_profit("12.5"), Ok(12_500_000));
    assert!(parse_min_profit("-1").is_err());
    assert!(parse_min_profit("1e3").is_err());
    assert_eq!(
        parse_funding("ZERO_PREFUND_SAME_TX,NO_SWAP_INVENTORY").unwrap(),
        vec!["NO_SWAP_INVENTORY", "ZERO_PREFUND_SAME_TX"]
    );
    assert_eq!(parse_funding("all").unwrap(), Vec::<String>::new());
    assert!(parse_capital_usd("0").is_err());
}

/// The summary hash covers rows and summary: the same rows and parameters give the same hash,
/// any changed amount or parameter a different one.
#[test]
fn summary_hash_is_deterministic_and_complete() {
    let rows = vec![
        row(1, Some(3 * MICRO), true, 1_000_000, "NO_SWAP_INVENTORY"),
        unmatched(2),
    ];
    let p = params(0, &[10], &[]);
    let a = summarize(&rows, &p, "M").unwrap();
    assert_eq!(a, summarize(&rows, &p, "M").unwrap());
    let mut changed = rows.clone();
    changed[0].facts.executable_net_usd_micro = Some(3 * MICRO + 1);
    assert_ne!(
        a.result_sha256,
        summarize(&changed, &p, "M").unwrap().result_sha256
    );
    assert_ne!(
        a.result_sha256,
        summarize(&rows, &params(1, &[10], &[]), "M")
            .unwrap()
            .result_sha256
    );
}

/// No look-ahead in the search: the point is chosen on the training rows only — rewriting every
/// holdout row changes the holdout score, never the choice; the split is by chain time.
#[test]
fn optimizer_chooses_on_training_rows_only() {
    // Window [0, 100 s): training rows at 10 s and 20 s, holdout at 60 s (holdout 4_000 bps).
    let window = (0, 100_000_000_000);
    let mut rows = vec![
        row(10, Some(4 * MICRO), true, 1_000_000, "NO_SWAP_INVENTORY"),
        row(20, Some(MICRO), true, 1_000_000, "ZERO_PREFUND_SAME_TX"),
        row(
            60,
            Some(50 * MICRO),
            true,
            1_000_000,
            "ZERO_PREFUND_SAME_TX",
        ),
    ];
    let (split_ns, train, holdout) = split(&rows, window, 4_000).unwrap();
    assert_eq!(split_ns, 60_000_000_000);
    assert_eq!((train.len(), holdout.len()), (2, 1));
    let space = Space {
        min_profit_usd_micro: vec![0, 2 * MICRO],
        funding: vec![vec![], vec!["ZERO_PREFUND_SAME_TX".into()]],
        capital_usd_micro: vec![1_000_000],
    };
    let choose = |rows: &[BacktestRow]| choose(&space.points(), rows, window, 4_000);
    // Training: min 0 / all classes earns 5 USD on 1 USD capital (50_000 bps).
    assert_eq!(choose(&rows), Ok(Some(0)));
    rows[2].facts.executable_net_usd_micro = Some(-50 * MICRO);
    rows[2].profit_per_capital_bps = Some(-500_000);
    assert_eq!(
        choose(&rows),
        Ok(Some(0)),
        "holdout rows never change the choice"
    );
    // A row without a chain time cannot be split: fail closed.
    rows[0].block_time = None;
    assert_eq!(
        split(&rows, window, 4_000).err().as_deref(),
        Some("OPTIMIZE_ROW_TIME_UNKNOWN")
    );
    for bad in [0, 10_000] {
        assert!(split(&rows, window, bad).is_err());
    }
}

/// A grid point without any actionable training row has no objective (NO_SAMPLE), never 0; ties
/// prefer fewer UNKNOWN rows.
#[test]
fn objective_is_exact_and_unknown_is_never_zero() {
    let rows = [row(
        1,
        Some(3 * MICRO),
        true,
        2 * 1_000_000,
        "NO_SWAP_INVENTORY",
    )];
    let refs: Vec<_> = rows.iter().collect();
    let s = score(&refs, &params(0, &[2, 4], &[]), "M");
    // 3 USD on 2 USD capital = 15_000 bps (4 USD: 7_500).
    assert_eq!(s.objective_bps, Some(15_000));
    assert_eq!(score(&refs, &params(5, &[2], &[]), "M").objective_bps, None);
    let unknown = [row(2, None, true, 1, "NO_SWAP_INVENTORY")];
    let u: Vec<_> = unknown.iter().collect();
    let (a, b) = (
        score(&refs, &params(0, &[2], &[]), "M"),
        score(&u, &params(0, &[2], &[]), "M"),
    );
    assert!(better(&a, &b) && !better(&b, &a));
}

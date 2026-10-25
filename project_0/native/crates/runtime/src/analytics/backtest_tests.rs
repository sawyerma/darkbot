// Native backtest/optimizer: the production admission over typed rows and false positives
// (exact amounts, failed-attempt costs, UNKNOWN never 0, no look-ahead, deterministic search).
// End-to-end runs over `.p0j` evidence: market/report_tests.rs.
use super::*;

const MICRO: i128 = 1_000_000;
const MICRO_U: u64 = 1_000_000;

/// A verified liquidation joined with one evaluation: `net` (USD-micro) executable when `Some`,
/// signed before the winner's slot when `early`, needing `budget` (USD-micro); a failed attempt
/// of it costs 1 USD.
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
        failed_attempt_reserve_usd_micro: Some(MICRO_U),
    }
}

fn unmatched(slot: u64) -> BacktestRow {
    let mut r = row(slot, None, false, 1, "NO_SWAP_INVENTORY");
    r.facts.record_digest = None;
    r.facts.loss_reason = "NOT_DETECTED".into();
    r
}

/// An executable evaluation without any verified liquidation (a false positive) received at
/// `t_rx_ns`, costing `reserve` (USD-micro) when sent.
fn attempt(t_rx_ns: u64, net: i128, budget: u64, funding: &str, reserve: Option<u64>) -> Attempt {
    Attempt {
        account: format!("fp-{t_rx_ns}"),
        sign_slot: Some(t_rx_ns),
        t_rx_ns,
        funding: funding.into(),
        net_usd_micro: net,
        profit_per_capital_bps: Some(net * 10_000 / i128::from(budget)),
        required_total_budget_usd_micro: Some(budget),
        failed_attempt_reserve_usd_micro: reserve,
        record_digest: [0xEE; 32],
    }
}

fn params(min_usd: i128, capital_usd: &[u64], funding: &[&str]) -> Params {
    Params {
        min_profit_usd_micro: min_usd * MICRO,
        capital_usd_micro: capital_usd.iter().map(|c| c * MICRO_U).collect(),
        funding: funding.iter().map(|f| f.to_string()).collect(),
    }
}

/// A deadline no unit test reaches.
fn far() -> Instant {
    Instant::now() + Duration::from_secs(3_600)
}

/// Every admission path of the production rule: only an exact-match, executable, known
/// evaluation above the minimum (strictly, `economics::actionable`) whose budget fits a scenario
/// is attempted. Pre-winner-slot: a win; in or after the winner's slot: a lost race; without a
/// verified liquidation: a false positive — both failures cost their recorded reserve. Sums are
/// exact integers; UNKNOWN is never 0.
#[test]
fn counterfactual_applies_the_production_admission_exactly() {
    let rows = [
        row(1, Some(3 * MICRO), true, 100 * MICRO_U, "NO_SWAP_INVENTORY"),
        row(
            2,
            Some(5 * MICRO),
            true,
            900 * MICRO_U,
            "ZERO_PREFUND_SAME_TX",
        ),
        row(3, Some(2 * MICRO), true, 100 * MICRO_U, "NO_SWAP_INVENTORY"), // == minimum
        row(
            4,
            Some(9 * MICRO),
            false,
            100 * MICRO_U,
            "NO_SWAP_INVENTORY",
        ), // lost race
        row(5, None, true, 100 * MICRO_U, "NO_SWAP_INVENTORY"),            // UNKNOWN
        unmatched(6),
    ];
    let fp = [attempt(
        7,
        4 * MICRO,
        100 * MICRO_U,
        "NO_SWAP_INVENTORY",
        Some(MICRO_U / 2),
    )];
    let refs: Vec<_> = rows.iter().collect();
    let fps: Vec<_> = fp.iter().collect();
    let c = counterfactual(&refs, &fps, &params(2, &[500, 1_000], &[]), "M");
    assert_eq!(
        (
            c.matched,
            c.wins,
            c.below_minimum,
            c.lost_races,
            c.no_chain_event,
            c.unknown
        ),
        (5, 2, 1, 1, 1, 1)
    );
    // 500 USD: the 900 USD win does not fit; the lost race and the false positive cost 1.5 USD.
    let s = &c.scenarios[0];
    assert_eq!((s.wins, s.over_budget, s.failed_attempts), (1, 1, 2));
    assert_eq!(s.failed_cost_usd_micro, 1_500_000);
    assert_eq!(s.net_usd_micro, Some(3 * MICRO - 1_500_000));
    let s = &c.scenarios[1];
    assert_eq!((s.wins, s.over_budget, s.failed_attempts), (2, 0, 2));
    assert_eq!(s.net_usd_micro, Some(8 * MICRO - 1_500_000));
    assert_eq!(c.landing, "UNPROVEN");
    assert!(c.safety.starts_with("NOT_EVALUATED"));
    assert!(!c.concurrent_capital_allocation_proven);
    assert_eq!(c.loss_attribution.get("NOT_DETECTED"), Some(&1));
    // Funding admission: only the selected candidate's class counts.
    let only = counterfactual(
        &refs,
        &fps,
        &params(2, &[1_000], &["NO_SWAP_INVENTORY"]),
        "M",
    );
    assert_eq!((only.wins, only.funding_excluded), (1, 1));
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

/// A failed attempt whose reserve is unknown makes every scenario it fits UNKNOWN, never a net
/// that silently omits the cost.
#[test]
fn unknown_failed_attempt_cost_is_never_zero() {
    let rows = [row(1, Some(3 * MICRO), true, MICRO_U, "NO_SWAP_INVENTORY")];
    let fp = [attempt(2, MICRO, MICRO_U, "NO_SWAP_INVENTORY", None)];
    let refs: Vec<_> = rows.iter().collect();
    let fps: Vec<_> = fp.iter().collect();
    let c = counterfactual(&refs, &fps, &params(0, &[1, 2], &[]), "M");
    for s in &c.scenarios {
        assert_eq!((s.failed_cost_unknown, s.net_usd_micro), (1, None));
    }
    assert_eq!(
        score(&refs, &fps, &params(0, &[1], &[]), "M").objective_bps,
        None
    );
}

/// Parameters are validated (no negative minimum, ascending capital, known classes), the
/// command-line amounts are exact decimals, a flag without its value is an error and a profile
/// excludes explicit parameters.
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
    let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(flag(&args(&["--holdout-bps"]), "--holdout-bps").is_err());
    assert!(flag(
        &args(&["--holdout-bps", "--max-points", "4"]),
        "--holdout-bps"
    )
    .is_err());
    assert_eq!(flag(&args(&["--x", "1"]), "--x"), Ok(Some("1".into())));
    assert!(parse_budget(&args(&["--max-seconds", "1"])).is_err());
    assert!(parse_params(
        Path::new("/nonexistent"),
        &args(&["--profile", "p.p0j", "--funding", "all"])
    )
    .unwrap_err()
    .starts_with("--profile excludes"));
}

/// The summary hash covers parameters, rows, attempts and summary: the same inputs give the same
/// hash, any changed amount, cost or parameter a different one.
#[test]
fn summary_hash_is_deterministic_and_complete() {
    let rows = vec![
        row(1, Some(3 * MICRO), true, MICRO_U, "NO_SWAP_INVENTORY"),
        unmatched(2),
    ];
    let fps = vec![attempt(
        3,
        MICRO,
        MICRO_U,
        "NO_SWAP_INVENTORY",
        Some(MICRO_U),
    )];
    let p = params(0, &[10], &[]);
    let a = summarize(&rows, &fps, &p, "M").unwrap();
    assert_eq!(a, summarize(&rows, &fps, &p, "M").unwrap());
    let mut changed = rows.clone();
    changed[0].facts.executable_net_usd_micro = Some(3 * MICRO + 1);
    assert_ne!(
        a.result_sha256,
        summarize(&changed, &fps, &p, "M").unwrap().result_sha256
    );
    let mut cost = fps.clone();
    cost[0].failed_attempt_reserve_usd_micro = Some(MICRO_U + 1);
    assert_ne!(
        a.result_sha256,
        summarize(&rows, &cost, &p, "M").unwrap().result_sha256
    );
    assert_ne!(
        a.result_sha256,
        summarize(&rows, &fps, &params(1, &[10], &[]), "M")
            .unwrap()
            .result_sha256
    );
}

/// No look-ahead in the search: the point is chosen on the training part only — rewriting every
/// holdout row changes the holdout score, never the choice. Rows split by chain time, false
/// positives by receipt time, at the same share.
#[test]
fn optimizer_chooses_on_training_rows_only() {
    // Windows [0, 100 s): training rows at 10 s and 20 s, holdout at 60 s (holdout 4_000 bps).
    let window = (0, 100_000_000_000);
    let mut rows = vec![
        row(10, Some(4 * MICRO), true, MICRO_U, "NO_SWAP_INVENTORY"),
        row(20, Some(MICRO), true, MICRO_U, "ZERO_PREFUND_SAME_TX"),
        row(60, Some(50 * MICRO), true, MICRO_U, "ZERO_PREFUND_SAME_TX"),
    ];
    let fps = vec![
        attempt(30_000_000_000, MICRO, MICRO_U, "NO_SWAP_INVENTORY", Some(1)),
        attempt(70_000_000_000, MICRO, MICRO_U, "NO_SWAP_INVENTORY", Some(1)),
    ];
    let parts = split(&rows, &fps, window, window, 4_000).unwrap();
    assert_eq!(parts.split_unix_ns, 60_000_000_000);
    assert_eq!((parts.train.0.len(), parts.train.1.len()), (2, 1));
    assert_eq!((parts.holdout.0.len(), parts.holdout.1.len()), (1, 1));
    let space = Space {
        min_profit_usd_micro: vec![0, 2 * MICRO],
        funding: vec![vec![], vec!["ZERO_PREFUND_SAME_TX".into()]],
        capital_usd_micro: vec![MICRO_U],
    };
    let choose = |rows: &[BacktestRow]| {
        let s = split(rows, &fps, window, window, 4_000).unwrap();
        search(&space.points(), (&s.train.0, &s.train.1), "M", far()).map(|(_, c)| c)
    };
    // Training: min 0 / all classes earns 5 USD minus 1 USD-micro on 1 USD capital.
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
        split(&rows, &fps, window, window, 4_000).err().as_deref(),
        Some("OPTIMIZE_ROW_TIME_UNKNOWN")
    );
    for bad in [0, 10_000] {
        assert!(split(&rows, &fps, window, window, bad).is_err());
    }
    // An expired deadline stops the search.
    assert!(search(&space.points(), (&[], &[]), "M", Instant::now()).is_err());
}

/// False positives cost money: with them the loosest grid point is no longer chosen by default.
#[test]
fn failed_attempts_keep_the_optimizer_from_the_loosest_point() {
    let rows = [row(1, Some(6 * MICRO), true, MICRO_U, "NO_SWAP_INVENTORY")];
    let fp: Vec<_> = (2..5)
        .map(|t| attempt(t, MICRO, MICRO_U, "NO_SWAP_INVENTORY", Some(3 * MICRO_U)))
        .collect();
    let refs: Vec<_> = rows.iter().collect();
    let fps: Vec<_> = fp.iter().collect();
    let space = Space {
        min_profit_usd_micro: vec![0, 5 * MICRO],
        funding: vec![vec![]],
        capital_usd_micro: vec![MICRO_U],
    };
    let (scores, chosen) = search(&space.points(), (&refs, &fps), "M", far()).unwrap();
    // min 0: 6 USD win minus three 3 USD failures = -3 USD; min 5: the 1 USD attempts are below.
    assert_eq!(scores[0].objective_bps, Some(-30_000));
    assert_eq!(scores[1].objective_bps, Some(60_000));
    assert_eq!(chosen, Some(1));
}

/// A grid point without any attempt has no objective (never 0); ties prefer fewer UNKNOWN
/// evaluations.
#[test]
fn objective_is_exact_and_unknown_is_never_zero() {
    let rows = [row(
        1,
        Some(3 * MICRO),
        true,
        2 * MICRO_U,
        "NO_SWAP_INVENTORY",
    )];
    let refs: Vec<_> = rows.iter().collect();
    let s = score(&refs, &[], &params(0, &[2, 4], &[]), "M");
    // 3 USD on 2 USD capital = 15_000 bps (4 USD: 7_500).
    assert_eq!(s.objective_bps, Some(15_000));
    assert_eq!(
        score(&refs, &[], &params(5, &[2], &[]), "M").objective_bps,
        None
    );
    let unknown = [row(2, None, true, 1, "NO_SWAP_INVENTORY")];
    let u: Vec<_> = unknown.iter().collect();
    let (a, b) = (
        score(&refs, &[], &params(0, &[2], &[]), "M"),
        score(&u, &[], &params(0, &[2], &[]), "M"),
    );
    assert!(better(&a, &b) && !better(&b, &a));
}

/// U-8 (PERSIST-04): the binary layouts of the backtest kinds 350–355 are pinned (kind, schema,
/// SHA-256 of a canonical value). A failing pin is a layout change: bump the kind's `SCHEMA` and
/// keep the old frames readable (`decode_earlier`), then renew the pin — never re-pin silently.
#[test]
fn backtest_kind_layouts_are_pinned() {
    fn pin<P: crate::analytics::payload::Payload>(v: &P) -> (u16, u16, String) {
        let bytes = crate::analytics::payload::encode(v).unwrap();
        (
            P::KIND,
            P::SCHEMA,
            solana_sdk::hash::hash(&bytes).to_string(),
        )
    }
    let rows = vec![
        row(1, Some(3 * MICRO), true, MICRO_U, "NO_SWAP_INVENTORY"),
        unmatched(2),
    ];
    let fps = vec![attempt(
        3,
        MICRO,
        MICRO_U,
        "NO_SWAP_INVENTORY",
        Some(MICRO_U),
    )];
    let p = params(0, &[10], &[]);
    let header = BacktestHeader {
        job_id: "job".into(),
        capture_run_id: "run".into(),
        capture_evidence_digest: "evidence".into(),
        verification_spec_id: "spec".into(),
        verification_result_id: "result".into(),
        verification_status: "CANONICAL_VERIFIED".into(),
        verified_through_slot: Some(9),
        capture_window: (1, 2),
        live_coverage: "COMPLETE".into(),
        market_commitment: "confirmed".into(),
        earlier_attribution_records: 3,
        params: p.clone(),
        engine: "engine".into(),
        binary_sha256: [7; 32],
    };
    let refs = (
        rows.iter().collect::<Vec<_>>(),
        fps.iter().collect::<Vec<_>>(),
    );
    let s = score(&refs.0, &refs.1, &p, "M");
    let optimize = OptimizeResult {
        job_id: "opt".into(),
        capture_run_id: "run".into(),
        capture_evidence_digest: "evidence".into(),
        verification_result_id: "result".into(),
        verification_status: "CANONICAL_VERIFIED".into(),
        verified_through_slot: Some(9),
        live_coverage: "COMPLETE".into(),
        recorded_min_profit_usd_micro: MICRO,
        baseline: p.clone(),
        space: Space {
            min_profit_usd_micro: vec![0, MICRO],
            funding: vec![vec![], vec!["NO_SWAP_INVENTORY".into()]],
            capital_usd_micro: vec![MICRO_U],
        },
        holdout_bps: 2_000,
        split_unix_ns: 5,
        train_rows: 2,
        train_attempts: 1,
        holdout_rows: 0,
        holdout_attempts: 0,
        train: vec![s.clone()],
        chosen: Some(0),
        chosen_holdout: Some(s.clone()),
        baseline_train: s.clone(),
        baseline_holdout: s,
        engine: "engine".into(),
        binary_sha256: [7; 32],
        result_sha256: [8; 32],
    };
    let profile = Profile {
        params: p.clone(),
        origin: ORIGIN_OPTIMIZER.into(),
        optimize_job_id: Some("opt".into()),
    };
    let pins = vec![
        pin(&header),
        pin(&rows[0]),
        pin(&summarize(&rows, &fps, &p, "M").unwrap()),
        pin(&optimize),
        pin(&profile),
        pin(&fps[0]),
    ];
    let expected: Vec<(u16, u16, String)> = [
        (350, 2, "wWwCseMMpx8N5vf4aLZsphQQWHwViSSNQPsnzPPDi1V"),
        (351, 2, "E9xuPeDYKCRjwh8uANfYxHQ7TkCjYwHpgfsmcKT7Xopv"),
        (352, 2, "E9Cewjqu4dCwFpQWuNa1f6NMh25i7JqHi5SL341MyH9W"),
        (353, 2, "5AnB2eGjPXazZajy1wbL5GkWduBiLNnJMcb399Wi2cj8"),
        (354, 2, "4CBZ2VK9HEfYghj1cqZKH9bVrY2oayUpHDYnntijj5BS"),
        (355, 2, "J5TMm7LgFNYN7Y84kirkQQjTteQQEgk4ZDmtkKPgGW7s"),
    ]
    .into_iter()
    .map(|(k, s, h)| (k, s, h.to_string()))
    .collect();
    assert_eq!(pins, expected);
}

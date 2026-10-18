// A6 (included in the canonical `regression` tests): the canonical capital report uses the run's
// configured capital list live and offline.

/// A6: with a custom capital list the canonical summary, the joined row view and the offline
/// replay use exactly that list (identical fundable/blocked/unknown live and offline); the
/// default stays the standard list and an invalid list is refused.
#[test]
fn a6_canonical_capital_uses_the_configured_list_live_and_offline() {
    let custom = vec![150, 300];
    let mut w = worker();
    w.h.cfg.capital = custom.clone();
    let r = identified();
    w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
    let mut event = row(&r, 10, 0).market;
    event.block_time = Some(101);
    w.canonical_market.push(event.clone());
    w.finalize_canonical().unwrap();
    let live = w.canonical_metrics(0, SNAPSHOT_COUNT);
    let budgets: Vec<u64> = live.capital.iter().map(|c| c.budget_usd).collect();
    assert_eq!(budgets, custom);
    let view = joined_value(&w.canonical_joined[0]);
    let listed: Vec<u64> = view["fundable_by_budget"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["budget_usd"].as_u64().unwrap())
        .collect();
    assert_eq!(listed, custom);

    let input = w.h.cfg.run_dir.join("a6_market.jsonl");
    fs::write(&input, format!("{}\n", serde_json::to_string(&event).unwrap())).unwrap();
    let out = w.h.cfg.run_dir.join("a6_replay");
    let plan = MeasurementPlan::new(61).unwrap();
    replay(
        &input,
        &w.h.cfg.run_dir.join("shadow_evaluations.jsonl"),
        &out,
        &plan,
        100_000_000_000,
        30,
        &ReplayScope {
                capture_start_ns: None,
                capital: &custom,
            },
    )
    .unwrap();
    let v: Value =
        serde_json::from_reader(File::open(out.join("economics_final.json")).unwrap()).unwrap();
    assert_eq!(v["capital_usd"], json!(custom));
    let offline: Vec<CanonicalCapital> =
        serde_json::from_value(v["cumulative"]["capital"].clone()).unwrap();
    assert_eq!(offline, live.capital, "fundable/blocked/unknown identical");

    assert_eq!(
        summarize(&[], 1., 0, true, &CAPITAL_USD)
            .capital
            .iter()
            .map(|c| c.budget_usd)
            .collect::<Vec<_>>(),
        CAPITAL_USD.to_vec()
    );
    for bad in ["", "0", "abc", "100,,200"] {
        assert!(super::super::parse_capital(bad).is_err(), "{bad:?}");
    }
}

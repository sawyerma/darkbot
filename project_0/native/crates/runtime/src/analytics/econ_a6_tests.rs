// A6 (included in the canonical `regression` tests): the canonical capital report uses the run's
// configured capital list live and offline.

/// A6: with a custom capital list the canonical summary, the joined row view and the native
/// join use exactly that list; the backtest reads the same list (USD-micro); the default stays
/// the standard list and an invalid list is refused.
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

    // The native join carries the configured list; the backtest takes it in USD-micro.
    let joined = join(
        vec![event],
        &journal::binary_path(&w.h.cfg.run_dir.join("shadow_evaluations.jsonl")),
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &JoinContext {
            market_commitment: "UNKNOWN",
            slot_first_seen: None,
            capture: None,
            capital: &custom,
        },
    )
    .unwrap();
    assert_eq!(joined.rows[0].capital, custom);
    assert_eq!(
        crate::analytics::backtest::parse_capital_usd("300,150").unwrap(),
        vec![150_000_000, 300_000_000]
    );
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

// Included in observer tests: the market summary (`summary.json`, the summaries handed to the
// analytics worker) is recorded once and every later change must render it identically
// (JFI-FIX-05 parity). Fixed keys only, so the render is deterministic.

/// A rich capture: live receivership events (two in one transaction), a failed and a truncated
/// transaction, a duplicate, a transport gap with reconnect and a full-slot reconciliation with
/// one canonical liquidation and deployment continuity.
fn golden_core() -> ObserverCore {
    let p = program();
    let k = |n: u8| Pubkey::new_from_array([n; 32]);
    let mut c = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    c.reconcile_cfg.max_attempts = 1;
    c.reconcile_cfg.finality_wait_secs = 0;
    let mut s = VecSink::default();
    gap_slot_frame(&mut c, &mut s, 10, 10);
    let mut live = |c: &mut ObserverCore, sig: &str, slot, data: Vec<String>, err, at| {
        c.on_logs(
            logs_event(sig, slot, logs_with(&p, &data), err, at),
            0,
            &mut s,
        )
        .unwrap();
    };
    live(
        &mut c,
        "live-a",
        10,
        vec![data_line(k(1), k(2), 7.0, 2.0, 1)],
        None,
        11,
    );
    live(
        &mut c,
        "live-b",
        11,
        vec![
            data_line(k(3), k(2), 9.5, 4.0, 2),
            data_line(k(4), k(5), 3.0, 1.0, 3),
        ],
        None,
        12,
    );
    live(
        &mut c,
        "live-b",
        11,
        vec![data_line(k(3), k(2), 9.5, 4.0, 2)],
        None,
        13,
    );
    live(
        &mut c,
        "failed",
        11,
        vec![data_line(k(6), k(7), 1.0, 0.5, 1)],
        Some("x"),
        14,
    );
    c.on_logs(
        logs_event(
            "truncated",
            11,
            vec![
                format!("Program {p} invoke [1]"),
                data_line(k(8), k(9), 2.0, 1.0, 1),
                "Log truncated".into(),
            ],
            None,
            15,
        ),
        0,
        &mut s,
    )
    .unwrap();
    gap_open(&mut c, &mut s, Some(11), 16);
    gap_reconnect(&mut c, &mut s, 17);
    gap_slot_frame(&mut c, &mut s, 12, 18);
    c.stop_observation();
    let mut found = liq_tx_json(
        "found",
        11,
        &[(k(10), k(11))],
        false,
        false,
        logs_with(&p, &[data_line(k(10), k(11), 6.0, 2.5, 1)]),
        false,
    );
    found["meta"]["innerInstructions"] = json!([]);
    c.reconcile_slots_wire(
        &mut |m, params, _| {
            Ok(match m {
                "getSlot" => json!(12),
                "getBlocks" => json!([10, 11, 12]),
                "getBlock" => {
                    let slot = params[0].as_u64().unwrap();
                    let txs = if slot == 11 {
                        vec![found.clone()]
                    } else {
                        vec![]
                    };
                    canonical_block(slot, slot - 1, txs)
                }
                _ => panic!(),
            })
        },
        &mut s,
    )
    .unwrap();
    let before = paths::Deployment {
        context_slot: 9,
        hash: "H57mqzeof3mnCmVTPJzmzeTfs7G6zbpaApQP4yNycr3f".into(),
        program_data: "fixture".into(),
    };
    let mut after = before.clone();
    after.context_slot = 12;
    c.deployment_before = Some(before);
    c.deployment_after = Some(after);
    c
}

#[test]
fn market_summary_render_matches_golden() {
    let c = golden_core();
    let mut cx = ctx(true, true, 4.0);
    cx.process_started_at = "2026-10-17T00:00:00Z".into();
    cx.stream_ready_at = Some("2026-10-17T00:00:01Z".into());
    cx.min_sample_liquidations = 2;
    cx.enrichment_available = false;
    let v = view(&c.summary(&cx));
    assert!(v["liquidation_events"].as_u64().unwrap() >= 3);
    assert!(v["full_slot_reconciliation_complete"].as_bool().unwrap());
    crate::analytics::golden::check("market_summary", &v, &["provider_budget"]);
    let early = view(&c.summary(&ctx(false, false, 1.5)));
    crate::analytics::golden::check("market_summary_early", &early, &["provider_budget"]);
}

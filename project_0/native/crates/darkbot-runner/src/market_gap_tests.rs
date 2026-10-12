// Included inside the existing market observer tests, reusing its F8/log fixtures.
#[test]
fn transport_recovery_is_not_full_market_truth_under_master_contract() {
    let (mut c, mut s) = gap_core(10, 10);
    c.recover_gaps(&mut |slot, _| Ok(gap_block(slot, vec![])), &mut s)
        .unwrap();
    let v = c.summary(&ctx(true, true, 4.0));
    assert_eq!(v["receivership_transport_capture_complete"], true);
    assert_eq!(v["market_capture_complete"], false);
    assert_eq!(v["full_slot_reconciliation_complete"], false);
    assert_eq!(v["liquidation_path_coverage_complete"], false);
    assert_eq!(v["verdict"], "MARKET_DATA_INCOMPLETE");
}
fn gap_open(core: &mut ObserverCore, sink: &mut VecSink, from: Option<u64>, at: u64) {
    core.on_ingress_event(
        IngressEvent::Gap {
            source: SourceKind::WsRpcControl,
            from_slot: from,
            to_slot: None,
            t_rx_ns: at,
        },
        0,
        sink,
    )
    .unwrap();
}
fn gap_reconnect(core: &mut ObserverCore, sink: &mut VecSink, at: u64) {
    core.on_ingress_event(
        IngressEvent::Reconnected {
            source: SourceKind::WsRpcControl,
            attempt: 2,
            t_rx_ns: at,
        },
        0,
        sink,
    )
    .unwrap();
}
fn gap_slot_frame(core: &mut ObserverCore, sink: &mut VecSink, slot: u64, at: u64) {
    core.on_ingress_event(
        IngressEvent::Slot(SlotUpdate {
            source: SourceKind::WsRpcControl,
            slot,
            parent: Some(slot - 1),
            status: SlotStatus::Processed,
            t_rx_ns: at,
        }),
        0,
        sink,
    )
    .unwrap();
}
fn gap_core(lo: u64, hi: u64) -> (ObserverCore, VecSink) {
    let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    core.gap_cfg.max_attempts = 1;
    gap_open(&mut core, &mut sink, Some(lo), 10);
    gap_reconnect(&mut core, &mut sink, 20);
    gap_slot_frame(&mut core, &mut sink, hi, 30);
    core.stop_observation();
    (core, sink)
}
fn gap_block(slot: u64, txs: Vec<Value>) -> gap::GapSlot {
    gap::GapSlot::Block(
        json!({"blockhash":"fixture-finalized-hash", "parentSlot":slot-1, "transactions":txs}),
    )
}
fn gap_liquidation(sig: &str, slot: u64, failed: bool) -> Value {
    let target = Pubkey::new_unique();
    let receiver = Pubkey::new_unique();
    liq_tx_json(
        sig,
        slot,
        &[(target, receiver)],
        false,
        false,
        logs_with(&program(), &[data_line(target, receiver, 7.0, 2.0, 1)]),
        failed,
    )
}

#[test]
fn gap_then_reconnect_then_slot_closes_range() {
    let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    gap_open(&mut core, &mut sink, Some(10), 10);
    gap_slot_frame(&mut core, &mut sink, 11, 11); // no ACK: cannot close
    gap_reconnect(&mut core, &mut sink, 20);
    assert_eq!(
        core.summary(&ctx(true, true, 1.0))["gaps"][0]["first_post_reconnect_slot"],
        Value::Null
    );
    assert!(!core.coverage_complete());
    gap_slot_frame(&mut core, &mut sink, 12, 30);
    let g = &core.summary(&ctx(true, true, 1.0))["gaps"][0];
    assert_eq!(g["first_post_reconnect_slot"], 12);
    assert_eq!(g["first_post_reconnect_t_rx_ns"], 30);
    assert_eq!(g["reconnect_attempt"], 2);
    assert!(!core.coverage_complete());
    assert!(sink.cov.iter().any(|v| v["event"] == "GAP_BOUNDARY"));
}
#[test]
fn gap_boundary_slots_are_included_and_complete_backfill_restores_coverage() {
    let (mut c, mut s) = gap_core(10, 12);
    let mut fetched = vec![];
    c.recover_gaps(
        &mut |slot, _| {
            fetched.push(slot);
            Ok(gap_block(slot, vec![]))
        },
        &mut s,
    )
    .unwrap();
    assert_eq!(fetched, vec![10, 11, 12]);
    assert!(c.coverage_complete());
    let v = c.summary(&ctx(true, true, 4.0));
    assert_eq!(v["gap_recovered"], 1);
    assert_eq!(v["gap_unrecovered"], 0);
    assert_eq!(v["receivership_transport_capture_complete"], true);
    assert!(!v["incomplete_reasons"]
        .as_array()
        .unwrap()
        .contains(&json!("GAP")));
}
#[test]
fn missing_block_rpc_error_timeout_and_pruned_keep_gap_unrecovered() {
    for reason in [
        "RPC_MISSING_BLOCK",
        "RPC_ERROR",
        "RPC_TIMEOUT",
        "RPC_RATE_LIMIT",
        "RPC_PRUNED_OR_UNAVAILABLE",
        "RPC_UNSUPPORTED_VERSION",
    ] {
        let (mut c, mut s) = gap_core(10, 12);
        c.recover_gaps(&mut |_, _| Err(reason), &mut s).unwrap();
        assert!(!c.coverage_complete());
        let v = c.summary(&ctx(true, true, 4.0));
        assert_eq!(v["gap_unrecovered"], 1);
        assert_eq!(v["market_capture_complete"], false);
        assert_eq!(v["gaps"][0]["error_code"], reason);
    }
}
#[test]
fn confirmed_skipped_slot_is_not_false_failure() {
    let (mut c, mut s) = gap_core(10, 12);
    c.recover_gaps(
        &mut |slot, _| {
            if slot == 11 {
                Ok(gap::GapSlot::Skipped {
                    successor: 12,
                    parent: 10,
                    blockhash: "h".into(),
                })
            } else {
                Ok(gap_block(slot, vec![]))
            }
        },
        &mut s,
    )
    .unwrap();
    assert!(c.coverage_complete());
    assert_eq!(
        c.summary(&ctx(true, true, 4.0))["gap_slots_skipped_confirmed"],
        1
    );
    let (mut c, mut s) = gap_core(11, 11);
    c.recover_gaps(
        &mut |_, _| {
            Ok(gap::GapSlot::Skipped {
                successor: 12,
                parent: 11,
                blockhash: "h".into(),
            })
        },
        &mut s,
    )
    .unwrap();
    assert!(!c.coverage_complete());
}
#[test]
fn duplicate_live_and_backfilled_signature_counted_once() {
    let (mut c, mut s) = gap_core(10, 10);
    let tx = gap_liquidation("sig", 10, false);
    c.start_observation(0, 100);
    c.on_transaction(&as_observed(&tx, "sig", 10, false), 0, &mut s)
        .unwrap();
    c.stop_observation();
    c.recover_gaps(&mut |slot, _| Ok(gap_block(slot, vec![tx.clone()])), &mut s)
        .unwrap();
    assert_eq!(c.stats.liquidation_events, 1);
    assert_eq!(c.stats.unique_transactions, 1);
    assert_eq!(c.gap_stats.gap_transactions_deduplicated, 1);
    assert!(c.coverage_complete());
}
#[test]
fn duplicate_backfill_does_not_double_count_and_restores_market_gross_edge() {
    let (mut c, mut s) = gap_core(10, 10);
    c.start_observation(0, 100);
    gap_open(&mut c, &mut s, Some(10), 40);
    gap_reconnect(&mut c, &mut s, 50);
    gap_slot_frame(&mut c, &mut s, 10, 60);
    c.stop_observation();
    let tx = gap_liquidation("sig", 10, false);
    c.recover_gaps(&mut |slot, _| Ok(gap_block(slot, vec![tx.clone()])), &mut s)
        .unwrap();
    c.recover_gaps(&mut |_, _| panic!("already recovered"), &mut s)
        .unwrap();
    assert_eq!(c.stats.liquidation_events, 1);
    let v = c.summary(&ctx(true, true, 4.0));
    assert_eq!(v["gross_edge_total_usd"], 5.0);
    assert_eq!(v["gap_recovered"], 2);
    assert_eq!(v["gap_transactions_deduplicated"], 1);
    assert_eq!(s.liq[0]["t_rx_ns"], Value::Null);
    assert_eq!(s.liq[0]["observed_at_utc"], Value::Null);
    assert_eq!(s.liq[0]["source"], "gap_backfill");
    assert_eq!(s.liq[0]["live_timestamp_qualified"], false);
    assert_eq!(s.tx[0]["transaction_index"], 0);
}
#[test]
fn backfilled_non_liquidation_and_failed_transaction_do_not_create_event() {
    let (mut c, mut s) = gap_core(10, 10);
    let failed = gap_liquidation("failed", 10, true);
    let normal = liq_tx_json("normal", 10, &[], false, false, vec![], false);
    c.recover_gaps(
        &mut |slot, _| Ok(gap_block(slot, vec![failed.clone(), normal.clone()])),
        &mut s,
    )
    .unwrap();
    assert_eq!(c.stats.liquidation_events, 0);
    assert_eq!(c.stats.failed_transactions, 1);
    assert_eq!(c.stats.successful_transactions, 1);
    assert!(c.coverage_complete());
}
#[test]
fn backfill_uses_loaded_addresses_and_rejects_missing_loaded_view() {
    for loaded in [true, false] {
        let (mut c, mut s) = gap_core(10, 10);
        let t = Pubkey::new_unique();
        let r = Pubkey::new_unique();
        let tx = liq_tx_json(
            "lut",
            10,
            &[(t, r)],
            true,
            loaded,
            logs_with(&program(), &[data_line(t, r, 7.0, 2.0, 1)]),
            false,
        );
        c.recover_gaps(&mut |slot, _| Ok(gap_block(slot, vec![tx.clone()])), &mut s)
            .unwrap();
        assert_eq!(c.coverage_complete(), loaded);
        assert_eq!(c.stats.liquidation_events, u64::from(loaded));
    }
}
#[test]
fn backfilled_truncated_mismatch_fails_closed() {
    let (mut c, mut s) = gap_core(10, 10);
    let tx = liq_tx_json(
        "bad",
        10,
        &[(Pubkey::new_unique(), Pubkey::new_unique())],
        false,
        false,
        truncated_logs(&[]),
        false,
    );
    c.recover_gaps(&mut |slot, _| Ok(gap_block(slot, vec![tx.clone()])), &mut s)
        .unwrap();
    assert!(!c.coverage_complete());
    assert_eq!(c.stats.liquidation_events, 0);
}
#[test]
fn multiple_gaps_are_independent() {
    let (mut c, mut s) = gap_core(10, 10);
    c.start_observation(0, 100);
    gap_open(&mut c, &mut s, Some(20), 40);
    gap_reconnect(&mut c, &mut s, 50);
    gap_slot_frame(&mut c, &mut s, 20, 60);
    c.stop_observation();
    c.recover_gaps(
        &mut |slot, _| {
            if slot == 10 {
                Err("RPC_ERROR")
            } else {
                Ok(gap_block(slot, vec![]))
            }
        },
        &mut s,
    )
    .unwrap();
    assert_eq!(c.gap_stats.gap_recovered, 1);
    assert!(!c.coverage_complete());
}
#[test]
fn gap_without_post_reconnect_slot_or_from_slot_fails_closed() {
    for from in [None, Some(10)] {
        let mut c = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
        let mut s = VecSink::default();
        gap_open(&mut c, &mut s, from, 10);
        gap_reconnect(&mut c, &mut s, 20);
        if from.is_none() {
            gap_slot_frame(&mut c, &mut s, 12, 30);
        }
        c.stop_observation();
        c.recover_gaps(&mut |_, _| panic!("no provable range"), &mut s)
            .unwrap();
        assert!(!c.coverage_complete());
    }
}
#[test]
fn backfill_never_runs_during_capture_and_limits_fail_closed() {
    let (mut c, mut s) = gap_core(10, 12);
    c.start_observation(0, 100);
    assert!(c
        .recover_gaps(&mut |_, _| panic!("capture RPC"), &mut s)
        .is_err());
    c.stop_observation();
    c.gap_cfg.max_slots = 2;
    c.recover_gaps(&mut |_, _| panic!("slot limit"), &mut s)
        .unwrap();
    assert!(!c.coverage_complete());
    let (mut c, mut s) = gap_core(10, 10);
    c.gap_cfg.phase_max_secs = 0;
    c.recover_gaps(&mut |_, _| panic!("time limit"), &mut s)
        .unwrap();
    assert!(!c.coverage_complete());
}
#[test]
fn five_hundred_slot_recovery_is_streamed_and_bounded() {
    let (mut c, mut s) = gap_core(100, 599);
    let mut calls = 0;
    c.recover_gaps(
        &mut |slot, _| {
            calls += 1;
            Ok(gap_block(slot, vec![]))
        },
        &mut s,
    )
    .unwrap();
    assert_eq!(calls, 500);
    assert_eq!(c.gaps.len(), 1);
    assert_eq!(c.seen.len(), 0);
    assert_eq!(c.gap_stats.gap_slots_fetched, 500);
    assert!(c.coverage_complete());
    c.start_observation(0, 10000);
    c.gap_cfg.max_gaps = 1;
    gap_open(&mut c, &mut s, Some(600), 100);
    assert_eq!(c.gaps.len(), 1);
    assert!(!c.coverage_complete());
}

#[test]
fn full_window_proof_recovers_contained_gap_without_refetch() {
    let (mut c, mut s) = canonical_core(10, 12);
    c.reconcile_slots(
        &mut |m, p, _| {
            Ok(match m {
                "getSlot" => json!(12),
                "getBlocks" => json!([10, 11, 12]),
                "getBlock" => {
                    let slot = p[0].as_u64().unwrap();
                    canonical_block(slot, slot - 1, vec![])
                }
                _ => panic!(),
            })
        },
        &mut s,
    )
    .unwrap();
    assert!(c.reconciliation.complete);
    let (g, _) = gap_core(10, 12);
    c.gaps = g.gaps;
    c.recover_gaps(&mut |_, _| panic!("full proof must not refetch"), &mut s)
        .unwrap();
    let g = serde_json::to_value(&c.gaps[0]).unwrap();
    assert_eq!(g["recovery_status"], "RECOVERED");
    assert_eq!(g["slots_checked"], 3);
    assert!(s
        .cov
        .iter()
        .any(|v| v["proof"] == "FULL_FINALIZED_SLOT_RECONCILIATION"));
}

#[test]
fn complete_canonical_window_is_authority_despite_transport_gap_telemetry() {
    let (mut c, mut s) = canonical_core(10, 12);
    c.reconcile_slots(
        &mut |m, p, _| {
            Ok(match m {
                "getSlot" => json!(12),
                "getBlocks" => json!([10, 11, 12]),
                "getBlock" => {
                    let slot = p[0].as_u64().unwrap();
                    canonical_block(slot, slot - 1, vec![])
                }
                _ => panic!(),
            })
        },
        &mut s,
    )
    .unwrap();
    c.stats.gap_count = 1;
    c.mark_incomplete("GAP");
    let before = paths::Deployment {
        context_slot: 9,
        hash: "H57mqzeof3mnCmVTPJzmzeTfs7G6zbpaApQP4yNycr3f".into(),
        program_data: "fixture".into(),
    };
    let mut after = before.clone();
    after.context_slot = 12;
    c.deployment_before = Some(before);
    c.deployment_after = Some(after);
    let v = c.summary(&ctx(true, true, 4.0));
    assert_eq!(v["market_capture_complete"], true);
    assert_eq!(v["coverage_complete"], false);
    assert_eq!(v["liquidation_path_coverage_complete"], true);
    assert_eq!(v["source_coverage_verified"], true);
    c.reconciliation.complete = false;
    assert_eq!(
        c.summary(&ctx(true, true, 4.0))["market_capture_complete"],
        false
    );
}

#[test]
fn p2_527_failures_coalesce_without_losing_raw_evidence_or_claiming_recovery() {
    let mut c = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    c.start_observation(0, 10_000);
    let mut s = VecSink::default();
    for n in 0..527 {
        gap_open(&mut c, &mut s, Some(10), 10 + n * 3);
        gap_reconnect(&mut c, &mut s, 11 + n * 3); // ACKs alone don't close an interval
    }
    assert_eq!(c.gaps.len(), 1);
    assert_eq!(s.cov.iter().filter(|v| v["event"] == "GAP").count(), 527);
    let v = c.summary(&ctx(true, false, 4.0));
    assert_eq!(v["gap_count_raw"], 527);
    assert_eq!(v["gaps_merged"], 526);
    assert_eq!(v["gap_unrecovered"], 1);
    assert!(!c.incomplete_reasons.contains("GAP_STATE_CAPACITY"));
    assert!(!c.coverage_complete());
    gap_slot_frame(&mut c, &mut s, 12, 2000);
    c.stop_observation();
    c.recover_gaps(&mut |slot, _| Ok(gap_block(slot, vec![])), &mut s)
        .unwrap();
    assert!(c.coverage_complete()); // finalized backfill, not ACKs, repairs this interval
    assert_eq!(c.gap_stats.gap_recovered, 1);
}

#[test]
fn p2_failure_after_ack_invalidates_pending_restoration() {
    let mut c = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut s = VecSink::default();
    gap_open(&mut c, &mut s, None, 10);
    gap_reconnect(&mut c, &mut s, 20);
    gap_open(&mut c, &mut s, Some(10), 21);
    gap_slot_frame(&mut c, &mut s, 12, 30);
    let v = c.summary(&ctx(true, false, 4.0));
    assert_eq!(v["gaps"][0]["from_slot"], Value::Null);
    assert_eq!(v["gaps"][0]["first_post_reconnect_slot"], Value::Null);
    assert_eq!(v["gaps"][0]["raw_failures"], 2);
}

#[test]
fn p2_abort_writes_incomplete_report_and_never_enters_verification_rpc() {
    let root = std::env::temp_dir().join(format!(
        "p2-abort-{}-{}",
        std::process::id(),
        Clock::new().unix_ns(0)
    ));
    let cfg = MarketConfig::from_lookup(
        &env_of(&[
            ("P0_WS_URL", "ws://127.0.0.1:1"),
            ("P0_RPC_URL", "http://127.0.0.1:1"),
            (
                "P0_PROGRAM_ID",
                darkbot_project0::protocol::PROGRAM_ID_MAINNET,
            ),
            ("P0_MARKET_SOURCE", "logs"),
            ("P0_MARKET_OUT_DIR", root.to_str().unwrap()),
            ("P0_MARKET_RUN_ID", "abort"),
        ]),
        0,
    )
    .unwrap();
    fs::create_dir_all(&cfg.run_dir).unwrap();
    let mut sink = JsonlSink::create(&cfg.run_dir).unwrap();
    let mut core = ObserverCore::new(cfg.program, cfg.source, cfg.enrich);
    core.start_observation(0, 18_000_000_000_000);
    core.stop_observation();
    let outcome = CaptureOutcome {
        process_started_unix_s: 0,
        ready_unix_s: Some(0),
        window: Some((0, 18_000_000_000_000)),
        capture_end_ns: 120_000_000_000,
        observation_end_unix_s: 120,
        stop_reason: "INGRESS_THREAD_ENDED",
        drain_complete: true,
        write_error: None,
    };
    let before = darkbot_solana::provider::shared().snapshot()["requests"].clone();
    let (summary, _, error) = finish_capture(&cfg, &mut core, &mut sink, outcome).unwrap();
    assert!(error.is_none());
    assert_eq!(summary["market_capture_complete"], false);
    assert_eq!(summary["actual_observation_seconds"], 120.0);
    assert_eq!(
        summary["full_slot_reconciliation"]["error"],
        "VERIFICATION_DEFERRED_AFTER_ABORT"
    );
    assert_eq!(
        darkbot_solana::provider::shared().snapshot()["requests"],
        before
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn p4_timer_finalizes_locally_without_verification_rpc() {
    let root = std::env::temp_dir().join(format!(
        "p4-off-{}-{}",
        std::process::id(),
        Clock::new().unix_ns(0)
    ));
    let cfg = MarketConfig::from_lookup(
        &env_of(&[
            ("P0_WS_URL", "ws://127.0.0.1:1"),
            ("P0_RPC_URL", "http://127.0.0.1:1"),
            (
                "P0_PROGRAM_ID",
                darkbot_project0::protocol::PROGRAM_ID_MAINNET,
            ),
            ("P0_MARKET_SOURCE", "logs"),
            ("P0_MARKET_OUT_DIR", root.to_str().unwrap()),
            ("P0_MARKET_RUN_ID", "abort"),
        ]),
        0,
    )
    .unwrap();
    fs::create_dir_all(&cfg.run_dir).unwrap();
    let mut sink = JsonlSink::create(&cfg.run_dir).unwrap();
    let mut core = ObserverCore::new(cfg.program, cfg.source, cfg.enrich);
    core.start_observation(0, 18_000_000_000_000);
    core.stop_observation();
    let outcome = CaptureOutcome {
        process_started_unix_s: 0,
        ready_unix_s: Some(0),
        window: Some((0, 18_000_000_000_000)),
        capture_end_ns: 18_000_000_000_000,
        observation_end_unix_s: 18_000,
        stop_reason: "TIMER",
        drain_complete: true,
        write_error: None,
    };
    let before = darkbot_solana::provider::shared().snapshot()["requests"].clone();
    let (summary, _, error) = finish_capture(&cfg, &mut core, &mut sink, outcome).unwrap();
    assert!(error.is_none());
    assert_eq!(summary["market_capture_complete"], false);
    assert_eq!(summary["actual_observation_seconds"], 18_000.0);
    assert_eq!(
        summary["full_slot_reconciliation"]["error"],
        "VERIFICATION_NOT_RUN"
    );
    assert_eq!(
        darkbot_solana::provider::shared().snapshot()["requests"],
        before
    );
    fs::remove_dir_all(root).unwrap();
}

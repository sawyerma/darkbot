// Included inside the existing market observer tests, reusing its F8/log fixtures.
#[test]
fn transport_completeness_is_not_full_market_truth_under_master_contract() {
    let (c, _) = gap_core_clean(10, 10);
    let v = view(&c.summary(&ctx(true, true, 4.0)));
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
fn gap_core_clean(lo: u64, hi: u64) -> (ObserverCore, VecSink) {
    let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    gap_slot_frame(&mut core, &mut sink, lo, 10);
    gap_slot_frame(&mut core, &mut sink, hi, 30);
    core.stop_observation();
    (core, sink)
}
fn gap_core(lo: u64, hi: u64) -> (ObserverCore, VecSink) {
    let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    gap_open(&mut core, &mut sink, Some(lo), 10);
    gap_reconnect(&mut core, &mut sink, 20);
    gap_slot_frame(&mut core, &mut sink, hi, 30);
    core.stop_observation();
    (core, sink)
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
        view(&core.summary(&ctx(true, true, 1.0)))["gaps"][0]["first_post_reconnect_slot"],
        Value::Null
    );
    assert!(!core.coverage_complete());
    gap_slot_frame(&mut core, &mut sink, 12, 30);
    let g = &view(&core.summary(&ctx(true, true, 1.0)))["gaps"][0];
    assert_eq!(g["first_post_reconnect_slot"], 12);
    assert_eq!(g["first_post_reconnect_t_rx_ns"], 30);
    assert_eq!(g["reconnect_attempt"], 2);
    assert!(!core.coverage_complete());
    assert!(sink.cov.iter().any(|v| v["event"] == "GAP_BOUNDARY"));
}
#[test]
fn gap_intervals_are_bounded_and_capacity_is_incomplete() {
    let (mut c, mut s) = gap_core(100, 599);
    assert_eq!(c.gaps.len(), 1);
    assert!(!c.coverage_complete());
    c.start_observation(0, 10000);
    c.gap_cfg.max_gaps = 1;
    gap_open(&mut c, &mut s, Some(600), 100);
    gap_reconnect(&mut c, &mut s, 110);
    gap_slot_frame(&mut c, &mut s, 601, 120);
    gap_open(&mut c, &mut s, Some(700), 130);
    assert_eq!(c.gaps.len(), 1);
    assert!(c.incomplete_reasons.contains("GAP_STATE_CAPACITY"));
    assert!(!c.coverage_complete());
}

#[test]
fn complete_canonical_window_is_authority_despite_transport_gap_telemetry() {
    let (mut c, mut s) = canonical_core(10, 12);
    c.reconcile_slots_wire(
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
    let v = view(&c.summary(&ctx(true, true, 4.0)));
    assert_eq!(v["market_capture_complete"], true);
    assert_eq!(v["coverage_complete"], false);
    assert_eq!(v["liquidation_path_coverage_complete"], true);
    assert_eq!(v["source_coverage_verified"], true);
    c.reconciliation.complete = false;
    assert_eq!(
        view(&c.summary(&ctx(true, true, 4.0)))["market_capture_complete"],
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
    let v = view(&c.summary(&ctx(true, false, 4.0)));
    assert_eq!(v["gap_count_raw"], 527);
    assert_eq!(v["gaps_merged"], 526);
    assert_eq!(v["gap_unrecovered"], 1);
    assert!(!c.incomplete_reasons.contains("GAP_STATE_CAPACITY"));
    assert!(!c.coverage_complete());
    gap_slot_frame(&mut c, &mut s, 12, 2000);
    c.stop_observation();
    // Restored transport is still a gap: only the strict finalized scan can prove the interval.
    assert!(!c.coverage_complete());
    assert_eq!(
        view(&c.summary(&ctx(true, false, 4.0)))["gap_unrecovered"],
        1
    );
}

#[test]
fn p2_failure_after_ack_invalidates_pending_restoration() {
    let mut c = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut s = VecSink::default();
    gap_open(&mut c, &mut s, None, 10);
    gap_reconnect(&mut c, &mut s, 20);
    gap_open(&mut c, &mut s, Some(10), 21);
    gap_slot_frame(&mut c, &mut s, 12, 30);
    let v = view(&c.summary(&ctx(true, false, 4.0)));
    assert_eq!(v["gaps"][0]["from_slot"], Value::Null);
    assert_eq!(v["gaps"][0]["first_post_reconnect_slot"], Value::Null);
    assert_eq!(v["gaps"][0]["raw_failures"], 2);
}

/// P1B-T1: an RPC endpoint of this test alone — "no verification RPC" is proven by it receiving
/// no connection, not by the process-wide provider counter that parallel tests also advance.
fn idle_rpc() -> (std::net::TcpListener, String) {
    let rpc = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    rpc.set_nonblocking(true).unwrap();
    let url = format!("http://{}", rpc.local_addr().unwrap());
    (rpc, url)
}
fn assert_no_rpc(rpc: &std::net::TcpListener) {
    assert_eq!(
        rpc.accept().map(drop).map_err(|e| e.kind()),
        Err(std::io::ErrorKind::WouldBlock),
        "a request reached the RPC endpoint"
    );
}

#[test]
fn p2_abort_writes_incomplete_report_and_never_enters_verification_rpc() {
    let root = std::env::temp_dir().join(format!(
        "p2-abort-{}-{}",
        std::process::id(),
        Clock::new().unix_ns(0)
    ));
    let (rpc, rpc_url) = idle_rpc();
    let cfg = MarketConfig::from_lookup(
        &env_of(&[
            ("P0_WS_URL", "ws://127.0.0.1:1"),
            ("P0_RPC_URL", rpc_url.as_str()),
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
        finished_unix_s: 1_700_000_100,
        process_started_unix_s: 0,
        ready_unix_s: Some(0),
        window: Some((0, 18_000_000_000_000)),
        capture_end_ns: 120_000_000_000,
        observation_end_unix_s: 120,
        stop_reason: "INGRESS_THREAD_ENDED",
        drain_complete: true,
        write_error: None,
    };
    let (summary, _, error) = finish_capture(&cfg, &mut core, &mut sink, outcome).unwrap();
    let summary = view(&summary);
    assert!(error.is_none());
    assert_eq!(summary["market_capture_complete"], false);
    assert_eq!(summary["actual_observation_seconds"], 120.0);
    assert_eq!(
        summary["full_slot_reconciliation"]["error"],
        "VERIFICATION_DEFERRED_AFTER_ABORT"
    );
    assert_no_rpc(&rpc);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn p4_timer_finalizes_locally_without_verification_rpc() {
    let root = std::env::temp_dir().join(format!(
        "p4-off-{}-{}",
        std::process::id(),
        Clock::new().unix_ns(0)
    ));
    let (rpc, rpc_url) = idle_rpc();
    let cfg = MarketConfig::from_lookup(
        &env_of(&[
            ("P0_WS_URL", "ws://127.0.0.1:1"),
            ("P0_RPC_URL", rpc_url.as_str()),
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
        finished_unix_s: 1_700_000_100,
        process_started_unix_s: 0,
        ready_unix_s: Some(0),
        window: Some((0, 18_000_000_000_000)),
        capture_end_ns: 18_000_000_000_000,
        observation_end_unix_s: 18_000,
        stop_reason: "TIMER",
        drain_complete: true,
        write_error: None,
    };
    let (summary, _, error) = finish_capture(&cfg, &mut core, &mut sink, outcome).unwrap();
    let summary = view(&summary);
    assert!(error.is_none());
    assert_eq!(summary["market_capture_complete"], false);
    assert_eq!(summary["actual_observation_seconds"], 18_000.0);
    assert_eq!(
        summary["full_slot_reconciliation"]["error"],
        "VERIFICATION_NOT_RUN"
    );
    assert_no_rpc(&rpc);
    fs::remove_dir_all(root).unwrap();
}

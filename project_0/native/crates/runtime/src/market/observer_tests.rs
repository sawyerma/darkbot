//! Tests of the C0-MO market observer (split from market_observer.rs, structure iteration 29).
include!("market_gap_tests.rs");
include!("market_reconcile_tests.rs");
include!("market_truth_tests.rs");
use super::*;
use darkbot_solana::ingress::{SlotStatus, SlotUpdate, SourceKind, TxVersion};

#[derive(Default)]
struct VecSink {
    tx: Vec<Value>,
    liq: Vec<Value>,
    cov: Vec<Value>,
}

impl MarketSink for VecSink {
    fn transaction(&mut self, row: &Value) -> std::io::Result<()> {
        self.tx.push(row.clone());
        Ok(())
    }
    fn liquidation(&mut self, row: &Value) -> std::io::Result<()> {
        self.liq.push(row.clone());
        Ok(())
    }
    fn coverage(&mut self, row: &Value) -> std::io::Result<()> {
        self.cov.push(row.clone());
        Ok(())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn program() -> Pubkey {
    Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET).unwrap()
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn data_line(target: Pubkey, receiver: Pubkey, seized: f64, repaid: f64, fee: u32) -> String {
    let mut b = LIQUIDATION_RECEIVER_EVENT_DISCRIMINATOR.to_vec();
    b.extend_from_slice(target.as_ref());
    b.extend_from_slice(receiver.as_ref());
    b.extend_from_slice(&seized.to_le_bytes());
    b.extend_from_slice(&repaid.to_le_bytes());
    b.extend_from_slice(&fee.to_le_bytes());
    format!("{PROGRAM_DATA_PREFIX}{}", b64(&b))
}

fn event_line() -> String {
    data_line(Pubkey::new_unique(), Pubkey::new_unique(), 2.0, 1.5, 1)
}

fn logs_with(p: &Pubkey, data: &[String]) -> Vec<String> {
    let mut v = vec![format!("Program {p} invoke [1]")];
    v.extend(data.iter().cloned());
    v.push(format!("Program {p} consumed 1000 of 200000 compute units"));
    v.push(format!("Program {p} success"));
    v
}

fn tx_event(
    sig: &str,
    slot: u64,
    logs: &[String],
    err: Option<&str>,
    t_rx_ns: u64,
) -> TransactionObserved {
    let meta_err = err
        .map(|e| json!({"InstructionError": [0, e]}))
        .unwrap_or(Value::Null);
    TransactionObserved {
        signature: sig.into(),
        slot,
        tx_index_in_slot: None,
        version: TxVersion::V0,
        err: err.map(str::to_string),
        source: SourceKind::WsRpcControl,
        t_rx_ns,
        raw: json!({"signature": sig, "slot": slot, "transaction": {
            "transaction": {"signatures": [sig], "message": {"accountKeys": [], "instructions": []}},
            "meta": {"err": meta_err, "fee": 5000, "logMessages": logs,
                     "preBalances": [], "postBalances": [], "preTokenBalances": [], "postTokenBalances": []},
            "version": 0}}),
    }
}

fn logs_event(
    sig: &str,
    slot: u64,
    logs: Vec<String>,
    err: Option<&str>,
    t_rx_ns: u64,
) -> LogsObserved {
    LogsObserved {
        signature: sig.into(),
        slot,
        err: err.map(str::to_string),
        logs: logs.clone(),
        source: SourceKind::WsRpcControl,
        t_rx_ns,
        raw: json!({"context": {"slot": slot}, "value": {"signature": sig, "err": null, "logs": logs}}),
    }
}

fn ctx(ready: bool, full: bool, secs: f64) -> SummaryContext {
    SummaryContext {
        run_id: "market_test".into(),
        commitment: "finalized".into(),
        stream_ready: ready,
        full_duration_observed: full,
        drain_complete: true,
        actual_observation_seconds: secs,
        configured_duration_seconds: 14_400.0,
        ..SummaryContext::default()
    }
}

fn open_core(source: MarketSource, enrich: EnrichConfig) -> ObserverCore {
    let mut core = ObserverCore::new(program(), source, enrich);
    core.start_observation(0, u64::MAX);
    core
}

// ---------------------------------------------------------------- decoder

#[test]
fn decodes_valid_receiver_event() {
    let (p, t, r) = (program(), Pubkey::new_unique(), Pubkey::new_unique());
    let d = decode_receiver_events(&logs_with(&p, &[data_line(t, r, 2.5, 2.0, 1_000_000)]), &p);
    assert_eq!(d.events.len(), 1);
    let e = &d.events[0];
    assert_eq!(
        (
            e.event_index,
            e.target,
            e.receiver,
            e.protocol_flat_fee_lamports
        ),
        (0, t, r, 1_000_000)
    );
    assert!((e.gross_edge_usd() - 0.5).abs() < 1e-12);
    assert_eq!(d.decoder_errors, 0);
}

#[test]
fn wrong_discriminator_is_not_a_liquidation_event() {
    let p = program();
    let line = format!("{PROGRAM_DATA_PREFIX}{}", b64(&[7u8; 92]));
    let d = decode_receiver_events(&logs_with(&p, &[line]), &p);
    assert!(d.events.is_empty());
    assert_eq!(d.decoder_errors, 0);
}

#[test]
fn short_non_finite_or_undecodable_marginfi_data_is_a_decoder_error() {
    let p = program();
    let mut short = LIQUIDATION_RECEIVER_EVENT_DISCRIMINATOR.to_vec();
    short.extend_from_slice(&[1u8; 40]);
    let short = format!("{PROGRAM_DATA_PREFIX}{}", b64(&short));
    let nan = data_line(Pubkey::new_unique(), Pubkey::new_unique(), f64::NAN, 1.0, 0);
    let bad_b64 = format!("{PROGRAM_DATA_PREFIX}!!!not-base64!!!");
    let d = decode_receiver_events(&logs_with(&p, &[short, nan, bad_b64]), &p);
    assert!(d.events.is_empty());
    assert_eq!((d.decoder_errors, d.base64_errors), (3, 0));
}

#[test]
fn data_from_another_program_is_never_counted() {
    let p = program();
    let other = Pubkey::new_unique();
    let logs = vec![
        format!("Program {p} invoke [1]"),
        format!("Program {other} invoke [2]"),
        data_line(Pubkey::new_unique(), Pubkey::new_unique(), 100.0, 1.0, 0),
        format!("{PROGRAM_DATA_PREFIX}!!!"),
        format!("Program {other} success"),
        format!("Program {p} success"),
    ];
    let d = decode_receiver_events(&logs, &p);
    assert!(d.events.is_empty());
    assert_eq!(
        (d.foreign_program_events, d.base64_errors, d.decoder_errors),
        (1, 1, 0)
    );
}

#[test]
fn truncated_logs_are_flagged() {
    let p = program();
    let mut logs = logs_with(&p, &[]);
    logs.push("Log truncated".into());
    assert!(decode_receiver_events(&logs, &p).truncated);
}

// ---------------------------------------------------------------- observer

#[test]
fn failed_transaction_is_recorded_but_not_a_liquidation() {
    let p = program();
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    core.on_transaction(
        &tx_event("f1", 10, &logs_with(&p, &[event_line()]), Some("Custom"), 1),
        0,
        &mut sink,
    )
    .unwrap();
    assert_eq!(sink.tx.len(), 1);
    assert_eq!(sink.tx[0]["success"], false);
    assert!(sink.liq.is_empty());
    assert_eq!(
        (
            core.stats.failed_transactions,
            core.stats.liquidation_events
        ),
        (1, 0)
    );
    assert_eq!(core.stats.failed_tx_receiver_events_ignored, 1);
}

#[test]
fn duplicate_signature_counts_once() {
    let p = program();
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    let ev = tx_event("dup", 10, &logs_with(&p, &[event_line()]), None, 1);
    core.on_transaction(&ev, 0, &mut sink).unwrap();
    core.on_transaction(&ev, 0, &mut sink).unwrap();
    let s = &core.stats;
    assert_eq!(
        (
            s.transactions_received,
            s.unique_transactions,
            s.duplicate_transactions
        ),
        (2, 1, 1)
    );
    assert_eq!(
        (s.liquidation_events, sink.tx.len(), sink.liq.len()),
        (1, 1, 1)
    );
}

#[test]
fn two_events_in_one_transaction_keep_distinct_indices() {
    let p = program();
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    core.on_transaction(
        &tx_event(
            "two",
            10,
            &logs_with(&p, &[event_line(), event_line()]),
            None,
            1,
        ),
        0,
        &mut sink,
    )
    .unwrap();
    assert_eq!(sink.liq.len(), 2);
    assert_eq!(
        (
            sink.liq[0]["event_index"].as_u64(),
            sink.liq[1]["event_index"].as_u64()
        ),
        (Some(0), Some(1))
    );
    assert_eq!(
        (
            core.stats.liquidation_transactions,
            core.stats.liquidation_events
        ),
        (1, 2)
    );
    assert_eq!(core.stats.tx_fee_total_lamports, 5_000); // once per transaction
    assert_eq!(
        (
            core.stats.enrichment_requested,
            core.stats.enrichment_succeeded
        ),
        (1, 1)
    );
}

#[test]
fn v0_loaded_addresses_resolve_receiver_and_lookups_without_loaded_are_unknown() {
    let (payer, receiver, cb) = (
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        COMPUTE_BUDGET_PROGRAM,
    );
    let limit = bs58::encode([2u8, 0x40, 0x42, 0x0f, 0x00]).into_string();
    let price = bs58::encode([3u8, 7, 0, 0, 0, 0, 0, 0, 0]).into_string();
    let tx = json!({"version": 0,
        "transaction": {"signatures": ["s"], "message": {
            "accountKeys": [payer.to_string(), cb],
            "addressTableLookups": [{"accountKey": Pubkey::new_unique().to_string(), "writableIndexes": [0], "readonlyIndexes": []}],
            "instructions": [{"programIdIndex": 1, "accounts": [], "data": limit},
                             {"programIdIndex": 1, "accounts": [], "data": price}]}},
        "meta": {"err": null, "fee": 12_000,
            "loadedAddresses": {"writable": [receiver.to_string()], "readonly": [Pubkey::new_unique().to_string()]},
            "preBalances": [10, 1, 100, 5], "postBalances": [9, 1, 150, 5],
            "preTokenBalances": [{"accountIndex": 3, "mint": "M1", "owner": receiver.to_string(), "uiTokenAmount": {"amount": "10", "decimals": 6}}],
            "postTokenBalances": [{"accountIndex": 3, "mint": "M1", "owner": receiver.to_string(), "uiTokenAmount": {"amount": "35", "decimals": 6}},
                                  {"accountIndex": 4, "mint": "M2", "owner": receiver.to_string(), "uiTokenAmount": {"amount": "4", "decimals": 9}}]}});
    let e = enrich(&tx, &receiver);
    assert_eq!(e.receiver_lamport_delta, Some(50));
    assert_eq!(
        (e.tx_fee_lamports, e.fee_payer.as_deref()),
        (Some(12_000), Some(payer.to_string().as_str()))
    );
    assert_eq!(
        (e.cu_limit, e.cu_price_micro_lamports),
        (Some(1_000_000), Some(7))
    );
    assert_eq!(
        e.receiver_token_deltas,
        vec![
            TokenDelta {
                mint: "M1".into(),
                native_delta: 25,
                decimals: Some(6)
            },
            TokenDelta {
                mint: "M2".into(),
                native_delta: 4,
                decimals: Some(9)
            },
        ]
    );
    let mut no_loaded = tx.clone();
    no_loaded["meta"]
        .as_object_mut()
        .unwrap()
        .remove("loadedAddresses");
    let e = enrich(&no_loaded, &receiver);
    assert_eq!(
        (e.receiver_lamport_delta, e.fee_payer.clone()),
        (None, None)
    );
}

// [R2-2] Four-hour cutoff on the monotonic ingress clock, after the timer fired.
#[test]
fn cutoff_counts_queued_events_received_before_and_rejects_events_after() {
    let p = program();
    let mut core = ObserverCore::new(p, MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    core.start_observation(1_000, 2_000);
    // Timer has fired; these were still queued and are drained now.
    core.on_transaction(
        &tx_event("A", 10, &logs_with(&p, &[event_line()]), None, 1_999),
        0,
        &mut sink,
    )
    .unwrap();
    core.on_transaction(
        &tx_event("A2", 10, &logs_with(&p, &[]), None, 2_000),
        0,
        &mut sink,
    )
    .unwrap();
    core.on_transaction(
        &tx_event("B", 11, &logs_with(&p, &[event_line()]), None, 2_001),
        0,
        &mut sink,
    )
    .unwrap();
    assert_eq!(
        (
            core.stats.unique_transactions,
            core.stats.liquidation_events
        ),
        (2, 1)
    );
    assert_eq!(core.stats.events_after_cutoff, 1);
    // Coverage events after the cutoff do not taint the window either.
    core.coverage(&mut sink, "GAP", 0, Some(2_500), json!({}))
        .unwrap();
    assert!(core.coverage_complete());
    core.coverage(&mut sink, "GAP", 0, Some(1_500), json!({}))
        .unwrap();
    assert!(!core.coverage_complete());
}

#[test]
fn gap_before_stream_ready_is_recorded_but_not_counted() {
    let mut core = ObserverCore::new(program(), MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    core.coverage(&mut sink, "GAP", 0, Some(5), json!({}))
        .unwrap();
    assert!(core.coverage_complete());
    assert_eq!(sink.cov[0]["counted"], false);
}

#[test]
fn ingress_overflow_makes_data_incomplete() {
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    core.coverage(
        &mut sink,
        "INGRESS_OVERFLOW",
        1,
        None,
        json!({"overflow_total": 1}),
    )
    .unwrap();
    let s = core.summary(&ctx(true, true, 60.0));
    assert_eq!(
        (s["coverage_complete"].as_bool(), s["verdict"].as_str()),
        (Some(false), Some("MARKET_DATA_INCOMPLETE"))
    );
}

// [R2-4] A corrupted receiver event in a successful transaction is never VALID.
#[test]
fn decoder_error_in_successful_transaction_blocks_valid() {
    let p = program();
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    let mut short = LIQUIDATION_RECEIVER_EVENT_DISCRIMINATOR.to_vec();
    short.extend_from_slice(&[0u8; 10]);
    let line = format!("{PROGRAM_DATA_PREFIX}{}", b64(&short));
    core.on_transaction(
        &tx_event(
            "bad",
            10,
            &logs_with(&p, std::slice::from_ref(&line)),
            None,
            1,
        ),
        0,
        &mut sink,
    )
    .unwrap();
    assert!(!core.decode_complete());
    let s = core.summary(&ctx(true, true, 3_600.0));
    assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    assert_eq!(s["decoder_errors_successful_tx"].as_u64(), Some(1));
    // The same corruption in a failed transaction is irrelevant for completeness.
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    core.on_transaction(
        &tx_event("badf", 10, &logs_with(&p, &[line]), Some("Custom"), 1),
        0,
        &mut sink,
    )
    .unwrap();
    assert!(core.decode_complete());
}

// [R2-7] logs source can never claim unqualified MARKET_DATA_VALID.
#[test]
fn verdicts_by_source_and_without_liquidations() {
    let tx = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT)
        .summary(&ctx(true, true, 14_400.0));
    assert_eq!(tx["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    assert_eq!(tx["source_coverage_verified"].as_bool(), Some(false));
    assert_eq!(
        tx["receivership_transport_capture_complete"].as_bool(),
        Some(true)
    );
    assert!(tx["gross_edge_median_usd"].is_null() && tx["top_receiver"].is_null());
    assert_eq!(tx["liquidations_per_hour"].as_f64(), Some(0.0));
    let logs =
        open_core(MarketSource::Logs, EnrichConfig::DEFAULT).summary(&ctx(true, true, 14_400.0));
    assert_eq!(logs["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    assert_eq!(logs["source_coverage_verified"].as_bool(), Some(false));
    assert!(logs["source_coverage_note"]
        .as_str()
        .unwrap()
        .contains("lookup table"));
    let early = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT)
        .summary(&ctx(true, false, 0.0));
    assert!(early["liquidations_per_hour"].is_null());
    assert_eq!(early["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    let mut undrained = ctx(true, true, 14_400.0);
    undrained.drain_complete = false;
    let s = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT).summary(&undrained);
    assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    assert!(s["incomplete_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r == "DRAIN_INCOMPLETE"));
    let not_ready = ObserverCore::new(program(), MarketSource::Logs, EnrichConfig::DEFAULT)
        .summary(&ctx(false, false, 0.0));
    assert_eq!(
        not_ready["verdict"].as_str(),
        Some("MARKET_OBSERVER_STREAM_NOT_READY")
    );
}

// [R2-8] Slot telemetry frames document stream liveness inside the window only.
#[test]
fn slot_telemetry_frames_measure_stream_silence() {
    let mut core = ObserverCore::new(program(), MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    core.start_observation(0, 10_000_000_000);
    let slot = |t: u64| {
        IngressEvent::Slot(SlotUpdate {
            slot: 1,
            parent: None,
            status: SlotStatus::Processed,
            source: SourceKind::WsRpcControl,
            t_rx_ns: t,
        })
    };
    for t in [100_000_000, 500_000_000, 2_500_000_000, 20_000_000_000] {
        core.on_ingress_event(slot(t), 0, &mut sink).unwrap();
    }
    assert_eq!(
        (
            core.stats.slot_notifications,
            core.stats.max_stream_silence_ms
        ),
        (3, 2_000)
    );
}

#[test]
fn events_before_observation_are_not_counted() {
    let p = program();
    let mut core = ObserverCore::new(p, MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    core.on_transaction(
        &tx_event("early", 1, &logs_with(&p, &[]), None, 1),
        0,
        &mut sink,
    )
    .unwrap();
    assert_eq!(
        (
            core.stats.transactions_received,
            core.stats.events_outside_observation
        ),
        (0, 1)
    );
    assert!(sink.tx.is_empty());
}

// ---------------------------------------------------------------- [R2-1/R2-9] deferred enrichment

#[test]
fn logs_capture_never_fetches_and_enrichment_runs_after_capture() {
    let p = program();
    let all = fixtures();
    let fx = &all[0];
    let real: Value = serde_json::from_str(fx.raw).unwrap();
    let real_logs: Vec<String> = real["meta"]["logMessages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_string())
        .collect();
    let cfg = EnrichConfig {
        max_pending: 10,
        max_attempts: 3,
        retry_ms: 1,
        timeout_ms: 1,
        phase_max_secs: 1,
    };
    let mut core = open_core(MarketSource::Logs, cfg);
    let mut sink = VecSink::default();
    // CAPTURE: no fetch function exists on this path; gross edge counted immediately.
    core.on_logs(
        &logs_event(fx.signature, fx.slot, real_logs, None, 1),
        0,
        &mut sink,
    )
    .unwrap();
    core.on_logs(
        &logs_event("other", 7, logs_with(&p, &[event_line()]), None, 2),
        0,
        &mut sink,
    )
    .unwrap();
    assert_eq!(core.stats.liquidation_events, 2);
    assert!(sink.liq.is_empty());
    assert_eq!(core.stats.rpc_fallback_count, 0);
    // ENRICHMENT after capture.
    let (mut calls, mut waits) = (0u32, 0u32);
    core.enrich_deferred(
        &mut sink,
        &mut |sig: &str| {
            calls += 1;
            if sig == fx.signature {
                Ok(Some(real.clone()))
            } else {
                Err("http 429".into())
            }
        },
        &mut |_d: Duration| waits += 1,
        &mut || true,
    )
    .unwrap();
    assert_eq!((calls, waits), (1 + 3, 2));
    assert_eq!(sink.liq.len(), 2);
    assert_eq!(sink.liq[0]["enrichment_status"], "RPC_GET_TRANSACTION");
    assert_eq!(sink.liq[0]["tx_fee_lamports"].as_u64(), Some(fx.fee));
    assert_eq!(sink.liq[1]["enrichment_status"], "UNRESOLVED");
    let s = core.summary(&ctx(true, true, 3_600.0));
    assert_eq!(
        (
            s["enrichment_requested"].as_u64(),
            s["enrichment_succeeded"].as_u64(),
            s["enrichment_failed"].as_u64()
        ),
        (Some(2), Some(1), Some(1))
    );
    assert_eq!(
        (
            s["enrichment_complete"].as_bool(),
            s["fee_totals_complete"].as_bool()
        ),
        (Some(false), Some(false))
    );
    assert_eq!(s["liquidation_events"].as_u64(), Some(2)); // enrichment never removes gross edge
    assert_eq!(
        s["receivership_transport_capture_complete"].as_bool(),
        Some(true)
    );
}

#[test]
fn enrichment_budget_identity_and_queue_limits_are_explicit() {
    let p = program();
    let cfg = EnrichConfig {
        max_pending: 1,
        max_attempts: 2,
        retry_ms: 1,
        timeout_ms: 1,
        phase_max_secs: 1,
    };
    let mut core = open_core(MarketSource::Logs, cfg);
    let mut sink = VecSink::default();
    core.on_logs(
        &logs_event("a", 5, logs_with(&p, &[event_line()]), None, 1),
        0,
        &mut sink,
    )
    .unwrap();
    core.on_logs(
        &logs_event("b", 6, logs_with(&p, &[event_line()]), None, 2),
        0,
        &mut sink,
    )
    .unwrap();
    assert_eq!(
        sink.liq.last().unwrap()["enrichment_status"],
        "SKIPPED_QUEUE_FULL"
    );
    // wrong transaction returned -> identity mismatch, no retry
    let mut calls = 0;
    core.enrich_deferred(
        &mut sink,
        &mut |_s: &str| {
            calls += 1;
            Ok(Some(
                json!({"slot": 5, "transaction": {"signatures": ["zzz"]}}),
            ))
        },
        &mut |_d: Duration| {},
        &mut || true,
    )
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(
        sink.liq.last().unwrap()["enrichment_status"],
        "IDENTITY_MISMATCH"
    );
    // phase budget exhausted
    core.on_logs(
        &logs_event("c", 7, logs_with(&p, &[event_line()]), None, 3),
        0,
        &mut sink,
    )
    .unwrap();
    core.enrich_deferred(
        &mut sink,
        &mut |_s: &str| Ok(None),
        &mut |_d: Duration| {},
        &mut || false,
    )
    .unwrap();
    assert_eq!(
        sink.liq.last().unwrap()["enrichment_status"],
        "SKIPPED_PHASE_TIMEOUT"
    );
    assert_eq!(
        (
            core.stats.enrichment_requested,
            core.stats.enrichment_failed
        ),
        (3, 3)
    );
    assert_eq!(core.stats.liquidation_events, 3);
}

#[test]
fn finalize_writes_summary_report_and_unenriched_rows() {
    let dir = std::env::temp_dir().join(format!(
        "c0mo_finalize_{}_{}",
        std::process::id(),
        utc_compact(unix_now_s())
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let p = program();
    let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut sink = JsonlSink::create(&dir).unwrap();
    core.on_logs(
        &logs_event(
            "x",
            9,
            logs_with(
                &p,
                &[data_line(
                    Pubkey::new_unique(),
                    Pubkey::new_unique(),
                    3.0,
                    1.0,
                    1,
                )],
            ),
            None,
            1,
        ),
        0,
        &mut sink,
    )
    .unwrap();
    core.stop_observation();
    let (summary, report) = finalize(
        &mut core,
        &mut sink,
        &ctx(true, true, 600.0),
        &dir,
        0,
        json!({"reason": "TIMER"}),
    )
    .unwrap();
    assert_eq!(summary["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    assert_eq!(summary["commitment"].as_str(), Some("finalized"));
    assert!(report.contains("verdict=MARKET_DATA_INCOMPLETE"));
    assert!(fs::read_to_string(dir.join("liquidations.jsonl"))
        .unwrap()
        .contains("NOT_ENRICHED"));
    assert_eq!(
        fs::read_to_string(dir.join("transactions.jsonl"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(fs::read_to_string(dir.join("coverage.jsonl"))
        .unwrap()
        .contains("STOPPED"));
    let parsed: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("summary.json")).unwrap()).unwrap();
    assert_eq!(parsed["liquidation_events"].as_u64(), Some(1));
    assert!(dir.join("final_report.txt").exists());
    assert!(JsonlSink::create(&dir).is_err()); // never overwrites a run directory
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- config / safety

#[test]
fn market_config_needs_no_keypair_and_validates_source_and_commitment() {
    let env = |pairs: &[(&str, &str)]| {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k: &str| m.get(k).cloned()
    };
    let base = [
        ("P0_WS_URL", "wss://h/?api-key=x"),
        (
            "P0_PROGRAM_ID",
            darkbot_project0::protocol::PROGRAM_ID_MAINNET,
        ),
    ];
    let c = MarketConfig::from_lookup(&env(&base), 0).unwrap();
    assert_eq!(
        (c.source, c.commitment, c.duration_secs),
        (MarketSource::Transactions, "finalized", 14_400)
    );
    assert_eq!(c.min_sample_liquidations, DEFAULT_MIN_SAMPLE_LIQUIDATIONS);
    assert_eq!(
        (c.rpc_url.clone(), c.drain_timeout_secs),
        (None, DEFAULT_DRAIN_TIMEOUT_SECS)
    );
    assert_eq!(c.run_id, "market_19700101T000000Z");
    let confirmed = MarketConfig::from_lookup(
        &env(&[base[0], base[1], ("P0_MARKET_COMMITMENT", "confirmed")]),
        0,
    )
    .unwrap();
    assert_eq!(confirmed.commitment, "confirmed");
    assert!(MarketConfig::from_lookup(
        &env(&[base[0], base[1], ("P0_MARKET_COMMITMENT", "processed")]),
        0
    )
    .is_err());
    // [R4-A1] logs without RPC is a valid configuration: enrichment unavailable, report still produced.
    let logs_no_rpc =
        MarketConfig::from_lookup(&env(&[base[0], base[1], ("P0_MARKET_SOURCE", "logs")]), 0)
            .unwrap();
    assert_eq!(
        (logs_no_rpc.source, logs_no_rpc.rpc_url.clone()),
        (MarketSource::Logs, None)
    );
    assert!(MarketConfig::from_lookup(&env(&[base[1]]), 0).is_err()); // neither P0_WS_URL nor HELIUS_API_KEY
    let logs = [
        base[0],
        base[1],
        ("P0_MARKET_SOURCE", "logs"),
        ("P0_RPC_URL", "https://r"),
        ("P0_MARKET_DURATION_SECS", "600"),
    ];
    let c = MarketConfig::from_lookup(&env(&logs), 0).unwrap();
    assert_eq!((c.source, c.duration_secs), (MarketSource::Logs, 600));
    assert!(MarketConfig::from_lookup(
        &env(&[base[0], base[1], ("P0_MARKET_SOURCE", "blocks")]),
        0
    )
    .is_err());
    assert!(MarketConfig::from_lookup(
        &env(&[base[0], base[1], ("P0_MARKET_DURATION_SECS", "0")]),
        0
    )
    .is_err());
    assert!(
        MarketConfig::from_lookup(&env(&[base[0], base[1], ("P0_MARKET_RUN_ID", "../x")]), 0)
            .is_err()
    );
    assert_eq!(redact_url("wss://h/?api-key=secret"), "wss://h");
    assert_eq!(
        redact_url("https://private:secret@host/PRIVATE_KEY?q=SECRET"),
        "https://host"
    );
}

#[test]
fn market_mode_has_no_signing_simulation_or_submission_path() {
    let src = concat!(
        include_str!("../market/observer.rs"),
        include_str!("../market/configuration.rs"),
        include_str!("../market/time.rs"),
        include_str!("../market/decoding.rs"),
        include_str!("../market/enrichment.rs"),
        include_str!("../market/sink.rs"),
        include_str!("../market/core.rs"),
        include_str!("../market/summary.rs"),
        include_str!("../analytics/market_report.rs"),
        include_str!("../market/deployment.rs"),
        include_str!("../market/fetch.rs"),
        include_str!("../market/capture.rs"),
    );
    for forbidden in [
        concat!("Key", "pair"),
        concat!("Sig", "ner"),
        concat!("Rpc", "Submitter"),
        concat!("Jito", "Submitter"),
        concat!("send_", "transaction"),
        concat!("sendTrans", "action"),
        concat!("simulate", "Transaction"),
        concat!("simulate_", "with"),
        concat!("Blockhash", "Cache"),
        concat!("Crate", "Health"),
        concat!("P0Receivership", "Builder"),
        concat!("crate::", "pipeline"),
    ] {
        assert!(
            !src.contains(forbidden),
            "market observer must not reference {forbidden}"
        );
    }
    let main = concat!(
        include_str!("../orchestration/startup.rs"),
        include_str!("../orchestration/state_owner.rs"),
        include_str!("../orchestration/state_owner/setup.rs")
    );
    let branch = main
        .find("market_observer::run_from_env")
        .expect("market branch present");
    let run_fn = main.find("fn run()").expect("run()");
    let rpc_url = main
        .find(concat!("config::require(\"P0_", "RPC_URL\")"))
        .expect("rpc url read");
    assert!(run_fn < branch && branch < rpc_url);
}

// [R3-1] No unqualified source can produce MARKET_DATA_VALID.
#[test]
fn unverified_sources_never_yield_market_data_valid() {
    for source in [MarketSource::Transactions, MarketSource::Logs] {
        assert!(!source.coverage_verified());
        let s = open_core(source, EnrichConfig::DEFAULT).summary(&ctx(true, true, 14_400.0));
        assert_eq!(
            s["receivership_transport_capture_complete"].as_bool(),
            Some(true)
        );
        assert_ne!(s["verdict"].as_str(), Some("MARKET_DATA_VALID"));
        assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    }
    assert_eq!(market_verdict(true, true, true), "MARKET_DATA_VALID");
    assert_eq!(
        market_verdict(true, true, false),
        "MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED"
    );
    assert_eq!(market_verdict(true, false, true), "MARKET_DATA_INCOMPLETE");
    assert_eq!(
        market_verdict(false, true, true),
        "MARKET_OBSERVER_STREAM_NOT_READY"
    );
}

// [R3-2] Market config enables the transport heartbeat; Shadow defaults stay untouched.
#[test]
fn market_ws_config_enables_heartbeat_and_shadow_default_does_not() {
    let mut cfg = MarketConfig::from_lookup(
        &|k: &str| match k {
            "P0_WS_URL" => Some("wss://h/?api-key=x".to_string()),
            "P0_PROGRAM_ID" => Some(darkbot_project0::protocol::PROGRAM_ID_MAINNET.to_string()),
            _ => None,
        },
        0,
    )
    .unwrap();
    let ws = market_ws_config(&cfg).unwrap();
    assert_eq!(
        ws.heartbeat_interval,
        Some(Duration::from_secs(DEFAULT_HEARTBEAT_SECS))
    );
    assert_eq!(ws.commitment, "finalized");
    assert!(ws.subscription.slots);
    assert_eq!(ws.subscription.transactions_mentioning, vec![cfg.program]);
    assert!(ws.subscription.logs_mentioning.is_empty());
    let shadow = WsRpcConfig::new("wss://h".into(), Subscription::default());
    assert_eq!(shadow.heartbeat_interval, None);
    assert_eq!(shadow.commitment, "confirmed");
    cfg.heartbeat_secs = shadow.idle_timeout.as_secs();
    assert!(market_ws_config(&cfg).is_err()); // heartbeat must stay below the idle timeout
}

// [R3-3] A failing enrichment client never loses gross edge or the final report.
#[test]
fn enrichment_init_failure_still_produces_summary_and_report() {
    let dir = std::env::temp_dir().join(format!(
        "c0mo_enrich_init_{}_{}",
        std::process::id(),
        utc_compact(unix_now_s())
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let p = program();
    let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut sink = JsonlSink::create(&dir).unwrap();
    core.on_logs(
        &logs_event(
            "x",
            9,
            logs_with(
                &p,
                &[data_line(
                    Pubkey::new_unique(),
                    Pubkey::new_unique(),
                    3.0,
                    1.0,
                    1,
                )],
            ),
            None,
            1,
        ),
        0,
        &mut sink,
    )
    .unwrap();
    core.stop_observation();
    core.enrichment_phase(
        &mut sink,
        Err("enrichment client: tls init failed".into()),
        &mut |_d: Duration| {},
        &mut || false,
        0,
    )
    .unwrap();
    let (summary, report) = finalize(
        &mut core,
        &mut sink,
        &ctx(true, true, 600.0),
        &dir,
        0,
        json!({"reason": "TIMER"}),
    )
    .unwrap();
    assert_eq!(summary["liquidation_events"].as_u64(), Some(1));
    assert!((summary["gross_edge_total_usd"].as_f64().unwrap() - 2.0).abs() < 1e-12);
    assert_eq!(
        (
            summary["enrichment_complete"].as_bool(),
            summary["fee_totals_complete"].as_bool()
        ),
        (Some(false), Some(false))
    );
    assert_eq!(
        summary["enrichment_init_error"].as_str(),
        Some("enrichment client: tls init failed")
    );
    assert_eq!(
        summary["receivership_transport_capture_complete"].as_bool(),
        Some(true)
    );
    assert!(dir.join("summary.json").exists() && dir.join("final_report.txt").exists());
    assert!(report.contains("enrichment_complete=false"));
    assert!(fs::read_to_string(dir.join("liquidations.jsonl"))
        .unwrap()
        .contains("ENRICHMENT_INIT_FAILED"));
    assert!(fs::read_to_string(dir.join("coverage.jsonl"))
        .unwrap()
        .contains("ENRICHMENT_INIT_FAILED"));
    let _ = fs::remove_dir_all(&dir);
}

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let m: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |k: &str| m.get(k).cloned()
}

// [R4-A2]
#[test]
fn transactions_with_helius_key_uses_enhanced_mainnet_endpoint() {
    let pid = darkbot_project0::protocol::PROGRAM_ID_MAINNET;
    let c = MarketConfig::from_lookup(
        &env_of(&[("P0_PROGRAM_ID", pid), ("HELIUS_API_KEY", "K")]),
        0,
    )
    .unwrap();
    assert_eq!(c.ws_url, "wss://atlas-mainnet.helius-rpc.com/?api-key=K");
    assert_eq!(c.rpc_url, None); // transactions needs no enrichment RPC
    let l = MarketConfig::from_lookup(
        &env_of(&[
            ("P0_PROGRAM_ID", pid),
            ("HELIUS_API_KEY", "K"),
            ("P0_MARKET_SOURCE", "logs"),
        ]),
        0,
    )
    .unwrap();
    assert_eq!(l.ws_url, "wss://mainnet.helius-rpc.com/?api-key=K");
    assert_eq!(
        l.rpc_url.as_deref(),
        Some("https://mainnet.helius-rpc.com/?api-key=K")
    );
}

#[test]
fn explicit_ws_url_overrides_helius_derived_url() {
    let pid = darkbot_project0::protocol::PROGRAM_ID_MAINNET;
    let c = MarketConfig::from_lookup(
        &env_of(&[
            ("P0_PROGRAM_ID", pid),
            ("HELIUS_API_KEY", "K"),
            ("P0_WS_URL", "wss://mine/?k=1"),
            ("P0_RPC_URL", "https://mine-rpc"),
        ]),
        0,
    )
    .unwrap();
    assert_eq!(c.ws_url, "wss://mine/?k=1");
    assert_eq!(c.rpc_url.as_deref(), Some("https://mine-rpc"));
}

// [R4-A1] Real entry path: env/config -> production finishing path (no test-only branch).
#[test]
fn logs_without_rpc_url_still_reaches_finalize_and_report() {
    let root = std::env::temp_dir().join(format!(
        "c0mo_norpc_{}_{}",
        std::process::id(),
        utc_compact(unix_now_s())
    ));
    let _ = fs::remove_dir_all(&root);
    let root_s = root.to_string_lossy().to_string();
    let pid = darkbot_project0::protocol::PROGRAM_ID_MAINNET;
    let cfg = MarketConfig::from_lookup(
        &env_of(&[
            ("P0_WS_URL", "wss://h"),
            ("P0_PROGRAM_ID", pid),
            ("P0_MARKET_SOURCE", "logs"),
            ("P0_MARKET_OUT_DIR", root_s.as_str()),
            ("P0_MARKET_RUN_ID", "norpc"),
        ]),
        0,
    )
    .unwrap();
    assert_eq!(cfg.rpc_url, None);
    fs::create_dir_all(&cfg.run_dir).unwrap();
    let mut sink = JsonlSink::create(&cfg.run_dir).unwrap();
    let mut core = ObserverCore::new(cfg.program, cfg.source, cfg.enrich);
    core.start_observation(0, 600_000_000_000);
    core.on_logs(
        &logs_event(
            "x",
            9,
            logs_with(
                &cfg.program,
                &[data_line(
                    Pubkey::new_unique(),
                    Pubkey::new_unique(),
                    3.0,
                    1.0,
                    1,
                )],
            ),
            None,
            1,
        ),
        0,
        &mut sink,
    )
    .unwrap();
    core.stop_observation();
    let outcome = CaptureOutcome {
        process_started_unix_s: 0,
        ready_unix_s: Some(0),
        window: Some((0, 600_000_000_000)),
        capture_end_ns: 600_000_000_000,
        observation_end_unix_s: 600,
        stop_reason: "TIMER",
        drain_complete: true,
        write_error: None,
    };
    let (summary, report, write_error) =
        finish_capture(&cfg, &mut core, &mut sink, outcome).unwrap();
    assert_eq!(write_error, None);
    assert!(
        cfg.run_dir.join("summary.json").exists() && cfg.run_dir.join("final_report.txt").exists()
    );
    assert_eq!(summary["liquidation_events"].as_u64(), Some(1));
    assert!((summary["gross_edge_total_usd"].as_f64().unwrap() - 2.0).abs() < 1e-12);
    assert_eq!(summary["enrichment_available"].as_bool(), Some(false));
    assert_eq!(
        (
            summary["enrichment_complete"].as_bool(),
            summary["fee_totals_complete"].as_bool()
        ),
        (Some(false), Some(false))
    );
    assert!(summary["enrichment_init_error"].is_null());
    assert_eq!(summary["verification_status"], "NOT_RUN");
    assert_eq!(
        summary["full_slot_reconciliation"]["error"],
        "VERIFICATION_NOT_RUN"
    );
    assert_eq!(
        summary["sample_status"].as_str(),
        Some("INSUFFICIENT_SAMPLE")
    );
    assert_eq!(summary["unique_targets"].as_u64(), Some(1));
    assert!((summary["actual_observation_seconds"].as_f64().unwrap() - 600.0).abs() < 1e-9);
    assert!(report.contains("sample_status=INSUFFICIENT_SAMPLE"));
    assert!(!fs::read_to_string(cfg.run_dir.join("coverage.jsonl"))
        .unwrap()
        .contains("ENRICHMENT_INIT_FAILED"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn quantiles_are_nearest_rank() {
    let v: Vec<f64> = (1..=10).map(f64::from).collect();
    assert_eq!(quantile_nearest_rank(&v, 0.5), Some(5.0));
    assert_eq!(quantile_nearest_rank(&v, 0.9), Some(9.0));
    assert_eq!(quantile_nearest_rank(&v, 0.99), Some(10.0));
    assert_eq!(quantile_nearest_rank(&v, 0.0), Some(1.0));
    assert_eq!(quantile_nearest_rank(&[4.2], 0.99), Some(4.2));
    assert_eq!(quantile_nearest_rank(&[], 0.5), None);
    assert_eq!(quantile_nearest_rank(&v, f64::NAN), None);
}

#[test]
fn utc_formatting() {
    assert_eq!(utc_iso(0), "1970-01-01T00:00:00Z");
    assert_eq!(utc_iso(1_700_000_000), "2023-11-14T22:13:20Z");
    assert_eq!(utc_compact(951_782_400), "20000229T000000Z");
}

// ---------------------------------------------------------------- [r6] truncated-log qualification

#[allow(clippy::too_many_arguments)]
fn liq_tx_json(
    sig: &str,
    slot: u64,
    ixs: &[(Pubkey, Pubkey)],
    receiver_in_lut: bool,
    with_loaded: bool,
    logs: Vec<String>,
    failed: bool,
) -> Value {
    fn push(keys: &mut Vec<String>, k: String) -> usize {
        keys.push(k);
        keys.len() - 1
    }
    let mut statics: Vec<String> = vec![Pubkey::new_unique().to_string(), program().to_string()];
    let mut loaded_w: Vec<String> = Vec::new();
    let mut layouts: Vec<(usize, usize, usize, Result<usize, usize>, usize)> = Vec::new();
    for (t, r) in ixs {
        let ti = push(&mut statics, t.to_string());
        let rec = push(&mut statics, Pubkey::new_unique().to_string());
        let grp = push(&mut statics, Pubkey::new_unique().to_string());
        let recv = if receiver_in_lut {
            loaded_w.push(r.to_string());
            Err(loaded_w.len() - 1)
        } else {
            Ok(push(&mut statics, r.to_string()))
        };
        let fee = push(&mut statics, Pubkey::new_unique().to_string());
        layouts.push((ti, rec, grp, recv, fee));
    }
    let n_static = statics.len();
    let data = bs58::encode(darkbot_project0::protocol::ix::END_LIQUIDATION).into_string();
    let instructions: Vec<Value> = layouts
        .iter()
        .map(|(ti, rec, grp, recv, fee)| {
            let ri = match recv {
                Ok(i) => *i,
                Err(pos) => n_static + *pos,
            };
            json!({"programIdIndex": 1, "accounts": [ti, rec, grp, ri, fee], "data": data})
        })
        .collect();
    let lookups = if receiver_in_lut {
        json!([{"accountKey": Pubkey::new_unique().to_string(), "writableIndexes": [0], "readonlyIndexes": []}])
    } else {
        json!([])
    };
    let err = if failed {
        json!({"InstructionError": [0, "Custom"]})
    } else {
        Value::Null
    };
    let mut meta = json!({"err": err, "fee": 5000, "logMessages": logs, "preBalances": [], "postBalances": [],
        "preTokenBalances": [], "postTokenBalances": []});
    if with_loaded {
        meta["loadedAddresses"] = json!({"writable": loaded_w, "readonly": []});
    }
    json!({"slot": slot, "version": 0,
        "transaction": {"signatures": [sig], "message": {"accountKeys": statics, "instructions": instructions, "addressTableLookups": lookups}},
        "meta": meta})
}

/// Helius `transactionNotification.result` shape built from a getTransaction-shaped tx.
fn as_observed(tx: &Value, sig: &str, slot: u64, failed: bool) -> TransactionObserved {
    let mut inner = tx.clone();
    inner.as_object_mut().unwrap().remove("slot");
    TransactionObserved {
        signature: sig.into(),
        slot,
        tx_index_in_slot: None,
        version: TxVersion::V0,
        err: if failed { Some("Custom".into()) } else { None },
        source: SourceKind::WsRpcControl,
        t_rx_ns: 1,
        raw: json!({"signature": sig, "slot": slot, "transaction": inner}),
    }
}

fn truncated_logs(events: &[(Pubkey, Pubkey)]) -> Vec<String> {
    let p = program();
    let mut v = vec![format!("Program {p} invoke [1]")];
    v.extend(events.iter().map(|(t, r)| data_line(*t, *r, 2.0, 1.0, 1)));
    v.push("Log truncated".into());
    v
}

fn observe_truncated(
    ixs: &[(Pubkey, Pubkey)],
    events: &[(Pubkey, Pubkey)],
    lut: bool,
    loaded: bool,
) -> ObserverCore {
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    let tx = liq_tx_json("sigT", 50, ixs, lut, loaded, truncated_logs(events), false);
    core.on_transaction(&as_observed(&tx, "sigT", 50, false), 0, &mut sink)
        .unwrap();
    core
}

#[test]
fn truncated_logs_with_no_end_liquidation_can_be_qualified() {
    let core = observe_truncated(&[], &[], false, false);
    assert_eq!(
        (
            core.stats.logs_truncated,
            core.stats.logs_truncated_qualified,
            core.stats.logs_truncated_unqualified
        ),
        (1, 1, 0)
    );
    assert!(core.decode_complete());
    let s = core.summary(&ctx(true, true, 600.0));
    assert_eq!(
        s["receivership_transport_capture_complete"].as_bool(),
        Some(true)
    );
    assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE")); // coverage stays fail closed
    assert_eq!(s["source_coverage_verified"].as_bool(), Some(false));
    // logs source: deferred until after capture, then qualified against the fetched transaction
    let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    core.on_logs(
        &logs_event("sigL", 51, truncated_logs(&[]), None, 1),
        0,
        &mut sink,
    )
    .unwrap();
    assert!(!core.decode_complete()); // pending qualification is not complete
    let fetched = liq_tx_json("sigL", 51, &[], false, false, truncated_logs(&[]), false);
    core.qualify_truncated_deferred(
        &mut |_s: &str| Ok(Some(fetched.clone())),
        &mut |_d: Duration| {},
        &mut || true,
    );
    assert_eq!(core.stats.logs_truncated_qualified, 1);
    assert!(core.decode_complete());
}

#[test]
fn truncated_logs_with_exact_instruction_event_parity_can_be_qualified() {
    let (t1, r1, t2, r2) = (
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    );
    // multiset parity, independent of order
    let core = observe_truncated(&[(t1, r1), (t2, r2)], &[(t2, r2), (t1, r1)], false, false);
    assert_eq!(core.stats.logs_truncated_qualified, 1);
    assert_eq!(
        (
            core.stats.truncated_instruction_end_liquidations,
            core.stats.truncated_log_end_liquidations
        ),
        (2, 2)
    );
    assert_eq!(core.stats.liquidation_events, 2);
    assert!(core.decode_complete());
}

#[test]
fn truncated_logs_missing_liquidation_event_fail_closed() {
    let (t, r) = (Pubkey::new_unique(), Pubkey::new_unique());
    let core = observe_truncated(&[(t, r)], &[], false, false);
    assert_eq!(core.stats.logs_truncated_liquidation_mismatch, 1);
    assert!(!core.decode_complete());
    let s = core.summary(&ctx(true, true, 600.0));
    assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    assert!(s["incomplete_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x == "TRUNCATED_LIQUIDATION_MISMATCH"));
    assert_eq!(s["liquidation_events"].as_u64(), Some(0)); // nothing invented
}

#[test]
fn truncated_logs_wrong_target_or_receiver_fail_closed() {
    let (t, r) = (Pubkey::new_unique(), Pubkey::new_unique());
    let wrong_receiver = observe_truncated(&[(t, r)], &[(t, Pubkey::new_unique())], false, false);
    assert_eq!(wrong_receiver.stats.logs_truncated_liquidation_mismatch, 1);
    let wrong_target = observe_truncated(&[(t, r)], &[(Pubkey::new_unique(), r)], false, false);
    assert_eq!(wrong_target.stats.logs_truncated_liquidation_mismatch, 1);
    let duplicated = observe_truncated(&[(t, r)], &[(t, r), (t, r)], false, false);
    assert_eq!(duplicated.stats.logs_truncated_liquidation_mismatch, 1); // multiset, not set
    assert!(
        !wrong_receiver.decode_complete()
            && !wrong_target.decode_complete()
            && !duplicated.decode_complete()
    );
}

#[test]
fn truncated_v0_loaded_addresses_are_resolved() {
    let (t, r) = (Pubkey::new_unique(), Pubkey::new_unique());
    let core = observe_truncated(&[(t, r)], &[(t, r)], true, true);
    assert_eq!(
        (
            core.stats.logs_truncated_qualified,
            core.stats.logs_truncated_liquidation_mismatch
        ),
        (1, 0)
    );
    assert!(core.decode_complete());
}

#[test]
fn truncated_missing_loaded_addresses_fail_closed() {
    let (t, r) = (Pubkey::new_unique(), Pubkey::new_unique());
    let core = observe_truncated(&[(t, r)], &[(t, r)], true, false);
    assert_eq!(core.stats.logs_truncated_unqualified, 1);
    assert!(!core.decode_complete());
    let s = core.summary(&ctx(true, true, 600.0));
    assert_eq!(
        s["truncated_unqualified_reasons"]["LOADED_ADDRESSES_MISSING"].as_u64(),
        Some(1)
    );
    assert!(s["incomplete_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x == "LOGS_TRUNCATED_UNQUALIFIED"));
}

#[test]
fn truncated_fetch_failure_or_wrong_transaction_fail_closed() {
    let mut core = open_core(
        MarketSource::Logs,
        EnrichConfig {
            max_attempts: 2,
            ..EnrichConfig::DEFAULT
        },
    );
    let mut sink = VecSink::default();
    core.on_logs(
        &logs_event("a", 1, truncated_logs(&[]), None, 1),
        0,
        &mut sink,
    )
    .unwrap();
    core.on_logs(
        &logs_event("b", 2, truncated_logs(&[]), None, 2),
        0,
        &mut sink,
    )
    .unwrap();
    let other = liq_tx_json("zzz", 2, &[], false, false, vec![], false);
    core.qualify_truncated_deferred(
        &mut |sig: &str| {
            if sig == "a" {
                Err("http 429".into())
            } else {
                Ok(Some(other.clone()))
            }
        },
        &mut |_d: Duration| {},
        &mut || true,
    );
    assert_eq!(core.stats.logs_truncated_unqualified, 2);
    let s = core.summary(&ctx(true, true, 600.0));
    assert_eq!(
        s["truncated_unqualified_reasons"]["FETCH_FAILED"].as_u64(),
        Some(1)
    );
    assert_eq!(
        s["truncated_unqualified_reasons"]["IDENTITY_MISMATCH"].as_u64(),
        Some(1)
    );
    // never qualified before finalize -> fail closed
    let mut late = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    late.on_logs(
        &logs_event("c", 3, truncated_logs(&[]), None, 3),
        0,
        &mut sink,
    )
    .unwrap();
    late.fail_truncated_pending("NO_RPC_FOR_QUALIFICATION");
    assert_eq!(late.stats.logs_truncated_unqualified, 1);
}

#[test]
fn failed_transaction_does_not_create_liquidation() {
    let (t, r) = (Pubkey::new_unique(), Pubkey::new_unique());
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    let tx = liq_tx_json(
        "f",
        9,
        &[(t, r)],
        false,
        false,
        truncated_logs(&[(t, r)]),
        true,
    );
    core.on_transaction(&as_observed(&tx, "f", 9, true), 0, &mut sink)
        .unwrap();
    assert_eq!(
        (core.stats.liquidation_events, core.stats.logs_truncated),
        (0, 0)
    );
    assert!(sink.liq.is_empty());
    assert!(core.decode_complete());
}

#[test]
fn non_truncated_existing_behavior_unchanged() {
    let (t, r) = (Pubkey::new_unique(), Pubkey::new_unique());
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    let logs = logs_with(&program(), &[data_line(t, r, 2.0, 1.0, 1)]);
    let tx = liq_tx_json("n", 7, &[(t, r)], false, false, logs, false);
    core.on_transaction(&as_observed(&tx, "n", 7, false), 0, &mut sink)
        .unwrap();
    assert_eq!(core.stats.liquidation_events, 1);
    assert_eq!(
        (
            core.stats.logs_truncated,
            core.stats.logs_truncated_qualified,
            core.stats.truncated_instruction_end_liquidations
        ),
        (0, 0, 0)
    );
    assert!(core.decode_complete());
    let s = core.summary(&ctx(true, true, 600.0));
    assert_eq!(
        s["receivership_transport_capture_complete"].as_bool(),
        Some(true)
    );
}

// ---------------------------------------------------------------- real audit fixtures (6/6)

struct Fx {
    raw: &'static str,
    signature: &'static str,
    slot: u64,
    gross: f64,
    receiver: &'static str,
    target: &'static str,
    fee: u64,
    cu_limit: Option<u32>,
    lamport_delta: i128,
    token_mints: usize,
}

fn fixtures() -> Vec<Fx> {
    vec![
        Fx { raw: include_str!("../../../../../docs/evidence/AUDIT_20261005/2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf.json"), signature: "2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf", slot: 451703584, gross: 0.186822, receiver: "D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn", target: "3GqwCURBtJZfdQxMSyZ1VyWNSB5WXovg7Q8Q46HY7ihx", fee: 19000, cu_limit: None, lamport_delta: -1019000, token_mints: 2 },
        Fx { raw: include_str!("../../../../../docs/evidence/AUDIT_20261005/38ZXvW6P5sGZmdzj4QbUPSdQnCnJEBHoXTh4vvvfitFPNDHRsgPjELCnFxxCfW2wygE8J6k2Yv8RJJXPUVchBm7F.json"), signature: "38ZXvW6P5sGZmdzj4QbUPSdQnCnJEBHoXTh4vvvfitFPNDHRsgPjELCnFxxCfW2wygE8J6k2Yv8RJJXPUVchBm7F", slot: 451704201, gross: 0.531769, receiver: "D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn", target: "AzQVTAtLxdaQUJzh7xZ7rj956tkeVNk4D4iuK5EVkxDi", fee: 19000, cu_limit: None, lamport_delta: -1019000, token_mints: 2 },
        Fx { raw: include_str!("../../../../../docs/evidence/AUDIT_20261005/3ab7mnuTErWjRtS8iz4Nm619VNvm3FSw47VVvdjYnSLNzB2btCoreDsQqmzfb4Qt5oBA29saPQ98Rgy2nyAUExRB.json"), signature: "3ab7mnuTErWjRtS8iz4Nm619VNvm3FSw47VVvdjYnSLNzB2btCoreDsQqmzfb4Qt5oBA29saPQ98Rgy2nyAUExRB", slot: 451657746, gross: 0.18595, receiver: "TraceR2t18nnwkXgHoB54iPD5N4x2EJZC1uS55ggsH8", target: "6mzmLGPk7CeGCM7dRH8ZZpHsSzfy6aAYf47nbH8HuYy8", fee: 5000, cu_limit: Some(598236), lamport_delta: -1005000, token_mints: 2 },
        Fx { raw: include_str!("../../../../../docs/evidence/AUDIT_20261005/59Qt6h1iPnEgfaDaEi5Y5jVvqHJCs4gzwvHqWW37wVJMHyThYjM6zsS8AmE9bvoxqdUj8AMnNbBuKHQtXysdnwNa.json"), signature: "59Qt6h1iPnEgfaDaEi5Y5jVvqHJCs4gzwvHqWW37wVJMHyThYjM6zsS8AmE9bvoxqdUj8AMnNbBuKHQtXysdnwNa", slot: 453063240, gross: 0.060183, receiver: "qbvAGHo9JpVRsuRem8rBCzLJJNV6ZDNv63qYHkcmNCV", target: "HMjc1LecbCMUqYG3x36gaCzxG5AJ7gJPkPB85fx1pPKK", fee: 5000, cu_limit: Some(1400000), lamport_delta: -1005000, token_mints: 2 },
        Fx { raw: include_str!("../../../../../docs/evidence/AUDIT_20261005/C6JnpzGrv53y9dEExtpDYkzTbexLEu7oikPRNx8sCqf85Jvobc7UBpsaDSgGHzEgeB88HkVFqMkCkvs1ZqVWRwZ.json"), signature: "C6JnpzGrv53y9dEExtpDYkzTbexLEu7oikPRNx8sCqf85Jvobc7UBpsaDSgGHzEgeB88HkVFqMkCkvs1ZqVWRwZ", slot: 451635082, gross: 0.2098, receiver: "TraceR2t18nnwkXgHoB54iPD5N4x2EJZC1uS55ggsH8", target: "6mzmLGPk7CeGCM7dRH8ZZpHsSzfy6aAYf47nbH8HuYy8", fee: 5000, cu_limit: Some(779995), lamport_delta: -1005000, token_mints: 2 },
        Fx { raw: include_str!("../../../../../docs/evidence/AUDIT_20261005/gLVXyfVZmizZqpF9752x3uhgJixW5Jqrp3Agjhhr6cNUnU5aQDBAxQ1CF9RWSpy2oB2K9EAroFJaGPTkz5vkUzB.json"), signature: "gLVXyfVZmizZqpF9752x3uhgJixW5Jqrp3Agjhhr6cNUnU5aQDBAxQ1CF9RWSpy2oB2K9EAroFJaGPTkz5vkUzB", slot: 451643526, gross: 0.1742, receiver: "D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn", target: "6mzmLGPk7CeGCM7dRH8ZZpHsSzfy6aAYf47nbH8HuYy8", fee: 19000, cu_limit: None, lamport_delta: -1019000, token_mints: 2 },
    ]
}

#[test]
fn six_real_audit_liquidations_decode_enrich_and_verify_identity() {
    let p = program();
    for fx in fixtures() {
        let tx: Value = serde_json::from_str(fx.raw).unwrap();
        let logs: Vec<String> = tx["meta"]["logMessages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap().to_string())
            .collect();
        let d = decode_receiver_events(&logs, &p);
        assert_eq!(d.events.len(), 1, "{}", fx.signature);
        assert_eq!(d.decoder_errors, 0);
        let e = &d.events[0];
        assert!(
            (e.gross_edge_usd() - fx.gross).abs() < 1e-6,
            "{} gross {}",
            fx.signature,
            e.gross_edge_usd()
        );
        assert_eq!(e.receiver.to_string(), fx.receiver);
        assert_eq!(e.target.to_string(), fx.target);
        assert_eq!(e.protocol_flat_fee_lamports, 1_000_000);
        let en = enrich(&tx, &e.receiver);
        assert_eq!(en.tx_fee_lamports, Some(fx.fee));
        assert_eq!(en.fee_payer.as_deref(), Some(fx.receiver));
        assert_eq!(en.cu_limit, fx.cu_limit);
        assert_eq!(en.receiver_lamport_delta, Some(fx.lamport_delta));
        assert_eq!(en.receiver_token_deltas.len(), fx.token_mints);
        assert!(en.token_deltas_complete);
        assert_eq!(verify_tx_identity(&tx, fx.signature, fx.slot), Ok(()));
        assert!(verify_tx_identity(&tx, fx.signature, fx.slot + 1).is_err());
    }
}

#[test]
fn six_real_audit_liquidations_aggregate_into_market_summary() {
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    for fx in fixtures() {
        let tx: Value = serde_json::from_str(fx.raw).unwrap();
        let ev = TransactionObserved {
            signature: fx.signature.into(),
            slot: fx.slot,
            tx_index_in_slot: None,
            version: TxVersion::V0,
            err: None,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 1,
            raw: json!({"signature": fx.signature, "slot": fx.slot, "transaction": tx}),
        };
        core.on_transaction(&ev, 0, &mut sink).unwrap();
    }
    let s = core.summary(&ctx(true, true, 3_600.0));
    assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    assert_eq!(
        s["receivership_transport_capture_complete"].as_bool(),
        Some(true)
    );
    assert_eq!(
        (
            s["liquidation_events"].as_u64(),
            s["liquidation_transactions"].as_u64()
        ),
        (Some(6), Some(6))
    );
    let f = |k: &str| s[k].as_f64().unwrap();
    assert!((f("gross_edge_total_usd") - 1.348724).abs() < 1e-5);
    assert!((f("gross_edge_per_hour_usd") - 1.348724).abs() < 1e-5);
    assert!((f("gross_edge_median_usd") - 0.18595).abs() < 1e-6);
    assert!((f("gross_edge_p90_usd") - 0.531769).abs() < 1e-6);
    assert!((f("gross_edge_min_usd") - 0.060183).abs() < 1e-6);
    assert_eq!(
        s["top_receiver"].as_str(),
        Some("D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn")
    );
    assert!((f("top_receiver_liquidation_share") - 0.5).abs() < 1e-12);
    assert!((f("top_receiver_gross_edge_share") - 0.892791 / 1.348724).abs() < 1e-5);
    assert!((f("top3_receiver_liquidation_share") - 1.0).abs() < 1e-12);
    assert_eq!(s["unique_receivers"].as_u64(), Some(3));
    assert_eq!(
        s["protocol_flat_fee_total_lamports"].as_u64(),
        Some(6_000_000)
    );
    assert_eq!(s["tx_fee_total_lamports"].as_u64(), Some(72_000));
    assert_eq!(
        (
            s["enrichment_complete"].as_bool(),
            s["fee_totals_complete"].as_bool()
        ),
        (Some(true), Some(true))
    );
    assert!(sink.liq.iter().all(|r| r["enrichment_status"] == "PAYLOAD"));
}

fn journal_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "c0mo_{name}_{}_{}",
        std::process::id(),
        Pubkey::new_unique()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn journal_sink_views_are_byte_identical_to_the_jsonl_sink() {
    let dir = journal_test_dir("journal_parity");
    let (legacy, binary) = (dir.join("jsonl"), dir.join("journal"));
    fs::create_dir_all(&legacy).unwrap();
    fs::create_dir_all(&binary).unwrap();
    let raw = json!({"value":{"logs":["Program log: \u{e9}\"q\"\n", "x"],"err":null},"meta":{"fee":5000}});
    let rows = [
        (
            0,
            json!({"slot":5,"t_rx_ns":7,"signature":"s1","raw":raw,"tx_version":null,"success":true}),
        ),
        (
            1,
            json!({"slot":5,"seized_equity_usd":12.25,"gross_edge_usd":0.30000000000000004,
            "repaid_equity_usd":1e-7,"receiver_lamport_delta":i128::MAX.to_string(),"cu_limit":u64::MAX}),
        ),
        (
            2,
            json!({"event":"START","timestamp_utc":"2026-10-16T00:00:00Z","t_rx_ns":null,"counted":true}),
        ),
        (
            0,
            json!({"slot":6,"t_rx_ns":8,"signature":"s2","raw":{},"negative":-17,"big":i128::MIN}),
        ),
        (
            2,
            json!({"event":"MARKET_SLOT_BOUNDARY_EVIDENCE","event_extra":{"slot":6}}),
        ),
    ];
    let mut jsonl = JsonlSink::create(&legacy).unwrap();
    let mut journal = JournalSink::create(&binary).unwrap();
    for (kind, row) in &rows {
        for sink in [&mut jsonl as &mut dyn MarketSink, &mut journal] {
            match kind {
                0 => sink.transaction(row),
                1 => sink.liquidation(row),
                _ => sink.coverage(row),
            }
            .unwrap();
        }
    }
    jsonl.flush().unwrap();
    let binding = journal.finish().unwrap().unwrap();
    for (_, name) in MARKET_VIEWS {
        assert_eq!(
            fs::read(legacy.join(name)).unwrap(),
            fs::read(binary.join(name)).unwrap(),
            "{name}"
        );
    }
    assert_eq!(binding["status"], "COMPLETE");
    assert_eq!(binding["event_frames"], 5);
    assert_eq!(binding["views"]["transactions.jsonl"]["rows"], 2);
    assert_eq!(binding["views"]["liquidations.jsonl"]["rows"], 1);
    assert_eq!(binding["views"]["coverage.jsonl"]["rows"], 2);
    let mut reader = crate::analytics::journal::Reader::open(&binary.join(MARKET_JOURNAL)).unwrap();
    let first = reader.next_frame().unwrap().unwrap();
    assert_eq!(
        (first.provenance.slot, first.provenance.rx_ns),
        (Some(5), Some(7))
    );
    while reader.next_frame().unwrap().is_some() {}
    assert!(reader.sealed && reader.complete);
    assert_eq!(
        crate::analytics::journal::hex(&reader.root()),
        binding["root_sha256"]
    );
    // One finish only; an existing journal or view is never overwritten.
    assert!(journal.finish().is_err());
    drop(journal);
    assert!(JournalSink::create(&binary).is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn journal_sink_finalize_binds_views_and_early_exit_still_renders_incomplete() {
    let dir = journal_test_dir("journal_finalize");
    let p = program();
    let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut sink = JournalSink::create(&dir).unwrap();
    core.on_logs(
        &logs_event(
            "x",
            9,
            logs_with(
                &p,
                &[data_line(
                    Pubkey::new_unique(),
                    Pubkey::new_unique(),
                    3.0,
                    1.0,
                    1,
                )],
            ),
            None,
            1,
        ),
        0,
        &mut sink,
    )
    .unwrap();
    core.stop_observation();
    // During the capture only the journal exists; the views are rendered by finalize.
    for (_, name) in MARKET_VIEWS {
        assert!(!dir.join(name).exists(), "{name}");
    }
    let (summary, report) = finalize(
        &mut core,
        &mut sink,
        &ctx(true, true, 600.0),
        &dir,
        0,
        json!({"reason": "TIMER"}),
    )
    .unwrap();
    let binding = &summary["evidence_journal"];
    assert_eq!(binding["journal"], MARKET_JOURNAL);
    assert_eq!(binding["status"], "COMPLETE");
    assert_eq!(binding["views"]["transactions.jsonl"]["rows"], 1);
    assert!(report.contains(MARKET_JOURNAL));
    let on_disk: Value =
        serde_json::from_slice(&fs::read(dir.join("summary.json")).unwrap()).unwrap();
    assert_eq!(&on_disk["evidence_journal"], binding);
    assert!(fs::read_to_string(dir.join("coverage.jsonl"))
        .unwrap()
        .contains("STOPPED"));
    drop(sink);
    // Early exit (for example a failed warmup): sealed INCOMPLETE, views still rendered.
    let early = dir.join("early");
    fs::create_dir_all(&early).unwrap();
    {
        let mut sink = JournalSink::create(&early).unwrap();
        sink.coverage(&json!({"event":"SHADOW_WARMUP_FAILED","error":"X"}))
            .unwrap();
    }
    assert!(fs::read_to_string(early.join("coverage.jsonl"))
        .unwrap()
        .contains("SHADOW_WARMUP_FAILED"));
    let mut reader = crate::analytics::journal::Reader::open(&early.join(MARKET_JOURNAL)).unwrap();
    while reader.next_frame().unwrap().is_some() {}
    assert!(reader.sealed && !reader.complete);
    fs::remove_dir_all(dir).unwrap();
}

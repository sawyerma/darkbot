//! Tests of the C0-MO market observer (split from market_observer.rs, structure iteration 29).
include!("market_gap_tests.rs");
include!("market_reconcile_tests.rs");
include!("market_truth_tests.rs");
include!("report_tests.rs");
include!("summary_golden_tests.rs");
use super::*;
use darkbot_solana::ingress::{SlotStatus, SlotUpdate, SourceKind};

#[derive(Default)]
struct VecSink {
    tx: Vec<Value>,
    liq: Vec<Value>,
    cov: Vec<Value>,
    rel: Vec<related::MarketRelated>,
}

/// Stores the offline legacy render of every typed row: every assertion on a row field is a
/// parity check of the typed capture against the historical JSONL contract.
impl MarketSink for VecSink {
    fn transaction(&mut self, row: &super::rows::MarketTransaction) -> std::io::Result<()> {
        self.tx.push(row.legacy_json());
        Ok(())
    }
    fn liquidation(&mut self, row: &super::rows::MarketLiquidation) -> std::io::Result<()> {
        self.liq.push(row.legacy_json());
        Ok(())
    }
    fn coverage(&mut self, row: &super::rows::MarketCoverage) -> std::io::Result<()> {
        self.cov.push(row.legacy_json());
        Ok(())
    }
    fn related(&mut self, row: &related::MarketRelated) -> std::io::Result<()> {
        self.rel.push(row.clone());
        Ok(())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Render of a typed document (assertions read the historical keys).
fn view<T: serde::Serialize>(doc: &T) -> Value {
    serde_json::to_value(doc).unwrap()
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
    // Real RPC form of a failed instruction (the test label `err` only marks failure).
    let meta_err = err
        .map(|_| json!({"InstructionError": [0, {"Custom": 1}]}))
        .unwrap_or(Value::Null);
    let raw = json!({"signature": sig, "slot": slot, "transaction": {
        "transaction": {"signatures": [sig], "message": {"accountKeys": [], "instructions": []}},
        "meta": {"err": meta_err, "fee": 5000, "logMessages": logs,
                 "preBalances": [], "postBalances": [], "preTokenBalances": [], "postTokenBalances": []},
        "version": 0}});
    TransactionObserved {
        signature: sig.into(),
        slot,
        tx_index_in_slot: None,
        source: SourceKind::WsRpcControl,
        t_rx_ns,
        tx: darkbot_solana::chain::decode_notification(&raw),
    }
}

fn logs_event(
    sig: &str,
    slot: u64,
    logs: Vec<String>,
    err: Option<&str>,
    t_rx_ns: u64,
) -> LogsObserved {
    let status = match err {
        Some(_) => darkbot_solana::chain::TxStatus::Failed(
            solana_sdk::transaction::TransactionError::InstructionError(
                0,
                solana_sdk::instruction::InstructionError::Custom(1),
            ),
        ),
        None => darkbot_solana::chain::TxStatus::Success,
    };
    LogsObserved {
        source: SourceKind::WsRpcControl,
        t_rx_ns,
        logs: darkbot_solana::chain::ChainLogs {
            slot,
            signature: sig.into(),
            status,
            logs: Some(logs),
        },
    }
}

/// The explicit offline export (`p0-journal renders`): legacy views of the sealed journal.
fn render_views(dir: &Path) {
    let views: Vec<(Option<u16>, PathBuf)> = MARKET_VIEWS
        .iter()
        .map(|(k, n)| (Some(*k), dir.join(n)))
        .collect();
    let refs: Vec<(Option<u16>, &Path)> = views.iter().map(|(k, p)| (*k, p.as_path())).collect();
    crate::analytics::journal::export_views(&dir.join(MARKET_JOURNAL), &refs, false).unwrap();
}

fn gap_event() -> super::rows::WindowEvent {
    super::rows::WindowEvent::Gap {
        from_slot: None,
        to_slot: None,
    }
}

fn stopped(reason: &str) -> super::rows::WindowEvent {
    super::rows::WindowEvent::Stopped {
        reason: reason.into(),
        drain_complete: true,
        write_error: None,
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
        tx_event("f1", 10, &logs_with(&p, &[event_line()]), Some("Custom"), 1),
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

/// A1: a notification without `meta` (or with an undecodable payload) is recorded and counted
/// UNKNOWN — never success, never a liquidation — and keeps the capture INCOMPLETE.
#[test]
fn notification_without_status_or_decodable_payload_is_unknown_not_success() {
    let p = program();
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    let mut ev = tx_event("nometa", 10, &logs_with(&p, &[event_line()]), None, 1);
    if let Ok(tx) = &mut ev.tx {
        tx.meta = None;
    }
    core.on_transaction(ev, 0, &mut sink).unwrap();
    let odd = TransactionObserved {
        tx: Err(darkbot_solana::chain::ChainError("NOTIFICATION_DECODE")),
        ..tx_event("odd", 11, &[], None, 2)
    };
    core.on_transaction(odd, 0, &mut sink).unwrap();
    assert_eq!(
        (
            sink.tx[0]["success"].as_bool(),
            sink.tx[1]["success"].as_bool()
        ),
        (Some(false), Some(false))
    );
    assert_eq!(sink.tx[1]["decode_error"], "NOTIFICATION_DECODE");
    assert!(sink.liq.is_empty());
    let s = &core.stats;
    assert_eq!(
        (
            s.unknown_status_transactions,
            s.successful_transactions,
            s.liquidation_events
        ),
        (2, 0, 0)
    );
    let summary = view(&core.summary(&ctx(true, true, 60.0)));
    let reasons = summary["incomplete_reasons"].to_string();
    assert!(reasons.contains("TX_STATUS_UNKNOWN") && reasons.contains("NOTIFICATION_DECODE"));
}

#[test]
fn duplicate_signature_counts_once() {
    let p = program();
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    let ev = tx_event("dup", 10, &logs_with(&p, &[event_line()]), None, 1);
    core.on_transaction(ev.clone(), 0, &mut sink).unwrap();
    core.on_transaction(ev.clone(), 0, &mut sink).unwrap();
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
        tx_event(
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
    let (m1, m2) = (
        Pubkey::new_unique().to_string(),
        Pubkey::new_unique().to_string(),
    );
    let tx = json!({"version": 0,
        "transaction": {"signatures": [solana_sdk::signature::Signature::default().to_string()], "message": {
            "header": {"numRequiredSignatures": 1, "numReadonlySignedAccounts": 0, "numReadonlyUnsignedAccounts": 1},
            "recentBlockhash": Pubkey::new_unique().to_string(),
            "accountKeys": [payer.to_string(), cb],
            "addressTableLookups": [{"accountKey": Pubkey::new_unique().to_string(), "writableIndexes": [0], "readonlyIndexes": []}],
            "instructions": [{"programIdIndex": 1, "accounts": [], "data": limit},
                             {"programIdIndex": 1, "accounts": [], "data": price}]}},
        "meta": {"err": null, "fee": 12_000,
            "loadedAddresses": {"writable": [receiver.to_string()], "readonly": [Pubkey::new_unique().to_string()]},
            "preBalances": [10, 1, 100, 5], "postBalances": [9, 1, 150, 5],
            "preTokenBalances": [{"accountIndex": 3, "mint": m1, "owner": receiver.to_string(), "uiTokenAmount": {"amount": "10", "decimals": 6}}],
            "postTokenBalances": [{"accountIndex": 3, "mint": m1, "owner": receiver.to_string(), "uiTokenAmount": {"amount": "35", "decimals": 6}},
                                  {"accountIndex": 4, "mint": m2, "owner": receiver.to_string(), "uiTokenAmount": {"amount": "4", "decimals": 9}}]}});
    let decode = |v: &Value| darkbot_solana::chain::decode_entry(v, 1, None, None).unwrap();
    let e = enrich(&decode(&tx), &receiver);
    assert_eq!(e.receiver_lamport_delta, Some(50));
    assert_eq!(
        (e.tx_fee_lamports, e.fee_payer.as_deref()),
        (Some(12_000), Some(payer.to_string().as_str()))
    );
    assert_eq!(
        (e.cu_limit, e.cu_price_micro_lamports),
        (Some(1_000_000), Some(7))
    );
    // Per-mint deltas come out ordered by mint (BTreeMap), so order the expectation the same way.
    let mut want = vec![
        TokenDelta {
            mint: m1.clone(),
            native_delta: 25,
            decimals: Some(6),
        },
        TokenDelta {
            mint: m2.clone(),
            native_delta: 4,
            decimals: Some(9),
        },
    ];
    want.sort_by(|a, b| a.mint.cmp(&b.mint));
    assert_eq!(e.receiver_token_deltas, want);
    let mut no_loaded = tx.clone();
    no_loaded["meta"]
        .as_object_mut()
        .unwrap()
        .remove("loadedAddresses");
    let e = enrich(&decode(&no_loaded), &receiver);
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
        tx_event("A", 10, &logs_with(&p, &[event_line()]), None, 1_999),
        0,
        &mut sink,
    )
    .unwrap();
    core.on_transaction(
        tx_event("A2", 10, &logs_with(&p, &[]), None, 2_000),
        0,
        &mut sink,
    )
    .unwrap();
    core.on_transaction(
        tx_event("B", 11, &logs_with(&p, &[event_line()]), None, 2_001),
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
    core.coverage(&mut sink, 0, Some(2_500), gap_event())
        .unwrap();
    assert!(core.coverage_complete());
    core.coverage(&mut sink, 0, Some(1_500), gap_event())
        .unwrap();
    assert!(!core.coverage_complete());
}

#[test]
fn gap_before_stream_ready_is_recorded_but_not_counted() {
    let mut core = ObserverCore::new(program(), MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    core.coverage(&mut sink, 0, Some(5), gap_event()).unwrap();
    assert!(core.coverage_complete());
    assert_eq!(sink.cov[0]["counted"], false);
}

#[test]
fn ingress_overflow_makes_data_incomplete() {
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    core.coverage(
        &mut sink,
        1,
        None,
        super::rows::WindowEvent::IngressOverflow {
            overflow_total: 1,
            drain: false,
        },
    )
    .unwrap();
    let s = view(&core.summary(&ctx(true, true, 60.0)));
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
        tx_event(
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
    let s = view(&core.summary(&ctx(true, true, 3_600.0)));
    assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    assert_eq!(s["decoder_errors_successful_tx"].as_u64(), Some(1));
    // The same corruption in a failed transaction is irrelevant for completeness.
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    core.on_transaction(
        tx_event("badf", 10, &logs_with(&p, &[line]), Some("Custom"), 1),
        0,
        &mut sink,
    )
    .unwrap();
    assert!(core.decode_complete());
}

// [R2-7] logs source can never claim unqualified MARKET_DATA_VALID.
#[test]
fn verdicts_by_source_and_without_liquidations() {
    let tx = view(
        &open_core(MarketSource::Transactions, EnrichConfig::DEFAULT)
            .summary(&ctx(true, true, 14_400.0)),
    );
    assert_eq!(tx["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    assert_eq!(tx["source_coverage_verified"].as_bool(), Some(false));
    assert_eq!(
        tx["receivership_transport_capture_complete"].as_bool(),
        Some(true)
    );
    assert!(tx["gross_edge_median_usd"].is_null() && tx["top_receiver"].is_null());
    assert_eq!(tx["liquidations_per_hour"].as_f64(), Some(0.0));
    let logs = view(
        &open_core(MarketSource::Logs, EnrichConfig::DEFAULT).summary(&ctx(true, true, 14_400.0)),
    );
    assert_eq!(logs["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    assert_eq!(logs["source_coverage_verified"].as_bool(), Some(false));
    assert!(logs["source_coverage_note"]
        .as_str()
        .unwrap()
        .contains("lookup table"));
    let early = view(
        &open_core(MarketSource::Transactions, EnrichConfig::DEFAULT)
            .summary(&ctx(true, false, 0.0)),
    );
    assert!(early["liquidations_per_hour"].is_null());
    assert_eq!(early["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    let mut undrained = ctx(true, true, 14_400.0);
    undrained.drain_complete = false;
    let s = view(&open_core(MarketSource::Transactions, EnrichConfig::DEFAULT).summary(&undrained));
    assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    assert!(s["incomplete_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r == "DRAIN_INCOMPLETE"));
    let not_ready = view(
        &ObserverCore::new(program(), MarketSource::Logs, EnrichConfig::DEFAULT)
            .summary(&ctx(false, false, 0.0)),
    );
    assert_eq!(
        not_ready["verdict"].as_str(),
        Some("MARKET_OBSERVER_STREAM_NOT_READY")
    );
}

/// D6: counter evidence only for a change — an ingress event that changes no counter emits
/// nothing, except the first event of a live hour (it carries the running stream-silence maximum
/// into that hour); every hourly figure of the real worker equals full per-event emission. Hour 1
/// here has unchanged events only.
#[test]
fn d6_counter_evidence_only_on_change_or_a_new_hour() {
    use crate::analytics::econ::{tests as econ, Evidence};
    use darkbot_solana::ingress::SourceHealth;
    let (h, rx) = econ::window_handle();
    let mut core = ObserverCore::new(program(), MarketSource::Logs, EnrichConfig::DEFAULT);
    core.start_observation(1, 4_000_000_001);
    core.econ = Some(h);
    let mut sink = VecSink::default();
    let health = |t_rx_ns| {
        IngressEvent::Health(SourceHealth {
            source: SourceKind::WsRpcControl,
            connected: true,
            last_frame_age_ns: None,
            frames: 1,
            reconnects: 0,
            last_slot: None,
            t_rx_ns,
        })
    };
    let slot = |t_rx_ns| {
        IngressEvent::Slot(SlotUpdate {
            slot: 1,
            parent: None,
            status: SlotStatus::Processed,
            source: SourceKind::WsRpcControl,
            t_rx_ns,
        })
    };
    let script = [
        slot(100_000_000),
        health(200_000_000),
        slot(400_000_000),
        health(500_000_000),
        health(1_200_000_000),
        health(1_600_000_000),
        slot(2_500_000_000),
        health(2_600_000_000),
    ];
    let mut full = Vec::new();
    for ev in script {
        let (at_ns, before) = (ev.t_rx_ns(), core.stats.clone());
        core.on_ingress_event(ev, 0, &mut sink).unwrap();
        full.push(Evidence::MarketCounters {
            at_ns,
            before: Box::new(before),
            after: Box::new(core.stats.clone()),
            observed_slots: None,
        });
    }
    let mut emitted = Vec::new();
    while let Some(e) = rx.try_pop() {
        if matches!(e, Evidence::MarketCounters { .. }) {
            emitted.push(e);
        }
    }
    // slot, slot, first of hour 1, slot (opens hour 2); four unchanged events add nothing.
    assert_eq!(emitted.len(), 4);
    let hourly = econ::hourly_capture_counters(emitted);
    assert_eq!(hourly, econ::hourly_capture_counters(full));
    assert_eq!(
        hourly[1]["max_stream_silence_ms"], 300,
        "hour 1 keeps its carry"
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
        tx_event("early", 1, &logs_with(&p, &[]), None, 1),
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
        logs_event(fx.signature, fx.slot, real_logs, None, 1),
        0,
        &mut sink,
    )
    .unwrap();
    core.on_logs(
        logs_event("other", 7, logs_with(&p, &[event_line()]), None, 2),
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
    let s = view(&core.summary(&ctx(true, true, 3_600.0)));
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
        logs_event("a", 5, logs_with(&p, &[event_line()]), None, 1),
        0,
        &mut sink,
    )
    .unwrap();
    core.on_logs(
        logs_event("b", 6, logs_with(&p, &[event_line()]), None, 2),
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
            Ok(Some(json!({"slot": 5, "version": 0, "meta": {"err": null},
                "transaction": {"signatures": ["zzz"], "message": {"accountKeys": [], "instructions": []}}})))
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
        logs_event("c", 7, logs_with(&p, &[event_line()]), None, 3),
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
        utc_compact(unix_now_s().unwrap())
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let p = program();
    let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut sink = JournalSink::create(&dir).unwrap();
    core.on_logs(
        logs_event(
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
        stopped("TIMER"),
    )
    .unwrap();
    let summary = view(&summary);
    assert_eq!(summary["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
    assert_eq!(summary["commitment"].as_str(), Some("finalized"));
    assert!(report.contains("verdict=MARKET_DATA_INCOMPLETE"));
    render_views(&dir);
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
    // The typed summary journal is the record; the run writes no summary.json (JFI-03).
    let typed: MarketFinal =
        crate::analytics::journal::read_typed_value(&dir.join(MARKET_SUMMARY_JOURNAL)).unwrap();
    assert_eq!(view(&typed)["liquidation_events"].as_u64(), Some(1));
    assert!(!dir.join("summary.json").exists());
    assert!(dir.join("final_report.txt").exists());
    assert!(JournalSink::create(&dir).is_err()); // never overwrites a run directory
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
    let continuous = MarketConfig::from_lookup(
        &env(&[
            base[0],
            base[1],
            ("P0_MARKET_DURATION_SECS", "until-stopped"),
        ]),
        0,
    )
    .unwrap();
    assert_eq!(continuous.duration_secs, 0);
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
        let s = view(&open_core(source, EnrichConfig::DEFAULT).summary(&ctx(true, true, 14_400.0)));
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
        utc_compact(unix_now_s().unwrap())
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let p = program();
    let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut sink = JournalSink::create(&dir).unwrap();
    core.on_logs(
        logs_event(
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
        stopped("TIMER"),
    )
    .unwrap();
    let summary = view(&summary);
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
    assert!(dir.join(MARKET_SUMMARY_JOURNAL).exists() && dir.join("final_report.txt").exists());
    assert!(!dir.join("summary.json").exists());
    assert!(report.contains("enrichment_complete=false"));
    render_views(&dir);
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
        utc_compact(unix_now_s().unwrap())
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
    let mut sink = JournalSink::create(&cfg.run_dir).unwrap();
    let mut core = ObserverCore::new(cfg.program, cfg.source, cfg.enrich);
    core.start_observation(0, 600_000_000_000);
    core.on_logs(
        logs_event(
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
        finished_unix_s: 1_700_000_100,
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
    let summary = view(&summary);
    assert_eq!(write_error, None);
    assert!(
        cfg.run_dir.join(MARKET_SUMMARY_JOURNAL).exists()
            && cfg.run_dir.join("final_report.txt").exists()
            && !cfg.run_dir.join("summary.json").exists()
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
    render_views(&cfg.run_dir);
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
    type Layout = (usize, usize, usize, Result<usize, usize>, usize);
    let mut layouts: Vec<Layout> = Vec::new();
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
        json!({"InstructionError": [0, {"Custom": 1}]})
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
    if failed {
        if let Some(meta) = inner.get_mut("meta").filter(|m| m.is_object()) {
            meta["err"] = json!({"InstructionError": [0, {"Custom": 1}]});
        }
    }
    let raw = json!({"signature": sig, "slot": slot, "transaction": inner});
    TransactionObserved {
        signature: sig.into(),
        slot,
        tx_index_in_slot: None,
        source: SourceKind::WsRpcControl,
        t_rx_ns: 1,
        tx: darkbot_solana::chain::decode_notification(&raw),
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
    core.on_transaction(as_observed(&tx, "sigT", 50, false), 0, &mut sink)
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
    let s = view(&core.summary(&ctx(true, true, 600.0)));
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
        logs_event("sigL", 51, truncated_logs(&[]), None, 1),
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
    let s = view(&core.summary(&ctx(true, true, 600.0)));
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
    let s = view(&core.summary(&ctx(true, true, 600.0)));
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
        logs_event("a", 1, truncated_logs(&[]), None, 1),
        0,
        &mut sink,
    )
    .unwrap();
    core.on_logs(
        logs_event("b", 2, truncated_logs(&[]), None, 2),
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
    let s = view(&core.summary(&ctx(true, true, 600.0)));
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
        logs_event("c", 3, truncated_logs(&[]), None, 3),
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
    core.on_transaction(as_observed(&tx, "f", 9, true), 0, &mut sink)
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
    core.on_transaction(as_observed(&tx, "n", 7, false), 0, &mut sink)
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
    let s = view(&core.summary(&ctx(true, true, 600.0)));
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
        Fx { raw: include_str!("../../../../../docs/evidence/AUDIT_20261005/2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf.json"), signature: "2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf", slot: 451703584, gross: 0.186822, receiver: "D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn", target: "3GqwCURBtJZfdQxMSyZ1VyWNSB5WXovg7Q8Q46HY7ihx", fee: 19000, cu_limit: Some(1400000), lamport_delta: -1019000, token_mints: 2 },
        Fx { raw: include_str!("../../../../../docs/evidence/AUDIT_20261005/38ZXvW6P5sGZmdzj4QbUPSdQnCnJEBHoXTh4vvvfitFPNDHRsgPjELCnFxxCfW2wygE8J6k2Yv8RJJXPUVchBm7F.json"), signature: "38ZXvW6P5sGZmdzj4QbUPSdQnCnJEBHoXTh4vvvfitFPNDHRsgPjELCnFxxCfW2wygE8J6k2Yv8RJJXPUVchBm7F", slot: 451704201, gross: 0.531769, receiver: "D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn", target: "AzQVTAtLxdaQUJzh7xZ7rj956tkeVNk4D4iuK5EVkxDi", fee: 19000, cu_limit: Some(1400000), lamport_delta: -1019000, token_mints: 2 },
        Fx { raw: include_str!("../../../../../docs/evidence/AUDIT_20261005/3ab7mnuTErWjRtS8iz4Nm619VNvm3FSw47VVvdjYnSLNzB2btCoreDsQqmzfb4Qt5oBA29saPQ98Rgy2nyAUExRB.json"), signature: "3ab7mnuTErWjRtS8iz4Nm619VNvm3FSw47VVvdjYnSLNzB2btCoreDsQqmzfb4Qt5oBA29saPQ98Rgy2nyAUExRB", slot: 451657746, gross: 0.18595, receiver: "TraceR2t18nnwkXgHoB54iPD5N4x2EJZC1uS55ggsH8", target: "6mzmLGPk7CeGCM7dRH8ZZpHsSzfy6aAYf47nbH8HuYy8", fee: 5000, cu_limit: Some(598236), lamport_delta: -1005000, token_mints: 2 },
        Fx { raw: include_str!("../../../../../docs/evidence/AUDIT_20261005/59Qt6h1iPnEgfaDaEi5Y5jVvqHJCs4gzwvHqWW37wVJMHyThYjM6zsS8AmE9bvoxqdUj8AMnNbBuKHQtXysdnwNa.json"), signature: "59Qt6h1iPnEgfaDaEi5Y5jVvqHJCs4gzwvHqWW37wVJMHyThYjM6zsS8AmE9bvoxqdUj8AMnNbBuKHQtXysdnwNa", slot: 453063240, gross: 0.060183, receiver: "qbvAGHo9JpVRsuRem8rBCzLJJNV6ZDNv63qYHkcmNCV", target: "HMjc1LecbCMUqYG3x36gaCzxG5AJ7gJPkPB85fx1pPKK", fee: 5000, cu_limit: Some(1400000), lamport_delta: -1005000, token_mints: 2 },
        Fx { raw: include_str!("../../../../../docs/evidence/AUDIT_20261005/C6JnpzGrv53y9dEExtpDYkzTbexLEu7oikPRNx8sCqf85Jvobc7UBpsaDSgGHzEgeB88HkVFqMkCkvs1ZqVWRwZ.json"), signature: "C6JnpzGrv53y9dEExtpDYkzTbexLEu7oikPRNx8sCqf85Jvobc7UBpsaDSgGHzEgeB88HkVFqMkCkvs1ZqVWRwZ", slot: 451635082, gross: 0.2098, receiver: "TraceR2t18nnwkXgHoB54iPD5N4x2EJZC1uS55ggsH8", target: "6mzmLGPk7CeGCM7dRH8ZZpHsSzfy6aAYf47nbH8HuYy8", fee: 5000, cu_limit: Some(779995), lamport_delta: -1005000, token_mints: 2 },
        Fx { raw: include_str!("../../../../../docs/evidence/AUDIT_20261005/gLVXyfVZmizZqpF9752x3uhgJixW5Jqrp3Agjhhr6cNUnU5aQDBAxQ1CF9RWSpy2oB2K9EAroFJaGPTkz5vkUzB.json"), signature: "gLVXyfVZmizZqpF9752x3uhgJixW5Jqrp3Agjhhr6cNUnU5aQDBAxQ1CF9RWSpy2oB2K9EAroFJaGPTkz5vkUzB", slot: 451643526, gross: 0.1742, receiver: "D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn", target: "6mzmLGPk7CeGCM7dRH8ZZpHsSzfy6aAYf47nbH8HuYy8", fee: 19000, cu_limit: Some(1400000), lamport_delta: -1019000, token_mints: 2 },
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
        let chain = darkbot_solana::chain::decode_transaction(&tx).unwrap();
        let en = enrich(&chain, &e.receiver);
        assert_eq!(en.tx_fee_lamports, Some(fx.fee));
        assert_eq!(en.fee_payer.as_deref(), Some(fx.receiver));
        assert_eq!(en.cu_limit, fx.cu_limit);
        // v1: `transactionConfig.priorityFee` is lamports; fee = 5000 base + priority.
        if chain.message.transaction_config.is_some() {
            assert_eq!(en.priority_fee_lamports, Some(14_000));
            assert_eq!(fx.fee, 5_000 + 14_000);
        }
        assert_eq!(en.receiver_lamport_delta, Some(fx.lamport_delta));
        assert_eq!(en.receiver_token_deltas.len(), fx.token_mints);
        assert!(en.token_deltas_complete);
        assert_eq!(
            check_transaction_identity(&chain, fx.signature, fx.slot),
            Ok(())
        );
        assert!(check_transaction_identity(&chain, fx.signature, fx.slot + 1).is_err());
    }
}

#[test]
fn six_real_audit_liquidations_aggregate_into_market_summary() {
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut sink = VecSink::default();
    for fx in fixtures() {
        let tx: Value = serde_json::from_str(fx.raw).unwrap();
        let raw = json!({"signature": fx.signature, "slot": fx.slot, "transaction": tx});
        let ev = TransactionObserved {
            signature: fx.signature.into(),
            slot: fx.slot,
            tx_index_in_slot: None,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 1,
            tx: darkbot_solana::chain::decode_notification(&raw),
        };
        core.on_transaction(ev, 0, &mut sink).unwrap();
    }
    let s = view(&core.summary(&ctx(true, true, 3_600.0)));
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
fn typed_market_frames_render_offline_to_their_legacy_rows() {
    use super::rows::*;
    let dir = journal_test_dir("journal_typed");
    let p = program();
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    let mut capture = VecSink::default();
    let mut sink = JournalSink::create(&dir).unwrap();
    let ev = tx_event("s1", 5, &logs_with(&p, &[event_line()]), None, 7);
    core.on_transaction(ev.clone(), 0, &mut capture).unwrap();
    let mut core2 = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    core2.on_transaction(ev, 0, &mut sink).unwrap();
    core2
        .coverage(
            &mut sink,
            1,
            Some(8),
            WindowEvent::Gap {
                from_slot: Some(4),
                to_slot: None,
            },
        )
        .unwrap();
    core.coverage(
        &mut capture,
        1,
        Some(8),
        WindowEvent::Gap {
            from_slot: Some(4),
            to_slot: None,
        },
    )
    .unwrap();
    let binding = sink.finish(true).unwrap().unwrap();
    assert_eq!(
        (binding.status.as_str(), binding.event_frames),
        ("COMPLETE", 3)
    );
    // No view exists after capture; the explicit offline export renders the legacy rows.
    for (_, name) in MARKET_VIEWS {
        assert!(!dir.join(name).exists(), "{name}");
    }
    let views: Vec<(Option<u16>, PathBuf)> = MARKET_VIEWS
        .iter()
        .map(|(k, n)| (Some(*k), dir.join(n)))
        .collect();
    let refs: Vec<(Option<u16>, &Path)> = views.iter().map(|(k, p)| (*k, p.as_path())).collect();
    crate::analytics::journal::export_views(&dir.join(MARKET_JOURNAL), &refs, false).unwrap();
    for (rows, (_, name)) in [&capture.tx, &capture.liq, &capture.cov]
        .into_iter()
        .zip(MARKET_VIEWS)
    {
        let expected: String = rows.iter().map(|r| format!("{r}\n")).collect();
        assert_eq!(
            fs::read_to_string(dir.join(name)).unwrap(),
            expected,
            "{name}"
        );
    }
    let mut reader = crate::analytics::journal::Reader::open(&dir.join(MARKET_JOURNAL)).unwrap();
    let first = reader.next_frame().unwrap().unwrap();
    assert_eq!(
        (first.schema, first.provenance.slot, first.provenance.rx_ns),
        (2, Some(5), Some(7))
    );
    let row: MarketTransaction = first.typed().unwrap();
    assert!(row.chain_tx().is_some_and(|t| t.status().is_success()));
    while reader.next_frame().unwrap().is_some() {}
    assert!(reader.sealed && reader.complete);
    assert_eq!(
        crate::analytics::journal::hex(&reader.root()),
        binding.root_sha256
    );
    // Schema-1 market rows are refused (no legacy rollback of a typed kind).
    let mut w = crate::analytics::journal::Writer::create(&dir.join("legacy.p0j")).unwrap();
    assert!(w
        .append(
            crate::analytics::journal::kind::MARKET_COVERAGE,
            1,
            Default::default(),
            b"x"
        )
        .is_err());
    // One finish only; an existing journal is never overwritten.
    assert!(sink.finish(true).is_err());
    drop(sink);
    assert!(JournalSink::create(&dir).is_err());
    fs::remove_dir_all(dir).unwrap();
}

/// D1: an ordered capture end seals the market journal COMPLETE only after a full TIMER window
/// with a completed drain, no write error and no persistent incomplete reason; every abort path
/// seals INCOMPLETE in journal, binding and verifier alike.
#[test]
fn capture_operator_stop_seals_incomplete() {
    let root = std::env::temp_dir().join(format!(
        "c0mo_d1_{}_{}",
        std::process::id(),
        utc_compact(unix_now_s().unwrap())
    ));
    let _ = fs::remove_dir_all(&root);
    let root_s = root.to_string_lossy().to_string();
    let cases: [(&str, &'static str, bool, Option<&str>, bool); 7] = [
        ("timer", "TIMER", true, None, true),
        ("operator", "OPERATOR_STOP", true, None, false),
        ("not_ready", "STREAM_NOT_READY", true, None, false),
        ("ingress", "INGRESS_THREAD_ENDED", true, None, false),
        ("abort", "C0_E1_ABORT", true, None, false),
        ("write", "TIMER", true, Some("WRITE_FAILED"), false),
        ("drain", "TIMER", false, None, false),
    ];
    for (run, stop, drain, write_error, complete) in cases {
        let cfg = MarketConfig::from_lookup(
            &env_of(&[
                ("P0_WS_URL", "wss://h"),
                (
                    "P0_PROGRAM_ID",
                    darkbot_project0::protocol::PROGRAM_ID_MAINNET,
                ),
                ("P0_MARKET_SOURCE", "logs"),
                ("P0_MARKET_OUT_DIR", root_s.as_str()),
                ("P0_MARKET_RUN_ID", run),
            ]),
            0,
        )
        .unwrap();
        fs::create_dir_all(&cfg.run_dir).unwrap();
        let mut sink = JournalSink::create(&cfg.run_dir).unwrap();
        let mut core = ObserverCore::new(cfg.program, cfg.source, cfg.enrich);
        let ready = stop != "STREAM_NOT_READY";
        if ready {
            core.start_observation(0, 600_000_000_000);
            core.stop_observation();
        }
        let outcome = CaptureOutcome {
            finished_unix_s: 1_700_000_100,
            process_started_unix_s: 0,
            ready_unix_s: ready.then_some(0),
            window: ready.then_some((0, 600_000_000_000)),
            capture_end_ns: if stop == "TIMER" { 600_000_000_000 } else { 1 },
            observation_end_unix_s: 600,
            stop_reason: stop,
            drain_complete: drain,
            write_error: write_error.map(String::from),
        };
        let (summary, _, _) = finish_capture(&cfg, &mut core, &mut sink, outcome).unwrap();
        let status = if complete { "COMPLETE" } else { "INCOMPLETE" };
        assert_eq!(
            view(&summary)["evidence_journal"]["status"],
            status,
            "{run}"
        );
        let chain =
            crate::storage::segments::verify_chain(&cfg.run_dir.join(MARKET_JOURNAL)).unwrap();
        assert_eq!((chain.sealed, chain.complete), (true, complete), "{run}");
    }
    let _ = fs::remove_dir_all(&root);
}

/// R2: a finished ingress thread is joined before the journal is sealed. A clean stop keeps a
/// complete drain (COMPLETE); a panic or an error return right after the last delivered event is
/// INCOMPLETE in the summary binding and the journal chain — never COMPLETE.
#[test]
fn ingress_panic_after_last_event_never_seals_complete() {
    use std::time::{Duration, Instant};
    let root = std::env::temp_dir().join(format!(
        "c0mo_r2_{}_{}",
        std::process::id(),
        utc_compact(unix_now_s().unwrap())
    ));
    let _ = fs::remove_dir_all(&root);
    let root_s = root.to_string_lossy().to_string();
    for (run, end, complete) in [
        ("clean", 0u8, true),
        ("panic", 1, false),
        ("error", 2, false),
    ] {
        let cfg = MarketConfig::from_lookup(
            &env_of(&[
                ("P0_WS_URL", "wss://h"),
                (
                    "P0_PROGRAM_ID",
                    darkbot_project0::protocol::PROGRAM_ID_MAINNET,
                ),
                ("P0_MARKET_SOURCE", "logs"),
                ("P0_MARKET_OUT_DIR", root_s.as_str()),
                ("P0_MARKET_RUN_ID", run),
            ]),
            0,
        )
        .unwrap();
        fs::create_dir_all(&cfg.run_dir).unwrap();
        let mut sink = JournalSink::create(&cfg.run_dir).unwrap();
        let mut core = ObserverCore::new(cfg.program, cfg.source, cfg.enrich);
        core.start_observation(0, 600_000_000_000);
        let (tx, rx) = darkbot_common::channel::bounded::<IngressEvent>("r2", 16);
        let thread: super::capture::IngressThread = std::thread::spawn(move || {
            tx.try_push(IngressEvent::Slot(SlotUpdate {
                slot: 100,
                parent: Some(99),
                status: SlotStatus::Processed,
                source: SourceKind::WsRpcControl,
                t_rx_ns: 1,
            }))
            .unwrap();
            match end {
                0 => Ok(()),
                1 => panic!("R2 fake ingress panic after its last event"),
                _ => Err("WSS_RECOVERY_EXHAUSTED"),
            }
        });
        let mut ingress = Some(thread);
        let mut last_overflow = 0;
        let (drain, write_error, end_error) = super::capture::drain_after_stop(
            &rx,
            &mut ingress,
            &mut core,
            &mut sink,
            darkbot_common::timing::Clock::new(),
            Instant::now() + Duration::from_secs(10),
            &mut last_overflow,
        );
        assert!(
            ingress.is_none(),
            "{run}: a finished thread is joined in the drain"
        );
        assert_eq!((drain, write_error), (complete, None), "{run}");
        assert_eq!(
            end_error,
            match end {
                0 => None,
                1 => Some("INGRESS_THREAD_PANIC"),
                _ => Some("WSS_RECOVERY_EXHAUSTED"),
            },
            "{run}"
        );
        core.stop_observation();
        let outcome = CaptureOutcome {
            finished_unix_s: 1_700_000_100,
            process_started_unix_s: 0,
            ready_unix_s: Some(0),
            window: Some((0, 600_000_000_000)),
            capture_end_ns: 600_000_000_000,
            observation_end_unix_s: 600,
            stop_reason: "TIMER",
            drain_complete: drain,
            write_error: None,
        };
        let (summary, _, _) = finish_capture(&cfg, &mut core, &mut sink, outcome).unwrap();
        let status = if complete { "COMPLETE" } else { "INCOMPLETE" };
        assert_eq!(
            view(&summary)["evidence_journal"]["status"],
            status,
            "{run}"
        );
        let chain =
            crate::storage::segments::verify_chain(&cfg.run_dir.join(MARKET_JOURNAL)).unwrap();
        assert_eq!((chain.sealed, chain.complete), (true, complete), "{run}");
    }
    let _ = fs::remove_dir_all(&root);
}

/// R1 (rest): at the stop the consumer queue is full, half full, or still filling while the
/// ingress ends: every queued and late frame is processed before the capture seals, the drain
/// is complete, ends far inside the deadline and the journal seals COMPLETE (TIMER). Like the
/// real WSS producer, the fake never retries: a frame refused by a full queue is lost, and that
/// capture seals INCOMPLETE (`lost`).
#[test]
fn r1_full_and_half_full_queue_drain_completely_before_the_seal() {
    use std::time::{Duration, Instant};
    const CAP: usize = 64;
    let root = std::env::temp_dir().join(format!(
        "c0mo_r1_{}_{}",
        std::process::id(),
        utc_compact(unix_now_s().unwrap())
    ));
    let _ = fs::remove_dir_all(&root);
    let root_s = root.to_string_lossy().to_string();
    for (run, queued, late, lost) in [
        ("full", CAP, 0usize, false),
        ("half", CAP / 2, 0, false),
        ("late", CAP / 2, 40, false),
        ("lost", CAP, 0, true),
    ] {
        let cfg = MarketConfig::from_lookup(
            &env_of(&[
                ("P0_WS_URL", "wss://h"),
                (
                    "P0_PROGRAM_ID",
                    darkbot_project0::protocol::PROGRAM_ID_MAINNET,
                ),
                ("P0_MARKET_SOURCE", "logs"),
                ("P0_MARKET_OUT_DIR", root_s.as_str()),
                ("P0_MARKET_RUN_ID", run),
            ]),
            0,
        )
        .unwrap();
        fs::create_dir_all(&cfg.run_dir).unwrap();
        let mut sink = JournalSink::create(&cfg.run_dir).unwrap();
        let mut core = ObserverCore::new(cfg.program, cfg.source, cfg.enrich);
        core.start_observation(0, 600_000_000_000);
        let (tx, rx) = darkbot_common::channel::bounded::<IngressEvent>("r1", CAP);
        let slot = |n: usize| {
            IngressEvent::Slot(SlotUpdate {
                slot: 100 + n as u64,
                parent: Some(99 + n as u64),
                status: SlotStatus::Processed,
                source: SourceKind::WsRpcControl,
                t_rx_ns: 1,
            })
        };
        for n in 0..queued {
            tx.try_push(slot(n)).unwrap();
        }
        if lost {
            assert!(
                tx.try_push(slot(queued)).is_err(),
                "queue full: the frame is lost"
            );
        }
        // The ingress delivers its last frames while the drain runs, then ends cleanly.
        let thread: super::capture::IngressThread = std::thread::spawn(move || {
            for n in queued..queued + late {
                std::thread::sleep(Duration::from_millis(1));
                tx.try_push(slot(n))
                    .expect("the draining consumer keeps up");
            }
            Ok(())
        });
        let mut ingress = Some(thread);
        let mut last_overflow = 0;
        let started = Instant::now();
        let (drain, write_error, end_error) = super::capture::drain_after_stop(
            &rx,
            &mut ingress,
            &mut core,
            &mut sink,
            darkbot_common::timing::Clock::new(),
            Instant::now() + Duration::from_secs(120),
            &mut last_overflow,
        );
        assert!(started.elapsed() < Duration::from_secs(10), "{run}");
        assert_eq!((drain, write_error, end_error), (true, None, None), "{run}");
        assert!(ingress.is_none(), "{run}");
        assert_eq!(
            rx.stats().received as usize,
            queued + late,
            "{run}: every frame"
        );
        assert_eq!(
            core.stats.slot_notifications as usize,
            queued + late,
            "{run}"
        );
        core.stop_observation();
        let outcome = CaptureOutcome {
            finished_unix_s: 1_700_000_100,
            process_started_unix_s: 0,
            ready_unix_s: Some(0),
            window: Some((0, 600_000_000_000)),
            capture_end_ns: 600_000_000_000,
            observation_end_unix_s: 600,
            stop_reason: "TIMER",
            drain_complete: drain,
            write_error: None,
        };
        let (summary, _, _) = finish_capture(&cfg, &mut core, &mut sink, outcome).unwrap();
        let status = if lost { "INCOMPLETE" } else { "COMPLETE" };
        assert_eq!(
            view(&summary)["evidence_journal"]["status"],
            status,
            "{run}"
        );
        let chain =
            crate::storage::segments::verify_chain(&cfg.run_dir.join(MARKET_JOURNAL)).unwrap();
        assert_eq!((chain.sealed, chain.complete), (true, !lost), "{run}");
    }
    let _ = fs::remove_dir_all(&root);
}

/// R1 (deadline case): an ingress that is still alive when the drain deadline passes — it keeps
/// its producer open and delivers a frame now and then — ends the drain at the deadline (bounded,
/// never waiting for the thread), the frames received until then are processed, the thread is
/// not joined, the drain is not complete and the capture seals INCOMPLETE: a real timeout is
/// never a complete capture (fail-closed).
#[test]
fn r1_ingress_alive_past_the_drain_deadline_seals_incomplete() {
    use std::time::{Duration, Instant};
    let root = std::env::temp_dir().join(format!(
        "c0mo_r1d_{}_{}",
        std::process::id(),
        utc_compact(unix_now_s().unwrap())
    ));
    let _ = fs::remove_dir_all(&root);
    let root_s = root.to_string_lossy().to_string();
    let cfg = MarketConfig::from_lookup(
        &env_of(&[
            ("P0_WS_URL", "wss://h"),
            (
                "P0_PROGRAM_ID",
                darkbot_project0::protocol::PROGRAM_ID_MAINNET,
            ),
            ("P0_MARKET_SOURCE", "logs"),
            ("P0_MARKET_OUT_DIR", root_s.as_str()),
            ("P0_MARKET_RUN_ID", "deadline"),
        ]),
        0,
    )
    .unwrap();
    fs::create_dir_all(&cfg.run_dir).unwrap();
    let mut sink = JournalSink::create(&cfg.run_dir).unwrap();
    let mut core = ObserverCore::new(cfg.program, cfg.source, cfg.enrich);
    core.start_observation(0, 600_000_000_000);
    let (tx, rx) = darkbot_common::channel::bounded::<IngressEvent>("r1d", 64);
    let slot = |n: u64| {
        IngressEvent::Slot(SlotUpdate {
            slot: 100 + n,
            parent: Some(99 + n),
            status: SlotStatus::Processed,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 1,
        })
    };
    let release = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let hold = release.clone();
    // Alive well past the deadline: one frame every 20 ms until the test releases it.
    let thread: super::capture::IngressThread = std::thread::spawn(move || {
        let mut n = 0;
        while !hold.load(std::sync::atomic::Ordering::Acquire) {
            let _ = tx.try_push(slot(n));
            n += 1;
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    });
    let mut ingress = Some(thread);
    let mut last_overflow = 0;
    let deadline = Instant::now() + Duration::from_millis(300);
    let (drain, write_error, end_error) = super::capture::drain_after_stop(
        &rx,
        &mut ingress,
        &mut core,
        &mut sink,
        darkbot_common::timing::Clock::new(),
        deadline,
        &mut last_overflow,
    );
    let ended = Instant::now();
    assert!(ended >= deadline, "not before the deadline");
    assert!(
        ended < deadline + Duration::from_millis(500),
        "bounded: the drain never waits for the thread"
    );
    assert_eq!((drain, write_error, end_error), (false, None, None));
    assert!(ingress.is_some(), "a live thread is not joined");
    assert!(
        core.stats.slot_notifications > 0,
        "frames until then processed"
    );
    assert_eq!(
        core.stats.slot_notifications,
        rx.stats().received,
        "every popped frame processed"
    );
    core.stop_observation();
    let outcome = CaptureOutcome {
        finished_unix_s: 1_700_000_100,
        process_started_unix_s: 0,
        ready_unix_s: Some(0),
        window: Some((0, 600_000_000_000)),
        capture_end_ns: 600_000_000_000,
        observation_end_unix_s: 600,
        stop_reason: "TIMER",
        drain_complete: drain,
        write_error: None,
    };
    let (summary, _, _) = finish_capture(&cfg, &mut core, &mut sink, outcome).unwrap();
    assert_eq!(view(&summary)["evidence_journal"]["status"], "INCOMPLETE");
    let chain = crate::storage::segments::verify_chain(&cfg.run_dir.join(MARKET_JOURNAL)).unwrap();
    assert_eq!((chain.sealed, chain.complete), (true, false));
    // The thread ends after the seal, before the join (sealing can take long): the join past the
    // deadline finds it finished — quiescent, `Ok` — and the run result is still the incomplete
    // capture, never a successful run.
    release.store(true, std::sync::atomic::Ordering::Release);
    let thread = ingress.take().unwrap();
    while !thread.is_finished() {
        std::thread::sleep(Duration::from_millis(5));
    }
    let quiescent = super::capture::join_after_seal(Some(thread), deadline);
    assert_eq!(quiescent, Ok(()), "a finished thread is joined at once");
    assert_eq!(
        super::capture::capture_end(&cfg, &quiescent, "TIMER", true, None, None, drain),
        Err("MARKET_CAPTURE_INCOMPLETE: DRAIN_INCOMPLETE".into())
    );
    let _ = fs::remove_dir_all(&root);
}

/// R1 rest: the production `capture::run` end to end against a loopback WebSocket — an
/// established plain `ws://` session through the real WSS adapter, never Mainnet. The peer
/// acknowledges both subscriptions and delivers slot progress every 50 ms: StreamReady, the 1-s
/// window, the requested stop ends the source and closes the session without waiting for the
/// peer, the drain completes before its deadline, the journal seals COMPLETE and the run returns
/// `Ok` — no avoidable INCOMPLETE for an ended stream.
#[test]
fn r1_capture_run_end_to_end_over_a_loopback_websocket() {
    use std::time::{Duration, Instant};
    use tungstenite::Message;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        let mut methods = Vec::new();
        let mut slot_subscription = None;
        let mut slot = 1_000u64;
        loop {
            match ws.read() {
                Ok(Message::Text(text)) => {
                    let request: Value = serde_json::from_str(&text).unwrap();
                    let id = request["id"].as_u64().unwrap();
                    let method = request["method"].as_str().unwrap().to_string();
                    if method == "slotSubscribe" {
                        slot_subscription = Some(id);
                    }
                    methods.push(method);
                    let ack = json!({"jsonrpc": "2.0", "id": id, "result": id});
                    if ws.send(Message::Text(ack.to_string())).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                // The client closed or dropped the session.
                Err(_) => break,
            }
            if let (2, Some(subscription)) = (methods.len(), slot_subscription) {
                slot += 1;
                let notification = json!({"jsonrpc": "2.0", "method": "slotNotification",
                    "params": {"subscription": subscription,
                        "result": {"slot": slot, "parent": slot - 1, "root": slot - 2}}});
                if ws.send(Message::Text(notification.to_string())).is_err() {
                    break;
                }
            }
        }
        (methods, Instant::now())
    });
    let root = std::env::temp_dir().join(format!(
        "c0mo_r1e2e_{}_{}",
        std::process::id(),
        utc_compact(unix_now_s().unwrap())
    ));
    let _ = fs::remove_dir_all(&root);
    let root_s = root.to_string_lossy().to_string();
    let cfg = MarketConfig::from_lookup(
        &env_of(&[
            ("P0_WS_URL", url.as_str()),
            (
                "P0_PROGRAM_ID",
                darkbot_project0::protocol::PROGRAM_ID_MAINNET,
            ),
            ("P0_MARKET_SOURCE", "logs"),
            ("P0_MARKET_OUT_DIR", root_s.as_str()),
            ("P0_MARKET_RUN_ID", "e2e"),
            ("P0_MARKET_DURATION_SECS", "1"),
            ("P0_MARKET_READY_TIMEOUT_SECS", "10"),
            ("P0_MARKET_DRAIN_TIMEOUT_SECS", "5"),
            ("P0_MARKET_HEARTBEAT_SECS", "1"),
        ]),
        unix_now_s().unwrap(),
    )
    .unwrap();
    let run_dir = cfg.run_dir.clone();
    let started = Instant::now();
    let result = super::capture::run(cfg);
    let ended = Instant::now();
    // Before the join: a session that never stops would never be closed for the peer.
    assert_eq!(result, Ok(()));
    let (mut methods, closed) = server.join().unwrap();
    methods.sort();
    assert_eq!(methods, ["logsSubscribe", "slotSubscribe"]);
    assert!(
        closed <= ended,
        "the session is closed before the run returns"
    );
    assert!(
        ended - started < Duration::from_secs(1 + 5),
        "within the window and the drain budget: {:?}",
        ended - started
    );
    let chain = crate::storage::segments::verify_chain(&run_dir.join(MARKET_JOURNAL)).unwrap();
    assert_eq!((chain.sealed, chain.complete), (true, true));
    let summary: MarketFinal =
        crate::analytics::journal::read_typed_value(&run_dir.join(MARKET_SUMMARY_JOURNAL)).unwrap();
    assert_eq!(summary.evidence_journal.unwrap().status, "COMPLETE");
    let s = summary.summary;
    assert!(s.stream_ready_at.is_some());
    assert!(s.drain_complete && s.capture_clock_complete);
    let _ = fs::remove_dir_all(&root);
}

/// R1/D8: after the seal the ingress thread is joined within what is left of the drain budget:
/// a thread still alive at the passed drain deadline is reported at once (`INGRESS_NOT_QUIESCENT`,
/// no second full budget — the former call waited `drain_timeout_secs` again), a thread ending
/// within the remaining budget is joined, no thread is quiescent.
#[test]
fn r1_ingress_is_joined_within_the_remaining_drain_budget() {
    use std::time::{Duration, Instant};
    let release = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let hold = release.clone();
    let alive = std::thread::spawn(move || {
        while !hold.load(std::sync::atomic::Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    let started = Instant::now();
    assert_eq!(
        super::capture::join_after_seal(Some(alive), started - Duration::from_millis(1)),
        Err("INGRESS_NOT_QUIESCENT".into())
    );
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "no second budget"
    );
    release.store(true, std::sync::atomic::Ordering::Release);
    let ending = std::thread::spawn(|| std::thread::sleep(Duration::from_millis(50)));
    assert_eq!(
        super::capture::join_after_seal(Some(ending), Instant::now() + Duration::from_secs(5)),
        Ok(())
    );
    assert_eq!(
        super::capture::join_after_seal::<()>(None, Instant::now()),
        Ok(())
    );
}

/// R9: the capture's periodic sink check surfaces a sticky background writer failure while no
/// further row is written (the tick used to call a no-op `JournalSink::flush`).
#[test]
fn journal_sink_flush_reports_a_failed_writer_without_a_new_row() {
    use super::rows::MarketCoverage;
    use std::time::{Duration, Instant};
    let dir = std::env::temp_dir().join(format!(
        "c0mo_r9_{}_{}",
        std::process::id(),
        utc_compact(unix_now_s().unwrap())
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let cfg = crate::storage::writer::WriterConfig {
        initial_bytes: 1 << 20,
        max_bytes: 1 << 20,
        system_reserve_bytes: 0,
        batch_frames: 1,
        flush_interval: Duration::from_millis(1),
        sync_interval: Duration::from_millis(20),
        stall_budget: Duration::from_millis(200),
        disk_reserve_bytes: 0,
        segment_bytes: 0,
        fail_at_batch: Some(0),
        hold_first: Duration::ZERO,
    };
    let mut sink = JournalSink::create_with(&dir, cfg).unwrap();
    assert!(sink.flush().is_ok(), "a healthy writer");
    sink.coverage(&MarketCoverage::SlotFailed {
        slot: 1,
        error: "x".into(),
    })
    .unwrap();
    // The first batch fails on the writer thread; no further row is written.
    let deadline = Instant::now() + Duration::from_secs(5);
    let reported = loop {
        match sink.flush() {
            Err(e) => break e.to_string(),
            Ok(()) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            Ok(()) => panic!("a failed writer stays invisible to the periodic check"),
        }
    };
    assert!(reported.contains("injected write failure"), "{reported}");
    drop(sink);
    let _ = fs::remove_dir_all(&dir);
}

/// D8: a stopped ingress thread is joined within the budget; one still running is reported,
/// never declared quiescent.
#[test]
fn ingress_is_joined_or_reported_not_quiescent() {
    use std::time::{Duration, Instant};
    let quick = std::thread::spawn(|| std::thread::sleep(Duration::from_millis(100)));
    assert_eq!(
        super::capture::quiesce(quick, Duration::from_secs(2)),
        Ok(())
    );
    let (release, rx) = std::sync::mpsc::channel::<()>();
    let stuck = std::thread::spawn(move || {
        let _ = rx.recv_timeout(Duration::from_secs(5));
    });
    let t0 = Instant::now();
    assert_eq!(
        super::capture::quiesce(stuck, Duration::from_millis(200)).unwrap_err(),
        "INGRESS_NOT_QUIESCENT"
    );
    assert!(t0.elapsed() < Duration::from_secs(2));
    drop(release);
}

/// D1 (review rest): the compatibility batch sink honours the completeness of `finish` too.
#[test]
fn jsonl_sink_finish_seals_incomplete_when_asked() {
    use super::rows::MarketCoverage;
    for complete in [false, true] {
        let dir = journal_test_dir(if complete { "d1_jsonl_c" } else { "d1_jsonl_i" });
        let mut sink = JsonlSink::create(&dir).unwrap();
        sink.coverage(&MarketCoverage::ShadowWarmupFailed { error: "X".into() })
            .unwrap();
        sink.finish(complete).unwrap();
        let mut reader =
            crate::analytics::journal::Reader::open(&dir.join(MARKET_JOURNAL)).unwrap();
        while reader.next_frame().unwrap().is_some() {}
        assert_eq!((reader.sealed, reader.complete), (true, complete));
        fs::remove_dir_all(dir).unwrap();
    }
}

/// D4: logs admission never flushes per transaction, so a batch sink is not finished early and
/// every row reaches the journal before the explicit batch end.
#[test]
fn logs_never_finish_a_batch_sink_per_transaction() {
    let dir = journal_test_dir("d4_batch");
    let p = program();
    let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut sink = JsonlSink::create(&dir).unwrap();
    for (sig, slot) in [("d4-a", 9), ("d4-b", 10), ("d4-c", 11)] {
        core.on_logs(
            logs_event(sig, slot, logs_with(&p, &[]), None, 1),
            0,
            &mut sink,
        )
        .unwrap();
    }
    sink.flush().unwrap();
    let mut reader = crate::analytics::journal::Reader::open(&dir.join(MARKET_JOURNAL)).unwrap();
    let mut transactions = 0;
    while let Some(f) = reader.next_frame().unwrap() {
        transactions += u64::from(f.kind == crate::analytics::journal::kind::MARKET_TRANSACTION);
    }
    assert_eq!(transactions, 3);
    fs::remove_dir_all(dir).unwrap();
}

/// D5: an unexpected Receivership in the non-receiver decoder output is a decoder error
/// (INCOMPLETE), never a panic and never counted as a provisional liquidation.
#[test]
fn unexpected_receivership_in_additional_events_is_a_decoder_error() {
    let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    core.count_provisional(truth::LiquidationType::Classic);
    core.count_provisional(truth::LiquidationType::Receivership);
    assert_eq!(core.stats.provisional_classic_events, 1);
    assert_eq!(core.stats.decoder_error_count, 1);
    assert!(core.incomplete_reasons.contains("DECODER_ERROR"));
}

#[test]
fn journal_sink_finalize_binds_the_journal_and_early_exit_seals_incomplete() {
    use super::rows::*;
    let dir = journal_test_dir("journal_finalize");
    let p = program();
    let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut sink = JournalSink::create(&dir).unwrap();
    core.on_logs(
        logs_event(
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
        stopped("TIMER"),
    )
    .unwrap();
    let summary = view(&summary);
    let binding = &summary["evidence_journal"];
    assert_eq!(binding["journal"], MARKET_JOURNAL);
    assert_eq!(binding["status"], "COMPLETE");
    assert!(report.contains(MARKET_JOURNAL));
    let on_disk: MarketFinal =
        crate::analytics::journal::read_typed_value(&dir.join(MARKET_SUMMARY_JOURNAL)).unwrap();
    assert_eq!(&view(&on_disk)["evidence_journal"], binding);
    // Capture renders no view; the journal holds STOPPED as a typed coverage frame.
    for (_, name) in MARKET_VIEWS {
        assert!(!dir.join(name).exists(), "{name}");
    }
    let mut reader = crate::analytics::journal::Reader::open(&dir.join(MARKET_JOURNAL)).unwrap();
    let mut stopped_seen = false;
    while let Some(f) = reader.next_frame().unwrap() {
        if f.kind == crate::analytics::journal::kind::MARKET_COVERAGE {
            let row: MarketCoverage = f.typed().unwrap();
            stopped_seen |= matches!(
                row,
                MarketCoverage::Window {
                    event: WindowEvent::Stopped { .. },
                    ..
                }
            );
        }
    }
    assert!(stopped_seen);
    drop(sink);
    // Early exit (for example a failed warmup): sealed INCOMPLETE.
    let early = dir.join("early");
    fs::create_dir_all(&early).unwrap();
    {
        let mut sink = JournalSink::create(&early).unwrap();
        sink.coverage(&MarketCoverage::ShadowWarmupFailed { error: "X".into() })
            .unwrap();
    }
    let mut reader = crate::analytics::journal::Reader::open(&early.join(MARKET_JOURNAL)).unwrap();
    while reader.next_frame().unwrap().is_some() {}
    assert!(reader.sealed && !reader.complete);
    fs::remove_dir_all(dir).unwrap();
}

fn produced(slot: u64) -> super::rows::MarketCoverage {
    super::rows::MarketCoverage::SlotProduced {
        slot,
        blockhash: "anchor".into(),
        parent_slot: slot - 1,
        previous_blockhash: "parent".into(),
        block_time: Some(1),
        transactions_scanned: 0,
    }
}

#[test]
fn strict_compat_sink_persists_binary_first_and_exports_at_batch_boundary() {
    let dir = journal_test_dir("strict_binary");
    let mut sink = JsonlSink::create(&dir).unwrap();
    let coverage = produced(10);
    sink.coverage(&coverage).unwrap();
    assert!(dir.join(MARKET_JOURNAL).exists());
    for (_, name) in MARKET_VIEWS {
        assert!(!dir.join(name).exists());
    }
    sink.flush().unwrap();
    sink.flush().unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("coverage.jsonl")).unwrap(),
        format!("{}\n", coverage.legacy_json())
    );
    let anchor: Value = serde_json::from_str(
        fs::read_to_string(dir.join("coverage.jsonl"))
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(
        (anchor["status"].as_str(), anchor["blockhash"].as_str()),
        (Some("PRODUCED_FETCHED"), Some("anchor"))
    );
    let mut reader = crate::analytics::journal::Reader::open(&dir.join(MARKET_JOURNAL)).unwrap();
    while reader.next_frame().unwrap().is_some() {}
    assert!(reader.sealed && reader.complete);
    // The pinned verifier finishes each batch once. Post-checkpoint additions must fail.
    assert!(sink.coverage(&coverage).is_err());
    drop(sink);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn strict_compat_export_failure_stays_failed_on_retry() {
    let dir = journal_test_dir("strict_export_failure");
    let mut sink = JsonlSink::create(&dir).unwrap();
    sink.coverage(&produced(10)).unwrap();
    fs::write(dir.join("coverage.jsonl"), b"existing evidence").unwrap();
    assert!(sink.flush().is_err());
    assert!(sink.flush().is_err());
    assert_eq!(
        fs::read(dir.join("coverage.jsonl")).unwrap(),
        b"existing evidence"
    );
    drop(sink);
    fs::remove_dir_all(dir).unwrap();
}

/// R12: the live capture retention cap is exact and fail-closed: `cap` distinct transactions
/// (successful or failed relevant ones) are admitted, a duplicate at the cap is a duplicate
/// (no overflow), the next distinct transaction marks INCOMPLETE and is counted — never a
/// silent eviction. Applied with the default cap 262,144 at 262,144 / 262,145.
#[test]
fn r12_live_retention_cap_is_exact_and_fail_closed() {
    for cap in [4usize, 262_144] {
        let (h, _rx) = crate::analytics::econ::test_handle(cap);
        let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
        core.econ = Some(h.clone());
        for i in 0..cap {
            assert!(
                core.admit_identity(&format!("sig{i}"), 10, Some(i % 3 != 0)),
                "{i}"
            );
        }
        assert_eq!(core.stats.unique_transactions as usize, cap);
        assert!(core.stats.failed_transactions > 0);
        assert!(!core.admit("sig0", 10, Some(true), 1), "duplicate");
        assert_eq!(core.stats.duplicate_transactions, 1);
        let overflow = || {
            h.shared
                .retention_overflow
                .load(std::sync::atomic::Ordering::Relaxed)
        };
        assert_eq!(overflow(), 0);
        assert!(!core
            .incomplete_reasons
            .contains("C0_E1_MARKET_RETENTION_OVERFLOW"));
        assert!(!core.admit_identity("one-more", 11, Some(true)));
        assert_eq!(overflow(), 1);
        assert!(core
            .incomplete_reasons
            .contains("C0_E1_MARKET_RETENTION_OVERFLOW"));
        assert_eq!(
            core.stats.unique_transactions as usize, cap,
            "nothing evicted"
        );
    }
}

/// D7: the per-transaction keys (`seen`, `live_receipts`) are bounded by the retention count and
/// measured at the default cap with real 88-character signatures: exact table + heap bytes per
/// retained transaction; one transaction from a second slot/receipt stays one entry with its first
/// receipt; beyond the cap INCOMPLETE, nothing evicted.
#[test]
fn d7_retained_transaction_keys_are_count_bounded_and_measured() {
    use std::mem::size_of;
    let cap = crate::analytics::econ::DEFAULT_RETENTION;
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    core.retention = cap;
    let sig = |i: usize| {
        let mut b = [0xA5u8; 64];
        b[..8].copy_from_slice(&(i as u64).to_le_bytes());
        solana_sdk::signature::Signature::from(b).to_string()
    };
    assert!(core.admit(&sig(0), 10, Some(true), 1));
    assert!(!core.admit(&sig(0), 11, Some(false), 2), "same transaction");
    assert_eq!(
        core.truth.live_receipts[&sig(0)],
        (10, 1),
        "first receipt kept"
    );
    for i in 1..cap {
        assert!(core.admit(&sig(i), 10, Some(true), 1), "{i}");
    }
    assert!(!core.admit(&sig(cap), 10, Some(true), 1));
    assert!(core
        .incomplete_reasons
        .contains("MARKET_RETENTION_OVERFLOW"));
    assert_eq!(core.seen.len(), cap, "nothing evicted");
    // hashbrown: buckets = next power of two of capacity × 8/7, one control byte per bucket.
    let table =
        |capacity: usize, entry: usize| (capacity * 8 / 7).next_power_of_two() * (entry + 1);
    let heap = |keys: &mut dyn Iterator<Item = &String>| keys.map(String::capacity).sum::<usize>();
    let seen = table(core.seen.capacity(), size_of::<String>()) + heap(&mut core.seen.iter());
    let receipts = table(
        core.truth.live_receipts.capacity(),
        size_of::<(String, (u64, u64))>(),
    ) + heap(&mut core.truth.live_receipts.keys());
    let per_tx = (seen + receipts) / cap;
    eprintln!(
        "D7_RETENTION cap={cap} signature_chars={} seen_bytes={seen} receipts_bytes={receipts} \
         per_tx_bytes={per_tx} total_mib={}",
        sig(1).len(),
        (seen + receipts) / (1024 * 1024)
    );
    assert!(per_tx <= 512, "{per_tx}");
}

/// R24 (event branch): liquidation events are retained up to the same cap; beyond it the capture
/// is INCOMPLETE (`MARKET_EVENT_RETENTION_OVERFLOW` market-only, `C0_E1_EVENT_RETENTION_OVERFLOW`
/// counted with econ), the event is not aggregated and nothing retained is evicted.
#[test]
fn r24_event_retention_is_bounded_and_fail_closed() {
    let event = |i: u32| ReceiverEvent {
        event_index: i,
        target: Pubkey::new_unique(),
        receiver: Pubkey::new_unique(),
        seized_equity_usd: 2.0,
        repaid_equity_usd: 1.0,
        protocol_flat_fee_lamports: 5,
    };
    for with_econ in [false, true] {
        let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
        core.retention = 2;
        let econ = with_econ.then(|| crate::analytics::econ::test_handle(2));
        if let Some((h, _)) = &econ {
            core.econ = Some(h.clone());
        }
        core.aggregate("tx-a", 10, Some(1), Some(0), &[event(0), event(1)]);
        let reason = if with_econ {
            "C0_E1_EVENT_RETENTION_OVERFLOW"
        } else {
            "MARKET_EVENT_RETENTION_OVERFLOW"
        };
        assert!(!core.incomplete_reasons.contains(reason));
        core.aggregate("tx-b", 11, Some(2), Some(0), &[event(0)]);
        assert!(core.incomplete_reasons.contains(reason), "{reason}");
        assert_eq!(
            (
                core.edges.len(),
                core.stats.liquidation_events,
                core.targets.len()
            ),
            (2, 2, 2),
            "bounded, nothing evicted"
        );
        if let Some((h, _)) = &econ {
            assert_eq!(
                h.shared
                    .retention_overflow
                    .load(std::sync::atomic::Ordering::Relaxed),
                1
            );
        }
    }
}

/// R24: a market-only capture (no econ handle) is bounded by its configured retention
/// (`P0_MARKET_RETENTION`, default the shared 262,144): at the cap a new transaction marks the
/// capture INCOMPLETE and nothing retained is evicted; the setting is validated.
#[test]
fn r24_market_only_retention_is_configured_bounded_and_fail_closed() {
    let base = |extra: &[(&'static str, &'static str)]| {
        let mut v = vec![
            ("P0_WS_URL", "wss://h"),
            (
                "P0_PROGRAM_ID",
                darkbot_project0::protocol::PROGRAM_ID_MAINNET,
            ),
        ];
        v.extend_from_slice(extra);
        MarketConfig::from_lookup(&env_of(&v), 0)
    };
    assert_eq!(
        base(&[]).unwrap().retention,
        crate::analytics::econ::DEFAULT_RETENTION
    );
    assert_eq!(base(&[("P0_MARKET_RETENTION", "3")]).unwrap().retention, 3);
    assert!(base(&[("P0_MARKET_RETENTION", "0")]).is_err());
    assert!(base(&[("P0_MARKET_RETENTION", "2000001")]).is_err());
    let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
    core.retention = 3;
    for i in 0..3 {
        assert!(core.admit(&format!("sig{i}"), 10, Some(true), 1));
    }
    assert!(
        !core.admit("sig0", 10, Some(true), 1),
        "duplicate, no overflow"
    );
    assert!(!core
        .incomplete_reasons
        .contains("MARKET_RETENTION_OVERFLOW"));
    assert!(!core.admit("sig3", 10, Some(true), 1));
    assert!(core
        .incomplete_reasons
        .contains("MARKET_RETENTION_OVERFLOW"));
    assert!(core.truth.retention_error);
    assert_eq!(core.stats.unique_transactions, 3, "nothing evicted");
    assert!(core.seen.len() <= 3, "bounded");
}

/// A01: the reconnects of the ingress session reach the Capture projection with its health.
#[test]
fn a01_source_health_reconnects_reach_the_capture_projection() {
    use darkbot_solana::ingress::SourceHealth;
    let tap = crate::status_tap::StatusTap::new();
    let mut reader = tap.reader();
    let mut core = ObserverCore::new(program(), MarketSource::Logs, EnrichConfig::DEFAULT);
    core.status_tap = Some(tap);
    core.start_observation(1, 4_000_000_001);
    let health = IngressEvent::Health(SourceHealth {
        source: SourceKind::WsRpcControl,
        connected: true,
        last_frame_age_ns: None,
        frames: 3,
        reconnects: 2,
        last_slot: None,
        t_rx_ns: 100,
    });
    core.on_ingress_event(health, 0, &mut VecSink::default())
        .unwrap();
    let s = reader
        .wait_changed(std::time::Duration::from_millis(5))
        .unwrap();
    assert_eq!(s.transport.unwrap().reconnects, 2);
}

/// DEC-16: an unbounded raw capture (`until-stopped`, admitted with its rolling binding) against
/// a loopback WebSocket rolls its windows: each boundary seals a market segment and journals the
/// window's verdict; while its gates hold the capture goes on, and the first window that finds
/// no valid provider evidence halts it — drained, sealed INCOMPLETE, the run result names the
/// HALT. Ordered by the journal, never by timing: the evidence goes once a window held.
#[test]
fn dec16_an_unbounded_capture_rolls_its_windows_and_halts_at_its_first_violated_gate() {
    use crate::orchestration::provider_evidence::{self as pe, ProviderEvidence, ProviderProof};
    use crate::orchestration::until_stopped as us;
    use std::time::{Duration, Instant};
    use tungstenite::Message;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        let (mut acked, mut slot_subscription, mut slot) = (0, None, 1_000u64);
        loop {
            match ws.read() {
                Ok(Message::Text(text)) => {
                    let request: Value = serde_json::from_str(&text).unwrap();
                    let id = request["id"].as_u64().unwrap();
                    if request["method"] == "slotSubscribe" {
                        slot_subscription = Some(id);
                    }
                    acked += 1;
                    let ack = json!({"jsonrpc": "2.0", "id": id, "result": id});
                    if ws.send(Message::Text(ack.to_string())).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break,
            }
            if let (2, Some(subscription)) = (acked, slot_subscription) {
                slot += 1;
                let notification = json!({"jsonrpc": "2.0", "method": "slotNotification",
                    "params": {"subscription": subscription,
                        "result": {"slot": slot, "parent": slot - 1, "root": slot - 2}}});
                if ws.send(Message::Text(notification.to_string())).is_err() {
                    break;
                }
            }
        }
    });
    let root = std::env::temp_dir().join(format!(
        "dec16_capture_{}_{}",
        std::process::id(),
        utc_compact(unix_now_s().unwrap())
    ));
    let _ = fs::remove_dir_all(&root);
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    let config = darkbot_solana::runtime_config::RuntimeConfig {
        rpc_endpoints: vec!["https://rpc.invalid".into()],
        ..Default::default()
    };
    // Provider evidence covering the 2-s runway of 1-s windows, in the controller's store.
    let evidence = ProviderEvidence {
        providers: vec![ProviderProof {
            policy: "FREE_ONLY".into(),
            passed: true,
            provider_binding: crate::analytics::journal::hex(
                &solana_sdk::hash::hash(b"https://rpc.invalid").to_bytes(),
            ),
            measured_at_unix_s: unix_now_s().unwrap() as f64,
            evidence_sha256: Some("a".repeat(64)),
            allowed_rps: config.limits.rpc_rps as f64,
            available_calls: config.limits.max_requests * 2,
        }],
        wss_provider_binding: None,
        wss_free_only: true,
        archive_qualified: false,
        source_sha256: "s".into(),
    };
    let file = root.join("evidence.p0j");
    crate::analytics::journal::write_typed_value(&file, 0, &evidence).unwrap();
    pe::import(&project, &file).unwrap();
    let rolling = us::Rolling {
        project: project.clone(),
        binding: us::tests::binding(1, 2),
        qualification_id: "0123456789abcdef".into(),
        config,
    };
    let out = root.join("out").to_string_lossy().to_string();
    let cfg = MarketConfig::from_lookup(
        &env_of(&[
            ("P0_WS_URL", url.as_str()),
            (
                "P0_PROGRAM_ID",
                darkbot_project0::protocol::PROGRAM_ID_MAINNET,
            ),
            ("P0_MARKET_SOURCE", "logs"),
            ("P0_MARKET_OUT_DIR", out.as_str()),
            ("P0_MARKET_RUN_ID", "unbounded"),
            ("P0_MARKET_DURATION_SECS", "until-stopped"),
            ("P0_MARKET_READY_TIMEOUT_SECS", "10"),
            ("P0_MARKET_DRAIN_TIMEOUT_SECS", "5"),
            ("P0_MARKET_HEARTBEAT_SECS", "1"),
        ]),
        unix_now_s().unwrap(),
    )
    .unwrap();
    let run_dir = cfg.run_dir.clone();
    let capture = std::thread::spawn(move || {
        super::capture::run_published(cfg, Clock::try_new().unwrap(), None, None, Some(rolling))
    });
    let windows = run_dir.join(us::WINDOWS_JOURNAL);
    let verdicts = || -> Vec<us::WindowCheck> {
        let Ok(mut r) = crate::storage::segments::ChainReader::open(&windows) else {
            return Vec::new();
        };
        let mut seen = Vec::new();
        while let Ok(Some(f)) = r.next_frame() {
            if f.kind == crate::analytics::journal::kind::UNTIL_STOPPED_WINDOW {
                seen.push(f.typed().unwrap());
            }
        }
        seen
    };
    let waiting = Instant::now();
    while verdicts().is_empty() {
        assert!(
            waiting.elapsed() < Duration::from_secs(30),
            "no window closed"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        verdicts()[0].verdict,
        us::CONTINUE,
        "{}",
        verdicts()[0].detail
    );
    fs::remove_dir_all(project.join(pe::STORE)).unwrap();
    let result = capture.join().unwrap();
    server.join().unwrap();
    assert_eq!(
        result,
        Err("MARKET_CAPTURE_INCOMPLETE: UNTIL_STOPPED_HALT".into())
    );
    let all = verdicts();
    let (last, held) = all.split_last().unwrap();
    assert_eq!(last.verdict, "UNTIL_STOPPED_PROVIDER_EVIDENCE_MISSING");
    assert!(held.iter().all(|w| w.verdict == us::CONTINUE), "{all:?}");
    for (i, w) in all.iter().enumerate() {
        assert_eq!(w.window, i as u64 + 1);
        // Each boundary sealed a market segment, re-verified by its window.
        let sealed = w.sealed.as_ref().unwrap();
        assert!(run_dir.join(&sealed.file).is_file(), "{}", sealed.file);
    }
    let chain = crate::storage::segments::verify_chain(&windows).unwrap();
    assert!(
        chain.sealed && !chain.complete,
        "a HALT never completes the run: {chain:?}"
    );
    let market = crate::storage::segments::verify_chain(&run_dir.join(MARKET_JOURNAL)).unwrap();
    assert!(market.segments as usize > all.len() && market.sealed && !market.complete);
    let _ = fs::remove_dir_all(&root);
}

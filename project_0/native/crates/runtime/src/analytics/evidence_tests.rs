use super::*;
use std::sync::Barrier;

fn directory() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "p0-owner-evidence-{}-{}",
        std::process::id(),
        Pubkey::new_unique()
    ));
    std::fs::create_dir(&p).unwrap();
    p
}
fn event(reason: &str) -> OwnerEvidence {
    OwnerEvidence::Record(RecordEvent::IntegrationSkip {
        account: Pubkey::default(),
        reason: reason.into(),
    })
}
fn paused(dir: &std::path::Path, read_only: bool) -> (OwnerEvidenceWriter, Arc<Barrier>) {
    let mut journal = Writer::create(&dir.join("events.p0j")).unwrap();
    if read_only {
        journal.test_read_only();
    }
    let recovery = dir.join("recovery.p0j");
    let (tx, rx) = mpsc::sync_channel(1);
    let (terminal, terminal_rx) = mpsc::channel();
    let failed = Arc::new(AtomicBool::new(false));
    let signal = failed.clone();
    let gate = Arc::new(Barrier::new(2));
    let worker_gate = gate.clone();
    let worker = std::thread::spawn(move || {
        worker_gate.wait();
        run_writer(journal, recovery, signal, rx, terminal_rx)
    });
    (
        OwnerEvidenceWriter {
            tx: Some(tx),
            terminal: Some(terminal),
            worker: Some(worker),
            failed,
            refused: None,
            admission_error: None,
        },
        gate,
    )
}
#[test]
fn full_queue_retains_refused_record_drains_fifo_and_fails_closed() {
    let dir = directory();
    let (mut w, gate) = paused(&dir, false);
    let first = w.emit(event("first"));
    let second = w.emit(event("second"));
    gate.wait();
    assert!(first.is_ok());
    assert_eq!(second.unwrap_err().0, "OWNER_EVIDENCE_OVERFLOW");
    assert_eq!(
        w.finish(Some(DaySummary::default())).unwrap_err().0,
        "OWNER_EVIDENCE_OVERFLOW"
    );
    let rows = journal::values(&dir.join("events.p0j")).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["reason"], "first");
    assert_eq!(rows[1]["reason"], "second");
    let mut reader = journal::Reader::open(&dir.join("events.p0j")).unwrap();
    while reader.next_frame().unwrap().is_some() {}
    assert!(reader.sealed);
    assert!(!reader.complete);
    assert!(!dir.join("recovery.p0j").exists());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn real_write_failure_preserves_queue_refusal_and_final_summary_in_recovery() {
    let dir = directory();
    let (mut w, gate) = paused(&dir, true);
    let _ = w.emit(event("first"));
    let _ = w.emit(event("second"));
    gate.wait();
    assert_eq!(
        w.finish(Some(DaySummary::default())).unwrap_err().0,
        "OWNER_EVIDENCE_WRITE_FAILED_RECOVERY_SAVED"
    );
    let rows = journal::values(&dir.join("recovery.p0j")).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["event"]["reason"], "first");
    assert_eq!(rows[1]["event"]["reason"], "second");
    assert_eq!(rows[2]["event_kind"], 103);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn early_error_drop_joins_writer_and_does_not_abandon_accepted_records() {
    let dir = directory();
    let (mut w, gate) = paused(&dir, false);
    let sent = w.emit(event("accepted"));
    gate.wait();
    assert!(sent.is_ok());
    drop(w);
    assert_eq!(
        journal::values(&dir.join("events.p0j")).unwrap()[0]["reason"],
        "accepted"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn recovery_destination_collision_is_not_overwritten_or_reported_as_success() {
    let dir = directory();
    std::fs::write(dir.join("recovery.p0j"), b"existing").unwrap();
    let (mut w, gate) = paused(&dir, true);
    let _ = w.emit(event("failed"));
    gate.wait();
    assert_eq!(
        w.finish(None).unwrap_err().0,
        "OWNER_EVIDENCE_RECOVERY_FAILED"
    );
    assert_eq!(
        std::fs::read(dir.join("recovery.p0j")).unwrap(),
        b"existing"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
fn parity(event: RecordEvent, expected: Value) {
    let (_, value) = OwnerEvidence::Record(event).encode().unwrap();
    let bytes =
        serde_json::to_vec(&journal::decode(&journal::encode(&value).unwrap()).unwrap()).unwrap();
    assert_eq!(bytes, format!("{expected}").as_bytes());
}
#[test]
fn legacy_jsonl_byte_parity_all_structured_record_variants() {
    let account = Pubkey::default();
    let text = "quoted \"reason\"\n\\雪";
    parity(
        RecordEvent::IntegrationPlan {
            account,
            trigger: "account",
            slot: 3,
            reason: text.into(),
        },
        json!({"account":account.to_string(),"trigger":"account","slot":3,"decision":"Skip","loss":"IntegrationRefresh","reason":text}),
    );
    parity(
        RecordEvent::IntegrationSkip {
            account,
            reason: text.into(),
        },
        json!({"account":account.to_string(),"decision":"Skip","loss":"IntegrationRefresh","reason":text}),
    );
    parity(
        RecordEvent::IntegrationPending {
            account,
            trigger: "bank",
            slot: 3,
            integration_kinds: vec!["kamino"],
            pricing_integration_kinds: vec![],
        },
        json!({"account":account.to_string(),"trigger":"bank","slot":3,"decision":"Skip","loss":"IntegrationRefresh","reason":"integration projection pending","integration_kinds":["kamino"],"pricing_integration_kinds":[]}),
    );
    parity(
        RecordEvent::ProjectionReady {
            account,
            base_slot: 2,
            projection_slot: 3,
            projection_unix_s: 4,
            integration_kinds: vec![],
            pricing_integration_kinds: vec![],
            simulated_cu: None,
        },
        json!({"projection":{"account":account.to_string(),"base_slot":2,"projection_slot":3,"projection_unix_s":4,"status":"ready","integration_kinds":[],"pricing_integration_kinds":[],"simulated_cu":null}}),
    );
    parity(
        RecordEvent::ProjectionStale {
            account,
            base_slot: 2,
            projection_slot: 3,
            canonical_matches: false,
        },
        json!({"projection":{"account":account.to_string(),"base_slot":2,"projection_slot":3,"status":"stale","canonical_matches":false}}),
    );
    parity(
        RecordEvent::ProjectionFailed {
            account,
            base_slot: 2,
            error: text.into(),
        },
        json!({"projection":{"account":account.to_string(),"base_slot":2,"status":"failed","error":text}}),
    );
    let winner = Winner {
        slot: 3,
        identity: crate::market::winner::WinnerIdentity::SameSlotAmbiguous { candidates: vec![] },
    };
    let expected = json!({"race":{"account":account.to_string(),"winner":winner,"our_sign_slot":null,"outcome":RaceOutcome::SameSlotUnknown,"expected_net":i128::MAX}});
    parity(
        RecordEvent::Race {
            account,
            winner,
            our_sign_slot: None,
            outcome: RaceOutcome::SameSlotUnknown,
            expected_net: Some(i128::MAX),
        },
        expected,
    );
}
#[test]
fn health_full_i128_and_unknown_variants_keep_exact_wire_contract() {
    let account = Pubkey::default();
    for bits in [
        i128::MIN,
        i64::MIN as i128 - 1,
        0,
        u64::MAX as i128 + 1,
        i128::MAX,
    ] {
        parity(
            RecordEvent::Health {
                account,
                trigger: "bank",
                t_rx_ns: 1,
                slot: 2,
                evaluation_slot: 3,
                evaluation_unix_s: 4,
                result: HealthEvidence::Known {
                    liquidatable: true,
                    maintenance_health: bits,
                    equity_assets: bits,
                    bad_debt: false,
                },
            },
            json!({"account":account.to_string(),"trigger":"bank","t_rx_ns":1,"slot":2,"evaluation_slot":3,"evaluation_unix_s":4,"liquidatable":true,"maint_health_bits":bits,"equity_assets_bits":bits,"bad_debt":false}),
        );
    }
    for (result, field, value) in [
        (HealthEvidence::Unavailable, "decision", "Unavailable"),
        (
            HealthEvidence::Error("bad \"oracle\"".into()),
            "error",
            "bad \"oracle\"",
        ),
    ] {
        let mut expected = json!({"account":account.to_string(),"trigger":"bank","t_rx_ns":1,"slot":2,"evaluation_slot":3,"evaluation_unix_s":4});
        expected[field] = json!(value);
        parity(
            RecordEvent::Health {
                account,
                trigger: "bank",
                t_rx_ns: 1,
                slot: 2,
                evaluation_slot: 3,
                evaluation_unix_s: 4,
                result,
            },
            expected,
        );
    }
}
#[test]
fn owner_admission_has_no_json_or_direct_file_writes_and_all_exits_drain() {
    for source in [
        include_str!("../orchestration/state_owner/detect.rs"),
        include_str!("../orchestration/state_owner/remote_apply.rs"),
    ] {
        for forbidden in [
            "serde_json",
            "writeln!",
            "self.records",
            "self.summaries",
            ".write_all(",
            ".flush(",
            ".sync_all(",
        ] {
            assert!(!source.contains(forbidden), "{forbidden}");
        }
    }
    let owner = include_str!("../orchestration/state_owner.rs");
    assert!(owner.contains("let finish = owner.finish();"));
    assert!(owner.contains("owner.evidence.check()?"));
}

/// PERSIST-02 over the whole owner loop: no JSON, file access, flush/fsync, environment read
/// or direct stderr/stdout write. Diagnostics leave through the bounded evidence queue.
/// Excluded: setup (before the loop), ingress (its own thread) and the test module.
#[test]
fn owner_thread_sources_have_no_json_io_env_or_blocking_logging() {
    let owner = [
        (
            "state_owner.rs",
            include_str!("../orchestration/state_owner.rs"),
        ),
        (
            "accounts.rs",
            include_str!("../orchestration/state_owner/accounts.rs"),
        ),
        (
            "candidates.rs",
            include_str!("../orchestration/state_owner/candidates.rs"),
        ),
        (
            "detect.rs",
            include_str!("../orchestration/state_owner/detect.rs"),
        ),
        (
            "dispatch.rs",
            include_str!("../orchestration/state_owner/dispatch.rs"),
        ),
        (
            "events.rs",
            include_str!("../orchestration/state_owner/events.rs"),
        ),
        (
            "lifecycle.rs",
            include_str!("../orchestration/state_owner/lifecycle.rs"),
        ),
        (
            "metrics.rs",
            include_str!("../orchestration/state_owner/metrics.rs"),
        ),
        (
            "remote_apply.rs",
            include_str!("../orchestration/state_owner/remote_apply.rs"),
        ),
        (
            "shutdown.rs",
            include_str!("../orchestration/state_owner/shutdown.rs"),
        ),
        (
            "sweep.rs",
            include_str!("../orchestration/state_owner/sweep.rs"),
        ),
        (
            "telemetry.rs",
            include_str!("../orchestration/state_owner/telemetry.rs"),
        ),
    ];
    for (name, source) in owner {
        for forbidden in [
            "eprintln!",
            "println!",
            "writeln!",
            "serde_json",
            "std::fs",
            "OpenOptions",
            ".write_all(",
            ".flush(",
            ".sync_all(",
            "env::var",
            "config::parsed_or",
            "config::require",
        ] {
            // accounts.rs builds the WSS subscription filters: an external JSON-RPC contract.
            if name == "accounts.rs" && forbidden == "serde_json" {
                continue;
            }
            assert!(!source.contains(forbidden), "{name}: {forbidden}");
        }
    }
    // Code the owner calls during an evaluation or a state recovery.
    for (name, source) in [
        ("pipeline.rs", include_str!("../execution/pipeline.rs")),
        ("builder.rs", include_str!("../execution/builder.rs")),
        ("route.rs", include_str!("../execution/route.rs")),
        (
            "recovery.rs",
            include_str!("../../../solana/src/state/recovery.rs"),
        ),
    ] {
        let production = source.split("#[cfg(test)]").next().unwrap();
        for forbidden in ["eprintln!", "println!"] {
            assert!(!production.contains(forbidden), "{name}: {forbidden}");
        }
    }
}

#[test]
fn owner_diagnostics_are_journaled_before_they_are_printed() {
    let dir = std::env::temp_dir().join(format!("p0-diag-{}", Pubkey::new_unique()));
    std::fs::create_dir(&dir).unwrap();
    let mut w =
        OwnerEvidenceWriter::start(dir.join("events.p0j"), dir.join("recovery.p0j"), 4).unwrap();
    w.diagnostic("oracle_trigger_stats: a=1".into()).unwrap();
    w.finish(None).unwrap();
    let mut reader = journal::Reader::open(&dir.join("events.p0j")).unwrap();
    let frame = reader.next_frame().unwrap().unwrap();
    assert_eq!(frame.kind, journal::kind::OWNER_DIAGNOSTIC);
    assert_eq!(frame.value["line"], "oracle_trigger_stats: a=1");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ordinary_writer_preserves_opportunity_reconcile_and_summary_bytes() {
    use crate::analytics::record::{Decision, TimingEvidence};
    let record = OpportunityRecord {
        progress: Default::default(),
        market_plan_identity: None,
        timing: TimingEvidence {
            t_rx_ns: Some(10),
            t_state_ns: Some(15),
            t_detect_ns: Some(20),
            t_ready_ns: Some(30),
            t_sign_ns: Some(40),
            t_handoff_ns: None,
        },
        rx_slot: Some(1),
        detect_slot: Some(2),
        selected_costs: None,
        simulation_result: Some("PASS".into()),
        account: Pubkey::new_unique().to_string(),
        trigger: "test".into(),
        decision: Decision::ShadowReady,
        net_profit: Some(i128::MAX),
        economics_known: true,
        premium_bps: None,
        t_profitable_unix_s: None,
        tx_version: Some("V0".into()),
        serialized_bytes: Some(100),
        locked_accounts: Some(3),
        path: Some("RPC".into()),
        funding: Some("ZERO_PREFUND_SAME_TX".into()),
        signature: None,
        sign_slot: Some(3),
        simulated_cu: Some(100),
        candidate_costs: vec![],
        exposure: None,
        all_signatures: vec![],
        bundle_id: None,
        submitted_tip: None,
        intervals_ns: vec![],
        reason: None,
        loss: None,
    };
    let reconciled = ReconcileRecord {
        signature: "signed".into(),
        path: "RPC",
        outcome: "UNKNOWN".into(),
        landed_slot: None,
        fee_lamports: None,
        compute_units_consumed: None,
        paid_tip_lamports: None,
        network_and_tip_lamports: None,
        error: Some("quoted \"reason\"".into()),
        exposure_released: 0,
        pnl_complete: false,
    };
    let summary = DaySummary {
        expected_net_wins_sum: i128::MAX,
        ..DaySummary::default()
    };
    let expected_records = format!(
        "{}\n{}\n",
        serde_json::to_string(&record).unwrap(),
        serde_json::to_string(&reconciled).unwrap()
    );
    let expected_summary = format!("{}\n", serde_json::to_string(&summary).unwrap());
    let dir = directory();
    let path = dir.join("events.p0j");
    let mut writer = OwnerEvidenceWriter::start(path.clone(), dir.join("recovery.p0j"), 4).unwrap();
    writer
        .emit(OwnerEvidence::Opportunity(Box::new(record)))
        .unwrap();
    writer.emit(OwnerEvidence::Reconcile(reconciled)).unwrap();
    writer.finish(Some(summary)).unwrap();
    let values = journal::values(&path).unwrap();
    // JSON object order is canonical in the binary codec; field values and nulls remain exact.
    let expected: Vec<Value> = expected_records
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(&values[..2], &expected);
    assert_eq!(
        values[2],
        serde_json::from_str::<Value>(&expected_summary).unwrap()
    );
    assert!(!dir.join("recovery.p0j").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

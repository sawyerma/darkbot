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
/// Data frames of a journal (manifest excluded).
fn frames(path: &std::path::Path) -> Vec<journal::Frame> {
    let mut r = journal::Reader::open(path).unwrap();
    std::iter::from_fn(|| r.next_frame().unwrap())
        .filter(|f| f.kind != journal::MANIFEST)
        .collect()
}
fn reason(event: RecordEvent) -> String {
    match event {
        RecordEvent::IntegrationSkip { reason, .. } => reason,
        other => panic!("{other:?}"),
    }
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
    let syncs = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counted = syncs.clone();
    let worker = std::thread::spawn(move || {
        worker_gate.wait();
        run_writer(
            journal,
            recovery,
            signal,
            rx,
            terminal_rx,
            std::time::Duration::from_secs(60),
            counted,
        )
    });
    (
        OwnerEvidenceWriter {
            tx: Some(tx),
            terminal: Some(terminal),
            worker: Some(worker),
            failed,
            refused: None,
            admission_error: None,
            syncs,
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
    let rows = frames(&dir.join("events.p0j"));
    assert_eq!(rows.len(), 3);
    assert_eq!(reason(rows[0].typed().unwrap()), "first");
    assert_eq!(reason(rows[1].typed().unwrap()), "second");
    assert_eq!(
        rows[2].typed::<DaySummary>().unwrap(),
        DaySummary::default()
    );
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
    let rows: Vec<OwnerRecovery> = frames(&dir.join("recovery.p0j"))
        .iter()
        .map(|f| f.typed().unwrap())
        .collect();
    assert_eq!(rows.len(), 3);
    let saved = |r: &OwnerRecovery| -> RecordEvent {
        payload::decode(r.event_kind, r.event_schema, &r.event).unwrap()
    };
    assert_eq!(reason(saved(&rows[0])), "first");
    assert_eq!(reason(saved(&rows[1])), "second");
    assert_eq!(rows[2].event_kind, journal::kind::OWNER_SUMMARY);
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
        reason(frames(&dir.join("events.p0j"))[0].typed().unwrap()),
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
/// JFI-02: every owner record variant round-trips through its typed binary payload exactly,
/// with the frame provenance taken from the event itself (no JSON lookup).
fn parity(event: RecordEvent, slot: Option<u64>, rx_ns: Option<u64>) {
    let e = OwnerEvidence::Record(event.clone()).encode().unwrap();
    assert_eq!(
        (e.kind, e.schema),
        (journal::kind::OWNER_RECORD, payload::TYPED)
    );
    assert_eq!((e.slot, e.rx_ns), (slot, rx_ns));
    let decoded: RecordEvent = payload::decode(e.kind, e.schema, &e.bytes).unwrap();
    assert_eq!(decoded, event);
    assert_eq!(payload::encode(&decoded).unwrap(), e.bytes);
    assert!(payload::decode::<RecordEvent>(e.kind, e.schema + 1, &e.bytes).is_err());
    assert!(
        payload::decode::<RecordEvent>(e.kind, e.schema, &e.bytes[..e.bytes.len() - 1]).is_err()
    );
}
#[test]
fn typed_payload_round_trip_all_structured_record_variants() {
    let account = Pubkey::default();
    let text = "quoted \"reason\"\n\\雪";
    parity(
        RecordEvent::IntegrationPlan {
            account,
            trigger: "account".into(),
            slot: 3,
            reason: text.into(),
        },
        Some(3),
        None,
    );
    parity(
        RecordEvent::IntegrationSkip {
            account,
            reason: text.into(),
        },
        None,
        None,
    );
    parity(
        RecordEvent::IntegrationPending {
            account,
            trigger: "bank".into(),
            slot: 3,
            integration_kinds: vec!["kamino".into()],
            pricing_integration_kinds: vec![],
        },
        Some(3),
        None,
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
        None,
        None,
    );
    parity(
        RecordEvent::ProjectionStale {
            account,
            base_slot: 2,
            projection_slot: 3,
            canonical_matches: false,
        },
        None,
        None,
    );
    parity(
        RecordEvent::ProjectionFailed {
            account,
            base_slot: 2,
            error: text.into(),
        },
        None,
        None,
    );
    let winner = Winner {
        slot: 3,
        identity: crate::market::winner::WinnerIdentity::SameSlotAmbiguous { candidates: vec![] },
    };
    parity(
        RecordEvent::Race {
            account,
            winner: (&winner).into(),
            our_sign_slot: None,
            outcome: RaceOutcome::SameSlotUnknown,
            expected_net: Some(i128::MAX),
        },
        None,
        None,
    );
}
#[test]
fn health_full_i128_and_unknown_variants_round_trip_exactly() {
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
                trigger: "bank".into(),
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
            Some(2),
            Some(1),
        );
    }
    for result in [
        HealthEvidence::Unavailable,
        HealthEvidence::Error("bad \"oracle\"".into()),
    ] {
        parity(
            RecordEvent::Health {
                account,
                trigger: "bank".into(),
                t_rx_ns: 1,
                slot: 2,
                evaluation_slot: 3,
                evaluation_unix_s: 4,
                result,
            },
            Some(2),
            Some(1),
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
            "sources.rs",
            include_str!("../orchestration/state_owner/sources.rs"),
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
    let mut w = OwnerEvidenceWriter::start(
        dir.join("events.p0j"),
        dir.join("recovery.p0j"),
        4,
        std::time::Duration::from_secs(60),
    )
    .unwrap();
    w.diagnostic("oracle_trigger_stats: a=1".into()).unwrap();
    w.finish(None).unwrap();
    let mut reader = journal::Reader::open(&dir.join("events.p0j")).unwrap();
    let frame = reader.next_frame().unwrap().unwrap();
    assert_eq!(frame.kind, journal::kind::OWNER_DIAGNOSTIC);
    let line: OwnerDiagnostic = frame.typed().unwrap();
    assert_eq!(line.line, "oracle_trigger_stats: a=1");
    std::fs::remove_dir_all(dir).unwrap();
}

fn opportunity_fixture() -> OpportunityRecord {
    use crate::analytics::record::{Decision, TimingEvidence};
    OpportunityRecord {
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
    }
}
#[test]
fn ordinary_writer_preserves_opportunity_reconcile_and_summary_bytes() {
    let record = opportunity_fixture();
    let reconciled = ReconcileRecord {
        signature: "signed".into(),
        path: "RPC".into(),
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
    let dir = directory();
    let path = dir.join("events.p0j");
    let mut writer = OwnerEvidenceWriter::start(
        path.clone(),
        dir.join("recovery.p0j"),
        4,
        std::time::Duration::from_secs(60),
    )
    .unwrap();
    let record_bytes = payload::encode(&record).unwrap();
    let reconciled_bytes = payload::encode(&reconciled).unwrap();
    let summary_bytes = payload::encode(&summary).unwrap();
    writer
        .emit(OwnerEvidence::Opportunity(Box::new(record)))
        .unwrap();
    writer.emit(OwnerEvidence::Reconcile(reconciled)).unwrap();
    writer.finish(Some(summary.clone())).unwrap();
    // JFI-02: the frames carry the typed payloads byte for byte and decode back exactly.
    let mut reader = journal::Reader::open(&path).unwrap();
    let frames: Vec<_> = std::iter::from_fn(|| reader.next_frame().unwrap()).collect();
    assert_eq!(frames[0].payload, record_bytes);
    assert_eq!(frames[1].payload, reconciled_bytes);
    assert_eq!(frames[2].payload, summary_bytes);
    let opportunity: OpportunityRecord = frames[0].typed().unwrap();
    assert_eq!(payload::encode(&opportunity).unwrap(), record_bytes);
    let _: ReconcileRecord = frames[1].typed().unwrap();
    assert_eq!(frames[2].typed::<DaySummary>().unwrap(), summary);
    assert!(frames[0].legacy_value().is_err());
    assert!(reader.complete);
    assert!(!dir.join("recovery.p0j").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

/// F1: the bounded simulation logs of a failed candidate are a typed owner frame of their own
/// kind (slot provenance from the record), decoded back exactly.
#[test]
fn f1_simulation_failure_is_a_typed_owner_frame() {
    let failure = crate::analytics::record::SimulationFailureRecord {
        account: Pubkey::new_unique().to_string(),
        rx_slot: Some(42),
        failure: darkbot_solana::submit::SimulationFailure {
            code: "RPC_SIMULATION_FAILED InstructionError(2, Custom(6009))".into(),
            units_consumed: Some(31_337),
            logs: vec!["Program log: Error: Insufficient funds".into()],
            log_lines: Some(9),
            logs_truncated: false,
        },
    };
    let dir = directory();
    let path = dir.join("events.p0j");
    let mut writer = OwnerEvidenceWriter::start(
        path.clone(),
        dir.join("recovery.p0j"),
        4,
        std::time::Duration::from_secs(60),
    )
    .unwrap();
    writer
        .emit(OwnerEvidence::SimulationFailure(Box::new(failure.clone())))
        .unwrap();
    writer.finish(None).unwrap();
    let mut reader = journal::Reader::open(&path).unwrap();
    let frame = reader.next_frame().unwrap().unwrap();
    assert_eq!(frame.kind, journal::kind::OWNER_SIMULATION_FAILURE);
    assert_eq!(frame.provenance.slot, Some(42));
    let back: crate::analytics::record::SimulationFailureRecord = frame.typed().unwrap();
    assert_eq!(back, failure);
    std::fs::remove_dir_all(dir).unwrap();
}

/// JFI-02 local measurement (`--ignored`): writer-thread cost per owner opportunity frame, the
/// former JSON value path against the typed payload path. Synthetic, not a Mainnet measurement.
#[test]
#[ignore]
fn jfi02_owner_opportunity_writer_cost_typed_vs_json_value() {
    use std::time::Instant;
    let record = opportunity_fixture();
    let pct = |v: &mut Vec<u64>, q: f64| {
        v.sort_unstable();
        v[((v.len() as f64 - 1.0) * q).round() as usize]
    };
    for typed in [false, true] {
        let dir = directory();
        let path = dir.join("bench.p0j");
        let mut w = Writer::create(&path).unwrap();
        let mut lat = Vec::with_capacity(50_000);
        for _ in 0..50_000 {
            let t = Instant::now();
            if typed {
                w.append_typed(Provenance::default(), &record).unwrap();
            } else {
                let value = serde_json::to_value(&record).unwrap();
                // The former value-tree path, as historical journals were written.
                w.test_append_historical(journal::kind::OWNER_OPPORTUNITY, &value)
                    .unwrap();
            }
            lat.push(t.elapsed().as_nanos() as u64);
        }
        w.seal(true).unwrap();
        let bytes = std::fs::metadata(&path).unwrap().len() / 50_000;
        println!(
            "JFI02_OWNER_OPPORTUNITY path={} p50_ns={} p95_ns={} p99_ns={} max_ns={} bytes_per_frame={bytes}",
            if typed { "typed" } else { "json_value" },
            pct(&mut lat, 0.5),
            pct(&mut lat, 0.95),
            pct(&mut lat, 0.99),
            pct(&mut lat, 1.0)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

/// M2: the owner journal is synced on the interval also without new events.
#[test]
fn owner_evidence_syncs_periodically() {
    let dir = std::env::temp_dir().join(format!("p0-sync-{}", Pubkey::new_unique()));
    std::fs::create_dir(&dir).unwrap();
    let mut w = OwnerEvidenceWriter::start(
        dir.join("events.p0j"),
        dir.join("recovery.p0j"),
        4,
        std::time::Duration::from_millis(20),
    )
    .unwrap();
    let t0 = std::time::Instant::now();
    while w.syncs() < 3 {
        assert!(
            t0.elapsed() < std::time::Duration::from_secs(5),
            "{}",
            w.syncs()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    w.finish(None).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

/// M2: a failed periodic sync is a sticky writer failure (HALT), like a failed append.
#[test]
fn owner_evidence_sync_failure_is_sticky() {
    let dir = std::env::temp_dir().join(format!("p0-syncfail-{}", Pubkey::new_unique()));
    std::fs::create_dir(&dir).unwrap();
    let mut journal = journal::Writer::create(&dir.join("events.p0j")).unwrap();
    journal.test_fail_next_sync(std::io::ErrorKind::Other);
    let mut w = OwnerEvidenceWriter::start_with(
        journal,
        dir.join("recovery.p0j"),
        4,
        std::time::Duration::from_millis(20),
    )
    .unwrap();
    let t0 = std::time::Instant::now();
    while w.check().is_ok() {
        assert!(t0.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(w.check().unwrap_err().0, "OWNER_EVIDENCE_WRITER_FAILED");
    assert!(w.diagnostic("after".into()).is_err());
    assert!(w.finish(None).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

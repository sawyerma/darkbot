// A2 (included in the canonical `regression` tests): race margins under a finalized market
// receipt, the canonical winner-slot margin live and offline, and the readable earlier schemas.

/// Slot receipts of one capture: slot 10 twice (first one counts), slot 11, and a receipt of
/// slot 10 before the window (`t_rx` 0 < start 1) that neither side may use.
const A2_RECEIPTS: [(u64, u64); 4] = [(10, 60), (10, 90), (11, 70), (10, 0)];

/// The market journal of these receipts through the capture's own writer, with its start record.
fn a2_market_journal(dir: &Path, commitment: &str) -> PathBuf {
    use crate::market_observer::rows::{CaptureStart, WindowEvent};
    use crate::market_observer::{EnrichConfig, JournalSink, MarketSink, MarketSource, ObserverCore};
    use darkbot_solana::ingress::{IngressEvent, SlotStatus, SlotUpdate, SourceKind};
    let market = dir.join("a2_market");
    fs::create_dir_all(&market).unwrap();
    let program = darkbot_project0::protocol::PROGRAM_ID_MAINNET
        .parse()
        .unwrap();
    let mut sink = JournalSink::create(&market).unwrap();
    let mut core = ObserverCore::new(program, MarketSource::Logs, EnrichConfig::DEFAULT);
    let start = CaptureStart {
        source: "logs".into(),
        commitment: commitment.into(),
        program,
        configured_duration_seconds: 4,
        ws_host: "ws://fixture".into(),
        recovery_budget_ms: None,
        recovery_attempts: 1,
        idle_timeout_ms: 1,
        healthy_reset_ms: 1,
        readiness: "ALL_ACKS_AND_NEW_SLOT_PROGRESS".into(),
        provider_budget: darkbot_solana::provider::shared().snapshot(),
    };
    core.coverage(&mut sink, 0, None, WindowEvent::Start(Box::new(start)))
        .unwrap();
    // The window of the test worker (`h.start(1, 4_000_000_001)`).
    core.start_observation(1, Some(4_000_000_001));
    for (slot, t_rx_ns) in A2_RECEIPTS {
        let ev = IngressEvent::Slot(SlotUpdate {
            slot,
            parent: Some(slot - 1),
            status: SlotStatus::Processed,
            source: SourceKind::WsRpcControl,
            t_rx_ns,
        });
        core.on_ingress_event(ev, 0, &mut sink).unwrap();
    }
    sink.finish(true).unwrap();
    market.join(crate::market_observer::MARKET_JOURNAL)
}

/// A2: under `finalized` no margin against the winner's live receipt (its finalized
/// notification); the canonical winner-slot margin uses the first in-window local receipt and is
/// identical live (Worker receipts) and offline (the native report's journal inputs and join).
#[test]
fn a2_slot_margin_is_identical_live_and_offline_and_rx_only_when_confirmed() {
    for commitment in ["finalized", "confirmed"] {
        let mut w = worker();
        w.commitment = commitment.into();
        let r = identified();
        w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
        let event = row(&r, 10, 0).market;
        w.canonical_market.push(event.clone());
        for (slot, t_rx_ns) in A2_RECEIPTS {
            w.accept(Evidence::Slot { slot, t_rx_ns }).unwrap();
        }
        w.finalize_canonical().unwrap();
        let live = joined_facts(&w.canonical_joined[0]);

        let journal_path = a2_market_journal(&w.h.cfg.run_dir, commitment);
        let mut inputs = crate::analytics::report::RaceInputs::default();
        let mut reader = crate::storage::segments::ChainReader::open(&journal_path).unwrap();
        while let Some(frame) = reader.next_frame().unwrap() {
            if frame.kind == journal::kind::MARKET_COVERAGE {
                inputs.note(&frame).unwrap();
            }
        }
        assert_eq!(inputs.market_commitment.as_deref(), Some(commitment));
        assert_eq!(inputs.slot_first_seen, BTreeMap::from([(10, 60), (11, 70)]));
        assert_eq!(w.slots, inputs.slot_first_seen, "live keeps the journaled receipts");
        let offline = join(
            vec![event],
            &w.h.cfg.run_dir.join("shadow_evaluations.jsonl"),
            std::time::Instant::now() + Duration::from_secs(10),
            &JoinContext {
                market_commitment: commitment,
                slot_first_seen: Some(&inputs.slot_first_seen),
                capture: None,
                capital: &w.h.cfg.capital,
            },
        )
        .unwrap();
        let offline = joined_facts(&offline.rows[0]);
        assert_eq!(live, offline, "{commitment}");
        assert_eq!(live.identity_status, "TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT");
        assert_eq!(live.local_slot_race_margin_ns, Some(20), "first receipt 60 - sign 40");
        let rx = (commitment == "confirmed").then_some(10);
        assert_eq!(live.local_observed_race_margin_ns, rx, "{commitment}");

        let canonical = w.canonical_metrics(0, SNAPSHOT_COUNT);
        assert_eq!(canonical.local_margin_ms.count_known, usize::from(rx.is_some()));
        let m = w.race_margin_metrics(0, SNAPSHOT_COUNT);
        assert_eq!(m.market_commitment, commitment);
        assert_eq!(m.canonical_slot_margin_ms.count_known, 1);
        assert_eq!(m.canonical_slot_margin_ms.median, Some(20.0 / 1e6));
        assert_eq!(m.winner_slot_margin_status, "COMPUTED");
        assert_eq!(
            m.winner_rx_margin_status.starts_with("COMPUTED"),
            rx.is_some(),
            "{commitment}"
        );
        // The terminal names the slot margin as the race figure and never prints a finalized
        // rx figure.
        let report = w.report(SNAPSHOT_COUNT, true);
        let text = w.terminal_summary(&report);
        assert!(
            text.contains("race_margin_to_winner_slot_ms (canonical, first local slot receipt): count=1"),
            "{text}"
        );
        assert_eq!(
            text.contains("winner_rx_required_speedup_ms: NOT_COMPUTED"),
            rx.is_none(),
            "{text}"
        );
    }
}

/// A2 without slot receipts (explicit event input, no journal): the slot margin is
/// NOT_AVAILABLE with its reason, the rx margin NOT_COMPUTED for an unknown commitment.
#[test]
fn a2_without_receipts_or_commitment_nothing_is_claimed() {
    let r = identified();
    let mut joined = row(&r, 10, 0);
    joined.market_commitment = "UNKNOWN".into();
    joined.our = Some(r);
    let facts = joined_facts(&joined);
    assert_eq!(
        (facts.local_observed_race_margin_ns, facts.local_slot_race_margin_ns),
        (None, None)
    );
    let m = race_margins(&[&joined], "UNKNOWN", false);
    assert!(m.winner_slot_margin_status.starts_with("NOT_AVAILABLE"));
    assert!(m.winner_rx_margin_status.starts_with("NOT_COMPUTED"));
    assert_eq!(m.canonical_slot_margin_ms.count_known, 0);
}

/// A2 schema bump: frames of the previous schema (2) of every changed kind stay readable; the
/// earlier layout is the current one without the appended fields.
#[test]
fn a2_earlier_schema_frames_stay_readable() {
    use crate::analytics::payload::{decode, encode, encode_wire, Payload, TYPED};
    let r = identified();
    // P8-CTRL.4: an earlier row's record has no signing (the record's last byte, `None`).
    let rec_bytes = encode_wire(&r).unwrap();
    let before_signing = |bytes: Vec<u8>| -> Vec<u8> {
        let n = rec_bytes.len();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i..].starts_with(&rec_bytes) {
                out.extend_from_slice(&rec_bytes[..n - 1]);
                i += n;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        out
    };
    let earlier = |current: &dyn Fn() -> Vec<u8>, tail: Vec<u8>| -> Vec<u8> {
        let full = current();
        assert!(full.ends_with(&tail));
        before_signing(full[..full.len() - tail.len()].to_vec())
    };
    let opp = OpportunityRow {
        rec: r.clone(),
        winner: None,
        market: None,
        slot_seen: Some(3),
        capital: vec![200],
        market_commitment: "confirmed".into(),
    };
    let back: OpportunityRow = decode(
        OpportunityRow::KIND,
        TYPED,
        &earlier(&|| encode(&opp).unwrap(), encode_wire(&opp.market_commitment).unwrap()),
    )
    .unwrap();
    assert_eq!(
        (back.slot_seen, back.capital, back.market_commitment.as_str()),
        (Some(3), vec![200], "UNKNOWN")
    );

    let mut joined = row(&r, 10, 0);
    joined.our = Some(r);
    joined.identity_status = "TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT";
    joined.winner_slot_first_seen_ns = Some(60);
    joined.capital = vec![300, 700];
    // Schema 2 (before A2) and 3 (A2, before A6) of the canonical row.
    let back: Joined = decode(
        Joined::KIND,
        TYPED,
        &earlier(
            &|| encode(&joined).unwrap(),
            encode_wire(&(
                &joined.market_commitment,
                joined.winner_slot_first_seen_ns,
                &joined.capital,
            ))
            .unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(back.identity_status, joined.identity_status);
    assert_eq!(
        (back.market_commitment.as_str(), back.winner_slot_first_seen_ns),
        ("UNKNOWN", None)
    );
    assert_eq!(back.capital, CAPITAL_USD.to_vec(), "the list its view used");
    assert_eq!(joined_facts(&back).local_observed_race_margin_ns, None);
    let back: Joined = decode(
        Joined::KIND,
        3,
        &earlier(&|| encode(&joined).unwrap(), encode_wire(&joined.capital).unwrap()),
    )
    .unwrap();
    assert_eq!(
        (back.market_commitment.as_str(), back.winner_slot_first_seen_ns),
        ("confirmed", Some(60))
    );
    assert_eq!(back.capital, CAPITAL_USD.to_vec());

    // Snapshot/final reports of schema 2 and 3 are read from real historical frames:
    // `n1_n5_historical_econ_frames_stay_readable`. Any other version stays refused.
    let frame = SnapshotFrame {
        index: 1,
        finalizing: false,
        report: worker().report(1, false),
    };
    assert!(decode::<SnapshotFrame>(SnapshotFrame::KIND, 9, &encode(&frame).unwrap()).is_err());
}

#[cfg(feature = "historical")]
/// N1/N5: ECON journals written by the schema-2 (17.10.2026) and schema-3 (18.10.2026) writers
/// (test runs of that time; sealed, chain-verified fixtures) stay readable through their explicit
/// layouts. Every snapshot and final frame decodes; the recorded figures are unchanged — the
/// schema-2 final equals the JSON its own writer rendered then, the schema-3 final equals the
/// render of the schema-3 reader — and the fields added by N1/N5 say "not recorded" instead of
/// presenting new values as historical measurements.
#[test]
fn n1_n5_historical_econ_frames_stay_readable() {
    use crate::analytics::payload::{decode, Payload};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/analytics/fixtures");
    let not_recorded = |r: &SnapshotReport| {
        for (c, m) in [
            (&r.canonical.window_delta, &r.race_margins.window_delta),
            (&r.canonical.cumulative, &r.race_margins.cumulative),
        ] {
            assert_eq!(c.loss_attribution_model, LOSS_ATTRIBUTION_PRE_N1);
            assert!(c.local_margin_ms_technical.is_none());
            assert!(c.required_local_speedup_ms_technical.is_none());
            assert_eq!(m.technical_status, TECHNICAL_NOT_RECORDED);
            assert!(m.canonical_slot_margin_ms_technical.is_none());
            assert!(m.required_canonical_slot_speedup_ms_technical.is_none());
        }
    };
    for (schema, journal_file, golden, golden_has_race_margins) in [
        (2u16, "econ_reports_schema2.p0j", "econ_final_schema2_writer.json", false),
        (3, "econ_reports_schema3.p0j", "econ_final_schema3_reader.json", true),
    ] {
        let path = dir.join(journal_file);
        let chain = crate::storage::segments::verify_chain(&path).unwrap();
        assert!(chain.sealed && chain.complete, "{journal_file}");
        let mut reader = journal::Reader::open(&path).unwrap();
        let (mut snapshots, mut last_final) = (0, None);
        while let Some(f) = reader.next_frame().unwrap() {
            if f.kind == SnapshotFrame::KIND {
                assert_eq!(f.schema, schema);
                let frame: SnapshotFrame = decode(f.kind, f.schema, &f.payload).unwrap();
                not_recorded(&frame.report);
                snapshots += 1;
            } else if f.kind == SnapshotReport::KIND {
                assert_eq!(f.schema, schema);
                let report: SnapshotReport = decode(f.kind, f.schema, &f.payload).unwrap();
                not_recorded(&report);
                assert!(last_final.replace(report).is_none(), "one final");
            }
        }
        assert_eq!(snapshots, 8, "{journal_file}");
        let mut rendered = render(&last_final.unwrap()).unwrap();
        for section in ["window_delta", "cumulative"] {
            let c = rendered["canonical"][section].as_object_mut().unwrap();
            for k in [
                "loss_attribution_model",
                "local_margin_ms_technical",
                "required_local_speedup_ms_technical",
            ] {
                assert!(c.remove(k).is_some(), "{k}");
            }
            let m = rendered["race_margins"][section].as_object_mut().unwrap();
            for k in [
                "technical_status",
                "canonical_slot_margin_ms_technical",
                "required_canonical_slot_speedup_ms_technical",
            ] {
                assert!(m.remove(k).is_some(), "{k}");
            }
        }
        if !golden_has_race_margins {
            // Schema 2 predates A2: its writer had no race margins.
            rendered.as_object_mut().unwrap().remove("race_margins");
        }
        let expected: Value =
            serde_json::from_slice(&std::fs::read(dir.join(golden)).unwrap()).unwrap();
        assert_eq!(rendered, expected, "{golden}");
    }
}

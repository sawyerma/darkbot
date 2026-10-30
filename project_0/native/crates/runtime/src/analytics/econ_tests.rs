use super::*;
use darkbot_solana::submit::{
    MessageFeeQuote, SimResult, SubmissionAdapter, SubmissionPath, SubmitAck, SubmitError,
};
#[cfg(feature = "historical")]
use serde_json::json;
use serde_json::Value;
use solana_sdk::transaction::VersionedTransaction;
#[cfg(feature = "historical")]
use std::fs;

use crate::analytics::record::{LossReason, TimingEvidence};
use darkbot_project0::economics::{self, CandidateInput, Evaluation, FailedCostMatrix};
use darkbot_project0::funding::FundingClass;
use solana_sdk::pubkey::Pubkey;

fn input(capital: u64) -> CandidateInput {
    CandidateInput {
        gross_seized_realizable_usd_micro: Some(300_000_000),
        repay_usd_micro: 100_000_000,
        protocol_fees_usd_micro: 0,
        flat_sol_fee_usd_micro: 100,
        swap_fees_and_impact_usd_micro: 0,
        transaction_fee_usd_micro: Some(200),
        jito_tip_usd_micro: 0,
        rent_and_token_usd_micro: Some(0),
        required_capital_usd_micro: capital,
        path: economics::Path::Rpc,
        funding: FundingClass::ZeroPrefundSameTx,
    }
}
pub(super) fn costs(capital: u64) -> CandidateCostRecord {
    let input = input(capital);
    let failed = economics::rpc_message_failure_costs(input.transaction_fee_usd_micro);
    CandidateCostRecord::from_input(
        &input,
        economics::evaluate(&input, &failed),
        failed.conservative(input.path),
        None,
    )
}
pub(crate) fn rec() -> OpportunityRecord {
    let c = costs(100_000_000);
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
        selected_costs: Some(c.clone()),
        simulation_result: Some("PASS".into()),
        account: Pubkey::new_unique().to_string(),
        trigger: "test".into(),
        decision: Decision::ShadowReady,
        net_profit: c.net_profit_usd_micro,
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
        candidate_costs: vec![c],
        exposure: None,
        all_signatures: vec![],
        bundle_id: None,
        submitted_tip: None,
        intervals_ns: vec![],
        reason: None,
        loss: None,
        signing: None,
    }
}
fn pair(rec: &OpportunityRecord, slot: u64, rx: u64) -> (Winner, MarketEvent) {
    let receiver = Pubkey::new_unique();
    (
        Winner {
            slot,
            identity: WinnerIdentity::Unique {
                signature: "winner".into(),
                receiver: receiver.to_string(),
                tx_version: "0".into(),
            },
        },
        MarketEvent {
            signature: "winner".into(),
            slot,
            t_rx_ns: Some(rx),
            source: MarketEvidenceSource::Live,
            transaction_index: Some(5),
            event: ReceiverEvent {
                event_index: 0,
                target: rec.account.parse().unwrap(),
                receiver,
                seized_equity_usd: 105.0,
                repaid_equity_usd: 100.0,
                protocol_flat_fee_lamports: 5000,
            },
        },
    )
}
fn config() -> EconConfig {
    EconConfig::from_lookup(&|_| None, 0).unwrap()
}
pub(super) fn worker() -> Worker {
    worker_in(crate::run_layout::RunLayout::V1)
}
fn worker_in(layout: crate::run_layout::RunLayout) -> Worker {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let mut cfg = config();
    cfg.layout = layout;
    cfg.plan = MeasurementPlan::new(4).unwrap();
    cfg.run_dir = std::env::temp_dir().join(format!(
        "c0_e1_{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    // A reused process id must not find an earlier run's journals in the same directory.
    let _ = std::fs::remove_dir_all(&cfg.run_dir);
    cfg.paths().create().unwrap();
    let (tx, rx) = bounded("test-econ", 64);
    let h = Handle {
        tx,
        shared: Arc::new(Shared::default()),
        clock: Clock::new(),
        cfg,
    };
    h.start(1, 4_000_000_001);
    Worker::new(
        h,
        rx,
        "logs".into(),
        "finalized".into(),
        "UNKNOWN".into(),
        30,
    )
    .unwrap()
}
#[cfg(feature = "historical")]
/// D6 (market tests): a handle with `worker()`'s plan and window — the same live hours.
pub(crate) fn window_handle() -> (Handle, darkbot_common::channel::Consumer<Evidence>) {
    let (tx, rx) = bounded("test-econ-window", 1024);
    let mut cfg = config();
    cfg.plan = MeasurementPlan::new(4).unwrap();
    let h = Handle {
        tx,
        shared: Arc::new(Shared::default()),
        clock: Clock::new(),
        cfg,
    };
    h.start(1, 4_000_000_001);
    (h, rx)
}
#[cfg(feature = "historical")]
/// D6 (market tests): the four hourly capture counters a real worker reports for `events`.
pub(crate) fn hourly_capture_counters(events: Vec<Evidence>) -> Vec<Value> {
    let mut w = worker();
    for e in events {
        w.accept(e).unwrap();
    }
    (0..4)
        .map(|i| view(&w.metrics(i, i + 1))["market"]["capture_counters"].clone())
        .collect()
}
/// Render of a typed report document (assertions read the historical keys).
pub(super) fn view<T: Serialize>(doc: &T) -> Value {
    serde_json::to_value(doc).unwrap()
}
/// The summary of an empty finished capture (a real market observer summary).
pub(crate) fn market_summary(secs: f64) -> crate::market_observer::MarketSummary {
    use crate::market_observer::{EnrichConfig, MarketSource, ObserverCore, SummaryContext};
    let program = darkbot_project0::protocol::PROGRAM_ID_MAINNET
        .parse()
        .unwrap();
    let mut core = ObserverCore::new(program, MarketSource::Logs, EnrichConfig::DEFAULT);
    core.start_observation(0, None);
    core.summary(&SummaryContext {
        run_id: "test".into(),
        commitment: "finalized".into(),
        stream_ready: true,
        full_duration_observed: true,
        drain_complete: true,
        actual_observation_seconds: secs,
        configured_duration_seconds: secs,
        min_sample_liquidations: 1,
        ..Default::default()
    })
}
pub(super) fn shadow_stats() -> ShadowStats {
    ShadowStats {
        winner_watch_current: 0,
        winner_watch_max: 0,
        winner_watch_overflow: 0,
        winner_watch_errors: 0,
        winner_watch_resolved: 0,
        winner_watch_expired: 0,
        winner_queue_full_backoffs: 0,
        ingress_overflow: 0,
        winner_pending_at_deadline: 0,
        shadow_state_ready_at_shutdown: true,
        remote_winner_queue_depth: 0,
        remote_projection_queue_depth: 0,
    }
}
/// Typed `ECON_SNAPSHOT` / `ECON_FINAL` frames of a report journal (the journal is the
/// document; JSON views exist only after an explicit offline export).
pub(super) fn report_frames(path: &Path) -> (Vec<SnapshotFrame>, Option<SnapshotReport>) {
    let mut r = journal::Reader::open(path).unwrap();
    let (mut snapshots, mut last) = (Vec::new(), None);
    while let Ok(Some(f)) = r.next_frame() {
        match f.kind {
            kind::ECON_SNAPSHOT => snapshots.push(f.typed::<SnapshotFrame>().unwrap()),
            kind::ECON_FINAL => last = Some(f.typed::<SnapshotReport>().unwrap()),
            _ => {}
        }
    }
    (snapshots, last)
}
#[cfg(feature = "historical")]
/// No JSON/JSONL file anywhere under `dir` (JFI-03: a run writes no views).
pub(super) fn assert_no_json(dir: &Path) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
                assert!(
                    !matches!(ext, "json" | "jsonl"),
                    "run wrote {}",
                    p.display()
                );
            }
        }
    }
}
fn complete(w: &mut Worker) {
    w.h.shared.shadow_ready_ns.store(1, Ordering::Release);
    w.canonical_finalized = true;
    w.snapshots = SNAPSHOT_COUNT;
    let mut summary = market_summary(4.0);
    summary.market_capture_complete = true;
    summary.drain_complete = true;
    summary.decode_complete = true;
    summary.source_coverage_verified = false;
    summary.verdict = "MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED".into();
    w.market_final = Some(crate::market_observer::MarketFinal::checkpoint(summary));
    w.shadow_stats = Some(shadow_stats());
    w.h.shared.shadow_done.store(true, Ordering::Relaxed);
    w.h.shared.market_done.store(true, Ordering::Relaxed);
}
#[test]
fn config_requires_four_snapshots_and_live_disabled() {
    assert_eq!(config().capital, CAPITAL_USD);
    for (key, value) in [
        ("P0_LIVE_ENABLED", "true"),
        ("P0_SUBMISSION_ENABLED", "1"),
        ("P0_ECON_SNAPSHOT_SECS", "0"),
        ("P0_ECON_DURATION_SECS", "0"),
    ] {
        assert!(EconConfig::from_lookup(&|k| (k == key).then(|| value.into()), 0).is_err());
    }
    let c = EconConfig::from_lookup(
        &|k| match k {
            "P0_ECON_DURATION_SECS" => Some("600".into()),
            "P0_ECON_SNAPSHOT_SECS" => Some("150".into()),
            _ => None,
        },
        0,
    )
    .unwrap();
    assert_eq!(c.plan.duration_seconds, 600);
    assert_eq!(c.plan.seconds(1), 150.0);
}
#[test]
fn shared_clock_preserves_origin_across_producers() {
    let w = worker();
    let h = w.h.clone();
    let t = w.h.clock.now_ns();
    assert_eq!(h.clock.unix_ns(t), w.h.clock.unix_ns(t));
}
#[test]
fn economics_input_and_profit_capital_preserved() {
    let i = input(12345);
    let e = Evaluation::Known {
        net: 100,
        profit_per_capital_bps: 81,
    };
    let c = CandidateCostRecord::from_input(&i, e, Some(2), None);
    assert_eq!(c.required_working_capital_usd_micro, Some(12345));
    assert_eq!(c.profit_per_capital_bps, Some(81));
    assert_eq!(
        c.gross_seized_realizable_usd_micro,
        i.gross_seized_realizable_usd_micro
    );
}
#[test]
fn unknown_costs_and_economics_remain_null() {
    for missing in 0..4 {
        let mut i = input(0);
        let mut fail = Some(200);
        match missing {
            0 => i.gross_seized_realizable_usd_micro = None,
            1 => i.transaction_fee_usd_micro = None,
            2 => i.rent_and_token_usd_micro = None,
            _ => fail = None,
        }
        let c = CandidateCostRecord::from_input(&i, Evaluation::Unknown, fail, None);
        assert_eq!(c.net_profit_usd_micro, None);
        assert_eq!(c.profit_per_capital_bps, None);
        if missing > 0 {
            assert_eq!(c.total_budget(), None);
        }
        assert!(serde_json::to_value(c).unwrap()["net_profit_usd_micro"].is_null());
    }
}
#[test]
fn zero_prefund_does_not_erase_fees_and_unknown_budget_is_not_fundable() {
    let mut r = rec();
    r.selected_costs = Some(costs(0));
    assert_eq!(
        r.selected_costs.as_ref().unwrap().fee_tip_reserve(),
        Some(500)
    );
    assert_eq!(r.selected_costs.as_ref().unwrap().total_budget(), Some(500));
    for usd in CAPITAL_USD {
        let mut b = Capital::new(usd);
        b.add(&r, true, true, &RaceClass::PreWinnerSlotProven);
        assert_eq!(b.fundable.count, 1);
        r.selected_costs.as_mut().unwrap().rent_and_token_usd_micro = None;
        let mut b = Capital::new(usd);
        b.add(&r, true, true, &RaceClass::PreWinnerSlotProven);
        assert_eq!(b.fundable.count, 0);
        assert_eq!(b.capital_unknown, 1);
        r.selected_costs = Some(costs(0));
    }
}
#[test]
fn every_default_capital_boundary_includes_reserve() {
    for usd in CAPITAL_USD {
        let mut r = rec();
        r.selected_costs = Some(costs(usd * 1_000_000 - 500));
        let mut b = Capital::new(usd);
        b.add(&r, true, true, &RaceClass::PreWinnerSlotProven);
        assert_eq!(b.fundable.count, 1);
        r.selected_costs = Some(costs(usd * 1_000_000 - 499));
        let mut b = Capital::new(usd);
        b.add(&r, true, true, &RaceClass::PreWinnerSlotProven);
        assert_eq!(b.fundable.count, 0);
    }
}
#[test]
fn unique_winner_slot_classes_and_signed_margins() {
    let r = rec();
    for (slot, class) in [
        (4, RaceClass::PreWinnerSlotProven),
        (3, RaceClass::SameSlotUnknown),
        (2, RaceClass::PostWinnerSlotProvenLate),
    ] {
        let (w, m) = pair(&r, slot, 50);
        let out = race(&r, Some(&w), Some(&m), Some(30), true);
        assert_eq!(out.class, class);
        assert!(out.live_timestamp_qualified);
        assert_eq!(out.margin_to_winner_rx_ns, Some(10));
        assert_eq!(out.margin_to_winner_slot_seen_ns, Some(-10));
        assert_eq!(out.required_speedup_to_winner_rx_ns, Some(0));
        assert_eq!(out.required_speedup_to_winner_slot_seen_ns, Some(10));
    }
}
#[test]
fn identity_mismatch_never_yields_timing() {
    let r = rec();
    let (w, m) = pair(&r, 4, 50);
    for n in 0..4 {
        let mut m = m.clone();
        match n {
            0 => m.signature = "other".into(),
            1 => m.slot += 1,
            2 => m.event.target = Pubkey::new_unique(),
            _ => m.event.receiver = Pubkey::new_unique(),
        }
        let out = race(&r, Some(&w), Some(&m), Some(10), true);
        assert_eq!(out.class, RaceClass::LiveTimestampUnqualified);
        assert!(out.identity_mismatch);
        assert_eq!(out.margin_to_winner_rx_ns, None);
    }
}
#[test]
fn absent_sign_winner_market_and_ambiguous_identity_fail_closed() {
    let mut r = rec();
    let (w, m) = pair(&r, 4, 50);
    assert_eq!(
        race(&r, None, None, None, true).class,
        RaceClass::WinnerUnresolved
    );
    assert_eq!(
        race(&r, Some(&w), None, None, true).class,
        RaceClass::MarketMatchMissing
    );
    let amb = Winner {
        slot: 4,
        identity: WinnerIdentity::SameSlotAmbiguous {
            candidates: vec![crate::market::winner::SameSlotCandidate {
                signature: "winner".into(),
                receiver: m.event.receiver.to_string(),
                tx_version: "0".into(),
                transaction_index: Some(0),
            }],
        },
    };
    assert_eq!(
        race(&r, Some(&amb), Some(&m), None, true).class,
        RaceClass::WinnerIdentityAmbiguous
    );
    r.timing.t_sign_ns = None;
    assert_eq!(
        race(&r, Some(&w), Some(&m), None, true).class,
        RaceClass::NoSign
    );
}
#[test]
fn small_sample_percentiles_and_missing_intervals() {
    let d = distribution(vec![10.0, -10.0, 0.0]);
    assert_eq!(d.p10, Some(-10.0));
    assert_eq!(d.median, Some(0.0));
    assert_eq!(d.p90, Some(10.0));
    assert_eq!(distribution(vec![]).median, None);
    let mut r = rec();
    r.timing.t_ready_ns = None;
    assert_eq!(stage_intervals(&r)["ready_to_sign"], None);
    r.timing.t_sign_ns = Some(1);
    assert_eq!(stage_intervals(&r)["rx_to_sign"], None);
}
#[test]
fn strong_join_unmatched_diagnostics_and_speedup_math() {
    let mut w = worker();
    // The winner rx margin is a race observation only under `confirmed` (A2).
    w.commitment = "confirmed".into();
    let mut r = rec();
    r.timing.t_sign_ns = Some(50_000_040);
    r.sign_slot = Some(4);
    let (win, m) = pair(&r, 3, 40);
    w.accept(Evidence::Market(m)).unwrap();
    w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
    assert_eq!(
        view(&w.metrics(0, 4))["matching"]["market_without_shadow_match"],
        1
    );
    w.accept(Evidence::Winner {
        target: r.account.clone(),
        rx_slot: 1,
        winner: win,
    })
    .unwrap();
    let v = view(&w.metrics(0, 4));
    assert_eq!(v["matching"]["market_with_shadow_match"], 1);
    let s = &v["race_timing"]["observed_speedup_sensitivity"];
    assert_eq!(s[3]["winner_rx_boundary"]["count"], 0);
    assert_eq!(s[4]["winner_rx_boundary"]["count"], 1);
    assert_eq!(v["competition"]["post"]["count"], 1);
    // A repeated evaluation may never multiply one actual winner's edge.
    r.timing.t_rx_ns = Some(1_000_000_100);
    w.accept(Evidence::Shadow(Box::new(r))).unwrap();
    assert_eq!(
        view(&w.metrics(0, 4))["economics"]["known_executable"]["count"],
        1
    );
}
#[test]
fn winner_without_live_observation_is_visible_and_not_race_qualified() {
    let mut w = worker();
    let r = rec();
    let (win, _) = pair(&r, 3, 40);
    w.accept(Evidence::Winner {
        target: r.account.clone(),
        rx_slot: 1,
        winner: win,
    })
    .unwrap();
    w.accept(Evidence::Shadow(Box::new(r))).unwrap();
    let v = view(&w.metrics(0, 4));
    assert_eq!(v["matching"]["winner_live_observation_missing"], 1);
    assert_eq!(v["competition"]["unresolved"]["count"], 1);
}
#[test]
fn simulation_bottleneck_and_unknown_edge_not_fabricated() {
    let mut w = worker();
    let mut r = rec();
    r.decision = Decision::ShadowTechnicalSimFailed;
    r.economics_known = false;
    r.net_profit = None;
    r.loss = Some(LossReason::Build);
    w.accept(Evidence::Shadow(Box::new(r))).unwrap();
    let v = view(&w.metrics(0, 4));
    assert_eq!(
        v["bottlenecks"]["by_reason"]["Simulation"]["opportunity_count"],
        1
    );
    assert_eq!(
        v["bottlenecks"]["by_reason"]["Simulation"]["known_candidate_net_edge_usd_micro"],
        0
    );
    assert!(v["bottlenecks"]["by_reason"]["Simulation"]["matched_market_gross_edge_usd"].is_null());
}
#[test]
fn all_loss_reasons_are_retained_exactly() {
    for loss in [
        LossReason::State,
        LossReason::Health,
        LossReason::CircuitBreaker,
        LossReason::Bankruptcy,
        LossReason::IntegrationRefresh,
        LossReason::Funding,
        LossReason::MissingAta,
        LossReason::UnsupportedIntegration,
        LossReason::Sizing,
        LossReason::Economics,
        LossReason::Routing,
        LossReason::Build,
        LossReason::EconomicSkip,
        LossReason::ThresholdTooHigh,
        LossReason::Unknown,
    ] {
        let mut r = rec();
        r.decision = Decision::Skip;
        r.loss = Some(loss);
        assert_eq!(bottleneck(&r), format!("{loss:?}"));
    }
}
#[cfg(feature = "historical")]
#[test]
fn bounded_queue_and_retention_overflow_fail_qualification() {
    let mut w = worker();
    complete(&mut w);
    w.accept(Evidence::Shadow(Box::new(rec()))).unwrap();
    assert_eq!(view(&w.technical(true))["verdict"], "PASS");
    for _ in 0..65 {
        w.h.emit(Evidence::Slot {
            slot: 1,
            t_rx_ns: 1,
        });
    }
    assert_eq!(w.h.shared.drops.load(Ordering::Relaxed), 1);
    assert_eq!(view(&w.technical(true))["verdict"], "INCOMPLETE");
    let mut w = worker();
    complete(&mut w);
    w.h.cfg.retention = 1;
    for slot in [1, 2] {
        w.accept(Evidence::Slot { slot, t_rx_ns: 1 }).unwrap();
    }
    assert_eq!(w.slots.len(), 1);
    assert_eq!(view(&w.technical(true))["verdict"], "INCOMPLETE");
}
#[cfg(feature = "historical")]
#[test]
fn early_end_missing_snapshot_upgrade_and_writer_failures_are_incomplete() {
    let mut w = worker();
    complete(&mut w);
    w.snapshots = 3;
    assert_eq!(view(&w.technical(true))["verdict"], "INCOMPLETE");
    w.snapshots = SNAPSHOT_COUNT;
    w.market_final
        .as_mut()
        .unwrap()
        .summary
        .actual_observation_seconds = 3.0;
    assert_eq!(view(&w.technical(true))["verdict"], "INCOMPLETE");
    complete(&mut w);
    w.h.shared.upgrade.store(true, Ordering::Relaxed);
    assert_eq!(view(&w.technical(true))["verdict"], "INCOMPLETE");
    w.h.shared.upgrade.store(false, Ordering::Relaxed);
    w.h.shared.writer_errors.store(1, Ordering::Relaxed);
    assert_eq!(view(&w.technical(true))["verdict"], "INCOMPLETE");
}
#[cfg(feature = "historical")]
/// PERSIST-05: a real write failure of the report journal (injected at the I/O boundary) is
/// counted, fails the technical verdict INCOMPLETE and is sticky; the snapshots written before
/// it stay readable and no later one is written.
#[test]
fn report_journal_write_failure_is_counted_sticky_and_incomplete() {
    let mut w = worker();
    complete(&mut w);
    w.final_technical = Some(w.technical(false));
    w.write_snapshot(1, true).unwrap();
    assert_eq!(view(&w.technical(false))["snapshot_write_failures"], 0);
    w.reports.test_read_only();
    assert!(w.write_snapshot(2, true).is_err());
    assert_eq!(view(&w.technical(false))["snapshot_write_failures"], 1);
    w.final_technical = None;
    assert_eq!(view(&w.technical(true))["verdict"], "INCOMPLETE");
    assert!(w.write_snapshot(3, true).is_err(), "sticky");
    assert_eq!(view(&w.technical(false))["snapshot_write_failures"], 2);
    let (frames, last) = report_frames(&w.reports.path);
    assert_eq!(frames.iter().map(|f| f.index).collect::<Vec<_>>(), [1]);
    assert!(last.is_none());
}
#[cfg(feature = "historical")]
#[test]
fn four_boundaries_delta_cumulative_and_final_reconcile() {
    let mut w = worker();
    complete(&mut w);
    for hour in 0..4 {
        let mut r = rec();
        r.timing.t_rx_ns = Some(hour * 1_000_000_000 + 10);
        w.accept(Evidence::Shadow(Box::new(r))).unwrap();
    }
    w.final_technical = Some(w.technical(true));
    for i in 1..=4 {
        w.write_snapshot(i, true).unwrap();
    }
    let (frames, _) = report_frames(&w.reports.path);
    assert_eq!(
        frames
            .iter()
            .map(|f| (f.index, f.finalizing))
            .collect::<Vec<_>>(),
        [(1, true), (2, true), (3, true), (4, true)]
    );
    for f in &frames {
        let v = view(&f.report);
        assert_eq!(v["shadow"]["window_delta"]["evaluations"], 1);
        assert_eq!(v["shadow"]["cumulative"]["evaluations"], f.index);
    }
    let last = view(&frames[3].report);
    assert_eq!(last, view(&w.report(4, true)));
    assert_eq!(last["live_ready"], false);
    assert_eq!(
        last["technical_qualification"]["source_coverage_verified"],
        false
    );
    assert_no_json(&w.h.cfg.run_dir);
}
#[test]
fn counters_use_event_receive_hour_even_if_processed_after_boundary() {
    let mut w = worker();
    let a = crate::market_observer::MarketStats::default();
    let mut b = a.clone();
    b.unique_transactions = 1;
    w.accept(Evidence::MarketCounters {
        at_ns: 100,
        before: Box::new(a),
        after: Box::new(b),
        observed_slots: None,
    })
    .unwrap();
    assert_eq!(
        view(&w.metrics(0, 1))["market"]["capture_counters"]["unique_transactions"],
        1
    );
    assert_eq!(
        view(&w.metrics(1, 2))["market"]["capture_counters"]["unique_transactions"],
        0
    );
}

#[cfg(feature = "historical")]
/// R17: each hour reports the first/last slot of the transactions observed in that hour, not
/// the run-global first slot (hour 2 used to repeat hour 1's first slot).
#[test]
fn r17_hourly_observed_slots_are_per_hour() {
    let mut w = worker();
    let mut stats = crate::market_observer::MarketStats::default();
    for (at_ns, slots) in [
        (100, (110, 120)),
        (200, (105, 130)),
        (1_500_000_000, (300, 310)),
        (1_600_000_000, (305, 320)),
    ] {
        let before = stats.clone();
        stats.unique_transactions += 1;
        stats.first_observed_slot = Some(
            stats
                .first_observed_slot
                .map_or(slots.0, |s| s.min(slots.0)),
        );
        stats.last_observed_slot =
            Some(stats.last_observed_slot.map_or(slots.1, |s| s.max(slots.1)));
        w.accept(Evidence::MarketCounters {
            at_ns,
            before: Box::new(before),
            after: Box::new(stats.clone()),
            observed_slots: Some(slots),
        })
        .unwrap();
    }
    // An event without admitted transactions changes no slot bound.
    w.accept(Evidence::MarketCounters {
        at_ns: 1_700_000_000,
        before: Box::new(stats.clone()),
        after: Box::new(stats.clone()),
        observed_slots: None,
    })
    .unwrap();
    let hour = |from, to| view(&w.metrics(from, to))["market"]["capture_counters"].clone();
    assert_eq!(
        (
            hour(0, 1)["first_observed_slot"].clone(),
            hour(0, 1)["last_observed_slot"].clone()
        ),
        (json!(105), json!(130))
    );
    assert_eq!(
        (
            hour(1, 2)["first_observed_slot"].clone(),
            hour(1, 2)["last_observed_slot"].clone()
        ),
        (json!(300), json!(320))
    );
    assert_eq!(hour(0, 2)["first_observed_slot"], json!(105));
    assert_eq!(hour(0, 2)["last_observed_slot"], json!(320));
    assert!(hour(2, 3)
        .get("first_observed_slot")
        .is_none_or(|v| v.is_null()));
}

struct CountingSubmitter {
    calls: Arc<AtomicU64>,
    sims: Arc<AtomicU64>,
}
impl SubmissionAdapter for CountingSubmitter {
    fn path(&self) -> SubmissionPath {
        SubmissionPath::Rpc
    }
    fn fee_for_message(
        &self,
        tx: &VersionedTransaction,
        _: std::time::Instant,
    ) -> Result<MessageFeeQuote, darkbot_solana::submit::FeeQuoteError> {
        Ok(MessageFeeQuote {
            message_hash: tx.message.hash(),
            context_slot: 1,
            lamports: 200,
        })
    }
    fn simulate(&self, tx: &VersionedTransaction, _: std::time::Instant) -> SimResult {
        assert!(
            tx.verify_and_hash_message().is_ok(),
            "simulate exactly signed bytes"
        );
        self.sims.fetch_add(1, Ordering::Relaxed);
        SimResult::Ok {
            units_consumed: Some(100),
            context_slot: None,
        }
    }
    /// P8-CTRL.4: a Shadow simulation without a sign permit — signed, but never by the wallet.
    fn simulate_unverified(&self, tx: &VersionedTransaction, _: std::time::Instant) -> SimResult {
        assert!(
            tx.signatures.iter().all(|s| *s != Default::default())
                && tx.verify_and_hash_message().is_err(),
            "a synthetic signature, never the wallet's"
        );
        self.sims.fetch_add(1, Ordering::Relaxed);
        SimResult::Ok {
            units_consumed: Some(100),
            context_slot: None,
        }
    }
    fn simulate_bundle(&self, _: &[VersionedTransaction], _: std::time::Instant) -> SimResult {
        SimResult::Unavailable("test adapter: no bundle simulation".into())
    }
    fn submit(
        &self,
        _: &VersionedTransaction,
        _: u64,
        _: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        panic!("must never broadcast")
    }
    fn submit_bundle(
        &self,
        _: &[VersionedTransaction],
        _: u64,
        _: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        panic!("must never broadcast bundle")
    }
}
struct TestHealth;
impl darkbot_project0::health::HealthEngine for TestHealth {
    fn health(
        &self,
        _: &Pubkey,
        _: i64,
        _: u64,
    ) -> Result<darkbot_project0::health::HealthView, darkbot_project0::health::HealthError> {
        Ok(darkbot_project0::health::HealthView {
            maintenance_health: -1,
            maint_assets: 1,
            maint_liabs: 2,
            equity_assets: 1,
            equity_liabilities: 2,
            is_bad_debt: false,
            closeout_allowed: false,
            liquidatable: true,
        })
    }
    fn observation_accounts(
        &self,
        _: &Pubkey,
    ) -> Result<Vec<Pubkey>, darkbot_project0::health::HealthError> {
        Ok(vec![])
    }
    fn thresholds(
        &self,
        _: &Pubkey,
        _: i64,
        _: u64,
    ) -> Result<darkbot_project0::health::AccountThresholds, darkbot_project0::health::HealthError>
    {
        Ok(darkbot_project0::health::AccountThresholds { thresholds: vec![] })
    }
    fn premium_decay_per_second(
        &self,
        _: &Pubkey,
        _: i64,
        _: u64,
    ) -> Result<u128, darkbot_project0::health::HealthError> {
        Ok(0)
    }
}
/// E2 fixture: an instruction table of a test program and its Start/Repay/End instructions,
/// so the pipeline gate classifies real instruction bytes in these tests too.
fn test_table() -> &'static darkbot_project0::planner::InstructionTable {
    static TABLE: std::sync::OnceLock<darkbot_project0::planner::InstructionTable> =
        std::sync::OnceLock::new();
    TABLE.get_or_init(|| darkbot_project0::planner::InstructionTable {
        program: Pubkey::new_from_array([7; 32]),
        start: [1; 8],
        end: [2; 8],
        withdraw: vec![[3; 8]],
        repay: [5; 8],
        init_record: [6; 8],
        // The swap program of test prerequisites (E2: a prerequisite carries swaps only).
        swap_programs: [Pubkey::new_from_array([8; 32])].into(),
        ..Default::default()
    })
}
fn test_ix(disc: [u8; 8]) -> solana_sdk::instruction::Instruction {
    solana_sdk::instruction::Instruction {
        program_id: test_table().program,
        accounts: vec![],
        data: disc.to_vec(),
    }
}
struct TestBuilder;
impl crate::execution::pipeline::ReceivershipBuilder for TestBuilder {
    fn native_cost_usd_micro(&self, n: u64) -> Option<u64> {
        Some(n)
    }
    fn instruction_table(&self) -> &darkbot_project0::planner::InstructionTable {
        test_table()
    }
    fn plans(
        &self,
        _: &Pubkey,
        _: u64,
        _: u64,
        _: std::time::Instant,
    ) -> Result<
        Vec<crate::execution::pipeline::ReceivershipPlan>,
        crate::execution::pipeline::PlanError,
    > {
        use darkbot_project0::planner::RouteKind;
        let t = test_table();
        Ok(vec![crate::execution::pipeline::ReceivershipPlan {
            market_identity: None,
            instructions: vec![
                test_ix(t.start),
                test_ix(t.withdraw[0]),
                test_ix(t.repay),
                test_ix(t.end),
            ],
            route: RouteKind::NoSwap,
            input: input(0),
            premium_bps: 500,
            tag_growth: None,
            exposure: 0,
            inventory_hold_usd_micro: 0,
            tip: 0,
            slippage_bps: 0,
            assets: vec![],
            tx_version: darkbot_solana::tx::BuildVersion::V0,
            lookup_tables: vec![].into(),
            prerequisite_txs: vec![],
            sol_costs: Default::default(),
            inventory_holds: vec![],
            hold_facts: None,
        }])
    }
}
#[test]
fn actual_shadow_pipeline_builds_signs_simulates_and_never_submits() {
    use crate::execution::pipeline::{Mode, Pipeline};
    use darkbot_project0::{
        execution::Freshness,
        planner::Allowlist,
        safety::{Allow, Limits, Safety},
    };
    use darkbot_solana::{
        state::blockhash::{BlockhashCache, CachedBlockhash},
        tx::{Resources, TxLimits},
    };
    let calls = Arc::new(AtomicU64::new(0));
    let sims = Arc::new(AtomicU64::new(0));
    let adapter = ObserveOnly::new(CountingSubmitter {
        calls: calls.clone(),
        sims: sims.clone(),
    });
    let clock = Clock::new();
    let bh = BlockhashCache::new();
    bh.set(CachedBlockhash {
        hash: solana_sdk::hash::Hash::new_unique(),
        last_valid_block_height: 100,
        refreshed_ns: clock.now_ns(),
        context_slot: None,
    });
    let mut safety = Safety::new(
        Limits {
            max_total_capital: 0,
            max_outstanding_inventory: 0,
            max_concurrent_positions: 0,
            max_exposure_per_attempt: 0,
            max_daily_loss: 0,
            max_failed_attempts_per_day: 0,
            max_tip: 0,
            max_slippage_bps: 0,
            max_outstanding_attempts: 0,
        },
        Allow::default(),
        false,
    );
    let payer = solana_sdk::signature::Keypair::new();
    let allow = Allowlist::default();
    let failed = FailedCostMatrix::default();
    let slots = darkbot_solana::ingress::SlotWatch::new();
    slots.observe(10);
    let mut network = crate::execution::pipeline::NetworkBudget::unbounded();
    let mut p = Pipeline {
        mode: Mode::Shadow,
        clock: &clock,
        health: &TestHealth,
        builder: &TestBuilder,
        submitter: &adapter,
        safety: &mut safety,
        blockhash: &bh,
        payer: &payer,
        allowlist: &allow,
        failed_costs: &failed,
        limits: TxLimits::default(),
        resources: Resources {
            compute_unit_limit: 100_000,
            priority_micro_lamports_per_cu: 0,
            loaded_accounts_data_size: None,
        },
        min_profit: 0,
        blockhash_max_age_ns: u64::MAX,
        p7_passed: false,
        freshness: Freshness {
            max_opportunity_age_ns: u64::MAX,
            execution_deadline_ns: u64::MAX,
        },
        bundle_tip: None,
        global_allowed: std::slice::from_ref(&test_table().program),
        prereq_allowed: &[],
        slot_watch: &slots,
        network: &mut network,
        simulation_failure: None,
        blockhash_park: crate::execution::pipeline::BlockhashPark::Off,
        fe04: None,
        hold: &crate::execution::hold_b::HoldGate::OBSERVE,
        egress: None,
        synthetic: crate::execution::pipeline::test_synthetic(),
    };
    let r = p.evaluate(Pubkey::new_unique(), "test", clock.now_ns(), 100, 10, true);
    assert_eq!(r.decision, Decision::ShadowReady);
    assert_eq!(r.simulation_result.as_deref(), Some("PASS"));
    // P8-CTRL.4: Shadow without a SIM_ONLY grant signs synthetically (restricted).
    assert_eq!(
        r.signing,
        Some(crate::analytics::record::Signing::Synthetic)
    );
    assert_eq!(r.sign_slot, Some(10));
    assert!(r.timing.t_sign_ns.is_some());
    assert_eq!(r.timing.t_handoff_ns, None);
    assert_eq!(
        r.selected_costs.unwrap().required_working_capital_usd_micro,
        Some(0)
    );
    assert_eq!(sims.load(Ordering::Relaxed), 1);
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert_eq!(adapter.attempts.load(Ordering::Relaxed), 0);
    let tx = VersionedTransaction {
        signatures: vec![],
        message: solana_sdk::message::VersionedMessage::Legacy(
            solana_sdk::message::Message::default(),
        ),
    };
    assert!(adapter
        .submit(
            &tx,
            0,
            std::time::Instant::now() + std::time::Duration::from_secs(60)
        )
        .is_err());
    assert!(adapter
        .submit_bundle(
            &[tx],
            0,
            std::time::Instant::now() + std::time::Duration::from_secs(60)
        )
        .is_err());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert_eq!(adapter.attempts.load(Ordering::Relaxed), 2);
}
/// MT2: a detection's exact Health is computed once and carried as a value — the pipeline
/// evaluates `evaluate_prepared` on it without calling the engine, with the same record as
/// `evaluate` (which computes its own, once), and records a carried Health error exactly as
/// its own.
struct CountingHealth {
    calls: AtomicU64,
}
impl darkbot_project0::health::HealthEngine for CountingHealth {
    fn health(
        &self,
        a: &Pubkey,
        now: i64,
        slot: u64,
    ) -> Result<darkbot_project0::health::HealthView, darkbot_project0::health::HealthError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        TestHealth.health(a, now, slot)
    }
    fn observation_accounts(
        &self,
        a: &Pubkey,
    ) -> Result<Vec<Pubkey>, darkbot_project0::health::HealthError> {
        TestHealth.observation_accounts(a)
    }
    fn thresholds(
        &self,
        a: &Pubkey,
        now: i64,
        slot: u64,
    ) -> Result<darkbot_project0::health::AccountThresholds, darkbot_project0::health::HealthError>
    {
        TestHealth.thresholds(a, now, slot)
    }
    fn premium_decay_per_second(
        &self,
        a: &Pubkey,
        now: i64,
        slot: u64,
    ) -> Result<u128, darkbot_project0::health::HealthError> {
        TestHealth.premium_decay_per_second(a, now, slot)
    }
}
#[test]
fn mt2_pipeline_evaluates_on_the_carried_health_and_computes_none() {
    use crate::execution::pipeline::{Mode, Pipeline};
    use darkbot_project0::health::{HealthEngine, HealthError, HealthView};
    use darkbot_project0::{
        execution::Freshness,
        planner::Allowlist,
        safety::{Allow, Limits, Safety},
    };
    use darkbot_solana::{
        state::blockhash::{BlockhashCache, CachedBlockhash},
        tx::{Resources, TxLimits},
    };
    let adapter = ObserveOnly::new(CountingSubmitter {
        calls: Arc::new(AtomicU64::new(0)),
        sims: Arc::new(AtomicU64::new(0)),
    });
    let clock = Clock::new();
    let bh = BlockhashCache::new();
    bh.set(CachedBlockhash {
        hash: solana_sdk::hash::Hash::new_unique(),
        last_valid_block_height: 100,
        refreshed_ns: clock.now_ns(),
        context_slot: None,
    });
    let mut safety = Safety::new(
        Limits {
            max_total_capital: 0,
            max_outstanding_inventory: 0,
            max_concurrent_positions: 0,
            max_exposure_per_attempt: 0,
            max_daily_loss: 0,
            max_failed_attempts_per_day: 0,
            max_tip: 0,
            max_slippage_bps: 0,
            max_outstanding_attempts: 0,
        },
        Allow::default(),
        false,
    );
    let payer = solana_sdk::signature::Keypair::new();
    let allow = Allowlist::default();
    let failed = FailedCostMatrix::default();
    let slots = darkbot_solana::ingress::SlotWatch::new();
    slots.observe(10);
    let mut network = crate::execution::pipeline::NetworkBudget::unbounded();
    let health = CountingHealth {
        calls: AtomicU64::new(0),
    };
    let mut p = Pipeline {
        mode: Mode::Shadow,
        clock: &clock,
        health: &health,
        builder: &TestBuilder,
        submitter: &adapter,
        safety: &mut safety,
        blockhash: &bh,
        payer: &payer,
        allowlist: &allow,
        failed_costs: &failed,
        limits: TxLimits::default(),
        resources: Resources {
            compute_unit_limit: 100_000,
            priority_micro_lamports_per_cu: 0,
            loaded_accounts_data_size: None,
        },
        min_profit: 0,
        blockhash_max_age_ns: u64::MAX,
        p7_passed: false,
        freshness: Freshness {
            max_opportunity_age_ns: u64::MAX,
            execution_deadline_ns: u64::MAX,
        },
        bundle_tip: None,
        global_allowed: std::slice::from_ref(&test_table().program),
        prereq_allowed: &[],
        slot_watch: &slots,
        network: &mut network,
        simulation_failure: None,
        blockhash_park: crate::execution::pipeline::BlockhashPark::Off,
        fe04: None,
        hold: &crate::execution::hold_b::HoldGate::OBSERVE,
        egress: None,
        synthetic: crate::execution::pipeline::test_synthetic(),
    };
    let account = Pubkey::new_unique();
    let key = |r: &OpportunityRecord| {
        (
            r.decision,
            r.loss,
            r.reason.clone(),
            r.progress.health_liquidatable,
            r.progress.candidates_created,
            r.simulation_result.clone(),
            r.net_profit,
            r.premium_bps,
            r.sign_slot,
            r.economics_known,
        )
    };
    // `evaluate`: the pipeline computes its own Health, once.
    let own = p.evaluate(account, "test", clock.now_ns(), 100, 10, true);
    assert_eq!(own.decision, Decision::ShadowReady);
    assert_eq!(health.calls.load(Ordering::Relaxed), 1);
    // `evaluate_prepared`: the carried value, no engine call; the same record.
    let carried = health.health(&account, 100, 10);
    assert_eq!(health.calls.load(Ordering::Relaxed), 2);
    let prepared = p.evaluate_prepared(
        account,
        "test",
        clock.now_ns(),
        100,
        10,
        true,
        carried.as_ref(),
    );
    assert_eq!(
        health.calls.load(Ordering::Relaxed),
        2,
        "no Health computed by the pipeline"
    );
    assert_eq!(key(&prepared), key(&own));
    // A carried Health error is recorded as the pipeline's own; a non-liquidatable view ends
    // at the Health stage. Neither computes Health.
    let unavailable: Result<HealthView, HealthError> = Err(HealthError::Unavailable);
    let r = p.evaluate_prepared(
        account,
        "test",
        clock.now_ns(),
        100,
        10,
        true,
        unavailable.as_ref(),
    );
    assert_eq!(
        (r.decision, r.loss, r.reason_text().as_deref()),
        (
            Decision::Unavailable,
            Some(LossReason::Health),
            Some("health engine unavailable")
        )
    );
    let stale: Result<HealthView, HealthError> = Err(HealthError::IntegrationStale("k".into()));
    let r = p.evaluate_prepared(
        account,
        "test",
        clock.now_ns(),
        100,
        10,
        true,
        stale.as_ref(),
    );
    assert_eq!(r.loss, Some(LossReason::IntegrationRefresh));
    let healthy = HealthView {
        liquidatable: false,
        ..carried.unwrap()
    };
    let r = p.evaluate_prepared(account, "test", clock.now_ns(), 100, 10, true, Ok(&healthy));
    assert_eq!(
        (r.progress.health_liquidatable, r.reason_text().as_deref()),
        (Some(false), Some("not liquidatable"))
    );
    // An untradable state is refused before Health on both paths.
    let r = p.evaluate(account, "test", clock.now_ns(), 100, 10, false);
    assert_eq!(
        (r.loss, r.reason_text().as_deref()),
        (Some(LossReason::State), Some("state not tradable"))
    );
    assert_eq!(health.calls.load(Ordering::Relaxed), 2);
}
/// P2-05: a known plan below `min_profit` whose premium is the growing tag cap records the
/// second the curve could make it actionable (`tag_wakeup` of its own numbers); without tag
/// growth, or once a plan is actionable, there is none.
#[test]
fn p2_05_pipeline_records_the_tag_wakeup_of_a_non_actionable_plan() {
    use crate::execution::pipeline::{Mode, Pipeline, ReceivershipBuilder};
    use darkbot_project0::economics::TagGrowth;
    use darkbot_project0::{
        execution::Freshness,
        planner::Allowlist,
        safety::{Allow, Limits, Safety},
    };
    use darkbot_solana::{
        state::blockhash::{BlockhashCache, CachedBlockhash},
        tx::{Resources, TxLimits},
    };
    /// Tag growth of every plan; optionally a second plan realizing only the given USD micro.
    struct Tagged(Option<TagGrowth>, Option<u64>);
    impl ReceivershipBuilder for Tagged {
        fn native_cost_usd_micro(&self, n: u64) -> Option<u64> {
            TestBuilder.native_cost_usd_micro(n)
        }
        fn instruction_table(&self) -> &darkbot_project0::planner::InstructionTable {
            test_table()
        }
        fn plans(
            &self,
            a: &Pubkey,
            now: u64,
            slot: u64,
            t: std::time::Instant,
        ) -> Result<
            Vec<crate::execution::pipeline::ReceivershipPlan>,
            crate::execution::pipeline::PlanError,
        > {
            let mut plans = TestBuilder.plans(a, now, slot, t)?;
            if let Some(gross) = self.1 {
                let mut low = TestBuilder.plans(a, now, slot, t)?;
                low[0].input.gross_seized_realizable_usd_micro = Some(gross);
                plans.extend(low);
            }
            for p in &mut plans {
                p.tag_growth = self.0;
            }
            Ok(plans)
        }
    }
    let run = |builder: &Tagged, min_profit: i128| {
        let adapter = ObserveOnly::new(CountingSubmitter {
            calls: Arc::new(AtomicU64::new(0)),
            sims: Arc::new(AtomicU64::new(0)),
        });
        let clock = Clock::new();
        let bh = BlockhashCache::new();
        bh.set(CachedBlockhash {
            hash: solana_sdk::hash::Hash::new_unique(),
            last_valid_block_height: 100,
            refreshed_ns: clock.now_ns(),
            context_slot: None,
        });
        let mut safety = Safety::new(
            Limits {
                max_total_capital: 0,
                max_outstanding_inventory: 0,
                max_concurrent_positions: 0,
                max_exposure_per_attempt: 0,
                max_daily_loss: 0,
                max_failed_attempts_per_day: 0,
                max_tip: 0,
                max_slippage_bps: 0,
                max_outstanding_attempts: 0,
            },
            Allow::default(),
            false,
        );
        let payer = solana_sdk::signature::Keypair::new();
        let allow = Allowlist::default();
        let failed = FailedCostMatrix::default();
        let slots = darkbot_solana::ingress::SlotWatch::new();
        slots.observe(10);
        let mut network = crate::execution::pipeline::NetworkBudget::unbounded();
        let mut p = Pipeline {
            mode: Mode::Shadow,
            clock: &clock,
            health: &TestHealth,
            builder,
            submitter: &adapter,
            safety: &mut safety,
            blockhash: &bh,
            payer: &payer,
            allowlist: &allow,
            failed_costs: &failed,
            limits: TxLimits::default(),
            resources: Resources {
                compute_unit_limit: 100_000,
                priority_micro_lamports_per_cu: 0,
                loaded_accounts_data_size: None,
            },
            min_profit,
            blockhash_max_age_ns: u64::MAX,
            p7_passed: false,
            freshness: Freshness {
                max_opportunity_age_ns: u64::MAX,
                execution_deadline_ns: u64::MAX,
            },
            bundle_tip: None,
            global_allowed: std::slice::from_ref(&test_table().program),
            prereq_allowed: &[],
            slot_watch: &slots,
            network: &mut network,
            simulation_failure: None,
            blockhash_park: crate::execution::pipeline::BlockhashPark::Off,
            fe04: None,
            hold: &crate::execution::hold_b::HoldGate::OBSERVE,
            egress: None,
            synthetic: crate::execution::pipeline::test_synthetic(),
        };
        p.evaluate(Pubkey::new_unique(), "test", clock.now_ns(), 100, 10, true)
    };
    let growth = TagGrowth {
        tagged_at: 50,
        fee_state_max_bps: 0,
    };
    // 300 USD realizable for a 100 USD repay at 5 %: ≈ 200 USD net, 250 USD required.
    let r = run(&Tagged(Some(growth), None), 250_000_000);
    assert_eq!(r.decision, Decision::Skip);
    let net = r.net_profit.unwrap();
    let wake = economics::tag_wakeup(100, growth, 500, 100_000_000, 300_000_000, net, 250_000_000);
    assert!(wake.is_some_and(|t| t > 100));
    assert_eq!(r.t_profitable_unix_s, wake);
    assert_eq!(
        run(&Tagged(None, None), 250_000_000).t_profitable_unix_s,
        None
    );
    let ready = run(&Tagged(Some(growth), None), 0);
    assert_eq!(ready.decision, Decision::ShadowReady);
    assert_eq!(ready.t_profitable_unix_s, None, "actionable: no wake-up");
    // An actionable plan beside a lower tagged one (≈ 100 USD net against 150 USD): none.
    let both = run(&Tagged(Some(growth), Some(200_000_000)), 150_000_000);
    assert_eq!(both.decision, Decision::ShadowReady);
    assert_eq!(both.t_profitable_unix_s, None, "an actionable plan exists");
}
/// B1: a LIVE pipeline with every limit open and an enabled Safety stops at the send boundary
/// after build/sign/simulation; it never calls `submit`/`submit_bundle` (the submitter panics)
/// and leaves no reservation. Sending exists only in the FE-04 one-shot owner.
#[test]
fn b1_live_pipeline_never_sends_and_reserves_nothing() {
    use crate::execution::pipeline::{Mode, Pipeline, LIVE_SEND_REQUIRES_FE04};
    use darkbot_project0::{
        execution::Freshness,
        planner::Allowlist,
        safety::{Allow, Limits, Safety},
    };
    use darkbot_solana::{
        state::blockhash::{BlockhashCache, CachedBlockhash},
        tx::{Resources, TxLimits},
    };
    let calls = Arc::new(AtomicU64::new(0));
    let sims = Arc::new(AtomicU64::new(0));
    let submitter = CountingSubmitter {
        calls: calls.clone(),
        sims: sims.clone(),
    };
    let clock = Clock::new();
    let bh = BlockhashCache::new();
    bh.set(CachedBlockhash {
        hash: solana_sdk::hash::Hash::new_unique(),
        last_valid_block_height: 100,
        refreshed_ns: clock.now_ns(),
        context_slot: None,
    });
    let mut allow_live = Allow::default();
    allow_live.execution_classes.insert("longtail");
    let mut safety = Safety::new(
        Limits {
            max_total_capital: u64::MAX,
            max_outstanding_inventory: u64::MAX,
            max_concurrent_positions: u32::MAX,
            max_exposure_per_attempt: u64::MAX,
            max_daily_loss: u64::MAX,
            max_failed_attempts_per_day: u32::MAX,
            max_tip: u64::MAX,
            max_slippage_bps: u64::MAX,
            max_outstanding_attempts: u32::MAX,
        },
        allow_live,
        true,
    );
    let payer = solana_sdk::signature::Keypair::new();
    let allow = Allowlist::default();
    let failed = FailedCostMatrix::default();
    let slots = darkbot_solana::ingress::SlotWatch::new();
    slots.observe(10);
    let mut network = crate::execution::pipeline::NetworkBudget::unbounded();
    // P8-CTRL.4/.7: a LIVE evaluation signs under the gates (every switch, the FE-06 stand-in).
    let gates = crate::orchestration::capability_control::OfflineGates::enabled(
        solana_sdk::signature::Signer::pubkey(&payer),
    );
    let mut p = Pipeline {
        mode: Mode::Live,
        clock: &clock,
        health: &TestHealth,
        builder: &TestBuilder,
        submitter: &submitter,
        safety: &mut safety,
        blockhash: &bh,
        payer: &payer,
        allowlist: &allow,
        failed_costs: &failed,
        limits: TxLimits::default(),
        resources: Resources {
            compute_unit_limit: 100_000,
            priority_micro_lamports_per_cu: 0,
            loaded_accounts_data_size: None,
        },
        min_profit: 0,
        blockhash_max_age_ns: u64::MAX,
        p7_passed: false,
        freshness: Freshness {
            max_opportunity_age_ns: u64::MAX,
            execution_deadline_ns: u64::MAX,
        },
        bundle_tip: None,
        global_allowed: std::slice::from_ref(&test_table().program),
        prereq_allowed: &[],
        slot_watch: &slots,
        network: &mut network,
        simulation_failure: None,
        blockhash_park: crate::execution::pipeline::BlockhashPark::Off,
        fe04: None,
        hold: &crate::execution::hold_b::HoldGate::OBSERVE,
        egress: Some(gates.gates()),
        synthetic: crate::execution::pipeline::test_synthetic(),
    };
    let r = p.evaluate(Pubkey::new_unique(), "test", clock.now_ns(), 100, 10, true);
    assert_eq!(r.decision, Decision::Unavailable);
    assert!(r
        .reason_text()
        .as_deref()
        .is_some_and(|x| x.starts_with(LIVE_SEND_REQUIRES_FE04)));
    assert_eq!(r.simulation_result.as_deref(), Some("PASS"));
    // P8-CTRL.4: LIVE signs with the wallet's key under its permit.
    assert_eq!(r.signing, Some(crate::analytics::record::Signing::Wallet));
    assert_eq!(
        (r.signature, r.exposure, r.timing.t_handoff_ns),
        (None, None, None)
    );
    assert_eq!(sims.load(Ordering::Relaxed), 1);
    assert_eq!(calls.load(Ordering::Relaxed), 0, "no submit call");
    assert_eq!(safety.capital_in_use(), 0, "no reservation left");
}
/// FE-04 test adapter: counts simulations and submits; a submit answers the exact ACK of the
/// bytes, or fails like a lost transport.
struct HandoffSubmitter {
    submits: Arc<AtomicU64>,
    sims: Arc<AtomicU64>,
    transport_fails: bool,
    /// V2-15: how long the first simulation takes, and every later one.
    simulate_delay: Duration,
    simulate_delay_again: Duration,
    /// DEC-02: the slot the first simulation reports (`live_evaluate` evaluates at slot 10),
    /// and every later one.
    context_slot: Option<u64>,
    context_slot_again: Option<u64>,
}
impl HandoffSubmitter {
    fn simulated(&self, _deadline: std::time::Instant) -> SimResult {
        let earlier = self.sims.fetch_add(1, Ordering::Relaxed);
        std::thread::sleep(if earlier == 0 {
            self.simulate_delay
        } else {
            self.simulate_delay_again
        });
        SimResult::Ok {
            units_consumed: Some(100),
            context_slot: if earlier == 0 {
                self.context_slot
            } else {
                self.context_slot_again
            },
        }
    }
}

impl SubmissionAdapter for HandoffSubmitter {
    fn path(&self) -> SubmissionPath {
        SubmissionPath::Rpc
    }
    fn fee_for_message(
        &self,
        tx: &VersionedTransaction,
        _: std::time::Instant,
    ) -> Result<MessageFeeQuote, darkbot_solana::submit::FeeQuoteError> {
        Ok(MessageFeeQuote {
            message_hash: tx.message.hash(),
            context_slot: 1,
            lamports: 200,
        })
    }
    /// P8-CTRL.4: a Shadow simulation without a sign permit — signed, but never by the wallet.
    fn simulate_unverified(
        &self,
        tx: &VersionedTransaction,
        deadline: std::time::Instant,
    ) -> SimResult {
        assert!(
            tx.signatures.iter().all(|s| *s != Default::default())
                && tx.verify_and_hash_message().is_err(),
            "a synthetic signature, never the wallet's"
        );
        self.simulated(deadline)
    }
    fn simulate(&self, tx: &VersionedTransaction, deadline: std::time::Instant) -> SimResult {
        assert!(tx.verify_and_hash_message().is_ok());
        self.simulated(deadline)
    }
    fn simulate_bundle(&self, _: &[VersionedTransaction], _: std::time::Instant) -> SimResult {
        SimResult::Unavailable("no bundle".into())
    }
    fn submit(
        &self,
        tx: &VersionedTransaction,
        t: u64,
        deadline: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        // DEC-12: like every adapter, nothing is sent after the handoff deadline.
        if std::time::Instant::now() >= deadline {
            return Err(SubmitError::Deadline { sent: false });
        }
        self.submits.fetch_add(1, Ordering::Relaxed);
        if self.transport_fails {
            return Err(SubmitError::Transport("lost".into()));
        }
        Ok(SubmitAck {
            path: SubmissionPath::Rpc,
            signature: tx.signatures[0],
            signatures: vec![tx.signatures[0]],
            submission_id: None,
            t_handoff_ns: t,
            serialized_len: darkbot_solana::submit::serialize_tx(tx).unwrap().len(),
        })
    }
    fn submit_bundle(
        &self,
        _: &[VersionedTransaction],
        _: u64,
        _: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        panic!("no bundle path")
    }
}
/// `TestBuilder`'s plan with a known record rent (no record created), a flat protocol fee of
/// 5 lamports and an exposure of 1 000.
struct AdmissionBuilder;
impl crate::execution::pipeline::ReceivershipBuilder for AdmissionBuilder {
    fn native_cost_usd_micro(&self, n: u64) -> Option<u64> {
        Some(n)
    }
    fn instruction_table(&self) -> &darkbot_project0::planner::InstructionTable {
        test_table()
    }
    fn plans(
        &self,
        a: &Pubkey,
        n: u64,
        s: u64,
        d: std::time::Instant,
    ) -> Result<
        Vec<crate::execution::pipeline::ReceivershipPlan>,
        crate::execution::pipeline::PlanError,
    > {
        let mut plans = TestBuilder.plans(a, n, s, d)?;
        for p in &mut plans {
            p.sol_costs.record_rent_lamports = Some(ADMISSION_RENT.with(|r| r.get()));
            p.sol_costs.flat_fee_lamports = 5;
            p.exposure = 1_000;
        }
        Ok(plans)
    }
    /// DEC-04: an exact B of the plan — its SOL debit, nothing kept, an inventory value of 7.
    fn hold_b(
        &self,
        _: &crate::execution::pipeline::ReceivershipPlan,
        sol_debit: Option<u64>,
    ) -> crate::execution::hold_b::BHold {
        use crate::execution::hold_b::{AssetFlow, BHold, PlanFlows};
        BHold::Exact(PlanFlows {
            flows: [(
                "SOL".to_string(),
                AssetFlow {
                    reserve: sol_debit.unwrap_or(0),
                    residual_max: Some(0),
                },
            )]
            .into(),
            inventory_usd_micro: 7,
            kind: "PLAIN".into(),
        })
    }
}
thread_local! {
    /// B4: the record rent (lamports) `AdmissionBuilder` plans carry on this test thread.
    static ADMISSION_RENT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// One LIVE evaluation of `target` through the production pipeline with the admission builder.
fn live_evaluate(
    adapter: &HandoffSubmitter,
    safety: &mut darkbot_project0::safety::Safety,
    fe04: Option<&mut crate::execution::one_shot::admission::Fe04Admission>,
    payer: &solana_sdk::signature::Keypair,
    target: Pubkey,
) -> crate::analytics::record::OpportunityRecord {
    live_evaluate_within(adapter, safety, fe04, payer, target, u64::MAX)
}

/// `live_evaluate` with the owner's candidate network deadline (`deadline_ns`).
fn live_evaluate_within(
    adapter: &HandoffSubmitter,
    safety: &mut darkbot_project0::safety::Safety,
    fe04: Option<&mut crate::execution::one_shot::admission::Fe04Admission>,
    payer: &solana_sdk::signature::Keypair,
    target: Pubkey,
    deadline_ns: u64,
) -> crate::analytics::record::OpportunityRecord {
    evaluate_in(
        crate::execution::pipeline::Mode::Live,
        (adapter, safety, fe04),
        payer,
        target,
        deadline_ns,
        &crate::execution::hold_b::HoldGate::OBSERVE,
    )
}

/// The admission candidate's evaluation in `mode` (DEC-13: the controller gate's).
fn evaluate_in(
    mode: crate::execution::pipeline::Mode,
    (adapter, safety, fe04): (
        &HandoffSubmitter,
        &mut darkbot_project0::safety::Safety,
        Option<&mut crate::execution::one_shot::admission::Fe04Admission>,
    ),
    payer: &solana_sdk::signature::Keypair,
    target: Pubkey,
    deadline_ns: u64,
    hold: &crate::execution::hold_b::HoldGate,
) -> crate::analytics::record::OpportunityRecord {
    use crate::execution::pipeline::Pipeline;
    use darkbot_project0::{execution::Freshness, planner::Allowlist};
    use darkbot_solana::{
        state::blockhash::{BlockhashCache, CachedBlockhash},
        tx::{Resources, TxLimits},
    };
    let clock = Clock::new();
    let bh = BlockhashCache::new();
    bh.set(CachedBlockhash {
        hash: solana_sdk::hash::Hash::new_unique(),
        last_valid_block_height: 100,
        refreshed_ns: clock.now_ns(),
        context_slot: None,
    });
    let allow = Allowlist::default();
    let failed = FailedCostMatrix::default();
    let slots = darkbot_solana::ingress::SlotWatch::new();
    slots.observe(10);
    let mut network = crate::execution::pipeline::NetworkBudget::new(usize::MAX, deadline_ns);
    // P8-CTRL.7: a LIVE evaluation's egress permit (every gate, the FE-06 stand-in).
    let gates = crate::orchestration::capability_control::OfflineGates::enabled(
        solana_sdk::signature::Signer::pubkey(payer),
    );
    let mut p = Pipeline {
        mode,
        clock: &clock,
        health: &TestHealth,
        builder: &AdmissionBuilder,
        submitter: adapter,
        safety,
        blockhash: &bh,
        payer,
        allowlist: &allow,
        failed_costs: &failed,
        limits: TxLimits::default(),
        resources: Resources {
            compute_unit_limit: 100_000,
            priority_micro_lamports_per_cu: 0,
            loaded_accounts_data_size: None,
        },
        min_profit: 0,
        blockhash_max_age_ns: u64::MAX,
        p7_passed: false,
        freshness: Freshness {
            max_opportunity_age_ns: u64::MAX,
            execution_deadline_ns: u64::MAX,
        },
        bundle_tip: None,
        global_allowed: std::slice::from_ref(&test_table().program),
        prereq_allowed: &[],
        slot_watch: &slots,
        network: &mut network,
        simulation_failure: None,
        blockhash_park: crate::execution::pipeline::BlockhashPark::Off,
        fe04,
        hold,
        egress: Some(gates.gates()),
        synthetic: crate::execution::pipeline::test_synthetic(),
    };
    p.evaluate(target, "test", clock.now_ns(), 100, 10, true)
}

fn admission(
    dir: &std::path::Path,
    payer: &solana_sdk::signature::Keypair,
    target: Pubkey,
    class: &str,
) -> crate::execution::one_shot::admission::Fe04Admission {
    use solana_sdk::signer::Signer;
    let wallet = payer.pubkey().to_string();
    crate::execution::one_shot::admission::Fe04Admission {
        journal: crate::execution::one_shot::Journal::open(dir, &wallet).unwrap(),
        approval: crate::execution::one_shot::Approval {
            schema: 1,
            authorized: true,
            approval_id: "approval-1".into(),
            code_sha: "a".repeat(40),
            binary_sha256: "b".repeat(64),
            wallet,
            program: test_table().program.to_string(),
            program_data_hash: "pd".into(),
            funding_class: class.into(),
            allowed_targets: [target.to_string()].into_iter().collect(),
            expires_unix_s: u64::MAX,
            max_state_age_ms: 60_000,
            max_capital_usd_micro: u64::MAX,
            max_total_sol_debit_lamports: 1_000_000,
            max_tip_lamports: 0,
            max_rent_lamports: 0,
            max_loss_usd_micro: u64::MAX,
            min_net_usd_micro: 1,
            max_slippage_bps: 50,
        },
        state_generation: 1,
        program_data_hash: "pd".into(),
        balances: [("SOL".to_string(), 1_000_000_000)].into_iter().collect(),
        reserved_elsewhere: Default::default(),
        policy_reserve: Default::default(),
        valid_until_slot: 160,
        simulation: Default::default(),
        reserved: None,
    }
}
/// V2-15 (P9-01b): the handoff is validated against the time and slot at the handoff, not at
/// the evaluation — a state that aged past the approval's `max_state_age_ms` during the
/// simulation is never sent (before: the evaluation's `Current` was reused and it was sent).
#[test]
fn v2_15_a_state_aged_during_the_simulation_is_never_handed_off() {
    use darkbot_project0::safety::{Allow, Limits, Safety};
    let target = Pubkey::new_unique();
    let payer = solana_sdk::signature::Keypair::new();
    let mut allow = Allow::default();
    allow.execution_classes.insert("longtail");
    let mut safety = Safety::new(
        Limits {
            max_total_capital: u64::MAX,
            max_outstanding_inventory: u64::MAX,
            max_concurrent_positions: u32::MAX,
            max_exposure_per_attempt: u64::MAX,
            max_daily_loss: u64::MAX,
            max_failed_attempts_per_day: u32::MAX,
            max_tip: u64::MAX,
            max_slippage_bps: u64::MAX,
            max_outstanding_attempts: u32::MAX,
        },
        allow,
        true,
    );
    let adapter = HandoffSubmitter {
        submits: Arc::new(AtomicU64::new(0)),
        sims: Arc::new(AtomicU64::new(0)),
        transport_fails: false,
        simulate_delay: Duration::from_millis(120),
        simulate_delay_again: Duration::from_millis(120),
        context_slot: Some(10),
        context_slot_again: Some(10),
    };
    let dir = std::env::temp_dir().join(format!("p0-v2-15-{}", Pubkey::new_unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut fe04 = admission(&dir, &payer, target, "ZERO_PREFUND_SAME_TX");
    fe04.approval.max_state_age_ms = 50;
    let r = live_evaluate(&adapter, &mut safety, Some(&mut fe04), &payer, target);
    assert!(adapter.sims.load(Ordering::Relaxed) >= 1);
    assert_eq!(adapter.submits.load(Ordering::Relaxed), 0, "{:?}", r.reason);
    assert_ne!(r.decision, Decision::Submitted);
    assert!(
        r.reason_text().as_deref().unwrap_or("").contains("STALE"),
        "{:?}",
        r.reason
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// DEC-04 through the LIVE pipeline and the FE-04 admission: in OBSERVE B is computed for the
/// signed plan and kept with the reservation (mode, A's inventory hold, B), A's inventory hold
/// is reserved; in an effective ACTIVE of a qualified kind B's is; in OFF nothing is computed.
#[test]
fn dec04_live_admission_keeps_the_holds_and_active_reserves_b() {
    use crate::execution::hold_b::{BEvidence, BHold, HoldGate, HoldMode};
    use darkbot_project0::safety::{Allow, Limits, Safety};
    let target = Pubkey::new_unique();
    let payer = solana_sdk::signature::Keypair::new();
    let safety = || {
        let mut allow = Allow::default();
        allow.execution_classes.insert("longtail");
        Safety::new(
            Limits {
                max_total_capital: u64::MAX,
                max_outstanding_inventory: u64::MAX,
                max_concurrent_positions: u32::MAX,
                max_exposure_per_attempt: u64::MAX,
                max_daily_loss: u64::MAX,
                max_failed_attempts_per_day: u32::MAX,
                max_tip: u64::MAX,
                max_slippage_bps: u64::MAX,
                max_outstanding_attempts: u32::MAX,
            },
            allow,
            true,
        )
    };
    let adapter = || HandoffSubmitter {
        submits: Arc::new(AtomicU64::new(0)),
        sims: Arc::new(AtomicU64::new(0)),
        transport_fails: false,
        simulate_delay: Duration::ZERO,
        simulate_delay_again: Duration::ZERO,
        context_slot: Some(10),
        context_slot_again: Some(10),
    };
    let run = |gate: &HoldGate| {
        let dir = std::env::temp_dir().join(format!("p0-dec04-{}", Pubkey::new_unique()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, mut s) = (adapter(), safety());
        let mut fe04 = admission(&dir, &payer, target, "ZERO_PREFUND_SAME_TX");
        let r = evaluate_in(
            crate::execution::pipeline::Mode::Live,
            (&a, &mut s, Some(&mut fe04)),
            &payer,
            target,
            u64::MAX,
            gate,
        );
        assert_eq!(r.decision, Decision::Submitted, "{:?}", r.reason);
        let hold = fe04.journal.attempt_holds().and_then(|h| h.hold.cloned());
        let _ = std::fs::remove_dir_all(dir);
        (hold, s.inventory_reserved())
    };
    let (hold, reserved) = run(&HoldGate::OBSERVE);
    let hold = hold.expect("kept with the reservation");
    assert_eq!(
        (hold.mode, hold.a_inventory_usd_micro),
        (HoldMode::Observe, 0)
    );
    assert!(
        matches!(&hold.b, BHold::Exact(f) if f.inventory_usd_micro == 7 && f.flows["SOL"].reserve == 205)
    );
    assert_eq!(reserved, 0, "OBSERVE reserves A");
    let mut evidence = BEvidence::default();
    evidence.per_kind.insert(
        crate::execution::hold_b::class_of("ZERO_PREFUND_SAME_TX", "PLAIN"),
        (1, 0),
    );
    evidence.exact = 1;
    let active = HoldGate {
        mode: HoldMode::Active,
        n: Some(1),
        evidence,
    };
    let (hold, reserved) = run(&active);
    let hold = hold.unwrap();
    assert_eq!(hold.mode, HoldMode::Active);
    assert_eq!(reserved, 7, "an effective ACTIVE reserves B");
    assert_eq!(
        hold.reserved_inventory_usd_micro, 7,
        "the journal keeps what was reserved"
    );
    // A refused admission releases exactly what was reserved (B's value), nothing stays held.
    let dir = std::env::temp_dir().join(format!("p0-dec04-refused-{}", Pubkey::new_unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, mut s) = (adapter(), safety());
    let mut fe04 = admission(&dir, &payer, target, "ZERO_PREFUND_SAME_TX");
    fe04.approval.max_total_sol_debit_lamports = 0;
    let r = evaluate_in(
        crate::execution::pipeline::Mode::Live,
        (&a, &mut s, Some(&mut fe04)),
        &payer,
        target,
        u64::MAX,
        &active,
    );
    assert_eq!(r.decision, Decision::Unavailable, "{:?}", r.reason);
    assert_eq!((s.inventory_reserved(), s.capital_in_use()), (0, 0));
    let _ = std::fs::remove_dir_all(dir);
    let off = HoldGate {
        mode: HoldMode::Off,
        ..HoldGate::OBSERVE
    };
    assert_eq!(run(&off), (None, 0), "OFF computes nothing");
}

/// B4: a plan that creates the liquidation record carries its rent into the admission: above
/// the approval's `max_rent_lamports` it is refused before anything durable or sent (and the
/// Safety reservation released); approved, the intent binds the rent and the SOL hold covers fee +
/// tip + flat fee + rent.
#[test]
fn b4_admission_binds_a_nonzero_record_rent() {
    use darkbot_project0::safety::{Allow, Limits, Safety};
    let target = Pubkey::new_unique();
    let payer = solana_sdk::signature::Keypair::new();
    let safety = || {
        let mut allow = Allow::default();
        allow.execution_classes.insert("longtail");
        Safety::new(
            Limits {
                max_total_capital: u64::MAX,
                max_outstanding_inventory: u64::MAX,
                max_concurrent_positions: u32::MAX,
                max_exposure_per_attempt: u64::MAX,
                max_daily_loss: u64::MAX,
                max_failed_attempts_per_day: u32::MAX,
                max_tip: u64::MAX,
                max_slippage_bps: u64::MAX,
                max_outstanding_attempts: u32::MAX,
            },
            allow,
            true,
        )
    };
    let adapter = || HandoffSubmitter {
        submits: Arc::new(AtomicU64::new(0)),
        sims: Arc::new(AtomicU64::new(0)),
        transport_fails: false,
        simulate_delay: Duration::ZERO,
        simulate_delay_again: Duration::ZERO,
        context_slot: Some(10),
        context_slot_again: Some(10),
    };
    ADMISSION_RENT.with(|r| r.set(3_000));
    let dir = |n: &str| {
        let d = std::env::temp_dir().join(format!("p0-b4-rent-{n}-{}", Pubkey::new_unique()));
        std::fs::create_dir_all(&d).unwrap();
        d
    };
    // Rent above the approval: refused, nothing durable, nothing sent, nothing reserved.
    let (refused_dir, refused) = (dir("refused"), adapter());
    let mut s = safety();
    let mut fe04 = admission(&refused_dir, &payer, target, "ZERO_PREFUND_SAME_TX");
    let r = live_evaluate(&refused, &mut s, Some(&mut fe04), &payer, target);
    assert_eq!(r.decision, Decision::Unavailable, "{:?}", r.reason);
    assert!(
        r.reason_text()
            .as_deref()
            .unwrap_or("")
            .contains("APPROVAL_BUDGET_EXCEEDED"),
        "{:?}",
        r.reason
    );
    assert_eq!(refused.submits.load(Ordering::Relaxed), 0);
    assert!(fe04.journal.active_hold().is_none());
    assert_eq!(s.capital_in_use(), 0, "the Safety reservation is released");
    // Approved rent: bound into the intent and the SOL hold.
    let (ok_dir, ok) = (dir("ok"), adapter());
    let mut s = safety();
    let mut fe04 = admission(&ok_dir, &payer, target, "ZERO_PREFUND_SAME_TX");
    fe04.approval.max_rent_lamports = 3_000;
    let r = live_evaluate(&ok, &mut s, Some(&mut fe04), &payer, target);
    assert_eq!(r.decision, Decision::Submitted, "{:?}", r.reason);
    assert_eq!(fe04.journal.intent().unwrap().rent_lamports, 3_000);
    assert_eq!(
        fe04.journal.active_hold().unwrap().holds["SOL"],
        3_205,
        "fee 200 + tip 0 + flat fee 5 + record rent 3000"
    );
    assert_eq!(ok.submits.load(Ordering::Relaxed), 1);
    ADMISSION_RENT.with(|r| r.set(0));
    let _ = std::fs::remove_dir_all(refused_dir);
    let _ = std::fs::remove_dir_all(ok_dir);
}

/// V9 through the LIVE `Pipeline::evaluate`: a plan the economics would act on that a Safety
/// limit refuses is attributed to SAFETY with decision DeniedBySafety and the explicit denial
/// text — in the plan preview (before V9 an economic skip) and at the admission boundary
/// (before V9 no loss at all) — and nothing is reserved, journaled or sent.
#[test]
fn v9_safety_denials_are_their_own_cause() {
    use darkbot_project0::safety::{Allow, Limits, Safety};
    let target = Pubkey::new_unique();
    let payer = solana_sdk::signature::Keypair::new();
    let safety = |max_total_capital: u64, max_daily_loss: u64| {
        let mut allow = Allow::default();
        allow.execution_classes.insert("longtail");
        Safety::new(
            Limits {
                max_total_capital,
                max_outstanding_inventory: u64::MAX,
                max_concurrent_positions: u32::MAX,
                max_exposure_per_attempt: u64::MAX,
                max_daily_loss,
                max_failed_attempts_per_day: u32::MAX,
                max_tip: u64::MAX,
                max_slippage_bps: u64::MAX,
                max_outstanding_attempts: u32::MAX,
            },
            allow,
            true,
        )
    };
    let adapter = HandoffSubmitter {
        submits: Arc::new(AtomicU64::new(0)),
        sims: Arc::new(AtomicU64::new(0)),
        transport_fails: false,
        simulate_delay: Duration::ZERO,
        simulate_delay_again: Duration::ZERO,
        context_slot: Some(10),
        context_slot_again: Some(10),
    };
    // Preview: no capital at all — the only plan is refused before the economics act.
    let mut s = safety(0, u64::MAX);
    let r = live_evaluate(&adapter, &mut s, None, &payer, target);
    assert_eq!(
        (r.decision, r.loss),
        (Decision::DeniedBySafety, Some(LossReason::Safety)),
        "{:?}",
        r.reason
    );
    assert!(
        r.reason_text()
            .as_deref()
            .unwrap_or("")
            .contains("safety TotalCapital"),
        "{:?}",
        r.reason
    );
    // V2-04: a plan Safety refuses at the economic gate is no detection.
    assert_eq!((r.timing.t_detect_ns, r.detect_slot), (None, None));
    // Admission boundary: the plan passes the preview, the day's loss limit refuses its worst case.
    let dir = std::env::temp_dir().join(format!("p0-v9-safety-{}", Pubkey::new_unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut fe04 = admission(&dir, &payer, target, "ZERO_PREFUND_SAME_TX");
    let mut s = safety(u64::MAX, 1);
    let r = live_evaluate(&adapter, &mut s, Some(&mut fe04), &payer, target);
    assert_eq!(
        (r.decision, r.loss, r.reason_text().as_deref()),
        (
            Decision::DeniedBySafety,
            Some(LossReason::Safety),
            Some("DailyLoss")
        )
    );
    assert_eq!(adapter.submits.load(Ordering::Relaxed), 0);
    assert!(fe04.journal.active_hold().is_none());
    assert_eq!(s.capital_in_use(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

/// B1/FE-04: the admission is the pipeline's only send. A LIVE candidate is reserved durably
/// first (intent bound to the exact signed bytes, fee, tip, rent and the full SOL debit), then
/// submitted exactly once; a second trigger never submits again; a refusal before anything
/// durable sends nothing and releases the Safety reservation; a lost transport after the durable
/// handoff stays UNKNOWN with its hold and halts.
#[test]
fn fe04_admission_is_the_only_send_and_hands_off_exactly_once() {
    use darkbot_project0::safety::{Allow, Limits, Safety};
    let target = Pubkey::new_unique();
    let payer = solana_sdk::signature::Keypair::new();
    let run =
        |adapter: &HandoffSubmitter,
         safety: &mut Safety,
         fe04: Option<&mut crate::execution::one_shot::admission::Fe04Admission>| {
            live_evaluate(adapter, safety, fe04, &payer, target)
        };
    let open_safety = || {
        let mut allow = Allow::default();
        allow.execution_classes.insert("longtail");
        Safety::new(
            Limits {
                max_total_capital: u64::MAX,
                max_outstanding_inventory: u64::MAX,
                max_concurrent_positions: u32::MAX,
                max_exposure_per_attempt: u64::MAX,
                max_daily_loss: u64::MAX,
                max_failed_attempts_per_day: u32::MAX,
                max_tip: u64::MAX,
                max_slippage_bps: u64::MAX,
                max_outstanding_attempts: u32::MAX,
            },
            allow,
            true,
        )
    };
    let adapter = |transport_fails| HandoffSubmitter {
        submits: Arc::new(AtomicU64::new(0)),
        sims: Arc::new(AtomicU64::new(0)),
        transport_fails,
        simulate_delay: Duration::ZERO,
        simulate_delay_again: Duration::ZERO,
        context_slot: Some(10),
        context_slot_again: Some(10),
    };
    let dir = |n: &str| {
        let d = std::env::temp_dir().join(format!("p0-fe04-adm-{n}-{}", Pubkey::new_unique()));
        std::fs::create_dir_all(&d).unwrap();
        d
    };

    // Durable reserve, then exactly one submit of the simulated bytes.
    let (ok_dir, ok) = (dir("ok"), adapter(false));
    let mut safety = open_safety();
    let mut fe04 = admission(&ok_dir, &payer, target, "ZERO_PREFUND_SAME_TX");
    let r = run(&ok, &mut safety, Some(&mut fe04));
    assert_eq!(r.decision, Decision::Submitted, "{:?}", r.reason);
    assert_eq!(ok.submits.load(Ordering::Relaxed), 1);
    assert_eq!(
        ok.sims.load(Ordering::Relaxed),
        1,
        "DEC-02: the pipeline's simulation of the exact payload is reused at the handoff"
    );
    assert_eq!(
        (fe04.simulation.reused, fe04.simulation.resimulated),
        (1, 0)
    );
    assert!(r.timing.t_handoff_ns.is_some());
    let hold = fe04.journal.active_hold().unwrap();
    assert!(hold.unknown_outcome, "submitted is UNKNOWN until settled");
    assert_eq!(
        fe04.journal.intent().unwrap().last_valid_block_height,
        Some(100),
        "C12: the signed blockhash's expiry is bound"
    );
    assert_eq!(
        hold.holds["SOL"], 205,
        "fee 200 + tip 0 + flat fee 5 + record rent 0"
    );
    assert_eq!(safety.capital_in_use(), 1_000, "reserved until settled");
    assert_eq!(hold.target, target.to_string());
    assert_eq!(r.signature.as_deref(), Some(r.all_signatures[0].as_str()));
    // A second trigger never submits again (the one-shot is consumed) and releases its check.
    let again = run(&ok, &mut safety, Some(&mut fe04));
    assert_eq!(again.decision, Decision::Unavailable);
    assert!(again
        .reason_text()
        .unwrap()
        .contains("ONE_SHOT_ALREADY_CONSUMED"));
    assert_eq!(ok.submits.load(Ordering::Relaxed), 1);

    // FE-02: the day's loss limit holds with this attempt's worst case (SOL debit 205 plus its
    // capital): a budget below it is a Safety denial before any reservation or send.
    let (tight_dir, tight) = (dir("tight"), adapter(false));
    let mut fe04 = admission(&tight_dir, &payer, target, "ZERO_PREFUND_SAME_TX");
    let mut limited = {
        let mut allow = Allow::default();
        allow.execution_classes.insert("longtail");
        Safety::new(
            Limits {
                max_total_capital: u64::MAX,
                max_outstanding_inventory: u64::MAX,
                max_concurrent_positions: u32::MAX,
                max_exposure_per_attempt: u64::MAX,
                max_daily_loss: 204,
                max_failed_attempts_per_day: u32::MAX,
                max_tip: u64::MAX,
                max_slippage_bps: u64::MAX,
                max_outstanding_attempts: u32::MAX,
            },
            allow,
            true,
        )
    };
    let r = run(&tight, &mut limited, Some(&mut fe04));
    assert_eq!(r.decision, Decision::DeniedBySafety, "{:?}", r.reason);
    assert_eq!(r.reason_text().as_deref(), Some("DailyLoss"));
    assert_eq!(tight.submits.load(Ordering::Relaxed), 0);
    assert!(fe04.journal.active_hold().is_none());
    assert_eq!(limited.capital_in_use(), 0, "nothing reserved");
    // FE-02: the admission binds only complete economics (never assumed).
    let mut plans = crate::execution::pipeline::ReceivershipBuilder::plans(
        &AdmissionBuilder,
        &target,
        0,
        0,
        std::time::Instant::now(),
    )
    .unwrap();
    let complete = &plans[0].input;
    assert!(crate::execution::one_shot::admission::economics_known(
        complete
    ));
    plans[0].input.gross_seized_realizable_usd_micro = None;
    assert!(!crate::execution::one_shot::admission::economics_known(
        &plans[0].input
    ));
    plans[0].input.gross_seized_realizable_usd_micro = Some(1);
    plans[0].input.rent_and_token_usd_micro = None;
    assert!(!crate::execution::one_shot::admission::economics_known(
        &plans[0].input
    ));

    // Refused before anything durable: nothing sent, no hold, Safety released.
    let (refused_dir, none) = (dir("refused"), adapter(false));
    let mut safety = open_safety();
    let mut fe04 = admission(&refused_dir, &payer, target, "NO_SWAP_INVENTORY");
    let r = run(&none, &mut safety, Some(&mut fe04));
    assert_eq!(r.decision, Decision::Unavailable);
    assert_eq!(
        r.reason_text().as_deref(),
        Some("FE04_FUNDING_CLASS_MISMATCH")
    );
    assert_eq!(none.submits.load(Ordering::Relaxed), 0);
    assert!(fe04.journal.active_hold().is_none());
    assert_eq!((safety.capital_in_use(), safety.halted()), (0, None));
    // The full SOL debit (with the flat fee) is checked against the approval.
    drop(fe04);
    let mut fe04 = admission(&refused_dir, &payer, target, "ZERO_PREFUND_SAME_TX");
    fe04.approval.max_total_sol_debit_lamports = 204;
    let r = run(&none, &mut safety, Some(&mut fe04));
    assert_eq!(
        r.reason_text().as_deref(),
        Some("APPROVAL_BUDGET_EXCEEDED:SOL_DEBIT")
    );
    assert_eq!(
        (
            none.submits.load(Ordering::Relaxed),
            safety.capital_in_use()
        ),
        (0, 0)
    );

    // Lost transport after the durable handoff: UNKNOWN, hold kept, Safety halted.
    let (lost_dir, lost) = (dir("lost"), adapter(true));
    let mut safety = open_safety();
    let mut fe04 = admission(&lost_dir, &payer, target, "ZERO_PREFUND_SAME_TX");
    let r = run(&lost, &mut safety, Some(&mut fe04));
    assert_eq!(r.decision, Decision::AmbiguousSubmission);
    assert_eq!(lost.submits.load(Ordering::Relaxed), 1);
    assert!(fe04
        .journal
        .active_hold()
        .is_some_and(|h| h.unknown_outcome));
    assert_eq!(
        safety.halted(),
        Some(darkbot_common::error::HaltReason::UnknownEconomics)
    );
    assert_eq!(safety.capital_in_use(), 1_000, "UNKNOWN releases nothing");

    // Without an admission LIVE still never sends.
    let (_, idle) = (dir("idle"), adapter(false));
    let mut safety = open_safety();
    assert_eq!(
        run(&idle, &mut safety, None).decision,
        Decision::Unavailable
    );
    assert_eq!(idle.submits.load(Ordering::Relaxed), 0);
    for d in [ok_dir, refused_dir, lost_dir] {
        std::fs::remove_dir_all(d).unwrap();
    }
}
/// B18: a plan with a tip on a submitter without bundle path is signed as one transaction:
/// T_sign and its slot are recorded once, at that signature (before: no T_sign at all).
#[test]
fn b18_single_tx_with_tip_marks_t_sign_once() {
    use crate::execution::pipeline::{Mode, Pipeline, ReceivershipBuilder, ReceivershipPlan};
    use darkbot_project0::{
        execution::Freshness,
        planner::Allowlist,
        safety::{Allow, Limits, Safety},
    };
    use darkbot_solana::{
        state::blockhash::{BlockhashCache, CachedBlockhash},
        tx::{Resources, TxLimits},
    };
    struct TipBuilder;
    impl ReceivershipBuilder for TipBuilder {
        fn native_cost_usd_micro(&self, n: u64) -> Option<u64> {
            Some(n)
        }
        fn instruction_table(&self) -> &darkbot_project0::planner::InstructionTable {
            test_table()
        }
        fn plans(
            &self,
            a: &Pubkey,
            now: u64,
            slot: u64,
            deadline: std::time::Instant,
        ) -> Result<Vec<ReceivershipPlan>, crate::execution::pipeline::PlanError> {
            let mut plans = TestBuilder.plans(a, now, slot, deadline)?;
            plans[0].tip = 7;
            Ok(plans)
        }
    }
    let calls = Arc::new(AtomicU64::new(0));
    let sims = Arc::new(AtomicU64::new(0));
    let submitter = CountingSubmitter {
        calls: calls.clone(),
        sims: sims.clone(),
    };
    let clock = Clock::new();
    let bh = BlockhashCache::new();
    bh.set(CachedBlockhash {
        hash: solana_sdk::hash::Hash::new_unique(),
        last_valid_block_height: 100,
        refreshed_ns: clock.now_ns(),
        context_slot: None,
    });
    let zero = Limits {
        max_total_capital: 0,
        max_outstanding_inventory: 0,
        max_concurrent_positions: 0,
        max_exposure_per_attempt: 0,
        max_daily_loss: 0,
        max_failed_attempts_per_day: 0,
        max_tip: 0,
        max_slippage_bps: 0,
        max_outstanding_attempts: 0,
    };
    let mut safety = Safety::new(zero, Allow::default(), false);
    let payer = solana_sdk::signature::Keypair::new();
    let allow = Allowlist::default();
    let failed = FailedCostMatrix::default();
    let slots = darkbot_solana::ingress::SlotWatch::new();
    slots.observe(10);
    let mut network = crate::execution::pipeline::NetworkBudget::unbounded();
    let mut p = Pipeline {
        mode: Mode::Shadow,
        clock: &clock,
        health: &TestHealth,
        builder: &TipBuilder,
        submitter: &submitter,
        safety: &mut safety,
        blockhash: &bh,
        payer: &payer,
        allowlist: &allow,
        failed_costs: &failed,
        limits: TxLimits::default(),
        resources: Resources {
            compute_unit_limit: 100_000,
            priority_micro_lamports_per_cu: 0,
            loaded_accounts_data_size: None,
        },
        min_profit: 0,
        blockhash_max_age_ns: u64::MAX,
        p7_passed: false,
        freshness: Freshness {
            max_opportunity_age_ns: u64::MAX,
            execution_deadline_ns: u64::MAX,
        },
        bundle_tip: None,
        global_allowed: std::slice::from_ref(&test_table().program),
        prereq_allowed: &[],
        slot_watch: &slots,
        network: &mut network,
        simulation_failure: None,
        blockhash_park: crate::execution::pipeline::BlockhashPark::Off,
        fe04: None,
        hold: &crate::execution::hold_b::HoldGate::OBSERVE,
        egress: None,
        synthetic: crate::execution::pipeline::test_synthetic(),
    };
    let r = p.evaluate(Pubkey::new_unique(), "test", clock.now_ns(), 100, 10, true);
    assert_eq!(r.simulation_result.as_deref(), Some("PASS"));
    assert!(r.timing.t_sign_ns.is_some(), "{:?}", r.timing);
    assert_eq!(r.sign_slot, Some(10));
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}
/// E2: the pipeline gate classifies the built instructions themselves: an instruction after the
/// real End, or an unknown discriminator of the receivership program, rejects the plan before
/// any build, signature or simulation.
#[test]
fn e2_pipeline_rejects_plans_by_their_built_instructions() {
    use crate::execution::pipeline::{
        Mode, NetworkBudget, Pipeline, ReceivershipBuilder, ReceivershipPlan,
    };
    use darkbot_project0::{
        execution::Freshness,
        planner::Allowlist,
        safety::{Allow, Limits, Safety},
    };
    use darkbot_solana::{
        state::blockhash::{BlockhashCache, CachedBlockhash},
        tx::{Resources, TxLimits},
    };
    struct Rewritten(fn(&mut Vec<solana_sdk::instruction::Instruction>));

    impl ReceivershipBuilder for Rewritten {
        fn native_cost_usd_micro(&self, n: u64) -> Option<u64> {
            Some(n)
        }
        fn instruction_table(&self) -> &darkbot_project0::planner::InstructionTable {
            test_table()
        }
        fn plans(
            &self,
            a: &Pubkey,
            now: u64,
            slot: u64,
            deadline: std::time::Instant,
        ) -> Result<Vec<ReceivershipPlan>, crate::execution::pipeline::PlanError> {
            let mut plans = TestBuilder.plans(a, now, slot, deadline)?;
            (self.0)(&mut plans[0].instructions);
            Ok(plans)
        }
    }
    type Rewrite = fn(&mut Vec<solana_sdk::instruction::Instruction>);
    let cases: [(Rewrite, &str); 3] = [
        (|ixs| ixs.push(test_ix([9; 8])), "UnknownInstruction(4)"),
        (
            |ixs| ixs.insert(1, test_ix([9; 8])),
            "UnknownInstruction(1)",
        ),
        (
            |ixs| {
                ixs.swap(2, 3);
            },
            "EndNotLast",
        ),
    ];
    for (rewrite, expected) in cases {
        let calls = Arc::new(AtomicU64::new(0));
        let sims = Arc::new(AtomicU64::new(0));
        let submitter = CountingSubmitter {
            calls: calls.clone(),
            sims: sims.clone(),
        };
        let clock = Clock::new();
        let bh = BlockhashCache::new();
        bh.set(CachedBlockhash {
            hash: solana_sdk::hash::Hash::new_unique(),
            last_valid_block_height: 100,
            refreshed_ns: clock.now_ns(),
            context_slot: None,
        });
        let zero = Limits {
            max_total_capital: 0,
            max_outstanding_inventory: 0,
            max_concurrent_positions: 0,
            max_exposure_per_attempt: 0,
            max_daily_loss: 0,
            max_failed_attempts_per_day: 0,
            max_tip: 0,
            max_slippage_bps: 0,
            max_outstanding_attempts: 0,
        };
        let mut safety = Safety::new(zero, Allow::default(), false);
        let payer = solana_sdk::signature::Keypair::new();
        let allow = Allowlist::default();
        let failed = FailedCostMatrix::default();
        let slots = darkbot_solana::ingress::SlotWatch::new();
        let mut network = NetworkBudget::unbounded();
        let builder = Rewritten(rewrite);
        let mut p = Pipeline {
            mode: Mode::Shadow,
            clock: &clock,
            health: &TestHealth,
            builder: &builder,
            submitter: &submitter,
            safety: &mut safety,
            blockhash: &bh,
            payer: &payer,
            allowlist: &allow,
            failed_costs: &failed,
            limits: TxLimits::default(),
            resources: Resources {
                compute_unit_limit: 100_000,
                priority_micro_lamports_per_cu: 0,
                loaded_accounts_data_size: None,
            },
            min_profit: 0,
            blockhash_max_age_ns: u64::MAX,
            p7_passed: false,
            freshness: Freshness {
                max_opportunity_age_ns: u64::MAX,
                execution_deadline_ns: u64::MAX,
            },
            bundle_tip: None,
            global_allowed: std::slice::from_ref(&test_table().program),
            prereq_allowed: &[],
            slot_watch: &slots,
            network: &mut network,
            simulation_failure: None,
            blockhash_park: crate::execution::pipeline::BlockhashPark::Off,
            fe04: None,
            hold: &crate::execution::hold_b::HoldGate::OBSERVE,
            egress: None,
            synthetic: crate::execution::pipeline::test_synthetic(),
        };
        let r = p.evaluate(Pubkey::new_unique(), "test", clock.now_ns(), 100, 10, true);
        let reason = r.reason_text().unwrap_or_default();
        assert!(
            reason.contains(&format!("plan invalid: {expected}")),
            "{reason}"
        );
        assert_eq!(r.progress.transaction_built, None);
        assert_eq!(sims.load(Ordering::Relaxed), 0);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }
}
/// D2: NoSwap and Preheld inventory plans with byte-identical transactions share one fee
/// quote within the evaluation; both funding classes keep their own cost record with it.
#[test]
fn d2_identical_funding_plans_share_one_fee_quote() {
    use crate::execution::pipeline::{
        Mode, NetworkBudget, Pipeline, ReceivershipBuilder, ReceivershipPlan,
    };
    use darkbot_project0::{
        execution::Freshness,
        funding::FundingClass,
        planner::Allowlist,
        safety::{Allow, Limits, Safety},
    };
    use darkbot_solana::{
        state::blockhash::{BlockhashCache, CachedBlockhash},
        tx::{Resources, TxLimits},
    };
    struct FeeCounting(Arc<AtomicU64>);
    impl SubmissionAdapter for FeeCounting {
        fn path(&self) -> SubmissionPath {
            SubmissionPath::Rpc
        }
        fn fee_for_message(
            &self,
            tx: &VersionedTransaction,
            _: std::time::Instant,
        ) -> Result<MessageFeeQuote, darkbot_solana::submit::FeeQuoteError> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(MessageFeeQuote {
                message_hash: tx.message.hash(),
                context_slot: 1,
                lamports: 5_000,
            })
        }
        fn simulate(&self, _: &VersionedTransaction, _: std::time::Instant) -> SimResult {
            SimResult::Ok {
                units_consumed: Some(100),
                context_slot: None,
            }
        }
        fn simulate_bundle(&self, _: &[VersionedTransaction], _: std::time::Instant) -> SimResult {
            SimResult::Unavailable("test adapter: no bundle simulation".into())
        }
        fn submit(
            &self,
            _: &VersionedTransaction,
            _: u64,
            _: std::time::Instant,
        ) -> Result<SubmitAck, SubmitError> {
            panic!("must never broadcast")
        }
        fn submit_bundle(
            &self,
            _: &[VersionedTransaction],
            _: u64,
            _: std::time::Instant,
        ) -> Result<SubmitAck, SubmitError> {
            panic!("must never broadcast bundle")
        }
    }
    struct TwoClasses(bool);
    impl ReceivershipBuilder for TwoClasses {
        fn native_cost_usd_micro(&self, n: u64) -> Option<u64> {
            Some(n)
        }
        fn instruction_table(&self) -> &darkbot_project0::planner::InstructionTable {
            test_table()
        }
        fn plans(
            &self,
            a: &Pubkey,
            now: u64,
            slot: u64,
            deadline: std::time::Instant,
        ) -> Result<Vec<ReceivershipPlan>, crate::execution::pipeline::PlanError> {
            let mut first = TestBuilder.plans(a, now, slot, deadline)?.remove(0);
            first.input.funding = FundingClass::NoSwapInventory;
            let mut second = TestBuilder.plans(a, now, slot, deadline)?.remove(0);
            second.input.funding = FundingClass::PreheldInventory;
            if self.0 {
                // A different transaction (one more byte of data): its own quote.
                second.instructions[1].data.push(0);
            }
            Ok(vec![first, second])
        }
    }
    for (different, expected_calls) in [(false, 1u64), (true, 2)] {
        let fees = Arc::new(AtomicU64::new(0));
        let submitter = FeeCounting(fees.clone());
        let clock = Clock::new();
        let bh = BlockhashCache::new();
        bh.set(CachedBlockhash {
            hash: solana_sdk::hash::Hash::new_unique(),
            last_valid_block_height: 100,
            refreshed_ns: clock.now_ns(),
            context_slot: None,
        });
        let zero = Limits {
            max_total_capital: 0,
            max_outstanding_inventory: 0,
            max_concurrent_positions: 0,
            max_exposure_per_attempt: 0,
            max_daily_loss: 0,
            max_failed_attempts_per_day: 0,
            max_tip: 0,
            max_slippage_bps: 0,
            max_outstanding_attempts: 0,
        };
        let mut safety = Safety::new(zero, Allow::default(), false);
        let payer = solana_sdk::signature::Keypair::new();
        let allow = Allowlist::default();
        let failed = FailedCostMatrix::default();
        let slots = darkbot_solana::ingress::SlotWatch::new();
        let mut network = NetworkBudget::unbounded();
        let builder = TwoClasses(different);
        let mut p = Pipeline {
            mode: Mode::Shadow,
            clock: &clock,
            health: &TestHealth,
            builder: &builder,
            submitter: &submitter,
            safety: &mut safety,
            blockhash: &bh,
            payer: &payer,
            allowlist: &allow,
            failed_costs: &failed,
            limits: TxLimits::default(),
            resources: Resources {
                compute_unit_limit: 100_000,
                priority_micro_lamports_per_cu: 0,
                loaded_accounts_data_size: None,
            },
            min_profit: 0,
            blockhash_max_age_ns: u64::MAX,
            p7_passed: false,
            freshness: Freshness {
                max_opportunity_age_ns: u64::MAX,
                execution_deadline_ns: u64::MAX,
            },
            bundle_tip: None,
            global_allowed: std::slice::from_ref(&test_table().program),
            prereq_allowed: &[],
            slot_watch: &slots,
            network: &mut network,
            simulation_failure: None,
            blockhash_park: crate::execution::pipeline::BlockhashPark::Off,
            fe04: None,
            hold: &crate::execution::hold_b::HoldGate::OBSERVE,
            egress: None,
            synthetic: crate::execution::pipeline::test_synthetic(),
        };
        let r = p.evaluate(Pubkey::new_unique(), "test", clock.now_ns(), 100, 10, true);
        assert_eq!(
            fees.load(Ordering::Relaxed),
            expected_calls,
            "different={different}"
        );
        let labels: Vec<_> = r
            .candidate_costs
            .iter()
            .map(|c| c.funding.clone())
            .collect();
        assert_eq!(labels.len(), 2, "{labels:?}");
        assert!(r
            .candidate_costs
            .iter()
            .all(|c| c.message_fee_quote.is_some()));
    }
}
#[cfg(feature = "historical")]
#[test]
fn runtime_worker_writes_four_snapshots_then_identical_final() {
    four_snapshots_then_identical_final(crate::run_layout::RunLayout::V1);
}
#[cfg(feature = "historical")]
#[test]
fn runtime_worker_writes_layout_v2_capture_and_snapshots() {
    let root = four_snapshots_then_identical_final(crate::run_layout::RunLayout::V2);
    let mut names: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["capture", "snapshots"]);
    // PERSIST-01: one binary evaluation stream, no JSONL stream and no separate offset index.
    assert!(root.join("capture/shadow_evaluations.p0j").is_file());
    for name in ["shadow_evaluations.jsonl", "shadow_offsets.bin"] {
        assert!(!root.join("capture").join(name).exists(), "{name}");
    }
    // The sealed report journal holds every document as a typed frame of its kind's current
    // version and the run wrote no JSON view (JFI-03).
    let mut reader = journal::Reader::open(&root.join("snapshots").join(REPORT_JOURNAL)).unwrap();
    let mut kinds = BTreeSet::new();
    while let Some(frame) = reader.next_frame().unwrap() {
        if frame.kind != journal::MANIFEST {
            assert_eq!(
                Some(frame.schema),
                crate::analytics::payload::registered_schema(frame.kind),
                "kind {}",
                frame.kind
            );
            kinds.insert(frame.kind);
        }
    }
    assert!(reader.sealed && reader.complete);
    // No canonical event in this worker-only run, hence no ECON_CANONICAL frame.
    assert_eq!(
        kinds,
        BTreeSet::from([
            kind::ECON_SNAPSHOT,
            kind::ECON_FINAL,
            kind::ECON_OPPORTUNITY
        ])
    );
    assert_no_json(&root);
}
#[cfg(feature = "historical")]
#[test]
fn historical_views_are_checked_read_only_against_the_journals() {
    use crate::analytics::renders;
    let root = four_snapshots_then_identical_final(crate::run_layout::RunLayout::V2);
    // A worker-only run has no manifest.json, which `detect` needs to recognize layout V2.
    assert_eq!(
        renders::detect(&root).unwrap().layout,
        crate::run_layout::RunLayout::V1
    );
    let paths = crate::run_layout::RunPaths::new(&root, crate::run_layout::RunLayout::V2);
    let status = || -> BTreeMap<String, &'static str> {
        renders::check(&paths)
            .unwrap()
            .into_iter()
            .map(|(p, s)| (p.strip_prefix(&root).unwrap().display().to_string(), s))
            .collect()
    };
    // The run wrote no view (P0-PROD-CTRL-02): every one is MISSING and none is rendered into it.
    let before = status();
    for name in [
        "snapshots/hour_01.json",
        "snapshots/hour_04.json",
        "snapshots/economics_final.json",
        "snapshots/opportunity_economics.jsonl",
        "snapshots/canonical_market_shadow.jsonl",
        "capture/measurement_phases.jsonl",
    ] {
        assert_eq!(
            before.get(name).copied(),
            Some("MISSING"),
            "{name} {before:?}"
        );
    }
    // A historical run carries the views written in its time: the read-only check accepts exactly
    // the byte-exact render of its journal frames.
    for v in renders::views(&paths).unwrap() {
        if v.journal.exists() {
            if let Some(bytes) = renders::render(&v).unwrap() {
                fs::write(&v.output, bytes).unwrap();
            }
        }
    }
    assert!(status().values().all(|s| *s == "MATCHES"));
    let (frames, last) = report_frames(&root.join("snapshots").join(REPORT_JOURNAL));
    let pretty = |v: &Value| format!("{}\n", serde_json::to_string_pretty(v).unwrap());
    let hour4 = frames.iter().rev().find(|f| f.index == 4).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("snapshots/hour_04.json")).unwrap(),
        pretty(&view(&hour4.report))
    );
    assert_eq!(
        fs::read_to_string(root.join("snapshots/economics_final.json")).unwrap(),
        pretty(&view(&last.unwrap()))
    );
    // A changed view is never accepted and never overwritten.
    let hour = root.join("snapshots/hour_02.json");
    let mut bytes = fs::read(&hour).unwrap();
    bytes.insert(1, b' ');
    fs::write(&hour, &bytes).unwrap();
    let err = renders::check(&paths).unwrap_err();
    assert!(err.starts_with("RENDER_MISMATCH"), "{err}");
    assert_eq!(fs::read(&hour).unwrap(), bytes);
}
#[cfg(feature = "historical")]
fn four_snapshots_then_identical_final(layout: crate::run_layout::RunLayout) -> std::path::PathBuf {
    let mut w = worker_in(layout);
    complete(&mut w);
    w.snapshots = 0;
    w.accept(Evidence::Shadow(Box::new(rec()))).unwrap();
    let h = w.h.clone();
    let paths = h.cfg.paths();
    let join = std::thread::spawn(move || w.run());
    // Real automatic timer; no manually invoked snapshot writer in this integration test.
    // The worker polls periodically; a fixed 150 ms grace races its next poll
    // under parallel test load. Wait for the real automatic fourth snapshot.
    let reports = paths.snapshot(REPORT_JOURNAL);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let fourth = || reports.exists() && report_frames(&reports).0.iter().any(|f| f.index == 4);
    while !fourth() && !join.is_finished() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    h.shared.finish.store(true, Ordering::Release);
    join.join().unwrap().unwrap();
    // The final report is the newest snapshot of the last index; every boundary has a frame.
    let (frames, end) = report_frames(&reports);
    let hour = frames.iter().rev().find(|f| f.index == 4).unwrap();
    assert_eq!(view(&hour.report), view(&end.unwrap()));
    for n in 1..=4 {
        assert!(frames.iter().any(|f| f.index == n), "{n}");
    }
    assert_no_json(&paths.root);
    paths.root
}

#[test]
fn unknown_capital_does_not_remove_market_match() {
    let mut r = rec();
    r.selected_costs.as_mut().unwrap().rent_and_token_usd_micro = None;
    let mut b = Capital::new(200);
    b.add(&r, true, true, &RaceClass::SameSlotUnknown);
    assert_eq!(b.matched, 1);
    assert_eq!(b.capital_unknown, 1);
    assert_eq!(b.fundable.count, 0);
}

#[cfg(feature = "historical")]
#[test]
fn backfill_preserves_chain_race_but_never_live_rx_or_speedup() {
    let r = rec();
    let (win, mut market) = pair(&r, 4, 50);
    market.source = MarketEvidenceSource::GapBackfill;
    market.t_rx_ns = None;
    for (slot, expected) in [
        (3, RaceClass::PreWinnerSlotProven),
        (4, RaceClass::SameSlotUnknown),
        (5, RaceClass::PostWinnerSlotProvenLate),
    ] {
        let mut r = r.clone();
        r.sign_slot = Some(slot);
        let race = race(&r, Some(&win), Some(&market), Some(100), true);
        assert_eq!(race.class, expected);
        assert!(!race.live_timestamp_qualified);
        assert_eq!(race.margin_to_winner_rx_ns, None);
        assert_eq!(race.margin_to_winner_slot_seen_ns, None);
        assert_eq!(race.required_speedup_to_winner_rx_ns, None);
        assert_eq!(race.required_speedup_to_winner_slot_seen_ns, None);
    }
    let mut w = worker();
    w.accept(Evidence::Market(market)).unwrap();
    w.accept(Evidence::Winner {
        target: r.account.clone(),
        rx_slot: 1,
        winner: win,
    })
    .unwrap();
    w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
    let output = opportunity_view(&w.opportunity(&r));
    assert_eq!(output["strong_join"], true);
    assert_eq!(output["t_winner_rx_ns"], Value::Null);
    assert_eq!(view(&w.metrics(0, 3))["market"]["liquidation_events"], 0);
    assert_eq!(view(&w.metrics(3, 4))["market"]["liquidation_events"], 1);
    assert_eq!(
        view(&w.metrics(0, 4))["matching"]["winner_live_observation_matched"],
        0
    );
}

// Deliberately preserve the pre-fix forward reduction as a differential oracle.
#[cfg(feature = "historical")]
fn original_selection(w: &Worker, rows: &[(usize, OpportunityRecord)]) -> Value {
    let mut selected: BTreeMap<String, (usize, &OpportunityRecord)> = BTreeMap::new();
    for (hour, r) in rows {
        let win = w.winner_for(r);
        let key = match win {
            Some(win) => format!("{}:winner:{}", r.account, win.slot),
            None => format!("{}:unresolved:{hour}", r.account),
        };
        let assigned = w
            .market_for(r, win)
            .and_then(|m| w.market_hour(m))
            .unwrap_or(*hour);
        let replace = match selected.get(&key) {
            None => true,
            Some((_, old)) if executable(old) => {
                executable(r) && r.timing.t_sign_ns < old.timing.t_sign_ns
            }
            Some(_) => true,
        };
        if replace {
            selected.insert(key, (assigned, r));
        }
    }
    serde_json::to_value(selected.into_values().collect::<Vec<_>>()).unwrap()
}

#[cfg(feature = "historical")]
#[test]
fn million_revisions_stream_every_row_with_bounded_semantic_retention() {
    let mut w = worker();
    w.h.cfg.retention = 16;
    let mut r = rec();
    r.candidate_costs.clear();
    r.selected_costs = None;
    r.economics_known = false;
    r.decision = Decision::Skip;
    let accounts: Vec<_> = (0..4).map(|_| Pubkey::new_unique().to_string()).collect();
    for n in 0..1_000_000 {
        r.account.clone_from(&accounts[n % 4]);
        r.timing.t_rx_ns = Some((n / 250_000) as u64 * 1_000_000_000 + 10);
        w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
    }
    assert_eq!(w.evaluation_counts, [250_000; 4]);
    assert_eq!(w.shadow_evaluations_received, 1_000_000);
    assert_eq!(w.shadow_evaluations_streamed, 1_000_000);
    assert_eq!(w.shadow_evaluations_coalesced, 999_984);
    assert_eq!(w.shadow.len(), 16);
    assert_eq!(w.shadow_semantic_entries_max, 16);
    assert_eq!(w.shadow_heads.len(), 4);
    assert_eq!(w.shadow_semantic_evictions, 0);
    assert_eq!(w.h.shared.retention_overflow.load(Ordering::Relaxed), 0);
    let path = journal::binary_path(&w.h.cfg.run_dir.join("shadow_evaluations.jsonl"));
    // Typed frames across the linked segment chain (SK-07), every evaluation exactly once.
    let mut raw = crate::storage::segments::ChainReader::open(&path).unwrap();
    let mut count = 0;
    while let Some(frame) = raw.next_frame().unwrap() {
        if frame.kind == journal::MANIFEST {
            continue;
        }
        // DEC-01: written under EXHAUSTIVE_V3; DEC-12: the typed record layout; P8-CTRL.4: with
        // its signing (schema 6).
        assert_eq!(
            (frame.kind, frame.schema),
            (kind::ECON_SHADOW_EVALUATION, 6)
        );
        count += 1;
    }
    assert_eq!(count, 1_000_000);
    assert!(raw.segments() > 1, "segments {}", raw.segments());
    complete(&mut w);
    w.final_technical = Some(w.technical(true));
    for i in 1..=4 {
        w.write_snapshot(i, true).unwrap();
        let v = view(&w.report(i, true));
        assert_eq!(v["shadow"]["window_delta"]["evaluations"], 250_000);
        assert_eq!(v["shadow"]["cumulative"]["evaluations"], 250_000 * i);
    }
    let last = view(&report_frames(&w.reports.path).0[3].report);
    assert_eq!(last, view(&w.report(4, true)));
    assert_eq!(last["technical_qualification"]["verdict"], "PASS");
    assert_eq!(last["technical_qualification"]["submissions"], 0);
    println!("RETENTION_STRESS received=1000000 streamed=1000000 coalesced=999984 semantic_max=16 target_heads=4 overflow=0 evictions=0 raw_rows=1000000");
    let path = w.h.cfg.run_dir.clone();
    drop(raw);
    drop(w);
    fs::remove_dir_all(path).unwrap();
}

#[cfg(feature = "historical")]
#[test]
fn late_winners_split_coalesced_groups_exactly_like_original_selection() {
    let mut w = worker();
    let r = rec();
    let mut rows = Vec::new();
    // Deliberately mix rx slots across one hour; a single representative cannot
    // recover these three distinct opportunities without the disk audit trail.
    for n in 0..60 {
        let mut r = r.clone();
        r.rx_slot = Some(1 + n % 3 * 10);
        r.timing.t_rx_ns = Some((n % 2) * 1_000_000_000 + 10);
        r.timing.t_sign_ns = Some(40 + n % 5);
        r.reason = Some(crate::analytics::rejection::Rejection::Plan(format!(
            "revision-{n}"
        )));
        if n % 4 != 0 {
            r.decision = Decision::Skip;
        }
        if n == 20 {
            r.selected_costs.as_mut().unwrap().profit_per_capital_bps = Some(i128::MAX);
        }
        let hour = w.hour(r.timing.t_rx_ns.unwrap()).unwrap();
        rows.push((hour, r.clone()));
        w.accept(Evidence::Shadow(Box::new(r))).unwrap();
        assert_eq!(
            serde_json::to_value(w.selected()).unwrap(),
            original_selection(&w, &rows)
        );
    }
    assert_eq!(w.shadow.len(), 2);
    for slot in [25, 5, 15] {
        let (mut winner, mut market) = pair(&r, slot, 3_000_000_010);
        if let WinnerIdentity::Unique { signature, .. } = &mut winner.identity {
            *signature = format!("winner-{slot}");
            market.signature = signature.clone();
        }
        w.accept(Evidence::Winner {
            target: r.account.clone(),
            rx_slot: 1,
            winner,
        })
        .unwrap();
        assert_eq!(
            serde_json::to_value(w.selected()).unwrap(),
            original_selection(&w, &rows)
        );
        // Late market evidence must change hour assignment, not selection.
        w.accept(Evidence::Market(market)).unwrap();
        assert_eq!(
            serde_json::to_value(w.selected()).unwrap(),
            original_selection(&w, &rows)
        );
    }
    assert_eq!(w.shadow.len(), 3);
    assert!(w.selected().iter().all(|(h, _)| *h == 3));
    assert_eq!(w.evaluation_counts.iter().sum::<u64>(), 60);
    assert_eq!(w.shadow_evaluations_streamed, 60);
    assert_eq!(w.shadow_evaluations_coalesced, 58);
    assert_eq!(w.h.shared.retention_overflow.load(Ordering::Relaxed), 0);
}

#[cfg(feature = "historical")]
#[test]
fn semantic_overflow_and_corrupt_journal_fail_closed() {
    let mut w = worker();
    complete(&mut w);
    w.h.cfg.retention = 1;
    let r = rec();
    for _ in 0..10 {
        w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
    }
    assert_eq!(w.shadow.len(), 1);
    assert_eq!(w.h.shared.retention_overflow.load(Ordering::Relaxed), 0);
    let mut other_hour = r.clone();
    other_hour.timing.t_rx_ns = Some(1_000_000_010);
    w.accept(Evidence::Shadow(Box::new(other_hour))).unwrap();
    assert_eq!(w.shadow_semantic_evictions, 1);
    assert_eq!(w.h.shared.retention_overflow.load(Ordering::Relaxed), 1);
    assert_eq!(w.shadow_evaluations_streamed, 11);
    assert_eq!(view(&w.technical(true))["verdict"], "INCOMPLETE");
    let mut w = worker();
    w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
    // A damaged evaluation frame fails the chain walk closed; nothing is guessed.
    let path = journal::binary_path(&w.h.cfg.run_dir.join("shadow_evaluations.jsonl"));
    let mut bytes = fs::read(&path).unwrap();
    let n = bytes.len();
    bytes[n - 40] ^= 1;
    fs::write(&path, bytes).unwrap();
    let (win, _) = pair(&r, 3, 40);
    assert!(w
        .accept(Evidence::Winner {
            target: r.account,
            rx_slot: 1,
            winner: win
        })
        .is_err());
}

#[test]
fn duplicate_winner_at_capacity_does_not_lose_evidence() {
    let mut w = worker();
    w.h.cfg.retention = 1;
    let r = rec();
    let (win, _) = pair(&r, 3, 40);
    for _ in 0..2 {
        w.accept(Evidence::Winner {
            target: r.account.clone(),
            rx_slot: 1,
            winner: win.clone(),
        })
        .unwrap();
    }
    assert_eq!(w.winners[&r.account].len(), 1);
    assert_eq!(w.h.shared.retention_overflow.load(Ordering::Relaxed), 0);
}

#[test]
fn market_rpc_barrier_waits_for_shadow_and_rejects_failed_shadow() {
    let (tx, _rx) = bounded("barrier-test", 1);
    let h = Handle {
        tx,
        shared: Arc::new(Shared::default()),
        clock: Clock::new(),
        cfg: config(),
    };
    assert_eq!(
        h.wait_shadow_quiescent(Duration::ZERO).unwrap_err(),
        "SHADOW_DRAIN_BARRIER_TIMEOUT"
    );
    let shared = h.shared.clone();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        shared.shadow_done.store(true, Ordering::Release);
    });
    h.wait_shadow_quiescent(Duration::from_secs(2)).unwrap();
    worker.join().unwrap();
    h.shared.shadow_failed.store(true, Ordering::Release);
    assert_eq!(
        h.wait_shadow_quiescent(Duration::ZERO).unwrap_err(),
        "SHADOW_FAILED_BEFORE_MARKET_RECONCILIATION"
    );
}

#[test]
fn startup_barrier_never_starts_clock_on_timeout_or_failed_shadow() {
    let (tx, _rx) = bounded("warmup", 1);
    let h = Handle {
        tx,
        shared: Arc::new(Shared::default()),
        clock: Clock::new(),
        cfg: config(),
    };
    assert_eq!(
        h.wait_shadow_ready(Duration::ZERO).unwrap_err(),
        "SHADOW_STARTUP_TIMEOUT"
    );
    assert_eq!(h.cutoff(), None);
    let shared = h.shared.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        shared.shadow_ready.store(true, Ordering::Release);
    });
    h.wait_shadow_ready(Duration::from_secs(1)).unwrap();
    t.join().unwrap();
    assert_eq!(h.cutoff(), None); // ACK of the subsequent market stream owns the clock.
    h.shared.shadow_failed.store(true, Ordering::Release);
    assert_eq!(
        h.wait_shadow_ready(Duration::ZERO).unwrap_err(),
        "SHADOW_STARTUP_FAILED"
    );
}

#[cfg(feature = "historical")]
#[test]
fn empty_measurement_or_recovery_after_start_cannot_pass() {
    let mut w = worker();
    complete(&mut w);
    assert_eq!(view(&w.technical(true))["verdict"], "INCOMPLETE");
    w.accept(Evidence::Shadow(Box::new(rec()))).unwrap();
    assert_eq!(view(&w.technical(true))["verdict"], "PASS");
    w.h.shared.shadow_ready_ns.store(2, Ordering::Release); // capture started at1
    assert_eq!(view(&w.technical(true))["verdict"], "INCOMPLETE");
}

#[cfg(feature = "historical")]
#[test]
fn measurement_plan_report_and_attribution_agree_for_all_durations() {
    for seconds in [1, 61, 600, 3600, 18000, 86400] {
        let mut w = worker();
        w.h.cfg.plan = MeasurementPlan::new(seconds).unwrap();
        let start = 599_000_000_001; // Bootstrap is deliberately longer than short captures.
        let cutoff = w.h.cfg.plan.cutoff(start).unwrap();
        w.h.start(start, cutoff);
        complete(&mut w);
        w.market_final
            .as_mut()
            .unwrap()
            .summary
            .actual_observation_seconds = seconds as f64;
        for i in 1..=SNAPSHOT_COUNT {
            let end = start + w.h.cfg.plan.offset_ns(i);
            assert_eq!(w.hour(end), Some(i - 1));
            if i < SNAPSHOT_COUNT {
                assert_eq!(w.hour(end + 1), Some(i));
            }
            let report = view(&w.report(i, true));
            assert_eq!(
                report["window"]["through_offset_ns"],
                w.h.cfg.plan.offset_ns(i)
            );
            assert_eq!(report["window"]["through_seconds"], w.h.cfg.plan.seconds(i));
            assert_eq!(report["identity"]["complete"], true);
            assert_eq!(report["capture_window"]["capture_cutoff_ns"], cutoff);
            assert_eq!(report["measurement_plan"]["duration_seconds"], seconds);
        }
        assert_eq!(w.hour(cutoff + 1), None);
        assert_eq!(
            view(&w.report(SNAPSHOT_COUNT, true))["identity"]["actual_elapsed_seconds"],
            seconds as f64
        );
    }
}

/// Recomputed retention bytes of every accounted collection (the worker's ledger must equal it).
fn accounted(w: &Worker) -> u64 {
    w.shadow.values().map(|e| e.bytes).sum::<u64>()
        + w.shadow_heads
            .keys()
            .map(|k| retained_size(k.as_str()) + 12)
            .sum::<u64>()
        + w.winners
            .iter()
            .flat_map(|(t, v)| {
                v.iter()
                    .map(move |(_, win)| retained_size(win) + t.len() as u64 + 8)
            })
            .sum::<u64>()
        + w.market.iter().map(retained_size).sum::<u64>()
        + w.canonical_market.iter().map(retained_size).sum::<u64>()
        + w.slots.len() as u64 * (16 + RETAINED_OVERHEAD)
        + w.checkpoints.values().map(retained_size).sum::<u64>()
}

/// SK-03: every retained collection of the worker is inside one byte budget. Typed entries get
/// finite sizes; admission beyond the limit is refused and counted (technical INCOMPLETE),
/// streaming to disk continues, and a winner recorrelation releases exactly what it removes.
#[test]
fn retention_is_byte_bounded_and_released_exactly() {
    let mut w = worker();
    let r = rec();
    let (winner, market) = pair(&r, 3, 40);
    for size in [
        retained_size(&r),
        retained_size(&winner),
        retained_size(&market),
    ] {
        assert!(size > RETAINED_OVERHEAD && size < 64 * 1024, "{size}");
    }
    // Room for a few retained Shadow entries only.
    w.retention_byte_limit = 4 * retained_size(&r);
    let accounts: Vec<_> = (0..12).map(|_| Pubkey::new_unique().to_string()).collect();
    let mut r = r;
    for (n, account) in accounts.iter().enumerate() {
        r.account.clone_from(account);
        r.timing.t_rx_ns = Some(10 + n as u64);
        w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
        assert!(w.retained_bytes <= w.retention_byte_limit);
    }
    assert_eq!(
        w.shadow_evaluations_streamed, 12,
        "every evaluation still streamed"
    );
    assert!(w.h.shared.retention_overflow.load(Ordering::Relaxed) > 0);
    assert!(w.shadow.len() < 12 && w.retained_bytes_peak <= w.retention_byte_limit);
    // Recorrelation of a retained target releases its entries' bytes before replaying them.
    let target = w.shadow.values().next().unwrap().rec.account.clone();
    let before = w.retained_bytes;
    w.retention_byte_limit = u64::MAX / 2;
    let (win, _) = pair(&r, 3, 40);
    w.accept(Evidence::Winner {
        target,
        rx_slot: 1,
        winner: win.clone(),
    })
    .unwrap();
    assert_ne!(w.retained_bytes, before);
    assert_eq!(
        w.retained_bytes,
        accounted(&w),
        "ledger equals the recomputed sum"
    );
}

#[cfg(feature = "historical")]
/// A2: under `finalized` (or an unknown commitment) the winner's live receipt is its finalized
/// notification: no rx margin or rx speedup, the slot margin stays; under `confirmed` both. The
/// typed opportunity row renders the same rule offline.
#[test]
fn a2_rx_margins_only_for_a_confirmed_receipt() {
    let r = rec();
    let (w, m) = pair(&r, 4, 50);
    for (commitment, valid) in [
        ("confirmed", true),
        ("finalized", false),
        ("UNKNOWN", false),
    ] {
        assert_eq!(rx_margin_valid(commitment), valid);
        let out = race(&r, Some(&w), Some(&m), Some(30), valid);
        assert!(out.live_timestamp_qualified);
        assert_eq!(
            out.margin_to_winner_rx_ns,
            valid.then_some(10),
            "{commitment}"
        );
        assert_eq!(out.required_speedup_to_winner_rx_ns, valid.then_some(0));
        assert_eq!(out.margin_to_winner_slot_seen_ns, Some(-10));
        let row = OpportunityRow {
            rec: r.clone(),
            winner: Some(w.clone()),
            market: Some(m.clone()),
            slot_seen: Some(30),
            capital: vec![],
            market_commitment: commitment.into(),
        };
        let v = opportunity_view(&row);
        assert_eq!(v["race"]["margin_to_winner_rx_ns"].is_null(), !valid);
        assert_eq!(v["race"]["margin_to_winner_slot_seen_ns"], -10);
        assert_eq!(rx_margin_status(commitment).starts_with("COMPUTED"), valid);
    }
    // Hourly report: a finalized run has no rx distribution or rx sensitivity, the slot ones stay.
    for (commitment, valid) in [("finalized", false), ("confirmed", true)] {
        let mut wk = worker();
        wk.commitment = commitment.into();
        let mut r = rec();
        r.timing.t_sign_ns = Some(50_000_040);
        r.sign_slot = Some(4);
        let (win, m) = pair(&r, 3, 40);
        wk.accept(Evidence::Market(m)).unwrap();
        wk.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
        wk.accept(Evidence::Winner {
            target: r.account.clone(),
            rx_slot: 1,
            winner: win,
        })
        .unwrap();
        wk.accept(Evidence::Slot {
            slot: 3,
            t_rx_ns: 30,
        })
        .unwrap();
        let v = view(&wk.metrics(0, 4));
        let t = &v["race_timing"];
        assert_eq!(t["margin_to_winner_rx_ns"]["count_known"], u64::from(valid));
        assert_eq!(t["margin_to_winner_slot_seen_ns"]["count_known"], 1);
        assert_eq!(
            t["observed_speedup_sensitivity"][4]["winner_rx_boundary"]["count"],
            u64::from(valid)
        );
        // Slot margin 30 - 50_000_040: just over 50 ms, inside the 100 ms boundary.
        assert_eq!(
            t["observed_speedup_sensitivity"][5]["winner_slot_first_seen_boundary"]["count"],
            1
        );
    }
}

/// Stub RPC for B6: every fee quote and simulation takes `delay` (a slow provider).
struct SlowSubmitter {
    delay: Duration,
}
impl SubmissionAdapter for SlowSubmitter {
    fn path(&self) -> SubmissionPath {
        SubmissionPath::Rpc
    }
    fn fee_for_message(
        &self,
        tx: &VersionedTransaction,
        _: std::time::Instant,
    ) -> Result<MessageFeeQuote, darkbot_solana::submit::FeeQuoteError> {
        std::thread::sleep(self.delay);
        Ok(MessageFeeQuote {
            message_hash: tx.message.hash(),
            context_slot: 1,
            lamports: 200,
        })
    }
    fn simulate(&self, _: &VersionedTransaction, _: std::time::Instant) -> SimResult {
        std::thread::sleep(self.delay);
        SimResult::Ok {
            units_consumed: Some(100),
            context_slot: None,
        }
    }
    fn simulate_bundle(&self, _: &[VersionedTransaction], _: std::time::Instant) -> SimResult {
        SimResult::Unavailable("test adapter: no bundle simulation".into())
    }
    fn submit(
        &self,
        _: &VersionedTransaction,
        _: u64,
        _: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        panic!("must never broadcast")
    }
    fn submit_bundle(
        &self,
        _: &[VersionedTransaction],
        _: u64,
        _: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        panic!("must never broadcast bundle")
    }
}

/// B6: the Shadow owner evaluates one candidate synchronously
/// after at most 64 ingress events (`P0_INGRESS_BURST_BEFORE_DETECT`) while candidates are
/// queued. The candidate's block grows with the provider latency (fee quote + simulation of the
/// real pipeline); once rate × block > 64 the ingress queue grows by the difference per queued
/// candidate until it overflows. Scaled offline: 100 ms per RPC, 1,000 events/s, a 1,024-event
/// queue, 24 queued candidates; the real owner queue holds 65,536 events. With the B6 network
/// budget the queue stays bounded and the deferred candidates are recorded.
#[test]
fn b6_owner_rpc_block_grows_the_ingress_queue_until_overflow() {
    use crate::execution::pipeline::{Mode, Pipeline};
    use darkbot_project0::{
        execution::Freshness,
        planner::Allowlist,
        safety::{Allow, Limits, Safety},
    };
    use darkbot_solana::{
        state::blockhash::{BlockhashCache, CachedBlockhash},
        tx::{Resources, TxLimits},
    };
    let adapter = ObserveOnly::new(SlowSubmitter {
        delay: Duration::from_millis(100),
    });
    let clock = Clock::new();
    let bh = BlockhashCache::new();
    bh.set(CachedBlockhash {
        hash: solana_sdk::hash::Hash::new_unique(),
        last_valid_block_height: 100,
        refreshed_ns: clock.now_ns(),
        context_slot: None,
    });
    let zero = Limits {
        max_total_capital: 0,
        max_outstanding_inventory: 0,
        max_concurrent_positions: 0,
        max_exposure_per_attempt: 0,
        max_daily_loss: 0,
        max_failed_attempts_per_day: 0,
        max_tip: 0,
        max_slippage_bps: 0,
        max_outstanding_attempts: 0,
    };
    let mut safety = Safety::new(zero, Allow::default(), false);
    let payer = solana_sdk::signature::Keypair::new();
    let allow = Allowlist::default();
    let failed = FailedCostMatrix::default();
    let slots = darkbot_solana::ingress::SlotWatch::new();
    slots.observe(10);
    let mut network = crate::execution::pipeline::NetworkBudget::unbounded();
    let mut p = Pipeline {
        mode: Mode::Shadow,
        clock: &clock,
        health: &TestHealth,
        builder: &TestBuilder,
        submitter: &adapter,
        safety: &mut safety,
        blockhash: &bh,
        payer: &payer,
        allowlist: &allow,
        failed_costs: &failed,
        limits: TxLimits::default(),
        resources: Resources {
            compute_unit_limit: 100_000,
            priority_micro_lamports_per_cu: 0,
            loaded_accounts_data_size: None,
        },
        min_profit: 0,
        blockhash_max_age_ns: u64::MAX,
        p7_passed: false,
        freshness: Freshness {
            max_opportunity_age_ns: u64::MAX,
            execution_deadline_ns: u64::MAX,
        },
        bundle_tip: None,
        global_allowed: std::slice::from_ref(&test_table().program),
        prereq_allowed: &[],
        slot_watch: &slots,
        network: &mut network,
        simulation_failure: None,
        blockhash_park: crate::execution::pipeline::BlockhashPark::Off,
        fe04: None,
        hold: &crate::execution::hold_b::HoldGate::OBSERVE,
        egress: None,
        synthetic: crate::execution::pipeline::test_synthetic(),
    };
    // One owner run: 24 queued candidates, 64 ingress events drained before each, 1,000
    // events/s into a 1,024-event queue. `guarded`: an empty queue opens a new drain window of
    // the network budget (the owner's rule). Returns (overflow, max depth, losses, block max).
    let run = |p: &mut Pipeline<'_, TestHealth, TestBuilder, ObserveOnly<SlowSubmitter>>,
               guarded: bool| {
        let (tx, rx) = darkbot_common::channel::bounded::<u64>("b6-ingress", 1024);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let producer_stop = stop.clone();
        let producer = std::thread::spawn(move || {
            let mut n = 0u64;
            let started = std::time::Instant::now();
            while !producer_stop.load(Ordering::Acquire) {
                n += 1;
                let _ = tx.try_push(n);
                // 1,000 events/s on average.
                if let Some(wait) = Duration::from_millis(n).checked_sub(started.elapsed()) {
                    std::thread::sleep(wait);
                }
            }
        });
        let (mut block, mut max_depth, mut losses) = (Duration::ZERO, 0u64, Vec::new());
        for _ in 0..24 {
            for _ in 0..64 {
                if rx.try_pop().is_none() {
                    break;
                }
            }
            let s = rx.stats();
            if guarded && s.sent <= s.received {
                p.network.drained();
            }
            let started = std::time::Instant::now();
            let r = p.evaluate(Pubkey::new_unique(), "test", clock.now_ns(), 100, 10, true);
            block = block.max(started.elapsed());
            losses.push(r.loss);
            let s = rx.stats();
            max_depth = max_depth.max(s.sent.saturating_sub(s.received));
        }
        stop.store(true, Ordering::Release);
        producer.join().unwrap();
        (rx.stats().overflow, max_depth, losses, block)
    };
    // Measurement (unbounded, the behaviour before B6): the queue overflows.
    let (overflow, max_depth, losses, block) = run(&mut p, false);
    println!(
        "B6_OWNER_BLOCK rpc_ms=100 candidate_block_ms_max={} candidates=24 rate_per_s=1000 burst=64 queue=1024 max_depth={max_depth} overflow={overflow}",
        block.as_millis()
    );
    // Fee quote and simulation of one candidate: at least two provider round trips.
    assert!(block >= Duration::from_millis(200), "{block:?}");
    assert!(losses.iter().all(Option::is_none));
    // 1,000/s × ≥0.2 s − 64 drained ⇒ ≥136 events gained per queued candidate.
    assert!(overflow > 0, "max_depth {max_depth}");

    // B6 guard: one network candidate per drain window. The queue never overflows; the
    // candidates of a window that is still behind are recorded as RpcBudget, not dropped.
    *p.network = crate::execution::pipeline::NetworkBudget::new(1, u64::MAX);
    let (overflow, max_depth, losses, _) = run(&mut p, true);
    let deferred = losses
        .iter()
        .filter(|l| matches!(l, Some(LossReason::RpcBudget)))
        .count();
    println!(
        "B6_OWNER_GUARD per_drain=1 max_depth={max_depth} overflow={overflow} deferred={deferred}"
    );
    assert_eq!(overflow, 0, "max_depth {max_depth}");
    assert!(deferred > 0 && losses.iter().any(Option::is_none));

    // B6 deadline: a candidate stops calling the provider once its deadline passed (here after
    // the fee quote, before the simulation).
    *p.network = crate::execution::pipeline::NetworkBudget::new(usize::MAX, 50_000_000);
    let started = std::time::Instant::now();
    let r = p.evaluate(Pubkey::new_unique(), "test", clock.now_ns(), 100, 10, true);
    assert!(
        matches!(r.loss, Some(LossReason::RpcDeadline)),
        "{:?}",
        r.loss
    );
    assert_eq!(r.simulation_result, None);
    assert!(started.elapsed() < Duration::from_millis(200));
}

/// V9: when every plan of a candidate fails a technical stage before economics, the loss is that
/// stage's cause — `Build` for the plan gate, allowlists, transaction build and compiled-message
/// check, `Submission` for a path capability the adapter lacks — never `EconomicSkip`. A plan
/// that reached economics keeps the economic attribution (`ThresholdTooHigh`/`EconomicSkip`).
#[test]
fn v9_technical_plan_rejections_keep_their_own_cause() {
    use crate::execution::pipeline::{ReceivershipBuilder, ReceivershipPlan};
    /// The shape of one plan: an unknown instruction (fails the plan gate), a prerequisite
    /// transaction (needs the bundle path the RPC adapter lacks), valid, or valid but losing.
    #[derive(Clone, Copy)]
    enum Shape {
        Broken,
        Bundle,
        Valid,
        Losing,
    }
    struct Shaped(Vec<Shape>);
    impl ReceivershipBuilder for Shaped {
        fn native_cost_usd_micro(&self, n: u64) -> Option<u64> {
            Some(n)
        }
        fn instruction_table(&self) -> &darkbot_project0::planner::InstructionTable {
            test_table()
        }
        fn plans(
            &self,
            a: &Pubkey,
            now: u64,
            slot: u64,
            deadline: std::time::Instant,
        ) -> Result<Vec<ReceivershipPlan>, crate::execution::pipeline::PlanError> {
            let mut out = Vec::new();
            for shape in &self.0 {
                let mut plan = TestBuilder.plans(a, now, slot, deadline)?.remove(0);
                match shape {
                    Shape::Broken => plan.instructions.push(test_ix([9; 8])),
                    // A prerequisite on its own allowlisted program (e.g. a swap).
                    Shape::Bundle => {
                        plan.prerequisite_txs = vec![vec![solana_sdk::instruction::Instruction {
                            program_id: Pubkey::new_from_array([8; 32]),
                            accounts: vec![],
                            data: vec![1],
                        }]]
                    }
                    Shape::Valid => {}
                    // Seized value below the repayment: known, negative net.
                    Shape::Losing => plan.input.gross_seized_realizable_usd_micro = Some(1),
                }
                out.push(plan);
            }
            Ok(out)
        }
    }
    let evaluate = |builder: &Shaped, min_profit: i128| evaluate_once(builder, min_profit);
    use Shape::*;
    let loss = |shapes: &[Shape], min_profit: i128| {
        let r = evaluate(&Shaped(shapes.to_vec()), min_profit);
        assert_eq!(r.loss.is_some(), r.reason.is_some(), "{:?}", r.reason);
        r.loss
    };
    // Every plan fails the plan gate: Build.
    assert_eq!(loss(&[Broken, Broken], 0), Some(LossReason::Build));
    // Every plan needs a bundle the RPC adapter does not offer: Submission.
    assert_eq!(loss(&[Bundle], 0), Some(LossReason::Submission));
    // The furthest stage decides, in either order: capability (3) is past the gate (1).
    assert_eq!(loss(&[Broken, Bundle], 0), Some(LossReason::Submission));
    assert_eq!(loss(&[Bundle, Broken], 0), Some(LossReason::Submission));
    // A plan that reached economics keeps the economic attribution over any technical stage:
    // positive below the threshold, or a known loss.
    assert_eq!(
        loss(&[Broken, Valid, Bundle], i128::MAX),
        Some(LossReason::ThresholdTooHigh)
    );
    assert_eq!(
        loss(&[Bundle, Losing, Broken], 0),
        Some(LossReason::EconomicSkip)
    );
    // Control: a valid plan is selected, no loss.
    assert_eq!(loss(&[Broken, Valid], 0), None);
}

/// One Shadow `Pipeline::evaluate` of `builder` with the counting RPC adapter, zero safety
/// limits and a fresh blockhash (V9/Q5); prerequisites may use program `[8; 32]`.
fn evaluate_once<B: crate::execution::pipeline::ReceivershipBuilder>(
    builder: &B,
    min_profit: i128,
) -> OpportunityRecord {
    let unbounded = darkbot_project0::execution::Freshness {
        max_opportunity_age_ns: u64::MAX,
        execution_deadline_ns: u64::MAX,
    };
    evaluate_shadow(
        builder,
        &counting_submitter(),
        min_profit,
        unbounded,
        Duration::ZERO,
        true,
    )
}

fn counting_submitter() -> CountingSubmitter {
    CountingSubmitter {
        calls: Arc::new(AtomicU64::new(0)),
        sims: Arc::new(AtomicU64::new(0)),
    }
}

/// `evaluate_once` with its own adapter and freshness; `rx_age` passes between T_rx and the
/// evaluation; without `blockhash` the blockhash cache is empty.
fn evaluate_shadow<B: crate::execution::pipeline::ReceivershipBuilder, S: SubmissionAdapter>(
    builder: &B,
    submitter: &S,
    min_profit: i128,
    freshness: darkbot_project0::execution::Freshness,
    rx_age: Duration,
    blockhash: bool,
) -> OpportunityRecord {
    use darkbot_solana::state::blockhash::{BlockhashCache, CachedBlockhash};
    let clock = Clock::new();
    let bh = BlockhashCache::new();
    if blockhash {
        bh.set(CachedBlockhash {
            hash: solana_sdk::hash::Hash::new_unique(),
            last_valid_block_height: 100,
            refreshed_ns: clock.now_ns(),
            context_slot: None,
        });
    }
    let mut network = crate::execution::pipeline::NetworkBudget::unbounded();
    evaluate_parked(
        builder,
        submitter,
        (min_profit, freshness, rx_age),
        (&clock, &bh, u64::MAX),
        crate::execution::pipeline::BlockhashPark::Off,
        &mut network,
    )
    .0
}

/// `evaluate_shadow` on a given clock, blockhash cache and maximum age, with the DEC-01 early
/// refusal `park` and the owner's network budget; returns the record and the park result.
fn evaluate_parked<B: crate::execution::pipeline::ReceivershipBuilder, S: SubmissionAdapter>(
    builder: &B,
    submitter: &S,
    (min_profit, freshness, rx_age): (i128, darkbot_project0::execution::Freshness, Duration),
    (clock, bh, blockhash_max_age_ns): (
        &Clock,
        &darkbot_solana::state::blockhash::BlockhashCache,
        u64,
    ),
    park: crate::execution::pipeline::BlockhashPark,
    network: &mut crate::execution::pipeline::NetworkBudget,
) -> (OpportunityRecord, crate::execution::pipeline::BlockhashPark) {
    use crate::execution::pipeline::{Mode, Pipeline};
    use darkbot_project0::{
        planner::Allowlist,
        safety::{Allow, Limits, Safety},
    };
    use darkbot_solana::tx::{Resources, TxLimits};
    let prereq_program = [Pubkey::new_from_array([8; 32])];
    let zero = Limits {
        max_total_capital: 0,
        max_outstanding_inventory: 0,
        max_concurrent_positions: 0,
        max_exposure_per_attempt: 0,
        max_daily_loss: 0,
        max_failed_attempts_per_day: 0,
        max_tip: 0,
        max_slippage_bps: 0,
        max_outstanding_attempts: 0,
    };
    let mut safety = Safety::new(zero, Allow::default(), false);
    let payer = solana_sdk::signature::Keypair::new();
    let allow = Allowlist::default();
    let failed = FailedCostMatrix::default();
    let slots = darkbot_solana::ingress::SlotWatch::new();
    let mut p = Pipeline {
        mode: Mode::Shadow,
        clock,
        health: &TestHealth,
        builder,
        submitter,
        safety: &mut safety,
        blockhash: bh,
        payer: &payer,
        allowlist: &allow,
        failed_costs: &failed,
        limits: TxLimits::default(),
        resources: Resources {
            compute_unit_limit: 100_000,
            priority_micro_lamports_per_cu: 0,
            loaded_accounts_data_size: None,
        },
        min_profit,
        blockhash_max_age_ns,
        p7_passed: false,
        freshness,
        bundle_tip: None,
        global_allowed: std::slice::from_ref(&test_table().program),
        prereq_allowed: &prereq_program,
        slot_watch: &slots,
        network,
        simulation_failure: None,
        blockhash_park: park,
        fe04: None,
        hold: &crate::execution::hold_b::HoldGate::OBSERVE,
        egress: None,
        synthetic: crate::execution::pipeline::test_synthetic(),
    };
    let t_rx = clock.now_ns();
    std::thread::sleep(rx_age);
    let rec = p.evaluate(Pubkey::new_unique(), "test", t_rx, 100, 10, true);
    (rec, p.blockhash_park)
}

/// V2-04: the counting adapter whose fee quote takes `delay`.
struct SlowFeeSubmitter {
    inner: CountingSubmitter,
    delay: Duration,
}
impl SubmissionAdapter for SlowFeeSubmitter {
    fn path(&self) -> SubmissionPath {
        self.inner.path()
    }
    fn fee_for_message(
        &self,
        tx: &VersionedTransaction,
        deadline: std::time::Instant,
    ) -> Result<MessageFeeQuote, darkbot_solana::submit::FeeQuoteError> {
        std::thread::sleep(self.delay);
        self.inner.fee_for_message(tx, deadline)
    }
    fn simulate(&self, tx: &VersionedTransaction, deadline: std::time::Instant) -> SimResult {
        self.inner.simulate(tx, deadline)
    }
    fn simulate_unverified(
        &self,
        tx: &VersionedTransaction,
        deadline: std::time::Instant,
    ) -> SimResult {
        self.inner.simulate_unverified(tx, deadline)
    }
    fn simulate_bundle(&self, txs: &[VersionedTransaction], d: std::time::Instant) -> SimResult {
        self.inner.simulate_bundle(txs, d)
    }
    fn submit(
        &self,
        tx: &VersionedTransaction,
        n: u64,
        deadline: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        self.inner.submit(tx, n, deadline)
    }
    fn submit_bundle(
        &self,
        txs: &[VersionedTransaction],
        n: u64,
        deadline: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        self.inner.submit_bundle(txs, n, deadline)
    }
}

/// V2-04 (P1-06): T_detect = valid and economic, marked after the fee quote and the economic
/// gate. A fee quote of 30 ms lies between T_rx and T_detect, never between T_detect and T_ready
/// (before: marked ahead of the build, T_detect→T_ready ≥ 30 ms). A known plan below the
/// threshold is no detection: no T_detect, no detect slot.
#[test]
fn v2_04_t_detect_is_marked_after_the_fee_quote_and_the_economics() {
    let delay = Duration::from_millis(30);
    let unbounded = darkbot_project0::execution::Freshness {
        max_opportunity_age_ns: u64::MAX,
        execution_deadline_ns: u64::MAX,
    };
    let slow = SlowFeeSubmitter {
        inner: counting_submitter(),
        delay,
    };
    let r = evaluate_shadow(&TestBuilder, &slow, 0, unbounded, Duration::ZERO, true);
    assert_eq!(r.decision, Decision::ShadowReady, "{:?}", r.reason);
    let t = &r.timing;
    let (rx, detect, ready) = (
        t.t_rx_ns.unwrap(),
        t.t_detect_ns.unwrap(),
        t.t_ready_ns.unwrap(),
    );
    let delay_ns = delay.as_nanos() as u64;
    assert!(detect - rx >= delay_ns, "rx→detect {}", detect - rx);
    assert!(ready - detect < delay_ns, "detect→ready {}", ready - detect);
    let below = evaluate_once(&TestBuilder, i128::MAX);
    assert_eq!(below.loss, Some(LossReason::ThresholdTooHigh));
    assert_eq!((below.timing.t_detect_ns, below.detect_slot), (None, None));
}

/// V2-09 (P1-01): a blockhash already stale before planning is counted
/// (`blockhash_stale_before_planning`); the late check after planning stays the verdict — the
/// planning still runs once and the loss is `Build` "blockhash stale" (loss priority unchanged
/// until decided).
#[test]
fn v2_09_stale_blockhash_before_planning_is_counted_and_the_late_check_decides() {
    use crate::execution::pipeline::ReceivershipBuilder;
    use darkbot_common::mt0::{Counter, MT0};
    struct Counted(std::cell::Cell<u32>);
    impl ReceivershipBuilder for Counted {
        fn native_cost_usd_micro(&self, n: u64) -> Option<u64> {
            TestBuilder.native_cost_usd_micro(n)
        }
        fn instruction_table(&self) -> &darkbot_project0::planner::InstructionTable {
            test_table()
        }
        fn plans(
            &self,
            a: &Pubkey,
            n: u64,
            s: u64,
            d: std::time::Instant,
        ) -> Result<
            Vec<crate::execution::pipeline::ReceivershipPlan>,
            crate::execution::pipeline::PlanError,
        > {
            self.0.set(self.0.get() + 1);
            TestBuilder.plans(a, n, s, d)
        }
    }
    let unbounded = darkbot_project0::execution::Freshness {
        max_opportunity_age_ns: u64::MAX,
        execution_deadline_ns: u64::MAX,
    };
    let builder = Counted(std::cell::Cell::new(0));
    let before = MT0.get(Counter::BlockhashStaleBeforePlanning);
    let r = evaluate_shadow(
        &builder,
        &counting_submitter(),
        0,
        unbounded,
        Duration::ZERO,
        false,
    );
    assert_eq!(
        (r.loss, r.reason_text().as_deref()),
        (Some(LossReason::Build), Some("blockhash stale"))
    );
    assert_eq!(builder.0.get(), 1, "planning unchanged");
    assert!(MT0.get(Counter::BlockhashStaleBeforePlanning) > before);
}

/// A builder counting its planning calls; `stale_during` replaces the cached blockhash with an
/// expired one while planning (a refresh racing the evaluation the other way round).
struct CountedPlans<'c> {
    calls: std::cell::Cell<u32>,
    stale_during: Option<&'c darkbot_solana::state::blockhash::BlockhashCache>,
}
impl crate::execution::pipeline::ReceivershipBuilder for CountedPlans<'_> {
    fn native_cost_usd_micro(&self, n: u64) -> Option<u64> {
        TestBuilder.native_cost_usd_micro(n)
    }
    fn instruction_table(&self) -> &darkbot_project0::planner::InstructionTable {
        test_table()
    }
    fn plans(
        &self,
        a: &Pubkey,
        n: u64,
        s: u64,
        d: std::time::Instant,
    ) -> Result<
        Vec<crate::execution::pipeline::ReceivershipPlan>,
        crate::execution::pipeline::PlanError,
    > {
        self.calls.set(self.calls.get() + 1);
        if let Some(bh) = self.stale_during {
            bh.set(darkbot_solana::state::blockhash::CachedBlockhash {
                hash: solana_sdk::hash::Hash::new_unique(),
                last_valid_block_height: 100,
                refreshed_ns: 0,
                context_slot: None,
            });
        }
        TestBuilder.plans(a, n, s, d)
    }
}

/// DEC-01 (V2-09): for a caller that parks (`Armed`), a stale or missing blockhash is refused
/// before planning — no builder call, no network budget, loss `Build` with the cause
/// BLOCKHASH_STALE, the routing/plan progress not run stays unset — and the refusal carries the
/// cache generation read before the check (before: planned once, budget spent, decided late).
/// `Off` keeps the V2-09 behaviour; a stale opportunity still falls out as StateRace first.
#[test]
fn dec01_a_stale_blockhash_is_refused_before_planning_without_budget() {
    use crate::execution::pipeline::{BlockhashPark, NetworkBudget, BLOCKHASH_STALE_EARLY};
    use darkbot_solana::state::blockhash::{BlockhashCache, CachedBlockhash};
    let unbounded = darkbot_project0::execution::Freshness {
        max_opportunity_age_ns: u64::MAX,
        execution_deadline_ns: u64::MAX,
    };
    // The clock runs from its creation: an entry refreshed at 0 is older than 1 ms after 2 ms.
    let clock = Clock::new();
    std::thread::sleep(Duration::from_millis(2));
    for stale in [false, true] {
        let bh = BlockhashCache::new();
        if stale {
            bh.set(CachedBlockhash {
                hash: solana_sdk::hash::Hash::new_unique(),
                last_valid_block_height: 100,
                refreshed_ns: 0,
                context_slot: None,
            });
        }
        let generation = bh.generation();
        let builder = CountedPlans {
            calls: std::cell::Cell::new(0),
            stale_during: None,
        };
        let mut network = NetworkBudget::new(3, u64::MAX);
        let (r, park) = evaluate_parked(
            &builder,
            &counting_submitter(),
            (0, unbounded, Duration::ZERO),
            (&clock, &bh, 1_000_000),
            BlockhashPark::Armed,
            &mut network,
        );
        assert_eq!(
            (r.loss, r.reason_text().as_deref()),
            (Some(LossReason::Build), Some(BLOCKHASH_STALE_EARLY))
        );
        assert_eq!(park, BlockhashPark::Refused { generation });
        assert_eq!(builder.calls.get(), 0, "no planning call");
        assert_eq!(network.remaining, 3, "no network budget");
        assert_eq!(
            (r.progress.candidates_created, r.progress.plan_validated),
            (None, None),
            "checks not run are neither passed nor failed"
        );
        // Off: the late check decides after one planning call and one budget unit.
        let builder = CountedPlans {
            calls: std::cell::Cell::new(0),
            stale_during: None,
        };
        let (r, park) = evaluate_parked(
            &builder,
            &counting_submitter(),
            (0, unbounded, Duration::ZERO),
            (&clock, &bh, 1_000_000),
            BlockhashPark::Off,
            &mut network,
        );
        assert_eq!(
            (r.loss, r.reason_text().as_deref(), park),
            (
                Some(LossReason::Build),
                Some("blockhash stale"),
                BlockhashPark::Off
            )
        );
        assert_eq!((builder.calls.get(), network.remaining), (1, 2));
    }
    // An expired opportunity falls out regularly before the blockhash is looked at.
    let expired = darkbot_project0::execution::Freshness {
        max_opportunity_age_ns: 1,
        execution_deadline_ns: u64::MAX,
    };
    let bh = BlockhashCache::new();
    let mut network = NetworkBudget::new(3, u64::MAX);
    let (r, park) = evaluate_parked(
        &TestBuilder,
        &counting_submitter(),
        (0, expired, Duration::from_millis(2)),
        (&clock, &bh, 1_000_000),
        BlockhashPark::Armed,
        &mut network,
    );
    assert_eq!(
        (r.loss, park),
        (Some(LossReason::StateRace), BlockhashPark::Armed)
    );
}

/// DEC-01: the late check before the build stays — a blockhash fresh at the early check
/// (`Passed`) that expires during planning is refused there (`Build` "blockhash stale"), never
/// parked.
#[test]
fn dec01_the_late_check_before_the_build_stays() {
    use crate::execution::pipeline::{BlockhashPark, NetworkBudget};
    use darkbot_solana::state::blockhash::{BlockhashCache, CachedBlockhash};
    let unbounded = darkbot_project0::execution::Freshness {
        max_opportunity_age_ns: u64::MAX,
        execution_deadline_ns: u64::MAX,
    };
    // The clock runs from its creation: an entry refreshed at 0 is older than 1 ms after 2 ms.
    let clock = Clock::new();
    std::thread::sleep(Duration::from_millis(2));
    let bh = BlockhashCache::new();
    // Fresh at the early check (stored ahead of the clock), expired by the builder.
    bh.set(CachedBlockhash {
        hash: solana_sdk::hash::Hash::new_unique(),
        last_valid_block_height: 100,
        refreshed_ns: clock.now_ns() + 60_000_000_000,
        context_slot: None,
    });
    let builder = CountedPlans {
        calls: std::cell::Cell::new(0),
        stale_during: Some(&bh),
    };
    let mut network = NetworkBudget::unbounded();
    let (r, park) = evaluate_parked(
        &builder,
        &counting_submitter(),
        (0, unbounded, Duration::ZERO),
        (&clock, &bh, 1_000_000),
        BlockhashPark::Armed,
        &mut network,
    );
    assert_eq!(
        (r.loss, r.reason_text().as_deref(), park),
        (
            Some(LossReason::Build),
            Some("blockhash stale"),
            BlockhashPark::Passed
        )
    );
    assert_eq!(builder.calls.get(), 1);
}

/// V2-04: the execution deadline counts from T_rx, not from the later T_detect mark: 300 ms
/// between T_rx and the evaluation exceed a 200 ms deadline (before: counted from T_detect,
/// inside it); within the deadline the same candidate is ready.
#[test]
fn v2_04_execution_deadline_counts_from_t_rx() {
    let deadline = darkbot_project0::execution::Freshness {
        max_opportunity_age_ns: u64::MAX,
        execution_deadline_ns: 200_000_000,
    };
    let late = evaluate_shadow(
        &TestBuilder,
        &counting_submitter(),
        0,
        deadline,
        Duration::from_millis(300),
        true,
    );
    assert_eq!(
        (late.loss, late.reason_text().as_deref()),
        (Some(LossReason::Build), Some("execution deadline passed"))
    );
    let fresh = evaluate_shadow(
        &TestBuilder,
        &counting_submitter(),
        0,
        deadline,
        Duration::ZERO,
        true,
    );
    assert_eq!(fresh.decision, Decision::ShadowReady, "{:?}", fresh.reason);
}

/// Q5 through `Pipeline::evaluate`: a planning failure is recorded with the decision and loss of
/// its typed kind and its unchanged text as reason — a routing failure whose remote message says
/// "unavailable" stays a judged routing skip (by text it was UNAVAILABLE).
#[test]
fn q5_evaluate_records_the_typed_plan_error() {
    use crate::execution::pipeline::{PlanError, PlanErrorKind, ReceivershipBuilder};
    struct Failing(PlanError);
    impl ReceivershipBuilder for Failing {
        fn instruction_table(&self) -> &darkbot_project0::planner::InstructionTable {
            test_table()
        }
        fn plans(
            &self,
            _: &Pubkey,
            _: u64,
            _: u64,
            _: std::time::Instant,
        ) -> Result<Vec<crate::execution::pipeline::ReceivershipPlan>, PlanError> {
            Err(self.0.clone())
        }
    }
    for (kind, text, decision, loss) in [
        (
            PlanErrorKind::Routing,
            "ROUTING:exact-in a→b: provider temporarily unavailable",
            Decision::Skip,
            LossReason::Routing,
        ),
        (
            PlanErrorKind::FeeStateUnavailable,
            "FEE_STATE_UNAVAILABLE:FeeState account missing",
            Decision::Unavailable,
            LossReason::Unknown,
        ),
        (
            PlanErrorKind::IntegrationStale,
            "INTEGRATION_STALE:equity price: AnchorError code 6504",
            Decision::Skip,
            LossReason::IntegrationRefresh,
        ),
        (
            PlanErrorKind::UnsupportedIntegration,
            "SIZING:UNSUPPORTED_INTEGRATION_ASSET",
            Decision::Skip,
            LossReason::UnsupportedIntegration,
        ),
        // V17: own persisted causes (before: ROUTING).
        (
            PlanErrorKind::Other,
            "mint not in state",
            Decision::Skip,
            LossReason::Plan,
        ),
        (
            PlanErrorKind::HealthUnavailable,
            "HEALTH_UNAVAILABLE:no health engine",
            Decision::Unavailable,
            LossReason::Health,
        ),
    ] {
        let r = evaluate_once(&Failing(PlanError::new(kind, text)), 0);
        assert_eq!(r.decision, decision, "{text}");
        assert_eq!(r.loss, Some(loss), "{text}");
        assert_eq!(r.reason_text().as_deref(), Some(text));
    }
}

#[cfg(feature = "historical")]
/// R12: the retention byte budget is the configured `econ_retention_mib` and, when tight, refuses
/// further evidence fail-closed: with room for exactly one more market event the next distinct
/// one is refused and counted, nothing retained is evicted, the accounted bytes stay within the
/// limit, and the technical verdict of an otherwise complete run turns INCOMPLETE.
#[test]
fn r12_tight_byte_budget_refuses_evidence_and_fails_the_run() {
    let mut w = worker();
    assert_eq!(
        w.retention_byte_limit,
        darkbot_solana::runtime_config::active()
            .limits
            .econ_retention_mib
            * crate::storage::budget::MIB
    );
    complete(&mut w);
    w.accept(Evidence::Shadow(Box::new(rec()))).unwrap();
    assert_eq!(view(&w.technical(true))["verdict"], "PASS");
    let (_, first) = pair(&rec(), 3, 40);
    let mut second = first.clone();
    second.signature = "second".into();
    let retained = w.market.len();
    // Room for exactly one more market event.
    w.retention_byte_limit = w.retained_bytes + retained_size(&first);
    w.accept(Evidence::Market(first)).unwrap();
    assert_eq!(w.market.len(), retained + 1);
    assert_eq!(w.h.shared.retention_overflow.load(Ordering::Relaxed), 0);
    assert_eq!(w.retained_bytes, w.retention_byte_limit);
    w.accept(Evidence::Market(second)).unwrap();
    assert_eq!(w.market.len(), retained + 1, "refused, nothing evicted");
    assert_eq!(w.h.shared.retention_overflow.load(Ordering::Relaxed), 1);
    assert!(w.retained_bytes <= w.retention_byte_limit);
    assert_eq!(w.retained_bytes, accounted(&w));
    assert_eq!(view(&w.technical(true))["verdict"], "INCOMPLETE");
}

/// R12: `P0_ECON_RETENTION` accepts every count up to the hard bound 2,000,000 and refuses 0 and
/// anything beyond; the default stays 262,144. The byte budget (`econ_retention_mib`) bounds the
/// retained bytes independently and may bind first at a large count — fail-closed either way.
#[test]
fn r12_econ_retention_bound_is_exact() {
    let with = |v: &'static str| {
        EconConfig::from_lookup(&|k| (k == "P0_ECON_RETENTION").then(|| v.to_string()), 0)
    };
    assert_eq!(
        EconConfig::from_lookup(&|_| None, 0).unwrap().retention,
        crate::analytics::econ::DEFAULT_RETENTION
    );
    assert_eq!(with("1").unwrap().retention, 1);
    assert_eq!(with("2000000").unwrap().retention, 2_000_000);
    assert!(with("2000001").is_err());
    assert!(with("0").is_err());
}

/// C0E1-P4-VC fixture: the final report of a run whose every technical gate passes (`complete`
/// plus one Shadow evaluation in the window), bound to `run_id` and `plan`.
pub(crate) fn passing_final_report(run_id: &str, plan: &MeasurementPlan) -> SnapshotReport {
    let mut w = worker();
    complete(&mut w);
    w.accept(Evidence::Shadow(Box::new(rec()))).unwrap();
    w.final_technical = Some(w.technical(true));
    assert_eq!(w.final_technical.as_ref().unwrap().verdict, "PASS");
    let mut r = w.report(SNAPSHOT_COUNT, true);
    r.identity.run_id = run_id.into();
    r.identity.measurement_plan = plan.clone();
    let t = &mut r.technical_qualification;
    t.requested_duration_seconds = plan.duration_seconds;
    t.actual_duration_seconds = Some(plan.duration_seconds as f64);
    let _ = std::fs::remove_dir_all(&w.h.cfg.run_dir);
    r
}

/// C0E1-P4-VC / V21: the master qualification passes only with every axis — a final canonical
/// result, complete capture clock and live coverage, a canonical economics replay, every gate of
/// the run's technical record, no LIVE or submission and a clean owners' end — and names each
/// failed gate otherwise. The run-end technical verdict is INCOMPLETE for its capture-time
/// market/canonical terms only; the verification replaces them, so the master can still pass.
#[test]
fn c0e1_master_qualification_requires_every_axis() {
    use crate::analytics::qualification::{assess, Evidence as Q, OwnersEnd, MASTER_PASS};
    use crate::analytics::report::{DatasetIdentity, DatasetRecord, Economics};
    let plan = MeasurementPlan::new(60).unwrap();
    let mut report = passing_final_report("run_q", &plan);
    // As at every run end since L.2: no canonical evidence yet, so the run-end verdict fails.
    report.technical_qualification.market_capture_complete = false;
    report.technical_qualification.final_reconciliation_complete = false;
    report.technical_qualification.verdict = "INCOMPLETE".into();
    let dataset = DatasetRecord {
        identity: DatasetIdentity {
            renderer: "test".into(),
            capture_run_id: "run_q".into(),
            capture_evidence_digest: "evidence".into(),
            verification_spec_id: Some("spec".into()),
            verification_result_id: Some("result".into()),
            report_binary_sha256: [0; 32],
        },
        record_count: 1,
        records_sha256: [1; 32],
        verification_status: "CANONICAL_VERIFIED".into(),
        report_status: "FINAL_VERIFICATION_RESULT".into(),
        live_coverage_status: "LIVE_COVERAGE_COMPLETE".into(),
        capture_clock_complete: true,
        economics_replay: "PASS events=1 shadow_available=true".into(),
        c0_e1_master_pass: false,
    };
    let replayed = Economics::Replayed {
        events: 1,
        shadow_available: true,
    };
    let clean = OwnersEnd {
        aborted: false,
        shadow_failed: false,
        market_failed: false,
    };
    // A capture that ran its clock with complete transport coverage and proven deployment
    // continuity; the full-slot reconciliation is the verification's (not the capture's).
    let capture = crate::market_observer::rows::MarketCompletion {
        run_id: "run_q".into(),
        verdict: "MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED".into(),
        stream_ready: true,
        full_duration_observed: true,
        drain_complete: true,
        coverage_complete: true,
        decode_complete: true,
        receivership_transport_capture_complete: true,
        full_slot_reconciliation_complete: false,
        liquidation_path_coverage_complete: true,
        market_capture_complete: false,
        source_coverage_verified: false,
        incomplete_reasons: vec!["VERIFICATION_NOT_RUN".into()],
    };
    let assessed = |d: &DatasetRecord,
                    e: &Economics,
                    r: Option<&SnapshotReport>,
                    o: Option<OwnersEnd>,
                    p: &MeasurementPlan,
                    c: &crate::market_observer::rows::MarketCompletion,
                    journal: bool| {
        let a = assess(&Q {
            dataset: d,
            economics: e,
            plan: p,
            capture: c,
            capture_journal_complete: journal,
            deployment_proven_after: false,
            final_report: r,
            owners: o,
        });
        (a.verdict(), a.failed)
    };
    let run = |d: &DatasetRecord,
               e: &Economics,
               r: Option<&SnapshotReport>,
               o: Option<OwnersEnd>,
               p: &MeasurementPlan| assessed(d, e, r, o, p, &capture, true);
    assert_eq!(
        run(&dataset, &replayed, Some(&report), Some(clean), &plan),
        (MASTER_PASS, vec![])
    );
    let not = |failed: &[&'static str]| ("C0_E1_NOT_QUALIFIED", failed.to_vec());
    // Capture, coverage and verification axes.
    let with = |f: &dyn Fn(&mut DatasetRecord)| {
        let mut d = dataset.clone();
        f(&mut d);
        run(&d, &replayed, Some(&report), Some(clean), &plan)
    };
    assert_eq!(
        with(&|d| d.capture_clock_complete = false),
        not(&["CAPTURE_CLOCK_INCOMPLETE"])
    );
    assert_eq!(
        with(&|d| d.live_coverage_status = "LIVE_COVERAGE_INCOMPLETE".into()),
        not(&["LIVE_COVERAGE_INCOMPLETE"])
    );
    for status in [
        "NOT_RUN",
        "INDEX_VERIFIED",
        "CROSSCHECK_VERIFIED",
        "HISTORICAL_INCOMPLETE",
        "CANONICAL_INCOMPLETE",
    ] {
        // Also when the report status claims a final result (never trusted alone).
        for report_status in ["PROVISIONAL/INCOMPLETE", "FINAL_VERIFICATION_RESULT"] {
            assert_eq!(
                with(&|d| {
                    d.verification_status = status.into();
                    d.report_status = report_status.into();
                }),
                not(&["VERIFICATION_NOT_CANONICAL"]),
                "{status} {report_status}"
            );
        }
    }
    assert_eq!(
        with(&|d| d.report_status = "PROVISIONAL/INCOMPLETE".into()),
        not(&["VERIFICATION_NOT_CANONICAL"])
    );
    // The capture journal's own seal and the audited deployment continuity.
    assert_eq!(
        assessed(
            &dataset,
            &replayed,
            Some(&report),
            Some(clean),
            &plan,
            &capture,
            false
        ),
        not(&["CAPTURE_JOURNAL_INCOMPLETE"])
    );
    let unproven = crate::market_observer::rows::MarketCompletion {
        liquidation_path_coverage_complete: false,
        ..capture.clone()
    };
    assert_eq!(
        assessed(
            &dataset,
            &replayed,
            Some(&report),
            Some(clean),
            &plan,
            &unproven,
            true
        ),
        not(&["DEPLOYMENT_CONTINUITY_UNPROVEN"])
    );
    // A verified post-capture deployment proof of the selected result proves it.
    let proven_after = assess(&Q {
        dataset: &dataset,
        economics: &replayed,
        plan: &plan,
        capture: &unproven,
        capture_journal_complete: true,
        deployment_proven_after: true,
        final_report: Some(&report),
        owners: Some(clean),
    });
    assert_eq!(
        (proven_after.verdict(), proven_after.failed),
        (MASTER_PASS, vec![])
    );
    // Economics axis.
    assert_eq!(
        run(
            &dataset,
            &Economics::Incomplete("JOIN".into()),
            Some(&report),
            Some(clean),
            &plan
        ),
        not(&["ECONOMICS_REPLAY_INCOMPLETE"])
    );
    assert_eq!(
        run(
            &dataset,
            &Economics::NotRun,
            Some(&report),
            Some(clean),
            &plan
        ),
        not(&["ECONOMICS_NOT_REPLAYED"])
    );
    // The final technical record: missing, of another run or plan, and each of its gates.
    assert_eq!(
        run(&dataset, &replayed, None, Some(clean), &plan),
        not(&["ECON_FINAL_MISSING"])
    );
    let tech = |f: &dyn Fn(&mut SnapshotReport)| {
        let mut r = report.clone();
        f(&mut r);
        run(&dataset, &replayed, Some(&r), Some(clean), &plan)
    };
    assert_eq!(
        tech(&|r| r.identity.run_id = "other".into()),
        not(&["ECON_FINAL_OTHER_RUN"])
    );
    assert_eq!(
        run(
            &dataset,
            &replayed,
            Some(&report),
            Some(clean),
            &MeasurementPlan::new(120).unwrap()
        ),
        not(&["ECON_FINAL_OTHER_PLAN"])
    );
    type Gate<'a> = (&'a dyn Fn(&mut Technical), &'static str);
    let gates: [Gate; 13] = [
        (
            &|t| t.startup_excluded_from_measurement = false,
            "TECHNICAL_STARTUP_IN_WINDOW",
        ),
        (
            &|t| t.shadow_evaluations_in_window = 0,
            "TECHNICAL_NO_EVALUATIONS",
        ),
        (&|t| t.shadow_telemetry = None, "TECHNICAL_SHADOW_STATE"),
        (
            &|t| t.actual_duration_seconds = Some(59.0),
            "TECHNICAL_DURATION",
        ),
        (&|t| t.four_snapshots_present = false, "TECHNICAL_SNAPSHOTS"),
        (&|t| t.snapshot_write_failures = 1, "TECHNICAL_WRITER"),
        (
            &|t| t.shadow_evaluations_streamed = t.shadow_evaluations_received + 1,
            "TECHNICAL_EVALUATIONS_NOT_STREAMED",
        ),
        (
            &|t| t.shadow_semantic_evictions = 1,
            "TECHNICAL_SEMANTIC_EVICTIONS",
        ),
        (&|t| t.analytics_queue_drops = 1, "TECHNICAL_QUEUE_DROPS"),
        (
            &|t| t.retention_overflow = 1,
            "TECHNICAL_RETENTION_OVERFLOW",
        ),
        (
            &|t| t.shadow_runtime_terminated_early = true,
            "TECHNICAL_SHADOW_FAILED",
        ),
        (
            &|t| t.shadow_runtime_finished = false,
            "TECHNICAL_SHADOW_NOT_FINISHED",
        ),
        (
            &|t| t.program_upgrade_halt = true,
            "TECHNICAL_PROGRAM_UPGRADE",
        ),
    ];
    for (f, code) in gates {
        assert_eq!(
            tech(&|r| f(&mut r.technical_qualification)),
            not(&[code]),
            "{code}"
        );
    }
    let mut overflow = report.clone();
    overflow
        .technical_qualification
        .shadow_telemetry
        .as_mut()
        .unwrap()
        .ingress_overflow = 1;
    assert_eq!(
        run(&dataset, &replayed, Some(&overflow), Some(clean), &plan),
        not(&["TECHNICAL_SHADOW_STATE"])
    );
    // Trading: no LIVE, no submission.
    for f in [
        &(|r: &mut SnapshotReport| r.technical_qualification.submissions = 1)
            as &dyn Fn(&mut SnapshotReport),
        &|r| r.technical_qualification.submission_attempts_blocked = 1,
        &|r| r.technical_qualification.live_ready = true,
        &|r| r.identity.live_enabled = true,
    ] {
        assert_eq!(tech(f), not(&["TRADING_LIVE_OR_SUBMISSION"]));
    }
    // The owners' end.
    assert_eq!(
        run(&dataset, &replayed, Some(&report), None, &plan),
        not(&["OWNERS_END_UNKNOWN"])
    );
    for (o, code) in [
        (
            OwnersEnd {
                aborted: true,
                ..clean
            },
            "OWNERS_ABORTED",
        ),
        (
            OwnersEnd {
                shadow_failed: true,
                ..clean
            },
            "OWNERS_SHADOW_FAILED",
        ),
        (
            OwnersEnd {
                market_failed: true,
                ..clean
            },
            "OWNERS_MARKET_FAILED",
        ),
    ] {
        assert_eq!(
            run(&dataset, &replayed, Some(&report), Some(o), &plan),
            not(&[code])
        );
    }
}

/// Review 5: the gap smoke (`scripts/smoke.sh`) reads three facts of the typed market summary
/// from `p0-journal frames` (`payload::describe`); their text form is pinned here.
#[test]
fn market_summary_frame_text_names_the_gap_facts() {
    use crate::analytics::payload::{describe, encode, Payload};
    use crate::market_observer::MarketFinal;
    let mut s = market_summary(60.0);
    s.gap_count = 2;
    s.gap_unrecovered = 0;
    s.full_slot_reconciliation.slots_unresolved = 0;
    let f = MarketFinal::checkpoint(s);
    let text = describe(MarketFinal::KIND, MarketFinal::SCHEMA, &encode(&f).unwrap()).unwrap();
    for fact in [
        "gap_count: 2,",
        "gap_unrecovered: 0,",
        "slots_unresolved: 0,",
    ] {
        assert!(text.contains(fact), "{fact} in {text}");
    }
}

/// Review E4: the decision latency is T_rx→T_ready (economics and the built plan), not
/// T_rx→T_detect; a record without both stamps adds no sample.
#[test]
fn decision_latency_spans_rx_to_ready() {
    let mut l = crate::analytics::shadow_ledger::ShadowLedger::new();
    assert_eq!(l.rx_ready_p95_ns(), None);
    let mut r = rec();
    r.timing.t_rx_ns = Some(1_000);
    r.timing.t_detect_ns = Some(1_200);
    r.timing.t_ready_ns = Some(5_000);
    r.intervals_ns = vec![("T_rx→T_detect".into(), Some(200))];
    l.on_record(&r, None);
    let p95 = l.rx_ready_p95_ns().unwrap();
    assert!((4_000..5_000).contains(&p95), "{p95}");
    r.timing.t_ready_ns = None;
    l.on_record(&r, None);
    assert_eq!(l.rx_ready_p95_ns(), Some(p95));
}

/// DEC-12: opportunity rows written before DEC-12 (schema 3, the record layout of the earlier
/// writer) stay readable with the record's text.
#[test]
fn dec12_opportunity_rows_of_the_earlier_layout_stay_readable() {
    use crate::analytics::payload::{self, Payload};
    let bytes = crate::analytics::record::earlier_writer_record_bytes();
    let wire = OpportunityRowV3 {
        rec: payload::decode_wire(&bytes).unwrap(),
        winner: None,
        market: None,
        slot_seen: Some(3),
        capital: vec![7],
        market_commitment: "confirmed".into(),
    };
    let row: OpportunityRow = payload::decode(
        OpportunityRow::KIND,
        3,
        &payload::encode_wire(&wire).unwrap(),
    )
    .unwrap();
    assert_eq!(row.rec.reason_text().as_deref(), Some("mint not in state"));
    assert_eq!(
        (
            row.slot_seen,
            row.capital.as_slice(),
            row.market_commitment.as_str()
        ),
        (Some(3), &[7][..], "confirmed")
    );
}

/// DEC-12 (V2-14/P1-05): the pipeline records its rejection typed at the source (the variant
/// with its typed values, never a pre-made text); its text is made where it is written.
#[test]
fn dec12_the_pipeline_records_typed_rejections() {
    use crate::analytics::rejection::Rejection;
    use crate::execution::pipeline::{BlockhashPark, NetworkBudget};
    use darkbot_solana::state::blockhash::BlockhashCache;
    let unbounded = darkbot_project0::execution::Freshness {
        max_opportunity_age_ns: u64::MAX,
        execution_deadline_ns: u64::MAX,
    };
    let clock = Clock::new();
    let (r, _) = evaluate_parked(
        &TestBuilder,
        &counting_submitter(),
        (0, unbounded, Duration::ZERO),
        (&clock, &BlockhashCache::new(), 1_000_000_000),
        BlockhashPark::Armed,
        &mut NetworkBudget::unbounded(),
    );
    assert!(
        matches!(r.reason, Some(Rejection::BlockhashStaleEarly)),
        "{:?}",
        r.reason
    );
    let late = evaluate_shadow(
        &TestBuilder,
        &counting_submitter(),
        0,
        unbounded,
        Duration::ZERO,
        false,
    );
    assert!(
        matches!(late.reason, Some(Rejection::BlockhashStaleLate)),
        "{:?}",
        late.reason
    );
    let below = evaluate_once(&TestBuilder, i128::MAX);
    assert!(
        matches!(&below.reason, Some(Rejection::PlansRejected(plans)) if !plans.is_empty()),
        "{:?}",
        below.reason
    );
}

/// DEC-12 (V2-14/P6-04): the FE-04 handoff is bounded by the candidate's monotonic network
/// deadline (`NetworkBudget::deadline_ns`, configured): an exact-payload simulation that ends
/// past it reserves and sends nothing and releases the Safety reservation (before: handed off
/// after the deadline); within it the candidate is handed off once.
#[test]
fn dec12_the_fe04_handoff_is_bounded_by_the_candidate_deadline() {
    use crate::analytics::rejection::RejectionCode;
    use darkbot_project0::safety::{Allow, Limits, Safety};
    let target = Pubkey::new_unique();
    let payer = solana_sdk::signature::Keypair::new();
    let open_safety = || {
        let mut allow = Allow::default();
        allow.execution_classes.insert("longtail");
        Safety::new(
            Limits {
                max_total_capital: u64::MAX,
                max_outstanding_inventory: u64::MAX,
                max_concurrent_positions: u32::MAX,
                max_exposure_per_attempt: u64::MAX,
                max_daily_loss: u64::MAX,
                max_failed_attempts_per_day: u32::MAX,
                max_tip: u64::MAX,
                max_slippage_bps: u64::MAX,
                max_outstanding_attempts: u32::MAX,
            },
            allow,
            true,
        )
    };
    // A node behind the state slot: the handoff simulates again (DEC-02), past the deadline.
    let slow = HandoffSubmitter {
        submits: Arc::new(AtomicU64::new(0)),
        sims: Arc::new(AtomicU64::new(0)),
        transport_fails: false,
        // The pipeline's simulation at once, the handoff's again past the deadline (a throttled
        // runner still ends the pipeline before the deadline: CI flake at `162f5953`).
        simulate_delay: Duration::ZERO,
        simulate_delay_again: Duration::from_millis(1_000),
        context_slot: Some(5),
        context_slot_again: Some(10),
    };
    for (deadline_ns, sent) in [(500_000_000u64, false), (60_000_000_000, true)] {
        let dir = std::env::temp_dir().join(format!("p0-dec12-{}", Pubkey::new_unique()));
        std::fs::create_dir_all(&dir).unwrap();
        let before = slow.submits.load(Ordering::Relaxed);
        let mut safety = open_safety();
        let mut fe04 = admission(&dir, &payer, target, "ZERO_PREFUND_SAME_TX");
        let r = live_evaluate_within(
            &slow,
            &mut safety,
            Some(&mut fe04),
            &payer,
            target,
            deadline_ns,
        );
        let submits = slow.submits.load(Ordering::Relaxed) - before;
        if sent {
            assert_eq!(
                (r.decision, submits),
                (Decision::Submitted, 1),
                "{:?}",
                r.reason
            );
        } else {
            assert_eq!(r.reason_text().as_deref(), Some("FE04_HANDOFF_DEADLINE"));
            assert_eq!(
                r.reason.as_ref().map(|x| x.code()),
                Some(RejectionCode::SubmissionRefused)
            );
            assert_eq!(submits, 0);
            assert!(fe04.journal.active_hold().is_none(), "nothing reserved");
            assert_eq!(safety.capital_in_use(), 0, "Safety released");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// DEC-02 (V2-15/P7-03): the FE-04 handoff reuses the pipeline's simulation of the exact signed
/// payload — exactly one simulation (before: two) — while every binding holds; a node behind the
/// state slot is simulated again within the deadline; without time left it is refused before
/// anything is reserved, sent or held (the Safety reservation released).
#[test]
fn dec02_the_handoff_reuses_the_bound_simulation_or_simulates_again_or_refuses() {
    use darkbot_project0::safety::{Allow, Limits, Safety};
    let target = Pubkey::new_unique();
    let payer = solana_sdk::signature::Keypair::new();
    let open_safety = || {
        let mut allow = Allow::default();
        allow.execution_classes.insert("longtail");
        Safety::new(
            Limits {
                max_total_capital: u64::MAX,
                max_outstanding_inventory: u64::MAX,
                max_concurrent_positions: u32::MAX,
                max_exposure_per_attempt: u64::MAX,
                max_daily_loss: u64::MAX,
                max_failed_attempts_per_day: u32::MAX,
                max_tip: u64::MAX,
                max_slippage_bps: u64::MAX,
                max_outstanding_attempts: u32::MAX,
            },
            allow,
            true,
        )
    };
    // (context slots of the first and every later simulation, simulation time, deadline) ->
    // (simulations, handed off, reused/new/refused, last reuse refusal, refusal of the handoff)
    type Case = (
        (Option<u64>, Option<u64>),
        u64,
        u64,
        u64,
        bool,
        (u64, u64, u64),
        Option<&'static str>,
        Option<&'static str>,
    );
    let cases: [Case; 5] = [
        (
            (Some(10), Some(10)),
            0,
            u64::MAX,
            1,
            true,
            (1, 0, 0),
            None,
            None,
        ),
        (
            (Some(5), Some(10)),
            0,
            u64::MAX,
            2,
            true,
            (0, 1, 0),
            Some("SIM_REUSE_CONTEXT_SLOT"),
            None,
        ),
        (
            (None, Some(10)),
            0,
            u64::MAX,
            2,
            true,
            (0, 1, 0),
            Some("SIM_REUSE_CONTEXT_SLOT"),
            None,
        ),
        (
            (Some(5), Some(9)),
            0,
            u64::MAX,
            2,
            false,
            (0, 1, 1),
            Some("SIM_REUSE_CONTEXT_SLOT"),
            Some("FE04_SIMULATION_CONTEXT_BEHIND"),
        ),
        (
            (Some(5), Some(10)),
            160,
            150_000_000,
            1,
            false,
            (0, 0, 1),
            Some("SIM_REUSE_CONTEXT_SLOT"),
            Some("FE04_SIMULATION_NOT_REUSABLE_NO_TIME"),
        ),
    ];
    for (
        (context_slot, context_slot_again),
        delay_ms,
        deadline_ns,
        sims,
        submitted,
        stats,
        refusal,
        refused,
    ) in cases
    {
        let adapter = HandoffSubmitter {
            submits: Arc::new(AtomicU64::new(0)),
            sims: Arc::new(AtomicU64::new(0)),
            transport_fails: false,
            simulate_delay: Duration::from_millis(delay_ms),
            simulate_delay_again: Duration::from_millis(delay_ms),
            context_slot,
            context_slot_again,
        };
        let dir = std::env::temp_dir().join(format!("p0-dec02-{}", Pubkey::new_unique()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut safety = open_safety();
        let mut fe04 = admission(&dir, &payer, target, "ZERO_PREFUND_SAME_TX");
        let r = live_evaluate_within(
            &adapter,
            &mut safety,
            Some(&mut fe04),
            &payer,
            target,
            deadline_ns,
        );
        let case = format!("{context_slot:?} {delay_ms} ms");
        assert_eq!(adapter.sims.load(Ordering::Relaxed), sims, "{case}");
        let s = fe04.simulation;
        assert_eq!((s.reused, s.resimulated, s.refused), stats, "{case}");
        assert_eq!(s.last_reuse_refusal, refusal, "{case}");
        if submitted {
            assert_eq!(r.decision, Decision::Submitted, "{case}: {:?}", r.reason);
            assert_eq!(adapter.submits.load(Ordering::Relaxed), 1, "{case}");
        } else {
            assert_eq!(r.reason_text().as_deref(), refused, "{case}");
            assert_eq!(adapter.submits.load(Ordering::Relaxed), 0, "{case}");
            assert!(fe04.journal.active_hold().is_none(), "{case}: nothing held");
            assert_eq!(safety.capital_in_use(), 0, "{case}: Safety released");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// DEC-13: whatever the operator requests, the controller gate evaluates a candidate in Shadow —
/// LIVE is effective only with FE-06, which this build holds none of. Even with LIVE requested,
/// an FE-04 admission at hand and a counting adapter, the candidate is built, signed and
/// simulated and not a single submit call happens; nothing is reserved or journaled.
#[test]
fn dec13_without_fe06_a_live_request_reaches_no_submit() {
    use crate::orchestration::capability_control;
    use darkbot_project0::safety::{Allow, Limits, Safety};
    let target = Pubkey::new_unique();
    let payer = solana_sdk::signature::Keypair::new();
    let mut allow = Allow::default();
    allow.execution_classes.insert("longtail");
    let mut safety = Safety::new(
        Limits {
            max_total_capital: u64::MAX,
            max_outstanding_inventory: u64::MAX,
            max_concurrent_positions: u32::MAX,
            max_exposure_per_attempt: u64::MAX,
            max_daily_loss: u64::MAX,
            max_failed_attempts_per_day: u32::MAX,
            max_tip: u64::MAX,
            max_slippage_bps: u64::MAX,
            max_outstanding_attempts: u32::MAX,
        },
        allow,
        true,
    );
    let counting = HandoffSubmitter {
        submits: Arc::new(AtomicU64::new(0)),
        sims: Arc::new(AtomicU64::new(0)),
        transport_fails: false,
        simulate_delay: Duration::ZERO,
        simulate_delay_again: Duration::ZERO,
        context_slot: Some(10),
        context_slot_again: Some(10),
    };
    let dir = std::env::temp_dir().join(format!("p0-dec13-{}", Pubkey::new_unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut fe04 = admission(&dir, &payer, target, "ZERO_PREFUND_SAME_TX");
    // Review L5: every switch and the signer ENABLED — FE-06 alone keeps it Shadow.
    let mut econ = capability_control::EconSwitches::default();
    for e in capability_control::ECON {
        econ.set(e, true);
    }
    let mut signer = capability_control::WalletSwitch::configured(
        solana_sdk::signature::Signer::pubkey(&payer),
        &[capability_control::ROLE_FEE_PAYER],
    );
    signer.enabled = true;
    let gates = capability_control::LiveGates {
        requested: capability_control::LIVE,
        econ: &econ,
        wallets: std::slice::from_ref(&signer),
        fe06: None,
        now_unix_s: Some(0),
        epoch: None,
    };
    assert_eq!(
        capability_control::live_decision(&gates).1,
        capability_control::LIVE_BLOCK_REASON
    );
    let mode = capability_control::pipeline_mode(&gates);
    let r = evaluate_in(
        mode,
        (&counting, &mut safety, Some(&mut fe04)),
        &payer,
        target,
        u64::MAX,
        &crate::execution::hold_b::HoldGate::OBSERVE,
    );
    assert_eq!(mode, crate::execution::pipeline::Mode::Shadow);
    assert!(
        matches!(
            r.decision,
            Decision::ShadowReady | Decision::ShadowTechnicalReady
        ),
        "{:?} {:?}",
        r.decision,
        r.reason
    );
    assert_eq!(
        counting.submits.load(Ordering::Relaxed),
        0,
        "no submit call"
    );
    assert_eq!(
        counting.sims.load(Ordering::Relaxed),
        1,
        "built, signed, simulated"
    );
    assert!(fe04.journal.active_hold().is_none(), "nothing reserved");
    assert_eq!(safety.capital_in_use(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

/// DEC-11: `AdmissionBuilder`'s plan as a JIT bundle — one prerequisite swap TX before the
/// receivership TX, and the `funding` (50 base units) its buy spends held and an asset of the
/// plan (as the builder's JIT plan). `unpriced`: a lamport amount the price cannot convert.
struct BundleBuilder {
    funding: Pubkey,
    unpriced: Option<u64>,
}
impl crate::execution::pipeline::ReceivershipBuilder for BundleBuilder {
    fn native_cost_usd_micro(&self, n: u64) -> Option<u64> {
        (self.unpriced != Some(n)).then_some(n)
    }
    fn instruction_table(&self) -> &darkbot_project0::planner::InstructionTable {
        test_table()
    }
    fn plans(
        &self,
        a: &Pubkey,
        n: u64,
        s: u64,
        d: std::time::Instant,
    ) -> Result<
        Vec<crate::execution::pipeline::ReceivershipPlan>,
        crate::execution::pipeline::PlanError,
    > {
        let mut plans = AdmissionBuilder.plans(a, n, s, d)?;
        for p in &mut plans {
            p.prerequisite_txs = vec![vec![solana_sdk::instruction::Instruction {
                program_id: Pubkey::new_from_array([8; 32]),
                accounts: vec![],
                data: vec![1],
            }]];
            p.inventory_holds = vec![(self.funding, 50)];
            p.assets = vec![self.funding];
        }
        Ok(plans)
    }
    fn hold_b(
        &self,
        plan: &crate::execution::pipeline::ReceivershipPlan,
        sol_debit: Option<u64>,
    ) -> crate::execution::hold_b::BHold {
        AdmissionBuilder.hold_b(plan, sol_debit)
    }
}

/// DEC-11: the shape of the owner's submitter — single TXs on RPC, a bundle path — whose
/// bundle simulation passes: the stand-in for a qualified bundle simulator (production holds
/// none). Every message costs 200 lamports; the bundle handoff is recorded.
struct QualifiedBundleSim {
    bundles: Arc<AtomicU64>,
    sims: Arc<AtomicU64>,
}
impl SubmissionAdapter for QualifiedBundleSim {
    fn path(&self) -> SubmissionPath {
        SubmissionPath::Rpc
    }
    fn supports_bundle(&self) -> bool {
        true
    }
    fn fee_for_message(
        &self,
        tx: &VersionedTransaction,
        _: std::time::Instant,
    ) -> Result<MessageFeeQuote, darkbot_solana::submit::FeeQuoteError> {
        Ok(MessageFeeQuote {
            message_hash: tx.message.hash(),
            context_slot: 1,
            lamports: 200,
        })
    }
    fn simulate(&self, _: &VersionedTransaction, _: std::time::Instant) -> SimResult {
        panic!("a bundle is never simulated as a single TX")
    }
    fn simulate_bundle(&self, txs: &[VersionedTransaction], _: std::time::Instant) -> SimResult {
        assert!(txs.iter().all(|t| t.verify_and_hash_message().is_ok()));
        self.sims.fetch_add(1, Ordering::Relaxed);
        SimResult::Ok {
            units_consumed: Some(300),
            context_slot: Some(10),
        }
    }
    fn submit(
        &self,
        _: &VersionedTransaction,
        _: u64,
        _: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        panic!("a bundle is never submitted as a single TX")
    }
    fn submit_bundle(
        &self,
        txs: &[VersionedTransaction],
        t: u64,
        deadline: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        if std::time::Instant::now() >= deadline {
            return Err(SubmitError::Deadline { sent: false });
        }
        self.bundles.fetch_add(1, Ordering::Relaxed);
        let signatures: Vec<_> = txs.iter().map(|t| t.signatures[0]).collect();
        Ok(SubmitAck {
            path: SubmissionPath::JitoBundle,
            signature: signatures[0],
            signatures,
            submission_id: Some("bundle-1".into()),
            t_handoff_ns: t,
            serialized_len: txs
                .iter()
                .map(|t| darkbot_solana::submit::serialize_tx(t).unwrap().len())
                .sum(),
        })
    }
}

/// DEC-11: one LIVE evaluation of `target` as a JIT bundle through the production pipeline,
/// with the configured `tip`, the bundle path's failure costs and the prerequisite program
/// allowlisted.
fn bundle_evaluate<S: SubmissionAdapter>(
    adapter: &S,
    safety: &mut darkbot_project0::safety::Safety,
    fe04: &mut crate::execution::one_shot::admission::Fe04Admission,
    (payer, target): (&solana_sdk::signature::Keypair, Pubkey),
    tip: crate::execution::bundle_path::BundleTip,
    builder: &BundleBuilder,
) -> OpportunityRecord {
    use crate::execution::pipeline::{Mode, Pipeline};
    use darkbot_project0::{execution::Freshness, planner::Allowlist};
    use darkbot_solana::{
        state::blockhash::{BlockhashCache, CachedBlockhash},
        tx::{Resources, TxLimits},
    };
    let clock = Clock::new();
    let bh = BlockhashCache::new();
    bh.set(CachedBlockhash {
        hash: solana_sdk::hash::Hash::new_unique(),
        last_valid_block_height: 100,
        refreshed_ns: clock.now_ns(),
        context_slot: None,
    });
    let allow = Allowlist::default();
    let failed = FailedCostMatrix {
        jito_bundle_not_landed: Some(0),
        jito_bundle_failed: Some(0),
        ..Default::default()
    };
    let slots = darkbot_solana::ingress::SlotWatch::new();
    slots.observe(10);
    let prereq_program = [Pubkey::new_from_array([8; 32])];
    let mut network = crate::execution::pipeline::NetworkBudget::unbounded();
    // P8-CTRL.7: the LIVE bundle's egress permit (every gate, the FE-06 stand-in).
    let gates = crate::orchestration::capability_control::OfflineGates::enabled(
        solana_sdk::signature::Signer::pubkey(payer),
    );
    let mut p = Pipeline {
        mode: Mode::Live,
        clock: &clock,
        health: &TestHealth,
        builder,
        submitter: adapter,
        safety,
        blockhash: &bh,
        payer,
        allowlist: &allow,
        failed_costs: &failed,
        limits: TxLimits::default(),
        resources: Resources {
            compute_unit_limit: 100_000,
            priority_micro_lamports_per_cu: 0,
            loaded_accounts_data_size: None,
        },
        min_profit: 0,
        blockhash_max_age_ns: u64::MAX,
        p7_passed: false,
        freshness: Freshness {
            max_opportunity_age_ns: u64::MAX,
            execution_deadline_ns: u64::MAX,
        },
        bundle_tip: Some(tip),
        global_allowed: std::slice::from_ref(&test_table().program),
        prereq_allowed: &prereq_program,
        slot_watch: &slots,
        network: &mut network,
        simulation_failure: None,
        blockhash_park: crate::execution::pipeline::BlockhashPark::Off,
        fe04: Some(fe04),
        hold: &crate::execution::hold_b::HoldGate::OBSERVE,
        egress: Some(gates.gates()),
        synthetic: crate::execution::pipeline::test_synthetic(),
    };
    p.evaluate(target, "test", clock.now_ns(), 100, 10, true)
}

/// DEC-11 (B02/B05/F-20): the bundle fee model, its holds, the admission and the simulation
/// boundary. A JIT plan on the bundle path pays the configured tip in its own TX and is priced
/// as a bundle: every transaction's exact message fee (prerequisite, receivership, tip) summed,
/// plus the tip. Through the owner's production submitter (bundle path configured, no
/// qualified bundle simulator) the bundle is fee-quoted on RPC and never simulated: no Jito
/// endpoint is called, nothing is reserved. With a qualified simulation (a test stand-in) the
/// FE-04 admission binds all three signatures, the summed fee, the tip and the SOL and funding
/// holds, and hands the bundle off once (before: refused `FE04_BUNDLE_FEES_UNQUOTED`).
#[test]
fn dec11_bundle_fee_model_holds_admission_and_simulation_boundary() {
    use crate::execution::bundle_path::{
        BundleConfig, BundleTip, OwnerSubmitter, BUNDLE_SIMULATOR_UNQUALIFIED,
    };
    use crate::execution::observe_only::ObserveOnly;
    use crate::execution::one_shot::non_landing::loopback::{endpoint, Reply};
    use darkbot_project0::safety::{Allow, Limits, Safety};
    let target = Pubkey::new_unique();
    let payer = solana_sdk::signature::Keypair::new();
    let mint = Pubkey::new_unique();
    let tip = BundleTip {
        account: Pubkey::new_unique(),
        lamports: 1_000,
    };
    let open_safety = || {
        let mut allow = Allow::default();
        allow.execution_classes.insert("longtail");
        allow.assets.insert(mint);
        Safety::new(
            Limits {
                max_total_capital: u64::MAX,
                max_outstanding_inventory: u64::MAX,
                max_concurrent_positions: u32::MAX,
                max_exposure_per_attempt: u64::MAX,
                max_daily_loss: u64::MAX,
                max_failed_attempts_per_day: u32::MAX,
                max_tip: u64::MAX,
                max_slippage_bps: u64::MAX,
                max_outstanding_attempts: u32::MAX,
            },
            allow,
            true,
        )
    };
    let journal = |name: &str| {
        let d = std::env::temp_dir().join(format!("p0-dec11-{name}-{}", Pubkey::new_unique()));
        std::fs::create_dir_all(&d).unwrap();
        let mut fe04 = admission(&d, &payer, target, "ZERO_PREFUND_SAME_TX");
        fe04.approval.max_tip_lamports = tip.lamports;
        fe04.balances.insert(mint.to_string(), 1_000);
        fe04
    };

    // The production submitter: RPC fee quotes from a loopback (the `fail`-th quote, counted
    // from 0, unavailable), the Jito endpoints counted.
    let (jito_url, jito_calls) = endpoint(Arc::new(|_, _| Reply::Error));
    let production = |fail: Option<u64>| {
        let quotes = Arc::new(AtomicU64::new(0));
        let (rpc_url, rpc_calls) = endpoint(Arc::new(move |method, _| match method {
            "getFeeForMessage" if Some(quotes.fetch_add(1, Ordering::SeqCst)) == fail => {
                Reply::Error
            }
            "getFeeForMessage" => Reply::Result(serde_json::json!({
                "context": {"slot": 10}, "value": 5_000
            })),
            _ => Reply::Error,
        }));
        let mut config = darkbot_solana::runtime_config::RuntimeConfig::default();
        (config.limits.rpc_rps, config.limits.rpc_burst) = (1_000, 1_000);
        let budget = Arc::new(darkbot_solana::provider::Budget::configured(&config));
        let mut owner = OwnerSubmitter::new(
            darkbot_solana::submit::rpc::RpcSubmitter::with_budget(rpc_url, true, budget).unwrap(),
        );
        let bundle_path = BundleConfig {
            block_engine_url: jito_url.clone(),
            bundle_sim_rpc_url: Some(jito_url.clone()),
            tip,
        };
        owner
            .set_bundle_path(Some(&bundle_path), Duration::from_secs(1))
            .unwrap();
        (ObserveOnly::new(owner), rpc_calls)
    };
    let priced = BundleBuilder {
        funding: mint,
        unpriced: None,
    };
    let (production_submitter, rpc_calls) = production(None);
    let mut safety = open_safety();
    let mut fe04 = journal("unqualified");
    let r = bundle_evaluate(
        &production_submitter,
        &mut safety,
        &mut fe04,
        (&payer, target),
        tip,
        &priced,
    );
    let costs = &r.candidate_costs[0];
    assert_eq!(costs.path, "JitoBundle");
    assert_eq!(
        (costs.transaction_fee_usd_micro, costs.jito_tip_usd_micro),
        (Some(15_000), Some(1_000)),
        "three messages of 5 000 lamports, plus the tip"
    );
    let quotes = rpc_calls.lock().unwrap().clone();
    assert_eq!(quotes, vec!["getFeeForMessage"; 3], "one quote per message");
    assert_eq!(r.path.as_deref(), Some(SubmissionPath::JitoBundle.as_str()));
    assert!(
        r.reason_text()
            .unwrap()
            .contains(BUNDLE_SIMULATOR_UNQUALIFIED),
        "{:?}",
        r.reason_text()
    );
    assert!(jito_calls.lock().unwrap().is_empty(), "no Jito call");
    assert!(fe04.journal.active_hold().is_none(), "nothing is reserved");
    assert_eq!(safety.capital_in_use(), 0);
    // An unknown part leaves the bundle's cost unknown, never 0: the tip TX's quote unavailable,
    // or a tip the price cannot convert.
    for (fail, unpriced) in [(Some(2), None), (None, Some(tip.lamports))] {
        let (submitter, _) = production(fail);
        let builder = BundleBuilder {
            funding: mint,
            unpriced,
        };
        let mut fe04 = journal("unknown");
        let r = bundle_evaluate(
            &submitter,
            &mut open_safety(),
            &mut fe04,
            (&payer, target),
            tip,
            &builder,
        );
        let costs = &r.candidate_costs[0];
        assert_eq!(
            (costs.transaction_fee_usd_micro, costs.economics_known),
            (None, false),
            "{fail:?} {unpriced:?}"
        );
    }
    assert!(jito_calls.lock().unwrap().is_empty(), "no Jito call");

    // A qualified bundle simulation: the admission binds and holds the whole bundle.
    let qualified = QualifiedBundleSim {
        bundles: Arc::new(AtomicU64::new(0)),
        sims: Arc::new(AtomicU64::new(0)),
    };
    let mut safety = open_safety();
    let mut fe04 = journal("qualified");
    let r = bundle_evaluate(
        &qualified,
        &mut safety,
        &mut fe04,
        (&payer, target),
        tip,
        &priced,
    );
    assert_eq!(r.decision, Decision::Submitted, "{:?}", r.reason_text());
    assert_eq!(
        (
            qualified.sims.load(Ordering::Relaxed),
            qualified.bundles.load(Ordering::Relaxed)
        ),
        (1, 1),
        "simulated once (reused at the handoff), handed off once"
    );
    let intent = fe04.journal.intent().unwrap();
    assert_eq!(
        intent.signatures.len(),
        3,
        "prerequisite, receivership and tip TX"
    );
    assert_eq!(r.all_signatures, intent.signatures);
    assert_eq!(
        (intent.transaction_fee_lamports, intent.tip_lamports),
        (600, 1_000),
        "three messages of 200 lamports; the tip"
    );
    let hold = fe04.journal.active_hold().unwrap();
    assert_eq!(
        hold.holds["SOL"], 1_605,
        "fees 600 + tip 1 000 + flat fee 5 + record rent 0"
    );
    assert_eq!(
        hold.holds[&mint.to_string()],
        50,
        "the funding the buy spends"
    );
    assert_eq!(
        r.submitted_tip
            .as_ref()
            .map(|t| (t.signature.as_str(), t.lamports)),
        Some((intent.signatures[2].as_str(), 1_000))
    );
    assert_eq!(safety.capital_in_use(), 1_000, "reserved until settled");
}

/// DEC-16: a worker of an unbounded run (`until-stopped`): windows of 4 s from t = 1 ns.
fn rolling_worker() -> Worker {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let mut cfg = config();
    cfg.plan = MeasurementPlan::new(4).unwrap();
    cfg.unbounded = true;
    cfg.run_dir = std::env::temp_dir().join(format!(
        "c0_e1_rolling_{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&cfg.run_dir);
    cfg.paths().create().unwrap();
    let (tx, rx) = bounded("test-econ-rolling", 64);
    let h = Handle {
        tx,
        shared: Arc::new(Shared::default()),
        clock: Clock::new(),
        cfg,
    };
    h.start_rolling(1);
    Worker::new(
        h,
        rx,
        "logs".into(),
        "confirmed".into(),
        "UNKNOWN".into(),
        30,
    )
    .unwrap()
}
fn shadow_at(t: u64) -> Evidence {
    let mut r = rec();
    r.timing.t_rx_ns = Some(t);
    Evidence::Shadow(Box::new(r))
}
fn windows_of(w: &Worker) -> Vec<EconWindow> {
    let path = w.h.cfg.paths().snapshot(ECON_WINDOWS_JOURNAL);
    let mut r = crate::storage::segments::ChainReader::open(&path).unwrap();
    let mut seen = Vec::new();
    while let Ok(Some(f)) = r.next_frame() {
        if f.kind == kind::ECON_WINDOW {
            seen.push(f.typed().unwrap());
        }
    }
    seen
}
fn sealed(path: &Path) -> bool {
    let mut r = crate::analytics::journal::Reader::open(path).unwrap();
    while let Ok(Some(_)) = r.next_frame() {}
    r.sealed
}

/// DEC-16: the grid of an unbounded run — the first window `[s, s+W]`, then `(s+(w-1)W, s+wW]`
/// (a boundary belongs to the earlier window), hours within each window, nothing past the stop;
/// the stop is the run's cutoff and the first stop wins. A fixed run keeps its one window.
#[test]
fn dec16_an_unbounded_run_has_a_grid_of_windows_until_its_stop() {
    let w = rolling_worker();
    let h = &w.h;
    assert!(h.rolling() && h.cutoff().is_none() && !h.admission_closed());
    let s = 1u64;
    let width = 4_000_000_000u64;
    assert_eq!(h.window_of(0), None);
    assert_eq!(h.window_of(s), Some((1, s, s + width)));
    assert_eq!(h.window_of(s + width), Some((1, s, s + width)));
    assert_eq!(
        h.window_of(s + width + 1),
        Some((2, s + width, s + 2 * width))
    );
    assert_eq!(
        (h.hour(s), h.hour(s + width), h.hour(s + width + 1)),
        (Some(0), Some(3), Some(0))
    );
    h.stop_rolling(s + width + 10);
    h.stop_rolling(s + 3 * width);
    assert_eq!(h.cutoff(), Some(s + width + 10));
    assert_eq!(h.window_of(s + width + 11), None);
    let fixed = worker();
    assert!(!fixed.h.rolling());
    assert_eq!(fixed.h.window_of(1), Some((1, 1, 4_000_000_001)));
    assert_eq!(fixed.h.window_of(4_000_000_002), None);
}

/// DEC-16: an unbounded run measures its windows one after another with the fixed run's
/// reports: each closed window has its four provisional snapshots (no ECON_FINAL) in its own
/// sealed report journal and one typed window record; evidence of the next window waits for
/// it, evidence after its window closed is counted late and never attributed back; a close out
/// of order is refused; the stop records the open window as partial and seals the journals.
#[test]
fn dec16_an_unbounded_run_measures_its_windows_one_after_another() {
    let mut w = rolling_worker();
    let width = 4_000_000_000u64;
    w.accept(shadow_at(10)).unwrap();
    w.accept(shadow_at(3_000_000_000)).unwrap();
    // Window 2's evaluation arrives before window 1 closed: it waits.
    w.accept(shadow_at(width + 500)).unwrap();
    assert_eq!(w.evaluation_counts.iter().sum::<u64>(), 2);
    let first = w.reports.path.clone();
    w.accept(Evidence::WindowClosed {
        window: 1,
        boundary_ns: 1 + width,
        verdict: "CONTINUE".into(),
    })
    .unwrap();
    let (frames, last) = report_frames(&first);
    assert_eq!(
        frames
            .iter()
            .map(|f| (f.index, f.finalizing))
            .collect::<Vec<_>>(),
        [(1, false), (2, false), (3, false), (4, false)]
    );
    assert!(last.is_none(), "a window has no ECON_FINAL");
    let four = &frames[3].report;
    assert_eq!(four.shadow.cumulative.evaluations, 2);
    assert!(four.window.provisional);
    assert_eq!(
        four.capture_window.as_ref().map(|c| c.capture_start_ns),
        Some(1)
    );
    assert!(sealed(&first));
    // The waiting evaluation opened window 2; one from window 1 now is late.
    assert_eq!(w.evaluation_counts.iter().sum::<u64>(), 1);
    // The boundary instant belongs to window 1 only.
    assert!(!w.in_window(1 + width) && w.in_window(2 + width));
    w.accept(shadow_at(2_000_000_000)).unwrap();
    assert_eq!(
        w.evaluation_counts.iter().sum::<u64>(),
        1,
        "never attributed back"
    );
    assert_ne!(w.reports.path, first);
    w.accept(Evidence::WindowClosed {
        window: 2,
        boundary_ns: 1 + 2 * width,
        verdict: "CONTINUE".into(),
    })
    .unwrap();
    let (frames, _) = report_frames(&w.h.cfg.run_dir.join(&windows_of(&w)[1].report_journal));
    assert_eq!(frames[3].report.shadow.cumulative.evaluations, 1);
    w.h.shared.operator_stop.store(true, Ordering::Release);
    w.h.stop_rolling(1 + 2 * width + 100);
    let run_dir = w.h.cfg.run_dir.clone();
    let paths = w.h.cfg.paths();
    w.finish_rolling().unwrap();
    let path = paths.snapshot(ECON_WINDOWS_JOURNAL);
    let mut r = crate::storage::segments::ChainReader::open(&path).unwrap();
    let mut windows: Vec<EconWindow> = Vec::new();
    while let Ok(Some(f)) = r.next_frame() {
        if f.kind == kind::ECON_WINDOW {
            windows.push(f.typed().unwrap());
        }
    }
    assert_eq!(
        windows
            .iter()
            .map(|w| (w.window, w.status.clone(), w.late_evidence))
            .collect::<Vec<_>>(),
        [
            (
                1,
                WindowStatus::Closed {
                    verdict: "CONTINUE".into()
                },
                0
            ),
            (
                2,
                WindowStatus::Closed {
                    verdict: "CONTINUE".into()
                },
                1
            ),
            (
                3,
                WindowStatus::Partial {
                    reason: "OPERATOR_STOP".into()
                },
                0
            ),
        ]
    );
    assert!(windows
        .iter()
        .all(|w| w.opportunities == 0 || w.snapshots == 4));
    assert_ne!(windows[0].evaluations, windows[1].evaluations);
    let chain = crate::storage::segments::verify_chain(&path).unwrap();
    assert!(chain.sealed && chain.complete, "{chain:?}");
    assert!(
        !paths.snapshot(REPORT_JOURNAL).exists(),
        "no run-level ECON report"
    );
    let _ = std::fs::remove_dir_all(run_dir);
}

/// DEC-16 (review M3): a window close out of order is never measured on — the run stops and
/// ends incomplete with that reason, its journals sealed.
#[test]
fn dec16_a_window_close_out_of_order_stops_the_run_incomplete() {
    let mut w = rolling_worker();
    w.accept(Evidence::WindowClosed {
        window: 2,
        boundary_ns: 9,
        verdict: "CONTINUE".into(),
    })
    .unwrap();
    assert!(w.h.shared.abort.load(Ordering::Acquire));
    let paths = w.h.cfg.paths();
    let run_dir = w.h.cfg.run_dir.clone();
    let end = w.finish_rolling().unwrap_err();
    assert!(end.contains("ECON_WINDOW_ORDER: closed 2, open 1"), "{end}");
    let path = paths.snapshot(ECON_WINDOWS_JOURNAL);
    let chain = crate::storage::segments::verify_chain(&path).unwrap();
    assert!(chain.sealed && !chain.complete, "{chain:?}");
    let _ = std::fs::remove_dir_all(run_dir);
}

/// DEC-16 (review M3): control evidence is offered until its bound and never dropped silently —
/// a full queue takes it once room appears, else it is counted as a drop and reported.
#[test]
fn dec16_control_evidence_reaches_the_analytics_or_is_reported() {
    let (tx, rx) = bounded("test-econ-control", 4);
    let h = Handle {
        tx,
        shared: Arc::new(Shared::default()),
        clock: Clock::new(),
        cfg: config(),
    };
    let window = |n: u64| Evidence::WindowClosed {
        window: n,
        boundary_ns: n,
        verdict: "CONTINUE".into(),
    };
    while h.emit_control(window(0), Duration::ZERO) {}
    assert_eq!(h.shared.drops.load(Ordering::Relaxed), 1);
    assert!(!h.emit_control(window(1), Duration::from_millis(5)));
    assert_eq!(h.shared.drops.load(Ordering::Relaxed), 2);
    // The worker takes one item later and stays (a gone worker refuses everything).
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        let took = rx.try_pop().is_some();
        (took, rx)
    });
    assert!(h.emit_control(window(2), Duration::from_secs(10)));
    assert!(worker.join().unwrap().0);
    assert_eq!(h.shared.drops.load(Ordering::Relaxed), 2);
}

/// DEC-16 (D-3): a crashed unbounded econ4h run continues as itself — its windows journal and
/// its evaluations continue their chains, every window its crash interrupted is recorded partial
/// (`CRASH_RECOVERED`: nothing attributed back), and its windows go on numbered after the last
/// one its capture checked, on the grid the restart opens.
#[test]
fn dec16_a_crashed_econ4h_run_continues_its_window_numbers() {
    use crate::orchestration::until_stopped as us;
    let width = 4_000_000_000u64;
    let mut w = rolling_worker();
    w.accept(shadow_at(10)).unwrap();
    w.accept(Evidence::WindowClosed {
        window: 1,
        boundary_ns: 1 + width,
        verdict: "CONTINUE".into(),
    })
    .unwrap();
    w.accept(shadow_at(width + 500)).unwrap();
    let cfg = w.h.cfg.clone();
    let paths = cfg.paths();
    // The capture had checked window 1 (its windows journal in the market directory).
    let mut b = us::tests::binding(4, 8);
    b.mode = "econ4h".into();
    std::fs::create_dir_all(paths.market_dir()).unwrap();
    let mut capture =
        us::RollingWindows::start(&paths.market_dir(), b.clone(), "0123456789abcdef", 1_000, 0)
            .unwrap();
    capture
        .close(
            &darkbot_solana::runtime_config::RuntimeConfig::default(),
            "logs",
            us::Measured {
                now: 1_004,
                bytes: 0,
                run_volume: &cfg.run_dir,
                environment: crate::storage::preflight::Environment::default(),
                provider: darkbot_solana::provider::WindowUsage {
                    requests: 0,
                    bytes: 0,
                },
                evidence: None,
                sealed: None,
            },
        )
        .unwrap();
    // The crash: nothing ends in order, no journal is sealed.
    drop(capture);
    drop(w);
    assert_eq!(
        rolling::resumed_base(&paths).unwrap(),
        2,
        "window 1 checked, 2 interrupted"
    );
    let mut resumed = cfg.clone();
    resumed.rolling = Some(us::Rolling {
        project: cfg.run_dir.clone(),
        binding: b,
        qualification_id: "0123456789abcdef".into(),
        config: darkbot_solana::runtime_config::RuntimeConfig::default(),
        max_runtime_seconds: None,
        resume: Some(cfg.run_id.clone()),
        autostart: None,
    });
    let shared = Shared::default();
    shared.window_base.store(2, Ordering::Release);
    let (tx, rx) = bounded("test-econ-resumed", 64);
    let h = Handle {
        tx,
        shared: Arc::new(shared),
        clock: Clock::new(),
        cfg: resumed,
    };
    h.start_rolling(1);
    assert_eq!(
        h.window_of(2).map(|(n, ..)| n),
        Some(3),
        "numbered after the interrupted one"
    );
    assert_eq!(h.window_of(2 + width).map(|(n, ..)| n), Some(4));
    let mut w = Worker::new(
        h,
        rx,
        "logs".into(),
        "confirmed".into(),
        "UNKNOWN".into(),
        30,
    )
    .unwrap();
    assert_eq!(w.rolling.as_ref().unwrap().window, 3);
    assert_eq!(
        w.window(),
        Some((1, 1 + width)),
        "window 3 opens on the restart's grid"
    );
    assert!(w.evaluations.recovered(), "the evaluation chain continues");
    let windows = windows_of(&w);
    assert_eq!(
        windows
            .iter()
            .map(|x| (x.window, x.status.clone()))
            .collect::<Vec<_>>(),
        [
            (
                1,
                WindowStatus::Closed {
                    verdict: "CONTINUE".into()
                }
            ),
            (
                2,
                WindowStatus::Partial {
                    reason: us::CRASH_RECOVERED.into()
                }
            )
        ]
    );
    assert_eq!(
        (
            windows[1].snapshots,
            windows[1].opportunities,
            &windows[1].evaluations
        ),
        (0, 0, w.evaluations.recovered_from().unwrap()),
        "the interrupted window's evidence ended with its process"
    );
    w.accept(shadow_at(100)).unwrap();
    w.accept(Evidence::WindowClosed {
        window: 3,
        boundary_ns: 1 + width,
        verdict: "CONTINUE".into(),
    })
    .unwrap();
    w.h.shared.operator_stop.store(true, Ordering::Release);
    w.h.stop_rolling(1 + width + 100);
    let path = paths.snapshot(ECON_WINDOWS_JOURNAL);
    w.finish_rolling().unwrap();
    let mut r = crate::storage::segments::ChainReader::open(&path).unwrap();
    let mut numbers = Vec::new();
    while let Ok(Some(f)) = r.next_frame() {
        if f.kind == kind::ECON_WINDOW {
            numbers.push(f.typed::<EconWindow>().unwrap().window);
        }
    }
    assert_eq!(numbers, [1, 2, 3, 4]);
    assert!(
        r.sealed && !r.complete,
        "a continued chain is never complete"
    );
    let _ = std::fs::remove_dir_all(&cfg.run_dir);
}

/// DEC-16 (review M-1): the Shadow owner of a continued econ4h run reads nothing before its
/// capture held the window the crash interrupted — it waits for the verdict (woken by it), is
/// refused with the capture's refusal, and gives up at its bound without one.
#[cfg(any(feature = "historical", feature = "crate_health"))]
#[test]
fn dec16_a_continued_econ4h_owner_waits_for_the_capture_resume_verdict() {
    let (h, _rx) = test_handle(16);
    assert!(h
        .await_resume_check(Duration::from_millis(20))
        .unwrap_err()
        .starts_with("UNTIL_STOPPED_RESUME_UNCHECKED"));
    let capture = h.clone();
    let verdict = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        capture.resume_checked(true);
    });
    assert_eq!(h.await_resume_check(Duration::from_secs(10)), Ok(()));
    verdict.join().unwrap();
    let (refused, _rx) = test_handle(16);
    refused.resume_checked(false);
    assert!(refused
        .await_resume_check(Duration::from_secs(10))
        .unwrap_err()
        .starts_with("UNTIL_STOPPED_RESUME_REFUSED"));
}

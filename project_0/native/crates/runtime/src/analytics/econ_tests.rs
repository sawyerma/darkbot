use super::*;
use darkbot_solana::submit::{
    MessageFeeQuote, SimResult, SubmissionAdapter, SubmissionPath, SubmitAck, SubmitError,
};
use solana_sdk::transaction::VersionedTransaction;

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
pub(super) fn rec() -> OpportunityRecord {
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
/// Render of a typed report document (assertions read the historical keys).
pub(super) fn view<T: Serialize>(doc: &T) -> Value {
    serde_json::to_value(doc).unwrap()
}
/// The summary of an empty finished capture (a real market observer summary).
pub(super) fn market_summary(secs: f64) -> crate::market_observer::MarketSummary {
    use crate::market_observer::{EnrichConfig, MarketSource, ObserverCore, SummaryContext};
    let program = darkbot_project0::protocol::PROGRAM_ID_MAINNET
        .parse()
        .unwrap();
    let mut core = ObserverCore::new(program, MarketSource::Logs, EnrichConfig::DEFAULT);
    core.start_observation(0, u64::MAX);
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
        let out = race(&r, Some(&w), Some(&m), Some(30));
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
        let out = race(&r, Some(&w), Some(&m), Some(10));
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
        race(&r, None, None, None).class,
        RaceClass::WinnerUnresolved
    );
    assert_eq!(
        race(&r, Some(&w), None, None).class,
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
        race(&r, Some(&amb), Some(&m), None).class,
        RaceClass::WinnerIdentityAmbiguous
    );
    r.timing.t_sign_ns = None;
    assert_eq!(race(&r, Some(&w), Some(&m), None).class, RaceClass::NoSign);
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
fn atomic_writer_only_exposes_whole_json() {
    let w = worker();
    let path = w.h.cfg.run_dir.join("atomic.json");
    atomic_json(&path, &json!({"v":1})).unwrap();
    // A partial staging file cannot alter the committed snapshot.
    fs::write(path.with_extension("json.tmp"), "{broken").unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap()["v"],
        1
    );
    atomic_json(&path, &json!({"v":2})).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap()["v"],
        2
    );
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

struct CountingSubmitter {
    calls: Arc<AtomicU64>,
    sims: Arc<AtomicU64>,
}
impl SubmissionAdapter for CountingSubmitter {
    fn path(&self) -> SubmissionPath {
        SubmissionPath::Rpc
    }
    fn fee_for_message(&self, tx: &VersionedTransaction) -> Option<MessageFeeQuote> {
        Some(MessageFeeQuote {
            message_hash: tx.message.hash().to_string(),
            context_slot: 1,
            lamports: 200,
        })
    }
    fn simulate(&self, tx: &VersionedTransaction) -> SimResult {
        assert!(
            tx.verify_and_hash_message().is_ok(),
            "simulate exactly signed bytes"
        );
        self.sims.fetch_add(1, Ordering::Relaxed);
        SimResult::Ok {
            units_consumed: Some(100),
        }
    }
    fn submit(&self, _: &VersionedTransaction, _: u64) -> Result<SubmitAck, SubmitError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        panic!("must never broadcast")
    }
    fn submit_bundle(&self, _: &[VersionedTransaction], _: u64) -> Result<SubmitAck, SubmitError> {
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
    ) -> Result<u128, darkbot_project0::health::HealthError> {
        Ok(0)
    }
}
struct TestBuilder;
impl crate::execution::pipeline::ReceivershipBuilder for TestBuilder {
    fn native_cost_usd_micro(&self, n: u64) -> Option<u64> {
        Some(n)
    }
    fn plans(
        &self,
        _: &Pubkey,
        _: u64,
        _: u64,
    ) -> Result<Vec<crate::execution::pipeline::ReceivershipPlan>, String> {
        use darkbot_project0::planner::{RouteKind, Step};
        Ok(vec![crate::execution::pipeline::ReceivershipPlan {
            market_identity: None,
            steps: vec![Step::StartLiquidation, Step::Repay, Step::EndLiquidation],
            instructions: vec![],
            route: RouteKind::NoSwap,
            input: input(0),
            premium_bps: 500,
            exposure: 0,
            tip: 0,
            slippage_bps: 0,
            assets: vec![],
            tx_version: darkbot_solana::tx::BuildVersion::V0,
            lookup_tables: vec![],
            prerequisite_txs: vec![],
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
        tip_account: None,
        global_allowed: vec![],
        prereq_allowed: vec![],
        slot_watch: &slots,
    };
    let r = p.evaluate(Pubkey::new_unique(), "test", clock.now_ns(), 100, 10, true);
    assert_eq!(r.decision, Decision::ShadowReady);
    assert_eq!(r.simulation_result.as_deref(), Some("PASS"));
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
    assert!(adapter.submit(&tx, 0).is_err());
    assert!(adapter.submit_bundle(&[tx], 0).is_err());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert_eq!(adapter.attempts.load(Ordering::Relaxed), 2);
}
#[test]
fn runtime_worker_writes_four_snapshots_then_identical_final() {
    four_snapshots_then_identical_final(crate::run_layout::RunLayout::V1);
}
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
    // The sealed report journal holds every document as a typed frame (schema 2) and the run
    // wrote no JSON view (JFI-03).
    let mut reader = journal::Reader::open(&root.join("snapshots").join(REPORT_JOURNAL)).unwrap();
    let mut kinds = BTreeSet::new();
    while let Some(frame) = reader.next_frame().unwrap() {
        if frame.kind != journal::MANIFEST {
            assert_eq!(frame.schema, 2, "kind {}", frame.kind);
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
#[test]
fn legacy_views_are_checked_and_missing_ones_rendered_from_the_journals() {
    use crate::analytics::renders;
    let root = four_snapshots_then_identical_final(crate::run_layout::RunLayout::V2);
    // A worker-only run has no manifest.json, which `detect` needs to recognize layout V2.
    assert_eq!(
        renders::detect(&root).unwrap().layout,
        crate::run_layout::RunLayout::V1
    );
    let paths = crate::run_layout::RunPaths::new(&root, crate::run_layout::RunLayout::V2);
    let status = |create| -> BTreeMap<String, &'static str> {
        renders::ensure(&paths, create)
            .unwrap()
            .into_iter()
            .map(|(p, s)| (p.strip_prefix(&root).unwrap().display().to_string(), s))
            .collect()
    };
    // The run wrote no view: every one is missing until the explicit export creates it, and
    // each created view is the byte-exact render of its journal frame.
    let before = status(false);
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
    assert!(status(true).values().all(|s| *s == "CREATED"));
    assert!(status(false).values().all(|s| *s == "MATCHES"));
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
    let err = renders::ensure(&paths, true).unwrap_err();
    assert!(err.starts_with("RENDER_MISMATCH"), "{err}");
    assert_eq!(fs::read(&hour).unwrap(), bytes);
}
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
        let race = race(&r, Some(&win), Some(&market), Some(100));
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
        assert_eq!(
            (frame.kind, frame.schema),
            (kind::ECON_SHADOW_EVALUATION, 2)
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
        r.reason = Some(format!("revision-{n}"));
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

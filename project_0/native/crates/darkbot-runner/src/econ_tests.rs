use super::*;
use crate::record::{LossReason, TimingEvidence};
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
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let mut cfg = config();
    cfg.snapshot_secs = 1;
    cfg.duration_secs = 4;
    cfg.run_dir = std::env::temp_dir().join(format!(
        "c0_e1_{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&cfg.run_dir).unwrap();
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
fn complete(w: &mut Worker) {
    w.canonical_finalized = true;
    w.snapshots = 4;
    w.market_final = Some(
        json!({"actual_observation_seconds":4.0,"market_capture_complete":true,"drain_complete":true,"decode_complete":true,"source_coverage_verified":false,"verdict":"MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED"}),
    );
    w.shadow_stats = json!({"ingress_overflow":0,"winner_watch_overflow":0,"shadow_state_ready_at_shutdown":true});
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
        ("P0_ECON_DURATION_SECS", "600"),
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
    assert_eq!(c.duration_secs, 4 * c.snapshot_secs);
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
            candidates: vec![crate::winner::SameSlotCandidate {
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
        w.metrics(0, 4)["matching"]["market_without_shadow_match"],
        1
    );
    w.accept(Evidence::Winner {
        target: r.account.clone(),
        rx_slot: 1,
        winner: win,
    })
    .unwrap();
    let v = w.metrics(0, 4);
    assert_eq!(v["matching"]["market_with_shadow_match"], 1);
    let s = &v["race_timing"]["observed_speedup_sensitivity"];
    assert_eq!(s[3]["winner_rx_boundary"]["count"], 0);
    assert_eq!(s[4]["winner_rx_boundary"]["count"], 1);
    assert_eq!(v["competition"]["post"]["count"], 1);
    // A repeated evaluation may never multiply one actual winner's edge.
    r.timing.t_rx_ns = Some(1_000_000_100);
    w.accept(Evidence::Shadow(Box::new(r))).unwrap();
    assert_eq!(w.metrics(0, 4)["economics"]["known_executable"]["count"], 1);
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
    let v = w.metrics(0, 4);
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
    let v = w.metrics(0, 4);
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
    assert_eq!(w.technical(true)["verdict"], "PASS");
    for _ in 0..65 {
        w.h.emit(Evidence::Slot {
            slot: 1,
            t_rx_ns: 1,
        });
    }
    assert_eq!(w.h.shared.drops.load(Ordering::Relaxed), 1);
    assert_eq!(w.technical(true)["verdict"], "INCOMPLETE");
    let mut w = worker();
    complete(&mut w);
    w.h.cfg.retention = 1;
    for slot in [1, 2] {
        w.accept(Evidence::Slot { slot, t_rx_ns: 1 }).unwrap();
    }
    assert_eq!(w.slots.len(), 1);
    assert_eq!(w.technical(true)["verdict"], "INCOMPLETE");
}
#[test]
fn early_end_missing_snapshot_upgrade_and_writer_failures_are_incomplete() {
    let mut w = worker();
    complete(&mut w);
    w.snapshots = 3;
    assert_eq!(w.technical(true)["verdict"], "INCOMPLETE");
    w.snapshots = 4;
    w.market_final.as_mut().unwrap()["actual_observation_seconds"] = json!(3.0);
    assert_eq!(w.technical(true)["verdict"], "INCOMPLETE");
    complete(&mut w);
    w.h.shared.upgrade.store(true, Ordering::Relaxed);
    assert_eq!(w.technical(true)["verdict"], "INCOMPLETE");
    w.h.shared.upgrade.store(false, Ordering::Relaxed);
    w.h.shared.writer_errors.store(1, Ordering::Relaxed);
    assert_eq!(w.technical(true)["verdict"], "INCOMPLETE");
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
        let v: Value = serde_json::from_slice(
            &fs::read(w.h.cfg.run_dir.join(format!("hour_{i:02}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(v["shadow"]["window_delta"]["evaluations"], 1);
        assert_eq!(v["shadow"]["cumulative"]["evaluations"], i);
    }
    let last: Value =
        serde_json::from_slice(&fs::read(w.h.cfg.run_dir.join("hour_04.json")).unwrap()).unwrap();
    assert_eq!(last, w.report(4, true));
    assert_eq!(last["live_ready"], false);
    assert_eq!(
        last["technical_qualification"]["source_coverage_verified"],
        false
    );
    assert!(!w.h.cfg.run_dir.join("hour_05.json").exists());
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
        before: a,
        after: Box::new(b),
    })
    .unwrap();
    assert_eq!(
        w.metrics(0, 1)["market"]["capture_counters"]["unique_transactions"],
        1
    );
    assert_eq!(
        w.metrics(1, 2)["market"]["capture_counters"]["unique_transactions"],
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
impl crate::pipeline::ReceivershipBuilder for TestBuilder {
    fn native_cost_usd_micro(&self, n: u64) -> Option<u64> {
        Some(n)
    }
    fn plans(
        &self,
        _: &Pubkey,
        _: u64,
        _: u64,
    ) -> Result<Vec<crate::pipeline::ReceivershipPlan>, String> {
        use darkbot_project0::planner::{RouteKind, Step};
        Ok(vec![crate::pipeline::ReceivershipPlan {
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
    use crate::pipeline::{Mode, Pipeline};
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
    let mut w = worker();
    complete(&mut w);
    w.snapshots = 0;
    let h = w.h.clone();
    let dir = h.cfg.run_dir.clone();
    let join = std::thread::spawn(move || w.run());
    // Real automatic timer; no manually invoked snapshot writer in this integration test.
    std::thread::sleep(Duration::from_millis(4150));
    h.shared.finish.store(true, Ordering::Release);
    join.join().unwrap().unwrap();
    let hour: Value = serde_json::from_slice(&fs::read(dir.join("hour_04.json")).unwrap()).unwrap();
    let end: Value =
        serde_json::from_slice(&fs::read(dir.join("economics_final.json")).unwrap()).unwrap();
    assert_eq!(hour, end);
    for n in 1..=4 {
        assert!(dir.join(format!("hour_{n:02}.json")).exists());
    }
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
    let output = w.opportunity(&r);
    assert_eq!(output["strong_join"], true);
    assert_eq!(output["t_winner_rx_ns"], Value::Null);
    assert_eq!(w.metrics(0, 3)["market"]["liquidation_events"], 0);
    assert_eq!(w.metrics(3, 4)["market"]["liquidation_events"], 1);
    assert_eq!(
        w.metrics(0, 4)["matching"]["winner_live_observation_matched"],
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
    use std::io::BufRead;
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
    w.evaluations.flush().unwrap();
    let mut raw =
        BufReader::new(File::open(w.h.cfg.run_dir.join("shadow_evaluations.jsonl")).unwrap());
    let mut buf = String::new();
    let mut count = 0;
    while raw.read_line(&mut buf).unwrap() != 0 {
        count += 1;
        buf.clear();
    }
    assert_eq!(count, 1_000_000);
    complete(&mut w);
    w.final_technical = Some(w.technical(true));
    for i in 1..=4 {
        w.write_snapshot(i, true).unwrap();
        let v = w.report(i, true);
        assert_eq!(v["shadow"]["window_delta"]["evaluations"], 250_000);
        assert_eq!(v["shadow"]["cumulative"]["evaluations"], 250_000 * i);
    }
    let last: Value =
        serde_json::from_reader(File::open(w.h.cfg.run_dir.join("hour_04.json")).unwrap()).unwrap();
    assert_eq!(last, w.report(4, true));
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
    assert_eq!(w.technical(true)["verdict"], "INCOMPLETE");
    let mut w = worker();
    w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
    w.journal.flush().unwrap();
    OpenOptions::new()
        .write(true)
        .open(w.h.cfg.run_dir.join("shadow_offsets.bin"))
        .unwrap()
        .set_len(0)
        .unwrap();
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

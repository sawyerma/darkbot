//! Project_0 read-only status publisher. Engine owns the socket; gateway reads it.
pub mod analytics;
pub mod env_file;
pub mod generation;
pub mod proto;

use darkbot_runner::status_tap::{
    AssessmentSnapshot, CapturePhase, MarketSnapshot, OperationalSnapshot, OwnerSnapshot,
    PhaseSnapshot, ProviderSample, RunSnapshot, StatusSnapshot, StatusTap, StorageSample,
    TransportSnapshot, TransportStateSnapshot,
};
use prost::Message;
use proto::control::{
    engine_frame, EngineFrame, EngineHello, Heartbeat, ProjectId, StateFull, StateQuality,
    StateWatermark, TopicId,
};
use proto::project0::{
    AssetAmount, Capability, CapturePhase as WirePhase, CaptureState, Counter, DataSource,
    EffectiveRecord, Fe04Approval, FinanceSummary, LiquidatorState, LossCount, MarketAssessment,
    MarketCounters, NonLanding, NonLandingSource, OperationalConfig, OperationalParameter,
    OwnerEngine, OwnerPipeline, OwnerRun, OwnerSafety, OwnerTrigger, PhaseStatus, ProgramPin,
    ProviderBudget, ReasonCount, RebalanceBlocked, RebalancePlan, RebalanceTrade, ReceiverTotal,
    RunInfo, SourceDecision, SourceStats, StateCounters, SystemStatus, TransportStatus,
    WalletsState, WriterLane,
};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const SCHEMA_ID: &str = include_str!("../../../../../protocol/schema_id.txt");

pub fn schema_id() -> &'static str {
    SCHEMA_ID
}

#[derive(Clone, Debug)]
pub struct ReporterConfig {
    pub socket_path: PathBuf,
    pub generation_file: PathBuf,
    pub run_id: String,
    pub heartbeat: Duration,
    pub write_timeout: Duration,
    /// Minimum spacing of capture-state frames; newer states in between replace older ones.
    pub min_interval: Duration,
    /// The process runs a state owner: it publishes project0.liquidator.state, .wallets.state
    /// and .finance.summary (declared in the hello).
    pub owner: bool,
    /// B03/U-9: the project directory whose analytics job store this process publishes
    /// (project0.analytics.jobs, declared in the hello); `None`: not published.
    pub project_dir: Option<PathBuf>,
    /// The process publishes project0.status and project0.capture.state (a capture or runtime);
    /// the standalone analytics publisher does not.
    pub capture: bool,
}

impl ReporterConfig {
    pub fn new(socket_path: PathBuf, generation_file: PathBuf, run_id: String) -> Self {
        Self {
            socket_path,
            generation_file,
            run_id,
            heartbeat: Duration::from_secs(5),
            write_timeout: Duration::from_secs(2),
            min_interval: Duration::from_millis(100),
            owner: false,
            project_dir: None,
            capture: true,
        }
    }
}

#[derive(Default)]
pub struct Counters {
    pub connections: AtomicU64,
    pub frames_sent: AtomicU64,
    pub write_errors: AtomicU64,
    pub seq: AtomicU64,
}

pub struct ReporterHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// The analytics projection's publisher (B03/U-9), when the config names a project.
    analytics: Option<JoinHandle<()>>,
    socket_path: PathBuf,
    lock_path: PathBuf,
    socket_lock_path: PathBuf,
    /// R22: owner locks of the generation and of the socket path; `None` once released, so a
    /// second shutdown (`stop` and then `Drop`) never touches a successor's files.
    locks: Option<(fs::File, fs::File)>,
    pub generation: u64,
    pub counters: Arc<Counters>,
}

impl ReporterHandle {
    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(analytics) = self.analytics.take() {
            analytics.thread().unpark();
            let _ = analytics.join();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        // R22: remove our paths only while we still hold their locks, exactly once.
        if let Some(locks) = self.locks.take() {
            let _ = fs::remove_file(&self.socket_path);
            let _ = fs::remove_file(&self.socket_lock_path);
            let _ = fs::remove_file(&self.lock_path);
            drop(locks);
        }
    }
}

impl Drop for ReporterHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- tap → contract

/// project0.status; `None` until the engine reported its operational configuration.
pub fn system_status(s: &StatusSnapshot) -> Option<SystemStatus> {
    s.operational.as_deref().map(|o| SystemStatus {
        operational: Some(operational_config(o)),
    })
}

/// project0.liquidator.state of the state owner's projection.
pub fn liquidator_state(o: &OwnerSnapshot) -> LiquidatorState {
    let c = &o.engine.counters;
    let day = &o.pipeline.day;
    LiquidatorState {
        run: Some(OwnerRun {
            run_id: o.run_id.clone(),
            mode: o.mode.clone(),
            published_unix_ms: o.at_unix_ms,
            wallet: o.wallet.clone(),
            program_id: o.program_id.clone(),
            end_result: o.end_result.clone().unwrap_or_default(),
        }),
        engine: Some(OwnerEngine {
            run_state: o.engine.run_state.into(),
            halt_reason: o.engine.halt_reason.clone().unwrap_or_default(),
            ingress_generation: o.engine.ingress_generation,
            reconnects: o.engine.reconnects,
            gap_open: o.engine.gap_open,
            highest_slot: o.engine.highest_slot,
            ingress_halt_raised: o.engine.ingress_halt_raised,
            program_upgrade_latched: o.engine.program_upgrade_latched,
            counters: Some(StateCounters {
                applied: c.applied,
                stale_slot: c.stale_slot,
                stale_write_version: c.stale_write_version,
                same_slot_no_wv: c.same_slot_no_wv,
                same_slot_unversioned_refused: c.same_slot_unversioned_refused,
                below_root: c.below_root,
                buffered: c.buffered,
                buffer_dropped_le_snapshot: c.buffer_dropped_le_snapshot,
                reconcile_mismatch: c.reconcile_mismatch,
                reconcile_missing_local: c.reconcile_missing_local,
                reconcile_missing_remote: c.reconcile_missing_remote,
                orphaned_writes: c.orphaned_writes,
            }),
            program_pin: o.engine.program_pin.as_ref().map(|p| ProgramPin {
                verdict: p.verdict.into(),
                observed_hash: p.observed_hash.clone(),
                deployment_slot: p.deployment_slot,
                upgrade_authority: p.upgrade_authority.clone(),
                source_tag: p.source_tag.into(),
                reference_source_tag: p.reference_source_tag.into(),
                reference_hash: p.reference_hash.into(),
            }),
        }),
        trigger: Some(OwnerTrigger {
            mode: o.trigger.mode.into(),
            threshold_ready: o.trigger.threshold_ready,
            lane_l0: o.trigger.lanes[0],
            lane_l1: o.trigger.lanes[1],
            lane_l2: o.trigger.lanes[2],
            detect_queue: o.trigger.detect_queue,
            indexed_accounts: o.trigger.indexed_accounts,
            suppressed: o.trigger.suppressed,
            softcap_suppressed: o.trigger.softcap_suppressed,
            blockhash_refused: o.trigger.blockhash_refused,
            blockhash_parked: o.trigger.blockhash_parked,
            blockhash_rearmed: o.trigger.blockhash_rearmed,
            blockhash_superseded: o.trigger.blockhash_superseded,
            blockhash_refresh_requests: o.trigger.blockhash_refresh_requests,
        }),
        pipeline: Some(OwnerPipeline {
            evaluations: o.pipeline.evaluations,
            health_checked: day.health_checked,
            liquidatable: day.liquidatable,
            plans_built: day.plans_built,
            plans_rejected: day.plans_rejected,
            simulations_passed: day.sim_pass,
            simulations_failed: day.sim_fail,
            shadow_ready: day.shadow_ready,
            shadow_wins: day.shadow_wins,
            losses: day
                .losses
                .iter()
                .map(|(reason, count)| LossCount {
                    reason: reason.clone(),
                    count: *count,
                })
                .collect(),
            decision_p95_ns: o.pipeline.decision_p95_ns,
            fee_quote_p95_ns: o.pipeline.fee_quote_p95_ns,
            simulation_p95_ns: o.pipeline.simulation_p95_ns,
        }),
        safety: Some(OwnerSafety {
            capital_in_use_usd_micro: o.safety.capital_in_use_usd_micro,
            max_total_capital_usd_micro: o.safety.max_total_capital_usd_micro,
            max_daily_loss_usd_micro: o.safety.max_daily_loss_usd_micro,
            max_exposure_per_attempt_usd_micro: o.safety.max_exposure_per_attempt_usd_micro,
            max_concurrent_positions: o.safety.max_concurrent_positions,
            max_outstanding_attempts: o.safety.max_outstanding_attempts,
            max_failed_attempts_per_day: o.safety.max_failed_attempts_per_day,
            halt_reason: o.safety.halt_reason.clone().unwrap_or_default(),
            day_realized_usd_micro: i64::try_from(o.safety.day_realized_usd_micro).ok(),
            day_failed_attempts: o.safety.day_failed_attempts,
            day_attempts: o.safety.day_attempts,
        }),
        capabilities: o
            .capabilities
            .iter()
            .map(|c| Capability {
                name: c.name.into(),
                requested: c.requested.into(),
                effective: c.effective.into(),
                block_reason: c.block_reason.into(),
            })
            .collect(),
        rebalance: o.rebalance.as_ref().map(|r| RebalancePlan {
            mode: r.mode.into(),
            at_unix_ms: r.at_unix_ms,
            slot: r.slot,
            trades: r
                .trades
                .iter()
                .map(|t| RebalanceTrade {
                    sell: t.sell.clone(),
                    buy: t.buy.clone(),
                    amount_in: t.amount_in,
                    minimum_out: t.minimum_out,
                    cost_usd_micro: t.cost_usd_micro,
                })
                .collect(),
            blocked: r
                .blocked
                .iter()
                .map(|(asset, reason)| RebalanceBlocked {
                    asset: asset.clone(),
                    reason: (*reason).into(),
                })
                .collect(),
            quotes_pending: r.quotes_pending,
            budget_usd_micro: r.budget_usd_micro,
        }),
        sources: o
            .sources
            .iter()
            .map(|s| DataSource {
                unit: s.unit.clone(),
                role: s.role.into(),
                mode: s.mode.into(),
                qualification: s.qualification.clone(),
                generation: s.generation,
                canonical: s.canonical,
            })
            .collect(),
        last_source_decision: o.last_source_decision.as_ref().map(|d| SourceDecision {
            seq: d.seq,
            request: d.request.clone(),
            applied: d.applied,
            detail: d.detail.clone(),
        }),
        source_stats: Some(SourceStats {
            observed: o.source_stats.observed,
            rejected: o.source_stats.rejected,
            applied: o.source_stats.applied,
            refused: o.source_stats.refused,
        }),
        source_config_version: o.source_config_version,
        approval: Some(fe04_approval(&o.approval)),
        capability_config_version: o.capability_config_version,
        last_capability_decision: o.last_capability_decision.as_ref().map(|d| SourceDecision {
            seq: d.seq,
            request: d.request.clone(),
            applied: d.applied,
            detail: d.detail.clone(),
        }),
    }
}

fn amounts(m: &std::collections::BTreeMap<String, u64>) -> Vec<AssetAmount> {
    m.iter()
        .map(|(asset, units)| AssetAmount {
            asset: asset.clone(),
            units: *units,
        })
        .collect()
}

/// project0.wallets.state of the state owner's projection.
pub fn wallets_state(o: &OwnerSnapshot) -> WalletsState {
    let w = &o.wallets;
    WalletsState {
        finance_snapshot_id: w.finance_snapshot_id.clone(),
        wallet: o.wallet.clone(),
        journal_read: w.journal_read,
        verdict: w.verdict.clone(),
        unknown_reasons: w.unknown_reasons.clone(),
        // Unknown without a read journal (never a measured 0).
        attempts: w.journal_read.then_some(w.totals.attempts as u64),
        archived_attempts: w.journal_read.then_some(w.totals.archived as u64),
        landed_attempts: w.journal_read.then_some(w.totals.landed_attempts as u64),
        settled_exits: w.journal_read.then_some(w.totals.settled_exits as u64),
        fee_lamports: w.totals.fee_lamports.filter(|_| w.journal_read),
        open_inventory_mark_usd_micro: w
            .totals
            .open_inventory_mark_usd_micro
            .filter(|_| w.journal_read)
            .and_then(|v| i64::try_from(v).ok()),
        holds: amounts(&w.holds),
        policy_reserves: amounts(&w.policy_reserves),
        held_attempt: w.held_attempt.into(),
        non_landing: w.non_landing.as_ref().map(|n| NonLanding {
            subject: n.subject.into(),
            result_class: n.result_class.into(),
            alarm: n.alarm,
            sources: n
                .sources
                .iter()
                .map(|(operator, class)| NonLandingSource {
                    operator: operator.clone(),
                    class: class.clone(),
                })
                .collect(),
            c_result: n.c_result.into(),
            hold_ms: n.hold_ms,
            calls: n.calls,
            bytes: n.bytes,
            reads: n.reads,
        }),
    }
}

/// DEC-10: the secret-free approval status.
fn fe04_approval(
    a: &darkbot_runner::execution::one_shot::approval_file::ApprovalStatus,
) -> Fe04Approval {
    Fe04Approval {
        state: a.state.into(),
        reason: a.reason.clone(),
        kind: a.kind.into(),
        approval_id: a.approval_id.clone(),
        funding_class: a.funding_class.clone(),
        expires_unix_s: a.expires_unix_s,
        max_capital_usd_micro: a.max_capital_usd_micro,
        max_loss_usd_micro: a.max_loss_usd_micro,
        min_net_usd_micro: a.min_net_usd_micro,
        max_slippage_bps: a.max_slippage_bps,
        allowed_targets: a.allowed_targets,
    }
}

/// project0.finance.summary of the state owner's projection (same snapshot id as the wallets).
pub fn finance_summary(o: &OwnerSnapshot) -> FinanceSummary {
    let f = &o.finance;
    let usd = |v: Option<i128>| v.and_then(|v| i64::try_from(v).ok());
    FinanceSummary {
        finance_snapshot_id: o.wallets.finance_snapshot_id.clone(),
        as_of_slot: f.as_of_slot,
        total_capital_usd_micro: usd(f.total_capital_usd_micro),
        available_capital_usd_micro: usd(f.available_capital_usd_micro),
        reserved_capital_usd_micro: usd(f.reserved_capital_usd_micro),
        realized_net_pnl_usd_micro: usd(f.realized_net_pnl_usd_micro),
        withdrawable_profit_usd_micro: usd(f.withdrawable_profit_usd_micro),
        transferred_to_treasury_usd_micro: usd(f.transferred_to_treasury_usd_micro),
        status: f.status.into(),
    }
}

/// project0.capture.state; blocks the engine has not reported stay unset.
pub fn capture_state(s: &StatusSnapshot) -> CaptureState {
    CaptureState {
        run: s.run.as_deref().map(run_info),
        phase: s.phase.as_ref().map(phase_status),
        transport: transport_status(s.transport.as_ref(), s.transport_state.as_deref()),
        market: s.market.as_ref().map(market_counters),
        assessment: s.assessment.as_deref().map(market_assessment),
        writer: s.storage.as_ref().map(writer_lane),
        provider: s.provider.as_deref().map(provider_budget),
    }
}

fn run_info(r: &RunSnapshot) -> RunInfo {
    RunInfo {
        run_id: r.run_id.clone(),
        program_id: r.program_id.clone(),
        source: r.source.to_owned(),
        commitment: r.commitment.to_owned(),
        ws_host: r.ws_host.clone(),
        rpc_host: r.rpc_host.clone(),
        configured_duration_seconds: r.configured_duration_seconds,
        ready_timeout_seconds: r.ready_timeout_seconds,
        drain_timeout_seconds: r.drain_timeout_seconds,
        heartbeat_seconds: r.heartbeat_seconds,
        min_sample_liquidations: r.min_sample_liquidations,
        ingress_buffer_events: r.ingress_buffer_events,
        run_dir: r.run_dir.clone(),
        enrich_max_pending: r.enrich_max_pending,
        enrich_max_attempts: r.enrich_max_attempts,
        enrich_retry_ms: r.enrich_retry_ms,
        enrich_timeout_ms: r.enrich_timeout_ms,
        enrich_phase_max_seconds: r.enrich_phase_max_seconds,
        source_coverage_verified: r.source_coverage_verified,
        enrichment_available: r.enrichment_available,
        recovery_budget_ms: r.recovery_budget_ms,
        recovery_attempts: r.recovery_attempts,
        idle_timeout_ms: r.idle_timeout_ms,
        healthy_reset_ms: r.healthy_reset_ms,
        readiness: r.readiness.clone(),
        process_started_unix_s: r.process_started_unix_s,
    }
}

fn phase_status(p: &PhaseSnapshot) -> PhaseStatus {
    let phase = match p.phase {
        CapturePhase::WaitingForStream => WirePhase::WaitingForStream,
        CapturePhase::Capture => WirePhase::Capture,
        CapturePhase::Drain => WirePhase::Drain,
        CapturePhase::Finalize => WirePhase::Finalize,
        CapturePhase::Ended => WirePhase::Ended,
    };
    PhaseStatus {
        phase: phase as i32,
        changed_unix_ms: p.changed_unix_ms,
        stream_ready_unix_s: p.stream_ready_unix_s,
        stop_reason: p.stop_reason.to_owned(),
        drain_complete: p.drain_complete,
        write_error: p.write_error.clone(),
        end_result: p.end_result.clone(),
    }
}

fn transport_status(
    health: Option<&TransportSnapshot>,
    state: Option<&TransportStateSnapshot>,
) -> Option<TransportStatus> {
    if health.is_none() && state.is_none() {
        return None;
    }
    Some(TransportStatus {
        connected: health.map(|h| h.connected),
        // R19: the age at this frame's send time, never a stale relative age.
        last_frame_age_ns: health.and_then(TransportSnapshot::last_frame_age_ns_now),
        application_frames: health.map(|h| h.application_frames),
        reconnects: health.map(|h| h.reconnects),
        last_slot: health.and_then(|h| h.last_slot),
        last_state: state.map(|s| s.state.clone()).unwrap_or_default(),
        last_code: state.map(|s| s.code.clone()).unwrap_or_default(),
        last_attempt: state.map(|s| s.attempt),
        last_valid_slot: state.and_then(|s| s.last_valid_slot),
        last_state_unix_ms: state.map(|s| s.at_unix_ms),
    })
}

fn market_counters(m: &MarketSnapshot) -> MarketCounters {
    MarketCounters {
        observing: m.observing,
        stream_ready: m.stream_ready,
        counters: m
            .stats
            .counters()
            .into_iter()
            .map(|(name, value)| Counter {
                name: name.to_owned(),
                value,
            })
            .collect(),
    }
}

fn market_assessment(a: &AssessmentSnapshot) -> MarketAssessment {
    MarketAssessment {
        verdict: a.verdict.to_owned(),
        coverage_complete: a.coverage_complete,
        decode_complete: a.decode_complete,
        enrichment_complete: a.enrichment_complete,
        incomplete_reasons: a
            .incomplete_reasons
            .iter()
            .map(|r| (*r).to_owned())
            .collect(),
        truncated_unqualified_reasons: a
            .truncated_unqualified_reasons
            .iter()
            .map(|(reason, count)| ReasonCount {
                reason: (*reason).to_owned(),
                count: *count,
            })
            .collect(),
        gap_records: a.gap_records,
        truncated_pending: a.truncated_pending,
        deferred_liquidations: a.deferred_liquidations,
        distinct_targets: a.distinct_targets,
        distinct_receivers: a.distinct_receivers,
        gross_edge_total_usd: a.gross_edge_total_usd,
        gross_edge_p50_usd: a.gross_edge_p50_usd,
        gross_edge_p90_usd: a.gross_edge_p90_usd,
        gross_edge_p99_usd: a.gross_edge_p99_usd,
        top_receivers: a
            .top_receivers
            .iter()
            .map(|r| ReceiverTotal {
                receiver: r.receiver.clone(),
                liquidation_events: r.liquidation_events,
                gross_edge_total_usd: r.gross_edge_total_usd,
            })
            .collect(),
        deployment_error: a.deployment_error.map(str::to_owned),
        assessed_unix_ms: a.assessed_unix_ms,
    }
}

/// `StorageStatus` → `WriterLane`, field by field, no derived values.
fn writer_lane(sample: &StorageSample) -> WriterLane {
    let w = &sample.status;
    WriterLane {
        sampled_unix_ms: sample.at_unix_ms,
        queue_bytes: w.queue_bytes,
        queue_peak_bytes: w.queue_peak_bytes,
        admission_limit_bytes: w.admission_limit_bytes,
        max_bytes: w.max_bytes,
        free_budget_bytes: w.free_budget_bytes,
        ingress_bytes_per_s: w.ingress_bytes_per_s,
        write_bytes_per_s: w.write_bytes_per_s,
        lag_frames: w.lag_frames,
        budget_exhaustion_s: w.budget_exhaustion_s,
        disk_free_bytes: w.disk_free_bytes,
        disk_reserve_bytes: w.disk_reserve_bytes,
        disk_exhaustion_s: w.disk_exhaustion_s,
    }
}

/// `ProviderSample` → `ProviderBudget`, field by field, no derived values (F-01b).
fn provider_budget(p: &ProviderSample) -> ProviderBudget {
    let s = &p.snapshot;
    ProviderBudget {
        sampled_unix_ms: p.at_unix_ms,
        scope: s.scope.clone(),
        requests: s.requests,
        http_429_count: s.http_429_count,
        in_flight: s.in_flight,
        queued: s.queued,
        max_rps: s.max_rps,
        effective_rps: s.effective_rps,
        max_concurrency: s.max_concurrency,
        burst: s.burst,
        bytes: s.bytes,
        cooldown_cause: p.cooldown.cause.as_str().into(),
        cooldown_remaining_ms: p.cooldown.remaining_ms,
        halt_code: p.halt_code.map(Into::into),
        quota_qualified: s.quota_qualified,
    }
}

fn operational_config(o: &OperationalSnapshot) -> OperationalConfig {
    OperationalConfig {
        parameters: o
            .parameters
            .iter()
            .map(|p| OperationalParameter {
                field: p.field.to_owned(),
                unit: p.unit.to_owned(),
                value: p.value,
                default_value: p.default,
                env: p.env.to_owned(),
                source: p.source.clone(),
                minimum: p.minimum,
                maximum: p.maximum,
                provisional: p.provisional.unwrap_or_default().to_owned(),
                change_rule: p.change_rule.to_owned(),
            })
            .collect(),
        effective: o.effective.as_ref().map(|e| EffectiveRecord {
            schema: e.schema.clone(),
            profile: e.profile.name().to_owned(),
            rpc_endpoint_count: e.rpc_endpoint_count,
            wss_endpoint_count: e.wss_endpoint_count,
            fallback: e.fallback,
            adaptive: e.adaptive,
            duration: e.duration.clone(),
            verification: e.verification.clone(),
            diagnostics: e.diagnostics,
            fe_mode: e.fe_mode.clone(),
            fe_targets: e.fe_targets.clone(),
            fe_group: e.fe_group.clone(),
            sources: e
                .sources
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            provider_capacity_qualified: e.provider_capacity_qualified,
            free_only: e.free_only,
            live_authorized: e.live_authorized,
            submission_authorized: e.submission_authorized,
            threshold_trigger: e.threshold_trigger,
            c0_census: e.c0_census,
            source_geyser: e.source_geyser.clone(),
            source_geyser_provider: e.source_geyser_provider.clone(),
        }),
    }
}

// ---------------------------------------------------------------- framing

fn frame(message: engine_frame::Msg) -> Vec<u8> {
    let body = EngineFrame { msg: Some(message) }.encode_to_vec();
    let mut output = Vec::with_capacity(body.len() + 4);
    output.extend_from_slice(&(body.len() as u32).to_be_bytes());
    output.extend_from_slice(&body);
    output
}

/// The topics this source publishes: a capture or runtime project0.status and the capture
/// state, an owner process the three owner topics, a process with a project directory the
/// analytics topic.
pub fn hello_frame(
    source_id: &str,
    generation: u64,
    capture: bool,
    owner: bool,
    analytics: bool,
) -> Vec<u8> {
    let mut topics = Vec::new();
    if capture {
        topics.extend([
            TopicId::Project0Status as i32,
            TopicId::Project0CaptureState as i32,
        ]);
    }
    if owner {
        topics.extend([
            TopicId::Project0FinanceSummary as i32,
            TopicId::Project0LiquidatorState as i32,
            TopicId::Project0WalletsState as i32,
        ]);
    }
    if analytics {
        topics.push(TopicId::Project0AnalyticsJobs as i32);
    }
    frame(engine_frame::Msg::Hello(EngineHello {
        project: ProjectId::Project0 as i32,
        schema_id: schema_id().to_owned(),
        source_id: source_id.to_owned(),
        generation,
        topics,
    }))
}

pub fn state_frame<M: Message>(
    topic: TopicId,
    source_id: &str,
    generation: u64,
    seq: u64,
    value: &M,
) -> Vec<u8> {
    frame(engine_frame::Msg::State(StateFull {
        topic: topic as i32,
        generation,
        seq,
        source_timestamp_unix_ns: now_ns(),
        quality: StateQuality::Ok as i32,
        source_id: source_id.to_owned(),
        payload: value.encode_to_vec().into(),
    }))
}

/// R23: the heartbeat carries, per topic sent in this session, the generation and the highest
/// `seq` sent (the `StateWatermark` contract of `stream.proto`).
pub fn heartbeat_frame(id: u64, generation: u64, watermarks: &[(TopicId, u64)]) -> Vec<u8> {
    frame(engine_frame::Msg::Heartbeat(Heartbeat {
        heartbeat_id: id,
        server_time_unix_ns: now_ns(),
        state_watermarks: watermarks
            .iter()
            .map(|(topic, seq)| StateWatermark {
                topic: *topic as i32,
                generation,
                highest_seq_sent: *seq,
            })
            .collect(),
    }))
}

fn next_seq(counters: &Counters) -> Result<u64, String> {
    counters
        .seq
        .fetch_add(1, Ordering::Relaxed)
        .checked_add(1)
        .ok_or_else(|| "reporter sequence overflow".to_string())
}

/// Versions already sent in this session, and the highest `seq` sent per topic.
#[derive(Default)]
struct Sent {
    system: u64,
    capture: u64,
    owner: u64,
    analytics: u64,
    analytics_seq: u64,
    system_seq: u64,
    capture_seq: u64,
    liquidator_seq: u64,
    wallets_seq: u64,
    finance_seq: u64,
}
impl Sent {
    fn watermarks(&self) -> Vec<(TopicId, u64)> {
        [
            (TopicId::Project0Status, self.system_seq),
            (TopicId::Project0CaptureState, self.capture_seq),
            (TopicId::Project0LiquidatorState, self.liquidator_seq),
            (TopicId::Project0WalletsState, self.wallets_seq),
            (TopicId::Project0FinanceSummary, self.finance_seq),
            (TopicId::Project0AnalyticsJobs, self.analytics_seq),
        ]
        .into_iter()
        .filter(|(_, seq)| *seq > 0)
        .collect()
    }
}

/// Sends each topic whose blocks changed since the last send. Returns whether a capture-state
/// frame was sent.
fn send_changes(
    snapshot: &StatusSnapshot,
    sent: &mut Sent,
    source_id: &str,
    generation: u64,
    counters: &Counters,
    send: &mut dyn FnMut(&[u8]) -> Result<(), String>,
) -> Result<bool, String> {
    if snapshot.system_version != sent.system {
        if let Some(value) = system_status(snapshot) {
            let seq = next_seq(counters)?;
            send(&state_frame(
                TopicId::Project0Status,
                source_id,
                generation,
                seq,
                &value,
            ))?;
            sent.system_seq = seq;
        }
        sent.system = snapshot.system_version;
    }
    if snapshot.owner_version != sent.owner {
        if let Some(owner) = snapshot.owner.as_deref() {
            let seq = next_seq(counters)?;
            send(&state_frame(
                TopicId::Project0LiquidatorState,
                source_id,
                generation,
                seq,
                &liquidator_state(owner),
            ))?;
            sent.liquidator_seq = seq;
            let seq = next_seq(counters)?;
            send(&state_frame(
                TopicId::Project0WalletsState,
                source_id,
                generation,
                seq,
                &wallets_state(owner),
            ))?;
            sent.wallets_seq = seq;
            let seq = next_seq(counters)?;
            send(&state_frame(
                TopicId::Project0FinanceSummary,
                source_id,
                generation,
                seq,
                &finance_summary(owner),
            ))?;
            sent.finance_seq = seq;
        }
        sent.owner = snapshot.owner_version;
    }
    if snapshot.analytics_version != sent.analytics {
        if let Some(a) = snapshot.analytics.as_deref() {
            let seq = next_seq(counters)?;
            send(&state_frame(
                TopicId::Project0AnalyticsJobs,
                source_id,
                generation,
                seq,
                &analytics::analytics_jobs(a),
            ))?;
            sent.analytics_seq = seq;
        }
        sent.analytics = snapshot.analytics_version;
    }
    if snapshot.capture_version == sent.capture {
        return Ok(false);
    }
    let seq = next_seq(counters)?;
    send(&state_frame(
        TopicId::Project0CaptureState,
        source_id,
        generation,
        seq,
        &capture_state(snapshot),
    ))?;
    sent.capture = snapshot.capture_version;
    sent.capture_seq = seq;
    Ok(true)
}

// ---------------------------------------------------------------- socket lifecycle

pub fn start(cfg: ReporterConfig, tap: StatusTap) -> Result<ReporterHandle, String> {
    use std::os::unix::ffi::OsStrExt;
    let n = cfg.socket_path.as_os_str().as_bytes().len();
    if n >= 100 {
        return Err(format!(
            "reporter socket path too long ({n} bytes, macOS limit ~104): {}",
            cfg.socket_path.display()
        ));
    }
    let lock_path = cfg.generation_file.with_extension("generation.lock");
    let lock = acquire_owner_lock(&lock_path, "generation")?;
    // R22: the socket path has an owner lock of its own, so two reporters with different
    // generation files serialize probe, stale removal and bind of the same socket.
    let mut socket_lock_name = cfg.socket_path.as_os_str().to_owned();
    socket_lock_name.push(".lock");
    let socket_lock_path = PathBuf::from(socket_lock_name);
    let socket_lock = match acquire_owner_lock(&socket_lock_path, "socket") {
        Ok(l) => l,
        Err(e) => {
            let _ = fs::remove_file(&lock_path);
            return Err(e);
        }
    };
    let release = |socket: bool| {
        if socket {
            let _ = fs::remove_file(&cfg.socket_path);
        }
        let _ = fs::remove_file(&socket_lock_path);
        let _ = fs::remove_file(&lock_path);
    };
    if let Err(e) = remove_stale_socket(&cfg.socket_path) {
        release(false);
        return Err(e);
    }
    // Refuse replacing someone else's UNIX socket, including a live server.
    let listener = match UnixListener::bind(&cfg.socket_path) {
        Ok(listener) => listener,
        Err(e) => {
            release(false);
            return Err(format!("reporter bind: {e}"));
        }
    };
    let ready = (|| -> Result<u64, String> {
        fs::set_permissions(&cfg.socket_path, fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        generation::next(&cfg.generation_file)
    })();
    let generation = match ready {
        Ok(n) => n,
        Err(e) => {
            release(true);
            return Err(e);
        }
    };
    let stop = Arc::new(AtomicBool::new(false));
    let analytics = match &cfg.project_dir {
        Some(project) => match darkbot_runner::analytics::jobs::spawn_projection(
            project.clone(),
            tap.clone(),
            stop.clone(),
        ) {
            Ok(thread) => Some(thread),
            Err(e) => {
                release(true);
                return Err(format!("reporter analytics publisher: {e}"));
            }
        },
        None => None,
    };
    let counters = Arc::new(Counters::default());
    let source_id = format!("project0-{}-{}", std::process::id(), now_ns());
    let (stop_thread, counts_thread, cfg_thread) = (stop.clone(), counters.clone(), cfg.clone());
    let join = thread::Builder::new()
        .name("p0-reporter".into())
        .spawn(move || {
            serve(
                listener,
                cfg_thread,
                tap,
                source_id,
                generation,
                stop_thread,
                counts_thread,
            );
        })
        .map_err(|e| e.to_string());
    let thread = match join {
        Ok(thread) => thread,
        Err(e) => {
            stop.store(true, Ordering::Release);
            if let Some(analytics) = analytics {
                analytics.thread().unpark();
                let _ = analytics.join();
            }
            release(true);
            return Err(e);
        }
    };
    Ok(ReporterHandle {
        stop,
        thread: Some(thread),
        analytics,
        socket_path: cfg.socket_path,
        lock_path,
        socket_lock_path,
        locks: Some((lock, socket_lock)),
        generation,
        counters,
    })
}

/// R22: process-bound ownership. The advisory lock dies with its process, so a crashed owner's
/// leftover lock file never blocks a restart, while a live owner keeps every second start out.
/// The locked inode must still be the path's file (an owner removing it on shutdown races
/// with a starter that opened it before).
fn acquire_owner_lock(lock_path: &std::path::Path, what: &str) -> Result<fs::File, String> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(libc::O_NOFOLLOW)
        .open(lock_path)
        .map_err(|e| format!("reporter {what} lock inaccessible: {e}"))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(fs::TryLockError::WouldBlock) => {
            return Err(format!("reporter {what} already owned by a live process"))
        }
        Err(fs::TryLockError::Error(e)) => return Err(format!("reporter {what} lock: {e}")),
    }
    let held = lock.metadata().map_err(|e| e.to_string())?;
    match fs::symlink_metadata(lock_path) {
        Ok(path) if path.dev() == held.dev() && path.ino() == held.ino() => Ok(lock),
        _ => Err(format!("reporter {what} lock replaced concurrently; retry")),
    }
}

/// R22: a socket file left by a crashed owner (we hold the owner lock) is removed only if it is
/// a socket of our own user that nobody serves; a live server, a foreign user's socket, a
/// symlink or any other file stays untouched and refuses the start.
fn remove_stale_socket(socket_path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let meta = match fs::symlink_metadata(socket_path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("reporter socket path: {e}")),
    };
    // SAFETY: geteuid has no preconditions and cannot fail.
    let uid = unsafe { libc::geteuid() };
    if !meta.file_type().is_socket() || meta.uid() != uid {
        return Err("reporter socket path occupied by a foreign file".into());
    }
    match UnixStream::connect(socket_path) {
        Ok(_) => Err("reporter socket served by a live process".into()),
        Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
            fs::remove_file(socket_path).map_err(|e| format!("reporter stale socket removal: {e}"))
        }
        Err(e) => Err(format!("reporter socket probe: {e}")),
    }
}

/// Longest wait between checks of the stop flag.
const STOP_POLL: Duration = Duration::from_millis(500);

fn serve(
    listener: UnixListener,
    cfg: ReporterConfig,
    tap: StatusTap,
    source_id: String,
    generation: u64,
    stop: Arc<AtomicBool>,
    counts: Arc<Counters>,
) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                counts.connections.fetch_add(1, Ordering::Relaxed);
                if let Err(e) = session(stream, &cfg, &tap, &source_id, generation, &stop, &counts)
                {
                    counts.write_errors.fetch_add(1, Ordering::Relaxed);
                    // R23: bounded, non-blocking diagnostics; a blocked stderr never holds the
                    // reporter thread that shutdown joins.
                    darkbot_runner::console::line(format!("[P0/REPORTER] session error: {e}"));
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50))
            }
            Err(e) => {
                darkbot_runner::console::line(format!("[P0/REPORTER] accept error: {e}"));
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn session(
    mut stream: UnixStream,
    cfg: &ReporterConfig,
    tap: &StatusTap,
    source_id: &str,
    generation: u64,
    stop: &AtomicBool,
    counters: &Counters,
) -> Result<(), String> {
    // An accepted stream inherits the listener's non-blocking mode on macOS: writes block, up
    // to the write timeout, never fail with EAGAIN on a frame larger than the socket buffer.
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(cfg.write_timeout))
        .map_err(|e| e.to_string())?;
    let mut send = |bytes: &[u8]| -> Result<(), String> {
        stream.write_all(bytes).map_err(|e| e.to_string())?;
        counters.frames_sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    };
    send(&hello_frame(
        source_id,
        generation,
        cfg.capture,
        cfg.owner,
        cfg.project_dir.is_some(),
    ))?;
    let mut reader = tap.reader();
    let mut sent = Sent::default();
    let mut heartbeat_id = 0u64;
    let mut last_heartbeat_at = Instant::now();
    let mut last_capture_at: Option<Instant> = None;
    while !stop.load(Ordering::Acquire) {
        // R21: wake for the next heartbeat deadline (configured period), at the latest after
        // STOP_POLL to observe a stop request; state changes wake earlier.
        let until_heartbeat = cfg.heartbeat.saturating_sub(last_heartbeat_at.elapsed());
        if let Some(mut snapshot) = reader.wait_changed(until_heartbeat.min(STOP_POLL)) {
            // At most one capture state per min_interval; whatever arrived meanwhile wins.
            if snapshot.capture_version != sent.capture {
                if let Some(at) = last_capture_at {
                    let since = at.elapsed();
                    if since < cfg.min_interval {
                        thread::sleep(cfg.min_interval - since);
                        if let Some(newer) = reader.wait_changed(Duration::ZERO) {
                            snapshot = newer;
                        }
                    }
                }
            }
            if send_changes(
                &snapshot, &mut sent, source_id, generation, counters, &mut send,
            )? {
                last_capture_at = Some(Instant::now());
            }
            reader.delivered(snapshot.seq);
        }
        // Heartbeats follow their own clock, including during continuous state traffic.
        if last_heartbeat_at.elapsed() >= cfg.heartbeat {
            heartbeat_id = heartbeat_id.saturating_add(1);
            send(&heartbeat_frame(
                heartbeat_id,
                generation,
                &sent.watermarks(),
            ))?;
            last_heartbeat_at = Instant::now();
        }
    }
    // Shutdown: deliver the final state (e.g. phase ENDED) before the socket closes.
    if let Some(snapshot) = reader.wait_changed(Duration::ZERO) {
        if let Some(at) = last_capture_at {
            thread::sleep(cfg.min_interval.saturating_sub(at.elapsed()));
        }
        send_changes(
            &snapshot, &mut sent, source_id, generation, counters, &mut send,
        )?;
        reader.delivered(snapshot.seq);
    }
    Ok(())
}

pub fn config_from_env(
    run_id: &str,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<ReporterConfig>, String> {
    let Some(socket) = lookup("P0_REPORT_SOCKET").filter(|s| !s.trim().is_empty()) else {
        return Ok(None);
    };
    let generation_file = lookup("P0_REPORT_GENERATION_FILE")
        .filter(|s| !s.trim().is_empty())
        .ok_or("P0_REPORT_GENERATION_FILE required with P0_REPORT_SOCKET")?;
    let ms =
        |key: &str, default: u64, range: std::ops::RangeInclusive<u64>| -> Result<u64, String> {
            let v = lookup(key)
                .map(|v| v.trim().parse::<u64>().map_err(|e| format!("{key}: {e}")))
                .transpose()?
                .unwrap_or(default);
            if range.contains(&v) {
                Ok(v)
            } else {
                Err(format!("{key} out of range: {v}"))
            }
        };
    let heartbeat = ms("P0_REPORT_HEARTBEAT_MS", 5000, 500..=60_000)?;
    // At most 10 Hz, with room inside the capture's two-second final handoff budget.
    let min_interval = ms("P0_REPORT_MIN_INTERVAL_MS", 100, 100..=1_000)?;
    let mut config = ReporterConfig::new(
        socket.trim().into(),
        generation_file.trim().into(),
        run_id.to_owned(),
    );
    config.heartbeat = Duration::from_millis(heartbeat);
    config.min_interval = Duration::from_millis(min_interval);
    // B03/U-9: only the standalone analytics publisher sets it; with the gateway's
    // `project0.analytics` feed it is the topic's one owner.
    config.project_dir = lookup("P0_REPORT_ANALYTICS_PROJECT_DIR")
        .filter(|s| !s.trim().is_empty())
        .map(|s| PathBuf::from(s.trim()));
    Ok(Some(config))
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use darkbot_runner::status_tap::{
        operational_snapshot_of, EffectiveConfig, Limits, Profile, ReceiverRank,
    };
    use std::path::Path;

    /// An owner projection with every block set and an unknown finance total (never 0).
    /// A counterfactual with `n` capital scenarios, amounts beyond u64/i64 where they can be.
    fn golden_counterfactual(n: u64) -> darkbot_runner::analytics::backtest::Counterfactual {
        use darkbot_runner::analytics::backtest::{Counterfactual, Scenario, LANDING, SAFETY};
        Counterfactual {
            matched: 1,
            unknown: 2,
            funding_excluded: 3,
            below_minimum: 4,
            wins: 5,
            lost_races: 6,
            no_chain_event: 7,
            scenarios: (1..=n)
                .map(|i| Scenario {
                    capital_usd_micro: i * 1_000_000_000,
                    wins: i,
                    win_net_usd_micro: -170_141_183_460_469_231_731_687_303_715_884_105_728,
                    failed_attempts: 1,
                    failed_cost_usd_micro: u128::MAX,
                    failed_cost_unknown: 1,
                    over_budget: 0,
                    net_usd_micro: (i % 2 == 0).then_some(i128::from(i) * -1_000),
                })
                .collect(),
            loss_attribution: Default::default(),
            loss_attribution_model: "M".into(),
            landing: LANDING.into(),
            safety: SAFETY.into(),
            concurrent_capital_allocation_proven: false,
        }
    }

    /// One run without and one with an unreadable qualification; a completed backtest and a
    /// running optimization.
    fn golden_analytics() -> darkbot_runner::analytics::jobs::AnalyticsSnapshot {
        use darkbot_runner::analytics::backtest::{BacktestSummary, Observed, Params, Space};
        use darkbot_runner::analytics::jobs::*;
        let params = Params {
            min_profit_usd_micro: 1_500_000,
            capital_usd_micro: vec![1_000_000_000, 2_000_000_000],
            funding: vec!["JIT_EXACT_IN".into()],
        };
        let request = |id: &str, spec| JobRequest {
            job_id: id.into(),
            run: "run-a".into(),
            verification_result: None,
            max_seconds: 60,
            max_ram_bytes: 1 << 20,
            spec,
            requested_unix_ms: Some(1_760_000_000_000),
        };
        let run = |id: &str, qualification| RunEntry {
            run_id: id.into(),
            root: "data/econ4h",
            layout: "NATIVE_V2",
            capture_complete: true,
            qualification,
            modified_unix_ms: Some(1_760_000_000_100),
            running: false,
        };
        AnalyticsSnapshot {
            at_unix_ms: 1_760_000_003_000,
            runner_busy: true,
            runner_error: None,
            runs: vec![
                run("run-a", None),
                run(
                    "run-b",
                    Some(Err("QUALIFICATION_UNREADABLE: FRAME_CRC".into())),
                ),
            ],
            jobs: vec![
                JobStatus {
                    request: request(
                        "opt-1",
                        JobSpec::Optimize {
                            baseline: params.clone(),
                            space: Space {
                                min_profit_usd_micro: vec![1_500_000, 2_000_000],
                                funding: vec![vec![], vec!["JIT_EXACT_IN".into()]],
                                capital_usd_micro: params.capital_usd_micro.clone(),
                            },
                            holdout_bps: 3_000,
                            max_points: 4,
                            save_profile: true,
                        },
                    ),
                    state: JobState::Running {
                        pid: 7,
                        started_unix_ms: Some(1_760_000_001_000),
                    },
                },
                JobStatus {
                    request: request("bt-1", JobSpec::Backtest { params }),
                    state: JobState::Done(Box::new(JobOutcome {
                        job_id: "bt-1".into(),
                        state: COMPLETED.into(),
                        code: String::new(),
                        result: Some(JobResult::Backtest {
                            result_id: "r".repeat(64),
                            capture_run_id: "run-a".into(),
                            summary: Box::new(BacktestSummary {
                                rows: 1,
                                attempts: 2,
                                observed: Observed {
                                    liquidations: 3,
                                    ..Default::default()
                                },
                                counterfactual: golden_counterfactual(2),
                                realized: "NOT_RUN".into(),
                                result_sha256: [9; 32],
                            }),
                        }),
                        worker_pid: 7,
                        started_unix_ms: Some(1_760_000_000_500),
                        finished_unix_ms: None,
                    })),
                },
                JobStatus {
                    request: request(
                        "pc-1",
                        JobSpec::PostCapture {
                            chain_verify: "strict".into(),
                            offline: true,
                            resume: true,
                            provider_evidence: None,
                        },
                    ),
                    state: JobState::Done(Box::new(JobOutcome {
                        job_id: "pc-1".into(),
                        state: COMPLETED.into(),
                        code: String::new(),
                        result: Some(JobResult::PostCapture {
                            verification_ok: false,
                            report_ok: true,
                            verdict: None,
                        }),
                        worker_pid: 8,
                        started_unix_ms: Some(1_760_000_000_600),
                        finished_unix_ms: Some(1_760_000_000_700),
                    })),
                },
            ],
            error: None,
        }
    }

    fn golden_owner() -> OwnerSnapshot {
        use darkbot_runner::status_tap::{
            OwnerCapability, OwnerEngine as E, OwnerFinance, OwnerPipeline as P, OwnerSafety as S,
            OwnerTrigger as T, OwnerWallets,
        };
        let mut day = darkbot_runner::analytics::shadow_ledger::DaySummary {
            health_checked: 40,
            liquidatable: 3,
            plans_built: 2,
            sim_pass: 1,
            sim_fail: 1,
            shadow_ready: 1,
            ..Default::default()
        };
        day.losses.insert("ROUTING".into(), 2);
        let counters = darkbot_runner::status_tap::StateCounters {
            applied: 1_000,
            same_slot_unversioned_refused: 2,
            ..Default::default()
        };
        OwnerSnapshot {
            at_unix_ms: 1_760_000_003_000,
            run_id: "golden-run".into(),
            mode: "shadow".into(),
            wallet: "LiqWa11et1111111111111111111111111111111111".into(),
            program_id: "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA".into(),
            end_result: None,
            engine: E {
                run_state: "READY",
                halt_reason: None,
                ingress_generation: 2,
                reconnects: 1,
                gap_open: false,
                highest_slot: 400_000_200,
                ingress_halt_raised: false,
                program_upgrade_latched: true,
                counters,
                program_pin: Some(darkbot_runner::status_tap::OwnerProgramPin {
                    verdict: "NOT_REFERENCE",
                    observed_hash: "8eY1fD3o9pR4vZ2wQ7nK5mT6xJ1cB3aL9hG2sF4dE8uW".into(),
                    deployment_slot: 371_000_000,
                    upgrade_authority: Some("Upgrade1111111111111111111111111111111111111".into()),
                    source_tag: "mrgn-0.1.12-rc2",
                    reference_source_tag: "mrgn-0.1.11-rc1",
                    reference_hash: "H57mqzeof3mnCmVTPJzmzeTfs7G6zbpaApQP4yNycr3f",
                }),
            },
            trigger: T {
                mode: "sweep",
                threshold_ready: false,
                lanes: [1, 2, 3],
                detect_queue: 4,
                indexed_accounts: 83_274,
                suppressed: 5,
                softcap_suppressed: 1,
                blockhash_refused: 6,
                blockhash_parked: 1,
                blockhash_rearmed: 4,
                blockhash_superseded: 1,
                blockhash_refresh_requests: 2,
            },
            pipeline: P {
                evaluations: 7,
                day,
                decision_p95_ns: Some(4_500_000),
                fee_quote_p95_ns: Some(180_000_000),
                simulation_p95_ns: None,
            },
            safety: S {
                capital_in_use_usd_micro: 0,
                max_total_capital_usd_micro: 100_000_000,
                max_daily_loss_usd_micro: 5_000_000,
                max_exposure_per_attempt_usd_micro: 10_000_000,
                max_concurrent_positions: 1,
                max_outstanding_attempts: 1,
                max_failed_attempts_per_day: 3,
                halt_reason: None,
                day_realized_usd_micro: 0,
                day_failed_attempts: 0,
                day_attempts: 0,
            },
            // DEC-13: LIVE requested by the operator, still not effective without FE-06.
            capabilities: vec![OwnerCapability {
                name: "LIVE",
                requested: "LIVE",
                effective: "BLOCKED",
                block_reason: "FE-06/P8-LT: LIVE is not authorized (B10)",
            }],
            wallets: OwnerWallets {
                finance_snapshot_id: "golden-run-3".into(),
                journal_read: true,
                verdict: "NO_TRADE".into(),
                unknown_reasons: vec![],
                totals: darkbot_runner::execution::accounting::Totals {
                    realized_net_usd_micro: Some(0),
                    fee_lamports: Some(0),
                    open_inventory_mark_usd_micro: Some(0),
                    ..Default::default()
                },
                holds: [("SOL".to_string(), 0)].into(),
                policy_reserves: [("SOL".to_string(), 50_000_000)].into(),
                held_attempt: "NONE",
                non_landing: None,
            },
            finance: OwnerFinance {
                as_of_slot: 400_000_200,
                total_capital_usd_micro: None,
                available_capital_usd_micro: None,
                reserved_capital_usd_micro: Some(0),
                realized_net_pnl_usd_micro: Some(0),
                withdrawable_profit_usd_micro: None,
                transferred_to_treasury_usd_micro: Some(0),
                status: "PARTIAL",
            },
            rebalance: Some(darkbot_runner::status_tap::OwnerRebalance {
                mode: "OBSERVE",
                at_unix_ms: Some(1_760_000_000_900),
                slot: 400_000_200,
                trades: vec![darkbot_runner::status_tap::RebalanceTrade {
                    sell: "SOL".into(),
                    buy: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".into(),
                    amount_in: 3_000_000_000,
                    minimum_out: 450_000_000,
                    cost_usd_micro: 1_250_000,
                }],
                blocked: vec![("PREHELD".into(), "INVENTORY_BALANCE_UNKNOWN")],
                quotes_pending: 0,
                budget_usd_micro: 5_000_000,
            }),
            sources: vec![
                darkbot_runner::status_tap::OwnerSource {
                    unit: "geyser:publicnode".into(),
                    role: "AUTHORITATIVE",
                    mode: "observe",
                    qualification: "NOT_RUN".into(),
                    generation: 2,
                    canonical: false,
                },
                darkbot_runner::status_tap::OwnerSource {
                    unit: "wss".into(),
                    role: "AUTHORITATIVE",
                    mode: "active",
                    qualification: "QUALIFIED:LONG_TAIL_WSS_BASELINE".into(),
                    generation: 1,
                    canonical: true,
                },
            ],
            last_source_decision: Some(darkbot_runner::status_tap::OwnerSourceDecision {
                seq: 3,
                request: "geyser:publicnode active".into(),
                applied: false,
                detail: "SOURCE_ACTIVE_REQUIRES_QUALIFICATION".into(),
            }),
            source_stats: darkbot_runner::status_tap::OwnerSourceStats {
                observed: 120,
                rejected: 2,
                applied: 1,
                refused: 1,
            },
            source_config_version: Some(3),
            capability_config_version: Some(1),
            last_capability_decision: Some(darkbot_runner::status_tap::OwnerSourceDecision {
                seq: 1,
                request: "LIVE LIVE".into(),
                applied: true,
                detail:
                    "requested LIVE · effective BLOCKED · FE-06/P8-LT: LIVE is not authorized (B10)"
                        .into(),
            }),
            approval: darkbot_runner::execution::one_shot::approval_file::ApprovalStatus {
                state: "BINDING_MISMATCH",
                reason: "code_sha".into(),
                kind: "attempt",
                approval_id: "pilot-1".into(),
                funding_class: "ZERO_PREFUND_SAME_TX".into(),
                expires_unix_s: 4_000_000_000,
                max_capital_usd_micro: 10_000_000,
                max_loss_usd_micro: 1_000_000,
                min_net_usd_micro: 100,
                max_slippage_bps: 50,
                allowed_targets: 1,
            },
        }
    }

    fn golden_snapshot() -> StatusSnapshot {
        let mut market = MarketSnapshot {
            observing: true,
            stream_ready: true,
            ..Default::default()
        };
        market.stats.unique_transactions = 12;
        market.stats.liquidation_events = 2;
        market.stats.gap_count = 3;
        market.stats.transport_http_429 = 7;
        market.stats.first_observed_slot = Some(400_000_000);
        market.stats.last_observed_slot = Some(400_000_123);
        let mut storage = StorageSample {
            at_unix_ms: 1_760_000_002_000,
            status: Default::default(),
        };
        storage.status.queue_bytes = 1;
        storage.status.queue_peak_bytes = u64::MAX;
        storage.status.disk_free_bytes = Some(10);
        storage.status.disk_reserve_bytes = 11;
        StatusSnapshot {
            seq: 9,
            system_version: 1,
            capture_version: 8,
            owner_version: 1,
            owner: Some(Arc::new(golden_owner())),
            analytics_version: 1,
            analytics: Some(Arc::new(golden_analytics())),
            operational: Some(Arc::new(operational_snapshot_of(&Limits::default(), None))),
            run: Some(Arc::new(RunSnapshot {
                run_id: "golden-run".into(),
                program_id: "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA".into(),
                source: "logs",
                commitment: "finalized",
                ws_host: "wss://api.mainnet-beta.solana.com".into(),
                configured_duration_seconds: 180,
                process_started_unix_s: 1_760_000_000,
                ..Default::default()
            })),
            phase: Some(PhaseSnapshot {
                phase: CapturePhase::Capture,
                changed_unix_ms: 1_760_000_001_000,
                stream_ready_unix_s: Some(1_760_000_001),
                ..Default::default()
            }),
            transport: Some(TransportSnapshot {
                connected: true,
                last_frame_age_ns: Some(1_500_000),
                last_slot: Some(400_000_123),
                application_frames: 42,
                reconnects: 1,
                sampled_at: None,
            }),
            transport_state: Some(Arc::new(TransportStateSnapshot {
                state: "RECOVERED".into(),
                code: "WSS_HTTP_429".into(),
                attempt: 2,
                last_valid_slot: Some(400_000_100),
                at_unix_ms: 1_760_000_000_500,
            })),
            market: Some(market),
            assessment: Some(Arc::new(AssessmentSnapshot {
                verdict: "MARKET_DATA_INCOMPLETE",
                incomplete_reasons: vec!["GAP"],
                gap_records: 1,
                assessed_unix_ms: 1_760_000_002_000,
                ..Default::default()
            })),
            storage: Some(storage),
            provider: Some(Arc::new(provider_sample())),
        }
    }

    /// A budget in a server cooldown that refuses every admission (F-01b golden values).
    fn provider_sample() -> ProviderSample {
        ProviderSample {
            at_unix_ms: 1_760_000_002_000,
            snapshot: darkbot_runner::status_tap::ProviderSnapshot {
                scope: "PROCESS_SOLANA_PROVIDER_ACCOUNT".into(),
                requests: 21,
                http_429_count: 3,
                in_flight: 1,
                queued: 2,
                max_rps: 10,
                effective_rps: 2.5,
                max_concurrency: 4,
                burst: 5,
                bytes: u64::MAX,
                quota_qualified: false,
            },
            cooldown: darkbot_runner::status_tap::CooldownStatus {
                cause: darkbot_runner::status_tap::CooldownCause::RetryAfterUnrepresentable,
                remaining_ms: None,
                last_retry_after: darkbot_runner::status_tap::RetryAfterHeader::Absent,
                max_retry_after_secs: 30,
                http_429_count: 3,
            },
            halt_code: Some("PROVIDER_BUDGET_POISONED"),
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Committed wire bytes, also decoded by the frontend contract test. Regenerate on purpose
    /// with `UPDATE_GOLDEN=1` and review the diff.
    fn check_golden(name: &str, bytes: &[u8]) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../protocol/golden/project0")
            .join(name);
        let encoded = hex(bytes);
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &encoded).unwrap();
            return;
        }
        let expected = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; generate with UPDATE_GOLDEN=1", path.display()));
        assert_eq!(expected.trim(), encoded, "{name}: wire bytes changed");
    }

    #[test]
    fn golden_frames_match_the_committed_wire_bytes() {
        let s = golden_snapshot();
        check_golden("capture_state.hex", &capture_state(&s).encode_to_vec());
        check_golden(
            "system_status.hex",
            &system_status(&s).unwrap().encode_to_vec(),
        );
        let owner = s.owner.as_deref().unwrap();
        check_golden(
            "liquidator_state.hex",
            &liquidator_state(owner).encode_to_vec(),
        );
        check_golden("wallets_state.hex", &wallets_state(owner).encode_to_vec());
        // DEC-03: an UNKNOWN attempt with an open non-landing contradiction (N-2).
        let mut held = owner.clone();
        held.wallets.held_attempt = "UNKNOWN";
        held.wallets.non_landing = Some(darkbot_runner::status_tap::OwnerNonLanding {
            subject: "ATTEMPT",
            result_class: "VETO_CONTRADICTION_OPEN",
            alarm: true,
            sources: vec![
                ("op-a".into(), "SIGHTING".into()),
                ("op-b".into(), "UNUSABLE_RATE_LIMITED".into()),
            ],
            c_result: "NEGATIVE",
            hold_ms: Some(42_000),
            calls: Some(20),
            bytes: Some(1_234_567),
            reads: 3,
        });
        check_golden(
            "wallets_state_non_landing.hex",
            &wallets_state(&held).encode_to_vec(),
        );
        check_golden(
            "finance_summary.hex",
            &finance_summary(owner).encode_to_vec(),
        );
        check_golden(
            "analytics_jobs.hex",
            &analytics::analytics_jobs(s.analytics.as_deref().unwrap()).encode_to_vec(),
        );
    }

    /// B03/U-9: the analytics projection keeps i128/u128 amounts exact, an unknown net and a
    /// missing qualification unset, an unreadable qualification visible, and the job states and
    /// parameters as the store holds them.
    #[test]
    fn b03_analytics_projection_is_exact_and_never_invents_values() {
        use crate::proto::project0::analytics_job::Result as R;
        let a = analytics::analytics_jobs(&golden_analytics());
        assert!(a.runner_busy && a.store_error.is_empty());
        assert_eq!(a.runs[0].qualification, None);
        let unreadable = a.runs[1].qualification.as_ref().unwrap();
        assert_eq!(
            (unreadable.verdict.as_str(), unreadable.error.as_str()),
            ("UNREADABLE", "QUALIFICATION_UNREADABLE: FRAME_CRC")
        );
        let (opt, bt) = (&a.jobs[0], &a.jobs[1]);
        assert_eq!(
            (opt.state.as_str(), opt.started_unix_ms),
            ("RUNNING", Some(1_760_000_001_000))
        );
        assert!(opt.result.is_none() && opt.grid.as_ref().unwrap().save_profile);
        assert_eq!(
            opt.params
                .as_ref()
                .unwrap()
                .min_profit_usd_micro_signed
                .as_ref()
                .unwrap()
                .value,
            "1500000"
        );
        let Some(R::Backtest(r)) = &bt.result else {
            panic!("{:?}", bt.result)
        };
        assert_eq!(
            (bt.state.as_str(), r.liquidations, r.rows),
            ("COMPLETED", 3, 1)
        );
        let s = &r.counterfactual.as_ref().unwrap().scenarios;
        assert_eq!(
            s[0].win_net_usd_micro_signed.as_ref().unwrap().value,
            i128::MIN.to_string()
        );
        assert_eq!(
            s[0].failed_cost_usd_micro_sum.as_ref().unwrap().value,
            u128::MAX.to_string()
        );
        assert_eq!(s[0].net_usd_micro_signed, None, "unknown net stays unset");
        assert_eq!(s[1].net_usd_micro_signed.as_ref().unwrap().value, "-2000");
        assert_eq!(bt.finished_unix_ms, None);
    }

    /// The analytics topic is declared only by a process that publishes a project's store.
    #[test]
    fn b03_hello_declares_the_analytics_topic_only_with_a_project() {
        let topics = |analytics| match EngineFrame::decode(
            &hello_frame("s", 1, true, false, analytics)[4..],
        )
        .unwrap()
        .msg
        {
            Some(engine_frame::Msg::Hello(h)) => h.topics,
            other => panic!("{other:?}"),
        };
        let analytics = TopicId::Project0AnalyticsJobs as i32;
        assert!(!topics(false).contains(&analytics));
        assert!(topics(true).contains(&analytics));
    }

    /// A01: the ingress session reconnects reach the Capture transport status; before a health
    /// report the field stays unset.
    #[test]
    fn a01_capture_transport_carries_the_session_reconnects() {
        let t = capture_state(&golden_snapshot()).transport.unwrap();
        assert_eq!(t.reconnects, Some(1));
        let state_only = transport_status(
            None,
            Some(&TransportStateSnapshot {
                state: "RECOVERING".into(),
                ..Default::default()
            }),
        );
        assert_eq!(state_only.unwrap().reconnects, None);
    }

    /// P0-PROD-CTRL-04/08: the owner projection reaches the wire unchanged; unknown finance
    /// totals stay unset (never 0) and both wallet topics carry the same snapshot id.
    #[test]
    fn owner_projection_keeps_unknown_unset_and_one_snapshot_id() {
        let owner = golden_owner();
        let f = finance_summary(&owner);
        assert_eq!(f.total_capital_usd_micro, None);
        assert_eq!(f.available_capital_usd_micro, None);
        assert_eq!(f.withdrawable_profit_usd_micro, None);
        assert_eq!(f.reserved_capital_usd_micro, Some(0));
        assert_eq!(
            f.finance_snapshot_id,
            wallets_state(&owner).finance_snapshot_id
        );
        let l = liquidator_state(&owner);
        assert_eq!(l.engine.as_ref().unwrap().reconnects, 1);
        assert_eq!(
            l.engine
                .as_ref()
                .unwrap()
                .counters
                .as_ref()
                .unwrap()
                .same_slot_unversioned_refused,
            2
        );
        assert_eq!(l.pipeline.as_ref().unwrap().simulation_p95_ns, None);
        // §6: the ProgramData verdict reaches the wire with both identities; before the owner
        // read its ProgramData there is none.
        let e = l.engine.as_ref().unwrap();
        let pin = e.program_pin.as_ref().unwrap();
        assert!(e.program_upgrade_latched);
        assert_eq!(
            (
                pin.verdict.as_str(),
                pin.reference_source_tag.as_str(),
                pin.deployment_slot
            ),
            ("NOT_REFERENCE", "mrgn-0.1.11-rc1", 371_000_000)
        );
        let mut unread = owner.clone();
        unread.engine.program_pin = None;
        assert_eq!(liquidator_state(&unread).engine.unwrap().program_pin, None);
        // B01: the rebalancing plan reaches the wire; OFF has none.
        let r = l.rebalance.as_ref().unwrap();
        assert_eq!(
            (
                r.mode.as_str(),
                r.trades[0].amount_in,
                r.blocked[0].reason.as_str()
            ),
            ("OBSERVE", 3_000_000_000, "INVENTORY_BALANCE_UNKNOWN")
        );
        let mut off = owner.clone();
        off.rebalance = None;
        assert_eq!(liquidator_state(&off).rebalance, None);
        // S1-02: every unit's applied control, the last decision and the counters.
        assert_eq!(l.sources.len(), 2);
        assert!(l.sources[1].canonical && l.sources[1].mode == "active");
        let d = l.last_source_decision.as_ref().unwrap();
        assert_eq!((d.seq, d.applied), (3, false));
        // CTRL-03: the version a source change states as its CAS.
        assert_eq!(l.source_config_version, Some(3));
        assert_eq!(l.source_stats.as_ref().unwrap().refused, 1);
        let mut unread = owner.clone();
        unread.wallets.journal_read = false;
        let w = wallets_state(&unread);
        assert_eq!(
            (w.fee_lamports, w.open_inventory_mark_usd_micro),
            (None, None)
        );
        assert_eq!((w.attempts, w.landed_attempts), (None, None));
    }

    #[test]
    fn unreported_blocks_are_not_invented() {
        let s = StatusSnapshot::default();
        let c = capture_state(&s);
        assert!(c.run.is_none() && c.phase.is_none() && c.transport.is_none());
        assert!(c.market.is_none() && c.assessment.is_none() && c.writer.is_none());
        assert!(c.provider.is_none());
        assert!(system_status(&s).is_none());
    }

    /// F-01b: every budget value reaches the wire unchanged; an unrepresentable cooldown stays
    /// unset (never 0) and the halt code is carried.
    #[test]
    fn provider_budget_carries_every_budget_field() {
        let b = provider_budget(&provider_sample());
        assert_eq!(b.sampled_unix_ms, 1_760_000_002_000);
        assert_eq!(b.scope, "PROCESS_SOLANA_PROVIDER_ACCOUNT");
        assert_eq!(
            (b.requests, b.http_429_count, b.in_flight, b.queued),
            (21, 3, 1, 2)
        );
        assert_eq!((b.max_rps, b.max_concurrency, b.burst), (10, 4, 5));
        assert_eq!((b.effective_rps, b.bytes), (2.5, u64::MAX));
        assert_eq!(b.cooldown_cause, "RETRY_AFTER_UNREPRESENTABLE");
        assert_eq!(b.cooldown_remaining_ms, None);
        assert_eq!(b.halt_code.as_deref(), Some("PROVIDER_BUDGET_POISONED"));
        assert!(!b.quota_qualified);
    }

    #[test]
    fn writer_lane_carries_every_writer_field() {
        let mut sample = StorageSample {
            at_unix_ms: 1_700_000_000_000,
            status: Default::default(),
        };
        let w = &mut sample.status;
        w.queue_bytes = 1;
        w.queue_peak_bytes = 2;
        w.admission_limit_bytes = 3;
        w.max_bytes = 4;
        w.free_budget_bytes = 5;
        w.ingress_bytes_per_s = 6.5;
        w.write_bytes_per_s = 7.5;
        w.lag_frames = 8;
        w.budget_exhaustion_s = Some(9.0);
        w.disk_free_bytes = Some(10);
        w.disk_reserve_bytes = 11;
        w.disk_exhaustion_s = Some(12.0);
        let l = writer_lane(&sample);
        assert_eq!(l.sampled_unix_ms, 1_700_000_000_000);
        assert_eq!(
            (l.queue_bytes, l.queue_peak_bytes, l.admission_limit_bytes),
            (1, 2, 3)
        );
        assert_eq!((l.max_bytes, l.free_budget_bytes, l.lag_frames), (4, 5, 8));
        assert_eq!((l.ingress_bytes_per_s, l.write_bytes_per_s), (6.5, 7.5));
        assert_eq!(
            (l.budget_exhaustion_s, l.disk_free_bytes),
            (Some(9.0), Some(10))
        );
        assert_eq!(
            (l.disk_reserve_bytes, l.disk_exhaustion_s),
            (11, Some(12.0))
        );
    }

    #[test]
    fn every_market_counter_is_sent_by_engine_name() {
        let mut m = MarketSnapshot::default();
        m.stats.gap_count = 3;
        m.stats.transport_http_429 = 7;
        let c = market_counters(&m);
        assert_eq!(c.counters.len(), m.stats.counters().len());
        let get = |n: &str| c.counters.iter().find(|x| x.name == n).map(|x| x.value);
        assert_eq!(
            (get("gap_count"), get("transport_http_429")),
            (Some(3), Some(7))
        );
    }

    #[test]
    fn only_changed_topics_are_sent() {
        let mut s = golden_snapshot();
        let counters = Counters::default();
        let mut sent = Sent::default();
        let mut frames = 0;
        let mut count = |_: &[u8]| -> Result<(), String> {
            frames += 1;
            Ok(())
        };
        assert!(send_changes(&s, &mut sent, "t", 1, &counters, &mut count).unwrap());
        s.capture_version += 1;
        assert!(send_changes(&s, &mut sent, "t", 1, &counters, &mut count).unwrap());
        assert!(!send_changes(&s, &mut sent, "t", 1, &counters, &mut count).unwrap());
        // An owner publication re-sends its three topics only, an analytics one its topic.
        s.owner_version += 1;
        assert!(!send_changes(&s, &mut sent, "t", 1, &counters, &mut count).unwrap());
        s.analytics_version += 1;
        assert!(!send_changes(&s, &mut sent, "t", 1, &counters, &mut count).unwrap());
        assert_eq!(
            frames, 11,
            "system once, the three owner topics twice, analytics twice, capture twice"
        );
    }

    #[test]
    fn worst_case_frames_stay_far_below_the_gateway_limit() {
        // Gateway: Limits::max_state_full_bytes = 262_144.
        let limits = Limits::default();
        let base = operational_snapshot_of(&limits, None);
        let effective = EffectiveConfig {
            schema: "Project0/OperationalConfig/2".into(),
            profile: Profile::Custom,
            limits: limits.clone(),
            rpc_endpoint_count: 4,
            wss_endpoint_count: 4,
            fallback: true,
            adaptive: true,
            duration: "24h".into(),
            verification: "crosscheck".into(),
            diagnostics: true,
            fe_mode: "full".into(),
            fe_targets: vec!["A45dVtuyLccxs1xpbVJsTNh7AdtAotAZi4UxEcyMQDMS".into(); 100],
            fe_group: "4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8".into(),
            sources: base
                .parameters
                .iter()
                .map(|p| (p.field.to_owned(), "config_file".to_owned()))
                .collect(),
            provider_capacity_qualified: false,
            free_only: true,
            live_authorized: false,
            submission_authorized: false,
            threshold_trigger: true,
            c0_census: true,
            source_geyser: "observe".into(),
            source_geyser_provider: "p".repeat(64),
        };
        let mut s = golden_snapshot();
        s.operational = Some(Arc::new(operational_snapshot_of(&limits, Some(&effective))));
        s.assessment = Some(Arc::new(AssessmentSnapshot {
            verdict: "MARKET_DATA_INCOMPLETE",
            incomplete_reasons: vec!["TRUNCATED_LIQUIDATION_MISMATCH"; 13],
            truncated_unqualified_reasons: vec![("PHASE_BUDGET_EXHAUSTED", u64::MAX); 10],
            top_receivers: (0..10)
                .map(|i| ReceiverRank {
                    receiver: format!("{i:0>44}"),
                    liquidation_events: u64::MAX,
                    gross_edge_total_usd: f64::MAX,
                })
                .collect(),
            deployment_error: Some("POST_CAPTURE_DEPLOYMENT_VERIFICATION_NOT_RUN"),
            ..Default::default()
        }));
        let system = system_status(&s).unwrap().encoded_len();
        let capture = capture_state(&s).encoded_len();
        assert!(system < 32 * 1024, "system status {system} bytes");
        assert!(capture < 8 * 1024, "capture state {capture} bytes");
        // B03/U-9: the most runs and jobs listed, every job an optimization with the most
        // scenarios in both scores.
        use darkbot_runner::analytics::backtest::{Params, Score};
        use darkbot_runner::analytics::jobs::{JobOutcome, JobResult, JobState};
        use darkbot_runner::status_tap::defaults;
        let mut a = golden_analytics();
        let template = a.runs[1].clone();
        a.runs = (0..defaults::ANALYTICS_RUNS_MAX)
            .map(|i| darkbot_runner::analytics::jobs::RunEntry {
                run_id: format!("{i:0>64}"),
                ..template.clone()
            })
            .collect();
        let score = || Score {
            counterfactual: golden_counterfactual(32),
            objective_bps: Some(i128::MIN),
        };
        let mut job = a.jobs[0].clone();
        job.state = JobState::Done(Box::new(JobOutcome {
            job_id: "j".repeat(64),
            state: "COMPLETED".into(),
            code: "C".repeat(256),
            result: Some(JobResult::Optimize {
                result_id: "r".repeat(64),
                capture_run_id: "c".repeat(64),
                points: u64::MAX,
                chosen: Some(Params {
                    min_profit_usd_micro: i128::MAX,
                    capital_usd_micro: vec![u64::MAX; 32],
                    funding: vec!["JIT_EXACT_IN".into(); 4],
                }),
                chosen_holdout: Some(Box::new(score())),
                baseline_holdout: Box::new(score()),
                result_sha256: [0xFF; 32],
                profile: Some("p".repeat(80)),
            }),
            worker_pid: u32::MAX,
            started_unix_ms: Some(u64::MAX),
            finished_unix_ms: Some(u64::MAX),
        }));
        a.jobs = vec![job; defaults::ANALYTICS_JOBS_MAX as usize];
        let analytics = analytics::analytics_jobs(&a).encoded_len();
        // At most three quarters of the gateway's 256 KiB STATE frame.
        assert!(analytics < 192 * 1024, "analytics jobs {analytics} bytes");
    }
}

#[cfg(test)]
mod schema_id_guard {
    #[test]
    fn schema_id_is_a_bare_sha256() {
        assert_eq!(
            super::SCHEMA_ID.len(),
            64,
            "schema_id.txt must hold exactly the id, no newline"
        );
        assert!(super::SCHEMA_ID.bytes().all(|b| b.is_ascii_hexdigit()));
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;

    /// R22: shutdown releases our paths exactly once. A second shutdown of the same handle
    /// (`stop` followed by `Drop`) after a successor started leaves the successor's socket and
    /// locks in place.
    #[test]
    fn r22_second_shutdown_never_removes_a_successors_files() {
        let dir = std::env::temp_dir().join(format!("p0r-own-{}-{}", std::process::id(), now_ns()));
        fs::create_dir(&dir).unwrap();
        let socket = PathBuf::from(format!(
            "/tmp/p0r-own-{}-{:x}.sock",
            std::process::id(),
            now_ns() & 0xffff_ffff
        ));
        let cfg = ReporterConfig::new(socket.clone(), dir.join("generation"), "TEST_ONLY".into());
        let mut first = start(cfg.clone(), StatusTap::new()).unwrap();
        first.shutdown();
        let second = start(cfg.clone(), StatusTap::new()).unwrap();
        first.shutdown();
        drop(first);
        assert!(socket.exists(), "successor socket kept");
        assert!(dir.join("generation.generation.lock").exists());
        assert!(UnixStream::connect(&socket).is_ok());
        second.stop();
        assert!(!socket.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod transport_age_tests {
    use super::*;

    /// R19: a capture state sent later (e.g. after another block changed) carries the frame age
    /// at its send time; a frozen transport therefore ages instead of looking fresh.
    #[test]
    fn r19_frame_age_is_advanced_to_the_send_time() {
        let tap = StatusTap::new();
        tap.publish_transport(TransportSnapshot {
            connected: true,
            last_frame_age_ns: Some(1_000_000),
            ..Default::default()
        });
        let mut reader = tap.reader();
        let first = reader.wait_changed(Duration::from_millis(5)).unwrap();
        let sent_now = capture_state(&first)
            .transport
            .unwrap()
            .last_frame_age_ns
            .unwrap();
        assert!(sent_now >= 1_000_000);
        std::thread::sleep(Duration::from_millis(300));
        let later = capture_state(&first)
            .transport
            .unwrap()
            .last_frame_age_ns
            .unwrap();
        assert!(later >= 301_000_000, "{later}");
        // No published age stays unknown, never 0.
        tap.publish_transport(TransportSnapshot::default());
        let unknown = reader.wait_changed(Duration::from_millis(5)).unwrap();
        assert_eq!(
            capture_state(&unknown).transport.unwrap().last_frame_age_ns,
            None
        );
    }
}

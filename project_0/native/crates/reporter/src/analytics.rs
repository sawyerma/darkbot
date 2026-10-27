//! B03/U-9: the analytics projection (`analytics::jobs::AnalyticsSnapshot`) as
//! `darkbot.project0.v1.AnalyticsJobs` (topic project0.analytics.jobs). Values are copied as the
//! job store holds them; i128/u128 amounts stay exact decimals, unknown stays unset.
use crate::proto::control::{SignedDecimal, UnsignedDecimal};
use crate::proto::project0::{
    analytics_job, AnalyticsJob, AnalyticsJobs, AnalyticsRun, AutostartGrant, BacktestResult,
    BootFlow, BootStep, CapitalScenario, FundingSet, JobGrid, JobParams, OptimizeResult,
    OptimizeScore, PostCaptureResult, PostCaptureSpec, RunQualification, ShadowCounterfactual,
    UntilStopped, UntilStoppedQualification, UntilStoppedRun,
};
use darkbot_runner::analytics::backtest::{Counterfactual, Params, Score};
use darkbot_runner::analytics::jobs::{
    AnalyticsSnapshot, JobResult, JobSpec, JobState, JobStatus, RunEntry,
};

fn signed(v: i128) -> SignedDecimal {
    SignedDecimal {
        value: v.to_string(),
    }
}

fn params(p: &Params) -> JobParams {
    JobParams {
        min_profit_usd_micro_signed: Some(signed(p.min_profit_usd_micro)),
        capital_usd_micro: p.capital_usd_micro.clone(),
        funding: p.funding.clone(),
    }
}

fn counterfactual(c: &Counterfactual) -> ShadowCounterfactual {
    ShadowCounterfactual {
        matched: c.matched,
        unknown: c.unknown,
        funding_excluded: c.funding_excluded,
        below_minimum: c.below_minimum,
        wins: c.wins,
        lost_races: c.lost_races,
        no_chain_event: c.no_chain_event,
        scenarios: c
            .scenarios
            .iter()
            .map(|s| CapitalScenario {
                capital_usd_micro: s.capital_usd_micro,
                wins: s.wins,
                win_net_usd_micro_signed: Some(signed(s.win_net_usd_micro)),
                failed_attempts: s.failed_attempts,
                failed_cost_usd_micro_sum: Some(UnsignedDecimal {
                    value: s.failed_cost_usd_micro.to_string(),
                }),
                failed_cost_unknown: s.failed_cost_unknown,
                over_budget: s.over_budget,
                net_usd_micro_signed: s.net_usd_micro.map(signed),
            })
            .collect(),
        landing: c.landing.clone(),
        safety: c.safety.clone(),
    }
}

fn score(s: &Score) -> OptimizeScore {
    OptimizeScore {
        counterfactual: Some(counterfactual(&s.counterfactual)),
        objective_bps_signed: s.objective_bps.map(signed),
    }
}

fn run(r: &RunEntry) -> AnalyticsRun {
    AnalyticsRun {
        run_id: r.run_id.clone(),
        root: r.root.into(),
        layout: r.layout.into(),
        capture_complete: r.capture_complete,
        qualification: r.qualification.as_ref().map(|q| match q {
            Ok(q) => RunQualification {
                verdict: q.verdict.clone(),
                verification: q.verification.clone(),
                failed: q.failed.clone(),
                qualification_id: q.qualification_id.clone(),
                error: String::new(),
            },
            Err(e) => RunQualification {
                verdict: "UNREADABLE".into(),
                error: e.clone(),
                ..Default::default()
            },
        }),
        modified_unix_ms: r.modified_unix_ms,
        running: r.running,
    }
}

fn job(s: &JobStatus) -> AnalyticsJob {
    let r = &s.request;
    let (state, code, started, finished, result) = match &s.state {
        JobState::Done(o) => (
            o.state.clone(),
            o.code.clone(),
            o.started_unix_ms,
            o.finished_unix_ms,
            o.result.as_ref().map(|res| match res {
                JobResult::Backtest {
                    result_id,
                    capture_run_id,
                    summary,
                } => analytics_job::Result::Backtest(BacktestResult {
                    result_id: result_id.clone(),
                    capture_run_id: capture_run_id.clone(),
                    rows: summary.rows,
                    attempts: summary.attempts,
                    liquidations: summary.observed.liquidations,
                    counterfactual: Some(counterfactual(&summary.counterfactual)),
                    realized: summary.realized.clone(),
                    result_sha256: summary.result_sha256.to_vec().into(),
                }),
                JobResult::Optimize {
                    result_id,
                    capture_run_id,
                    points,
                    chosen,
                    chosen_holdout,
                    baseline_holdout,
                    result_sha256,
                    profile,
                } => analytics_job::Result::Optimize(OptimizeResult {
                    result_id: result_id.clone(),
                    capture_run_id: capture_run_id.clone(),
                    points: *points,
                    chosen: chosen.as_ref().map(params),
                    chosen_holdout: chosen_holdout.as_deref().map(score),
                    baseline_holdout: Some(score(baseline_holdout)),
                    result_sha256: result_sha256.to_vec().into(),
                    profile: profile.clone().unwrap_or_default(),
                }),
                JobResult::PostCapture {
                    verification_ok,
                    report_ok,
                    verdict,
                } => analytics_job::Result::PostCapture(PostCaptureResult {
                    verification_ok: *verification_ok,
                    report_ok: *report_ok,
                    verdict: verdict.clone().unwrap_or_default(),
                }),
            }),
        ),
        JobState::Running {
            started_unix_ms, ..
        } => (
            s.state.name().into(),
            String::new(),
            *started_unix_ms,
            None,
            None,
        ),
        other => (other.name().into(), String::new(), None, None, None),
    };
    let (job_params, grid, post_capture_spec) = match &r.spec {
        JobSpec::Backtest { params: p } => (Some(params(p)), None, None),
        JobSpec::Optimize {
            baseline,
            space,
            holdout_bps,
            max_points,
            save_profile,
        } => (
            Some(params(baseline)),
            Some(JobGrid {
                min_profit_usd_micro_signed: space
                    .min_profit_usd_micro
                    .iter()
                    .map(|v| signed(*v))
                    .collect(),
                funding_sets: space
                    .funding
                    .iter()
                    .map(|f| FundingSet { classes: f.clone() })
                    .collect(),
                holdout_bps: *holdout_bps,
                max_points: *max_points,
                save_profile: *save_profile,
            }),
            None,
        ),
        JobSpec::PostCapture {
            chain_verify,
            offline,
            resume,
            provider_evidence,
        } => (
            None,
            None,
            Some(PostCaptureSpec {
                chain_verify: chain_verify.clone(),
                offline: *offline,
                resume: *resume,
                provider_evidence: provider_evidence.is_some(),
            }),
        ),
    };
    AnalyticsJob {
        job_id: r.job_id.clone(),
        kind: r.spec.kind().into(),
        run: r.run.clone(),
        state,
        code,
        requested_unix_ms: r.requested_unix_ms,
        started_unix_ms: started,
        finished_unix_ms: finished,
        max_seconds: r.max_seconds,
        max_ram_bytes: r.max_ram_bytes,
        params: job_params,
        grid,
        post_capture_spec,
        verification_result: r.verification_result.clone().unwrap_or_default(),
        result,
    }
}

pub fn analytics_jobs(s: &AnalyticsSnapshot) -> AnalyticsJobs {
    AnalyticsJobs {
        published_unix_ms: s.at_unix_ms,
        runner_busy: s.runner_busy,
        runner_error: s.runner_error.clone().unwrap_or_default(),
        runs: s.runs.iter().map(run).collect(),
        jobs: s.jobs.iter().map(job).collect(),
        store_error: s.error.clone().unwrap_or_default(),
        boot: s.boot.as_ref().map(boot),
        boot_error: s.boot_error.clone().unwrap_or_default(),
        until_stopped: Some(until_stopped(&s.until_stopped)),
    }
}

/// DEC-16: unbounded runs, values copied as the stores hold them.
fn until_stopped(
    v: &darkbot_runner::orchestration::until_stopped_view::UntilStoppedView,
) -> UntilStopped {
    use darkbot_runner::orchestration::until_stopped_view::GrantView;
    let grant = |g: &GrantView| AutostartGrant {
        digest: g.digest.clone(),
        mode: g.grant.binding.mode.clone(),
        config_sha256: g.grant.binding.identity.runtime_config_sha256.clone(),
        binary_sha256: g.grant.binding.binary_sha256.clone(),
        duration: g.grant.duration.clone(),
        checkpoint_seconds: g.grant.binding.checkpoint_seconds,
        minimum_runway_seconds: g.grant.binding.minimum_runway_seconds,
        qualification_id: g.grant.qualification_id.clone(),
        provider_evidence_id: g.grant.provider_evidence_id.clone(),
        genesis_hash: g.grant.genesis_hash.clone(),
        capture_source: g.grant.capture_source.clone(),
        rpc_rps: g.grant.budget.rpc_rps,
        max_requests: g.grant.budget.max_requests,
        max_total_mib: g.grant.budget.max_total_mib,
        max_runtime_seconds: g.grant.max_runtime_seconds,
        expires_unix_s: g.grant.expires_unix_s,
        recorded_unix_s: g.recorded_unix_s,
    };
    UntilStopped {
        autostart: v.autostart.as_ref().map(grant),
        preview: v.preview.as_ref().map(grant),
        qualification: v.qualification.as_ref().map(|q| UntilStoppedQualification {
            id: q.id.clone(),
            mode: q.qualification.binding.mode.clone(),
            checkpoint_seconds: q.qualification.binding.checkpoint_seconds,
            minimum_runway_seconds: q.qualification.binding.minimum_runway_seconds,
            complete: q.open.is_empty(),
            open: q.open.clone(),
            evidence_id: q.qualification.evidence_id.clone(),
            evidence_valid_until_unix_s: q.qualification.evidence_valid_until_unix_s,
            qualified_at_unix_s: q.qualification.qualified_at_unix_s,
        }),
        run: v.run.as_ref().map(|r| UntilStoppedRun {
            run_id: r.run_id.clone(),
            qualification_id: r.record.qualification_id.clone(),
            started_unix_s: r.record.started_unix_s,
            checkpoint_seconds: r.record.binding.checkpoint_seconds,
            windows: r.windows,
            last_verdict: r.last.as_ref().map(|w| w.verdict.clone()),
            last_detail: r
                .last
                .as_ref()
                .map(|w| w.detail.clone())
                .unwrap_or_default(),
            last_ended_unix_s: r.last.as_ref().map(|w| w.ended_unix_s),
            halted: r.halted.clone(),
            sealed: r.sealed,
            complete: r.complete,
        }),
        errors: v.errors.clone(),
    }
}

/// DEC-07: the boot controller's newest flow, values copied as the controller holds them.
fn boot(v: &darkbot_runner::orchestration::boot_control::BootView) -> BootFlow {
    let step = |s: &darkbot_runner::orchestration::boot_control::StepView| BootStep {
        state: s.state.clone(),
        code: s.code.clone(),
        run_id: s.run_id.clone(),
        started_unix_ms: s.started_unix_ms,
        finished_unix_ms: s.finished_unix_ms,
    };
    BootFlow {
        flow_id: v.flow_id.clone(),
        mode: v.mode.clone(),
        duration: v.duration.clone(),
        created_unix_ms: v.created_unix_ms,
        offline_verdict: v.offline_verdict.into(),
        offline_token: v.offline_token.clone(),
        offline_blocked: v.offline_blocked.clone(),
        ttl_seconds: v.ttl_seconds,
        ttl_source: v.ttl_source.clone(),
        network: Some(step(&v.network)),
        network_verdict: v.network_verdict.map(Into::into),
        network_token: v.network_token.clone(),
        network_blocked: v.network_blocked.clone(),
        observed_unix_s: v.observed_unix_s,
        expires_unix_s: v.expires_unix_s,
        start: Some(step(&v.start)),
    }
}

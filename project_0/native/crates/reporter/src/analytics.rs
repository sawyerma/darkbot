//! B03/U-9: the analytics projection (`analytics::jobs::AnalyticsSnapshot`) as
//! `darkbot.project0.v1.AnalyticsJobs` (topic project0.analytics.jobs). Values are copied as the
//! job store holds them; i128/u128 amounts stay exact decimals, unknown stays unset.
use crate::proto::control::{SignedDecimal, UnsignedDecimal};
use crate::proto::project0::{
    analytics_job, AnalyticsJob, AnalyticsJobs, AnalyticsRun, BacktestResult, CapitalScenario,
    FundingSet, JobGrid, JobParams, OptimizeResult, OptimizeScore, PostCaptureResult,
    PostCaptureSpec, RunQualification, ShadowCounterfactual,
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
        runs: s.runs.iter().map(run).collect(),
        jobs: s.jobs.iter().map(job).collect(),
        store_error: s.error.clone().unwrap_or_default(),
    }
}

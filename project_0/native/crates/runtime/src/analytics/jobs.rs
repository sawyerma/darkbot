//! B03/U-9 (STARTUP §4 B/C/E, P0-PROD-CTRL-03): the native analytics job controller. The
//! terminal (`p0ctl job …`, `p0ctl backtest|optimize`) and the GUI (gateway → `darkbotctl p0 job
//! …` → `p0ctl job …`) reach the same actions here; the job store is a job's only authority.
//!
//! * `<project>/data/analytics-jobs/<job_id>/request.p0j`: the typed request (immutable).
//! * `worker.lock`: held (flock) by the one worker process running the job, its pid inside.
//! * `outcome.p0j`: the terminal outcome (immutable) — COMPLETED with the result identity,
//!   FAILED with the refusal code, CANCELLED after `job cancel`.
//!
//! A job without an outcome is RUNNING while its worker holds the lock, QUEUED before a worker
//! took it, and INTERRUPTED (its end unknown, never a result) once a worker had started and is
//! gone. A project runs one job at a time (`runner.lock`); a start while one runs is refused.
use super::backtest::{self, Backtest, BacktestSummary, OptimizeResult, Params, Score, Space};
use super::journal;
use super::payload::Payload;
use crate::run_layout::{find_run, native_layout};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

/// The job store of a project.
pub const JOB_DIR: &str = "data/analytics-jobs";
const REQUEST: &str = "request.p0j";
const OUTCOME: &str = "outcome.p0j";
const WORKER_LOCK: &str = "worker.lock";
const RUNNER_LOCK: &str = "runner.lock";
/// The longest job id (the gateway's UUIDs and `p0ctl`'s generated ids fit).
const MAX_ID: usize = 64;

pub const COMPLETED: &str = "COMPLETED";
pub const FAILED: &str = "FAILED";
pub const CANCELLED: &str = "CANCELLED";

/// What a job computes (the parameters `p0ctl backtest|optimize` take).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobSpec {
    Backtest {
        params: Params,
    },
    Optimize {
        baseline: Params,
        space: Space,
        holdout_bps: u64,
        max_points: u64,
        /// Save the chosen point as an offline Shadow candidate profile (never a LIVE setting).
        save_profile: bool,
    },
    /// A08/F-02: the production chain after a capture (`post_capture`: the verification, the
    /// native report, the master qualification). The job's budget is the verification's time
    /// and RAM budget; its other bounds are the registry's (`verify_*`).
    PostCapture {
        /// `off` (identities only, no chain read) or `strict`.
        chain_verify: String,
        /// Strict from the verification cache only (no network).
        offline: bool,
        resume: bool,
        /// The operator's provider evidence for an online strict verification (terminal only).
        provider_evidence: Option<String>,
    },
}

impl JobSpec {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Backtest { .. } => "BACKTEST",
            Self::Optimize { .. } => "OPTIMIZE",
            Self::PostCapture { .. } => "POST_CAPTURE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRequest {
    pub job_id: String,
    /// The run as given (an id under the project's run roots, or a run directory).
    pub run: String,
    pub verification_result: Option<String>,
    pub max_seconds: u64,
    pub max_ram_bytes: u64,
    pub spec: JobSpec,
    /// `None`: the wall clock was not readable (never 0).
    pub requested_unix_ms: Option<u64>,
}
impl Payload for JobRequest {
    const KIND: u16 = journal::kind::ANALYTICS_JOB_REQUEST;
}

/// What a completed job produced: the result journal's identity and its headline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobResult {
    Backtest {
        /// The content-addressed result id (`reports/backtests/<id>.p0j` of the run).
        result_id: String,
        capture_run_id: String,
        summary: Box<BacktestSummary>,
    },
    Optimize {
        result_id: String,
        capture_run_id: String,
        points: u64,
        chosen: Option<Params>,
        chosen_holdout: Option<Box<Score>>,
        baseline_holdout: Box<Score>,
        result_sha256: [u8; 32],
        /// The saved candidate profile's file name under `data/profiles/`.
        profile: Option<String>,
    },
    /// The chain ran: each stage's outcome; the qualification record is the run's own.
    PostCapture {
        /// `off` (NOT_RUN recorded) or a CANONICAL_VERIFIED strict result.
        verification_ok: bool,
        report_ok: bool,
        /// The master verdict; `None`: the qualification could not run.
        verdict: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobOutcome {
    pub job_id: String,
    /// COMPLETED, FAILED or CANCELLED.
    pub state: String,
    /// The refusal code of a failed or cancelled job; empty when completed.
    pub code: String,
    pub result: Option<JobResult>,
    pub worker_pid: u32,
    pub started_unix_ms: Option<u64>,
    pub finished_unix_ms: Option<u64>,
}
impl Payload for JobOutcome {
    const KIND: u16 = journal::kind::ANALYTICS_JOB_OUTCOME;
}

/// A job as the store shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobState {
    /// Requested; no worker has taken it yet.
    Queued,
    Running {
        pid: u32,
        started_unix_ms: Option<u64>,
    },
    /// A worker had started and is gone without an outcome: the end is unknown.
    Interrupted,
    Done(Box<JobOutcome>),
}

impl JobState {
    pub fn name(&self) -> &str {
        match self {
            Self::Queued => "QUEUED",
            Self::Running { .. } => "RUNNING",
            Self::Interrupted => "INTERRUPTED",
            Self::Done(o) => &o.state,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobStatus {
    pub request: JobRequest,
    pub state: JobState,
}

/// A job id: 1..=64 of `[A-Za-z0-9-]` (never a path).
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn store(project: &Path) -> PathBuf {
    project.join(JOB_DIR)
}

fn job_dir(project: &Path, job_id: &str) -> Result<PathBuf, String> {
    if !valid_id(job_id) {
        return Err("JOB_ID_INVALID".into());
    }
    Ok(store(project).join(job_id))
}

fn io(e: impl std::fmt::Display) -> String {
    format!("JOB_STORE_IO: {e}")
}

fn open_lock(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

/// flock without blocking: `Ok(false)` when another holder has it.
fn flock_nb(file: &File, op: libc::c_int) -> std::io::Result<bool> {
    // SAFETY: flock on an owned, open file descriptor.
    if unsafe { libc::flock(file.as_raw_fd(), op | libc::LOCK_NB) } == 0 {
        return Ok(true);
    }
    let e = std::io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::EWOULDBLOCK) {
        Ok(false)
    } else {
        Err(e)
    }
}

/// Whether a process holds the exclusive lock of `path` (a missing file: nobody). A probe takes
/// the shared lock for an instant only; a worker takes its job lock blocking, so a probe never
/// makes it fail.
fn held(path: &Path) -> Result<bool, String> {
    match File::open(path) {
        Ok(f) => flock_nb(&f, libc::LOCK_SH).map(|free| !free).map_err(io),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io(e)),
    }
}

/// The pid and start time a worker wrote into its lock (`pid=… started_unix_ms=…`).
fn worker_record(path: &Path) -> (u32, Option<u64>) {
    let mut text = String::new();
    if let Ok(mut f) = File::open(path) {
        let _ = f.read_to_string(&mut text);
    }
    let field = |name: &str| {
        text.split_whitespace()
            .find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))
            .and_then(|v| v.parse::<u64>().ok())
    };
    (
        field("pid")
            .and_then(|p| u32::try_from(p).ok())
            .unwrap_or(0),
        field("started_unix_ms"),
    )
}

/// The state of one job.
pub fn status(project: &Path, job_id: &str) -> Result<JobStatus, String> {
    let dir = job_dir(project, job_id)?;
    let request: JobRequest = journal::read_typed_value(&dir.join(REQUEST)).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "JOB_UNKNOWN".to_string()
        } else {
            format!("JOB_REQUEST_INVALID: {e}")
        }
    })?;
    let outcome = dir.join(OUTCOME);
    let state = if outcome.exists() {
        JobState::Done(Box::new(
            journal::read_typed_value(&outcome).map_err(|e| format!("JOB_OUTCOME_INVALID: {e}"))?,
        ))
    } else {
        let lock = dir.join(WORKER_LOCK);
        let (pid, started_unix_ms) = worker_record(&lock);
        if held(&lock)? {
            JobState::Running {
                pid,
                started_unix_ms,
            }
        } else if pid > 0 {
            // A worker recorded itself and is gone without an outcome.
            JobState::Interrupted
        } else {
            JobState::Queued
        }
    };
    Ok(JobStatus { request, state })
}

/// The newest `max` jobs of the store, newest request first (ids break ties); unreadable job
/// directories are skipped, never shown as a state.
pub fn list(project: &Path, max: usize) -> Result<Vec<JobStatus>, String> {
    let entries = match std::fs::read_dir(store(project)) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(io(e)),
    };
    let mut jobs: Vec<JobStatus> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|id| valid_id(id))
        .filter_map(|id| status(project, &id).ok())
        .collect();
    jobs.sort_by(|a, b| {
        (b.request.requested_unix_ms, &b.request.job_id)
            .cmp(&(a.request.requested_unix_ms, &a.request.job_id))
    });
    jobs.truncate(max);
    Ok(jobs)
}

/// Checks a request before anything is written: id, run (native layout V2), budget and the
/// parameters of its kind.
fn validate(project: &Path, r: &JobRequest) -> Result<PathBuf, String> {
    if !valid_id(&r.job_id) {
        return Err("JOB_ID_INVALID".into());
    }
    let run = find_run(project, &r.run).ok_or("RUN_NOT_FOUND_OR_AMBIGUOUS")?;
    native_layout(&run)?;
    if r.max_seconds == 0 || r.max_ram_bytes == 0 {
        return Err("BACKTEST_BUDGET_INVALID".into());
    }
    match &r.spec {
        JobSpec::Backtest { params } => params.validate()?,
        JobSpec::Optimize {
            baseline,
            space,
            max_points,
            ..
        } => {
            baseline.validate()?;
            let points = space.points();
            if points.is_empty() || points.len() as u64 > *max_points {
                return Err("OPTIMIZE_GRID_OUT_OF_BOUNDS".into());
            }
            for p in &points {
                p.validate()?;
            }
        }
        JobSpec::PostCapture {
            chain_verify,
            offline,
            provider_evidence,
            ..
        } => {
            if !matches!(chain_verify.as_str(), "off" | "strict") {
                return Err(format!(
                    "CHAIN_VERIFY_{chain_verify}_BLOCKED: a native run verifies strict or off"
                ));
            }
            if chain_verify == "strict" && !offline && provider_evidence.is_none() {
                return Err("STRICT_PROVIDER_EVIDENCE_REQUIRED: online strict verification needs the operator's --provider-evidence (else --offline)".into());
            }
        }
    }
    Ok(run)
}

/// Records a new request (validated; an existing id is refused, never overwritten).
pub fn submit(project: &Path, request: &JobRequest) -> Result<(), String> {
    validate(project, request)?;
    let dir = job_dir(project, &request.job_id)?;
    std::fs::create_dir_all(store(project)).map_err(io)?;
    match std::fs::create_dir(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err("JOB_EXISTS".into()),
        Err(e) => return Err(io(e)),
    }
    journal::write_typed_value(&dir.join(REQUEST), journal::source::BACKTEST, request).map_err(io)
}

/// Whether a job of the project is running now.
pub fn runner_busy(project: &Path) -> Result<bool, String> {
    held(&store(project).join(RUNNER_LOCK))
}

/// `job start`: records the request and starts its detached worker (`worker` is the `p0ctl`
/// binary: `p0ctl job run --id <id>` in its own process group, so neither the caller's end nor
/// its terminal ends the job). Refused while another job runs. The worker's output goes to
/// `worker.log` of the job.
pub fn start(project: &Path, request: &JobRequest, worker: &Path) -> Result<(), String> {
    if runner_busy(project)? {
        return Err("JOB_RUNNER_BUSY: another analytics job is running".into());
    }
    submit(project, request)?;
    let dir = job_dir(project, &request.job_id)?;
    let log = File::create(dir.join("worker.log")).map_err(io)?;
    let spawned = {
        use std::os::unix::process::CommandExt;
        std::process::Command::new(worker)
            .args(["job", "run", "--id", &request.job_id])
            .env("P0_PROJECT_DIR", project)
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone().map_err(io)?)
            .stderr(log)
            .process_group(0)
            .spawn()
    };
    if let Err(e) = spawned {
        // The request stays; its outcome says why it never ran.
        let outcome = JobOutcome {
            job_id: request.job_id.clone(),
            state: FAILED.into(),
            code: format!("JOB_SPAWN_FAILED: {e}"),
            result: None,
            worker_pid: 0,
            started_unix_ms: None,
            finished_unix_ms: crate::status_tap::unix_ms_now(),
        };
        write_outcome(&dir, &outcome)?;
        return Err(outcome.code);
    }
    Ok(())
}

/// `job cancel`: asks the running worker to stop (SIGTERM); it records CANCELLED itself.
pub fn cancel(project: &Path, job_id: &str) -> Result<u32, String> {
    let s = status(project, job_id)?;
    match s.state {
        JobState::Running { .. } if matches!(s.request.spec, JobSpec::PostCapture { .. }) => {
            Err("JOB_NOT_CANCELLABLE: a post-capture verification ends at its own budget".into())
        }
        JobState::Running { pid, .. } if pid > 0 => {
            // SAFETY: kill(2) with SIGTERM to the pid of the job's worker lock holder.
            if unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) } != 0 {
                return Err(format!(
                    "JOB_CANCEL_SIGNAL: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(pid)
        }
        JobState::Running { .. } => {
            Err("JOB_STARTING: the worker has not recorded itself yet; retry".into())
        }
        s => Err(format!("JOB_NOT_RUNNING: {}", s.name())),
    }
}

fn write_outcome(dir: &Path, outcome: &JobOutcome) -> Result<(), String> {
    journal::write_typed_value(&dir.join(OUTCOME), journal::source::BACKTEST, outcome).map_err(io)
}

/// What one execution produced, for the terminal's full rendering.
pub enum Executed {
    Backtest(Box<Backtest>),
    /// The chain's terminal text and stage outcomes.
    PostCapture(super::post_capture::Outcome),
    Optimize {
        result: Box<OptimizeResult>,
        path: PathBuf,
        profile: Option<PathBuf>,
    },
}

impl Executed {
    fn result(&self) -> JobResult {
        match self {
            Self::Backtest(b) => JobResult::Backtest {
                result_id: b.header.job_id.clone(),
                capture_run_id: b.header.capture_run_id.clone(),
                summary: Box::new(b.summary.clone()),
            },
            Self::Optimize {
                result, profile, ..
            } => JobResult::Optimize {
                result_id: result.job_id.clone(),
                capture_run_id: result.capture_run_id.clone(),
                points: result.train.len() as u64,
                chosen: result
                    .chosen
                    .map(|i| result.space.points()[i as usize].clone()),
                chosen_holdout: result.chosen_holdout.clone().map(Box::new),
                baseline_holdout: Box::new(result.baseline_holdout.clone()),
                result_sha256: result.result_sha256,
                profile: profile
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned()),
            },
            Self::PostCapture(o) => JobResult::PostCapture {
                verification_ok: o.verification_ok,
                report_ok: o.report_ok,
                verdict: o.verdict.clone(),
            },
        }
    }
}

/// The computation of a request (the existing backtest and optimizer, no second path).
fn execute(
    project: &Path,
    r: &JobRequest,
    cancelled: Option<fn() -> bool>,
) -> Result<Executed, String> {
    let run = validate(project, r)?;
    let budget = backtest::Budget {
        max_seconds: r.max_seconds,
        max_ram_bytes: r.max_ram_bytes,
        cancelled,
    };
    let wanted = r.verification_result.as_deref();
    match &r.spec {
        JobSpec::Backtest { params } => backtest::backtest(&run, wanted, params, budget)
            .map(|b| Executed::Backtest(Box::new(b))),
        JobSpec::Optimize {
            baseline,
            space,
            holdout_bps,
            max_points,
            save_profile,
        } => {
            let (result, path) = backtest::optimize(
                &run,
                wanted,
                baseline,
                space,
                *holdout_bps,
                *max_points,
                budget,
            )?;
            let profile = match (save_profile, result.chosen) {
                (true, Some(i)) => Some(
                    backtest::Profile {
                        params: space.points()[i as usize].clone(),
                        origin: backtest::ORIGIN_OPTIMIZER.into(),
                        optimize_job_id: Some(result.job_id.clone()),
                    }
                    .save(&project.join(backtest::PROFILE_DIR))?,
                ),
                _ => None,
            };
            Ok(Executed::Optimize {
                result: Box::new(result),
                path,
                profile,
            })
        }
        JobSpec::PostCapture {
            chain_verify,
            offline,
            resume,
            provider_evidence,
        } => {
            let mut args = vec![
                "--verify-seconds".to_string(),
                r.max_seconds.to_string(),
                "--verify-ram-bytes".into(),
                r.max_ram_bytes.to_string(),
            ];
            if *offline {
                args.push("--offline".into());
            }
            if *resume {
                args.push("--resume-verification".into());
            }
            if let Some(file) = provider_evidence {
                args.extend(["--provider-evidence".into(), file.clone()]);
            }
            if let Some(wanted) = &r.verification_result {
                args.extend(["--verification-result".into(), wanted.clone()]);
            }
            super::post_capture::post_capture(&run, chain_verify, &args).map(Executed::PostCapture)
        }
    }
}

/// The worker body (`p0ctl job run`, or a foreground `p0ctl backtest|optimize`): takes the
/// project's runner and the job's worker lock, executes the request once and records the
/// outcome. A job that already has an outcome or a live worker is never run again.
pub fn run(
    project: &Path,
    job_id: &str,
    cancelled: Option<fn() -> bool>,
) -> Result<(JobOutcome, Option<Executed>), String> {
    let dir = job_dir(project, job_id)?;
    let request: JobRequest = journal::read_typed_value(&dir.join(REQUEST))
        .map_err(|e| format!("JOB_REQUEST_INVALID: {e}"))?;
    if dir.join(OUTCOME).exists() {
        return Err("JOB_DONE: the job has an outcome".into());
    }
    let runner = open_lock(&store(project).join(RUNNER_LOCK)).map_err(io)?;
    let finish = |state: &str, code: String, result, started| -> Result<JobOutcome, String> {
        let outcome = JobOutcome {
            job_id: job_id.into(),
            state: state.into(),
            code,
            result,
            worker_pid: std::process::id(),
            started_unix_ms: started,
            finished_unix_ms: crate::status_tap::unix_ms_now(),
        };
        write_outcome(&dir, &outcome)?;
        Ok(outcome)
    };
    if !flock_nb(&runner, libc::LOCK_EX).map_err(io)? {
        let outcome = finish(
            FAILED,
            "JOB_RUNNER_BUSY: another analytics job is running".into(),
            None,
            None,
        )?;
        return Ok((outcome, None));
    }
    let mut worker = open_lock(&dir.join(WORKER_LOCK)).map_err(io)?;
    // Blocking: only an instant status probe can hold it besides a duplicate worker, and a
    // duplicate cannot pass the runner lock.
    // SAFETY: flock on an owned, open file descriptor.
    if unsafe { libc::flock(worker.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(io(std::io::Error::last_os_error()));
    }
    if dir.join(OUTCOME).exists() {
        return Err("JOB_DONE: the job has an outcome".into());
    }
    let started = crate::status_tap::unix_ms_now();
    (|| {
        worker.set_len(0)?;
        worker.seek(SeekFrom::Start(0))?;
        writeln!(
            worker,
            "pid={} started_unix_ms={}",
            std::process::id(),
            started.map_or_else(|| "UNKNOWN".into(), |s| s.to_string())
        )?;
        worker.sync_all()
    })()
    .map_err(io)?;
    let (outcome, executed) = match execute(project, &request, cancelled) {
        Ok(done) => (
            finish(COMPLETED, String::new(), Some(done.result()), started)?,
            Some(done),
        ),
        Err(code) if code == backtest::CANCELLED => (finish(CANCELLED, code, None, started)?, None),
        Err(code) => (finish(FAILED, code, None, started)?, None),
    };
    drop(worker);
    drop(runner);
    Ok((outcome, executed))
}

/// A new job id: 32 hex digits.
pub fn new_id() -> Result<String, String> {
    journal::random16().map(|b| journal::hex(&b)).map_err(io)
}

/// `p0ctl job start|backtest|optimize` arguments to a request (the same flags as the existing
/// `p0ctl backtest|optimize`; `--id` the caller's id, else a new one).
pub fn request_of(project: &Path, kind: &str, args: &[String]) -> Result<JobRequest, String> {
    let flag = |name: &str| -> Result<Option<String>, String> {
        match args.iter().position(|a| a == name) {
            None => Ok(None),
            Some(i) => match args.get(i + 1) {
                Some(v) if !v.starts_with("--") => Ok(Some(v.clone())),
                _ => Err(format!("{name}: value required")),
            },
        }
    };
    let run_arg = flag("--run")?.ok_or("--run ID|PATH required")?;
    let run = find_run(project, &run_arg).ok_or("RUN_NOT_FOUND_OR_AMBIGUOUS")?;
    native_layout(&run)?;
    let budget = backtest::parse_budget(args)?;
    let spec = match kind {
        "backtest" => JobSpec::Backtest {
            params: backtest::parse_params(&run, args)?,
        },
        "optimize" => {
            let baseline = backtest::parse_params(&run, args)?;
            let space = backtest::parse_space(args, &baseline)?;
            let (holdout_bps, max_points) = backtest::parse_search(args)?;
            JobSpec::Optimize {
                baseline,
                space,
                holdout_bps,
                max_points,
                save_profile: args.iter().any(|a| a == "--save-profile"),
            }
        }
        "post-capture" => JobSpec::PostCapture {
            chain_verify: flag("--chain-verify")?.ok_or("--chain-verify off|strict required")?,
            offline: args.iter().any(|a| a == "--offline"),
            resume: args.iter().any(|a| a == "--resume-verification"),
            provider_evidence: flag("--provider-evidence")?,
        },
        other => {
            return Err(format!(
                "JOB_KIND_UNKNOWN: {other} (backtest|optimize|post-capture)"
            ))
        }
    };
    Ok(JobRequest {
        job_id: match flag("--id")? {
            Some(id) => id,
            None => new_id()?,
        },
        run: run_arg,
        verification_result: flag("--verification-result")?,
        max_seconds: budget.max_seconds,
        max_ram_bytes: budget.max_ram_bytes,
        spec,
        requested_unix_ms: crate::status_tap::unix_ms_now(),
    })
}

/// One line per job for `p0ctl job status`.
pub fn render(s: &JobStatus) -> String {
    let r = &s.request;
    let detail = match &s.state {
        JobState::Running { pid, .. } => format!("pid {pid}"),
        JobState::Done(o) if !o.code.is_empty() => o.code.clone(),
        JobState::Done(o) => match &o.result {
            Some(JobResult::Backtest { result_id, .. } | JobResult::Optimize { result_id, .. }) => {
                format!("result {result_id}")
            }
            Some(JobResult::PostCapture {
                verification_ok,
                report_ok,
                verdict,
            }) => format!(
                "verification_ok={verification_ok} report_ok={report_ok} verdict={}",
                verdict.as_deref().unwrap_or("NOT_RUN")
            ),
            None => String::new(),
        },
        _ => String::new(),
    };
    format!(
        "{} {} {} run={} {}",
        r.job_id,
        r.spec.kind(),
        s.state.name(),
        r.run,
        detail
    )
}

/// One run of the project as the analytics projection lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunEntry {
    pub run_id: String,
    /// Its run root under the project (`data/econ4h`, …).
    pub root: &'static str,
    /// NATIVE_V2 (a job can read it), NATIVE_V1 (no native reader) or HISTORICAL (no native
    /// records).
    pub layout: &'static str,
    /// The capture recorded its completion (`capture/market/market_completion.p0j`).
    pub capture_complete: bool,
    /// The newest post-capture qualification (A12/F-03): the record, or why it is unreadable.
    pub qualification: Option<Result<super::qualification::QualificationRecord, String>>,
    /// Directory modification time; `None` when unreadable.
    pub modified_unix_ms: Option<u64>,
    /// The runtime owner runs it now.
    pub running: bool,
}

/// The analytics projection (topic project0.analytics.jobs): runs a job can read and the jobs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalyticsSnapshot {
    pub at_unix_ms: u64,
    pub runner_busy: bool,
    pub runs: Vec<RunEntry>,
    pub jobs: Vec<JobStatus>,
    /// Why the job store was not readable (the job list is then empty, never a guess).
    pub error: Option<String>,
}

fn layout_of(run: &Path) -> &'static str {
    if run.join("capture/run_manifest.p0j").is_file() {
        "NATIVE_V2"
    } else if run.join("run_manifest.p0j").is_file() {
        "NATIVE_V1"
    } else {
        "HISTORICAL"
    }
}

/// The newest `runs_max` runs (by directory modification time, then id) and `jobs_max` jobs of
/// `project`; `running` is the run the runtime owner runs now.
pub fn projection(
    project: &Path,
    runs_max: usize,
    jobs_max: usize,
    running: Option<&str>,
) -> AnalyticsSnapshot {
    let mut runs: Vec<RunEntry> = crate::run_layout::runs(project)
        .into_iter()
        .map(|r| RunEntry {
            layout: layout_of(&r.path),
            capture_complete: r
                .path
                .join("capture/market")
                .join(crate::market::observer::rows::MARKET_COMPLETION_JOURNAL)
                .is_file(),
            qualification: super::qualification::latest(&r.path),
            modified_unix_ms: std::fs::metadata(&r.path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|d| u64::try_from(d.as_millis()).ok()),
            running: running == Some(r.run_id.as_str()),
            run_id: r.run_id,
            root: r.root,
        })
        .collect();
    runs.sort_by(|a, b| {
        (b.modified_unix_ms, &a.run_id, a.root).cmp(&(a.modified_unix_ms, &b.run_id, b.root))
    });
    runs.truncate(runs_max);
    let (jobs, error) = match list(project, jobs_max) {
        Ok(jobs) => (jobs, None),
        Err(e) => (vec![], Some(e)),
    };
    AnalyticsSnapshot {
        at_unix_ms: 0,
        runner_busy: runner_busy(project).unwrap_or(false),
        runs,
        jobs,
        error,
    }
}

/// Publishes the projection of `project` into `tap` every `analytics_status_ms` while it
/// changed, until `stop` (the reporter's stop; it unparks this thread). Without a wall clock
/// (D08) nothing is published.
pub fn spawn_projection(
    project: PathBuf,
    tap: crate::status_tap::StatusTap,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    use std::sync::atomic::Ordering;
    std::thread::Builder::new()
        .name("p0-analytics".into())
        .spawn(move || {
            let mut last: Option<AnalyticsSnapshot> = None;
            while !stop.load(Ordering::Acquire) {
                let limits = crate::status_tap::active_limits();
                let running = match crate::orchestration::control::owner_state(
                    &crate::orchestration::control::lock_path(&project),
                ) {
                    Ok(crate::orchestration::control::OwnerState::Running { run_id, .. }) => {
                        Some(run_id)
                    }
                    _ => None,
                };
                let mut s = projection(
                    &project,
                    usize::try_from(limits.analytics_runs_listed).unwrap_or(usize::MAX),
                    usize::try_from(limits.analytics_jobs_listed).unwrap_or(usize::MAX),
                    running.as_deref(),
                );
                if last.as_ref() != Some(&s) {
                    if let Some(at) = crate::status_tap::unix_ms_now() {
                        last = Some(s.clone());
                        s.at_unix_ms = at;
                        tap.publish_analytics(s);
                    }
                }
                std::thread::park_timeout(std::time::Duration::from_millis(
                    limits.analytics_status_ms,
                ));
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("p0_jobs_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("data/econ4h/run-a/capture")).unwrap();
        d
    }

    fn params() -> Params {
        Params {
            min_profit_usd_micro: 0,
            capital_usd_micro: vec![1_000_000_000],
            funding: vec![],
        }
    }

    fn request(id: &str) -> JobRequest {
        JobRequest {
            job_id: id.into(),
            run: "run-a".into(),
            verification_result: None,
            max_seconds: 5,
            max_ram_bytes: 1 << 20,
            spec: JobSpec::Backtest { params: params() },
            requested_unix_ms: Some(7),
        }
    }

    /// Invalid requests are refused before anything is written: a path as id, an unknown run,
    /// a zero budget, invalid parameters and an out-of-bounds grid.
    #[test]
    fn invalid_requests_write_nothing() {
        let d = project("invalid");
        let refused = |r: JobRequest| submit(&d, &r).err().unwrap();
        assert_eq!(refused(request("../x")), "JOB_ID_INVALID");
        assert_eq!(refused(request(&"a".repeat(65))), "JOB_ID_INVALID");
        let mut r = request("j");
        r.run = "missing".into();
        assert_eq!(refused(r), "RUN_NOT_FOUND_OR_AMBIGUOUS");
        let mut r = request("j");
        r.max_ram_bytes = 0;
        assert_eq!(refused(r), "BACKTEST_BUDGET_INVALID");
        let mut r = request("j");
        r.spec = JobSpec::Backtest {
            params: Params {
                capital_usd_micro: vec![],
                ..params()
            },
        };
        assert_eq!(refused(r), "BACKTEST_CAPITAL_INVALID");
        let mut r = request("j");
        r.spec = JobSpec::Optimize {
            baseline: params(),
            space: Space {
                min_profit_usd_micro: vec![0, 1],
                funding: vec![vec![]],
                capital_usd_micro: vec![1_000_000_000],
            },
            holdout_bps: 5_000,
            max_points: 1,
            save_profile: false,
        };
        assert_eq!(refused(r), "OPTIMIZE_GRID_OUT_OF_BOUNDS");
        assert!(!d.join(JOB_DIR).join("j").exists());
        assert_eq!(list(&d, 10).unwrap(), vec![]);
        let _ = std::fs::remove_dir_all(d);
    }

    /// QUEUED until a worker records itself, RUNNING while it holds its lock, INTERRUPTED once a
    /// recorded worker is gone without an outcome; only a running job can be cancelled.
    #[test]
    fn job_states_follow_the_worker_lock() {
        let d = project("states");
        submit(&d, &request("j")).unwrap();
        let state = || status(&d, "j").unwrap().state;
        assert_eq!(state(), JobState::Queued);
        assert!(cancel(&d, "j").unwrap_err().starts_with("JOB_NOT_RUNNING"));
        let lock = d.join(JOB_DIR).join("j").join(WORKER_LOCK);
        let mut held_by_test = open_lock(&lock).unwrap();
        assert!(flock_nb(&held_by_test, libc::LOCK_EX).unwrap());
        assert_eq!(
            state(),
            JobState::Running {
                pid: 0,
                started_unix_ms: None
            },
            "a worker holds the lock before it recorded itself"
        );
        assert!(cancel(&d, "j").unwrap_err().starts_with("JOB_STARTING"));
        writeln!(held_by_test, "pid=4242 started_unix_ms=9").unwrap();
        assert_eq!(
            state(),
            JobState::Running {
                pid: 4242,
                started_unix_ms: Some(9)
            }
        );
        drop(held_by_test);
        assert_eq!(state(), JobState::Interrupted);
        assert!(cancel(&d, "j").unwrap_err().starts_with("JOB_NOT_RUNNING"));
        assert_eq!(status(&d, "other").unwrap_err(), "JOB_UNKNOWN");
        let _ = std::fs::remove_dir_all(d);
    }

    /// One job per project: a start while one runs is refused before anything is written, a
    /// worker that finds the runner busy records FAILED; a run without evidence fails closed
    /// with the backtest's own code, never a result.
    #[test]
    fn one_job_at_a_time_and_failures_are_recorded() {
        let d = project("busy");
        submit(&d, &request("first")).unwrap();
        let runner = open_lock(&d.join(JOB_DIR).join(RUNNER_LOCK)).unwrap();
        assert!(flock_nb(&runner, libc::LOCK_EX).unwrap());
        assert!(runner_busy(&d).unwrap());
        assert!(start(&d, &request("second"), Path::new("/nonexistent"))
            .unwrap_err()
            .starts_with("JOB_RUNNER_BUSY"));
        assert!(!d.join(JOB_DIR).join("second").exists());
        let (busy, _) = run(&d, "first", None).unwrap();
        assert_eq!(busy.state, FAILED);
        assert!(busy.code.starts_with("JOB_RUNNER_BUSY"));
        drop(runner);
        submit(&d, &request("third")).unwrap();
        let (failed, executed) = run(&d, "third", None).unwrap();
        assert!(executed.is_none());
        assert_eq!((failed.state.as_str(), &failed.result), (FAILED, &None));
        assert!(!failed.code.is_empty());
        assert_eq!(failed.worker_pid, std::process::id());
        // A worker that cannot be spawned leaves its reason as the outcome.
        let spawn = start(&d, &request("fourth"), Path::new("/nonexistent/p0ctl")).unwrap_err();
        assert!(spawn.starts_with("JOB_SPAWN_FAILED"), "{spawn}");
        match status(&d, "fourth").unwrap().state {
            JobState::Done(o) => assert_eq!((o.state.as_str(), o.code), (FAILED, spawn)),
            s => panic!("{s:?}"),
        }
        let _ = std::fs::remove_dir_all(d);
    }

    /// A08/F-02: a post-capture job verifies strict or off; online strict needs the operator's
    /// provider evidence (the GUI can only ask for strict from the cache); a running
    /// verification is not cancellable (it ends at its own budget).
    #[test]
    fn post_capture_jobs_are_validated_and_not_cancellable() {
        let d = project("post_capture");
        let spec = |mode: &str, offline, evidence: Option<&str>| JobSpec::PostCapture {
            chain_verify: mode.into(),
            offline,
            resume: false,
            provider_evidence: evidence.map(String::from),
        };
        let with = |id: &str, s| JobRequest {
            spec: s,
            ..request(id)
        };
        assert!(submit(&d, &with("a", spec("index", false, None)))
            .unwrap_err()
            .starts_with("CHAIN_VERIFY_index_BLOCKED"));
        assert!(submit(&d, &with("b", spec("strict", false, None)))
            .unwrap_err()
            .starts_with("STRICT_PROVIDER_EVIDENCE_REQUIRED"));
        submit(&d, &with("c", spec("strict", true, None))).unwrap();
        submit(&d, &with("e", spec("strict", false, Some("/evidence.p0j")))).unwrap();
        let args: Vec<String> = [
            "--run",
            "run-a",
            "--id",
            "f",
            "--chain-verify",
            "strict",
            "--offline",
            "--max-seconds",
            "5",
            "--max-ram-bytes",
            "9",
        ]
        .map(String::from)
        .to_vec();
        let parsed = request_of(&d, "post-capture", &args).unwrap();
        assert_eq!(parsed.spec, spec("strict", true, None));
        assert_eq!((parsed.job_id.as_str(), parsed.max_seconds), ("f", 5));
        let lock = d.join(JOB_DIR).join("c").join(WORKER_LOCK);
        let mut worker = open_lock(&lock).unwrap();
        assert!(flock_nb(&worker, libc::LOCK_EX).unwrap());
        writeln!(worker, "pid=4242 started_unix_ms=9").unwrap();
        assert!(cancel(&d, "c")
            .unwrap_err()
            .starts_with("JOB_NOT_CANCELLABLE"));
        let _ = std::fs::remove_dir_all(d);
    }

    /// PERSIST-04: the layouts of the job request and outcome are pinned.
    #[test]
    fn job_payload_layouts_are_pinned() {
        fn pin<P: Payload>(v: &P) -> (u16, u16, String) {
            let bytes = crate::analytics::payload::encode(v).unwrap();
            (
                P::KIND,
                P::SCHEMA,
                solana_sdk::hash::hash(&bytes).to_string(),
            )
        }
        let outcome = JobOutcome {
            job_id: "j".into(),
            state: CANCELLED.into(),
            code: backtest::CANCELLED.into(),
            result: None,
            worker_pid: 1,
            started_unix_ms: Some(2),
            finished_unix_ms: None,
        };
        let pins = vec![pin(&request("j")), pin(&outcome)];
        let expected: Vec<(u16, u16, String)> = [
            (356, 2, "EwijWdqhBmCFesGM5giiKp36vU1AZpikvQ8K6vtsrhWw"),
            (357, 2, "BnxV8MzqqLaFtWzNWfkMsT9mtsKKGFtK6gfJxkMuUFu5"),
        ]
        .into_iter()
        .map(|(k, s, h)| (k, s, h.to_string()))
        .collect();
        assert_eq!(pins, expected);
    }
}

//! DEC-07 (N7, ENTSCHEIDUNGEN §7): the boot controller — the one path of `p0ctl boot …` (the
//! terminal) and the Capture tab (gateway → `darkbotctl p0 boot …`) from the offline preflight
//! to a started run: (1) offline preflight, (2) the operator's network approval of the offline
//! token, (3) the network audit (typed `BOOT_NETWORK` record), (4) the operator's start approval
//! of the network token, (5) the start, whose internal token handover re-checks the TTL and the
//! BOOT-12 identities (`run start`). Both approvals are separate explicit commands naming the
//! token they approve. Every step of a flow runs at most once and one boot step runs per
//! project at a time, so nothing is approved or repeated automatically: a new attempt is a new
//! flow. The TTL is the registry value the flow's offline step resolved (between runs), sealed
//! into its network record.
//!
//! * `<project>/data/boot/<flow_id>/flow.p0j`: the offline step — audit, mode and operational
//!   arguments (immutable; the network audit and the start resolve exactly these).
//! * `network.p0j`: the network audit record; `network.outcome.p0j` the step's outcome.
//! * `start.outcome.p0j`: the start step's outcome (the run it launched).
//! * `<step>.lock`: held by the step's one worker, its pid inside; `<step>.spawn` its spawner.
use super::boot_audit::{AuditVerdict, BootAudit, CheckState, NetworkAudit};
use crate::analytics::jobs::{alive, flock_nb, held, open_lock, valid_id, worker_record};
use crate::analytics::journal;
use crate::analytics::payload::Payload;
use serde::{Deserialize, Serialize};
use std::io::{Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

/// The boot flows of a project.
pub const BOOT_DIR: &str = "data/boot";
const FLOW: &str = "flow.p0j";
/// The network audit record of a flow (`p0ctl boot network` writes it, `boot start` reads it).
pub const NETWORK_RECORD: &str = "network.p0j";
/// The network audit step.
pub const NETWORK: &str = "NETWORK";
/// The start step.
pub const START: &str = "START";

pub const COMPLETED: &str = "COMPLETED";
pub const FAILED: &str = "FAILED";
pub const CANCELLED: &str = "CANCELLED";

/// A flow's offline step: what was audited offline, for which mode and with which operational
/// arguments; the TTL the registry resolved for it and that value's source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootFlow {
    pub flow_id: String,
    /// The mode `run start` launches (econ4h|shadow); every audit resolves its configuration.
    pub mode: String,
    /// The operational arguments (after `--`) of every step of the flow.
    pub operational: Vec<String>,
    pub offline: BootAudit,
    pub ttl_seconds: u64,
    pub ttl_source: String,
    /// `None`: the wall clock was not readable (never 0).
    pub created_unix_ms: Option<u64>,
}
impl Payload for BootFlow {
    const KIND: u16 = journal::kind::BOOT_FLOW;
}

/// The end of one step of a flow.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootStepOutcome {
    pub flow_id: String,
    /// NETWORK or START.
    pub step: String,
    /// COMPLETED, FAILED or CANCELLED.
    pub state: String,
    /// The refusal code of a failed or cancelled step; empty when completed.
    pub code: String,
    /// The run a start step launched.
    pub run_id: Option<String>,
    pub worker_pid: u32,
    pub started_unix_ms: Option<u64>,
    pub finished_unix_ms: Option<u64>,
}
impl Payload for BootStepOutcome {
    const KIND: u16 = journal::kind::BOOT_STEP;
}

/// A step as the store shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepState {
    /// Never requested.
    NotRun,
    /// Requested; its worker has not taken it yet.
    Queued,
    Running {
        pid: u32,
        started_unix_ms: Option<u64>,
    },
    /// A worker had started and is gone without an outcome: the end is unknown.
    Interrupted,
    Done(Box<BootStepOutcome>),
}

impl StepState {
    pub fn name(&self) -> &str {
        match self {
            Self::NotRun => "NOT_RUN",
            Self::Queued => "QUEUED",
            Self::Running { .. } => "RUNNING",
            Self::Interrupted => "INTERRUPTED",
            Self::Done(o) => &o.state,
        }
    }
    fn active(&self) -> bool {
        matches!(self, Self::Queued | Self::Running { .. })
    }
}

/// A flow as the store shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowStatus {
    pub flow: BootFlow,
    pub network: StepState,
    /// The network audit record, once the network step wrote one.
    pub network_record: Option<NetworkAudit>,
    pub start: StepState,
}

fn io(e: impl std::fmt::Display) -> String {
    format!("BOOT_STORE_IO: {e}")
}

fn store(project: &Path) -> PathBuf {
    project.join(BOOT_DIR)
}

fn flow_dir(project: &Path, flow_id: &str) -> Result<PathBuf, String> {
    if !valid_id(flow_id) {
        return Err("BOOT_FLOW_ID_INVALID".into());
    }
    Ok(store(project).join(flow_id))
}

fn file(step: &str, suffix: &str) -> Result<String, String> {
    match step {
        NETWORK => Ok(format!("network.{suffix}")),
        START => Ok(format!("start.{suffix}")),
        _ => Err(format!("BOOT_STEP_INVALID: {step} (NETWORK|START)")),
    }
}

/// Records a flow's offline step (an existing id is refused, never overwritten).
pub fn create(project: &Path, flow: &BootFlow) -> Result<(), String> {
    let dir = flow_dir(project, &flow.flow_id)?;
    std::fs::create_dir_all(store(project)).map_err(io)?;
    match std::fs::create_dir(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err("BOOT_FLOW_EXISTS".into())
        }
        Err(e) => return Err(io(e)),
    }
    journal::write_typed_value(&dir.join(FLOW), journal::source::BOOT_CONTROL, flow).map_err(io)
}

/// A flow's offline step.
pub fn flow(project: &Path, flow_id: &str) -> Result<BootFlow, String> {
    let path = flow_dir(project, flow_id)?.join(FLOW);
    journal::read_typed_value(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "BOOT_FLOW_UNKNOWN".to_string()
        } else {
            format!("BOOT_FLOW_INVALID: {e}")
        }
    })
}

/// The network audit record of a flow (where its network step writes it).
pub fn network_record_path(project: &Path, flow_id: &str) -> Result<PathBuf, String> {
    Ok(flow_dir(project, flow_id)?.join(NETWORK_RECORD))
}

fn step_state(dir: &Path, step: &str) -> Result<StepState, String> {
    let outcome = dir.join(file(step, "outcome.p0j")?);
    let read = |p: &Path| {
        journal::read_typed_value::<BootStepOutcome>(p)
            .map(|o| StepState::Done(Box::new(o)))
            .map_err(|e| format!("BOOT_STEP_OUTCOME_INVALID: {e}"))
    };
    if outcome.exists() {
        return read(&outcome);
    }
    let lock = dir.join(file(step, "lock")?);
    let spawn = dir.join(file(step, "spawn")?);
    let (pid, started_unix_ms) = worker_record(&lock);
    Ok(if held(&lock)? {
        StepState::Running {
            pid,
            started_unix_ms,
        }
    } else if outcome.exists() {
        // The worker finished between the outcome check and the lock probe.
        read(&outcome)?
    } else if !spawn.exists() {
        StepState::NotRun
    } else if pid == 0 && alive(worker_record(&spawn).0) {
        StepState::Queued
    } else {
        StepState::Interrupted
    })
}

/// The state of one flow.
pub fn status(project: &Path, flow_id: &str) -> Result<FlowStatus, String> {
    let flow = flow(project, flow_id)?;
    let dir = flow_dir(project, flow_id)?;
    let network_record = journal::read_typed_value::<NetworkAudit>(&dir.join(NETWORK_RECORD)).ok();
    Ok(FlowStatus {
        network: step_state(&dir, NETWORK)?,
        start: step_state(&dir, START)?,
        network_record,
        flow,
    })
}

/// Every readable flow, newest first (ids break ties).
pub fn list(project: &Path) -> Result<Vec<FlowStatus>, String> {
    let entries = match std::fs::read_dir(store(project)) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(io(e)),
    };
    let mut flows: Vec<FlowStatus> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|id| valid_id(id))
        .filter_map(|id| status(project, &id).ok())
        .collect();
    flows.sort_by(|a, b| {
        (b.flow.created_unix_ms, &b.flow.flow_id).cmp(&(a.flow.created_unix_ms, &a.flow.flow_id))
    });
    Ok(flows)
}

/// The step of `flow` the operator approves with `token`: the network step the offline audit's
/// token, the start step the network record's. Checked before any worker exists; each audit
/// checks its own token again (`--network-approved`, `--mainnet-approved`).
pub fn check_approval(s: &FlowStatus, step: &str, token: &str) -> Result<(), String> {
    let approves = |readiness: &str| token.len() == 16 && readiness.starts_with(token);
    match step {
        NETWORK if s.flow.offline.verdict == AuditVerdict::Blocked => {
            Err("BOOT_OFFLINE_BLOCKED: the offline preflight blocks; nothing to approve".into())
        }
        NETWORK if !approves(&s.flow.offline.readiness_token) => {
            Err("BOOT_NETWORK_APPROVAL_MISMATCH: the approval names another offline token".into())
        }
        NETWORK => Ok(()),
        START => {
            let ready = matches!(&s.network, StepState::Done(o) if o.state == COMPLETED);
            match &s.network_record {
                Some(r) if ready && r.audit.verdict == AuditVerdict::Ready => {
                    if approves(&r.audit.readiness_token) {
                        Ok(())
                    } else {
                        Err("BOOT_START_APPROVAL_MISMATCH: the approval names another network token".into())
                    }
                }
                _ => Err("BOOT_NETWORK_NOT_READY: no READY network audit in this flow".into()),
            }
        }
        other => Err(format!("BOOT_STEP_INVALID: {other} (NETWORK|START)")),
    }
}

/// `boot network|start`: after the approval check, starts the step's detached worker (`worker`
/// is `p0ctl`: `p0ctl boot run --flow ID --step STEP <args>` in its own process group, its
/// output in `<step>.log`). Refused when the step ran or was requested before, or while any boot
/// step of the project is active.
pub fn begin(
    project: &Path,
    flow_id: &str,
    step: &str,
    token: &str,
    worker: &Path,
    args: &[String],
) -> Result<(), String> {
    let dir = flow_dir(project, flow_id)?;
    let s = status(project, flow_id)?;
    let state = match step {
        NETWORK => &s.network,
        START => &s.start,
        other => return Err(format!("BOOT_STEP_INVALID: {other} (NETWORK|START)")),
    };
    if *state != StepState::NotRun {
        return Err(format!(
            "BOOT_STEP_ALREADY_REQUESTED: {step} is {} (a new attempt is a new flow)",
            state.name()
        ));
    }
    if list(project)?
        .iter()
        .any(|f| f.network.active() || f.start.active())
    {
        return Err("BOOT_BUSY: another boot step of this project is active".into());
    }
    check_approval(&s, step, token)?;
    let spawn = dir.join(file(step, "spawn")?);
    std::fs::write(&spawn, format!("pid={}\n", std::process::id())).map_err(io)?;
    let log = std::fs::File::create(dir.join(file(step, "log")?)).map_err(io)?;
    let spawned = {
        use std::os::unix::process::CommandExt;
        std::process::Command::new(worker)
            .args(["boot", "run", "--flow", flow_id, "--step", step])
            .args(args)
            .env("P0_PROJECT_DIR", project)
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone().map_err(io)?)
            .stderr(log)
            .process_group(0)
            .spawn()
    };
    match spawned {
        Ok(child) => {
            let partial = dir.join(format!(".{}.{}", file(step, "spawn")?, std::process::id()));
            std::fs::write(&partial, format!("pid={}\n", child.id())).map_err(io)?;
            std::fs::rename(&partial, &spawn).map_err(io)
        }
        Err(e) => {
            let outcome = BootStepOutcome {
                flow_id: flow_id.into(),
                step: step.into(),
                state: FAILED.into(),
                code: format!("BOOT_SPAWN_FAILED: {e}"),
                run_id: None,
                worker_pid: 0,
                started_unix_ms: None,
                finished_unix_ms: crate::status_tap::unix_ms_now(),
            };
            write_outcome(&dir, step, &outcome)?;
            Err(outcome.code)
        }
    }
}

fn write_outcome(dir: &Path, step: &str, outcome: &BootStepOutcome) -> Result<(), String> {
    journal::write_typed_value(
        &dir.join(file(step, "outcome.p0j")?),
        journal::source::BOOT_CONTROL,
        outcome,
    )
    .map_err(io)
}

/// The worker body of a step (`p0ctl boot run`): takes the step's lock, runs `body` once and
/// records its outcome — COMPLETED (with the run a start launched), FAILED with the refusal
/// code, CANCELLED when `cancelled` says the operator stopped it. A step with an outcome never
/// runs again.
pub fn run_step(
    project: &Path,
    flow_id: &str,
    step: &str,
    cancelled: fn() -> bool,
    body: impl FnOnce(&BootFlow, &Path) -> Result<Option<String>, String>,
) -> Result<BootStepOutcome, String> {
    let dir = flow_dir(project, flow_id)?;
    if dir.join(file(step, "outcome.p0j")?).exists() {
        return Err("BOOT_STEP_DONE: the step has an outcome".into());
    }
    let finish = |state: &str, code: String, run_id, started| -> Result<BootStepOutcome, String> {
        let outcome = BootStepOutcome {
            flow_id: flow_id.into(),
            step: step.into(),
            state: state.into(),
            code,
            run_id,
            worker_pid: std::process::id(),
            started_unix_ms: started,
            finished_unix_ms: crate::status_tap::unix_ms_now(),
        };
        write_outcome(&dir, step, &outcome)?;
        Ok(outcome)
    };
    let flow = match flow(project, flow_id) {
        Ok(f) => f,
        Err(e) => return finish(FAILED, e, None, None),
    };
    let mut lock = match open_lock(&dir.join(file(step, "lock")?)) {
        Ok(l) => l,
        Err(e) => return finish(FAILED, io(e), None, None),
    };
    match flock_nb(&lock, libc::LOCK_EX) {
        Ok(true) => {}
        Ok(false) => return Err("BOOT_STEP_RUNNING: another worker holds the step".into()),
        Err(e) => return finish(FAILED, io(e), None, None),
    }
    if dir.join(file(step, "outcome.p0j")?).exists() {
        return Err("BOOT_STEP_DONE: the step has an outcome".into());
    }
    let started = crate::status_tap::unix_ms_now();
    if let Err(e) = (|| {
        lock.set_len(0)?;
        lock.seek(SeekFrom::Start(0))?;
        writeln!(
            lock,
            "pid={} started_unix_ms={}",
            std::process::id(),
            started.map_or_else(|| "UNKNOWN".into(), |s| s.to_string())
        )?;
        lock.sync_all()
    })() {
        return finish(FAILED, io(e), None, started);
    }
    let outcome = match body(&flow, &dir) {
        Ok(run_id) => finish(COMPLETED, String::new(), run_id, started)?,
        Err(code) if cancelled() => finish(CANCELLED, code, None, started)?,
        Err(code) => finish(FAILED, code, None, started)?,
    };
    // SAFETY: flock on an owned, open file descriptor (released by the drop anyway).
    unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_UN) };
    Ok(outcome)
}

/// `boot cancel`: asks the running step's worker to stop (SIGTERM). A network audit ends; a
/// start forwards the stop to its runtime (the capture drains and seals).
pub fn cancel(project: &Path, flow_id: &str) -> Result<u32, String> {
    let s = status(project, flow_id)?;
    let pid = match (&s.network, &s.start) {
        (StepState::Running { pid, .. }, _) | (_, StepState::Running { pid, .. }) => *pid,
        _ => return Err("BOOT_NOT_RUNNING: no step of the flow runs".into()),
    };
    if pid == 0 {
        return Err("BOOT_STARTING: the worker has not recorded itself yet; retry".into());
    }
    // SAFETY: kill(2) with SIGTERM to the pid of the step's lock holder.
    if unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) } != 0 {
        return Err(format!(
            "BOOT_CANCEL_SIGNAL: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(pid)
}

/// The flow as the Capture tab shows it (secret-free: tokens are content hashes the operator
/// approves, never keys).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootView {
    pub flow_id: String,
    pub mode: String,
    pub created_unix_ms: Option<u64>,
    pub offline_verdict: &'static str,
    /// The 16 characters the network approval names.
    pub offline_token: String,
    /// `id: reason` of every critical check that did not pass.
    pub offline_blocked: Vec<String>,
    pub ttl_seconds: u64,
    pub ttl_source: String,
    pub network: StepView,
    pub network_verdict: Option<&'static str>,
    /// The 16 characters the start approval names.
    pub network_token: Option<String>,
    pub network_blocked: Vec<String>,
    /// The TTL window of the network record: its oldest read and its expiry (unix seconds).
    pub observed_unix_s: Option<u64>,
    pub expires_unix_s: Option<u64>,
    pub start: StepView,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepView {
    pub state: String,
    pub code: String,
    pub run_id: Option<String>,
    pub started_unix_ms: Option<u64>,
    pub finished_unix_ms: Option<u64>,
}

fn verdict(v: AuditVerdict) -> &'static str {
    match v {
        AuditVerdict::Ready => "READY",
        AuditVerdict::OfflineReadyNetworkNotRun => "OFFLINE_READY_NETWORK_NOT_RUN",
        AuditVerdict::Blocked => "BLOCKED",
    }
}

fn blocked(a: &BootAudit) -> Vec<String> {
    a.checks
        .iter()
        .filter(|c| c.critical && !matches!(c.state, CheckState::Pass | CheckState::NotApplicable))
        .map(|c| format!("{}: {}", c.id, c.reason))
        .collect()
}

fn step_view(s: &StepState) -> StepView {
    let done = match s {
        StepState::Done(o) => Some(o.as_ref()),
        _ => None,
    };
    StepView {
        state: s.name().to_string(),
        code: done.map(|o| o.code.clone()).unwrap_or_default(),
        run_id: done.and_then(|o| o.run_id.clone()),
        started_unix_ms: match s {
            StepState::Running {
                started_unix_ms, ..
            } => *started_unix_ms,
            _ => done.and_then(|o| o.started_unix_ms),
        },
        finished_unix_ms: done.and_then(|o| o.finished_unix_ms),
    }
}

impl BootView {
    pub fn of(s: &FlowStatus) -> Self {
        let token = |t: &str| t.get(..16).unwrap_or(t).to_string();
        let record = s.network_record.as_ref();
        Self {
            flow_id: s.flow.flow_id.clone(),
            mode: s.flow.mode.clone(),
            created_unix_ms: s.flow.created_unix_ms,
            offline_verdict: verdict(s.flow.offline.verdict),
            offline_token: token(&s.flow.offline.readiness_token),
            offline_blocked: blocked(&s.flow.offline),
            ttl_seconds: s.flow.ttl_seconds,
            ttl_source: s.flow.ttl_source.clone(),
            network: step_view(&s.network),
            network_verdict: record.map(|r| verdict(r.audit.verdict)),
            network_token: record.map(|r| token(&r.audit.readiness_token)),
            network_blocked: record.map(|r| blocked(&r.audit)).unwrap_or_default(),
            observed_unix_s: record.map(|r| r.observed_at),
            expires_unix_s: record.map(|r| r.expires_at),
            start: step_view(&s.start),
        }
    }
}

/// The newest flow of the project as the Capture tab shows it (`None`: no flow yet).
pub fn projection(project: &Path) -> Option<BootView> {
    list(project).ok()?.first().map(BootView::of)
}

#[cfg(test)]
#[path = "boot_control_tests.rs"]
mod tests;

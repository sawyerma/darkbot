//! Authenticated local capture control. No trading, wallet, arbitrary paths or shell commands.
use crate::{
    http::AppState,
    lifecycle::{classify, CaptureProcess},
    proto::{
        expected_version, CommandLookupResponse, CommandPhase, CommandRejectReason, CommandRequest,
        CommandResponse, CommandStatus, ProjectId,
    },
    session,
};
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{process::Command, sync::Mutex};

const START: &str = "project0.capture.start";
const STOP: &str = "project0.capture.stop";
const BSC_START: &str = "bsc.readiness.start";
const BSC_STOP: &str = "bsc.readiness.stop";
/// Offline G0.7 A/B candidate selection request; never arm, sign or send.
const BSC_CANDIDATE: &str = "bsc.candidate.select";
fn allowed(project: i32, action: &str) -> bool {
    (project == ProjectId::Project0 as i32 && [START, STOP].contains(&action))
        || (project == ProjectId::Bsc as i32
            && [BSC_START, BSC_STOP, BSC_CANDIDATE].contains(&action))
}
/// The only accepted candidate request: a fixed profile as resource and the
/// selection generation of the last applied status as `profile_version` CAS.
fn candidate_request(req: &CommandRequest) -> Option<(&'static str, u64)> {
    let profile = match req.resource_id.as_str() {
        "SELF_A" => "SELF_A",
        "SELF_B" => "SELF_B",
        _ => return None,
    };
    match req.expected.as_ref()?.version? {
        expected_version::Version::ProfileVersion(generation) if req.params.is_empty() => {
            Some((profile, generation))
        }
        _ => None,
    }
}
/// Ledger retention (Wire V1.3 §7, idempotency for the intent/receipt lifetime): every open
/// record (PENDING / PENDING_RECONCILE) stays in `commands/`. At MAX_RECORDS the oldest terminal
/// receipt (COMPLETED / FAILED, by last update) moves to `commands/archive/`, which replay and
/// conflict checks still read. When the archive is full, the receipt issued earliest leaves it,
/// after `retired_through` durably took its `issued_at_unix_ns`. Expiry rule: a request_id the
/// ledger does not know whose `issued_at_unix_ns` is at or before that mark may be a retired
/// command and is refused, never executed; a request issued later than the gateway's receive
/// time plus the liveness window is refused, so no request can move the mark ahead. A ledger of
/// open intents refuses new ones.
const MAX_RECORDS: usize = 1024;

#[derive(Clone)]
pub struct Paths {
    pub ctl: PathBuf,
    pub rundir: PathBuf,
    pub logdir: PathBuf,
    pub label: String,
}
pub struct Control {
    paths: Option<Paths>,
    serial: Mutex<()>,
    /// MAX_RECORDS; smaller only in tests.
    capacity: usize,
}
#[derive(Clone, Serialize)]
pub struct ProcessSnapshot {
    pub state: String,
    pub run_id: String,
    pub pid: Option<u32>,
    pub reason: String,
    pub observed_unix_ms: u64,
    pub controller_available: bool,
}
#[derive(Serialize, Deserialize)]
struct Record {
    request: Vec<u8>,
    response: Vec<u8>,
    phase: i32,
    detail: String,
    launch_id: String,
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn text_file(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_owned()
}
fn field(text: &str, name: &str) -> Option<i32> {
    text.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&format!("\"{name}\" = "))
            .and_then(|s| s.trim_end_matches(';').parse().ok())
    })
}
impl Control {
    pub fn from_env() -> Self {
        let paths = (|| {
            Some(Paths {
                ctl: std::env::var_os("DARKBOT_CTL")?.into(),
                rundir: std::env::var_os("DARKBOT_RUNDIR")?.into(),
                logdir: std::env::var_os("DARKBOT_LOGDIR")?.into(),
                label: std::env::var("DARKBOT_CAPTURE_LABEL").ok()?,
            })
        })();
        Self::new(paths)
    }
    pub fn new(paths: Option<Paths>) -> Self {
        Self {
            paths,
            serial: Mutex::new(()),
            capacity: MAX_RECORDS,
        }
    }
    pub async fn observe(&self) -> ProcessSnapshot {
        let mut snapshot = ProcessSnapshot {
            state: "UNKNOWN".into(),
            run_id: String::new(),
            pid: None,
            reason: "Capture controller not configured".into(),
            observed_unix_ms: now_ms(),
            controller_available: false,
        };
        let Some(p) = &self.paths else {
            return snapshot;
        };
        snapshot.controller_available = p.ctl.is_file();
        snapshot.run_id = text_file(&p.rundir.join("capture.launch"));
        // List is read-only. An execution/permission/timeout error is NEVER evidence of STOPPED.
        let output = tokio::time::timeout(
            Duration::from_secs(2),
            Command::new("/bin/launchctl")
                .args(["list", &p.label])
                .kill_on_drop(true)
                .output(),
        )
        .await;
        let Ok(Ok(out)) = output else {
            snapshot.reason = "launchd query unavailable".into();
            return snapshot;
        };
        let state = if !out.status.success() {
            let error = String::from_utf8_lossy(&out.stderr);
            if error.contains("Could not find service")
                || error.contains("Could not find specified service")
            {
                CaptureProcess::Stopped
            } else {
                CaptureProcess::Unknown
            }
        } else {
            let text = String::from_utf8_lossy(&out.stdout);
            snapshot.pid = field(&text, "PID").filter(|n| *n > 0).map(|n| n as u32);
            let stopping = text_file(&p.rundir.join("capture.stopping"));
            let user_stop = text_file(&p.rundir.join("capture.user_stopped"));
            let matched = !snapshot.run_id.is_empty()
                && (stopping == snapshot.run_id
                    || user_stop
                        .split_whitespace()
                        .any(|s| s == format!("launch={}", snapshot.run_id)));
            let result = text_file(&p.rundir.join("capture.result"));
            let completed = result
                .lines()
                .any(|s| s == format!("launch_id={}", snapshot.run_id))
                && result.lines().any(|s| {
                    matches!(
                        s,
                        "code=CAPTURE_COMPLETE" | "code=MARKET_CAPTURE_INCOMPLETE"
                    )
                });
            classify(
                snapshot.pid,
                field(&text, "LastExitStatus"),
                matched,
                completed,
            )
        };
        snapshot.state = state.as_str().into();
        snapshot.reason = format!("launchd: {}", p.label);
        snapshot.observed_unix_ms = now_ms();
        snapshot
    }
    pub async fn observe_project(&self, project: i32) -> ProcessSnapshot {
        if project != ProjectId::Bsc as i32 {
            return self.observe().await;
        }
        let mut s = ProcessSnapshot {
            state: "UNKNOWN".into(),
            run_id: String::new(),
            pid: None,
            reason: "BSC controller unavailable".into(),
            observed_unix_ms: now_ms(),
            controller_available: false,
        };
        let Some(p) = &self.paths else {
            return s;
        };
        let label = p
            .label
            .strip_suffix(".capture")
            .map(|x| format!("{x}.bsc.readiness"))
            .unwrap_or_default();
        if label.is_empty() {
            return s;
        }
        s.controller_available = p.ctl.parent().is_some_and(|d| {
            d.join("g07_pre_live_readiness").is_file()
                && d.join("bsc-root/SNAPSHOT_SHA256").is_file()
        });
        let launch = text_file(&p.rundir.join("bsc.readiness.launch"));
        s.run_id = kv(&launch, "launch_id");
        let result = text_file(&p.rundir.join("bsc.readiness.result"));
        let matched = !s.run_id.is_empty() && kv(&result, "launch_id") == s.run_id;
        // A run that ended with its native verdict (BLOCKED included) or on request is
        // STOPPED; FAILED is reserved for a runner or process failure.
        let completed = matched
            && ["PASS_READ_ONLY", "BLOCKED", "STOP", "STOPPED"]
                .contains(&kv(&result, "result_code").as_str());
        let stopping = !s.run_id.is_empty()
            && kv(
                &text_file(&p.rundir.join("bsc.readiness.stopping")),
                "launch_id",
            ) == s.run_id;
        let out = tokio::time::timeout(
            Duration::from_secs(2),
            Command::new("/bin/launchctl")
                .args(["list", &label])
                .kill_on_drop(true)
                .output(),
        )
        .await;
        if let Ok(Ok(out)) = out {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                s.pid = field(&text, "PID").filter(|v| *v > 0).map(|v| v as u32);
                s.state = if s.pid.is_some() {
                    if stopping {
                        "STOPPING"
                    } else {
                        "RUNNING"
                    }
                } else if completed {
                    "STOPPED"
                } else if field(&text, "LastExitStatus").is_some() || matched {
                    "FAILED"
                } else {
                    "UNKNOWN"
                }
                .into();
            } else if String::from_utf8_lossy(&out.stderr).contains("Could not find") {
                s.state = if s.run_id.is_empty() || completed {
                    "STOPPED"
                } else {
                    "FAILED"
                }
                .into();
            }
            s.reason = format!(
                "launchd: {label}; result {}",
                if matched {
                    kv(&result, "result_code")
                } else {
                    String::new()
                }
            );
        }
        s
    }
    fn record_path(&self, id: &str) -> Option<PathBuf> {
        self.paths
            .as_ref()
            .map(|p| p.rundir.join("commands").join(format!("{id}.json")))
    }
    fn load(&self, id: &str) -> Result<Option<Record>, ()> {
        let path = self.record_path(id).ok_or(())?;
        let archived = path
            .parent()
            .ok_or(())?
            .join("archive")
            .join(format!("{id}.json"));
        for p in [path, archived] {
            match fs::read(p) {
                Ok(b) => return serde_json::from_slice(&b).map(Some).map_err(|_| ()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err(()),
            }
        }
        Ok(None)
    }
    fn persist(&self, id: &str, record: &Record) -> Result<(), ()> {
        let path = self.record_path(id).ok_or(())?;
        let dir = path.parent().ok_or(())?;
        fs::create_dir_all(dir).map_err(|_| ())?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|_| ())?;
        if !path.exists() && records(dir)?.len() >= self.capacity {
            archive_oldest_terminal(dir, self.capacity)?;
        }
        let tmp = path.with_extension(format!("tmp-{}", session::random_token(8)));
        let written = (|| {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)
                .map_err(|_| ())?;
            f.write_all(&serde_json::to_vec(record).map_err(|_| ())?)
                .map_err(|_| ())?;
            f.sync_all().map_err(|_| ())?;
            fs::rename(&tmp, &path).map_err(|_| ())?;
            fs::File::open(dir)
                .and_then(|f| f.sync_all())
                .map_err(|_| ())
        })();
        if written.is_err() {
            let _ = fs::remove_file(tmp);
        }
        written
    }
    async fn execute(&self, req: &CommandRequest) -> CommandResponse {
        let _guard = self.serial.lock().await;
        let id = &req.request_id;
        if !valid_id(id) {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::InvalidParams,
                "request_id must be a UUID",
            );
        }
        if !allowed(req.project, &req.action) {
            return reply(
                id,
                CommandStatus::NotImplemented,
                CommandRejectReason::Forbidden,
                "Only managed Project 0 capture, BSC read-only readiness start/stop and the offline BSC candidate request are available",
            );
        }
        let candidate = if req.action == BSC_CANDIDATE {
            match candidate_request(req) {
                Some(c) => Some(c),
                None => {
                    return reply(
                        id,
                        CommandStatus::Rejected,
                        CommandRejectReason::InvalidParams,
                        "Candidate request takes exactly SELF_A or SELF_B and the profile_version generation",
                    )
                }
            }
        } else {
            None
        };
        if candidate.is_none()
            && ((req.project != ProjectId::Bsc as i32 && !req.resource_id.is_empty())
                || (req.project == ProjectId::Bsc as i32
                    && !req.resource_id.is_empty()
                    && !valid_id(&req.resource_id))
                || req.expected.is_some()
                || !req.params.is_empty())
        {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::InvalidParams,
                "This capture action takes no resource, version or parameters",
            );
        }
        let Some(p) = &self.paths else {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::EngineUnavailable,
                "Capture controller not configured",
            );
        };
        let encoded = req.encode_to_vec();
        match self.load(id) {
            Ok(Some(record)) if record.request == encoded => {
                // A replay returns the stored receipt with the current ledger phase.
                return CommandResponse::decode(record.response.as_slice())
                    .map(|r| CommandResponse {
                        phase: record.phase,
                        ..r
                    })
                    .unwrap_or_else(|_| {
                        reply(
                            id,
                            CommandStatus::Rejected,
                            CommandRejectReason::EngineUnavailable,
                            "Command receipt unreadable",
                        )
                    });
            }
            Ok(Some(_)) => {
                return reply(
                    id,
                    CommandStatus::Rejected,
                    CommandRejectReason::IdempotencyConflict,
                    "request_id already belongs to a different payload",
                )
            }
            Err(_) => {
                return reply(
                    id,
                    CommandStatus::Rejected,
                    CommandRejectReason::EngineUnavailable,
                    "Command ledger unreadable",
                )
            }
            _ => {}
        }
        // Expiry rule (MAX_RECORDS): an unknown request must be newer than every retired receipt.
        if req.issued_at_unix_ns > crate::overview::unix_ns_now().saturating_add(clock_lead_ns()) {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::InvalidParams,
                "issued_at lies after the gateway receive time beyond the liveness window",
            );
        }
        let ledger = self
            .record_path(id)
            .and_then(|p| p.parent().map(Path::to_path_buf));
        match ledger.map(|d| retired_through(&d)) {
            Some(Ok(Some(mark))) if req.issued_at_unix_ns <= mark => {
                return reply(
                    id,
                    CommandStatus::Rejected,
                    CommandRejectReason::IdempotencyConflict,
                    "Expired: issued no later than a retired receipt; never executed again, issue a new command",
                )
            }
            Some(Err(())) | None => {
                return reply(
                    id,
                    CommandStatus::Rejected,
                    CommandRejectReason::EngineUnavailable,
                    "Command ledger unreadable",
                )
            }
            _ => {}
        }
        let before = self.observe_project(req.project).await;
        if req.project == ProjectId::Bsc as i32
            && candidate.is_none()
            && req.resource_id != before.run_id
        {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::VersionConflict,
                "Readiness launch changed; refresh process state",
            );
        }
        if before.state == "UNKNOWN"
            || ([START, BSC_START].contains(&req.action.as_str()) && before.state == "STOPPING")
        {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::GateBlocked,
                "Capture state unknown or stopping; no start issued",
            );
        }
        let accepted = reply(
            id,
            CommandStatus::Accepted,
            CommandRejectReason::Unspecified,
            "Capture intent persisted; execution is tracked separately",
        );
        let mut record = Record {
            request: encoded,
            response: accepted.encode_to_vec(),
            phase: CommandPhase::PendingReconcile as i32,
            detail: "Controller dispatch pending; never automatically replayed after interruption"
                .into(),
            launch_id: before.run_id,
        };
        if self.persist(id, &record).is_err() {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::EngineUnavailable,
                "Cannot durably record command (ledger full or unavailable)",
            );
        }
        let generation = candidate.map(|(_, g)| g.to_string()).unwrap_or_default();
        let args = if let Some((profile, _)) = candidate {
            vec!["bsc", "candidate", "request", profile, &generation, id]
        } else if req.action == BSC_START {
            vec!["bsc", "readiness", "start", &req.resource_id]
        } else if req.action == BSC_STOP {
            vec!["bsc", "readiness", "stop", &req.resource_id]
        } else if req.action == START {
            vec!["capture", "start-configured"]
        } else {
            vec!["capture", "stop", "--signal-only"]
        };
        let output = tokio::time::timeout(
            Duration::from_secs(50),
            Command::new(&p.ctl)
                .args(args)
                .env("DARKBOT_STAGED", "1")
                .env("DARKBOT_NO_OPEN", "1")
                .kill_on_drop(true)
                .output(),
        )
        .await;
        let after = self.observe_project(req.project).await;
        match output {
            Ok(Ok(out)) if out.status.success() && candidate.is_some() => {
                record.phase = CommandPhase::Completed as i32;
                record.detail = "Selection request recorded; the native store applies or rejects it at the next desktop bsc sync".into();
            }
            Ok(Ok(_)) if candidate.is_some() => {
                record.phase = CommandPhase::Failed as i32;
                record.detail = "Native candidate request refused (stale generation, unknown candidate, armed guard or a pending request)".into();
            }
            Ok(Ok(out)) if out.status.success() => {
                if [START, BSC_START].contains(&req.action.as_str()) {
                    record.launch_id = after.run_id;
                    record.phase = if after.state == "RUNNING"
                        || (req.action == BSC_START && after.state == "STOPPED")
                    {
                        CommandPhase::Completed
                    } else {
                        CommandPhase::Failed
                    } as i32;
                    record.detail = if after.state == "RUNNING"
                        || (req.action == BSC_START && after.state == "STOPPED")
                    {
                        "Managed process started (a short read-only job may already be complete)"
                    } else {
                        "Capture did not reach RUNNING; inspect logs"
                    }
                    .into();
                } else {
                    record.phase = match after.state.as_str() {
                        "STOPPED" => CommandPhase::Completed,
                        "FAILED" => CommandPhase::Failed,
                        _ => CommandPhase::Pending,
                    } as i32;
                    record.detail =
                        "Orderly stop requested; journal drain/seal remains owned by capture"
                            .into();
                }
            }
            Ok(Ok(_)) => {
                record.phase = CommandPhase::Failed as i32;
                record.detail =
                    "Controller rejected command; inspect Project 0 / gateway logs".into();
            }
            _ => {
                record.phase = CommandPhase::PendingReconcile as i32;
                record.detail =
                    "Controller outcome unknown; inspect process state before a new action".into();
            }
        }
        // Receipt remains durable even if this final update fails; a retry cannot redispatch it.
        if self.persist(id, &record).is_err() {
            tracing::error!(request_id=%id, "command outcome could not be persisted");
        }
        tracing::info!(request_id=%id, action=%req.action, phase=record.phase, "capture command");
        // The ledger phase known now; later progress is visible in the domain STATE.
        CommandResponse {
            phase: record.phase,
            ..accepted
        }
    }
    async fn lookup(&self, id: &str, project: i32) -> CommandLookupResponse {
        let _guard = self.serial.lock().await;
        let Some(mut record) = self.load(id).ok().flatten() else {
            return CommandLookupResponse {
                found: false,
                command_id: String::new(),
                phase: CommandPhase::Unknown as i32,
                detail: "No recorded command".into(),
            };
        };
        let request = CommandRequest::decode(record.request.as_slice()).ok();
        if request.as_ref().map(|r| r.project) != Some(project) {
            return CommandLookupResponse {
                found: false,
                command_id: String::new(),
                phase: CommandPhase::Unknown as i32,
                detail: "No recorded command for project".into(),
            };
        }
        if record.phase == CommandPhase::Pending as i32 {
            let state = self.observe_project(project).await;
            if state.run_id != record.launch_id {
                record.phase = CommandPhase::Completed as i32;
                record.detail = "Target launch replaced; no signal sent to replacement".into();
            } else if state.state == "STOPPED" || state.state == "FAILED" {
                record.phase = if state.state == "STOPPED" {
                    CommandPhase::Completed
                } else {
                    CommandPhase::Failed
                } as i32;
                record.detail = format!("Target capture {}", state.state);
            }
            let _ = self.persist(id, &record);
        }
        CommandLookupResponse {
            found: true,
            command_id: id.into(),
            phase: record.phase,
            detail: record.detail,
        }
    }
}
/// The ledger records of one directory with their last update time.
fn records(dir: &Path) -> Result<Vec<(SystemTime, PathBuf)>, ()> {
    Ok(fs::read_dir(dir)
        .map_err(|_| ())?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect())
}
fn terminal(path: &Path) -> bool {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Record>(&b).ok())
        .is_some_and(|r| {
            r.phase == CommandPhase::Completed as i32 || r.phase == CommandPhase::Failed as i32
        })
}
fn oldest_terminal(dir: &Path) -> Result<Option<PathBuf>, ()> {
    let mut all = records(dir)?;
    all.sort();
    Ok(all.into_iter().map(|(_, p)| p).find(|p| terminal(p)))
}
fn issued_at(path: &Path) -> Option<i64> {
    let record: Record = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    CommandRequest::decode(record.request.as_slice())
        .ok()
        .map(|r| r.issued_at_unix_ns)
}
const RETIRED_THROUGH: &str = "retired_through";
/// Tolerated lead of a browser clock over the gateway for `issued_at_unix_ns`: the existing
/// liveness window (heartbeat interval × missed heartbeats, Wire V1.3 §5). A later stamp is
/// refused, so the expiry mark never runs ahead of real time by more than this window.
fn clock_lead_ns() -> i64 {
    let l = crate::config::Limits::default();
    i64::from(l.heartbeat_interval_ms) * i64::from(l.stale_after_missed_heartbeats) * 1_000_000
}
/// The latest `issued_at_unix_ns` of every receipt removed from the archive; None before the first.
fn retired_through(dir: &Path) -> Result<Option<i64>, ()> {
    match fs::read_to_string(dir.join("archive").join(RETIRED_THROUGH)) {
        Ok(t) => t.trim().parse().map(Some).map_err(|_| ()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    }
}
/// Removes the archived receipt issued earliest; the mark is durable before the receipt is gone.
fn retire_earliest(dir: &Path, archive: &Path) -> Result<(), ()> {
    let (issued, victim) = records(archive)?
        .into_iter()
        .filter(|(_, p)| terminal(p))
        .filter_map(|(t, p)| Some((issued_at(&p)?, t, p)))
        .min()
        .map(|(i, _, p)| (i, p))
        .ok_or(())?;
    let mark = retired_through(dir)?.map_or(issued, |m| m.max(issued));
    let path = archive.join(RETIRED_THROUGH);
    let tmp = path.with_extension(format!("tmp-{}", session::random_token(8)));
    let written = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|_| ())?;
        f.write_all(mark.to_string().as_bytes()).map_err(|_| ())?;
        f.sync_all().map_err(|_| ())?;
        fs::rename(&tmp, &path).map_err(|_| ())?;
        fs::File::open(archive)
            .and_then(|f| f.sync_all())
            .map_err(|_| ())
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
        return written;
    }
    fs::remove_file(victim).map_err(|_| ())
}
/// Frees one ledger slot without touching an open intent; see MAX_RECORDS.
fn archive_oldest_terminal(dir: &Path, capacity: usize) -> Result<(), ()> {
    let source = oldest_terminal(dir)?.ok_or(())?;
    let archive = dir.join("archive");
    fs::create_dir_all(&archive).map_err(|_| ())?;
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o700)).map_err(|_| ())?;
    if records(&archive)?.len() >= capacity {
        retire_earliest(dir, &archive)?;
    }
    fs::rename(&source, archive.join(source.file_name().ok_or(())?)).map_err(|_| ())?;
    for d in [&archive, &dir.to_path_buf()] {
        fs::File::open(d)
            .and_then(|f| f.sync_all())
            .map_err(|_| ())?;
    }
    Ok(())
}
fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
fn reply(
    id: &str,
    status: CommandStatus,
    reject: CommandRejectReason,
    detail: &str,
) -> CommandResponse {
    CommandResponse {
        status: status as i32,
        request_id: id.into(),
        command_id: if status == CommandStatus::Accepted {
            id.into()
        } else {
            String::new()
        },
        reject_reason: reject as i32,
        detail: detail.into(),
        persisted: None,
        phase: CommandPhase::Unspecified as i32,
    }
}
fn authorized(app: &AppState, h: &HeaderMap, mutate: bool) -> Result<(), StatusCode> {
    session::check_fetch_site(h).map_err(|_| StatusCode::FORBIDDEN)?;
    if mutate || h.contains_key("origin") {
        session::check_origin(h, &app.cfg).map_err(|_| StatusCode::FORBIDDEN)?;
    }
    let token = session::cookie_value(h, &app.cfg.session_cookie)
        .filter(|t| app.sessions.validate(t))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if mutate {
        let expected = app.sessions.csrf_of(&token).ok_or(StatusCode::FORBIDDEN)?;
        let sent = h
            .get("x-csrf-token")
            .and_then(|s| s.to_str().ok())
            .ok_or(StatusCode::FORBIDDEN)?;
        if sent != expected {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(())
}
fn protobuf(msg: impl Message) -> Response {
    (
        [
            (CONTENT_TYPE, "application/x-protobuf"),
            (axum::http::header::CACHE_CONTROL, "no-store"),
        ],
        msg.encode_to_vec(),
    )
        .into_response()
}
pub async fn command(State(app): State<Arc<AppState>>, h: HeaderMap, body: Bytes) -> Response {
    if let Err(s) = authorized(&app, &h, true) {
        return s.into_response();
    }
    if h.get(CONTENT_TYPE).and_then(|v| v.to_str().ok()) != Some("application/x-protobuf") {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    let Ok(req) = CommandRequest::decode(body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    protobuf(app.control.execute(&req).await)
}
#[derive(Deserialize)]
pub struct Lookup {
    project: i32,
    request_id: String,
}
pub async fn lookup(
    State(app): State<Arc<AppState>>,
    h: HeaderMap,
    Query(q): Query<Lookup>,
) -> Response {
    if let Err(s) = authorized(&app, &h, false) {
        return s.into_response();
    }
    if ![ProjectId::Project0 as i32, ProjectId::Bsc as i32].contains(&q.project)
        || !valid_id(&q.request_id)
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    protobuf(app.control.lookup(&q.request_id, q.project).await)
}
pub async fn process(State(app): State<Arc<AppState>>, h: HeaderMap) -> Response {
    if let Err(s) = authorized(&app, &h, false) {
        return s.into_response();
    }
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(app.control.observe().await),
    )
        .into_response()
}
pub async fn bsc_process(State(app): State<Arc<AppState>>, h: HeaderMap) -> Response {
    if let Err(s) = authorized(&app, &h, false) {
        return s.into_response();
    }
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(app.control.observe_project(ProjectId::Bsc as i32).await),
    )
        .into_response()
}
fn kv(text: &str, key: &str) -> String {
    text.lines()
        .find_map(|l| l.strip_prefix(&format!("{key}=")).map(str::to_owned))
        .unwrap_or_default()
}
#[derive(Deserialize)]
pub struct LogQuery {
    source: String,
}
#[derive(Serialize)]
struct LogPart {
    source: String,
    text: String,
}
fn redact(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if [
        "api-key=",
        "api_key=",
        "authorization:",
        "bearer ",
        "secret=",
        "token=",
        "password=",
    ]
    .iter()
    .any(|x| lower.contains(x))
    {
        return "[sensitive log line redacted]".into();
    }
    line.split_whitespace()
        .map(|s| {
            if s.contains("://") {
                "[endpoint redacted]"
            } else {
                s
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}
fn tail(logdir: &Path, name: &str) -> Result<String, ()> {
    let base = logdir.canonicalize().map_err(|_| ())?;
    let path = logdir.join(name).canonicalize().map_err(|_| ())?;
    if !path.starts_with(&base) {
        return Err(());
    }
    let mut f = fs::File::open(path).map_err(|_| ())?;
    let len = f.metadata().map_err(|_| ())?.len();
    if !f.metadata().map_err(|_| ())?.is_file() {
        return Err(());
    }
    f.seek(SeekFrom::Start(len.saturating_sub(65536)))
        .map_err(|_| ())?;
    let mut b = Vec::new();
    f.take(65536).read_to_end(&mut b).map_err(|_| ())?;
    let text = String::from_utf8_lossy(&b);
    let lines: Vec<_> = text.lines().collect();
    Ok(lines
        .iter()
        .skip(lines.len().saturating_sub(200))
        .map(|s| redact(s))
        .collect::<Vec<_>>()
        .join("\n"))
}
pub async fn logs(
    State(app): State<Arc<AppState>>,
    h: HeaderMap,
    Query(q): Query<LogQuery>,
) -> Response {
    if let Err(s) = authorized(&app, &h, false) {
        return s.into_response();
    }
    let names = match q.source.as_str() {
        "project0" => vec![("project0", "capture-latest.log")],
        "bsc" => vec![
            ("bsc-readiness", "bsc-readiness-latest.log"),
            ("bsc-reporter", "bsc-reporter.log"),
        ],
        "gateway" => vec![("gateway", "gateway.log")],
        "all" => vec![
            ("project0", "capture-latest.log"),
            ("gateway", "gateway.log"),
            ("bsc-readiness", "bsc-readiness-latest.log"),
            ("bsc-reporter", "bsc-reporter.log"),
        ],
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(paths) = &app.control.paths else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let dir = paths.logdir.clone();
    let result = tokio::task::spawn_blocking(move || {
        names
            .into_iter()
            .map(|(source, name)| LogPart {
                source: source.into(),
                text: tail(&dir, name).unwrap_or_else(|_| "Log not available".into()),
            })
            .collect::<Vec<_>>()
    })
    .await;
    match result {
        Ok(parts) => (
            [(axum::http::header::CACHE_CONTROL, "no-store")],
            Json(parts),
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ledger(name: &str) -> (Control, PathBuf) {
        let dir = std::env::temp_dir().join(format!("gw-ledger-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let paths = Paths {
            ctl: dir.join("ctl"),
            rundir: dir.clone(),
            logdir: dir.clone(),
            label: "test".into(),
        };
        let control = Control {
            capacity: CAPACITY,
            ..Control::new(Some(paths))
        };
        (control, dir)
    }
    /// Same retention logic as MAX_RECORDS, small enough for fsync-bound tests.
    const CAPACITY: usize = 16;
    fn uuid(n: usize) -> String {
        format!("00000000-0000-4000-8000-{n:012x}")
    }
    fn capture_start(id: &str) -> CommandRequest {
        CommandRequest {
            project: ProjectId::Project0 as i32,
            request_id: id.into(),
            action: START.into(),
            issued_at_unix_ns: 1,
            ..Default::default()
        }
    }
    /// The request `put(n)` records: issued in the order of n.
    fn stored(n: usize) -> CommandRequest {
        CommandRequest {
            issued_at_unix_ns: 1_000 + n as i64,
            ..capture_start(&uuid(n))
        }
    }
    /// One recorded command with a strictly increasing last-update time.
    fn put(control: &Control, n: usize, phase: CommandPhase) -> Result<(), ()> {
        let id = uuid(n);
        let record = Record {
            request: stored(n).encode_to_vec(),
            response: reply(
                &id,
                CommandStatus::Accepted,
                CommandRejectReason::Unspecified,
                "recorded",
            )
            .encode_to_vec(),
            phase: phase as i32,
            detail: String::new(),
            launch_id: String::new(),
        };
        control.persist(&id, &record)?;
        fs::File::options()
            .write(true)
            .open(control.record_path(&id).unwrap())
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_000_000 + n as u64))
            .unwrap();
        Ok(())
    }
    #[tokio::test]
    async fn a_full_ledger_archives_the_oldest_receipt_which_still_replays_and_conflicts() {
        let (control, dir) = ledger("full");
        // The oldest record is an open intent; the oldest receipt is the second one.
        put(&control, 0, CommandPhase::PendingReconcile).unwrap();
        for n in 1..CAPACITY {
            put(&control, n, CommandPhase::Completed).unwrap();
        }
        put(&control, CAPACITY, CommandPhase::Completed).unwrap();
        assert_eq!(records(&dir.join("commands")).unwrap().len(), CAPACITY);
        assert!(control.record_path(&uuid(0)).unwrap().exists());
        assert!(dir
            .join(format!("commands/archive/{}.json", uuid(1)))
            .exists());
        let replay = control.execute(&stored(1)).await;
        assert_eq!(replay.status, CommandStatus::Accepted as i32);
        assert_eq!(replay.phase, CommandPhase::Completed as i32);
        let other = CommandRequest {
            action: STOP.into(),
            ..stored(1)
        };
        let conflict = control.execute(&other).await;
        assert_eq!(
            conflict.reject_reason,
            CommandRejectReason::IdempotencyConflict as i32
        );
        fs::remove_dir_all(dir).unwrap();
    }
    /// A dispatching ledger whose stub controller counts executions; `restart` reopens it the way
    /// a new gateway process does (only the files carry over).
    struct Env {
        dir: PathBuf,
        runs: PathBuf,
    }
    impl Env {
        fn new(name: &str) -> Self {
            use std::os::unix::fs::PermissionsExt;
            let dir = std::env::temp_dir().join(format!("gw-ledger-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join("bsc-root")).unwrap();
            fs::create_dir_all(dir.join("run")).unwrap();
            fs::write(dir.join("bsc-root/SNAPSHOT_SHA256"), "a").unwrap();
            fs::write(dir.join("g07_pre_live_readiness"), "").unwrap();
            let runs = dir.join("runs");
            fs::write(
                dir.join("darkbotctl"),
                format!("#!/bin/sh\nprintf '%s\\n' \"$6\" >> '{}'\n", runs.display()),
            )
            .unwrap();
            fs::set_permissions(dir.join("darkbotctl"), fs::Permissions::from_mode(0o755)).unwrap();
            Env { dir, runs }
        }
        fn restart(&self) -> Control {
            Control {
                capacity: CAPACITY,
                ..Control::new(Some(Paths {
                    ctl: self.dir.join("darkbotctl"),
                    rundir: self.dir.join("run"),
                    logdir: self.dir.clone(),
                    label: "test.capture".into(),
                }))
            }
        }
        fn executions(&self) -> usize {
            fs::read_to_string(&self.runs)
                .unwrap_or_default()
                .lines()
                .count()
        }
        fn ledger(&self) -> PathBuf {
            self.dir.join("run/commands")
        }
    }
    impl Drop for Env {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
    fn candidate(n: usize, issued_at_unix_ns: i64) -> CommandRequest {
        CommandRequest {
            project: ProjectId::Bsc as i32,
            request_id: uuid(n),
            action: BSC_CANDIDATE.into(),
            resource_id: "SELF_B".into(),
            expected: Some(crate::proto::ExpectedVersion {
                version: Some(expected_version::Version::ProfileVersion(7)),
            }),
            params: Default::default(),
            issued_at_unix_ns,
        }
    }
    fn age(control: &Control, id: &str, secs: u64) {
        fs::File::options()
            .write(true)
            .open(control.record_path(id).unwrap())
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(secs))
            .unwrap();
    }
    fn refused(r: &CommandResponse, reason: CommandRejectReason) -> bool {
        r.status == CommandStatus::Rejected as i32 && r.reject_reason == reason as i32
    }
    #[tokio::test]
    async fn an_archived_receipt_replays_after_a_restart_and_other_bytes_conflict() {
        let env = Env::new("restart");
        let control = env.restart();
        let x = candidate(9_000, 500);
        assert_eq!(
            control.execute(&x).await.phase,
            CommandPhase::Completed as i32
        );
        age(&control, &x.request_id, 1);
        for n in 0..CAPACITY {
            put(&control, n, CommandPhase::Completed).unwrap();
        }
        assert!(env
            .ledger()
            .join(format!("archive/{}.json", x.request_id))
            .exists());
        let control = env.restart();
        let replay = control.execute(&x).await;
        assert_eq!(
            (replay.status, replay.phase),
            (
                CommandStatus::Accepted as i32,
                CommandPhase::Completed as i32
            )
        );
        for other in [
            CommandRequest {
                issued_at_unix_ns: 501,
                ..x.clone()
            },
            CommandRequest {
                resource_id: "SELF_A".into(),
                ..x.clone()
            },
        ] {
            let r = control.execute(&other).await;
            assert!(
                refused(&r, CommandRejectReason::IdempotencyConflict),
                "{}",
                r.detail
            );
        }
        assert_eq!(env.executions(), 1);
    }
    #[tokio::test]
    async fn the_retired_through_mark_survives_restarts_and_crashes_fail_closed() {
        let env = Env::new("mark");
        let control = env.restart();
        let x = candidate(9_000, 500);
        control.execute(&x).await;
        age(&control, &x.request_id, 1);
        for n in 0..2 * CAPACITY {
            put(&control, n, CommandPhase::Completed).unwrap();
        }
        let archive = env.ledger().join("archive");
        assert_eq!(retired_through(&env.ledger()), Ok(Some(500)));
        // Crash before the atomic rename: a stray temporary file changes nothing.
        fs::write(archive.join("retired_through.tmp-crashed"), "999999999999").unwrap();
        let control = env.restart();
        assert!(refused(
            &control.execute(&x).await,
            CommandRejectReason::IdempotencyConflict
        ));
        // Crash after raising the mark but before removing the receipt: it still replays.
        let kept = stored(0); // the earliest receipt still archived
        assert!(archive.join(format!("{}.json", kept.request_id)).exists());
        fs::write(
            archive.join(RETIRED_THROUGH),
            kept.issued_at_unix_ns.to_string(),
        )
        .unwrap();
        let replay = env.restart().execute(&kept).await;
        assert_eq!(
            replay.status,
            CommandStatus::Accepted as i32,
            "{}",
            replay.detail
        );
        // An unreadable mark refuses every new command instead of guessing.
        fs::write(archive.join(RETIRED_THROUGH), "not a number").unwrap();
        let now = crate::overview::unix_ns_now();
        assert!(refused(
            &env.restart().execute(&candidate(9_001, now)).await,
            CommandRejectReason::EngineUnavailable
        ));
        assert_eq!(env.executions(), 1);
    }
    #[tokio::test]
    async fn new_commands_run_in_any_stamp_order_above_the_mark_and_within_the_clock_window() {
        let env = Env::new("clock");
        let control = env.restart();
        control.execute(&candidate(9_000, 500)).await;
        age(&control, &uuid(9_000), 1);
        for n in 0..2 * CAPACITY {
            put(&control, n, CommandPhase::Completed).unwrap();
        }
        let now = crate::overview::unix_ns_now();
        // Every new command retires one more receipt, so the mark rises with them; client stamps
        // only have to be later than it: unordered, behind by an hour or ahead by 2 s.
        for (n, issued) in [
            (1, now - 5_000_000_000),
            (2, now - 10_000_000_000),
            (3, now - 3_600_000_000_000),
            (4, now + 2_000_000_000),
        ] {
            let r = control.execute(&candidate(10_000 + n, issued)).await;
            assert_eq!(r.phase, CommandPhase::Completed as i32, "{n}: {}", r.detail);
        }
        assert_eq!(env.executions(), 5);
        // At or below the mark, or ahead of the gateway beyond the liveness window: refused.
        let mark = retired_through(&env.ledger()).unwrap().unwrap();
        let at_mark = control.execute(&candidate(10_010, mark)).await;
        assert!(refused(&at_mark, CommandRejectReason::IdempotencyConflict));
        let ahead = control
            .execute(&candidate(10_011, now + 120_000_000_000))
            .await;
        assert!(refused(&ahead, CommandRejectReason::InvalidParams));
        assert_eq!(env.executions(), 5);
    }
    /// Limit of a finite retention (Wire V1.3 §7): once a receipt is retired, its id is only known
    /// through the mark. Bytes stamped at or before it stay refused; the same id stamped later is
    /// a different payload and counts as a new intent — the retired one itself never runs again.
    #[tokio::test]
    async fn beyond_the_retention_only_a_later_stamped_reuse_of_an_id_is_a_new_intent() {
        let env = Env::new("reuse");
        let control = env.restart();
        let x = candidate(9_000, 500);
        control.execute(&x).await;
        age(&control, &x.request_id, 1);
        for n in 0..2 * CAPACITY {
            put(&control, n, CommandPhase::Completed).unwrap();
        }
        let earlier = CommandRequest {
            issued_at_unix_ns: 499,
            ..x.clone()
        };
        assert!(refused(
            &control.execute(&earlier).await,
            CommandRejectReason::IdempotencyConflict
        ));
        assert_eq!(env.executions(), 1);
        let later = CommandRequest {
            issued_at_unix_ns: crate::overview::unix_ns_now(),
            ..x.clone()
        };
        assert_eq!(
            control.execute(&later).await.phase,
            CommandPhase::Completed as i32
        );
        assert_eq!(env.executions(), 2);
    }
    #[tokio::test]
    async fn open_intents_survive_a_restart_and_are_never_dispatched_again() {
        let env = Env::new("open");
        let control = env.restart();
        put(&control, 0, CommandPhase::PendingReconcile).unwrap();
        for n in 1..CAPACITY {
            put(&control, n, CommandPhase::Completed).unwrap();
        }
        for n in CAPACITY..3 * CAPACITY {
            put(&control, n, CommandPhase::Failed).unwrap();
        }
        let control = env.restart();
        assert!(control.record_path(&uuid(0)).unwrap().exists());
        let replay = control.execute(&stored(0)).await;
        assert_eq!(replay.phase, CommandPhase::PendingReconcile as i32);
        let lookup = control.lookup(&uuid(0), ProjectId::Project0 as i32).await;
        assert!(lookup.found);
        assert_eq!(env.executions(), 0);
    }
    /// The receipt of an executed command leaves the bounded archive; the exact same request
    /// arriving again must not run a second time, while a fresh command still runs.
    #[tokio::test]
    async fn a_command_whose_receipt_left_the_archive_is_never_executed_again() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("gw-ledger-retired-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("bsc-root")).unwrap();
        fs::write(dir.join("bsc-root/SNAPSHOT_SHA256"), "a").unwrap();
        fs::write(dir.join("g07_pre_live_readiness"), "").unwrap();
        let (ctl, runs) = (dir.join("darkbotctl"), dir.join("runs"));
        fs::write(
            &ctl,
            format!("#!/bin/sh\nprintf '%s\\n' \"$6\" >> '{}'\n", runs.display()),
        )
        .unwrap();
        fs::set_permissions(&ctl, fs::Permissions::from_mode(0o755)).unwrap();
        fs::create_dir_all(dir.join("run")).unwrap();
        let control = Control {
            capacity: CAPACITY,
            ..Control::new(Some(Paths {
                ctl,
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "test.capture".into(),
            }))
        };
        let candidate = |n: usize, issued_at_unix_ns| CommandRequest {
            project: ProjectId::Bsc as i32,
            request_id: uuid(n),
            action: BSC_CANDIDATE.into(),
            resource_id: "SELF_B".into(),
            expected: Some(crate::proto::ExpectedVersion {
                version: Some(expected_version::Version::ProfileVersion(7)),
            }),
            params: Default::default(),
            issued_at_unix_ns,
        };
        let executions = || {
            fs::read_to_string(&runs)
                .unwrap_or_default()
                .lines()
                .count()
        };
        let first = candidate(9_999, 500);
        assert_eq!(
            control.execute(&first).await.phase,
            CommandPhase::Completed as i32
        );
        assert_eq!(executions(), 1);
        fs::File::options()
            .write(true)
            .open(control.record_path(&first.request_id).unwrap())
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(1))
            .unwrap();
        for n in 0..2 * CAPACITY {
            put(&control, n, CommandPhase::Completed).unwrap();
        }
        assert!(
            control.load(&first.request_id).unwrap().is_none(),
            "receipt left the archive"
        );
        let replay = control.execute(&first).await;
        assert_eq!(
            (replay.status, replay.reject_reason),
            (
                CommandStatus::Rejected as i32,
                CommandRejectReason::IdempotencyConflict as i32
            ),
            "{}",
            replay.detail
        );
        assert_eq!(executions(), 1, "a retired command never runs twice");
        // No permanent refusal: a fresh command runs; a future-dated one cannot move the mark.
        let now = crate::overview::unix_ns_now();
        assert_eq!(
            control.execute(&candidate(10_000, now)).await.phase,
            CommandPhase::Completed as i32
        );
        let future = control
            .execute(&candidate(10_001, now + 60_000_000_000))
            .await;
        assert_eq!(future.status, CommandStatus::Rejected as i32);
        assert_eq!(executions(), 2);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn a_ledger_of_open_intents_refuses_a_new_one_and_drops_nothing() {
        let (control, dir) = ledger("open");
        for n in 0..CAPACITY {
            put(&control, n, CommandPhase::Pending).unwrap();
        }
        assert!(put(&control, CAPACITY, CommandPhase::Completed).is_err());
        assert_eq!(records(&dir.join("commands")).unwrap().len(), CAPACITY);
        assert!(!dir.join("commands/archive").exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn the_archive_keeps_the_newest_receipts_and_drops_only_its_oldest() {
        let (control, dir) = ledger("archive");
        for n in 0..=2 * CAPACITY {
            put(&control, n, CommandPhase::Failed).unwrap();
        }
        assert_eq!(records(&dir.join("commands")).unwrap().len(), CAPACITY);
        assert_eq!(
            records(&dir.join("commands/archive")).unwrap().len(),
            CAPACITY
        );
        assert!(control.load(&uuid(0)).unwrap().is_none());
        assert!(control.load(&uuid(1)).unwrap().is_some());
        // The mark of the removed receipt is durable; its exact replay is refused, never run.
        assert_eq!(retired_through(&dir.join("commands")), Ok(Some(1_000)));
        assert!(control.load(&uuid(2 * CAPACITY)).unwrap().is_some());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn bsc_commands_are_strictly_read_only() {
        for action in [
            "g07",
            "arm",
            "probe",
            "send",
            "bsc.g07",
            "bsc.arm",
            "bsc.probe",
            "bsc.send",
            "bsc.dry.start",
            START,
        ] {
            assert!(!allowed(ProjectId::Bsc as i32, action));
        }
        assert!(allowed(ProjectId::Bsc as i32, BSC_START));
        assert!(allowed(ProjectId::Bsc as i32, BSC_STOP));
        assert!(allowed(ProjectId::Bsc as i32, BSC_CANDIDATE));
        assert!(!allowed(ProjectId::Project0 as i32, BSC_CANDIDATE));
        assert!(!allowed(ProjectId::Solana as i32, BSC_START));
    }
    #[test]
    fn a_candidate_request_takes_only_a_fixed_profile_and_the_generation() {
        let request = |resource: &str,
                       expected: Option<expected_version::Version>,
                       params: &[u8]| CommandRequest {
            project: ProjectId::Bsc as i32,
            request_id: "12345678-1234-1234-1234-123456789abc".into(),
            action: BSC_CANDIDATE.into(),
            resource_id: resource.into(),
            expected: expected.map(|v| crate::proto::ExpectedVersion { version: Some(v) }),
            params: params.to_vec().into(),
            issued_at_unix_ns: 0,
        };
        use expected_version::Version::*;
        assert_eq!(
            candidate_request(&request("SELF_B", Some(ProfileVersion(3)), b"")),
            Some(("SELF_B", 3))
        );
        assert_eq!(
            candidate_request(&request("SELF_A", Some(ProfileVersion(0)), b"")),
            Some(("SELF_A", 0))
        );
        for bad in [
            request("SELF_C", Some(ProfileVersion(3)), b""),
            request("../SELF_A", Some(ProfileVersion(3)), b""),
            request("", Some(ProfileVersion(3)), b""),
            request("SELF_A", None, b""),
            request("SELF_A", Some(ConfigVersion(3)), b""),
            request("SELF_A", Some(ProfileVersion(3)), b"x"),
        ] {
            assert_eq!(candidate_request(&bad), None);
        }
    }
    #[tokio::test]
    async fn a_candidate_request_dispatches_one_fixed_native_command() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("gw-candidate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("bsc-root")).unwrap();
        fs::write(dir.join("bsc-root/SNAPSHOT_SHA256"), "a").unwrap();
        fs::write(dir.join("g07_pre_live_readiness"), "").unwrap();
        let ctl = dir.join("darkbotctl");
        let log = dir.join("argv");
        fs::write(
            &ctl,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n[ \"$4\" = SELF_B ]\n",
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&ctl, fs::Permissions::from_mode(0o755)).unwrap();
        let control = Control::new(Some(Paths {
            ctl,
            rundir: dir.join("run"),
            logdir: dir.clone(),
            label: "test.capture".into(),
        }));
        fs::create_dir_all(dir.join("run")).unwrap();
        let mut req = CommandRequest {
            project: ProjectId::Bsc as i32,
            request_id: "12345678-1234-1234-1234-123456789abc".into(),
            action: BSC_CANDIDATE.into(),
            resource_id: "SELF_B".into(),
            expected: Some(crate::proto::ExpectedVersion {
                version: Some(expected_version::Version::ProfileVersion(7)),
            }),
            params: Default::default(),
            issued_at_unix_ns: 0,
        };
        let response = control.execute(&req).await;
        assert_eq!(
            response.status,
            CommandStatus::Accepted as i32,
            "{}",
            response.detail
        );
        // The response already carries the decided ledger phase; a replay returns it again.
        assert_eq!(response.phase, CommandPhase::Completed as i32);
        assert_eq!(
            control.execute(&req).await.phase,
            CommandPhase::Completed as i32
        );
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            "bsc\ncandidate\nrequest\nSELF_B\n7\n12345678-1234-1234-1234-123456789abc\n"
        );
        assert_eq!(
            control
                .lookup(&req.request_id, ProjectId::Bsc as i32)
                .await
                .phase,
            CommandPhase::Completed as i32
        );
        // A native refusal is a failed receipt, never a completed switch.
        req.request_id = "22345678-1234-1234-1234-123456789abc".into();
        req.resource_id = "SELF_A".into();
        let refused = control.execute(&req).await;
        assert_eq!(
            (refused.status, refused.phase),
            (CommandStatus::Accepted as i32, CommandPhase::Failed as i32)
        );
        assert_eq!(
            control
                .lookup(&req.request_id, ProjectId::Bsc as i32)
                .await
                .phase,
            CommandPhase::Failed as i32
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn a_completed_blocked_readiness_run_is_stopped_not_failed() {
        let dir = std::env::temp_dir().join(format!("gw-blocked-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("run")).unwrap();
        fs::write(
            dir.join("run/bsc.readiness.launch"),
            "launch_id=job-1\nstarted_unix_s=1\n",
        )
        .unwrap();
        fs::write(
            dir.join("run/bsc.readiness.result"),
            "launch_id=job-1\nended_unix_s=2\nresult_code=BLOCKED\nexit_code=2\n",
        )
        .unwrap();
        let control = Control::new(Some(Paths {
            ctl: dir.join("darkbotctl"),
            rundir: dir.join("run"),
            logdir: dir.clone(),
            label: "com.darkbot.test.absent.capture".into(),
        }));
        let s = control.observe_project(ProjectId::Bsc as i32).await;
        assert_eq!((s.state.as_str(), s.run_id.as_str()), ("STOPPED", "job-1"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn rejects_path_ids_and_redacts_provider_credentials() {
        assert!(valid_id("12345678-1234-1234-1234-123456789abc"));
        assert!(!valid_id("../../capture"));
        assert!(!redact("error wss://provider/path-secret").contains("path-secret"));
        assert!(!redact("api-key=TOPSECRET").contains("TOPSECRET"));
    }
}

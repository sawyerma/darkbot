//! Authenticated local capture control. No trading, wallet, arbitrary paths or shell commands.
use crate::{
    config::Limits,
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
/// B03/U-9: offline analytics jobs of the Project 0 job controller (`p0ctl job …`).
const BACKTEST_START: &str = "project0.analytics.backtest.start";
const OPTIMIZE_START: &str = "project0.analytics.optimize.start";
const JOB_CANCEL: &str = "project0.analytics.job.cancel";
/// A08/F-02: the post-capture chain (verification, report, qualification) of a recorded run.
const VERIFY_START: &str = "project0.capture.verify.start";
/// S1-02: a data source's operating mode (`p0ctl source set`); the running owner decides.
const SOURCE_SET: &str = "project0.source.set";
const JOB_ACTIONS: [&str; 5] = [
    BACKTEST_START,
    OPTIMIZE_START,
    JOB_CANCEL,
    VERIFY_START,
    SOURCE_SET,
];
/// Online strict verification reads the chain with the operator's provider evidence: a terminal
/// action (`p0ctl job start post-capture --provider-evidence`), never a GUI one.
fn online_strict(req: &CommandRequest) -> bool {
    use crate::proto::project0::v1::PostCaptureJobCommand;
    req.action == VERIFY_START
        && PostCaptureJobCommand::decode(req.params.as_ref())
            .is_ok_and(|c| c.chain_verify == "strict" && !c.offline)
}
fn allowed(project: i32, action: &str) -> bool {
    (project == ProjectId::Project0 as i32
        && ([START, STOP].contains(&action) || JOB_ACTIONS.contains(&action)))
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

/// A run id under the project's run roots: never a path.
fn run_id_ok(s: &str) -> bool {
    (1..=128).contains(&s.len())
        && !s.starts_with(['.', '-'])
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
/// An exact USD decimal as `p0ctl` reads it: digits, at most 6 fraction digits.
fn usd_ok(s: &str) -> bool {
    let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
    (1..=15).contains(&whole.len())
        && whole.bytes().all(|b| b.is_ascii_digit())
        && (!s.contains('.') || (1..=6).contains(&fraction.len()))
        && fraction.bytes().all(|b| b.is_ascii_digit())
}
/// A funding class name (`FundingClass::as_str`); the controller checks it is one.
fn class_ok(s: &str) -> bool {
    (1..=32).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}
fn classes(set: &[String]) -> Option<String> {
    if set.len() > 8 || !set.iter().all(|c| class_ok(c)) {
        return None;
    }
    Some(if set.is_empty() {
        "all".into()
    } else {
        set.join(",")
    })
}
/// The options of one backtest point (`p0ctl backtest`'s flags).
fn point_args(c: &crate::proto::project0::v1::BacktestJobCommand) -> Option<Vec<String>> {
    if !run_id_ok(&c.run_id)
        || !usd_ok(&c.min_profit_usd)
        || !(1..=32).contains(&c.capital_usd.len())
        || c.capital_usd.contains(&0)
        || c.max_seconds == 0
        || c.max_ram_bytes == 0
    {
        return None;
    }
    let mut args = vec![
        "--run".to_string(),
        c.run_id.clone(),
        "--min-profit-usd".into(),
        c.min_profit_usd.clone(),
        "--capital-usd".into(),
        c.capital_usd
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(","),
        "--funding".into(),
        classes(&c.funding)?,
        "--max-seconds".into(),
        c.max_seconds.to_string(),
        "--max-ram-bytes".into(),
        c.max_ram_bytes.to_string(),
    ];
    if !c.verification_result.is_empty() {
        if !valid_job_id(&c.verification_result) {
            return None;
        }
        args.extend([
            "--verification-result".into(),
            c.verification_result.clone(),
        ]);
    }
    Some(args)
}
/// A job id of the controller (`[A-Za-z0-9-]`, at most 64).
fn valid_job_id(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && !s.starts_with('-')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
/// B03/U-9: the `darkbotctl p0 job …` arguments of an analytics job action — the typed
/// parameters mapped 1:1 onto the options of `p0ctl job`, which validates them again. A start's
/// job id is the request id; a cancel names its job as the resource.
fn job_args(req: &CommandRequest) -> Option<Vec<String>> {
    use crate::proto::project0::v1::{
        BacktestJobCommand, OptimizeJobCommand, PostCaptureJobCommand, SourceControlCommand,
    };
    // CTRL-03: a source change carries its CAS — the configuration version its view showed
    // (`LiquidatorState.source_config_version`), required here (review 5353cdcb); `p0ctl source
    // set --expect-version` checks it. No other action takes an expectation.
    let expect_version = match req.expected.as_ref().map(|e| e.version) {
        None => None,
        Some(Some(expected_version::Version::ConfigVersion(v))) if req.action == SOURCE_SET => {
            Some(v)
        }
        Some(_) => return None,
    };
    let start = |kind: &str| -> Vec<String> {
        ["p0", "job", "start", kind, "--id", &req.request_id]
            .map(String::from)
            .to_vec()
    };
    match req.action.as_str() {
        BACKTEST_START if req.resource_id.is_empty() => {
            let c = BacktestJobCommand::decode(req.params.as_ref()).ok()?;
            let mut args = start("backtest");
            args.extend(point_args(&c)?);
            Some(args)
        }
        OPTIMIZE_START if req.resource_id.is_empty() => {
            let c = OptimizeJobCommand::decode(req.params.as_ref()).ok()?;
            let mut args = start("optimize");
            args.extend(point_args(c.baseline.as_ref()?)?);
            if !(1..=64).contains(&c.min_profit_usd_grid.len())
                || !c.min_profit_usd_grid.iter().all(|v| usd_ok(v))
                || !(1..=16).contains(&c.funding_sets.len())
                || c.holdout_bps == 0
                || c.max_points == 0
            {
                return None;
            }
            let sets = c
                .funding_sets
                .iter()
                .map(|s| classes(&s.classes))
                .collect::<Option<Vec<_>>>()?;
            args.extend([
                "--min-profit-usd-grid".into(),
                c.min_profit_usd_grid.join(","),
                "--funding-sets".into(),
                sets.join(";"),
                "--holdout-bps".into(),
                c.holdout_bps.to_string(),
                "--max-points".into(),
                c.max_points.to_string(),
            ]);
            if c.save_profile {
                args.push("--save-profile".into());
            }
            Some(args)
        }
        VERIFY_START if req.resource_id.is_empty() => {
            let c = PostCaptureJobCommand::decode(req.params.as_ref()).ok()?;
            let offline_strict = c.chain_verify == "strict" && c.offline;
            if !run_id_ok(&c.run_id)
                || !(c.chain_verify == "off" || offline_strict)
                || (c.offline && c.chain_verify == "off")
                || c.max_seconds == 0
                || c.max_ram_bytes == 0
            {
                return None;
            }
            let mut args = start("post-capture");
            args.extend([
                "--run".into(),
                c.run_id,
                "--chain-verify".into(),
                c.chain_verify,
                "--max-seconds".into(),
                c.max_seconds.to_string(),
                "--max-ram-bytes".into(),
                c.max_ram_bytes.to_string(),
            ]);
            if c.offline {
                args.push("--offline".into());
            }
            if c.resume_verification {
                args.push("--resume-verification".into());
            }
            Some(args)
        }
        SOURCE_SET if req.resource_id.is_empty() => {
            let c = SourceControlCommand::decode(req.params.as_ref()).ok()?;
            let unit_ok = c.unit == "wss"
                || c.unit.strip_prefix("geyser:").is_some_and(|p| {
                    (1..=32).contains(&p.len())
                        && p.bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                });
            if !unit_ok || !["off", "observe", "active"].contains(&c.mode.as_str()) {
                return None;
            }
            let mut args: Vec<String> = ["p0", "source", "set", &c.unit, &c.mode]
                .map(String::from)
                .to_vec();
            args.extend(["--expect-version".into(), expect_version?.to_string()]);
            Some(args)
        }
        JOB_CANCEL if req.params.is_empty() && valid_job_id(&req.resource_id) => Some(
            ["p0", "job", "cancel", "--id", &req.resource_id]
                .map(String::from)
                .to_vec(),
        ),
        _ => None,
    }
}
/// The native refusal of `p0ctl` (its last error line), bounded and redacted.
fn refusal(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let line = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("no reason reported");
    let code = redact(line.trim().trim_start_matches("p0ctl: "));
    let code: String = code.chars().take(200).collect();
    format!("Native controller refused: {code}")
}

#[derive(Clone)]
pub struct Paths {
    pub ctl: PathBuf,
    pub rundir: PathBuf,
    pub logdir: PathBuf,
    pub label: String,
}
/// Time-bounded idempotency (Wire V1.3 §7), all bounds from `Limits` (`command_*`). Every open
/// record (PENDING / PENDING_RECONCILE) stays in `commands/`. At `command_ledger_records` the
/// oldest terminal receipt (COMPLETED / FAILED, by last update) moves to `commands/archive/`, which
/// replay and conflict checks still read. A receipt leaves the full archive only once its last
/// update is at least `command_retention_ms` old, the one issued earliest first, after
/// `retired_through` durably took its `issued_at_unix_ns`; with none that old, new commands are
/// refused instead of forgetting a younger receipt. A new request is accepted only with
/// `issued_at_unix_ns` within `command_clock_window_ms` of the gateway receive time, so a
/// forgotten request is always outside the window and never runs again; the mark refuses it too.
pub struct Control {
    paths: Option<Paths>,
    serial: Mutex<()>,
    capacity: usize,
    window_ns: u64,
    retention: Duration,
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
#[derive(Debug, PartialEq, Deserialize)]
struct Record {
    request: Vec<u8>,
    response: Vec<u8>,
    phase: i32,
    detail: String,
    launch_id: String,
}
/// BSC-051: one ledger record, binary and versioned (`DBCMD001`): the phase (i32 LE), then the
/// request, response, detail and launch ID, each with a u32-LE length, then a CRC-32 (IEEE) of
/// everything before it. Records written before it (`<id>.json`) stay readable and are never
/// written again; the next update of such a record replaces it with its binary form.
mod ledger_record {
    use super::Record;
    use std::path::Path;
    pub const EXT: &str = "cmd";
    pub const LEGACY_EXT: &str = "json";
    const MAGIC: &[u8; 8] = b"DBCMD001";

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for b in bytes {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    pub fn encode(r: &Record) -> Result<Vec<u8>, ()> {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&r.phase.to_le_bytes());
        for field in [
            &r.request[..],
            &r.response,
            r.detail.as_bytes(),
            r.launch_id.as_bytes(),
        ] {
            out.extend_from_slice(&u32::try_from(field.len()).map_err(|_| ())?.to_le_bytes());
            out.extend_from_slice(field);
        }
        let crc = crc32(&out);
        out.extend_from_slice(&crc.to_le_bytes());
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Record, ()> {
        let body_len = bytes.len().checked_sub(4).ok_or(())?;
        let (body, crc) = bytes.split_at(body_len);
        if crc32(body).to_le_bytes() != crc || body.get(..8) != Some(&MAGIC[..]) {
            return Err(());
        }
        let mut rest = &body[8..];
        let mut take = |n: usize| -> Result<&[u8], ()> {
            let (head, tail) = (rest.get(..n).ok_or(())?, rest.get(n..).ok_or(())?);
            rest = tail;
            Ok(head)
        };
        let phase = i32::from_le_bytes(take(4)?.try_into().map_err(|_| ())?);
        let mut field = || -> Result<Vec<u8>, ()> {
            let n = u32::from_le_bytes(take(4)?.try_into().map_err(|_| ())?);
            Ok(take(usize::try_from(n).map_err(|_| ())?)?.to_vec())
        };
        let request = field()?;
        let response = field()?;
        let detail = String::from_utf8(field()?).map_err(|_| ())?;
        let launch_id = String::from_utf8(field()?).map_err(|_| ())?;
        if !rest.is_empty() {
            return Err(());
        }
        Ok(Record {
            request,
            response,
            phase,
            detail,
            launch_id,
        })
    }

    /// A record file of either generation, by its extension.
    pub fn read(path: &Path) -> Result<Record, ()> {
        let bytes = std::fs::read(path).map_err(|_| ())?;
        match path.extension().and_then(|x| x.to_str()) {
            Some(EXT) => decode(&bytes),
            Some(LEGACY_EXT) => serde_json::from_slice(&bytes).map_err(|_| ()),
            _ => Err(()),
        }
    }
}
/// BSC-053: the wall clock in ms; `None` when it is before 1970 (never time 0).
fn now_ms() -> Option<u64> {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(since.as_millis()).ok()
}
/// Stamps a process observation; without a clock it is UNKNOWN, never a state at time 0.
fn observed_now(mut snapshot: ProcessSnapshot) -> ProcessSnapshot {
    stamp(&mut snapshot, now_ms());
    snapshot
}
fn stamp(snapshot: &mut ProcessSnapshot, now: Option<u64>) {
    match now {
        Some(ms) => snapshot.observed_unix_ms = ms,
        None => {
            snapshot.state = "UNKNOWN".into();
            snapshot.reason = "gateway clock unavailable".into();
            snapshot.observed_unix_ms = 0;
        }
    }
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
    pub fn from_env(limits: &Limits) -> Self {
        let paths = (|| {
            Some(Paths {
                ctl: std::env::var_os("DARKBOT_CTL")?.into(),
                rundir: std::env::var_os("DARKBOT_RUNDIR")?.into(),
                logdir: std::env::var_os("DARKBOT_LOGDIR")?.into(),
                label: std::env::var("DARKBOT_CAPTURE_LABEL").ok()?,
            })
        })();
        Self::new(paths, limits)
    }
    pub fn new(paths: Option<Paths>, limits: &Limits) -> Self {
        Self {
            paths,
            serial: Mutex::new(()),
            capacity: limits.command_ledger_records,
            window_ns: limits.command_clock_window_ms.saturating_mul(1_000_000),
            retention: Duration::from_millis(limits.command_retention_ms),
        }
    }
    pub async fn observe(&self) -> ProcessSnapshot {
        let mut snapshot = ProcessSnapshot {
            state: "UNKNOWN".into(),
            run_id: String::new(),
            pid: None,
            reason: "Capture controller not configured".into(),
            observed_unix_ms: 0,
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
        observed_now(snapshot)
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
            observed_unix_ms: 0,
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
        observed_now(s)
    }
    fn record_path(&self, id: &str) -> Option<PathBuf> {
        self.paths.as_ref().map(|p| {
            p.rundir
                .join("commands")
                .join(format!("{id}.{}", ledger_record::EXT))
        })
    }
    /// The record of `id`: binary before legacy, open ledger before archive. A file that exists
    /// but cannot be read or decoded makes the ledger unreadable (never "no record").
    fn load(&self, id: &str) -> Result<Option<Record>, ()> {
        let path = self.record_path(id).ok_or(())?;
        let dir = path.parent().ok_or(())?;
        for d in [dir.to_path_buf(), dir.join("archive")] {
            for ext in [ledger_record::EXT, ledger_record::LEGACY_EXT] {
                let p = d.join(format!("{id}.{ext}"));
                match fs::symlink_metadata(&p) {
                    Ok(_) => return ledger_record::read(&p).map(Some),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return Err(()),
                }
            }
        }
        Ok(None)
    }
    fn persist(&self, id: &str, record: &Record) -> Result<(), ()> {
        let path = self.record_path(id).ok_or(())?;
        let dir = path.parent().ok_or(())?;
        let legacy = dir.join(format!("{id}.{}", ledger_record::LEGACY_EXT));
        fs::create_dir_all(dir).map_err(|_| ())?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|_| ())?;
        if !path.exists() && !legacy.exists() && records(dir)?.len() >= self.capacity {
            archive_oldest_terminal(dir, self.capacity, self.retention)?;
        }
        let bytes = ledger_record::encode(record)?;
        let tmp = path.with_extension(format!("tmp-{}", session::random_token(8)));
        let written = (|| {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)
                .map_err(|_| ())?;
            f.write_all(&bytes).map_err(|_| ())?;
            f.sync_all().map_err(|_| ())?;
            fs::rename(&tmp, &path).map_err(|_| ())?;
            // The binary form replaces a legacy record of the same id.
            match fs::remove_file(&legacy) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(()),
                _ => {}
            }
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
                "Only managed Project 0 capture, its offline analytics jobs, BSC read-only readiness start/stop and the offline BSC candidate request are available",
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
        if online_strict(req) {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::GateBlocked,
                "Online strict verification needs the operator's provider evidence: p0ctl job start post-capture --provider-evidence FILE (terminal); the GUI verifies off or strict from the cache",
            );
        }
        let job = if JOB_ACTIONS.contains(&req.action.as_str()) {
            match job_args(req) {
                Some(args) => Some(args),
                None => {
                    return reply(
                        id,
                        CommandStatus::Rejected,
                        CommandRejectReason::InvalidParams,
                        "Analytics job parameters invalid (run id, exact USD, capital, funding classes, budget, grid or job id)",
                    )
                }
            }
        } else {
            None
        };
        if candidate.is_none()
            && job.is_none()
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
        // Replay rule (see Control): a new request must be issued within the clock window and after
        // every forgotten receipt.
        if let Err((reason, detail)) = clock_window(
            req.issued_at_unix_ns,
            crate::overview::unix_ns(),
            self.window_ns,
        ) {
            return reply(id, CommandStatus::Rejected, reason, &detail);
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
        if let Some(args) = job {
            return self.dispatch_job(p, id, encoded, &req.action, args).await;
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
                "Cannot durably record command (ledger full within the idempotency window, or unavailable); retry later",
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
    /// B03/U-9: an analytics job action through `darkbotctl p0 job …` (the same `p0ctl job`
    /// a terminal runs). No capture process is observed: COMPLETED means the controller recorded
    /// the job and started its worker (or signalled it to stop); the job's own progress and end
    /// are its state in project0.analytics.jobs, never this receipt.
    async fn dispatch_job(
        &self,
        p: &Paths,
        id: &str,
        encoded: Vec<u8>,
        action: &str,
        args: Vec<String>,
    ) -> CommandResponse {
        let accepted = reply(
            id,
            CommandStatus::Accepted,
            CommandRejectReason::Unspecified,
            "Analytics job intent persisted; the job's progress is project0.analytics.jobs",
        );
        let mut record = Record {
            request: encoded,
            response: accepted.encode_to_vec(),
            phase: CommandPhase::PendingReconcile as i32,
            detail: "Controller dispatch pending; never automatically replayed after interruption"
                .into(),
            launch_id: String::new(),
        };
        if self.persist(id, &record).is_err() {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::EngineUnavailable,
                "Cannot durably record command (ledger full within the idempotency window, or unavailable); retry later",
            );
        }
        let output = tokio::time::timeout(
            Duration::from_secs(50),
            Command::new(&p.ctl).args(&args).kill_on_drop(true).output(),
        )
        .await;
        match output {
            Ok(Ok(out)) if out.status.success() => {
                record.phase = CommandPhase::Completed as i32;
                record.detail = if action == JOB_CANCEL {
                    "Stop requested from the job's worker; it records CANCELLED in project0.analytics.jobs"
                } else if action == SOURCE_SET {
                    "Source request recorded and the runtime rung; the owner's decision follows in project0.liquidator.state"
                } else if action == VERIFY_START {
                    "Post-capture job recorded and its worker started; the run's qualification follows in project0.analytics.jobs"
                } else {
                    "Job recorded and its worker started; progress in project0.analytics.jobs"
                }
                .into();
            }
            Ok(Ok(out)) => {
                record.phase = CommandPhase::Failed as i32;
                record.detail = refusal(&out.stderr);
            }
            _ => {
                record.detail =
                    "Controller outcome unknown; inspect project0.analytics.jobs before a new action"
                        .into();
            }
        }
        if self.persist(id, &record).is_err() {
            tracing::error!(request_id=%id, "command outcome could not be persisted");
        }
        tracing::info!(request_id=%id, %action, phase=record.phase, "analytics job command");
        CommandResponse {
            phase: record.phase,
            detail: record.detail,
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
/// Replay rule (see Control): a new request is issued within `window_ns` of the gateway receive
/// time. Without a gateway clock (BSC-053) no request is inside the window.
fn clock_window(
    issued_at_unix_ns: i64,
    now: Option<i64>,
    window_ns: u64,
) -> Result<(), (CommandRejectReason, String)> {
    let Some(now) = now else {
        return Err((
            CommandRejectReason::EngineUnavailable,
            "Gateway clock unavailable; nothing was executed".into(),
        ));
    };
    if issued_at_unix_ns.abs_diff(now) > window_ns {
        return Err((
            CommandRejectReason::InvalidParams,
            format!(
                "issued_at differs from the gateway clock by more than {} ms (clock skew or a stale request); issue a new command",
                window_ns / 1_000_000
            ),
        ));
    }
    Ok(())
}
/// The ledger records of one directory with their last update time.
fn records(dir: &Path) -> Result<Vec<(SystemTime, PathBuf)>, ()> {
    Ok(fs::read_dir(dir)
        .map_err(|_| ())?
        .filter_map(Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x == ledger_record::EXT || x == ledger_record::LEGACY_EXT)
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect())
}
fn terminal(path: &Path) -> bool {
    ledger_record::read(path).is_ok_and(|r| {
            r.phase == CommandPhase::Completed as i32 || r.phase == CommandPhase::Failed as i32
        })
}
fn oldest_terminal(dir: &Path) -> Result<Option<PathBuf>, ()> {
    let mut all = records(dir)?;
    all.sort();
    Ok(all.into_iter().map(|(_, p)| p).find(|p| terminal(p)))
}
fn issued_at(path: &Path) -> Option<i64> {
    let record = ledger_record::read(path).ok()?;
    CommandRequest::decode(record.request.as_slice())
        .ok()
        .map(|r| r.issued_at_unix_ns)
}
const RETIRED_THROUGH: &str = "retired_through";
/// The latest `issued_at_unix_ns` of every receipt removed from the archive; None before the first.
fn retired_through(dir: &Path) -> Result<Option<i64>, ()> {
    match fs::read_to_string(dir.join("archive").join(RETIRED_THROUGH)) {
        Ok(t) => t.trim().parse().map(Some).map_err(|_| ()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    }
}
/// Removes the archived receipt issued earliest among those last updated at least `window` ago;
/// the mark is durable before the receipt is gone. None that old: refuse (Err), never forget early.
fn retire_earliest(dir: &Path, archive: &Path, window: Duration) -> Result<(), ()> {
    let cutoff = SystemTime::now().checked_sub(window).ok_or(())?;
    let (issued, victim) = records(archive)?
        .into_iter()
        .filter(|(t, p)| *t <= cutoff && terminal(p))
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
/// Frees one ledger slot without touching an open intent; see Control.
fn archive_oldest_terminal(dir: &Path, capacity: usize, window: Duration) -> Result<(), ()> {
    let source = oldest_terminal(dir)?.ok_or(())?;
    let archive = dir.join("archive");
    fs::create_dir_all(&archive).map_err(|_| ())?;
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o700)).map_err(|_| ())?;
    if records(&archive)?.len() >= capacity {
        retire_earliest(dir, &archive, window)?;
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
    /// Ledger bounds come from `Limits` exactly as in production; only the capacity is small so
    /// that fsync-bound tests stay fast, and real-time tests shrink the clock window.
    const CAPACITY: usize = 16;
    fn limits() -> Limits {
        Limits {
            command_ledger_records: CAPACITY,
            ..Limits::default()
        }
    }
    /// Real-time ledger: a short clock window and a ledger small enough to flood well within it.
    fn short(window_ms: u64) -> Limits {
        Limits {
            command_clock_window_ms: window_ms,
            command_retention_ms: crate::config::min_command_retention_ms(window_ms),
            command_ledger_records: 4,
            ..Limits::default()
        }
    }
    fn window_ns(l: &Limits) -> i64 {
        l.command_clock_window_ms as i64 * 1_000_000
    }
    fn now() -> i64 {
        crate::overview::unix_ns_now()
    }
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
        (Control::new(Some(paths), &limits()), dir)
    }
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
    /// The request `put(n)` records: long ago, issued in the order of n.
    fn stored(n: usize) -> CommandRequest {
        CommandRequest {
            issued_at_unix_ns: 1_000 + n as i64,
            ..capture_start(&uuid(n))
        }
    }
    /// A recorded receipt issued right after `x`, so that `x` is the one issued earliest.
    fn after(x: &CommandRequest, n: usize) -> CommandRequest {
        CommandRequest {
            issued_at_unix_ns: x.issued_at_unix_ns + 1 + n as i64,
            ..capture_start(&uuid(n))
        }
    }
    /// One recorded command for `request`, last updated long ago (strictly increasing with n).
    fn put_req(
        control: &Control,
        request: &CommandRequest,
        n: usize,
        phase: CommandPhase,
    ) -> Result<(), ()> {
        let id = &request.request_id;
        let record = Record {
            request: request.encode_to_vec(),
            response: reply(
                id,
                CommandStatus::Accepted,
                CommandRejectReason::Unspecified,
                "recorded",
            )
            .encode_to_vec(),
            phase: phase as i32,
            detail: String::new(),
            launch_id: String::new(),
        };
        control.persist(id, &record)?;
        age(control, id, 1_000_000 + n as u64);
        Ok(())
    }
    fn put(control: &Control, n: usize, phase: CommandPhase) -> Result<(), ()> {
        put_req(control, &stored(n), n, phase)
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
        assert!(
            dir.join(format!("commands/archive/{}.cmd", uuid(1)))
                .exists()
        );
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
        limits: Limits,
    }
    impl Env {
        fn new(name: &str) -> Self {
            Self::with(name, limits())
        }
        fn with(name: &str, limits: Limits) -> Self {
            use std::os::unix::fs::PermissionsExt;
            // Distinct from ledger()'s directories: tests run in parallel.
            let dir = std::env::temp_dir().join(format!("gw-env-{name}-{}", std::process::id()));
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
            Env { dir, runs, limits }
        }
        fn restart(&self) -> Control {
            let paths = Paths {
                ctl: self.dir.join("darkbotctl"),
                rundir: self.dir.join("run"),
                logdir: self.dir.clone(),
                label: "test.capture".into(),
            };
            Control::new(Some(paths), &self.limits)
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
        /// Waits until every receipt recorded so far is past the retention window.
        async fn outlast_retention(&self) {
            tokio::time::sleep(Duration::from_millis(
                self.limits.command_retention_ms + 100,
            ))
            .await;
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
    fn completed(r: &CommandResponse) -> bool {
        r.status == CommandStatus::Accepted as i32 && r.phase == CommandPhase::Completed as i32
    }
    /// BSC-051: the binary ledger record round-trips and refuses any damage.
    #[test]
    fn the_binary_ledger_record_round_trips_and_refuses_damage() {
        let r = Record {
            request: vec![1, 2, 3],
            response: vec![4],
            phase: CommandPhase::Completed as i32,
            detail: "done".into(),
            launch_id: "launch".into(),
        };
        let bytes = ledger_record::encode(&r).unwrap();
        assert_eq!(&bytes[..8], b"DBCMD001");
        assert_eq!(ledger_record::decode(&bytes), Ok(r));
        for cut in [0, 8, 12, bytes.len() - 1] {
            assert!(ledger_record::decode(&bytes[..cut]).is_err(), "{cut}");
        }
        for at in [0, 9, 13, bytes.len() - 1] {
            let mut flipped = bytes.clone();
            flipped[at] ^= 1;
            assert!(ledger_record::decode(&flipped).is_err(), "{at}");
        }
        let mut longer = bytes.clone();
        longer.insert(bytes.len() - 4, 0);
        assert!(ledger_record::decode(&longer).is_err());
    }
    /// BSC-051: a receipt written before the binary ledger (JSON) still replays and conflicts after
    /// a restart; its next write replaces it with the binary form; nothing writes JSON any more.
    #[tokio::test]
    async fn a_legacy_json_receipt_replays_and_is_replaced_by_its_binary_form() {
        let env = Env::new("legacy");
        let control = env.restart();
        let x = candidate(9_100, now());
        assert!(completed(&control.execute(&x).await));
        let cmd = env.ledger().join(format!("{}.cmd", x.request_id));
        let legacy = env.ledger().join(format!("{}.json", x.request_id));
        let record = ledger_record::read(&cmd).unwrap();
        let json = serde_json::json!({
            "request": record.request,
            "response": record.response,
            "phase": record.phase,
            "detail": record.detail,
            "launch_id": record.launch_id,
        });
        fs::write(&legacy, serde_json::to_vec(&json).unwrap()).unwrap();
        fs::remove_file(&cmd).unwrap();
        let control = env.restart();
        assert!(completed(&control.execute(&x).await));
        let other = CommandRequest {
            resource_id: "SELF_A".into(),
            ..x.clone()
        };
        assert!(refused(
            &control.execute(&other).await,
            CommandRejectReason::IdempotencyConflict
        ));
        assert_eq!(env.executions(), 1);
        control.persist(&x.request_id, &record).unwrap();
        assert!(cmd.exists() && !legacy.exists());
        assert_eq!(ledger_record::read(&cmd), Ok(record));
        assert!(
            fs::read_dir(env.ledger()).unwrap().all(|e| e
                .unwrap()
                .path()
                .extension()
                .is_none_or(|x| x != "json"))
        );
    }
    /// BSC-051: a damaged record makes the ledger unreadable; it is never "no record".
    #[tokio::test]
    async fn a_damaged_ledger_record_refuses_instead_of_running_again() {
        let env = Env::new("damaged");
        let control = env.restart();
        let x = candidate(9_200, now());
        assert!(completed(&control.execute(&x).await));
        let cmd = env.ledger().join(format!("{}.cmd", x.request_id));
        let mut bytes = fs::read(&cmd).unwrap();
        let n = bytes.len();
        bytes[n - 5] ^= 1;
        fs::write(&cmd, bytes).unwrap();
        assert!(refused(
            &env.restart().execute(&x).await,
            CommandRejectReason::EngineUnavailable
        ));
        assert_eq!(env.executions(), 1);
    }
    /// BSC-053: without the gateway clock no command is admitted and no process state is
    /// reported; never time 0.
    #[test]
    fn a_missing_gateway_clock_admits_nothing_and_observes_nothing() {
        let (reason, detail) = clock_window(now(), None, 1_000_000_000).unwrap_err();
        assert_eq!(reason, CommandRejectReason::EngineUnavailable);
        assert!(detail.contains("clock unavailable"), "{detail}");
        assert!(clock_window(now(), Some(now()), 1_000_000_000).is_ok());
        let mut s = ProcessSnapshot {
            state: "RUNNING".into(),
            run_id: "r".into(),
            pid: Some(1),
            reason: String::new(),
            observed_unix_ms: 7,
            controller_available: true,
        };
        stamp(&mut s, None);
        assert_eq!((s.state.as_str(), s.observed_unix_ms), ("UNKNOWN", 0));
        stamp(&mut s, Some(5));
        assert_eq!(s.observed_unix_ms, 5);
    }
    #[tokio::test]
    async fn an_archived_receipt_replays_after_a_restart_and_other_bytes_conflict() {
        let env = Env::new("restart");
        let control = env.restart();
        let x = candidate(9_000, now());
        assert!(completed(&control.execute(&x).await));
        age(&control, &x.request_id, 1);
        for n in 0..CAPACITY {
            put(&control, n, CommandPhase::Completed).unwrap();
        }
        assert!(
            env.ledger()
                .join(format!("archive/{}.cmd", x.request_id))
                .exists()
        );
        let control = env.restart();
        assert!(completed(&control.execute(&x).await));
        for other in [
            CommandRequest {
                issued_at_unix_ns: x.issued_at_unix_ns + 1,
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
        let x = candidate(9_000, now());
        control.execute(&x).await;
        age(&control, &x.request_id, 1);
        for n in 0..2 * CAPACITY {
            put_req(&control, &after(&x, n), n, CommandPhase::Completed).unwrap();
        }
        let archive = env.ledger().join("archive");
        assert_eq!(
            retired_through(&env.ledger()),
            Ok(Some(x.issued_at_unix_ns))
        );
        // Crash before the atomic rename: a stray temporary file changes nothing.
        fs::write(archive.join("retired_through.tmp-crashed"), "999999999999").unwrap();
        let control = env.restart();
        assert!(refused(
            &control.execute(&x).await,
            CommandRejectReason::IdempotencyConflict
        ));
        // Crash after raising the mark but before removing the receipt: it still replays.
        let kept = after(&x, 0); // the earliest receipt still archived
        assert!(archive.join(format!("{}.cmd", kept.request_id)).exists());
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
        assert!(refused(
            &env.restart().execute(&candidate(9_001, now())).await,
            CommandRejectReason::EngineUnavailable
        ));
        assert_eq!(env.executions(), 1);
    }
    #[tokio::test]
    async fn new_commands_run_in_any_stamp_order_within_the_clock_window_and_never_outside() {
        let env = Env::new("clock");
        let control = env.restart();
        for n in 0..=2 * CAPACITY {
            put(&control, n, CommandPhase::Completed).unwrap();
        }
        let w = window_ns(&env.limits);
        let t = now();
        // Unordered stamps and a browser clock ahead or behind, all within the window.
        for (n, issued) in [
            (1, t - w / 6),
            (2, t - w / 3),
            (3, t + w / 15),
            (4, t - w / 2),
        ] {
            let r = control.execute(&candidate(10_000 + n, issued)).await;
            assert!(completed(&r), "{n}: {}", r.detail);
        }
        assert_eq!(env.executions(), 4);
        // Outside the window in either direction: refused, whatever the ledger holds.
        for (n, issued) in [
            (5, now() - w - 1_000_000_000),
            (6, now() + w + 1_000_000_000),
        ] {
            let r = control.execute(&candidate(10_000 + n, issued)).await;
            assert!(refused(&r, CommandRejectReason::InvalidParams), "{n}");
            assert!(r
                .detail
                .contains(&format!("{} ms", env.limits.command_clock_window_ms)));
        }
        assert_eq!(env.executions(), 4);
    }
    /// Contract limit (Wire V1.3 §7): once a receipt is forgotten its id is no longer known; the
    /// same id with a new time is other bytes and counts as a new intent, the forgotten one never.
    #[tokio::test]
    async fn after_its_retention_a_reused_id_with_a_new_time_is_a_new_intent() {
        let env = Env::new("reuse");
        let control = env.restart();
        let x = candidate(9_000, now());
        control.execute(&x).await;
        age(&control, &x.request_id, 1);
        for n in 0..2 * CAPACITY {
            put_req(&control, &after(&x, n), n, CommandPhase::Completed).unwrap();
        }
        let earlier = CommandRequest {
            issued_at_unix_ns: x.issued_at_unix_ns - 1,
            ..x.clone()
        };
        assert!(refused(
            &control.execute(&earlier).await,
            CommandRejectReason::IdempotencyConflict
        ));
        assert_eq!(env.executions(), 1);
        let later = CommandRequest {
            issued_at_unix_ns: now(),
            ..x.clone()
        };
        assert!(completed(&control.execute(&later).await));
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
        let env = Env::new("retired");
        let control = env.restart();
        let first = candidate(9_999, now());
        assert!(completed(&control.execute(&first).await));
        assert_eq!(env.executions(), 1);
        age(&control, &first.request_id, 1);
        for n in 0..2 * CAPACITY {
            put_req(&control, &after(&first, n), n, CommandPhase::Completed).unwrap();
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
        assert_eq!(env.executions(), 1, "a retired command never runs twice");
        // No permanent refusal: a fresh command runs; a future-dated one is refused.
        assert!(completed(&control.execute(&candidate(10_000, now())).await));
        let w = window_ns(&env.limits);
        let future = control.execute(&candidate(10_001, now() + 2 * w)).await;
        assert_eq!(future.status, CommandStatus::Rejected as i32);
        assert_eq!(env.executions(), 2);
    }
    /// Counter-case 1, real time: within its retention the same id never runs twice, not even with
    /// a new time and the same action; a flood is refused instead of forgetting it early. After the
    /// retention its exact bytes are outside the clock window and the mark: never again. Only the
    /// same id with a new time is then a new intent (the documented contract limit).
    #[tokio::test]
    async fn within_its_retention_a_request_id_never_runs_twice() {
        let env = Env::with("case1", short(1_000));
        let cap = env.limits.command_ledger_records;
        let control = env.restart();
        let x = candidate(9_000, now());
        assert!(completed(&control.execute(&x).await));
        for n in 0..2 * cap {
            control.execute(&candidate(20_000 + n, now())).await;
        }
        assert_eq!(
            env.executions(),
            2 * cap,
            "the last one was refused, nothing forgotten"
        );
        let same_action_new_time = CommandRequest {
            issued_at_unix_ns: now(),
            ..x.clone()
        };
        assert!(refused(
            &control.execute(&same_action_new_time).await,
            CommandRejectReason::IdempotencyConflict
        ));
        assert!(
            completed(&control.execute(&x).await),
            "replay = original receipt"
        );
        assert_eq!(env.executions(), 2 * cap);
        env.outlast_retention().await;
        assert!(completed(&control.execute(&candidate(30_000, now())).await));
        assert!(
            control.load(&x.request_id).unwrap().is_none(),
            "x forgotten only now"
        );
        assert!(refused(
            &control.execute(&x).await,
            CommandRejectReason::InvalidParams
        ));
        let reused = CommandRequest {
            issued_at_unix_ns: now(),
            ..x.clone()
        };
        assert!(completed(&control.execute(&reused).await));
        assert_eq!(env.executions(), 2 * cap + 2);
    }
    /// Counter-case 2, real time: requests stamped right below the upper clock bound fill the
    /// ledger; new commands are refused only while every receipt is younger than the retention,
    /// and afterwards the mark never blocks a current stamp.
    #[tokio::test]
    async fn stamps_at_the_upper_clock_bound_never_block_current_commands() {
        let l = short(1_000);
        let env = Env::with("case2", l.clone());
        let cap = l.command_ledger_records;
        let control = env.restart();
        let upper = || now() + window_ns(&l) * 9 / 10;
        let mut accepted = 0;
        for n in 0..=2 * cap {
            let r = control.execute(&candidate(20_000 + n, upper())).await;
            accepted += usize::from(completed(&r));
        }
        assert_eq!(
            accepted,
            2 * cap,
            "the ledger is full of receipts inside the retention"
        );
        assert_eq!(retired_through(&env.ledger()), Ok(None));
        assert!(refused(
            &control.execute(&candidate(30_000, now())).await,
            CommandRejectReason::EngineUnavailable
        ));
        env.outlast_retention().await;
        assert!(completed(&control.execute(&candidate(30_001, now())).await));
        let mark = retired_through(&env.ledger()).unwrap().unwrap();
        assert!(
            mark < now() - window_ns(&l),
            "the mark stays behind every current stamp"
        );
        assert!(completed(&control.execute(&candidate(30_002, now())).await));
    }
    /// Counter-case 3, real time: a browser clock beyond the window is refused with the same
    /// reason before and after archiving (no ledger effect) and runs once its clock is corrected.
    #[tokio::test]
    async fn a_browser_clock_beyond_the_window_is_refused_regardless_of_archiving() {
        let env = Env::with("case3", short(1_000));
        let control = env.restart();
        let hour = 3_600_000_000_000;
        let behind = control.execute(&candidate(40_000, now() - hour)).await;
        assert!(refused(&behind, CommandRejectReason::InvalidParams));
        for n in 0..2 * env.limits.command_ledger_records {
            control.execute(&candidate(40_001 + n, now())).await;
        }
        env.outlast_retention().await;
        assert!(completed(&control.execute(&candidate(41_000, now())).await));
        assert!(
            retired_through(&env.ledger()).unwrap().is_some(),
            "archiving happened"
        );
        let again = control.execute(&candidate(41_001, now() - hour)).await;
        assert!(refused(&again, CommandRejectReason::InvalidParams));
        assert_eq!(again.detail, behind.detail);
        assert!(completed(&control.execute(&candidate(41_002, now())).await));
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
    fn only_a_source_change_carries_the_configuration_version_cas() {
        use crate::proto::project0::v1::SourceControlCommand;
        use expected_version::Version::*;
        let req = |action: &str,
                   resource: &str,
                   params: Vec<u8>,
                   expected: Option<expected_version::Version>| CommandRequest {
            project: ProjectId::Project0 as i32,
            request_id: "12345678-1234-1234-1234-123456789abc".into(),
            action: action.into(),
            resource_id: resource.into(),
            expected: expected.map(|v| crate::proto::ExpectedVersion { version: Some(v) }),
            params: params.into(),
            issued_at_unix_ns: 0,
        };
        let source = SourceControlCommand {
            unit: "geyser:publicnode".into(),
            mode: "off".into(),
        }
        .encode_to_vec();
        let cas = job_args(&req(SOURCE_SET, "", source.clone(), Some(ConfigVersion(4)))).unwrap();
        assert_eq!(
            cas[3..],
            ["geyser:publicnode", "off", "--expect-version", "4"]
        );
        // Required (review 5353cdcb): a source change without its version is refused.
        assert_eq!(job_args(&req(SOURCE_SET, "", source.clone(), None)), None);
        assert_eq!(
            job_args(&req(SOURCE_SET, "", source, Some(ProfileVersion(4)))),
            None
        );
        assert_eq!(
            job_args(&req(JOB_CANCEL, "job-1", vec![], None))
                .unwrap()
                .len(),
            5
        );
        assert_eq!(
            job_args(&req(JOB_CANCEL, "job-1", vec![], Some(ConfigVersion(1)))),
            None
        );
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
        let control = Control::new(
            Some(Paths {
                ctl,
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "test.capture".into(),
            }),
            &Limits::default(),
        );
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
            issued_at_unix_ns: crate::overview::unix_ns_now(),
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
    /// B03/U-9: an analytics job action maps its typed parameters 1:1 onto the options of
    /// `p0ctl job` (through `darkbotctl p0 job`), without observing a capture process; a native
    /// refusal is a FAILED receipt with its code; invalid parameters are rejected before the
    /// ledger and never dispatched.
    #[tokio::test]
    async fn analytics_job_commands_dispatch_the_p0ctl_job_options() {
        use crate::proto::project0::v1::{BacktestJobCommand, FundingSet, OptimizeJobCommand};
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("gw-jobs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("run")).unwrap();
        let ctl = dir.join("darkbotctl");
        let log = dir.join("argv");
        fs::write(
            &ctl,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n[ \"$3\" != cancel ] || {{ echo 'p0ctl: JOB_NOT_RUNNING: COMPLETED' >&2; exit 2; }}\n",
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&ctl, fs::Permissions::from_mode(0o755)).unwrap();
        let control = Control::new(
            Some(Paths {
                ctl,
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "com.darkbot.test.absent.capture".into(),
            }),
            &Limits::default(),
        );
        let point = BacktestJobCommand {
            run_id: "run-a".into(),
            min_profit_usd: "1.5".into(),
            capital_usd: vec![100, 1000],
            funding: vec![],
            max_seconds: 60,
            max_ram_bytes: 1_048_576,
            verification_result: String::new(),
        };
        let request = |n: u8, action: &str, resource: &str, params: Vec<u8>| CommandRequest {
            project: ProjectId::Project0 as i32,
            request_id: format!("{n}2345678-1234-1234-1234-123456789abc"),
            action: action.into(),
            resource_id: resource.into(),
            expected: None,
            params: params.into(),
            issued_at_unix_ns: crate::overview::unix_ns_now(),
        };
        let argv = || fs::read_to_string(&log).unwrap();
        let bt = request(1, BACKTEST_START, "", point.encode_to_vec());
        let r = control.execute(&bt).await;
        assert_eq!(
            (r.status, r.phase),
            (
                CommandStatus::Accepted as i32,
                CommandPhase::Completed as i32
            ),
            "{}",
            r.detail
        );
        assert_eq!(
            argv(),
            "p0\njob\nstart\nbacktest\n--id\n12345678-1234-1234-1234-123456789abc\n--run\nrun-a\n--min-profit-usd\n1.5\n--capital-usd\n100,1000\n--funding\nall\n--max-seconds\n60\n--max-ram-bytes\n1048576\n"
        );
        let opt = OptimizeJobCommand {
            baseline: Some(point.clone()),
            min_profit_usd_grid: vec!["1.5".into(), "2".into()],
            funding_sets: vec![
                FundingSet { classes: vec![] },
                FundingSet {
                    classes: vec!["NO_SWAP_INVENTORY".into(), "PREHELD_INVENTORY".into()],
                },
            ],
            holdout_bps: 3000,
            max_points: 4,
            save_profile: true,
        };
        let r = control
            .execute(&request(2, OPTIMIZE_START, "", opt.encode_to_vec()))
            .await;
        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
        assert!(argv().ends_with(
            "--min-profit-usd-grid\n1.5,2\n--funding-sets\nall;NO_SWAP_INVENTORY,PREHELD_INVENTORY\n--holdout-bps\n3000\n--max-points\n4\n--save-profile\n"
        ));
        let cancel = request(
            3,
            JOB_CANCEL,
            "12345678-1234-1234-1234-123456789abc",
            vec![],
        );
        let r = control.execute(&cancel).await;
        assert_eq!(
            (r.status, r.phase),
            (CommandStatus::Accepted as i32, CommandPhase::Failed as i32)
        );
        assert_eq!(
            r.detail,
            "Native controller refused: JOB_NOT_RUNNING: COMPLETED"
        );
        assert_eq!(
            control
                .lookup(&cancel.request_id, ProjectId::Project0 as i32)
                .await
                .detail,
            r.detail
        );
        assert_eq!(
            argv(),
            "p0\njob\ncancel\n--id\n12345678-1234-1234-1234-123456789abc\n"
        );
        // A08/F-02: verification off, or strict from the cache; online strict is gate-blocked.
        use crate::proto::project0::v1::PostCaptureJobCommand;
        let verify = PostCaptureJobCommand {
            run_id: "run-a".into(),
            chain_verify: "strict".into(),
            offline: true,
            resume_verification: true,
            max_seconds: 60,
            max_ram_bytes: 1_048_576,
        };
        let r = control
            .execute(&request(5, VERIFY_START, "", verify.encode_to_vec()))
            .await;
        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
        assert_eq!(
            argv(),
            "p0\njob\nstart\npost-capture\n--id\n52345678-1234-1234-1234-123456789abc\n--run\nrun-a\n--chain-verify\nstrict\n--max-seconds\n60\n--max-ram-bytes\n1048576\n--offline\n--resume-verification\n"
        );
        let online = PostCaptureJobCommand {
            offline: false,
            ..verify.clone()
        };
        let r = control
            .execute(&request(6, VERIFY_START, "", online.encode_to_vec()))
            .await;
        assert_eq!(
            (r.status, r.reject_reason),
            (
                CommandStatus::Rejected as i32,
                CommandRejectReason::GateBlocked as i32
            )
        );
        // S1-02: a source mode request maps onto `p0ctl source set`.
        use crate::proto::project0::v1::SourceControlCommand;
        let source = SourceControlCommand {
            unit: "geyser:publicnode".into(),
            mode: "observe".into(),
        };
        let mut set = request(7, SOURCE_SET, "", source.encode_to_vec());
        set.expected = Some(crate::proto::ExpectedVersion {
            version: Some(expected_version::Version::ConfigVersion(0)),
        });
        let r = control.execute(&set).await;
        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
        assert_eq!(
            argv(),
            "p0\nsource\nset\ngeyser:publicnode\nobserve\n--expect-version\n0\n"
        );
        // Invalid parameters: rejected, no receipt, nothing dispatched.
        fs::remove_file(&log).unwrap();
        let bad = |c: BacktestJobCommand| c.encode_to_vec();
        for (n, req) in [
            (
                4,
                request(
                    4,
                    BACKTEST_START,
                    "",
                    bad(BacktestJobCommand {
                        run_id: "../x".into(),
                        ..point.clone()
                    }),
                ),
            ),
            (
                5,
                request(
                    5,
                    BACKTEST_START,
                    "",
                    bad(BacktestJobCommand {
                        min_profit_usd: "1e5".into(),
                        ..point.clone()
                    }),
                ),
            ),
            (
                6,
                request(
                    6,
                    BACKTEST_START,
                    "",
                    bad(BacktestJobCommand {
                        capital_usd: vec![0],
                        ..point.clone()
                    }),
                ),
            ),
            (
                7,
                request(
                    7,
                    BACKTEST_START,
                    "",
                    bad(BacktestJobCommand {
                        funding: vec!["all; rm".into()],
                        ..point.clone()
                    }),
                ),
            ),
            (
                8,
                request(
                    8,
                    BACKTEST_START,
                    "",
                    bad(BacktestJobCommand {
                        max_seconds: 0,
                        ..point.clone()
                    }),
                ),
            ),
            (
                9,
                request(9, BACKTEST_START, "run-a", point.encode_to_vec()),
            ),
            (1, request(1, JOB_CANCEL, "../job", vec![])),
            (2, request(2, JOB_CANCEL, "job-1", vec![1])),
            (
                3,
                request(
                    3,
                    OPTIMIZE_START,
                    "",
                    OptimizeJobCommand {
                        baseline: None,
                        ..opt.clone()
                    }
                    .encode_to_vec(),
                ),
            ),
            (4, request(4, BACKTEST_START, "", vec![0xFF, 0xFF])),
            (
                5,
                request(
                    5,
                    VERIFY_START,
                    "",
                    PostCaptureJobCommand {
                        chain_verify: "index".into(),
                        ..verify.clone()
                    }
                    .encode_to_vec(),
                ),
            ),
            (
                6,
                request(
                    6,
                    VERIFY_START,
                    "",
                    PostCaptureJobCommand {
                        chain_verify: "off".into(),
                        offline: true,
                        ..verify.clone()
                    }
                    .encode_to_vec(),
                ),
            ),
            (
                7,
                request(
                    7,
                    VERIFY_START,
                    "",
                    PostCaptureJobCommand {
                        run_id: "/abs".into(),
                        ..verify.clone()
                    }
                    .encode_to_vec(),
                ),
            ),
            (
                8,
                request(
                    8,
                    BACKTEST_START,
                    "",
                    bad(BacktestJobCommand {
                        run_id: "--project".into(),
                        ..point.clone()
                    }),
                ),
            ),
            (9, request(9, JOB_CANCEL, "-job", vec![])),
            (
                1,
                request(
                    1,
                    SOURCE_SET,
                    "",
                    SourceControlCommand {
                        unit: "geyser:../x".into(),
                        mode: "observe".into(),
                    }
                    .encode_to_vec(),
                ),
            ),
            (
                2,
                request(
                    2,
                    SOURCE_SET,
                    "",
                    SourceControlCommand {
                        unit: "wss".into(),
                        mode: "on".into(),
                    }
                    .encode_to_vec(),
                ),
            ),
        ] {
            // Fresh ids: none of these may reuse a receipt of the dispatched commands above.
            let mut req = req;
            req.request_id = format!("{n}2345678-1234-1234-1234-000000000000");
            let r = control.execute(&req).await;
            assert_eq!(
                (r.status, r.reject_reason),
                (
                    CommandStatus::Rejected as i32,
                    CommandRejectReason::InvalidParams as i32
                ),
                "{}",
                req.action
            );
            assert!(
                !control
                    .lookup(&req.request_id, ProjectId::Project0 as i32)
                    .await
                    .found
            );
        }
        assert!(!log.exists(), "nothing dispatched");
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
        let control = Control::new(
            Some(Paths {
                ctl: dir.join("darkbotctl"),
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "com.darkbot.test.absent.capture".into(),
            }),
            &Limits::default(),
        );
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

//! Audit E 4.13: the GUI readiness job's one native owner (was
//! `bsc/tools/readiness_job.py`, the key=value files it and
//! `ops/bsc-control.sh` wrote, and two readers deriving the job's state from
//! them, this reporter's and the gateway's).
//!
//! One read-only native readiness run per launch: no key loading, signing,
//! dry run, builder call, forced kill or implicit retry. A TERM or INT is
//! forwarded to the run's own process group. The lifecycle marks are typed
//! records (`bsc_runtime::readiness_job`), one file per kind in the run
//! directory, replaced whole; the job's state is derived here only (the
//! gateway asks `bsc-status-reporter --readiness-status`).

use super::gui_readiness;
use super::wire::bsc::JobStatus;
use darkbot_bsc_record::schema::bsc_runtime::readiness_job as rj;
use darkbot_bsc_record::schema::g07::readiness;
use darkbot_bsc_record::{Draft, decode};
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The largest readiness record a run may publish: a record's bound.
const MAX_RESULT_BYTES: u64 = darkbot_bsc_record::MAX_RECORD_BYTES as u64;
/// A mark is a few dozen bytes; more is not a mark.
const MAX_MARK_BYTES: u64 = 4096;
/// The verdicts of a run that ended on its own or on request: a job result,
/// never a process failure.
const COMPLETED: [&str; 4] = ["PASS_READ_ONLY", "BLOCKED", "STOP", "STOPPED"];
/// The snapshot's public readiness configuration: exactly these keys.
const RPC_KEY: &str = "BSC_READINESS_RPC_HTTPS";
const CLOCK_KEY: &str = "DARKBOT_G07_CLOCK_ID";

/// One kind of lifecycle mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// The controller launched the job (its start time).
    Launch,
    /// The job's stop handler is armed: a TERM now ends it with a result.
    Armed,
    /// The controller asked the job to stop.
    Stopping,
    /// The run ended (its end time, result code and exit code).
    Result,
}

impl Mark {
    /// The marks the controller writes (`--readiness-mark`).
    pub fn of_controller(text: &str) -> Option<Self> {
        match text {
            "launch" => Some(Self::Launch),
            "stopping" => Some(Self::Stopping),
            _ => None,
        }
    }

    fn kind(self) -> &'static str {
        match self {
            Self::Launch => "LAUNCH",
            Self::Armed => "ARMED",
            Self::Stopping => "STOPPING",
            Self::Result => "RESULT",
        }
    }

    fn file(self) -> &'static str {
        match self {
            Self::Launch => "bsc.readiness.launch",
            Self::Armed => "bsc.readiness.armed",
            Self::Stopping => "bsc.readiness.stopping",
            Self::Result => "bsc.readiness.result",
        }
    }
}

/// One mark as read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marked {
    pub launch_id: String,
    pub at_unix_s: u64,
    /// `RESULT` only.
    pub result_code: String,
    /// `RESULT` only.
    pub exit_code: Option<i64>,
}

/// Unix seconds now.
pub fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Writes `mark` whole: a temporary file (create-new, 0600, synced) renamed
/// over the mark, then the directory synced.
pub fn write(
    rundir: &Path,
    mark: Mark,
    launch_id: &str,
    at_unix_s: u64,
    result: Option<(&str, i64)>,
) -> io::Result<()> {
    if !gui_readiness::launch_id_ok(launch_id) {
        return Err(io::Error::other("not a launch ID"));
    }
    let mut d = Draft::new(&rj::SCHEMA);
    d.set(rj::KIND, mark.kind())
        .set(rj::LAUNCH_ID, launch_id)
        .set(rj::AT_UNIX_S, at_unix_s);
    if let Some((code, exit)) = result {
        d.set(rj::RESULT_CODE, code).set(rj::EXIT_CODE, exit);
    }
    let bytes = d.encode().map_err(|e| io::Error::other(e.to_string()))?;
    let tmp = rundir.join(format!(
        "{}.{launch_id}.{}.tmp",
        mark.file(),
        std::process::id()
    ));
    let written = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, rundir.join(mark.file()))
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written?;
    fs::File::open(rundir)?.sync_all()
}

/// The mark of `mark`'s kind; none when absent or not such a mark.
pub fn read(rundir: &Path, mark: Mark) -> Option<Marked> {
    let mut bytes = Vec::new();
    fs::File::open(rundir.join(mark.file()))
        .ok()?
        .take(MAX_MARK_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    let r = decode(&rj::SCHEMA, &bytes).ok()?;
    if r.text(rj::KIND).ok()? != mark.kind() {
        return None;
    }
    Some(Marked {
        launch_id: r.text(rj::LAUNCH_ID).ok()?.to_owned(),
        at_unix_s: r.u64(rj::AT_UNIX_S).ok()?,
        result_code: r
            .opt_text(rj::RESULT_CODE)
            .ok()?
            .unwrap_or_default()
            .to_owned(),
        exit_code: r.opt_i64(rj::EXIT_CODE).ok()?,
    })
}

/// Whether the job can be controlled: a deployed readiness binary and a
/// snapshot.
pub fn controller_available(root: &Path, readiness_bin: Option<&Path>) -> bool {
    readiness_bin.is_some_and(Path::is_file) && root.join("SNAPSHOT_SHA256").is_file()
}

/// The job's state, its one derivation: launchd's view of `label` with the
/// marks of the current launch.
pub fn status(rundir: &Path, label: &str, controller_available: bool) -> JobStatus {
    let launch = read(rundir, Mark::Launch).unwrap_or_default();
    let mut status = JobStatus {
        job: "readiness".into(),
        state: "UNKNOWN".into(),
        launch_id: launch.launch_id.clone(),
        started_unix_s: launch.at_unix_s,
        controller_available: Some(controller_available),
        ..Default::default()
    };
    let current = |m: &Marked| !launch.launch_id.is_empty() && m.launch_id == launch.launch_id;
    let stopped = read(rundir, Mark::Stopping).is_some_and(|m| current(&m));
    let result = read(rundir, Mark::Result).filter(|m| current(m));
    let matched = result.is_some();
    let completed = result
        .as_ref()
        .is_some_and(|r| COMPLETED.contains(&r.result_code.as_str()));
    let Ok(out) = Command::new("/bin/launchctl")
        .args(["list", label])
        .output()
    else {
        return status;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let pid = text
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("\"PID\" = ")
                .and_then(|s| s.trim_end_matches(';').parse::<u32>().ok())
        })
        .filter(|p| *p > 0);
    if out.status.success() {
        status.pid = pid;
        status.state = if pid.is_some() {
            if stopped { "STOPPING" } else { "RUNNING" }
        } else {
            let code = text.lines().find_map(|l| {
                l.trim()
                    .strip_prefix("\"LastExitStatus\" = ")
                    .and_then(|s| s.trim_end_matches(';').parse::<i32>().ok())
            });
            if completed {
                "STOPPED"
            } else if code.is_some() || matched {
                "FAILED"
            } else {
                "UNKNOWN"
            }
        }
        .into();
    } else if String::from_utf8_lossy(&out.stderr).contains("Could not find service")
        || String::from_utf8_lossy(&out.stderr).contains("Could not find specified service")
    {
        status.state = if launch.launch_id.is_empty() || completed {
            "STOPPED"
        } else {
            "FAILED"
        }
        .into();
    }
    if let Some(r) = result {
        status.ended_unix_s = r.at_unix_s;
        status.result_code = r.result_code;
    }
    status
}

/// What one job run needs, from its plist (`ops/bsc-control.sh`).
pub struct Job<'a> {
    /// The snapshot.
    pub root: &'a Path,
    pub rundir: &'a Path,
    pub launch_id: &'a str,
    /// The deployed read-only readiness binary.
    pub readiness_bin: &'a Path,
}

/// A stop request and the run's process group, shared with the signal
/// thread.
#[derive(Default)]
struct Stop {
    requested: AtomicBool,
    group: AtomicI32,
}

impl Stop {
    fn requested(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }

    /// Forwards a TERM to the run's process group, if it runs (a gone
    /// group is no error).
    fn forward(&self) {
        let group = self.group.load(Ordering::SeqCst);
        if group > 0 {
            let _ = Command::new("/bin/kill")
                .env_clear()
                .args(["-TERM", "--", &format!("-{group}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

/// Runs one readiness launch; the process exit code: 0 for PASS_READ_ONLY or
/// a stop on request, 2 otherwise. The result mark is written in every case.
pub fn run(job: &Job<'_>) -> i32 {
    if !gui_readiness::launch_id_ok(job.launch_id) {
        eprintln!("READINESS_RUNNER_FAILED: launch ID");
        return 2;
    }
    let stop = Arc::new(Stop::default());
    let mut exit_code: i64 = 1;
    let mut result = "FAILED";
    let temporary = job
        .rundir
        .join(format!("bsc.readiness.{}.tmp", job.launch_id));
    match attempt(job, &stop, &temporary, &mut exit_code) {
        Ok(verdict) => {
            if let Some(verdict) = verdict {
                result = verdict;
            }
            if stop.requested() {
                result = "STOPPED";
            }
        }
        // The kind is diagnostic; never an environment or a payload.
        Err(kind) => eprintln!("READINESS_RUNNER_FAILED: {kind}"),
    }
    let _ = fs::remove_file(&temporary);
    if let Err(e) = write(
        job.rundir,
        Mark::Result,
        job.launch_id,
        now_s(),
        Some((result, exit_code)),
    ) {
        eprintln!("READINESS_RUNNER_FAILED: result mark: {}", e.kind());
        return 2;
    }
    println!("BSC_READINESS_RESULT={result} launch_id={}", job.launch_id);
    if stop.requested() || (exit_code == 0 && result == "PASS_READ_ONLY") {
        0
    } else {
        2
    }
}

/// The run itself: the verdict to publish (none: not valid), or the kind of
/// failure.
fn attempt(
    job: &Job<'_>,
    stop: &Arc<Stop>,
    temporary: &Path,
    exit_code: &mut i64,
) -> Result<Option<&'static str>, &'static str> {
    // The stop handler first, then the armed mark: a TERM before it would
    // end the job without a result (Paket 8 review L6); the controller
    // signals only once it reads the mark.
    use signal_hook::consts::{SIGINT, SIGTERM};
    let mut signals =
        signal_hook::iterator::Signals::new([SIGTERM, SIGINT]).map_err(|_| "signal handler")?;
    let on_signal = Arc::clone(stop);
    std::thread::Builder::new()
        .name("bsc-readiness-signals".into())
        .spawn(move || {
            for _ in signals.forever() {
                on_signal.requested.store(true, Ordering::SeqCst);
                on_signal.forward();
            }
        })
        .map_err(|_| "signal thread")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(job.rundir)
        .map_err(|_| "run directory")?;
    write(job.rundir, Mark::Armed, job.launch_id, now_s(), None).map_err(|_| "armed mark")?;

    // The snapshot, resolved once (the sync may swap its link meanwhile;
    // the provenance names the one the run used).
    let root = job.root.canonicalize().map_err(|_| "snapshot")?;
    let root = root.as_path();
    // Exact allowlist: never a signing key, RPC token or builder
    // authorization. BSC-050 A-scan: the endpoint and the clock from the
    // snapshot's public configuration, the boot ID from the OS, the main
    // checkout (attempt latch, shared authority) from the snapshot.
    let (rpc, clock) = public_config(&root.join("bsc/config/readiness.public.env"))?;
    let mut env: Vec<(&str, String)> = vec![
        ("PATH", "/usr/bin:/bin:/usr/sbin:/sbin".into()),
        ("BSC_MAINNET_RPC_HTTPS", rpc),
    ];
    if let Some(home) = home() {
        env.push(("HOME", home));
    }
    if let Some(clock) = clock.filter(|c| !c.is_empty()) {
        env.push((CLOCK_KEY, clock));
    }
    if let Ok(boot) = darkbot_bsc_execution::sources::boot_session() {
        env.push(("DARKBOT_G07_BOOT_ID", boot));
    }
    if let Ok(checkout) = fs::read_to_string(root.join("CHECKOUT")) {
        env.push(("DARKBOT_BSC_CHECKOUT", checkout.trim().to_owned()));
    }
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(temporary)
        .map_err(|_| "result file")?;
    let mut child = Command::new(job.readiness_bin)
        .args(["--current-rpc", "--binary"])
        .current_dir(root)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::from(output.try_clone().map_err(|_| "result file")?))
        .process_group(0)
        .spawn()
        .map_err(|_| "readiness spawn")?;
    let group = i32::try_from(child.id()).map_err(|_| "readiness pid")?;
    stop.group.store(group, Ordering::SeqCst);
    if stop.requested() {
        stop.forward();
    }
    let status = child.wait().map_err(|_| "readiness wait")?;
    // Reaped: a later stop signals no group (its ID may be reused).
    stop.group.store(0, Ordering::SeqCst);
    *exit_code = match status.code() {
        Some(code) => i64::from(code),
        None => -i64::from(status.signal().unwrap_or(0)),
    };
    output.sync_all().map_err(|_| "result file")?;
    drop(output);
    if stop.requested() {
        return Ok(None);
    }
    let size = fs::metadata(temporary).map_err(|_| "result file")?.len();
    if !(1..=MAX_RESULT_BYTES).contains(&size) {
        return Ok(None);
    }
    let bytes = fs::read(temporary).map_err(|_| "result file")?;
    // The typed native verdict is the result; it must agree with the exit
    // code (0 only for PASS_READ_ONLY, 2 for BLOCKED/STOP), else the runner
    // failed.
    let Some(verdict) = verdict_of(&bytes) else {
        return Ok(None);
    };
    if *exit_code != if verdict == "PASS_READ_ONLY" { 0 } else { 2 } {
        return Ok(None);
    }
    publish(job, root, temporary)?;
    Ok(Some(verdict))
}

/// The account's home: `HOME`, else the password database's.
fn home() -> Option<String> {
    std::env::home_dir().and_then(|h| h.into_os_string().into_string().ok())
}

/// The snapshot's public readiness configuration: exactly the two known
/// keys, each once; the endpoint https without whitespace.
fn public_config(path: &Path) -> Result<(String, Option<String>), &'static str> {
    let text = fs::read_to_string(path).map_err(|_| "readiness configuration")?;
    let (mut rpc, mut clock) = (None, None);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err("readiness configuration line");
        };
        let slot = match key {
            RPC_KEY => &mut rpc,
            CLOCK_KEY => &mut clock,
            _ => return Err("readiness configuration line"),
        };
        if slot.replace(value.trim().to_owned()).is_some() {
            return Err("readiness configuration line");
        }
    }
    match rpc {
        Some(rpc) if rpc.starts_with("https://") && !rpc.chars().any(char::is_whitespace) => {
            Ok((rpc, clock))
        }
        _ => Err("readiness RPC endpoint not configured"),
    }
}

/// The verdict of a readiness record: PASS_READ_ONLY, BLOCKED or STOP in a
/// known mode; none for anything else.
fn verdict_of(bytes: &[u8]) -> Option<&'static str> {
    let r = decode(&readiness::SCHEMA, bytes).ok()?;
    let mode = r.text(readiness::MODE).ok()?;
    if !["READ_ONLY_RPC", "OFFLINE_ZERO_EGRESS"].contains(&mode) {
        return None;
    }
    let status = r.text(readiness::STATUS).ok()?;
    ["PASS_READ_ONLY", "BLOCKED", "STOP"]
        .into_iter()
        .find(|v| *v == status)
}

/// Publishes the validated result: a hidden directory with the record and
/// its sealed provenance (BSC-048), synced, renamed into place.
fn publish(job: &Job<'_>, root: &Path, temporary: &Path) -> Result<(), &'static str> {
    let relative = gui_readiness::relative_path(job.launch_id);
    let operational = job.rundir.join("bsc-operational");
    let final_dir = operational.join(&relative);
    let parent = final_dir.parent().ok_or("result directory")?;
    let name = final_dir.file_name().ok_or("result directory")?;
    let staging = parent.join(format!(".{}", name.to_string_lossy()));
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|_| "result directory")?;
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&staging)
        .map_err(|_| "result directory")?;
    fs::rename(temporary, staging.join(gui_readiness::RESULT)).map_err(|_| "result file")?;
    gui_readiness::seal(root, &staging, job.launch_id, &relative)
        .map_err(|_| "provenance not sealed")?;
    fs::File::open(&staging)
        .and_then(|d| d.sync_all())
        .map_err(|_| "result directory")?;
    fs::rename(&staging, &final_dir).map_err(|_| "result directory")?;
    fs::File::open(parent)
        .and_then(|d| d.sync_all())
        .map_err(|_| "result directory")
}

//! `p0ctl` — thin terminal control of the one Project_0 runtime (FEUI-01, CONTROL_PROTOCOL §2).
//!
//! It starts, observes and stops the existing launcher/runtime and runs the existing verifier and
//! report entry points; it contains no trading, chain, classification or economics logic and
//! never starts a second runtime. Observe-only: LIVE and submission stay disabled.
//!
//! ```text
//! p0ctl [--project DIR] doctor [--scope offline]
//! p0ctl [--project DIR] preflight --duration D [--capture-source logs|transactions]
//!       [--chain-verify off|index|strict|crosscheck] [--verify-rps N] [--verify-calls N] [--verify-seconds N]
//! p0ctl [--project DIR] run start --duration D [test.sh options…]
//! p0ctl [--project DIR] run status [--run RUN_ID]
//! p0ctl [--project DIR] run stop
//! p0ctl [--project DIR] verify  --run RUN_ID --mode off|index|strict [--resume-verification] …
//! p0ctl [--project DIR] report  --run RUN_ID [--report-* …]
//! ```
use darkbot_runner::analytics::journal;
use darkbot_runner::orchestration::boot_audit::{self, AuditVerdict, OfflineInputs, SafetyFlags};
use darkbot_runner::orchestration::control::{self, OwnerState};
use darkbot_runner::storage::preflight;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

const RUN_ROOTS: [&str; 3] = ["data/runs", "data/econ4h", "data/market"];

fn project_dir(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    let dir = explicit
        .or_else(|| std::env::var_os("P0_PROJECT_DIR").map(PathBuf::from))
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    dir.canonicalize()
        .map_err(|e| format!("PROJECT_DIR {}: {e}", dir.display()))
}

fn lock_path(project: &Path) -> PathBuf {
    std::env::var_os("P0_RUNTIME_LOCK")
        .map(PathBuf::from)
        .unwrap_or_else(|| control::default_lock_path(project))
}

fn value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn doctor(project: &Path, args: &[String]) -> Result<bool, String> {
    match value(args, "--scope").as_deref() {
        None | Some("offline") => {}
        Some("network") => {
            return Err("DOCTOR_NETWORK_NOT_AVAILABLE: network checks (BOOT-03…08) need explicit Mainnet read permission and are not run by this build".into())
        }
        Some(s) => return Err(format!("SCOPE_INVALID: {s}")),
    }
    let audit = boot_audit::offline(&OfflineInputs::for_project(project))?;
    print!("{}", audit.terminal());
    Ok(audit.verdict != AuditVerdict::Blocked)
}

fn preflight_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    let duration = value(args, "--duration").ok_or("--duration required")?;
    let seconds =
        darkbot_runner::analytics::measurement::MeasurementPlan::parse_duration(&duration)?;
    let source = value(args, "--capture-source").unwrap_or_else(|| "logs".into());
    if !matches!(source.as_str(), "logs" | "transactions") {
        return Err(format!("CAPTURE_SOURCE_INVALID: {source}"));
    }
    let root = project.join("data/runs");
    let out = root.join(format!(".p0ctl-preflight-{}", std::process::id()));
    let mut req = preflight::Request::from_active(seconds, &out, &source);
    req.verify.mode = preflight::VerifyMode::parse(
        &value(args, "--chain-verify").unwrap_or_else(|| "off".into()),
    )?;
    for (flag, slot) in [
        ("--verify-rps", &mut req.verify.rps),
        ("--verify-calls", &mut req.verify.calls),
        ("--verify-seconds", &mut req.verify.seconds),
    ] {
        if let Some(v) = value(args, flag) {
            *slot = v
                .parse()
                .ok()
                .filter(|n| *n > 0)
                .ok_or(format!("{flag} must be a positive integer"))?;
        }
    }
    let basis = preflight::discover_runs(&root, None);
    // Advisory: the binding preflight runs inside the launcher for the actual run directory.
    let result = preflight::run(req, &basis, 64 * 1024 * 1024);
    let _ = std::fs::remove_dir_all(&out);
    let report = result?;
    print!("{}", report.terminal());
    let audit = boot_audit::offline(&OfflineInputs::for_project(project))?;
    println!(
        "boot audit: {} (details: p0ctl doctor)",
        audit.verdict.as_str()
    );
    Ok(report.start_allowed && audit.verdict != AuditVerdict::Blocked)
}

fn run_start(project: &Path, args: &[String]) -> Result<bool, String> {
    if value(args, "--duration").is_none() {
        return Err("--duration required".into());
    }
    let lock = lock_path(project);
    if let OwnerState::Running { pid, run_id, .. } = control::owner_state(&lock)? {
        return Err(format!(
            "RUNTIME_OWNER_LOCKED pid={pid} run_id={run_id}: stop it first (p0ctl run stop)"
        ));
    }
    let flags = SafetyFlags::from_env();
    let audit = boot_audit::offline(&OfflineInputs::for_project(project))?;
    print!("{}", audit.terminal());
    if audit.verdict == AuditVerdict::Blocked {
        return Err("BOOT_AUDIT_BLOCKED".into());
    }
    let run_id = std::env::var("P0_ECON_RUN_ID").unwrap_or_else(|_| {
        format!(
            "econ4h_{}",
            darkbot_runner::market_observer::utc_compact(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            )
        )
    });
    // BOOT-12: the audited identity must still hold right before the start.
    boot_audit::recheck(&audit, project, flags)?;
    control::install_stop_handler()?;
    let mut child = Command::new(project.join("scripts/test.sh"))
        .args(args)
        .env("P0_RUNTIME_LOCK", &lock)
        .env("P0_ECON_RUN_ID", &run_id)
        .process_group(0)
        .spawn()
        .map_err(|e| format!("LAUNCH: {e}"))?;
    println!(
        "[p0ctl] run {run_id} started (launcher pid {}); stop with Ctrl-C or `p0ctl run stop`",
        child.id()
    );
    let mut forwarded = false;
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            println!(
                "[p0ctl] run {run_id} ended: {status}; details: p0ctl run status --run {run_id}"
            );
            return Ok(status.success());
        }
        if control::stop_requested() && !forwarded {
            forwarded = true;
            match control::owner_state(&lock)? {
                OwnerState::Running { pid, .. } => {
                    // SAFETY: plain kill(2) to the runtime owner that holds the lock.
                    unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
                    println!("[p0ctl] stop forwarded to runtime pid {pid}: capture drains, journals are sealed, coverage is recorded INCOMPLETE (OPERATOR_STOP)");
                }
                OwnerState::NotRunning { .. } => {
                    // Before the runtime exists (build/plan/preflight) nothing needs draining.
                    unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGTERM) };
                    println!("[p0ctl] stop before runtime start: launcher terminated");
                }
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn find_run(project: &Path, run_id: &str) -> Option<PathBuf> {
    let direct = Path::new(run_id);
    if direct.is_dir() {
        return Some(direct.to_path_buf());
    }
    let found: Vec<_> = RUN_ROOTS
        .iter()
        .map(|r| project.join(r).join(run_id))
        .filter(|p| p.is_dir())
        .collect();
    (found.len() == 1).then(|| found[0].clone())
}

/// Last occurrence of each status tag in the bounded tail of the runtime log.
fn log_tail(path: &Path) -> Vec<String> {
    const TAIL: u64 = 256 * 1024;
    let Ok(mut f) = std::fs::File::open(path) else {
        return vec![];
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let _ = f.seek(SeekFrom::Start(len.saturating_sub(TAIL)));
    let mut text = String::new();
    let _ = f.take(TAIL).read_to_string(&mut text);
    [
        "[MARKET/STORAGE]",
        "[MARKET/STATUS]",
        "[SHADOW/STATUS]",
        "[MARKET/CANONICAL]",
        "[C0_E1/STOP]",
    ]
    .iter()
    .filter_map(|tag| {
        text.lines()
            .rev()
            .find(|l| l.contains(tag))
            .map(str::to_string)
    })
    .collect()
}

fn journal_line(path: &Path) -> String {
    let Ok(mut r) = darkbot_runner::storage::segments::ChainReader::open(path) else {
        return format!("{}: unreadable", path.display());
    };
    let (mut frames, mut last_rx, mut by_kind) = (0u64, None, std::collections::BTreeMap::new());
    let mut tail = "complete";
    loop {
        match r.next_frame() {
            Ok(Some(f)) => {
                if f.kind == journal::MANIFEST {
                    continue;
                }
                frames += 1;
                *by_kind.entry(f.kind).or_insert(0u64) += 1;
                last_rx = f.provenance.rx_ns.or(last_rx);
            }
            Ok(None) => break,
            Err(_) => {
                tail = "tail in flight or truncated";
                break;
            }
        }
    }
    format!(
        "{} segments={} frames={frames} kinds={by_kind:?} sealed={} complete={} last_rx_ns={} ({tail})",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
        r.segments(),
        r.sealed,
        r.complete,
        last_rx.map_or("-".into(), |v: u64| v.to_string())
    )
}

fn run_status(project: &Path, args: &[String]) -> Result<bool, String> {
    let lock = lock_path(project);
    let state = control::owner_state(&lock)?;
    let mut run_id = value(args, "--run");
    match &state {
        OwnerState::Running {
            pid,
            run_id: r,
            mode,
            started_unix_s,
        } => {
            println!(
                "runtime: RUNNING pid={pid} run_id={r} mode={mode} started_unix_s={started_unix_s}"
            );
            run_id = run_id.or(Some(r.clone()));
        }
        OwnerState::NotRunning { last } => {
            println!(
                "runtime: NOT_RUNNING (last owner: {})",
                last.as_deref().unwrap_or("none")
            );
            if run_id.is_none() {
                run_id = last
                    .as_deref()
                    .and_then(|l| {
                        l.split_whitespace()
                            .find_map(|kv| kv.strip_prefix("run_id="))
                    })
                    .map(str::to_string);
            }
        }
    }
    let Some(run_id) = run_id else {
        return Ok(true);
    };
    let Some(run) = find_run(project, &run_id) else {
        println!("run {run_id}: directory not found (or ambiguous) under {RUN_ROOTS:?}");
        return Ok(true);
    };
    println!("run {run_id}: {}", run.display());
    for capacity in [
        run.join("capture").join(preflight::REPORT_JOURNAL),
        run.join(preflight::REPORT_JOURNAL),
    ] {
        if let Ok(r) = journal::read_typed_value::<preflight::CapacityReport>(&capacity) {
            println!(
                "  capacity: {} start_allowed={} duration={}s chain_verify={}",
                r.verdict.as_str(),
                r.start_allowed,
                r.request.duration_s,
                r.request.verify.mode.as_str()
            );
        }
    }
    for market in [
        run.join("capture/market/market.p0j"),
        run.join("market/market.p0j"),
        run.join("market.p0j"),
    ] {
        if market.exists() {
            println!("  market journal: {}", journal_line(&market));
        }
    }
    let results = run.join("reports/post_capture_result.json");
    println!(
        "  post-capture result: {}",
        if results.exists() {
            "present (reports/)"
        } else {
            "not yet written"
        }
    );
    for logs in [run.join("logs/runtime.log"), run.join("runtime.log")] {
        for line in log_tail(&logs) {
            println!("  {line}");
        }
    }
    Ok(true)
}

fn run_stop(project: &Path) -> Result<bool, String> {
    match control::owner_state(&lock_path(project))? {
        OwnerState::NotRunning { .. } => {
            println!("NOT_RUNNING: nothing to stop");
            Ok(true)
        }
        OwnerState::Running { pid, run_id, .. } => {
            // SAFETY: plain kill(2) to the pid recorded by the live lock holder.
            if unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) } != 0 {
                let e = std::io::Error::last_os_error();
                if e.raw_os_error() == Some(libc::ESRCH) {
                    println!("NOT_RUNNING: owner {pid} already exited");
                    return Ok(true);
                }
                return Err(format!("STOP_SIGNAL: {e}"));
            }
            println!("STOP_REQUESTED run_id={run_id} pid={pid}: admission closed, capture drains, journals are sealed, coverage is recorded INCOMPLETE (OPERATOR_STOP) (repeat is harmless)");
            Ok(true)
        }
    }
}

fn delegate(project: &Path, script: &str, args: &[String]) -> Result<bool, String> {
    let e = Command::new(project.join("scripts").join(script))
        .args(args)
        .exec();
    Err(format!("EXEC {script}: {e}"))
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let explicit = match args.iter().position(|a| a == "--project") {
        Some(i) if i + 1 < args.len() => {
            let v = args.remove(i + 1);
            args.remove(i);
            Some(PathBuf::from(v))
        }
        _ => None,
    };
    let result = project_dir(explicit).and_then(|project| {
        let rest: Vec<String> = args.iter().skip(1).cloned().collect();
        match (args.first().map(String::as_str), rest.first().map(String::as_str)) {
            (Some("doctor"), _) => doctor(&project, &rest),
            (Some("preflight"), _) => preflight_cmd(&project, &rest),
            (Some("run"), Some("start")) => run_start(&project, &rest[1..]),
            (Some("run"), Some("status")) => run_status(&project, &rest[1..]),
            (Some("run"), Some("stop")) => run_stop(&project),
            (Some("verify"), _) => delegate(&project, "verify.sh", &rest),
            (Some("report"), _) => delegate(&project, "report.sh", &rest),
            _ => Err("usage: p0ctl [--project DIR] doctor|preflight|run start|run status|run stop|verify|report …".into()),
        }
    });
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(2),
        Err(e) => {
            eprintln!("p0ctl: {e}");
            ExitCode::from(2)
        }
    }
}

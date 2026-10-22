//! `p0ctl` — thin terminal control of the one Project_0 runtime (FEUI-01, CONTROL_PROTOCOL §2).
//!
//! It starts, observes and stops the existing launcher/runtime and runs the existing verifier and
//! report entry points; it contains no trading, chain, classification or economics logic and
//! never starts a second runtime. Observe-only: LIVE and submission stay disabled.
//!
//! ```text
//! p0ctl [--project DIR] doctor [--scope offline|network] [--network-approved TOKEN --genesis-hash HASH --provider-evidence FILE --output FILE --capture-source logs|transactions] [-- OPERATIONAL_ARGS]
//! p0ctl [--project DIR] preflight --duration D [--capture-source logs|transactions]
//!       [--chain-verify off|index|strict|crosscheck] [--verify-rps N] [--verify-calls N] [--verify-seconds N]
//! p0ctl [--project DIR] run start --duration D [test.sh options…]
//! p0ctl [--project DIR] run status [--run RUN_ID]
//! p0ctl [--project DIR] run stop
//! p0ctl [--project DIR] verify  --run RUN_ID --mode off|index|strict|crosscheck [--resume-verification] [--offline] [--provider-evidence FILE] [--verify-* N] …
//! p0ctl [--project DIR] report  --run RUN_ID [--report-* …] [--verification-result ID] [--format table|csv]
//! p0ctl [--project DIR] export  --run RUN_ID   (explicit offline legacy views for the pinned readers)
//! p0ctl [--project DIR] qualify --run RUN_ID [--verification-result ID]
//! p0ctl [--project DIR] post-capture --run RUN_ID --chain-verify off|strict [--provider-evidence FILE] [--verify-* N] [--report-* …]
//! ```
//!
//! `report` runs the native report (B7) for runs with native capture records (typed run
//! manifest and market completion); index provenance, JSON output and historical runs use the
//! pinned Python report. `qualify` is the separate C0-E1 master qualification after the
//! verification (PHASENPLAN L.2): it binds the report's axes, the run's final technical record
//! and the owners' end, persists an immutable result and exits 0 only for `C0_E1_MASTER_PASS`.
use darkbot_runner::analytics::journal;
use darkbot_runner::analytics::{qualification, renders, report};
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
            #[cfg(feature = "crate_health")]
            {
                return darkbot_runner::orchestration::boot_network::execute(project, args);
            }
            #[cfg(not(feature = "crate_health"))]
            {
                return Err("BOOT_NETWORK_REQUIRES_CRATE_HEALTH".into());
            }
        }
        Some(s) => return Err(format!("SCOPE_INVALID: {s}")),
    }
    darkbot_solana::runtime_config::Resolved::parse(
        args.split(|a| a == "--").nth(1).unwrap_or(&[]),
        &|k| std::env::var(k).ok(),
    )?
    .native_only()?
    .activate()?;
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

const RUN_START_USAGE: &str =
    "p0ctl run start --duration 10m|1h|5h|24h [--network-audit FILE --mainnet-approved TOKEN] [--capture-source logs|transactions]
    [--chain-verify off|strict] [--verify-rps N] [--verify-calls N] [--verify-seconds N]
    [--verify-bytes N] [--verify-disk-bytes N] [--verify-ram-bytes N] [--verify-min-free-bytes N]
    [--verify-max-pages N] [--provider-evidence FILE.p0j] [--report-* VALUE ...] [--format table|csv]
    [-- OPERATIONAL_ARGS...]
Typed C0-E1 measurement: offline boot audit, owner lock, capture, then the post-capture chain
(verification off|strict — online strict only with provider evidence —, native report, master
qualification). LIVE and submission stay disabled.
OFFLINE_READY_NETWORK_NOT_RUN is no Mainnet release. A sealed, fresh READY network audit
and the operator's explicit --mainnet-approved TOKEN for it are required.";

/// Set by `run start` for its launcher child only: `run start` and `capture.sh econ4h` refuse to
/// run inside a launcher, so `run start` -> `capture.sh econ4h-launcher` can never recurse.
const LAUNCH_MARKER: &str = "P0_CTL_LAUNCH_RUN_ID";

fn run_start(project: &Path, args: &[String]) -> Result<bool, String> {
    if let Some(run) = std::env::var_os(LAUNCH_MARKER) {
        return Err(format!(
            "RECURSIVE_LAUNCH: run start inside the launcher of run {}",
            run.to_string_lossy()
        ));
    }
    let options = args.split(|a| a == "--").next().unwrap_or(&[]);
    if options.iter().any(|a| a == "--help" || a == "-h") {
        println!("{RUN_START_USAGE}");
        return Ok(true);
    }
    if options
        .iter()
        .filter(|a| *a == "--mainnet-approved")
        .count()
        > 1
    {
        return Err("exactly one --mainnet-approved".into());
    }
    match options.iter().filter(|a| *a == "--duration").count() {
        1 => {}
        0 => return Err(format!("--duration required\n{RUN_START_USAGE}")),
        _ => return Err("exactly one --duration".into()),
    }
    let duration = value(options, "--duration").ok_or("--duration requires a value")?;
    darkbot_solana::runtime_config::duration_seconds(&duration)
        .map_err(|e| format!("--duration {duration}: {e}"))?;
    if let Some(mode @ ("index" | "crosscheck")) = value(options, "--chain-verify").as_deref() {
        return Err(format!("CHAIN_VERIFY_{mode}_BLOCKED: the index/crosscheck verifier exists only as the pinned Python reader, which writes JSON evidence (JFI-03); use --chain-verify strict or off"));
    }
    let lock = lock_path(project);
    if let OwnerState::Running { pid, run_id, .. } = control::owner_state(&lock)? {
        return Err(format!(
            "RUNTIME_OWNER_LOCKED pid={pid} run_id={run_id}: stop it first (p0ctl run stop)"
        ));
    }
    // X1: the same rule the launched runtime re-resolves its config with before admission.
    boot_audit::start_config(
        args.split(|a| a == "--").nth(1).unwrap_or(&[]),
        &duration,
        &|k| std::env::var(k).ok(),
    )?
    .activate()?;
    let flags = SafetyFlags::from_env();
    let audit = boot_audit::offline(&OfflineInputs::for_project(project))?;
    print!("{}", audit.terminal());
    if audit.verdict == AuditVerdict::Blocked {
        return Err("BOOT_AUDIT_BLOCKED".into());
    }
    // An offline-only verdict is no Mainnet release (network checks BOOT-03…08 NOT_RUN). The
    // capture reads Mainnet, so it starts only with the operator's explicit approval of exactly
    // this audited state.
    let network_path = value(options, "--network-audit")
        .ok_or("BOOT_NETWORK_AUDIT_REQUIRED: offline approval cannot qualify network readiness")?;
    let network: boot_audit::NetworkAudit = journal::read_typed_value(Path::new(&network_path))
        .map_err(|_| "BOOT_NETWORK_EVIDENCE_INVALID")?;
    let binary = boot_audit::binary_identity()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "BOOT_CLOCK")?
        .as_secs();
    network.validate_start(
        &audit.identity,
        &binary,
        now,
        &value(options, "--capture-source").unwrap_or_else(|| "logs".into()),
    )?;
    let token = &network.audit.readiness_token[..16];
    if value(options, "--mainnet-approved").as_deref() != Some(token) {
        return Err(
            "MAINNET_APPROVAL_REQUIRED: explicit approval of the network readiness token required"
                .into(),
        );
    }
    // The approval is p0ctl's own option; the launcher gets the remaining arguments.
    let mut launcher_args = Vec::new();
    let mut rest = args.iter();
    let mut in_options = true;
    while let Some(arg) = rest.next() {
        if in_options && matches!(arg.as_str(), "--mainnet-approved" | "--network-audit") {
            rest.next();
            continue;
        }
        in_options &= arg != "--";
        launcher_args.push(arg.clone());
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
    let approved_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "BOOT_CLOCK")?
        .as_secs();
    network.validate_start(
        &boot_audit::identity(project, flags)?,
        &binary,
        approved_at,
        &value(options, "--capture-source").unwrap_or_else(|| "logs".into()),
    )?;
    // X1: the runtime binds itself to exactly this approved record before admission.
    let network_record =
        std::fs::canonicalize(&network_path).map_err(|_| "BOOT_NETWORK_EVIDENCE_INVALID")?;
    control::install_stop_handler()?;
    let mut child = Command::new(project.join("scripts/capture.sh"))
        .arg("econ4h-launcher")
        .args(&launcher_args)
        .env("P0_RUNTIME_LOCK", &lock)
        .env("P0_ECON_RUN_ID", &run_id)
        .env(LAUNCH_MARKER, &run_id)
        .env(boot_audit::ENV_NETWORK_AUDIT, &network_record)
        .env(boot_audit::ENV_NETWORK_AUDIT_TOKEN, token)
        .env(
            boot_audit::ENV_NETWORK_AUDIT_APPROVED_AT,
            approved_at.to_string(),
        )
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

fn report_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    let native = value(args, "--run")
        .and_then(|r| find_run(project, &r))
        .filter(|run| {
            run.join("capture/run_manifest.p0j").is_file()
                && run.join("capture/market/market_completion.p0j").is_file()
        });
    let pinned_only = value(args, "--report-provenance").as_deref() == Some("index")
        || value(args, "--format").as_deref() == Some("json");
    let Some(run) = native.filter(|_| !pinned_only) else {
        return delegate(project, "report.sh", args);
    };
    let inv = report::parse_args(args)?;
    let out = report::report(&run, &inv.options, &inv.query)?;
    match inv.query.format {
        report::Format::Table => print!("{}\n{}", out.meta, out.data),
        report::Format::Csv => {
            eprint!("{}", out.meta);
            print!("{}", out.data);
        }
    }
    Ok(true)
}

/// C0E1-P5-CLI: one verify contract. A run with native records verifies `strict` and `off`
/// through the typed native driver (the request `p0-strict-verify --run` takes: online strict
/// only with `--provider-evidence`); `index`, `crosscheck` and historical runs use the pinned
/// Python verifier.
fn verify_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    use darkbot_runner::market_observer::strict_driver;
    let mode = value(args, "--mode").unwrap_or_else(|| "index".into());
    let native = value(args, "--run")
        .and_then(|r| find_run(project, &r))
        .filter(|run| run.join("capture/run_manifest.p0j").is_file());
    let Some(run) = native.filter(|_| matches!(mode.as_str(), "strict" | "off")) else {
        return delegate(project, "verify.sh", args);
    };
    let mut request = vec!["--run".to_string(), run.display().to_string()];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--run" {
            it.next();
        } else {
            request.push(a.clone());
        }
    }
    let (text, verified) = strict_driver::cli(&request)?;
    print!("{text}");
    // `off` is a complete outcome (NOT_RUN); strict passes only CANONICAL_VERIFIED.
    Ok(verified || mode == "off")
}

/// C0E1-P5-AUTO: the production chain after a capture (verification, report, qualification);
/// `Ok(false)` (exit 2) when the verification or the report did not complete.
fn post_capture_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    use darkbot_runner::analytics::post_capture;
    let wanted = value(args, "--run").ok_or("post-capture --run ID|PATH required")?;
    let run = find_run(project, &wanted).ok_or("RUN_NOT_FOUND_OR_AMBIGUOUS")?;
    let mode = value(args, "--chain-verify").unwrap_or_else(|| "off".into());
    let mut rest = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--run" || a == "--chain-verify" {
            it.next();
        } else {
            rest.push(a.clone());
        }
    }
    let out = post_capture::post_capture(&run, &mode, &rest)?;
    print!("{}", out.text);
    Ok(out.verification_ok && out.report_ok)
}

/// C0E1-P4-VC: the master qualification of a run with native records against its selected
/// verification result; prints the axes and the verdict, `Ok(false)` (exit 2) unless MASTER_PASS.
fn qualify_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    let wanted = value(args, "--run").ok_or("qualify --run ID|PATH required")?;
    let run = find_run(project, &wanted).ok_or("RUN_NOT_FOUND_OR_AMBIGUOUS")?;
    let opts = report::Options {
        verification_result: value(args, "--verification-result"),
        ..Default::default()
    };
    let (record, path) = qualification::qualify(&run, &opts)?;
    print!("{}", qualification::render(&record));
    println!("record={}", path.display());
    Ok(record.verdict == qualification::MASTER_PASS)
}

/// Explicit offline export (JFI-03): the legacy views the pinned Python readers need, rendered
/// from the journals of a run with native records (`manifest.json`, `config.json`, phase and
/// market JSONL, `summary.json`, hourly/final economics). Existing views must match; nothing is
/// overwritten. Runs without native records are left unchanged.
fn export_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    let wanted = value(args, "--run").ok_or("export --run ID|PATH required")?;
    let run = find_run(project, &wanted).ok_or("RUN_NOT_FOUND_OR_AMBIGUOUS")?;
    if !(run.join("capture/run_manifest.p0j").is_file() || run.join("run_manifest.p0j").is_file()) {
        println!(
            "EXPORT_NOT_NEEDED: {} has no native run manifest",
            run.display()
        );
        return Ok(true);
    }
    let paths = renders::detect(&run)?;
    for (path, status) in renders::ensure(&paths, true)? {
        println!("{status} {}", path.display());
    }
    Ok(true)
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
            (Some("verify"), _) => verify_cmd(&project, &rest),
            (Some("report"), _) => report_cmd(&project, &rest),
            (Some("export"), _) => export_cmd(&project, &rest),
            (Some("qualify"), _) => qualify_cmd(&project, &rest),
            (Some("post-capture"), _) => post_capture_cmd(&project, &rest),
            _ => Err("usage: p0ctl [--project DIR] doctor|preflight|run start|run status|run stop|verify|report|export|qualify|post-capture …".into()),
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

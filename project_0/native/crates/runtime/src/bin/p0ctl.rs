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
//! p0ctl [--project DIR] run preflight …   (= preflight)
//! p0ctl [--project DIR] run start --duration D [--profile NAME] [test.sh options…]
//! p0ctl [--project DIR] run start --until-stopped --checkpoint D --minimum-runway D   (gated)
//! p0ctl [--project DIR] run status [--run RUN_ID]
//! p0ctl [--project DIR] run stop [--run RUN_ID]
//! p0ctl [--project DIR] runs list
//! p0ctl [--project DIR] backtest --run RUN_ID (--profile FILE | --capital-usd LIST [--min-profit-usd X] [--funding all|A,B]) --max-seconds N --max-ram-bytes N [--verification-result ID]
//! p0ctl [--project DIR] optimize --run RUN_ID --capital-usd LIST [--min-profit-usd X] [--funding …] --min-profit-usd-grid X,Y --funding-sets all;A,B --holdout-bps N --max-points N --max-seconds N --max-ram-bytes N [--save-profile]
//! p0ctl [--project DIR] profiles compare --run RUN_ID --baseline FILE --candidate FILE --max-seconds N --max-ram-bytes N
//! p0ctl [--project DIR] job start backtest|optimize [--id ID] (the backtest|optimize options)
//! p0ctl [--project DIR] job start post-capture [--id ID] --run RUN_ID --chain-verify off|strict [--offline] [--resume-verification] [--provider-evidence FILE] --max-seconds N --max-ram-bytes N
//! p0ctl [--project DIR] job cancel --id ID
//! p0ctl [--project DIR] job status [--id ID]
//! p0ctl [--project DIR] verify  --run RUN_ID --mode off|strict [--resume-verification] [--offline] [--provider-evidence FILE] [--verify-* N] …
//! p0ctl [--project DIR] verify  --run RUN_ID --historical --mode index|crosscheck …   (pinned reader, runs without native records only)
//! p0ctl [--project DIR] report  --run RUN_ID [--report-* …] [--verification-result ID] [--format table|csv]
//! p0ctl [--project DIR] report  --run RUN_ID --historical …   (pinned reader, runs without native records only)
//! p0ctl [--project DIR] qualify --run RUN_ID [--verification-result ID]
//! p0ctl [--project DIR] post-capture --run RUN_ID --chain-verify off|strict [--provider-evidence FILE] [--verify-* N] [--report-* …]
//! p0ctl [--project DIR] source set wss|geyser:PROVIDER off|observe|active
//! p0ctl [--project DIR] source status
//! p0ctl [--project DIR] approval show FILE [--journal-dir DIR]
//! p0ctl [--project DIR] fe04 accounting --journal-dir DIR --wallet PUBKEY
//! p0ctl [--project DIR] fe04 archive --journal-dir DIR --wallet PUBKEY
//! p0ctl [--project DIR] fe04 migrate --journal-dir DIR --wallet PUBKEY
//! ```
//!
//! `fe04 accounting` (FE-04/FE-07) prints the finalized accounting of every own attempt of the
//! wallet from its FE-04 journals (archived and active, read-only): each attempt and exit, the
//! cumulative realized net, fees, open inventory mark and the verdict
//! (`FIRST_REALIZED_PROFIT_EVIDENCED` only with everything settled, no position left and a
//! positive sum). It exits 0 only for a verdict other than UNKNOWN. `fe04 archive` seals a
//! finished attempt (settled, no exit open, no position) and moves it to the archive so that a
//! new, separately approved attempt can start; it takes the wallet lock, so it refuses while a
//! runtime holds the journal. `fe04 migrate` imports a pre-binary `<wallet>.json` journal once
//! into the binary journal and keeps the JSON as hash-bound evidence (`<wallet>.legacy.json`);
//! the runtime refuses an unmigrated one (F8). All are offline: no network, no signer, no LIVE.
//!
//! `approval show` (FE-04) reads one versioned one-shot approval profile — an attempt
//! (`Approval`) or an exit (`ExitApproval`), strict JSON authored by the operator — checks it
//! alone (schema, authorization, ids, keys, bounds, expiry) and prints what it binds: wallet,
//! program/deployment or mint/route, every maximum and the file's SHA-256, and whether its
//! `code_sha` is this project's clean HEAD and its `binary_sha256` these binaries. It exits 0
//! only when the profile is valid and both bindings MATCH. With `--journal-dir` an attempt
//! approval is shown against the wallet's FE-04 history: whether the approval is already spent
//! (archived), an unfinished active attempt, the cumulative realized net and the verdict — and it
//! exits 0 only if a new attempt could start. It changes nothing, reads no network and does not
//! enable LIVE (FE-06 stays a separate explicit approval).
//!
//! `source set` (S1-02) records a typed source-control request and rings the running runtime;
//! the state owner applies or refuses every request in order and records its decision
//! (`outcome_<seq>.p0j`, journal `source_control: applied|refused seq=N`); `source status` shows
//! that decision as the applied state. A Geyser unit can be OFF or OBSERVE; ACTIVE is refused
//! without a qualification record.
//!
//! `backtest`, `optimize` and `profiles compare` (STARTUP §4 C–F) read a finalized V2 run's own
//! `.p0j` evidence through the production join and admission (`analytics::backtest`): the job's
//! sealed typed result lands under `reports/backtests|optimize/`; counterfactual Shadow results
//! are never realized profit; a saved profile (`data/profiles/`) is an offline Shadow candidate,
//! never a LIVE setting. No JSON, no Python, no network. Both are jobs of the analytics job
//! controller (`analytics::jobs`, B03): `backtest|optimize` run one in the foreground, `job
//! start` records the same request and runs it in a detached worker (`job run`, internal) — the
//! action the GUI starts — `job cancel` stops a running one, `job status` shows the store.
//!
//! `report` and `verify` read runs with native capture records natively (B7, strict/off); the
//! pinned Python readers serve only runs without native records and only on `--historical`
//! (JFI-01/03, P0-PROD-CTRL-02: never a fallback for a native run). `qualify` is the separate C0-E1 master qualification after the
//! verification (PHASENPLAN L.2): it binds the report's axes, the run's final technical record
//! and the owners' end, persists an immutable result and exits 0 only for `C0_E1_MASTER_PASS`.
use darkbot_runner::analytics::journal;
use darkbot_runner::analytics::measurement::RunHorizon;
use darkbot_runner::analytics::{backtest, jobs, qualification, report};
use darkbot_runner::orchestration::boot_audit::{self, AuditVerdict, OfflineInputs, SafetyFlags};
use darkbot_runner::orchestration::control::{self, OwnerState};
use darkbot_runner::orchestration::source_control;
use darkbot_runner::run_layout::{self, find_run, native_layout, native_run, RunDir, RUN_ROOTS};
use darkbot_runner::storage::preflight;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

fn project_dir(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    let dir = explicit
        .or_else(|| std::env::var_os("P0_PROJECT_DIR").map(PathBuf::from))
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    dir.canonicalize()
        .map_err(|e| format!("PROJECT_DIR {}: {e}", dir.display()))
}

fn lock_path(project: &Path) -> PathBuf {
    control::lock_path(project)
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
    "p0ctl run start [--mode econ4h|shadow] --duration 10m|1h|5h|24h [--profile NAME] [--network-audit FILE --mainnet-approved TOKEN] [--capture-source logs|transactions]
    [--chain-verify off|strict] [--verify-rps N] [--verify-calls N] [--verify-seconds N]
    [--verify-bytes N] [--verify-disk-bytes N] [--verify-ram-bytes N] [--verify-min-free-bytes N]
    [--verify-max-pages N] [--provider-evidence FILE.p0j] [--report-* VALUE ...] [--format table|csv]
    [-- OPERATIONAL_ARGS...]
Typed C0-E1 measurement: offline boot audit, owner lock, capture, then the post-capture chain
(verification off|strict — online strict only with provider evidence —, native report, master
qualification). LIVE and submission stay disabled.
--mode shadow: a bounded Mainnet Shadow run (P6-MT MT11/MT12) through the same audit, approval
and owner lock; only --duration, --profile, the approval and OPERATIONAL_ARGS apply.
OFFLINE_READY_NETWORK_NOT_RUN is no Mainnet release. A sealed, fresh READY network audit
and the operator's explicit --mainnet-approved TOKEN for it are required.";

/// What `run start` launches (CTRL-01/03, F-01): the C0-E1 measurement or a bounded Mainnet
/// Shadow run — both only through this controller (audit, approval, owner lock, stop).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunMode {
    Econ4h,
    Shadow,
}

impl RunMode {
    fn launcher(self) -> &'static str {
        match self {
            RunMode::Econ4h => "econ4h-launcher",
            RunMode::Shadow => "shadow-launcher",
        }
    }
    /// The run id variable the launcher and the runtime read.
    fn run_id_env(self) -> &'static str {
        match self {
            RunMode::Econ4h => "P0_ECON_RUN_ID",
            RunMode::Shadow => "P0_SHADOW_RUN_ID",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            RunMode::Econ4h => "econ4h",
            RunMode::Shadow => "shadow",
        }
    }
}

/// Options of the C0-E1 capture and its post-capture chain; a Shadow run has neither.
const ECON4H_ONLY: [&str; 15] = [
    "--capture-source",
    "--chain-verify",
    "--verify",
    "--verify-rps",
    "--verify-calls",
    "--verify-seconds",
    "--verify-bytes",
    "--verify-disk-bytes",
    "--verify-ram-bytes",
    "--verify-min-free-bytes",
    "--verify-max-pages",
    "--provider-evidence",
    "--format",
    "--verification-result",
    "--until-stopped",
];

/// The run mode and the launcher's arguments: p0ctl's own options (`--mode`, the approval) are
/// consumed here and never reach the launcher; a Shadow run refuses every C0-E1 option.
fn launch_plan(args: &[String]) -> Result<(RunMode, Vec<String>), String> {
    let options = args.split(|a| a == "--").next().unwrap_or(&[]);
    if options.iter().filter(|a| *a == "--mode").count() > 1 {
        return Err("exactly one --mode".into());
    }
    let mode = match given(options, "--mode")?.as_deref() {
        None | Some("econ4h") => RunMode::Econ4h,
        Some("shadow") => RunMode::Shadow,
        Some(other) => return Err(format!("--mode {other}: econ4h|shadow")),
    };
    if mode == RunMode::Shadow {
        if let Some(bad) = options
            .iter()
            .find(|a| ECON4H_ONLY.contains(&a.as_str()) || a.starts_with("--report-"))
        {
            return Err(format!(
                "RUN_MODE_OPTION_REFUSED: {bad} is no option of a Shadow run"
            ));
        }
    }
    let mut launcher_args = Vec::new();
    let mut rest = args.iter();
    let mut in_options = true;
    while let Some(arg) = rest.next() {
        if in_options
            && matches!(
                arg.as_str(),
                "--mainnet-approved" | "--network-audit" | "--mode"
            )
        {
            rest.next();
            continue;
        }
        in_options &= arg != "--";
        launcher_args.push(arg.clone());
    }
    Ok((mode, launcher_args))
}

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
    let args = &profile_to_operational(args)?;
    let options = args.split(|a| a == "--").next().unwrap_or(&[]);
    if options.iter().any(|a| a == "--help" || a == "-h") {
        println!("{RUN_START_USAGE}");
        return Ok(true);
    }
    let (mode, launcher_args) = launch_plan(args)?;
    // STARTUP §3: an unbounded run is a typed horizon, never a huge duration; it starts only
    // once its gates are qualified.
    if options.iter().any(|a| a == "--until-stopped") {
        if options.iter().any(|a| a == "--duration") {
            return Err("--until-stopped and --duration are exclusive".into());
        }
        let need = |flag| value(options, flag).ok_or(format!("--until-stopped requires {flag} D"));
        RunHorizon::until_stopped(&need("--checkpoint")?, &need("--minimum-runway")?)?.admit()?;
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
    let run_id = match std::env::var(mode.run_id_env()) {
        Ok(id) => id,
        Err(_) => format!(
            "{}_{}",
            mode.prefix(),
            darkbot_runner::market_observer::utc_compact(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| "BOOT_CLOCK")?
                    .as_secs()
            )
        ),
    };
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
        .arg(mode.launcher())
        .args(&launcher_args)
        .env("P0_RUNTIME_LOCK", &lock)
        .env(mode.run_id_env(), &run_id)
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
    for dir in [run.join("capture"), run.clone()] {
        if let Ok(line) = manifest_line(&dir) {
            println!("  manifest: {line}");
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
    println!(
        "  native report: {} · qualification: {}",
        has_p0j(&run.join(report::REPORT_DIR)),
        has_p0j(&run.join(qualification::QUALIFICATION_DIR))
    );
    for logs in [run.join("logs/runtime.log"), run.join("runtime.log")] {
        for line in log_tail(&logs) {
            println!("  {line}");
        }
    }
    Ok(true)
}

/// `--profile NAME` before `--` (STARTUP §6) selects the operational profile: it moves to the
/// operational arguments, which the start config and the launched runtime both read.
fn profile_to_operational(args: &[String]) -> Result<Vec<String>, String> {
    let split = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let (options, operational) = args.split_at(split);
    let (mut out, mut profile, mut it) = (Vec::new(), None, options.iter());
    while let Some(a) = it.next() {
        if a == "--profile" {
            let name = it.next().ok_or("--profile requires a value")?;
            if profile.replace(name.clone()).is_some() {
                return Err("exactly one --profile".into());
            }
        } else {
            out.push(a.clone());
        }
    }
    let Some(name) = profile else {
        return Ok(args.to_vec());
    };
    if operational.iter().any(|a| a == "--profile") {
        return Err("--profile given before and after --".into());
    }
    out.extend(["--".into(), "--profile".into(), name]);
    out.extend(operational.iter().skip(1).cloned());
    Ok(out)
}

/// `runs list`: one line per run directory under the run roots (ordered by run id), with the
/// typed records it holds and whether it is the running one.
fn runs_list(project: &Path) -> Result<bool, String> {
    let running = match control::owner_state(&lock_path(project))? {
        OwnerState::Running { run_id, .. } => Some(run_id),
        OwnerState::NotRunning { .. } => None,
    };
    let rows = run_rows(project, running.as_deref());
    if rows.is_empty() {
        println!("no runs under {RUN_ROOTS:?}");
    }
    for row in rows {
        println!("{row}");
    }
    Ok(true)
}

/// `yes` when `dir` holds a typed `.p0j` record (the native post-capture outputs).
fn has_p0j(dir: &Path) -> &'static str {
    let any = std::fs::read_dir(dir).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|e| e.path().extension().is_some_and(|x| x == "p0j"))
    });
    if any {
        "yes"
    } else {
        "no"
    }
}

fn run_rows(project: &Path, running: Option<&str>) -> Vec<String> {
    run_layout::runs(project)
        .into_iter()
        .map(|RunDir { run_id, root, path }| {
            let has = |p: &str| if path.join(p).is_file() { "yes" } else { "no" };
            format!(
                "{run_id}\troot={root}\tmanifest={}\tcompletion={}\treport={}\tqualification={}{}",
                has("capture/run_manifest.p0j"),
                has("capture/market/market_completion.p0j"),
                has_p0j(&path.join(report::REPORT_DIR)),
                has_p0j(&path.join(qualification::QUALIFICATION_DIR)),
                if running == Some(run_id.as_str()) {
                    "\tRUNNING"
                } else {
                    ""
                }
            )
        })
        .collect()
}

/// `run stop --run RUN_ID` stops only that run: another running one is refused.
fn stop_target(state: &OwnerState, wanted: Option<&str>) -> Result<(), String> {
    match (state, wanted) {
        (OwnerState::Running { run_id, .. }, Some(w)) if w != run_id => Err(format!(
            "STOP_RUN_MISMATCH: the running run is {run_id}, not {w}"
        )),
        _ => Ok(()),
    }
}

/// `flag VALUE`; a flag given without its value is an error, never ignored.
fn given(args: &[String], flag: &str) -> Result<Option<String>, String> {
    match args.iter().position(|a| a == flag) {
        None => Ok(None),
        Some(i) => args
            .get(i + 1)
            .filter(|v| !v.starts_with("--"))
            .cloned()
            .map(Some)
            .ok_or_else(|| format!("{flag} requires a value")),
    }
}

fn run_stop(project: &Path, args: &[String]) -> Result<bool, String> {
    let state = control::owner_state(&lock_path(project))?;
    stop_target(&state, given(args, "--run")?.as_deref())?;
    match state {
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

/// The typed run manifest of `dir` as one status line: the plan the runtime ran, whether the plan
/// journal beside it is exactly that plan, the LIVE/submission flags and the operational RPC rate
/// with its source (JFI-01: read natively, no JSON view).
fn manifest_line(dir: &Path) -> Result<String, String> {
    use darkbot_runner::analytics::measurement::{
        MeasurementPlan, RunManifest, PLAN_JOURNAL, RUN_MANIFEST_JOURNAL,
    };
    let m: RunManifest =
        journal::read_typed_value(&dir.join(RUN_MANIFEST_JOURNAL)).map_err(|e| e.to_string())?;
    let plan_bound = match journal::read_typed_value::<MeasurementPlan>(&dir.join(PLAN_JOURNAL)) {
        Ok(p) if p == m.plan => "yes",
        Ok(_) => "NO",
        Err(_) => "PLAN_JOURNAL_MISSING",
    };
    let rpc = m.operational_config.as_ref().map_or_else(
        || "rpc_rps=UNKNOWN".to_string(),
        |c| {
            format!(
                "rpc_rps={} source={}",
                c.limits.rpc_rps,
                c.sources.get("rpc_rps").map_or("UNKNOWN", String::as_str)
            )
        },
    );
    Ok(format!(
        "run_id={} plan_duration={}s plan_bound={plan_bound} live_enabled={} submission_enabled={} {rpc}",
        m.run_id, m.plan.duration_seconds, m.live_enabled, m.submission_enabled
    ))
}

/// JFI-01/03: the pinned Python readers serve historical runs (no native records) only, and only
/// on the explicit `--historical` request — never a fallback for a native run.
fn historical(project: &Path, script: &str, args: &[String]) -> Result<bool, String> {
    if value(args, "--run")
        .and_then(|r| find_run(project, &r))
        .is_some_and(|run| native_run(&run))
    {
        return Err(format!(
            "NATIVE_RUN_REFUSED: {script} is the pinned reader of historical runs; a native run is read natively"
        ));
    }
    if !args.iter().any(|a| a == "--historical") {
        return Err(format!(
            "HISTORICAL_READER_REQUIRES_OPT_IN: {script} reads historical runs only; add --historical"
        ));
    }
    let rest: Vec<String> = args
        .iter()
        .filter(|a| *a != "--historical")
        .cloned()
        .collect();
    delegate(project, script, &rest)
}

fn report_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    let Some(run) = value(args, "--run")
        .and_then(|r| find_run(project, &r))
        .filter(|run| native_run(run))
    else {
        return historical(project, "report.sh", args);
    };
    native_layout(&run)?;
    if !run.join("capture/market/market_completion.p0j").is_file() {
        return Err(
            "CAPTURE_NOT_FINALIZED: a native run without MARKET_COMPLETION has no report".into(),
        );
    }
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

/// C0E1-P5-CLI: one verify contract. A run with native records verifies `strict` or `off` (an
/// explicit `--mode`) through the typed native driver (the request `p0-strict-verify --run`
/// takes: online strict only with `--provider-evidence`); `index`/`crosscheck` are refused for
/// it. Only a historical run reaches the pinned Python verifier, on `--historical`.
fn verify_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    use darkbot_runner::market_observer::strict_driver;
    let Some(run) = value(args, "--run")
        .and_then(|r| find_run(project, &r))
        .filter(|run| native_run(run))
    else {
        return historical(project, "verify.sh", args);
    };
    native_layout(&run)?;
    let mode = value(args, "--mode").ok_or("VERIFY_MODE_REQUIRED: --mode strict|off")?;
    if !matches!(mode.as_str(), "strict" | "off") {
        return Err(format!(
            "VERIFY_MODE_REFUSED: {mode} exists only as the pinned historical reader; a native run verifies strict or off"
        ));
    }
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
    native_layout(&run)?;
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
    native_layout(&run)?;
    let opts = report::Options {
        verification_result: value(args, "--verification-result"),
        ..Default::default()
    };
    let (record, path) = qualification::qualify(&run, &opts)?;
    print!("{}", qualification::render(&record));
    println!("record={}", path.display());
    Ok(record.verdict == qualification::MASTER_PASS)
}

const SOURCE_USAGE: &str =
    "usage: p0ctl source set wss|geyser:PROVIDER off|observe|active | p0ctl source status";

fn source_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    let dir = source_control::request_dir(project);
    match args {
        [set, unit, mode] if set == "set" => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| "SOURCE_CONTROL_CLOCK")?
                .as_secs();
            let seq = source_control::write_request(&dir, unit, mode, now)?;
            match control::owner_state(&lock_path(project))? {
                OwnerState::Running { pid, run_id, .. } => {
                    source_control::signal(pid)?;
                    println!("[p0ctl] source request seq={seq} {unit} {mode} sent to runtime pid {pid} (run {run_id}); the owner journals `source_control: applied|refused seq={seq}`");
                }
                OwnerState::NotRunning { .. } => println!(
                    "[p0ctl] source request seq={seq} {unit} {mode} recorded; no runtime is running, nothing applies it (a run starts from P0_SOURCE_GEYSER)"
                ),
            }
            Ok(true)
        }
        [status] if status == "status" => {
            let request = source_control::latest_request(&dir)?;
            match &request {
                Some(r) => println!(
                    "last source request: seq={} unit={} mode={} requested_unix_s={}",
                    r.seq, r.unit, r.mode, r.requested_unix_s
                ),
                None => println!("no source request"),
            }
            // The applied state is the owner's decision, not the request.
            let decision = source_control::latest_outcome(&dir)?;
            match &decision {
                Some(d) => println!(
                    "last owner decision: seq={} {} request=\"{}\" detail={} owner_pid={} decided_unix_s={}",
                    d.seq,
                    if d.applied { "APPLIED" } else { "REFUSED" },
                    d.request,
                    d.detail,
                    d.owner_pid,
                    d.decided_unix_s.map_or("UNKNOWN".into(), |s| s.to_string())
                ),
                None => println!("no owner decision"),
            }
            let decided = decision.as_ref().map_or(0, |d| d.seq);
            if let Some(r) = request.filter(|r| r.seq > decided) {
                println!("undecided: requests after seq={decided} up to seq={} (no runtime has applied them yet)", r.seq);
            }
            match control::owner_state(&lock_path(project))? {
                OwnerState::Running { pid, run_id, .. } => println!(
                    "runtime: running pid={pid} run_id={run_id}; last decision from this runtime: {}",
                    decision.is_some_and(|d| d.owner_pid == pid)
                ),
                OwnerState::NotRunning { .. } => println!("runtime: not running"),
            }
            Ok(true)
        }
        _ => Err(SOURCE_USAGE.into()),
    }
}

fn run_of(project: &Path, args: &[String]) -> Result<PathBuf, String> {
    let wanted = given(args, "--run")?.ok_or("--run ID|PATH required")?;
    let run = find_run(project, &wanted).ok_or("RUN_NOT_FOUND_OR_AMBIGUOUS")?;
    native_layout(&run)?;
    Ok(run)
}

/// `backtest|optimize`: one analytics job in the foreground (the same request and worker body as
/// `job start`), its result rendered in full.
fn foreground_job(project: &Path, kind: &str, args: &[String]) -> Result<bool, String> {
    let request = jobs::request_of(project, kind, args)?;
    jobs::submit(project, &request)?;
    let (outcome, executed) = jobs::run(project, &request.job_id, None)?;
    match executed {
        Some(jobs::Executed::Backtest(b)) => print!("{}", backtest::render_backtest(&b)),
        Some(jobs::Executed::Optimize {
            result,
            path,
            profile,
        }) => {
            print!("{}", backtest::render_optimize(&result, &path));
            if let jobs::JobSpec::Optimize {
                save_profile: true, ..
            } = request.spec
            {
                match profile {
                    Some(saved) => println!(
                        "offline Shadow candidate profile (never a LIVE setting): {}",
                        saved.display()
                    ),
                    None => println!("no profile saved: no point chosen (NO_SAMPLE)"),
                }
            }
        }
        Some(jobs::Executed::PostCapture(chain)) => print!("{}", chain.text),
        None => return Err(outcome.code),
    }
    println!("job {} {}", outcome.job_id, outcome.state);
    Ok(true)
}

const JOB_USAGE: &str = "usage: p0ctl job start backtest|optimize|post-capture [--id ID] … | job cancel --id ID | job status [--id ID]";

/// `job start|cancel|status|run` (B03, P0-PROD-CTRL-03): the analytics job controller.
fn job_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    let id = || given(args, "--id")?.ok_or_else(|| "--id ID required".to_string());
    match args.first().map(String::as_str) {
        Some("start") => {
            let kind = args.get(1).ok_or(JOB_USAGE)?;
            let request = jobs::request_of(project, kind, &args[2..])?;
            let worker = std::env::current_exe().map_err(|e| format!("JOB_WORKER: {e}"))?;
            jobs::start(project, &request, &worker)?;
            println!("job {} started ({})", request.job_id, request.spec.kind());
            Ok(true)
        }
        Some("cancel") => {
            let pid = jobs::cancel(project, &id()?)?;
            println!("job cancel requested (worker pid {pid})");
            Ok(true)
        }
        Some("status") => {
            let all = match given(args, "--id")? {
                Some(one) => vec![jobs::status(project, &one)?],
                None => jobs::list(project, usize::MAX)?,
            };
            if all.is_empty() {
                println!("no analytics jobs");
            }
            for s in &all {
                println!("{}", jobs::render(s));
            }
            Ok(true)
        }
        Some("run") => {
            control::install_stop_handler()?;
            let (outcome, _) = jobs::run(project, &id()?, Some(control::stop_requested))?;
            println!("job {} {} {}", outcome.job_id, outcome.state, outcome.code);
            Ok(outcome.state == jobs::COMPLETED)
        }
        _ => Err(JOB_USAGE.into()),
    }
}

/// `profiles compare`: both profiles backtested on the same run (same inputs, same engine).
fn profiles_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    if args.first().map(String::as_str) != Some("compare") {
        return Err("usage: p0ctl profiles compare --run RUN_ID --baseline FILE --candidate FILE --max-seconds N --max-ram-bytes N".into());
    }
    let run = run_of(project, args)?;
    let budget = backtest::parse_budget(args)?;
    let wanted = given(args, "--verification-result")?;
    for side in ["--baseline", "--candidate"] {
        let file = given(args, side)?.ok_or_else(|| format!("{side} FILE required"))?;
        let profile = backtest::Profile::load(Path::new(&file))?;
        let b = backtest::backtest(&run, wanted.as_deref(), &profile.params, budget)?;
        println!(
            "{} profile {} (origin {}):",
            &side[2..],
            profile.id()?,
            profile.origin
        );
        print!("{}", backtest::render_backtest(&b));
    }
    Ok(true)
}

fn delegate(project: &Path, script: &str, args: &[String]) -> Result<bool, String> {
    let e = Command::new(project.join("scripts").join(script))
        .args(args)
        .exec();
    Err(format!("EXEC {script}: {e}"))
}

/// An approval profile is a small operator file; anything larger is not one.
const APPROVAL_MAX_BYTES: u64 = 64 * 1024;

/// FE-04 `fe04 accounting|archive`: see the module documentation.
fn fe04_cmd(args: &[String]) -> Result<bool, String> {
    const USAGE: &str =
        "usage: p0ctl fe04 accounting|archive|migrate --journal-dir DIR --wallet PUBKEY";
    let dir = value(args, "--journal-dir").ok_or(USAGE)?;
    let wallet = value(args, "--wallet").ok_or(USAGE)?;
    let dir = Path::new(&dir);
    match args.first().map(String::as_str) {
        Some("accounting") => {
            let ledger = darkbot_runner::execution::accounting::Ledger::load(dir, &wallet)?;
            for line in ledger.report() {
                println!("{line}");
            }
            println!(
                "note=offline accounting of the FE-04 journals; real Mainnet attempts NOT_RUN"
            );
            Ok(!matches!(
                ledger.verdict(),
                darkbot_runner::execution::accounting::Verdict::Unknown(_)
            ))
        }
        Some("archive") => {
            // Opening creates a journal; without an attempt there is nothing to archive.
            if !dir.join(format!("{wallet}.p0j")).exists() {
                return Err("ARCHIVE_NO_ATTEMPT".into());
            }
            let journal = darkbot_runner::execution::one_shot::Journal::open(dir, &wallet)?;
            let path = journal.archive()?;
            println!("archived={}", path.display());
            Ok(true)
        }
        Some("migrate") => {
            use darkbot_runner::execution::one_shot::{migrated_legacy, Journal};
            let sha = Journal::migrate_legacy(dir, &wallet)?;
            println!(
                "migrated legacy_sha256={sha} evidence={}",
                migrated_legacy(dir, &wallet).display()
            );
            Ok(true)
        }
        _ => Err(USAGE.into()),
    }
}

/// FE-04 `approval show FILE`: see the module documentation.
fn approval_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    use darkbot_runner::execution::one_shot::{exit::ExitApproval, Approval};
    const USAGE: &str = "usage: p0ctl approval show FILE [--journal-dir DIR]";
    let (show, file, journal_dir) = match args {
        [show, file] => (show, file, None),
        [show, file, flag, dir] if flag == "--journal-dir" => (show, file, Some(Path::new(dir))),
        _ => return Err(USAGE.into()),
    };
    if show != "show" {
        return Err(USAGE.into());
    }
    let path = Path::new(file);
    if std::fs::metadata(path)
        .map_err(|_| "APPROVAL_UNREADABLE")?
        .len()
        > APPROVAL_MAX_BYTES
    {
        return Err("APPROVAL_UNREADABLE: larger than 64 KiB".into());
    }
    let bytes = std::fs::read(path).map_err(|_| "APPROVAL_UNREADABLE")?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "WALL_CLOCK_UNAVAILABLE")?
        .as_secs();
    let attempt = serde_json::from_slice::<Approval>(&bytes);
    let exit = serde_json::from_slice::<ExitApproval>(&bytes);
    let mut lines: Vec<(String, String)> = Vec::new();
    let mut put = |k: &str, v: String| lines.push((k.to_string(), v));
    // FE-04: without `--journal-dir` the history is not checked (and not claimed).
    let mut history_ok = true;
    let (check, code_sha, binary_sha) = match (attempt, exit) {
        (Ok(a), Err(_)) => {
            put("approval", "attempt".into());
            put("approval_id", a.approval_id.clone());
            put("wallet", a.wallet.clone());
            put("program", a.program.clone());
            put("program_data_hash", a.program_data_hash.clone());
            put("funding_class", a.funding_class.clone());
            put(
                "allowed_targets",
                a.allowed_targets
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(","),
            );
            put("max_capital_usd_micro", a.max_capital_usd_micro.to_string());
            put(
                "max_total_sol_debit_lamports",
                a.max_total_sol_debit_lamports.to_string(),
            );
            put("max_tip_lamports", a.max_tip_lamports.to_string());
            put("max_rent_lamports", a.max_rent_lamports.to_string());
            put("max_loss_usd_micro", a.max_loss_usd_micro.to_string());
            put("min_net_usd_micro", a.min_net_usd_micro.to_string());
            put("max_slippage_bps", a.max_slippage_bps.to_string());
            put("max_state_age_ms", a.max_state_age_ms.to_string());
            put("expires_unix_s", a.expires_unix_s.to_string());
            if let Some(dir) = journal_dir {
                history_ok = attempt_history(dir, &a, &mut put);
            }
            (a.check(now), a.code_sha, a.binary_sha256)
        }
        (Err(_), Ok(e)) => {
            put("approval", "exit".into());
            put("approval_id", e.approval_id.clone());
            put("attempt_approval_id", e.attempt_approval_id.clone());
            put("wallet", e.wallet.clone());
            put("mint", e.mint.clone());
            put("route_program", e.route_program.clone());
            put("min_out_lamports", e.min_out_lamports.to_string());
            put("max_fee_lamports", e.max_fee_lamports.to_string());
            put("max_slippage_bps", e.max_slippage_bps.to_string());
            put("max_state_age_ms", e.max_state_age_ms.to_string());
            put("expires_unix_s", e.expires_unix_s.to_string());
            (e.check(now), e.code_sha, e.binary_sha256)
        }
        _ => {
            return Err(
                "APPROVAL_UNREADABLE: neither an attempt nor an exit approval (schema 1)".into(),
            )
        }
    };
    let head = boot_audit::clean_source_head(project);
    let binary = boot_audit::binary_identity().ok();
    let binding = |approved: &str, this: Option<&str>, what: &str| match this {
        Some(t) if t == approved => format!("{approved} MATCH"),
        Some(t) => format!("{approved} MISMATCH (this {what}: {t})"),
        None => format!("{approved} UNKNOWN (this {what}: not determinable)"),
    };
    put(
        "code_sha",
        binding(&code_sha, head.as_deref(), "project: clean HEAD"),
    );
    put(
        "binary_sha256",
        binding(&binary_sha, binary.as_deref(), "build: p0ctl+p0-longtail"),
    );
    put(
        "file_sha256",
        solana_sdk::hash::hash(&bytes)
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    );
    put(
        "valid",
        match &check {
            Ok(()) => "yes".into(),
            Err(code) => format!("no ({code})"),
        },
    );
    put("live", "LOCKED (FE-06: separate explicit approval)".into());
    for (k, v) in &lines {
        println!("{k}: {v}");
    }
    Ok(check.is_ok()
        && history_ok
        && head.as_deref() == Some(&code_sha)
        && binary.as_deref() == Some(&binary_sha))
}

/// FE-04: an attempt approval against the wallet's journal history; `true` only if a new attempt
/// could start (approval not spent, no unfinished active attempt, history readable).
fn attempt_history(
    dir: &Path,
    a: &darkbot_runner::execution::one_shot::Approval,
    put: &mut dyn FnMut(&str, String),
) -> bool {
    use darkbot_runner::execution::accounting::{Ledger, Verdict};
    let consumed =
        darkbot_runner::execution::one_shot::archive::is_consumed(dir, &a.wallet, &a.approval_id);
    put("approval_consumed", consumed.to_string());
    let ledger = match Ledger::load(dir, &a.wallet) {
        Ok(l) => l,
        Err(e) => {
            put("history", format!("UNREADABLE ({e})"));
            return false;
        }
    };
    let active = ledger.attempts.iter().find(|r| !r.archived);
    put(
        "active_attempt",
        active.map_or("none".into(), |r| {
            format!("{} {}", r.approval_id, r.state.name())
        }),
    );
    let t = ledger.totals();
    put(
        "history_attempts",
        format!("{} (archived {})", t.attempts, t.archived),
    );
    put(
        "history_cumulative_realized_net_usd_micro",
        t.realized_net_usd_micro
            .map_or("UNKNOWN".into(), |v| v.to_string()),
    );
    put(
        "history_verdict",
        match ledger.verdict() {
            Verdict::Unknown(r) => format!("UNKNOWN ({})", r.join(",")),
            v => v.name().into(),
        },
    );
    !consumed && active.is_none()
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
            (Some("run"), Some("stop")) => run_stop(&project, &rest[1..]),
            (Some("run"), Some("preflight")) => preflight_cmd(&project, &rest[1..]),
            (Some("runs"), Some("list")) => runs_list(&project),
            (Some("verify"), _) => verify_cmd(&project, &rest),
            (Some("report"), _) => report_cmd(&project, &rest),
            (Some("qualify"), _) => qualify_cmd(&project, &rest),
            (Some("post-capture"), _) => post_capture_cmd(&project, &rest),
            (Some("source"), _) => source_cmd(&project, &rest),
            (Some("backtest"), _) => foreground_job(&project, "backtest", &rest),
            (Some("optimize"), _) => foreground_job(&project, "optimize", &rest),
            (Some("job"), _) => job_cmd(&project, &rest),
            (Some("profiles"), _) => profiles_cmd(&project, &rest),
            (Some("approval"), _) => approval_cmd(&project, &rest),
            (Some("fe04"), _) => fe04_cmd(&rest),
            _ => Err("usage: p0ctl [--project DIR] doctor|preflight|run preflight|run start|run status|run stop|runs list|verify|report|qualify|post-capture|source|backtest|optimize|job|profiles|approval|fe04 …".into()),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    /// STARTUP §6: `--profile` before `--` reaches the operational arguments (once).
    #[test]
    fn profile_moves_to_the_operational_arguments() {
        assert_eq!(
            profile_to_operational(&strings(&["--duration", "75m", "--profile", "standard"])),
            Ok(strings(&[
                "--duration",
                "75m",
                "--",
                "--profile",
                "standard"
            ]))
        );
        assert_eq!(
            profile_to_operational(&strings(&["--profile", "smoke", "--", "--rpc-rps", "2"])),
            Ok(strings(&["--", "--profile", "smoke", "--rpc-rps", "2"]))
        );
        let same = strings(&["--duration", "1h", "--", "--profile", "smoke"]);
        assert_eq!(profile_to_operational(&same), Ok(same.clone()));
        for bad in [
            &["--profile"][..],
            &["--profile", "a", "--profile", "b"],
            &["--profile", "a", "--", "--profile", "b"],
        ] {
            assert!(profile_to_operational(&strings(bad)).is_err(), "{bad:?}");
        }
    }

    /// JFI-01/03, P0-PROD-CTRL-02: a run with native records is never handed to a pinned Python
    /// reader (report and verify stay native; `index`/`crosscheck` are refused for it), and a
    /// historical run reaches the pinned reader only on the explicit `--historical` request.
    #[test]
    fn native_runs_never_reach_the_pinned_python_readers() {
        let project = std::env::temp_dir().join(format!("p0ctl_route_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        let native = project.join("data/runs/n/capture");
        std::fs::create_dir_all(&native).unwrap();
        std::fs::write(native.join("run_manifest.p0j"), b"").unwrap();
        std::fs::create_dir_all(project.join("data/runs/h")).unwrap();
        let err = |r: Result<bool, String>| r.unwrap_err();
        for script in ["report.sh", "verify.sh"] {
            assert!(err(historical(
                &project,
                script,
                &strings(&["--run", "n", "--historical"])
            ))
            .starts_with("NATIVE_RUN_REFUSED"));
            assert!(err(historical(&project, script, &strings(&["--run", "h"])))
                .starts_with("HISTORICAL_READER_REQUIRES_OPT_IN"));
        }
        assert!(err(report_cmd(
            &project,
            &strings(&["--run", "n", "--historical"])
        ))
        .starts_with("CAPTURE_NOT_FINALIZED"));
        assert!(err(verify_cmd(&project, &strings(&["--run", "n"])))
            .starts_with("VERIFY_MODE_REQUIRED"));
        for mode in ["index", "crosscheck"] {
            assert!(err(verify_cmd(
                &project,
                &strings(&["--run", "n", "--mode", mode])
            ))
            .starts_with("VERIFY_MODE_REFUSED"));
        }
        let _ = std::fs::remove_dir_all(project);
    }

    /// F-01 (CTRL-01/03): a Shadow run starts only through `run start --mode shadow` — the
    /// shadow launcher, p0ctl's own options consumed, every C0-E1 option refused; econ4h stays
    /// the default; an unknown, repeated or value-less `--mode` fails closed.
    #[test]
    fn run_mode_selects_the_launcher_and_refuses_foreign_options() {
        let plan = |a: &[&str]| launch_plan(&strings(a));
        let (mode, args) = plan(&[
            "--mode",
            "shadow",
            "--duration",
            "3m",
            "--network-audit",
            "n.p0j",
            "--mainnet-approved",
            "t",
            "--",
            "--mode",
            "x",
        ])
        .unwrap();
        assert_eq!(
            (mode, mode.launcher()),
            (RunMode::Shadow, "shadow-launcher")
        );
        assert_eq!(args, strings(&["--duration", "3m", "--", "--mode", "x"]));
        let (mode, args) = plan(&["--duration", "10m", "--chain-verify", "strict"]).unwrap();
        assert_eq!(
            (mode, mode.launcher()),
            (RunMode::Econ4h, "econ4h-launcher")
        );
        assert_eq!(
            args,
            strings(&["--duration", "10m", "--chain-verify", "strict"])
        );
        for bad in [
            &[
                "--mode",
                "shadow",
                "--duration",
                "3m",
                "--chain-verify",
                "off",
            ][..],
            &["--mode", "shadow", "--report-top", "5"],
            &["--mode", "shadow", "--until-stopped"],
            &["--mode", "live", "--duration", "3m"],
            &["--mode"],
            &["--mode", "shadow", "--mode", "shadow"],
        ] {
            assert!(plan(bad).is_err(), "{bad:?}");
        }
    }

    /// Review 1: a native run in layout V1 (typed manifest at the run root) has no native reader;
    /// every native command refuses it with its own code instead of misreading it as unfinished.
    #[test]
    fn native_v1_runs_are_refused_with_their_own_code() {
        let project = std::env::temp_dir().join(format!("p0ctl_v1_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        let run = project.join("data/econ4h/v1run");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::write(run.join("run_manifest.p0j"), b"").unwrap();
        let refused =
            |r: Result<bool, String>| r.unwrap_err().starts_with("NATIVE_LAYOUT_V1_UNSUPPORTED");
        let args = strings(&["--run", "v1run"]);
        assert!(refused(report_cmd(&project, &args)));
        assert!(refused(verify_cmd(
            &project,
            &strings(&["--run", "v1run", "--mode", "off"])
        )));
        assert!(refused(qualify_cmd(&project, &args)));
        assert!(refused(post_capture_cmd(&project, &args)));
        assert!(refused(foreground_job(&project, "backtest", &args)));
        assert!(refused(job_cmd(
            &project,
            &strings(&["start", "optimize", "--run", "v1run"])
        )));
        let _ = std::fs::remove_dir_all(&project);
    }

    /// A flag given without its value is an error, never an absent flag (`run stop --run`).
    #[test]
    fn a_flag_without_its_value_fails_closed() {
        assert_eq!(
            given(&strings(&["--run", "a"]), "--run"),
            Ok(Some("a".into()))
        );
        assert_eq!(given(&strings(&["x"]), "--run"), Ok(None));
        for bad in [&["--run"][..], &["--run", "--force"]] {
            assert!(given(&strings(bad), "--run").is_err(), "{bad:?}");
        }
    }

    /// `run stop --run` never stops another run; without `--run` (or not running) it proceeds.
    #[test]
    fn stop_refuses_another_running_run() {
        let running = OwnerState::Running {
            pid: 1,
            run_id: "a".into(),
            mode: "econ".into(),
            started_unix_s: 0,
        };
        assert!(stop_target(&running, Some("b"))
            .unwrap_err()
            .starts_with("STOP_RUN_MISMATCH"));
        assert_eq!(stop_target(&running, Some("a")), Ok(()));
        assert_eq!(stop_target(&running, None), Ok(()));
        let idle = OwnerState::NotRunning { last: None };
        assert_eq!(stop_target(&idle, Some("b")), Ok(()));
    }

    /// `runs list`: every run directory of the roots, ordered by id, with its typed records and
    /// the running marker; files are no runs.
    #[test]
    fn runs_are_listed_with_their_records() {
        let project = std::env::temp_dir().join(format!("p0ctl-runs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        for run in ["data/runs/b", "data/econ4h/a/capture/market"] {
            std::fs::create_dir_all(project.join(run)).unwrap();
        }
        std::fs::write(project.join("data/econ4h/a/capture/run_manifest.p0j"), b"").unwrap();
        std::fs::create_dir_all(project.join("data/econ4h/a/qualification")).unwrap();
        std::fs::write(project.join("data/econ4h/a/qualification/q.p0j"), b"").unwrap();
        std::fs::write(project.join("data/runs/file"), b"").unwrap();
        assert_eq!(
            run_rows(&project, Some("b")),
            [
                "a\troot=data/econ4h\tmanifest=yes\tcompletion=no\treport=no\tqualification=yes",
                "b\troot=data/runs\tmanifest=no\tcompletion=no\treport=no\tqualification=no\tRUNNING",
            ]
        );
        std::fs::remove_dir_all(project).unwrap();
    }
}

//! `p0ctl` — thin terminal control of the one Project_0 runtime (FEUI-01, CONTROL_PROTOCOL §2).
//!
//! It starts, observes and stops the existing launcher/runtime and runs the existing verifier and
//! report entry points; it contains no trading, chain, classification or economics logic and
//! never starts a second runtime. Observe-only: LIVE and submission stay disabled.
//!
//! ```text
//! p0ctl [--project DIR] doctor [--mode econ4h|shadow] [--scope offline|network] [--network-approved TOKEN --genesis-hash HASH --provider-evidence FILE --output FILE --capture-source logs|transactions] [-- OPERATIONAL_ARGS]
//! p0ctl [--project DIR] preflight --duration D [--capture-source logs|transactions]
//!       [--chain-verify off|index|strict|crosscheck] [--verify-rps N] [--verify-calls N] [--verify-seconds N]
//! p0ctl [--project DIR] run preflight …   (= preflight)
//! p0ctl [--project DIR] run start --duration D [--profile NAME] [test.sh options…]
//! p0ctl [--project DIR] run start --until-stopped --checkpoint D --minimum-runway D [--qualification ID]   (DEC-16: RunHorizon::admit)
//! p0ctl [--project DIR] horizon qualify --mode econ4h|market|shadow --checkpoint D --minimum-runway D [--provider-evidence ID|FILE] [-- OPERATIONAL_ARGS...]
//! p0ctl [--project DIR] horizon status --qualification ID
//! p0ctl [--project DIR] autostart preview|grant|revoke|status|start …   (DEC-16: the standing autostart approval)
//! p0ctl [--project DIR] run status [--run RUN_ID]
//! p0ctl [--project DIR] run stop [--run RUN_ID]
//! p0ctl [--project DIR] runs list
//! p0ctl [--project DIR] backtest --run RUN_ID (--profile FILE | --capital-usd LIST [--min-profit-usd X] [--funding all|A,B]) --max-seconds N --max-ram-bytes N [--verification-result ID]
//! p0ctl [--project DIR] optimize --run RUN_ID --capital-usd LIST [--min-profit-usd X] [--funding …] --min-profit-usd-grid X,Y --funding-sets all;A,B --holdout-bps N --max-points N --max-seconds N --max-ram-bytes N [--save-profile]
//! p0ctl [--project DIR] profiles compare --run RUN_ID --baseline FILE --candidate FILE --max-seconds N --max-ram-bytes N
//! p0ctl [--project DIR] job start backtest|optimize [--id ID] (the backtest|optimize options)
//! p0ctl [--project DIR] job start post-capture [--id ID] --run RUN_ID --chain-verify off|index|strict|crosscheck [--offline] [--resume-verification] [--provider-evidence FILE] --max-seconds N --max-ram-bytes N
//! p0ctl [--project DIR] job start h0 [--id ID] [--window-days N] [--min-sample N] --max-seconds N --max-ram-bytes N   (H0: every saved run's verified liquidations of the window)
//! p0ctl [--project DIR] h0 [--window-days N] [--min-sample N] --max-seconds N --max-ram-bytes N   (the same in the foreground, its report printed)
//! p0ctl [--project DIR] job cancel --id ID
//! p0ctl [--project DIR] job status [--id ID]
//! p0ctl [--project DIR] verify  --run RUN_ID --mode off|index|strict|crosscheck [--resume-verification] [--offline] [--provider-evidence FILE] [--verify-* N] …
//! p0ctl [--project DIR] report  --run RUN_ID [--report-* …] [--verification-result ID] [--format table|csv]
//! p0ctl [--project DIR] qualify --run RUN_ID [--verification-result ID]
//! p0ctl [--project DIR] post-capture --run RUN_ID --chain-verify off|index|strict|crosscheck [--provider-evidence FILE] [--verify-* N] [--report-* …]
//! p0ctl [--project DIR] source set wss|geyser:PROVIDER off|observe|active
//! p0ctl [--project DIR] capability set LIVE OFF|LIVE | HOLD_B OFF|OBSERVE|ACTIVE | JITO_BUNDLE OFF|OBSERVE|ACTIVE | THRESHOLD_TRIGGER OFF|OBSERVE|ACTIVE | INVENTORY_REBALANCING OFF|OBSERVE|ACTIVE [--expect-version N] | capability status
//! p0ctl [--project DIR] boot offline|network|start|cancel|status … (DEC-07: the boot controller)
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
//! (`Approval`) or an exit (`ExitApproval`), the native `key = value` text authored by the
//! operator (DEC-10: `approval_file`; a JSON approval is refused, the `historical` build shows it
//! as never authorizing) — checks it
//! alone (schema, authorization, ids, keys, bounds, expiry) and prints what it binds: wallet,
//! program/deployment or mint/route, every maximum and the file's SHA-256, and whether its
//! `code_sha` is this project's clean HEAD and its `binary_sha256` these binaries. It exits 0
//! only when the status is VALID: valid, within the lifetime bound, both bindings MATCH and single
//! use checked with `--journal-dir` (the wallet's FE-04 history: an attempt approval unused and no
//! other attempt active, an exit approval unused with the open position of its attempt; the
//! history, cumulative realized net and verdict are shown). It changes nothing, reads no network and does not
//! enable LIVE (FE-06 stays a separate explicit approval).
//!
//! `capability set` (DEC-13) records a typed capability request the same way; the owner decides
//! the effective value (LIVE stays BLOCKED until a separate FE-06 authorization).
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
//! `report` and `verify` read runs with native capture records natively (B7, strict/off); p0ctl
//! never starts a pinned Python reader (A09, JFI-01/03, P0-PROD-CTRL-02): a historical run is
//! read with the isolated `scripts/report.sh` / `scripts/verify.sh`. `qualify` is the separate C0-E1 master qualification after the
//! verification (PHASENPLAN L.2): it binds the report's axes, the run's final technical record
//! and the owners' end, persists an immutable result and exits 0 only for `C0_E1_MASTER_PASS`.
use darkbot_runner::analytics::journal;
use darkbot_runner::analytics::measurement::RunHorizon;
use darkbot_runner::analytics::{backtest, jobs, qualification, report};
use darkbot_runner::orchestration::boot_audit::{self, AuditVerdict, OfflineInputs, SafetyFlags};
use darkbot_runner::orchestration::control::{self, OwnerState};
use darkbot_runner::orchestration::source_control;
use darkbot_runner::orchestration::until_stopped;
use darkbot_runner::run_layout::{self, find_run, native_layout, native_run, RunDir, RUN_ROOTS};
use darkbot_runner::storage::preflight;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::str::FromStr;
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
    // DEC-05: the audit resolves the operational config of the mode `run start` will launch
    // (its profile), so the audited identity is the launched one.
    let mode = RunMode::of(args.split(|a| a == "--").next().unwrap_or(&[]))?;
    match value(args, "--scope").as_deref() {
        None | Some("offline") => {}
        Some("network") => {
            #[cfg(feature = "crate_health")]
            {
                return darkbot_runner::orchestration::boot_network::execute(
                    project,
                    mode.name(),
                    args,
                );
            }
            #[cfg(not(feature = "crate_health"))]
            {
                return Err("BOOT_NETWORK_REQUIRES_CRATE_HEALTH".into());
            }
        }
        Some(s) => return Err(format!("SCOPE_INVALID: {s}")),
    }
    darkbot_solana::runtime_config::Resolved::for_mode(
        mode.name(),
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
    req.verify = preflight::VerifyBudget::for_mode(
        preflight::VerifyMode::parse(
            &value(args, "--chain-verify").unwrap_or_else(|| "off".into()),
        )?,
        &darkbot_solana::runtime_config::active().limits,
    );
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
    let result = preflight::run(req, &basis, preflight::PROBE_BYTES);
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
    [--chain-verify off|index|strict|crosscheck] [--verify-rps N] [--verify-calls N] [--verify-seconds N]
    [--verify-bytes N] [--verify-disk-bytes N] [--verify-ram-bytes N] [--verify-min-free-bytes N]
    [--verify-max-pages N] [--provider-evidence FILE.p0j] [--report-* VALUE ...] [--format table|csv]
    [-- OPERATIONAL_ARGS...]
Typed C0-E1 measurement: offline boot audit, owner lock, capture, then the post-capture chain
(verification off|index|strict|crosscheck — online only with provider evidence —, native report, master
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
    /// `--mode` of `options` (p0ctl's own options, before `--`): econ4h unless shadow is given.
    fn of(options: &[String]) -> Result<Self, String> {
        if options.iter().filter(|a| *a == "--mode").count() > 1 {
            return Err("exactly one --mode".into());
        }
        match given(options, "--mode")?.as_deref() {
            None | Some("econ4h") => Ok(RunMode::Econ4h),
            Some("shadow") => Ok(RunMode::Shadow),
            Some(other) => Err(format!("--mode {other}: econ4h|shadow")),
        }
    }
    /// The operating mode (`P0_MODE`) of the runtime it launches.
    fn name(self) -> &'static str {
        match self {
            RunMode::Econ4h => "econ4h",
            RunMode::Shadow => "shadow",
        }
    }
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
        self.name()
    }
}

/// Options of the C0-E1 capture and its post-capture chain; a Shadow run has neither.
const ECON4H_ONLY: [&str; 14] = [
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
];

/// The run mode and the launcher's arguments: p0ctl's own options (`--mode`, the approval) are
/// consumed here and never reach the launcher; a Shadow run refuses every C0-E1 option.
fn launch_plan(args: &[String]) -> Result<(RunMode, Vec<String>), String> {
    let options = args.split(|a| a == "--").next().unwrap_or(&[]);
    let mode = RunMode::of(options)?;
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

/// DEC-16: the launch of an admitted unbounded run — its options without the horizon flags and
/// with `--duration until-stopped:W:R`, its operational arguments unchanged.
fn until_stopped_launch(args: &[String], spec: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = args.iter();
    let mut in_options = true;
    while let Some(a) = it.next() {
        if in_options {
            match a.as_str() {
                "--until-stopped" => continue,
                "--checkpoint" | "--minimum-runway" | "--qualification" => {
                    it.next();
                    continue;
                }
                "--" => {
                    out.extend(["--duration".to_string(), spec.to_string()]);
                    in_options = false;
                }
                _ => {}
            }
        }
        out.push(a.clone());
    }
    if in_options {
        out.extend(["--duration".to_string(), spec.to_string()]);
    }
    out
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
    let (mode, _) = launch_plan(args)?;
    // STARTUP §3 / DEC-16: an unbounded run is a typed horizon, never a huge duration; it is
    // admitted only by `RunHorizon::admit` with its complete qualification bound to exactly this
    // run (`--qualification ID`, p0ctl horizon qualify), and then still needs its approvals.
    let mut until_stopped_qualification = None;
    // DEC-16: the crashed run of exactly this binding and qualification an unbounded start
    // continues (its run id; never a halted or an ended one) — the runtime admits that again.
    let mut resumed = None;
    let admitted;
    let args = if options.iter().any(|a| a == "--until-stopped") {
        if options.iter().any(|a| a == "--duration") {
            return Err("--until-stopped and --duration are exclusive".into());
        }
        if value(options, "--chain-verify").as_deref() == Some("strict") {
            return Err("UNTIL_STOPPED_STRICT_NOT_AVAILABLE: an unbounded run is not verified window by window; start it with --chain-verify off".into());
        }
        let need = |flag| value(options, flag).ok_or(format!("--until-stopped requires {flag} D"));
        let spec = darkbot_solana::runtime_config::until_stopped(
            &need("--checkpoint")?,
            &need("--minimum-runway")?,
        )?;
        let (binding, _) = until_stopped::binding(
            project,
            mode.name(),
            args.split(|a| a == "--").nth(1).unwrap_or(&[]),
            &spec,
            &|k| std::env::var(k).ok(),
            &boot_audit::binary_identity_for,
        )?;
        let qualification = value(options, "--qualification")
            .map(|id| until_stopped::get(project, &id))
            .transpose()?;
        RunHorizon::of(darkbot_solana::runtime_config::horizon(&spec)?).admit(
            qualification.as_ref(),
            &binding,
            unix_now()?,
        )?;
        if let Some(id) = value(options, "--qualification") {
            resumed = darkbot_runner::orchestration::until_stopped_view::resumable(
                project, &binding, &id,
            );
        }
        // The admitted run launches exactly its horizon, its qualification beside it.
        until_stopped_qualification = value(options, "--qualification");
        admitted = until_stopped_launch(args, &spec);
        &admitted
    } else {
        args
    };
    let options = args.split(|a| a == "--").next().unwrap_or(&[]);
    let (mode, launcher_args) = launch_plan(args)?;
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
    // DEC-16: an unbounded horizon only as the launch of an admitted --until-stopped run.
    match darkbot_solana::runtime_config::horizon(&duration)
        .map_err(|e| format!("--duration {duration}: {e}"))?
    {
        darkbot_solana::runtime_config::Horizon::Fixed { .. } => {}
        darkbot_solana::runtime_config::Horizon::UntilStopped { .. }
            if until_stopped_qualification.is_some() => {}
        darkbot_solana::runtime_config::Horizon::UntilStopped { .. } => {
            return Err(format!("RUN_HORIZON_UNTIL_STOPPED: --duration {duration} starts only as --until-stopped --checkpoint W --minimum-runway R --qualification ID"))
        }
    }
    let lock = lock_path(project);
    if let OwnerState::Running { pid, run_id, .. } = control::owner_state(&lock)? {
        return Err(format!(
            "RUNTIME_OWNER_LOCKED pid={pid} run_id={run_id}: stop it first (p0ctl run stop)"
        ));
    }
    // X1: the same rule the launched runtime re-resolves its config with before admission.
    boot_audit::start_config(
        mode.name(),
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
    let run_id = match (resumed.clone(), std::env::var(mode.run_id_env())) {
        (Some(run), _) => {
            println!("[p0ctl] the start continues the crashed unbounded run {run}");
            run
        }
        (None, Ok(id)) => id,
        (None, Err(_)) => format!(
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
    let mut launcher = Command::new(project.join("scripts/capture.sh"));
    // Only what this start decides reaches the runtime, never an inherited value.
    for key in [
        boot_audit::ENV_UNTIL_STOPPED_QUALIFICATION,
        until_stopped::ENV_RESUME,
        darkbot_runner::orchestration::autostart::ENV_AUTOSTART,
    ] {
        launcher.env_remove(key);
    }
    if let Some(id) = &until_stopped_qualification {
        launcher.env(boot_audit::ENV_UNTIL_STOPPED_QUALIFICATION, id);
    }
    if let Some(run) = &resumed {
        launcher.env(until_stopped::ENV_RESUME, run);
    }
    let mut child = launcher
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
        std::thread::sleep(POLL);
    }
}

/// The cadence a waiting controller re-reads a worker's state at.
const POLL: Duration = Duration::from_millis(200);

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
fn unix_now() -> Result<u64, String> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "BOOT_CLOCK")?
        .as_secs())
}

const HORIZON_USAGE: &str = "usage: p0ctl horizon qualify --mode econ4h|market|shadow --checkpoint D --minimum-runway D [--provider-evidence RECORD_ID|FILE.p0j] [-- OPERATIONAL_ARGS...]
  | p0ctl horizon status --qualification ID
DEC-16: the typed qualification of an unbounded run (RunHorizon::UntilStopped): the mode's
rolling windows, SK-07 rolling segments and crash recovery on this storage, the rolling capacity
for the runway, the provider budget of a window and the provider evidence, stored by id.
p0ctl run start --until-stopped --qualification ID admits only a complete one bound to exactly
the run, which still needs its network and start approvals; LIVE stays disabled.";

/// DEC-16: `p0ctl horizon` — the qualification of an unbounded run.
fn horizon_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    let options = args.split(|a| a == "--").next().unwrap_or(&[]);
    let operational = args.split(|a| a == "--").nth(1).unwrap_or(&[]);
    let need = |flag: &str| -> Result<String, String> {
        given(options, flag)?.ok_or_else(|| format!("{flag} required\n{HORIZON_USAGE}"))
    };
    match options.first().map(String::as_str) {
        Some("qualify") => {
            let mode = need("--mode")?;
            if mode == boot_audit::MARKET_MODE {
                if !operational.is_empty() {
                    return Err("BOOT_MARKET_ARGS: the raw capture takes its configuration from its environment files, no operational arguments".into());
                }
                load_capture_env()?;
            }
            let spec = darkbot_solana::runtime_config::until_stopped(
                &need("--checkpoint")?,
                &need("--minimum-runway")?,
            )?;
            let env = |k: &str| std::env::var(k).ok();
            let (binding, resolved) = until_stopped::binding(
                project,
                &mode,
                operational,
                &spec,
                &env,
                &boot_audit::binary_identity_for,
            )?;
            let evidence = given(options, "--provider-evidence")?
                .map(|v| evidence_id(project, &v))
                .transpose()?;
            let source = boot_audit::runtime_capture_source(&|k| {
                if k == "P0_MODE" {
                    Some(mode.clone())
                } else {
                    env(k)
                }
            })?;
            let now = unix_now()?;
            let q = until_stopped::qualify(
                binding.clone(),
                &resolved.config,
                source,
                &until_stopped::Inputs {
                    project,
                    runs_root: project.join("data/runs"),
                    evidence_id: evidence,
                    now,
                },
            )?;
            let id = until_stopped::put(project, &q)?;
            print!("{}", q.terminal(&id));
            let horizon = RunHorizon::of(darkbot_solana::runtime_config::horizon(&spec)?);
            match horizon.admit(Some(&q), &binding, now) {
                Ok(()) => {
                    println!("UNTIL_STOPPED_QUALIFICATION {id} ADMISSIBLE: the run still needs its network and start approvals");
                    Ok(true)
                }
                Err(gaps) => {
                    println!("UNTIL_STOPPED_QUALIFICATION {id} {gaps}");
                    Ok(false)
                }
            }
        }
        Some("status") => {
            let id = need("--qualification")?;
            let q = until_stopped::get(project, &id)?;
            print!("{}", q.terminal(&id));
            println!(
                "qualified_at_unix_s={} evidence_valid_until_unix_s={}",
                q.qualified_at_unix_s,
                q.evidence_valid_until_unix_s
                    .map_or("NONE".to_string(), |t| t.to_string())
            );
            Ok(true)
        }
        _ => Err(HORIZON_USAGE.into()),
    }
}

const AUTOSTART_USAGE: &str = "usage: p0ctl autostart preview --qualification ID --checkpoint D --minimum-runway D --genesis-hash HASH --max-runtime D (--valid-for D | --expires-unix-s N) [--capture-source logs|transactions]
  | p0ctl autostart grant --confirm DIGEST   (exactly the previewed binding)
  | p0ctl autostart revoke | p0ctl autostart status | p0ctl autostart start
DEC-16: the standing approval for the autostart of an unbounded raw capture (Dauerfreigabe),
separate from its qualification and from LIVE (disabled): bound to the run (mode, configuration
hash, executables, window, runway), its qualification, provider evidence, genesis pin, capture
source, window budget, the maximum runtime of each autostarted run and its expiry; the preview
shows that binding and its digest, and the grant records exactly it while the run still binds
to it. `start` (the app's autostart) runs the whole
boot flow under it - offline audit, network audit, start - and the capture admits itself under
it. A revocation ends it at once.";

/// DEC-16: `p0ctl autostart` — the standing approval of an unbounded raw capture's autostart.
fn autostart_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    use darkbot_runner::orchestration::autostart::{
        self as auto, Event, Grant, Preview, Terms, WindowBudget,
    };
    use darkbot_runner::orchestration::boot_control as bc;
    use darkbot_solana::runtime_config::duration_seconds;
    let need = |flag: &str| -> Result<String, String> {
        given(args, flag)?.ok_or_else(|| format!("{flag} required\n{AUTOSTART_USAGE}"))
    };
    let now = unix_now()?;
    // The run as it would start now: its binding and its window budget.
    let run_of =
        |mode: &str, duration: &str| -> Result<(until_stopped::Binding, WindowBudget), String> {
            if mode != boot_audit::MARKET_MODE {
                return Err(format!(
                    "AUTOSTART_MODE: {mode} (the app autostarts the raw capture, mode market)"
                ));
            }
            load_capture_env()?;
            let (binding, resolved) = until_stopped::binding(
                project,
                mode,
                &[],
                duration,
                &|k| std::env::var(k).ok(),
                &boot_audit::binary_identity_for,
            )?;
            Ok((binding, WindowBudget::of(&resolved.config)))
        };
    // Whether the grant permits an autostart now: its run unchanged and still admitted.
    let permitted = |g: &Grant| -> Result<(), String> {
        let (binding, budget) = run_of(&g.binding.mode, &g.duration)?;
        g.permits(&binding, &budget, now)?;
        raw_capture_source(Some(g.capture_source.clone()))
            .map_err(|e| format!("AUTOSTART_NOT_PERMITTED: {e}"))?;
        let q = until_stopped::get(project, &g.qualification_id)?;
        RunHorizon::of(darkbot_solana::runtime_config::horizon(&g.duration)?)
            .admit(Some(&q), &binding, now)
            .map_err(|gaps| format!("AUTOSTART_NOT_PERMITTED: {gaps}"))
    };
    match args.first().map(String::as_str) {
        Some("preview") => {
            let qid = need("--qualification")?;
            let q = until_stopped::get(project, &qid)?;
            let duration = darkbot_solana::runtime_config::until_stopped(
                &need("--checkpoint")?,
                &need("--minimum-runway")?,
            )?;
            let (binding, budget) = run_of(&q.binding.mode, &duration)?;
            let expires_unix_s = match (
                given(args, "--expires-unix-s")?,
                given(args, "--valid-for")?,
            ) {
                (Some(at), None) => at
                    .parse::<u64>()
                    .map_err(|_| "--expires-unix-s: unix seconds")?,
                (None, Some(d)) => now.saturating_add(duration_seconds(&d)?),
                _ => {
                    return Err(format!(
                        "AUTOSTART_EXPIRY: --valid-for D or --expires-unix-s N\n{AUTOSTART_USAGE}"
                    ))
                }
            };
            let grant = Grant::for_run(
                &q,
                &qid,
                &binding,
                &[],
                &duration,
                budget,
                Terms {
                    genesis_hash: need("--genesis-hash")?,
                    capture_source: raw_capture_source(given(args, "--capture-source")?)?,
                    max_runtime_seconds: duration_seconds(&need("--max-runtime")?)?,
                    expires_unix_s,
                },
                now,
            )?;
            let digest = grant.digest()?;
            print!("{}", grant.terminal()?);
            auto::put_preview(
                project,
                &Preview {
                    grant,
                    previewed_unix_s: now,
                },
            )?;
            println!("AUTOSTART_PREVIEW {digest}: to grant exactly this binding: p0ctl autostart grant --confirm {digest}");
            Ok(true)
        }
        Some("grant") => {
            let confirm = need("--confirm")?;
            let p = auto::preview(project)?.ok_or(
                "AUTOSTART_NO_PREVIEW: preview the binding first (p0ctl autostart preview)",
            )?;
            let shown = p.grant.digest()?;
            if confirm != shown {
                return Err(format!(
                    "AUTOSTART_CONFIRM_MISMATCH: the preview shows {shown}, not the confirmed one"
                ));
            }
            // The previewed terms bound again to the run as it would start now: the grant is
            // recorded only while that is exactly the binding confirmed.
            let g = p.grant;
            let q = until_stopped::get(project, &g.qualification_id)?;
            let (binding, budget) = run_of(&g.binding.mode, &g.duration)?;
            let grant = Grant::for_run(
                &q,
                &g.qualification_id,
                &binding,
                &g.operational,
                &g.duration,
                budget,
                Terms {
                    genesis_hash: g.genesis_hash.clone(),
                    capture_source: raw_capture_source(Some(g.capture_source.clone()))?,
                    max_runtime_seconds: g.max_runtime_seconds,
                    expires_unix_s: g.expires_unix_s,
                },
                now,
            )?;
            let digest = grant.digest()?;
            if digest != shown {
                return Err(format!(
                    "AUTOSTART_CONFIRM_MISMATCH: the run's binding changed since its preview (now {digest}); preview again"
                ));
            }
            let replaced = auto::current(project)?
                .map(|(_, g, _)| g.digest())
                .transpose()?;
            let seq = auto::record(
                project,
                &Event::Granted {
                    grant: Box::new(grant),
                    granted_unix_s: now,
                },
            )?;
            auto::clear_preview(project)?;
            println!(
                "AUTOSTART_GRANTED {digest} (event {seq}){}",
                replaced.map_or(String::new(), |r| format!("; replaces {r}"))
            );
            Ok(true)
        }
        Some("revoke") => {
            let (_, g, _) = auto::current(project)?
                .ok_or("AUTOSTART_NOT_GRANTED: no standing approval to revoke")?;
            let seq = auto::record(
                project,
                &Event::Revoked {
                    revoked_unix_s: now,
                },
            )?;
            println!("AUTOSTART_REVOKED {} (event {seq})", g.digest()?);
            Ok(true)
        }
        Some("status") => {
            let Some((seq, g, granted)) = auto::current(project)? else {
                println!("AUTOSTART_NONE: no standing approval in force");
                return Ok(false);
            };
            print!("{}", g.terminal()?);
            println!("granted_unix_s={granted} event={seq}");
            match permitted(&g) {
                Ok(()) => {
                    println!("AUTOSTART_PERMITTED {}", g.digest()?);
                    Ok(true)
                }
                Err(e) => {
                    println!("{e}");
                    Ok(false)
                }
            }
        }
        Some("start") => {
            let (_, g, _) = auto::current(project)?.ok_or(
                "AUTOSTART_NOT_GRANTED: no standing approval in force (p0ctl autostart grant)",
            )?;
            permitted(&g)?;
            let digest = g.digest()?;
            let strings = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
            let flow_id = jobs::new_id()?;
            // The whole boot flow, each step approved by the standing approval.
            if !boot_cmd(
                project,
                &strings(&[
                    "offline",
                    "--flow",
                    &flow_id,
                    "--mode",
                    &g.binding.mode,
                    "--duration",
                    &g.duration,
                ]),
            )? {
                return Err(format!("AUTOSTART_OFFLINE_BLOCKED: boot flow {flow_id}"));
            }
            auto::record(
                project,
                &Event::Used {
                    digest: digest.clone(),
                    flow_id: flow_id.clone(),
                    started_unix_s: now,
                },
            )?;
            let offline = bc::status(project, &flow_id)?.flow.offline.readiness_token;
            boot_cmd(
                project,
                &strings(&[
                    "network",
                    "--flow",
                    &flow_id,
                    "--approve",
                    &offline[..16],
                    "--capture-source",
                    &g.capture_source,
                    "--genesis-hash",
                    &g.genesis_hash,
                    "--provider-evidence",
                    &g.provider_evidence_id,
                ]),
            )?;
            let network = boot_step_end(project, &flow_id, bc::NETWORK)?;
            let token = network
                .network_record
                .map(|r| r.audit.readiness_token[..16].to_string())
                .ok_or("AUTOSTART_NETWORK_RECORD_MISSING")?;
            boot_cmd(
                project,
                &strings(&[
                    "start",
                    "--flow",
                    &flow_id,
                    "--approve",
                    &token,
                    "--qualification",
                    &g.qualification_id,
                    "--autostart",
                    &digest,
                ]),
            )?;
            let started = boot_step_end(project, &flow_id, bc::START)?;
            let run = match started.start {
                bc::StepState::Done(o) => o.run_id.unwrap_or_default(),
                _ => String::new(),
            };
            println!("AUTOSTART_STARTED {digest}: boot flow {flow_id} started {run}");
            Ok(true)
        }
        _ => Err(AUTOSTART_USAGE.into()),
    }
}

/// The flow once its `step` ended: COMPLETED, else its end is the refusal.
fn boot_step_end(
    project: &Path,
    flow_id: &str,
    step: &str,
) -> Result<darkbot_runner::orchestration::boot_control::FlowStatus, String> {
    use darkbot_runner::orchestration::boot_control::{self as bc, StepState};
    loop {
        let s = bc::status(project, flow_id)?;
        let state = if step == bc::NETWORK {
            &s.network
        } else {
            &s.start
        };
        match state {
            StepState::Done(o) if o.state == bc::COMPLETED => return Ok(s),
            StepState::Done(o) => return Err(format!("AUTOSTART_{step}_{}: {}", o.state, o.code)),
            StepState::Interrupted => return Err(format!("AUTOSTART_{step}_INTERRUPTED")),
            StepState::NotRun => return Err(format!("AUTOSTART_{step}_NOT_REQUESTED")),
            StepState::Queued | StepState::Running { .. } => std::thread::sleep(POLL),
        }
    }
}

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

/// A09 (P0-PROD-CTRL-02, JFI-01/03): p0ctl reads native runs only and never starts a pinned
/// Python reader. A run without native records is read with its isolated read-only reader
/// (`scripts/report.sh`, `scripts/verify.sh`), which refuses a native run itself (U-7).
fn not_native(project: &Path, script: &str, args: &[String]) -> String {
    match value(args, "--run") {
        None => "RUN_REQUIRED: --run RUN_ID".into(),
        Some(r) if find_run(project, &r).is_none() => format!("RUN_NOT_FOUND: {r}"),
        Some(r) => format!(
            "HISTORICAL_RUN_NOT_NATIVE: run {r} has no native capture records; p0ctl reads native runs only, a historical run is read with the isolated pinned reader project_0/scripts/{script}"
        ),
    }
}

fn report_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    let Some(run) = value(args, "--run")
        .and_then(|r| find_run(project, &r))
        .filter(|run| native_run(run))
    else {
        return Err(not_native(project, "report.sh", args));
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
/// it. A historical run is refused with its isolated pinned verifier named (A09).
fn verify_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    use darkbot_runner::market_observer::strict_driver;
    let Some(run) = value(args, "--run")
        .and_then(|r| find_run(project, &r))
        .filter(|run| native_run(run))
    else {
        return Err(not_native(project, "verify.sh", args));
    };
    native_layout(&run)?;
    let mode =
        value(args, "--mode").ok_or("VERIFY_MODE_REQUIRED: --mode off|index|strict|crosscheck")?;
    if !matches!(mode.as_str(), "off" | "index" | "strict" | "crosscheck") {
        return Err(format!(
            "VERIFY_MODE_REFUSED: {mode}: a native run verifies off|index|strict|crosscheck"
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
    // `off` is a complete outcome (NOT_RUN); a history read passes only its own verified status
    // (CANONICAL_VERIFIED, INDEX_VERIFIED, CROSSCHECK_VERIFIED — never promoted).
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
    "usage: p0ctl source set wss|geyser:PROVIDER off|observe|active [--expect-version N] | p0ctl source status";

fn source_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    let dir = source_control::request_dir(project);
    match args {
        [set, unit, mode, cas @ ..] if set == "set" => {
            // CTRL-03: `--expect-version N` (the version the operator's view showed).
            let expected = match cas {
                [] => None,
                [flag, v] if flag == "--expect-version" => {
                    Some(v.parse::<u64>().map_err(|_| {
                        format!("SOURCE_VERSION_INVALID: --expect-version {v}; {SOURCE_USAGE}")
                    })?)
                }
                _ => return Err(SOURCE_USAGE.into()),
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| "SOURCE_CONTROL_CLOCK")?
                .as_secs();
            source_control::validate(unit, mode)?;
            // Review 72b74fc1: a request nobody applies is no success; without a runtime owner of
            // this project nothing is written and the call fails (the GUI receipt says FAILED).
            let OwnerState::Running { pid, run_id, .. } =
                control::owner_state(&lock_path(project))?
            else {
                return Err("SOURCE_NO_RUNTIME: no runtime owner runs for this project; nothing would apply the request (a run starts from P0_SOURCE_GEYSER)".into());
            };
            let seq = source_control::write_request_expecting(&dir, unit, mode, now, expected)?;
            source_control::signal(pid)?;
            println!("[p0ctl] source request seq={seq} {unit} {mode} sent to runtime pid {pid} (run {run_id}); the owner journals `source_control: applied|refused seq={seq}`");
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

const BOOT_USAGE: &str = "usage: p0ctl boot offline [--flow ID] [--mode econ4h|shadow|market] --duration D|until-stopped:W:R [-- OPERATIONAL_ARGS...]
  | p0ctl boot network --flow ID --approve OFFLINE_TOKEN --genesis-hash HASH --provider-evidence RECORD_ID|FILE.p0j [--capture-source logs|transactions]
  | p0ctl boot start --flow ID --approve NETWORK_TOKEN [--duration D] [--capture-source S] [--chain-verify off|index|strict|crosscheck --provider-evidence RECORD_ID|FILE.p0j] [--qualification ID (an until-stopped flow)]
  | p0ctl boot evidence import FILE.p0j
  | p0ctl boot cancel --flow ID | p0ctl boot status [--flow ID]
The flow's duration, mode and operational arguments are fixed at its offline step and its capture
source at the network step: the audited identity is exactly the started one. Mode market is the
desktop raw capture (its environment files, no operational arguments), started through its
launcher with the admission of exactly the approved record. Provider evidence is named by the
record id of its import into the controller's store (`boot evidence import`, the typed record of
p0-journal import-provider-evidence); a local file given here is imported first.";

/// DEC-07: a boot step names provider evidence by its record id; a local record file (the
/// terminal) is imported into the controller's store first. An id never imported is refused.
fn evidence_id(project: &Path, value: &str) -> Result<String, String> {
    use darkbot_runner::orchestration::provider_evidence as pe;
    if pe::is_id(value) {
        pe::resolve(project, value)?;
        return Ok(value.to_string());
    }
    let path = Path::new(value);
    if !path.is_file() {
        return Err(format!("BOOT_PROVIDER_EVIDENCE_UNKNOWN: {value} is neither an imported record id nor a local record file"));
    }
    let id = pe::import(project, path)?;
    println!("[p0ctl] provider evidence {value} imported as record {id}");
    Ok(id)
}
/// The stored record a worker's evidence id names (the worker takes ids only).
fn evidence_path(project: &Path, id: &str) -> Result<String, String> {
    darkbot_runner::orchestration::provider_evidence::resolve(project, id)
        .map(|p| p.display().to_string())
}

/// DEC-07: the modes a boot flow starts — the runtime's (`run start`: econ4h, shadow) and the
/// desktop raw capture's (market).
fn boot_mode(options: &[String]) -> Result<&'static str, String> {
    match given(options, "--mode")?.as_deref() {
        None | Some("econ4h") => Ok("econ4h"),
        Some("shadow") => Ok("shadow"),
        Some("market") => Ok(boot_audit::MARKET_MODE),
        Some(other) => Err(format!("--mode {other}: econ4h|shadow|market")),
    }
}

/// DEC-07: a raw-capture flow audits the environment the desktop capture runs with — the
/// operator env files of its launcher (`P0_ENV_FILE`), loaded before any resolution.
fn load_capture_env() -> Result<(), String> {
    if let Some(spec) = std::env::var("P0_ENV_FILE")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        darkbot_runner::env_file::load_list(&spec)?;
    }
    Ok(())
}

/// DEC-07: the capture source a raw-capture flow audits — the configured one
/// (`P0_MARKET_SOURCE`, the admission's default logs); another one is refused.
fn raw_capture_source(asked: Option<String>) -> Result<String, String> {
    // The capture's own resolution (unset: transactions), never a default of its own.
    let configured = darkbot_runner::market_observer::source_name(
        std::env::var("P0_MARKET_SOURCE").ok().as_deref(),
    )?
    .to_string();
    match asked {
        Some(asked) if asked != configured => Err(format!(
            "BOOT_CAPTURE_SOURCE_MISMATCH: the raw capture is configured for {configured}"
        )),
        _ => Ok(configured),
    }
}

/// DEC-07: the start of a raw-capture flow — the desktop capture through its launcher
/// (`darkbotctl capture start SECONDS`, the `DARKBOT_CTL` that started this controller, else
/// the checkout's) with the admission of exactly this record, approved now: the capture admits
/// itself before any read (`boot_audit::market_admission`).
fn start_raw_capture(
    project: &Path,
    flow: &darkbot_runner::orchestration::boot_control::BootFlow,
    record: &str,
    token: &str,
    qualification: Option<String>,
    autostart: Option<String>,
) -> Result<Option<String>, String> {
    use darkbot_solana::runtime_config::{horizon, Horizon, UNTIL_STOPPED};
    // DEC-16: an unbounded flow runs the capture until stopped, admitted with its qualification.
    let length = match horizon(&flow.duration)? {
        Horizon::Fixed { seconds } => seconds.to_string(),
        Horizon::UntilStopped { .. } => UNTIL_STOPPED.to_string(),
    };
    let ctl = std::env::var_os("DARKBOT_CTL")
        .map(PathBuf::from)
        .unwrap_or_else(|| project.join("ops/darkbotctl"));
    if !ctl.is_file() {
        return Err(format!(
            "BOOT_DESKTOP_CONTROL_MISSING: {} (the raw capture starts through darkbotctl)",
            ctl.display()
        ));
    }
    let approved_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "BOOT_CLOCK")?
        .as_secs();
    let mut command = Command::new(&ctl);
    // Only what this start decides reaches the capture, never an inherited value.
    for key in [
        boot_audit::ENV_UNTIL_STOPPED_QUALIFICATION,
        until_stopped::ENV_RESUME,
        darkbot_runner::orchestration::autostart::ENV_AUTOSTART,
    ] {
        command.env_remove(key);
    }
    // DEC-16: an unbounded start continues the crashed run of exactly this binding and
    // qualification (never a halted or an ended one); the capture admits that again.
    let mut resumed = None;
    if let Some(id) = &qualification {
        command.env(boot_audit::ENV_UNTIL_STOPPED_QUALIFICATION, id);
        let (binding, _) = until_stopped::binding(
            project,
            &flow.mode,
            &flow.operational,
            &flow.duration,
            &|k| std::env::var(k).ok(),
            &boot_audit::binary_identity_for,
        )?;
        resumed =
            darkbot_runner::orchestration::until_stopped_view::resumable(project, &binding, id);
        if let Some(run) = &resumed {
            println!("[p0ctl] the start continues the crashed unbounded run {run}");
            command.env(until_stopped::ENV_RESUME, run);
        }
    }
    // DEC-16: an autostarted capture names the standing approval it runs under.
    if let Some(digest) = &autostart {
        command.env(
            darkbot_runner::orchestration::autostart::ENV_AUTOSTART,
            digest,
        );
    }
    let out = command
        .args(["capture", "start", &length])
        .env(boot_audit::ENV_NETWORK_AUDIT, record)
        .env(boot_audit::ENV_NETWORK_AUDIT_TOKEN, token)
        .env(
            boot_audit::ENV_NETWORK_AUDIT_APPROVED_AT,
            approved_at.to_string(),
        )
        .env("P0_MEASUREMENT_DURATION", &flow.duration)
        .env("P0_MODE", boot_audit::MARKET_MODE)
        .env("P0_PROJECT_DIR", project)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("BOOT_DESKTOP_CONTROL: {e}"))?;
    if out.status.success() {
        // The step names the capture it started (`darkbotctl`: "… launch=ID").
        let text = String::from_utf8_lossy(&out.stdout);
        let launch = text
            .split_whitespace()
            .find_map(|w| w.strip_prefix("launch="))
            .ok_or("BOOT_RAW_CAPTURE_UNCONFIRMED: the launcher named no started capture")?;
        // A continued run keeps its run id (the launcher passes it on).
        Ok(Some(resumed.unwrap_or_else(|| format!("app-{launch}"))))
    } else {
        let text = String::from_utf8_lossy(&out.stderr);
        let last = text
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("");
        Err(format!("BOOT_RAW_CAPTURE_REFUSED: {}", last.trim()))
    }
}

/// DEC-07 (N7): the boot controller (`orchestration::boot_control`), the terminal's and the
/// Capture tab's one path: `offline` audits locally and records a new flow with its mode,
/// operational arguments and TTL; `network` (the operator's approval of the offline token) and
/// `start` (the approval of the network token) each start that flow's step once in a detached
/// worker (`boot run`, internal), which runs the network audit, respectively `run start`, with
/// exactly the flow's mode and operational arguments; `cancel` stops a running step.
fn boot_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    use darkbot_runner::orchestration::boot_control as bc;
    let options = args.split(|a| a == "--").next().unwrap_or(&[]);
    let operational = args.split(|a| a == "--").nth(1).unwrap_or(&[]);
    let need = |flag: &str| -> Result<String, String> {
        given(options, flag)?.ok_or_else(|| format!("{flag} required\n{BOOT_USAGE}"))
    };
    let capture_source = || -> Result<String, String> {
        match given(options, "--capture-source")?.as_deref() {
            None | Some("logs") => Ok("logs".into()),
            Some("transactions") => Ok("transactions".into()),
            Some(other) => Err(format!("CAPTURE_SOURCE_INVALID: {other}")),
        }
    };
    let worker = || std::env::current_exe().map_err(|e| format!("BOOT_WORKER: {e}"));
    match options.first().map(String::as_str) {
        Some("offline") => {
            let flow_id = match given(options, "--flow")? {
                Some(id) => id,
                None => jobs::new_id()?,
            };
            let mode = boot_mode(&options[1..])?;
            if mode == boot_audit::MARKET_MODE {
                if !operational.is_empty() {
                    return Err("BOOT_MARKET_ARGS: the raw capture takes its configuration from its environment files, no operational arguments".into());
                }
                load_capture_env()?;
            }
            // DEC-07: the run's duration is part of the audited configuration: the flow audits
            // and starts exactly `run start`'s resolution (X1 `start_config`).
            // DEC-16: a fixed duration or an unbounded horizon until-stopped:W:R, in its one
            // canonical text (the text is part of the audited configuration).
            let given = need("--duration")?;
            let duration = darkbot_solana::runtime_config::canonical(&given)
                .map_err(|e| format!("--duration {given}: {e}"))?;
            let resolved =
                boot_audit::start_config(mode, operational, &duration, &|k| std::env::var(k).ok())?;
            // The TTL and its source, as the resolution names it.
            let ttl_source = resolved
                .sources
                .get("boot_token_ttl_seconds")
                .cloned()
                .unwrap_or_else(|| "builtin_default".into());
            resolved.activate()?;
            let ttl_seconds = darkbot_solana::runtime_config::active()
                .limits
                .boot_token_ttl_seconds;
            let audit = boot_audit::offline(&OfflineInputs::for_project(project))?;
            print!("{}", audit.terminal());
            let blocked = audit.verdict == AuditVerdict::Blocked;
            let token = audit.readiness_token[..16].to_string();
            bc::create_guarded(
                project,
                &bc::BootFlow {
                    flow_id: flow_id.clone(),
                    mode: mode.into(),
                    operational: operational.to_vec(),
                    duration,
                    offline: audit,
                    ttl_seconds,
                    ttl_source: ttl_source.clone(),
                    created_unix_ms: darkbot_runner::status_tap::unix_ms_now(),
                },
            )?;
            if blocked {
                println!("[p0ctl] boot flow {flow_id}: offline BLOCKED; nothing to approve");
            } else {
                println!("[p0ctl] boot flow {flow_id}: TTL {ttl_seconds} s [source {ttl_source}]; approve the network audit: p0ctl boot network --flow {flow_id} --approve {token} --genesis-hash HASH --provider-evidence RECORD_ID|FILE.p0j");
            }
            Ok(!blocked)
        }
        Some(step @ ("network" | "start")) => {
            let flow_id = need("--flow")?;
            let token = need("--approve")?;
            let s = bc::status(project, &flow_id)?;
            let mut worker_args = vec!["--approve".to_string(), token.clone()];
            let raw = s.flow.mode == boot_audit::MARKET_MODE;
            if raw {
                load_capture_env()?;
            }
            let step = if step == "network" {
                let source = if raw {
                    raw_capture_source(given(options, "--capture-source")?)?
                } else {
                    capture_source()?
                };
                // A Shadow run captures logs (`run start` passes no source for it).
                if s.flow.mode == RunMode::Shadow.name()
                    && source != boot_audit::SHADOW_CAPTURE_SOURCE
                {
                    return Err("BOOT_CAPTURE_SOURCE_SHADOW: a Shadow run captures logs".into());
                }
                // The approval first: nothing is imported for an unapproved step.
                bc::check_approval(&s, bc::NETWORK, &token)?;
                worker_args.extend([
                    "--capture-source".to_string(),
                    source,
                    "--genesis-hash".to_string(),
                    need("--genesis-hash")?,
                    "--provider-evidence".to_string(),
                    evidence_id(project, &need("--provider-evidence")?)?,
                ]);
                bc::NETWORK
            } else {
                // The start is exactly what was audited: the flow's duration and the record's
                // capture source; a different one is refused, never a drifted start.
                if let Some(d) = given(options, "--duration")? {
                    if d != s.flow.duration {
                        return Err(format!(
                            "BOOT_DURATION_MISMATCH: the flow audits --duration {}",
                            s.flow.duration
                        ));
                    }
                }
                let recorded = s.network_record.as_ref().map(|r| r.capture_source.clone());
                if let (Some(asked), Some(recorded)) =
                    (given(options, "--capture-source")?, recorded.as_ref())
                {
                    if &asked != recorded {
                        return Err(format!(
                            "BOOT_CAPTURE_SOURCE_MISMATCH: the network audit recorded {recorded}"
                        ));
                    }
                }
                // DEC-16: an unbounded flow starts only with its qualification, admitted here
                // first (the started runtime admits itself again before any read).
                let horizon = darkbot_solana::runtime_config::horizon(&s.flow.duration)?;
                if let darkbot_solana::runtime_config::Horizon::UntilStopped { .. } = horizon {
                    let id = given(options, "--qualification")?.ok_or(
                        "UNTIL_STOPPED_QUALIFICATION_REQUIRED: an unbounded flow starts with --qualification ID (p0ctl horizon qualify)",
                    )?;
                    let q = until_stopped::get(project, &id)?;
                    let (binding, _) = until_stopped::binding(
                        project,
                        &s.flow.mode,
                        &s.flow.operational,
                        &s.flow.duration,
                        &|k| std::env::var(k).ok(),
                        &boot_audit::binary_identity_for,
                    )?;
                    RunHorizon::of(horizon).admit(Some(&q), &binding, unix_now()?)?;
                    worker_args.extend(["--qualification".to_string(), id]);
                    // An autostart (`p0ctl autostart start`) names the standing approval in force.
                    if let Some(digest) = given(options, "--autostart")? {
                        if !raw {
                            return Err(
                                "AUTOSTART_MODE: only the raw capture (mode market) is autostarted"
                                    .into(),
                            );
                        }
                        darkbot_runner::orchestration::autostart::in_force(project, &digest)?;
                        worker_args.extend(["--autostart".to_string(), digest]);
                    }
                } else if given(options, "--autostart")?.is_some() {
                    return Err(
                        "AUTOSTART_UNBOUNDED_ONLY: a standing approval autostarts an unbounded run"
                            .into(),
                    );
                }
                let verify = given(options, "--chain-verify")?.unwrap_or_else(|| "off".into());
                worker_args.extend(["--chain-verify".to_string(), verify.clone()]);
                match verify.as_str() {
                    "off" => {}
                    "index" | "strict" | "crosscheck" => {
                        let evidence = given(options, "--provider-evidence")?.ok_or(
                            "STRICT_PROVIDER_EVIDENCE_REQUIRED: an online chain verification needs the operator's --provider-evidence",
                        )?;
                        bc::check_approval(&s, bc::START, &token)?;
                        worker_args.extend([
                            "--provider-evidence".to_string(),
                            evidence_id(project, &evidence)?,
                        ])
                    }
                    other => {
                        return Err(format!(
                            "CHAIN_VERIFY_{other}_INVALID: off|index|strict|crosscheck"
                        ))
                    }
                }
                bc::START
            };
            bc::begin(project, &flow_id, step, &token, &worker()?, &worker_args)?;
            println!("[p0ctl] boot flow {flow_id}: {step} approved and started; its end: p0ctl boot status --flow {flow_id}");
            Ok(true)
        }
        Some("run") => {
            control::install_stop_handler()?;
            let (flow_id, step) = (need("--flow")?, need("--step")?);
            let token = need("--approve")?;
            let outcome = bc::run_step(
                project,
                &flow_id,
                &step,
                control::stop_requested,
                |flow, dir| {
                    let record = dir.join(bc::NETWORK_RECORD).display().to_string();
                    let raw = flow.mode == boot_audit::MARKET_MODE;
                    if raw {
                        load_capture_env()?;
                    }
                    let mut call: Vec<String> = Vec::new();
                    if step == bc::NETWORK {
                        call.extend([
                            "--network-approved".into(),
                            token.clone(),
                            "--genesis-hash".into(),
                            need("--genesis-hash")?,
                            "--provider-evidence".into(),
                            evidence_path(project, &need("--provider-evidence")?)?,
                            "--provider-evidence-id".into(),
                            need("--provider-evidence")?,
                            "--capture-source".into(),
                            capture_source()?,
                            "--duration".into(),
                            flow.duration.clone(),
                            "--output".into(),
                            record,
                            "--".into(),
                        ]);
                        call.extend(flow.operational.iter().cloned());
                        #[cfg(feature = "crate_health")]
                        {
                            return match darkbot_runner::orchestration::boot_network::execute(
                                project, &flow.mode, &call,
                            )? {
                                true => Ok(None),
                                false => Err("BOOT_NETWORK_BLOCKED".into()),
                            };
                        }
                        #[cfg(not(feature = "crate_health"))]
                        {
                            let _ = call;
                            return Err("BOOT_NETWORK_REQUIRES_CRATE_HEALTH".into());
                        }
                    }
                    if raw {
                        return start_raw_capture(
                            project,
                            flow,
                            &record,
                            &token,
                            given(options, "--qualification")?,
                            given(options, "--autostart")?,
                        );
                    }
                    // DEC-16: an unbounded runtime run starts through run start --until-stopped
                    // (RunHorizon::admit with its qualification).
                    if let Some(rest) = flow
                        .duration
                        .strip_prefix(darkbot_solana::runtime_config::UNTIL_STOPPED)
                        .and_then(|r| r.strip_prefix(':'))
                    {
                        let (checkpoint, runway) = rest
                            .split_once(':')
                            .ok_or("RUN_HORIZON_UNTIL_STOPPED_FORM")?;
                        let mut call: Vec<String> = [
                            "--mode",
                            &flow.mode,
                            "--until-stopped",
                            "--checkpoint",
                            checkpoint,
                            "--minimum-runway",
                            runway,
                            "--network-audit",
                            &record,
                            "--mainnet-approved",
                            &token,
                        ]
                        .map(String::from)
                        .to_vec();
                        if let Some(id) = given(options, "--qualification")? {
                            call.extend(["--qualification".into(), id]);
                        }
                        call.push("--".into());
                        call.extend(flow.operational.iter().cloned());
                        return match run_start(project, &call)? {
                            true => Ok(None),
                            false => Err("RUN_ENDED_UNSUCCESSFUL".into()),
                        };
                    }
                    let mode = RunMode::of(&["--mode".to_string(), flow.mode.clone()])?;
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|_| "BOOT_CLOCK")?
                        .as_secs();
                    let run_id = format!(
                        "{}_{}",
                        mode.prefix(),
                        darkbot_runner::market_observer::utc_compact(now)
                    );
                    // The worker is single-threaded here: the run id `run start` launches with.
                    std::env::set_var(mode.run_id_env(), &run_id);
                    let audited: boot_audit::NetworkAudit =
                        journal::read_typed_value(Path::new(&record))
                            .map_err(|_| "BOOT_NETWORK_EVIDENCE_INVALID")?;
                    call.extend([
                        "--mode".into(),
                        flow.mode.clone(),
                        "--duration".into(),
                        flow.duration.clone(),
                        "--network-audit".into(),
                        record,
                        "--mainnet-approved".into(),
                        token.clone(),
                    ]);
                    if mode == RunMode::Econ4h {
                        call.extend([
                            "--capture-source".into(),
                            audited.capture_source.clone(),
                            "--chain-verify".into(),
                            need("--chain-verify")?,
                        ]);
                        if let Some(id) = given(options, "--provider-evidence")? {
                            call.extend([
                                "--provider-evidence".into(),
                                evidence_path(project, &id)?,
                            ]);
                        }
                    }
                    call.push("--".into());
                    call.extend(flow.operational.iter().cloned());
                    match run_start(project, &call)? {
                        true => Ok(Some(run_id)),
                        false => Err(format!("RUN_ENDED_UNSUCCESSFUL: {run_id}")),
                    }
                },
            )?;
            println!(
                "[p0ctl] boot flow {flow_id} {step} {} {}",
                outcome.state, outcome.code
            );
            Ok(outcome.state == bc::COMPLETED)
        }
        Some("evidence") if options.get(1).map(String::as_str) == Some("import") => {
            let file = options
                .get(2)
                .ok_or_else(|| format!("FILE.p0j required\n{BOOT_USAGE}"))?;
            let id =
                darkbot_runner::orchestration::provider_evidence::import(project, Path::new(file))?;
            println!("PROVIDER_EVIDENCE_ID {id} (p0ctl boot network … --provider-evidence {id})");
            Ok(true)
        }
        Some("cancel") => {
            let pid = bc::cancel(project, &need("--flow")?)?;
            println!("[p0ctl] boot step stop requested (worker pid {pid})");
            Ok(true)
        }
        Some("status") => {
            let flows = match given(options, "--flow")? {
                Some(id) => vec![bc::status(project, &id)?],
                None => bc::list(project)?,
            };
            if flows.is_empty() {
                println!("no boot flows");
            }
            for f in &flows {
                let v = bc::BootView::of(f);
                println!(
                    "flow {} mode={} ttl={}s[{}] offline={} token={} network={}{} start={}{}",
                    v.flow_id,
                    v.mode,
                    v.ttl_seconds,
                    v.ttl_source,
                    v.offline_verdict,
                    v.offline_token,
                    v.network.state,
                    v.network_token
                        .map(|t| format!(
                            " token={t} expires_unix_s={}",
                            v.expires_unix_s.unwrap_or(0)
                        ))
                        .unwrap_or_default(),
                    v.start.state,
                    v.start
                        .run_id
                        .map(|r| format!(" run={r}"))
                        .unwrap_or_default(),
                );
            }
            Ok(true)
        }
        _ => Err(BOOT_USAGE.into()),
    }
}

const CAPABILITY_USAGE: &str =
    "usage: p0ctl capability set LIVE OFF|LIVE | HOLD_B OFF|OBSERVE|ACTIVE | JITO_BUNDLE OFF|OBSERVE|ACTIVE | THRESHOLD_TRIGGER OFF|OBSERVE|ACTIVE | INVENTORY_REBALANCING OFF|OBSERVE|ACTIVE | ECON_SIGN|ECON_ARM|ECON_SEND|ECON_ATTEMPT|ECON_LIVE DISABLED|ENABLED | WALLET:<pubkey> DISABLED|ENABLED | FE06_GRANT|FE06_REVOKE <scope digest> [--expect-version N] [--expect-epoch E] [--request-id ID] | p0ctl capability status";

/// DEC-13/P8-CTRL: the operator's capability request (`set`), the same controller path as
/// `source`: written only for a running owner of this project, under a one-time request id (the
/// same id again is the same request) and the owner's epoch, CAS-versioned, then its doorbell
/// rung; the owner decides the effective value and records its receipt (LIVE stays BLOCKED until
/// every economic switch, every wallet and FE-06).
fn capability_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    use darkbot_runner::orchestration::capability_control;
    let dir = capability_control::request_dir(project);
    match args {
        [set, capability, requested, opts @ ..] if set == "set" => {
            let (mut expected, mut stated_epoch, mut request_id) = (None, None, None);
            let mut opts = opts.iter();
            while let Some(flag) = opts.next() {
                let value = opts.next().ok_or(CAPABILITY_USAGE)?;
                let number = |code: &str| {
                    value
                        .parse::<u64>()
                        .map_err(|_| format!("{code}: {flag} {value}; {CAPABILITY_USAGE}"))
                };
                match flag.as_str() {
                    "--expect-version" if expected.is_none() => {
                        expected = Some(number("CAPABILITY_VERSION_INVALID")?)
                    }
                    "--expect-epoch" if stated_epoch.is_none() => {
                        stated_epoch = Some(number("CAPABILITY_EPOCH_INVALID")?)
                    }
                    "--request-id" if request_id.is_none() => request_id = Some(value.clone()),
                    _ => return Err(CAPABILITY_USAGE.into()),
                }
            }
            capability_control::validate(capability, requested)?;
            if let Some(id) = &request_id {
                capability_control::validate_request_id(id)?;
            }
            // P8-CTRL.6: an economic or wallet switch and an FE-06 grant or revocation state
            // their version, epoch and id.
            let economic = capability_control::ECON.contains(&capability.as_str())
                || capability.starts_with(capability_control::WALLET_PREFIX)
                || capability == capability_control::FE06_GRANT
                || capability == capability_control::FE06_REVOKE;
            if economic && (expected.is_none() || stated_epoch.is_none() || request_id.is_none()) {
                return Err(format!("CAPABILITY_BINDING_REQUIRED: {capability} states --expect-version, --expect-epoch and --request-id (P8-CTRL.6); {CAPABILITY_USAGE}"));
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| "CAPABILITY_CONTROL_CLOCK")?
                .as_secs();
            let OwnerState::Running { pid, run_id, .. } =
                control::owner_state(&lock_path(project))?
            else {
                return Err("CAPABILITY_NO_RUNTIME: no runtime owner runs for this project; nothing would decide the request".into());
            };
            // P8-CTRL.6: the running owner's epoch — the one the operator states must be it.
            let epoch = capability_control::current_epoch(&dir)?;
            if epoch == 0 {
                return Err(
                    "CAPABILITY_NO_OWNER_EPOCH: the running owner opened no capability epoch"
                        .into(),
                );
            }
            // The epoch is the running owner's own (its record names this pid).
            let opener = capability_control::epoch_owner(&dir, epoch)?;
            if opener != pid {
                return Err(format!("CAPABILITY_EPOCH_FOREIGN: epoch {epoch} was opened by pid {opener}, the running owner is pid {pid}"));
            }
            if let Some(stated) = stated_epoch.filter(|e| *e != epoch) {
                return Err(format!(
                    "CAPABILITY_EPOCH_STALE: --expect-epoch {stated}, the running owner is epoch {epoch}; reload and retry"
                ));
            }
            let request_id = match request_id {
                Some(id) => id,
                None => capability_control::new_request_id()?,
            };
            let written = capability_control::write_request(
                &dir,
                &capability_control::NewRequest {
                    capability,
                    requested,
                    request_id: &request_id,
                    owner_epoch: epoch,
                    expected,
                    now_unix_s: now,
                },
            )?;
            source_control::signal(pid)?;
            println!(
                "[p0ctl] capability request seq={} {capability} {requested} request_id={request_id} epoch={epoch}{} sent to runtime pid {pid} (run {run_id}); the owner journals `capability_control: applied|refused seq={}` and records its receipt; LIVE stays effective BLOCKED until every economic switch, every wallet and FE-06",
                written.seq,
                if written.replayed {
                    " (the same request again: nothing new written)"
                } else {
                    ""
                },
                written.seq
            );
            Ok(true)
        }
        [status] if status == "status" => {
            let request = capability_control::latest_request(&dir)?;
            match &request {
                Some(r) => {
                    println!(
                    "last capability request: seq={} {} request_id={} epoch={} requested_unix_s={}",
                    r.seq,
                    capability_control::shown(&r.capability, &r.requested),
                    if r.request_id.is_empty() { "-" } else { &r.request_id },
                    r.owner_epoch,
                    r.requested_unix_s
                )
                }
                None => println!("no capability request"),
            }
            // What applies is the owner's decision, not the request; every start is DISABLED/OFF.
            let decision = capability_control::latest_outcome(&dir)?;
            match &decision {
                Some(d) => println!(
                    "last owner decision: seq={} {} request=\"{}\" requested={} effective={} detail={} owner_pid={} epoch={} decided_unix_s={}",
                    d.seq,
                    if d.applied { "APPLIED" } else { "REFUSED" },
                    d.request,
                    d.requested,
                    d.effective,
                    d.detail,
                    d.owner_pid,
                    d.owner_epoch,
                    d.decided_unix_s.map_or("UNKNOWN".into(), |s| s.to_string())
                ),
                None => println!("no owner decision"),
            }
            let decided = decision.as_ref().map_or(0, |d| d.seq);
            if let Some(r) = request.filter(|r| r.seq > decided) {
                println!("undecided: requests after seq={decided} up to seq={} (no runtime has decided them yet)", r.seq);
            }
            // P8-CTRL.6: the switches of the newest epoch as its receipts decided them.
            let epoch = capability_control::current_epoch(&dir)?;
            println!("owner epoch: {epoch} (every switch starts DISABLED and LIVE OFF in an epoch; unlisted capabilities are at their start value)");
            for d in capability_control::epoch_decisions(&dir, epoch)? {
                println!(
                    "  {} requested={} effective={} seq={} request_id={}{}",
                    d.capability,
                    d.requested,
                    d.effective,
                    d.seq,
                    d.request_id,
                    if d.block_reason.is_empty() {
                        String::new()
                    } else {
                        format!(" block_reason={}", d.block_reason)
                    }
                );
            }
            match control::owner_state(&lock_path(project))? {
                OwnerState::Running { pid, run_id, .. } => println!(
                    "runtime: running pid={pid} run_id={run_id}; last decision from this runtime: {} (a run starts with every switch DISABLED and LIVE requested OFF; state the owner's projected capability_config_version and epoch)",
                    decision.is_some_and(|d| d.owner_pid == pid)
                ),
                OwnerState::NotRunning { .. } => println!("runtime: not running (the next run opens a new epoch: every switch DISABLED, LIVE requested OFF)"),
            }
            Ok(true)
        }
        _ => Err(CAPABILITY_USAGE.into()),
    }
}

const LIVE_USAGE: &str = "usage: p0ctl live scope --file FILE | p0ctl live grant|revoke --digest D --expect-version N --expect-epoch E --request-id ID | p0ctl live status | p0ctl live attempts --journal-dir DIR --wallet PUBKEY [--digest D]";

/// P8-CTRL.3/.8: the FE-06 scope, grant and revocation — a scope is previewed natively (read,
/// validated against the registry's ceilings, stored under its digest and shown in full); a grant
/// or a revocation names that digest and is decided by the running owner through the capability
/// path (version, epoch, request id, receipt). A grant needs the operator's own authentication.
fn live_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    use darkbot_runner::orchestration::{capability_control, fe06};
    match args {
        [scope, flag, file] if scope == "scope" && flag == "--file" => {
            let bytes =
                darkbot_runner::execution::one_shot::approval_file::read_bytes(Path::new(file))
                    .map_err(|e| e.replace("APPROVAL", "FE06_SCOPE"))?;
            let limits =
                darkbot_solana::runtime_config::Resolved::parse(&[], &|k| std::env::var(k).ok())?
                    .config
                    .limits;
            let s = fe06::parse(&bytes, &fe06::Ceilings::of(&limits))?;
            let digest = fe06::put_preview(project, &s)?;
            print!("{}", fe06::terminal(&s)?);
            println!("[p0ctl] scope previewed: digest={digest}; a grant names exactly this digest (p0ctl live grant --digest {digest} --expect-version N --expect-epoch E --request-id ID)");
            Ok(true)
        }
        [verb, rest @ ..] if verb == "grant" || verb == "revoke" => {
            let digest = match rest.iter().position(|a| a == "--digest") {
                Some(i) => rest.get(i + 1).ok_or(LIVE_USAGE)?.clone(),
                None => return Err(LIVE_USAGE.into()),
            };
            let flags: Vec<String> = rest
                .iter()
                .enumerate()
                .filter(|(i, a)| {
                    *a != "--digest" && rest.get(i.wrapping_sub(1)).is_none_or(|p| p != "--digest")
                })
                .map(|(_, a)| a.clone())
                .collect();
            let capability = if verb == "grant" {
                // The scope must have been previewed: the operator confirms what was shown.
                fe06::preview(project, &digest)?;
                capability_control::FE06_GRANT
            } else {
                capability_control::FE06_REVOKE
            };
            let mut set = vec!["set".to_string(), capability.into(), digest];
            set.extend(flags);
            capability_cmd(project, &set)
        }
        [status] if status == "status" => {
            let dir = capability_control::request_dir(project);
            let epoch = capability_control::current_epoch(&dir)?;
            let decided: Vec<_> = capability_control::epoch_decisions(&dir, epoch)?
                .into_iter()
                .filter(|d| {
                    d.capability == capability_control::FE06_GRANT
                        || d.capability == capability_control::FE06_REVOKE
                })
                .collect();
            println!("owner epoch: {epoch} (no grant survives an owner restart)");
            for d in &decided {
                println!(
                    "  {} digest={} effective={} seq={} request_id={}",
                    d.capability, d.requested, d.effective, d.seq, d.request_id
                );
            }
            // Review L4: the last decided grant is in force only within its scope's validity.
            let last = decided.iter().max_by_key(|d| d.seq);
            let state = match last {
                Some(d) if d.effective == "IN_FORCE" => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|t| t.as_secs())
                        .ok();
                    match (fe06::preview(project, &d.requested), now) {
                        (Ok(s), Some(now))
                            if (s.not_before_unix_s..s.expires_unix_s).contains(&now) =>
                        {
                            format!("yes ({})", d.requested)
                        }
                        _ => capability_control::grant_not_in_force(&d.requested),
                    }
                }
                _ => "none".to_string(),
            };
            println!(
                "grant in force: {state} · {}",
                capability_control::FE06_AUTH_UNDECIDED
            );
            Ok(true)
        }
        [attempts, rest @ ..] if attempts == "attempts" => live_attempts(project, rest),
        _ => Err(LIVE_USAGE.into()),
    }
}

/// P8-CTRL.5/.8 `live attempts`: the attempts of a wallet's FE-04 journal (archived and active)
/// as they are booked against FE-06 grants — each booking, then the quotas of this UTC day, of
/// every campaign and every grant; with `--digest` that previewed scope's three quotas against its
/// limits (the same count the admission refuses a new attempt on). Read only.
fn live_attempts(project: &Path, args: &[String]) -> Result<bool, String> {
    use darkbot_runner::execution::one_shot::{archive, fe06_attempts_report};
    use std::collections::BTreeMap;
    // Strictly typed (review L2): each flag once with its value, nothing else; the wallet a
    // canonical public key (a mistyped one is no wallet without bookings).
    let mut flags: BTreeMap<&str, &str> = BTreeMap::new();
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let known = ["--journal-dir", "--wallet", "--digest"].contains(&flag.as_str());
        let value = rest.next().ok_or(LIVE_USAGE)?;
        if !known || flags.insert(flag, value).is_some() {
            return Err(LIVE_USAGE.into());
        }
    }
    let dir = flags.get("--journal-dir").ok_or(LIVE_USAGE)?.to_string();
    let wallet = flags.get("--wallet").ok_or(LIVE_USAGE)?.to_string();
    if solana_sdk::pubkey::Pubkey::from_str(&wallet)
        .map(|k| k.to_string() != wallet)
        .unwrap_or(true)
    {
        return Err(format!("FE06_WALLET_INVALID: {wallet} is no public key"));
    }
    let scope = flags
        .get("--digest")
        .map(|d| {
            darkbot_runner::orchestration::fe06::preview(project, d).map(|s| (d.to_string(), s))
        })
        .transpose()?;
    // A mistyped directory is no journal without attempts.
    if !Path::new(&dir).is_dir() {
        return Err(format!("FE04_JOURNAL_DIR_MISSING: {dir}"));
    }
    let records = archive::records(Path::new(&dir), &wallet)?;
    let today = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "FE06_CLOCK_UNREADABLE")?
        .as_secs()
        / 86_400;
    let shown = scope.as_ref().map(|(d, s)| (d.as_str(), s));
    for line in fe06_attempts_report(&records, &wallet, today, shown) {
        println!("{line}");
    }
    Ok(true)
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
    if kind == "h0" {
        load_capture_env()?;
    }
    let request = jobs::request_of(project, kind, args)?;
    jobs::submit(project, &request)?;
    // `job cancel` (SIGTERM) ends a foreground job as CANCELLED, as it ends a worker.
    control::install_stop_handler()?;
    let (outcome, executed) = jobs::run(project, &request.job_id, Some(control::stop_requested))?;
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
        Some(jobs::Executed::H0 { job_id, report }) => {
            print!("{}", darkbot_runner::analytics::h0::render(&report));
            println!(
                "h0 output: {}",
                darkbot_runner::analytics::h0::output_dir(project, &job_id).display()
            );
        }
        None => return Err(outcome.code),
    }
    println!("job {} {}", outcome.job_id, outcome.state);
    Ok(true)
}

const JOB_USAGE: &str = "usage: p0ctl job start backtest|optimize|post-capture|h0 [--id ID] … | job cancel --id ID | job status [--id ID]";

/// `job start|cancel|status|run` (B03, P0-PROD-CTRL-03): the analytics job controller.
fn job_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    let id = || given(args, "--id")?.ok_or_else(|| "--id ID required".to_string());
    match args.first().map(String::as_str) {
        Some("start") => {
            let kind = args.get(1).ok_or(JOB_USAGE)?;
            // H0's inputs (tip accounts, minimum profit, registry) are the operator's env files,
            // as for the capture; its worker inherits them.
            if kind == "h0" {
                load_capture_env()?;
            }
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
        // DEC-09: the pre-binary JSON import exists only in the `historical` build.
        #[cfg(feature = "historical")]
        Some("migrate") => {
            use darkbot_runner::execution::one_shot::{migrated_legacy, Journal};
            let sha = Journal::migrate_legacy(dir, &wallet)?;
            println!(
                "migrated legacy_sha256={sha} evidence={}",
                migrated_legacy(dir, &wallet).display()
            );
            Ok(true)
        }
        #[cfg(not(feature = "historical"))]
        Some("migrate") => {
            Err(darkbot_runner::analytics::renders::HISTORICAL_READER_NOT_BUILT.into())
        }
        _ => Err(USAGE.into()),
    }
}

/// FE-04 `approval show FILE`: see the module documentation. DEC-10: the one status evaluation
/// of the runtime (`approval_file::status`); this tool checks code, binary, expiry, lifetime and
/// single use (with `--journal-dir`) — the runtime also binds wallet, program, the observed
/// ProgramData and the Safety ceilings.
fn approval_cmd(project: &Path, args: &[String]) -> Result<bool, String> {
    use darkbot_runner::execution::one_shot::approval_file::{
        self, ApprovalBinding, ApprovalFile, Bind, Usage,
    };
    const USAGE: &str = "usage: p0ctl approval show FILE [--journal-dir DIR]";
    let (show, file, journal_dir) = match args {
        [show, file] => (show, file, None),
        [show, file, flag, dir] if flag == "--journal-dir" => (show, file, Some(Path::new(dir))),
        _ => return Err(USAGE.into()),
    };
    if show != "show" {
        return Err(USAGE.into());
    }
    let bytes = approval_file::read_bytes(Path::new(file))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "WALL_CLOCK_UNAVAILABLE")?
        .as_secs();
    // DEC-10: the native text; a historical JSON approval is shown only by the historical build
    // and never authorizes (it is never adopted into a native approval).
    let historical = approval_file::is_json(&bytes);
    #[cfg(feature = "historical")]
    let parsed = if historical {
        approval_file::read_historical_json(&bytes)?
    } else {
        approval_file::parse(&bytes)?
    };
    #[cfg(not(feature = "historical"))]
    let parsed = approval_file::parse(&bytes)?;
    let mut lines: Vec<(String, String)> = Vec::new();
    let mut put = |k: &str, v: String| lines.push((k.to_string(), v));
    if historical {
        put(
            "format",
            "HISTORICAL_JSON (never authorizes, DEC-10)".into(),
        );
    }
    let (wallet, code_sha, binary_sha) = match &parsed {
        ApprovalFile::Attempt(a) => {
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
            (
                a.wallet.clone(),
                a.code_sha.clone(),
                a.binary_sha256.clone(),
            )
        }
        ApprovalFile::Exit(e) => {
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
            (
                e.wallet.clone(),
                e.code_sha.clone(),
                e.binary_sha256.clone(),
            )
        }
    };
    // FE-04/DEC-10: single use is checked only against the wallet's journals.
    let usage = match journal_dir {
        None => Usage::Unchecked("SINGLE_USE_NOT_CHECKED: --journal-dir DIR".into()),
        Some(dir) => {
            if let ApprovalFile::Attempt(a) = &parsed {
                attempt_history(dir, a, &mut put);
            }
            match darkbot_runner::execution::accounting::Ledger::load(dir, &wallet) {
                Ok(ledger) => approval_file::usage(&parsed, &ledger),
                Err(e) => {
                    put("history", format!("UNREADABLE ({e})"));
                    Usage::Refused("INVALID", "FE04_JOURNAL_UNREADABLE".into())
                }
            }
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
    let lifetime =
        darkbot_solana::runtime_config::Resolved::parse(&[], &|k| std::env::var(k).ok())?
            .config
            .limits
            .fe04_approval_max_lifetime_seconds;
    let status = approval_file::status(
        Some(&Ok(parsed)),
        &ApprovalBinding {
            wallet: Bind::NotChecked,
            program: Bind::NotChecked,
            program_data_hash: Bind::NotChecked,
            code_sha: Bind::of(head),
            binary_sha256: Bind::of(binary),
            ceilings: None,
            max_lifetime_seconds: lifetime,
        },
        now,
        &usage,
    );
    let valid = status.state == "VALID" && !historical;
    put(
        "status",
        if status.reason.is_empty() {
            status.state.to_string()
        } else {
            format!("{} ({})", status.state, status.reason)
        },
    );
    put(
        "valid",
        if valid {
            "yes".into()
        } else if historical {
            "no (HISTORICAL_JSON_NEVER_AUTHORIZES)".into()
        } else {
            format!(
                "no ({})",
                if status.reason.is_empty() {
                    status.state
                } else {
                    &status.reason
                }
            )
        },
    );
    put("live", "LOCKED (FE-06: separate explicit approval)".into());
    for (k, v) in &lines {
        println!("{k}: {v}");
    }
    Ok(valid)
}

/// FE-04: an attempt approval against the wallet's journal history (display only; the single-use
/// decision is `approval_file::usage` on the same ledger): whether its attempt was archived, the
/// unfinished active attempt, the cumulative realized net and the verdict.
fn attempt_history(
    dir: &Path,
    a: &darkbot_runner::execution::one_shot::Approval,
    put: &mut dyn FnMut(&str, String),
) {
    use darkbot_runner::execution::accounting::{Ledger, Verdict};
    let archived =
        darkbot_runner::execution::one_shot::archive::is_consumed(dir, &a.wallet, &a.approval_id);
    put("approval_archived", archived.to_string());
    let Ok(ledger) = Ledger::load(dir, &a.wallet) else {
        return;
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
            (Some("capability"), _) => capability_cmd(&project, &rest),
            (Some("live"), _) => live_cmd(&project, &rest),
            (Some("boot"), _) => boot_cmd(&project, &rest),
            (Some("horizon"), _) => horizon_cmd(&project, &rest),
            (Some("autostart"), _) => autostart_cmd(&project, &rest),
            (Some("backtest"), _) => foreground_job(&project, "backtest", &rest),
            (Some("optimize"), _) => foreground_job(&project, "optimize", &rest),
            (Some("h0"), _) => foreground_job(&project, "h0", &rest),
            (Some("job"), _) => job_cmd(&project, &rest),
            (Some("profiles"), _) => profiles_cmd(&project, &rest),
            (Some("approval"), _) => approval_cmd(&project, &rest),
            (Some("fe04"), _) => fe04_cmd(&rest),
            _ => Err("usage: p0ctl [--project DIR] doctor|preflight|run preflight|run start|run status|run stop|runs list|verify|report|qualify|post-capture|source|capability|live|boot|horizon|autostart|backtest|optimize|job|profiles|approval|fe04 …".into()),
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
    /// DEC-16: the launch of an admitted unbounded run — the horizon flags become its
    /// `--duration until-stopped:W:R`, every other option and every operational argument stays.
    #[test]
    fn dec16_an_unbounded_launch_carries_exactly_its_horizon() {
        let args = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        let spec = "until-stopped:1h:1d";
        assert_eq!(
            super::until_stopped_launch(
                &args(&[
                    "--until-stopped",
                    "--checkpoint",
                    "1h",
                    "--capture-source",
                    "logs",
                    "--minimum-runway",
                    "24h",
                    "--qualification",
                    "0123456789abcdef",
                    "--",
                    "--checkpoint",
                    "x",
                ]),
                spec,
            ),
            args(&[
                "--capture-source",
                "logs",
                "--duration",
                spec,
                "--",
                "--checkpoint",
                "x"
            ])
        );
        assert_eq!(
            super::until_stopped_launch(&args(&["--until-stopped", "--checkpoint", "1h"]), spec),
            args(&["--duration", spec])
        );
    }

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
    /// reader (report and verify stay native; DEC-08: `index`/`crosscheck` too, through the
    /// native driver), and a historical run is refused with its isolated pinned reader named,
    /// never started (A09).
    #[test]
    fn native_runs_never_reach_the_pinned_python_readers() {
        let project = std::env::temp_dir().join(format!("p0ctl_route_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        let native = project.join("data/runs/n/capture");
        std::fs::create_dir_all(&native).unwrap();
        std::fs::write(native.join("run_manifest.p0j"), b"").unwrap();
        std::fs::create_dir_all(project.join("data/runs/h")).unwrap();
        let err = |r: Result<bool, String>| r.unwrap_err();
        // A09 (CTRL-02): p0ctl never starts a pinned Python reader; a historical run is named
        // with its isolated reader, with or without `--historical`.
        for args in [&["--run", "h"][..], &["--run", "h", "--historical"]] {
            let a = strings(args);
            for (r, script) in [
                (report_cmd(&project, &a), "scripts/report.sh"),
                (verify_cmd(&project, &a), "scripts/verify.sh"),
            ] {
                let e = err(r);
                assert!(
                    e.starts_with("HISTORICAL_RUN_NOT_NATIVE") && e.contains(script),
                    "{e}"
                );
            }
        }
        assert!(
            err(report_cmd(&project, &strings(&["--run", "missing"]))).starts_with("RUN_NOT_FOUND")
        );
        assert!(err(verify_cmd(&project, &[])).starts_with("RUN_REQUIRED"));
        assert!(err(report_cmd(
            &project,
            &strings(&["--run", "n", "--historical"])
        ))
        .starts_with("CAPTURE_NOT_FINALIZED"));
        assert!(err(verify_cmd(&project, &strings(&["--run", "n"])))
            .starts_with("VERIFY_MODE_REQUIRED"));
        // DEC-08: index/crosscheck reach the native driver (here: its manifest check), never
        // the pinned reader; an unknown mode is refused.
        for mode in ["index", "crosscheck"] {
            let e = err(verify_cmd(
                &project,
                &strings(&["--run", "n", "--mode", mode]),
            ));
            assert!(e.starts_with("NATIVE_MANIFEST_MISSING"), "{mode}: {e}");
        }
        assert!(err(verify_cmd(
            &project,
            &strings(&["--run", "n", "--mode", "full"])
        ))
        .starts_with("VERIFY_MODE_REFUSED"));
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
            &["--mode", "live", "--duration", "3m"],
            &["--mode"],
            &["--mode", "shadow", "--mode", "shadow"],
        ] {
            assert!(plan(bad).is_err(), "{bad:?}");
        }
        // DEC-16 (stage 3): a Shadow run runs until stopped like econ4h (admitted by run start).
        assert_eq!(
            plan(&["--mode", "shadow", "--until-stopped"]).unwrap().0,
            RunMode::Shadow
        );
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

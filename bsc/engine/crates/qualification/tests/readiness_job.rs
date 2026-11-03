//! Audit E 4.13: the GUI readiness job, run by its one native owner
//! (`bsc-status-reporter --readiness-job`, was `bsc/tools/readiness_job.py`
//! and its five tests): it keeps the validated native verdict
//! (PASS_READ_ONLY/BLOCKED/STOP), treats an exit code that contradicts the
//! record as a runner failure, hands the readiness binary exactly its
//! allowlisted environment, never starts it without a configured endpoint,
//! forwards a stop to the run's process group, and writes typed marks only.
//! Temporary fixtures; a stand-in readiness binary prints the golden record.

use darkbot_bsc_qualification::status::gui_readiness;
use darkbot_bsc_qualification::status::readiness_job::{self, Mark};
use darkbot_bsc_qualification::status::wire::bsc::JobStatus;
use darkbot_bsc_record::schema::g07::gui_readiness_provenance as p;
use prost::Message;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const REPORTER: &str = env!("CARGO_BIN_EXE_bsc-status-reporter");
const CONFIG: &str = "# public readiness configuration\nBSC_READINESS_RPC_HTTPS=https://rpc.example.invalid/\nDARKBOT_G07_CLOCK_ID=macos.CLOCK_UPTIME_RAW\n";

fn golden() -> PathBuf {
    // Status BLOCKED, mode OFFLINE_ZERO_EGRESS.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../protocol/golden/bsc/readiness.dbrec")
}

struct Fixture {
    base: PathBuf,
    root: PathBuf,
    run: PathBuf,
}

impl Fixture {
    fn new(name: &str, config: Option<&str>) -> Self {
        let base =
            std::env::temp_dir().join(format!("bsc-readiness-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (root, run) = (base.join("root"), base.join("run"));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("SNAPSHOT_SHA256"), "a".repeat(64)).unwrap();
        fs::write(root.join("COMMIT"), "fixture").unwrap();
        fs::write(root.join("CHECKOUT"), "/fixture/checkout\n").unwrap();
        if let Some(config) = config {
            fs::create_dir_all(root.join("bsc/config")).unwrap();
            fs::write(root.join("bsc/config/readiness.public.env"), config).unwrap();
        }
        Self { base, root, run }
    }

    /// A stand-in readiness binary: records its environment, prints the
    /// golden record and exits `code`.
    fn runner(&self, code: i32) -> PathBuf {
        let runner = self.base.join("g07_pre_live_readiness");
        fs::write(
            &runner,
            format!(
                "#!/bin/sh\nenv > '{}'\ncat '{}'\nexit {code}\n",
                self.base.join("runner.env").display(),
                golden().display()
            ),
        )
        .unwrap();
        fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
        runner
    }

    fn reporter(&self, runner: &Path) -> Command {
        let mut c = Command::new(REPORTER);
        c.env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("DARKBOT_BSC_ROOT", &self.root)
            .env("DARKBOT_RUNDIR", &self.run)
            .env("DARKBOT_BSC_LAUNCH_ID", "job-1")
            .env("DARKBOT_BSC_READINESS_BIN", runner)
            // Never inherited by the readiness binary.
            .env("BSC_LIVE_KEY_FILE", "/fixture/secret.key");
        c
    }

    fn job(&self, runner: &Path) -> (i32, readiness_job::Marked, bool) {
        let out = wait(
            self.reporter(runner)
                .arg("--readiness-job")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let result = readiness_job::read(&self.run, Mark::Result).expect("a result mark");
        (out.status.code().unwrap(), result, self.published())
    }

    fn published(&self) -> bool {
        let out = self
            .run
            .join("bsc-operational/bsc/native/evidence/g07/gui_readiness_job-1");
        if !out.join("readiness.dbrec").is_file() {
            return false;
        }
        // BSC-048: the provenance is the typed record the sync's import
        // gate accepts; no JSON.
        assert!(!out.join("provenance.json").exists());
        let r = gui_readiness::check(&out).unwrap();
        assert_eq!(r.text(p::LAUNCH_ID).unwrap(), "job-1");
        true
    }

    fn seen(&self) -> BTreeMap<String, String> {
        fs::read_to_string(self.base.join("runner.env"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn wait(mut child: Child) -> Output {
    let started = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if started.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            panic!("the job did not end");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    child.wait_with_output().unwrap()
}

#[test]
fn a_native_blocked_verdict_stays_blocked() {
    let f = Fixture::new("blocked", Some(CONFIG));
    let (code, result, published) = f.job(&f.runner(2));
    assert_eq!(
        (
            result.result_code.as_str(),
            result.exit_code,
            published,
            code
        ),
        ("BLOCKED", Some(2), true, 2)
    );
    assert_eq!(result.launch_id, "job-1");
    // The marks of the run are the job's typed records, none left over.
    assert_eq!(
        readiness_job::read(&f.run, Mark::Armed).unwrap().launch_id,
        "job-1"
    );
    let left: Vec<_> = fs::read_dir(&f.run)
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

#[test]
fn an_exit_code_contradicting_the_record_is_a_runner_failure() {
    let f = Fixture::new("contradicting", Some(CONFIG));
    let (code, result, published) = f.job(&f.runner(0));
    assert_eq!(
        (result.result_code.as_str(), published, code),
        ("FAILED", false, 2)
    );
}

#[test]
fn the_job_takes_rpc_and_clock_from_the_public_config_and_the_boot_id_from_the_os() {
    // BSC-050 A-scan: no hardcoded endpoint; the snapshot's checkout for
    // latch/authority; nothing else of the job's environment.
    let f = Fixture::new("env", Some(CONFIG));
    let (_, result, _) = f.job(&f.runner(2));
    assert_eq!(result.result_code, "BLOCKED");
    let seen = f.seen();
    assert_eq!(
        seen["BSC_MAINNET_RPC_HTTPS"],
        "https://rpc.example.invalid/"
    );
    assert_eq!(seen["DARKBOT_G07_CLOCK_ID"], "macos.CLOCK_UPTIME_RAW");
    // The host's boot identity as the native owner reads it.
    assert_eq!(
        seen["DARKBOT_G07_BOOT_ID"],
        darkbot_bsc_execution::sources::boot_session().unwrap()
    );
    assert_eq!(seen["DARKBOT_BSC_CHECKOUT"], "/fixture/checkout");
    assert_eq!(seen["PATH"], "/usr/bin:/bin:/usr/sbin:/sbin");
    for absent in [
        "BSC_LIVE_KEY_FILE",
        "DARKBOT_BSC_ROOT",
        "DARKBOT_RUNDIR",
        "DARKBOT_BSC_LAUNCH_ID",
    ] {
        assert!(
            !seen.contains_key(absent),
            "{absent} reached the readiness binary"
        );
    }
}

#[test]
fn without_a_configured_endpoint_the_readiness_binary_never_starts() {
    for (n, config) in [
        None,
        Some("DARKBOT_G07_CLOCK_ID=x\n"),
        Some("BSC_READINESS_RPC_HTTPS=http://plain.invalid/\n"),
        Some("BSC_READINESS_RPC_HTTPS=https://ok.invalid/\nOTHER=1\n"),
        Some("BSC_READINESS_RPC_HTTPS=https://ok.invalid/\nBSC_READINESS_RPC_HTTPS=https://two.invalid/\n"),
        Some("BSC_READINESS_RPC_HTTPS=https://ok.invalid/ x\n"),
    ]
    .into_iter()
    .enumerate()
    {
        let f = Fixture::new(&format!("config-{n}"), config);
        let (code, result, published) = f.job(&f.runner(0));
        assert_eq!(
            (result.result_code.as_str(), published, code, f.seen().is_empty()),
            ("FAILED", false, 2, true),
            "{config:?}"
        );
    }
}

#[test]
fn the_real_readiness_binary_end_to_end() {
    // The real native binary (execution's, the same workspace build) in an
    // empty snapshot: no v5 contract, a typed BLOCKED.
    let real = Path::new(REPORTER).with_file_name("g07_pre_live_readiness");
    assert!(
        real.is_file(),
        "{} missing: build execution with g07-mainnet (the workspace regression does)",
        real.display()
    );
    let f = Fixture::new("real", Some(CONFIG));
    let (code, result, published) = f.job(&real);
    assert_eq!(
        (
            result.result_code.as_str(),
            result.exit_code,
            published,
            code
        ),
        ("BLOCKED", Some(2), true, 2)
    );
}

/// A stop once the job is armed ends it STOPPED with a result, the run's
/// whole process group with it; nothing is published.
#[test]
fn a_stop_request_ends_the_run_and_its_process_group() {
    let f = Fixture::new("stop", Some(CONFIG));
    let runner = f.base.join("g07_pre_live_readiness");
    let pidfile = f.base.join("sleeper.pid");
    fs::write(
        &runner,
        format!(
            "#!/bin/sh\nsleep 30 &\necho $! > '{}'\nwait\n",
            pidfile.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
    let child = f
        .reporter(&runner)
        .arg("--readiness-job")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !pidfile.is_file() || fs::read_to_string(&pidfile).unwrap().trim().is_empty() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the run never started"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        readiness_job::read(&f.run, Mark::Armed).unwrap().launch_id,
        "job-1"
    );
    let sleeper = fs::read_to_string(&pidfile).unwrap().trim().to_owned();
    assert!(
        Command::new("/bin/kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let stopped = Instant::now();
    let out = wait(child);
    // Forwarded: the run ends now, not when its 30-s child would.
    assert!(stopped.elapsed() < Duration::from_secs(10), "{:?}", stopped.elapsed());
    let result = readiness_job::read(&f.run, Mark::Result).unwrap();
    assert_eq!(
        (
            out.status.code(),
            result.result_code.as_str(),
            f.published()
        ),
        (Some(0), "STOPPED", false)
    );
    let alive = Command::new("/bin/kill")
        .args(["-0", &sleeper])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success();
    assert!(!alive, "the run's process group outlived the stop");
}

/// The controller's marks and the job's state go through the owner only.
#[test]
fn the_controller_marks_and_reads_the_job_through_its_owner() {
    let f = Fixture::new("cli", Some(CONFIG));
    let runner = f.runner(2);
    let cli = |args: &[&str]| f.reporter(&runner).args(args).output().unwrap();
    // The controller's run directory exists (`darkbotctl`).
    fs::create_dir_all(&f.run).unwrap();
    assert!(
        cli(&["--readiness-mark", "launch", "job-1"])
            .status
            .success()
    );
    assert_eq!(
        String::from_utf8(cli(&["--readiness-launch"]).stdout).unwrap(),
        "job-1\n"
    );
    // Not armed yet: 1; a mark of another kind or an unsafe launch ID is refused.
    assert_eq!(cli(&["--readiness-armed", "job-1"]).status.code(), Some(1));
    assert!(
        !cli(&["--readiness-mark", "result", "job-1"])
            .status
            .success()
    );
    assert!(
        !cli(&["--readiness-mark", "stopping", "../job"])
            .status
            .success()
    );
    assert!(
        cli(&["--readiness-mark", "stopping", "job-1"])
            .status
            .success()
    );
    let status = |cli: &dyn Fn(&[&str]) -> Output| {
        JobStatus::decode(
            cli(&[
                "--readiness-status",
                "com.darkbot.test.absent-readiness-label",
            ])
            .stdout
            .as_slice(),
        )
        .unwrap()
    };
    let s = status(&cli);
    assert_eq!(
        (
            s.launch_id.as_str(),
            s.started_unix_s > 0,
            s.controller_available
        ),
        ("job-1", true, Some(true))
    );
    // launchd's view (macOS): the label is gone, the launch has no result.
    if cfg!(target_os = "macos") {
        assert_eq!(s.state, "FAILED");
    }
    let (code, result, _) = f.job(&runner);
    assert_eq!((code, result.result_code.as_str()), (2, "BLOCKED"));
    assert_eq!(cli(&["--readiness-armed", "job-1"]).status.code(), Some(0));
    assert_eq!(cli(&["--readiness-armed", "job-2"]).status.code(), Some(1));
    assert_eq!(
        String::from_utf8(cli(&["--readiness-result"]).stdout).unwrap(),
        "BLOCKED\n"
    );
    let s = status(&cli);
    assert_eq!(s.result_code, "BLOCKED");
    if cfg!(target_os = "macos") {
        assert_eq!(s.state, "STOPPED");
    }
}

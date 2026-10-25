//! B03/U-9: `p0ctl job …` over the real binary — `job start` records the typed request and
//! returns while a detached worker (`p0ctl job run`) executes it and records the outcome;
//! `job status` shows the store, `job cancel` refuses a job that is not running, an id is never
//! started twice. Offline: the run has no evidence, so the job fails closed with its own code.
use std::process::Command;
use std::time::{Duration, Instant};

fn p0ctl(project: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_p0ctl"))
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .arg("--project")
        .arg(project)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const START: &[&str] = &[
    "job",
    "start",
    "backtest",
    "--run",
    "run-a",
    "--id",
    "job-1",
    "--capital-usd",
    "1000",
    "--min-profit-usd",
    "0",
    "--max-seconds",
    "5",
    "--max-ram-bytes",
    "1048576",
];

#[test]
fn a_started_job_runs_in_its_detached_worker_and_records_the_outcome() {
    let project = std::env::temp_dir().join(format!("p0ctl_jobs_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(project.join("data/econ4h/run-a/capture")).unwrap();
    let (code, out, err) = p0ctl(&project, START);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("job job-1 started (BACKTEST)"), "{out}");
    let deadline = Instant::now() + Duration::from_secs(30);
    let line = loop {
        let (_, out, _) = p0ctl(&project, &["job", "status", "--id", "job-1"]);
        if out.contains(" FAILED ") || out.contains(" COMPLETED ") {
            break out;
        }
        assert!(Instant::now() < deadline, "no outcome: {out}");
        std::thread::sleep(Duration::from_millis(50));
    };
    // No capture binding in the run: the worker refuses with the backtest's code.
    assert!(line.starts_with("job-1 BACKTEST FAILED run=run-a "), "{line}");
    let log = std::fs::read_to_string(project.join("data/analytics-jobs/job-1/worker.log"));
    assert!(log.unwrap().contains("job job-1 FAILED"));
    let (code, _, err) = p0ctl(&project, &["job", "cancel", "--id", "job-1"]);
    assert_ne!(code, 0);
    assert!(err.contains("JOB_NOT_RUNNING: FAILED"), "{err}");
    let (code, _, err) = p0ctl(&project, START);
    assert_ne!(code, 0);
    assert!(err.contains("JOB_EXISTS"), "{err}");
    let (code, out, _) = p0ctl(&project, &["job", "status"]);
    assert_eq!((code, out.lines().count()), (0, 1), "{out}");
    let _ = std::fs::remove_dir_all(&project);
}

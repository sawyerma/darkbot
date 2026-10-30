//! H0 (review M2/M3): `p0ctl job start h0` over the real binary resolves its request from the
//! operator env files (`P0_ENV_FILE`, as the GUI's controller passes them) through the registry:
//! the configured window is the request's; without them the registry's default. Offline only.
use darkbot_runner::analytics::jobs;
use std::process::Command;
use std::time::{Duration, Instant};

fn start(project: &std::path::Path, id: &str, env_file: Option<&std::path::Path>) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_p0ctl"));
    c.env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .arg("--project")
        .arg(project)
        .args([
            "job",
            "start",
            "h0",
            "--id",
            id,
            "--max-seconds",
            "30",
            "--max-ram-bytes",
            "1048576",
        ]);
    if let Some(f) = env_file {
        c.env("P0_ENV_FILE", f);
    }
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The job's request and, once its worker is done, its outcome.
fn finished(project: &std::path::Path, id: &str) -> jobs::JobStatus {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let s = jobs::status(project, id).unwrap();
        if matches!(s.state, jobs::JobState::Done(_)) || Instant::now() > deadline {
            return s;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn h0_job_start_reads_the_operator_env_files() {
    let d = std::env::temp_dir().join(format!("p0ctl_h0_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("data")).unwrap();
    let env_file = d.join("operator.env");
    std::fs::write(&env_file, "P0_H0_WINDOW_DAYS=45\n").unwrap();
    // One analytics job at a time: each start waits for the previous worker.
    let run = |id: &str, file: Option<&std::path::Path>| {
        start(&d, id, file);
        let s = finished(&d, id);
        match &s.state {
            jobs::JobState::Done(o) => assert_eq!(o.state, jobs::COMPLETED, "{o:?}"),
            other => panic!("{other:?}"),
        }
        match s.request.spec {
            jobs::JobSpec::H0 { window_days, .. } => window_days,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(run("with-env", Some(&env_file)), 45);
    assert_eq!(
        run("without-env", None),
        darkbot_solana::runtime_config::Limits::default().h0_window_days
    );
    let _ = std::fs::remove_dir_all(&d);
}

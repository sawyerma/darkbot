//! DEC-16: `p0ctl horizon` and `run start --until-stopped` over the real binary, offline only:
//! a qualification records every part and is stored by id; an unbounded start is admitted only
//! by `RunHorizon::admit` with a complete qualification bound to exactly the run — without one,
//! with an open part or another horizon it is refused before any start step. No network read.
use std::process::Command;

struct Ctl {
    bin: std::path::PathBuf,
    project: std::path::PathBuf,
}
impl Ctl {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("p0ctl_horizon_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (bin, project) = (root.join("bin"), root.join("project"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(project.join("data")).unwrap();
        // The binding names p0ctl and the runtime beside it: a copied p0ctl with a stand-in
        // runtime keeps the test hermetic (the runtimes are built by the reporter crate).
        std::fs::copy(env!("CARGO_BIN_EXE_p0ctl"), bin.join("p0ctl")).unwrap();
        std::fs::write(bin.join("p0-longtail"), b"runtime stand-in").unwrap();
        Self { bin, project }
    }
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(self.bin.join("p0ctl"))
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .arg("--project")
            .arg(&self.project)
            .args(args)
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }
}
impl Drop for Ctl {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.bin.parent().unwrap());
    }
}

#[test]
fn dec16_an_unbounded_run_is_admitted_only_with_its_complete_qualification() {
    let ctl = Ctl::new("admit");
    let qualify = |extra: &[&str]| {
        let mut args = vec![
            "horizon",
            "qualify",
            "--mode",
            "econ4h",
            "--checkpoint",
            "1h",
            "--minimum-runway",
            "24h",
        ];
        args.extend(extra);
        ctl.run(&args)
    };
    let (code, out, err) = qualify(&[]);
    assert_eq!(code, 2, "{err}");
    for part in [
        "runtime            NOT_RUN",
        "sk07               PASS",
        "capacity           NOT_RUN",
        "provider_evidence  NOT_RUN",
    ] {
        assert!(out.contains(part), "{part}\n{out}");
    }
    let line = out
        .lines()
        .find(|l| l.starts_with("UNTIL_STOPPED_QUALIFICATION "))
        .unwrap();
    let id = line.split_whitespace().nth(1).unwrap().to_string();
    assert!(line.contains("RUN_HORIZON_UNTIL_STOPPED_GATED"), "{line}");
    assert!(ctl
        .project
        .join(format!("data/until_stopped/{id}.p0j"))
        .is_file());
    let (code, out, _) = ctl.run(&["horizon", "status", "--qualification", &id]);
    assert_eq!(code, 0);
    assert!(out.contains("sk07               PASS"), "{out}");
    // An evidence id never imported is refused before anything is stored.
    let (code, _, err) = qualify(&["--provider-evidence", "0123456789abcdef"]);
    assert_eq!(code, 2);
    assert!(err.contains("BOOT_PROVIDER_EVIDENCE_UNKNOWN"), "{err}");

    let start = |extra: &[&str]| {
        let mut args = vec![
            "run",
            "start",
            "--until-stopped",
            "--checkpoint",
            "1h",
            "--minimum-runway",
            "24h",
        ];
        args.extend(extra);
        ctl.run(&args)
    };
    let gated = |extra: &[&str], want: &str| {
        let (code, _, err) = start(extra);
        assert_eq!(code, 2);
        assert!(
            err.contains("RUN_HORIZON_UNTIL_STOPPED_GATED") && err.contains(want),
            "{want}\n{err}"
        );
    };
    gated(&[], "no qualification");
    gated(&["--qualification", &id], "runtime NOT_RUN");
    gated(&["--qualification", &id], "capacity NOT_RUN");
    // Another window is another horizon: the qualification does not bind it.
    let (code, _, err) = ctl.run(&[
        "run",
        "start",
        "--until-stopped",
        "--checkpoint",
        "2h",
        "--minimum-runway",
        "24h",
        "--qualification",
        &id,
    ]);
    assert_eq!(code, 2);
    assert!(err.contains("drift: runtime_config,horizon"), "{err}");
    // Refused before any start step: no owner lock, no run directory (the qualification's
    // probe ran on the run volume and left nothing).
    let runs = ctl.project.join("data/runs");
    assert!(!runs.exists() || std::fs::read_dir(&runs).unwrap().next().is_none());
    let (code, _, err) = start(&["--duration", "1h"]);
    assert_eq!(code, 2);
    assert!(err.contains("exclusive"), "{err}");
    let (code, _, err) = ctl.run(&[
        "run",
        "start",
        "--mode",
        "shadow",
        "--until-stopped",
        "--checkpoint",
        "1h",
        "--minimum-runway",
        "24h",
    ]);
    assert_eq!(code, 2);
    assert!(err.contains("RUN_MODE_OPTION_REFUSED"), "{err}");
}

/// DEC-16: an admitted unbounded econ4h start (a complete qualification of exactly this run)
/// launches its horizon instead of stopping before the launcher: it goes on to the start's own
/// gates — the offline audit and the network audit, none given here — exactly as a fixed run.
/// Strict verification and a bare unbounded `--duration` are refused.
#[test]
fn dec16_an_admitted_unbounded_econ4h_start_goes_on_to_its_start_gates() {
    use darkbot_runner::orchestration::until_stopped as us;
    let ctl = Ctl::new("launch");
    let (_, out, err) = ctl.run(&[
        "horizon",
        "qualify",
        "--mode",
        "econ4h",
        "--checkpoint",
        "1h",
        "--minimum-runway",
        "24h",
    ]);
    let open = out
        .lines()
        .find_map(|l| l.strip_prefix("UNTIL_STOPPED_QUALIFICATION "))
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or_else(|| panic!("{out}{err}"))
        .to_string();
    // The fixture completes the open parts (the gating under test, never a production path).
    let mut q = us::get(&ctl.project, &open).unwrap();
    let pass = us::Part {
        state: us::PartState::Pass,
        reason: "fixture".into(),
    };
    (
        q.runtime,
        q.capacity,
        q.provider_budget,
        q.provider_evidence,
    ) = (pass.clone(), pass.clone(), pass.clone(), pass);
    q.evidence_id = Some("0123456789abcdef".into());
    q.evidence_valid_until_unix_s = Some(u64::MAX / 2);
    let id = us::put(&ctl.project, &q).unwrap();
    let start = |extra: &[&str]| {
        let mut args = vec![
            "run",
            "start",
            "--until-stopped",
            "--checkpoint",
            "1h",
            "--minimum-runway",
            "24h",
            "--qualification",
            &id,
        ];
        args.extend(extra);
        ctl.run(&args)
    };
    let (code, out, err) = start(&[]);
    assert_eq!(code, 2);
    assert!(
        !err.contains("UNTIL_STOPPED_LAUNCHER") && !err.contains("RUN_HORIZON_UNTIL_STOPPED_GATED"),
        "{err}"
    );
    assert!(
        err.contains("BOOT_NETWORK_AUDIT_REQUIRED") || err.contains("BOOT_AUDIT_BLOCKED"),
        "{out}{err}"
    );
    let (code, _, err) = start(&["--chain-verify", "strict"]);
    assert_eq!(code, 2);
    assert!(err.contains("UNTIL_STOPPED_STRICT_NOT_AVAILABLE"), "{err}");
    let (code, _, err) = ctl.run(&["run", "start", "--duration", "until-stopped:1h:24h"]);
    assert_eq!(code, 2);
    assert!(err.contains("RUN_HORIZON_UNTIL_STOPPED:"), "{err}");
}

/// DEC-16: the plan compiler of an unbounded run compiles the plan of its window and names its
/// runway as the capacity horizon; a fixed run's capacity horizon is its duration.
#[test]
fn dec16_the_plan_compiler_plans_the_window_and_names_the_runway() {
    let dir = std::env::temp_dir().join(format!("p0_plan_unbounded_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let plan = |duration: &str, args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_p0-measurement-plan"))
            .env_clear()
            .env("P0_MEASUREMENT_DURATION", duration)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    let file = dir.join("plan.p0j");
    let path = file.to_string_lossy().to_string();
    assert_eq!(plan("until-stopped:1h:24h", &["--out", &path]), "3600");
    assert_eq!(plan("until-stopped:1h:24h", &["--runway"]), "86400");
    assert_eq!(plan("4h", &["--runway"]), "14400");
    let stored: darkbot_runner::analytics::measurement::MeasurementPlan =
        darkbot_runner::analytics::journal::read_typed_value(&file).unwrap();
    assert_eq!(stored.duration_seconds, 3600);
    let _ = std::fs::remove_dir_all(dir);
}

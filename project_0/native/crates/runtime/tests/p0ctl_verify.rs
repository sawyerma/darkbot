//! C0E1-P5-CLI: `p0ctl verify` is one contract over the real binary — a run with native records
//! verifies `off` and `strict` through the typed native driver (stable spec, attempts, resume),
//! and an online strict verification without provider evidence is refused before any chain RPC.
//! Offline only: the child runs with an empty environment (no RPC endpoint, no network).
use darkbot_runner::analytics::measurement::{
    journal, persist_manifest, MeasurementPlan, Phase, NS,
};
use darkbot_runner::run_layout::{RunLayout, RunPaths};
use std::process::Command;

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

#[test]
fn p0ctl_verify_routes_native_runs_through_the_typed_driver() {
    let d = std::env::temp_dir().join(format!("p0ctl_verify_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let root = d.join("run_v");
    let paths = RunPaths::new(&root, RunLayout::V2);
    paths.create().unwrap();
    let plan = MeasurementPlan::new(60).unwrap();
    let clock = darkbot_common::timing::Clock::new();
    persist_manifest(&paths, "run_v", "test", &plan, clock).unwrap();
    let window = plan.capture_window(clock, 1_000 * NS).unwrap();
    journal(&paths, clock, Phase::Capture(window)).unwrap();
    let run = root.to_str().unwrap();

    // Off: NOT_RUN with identities, no chain read; a second attempt needs resume.
    let (code, out, err) = p0ctl(&d, &["verify", "--run", run, "--mode", "off"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("status=NOT_RUN reason=VERIFICATION_OFF"),
        "{out}"
    );
    let (code, _, err) = p0ctl(&d, &["verify", "--run", run, "--mode", "off"]);
    assert_eq!(code, 2);
    assert!(err.contains("SPEC_EXISTS_USE_RESUME_VERIFICATION"), "{err}");
    let (code, out, _) = p0ctl(
        &d,
        &[
            "verify",
            "--run",
            run,
            "--mode",
            "off",
            "--resume-verification",
        ],
    );
    assert_eq!(code, 0, "{out}");

    // Strict offline without cached chain data: incomplete (exit 2), resumable evidence kept.
    let (code, out, err) = p0ctl(
        &d,
        &["verify", "--run", run, "--mode", "strict", "--offline"],
    );
    assert_eq!(code, 2, "{out}{err}");
    assert!(out.contains("status=HISTORICAL_INCOMPLETE"), "{out}");

    // Online strict without measured provider evidence: refused before any chain RPC.
    let (code, _, err) = p0ctl(
        &d,
        &[
            "verify",
            "--run",
            run,
            "--mode",
            "strict",
            "--resume-verification",
        ],
    );
    assert_eq!(code, 2);
    assert!(err.contains("STRICT_PROVIDER_EVIDENCE_REQUIRED"), "{err}");
    std::fs::remove_dir_all(&d).unwrap();
}

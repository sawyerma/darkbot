//! P0-026: every directory the runtime keeps its state in under a project is ignored by git. The
//! boot audit binds `git status --porcelain` of the project (BOOT-01 source identity), so state a
//! boot flow, a control request, an evidence import or a job writes never changes the identity
//! it was audited under.
use darkbot_runner::analytics::{backtest, h0, jobs};
use darkbot_runner::execution::fee_parity;
use darkbot_runner::orchestration::{
    boot_control, c01_records, capability_control, control, fe06, provider_evidence,
    provider_measurement, provider_registry, source_control, until_stopped,
};
use std::path::{Path, PathBuf};

fn ignored(project: &Path, path: &Path) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["check-ignore", "-q", "--no-index", "--"])
        .arg(path)
        .status()
        .expect("git")
        .success()
}

#[test]
fn p0_026_every_runtime_state_directory_of_a_project_is_ignored() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let root = Path::new("");
    let dirs: Vec<PathBuf> = vec![
        boot_control::BOOT_DIR.into(),
        provider_evidence::STORE.into(),
        provider_registry::STORE.into(),
        provider_measurement::STORE.into(),
        c01_records::STORE.into(),
        until_stopped::STORE.into(),
        fe06::STORE.into(),
        jobs::JOB_DIR.into(),
        backtest::PROFILE_DIR.into(),
        source_control::request_dir(root),
        capability_control::request_dir(root),
        fee_parity::seal_dir(root),
        h0::output_dir(root, "job"),
    ];
    let mut kept: Vec<String> = dirs
        .iter()
        .map(|d| d.join("state.p0j"))
        .chain([control::lock_path(root)])
        .filter(|p| !ignored(&project, p))
        .map(|p| p.display().to_string())
        .collect();
    kept.sort();
    assert!(kept.is_empty(), "runtime state git would see: {kept:?}");
}

/// P0-026: the audited source identity of a project (BOOT-01) stays the same while the runtime
/// writes its state: a git project carrying the real `project_0/.gitignore`, its identity before
/// and after a file in every runtime state directory and the runtime lock.
#[test]
fn p0_026_the_audited_identity_is_stable_over_the_runtime_state() {
    use darkbot_runner::orchestration::boot_audit::{identity_with, SafetyFlags};
    let real = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let d = std::env::temp_dir().join(format!("p0_026_identity_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&d)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q"]);
    std::fs::copy(real.join(".gitignore"), d.join(".gitignore")).unwrap();
    git(&["add", ".gitignore"]);
    git(&["commit", "-q", "-m", "t"]);
    let flags = SafetyFlags {
        live_enabled: false,
        submission_enabled: false,
    };
    let config = darkbot_solana::runtime_config::RuntimeConfig::default();
    let before = identity_with(&d, flags, &config).unwrap();
    assert!(before.source_dirty_sha256.is_some());
    let root = Path::new("");
    for dir in [
        PathBuf::from(boot_control::BOOT_DIR).join("flow-1"),
        provider_evidence::STORE.into(),
        provider_registry::STORE.into(),
        provider_measurement::STORE.into(),
        c01_records::STORE.into(),
        until_stopped::STORE.into(),
        fe06::STORE.into(),
        jobs::JOB_DIR.into(),
        backtest::PROFILE_DIR.into(),
        source_control::request_dir(root),
        capability_control::request_dir(root),
        fee_parity::seal_dir(root),
        h0::output_dir(root, "job"),
    ] {
        std::fs::create_dir_all(d.join(&dir)).unwrap();
        std::fs::write(d.join(&dir).join("state.p0j"), b"state").unwrap();
    }
    std::fs::write(d.join(control::lock_path(root)), b"lock").unwrap();
    assert_eq!(identity_with(&d, flags, &config).unwrap(), before);
    // A real source change does change it (the check is not vacuous).
    std::fs::write(d.join("source.rs"), b"fn f() {}").unwrap();
    assert_ne!(identity_with(&d, flags, &config).unwrap(), before);
    let _ = std::fs::remove_dir_all(d);
}

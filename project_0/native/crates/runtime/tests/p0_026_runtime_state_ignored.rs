//! P0-026: every directory the runtime keeps its state in under a project is ignored by git. The
//! boot audit binds `git status --porcelain` of the project (BOOT-01 source identity), so state a
//! boot flow, a control request, an evidence import or a job writes never changes the identity
//! it was audited under.
use darkbot_runner::analytics::{backtest, h0, jobs};
use darkbot_runner::execution::fee_parity;
use darkbot_runner::orchestration::{
    boot_control, capability_control, control, fe06, provider_evidence, provider_registry,
    source_control, until_stopped,
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

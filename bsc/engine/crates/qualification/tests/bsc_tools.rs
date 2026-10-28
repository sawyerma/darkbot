//! The two `bsc/tools` tests that need the status reporter, in the cargo
//! regression with their fixed test counts (the other five run in
//! execution's `bsc_tools`). Both get exactly this build's reporter; the
//! readiness binary is execution's: its `bsc_tools` checks in the same
//! workspace run that this path is that build's `CARGO_BIN_EXE_*` (cargo
//! test writes no dep-info to judge it by here); this test needs it present.

#[path = "../../execution/tests/support/bsc_tools.rs"]
mod bsc_tools;

use bsc_tools::{repo, run, same_binary};

const REPORTER: &str = env!("CARGO_BIN_EXE_bsc-status-reporter");

#[test]
fn test_readiness_job_py() {
    same_binary("bsc-status-reporter", REPORTER);
    let readiness = repo().join("bsc/engine/target/debug/g07_pre_live_readiness");
    assert!(
        readiness.is_file(),
        "{} missing: build execution with g07-mainnet (the workspace regression does)",
        readiness.display()
    );
    run(
        "test_readiness_job.py",
        5,
        &[("DARKBOT_TEST_REPORTER", REPORTER)],
    );
}

#[test]
fn test_status_sync_py() {
    same_binary("bsc-status-reporter", REPORTER);
    run(
        "test_status_sync.py",
        4,
        &[("DARKBOT_TEST_REPORTER", REPORTER)],
    );
}

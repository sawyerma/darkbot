//! The two `bsc/tools` tests that need the status reporter, in the cargo
//! regression with their fixed test counts (the other five run in
//! execution's `bsc_tools`). Both get exactly this build's reporter; the
//! readiness binary (execution) must be at least as new as its sources.

#[path = "../../execution/tests/support/bsc_tools.rs"]
mod bsc_tools;

use bsc_tools::{current, run, same_binary};

const REPORTER: &str = env!("CARGO_BIN_EXE_bsc-status-reporter");

#[test]
fn test_readiness_job_py() {
    same_binary("bsc-status-reporter", REPORTER);
    current(
        "g07_pre_live_readiness",
        &[
            "execution",
            "trading",
            "journal",
            "record",
            "tx_encoding",
            "geth_ipc",
        ],
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

//! The `bsc/tools` tests in the cargo regression (BSC-002 follow-up 28.10.:
//! `test_g06a_state_pin.cjs` was red from `86de25c9` because no gate ran
//! them). The five that need no status reporter, each with its fixed test
//! count; the two others run in qualification's `bsc_tools`. The tools call
//! the owner binaries by their fixed debug path: those must be this build's.

#[path = "support/bsc_tools.rs"]
mod bsc_tools;

use bsc_tools::{run, same_binary};

fn owner_binaries() {
    same_binary("g06a_dry_handoff", env!("CARGO_BIN_EXE_g06a_dry_handoff"));
    same_binary("bsc_runtime", env!("CARGO_BIN_EXE_bsc_runtime"));
    same_binary(
        "g07_pre_live_readiness",
        env!("CARGO_BIN_EXE_g07_pre_live_readiness"),
    );
}

#[test]
fn test_g06a_dry_modes_py() {
    owner_binaries();
    run("test_g06a_dry_modes.py", 18, &[]);
}

#[test]
fn test_g06a_shell_scope_py() {
    run("test_g06a_shell_scope.py", 7, &[]);
}

#[test]
fn test_g06a_key_hygiene_cjs() {
    owner_binaries();
    run("test_g06a_key_hygiene.cjs", 3, &[]);
}

#[test]
fn test_g06a_private_mode_cjs() {
    run("test_g06a_private_mode.cjs", 4, &[]);
}

#[test]
fn test_g06a_state_pin_cjs() {
    owner_binaries();
    run("test_g06a_state_pin.cjs", 9, &[]);
}

/// The runner counts every outcome: a failure, an error (Node: cancelled),
/// a skip or todo and a missing summary are all seen, so none passes.
#[test]
fn the_runner_sees_every_failure_error_skip_and_missing_summary() {
    let t = |s: &str| bsc_tools::tap(s).map(|o| (o.tests, o.pass, o.fail, o.error, o.skip));
    let tap = |pass, fail, cancelled, skipped, todo| {
        format!(
            "# tests 3\n# suites 0\n# pass {pass}\n# fail {fail}\n# cancelled {cancelled}\n# skipped {skipped}\n# todo {todo}\n"
        )
    };
    assert_eq!(t(&tap(3, 0, 0, 0, 0)), Some((3, 3, 0, 0, 0)));
    assert_eq!(t(&tap(2, 1, 0, 0, 0)), Some((3, 2, 1, 0, 0)));
    assert_eq!(t(&tap(2, 0, 1, 0, 0)), Some((3, 2, 0, 1, 0)));
    assert_eq!(t(&tap(2, 0, 0, 1, 0)), Some((3, 2, 0, 0, 1)));
    assert_eq!(t(&tap(2, 0, 0, 0, 1)), Some((3, 2, 0, 0, 1)));
    // A subtest's indented line is not the file's summary.
    assert_eq!(t("    # tests 9\n    # pass 9\n"), None);
    let u = |s: &str| bsc_tools::unittest(s).map(|o| (o.tests, o.pass, o.fail, o.error, o.skip));
    assert_eq!(
        u("....\n----\nRan 4 tests in 0.1s\n\nOK\n"),
        Some((4, 4, 0, 0, 0))
    );
    assert_eq!(
        u("Ran 5 tests in 1s\n\nOK (skipped=2)\n"),
        Some((5, 3, 0, 0, 2))
    );
    assert_eq!(
        u("Ran 5 tests in 1s\n\nFAILED (failures=1, errors=2, skipped=1)\n"),
        Some((5, 1, 1, 2, 1))
    );
    assert_eq!(
        u("Ran 2 tests in 1s\n\nOK (expected failures=1)\n"),
        Some((2, 1, 0, 0, 1))
    );
    assert_eq!(u("ImportError: no module\n"), None);
}

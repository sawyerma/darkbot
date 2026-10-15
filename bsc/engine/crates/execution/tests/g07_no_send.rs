//! Offline no-send tests of the G0.7 Mainnet self-launch + BUY binary.
//!
//! They run the built binary without the explicit send authorization and
//! without inputs: it must refuse before reading any input, touching the
//! network or opening a builder connection. A source-order check pins that
//! the compiled-in acceptance guard is evaluated, and refusal is terminal,
//! before the first builder TLS connection is opened.

use std::process::Command;

const G07: &str = env!("CARGO_BIN_EXE_g07_mainnet_self_launch_buy");
const SOURCE: &str = include_str!("../src/bin/g07_mainnet_self_launch_buy/main.rs");

fn run(env: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(G07);
    cmd.env_clear();
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

#[test]
fn refuses_without_explicit_send_authorization() {
    let out = run(&[]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "DARKBOT_G07_MAINNET_SEND=YES only for the explicitly authorized G0.7 attempt"
        ),
        "{err}"
    );
}

#[test]
fn any_other_authorization_value_is_refused() {
    let out = run(&[("DARKBOT_G07_MAINNET_SEND", "yes")]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn refuses_without_frozen_dry_evidence() {
    let out = run(&[("DARKBOT_G07_MAINNET_SEND", "YES")]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("DARKBOT_G07_DRY_EVIDENCE is required"),
        "{err}"
    );
}

#[test]
fn guard_refusal_is_terminal_before_any_builder_connection() {
    let guard = SOURCE
        .find("let contract = match verify_live_authorization(")
        .expect("guard call");
    let refused = SOURCE
        .find("panic!(\"G0.7 live authorization refused: {e}\")")
        .expect("terminal refusal");
    let first_tls = SOURCE
        .find("match connect_tls(")
        .expect("builder connection");
    assert!(guard < refused && refused < first_tls);
    assert_eq!(SOURCE.matches("verify_live_authorization(").count(), 1);
}

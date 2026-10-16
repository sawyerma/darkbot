//! Offline tests of the G0.7 builder probe. The dry run needs no input and
//! no network and always presents the same plan; a live run refuses before
//! any network access unless the approved plan SHA-256 matches. Source-order
//! checks pin that the approval precedes every read, latch and connection,
//! and that the probe never touches the G0.7 wallets or their keys.

use std::process::Command;

const PROBE: &str = env!("CARGO_BIN_EXE_g07_builder_probe");
const MAIN: &str = include_str!("../src/bin/g07_builder_probe/main.rs");
const PLAN: &str = include_str!("../src/bin/g07_builder_probe/plan.rs");

fn run(env: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(PROBE);
    cmd.env_clear();
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

fn plan_sha(stdout: &str) -> String {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix("PLAN_SHA256="))
        .expect("plan sha")
        .to_string()
}

#[test]
fn the_dry_run_needs_nothing_sends_nothing_and_is_deterministic() {
    let first = run(&[]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let stdout = String::from_utf8_lossy(&first.stdout).to_string();
    let sha = plan_sha(&stdout);
    assert_eq!(sha.len(), 64);
    assert!(sha.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(stdout.contains("DRY RUN: nothing was sent and no connection was opened"));
    assert_eq!(stdout.matches("\ncase").count(), 7);
    let second = run(&[("DARKBOT_G07_PROBE_SEND", "yes")]);
    assert!(second.status.success());
    assert_eq!(String::from_utf8_lossy(&second.stdout), stdout);
}

#[test]
fn a_live_run_refuses_without_the_matching_approval() {
    let out = run(&[("DARKBOT_G07_PROBE_SEND", "YES")]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("DARKBOT_G07_PROBE_APPROVED_PLAN_SHA256 is required"),
        "{err}"
    );

    let out = run(&[
        ("DARKBOT_G07_PROBE_SEND", "YES"),
        ("DARKBOT_G07_PROBE_APPROVED_PLAN_SHA256", &"0".repeat(64)),
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("does not match this probe plan; nothing sent"),
        "{err}"
    );

    // With the matching hash it proceeds to the inputs and stops there.
    let sha = plan_sha(&String::from_utf8_lossy(&run(&[]).stdout));
    let out = run(&[
        ("DARKBOT_G07_PROBE_SEND", "YES"),
        ("DARKBOT_G07_PROBE_APPROVED_PLAN_SHA256", &sha),
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION is required"),
        "{err}"
    );
}

#[test]
fn approval_precedes_every_read_latch_and_connection() {
    let pos = |needle: &str| {
        MAIN.find(needle)
            .unwrap_or_else(|| panic!("missing {needle:?}"))
    };
    let dry = pos("if env::var(\"DARKBOT_G07_PROBE_SEND\").as_deref() != Ok(\"YES\")");
    let dry_return = pos("        return;\n    }");
    let approval = pos("DARKBOT_G07_PROBE_APPROVED_PLAN_SHA256 does not match");
    let accounts = pos("rpc(\"eth_getBalance\"");
    let latch = pos("let probe_dir = latch_attempt(");
    let poller = pos("HeadPoller::connect(&rpc_endpoint)");
    let send = pos("connect_tls(host)");
    assert!(dry < dry_return && dry_return < approval && approval < accounts);
    assert!(accounts < latch && latch < poller && poller < send);
    assert_eq!(MAIN.matches("connect_tls(").count(), 1);
    assert_eq!(MAIN.matches("rpc(").count(), 2);
}

#[test]
fn the_probe_never_touches_the_g07_wallets_or_signed_bytes() {
    for source in [MAIN, PLAN] {
        for forbidden in [
            "DARKBOT_G07_LAUNCHER_PRIVATE_KEY",
            "DARKBOT_G07_BUYER_PRIVATE_KEY",
            "DARKBOT_G07_DRY_EVIDENCE",
            "DARKBOT_G07_SALT",
            "key_from_env(",
            "launch_calldata(",
            "buy_calldata(",
            "verify_live_authorization",
        ] {
            assert!(!source.contains(forbidden), "{forbidden}");
        }
    }
    // The only signing call signs a throwaway key.
    assert!(!MAIN.contains("sign_legacy("));
    let signer = PLAN
        .find("let key = throwaway_key(n);")
        .expect("throwaway key");
    let sign = PLAN.find("let signed = sign_legacy(").expect("sign");
    assert!(signer < sign);
}

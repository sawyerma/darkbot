//! Offline tests of the G0.7 builder probe. The dry run needs no input and
//! no network and always presents the same plan; a live run refuses before
//! any network access unless the approved plan SHA-256 matches. Source-order
//! checks pin that the approval precedes every read, latch and connection,
//! and that the probe never touches the G0.7 wallets or their keys.

use std::process::Command;

const PROBE: &str = env!("CARGO_BIN_EXE_g07_builder_probe");
const MAIN: &str = include_str!("../src/bin/g07_builder_probe/main.rs");
const PLAN: &str = include_str!("../src/bin/g07_builder_probe/plan.rs");
/// The plan the user approves. It includes the SHA-256 of the probe's and
/// the included G0.7 modules' sources, so any change to them, or to what a
/// live run would send, changes this hash and needs a new approval. (Pinned
/// here, not in plan.rs, which is itself part of that digest.)
/// Re-pinned 2026-10-21 (E6-2): Cargo.lock gained the geth_ipc -> trading
/// edge. The earlier pin a5ad014f… was already stale at 214d603e (plan
/// 20c4cfd2… after the E4 source changes). Re-pinned 2026-10-23 (C1/R1):
/// the guard, preflight reads and root check moved to the v5 contract (was
/// 32cd2947…); again for the B private cross-check binding (ee8d4aeb…).
/// D1 moved the root marker off the contract (was dd62ff52…). BSC-003: the
/// wire bodies and expiry offset moved to execution::bundle_wire, now bound
/// here too; the sent bytes are unchanged (golden tests) (was 06187594…).
/// BSC-010: the builder response has one absolute deadline (was 6c279792…).
/// Einheit 0: the guard requires the consumed attempt authority (was 8f8eb831…).
/// The builder response deadline is bundle_wire's, bound into v5 (was 75a04d5c…).
/// Review 25.10.: the guard checks the v5 runtime gates (was 69cee91f…).
/// T5: the authority return is a consumable capability (was 642280d6…).
/// Review 25.10. (Paket 1): ticketed release gate, durable abort (was 895ced5b…).
/// C1 runtime WSS dependencies in Cargo.lock (was afe825e3…).
/// C1 binary evidence: record/geth-ipc lines in Cargo.lock (was 19078cc4…).
/// Runtime SIGTERM handling: signal-hook in Cargo.lock (was 1207890f…).
/// Plan 27.10.: a live probe needs the native SEND switch (was 3a9dc0da…).
/// BSC3-023: libc for the journal's drive calls in Cargo.lock (was 331cec21…).
/// BSC3-063: secp256k1 for the source's signer recovery in Cargo.lock (was 9aca2d99…).
/// Neither value is an approval.
const PINNED_PLAN_SHA256: &str = "b522f502d5c6f018b1984feacb632df2ba73d0c86da72448a241f5c12e2f096b";

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
    assert_eq!(sha, PINNED_PLAN_SHA256);
    assert!(sha.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(stdout.contains("DRY RUN: nothing was sent and no connection was opened"));
    assert_eq!(stdout.matches("\ncase").count(), 11);
    // The dry run hands out no ready-made send command.
    assert!(!stdout.contains("DARKBOT_G07_PROBE_APPROVED_PLAN_SHA256="));
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
    let accounts = pos("rpc_balance(account)");
    let poller = pos("HeadPoller::connect(&rpc_endpoint)");
    let latch = pos("let probe_dir = latch_attempt(");
    let send = pos("connect_tls(host)");
    assert!(dry < dry_return && dry_return < approval && approval < accounts);
    assert!(accounts < poller && poller < latch && latch < send);
    assert_eq!(MAIN.matches("connect_tls(").count(), 1);
    // Requests come only from the shared request function the worked
    // examples use, and the connection is checked live right before the write.
    assert!(!MAIN.contains("http_request("));
    let build = pos("let built = case_request(n, case,");
    let alive = pos("transport_alive(&mut stream)");
    let write = pos(".write_all(&built.request)");
    assert!(send < build && build < alive && alive < write);
    assert_eq!(MAIN.matches("rpc_balance(").count(), 1);
    assert_eq!(MAIN.matches("rpc_pending_nonce(").count(), 1);
}

/// Plan 27.10.: a live probe needs BSC_ECON_SEND before its first read.
#[test]
fn a_live_probe_needs_the_send_switch_before_any_read() {
    let pos = |needle: &str| {
        MAIN.find(needle)
            .unwrap_or_else(|| panic!("missing {needle:?}"))
    };
    let send = pos("SwitchId::Send,");
    let root = pos("assert_repository_root();");
    let first_read = pos("rpc_balance(account)");
    assert!(root < send && send < first_read);
}

#[test]
fn a_failed_evidence_write_stops_sending_and_keeps_the_latch() {
    let pos = |needle: &str| {
        MAIN.find(needle)
            .unwrap_or_else(|| panic!("missing {needle:?}"))
    };
    // No evidence write is ignored.
    assert!(!MAIN.contains("let _ = evidence.replace"));
    assert!(!MAIN.contains("let _ = persist("));
    // A report is on file before the first send, and the latch is released
    // only before the send loop (nothing sent yet), never after it started.
    let first_write = pos("if let Err(e) = persist(&[], \"IN_PROGRESS\")");
    let loop_start = pos("let run = run_cases(");
    assert!(first_write < loop_start);
    assert_eq!(MAIN.matches("release_unsent_attempt(").count(), 2);
    assert!(!MAIN[loop_start..].contains("release_unsent_attempt("));
    assert!(MAIN[loop_start..].contains("STOPPED_EVIDENCE_WRITE_FAILED"));
    assert_eq!(MAIN.matches("std::process::exit(3)").count(), 2);
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
    assert_eq!(PLAN.matches("let signed = sign_legacy(").count(), 1);
    let signer = PLAN
        .find("case_keys(case_index).map(|key| {")
        .expect("throwaway keys");
    let sign = PLAN.find("let signed = sign_legacy(").expect("sign");
    assert!(signer < sign);
    // case_keys yields throwaway keys only.
    let keys = PLAN.find("pub(crate) fn case_keys(").expect("case_keys");
    let keys_end = keys + PLAN[keys..].find("\n}\n").expect("end");
    assert_eq!(PLAN[keys..keys_end].matches("throwaway_key(").count(), 2);
    assert!(!PLAN[keys..keys_end].contains("from_slice("));
}

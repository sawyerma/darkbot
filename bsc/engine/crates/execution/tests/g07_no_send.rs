//! Offline no-send tests of the G0.7 Mainnet self-launch + BUY binary.
//!
//! They run the built binary without the explicit send authorization and
//! without inputs: it must refuse before reading any input, touching the
//! network or opening a builder connection. Source-order checks pin that the
//! compiled-in acceptance guard is evaluated, and refusal is terminal, before
//! the first builder TLS connection is opened; that the attempt is latched and
//! the BlockRazor token checked before the guard; that the bundle expiry comes
//! from the next block read after the transport prewarm; that nothing after
//! the release can panic; and that child processes never inherit this
//! process's environment.

use std::process::Command;

const G07: &str = env!("CARGO_BIN_EXE_g07_mainnet_self_launch_buy");
const SOURCE: &str = include_str!("../src/bin/g07_mainnet_self_launch_buy/main.rs");
const BIN_SOURCES: [&str; 5] = [
    SOURCE,
    include_str!("../src/bin/g07_mainnet_self_launch_buy/builder_submission.rs"),
    include_str!("../src/bin/g07_mainnet_self_launch_buy/chain_reads.rs"),
    include_str!("../src/bin/g07_mainnet_self_launch_buy/inputs.rs"),
    include_str!("../src/bin/g07_mainnet_self_launch_buy/transactions.rs"),
];

fn pos(needle: &str) -> usize {
    SOURCE
        .find(needle)
        .unwrap_or_else(|| panic!("main.rs lacks {needle:?}"))
}

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
    let out = run(&[
        ("DARKBOT_G07_MAINNET_SEND", "yes"),
        ("DARKBOT_G07_DRY_EVIDENCE", "/nonexistent/dry.json"),
    ]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("DARKBOT_G07_MAINNET_SEND=YES only for the explicitly authorized G0.7 attempt"),
        "{err}"
    );
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
    // The only builder connections are these two, both after the guard.
    assert_eq!(SOURCE.matches("connect_tls(").count(), 2);
    assert!(!SOURCE.contains("connect_tls_port("));
    assert!(pos("connect_tls(BLOCKRAZOR_HOST)") > refused);
}

#[test]
fn retired_free_journal_path_is_refused() {
    let out = run(&[
        ("DARKBOT_G07_MAINNET_SEND", "YES"),
        ("DARKBOT_G07_CAPTURE_JOURNAL", "/tmp/g07.journal"),
    ]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("DARKBOT_G07_CAPTURE_JOURNAL is retired"),
        "{err}"
    );
}

#[test]
fn token_evidence_and_latch_precede_the_guard() {
    let token = pos("valid_header_value(&blockrazor_auth)");
    let evidence_target = pos("validate_new_file_target(&evidence_path)");
    let probe = pos("let probe = head_poller");
    let latch = pos("let attempt_dir = latch_attempt(");
    let evidence_file = pos("let mut evidence_file = ");
    let prearm = pos("CaptureSession::prearm(");
    let guard = pos("let contract = match verify_live_authorization(");
    assert!(token < probe && evidence_target < probe);
    assert!(probe < latch && latch < evidence_file && evidence_file < prearm && prearm < guard);
    assert_eq!(SOURCE.matches("req_env(\"DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION\")").count(), 1);
    assert!(SOURCE.contains("req_env(\"DARKBOT_G07_EVIDENCE_PATH\")"));
    assert!(!SOURCE.contains("env::var(\"DARKBOT_G07_EVIDENCE_PATH\")"));
    assert!(!SOURCE.contains("req_env(\"DARKBOT_G07_CAPTURE_JOURNAL\")"));
    // A refused run releases its latch (it journaled no signed byte) before
    // the terminal refusal.
    let refused = pos("panic!(\"G0.7 live authorization refused: {e}\")");
    assert!(SOURCE[guard..refused].contains("unsent.release(\"refused\")"));
}

#[test]
fn bundle_expiry_comes_from_the_next_block_read_after_the_transport_prewarm() {
    let guard = pos("let contract = match verify_live_authorization(");
    let raw_txs = pos("record_raw_tx(\"buy_B\"");
    let poller_check = pos("if let Err(e) = head_poller.latest()");
    let club48_tls = pos("match connect_tls(CLUB48_HOST)");
    let blockrazor_tls = pos("match connect_tls(BLOCKRAZOR_HOST)");
    let read = pos("let fresh = match head_poller.next_head(HEAD_TRANSITION_TIMEOUT)");
    let window = pos("let window = match bundle_window(");
    let first_body = pos("let club48_body = json!(");
    let journaled = pos(".and_then(|()| capture.wait_appended(CAPTURE_APPEND_TIMEOUT))");
    let idle = pos("if prewarmed_at.elapsed() > MAX_BUILDER_IDLE");
    let liveness = pos("transport_alive(stream)");
    let go = pos("let _ = gobr.send(());");
    let release = pos("barrier.wait();");
    assert!(guard < raw_txs && raw_txs < poller_check && poller_check < club48_tls);
    assert!(club48_tls < blockrazor_tls && blockrazor_tls < read && read < window);
    assert!(window < first_body && first_body < journaled && journaled < idle);
    assert!(idle < liveness && liveness < go && go < release);
    // The window is computed from the send head read, not the probe.
    assert!(SOURCE[window..first_body].contains("&fresh.head,"));
    assert!(SOURCE[window..first_body].contains("fresh.excess_staleness_ms,"));
    assert!(!SOURCE.contains("send_head.checked_add"));
    assert_eq!(SOURCE.matches("let max_block = window.max_block;").count(), 1);
    // Both bodies carry exactly that maxBlockNumber; maxTimestamp only 48Club.
    assert_eq!(SOURCE.matches("\"maxBlockNumber\"").count(), 2);
    assert_eq!(SOURCE.matches("\"maxBlockNumber\":max_block").count(), 2);
    assert_eq!(SOURCE.matches("\"maxTimestamp\"").count(), 1);
    assert_eq!(SOURCE.matches("\"maxTimestamp\":window.max_timestamp").count(), 1);
    assert!(pos("\"maxTimestamp\":") < pos("let blockrazor_body = json!("));
}

const PANICKING: [&str; 10] = [
    "panic!(",
    ".expect(",
    ".unwrap()",
    "assert!(",
    "assert_eq!(",
    "unreachable!(",
    "hex_u64(",
    "req_",
    "println!(",
    "eprintln!(",
];

fn function_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("missing {signature}"));
    let rest = &source[start..];
    let end = rest[1..]
        .find("\npub(crate) fn ")
        .or_else(|| rest.find("\n#[cfg(test)]"))
        .unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn nothing_after_the_release_can_panic() {
    let release = pos("barrier.wait();");
    let end = pos("fn mark_released(");
    let after = &SOURCE[release..end];
    for forbidden in PANICKING {
        assert!(!after.contains(forbidden), "{forbidden} after the release");
    }
    let builder = BIN_SOURCES[1];
    for signature in [
        "pub(crate) fn submit_provider(",
        "pub(crate) fn read_http_response(",
        "pub(crate) fn parse_http_response(",
        "pub(crate) fn decode_chunked_body(",
        "pub(crate) fn panicked_provider_result(",
        "pub(crate) fn provider_json(",
    ] {
        let body = function_body(builder, signature);
        for forbidden in PANICKING {
            assert!(!body.contains(forbidden), "{forbidden} in {signature}");
        }
    }
    let rpc = function_body(BIN_SOURCES[2], "pub(crate) fn rpc(");
    for forbidden in PANICKING {
        assert!(!rpc.contains(forbidden), "{forbidden} in rpc");
    }
    assert!(rpc.contains("\"--max-time\""));
    assert!(after.contains("panicked_provider_result(\"48Club/Puissant\")"));
    assert!(after.contains("panicked_provider_result(\"BlockRazor\")"));
    // Provisional evidence before the receipt window; the final evidence file
    // write before stdout; a failed write is a non-zero exit.
    assert!(pos("let provisional = write_evidence(") < pos("let receipt_poll_deadline"));
    let file = after.find("write_evidence(&mut evidence_file, &pretty)").expect("final evidence");
    let stdout = after.find("std::io::stdout()").expect("stdout copy");
    assert!(file < stdout);
    assert!(after.contains("std::process::exit(3)"));
}

#[test]
fn child_processes_are_spawned_only_with_a_cleared_environment() {
    let spawns: usize = BIN_SOURCES
        .iter()
        .map(|s| s.matches("Command::new(").count())
        .sum();
    assert_eq!(spawns, 1);
    let chain_reads = BIN_SOURCES[2];
    let helper = chain_reads
        .find("pub(crate) fn child_command(")
        .expect("child_command");
    let spawn = chain_reads.find("Command::new(").expect("spawn");
    let clear = chain_reads.find("cmd.env_clear();").expect("env_clear");
    assert!(helper < spawn && spawn < clear);
    assert!(chain_reads.contains("child_command(\"curl\")"));
    assert!(chain_reads.contains("child_command(\"node\")"));
}

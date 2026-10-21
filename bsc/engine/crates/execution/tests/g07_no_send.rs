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
const BIN_SOURCES: [&str; 6] = [
    SOURCE,
    include_str!("../src/bin/g07_mainnet_self_launch_buy/builder_submission.rs"),
    include_str!("../src/bin/g07_mainnet_self_launch_buy/chain_reads.rs"),
    include_str!("../src/bin/g07_mainnet_self_launch_buy/inputs.rs"),
    include_str!("../src/bin/g07_mainnet_self_launch_buy/transactions.rs"),
    include_str!("../src/bin/g07_mainnet_self_launch_buy/shared.rs"),
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
        err.contains(
            "DARKBOT_G07_MAINNET_SEND=YES only for the explicitly authorized G0.7 attempt"
        ),
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
    let evidence_target = pos("validate_evidence_target(&evidence_path)");
    let probe = pos("let probe = head_poller");
    let latch = pos("let attempt_dir = latch_attempt(");
    let evidence_file = pos("let evidence_out = owner_uid(&attempt_dir)");
    let prearm = pos("CaptureSession::prearm(");
    let guard = pos("let contract = match verify_live_authorization(");
    assert!(token < probe && evidence_target < probe);
    assert!(probe < latch && latch < evidence_file && evidence_file < prearm && prearm < guard);
    assert_eq!(
        SOURCE
            .matches("req_env(\"DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION\")")
            .count(),
        1
    );
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
    let first_body = pos("let club48_body = club48_bundle_body(");
    let journaled = pos(".and_then(|()| capture.wait_appended(CAPTURE_APPEND_TIMEOUT))");
    let idle = pos("if club48_ready_at.elapsed() > MAX_BUILDER_IDLE");
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
    assert_eq!(
        SOURCE.matches("let max_block = window.max_block;").count(),
        1
    );
    // Both bodies come from the shared builders (the probe sends the same
    // ones) with exactly that maxBlockNumber; maxTimestamp only for 48Club.
    assert_eq!(
        SOURCE
            .matches("club48_bundle_body(&request_id, txs, max_block, window.max_timestamp)")
            .count(),
        1
    );
    assert_eq!(
        SOURCE
            .matches("blockrazor_bundle_body(&buy_hash, txs, max_block)")
            .count(),
        1
    );
    assert!(!SOURCE.contains("\"maxBlockNumber\""));
    let club48 = function_body(BIN_SOURCES[1], "pub(crate) fn club48_bundle_body(");
    let blockrazor = function_body(BIN_SOURCES[1], "pub(crate) fn blockrazor_bundle_body(");
    assert!(club48.contains("\"maxBlockNumber\":{max_block}"));
    assert!(club48.contains("\"maxTimestamp\":{max_timestamp}"));
    assert!(blockrazor.contains("\"maxBlockNumber\":{max_block}"));
    assert!(!blockrazor.contains("maxTimestamp"));
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
        .map(|i| i + 1)
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
        "pub(crate) fn provider_record(",
    ] {
        let body = function_body(builder, signature);
        for forbidden in PANICKING {
            assert!(!body.contains(forbidden), "{forbidden} in {signature}");
        }
    }
    let rpc = function_body(BIN_SOURCES[2], "fn rpc(");
    for forbidden in PANICKING {
        assert!(!rpc.contains(forbidden), "{forbidden} in rpc");
    }
    assert!(rpc.contains("\"--max-time\""));
    assert!(after.contains("panicked_provider_result(\"48Club/Puissant\")"));
    assert!(after.contains("panicked_provider_result(\"BlockRazor\")"));
    // Evidence right after the release (before the joins), after the send
    // threads, and final; the final file write before stdout; a failed write
    // is a non-zero exit. Evidence is only ever replaced atomically.
    let at_release = pos("release_evidence = write_evidence(&evidence_out, &evidence);");
    let marker = pos("release_marker = mark_released(&attempt_dir, release_after_ns);");
    let joins = pos("h48.join()");
    assert!(release < at_release && at_release < marker && marker < joins);
    // Between the release and the joins nothing can leave the scope early,
    // and the only pre-egress stop after the barrier text is the one for a
    // failed spawn (the scope returns Err only before the barrier).
    assert!(!SOURCE[release..joins].contains("return"));
    assert!(!SOURCE[release..joins].contains(")?"));
    assert_eq!(after.matches("pre_egress_abort(").count(), 1);
    assert!(after.contains(
        "Err(e) => pre_egress_abort(capture, &capture_clock, &unsent, \"send threads\", &e)"
    ));
    let marker_fn =
        &SOURCE[pos("fn mark_released(")..pos("/// What a stop before the release hands back")];
    for forbidden in PANICKING {
        assert!(
            !marker_fn.contains(forbidden),
            "{forbidden} in mark_released"
        );
    }
    assert!(pos("let provisional = write_evidence(") < pos("let receipt_poll_deadline"));
    let file = after
        .find("let evidence_written = write_evidence(&evidence_out, &evidence);")
        .expect("final evidence");
    let stdout = after.find("std::io::stdout()").expect("stdout copy");
    assert!(file < stdout);
    assert!(after.contains("std::process::exit(3)"));
    for source in BIN_SOURCES {
        assert!(!source.contains("set_len("), "evidence truncated in place");
    }
    // G0.7 never claims the canonical window reconciled; G0.8 does that.
    assert!(SOURCE.contains("evidence::CANONICAL_WINDOW_RECONCILED_IN_G08, false"));
    assert!(!SOURCE.contains("evidence::CANONICAL_WINDOW_RECONCILED_IN_G08, true"));
}

#[test]
fn senders_wait_for_the_go_signal_and_the_barrier_before_writing() {
    // Each spawned sender first receives its go signal, then submit_provider
    // waits on the barrier before the first write.
    assert!(pos("wait48.recv().ok().map(") < pos("submit_provider("));
    let after_48 = pos("wait48.recv().ok().map(");
    assert!(SOURCE[after_48..].contains("waitbr.recv().ok().map("));
    let submit = function_body(BIN_SOURCES[1], "pub(crate) fn submit_provider(");
    let wait = submit.find("barrier.wait();").expect("barrier");
    let write = submit.find("write_all(&request)").expect("write");
    assert!(wait < write);
    assert_eq!(submit.matches("write_all(").count(), 1);
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
    assert!(!chain_reads.contains("child_command(\"node\")"));
}

#[test]
fn key_cleanup_precedes_every_external_or_concurrent_startup_boundary() {
    let source = include_str!("../src/bin/g07_mainnet_self_launch_buy/main.rs");
    let cleanup = source.find("unsafe { take_startup_keys() }").unwrap();
    let prefix = &source[source.find("fn main()").unwrap()..cleanup];
    for forbidden in ["rpc_", "connect_tls(", "HeadPoller::", "CaptureSession::", "thread::spawn", "Command::"] {
        // Endpoint parsing is a pure local operation, not RPC IO.
        assert!(!prefix.replace("parse_rpc_endpoint", "parse_endpoint").replace("rpc_endpoint", "endpoint").contains(forbidden), "{forbidden}");
    }
}

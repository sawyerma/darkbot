//! G0.7 pre-arm checks 6 and 10: probes the `eth_sendBundle` method, request
//! format and expiry acceptance of 48Club and BlockRazor with deliberately
//! non-executable bundles, through the G0.7 binary's own framing, TLS, head
//! poller, expiry window and signer (included by path, not copied).
//!
//! Without DARKBOT_G07_PROBE_SEND=YES it is a dry run: it prints the exact
//! plan and its SHA-256 and opens no connection, reads no secret and writes
//! nothing. A live run sends exactly that plan, once, and only when
//! DARKBOT_G07_PROBE_APPROVED_PLAN_SHA256 equals the plan's SHA-256. The G0.7
//! wallets, their keys and the signed DRY bytes are never read.

#[path = "../g07_mainnet_self_launch_buy/shared.rs"]
mod shared;

#[allow(dead_code)]
#[path = "../g07_mainnet_self_launch_buy/builder_submission.rs"]
mod builder_submission;
#[allow(dead_code)]
#[path = "../g07_mainnet_self_launch_buy/chain_reads.rs"]
mod chain_reads;
#[allow(dead_code)]
#[path = "../g07_mainnet_self_launch_buy/inputs.rs"]
mod inputs;
#[allow(dead_code)]
#[path = "../g07_mainnet_self_launch_buy/transactions.rs"]
mod transactions;

mod plan;

use builder_submission::*;
use chain_reads::*;
use inputs::*;
use plan::*;
use shared::*;
use transactions::*;

fn main() {
    let plan = plan_text();
    let plan_sha256 = sha256_hex(plan.as_bytes());
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "{plan}");
    let _ = writeln!(out, "PLAN_SHA256={plan_sha256}");

    if env::var("DARKBOT_G07_PROBE_SEND").as_deref() != Ok("YES") {
        let _ = writeln!(
            out,
            "DRY RUN: nothing was sent and no connection was opened. A live run needs DARKBOT_G07_PROBE_SEND=YES and the user's approval of exactly this PLAN_SHA256."
        );
        return;
    }
    let approved = req_env("DARKBOT_G07_PROBE_APPROVED_PLAN_SHA256");
    assert_eq!(
        approved.trim().to_ascii_lowercase(),
        plan_sha256,
        "DARKBOT_G07_PROBE_APPROVED_PLAN_SHA256 does not match this probe plan; nothing sent"
    );

    // Inputs, before anything touches the network.
    let token = req_env("DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION");
    assert!(
        valid_header_value(&token),
        "invalid BlockRazor Authorization value"
    );
    let rpc_endpoint =
        parse_rpc_endpoint(&req_env("BSC_MAINNET_RPC_HTTPS")).unwrap_or_else(|e| panic!("{e}"));
    assert_repository_root();

    // Non-executability, read-only: the throwaway accounts hold nothing and
    // have sent nothing.
    for account in throwaway_accounts() {
        let from = account.to_string();
        let balance = rpc("eth_getBalance", json!([from, "latest"]))
            .unwrap_or_else(|e| panic!("balance of {from}: {e}; nothing sent"));
        let nonce = rpc("eth_getTransactionCount", json!([from, "pending"]))
            .unwrap_or_else(|e| panic!("nonce of {from}: {e}; nothing sent"));
        assert!(
            balance.as_str() == Some("0x0") && nonce.as_str() == Some("0x0"),
            "throwaway account {from} is not empty (balance {balance}, pending nonce {nonce}); nothing sent"
        );
    }

    // The head poller, exercised read-only before the plan is used up.
    let mut head_poller = HeadPoller::connect(&rpc_endpoint)
        .unwrap_or_else(|e| panic!("head poller: {e}; nothing sent"));
    let probe = head_poller
        .next_head(HEAD_TRANSITION_TIMEOUT)
        .unwrap_or_else(|e| panic!("head probe: {e}; nothing sent"));
    bundle_window(
        probe.head.number,
        &probe.head.hash,
        &probe.head,
        probe.local_unix_ms,
        probe.excess_staleness_ms,
    )
    .unwrap_or_else(|e| panic!("head probe: {e}; nothing sent"));

    // Single use per approved plan, and the private place of its evidence.
    let latch_id = format!("builder-probe-{}", &plan_sha256[..12]);
    let probe_dir = latch_attempt(
        Path::new(ATTEMPT_STATE_ROOT),
        &latch_id,
        &format!(
            "plan_sha256={plan_sha256}\npid={}\nunix_time_ms={}\n",
            std::process::id(),
            unix_time_ms()
        ),
    )
    .unwrap_or_else(|e| panic!("builder probe latch: {e}; nothing sent"));
    let evidence_path = probe_dir.join("probe.evidence.json");
    let evidence = owner_uid(&probe_dir)
        .and_then(|uid| EvidenceFile::create(&evidence_path.to_string_lossy(), uid))
        .unwrap_or_else(|e| {
            let released = release_unsent_attempt(&probe_dir, None, "pre-egress");
            panic!("builder probe evidence: {e}; nothing sent; {released}")
        });

    let mut results = Vec::new();
    for (n, case) in CASES.iter().enumerate() {
        if n > 0 {
            thread::sleep(PROBE_SPACING);
        }
        results.push(run_case(n, case, &mut head_poller, &token));
        let report = report(&plan_sha256, &results);
        let _ = evidence.replace(format!("{report}\n").as_bytes());
    }
    let report = report(&plan_sha256, &results);
    let written = evidence.replace(format!("{report}\n").as_bytes());
    let _ = writeln!(out, "{report}");
    let _ = writeln!(
        out,
        "PROBE_EVIDENCE={} ({})",
        evidence_path.display(),
        match &written {
            Ok(()) => "written".to_string(),
            Err(e) => format!("NOT written: {e}"),
        }
    );
    if written.is_err() {
        std::process::exit(3);
    }
}

/// Sends case `n`: a new connection first (as G0.7 prewarms), then a fresh
/// head (up to PROBE_HEAD_ATTEMPTS new heads that pass the G0.7 gates, else
/// the case is skipped), then the request from the shared request function
/// over the still-live connection; the answer is recorded. Nothing is retried
/// after a send.
fn run_case(n: usize, case: &Case, poller: &mut HeadPoller, token: &str) -> Value {
    let host = case.builder.host();
    let before_connect_ms = unix_time_ms();
    let mut stream = match connect_tls(host) {
        Ok(stream) => stream,
        Err(e) => return json!({"case":case.id,"outcome":"TRANSPORT_ERROR_NOT_SENT","reason":e}),
    };
    let connected_ms = unix_time_ms();
    let mut refused = Vec::new();
    let mut fresh_window = None;
    for _ in 0..PROBE_HEAD_ATTEMPTS {
        match poller.next_head(HEAD_TRANSITION_TIMEOUT) {
            Ok(fresh) => match bundle_window(
                fresh.head.number,
                &fresh.head.hash,
                &fresh.head,
                fresh.local_unix_ms,
                fresh.excess_staleness_ms,
            ) {
                Ok(window) => {
                    fresh_window = Some((fresh, window));
                    break;
                }
                Err(e) => refused.push(e),
            },
            Err(e) => refused.push(e),
        }
    }
    let Some((fresh, window)) = fresh_window else {
        return json!({"case":case.id,"outcome":"SKIPPED_NO_FRESH_HEAD","reasons":refused});
    };
    let built = case_request(n, case, &fresh.head, &window, token);
    if let Err(e) = transport_alive(&mut stream) {
        return json!({"case":case.id,"outcome":"TRANSPORT_ERROR_NOT_SENT","reason":e});
    }
    let mut written_ms = None;
    let sent = stream
        .write_all(&built.request)
        .and_then(|()| stream.flush())
        .map_err(|e| format!("write: {e}"))
        .and_then(|()| {
            written_ms = Some(unix_time_ms());
            read_http_response(&mut stream)
        });
    let answered_ms = unix_time_ms();
    let (outcome, status, answer) = match &sent {
        Ok(r) => (
            classify(&r.status, &r.body),
            Some(r.status.as_str()),
            Some(r.body.as_str()),
        ),
        Err(_) => ("TRANSPORT_ERROR", None, None),
    };
    json!({
        "case":case.id,
        "builder":built.host,
        "question":case.question,
        "gas_price_wei":case.gas_price,
        "tx_hashes":[built.txs[0].signed.hash.to_string(), built.txs[1].signed.hash.to_string()],
        "send_head":fresh.head.number,
        "send_head_hash":fresh.head.hash,
        "send_head_milli_timestamp":fresh.head.milli_timestamp,
        "send_head_millisecond_phase":fresh.head.milli_timestamp % 1000,
        "head_read_local_unix_ms":fresh.local_unix_ms,
        "head_age_ms":window.head_age_ms,
        "heads_refused_before":refused,
        "max_block":built.max_block,
        "max_timestamp":built.max_timestamp,
        "body":built.body,
        "body_sha256":sha256_hex(built.body.as_bytes()),
        // The full request length would reveal the token's length.
        "body_len":built.body.len(),
        "authorization_persisted":false,
        "before_connect_unix_ms":before_connect_ms,
        "connected_unix_ms":connected_ms,
        "request_written_unix_ms":written_ms,
        "answer_unix_ms":answered_ms,
        "http_status":status,
        "response_body":answer,
        "transport_error":sent.as_ref().err(),
        "outcome":outcome
    })
}

fn report(plan_sha256: &str, results: &[Value]) -> String {
    let accounts = throwaway_accounts();
    serde_json::to_string_pretty(&json!({
        "schema":PROBE_SCHEMA,
        "plan_sha256":plan_sha256,
        "throwaway_accounts":accounts.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
        "cases":results,
        "scope":"submission-time acceptance only; building-time behavior, inclusion and whether 0.05 gwei suffices are not shown",
        "outcome_classes":"ACCEPTED, METHOD_REJECTED, FORMAT_REJECTED, EXECUTION_REJECTED, PRICE_REJECTED, EXPIRY_REJECTED, BUNDLE_REJECTED_UNSPECIFIED, OTHER_REJECTED, HTTP_ERROR, UNPARSEABLE, TRANSPORT_ERROR (after the write began), TRANSPORT_ERROR_NOT_SENT, SKIPPED_NO_FRESH_HEAD; the raw answer is the evidence"
    }))
    .unwrap_or_else(|e| format!("{{\"report_serialization_error\":\"{e}\"}}"))
}

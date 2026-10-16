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
    let txs = probe_txs();
    let plan = plan_text(&txs);
    let plan_sha256 = sha256_hex(plan.as_bytes());
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "{plan}");
    let _ = writeln!(out, "PLAN_SHA256={plan_sha256}");

    if env::var("DARKBOT_G07_PROBE_SEND").as_deref() != Ok("YES") {
        let _ = writeln!(
            out,
            "DRY RUN: nothing was sent and no connection was opened. To send exactly this plan once: DARKBOT_G07_PROBE_SEND=YES DARKBOT_G07_PROBE_APPROVED_PLAN_SHA256={plan_sha256}"
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
    for tx in &txs {
        let from = tx.from.to_string();
        let balance = rpc("eth_getBalance", json!([from, "latest"]))
            .unwrap_or_else(|e| panic!("balance of {from}: {e}; nothing sent"));
        let nonce = rpc("eth_getTransactionCount", json!([from, "pending"]))
            .unwrap_or_else(|e| panic!("nonce of {from}: {e}; nothing sent"));
        assert!(
            balance.as_str() == Some("0x0") && nonce.as_str() == Some("0x0"),
            "throwaway account {from} is not empty (balance {balance}, pending nonce {nonce}); nothing sent"
        );
    }

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
        .unwrap_or_else(|e| panic!("builder probe evidence: {e}; nothing sent"));

    let mut head_poller = HeadPoller::connect(&rpc_endpoint)
        .unwrap_or_else(|e| panic!("head poller: {e}; nothing sent"));
    let mut results = Vec::new();
    for (n, case) in CASES.iter().enumerate() {
        if n > 0 {
            thread::sleep(PROBE_SPACING);
        }
        results.push(run_case(case, &txs, &mut head_poller, &token));
        let report = report(&plan_sha256, &txs, &results);
        let _ = evidence.replace(format!("{report}\n").as_bytes());
    }
    let report = report(&plan_sha256, &txs, &results);
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

/// Sends one case on a fresh head over a new connection and records the
/// answer. A stale head skips the case; nothing is retried.
fn run_case(case: &Case, txs: &[ProbeTx; 2], poller: &mut HeadPoller, token: &str) -> Value {
    let fresh = match poller.next_head(HEAD_TRANSITION_TIMEOUT) {
        Ok(fresh) => fresh,
        Err(e) => return json!({"case":case.id,"outcome":"SKIPPED_NO_FRESH_HEAD","reason":e}),
    };
    let window = match bundle_window(
        fresh.head.number,
        &fresh.head.hash,
        &fresh.head,
        fresh.local_unix_ms,
        fresh.excess_staleness_ms,
    ) {
        Ok(window) => window,
        Err(e) => return json!({"case":case.id,"outcome":"SKIPPED_STALE_HEAD","reason":e}),
    };
    let (max_block, max_timestamp) = expiry_values(case.expiry, &fresh.head, &window);
    let body = case_body(case, txs, max_block, max_timestamp);
    let host = case.builder.host();
    let authorization = (case.builder == Builder::BlockRazor).then_some(token);
    let request = http_request(host, &body, authorization);
    let sent = connect_tls(host).and_then(|mut stream| {
        stream
            .write_all(&request)
            .and_then(|()| stream.flush())
            .map_err(|e| format!("write: {e}"))?;
        read_http_response(&mut stream)
    });
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
        "builder":host,
        "question":case.question,
        "send_head":fresh.head.number,
        "send_head_milli_timestamp":fresh.head.milli_timestamp,
        "head_age_ms":window.head_age_ms,
        "max_block":max_block,
        "max_timestamp":max_timestamp,
        "body":body,
        "body_sha256":sha256_hex(body.as_bytes()),
        "request_len":request.len(),
        "authorization_persisted":false,
        "http_status":status,
        "response_body":answer,
        "transport_error":sent.as_ref().err(),
        "outcome":outcome
    })
}

fn report(plan_sha256: &str, txs: &[ProbeTx; 2], results: &[Value]) -> String {
    serde_json::to_string_pretty(&json!({
        "schema":PROBE_SCHEMA,
        "plan_sha256":plan_sha256,
        "throwaway_accounts":[txs[0].from.to_string(), txs[1].from.to_string()],
        "tx_hashes":[txs[0].signed.hash.to_string(), txs[1].signed.hash.to_string()],
        "cases":results,
        "scope":"submission-time acceptance only; building-time behavior and inclusion are not shown",
        "outcome_classes":"ACCEPTED, FORMAT_REJECTED, EXPIRY_REJECTED, PRICE_REJECTED, EXECUTION_REJECTED, OTHER_REJECTED, HTTP_ERROR, UNPARSEABLE, TRANSPORT_ERROR, SKIPPED_*; the raw answer is the evidence"
    }))
    .unwrap_or_else(|e| format!("{{\"report_serialization_error\":\"{e}\"}}"))
}

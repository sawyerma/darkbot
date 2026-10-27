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
use darkbot_bsc_record::{Value as BinaryValue, schema::g07::probe};
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
    // Plan 27.10.: every Mainnet bundle submission needs BSC_ECON_SEND
    // ENABLED in the native owner state of this boot session (the probe
    // signs with throwaway accounts only, so no wallet switch).
    darkbot_bsc_execution::sources::require_enabled_here(&[
        darkbot_bsc_execution::sources::SwitchId::Send,
    ])
    .unwrap_or_else(|e| panic!("probe refused by the native switches: {e}; nothing sent"));

    // Non-executability, read-only: the throwaway accounts hold nothing and
    // have sent nothing.
    for account in throwaway_accounts() {
        let from = account.to_string();
        let balance =
            rpc_balance(account).unwrap_or_else(|e| panic!("balance of {from}: {e}; nothing sent"));
        let nonce = rpc_pending_nonce(account)
            .unwrap_or_else(|e| panic!("nonce of {from}: {e}; nothing sent"));
        assert!(
            balance == U256::ZERO && nonce == 0,
            "throwaway account {from} is not empty; nothing sent"
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
    let evidence_path = probe_dir.join("probe.evidence.dbrec");
    let evidence = owner_uid(&probe_dir)
        .and_then(|uid| EvidenceFile::create(&evidence_path.to_string_lossy(), uid))
        .unwrap_or_else(|e| {
            let (_, released) = release_unsent_attempt(&probe_dir, None, "pre-egress");
            panic!("builder probe evidence: {e}; nothing sent; {released}")
        });

    let persist = |results: &[Draft], status: &str| {
        report(&plan_sha256, results, status).and_then(|bytes| evidence.replace(&bytes))
    };
    // The evidence file must take a report before anything is sent.
    if let Err(e) = persist(&[], "IN_PROGRESS") {
        let (_, released) = release_unsent_attempt(&probe_dir, Some(&evidence), "pre-egress");
        panic!("builder probe evidence: {e}; nothing sent; {released}");
    }

    let run = run_cases(
        CASES.len(),
        PROBE_SPACING,
        |n| run_case(n, &CASES[n], &mut head_poller, &token),
        |results| persist(results, "IN_PROGRESS"),
    );
    match run {
        Ok(results) => {
            let written = persist(&results, "COMPLETE");
            let _ = writeln!(out, "{}", report_view(&plan_sha256, &results, "COMPLETE"));
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
        Err((results, e)) => {
            // A request may have left: the latch is kept, nothing else is sent.
            let stopped = "STOPPED_EVIDENCE_WRITE_FAILED";
            let _ = writeln!(out, "{}", report_view(&plan_sha256, &results, stopped));
            let _ = writeln!(
                std::io::stderr().lock(),
                "STOP: evidence write failed after case {} ({e}); the remaining cases were not sent; the latch stays; stdout holds the report",
                results.len()
            );
            std::process::exit(3);
        }
    }
}

/// Runs the cases in order and persists the evidence after each one. A
/// failed write stops the run at once: no further case is sent, and the
/// results so far come back with the error.
fn run_cases(
    count: usize,
    spacing: Duration,
    mut send: impl FnMut(usize) -> Draft,
    mut persist: impl FnMut(&[Draft]) -> Result<(), String>,
) -> Result<Vec<Draft>, (Vec<Draft>, String)> {
    let mut results = Vec::with_capacity(count);
    for n in 0..count {
        if n > 0 {
            thread::sleep(spacing);
        }
        results.push(send(n));
        if let Err(e) = persist(&results) {
            return Err((results, e));
        }
    }
    Ok(results)
}

/// Sends case `n`: a new connection first (as G0.7 prewarms), then a fresh
/// head (up to PROBE_HEAD_ATTEMPTS new heads that pass the G0.7 gates, else
/// the case is skipped), then the request from the shared request function
/// over the still-live connection; the answer is recorded. Nothing is retried
/// after a send.
fn run_case(n: usize, case: &Case, poller: &mut HeadPoller, token: &str) -> Draft {
    let host = case.builder.host();
    let before_connect_ms = unix_time_ms();
    let mut stream = match connect_tls(host) {
        Ok(stream) => stream,
        Err(e) => return case_failure(case, "TRANSPORT_ERROR_NOT_SENT", &e),
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
        return case_failure(case, "SKIPPED_NO_FRESH_HEAD", &refused.join("; "));
    };
    let built = case_request(n, case, &fresh.head, &window, token);
    if let Err(e) = transport_alive(&mut stream) {
        return case_failure(case, "TRANSPORT_ERROR_NOT_SENT", &e);
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
    let outcome = match &sent {
        Ok(r) => classify(&r.status, &String::from_utf8_lossy(&r.body)),
        Err(_) => "TRANSPORT_ERROR",
    };
    let provider = ProviderResult {
        provider: host,
        write_started_ns: None,
        write_flushed_ns: None,
        bytes_may_have_left: true,
        answer: sent
            .as_ref()
            .ok()
            .map(|r| decode_builder_answer(&r.status, &r.body)),
        error: sent.as_ref().err().cloned(),
    };
    use probe::case as c;
    let mut d = Draft::new(&c::SCHEMA);
    d.set(c::CASE, case.id)
        .set(c::OUTCOME, outcome)
        .set(c::BUILDER, built.host)
        .set(c::QUESTION, case.question)
        .set(c::GAS_PRICE, case.gas_price)
        .set(c::SEND_HEAD, fresh.head.number)
        .set(c::SEND_HEAD_HASH, fresh.head.hash.as_str())
        .set(c::SEND_HEAD_MS, fresh.head.milli_timestamp)
        .set(c::HEAD_READ_MS, fresh.local_unix_ms)
        .set(c::HEAD_AGE_MS, window.head_age_ms)
        .set(c::MAX_BLOCK, built.max_block)
        .set_opt(c::MAX_TIMESTAMP, built.max_timestamp)
        .set(
            c::BODY_SHA256,
            BinaryValue::B256(darkbot_bsc_record::sha256(built.body.as_bytes())),
        )
        .set(c::BODY_LEN, built.body.len() as u64)
        .set(c::BEFORE_CONNECT_MS, before_connect_ms)
        .set(c::CONNECTED_MS, connected_ms)
        .set_opt(c::WRITTEN_MS, written_ms)
        .set(c::ANSWER_MS, answered_ms)
        .set(c::PROVIDER_RESULT, provider_record(&provider));
    for tx in &built.txs {
        d.push(c::TX_HASH, BinaryValue::B256(tx.signed.hash.0));
    }
    for reason in refused {
        d.push(c::REFUSED, reason);
    }
    d
}

fn case_failure(case: &Case, outcome: &str, reason: &str) -> Draft {
    let mut d = Draft::new(&probe::case::SCHEMA);
    d.set(probe::case::CASE, case.id)
        .set(probe::case::OUTCOME, outcome)
        .set(probe::case::REASON, reason);
    d
}

fn report(plan_sha256: &str, results: &[Draft], status: &str) -> Result<Vec<u8>, String> {
    let hash: [u8; 32] = hex::decode(plan_sha256)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "invalid plan hash")?;
    let mut d = Draft::new(&probe::SCHEMA);
    d.set(probe::PLAN_SHA256, BinaryValue::B256(hash)).set(probe::STATUS, status)
        .set(probe::SCOPE, "submission-time acceptance only; building-time behavior, inclusion and whether 0.05 gwei suffices are not shown");
    for account in throwaway_accounts() {
        d.push(probe::ACCOUNTS, BinaryValue::Address(account.into_array()));
    }
    for result in results {
        d.push(probe::CASES, result.clone().into_nested());
    }
    d.encode().map_err(|e| e.to_string())
}

fn report_view(hash: &str, results: &[Draft], status: &str) -> String {
    report(hash, results, status)
        .and_then(|b| darkbot_bsc_record::decode(&probe::SCHEMA, &b).map_err(|e| e.to_string()))
        .map(|r| r.render())
        .unwrap_or_else(|e| format!("REPORT_ERROR={e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_report_round_trip_and_integrity() {
        let cases = vec![case_failure(&CASES[0], "NOT_SENT_TEST", "offline")];
        let bytes = report(&"ab".repeat(32), &cases, "IN_PROGRESS").unwrap();
        let decoded = darkbot_bsc_record::decode(&probe::SCHEMA, &bytes).unwrap();
        assert_eq!(decoded.nested_all(probe::CASES).unwrap().len(), 1);
        assert_eq!(decoded.text(probe::STATUS).unwrap(), "IN_PROGRESS");
        assert_eq!(
            bytes,
            report(&"ab".repeat(32), &cases, "IN_PROGRESS").unwrap()
        );
        assert!(darkbot_bsc_record::decode(&probe::SCHEMA, &bytes[..bytes.len() - 1]).is_err());
        let mut corrupt = bytes.clone();
        corrupt[25] ^= 1;
        assert!(darkbot_bsc_record::decode(&probe::SCHEMA, &corrupt).is_err());
        assert!(!decoded.render().contains("response_body"));
    }

    #[test]
    fn a_failed_evidence_write_stops_the_run_before_the_next_case() {
        for fail_after in 0..CASES.len() {
            let mut sent = Vec::new();
            let run = run_cases(
                CASES.len(),
                Duration::ZERO,
                |n| {
                    sent.push(n);
                    case_failure(&CASES[n], "NOT_SENT_TEST", "offline fixture")
                },
                |results| {
                    if results.len() > fail_after {
                        Err("disk full".to_string())
                    } else {
                        Ok(())
                    }
                },
            );
            let (results, e) = run.unwrap_err();
            assert_eq!(sent, (0..=fail_after).collect::<Vec<_>>());
            assert_eq!(results.len(), fail_after + 1);
            assert_eq!(e, "disk full");
        }
    }

    #[test]
    fn every_case_is_persisted_right_after_it_is_sent() {
        let mut events = Vec::new();
        let events_ref = std::cell::RefCell::new(&mut events);
        let run = run_cases(
            3,
            Duration::ZERO,
            |n| {
                events_ref.borrow_mut().push(format!("send {n}"));
                case_failure(&CASES[n], "NOT_SENT_TEST", "offline fixture")
            },
            |results| {
                events_ref
                    .borrow_mut()
                    .push(format!("persist {}", results.len()));
                Ok(())
            },
        );
        assert_eq!(run.unwrap().len(), 3);
        assert_eq!(
            events,
            [
                "send 0",
                "persist 1",
                "send 1",
                "persist 2",
                "send 2",
                "persist 3"
            ]
        );
    }
}

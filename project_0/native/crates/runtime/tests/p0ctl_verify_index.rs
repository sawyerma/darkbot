//! DEC-08 end to end over the real binary (each `p0ctl` its own process; loopback endpoints,
//! never Mainnet): `p0ctl verify --mode index` online with measured provider evidence — the
//! provider answers the first page with HTTP 429 (HISTORICAL_INCOMPLETE `RPC_RATE_LIMIT`, the
//! attempt kept), the resumed attempt verifies the window (INDEX_VERIFIED, exit 0, never
//! canonical); `--mode crosscheck` without a second qualified source is NOT_RUN (C-02); offline
//! without checkpoints both history reads are HISTORICAL_INCOMPLETE.
use darkbot_runner::analytics::journal::{self, source, Provenance, Writer};
use darkbot_runner::analytics::measurement::{
    journal as phase, persist_manifest, MeasurementPlan, Phase, NS,
};
use darkbot_runner::market_observer::rows;
use darkbot_runner::orchestration::provider_evidence::{ProviderEvidence, ProviderProof};
use darkbot_runner::run_layout::{RunLayout, RunPaths};
use serde_json::{json, Value};
use solana_sdk::signature::Signature;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const FIXTURE: &str = include_str!(
    "../../../../docs/evidence/AUDIT_20261005/2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf.json"
);

fn p0ctl(project: &Path, env: &[(&str, &str)], args: &[&str]) -> (i32, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_p0ctl"));
    c.env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());
    for (k, v) in env {
        c.env(k, v);
    }
    let out = c.arg("--project").arg(project).args(args).output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn variant(signature: &str, slot: u64, bt: i64) -> Value {
    let mut tx: Value = serde_json::from_str(FIXTURE).unwrap();
    tx["transaction"]["signatures"][0] = json!(signature);
    tx["slot"] = json!(slot);
    tx["blockTime"] = json!(bt);
    tx
}
fn row(signature: &str, slot: u64, bt: i64) -> Value {
    json!({"signature": signature, "slot": slot, "blockTime": bt, "err": null, "memo": null,
        "confirmationStatus": "finalized"})
}

/// The provider: the head page (after the end, the liquidation), the page before the start and
/// the liquidation; the next `limited` requests are answered `429 Too Many Requests`, the next
/// `limited_body` ones with a JSON-RPC error 429 in the body.
fn endpoint(
    start_s: i64,
    end_s: i64,
    f: String,
    limited: Arc<AtomicU64>,
    limited_body: Arc<AtomicU64>,
) -> String {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (after, before) = (
        Signature::new_unique().to_string(),
        Signature::new_unique().to_string(),
    );
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            if limited
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
            {
                let _ = write!(
                    stream,
                    "HTTP/1.1 429 Too Many Requests\r\nretry-after: 0\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                );
                continue;
            }
            let q: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            if limited_body
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
            {
                let answer = json!({"jsonrpc": "2.0", "id": q["id"],
                    "error": {"code": 429, "message": "Too many requests"}})
                .to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    answer.len(),
                    answer
                );
                continue;
            }
            let result = match q["method"].as_str() {
                Some("getSignaturesForAddress") => match q["params"][1]["before"].as_str() {
                    None => json!([row(&after, 2_000, end_s + 50), row(&f, 1_000, start_s + 10)]),
                    Some(b) if b == f => json!([row(&before, 900, start_s - 50)]),
                    Some(_) => json!([]),
                },
                Some("getTransaction") if q["params"][0].as_str() == Some(f.as_str()) => {
                    variant(&f, 1_000, start_s + 10)
                }
                _ => Value::Null,
            };
            let answer = json!({"jsonrpc": "2.0", "id": q["id"], "result": result}).to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                answer.len(),
                answer
            );
        }
    });
    url
}

#[test]
fn dec08_p0ctl_verifies_index_through_429_and_resume_and_crosscheck_is_not_run() {
    let d = std::env::temp_dir().join(format!("p0ctl_verify_index_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let root = d.join("run_x");
    let paths = RunPaths::new(&root, RunLayout::V2);
    paths.create().unwrap();
    let plan = MeasurementPlan::new(60).unwrap();
    let clock = darkbot_common::timing::Clock::new();
    persist_manifest(&paths, "run_x", "test", &plan, clock).unwrap();
    let window = plan.capture_window(clock, 1_000 * NS).unwrap();
    let (start_s, end_s) = (
        (window.capture_start_unix_ns / u128::from(NS)) as i64,
        (window.capture_cutoff_unix_ns / u128::from(NS)) as i64,
    );
    phase(&paths, clock, Phase::Capture(window)).unwrap();
    // The capture received the liquidation live with its receivership row.
    let f = Signature::new_unique().to_string();
    let tx = darkbot_solana::chain::decode_transaction(&variant(&f, 1_000, start_s + 10)).unwrap();
    let program =
        solana_sdk::pubkey::Pubkey::from_str_const(darkbot_project0::protocol::PROGRAM_ID_MAINNET);
    let receiver = darkbot_runner::market_observer::decode_receiver_events(
        tx.meta.as_ref().unwrap().log_messages.as_ref().unwrap(),
        &program,
    )
    .events[0]
        .clone();
    std::fs::create_dir_all(paths.market_dir()).unwrap();
    let mut w = Writer::create(
        &paths
            .market_dir()
            .join(darkbot_runner::market_observer::MARKET_JOURNAL),
    )
    .unwrap();
    let market = Provenance {
        source: source::MARKET,
        ..Default::default()
    };
    w.append_typed(
        market.clone(),
        &rows::MarketTransaction {
            signature: f.clone(),
            slot: 1_000,
            origin: rows::TxOrigin::TransactionSubscribe {
                observed_unix_s: (start_s + 10) as u64,
                t_rx_ns: 1,
                tx: Ok(tx),
            },
        },
    )
    .unwrap();
    w.append_typed(
        market,
        &rows::MarketLiquidation::Receiver(Box::new(rows::ReceiverRow {
            signature: f.clone(),
            slot: 1_000,
            t_rx_ns: Some(1),
            observed_unix_s: (start_s + 10) as u64,
            event: receiver,
            enrichment: Default::default(),
            enrichment_status: "COMPLETE".into(),
        })),
    )
    .unwrap();
    w.seal(true).unwrap();
    let run = root.to_str().unwrap();

    // Offline without checkpoints: both history reads are incomplete (exit 2), nothing is read.
    for mode in ["index", "crosscheck"] {
        let (code, out, err) = p0ctl(
            &d,
            &[],
            &["verify", "--run", run, "--mode", mode, "--offline"],
        );
        assert_eq!(code, 2, "{mode}: {out}{err}");
        assert!(
            out.contains("status=HISTORICAL_INCOMPLETE"),
            "{mode}: {out}"
        );
    }
    // Online without measured provider evidence: refused before any chain RPC.
    let (code, _, err) = p0ctl(
        &d,
        &[],
        &[
            "verify",
            "--run",
            run,
            "--mode",
            "index",
            "--resume-verification",
        ],
    );
    assert_eq!(code, 2);
    assert!(err.contains("STRICT_PROVIDER_EVIDENCE_REQUIRED"), "{err}");

    // The measured FREE_ONLY archive evidence of the loopback endpoint.
    // As many 429s as the configured attempts (`rpc_attempts`) allow for one read.
    let attempts = darkbot_solana::runtime_config::Limits::default().rpc_attempts;
    let limited = Arc::new(AtomicU64::new(attempts));
    let limited_body = Arc::new(AtomicU64::new(0));
    let url = endpoint(
        start_s,
        end_s,
        f.clone(),
        limited.clone(),
        limited_body.clone(),
    );
    let sha = |b: &[u8]| journal::hex(&solana_sdk::hash::hash(b).to_bytes());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let proof = |name: &str, available_calls: u64| {
        let path = d.join(name);
        journal::write_typed_value(
            &path,
            source::FE01,
            &ProviderEvidence {
                providers: vec![ProviderProof {
                    policy: "FREE_ONLY".into(),
                    passed: true,
                    provider_binding: sha(url.as_bytes()),
                    measured_at_unix_s: now - 1.0,
                    evidence_sha256: Some(sha(b"loopback")),
                    allowed_rps: 1_000.0,
                    available_calls,
                }],
                wss_provider_binding: None,
                wss_free_only: false,
                archive_qualified: true,
                source_sha256: sha(b"measured"),
            },
        )
        .unwrap();
        path
    };
    let evidence = proof("provider.p0j", 1_000_000);
    // A provider that proved fewer requests than the method's budget is refused before any RPC.
    let small = proof("small.p0j", 10);
    let (code, _, err) = p0ctl(
        &d,
        &[("P0_RPC_URL", url.as_str())],
        &[
            "verify",
            "--run",
            run,
            "--mode",
            "index",
            "--provider-evidence",
            small.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 2);
    assert!(
        err.contains("PROVIDER_CAPACITY_BELOW_METHOD_BUDGET"),
        "{err}"
    );
    let env = [("P0_RPC_URL", url.as_str())];
    let online = |resume: bool| {
        let mut a = vec![
            "verify",
            "--run",
            run,
            "--mode",
            "index",
            "--provider-evidence",
            evidence.to_str().unwrap(),
            "--verify-rps",
            "100",
        ];
        if resume {
            a.push("--resume-verification");
        }
        p0ctl(&d, &env, &a)
    };
    // 429 on every attempt of the first page: incomplete with its reason, resumable.
    let (code, out, err) = online(false);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("status=HISTORICAL_INCOMPLETE reason=RPC_RATE_LIMIT"),
        "{out}{err}"
    );
    assert_eq!(limited.load(Ordering::SeqCst), 0, "every attempt was made");
    // One 429 on the resume — a JSON-RPC rate limit in the body — is retried within the attempt
    // (after the provider's cooldown).
    limited_body.store(1, Ordering::SeqCst);
    // Resumed: the window verifies; never canonical.
    let (code, out, err) = online(true);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("status=INDEX_VERIFIED reason=none"), "{out}");
    assert!(out.contains("CANONICAL_VERIFIED=false"), "{out}");
    assert!(out.contains("comparison MATCH=2"), "{out}");
    assert_eq!(limited_body.load(Ordering::SeqCst), 0, "the retried 429");
    // Crosscheck online with one configured source: NOT_RUN (C-02), no chain read.
    let (code, out, err) = p0ctl(
        &d,
        &env,
        &[
            "verify",
            "--run",
            run,
            "--mode",
            "crosscheck",
            "--provider-evidence",
            evidence.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("status=NOT_RUN reason=CROSSCHECK_SECOND_PROVIDER_MISSING"),
        "{out}{err}"
    );
    std::fs::remove_dir_all(&d).unwrap();
}

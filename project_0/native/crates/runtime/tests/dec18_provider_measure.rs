//! DEC-18 §3 (P0-022, C-11) over the real `p0ctl`: the provider pre-measurement against a
//! loopback RPC/WSS pair of the registry (a query credential from the test store). The plan
//! reads nothing; its run reads each probe once inside the bounds (one WSS connection, one
//! subscription at a time, every request with the bound credential), writes the measurement and
//! its PROVIDER_EVIDENCE, and the BOOT audit admits exactly the modes it proved. Refused before
//! any read: another plan's token, a second run, a credential rotated since the plan. A 429, an
//! unsupported method, a subscription without events and an aborted run are recorded as what
//! they are, never as a pass. A debug build only (the test store).
#![cfg(all(debug_assertions, feature = "crate_health"))]
use darkbot_runner::orchestration::{provider_evidence, provider_measure as pm};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BLOCKHASH: &str = "4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn";
const BANK: &str = "2s37akK2eyBbp8DZgCm7RtsaEz8eJP3Nxd4urLHQv7yB";
const SIGNATURE: &str =
    "5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW";

/// How the loopback provider answers.
#[derive(Clone, Default)]
struct Behaviour {
    /// Every request is answered 429 (Retry-After 1).
    rate_limited: bool,
    /// This RPC method is unknown to the provider (-32601).
    unsupported: Option<&'static str>,
    /// This subscription is acknowledged but never notified.
    silent: Option<&'static str>,
    /// Each RPC answer waits this long (an abort window).
    delay_ms: u64,
    /// Every RPC request is answered 401.
    unauthorized: bool,
    /// This subscription is never acknowledged.
    unacknowledged: Option<&'static str>,
}

/// What the loopback provider saw.
#[derive(Default)]
struct Seen {
    rpc: Vec<(String, String)>,
    /// When each RPC request arrived.
    rpc_at: Vec<std::time::Instant>,
    /// The request target of each WSS connection (its credential).
    ws_targets: Vec<String>,
    ws_connections: usize,
    subscribed: Vec<String>,
    active_max: usize,
}

fn result_for(method: &str) -> String {
    match method {
        "getGenesisHash" => format!("\"{BLOCKHASH}\""),
        "getVersion" => r#"{"solana-core":"2.2.0"}"#.into(),
        "getHealth" => "\"ok\"".into(),
        "getSlot" => "100".into(),
        "getMultipleAccounts" | "getAccountInfo" => r#"{"context":{"slot":1},"value":null}"#.into(),
        "getProgramAccounts" => format!(
            r#"{{"context":{{"slot":1}},"value":[{{"pubkey":"{BANK}","account":{{"data":["","base64+zstd"],"executable":false,"lamports":1,"owner":"11111111111111111111111111111111","rentEpoch":0,"space":0}}}}]}}"#
        ),
        "getLatestBlockhash" => format!(
            r#"{{"context":{{"slot":1}},"value":{{"blockhash":"{BLOCKHASH}","lastValidBlockHeight":10}}}}"#
        ),
        "getFeeForMessage" => r#"{"context":{"slot":1},"value":5000}"#.into(),
        "simulateTransaction" => {
            r#"{"context":{"slot":1},"value":{"err":"AccountNotFound","logs":[],"accounts":null,"unitsConsumed":0,"returnData":null}}"#.into()
        }
        "getSignaturesForAddress" => format!(
            r#"[{{"signature":"{SIGNATURE}","slot":1,"err":null,"memo":null,"blockTime":null,"confirmationStatus":"confirmed"}}]"#
        ),
        _ => "null".into(),
    }
}

/// A loopback JSON-RPC provider: records each request's method and request line.
fn rpc(b: Behaviour, seen: Arc<Mutex<Seen>>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let (b, seen) = (b.clone(), seen.clone());
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    let mut length = 0usize;
                    loop {
                        let mut h = String::new();
                        if reader.read_line(&mut h).unwrap_or(0) == 0 {
                            return;
                        }
                        if h == "\r\n" {
                            break;
                        }
                        if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                            length = v.trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0; length];
                    if reader.read_exact(&mut body).is_err() {
                        return;
                    }
                    let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    let method = request["method"].as_str().unwrap_or_default().to_string();
                    let mut s = seen.lock().unwrap();
                    s.rpc.push((method.clone(), line.trim().to_string()));
                    s.rpc_at.push(std::time::Instant::now());
                    drop(s);
                    std::thread::sleep(Duration::from_millis(b.delay_ms));
                    let (status, reply) = if b.unauthorized {
                        ("401 Unauthorized", "{}".to_string())
                    } else if b.rate_limited {
                        ("429 Too Many Requests\r\nretry-after: 1", "{}".to_string())
                    } else if b.unsupported == Some(method.as_str()) {
                        ("200 OK", r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#.to_string())
                    } else {
                        (
                            "200 OK",
                            format!(
                                r#"{{"jsonrpc":"2.0","id":1,"result":{}}}"#,
                                result_for(&method)
                            ),
                        )
                    };
                    let out = format!(
                        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{reply}",
                        reply.len()
                    );
                    if reader.get_mut().write_all(out.as_bytes()).is_err() {
                        return;
                    }
                }
            });
        }
    });
    url
}

/// A loopback WSS provider: acknowledges each subscription, notifies it once (unless silent),
/// confirms each unsubscribe; counts connections and the most subscriptions active at once.
fn wss(b: Behaviour, seen: Arc<Mutex<Seen>>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let (b, seen) = (b.clone(), seen.clone());
            std::thread::spawn(move || {
                let target = Arc::new(Mutex::new(String::new()));
                let into = target.clone();
                let accepted = tungstenite::accept_hdr(
                    stream,
                    move |req: &tungstenite::handshake::server::Request, res| {
                        *into.lock().unwrap() = req.uri().to_string();
                        Ok(res)
                    },
                );
                let Ok(mut ws) = accepted else {
                    return;
                };
                seen.lock()
                    .unwrap()
                    .ws_targets
                    .push(target.lock().unwrap().clone());
                seen.lock().unwrap().ws_connections += 1;
                let mut active: Vec<u64> = Vec::new();
                let mut next = 7u64;
                while let Ok(m) = ws.read() {
                    let tungstenite::Message::Text(t) = m else {
                        if m.is_close() {
                            return;
                        }
                        continue;
                    };
                    let v: serde_json::Value = serde_json::from_str(&t).unwrap();
                    let (id, method) = (
                        v["id"].clone(),
                        v["method"].as_str().unwrap_or("").to_string(),
                    );
                    if method.ends_with("Unsubscribe") {
                        let sub = v["params"][0].as_u64().unwrap();
                        active.retain(|s| *s != sub);
                        let _ = ws.send(tungstenite::Message::Text(
                            serde_json::json!({"jsonrpc":"2.0","result":true,"id":id}).to_string(),
                        ));
                        continue;
                    }
                    let mut s = seen.lock().unwrap();
                    s.subscribed.push(method.clone());
                    if b.unacknowledged == Some(method.as_str()) {
                        continue;
                    }
                    next += 1;
                    active.push(next);
                    s.active_max = s.active_max.max(active.len());
                    drop(s);
                    let _ = ws.send(tungstenite::Message::Text(
                        serde_json::json!({"jsonrpc":"2.0","result":next,"id":id}).to_string(),
                    ));
                    if b.silent != Some(method.as_str()) {
                        let note = method.replace("Subscribe", "Notification");
                        let _ = ws.send(tungstenite::Message::Text(
                            serde_json::json!({"jsonrpc":"2.0","method":note,"params":{"result":{"slot":1},"subscription":next}}).to_string(),
                        ));
                    }
                }
            });
        }
    });
    url
}

/// A project whose registry selects the loopback pair (a query credential in the test store).
struct Project {
    base: PathBuf,
    project: PathBuf,
    store: PathBuf,
    seen: Arc<Mutex<Seen>>,
}

fn run(
    p: &Project,
    bin: &str,
    args: &[&str],
    env: &[(&str, &str)],
    stdin: Option<&[u8]>,
) -> (bool, String) {
    let mut c = Command::new(bin);
    c.args(args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("P0_PROJECT_DIR", &p.project)
        .env(darkbot_solana::providers::TEST_STORE_ENV, &p.store)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        c.env(k, v);
    }
    let mut child = c.spawn().unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        if let Some(bytes) = stdin {
            input.write_all(bytes).unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

fn ctl(p: &Project, args: &[&str], stdin: Option<&[u8]>) -> String {
    let mut all = vec!["--project", p.project.to_str().unwrap(), "provider"];
    all.extend_from_slice(args);
    let (ok, text) = run(p, env!("CARGO_BIN_EXE_p0ctl"), &all, &[], stdin);
    assert!(ok, "{args:?}: {text}");
    text
}

fn setup(tag: &str, b: Behaviour) -> Project {
    let base = std::env::temp_dir().join(format!("p0_022_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (project, store) = (base.join("project"), base.join("keychain"));
    std::fs::create_dir_all(&project).unwrap();
    let seen = Arc::new(Mutex::new(Seen::default()));
    let p = Project {
        base,
        project,
        store,
        seen: seen.clone(),
    };
    let (http, ws) = (rpc(b.clone(), seen.clone()), wss(b, seen));
    for (id, kind, endpoint) in [("loop-rpc", "rpc", &http), ("loop-wss", "wss", &ws)] {
        ctl(
            &p,
            &[
                "add",
                "--id",
                id,
                "--kind",
                kind,
                "--endpoint",
                endpoint,
                "--auth",
                "query:api-key",
                "--credential",
                "loop",
                "--plan",
                "free",
                "--entitlement",
                "loopback-free",
            ],
            None,
        );
    }
    ctl(
        &p,
        &["credential", "set", "--credential", "loop"],
        Some(b"MEASURE-SECRET-1\n"),
    );
    for (kind, id) in [("rpc", "loop-rpc"), ("wss", "loop-wss")] {
        ctl(&p, &["select", "--kind", kind, "--id", id], None);
    }
    p
}

/// `measure plan`: its id and token (it reads nothing).
fn plan(p: &Project, env: &[(&str, &str)]) -> (bool, String, String, String) {
    let (ok, text) = run(
        p,
        env!("CARGO_BIN_EXE_p0ctl"),
        &[
            "--project",
            p.project.to_str().unwrap(),
            "provider",
            "measure",
            "plan",
        ],
        env,
        None,
    );
    let field = |k: &str| {
        text.split_whitespace()
            .find_map(|w| w.strip_prefix(k))
            .unwrap_or_default()
            .to_string()
    };
    (ok, field("id="), field("token="), text)
}

fn measure(p: &Project, plan_id: &str, token: &str) -> (bool, String) {
    measure_with(p, plan_id, token, &[])
}

fn measure_with(p: &Project, plan_id: &str, token: &str, env: &[(&str, &str)]) -> (bool, String) {
    run(
        p,
        env!("CARGO_BIN_EXE_p0ctl"),
        &[
            "--project",
            p.project.to_str().unwrap(),
            "provider",
            "measure",
            "run",
            "--plan",
            plan_id,
            "--approve",
            token,
        ],
        env,
        None,
    )
}

fn evidence_id(text: &str) -> String {
    text.lines()
        .find_map(|l| l.strip_prefix("PROVIDER_EVIDENCE_ID "))
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or_else(|| panic!("{text}"))
        .to_string()
}

fn evidence(p: &Project, text: &str) -> provider_evidence::ProviderEvidence {
    provider_evidence::load(&p.project, &evidence_id(text))
        .unwrap()
        .1
}

/// The BOOT audit's admission of the evidence `id` for a flow of `mode` (its own path, before any
/// read), the run's rate `rps`.
fn admit(p: &Project, id: &str, mode: &str, rps: &str) -> Result<(), String> {
    use darkbot_runner::orchestration::{boot_network, provider_registry};
    let get = |k: &str| (k == "P0_RPC_RPS").then(|| rps.to_string());
    let mut r = darkbot_solana::runtime_config::Resolved::parse(&[], &get).unwrap();
    provider_registry::bind(&p.project, &mut r).unwrap();
    let path = provider_evidence::resolve(&p.project, id)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    boot_network::admit_evidence(&p.project, &r.config, &path, Some(id), mode, false, now)
        .map(|_| ())
}

#[test]
fn p0_022_a_measurement_proves_each_read_once_and_admits_exactly_its_modes() {
    let p = setup("pass", Behaviour::default());
    let (ok, plan_id, token, text) = plan(&p, &[]);
    assert!(ok && plan_id.len() == 16 && token.len() == 16, "{text}");
    assert!(
        p.seen.lock().unwrap().rpc.is_empty(),
        "the plan reads nothing"
    );
    assert!(text.contains("probe simulateTransaction") && text.contains("UNSIGNED_SIMULATION"));
    // Another plan's token is refused before any read; the plan stays unused.
    let (ok, text) = measure(&p, &plan_id, "0000000000000000");
    assert!(!ok && text.contains("MEASURE_APPROVAL_MISMATCH"), "{text}");
    assert!(p.seen.lock().unwrap().rpc.is_empty());

    let (ok, text) = measure(&p, &plan_id, &token);
    assert!(ok, "{text}");
    let seen = p.seen.lock().unwrap();
    let methods: Vec<&str> = seen.rpc.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(methods.len(), 12, "each RPC method once: {methods:?}");
    let mut unique = methods.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 12);
    assert!(!methods.contains(&"sendTransaction"));
    assert!(
        seen.rpc
            .iter()
            .all(|(_, l)| l.contains("?api-key=MEASURE-SECRET-1 ")),
        "every request carries the bound credential"
    );
    assert_eq!(seen.ws_connections, 1, "one WSS connection");
    assert_eq!(seen.active_max, 1, "one subscription at a time");
    assert_eq!(seen.subscribed.len(), 6, "{:?}", seen.subscribed);
    drop(seen);
    assert!(!text.contains("MEASURE-SECRET"), "{text}");
    for mode in ["boot", "econ4h", "c0-mo"] {
        assert!(text.contains(&format!("mode {mode} SUPPORTED")), "{text}");
    }
    let e = evidence(&p, &text);
    let proof = &e.providers[0];
    assert!(proof.passed && e.wss_free_only && !e.archive_qualified);
    assert_eq!(
        (proof.allowed_rps, proof.available_calls),
        (1.0, 12),
        "measured, never declared"
    );
    let seen = p.seen.lock().unwrap();
    assert!(
        seen.rpc_at
            .windows(2)
            .all(|w| w[1] - w[0] >= Duration::from_millis(900)),
        "one request per second"
    );
    assert!(
        seen.ws_targets
            .iter()
            .all(|t| t.contains("api-key=MEASURE-SECRET-1")),
        "the WSS connection carries the bound credential"
    );
    drop(seen);
    // The BOOT audit's admission (its own path): every measured mode at the measured rate; a run
    // at a higher rate, a mode the measurement does not cover and a record not derived from the
    // measurement are refused.
    let id = evidence_id(&text);
    for mode in ["econ4h", "market"] {
        admit(&p, &id, mode, "1").unwrap();
    }
    assert!(admit(&p, &id, "econ4h", "5")
        .unwrap_err()
        .starts_with("FREE_ONLY_PROVIDER_CAPACITY_UNQUALIFIED"));
    assert!(admit(&p, &id, "shadow", "1")
        .unwrap_err()
        .starts_with("PROVIDER_MEASUREMENT_MODE_UNMEASURED"));
    let mut forged = e.clone();
    forged.providers[0].allowed_rps = 50.0;
    let file = p.base.join("forged.p0j");
    darkbot_runner::analytics::journal::create_typed_value(
        &file,
        darkbot_runner::analytics::journal::source::PROVIDER_MEASUREMENT,
        &forged,
    )
    .unwrap();
    let forged = provider_evidence::import(&p.project, &file).unwrap();
    assert!(admit(&p, &forged, "econ4h", "1")
        .unwrap_err()
        .starts_with("PROVIDER_EVIDENCE_NOT_DERIVED"));
    // A plan runs once: a second run (a restart) is refused before any read.
    let before = p.seen.lock().unwrap().rpc.len();
    let (ok, text) = measure(&p, &plan_id, &token);
    assert!(!ok && text.contains("MEASURE_PLAN_USED"), "{text}");
    assert_eq!(p.seen.lock().unwrap().rpc.len(), before);
    // A changed measurement record is no evidence.
    let results = p.project.join(pm::STORE).join("results");
    let record = std::fs::read_dir(&results)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|f| {
            f.extension().is_some_and(|x| x == "p0j")
                && !f.file_name().unwrap().to_string_lossy().starts_with('.')
        })
        .unwrap();
    let mut bytes = std::fs::read(&record).unwrap();
    let at = bytes.len() / 2;
    bytes[at] ^= 1;
    std::fs::write(&record, bytes).unwrap();
    assert_eq!(
        admit(&p, &id, "econ4h", "1").unwrap_err(),
        "PROVIDER_MEASUREMENT_CHANGED"
    );
    let _ = std::fs::remove_dir_all(&p.base);
}

#[test]
fn p0_022_an_unsupported_method_and_a_silent_subscription_block_only_their_mode() {
    let p = setup(
        "partial",
        Behaviour {
            unsupported: Some("getFeeForMessage"),
            silent: Some("rootSubscribe"),
            ..Default::default()
        },
    );
    let (_, plan_id, token, _) = plan(&p, &[("P0_MEASURE_SUBSCRIPTION_SECONDS", "1")]);
    // The plan bound its bounds: a measurement under other bounds is another plan.
    let (ok, text) = measure(&p, &plan_id, &token);
    assert!(
        !ok && text.contains("MEASURE_PLAN_DRIFT: the configuration"),
        "{text}"
    );
    let (_, plan_id, token, _) = plan(&p, &[]);
    let (ok, text) = run(
        &p,
        env!("CARGO_BIN_EXE_p0ctl"),
        &[
            "--project",
            p.project.to_str().unwrap(),
            "provider",
            "measure",
            "run",
            "--plan",
            &plan_id,
            "--approve",
            &token,
        ],
        &[],
        None,
    );
    assert!(!ok, "a blocked mode is no full pass: {text}");
    assert!(
        text.contains("read getFeeForMessage METHOD_UNSUPPORTED"),
        "{text}"
    );
    assert!(
        text.contains("read rootSubscribe@- NOT_PROVEN NO_EVENT_IN_WINDOW"),
        "{text}"
    );
    assert!(
        text.contains("mode boot SUPPORTED") && text.contains("mode c0-mo SUPPORTED"),
        "{text}"
    );
    assert!(text.contains("mode econ4h METHOD_UNSUPPORTED"), "{text}");
    let id = evidence_id(&text);
    admit(&p, &id, "market", "1").unwrap();
    assert!(admit(&p, &id, "econ4h", "1")
        .unwrap_err()
        .starts_with("PROVIDER_MEASUREMENT_MODE_BLOCKED: econ4h METHOD_UNSUPPORTED"));
    let _ = std::fs::remove_dir_all(&p.base);
}

#[test]
fn p0_022_a_rate_limit_qualifies_no_rate_and_a_rotated_credential_needs_a_new_plan() {
    let p = setup(
        "limited",
        Behaviour {
            rate_limited: true,
            ..Default::default()
        },
    );
    // A credential rotated since the plan: refused before any read.
    let (_, plan_id, token, _) = plan(&p, &[]);
    ctl(
        &p,
        &["credential", "rotate", "--credential", "loop"],
        Some(b"MEASURE-SECRET-2\n"),
    );
    let (ok, text) = measure(&p, &plan_id, &token);
    assert!(
        !ok && text.contains("MEASURE_PLAN_DRIFT: the provider pair or its credential revisions"),
        "{text}"
    );
    assert!(p.seen.lock().unwrap().rpc.is_empty());
    let (_, plan_id, token, _) = plan(&p, &[]);
    let (ok, text) = measure(&p, &plan_id, &token);
    assert!(!ok, "{text}");
    assert!(text.contains("rate_limited=true allowed_rps=0"), "{text}");
    assert!(text.contains("BLOCKED RPC_RATE_LIMIT"), "{text}");
    let e = evidence(&p, &text);
    assert!(!e.providers[0].passed && e.providers[0].allowed_rps == 0.0);
    assert!(p
        .seen
        .lock()
        .unwrap()
        .rpc
        .iter()
        .all(|(_, l)| l.contains("?api-key=MEASURE-SECRET-2 ")));
    let _ = std::fs::remove_dir_all(&p.base);
}

#[test]
fn p0_022_an_aborted_run_leaves_no_evidence_and_its_plan_is_used() {
    let p = setup(
        "abort",
        Behaviour {
            delay_ms: 400,
            ..Default::default()
        },
    );
    let (_, plan_id, token, _) = plan(&p, &[]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_p0ctl"))
        .args([
            "--project",
            p.project.to_str().unwrap(),
            "provider",
            "measure",
            "run",
            "--plan",
            &plan_id,
            "--approve",
            &token,
        ])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("P0_PROJECT_DIR", &p.project)
        .env(darkbot_solana::providers::TEST_STORE_ENV, &p.store)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while p.seen.lock().unwrap().rpc.len() < 2 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    child.kill().unwrap();
    let _ = child.wait();
    let results = p.project.join(pm::STORE).join("results");
    assert!(
        std::fs::read_dir(&results).map_or(0, |r| r.count()) == 0,
        "no measurement of an aborted run"
    );
    assert!(
        std::fs::read_dir(p.project.join(provider_evidence::STORE)).map_or(0, |r| r.count()) == 0
    );
    let (ok, text) = measure(&p, &plan_id, &token);
    assert!(!ok && text.contains("MEASURE_PLAN_USED"), "{text}");
    let _ = std::fs::remove_dir_all(&p.base);
}

#[test]
fn p0_022_a_plan_beyond_the_approved_bounds_is_refused() {
    let p = setup("bounds", Behaviour::default());
    let (ok, _, _, text) = plan(&p, &[("P0_MEASURE_SUBSCRIPTION_TYPES", "4")]);
    assert!(
        !ok && text.contains("MEASURE_PLAN_EXCEEDS_BOUNDS"),
        "{text}"
    );
    // Above the approval: refused as a parameter.
    for (k, v) in [
        ("P0_MEASURE_RPS", "2"),
        ("P0_MEASURE_SECONDS", "121"),
        ("P0_MEASURE_SUBSCRIPTION_TYPES", "6"),
    ] {
        let (ok, _, _, text) = plan(&p, &[(k, v)]);
        assert!(!ok, "{k}={v}: {text}");
    }
    assert!(p.seen.lock().unwrap().rpc.is_empty());
    let _ = Path::new(&p.base);
    let _ = std::fs::remove_dir_all(&p.base);
}

#[test]
fn p0_022_a_refused_authorization_and_an_unacknowledged_subscription_stop_what_they_must() {
    let p = setup(
        "refused",
        Behaviour {
            unauthorized: true,
            unacknowledged: Some("programSubscribe"),
            ..Default::default()
        },
    );
    let env = [("P0_MEASURE_SUBSCRIPTION_SECONDS", "2")];
    let (_, plan_id, token, _) = plan(&p, &env);
    let (ok, text) = measure_with(&p, &plan_id, &token, &env);
    assert!(!ok, "{text}");
    assert!(
        text.contains("read getGenesisHash BLOCKED RPC_UNAUTHORIZED"),
        "{text}"
    );
    assert!(
        text.contains("mode boot BLOCKED RPC_UNAUTHORIZED"),
        "{text}"
    );
    assert!(
        text.contains("read programSubscribe@confirmed NOT_PROVEN NO_ACK_IN_WINDOW"),
        "{text}"
    );
    assert!(
        text.contains("BLOCKED WSS_STOPPED: an unacknowledged subscription"),
        "{text}"
    );
    let seen = p.seen.lock().unwrap();
    assert_eq!(
        seen.subscribed.last().map(String::as_str),
        Some("programSubscribe"),
        "no subscription after an unacknowledged one: {:?}",
        seen.subscribed
    );
    drop(seen);
    let e = evidence(&p, &text);
    // No RPC read passed; the BOOT subscriptions (slot, logs) were proven before the stop.
    assert!(!e.providers[0].passed && e.wss_free_only);
    let _ = std::fs::remove_dir_all(&p.base);
}

//! DEC-18 §1.2/§1.3.6 end to end over the real binaries: a provider of the registry
//! (`p0ctl provider …`, authenticating by a query parameter) is the endpoint of a read-only
//! FE-01 observation (`p0-fe-qualify --fe-mode smoke`); the credential is read once from the
//! credential store (the `test-support` directory store; the production build has the macOS
//! keychain alone) and applied by the transport only — the provider sees it on the wire, no
//! output, journal or report holds it. A rotation is effective at the next start; a revision
//! whose item is gone refuses the start before any read; a remote endpoint outside the registry
//! is refused. A debug build only (the test store does not exist in a release build).
#![cfg(all(debug_assertions, feature = "crate_health"))]
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// A loopback JSON-RPC endpoint: records every request line, answers a JSON-RPC error.
fn rpc() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let log = log.clone();
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
                    log.lock().unwrap().push(line.trim().to_string());
                    let reply =
                        r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"loopback"}}"#;
                    let out = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{reply}",
                        reply.len()
                    );
                    if reader.get_mut().write_all(out.as_bytes()).is_err() {
                        return;
                    }
                }
            });
        }
    });
    (url, seen)
}

fn run(
    bin: &str,
    project: &Path,
    store: &Path,
    args: &[&str],
    stdin: Option<&[u8]>,
) -> (bool, String) {
    let mut child = Command::new(bin)
        .args(args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("P0_PROJECT_DIR", project)
        .env(darkbot_solana::providers::TEST_STORE_ENV, store)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
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

/// Every byte under `dir`.
fn bytes_under(dir: &Path) -> Vec<u8> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(bytes_under(&p));
        } else {
            out.extend(std::fs::read(&p).unwrap());
        }
    }
    out
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

#[test]
fn dec18_the_transport_applies_the_bound_credential_and_nothing_else_holds_it() {
    let base = std::env::temp_dir().join(format!("p0_dec18_transport_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (project, store) = (base.join("project"), base.join("keychain"));
    std::fs::create_dir_all(&project).unwrap();
    let (url, seen) = rpc();
    let ctl = |args: &[&str], stdin: Option<&[u8]>| {
        let mut all = vec!["--project", project.to_str().unwrap(), "provider"];
        all.extend_from_slice(args);
        let (ok, text) = run(env!("CARGO_BIN_EXE_p0ctl"), &project, &store, &all, stdin);
        assert!(ok, "{args:?}: {text}");
        text
    };
    ctl(
        &[
            "add",
            "--id",
            "loop-rpc",
            "--kind",
            "rpc",
            "--endpoint",
            &url,
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
    ctl(
        &["credential", "set", "--credential", "loop"],
        Some(b"LOOP-SECRET-1\n"),
    );
    ctl(&["select", "--kind", "rpc", "--id", "loop-rpc"], None);
    let observe = |n: u32| {
        let output = base.join(format!("smoke-{n}"));
        let (_, text) = run(
            env!("CARGO_BIN_EXE_p0-fe-qualify"),
            &project,
            &store,
            &[
                "--fe-mode",
                "smoke",
                "--execute",
                "--output",
                output.to_str().unwrap(),
            ],
            None,
        );
        (output, text)
    };

    let (first, text) = observe(1);
    let lines = seen.lock().unwrap().clone();
    assert!(
        !lines.is_empty(),
        "the observation read through the provider: {text}"
    );
    assert!(
        lines.iter().all(|l| l.contains("?api-key=LOOP-SECRET-1 ")),
        "every request carries the bound credential: {lines:?}"
    );
    let evidence = bytes_under(&first);
    assert!(!text.contains("LOOP-SECRET") && !contains(&evidence, "LOOP-SECRET"));
    assert!(
        contains(&evidence, "loop-rpc"),
        "the evidence names the bound provider"
    );
    assert!(
        !contains(&evidence, &url),
        "the evidence binds the provider, never its endpoint"
    );

    // A rotation is effective at the next start, its own revision.
    ctl(
        &["credential", "rotate", "--credential", "loop"],
        Some(b"LOOP-SECRET-2\n"),
    );
    let before = seen.lock().unwrap().len();
    let (second, text) = observe(2);
    let lines = seen.lock().unwrap()[before..].to_vec();
    assert!(!lines.is_empty(), "{text}");
    assert!(
        lines.iter().all(|l| l.contains("?api-key=LOOP-SECRET-2 ")),
        "{lines:?}"
    );
    assert!(!contains(&bytes_under(&second), "LOOP-SECRET"));

    // The bound revision's item gone from the store: refused before any read.
    for e in std::fs::read_dir(&store).unwrap() {
        std::fs::remove_file(e.unwrap().path()).unwrap();
    }
    let before = seen.lock().unwrap().len();
    let (_, text) = observe(3);
    assert!(text.contains("CREDENTIAL_UNAVAILABLE"), "{text}");
    assert_eq!(
        seen.lock().unwrap().len(),
        before,
        "no read without the bound revision"
    );

    // A remote endpoint outside the registry is refused, never echoed.
    let output = base.join("smoke-remote");
    let mut child = Command::new(env!("CARGO_BIN_EXE_p0-fe-qualify"));
    child
        .args([
            "--fe-mode",
            "smoke",
            "--execute",
            "--output",
            output.to_str().unwrap(),
        ])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("P0_RPC_URL", "https://remote.invalid/?api-key=ENV-SECRET");
    let out = child.output().unwrap();
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && text.contains("PROVIDER_URL_QUERY"),
        "{text}"
    );
    assert!(
        !text.contains("ENV-SECRET") && !text.contains("remote.invalid"),
        "{text}"
    );
    // A key-free remote endpoint outside the registry is refused as well, never echoed.
    let out = Command::new(env!("CARGO_BIN_EXE_p0-fe-qualify"))
        .args([
            "--fe-mode",
            "smoke",
            "--execute",
            "--output",
            output.to_str().unwrap(),
        ])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("P0_RPC_URL", "https://remote.invalid")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && text.contains("PROVIDER_ENDPOINT_UNREGISTERED"),
        "{text}"
    );
    assert!(!text.contains("remote.invalid"), "{text}");
    let _ = std::fs::remove_dir_all(&base);
}

/// DEC-18 §3.4 (P0-023): the full FE-01 observation is admitted on the typed `PROVIDER_EVIDENCE`
/// record of the project's store, named by its id. The measured JSON flag is retired (no
/// fallback) and an id never imported is refused, each before any read; the record's
/// observation reads through the bound provider and binds the record's bytes.
#[test]
fn p0_023_the_full_observation_takes_the_typed_record_by_its_id() {
    use darkbot_runner::analytics::journal;
    use darkbot_runner::orchestration::{fe_qualification, provider_evidence, provider_registry};
    let base = std::env::temp_dir().join(format!("p0_023_full_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (project, store) = (base.join("project"), base.join("keychain"));
    std::fs::create_dir_all(&project).unwrap();
    let (url, seen) = rpc();
    let ctl = |args: &[&str], stdin: Option<&[u8]>| {
        let mut all = vec!["--project", project.to_str().unwrap(), "provider"];
        all.extend_from_slice(args);
        let (ok, text) = run(env!("CARGO_BIN_EXE_p0ctl"), &project, &store, &all, stdin);
        assert!(ok, "{args:?}: {text}");
    };
    ctl(
        &[
            "add",
            "--id",
            "loop-rpc",
            "--kind",
            "rpc",
            "--endpoint",
            &url,
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
    ctl(
        &["credential", "set", "--credential", "loop"],
        Some(b"LOOP-SECRET-1\n"),
    );
    ctl(&["select", "--kind", "rpc", "--id", "loop-rpc"], None);
    let registry = provider_registry::current(&project).unwrap();
    let binding = registry
        .binding_of(registry.provider("loop-rpc").unwrap())
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let record = base.join("measured.p0j");
    journal::create_typed_value(
        &record,
        journal::source::PROVIDER_MEASUREMENT,
        &provider_evidence::ProviderEvidence {
            providers: vec![provider_evidence::ProviderProof {
                policy: "FREE_ONLY".into(),
                passed: true,
                provider_binding: binding.clone(),
                measured_at_unix_s: now - 1.0,
                evidence_sha256: Some("a".repeat(64)),
                allowed_rps: 1e6,
                available_calls: u64::MAX,
            }],
            wss_provider_binding: None,
            wss_free_only: false,
            archive_qualified: false,
            source_sha256: "0".repeat(64),
        },
    )
    .unwrap();
    let id = provider_evidence::import(&project, &record).unwrap();
    // The same measurement as JSON: the shape the retired flag read.
    let json = base.join("measured.json");
    std::fs::write(
        &json,
        format!(
            r#"{{"policy":"FREE_ONLY","passed":true,"provider_binding":"{binding}","measured_at_unix_s":{now},"evidence_sha256":"{}","allowed_rps":1000000,"available_calls":1000000}}"#,
            "a".repeat(64)
        ),
    )
    .unwrap();
    let observe_with = |name: &str, evidence: &[&str], keychain: &Path, mode: &[&str]| {
        let output = base.join(name);
        let mut args = vec!["--fe-mode", "full"];
        args.extend_from_slice(mode);
        args.extend_from_slice(&["--output", output.to_str().unwrap()]);
        args.extend_from_slice(evidence);
        let (ok, text) = run(
            env!("CARGO_BIN_EXE_p0-fe-qualify"),
            &project,
            keychain,
            &args,
            None,
        );
        (ok, text, output)
    };

    // A replay reads no provider: it takes no provider evidence (review L2).
    let (ok, text, output) = observe_with(
        "replay",
        &["--provider-evidence", &id],
        &store,
        &["--replay", base.to_str().unwrap()],
    );
    assert!(
        !ok && text.contains("FE_REPLAY_TAKES_NO_PROVIDER_EVIDENCE"),
        "{text}"
    );
    assert!(!output.exists());
    // Every refusal comes before the credential store is opened (review L1): with no store,
    // each is still its own refusal, never CREDENTIAL_UNAVAILABLE.
    let no_store = base.join("no-keychain");
    for (name, evidence, code) in [
        (
            "json",
            vec!["--provider-qualification", json.to_str().unwrap()],
            "FE_PROVIDER_QUALIFICATION_RETIRED",
        ),
        (
            "unknown",
            vec!["--provider-evidence", "0123456789abcdef"],
            "BOOT_PROVIDER_EVIDENCE_UNKNOWN",
        ),
        (
            "path",
            vec!["--provider-evidence", record.to_str().unwrap()],
            "BOOT_PROVIDER_EVIDENCE_ID_INVALID",
        ),
        ("none", vec![], "FREE_ONLY_PROVIDER_QUALIFICATION_REQUIRED"),
    ] {
        let (ok, text, output) = observe_with(name, &evidence, &no_store, &["--plan"]);
        assert!(!ok && text.contains(code), "{name}: {text}");
        assert!(!output.exists(), "{name}: no evidence directory");
        assert!(seen.lock().unwrap().is_empty(), "{name}: no read");
    }

    // C-01 step 6: the plan reads nothing and opens no credential (no store here either).
    let (ok, text, output) = observe_with(
        "record",
        &["--provider-evidence", &id],
        &no_store,
        &["--plan"],
    );
    assert!(ok, "{text}");
    assert!(
        text.contains("FE01_PLAN id=") && text.contains("base64+zstd"),
        "{text}"
    );
    assert!(seen.lock().unwrap().is_empty(), "the plan reads nothing");
    let token = text
        .split_whitespace()
        .find_map(|w| w.strip_prefix("token="))
        .unwrap()
        .to_string();
    let execute = |dir: &Path, approve: &str, extra: &[&str]| {
        let mut args = vec![
            "--fe-mode",
            "full",
            "--execute",
            "--approve",
            approve,
            "--output",
            dir.to_str().unwrap(),
        ];
        args.extend_from_slice(extra);
        run(
            env!("CARGO_BIN_EXE_p0-fe-qualify"),
            &project,
            &store,
            &args,
            None,
        )
    };
    // Refused before any read: another token, no plan, the evidence or class at execution.
    for (dir, approve, extra, code) in [
        (
            output.as_path(),
            "0000000000000000",
            &[][..],
            "FE_PLAN_APPROVAL_MISMATCH",
        ),
        (
            base.join("no-plan").as_path(),
            token.as_str(),
            &[][..],
            "FE_PLAN_REQUIRED",
        ),
        (
            output.as_path(),
            token.as_str(),
            &["--provider-evidence", id.as_str()][..],
            "FE_PLAN_NAMES_EVIDENCE_AND_CLASS",
        ),
    ] {
        let (ok, text) = execute(dir, approve, extra);
        assert!(!ok && text.contains(code), "{code}: {text}");
        assert!(seen.lock().unwrap().is_empty(), "{code}: no read");
    }
    let (_, text) = execute(&output, &token, &[]);
    let lines = seen.lock().unwrap().clone();
    assert!(!lines.is_empty(), "the observation read: {text}");
    assert!(
        lines.iter().all(|l| l.contains("?api-key=LOOP-SECRET-1 ")),
        "{lines:?}"
    );
    let stored = project
        .join(provider_evidence::STORE)
        .join(format!("{id}.p0j"));
    let evidence = bytes_under(&output);
    assert!(
        contains(
            &evidence,
            &fe_qualification::sha(&std::fs::read(stored).unwrap())
        ),
        "the observation binds the record's bytes: {text}"
    );
    assert!(!contains(&evidence, "LOOP-SECRET"));
    // A plan runs once.
    let before = seen.lock().unwrap().len();
    let (ok, text) = execute(&output, &token, &[]);
    assert!(!ok && text.contains("FE_PLAN_USED"), "{text}");
    assert_eq!(seen.lock().unwrap().len(), before);
    // A rotated credential is another registry version: the plan made before it is refused.
    let (ok, text, drifted) = observe_with(
        "drifted",
        &["--provider-evidence", &id],
        &no_store,
        &["--plan"],
    );
    assert!(ok, "{text}");
    let token = text
        .split_whitespace()
        .find_map(|w| w.strip_prefix("token="))
        .unwrap()
        .to_string();
    ctl(
        &["credential", "rotate", "--credential", "loop"],
        Some(b"LOOP-SECRET-2\n"),
    );
    let (ok, text) = execute(&drifted, &token, &[]);
    assert!(
        !ok && text.contains("FE_PLAN_DRIFT: the provider"),
        "{text}"
    );
    assert_eq!(seen.lock().unwrap().len(), before);
    let _ = std::fs::remove_dir_all(&base);
}

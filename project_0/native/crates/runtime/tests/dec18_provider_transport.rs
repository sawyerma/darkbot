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

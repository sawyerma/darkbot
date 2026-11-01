//! DEC-18 (review H3, L1; P0-021): every blocking HTTP transport of a provider — the deployment
//! inspection, the verifier's history reads, Jupiter — applies the run's installed credential to
//! its bound endpoint (a query parameter in an origin-form target, a header), and none follows a
//! redirect (a credential never reaches a host the run did not bind). The one credential
//! installation of this test binary (a run's credentials are installed once).
use super::*;
use crate::execution::builder::p0::RouteProvider;
use darkbot_solana::providers::{
    activate_credentials, ActiveCredential, AuthKind, Secret, Zeroizing,
};
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};

/// A loopback HTTP endpoint answering `head` + `body`: every request's request line and headers.
fn server(head: String, body: &'static str) -> (String, Arc<Mutex<Vec<Vec<String>>>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut lines = vec![];
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) <= 2 {
                    break;
                }
                lines.push(line.trim_end().to_string());
            }
            log.lock().unwrap().push(lines);
            let _ = stream.write_all(
                format!(
                    "{head}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            );
        }
    });
    (url, seen)
}

#[test]
fn dec18_every_blocking_transport_applies_the_bound_credential_and_follows_no_redirect() {
    let ok = || {
        server(
            "HTTP/1.1 200 OK".into(),
            r#"{"jsonrpc":"2.0","id":1,"result":null}"#,
        )
    };
    let (deployment, deployment_seen) = ok();
    let (history, history_seen) = ok();
    let (jupiter, jupiter_seen) = server("HTTP/1.1 404 Not Found".into(), "{}");
    let (sink, sink_seen) = ok();
    let (redirecting, redirecting_seen) = server(
        format!("HTTP/1.1 307 Temporary Redirect\r\nlocation: {sink}/"),
        "{}",
    );
    let secret = |s: &str| Secret::new(Zeroizing::new(s.as_bytes().to_vec()));
    let query = |endpoint: &str, key: &str| ActiveCredential {
        endpoint: endpoint.into(),
        auth: AuthKind::Query("api-key".into()),
        secret: secret(key),
    };
    activate_credentials(vec![
        query(&deployment, "DEP-SECRET"),
        query(&history, "HIST-SECRET"),
        query(&redirecting, "REDIR-SECRET"),
        ActiveCredential {
            endpoint: jupiter.clone(),
            auth: AuthKind::Header("x-api-key".into()),
            secret: secret("JUP-SECRET"),
        },
    ])
    .unwrap();
    let first = |seen: &Arc<Mutex<Vec<Vec<String>>>>| seen.lock().unwrap()[0].clone();

    // The deployment inspection (the capture's continuity, the verifier's deployment proof).
    let program = Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET).unwrap();
    let _ = paths::inspect(Some(&deployment), &program);
    let request = first(&deployment_seen);
    assert!(
        request[0].starts_with("POST /?api-key=DEP-SECRET "),
        "{request:?}"
    );

    // The verifier's history reads.
    let deadline = || Instant::now() + Duration::from_secs(5);
    let fetcher = RpcFetcher::new(history.clone(), 2_000, "finalized").unwrap();
    // (A null result is no block; the request is what this checks.)
    let _ =
        fetcher.gap_request_metered("getSlot", serde_json::json!([]), deadline(), 2_000, 1 << 20);
    assert!(first(&history_seen)[0].starts_with("POST /?api-key=HIST-SECRET "));

    // Jupiter: the header, never the URL or an error.
    let (from, to) = (Pubkey::new_unique(), Pubkey::new_unique());
    let router =
        crate::execution::route::JupiterV2RouteProvider::new(Pubkey::new_unique(), 50, 64, false)
            .unwrap()
            .with_base_url(jupiter)
            .unpaced();
    let err = router
        .quote_exact_in(&from, &to, 500, &[], deadline())
        .unwrap_err();
    assert!(!err.contains("JUP-SECRET"), "{err}");
    let request = first(&jupiter_seen);
    assert!(request[0].starts_with("GET /build?") && !request[0].contains("JUP-SECRET"));
    assert!(
        request
            .iter()
            .any(|l| l.eq_ignore_ascii_case("x-api-key: JUP-SECRET")),
        "{request:?}"
    );

    // A redirect is not followed: the credential never reaches the other host.
    let redirected = RpcFetcher::new(redirecting, 2_000, "finalized").unwrap();
    let refused = redirected.gap_request_metered(
        "getSlot",
        serde_json::json!([]),
        deadline(),
        2_000,
        1 << 20,
    );
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(redirecting_seen.lock().unwrap().len(), 1);
    assert!(
        sink_seen.lock().unwrap().is_empty(),
        "no request reached the redirect's target"
    );
}

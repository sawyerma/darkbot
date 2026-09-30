use rustls::{
    pki_types::ServerName,
    ClientConfig,
    ClientConnection,
    RootCertStore,
    StreamOwned,
};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

const HOST: &str = "bsc-dataseed.bnbchain.org";
const PORT: u16 = 443;

fn http_request(body: &str) -> Vec<u8> {
    format!(
        "POST / HTTP/1.1\r\n\
         Host: {HOST}\r\n\
         Content-Type: application/json\r\n\
         Accept: application/json\r\n\
         Accept-Encoding: identity\r\n\
         Connection: keep-alive\r\n\
         Content-Length: {}\r\n\
         \r\n{}",
        body.len(),
        body
    )
    .into_bytes()
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn read_http(
    stream: &mut StreamOwned<ClientConnection, TcpStream>,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 8192];

    loop {
        let n = stream.read(&mut tmp).expect("TLS read failed");
        assert!(n > 0, "server closed connection");

        buf.extend_from_slice(&tmp[..n]);

        let Some(h) = find_header_end(&buf) else {
            continue;
        };

        let header = String::from_utf8_lossy(&buf[..h]).to_lowercase();

        assert!(
            !header.contains("transfer-encoding: chunked"),
            "chunked response unsupported by this bounded probe"
        );

        let content_length = header
            .lines()
            .find_map(|line| {
                line.strip_prefix("content-length:")
                    .map(|x| x.trim().parse::<usize>().unwrap())
            })
            .expect("response has no Content-Length");

        let full = h + 4 + content_length;

        if buf.len() >= full {
            buf.truncate(full);
            return buf;
        }
    }
}

fn body_of(response: &[u8]) -> &[u8] {
    let h = find_header_end(response).unwrap();
    &response[h + 4..]
}

fn stats(mut v: Vec<u64>) -> Value {
    v.sort_unstable();

    let at = |num: usize, den: usize| -> u64 {
        v[((v.len() - 1) * num) / den]
    };

    let sum: u128 = v.iter().map(|x| *x as u128).sum();

    json!({
        "p50_ns": at(50,100),
        "p95_ns": at(95,100),
        "p99_ns": at(99,100),
        "max_ns": *v.last().unwrap(),
        "mean_ns": (sum / v.len() as u128) as u64,
        "p50_us": at(50,100) as f64 / 1000.0,
        "p95_us": at(95,100) as f64 / 1000.0,
        "p99_us": at(99,100) as f64 / 1000.0,
        "max_us": *v.last().unwrap() as f64 / 1000.0,
        "p50_ms": at(50,100) as f64 / 1_000_000.0,
        "p95_ms": at(95,100) as f64 / 1_000_000.0,
        "p99_ms": at(99,100) as f64 / 1_000_000.0,
        "max_ms": *v.last().unwrap() as f64 / 1_000_000.0
    })
}

fn main() {
    // DNS, TCP and TLS setup happen OUTSIDE the measured hot path.
    let addr = (HOST, PORT)
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("DNS returned no address");

    let tcp = TcpStream::connect_timeout(
        &addr,
        Duration::from_secs(5),
    )
    .expect("TCP connect failed");

    tcp.set_nodelay(true).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(5))).unwrap();

    let roots =
        RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    let server_name =
        ServerName::try_from(HOST.to_string()).unwrap();

    let conn =
        ClientConnection::new(Arc::new(config), server_name).unwrap();

    let mut stream = StreamOwned::new(conn, tcp);

    // First verify this really is BSC mainnet.
    let chain_body =
        r#"{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}"#;

    let chain_req = http_request(chain_body);

    stream.write_all(&chain_req).unwrap();
    stream.flush().unwrap();

    let chain_resp = read_http(&mut stream);
    let chain_text =
        String::from_utf8_lossy(body_of(&chain_resp));

    assert!(
        chain_text.contains("\"0x38\""),
        "endpoint did not report BSC chainId 56: {chain_text}"
    );

    // Same approximate size as our 343-byte signed Direct-BUY TX,
    // but deliberately impossible to parse as a transaction.
    //
    // First byte 0x00 means the payload is NOT an RLP transaction list.
    // Therefore this can never be accepted/broadcast as our BUY.
    let invalid_raw = format!("0x{}", "00".repeat(343));

    let send_body = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"eth_sendRawTransaction","params":["{}"]}}"#,
        invalid_raw
    );

    let request = http_request(&send_body);

    const WARMUP: usize = 10;
    const SAMPLES: usize = 100;

    let mut handoff = Vec::with_capacity(SAMPLES);
    let mut roundtrip = Vec::with_capacity(SAMPLES);

    let mut first_error = None;

    for i in 0..(WARMUP + SAMPLES) {
        let t0 = Instant::now();

        stream.write_all(&request).unwrap();
        stream.flush().unwrap();

        // T_send candidate boundary for THIS remote RPC transport:
        // all request bytes handed through warm TLS/TCP stream.
        let t_send = Instant::now();

        let response = read_http(&mut stream);
        let t_resp = Instant::now();

        let body =
            String::from_utf8_lossy(body_of(&response)).to_string();

        assert!(
            body.contains("\"error\""),
            "invalid transaction unexpectedly not rejected: {body}"
        );

        assert!(
            !body.contains("\"result\":\"0x"),
            "SAFETY FAILURE: endpoint returned transaction hash"
        );

        let lower = body.to_lowercase();

        assert!(
            !lower.contains("rate limit")
                && !lower.contains("too many request"),
            "provider rate limit encountered: {body}"
        );

        if first_error.is_none() {
            first_error = Some(body);
        }

        if i >= WARMUP {
            handoff.push(
                (t_send - t0).as_nanos() as u64
            );

            roundtrip.push(
                (t_resp - t0).as_nanos() as u64
            );
        }

        // Outside measured section; keep well below provider rate limits.
        thread::sleep(Duration::from_millis(50));
    }

    let result = json!({
        "status": "G0.5_REAL_REMOTE_TRANSPORT_PREQUALIFICATION",
        "endpoint_host": HOST,
        "chain_id_verified": 56,
        "transport": "preconnected TLS/TCP HTTP/1.1",
        "warmup_samples": WARMUP,
        "measured_samples": SAMPLES,
        "wire_raw_tx_bytes": 343,
        "wire_payload": "deliberately invalid/non-broadcastable transaction-shaped payload",
        "first_rejection": first_error,
        "stages": {
            "TLS_TCP_write_flush_handoff": stats(handoff),
            "request_to_JSON_RPC_response": stats(roundtrip)
        },
        "non_claims": [
            "Invalid payload only; no valid transaction broadcast",
            "No value-bearing BUY sent",
            "Remote RPC socket handoff is not local bsc-geth P2P propagation",
            "Response RTT is not T_send and is not T_buy",
            "This alone does not close final G0.5"
        ]
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&result).unwrap()
    );
}

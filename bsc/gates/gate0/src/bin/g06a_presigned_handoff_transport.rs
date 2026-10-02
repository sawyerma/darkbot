use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use serde_json::json;
use std::{
    env,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

const WARMUP: usize = 10;
const SAMPLES: usize = 100;
const DEFAULT_PAD_BYTES: usize = 8192;

#[derive(Clone, Copy)]
enum Target {
    FortyEightClub,
    BlockRazor,
}

impl Target {
    fn from_args() -> Self {
        match env::args().nth(1).as_deref() {
            Some("--48club") => Self::FortyEightClub,
            Some("--blockrazor") => Self::BlockRazor,
            _ => panic!("usage: g06a_presigned_handoff_transport --48club|--blockrazor"),
        }
    }

    fn host(self) -> String {
        match self {
            Self::FortyEightClub => env::var("DARKBOT_G06A_48CLUB_HOST")
                .unwrap_or_else(|_| "puissant-builder.48.club".to_string()),
            Self::BlockRazor => env::var("DARKBOT_G06A_BLOCKRAZOR_FAST_HOST")
                .unwrap_or_else(|_| "bsc.blockrazor.xyz".to_string()),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::FortyEightClub => "48club",
            Self::BlockRazor => "blockrazor_or_fast_submit",
        }
    }
}

fn connect_tls(host: &str) -> StreamOwned<ClientConnection, TcpStream> {
    let addr = (host, 443)
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("DNS returned no address");

    let mut tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
        .expect("TCP connect failed");

    tcp.set_nodelay(true).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(5))).unwrap();

    let roots =
        RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    let server_name = ServerName::try_from(host.to_string()).unwrap();
    let mut conn = ClientConnection::new(Arc::new(config), server_name).unwrap();
    conn.complete_io(&mut tcp).expect("TLS handshake failed");

    StreamOwned::new(conn, tcp)
}

fn request(host: &str, body: &str) -> Vec<u8> {
    format!(
        "POST / HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nAccept: application/json\r\nAccept-Encoding: identity\r\nConnection: keep-alive\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    )
    .into_bytes()
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn read_http<W: Read>(stream: &mut W) -> Vec<u8> {
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

        if let Some(line) = header
            .lines()
            .find(|line| line.starts_with("content-length:"))
        {
            let len = line["content-length:".len()..]
                .trim()
                .parse::<usize>()
                .unwrap();
            let full = h + 4 + len;
            if buf.len() >= full {
                buf.truncate(full);
                return buf;
            }
        }

        if header.contains("transfer-encoding: chunked") {
            if buf[h + 4..].windows(5).any(|w| w == b"0\r\n\r\n") {
                return buf;
            }
        }
    }
}

#[derive(Default)]
struct Sample {
    select_ns: u64,
    handoff_ns: u64,
    total_ns: u64,
    response_ns: u64,
}

fn run_once<W: Read + Write>(
    stream: &mut W,
    prepared_request: &[u8],
    expected_prefix: &[u8],
) -> Sample {
    let t_signal = Instant::now();

    assert!(prepared_request.starts_with(expected_prefix));
    let selected = prepared_request;
    let t_ready = Instant::now();

    let t_queue = Instant::now();
    stream.write_all(selected).expect("TLS write failed");
    stream.flush().expect("TLS flush failed");
    let t_send = Instant::now();

    let _response = read_http(stream);
    let t_response = Instant::now();

    Sample {
        select_ns: (t_ready - t_signal).as_nanos() as u64,
        handoff_ns: (t_send - t_queue).as_nanos() as u64,
        total_ns: (t_send - t_signal).as_nanos() as u64,
        response_ns: (t_response - t_signal).as_nanos() as u64,
    }
}

fn stats(mut v: Vec<u64>) -> serde_json::Value {
    v.sort_unstable();

    let at = |num: usize, den: usize| -> u64 {
        let idx = ((v.len() - 1) * num) / den;
        v[idx]
    };

    let sum: u128 = v.iter().map(|x| *x as u128).sum();
    let over_1ms = v.iter().filter(|x| **x > 1_000_000).count();

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
        "over_1ms_count": over_1ms,
        "over_1ms_fraction": over_1ms as f64 / v.len() as f64
    })
}

fn main() {
    let target = Target::from_args();
    let host = target.host();

    let pad_bytes = env::var("DARKBOT_G06A_PREPARED_PAYLOAD_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_PAD_BYTES);

    let body = serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_chainId",
        "params": [],
        "pad": "x".repeat(pad_bytes)
    }))
    .unwrap();

    let prepared_request = request(&host, &body);
    let expected_prefix = b"POST / HTTP/1.1\r\n";

    let mut stream = connect_tls(&host);

    for _ in 0..WARMUP {
        let _ = run_once(&mut stream, &prepared_request, expected_prefix);
        thread::sleep(Duration::from_millis(200));
    }

    let mut select = Vec::with_capacity(SAMPLES);
    let mut handoff = Vec::with_capacity(SAMPLES);
    let mut total = Vec::with_capacity(SAMPLES);
    let mut response = Vec::with_capacity(SAMPLES);

    for _ in 0..SAMPLES {
        let s = run_once(&mut stream, &prepared_request, expected_prefix);
        select.push(s.select_ns);
        handoff.push(s.handoff_ns);
        total.push(s.total_ns);
        response.push(s.response_ns);
        thread::sleep(Duration::from_millis(200));
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "G0.6A_PRESIGNED_PREPARED_TRANSPORT_MEASUREMENT",
            "target": target.label(),
            "endpoint_host": host,
            "prepared_before_T0": true,
            "request_preframed_before_T0": true,
            "payload_padding_bytes": pad_bytes,
            "prepared_request_bytes": prepared_request.len(),
            "measured_samples": SAMPLES,
            "warmup_samples": WARMUP,
            "stages": {
                "T_signal_to_T_ready_select_only": stats(select),
                "T_queue_to_T_send_TLS_write_flush": stats(handoff),
                "T_signal_to_T_send": stats(total),
                "T_signal_to_response": stats(response)
            },
            "interpretation": [
                "Measures the frozen G0.2/G0.5 controlled-self-launch preparation rule: all expensive work is complete before T0.",
                "No BUY build, quote, signing, transaction serialization or bundle construction occurs after T_signal.",
                "This is a transport/hotpath latency measurement only; it is not T_buy or inclusion evidence.",
                "BlockRazor Fast Submit can be measured by supplying the provider-issued host in DARKBOT_G06A_BLOCKRAZOR_FAST_HOST."
            ],
            "network_action": "read-only JSON-RPC eth_chainId request with inert padding; no transaction or bundle submission",
            "value_bearing_action": false
        }))
        .unwrap()
    );
}

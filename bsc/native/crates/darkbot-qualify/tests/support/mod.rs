//! A loopback TLS server that answers the probe like a builder that refuses
//! it, with the runner's test certificates. No real provider is contacted.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use darkbot_runner::inclusion::tls::{TlsEndpoint, TlsSetup};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

pub const TEST_CA: &[u8] = include_bytes!("../../../darkbot-runner/tests/fixtures/tls/ca.der");
pub const UNTRUSTED_CA: &[u8] =
    include_bytes!("../../../darkbot-runner/tests/fixtures/tls/other_ca.der");
const CERT: &[u8] = include_bytes!("../../../darkbot-runner/tests/fixtures/tls/localhost.der");
const KEY: &[u8] = include_bytes!("../../../darkbot-runner/tests/fixtures/tls/localhost.key.der");

/// How the server answers each request.
#[derive(Clone, Copy)]
pub enum Answer {
    /// HTTP 200 with a JSON-RPC error carrying the request's id.
    RejectEcho,
    /// HTTP 401 with a small body.
    Unauthorized,
    /// No answer; the connection is closed.
    CloseAfterRequest,
}

pub struct EchoServer {
    pub address: SocketAddr,
    accepted: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl EchoServer {
    pub fn start(answer: Answer) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(CERT.to_vec())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY.to_vec())),
            )
            .unwrap();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let config = Arc::new(config);
        let accepted = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (count, seen) = (Arc::clone(&accepted), Arc::clone(&requests));
        thread::spawn(move || {
            for socket in listener.incoming() {
                let Ok(socket) = socket else { return };
                count.fetch_add(1, Ordering::SeqCst);
                let config = Arc::clone(&config);
                let seen = Arc::clone(&seen);
                thread::spawn(move || serve(socket, config, answer, seen));
            }
        });
        Self {
            address,
            accepted,
            requests,
        }
    }

    pub fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }

    pub fn requests(&self) -> Vec<Vec<u8>> {
        self.requests.lock().unwrap().clone()
    }

    /// `localhost` at this server's address.
    pub fn endpoint(&self) -> TlsEndpoint {
        TlsEndpoint::new("localhost", self.address.port())
            .unwrap()
            .with_address(self.address)
    }
}

pub fn trusting(ca: &[u8]) -> TlsSetup {
    TlsSetup::with_trust_anchors(&[ca]).unwrap()
}

fn serve(
    socket: TcpStream,
    config: Arc<ServerConfig>,
    answer: Answer,
    requests: Arc<Mutex<Vec<Vec<u8>>>>,
) {
    let Ok(connection) = ServerConnection::new(config) else {
        return;
    };
    let mut tls = StreamOwned::new(connection, socket);
    while tls.conn.is_handshaking() {
        if tls.conn.complete_io(&mut tls.sock).is_err() {
            return;
        }
    }
    while let Some(request) = read_request(&mut tls) {
        let id = request_id(&request);
        requests.lock().unwrap().push(request);
        let response = match answer {
            Answer::RejectEcho => {
                let body = format!(
                    "{{\"jsonrpc\":\"2.0\",\"id\":\"{id}\",\
                     \"error\":{{\"code\":-32602,\"message\":\"invalid bundle\"}}}}"
                );
                http("200 OK", &body)
            }
            Answer::Unauthorized => http("401 Unauthorized", "{\"error\":\"unauthorized\"}"),
            Answer::CloseAfterRequest => return,
        };
        if tls.write_all(&response).is_err() || tls.flush().is_err() {
            return;
        }
    }
}

fn http(status: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// The JSON-RPC id of a request, as the probe writes it.
fn request_id(request: &[u8]) -> String {
    let text = String::from_utf8_lossy(request);
    let marker = "\"id\":\"";
    text.find(marker)
        .and_then(|at| {
            let rest = &text[at + marker.len()..];
            rest.find('"').map(|end| rest[..end].to_owned())
        })
        .unwrap_or_default()
}

fn read_request(tls: &mut StreamOwned<ServerConnection, TcpStream>) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let head_end = loop {
        if let Some(end) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            break end;
        }
        let count = tls.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        data.extend_from_slice(&chunk[..count]);
    };
    let head = String::from_utf8_lossy(&data[..head_end]).to_string();
    let length: usize = head.split("\r\n").find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            value.trim().parse().ok()
        } else {
            None
        }
    })?;
    let total = head_end + 4 + length;
    while data.len() < total {
        let count = tls.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        data.extend_from_slice(&chunk[..count]);
    }
    data.truncate(total);
    Some(data)
}

//! A loopback TLS server for the transport tests, and client settings
//! that trust its test CA.
//!
//! The certificates and keys under `tests/fixtures/tls` are test material,
//! made by `generate.py` there. No real provider is contacted.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use darkbot_bsc_execution::submission::tls::{
    TlsConnectionOwner, TlsEndpoint, TlsPolicy, TlsSetup,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

/// The CA the tests trust.
pub const TEST_CA: &[u8] = include_bytes!("../fixtures/tls/ca.der");
/// A CA the tests do not trust.
pub const UNTRUSTED_CA: &[u8] = include_bytes!("../fixtures/tls/other_ca.der");

const LOCALHOST_CERT: &[u8] = include_bytes!("../fixtures/tls/localhost.der");
const LOCALHOST_KEY: &[u8] = include_bytes!("../fixtures/tls/localhost.key.der");
const OTHER_NAME_CERT: &[u8] = include_bytes!("../fixtures/tls/other_name.der");
const OTHER_NAME_KEY: &[u8] = include_bytes!("../fixtures/tls/other_name.key.der");

/// How long a server holds a connection it does not answer.
const HOLD: Duration = Duration::from_secs(10);

/// Which certificate the server presents.
#[derive(Clone, Copy)]
pub enum Served {
    /// For `localhost`, from the test CA.
    Localhost,
    /// For `other.test`, from the test CA.
    OtherName,
}

/// What the server offers by ALPN.
#[derive(Clone, Copy)]
pub enum ServerAlpn {
    /// `http/1.1`.
    Http11,
    /// Nothing: no ALPN.
    Absent,
    /// `h2` only.
    H2Only,
}

/// What the server does on each connection.
#[derive(Clone)]
pub enum Behaviour {
    /// Reads each request and answers it with `pieces`, waiting `gap`
    /// before each piece.
    Answer { pieces: Vec<Vec<u8>>, gap: Duration },
    /// Reads the request and closes without an answer.
    CloseAfterRequest,
    /// Reads the request and never answers.
    Hold,
    /// Completes the handshake and never reads.
    NeverRead,
    /// Completes the handshake and closes.
    CloseAfterHandshake,
    /// Accepts the TCP connection and never takes part in a handshake.
    SilentTcp,
}

/// A running server on 127.0.0.1.
pub struct TestServer {
    pub address: SocketAddr,
    accepted: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl TestServer {
    pub fn start(served: Served, alpn: ServerAlpn, behaviour: Behaviour) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let config = Arc::new(server_config(served, alpn));
        let accepted = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (count, seen) = (Arc::clone(&accepted), Arc::clone(&requests));
        thread::spawn(move || {
            for socket in listener.incoming() {
                let Ok(socket) = socket else { return };
                count.fetch_add(1, Ordering::SeqCst);
                let config = Arc::clone(&config);
                let behaviour = behaviour.clone();
                let seen = Arc::clone(&seen);
                thread::spawn(move || serve(socket, config, behaviour, seen));
            }
        });
        Self {
            address,
            accepted,
            requests,
        }
    }

    /// TCP connections accepted so far.
    pub fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }

    /// Complete requests received so far.
    pub fn requests(&self) -> Vec<Vec<u8>> {
        self.requests.lock().unwrap().clone()
    }
}

fn server_config(served: Served, alpn: ServerAlpn) -> ServerConfig {
    let (cert, key) = match served {
        Served::Localhost => (LOCALHOST_CERT, LOCALHOST_KEY),
        Served::OtherName => (OTHER_NAME_CERT, OTHER_NAME_KEY),
    };
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(cert.to_vec())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.to_vec())),
        )
        .unwrap();
    config.alpn_protocols = match alpn {
        ServerAlpn::Http11 => vec![b"http/1.1".to_vec()],
        ServerAlpn::Absent => Vec::new(),
        ServerAlpn::H2Only => vec![b"h2".to_vec()],
    };
    config
}

fn serve(
    socket: TcpStream,
    config: Arc<ServerConfig>,
    behaviour: Behaviour,
    requests: Arc<Mutex<Vec<Vec<u8>>>>,
) {
    if matches!(behaviour, Behaviour::SilentTcp) {
        thread::sleep(HOLD);
        return;
    }
    let Ok(connection) = ServerConnection::new(config) else {
        return;
    };
    let mut tls = StreamOwned::new(connection, socket);
    while tls.conn.is_handshaking() {
        if tls.conn.complete_io(&mut tls.sock).is_err() {
            return;
        }
    }
    match behaviour {
        Behaviour::NeverRead => {
            thread::sleep(HOLD);
            return;
        }
        Behaviour::CloseAfterHandshake => return,
        _ => {}
    }
    while let Some(request) = read_request(&mut tls) {
        requests.lock().unwrap().push(request);
        match &behaviour {
            Behaviour::Answer { pieces, gap } => {
                for piece in pieces {
                    thread::sleep(*gap);
                    if tls.write_all(piece).is_err() || tls.flush().is_err() {
                        return;
                    }
                }
            }
            Behaviour::Hold => {
                thread::sleep(HOLD);
                return;
            }
            _ => return,
        }
    }
}

/// One complete HTTP/1.1 request with a `Content-Length` body.
fn read_request(tls: &mut StreamOwned<ServerConnection, TcpStream>) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let head_end = loop {
        if let Some(end) = find(&data, b"\r\n\r\n") {
            break end;
        }
        let count = tls.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        data.extend_from_slice(&chunk[..count]);
    };
    let total = head_end + 4 + content_length(&data[..head_end])?;
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

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn content_length(head: &[u8]) -> Option<usize> {
    let head = std::str::from_utf8(head).ok()?;
    head.split("\r\n").find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            value.trim().parse().ok()
        } else {
            None
        }
    })
}

/// Generous limits, a long idle time and room for large requests.
pub fn policy() -> TlsPolicy {
    TlsPolicy {
        connect_timeout: Duration::from_secs(2),
        handshake_timeout: Duration::from_secs(2),
        max_idle: Duration::from_secs(30),
        response_timeout: Duration::from_secs(5),
        max_plaintext_request_len: 64 << 20,
    }
}

pub fn trusting(ca: &[u8]) -> TlsSetup {
    TlsSetup::with_trust_anchors(&[ca]).unwrap()
}

/// `localhost` at the server's address.
pub fn endpoint(server: &TestServer) -> TlsEndpoint {
    TlsEndpoint::new("localhost", server.address.port())
        .unwrap()
        .with_address(server.address)
}

pub fn owner_with(server: &TestServer, setup: TlsSetup, policy: TlsPolicy) -> TlsConnectionOwner {
    TlsConnectionOwner::new(endpoint(server), setup, policy).unwrap()
}

pub fn owner(server: &TestServer) -> TlsConnectionOwner {
    owner_with(server, trusting(TEST_CA), policy())
}

/// Waits until `done` holds, at most five seconds.
pub fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if done() {
            return true;
        }
        thread::sleep(Duration::from_millis(2));
    }
    done()
}

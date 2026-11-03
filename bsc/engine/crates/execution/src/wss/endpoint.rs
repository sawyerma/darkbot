//! The `wss://` and `https://` endpoint: URL validation, TCP connect and the
//! rustls client.

use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::submission::tls::{SetupError, TlsSetup};

use super::{DisconnectReason, UrlError, WssLimits, stopped};

/// The port of `wss://` and `https://` when the URL names none (RFC 6455
/// §3, RFC 9110 §4.2.2).
const DEFAULT_PORT: u16 = 443;

/// The TLS stream the WebSocket runs over.
pub(super) type TlsStream = StreamOwned<ClientConnection, TcpStream>;

/// A validated `wss://` or `https://` endpoint.
#[derive(Debug)]
pub(crate) struct Endpoint {
    host: String,
    port: u16,
    /// The authority as the URL names it: the `Host` header value.
    authority: String,
    /// The request target (`/` when absent).
    path: String,
    /// The normalized URL (path `/` when absent).
    url: String,
}

impl Endpoint {
    /// Accepts exactly `wss://host[:port][/path]` with a DNS name or IPv4
    /// host, no user information, query or fragment.
    pub(crate) fn parse(url: &str) -> Result<Self, UrlError> {
        Self::parse_scheme(url, "wss://")
    }

    /// The same rules for `https://host[:port][/path]` (the canonical
    /// follower's JSON-RPC endpoints; Audit E 5.14).
    pub(crate) fn parse_https(url: &str) -> Result<Self, UrlError> {
        Self::parse_scheme(url, "https://")
    }

    fn parse_scheme(url: &str, scheme: &str) -> Result<Self, UrlError> {
        let rest = url.strip_prefix(scheme).ok_or(UrlError::WrongScheme)?;
        if rest.contains(['?', '#']) {
            return Err(UrlError::QueryOrFragment);
        }
        let (authority, path) = match rest.find('/') {
            Some(at) => rest.split_at(at),
            None => (rest, "/"),
        };
        if authority.contains('@') {
            return Err(UrlError::Credentials);
        }
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        let host_ok = !host.is_empty()
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            && !host.starts_with(['.', '-'])
            && !host.ends_with(['.', '-']);
        if !host_ok || ServerName::try_from(host.to_owned()).is_err() {
            return Err(UrlError::Host);
        }
        let port = match port {
            Some(port) => parse_port(port)?,
            None => DEFAULT_PORT,
        };
        if !path.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(UrlError::Path);
        }
        Ok(Self {
            host: host.to_owned(),
            port,
            authority: authority.to_owned(),
            path: path.to_owned(),
            url: format!("{scheme}{authority}{path}"),
        })
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// The `Host` header value and the request target.
    pub(crate) fn authority_and_path(&self) -> (String, String) {
        (self.authority.clone(), self.path.clone())
    }
}

fn parse_port(port: &str) -> Result<u16, UrlError> {
    if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return Err(UrlError::Port);
    }
    match port.parse::<u16>() {
        Ok(p) if p != 0 => Ok(p),
        _ => Err(UrlError::Port),
    }
}

/// The client configuration: the builders' TLS setup (`TlsSetup`: the ring
/// provider explicitly, never a process default; ALPN `http/1.1` for the
/// upgrade) over the webpki roots (Audit E 4.5).
pub(crate) fn tls_config() -> Result<Arc<ClientConfig>, SetupError> {
    let roots = harness_roots(RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    })?;
    Ok(TlsSetup::from_roots(roots)?.config())
}

/// REV.1.0 R11: the harness build (feature `loopback-harness`, never the
/// production build) also trusts the test CA, for loopback mock servers.
#[cfg(feature = "loopback-harness")]
fn harness_roots(mut roots: RootCertStore) -> Result<RootCertStore, SetupError> {
    const TEST_CA: &[u8] = include_bytes!("../../tests/fixtures/tls/ca.der");
    roots
        .add(rustls::pki_types::CertificateDer::from(TEST_CA))
        .map_err(|_| SetupError::InvalidTrustAnchor)?;
    Ok(roots)
}

#[cfg(not(feature = "loopback-harness"))]
fn harness_roots(roots: RootCertStore) -> Result<RootCertStore, SetupError> {
    Ok(roots)
}

/// Applies the socket bounds of `limits` to a connected TCP stream.
pub(super) fn configure_socket(tcp: &TcpStream, limits: &WssLimits) -> io::Result<()> {
    tcp.set_nodelay(true)?;
    tcp.set_read_timeout(Some(limits.read_tick))?;
    tcp.set_write_timeout(Some(limits.write_timeout))
}

/// A read or write that only ran into the socket timeout: the caller checks
/// its bounds and retries.
pub(crate) fn is_tick(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

/// A socket error, or a TLS error carried in one.
pub(crate) fn classify_io(e: &io::Error) -> DisconnectReason {
    match e.get_ref() {
        Some(inner) if inner.is::<rustls::Error>() => DisconnectReason::Tls,
        _ => DisconnectReason::Io(e.kind()),
    }
}

/// Resolves, connects (each address within `connect_timeout`) and completes
/// the TLS handshake within the attempt's `handshake_timeout`.
pub(crate) fn connect_tls(
    endpoint: &Endpoint,
    tls: &Arc<ClientConfig>,
    limits: &WssLimits,
    stop: &AtomicBool,
    started: Instant,
) -> Result<TlsStream, DisconnectReason> {
    let addrs = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .map_err(|e| DisconnectReason::Io(e.kind()))?;
    let mut last = DisconnectReason::NoAddress;
    for addr in addrs {
        if stopped(stop) {
            return Err(DisconnectReason::Stopped);
        }
        if started.elapsed() >= limits.handshake_timeout {
            return Err(DisconnectReason::Timeout);
        }
        match TcpStream::connect_timeout(&addr, limits.connect_timeout) {
            Ok(tcp) => return tls_handshake(tcp, endpoint, tls, limits, stop, started),
            Err(e) => last = DisconnectReason::Io(e.kind()),
        }
    }
    Err(last)
}

fn tls_handshake(
    mut tcp: TcpStream,
    endpoint: &Endpoint,
    tls: &Arc<ClientConfig>,
    limits: &WssLimits,
    stop: &AtomicBool,
    started: Instant,
) -> Result<TlsStream, DisconnectReason> {
    configure_socket(&tcp, limits).map_err(|e| classify_io(&e))?;
    let server_name =
        ServerName::try_from(endpoint.host.clone()).map_err(|_| DisconnectReason::Tls)?;
    let mut conn =
        ClientConnection::new(Arc::clone(tls), server_name).map_err(|_| DisconnectReason::Tls)?;
    while conn.is_handshaking() {
        if stopped(stop) {
            return Err(DisconnectReason::Stopped);
        }
        if started.elapsed() >= limits.handshake_timeout {
            return Err(DisconnectReason::Timeout);
        }
        match conn.complete_io(&mut tcp) {
            Ok(_) => {}
            Err(e) if is_tick(&e) => {}
            Err(e) => return Err(classify_io(&e)),
        }
    }
    Ok(StreamOwned::new(conn, tcp))
}

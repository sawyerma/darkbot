//! Establishing one warm TLS connection. Maintenance only: this is where
//! name resolution, TCP connect and the TLS handshake happen, and nowhere
//! else.

use std::fmt;
use std::io;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::{CipherSuite, ClientConfig, ClientConnection, ProtocolVersion};

use super::config::{ALPN_HTTP11, TlsEndpoint, TlsPolicy, tls_output_capacity};

/// Id of this transport implementation, as a qualification would name it:
/// the runner's TLS transport, version 1, over rustls 0.23.45 with the ring
/// provider, speaking HTTP/1.1.
pub const TLS_TRANSPORT_IMPLEMENTATION: &str = "darkbot-runner/tls-v1 rustls-0.23.45 ring http/1.1";

/// The HTTP version spoken on a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpVersion {
    /// HTTP/1.1: offered as the only ALPN protocol and accepted either
    /// when the server selected it or when the server selected none.
    Http11,
}

/// What a connection was established as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionFacts {
    /// Generation of the connection: one more with every connection the
    /// owner establishes.
    pub generation: u64,
    /// The address connected to.
    pub peer: SocketAddr,
    /// The negotiated TLS version.
    pub tls_version: ProtocolVersion,
    /// The negotiated cipher suite.
    pub cipher_suite: CipherSuite,
    /// The HTTP version spoken.
    pub http: HttpVersion,
    /// Whether the server selected `http/1.1` by ALPN, rather than none.
    pub alpn_selected: bool,
}

/// Why a connection could not be established. Carries no request data and
/// no credential.
#[derive(Clone, Debug, PartialEq)]
pub enum ConnectError {
    /// The host resolved to no address.
    Resolve,
    /// No address accepted a TCP connection in time.
    Connect,
    /// The TLS handshake did not complete in time.
    HandshakeTimeout,
    /// The server's certificate was refused.
    Certificate(rustls::Error),
    /// Another TLS failure, including an alert from the server.
    Tls(rustls::Error),
    /// The server selected another protocol than HTTP/1.1.
    NotHttp11,
    /// The server closed the connection during the handshake.
    Closed,
    /// A socket operation failed.
    Io(io::ErrorKind),
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resolve => f.write_str("tls connect: host resolved to no address"),
            Self::Connect => f.write_str("tls connect: no address accepted in time"),
            Self::HandshakeTimeout => f.write_str("tls connect: handshake timed out"),
            Self::Certificate(error) => write!(f, "tls connect: certificate refused: {error}"),
            Self::Tls(error) => write!(f, "tls connect: {error}"),
            Self::NotHttp11 => f.write_str("tls connect: server selected another protocol"),
            Self::Closed => f.write_str("tls connect: closed during the handshake"),
            Self::Io(kind) => write!(f, "tls connect: socket error {kind:?}"),
        }
    }
}

impl std::error::Error for ConnectError {}

/// One established TLS connection: a TCP socket in non-blocking mode and
/// the rustls client connection on it, handshake complete, nothing
/// pending.
pub struct WarmTlsConnection {
    pub(crate) socket: TcpStream,
    pub(crate) tls: ClientConnection,
    facts: ConnectionFacts,
}

impl WarmTlsConnection {
    /// Resolves, connects and completes the handshake. Blocking, with the
    /// policy's limits; never called from a dispatch.
    pub(crate) fn establish(
        endpoint: &TlsEndpoint,
        policy: &TlsPolicy,
        config: Arc<ClientConfig>,
        generation: u64,
    ) -> Result<Self, ConnectError> {
        let addresses: Vec<SocketAddr> = match endpoint.connect_to() {
            Some(address) => vec![address],
            None => (endpoint.host(), endpoint.port())
                .to_socket_addrs()
                .map_err(|_| ConnectError::Resolve)?
                .collect(),
        };
        if addresses.is_empty() {
            return Err(ConnectError::Resolve);
        }
        let mut socket = addresses
            .iter()
            .find_map(|address| TcpStream::connect_timeout(address, policy.connect_timeout).ok())
            .ok_or(ConnectError::Connect)?;
        let peer = socket.peer_addr().map_err(io_error)?;
        socket.set_nodelay(true).map_err(io_error)?;

        let mut tls =
            ClientConnection::new(config, endpoint.server_name()).map_err(ConnectError::Tls)?;
        tls.set_buffer_limit(Some(tls_output_capacity(policy.max_plaintext_request_len)));
        handshake(&mut tls, &mut socket, policy.handshake_timeout)?;

        let alpn_selected = match tls.alpn_protocol() {
            None => false,
            Some(protocol) if protocol == ALPN_HTTP11 => true,
            Some(_) => return Err(ConnectError::NotHttp11),
        };
        let tls_version = tls.protocol_version().ok_or(ConnectError::Closed)?;
        let cipher_suite = tls
            .negotiated_cipher_suite()
            .map(|suite| suite.suite())
            .ok_or(ConnectError::Closed)?;

        socket.set_read_timeout(None).map_err(io_error)?;
        socket.set_write_timeout(None).map_err(io_error)?;
        socket.set_nonblocking(true).map_err(io_error)?;
        Ok(Self {
            socket,
            tls,
            facts: ConnectionFacts {
                generation,
                peer,
                tls_version,
                cipher_suite,
                http: HttpVersion::Http11,
                alpn_selected,
            },
        })
    }

    /// What the connection was established as.
    pub fn facts(&self) -> ConnectionFacts {
        self.facts
    }
}

impl fmt::Debug for WarmTlsConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WarmTlsConnection")
            .field("facts", &self.facts)
            .finish_non_exhaustive()
    }
}

/// Drives the handshake to completion on a blocking socket, and sends
/// what it leaves pending, within `limit`.
fn handshake(
    tls: &mut ClientConnection,
    socket: &mut TcpStream,
    limit: Duration,
) -> Result<(), ConnectError> {
    let started = Instant::now();
    loop {
        let remaining = limit
            .checked_sub(started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(ConnectError::HandshakeTimeout)?;
        socket.set_read_timeout(Some(remaining)).map_err(io_error)?;
        socket
            .set_write_timeout(Some(remaining))
            .map_err(io_error)?;
        if tls.is_handshaking() {
            tls.complete_io(socket).map_err(|error| classify(&error))?;
        } else if tls.wants_write() {
            tls.write_tls(socket).map_err(|error| classify(&error))?;
        } else {
            return Ok(());
        }
    }
}

/// What an I/O error during the handshake amounts to.
fn classify(error: &io::Error) -> ConnectError {
    match error.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => ConnectError::HandshakeTimeout,
        io::ErrorKind::UnexpectedEof
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::BrokenPipe => ConnectError::Closed,
        io::ErrorKind::InvalidData => {
            let inner = error
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<rustls::Error>());
            match inner {
                Some(tls @ rustls::Error::InvalidCertificate(_)) => {
                    ConnectError::Certificate(tls.clone())
                }
                Some(tls) => ConnectError::Tls(tls.clone()),
                None => ConnectError::Io(io::ErrorKind::InvalidData),
            }
        }
        kind => ConnectError::Io(kind),
    }
}

fn io_error(error: io::Error) -> ConnectError {
    ConnectError::Io(error.kind())
}

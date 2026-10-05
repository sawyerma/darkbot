//! What a TLS connection is made with: trust anchors, endpoint, timing and
//! size rules, and the provider target that ties the TLS name to the HTTP
//! `Host` header and carries the authorization.
//!
//! Nothing here reads files or the environment. Endpoints, credentials and
//! policies are handed in, already validated by their owner.

use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use darkbot_core::inclusion::capability::AdapterKind;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};

use crate::inclusion::frame::Endpoint;

/// The port of HTTPS.
pub const HTTPS_PORT: u16 = 443;

/// Host of the 48Club Puissant builder as its documentation names it
/// (`https://puissant-builder.48.club/`). A reference for the operator's
/// configuration, which is what is actually used.
pub const CLUB48_DOCUMENTED_HOST: &str = "puissant-builder.48.club";

/// Host of the BlockRazor block builder (Virginia) as its frozen reference
/// names it (`https://virginia.builder.blockrazor.io`). A reference for the
/// operator's configuration, which is what is actually used.
pub const BLOCKRAZOR_DOCUMENTED_HOST: &str = "virginia.builder.blockrazor.io";

/// ALPN protocol id of HTTP/1.1, the only protocol offered.
pub(crate) const ALPN_HTTP11: &[u8] = b"http/1.1";

/// Reason a TLS setup cannot be made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupError {
    /// No trust anchor was given.
    NoTrustAnchors,
    /// A given trust anchor is not a usable certificate.
    InvalidTrustAnchor,
    /// The crypto provider supports none of the protocol versions.
    ProtocolVersions,
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoTrustAnchors => "tls setup: no trust anchor",
            Self::InvalidTrustAnchor => "tls setup: unusable trust anchor",
            Self::ProtocolVersions => "tls setup: no supported protocol version",
        })
    }
}

impl std::error::Error for SetupError {}

/// The client side of TLS: the ring crypto provider, TLS 1.3 and 1.2,
/// server certificates verified against the given trust anchors, no client
/// certificate, and ALPN offering `http/1.1` only.
#[derive(Clone)]
pub struct TlsSetup {
    config: Arc<ClientConfig>,
    trust_anchors: usize,
}

impl TlsSetup {
    /// Trusts the Mozilla root store as `webpki-roots` ships it.
    pub fn web_pki_roots() -> Result<Self, SetupError> {
        let roots = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        Self::from_roots(roots)
    }

    /// Trusts exactly the given DER certificates.
    pub fn with_trust_anchors(certificates: &[&[u8]]) -> Result<Self, SetupError> {
        let mut roots = RootCertStore::empty();
        for der in certificates {
            roots
                .add(CertificateDer::from(der.to_vec()))
                .map_err(|_| SetupError::InvalidTrustAnchor)?;
        }
        Self::from_roots(roots)
    }

    fn from_roots(roots: RootCertStore) -> Result<Self, SetupError> {
        if roots.is_empty() {
            return Err(SetupError::NoTrustAnchors);
        }
        let trust_anchors = roots.len();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|_| SetupError::ProtocolVersions)?
            .with_root_certificates(roots)
            .with_no_client_auth();
        config.alpn_protocols = vec![ALPN_HTTP11.to_vec()];
        Ok(Self {
            config: Arc::new(config),
            trust_anchors,
        })
    }

    /// Number of trust anchors.
    pub fn trust_anchors(&self) -> usize {
        self.trust_anchors
    }

    pub(crate) fn config(&self) -> Arc<ClientConfig> {
        Arc::clone(&self.config)
    }
}

impl fmt::Debug for TlsSetup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsSetup")
            .field("trust_anchors", &self.trust_anchors)
            .field("alpn", &"http/1.1")
            .finish()
    }
}

/// Timing and size rules of one connection. There are no defaults: every
/// value is the operator's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TlsPolicy {
    /// Longest wait for the TCP connection, per address.
    pub connect_timeout: Duration,
    /// Longest wait for the TLS handshake.
    pub handshake_timeout: Duration,
    /// Longest idle time after which a ready connection is no longer
    /// eligible: it reports itself stale and is rebuilt.
    pub max_idle: Duration,
    /// Longest wait for a response before the connection is given up.
    pub response_timeout: Duration,
    /// Largest HTTP request, in plaintext bytes, the connection takes. A
    /// dispatch refuses a larger frame before anything is consumed. The
    /// TLS output buffer is sized from it, see [`tls_output_capacity`].
    pub max_plaintext_request_len: usize,
}

/// Largest TLS record payload: 2^14 bytes.
const MAX_RECORD_PAYLOAD: usize = 16_384;

/// Bytes a TLS record adds to its payload, as an upper bound for every
/// suite the setup offers: 5 bytes of header, at most 8 of explicit nonce
/// (TLS 1.2 AES-GCM), 1 of inner content type (TLS 1.3) and 16 of
/// authentication tag are 30. The bound used is 64.
const RECORD_OVERHEAD_BOUND: usize = 64;

/// Room left for anything else rustls may queue, such as an alert.
const OUTPUT_SLACK: usize = 4_096;

/// Size of rustls' outgoing buffer for requests of up to
/// `max_plaintext_request_len` bytes: the plaintext, the record overhead of
/// as many records as it needs, and some slack.
///
/// rustls compares the length of new plaintext with the room left below
/// this limit in its buffer of ciphertext. The transport prepares a request
/// only into an empty buffer, so any request within the plaintext limit is
/// taken whole; the overhead and the slack keep the encrypted request
/// itself within the limit as well.
pub fn tls_output_capacity(max_plaintext_request_len: usize) -> usize {
    let records = max_plaintext_request_len.div_ceil(MAX_RECORD_PAYLOAD);
    max_plaintext_request_len
        .saturating_add(records.saturating_mul(RECORD_OVERHEAD_BOUND))
        .saturating_add(OUTPUT_SLACK)
}

/// Reason a policy is not usable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    /// A duration or the request size is zero.
    Zero,
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("tls policy: zero duration or size")
    }
}

impl std::error::Error for PolicyError {}

impl TlsPolicy {
    /// Fails for a zero duration or a zero request size.
    pub fn validate(&self) -> Result<(), PolicyError> {
        let zero = self.connect_timeout.is_zero()
            || self.handshake_timeout.is_zero()
            || self.max_idle.is_zero()
            || self.response_timeout.is_zero()
            || self.max_plaintext_request_len == 0;
        if zero { Err(PolicyError::Zero) } else { Ok(()) }
    }
}

/// Reason an endpoint or a provider target is not usable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetError {
    /// The host is not a DNS name.
    InvalidHost,
    /// The port is zero.
    InvalidPort,
    /// The authorization is empty or has a character a header cannot hold.
    InvalidAuthorization,
}

impl fmt::Display for TargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidHost => "tls target: host is not a DNS name",
            Self::InvalidPort => "tls target: port zero",
            Self::InvalidAuthorization => "tls target: unusable authorization",
        })
    }
}

impl std::error::Error for TargetError {}

/// Where a connection goes: a DNS name, which is both the TLS server name
/// the certificate must match and the name resolved, and a port.
#[derive(Clone, Debug)]
pub struct TlsEndpoint {
    host: String,
    server_name: ServerName<'static>,
    port: u16,
    connect_to: Option<SocketAddr>,
}

impl TlsEndpoint {
    /// An endpoint for the DNS name `host` on `port`.
    pub fn new(host: &str, port: u16) -> Result<Self, TargetError> {
        if port == 0 {
            return Err(TargetError::InvalidPort);
        }
        let printable = !host.is_empty() && host.bytes().all(|b| (0x21..=0x7e).contains(&b));
        if !printable {
            return Err(TargetError::InvalidHost);
        }
        let server_name = match ServerName::try_from(host) {
            Ok(ServerName::DnsName(name)) => ServerName::DnsName(name.to_owned()),
            _ => return Err(TargetError::InvalidHost),
        };
        Ok(Self {
            host: host.to_owned(),
            server_name,
            port,
            connect_to: None,
        })
    }

    /// The same endpoint, connected at `address` instead of resolving the
    /// host. The certificate must still match the host.
    pub fn with_address(mut self, address: SocketAddr) -> Self {
        self.connect_to = Some(address);
        self
    }

    /// The DNS name.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The fixed address, if one was given.
    pub fn connect_to(&self) -> Option<SocketAddr> {
        self.connect_to
    }

    pub(crate) fn server_name(&self) -> ServerName<'static> {
        self.server_name.clone()
    }
}

/// The value of an `Authorization` header. A secret: its `Debug` output is
/// redacted, and it is only given out where a request is framed.
#[derive(Clone)]
pub struct AuthorizationToken(String);

impl AuthorizationToken {
    /// A token. Fails for an empty value and for a character that is not
    /// printable ASCII or a space.
    pub fn new(value: String) -> Result<Self, TargetError> {
        let usable = !value.is_empty()
            && value
                .bytes()
                .all(|b| (0x21..=0x7e).contains(&b) || b == b' ');
        if usable {
            Ok(Self(value))
        } else {
            Err(TargetError::InvalidAuthorization)
        }
    }

    /// The value, for framing a request.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AuthorizationToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthorizationToken(redacted)")
    }
}

/// One builder as the runner reaches it: its kind, its TLS endpoint, the
/// request path and, for BlockRazor, the authorization.
///
/// The HTTP `Host` header of every request framed for it is the TLS
/// endpoint's host: the name the certificate was checked for.
#[derive(Clone)]
pub struct ProviderTarget {
    kind: AdapterKind,
    endpoint: TlsEndpoint,
    path: &'static str,
    authorization: Option<AuthorizationToken>,
}

impl ProviderTarget {
    /// The 48Club Puissant builder at `endpoint`. It takes no
    /// authorization.
    pub fn club48(endpoint: TlsEndpoint) -> Self {
        Self {
            kind: AdapterKind::Club48Builder,
            endpoint,
            path: "/",
            authorization: None,
        }
    }

    /// The BlockRazor block builder at `endpoint`, with its authorization.
    pub fn blockrazor(endpoint: TlsEndpoint, authorization: AuthorizationToken) -> Self {
        Self {
            kind: AdapterKind::BlockRazorBuilder,
            endpoint,
            path: "/",
            authorization: Some(authorization),
        }
    }

    /// The adapter kind.
    pub fn kind(&self) -> AdapterKind {
        self.kind
    }

    /// The TLS endpoint.
    pub fn tls_endpoint(&self) -> &TlsEndpoint {
        &self.endpoint
    }

    /// The endpoint for framing a request to this target.
    pub fn frame_endpoint(&self) -> Endpoint<'_> {
        Endpoint {
            host: self.endpoint.host(),
            path: self.path,
            authorization: self.authorization.as_ref().map(AuthorizationToken::expose),
        }
    }
}

impl fmt::Debug for ProviderTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderTarget")
            .field("kind", &self.kind)
            .field("endpoint", &self.endpoint)
            .field("path", &self.path)
            .field("authorization", &self.authorization)
            .finish()
    }
}

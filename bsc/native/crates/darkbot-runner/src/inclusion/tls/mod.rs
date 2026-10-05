//! The production transport of the two builders: HTTP/1.1 over TLS over
//! TCP, kept warm.
//!
//! # Who does what
//!
//! - [`TlsConnectionOwner`], one per builder, owns everything that takes
//!   time or can block: name resolution, TCP connect, the TLS handshake,
//!   probing an idle connection, giving up a connection whose response is
//!   overdue, and reconnecting. It does so at startup and in maintenance,
//!   never in a dispatch.
//! - [`TlsTransport`] is the runner's `Transport` over the owner's current
//!   connection. A dispatch borrows it by `&mut` from the owner: no lock,
//!   channel or thread in between. If it is not ready, the dispatch skips
//!   it; a dispatch never connects.
//! - [`WarmTlsConnection`] is one established connection: a non-blocking
//!   TCP socket and a rustls client connection with the handshake done.
//!
//! # TLS
//!
//! rustls 0.23.45 with the ring provider, TLS 1.3 and 1.2, certificates
//! verified against the trust anchors of the [`TlsSetup`] (the Mozilla
//! roots for production, test anchors for tests). ALPN offers `http/1.1`
//! only, and a connection on which the server selected anything else is
//! refused: the request framing and the response parser are HTTP/1.1.
//!
//! The rustls stream adapters are not used on the send path: they drive
//! I/O to completion internally. The transport sends in two stages: it
//! encrypts the whole request into TLS records in `prepare`, before the
//! dispatch's last deadline check, and after that check its first `write`
//! is the first `write_tls` to the socket; `flush` drains the rest. A
//! `WouldBlock` or a partial write surfaces exactly where it happens, and
//! no encryption lies between the last check and the wire.
//!
//! # Not qualified
//!
//! Nothing here makes a transport qualified. A `TlsTransport` is a
//! `Transport` and can be used for a diagnostic dispatch; a release
//! dispatch needs a `QualifiedTransport`, which still has no production
//! constructor. That constructor needs evidence from a real qualification
//! of this implementation against each builder, bound to what
//! [`TlsProfile`] lists and to the adapter, kind, capability generation,
//! handoff boundary and qualification generation.
//!
//! # Credentials
//!
//! The transport holds none. The BlockRazor authorization is part of the
//! request bytes, framed from a [`ProviderTarget`]; nothing in this module
//! reads files or the environment, and no `Debug` output or error carries
//! a credential or request data.

mod config;
mod connection;
mod owner;
mod transport;

pub use config::{
    AuthorizationToken, BLOCKRAZOR_DOCUMENTED_HOST, CLUB48_DOCUMENTED_HOST, HTTPS_PORT,
    PolicyError, ProviderTarget, SetupError, TargetError, TlsEndpoint, TlsPolicy, TlsSetup,
    tls_output_capacity,
};
pub use connection::{
    ConnectError, ConnectionFacts, HttpVersion, TLS_TRANSPORT_IMPLEMENTATION, WarmTlsConnection,
};
pub use owner::{Maintenance, TlsConnectionOwner, TlsProfile};
pub use transport::TlsTransport;

//! The native WSS pending-transaction source: one adapter for a JSON-RPC
//! WebSocket provider's `newPendingTransactions` stream.
//!
//! Transport: TCP with a connect timeout, this crate's rustls client (ring,
//! webpki roots, ALPN `http/1.1`), and the client handshake and framing of
//! `tungstenite` over that TLS stream. Protocol: `eth_subscribe` with
//! `["newPendingTransactions", true]`; the confirmed subscription is
//! reported as [`WssEvent::Connected`], each notification as a hash
//! ([`WssEvent::HashOnly`]) or a full transaction.
//!
//! JSON exists only at that wire: each frame is converted at once into typed
//! values (`jsonrpc`). A full transaction object is never passed on as it
//! came: `verify` rebuilds its canonical signed bytes (legacy EIP-155,
//! EIP-2930, EIP-1559), requires their Keccak-256 to be the announced hash,
//! recovers the signer and requires it to be the announced sender, and
//! cross-checks the bytes with the independent envelope decoder of
//! `darkbot-bsc-geth-ipc`. Anything else is [`WssEvent::Rejected`].
//!
//! Bounds: every timeout, the frame and message size and the reconnect
//! backoff come from [`WssLimits`], which the caller fills. `stop` is
//! observed at least once per `read_tick` while connected, handshaking or
//! waiting, plus at most one `write_timeout` for the closing frame; a TCP
//! connect may take up to `connect_timeout`. Name resolution is the
//! operating system's and has no bound of its own.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use darkbot_bsc_trading::signer::SignerError;

pub(crate) mod endpoint;
mod jsonrpc;
mod session;
mod verify;

#[cfg(test)]
mod tests_live;
#[cfg(test)]
mod tests_session;
#[cfg(test)]
mod tests_verify;

pub(crate) use jsonrpc::{ParsedTx, parse_tx_map, verify_parsed};
pub use verify::verify_tx_object;

/// The chain every full transaction of this source must be signed for.
pub const BSC_MAINNET_CHAIN_ID: u64 = 56;

/// One provider endpoint. `url` is `wss://host[:port][/path]`: no
/// credentials, no query, no fragment. The caller supplies it; no endpoint
/// is compiled in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WssSource {
    /// The source's operator-facing name (e.g. `BSC_INGRESS_PUBLICNODE`).
    pub name: &'static str,
    pub url: String,
}

/// Every bound of the adapter. The caller fills all of them; there are no
/// compiled-in operating values. [`WssLimits::validate`] states the rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WssLimits {
    /// One TCP connect attempt (per resolved address).
    pub connect_timeout: Duration,
    /// From the start of an attempt until the subscription is confirmed:
    /// TCP, TLS, the WebSocket upgrade and the `eth_subscribe` answer.
    pub handshake_timeout: Duration,
    /// The socket read timeout: how often `stop`, the handshake deadline
    /// and the idle bound are checked when nothing arrives.
    pub read_tick: Duration,
    /// The socket write timeout.
    pub write_timeout: Duration,
    /// A subscribed connection without any notification for this long is
    /// dropped as [`DisconnectReason::Idle`]. Pings do not count.
    pub idle_timeout: Duration,
    /// Largest accepted frame payload; also bounds the write buffer.
    pub max_frame_bytes: usize,
    /// Largest accepted (possibly fragmented) message.
    pub max_message_bytes: usize,
    /// First wait after a failed attempt; doubled after each further one.
    pub backoff_initial: Duration,
    /// Cap of the wait; also the wait after any rate-limit signal.
    pub backoff_max: Duration,
    /// A session that stayed subscribed this long resets the wait to
    /// `backoff_initial`.
    pub stable_after: Duration,
}

impl WssLimits {
    /// Every duration is non-zero, `backoff_initial <= backoff_max`, and
    /// `session::min_frame_bytes() <= max_frame_bytes <= max_message_bytes`
    /// (the lower bound is the subscribe request plus a frame header).
    pub fn validate(&self) -> Result<(), LimitsError> {
        let durations = [
            ("connect_timeout", self.connect_timeout),
            ("handshake_timeout", self.handshake_timeout),
            ("read_tick", self.read_tick),
            ("write_timeout", self.write_timeout),
            ("idle_timeout", self.idle_timeout),
            ("backoff_initial", self.backoff_initial),
            ("backoff_max", self.backoff_max),
            ("stable_after", self.stable_after),
        ];
        if let Some((name, _)) = durations.iter().find(|(_, d)| d.is_zero()) {
            return Err(LimitsError::ZeroDuration(name));
        }
        if self.backoff_initial > self.backoff_max {
            return Err(LimitsError::BackoffInitialAboveMax);
        }
        if self.max_frame_bytes < session::min_frame_bytes() {
            return Err(LimitsError::FrameBelowMinimum);
        }
        if self.max_frame_bytes > self.max_message_bytes {
            return Err(LimitsError::FrameAboveMessage);
        }
        Ok(())
    }
}

/// A configuration the adapter refuses before connecting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WssConfigError {
    Url(UrlError),
    Limits(LimitsError),
    /// The rustls client configuration could not be built.
    Tls,
}

/// Why a source URL is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UrlError {
    /// The scheme is not `wss://`.
    NotWss,
    /// The authority carries user information.
    Credentials,
    /// The URL has a query or a fragment.
    QueryOrFragment,
    /// The host is empty or not a plain DNS name / IPv4 address.
    Host,
    /// The port is not a decimal number in `1..=65535`.
    Port,
    /// The path contains a character outside visible ASCII.
    Path,
}

/// Which rule of [`WssLimits::validate`] is violated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitsError {
    ZeroDuration(&'static str),
    BackoffInitialAboveMax,
    FrameBelowMinimum,
    FrameAboveMessage,
}

/// What the source reports to its sink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WssEvent {
    /// The provider confirmed the subscription.
    Connected,
    /// A full pending transaction whose bytes, hash and signer were verified.
    Full(VerifiedTx),
    /// The provider sent only a transaction hash (unverifiable, as sent).
    HashOnly([u8; 32]),
    /// A notification that failed parsing or verification. Counted, never
    /// used.
    Rejected(RejectReason),
    /// One connection attempt ended. Every attempt ends with exactly one.
    Disconnected(DisconnectReason),
}

/// A pending transaction rebuilt from the provider's fields and verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedTx {
    /// Keccak-256 of `raw`, equal to the announced hash.
    pub hash: [u8; 32],
    /// The signer recovered from the signature, equal to the announced one.
    pub from: [u8; 20],
    /// `None` for a contract creation.
    pub to: Option<[u8; 20]>,
    pub nonce: u64,
    /// The chain the signature commits to (EIP-155 `v` for legacy).
    pub chain_id: Option<u64>,
    /// `None` for legacy, the EIP-2718 type byte otherwise.
    pub tx_type: Option<u8>,
    /// The canonical signed transaction bytes.
    pub raw: Vec<u8>,
}

/// A JSON field of a transaction object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxField {
    Type,
    Hash,
    From,
    To,
    Nonce,
    Gas,
    GasPrice,
    MaxFeePerGas,
    MaxPriorityFeePerGas,
    Value,
    Input,
    ChainId,
    AccessList,
    V,
    YParity,
    R,
    S,
}

/// Why a notification is not a verified transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectReason {
    /// The notification's `result` is neither a hash nor an object.
    NotATransaction,
    /// A required field is absent or null.
    Missing(TxField),
    /// A field is not the expected hex quantity or data of its width.
    Malformed(TxField),
    /// A transaction type this source does not rebuild (EIP-4844, EIP-7702,
    /// unknown).
    UnsupportedType(u8),
    /// A legacy transaction without EIP-155 replay protection (`v` 27/28).
    PreEip155,
    /// Signed for another chain.
    WrongChain(u64),
    /// A legacy object whose `chainId` disagrees with its EIP-155 `v`.
    ChainIdMismatch,
    /// A typed object whose `v` and `yParity` disagree, or are not 0/1.
    YParityMismatch,
    /// Keccak-256 of the rebuilt bytes is not the announced hash.
    HashMismatch,
    /// The signature does not recover (invalid scalars, high `s`, ...).
    Signature(SignerError),
    /// The recovered signer is not the announced `from`.
    SignerMismatch,
    /// The independent envelope decoder disagrees with the rebuilt bytes.
    Envelope,
}

/// Which signal reported rate limiting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RateLimitSignal {
    /// HTTP 429 in place of the WebSocket upgrade.
    Http429,
    /// A JSON-RPC error with a rate-limit code (-32005, 429).
    RpcError(i64),
    /// A close frame with 1013 (Try Again Later).
    CloseCode(u16),
}

/// A violation of the WebSocket or JSON-RPC protocol by the provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolFault {
    /// A framing, UTF-8 or handshake-format violation (tungstenite).
    WebSocket,
    /// A frame or message above the configured bound.
    FrameTooLarge,
    /// A frame that is not a JSON value.
    MalformedJson,
    /// Valid JSON that is neither the subscribe answer, an error, nor a
    /// notification.
    UnexpectedMessage,
    /// `eth_subscribe` answered with this JSON-RPC error code.
    SubscribeRefused(i64),
    /// A JSON-RPC error after the subscription.
    RpcError(i64),
    /// A notification for a subscription this connection does not hold.
    UnknownSubscription,
}

/// How a connection attempt ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisconnectReason {
    /// The peer closed: a close frame with its code, or the connection
    /// ended without one (`None`).
    Closed(Option<u16>),
    /// A socket error (connect, read, write, name resolution).
    Io(io::ErrorKind),
    /// Name resolution returned no address.
    NoAddress,
    /// The TLS layer failed (certificate, alert, record).
    Tls,
    /// The upgrade was answered with this HTTP status instead of 101.
    Handshake(u16),
    RateLimited(RateLimitSignal),
    Protocol(ProtocolFault),
    /// No subscription confirmation within `handshake_timeout`.
    Timeout,
    /// No notification within `idle_timeout`.
    Idle,
    /// `stop` was observed.
    Stopped,
}

/// Runs the source until `stop` is set: connect, subscribe, read; after
/// each ended attempt wait the backoff (interruptible by `stop`) and
/// reconnect. Every event goes to `sink` on the calling thread.
///
/// Returns `Err` only for a configuration refused before any connect.
pub fn run_source(
    source: &WssSource,
    limits: &WssLimits,
    stop: &AtomicBool,
    sink: &mut dyn FnMut(WssEvent),
) -> Result<(), WssConfigError> {
    let endpoint = endpoint::Endpoint::parse(&source.url).map_err(WssConfigError::Url)?;
    limits.validate().map_err(WssConfigError::Limits)?;
    let tls = endpoint::tls_config().map_err(|_| WssConfigError::Tls)?;
    session::run_loop(limits, stop, sink, |started| {
        let stream = endpoint::connect_tls(&endpoint, &tls, limits, stop, started)?;
        session::ws_handshake(stream, endpoint.url(), limits, stop, started)
    });
    Ok(())
}

/// One load of the stop flag; every wait in this module goes through it.
fn stopped(stop: &AtomicBool) -> bool {
    stop.load(Ordering::Acquire)
}

//! The native canonical chain follower: the G0.4 denominator source.
//!
//! It answers from the canonical chain alone which launches really
//! happened: in which canonical block, at which transaction index, and how
//! many buys of the new token by other transactions followed in that block.
//!
//! Transport: HTTPS JSON-RPC, HTTP/1.1 `POST` over one keep-alive rustls
//! connection (the TCP connect and TLS client of [`crate::wss`]'s endpoint),
//! replaced after any failure. Responses are framed by `Content-Length` or
//! `chunked`; every response is bounded in size and time.
//!
//! Protocol: `eth_blockNumber` every [`FollowerLimits::poll`]; for every new
//! block, in order, `eth_getBlockByNumber(n, false)` and
//! `eth_getBlockReceipts(n)`. JSON exists only at that wire (`jsonrpc`):
//! every field is checked for its exact shape (canonical lowercase hex
//! quantities, fixed-width data) and converted at once into typed values.
//! The receipts must be exactly the block's transactions, in order, bound to
//! the block's hash and number, with strictly increasing log indices;
//! otherwise nothing of the block is used and the block is fetched again.
//!
//! Canonicity: the follower keeps the last [`FollowerLimits::reorg_depth`]
//! blocks plus their parent (the deepest fork point it can still prove). A
//! new block whose parent hash is not the kept head starts a walk back
//! along the new chain's parent hashes until a kept block is reached:
//! [`CanonicalEvent::Reorg`] names the orphaned blocks, then the new chain's
//! blocks follow. If no kept block is reached, [`CanonicalEvent::ReorgTooDeep`]
//! names every kept block as orphaned and following resumes at the new
//! head. Heights that are never followed are always reported
//! ([`CanonicalEvent::Gap`], [`CanonicalEvent::ReorgTooDeep`]); nothing is
//! skipped silently. Following starts at the head first reported.
//!
//! Launches: exactly the venues of [`launches::LAUNCH_EVENTS`], each by
//! contract address and event topic, in successful receipts only. Each
//! launch's transaction is fetched (`eth_getTransactionByHash`), bound to
//! its receipt and verified by the WSS source's verifier for chain 56 (the
//! signed bytes rebuilt, hash and signer checked); its sender, nonce and C1
//! scope come from those verified bytes. A block is reported only with all
//! of its launches verified.
//!
//! Time: a block's `timestamp_ms` is bsc-geth's `Header.MilliTimestamp()`
//! (v1.7.8, `core/types/block.go`): `timestamp * 1000` plus the 256-bit
//! big-endian `mixHash` when it is non-zero, which must be below 1000.
//! `observed`/`observed_unix_ms` are the monotonic and wall clock readings
//! taken together when the follower first held the header.
//!
//! Bounds: every timeout, size, depth and wait comes from [`FollowerLimits`],
//! which the caller fills. `stop` is observed at least once per `read_tick`
//! while a request runs or the follower waits; a connect may take up to
//! `connect_timeout` per resolved address and at most `handshake_timeout`
//! in all. Name resolution is the operating system's and has no bound of
//! its own.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::wss::endpoint::{Endpoint, connect_tls, tls_config};
use crate::wss::{DisconnectReason, RateLimitSignal, RejectReason, UrlError, WssLimits};

mod follow;
mod http;
mod jsonrpc;
pub mod launches;

#[cfg(test)]
mod tests_follow;
#[cfg(test)]
mod tests_launches;
#[cfg(test)]
mod tests_live;
#[cfg(test)]
mod tests_wire;

pub use launches::Venue;

/// Every bound of the follower. The caller fills all of them; there are no
/// compiled-in operating values. [`FollowerLimits::validate`] states the
/// rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FollowerLimits {
    /// The interval of `eth_blockNumber` once the follower has caught up.
    pub poll: Duration,
    /// The deepest reorg (orphaned blocks) the follower resolves; deeper
    /// ones are [`CanonicalEvent::ReorgTooDeep`].
    pub reorg_depth: u64,
    /// The most blocks followed after one head report; a head further ahead
    /// is a [`CanonicalEvent::Gap`] and following resumes at that head.
    pub max_catch_up: u64,
    /// One TCP connect attempt (per resolved address).
    pub connect_timeout: Duration,
    /// From the start of a connection attempt until TLS is established.
    pub handshake_timeout: Duration,
    /// The socket read timeout: how often `stop` and the request deadline
    /// are checked while waiting for bytes.
    pub read_tick: Duration,
    /// The socket write timeout.
    pub write_timeout: Duration,
    /// From the start of a request on a connected stream until its whole
    /// response has arrived.
    pub request_timeout: Duration,
    /// Largest accepted response (status line, headers, framing and body).
    pub max_response_bytes: usize,
    /// First wait after a failed step; doubled after each further one.
    pub backoff_initial: Duration,
    /// Cap of the wait; also the wait after any rate-limit signal.
    pub backoff_max: Duration,
}

impl FollowerLimits {
    /// Every duration is non-zero, `reorg_depth`, `max_catch_up` and
    /// `max_response_bytes` are at least 1, and
    /// `backoff_initial <= backoff_max`.
    pub fn validate(&self) -> Result<(), LimitsError> {
        let durations = [
            ("poll", self.poll),
            ("connect_timeout", self.connect_timeout),
            ("handshake_timeout", self.handshake_timeout),
            ("read_tick", self.read_tick),
            ("write_timeout", self.write_timeout),
            ("request_timeout", self.request_timeout),
            ("backoff_initial", self.backoff_initial),
            ("backoff_max", self.backoff_max),
        ];
        if let Some((name, _)) = durations.iter().find(|(_, d)| d.is_zero()) {
            return Err(LimitsError::ZeroDuration(name));
        }
        if self.reorg_depth == 0 {
            return Err(LimitsError::Zero("reorg_depth"));
        }
        if self.max_catch_up == 0 {
            return Err(LimitsError::Zero("max_catch_up"));
        }
        if self.max_response_bytes == 0 {
            return Err(LimitsError::Zero("max_response_bytes"));
        }
        if self.backoff_initial > self.backoff_max {
            return Err(LimitsError::BackoffInitialAboveMax);
        }
        Ok(())
    }
}

/// Which rule of [`FollowerLimits::validate`] is violated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitsError {
    ZeroDuration(&'static str),
    Zero(&'static str),
    BackoffInitialAboveMax,
}

/// A configuration the follower refuses before connecting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowerConfigError {
    /// The endpoint is not `https://`.
    NotHttps,
    /// The authority or path of the endpoint is refused (the `wss://`
    /// rules of the same authority and path).
    Url(UrlError),
    Limits(LimitsError),
    /// The rustls client configuration could not be built.
    Tls,
}

/// What the follower reports, in chain order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalEvent {
    /// A block of the canonical chain, once, after its receipts were
    /// verified. Its launches follow it.
    Block {
        number: u64,
        hash: [u8; 32],
        parent_hash: [u8; 32],
        timestamp_s: u64,
        /// The header's millisecond timestamp (bsc-geth `MilliTimestamp`).
        timestamp_ms: u64,
        /// When this follower first held this block's header.
        observed: Instant,
        /// The wall clock (Unix ms) read next to `observed`; `None` when
        /// the system clock could not be read.
        observed_unix_ms: Option<u64>,
    },
    Launch(CanonicalLaunch),
    /// Earlier reported blocks left the canonical chain: `orphaned` in
    /// ascending order, the fork point is the block below the first. The
    /// new chain's blocks up to `new_head` (the block that revealed the
    /// reorg) follow as [`CanonicalEvent::Block`]s.
    Reorg {
        orphaned: Vec<(u64, [u8; 32])>,
        new_head: (u64, [u8; 32]),
    },
    /// No kept block is an ancestor of `new_head`: every kept block in
    /// `orphaned` left the chain, and the fork point is below them. The new
    /// chain's blocks at and below the orphaned heights are not followed;
    /// following resumes at `new_head`, whose [`CanonicalEvent::Block`]
    /// follows.
    ReorgTooDeep {
        orphaned: Vec<(u64, [u8; 32])>,
        new_head: (u64, [u8; 32]),
    },
    /// The canonical blocks `from..=to` are not followed (the head moved
    /// more than `max_catch_up` ahead, or followed only after a resync).
    Gap {
        from: u64,
        to: u64,
    },
    /// A step failed; the follower waits its backoff and tries again.
    Fault(FollowerFault),
}

/// One launch in a canonical block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalLaunch {
    pub venue: Venue,
    pub block_number: u64,
    pub block_hash: [u8; 32],
    pub tx_index: u64,
    pub tx_hash: [u8; 32],
    /// The new token, `None` when the venue's event words are not
    /// canonical (never guessed).
    pub token: Option<[u8; 20]>,
    pub creator: Option<[u8; 20]>,
    /// Buys of `token` by later transactions of the same block. `None` when
    /// the venue's buy event is not decoded (Four.meme) or a buy event of
    /// the venue in a later transaction is malformed: unknown, never 0.
    pub external_buys_after: Option<u32>,
    /// The transaction index of the first of those buys.
    pub first_external_buy_index: Option<u64>,
    /// The verified signer of the launch transaction.
    pub sender: [u8; 20],
    pub nonce: u64,
    /// The launch transaction is in the C1 scope (`ingress::is_flap_launch`
    /// of its verified signed bytes: a direct Portal `newTokenV6` call).
    pub c1_scope: bool,
}

/// Why a step of the follower failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowerFault {
    /// A request failed.
    Request { call: Call, error: RequestError },
    /// The provider answered `null` for a block it reported (a lagging
    /// backend, or the block left the chain in between).
    Unavailable(Call),
    /// The receipts are not exactly the block's transactions (count, order,
    /// hashes, block hash or number, log order).
    ReceiptsMismatch(u64),
    /// During a reorg walk the parent link of the new chain broke at this
    /// height: the chain moved again; the step is retried.
    ChainMoved(u64),
}

/// The request a fault belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    BlockNumber,
    Header(u64),
    Receipts(u64),
    /// The transaction of a launch, by hash.
    Transaction([u8; 32]),
}

/// Why one request failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestError {
    /// Connect, TLS, socket error, peer close, request timeout or `stop`.
    Transport(DisconnectReason),
    /// HTTP 429, or a JSON-RPC error with a rate-limit code.
    RateLimited(RateLimitSignal),
    /// A status other than 200 and 429.
    HttpStatus(u16),
    Http(HttpFault),
    /// The body is not a JSON value.
    MalformedJson,
    /// A JSON-RPC error with another code.
    Rpc(i64),
    /// A field of the answer has the wrong shape.
    Shape(Field),
    /// The transaction object failed the WSS verifier.
    Unverifiable(RejectReason),
}

/// A violation of HTTP/1.1 framing by the provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpFault {
    /// The status line is not `HTTP/1.x <3 digits> ...`.
    StatusLine,
    /// A header line is malformed or not ASCII.
    Header,
    /// Missing, invalid or conflicting `Content-Length`/`Transfer-Encoding`.
    Framing,
    /// A transfer or content coding other than `chunked`/identity.
    Coding,
    /// A malformed chunk.
    Chunk,
    /// The response is larger than `max_response_bytes`.
    TooLarge,
    /// Bytes after the end of the response.
    Trailing,
}

/// A field of a JSON-RPC answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    /// The answer is not an object with `"jsonrpc":"2.0"`.
    Envelope,
    /// The `id` is not the request's.
    Id,
    /// The `error` object has no integer `code`.
    ErrorCode,
    /// `result` is missing or of the wrong JSON type.
    Result,
    Number,
    Hash,
    ParentHash,
    Timestamp,
    /// Not 32 bytes, or a millisecond value of 1000 or more.
    MixHash,
    Transactions,
    TxHash,
    TxIndex,
    BlockHash,
    BlockNumber,
    Status,
    Logs,
    LogAddress,
    LogTopics,
    LogData,
    LogIndex,
    LogRemoved,
    /// A log's block or transaction fields differ from its receipt's.
    LogBinding,
    /// A transaction's hash, block or index differ from its receipt's.
    TxBinding,
}

/// Follows the canonical chain at `endpoint` (`https://host[:port][/path]`;
/// the caller supplies it, no endpoint is compiled in) until `stop` is
/// set. Every event goes to `sink` on the calling thread.
///
/// Returns `Err` only for a configuration refused before any connect.
pub fn run_follower(
    endpoint: &str,
    limits: &FollowerLimits,
    stop: &AtomicBool,
    sink: &mut dyn FnMut(CanonicalEvent),
) -> Result<(), FollowerConfigError> {
    let rest = endpoint
        .strip_prefix("https://")
        .ok_or(FollowerConfigError::NotHttps)?;
    // `https://` and `wss://` share the authority and path rules and the
    // default port 443: the WSS endpoint checks and connects both.
    let target = Endpoint::parse(&format!("wss://{rest}")).map_err(FollowerConfigError::Url)?;
    limits.validate().map_err(FollowerConfigError::Limits)?;
    let tls = tls_config().map_err(|_| FollowerConfigError::Tls)?;
    let (authority, path) = authority_and_path(target.url());
    let bounds = connect_bounds(limits);
    let connect = |stop: &AtomicBool| connect_tls(&target, &tls, &bounds, stop, Instant::now());
    let client = http::Client::new(connect, authority, path, limits);
    follow::run(&mut follow::Rpc::new(client), limits, stop, sink);
    Ok(())
}

/// `wss://authority/path` (as [`Endpoint::url`] normalizes it) split into
/// the `Host` header value and the request target.
fn authority_and_path(url: &str) -> (String, String) {
    let rest = url.strip_prefix("wss://").unwrap_or(url);
    match rest.find('/') {
        Some(at) => (rest[..at].to_owned(), rest[at..].to_owned()),
        None => (rest.to_owned(), "/".to_owned()),
    }
}

/// The bounds [`connect_tls`] applies: it reads `connect_timeout`,
/// `handshake_timeout`, `read_tick` and `write_timeout`. The other fields
/// are not read by the connect; they carry the follower's own equivalents.
fn connect_bounds(limits: &FollowerLimits) -> WssLimits {
    WssLimits {
        connect_timeout: limits.connect_timeout,
        handshake_timeout: limits.handshake_timeout,
        read_tick: limits.read_tick,
        write_timeout: limits.write_timeout,
        idle_timeout: limits.request_timeout,
        max_frame_bytes: limits.max_response_bytes,
        max_message_bytes: limits.max_response_bytes,
        backoff_initial: limits.backoff_initial,
        backoff_max: limits.backoff_max,
        stable_after: limits.poll,
    }
}

/// One load of the stop flag; every wait in this module goes through it.
fn stopped(stop: &AtomicBool) -> bool {
    stop.load(Ordering::Acquire)
}

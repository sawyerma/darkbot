//! Bounded request frames.
//!
//! A [`Frame`] is the complete byte string one adapter will write for one
//! plan. Everything in it that is known before the trigger is written
//! before the trigger: the HTTP head, the JSON envelope with exactly the
//! options the plan derived for this adapter, and the original launch
//! bytes. One part cannot be: the signed BUY. The intent lane releases
//! those bytes only against a consumed permit, so the frame keeps a slot of
//! the right size and the BUY is encoded into it on the entry path, after
//! `start_write` and before the first transport write. That is a bounded
//! copy into storage that already exists; nothing is allocated.
//!
//! After the write the slot is cleared again. The lane stays the only
//! holder of the signed BUY.
//!
//! A frame is built for one plan and one adapter and carries both; the
//! send path refuses a frame that belongs to another plan.
//!
//! The JSON-RPC `id` of a request is the hash of the plan's BUY, so that a
//! response can be matched to exactly this submission: a response to any
//! other request has another `id` and is refused.
//!
//! The raw request is not public: it carries the adapter's authorization.
//! [`Frame::redacted_request`] shows it with the authorization replaced.

use darkbot_bsc_trading::intent::AdapterId;
use darkbot_bsc_trading::submission_policy::capability::{
    AdapterKind, AdapterTable, SubmissionMode,
};
use darkbot_bsc_trading::submission_policy::plan::{
    EnvelopeOptions, ProviderValidity, SubmissionPlan,
};

/// Where a request goes and how it authenticates. Operating configuration,
/// supplied by the caller; nothing here is a default.
#[derive(Clone, Copy)]
pub struct Endpoint<'a> {
    /// Value of the `Host` header.
    pub host: &'a str,
    /// Request path.
    pub path: &'a str,
    /// Value of the `Authorization` header, if the adapter needs one. A
    /// secret: it is never logged and a frame has no `Debug` output.
    pub authorization: Option<&'a str>,
}

/// Reason a frame cannot be built or filled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// The capability table is not the one the plan was formed against.
    StaleCapabilities,
    /// The adapter is not part of the plan.
    AdapterNotInPlan,
    /// The adapter is of another kind than this framing is for.
    WrongAdapterKind,
    /// The plan's mode is not one this framing carries.
    UnsupportedMode,
    /// The launch bytes are not the transaction the plan refers to.
    LaunchBytesMismatch,
    /// A bundle without a provider validity.
    MissingProviderValidity,
    /// The adapter needs an authorization and none was given.
    MissingAuthorization,
    /// An authorization was given for an adapter that takes none.
    UnexpectedAuthorization,
    /// Host, path or authorization is empty or has a character that does
    /// not belong into a header.
    InvalidEndpoint,
    /// The frame does not fit the declared capacity.
    Oversize,
    /// The BUY bytes do not have the length the frame was built for.
    BuyLengthMismatch,
}

impl core::fmt::Display for FrameError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::StaleCapabilities => "frame: capability table changed",
            Self::AdapterNotInPlan => "frame: adapter is not part of the plan",
            Self::WrongAdapterKind => "frame: adapter is of another kind",
            Self::UnsupportedMode => "frame: mode not carried by this framing",
            Self::LaunchBytesMismatch => "frame: launch bytes are not the planned launch",
            Self::MissingProviderValidity => "frame: bundle without provider validity",
            Self::MissingAuthorization => "frame: authorization missing",
            Self::UnexpectedAuthorization => "frame: authorization not expected",
            Self::InvalidEndpoint => "frame: invalid host, path or authorization",
            Self::Oversize => "frame: exceeds the declared capacity",
            Self::BuyLengthMismatch => "frame: BUY length differs from the plan",
        })
    }
}

impl std::error::Error for FrameError {}

/// Length of a request id: `0x` and 64 hex digits.
const REQUEST_ID_LEN: usize = 66;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// The BUY as lowercase hex digits inside a JSON string.
    Hex,
    /// The BUY as it is.
    Raw,
}

/// The complete request of one adapter for one plan, with a slot for the
/// signed BUY.
pub struct Frame {
    plan: SubmissionPlan,
    adapter: AdapterId,
    kind: AdapterKind,
    buf: Vec<u8>,
    slot: Slot,
    slot_start: usize,
    slot_len: usize,
    filled: bool,
    request_id: [u8; REQUEST_ID_LEN],
    secret: Option<(usize, usize)>,
}

impl Frame {
    /// The plan the frame was built for.
    #[inline]
    pub fn plan(&self) -> &SubmissionPlan {
        &self.plan
    }

    /// The adapter the frame was built for.
    #[inline]
    pub fn adapter(&self) -> AdapterId {
        self.adapter
    }

    /// Kind of that adapter.
    #[inline]
    pub fn kind(&self) -> AdapterKind {
        self.kind
    }

    /// Length of the complete request in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether the request is empty. It never is.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Whether the BUY slot currently holds the signed BUY.
    #[inline]
    pub fn is_filled(&self) -> bool {
        self.filled
    }

    /// The JSON-RPC `id` of the request: the BUY's hash as a hex string.
    #[inline]
    pub fn request_id(&self) -> &[u8] {
        &self.request_id
    }

    /// A copy of the request for inspection, with the authorization value
    /// replaced by `*` of the same length and the BUY slot as it is
    /// outside a dispatch: zeros. Allocates; not for the send path.
    pub fn redacted_request(&self) -> Vec<u8> {
        let mut copy = self.buf.clone();
        if let Some((start, end)) = self.secret {
            copy[start..end].fill(b'*');
        }
        copy
    }

    /// The request to write. Complete only while the slot is filled.
    #[inline]
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.buf
    }

    /// Encodes the signed BUY into its slot. No allocation.
    pub(crate) fn fill_buy(&mut self, raw: &[u8]) -> Result<(), FrameError> {
        let slot = &mut self.buf[self.slot_start..self.slot_start + self.slot_len];
        match self.slot {
            Slot::Hex => {
                if raw.len() * 2 != slot.len() {
                    return Err(FrameError::BuyLengthMismatch);
                }
                for (pair, byte) in slot.chunks_exact_mut(2).zip(raw) {
                    pair[0] = HEX[usize::from(*byte >> 4)];
                    pair[1] = HEX[usize::from(*byte & 0x0f)];
                }
            }
            Slot::Raw => {
                if raw.len() != slot.len() {
                    return Err(FrameError::BuyLengthMismatch);
                }
                slot.copy_from_slice(raw);
            }
        }
        self.filled = true;
        Ok(())
    }

    /// Removes the signed BUY from the slot again.
    pub(crate) fn clear_buy(&mut self) {
        let fill = match self.slot {
            Slot::Hex => b'0',
            Slot::Raw => 0,
        };
        self.buf[self.slot_start..self.slot_start + self.slot_len].fill(fill);
        self.filled = false;
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Something request bytes are written to: a counter or a bounded buffer.
/// Framing runs once over each, so the announced length and the written
/// bytes come from the same code.
trait Out {
    fn put(&mut self, bytes: &[u8]) -> Result<(), FrameError>;
    /// Reserves `len` bytes filled with `fill` and returns their offset.
    fn reserve(&mut self, len: usize, fill: u8) -> Result<usize, FrameError>;
    /// Offset of the next byte.
    fn position(&self) -> usize;
}

struct Counter(usize);

impl Out for Counter {
    fn put(&mut self, bytes: &[u8]) -> Result<(), FrameError> {
        self.reserve(bytes.len(), 0).map(|_| ())
    }

    fn reserve(&mut self, len: usize, _fill: u8) -> Result<usize, FrameError> {
        let start = self.0;
        self.0 = start.checked_add(len).ok_or(FrameError::Oversize)?;
        Ok(start)
    }

    fn position(&self) -> usize {
        self.0
    }
}

/// A buffer that was allocated once and never grows.
struct Bounded {
    buf: Vec<u8>,
    capacity: usize,
}

impl Out for Bounded {
    fn put(&mut self, bytes: &[u8]) -> Result<(), FrameError> {
        if bytes.len() > self.capacity - self.buf.len() {
            return Err(FrameError::Oversize);
        }
        self.buf.extend_from_slice(bytes);
        Ok(())
    }

    fn reserve(&mut self, len: usize, fill: u8) -> Result<usize, FrameError> {
        if len > self.capacity - self.buf.len() {
            return Err(FrameError::Oversize);
        }
        let start = self.buf.len();
        self.buf.resize(start + len, fill);
        Ok(start)
    }

    fn position(&self) -> usize {
        self.buf.len()
    }
}

fn put_hex<O: Out>(out: &mut O, bytes: &[u8]) -> Result<(), FrameError> {
    let mut chunk = [0u8; 64];
    for part in bytes.chunks(32) {
        for (pair, byte) in chunk.chunks_exact_mut(2).zip(part) {
            pair[0] = HEX[usize::from(*byte >> 4)];
            pair[1] = HEX[usize::from(*byte & 0x0f)];
        }
        out.put(&chunk[..part.len() * 2])?;
    }
    Ok(())
}

fn put_u64<O: Out>(out: &mut O, mut value: u64) -> Result<(), FrameError> {
    let mut digits = [0u8; 20];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    out.put(&digits[start..])
}

/// The JSON-RPC body of `eth_sendBundle` for `[launch, BUY]`.
///
/// Members are written in a fixed order and only when the plan has them:
/// `maxBlockNumber` always, the timestamps when the plan sets them,
/// `noMerge` and `positionFirst` only when the plan's envelope for this
/// adapter says so. Nothing is added, defaulted or upgraded here.
/// `revertingTxHashes` is never written: no transaction may revert.
fn put_bundle_body<O: Out>(
    out: &mut O,
    request_id: &[u8],
    launch_raw: &[u8],
    buy_len: usize,
    validity: &ProviderValidity,
    options: EnvelopeOptions,
) -> Result<usize, FrameError> {
    out.put(b"{\"jsonrpc\":\"2.0\",\"id\":\"")?;
    out.put(request_id)?;
    out.put(b"\",\"method\":\"eth_sendBundle\",")?;
    out.put(b"\"params\":[{\"txs\":[\"0x")?;
    put_hex(out, launch_raw)?;
    out.put(b"\",\"0x")?;
    let hex_len = buy_len.checked_mul(2).ok_or(FrameError::Oversize)?;
    let slot_start = out.reserve(hex_len, b'0')?;
    out.put(b"\"],\"maxBlockNumber\":")?;
    put_u64(out, validity.max_block_number)?;
    if let Some(min_timestamp) = validity.min_timestamp {
        out.put(b",\"minTimestamp\":")?;
        put_u64(out, min_timestamp)?;
    }
    if let Some(max_timestamp) = validity.max_timestamp {
        out.put(b",\"maxTimestamp\":")?;
        put_u64(out, max_timestamp)?;
    }
    if options.no_merge {
        out.put(b",\"noMerge\":true")?;
    }
    if options.position_first {
        out.put(b",\"positionFirst\":true")?;
    }
    out.put(b"}]}")?;
    Ok(slot_start)
}

/// Writes the HTTP head and returns where the authorization value is.
fn put_head<O: Out>(
    out: &mut O,
    endpoint: &Endpoint<'_>,
    body_len: usize,
) -> Result<Option<(usize, usize)>, FrameError> {
    out.put(b"POST ")?;
    out.put(endpoint.path.as_bytes())?;
    out.put(b" HTTP/1.1\r\nHost: ")?;
    out.put(endpoint.host.as_bytes())?;
    out.put(b"\r\nContent-Type: application/json\r\nContent-Length: ")?;
    put_u64(
        out,
        u64::try_from(body_len).map_err(|_| FrameError::Oversize)?,
    )?;
    out.put(b"\r\n")?;
    let mut secret = None;
    if let Some(authorization) = endpoint.authorization {
        out.put(b"Authorization: ")?;
        let start = out.position();
        out.put(authorization.as_bytes())?;
        secret = Some((start, out.position()));
        out.put(b"\r\n")?;
    }
    out.put(b"\r\n")?;
    Ok(secret)
}

/// Whether `text` may stand in a request line or header: not empty,
/// printable ASCII only, and no space unless `spaces` allows it.
fn header_safe(text: &str, spaces: bool) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| (0x21..=0x7e).contains(&b) || (spaces && b == b' '))
}

/// Builds the `eth_sendBundle` frame of a builder adapter.
pub(crate) fn bundle_frame(
    plan: &SubmissionPlan,
    table: &AdapterTable,
    adapter: AdapterId,
    kind: AdapterKind,
    endpoint: &Endpoint<'_>,
    launch_raw: &[u8],
    capacity: usize,
) -> Result<Frame, FrameError> {
    let options = planned(plan, table, adapter, kind)?;
    match plan.mode() {
        SubmissionMode::PublicFullLaunchBundle | SubmissionMode::ControlledSelfLaunchBundle => {}
        SubmissionMode::SingleRawBuy | SubmissionMode::PrivateTarget => {
            return Err(FrameError::UnsupportedMode);
        }
    }
    // The launch goes out exactly as it is: the plan refers to these very
    // bytes, and they are only hex-encoded, never decoded or re-signed.
    let launch = plan.launch().tx().ok_or(FrameError::UnsupportedMode)?;
    if !launch.matches(launch_raw) {
        return Err(FrameError::LaunchBytesMismatch);
    }
    let validity = plan
        .provider_validity()
        .ok_or(FrameError::MissingProviderValidity)?;
    let path_ok = header_safe(endpoint.path, false) && endpoint.path.starts_with('/');
    let auth_ok = endpoint
        .authorization
        .is_none_or(|token| header_safe(token, true));
    if !header_safe(endpoint.host, false) || !path_ok || !auth_ok {
        return Err(FrameError::InvalidEndpoint);
    }
    let buy_len = plan.buy().len();
    let request_id = request_id(plan);

    // First pass: lengths only.
    let mut body = Counter(0);
    put_bundle_body(
        &mut body,
        &request_id,
        launch_raw,
        buy_len,
        &validity,
        options,
    )?;
    let mut head = Counter(0);
    put_head(&mut head, endpoint, body.0)?;
    let total = head.0.checked_add(body.0).ok_or(FrameError::Oversize)?;
    if total > capacity {
        return Err(FrameError::Oversize);
    }

    // Second pass: the bytes, into a buffer of exactly the declared size.
    let mut out = Bounded {
        buf: Vec::with_capacity(capacity),
        capacity,
    };
    let secret = put_head(&mut out, endpoint, body.0)?;
    let slot_start = put_bundle_body(
        &mut out,
        &request_id,
        launch_raw,
        buy_len,
        &validity,
        options,
    )?;
    if out.buf.len() != total {
        return Err(FrameError::Oversize);
    }
    Ok(Frame {
        plan: *plan,
        adapter,
        kind,
        buf: out.buf,
        slot: Slot::Hex,
        slot_start,
        slot_len: buy_len * 2,
        filled: false,
        request_id,
        secret,
    })
}

/// Builds the frame of an adapter that takes the signed BUY as it is.
pub(crate) fn raw_buy_frame(
    plan: &SubmissionPlan,
    table: &AdapterTable,
    adapter: AdapterId,
    kind: AdapterKind,
    capacity: usize,
) -> Result<Frame, FrameError> {
    planned(plan, table, adapter, kind)?;
    if plan.mode() != SubmissionMode::SingleRawBuy {
        return Err(FrameError::UnsupportedMode);
    }
    let buy_len = plan.buy().len();
    if buy_len > capacity {
        return Err(FrameError::Oversize);
    }
    Ok(Frame {
        plan: *plan,
        adapter,
        kind,
        buf: vec![0u8; buy_len],
        slot: Slot::Raw,
        slot_start: 0,
        slot_len: buy_len,
        filled: false,
        request_id: request_id(plan),
        secret: None,
    })
}

/// The request id of `plan`: `0x` and the BUY's hash in lowercase hex.
fn request_id(plan: &SubmissionPlan) -> [u8; REQUEST_ID_LEN] {
    let mut id = [0u8; REQUEST_ID_LEN];
    id[0] = b'0';
    id[1] = b'x';
    for (pair, byte) in id[2..]
        .chunks_exact_mut(2)
        .zip(plan.buy().hash().as_slice())
    {
        pair[0] = HEX[usize::from(*byte >> 4)];
        pair[1] = HEX[usize::from(*byte & 0x0f)];
    }
    id
}

/// Checks that `adapter` is of `kind` and part of `plan` under `table`, and
/// returns the envelope options the plan derived for it.
fn planned(
    plan: &SubmissionPlan,
    table: &AdapterTable,
    adapter: AdapterId,
    kind: AdapterKind,
) -> Result<EnvelopeOptions, FrameError> {
    if table.generation() != plan.capability_generation() {
        return Err(FrameError::StaleCapabilities);
    }
    let record = table.record(adapter).ok_or(FrameError::AdapterNotInPlan)?;
    if record.kind() != kind {
        return Err(FrameError::WrongAdapterKind);
    }
    if !plan.adapters().contains(adapter) {
        return Err(FrameError::AdapterNotInPlan);
    }
    plan.envelope(adapter).ok_or(FrameError::AdapterNotInPlan)
}

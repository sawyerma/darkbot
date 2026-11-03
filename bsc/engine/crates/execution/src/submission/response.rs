//! Bounded parsing of provider responses.
//!
//! A response says what the provider answered and nothing else. An
//! acknowledgement is not an inclusion, and a rejection is not evidence
//! that the submitted transaction will not execute. The outcome only moves
//! the submission state of one adapter; inclusion, revert, canonicality and
//! orphaning are decided by the chain-state owner alone.
//!
//! The parser handles what the two builders are documented to send: an
//! HTTP/1.1 response with a JSON-RPC body that has either a `result` or an
//! `error`. It reads into storage of a fixed size, accepts a body framed by
//! `Content-Length` or by chunks, and fails closed on everything else.
//!
//! # How a response is bound to its request
//!
//! Two layers, and they are stated separately because they bind
//! differently:
//!
//! - **The connection.** One request is in flight per connection: after a
//!   handoff the connection awaits its response and carries no other
//!   request until that response was read completely. HTTP/1.1 answers in
//!   order, and a connection whose response could not be understood is
//!   never used again. So the next HTTP response on a connection is the
//!   answer to its request in flight. This is the only binding of an
//!   HTTP-level answer: a `429` is taken as the provider's refusal of that
//!   request after its HTTP body is completely framed and consumed; the
//!   body contents are not interpreted. Any other status than `200` and
//!   `429` moves nothing at all.
//! - **The JSON-RPC body**, for HTTP `200` only: its `jsonrpc` must be
//!   `"2.0"` and its `id` must be the request's id. A body that answers
//!   another request, or has no id, is a protocol failure and moves no
//!   submission state.
//!
//! An acknowledgement is a `result` that has the form of a bundle hash,
//! `0x` and exactly 64 hex digits, as both builders document it. Any other
//! `result` is not understood.

/// What a provider's response amounts to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseOutcome {
    /// The provider accepted the request: HTTP 200 with the request's id
    /// and a `result` in the form of a bundle hash.
    Ack,
    /// The provider refused the request with a JSON-RPC error.
    Reject {
        /// The error's `code`, if it has an integer one.
        code: Option<i64>,
    },
    /// The provider refused the request with HTTP 429. Bound to the
    /// request by the connection alone; the body is fully consumed but not interpreted.
    RateLimited,
    /// The response could not be understood. Nothing is concluded from it.
    Protocol(ProtocolFailure),
    /// The connection ended or failed before a response was complete.
    Connection,
}

/// Why a response could not be understood.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolFailure {
    /// Larger than the storage declared for it.
    Oversize,
    /// Not a well-formed HTTP/1.1 response.
    MalformedHttp,
    /// Neither `Content-Length` nor chunked: its end cannot be known.
    UnframedBody,
    /// An HTTP status other than 200 and 429.
    HttpStatus(u16),
    /// The body is not well-formed JSON, or not an object.
    MalformedJson,
    /// `jsonrpc` is missing or not `"2.0"`.
    WrongVersion,
    /// `id` is missing or not the id of the request.
    WrongId,
    /// The body has neither `result` nor `error`.
    NeitherResultNorError,
    /// The body has both `result` and `error`.
    BothResultAndError,
    /// The `result` is not a bundle hash: `0x` and exactly 64 hex digits.
    UnexpectedResult,
}

/// Fixed storage for one response.
#[derive(Debug)]
pub struct ResponseBuffer {
    buf: Vec<u8>,
    len: usize,
    body: Vec<u8>,
    /// The status and the body framer once the head arrived: the body is
    /// framed on from where the last read stopped.
    framer: Option<(u16, crate::http1::Body)>,
}

impl ResponseBuffer {
    /// Storage for a response of at most `capacity` bytes. Allocates once.
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0u8; capacity],
            len: 0,
            body: Vec::with_capacity(capacity),
            framer: None,
        }
    }

    /// Forgets what was received.
    pub fn reset(&mut self) {
        self.len = 0;
        self.body.clear();
        self.framer = None;
    }

    /// The part of the storage that is still free, to read into.
    pub fn spare(&mut self) -> &mut [u8] {
        &mut self.buf[self.len..]
    }

    /// Records that `count` bytes were read into [`Self::spare`] and
    /// returns the outcome once the response is complete. `request_id` is
    /// the id of the request the response must answer. A response that
    /// fills the storage without being complete is oversize.
    pub fn advance(&mut self, count: usize, request_id: &[u8]) -> Option<ResponseOutcome> {
        self.len = (self.len + count).min(self.buf.len());
        let outcome = parse(
            &self.buf[..self.len],
            self.buf.len(),
            &mut self.framer,
            &mut self.body,
            request_id,
        );
        if outcome.is_none() && self.len == self.buf.len() {
            return Some(ResponseOutcome::Protocol(ProtocolFailure::Oversize));
        }
        outcome
    }
}

/// Parses `data` as far as it goes, within storage of `capacity` bytes
/// (`crate::http1`, the framer the canonical follower shares: one
/// `Content-Length` or `chunked`, nothing else), resuming the framer in
/// `state` across reads. `None` means incomplete.
fn parse(
    data: &[u8],
    capacity: usize,
    state: &mut Option<(u16, crate::http1::Body)>,
    body: &mut Vec<u8>,
    request_id: &[u8],
) -> Option<ResponseOutcome> {
    use crate::http1::{self, Fault, Step};
    let failure = |fault: Fault| {
        Some(ResponseOutcome::Protocol(match fault {
            Fault::Unframed => ProtocolFailure::UnframedBody,
            Fault::TooLarge => ProtocolFailure::Oversize,
            _ => ProtocolFailure::MalformedHttp,
        }))
    };
    if state.is_none() {
        let head_len = http1::head_len(data)?;
        let head = match http1::parse_head(&data[..head_len]) {
            Ok(head) => head,
            Err(fault) => return failure(fault),
        };
        let framing = match head.framing {
            Ok(framing) => framing,
            Err(fault) => return failure(fault),
        };
        body.clear();
        *state = Some((head.status, http1::Body::new(framing, head_len)));
    }
    let (status, framer) = state.as_mut()?;
    match framer.advance(data, body, capacity) {
        Step::More => return None,
        Step::Fault(fault) => return failure(fault),
        Step::Done => {}
    }
    Some(match *status {
        200 => classify(framer.body(data, body), request_id),
        429 => ResponseOutcome::RateLimited,
        other => ResponseOutcome::Protocol(ProtocolFailure::HttpStatus(other)),
    })
}

/// Classifies a JSON-RPC response body to the request `request_id`.
fn classify(body: &[u8], request_id: &[u8]) -> ResponseOutcome {
    let mut json = Json { s: body, i: 0 };
    let Ok(top) = json.top(request_id) else {
        return ResponseOutcome::Protocol(ProtocolFailure::MalformedJson);
    };
    if top.version != Some(true) {
        return ResponseOutcome::Protocol(ProtocolFailure::WrongVersion);
    }
    if top.id != Some(true) {
        return ResponseOutcome::Protocol(ProtocolFailure::WrongId);
    }
    match (top.result, top.error) {
        (Some(true), None) => ResponseOutcome::Ack,
        (Some(false), None) => ResponseOutcome::Protocol(ProtocolFailure::UnexpectedResult),
        (None, Some(code)) => ResponseOutcome::Reject { code },
        (None, None) => ResponseOutcome::Protocol(ProtocolFailure::NeitherResultNorError),
        (Some(_), Some(_)) => ResponseOutcome::Protocol(ProtocolFailure::BothResultAndError),
    }
}

/// What the top-level object of a response holds.
#[derive(Default)]
struct Top {
    /// `jsonrpc` is present; `true` if it is `"2.0"`.
    version: Option<bool>,
    /// `id` is present; `true` if it is the request's id as a string.
    id: Option<bool>,
    /// `result` is present; `true` if it is a bundle hash.
    result: Option<bool>,
    /// `error` is present; its integer `code`, if any.
    error: Option<Option<i64>>,
}

/// Length of a bundle hash: `0x` and 64 hex digits.
const BUNDLE_HASH_LEN: usize = 66;

/// Whether the raw content of a JSON string is a bundle hash. Escapes are
/// not decoded, so a hash written with escapes is not one.
fn is_bundle_hash(content: &[u8]) -> bool {
    content.len() == BUNDLE_HASH_LEN
        && content.starts_with(b"0x")
        && content[2..].iter().all(u8::is_ascii_hexdigit)
}

/// Largest nesting depth accepted.
const MAX_DEPTH: u8 = 16;

/// A strict JSON reader over a byte slice. It validates the whole text and
/// extracts only what [`Top`] needs.
struct Json<'a> {
    s: &'a [u8],
    i: usize,
}

type Parsed<T> = Result<T, ()>;

impl<'a> Json<'a> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> Parsed<()> {
        if self.peek() == Some(byte) {
            self.i += 1;
            Ok(())
        } else {
            Err(())
        }
    }

    /// The whole text: one object, then nothing.
    fn top(&mut self, request_id: &[u8]) -> Parsed<Top> {
        let mut top = Top::default();
        self.ws();
        self.eat(b'{')?;
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
        } else {
            loop {
                self.ws();
                let key = self.string()?;
                self.ws();
                self.eat(b':')?;
                self.ws();
                if key == b"jsonrpc" || key == b"id" {
                    let seen = if key == b"id" {
                        &mut top.id
                    } else {
                        &mut top.version
                    };
                    if seen.is_some() {
                        return Err(());
                    }
                    let expected: &[u8] = if key == b"id" { request_id } else { b"2.0" };
                    let matches = if self.peek() == Some(b'"') {
                        self.string()? == expected
                    } else {
                        self.value(1)?;
                        false
                    };
                    *seen = Some(matches);
                } else if key == b"result" {
                    if top.result.is_some() {
                        return Err(());
                    }
                    let hash = if self.peek() == Some(b'"') {
                        is_bundle_hash(self.string()?)
                    } else {
                        self.value(1)?;
                        false
                    };
                    top.result = Some(hash);
                } else if key == b"error" {
                    if top.error.is_some() {
                        return Err(());
                    }
                    top.error = Some(self.error()?);
                } else {
                    self.value(1)?;
                }
                self.ws();
                if self.peek() == Some(b',') {
                    self.i += 1;
                } else {
                    self.eat(b'}')?;
                    break;
                }
            }
        }
        self.ws();
        if self.i == self.s.len() {
            Ok(top)
        } else {
            Err(())
        }
    }

    /// The `error` member: any value; if it is an object, its integer
    /// `code` is returned.
    fn error(&mut self) -> Parsed<Option<i64>> {
        if self.peek() != Some(b'{') {
            self.value(1)?;
            return Ok(None);
        }
        self.i += 1;
        let mut code = None;
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(None);
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            self.eat(b':')?;
            self.ws();
            if key == b"code" && matches!(self.peek(), Some(b'-' | b'0'..=b'9')) {
                code = self.number()?;
            } else {
                self.value(2)?;
            }
            self.ws();
            if self.peek() == Some(b',') {
                self.i += 1;
            } else {
                self.eat(b'}')?;
                return Ok(code);
            }
        }
    }

    /// Any value, validated and skipped.
    fn value(&mut self, depth: u8) -> Parsed<()> {
        if depth > MAX_DEPTH {
            return Err(());
        }
        match self.peek().ok_or(())? {
            b'"' => self.string().map(|_| ()),
            b'-' | b'0'..=b'9' => self.number().map(|_| ()),
            b't' => self.literal(b"true"),
            b'f' => self.literal(b"false"),
            b'n' => self.literal(b"null"),
            open @ (b'{' | b'[') => {
                let close = if open == b'{' { b'}' } else { b']' };
                self.i += 1;
                self.ws();
                if self.peek() == Some(close) {
                    self.i += 1;
                    return Ok(());
                }
                loop {
                    self.ws();
                    if open == b'{' {
                        self.string()?;
                        self.ws();
                        self.eat(b':')?;
                        self.ws();
                    }
                    self.value(depth + 1)?;
                    self.ws();
                    if self.peek() == Some(b',') {
                        self.i += 1;
                    } else {
                        return self.eat(close);
                    }
                }
            }
            _ => Err(()),
        }
    }

    fn literal(&mut self, word: &[u8]) -> Parsed<()> {
        if self.s[self.i..].starts_with(word) {
            self.i += word.len();
            Ok(())
        } else {
            Err(())
        }
    }

    /// A string; returns its raw content between the quotes.
    fn string(&mut self) -> Parsed<&'a [u8]> {
        self.eat(b'"')?;
        let text: &'a [u8] = self.s;
        let start = self.i;
        loop {
            match self.peek().ok_or(())? {
                b'"' => {
                    let content = &text[start..self.i];
                    self.i += 1;
                    return Ok(content);
                }
                b'\\' => {
                    self.i += 1;
                    match self.peek().ok_or(())? {
                        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => self.i += 1,
                        b'u' => {
                            let hex = self.s.get(self.i + 1..self.i + 5).ok_or(())?;
                            if !hex.iter().all(u8::is_ascii_hexdigit) {
                                return Err(());
                            }
                            self.i += 5;
                        }
                        _ => return Err(()),
                    }
                }
                0x00..=0x1f => return Err(()),
                _ => self.i += 1,
            }
        }
    }

    /// A number; returns it if it is an integer that fits `i64`.
    fn number(&mut self) -> Parsed<Option<i64>> {
        let negative = self.peek() == Some(b'-');
        if negative {
            self.i += 1;
        }
        let first = self.peek();
        let mut integer: Option<i64> = Some(0);
        let digits = self.digits(&mut integer)?;
        // No leading zeros: "0" is a number, "01" is not.
        if digits == 0 || (first == Some(b'0') && digits > 1) {
            return Err(());
        }
        let mut is_integer = true;
        if self.peek() == Some(b'.') {
            self.i += 1;
            is_integer = false;
            if self.digits(&mut None)? == 0 {
                return Err(());
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            is_integer = false;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if self.digits(&mut None)? == 0 {
                return Err(());
            }
        }
        if !is_integer {
            return Ok(None);
        }
        Ok(integer.and_then(|value| {
            if negative {
                value.checked_neg()
            } else {
                Some(value)
            }
        }))
    }

    /// Consumes decimal digits, accumulating them into `value` as long as
    /// they fit, and returns how many there were.
    fn digits(&mut self, value: &mut Option<i64>) -> Parsed<usize> {
        let start = self.i;
        while let Some(digit @ b'0'..=b'9') = self.peek() {
            *value = value
                .and_then(|v| v.checked_mul(10))
                .and_then(|v| v.checked_add(i64::from(digit - b'0')));
            self.i += 1;
        }
        Ok(self.i - start)
    }
}

//! HTTP/1.1 `POST` over one keep-alive connection, generic over the byte
//! stream (in production the rustls stream of `crate::wss::endpoint`).
//!
//! One request at a time, no pipelining. A response is framed by
//! `Content-Length` or `chunked` (RFC 9112 §6, §7.1) and bounded by
//! `max_response_bytes` in all and by `request_timeout` from the first
//! byte written. The connection is kept after a complete 200 response
//! unless the server closes it, and dropped after anything else.

use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use super::{FollowerLimits, HttpFault, RequestError, stopped};
use crate::wss::endpoint::{classify_io, is_tick};
use crate::wss::{DisconnectReason, RateLimitSignal};

const HTTP_OK: u16 = 200;
const HTTP_TOO_MANY_REQUESTS: u16 = 429;
/// The size of one read: a buffer size, not a bound (the bound of a
/// response is `max_response_bytes`).
const READ_CHUNK: usize = 16 * 1024;

/// One endpoint's client: its connector and the kept connection.
pub(super) struct Client<S, C> {
    connect: C,
    conn: Option<S>,
    authority: String,
    path: String,
    request_timeout: Duration,
    max_response_bytes: usize,
}

/// A failed exchange, and whether any byte of a response had arrived.
#[derive(Debug)]
pub(super) struct Failure {
    pub(super) error: RequestError,
    pub(super) answered: bool,
}

impl Failure {
    fn before(error: RequestError) -> Self {
        Self {
            error,
            answered: false,
        }
    }

    fn after(error: RequestError) -> Self {
        Self {
            error,
            answered: true,
        }
    }
}

/// A response head and its body (empty unless the status is 200).
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Response {
    pub(super) status: u16,
    pub(super) body: Vec<u8>,
    /// The server ends the connection after this response.
    pub(super) close: bool,
}

impl<S, C> Client<S, C>
where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
{
    /// `authority` is the `Host` header value, `path` the request target.
    pub(super) fn new(
        connect: C,
        authority: String,
        path: String,
        limits: &FollowerLimits,
    ) -> Self {
        Self {
            connect,
            conn: None,
            authority,
            path,
            request_timeout: limits.request_timeout,
            max_response_bytes: limits.max_response_bytes,
        }
    }

    /// POSTs `body` and returns the body of the 200 response. A kept
    /// connection that ends before any byte of the answer (the server
    /// closed it while idle) is replaced once.
    pub(super) fn post(&mut self, body: &[u8], stop: &AtomicBool) -> Result<Vec<u8>, RequestError> {
        let reused = self.conn.is_some();
        match self.exchange(body, stop) {
            Err(Failure {
                error: RequestError::Transport(reason),
                answered: false,
            }) if reused
                && !matches!(
                    reason,
                    DisconnectReason::Stopped | DisconnectReason::Timeout
                ) =>
            {
                self.exchange(body, stop).map_err(|f| f.error)
            }
            other => other.map_err(|f| f.error),
        }
    }

    fn exchange(&mut self, body: &[u8], stop: &AtomicBool) -> Result<Vec<u8>, Failure> {
        let mut stream = match self.conn.take() {
            Some(stream) => stream,
            None => (self.connect)(stop)
                .map_err(|reason| Failure::before(RequestError::Transport(reason)))?,
        };
        let response = round_trip(
            &mut stream,
            &self.authority,
            &self.path,
            body,
            Bounds {
                started: Instant::now(),
                timeout: self.request_timeout,
                max: self.max_response_bytes,
            },
            stop,
        )?;
        match response.status {
            HTTP_OK => {
                if !response.close {
                    self.conn = Some(stream);
                }
                Ok(response.body)
            }
            HTTP_TOO_MANY_REQUESTS => Err(Failure::after(RequestError::RateLimited(
                RateLimitSignal::Http429,
            ))),
            status => Err(Failure::after(RequestError::HttpStatus(status))),
        }
    }
}

/// The time and size bounds of one exchange.
#[derive(Clone, Copy, Debug)]
pub(super) struct Bounds {
    pub(super) started: Instant,
    pub(super) timeout: Duration,
    pub(super) max: usize,
}

impl Bounds {
    /// `stop` or the deadline, checked between reads and writes.
    fn check(&self, stop: &AtomicBool) -> Result<(), RequestError> {
        if stopped(stop) {
            return Err(RequestError::Transport(DisconnectReason::Stopped));
        }
        if self.started.elapsed() >= self.timeout {
            return Err(RequestError::Transport(DisconnectReason::Timeout));
        }
        Ok(())
    }
}

/// Writes one request and reads its response.
pub(super) fn round_trip<S: Read + Write>(
    stream: &mut S,
    authority: &str,
    path: &str,
    body: &[u8],
    bounds: Bounds,
    stop: &AtomicBool,
) -> Result<Response, Failure> {
    let mut request = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    request.extend_from_slice(body);
    write_all(stream, &request, &bounds, stop).map_err(Failure::before)?;

    let mut inbound = Inbound {
        stream,
        buf: Vec::new(),
        bounds,
        stop,
    };
    let head_len = inbound.head()?;
    let head =
        parse_head(&inbound.buf[..head_len]).map_err(|f| Failure::after(RequestError::Http(f)))?;
    if head.status != HTTP_OK {
        return Ok(Response {
            status: head.status,
            body: Vec::new(),
            close: true,
        });
    }
    let body = match head
        .framing
        .map_err(|f| Failure::after(RequestError::Http(f)))?
    {
        Framing::Length(len) => inbound.exact(head_len, len)?,
        Framing::Chunked => inbound.chunked(head_len)?,
    };
    Ok(Response {
        status: head.status,
        body,
        close: head.close,
    })
}

fn write_all<S: Write>(
    stream: &mut S,
    bytes: &[u8],
    bounds: &Bounds,
    stop: &AtomicBool,
) -> Result<(), RequestError> {
    let mut written = 0;
    while written < bytes.len() {
        bounds.check(stop)?;
        match stream.write(&bytes[written..]) {
            Ok(0) => return Err(RequestError::Transport(DisconnectReason::Closed(None))),
            Ok(n) => written += n,
            Err(e) if is_tick(&e) => {}
            Err(e) => return Err(RequestError::Transport(classify_io(&e))),
        }
    }
    loop {
        bounds.check(stop)?;
        match stream.flush() {
            Ok(()) => return Ok(()),
            Err(e) if is_tick(&e) => {}
            Err(e) => return Err(RequestError::Transport(classify_io(&e))),
        }
    }
}

/// How the body of a 200 response is delimited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Framing {
    Length(usize),
    Chunked,
}

#[derive(Debug)]
struct Head {
    status: u16,
    /// Checked only for a 200 response.
    framing: Result<Framing, HttpFault>,
    close: bool,
}

/// Parses the status line and the headers (`head` ends with the blank
/// line). Only framing, coding and connection headers are interpreted.
fn parse_head(head: &[u8]) -> Result<Head, HttpFault> {
    if !head.is_ascii() {
        return Err(HttpFault::Header);
    }
    let text = std::str::from_utf8(head).map_err(|_| HttpFault::Header)?;
    let text = text.strip_suffix("\r\n\r\n").ok_or(HttpFault::Header)?;
    let mut lines = text.split("\r\n");
    let status_line = lines.next().ok_or(HttpFault::StatusLine)?;
    let (version, rest) = status_line.split_once(' ').ok_or(HttpFault::StatusLine)?;
    let mut close = match version {
        "HTTP/1.1" => false,
        "HTTP/1.0" => true,
        _ => return Err(HttpFault::StatusLine),
    };
    let (code, reason) = rest.split_at_checked(3).ok_or(HttpFault::StatusLine)?;
    if !code.bytes().all(|b| b.is_ascii_digit()) || !(reason.is_empty() || reason.starts_with(' '))
    {
        return Err(HttpFault::StatusLine);
    }
    let status: u16 = code.parse().map_err(|_| HttpFault::StatusLine)?;

    let mut length: Option<usize> = None;
    let mut chunked = false;
    let mut framing_fault = None;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(HttpFault::Header)?;
        if name.is_empty() || !name.bytes().all(is_token_char) {
            return Err(HttpFault::Header);
        }
        let value = value.trim_matches([' ', '\t']);
        if name.eq_ignore_ascii_case("content-length") {
            let parsed = value
                .bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| value.parse::<usize>().ok())
                .flatten();
            match parsed {
                Some(n) if length.is_none_or(|l| l == n) => length = Some(n),
                _ => framing_fault = Some(HttpFault::Framing),
            }
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            if chunked || !value.eq_ignore_ascii_case("chunked") {
                framing_fault = Some(HttpFault::Coding);
            }
            chunked = true;
        } else if name.eq_ignore_ascii_case("content-encoding") {
            if !value.eq_ignore_ascii_case("identity") {
                framing_fault = Some(HttpFault::Coding);
            }
        } else if name.eq_ignore_ascii_case("connection") {
            for option in value.split(',').map(|o| o.trim_matches([' ', '\t'])) {
                if option.eq_ignore_ascii_case("close") {
                    close = true;
                } else if option.eq_ignore_ascii_case("keep-alive") && version == "HTTP/1.0" {
                    close = false;
                }
            }
        }
    }
    let framing = match (framing_fault, length, chunked) {
        (Some(fault), _, _) => Err(fault),
        (None, Some(n), false) => Ok(Framing::Length(n)),
        (None, None, true) => Ok(Framing::Chunked),
        (None, _, _) => Err(HttpFault::Framing),
    };
    Ok(Head {
        status,
        framing,
        close,
    })
}

/// RFC 9110 `tchar`.
fn is_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// The response bytes read so far and the means to read more.
struct Inbound<'a, S> {
    stream: &'a mut S,
    buf: Vec<u8>,
    bounds: Bounds,
    stop: &'a AtomicBool,
}

impl<S: Read> Inbound<'_, S> {
    fn fail(&self, error: RequestError) -> Failure {
        Failure {
            error,
            answered: !self.buf.is_empty(),
        }
    }

    fn fault(&self, fault: HttpFault) -> Failure {
        self.fail(RequestError::Http(fault))
    }

    /// Reads at least one more byte, within the size and time bounds.
    fn fill(&mut self) -> Result<(), Failure> {
        let room = self.bounds.max.saturating_sub(self.buf.len());
        if room == 0 {
            return Err(self.fault(HttpFault::TooLarge));
        }
        let mut chunk = [0u8; READ_CHUNK];
        let want = room.min(READ_CHUNK);
        loop {
            self.bounds.check(self.stop).map_err(|e| self.fail(e))?;
            match self.stream.read(&mut chunk[..want]) {
                Ok(0) => {
                    return Err(self.fail(RequestError::Transport(DisconnectReason::Closed(None))));
                }
                Ok(n) => {
                    self.buf.extend_from_slice(&chunk[..n]);
                    return Ok(());
                }
                Err(e) if is_tick(&e) => {}
                Err(e) => return Err(self.fail(RequestError::Transport(classify_io(&e)))),
            }
        }
    }

    /// Reads up to the blank line after the headers; the head's length.
    fn head(&mut self) -> Result<usize, Failure> {
        let mut searched = 0;
        loop {
            if let Some(at) = find(&self.buf[searched..], b"\r\n\r\n") {
                return Ok(searched + at + 4);
            }
            searched = self.buf.len().saturating_sub(3);
            self.fill()?;
        }
    }

    /// Reads up to a CRLF at or after `from`; the index of its CR.
    fn line(&mut self, from: usize) -> Result<usize, Failure> {
        let mut searched = from;
        loop {
            if let Some(at) = self
                .buf
                .get(searched..)
                .and_then(|rest| find(rest, b"\r\n"))
            {
                return Ok(searched + at);
            }
            searched = self.buf.len().saturating_sub(1).max(from);
            self.fill()?;
        }
    }

    /// A `Content-Length` body starting at `from`, the end of the response.
    fn exact(&mut self, from: usize, len: usize) -> Result<Vec<u8>, Failure> {
        let end = from
            .checked_add(len)
            .filter(|end| *end <= self.bounds.max)
            .ok_or_else(|| self.fault(HttpFault::TooLarge))?;
        while self.buf.len() < end {
            self.fill()?;
        }
        if self.buf.len() > end {
            return Err(self.fault(HttpFault::Trailing));
        }
        Ok(self.buf.split_off(from))
    }

    /// A chunked body starting at `from`; trailer fields are read and
    /// ignored.
    fn chunked(&mut self, from: usize) -> Result<Vec<u8>, Failure> {
        let mut body = Vec::new();
        let mut pos = from;
        loop {
            let line_end = self.line(pos)?;
            let size =
                chunk_size(&self.buf[pos..line_end]).ok_or_else(|| self.fault(HttpFault::Chunk))?;
            pos = line_end + 2;
            if size == 0 {
                break;
            }
            let end = pos
                .checked_add(size)
                .and_then(|end| end.checked_add(2))
                .filter(|end| *end <= self.bounds.max)
                .ok_or_else(|| self.fault(HttpFault::TooLarge))?;
            while self.buf.len() < end {
                self.fill()?;
            }
            if self.buf[end - 2..end] != *b"\r\n" {
                return Err(self.fault(HttpFault::Chunk));
            }
            body.extend_from_slice(&self.buf[pos..end - 2]);
            pos = end;
        }
        loop {
            let line_end = self.line(pos)?;
            let empty = line_end == pos;
            pos = line_end + 2;
            if empty {
                break;
            }
        }
        if self.buf.len() > pos {
            return Err(self.fault(HttpFault::Trailing));
        }
        Ok(body)
    }
}

/// `chunk-size [; chunk-ext]`: hex digits, either case.
fn chunk_size(line: &[u8]) -> Option<usize> {
    let size = match line.iter().position(|b| *b == b';') {
        Some(at) => &line[..at],
        None => line,
    };
    let size = size.trim_ascii_end();
    if size.is_empty() || !size.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    usize::from_str_radix(std::str::from_utf8(size).ok()?, 16).ok()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

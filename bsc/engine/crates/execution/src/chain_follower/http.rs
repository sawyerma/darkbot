//! HTTP/1.1 `POST` over one keep-alive connection, generic over the byte
//! stream (in production the rustls stream of `crate::wss::endpoint`).
//!
//! One request at a time, no pipelining. A response is framed by
//! `Content-Length` or `chunked` (RFC 9112 §6, §7.1; `crate::http1`, the
//! framer the builders' responses share) and bounded by
//! `max_response_bytes` in all and by `request_timeout` from the first
//! byte written. The connection is kept after a complete 200 response
//! unless the server closes it, and dropped after anything else.

use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use super::{FollowerLimits, HttpFault, RequestError, stopped};
use crate::http1;
use crate::wss::endpoint::{classify_io, is_tick};
use crate::wss::{DisconnectReason, RateLimitSignal};

const HTTP_OK: u16 = 200;
const HTTP_TOO_MANY_REQUESTS: u16 = 429;
/// The size of one read: a buffer size, not a bound (the bound of a
/// response is `max_response_bytes`).
const READ_CHUNK: usize = 16 * 1024;

/// One endpoint's client: its connector and the kept connection.
pub(crate) struct Client<S, C> {
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
    pub(crate) fn new(
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
    pub(crate) fn post(&mut self, body: &[u8], stop: &AtomicBool) -> Result<Vec<u8>, RequestError> {
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
    let head = http1::parse_head(&inbound.buf[..head_len])
        .map_err(|f| Failure::after(RequestError::Http(http_fault(f))))?;
    if head.status != HTTP_OK {
        return Ok(Response {
            status: head.status,
            body: Vec::new(),
            close: true,
        });
    }
    let framing = head
        .framing
        .map_err(|f| Failure::after(RequestError::Http(http_fault(f))))?;
    let body = inbound.body(framing, head_len)?;
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

/// The follower's fault of a framing fault: a response without framing is
/// a framing fault too.
fn http_fault(fault: http1::Fault) -> HttpFault {
    match fault {
        http1::Fault::StatusLine => HttpFault::StatusLine,
        http1::Fault::Header => HttpFault::Header,
        http1::Fault::Unframed | http1::Fault::Framing => HttpFault::Framing,
        http1::Fault::Coding => HttpFault::Coding,
        http1::Fault::Chunk => HttpFault::Chunk,
        http1::Fault::Trailing => HttpFault::Trailing,
        http1::Fault::TooLarge => HttpFault::TooLarge,
    }
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
            if let Some(len) = http1::head_len(&self.buf[searched..]) {
                return Ok(searched + len);
            }
            searched = self.buf.len().saturating_sub(3);
            self.fill()?;
        }
    }

    /// The body after a head of `start` bytes, read to the end of the
    /// response (trailer fields of a chunked one are read and ignored).
    fn body(&mut self, framing: http1::Framing, start: usize) -> Result<Vec<u8>, Failure> {
        let mut framer = http1::Body::new(framing, start);
        let mut chunks = Vec::new();
        loop {
            match framer.advance(&self.buf, &mut chunks, self.bounds.max) {
                http1::Step::Done => break,
                http1::Step::More => self.fill()?,
                http1::Step::Fault(fault) => return Err(self.fault(http_fault(fault))),
            }
        }
        Ok(match framing {
            http1::Framing::Length(_) => self.buf.split_off(start),
            http1::Framing::Chunked => chunks,
        })
    }
}

//! HTTP/1.1 response framing (RFC 9112), the one framer of the canonical
//! follower's JSON-RPC client and of the builders' submission responses
//! (Audit E 4.4).
//!
//! Pure functions over the bytes received so far: the status line and the
//! headers, then the body, framed by exactly one `Content-Length` or by
//! `chunked`. Only framing, coding and connection headers are interpreted;
//! everything else about the framing fails closed. The body framer resumes
//! where it stopped, so a caller reading more bytes never frames twice.

/// Why a response cannot be framed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fault {
    /// The status line is not `HTTP/1.0|1.1 <3 digits>[ reason]`.
    StatusLine,
    /// A header line is malformed or not ASCII.
    Header,
    /// Neither `Content-Length` nor `chunked`: the end cannot be known.
    Unframed,
    /// An invalid or repeated `Content-Length`, or it with `chunked`.
    Framing,
    /// A transfer or content coding other than `chunked`/identity.
    Coding,
    /// A malformed chunk.
    Chunk,
    /// Bytes after the end of the response.
    Trailing,
    /// The response would be larger than the caller's bound.
    TooLarge,
}

/// How the body is delimited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Framing {
    Length(usize),
    Chunked,
}

/// A parsed head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Head {
    pub(crate) status: u16,
    /// The body's framing, or why it has none a recipient can trust.
    pub(crate) framing: Result<Framing, Fault>,
    /// The connection ends after this response.
    pub(crate) close: bool,
}

/// The length of the head, through its blank line, once it has arrived.
pub(crate) fn head_len(data: &[u8]) -> Option<usize> {
    find(data, b"\r\n\r\n").map(|at| at + 4)
}

/// Parses the status line and the headers (`head` ends with the blank
/// line). One pass over the bytes: lines end with CRLF (a bare CR or LF
/// ends none), and hold SP, HTAB and visible ASCII only (RFC 9112 §2.2).
pub(crate) fn parse_head(head: &[u8]) -> Result<Head, Fault> {
    let mut rest = head.strip_suffix(b"\r\n\r\n").ok_or(Fault::Header)?;
    let status_line = next_line(&mut rest, Fault::StatusLine)?.ok_or(Fault::StatusLine)?;
    let space = status_line
        .iter()
        .position(|b| *b == b' ')
        .ok_or(Fault::StatusLine)?;
    let (version, rest_of_status) = (&status_line[..space], &status_line[space + 1..]);
    let http10 = match version {
        b"HTTP/1.1" => false,
        b"HTTP/1.0" => true,
        _ => return Err(Fault::StatusLine),
    };
    let mut close = http10;
    let (code, reason) = rest_of_status
        .split_at_checked(3)
        .ok_or(Fault::StatusLine)?;
    if !code.iter().all(u8::is_ascii_digit) || !(reason.is_empty() || reason[0] == b' ') {
        return Err(Fault::StatusLine);
    }
    let status = code.iter().fold(0u16, |v, d| v * 10 + u16::from(*d - b'0'));

    let mut length: Option<usize> = None;
    let mut chunked = false;
    let mut framing_fault = None;
    while let Some(line) = next_line(&mut rest, Fault::Header)? {
        let colon = line.iter().position(|b| *b == b':').ok_or(Fault::Header)?;
        let (name, value) = (&line[..colon], line[colon + 1..].trim_ascii());
        if name.is_empty() || !name.iter().all(|b| is_token_char(*b)) {
            return Err(Fault::Header);
        }
        if name.eq_ignore_ascii_case(b"content-length") {
            // One length, given once: a repeated one is refused even when
            // equal, never resolved.
            match decimal(value) {
                Some(n) if length.is_none() => length = Some(n),
                _ => framing_fault = Some(Fault::Framing),
            }
        } else if name.eq_ignore_ascii_case(b"transfer-encoding") {
            if chunked || !value.eq_ignore_ascii_case(b"chunked") {
                framing_fault = Some(Fault::Coding);
            }
            chunked = true;
        } else if name.eq_ignore_ascii_case(b"content-encoding") {
            if !value.eq_ignore_ascii_case(b"identity") {
                framing_fault = Some(Fault::Coding);
            }
        } else if name.eq_ignore_ascii_case(b"connection") {
            for option in value.split(|b| *b == b',').map(<[u8]>::trim_ascii) {
                if option.eq_ignore_ascii_case(b"close") {
                    close = true;
                } else if option.eq_ignore_ascii_case(b"keep-alive") && http10 {
                    close = false;
                }
            }
        }
    }
    let framing = match (framing_fault, length, chunked) {
        (Some(fault), _, _) => Err(fault),
        (None, Some(n), false) => Ok(Framing::Length(n)),
        (None, None, true) => Ok(Framing::Chunked),
        (None, None, false) => Err(Fault::Unframed),
        (None, Some(_), true) => Err(Fault::Framing),
    };
    Ok(Head {
        status,
        framing,
        close,
    })
}

/// Splits the next line off `rest` (CRLF-separated; the last one has its
/// CRLF in the blank line), in one pass: `fault` for a bare CR or LF or
/// another control character than HTAB.
fn next_line<'a>(rest: &mut &'a [u8], fault: Fault) -> Result<Option<&'a [u8]>, Fault> {
    let bytes: &'a [u8] = rest;
    if bytes.is_empty() {
        return Ok(None);
    }
    for (i, b) in bytes.iter().enumerate() {
        match *b {
            b'\r' => {
                if bytes.get(i + 1) != Some(&b'\n') || i + 2 == bytes.len() {
                    // A bare CR, or a CRLF before the blank line (an empty
                    // line).
                    return Err(fault);
                }
                *rest = &bytes[i + 2..];
                return Ok(Some(&bytes[..i]));
            }
            b'\t' | b' '..=b'~' => {}
            _ => return Err(fault),
        }
    }
    *rest = &[];
    Ok(Some(bytes))
}

/// A plain decimal within `usize`.
fn decimal(digits: &[u8]) -> Option<usize> {
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    digits.iter().try_fold(0usize, |v, d| {
        v.checked_mul(10)?.checked_add(usize::from(*d - b'0'))
    })
}

/// RFC 9110 `tchar`.
fn is_token_char(b: u8) -> bool {
    matches!(b, b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'!' | b'#'..=b'\'' | b'*' | b'+' | b'-' | b'.' | b'^'..=b'`' | b'|' | b'~')
}

/// What one step of the body framer found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// The response is not complete yet.
    More,
    /// The response ends exactly at the end of the bytes given.
    Done,
    Fault(Fault),
}

#[derive(Clone, Copy, Debug)]
enum State {
    /// At a chunk-size line.
    Size,
    /// In a chunk whose data and CRLF end at this index.
    Data(usize),
    /// After the last chunk: trailer fields up to the empty line.
    Trailers,
    Done,
}

/// The body framer of one response, resumable as more bytes arrive.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Body {
    framing: Framing,
    /// Where the body starts (the head's length).
    start: usize,
    /// How far the response is framed.
    pos: usize,
    /// How far the line at `pos` is checked (no CRLF before it).
    scanned: usize,
    state: State,
}

impl Body {
    /// The body of a response whose head is `start` bytes long.
    pub(crate) fn new(framing: Framing, start: usize) -> Self {
        Self {
            framing,
            start,
            pos: start,
            scanned: start,
            state: State::Size,
        }
    }

    /// Frames `data` (the whole response so far, its head included) on
    /// from where the last step stopped; a chunked body's data is appended
    /// to `out`. `max` bounds the whole response: an announced length or
    /// chunk beyond it is refused before it arrives.
    pub(crate) fn advance(&mut self, data: &[u8], out: &mut Vec<u8>, max: usize) -> Step {
        let step = match self.framing {
            Framing::Length(len) => match self.start.checked_add(len).filter(|e| *e <= max) {
                None => Step::Fault(Fault::TooLarge),
                Some(end) if data.len() < end => Step::More,
                Some(end) => {
                    self.pos = end;
                    self.state = State::Done;
                    Step::Done
                }
            },
            Framing::Chunked => self.chunks(data, out, max),
        };
        match step {
            Step::Done if data.len() > self.pos => Step::Fault(Fault::Trailing),
            other => other,
        }
    }

    /// The body once [`Step::Done`]: the bytes after the head, or the
    /// chunks' data in `out`.
    pub(crate) fn body<'a>(&self, data: &'a [u8], out: &'a [u8]) -> &'a [u8] {
        match self.framing {
            Framing::Length(_) => &data[self.start..self.pos],
            Framing::Chunked => out,
        }
    }

    fn advance_to(&mut self, pos: usize) {
        self.pos = pos;
        self.scanned = pos;
    }

    /// The index of the CR of the CRLF ending the line at `pos`, once it
    /// arrived. Each byte is checked once across steps: a chunk-size line
    /// or a trailer field holds SP, HTAB and visible ASCII only, as the head
    /// does (a bare CR or LF is a chunk fault).
    fn line(&mut self, data: &[u8]) -> Result<Option<usize>, Fault> {
        while let Some(b) = data.get(self.scanned) {
            match *b {
                b'\r' => {
                    return match data.get(self.scanned + 1) {
                        Some(b'\n') => Ok(Some(self.scanned)),
                        Some(_) => Err(Fault::Chunk),
                        None => Ok(None),
                    };
                }
                b'\t' | b' '..=b'~' => self.scanned += 1,
                _ => return Err(Fault::Chunk),
            }
        }
        Ok(None)
    }

    fn chunks(&mut self, data: &[u8], out: &mut Vec<u8>, max: usize) -> Step {
        loop {
            match self.state {
                State::Size => {
                    let line_end = match self.line(data) {
                        Ok(Some(end)) => end,
                        Ok(None) => return Step::More,
                        Err(fault) => return Step::Fault(fault),
                    };
                    let Some(size) = chunk_size(&data[self.pos..line_end]) else {
                        return Step::Fault(Fault::Chunk);
                    };
                    let next = line_end + 2;
                    if size == 0 {
                        self.advance_to(next);
                        self.state = State::Trailers;
                        continue;
                    }
                    // Checked before the state moves: the fault stays.
                    let end = next
                        .checked_add(size)
                        .and_then(|end| end.checked_add(2))
                        .filter(|end| *end <= max);
                    let Some(end) = end else {
                        return Step::Fault(Fault::TooLarge);
                    };
                    self.advance_to(next);
                    self.state = State::Data(end);
                }
                State::Data(end) => {
                    if data.len() < end {
                        return Step::More;
                    }
                    if data[end - 2..end] != *b"\r\n" {
                        return Step::Fault(Fault::Chunk);
                    }
                    out.extend_from_slice(&data[self.pos..end - 2]);
                    self.advance_to(end);
                    self.state = State::Size;
                }
                // Trailer fields are read and ignored.
                State::Trailers => {
                    let line_end = match self.line(data) {
                        Ok(Some(end)) => end,
                        Ok(None) => return Step::More,
                        Err(fault) => return Step::Fault(fault),
                    };
                    let empty = line_end == self.pos;
                    self.advance_to(line_end + 2);
                    if empty {
                        self.state = State::Done;
                    }
                }
                State::Done => return Step::Done,
            }
        }
    }
}

/// `chunk-size [; chunk-ext]`: hex digits, either case.
fn chunk_size(line: &[u8]) -> Option<usize> {
    let size = match line.iter().position(|b| *b == b';') {
        // Whitespace only before an extension (RFC 9112 §7.1.1: BWS).
        Some(at) => line[..at].trim_ascii_end(),
        None => line,
    };
    if size.is_empty() {
        return None;
    }
    size.iter().try_fold(0usize, |v, d| {
        let digit = char::from(*d).to_digit(16)?;
        v.checked_mul(16)?.checked_add(digit as usize)
    })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frames `response` through one head parse and one `Body` fed a byte
    /// at a time: `More` up to its last byte, then `Done`, the body, and
    /// one more byte is `Trailing`.
    fn byte_by_byte(response: &[u8]) -> Vec<u8> {
        let head = head_len(response).unwrap();
        for cut in 0..head {
            assert_eq!(head_len(&response[..cut]), None);
        }
        let framing = parse_head(&response[..head]).unwrap().framing.unwrap();
        let mut framer = Body::new(framing, head);
        let mut out = Vec::new();
        for end in head..response.len() {
            assert_eq!(
                framer.advance(&response[..end], &mut out, 4_096),
                Step::More,
                "{end}"
            );
        }
        assert_eq!(framer.advance(response, &mut out, 4_096), Step::Done);
        let body = framer.body(response, &out).to_vec();
        let mut longer = response.to_vec();
        longer.push(b'x');
        assert_eq!(
            framer.advance(&longer, &mut out, 4_096),
            Step::Fault(Fault::Trailing)
        );
        body
    }

    #[test]
    fn a_body_resumes_at_every_split_point() {
        let plain = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        assert_eq!(byte_by_byte(plain), b"hello");
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                        3 ;ext=1\r\nhel\r\n2\r\nlo\r\n0\r\nX-Trailer: 1\r\n\r\n";
        assert_eq!(byte_by_byte(chunked), b"hello");
    }

    /// Delta review L1: a chunk beyond the bound stays refused when the
    /// framer is asked again.
    #[test]
    fn a_chunk_beyond_the_bound_stays_refused() {
        let response = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                         ff\r\n{}\r\n0\r\n\r\n";
        let head = head_len(response).unwrap();
        let mut framer = Body::new(Framing::Chunked, head);
        for _ in 0..2 {
            assert_eq!(
                framer.advance(response, &mut Vec::new(), 64),
                Step::Fault(Fault::TooLarge)
            );
        }
    }

    /// Review L1: chunk-size lines and trailer fields follow the head's
    /// byte rule; a bare CR or LF or a control byte is a chunk fault.
    #[test]
    fn chunk_lines_and_trailers_take_the_heads_byte_rule() {
        let head = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        for tail in [
            &b"5\n\r\nhello\r\n0\r\n\r\n"[..],
            b"5\r;x\r\nhello\r\n0\r\n\r\n",
            b"5\x0c\r\nhello\r\n0\r\n\r\n",
            b"5;\x00\r\nhello\r\n0\r\n\r\n",
            b"5\r\nhello\r\n0\r\nX:\x01\r\n\r\n",
            b"5\r\nhello\r\n0\r\nX: \xc3\xa4\r\n\r\n",
            // Delta review L2: whitespace without an extension.
            b"5 \r\nhello\r\n0\r\n\r\n",
        ] {
            let response = [&head[..], tail].concat();
            let framing = parse_head(head).unwrap().framing.unwrap();
            let mut framer = Body::new(framing, head.len());
            assert_eq!(
                framer.advance(&response, &mut Vec::new(), 4_096),
                Step::Fault(Fault::Chunk),
                "{}",
                String::from_utf8_lossy(tail)
            );
        }
    }
}

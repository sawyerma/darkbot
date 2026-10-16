//! G0.7-only builder submission: TLS connections to 48Club and BlockRazor,
//! HTTP framing and response parsing. Reached only after the acceptance guard.

use super::*;

pub(crate) fn connect_tls(host: &str) -> Result<Tls, String> {
    connect_tls_port(host, HTTPS_PORT)
}

pub(crate) fn connect_tls_port(host: &str, port: u16) -> Result<Tls, String> {
    let addrs: Vec<_> = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("DNS {host}: {e}"))?
        .collect();
    if addrs.is_empty() {
        return Err(format!("DNS returned no addresses for {host}"));
    }
    let mut failures = Vec::new();
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
            Ok(mut tcp) => {
                tcp.set_nodelay(true).map_err(|e| e.to_string())?;
                tcp.set_read_timeout(Some(Duration::from_secs(15)))
                    .map_err(|e| e.to_string())?;
                tcp.set_write_timeout(Some(Duration::from_secs(5)))
                    .map_err(|e| e.to_string())?;
                let roots =
                    RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
                let mut config = ClientConfig::builder()
                    .with_root_certificates(roots)
                    .with_no_client_auth();
                config.alpn_protocols = vec![b"http/1.1".to_vec()];
                let server_name = ServerName::try_from(host.to_string())
                    .map_err(|e| format!("invalid TLS server name {host}: {e}"))?;
                let mut conn = ClientConnection::new(Arc::new(config), server_name)
                    .map_err(|e| format!("TLS init {host}: {e}"))?;
                match conn.complete_io(&mut tcp) {
                    Ok(_) => return Ok(StreamOwned::new(conn, tcp)),
                    Err(e) => failures.push(format!("{addr}: TLS {e}")),
                }
            }
            Err(e) => failures.push(format!("{addr}: TCP {e}")),
        }
    }
    Err(format!(
        "cannot connect to {host}: {}",
        failures.join(" | ")
    ))
}

/// A non-empty HTTP header value without control characters. main checks the
/// BlockRazor token with it before the guard, so a bad token stops the run
/// before the attempt is latched; framing checks it again.
pub(crate) fn valid_header_value(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| (0x21..=0x7e).contains(&b) || b == b' ')
}

pub(crate) fn http_request(host: &str, body: &str, authorization: Option<&str>) -> Vec<u8> {
    let auth = authorization.map_or_else(String::new, |token| {
        assert!(
            valid_header_value(token),
            "invalid BlockRazor Authorization value"
        );
        format!("Authorization: {token}\r\n")
    });
    format!(
        "POST / HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{auth}\r\n{}",
        body.len(),
        body
    )
    .into_bytes()
}

pub(crate) fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

pub(crate) fn decode_chunked_body(raw: &[u8]) -> Option<Vec<u8>> {
    let mut pos = 0usize;
    let mut out = Vec::new();
    loop {
        let line_end_rel = raw[pos..].windows(2).position(|w| w == b"\r\n")?;
        let line_end = pos + line_end_rel;
        let size_line = std::str::from_utf8(&raw[pos..line_end]).ok()?;
        let size_hex = size_line.split(';').next()?.trim();
        let size = usize::from_str_radix(size_hex, 16).ok()?;
        let data_start = line_end + 2;
        if size == 0 {
            return Some(out);
        }
        let data_end = data_start.checked_add(size)?;
        let chunk_end = data_end.checked_add(2)?;
        if raw.len() < chunk_end || &raw[data_end..chunk_end] != b"\r\n" {
            return None;
        }
        out.extend_from_slice(&raw[data_start..data_end]);
        pos = chunk_end;
    }
}

/// Builder and RPC answers are small; anything larger is refused, not buffered.
const MAX_HTTP_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

pub(crate) struct HttpResponse {
    pub(crate) status: String,
    /// The server announced `Connection: close`.
    pub(crate) close: bool,
    pub(crate) body: String,
}

/// Parses one buffered HTTP/1.1 response: `Ok(None)` while incomplete. No
/// arithmetic on peer-controlled lengths can panic.
pub(crate) fn parse_http_response(buf: &[u8]) -> Result<Option<HttpResponse>, String> {
    if buf.len() > MAX_HTTP_RESPONSE_BYTES {
        return Err(format!(
            "HTTP response exceeds {MAX_HTTP_RESPONSE_BYTES} bytes"
        ));
    }
    let Some(h) = find_header_end(buf) else {
        return Ok(None);
    };
    let header = String::from_utf8_lossy(&buf[..h]).to_string();
    let lower = header.to_ascii_lowercase();
    let status = header.lines().next().unwrap_or("").to_string();
    let close = lower
        .lines()
        .any(|line| line.starts_with("connection:") && line.contains("close"));
    let rest = &buf[h + 4..];
    let body = if lower.contains("transfer-encoding: chunked") {
        let Some(decoded) = decode_chunked_body(rest) else {
            return Ok(None);
        };
        decoded
    } else {
        let content_length = lower.lines().find_map(|line| {
            line.strip_prefix("content-length:")
                .and_then(|x| x.trim().parse::<usize>().ok())
        });
        let Some(content_length) = content_length else {
            return Ok(None);
        };
        if content_length > MAX_HTTP_RESPONSE_BYTES {
            return Err(format!("HTTP Content-Length {content_length} is too large"));
        }
        if rest.len() < content_length {
            return Ok(None);
        }
        rest[..content_length].to_vec()
    };
    Ok(Some(HttpResponse {
        status,
        close,
        body: String::from_utf8_lossy(&body).to_string(),
    }))
}

pub(crate) fn read_http_response(stream: &mut Tls) -> Result<HttpResponse, String> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 8192];
    loop {
        let n = stream
            .read(&mut tmp)
            .map_err(|e| format!("TLS read: {e}"))?;
        if n == 0 {
            return Err("server closed connection before complete response".into());
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(response) = parse_http_response(&buf)? {
            return Ok(response);
        }
    }
}

pub(crate) fn read_http(stream: &mut Tls) -> Result<(String, String), String> {
    read_http_response(stream).map(|r| (r.status, r.body))
}

/// Pre-egress liveness of a prewarmed builder connection: processes what the
/// peer sent since the handshake without blocking and fails if it closed the
/// connection (EOF, close_notify, a socket or TLS error) or sent application
/// data before any request, which would later be read as the bundle response.
/// Nothing is written.
pub(crate) fn transport_alive(stream: &mut Tls) -> Result<(), String> {
    stream
        .sock
        .set_nonblocking(true)
        .map_err(|e| format!("set non-blocking: {e}"))?;
    let state = loop {
        match stream.conn.read_tls(&mut stream.sock) {
            Ok(0) => break Err("peer closed the connection".to_string()),
            Ok(_) => match stream.conn.process_new_packets() {
                Ok(io) if io.peer_has_closed() => break Err("peer sent close_notify".to_string()),
                Ok(io) if io.plaintext_bytes_to_read() > 0 => {
                    break Err("peer sent unsolicited data before the request".to_string());
                }
                Ok(_) => continue,
                Err(e) => break Err(format!("TLS error: {e}")),
            },
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break Ok(()),
            Err(e) => break Err(format!("socket error: {e}")),
        }
    };
    stream
        .sock
        .set_nonblocking(false)
        .map_err(|e| format!("restore blocking: {e}"))?;
    state
}

pub(crate) fn submit_provider(
    provider: &'static str,
    mut stream: Tls,
    request: Vec<u8>,
    barrier: Arc<Barrier>,
    clock: CaptureClock,
) -> ProviderResult {
    barrier.wait();
    let write_started_ns = clock.now_ns();
    match stream.write_all(&request).and_then(|_| stream.flush()) {
        Err(e) => ProviderResult {
            provider,
            write_started_ns: Some(write_started_ns),
            write_flushed_ns: None,
            bytes_may_have_left: true,
            http_status: None,
            response_body: None,
            error: Some(format!("write/flush failed: {e}")),
        },
        Ok(()) => {
            let write_flushed_ns = clock.now_ns();
            match read_http(&mut stream) {
                Ok((status, body)) => ProviderResult {
                    provider,
                    write_started_ns: Some(write_started_ns),
                    write_flushed_ns: Some(write_flushed_ns),
                    bytes_may_have_left: true,
                    http_status: Some(status),
                    response_body: Some(body),
                    error: None,
                },
                Err(e) => ProviderResult {
                    provider,
                    write_started_ns: Some(write_started_ns),
                    write_flushed_ns: Some(write_flushed_ns),
                    bytes_may_have_left: true,
                    http_status: None,
                    response_body: None,
                    error: Some(format!("request sent; response unavailable: {e}")),
                },
            }
        }
    }
}

/// The result of a send thread that panicked after the release: its bytes
/// may have left, nothing else is known.
pub(crate) fn panicked_provider_result(provider: &'static str) -> ProviderResult {
    ProviderResult {
        provider,
        write_started_ns: None,
        write_flushed_ns: None,
        bytes_may_have_left: true,
        http_status: None,
        response_body: None,
        error: Some("send thread panicked after the release; bytes may have left".into()),
    }
}

pub(crate) fn provider_json(r: &ProviderResult) -> Value {
    let response_json = r
        .response_body
        .as_deref()
        .and_then(|body| serde_json::from_str::<Value>(body).ok());
    json!({
        "provider": r.provider,
        "write_started_ns": r.write_started_ns,
        "write_flushed_ns": r.write_flushed_ns,
        "bytes_may_have_left": r.bytes_may_have_left,
        "http_status": r.http_status.as_deref(),
        "response_json": response_json,
        "response_raw_body": r.response_body.as_deref(),
        "error": r.error.as_deref()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_value_rejects_empty_and_control_characters() {
        assert!(valid_header_value("Bearer abc-123_XYZ"));
        assert!(!valid_header_value(""));
        assert!(!valid_header_value("abc\r\nX-Injected: 1"));
        assert!(!valid_header_value("abc\tdef"));
        assert!(!valid_header_value("tökén"));
    }

    #[test]
    fn http_parsing_waits_for_complete_bodies_and_never_panics_on_lengths() {
        let ok = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
        let r = parse_http_response(ok).unwrap().unwrap();
        assert_eq!((r.status.as_str(), r.close, r.body.as_str()), ("HTTP/1.1 200 OK", true, "{}"));
        let keep = b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}";
        assert!(!parse_http_response(keep).unwrap().unwrap().close);
        assert!(parse_http_response(&ok[..ok.len() - 1]).unwrap().is_none());
        assert!(parse_http_response(b"HTTP/1.1 200 OK\r\nContent-Length: 2").unwrap().is_none());
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n";
        assert_eq!(parse_http_response(chunked).unwrap().unwrap().body, "{}");
        let huge = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", usize::MAX);
        assert!(parse_http_response(huge.as_bytes()).is_err());
        let huge_chunk = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\nab";
        assert!(parse_http_response(huge_chunk).unwrap().is_none());
        assert!(decode_chunked_body(b"fffffffffffffffe\r\n").is_none());
        assert!(parse_http_response(&vec![b'x'; MAX_HTTP_RESPONSE_BYTES + 1]).is_err());
    }

    fn unhandshaked_client(listener: &std::net::TcpListener) -> Tls {
        let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let conn = ClientConnection::new(
            Arc::new(config),
            ServerName::try_from("localhost".to_string()).unwrap(),
        )
        .unwrap();
        StreamOwned::new(conn, TcpStream::connect(listener.local_addr().unwrap()).unwrap())
    }

    #[test]
    fn liveness_refuses_closed_or_garbage_peers_and_restores_blocking_mode() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();

        let mut silent = unhandshaked_client(&listener);
        let (_silent_peer, _) = listener.accept().unwrap();
        assert!(transport_alive(&mut silent).is_ok());
        silent
            .sock
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let started = Instant::now();
        assert!(silent.sock.read(&mut [0u8; 1]).is_err());
        assert!(started.elapsed() >= Duration::from_millis(80), "socket left non-blocking");

        let mut closed = unhandshaked_client(&listener);
        drop(listener.accept().unwrap());
        thread::sleep(Duration::from_millis(50));
        let e = transport_alive(&mut closed).unwrap_err();
        assert!(e.contains("closed"), "{e}");

        let mut garbage = unhandshaked_client(&listener);
        let (mut garbage_peer, _) = listener.accept().unwrap();
        garbage_peer.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").unwrap();
        thread::sleep(Duration::from_millis(50));
        let e = transport_alive(&mut garbage).unwrap_err();
        assert!(e.contains("TLS"), "{e}");
    }
}

//! G0.7-only builder submission: TLS connections to 48Club and BlockRazor,
//! HTTP framing and response parsing. Reached only after the acceptance guard.

use super::*;

pub(crate) fn connect_tls(host: &str) -> Result<Tls, String> {
    let addrs: Vec<_> = (host, HTTPS_PORT)
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

pub(crate) fn http_request(host: &str, body: &str, authorization: Option<&str>) -> Vec<u8> {
    let auth = authorization.map_or_else(String::new, |token| {
        assert!(
            !token.is_empty()
                && token
                    .bytes()
                    .all(|b| (0x21..=0x7e).contains(&b) || b == b' '),
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
        if raw.len() < data_end + 2 || &raw[data_end..data_end + 2] != b"\r\n" {
            return None;
        }
        out.extend_from_slice(&raw[data_start..data_end]);
        pos = data_end + 2;
    }
}

pub(crate) fn read_http(stream: &mut Tls) -> Result<(String, String), String> {
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
        let Some(h) = find_header_end(&buf) else {
            continue;
        };
        let header = String::from_utf8_lossy(&buf[..h]).to_string();
        let lower = header.to_ascii_lowercase();
        if lower.contains("transfer-encoding: chunked") {
            let Some(decoded) = decode_chunked_body(&buf[h + 4..]) else {
                continue;
            };
            return Ok((
                header.lines().next().unwrap_or("").to_string(),
                String::from_utf8_lossy(&decoded).to_string(),
            ));
        }
        let content_length = lower.lines().find_map(|line| {
            line.strip_prefix("content-length:")
                .and_then(|x| x.trim().parse::<usize>().ok())
        });
        let Some(content_length) = content_length else {
            continue;
        };
        let full = h + 4 + content_length;
        if buf.len() >= full {
            return Ok((
                header.lines().next().unwrap_or("").to_string(),
                String::from_utf8_lossy(&buf[h + 4..full]).to_string(),
            ));
        }
    }
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

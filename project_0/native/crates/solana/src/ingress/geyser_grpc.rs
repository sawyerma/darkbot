//! S1-02 transport: the gRPC `geyser.Geyser/Subscribe` stream over HTTP/2 (`h2`), TLS (rustls,
//! `ring`, webpki roots, ALPN h2) for every remote endpoint, plaintext only on loopback. The
//! request stream stays open for pongs. The token (`x-token`) is read once from the environment,
//! never stored in the repository and never printed (`Debug` redacts it). A gRPC status that a
//! retry cannot fix rejects the source with its reason (auth, quota, capability, request); network
//! failures are retryable. No connection is made here except to the configured endpoint — Mainnet
//! endpoints are not used in this phase (tests run a loopback HTTP/2 server).
use super::geyser::{frame, proto, GeyserStream, GeyserTransport, StreamError};
use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;

/// A parsed endpoint: host, port, authority and whether TLS is used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub tls: bool,
}

impl Endpoint {
    /// `https://host[:port][/]`; `http://` only for loopback (`127.0.0.1`, `localhost`, `[::1]`).
    pub fn parse(url: &str) -> Result<Self, StreamError> {
        let (tls, rest) = if let Some(r) = url.strip_prefix("https://") {
            (true, r)
        } else if let Some(r) = url.strip_prefix("http://") {
            (false, r)
        } else {
            return Err(StreamError::Rejected("GEYSER_ENDPOINT_SCHEME"));
        };
        let authority = rest.trim_end_matches('/');
        if authority.is_empty() || authority.contains('/') || authority.contains('@') {
            return Err(StreamError::Rejected("GEYSER_ENDPOINT_INVALID"));
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => (
                h.to_string(),
                p.parse()
                    .map_err(|_| StreamError::Rejected("GEYSER_ENDPOINT_PORT"))?,
            ),
            _ => (authority.to_string(), if tls { 443 } else { 80 }),
        };
        let loopback = matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]");
        if !tls && !loopback {
            return Err(StreamError::Rejected("GEYSER_TLS_REQUIRED"));
        }
        Ok(Self { host, port, tls })
    }

    fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// gRPC status codes (`grpc-status`) as a rejection or a retry.
pub fn status_error(code: u32) -> Option<StreamError> {
    Some(match code {
        0 => return None,
        3 => StreamError::Rejected("GEYSER_REQUEST_REJECTED"),
        7 => StreamError::Rejected("GEYSER_AUTH_PERMISSION_DENIED"),
        8 => StreamError::Rejected("GEYSER_QUOTA_EXHAUSTED"),
        12 => StreamError::Rejected("GEYSER_CAPABILITY_UNIMPLEMENTED"),
        16 => StreamError::Rejected("GEYSER_AUTH_UNAUTHENTICATED"),
        other => StreamError::Retryable(format!("GRPC_STATUS_{other}")),
    })
}

fn grpc_status(headers: &http::HeaderMap) -> Option<u32> {
    headers
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
}

/// The production transport. One multi-thread runtime with one worker drives the HTTP/2
/// connection; the source's blocking calls wait on it.
pub struct GrpcTransport {
    endpoint: Endpoint,
    token: Option<String>,
    rt: Arc<tokio::runtime::Runtime>,
    connect_timeout: Duration,
    idle_timeout: Duration,
}

impl std::fmt::Debug for GrpcTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GrpcTransport")
            .field("endpoint", &self.endpoint)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl GrpcTransport {
    /// `token`: the provider's token (from the environment by the caller); `None` when the
    /// provider needs none.
    pub fn new(
        endpoint: &str,
        token: Option<String>,
        connect_timeout: Duration,
        idle_timeout: Duration,
    ) -> Result<Self, StreamError> {
        let endpoint = Endpoint::parse(endpoint)?;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|_| StreamError::Rejected("GEYSER_RUNTIME"))?;
        Ok(Self {
            endpoint,
            token,
            rt: Arc::new(rt),
            connect_timeout,
            idle_timeout,
        })
    }

    async fn open<IO>(
        io: IO,
        authority: String,
        scheme: &'static str,
        token: Option<String>,
        request: Vec<u8>,
        timeout: Duration,
    ) -> Result<(h2::SendStream<Bytes>, h2::RecvStream), StreamError>
    where
        IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let retry = |what: &str| StreamError::Retryable(what.into());
        let (client, conn) = h2::client::handshake(io)
            .await
            .map_err(|_| retry("GRPC_HANDSHAKE"))?;
        tokio::spawn(conn);
        let mut client = client.ready().await.map_err(|_| retry("GRPC_NOT_READY"))?;
        let mut req = http::Request::builder()
            .method("POST")
            .uri(format!("{scheme}://{authority}/geyser.Geyser/Subscribe"))
            .header("content-type", "application/grpc")
            .header("te", "trailers");
        if let Some(t) = &token {
            req = req.header("x-token", t.as_str());
        }
        let req = req
            .body(())
            .map_err(|_| StreamError::Rejected("GEYSER_TOKEN_INVALID"))?;
        let (response, mut send) = client
            .send_request(req, false)
            .map_err(|_| retry("GRPC_SEND"))?;
        send.send_data(Bytes::from(request), false)
            .map_err(|_| retry("GRPC_SEND"))?;
        let response = tokio::time::timeout(timeout, response)
            .await
            .map_err(|_| retry("GRPC_RESPONSE_TIMEOUT"))?
            .map_err(|_| retry("GRPC_RESPONSE"))?;
        match response.status().as_u16() {
            200 => {}
            401 | 403 => return Err(StreamError::Rejected("GEYSER_AUTH_UNAUTHENTICATED")),
            429 => return Err(StreamError::Rejected("GEYSER_QUOTA_EXHAUSTED")),
            other => return Err(StreamError::Retryable(format!("HTTP_{other}"))),
        }
        // A trailers-only response carries the gRPC status in its headers.
        if let Some(e) = grpc_status(response.headers()).and_then(status_error) {
            return Err(e);
        }
        Ok((send, response.into_body()))
    }
}

pub struct GrpcStream {
    rt: Arc<tokio::runtime::Runtime>,
    send: h2::SendStream<Bytes>,
    body: h2::RecvStream,
    idle_timeout: Duration,
}

impl GeyserTransport for GrpcTransport {
    type Stream = GrpcStream;

    fn connect(&mut self, request: &[u8]) -> Result<GrpcStream, StreamError> {
        let request = {
            let mut f = vec![0u8];
            f.extend_from_slice(&(request.len() as u32).to_be_bytes());
            f.extend_from_slice(request);
            f
        };
        let (endpoint, token, timeout) = (
            self.endpoint.clone(),
            self.token.clone(),
            self.connect_timeout,
        );
        let (send, body) = self.rt.block_on(async move {
            let tcp = tokio::time::timeout(
                timeout,
                tokio::net::TcpStream::connect((
                    endpoint.host.trim_matches(['[', ']']),
                    endpoint.port,
                )),
            )
            .await
            .map_err(|_| StreamError::Retryable("GRPC_CONNECT_TIMEOUT".into()))?
            .map_err(|_| StreamError::Retryable("GRPC_CONNECT".into()))?;
            let _ = tcp.set_nodelay(true);
            if !endpoint.tls {
                return Self::open(tcp, endpoint.authority(), "http", token, request, timeout)
                    .await;
            }
            let mut roots = tokio_rustls::rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
            let mut config = tokio_rustls::rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .map_err(|_| StreamError::Rejected("GEYSER_TLS_CONFIG"))?
                .with_root_certificates(roots)
                .with_no_client_auth();
            config.alpn_protocols = vec![b"h2".to_vec()];
            let name = tokio_rustls::rustls::pki_types::ServerName::try_from(endpoint.host.clone())
                .map_err(|_| StreamError::Rejected("GEYSER_ENDPOINT_INVALID"))?;
            let tls = tokio::time::timeout(
                timeout,
                tokio_rustls::TlsConnector::from(Arc::new(config)).connect(name, tcp),
            )
            .await
            .map_err(|_| StreamError::Retryable("GRPC_TLS_TIMEOUT".into()))?
            .map_err(|_| StreamError::Retryable("GRPC_TLS".into()))?;
            if tls.get_ref().1.alpn_protocol() != Some(b"h2") {
                return Err(StreamError::Rejected("GEYSER_CAPABILITY_NO_HTTP2"));
            }
            Self::open(tls, endpoint.authority(), "https", token, request, timeout).await
        })?;
        Ok(GrpcStream {
            rt: self.rt.clone(),
            send,
            body,
            idle_timeout: self.idle_timeout,
        })
    }
}

impl GeyserStream for GrpcStream {
    fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, StreamError> {
        let (body, idle) = (&mut self.body, self.idle_timeout);
        self.rt.block_on(async move {
            match tokio::time::timeout(idle, body.data()).await {
                Err(_) => Err(StreamError::Retryable("GRPC_IDLE_TIMEOUT".into())),
                Ok(Some(Ok(bytes))) => {
                    let _ = body.flow_control().release_capacity(bytes.len());
                    Ok(Some(bytes.to_vec()))
                }
                Ok(Some(Err(_))) => Err(StreamError::Retryable("GRPC_STREAM".into())),
                Ok(None) => {
                    let trailers = tokio::time::timeout(idle, body.trailers())
                        .await
                        .map_err(|_| StreamError::Retryable("GRPC_TRAILERS_TIMEOUT".into()))?
                        .map_err(|_| StreamError::Retryable("GRPC_TRAILERS".into()))?;
                    match trailers
                        .as_ref()
                        .and_then(grpc_status)
                        .and_then(status_error)
                    {
                        Some(e) => Err(e),
                        None => Ok(None),
                    }
                }
            }
        })
    }

    fn pong(&mut self, id: i32) -> Result<(), StreamError> {
        let ping = proto::SubscribeRequest {
            ping: Some(proto::SubscribeRequestPing { id }),
            ..Default::default()
        };
        self.send
            .send_data(Bytes::from(frame(&ping)), false)
            .map_err(|_| StreamError::Retryable("GRPC_PONG".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::geyser::{
        frame, proto, subscribe_request, BinaryGeyserSource, FrameDecoder, GeyserConfig,
    };
    use super::super::{IngressEvent, IngressHalt, IngressSource, SlotWatch, Subscription};
    use super::*;
    use darkbot_common::channel::bounded;
    use darkbot_common::lifecycle::Shutdown;
    use darkbot_common::timing::Clock;
    use prost::Message;
    use std::sync::Mutex;

    /// What the loopback server answers one `Subscribe` call.
    #[derive(Clone)]
    enum Reply {
        /// DATA frames (a `None` waits for the client's pong), then trailers with this status.
        Frames(Vec<Option<Vec<u8>>>, u32),
        TrailersOnly(u32),
        Http(u16),
    }

    #[derive(Default)]
    struct Seen {
        calls: usize,
        path: String,
        token: Option<String>,
        request: Option<proto::SubscribeRequest>,
        pongs: Vec<i32>,
    }

    /// A loopback HTTP/2 gRPC server (never a remote endpoint).
    fn serve(reply: Reply) -> (String, Arc<Mutex<Seen>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Seen::default()));
        let log = seen.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                listener.set_nonblocking(true).unwrap();
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                loop {
                    let Ok((socket, _)) = listener.accept().await else {
                        return;
                    };
                    let (reply, log) = (reply.clone(), log.clone());
                    tokio::spawn(async move {
                        let Ok(mut conn) = h2::server::handshake(socket).await else {
                            return;
                        };
                        while let Some(Ok((request, mut respond))) = conn.accept().await {
                            let (reply, log) = (reply.clone(), log.clone());
                            tokio::spawn(async move {
                                let (parts, mut body) = request.into_parts();
                                let mut frames = FrameDecoder::new(1 << 20);
                                {
                                    let mut s = log.lock().unwrap();
                                    s.calls += 1;
                                    s.path = parts.uri.path().into();
                                    s.token = parts
                                        .headers
                                        .get("x-token")
                                        .map(|v| v.to_str().unwrap().into());
                                }
                                if let Some(Ok(d)) = body.data().await {
                                    frames.push(&d);
                                    let _ = body.flow_control().release_capacity(d.len());
                                    log.lock().unwrap().request = frames.next_message().unwrap();
                                }
                                let grpc = |status: Option<u32>| {
                                    let mut r = http::Response::builder()
                                        .status(200)
                                        .header("content-type", "application/grpc");
                                    if let Some(s) = status {
                                        r = r.header("grpc-status", s.to_string());
                                    }
                                    r.body(()).unwrap()
                                };
                                match reply {
                                    Reply::Http(code) => {
                                        let r = http::Response::builder()
                                            .status(code)
                                            .body(())
                                            .unwrap();
                                        let _ = respond.send_response(r, true);
                                    }
                                    Reply::TrailersOnly(status) => {
                                        let _ = respond.send_response(grpc(Some(status)), true);
                                    }
                                    Reply::Frames(data, status) => {
                                        let mut send =
                                            respond.send_response(grpc(None), false).unwrap();
                                        for f in data {
                                            match f {
                                                Some(bytes) => {
                                                    send.send_data(bytes.into(), false).unwrap()
                                                }
                                                None => {
                                                    if let Some(Ok(d)) = body.data().await {
                                                        frames.push(&d);
                                                        let ping: proto::SubscribeRequest =
                                                            frames.next_message().unwrap().unwrap();
                                                        log.lock()
                                                            .unwrap()
                                                            .pongs
                                                            .push(ping.ping.unwrap().id);
                                                    }
                                                }
                                            }
                                        }
                                        let mut trailers = http::HeaderMap::new();
                                        trailers.insert(
                                            "grpc-status",
                                            status.to_string().parse().unwrap(),
                                        );
                                        let _ = send.send_trailers(trailers);
                                    }
                                }
                            });
                        }
                    });
                }
            });
        });
        (url, seen)
    }

    fn account_frame(slot: u64) -> Vec<u8> {
        frame(&proto::SubscribeUpdate {
            filters: vec![],
            update_oneof: Some(proto::subscribe_update::UpdateOneof::Account(
                proto::SubscribeUpdateAccount {
                    account: Some(proto::SubscribeUpdateAccountInfo {
                        pubkey: vec![1; 32],
                        owner: vec![2; 32],
                        write_version: 1,
                        ..Default::default()
                    }),
                    slot,
                    is_startup: false,
                    bank_id: None,
                },
            )),
        })
    }

    fn ping_frame() -> Vec<u8> {
        frame(&proto::SubscribeUpdate {
            filters: vec![],
            update_oneof: Some(proto::subscribe_update::UpdateOneof::Ping(
                Default::default(),
            )),
        })
    }

    fn transport(url: &str, token: Option<&str>) -> GrpcTransport {
        GrpcTransport::new(
            url,
            token.map(str::to_string),
            Duration::from_secs(2),
            Duration::from_secs(2),
        )
        .unwrap()
    }

    /// The production transport against a loopback HTTP/2 server: the request goes to
    /// `/geyser.Geyser/Subscribe` with the token header and the encoded subscription; DATA
    /// chunks arrive in order, a ping gets its pong on the open request stream, `grpc-status: 0`
    /// ends the stream cleanly. The token never appears in `Debug`.
    #[test]
    fn transport_streams_frames_answers_pings_and_ends_cleanly() {
        let (url, seen) = serve(Reply::Frames(
            vec![
                Some(account_frame(5)),
                Some(ping_frame()),
                None,
                Some(account_frame(6)),
            ],
            0,
        ));
        let mut t = transport(&url, Some("secret-token"));
        assert!(!format!("{t:?}").contains("secret-token"));
        let request = subscribe_request(
            &Subscription {
                slots: true,
                ..Default::default()
            },
            proto::CommitmentLevel::Confirmed,
            None,
        );
        let mut stream = t.connect(&request).unwrap();
        let mut frames = FrameDecoder::new(1 << 20);
        let mut updates = Vec::new();
        while let Some(chunk) = stream.next_chunk().unwrap() {
            frames.push(&chunk);
            while let Some(u) = frames.next_message::<proto::SubscribeUpdate>().unwrap() {
                if matches!(
                    u.update_oneof,
                    Some(proto::subscribe_update::UpdateOneof::Ping(_))
                ) {
                    stream.pong(1).unwrap();
                }
                updates.push(u);
            }
        }
        assert_eq!(updates.len(), 3);
        let s = seen.lock().unwrap();
        assert_eq!(s.path, "/geyser.Geyser/Subscribe");
        assert_eq!(s.token.as_deref(), Some("secret-token"));
        assert_eq!(s.request.as_ref().unwrap().commitment, Some(1));
        assert!(s.request.as_ref().unwrap().slots.contains_key("slots"));
        assert_eq!(s.pongs, [1]);
    }

    /// A rejection no retry can fix stops the source at once with its reason (Fatal, halt), on
    /// a trailers-only status, a status after data or an HTTP status — the server sees one call.
    #[test]
    fn auth_quota_and_capability_rejections_stop_without_retry() {
        for (reply, code) in [
            (Reply::TrailersOnly(16), "GEYSER_AUTH_UNAUTHENTICATED"),
            (Reply::TrailersOnly(7), "GEYSER_AUTH_PERMISSION_DENIED"),
            (Reply::TrailersOnly(8), "GEYSER_QUOTA_EXHAUSTED"),
            (
                Reply::Frames(vec![Some(account_frame(9))], 12),
                "GEYSER_CAPABILITY_UNIMPLEMENTED",
            ),
            (Reply::Http(429), "GEYSER_QUOTA_EXHAUSTED"),
            (Reply::Http(401), "GEYSER_AUTH_UNAUTHENTICATED"),
        ] {
            let (url, seen) = serve(reply);
            let mut config = GeyserConfig::publicnode();
            config.endpoint = url.clone();
            config.max_reconnects = 3;
            let source = BinaryGeyserSource {
                config,
                subscription: Subscription::default(),
                transport: transport(&url, None),
                clock: Clock::new(),
            };
            let (tx, rx) = bounded("geyser", 16);
            let halt = IngressHalt::new();
            let err = source
                .run(tx, Shutdown::new(), halt.clone(), SlotWatch::new())
                .unwrap_err();
            assert_eq!(err.to_string(), format!("ingress protocol: {code}"));
            assert!(halt.is_raised());
            let events: Vec<_> = std::iter::from_fn(|| rx.try_pop()).collect();
            assert!(
                matches!(events.last(), Some(IngressEvent::Fatal { .. })),
                "{code}"
            );
            assert_eq!(seen.lock().unwrap().calls, 1, "{code}: no retry");
        }
    }

    /// A provider that cannot be reached is retried within the reconnect budget, then the
    /// source ends Fatal (fail closed); shutdown before the first connect makes no connection.
    #[test]
    fn unreachable_provider_exhausts_reconnects_and_shutdown_stops_first() {
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", closed.local_addr().unwrap());
        drop(closed);
        let mut config = GeyserConfig::publicnode();
        config.endpoint = url.clone();
        config.max_reconnects = 2;
        let source = |config: GeyserConfig| BinaryGeyserSource {
            config,
            subscription: Subscription::default(),
            transport: transport(&url, None),
            clock: Clock::new(),
        };
        let (tx, rx) = bounded("geyser", 16);
        let err = source(config.clone())
            .run(tx, Shutdown::new(), IngressHalt::new(), SlotWatch::new())
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "ingress protocol: GEYSER_RECONNECTS_EXHAUSTED"
        );
        assert!(matches!(rx.try_pop(), Some(IngressEvent::Fatal { .. })));
        let stop = Shutdown::new();
        stop.request();
        let (tx, rx) = bounded("geyser", 16);
        assert!(source(config)
            .run(tx, stop, IngressHalt::new(), SlotWatch::new())
            .is_ok());
        assert!(rx.try_pop().is_none());
    }

    /// Endpoint rules without any network: TLS for every remote endpoint, plaintext on loopback
    /// only; malformed endpoints are rejected.
    #[test]
    fn endpoints_need_tls_except_loopback() {
        assert_eq!(
            Endpoint::parse("https://solana.publicnode.com").unwrap(),
            Endpoint {
                host: "solana.publicnode.com".into(),
                port: 443,
                tls: true
            }
        );
        assert_eq!(
            Endpoint::parse("http://127.0.0.1:10000").unwrap().port,
            10000
        );
        for (url, code) in [
            ("http://solana.publicnode.com", "GEYSER_TLS_REQUIRED"),
            ("grpc://x", "GEYSER_ENDPOINT_SCHEME"),
            ("https://user@host", "GEYSER_ENDPOINT_INVALID"),
            ("https://host:notaport", "GEYSER_ENDPOINT_PORT"),
        ] {
            assert_eq!(
                Endpoint::parse(url).unwrap_err(),
                StreamError::Rejected(code),
                "{url}"
            );
        }
        assert!(GrpcTransport::new(
            "http://solana.publicnode.com",
            None,
            Duration::ZERO,
            Duration::ZERO
        )
        .is_err());
        let _ = proto::SubscribeRequest::default().encode_to_vec();
    }
}

//! DEC-17c (S3-R01): `JitoGrpcSubmitter` — the block engine's searcher gRPC
//! (`searcher.SearcherService`: `SendBundle`, `GetTipAccounts`) over the HTTP/2 + TLS transport of
//! the Yellowstone source (`ingress::geyser_grpc`: rustls with `ring`, webpki roots, ALPN h2;
//! plaintext only on loopback). The protobuf messages are those of the pinned, vendored source
//! (`proto/jito/`, jito-labs/mev-protos [`PROTO_SOURCE_COMMIT`], Apache-2.0), transcribed below
//! with the fields this adapter writes or reads — no external Jito crate; the pins and the
//! transcription are tested against the vendored files. FREE_ONLY: no auth keypair (no
//! `AuthService` challenge — a keypair is only an optional field after a measured advantage,
//! NOT_MEASURED); Jito's current access policy is observed at the real access (an
//! UNAUTHENTICATED/PERMISSION_DENIED answer is `JITO_AUTH_REQUIRED`), never assumed. Offline
//! preparation: production use waits for S1-RACE PASS (S3-R02/R03 select it).
use super::{SubmissionAdapter, SubmissionPath, SubmitAck, SubmitError};
use crate::ingress::geyser_grpc::Endpoint;
use bytes::Bytes;
use prost::Message;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::transaction::VersionedTransaction;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The pinned mev-protos commit of the vendored `proto/jito/` files.
pub const PROTO_SOURCE_COMMIT: &str = "46ead86a13a55a0ef2c139db96a8ee93bf7505e3";
/// The vendored files and their SHA-256 at [`PROTO_SOURCE_COMMIT`].
pub const PROTO_FILES: [(&str, &str); 5] = [
    (
        "searcher.proto",
        "e0124eb65bb67688ff0bfe618682cfe66fc562465ce53080e6cedb835f8aa39a",
    ),
    (
        "bundle.proto",
        "5063a448a84a510f4f67de3e70c7ee6dc9d24fd12a8aca488095ddb6937fa0f0",
    ),
    (
        "packet.proto",
        "74ed8ffb9ebe58eac71a339b2d36433a122f0def26b36be76da692b9be6a7511",
    ),
    (
        "shared.proto",
        "7e8feaf9c77e7f1a947ad5805e99489509a17f55e78eb5e997158c52ab2b5c72",
    ),
    (
        "LICENSE",
        "c1bfcace4b6c3f520b621f4ce0edca75d2480fd42dfce74e0de23fc92803646d",
    ),
];
/// The gRPC service of the searcher API.
const SERVICE: &str = "searcher.SearcherService";

/// Messages of the vendored `searcher.proto`, `bundle.proto`, `packet.proto`, `shared.proto`
/// (only the fields used; prost skips every other field of a known message).
pub mod proto {
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SendBundleRequest {
        #[prost(message, optional, tag = "1")]
        pub bundle: Option<Bundle>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SendBundleResponse {
        #[prost(string, tag = "1")]
        pub uuid: String,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Bundle {
        #[prost(message, optional, tag = "2")]
        pub header: Option<Header>,
        #[prost(message, repeated, tag = "3")]
        pub packets: Vec<Packet>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Header {
        #[prost(message, optional, tag = "1")]
        pub ts: Option<Timestamp>,
    }
    /// `google.protobuf.Timestamp`.
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Timestamp {
        #[prost(int64, tag = "1")]
        pub seconds: i64,
        #[prost(int32, tag = "2")]
        pub nanos: i32,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Packet {
        #[prost(bytes = "vec", tag = "1")]
        pub data: Vec<u8>,
        #[prost(message, optional, tag = "2")]
        pub meta: Option<Meta>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Meta {
        #[prost(uint64, tag = "1")]
        pub size: u64,
        #[prost(string, tag = "2")]
        pub addr: String,
        #[prost(uint32, tag = "3")]
        pub port: u32,
        #[prost(message, optional, tag = "4")]
        pub flags: Option<PacketFlags>,
        #[prost(uint64, tag = "5")]
        pub sender_stake: u64,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct PacketFlags {
        #[prost(bool, tag = "1")]
        pub discard: bool,
        #[prost(bool, tag = "2")]
        pub forwarded: bool,
        #[prost(bool, tag = "3")]
        pub repair: bool,
        #[prost(bool, tag = "4")]
        pub simple_vote_tx: bool,
        #[prost(bool, tag = "5")]
        pub tracer_packet: bool,
        #[prost(bool, tag = "6")]
        pub from_staked_node: bool,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct GetTipAccountsRequest {}
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct GetTipAccountsResponse {
        #[prost(string, repeated, tag = "1")]
        pub accounts: Vec<String>,
    }
}

/// One gRPC message frame (uncompressed).
fn frame<M: Message>(m: &M) -> Vec<u8> {
    let body = m.encode_to_vec();
    let mut f = Vec::with_capacity(5 + body.len());
    f.push(0);
    f.extend_from_slice(&(body.len() as u32).to_be_bytes());
    f.extend_from_slice(&body);
    f
}

/// The one message of a unary response body.
fn unframe(body: &[u8]) -> Result<&[u8], SubmitError> {
    let bad = |what: &str| SubmitError::Transport(format!("JITO_GRPC_FRAME: {what}"));
    if body.len() < 5 {
        return Err(bad("short"));
    }
    if body[0] != 0 {
        return Err(bad("compressed"));
    }
    let len = u32::from_be_bytes([body[1], body[2], body[3], body[4]]) as usize;
    if body.len() != 5 + len {
        return Err(bad("length"));
    }
    Ok(&body[5..])
}

/// A gRPC status of the block engine (`grpc-status`, 0 = OK): a refusal, an ambiguous deadline
/// (the bundle may have been forwarded) or a transport failure.
pub fn status_error(code: u32) -> Option<SubmitError> {
    Some(match code {
        0 => return None,
        3 => SubmitError::Rejected("JITO_GRPC_INVALID_ARGUMENT".into()),
        4 => SubmitError::Deadline { sent: true },
        7 | 16 => SubmitError::Rejected("JITO_AUTH_REQUIRED".into()),
        8 => SubmitError::Rejected("JITO_RATE_LIMITED".into()),
        12 => SubmitError::Rejected("JITO_GRPC_UNIMPLEMENTED".into()),
        other => SubmitError::Transport(format!("JITO_GRPC_STATUS_{other}")),
    })
}

fn grpc_status(headers: &http::HeaderMap) -> Option<u32> {
    headers
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
}

/// The searcher gRPC adapter: one HTTP/2 connection, opened at the first call and again after a
/// failure; every call bounded by its deadline (or the configured timeout).
pub struct JitoGrpcSubmitter {
    endpoint: Endpoint,
    rt: Arc<tokio::runtime::Runtime>,
    conn: Mutex<Option<h2::client::SendRequest<Bytes>>>,
    timeout: Duration,
}

impl std::fmt::Debug for JitoGrpcSubmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JitoGrpcSubmitter")
            .field("endpoint", &self.endpoint)
            .finish()
    }
}

impl JitoGrpcSubmitter {
    /// `block_engine_url`: `https://host[:port]` (`http://` only on loopback).
    pub fn new(block_engine_url: &str, timeout: Duration) -> Result<Self, SubmitError> {
        let endpoint = Endpoint::parse(block_engine_url)
            .map_err(|_| SubmitError::Rejected("JITO_GRPC_ENDPOINT_INVALID".into()))?;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            // P6-MT.12: its worker makes the block engine's remote calls.
            .on_thread_start(|| darkbot_common::qos::enter(darkbot_common::qos::ThreadRole::Remote))
            .enable_all()
            .build()
            .map_err(|_| SubmitError::Transport("JITO_GRPC_RUNTIME".into()))?;
        Ok(Self {
            endpoint,
            rt: Arc::new(rt),
            conn: Mutex::new(None),
            timeout,
        })
    }

    async fn connect(
        endpoint: Endpoint,
        timeout: Duration,
    ) -> Result<h2::client::SendRequest<Bytes>, SubmitError> {
        let transport = |what: &str| SubmitError::Transport(what.into());
        let tcp = tokio::time::timeout(
            timeout,
            tokio::net::TcpStream::connect((endpoint.host.trim_matches(['[', ']']), endpoint.port)),
        )
        .await
        .map_err(|_| transport("JITO_GRPC_CONNECT_TIMEOUT"))?
        .map_err(|_| transport("JITO_GRPC_CONNECT"))?;
        let _ = tcp.set_nodelay(true);
        if !endpoint.tls {
            return Self::handshake(tcp).await;
        }
        let config = crate::ingress::geyser_grpc::h2_tls_config()
            .map_err(|_| SubmitError::Rejected("JITO_GRPC_TLS_CONFIG".into()))?;
        let name = tokio_rustls::rustls::pki_types::ServerName::try_from(endpoint.host.clone())
            .map_err(|_| SubmitError::Rejected("JITO_GRPC_ENDPOINT_INVALID".into()))?;
        let tls = tokio::time::timeout(
            timeout,
            tokio_rustls::TlsConnector::from(config).connect(name, tcp),
        )
        .await
        .map_err(|_| transport("JITO_GRPC_TLS_TIMEOUT"))?
        .map_err(|_| transport("JITO_GRPC_TLS"))?;
        if tls.get_ref().1.alpn_protocol() != Some(b"h2") {
            return Err(SubmitError::Rejected("JITO_GRPC_NO_HTTP2".into()));
        }
        Self::handshake(tls).await
    }

    async fn handshake<IO>(io: IO) -> Result<h2::client::SendRequest<Bytes>, SubmitError>
    where
        IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let (client, conn) = h2::client::handshake(io)
            .await
            .map_err(|_| SubmitError::Transport("JITO_GRPC_HANDSHAKE".into()))?;
        tokio::spawn(conn);
        Ok(client)
    }

    /// One unary call of `method`: nothing starts after `deadline`; a failure once the request
    /// was sent is ambiguous at the deadline (`Deadline { sent: true }`).
    fn unary<Req: Message, Resp: Message + Default>(
        &self,
        method: &str,
        request: &Req,
        deadline: Option<Instant>,
    ) -> Result<Resp, SubmitError> {
        let remaining = match deadline {
            Some(d) => d
                .saturating_duration_since(Instant::now())
                .min(self.timeout),
            None => self.timeout,
        };
        if remaining.is_zero() {
            return Err(SubmitError::Deadline { sent: false });
        }
        let cached = self
            .conn
            .lock()
            .map_err(|_| SubmitError::Transport("JITO_GRPC_LOCK".into()))?
            .clone();
        let (endpoint, scheme) = (
            self.endpoint.clone(),
            if self.endpoint.tls { "https" } else { "http" },
        );
        let uri = format!(
            "{scheme}://{}:{}/{SERVICE}/{method}",
            endpoint.host, endpoint.port
        );
        let body = Bytes::from(frame(request));
        let call = async move {
            let client = match cached {
                Some(c) => c,
                None => Self::connect(endpoint, remaining).await?,
            };
            let mut ready = client
                .clone()
                .ready()
                .await
                .map_err(|_| SubmitError::Transport("JITO_GRPC_NOT_READY".into()))?;
            let req = http::Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/grpc")
                .header("te", "trailers")
                .body(())
                .map_err(|_| SubmitError::Rejected("JITO_GRPC_REQUEST".into()))?;
            let (response, mut send) = ready
                .send_request(req, false)
                .map_err(|_| SubmitError::Transport("JITO_GRPC_SEND".into()))?;
            send.send_data(body, true)
                .map_err(|_| SubmitError::Transport("JITO_GRPC_SEND".into()))?;
            // From here the request is out: a failure is ambiguous.
            let sent = |_| SubmitError::Deadline { sent: true };
            let response = response.await.map_err(sent)?;
            if response.status().as_u16() != 200 {
                return Err(match response.status().as_u16() {
                    401 | 403 => SubmitError::Rejected("JITO_AUTH_REQUIRED".into()),
                    429 => SubmitError::Rejected("JITO_RATE_LIMITED".into()),
                    other => SubmitError::Transport(format!("JITO_GRPC_HTTP_{other}")),
                });
            }
            // A trailers-only response carries the status in its headers.
            if let Some(e) = grpc_status(response.headers()).and_then(status_error) {
                return Err(e);
            }
            let mut body = response.into_body();
            let mut bytes = Vec::new();
            while let Some(chunk) = body.data().await {
                let chunk = chunk.map_err(sent)?;
                let _ = body.flow_control().release_capacity(chunk.len());
                bytes.extend_from_slice(&chunk);
            }
            let trailers = body.trailers().await.map_err(sent)?;
            match trailers.as_ref().and_then(grpc_status) {
                Some(code) => {
                    if let Some(e) = status_error(code) {
                        return Err(e);
                    }
                }
                None => return Err(SubmitError::Deadline { sent: true }),
            }
            Ok((client, bytes))
        };
        let outcome = self.rt.block_on(async move {
            tokio::time::timeout(remaining, call)
                .await
                .unwrap_or(Err(SubmitError::Deadline { sent: true }))
        });
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| SubmitError::Transport("JITO_GRPC_LOCK".into()))?;
        match outcome {
            Ok((client, bytes)) => {
                *conn = Some(client);
                Resp::decode(unframe(&bytes)?)
                    .map_err(|_| SubmitError::Transport("JITO_GRPC_DECODE".into()))
            }
            Err(e) => {
                // A refusal keeps the connection; any other failure opens a new one.
                if !matches!(e, SubmitError::Rejected(_)) {
                    *conn = None;
                }
                Err(e)
            }
        }
    }

    /// The tip accounts the block engine names now.
    pub fn tip_accounts(&self) -> Result<Vec<Pubkey>, SubmitError> {
        let r: proto::GetTipAccountsResponse =
            self.unary("GetTipAccounts", &proto::GetTipAccountsRequest {}, None)?;
        r.accounts
            .iter()
            .map(|a| {
                Pubkey::from_str(a)
                    .map_err(|_| SubmitError::Transport("JITO_GRPC_TIP_ACCOUNT".into()))
            })
            .collect()
    }

    /// The `SendBundle` request of `txs` (each one's wire bytes as a packet, its size in the
    /// packet's meta) — the bytes the block engine receives.
    pub fn bundle_request(
        txs: &[VersionedTransaction],
    ) -> Result<proto::SendBundleRequest, SubmitError> {
        let packets = txs
            .iter()
            .map(|t| {
                super::serialize_tx(t)
                    .map_err(|e| SubmitError::Serialize(e.to_string()))
                    .map(|data| proto::Packet {
                        meta: Some(proto::Meta {
                            size: data.len() as u64,
                            ..Default::default()
                        }),
                        data,
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(proto::SendBundleRequest {
            bundle: Some(proto::Bundle {
                header: None,
                packets,
            }),
        })
    }

    /// An atomic bundle (1..=[`BUNDLE_MAX_TXS`]): its uuid — no proof of landing.
    pub fn submit_bundle(
        &self,
        txs: &[VersionedTransaction],
        t_handoff_ns: u64,
        deadline: Instant,
    ) -> Result<SubmitAck, SubmitError> {
        let (sigs, sentinel) = super::jito::bundle_signatures(txs)?;
        let request = Self::bundle_request(txs)?;
        let serialized_len = request
            .bundle
            .as_ref()
            .map_or(0, |b| b.packets.iter().map(|p| p.data.len()).sum());
        let r: proto::SendBundleResponse = self.unary("SendBundle", &request, Some(deadline))?;
        if r.uuid.is_empty() {
            return Err(SubmitError::Deadline { sent: true });
        }
        Ok(SubmitAck {
            path: SubmissionPath::JitoBundle,
            signature: sentinel,
            signatures: sigs,
            submission_id: Some(r.uuid),
            t_handoff_ns,
            serialized_len,
        })
    }
}

impl SubmissionAdapter for JitoGrpcSubmitter {
    fn path(&self) -> SubmissionPath {
        SubmissionPath::JitoBundle
    }

    fn supports_bundle(&self) -> bool {
        true
    }

    fn submit_bundle(
        &self,
        txs: &[VersionedTransaction],
        t_handoff_ns: u64,
        deadline: Instant,
    ) -> Result<SubmitAck, SubmitError> {
        JitoGrpcSubmitter::submit_bundle(self, txs, t_handoff_ns, deadline)
    }

    /// The searcher API quotes no message fee (no network call).
    fn fee_for_message(
        &self,
        _: &VersionedTransaction,
        _: Instant,
    ) -> Result<super::MessageFeeQuote, super::FeeQuoteError> {
        Err(super::FeeQuoteError::Unavailable("JITO_PATH_NO_FEE_QUOTE"))
    }

    /// The searcher API simulates nothing (no network call).
    fn simulate(&self, _: &VersionedTransaction, _: Instant) -> super::SimResult {
        super::SimResult::Unavailable("no simulation over the searcher gRPC".into())
    }

    fn simulate_bundle(&self, _: &[VersionedTransaction], _: Instant) -> super::SimResult {
        super::SimResult::Unavailable("no bundle simulation over the searcher gRPC".into())
    }

    /// The searcher API takes bundles only (no network call).
    fn submit(
        &self,
        _: &VersionedTransaction,
        _: u64,
        _: Instant,
    ) -> Result<SubmitAck, SubmitError> {
        Err(SubmitError::Rejected("JITO_GRPC_BUNDLE_ONLY".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::jito::BUNDLE_MAX_TXS;
    use super::*;
    use solana_sdk::signature::{Keypair, Signer};

    /// The vendored files are exactly the pinned ones.
    #[test]
    fn dec17c_the_vendored_protos_are_the_pinned_source() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("proto/jito");
        for (name, sha) in PROTO_FILES {
            let bytes = std::fs::read(dir.join(name)).unwrap();
            // A Solana hash is the SHA-256 of the bytes.
            assert_eq!(
                hex(&solana_sdk::hash::hash(&bytes).to_bytes()),
                sha,
                "{name}"
            );
        }
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// Every transcribed field is the vendored file's (name, type, tag).
    #[test]
    fn dec17c_the_transcription_is_the_vendored_layout() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("proto/jito");
        let text: String = [
            "searcher.proto",
            "bundle.proto",
            "packet.proto",
            "shared.proto",
        ]
        .iter()
        .map(|f| std::fs::read_to_string(dir.join(f)).unwrap())
        .collect();
        let fields = |message: &str| -> Vec<String> {
            let start = text.find(&format!("message {message} {{")).unwrap();
            let body = &text[start..start + text[start..].find('}').unwrap()];
            body.lines()
                .skip(1)
                .map(|l| l.split("//").next().unwrap().trim())
                .filter(|l| l.ends_with(';'))
                .map(|l| {
                    l.trim_end_matches(';')
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect()
        };
        for (message, expected) in [
            ("SendBundleRequest", vec!["bundle.Bundle bundle = 1"]),
            ("SendBundleResponse", vec!["string uuid = 1"]),
            (
                "Bundle",
                vec![
                    "shared.Header header = 2",
                    "repeated packet.Packet packets = 3",
                ],
            ),
            ("Header", vec!["google.protobuf.Timestamp ts = 1"]),
            ("Packet", vec!["bytes data = 1", "Meta meta = 2"]),
            (
                "Meta",
                vec![
                    "uint64 size = 1",
                    "string addr = 2",
                    "uint32 port = 3",
                    "PacketFlags flags = 4",
                    "uint64 sender_stake = 5",
                ],
            ),
            (
                "GetTipAccountsResponse",
                vec!["repeated string accounts = 1"],
            ),
        ] {
            assert_eq!(fields(message), expected, "{message}");
        }
    }

    /// Golden: the wire bytes of a request and a response, derived by hand from the vendored
    /// tags (field 1 of the request, 3 of the bundle, 1/2 of the packet, 1 of the meta).
    #[test]
    fn dec17c_golden_wire_bytes() {
        let request = proto::SendBundleRequest {
            bundle: Some(proto::Bundle {
                header: None,
                packets: vec![proto::Packet {
                    data: vec![0xaa, 0xbb],
                    meta: Some(proto::Meta {
                        size: 2,
                        ..Default::default()
                    }),
                }],
            }),
        };
        assert_eq!(hex(&request.encode_to_vec()), "0a0a1a080a02aabb12020802");
        assert_eq!(hex(&frame(&request)), "000000000c0a0a1a080a02aabb12020802");
        let tips =
            proto::GetTipAccountsResponse::decode(&[0x0a, 0x01, b'a', 0x0a, 0x02, b'b', b'c'][..])
                .unwrap();
        assert_eq!(tips.accounts, ["a", "bc"]);
        let uuid = proto::SendBundleResponse::decode(&[0x0a, 0x02, b'i', b'd'][..]).unwrap();
        assert_eq!(uuid.uuid, "id");
    }

    /// What the loopback block engine answers each call, and what it saw.
    #[derive(Clone)]
    enum Answer {
        Message(Vec<u8>),
        Status(u32),
        TrailersOnly(u32),
    }

    #[derive(Default)]
    struct Seen {
        accepts: usize,
        calls: Vec<(String, Vec<u8>)>,
    }

    /// A loopback HTTP/2 gRPC server (never a remote endpoint) answering every call with `answer`.
    fn serve(answer: Answer) -> (String, Arc<Mutex<Seen>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Seen::default()));
        let log = seen.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                listener.set_nonblocking(true).unwrap();
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                while let Ok((socket, _)) = listener.accept().await {
                    log.lock().unwrap().accepts += 1;
                    let (answer, log) = (answer.clone(), log.clone());
                    tokio::spawn(async move {
                        let Ok(mut conn) = h2::server::handshake(socket).await else {
                            return;
                        };
                        while let Some(Ok((request, mut respond))) = conn.accept().await {
                            // One task per request: the connection keeps being polled.
                            let (answer, log) = (answer.clone(), log.clone());
                            tokio::spawn(async move {
                                let (parts, mut body) = request.into_parts();
                                let mut bytes = Vec::new();
                                while let Some(Ok(d)) = body.data().await {
                                    let _ = body.flow_control().release_capacity(d.len());
                                    bytes.extend_from_slice(&d);
                                }
                                log.lock()
                                    .unwrap()
                                    .calls
                                    .push((parts.uri.path().into(), bytes));
                                let head = |status: Option<u32>| {
                                    let mut r = http::Response::builder()
                                        .status(200)
                                        .header("content-type", "application/grpc");
                                    if let Some(s) = status {
                                        r = r.header("grpc-status", s.to_string());
                                    }
                                    r.body(()).unwrap()
                                };
                                let trailers = |code: u32| {
                                    let mut t = http::HeaderMap::new();
                                    t.insert("grpc-status", code.to_string().parse().unwrap());
                                    t
                                };
                                match answer.clone() {
                                    Answer::TrailersOnly(code) => {
                                        let _ = respond.send_response(head(Some(code)), true);
                                    }
                                    Answer::Status(code) => {
                                        let mut send =
                                            respond.send_response(head(None), false).unwrap();
                                        let _ = send.send_trailers(trailers(code));
                                    }
                                    Answer::Message(m) => {
                                        let mut send =
                                            respond.send_response(head(None), false).unwrap();
                                        let mut f = vec![0u8];
                                        f.extend_from_slice(&(m.len() as u32).to_be_bytes());
                                        f.extend_from_slice(&m);
                                        send.send_data(f.into(), false).unwrap();
                                        let _ = send.send_trailers(trailers(0));
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

    fn signed_tx(payer: &Keypair) -> VersionedTransaction {
        let message = solana_sdk::message::VersionedMessage::Legacy(
            solana_sdk::message::Message::new_with_blockhash(
                &[super::super::tip_instruction(
                    &payer.pubkey(),
                    &Pubkey::new_unique(),
                    1,
                )],
                Some(&payer.pubkey()),
                &solana_sdk::hash::Hash::new_unique(),
            ),
        );
        VersionedTransaction::try_new(message, &[payer]).unwrap()
    }

    /// DEC-17c: a bundle goes as one `SendBundle` of its wire bytes and comes back with the
    /// engine's uuid; tip accounts are read; the connection is reused.
    #[test]
    fn dec17c_a_bundle_is_sent_over_the_searcher_grpc() {
        let (url, seen) = serve(Answer::Message(
            proto::SendBundleResponse {
                uuid: "bundle-1".into(),
            }
            .encode_to_vec(),
        ));
        let jito = JitoGrpcSubmitter::new(&url, Duration::from_secs(5)).unwrap();
        let payer = Keypair::new();
        let txs = [signed_tx(&payer), signed_tx(&payer)];
        let ack = jito
            .submit_bundle(&txs, 7, Instant::now() + Duration::from_secs(5))
            .unwrap();
        assert_eq!(
            (ack.path, ack.submission_id.as_deref(), ack.t_handoff_ns),
            (SubmissionPath::JitoBundle, Some("bundle-1"), 7)
        );
        assert_eq!(ack.signatures, [txs[0].signatures[0], txs[1].signatures[0]]);
        let again = jito
            .submit_bundle(&txs, 8, Instant::now() + Duration::from_secs(5))
            .unwrap();
        assert_eq!(again.submission_id.as_deref(), Some("bundle-1"));
        let s = seen.lock().unwrap();
        assert_eq!(s.accepts, 1, "one connection for both calls");
        let (path, body) = &s.calls[0];
        assert_eq!(path, "/searcher.SearcherService/SendBundle");
        let sent = proto::SendBundleRequest::decode(unframe(body).unwrap()).unwrap();
        assert_eq!(sent, JitoGrpcSubmitter::bundle_request(&txs).unwrap());
        assert_eq!(
            sent.bundle.unwrap().packets[1].data,
            super::super::serialize_tx(&txs[1]).unwrap()
        );
        drop(s);
        let tips = [Pubkey::new_unique(), Pubkey::new_unique()];
        let (url, seen) = serve(Answer::Message(
            proto::GetTipAccountsResponse {
                accounts: tips.iter().map(Pubkey::to_string).collect(),
            }
            .encode_to_vec(),
        ));
        let jito = JitoGrpcSubmitter::new(&url, Duration::from_secs(5)).unwrap();
        assert_eq!(jito.tip_accounts().unwrap(), tips);
        assert_eq!(
            seen.lock().unwrap().calls[0].0,
            "/searcher.SearcherService/GetTipAccounts"
        );
    }

    /// Refused before any byte leaves: an unsigned bundle, one too large, an expired deadline, a
    /// single transaction. The engine's refusals are named (auth, rate, argument); a server
    /// deadline is ambiguous.
    #[test]
    fn dec17c_refusals_are_named_and_nothing_leaves_unchecked() {
        let (url, seen) = serve(Answer::Status(16));
        let jito = JitoGrpcSubmitter::new(&url, Duration::from_secs(5)).unwrap();
        let payer = Keypair::new();
        let later = || Instant::now() + Duration::from_secs(5);
        let mut unsigned = signed_tx(&payer);
        unsigned.signatures[0] = Default::default();
        let e = jito.submit_bundle(&[unsigned], 1, later()).unwrap_err();
        assert!(
            matches!(e, SubmitError::Rejected(ref c) if c == "BUNDLE_SIGNATURE_MISSING"),
            "{e}"
        );
        let six: Vec<_> = (0..=BUNDLE_MAX_TXS).map(|_| signed_tx(&payer)).collect();
        assert!(matches!(
            jito.submit_bundle(&six, 1, later()),
            Err(SubmitError::Rejected(_))
        ));
        let past = Instant::now();
        assert!(matches!(
            jito.submit_bundle(&[signed_tx(&payer)], 1, past),
            Err(SubmitError::Deadline { sent: false })
        ));
        assert!(matches!(
            SubmissionAdapter::submit(&jito, &signed_tx(&payer), 1, later()),
            Err(SubmitError::Rejected(_))
        ));
        assert_eq!(seen.lock().unwrap().calls.len(), 0);
        let e = jito
            .submit_bundle(&[signed_tx(&payer)], 1, later())
            .unwrap_err();
        assert!(
            matches!(e, SubmitError::Rejected(ref c) if c == "JITO_AUTH_REQUIRED"),
            "{e}"
        );
        for (answer, expected) in [
            (Answer::TrailersOnly(8), "JITO_RATE_LIMITED"),
            (Answer::Status(3), "JITO_GRPC_INVALID_ARGUMENT"),
        ] {
            let (url, _) = serve(answer);
            let jito = JitoGrpcSubmitter::new(&url, Duration::from_secs(5)).unwrap();
            let e = jito
                .submit_bundle(&[signed_tx(&payer)], 1, later())
                .unwrap_err();
            assert!(
                matches!(e, SubmitError::Rejected(ref c) if c == expected),
                "{e}"
            );
        }
        let (url, _) = serve(Answer::Status(4));
        let jito = JitoGrpcSubmitter::new(&url, Duration::from_secs(5)).unwrap();
        assert!(matches!(
            jito.submit_bundle(&[signed_tx(&payer)], 1, later()),
            Err(SubmitError::Deadline { sent: true })
        ));
        // A remote endpoint needs TLS.
        assert!(
            JitoGrpcSubmitter::new("http://block-engine.invalid", Duration::from_secs(1)).is_err()
        );
    }
}

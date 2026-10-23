//! A loopback HTTP/2 gRPC `Subscribe` server for offline tests (never a remote endpoint): the
//! solana crate's transport tests and, with the `test-support` feature, the runtime's owner E2E.
use super::geyser::{proto, FrameDecoder};
use std::sync::{Arc, Mutex};

/// What the loopback server answers one `Subscribe` call.
#[derive(Clone)]
pub enum Reply {
    /// DATA frames (a `None` waits for the client's pong), then trailers with this status.
    Frames(Vec<Option<Vec<u8>>>, u32),
    TrailersOnly(u32),
    Http(u16),
    /// DATA frames, then a server ping every 50 ms until the client closes the stream (an open,
    /// live stream: the client never idles out and sees its shutdown between chunks).
    Hold(Vec<Vec<u8>>),
}

#[derive(Default)]
pub struct Seen {
    pub calls: usize,
    pub path: String,
    pub token: Option<String>,
    pub request: Option<proto::SubscribeRequest>,
    pub pongs: Vec<i32>,
}

/// A loopback HTTP/2 gRPC server (never a remote endpoint).
pub fn serve(reply: Reply) -> (String, Arc<Mutex<Seen>>) {
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
                                    let r =
                                        http::Response::builder().status(code).body(()).unwrap();
                                    let _ = respond.send_response(r, true);
                                }
                                Reply::TrailersOnly(status) => {
                                    let _ = respond.send_response(grpc(Some(status)), true);
                                }
                                Reply::Hold(data) => {
                                    let mut send =
                                        respond.send_response(grpc(None), false).unwrap();
                                    for bytes in data {
                                        if send.send_data(bytes.into(), false).is_err() {
                                            return;
                                        }
                                    }
                                    let ping = super::geyser::frame(&proto::SubscribeUpdate {
                                        filters: vec![],
                                        update_oneof: Some(
                                            proto::subscribe_update::UpdateOneof::Ping(
                                                Default::default(),
                                            ),
                                        ),
                                    });
                                    loop {
                                        tokio::time::sleep(std::time::Duration::from_millis(50))
                                            .await;
                                        if send.send_data(ping.clone().into(), false).is_err() {
                                            return;
                                        }
                                    }
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
                                    trailers
                                        .insert("grpc-status", status.to_string().parse().unwrap());
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

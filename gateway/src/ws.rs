use crate::{
    http::AppState,
    hub::Slot,
    overview::unix_ns_now,
    proto::{
        client_message, server_message, AckReason, AckStatus, ClientMessage, ControlAck, ErrorCode,
        Heartbeat, ServerLimits, ServerMessage, StateWatermark, TopicClass, TopicId, Welcome,
        WelcomeStatus, WireError,
    },
    session, topics, PROTOCOL_MAJOR, PROTOCOL_MINOR, SCHEMA_ID,
};
use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use prost::Message as _;
use std::{
    collections::HashMap,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tokio_stream::{wrappers::WatchStream, StreamExt, StreamMap};
pub async fn upgrade(
    State(app): State<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if session::check_origin(&headers, &app.cfg).is_err() {
        return (StatusCode::FORBIDDEN, "origin rejected").into_response();
    }
    if !session::has_valid_session(&headers, &app.cfg, &app.sessions) {
        return (StatusCode::UNAUTHORIZED, "session required").into_response();
    }
    // Atomic reservation prevents a concurrent upgrade from exceeding max_clients.
    let limit = app.cfg.limits.max_clients;
    if app
        .clients
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < limit).then_some(n + 1)
        })
        .is_err()
    {
        return (StatusCode::SERVICE_UNAVAILABLE, "client limit").into_response();
    }
    let size = app.cfg.limits.max_client_frame_bytes;
    ws.max_message_size(size)
        .max_frame_size(size)
        .on_upgrade(move |socket| run(app, socket))
}
struct Guard(Arc<AppState>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.clients.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Client {
    app: Arc<AppState>,
    socket: WebSocket,
    id: String,
    streams: StreamMap<TopicId, WatchStream<Slot>>,
    sent: HashMap<TopicId, (u64, u64)>,
    hb: u64,
    pending: Option<u64>,
    missed: u32,
}
async fn run(app: Arc<AppState>, socket: WebSocket) {
    let _guard = Guard(app.clone());
    let mut c = Client {
        app,
        socket,
        id: session::random_token(12),
        streams: StreamMap::new(),
        sent: HashMap::new(),
        hb: 0,
        pending: None,
        missed: 0,
    };
    if c.handshake().await.is_ok() {
        let _ = c.loop_frames().await;
    }
}
fn decode(m: &Message) -> Option<ClientMessage> {
    match m {
        Message::Binary(b) => ClientMessage::decode(b.as_slice()).ok(),
        _ => None,
    }
}
impl Client {
    /// A client that stops reading must not hold a connection slot: a send that cannot complete
    /// within the heartbeat liveness window closes the connection (Wire V1.3 §5).
    fn send_timeout(&self) -> Duration {
        let l = &self.app.cfg.limits;
        Duration::from_millis(
            u64::from(l.heartbeat_interval_ms) * u64::from(l.stale_after_missed_heartbeats.max(1)),
        )
        .max(Duration::from_secs(1))
    }
    async fn send(&mut self, m: server_message::Msg) -> Result<(), ()> {
        let frame = Message::Binary(ServerMessage { msg: Some(m) }.encode_to_vec());
        match tokio::time::timeout(self.send_timeout(), self.socket.send(frame)).await {
            Ok(r) => r.map_err(|_| ()),
            Err(_) => {
                tracing::warn!(connection = %self.id, "client stopped reading; connection closed");
                Err(())
            }
        }
    }
    /// Bounded like every send: a client that stopped reading cannot hold the slot while closing.
    async fn close(&mut self, code: u16, why: &'static str) {
        let frame = Message::Close(Some(CloseFrame {
            code,
            reason: why.into(),
        }));
        let _ = tokio::time::timeout(self.send_timeout(), self.socket.send(frame)).await;
    }
    async fn ack(
        &mut self,
        id: u32,
        topic: i32,
        status: AckStatus,
        reason: AckReason,
        detail: &str,
    ) -> Result<(), ()> {
        self.send(server_message::Msg::ControlAck(ControlAck {
            request_id: id,
            topic,
            status: status as i32,
            reason: reason as i32,
            detail: detail.to_string(),
        }))
        .await
    }
    async fn error(
        &mut self,
        topic: Option<TopicId>,
        code: ErrorCode,
        detail: &str,
    ) -> Result<(), ()> {
        self.send(server_message::Msg::Error(WireError {
            topic: topic.map(|t| t as i32).unwrap_or(0),
            request_id: None,
            code: code as i32,
            retryable: false,
            message: detail.to_string(),
        }))
        .await
    }
    async fn handshake(&mut self) -> Result<(), ()> {
        let timeout = Duration::from_millis(self.app.cfg.limits.handshake_timeout_ms);
        let incoming = tokio::time::timeout(timeout, self.socket.recv())
            .await
            .map_err(|_| ())?
            .ok_or(())?
            .map_err(|_| ())?;
        let hello = match decode(&incoming).and_then(|m| m.msg) {
            Some(client_message::Msg::Hello(h)) => h,
            _ => {
                self.close(1002, "expected Hello").await;
                return Err(());
            }
        };
        // Major decides compatibility; within a major only additive fields exist, so a different
        // schema_id (minor fingerprint) is diagnostic and never blocks (stream.proto, README §3).
        let (status, reason) = if hello.protocol_major != PROTOCOL_MAJOR {
            (WelcomeStatus::RejectedMajor, "major mismatch")
        } else {
            if hello.schema_id != SCHEMA_ID {
                tracing::info!(connection = %self.id, client_minor = hello.protocol_minor, client_schema = %hello.schema_id, "compatible minor schema skew");
            }
            (WelcomeStatus::Accepted, "")
        };
        let l = &self.app.cfg.limits;
        let welcome = Welcome {
            status: status as i32,
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            schema_id: SCHEMA_ID.to_owned(),
            connection_id: self.id.clone(),
            server_time_unix_ns: unix_ns_now(),
            limits: Some(ServerLimits {
                max_state_full_bytes: l.max_state_full_bytes,
                heartbeat_interval_ms: l.heartbeat_interval_ms,
                stale_after_missed_heartbeats: l.stale_after_missed_heartbeats,
                unsubscribe_grace_ms: l.unsubscribe_grace_ms,
                max_catchup_retries: l.max_catchup_retries,
            }),
            topics: topics::descriptors(),
            reject_reason: reason.to_owned(),
        };
        self.send(server_message::Msg::Welcome(welcome)).await?;
        if status != WelcomeStatus::Accepted {
            self.close(1002, "handshake rejected").await;
            return Err(());
        }
        Ok(())
    }
    async fn loop_frames(&mut self) -> Result<(), ()> {
        let mut timer = tokio::time::interval(Duration::from_millis(
            u64::from(self.app.cfg.limits.heartbeat_interval_ms).max(100),
        ));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        timer.tick().await;
        loop {
            tokio::select! {
            m=self.socket.recv()=>{let m=m.ok_or(())?.map_err(|_|())?;if !self.on_msg(m).await?{return Ok(())}},
            Some((topic,slot))=self.streams.next(),if !self.streams.is_empty()=>{if let Some(st)=slot{if st.too_large||st.payload.len()>self.app.cfg.limits.max_state_full_bytes as usize{self.streams.remove(&topic);self.sent.remove(&topic);self.error(Some(topic),ErrorCode::FrameTooLarge,"state too large").await?;}else{self.send(server_message::Msg::StateFull(st.to_frame())).await?;self.sent.insert(topic,(st.generation,st.seq));}}},
            _=timer.tick()=>{if self.pending.is_some(){self.missed+=1;if self.missed>=self.app.cfg.limits.stale_after_missed_heartbeats{self.close(1001,"pong timeout").await;return Err(())}}self.hb+=1;let id=self.hb;let watermark=self.sent.iter().map(|(t,(g,s))|StateWatermark{topic:*t as i32,generation:*g,highest_seq_sent:*s}).collect();self.send(server_message::Msg::Heartbeat(Heartbeat{heartbeat_id:id,server_time_unix_ns:unix_ns_now(),state_watermarks:watermark})).await?;self.pending=Some(id);}
            }
        }
    }
    async fn on_msg(&mut self, m: Message) -> Result<bool, ()> {
        match m {
            Message::Close(_) => return Ok(false),
            Message::Ping(_) | Message::Pong(_) => return Ok(true),
            _ => {}
        }
        let msg = match decode(&m).and_then(|m| m.msg) {
            Some(x) => x,
            None => {
                self.error(None, ErrorCode::ProtocolViolation, "invalid frame")
                    .await?;
                self.close(1002, "protocol violation").await;
                return Ok(false);
            }
        };
        match msg {
            client_message::Msg::Subscribe(s) => {
                let Some(spec) = topics::spec_from_wire(s.topic) else {
                    self.ack(
                        s.request_id,
                        s.topic,
                        AckStatus::Rejected,
                        AckReason::UnknownTopic,
                        "",
                    )
                    .await?;
                    return Ok(true);
                };
                if spec.class == TopicClass::Event || !spec.available() {
                    self.ack(
                        s.request_id,
                        s.topic,
                        AckStatus::Rejected,
                        AckReason::TopicUnavailable,
                        "topic unavailable in F3",
                    )
                    .await?;
                    return Ok(true);
                }
                if s.cursor.is_some() {
                    self.ack(
                        s.request_id,
                        s.topic,
                        AckStatus::Rejected,
                        AckReason::InvalidCursorForState,
                        "",
                    )
                    .await?;
                    return Ok(true);
                }
                if !self.streams.contains_key(&spec.id)
                    && self.streams.len() >= self.app.cfg.limits.max_subscriptions_per_client
                {
                    self.ack(
                        s.request_id,
                        s.topic,
                        AckStatus::Rejected,
                        AckReason::Unspecified,
                        "subscription limit",
                    )
                    .await?;
                    return Ok(true);
                }
                if let Some(rx) = self.app.hub.subscribe(spec.id) {
                    self.ack(
                        s.request_id,
                        s.topic,
                        AckStatus::Accepted,
                        AckReason::Unspecified,
                        "",
                    )
                    .await?;
                    self.streams.insert(spec.id, WatchStream::new(rx));
                }
            }
            client_message::Msg::Unsubscribe(u) => {
                let Some(spec) = topics::spec_from_wire(u.topic) else {
                    self.ack(
                        u.request_id,
                        u.topic,
                        AckStatus::Rejected,
                        AckReason::UnknownTopic,
                        "",
                    )
                    .await?;
                    return Ok(true);
                };
                if self.streams.remove(&spec.id).is_some() {
                    self.sent.remove(&spec.id);
                    self.ack(
                        u.request_id,
                        u.topic,
                        AckStatus::Accepted,
                        AckReason::Unspecified,
                        "",
                    )
                    .await?;
                } else {
                    self.ack(
                        u.request_id,
                        u.topic,
                        AckStatus::Rejected,
                        AckReason::NotSubscribed,
                        "",
                    )
                    .await?;
                }
            }
            client_message::Msg::Pong(p) => {
                if self.pending == Some(p.heartbeat_id) {
                    self.pending = None;
                    self.missed = 0;
                }
            }
            client_message::Msg::Hello(_) => {
                self.error(None, ErrorCode::ProtocolViolation, "duplicate Hello")
                    .await?;
                self.close(1002, "protocol violation").await;
                return Ok(false);
            }
        }
        Ok(true)
    }
}

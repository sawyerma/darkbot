use crate::{
    engine::{EngineFeed, FeedError, FeedMessage},
    hub::StateUpdate,
    proto::{engine_frame, EngineFrame, EngineHello, ProjectId, StateQuality, TopicId},
    topics, SCHEMA_ID,
};
use async_trait::async_trait;
use bytes::Bytes;
use prost::Message;
use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};
use tokio::{io::AsyncReadExt, net::UnixStream, sync::mpsc};
use tracing::warn;

/// Envelope reserve of the engine-feed contract (engine_feed.proto): no frame exceeds
/// max_state_full_bytes + ENVELOPE_RESERVE bytes.
pub const ENVELOPE_RESERVE: usize = 4096;
pub struct UnixSocketFeed {
    project: ProjectId,
    label: &'static str,
    path: PathBuf,
    max_payload: usize,
    max_frame: usize,
}
/// Exact largest valid STATE frame of one source: a StateFull carrying `max_payload` bytes with
/// every other field at its longest encoding (u64::MAX, i64::MIN, the highest topic id) and the
/// source_id announced in its Hello. Any larger frame is a protocol violation of the engine.
pub fn max_state_frame(max_payload: usize, source_id: &str) -> usize {
    EngineFrame {
        msg: Some(engine_frame::Msg::State(crate::proto::StateFull {
            topic: topics::TOPICS
                .iter()
                .map(|t| t.id as i32)
                .max()
                .unwrap_or(0),
            generation: u64::MAX,
            seq: u64::MAX,
            source_timestamp_unix_ns: i64::MIN,
            quality: StateQuality::Ok as i32,
            source_id: source_id.into(),
            payload: Bytes::from(vec![0u8; max_payload]),
        })),
    }
    .encoded_len()
}
impl UnixSocketFeed {
    pub fn new(project: ProjectId, label: &'static str, path: PathBuf, max: usize) -> Arc<Self> {
        Arc::new(Self {
            project,
            label,
            path,
            max_payload: max,
            max_frame: max.saturating_add(ENVELOPE_RESERVE),
        })
    }
    fn validate(&self, h: &EngineHello) -> Result<HashSet<TopicId>, FeedError> {
        if h.project != self.project as i32
            || h.schema_id != SCHEMA_ID
            || h.source_id.is_empty()
            || h.generation == 0
        {
            return Err(FeedError::Mismatch(
                "engine hello project, identity, generation or schema mismatch".into(),
            ));
        }
        let mut ids = HashSet::new();
        for raw in &h.topics {
            let spec = topics::spec_from_wire(*raw)
                .ok_or_else(|| FeedError::Mismatch("unknown topic".into()))?;
            if spec.project != self.project
                || !spec.available()
                || !spec.is_state()
                || !ids.insert(spec.id)
            {
                return Err(FeedError::Mismatch(
                    "unauthorized, unavailable or duplicate topic".into(),
                ));
            }
        }
        if ids.is_empty() {
            return Err(FeedError::Mismatch("no topics".into()));
        }
        Ok(ids)
    }
}
async fn next(s: &mut UnixStream, max: usize) -> Result<Option<EngineFrame>, String> {
    let mut prefix = [0u8; 4];
    match s.read(&mut prefix[..1]).await {
        Ok(0) => return Ok(None),
        Ok(_) => (),
        Err(e) => return Err(e.to_string()),
    }
    s.read_exact(&mut prefix[1..])
        .await
        .map_err(|e| format!("partial prefix: {e}"))?;
    let n = u32::from_be_bytes(prefix) as usize;
    if n == 0 || n > max {
        return Err(format!("invalid frame size: {n}"));
    }
    let mut buf = vec![0; n];
    s.read_exact(&mut buf)
        .await
        .map_err(|e| format!("partial frame: {e}"))?;
    EngineFrame::decode(&buf[..])
        .map(Some)
        .map_err(|e| e.to_string())
}
#[async_trait]
impl EngineFeed for UnixSocketFeed {
    fn project(&self) -> ProjectId {
        self.project
    }
    fn label(&self) -> &'static str {
        self.label
    }
    async fn attach(&self) -> Result<mpsc::Receiver<FeedMessage>, FeedError> {
        let mut stream = UnixStream::connect(&self.path)
            .await
            .map_err(|e| FeedError::Unreachable(e.to_string()))?;
        // Authenticated engine hop: only a publisher running as this user may feed the gateway.
        let peer = stream
            .peer_cred()
            .map_err(|e| FeedError::Unreachable(format!("peer credentials: {e}")))?;
        if !same_user(peer.uid(), current_uid()) {
            return Err(FeedError::Mismatch(format!(
                "engine feed peer uid {} is not the gateway user",
                peer.uid()
            )));
        }
        let first = tokio::time::timeout(Duration::from_secs(5), next(&mut stream, self.max_frame))
            .await
            .map_err(|_| FeedError::Unreachable("hello timeout".into()))?
            .map_err(FeedError::Mismatch)?;
        let hello = match first {
            Some(EngineFrame {
                msg: Some(engine_frame::Msg::Hello(h)),
            }) => h,
            _ => return Err(FeedError::Mismatch("expected first hello".into())),
        };
        let allowed = self.validate(&hello)?;
        // From the Hello on, the exact bound of this source replaces the contract maximum.
        let max = max_state_frame(self.max_payload, &hello.source_id);
        if max > self.max_frame {
            return Err(FeedError::Mismatch(
                "engine source_id too long for the engine frame limit".into(),
            ));
        }
        let source: Arc<str> = Arc::from(hello.source_id);
        let generation = hello.generation;
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            loop {
                match next(&mut stream, max).await {
                    Ok(Some(EngineFrame {
                        msg: Some(engine_frame::Msg::State(s)),
                    })) => {
                        let topic = match TopicId::try_from(s.topic) {
                            Ok(v) if allowed.contains(&v) => v,
                            _ => {
                                warn!(topic = s.topic, "engine state topic not announced in hello");
                                break;
                            }
                        };
                        if s.source_id != source.as_ref()
                            || s.generation != generation
                            || s.seq == 0
                            || s.quality != StateQuality::Ok as i32
                        {
                            warn!(topic=s.topic, received_source=%s.source_id, expected_source=%source, received_generation=s.generation, expected_generation=generation, seq=s.seq, quality=s.quality, "engine state rejected: envelope mismatch");
                            break;
                        }
                        let u = StateUpdate {
                            topic,
                            generation,
                            seq: s.seq,
                            source_timestamp_unix_ns: s.source_timestamp_unix_ns,
                            source_id: source.clone(),
                            payload: s.payload,
                        };
                        if tx.send(FeedMessage::State(u)).await.is_err() {
                            break;
                        }
                    }
                    Ok(Some(EngineFrame {
                        msg: Some(engine_frame::Msg::StateTooLarge(s)),
                    })) => {
                        // Same envelope rules as a StateFull; the payload was never sent.
                        let topic = match TopicId::try_from(s.topic) {
                            Ok(v) if allowed.contains(&v) => v,
                            _ => {
                                warn!(topic = s.topic, "engine state topic not announced in hello");
                                break;
                            }
                        };
                        if s.source_id != source.as_ref()
                            || s.generation != generation
                            || s.seq == 0
                            || s.payload_bytes == 0
                        {
                            warn!(
                                topic = s.topic,
                                seq = s.seq,
                                "engine too-large notice rejected: envelope mismatch"
                            );
                            break;
                        }
                        let update = StateUpdate {
                            topic,
                            generation,
                            seq: s.seq,
                            source_timestamp_unix_ns: s.source_timestamp_unix_ns,
                            source_id: source.clone(),
                            payload: Bytes::new(),
                        };
                        let m = FeedMessage::StateTooLarge {
                            update,
                            bytes: s.payload_bytes,
                        };
                        if tx.send(m).await.is_err() {
                            break;
                        }
                    }
                    Ok(Some(EngineFrame {
                        msg: Some(engine_frame::Msg::Heartbeat(_)),
                    })) => {
                        if tx.send(FeedMessage::Heartbeat).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Ok(_) => {
                        warn!("invalid engine IPC message");
                        break;
                    }
                    Err(e) => {
                        warn!(error=%e,"engine feed disconnected");
                        break;
                    }
                }
            }
        });
        Ok(rx)
    }
}

fn current_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

fn same_user(peer: u32, own: u32) -> bool {
    peer == own
}

#[cfg(test)]
mod peer_tests {
    use super::{current_uid, same_user};
    #[test]
    fn only_the_gateway_user_may_feed() {
        assert!(same_user(current_uid(), current_uid()));
        assert!(!same_user(current_uid().wrapping_add(1), current_uid()));
    }
}

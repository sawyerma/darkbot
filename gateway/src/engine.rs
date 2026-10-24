//! Engine-Feed-Interface und Source-Supervisor (Gateway↔Engine-Hop, Wire V1.3 §2/§5).
//! Das Gateway kennt von einer Engine nur: typisierte STATE-Projektionen und Liveness.
//! Die konkreten P0-/BSC-Feeds werden separat gegen die Backend-Verträge implementiert.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::RwLock;
use thiserror::Error;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::config::GatewayConfig;
use crate::hub::{PublishError, PublishOutcome, StateHub, StateUpdate};
use crate::proto::ProjectId;
use crate::topics;

#[derive(Debug)]
pub enum FeedMessage {
    State(StateUpdate),
    /// A state the engine could not send within the frame limit (`StateTooLarge`); empty payload.
    StateTooLarge {
        update: StateUpdate,
        bytes: u64,
    },
    /// A frame beyond the absolute IPC bound of the source: a protocol error of the engine, never a
    /// topic error; the source is detached and reattached with backoff.
    ProtocolViolation(String),
    /// Liveness ohne Zustandsänderung.
    Heartbeat,
}

#[derive(Debug, Error)]
pub enum FeedError {
    #[error("engine not reachable: {0}")]
    Unreachable(String),
    #[error("engine identity/schema mismatch: {0}")]
    Mismatch(String),
}

/// Ein Feed liefert pro `attach()` einen Stream von Nachrichten, bis die Engine die
/// Verbindung beendet (Receiver schließt). Der Supervisor hängt sich dann erneut an.
#[async_trait]
pub trait EngineFeed: Send + Sync + 'static {
    fn project(&self) -> ProjectId;
    fn label(&self) -> &'static str;
    async fn attach(&self) -> Result<mpsc::Receiver<FeedMessage>, FeedError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceState {
    NotConnected,
    Connected,
    Stale,
    Detached,
}

#[derive(Debug, Clone)]
pub struct SourceStatus {
    pub state: SourceState,
    pub label: &'static str,
    pub source_id: Option<Arc<str>>,
    pub last_message_at: Option<Instant>,
    pub attach_count: u32,
    pub detach_count: u32,
    /// Feeds ended by an engine protocol violation (e.g. a frame beyond the IPC bound).
    pub protocol_violations: u32,
}

impl SourceStatus {
    fn new(label: &'static str) -> Self {
        Self {
            state: SourceState::NotConnected,
            label,
            source_id: None,
            last_message_at: None,
            attach_count: 0,
            detach_count: 0,
            protocol_violations: 0,
        }
    }
}

/// Zustand aller Quellen; Änderung bumpt `version`, der Overview-Publisher reagiert darauf.
pub struct SourceRegistry {
    inner: RwLock<HashMap<ProjectId, SourceStatus>>,
    version: watch::Sender<u64>,
}

impl SourceRegistry {
    pub fn new() -> Arc<Self> {
        let mut inner = HashMap::new();
        inner.insert(ProjectId::Project0, SourceStatus::new("Project 0"));
        inner.insert(ProjectId::Bsc, SourceStatus::new("BSC"));
        inner.insert(ProjectId::Solana, SourceStatus::new("Solana"));
        let (version, _) = watch::channel(0);
        Arc::new(Self {
            inner: RwLock::new(inner),
            version,
        })
    }

    pub fn snapshot(&self) -> Vec<(ProjectId, SourceStatus)> {
        let g = self.inner.read();
        let mut v: Vec<_> = g.iter().map(|(k, s)| (*k, s.clone())).collect();
        v.sort_by_key(|(k, _)| *k as i32);
        v
    }

    pub fn status(&self, project: ProjectId) -> Option<SourceStatus> {
        self.inner.read().get(&project).cloned()
    }

    pub fn watch(&self) -> watch::Receiver<u64> {
        self.version.subscribe()
    }

    fn update(&self, project: ProjectId, f: impl FnOnce(&mut SourceStatus) -> bool) {
        let changed = {
            let mut g = self.inner.write();
            match g.get_mut(&project) {
                Some(s) => f(s),
                None => false,
            }
        };
        if changed {
            self.version.send_modify(|v| *v += 1);
        }
    }
}

impl Default for SourceRegistry {
    fn default() -> Self {
        let (version, _) = watch::channel(0);
        Self {
            inner: RwLock::new(HashMap::new()),
            version,
        }
    }
}

/// Reject cross-project, gateway-owned and not-yet-registered projections.
fn validate_feed_state(project: ProjectId, u: &StateUpdate) -> Result<(), FeedError> {
    let spec = topics::spec(u.topic).ok_or_else(|| FeedError::Mismatch("unknown topic".into()))?;
    if project == ProjectId::Unspecified
        || spec.project != project
        || !spec.is_state()
        || !spec.available()
        || u.source_id.is_empty()
    {
        return Err(FeedError::Mismatch(format!(
            "feed not authorized for {:?}",
            u.topic
        )));
    }
    Ok(())
}

/// One validated engine state into the hub (`declared`: size of a state the engine reported as
/// too large instead of sending it). A stored state or FRAME_TOO_LARGE marker shows a live,
/// connected source (`Ok(true)`); an older or duplicate one changes nothing (`Ok(false)`); an
/// identity or topic violation detaches the source (`Err`).
fn deliver(
    hub: &StateHub,
    registry: &SourceRegistry,
    project: ProjectId,
    u: StateUpdate,
    declared: Option<u64>,
    at: Instant,
) -> Result<bool, ()> {
    let sid = u.source_id.clone();
    if let Err(e) = validate_feed_state(project, &u) {
        warn!(?project, error = %e, "invalid engine feed identity or topic; detaching");
        return Err(());
    }
    let topic = u.topic;
    let published = match declared {
        Some(bytes) => hub.publish_too_large(u, bytes),
        None => hub.publish(u),
    };
    match published {
        Ok(PublishOutcome::Applied | PublishOutcome::RefreshedTooLarge) => {}
        Err(e @ PublishError::TooLarge { .. }) => warn!(
            ?project,
            ?topic,
            error = %e,
            "FRAME_TOO_LARGE for this topic until its next valid state; source stays attached"
        ),
        Ok(outcome) => {
            tracing::trace!(
                ?project,
                ?outcome,
                "ignored old state; liveness not promoted"
            );
            return Ok(false);
        }
        Err(e) => {
            warn!(?project, error = %e, "state rejected; detaching");
            return Err(());
        }
    }
    registry.update(project, |s| {
        let changed = s.state != SourceState::Connected || s.source_id.as_deref() != Some(&*sid);
        s.state = SourceState::Connected;
        s.source_id = Some(sid.clone());
        s.last_message_at = Some(at);
        changed
    });
    Ok(true)
}

/// Überwacht einen Feed: Attach, Liveness, STALE/DETACHED-Overlay im Hub, Reattach mit Backoff.
pub fn spawn_supervisor(
    feed: Arc<dyn EngineFeed>,
    hub: Arc<StateHub>,
    registry: Arc<SourceRegistry>,
    cfg: &GatewayConfig,
) -> tokio::task::JoinHandle<()> {
    let stale_after = Duration::from_millis(cfg.source_stale_after_ms);
    let detach_after = Duration::from_millis(cfg.source_detach_after_ms);
    let backoff = Duration::from_millis(cfg.source_reattach_backoff_ms);

    tokio::spawn(async move {
        let project = feed.project();
        let mut attach_failures: u32 = 0;
        loop {
            let mut rx = match feed.attach().await {
                Ok(rx) => {
                    attach_failures = 0;
                    rx
                }
                Err(e) => {
                    attach_failures = attach_failures.saturating_add(1);
                    if attach_failures == 1 || attach_failures % 60 == 0 {
                        warn!(?project, attach_failures, error = %e, "engine feed attach failed");
                    } else {
                        tracing::debug!(?project, attach_failures, error = %e, "engine feed attach failed");
                    }
                    registry.update(project, |s| {
                        let next = if s.attach_count == 0 {
                            SourceState::NotConnected
                        } else {
                            SourceState::Detached
                        };
                        let changed = s.state != next;
                        s.state = next;
                        changed
                    });
                    tokio::time::sleep(
                        (backoff * attach_failures.min(5)).min(Duration::from_secs(10)),
                    )
                    .await;
                    continue;
                }
            };

            info!(
                ?project,
                "engine feed attached; waiting for validated state"
            );
            registry.update(project, |s| {
                // IPC attach by itself is not a qualified/connected projection.
                s.attach_count += 1;
                s.last_message_at = Some(Instant::now());
                true
            });

            let mut last_msg = Instant::now();
            let mut stale_marked = false;

            loop {
                match tokio::time::timeout(stale_after, rx.recv()).await {
                    Ok(Some(m @ (FeedMessage::State(_) | FeedMessage::StateTooLarge { .. }))) => {
                        last_msg = Instant::now();
                        let delivered = match m {
                            FeedMessage::StateTooLarge { update, bytes } => {
                                deliver(&hub, &registry, project, update, Some(bytes), last_msg)
                            }
                            FeedMessage::State(u) => {
                                deliver(&hub, &registry, project, u, None, last_msg)
                            }
                            FeedMessage::Heartbeat | FeedMessage::ProtocolViolation(_) => Ok(false),
                        };
                        match delivered {
                            Ok(true) => stale_marked = false,
                            Ok(false) => {}
                            Err(()) => break,
                        }
                    }
                    Ok(Some(FeedMessage::ProtocolViolation(why))) => {
                        tracing::error!(?project, %why, "engine IPC protocol violation; detaching");
                        registry.update(project, |s| {
                            s.protocol_violations += 1;
                            true
                        });
                        break;
                    }
                    Ok(Some(FeedMessage::Heartbeat)) => {
                        last_msg = Instant::now();
                        // Heartbeats alone never restore STALE / DETACHED cached state.
                        registry.update(project, |s| {
                            s.last_message_at = Some(last_msg);
                            false
                        });
                    }
                    Ok(None) => {
                        warn!(?project, "engine feed closed");
                        break;
                    }
                    Err(_elapsed) => {
                        let silence = last_msg.elapsed();
                        if silence >= detach_after {
                            warn!(
                                ?project,
                                ?silence,
                                "engine feed silent beyond detach threshold"
                            );
                            break;
                        }
                        if !stale_marked {
                            stale_marked = true;
                            hub.mark_stale(project);
                            registry.update(project, |s| {
                                // Never-connected source stays NOT_CONNECTED.
                                if s.state == SourceState::Connected {
                                    s.state = SourceState::Stale;
                                    true
                                } else {
                                    false
                                }
                            });
                        }
                    }
                }
            }

            // DETACHED: bestehende Subscriber bekommen das Overlay sofort, neue sehen nie OK.
            let n = hub.mark_detached(project);
            registry.update(project, |s| {
                s.state = SourceState::Detached;
                s.detach_count += 1;
                true
            });
            info!(?project, overlaid_topics = n, "engine source detached");
            tokio::time::sleep(backoff).await;
        }
    })
}

/// In-Prozess-Feed ausschließlich für Tests (Feature `test-feed` oder cfg(test)).
/// Liefert keine fachlichen Daten; Tests schieben TEST_ONLY-Bytes hinein.
#[cfg(any(test, feature = "test-feed"))]
pub mod test_feed {
    use super::*;
    use tokio::sync::Mutex;

    pub struct ChannelFeed {
        project: ProjectId,
        label: &'static str,
        pending: Mutex<Option<mpsc::Receiver<FeedMessage>>>,
        handed: watch::Sender<u32>,
    }

    impl ChannelFeed {
        pub fn new(project: ProjectId, label: &'static str) -> Arc<Self> {
            let (handed, _) = watch::channel(0);
            Arc::new(Self {
                project,
                label,
                pending: Mutex::new(None),
                handed,
            })
        }

        /// Stellt eine neue "Engine-Verbindung" bereit; das zurückgegebene Sender-Ende
        /// simuliert die Engine. Drop des Senders = Engine-Kill.
        pub async fn connect(&self, capacity: usize) -> mpsc::Sender<FeedMessage> {
            let (tx, rx) = mpsc::channel(capacity);
            *self.pending.lock().await = Some(rx);
            tx
        }

        pub fn attach_count(&self) -> watch::Receiver<u32> {
            self.handed.subscribe()
        }
    }

    #[async_trait]
    impl EngineFeed for ChannelFeed {
        fn project(&self) -> ProjectId {
            self.project
        }
        fn label(&self) -> &'static str {
            self.label
        }
        async fn attach(&self) -> Result<mpsc::Receiver<FeedMessage>, FeedError> {
            loop {
                if let Some(rx) = self.pending.lock().await.take() {
                    self.handed.send_modify(|n| *n += 1);
                    return Ok(rx);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }
}

#[cfg(test)]
mod trust_boundary_tests {
    use super::*;
    use crate::proto::TopicId;
    use bytes::Bytes;

    #[test]
    fn engine_feeds_cannot_spoof_gateway_or_cross_project_topics() {
        let state = |topic| StateUpdate {
            topic,
            generation: 1,
            seq: 1,
            source_timestamp_unix_ns: 1,
            source_id: Arc::from("feed"),
            payload: Bytes::from_static(b"payload"),
        };
        assert!(validate_feed_state(ProjectId::Bsc, &state(TopicId::OverviewAggregates)).is_err());
        assert!(validate_feed_state(ProjectId::Bsc, &state(TopicId::Project0Status)).is_err());
        assert!(validate_feed_state(ProjectId::Bsc, &state(TopicId::BscStatus)).is_ok());
    }
}

#[cfg(test)]
mod liveness_tests {
    use super::test_feed::ChannelFeed;
    use super::*;
    use crate::config::Limits;
    use crate::proto::{StateQuality, TopicId};
    use bytes::Bytes;

    fn cfg() -> GatewayConfig {
        GatewayConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            static_dir: None,
            allowed_origins: vec![],
            allowed_hosts: vec![],
            session_cookie: "s".into(),
            limits: Limits::default(),
            source_stale_after_ms: 100,
            source_detach_after_ms: 400,
            source_reattach_backoff_ms: 50,
            overview_interval_ms: 60_000,
            feeds: vec![],
        }
    }
    fn state(seq: u64, bytes: usize) -> FeedMessage {
        FeedMessage::State(StateUpdate {
            topic: TopicId::BscStatus,
            generation: 1,
            seq,
            source_timestamp_unix_ns: 1,
            source_id: Arc::from("TEST_ONLY"),
            payload: Bytes::from(vec![8u8; bytes]),
        })
    }
    async fn until(what: &str, f: impl Fn() -> bool) {
        for _ in 0..200 {
            if f() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("timeout: {what}");
    }

    /// A real engine socket: the reporter declares one topic too large instead of sending it.
    #[tokio::test]
    async fn a_too_large_notice_marks_only_its_topic_and_the_next_state_restores_it() {
        use crate::feeds::unix_socket::UnixSocketFeed;
        use crate::proto::{engine_frame, EngineFrame, EngineHello, StateFull, StateTooLarge};
        use prost::Message;
        use tokio::io::AsyncWriteExt;
        let dir = std::env::temp_dir().join(format!("gw-too-large-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("engine.sock");
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();
        let (hub, registry) = (Arc::new(StateHub::new(1024)), SourceRegistry::new());
        let feed = UnixSocketFeed::new(ProjectId::Bsc, "bsc", sock.clone(), 1024);
        let slow = GatewayConfig {
            source_stale_after_ms: 10_000,
            source_detach_after_ms: 20_000,
            ..cfg()
        };
        let _sup = spawn_supervisor(feed, hub.clone(), registry.clone(), &slow);
        let (mut engine, _) = listener.accept().await.unwrap();
        let frame = |msg| {
            let b = EngineFrame { msg: Some(msg) }.encode_to_vec();
            [(b.len() as u32).to_be_bytes().to_vec(), b].concat()
        };
        let full = |topic: TopicId, seq| {
            engine_frame::Msg::State(StateFull {
                topic: topic as i32,
                generation: 7,
                seq,
                source_timestamp_unix_ns: 1,
                quality: StateQuality::Ok as i32,
                source_id: "TEST_ONLY".into(),
                payload: Bytes::from_static(&[8u8; 4]),
            })
        };
        let too_large = |seq| {
            engine_frame::Msg::StateTooLarge(StateTooLarge {
                topic: TopicId::BscLaunchState as i32,
                generation: 7,
                seq,
                source_timestamp_unix_ns: 1,
                source_id: "TEST_ONLY".into(),
                payload_bytes: 300_000,
            })
        };
        let hello = engine_frame::Msg::Hello(EngineHello {
            project: ProjectId::Bsc as i32,
            schema_id: crate::SCHEMA_ID.into(),
            source_id: "TEST_ONLY".into(),
            generation: 7,
            topics: vec![TopicId::BscStatus as i32, TopicId::BscLaunchState as i32],
        });
        for m in [hello, full(TopicId::BscStatus, 1), too_large(1)] {
            engine.write_all(&frame(m)).await.unwrap();
        }
        let launch = || hub.current(TopicId::BscLaunchState);
        until("marker", || {
            launch().is_some_and(|s| s.too_large && s.seq == 1)
        })
        .await;
        assert_eq!(launch().unwrap().quality, StateQuality::Unknown);
        assert!(launch().unwrap().payload.is_empty());
        assert_eq!(
            hub.current(TopicId::BscStatus).unwrap().quality,
            StateQuality::Ok
        );
        let status = registry.status(ProjectId::Bsc).unwrap();
        assert_eq!(
            (status.state, status.attach_count),
            (SourceState::Connected, 1)
        );
        // Still too large at the next seq: identity follows, subscribers are not notified again.
        let mut rx = hub.subscribe(TopicId::BscLaunchState).unwrap();
        rx.borrow_and_update();
        engine.write_all(&frame(too_large(2))).await.unwrap();
        until("refreshed marker", || launch().is_some_and(|s| s.seq == 2)).await;
        assert!(!rx.has_changed().unwrap());
        engine
            .write_all(&frame(full(TopicId::BscLaunchState, 3)))
            .await
            .unwrap();
        until("restored", || {
            launch().is_some_and(|s| !s.too_large && s.seq == 3)
        })
        .await;
        assert!(rx.has_changed().unwrap());
        let status = registry.status(ProjectId::Bsc).unwrap();
        assert_eq!(status.attach_count, 1);
        assert_eq!(
            status.protocol_violations, 0,
            "a too-large STATE is a topic error only"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The exact engine frame bound of one source: the largest valid frame passes, one byte more
    /// is a protocol violation of the engine, an over-long source_id is refused at the Hello.
    #[tokio::test]
    async fn the_exact_engine_frame_bound_admits_the_largest_frame_and_refuses_one_byte_more() {
        use crate::feeds::unix_socket::{max_state_frame, UnixSocketFeed, ENVELOPE_RESERVE};
        use crate::proto::{engine_frame, EngineFrame, EngineHello, StateFull};
        use prost::Message;
        use tokio::io::AsyncWriteExt;
        // The deployed BSC reporter at the real limit stays inside the contract maximum.
        let bsc = max_state_frame(262_144, "bsc-status-reporter");
        assert!(bsc <= 262_144 + ENVELOPE_RESERVE, "{bsc}");
        let dir = std::env::temp_dir().join(format!("gw-frame-bound-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("engine.sock");
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();
        let frame = |msg| {
            let b = EngineFrame { msg: Some(msg) }.encode_to_vec();
            [(b.len() as u32).to_be_bytes().to_vec(), b].concat()
        };
        let hello = |source: &str| {
            engine_frame::Msg::Hello(EngineHello {
                project: ProjectId::Bsc as i32,
                schema_id: crate::SCHEMA_ID.into(),
                source_id: source.into(),
                generation: u64::MAX,
                topics: vec![TopicId::BscStatus as i32],
            })
        };
        let largest = |payload: usize| {
            engine_frame::Msg::State(StateFull {
                topic: TopicId::BscStatus as i32,
                generation: u64::MAX,
                seq: u64::MAX,
                source_timestamp_unix_ns: i64::MIN,
                quality: StateQuality::Ok as i32,
                source_id: "TEST_ONLY".into(),
                payload: Bytes::from(vec![8u8; payload]),
            })
        };
        // An over-long source_id would need a frame beyond the contract maximum: refused.
        let feed = UnixSocketFeed::new(ProjectId::Bsc, "bsc", sock.clone(), 1024);
        let attach = tokio::spawn({
            let feed = feed.clone();
            async move { feed.attach().await.map(|_| ()) }
        });
        let (mut engine, _) = listener.accept().await.unwrap();
        engine
            .write_all(&frame(hello(&"s".repeat(5_000))))
            .await
            .unwrap();
        assert!(matches!(attach.await.unwrap(), Err(FeedError::Mismatch(_))));
        drop(engine);
        let (hub, registry) = (Arc::new(StateHub::new(1024)), SourceRegistry::new());
        let slow = GatewayConfig {
            source_stale_after_ms: 10_000,
            source_detach_after_ms: 20_000,
            ..cfg()
        };
        let _sup = spawn_supervisor(feed, hub.clone(), registry.clone(), &slow);
        let (mut engine, _) = listener.accept().await.unwrap();
        let exact = max_state_frame(1024, "TEST_ONLY");
        assert_eq!(
            EngineFrame {
                msg: Some(largest(1024))
            }
            .encoded_len(),
            exact
        );
        for m in [hello("TEST_ONLY"), largest(1024)] {
            engine.write_all(&frame(m)).await.unwrap();
        }
        let q = || {
            hub.current(TopicId::BscStatus)
                .map(|s| (s.quality, s.too_large))
        };
        until("largest frame applied", || {
            q() == Some((StateQuality::Ok, false))
        })
        .await;
        assert_eq!(
            registry.status(ProjectId::Bsc).unwrap().protocol_violations,
            0
        );
        engine.write_all(&frame(largest(1025))).await.unwrap();
        until("one byte more detaches", || {
            q() == Some((StateQuality::SourceDetached, false))
        })
        .await;
        // Classified as an engine protocol violation, not as a too-large topic.
        assert_eq!(
            registry.status(ProjectId::Bsc).unwrap().protocol_violations,
            1
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A topic marked too large stays without value through a source detach and recovers with
    /// the first valid state of the reattached publisher.
    #[tokio::test]
    async fn a_too_large_topic_is_never_ok_across_detach_and_recovers_after_reattach() {
        let (hub, registry) = (Arc::new(StateHub::new(1024)), SourceRegistry::new());
        let feed = ChannelFeed::new(ProjectId::Bsc, "bsc");
        let tx = feed.connect(16).await;
        let _sup = spawn_supervisor(feed.clone(), hub.clone(), registry.clone(), &cfg());
        tx.send(state(1, 2_000)).await.unwrap();
        let cur = || hub.current(TopicId::BscStatus);
        until("marker", || cur().is_some_and(|s| s.too_large)).await;
        drop(tx); // engine killed
        until("detached marker", || {
            cur().is_some_and(|s| s.quality == StateQuality::SourceDetached)
        })
        .await;
        let s = cur().unwrap();
        assert!(s.too_large && s.payload.is_empty());
        let tx = feed.connect(16).await;
        tx.send(FeedMessage::State(StateUpdate {
            generation: 2,
            ..match state(1, 4) {
                FeedMessage::State(u) => u,
                _ => unreachable!(),
            }
        }))
        .await
        .unwrap();
        until("recovered", || {
            cur()
                .is_some_and(|s| !s.too_large && s.quality == StateQuality::Ok && s.generation == 2)
        })
        .await;
    }

    #[tokio::test]
    async fn silence_turns_stale_then_detached_and_heartbeats_alone_restore_nothing() {
        let (hub, registry) = (Arc::new(StateHub::new(1024)), SourceRegistry::new());
        let feed = ChannelFeed::new(ProjectId::Bsc, "bsc");
        let tx = feed.connect(16).await;
        let _sup = spawn_supervisor(feed.clone(), hub.clone(), registry.clone(), &cfg());
        tx.send(state(1, 4)).await.unwrap();
        let q = || hub.current(TopicId::BscStatus).map(|s| s.quality);
        until("OK", || q() == Some(StateQuality::Ok)).await;
        until("STALE after silence", || q() == Some(StateQuality::Stale)).await;
        assert_eq!(
            registry.status(ProjectId::Bsc).unwrap().state,
            SourceState::Stale
        );
        // A heartbeat keeps the feed alive but never turns STALE back into OK.
        tx.send(FeedMessage::Heartbeat).await.unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(q(), Some(StateQuality::Stale));
        until("DETACHED after the detach threshold", || {
            q() == Some(StateQuality::SourceDetached)
        })
        .await;
    }

    #[tokio::test]
    async fn an_oversized_state_is_a_topic_error_and_keeps_the_source_attached() {
        let (hub, registry) = (Arc::new(StateHub::new(8)), SourceRegistry::new());
        let feed = ChannelFeed::new(ProjectId::Bsc, "bsc");
        let tx = feed.connect(16).await;
        let attached = feed.attach_count();
        let _sup = spawn_supervisor(feed.clone(), hub.clone(), registry.clone(), &cfg());
        tx.send(state(1, 4)).await.unwrap();
        until("connected", || {
            registry.status(ProjectId::Bsc).map(|s| s.state) == Some(SourceState::Connected)
        })
        .await;
        tx.send(state(2, 64)).await.unwrap();
        until("marker", || {
            hub.current(TopicId::BscStatus).is_some_and(|s| s.too_large)
        })
        .await;
        tx.send(state(3, 4)).await.unwrap();
        until("next state applied on the same attachment", || {
            hub.current(TopicId::BscStatus)
                .is_some_and(|s| s.seq == 3 && s.quality == StateQuality::Ok)
        })
        .await;
        assert_eq!(*attached.borrow(), 1, "the source was never detached");
    }
}

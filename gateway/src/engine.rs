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
use crate::hub::{PublishOutcome, StateHub, StateUpdate};
use crate::proto::ProjectId;
use crate::topics;

#[derive(Debug)]
pub enum FeedMessage {
    State(StateUpdate),
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
                    Ok(Some(FeedMessage::State(u))) => {
                        last_msg = Instant::now();
                        let sid = u.source_id.clone();
                        if let Err(e) = validate_feed_state(project, &u) {
                            warn!(?project, error = %e, "invalid engine feed identity or topic; detaching");
                            break;
                        }
                        match hub.publish(u) {
                            Ok(PublishOutcome::Applied) => {
                                registry.update(project, |s| {
                                    let changed = s.state != SourceState::Connected
                                        || s.source_id.as_deref() != Some(&*sid);
                                    s.state = SourceState::Connected;
                                    s.source_id = Some(sid.clone());
                                    s.last_message_at = Some(last_msg);
                                    changed
                                });
                                stale_marked = false;
                            }
                            Ok(outcome) => tracing::trace!(
                                ?project,
                                ?outcome,
                                "ignored old state; liveness not promoted"
                            ),
                            Err(e) => {
                                warn!(?project, error = %e, "state rejected; detaching");
                                break;
                            }
                        }
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

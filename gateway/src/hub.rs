//! STATE-Hub: genau ein Latest-Value-Cache pro STATE-Topic (watch::Sender) und pro
//! Client-Subscription ein watch::Receiver (Latest-Slot). Kein Ring, kein Delta, keine Queue.

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use thiserror::Error;
use tokio::sync::watch;

use crate::proto::{ProjectId, StateFull, StateQuality, TopicId};
use crate::topics::{self, TopicSpec};

#[derive(Debug, Clone)]
pub struct CachedState {
    pub topic: TopicId,
    pub generation: u64,
    pub seq: u64,
    pub quality: StateQuality,
    pub source_timestamp_unix_ns: i64,
    pub source_id: Arc<str>,
    pub payload: Bytes,
    /// The newest state exceeded the frame limit: subscribers get FRAME_TOO_LARGE for this
    /// topic instead of an older value; the payload is not kept.
    pub too_large: bool,
}

impl CachedState {
    pub fn to_frame(&self) -> StateFull {
        StateFull {
            topic: self.topic as i32,
            generation: self.generation,
            seq: self.seq,
            source_timestamp_unix_ns: self.source_timestamp_unix_ns,
            quality: self.quality as i32,
            source_id: self.source_id.to_string(),
            payload: self.payload.clone(),
        }
    }
}

pub type Slot = Option<Arc<CachedState>>;

#[derive(Debug, Clone)]
pub struct StateUpdate {
    pub topic: TopicId,
    pub generation: u64,
    pub seq: u64,
    pub source_timestamp_unix_ns: i64,
    pub source_id: Arc<str>,
    pub payload: Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    Applied,
    /// A newer seq of a topic that is still too large: the marker takes the new identity
    /// without notifying subscribers again (no repeated FRAME_TOO_LARGE per publish).
    RefreshedTooLarge,
    IgnoredOlderGeneration,
    IgnoredOlderSeq,
    IgnoredDuplicate,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PublishError {
    #[error("unknown topic")]
    UnknownTopic,
    #[error("topic {0:?} is not a STATE topic")]
    NotState(TopicId),
    #[error("topic {0:?} has no payload binding yet (F0-VAR)")]
    Unavailable(TopicId),
    #[error("payload {bytes} bytes exceeds limit {max}")]
    TooLarge { bytes: usize, max: usize },
}

pub struct StateHub {
    slots: HashMap<TopicId, watch::Sender<Slot>>,
    max_state_bytes: usize,
}

impl StateHub {
    pub fn new(max_state_bytes: usize) -> Self {
        let slots = topics::state_topics()
            .map(|t| {
                let (tx, _rx) = watch::channel::<Slot>(None);
                (t.id, tx)
            })
            .collect();
        Self {
            slots,
            max_state_bytes,
        }
    }

    pub fn spec(topic: TopicId) -> Option<&'static TopicSpec> {
        topics::spec(topic)
    }

    /// Publisher-Ordnung (Wire V1.3 §2): neue Generation gewinnt, gleiche Generation nur bei
    /// höherer seq, ältere Generation wird verworfen. Größenlimit hart: ein zu großer neuer
    /// Zustand ersetzt den Cache durch eine Markierung ohne Payload (Topic-Fehler statt alter
    /// Wert), die Quelle bleibt verbunden; Rückgabe `TooLarge`.
    pub fn publish(&self, u: StateUpdate) -> Result<PublishOutcome, PublishError> {
        self.store(u, None)
    }

    /// Engine-declared oversized state (`StateTooLarge`): exactly the rule above for a payload
    /// of `bytes` that was never sent; one implementation for every engine.
    pub fn publish_too_large(
        &self,
        u: StateUpdate,
        bytes: u64,
    ) -> Result<PublishOutcome, PublishError> {
        self.store(u, Some(bytes))
    }

    fn store(&self, u: StateUpdate, declared: Option<u64>) -> Result<PublishOutcome, PublishError> {
        let spec = topics::spec(u.topic).ok_or(PublishError::UnknownTopic)?;
        if !spec.is_state() {
            return Err(PublishError::NotState(u.topic));
        }
        if !spec.available() {
            return Err(PublishError::Unavailable(u.topic));
        }
        let too_large = declared.is_some() || u.payload.len() > self.max_state_bytes;
        let bytes = declared.map_or(u.payload.len(), |b| {
            usize::try_from(b).unwrap_or(usize::MAX)
        });
        let tx = self.slots.get(&u.topic).ok_or(PublishError::UnknownTopic)?;

        let mut outcome = PublishOutcome::Applied;
        tx.send_if_modified(|slot| {
            if let Some(cur) = slot.as_ref() {
                if u.generation < cur.generation {
                    outcome = PublishOutcome::IgnoredOlderGeneration;
                    return false;
                }
                if u.generation == cur.generation {
                    if u.seq < cur.seq {
                        outcome = PublishOutcome::IgnoredOlderSeq;
                        return false;
                    }
                    if u.seq == cur.seq {
                        outcome = PublishOutcome::IgnoredDuplicate;
                        return false;
                    }
                }
            }
            let refresh = too_large
                && slot
                    .as_ref()
                    .is_some_and(|cur| cur.too_large && cur.generation == u.generation);
            *slot = Some(Arc::new(CachedState {
                topic: u.topic,
                generation: u.generation,
                seq: u.seq,
                quality: if too_large {
                    StateQuality::Unknown
                } else {
                    StateQuality::Ok
                },
                source_timestamp_unix_ns: u.source_timestamp_unix_ns,
                source_id: u.source_id.clone(),
                payload: if too_large {
                    Bytes::new()
                } else {
                    u.payload.clone()
                },
                too_large,
            }));
            if refresh {
                outcome = PublishOutcome::RefreshedTooLarge;
            }
            !refresh
        });
        // Only a newly stored marker is a topic error; an older, duplicate or refreshed one is not.
        if too_large && outcome == PublishOutcome::Applied {
            return Err(PublishError::TooLarge {
                bytes,
                max: self.max_state_bytes,
            });
        }
        Ok(outcome)
    }

    /// Quality-Overlay mit unveränderter fachlicher (generation, seq). Wird an bestehende
    /// Subscriber gepusht; neue Subscriber sehen den Wert nie als OK (Wire V1.3 §2).
    pub fn overlay_quality(&self, project: ProjectId, quality: StateQuality) -> usize {
        let mut changed = 0;
        for t in topics::topics_of(project).filter(|t| t.is_state()) {
            if let Some(tx) = self.slots.get(&t.id) {
                tx.send_if_modified(|slot| match slot.as_ref() {
                    Some(cur) if cur.quality != quality => {
                        let mut next = (**cur).clone();
                        next.quality = quality;
                        *slot = Some(Arc::new(next));
                        changed += 1;
                        true
                    }
                    _ => false,
                });
            }
        }
        changed
    }

    pub fn mark_detached(&self, project: ProjectId) -> usize {
        self.overlay_quality(project, StateQuality::SourceDetached)
    }

    pub fn mark_stale(&self, project: ProjectId) -> usize {
        self.overlay_quality(project, StateQuality::Stale)
    }

    pub fn subscribe(&self, topic: TopicId) -> Option<watch::Receiver<Slot>> {
        self.slots.get(&topic).map(watch::Sender::subscribe)
    }

    pub fn current(&self, topic: TopicId) -> Slot {
        self.slots.get(&topic).and_then(|tx| tx.borrow().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upd(topic: TopicId, g: u64, s: u64) -> StateUpdate {
        StateUpdate {
            topic,
            generation: g,
            seq: s,
            source_timestamp_unix_ns: 1,
            source_id: Arc::from("test"),
            payload: Bytes::from_static(b"\x08\x01"),
        }
    }

    #[test]
    fn ordering_rules() {
        let hub = StateHub::new(1024);
        let t = TopicId::OverviewAggregates;
        assert_eq!(hub.publish(upd(t, 1, 5)).unwrap(), PublishOutcome::Applied);
        assert_eq!(
            hub.publish(upd(t, 1, 4)).unwrap(),
            PublishOutcome::IgnoredOlderSeq
        );
        assert_eq!(
            hub.publish(upd(t, 1, 5)).unwrap(),
            PublishOutcome::IgnoredDuplicate
        );
        assert_eq!(hub.publish(upd(t, 1, 9)).unwrap(), PublishOutcome::Applied);
        assert_eq!(
            hub.publish(upd(t, 0, 99)).unwrap(),
            PublishOutcome::IgnoredOlderGeneration
        );
        assert_eq!(hub.publish(upd(t, 2, 1)).unwrap(), PublishOutcome::Applied);
        let cur = hub.current(t).unwrap();
        assert_eq!((cur.generation, cur.seq), (2, 1));
    }

    #[test]
    fn detached_overlay_keeps_generation_and_seq_and_is_seen_by_new_subscriber() {
        let hub = StateHub::new(1024);
        let t = TopicId::OverviewAggregates;
        hub.publish(upd(t, 3, 7)).unwrap();
        assert_eq!(hub.mark_detached(ProjectId::Unspecified), 1);
        let rx = hub.subscribe(t).unwrap();
        let cur = rx.borrow().clone().unwrap();
        assert_eq!(cur.quality, StateQuality::SourceDetached);
        assert_eq!((cur.generation, cur.seq), (3, 7));
        // erneute Publikation derselben seq wird ignoriert; neue seq setzt Quality auf OK
        assert_eq!(
            hub.publish(upd(t, 3, 7)).unwrap(),
            PublishOutcome::IgnoredDuplicate
        );
        assert_eq!(hub.publish(upd(t, 3, 8)).unwrap(), PublishOutcome::Applied);
        assert_eq!(hub.current(t).unwrap().quality, StateQuality::Ok);
    }

    #[test]
    fn rejects_unavailable_and_oversized() {
        let hub = StateHub::new(4);
        assert!(matches!(
            hub.publish(upd(TopicId::SolanaStatus, 1, 1)),
            Err(PublishError::Unavailable(_))
        ));
        let mut big = upd(TopicId::OverviewAggregates, 1, 1);
        big.payload = Bytes::from(vec![0u8; 5]);
        assert!(matches!(
            hub.publish(big),
            Err(PublishError::TooLarge { .. })
        ));
        // The oversized newest state replaces the value by a payload-free marker, never OK.
        let cur = hub.current(TopicId::OverviewAggregates).unwrap();
        assert!(cur.too_large && cur.payload.is_empty());
        assert_eq!(cur.quality, StateQuality::Unknown);
        // A later state that fits replaces the marker.
        let mut small = upd(TopicId::OverviewAggregates, 1, 2);
        small.payload = Bytes::from_static(b"ok");
        assert_eq!(hub.publish(small).unwrap(), PublishOutcome::Applied);
        assert!(!hub.current(TopicId::OverviewAggregates).unwrap().too_large);
        // An older oversized update changes nothing and is no topic error.
        let mut stale = upd(TopicId::OverviewAggregates, 1, 1);
        stale.payload = Bytes::from(vec![0u8; 5]);
        assert_eq!(hub.publish(stale).unwrap(), PublishOutcome::IgnoredOlderSeq);
        assert_eq!(
            hub.current(TopicId::OverviewAggregates).unwrap().payload,
            &b"ok"[..]
        );
    }
}

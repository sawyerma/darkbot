//! Bounded Channel mit explizitem Overflow (S0-05).
//!
//! Regeln (PHASENPLAN §S1.5): bounded, kein stiller Overwrite, Overflow/Gap
//! explizit gezählt, keine Serialisierung zwischen Modulen.
//!
//! Implementiert über `std::sync::mpsc::sync_channel`. Ein lock-freier SPSC-Ring
//! wird erst gebaut, wenn eine Messung (Ledger) zeigt, dass dieser Channel im
//! Hotpath ein Bottleneck ist — nicht vorher.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
pub enum ChannelError<T> {
    /// Kapazität erschöpft; das Element wird **zurückgegeben**, nie verworfen.
    Full(T),
    /// Gegenseite existiert nicht mehr.
    Disconnected(T),
}

#[derive(Debug, Default)]
struct Counters {
    sent: AtomicU64,
    overflow: AtomicU64,
    dropped: AtomicU64,
    received: AtomicU64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelStats {
    pub name: &'static str,
    pub capacity: usize,
    pub sent: u64,
    /// Failed push attempts (`Full`); an element retried n times counts n times.
    pub overflow: u64,
    /// L3: elements the producer finally gave up after `Full` (`give_up`), each once.
    pub dropped: u64,
    pub received: u64,
}

/// [FIX-34] Clone, damit ein Ingress-Adapter neu gestartet werden kann (Subscription-Rebuild).
#[derive(Clone)]
pub struct Producer<T> {
    tx: SyncSender<T>,
    counters: Arc<Counters>,
    name: &'static str,
    capacity: usize,
}

pub struct Consumer<T> {
    rx: Receiver<T>,
    counters: Arc<Counters>,
    name: &'static str,
    capacity: usize,
}

/// Erzeugt einen bounded Channel. `capacity` muss > 0 sein.
pub fn bounded<T>(name: &'static str, capacity: usize) -> (Producer<T>, Consumer<T>) {
    assert!(capacity > 0, "bounded channel needs capacity > 0");
    let (tx, rx) = mpsc::sync_channel::<T>(capacity);
    let counters = Arc::new(Counters::default());
    (
        Producer {
            tx,
            counters: counters.clone(),
            name,
            capacity,
        },
        Consumer {
            rx,
            counters,
            name,
            capacity,
        },
    )
}

impl<T> Producer<T> {
    /// Nicht-blockierend. Bei voller Queue: Overflow-Zähler +1, Element zurück.
    #[inline]
    pub fn try_push(&self, v: T) -> Result<(), ChannelError<T>> {
        match self.tx.try_send(v) {
            Ok(()) => {
                self.counters.sent.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(TrySendError::Full(v)) => {
                self.counters.overflow.fetch_add(1, Ordering::Relaxed);
                Err(ChannelError::Full(v))
            }
            Err(TrySendError::Disconnected(v)) => Err(ChannelError::Disconnected(v)),
        }
    }

    /// L3: the element rejected by the last `try_push` is abandoned, not retried.
    pub fn give_up(&self) {
        self.counters.dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn stats(&self) -> ChannelStats {
        stats(self.name, self.capacity, &self.counters)
    }
}

impl<T> Consumer<T> {
    #[inline]
    pub fn try_pop(&self) -> Option<T> {
        match self.rx.try_recv() {
            Ok(v) => {
                self.counters.received.fetch_add(1, Ordering::Relaxed);
                Some(v)
            }
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }

    /// Blockierend mit Timeout; `None` bei Timeout oder Disconnect.
    pub fn pop_timeout(&self, timeout: Duration) -> Option<T> {
        match self.rx.recv_timeout(timeout) {
            Ok(v) => {
                self.counters.received.fetch_add(1, Ordering::Relaxed);
                Some(v)
            }
            Err(_) => None,
        }
    }

    /// Like `pop_timeout`, but `Err(())` once every producer is gone and the queue is empty,
    /// so a consumer thread can tell a quiet channel from a closed one.
    // Public API compatibility: Err(()) is the established disconnected sentinel.
    // Changing it would force unrelated consumers to migrate during reporter qualification.
    #[allow(clippy::result_unit_err)]
    pub fn pop_timeout_or_closed(&self, timeout: Duration) -> Result<Option<T>, ()> {
        match self.rx.recv_timeout(timeout) {
            Ok(v) => {
                self.counters.received.fetch_add(1, Ordering::Relaxed);
                Ok(Some(v))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(()),
        }
    }

    pub fn stats(&self) -> ChannelStats {
        stats(self.name, self.capacity, &self.counters)
    }
}

fn stats(name: &'static str, capacity: usize, c: &Counters) -> ChannelStats {
    ChannelStats {
        name,
        capacity,
        sent: c.sent.load(Ordering::Relaxed),
        overflow: c.overflow.load(Ordering::Relaxed),
        dropped: c.dropped.load(Ordering::Relaxed),
        received: c.received.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    /// L3: retried pushes are failed attempts, not lost elements; only `give_up` counts a drop.
    #[test]
    fn l3_failed_attempts_and_dropped_elements_are_separate() {
        let (tx, rx) = bounded::<u32>("l3", 1);
        tx.try_push(1).unwrap();
        let mut v = 2;
        for _ in 0..3 {
            match tx.try_push(v) {
                Err(ChannelError::Full(back)) => v = back,
                _ => panic!("expected Full"),
            }
        }
        let s = tx.stats();
        assert_eq!((s.sent, s.overflow, s.dropped), (1, 3, 0));
        assert_eq!(rx.try_pop(), Some(1));
        tx.try_push(v).unwrap();
        assert!(matches!(tx.try_push(3), Err(ChannelError::Full(3))));
        tx.give_up();
        let s = rx.stats();
        assert_eq!((s.sent, s.overflow, s.dropped, s.received), (2, 4, 1, 1));
    }

    use super::*;

    #[test]
    fn overflow_is_explicit_and_returns_element() {
        let (p, c) = bounded::<u32>("test", 2);
        assert!(p.try_push(1).is_ok());
        assert!(p.try_push(2).is_ok());
        match p.try_push(3) {
            Err(ChannelError::Full(v)) => assert_eq!(v, 3),
            other => panic!("expected Full, got {:?}", other.is_ok()),
        }
        let s = p.stats();
        assert_eq!(s.sent, 2);
        assert_eq!(s.overflow, 1);
        assert_eq!(c.try_pop(), Some(1));
        assert_eq!(c.try_pop(), Some(2));
        assert_eq!(c.try_pop(), None);
        assert_eq!(c.stats().received, 2);
    }

    #[test]
    fn disconnect_is_reported() {
        let (p, c) = bounded::<u32>("test", 1);
        drop(c);
        match p.try_push(7) {
            Err(ChannelError::Disconnected(v)) => assert_eq!(v, 7),
            _ => panic!("expected Disconnected"),
        }
    }

    #[test]
    fn pop_timeout_returns_none_on_empty() {
        let (_p, c) = bounded::<u32>("test", 1);
        assert_eq!(c.pop_timeout(Duration::from_millis(5)), None);
    }
}

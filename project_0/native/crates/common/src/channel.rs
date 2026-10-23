//! Bounded Channel mit explizitem Overflow (S0-05).
//!
//! Regeln (PHASENPLAN §S1.5): bounded, kein stiller Overwrite, Overflow/Gap
//! explizit gezählt, keine Serialisierung zwischen Modulen.
//!
//! Implementiert über `std::sync::mpsc::sync_channel`. Ein lock-freier SPSC-Ring
//! wird erst gebaut, wenn eine Messung (Ledger) zeigt, dass dieser Channel im
//! Hotpath ein Bottleneck ist — nicht vorher.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// A23-01 (P6-MT.4): one doorbell for the bounded queues one consumer serves (the state owner:
/// ingress, worker results, control). A successful push bumps a counter; the condvar is only
/// touched while a consumer sleeps. A consumer takes a `token` before it checks its queues and
/// sleeps only while the counter still equals it, so a push between check and sleep is never
/// lost — no polling interval, no busy spin.
#[derive(Clone, Default)]
pub struct Wakeup {
    inner: Arc<WakeInner>,
}

impl std::fmt::Debug for Wakeup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wakeup")
            .field("notifications", &self.token())
            .finish()
    }
}

#[derive(Default)]
struct WakeInner {
    counter: AtomicU64,
    sleepers: AtomicU32,
    /// Guards `signal_ns`: when the first notification since the sleeper's entry signalled it
    /// (ns since `origin`, 0 = none). Exact for one waiter (the owner).
    lock: Mutex<u64>,
    cv: Condvar,
    origin: std::sync::OnceLock<Instant>,
}

/// Outcome of `Wakeup::wait`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Woken {
    /// A notification newer than the token came before the consumer had to sleep.
    Immediate,
    /// Slept; a notification signalled `latency_ns` before the consumer ran again.
    Signalled {
        latency_ns: u64,
    },
    TimedOut,
}

impl Wakeup {
    fn now_ns(&self) -> u64 {
        let origin = self.inner.origin.get_or_init(Instant::now);
        // At least 1: 0 marks "no signal".
        u64::try_from(origin.elapsed().as_nanos())
            .unwrap_or(u64::MAX)
            .max(1)
    }

    pub fn notify(&self) {
        let i = &self.inner;
        i.counter.fetch_add(1, Ordering::SeqCst);
        if i.sleepers.load(Ordering::SeqCst) > 0 {
            let mut signal = i.lock.lock().unwrap_or_else(|e| e.into_inner());
            if *signal == 0 {
                *signal = self.now_ns();
            }
            drop(signal);
            i.cv.notify_all();
        }
    }

    /// The current notification count; take it before checking the queues.
    pub fn token(&self) -> u64 {
        self.inner.counter.load(Ordering::SeqCst)
    }

    /// Sleeps until a notification newer than `token`, at most `timeout`.
    pub fn wait(&self, token: u64, timeout: Duration) -> Woken {
        let i = &self.inner;
        let deadline = Instant::now() + timeout;
        i.sleepers.fetch_add(1, Ordering::SeqCst);
        let mut signal = i.lock.lock().unwrap_or_else(|e| e.into_inner());
        // A signal stored after an earlier sleeper left is not this sleeper's.
        *signal = 0;
        let mut slept = false;
        let outcome = loop {
            if i.counter.load(Ordering::SeqCst) != token {
                break match (slept, *signal) {
                    (true, at) if at != 0 => Woken::Signalled {
                        latency_ns: self.now_ns().saturating_sub(at),
                    },
                    _ => Woken::Immediate,
                };
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break Woken::TimedOut;
            }
            signal =
                i.cv.wait_timeout(signal, left)
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            slept = true;
        };
        *signal = 0;
        drop(signal);
        i.sleepers.fetch_sub(1, Ordering::SeqCst);
        outcome
    }
}

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

#[derive(Clone, Copy, PartialEq, Eq)]
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
pub struct Producer<T> {
    tx: SyncSender<T>,
    counters: Arc<Counters>,
    name: &'static str,
    capacity: usize,
    /// A23-01: rung after every successful push.
    wake: Option<Wakeup>,
}

// Not derived: a derive would require `T: Clone`; the sender side clones for any `T`.
impl<T> Clone for Producer<T> {
    fn clone(&self) -> Self {
        Producer {
            tx: self.tx.clone(),
            counters: self.counters.clone(),
            name: self.name,
            capacity: self.capacity,
            wake: self.wake.clone(),
        }
    }
}

pub struct Consumer<T> {
    rx: Receiver<T>,
    counters: Arc<Counters>,
    name: &'static str,
    capacity: usize,
    /// A23-08: rung after every pop, so a producer waiting for space resumes at once.
    space: Option<Wakeup>,
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
            wake: None,
        },
        Consumer {
            rx,
            counters,
            name,
            capacity,
            space: None,
        },
    )
}

/// The `Debug` text is a parsed diagnostic format (`channel=ChannelStats { name: …, capacity: …,
/// sent: …, overflow: …, received: … }` in the owner `updates:` line, read by the R126/R127
/// qualification scripts): it stays exactly the derived layout of these five fields. `dropped`
/// is reported separately (`channel_dropped=`).
impl std::fmt::Debug for ChannelStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelStats")
            .field("name", &self.name)
            .field("capacity", &self.capacity)
            .field("sent", &self.sent)
            .field("overflow", &self.overflow)
            .field("received", &self.received)
            .finish()
    }
}

impl<T> Producer<T> {
    /// A23-01: every successful push rings `wake` (the consumer's doorbell).
    pub fn with_wakeup(mut self, wake: &Wakeup) -> Self {
        self.wake = Some(wake.clone());
        self
    }

    /// Nicht-blockierend. Bei voller Queue: Overflow-Zähler +1, Element zurück.
    #[inline]
    pub fn try_push(&self, v: T) -> Result<(), ChannelError<T>> {
        match self.tx.try_send(v) {
            Ok(()) => {
                // `sent` before the doorbell: a consumer that took its token earlier wakes, one
                // that takes it later sees the element as pending (`sent > received`).
                self.counters.sent.fetch_add(1, Ordering::SeqCst);
                if let Some(w) = &self.wake {
                    w.notify();
                }
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
    /// A23-08: every pop rings `space` (the doorbell producers wait on while the queue is full).
    pub fn with_space_wakeup(mut self, space: &Wakeup) -> Self {
        self.space = Some(space.clone());
        self
    }

    fn popped(&self) {
        self.counters.received.fetch_add(1, Ordering::SeqCst);
        if let Some(s) = &self.space {
            s.notify();
        }
    }

    /// Elements pushed and not yet popped (exact once every producer finished its push).
    pub fn pending(&self) -> bool {
        self.counters.sent.load(Ordering::SeqCst) > self.counters.received.load(Ordering::SeqCst)
    }

    #[inline]
    pub fn try_pop(&self) -> Option<T> {
        match self.rx.try_recv() {
            Ok(v) => {
                self.popped();
                Some(v)
            }
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }

    /// Blockierend mit Timeout; `None` bei Timeout oder Disconnect.
    pub fn pop_timeout(&self, timeout: Duration) -> Option<T> {
        match self.rx.recv_timeout(timeout) {
            Ok(v) => {
                self.popped();
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
                self.popped();
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
        // The parsed diagnostic layout is unchanged (R126/R127 qualification regex).
        assert_eq!(
            format!("{s:?}"),
            "ChannelStats { name: \"l3\", capacity: 1, sent: 2, overflow: 4, received: 1 }"
        );
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

    /// A23-01: a consumer that checks after taking its token and sleeps only then never sleeps
    /// through a push — 20k pushes from a racing producer (pausing at most 50 µs), no wait
    /// longer than 500 ms or reaching its timeout; woken sleeps report their signal latency.
    #[test]
    fn a23_01_doorbell_never_loses_a_push_between_check_and_sleep() {
        const N: u32 = 20_000;
        let wake = Wakeup::default();
        let (p, c) = bounded::<u32>("a23", 64);
        let p = p.with_wakeup(&wake);
        let started = std::time::Instant::now();
        let producer = std::thread::spawn(move || {
            for i in 0..N {
                let mut v = i;
                while let Err(ChannelError::Full(back)) = p.try_push(v) {
                    v = back;
                    if started.elapsed() > Duration::from_secs(30) {
                        return;
                    }
                    std::thread::yield_now();
                }
                if i % 64 == 0 {
                    std::thread::sleep(Duration::from_micros(50));
                }
            }
        });
        let (mut next, mut signalled, mut immediate) = (0u32, 0u32, 0u32);
        while next < N {
            let token = wake.token();
            if let Some(v) = c.try_pop() {
                assert_eq!(v, next, "FIFO, nothing lost or duplicated");
                next += 1;
                continue;
            }
            let w = std::time::Instant::now();
            match wake.wait(token, Duration::from_secs(1)) {
                Woken::Signalled { latency_ns } => {
                    assert!(latency_ns < 500_000_000);
                    signalled += 1;
                }
                Woken::Immediate => immediate += 1,
                Woken::TimedOut => panic!("slept through a push at {next}"),
            }
            assert!(
                w.elapsed() < Duration::from_millis(500),
                "late wake at {next}"
            );
        }
        producer.join().unwrap();
        assert!(
            signalled > 0,
            "the consumer slept and was woken ({immediate} immediate)"
        );
        assert!(!c.pending());
    }

    /// A23-01: without a notification the wait ends at its timeout (no early return); a
    /// notification from another thread ends a long wait at once; a notification before the
    /// wait (token stale) returns without sleeping; a consumer's pop rings its space doorbell.
    #[test]
    fn a23_01_doorbell_times_out_wakes_and_rings_space() {
        let wake = Wakeup::default();
        let t = std::time::Instant::now();
        assert_eq!(
            wake.wait(wake.token(), Duration::from_millis(20)),
            Woken::TimedOut
        );
        assert!(t.elapsed() >= Duration::from_millis(20));
        let ringer = wake.clone();
        let token = wake.token();
        let notifier = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            ringer.notify();
        });
        let t = std::time::Instant::now();
        let woken = wake.wait(token, Duration::from_secs(5));
        assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
        assert!(matches!(woken, Woken::Signalled { latency_ns } if latency_ns < 1_000_000_000));
        notifier.join().unwrap();
        let token = wake.token();
        wake.notify();
        let t = std::time::Instant::now();
        assert_eq!(wake.wait(token, Duration::from_secs(5)), Woken::Immediate);
        assert!(
            t.elapsed() < Duration::from_secs(1),
            "no sleep on a stale token"
        );
        let space = Wakeup::default();
        let (p, c) = bounded::<u32>("space", 1);
        let c = c.with_space_wakeup(&space);
        p.try_push(1).unwrap();
        let token = space.token();
        assert_eq!(c.try_pop(), Some(1));
        assert_ne!(space.token(), token, "a pop rings the space doorbell");
    }
}

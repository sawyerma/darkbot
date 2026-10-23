//! Restartable WSS sessions; retains the shared clock, halt and slot-watch semantics.
use super::*;

pub(super) struct IngressSessions {
    pub(super) ws_url: String,
    pub(super) slot_watch: SlotWatch,
    pub(super) clock: Clock,
    pub(super) econ: Option<crate::analytics::econ::Handle>,
}

/// One running WSS session (C6/D10): its stop request, its thread and its generation. A
/// replaced session is stopped and joined before the next one starts, so two generations never
/// write into the shared ingress queue at the same time.
pub(super) struct IngressSession {
    shutdown: Shutdown,
    thread: Option<std::thread::JoinHandle<()>>,
    pub(super) generation: u64,
}

impl IngressSession {
    pub(super) fn new(
        shutdown: Shutdown,
        thread: std::thread::JoinHandle<()>,
        generation: u64,
    ) -> Self {
        IngressSession {
            shutdown,
            thread: Some(thread),
            generation,
        }
    }

    pub(super) fn request(&self) {
        self.shutdown.request();
    }

    /// Requests the stop and waits up to `budget` for the thread to end, then joins it. `join`
    /// itself cannot time out, so the thread is polled first; `false` = still running.
    pub(super) fn stop_and_join(&mut self, budget: Duration) -> bool {
        self.shutdown.request();
        let deadline = std::time::Instant::now() + budget;
        loop {
            match &self.thread {
                None => return true,
                Some(t) if t.is_finished() => {
                    // A panicked session has ended as well; its events are already queued.
                    let _ = self.thread.take().map(|t| t.join());
                    return true;
                }
                Some(_) if std::time::Instant::now() >= deadline => return false,
                Some(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}

/// Events still queued from retired session generations (C6/D10). After the old session is
/// joined, every queued event precedes the first event of the new session (FIFO), so the next
/// `remaining` pops belong to retired generations. Their account/slot/root data is chain data and
/// is applied normally; their StreamReady (`Reconnected`) is no readiness of the new session and
/// never starts a recovery.
#[derive(Debug, Default)]
pub(super) struct RetiredEvents {
    remaining: u64,
    pub(super) stream_ready_dropped: u64,
}

impl RetiredEvents {
    /// The session was joined with `queued` events in the queue; all of them are retired.
    pub(super) fn retire(&mut self, queued: u64) {
        self.remaining = queued;
    }

    /// `None` = a retired generation's StreamReady, dropped and counted.
    pub(super) fn admit(&mut self, ev: IngressEvent) -> Option<IngressEvent> {
        if self.remaining == 0 {
            return Some(ev);
        }
        self.remaining -= 1;
        if matches!(ev, IngressEvent::Reconnected { .. }) {
            self.stream_ready_dropped = self.stream_ready_dropped.saturating_add(1);
            return None;
        }
        Some(ev)
    }
}

impl IngressSessions {
    pub(super) fn spawn(
        &self,
        sub: Subscription,
        producer: darkbot_common::channel::Producer<IngressEvent>,
        halt: IngressHalt,
        generation: u64,
    ) -> std::io::Result<IngressSession> {
        let session = Shutdown::new();
        let mut ws_config = WsRpcConfig::new(self.ws_url.clone(), sub);
        if self.econ.is_some() {
            ws_config.recovery_budget = Some(Duration::from_millis(
                darkbot_solana::runtime_config::active()
                    .limits
                    .wss_recovery_ms,
            ));
        }
        let src = WsRpcControlSource::new(ws_config, self.clock);
        let failure = self.econ.as_ref().map(|h| h.shared.clone());
        let sd = session.clone();
        let sw = self.slot_watch.clone();
        let thread = std::thread::Builder::new()
            .name("ingress-ws".into())
            .spawn(move || run_ingress(src, producer, sd, halt, sw, failure))?;
        Ok(IngressSession::new(session, thread, generation))
    }
}

/// The ingress thread. R10: only a source that ends with an error — a real recovery
/// exhaustion or another unexpected failure — marks the measurement failed and aborts it; a
/// requested stop ends the WSS source with `Ok` (no Fatal, no abort). R6: the end is reported
/// as a fixed class through the bounded console, never a blocking stderr write.
pub(super) fn run_ingress<S: IngressSource>(
    src: S,
    producer: darkbot_common::channel::Producer<IngressEvent>,
    shutdown: Shutdown,
    halt: IngressHalt,
    slots: SlotWatch,
    failure: Option<std::sync::Arc<crate::analytics::econ::Shared>>,
) {
    if let Err(e) = src.run(producer, shutdown, halt, slots) {
        crate::console::line(format!("ingress ended: {}", ingress_error_class(&e)));
        if let Some(shared) = failure {
            shared
                .shadow_failed
                .store(true, std::sync::atomic::Ordering::Release);
            shared.request_abort();
        }
    }
}

/// A secret-free class of an ingress error: the fixed code of a connect error, else the kind.
fn ingress_error_class(e: &darkbot_solana::ingress::IngressError) -> String {
    use darkbot_solana::ingress::IngressError as E;
    let code = |s: &str| {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_uppercase() || b == b'_'))
            .then(|| s.to_string())
    };
    match e {
        E::Connect(s) => code(s).unwrap_or_else(|| "INGRESS_CONNECT_ERROR".into()),
        E::Protocol(s) => code(s).unwrap_or_else(|| "INGRESS_PROTOCOL_ERROR".into()),
        E::Config(_) => "INGRESS_CONFIG_ERROR".into(),
    }
}

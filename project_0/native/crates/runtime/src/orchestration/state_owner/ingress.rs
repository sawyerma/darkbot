//! Restartable WSS sessions; retains the shared clock, halt and slot-watch semantics.
use super::*;

pub(super) struct IngressSessions {
    pub(super) ws_url: String,
    pub(super) slot_watch: SlotWatch,
    pub(super) clock: Clock,
    pub(super) econ: Option<crate::analytics::econ::Handle>,
}

impl IngressSessions {
    pub(super) fn spawn(
        &self,
        sub: Subscription,
        producer: darkbot_common::channel::Producer<IngressEvent>,
        halt: IngressHalt,
    ) -> std::io::Result<Shutdown> {
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
        let failure_handle = self.econ.clone();
        let sd = session.clone();
        let sw = self.slot_watch.clone();
        std::thread::Builder::new()
            .name("ingress-ws".into())
            .spawn(move || {
                if let Err(e) = src.run(producer, sd, halt, sw) {
                    eprintln!("ingress ended: {}", e);
                    if let Some(h) = failure_handle {
                        h.shared
                            .shadow_failed
                            .store(true, std::sync::atomic::Ordering::Release);
                        h.shared
                            .abort
                            .store(true, std::sync::atomic::Ordering::Release);
                    }
                }
            })?;
        Ok(session)
    }
}

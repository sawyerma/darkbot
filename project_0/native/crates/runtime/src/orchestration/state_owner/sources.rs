//! S1-02 owner integration of the controlled data sources (OFF / OBSERVE / ACTIVE).
//!
//! The arbiter holds every unit's control. Only the canonical unit's session feeds the owner's
//! ingress queue (`ingress_session`, the existing generation machinery). An OBSERVE Geyser
//! session feeds the observe queue with its own halt and slot watch: it is measured (health,
//! conflicts, leads), never applied, never halts the state and never aborts the run. A control
//! change (`p0ctl source set` via the control thread) is applied here atomically: the unit's
//! session stops and is joined and its queued events are dropped and counted — never routed
//! under the new control; stopping the canonical session halts the state (FeedGap, gap) until a
//! recovery, which only a canonical session's StreamReady starts.
use super::*;
use crate::orchestration::source_control::{self, ControlMessage};
use darkbot_common::channel::{Consumer, Producer};
use darkbot_solana::ingress::sources::{
    Handoff, Route, SourceArbiter, SourceControl, SourceHealth, SourceUnit, Stamp,
};

/// Starts a Geyser session for `control` into `sink` (production: the adapter of the configured
/// provider, built in setup; tests: a loopback transport). `canonical`: the session feeds
/// canonical state (its failure then aborts an econ run, as the WSS session's does).
pub(super) type GeyserSpawner = Box<
    dyn FnMut(
        &SourceControl,
        Subscription,
        Producer<IngressEvent>,
        Shutdown,
        IngressHalt,
        SlotWatch,
        bool,
    ) -> Result<std::thread::JoinHandle<()>, String>,
>;

/// Observed events handled per owner pass, so canonical work is never starved.
const OBSERVE_BATCH: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SourceStats {
    pub(super) observed: u64,
    /// Events the arbiter refused (stale stamp, OFF) or that were not observable.
    pub(super) rejected: u64,
    /// Events queued by a session that a control change ended (never applied).
    pub(super) dropped_on_change: u64,
    pub(super) controls_applied: u64,
    pub(super) controls_refused: u64,
}

pub(super) struct OwnerSources {
    pub(super) arbiter: SourceArbiter,
    /// The configured Geyser unit (one provider).
    geyser_unit: Option<SourceUnit>,
    /// The observing Geyser session and its stamp; a canonical one is `ingress_session`.
    observer: Option<(IngressSession, Stamp)>,
    spawn: GeyserSpawner,
    observe_tx: Producer<IngressEvent>,
    observe_rx: Consumer<IngressEvent>,
    controls: Option<Consumer<ControlMessage>>,
    /// Where the owner records its decisions (`outcome_<seq>.p0j`); `None`: not recorded.
    control_dir: Option<std::path::PathBuf>,
    pub(super) health: SourceHealth,
    pub(super) stats: SourceStats,
}

impl OwnerSources {
    /// `observe` queue capacity; its pushes ring the owner doorbell `wake`.
    pub(super) fn new(
        arbiter: SourceArbiter,
        geyser_unit: Option<SourceUnit>,
        spawn: GeyserSpawner,
        observe: usize,
        wake: &darkbot_common::channel::Wakeup,
        controls: Option<Consumer<ControlMessage>>,
    ) -> Self {
        let (observe_tx, observe_rx) = bounded::<IngressEvent>("source-observe", observe);
        Self {
            arbiter,
            geyser_unit,
            observer: None,
            spawn,
            observe_tx: observe_tx.with_wakeup(wake),
            observe_rx,
            controls,
            control_dir: None,
            health: SourceHealth::default(),
            stats: SourceStats::default(),
        }
    }

    /// The owner records every control decision in `dir` (`p0ctl source status`).
    pub(super) fn record_decisions_in(mut self, dir: std::path::PathBuf) -> Self {
        self.control_dir = Some(dir);
        self
    }

    /// P5 S1-RACE: a canonical account copy for the lead measurement, only while a source
    /// observes (nothing to compare with otherwise); snapshot reads are no stream copies.
    pub(super) fn measure_canonical(&mut self, ev: &IngressEvent) {
        if self.observer.is_some()
            && matches!(ev, IngressEvent::Account(a) if a.source != SourceKind::Snapshot)
        {
            self.arbiter.measure_canonical(ev);
        }
    }

    /// Observed events or control messages wait for `sources_step`.
    pub(super) fn pending(&self) -> bool {
        self.observe_rx.pending() || self.controls.as_ref().is_some_and(|c| c.pending())
    }

    pub(super) fn line(&self) -> String {
        let s = self.stats;
        let l = self.arbiter.leads.snapshot();
        format!(
            "sources: canonical={} observing={} observed={} rejected={} stale={} conflicts={} dropped_on_change={} controls_applied={} controls_refused={} health_gaps={} health_reconnects={} lead_samples={} lead_evicted={}",
            self.arbiter
                .canonical()
                .map_or_else(|| "none".to_string(), unit_name),
            self.observer.as_ref().map_or_else(|| "none".to_string(), |(_, s)| unit_name(&s.unit)),
            s.observed,
            s.rejected,
            self.arbiter.stale,
            self.arbiter.conflicts,
            s.dropped_on_change,
            s.controls_applied,
            s.controls_refused,
            self.health.gaps,
            self.health.reconnects,
            l.samples,
            l.evicted,
        )
    }
}

pub(super) fn unit_name(unit: &SourceUnit) -> String {
    match unit {
        SourceUnit::WsRpcControl => "wss".into(),
        SourceUnit::BinaryGeyser { provider } => format!("geyser:{provider}"),
        SourceUnit::Deshred { provider } => format!("deshred:{provider}"),
        SourceUnit::LocalShred => "localshred".into(),
    }
}

impl ShadowOwner {
    /// The WSS/Geyser subscription of the owner's current dependency set.
    pub(super) fn source_subscription(&self) -> Subscription {
        let mut accounts: Vec<Pubkey> = self.subscribed.iter().copied().collect();
        accounts.sort_unstable_by_key(|k| k.to_bytes());
        Subscription {
            programs: vec![],
            program_filters: vec![
                (self.program, liability_account_filters(&self.group)),
                (self.program, bank_filters(&self.group)),
            ],
            accounts,
            slots: true,
            roots: true,
            transactions_mentioning: vec![],
            logs_mentioning: vec![],
        }
    }

    /// Control messages, then a bounded batch of observed events.
    pub(super) fn sources_step(
        &mut self,
        ingress: &IngressSessions,
    ) -> Result<(), Box<dyn std::error::Error>> {
        while let Some(m) = self.sources.controls.as_ref().and_then(|c| c.try_pop()) {
            match m {
                ControlMessage::Apply { seq, control } => {
                    self.apply_source_control(seq, control, ingress)?
                }
                ControlMessage::Refused {
                    seq,
                    request,
                    reason,
                } => {
                    self.sources.stats.controls_refused += 1;
                    owner_log!(
                        self,
                        "source_control: refused seq={seq} request={request} reason={reason}"
                    );
                    self.record_source_decision(seq, request, false, reason);
                }
            }
        }
        let OwnerSources {
            arbiter,
            observer,
            observe_rx,
            health,
            stats,
            ..
        } = &mut self.sources;
        for _ in 0..OBSERVE_BATCH {
            let Some(ev) = observe_rx.try_pop() else {
                break;
            };
            let Some((_, stamp)) = observer.as_ref() else {
                stats.rejected += 1;
                continue;
            };
            health.observe(&ev);
            match arbiter.route(stamp, ev) {
                Route::Observed(_) | Route::Conflict { .. } => stats.observed += 1,
                // The observe queue never feeds state, whatever the route says.
                Route::Canonical(_) | Route::Rejected(_) => stats.rejected += 1,
            }
        }
        Ok(())
    }

    /// Starts the configured sources at the owner's start (an OBSERVE Geyser unit).
    pub(super) fn start_sources(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.start_observer()
    }

    /// One control change, atomically or not at all (A23-03).
    pub(super) fn apply_source_control(
        &mut self,
        seq: u64,
        next: SourceControl,
        ingress: &IngressSessions,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let unit = next.unit.clone();
        let name = unit_name(&unit);
        let was_canonical = self.sources.arbiter.canonical() == Some(&unit);
        let request = format!("{name} {}", source_control::mode_name(next.mode));
        let handoff = match self.sources.arbiter.change(next.clone()) {
            Ok(h) => h,
            Err(reason) => {
                self.sources.stats.controls_refused += 1;
                owner_log!(
                    self,
                    "source_control: refused seq={seq} unit={name} mode={:?} reason={reason}",
                    next.mode
                );
                self.record_source_decision(seq, request, false, reason.into());
                return Ok(());
            }
        };
        self.sources.stats.controls_applied += 1;
        // The unit's running session ends first; nothing it queued is routed under the new
        // control.
        if was_canonical {
            self.stop_canonical(&unit)?;
        } else if self.sources.geyser_unit.as_ref() == Some(&unit) {
            self.stop_observer()?;
        }
        if next.may_connect() {
            if self.sources.arbiter.canonical() == Some(&unit) {
                if let Some(session) = self.spawn_canonical(ingress)? {
                    self.ingress_session = session;
                }
            } else {
                self.start_observer()?;
            }
        }
        let handoff = match handoff {
            Handoff::None => "none",
            Handoff::CanonicalRevoked { .. } => "canonical_revoked",
            Handoff::CanonicalGranted { .. } => "canonical_granted_recovery_required",
        };
        owner_log!(
            self,
            "source_control: applied seq={seq} unit={name} mode={:?} handoff={handoff} canonical_generation={}",
            next.mode,
            self.ingress_session.generation
        );
        self.record_source_decision(seq, request, true, handoff.into());
        Ok(())
    }

    /// Records the owner's decision on request `seq` (the applied state of `p0ctl source
    /// status`); a failed write is journaled, the decision itself stands.
    fn record_source_decision(&mut self, seq: u64, request: String, applied: bool, detail: String) {
        let Some(dir) = self.sources.control_dir.clone() else {
            return;
        };
        let outcome = crate::analytics::payload::SourceControlOutcome {
            seq,
            request,
            applied,
            detail,
            owner_pid: std::process::id(),
            decided_unix_s: super::accounts::wall_unix_s().ok(),
        };
        if let Err(e) = source_control::write_outcome(&dir, &outcome) {
            owner_log!(self, "source_control: decision seq={seq} not recorded: {e}");
        }
    }

    /// Ends the canonical session: joined, its queued events dropped, the state halted with a
    /// gap until a recovery from a canonical session.
    fn stop_canonical(&mut self, unit: &SourceUnit) -> Result<(), Box<dyn std::error::Error>> {
        let budget = Duration::from_secs(
            darkbot_solana::runtime_config::active()
                .limits
                .shutdown_seconds,
        );
        if !self.ingress_session.stop_and_join(budget) {
            owner_log!(
                self,
                "INGRESS_SESSION_NOT_QUIESCENT: generation={} still running after {} s",
                self.ingress_session.generation,
                budget.as_secs()
            );
            return Err(Box::new(OwnerTerminal("INGRESS_SESSION_NOT_QUIESCENT")));
        }
        while self.consumer.try_pop().is_some() {
            self.sources.stats.dropped_on_change += 1;
        }
        self.retired_events.retire(0);
        self.halt.raise();
        self.st.halt(darkbot_common::error::HaltReason::FeedGap);
        self.st.on_event(IngressEvent::Gap {
            source: match unit {
                SourceUnit::WsRpcControl => SourceKind::WsRpcControl,
                _ => SourceKind::Geyser,
            },
            from_slot: None,
            to_slot: None,
            t_rx_ns: self.clock.now_ns(),
        });
        Ok(())
    }

    /// The next canonical generation of the canonical unit; `None` without a canonical unit or
    /// when its session cannot start (the state stays halted, the reason is journaled).
    pub(super) fn spawn_canonical(
        &mut self,
        ingress: &IngressSessions,
    ) -> Result<Option<IngressSession>, Box<dyn std::error::Error>> {
        let generation = self.ingress_session.generation.saturating_add(1);
        let sub = self.source_subscription();
        let Some(unit) = self.sources.arbiter.canonical().cloned() else {
            return Ok(None);
        };
        if unit == SourceUnit::WsRpcControl {
            return Ok(Some(ingress.spawn(
                sub,
                self.producer.clone(),
                self.halt.clone(),
                generation,
            )?));
        }
        let Some(control) = self.sources.arbiter.control(&unit).cloned() else {
            return Ok(None);
        };
        let shutdown = Shutdown::new();
        match (self.sources.spawn)(
            &control,
            sub,
            self.producer.clone(),
            shutdown.clone(),
            self.halt.clone(),
            self.slot_watch.clone(),
            true,
        ) {
            Ok(thread) => Ok(Some(IngressSession::new(shutdown, thread, generation))),
            Err(reason) => {
                owner_log!(
                    self,
                    "source_control: canonical session of {} not started: {reason}",
                    unit_name(&unit)
                );
                Ok(None)
            }
        }
    }

    /// Starts the Geyser unit observing when its control allows it and it is not canonical.
    fn start_observer(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(unit) = self.sources.geyser_unit.clone() else {
            return Ok(());
        };
        if self.sources.observer.is_some() || self.sources.arbiter.canonical() == Some(&unit) {
            return Ok(());
        }
        let (Ok(stamp), Some(control)) = (
            self.sources.arbiter.stamp(&unit),
            self.sources.arbiter.control(&unit).cloned(),
        ) else {
            return Ok(());
        };
        let shutdown = Shutdown::new();
        let sub = self.source_subscription();
        // Its own halt and slot watch: an observed source never halts or advances the owner.
        match (self.sources.spawn)(
            &control,
            sub,
            self.sources.observe_tx.clone(),
            shutdown.clone(),
            IngressHalt::new(),
            SlotWatch::new(),
            false,
        ) {
            Ok(thread) => {
                let generation = stamp.generation;
                self.sources.observer =
                    Some((IngressSession::new(shutdown, thread, generation), stamp));
            }
            Err(reason) => owner_log!(
                self,
                "source_control: {} not observing: {reason}",
                unit_name(&unit)
            ),
        }
        Ok(())
    }

    /// Ends the observing session (joined); its queued events are dropped.
    pub(super) fn stop_observer(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some((mut session, stamp)) = self.sources.observer.take() {
            let budget = Duration::from_secs(
                darkbot_solana::runtime_config::active()
                    .limits
                    .shutdown_seconds,
            );
            if !session.stop_and_join(budget) {
                owner_log!(
                    self,
                    "SOURCE_SESSION_NOT_QUIESCENT: {} still running after {} s",
                    unit_name(&stamp.unit),
                    budget.as_secs()
                );
                return Err(Box::new(OwnerTerminal("SOURCE_SESSION_NOT_QUIESCENT")));
            }
        }
        while self.sources.observe_rx.try_pop().is_some() {
            self.sources.stats.dropped_on_change += 1;
        }
        Ok(())
    }
}

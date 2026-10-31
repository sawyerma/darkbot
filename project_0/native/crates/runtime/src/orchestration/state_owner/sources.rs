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
    /// The owner's last control decision (the applied state the status projection shows).
    pub(super) last_decision: Option<crate::status_tap::OwnerSourceDecision>,
    /// CTRL-03: the source configuration version the owner has decided up to (the request
    /// log's head at its start, then every decided request); `None` without source control.
    pub(super) config_version: Option<u64>,
    /// DEC-13: the operator's capability requests and the owner's decisions on them.
    pub(super) capabilities: OwnerCapabilities,
}

/// DEC-13/P8-CTRL: the requested capability values (`p0ctl capability set`) and the owner's
/// decisions; what LIVE makes effective is `capability_control::live_decision`'s alone.
#[derive(Debug)]
pub(super) struct OwnerCapabilities {
    /// Where the owner records its decisions; `None`: no capability control in this runtime.
    pub(super) dir: Option<std::path::PathBuf>,
    /// CTRL-03: the capability configuration version decided up to; `None` without control.
    pub(super) config_version: Option<u64>,
    /// P8-CTRL.6: this start's owner epoch (`None` without capability control): a request of
    /// another epoch is refused.
    pub(super) epoch: Option<u64>,
    /// P8-CTRL.1: the requested economic switches — DISABLED at every start.
    pub(super) econ: crate::orchestration::capability_control::EconSwitches,
    /// P8-CTRL.1: one switch per configured signer — DISABLED at every start.
    pub(super) wallets: Vec<crate::orchestration::capability_control::WalletSwitch>,
    /// P8-CTRL.6: the request ids decided in this epoch — the first request's sequence number and
    /// content (a second request with the id has no second effect).
    pub(super) decided_ids: std::collections::BTreeMap<String, (u64, String, String)>,
    /// The requested LIVE value: `OFF` at every start (a run never inherits a LIVE request).
    pub(super) live_requested: &'static str,
    /// DEC-04: the requested B hold mode: OBSERVE at every start; the effective one is the
    /// owner's hold gate.
    pub(super) hold_b_requested: crate::execution::hold_b::HoldMode,
    /// Why the effective B hold mode is not the requested one ("" when it is).
    pub(super) hold_b_block: &'static str,
    /// DEC-11: the requested bundle path: OFF at every start.
    pub(super) jito_requested: &'static str,
    /// The effective bundle path and why it is not the requested one.
    pub(super) jito_effective: &'static str,
    pub(super) jito_block: &'static str,
    /// DEC-13: the FE-06 authorization that opens LIVE; `None` in this build (no production path
    /// constructs one), so LIVE stays BLOCKED.
    pub(super) fe06: Option<crate::orchestration::capability_control::Fe06Authorization>,
    pub(super) last_decision: Option<crate::status_tap::OwnerSourceDecision>,
    pub(super) applied: u64,
    pub(super) refused: u64,
    /// P8-CTRL.3 (review M1): the scopes previewed for this epoch, read at the journal refresh
    /// cadence — never by the status projection.
    pub(super) previews: Fe06Previews,
    /// P8-CTRL.3: what this owner runs, fixed at its start (`None`: no capability control).
    pub(super) running: Option<crate::orchestration::fe06::Running>,
    /// P8-CTRL.7 (review M5): the stops the control thread handed over, shared with it.
    pub(super) stop: std::sync::Arc<crate::orchestration::capability_control::StopSignal>,
}

/// P8-CTRL.3: the cache of the FE-06 preview store. Each entry is decoded once (a store entry is
/// content-addressed: its bytes never change); one of another epoch is remembered as such, an
/// unreadable one is read again at the next refresh.
#[derive(Debug, Default)]
pub(super) struct Fe06Previews {
    seen: std::collections::BTreeMap<String, Option<crate::status_tap::OwnerFe06Preview>>,
    refreshed: Option<std::time::Instant>,
}

impl Fe06Previews {
    /// Reads the store entries not seen yet (`project`: the capability control's project), at
    /// most once per `every`.
    pub(super) fn refresh(
        &mut self,
        project: &std::path::Path,
        epoch: u64,
        every: std::time::Duration,
    ) {
        use crate::orchestration::fe06;
        if self.refreshed.is_some_and(|t| t.elapsed() < every) {
            return;
        }
        self.refreshed = Some(std::time::Instant::now());
        let Ok(digests) = fe06::preview_digests(project) else {
            return;
        };
        // Review NEW-L1: an entry no longer in the store is no preview any more.
        self.seen.retain(|d, _| digests.contains(d));
        for digest in digests {
            if self.seen.contains_key(&digest) {
                continue;
            }
            if let Ok(scope) = fe06::preview(project, &digest) {
                let shown =
                    (scope.owner_epoch == epoch).then(|| crate::status_tap::OwnerFe06Preview {
                        terminal: fe06::terminal(&scope).unwrap_or_default(),
                        digest: digest.clone(),
                        scope_mode: scope.mode.code(),
                        owner_epoch: scope.owner_epoch,
                        valid_until_unix_s: scope.expires_unix_s,
                    });
                self.seen.insert(digest, shown);
            }
        }
    }

    /// The previews of this epoch, in digest order.
    pub(super) fn current(&self) -> Vec<crate::status_tap::OwnerFe06Preview> {
        self.seen.values().flatten().cloned().collect()
    }
}

impl OwnerCapabilities {
    /// P8-CTRL.3: refreshes the preview cache at most once per `every` (no capability control:
    /// nothing to read).
    pub(super) fn refresh_previews(&mut self, every: std::time::Duration) {
        if let (Some(dir), Some(epoch)) = (&self.dir, self.epoch) {
            if let Some(project) = dir.parent().and_then(std::path::Path::parent) {
                self.previews.refresh(project, epoch, every);
            }
        }
    }

    /// P8-CTRL.2: what LIVE's effective value rests on now.
    pub(super) fn live_gates(&self) -> crate::orchestration::capability_control::LiveGates<'_> {
        crate::orchestration::capability_control::LiveGates {
            requested: self.live_requested,
            econ: &self.econ,
            wallets: &self.wallets,
            fe06: self.fe06.as_ref(),
            now_unix_s: super::accounts::wall_unix_s().ok(),
            epoch: self.epoch,
            running: self.running.as_ref(),
            source: None,
            stop: Some(&self.stop),
        }
    }

    /// P8-CTRL.6: a request of this owner's epoch whose id is new; the same id again is the same
    /// request (refused: no second effect), with other content a conflict.
    fn admit(&self, r: &crate::analytics::payload::CapabilityControlRequest) -> Result<(), String> {
        if let Some(epoch) = self.epoch {
            if r.owner_epoch != epoch {
                return Err(format!(
                    "CAPABILITY_EPOCH_STALE: the request names owner epoch {}, this owner is epoch {epoch}; reload and retry",
                    r.owner_epoch
                ));
            }
            crate::orchestration::capability_control::validate_request_id(&r.request_id)?;
        }
        if r.request_id.is_empty() {
            return Ok(());
        }
        match self.decided_ids.get(&r.request_id) {
            Some((seq, capability, requested))
                if (capability, requested) == (&r.capability, &r.requested) =>
            {
                Err(format!(
                    "CAPABILITY_REQUEST_REPLAYED: request id {} was decided as seq {seq}; no second effect",
                    r.request_id
                ))
            }
            Some((seq, _, _)) => Err(format!(
                "CAPABILITY_REQUEST_ID_CONFLICT: request id {} is seq {seq} with other content",
                r.request_id
            )),
            None => Ok(()),
        }
    }
}

impl Default for OwnerCapabilities {
    fn default() -> Self {
        Self {
            dir: None,
            config_version: None,
            epoch: None,
            econ: Default::default(),
            wallets: Vec::new(),
            decided_ids: Default::default(),
            live_requested: crate::orchestration::capability_control::LIVE_OFF,
            hold_b_requested: crate::execution::hold_b::HoldMode::Observe,
            hold_b_block: "",
            jito_requested: "OFF",
            jito_effective: "OFF",
            jito_block: "",
            fe06: None,
            last_decision: None,
            applied: 0,
            refused: 0,
            previews: Fe06Previews::default(),
            running: None,
            stop: Default::default(),
        }
    }
}

impl OwnerSources {
    /// P8-CTRL.2/.3: the gates with the canonical source of the owner's state — what every
    /// permit and every LIVE decision of the owner rests on.
    pub(super) fn live_gates(&self) -> crate::orchestration::capability_control::LiveGates<'_> {
        crate::orchestration::capability_control::LiveGates {
            source: self.arbiter.canonical(),
            ..self.capabilities.live_gates()
        }
    }

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
            last_decision: None,
            config_version: None,
            capabilities: OwnerCapabilities::default(),
        }
    }

    /// The owner records every control decision in `dir` (`p0ctl source status`).
    pub(super) fn record_decisions_in(mut self, dir: std::path::PathBuf, baseline: u64) -> Self {
        self.control_dir = Some(dir);
        self.config_version = Some(baseline);
        self
    }

    /// DEC-13: the owner decides capability requests from `baseline` on and records them in
    /// `dir` (`p0ctl capability status`).
    pub(super) fn capability_control_in(
        mut self,
        dir: std::path::PathBuf,
        baseline: u64,
        epoch: u64,
        stop: std::sync::Arc<crate::orchestration::capability_control::StopSignal>,
    ) -> Self {
        self.capabilities.dir = Some(dir);
        self.capabilities.config_version = Some(baseline);
        self.capabilities.epoch = Some(epoch);
        self.capabilities.stop = stop;
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
                ControlMessage::Capability { seq, request, stop } => {
                    self.apply_capability_control(seq, request);
                    // P8-CTRL.7 (review M5/L-b): a counted stop is decided (applied or refused)
                    // — it waits no more.
                    if stop {
                        self.sources.capabilities.stop.decide();
                    }
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

    /// DEC-11: the effective bundle path of the requested one (`bundle_path::effective`); the
    /// submitter holds a `JitoAdapter` exactly while it is not OFF (constructing it calls
    /// nothing).
    pub(super) fn apply_bundle_path(&mut self) {
        let requested = self.sources.capabilities.jito_requested;
        let (mut effective, mut block) =
            crate::execution::bundle_path::effective(requested, self.bundle_config.is_some());
        let timeout = std::time::Duration::from_millis(
            darkbot_solana::runtime_config::active()
                .limits
                .rpc_timeout_ms,
        );
        let config = self.bundle_config.clone().filter(|_| effective != "OFF");
        if let Err(e) = self
            .submitter
            .inner_mut()
            .set_bundle_path(config.as_ref(), timeout)
        {
            owner_log!(self, "capability_control: JITO_BUNDLE not built: {e}");
            let _ = self.submitter.inner_mut().set_bundle_path(None, timeout);
            (effective, block) = ("OFF", "JITO_ADAPTER_CONSTRUCTION_FAILED");
        }
        let c = &mut self.sources.capabilities;
        (c.jito_effective, c.jito_block) = (effective, block);
    }

    /// DEC-04: the effective B hold mode of the requested one (`hold_b::effective`: ACTIVE only
    /// with the user's N and the evidence), and why it is not the requested one. Only a request
    /// decides (`promote`, its decision recorded); a refresh keeps the decision or takes ACTIVE
    /// back when the evidence no longer carries it — never promotes on its own.
    pub(super) fn refresh_hold_gate(&mut self, promote: bool) {
        use crate::execution::hold_b::HoldMode;
        let requested = self.sources.capabilities.hold_b_requested;
        let (mode, block) = crate::execution::hold_b::effective(
            requested,
            self.hold_gate.n,
            &self.hold_gate.evidence,
        );
        if promote || mode != HoldMode::Active || self.hold_gate.mode == HoldMode::Active {
            if !promote && self.hold_gate.mode == HoldMode::Active && mode != HoldMode::Active {
                owner_log!(
                    self,
                    "capability_control: HOLD_B ACTIVE taken back: {block}; holds are A's"
                );
            }
            if promote || self.hold_gate.mode != mode {
                self.hold_gate.mode = mode;
                self.sources.capabilities.hold_b_block = block;
            }
        }
    }

    /// DEC-13: one capability request: a valid one becomes the requested value; the effective
    /// one is decided by `capability_control::live_effective` alone (BLOCKED without FE-06).
    /// Recorded like a source decision (`outcome_<seq>.p0j`, the projection's last decision).
    pub(super) fn apply_capability_control(
        &mut self,
        seq: u64,
        request: Result<crate::analytics::payload::CapabilityControlRequest, String>,
    ) {
        use crate::orchestration::capability_control as cap;
        let request_capability = request.as_ref().ok().map(|r| r.capability.clone());
        let request_id = request
            .as_ref()
            .map_or(String::new(), |r| r.request_id.clone());
        let request_value = request.as_ref().ok().map(|r| r.requested.clone());
        let (text, valid) = match request {
            Ok(r) => (
                cap::shown(&r.capability, &r.requested),
                self.sources
                    .capabilities
                    .admit(&r)
                    .and_then(|()| cap::validate(&r.capability, &r.requested))
                    .map(|()| r.requested),
            ),
            Err(reason) => ("unreadable".to_string(), Err(reason)),
        };
        // P8-CTRL.1: an economic switch, or the switch of a configured signer (only those).
        let econ = matches!(&request_capability, Some(c) if cap::ECON.contains(&c.as_str()));
        // P8-CTRL.3: a grant of a previewed scope, or the revocation of the grant in force.
        let fe06_grant = request_capability.as_deref() == Some(cap::FE06_GRANT);
        let fe06_revoke = request_capability.as_deref() == Some(cap::FE06_REVOKE);
        let wallet = request_capability
            .as_deref()
            .and_then(cap::wallet_of)
            .and_then(Result::ok);
        let valid = match valid {
            Ok(requested)
                if wallet.is_some_and(|pk| {
                    !self
                        .sources
                        .capabilities
                        .wallets
                        .iter()
                        .any(|w| w.pubkey == pk)
                }) =>
            {
                Err(format!(
                    "WALLET_NOT_CONFIGURED: {} is no configured signer of this owner",
                    cap::shown(request_capability.as_deref().unwrap_or(""), &requested)
                ))
            }
            // P8-CTRL.3: no grant without the operator's own authentication (undecided).
            Ok(_) if fe06_grant => Err(cap::FE06_AUTH_UNDECIDED.into()),
            Ok(digest)
                if fe06_revoke
                    && self
                        .sources
                        .capabilities
                        .fe06
                        .as_ref()
                        .is_none_or(|a| a.digest() != digest) =>
            {
                Err(format!(
                    "FE06_NOT_IN_FORCE: no grant of digest {digest} is in force; nothing to revoke"
                ))
            }
            other => other,
        };
        let hold_b =
            matches!(&request_capability, Some(c) if c == crate::execution::hold_b::HOLD_B);
        let jito = matches!(
            &request_capability,
            Some(c) if c == crate::execution::bundle_path::JITO_BUNDLE
        );
        // DEC-06: the threshold mode, hot at the owner under its own rule (OFF → ACTIVE refused).
        let threshold = matches!(
            &request_capability,
            Some(c) if c == crate::orchestration::threshold_mode::THRESHOLD_TRIGGER
        );
        // B01: inventory rebalancing, applied by the owner's observation (OFF|OBSERVE; ACTIVE is
        // requested, never effective).
        let rebalance = matches!(
            &request_capability,
            Some(c) if c == crate::execution::inventory::INVENTORY_REBALANCING
        );
        let valid = match valid {
            Ok(requested) if threshold => self
                .threshold_request(&requested, seq)
                .map(|()| requested)
                .map_err(String::from),
            Ok(requested) if rebalance => self.rebalance_request(&requested),
            other => other,
        };
        let c = &mut self.sources.capabilities;
        let applied = valid.is_ok();
        // P8-CTRL.6: every decided id of this epoch, applied or refused — its first decision.
        if let (Some(capability), Some(requested)) = (&request_capability, &request_value) {
            if !request_id.is_empty() {
                c.decided_ids
                    .entry(request_id.clone())
                    .or_insert_with(|| (seq, capability.clone(), requested.clone()));
            }
        }
        if let Ok(requested) = &valid {
            let on = requested == cap::ENABLED;
            if fe06_revoke {
                // P8-CTRL.3: the grant ends at once and the submitter closes with it.
                c.fe06 = None;
            } else if econ {
                c.econ.set(request_capability.as_deref().unwrap_or(""), on);
            } else if let Some(pk) = wallet {
                if let Some(w) = c.wallets.iter_mut().find(|w| w.pubkey == pk) {
                    w.enabled = on;
                }
            } else if jito {
                c.jito_requested = crate::execution::bundle_path::JITO_BUNDLE_VALUES
                    .into_iter()
                    .find(|v| v == requested)
                    .unwrap_or("OFF");
            } else if hold_b {
                c.hold_b_requested =
                    crate::execution::hold_b::HoldMode::parse(requested).unwrap_or_default();
            } else if threshold || rebalance {
                // Applied by `threshold_request` / `rebalance_request` (the owner's own modes).
            } else {
                c.live_requested = cap::LIVE_VALUES
                    .into_iter()
                    .find(|v| v == requested)
                    .unwrap_or(cap::LIVE_OFF);
            }
            c.applied = c.applied.saturating_add(1);
        } else {
            c.refused = c.refused.saturating_add(1);
        }
        if fe06_revoke && applied {
            self.submitter.close();
        }
        let switch = |on: bool| if on { cap::ENABLED } else { cap::DISABLED };
        let fe06_state = |c: &OwnerCapabilities| match &c.fe06 {
            Some(_) => "IN_FORCE",
            None => "NONE",
        };
        let digest_requested = request_value.clone().unwrap_or_default();
        let (requested, effective, block_reason): (&str, &str, String) =
            if fe06_grant || fe06_revoke {
                let c = &self.sources.capabilities;
                (digest_requested.as_str(), fe06_state(c), String::new())
            } else if econ {
                // P8-CTRL.1: a partial permission — effective as requested, never a command.
                let on = self
                    .sources
                    .capabilities
                    .econ
                    .get(request_capability.as_deref().unwrap_or(""))
                    .unwrap_or(false);
                (switch(on), switch(on), String::new())
            } else if let Some(pk) = wallet {
                let on = self
                    .sources
                    .capabilities
                    .wallets
                    .iter()
                    .any(|w| w.pubkey == pk && w.enabled);
                (switch(on), switch(on), String::new())
            } else if jito {
                self.apply_bundle_path();
                let c = &self.sources.capabilities;
                (c.jito_requested, c.jito_effective, c.jito_block.into())
            } else if hold_b {
                self.refresh_hold_gate(true);
                let c = &self.sources.capabilities;
                (
                    c.hold_b_requested.code(),
                    self.hold_gate.mode.code(),
                    c.hold_b_block.into(),
                )
            } else if threshold {
                let t = &self.threshold;
                (t.requested.code(), t.effective.code(), t.block.into())
            } else if rebalance {
                self.rebalance_state()
            } else {
                let c = &self.sources.capabilities;
                let (effective, block_reason) = cap::live_decision(&c.live_gates());
                (c.live_requested, effective, block_reason)
            };
        let (requested, effective) = (requested.to_string(), effective.to_string());
        let detail = match valid {
            Ok(_) => format!("requested {requested} · effective {effective} · {block_reason}"),
            Err(reason) => reason,
        };
        owner_log!(
            self,
            "capability_control: {} seq={seq} request={text} {detail}",
            if applied { "applied" } else { "refused" }
        );
        let c = &mut self.sources.capabilities;
        if let Some(version) = &mut c.config_version {
            *version = (*version).max(seq);
        }
        c.last_decision = Some(crate::status_tap::OwnerSourceDecision {
            seq,
            request: text.clone(),
            applied,
            detail: detail.clone(),
        });
        let Some(dir) = c.dir.clone() else {
            return;
        };
        let outcome = crate::analytics::payload::CapabilityControlOutcome {
            seq,
            request: text,
            applied,
            requested,
            effective,
            detail,
            owner_pid: std::process::id(),
            decided_unix_s: super::accounts::wall_unix_s().ok(),
            capability: request_capability.clone().unwrap_or_default(),
            block_reason,
            request_id,
            owner_epoch: c.epoch.unwrap_or(0),
        };
        if let Err(e) = cap::write_outcome(&dir, &outcome) {
            owner_log!(
                self,
                "capability_control: decision seq={seq} not recorded: {e}"
            );
            // P8-CTRL.1/.6: a permission stands only with its durable receipt — an unrecorded
            // ENABLED or LIVE is taken back (DISABLED, OFF).
            let c = &mut self.sources.capabilities;
            if let Some(d) = c.last_decision.as_mut().filter(|d| d.seq == seq) {
                d.applied = false;
                d.detail = format!(
                    "{} · RECEIPT_NOT_RECORDED: {e}; an ENABLED or LIVE is taken back",
                    d.detail
                );
            }
            if applied && outcome.requested != cap::DISABLED {
                if econ {
                    c.econ
                        .set(request_capability.as_deref().unwrap_or(""), false);
                } else if let Some(pk) = wallet {
                    c.wallets
                        .iter_mut()
                        .filter(|w| w.pubkey == pk)
                        .for_each(|w| w.enabled = false);
                } else if request_capability.as_deref() == Some(cap::LIVE) {
                    c.live_requested = cap::LIVE_OFF;
                }
            }
            // DEC-06: a threshold ACTIVE request stands only recorded (durable receipts).
            if threshold
                && self.threshold.requested
                    == crate::orchestration::threshold_mode::Requested::Active
            {
                self.threshold_demote_unrecorded();
            }
            // DEC-04: an ACTIVE decision stands only recorded.
            if hold_b && self.hold_gate.mode == crate::execution::hold_b::HoldMode::Active {
                self.hold_gate.mode = crate::execution::hold_b::HoldMode::Observe;
                self.sources.capabilities.hold_b_block =
                    crate::execution::hold_b::B_DECISION_UNRECORDED;
            }
        }
    }

    /// Records the owner's decision on request `seq` (the applied state of `p0ctl source
    /// status`); a failed write is journaled, the decision itself stands.
    fn record_source_decision(&mut self, seq: u64, request: String, applied: bool, detail: String) {
        if let Some(version) = &mut self.sources.config_version {
            *version = (*version).max(seq);
        }
        self.sources.last_decision = Some(crate::status_tap::OwnerSourceDecision {
            seq,
            request: request.clone(),
            applied,
            detail: detail.clone(),
        });
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

/// B01: a build without the protocol crate has no rebalancing observation: every request is
/// refused, nothing is effective.
#[cfg(not(feature = "crate_health"))]
impl ShadowOwner {
    pub(super) fn rebalance_request(&mut self, _requested: &str) -> Result<String, String> {
        Err(REBALANCE_UNAVAILABLE.into())
    }

    pub(super) fn rebalance_state(&self) -> (&'static str, &'static str, String) {
        ("OFF", "OFF", REBALANCE_UNAVAILABLE.into())
    }
}

#[cfg(not(feature = "crate_health"))]
const REBALANCE_UNAVAILABLE: &str =
    "REBALANCE_UNAVAILABLE: inventory rebalancing needs the crate_health build";

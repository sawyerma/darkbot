//! S1-08 and source control: every data source is its own controllable unit — a transport
//! adapter (long-tail WSS/RPC), a provider instance of the binary Geyser adapter, or a later
//! pre-execution sensor (K-02 `SubscribeDeshred`, K-02b LocalShred Lite). Its operating mode
//! (OFF / OBSERVE / ACTIVE) and its qualification are separate: ACTIVE needs a qualification, is
//! never `P0_MODE=live`, and only an ACTIVE, qualified, authoritative unit feeds canonical state.
//! A provider is admitted against its typed configuration (endpoint/TLS, auth from the
//! environment, quotas, retry budget, capabilities) with stated reasons; OFF starts nothing. The
//! arbiter routes several sources: one canonical unit, the others observation only; two copies of
//! the same write that differ are a conflict, never resolved by arrival order. The later
//! `p0ctl`/frontend control uses this contract; nothing here connects to a provider.
use super::geyser::{proto, BinaryGeyserSource, GeyserConfig};
use super::geyser_grpc::{Endpoint, GrpcTransport};
use super::{IngressEvent, Subscription};
use darkbot_common::timing::Clock;
use solana_sdk::pubkey::Pubkey;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

/// A controllable source unit. Provider instance, transport adapter and sensor stay distinct.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceUnit {
    /// Long-tail `WsRpcControlSource` (WSS + RPC snapshot/recovery).
    WsRpcControl,
    /// The Yellowstone `BinaryGeyserSource` fed by one provider instance (e.g. `publicnode`).
    BinaryGeyser { provider: String },
    /// K-02 (later): `SubscribeDeshred` of a provider — a pre-execution sensor.
    Deshred { provider: String },
    /// K-02b (later): LocalShred Lite — a pre-execution sensor.
    LocalShred,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceRole {
    /// May feed canonical chain state (once ACTIVE and qualified).
    Authoritative,
    /// Pre-execution signals only: early detection, candidate preparation, prewarming, lead-time
    /// measurement — never canonical state, profit confirmation, approval or submission.
    PreExecutionSensor,
}

impl SourceUnit {
    pub fn role(&self) -> SourceRole {
        match self {
            SourceUnit::WsRpcControl | SourceUnit::BinaryGeyser { .. } => SourceRole::Authoritative,
            SourceUnit::Deshred { .. } | SourceUnit::LocalShred => SourceRole::PreExecutionSensor,
        }
    }

    /// Whether this build has the unit's adapter (the sensors are later tickets).
    pub fn implemented(&self) -> bool {
        self.role() == SourceRole::Authoritative
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperatingMode {
    /// No connection, no events.
    Off,
    /// Capture, check, measure and journal; no trading eligibility.
    Observe,
    /// Used in its role — only after a successful qualification.
    Active,
}

impl std::str::FromStr for OperatingMode {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "off" => Ok(Self::Off),
            "observe" => Ok(Self::Observe),
            "active" => Ok(Self::Active),
            _ => Err("SOURCE_MODE_INVALID"),
        }
    }
}

/// Qualification state, separate from the operating mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Qualification {
    NotRun,
    /// Offline tests only (mocks, fixtures, loopback) — never a source qualification.
    LocalPass,
    /// A recorded qualification (evidence reference).
    Qualified {
        evidence: String,
    },
    Blocked(&'static str),
}

/// The control contract of one unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceControl {
    pub unit: SourceUnit,
    pub mode: OperatingMode,
    pub qualification: Qualification,
}

impl SourceControl {
    /// Refuses an unimplemented unit that is not OFF and an ACTIVE unit without qualification.
    pub fn new(
        unit: SourceUnit,
        mode: OperatingMode,
        qualification: Qualification,
    ) -> Result<Self, &'static str> {
        if mode != OperatingMode::Off && !unit.implemented() {
            return Err("SOURCE_NOT_IMPLEMENTED");
        }
        if mode == OperatingMode::Active
            && !matches!(qualification, Qualification::Qualified { .. })
        {
            return Err("SOURCE_ACTIVE_REQUIRES_QUALIFICATION");
        }
        Ok(Self {
            unit,
            mode,
            qualification,
        })
    }

    /// OFF starts no connection.
    pub fn may_connect(&self) -> bool {
        self.mode != OperatingMode::Off
    }

    /// Only an ACTIVE, qualified, authoritative unit feeds canonical state. This is no LIVE
    /// permission: `P0_MODE`, economics, Safety and FE-04 stay separate gates.
    pub fn feeds_canonical_state(&self) -> bool {
        self.mode == OperatingMode::Active
            && matches!(self.qualification, Qualification::Qualified { .. })
            && self.unit.role() == SourceRole::Authoritative
    }
}

/// What a provider is known to deliver; `Unknown` until a qualification proves it (NOT_RUN).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    Unknown,
    Proven,
    Absent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub accounts: Capability,
    pub transactions: Capability,
    pub slots: Capability,
    pub roots: Capability,
    pub from_slot_replay: Capability,
    pub bank_id: Capability,
    pub deshred: Capability,
}

/// Provider limits the subscription and the reconnect behaviour must respect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quota {
    pub max_streams: u32,
    pub max_account_keys: usize,
    pub max_owner_filters: usize,
    pub max_message_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryBudget {
    pub max_reconnects: u32,
    pub connect_timeout: Duration,
    pub idle_timeout: Duration,
}

/// The authentication a provider requires: a header whose value comes only from this
/// environment variable (never from the repository, never logged).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthRequirement {
    pub header: &'static str,
    pub token_env: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderConfig {
    pub id: String,
    pub endpoint: String,
    pub auth: Option<AuthRequirement>,
    pub quota: Quota,
    pub retry: RetryBudget,
    pub capabilities: Capabilities,
}

impl ProviderConfig {
    /// PublicNode Yellowstone gRPC, the first FREE_ONLY candidate (S1-08). Its token is free per
    /// the provider; every capability is `Unknown` until a separately approved qualification.
    pub fn publicnode() -> Self {
        let unknown = Capability::Unknown;
        Self {
            id: "publicnode".into(),
            endpoint: "https://solana.publicnode.com".into(),
            auth: Some(AuthRequirement {
                header: "x-token",
                token_env: "P0_GEYSER_PUBLICNODE_TOKEN",
            }),
            quota: Quota {
                max_streams: 1,
                max_account_keys: 10_000,
                max_owner_filters: 16,
                max_message_bytes: 16 << 20,
            },
            retry: RetryBudget {
                max_reconnects: 5,
                connect_timeout: Duration::from_secs(10),
                idle_timeout: Duration::from_secs(30),
            },
            capabilities: Capabilities {
                accounts: unknown,
                transactions: unknown,
                slots: unknown,
                roots: unknown,
                from_slot_replay: unknown,
                bank_id: unknown,
                deshred: unknown,
            },
        }
    }

    /// Every reason this provider cannot serve `subscription` in `mode` (empty: admitted). OBSERVE
    /// may run with unproven capabilities (that is how they are measured); ACTIVE needs every
    /// used one proven; an absent one rejects in any mode.
    pub fn rejections(
        &self,
        mode: OperatingMode,
        s: &Subscription,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Vec<&'static str> {
        let mut out = Vec::new();
        if mode == OperatingMode::Off {
            return out;
        }
        if Endpoint::parse(&self.endpoint).is_err() {
            out.push("PROVIDER_ENDPOINT_OR_TLS_INVALID");
        }
        if let Some(auth) = self.auth {
            if env(auth.token_env).is_none_or(|t| t.trim().is_empty()) {
                out.push("PROVIDER_AUTH_TOKEN_MISSING");
            }
        }
        if self.quota.max_streams == 0 {
            out.push("PROVIDER_QUOTA_NO_STREAM");
        }
        if s.accounts.len() > self.quota.max_account_keys {
            out.push("PROVIDER_QUOTA_ACCOUNT_KEYS");
        }
        if s.programs.len() + s.program_filters.len() > self.quota.max_owner_filters {
            out.push("PROVIDER_QUOTA_OWNER_FILTERS");
        }
        let c = &self.capabilities;
        let accounts =
            !s.accounts.is_empty() || !s.programs.is_empty() || !s.program_filters.is_empty();
        let used = [
            (accounts, c.accounts),
            (!s.transactions_mentioning.is_empty(), c.transactions),
            (s.slots, c.slots),
            (s.roots, c.roots),
        ];
        for (_, capability) in used.iter().filter(|(needed, _)| *needed) {
            match (capability, mode) {
                (Capability::Absent, _) => out.push("PROVIDER_CAPABILITY_ABSENT"),
                (Capability::Unknown, OperatingMode::Active) => {
                    out.push("PROVIDER_CAPABILITY_UNPROVEN")
                }
                _ => {}
            }
        }
        if !s.logs_mentioning.is_empty() {
            out.push("PROVIDER_LOGS_NOT_A_GEYSER_STREAM");
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// The production entry: a `BinaryGeyserSource` for this unit and provider, or `None` when the
/// unit is OFF (nothing is constructed or started). `from_slot` replay is used only when proven.
pub fn geyser_source(
    control: &SourceControl,
    provider: &ProviderConfig,
    subscription: Subscription,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<BinaryGeyserSource<GrpcTransport>>, Vec<&'static str>> {
    match &control.unit {
        SourceUnit::BinaryGeyser { provider: id } if *id == provider.id => {}
        _ => return Err(vec!["SOURCE_UNIT_PROVIDER_MISMATCH"]),
    }
    if !control.may_connect() {
        return Ok(None);
    }
    let reasons = provider.rejections(control.mode, &subscription, env);
    if !reasons.is_empty() {
        return Err(reasons);
    }
    let token = provider.auth.and_then(|a| env(a.token_env));
    let transport = GrpcTransport::new(
        &provider.endpoint,
        token,
        provider.retry.connect_timeout,
        provider.retry.idle_timeout,
    )
    .map_err(|_| vec!["PROVIDER_TRANSPORT_INVALID"])?;
    Ok(Some(BinaryGeyserSource {
        config: GeyserConfig {
            endpoint: provider.endpoint.clone(),
            token_env: provider.auth.map_or("", |a| a.token_env),
            commitment: proto::CommitmentLevel::Processed,
            max_message_bytes: provider.quota.max_message_bytes,
            max_reconnects: provider.retry.max_reconnects,
            replay_from_slot: provider.capabilities.from_slot_replay == Capability::Proven,
        },
        subscription,
        transport,
        clock: Clock::new(),
    }))
}

/// A pre-execution sensor signal (K-02/K-02b, later). Deliberately not an `IngressEvent`: it
/// cannot reach the canonical state owner, confirm profit, create an approval or a submission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreExecutionSignal {
    pub unit: SourceUnit,
    pub signature: String,
    pub slot: u64,
    pub accounts: Vec<Pubkey>,
    pub t_rx_ns: u64,
}

/// Source health that never hides a gap: a gap or a fatal error stays visible until an explicit
/// recovery (snapshot reconciliation) — later data does not turn it green.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceHealth {
    pub events: u64,
    pub gaps: u64,
    pub reconnects: u64,
    pub gap_open: bool,
    pub fatal: bool,
    pub last_slot: Option<u64>,
}

impl SourceHealth {
    pub fn observe(&mut self, ev: &IngressEvent) {
        self.events += 1;
        match ev {
            IngressEvent::Gap { .. } => {
                self.gaps += 1;
                self.gap_open = true;
            }
            IngressEvent::Reconnected { .. } => {
                self.reconnects += 1;
                self.gap_open = true;
            }
            IngressEvent::Fatal { .. } => self.fatal = true,
            IngressEvent::Slot(s) => {
                self.last_slot = Some(self.last_slot.map_or(s.slot, |l| l.max(s.slot)))
            }
            _ => {}
        }
    }
    /// Only a completed recovery closes a gap; a fatal source stays failed.
    pub fn recovered(&mut self) {
        self.gap_open = false;
    }
    pub fn healthy(&self) -> bool {
        !self.gap_open && !self.fatal
    }
}

/// Where an event of a source goes.
#[derive(Debug, PartialEq)]
pub enum Route {
    /// To the canonical state owner (the one canonical unit).
    Canonical(IngressEvent),
    /// Measurement/journal only: no state, no trading eligibility.
    Observed(IngressEvent),
    /// Two sources delivered different bytes for the same write: neither is applied; the owner
    /// must treat the state as a gap.
    Conflict {
        pubkey: Pubkey,
        slot: u64,
        write_version: u64,
    },
    /// A unit that may not deliver events (OFF, a sensor).
    Rejected(&'static str),
}

/// Lead of an observed source over the canonical one for the same account write (ns; negative:
/// the observed source was later). Measurement only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lead {
    pub pubkey: Pubkey,
    pub slot: u64,
    pub lead_ns: i128,
}

/// A23-04: newest leads kept as samples; older ones stay in the counters and histograms.
pub const LEAD_RETENTION: usize = 4_096;

/// A23-04: bounded lead telemetry — fixed memory whatever the run length: the newest
/// `LEAD_RETENTION` samples, every lead in two histograms (observed ahead / behind, ns), and the
/// samples no longer retained counted, never silently lost.
#[derive(Default)]
pub struct LeadTelemetry {
    recent: VecDeque<Lead>,
    pub samples: u64,
    /// Samples evicted from `recent` (still in the histograms).
    pub evicted: u64,
    ahead: darkbot_common::mt0::Hist,
    behind: darkbot_common::mt0::Hist,
}

/// A typed snapshot of `LeadTelemetry` (quantiles are histogram upper bounds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeadSnapshot {
    pub samples: u64,
    pub evicted: u64,
    pub ahead_ns: darkbot_common::mt0::HistSnapshot,
    pub behind_ns: darkbot_common::mt0::HistSnapshot,
}

impl LeadTelemetry {
    fn record(&mut self, lead: Lead) {
        self.samples += 1;
        let ns = u64::try_from(lead.lead_ns.unsigned_abs()).unwrap_or(u64::MAX);
        if lead.lead_ns >= 0 {
            self.ahead.record(ns);
        } else {
            self.behind.record(ns);
        }
        if self.recent.len() == LEAD_RETENTION {
            self.recent.pop_front();
            self.evicted += 1;
        }
        self.recent.push_back(lead);
    }
    /// The retained samples, oldest first.
    pub fn recent(&self) -> impl Iterator<Item = &Lead> {
        self.recent.iter()
    }
    pub fn snapshot(&self) -> LeadSnapshot {
        LeadSnapshot {
            samples: self.samples,
            evicted: self.evicted,
            ahead_ns: self.ahead.snapshot(),
            behind_ns: self.behind.snapshot(),
        }
    }
}

const RECENT_WRITES: usize = 65_536;

/// One account write: (pubkey, slot, write version).
type WriteId = (Pubkey, u64, u64);
/// Its first copy: the unit, the content digest and its T_rx.
type FirstCopy = (SourceUnit, [u8; 32], u64);

/// A23-03: what a session of a unit was admitted under. Events carry it to the arbiter; a
/// control change of the unit bumps its generation, so events stamped before (queued, in
/// flight, of the replaced session) are refused, never routed under the new control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub unit: SourceUnit,
    pub generation: u64,
}

/// A23-03: the canonical consequence of a control change, for the owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Handoff {
    /// The canonical unit is unchanged.
    None,
    /// `unit` lost canonical authority (ACTIVE→OBSERVE/OFF, qualification revoked): no unit is
    /// canonical; the owner treats the state as a gap until a recovery from a canonical source.
    CanonicalRevoked { unit: SourceUnit },
    /// `unit` became canonical: its events count only under a new stamp, after a recovery.
    CanonicalGranted { unit: SourceUnit },
}

/// Routes the events of several units: at most one canonical unit; every other unit observes.
/// The arbiter holds the controls; a change is applied here atomically or not at all.
pub struct SourceArbiter {
    controls: HashMap<SourceUnit, (SourceControl, u64)>,
    canonical: Option<SourceUnit>,
    /// (pubkey, slot, write version) → (unit, digest, t_rx) of its first copy.
    writes: HashMap<WriteId, FirstCopy>,
    order: VecDeque<WriteId>,
    pub conflicts: u64,
    /// Events refused for a stale stamp or an unknown unit (A23-03).
    pub stale: u64,
    pub leads: LeadTelemetry,
}

impl SourceArbiter {
    /// The arbiter of these controls (generation 1 each): refuses a second canonical unit and a
    /// unit listed twice.
    pub fn new(controls: &[SourceControl]) -> Result<Self, &'static str> {
        let mut canonical = controls.iter().filter(|c| c.feeds_canonical_state());
        let first = canonical.next().map(|c| c.unit.clone());
        if canonical.next().is_some() {
            return Err("ARBITER_SECOND_CANONICAL_SOURCE");
        }
        let mut map = HashMap::new();
        for c in controls {
            if map.insert(c.unit.clone(), (c.clone(), 1)).is_some() {
                return Err("ARBITER_DUPLICATE_UNIT");
            }
        }
        Ok(Self {
            controls: map,
            canonical: first,
            writes: HashMap::new(),
            order: VecDeque::new(),
            conflicts: 0,
            stale: 0,
            leads: LeadTelemetry::default(),
        })
    }

    pub fn control(&self, unit: &SourceUnit) -> Option<&SourceControl> {
        self.controls.get(unit).map(|(c, _)| c)
    }

    pub fn canonical(&self) -> Option<&SourceUnit> {
        self.canonical.as_ref()
    }

    /// The stamp a new session of `unit` runs under; OFF, a sensor and an unknown unit get none.
    pub fn stamp(&self, unit: &SourceUnit) -> Result<Stamp, &'static str> {
        let (control, generation) = self.controls.get(unit).ok_or("SOURCE_UNKNOWN")?;
        if !control.may_connect() {
            return Err("SOURCE_OFF");
        }
        if control.unit.role() == SourceRole::PreExecutionSensor {
            return Err("SENSOR_NEVER_FEEDS_STATE");
        }
        Ok(Stamp {
            unit: unit.clone(),
            generation: *generation,
        })
    }

    /// Applies one unit's new control atomically: mode, qualification and generation change
    /// together, or (second canonical unit, unknown unit) nothing changes.
    pub fn change(&mut self, next: SourceControl) -> Result<Handoff, &'static str> {
        let unit = next.unit.clone();
        if !self.controls.contains_key(&unit) {
            return Err("SOURCE_UNKNOWN");
        }
        let was = self.canonical.as_ref() == Some(&unit);
        let feeds = next.feeds_canonical_state();
        if feeds && self.canonical.as_ref().is_some_and(|c| *c != unit) {
            return Err("ARBITER_SECOND_CANONICAL_SOURCE");
        }
        let entry = self.controls.get_mut(&unit).ok_or("SOURCE_UNKNOWN")?;
        *entry = (next, entry.1 + 1);
        Ok(match (was, feeds) {
            (true, false) => {
                self.canonical = None;
                Handoff::CanonicalRevoked { unit }
            }
            (false, true) => {
                self.canonical = Some(unit.clone());
                Handoff::CanonicalGranted { unit }
            }
            _ => Handoff::None,
        })
    }

    pub fn route(&mut self, stamp: &Stamp, ev: IngressEvent) -> Route {
        let Some((control, generation)) = self.controls.get(&stamp.unit) else {
            self.stale += 1;
            return Route::Rejected("SOURCE_UNKNOWN");
        };
        if *generation != stamp.generation {
            self.stale += 1;
            return Route::Rejected("SOURCE_GENERATION_STALE");
        }
        if !control.may_connect() {
            return Route::Rejected("SOURCE_OFF");
        }
        if control.unit.role() == SourceRole::PreExecutionSensor {
            return Route::Rejected("SENSOR_NEVER_FEEDS_STATE");
        }
        // Canonical only while the unit's current control feeds canonical state.
        let canonical =
            self.canonical.as_ref() == Some(&stamp.unit) && control.feeds_canonical_state();
        if let IngressEvent::Account(a) = &ev {
            if let Some(wv) = a.write_version {
                let digest = solana_sdk::hash::hashv(&[
                    a.owner.as_ref(),
                    &a.lamports.to_le_bytes(),
                    &a.data,
                ])
                .to_bytes();
                let id = (a.pubkey, a.slot, wv);
                match self.writes.get(&id) {
                    Some((unit, seen, _)) if *seen != digest && *unit != stamp.unit => {
                        self.conflicts += 1;
                        return Route::Conflict {
                            pubkey: a.pubkey,
                            slot: a.slot,
                            write_version: wv,
                        };
                    }
                    Some((unit, _, t)) if *unit != stamp.unit => {
                        let canonical_first = Some(unit) == self.canonical.as_ref();
                        if canonical_first || canonical {
                            let lead = i128::from(a.t_rx_ns) - i128::from(*t);
                            self.leads.record(Lead {
                                pubkey: a.pubkey,
                                slot: a.slot,
                                lead_ns: if canonical_first { -lead } else { lead },
                            });
                        }
                    }
                    Some(_) => {}
                    None => {
                        self.writes
                            .insert(id, (stamp.unit.clone(), digest, a.t_rx_ns));
                        self.order.push_back(id);
                        if self.order.len() > RECENT_WRITES {
                            if let Some(old) = self.order.pop_front() {
                                self.writes.remove(&old);
                            }
                        }
                    }
                }
            }
        }
        if canonical {
            Route::Canonical(ev)
        } else {
            Route::Observed(ev)
        }
    }
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;

//! The native owner of the BSC runtime's switches (plan §5a.2, v9 §2,
//! order V2 §5, plan decision 27.10.): the five signal sources, the runtime
//! components and the three venues, each with a user-requested mode
//! `OFF | SHADOW | LIVE` and the mode this owner makes effective, and the
//! wallet and economic switches `DISABLED | ENABLED`. A requested LIVE is
//! stored `BLOCKED` (`ECONOMIC_LIVE_DISABLED` while the global economic
//! LIVE switch is off, `ECONOMIC_LIVE_NOT_AUTHORIZED` once it is on); it is
//! effective LIVE only as it holds now ([`Generation::effective_live_now`],
//! plan 28.10.): with the operator's LIVE grant of this boot session for a
//! scope that names it, every wallet and economic switch ENABLED, BUY
//! attempts left today and in the campaign, and the qualification evidence
//! of the grant's venue, sources and adapters, its acquisition and its risk
//! decision. A source whose access is not qualified is `BLOCKED` even in
//! SHADOW. Health is the runtime's, not this configuration's.
//!
//! The switches are permissions, never actions: an ENABLED switch makes
//! nothing sign or send. Every Mainnet signing or sending path checks the
//! switches it needs ([`require_enabled_here`]) besides its own guard, V5,
//! nonce, budget and attempt checks. Every switch is DISABLED initially, in
//! a missing or unreadable state, and after every restart: an ENABLED
//! switch counts only within the host boot session it was written in, and
//! a restart of the owner ([`Store::restart`]) writes them DISABLED.
//!
//! State: one binary record per generation, `generations/<n>.dbrec`, created
//! create-new: two writers of the same next generation cannot both succeed,
//! so the generation is a compare-and-swap without a lock. Every request
//! leaves one receipt, `receipts/<request_id>.dbrec`; the same request again
//! gets the same receipt, another payload under its ID is refused. A crash
//! between the new generation and its receipt is recovered from the
//! generation, which names the request it applied. A receipt is kept as
//! long as the generation it names and the one its request expected
//! (BSC3-008), and goes at the next applied request after both left the
//! kept ones: no request applies twice without it, its expected
//! generation is stale then; the same request again reads UNRESOLVED or
//! REJECTED_STALE (BSC3-006). An ID whose receipt went names a new request
//! again (the generation it expects is the check, not the ID).

use alloy_primitives::U256;
use darkbot_bsc_record::schema::bsc_runtime::{source_mode_receipt as rc, source_modes as sm};
use darkbot_bsc_record::{Draft, Record, decode};
use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

mod admission;
mod grant;
mod ids;
mod reasons;
mod store;

pub use admission::*;
pub use grant::*;
pub use ids::*;
pub use reasons::*;
pub use store::*;

/// Where a checkout or snapshot keeps the configuration its runtime runs
/// with (`bsc_runtime run --config`).
pub const RUNTIME_CONFIG_PATH: &str = "bsc/config/ingress.public.env";

/// The value of `key` in a configuration file's text (`KEY=value` lines,
/// comments aside); a key set twice is refused, as the runtime refuses it.
pub fn config_value<'a>(text: &'a str, key: &str) -> Result<Option<&'a str>, String> {
    let mut values = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .filter(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim());
    let value = values.next();
    match values.next() {
        Some(_) => Err(format!("{key} is set twice")),
        None => Ok(value),
    }
}

/// Audit E 3.1/3.4: the owner's bounds, one value each in the runtime
/// configuration (`bsc/config/ingress.public.env`) that the runtime, every
/// owner command (`--config`), the reporter and the G0.7 runner read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnerLimits {
    /// `BSC_BUY_ATTEMPTS_TOTAL_MAX`: the most BUY attempts a day or the
    /// whole campaign may take (the user's decision 27.10./28.10.: at most
    /// ten real BUY attempts in all; the key's range).
    pub buy_attempts_total_max: u32,
    /// `BSC_OWNER_KEEP_GENERATIONS`: the generations a store keeps besides
    /// the current one (older ones and their receipts are pruned).
    pub keep_generations: u64,
}

impl OwnerLimits {
    pub const BUY_ATTEMPTS_TOTAL_MAX: &str = "BSC_BUY_ATTEMPTS_TOTAL_MAX";
    pub const KEEP_GENERATIONS: &str = "BSC_OWNER_KEEP_GENERATIONS";

    /// The two keys' values as written (each required, a decimal in its
    /// range).
    pub fn new(max: Option<&str>, keep: Option<&str>) -> Result<Self, String> {
        let take = |key: &str, v: Option<&str>, lo: u64, hi: u64| -> Result<u64, String> {
            let v: u64 = v
                .ok_or(format!("{key} is required"))?
                .parse()
                .map_err(|_| format!("{key} is not an unsigned integer"))?;
            if !(lo..=hi).contains(&v) {
                return Err(format!("{key}={v} is outside {lo}..={hi}"));
            }
            Ok(v)
        };
        Ok(OwnerLimits {
            buy_attempts_total_max: take(Self::BUY_ATTEMPTS_TOTAL_MAX, max, 1, 10)? as u32,
            keep_generations: take(Self::KEEP_GENERATIONS, keep, 1, 100_000)?,
        })
    }

    /// The bounds of a configuration file's text (`KEY=value` lines).
    pub fn of_config(text: &str) -> Result<Self, String> {
        Self::new(
            config_value(text, Self::BUY_ATTEMPTS_TOTAL_MAX)?,
            config_value(text, Self::KEEP_GENERATIONS)?,
        )
    }

    /// [`Self::of_config`] of the file at `path`.
    pub fn at(path: &Path) -> Result<Self, String> {
        Self::of_config(&fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?)
    }
}
/// The operator's LIVE grant of one scope (`GRANTED:<scope SHA-256>`) or
/// its revocation (`DISABLED`).
pub const LIVE_GRANT: &str = "BSC_LIVE_GRANT";

fn hex32(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 64 lowercase hex digits.
fn parse_hex32(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// One component of a generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentMode {
    pub component: ComponentId,
    pub requested: Mode,
    pub effective: Effective,
}

/// One source of a generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceMode {
    pub source: SourceId,
    pub requested: Mode,
    pub effective: Effective,
}

/// The request (or restart reset) that last set a venue or switch, and the
/// generation it wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetBy {
    pub request_id: String,
    pub generation: u64,
}

/// One venue of a generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VenueMode {
    pub venue: VenueId,
    pub requested: Mode,
    pub effective: Effective,
    pub set_by: Option<SetBy>,
}

/// One switch of a generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchMode {
    pub switch: SwitchId,
    pub requested: Mode,
    pub effective: Effective,
    pub set_by: Option<SetBy>,
}

/// The runtime mode or the daily BUY limit of a generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setting {
    pub requested: Mode,
    pub effective: Effective,
    pub set_by: Option<SetBy>,
}

/// One generation of every source, component, venue and switch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generation {
    pub generation: u64,
    pub sources: [SourceMode; 5],
    pub components: [ComponentMode; 3],
    pub venues: [VenueMode; 3],
    pub switches: [SwitchMode; 7],
    /// `BSC_RUNTIME_MODE` (OFF initially).
    pub runtime: Setting,
    /// `BSC_BUY_ATTEMPTS_PER_DAY` (0 initially).
    pub buy_limit: Setting,
    /// `BSC_BUY_ATTEMPTS_TOTAL` (0 initially).
    pub buy_total: Setting,
    /// The LIVE grant in force (none: never granted or revoked).
    pub live: Option<LiveGrant>,
    /// The request (or restart reset) that last granted or revoked LIVE.
    pub live_set_by: Option<SetBy>,
    /// REV.1.0 R9 (BSC3-001): the operator's presence signature of the
    /// grant in force, as granted with it (none: unsigned).
    pub live_presence: Option<[u8; 65]>,
    pub request_id: Option<String>,
    /// The host boot session this generation was written in (none for the
    /// initial state and generations from before switches existed).
    pub boot: Option<String>,
}

impl Generation {
    /// Generation 0: everything OFF and DISABLED (nothing was ever requested).
    pub fn initial() -> Self {
        Generation {
            generation: 0,
            sources: SourceId::ALL.map(|source| SourceMode {
                source,
                requested: Mode::Off,
                effective: Effective::Off,
            }),
            components: ComponentId::ALL.map(|component| ComponentMode {
                component,
                requested: Mode::Off,
                effective: Effective::Off,
            }),
            venues: VenueId::ALL.map(|venue| VenueMode {
                venue,
                requested: Mode::Off,
                effective: Effective::Off,
                set_by: None,
            }),
            switches: SwitchId::ALL.map(|switch| SwitchMode {
                switch,
                requested: Mode::Disabled,
                effective: Effective::Disabled,
                set_by: None,
            }),
            runtime: Setting {
                requested: Mode::Off,
                effective: Effective::Off,
                set_by: None,
            },
            buy_limit: Setting {
                requested: Mode::PerDay(0),
                effective: Effective::PerDay(0),
                set_by: None,
            },
            buy_total: Setting {
                requested: Mode::PerDay(0),
                effective: Effective::PerDay(0),
                set_by: None,
            },
            live: None,
            live_set_by: None,
            live_presence: None,
            request_id: None,
            boot: None,
        }
    }

    pub fn component(&self, id: ComponentId) -> ComponentMode {
        self.components[id.index()]
    }

    pub fn source(&self, id: SourceId) -> SourceMode {
        self.sources[id.index()]
    }

    pub fn venue(&self, id: VenueId) -> &VenueMode {
        &self.venues[id.index()]
    }

    pub fn switch(&self, id: SwitchId) -> &SwitchMode {
        &self.switches[id.index()]
    }

    /// The effective mode of any entity, as stored.
    pub fn effective_of(&self, entity: Entity) -> Effective {
        match entity {
            Entity::Source(s) => self.source(s).effective,
            Entity::Component(c) => self.component(c).effective,
            Entity::Venue(v) => self.venue(v).effective,
            Entity::Switch(s) => self.switch(s).effective,
            Entity::Runtime => self.runtime.effective,
            Entity::BuyLimit => self.buy_limit.effective,
            Entity::BuyTotal => self.buy_total.effective,
            Entity::LiveGrant => {
                Entity::LiveGrant.effective(self.requested_of(entity), LiveGate::Disabled)
            }
        }
    }

    /// Whether this generation (naming `request_id`) is that request's
    /// change: the entity holds the requested mode, and for a venue, switch
    /// or setting it was set by this request in this generation.
    fn applied(&self, request_id: &str, entity: Entity, requested: Mode) -> bool {
        let set_here = |by: &Option<SetBy>| {
            by.as_ref()
                .is_some_and(|b| b.request_id == request_id && b.generation == self.generation)
        };
        self.requested_of(entity) == requested
            && match entity {
                Entity::Venue(v) => set_here(&self.venue(v).set_by),
                Entity::Switch(s) => set_here(&self.switch(s).set_by),
                Entity::Runtime => set_here(&self.runtime.set_by),
                Entity::BuyLimit => set_here(&self.buy_limit.set_by),
                Entity::BuyTotal => set_here(&self.buy_total.set_by),
                Entity::LiveGrant => set_here(&self.live_set_by),
                // Sources and components keep no setter: the request changed
                // one of them only if it set nothing else in this generation.
                Entity::Source(_) | Entity::Component(_) => {
                    !self.venues.iter().any(|v| set_here(&v.set_by))
                        && !self.switches.iter().any(|s| set_here(&s.set_by))
                        && !set_here(&self.runtime.set_by)
                        && !set_here(&self.buy_limit.set_by)
                        && !set_here(&self.buy_total.set_by)
                        && !set_here(&self.live_set_by)
                }
            }
    }

    /// The requested mode of any entity.
    pub fn requested_of(&self, entity: Entity) -> Mode {
        match entity {
            Entity::Source(s) => self.source(s).requested,
            Entity::Component(c) => self.component(c).requested,
            Entity::Venue(v) => self.venue(v).requested,
            Entity::Switch(s) => self.switch(s).requested,
            Entity::Runtime => self.runtime.requested,
            Entity::BuyLimit => self.buy_limit.requested,
            Entity::BuyTotal => self.buy_total.requested,
            Entity::LiveGrant => self
                .live
                .as_ref()
                .map_or(Mode::Disabled, |g| Mode::Granted(g.scope_sha256())),
        }
    }

    /// The daily BUY attempt limit in force under the maximum `max`
    /// (BSC3-004: never more a day than the whole campaign may take).
    pub fn buy_attempts_per_day(&self, max: u32) -> u32 {
        match self.buy_limit.effective {
            Effective::PerDay(n) => n.min(max),
            _ => 0,
        }
    }

    /// The campaign's total BUY attempt limit in force under `max`.
    pub fn buy_attempts_total(&self, max: u32) -> u32 {
        match self.buy_total.effective {
            Effective::PerDay(n) => n.min(max),
            _ => 0,
        }
    }

    /// Every condition a LIVE of this generation misses now, in the order
    /// the operator meets them (empty: the grant is live). The switches and
    /// the grant count only in the boot session they were written in.
    pub fn live_blockers(&self, ctx: &LiveContext<'_>) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.switch_now(SwitchId::Live, ctx.boot) != Effective::Enabled {
            out.push(ECONOMIC_LIVE_DISABLED);
        }
        let grant = match &self.live {
            None => {
                out.push(LIVE_GRANT_MISSING);
                None
            }
            Some(_) if ctx.boot.is_none() || self.boot.as_deref() != ctx.boot => {
                out.push(LIVE_GRANT_RESET_BY_RESTART);
                None
            }
            Some(g) if ctx.now_unix_ms >= g.expires_unix_ms => {
                out.push(LIVE_GRANT_EXPIRED);
                None
            }
            Some(g) => Some(g),
        };
        for (switch, reason) in [
            (SwitchId::WalletLauncher, WALLET_LAUNCHER_DISABLED),
            (SwitchId::WalletBuyer, WALLET_BUYER_DISABLED),
            (SwitchId::Sign, ECON_SIGN_DISABLED),
            (SwitchId::Arm, ECON_ARM_DISABLED),
            (SwitchId::Send, ECON_SEND_DISABLED),
            (SwitchId::Attempt, ECON_ATTEMPT_DISABLED),
        ] {
            if self.switch_now(switch, ctx.boot) != Effective::Enabled {
                out.push(reason);
            }
        }
        if self.runtime.requested != Mode::Live {
            out.push(LIVE_RUNTIME_NOT_REQUESTED);
        }
        match ctx.daily {
            None => out.push(BUY_LIMIT_UNREADABLE),
            Some(d) if d.remaining() == 0 => out.push(BUY_LIMIT_EXHAUSTED),
            Some(_) => {}
        }
        match ctx.total {
            None => out.push(ATTEMPT_TOTAL_UNREADABLE),
            Some(t) if t.remaining() == 0 => out.push(ATTEMPT_TOTAL_EXHAUSTED),
            Some(t) => {
                if let (Some(g), Some(by)) = (grant, &self.live_set_by)
                    && t.used_by_grant(by.generation) >= u64::from(g.max_attempts)
                {
                    out.push(LIVE_GRANT_ATTEMPTS_EXHAUSTED);
                }
            }
        }
        match ctx.spent_wei {
            None => out.push(LIVE_SPEND_UNREADABLE),
            Some(spent) => {
                if let Some(g) = grant
                    && spent.saturating_add(g.max_input_wei) > g.max_spend_wei
                {
                    out.push(LIVE_GRANT_SPEND_EXHAUSTED);
                }
            }
        }
        if grant.is_some_and(|g| g.exit_only) {
            out.push(LIVE_GRANT_EXIT_ONLY);
        }
        if grant.is_some_and(|g| g.risk.is_none()) {
            out.push(LIVE_GRANT_RISK_MISSING);
        }
        if grant.is_some_and(|g| g.limits.is_none()) {
            out.push(LIVE_GRANT_LIMITS_MISSING);
        }
        if let Some(g) = grant {
            let q = ctx.qualification;
            if !q.venues.contains(&g.venue) {
                out.push(LIVE_VENUE_UNQUALIFIED);
            }
            if g.sources.iter().any(|s| !q.sources.contains(s)) {
                out.push(LIVE_SOURCE_UNQUALIFIED);
            }
            if g.adapters.iter().any(|a| !q.adapters.contains(a)) {
                out.push(LIVE_ADAPTER_UNQUALIFIED);
            }
            if !q.acquisition {
                out.push(LIVE_ACQUISITION_UNPROVEN);
            } else if g
                .adapters
                .iter()
                .any(|a| !q.acquisition_adapters.contains(a))
            {
                out.push(LIVE_ACQUISITION_BUILDER_UNPROVEN);
            }
            if !q.risk_decision {
                out.push(LIVE_RISK_DECISION_MISSING);
            } else if q.risk_scope_sha256 != Some(g.risk_scope_sha256()) {
                out.push(LIVE_RISK_DECISION_OTHER_SCOPE);
            }
        }
        out
    }

    /// The effective mode of any entity as it holds now: a requested LIVE
    /// is `LIVE` only when the gate is clear ([`Generation::live_blockers`])
    /// and the grant's scope names the entity; otherwise `BLOCKED` with the
    /// first reason. Four has no execution path whatever is granted. Every
    /// other request reads as [`Generation::effective_now`].
    pub fn effective_live_now(&self, entity: Entity, ctx: &LiveContext<'_>) -> Effective {
        self.live_effective(entity, ctx, false)
    }

    /// Every condition of the LIVE gate with the reason it misses now (none:
    /// it holds), in the order of [`LIVE_CONDITIONS`]. Without a usable
    /// grant the scope conditions name the grant's reason.
    pub fn live_checklist(
        &self,
        ctx: &LiveContext<'_>,
    ) -> Vec<(&'static str, Option<&'static str>)> {
        let blockers = self.live_blockers(ctx);
        let grant = LIVE_CONDITIONS[1].1;
        let no_grant = blockers.iter().find(|b| grant.contains(b)).copied();
        LIVE_CONDITIONS
            .iter()
            .enumerate()
            .map(|(i, (id, reasons))| {
                let missed = blockers.iter().find(|b| reasons.contains(b)).copied();
                let missed = missed.or(no_grant.filter(|_| i >= SCOPE_CONDITIONS_FROM));
                (*id, missed)
            })
            .collect()
    }

    /// [`Generation::live_blockers`] for an exit (approval or SELL) of a
    /// position the grant opened: without the BUY attempt reasons.
    pub fn exit_blockers(&self, ctx: &LiveContext<'_>) -> Vec<&'static str> {
        let mut out = self.live_blockers(ctx);
        out.retain(|r| !EXIT_EXEMPT.contains(r));
        out
    }

    /// [`Generation::effective_live_now`] for an exit: LIVE when
    /// [`Generation::exit_blockers`] is clear and the scope names it.
    pub fn effective_exit_now(&self, entity: Entity, ctx: &LiveContext<'_>) -> Effective {
        self.live_effective(entity, ctx, true)
    }

    /// Every entity effective LIVE (for an exit: without the BUY attempt
    /// reasons), or the first entity's reason.
    fn live_clear(
        &self,
        entities: &[Entity],
        ctx: &LiveContext<'_>,
        exit: bool,
    ) -> Result<(), String> {
        for entity in entities {
            match self.live_effective(*entity, ctx, exit) {
                Effective::Live => {}
                Effective::Blocked(reason) => return Err(reason.into()),
                other => return Err(format!("{}_{}", entity.name(), other.name())),
            }
        }
        Ok(())
    }

    fn live_effective(&self, entity: Entity, ctx: &LiveContext<'_>, exit: bool) -> Effective {
        let stored = self.effective_now(entity, ctx.boot);
        if self.requested_of(entity) != Mode::Live {
            return stored;
        }
        if let Entity::Venue(VenueId::FourClassic | VenueId::OpenFour) = entity {
            return Effective::Blocked(FOUR_EXECUTION_PATH_MISSING);
        }
        let blockers = if exit {
            self.exit_blockers(ctx)
        } else {
            self.live_blockers(ctx)
        };
        if let Some(first) = blockers.first() {
            return Effective::Blocked(first);
        }
        match &self.live {
            Some(g) if g.names(entity) => Effective::Live,
            _ => Effective::Blocked(LIVE_NOT_IN_SCOPE),
        }
    }

    /// The effective state of a switch in the boot session `boot`: ENABLED
    /// only within the session it was written in, never across a restart.
    pub fn switch_now(&self, id: SwitchId, boot: Option<&str>) -> Effective {
        match self.switch(id).effective {
            Effective::Enabled if boot.is_none() || self.boot.as_deref() != boot => {
                Effective::Blocked(RESET_BY_RESTART)
            }
            other => other,
        }
    }

    /// A stored block reason as it reads in the boot session `boot`: a
    /// requested LIVE names the economic LIVE switch as it holds now, and an
    /// ENABLED of an earlier boot no longer does (review L5).
    pub fn live_reason_now<'a>(&self, reason: &'a str, boot: Option<&str>) -> &'a str {
        if reason == ECONOMIC_LIVE_NOT_AUTHORIZED
            && self.switch_now(SwitchId::Live, boot) != Effective::Enabled
        {
            ECONOMIC_LIVE_DISABLED
        } else {
            reason
        }
    }

    /// The effective mode of any entity in the boot session `boot`.
    pub fn effective_now(&self, entity: Entity, boot: Option<&str>) -> Effective {
        match (entity, self.effective_of(entity)) {
            (Entity::Switch(s), _) => self.switch_now(s, boot),
            (_, Effective::Blocked(r)) => Effective::Blocked(self.live_reason_now(r, boot)),
            (_, other) => other,
        }
    }

    fn live_gate(&self) -> LiveGate {
        if self.switch(SwitchId::Live).requested == Mode::Enabled {
            LiveGate::Enabled
        } else {
            LiveGate::Disabled
        }
    }

    /// Every effective mode from the requested ones (the LIVE reasons depend
    /// on the global economic LIVE switch).
    fn rederive(&mut self) {
        let live = self.live_gate();
        for s in &mut self.sources {
            s.effective = effective(s.source, s.requested, live);
        }
        for c in &mut self.components {
            c.effective = component_effective(c.component, c.requested, live);
        }
        for v in &mut self.venues {
            v.effective = venue_effective(v.venue, v.requested, live);
        }
        for s in &mut self.switches {
            s.effective = switch_effective(s.requested);
        }
        self.runtime.effective = runtime_effective(self.runtime.requested, live);
        self.buy_limit.effective = Entity::BuyLimit.effective(self.buy_limit.requested, live);
        self.buy_total.effective = Entity::BuyTotal.effective(self.buy_total.requested, live);
    }

    /// Grants LIVE for `grant`'s scope, or revokes it (`None`).
    fn set_live(&mut self, grant: Option<LiveGrant>, set_by: SetBy, presence: Option<[u8; 65]>) {
        self.live = grant;
        self.live_set_by = Some(set_by);
        self.live_presence = presence;
    }

    fn set(&mut self, entity: Entity, requested: Mode, set_by: SetBy) {
        match entity {
            Entity::Source(source) => {
                let at = source.index();
                self.sources[at].requested = requested;
            }
            Entity::Component(component) => {
                let at = component.index();
                self.components[at].requested = requested;
            }
            Entity::Venue(venue) => {
                let at = venue.index();
                self.venues[at].requested = requested;
                self.venues[at].set_by = Some(set_by);
            }
            Entity::Switch(switch) => {
                let at = switch.index();
                self.switches[at].requested = requested;
                self.switches[at].set_by = Some(set_by);
            }
            Entity::Runtime => {
                self.runtime.requested = requested;
                self.runtime.set_by = Some(set_by);
            }
            Entity::BuyLimit => {
                self.buy_limit.requested = requested;
                self.buy_limit.set_by = Some(set_by);
            }
            Entity::BuyTotal => {
                self.buy_total.requested = requested;
                self.buy_total.set_by = Some(set_by);
            }
            // A grant's scope comes only through `Store::grant_live`; a
            // request here revokes.
            Entity::LiveGrant => self.set_live(None, set_by, None),
        }
        self.rederive();
    }

    /// Whether any switch is requested ENABLED or a LIVE grant is held.
    pub fn any_enabled(&self) -> bool {
        self.switches.iter().any(|s| s.requested == Mode::Enabled) || self.live.is_some()
    }

    /// Every ENABLED switch DISABLED and the LIVE grant revoked by the
    /// restart reset of `generation`.
    fn reset_switches(&mut self, generation: u64) {
        let reset = SetBy {
            request_id: RESTART_RESET.into(),
            generation,
        };
        for s in &mut self.switches {
            if s.requested == Mode::Enabled {
                s.requested = Mode::Disabled;
                s.set_by = Some(reset.clone());
            }
        }
        if self.live.is_some() {
            self.set_live(None, reset, None);
        }
        self.rederive();
    }

    fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&sm::SCHEMA);
        d.set(sm::GENERATION, self.generation)
            .set(sm::PROVENANCE_TOOL, "bsc_runtime")
            .set(sm::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms);
        for s in &self.sources {
            let mut e = Draft::new(&sm::entry::SCHEMA);
            e.set(sm::entry::SOURCE_ID, s.source.name())
                .set(sm::entry::REQUESTED, s.requested.name())
                .set(sm::entry::EFFECTIVE, s.effective.name());
            if let Some(reason) = s.effective.reason() {
                e.set(sm::entry::BLOCK_REASON, reason);
            }
            d.push(sm::SOURCES, e.into_nested());
        }
        for c in &self.components {
            use sm::component as k;
            let mut e = Draft::new(&k::SCHEMA);
            e.set(k::COMPONENT_ID, c.component.name())
                .set(k::REQUESTED, c.requested.name())
                .set(k::EFFECTIVE, c.effective.name());
            if let Some(reason) = c.effective.reason() {
                e.set(k::BLOCK_REASON, reason);
            }
            d.push(sm::COMPONENTS, e.into_nested());
        }
        for v in &self.venues {
            use sm::venue as k;
            let mut e = Draft::new(&k::SCHEMA);
            e.set(k::VENUE_ID, v.venue.name())
                .set(k::REQUESTED, v.requested.name())
                .set(k::EFFECTIVE, v.effective.name());
            if let Some(reason) = v.effective.reason() {
                e.set(k::BLOCK_REASON, reason);
            }
            if let Some(by) = &v.set_by {
                e.set(k::REQUEST_ID, by.request_id.as_str())
                    .set(k::SET_GENERATION, by.generation);
            }
            d.push(sm::VENUES, e.into_nested());
        }
        for s in &self.switches {
            use sm::switch as k;
            let mut e = Draft::new(&k::SCHEMA);
            e.set(k::SWITCH_ID, s.switch.name())
                .set(k::REQUESTED, s.requested.name())
                .set(k::EFFECTIVE, s.effective.name());
            if let Some(reason) = s.effective.reason() {
                e.set(k::BLOCK_REASON, reason);
            }
            if let Some(by) = &s.set_by {
                e.set(k::REQUEST_ID, by.request_id.as_str())
                    .set(k::SET_GENERATION, by.generation);
            }
            d.push(sm::SWITCHES, e.into_nested());
        }
        for (id, setting) in [
            (RUNTIME_MODE, &self.runtime),
            (BUY_ATTEMPTS_PER_DAY, &self.buy_limit),
            (BUY_ATTEMPTS_TOTAL, &self.buy_total),
        ] {
            use sm::setting as k;
            let mut e = Draft::new(&k::SCHEMA);
            e.set(k::SETTING_ID, id)
                .set(k::REQUESTED, setting.requested.name())
                .set(k::EFFECTIVE, setting.effective.name());
            if let Some(reason) = setting.effective.reason() {
                e.set(k::BLOCK_REASON, reason);
            }
            if let Some(by) = &setting.set_by {
                e.set(k::REQUEST_ID, by.request_id.as_str())
                    .set(k::SET_GENERATION, by.generation);
            }
            d.push(sm::SETTINGS, e.into_nested());
        }
        if let Some(by) = &self.live_set_by {
            use sm::live as k;
            let mut e = Draft::new(&k::SCHEMA);
            e.set(k::REQUEST_ID, by.request_id.as_str())
                .set(k::SET_GENERATION, by.generation);
            match &self.live {
                None => {
                    e.set(k::STATE, "REVOKED");
                }
                Some(g) => {
                    e.set(k::STATE, "GRANTED")
                        .set(k::VENUE, g.venue.name())
                        .set(k::STRATEGIES, g.strategies.name())
                        .set(
                            k::MAX_INPUT_WEI,
                            darkbot_bsc_record::Value::u128(g.max_input_wei),
                        )
                        .set(
                            k::MAX_SPEND_WEI,
                            darkbot_bsc_record::Value::u128(g.max_spend_wei),
                        )
                        .set(k::MAX_ATTEMPTS, u64::from(g.max_attempts))
                        .set(k::EXPIRES_UNIX_MS, g.expires_unix_ms)
                        .set(
                            k::SCOPE_SHA256,
                            darkbot_bsc_record::Value::B256(g.scope_sha256()),
                        )
                        .set_opt(k::EXIT_ONLY, g.exit_only.then_some(true));
                    if let Some(sig) = &self.live_presence {
                        e.set(
                            k::PRESENCE_SIGNATURE,
                            darkbot_bsc_record::Value::Bytes(sig.to_vec()),
                        );
                    }
                    if let Some(r) = &g.risk {
                        e.set(k::MAX_BUY_SLIPPAGE_BPS, u64::from(r.max_buy_slippage_bps))
                            .set(k::MAX_EXIT_SLIPPAGE_BPS, u64::from(r.max_exit_slippage_bps))
                            .set(k::MAX_GAS_PRICE_WEI, r.max_gas_price_wei)
                            .set(k::MAX_BUY_GAS_LIMIT, r.max_buy_gas_limit)
                            .set(k::MAX_APPROVE_GAS_LIMIT, r.max_approve_gas_limit)
                            .set(k::MAX_SELL_GAS_LIMIT, r.max_sell_gas_limit)
                            .set_opt(k::RISK_OUTLIER_ACK, r.outlier_ack.then_some(true));
                    }
                    if let Some(l) = &g.limits {
                        e.set(k::MAX_OPEN_POSITIONS, u64::from(l.max_open_positions))
                            .set(
                                k::MAX_REALIZED_LOSS_WEI,
                                darkbot_bsc_record::Value::u128(l.max_realized_loss_wei),
                            )
                            .set(
                                k::MAX_DAILY_REALIZED_LOSS_WEI,
                                darkbot_bsc_record::Value::u128(l.max_daily_realized_loss_wei),
                            )
                            .set(k::MIN_BUY_INTERVAL_MS, l.min_buy_interval_ms);
                    }
                    for s in &g.sources {
                        e.push(k::SOURCES, s.name());
                    }
                    for a in &g.adapters {
                        e.push(k::ADAPTERS, a.name());
                    }
                }
            }
            d.set(sm::LIVE, e.into_nested());
        }
        if let Some(id) = &self.request_id {
            d.set(sm::REQUEST_ID, id.as_str());
        }
        if let Some(boot) = &self.boot {
            d.set(
                sm::BOOT_SESSION,
                darkbot_bsc_record::Value::Bytes(boot.as_bytes().to_vec()),
            );
        }
        d.encode().map_err(|e| e.to_string())
    }

    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let r = decode_generation(bytes)?;
        let e = |x: darkbot_bsc_record::RecordError| x.0;
        let version = r.schema().schema_version;
        let mut g = Generation::initial();
        g.generation = r.u64(sm::GENERATION).map_err(e)?;
        g.request_id = r.opt_text(sm::REQUEST_ID).map_err(e)?.map(str::to_string);
        // The switches first: the LIVE reasons of every other entry depend
        // on the global economic LIVE switch. A generation from before the
        // switches existed has them DISABLED and its LIVE reason unchanged.
        let live = if version >= 3 {
            let mut seen: Vec<SwitchId> = Vec::new();
            for row in r.nested_all(sm::SWITCHES).map_err(e)? {
                use sm::switch as k;
                let switch = SwitchId::parse(row.text(k::SWITCH_ID).map_err(e)?)
                    .ok_or("unknown switch in a generation")?;
                if seen.contains(&switch) {
                    return Err(format!("{} appears twice", switch.name()));
                }
                seen.push(switch);
                let requested = Mode::parse(row.text(k::REQUESTED).map_err(e)?)
                    .filter(|m| Entity::Switch(switch).accepts(*m))
                    .ok_or("unknown requested switch state in a generation")?;
                let stored = Effective::parse(
                    row.text(k::EFFECTIVE).map_err(e)?,
                    row.opt_text(k::BLOCK_REASON).map_err(e)?,
                );
                if stored != Some(switch_effective(requested)) {
                    return Err(format!(
                        "{}: stored effective state is not the owner's",
                        switch.name()
                    ));
                }
                let at = switch.index();
                g.switches[at] = SwitchMode {
                    switch,
                    requested,
                    effective: switch_effective(requested),
                    set_by: set_by(&row, k::REQUEST_ID, k::SET_GENERATION)?,
                };
            }
            g.boot = r
                .opt_bytes(sm::BOOT_SESSION)
                .map_err(e)?
                .map(|b| String::from_utf8(b.to_vec()))
                .transpose()
                .map_err(|_| "the boot session is not text")?;
            g.live_gate()
        } else {
            LiveGate::Legacy
        };
        let rows = r.nested_all(sm::SOURCES).map_err(e)?;
        if rows.len() != 5 {
            return Err("a generation names exactly the five sources".into());
        }
        for row in rows {
            let source = SourceId::parse(row.text(sm::entry::SOURCE_ID).map_err(e)?)
                .ok_or("unknown source in a generation")?;
            let requested = Mode::parse(row.text(sm::entry::REQUESTED).map_err(e)?)
                .filter(|m| Entity::Source(source).accepts(*m))
                .ok_or("unknown requested mode in a generation")?;
            let derived = effective(source, requested, live);
            // The stored effective mode must be this owner's rule, never more.
            if row.text(sm::entry::EFFECTIVE).map_err(e)? != derived.name()
                || row.opt_text(sm::entry::BLOCK_REASON).map_err(e)? != derived.reason()
            {
                return Err(format!(
                    "{}: stored effective mode is not the owner's",
                    source.name()
                ));
            }
            let at = source.index();
            g.sources[at] = SourceMode {
                source,
                requested,
                effective: derived,
            };
        }
        // A generation from before a component existed has it OFF.
        let mut seen: Vec<ComponentId> = Vec::new();
        let rows = if version >= 2 {
            r.nested_all(sm::COMPONENTS).map_err(e)?
        } else {
            Vec::new()
        };
        for row in rows {
            use sm::component as k;
            let component = ComponentId::parse(row.text(k::COMPONENT_ID).map_err(e)?)
                .ok_or("unknown component in a generation")?;
            if seen.contains(&component) {
                return Err(format!("{} appears twice", component.name()));
            }
            seen.push(component);
            let requested = Mode::parse(row.text(k::REQUESTED).map_err(e)?)
                .filter(|m| Entity::Component(component).accepts(*m))
                .ok_or("unknown requested mode in a generation")?;
            let derived = component_effective(component, requested, live);
            if row.text(k::EFFECTIVE).map_err(e)? != derived.name()
                || row.opt_text(k::BLOCK_REASON).map_err(e)? != derived.reason()
            {
                return Err(format!(
                    "{}: stored effective mode is not the owner's",
                    component.name()
                ));
            }
            let at = component.index();
            g.components[at] = ComponentMode {
                component,
                requested,
                effective: derived,
            };
        }
        // A generation from before the venues existed has every venue OFF.
        let mut seen: Vec<VenueId> = Vec::new();
        let rows = if version >= 3 {
            r.nested_all(sm::VENUES).map_err(e)?
        } else {
            Vec::new()
        };
        for row in rows {
            use sm::venue as k;
            let venue = VenueId::parse(row.text(k::VENUE_ID).map_err(e)?)
                .ok_or("unknown venue in a generation")?;
            if seen.contains(&venue) {
                return Err(format!("{} appears twice", venue.name()));
            }
            seen.push(venue);
            let requested = Mode::parse(row.text(k::REQUESTED).map_err(e)?)
                .filter(|m| Entity::Venue(venue).accepts(*m))
                .ok_or("unknown requested mode in a generation")?;
            let derived = venue_effective(venue, requested, live);
            if row.text(k::EFFECTIVE).map_err(e)? != derived.name()
                || row.opt_text(k::BLOCK_REASON).map_err(e)? != derived.reason()
            {
                return Err(format!(
                    "{}: stored effective mode is not the owner's",
                    venue.name()
                ));
            }
            let at = venue.index();
            g.venues[at] = VenueMode {
                venue,
                requested,
                effective: derived,
                set_by: set_by(&row, k::REQUEST_ID, k::SET_GENERATION)?,
            };
        }
        // The runtime mode and the daily BUY limit; a generation from
        // before them has the runtime OFF and the limit 0.
        let rows = if version >= 4 {
            r.nested_all(sm::SETTINGS).map_err(e)?
        } else {
            Vec::new()
        };
        let mut seen: Vec<Entity> = Vec::new();
        for row in rows {
            use sm::setting as k;
            let entity = match row.text(k::SETTING_ID).map_err(e)? {
                RUNTIME_MODE => Entity::Runtime,
                BUY_ATTEMPTS_PER_DAY => Entity::BuyLimit,
                BUY_ATTEMPTS_TOTAL if version >= 5 => Entity::BuyTotal,
                _ => return Err("unknown setting in a generation".into()),
            };
            if seen.contains(&entity) {
                return Err(format!("{} appears twice", entity.name()));
            }
            seen.push(entity);
            let requested = Mode::parse(row.text(k::REQUESTED).map_err(e)?)
                .filter(|m| entity.accepts(*m))
                .ok_or("unknown requested setting in a generation")?;
            let derived = entity.effective(requested, live);
            if row.text(k::EFFECTIVE).map_err(e)? != derived.name()
                || row.opt_text(k::BLOCK_REASON).map_err(e)? != derived.reason()
            {
                return Err(format!(
                    "{}: stored effective setting is not the owner's",
                    entity.name()
                ));
            }
            let setting = Setting {
                requested,
                effective: derived,
                set_by: set_by(&row, k::REQUEST_ID, k::SET_GENERATION)?,
            };
            match entity {
                Entity::Runtime => g.runtime = setting,
                Entity::BuyTotal => g.buy_total = setting,
                _ => g.buy_limit = setting,
            }
        }
        // The LIVE grant (version 5): granted with its whole scope, or
        // revoked with none of it; anything else is not this owner's.
        if version >= 5
            && let Some(row) = r.opt_nested(sm::LIVE).map_err(e)?
        {
            use sm::live as k;
            let by = set_by(&row, k::REQUEST_ID, k::SET_GENERATION)?
                .ok_or("a LIVE grant names the request that set it")?;
            let scope = (
                row.opt_text(k::VENUE).map_err(e)?,
                row.opt_text(k::STRATEGIES).map_err(e)?,
                row.opt_u256(k::MAX_INPUT_WEI).map_err(e)?,
                row.opt_u256(k::MAX_SPEND_WEI).map_err(e)?,
                row.opt_u64(k::MAX_ATTEMPTS).map_err(e)?,
                row.opt_u64(k::EXPIRES_UNIX_MS).map_err(e)?,
                row.opt_b256(k::SCOPE_SHA256).map_err(e)?,
            );
            let sources = row.texts(k::SOURCES).map_err(e)?;
            let adapters = row.texts(k::ADAPTERS).map_err(e)?;
            let grant = match (row.text(k::STATE).map_err(e)?, scope) {
                ("REVOKED", (None, None, None, None, None, None, None))
                    if sources.is_empty() && adapters.is_empty() =>
                {
                    None
                }
                (
                    "GRANTED",
                    (
                        Some(venue),
                        Some(strategies),
                        Some(max_input),
                        Some(max_spend),
                        Some(max_attempts),
                        Some(expires),
                        Some(digest),
                    ),
                ) => {
                    let wei = |word: [u8; 32]| -> Result<u128, String> {
                        if word[..16].iter().any(|b| *b != 0) {
                            return Err("a LIVE limit exceeds 128 bits".into());
                        }
                        Ok(u128::from_be_bytes(
                            word[16..].try_into().unwrap_or([0; 16]),
                        ))
                    };
                    let grant = LiveGrant {
                        venue: VenueId::parse(venue).ok_or("unknown LIVE venue")?,
                        strategies: LiveStrategies::parse(strategies)
                            .ok_or("unknown LIVE strategies")?,
                        sources: sources
                            .iter()
                            .map(|s| SourceId::parse(s).ok_or("unknown LIVE source"))
                            .collect::<Result<_, _>>()?,
                        adapters: adapters
                            .iter()
                            .map(|a| LiveAdapter::parse(a).ok_or("unknown LIVE adapter"))
                            .collect::<Result<_, _>>()?,
                        max_input_wei: wei(max_input)?,
                        max_spend_wei: wei(max_spend)?,
                        max_attempts: u32::try_from(max_attempts)
                            .map_err(|_| "LIVE max_attempts out of range")?,
                        expires_unix_ms: expires,
                        exit_only: row.opt_bool(k::EXIT_ONLY).map_err(e)?.unwrap_or(false),
                        risk: grant_risk_of(&row)?,
                        limits: grant_limits_of(&row)?,
                    };
                    grant.validate()?;
                    if grant.scope_sha256() != digest {
                        return Err(
                            "the LIVE grant's scope is not the one it was granted for".into()
                        );
                    }
                    Some(grant)
                }
                _ => return Err("a LIVE grant is either the whole scope or a revocation".into()),
            };
            // R9 (BSC3-001): a presence signature only with a grant, 65 bytes.
            g.live_presence = match (row.opt_bytes(k::PRESENCE_SIGNATURE).map_err(e)?, &grant) {
                (None, _) => None,
                (Some(sig), Some(_)) => Some(
                    sig.try_into()
                        .map_err(|_| "a presence signature is 65 bytes")?,
                ),
                (Some(_), None) => return Err("a revocation carries no presence signature".into()),
            };
            g.live = grant;
            g.live_set_by = Some(by);
        }
        Ok(g)
    }
}

/// The optional (request, generation) of a venue or switch row: both or none.
fn set_by(row: &Record, request: u16, generation: u16) -> Result<Option<SetBy>, String> {
    let e = |x: darkbot_bsc_record::RecordError| x.0;
    match (
        row.opt_text(request).map_err(e)?,
        row.opt_u64(generation).map_err(e)?,
    ) {
        (Some(request_id), Some(generation)) => Ok(Some(SetBy {
            request_id: request_id.into(),
            generation,
        })),
        (None, None) => Ok(None),
        _ => Err("a request and its generation are recorded together".into()),
    }
}

/// A generation record of the current schema, or one written before the
/// runtime mode and BUY limit (version 3), the venues and switches
/// (version 2) or the components (version 1) existed.
fn decode_generation(bytes: &[u8]) -> Result<Record, String> {
    decode(&sm::SCHEMA, bytes)
        .or_else(|current| {
            decode(&sm::SCHEMA_V4, bytes)
                .or_else(|_| decode(&sm::SCHEMA_V3, bytes))
                .or_else(|_| decode(&sm::SCHEMA_V2, bytes))
                .or_else(|_| decode(&sm::SCHEMA_V1, bytes))
                .map_err(|_| current)
        })
        .map_err(|e| e.to_string())
}

/// The outcome of one request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub request_id: String,
    /// The source, component, venue or switch requested.
    pub entity: Entity,
    pub requested: Mode,
    pub expected_generation: u64,
    /// `APPLIED`, `REJECTED_STALE`, or `UNRESOLVED` (its receipt lost and
    /// the generation it would have written no longer kept).
    pub outcome: &'static str,
    pub generation: u64,
    pub effective: Effective,
}

impl Receipt {
    pub fn applied(&self) -> bool {
        self.outcome == "APPLIED"
    }

    /// One line for the controller and the gateway's receipt detail.
    pub fn line(&self) -> String {
        format!(
            "outcome={} source={} requested={} effective={}{} generation={} request_id={}",
            self.outcome,
            self.entity.name(),
            self.requested.name(),
            self.effective.name(),
            self.effective
                .reason()
                .map(|r| format!(" block_reason={r}"))
                .unwrap_or_default(),
            self.generation,
            self.request_id
        )
    }

    fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&rc::SCHEMA);
        d.set(rc::REQUEST_ID, self.request_id.as_str())
            .set(rc::SOURCE_ID, self.entity.name())
            .set(rc::REQUESTED, self.requested.name())
            .set(rc::EXPECTED_GENERATION, self.expected_generation)
            .set(rc::OUTCOME, self.outcome)
            .set(rc::GENERATION, self.generation)
            .set(rc::EFFECTIVE, self.effective.name())
            .set(rc::OBSERVED_UNIX_MS, observed_unix_ms);
        if let Some(reason) = self.effective.reason() {
            d.set(rc::BLOCK_REASON, reason);
        }
        d.encode().map_err(|e| e.to_string())
    }

    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let r = decode(&rc::SCHEMA, bytes).map_err(|e| e.to_string())?;
        let e = |x: darkbot_bsc_record::RecordError| x.0;
        let entity = Entity::parse(r.text(rc::SOURCE_ID).map_err(e)?).ok_or("unknown source")?;
        let requested = Mode::parse(r.text(rc::REQUESTED).map_err(e)?)
            .filter(|m| entity.accepts(*m))
            .ok_or("unknown mode")?;
        let outcome = match r.text(rc::OUTCOME).map_err(e)? {
            "APPLIED" => "APPLIED",
            "REJECTED_STALE" => "REJECTED_STALE",
            "UNRESOLVED" => "UNRESOLVED",
            _ => return Err("unknown receipt outcome".into()),
        };
        // The effective mode as recorded then (its LIVE reason depended on
        // the generation the request met).
        let effective = Effective::parse(
            r.text(rc::EFFECTIVE).map_err(e)?,
            r.opt_text(rc::BLOCK_REASON).map_err(e)?,
        )
        .ok_or("unknown effective mode in a receipt")?;
        Ok(Receipt {
            request_id: r.text(rc::REQUEST_ID).map_err(e)?.to_string(),
            entity,
            requested,
            expected_generation: r.u64(rc::EXPECTED_GENERATION).map_err(e)?,
            outcome,
            generation: r.u64(rc::GENERATION).map_err(e)?,
            effective,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests;

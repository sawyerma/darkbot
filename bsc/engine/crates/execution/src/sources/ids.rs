//! The switchable entities of the native owner (sources, runtime components,
//! venues, wallet and economic switches) and the effective mode each takes.

use super::*;

/// The five independently switchable inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceId {
    Geth,
    PublicNode,
    Drpc,
    BlockReq,
    Club48,
}

/// REV.1.0 R6 (PHASENPLAN §5a.2): a source's capability class, by design
/// ([`SourceId::capability`]) or as observed ([`crate::ingress::observed_capability`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceCapability {
    /// Verified full signed transactions: a C3 BUY's signal.
    Full,
    /// Only transaction hashes so far: never a signal.
    HashOnly,
    /// 48Club's auction events: never a signal.
    Qpt,
    /// Nothing delivered yet.
    Unknown,
}

impl SourceCapability {
    pub const fn name(self) -> &'static str {
        match self {
            SourceCapability::Full => "FULL",
            SourceCapability::HashOnly => "HASH_ONLY",
            SourceCapability::Qpt => "QPT",
            SourceCapability::Unknown => "UNKNOWN",
        }
    }
}

impl SourceId {
    /// Its place in [`SourceId::ALL`] (BSC3-002): a fixed index per value,
    /// the match exhaustive, so a new value needs its own; one left out of
    /// `ALL` is out of every array's bounds (fail-closed), never index 0.
    pub const fn index(self) -> usize {
        match self {
            SourceId::Geth => 0,
            SourceId::PublicNode => 1,
            SourceId::Drpc => 2,
            SourceId::BlockReq => 3,
            SourceId::Club48 => 4,
        }
    }

    /// REV.1.0 R4 (PHASENPLAN §5a.2, §20): a public WSS endpoint (anyone's
    /// view of the mempool), never a production BUY's only signal.
    pub const fn public(self) -> bool {
        matches!(
            self,
            SourceId::PublicNode | SourceId::Drpc | SourceId::BlockReq
        )
    }

    /// REV.1.0 R6 (PHASENPLAN §5a.2): what this source's adapter delivers
    /// by design: the WSS endpoints and the local geth's txpool lanes full
    /// signed transactions (a hash-only frame counted, never acted on),
    /// 48Club its auctions (QPT: no transaction to act on).
    pub const fn capability(self) -> SourceCapability {
        match self {
            SourceId::Geth | SourceId::PublicNode | SourceId::Drpc | SourceId::BlockReq => {
                SourceCapability::Full
            }
            SourceId::Club48 => SourceCapability::Qpt,
        }
    }

    pub const ALL: [SourceId; 5] = [
        SourceId::Geth,
        SourceId::PublicNode,
        SourceId::Drpc,
        SourceId::BlockReq,
        SourceId::Club48,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SourceId::Geth => "BSC_INGRESS_GETH",
            SourceId::PublicNode => "BSC_INGRESS_PUBLICNODE",
            SourceId::Drpc => "BSC_INGRESS_DRPC",
            SourceId::BlockReq => "BSC_INGRESS_BLOCKREQ",
            SourceId::Club48 => "BSC_INGRESS_48CLUB",
        }
    }

    pub fn parse(name: &str) -> Option<SourceId> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// What the user requests: `OFF | SHADOW | LIVE` for a source, component
/// or venue, `OFF | SHADOW | PAPER | LIVE` for the runtime mode,
/// `DISABLED | ENABLED` for a switch, a whole number for the daily BUY
/// attempt limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    Shadow,
    Paper,
    Live,
    Disabled,
    Enabled,
    /// A BUY attempt limit: per UTC day, or for the whole campaign.
    PerDay(u32),
    /// A LIVE grant of the scope whose canonical text has this SHA-256.
    Granted([u8; 32]),
}

impl Mode {
    pub fn name(self) -> String {
        match self {
            Mode::Off => "OFF".into(),
            Mode::Shadow => "SHADOW".into(),
            Mode::Paper => "PAPER".into(),
            Mode::Live => "LIVE".into(),
            Mode::Disabled => "DISABLED".into(),
            Mode::Enabled => "ENABLED".into(),
            Mode::PerDay(n) => n.to_string(),
            Mode::Granted(digest) => format!("GRANTED:{}", hex32(&digest)),
        }
    }

    /// A mode name, a plain decimal (no sign, no leading zero) for a limit,
    /// or `GRANTED:<64 lowercase hex>` for a LIVE grant.
    pub fn parse(name: &str) -> Option<Mode> {
        if !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
            return (name == "0" || !name.starts_with('0'))
                .then(|| name.parse::<u32>().ok().map(Mode::PerDay))
                .flatten();
        }
        if let Some(digest) = name.strip_prefix("GRANTED:") {
            return parse_hex32(digest).map(Mode::Granted);
        }
        [
            Mode::Off,
            Mode::Shadow,
            Mode::Paper,
            Mode::Live,
            Mode::Disabled,
            Mode::Enabled,
        ]
        .into_iter()
        .find(|m| m.name() == name)
    }
}

/// What the owner makes of a request. `Live` is never stored: it is what a
/// requested LIVE is now, under the LIVE gate
/// ([`Generation::effective_live_now`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effective {
    Off,
    Shadow,
    /// The runtime mode PAPER: the C3 path to the PAPER sink may run.
    Paper,
    /// Economically live under the operator's LIVE grant.
    Live,
    Blocked(&'static str),
    Disabled,
    Enabled,
    /// A BUY attempt limit in force.
    PerDay(u32),
}

impl Effective {
    pub fn name(self) -> String {
        match self {
            Effective::Off => "OFF".into(),
            Effective::Shadow => "SHADOW".into(),
            Effective::Paper => "PAPER".into(),
            Effective::Live => "LIVE".into(),
            Effective::Blocked(_) => "BLOCKED".into(),
            Effective::Disabled => "DISABLED".into(),
            Effective::Enabled => "ENABLED".into(),
            Effective::PerDay(n) => n.to_string(),
        }
    }

    pub fn reason(self) -> Option<&'static str> {
        match self {
            Effective::Blocked(reason) => Some(reason),
            _ => None,
        }
    }

    /// A stored effective mode and its reason, if this owner can write it.
    pub(super) fn parse(name: &str, reason: Option<&str>) -> Option<Effective> {
        match (name, reason) {
            ("OFF", None) => Some(Effective::Off),
            ("SHADOW", None) => Some(Effective::Shadow),
            ("PAPER", None) => Some(Effective::Paper),
            ("DISABLED", None) => Some(Effective::Disabled),
            ("ENABLED", None) => Some(Effective::Enabled),
            ("BLOCKED", Some(r)) => BLOCK_REASONS
                .into_iter()
                .find(|k| *k == r)
                .map(Effective::Blocked),
            (n, None) => match Mode::parse(n) {
                Some(Mode::PerDay(n)) => Some(Effective::PerDay(n)),
                _ => None,
            },
            _ => None,
        }
    }
}

/// What a requested LIVE meets: the global economic LIVE switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveGate {
    /// `BSC_ECON_LIVE` is not ENABLED.
    Disabled,
    /// `BSC_ECON_LIVE` is ENABLED; no economic LIVE executor is authorized.
    Enabled,
    /// A generation written before the switch existed (read only).
    Legacy,
}

impl LiveGate {
    fn blocked(self) -> Effective {
        match self {
            LiveGate::Disabled => Effective::Blocked(ECONOMIC_LIVE_DISABLED),
            LiveGate::Enabled | LiveGate::Legacy => {
                Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED)
            }
        }
    }
}

/// The owner's rule. 48Club's auction feed needs credentials and an
/// eligibility nobody proved, so it is BLOCKED even in SHADOW (plan §5a.2).
pub fn effective(source: SourceId, requested: Mode, live: LiveGate) -> Effective {
    match (source, requested) {
        (_, Mode::Live) => live.blocked(),
        (SourceId::Club48, Mode::Shadow) => Effective::Blocked(SOURCE_ACCESS_UNQUALIFIED),
        (_, Mode::Shadow) => Effective::Shadow,
        (
            _,
            Mode::Off
            | Mode::Paper
            | Mode::Disabled
            | Mode::Enabled
            | Mode::PerDay(_)
            | Mode::Granted(_),
        ) => Effective::Off,
    }
}

/// The switchable runtime components besides the sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentId {
    /// The C2 canonical/pending state owner (observes in SHADOW).
    StateC2,
    /// The C3 execution path: SHADOW runs it to the PAPER sink.
    ExecC3,
    /// Phase H: Strategy C on canonical Flap events; SHADOW runs it as a
    /// PAPER ledger (no signature, no send).
    StrategyC,
}

impl ComponentId {
    /// Its place in [`ComponentId::ALL`] (BSC3-002): a fixed index per value,
    /// the match exhaustive, so a new value needs its own; one left out of
    /// `ALL` is out of every array's bounds (fail-closed), never index 0.
    pub const fn index(self) -> usize {
        match self {
            ComponentId::StateC2 => 0,
            ComponentId::ExecC3 => 1,
            ComponentId::StrategyC => 2,
        }
    }

    pub const ALL: [ComponentId; 3] = [
        ComponentId::StateC2,
        ComponentId::ExecC3,
        ComponentId::StrategyC,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ComponentId::StateC2 => "BSC_STATE_C2",
            ComponentId::ExecC3 => "BSC_EXEC",
            ComponentId::StrategyC => "BSC_STRATEGY_C",
        }
    }

    pub fn parse(name: &str) -> Option<ComponentId> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }
}

/// The owner's rule for a component: SHADOW runs it without any economic
/// effect; LIVE (operational use of its state) is never effective here.
pub fn component_effective(component: ComponentId, requested: Mode, live: LiveGate) -> Effective {
    match (component, requested) {
        (_, Mode::Shadow) => Effective::Shadow,
        (ComponentId::StateC2, Mode::Live) => Effective::Blocked(C2_OPERATIONAL_NOT_AUTHORIZED),
        // LIVE would sign and send economically (C3) or trade on the
        // strategy's decisions: never without its own authorization
        // (G0.7/G0.8, J).
        (_, Mode::Live) => live.blocked(),
        (
            _,
            Mode::Off
            | Mode::Paper
            | Mode::Disabled
            | Mode::Enabled
            | Mode::PerDay(_)
            | Mode::Granted(_),
        ) => Effective::Off,
    }
}

/// The venues (plan v9 §2.3), each admitted independently: a candidate is
/// evaluated only where its venue is effective SHADOW.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VenueId {
    Flap,
    FourClassic,
    OpenFour,
}

impl VenueId {
    /// Its place in [`VenueId::ALL`] (BSC3-002): a fixed index per value,
    /// the match exhaustive, so a new value needs its own; one left out of
    /// `ALL` is out of every array's bounds (fail-closed), never index 0.
    pub const fn index(self) -> usize {
        match self {
            VenueId::Flap => 0,
            VenueId::FourClassic => 1,
            VenueId::OpenFour => 2,
        }
    }

    pub const ALL: [VenueId; 3] = [VenueId::Flap, VenueId::FourClassic, VenueId::OpenFour];

    pub fn name(self) -> &'static str {
        match self {
            VenueId::Flap => "BSC_VENUE_FLAP",
            VenueId::FourClassic => "BSC_VENUE_FOUR_CLASSIC",
            VenueId::OpenFour => "BSC_VENUE_OPENFOUR",
        }
    }

    pub fn parse(name: &str) -> Option<VenueId> {
        Self::ALL.into_iter().find(|v| v.name() == name)
    }
}

/// The owner's rule for a venue: SHADOW admits Flap launches to the C3
/// PAPER path and Strategy C, Four.meme Classic (R12c) and OpenFour (R12d,
/// its executed bonding preset) launches to the C3 PAPER path and (SC4)
/// Strategy C's ledger (their PAPER intents not yet); a requested
/// LIVE of a Four venue is blocked for its missing LIVE path, whatever the
/// LIVE gate (as the LIVE view reads it).
pub fn venue_effective(venue: VenueId, requested: Mode, live: LiveGate) -> Effective {
    match (venue, requested) {
        (VenueId::FourClassic | VenueId::OpenFour, Mode::Live) => {
            Effective::Blocked(FOUR_EXECUTION_PATH_MISSING)
        }
        (_, Mode::Live) => live.blocked(),
        // R12c/R12d: the Four.meme BUYs have their PAPER path (C3).
        (VenueId::Flap | VenueId::FourClassic | VenueId::OpenFour, Mode::Shadow) => {
            Effective::Shadow
        }
        (
            _,
            Mode::Off
            | Mode::Paper
            | Mode::Disabled
            | Mode::Enabled
            | Mode::PerDay(_)
            | Mode::Granted(_),
        ) => Effective::Off,
    }
}

/// The wallet and economic switches (plan decision 27.10.): permissions a
/// Mainnet signing or sending path needs besides its own authorities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchId {
    /// The launcher wallet may sign/send on Mainnet.
    WalletLauncher,
    /// The buyer wallet may sign/send on Mainnet.
    WalletBuyer,
    /// Mainnet wallet signatures (DRY SIGN, the G0.7 attempt).
    Sign,
    /// Use of an armed acceptance guard (the compiled V5 hash).
    Arm,
    /// Mainnet submission (the G0.7 release to the builders).
    Send,
    /// Consuming the one-attempt authority of a G0.7 attempt.
    Attempt,
    /// Global economic LIVE: the first condition of any requested LIVE.
    Live,
}

impl SwitchId {
    /// Its place in [`SwitchId::ALL`] (BSC3-002): a fixed index per value,
    /// the match exhaustive, so a new value needs its own; one left out of
    /// `ALL` is out of every array's bounds (fail-closed), never index 0.
    pub const fn index(self) -> usize {
        match self {
            SwitchId::WalletLauncher => 0,
            SwitchId::WalletBuyer => 1,
            SwitchId::Sign => 2,
            SwitchId::Arm => 3,
            SwitchId::Send => 4,
            SwitchId::Attempt => 5,
            SwitchId::Live => 6,
        }
    }

    pub const ALL: [SwitchId; 7] = [
        SwitchId::WalletLauncher,
        SwitchId::WalletBuyer,
        SwitchId::Sign,
        SwitchId::Arm,
        SwitchId::Send,
        SwitchId::Attempt,
        SwitchId::Live,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SwitchId::WalletLauncher => "BSC_WALLET_LAUNCHER",
            SwitchId::WalletBuyer => "BSC_WALLET_BUYER",
            SwitchId::Sign => "BSC_ECON_SIGN",
            SwitchId::Arm => "BSC_ECON_ARM",
            SwitchId::Send => "BSC_ECON_SEND",
            SwitchId::Attempt => "BSC_ECON_ATTEMPT",
            SwitchId::Live => "BSC_ECON_LIVE",
        }
    }

    pub fn parse(name: &str) -> Option<SwitchId> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// The global runtime execution mode (plan v9 §2.5), above the components:
/// SHADOW lets Strategy C keep its PAPER ledger (no signature), PAPER also
/// the C3 path to the PAPER sink (PAPER identity, nothing sent), OFF neither;
/// LIVE is BLOCKED like every LIVE. REPLAY is the offline job family
/// (`replay-proof`, `strategy-replay`), not a state of the running runtime.
pub const RUNTIME_MODE: &str = "BSC_RUNTIME_MODE";
/// The persisted daily limit of economic BUY attempts (plan 27.10.,
/// `8b4721bb`): every attempt that may send counts, at the latest before its
/// authority is consumed; the day is the UTC calendar day. 0 = none.
pub const BUY_ATTEMPTS_PER_DAY: &str = "BSC_BUY_ATTEMPTS_PER_DAY";

pub fn runtime_effective(requested: Mode, live: LiveGate) -> Effective {
    match requested {
        Mode::Shadow => Effective::Shadow,
        Mode::Paper => Effective::Paper,
        Mode::Live => live.blocked(),
        _ => Effective::Off,
    }
}

/// The owner's rule for a switch: effective as requested (a restart writes
/// it DISABLED; see [`Store::restart`]).
pub fn switch_effective(requested: Mode) -> Effective {
    match requested {
        Mode::Enabled => Effective::Enabled,
        _ => Effective::Disabled,
    }
}

/// Anything this owner switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entity {
    Source(SourceId),
    Component(ComponentId),
    Venue(VenueId),
    Switch(SwitchId),
    /// `BSC_RUNTIME_MODE`.
    Runtime,
    /// `BSC_BUY_ATTEMPTS_PER_DAY`.
    BuyLimit,
    /// `BSC_BUY_ATTEMPTS_TOTAL`.
    BuyTotal,
    /// `BSC_LIVE_GRANT` (its scope through [`Store::grant_live`]).
    LiveGrant,
}

impl Entity {
    pub fn name(self) -> &'static str {
        match self {
            Entity::Source(s) => s.name(),
            Entity::Component(c) => c.name(),
            Entity::Venue(v) => v.name(),
            Entity::Switch(s) => s.name(),
            Entity::Runtime => RUNTIME_MODE,
            Entity::BuyLimit => BUY_ATTEMPTS_PER_DAY,
            Entity::BuyTotal => BUY_ATTEMPTS_TOTAL,
            Entity::LiveGrant => LIVE_GRANT,
        }
    }

    pub fn parse(name: &str) -> Option<Entity> {
        match name {
            RUNTIME_MODE => return Some(Entity::Runtime),
            BUY_ATTEMPTS_PER_DAY => return Some(Entity::BuyLimit),
            BUY_ATTEMPTS_TOTAL => return Some(Entity::BuyTotal),
            LIVE_GRANT => return Some(Entity::LiveGrant),
            _ => {}
        }
        SourceId::parse(name)
            .map(Entity::Source)
            .or_else(|| ComponentId::parse(name).map(Entity::Component))
            .or_else(|| VenueId::parse(name).map(Entity::Venue))
            .or_else(|| SwitchId::parse(name).map(Entity::Switch))
    }

    /// Whether `mode` is a request this entity takes.
    pub fn accepts(self, mode: Mode) -> bool {
        match self {
            Entity::Switch(_) => matches!(mode, Mode::Disabled | Mode::Enabled),
            Entity::Runtime => matches!(mode, Mode::Off | Mode::Shadow | Mode::Paper | Mode::Live),
            // A stored limit of any size still decodes (BSC3-004 review M1);
            // a request above the configured maximum is refused in `apply`,
            // the limit in force is capped by it.
            Entity::BuyLimit | Entity::BuyTotal => matches!(mode, Mode::PerDay(_)),
            Entity::LiveGrant => matches!(mode, Mode::Granted(_) | Mode::Disabled),
            _ => matches!(mode, Mode::Off | Mode::Shadow | Mode::Live),
        }
    }

    /// The modes this entity takes, for a refusal.
    pub fn modes(self) -> &'static str {
        match self {
            Entity::Switch(_) => "DISABLED or ENABLED",
            Entity::Runtime => "OFF, SHADOW, PAPER or LIVE",
            Entity::BuyLimit => {
                "a whole number of BUY attempts per UTC day, at most BSC_BUY_ATTEMPTS_TOTAL_MAX (0 = none)"
            }
            Entity::BuyTotal => {
                "a whole number of BUY attempts for the whole campaign, at most BSC_BUY_ATTEMPTS_TOTAL_MAX (0 = none)"
            }
            Entity::LiveGrant => "a scope through `live grant`, or DISABLED to revoke it",
            _ => "OFF, SHADOW or LIVE",
        }
    }

    pub fn effective(self, requested: Mode, live: LiveGate) -> Effective {
        match self {
            Entity::Source(s) => effective(s, requested, live),
            Entity::Component(c) => component_effective(c, requested, live),
            Entity::Venue(v) => venue_effective(v, requested, live),
            Entity::Switch(_) => switch_effective(requested),
            Entity::Runtime => runtime_effective(requested, live),
            Entity::BuyLimit | Entity::BuyTotal => match requested {
                Mode::PerDay(n) => Effective::PerDay(n),
                _ => Effective::PerDay(0),
            },
            Entity::LiveGrant => match requested {
                Mode::Granted(_) => Effective::Enabled,
                _ => Effective::Disabled,
            },
        }
    }
}

impl From<SourceId> for Entity {
    fn from(s: SourceId) -> Self {
        Entity::Source(s)
    }
}

impl From<ComponentId> for Entity {
    fn from(c: ComponentId) -> Self {
        Entity::Component(c)
    }
}

impl From<VenueId> for Entity {
    fn from(v: VenueId) -> Self {
        Entity::Venue(v)
    }
}

impl From<SwitchId> for Entity {
    fn from(s: SwitchId) -> Self {
        Entity::Switch(s)
    }
}

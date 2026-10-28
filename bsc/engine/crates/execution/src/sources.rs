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
//! generation, which names the request it applied.

use darkbot_bsc_record::schema::bsc_runtime::{source_mode_receipt as rc, source_modes as sm};
use darkbot_bsc_record::{Draft, Record, decode};
use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The five independently switchable inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceId {
    Geth,
    PublicNode,
    Drpc,
    BlockReq,
    Club48,
}

impl SourceId {
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
    fn parse(name: &str, reason: Option<&str>) -> Option<Effective> {
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

pub const ECONOMIC_LIVE_NOT_AUTHORIZED: &str = "ECONOMIC_LIVE_NOT_AUTHORIZED";
/// A requested LIVE while the global economic LIVE switch is not ENABLED:
/// the first missing condition (plan v9 §2.5).
pub const ECONOMIC_LIVE_DISABLED: &str = "ECONOMIC_LIVE_DISABLED";
/// C2 may only run operationally after a real G0.8 acquisition PASS and the
/// evaluated G0.4 measurement (PHASENPLAN §10 offline override).
pub const C2_OPERATIONAL_NOT_AUTHORIZED: &str = "C2_OPERATIONAL_NOT_AUTHORIZED";
pub const SOURCE_ACCESS_UNQUALIFIED: &str = "SOURCE_ACCESS_UNQUALIFIED";
/// Four.meme Classic and OpenFour are recognized and decoded (Phase E), but
/// no economics or execution path exists for them (G/C3 after the G0.1
/// Four contract freeze): there is nothing to evaluate in SHADOW.
pub const FOUR_EXECUTION_PATH_MISSING: &str = "FOUR_EXECUTION_PATH_MISSING";
/// A switch ENABLED in an earlier boot session: every restart disables it.
pub const RESET_BY_RESTART: &str = "RESET_BY_RESTART";
const BLOCK_REASONS: [&str; 6] = [
    ECONOMIC_LIVE_NOT_AUTHORIZED,
    ECONOMIC_LIVE_DISABLED,
    C2_OPERATIONAL_NOT_AUTHORIZED,
    SOURCE_ACCESS_UNQUALIFIED,
    FOUR_EXECUTION_PATH_MISSING,
    RESET_BY_RESTART,
];
/// The request ID a restart reset of the switches records.
pub const RESTART_RESET: &str = "RESTART_RESET";

/// Why a requested LIVE is not live now (the LIVE gate, plan 28.10.); each
/// names the first missing condition, [`Generation::live_blockers`] all of
/// them. They are never stored: LIVE is evaluated as it holds now.
pub const LIVE_GRANT_MISSING: &str = "LIVE_GRANT_MISSING";
pub const LIVE_GRANT_RESET_BY_RESTART: &str = "LIVE_GRANT_RESET_BY_RESTART";
pub const LIVE_GRANT_EXPIRED: &str = "LIVE_GRANT_EXPIRED";
pub const LIVE_NOT_IN_SCOPE: &str = "LIVE_NOT_IN_SCOPE";
pub const LIVE_RUNTIME_NOT_REQUESTED: &str = "LIVE_RUNTIME_NOT_REQUESTED";
pub const WALLET_LAUNCHER_DISABLED: &str = "WALLET_LAUNCHER_DISABLED";
pub const WALLET_BUYER_DISABLED: &str = "WALLET_BUYER_DISABLED";
pub const ECON_SIGN_DISABLED: &str = "ECON_SIGN_DISABLED";
pub const ECON_ARM_DISABLED: &str = "ECON_ARM_DISABLED";
pub const ECON_SEND_DISABLED: &str = "ECON_SEND_DISABLED";
pub const ECON_ATTEMPT_DISABLED: &str = "ECON_ATTEMPT_DISABLED";
pub const BUY_LIMIT_UNREADABLE: &str = "BUY_LIMIT_UNREADABLE";
pub const BUY_LIMIT_EXHAUSTED: &str = "BUY_LIMIT_EXHAUSTED";
pub const ATTEMPT_TOTAL_UNREADABLE: &str = "ATTEMPT_TOTAL_UNREADABLE";
pub const ATTEMPT_TOTAL_EXHAUSTED: &str = "ATTEMPT_TOTAL_EXHAUSTED";
pub const LIVE_GRANT_ATTEMPTS_EXHAUSTED: &str = "LIVE_GRANT_ATTEMPTS_EXHAUSTED";
pub const LIVE_SPEND_UNREADABLE: &str = "LIVE_SPEND_UNREADABLE";
pub const LIVE_GRANT_SPEND_EXHAUSTED: &str = "LIVE_GRANT_SPEND_EXHAUSTED";
pub const LIVE_VENUE_UNQUALIFIED: &str = "LIVE_VENUE_UNQUALIFIED";
pub const LIVE_SOURCE_UNQUALIFIED: &str = "LIVE_SOURCE_UNQUALIFIED";
pub const LIVE_ADAPTER_UNQUALIFIED: &str = "LIVE_ADAPTER_UNQUALIFIED";
pub const LIVE_ACQUISITION_UNPROVEN: &str = "LIVE_ACQUISITION_UNPROVEN";
pub const LIVE_RISK_DECISION_MISSING: &str = "LIVE_RISK_DECISION_MISSING";
/// The grant on file is not the one the executor was armed under.
pub const LIVE_GRANT_CHANGED: &str = "LIVE_GRANT_CHANGED";
/// A BUY's input exceeds the grant's largest input.
pub const LIVE_INPUT_ABOVE_GRANT: &str = "LIVE_INPUT_ABOVE_GRANT";
/// The same signed transaction again: never a second time (no retry).
pub const LIVE_INTENT_SEEN: &str = "LIVE_INTENT_SEEN";
/// A LIVE transaction whose bytes may have left is not resolved on chain
/// (BSC3-041): no new arm may take its nonce.
pub const LIVE_RELEASE_UNRESOLVED: &str = "LIVE_RELEASE_UNRESOLVED";
/// The LIVE ledger cannot be read or written: what may have left is not
/// known, nothing arms.
pub const LIVE_LEDGER_UNREADABLE: &str = "LIVE_LEDGER_UNREADABLE";

/// The LIVE gate's conditions in the order an operator meets them, each
/// with the reasons that say it does not hold (plan 28.10.): the LIVE tab's
/// checklist. The scope conditions (from GRANT_ATTEMPTS on) are judged only
/// against a grant.
pub const LIVE_CONDITIONS: [(&str, &[&str]); 18] = [
    ("ECONOMIC_LIVE", &[ECONOMIC_LIVE_DISABLED]),
    (
        "LIVE_GRANT",
        &[
            LIVE_GRANT_MISSING,
            LIVE_GRANT_RESET_BY_RESTART,
            LIVE_GRANT_EXPIRED,
        ],
    ),
    ("WALLET_LAUNCHER", &[WALLET_LAUNCHER_DISABLED]),
    ("WALLET_BUYER", &[WALLET_BUYER_DISABLED]),
    ("ECON_SIGN", &[ECON_SIGN_DISABLED]),
    ("ECON_ARM", &[ECON_ARM_DISABLED]),
    ("ECON_SEND", &[ECON_SEND_DISABLED]),
    ("ECON_ATTEMPT", &[ECON_ATTEMPT_DISABLED]),
    ("RUNTIME_LIVE", &[LIVE_RUNTIME_NOT_REQUESTED]),
    (
        "BUY_LIMIT_DAILY",
        &[BUY_LIMIT_UNREADABLE, BUY_LIMIT_EXHAUSTED],
    ),
    (
        "BUY_LIMIT_TOTAL",
        &[ATTEMPT_TOTAL_UNREADABLE, ATTEMPT_TOTAL_EXHAUSTED],
    ),
    ("GRANT_ATTEMPTS", &[LIVE_GRANT_ATTEMPTS_EXHAUSTED]),
    (
        "GRANT_SPEND",
        &[LIVE_SPEND_UNREADABLE, LIVE_GRANT_SPEND_EXHAUSTED],
    ),
    ("VENUE_QUALIFIED", &[LIVE_VENUE_UNQUALIFIED]),
    ("SOURCES_QUALIFIED", &[LIVE_SOURCE_UNQUALIFIED]),
    ("ADAPTERS_QUALIFIED", &[LIVE_ADAPTER_UNQUALIFIED]),
    ("ACQUISITION_PROVEN", &[LIVE_ACQUISITION_UNPROVEN]),
    ("RISK_DECISION", &[LIVE_RISK_DECISION_MISSING]),
];

/// The first of [`LIVE_CONDITIONS`] judged only against a grant.
const SCOPE_CONDITIONS_FROM: usize = 11;

/// What an exit of a position the grant opened does not need of the LIVE
/// gate: BUY attempts (the attempt switch, the daily and campaign limits,
/// the grant's attempts) and room for another BUY input. Its own gas must
/// still fit the grant's spend ([`Store::admit_live`]).
const EXIT_EXEMPT: [&str; 8] = [
    ECON_ATTEMPT_DISABLED,
    BUY_LIMIT_UNREADABLE,
    BUY_LIMIT_EXHAUSTED,
    ATTEMPT_TOTAL_UNREADABLE,
    ATTEMPT_TOTAL_EXHAUSTED,
    LIVE_GRANT_ATTEMPTS_EXHAUSTED,
    LIVE_GRANT_SPEND_EXHAUSTED,
    LIVE_SPEND_UNREADABLE,
];

/// The campaign's total BUY attempt limit (plan 28.10., `0a647175`): every
/// real economic BUY attempt counts, G0.7 included, UNKNOWN ones too, over
/// every day and restart. 0 initially.
pub const BUY_ATTEMPTS_TOTAL: &str = "BSC_BUY_ATTEMPTS_TOTAL";
/// The user's decision bounds the total limit: at most ten real BUY
/// attempts in all (PHASENPLAN 27.10./28.10.).
pub const MAX_BUY_ATTEMPTS_TOTAL: u32 = 10;
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
/// PAPER path and Strategy C; Four.meme has no path to admit to yet.
pub fn venue_effective(venue: VenueId, requested: Mode, live: LiveGate) -> Effective {
    match (venue, requested) {
        (_, Mode::Live) => live.blocked(),
        (VenueId::Flap, Mode::Shadow) => Effective::Shadow,
        (_, Mode::Shadow) => Effective::Blocked(FOUR_EXECUTION_PATH_MISSING),
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
            Entity::BuyLimit => matches!(mode, Mode::PerDay(_)),
            Entity::BuyTotal => matches!(mode, Mode::PerDay(n) if n <= MAX_BUY_ATTEMPTS_TOTAL),
            Entity::LiveGrant => matches!(mode, Mode::Granted(_) | Mode::Disabled),
            _ => matches!(mode, Mode::Off | Mode::Shadow | Mode::Live),
        }
    }

    /// The modes this entity takes, for a refusal.
    pub fn modes(self) -> &'static str {
        match self {
            Entity::Switch(_) => "DISABLED or ENABLED",
            Entity::Runtime => "OFF, SHADOW, PAPER or LIVE",
            Entity::BuyLimit => "a whole number of BUY attempts per UTC day (0 = none)",
            Entity::BuyTotal => {
                "a whole number of BUY attempts for the whole campaign, at most 10 (0 = none)"
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

/// Which strategy paths a LIVE grant lets act.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveStrategies {
    /// C3: the BUY behind a pending launch, in the launch's bundle.
    C3,
    /// Strategy C: BUY and exit on canonical Flap events.
    StrategyC,
    Both,
}

impl LiveStrategies {
    pub fn name(self) -> &'static str {
        match self {
            Self::C3 => "C3",
            Self::StrategyC => "STRATEGY_C",
            Self::Both => "C3+STRATEGY_C",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        [Self::C3, Self::StrategyC, Self::Both]
            .into_iter()
            .find(|s| s.name() == text)
    }

    pub fn c3(self) -> bool {
        self != Self::StrategyC
    }

    pub fn strategy_c(self) -> bool {
        self != Self::C3
    }
}

/// The inclusion adapters a LIVE grant may send through (the frozen
/// capability records of `submission_policy::capability`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveAdapter {
    Club48BuilderV1,
    BlockRazorBuilderV1,
}

impl LiveAdapter {
    pub const ALL: [LiveAdapter; 2] = [
        LiveAdapter::Club48BuilderV1,
        LiveAdapter::BlockRazorBuilderV1,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Club48BuilderV1 => "CLUB48_BUILDER_V1",
            Self::BlockRazorBuilderV1 => "BLOCKRAZOR_BUILDER_V1",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.name() == text)
    }
}

/// The operator's LIVE grant (plan 28.10., `0a647175`): the one economic
/// approval of a LIVE scope with its limits. It is approved by its scope
/// text's SHA-256 (what the operator confirmed is what is stored), counts
/// only in the boot session it was written in and until it expires, and is
/// revoked by a restart, by `DISABLED` or by a new grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveGrant {
    pub venue: VenueId,
    pub strategies: LiveStrategies,
    pub sources: Vec<SourceId>,
    pub adapters: Vec<LiveAdapter>,
    /// The largest input of one BUY.
    pub max_input_wei: u128,
    /// The most the grant may spend in all (inputs, gas, approvals, SELLs).
    pub max_spend_wei: u128,
    /// The most BUY attempts under this grant (within the campaign's total).
    pub max_attempts: u32,
    pub expires_unix_ms: u64,
}

impl LiveGrant {
    /// Refused unless the scope can be live at all: Flap (Four has no
    /// execution path), at least one source and adapter, each named once,
    /// positive limits, an input within the spend, attempts within the
    /// campaign's ceiling.
    pub fn validate(&self) -> Result<(), String> {
        if self.venue != VenueId::Flap {
            return Err(format!(
                "{}: {FOUR_EXECUTION_PATH_MISSING}, only BSC_VENUE_FLAP can be live",
                self.venue.name()
            ));
        }
        if self.sources.is_empty() || self.adapters.is_empty() {
            return Err("a LIVE grant names at least one source and one adapter".into());
        }
        for (i, s) in self.sources.iter().enumerate() {
            if self.sources[..i].contains(s) {
                return Err(format!("{} is named twice", s.name()));
            }
        }
        for (i, a) in self.adapters.iter().enumerate() {
            if self.adapters[..i].contains(a) {
                return Err(format!("{} is named twice", a.name()));
            }
        }
        if self.max_input_wei == 0 || self.max_spend_wei < self.max_input_wei {
            return Err("max_input_wei must be positive and within max_spend_wei".into());
        }
        if !(1..=MAX_BUY_ATTEMPTS_TOTAL).contains(&self.max_attempts) {
            return Err(format!(
                "max_attempts {} outside 1..={MAX_BUY_ATTEMPTS_TOTAL}",
                self.max_attempts
            ));
        }
        if self.expires_unix_ms == 0 {
            return Err("expires_unix_ms must be set".into());
        }
        Ok(())
    }

    /// The canonical scope text the operator confirms, one `key=value` per
    /// line in this order (the frontend builds the same text).
    pub fn scope_text(&self) -> String {
        let join = |names: Vec<&str>| names.join(",");
        format!(
            "darkbot.bsc.live-grant.v1\nvenue={}\nstrategies={}\nsources={}\nadapters={}\nmax_input_wei={}\nmax_spend_wei={}\nmax_attempts={}\nexpires_unix_ms={}\n",
            self.venue.name(),
            self.strategies.name(),
            join(self.sources.iter().map(|s| s.name()).collect()),
            join(self.adapters.iter().map(|a| a.name()).collect()),
            self.max_input_wei,
            self.max_spend_wei,
            self.max_attempts,
            self.expires_unix_ms
        )
    }

    /// SHA-256 of [`LiveGrant::scope_text`].
    pub fn scope_sha256(&self) -> [u8; 32] {
        use sha2::Digest;
        sha2::Sha256::digest(self.scope_text().as_bytes()).into()
    }

    /// Whether `entity` is within the scope.
    fn names(&self, entity: Entity) -> bool {
        match entity {
            Entity::Runtime | Entity::Component(ComponentId::StateC2) => true,
            Entity::Component(ComponentId::ExecC3) => self.strategies.c3(),
            Entity::Component(ComponentId::StrategyC) => self.strategies.strategy_c(),
            Entity::Venue(v) => v == self.venue,
            Entity::Source(s) => self.sources.contains(&s),
            _ => false,
        }
    }
}

/// The qualification evidence the LIVE gate needs for a grant (what is
/// verified; package 2 reads it from its records). Empty: nothing is
/// qualified, so nothing is live.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveQualification {
    pub venues: Vec<VenueId>,
    pub sources: Vec<SourceId>,
    pub adapters: Vec<LiveAdapter>,
    /// A real G0.8 acquisition PASS of the scope.
    pub acquisition: bool,
    /// The operator's documented J risk decision for the scope.
    pub risk_decision: bool,
}

/// Everything the LIVE gate reads besides the owner state, as it is now.
#[derive(Clone, Copy, Debug)]
pub struct LiveContext<'a> {
    /// This host's boot session (none: every switch and grant counts as
    /// reset).
    pub boot: Option<&'a str>,
    pub now_unix_ms: u64,
    /// Today's BUY attempts (none: unreadable).
    pub daily: Option<&'a BuyUsage>,
    /// The campaign's BUY attempts (none: unreadable).
    pub total: Option<&'a TotalUsage>,
    /// What the grant has spent (none: unreadable).
    pub spent_wei: Option<u128>,
    pub qualification: &'a LiveQualification,
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
            request_id: None,
            boot: None,
        }
    }

    pub fn component(&self, id: ComponentId) -> ComponentMode {
        self.components[ComponentId::ALL.iter().position(|c| *c == id).unwrap_or(0)]
    }

    pub fn source(&self, id: SourceId) -> SourceMode {
        self.sources[SourceId::ALL.iter().position(|s| *s == id).unwrap_or(0)]
    }

    pub fn venue(&self, id: VenueId) -> &VenueMode {
        &self.venues[VenueId::ALL.iter().position(|v| *v == id).unwrap_or(0)]
    }

    pub fn switch(&self, id: SwitchId) -> &SwitchMode {
        &self.switches[SwitchId::ALL.iter().position(|s| *s == id).unwrap_or(0)]
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

    /// The daily BUY attempt limit in force.
    pub fn buy_attempts_per_day(&self) -> u32 {
        match self.buy_limit.effective {
            Effective::PerDay(n) => n,
            _ => 0,
        }
    }

    /// The campaign's total BUY attempt limit in force.
    pub fn buy_attempts_total(&self) -> u32 {
        match self.buy_total.effective {
            Effective::PerDay(n) => n.min(MAX_BUY_ATTEMPTS_TOTAL),
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
            }
            if !q.risk_decision {
                out.push(LIVE_RISK_DECISION_MISSING);
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
    fn set_live(&mut self, grant: Option<LiveGrant>, set_by: SetBy) {
        self.live = grant;
        self.live_set_by = Some(set_by);
    }

    fn set(&mut self, entity: Entity, requested: Mode, set_by: SetBy) {
        match entity {
            Entity::Source(source) => {
                let at = SourceId::ALL.iter().position(|s| *s == source).unwrap_or(0);
                self.sources[at].requested = requested;
            }
            Entity::Component(component) => {
                let at = ComponentId::ALL
                    .iter()
                    .position(|c| *c == component)
                    .unwrap_or(0);
                self.components[at].requested = requested;
            }
            Entity::Venue(venue) => {
                let at = VenueId::ALL.iter().position(|v| *v == venue).unwrap_or(0);
                self.venues[at].requested = requested;
                self.venues[at].set_by = Some(set_by);
            }
            Entity::Switch(switch) => {
                let at = SwitchId::ALL.iter().position(|s| *s == switch).unwrap_or(0);
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
            Entity::LiveGrant => self.set_live(None, set_by),
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
            self.set_live(None, reset);
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
                        );
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
                let at = SwitchId::ALL.iter().position(|s| *s == switch).unwrap_or(0);
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
            let at = SourceId::ALL.iter().position(|s| *s == source).unwrap_or(0);
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
            let at = ComponentId::ALL
                .iter()
                .position(|c| *c == component)
                .unwrap_or(0);
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
            let at = VenueId::ALL.iter().position(|v| *v == venue).unwrap_or(0);
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
    /// `APPLIED` or `REJECTED_STALE`.
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

/// The owner's state directory (`generations/`, `receipts/`).
pub struct Store {
    dir: PathBuf,
    /// Generations kept besides the current one (older ones are pruned).
    keep: u64,
    /// The host boot session every write is stamped with.
    boot: Result<String, String>,
}

/// A request ID is a single safe path component (the gateway's UUIDs fit).
fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Publishes `bytes` at `path` only if nothing is there: written and synced
/// in a private temporary file first, then hard-linked into place (fails
/// with `AlreadyExists` if another writer won), so a reader never sees a
/// partial record.
fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut nonce = [0u8; 8];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut nonce))?;
    let tmp = dir.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        nonce.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ));
    let written = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::hard_link(&tmp, path)
    })();
    let _ = fs::remove_file(&tmp);
    written?;
    fs::File::open(dir)?.sync_all()
}

/// Creates `d` (0700) unless it exists; a new directory is made durable in
/// its parent, so a slot or generation written into it survives power loss.
fn create_durable_dir(d: &Path) -> Result<(), String> {
    match fs::DirBuilder::new().mode(0o700).create(d) {
        Ok(()) => d
            .parent()
            .map_or(Ok(()), |p| fs::File::open(p).and_then(|f| f.sync_all()))
            .map_err(|e| format!("{}: {e}", d.display())),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(format!("{}: {e}", d.display())),
    }
}

/// The host's boot session, read once per process: macOS
/// `kern.bootsessionuuid`, Linux `/proc/sys/kernel/random/boot_id`. It
/// changes with every boot, so an ENABLED switch never outlives a restart.
pub fn boot_session() -> Result<String, String> {
    static BOOT: OnceLock<Result<String, String>> = OnceLock::new();
    BOOT.get_or_init(|| {
        let raw = if cfg!(target_os = "macos") {
            std::process::Command::new("/usr/sbin/sysctl")
                .env_clear()
                .args(["-n", "kern.bootsessionuuid"])
                .output()
                .map_err(|e| format!("boot session: {e}"))
                .and_then(|o| {
                    if o.status.success() {
                        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
                    } else {
                        Err("boot session: sysctl refused".into())
                    }
                })?
        } else {
            fs::read_to_string("/proc/sys/kernel/random/boot_id")
                .map_err(|e| format!("boot session: {e}"))?
        };
        let boot = raw.trim();
        if boot.is_empty() || boot.len() > 128 || !boot.bytes().all(|b| b.is_ascii_graphic()) {
            return Err("boot session: not a host boot identifier".into());
        }
        Ok(boot.to_string())
    })
    .clone()
}

/// The account home of this process's real user from the user database
/// (macOS directory services, Linux `/etc/passwd` only: an account known
/// only through NSS is refused, fail-closed), read once; never `$HOME`.
pub fn account_home() -> Result<PathBuf, String> {
    static HOME: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    HOME.get_or_init(|| {
        let run = |program: &str, args: &[&str]| {
            std::process::Command::new(program)
                .env_clear()
                .args(args)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .ok_or(format!("account home: {program} refused"))
        };
        let home = if cfg!(target_os = "macos") {
            let uid = run("/usr/bin/id", &["-u"])?;
            let entry = run(
                "/usr/bin/dscacheutil",
                &["-q", "user", "-a", "uid", uid.trim()],
            )?;
            entry
                .lines()
                .find_map(|l| l.strip_prefix("dir: "))
                .map(str::to_string)
        } else {
            let status = fs::read_to_string("/proc/self/status")
                .map_err(|e| format!("account home: {e}"))?;
            let uid = status
                .lines()
                .find_map(|l| l.strip_prefix("Uid:"))
                .and_then(|l| l.split_whitespace().next())
                .ok_or("account home: no real uid")?
                .to_string();
            let passwd =
                fs::read_to_string("/etc/passwd").map_err(|e| format!("account home: {e}"))?;
            passwd.lines().find_map(|l| {
                let f: Vec<&str> = l.split(':').collect();
                (f.len() >= 7 && f[2] == uid).then(|| f[5].to_string())
            })
        }
        .filter(|h| h.starts_with('/'))
        .ok_or("account home: not in the user database")?;
        Ok(PathBuf::from(home.trim()))
    })
    .clone()
}

/// The owner state darkbotctl's controller keeps (`$HOME/.darkbot/run`,
/// its RUNDIR, `bsc-sources`): the one place a Mainnet signing or sending
/// path reads its switches and its daily BUY limit from. It lies under the
/// account home from the user database; a `HOME` naming another directory
/// is refused, so no self-made state elsewhere (written by the owner CLI
/// with another `--state`) is ever honored instead of the one the GUI and
/// `darkbotctl bsc source show` display.
pub fn canonical_state_dir() -> Result<PathBuf, String> {
    let home = account_home()?;
    match std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        Some(h) if Path::new(&h) != home => Err(format!(
            "HOME is not the account home {}: the switches are read only from the account's owner state",
            home.display()
        )),
        _ => Ok(home.join(".darkbot/run/bsc-sources")),
    }
}

/// The generation on file in `dir` without creating anything: the initial
/// one (everything DISABLED) when there is none.
pub fn current_generation(dir: &Path) -> Result<Generation, String> {
    match fs::symlink_metadata(dir.join("generations")) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Generation::initial()),
        Err(e) => return Err(e.to_string()),
        Ok(m) if !m.is_dir() => return Err("generations is not a real directory".into()),
        Ok(_) => {}
    }
    match highest_generation(dir)? {
        None => Ok(Generation::initial()),
        Some(n) => {
            let bytes = fs::read(dir.join("generations").join(format!("{n:020}.dbrec")))
                .map_err(|e| e.to_string())?;
            let g = Generation::decode(&bytes)?;
            if g.generation != n {
                return Err(format!(
                    "generation file {n} holds generation {}",
                    g.generation
                ));
            }
            Ok(g)
        }
    }
}

/// Refused unless every switch in `needed` is ENABLED in the generation on
/// file in `dir` and was written in the boot session `boot`. Returns the
/// generation checked. A missing, unreadable or foreign state refuses: every
/// switch is DISABLED then.
pub fn require_enabled(dir: &Path, boot: &str, needed: &[SwitchId]) -> Result<u64, String> {
    let g = current_generation(dir)
        .map_err(|e| format!("CONTROL_STATE_UNREADABLE: {e}; every switch DISABLED"))?;
    let off: Vec<String> = needed
        .iter()
        .filter_map(|id| match g.switch_now(*id, Some(boot)) {
            Effective::Enabled => None,
            other => Some(format!(
                "{} {}{}",
                id.name(),
                other.name(),
                other
                    .reason()
                    .map(|r| format!(" ({r})"))
                    .unwrap_or_default()
            )),
        })
        .collect();
    if off.is_empty() {
        Ok(g.generation)
    } else {
        Err(format!(
            "CONTROL_DISABLED at generation {}: {}",
            g.generation,
            off.join(", ")
        ))
    }
}

/// [`require_enabled`] on the canonical owner state of this boot session.
pub fn require_enabled_here(needed: &[SwitchId]) -> Result<u64, String> {
    let dir = canonical_state_dir()
        .map_err(|e| format!("CONTROL_STATE_UNREADABLE: {e}; every switch DISABLED"))?;
    let boot = boot_session().map_err(|e| format!("{e}; every switch DISABLED"))?;
    require_enabled(&dir, &boot, needed)
}

/// One UTC calendar day of the BUY attempt limit (the contract's day:
/// 00:00:00.000 UTC to the next).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuyDay {
    /// Days since 1970-01-01 UTC.
    pub index: u64,
    /// `YYYY-MM-DD`.
    pub utc: String,
}

/// Milliseconds of one UTC day.
const DAY_MS: u64 = 86_400_000;

impl BuyDay {
    pub fn at(unix_ms: u64) -> BuyDay {
        let index = unix_ms / DAY_MS;
        // Civil date from days since the epoch (proleptic Gregorian).
        let z = index as i64 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        BuyDay {
            index,
            utc: format!("{year:04}-{month:02}-{day:02}"),
        }
    }
}

/// The daily BUY attempts of one UTC day: taken slots and the limit in
/// force (an earlier, higher limit can leave more taken than allowed now).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuyUsage {
    pub day: BuyDay,
    pub used: u64,
    pub limit: u32,
    /// The owner generation the limit was read from.
    pub generation: u64,
}

impl BuyUsage {
    pub fn remaining(&self) -> u64 {
        u64::from(self.limit).saturating_sub(self.used)
    }
}

/// One taken slot: the attempt's place in its day.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuySlot {
    pub day: BuyDay,
    pub slot: u64,
    pub limit: u32,
}

fn buy_day_dir(dir: &Path, day: &BuyDay) -> PathBuf {
    dir.join("buy-attempts").join(&day.utc)
}

/// The slot files of one day, each with the attempt that took it; a slot
/// file that does not decode counts as taken (never under-counted).
fn buy_slots(day_dir: &Path) -> Result<Vec<(u64, Option<String>)>, String> {
    use darkbot_bsc_record::schema::bsc_runtime::buy_attempt_slot as bs;
    let entries = match fs::read_dir(day_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("buy attempts unreadable: {e}")),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let Some(slot) = name
            .to_string_lossy()
            .strip_suffix(".dbrec")
            .and_then(|n| n.parse::<u64>().ok())
        else {
            continue;
        };
        let attempt = fs::read(entry.path())
            .ok()
            .and_then(|b| decode(&bs::SCHEMA, &b).ok())
            .and_then(|r| r.text(bs::ATTEMPT_ID).ok().map(str::to_string));
        out.push((slot, attempt));
    }
    Ok(out)
}

/// The BUY attempts of the UTC day of `now_ms` against the limit in force,
/// read-only (for the status projection).
pub fn buy_usage(dir: &Path, now_ms: u64) -> Result<BuyUsage, String> {
    let g = current_generation(dir)?;
    let day = BuyDay::at(now_ms);
    let used = buy_slots(&buy_day_dir(dir, &day))?.len() as u64;
    Ok(BuyUsage {
        day,
        used,
        limit: g.buy_attempts_per_day(),
        generation: g.generation,
    })
}

/// The campaign's BUY attempts against its total limit in force.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TotalUsage {
    pub used: u64,
    pub limit: u32,
    /// The owner generation the limit was read from.
    pub generation: u64,
    /// The LIVE grant generation of every taken slot (0: none, e.g. G0.7).
    pub grants: Vec<u64>,
}

impl TotalUsage {
    pub fn remaining(&self) -> u64 {
        u64::from(self.limit).saturating_sub(self.used)
    }

    /// The attempts taken under the grant written at `generation`.
    pub fn used_by_grant(&self, generation: u64) -> u64 {
        self.grants.iter().filter(|g| **g == generation).count() as u64
    }
}

/// One taken slot of the campaign's total.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TotalSlot {
    pub slot: u64,
    pub limit: u32,
}

fn total_dir(dir: &Path) -> PathBuf {
    dir.join("buy-attempts-total")
}

/// The campaign's slot files: slot, attempt and grant generation; a slot
/// that does not decode counts as taken by nobody (never under-counted).
fn total_slots(dir: &Path) -> Result<Vec<(u64, Option<String>, u64)>, String> {
    use darkbot_bsc_record::schema::bsc_runtime::buy_attempt_total_slot as ts;
    let entries = match fs::read_dir(total_dir(dir)) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("total BUY attempts unreadable: {e}")),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let Some(slot) = entry
            .file_name()
            .to_string_lossy()
            .strip_suffix(".dbrec")
            .and_then(|n| n.parse::<u64>().ok())
        else {
            continue;
        };
        let record = fs::read(entry.path())
            .ok()
            .and_then(|b| decode(&ts::SCHEMA, &b).ok());
        let attempt = record
            .as_ref()
            .and_then(|r| r.text(ts::ATTEMPT_ID).ok().map(str::to_string));
        let grant = record
            .as_ref()
            .and_then(|r| r.u64(ts::GRANT_GENERATION).ok())
            .unwrap_or(0);
        out.push((slot, attempt, grant));
    }
    Ok(out)
}

/// The campaign's BUY attempts against the total limit in force, read-only.
pub fn total_usage(dir: &Path) -> Result<TotalUsage, String> {
    let g = current_generation(dir)?;
    let slots = total_slots(dir)?;
    Ok(TotalUsage {
        used: slots.len() as u64,
        limit: g.buy_attempts_total(),
        generation: g.generation,
        grants: slots.into_iter().map(|(_, _, grant)| grant).collect(),
    })
}

/// The owner state the LIVE gate reads at `now_ms`: the generation, today's
/// and the campaign's BUY attempts.
pub fn live_inputs(dir: &Path, now_ms: u64) -> Result<(Generation, BuyUsage, TotalUsage), String> {
    Ok((
        current_generation(dir)?,
        buy_usage(dir, now_ms)?,
        total_usage(dir)?,
    ))
}

/// Everything besides the generation and the qualification the LIVE gate
/// reads at `now_ms`, owned: one gate for a request's receipt, the runtime
/// and every tab. A part that does not read stays unreadable, and the gate
/// names it.
pub struct GateReads {
    pub boot: Option<String>,
    pub now_unix_ms: u64,
    pub daily: Option<BuyUsage>,
    pub total: Option<TotalUsage>,
    pub spent_wei: Option<u128>,
}

impl GateReads {
    pub fn read(dir: &Path, g: &Generation, now_ms: u64) -> GateReads {
        GateReads {
            boot: boot_session().ok(),
            now_unix_ms: now_ms,
            daily: buy_usage(dir, now_ms).ok(),
            total: total_usage(dir).ok(),
            spent_wei: match &g.live_set_by {
                Some(by) => spent_by_grant(dir, by.generation).ok(),
                None => Some(0),
            },
        }
    }

    pub fn ctx<'a>(&'a self, qualification: &'a LiveQualification) -> LiveContext<'a> {
        LiveContext {
            boot: self.boot.as_deref(),
            now_unix_ms: self.now_unix_ms,
            daily: self.daily.as_ref(),
            total: self.total.as_ref(),
            spent_wei: self.spent_wei,
            qualification,
        }
    }
}

fn spend_dir(dir: &Path) -> PathBuf {
    dir.join("live-spend")
}

fn wei_u128(word: [u8; 32]) -> Option<u128> {
    if word[..16].iter().any(|b| *b != 0) {
        return None;
    }
    Some(u128::from_be_bytes(word[16..].try_into().ok()?))
}

/// What the grant written at `grant_generation` has spent: the sum of the
/// most each of its admitted intents may lose. An entry that does not
/// decode makes the spend unreadable (never under-counted).
pub fn spent_by_grant(dir: &Path, grant_generation: u64) -> Result<u128, String> {
    use darkbot_bsc_record::schema::bsc_runtime::live_spend as ls;
    let entries = match fs::read_dir(spend_dir(dir)) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(format!("{LIVE_SPEND_UNREADABLE}: {e}")),
    };
    let mut spent = 0u128;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{LIVE_SPEND_UNREADABLE}: {e}"))?;
        if !entry.file_name().to_string_lossy().ends_with(".dbrec") {
            continue;
        }
        let r = fs::read(entry.path())
            .ok()
            .and_then(|b| decode(&ls::SCHEMA, &b).ok())
            .ok_or_else(|| format!("{LIVE_SPEND_UNREADABLE}: {}", entry.path().display()))?;
        let unreadable = |_| format!("{LIVE_SPEND_UNREADABLE}: {}", entry.path().display());
        if r.u64(ls::GRANT_GENERATION).map_err(unreadable)? != grant_generation {
            continue;
        }
        let most = r
            .u256(ls::MAX_WEI)
            .ok()
            .and_then(wei_u128)
            .ok_or_else(|| format!("{LIVE_SPEND_UNREADABLE}: {}", entry.path().display()))?;
        spent = spent
            .checked_add(most)
            .ok_or_else(|| format!("{LIVE_SPEND_UNREADABLE}: the spend overflows"))?;
    }
    Ok(spent)
}

/// What a LIVE intent is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveIntentKind {
    Buy,
    Approve,
    Sell,
}

impl LiveIntentKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Buy => "BUY",
            Self::Approve => "APPROVE",
            Self::Sell => "SELL",
        }
    }
}

/// One LIVE intent the executor asks the owner to admit, immediately
/// before its dispatch (plan 28.10.).
#[derive(Clone, Copy, Debug)]
pub struct LiveIntent<'a> {
    /// The signed transaction's hash, hex: a BUY's attempt id.
    pub id: &'a str,
    pub component: ComponentId,
    pub kind: LiveIntentKind,
    pub input_wei: u128,
    /// Input and gas at the signed gas price and limit.
    pub max_wei: u128,
    /// The token it trades and the sender's nonce it is signed at: with
    /// the admission durable before any byte can leave (BSC3-024).
    pub token: [u8; 20],
    pub nonce: u64,
    /// The last block its bundle may be included in (BSC3-048).
    pub max_block: u64,
}

/// One LIVE intent the owner admitted, as its entry holds it: the
/// write-ahead record of a transaction whose bytes may have left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admitted {
    pub tx_hash: [u8; 32],
    pub kind: LiveIntentKind,
    pub component: ComponentId,
    pub grant_generation: u64,
    pub input_wei: u128,
    pub max_wei: u128,
    /// Absent in an entry written before BSC3-024.
    pub token: Option<[u8; 20]>,
    pub nonce: Option<u64>,
    /// Absent in an entry written before BSC3-048: the window is unknown.
    pub max_block: Option<u64>,
}

/// Every LIVE intent the owner ever admitted, in no particular order. An
/// entry that does not decode refuses them all (never under-counted).
pub fn admitted_intents(dir: &Path) -> Result<Vec<Admitted>, String> {
    use darkbot_bsc_record::schema::bsc_runtime::live_spend as ls;
    let entries = match fs::read_dir(spend_dir(dir)) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{LIVE_SPEND_UNREADABLE}: {e}")),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("{LIVE_SPEND_UNREADABLE}: {e}"))?;
        if !entry.file_name().to_string_lossy().ends_with(".dbrec") {
            continue;
        }
        let bad = || format!("{LIVE_SPEND_UNREADABLE}: {}", entry.path().display());
        let r = fs::read(entry.path())
            .ok()
            .and_then(|b| decode(&ls::SCHEMA, &b).ok())
            .ok_or_else(bad)?;
        let kind = match r.text(ls::KIND).map_err(|_| bad())? {
            "BUY" => LiveIntentKind::Buy,
            "APPROVE" => LiveIntentKind::Approve,
            "SELL" => LiveIntentKind::Sell,
            _ => return Err(bad()),
        };
        out.push(Admitted {
            tx_hash: r
                .text(ls::INTENT_ID)
                .ok()
                .and_then(parse_hex32)
                .ok_or_else(bad)?,
            kind,
            component: r
                .text(ls::COMPONENT)
                .ok()
                .and_then(ComponentId::parse)
                .ok_or_else(bad)?,
            grant_generation: r.u64(ls::GRANT_GENERATION).map_err(|_| bad())?,
            input_wei: r
                .u256(ls::INPUT_WEI)
                .ok()
                .and_then(wei_u128)
                .ok_or_else(bad)?,
            max_wei: r
                .u256(ls::MAX_WEI)
                .ok()
                .and_then(wei_u128)
                .ok_or_else(bad)?,
            token: r.opt_address(ls::TOKEN).map_err(|_| bad())?,
            nonce: r.opt_u64(ls::NONCE).map_err(|_| bad())?,
            max_block: r.opt_u64(ls::MAX_BLOCK).map_err(|_| bad())?,
        });
    }
    Ok(out)
}

/// Proof that the owner's LIVE gate was clear, at one moment, for the
/// runtime, the grant's venue and the components armed: what arms a LIVE
/// end and loads its key. Only [`Store::live_now`] makes one; every intent
/// is admitted again under it ([`Store::admit_live`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveNow {
    grant: LiveGrant,
    grant_generation: u64,
    at_ms: u64,
}

impl LiveNow {
    pub fn grant(&self) -> &LiveGrant {
        &self.grant
    }

    /// The owner generation that wrote the grant.
    pub fn grant_generation(&self) -> u64 {
        self.grant_generation
    }

    pub fn at_ms(&self) -> u64 {
        self.at_ms
    }
}

/// What the owner admitted: a BUY's daily and campaign slot, and what the
/// grant has spent with this intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveAdmission {
    pub slots: Option<(BuySlot, TotalSlot)>,
    pub spent_wei: u128,
}

/// [`Store::consume_buy_attempt`] on the canonical owner state.
pub fn consume_buy_attempt_here(attempt_id: &str, now_ms: u64) -> Result<BuySlot, String> {
    let dir = canonical_state_dir().map_err(|e| format!("BUY_ATTEMPT_LIMIT_UNREADABLE: {e}"))?;
    Store::open(dir, KEEP_GENERATIONS)?.consume_buy_attempt(attempt_id, now_ms)
}

/// Source-mode generations the store keeps besides the current one (one
/// value for every caller; Paket 8 review L5).
pub const KEEP_GENERATIONS: u64 = 64;
impl Store {
    pub fn open(dir: impl Into<PathBuf>, keep: u64) -> Result<Self, String> {
        Self::open_in_session(dir, keep, boot_session())
    }

    /// A store whose writes are stamped with the boot session `boot`
    /// (tests simulate a restart with another session).
    pub fn open_in_session(
        dir: impl Into<PathBuf>,
        keep: u64,
        boot: Result<String, String>,
    ) -> Result<Self, String> {
        let dir = dir.into();
        for d in [dir.clone(), dir.join("generations"), dir.join("receipts")] {
            create_durable_dir(&d)?;
            if !fs::symlink_metadata(&d).is_ok_and(|m| m.is_dir()) {
                return Err(format!("{} is not a real directory", d.display()));
            }
        }
        Ok(Store { dir, keep, boot })
    }

    fn generation_path(&self, n: u64) -> PathBuf {
        self.dir.join("generations").join(format!("{n:020}.dbrec"))
    }

    /// The owner state directory (the LIVE ledger lives under it).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn boot(&self) -> Result<&str, String> {
        self.boot.as_deref().map_err(|e| e.clone())
    }

    /// The highest generation on file, or generation 0 when there is none.
    pub fn current(&self) -> Result<Generation, String> {
        current_generation(&self.dir)
    }

    /// Writes generation 0 (everything OFF and DISABLED) unless a generation
    /// exists: from then on the owner's state is on file for the status
    /// projection.
    pub fn init(&self, now_ms: u64) -> Result<Generation, String> {
        let current = self.current()?;
        if self.highest()?.is_none() {
            match write_new(&self.generation_path(0), &current.encode(now_ms)?) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("generation 0 not written: {e}")),
            }
        }
        self.current()
    }

    /// The owner's start (darkbotctl start/restart, the runtime's start or
    /// its crash restart): [`Store::init`], then every ENABLED switch
    /// DISABLED in a new generation (`RESTART_RESET`). Sources, components
    /// and venues keep their modes.
    pub fn restart(&self, now_ms: u64) -> Result<Generation, String> {
        let boot = self.boot()?.to_string();
        let mut current = self.init(now_ms)?;
        // Each lost race means another writer advanced; then again.
        while current.any_enabled() {
            let mut next = current.clone();
            next.generation = current
                .generation
                .checked_add(1)
                .ok_or("generation exhausted")?;
            next.request_id = Some(RESTART_RESET.into());
            next.reset_switches(next.generation);
            next.boot = Some(boot.clone());
            match write_new(
                &self.generation_path(next.generation),
                &next.encode(now_ms)?,
            ) {
                Ok(()) => {
                    self.prune(next.generation);
                    return Ok(next);
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => current = self.current()?,
                Err(e) => return Err(format!("restart reset not written: {e}")),
            }
        }
        Ok(current)
    }

    fn highest(&self) -> Result<Option<u64>, String> {
        highest_generation(&self.dir)
    }

    /// Takes one slot of the daily BUY attempt limit for `attempt_id` in
    /// the UTC day of `now_ms` (plan 27.10., `8b4721bb`) and one of the
    /// campaign's total (plan 28.10.): refused when either limit in force
    /// (0 initially) is used up. The same attempt again gets its slots
    /// (retry, crash before the caller went on). Slots are create-new files,
    /// so concurrent attempts never share one; a lost race or a crash can
    /// only over-count, never under-count.
    pub fn consume_buy_attempt(&self, attempt_id: &str, now_ms: u64) -> Result<BuySlot, String> {
        self.consume_attempt(attempt_id, 0, now_ms)
            .map(|(day, _)| day)
    }

    /// [`Store::consume_buy_attempt`] for an attempt under the LIVE grant
    /// written at `grant_generation` (0: none).
    pub fn consume_attempt(
        &self,
        attempt_id: &str,
        grant_generation: u64,
        now_ms: u64,
    ) -> Result<(BuySlot, TotalSlot), String> {
        let day = self.consume_daily(attempt_id, now_ms)?;
        let total = self.consume_total(attempt_id, grant_generation, now_ms)?;
        Ok((day, total))
    }

    /// Admits one LIVE intent as the owner's state holds now, immediately
    /// before its dispatch (plan 28.10.): the grant is still the one the
    /// executor was armed under; the runtime, the intent's component and
    /// the grant's venue are effective LIVE (an exit: without the BUY
    /// attempt reasons); a BUY's input is within the grant; the intent's
    /// most fits the grant's spend. Then a BUY takes its daily and campaign
    /// slot, and every intent writes its spend entry, create-new: both are
    /// durable before any byte can leave. Each refusal names its reason.
    /// The same transaction is never admitted twice (no retry).
    pub fn admit_live(
        &self,
        armed: &LiveNow,
        intent: &LiveIntent<'_>,
        qualification: &LiveQualification,
        now_ms: u64,
    ) -> Result<LiveAdmission, String> {
        use darkbot_bsc_record::schema::bsc_runtime::live_spend as ls;
        let g = self.current()?;
        let spent = spent_by_grant(&self.dir, armed.grant_generation).ok();
        let (daily, total) = (
            buy_usage(&self.dir, now_ms).ok(),
            total_usage(&self.dir).ok(),
        );
        let ctx = LiveContext {
            boot: self.boot().ok(),
            now_unix_ms: now_ms,
            daily: daily.as_ref(),
            total: total.as_ref(),
            spent_wei: spent,
            qualification,
        };
        let grant = match (&g.live, &g.live_set_by) {
            (Some(grant), Some(by))
                if by.generation == armed.grant_generation && *grant == armed.grant =>
            {
                grant
            }
            _ => return Err(LIVE_GRANT_CHANGED.into()),
        };
        let exit = intent.kind != LiveIntentKind::Buy;
        let entities = [
            Entity::Runtime,
            Entity::Component(intent.component),
            Entity::Venue(grant.venue),
        ];
        g.live_clear(&entities, &ctx, exit)?;
        if !exit && intent.input_wei > grant.max_input_wei {
            return Err(LIVE_INPUT_ABOVE_GRANT.into());
        }
        let spent = spent.ok_or(LIVE_SPEND_UNREADABLE)?;
        let after = spent
            .checked_add(intent.max_wei)
            .filter(|after| *after <= grant.max_spend_wei)
            .ok_or(LIVE_GRANT_SPEND_EXHAUSTED)?;
        let entry = spend_dir(&self.dir).join(format!("{}.dbrec", intent.id));
        if parse_hex32(intent.id).is_none() {
            return Err("a LIVE intent is named by its transaction hash".into());
        }
        if entry.exists() {
            return Err(LIVE_INTENT_SEEN.into());
        }
        let slots = match exit {
            false => Some(self.consume_attempt(intent.id, armed.grant_generation, now_ms)?),
            true => None,
        };
        let mut d = Draft::new(&ls::SCHEMA);
        d.set(ls::INTENT_ID, intent.id)
            .set(ls::KIND, intent.kind.name())
            .set(ls::COMPONENT, intent.component.name())
            .set(ls::PROVENANCE_TOOL, "bsc_runtime")
            .set(ls::PROVENANCE_OBSERVED_UNIX_MS, now_ms)
            .set(ls::GRANT_GENERATION, armed.grant_generation)
            .set(
                ls::INPUT_WEI,
                darkbot_bsc_record::Value::u128(intent.input_wei),
            )
            .set(ls::MAX_WEI, darkbot_bsc_record::Value::u128(intent.max_wei))
            .set(ls::TOKEN, darkbot_bsc_record::Value::Address(intent.token))
            .set(ls::NONCE, intent.nonce)
            .set(ls::MAX_BLOCK, intent.max_block);
        let bytes = d.encode().map_err(|e| e.to_string())?;
        create_durable_dir(&spend_dir(&self.dir))?;
        match write_new(&entry, &bytes) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                return Err(LIVE_INTENT_SEEN.into());
            }
            Err(e) => return Err(format!("{LIVE_SPEND_UNREADABLE}: entry not written: {e}")),
        }
        Ok(LiveAdmission {
            slots,
            spent_wei: after,
        })
    }

    /// The owner's LIVE gate as it holds now for the runtime, the grant's
    /// venue and `components` (plan 28.10.): the proof a LIVE end is armed
    /// with, or the first reason it is not.
    pub fn live_now(
        &self,
        components: &[ComponentId],
        qualification: &LiveQualification,
        now_ms: u64,
    ) -> Result<LiveNow, String> {
        self.live_now_with(components, qualification, now_ms, false)
    }

    /// [`Store::live_now`] for the exits of positions the grant opened
    /// (BSC3-071): without the BUY attempt and spend reasons an exit does
    /// not need ([`Store::admit_live`] admits an exit the same way). A
    /// revoked or expired grant, a switch off or a restart still refuse.
    pub fn live_now_exits(
        &self,
        components: &[ComponentId],
        qualification: &LiveQualification,
        now_ms: u64,
    ) -> Result<LiveNow, String> {
        self.live_now_with(components, qualification, now_ms, true)
    }

    fn live_now_with(
        &self,
        components: &[ComponentId],
        qualification: &LiveQualification,
        now_ms: u64,
        exit: bool,
    ) -> Result<LiveNow, String> {
        let g = self.current()?;
        let (grant, by) = match (&g.live, &g.live_set_by) {
            (Some(grant), Some(by)) => (grant, by),
            _ => return Err(LIVE_GRANT_MISSING.into()),
        };
        let (daily, total) = (
            buy_usage(&self.dir, now_ms).ok(),
            total_usage(&self.dir).ok(),
        );
        let ctx = LiveContext {
            boot: self.boot().ok(),
            now_unix_ms: now_ms,
            daily: daily.as_ref(),
            total: total.as_ref(),
            spent_wei: spent_by_grant(&self.dir, by.generation).ok(),
            qualification,
        };
        let entities: Vec<Entity> = [Entity::Runtime, Entity::Venue(grant.venue)]
            .into_iter()
            .chain(components.iter().map(|c| Entity::Component(*c)))
            .collect();
        g.live_clear(&entities, &ctx, exit)?;
        Ok(LiveNow {
            grant: grant.clone(),
            grant_generation: by.generation,
            at_ms: now_ms,
        })
    }

    fn consume_total(
        &self,
        attempt_id: &str,
        grant_generation: u64,
        now_ms: u64,
    ) -> Result<TotalSlot, String> {
        use darkbot_bsc_record::schema::bsc_runtime::buy_attempt_total_slot as ts;
        let g = self.current()?;
        let limit = g.buy_attempts_total();
        create_durable_dir(&total_dir(&self.dir))?;
        let taken = total_slots(&self.dir)?;
        if let Some((slot, _, _)) = taken
            .iter()
            .find(|(_, a, _)| a.as_deref() == Some(attempt_id))
        {
            return Ok(TotalSlot { slot: *slot, limit });
        }
        for slot in 0..u64::from(limit) {
            let mut d = Draft::new(&ts::SCHEMA);
            d.set(ts::ATTEMPT_ID, attempt_id)
                .set(ts::SLOT, slot)
                .set(ts::LIMIT, u64::from(limit))
                .set(ts::GENERATION, g.generation)
                .set(ts::GRANT_GENERATION, grant_generation)
                .set(ts::PROVENANCE_TOOL, "bsc_runtime")
                .set(ts::PROVENANCE_OBSERVED_UNIX_MS, now_ms);
            let bytes = d.encode().map_err(|e| e.to_string())?;
            match write_new(
                &total_dir(&self.dir).join(format!("{slot:010}.dbrec")),
                &bytes,
            ) {
                Ok(()) => return Ok(TotalSlot { slot, limit }),
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("total buy attempt slot not written: {e}")),
            }
        }
        Err(format!(
            "BUY_ATTEMPT_TOTAL_REACHED: {} of {limit} BUY attempts of the campaign taken at generation {}",
            taken.len().max(limit as usize),
            g.generation
        ))
    }

    fn consume_daily(&self, attempt_id: &str, now_ms: u64) -> Result<BuySlot, String> {
        use darkbot_bsc_record::schema::bsc_runtime::buy_attempt_slot as bs;
        // The G0.7 runner's attempt ID rule, exactly (the slot files are
        // named by their number, the ID is only their content).
        if !(1..=128).contains(&attempt_id.len())
            || attempt_id.starts_with('.')
            || !attempt_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err("the attempt ID is not a valid attempt identifier".into());
        }
        let g = self.current()?;
        let limit = g.buy_attempts_per_day();
        let day = BuyDay::at(now_ms);
        let day_dir = buy_day_dir(&self.dir, &day);
        for d in [self.dir.join("buy-attempts"), day_dir.clone()] {
            create_durable_dir(&d)?;
        }
        let taken = buy_slots(&day_dir)?;
        if let Some((slot, _)) = taken.iter().find(|(_, a)| a.as_deref() == Some(attempt_id)) {
            return Ok(BuySlot {
                day,
                slot: *slot,
                limit,
            });
        }
        for slot in 0..u64::from(limit) {
            let mut d = Draft::new(&bs::SCHEMA);
            d.set(bs::ATTEMPT_ID, attempt_id)
                .set(bs::DAY_UTC, day.utc.as_str())
                .set(bs::DAY_INDEX, day.index)
                .set(bs::SLOT, slot)
                .set(bs::LIMIT, u64::from(limit))
                .set(bs::GENERATION, g.generation)
                .set(bs::PROVENANCE_TOOL, "bsc_runtime")
                .set(bs::PROVENANCE_OBSERVED_UNIX_MS, now_ms);
            let bytes = d.encode().map_err(|e| e.to_string())?;
            match write_new(&day_dir.join(format!("{slot:010}.dbrec")), &bytes) {
                Ok(()) => return Ok(BuySlot { day, slot, limit }),
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("buy attempt slot not written: {e}")),
            }
        }
        Err(format!(
            "BUY_ATTEMPT_LIMIT_REACHED: {} of {limit} BUY attempts on {} (UTC) taken at generation {}",
            taken.len().max(limit as usize),
            day.utc,
            g.generation
        ))
    }

    /// Applies one request (or replays its receipt). `now_ms` is the record
    /// time; a clock fault is the caller's refusal, never time 0. A LIVE
    /// grant's scope comes only through [`Store::grant_live`]; `DISABLED` on
    /// `BSC_LIVE_GRANT` revokes it.
    pub fn request(
        &self,
        request_id: &str,
        entity: impl Into<Entity>,
        requested: Mode,
        expected_generation: u64,
        now_ms: u64,
    ) -> Result<Receipt, String> {
        if let Mode::Granted(_) = requested {
            return Err("a LIVE grant's scope comes through `live grant`".into());
        }
        self.apply(
            request_id,
            entity.into(),
            requested,
            None,
            expected_generation,
            now_ms,
        )
    }

    /// Grants LIVE for `grant`'s scope (plan 28.10.): refused unless the
    /// scope is valid and `confirmed_sha256` is its scope text's SHA-256,
    /// the one the operator confirmed. A later grant replaces it.
    pub fn grant_live(
        &self,
        request_id: &str,
        grant: LiveGrant,
        confirmed_sha256: [u8; 32],
        expected_generation: u64,
        now_ms: u64,
    ) -> Result<Receipt, String> {
        grant.validate()?;
        let digest = grant.scope_sha256();
        if digest != confirmed_sha256 {
            return Err(format!(
                "the confirmed scope {} is not this grant's scope {}",
                hex32(&confirmed_sha256),
                hex32(&digest)
            ));
        }
        self.apply(
            request_id,
            Entity::LiveGrant,
            Mode::Granted(digest),
            Some(grant),
            expected_generation,
            now_ms,
        )
    }

    fn apply(
        &self,
        request_id: &str,
        entity: Entity,
        requested: Mode,
        grant: Option<LiveGrant>,
        expected_generation: u64,
        now_ms: u64,
    ) -> Result<Receipt, String> {
        if !safe_id(request_id) {
            return Err("the request ID is not a safe identifier".into());
        }
        if !entity.accepts(requested) {
            return Err(format!("{} takes {}", entity.name(), entity.modes()));
        }
        let receipt_path = self
            .dir
            .join("receipts")
            .join(format!("{request_id}.dbrec"));
        let same = |r: &Receipt| {
            r.entity == entity
                && r.requested == requested
                && r.expected_generation == expected_generation
        };
        match fs::read(&receipt_path) {
            Ok(bytes) => {
                let r = Receipt::decode(&bytes)?;
                return if same(&r) {
                    Ok(r)
                } else {
                    Err("the request ID already belongs to another request".into())
                };
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(format!("receipt unreadable: {e}")),
        }
        let current = self.current()?;
        let mut receipt = Receipt {
            request_id: request_id.into(),
            entity,
            requested,
            expected_generation,
            outcome: "REJECTED_STALE",
            generation: current.generation,
            effective: current.effective_of(entity),
        };
        if current.request_id.as_deref() == Some(request_id) {
            // Crash recovery: the generation names this request, the receipt
            // was lost; only the same payload is that request.
            if !current.applied(request_id, entity, requested) {
                return Err("the request ID already belongs to another request".into());
            }
            receipt.outcome = "APPLIED";
        } else if current.generation == expected_generation {
            let boot = self.boot()?.to_string();
            let mut next = current.clone();
            next.generation = current
                .generation
                .checked_add(1)
                .ok_or("generation exhausted")?;
            next.request_id = Some(request_id.into());
            // A switch ENABLED in another boot session never carries over.
            if current.boot.as_deref() != Some(boot.as_str()) {
                next.reset_switches(next.generation);
            }
            next.boot = Some(boot);
            let set_by = SetBy {
                request_id: request_id.into(),
                generation: next.generation,
            };
            match grant {
                Some(grant) => next.set_live(Some(grant), set_by),
                None => next.set(entity, requested, set_by),
            }
            match write_new(
                &self.generation_path(next.generation),
                &next.encode(now_ms)?,
            ) {
                Ok(()) => {
                    receipt.outcome = "APPLIED";
                    receipt.generation = next.generation;
                    receipt.effective = next.effective_of(entity);
                }
                // Another writer took this generation first: stale, unless it
                // was this very request (an identical twin, e.g. CLI and GUI).
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    let now = self.current()?;
                    if now.request_id.as_deref() == Some(request_id)
                        && now.applied(request_id, entity, requested)
                    {
                        receipt.outcome = "APPLIED";
                    }
                    receipt.generation = now.generation;
                    receipt.effective = now.effective_of(entity);
                }
                Err(e) => return Err(format!("generation not written: {e}")),
            }
        }
        match write_new(&receipt_path, &receipt.encode(now_ms)?) {
            Ok(()) => {}
            // A concurrent identical request wrote it first: its receipt counts.
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                let r = Receipt::decode(&fs::read(&receipt_path).map_err(|e| e.to_string())?)?;
                return if same(&r) {
                    Ok(r)
                } else {
                    Err("the request ID already belongs to another request".into())
                };
            }
            Err(e) => return Err(format!("receipt not written: {e}")),
        }
        if receipt.applied() {
            self.prune(receipt.generation);
        }
        Ok(receipt)
    }

    /// Removes generations older than `keep` before `current` (best effort).
    fn prune(&self, current: u64) {
        let Some(below) = current.checked_sub(self.keep) else {
            return;
        };
        if let Ok(entries) = fs::read_dir(self.dir.join("generations")) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if let Some(n) = name
                    .to_string_lossy()
                    .strip_suffix(".dbrec")
                    .and_then(|n| n.parse::<u64>().ok())
                    && n < below
                {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// The highest generation number on file in `dir/generations`.
fn highest_generation(dir: &Path) -> Result<Option<u64>, String> {
    let mut highest: Option<u64> = None;
    for entry in fs::read_dir(dir.join("generations")).map_err(|e| e.to_string())? {
        let name = entry.map_err(|e| e.to_string())?.file_name();
        if let Some(n) = name
            .to_string_lossy()
            .strip_suffix(".dbrec")
            .and_then(|n| n.parse::<u64>().ok())
        {
            highest = highest.max(Some(n));
        }
    }
    Ok(highest)
}

/// Read-only, for the status projection: the current generation's file and
/// record, validated against the owner's rule; `None` before `init`.
pub fn current_record(dir: &Path) -> Result<Option<(PathBuf, Record)>, String> {
    Ok(current_record_and_generation(dir)?.map(|(path, record, _)| (path, record)))
}

/// The current generation's record and the generation it decodes to, from
/// one read of the file.
pub fn current_record_and_generation(
    dir: &Path,
) -> Result<Option<(PathBuf, Record, Generation)>, String> {
    let Some(n) = highest_generation(dir)? else {
        return Ok(None);
    };
    let path = dir.join("generations").join(format!("{n:020}.dbrec"));
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let g = Generation::decode(&bytes)?;
    if g.generation != n {
        return Err(format!("generation file {n} holds another generation"));
    }
    let record = decode_generation(&bytes)?;
    Ok(Some((path, record, g)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn store(tag: &str) -> (PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!("bsc-sources-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = Store::open(&dir, 8).unwrap();
        (dir, s)
    }

    /// PHASENPLAN §10 / V2 §5: the C2 state component runs in SHADOW, and a
    /// requested LIVE is recorded but never effective.
    #[test]
    fn the_c2_component_is_switchable_and_never_operationally_live() {
        let (dir, s) = store("c2");
        let c2 = ComponentId::StateC2;
        assert_eq!(Entity::parse("BSC_STATE_C2"), Some(Entity::Component(c2)));
        assert_eq!(
            component_effective(c2, Mode::Off, LiveGate::Disabled),
            Effective::Off
        );
        assert_eq!(
            component_effective(c2, Mode::Shadow, LiveGate::Disabled),
            Effective::Shadow
        );
        let r = s.request("c2-1", c2, Mode::Shadow, 0, 1).unwrap();
        assert!(r.applied());
        assert_eq!(
            s.current().unwrap().component(c2).effective,
            Effective::Shadow
        );
        let r = s.request("c2-2", c2, Mode::Live, 1, 2).unwrap();
        assert!(r.applied());
        assert_eq!(
            r.effective,
            Effective::Blocked(C2_OPERATIONAL_NOT_AUTHORIZED)
        );
        assert!(
            r.line()
                .contains("source=BSC_STATE_C2 requested=LIVE effective=BLOCKED")
        );
        // The sources are untouched by a component request.
        assert!(
            s.current()
                .unwrap()
                .sources
                .iter()
                .all(|m| m.requested == Mode::Off)
        );
        // A stale component request changes nothing.
        let stale = s.request("c2-3", c2, Mode::Off, 1, 3).unwrap();
        assert_eq!(stale.outcome, "REJECTED_STALE");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// C3: the execution component runs only to the PAPER sink; a
    /// requested LIVE is recorded and never effective.
    #[test]
    fn the_exec_component_is_paper_only_and_never_live() {
        let (dir, s) = store("exec");
        let exec = ComponentId::ExecC3;
        assert_eq!(Entity::parse("BSC_EXEC"), Some(Entity::Component(exec)));
        let r = s
            .request("e-1", ComponentId::StateC2, Mode::Shadow, 0, 1)
            .unwrap();
        assert!(r.applied());
        // A C2 request leaves the exec component OFF.
        assert_eq!(
            s.current().unwrap().component(exec).effective,
            Effective::Off
        );
        let r = s.request("e-2", exec, Mode::Shadow, 1, 2).unwrap();
        assert_eq!(r.effective, Effective::Shadow);
        let r = s.request("e-3", exec, Mode::Live, 2, 3).unwrap();
        assert!(r.applied());
        assert_eq!(r.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
        assert!(
            r.line()
                .contains("source=BSC_EXEC requested=LIVE effective=BLOCKED")
        );
        // C2 untouched by the exec request.
        assert_eq!(
            s.current()
                .unwrap()
                .component(ComponentId::StateC2)
                .effective,
            Effective::Shadow
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Phase H: Strategy C runs as a PAPER ledger in SHADOW; LIVE is
    /// recorded and blocked; a generation written before it reads it OFF.
    #[test]
    fn the_strategy_component_is_paper_only_and_never_live() {
        let (dir, s) = store("strategy");
        let strategy = ComponentId::StrategyC;
        assert_eq!(
            Entity::parse("BSC_STRATEGY_C"),
            Some(Entity::Component(strategy))
        );
        let r = s
            .request("s-1", ComponentId::ExecC3, Mode::Shadow, 0, 1)
            .unwrap();
        assert!(r.applied());
        assert_eq!(
            s.current().unwrap().component(strategy).effective,
            Effective::Off
        );
        // A generation of the two earlier components: the strategy reads OFF.
        let (_, two) = current_record(&dir).unwrap().unwrap();
        let mut d = Draft::new(&sm::SCHEMA);
        d.set(sm::GENERATION, 2u64)
            .set(sm::PROVENANCE_TOOL, "bsc_runtime")
            .set(sm::PROVENANCE_OBSERVED_UNIX_MS, 2u64);
        for row in two.nested_all(sm::SOURCES).unwrap() {
            let mut e = Draft::new(&sm::entry::SCHEMA);
            for tag in [
                sm::entry::SOURCE_ID,
                sm::entry::REQUESTED,
                sm::entry::EFFECTIVE,
            ] {
                e.set(tag, row.text(tag).unwrap());
            }
            d.push(sm::SOURCES, e.into_nested());
        }
        for row in two.nested_all(sm::COMPONENTS).unwrap() {
            if row.text(sm::component::COMPONENT_ID).unwrap() == "BSC_STRATEGY_C" {
                continue;
            }
            let mut e = Draft::new(&sm::component::SCHEMA);
            for tag in [
                sm::component::COMPONENT_ID,
                sm::component::REQUESTED,
                sm::component::EFFECTIVE,
            ] {
                e.set(tag, row.text(tag).unwrap());
            }
            d.push(sm::COMPONENTS, e.into_nested());
        }
        fs::write(
            dir.join("generations").join(format!("{:020}.dbrec", 2)),
            d.encode().unwrap(),
        )
        .unwrap();
        let g = s.current().unwrap();
        assert_eq!(g.generation, 2);
        assert_eq!(
            g.component(ComponentId::ExecC3).effective,
            Effective::Shadow
        );
        assert_eq!(g.component(strategy).effective, Effective::Off);
        let r = s.request("s-2", strategy, Mode::Shadow, 2, 3).unwrap();
        assert_eq!(r.effective, Effective::Shadow);
        let r = s.request("s-3", strategy, Mode::Live, 3, 4).unwrap();
        assert!(r.applied());
        assert_eq!(r.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
        assert!(
            r.line()
                .contains("source=BSC_STRATEGY_C requested=LIVE effective=BLOCKED")
        );
        assert_eq!(
            s.current()
                .unwrap()
                .component(ComponentId::ExecC3)
                .effective,
            Effective::Shadow
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A generation written before components existed (schema version 1)
    /// still reads, with every component OFF.
    #[test]
    fn a_generation_from_before_components_reads_with_them_off() {
        let (dir, s) = store("v1");
        let mut d = Draft::new(&sm::SCHEMA_V1);
        d.set(sm::GENERATION, 0u64)
            .set(sm::PROVENANCE_TOOL, "bsc_runtime")
            .set(sm::PROVENANCE_OBSERVED_UNIX_MS, 1u64);
        for source in SourceId::ALL {
            let mut e = Draft::new(&sm::entry::SCHEMA);
            e.set(sm::entry::SOURCE_ID, source.name())
                .set(sm::entry::REQUESTED, "OFF")
                .set(sm::entry::EFFECTIVE, "OFF");
            d.push(sm::SOURCES, e.into_nested());
        }
        fs::write(
            dir.join("generations").join(format!("{:020}.dbrec", 0)),
            d.encode().unwrap(),
        )
        .unwrap();
        let g = s.current().unwrap();
        assert_eq!(g.component(ComponentId::StateC2).effective, Effective::Off);
        assert!(current_record(&dir).unwrap().is_some());
        // The next request writes the current schema with the component.
        s.request("after-v1", ComponentId::StateC2, Mode::Shadow, 0, 2)
            .unwrap();
        let (_, r) = current_record(&dir).unwrap().unwrap();
        assert_eq!(r.schema().schema_version, sm::SCHEMA.schema_version);
        assert_eq!(
            r.nested_all(sm::COMPONENTS).unwrap().len(),
            ComponentId::ALL.len()
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// V2 §5: no request makes a source economically live; 48Club is
    /// blocked even in SHADOW; OFF and SHADOW are effective as requested.
    #[test]
    fn the_owner_never_makes_a_source_live() {
        for s in SourceId::ALL {
            assert_eq!(effective(s, Mode::Off, LiveGate::Disabled), Effective::Off);
            assert_eq!(
                effective(s, Mode::Live, LiveGate::Disabled),
                Effective::Blocked(ECONOMIC_LIVE_DISABLED)
            );
            assert_eq!(
                effective(s, Mode::Live, LiveGate::Enabled),
                Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED)
            );
        }
        assert_eq!(
            effective(SourceId::PublicNode, Mode::Shadow, LiveGate::Disabled),
            Effective::Shadow
        );
        assert_eq!(
            effective(SourceId::Club48, Mode::Shadow, LiveGate::Disabled),
            Effective::Blocked(SOURCE_ACCESS_UNQUALIFIED)
        );
    }

    #[test]
    fn a_request_applies_with_its_generation_and_is_idempotent() {
        let (dir, s) = store("apply");
        assert_eq!(s.current().unwrap(), Generation::initial());
        let r = s
            .request("req-1", SourceId::PublicNode, Mode::Shadow, 0, 1)
            .unwrap();
        assert_eq!(
            (r.outcome, r.generation, r.effective),
            ("APPLIED", 1, Effective::Shadow)
        );
        // The same request again: the same receipt, no second generation.
        assert_eq!(
            s.request("req-1", SourceId::PublicNode, Mode::Shadow, 0, 2)
                .unwrap(),
            r
        );
        assert_eq!(s.current().unwrap().generation, 1);
        // Another payload under the same ID is refused.
        assert!(
            s.request("req-1", SourceId::Drpc, Mode::Shadow, 0, 3)
                .is_err()
        );
        // LIVE is recorded as requested and blocked as effective.
        let r = s
            .request("req-2", SourceId::PublicNode, Mode::Live, 1, 4)
            .unwrap();
        assert_eq!(r.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
        let g = s.current().unwrap();
        assert_eq!(g.generation, 2);
        assert_eq!(g.source(SourceId::PublicNode).requested, Mode::Live);
        assert_eq!(g.source(SourceId::Drpc).requested, Mode::Off);
        fs::remove_dir_all(dir).unwrap();
    }

    /// `init` puts generation 0 on file once; the projection reads it.
    #[test]
    fn init_puts_generation_zero_on_file_once() {
        let (dir, s) = store("init");
        assert_eq!(current_record(&dir).unwrap().map(|(p, _)| p), None);
        assert_eq!(s.init(1).unwrap(), Generation::initial());
        let (path, record) = current_record(&dir).unwrap().unwrap();
        assert!(path.ends_with(format!("generations/{:020}.dbrec", 0)));
        assert_eq!(record.u64(sm::GENERATION).unwrap(), 0);
        s.request("i", SourceId::Drpc, Mode::Shadow, 0, 2).unwrap();
        assert_eq!(s.init(3).unwrap().generation, 1, "init never rewinds");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_stale_generation_is_refused_and_the_state_survives_a_restart() {
        let (dir, s) = store("stale");
        s.request("a", SourceId::Drpc, Mode::Shadow, 0, 1).unwrap();
        let r = s
            .request("b", SourceId::BlockReq, Mode::Shadow, 0, 2)
            .unwrap();
        assert_eq!((r.outcome, r.generation), ("REJECTED_STALE", 1));
        assert_eq!(r.effective, Effective::Off, "BlockReq unchanged");
        // A new process (restart) reads the same state.
        let again = Store::open(&dir, 8).unwrap();
        assert_eq!(again.current().unwrap(), s.current().unwrap());
        fs::remove_dir_all(dir).unwrap();
    }

    /// Two writers of the same next generation: create-new lets one win.
    #[test]
    fn concurrent_writers_of_one_generation_have_one_winner() {
        let (dir, s) = store("race");
        let outcomes: Vec<&str> = std::thread::scope(|scope| {
            let runs: Vec<_> = (0..6)
                .map(|i| {
                    let s = &s;
                    scope.spawn(move || {
                        s.request(&format!("r{i}"), SourceId::Geth, Mode::Shadow, 0, 1)
                            .unwrap()
                            .outcome
                    })
                })
                .collect();
            runs.into_iter().map(|r| r.join().unwrap()).collect()
        });
        assert_eq!(
            outcomes.iter().filter(|o| **o == "APPLIED").count(),
            1,
            "{outcomes:?}"
        );
        assert_eq!(s.current().unwrap().generation, 1);
        fs::remove_dir_all(dir).unwrap();
    }

    /// A crash after the new generation but before its receipt: the replay
    /// of the same request reports APPLIED from the generation.
    #[test]
    fn a_lost_receipt_is_recovered_from_the_generation() {
        let (dir, s) = store("crash");
        s.request("x", SourceId::Geth, Mode::Shadow, 0, 1).unwrap();
        fs::remove_file(dir.join("receipts/x.dbrec")).unwrap();
        let r = s.request("x", SourceId::Geth, Mode::Shadow, 0, 2).unwrap();
        assert_eq!((r.outcome, r.generation), ("APPLIED", 1));
        fs::remove_dir_all(dir).unwrap();
    }

    pub(crate) fn session(tag: &str, boot: &str) -> (PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!("bsc-switch-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = Store::open_in_session(&dir, 8, Ok(boot.into())).unwrap();
        (dir, s)
    }

    /// Plan 27.10.: LAUNCHER and BUYER are independent switches, DISABLED
    /// initially, taking only DISABLED or ENABLED; each records the request
    /// that set it.
    #[test]
    fn the_wallet_switches_are_independent_and_disabled_initially() {
        let (dir, s) = session("wallets", "boot-1");
        let g = s.current().unwrap();
        for id in SwitchId::ALL {
            assert_eq!(g.switch(id).requested, Mode::Disabled);
            assert_eq!(g.switch(id).effective, Effective::Disabled);
        }
        for m in [Mode::Off, Mode::Shadow, Mode::Live] {
            let e = s
                .request("w-bad", SwitchId::WalletLauncher, m, 0, 1)
                .unwrap_err();
            assert!(e.contains("takes DISABLED or ENABLED"), "{e}");
        }
        assert!(
            s.request("w-bad2", SourceId::Drpc, Mode::Enabled, 0, 1)
                .unwrap_err()
                .contains("takes OFF, SHADOW or LIVE")
        );
        let r = s
            .request("w-1", SwitchId::WalletLauncher, Mode::Enabled, 0, 1)
            .unwrap();
        assert_eq!((r.outcome, r.effective), ("APPLIED", Effective::Enabled));
        assert!(r.line().contains(
            "source=BSC_WALLET_LAUNCHER requested=ENABLED effective=ENABLED generation=1"
        ));
        let g = s.current().unwrap();
        assert_eq!(
            g.switch(SwitchId::WalletLauncher).set_by,
            Some(SetBy {
                request_id: "w-1".into(),
                generation: 1
            })
        );
        assert_eq!(
            g.switch(SwitchId::WalletBuyer).effective,
            Effective::Disabled
        );
        assert_eq!(g.boot.as_deref(), Some("boot-1"));
        assert_eq!(
            require_enabled(&dir, "boot-1", &[SwitchId::WalletLauncher]),
            Ok(1)
        );
        let refused = require_enabled(
            &dir,
            "boot-1",
            &[SwitchId::WalletLauncher, SwitchId::WalletBuyer],
        )
        .unwrap_err();
        assert!(
            refused.contains("BSC_WALLET_BUYER DISABLED")
                && !refused.contains("BSC_WALLET_LAUNCHER"),
            "{refused}"
        );
        let r = s
            .request("w-2", SwitchId::WalletLauncher, Mode::Disabled, 1, 2)
            .unwrap();
        assert_eq!(r.effective, Effective::Disabled);
        assert!(require_enabled(&dir, "boot-1", &[SwitchId::WalletLauncher]).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    /// An ENABLED wallet or economic switch makes nothing else effective:
    /// a requested LIVE stays BLOCKED; the global LIVE switch only names
    /// the next missing condition.
    #[test]
    fn a_switch_enables_nothing_else() {
        let (dir, s) = session("nothing-else", "boot-1");
        let mut g = 0;
        for id in SwitchId::ALL {
            if id == SwitchId::Live {
                continue;
            }
            s.request(&format!("e-{g}"), id, Mode::Enabled, g, 1)
                .unwrap();
            g += 1;
        }
        let r = s
            .request("exec-live", ComponentId::ExecC3, Mode::Live, g, 2)
            .unwrap();
        assert_eq!(r.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
        let r = s
            .request("flap-live", VenueId::Flap, Mode::Live, g + 1, 3)
            .unwrap();
        assert_eq!(r.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
        let now = s.current().unwrap();
        assert!(now.sources.iter().all(|m| m.effective == Effective::Off));
        s.request("live-on", SwitchId::Live, Mode::Enabled, g + 2, 4)
            .unwrap();
        let now = s.current().unwrap();
        assert_eq!(
            now.component(ComponentId::ExecC3).effective,
            Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED)
        );
        assert_eq!(
            now.venue(VenueId::Flap).effective,
            Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED)
        );
        // The receipt of the earlier LIVE request keeps the reason it met.
        let again = s
            .request("exec-live", ComponentId::ExecC3, Mode::Live, g, 5)
            .unwrap();
        assert_eq!(again.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
        s.request("live-off", SwitchId::Live, Mode::Disabled, g + 3, 6)
            .unwrap();
        assert_eq!(
            s.current()
                .unwrap()
                .component(ComponentId::ExecC3)
                .effective,
            Effective::Blocked(ECONOMIC_LIVE_DISABLED)
        );
        fs::remove_dir_all(dir).unwrap();
    }

    /// Review L5: after a reboot nobody restarted the owner for, a requested
    /// LIVE names the LIVE switch as it holds in this boot, not as written.
    #[test]
    fn a_live_reason_reads_the_live_switch_of_this_boot() {
        let (dir, s) = session("live-boot", "boot-1");
        s.request("l-on", SwitchId::Live, Mode::Enabled, 0, 1)
            .unwrap();
        s.request("v-live", VenueId::Flap, Mode::Live, 1, 2)
            .unwrap();
        s.request("r-live", Entity::Runtime, Mode::Live, 2, 3)
            .unwrap();
        let g = s.current().unwrap();
        let written = Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED);
        for e in [Entity::Venue(VenueId::Flap), Entity::Runtime] {
            assert_eq!(g.effective_of(e), written);
            assert_eq!(g.effective_now(e, Some("boot-1")), written);
            for boot in [Some("boot-2"), None] {
                assert_eq!(
                    g.effective_now(e, boot),
                    Effective::Blocked(ECONOMIC_LIVE_DISABLED)
                );
            }
        }
        assert_eq!(
            g.effective_now(Entity::Switch(SwitchId::Live), Some("boot-2")),
            Effective::Blocked(RESET_BY_RESTART)
        );
        assert_eq!(
            g.live_reason_now(FOUR_EXECUTION_PATH_MISSING, Some("boot-2")),
            FOUR_EXECUTION_PATH_MISSING
        );
        fs::remove_dir_all(dir).unwrap();
    }

    /// Review pass 3 L3a: a lost receipt replayed as a source or component
    /// request the generation happens to hold is not that request's change
    /// when the request set something else; the genuine replay still is.
    #[test]
    fn a_replay_as_an_unchanged_source_is_not_the_request() {
        let (dir, s) = session("replay-source", "boot-1");
        s.request("x", SwitchId::Arm, Mode::Enabled, 0, 1).unwrap();
        fs::remove_file(dir.join("receipts/x.dbrec")).unwrap();
        let e = s.request("x", SourceId::Drpc, Mode::Off, 0, 2).unwrap_err();
        assert!(e.contains("belongs to another request"), "{e}");
        let e = s
            .request("x", ComponentId::ExecC3, Mode::Off, 0, 3)
            .unwrap_err();
        assert!(e.contains("belongs to another request"), "{e}");
        let genuine = s.request("x", SwitchId::Arm, Mode::Enabled, 0, 4).unwrap();
        assert_eq!((genuine.outcome, genuine.generation), ("APPLIED", 1));
        s.request("y", SourceId::Drpc, Mode::Shadow, 1, 5).unwrap();
        fs::remove_file(dir.join("receipts/y.dbrec")).unwrap();
        let again = s.request("y", SourceId::Drpc, Mode::Shadow, 1, 6).unwrap();
        assert_eq!((again.outcome, again.generation), ("APPLIED", 2));
        fs::remove_dir_all(dir).unwrap();
    }

    /// Plan v9 §2.3: Flap is admitted in SHADOW; Four.meme has no path to
    /// admit to, so its SHADOW is BLOCKED with that reason.
    #[test]
    fn the_venues_are_switchable_and_four_has_no_path_yet() {
        let (dir, s) = session("venues", "boot-1");
        assert_eq!(
            Entity::parse("BSC_VENUE_FLAP"),
            Some(Entity::Venue(VenueId::Flap))
        );
        let r = s.request("v-1", VenueId::Flap, Mode::Shadow, 0, 1).unwrap();
        assert_eq!(r.effective, Effective::Shadow);
        for (i, v) in [VenueId::FourClassic, VenueId::OpenFour]
            .into_iter()
            .enumerate()
        {
            let r = s
                .request(&format!("v-four-{i}"), v, Mode::Shadow, 1 + i as u64, 2)
                .unwrap();
            assert_eq!(r.effective, Effective::Blocked(FOUR_EXECUTION_PATH_MISSING));
        }
        let g = s.current().unwrap();
        assert_eq!(g.venue(VenueId::Flap).effective, Effective::Shadow);
        assert_eq!(
            g.venue(VenueId::OpenFour).set_by,
            Some(SetBy {
                request_id: "v-four-1".into(),
                generation: 3
            })
        );
        fs::remove_dir_all(dir).unwrap();
    }

    /// Plan 27.10.: every restart disables the switches; sources, components
    /// and venues keep their modes; a restart with nothing ENABLED writes
    /// nothing.
    #[test]
    fn a_restart_disables_every_switch_and_keeps_the_modes() {
        let (dir, s) = session("restart", "boot-1");
        s.request("r-1", SourceId::PublicNode, Mode::Shadow, 0, 1)
            .unwrap();
        s.request("r-2", SwitchId::WalletBuyer, Mode::Enabled, 1, 2)
            .unwrap();
        s.request("r-3", SwitchId::Send, Mode::Enabled, 2, 3)
            .unwrap();
        let g = s.restart(4).unwrap();
        assert_eq!(g.generation, 4);
        assert_eq!(g.request_id.as_deref(), Some(RESTART_RESET));
        assert!(!g.any_enabled());
        assert_eq!(
            g.switch(SwitchId::WalletBuyer).set_by,
            Some(SetBy {
                request_id: RESTART_RESET.into(),
                generation: 4
            })
        );
        assert_eq!(g.source(SourceId::PublicNode).effective, Effective::Shadow);
        assert_eq!(s.restart(5).unwrap().generation, 4, "nothing to reset");
        // A stale request from before the restart changes nothing.
        let stale = s
            .request("r-old", SwitchId::WalletBuyer, Mode::Enabled, 3, 6)
            .unwrap();
        assert_eq!(stale.outcome, "REJECTED_STALE");
        assert!(require_enabled(&dir, "boot-1", &[SwitchId::WalletBuyer]).is_err());
        // A first start writes generation 0 and nothing else.
        let (fresh, f) = session("restart-fresh", "boot-1");
        assert_eq!(f.restart(1).unwrap(), Generation::initial());
        assert!(current_record(&fresh).unwrap().is_some());
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(fresh).unwrap();
    }

    /// An ENABLED switch counts only in the boot session it was written in:
    /// after a reboot nobody restarted the owner for, it is refused, and the
    /// next write carries it over DISABLED.
    #[test]
    fn a_switch_never_outlives_its_boot_session() {
        let (dir, s) = session("boot", "boot-1");
        s.request("b-1", SwitchId::WalletLauncher, Mode::Enabled, 0, 1)
            .unwrap();
        assert_eq!(
            require_enabled(&dir, "boot-1", &[SwitchId::WalletLauncher]),
            Ok(1)
        );
        let e = require_enabled(&dir, "boot-2", &[SwitchId::WalletLauncher]).unwrap_err();
        assert!(e.contains("BLOCKED (RESET_BY_RESTART)"), "{e}");
        assert_eq!(
            s.current()
                .unwrap()
                .switch_now(SwitchId::WalletLauncher, None),
            Effective::Blocked(RESET_BY_RESTART)
        );
        let after = Store::open_in_session(&dir, 8, Ok("boot-2".into())).unwrap();
        after
            .request("b-2", SourceId::Drpc, Mode::Shadow, 1, 2)
            .unwrap();
        let g = after.current().unwrap();
        assert_eq!(
            g.switch(SwitchId::WalletLauncher).effective,
            Effective::Disabled
        );
        assert_eq!(g.boot.as_deref(), Some("boot-2"));
        assert!(require_enabled(&dir, "boot-2", &[SwitchId::WalletLauncher]).is_err());
        // Without a boot session nothing can be written.
        let blind = Store::open_in_session(&dir, 8, Err("no boot".into())).unwrap();
        assert!(
            blind
                .request("b-3", SwitchId::WalletLauncher, Mode::Enabled, 2, 3)
                .is_err()
        );
        fs::remove_dir_all(dir).unwrap();
    }

    /// A missing state is everything DISABLED; an unreadable or tampered one
    /// refuses every switch.
    #[test]
    fn a_missing_unreadable_or_tampered_state_refuses_every_switch() {
        let missing = std::env::temp_dir().join(format!("bsc-switch-none-{}", std::process::id()));
        let _ = fs::remove_dir_all(&missing);
        let e = require_enabled(&missing, "boot-1", &[SwitchId::Sign]).unwrap_err();
        assert!(e.contains("BSC_ECON_SIGN DISABLED"), "{e}");
        assert!(!missing.exists(), "a check creates nothing");
        let (dir, s) = session("tampered", "boot-1");
        s.request("t-1", SwitchId::Sign, Mode::Enabled, 0, 1)
            .unwrap();
        // Stored ENABLED for a requested DISABLED: not the owner's.
        let mut g = s.current().unwrap();
        g.generation = 2;
        g.switches[2].requested = Mode::Disabled;
        let mut bytes = g.encode(2).unwrap();
        fs::write(dir.join(format!("generations/{:020}.dbrec", 2)), &bytes).unwrap();
        let e = require_enabled(&dir, "boot-1", &[SwitchId::Sign]).unwrap_err();
        assert!(e.starts_with("CONTROL_STATE_UNREADABLE"), "{e}");
        assert!(e.contains("BSC_ECON_SIGN: stored effective state"), "{e}");
        // A torn file refuses as well.
        bytes.truncate(bytes.len() / 2);
        fs::write(dir.join(format!("generations/{:020}.dbrec", 2)), &bytes).unwrap();
        assert!(
            require_enabled(&dir, "boot-1", &[SwitchId::Sign])
                .unwrap_err()
                .starts_with("CONTROL_STATE_UNREADABLE")
        );
        assert!(
            s.request("t-2", SwitchId::Sign, Mode::Enabled, 2, 3)
                .is_err()
        );
        fs::remove_dir_all(dir).unwrap();
    }

    /// Concurrent switch requests and a restart reset race on the CAS: one
    /// writer per generation, and a reset is never lost.
    #[test]
    fn switch_requests_and_a_restart_race_on_the_generation() {
        let (dir, s) = session("switch-race", "boot-1");
        let outcomes: Vec<&str> = std::thread::scope(|scope| {
            let runs: Vec<_> = (0..6)
                .map(|i| {
                    let s = &s;
                    scope.spawn(move || {
                        s.request(
                            &format!("sw{i}"),
                            SwitchId::WalletBuyer,
                            Mode::Enabled,
                            0,
                            1,
                        )
                        .unwrap()
                        .outcome
                    })
                })
                .collect();
            runs.into_iter().map(|r| r.join().unwrap()).collect()
        });
        assert_eq!(outcomes.iter().filter(|o| **o == "APPLIED").count(), 1);
        std::thread::scope(|scope| {
            let s = &s;
            let reset = scope.spawn(move || s.restart(2).unwrap());
            let enable = scope.spawn(move || {
                s.request("sw-late", SwitchId::WalletLauncher, Mode::Enabled, 1, 2)
                    .unwrap()
            });
            reset.join().unwrap();
            enable.join().unwrap();
        });
        let g = s.current().unwrap();
        // The reset ran after (or instead of) every ENABLE it met.
        assert!(
            !g.switch(SwitchId::WalletBuyer)
                .effective
                .eq(&Effective::Enabled)
        );
        if g.switch(SwitchId::WalletLauncher).effective == Effective::Enabled {
            assert_eq!(g.request_id.as_deref(), Some("sw-late"));
        }
        fs::remove_dir_all(dir).unwrap();
    }

    /// Plan v9 §2.5: the global runtime mode, OFF initially; SHADOW and
    /// PAPER effective as requested, LIVE BLOCKED like every LIVE; a
    /// generation from before it reads OFF with a daily BUY limit of 0.
    #[test]
    fn the_runtime_mode_is_off_initially_and_never_live() {
        let (dir, s) = session("runtime", "boot-1");
        let g = s.current().unwrap();
        assert_eq!(
            (g.runtime.effective, g.buy_attempts_per_day()),
            (Effective::Off, 0)
        );
        assert_eq!(Entity::parse(RUNTIME_MODE), Some(Entity::Runtime));
        assert!(
            s.request("rt-bad", Entity::Runtime, Mode::Enabled, 0, 1)
                .unwrap_err()
                .contains("takes OFF, SHADOW, PAPER or LIVE")
        );
        let r = s
            .request("rt-1", Entity::Runtime, Mode::Paper, 0, 1)
            .unwrap();
        assert_eq!(r.effective, Effective::Paper);
        assert!(
            r.line()
                .contains("source=BSC_RUNTIME_MODE requested=PAPER effective=PAPER generation=1")
        );
        let r = s
            .request("rt-2", Entity::Runtime, Mode::Live, 1, 2)
            .unwrap();
        assert_eq!(r.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
        // Survives a restart (a mode, not a permission).
        assert_eq!(s.restart(3).unwrap().runtime.requested, Mode::Live);
        // Stored decode: a v3 generation (before settings) reads OFF / 0.
        let mut d = Draft::new(&sm::SCHEMA_V3);
        d.set(sm::GENERATION, 3u64)
            .set(sm::PROVENANCE_TOOL, "bsc_runtime")
            .set(sm::PROVENANCE_OBSERVED_UNIX_MS, 4u64);
        for source in SourceId::ALL {
            let mut e = Draft::new(&sm::entry::SCHEMA);
            e.set(sm::entry::SOURCE_ID, source.name())
                .set(sm::entry::REQUESTED, "OFF")
                .set(sm::entry::EFFECTIVE, "OFF");
            d.push(sm::SOURCES, e.into_nested());
        }
        fs::write(
            dir.join(format!("generations/{:020}.dbrec", 3)),
            d.encode().unwrap(),
        )
        .unwrap();
        let g = s.current().unwrap();
        assert_eq!(
            (g.runtime.effective, g.buy_attempts_per_day()),
            (Effective::Off, 0)
        );
        fs::remove_dir_all(dir).unwrap();
    }

    /// The contract's day is the UTC calendar day.
    #[test]
    fn the_buy_day_is_the_utc_calendar_day() {
        for (ms, day) in [
            (0, "1970-01-01"),
            (951_782_400_000, "2000-02-29"),
            (1_709_164_800_000, "2024-02-29"),
            (1_709_251_199_999, "2024-02-29"),
            (1_793_059_200_000, "2026-10-27"),
        ] {
            assert_eq!(BuyDay::at(ms).utc, day, "{ms}");
        }
        assert_eq!(BuyDay::at(DAY_MS - 1).index, 0);
        assert_eq!(BuyDay::at(DAY_MS).index, 1);
    }

    /// Plan 27.10. (8b4721bb): the persisted daily BUY limit, 0 initially,
    /// counts every attempt once per UTC day: refused when used up, the
    /// same attempt again keeps its slot, a new day starts empty, a restart
    /// keeps both the limit and the count, and decimal requests only.
    #[test]
    fn the_daily_buy_limit_counts_every_attempt_once_per_utc_day() {
        let (dir, s) = session("buy-limit", "boot-1");
        let now = 1_793_059_200_000;
        let e = s.consume_buy_attempt("a1", now).unwrap_err();
        assert!(
            e.starts_with("BUY_ATTEMPT_LIMIT_REACHED: 0 of 0 BUY attempts on 2026-10-27 (UTC)"),
            "{e}"
        );
        for bad in ["-1", "01", "x", "4294967296", ""] {
            assert_eq!(Mode::parse(bad), None, "{bad}");
        }
        assert!(
            s.request("bl-bad", Entity::BuyLimit, Mode::Enabled, 0, 1)
                .is_err()
        );
        // The campaign's total leaves room; the daily limit is what binds.
        s.request("bt", Entity::BuyTotal, Mode::PerDay(10), 0, 1)
            .unwrap();
        let r = s
            .request("bl-1", Entity::BuyLimit, Mode::parse("2").unwrap(), 1, 1)
            .unwrap();
        assert!(
            r.line()
                .contains("source=BSC_BUY_ATTEMPTS_PER_DAY requested=2 effective=2")
        );
        assert_eq!(s.consume_buy_attempt("a1", now).unwrap().slot, 0);
        assert_eq!(
            s.consume_buy_attempt("a1", now + 5).unwrap().slot,
            0,
            "retry keeps its slot"
        );
        assert_eq!(s.consume_buy_attempt("a2", now + 6).unwrap().slot, 1);
        let e = s.consume_buy_attempt("a3", now + 7).unwrap_err();
        assert!(e.starts_with("BUY_ATTEMPT_LIMIT_REACHED: 2 of 2"), "{e}");
        let usage = buy_usage(&dir, now).unwrap();
        assert_eq!((usage.used, usage.limit, usage.remaining()), (2, 2, 0));
        // The next UTC day starts empty.
        assert_eq!(s.consume_buy_attempt("a3", now + DAY_MS).unwrap().slot, 0);
        // A restart keeps the limit and the count.
        let again = Store::open_in_session(&dir, 8, Ok("boot-2".into())).unwrap();
        again.restart(now + 8).unwrap();
        assert_eq!(buy_usage(&dir, now).unwrap().used, 2);
        assert_eq!(again.current().unwrap().buy_attempts_per_day(), 2);
        // A lower limit leaves nothing remaining, never a negative count.
        let g = again.current().unwrap().generation;
        again
            .request("bl-2", Entity::BuyLimit, Mode::PerDay(1), g, now + 9)
            .unwrap();
        let usage = buy_usage(&dir, now).unwrap();
        assert_eq!((usage.used, usage.limit, usage.remaining()), (2, 1, 0));
        assert!(again.consume_buy_attempt("a4", now + 10).is_err());
        assert!(s.consume_buy_attempt("../x", now).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    /// Concurrent attempts race for the slots: never more than the limit.
    #[test]
    fn concurrent_buy_attempts_never_exceed_the_limit() {
        let (dir, s) = session("buy-race", "boot-1");
        s.request("bt", Entity::BuyTotal, Mode::PerDay(10), 0, 1)
            .unwrap();
        s.request("bl", Entity::BuyLimit, Mode::PerDay(3), 1, 1)
            .unwrap();
        let ok = std::thread::scope(|scope| {
            let runs: Vec<_> = (0..8)
                .map(|i| {
                    let s = &s;
                    scope.spawn(move || s.consume_buy_attempt(&format!("r{i}"), 1).is_ok())
                })
                .collect();
            runs.into_iter()
                .map(|r| r.join().unwrap())
                .filter(|ok| *ok)
                .count()
        });
        assert_eq!(ok, 3);
        assert_eq!(buy_usage(&dir, 1).unwrap().used, 3);
        fs::remove_dir_all(dir).unwrap();
    }

    /// A LIVE grant of Flap / C3 from PublicNode through 48Club.
    pub(crate) fn grant(expires: u64) -> LiveGrant {
        grant_of(expires, LiveStrategies::C3)
    }

    pub(crate) fn grant_of(expires: u64, strategies: LiveStrategies) -> LiveGrant {
        LiveGrant {
            venue: VenueId::Flap,
            strategies,
            sources: vec![SourceId::PublicNode],
            adapters: vec![LiveAdapter::Club48BuilderV1],
            max_input_wei: 1_000_000_000_000,
            max_spend_wei: 10_000_000_000_000_000,
            max_attempts: 2,
            expires_unix_ms: expires,
        }
    }

    /// Evidence for exactly that scope.
    pub(crate) fn qualified() -> LiveQualification {
        LiveQualification {
            venues: vec![VenueId::Flap],
            sources: vec![SourceId::PublicNode],
            adapters: vec![LiveAdapter::Club48BuilderV1],
            acquisition: true,
            risk_decision: true,
        }
    }

    /// A store of boot session `boot-1` with the whole scope requested
    /// LIVE, every switch ENABLED, both limits set and the grant written.
    pub(crate) fn live_ready(tag: &str) -> (PathBuf, Store, u64) {
        live_ready_with(tag, LiveStrategies::C3)
    }

    /// [`live_ready`] with a grant of `strategies`.
    pub(crate) fn live_ready_with(tag: &str, strategies: LiveStrategies) -> (PathBuf, Store, u64) {
        live_ready_granting(tag, |now| grant_of(now + 3_600_000, strategies))
    }

    /// [`live_ready`] with the grant `grant(now)`, its scope requested.
    pub(crate) fn live_ready_granting(
        tag: &str,
        grant: impl FnOnce(u64) -> LiveGrant,
    ) -> (PathBuf, Store, u64) {
        let (dir, s) = session(tag, "boot-1");
        let now = 1_793_059_200_000;
        let grant = grant(now);
        let strategies = grant.strategies;
        let mut g = 0;
        let mut req = |entity: Entity, mode: Mode| {
            let r = s.request(&format!("r-{g}"), entity, mode, g, now).unwrap();
            assert!(r.applied(), "{}", r.line());
            g += 1;
        };
        for id in SwitchId::ALL {
            req(Entity::Switch(id), Mode::Enabled);
        }
        req(Entity::Runtime, Mode::Live);
        if strategies.c3() {
            req(ComponentId::ExecC3.into(), Mode::Live);
        }
        if strategies.strategy_c() {
            req(ComponentId::StrategyC.into(), Mode::Live);
        }
        req(ComponentId::StateC2.into(), Mode::Live);
        req(VenueId::Flap.into(), Mode::Live);
        req(SourceId::PublicNode.into(), Mode::Live);
        req(Entity::BuyLimit, Mode::PerDay(2));
        req(Entity::BuyTotal, Mode::PerDay(3));
        let r = s
            .grant_live("grant-1", grant.clone(), grant.scope_sha256(), g, now)
            .unwrap();
        assert!(r.applied(), "{}", r.line());
        (dir, s, now)
    }

    /// Plan 28.10.: a requested LIVE is effective LIVE only with every
    /// condition of the gate met and the grant's scope naming it; each
    /// condition alone blocks with its own reason. LIVE is never stored.
    #[test]
    fn live_is_effective_only_when_every_condition_holds() {
        let (dir, _s, now) = live_ready("live-gate");
        let (g, daily, total) = live_inputs(&dir, now).unwrap();
        let q = qualified();
        let ctx = LiveContext {
            boot: Some("boot-1"),
            now_unix_ms: now,
            daily: Some(&daily),
            total: Some(&total),
            spent_wei: Some(0),
            qualification: &q,
        };
        assert_eq!(g.live_blockers(&ctx), Vec::<&str>::new());
        let scope: [Entity; 5] = [
            Entity::Runtime,
            ComponentId::ExecC3.into(),
            ComponentId::StateC2.into(),
            VenueId::Flap.into(),
            SourceId::PublicNode.into(),
        ];
        for e in scope {
            assert_eq!(
                g.effective_live_now(e, &ctx),
                Effective::Live,
                "{}",
                e.name()
            );
            assert!(
                matches!(g.effective_of(e), Effective::Blocked(_)),
                "stored never LIVE"
            );
        }
        assert_eq!(
            g.effective_live_now(SwitchId::Sign.into(), &ctx),
            Effective::Enabled
        );
        // Outside the scope: Strategy C (a C3 grant), dRPC, Four.
        let mut wider = g.clone();
        let by = SetBy {
            request_id: "w".into(),
            generation: g.generation,
        };
        for e in [
            Entity::from(ComponentId::StrategyC),
            SourceId::Drpc.into(),
            VenueId::FourClassic.into(),
        ] {
            wider.set(e, Mode::Live, by.clone());
        }
        assert_eq!(
            wider.effective_live_now(ComponentId::StrategyC.into(), &ctx),
            Effective::Blocked(LIVE_NOT_IN_SCOPE)
        );
        assert_eq!(
            wider.effective_live_now(SourceId::Drpc.into(), &ctx),
            Effective::Blocked(LIVE_NOT_IN_SCOPE)
        );
        assert_eq!(
            wider.effective_live_now(VenueId::FourClassic.into(), &ctx),
            Effective::Blocked(FOUR_EXECUTION_PATH_MISSING)
        );
        // Each switch alone.
        for (id, reason) in [
            (SwitchId::Live, ECONOMIC_LIVE_DISABLED),
            (SwitchId::WalletLauncher, WALLET_LAUNCHER_DISABLED),
            (SwitchId::WalletBuyer, WALLET_BUYER_DISABLED),
            (SwitchId::Sign, ECON_SIGN_DISABLED),
            (SwitchId::Arm, ECON_ARM_DISABLED),
            (SwitchId::Send, ECON_SEND_DISABLED),
            (SwitchId::Attempt, ECON_ATTEMPT_DISABLED),
        ] {
            let mut off = g.clone();
            off.set(id.into(), Mode::Disabled, by.clone());
            assert_eq!(off.live_blockers(&ctx), vec![reason], "{}", id.name());
            assert_eq!(
                off.effective_live_now(ComponentId::ExecC3.into(), &ctx),
                Effective::Blocked(reason)
            );
        }
        // The runtime not asked for LIVE, no grant.
        let mut paper = g.clone();
        paper.set(Entity::Runtime, Mode::Paper, by.clone());
        assert_eq!(paper.live_blockers(&ctx), vec![LIVE_RUNTIME_NOT_REQUESTED]);
        assert_eq!(
            paper.effective_live_now(Entity::Runtime, &ctx),
            Effective::Paper
        );
        let mut revoked = g.clone();
        revoked.set(Entity::LiveGrant, Mode::Disabled, by.clone());
        assert_eq!(revoked.live_blockers(&ctx), vec![LIVE_GRANT_MISSING]);
        // The context: expiry, limits, spend, evidence.
        let grant = g.live.clone().unwrap();
        let used_up = |u: &BuyUsage| BuyUsage {
            used: u64::from(u.limit),
            ..u.clone()
        };
        let daily_out = used_up(&daily);
        let total_out = TotalUsage {
            used: u64::from(total.limit),
            ..total.clone()
        };
        let mut no_venue = qualified();
        no_venue.venues.clear();
        let mut no_source = qualified();
        no_source.sources = vec![SourceId::Drpc];
        let mut no_adapter = qualified();
        no_adapter.adapters = vec![LiveAdapter::BlockRazorBuilderV1];
        let mut no_acquisition = qualified();
        no_acquisition.acquisition = false;
        let mut no_risk = qualified();
        no_risk.risk_decision = false;
        let cases: Vec<(LiveContext<'_>, &str)> = vec![
            (
                LiveContext {
                    now_unix_ms: grant.expires_unix_ms,
                    ..ctx
                },
                LIVE_GRANT_EXPIRED,
            ),
            (LiveContext { daily: None, ..ctx }, BUY_LIMIT_UNREADABLE),
            (
                LiveContext {
                    daily: Some(&daily_out),
                    ..ctx
                },
                BUY_LIMIT_EXHAUSTED,
            ),
            (LiveContext { total: None, ..ctx }, ATTEMPT_TOTAL_UNREADABLE),
            (
                LiveContext {
                    total: Some(&total_out),
                    ..ctx
                },
                ATTEMPT_TOTAL_EXHAUSTED,
            ),
            (
                LiveContext {
                    spent_wei: None,
                    ..ctx
                },
                LIVE_SPEND_UNREADABLE,
            ),
            (
                LiveContext {
                    spent_wei: Some(grant.max_spend_wei - grant.max_input_wei + 1),
                    ..ctx
                },
                LIVE_GRANT_SPEND_EXHAUSTED,
            ),
            (
                LiveContext {
                    qualification: &no_venue,
                    ..ctx
                },
                LIVE_VENUE_UNQUALIFIED,
            ),
            (
                LiveContext {
                    qualification: &no_source,
                    ..ctx
                },
                LIVE_SOURCE_UNQUALIFIED,
            ),
            (
                LiveContext {
                    qualification: &no_adapter,
                    ..ctx
                },
                LIVE_ADAPTER_UNQUALIFIED,
            ),
            (
                LiveContext {
                    qualification: &no_acquisition,
                    ..ctx
                },
                LIVE_ACQUISITION_UNPROVEN,
            ),
            (
                LiveContext {
                    qualification: &no_risk,
                    ..ctx
                },
                LIVE_RISK_DECISION_MISSING,
            ),
        ];
        for (c, reason) in cases {
            assert_eq!(g.live_blockers(&c), vec![reason]);
            assert_eq!(
                g.effective_live_now(VenueId::Flap.into(), &c),
                Effective::Blocked(reason)
            );
        }
        // Another boot session: the switches and the grant are reset.
        let reboot = LiveContext {
            boot: Some("boot-2"),
            ..ctx
        };
        let blockers = g.live_blockers(&reboot);
        assert_eq!(blockers.first(), Some(&ECONOMIC_LIVE_DISABLED));
        assert!(
            blockers.contains(&LIVE_GRANT_RESET_BY_RESTART),
            "{blockers:?}"
        );
        assert!(blockers.contains(&WALLET_BUYER_DISABLED), "{blockers:?}");
        let blind = LiveContext { boot: None, ..ctx };
        assert!(
            g.live_blockers(&blind)
                .contains(&LIVE_GRANT_RESET_BY_RESTART)
        );
        fs::remove_dir_all(dir).unwrap();
    }

    /// The grant's own attempt limit counts only its attempts.
    #[test]
    fn a_grant_takes_no_more_attempts_than_it_allows() {
        let (dir, s, now) = live_ready("live-attempts");
        let grant_generation = s.current().unwrap().live_set_by.unwrap().generation;
        s.consume_attempt("l1", grant_generation, now).unwrap();
        s.consume_attempt("l2", grant_generation, now).unwrap();
        let (g, daily, total) = live_inputs(&dir, now).unwrap();
        assert_eq!(total.used_by_grant(grant_generation), 2);
        let q = qualified();
        let ctx = LiveContext {
            boot: Some("boot-1"),
            now_unix_ms: now,
            daily: Some(&daily),
            total: Some(&total),
            spent_wei: Some(0),
            qualification: &q,
        };
        // Two of three campaign attempts are left... none of the grant's.
        let blockers = g.live_blockers(&ctx);
        assert!(
            blockers.contains(&LIVE_GRANT_ATTEMPTS_EXHAUSTED),
            "{blockers:?}"
        );
        assert!(!blockers.contains(&ATTEMPT_TOTAL_EXHAUSTED), "{blockers:?}");
        fs::remove_dir_all(dir).unwrap();
    }

    /// The operator's confirmation is the scope: what was not confirmed is
    /// never stored; the same request is its receipt; DISABLED, a restart
    /// and a write in another boot session revoke it.
    #[test]
    fn a_live_grant_needs_its_confirmed_scope_and_ends_with_revoke_or_restart() {
        let (dir, s) = session("live-grant", "boot-1");
        let now = 1_793_059_200_000;
        let grant = grant(now + 1_000);
        let e = s
            .grant_live("g1", grant.clone(), [0; 32], 0, now)
            .unwrap_err();
        assert!(e.contains("is not this grant's scope"), "{e}");
        let mut four = grant.clone();
        four.venue = VenueId::FourClassic;
        let e = s
            .grant_live("g1", four.clone(), four.scope_sha256(), 0, now)
            .unwrap_err();
        assert!(e.contains(FOUR_EXECUTION_PATH_MISSING), "{e}");
        let mut twice = grant.clone();
        twice.sources.push(SourceId::PublicNode);
        let mut many = grant.clone();
        many.max_attempts = MAX_BUY_ATTEMPTS_TOTAL + 1;
        let mut nothing = grant.clone();
        nothing.adapters.clear();
        let mut over = grant.clone();
        over.max_input_wei = over.max_spend_wei + 1;
        for bad in [twice, many, nothing, over] {
            assert!(
                s.grant_live("g1", bad.clone(), bad.scope_sha256(), 0, now)
                    .is_err()
            );
        }
        assert!(
            s.request(
                "g1",
                Entity::LiveGrant,
                Mode::Granted(grant.scope_sha256()),
                0,
                now
            )
            .unwrap_err()
            .contains("live grant")
        );
        let r = s
            .grant_live("g1", grant.clone(), grant.scope_sha256(), 0, now)
            .unwrap();
        assert_eq!((r.outcome, r.generation), ("APPLIED", 1));
        assert!(
            r.line().contains(&format!(
                "source=BSC_LIVE_GRANT requested=GRANTED:{} effective=ENABLED",
                hex32(&grant.scope_sha256())
            )),
            "{}",
            r.line()
        );
        assert_eq!(s.current().unwrap().live.as_ref(), Some(&grant));
        assert_eq!(
            s.grant_live("g1", grant.clone(), grant.scope_sha256(), 0, now)
                .unwrap(),
            r
        );
        let mut other = grant.clone();
        other.max_input_wei += 1;
        assert!(
            s.grant_live("g1", other.clone(), other.scope_sha256(), 0, now)
                .unwrap_err()
                .contains("another request")
        );
        s.request("g2", Entity::LiveGrant, Mode::Disabled, 1, now)
            .unwrap();
        let g = s.current().unwrap();
        assert_eq!(g.live, None);
        assert_eq!(g.live_set_by.map(|b| b.request_id), Some("g2".into()));
        s.grant_live("g3", grant.clone(), grant.scope_sha256(), 2, now)
            .unwrap();
        let after = s.restart(now).unwrap();
        assert_eq!(after.live, None);
        assert_eq!(
            after.live_set_by.as_ref().unwrap().request_id,
            RESTART_RESET
        );
        s.grant_live(
            "g4",
            grant.clone(),
            grant.scope_sha256(),
            after.generation,
            now,
        )
        .unwrap();
        let other_boot = Store::open_in_session(&dir, 8, Ok("boot-2".into())).unwrap();
        let g = other_boot.current().unwrap();
        assert!(g.live.is_some());
        other_boot
            .request("g5", SourceId::Drpc, Mode::Shadow, g.generation, now)
            .unwrap();
        assert_eq!(other_boot.current().unwrap().live, None);
        fs::remove_dir_all(dir).unwrap();
    }

    /// A stored grant that is not exactly one granted scope or one
    /// revocation never decodes (no more permissive state is accepted).
    #[test]
    fn a_tampered_or_partial_live_grant_never_decodes() {
        use darkbot_bsc_record::{Presence, Value};
        use sm::live as k;
        let (dir, s, now) = live_ready("live-tamper");
        let g = s.current().unwrap();
        let bytes = g.encode(now).unwrap();
        type Edit<'a> = &'a dyn Fn(&mut Vec<(u16, Value)>);
        let tampered = |edit: Edit<'_>| -> Vec<u8> {
            let r = decode_generation(&bytes).unwrap();
            let mut d = Draft::new(r.schema());
            for (tag, v) in r.fields() {
                let mut v = v.clone();
                if *tag == sm::LIVE
                    && let Value::Nested(inner) = &mut v
                {
                    edit(inner);
                }
                if r.schema()
                    .field(*tag)
                    .is_some_and(|f| f.presence == Presence::Repeated)
                {
                    d.push(*tag, v);
                } else {
                    d.set(*tag, v);
                }
            }
            d.encode().unwrap()
        };
        assert!(
            Generation::decode(&tampered(&|_| {})).is_ok(),
            "the copy itself decodes"
        );
        let digest = |inner: &mut Vec<(u16, Value)>| {
            for (t, v) in inner.iter_mut() {
                if *t == k::SCOPE_SHA256 {
                    *v = Value::B256([7; 32]);
                }
            }
        };
        let partial = |inner: &mut Vec<(u16, Value)>| inner.retain(|(t, _)| *t != k::VENUE);
        let revoked_with_scope = |inner: &mut Vec<(u16, Value)>| {
            for (t, v) in inner.iter_mut() {
                if *t == k::STATE {
                    *v = Value::Text("REVOKED".into());
                }
            }
        };
        for (edit, why) in [
            (&digest as Edit<'_>, "not the one it was granted for"),
            (&partial, "either the whole scope or a revocation"),
            (
                &revoked_with_scope,
                "either the whole scope or a revocation",
            ),
        ] {
            let e = Generation::decode(&tampered(edit)).unwrap_err();
            assert!(e.contains(why), "{e}");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    /// Plan 28.10.: the campaign's total BUY limit, 0 initially and at most
    /// ten, bounds every attempt over every day and restart; the same
    /// attempt keeps its slot; concurrent attempts never exceed it.
    #[test]
    fn the_campaign_total_bounds_every_attempt_across_days_and_restarts() {
        let (dir, s) = session("buy-total", "boot-1");
        let now = 1_793_059_200_000;
        s.request("bl", Entity::BuyLimit, Mode::PerDay(10), 0, now)
            .unwrap();
        let e = s.consume_buy_attempt("a1", now).unwrap_err();
        assert!(e.starts_with("BUY_ATTEMPT_TOTAL_REACHED: 0 of 0"), "{e}");
        assert!(
            s.request("bt-bad", Entity::BuyTotal, Mode::PerDay(11), 1, now)
                .is_err(),
            "at most ten"
        );
        s.request("bt", Entity::BuyTotal, Mode::PerDay(2), 1, now)
            .unwrap();
        // The refused attempt kept its daily slot (over-, never under-counted).
        assert_eq!(s.consume_buy_attempt("a1", now).unwrap().slot, 0);
        let (day, total) = s.consume_attempt("a1", 0, now + 1).unwrap();
        assert_eq!(
            (day.slot, total.slot),
            (0, 0),
            "the same attempt, the same slots"
        );
        s.consume_attempt("a2", 7, now + 2).unwrap();
        let e = s.consume_buy_attempt("a3", now + DAY_MS).unwrap_err();
        assert!(e.starts_with("BUY_ATTEMPT_TOTAL_REACHED: 2 of 2"), "{e}");
        let usage = total_usage(&dir).unwrap();
        assert_eq!(
            (
                usage.used,
                usage.limit,
                usage.remaining(),
                usage.used_by_grant(7)
            ),
            (2, 2, 0, 1)
        );
        let again = Store::open_in_session(&dir, 8, Ok("boot-2".into())).unwrap();
        again.restart(now).unwrap();
        assert_eq!(total_usage(&dir).unwrap().used, 2);
        assert_eq!(again.current().unwrap().buy_attempts_total(), 2);
        fs::remove_dir_all(dir).unwrap();
        let (dir, s) = session("buy-total-race", "boot-1");
        s.request("bl", Entity::BuyLimit, Mode::PerDay(10), 0, 1)
            .unwrap();
        s.request("bt", Entity::BuyTotal, Mode::PerDay(3), 1, 1)
            .unwrap();
        let ok = std::thread::scope(|scope| {
            let runs: Vec<_> = (0..8)
                .map(|i| {
                    let s = &s;
                    scope.spawn(move || s.consume_attempt(&format!("t{i}"), 0, 1).is_ok())
                })
                .collect();
            runs.into_iter()
                .map(|r| r.join().unwrap())
                .filter(|ok| *ok)
                .count()
        });
        assert_eq!(ok, 3);
        assert_eq!(total_usage(&dir).unwrap().used, 3);
        fs::remove_dir_all(dir).unwrap();
    }

    /// Review 27.10. L4: a lost receipt is recovered only for the same
    /// payload; an identical twin request that loses the generation race
    /// still reports APPLIED.
    #[test]
    fn receipts_are_recovered_only_for_the_same_request() {
        let (dir, s) = session("receipt-recovery", "boot-1");
        s.request("x", SwitchId::Send, Mode::Enabled, 0, 1).unwrap();
        fs::remove_file(dir.join("receipts/x.dbrec")).unwrap();
        assert!(
            s.request("x", SwitchId::Send, Mode::Disabled, 0, 2)
                .unwrap_err()
                .contains("another request")
        );
        // Another entity that happens to hold the mode is not this request.
        assert!(
            s.request("x", SwitchId::Arm, Mode::Disabled, 0, 2)
                .unwrap_err()
                .contains("another request")
        );
        let r = s.request("x", SwitchId::Send, Mode::Enabled, 0, 3).unwrap();
        assert_eq!((r.outcome, r.generation), ("APPLIED", 1));
        let outcomes: Vec<&str> = std::thread::scope(|scope| {
            let runs: Vec<_> = (0..6)
                .map(|_| {
                    let s = &s;
                    scope.spawn(move || {
                        s.request("twin", SwitchId::Arm, Mode::Enabled, 1, 4)
                            .unwrap()
                            .outcome
                    })
                })
                .collect();
            runs.into_iter().map(|r| r.join().unwrap()).collect()
        });
        assert!(outcomes.iter().all(|o| *o == "APPLIED"), "{outcomes:?}");
        assert_eq!(s.current().unwrap().generation, 2);
        fs::remove_dir_all(dir).unwrap();
    }

    /// Review 27.10. L6: the daily limit takes the G0.7 runner's attempt IDs.
    #[test]
    fn the_daily_limit_takes_the_runners_attempt_ids() {
        let (dir, s) = session("attempt-ids", "boot-1");
        s.request("bt", Entity::BuyTotal, Mode::PerDay(10), 0, 1)
            .unwrap();
        s.request("bl", Entity::BuyLimit, Mode::PerDay(3), 1, 1)
            .unwrap();
        assert!(s.consume_buy_attempt("g07.attempt_1-a", 1).is_ok());
        assert!(s.consume_buy_attempt("-x", 1).is_ok(), "G0.7 takes it");
        for bad in ["", ".x", "a/b", &"a".repeat(129)] {
            assert!(s.consume_buy_attempt(bad, 1).is_err(), "{bad}");
        }
        assert!(s.consume_buy_attempt(&"a".repeat(128), 1).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }

    /// Review 27.10. pass 2: the legitimate path. The account home from
    /// the user database is this test process's HOME (when set), and the
    /// canonical owner state lies under it.
    #[test]
    fn the_canonical_state_is_under_the_account_home() {
        let home = account_home().unwrap();
        assert!(home.is_absolute() && home.is_dir(), "{}", home.display());
        if let Some(h) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
            assert_eq!(PathBuf::from(h), home);
        }
        assert_eq!(
            canonical_state_dir().unwrap(),
            home.join(".darkbot/run/bsc-sources")
        );
    }

    /// A generation whose stored effective mode claims more than the rule
    /// (e.g. a hand-edited SHADOW for 48Club) is refused, never trusted.
    #[test]
    fn a_tampered_generation_is_refused() {
        let (dir, s) = store("tamper");
        s.request("t", SourceId::Club48, Mode::Shadow, 0, 1)
            .unwrap();
        let path = dir.join(format!("generations/{:020}.dbrec", 1));
        let bytes = fs::read(&path).unwrap();
        let g = Generation::decode(&bytes).unwrap();
        let mut d = Draft::new(&sm::SCHEMA);
        d.set(sm::GENERATION, g.generation)
            .set(sm::PROVENANCE_TOOL, "bsc_runtime")
            .set(sm::PROVENANCE_OBSERVED_UNIX_MS, 1u64);
        for m in g.sources {
            let mut e = Draft::new(&sm::entry::SCHEMA);
            e.set(sm::entry::SOURCE_ID, m.source.name())
                .set(sm::entry::REQUESTED, m.requested.name())
                .set(sm::entry::EFFECTIVE, "SHADOW");
            d.push(sm::SOURCES, e.into_nested());
        }
        fs::write(&path, d.encode().unwrap()).unwrap();
        assert!(s.current().unwrap_err().contains("not the owner's"));
        fs::remove_dir_all(dir).unwrap();
    }

    /// Plan 28.10.: the owner admits each LIVE intent as its state holds
    /// at that moment: a BUY takes its daily and campaign slot and its spend
    /// entry before any byte can leave; an exit needs no BUY attempt; the
    /// same transaction never twice; another grant, a restart or a scope
    /// that does not name the component admits nothing.
    #[test]
    fn each_live_intent_is_admitted_by_the_owner_as_it_holds_now() {
        let (dir, s, now) = live_ready("live-admit");
        let q = qualified();
        assert_eq!(
            s.live_now(&[ComponentId::StrategyC], &q, now).unwrap_err(),
            "BSC_STRATEGY_C_OFF"
        );
        assert_eq!(
            s.live_now(&[ComponentId::ExecC3], &LiveQualification::default(), now)
                .unwrap_err(),
            LIVE_VENUE_UNQUALIFIED
        );
        let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
        let grant = armed.grant().clone();
        let generation = armed.grant_generation();
        let id = |n: u8| hex32(&[n; 32]);
        let ids: Vec<String> = (0..8).map(id).collect();
        let intent = |n: usize, kind: LiveIntentKind, input: u128, most: u128| LiveIntent {
            id: &ids[n],
            component: ComponentId::ExecC3,
            kind,
            input_wei: input,
            max_wei: most,
            token: [0x10 + n as u8; 20],
            nonce: 40 + n as u64,
            max_block: 900 + n as u64,
        };
        let buy = |n| intent(n, LiveIntentKind::Buy, 1_000, 2_000);
        let first = s.admit_live(&armed, &buy(0), &q, now).unwrap();
        assert_eq!(first.spent_wei, 2_000);
        let (day, total) = first.slots.unwrap();
        assert_eq!((day.slot, total.slot), (0, 0));
        assert_eq!(spent_by_grant(&dir, generation).unwrap(), 2_000);
        // BSC3-024: the admission is the transaction's write-ahead record,
        // with its token and nonce.
        assert_eq!(
            admitted_intents(&dir).unwrap(),
            vec![Admitted {
                tx_hash: [0; 32],
                kind: LiveIntentKind::Buy,
                component: ComponentId::ExecC3,
                grant_generation: generation,
                input_wei: 1_000,
                max_wei: 2_000,
                token: Some([0x10; 20]),
                nonce: Some(40),
                max_block: Some(900),
            }]
        );
        assert_eq!(
            s.admit_live(&armed, &buy(0), &q, now).unwrap_err(),
            LIVE_INTENT_SEEN
        );
        assert_eq!(
            s.admit_live(
                &armed,
                &intent(1, LiveIntentKind::Buy, grant.max_input_wei + 1, 1),
                &q,
                now
            )
            .unwrap_err(),
            LIVE_INPUT_ABOVE_GRANT
        );
        assert_eq!(
            s.admit_live(
                &armed,
                &intent(1, LiveIntentKind::Buy, 1, grant.max_spend_wei),
                &q,
                now
            )
            .unwrap_err(),
            LIVE_GRANT_SPEND_EXHAUSTED
        );
        assert!(s.admit_live(&armed, &buy(1), &q, now).is_ok());
        // The day's two BUYs are taken: no third BUY, the exit still.
        assert_eq!(
            s.admit_live(&armed, &buy(2), &q, now).unwrap_err(),
            BUY_LIMIT_EXHAUSTED
        );
        let sell = s
            .admit_live(&armed, &intent(3, LiveIntentKind::Sell, 0, 500), &q, now)
            .unwrap();
        assert_eq!((sell.slots, sell.spent_wei), (None, 4_500));
        assert_eq!(buy_usage(&dir, now).unwrap().used, 2);
        // Not the grant armed under; not in the scope; evidence missing.
        let other = LiveNow {
            grant_generation: generation + 1,
            ..armed.clone()
        };
        let exit = intent(4, LiveIntentKind::Sell, 0, 1);
        assert_eq!(
            s.admit_live(&other, &exit, &q, now).unwrap_err(),
            LIVE_GRANT_CHANGED
        );
        let strategy = LiveIntent {
            component: ComponentId::StrategyC,
            ..exit
        };
        assert!(
            s.admit_live(&armed, &strategy, &q, now)
                .unwrap_err()
                .starts_with("BSC_STRATEGY_C_")
        );
        let none = LiveQualification::default();
        assert_eq!(
            s.admit_live(&armed, &exit, &none, now).unwrap_err(),
            LIVE_VENUE_UNQUALIFIED
        );
        // After a restart nothing is LIVE, an exit neither.
        let restarted = Store::open_in_session(&dir, 8, Ok("boot-2".into())).unwrap();
        assert_eq!(
            restarted.admit_live(&armed, &exit, &q, now).unwrap_err(),
            ECONOMIC_LIVE_DISABLED
        );
        assert_eq!(
            restarted
                .live_now(&[ComponentId::ExecC3], &q, now)
                .unwrap_err(),
            ECONOMIC_LIVE_DISABLED
        );
        assert_eq!(spent_by_grant(&dir, generation).unwrap(), 4_500);
        fs::remove_dir_all(dir).unwrap();
    }

    /// Plan 28.10.: the LIVE tab's checklist names every condition once,
    /// in order, with the gate's own reason; without a grant the scope
    /// conditions cannot hold.
    #[test]
    fn the_live_checklist_names_every_condition_with_its_reason() {
        let (dir, _s, now) = live_ready("live-checklist");
        let (g, daily, total) = live_inputs(&dir, now).unwrap();
        let q = qualified();
        let ctx = LiveContext {
            boot: Some("boot-1"),
            now_unix_ms: now,
            daily: Some(&daily),
            total: Some(&total),
            spent_wei: Some(0),
            qualification: &q,
        };
        let all = g.live_checklist(&ctx);
        assert_eq!(all.len(), LIVE_CONDITIONS.len());
        assert!(all.iter().all(|(_, r)| r.is_none()));
        let by = SetBy {
            request_id: "x".into(),
            generation: g.generation,
        };
        let mut off = g.clone();
        off.set(SwitchId::Send.into(), Mode::Disabled, by.clone());
        off.set(Entity::LiveGrant, Mode::Disabled, by);
        let list = off.live_checklist(&LiveContext {
            qualification: &LiveQualification::default(),
            ..ctx
        });
        let reason = |id: &str| list.iter().find(|(c, _)| *c == id).unwrap().1;
        assert_eq!(reason("ECON_SEND"), Some(ECON_SEND_DISABLED));
        assert_eq!(reason("LIVE_GRANT"), Some(LIVE_GRANT_MISSING));
        assert_eq!(reason("VENUE_QUALIFIED"), Some(LIVE_GRANT_MISSING));
        assert_eq!(reason("WALLET_BUYER"), None);
        fs::remove_dir_all(dir).unwrap();
    }
}

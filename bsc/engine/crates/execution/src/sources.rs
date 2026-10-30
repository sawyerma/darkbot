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

use alloy_primitives::U256;
use darkbot_bsc_record::schema::bsc_runtime::{source_mode_receipt as rc, source_modes as sm};
use darkbot_bsc_record::{Draft, Record, decode};
use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
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
/// BSC3-049: a granted builder did not accept the proven acquisition's
/// attempt (exits are exempt: they need no acquisition through it).
pub const LIVE_ACQUISITION_BUILDER_UNPROVEN: &str = "LIVE_ACQUISITION_BUILDER_UNPROVEN";
pub const LIVE_RISK_DECISION_MISSING: &str = "LIVE_RISK_DECISION_MISSING";
/// The grant on file is not the one the executor was armed under.
pub const LIVE_GRANT_CHANGED: &str = "LIVE_GRANT_CHANGED";
/// A BUY's input exceeds the grant's largest input.
pub const LIVE_INPUT_ABOVE_GRANT: &str = "LIVE_INPUT_ABOVE_GRANT";
/// The same signed transaction again: never a second time (no retry).
pub const LIVE_INTENT_SEEN: &str = "LIVE_INTENT_SEEN";
/// The grant admits the exits of open positions only (BSC3-058): no BUY.
pub const LIVE_GRANT_EXIT_ONLY: &str = "LIVE_GRANT_EXIT_ONLY";
/// BSC3-069: a grant without the execution risk the operator confirmed
/// (written before it): it admits nothing.
pub const LIVE_GRANT_RISK_MISSING: &str = "LIVE_GRANT_RISK_MISSING";
/// BSC3-069: the executor's configuration signs beyond the granted risk.
pub const LIVE_RISK_OUTSIDE_GRANT: &str = "LIVE_RISK_OUTSIDE_GRANT";
/// BSC3-012: the verified LIVE qualification on file is another record than
/// the one the armed path rests on.
pub const LIVE_QUALIFICATION_CHANGED: &str = "LIVE_QUALIFICATION_CHANGED";
/// A BUY would leave the grant less than the exits of the open positions
/// and of its own may still cost (BSC3-057): it is refused, the exits are
/// not.
pub const LIVE_EXIT_RESERVE_MISSING: &str = "LIVE_EXIT_RESERVE_MISSING";
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
pub const LIVE_CONDITIONS: [(&str, &[&str]); 20] = [
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
    ("GRANT_BUYS", &[LIVE_GRANT_EXIT_ONLY]),
    ("GRANT_RISK", &[LIVE_GRANT_RISK_MISSING]),
    ("VENUE_QUALIFIED", &[LIVE_VENUE_UNQUALIFIED]),
    ("SOURCES_QUALIFIED", &[LIVE_SOURCE_UNQUALIFIED]),
    ("ADAPTERS_QUALIFIED", &[LIVE_ADAPTER_UNQUALIFIED]),
    (
        "ACQUISITION_PROVEN",
        &[LIVE_ACQUISITION_UNPROVEN, LIVE_ACQUISITION_BUILDER_UNPROVEN],
    ),
    ("RISK_DECISION", &[LIVE_RISK_DECISION_MISSING]),
];

/// The first of [`LIVE_CONDITIONS`] judged only against a grant.
const SCOPE_CONDITIONS_FROM: usize = 11;

/// What an exit of a position the grant opened does not need of the LIVE
/// gate: BUY attempts (the attempt switch, the daily and campaign limits,
/// the grant's attempts), room for another BUY input, and a grant that
/// admits exits only (BSC3-058). Its own gas must still fit the grant's
/// spend ([`Store::admit_live`]).
const EXIT_EXEMPT: [&str; 10] = [
    LIVE_ACQUISITION_BUILDER_UNPROVEN,
    ECON_ATTEMPT_DISABLED,
    BUY_LIMIT_UNREADABLE,
    BUY_LIMIT_EXHAUSTED,
    ATTEMPT_TOTAL_UNREADABLE,
    ATTEMPT_TOTAL_EXHAUSTED,
    LIVE_GRANT_ATTEMPTS_EXHAUSTED,
    LIVE_GRANT_SPEND_EXHAUSTED,
    LIVE_SPEND_UNREADABLE,
    LIVE_GRANT_EXIT_ONLY,
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
            // a request above the campaign's is refused in `apply`, the
            // limit in force is capped.
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
            Entity::BuyLimit => "a whole number of BUY attempts per UTC day, at most 10 (0 = none)",
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
    /// The operator's separate authorization of the exits of open
    /// positions alone (BSC3-058): no BUY under it. A full stop (a switch
    /// off), its expiry and a restart still end it.
    pub exit_only: bool,
    /// The execution risk the operator confirmed (BSC3-069); none in a
    /// grant written before it: such a grant admits nothing.
    pub risk: Option<GrantRisk>,
}

/// The execution risk of a LIVE grant (BSC3-069): the most slippage a BUY
/// and an exit may accept, and the most gas price and gas limits a BUY, an
/// approval and a SELL may be signed with. The executor's configuration
/// arms only within it; the grant's spend still bounds what all of it may
/// cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantRisk {
    pub max_buy_slippage_bps: u16,
    pub max_exit_slippage_bps: u16,
    pub max_gas_price_wei: u64,
    pub max_buy_gas_limit: u64,
    pub max_approve_gas_limit: u64,
    pub max_sell_gas_limit: u64,
}

/// The risk of a stored grant: all six bounds, or none (a grant written
/// before them); a part of them is refused.
fn grant_risk_of(row: &Record) -> Result<Option<GrantRisk>, String> {
    use darkbot_bsc_record::schema::bsc_runtime::source_modes::live as k;
    let e = |x: darkbot_bsc_record::RecordError| x.0;
    let values = [
        row.opt_u64(k::MAX_BUY_SLIPPAGE_BPS).map_err(e)?,
        row.opt_u64(k::MAX_EXIT_SLIPPAGE_BPS).map_err(e)?,
        row.opt_u64(k::MAX_GAS_PRICE_WEI).map_err(e)?,
        row.opt_u64(k::MAX_BUY_GAS_LIMIT).map_err(e)?,
        row.opt_u64(k::MAX_APPROVE_GAS_LIMIT).map_err(e)?,
        row.opt_u64(k::MAX_SELL_GAS_LIMIT).map_err(e)?,
    ];
    let bps = |v: u64| u16::try_from(v).map_err(|_| "a granted slippage out of range".to_string());
    match values {
        [None, None, None, None, None, None] => Ok(None),
        [
            Some(buy),
            Some(exit),
            Some(price),
            Some(gas),
            Some(approve),
            Some(sell),
        ] => Ok(Some(GrantRisk {
            max_buy_slippage_bps: bps(buy)?,
            max_exit_slippage_bps: bps(exit)?,
            max_gas_price_wei: price,
            max_buy_gas_limit: gas,
            max_approve_gas_limit: approve,
            max_sell_gas_limit: sell,
        })),
        _ => Err("a LIVE grant names its risk whole or not at all".into()),
    }
}

/// What an executor signs with, as [`GrantRisk::admits`] compares it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignedRisk {
    pub buy_slippage_bps: u16,
    pub exit_slippage_bps: u16,
    pub gas_price_wei: u64,
    pub buy_gas_limit: u64,
    pub approve_gas_limit: u64,
    pub sell_gas_limit: u64,
}

impl GrantRisk {
    /// The least gas limit of a transaction (a plain transfer's).
    const MIN_GAS_LIMIT: u64 = 21_000;

    /// Refused unless each bound is one a transaction can be signed within:
    /// slippage below 100 %, a gas price, gas limits of at least a plain
    /// transfer.
    fn validate(&self) -> Result<(), String> {
        if self.max_buy_slippage_bps >= 10_000 || self.max_exit_slippage_bps >= 10_000 {
            return Err("a granted slippage is below 10000 bps".into());
        }
        if self.max_gas_price_wei == 0 {
            return Err("the granted gas price must be positive".into());
        }
        if [
            self.max_buy_gas_limit,
            self.max_approve_gas_limit,
            self.max_sell_gas_limit,
        ]
        .iter()
        .any(|l| *l < Self::MIN_GAS_LIMIT)
        {
            return Err(format!(
                "a granted gas limit is at least {}",
                Self::MIN_GAS_LIMIT
            ));
        }
        Ok(())
    }

    /// Whether an executor signing with `signed` stays within this risk;
    /// otherwise every value beyond it. Under a grant of exits only
    /// (`exits`, BSC3-058) the BUY's bounds do not apply: no BUY is
    /// signed, and they never keep the exits of open positions from going.
    pub fn admits(&self, signed: &SignedRisk, exits: bool) -> Result<(), String> {
        let bounds = [
            (
                "buy_slippage_bps",
                u64::from(signed.buy_slippage_bps),
                u64::from(self.max_buy_slippage_bps),
                true,
            ),
            (
                "exit_slippage_bps",
                u64::from(signed.exit_slippage_bps),
                u64::from(self.max_exit_slippage_bps),
                false,
            ),
            (
                "gas_price_wei",
                signed.gas_price_wei,
                self.max_gas_price_wei,
                false,
            ),
            (
                "buy_gas_limit",
                signed.buy_gas_limit,
                self.max_buy_gas_limit,
                true,
            ),
            (
                "approve_gas_limit",
                signed.approve_gas_limit,
                self.max_approve_gas_limit,
                false,
            ),
            (
                "sell_gas_limit",
                signed.sell_gas_limit,
                self.max_sell_gas_limit,
                false,
            ),
        ];
        let over: Vec<String> = bounds
            .iter()
            .filter(|(_, has, most, buy)| !(exits && *buy) && has > most)
            .map(|(name, has, most, _)| format!("{name} {has} above {most}"))
            .collect();
        match over.is_empty() {
            true => Ok(()),
            false => Err(format!("{LIVE_RISK_OUTSIDE_GRANT}: {}", over.join("; "))),
        }
    }
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
        if let Some(r) = &self.risk {
            r.validate()?;
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
        ) + &self.risk.map_or(String::new(), |r| {
            format!(
                "max_buy_slippage_bps={}\nmax_exit_slippage_bps={}\nmax_gas_price_wei={}\nmax_buy_gas_limit={}\nmax_approve_gas_limit={}\nmax_sell_gas_limit={}\n",
                r.max_buy_slippage_bps,
                r.max_exit_slippage_bps,
                r.max_gas_price_wei,
                r.max_buy_gas_limit,
                r.max_approve_gas_limit,
                r.max_sell_gas_limit
            )
        }) + if self.exit_only {
            "exit_only=true\n"
        } else {
            ""
        }
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
    /// BSC3-049: the builders its G0.7 attempt was sent through; the
    /// grant's adapters must be among them.
    pub acquisition_adapters: Vec<LiveAdapter>,
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

    /// The daily BUY attempt limit in force.
    pub fn buy_attempts_per_day(&self) -> u32 {
        match self.buy_limit.effective {
            // BSC3-004: never more a day than the whole campaign may take.
            Effective::PerDay(n) => n.min(MAX_BUY_ATTEMPTS_TOTAL),
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
        if grant.is_some_and(|g| g.exit_only) {
            out.push(LIVE_GRANT_EXIT_ONLY);
        }
        if grant.is_some_and(|g| g.risk.is_none()) {
            out.push(LIVE_GRANT_RISK_MISSING);
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
                        )
                        .set_opt(k::EXIT_ONLY, g.exit_only.then_some(true));
                    if let Some(r) = &g.risk {
                        e.set(k::MAX_BUY_SLIPPAGE_BPS, u64::from(r.max_buy_slippage_bps))
                            .set(k::MAX_EXIT_SLIPPAGE_BPS, u64::from(r.max_exit_slippage_bps))
                            .set(k::MAX_GAS_PRICE_WEI, r.max_gas_price_wei)
                            .set(k::MAX_BUY_GAS_LIMIT, r.max_buy_gas_limit)
                            .set(k::MAX_APPROVE_GAS_LIMIT, r.max_approve_gas_limit)
                            .set(k::MAX_SELL_GAS_LIMIT, r.max_sell_gas_limit);
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

/// The owner's state directory (`generations/`, `receipts/`).
pub struct Store {
    dir: PathBuf,
    /// The owner of its directories (BSC3-003): every file read is theirs.
    owner: u32,
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
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_new_as(path, bytes, Durability::Full)
}

/// How a create-new write reaches the medium (BSC3-023).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Durability {
    /// Durable when the write returns: the file and its directory each
    /// flushed to the medium.
    Full,
    /// Handed to the drive only; durable once the caller has flushed the
    /// device ([`flush_device`]) before anything depends on it.
    Handed,
}

/// Tests: what reached the drive on this thread, in order (BSC3-023).
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
enum Medium {
    /// A file (named by its final path) or a directory flushed on its own.
    Full(PathBuf),
    /// A file's content handed to the drive, before anything after it.
    Ordered(PathBuf),
    /// A new file linked into place.
    Linked(PathBuf),
    /// A directory handed to the drive.
    Handed(PathBuf),
    /// The device flushed through this directory, for these written ones.
    Flush(PathBuf, Vec<PathBuf>),
    /// A file's content synced while its path was already published.
    Published(PathBuf),
}

#[cfg(test)]
thread_local! {
    static MEDIUM: std::cell::RefCell<Vec<Medium>> = const { std::cell::RefCell::new(Vec::new()) };
    /// Tests: the next device flush on this thread fails.
    static FLUSH_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn medium(event: Medium) {
    MEDIUM.with(|m| m.borrow_mut().push(event));
}

/// A new file's content (named by its final `path`) synced: under
/// `Handed` ordered before the link that publishes it.
fn sync_content_as(file: &fs::File, path: &Path, durability: Durability) -> std::io::Result<()> {
    #[cfg(test)]
    if path.exists() {
        medium(Medium::Published(path.to_path_buf()));
    }
    #[cfg(not(test))]
    let _ = path;
    match durability {
        Durability::Full => {
            #[cfg(test)]
            medium(Medium::Full(path.to_path_buf()));
            file.sync_all()
        }
        Durability::Handed => {
            #[cfg(test)]
            medium(Medium::Ordered(path.to_path_buf()));
            darkbot_bsc_journal::durable::ordered(file)
        }
    }
}

fn sync_dir_as(dir: &Path, durability: Durability) -> std::io::Result<()> {
    let file = fs::File::open(dir)?;
    match durability {
        Durability::Full => {
            #[cfg(test)]
            medium(Medium::Full(dir.to_path_buf()));
            file.sync_all()
        }
        Durability::Handed => {
            #[cfg(test)]
            medium(Medium::Handed(dir.to_path_buf()));
            darkbot_bsc_journal::durable::handed(&file)
        }
    }
}

/// Puts every write handed to the drive before it on the medium
/// (BSC3-023), through the directory `dir`. The flush covers `dir`'s
/// device only: every directory in `written` must be on it.
fn flush_device(dir: &Path, written: &[PathBuf]) -> std::io::Result<()> {
    let device = fs::metadata(dir)?.dev();
    for w in written {
        if fs::metadata(w)?.dev() != device {
            return Err(std::io::Error::other(format!(
                "{} is not on the device of {}",
                w.display(),
                dir.display()
            )));
        }
    }
    #[cfg(test)]
    {
        medium(Medium::Flush(dir.to_path_buf(), written.to_vec()));
        if FLUSH_FAILS.with(|f| f.replace(false)) {
            return Err(std::io::Error::other("the device flush failed (test)"));
        }
    }
    darkbot_bsc_journal::durable::flush(&fs::File::open(dir)?)
}

fn write_new_as(path: &Path, bytes: &[u8], durability: Durability) -> std::io::Result<()> {
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
        sync_content_as(&f, path, durability)?;
        fs::hard_link(&tmp, path)?;
        #[cfg(test)]
        medium(Medium::Linked(path.to_path_buf()));
        Ok::<(), std::io::Error>(())
    })();
    let _ = fs::remove_file(&tmp);
    written?;
    sync_dir_as(dir, durability)
}

/// Why a directory or file of a private store is refused.
#[derive(Debug)]
pub(crate) enum DirRefusal {
    Io(String),
    /// Another user can read or change it.
    Exposed(String),
}

impl std::fmt::Display for DirRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DirRefusal::Io(e) | DirRefusal::Exposed(e) => f.write_str(e),
        }
    }
}

/// `dir` is owner-only and no directory on its way (as written and as
/// resolved) can be changed by another user: each belongs to `dir`'s owner
/// or root and is not writable by group or others unless sticky; a link on
/// the written path is the owner's or root's (its target is on the
/// resolved path). Returns that owner: a process that opens a file inside
/// an owner-only directory is its owner (or root). BSC3-003/067.
pub(crate) fn private_directory(dir: &Path) -> Result<u32, DirRefusal> {
    let io = |e: std::io::Error| DirRefusal::Io(format!("{}: {e}", dir.display()));
    let real = fs::canonicalize(dir).map_err(io)?;
    let meta = fs::metadata(&real).map_err(io)?;
    if !meta.is_dir() || meta.mode() & 0o077 != 0 {
        return Err(DirRefusal::Exposed(format!(
            "{} is accessible to others",
            dir.display()
        )));
    }
    let owner = meta.uid();
    for a in dir
        .ancestors()
        .chain(real.ancestors())
        .filter(|a| !a.as_os_str().is_empty())
    {
        let m = fs::symlink_metadata(a).map_err(io)?;
        let others_write =
            !m.file_type().is_symlink() && m.mode() & 0o022 != 0 && m.mode() & 0o1000 == 0;
        if (m.uid() != owner && m.uid() != 0) || others_write {
            return Err(DirRefusal::Exposed(format!(
                "{} can be changed by another user",
                a.display()
            )));
        }
    }
    Ok(owner)
}

/// The bytes of the store file `path` whose directory belongs to `owner`:
/// the name checked (a regular file, no link), the file opened once and
/// checked on its descriptor (the same inode, `owner`'s, writable by no
/// one else). Its link count is not: [`write_new`] publishes by a second
/// name for a moment, and no other user can link into an owner-only
/// directory. BSC3-003.
pub(crate) fn read_private(path: &Path, owner: u32) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let named = fs::symlink_metadata(path)?;
    let mut file = fs::File::open(path)?;
    let opened = file.metadata()?;
    if !named.is_file()
        || (opened.dev(), opened.ino()) != (named.dev(), named.ino())
        || opened.uid() != owner
        || opened.mode() & 0o022 != 0
    {
        return Err(std::io::Error::new(
            ErrorKind::PermissionDenied,
            "not a private file of the store's owner",
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
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

/// The real UID of a `/proc/self/status` text (`Uid:` real, effective,
/// saved, filesystem).
fn real_uid_of_status(status: &str) -> Option<&str> {
    status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|l| l.split_whitespace().next())
}

/// The home of `uid` in a `/etc/passwd` text (seven fields, the shell may
/// be empty).
fn home_of_passwd<'a>(passwd: &'a str, uid: &str) -> Option<&'a str> {
    passwd.lines().find_map(|l| {
        let f: Vec<&str> = l.split(':').collect();
        (f.len() >= 7 && f[2] == uid).then_some(f[5])
    })
}

/// The home of a `dscacheutil -q user` answer.
fn home_of_dscache(entry: &str) -> Option<&str> {
    entry.lines().find_map(|l| l.strip_prefix("dir: "))
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
        // BSC3-007: the real UID on both systems (`id -ru`; the first of
        // the four `Uid:` fields), never the effective one.
        let home = if cfg!(target_os = "macos") {
            let uid = run("/usr/bin/id", &["-ru"])?;
            let entry = run(
                "/usr/bin/dscacheutil",
                &["-q", "user", "-a", "uid", uid.trim()],
            )?;
            home_of_dscache(&entry).map(str::to_string)
        } else {
            let status = fs::read_to_string("/proc/self/status")
                .map_err(|e| format!("account home: {e}"))?;
            let uid = real_uid_of_status(&status).ok_or("account home: no real uid")?;
            let passwd =
                fs::read_to_string("/etc/passwd").map_err(|e| format!("account home: {e}"))?;
            home_of_passwd(&passwd, uid).map(str::to_string)
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
/// The owner of `dir`'s `generations`: a real, owner-only directory
/// (BSC3-003; the way to it is checked when a store opens and before a
/// switch is required).
fn generations_owner(dir: &Path) -> Result<u32, String> {
    let m = fs::symlink_metadata(dir.join("generations")).map_err(|e| e.to_string())?;
    if !m.is_dir() || m.mode() & 0o077 != 0 {
        return Err("generations is not a private real directory".into());
    }
    Ok(m.uid())
}

pub fn current_generation(dir: &Path) -> Result<Generation, String> {
    match fs::symlink_metadata(dir.join("generations")) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Generation::initial()),
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }
    let owner = generations_owner(dir)?;
    match highest_generation(dir)? {
        None => Ok(Generation::initial()),
        Some(n) => {
            let bytes = read_private(
                &dir.join("generations").join(format!("{n:020}.dbrec")),
                owner,
            )
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
    // BSC3-003: the state of a store another user can change counts nothing
    // (a missing one is everything DISABLED below).
    if dir.join("generations").exists() {
        private_directory(&dir.join("generations"))
            .map_err(|e| format!("CONTROL_STATE_UNREADABLE: {e}; every switch DISABLED"))?;
    }
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

#[cfg(test)]
thread_local! {
    /// Runs between a slot's limit read and its write (BSC3-004 test).
    static BEFORE_SLOT_WRITE: std::cell::RefCell<Option<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    /// Runs between the spend's read and its entry's write (budget test).
    static AFTER_SPEND_READ: std::cell::RefCell<Option<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(None) };
}

fn after_spend_read() {
    #[cfg(test)]
    AFTER_SPEND_READ.with(|h| {
        if let Some(h) = h.borrow().as_ref() {
            h();
        }
    });
}

fn before_slot_write() {
    #[cfg(test)]
    BEFORE_SLOT_WRITE.with(|h| {
        if let Some(h) = h.borrow().as_ref() {
            h();
        }
    });
}

/// The daily slot `attempt_id` holds on any UTC day (BSC3-004: an attempt
/// tried again after midnight is the same attempt, never a second daily
/// slot).
fn daily_slot_of(dir: &Path, attempt_id: &str) -> Result<Option<(BuyDay, u64)>, String> {
    use darkbot_bsc_record::schema::bsc_runtime::buy_attempt_slot as bs;
    let days = match fs::read_dir(dir.join("buy-attempts")) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("buy attempts unreadable: {e}")),
    };
    for day in days {
        let day = day.map_err(|e| e.to_string())?.path();
        // Review L4: a stray file (e.g. `.DS_Store`) is no day.
        if !day.is_dir() {
            continue;
        }
        for (slot, attempt) in buy_slots(&day)? {
            if attempt.as_deref() != Some(attempt_id) {
                continue;
            }
            let r = fs::read(day.join(format!("{slot:010}.dbrec")))
                .map_err(|e| e.to_string())
                .and_then(|b| decode(&bs::SCHEMA, &b).map_err(|e| e.to_string()))?;
            let e = |x: darkbot_bsc_record::RecordError| x.0;
            return Ok(Some((
                BuyDay {
                    index: r.u64(bs::DAY_INDEX).map_err(e)?,
                    utc: r.text(bs::DAY_UTC).map_err(e)?.to_string(),
                },
                slot,
            )));
        }
    }
    Ok(None)
}

/// The latest time a campaign slot was taken at (BSC3-004: slots are taken
/// on a forward clock only).
fn latest_slot_ms(dir: &Path) -> Result<Option<u64>, String> {
    use darkbot_bsc_record::schema::bsc_runtime::buy_attempt_total_slot as ts;
    let entries = match fs::read_dir(total_dir(dir)) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("total BUY attempts unreadable: {e}")),
    };
    let mut latest = None;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_name().to_string_lossy().ends_with(".dbrec") {
            continue;
        }
        if let Some(t) = fs::read(entry.path())
            .ok()
            .and_then(|b| decode(&ts::SCHEMA, &b).ok())
            .and_then(|r| r.u64(ts::PROVENANCE_OBSERVED_UNIX_MS).ok())
        {
            latest = latest.max(Some(t));
        }
    }
    Ok(latest)
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

/// What the grant written at `grant_generation` has spent: per (chain,
/// sender, nonce) the most of its admitted intents may lose (one of them
/// lands, whichever), and every intent without its sender and chain (an
/// entry written before them) in full. Every entry counts, whatever came
/// of it; none is ever released. An entry that does not decode makes the
/// spend unreadable (never under-counted).
pub fn spent_by_grant(dir: &Path, grant_generation: u64) -> Result<u128, String> {
    spend_of_grant(&admitted_intents(dir)?, grant_generation)?.wei()
}

/// [`spent_by_grant`] of the admitted intents `admitted`, by nonce.
fn spend_of_grant(admitted: &[Admitted], grant_generation: u64) -> Result<NonceCosts, String> {
    let mut book = NonceCosts::default();
    for a in admitted
        .iter()
        .filter(|a| a.grant_generation == grant_generation)
    {
        book.add(a.key(), U256::from(a.max_wei))
            .ok_or_else(|| format!("{LIVE_SPEND_UNREADABLE}: the spend overflows"))?;
    }
    Ok(book)
}

/// The account and chain a LIVE transaction's nonce is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NonceOrigin {
    pub chain_id: u64,
    pub sender: [u8; 20],
}

/// One (chain, sender, nonce): its transactions are alternatives, at most
/// one of them lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NonceKey {
    pub origin: NonceOrigin,
    pub nonce: u64,
}

/// What transactions may cost when the alternatives of one nonce count
/// once, at the most of them; a cost without its nonce's sender and chain
/// counts alone, in full (never grouped by its nonce alone). Nothing
/// counted is ever taken back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NonceCosts {
    groups: std::collections::BTreeMap<NonceKey, U256>,
    alone: U256,
}

impl NonceCosts {
    /// Counts `wei` under `key` (none: alone). None: the sum overflows.
    pub fn add(&mut self, key: Option<NonceKey>, wei: U256) -> Option<()> {
        match key {
            Some(key) => {
                let most = self.groups.entry(key).or_default();
                *most = (*most).max(wei);
            }
            None => self.alone = self.alone.checked_add(wei)?,
        }
        Some(())
    }

    /// The most counted under `key` so far.
    pub fn of(&self, key: &NonceKey) -> U256 {
        self.groups.get(key).copied().unwrap_or_default()
    }

    /// What counting `wei` under `key` adds to [`Self::total`]: only what
    /// it is above the most of its alternatives.
    pub fn increase(&self, key: Option<NonceKey>, wei: U256) -> U256 {
        match key {
            Some(key) => wei.saturating_sub(self.of(&key)),
            None => wei,
        }
    }

    /// Every group's most and every cost alone (none: it overflows).
    pub fn total(&self) -> Option<U256> {
        self.groups
            .values()
            .try_fold(self.alone, |sum, wei| sum.checked_add(*wei))
    }

    fn wei(&self) -> Result<u128, String> {
        self.total()
            .and_then(|t| u128::try_from(t).ok())
            .ok_or_else(|| format!("{LIVE_SPEND_UNREADABLE}: the spend overflows"))
    }
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
    /// The sender and chain of that nonce (the lane's): the alternatives
    /// of one nonce spend once.
    pub origin: NonceOrigin,
    /// The last block its bundle may be included in (BSC3-048).
    pub max_block: u64,
    /// A BUY: the most the exits of the sender's open positions and of its
    /// own (approval and SELL at the signed gas) may still cost; the
    /// grant's spend keeps room for it (BSC3-057). An exit step: zero.
    pub exit_reserve_wei: u128,
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
    /// Absent in an entry written before its sender and chain were: it
    /// counts alone.
    pub origin: Option<NonceOrigin>,
}

impl Admitted {
    /// The nonce it is one of the alternatives of (none: not known whole).
    pub fn key(&self) -> Option<NonceKey> {
        Some(NonceKey {
            origin: self.origin?,
            nonce: self.nonce?,
        })
    }
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
        let bytes = fs::read(entry.path()).map_err(|_| bad())?;
        let (r, v2) = match decode(&ls::SCHEMA, &bytes) {
            Ok(r) => (r, true),
            Err(_) => (decode(&ls::V1, &bytes).map_err(|_| bad())?, false),
        };
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
            origin: match v2 {
                true => Some(NonceOrigin {
                    chain_id: r.u64(ls::CHAIN_ID).map_err(|_| bad())?,
                    sender: r.address(ls::SENDER).map_err(|_| bad())?,
                }),
                false => None,
            },
        });
        // An entry that names its sender and chain names its nonce too.
        if out
            .last()
            .is_some_and(|a| a.origin.is_some() && a.nonce.is_none())
        {
            return Err(bad());
        }
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
        let mut owner = None;
        for d in [dir.clone(), dir.join("generations"), dir.join("receipts")] {
            create_durable_dir(&d)?;
            if !fs::symlink_metadata(&d).is_ok_and(|m| m.is_dir()) {
                return Err(format!("{} is not a real directory", d.display()));
            }
            // BSC3-003: an existing directory is taken only owner-only, of
            // one owner, on a way no other user can change.
            let o = private_directory(&d).map_err(|r| r.to_string())?;
            if owner.is_some_and(|x| x != o) {
                return Err(format!("{} has another owner", d.display()));
            }
            owner = Some(o);
        }
        let owner = owner.ok_or("no store directory")?;
        Ok(Store {
            dir,
            owner,
            keep,
            boot,
        })
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
        self.consume_attempt_as(attempt_id, 0, now_ms, Durability::Full)
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
        self.consume_attempt_as(attempt_id, grant_generation, now_ms, Durability::Full)
    }

    fn consume_attempt_as(
        &self,
        attempt_id: &str,
        grant_generation: u64,
        now_ms: u64,
        durability: Durability,
    ) -> Result<(BuySlot, TotalSlot), String> {
        // BSC3-004: a clock set back takes no slot (its day would be one
        // already counted, or a day before the campaign's last attempt).
        if let Some(latest) = latest_slot_ms(&self.dir)?
            && now_ms < latest
        {
            return Err(format!(
                "BUY_ATTEMPT_CLOCK_BEHIND: now {now_ms} ms is before the last BUY attempt at {latest} ms"
            ));
        }
        let day = self.consume_daily(attempt_id, now_ms, durability)?;
        let total = self.consume_total(attempt_id, grant_generation, now_ms, durability)?;
        Ok((day, total))
    }

    /// The owner's LIVE spend lock, held until dropped: an exclusive lock
    /// of `live-spend.lock` under the owner state (released by the system
    /// when the process ends). Never waited for: held by another admission
    /// (another thread or process), or not to be taken, this one admits
    /// nothing; the clock it was given stays the admission's.
    fn spend_lock(&self) -> Result<fs::File, String> {
        let path = self.dir.join("live-spend.lock");
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("{LIVE_SPEND_UNREADABLE}: {}: {e}", path.display()))?;
        match file.try_lock() {
            Ok(()) => Ok(file),
            Err(fs::TryLockError::WouldBlock) => Err(format!(
                "{LIVE_SPEND_UNREADABLE}: {}: held by another admission",
                path.display()
            )),
            Err(fs::TryLockError::Error(e)) => {
                Err(format!("{LIVE_SPEND_UNREADABLE}: {}: {e}", path.display()))
            }
        }
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
        // One admission at a time, across threads and processes: what is
        // read and checked below is what the entry is written against.
        let _admitting = self.spend_lock()?;
        let g = self.current()?;
        let book =
            admitted_intents(&self.dir).and_then(|a| spend_of_grant(&a, armed.grant_generation));
        let spent = book.as_ref().ok().and_then(|b| b.wei().ok());
        after_spend_read();
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
        let (Ok(book), Some(spent)) = (&book, spent) else {
            return Err(LIVE_SPEND_UNREADABLE.into());
        };
        // An alternative of an admitted nonce adds only what it may lose
        // above the most of the others: one of them lands.
        let key = NonceKey {
            origin: intent.origin,
            nonce: intent.nonce,
        };
        let more = u128::try_from(book.increase(Some(key), U256::from(intent.max_wei)))
            .map_err(|_| LIVE_SPEND_UNREADABLE)?;
        let after = spent
            .checked_add(more)
            .filter(|after| *after <= grant.max_spend_wei)
            .ok_or(LIVE_GRANT_SPEND_EXHAUSTED)?;
        if !exit
            && after
                .checked_add(intent.exit_reserve_wei)
                .is_none_or(|kept| kept > grant.max_spend_wei)
        {
            return Err(LIVE_EXIT_RESERVE_MISSING.into());
        }
        let entry = spend_dir(&self.dir).join(format!("{}.dbrec", intent.id));
        if parse_hex32(intent.id).is_none() {
            return Err("a LIVE intent is named by its transaction hash".into());
        }
        if entry.exists() {
            return Err(LIVE_INTENT_SEEN.into());
        }
        let slots = match exit {
            false => Some(self.consume_attempt_as(
                intent.id,
                armed.grant_generation,
                now_ms,
                Durability::Handed,
            )?),
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
            .set(ls::MAX_BLOCK, intent.max_block)
            .set(ls::CHAIN_ID, intent.origin.chain_id)
            .set(
                ls::SENDER,
                darkbot_bsc_record::Value::Address(intent.origin.sender),
            )
            .set(
                ls::EXIT_RESERVE_WEI,
                darkbot_bsc_record::Value::u128(intent.exit_reserve_wei),
            );
        let bytes = d.encode().map_err(|e| e.to_string())?;
        create_durable_dir(&spend_dir(&self.dir))?;
        match write_new_as(&entry, &bytes, Durability::Handed) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                return Err(LIVE_INTENT_SEEN.into());
            }
            Err(e) => return Err(format!("{LIVE_SPEND_UNREADABLE}: entry not written: {e}")),
        }
        // BSC3-023: the slots and the entry were handed to the drive; one
        // flush puts them on the medium before the admission returns, so
        // before any byte of the intent can leave (BSC3-024). Not flushed:
        // no admission (they over-count, nothing went).
        let written: Vec<PathBuf> = slots
            .iter()
            .flat_map(|(day, _)| [buy_day_dir(&self.dir, &day.day), total_dir(&self.dir)])
            .collect();
        flush_device(&spend_dir(&self.dir), &written)
            .map_err(|e| format!("{LIVE_SPEND_UNREADABLE}: not on the medium: {e}"))?;
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
        durability: Durability,
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
            if *slot >= u64::from(limit) {
                return Err(format!(
                    "BUY_ATTEMPT_TOTAL_REACHED: slot {slot} is beyond the limit {limit}"
                ));
            }
            // Review (BSC3-023): its link may have been all of a write that
            // ended before its directory was synced.
            sync_dir_as(&total_dir(&self.dir), durability)
                .map_err(|e| format!("total buy attempt slot not synced: {e}"))?;
            return Ok(TotalSlot { slot: *slot, limit });
        }
        for slot in 0..u64::from(limit) {
            if taken.iter().any(|(t, _, _)| *t == slot) {
                continue;
            }
            let mut d = Draft::new(&ts::SCHEMA);
            d.set(ts::ATTEMPT_ID, attempt_id)
                .set(ts::SLOT, slot)
                .set(ts::LIMIT, u64::from(limit))
                .set(ts::GENERATION, g.generation)
                .set(ts::GRANT_GENERATION, grant_generation)
                .set(ts::PROVENANCE_TOOL, "bsc_runtime")
                .set(ts::PROVENANCE_OBSERVED_UNIX_MS, now_ms);
            let bytes = d.encode().map_err(|e| e.to_string())?;
            match write_new_as(
                &total_dir(&self.dir).join(format!("{slot:010}.dbrec")),
                &bytes,
                durability,
            ) {
                Ok(()) => {
                    let now = self.current()?.buy_attempts_total();
                    if slot >= u64::from(now) {
                        return Err(format!(
                            "BUY_ATTEMPT_TOTAL_REACHED: slot {slot} taken while the limit fell to {now}"
                        ));
                    }
                    return Ok(TotalSlot { slot, limit });
                }
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

    fn consume_daily(
        &self,
        attempt_id: &str,
        now_ms: u64,
        durability: Durability,
    ) -> Result<BuySlot, String> {
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
        if let Some((day, slot)) = daily_slot_of(&self.dir, attempt_id)? {
            // Review M2: a retry finds its slot, within the limit in force.
            if slot >= u64::from(limit) {
                return Err(format!(
                    "BUY_ATTEMPT_LIMIT_REACHED: slot {slot} of {} is beyond the limit {limit}",
                    day.utc
                ));
            }
            sync_dir_as(&buy_day_dir(&self.dir, &day), durability)
                .map_err(|e| format!("buy attempt slot not synced: {e}"))?;
            return Ok(BuySlot { day, slot, limit });
        }
        let taken = buy_slots(&day_dir)?;
        for slot in 0..u64::from(limit) {
            // BSC3-023: a slot seen taken costs no write (one taken
            // meanwhile still fails its link below).
            if taken.iter().any(|(t, _)| *t == slot) {
                continue;
            }
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
            before_slot_write();
            match write_new_as(
                &day_dir.join(format!("{slot:010}.dbrec")),
                &bytes,
                durability,
            ) {
                Ok(()) => {
                    // BSC3-004: a limit lowered meanwhile holds; the slot
                    // stays taken (counted, never given back).
                    let now = self.current()?.buy_attempts_per_day();
                    if slot >= u64::from(now) {
                        return Err(format!(
                            "BUY_ATTEMPT_LIMIT_REACHED: slot {slot} taken while the limit fell to {now}"
                        ));
                    }
                    return Ok(BuySlot { day, slot, limit });
                }
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
        // BSC3-069: a new grant names the execution risk it admits.
        if grant.risk.is_none() {
            return Err(format!(
                "{LIVE_GRANT_RISK_MISSING}: a LIVE grant names its execution risk"
            ));
        }
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
        if !entity.accepts(requested)
            || (entity == Entity::BuyLimit
                && matches!(requested, Mode::PerDay(n) if n > MAX_BUY_ATTEMPTS_TOTAL))
        {
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
        match read_private(&receipt_path, self.owner) {
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
        } else if expected_generation < current.generation {
            // BSC3-006: later requests moved the generation on since; the
            // generation this request would have written tells whether it
            // did (its receipt lost), else another request took it. One no
            // longer kept tells nothing: UNRESOLVED, never a guess.
            let path = self.generation_path(expected_generation.saturating_add(1));
            match read_private(&path, self.owner) {
                Ok(bytes) => {
                    let then = Generation::decode(&bytes)?;
                    if then.request_id.as_deref() == Some(request_id) {
                        if !then.applied(request_id, entity, requested) {
                            return Err("the request ID already belongs to another request".into());
                        }
                        receipt.outcome = "APPLIED";
                        receipt.generation = then.generation;
                        receipt.effective = then.effective_of(entity);
                    }
                }
                Err(e) if e.kind() == ErrorKind::NotFound => receipt.outcome = "UNRESOLVED",
                Err(e) => return Err(format!("{}: {e}", path.display())),
            }
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
                let r = Receipt::decode(
                    &read_private(&receipt_path, self.owner).map_err(|e| e.to_string())?,
                )?;
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
    let bytes = read_private(&path, generations_owner(dir)?).map_err(|e| e.to_string())?;
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

    /// BSC3-002/021: each owner enum's ALL lists every value at its own
    /// fixed index, each index once (no value reads another's, none index
    /// 0 by default).
    #[test]
    fn every_value_has_its_own_index() {
        fn check<T: Copy>(all: &[T], index: impl Fn(T) -> usize) {
            let mut seen = vec![false; all.len()];
            for (i, x) in all.iter().enumerate() {
                assert_eq!(index(*x), i);
                assert!(!std::mem::replace(&mut seen[i], true));
            }
        }
        check(&SourceId::ALL, SourceId::index);
        check(&ComponentId::ALL, ComponentId::index);
        check(&VenueId::ALL, VenueId::index);
        check(&SwitchId::ALL, SwitchId::index);
    }

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

    /// BSC3-007: the account home is the real user's: the first `Uid:` field
    /// (not the effective second), the passwd record of exactly that UID
    /// (none: refused), a record without a shell counts.
    #[test]
    fn the_account_home_is_the_real_users() {
        let status = "Name:\tbsc\nUid:\t1000\t0\t0\t0\nGid:\t1000\t1000\t1000\t1000\n";
        assert_eq!(real_uid_of_status(status), Some("1000"));
        assert_eq!(real_uid_of_status("Name:\tbsc\n"), None);
        let passwd = "root:x:0:0:root:/root:/bin/sh\nbsc:x:1000:1000::/home/bsc:\n";
        assert_eq!(home_of_passwd(passwd, "1000"), Some("/home/bsc"));
        assert_eq!(home_of_passwd(passwd, "0"), Some("/root"));
        assert_eq!(home_of_passwd(passwd, "1001"), None);
        assert_eq!(home_of_passwd("bsc:x:1000:1000::/home/bsc\n", "1000"), None);
        assert_eq!(
            home_of_dscache("name: bsc\npassword: *\nuid: 501\ndir: /Users/bsc\n"),
            Some("/Users/bsc")
        );
        assert_eq!(home_of_dscache(""), None);
        // This host's account home: an absolute directory of the real user.
        let home = account_home().unwrap();
        assert!(home.is_absolute() && home.is_dir(), "{}", home.display());
    }

    /// BSC3-003: the owner's state is taken only from owner-only directories
    /// on a way no other user can change, and its files only as private
    /// files of that owner read through their own name.
    #[test]
    fn an_exposed_or_linked_store_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("bsc-store-exposed-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(&base)
            .unwrap();
        let dir = base.join("bsc-sources");
        let s = Store::open_in_session(&dir, 8, Ok("boot-1".into())).unwrap();
        s.request("a", SwitchId::Sign, Mode::Enabled, 0, 1).unwrap();
        assert_eq!(require_enabled(&dir, "boot-1", &[SwitchId::Sign]), Ok(1));
        let mode =
            |p: &Path, m: u32| fs::set_permissions(p, fs::Permissions::from_mode(m)).unwrap();
        // An existing directory others may read, or a way others may change:
        // no store opens on it; a switch is required only from state no
        // other user can change.
        for (p, m, required) in [
            (dir.clone(), 0o755, true),
            (dir.join("generations"), 0o750, false),
            (base.clone(), 0o777, false),
        ] {
            mode(&p, m);
            let e = Store::open_in_session(&dir, 8, Ok("boot-1".into()))
                .err()
                .unwrap();
            assert!(
                e.contains("accessible to others") || e.contains("changed by another user"),
                "{e}"
            );
            let r = require_enabled(&dir, "boot-1", &[SwitchId::Sign]);
            match required {
                true => assert_eq!(r, Ok(1)),
                false => assert!(r.unwrap_err().contains("CONTROL_STATE_UNREADABLE")),
            }
            mode(&p, 0o700);
        }
        mode(&base, 0o1777);
        assert!(Store::open_in_session(&dir, 8, Ok("boot-1".into())).is_ok());
        mode(&base, 0o700);
        // The current generation writable by others, or a link to another file.
        let current = s.generation_path(1);
        mode(&current, 0o666);
        assert!(s.current().unwrap_err().contains("not a private file"));
        mode(&current, 0o600);
        let elsewhere = base.join("elsewhere.dbrec");
        fs::copy(&current, &elsewhere).unwrap();
        fs::remove_file(&current).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &current).unwrap();
        assert!(s.current().unwrap_err().contains("not a private file"));
        assert!(current_record_and_generation(&dir).is_err());
        fs::remove_file(&current).unwrap();
        fs::rename(&elsewhere, &current).unwrap();
        assert_eq!(s.current().unwrap().generation, 1);
        // A receipt that is a link: no answer from it.
        let receipt = dir.join("receipts/a.dbrec");
        let copy = base.join("receipt.dbrec");
        fs::copy(&receipt, &copy).unwrap();
        fs::remove_file(&receipt).unwrap();
        std::os::unix::fs::symlink(&copy, &receipt).unwrap();
        assert!(s.request("a", SwitchId::Sign, Mode::Enabled, 0, 2).is_err());
        fs::remove_dir_all(base).unwrap();
    }

    /// BSC3-006: a lost receipt is recovered from the generation its request
    /// wrote, also after later requests moved the generation on; another
    /// payload under its ID is refused; a request that met a generation no
    /// longer kept is UNRESOLVED, a stale one REJECTED_STALE.
    #[test]
    fn a_lost_receipt_is_recovered_after_later_generations() {
        let (dir, s) = store("crash-later");
        s.request("a", SourceId::Geth, Mode::Shadow, 0, 1).unwrap();
        fs::remove_file(dir.join("receipts/a.dbrec")).unwrap();
        s.request("b", SourceId::Drpc, Mode::Shadow, 1, 2).unwrap();
        let r = s.request("a", SourceId::Geth, Mode::Shadow, 0, 3).unwrap();
        assert_eq!((r.outcome, r.generation), ("APPLIED", 1));
        assert_eq!(r.effective, Effective::Shadow);
        // The same again: its receipt now.
        assert_eq!(
            s.request("a", SourceId::Geth, Mode::Shadow, 0, 4).unwrap(),
            r
        );
        // Another payload under a lost receipt's ID is no recovery.
        s.request("c", SourceId::Geth, Mode::Off, 2, 5).unwrap();
        fs::remove_file(dir.join("receipts/c.dbrec")).unwrap();
        s.request("d", SourceId::Drpc, Mode::Off, 3, 6).unwrap();
        assert!(
            s.request("c", SourceId::Geth, Mode::Shadow, 2, 7)
                .unwrap_err()
                .contains("another request")
        );
        // A request that met generation 1 but never wrote: stale.
        let stale = s.request("e", SourceId::Geth, Mode::Off, 1, 8).unwrap();
        assert_eq!(stale.outcome, "REJECTED_STALE");
        // The generation it would have written no longer kept: unresolved.
        fs::remove_file(s.generation_path(1)).unwrap();
        fs::remove_file(dir.join("receipts/a.dbrec")).unwrap();
        let unknown = s.request("a", SourceId::Geth, Mode::Shadow, 0, 9).unwrap();
        assert_eq!(unknown.outcome, "UNRESOLVED");
        assert!(!unknown.applied());
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

    /// BSC3-004: a day never allows more than the campaign; a clock set back
    /// takes no slot; an attempt tried again after midnight UTC is the same
    /// attempt (no second daily slot); a limit lowered between its read and
    /// the slot's write holds.
    #[test]
    fn the_daily_limit_holds_across_days_clocks_and_changes() {
        const DAY: u64 = 86_400_000;
        let (dir, s) = session("buy-days", "boot-1");
        assert!(
            s.request("bl-11", Entity::BuyLimit, Mode::PerDay(11), 0, 1)
                .is_err()
        );
        s.request("bt", Entity::BuyTotal, Mode::PerDay(10), 0, 1)
            .unwrap();
        s.request("bl", Entity::BuyLimit, Mode::PerDay(10), 1, 1)
            .unwrap();
        let (day1, _) = s.consume_attempt("a", 0, DAY + 5).unwrap();
        // The same attempt after midnight: its slot of the first day.
        let (again, total) = s.consume_attempt("a", 0, 2 * DAY + 5).unwrap();
        assert_eq!((again.day, again.slot), (day1.day.clone(), day1.slot));
        assert_eq!(total.slot, 0);
        assert_eq!(buy_usage(&dir, 2 * DAY + 5).unwrap().used, 0);
        // A clock set back before the last attempt: refused, nothing taken.
        let e = s.consume_attempt("b", 0, DAY + 4).unwrap_err();
        assert!(e.contains("BUY_ATTEMPT_CLOCK_BEHIND"), "{e}");
        assert_eq!(total_usage(&dir).unwrap().used, 1);
        // The daily limit lowered to 1 after it was read for "c": c's slot
        // 1 is taken (counted) but refused.
        let g = s.current().unwrap().generation;
        let at = dir.clone();
        let lower = move || {
            let owner = Store::open_in_session(&at, 8, Ok("boot-1".into())).unwrap();
            owner
                .request("bl-1", Entity::BuyLimit, Mode::PerDay(1), g, DAY + 6)
                .unwrap();
        };
        BEFORE_SLOT_WRITE.with(|h| *h.borrow_mut() = Some(Box::new(lower)));
        let e = s.consume_attempt("c", 0, DAY + 7).unwrap_err();
        BEFORE_SLOT_WRITE.with(|h| *h.borrow_mut() = None);
        assert!(e.contains("limit fell to 1"), "{e}");
        assert_eq!(buy_usage(&dir, DAY + 7).unwrap().used, 2);
        // Review M2: tried again, its slot is still beyond the limit.
        let e = s.consume_attempt("c", 0, DAY + 8).unwrap_err();
        assert!(e.contains("beyond the limit"), "{e}");
        // Review M1: a daily limit stored above the campaign's (an older
        // build took any) still decodes; the limit in force is capped.
        let mut stored = s.current().unwrap();
        stored.set(
            Entity::BuyLimit,
            Mode::PerDay(20),
            SetBy {
                request_id: "old".into(),
                generation: stored.generation,
            },
        );
        let back = Generation::decode(&stored.encode(1).unwrap()).unwrap();
        assert_eq!(back.buy_attempts_per_day(), MAX_BUY_ATTEMPTS_TOTAL);
        // Review L4: a stray file among the days blocks nothing.
        fs::write(dir.join("buy-attempts/.DS_Store"), b"x").unwrap();
        assert!(s.consume_attempt("a", 0, DAY + 9).is_ok());
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

    /// The buyer and chain of the LIVE intents of these tests.
    pub(crate) const ORIGIN: NonceOrigin = NonceOrigin {
        chain_id: 56,
        sender: [0xb0; 20],
    };

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
            exit_only: false,
            risk: Some(TEST_RISK),
        }
    }

    /// A granted risk the tests' executor signs within.
    pub(crate) const TEST_RISK: GrantRisk = GrantRisk {
        max_buy_slippage_bps: 1_000,
        max_exit_slippage_bps: 1_000,
        max_gas_price_wei: 1_000_000_000_000,
        max_buy_gas_limit: 10_000_000,
        max_approve_gas_limit: 1_000_000,
        max_sell_gas_limit: 5_000_000,
    };

    /// Evidence for exactly that scope.
    pub(crate) fn qualified() -> LiveQualification {
        LiveQualification {
            venues: vec![VenueId::Flap],
            sources: vec![SourceId::PublicNode],
            adapters: vec![LiveAdapter::Club48BuilderV1],
            acquisition: true,
            acquisition_adapters: vec![LiveAdapter::Club48BuilderV1],
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
        live_ready_limited(tag, (2, 3), grant)
    }

    /// [`live_ready_granting`] with the daily and campaign BUY limits.
    pub(crate) fn live_ready_limited(
        tag: &str,
        (per_day, in_total): (u32, u32),
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
        req(Entity::BuyLimit, Mode::PerDay(per_day));
        req(Entity::BuyTotal, Mode::PerDay(in_total));
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
        // BSC3-049: an acquisition sent through another builder only.
        let mut other_builder = qualified();
        other_builder.acquisition_adapters = vec![LiveAdapter::BlockRazorBuilderV1];
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
                    qualification: &other_builder,
                    ..ctx
                },
                LIVE_ACQUISITION_BUILDER_UNPROVEN,
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
        // BSC3-049 review: a builder without the acquisition still takes the
        // exits of what the grant holds.
        assert!(
            g.exit_blockers(&LiveContext {
                qualification: &other_builder,
                ..ctx
            })
            .is_empty()
        );
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

    /// BSC3-069: a grant names the execution risk the operator confirms:
    /// its scope text carries every bound (another risk, another digest),
    /// the owner record keeps it, a bound no transaction is signed within
    /// is refused; a new grant without one is refused, one held without it
    /// admits nothing, exits neither (GRANT_RISK); every bound an executor
    /// is beyond is named (a grant of exits only: not the BUY's).
    #[test]
    fn a_grant_names_the_execution_risk_it_admits() {
        let (dir, s, now) = live_ready_granting("live-risk", |now| {
            grant_of(now + 3_600_000, LiveStrategies::C3)
        });
        let q = qualified();
        let g = s.current().unwrap();
        let grant = g.live.clone().unwrap();
        assert_eq!(grant.risk, Some(TEST_RISK));
        assert!(grant.scope_text().ends_with(
            "max_buy_slippage_bps=1000\nmax_exit_slippage_bps=1000\nmax_gas_price_wei=1000000000000\n\
             max_buy_gas_limit=10000000\nmax_approve_gas_limit=1000000\nmax_sell_gas_limit=5000000\n"
        ));
        let tighter = LiveGrant {
            risk: Some(GrantRisk {
                max_buy_slippage_bps: 999,
                ..TEST_RISK
            }),
            ..grant.clone()
        };
        assert_ne!(tighter.scope_sha256(), grant.scope_sha256());
        assert!(s.live_now(&[ComponentId::ExecC3], &q, now).is_ok());
        for bad in [
            GrantRisk {
                max_buy_slippage_bps: 10_000,
                ..TEST_RISK
            },
            GrantRisk {
                max_exit_slippage_bps: 10_000,
                ..TEST_RISK
            },
            GrantRisk {
                max_gas_price_wei: 0,
                ..TEST_RISK
            },
            GrantRisk {
                max_approve_gas_limit: 20_999,
                ..TEST_RISK
            },
        ] {
            let refused = LiveGrant {
                risk: Some(bad),
                ..grant.clone()
            };
            assert!(refused.validate().is_err(), "{bad:?}");
        }
        // Every bound on its own: at the granted value it admits, one above
        // it it names that bound; several at once are all named; a grant
        // of exits only ignores the BUY's two.
        let signed = SignedRisk {
            buy_slippage_bps: 1_000,
            exit_slippage_bps: 1_000,
            gas_price_wei: 1_000_000_000_000,
            buy_gas_limit: 10_000_000,
            approve_gas_limit: 1_000_000,
            sell_gas_limit: 5_000_000,
        };
        assert_eq!(TEST_RISK.admits(&signed, false), Ok(()));
        let cases: [(&str, SignedRisk, bool); 6] = [
            (
                "buy_slippage_bps 1001 above 1000",
                SignedRisk {
                    buy_slippage_bps: 1_001,
                    ..signed
                },
                true,
            ),
            (
                "exit_slippage_bps 1001 above 1000",
                SignedRisk {
                    exit_slippage_bps: 1_001,
                    ..signed
                },
                false,
            ),
            (
                "gas_price_wei 1000000000001 above 1000000000000",
                SignedRisk {
                    gas_price_wei: 1_000_000_000_001,
                    ..signed
                },
                false,
            ),
            (
                "buy_gas_limit 10000001 above 10000000",
                SignedRisk {
                    buy_gas_limit: 10_000_001,
                    ..signed
                },
                true,
            ),
            (
                "approve_gas_limit 1000001 above 1000000",
                SignedRisk {
                    approve_gas_limit: 1_000_001,
                    ..signed
                },
                false,
            ),
            (
                "sell_gas_limit 5000001 above 5000000",
                SignedRisk {
                    sell_gas_limit: 5_000_001,
                    ..signed
                },
                false,
            ),
        ];
        for (named, over, buy_only) in cases {
            assert_eq!(
                TEST_RISK.admits(&over, false),
                Err(format!("{LIVE_RISK_OUTSIDE_GRANT}: {named}"))
            );
            assert_eq!(TEST_RISK.admits(&over, true).is_ok(), buy_only, "{named}");
        }
        assert_eq!(
            TEST_RISK.admits(
                &SignedRisk {
                    buy_slippage_bps: 1_001,
                    sell_gas_limit: 5_000_001,
                    ..signed
                },
                false
            ),
            Err(format!(
                "{LIVE_RISK_OUTSIDE_GRANT}: buy_slippage_bps 1001 above 1000; sell_gas_limit 5000001 above 5000000"
            ))
        );
        for bad in [
            GrantRisk {
                max_buy_gas_limit: 20_999,
                ..TEST_RISK
            },
            GrantRisk {
                max_sell_gas_limit: 20_999,
                ..TEST_RISK
            },
        ] {
            let refused = LiveGrant {
                risk: Some(bad),
                ..grant.clone()
            };
            assert!(refused.validate().is_err(), "{bad:?}");
        }
        // A new grant without the risk is refused, nothing written.
        let none = LiveGrant {
            risk: None,
            ..grant.clone()
        };
        let before = s.current().unwrap().generation;
        assert!(
            s.grant_live("no-risk", none.clone(), none.scope_sha256(), before, now)
                .unwrap_err()
                .starts_with(LIVE_GRANT_RISK_MISSING)
        );
        assert_eq!(s.current().unwrap().generation, before);
        // A grant held without one (written before it): nothing is LIVE,
        // exits neither.
        let mut g = s.current().unwrap();
        g.live.as_mut().unwrap().risk = None;
        let (_, daily, total) = live_inputs(&dir, now).unwrap();
        let ctx = LiveContext {
            boot: Some("boot-1"),
            now_unix_ms: now,
            daily: Some(&daily),
            total: Some(&total),
            spent_wei: Some(0),
            qualification: &q,
        };
        assert!(
            g.live_checklist(&ctx)
                .contains(&("GRANT_RISK", Some(LIVE_GRANT_RISK_MISSING)))
        );
        assert!(g.live_blockers(&ctx).contains(&LIVE_GRANT_RISK_MISSING));
        assert!(g.exit_blockers(&ctx).contains(&LIVE_GRANT_RISK_MISSING));
        fs::remove_dir_all(dir).unwrap();
    }

    /// BSC3-058: the operator's separate EXIT-ONLY grant. Its scope says
    /// so (another digest than the same scope with BUYs; a grant with BUYs
    /// keeps its text) and survives the owner's record; under it no BUY is
    /// admitted (LIVE_GRANT_EXIT_ONLY, nothing consumed), the gate for BUYs
    /// is blocked and names it, the exits are admitted. A restart and its
    /// expiry still end it: nothing economic goes by itself.
    #[test]
    fn an_exit_only_grant_admits_exits_and_no_buy() {
        let (dir, s, now) = live_ready_granting("live-exit-only", |now| LiveGrant {
            exit_only: true,
            ..grant_of(now + 3_600_000, LiveStrategies::C3)
        });
        let q = qualified();
        let g = s.current().unwrap();
        let grant = g.live.clone().unwrap();
        assert!(grant.exit_only);
        assert!(grant.scope_text().ends_with("exit_only=true\n"));
        let with_buys = LiveGrant {
            exit_only: false,
            ..grant.clone()
        };
        assert!(!with_buys.scope_text().contains("exit_only"));
        assert_ne!(grant.scope_sha256(), with_buys.scope_sha256());
        assert_eq!(
            s.live_now(&[ComponentId::ExecC3], &q, now).unwrap_err(),
            LIVE_GRANT_EXIT_ONLY
        );
        let (_, daily, total) = live_inputs(&dir, now).unwrap();
        let checklist = g.live_checklist(&LiveContext {
            boot: Some("boot-1"),
            now_unix_ms: now,
            daily: Some(&daily),
            total: Some(&total),
            spent_wei: Some(0),
            qualification: &q,
        });
        assert!(checklist.contains(&("GRANT_BUYS", Some(LIVE_GRANT_EXIT_ONLY))));
        assert_eq!(checklist.iter().filter(|(_, r)| r.is_some()).count(), 1);
        let armed = s.live_now_exits(&[ComponentId::ExecC3], &q, now).unwrap();
        let ids: Vec<String> = (0..3u8).map(|n| hex32(&[n; 32])).collect();
        let intent = |n: usize, kind: LiveIntentKind| LiveIntent {
            id: &ids[n],
            component: ComponentId::ExecC3,
            kind,
            input_wei: if kind == LiveIntentKind::Buy {
                1_000
            } else {
                0
            },
            max_wei: 2_000,
            token: [0x10; 20],
            nonce: 40 + n as u64,
            origin: ORIGIN,
            max_block: 900,
            exit_reserve_wei: 0,
        };
        assert_eq!(
            s.admit_live(&armed, &intent(0, LiveIntentKind::Buy), &q, now)
                .unwrap_err(),
            LIVE_GRANT_EXIT_ONLY
        );
        assert_eq!(buy_usage(&dir, now).unwrap().used, 0);
        assert_eq!(spent_by_grant(&dir, armed.grant_generation()).unwrap(), 0);
        assert!(
            s.admit_live(&armed, &intent(1, LiveIntentKind::Approve), &q, now)
                .is_ok()
        );
        // Expired: no exit either.
        let late = grant.expires_unix_ms;
        assert_eq!(
            s.live_now_exits(&[ComponentId::ExecC3], &q, late)
                .unwrap_err(),
            LIVE_GRANT_EXPIRED
        );
        // A restart (another boot session): the switches and the grant are
        // reset, no exit until the operator authorizes again.
        let restarted = Store::open_in_session(&dir, 8, Ok("boot-2".into())).unwrap();
        assert!(
            restarted
                .live_now_exits(&[ComponentId::ExecC3], &q, now)
                .is_err()
        );
        let after = g.exit_blockers(&LiveContext {
            boot: Some("boot-2"),
            now_unix_ms: now,
            daily: Some(&daily),
            total: Some(&total),
            spent_wei: Some(0),
            qualification: &q,
        });
        assert!(after.contains(&LIVE_GRANT_RESET_BY_RESTART), "{after:?}");
        fs::remove_dir_all(dir).unwrap();
    }

    /// BSC3-057: a BUY leaves the grant the room the exits of what is open
    /// and of its own position need (approval and SELL at the signed gas);
    /// a BUY that would not is refused before any byte leaves (before: the
    /// BUY was admitted and its SELL refused LIVE_GRANT_SPEND_EXHAUSTED).
    #[test]
    fn a_buy_leaves_the_grant_room_for_its_exit() {
        let (dir, s, now) = live_ready("live-exit-reserve");
        let q = qualified();
        let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
        let most = armed.grant().max_spend_wei;
        let ids: Vec<String> = (0..4u8).map(|n| hex32(&[n; 32])).collect();
        let (approve, sell) = (100u128, 500u128);
        let intent = |n: usize, kind: LiveIntentKind, max: u128| LiveIntent {
            id: &ids[n],
            component: ComponentId::ExecC3,
            kind,
            input_wei: if kind == LiveIntentKind::Buy {
                1_000
            } else {
                0
            },
            max_wei: max,
            token: [0x10; 20],
            nonce: 40 + n as u64,
            origin: ORIGIN,
            max_block: 900,
            exit_reserve_wei: if kind == LiveIntentKind::Buy {
                approve + sell
            } else {
                0
            },
        };
        // A BUY that leaves less than its exit costs is refused, nothing
        // consumed.
        let short = intent(0, LiveIntentKind::Buy, most - approve - sell + 1);
        assert_eq!(
            s.admit_live(&armed, &short, &q, now).unwrap_err(),
            LIVE_EXIT_RESERVE_MISSING
        );
        assert_eq!(spent_by_grant(&dir, armed.grant_generation()).unwrap(), 0);
        assert_eq!(buy_usage(&dir, now).unwrap().used, 0);
        // One that leaves exactly that: it, its approval and its SELL go.
        let buy = intent(1, LiveIntentKind::Buy, most - approve - sell);
        assert!(s.admit_live(&armed, &buy, &q, now).is_ok());
        let approval = s.admit_live(
            &armed,
            &intent(2, LiveIntentKind::Approve, approve),
            &q,
            now,
        );
        assert!(approval.is_ok(), "{approval:?}");
        let sold = s.admit_live(&armed, &intent(3, LiveIntentKind::Sell, sell), &q, now);
        assert_eq!(sold.unwrap().spent_wei, most);
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
            origin: ORIGIN,
            max_block: 900 + n as u64,
            exit_reserve_wei: 0,
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
                origin: Some(ORIGIN),
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

    /// A LIVE BUY of the tests' buyer at `nonce`, named `id`, of at most
    /// `most`.
    fn nonce_buy(id: &str, nonce: u64, most: u128) -> LiveIntent<'_> {
        LiveIntent {
            id,
            component: ComponentId::ExecC3,
            kind: LiveIntentKind::Buy,
            input_wei: 1_000,
            max_wei: most,
            token: [0x10; 20],
            nonce,
            origin: ORIGIN,
            max_block: 900,
            exit_reserve_wei: 0,
        }
    }

    /// An admitted intent `n` of grant `grant` as an entry holds it.
    fn admitted_of(
        n: u8,
        grant: u64,
        nonce: Option<u64>,
        origin: Option<NonceOrigin>,
        most: u128,
    ) -> Admitted {
        Admitted {
            tx_hash: [n; 32],
            kind: LiveIntentKind::Buy,
            component: ComponentId::ExecC3,
            grant_generation: grant,
            input_wei: 1,
            max_wei: most,
            token: Some([0x10; 20]),
            nonce,
            max_block: Some(900),
            origin,
        }
    }

    /// `a`'s spend entry as the owner writes it, or (no origin) as an older
    /// build wrote it (v1, no sender and chain).
    fn spend_entry(dir: &Path, a: &Admitted) {
        use darkbot_bsc_record::Value;
        use darkbot_bsc_record::schema::bsc_runtime::live_spend as ls;
        let mut d = Draft::new(match a.origin {
            Some(_) => &ls::SCHEMA,
            None => &ls::V1,
        });
        d.set(ls::INTENT_ID, hex32(&a.tx_hash).as_str())
            .set(ls::KIND, a.kind.name())
            .set(ls::COMPONENT, a.component.name())
            .set(ls::PROVENANCE_TOOL, "test")
            .set(ls::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(ls::GRANT_GENERATION, a.grant_generation)
            .set(ls::INPUT_WEI, Value::u128(a.input_wei))
            .set(ls::MAX_WEI, Value::u128(a.max_wei))
            .set_opt(ls::TOKEN, a.token.map(Value::Address))
            .set_opt(ls::NONCE, a.nonce)
            .set_opt(ls::MAX_BLOCK, a.max_block);
        if let Some(o) = a.origin {
            d.set(ls::CHAIN_ID, o.chain_id)
                .set(ls::SENDER, Value::Address(o.sender));
        }
        create_durable_dir(&spend_dir(dir)).unwrap();
        let path = spend_dir(dir).join(format!("{}.dbrec", hex32(&a.tx_hash)));
        fs::write(path, d.encode().unwrap()).unwrap();
    }

    /// Budget fix (tests A, M, G): the alternatives of one (chain, sender,
    /// nonce) spend their most once, whichever lands: a higher one adds only
    /// its difference, an equal or lower one nothing; another nonce adds its
    /// own most; the grant is checked against all of it. Every entry stays,
    /// and every BUY still takes its own daily, campaign and grant slot.
    #[test]
    fn alternatives_of_one_nonce_spend_only_their_most() {
        let (dir, s, now) = live_ready_limited("live-nonce-alt", (6, 6), |now| LiveGrant {
            max_attempts: 6,
            max_input_wei: 1_000,
            max_spend_wei: 21_000,
            ..grant_of(now + 3_600_000, LiveStrategies::C3)
        });
        let q = qualified();
        let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
        let generation = armed.grant_generation();
        let ids: Vec<String> = (0..10u8).map(|n| hex32(&[n; 32])).collect();
        let admit = |n: usize, nonce: u64, most: u128| {
            s.admit_live(&armed, &nonce_buy(&ids[n], nonce, most), &q, now)
                .map(|a| a.spent_wei)
        };
        assert_eq!(admit(0, 7, 10_000), Ok(10_000));
        assert_eq!(admit(1, 7, 12_000), Ok(12_000));
        assert_eq!(admit(2, 7, 11_000), Ok(12_000));
        assert_eq!(admit(3, 7, 12_000), Ok(12_000));
        assert_eq!(spent_by_grant(&dir, generation).unwrap(), 12_000);
        // Nonce 8: its own most on top; 12k + 9,001 is past the grant.
        assert_eq!(admit(4, 8, 9_001), Err(LIVE_GRANT_SPEND_EXHAUSTED.into()));
        assert_eq!(admit(5, 8, 7_999), Ok(19_999));
        // Above nonce 7's most only the difference counts: 1,002 more is
        // past the grant, 1,001 fills it.
        assert_eq!(admit(6, 7, 13_002), Err(LIVE_GRANT_SPEND_EXHAUSTED.into()));
        assert_eq!(admit(6, 7, 13_001), Ok(21_000));
        // An exit alternative of nonce 8 within its most still fits the
        // full grant.
        let exit = LiveIntent {
            kind: LiveIntentKind::Approve,
            input_wei: 0,
            ..nonce_buy(&ids[7], 8, 7_999)
        };
        assert_eq!(
            s.admit_live(&armed, &exit, &q, now).unwrap().spent_wei,
            21_000
        );
        // Each BUY took its slots, whatever it added (two added nothing):
        // the day's six are gone.
        assert_eq!(admit(8, 7, 1), Err(BUY_LIMIT_EXHAUSTED.into()));
        assert_eq!(buy_usage(&dir, now).unwrap().used, 6);
        let total = total_usage(&dir).unwrap();
        assert_eq!((total.used, total.grants), (6, vec![generation; 6]));
        // Every admitted entry stays, refused ones were never written.
        assert_eq!(admitted_intents(&dir).unwrap().len(), 7);
        assert_eq!(spent_by_grant(&dir, generation).unwrap(), 21_000);
        fs::remove_dir_all(dir).unwrap();
    }

    /// Budget fix (tests I, J, K): only one (chain, sender, nonce) groups.
    /// The same nonce under another grant counts there alone; of another
    /// sender or chain, in full; an entry without its sender and chain (an
    /// older build's) in full, and never with another of its nonce.
    #[test]
    fn only_one_chain_sender_and_nonce_groups() {
        let dir = std::env::temp_dir().join(format!("bsc-spend-groups-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let sender = NonceOrigin {
            sender: [0xb1; 20],
            ..ORIGIN
        };
        let chain = NonceOrigin {
            chain_id: 97,
            ..ORIGIN
        };
        for a in [
            admitted_of(1, 5, Some(7), Some(ORIGIN), 10_000),
            admitted_of(2, 6, Some(7), Some(ORIGIN), 12_000),
            admitted_of(3, 5, Some(7), Some(sender), 3_000),
            admitted_of(4, 5, Some(7), Some(chain), 4_000),
            admitted_of(5, 5, Some(7), None, 5_000),
            admitted_of(6, 5, Some(7), None, 6_000),
            admitted_of(7, 5, None, None, 700),
        ] {
            spend_entry(&dir, &a);
        }
        let old = 5_000 + 6_000 + 700;
        assert_eq!(
            spent_by_grant(&dir, 5).unwrap(),
            10_000 + 3_000 + 4_000 + old
        );
        assert_eq!(spent_by_grant(&dir, 6).unwrap(), 12_000);
        let read = admitted_intents(&dir).unwrap();
        assert_eq!(read.iter().filter(|a| a.origin.is_none()).count(), 3);
        assert_eq!(read.iter().filter(|a| a.key().is_some()).count(), 4);
        // An alternative of grant 5's nonce 7 above its most adds the
        // difference only.
        spend_entry(&dir, &admitted_of(8, 5, Some(7), Some(ORIGIN), 11_000));
        assert_eq!(
            spent_by_grant(&dir, 5).unwrap(),
            11_000 + 3_000 + 4_000 + old
        );
        assert_eq!(spent_by_grant(&dir, 6).unwrap(), 12_000);
        fs::remove_dir_all(dir).unwrap();
    }

    /// BSC3-023: an admission hands its slots and its entry to the drive,
    /// each record's content ordered before its link (a barrier, not a
    /// plain fsync), and puts them on the medium with one flush, its last
    /// step before it returns (it was a medium flush per file and
    /// directory, six for a BUY); a retry's slots found taken have their
    /// directory synced again; a G0.7 attempt's slots stay durable each on
    /// its own when written.
    #[test]
    fn an_admission_flushes_the_medium_once_before_it_returns() {
        use darkbot_bsc_journal::durable::{Op, hooks};
        let (dir, s, now) = live_ready_limited("live-flush-once", (5, 5), |now| LiveGrant {
            max_attempts: 5,
            ..grant_of(now + 3_600_000, LiveStrategies::C3)
        });
        let q = qualified();
        let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
        let ids: Vec<String> = (0..4u8).map(|n| hex32(&[n; 32])).collect();
        let medium = || MEDIUM.with(|m| std::mem::take(&mut *m.borrow_mut()));
        let spend = spend_dir(&dir);
        let entry = |id: &str| spend.join(format!("{id}.dbrec"));
        let day = buy_day_dir(&dir, &BuyDay::at(now));
        let total = total_dir(&dir);
        let slot = |d: &Path, n: u64| d.join(format!("{n:010}.dbrec"));
        hooks::record();
        // The first creates the directories.
        s.admit_live(&armed, &nonce_buy(&ids[0], 7, 1_000), &q, now)
            .unwrap();
        medium();
        hooks::take();
        s.admit_live(&armed, &nonce_buy(&ids[1], 8, 1_000), &q, now)
            .unwrap();
        // Per slot kind its own slot (the one taken before costs no write)
        // and its directory; then the entry and the spend directory; then
        // the one flush, for both slot directories.
        assert_eq!(
            medium(),
            [
                Medium::Ordered(slot(&day, 1)),
                Medium::Linked(slot(&day, 1)),
                Medium::Handed(day.clone()),
                Medium::Ordered(slot(&total, 1)),
                Medium::Linked(slot(&total, 1)),
                Medium::Handed(total.clone()),
                Medium::Ordered(entry(&ids[1])),
                Medium::Linked(entry(&ids[1])),
                Medium::Handed(spend.clone()),
                Medium::Flush(spend.clone(), vec![day.clone(), total.clone()]),
            ]
        );
        // What the drive was asked, in that order.
        let handed = [Op::Barrier, Op::Fsync];
        assert_eq!(
            hooks::take(),
            [handed, handed, handed]
                .concat()
                .into_iter()
                .chain([Op::FullFlush])
                .collect::<Vec<_>>()
        );
        let exit = LiveIntent {
            kind: LiveIntentKind::Approve,
            input_wei: 0,
            ..nonce_buy(&ids[2], 9, 1_000)
        };
        s.admit_live(&armed, &exit, &q, now).unwrap();
        assert_eq!(
            medium(),
            [
                Medium::Ordered(entry(&ids[2])),
                Medium::Linked(entry(&ids[2])),
                Medium::Handed(spend.clone()),
                Medium::Flush(spend.clone(), vec![]),
            ]
        );
        assert_eq!(hooks::take(), [Op::Barrier, Op::Fsync, Op::FullFlush]);
        // A retry whose slots a failed admission took: their directories
        // synced again before the entry.
        s.consume_attempt_as(&ids[3], armed.grant_generation(), now, Durability::Handed)
            .unwrap();
        medium();
        hooks::take();
        s.admit_live(&armed, &nonce_buy(&ids[3], 10, 1_000), &q, now)
            .unwrap();
        assert_eq!(
            medium(),
            [
                Medium::Handed(day.clone()),
                Medium::Handed(total.clone()),
                Medium::Ordered(entry(&ids[3])),
                Medium::Linked(entry(&ids[3])),
                Medium::Handed(spend.clone()),
                Medium::Flush(spend.clone(), vec![day.clone(), total.clone()]),
            ]
        );
        assert_eq!(
            hooks::take(),
            [Op::Fsync, Op::Fsync, Op::Barrier, Op::Fsync, Op::FullFlush]
        );
        s.consume_buy_attempt("g07-attempt", now).unwrap();
        // Durable each on its own: no write handed over, no device flush.
        assert_eq!(
            medium(),
            [
                Medium::Full(slot(&day, 3)),
                Medium::Linked(slot(&day, 3)),
                Medium::Full(day.clone()),
                Medium::Full(slot(&total, 3)),
                Medium::Linked(slot(&total, 3)),
                Medium::Full(total.clone()),
            ]
        );
        assert_eq!(hooks::take(), []);
        assert_eq!(admitted_intents(&dir).unwrap().len(), 4);
        fs::remove_dir_all(dir).unwrap();
    }

    /// BSC3-023: a device flush that fails leaves no admission; its entry
    /// stays (it may be on the medium), so the same intent is never
    /// admitted again and nothing of it leaves.
    #[test]
    fn an_admission_whose_flush_fails_is_refused_for_good() {
        let (dir, s, now) = live_ready_granting("live-flush-fails", |now| {
            grant_of(now + 3_600_000, LiveStrategies::C3)
        });
        let q = qualified();
        let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
        let id = hex32(&[0x42; 32]);
        let buy = nonce_buy(&id, 7, 1_000);
        FLUSH_FAILS.with(|f| f.set(true));
        let refused = s.admit_live(&armed, &buy, &q, now).unwrap_err();
        assert!(
            refused.starts_with(&format!("{LIVE_SPEND_UNREADABLE}: not on the medium")),
            "{refused}"
        );
        assert!(!FLUSH_FAILS.with(std::cell::Cell::get));
        assert!(matches!(
            MEDIUM.with(|m| m.borrow().last().cloned()),
            Some(Medium::Flush(d, _)) if d == spend_dir(&dir)
        ));
        assert_eq!(
            s.admit_live(&armed, &buy, &q, now).unwrap_err(),
            LIVE_INTENT_SEEN
        );
        fs::remove_dir_all(dir).unwrap();
    }

    /// BSC3-023: one flush covers one device; a written directory on
    /// another is refused before the flush.
    #[test]
    fn a_flush_refuses_a_directory_on_another_device() {
        let dir = std::env::temp_dir().join(format!("bsc-flush-device-{}", std::process::id()));
        create_durable_dir(&dir).unwrap();
        flush_device(&dir, std::slice::from_ref(&dir)).unwrap();
        let other = Path::new("/dev");
        assert_ne!(
            fs::metadata(other).unwrap().dev(),
            fs::metadata(&dir).unwrap().dev()
        );
        let refused = flush_device(&dir, &[other.to_path_buf()]).unwrap_err();
        assert!(refused.to_string().contains("is not on the device of"));
        fs::remove_dir_all(dir).unwrap();
    }

    /// Budget fix (test N): a damaged entry, one naming its sender and chain
    /// but not its nonce, and a spend past 128 bits leave the spend
    /// unreadable; an admission then, or with its lock not to be opened,
    /// is refused with nothing consumed and nothing written.
    #[test]
    fn an_unreadable_spend_refuses_the_admission() {
        let (dir, s, now) = live_ready_granting("live-spend-bad", |now| {
            grant_of(now + 3_600_000, LiveStrategies::C3)
        });
        let q = qualified();
        let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
        let generation = armed.grant_generation();
        let id = hex32(&[0x99; 32]);
        let admit = || s.admit_live(&armed, &nonce_buy(&id, 7, 1_000), &q, now);
        let lock = dir.join("live-spend.lock");
        fs::create_dir_all(&lock).unwrap();
        assert!(admit().unwrap_err().starts_with(LIVE_SPEND_UNREADABLE));
        fs::remove_dir(&lock).unwrap();
        create_durable_dir(&spend_dir(&dir)).unwrap();
        let damaged = spend_dir(&dir).join(format!("{}.dbrec", hex32(&[1; 32])));
        fs::write(&damaged, b"not a record").unwrap();
        assert!(
            spent_by_grant(&dir, generation)
                .unwrap_err()
                .starts_with(LIVE_SPEND_UNREADABLE)
        );
        assert!(admit().unwrap_err().starts_with(LIVE_SPEND_UNREADABLE));
        fs::remove_file(&damaged).unwrap();
        spend_entry(&dir, &admitted_of(2, generation, None, Some(ORIGIN), 1));
        assert!(
            spent_by_grant(&dir, generation)
                .unwrap_err()
                .starts_with(LIVE_SPEND_UNREADABLE)
        );
        assert!(admit().unwrap_err().starts_with(LIVE_SPEND_UNREADABLE));
        fs::remove_file(spend_dir(&dir).join(format!("{}.dbrec", hex32(&[2; 32])))).unwrap();
        let half = u128::MAX / 2 + 1;
        spend_entry(
            &dir,
            &admitted_of(3, generation, Some(1), Some(ORIGIN), half),
        );
        spend_entry(
            &dir,
            &admitted_of(4, generation, Some(2), Some(ORIGIN), half),
        );
        assert!(
            spent_by_grant(&dir, generation)
                .unwrap_err()
                .contains("overflows")
        );
        assert!(admit().unwrap_err().starts_with(LIVE_SPEND_UNREADABLE));
        for n in [3u8, 4] {
            fs::remove_file(spend_dir(&dir).join(format!("{}.dbrec", hex32(&[n; 32])))).unwrap();
        }
        // Past 128 bits among entries without sender and chain.
        spend_entry(&dir, &admitted_of(5, generation, Some(1), None, half));
        spend_entry(&dir, &admitted_of(6, generation, Some(1), None, half));
        assert!(
            spent_by_grant(&dir, generation)
                .unwrap_err()
                .contains("overflows")
        );
        assert!(admit().unwrap_err().starts_with(LIVE_SPEND_UNREADABLE));
        assert_eq!(buy_usage(&dir, now).unwrap().used, 0);
        assert_eq!(total_usage(&dir).unwrap().used, 0);
        assert!(!spend_dir(&dir).join(format!("{id}.dbrec")).exists());
        fs::remove_dir_all(dir).unwrap();
    }

    /// Budget fix (test L, threads): two admissions, each through its own
    /// store, with room for one: while the first holds its read the
    /// second admits nothing (never waits); after the first's write it
    /// reads its entry and is refused; exactly one is admitted.
    #[test]
    fn concurrent_admissions_never_exceed_the_grant() {
        let (dir, _s, now) = live_ready_granting("live-spend-race", |now| LiveGrant {
            max_input_wei: 1_000,
            max_spend_wei: 10_000,
            ..grant_of(now + 3_600_000, LiveStrategies::C3)
        });
        let (read, first_read) = std::sync::mpsc::channel();
        let (went_on, go_on) = std::sync::mpsc::channel::<()>();
        let at = dir.clone();
        let first = std::thread::spawn(move || {
            let s = Store::open_in_session(&at, 8, Ok("boot-1".into())).unwrap();
            let q = qualified();
            let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
            // Holds its read until the second admission has run.
            AFTER_SPEND_READ.with(|h| {
                *h.borrow_mut() = Some(Box::new(move || {
                    let _ = read.send(());
                    let _ = go_on.recv();
                }))
            });
            let id = hex32(&[1; 32]);
            s.admit_live(&armed, &nonce_buy(&id, 7, 6_000), &q, now)
                .map(|a| a.spent_wei)
        });
        first_read.recv().unwrap();
        let s = Store::open_in_session(&dir, 8, Ok("boot-1".into())).unwrap();
        let q = qualified();
        let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
        let ids = [hex32(&[2; 32]), hex32(&[3; 32])];
        let during = s.admit_live(&armed, &nonce_buy(&ids[0], 8, 6_000), &q, now);
        went_on.send(()).unwrap();
        assert_eq!(first.join().unwrap(), Ok(6_000));
        // While the first held its read, the second admitted nothing.
        let busy = during.unwrap_err();
        assert!(
            busy.starts_with(LIVE_SPEND_UNREADABLE) && busy.ends_with("held by another admission"),
            "{busy}"
        );
        // After it, the second reads the first's entry.
        let after = s.admit_live(&armed, &nonce_buy(&ids[1], 8, 6_000), &q, now);
        assert_eq!(after.unwrap_err(), LIVE_GRANT_SPEND_EXHAUSTED);
        assert_eq!(
            spent_by_grant(&dir, armed.grant_generation()).unwrap(),
            6_000
        );
        assert_eq!(admitted_intents(&dir).unwrap().len(), 1);
        assert_eq!(buy_usage(&dir, now).unwrap().used, 1);
        fs::remove_dir_all(dir).unwrap();
    }

    const SPEND_CHILD_DIR: &str = "BSC_SPEND_CHILD_DIR";

    /// The other process of
    /// [`admissions_of_two_processes_never_exceed_the_grant`]: admits one
    /// BUY, holding its read for a second.
    #[test]
    #[ignore = "run by its parent test"]
    fn spend_admission_child() {
        let Ok(dir) = std::env::var(SPEND_CHILD_DIR) else {
            return;
        };
        let dir = PathBuf::from(dir);
        let s = Store::open_in_session(&dir, 8, Ok("boot-1".into())).unwrap();
        let q = qualified();
        let now = 1_793_059_200_000;
        let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
        let (marker, parent) = (dir.join("child-read"), dir.join("parent-done"));
        // Holds its read until the parent's admission has run.
        AFTER_SPEND_READ.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || {
                fs::write(&marker, b"read").unwrap();
                let started = std::time::Instant::now();
                while !parent.exists() && started.elapsed() < std::time::Duration::from_secs(60) {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }))
        });
        let id = hex32(&[1; 32]);
        let spent = s
            .admit_live(&armed, &nonce_buy(&id, 7, 6_000), &q, now)
            .map(|a| a.spent_wei);
        let tmp = dir.join("child-result.tmp");
        fs::write(&tmp, format!("{spent:?}")).unwrap();
        fs::rename(&tmp, dir.join("child-result")).unwrap();
    }

    /// Budget fix (test L, processes): an admission of another process
    /// that has read the spend holds the owner's lock: this one admits
    /// nothing meanwhile; after the other's write it reads its entry and is
    /// refused; exactly one is admitted.
    #[test]
    fn admissions_of_two_processes_never_exceed_the_grant() {
        let (dir, s, now) = live_ready_granting("live-spend-procs", |now| LiveGrant {
            max_input_wei: 1_000,
            max_spend_wei: 10_000,
            ..grant_of(now + 3_600_000, LiveStrategies::C3)
        });
        assert_eq!(now, 1_793_059_200_000);
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "sources::tests::spend_admission_child",
            ])
            .env(SPEND_CHILD_DIR, &dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        while !dir.join("child-read").exists() {
            assert!(
                started.elapsed() < std::time::Duration::from_secs(60),
                "the child never read the spend"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let q = qualified();
        let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
        let ids = [hex32(&[2; 32]), hex32(&[3; 32])];
        let during = s.admit_live(&armed, &nonce_buy(&ids[0], 8, 6_000), &q, now);
        fs::write(dir.join("parent-done"), b"done").unwrap();
        assert!(child.wait().unwrap().success());
        let busy = during.unwrap_err();
        assert!(busy.ends_with("held by another admission"), "{busy}");
        let second = s.admit_live(&armed, &nonce_buy(&ids[1], 8, 6_000), &q, now);
        assert_eq!(
            fs::read_to_string(dir.join("child-result")).unwrap(),
            "Ok(6000)"
        );
        assert_eq!(second.unwrap_err(), LIVE_GRANT_SPEND_EXHAUSTED);
        assert_eq!(
            spent_by_grant(&dir, armed.grant_generation()).unwrap(),
            6_000
        );
        assert_eq!(admitted_intents(&dir).unwrap().len(), 1);
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

//! The operator's LIVE grant: its scope (venue, strategies, sources, adapters),
//! limits, risk decision and presence signature, and the qualification it needs.

use super::*;

/// R9: what a presence signature binds, before its grant's fields.
const PRESENCE_DOMAIN: &[u8] = b"darkbot.bsc.live-grant-presence.v1";

/// REV.1.0 R9 (BSC3-001): the 32 bytes the operator's presence key signs
/// for `grant` written as owner generation `generation` (as an EIP-191
/// `personal_sign` message): keccak of the domain, the grant's scope
/// SHA-256 (its whole scope, expiry and exit-only flag included) and the
/// generation (review M1: a signature is good for one grant write, never
/// again after a revocation or a restart).
pub fn presence_digest(grant: &LiveGrant, generation: u64) -> [u8; 32] {
    let mut message = PRESENCE_DOMAIN.to_vec();
    message.extend_from_slice(&grant.scope_sha256());
    message.extend_from_slice(&generation.to_be_bytes());
    alloy_primitives::keccak256(&message).0
}

/// R9: the address whose `personal_sign` of [`presence_digest`] is
/// `signature` (`r ‖ s ‖ v`, `v` 27/28 or 0/1), if it recovers (low `s`).
pub fn presence_signer(
    grant: &LiveGrant,
    generation: u64,
    signature: &[u8; 65],
) -> Option<[u8; 20]> {
    let mut message = b"\x19Ethereum Signed Message:\n32".to_vec();
    message.extend_from_slice(&presence_digest(grant, generation));
    let y_parity = match signature[64] {
        0 | 27 => false,
        1 | 28 => true,
        _ => return None,
    };
    let mut r = [0u8; 32];
    let mut s = [0u8; 32];
    r.copy_from_slice(&signature[..32]);
    s.copy_from_slice(&signature[32..64]);
    darkbot_bsc_trading::signer::recover_signer(
        &alloy_primitives::keccak256(&message),
        &darkbot_bsc_trading::transactions::legacy::RecoverableSignature { r, s, y_parity },
    )
    .ok()
    .map(|address| address.into_array())
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
    /// Every strategy choice of a grant, in the owner's order (Audit E 4.9:
    /// the projection's list; the LIVE tab offers exactly these).
    pub const ALL: [LiveStrategies; 3] = [Self::C3, Self::StrategyC, Self::Both];

    pub fn name(self) -> &'static str {
        match self {
            Self::C3 => "C3",
            Self::StrategyC => "STRATEGY_C",
            Self::Both => "C3+STRATEGY_C",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == text)
    }

    pub fn c3(self) -> bool {
        self != Self::StrategyC
    }

    pub fn strategy_c(self) -> bool {
        self != Self::C3
    }

    /// The components a grant of these paths arms and names: C3 when its
    /// BUYs are granted, and Strategy C always, which decides the exits of
    /// every lot the grant's BUYs open (01.11., R1: a C3-only grant had no
    /// exit decider); its own BUYs only with STRATEGY_C in the scope
    /// ([`Store::admit_live`]).
    pub fn components(self) -> Vec<ComponentId> {
        let mut out = Vec::with_capacity(2);
        if self.c3() {
            out.push(ComponentId::ExecC3);
        }
        out.push(ComponentId::StrategyC);
        out
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

/// The venues a LIVE grant can name: Flap alone (Four has no LIVE execution
/// path, `FOUR_EXECUTION_PATH_MISSING`). Audit E 4.9: the projection's list.
pub const LIVE_VENUES: [VenueId; 1] = [VenueId::Flap];

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
    /// The positions and losses the operator confirmed (31.10. order);
    /// none in a grant written before them: such a grant arms nothing.
    pub limits: Option<GrantLimits>,
}

/// The positions and losses of a LIVE grant (31.10. order): the most
/// positions open at once (a BUY pending, open or exiting counts), the
/// most realized net loss of the grant's positions and of the positions
/// that closed on one UTC day (closed: proceeds − input − gas; failed:
/// − gas; a gain offsets a loss), the least time between two BUYs. A BUY
/// beyond any of them is refused; exits always go on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantLimits {
    pub max_open_positions: u32,
    pub max_realized_loss_wei: u128,
    pub max_daily_realized_loss_wei: u128,
    pub min_buy_interval_ms: u64,
}

impl GrantLimits {
    /// Refused unless a BUY can come under them at all: one position at
    /// least, a loss bound each, the daily within the grant's.
    fn validate(&self) -> Result<(), String> {
        if self.max_open_positions == 0 {
            return Err("max_open_positions must be at least 1".into());
        }
        if self.max_realized_loss_wei == 0 || self.max_daily_realized_loss_wei == 0 {
            return Err("a granted realized loss bound must be positive".into());
        }
        if self.max_daily_realized_loss_wei > self.max_realized_loss_wei {
            return Err("max_daily_realized_loss_wei must be within max_realized_loss_wei".into());
        }
        Ok(())
    }
}

/// The limits of a stored grant: all four, or none (a grant written before
/// them); a part of them is refused.
pub(super) fn grant_limits_of(row: &Record) -> Result<Option<GrantLimits>, String> {
    use darkbot_bsc_record::schema::bsc_runtime::source_modes::live as k;
    let e = |x: darkbot_bsc_record::RecordError| x.0;
    let wei = |w: Option<[u8; 32]>| match w {
        None => Ok(None),
        Some(w) => wei_u128(w)
            .map(Some)
            .ok_or_else(|| "a granted loss exceeds 128 bits".to_string()),
    };
    let values = (
        row.opt_u64(k::MAX_OPEN_POSITIONS).map_err(e)?,
        wei(row.opt_u256(k::MAX_REALIZED_LOSS_WEI).map_err(e)?)?,
        wei(row.opt_u256(k::MAX_DAILY_REALIZED_LOSS_WEI).map_err(e)?)?,
        row.opt_u64(k::MIN_BUY_INTERVAL_MS).map_err(e)?,
    );
    match values {
        (None, None, None, None) => Ok(None),
        (Some(open), Some(loss), Some(daily), Some(interval)) => Ok(Some(GrantLimits {
            max_open_positions: u32::try_from(open)
                .map_err(|_| "max_open_positions out of range")?,
            max_realized_loss_wei: loss,
            max_daily_realized_loss_wei: daily,
            min_buy_interval_ms: interval,
        })),
        _ => Err("a LIVE grant names its limits whole or not at all".into()),
    }
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
    /// REV.1.0 R4 (BSC3-069): the operator confirmed these bounds as an
    /// outlier (above the runtime's soft bounds); part of the risk scope
    /// only when set.
    pub outlier_ack: bool,
}

/// The risk of a stored grant: all six bounds, or none (a grant written
/// before them); a part of them is refused.
pub(super) fn grant_risk_of(row: &Record) -> Result<Option<GrantRisk>, String> {
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
    let ack = row.opt_bool(k::RISK_OUTLIER_ACK).map_err(e)?;
    match values {
        // R4: an outlier's confirmation without the bounds it confirms is
        // no grant's.
        [None, None, None, None, None, None] if ack.is_none() => Ok(None),
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
            outlier_ack: ack == Some(true),
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
    /// [`Self::validate`] and at most `max` attempts (the configured
    /// `BSC_BUY_ATTEMPTS_TOTAL_MAX`): what a new grant or scope takes.
    pub fn validate_within(&self, max: u32) -> Result<(), String> {
        self.validate()?;
        if self.max_attempts > max {
            return Err(format!(
                "max_attempts {} above BSC_BUY_ATTEMPTS_TOTAL_MAX {max}",
                self.max_attempts
            ));
        }
        Ok(())
    }

    /// Refused unless the scope can be live at all: Flap (Four has no
    /// execution path), at least one source and adapter, each named once,
    /// positive limits, an input within the spend, at least one attempt (a
    /// grant on file stays readable when the maximum is lowered; the
    /// campaign's total bounds its attempts).
    pub fn validate(&self) -> Result<(), String> {
        if !LIVE_VENUES.contains(&self.venue) {
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
        if self.max_attempts == 0 {
            return Err("max_attempts must be at least 1".into());
        }
        if self.expires_unix_ms == 0 {
            return Err("expires_unix_ms must be set".into());
        }
        if let Some(l) = &self.limits {
            l.validate()?;
        }
        if let Some(r) = &self.risk {
            r.validate()?;
        }
        Ok(())
    }

    /// The canonical scope text the operator confirms, one `key=value` per
    /// line in this order (the frontend builds the same text).
    pub fn scope_text(&self) -> String {
        self.text("darkbot.bsc.live-grant.v1", true)
    }

    /// SHA-256 of [`LiveGrant::scope_text`].
    pub fn scope_sha256(&self) -> [u8; 32] {
        use sha2::Digest;
        sha2::Sha256::digest(self.scope_text().as_bytes()).into()
    }

    /// BSC3-049: the scope a risk decision is made for: the scope text
    /// without its validity and exit-only (a grant of the same scope again,
    /// or one for its exits alone, needs no new decision; any other limit,
    /// venue, strategy, source, builder or risk bound does), and the code
    /// of the venue it trades as this build pins it (another implementation
    /// pinned needs a new decision too).
    pub fn risk_scope_text(&self) -> String {
        let code = match self.venue {
            VenueId::Flap => {
                let pin = darkbot_bsc_trading::venues::flap::deployment::bsc_mainnet_v5_27_2();
                format!(
                    "{}:{}",
                    pin.portal_implementation,
                    pin.portal_implementation_runtime_sha256
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                )
            }
            // No execution path: nothing pinned.
            VenueId::FourClassic | VenueId::OpenFour => "none".into(),
        };
        self.text("darkbot.bsc.live-risk-scope.v1", false) + &format!("venue_code={code}\n")
    }

    /// SHA-256 of [`LiveGrant::risk_scope_text`].
    pub fn risk_scope_sha256(&self) -> [u8; 32] {
        use sha2::Digest;
        sha2::Sha256::digest(self.risk_scope_text().as_bytes()).into()
    }

    fn text(&self, header: &str, validity: bool) -> String {
        let join = |names: Vec<&str>| names.join(",");
        format!(
            "{header}\nvenue={}\nstrategies={}\nsources={}\nadapters={}\nmax_input_wei={}\nmax_spend_wei={}\nmax_attempts={}\n",
            self.venue.name(),
            self.strategies.name(),
            join(self.sources.iter().map(|s| s.name()).collect()),
            join(self.adapters.iter().map(|a| a.name()).collect()),
            self.max_input_wei,
            self.max_spend_wei,
            self.max_attempts,
        ) + &if validity {
            format!("expires_unix_ms={}\n", self.expires_unix_ms)
        } else {
            String::new()
        } + &self.risk.map_or(String::new(), |r| {
            format!(
                "max_buy_slippage_bps={}\nmax_exit_slippage_bps={}\nmax_gas_price_wei={}\nmax_buy_gas_limit={}\nmax_approve_gas_limit={}\nmax_sell_gas_limit={}\n",
                r.max_buy_slippage_bps,
                r.max_exit_slippage_bps,
                r.max_gas_price_wei,
                r.max_buy_gas_limit,
                r.max_approve_gas_limit,
                r.max_sell_gas_limit
            ) + if r.outlier_ack {
                "risk_outlier_ack=YES\n"
            } else {
                ""
            }
        }) + &self.limits.map_or(String::new(), |l| {
            format!(
                "max_open_positions={}\nmax_realized_loss_wei={}\nmax_daily_realized_loss_wei={}\nmin_buy_interval_ms={}\n",
                l.max_open_positions,
                l.max_realized_loss_wei,
                l.max_daily_realized_loss_wei,
                l.min_buy_interval_ms
            )
        }) + if validity && self.exit_only {
            "exit_only=true\n"
        } else {
            ""
        }
    }

    /// Whether `entity` is within the scope.
    pub(super) fn names(&self, entity: Entity) -> bool {
        match entity {
            Entity::Runtime | Entity::Component(ComponentId::StateC2) => true,
            Entity::Component(ComponentId::ExecC3) => self.strategies.c3(),
            // 01.11. (R1): Strategy C decides the exits of every lot the
            // grant's BUYs open; its own BUYs only with STRATEGY_C in the
            // scope ([`Store::admit_live`]).
            Entity::Component(ComponentId::StrategyC) => true,
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
    /// BSC3-049: the grant risk scope it was made for
    /// ([`LiveGrant::risk_scope_sha256`]); none before it named one.
    pub risk_scope_sha256: Option<[u8; 32]>,
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

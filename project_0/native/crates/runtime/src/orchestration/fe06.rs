//! P8-CTRL-02 (P8-CTRL.3/.4/.5): the FE-06 scope — the one versioned description of exactly what
//! a LIVE grant permits, read from the native `key = value` text (never JSON; every field required
//! once, every cost and limit stated: no unstated cost is zero), validated against the registry's
//! Safety ceilings and encoded canonically (bincode); its SHA-256 is the digest the operator
//! confirms. A scope grants nothing itself: the owner checks it against what it runs (cluster,
//! genesis, programs, deployment, code, binary, configuration, signers, epoch, time) before a
//! grant and at every use, and the grant needs the separate operator authorization.
//! Modes: ALL (new liquidations and their exits), EXIT_ONLY (only the exits of exactly the named
//! open positions — no new liquidation), SIM_ONLY (real signatures for simulations only — never
//! armed, never sent, never upgraded to LIVE).
use crate::analytics::journal::{self, kind};
use crate::analytics::payload::Payload;
use crate::execution::one_shot::approval_file::{self, Fields};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// The schema line of a scope text.
pub const SCOPE_SCHEMA: &str = "Project0/FE06Scope/1";
/// The store of previewed scopes and of grant events (`data/fe06`).
pub const STORE: &str = "data/fe06";
/// The cluster a scope may name (the qualified deployment's).
pub const CLUSTER: &str = "mainnet-beta";
/// The evidence a scope binds, by kind: a FREE_ONLY provider record (`provider_evidence`), a
/// run's master qualification and the FE-01 state-scope freeze.
pub const EVIDENCE_KINDS: [&str; 3] = ["provider", "qualification", "fe01"];
/// The submission adapters a scope may name.
pub const ADAPTERS: [&str; 3] = ["rpc", "jito_single", "jito_bundle"];

/// What a grant of the scope permits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// New liquidations within the limits and the exits of their positions.
    All,
    /// Only the exits of the named open positions; no new liquidation.
    ExitOnly,
    /// Real signatures for simulations only: never armed, never sent.
    SimOnly,
}
impl Mode {
    pub fn code(self) -> &'static str {
        match self {
            Mode::All => "ALL",
            Mode::ExitOnly => "EXIT_ONLY",
            Mode::SimOnly => "SIM_ONLY",
        }
    }
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "ALL" => Ok(Mode::All),
            "EXIT_ONLY" => Ok(Mode::ExitOnly),
            "SIM_ONLY" => Ok(Mode::SimOnly),
            _ => Err("FE06_SCOPE_FIELD_INVALID: mode (ALL|EXIT_ONLY|SIM_ONLY)".into()),
        }
    }
}

/// P8-CTRL.3: the FE-06 scope. Every field binds; nothing has a default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub scope_id: String,
    pub mode: Mode,
    // The chain and the code that run.
    pub cluster: String,
    pub genesis_hash: String,
    pub programs: BTreeSet<String>,
    pub program_data_hash: String,
    pub code_sha: String,
    pub binary_sha256: String,
    pub runtime_config_sha256: String,
    // Exactly the signers (`<pubkey>:<ROLE>+<ROLE>`) and the fee payer.
    pub signers: BTreeSet<String>,
    pub fee_payer: String,
    // What may be done.
    pub execution_classes: BTreeSet<String>,
    pub funding_classes: BTreeSet<String>,
    pub banks: BTreeSet<String>,
    pub mints: BTreeSet<String>,
    /// EXIT_ONLY: the open positions (`<attempt approval id>:<mint>`) whose exits it permits.
    pub exit_positions: BTreeSet<String>,
    pub submission_adapters: BTreeSet<String>,
    pub data_sources: BTreeSet<String>,
    /// `<kind>:<id>` (`EVIDENCE_KINDS`).
    pub evidence: BTreeSet<String>,
    // Money (USD-micro, lamports), each a ceiling.
    pub max_input_per_attempt_usd_micro: u64,
    pub max_exposure_usd_micro: u64,
    pub max_loss_usd_micro: u64,
    pub max_spend_lamports: u64,
    pub max_fee_lamports: u64,
    pub max_priority_fee_lamports: u64,
    pub max_tip_lamports: u64,
    pub max_rent_lamports: u64,
    pub max_protocol_fee_lamports: u64,
    pub max_exit_cost_lamports: u64,
    pub reserve_lamports: u64,
    pub max_slippage_bps: u64,
    pub min_out_bps: u64,
    pub route: String,
    // New attempts per UTC day, per campaign and per grant; exits per grant.
    pub campaign_id: String,
    pub max_attempts_per_utc_day: u64,
    pub max_attempts_campaign: u64,
    pub max_attempts_grant: u64,
    pub max_exits_grant: u64,
    // When, and under which owner epoch.
    pub not_before_unix_s: u64,
    pub expires_unix_s: u64,
    pub owner_epoch: u64,
}
impl Payload for Scope {
    const KIND: u16 = kind::FE06_SCOPE;
}

/// The registry's Safety ceilings a scope may not exceed (the operational config's limits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ceilings {
    pub max_exposure_per_attempt_usd_micro: u64,
    pub max_total_capital_usd_micro: u64,
    pub max_daily_loss_usd_micro: u64,
    pub max_tip_lamports: u64,
    pub max_slippage_bps: u64,
    pub max_lifetime_seconds: u64,
}
impl Ceilings {
    pub fn of(limits: &darkbot_solana::runtime_config::Limits) -> Self {
        Self {
            max_exposure_per_attempt_usd_micro: limits.max_exposure_per_attempt,
            max_total_capital_usd_micro: limits.max_total_capital,
            max_daily_loss_usd_micro: limits.max_daily_loss,
            max_tip_lamports: limits.max_tip,
            max_slippage_bps: limits.max_slippage_bps,
            max_lifetime_seconds: limits.fe06_grant_max_lifetime_seconds,
        }
    }
}

fn invalid(key: &str) -> String {
    format!("FE06_SCOPE_FIELD_INVALID: {key}")
}

fn pubkey(key: &str, v: &str) -> Result<Pubkey, String> {
    match Pubkey::from_str(v) {
        Ok(pk) if pk.to_string() == v => Ok(pk),
        _ => Err(invalid(key)),
    }
}

/// Parses and validates the native scope text (`parse`, then `validate`).
pub fn parse(bytes: &[u8], ceilings: &Ceilings) -> Result<Scope, String> {
    if approval_file::is_json(bytes) {
        return Err("FE06_SCOPE_JSON_REFUSED: a scope is the native key = value text".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "FE06_SCOPE_UNREADABLE: not text")?;
    let mut fields = Fields::of(text, "FE06_SCOPE")?;
    if fields.raw("schema")? != SCOPE_SCHEMA {
        return Err("FE06_SCOPE_SCHEMA_UNKNOWN".into());
    }
    let f = &mut fields;
    let s = Scope {
        scope_id: f.id("scope_id")?,
        mode: Mode::parse(&f.text("mode")?)?,
        cluster: f.text("cluster")?,
        genesis_hash: f.hash("genesis_hash")?,
        programs: f.list("programs")?,
        program_data_hash: f.hash("program_data_hash")?,
        code_sha: f.text("code_sha")?,
        binary_sha256: f.text("binary_sha256")?,
        runtime_config_sha256: f.text("runtime_config_sha256")?,
        signers: f.list("signers")?,
        fee_payer: f.text("fee_payer")?,
        execution_classes: f.list("execution_classes")?,
        funding_classes: f.list("funding_classes")?,
        banks: f.list("banks")?,
        mints: f.list("mints")?,
        exit_positions: f.list_or_none("exit_positions")?,
        submission_adapters: f.list("submission_adapters")?,
        data_sources: f.list("data_sources")?,
        evidence: f.list("evidence")?,
        max_input_per_attempt_usd_micro: f.number("max_input_per_attempt_usd_micro")?,
        max_exposure_usd_micro: f.number("max_exposure_usd_micro")?,
        max_loss_usd_micro: f.number("max_loss_usd_micro")?,
        max_spend_lamports: f.positive("max_spend_lamports")?,
        max_fee_lamports: f.positive("max_fee_lamports")?,
        max_priority_fee_lamports: f.number("max_priority_fee_lamports")?,
        max_tip_lamports: f.number("max_tip_lamports")?,
        max_rent_lamports: f.number("max_rent_lamports")?,
        max_protocol_fee_lamports: f.number("max_protocol_fee_lamports")?,
        // P8-CTRL.5: every scope bounds an exit (ALL/SIM_ONLY hold it as the attempt's exit
        // reserve, EXIT_ONLY spends it): zero is no safe reserve, refused at the preview.
        max_exit_cost_lamports: f.positive("max_exit_cost_lamports")?,
        reserve_lamports: f.number("reserve_lamports")?,
        max_slippage_bps: f.number("max_slippage_bps")?,
        min_out_bps: f.number("min_out_bps")?,
        route: f.text("route")?,
        campaign_id: f.id("campaign_id")?,
        max_attempts_per_utc_day: f.number("max_attempts_per_utc_day")?,
        max_attempts_campaign: f.number("max_attempts_campaign")?,
        max_attempts_grant: f.number("max_attempts_grant")?,
        max_exits_grant: f.number("max_exits_grant")?,
        not_before_unix_s: f.number("not_before_unix_s")?,
        expires_unix_s: f.number("expires_unix_s")?,
        owner_epoch: f.positive("owner_epoch")?,
    };
    fields.finish()?;
    validate(&s, ceilings)?;
    Ok(s)
}

/// The scope's own well-formedness: formats, units, the mode's limits and the registry ceilings.
pub fn validate(s: &Scope, c: &Ceilings) -> Result<(), String> {
    if s.cluster != CLUSTER {
        return Err(invalid("cluster"));
    }
    for p in &s.programs {
        pubkey("programs", p)?;
    }
    let hex64 = |key: &str, v: &str| {
        if v.len() == 64
            && v.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            Ok(())
        } else {
            Err(invalid(key))
        }
    };
    hex64("binary_sha256", &s.binary_sha256)?;
    hex64("runtime_config_sha256", &s.runtime_config_sha256)?;
    let mut fee_payer_role = false;
    for signer in &s.signers {
        let (key, roles) = signer.split_once(':').ok_or_else(|| invalid("signers"))?;
        pubkey("signers", key)?;
        if !canonical_roles(roles) {
            return Err(invalid("signers"));
        }
        fee_payer_role |= key == s.fee_payer
            && roles
                .split('+')
                .any(|r| r == super::capability_control::ROLE_FEE_PAYER);
    }
    pubkey("fee_payer", &s.fee_payer)?;
    if !fee_payer_role {
        return Err(
            "FE06_SCOPE_FIELD_INVALID: fee_payer (a signer with the FEE_PAYER role)".into(),
        );
    }
    for b in s.banks.iter() {
        pubkey("banks", b)?;
    }
    for m in s.mints.iter() {
        pubkey("mints", m)?;
    }
    for p in &s.exit_positions {
        let (approval, mint) = p.split_once(':').ok_or_else(|| invalid("exit_positions"))?;
        if !crate::execution::one_shot::archive::approval_id_valid(approval) {
            return Err(invalid("exit_positions"));
        }
        pubkey("exit_positions", mint)?;
    }
    if s.submission_adapters
        .iter()
        .any(|a| !ADAPTERS.contains(&a.as_str()))
    {
        return Err(invalid("submission_adapters"));
    }
    if !ROUTES.contains(&s.route.as_str()) {
        return Err(invalid("route"));
    }
    for d in &s.data_sources {
        super::source_control::parse_unit(d).map_err(|_| invalid("data_sources"))?;
    }
    let mut kinds = BTreeSet::new();
    for e in &s.evidence {
        let (kind, id) = e.split_once(':').ok_or_else(|| invalid("evidence"))?;
        if !EVIDENCE_KINDS.contains(&kind) || id.is_empty() || id.contains(char::is_whitespace) {
            return Err(invalid("evidence"));
        }
        kinds.insert(kind);
    }
    if kinds.len() != EVIDENCE_KINDS.len() {
        return Err(format!(
            "FE06_SCOPE_EVIDENCE_MISSING: every kind is bound ({})",
            EVIDENCE_KINDS.join(", ")
        ));
    }
    if s.max_slippage_bps > 10_000 || s.min_out_bps > 10_000 {
        return Err(invalid("max_slippage_bps/min_out_bps"));
    }
    // The registry's Safety ceilings bound every scope.
    let over = |key: &str, v: u64, ceiling: u64| {
        if v > ceiling {
            Err(format!(
                "FE06_SCOPE_ABOVE_CEILING: {key} {v} > {ceiling} (registry)"
            ))
        } else {
            Ok(())
        }
    };
    over(
        "max_input_per_attempt_usd_micro",
        s.max_input_per_attempt_usd_micro,
        c.max_exposure_per_attempt_usd_micro,
    )?;
    over(
        "max_exposure_usd_micro",
        s.max_exposure_usd_micro,
        c.max_total_capital_usd_micro,
    )?;
    over(
        "max_loss_usd_micro",
        s.max_loss_usd_micro,
        c.max_daily_loss_usd_micro,
    )?;
    over("max_tip_lamports", s.max_tip_lamports, c.max_tip_lamports)?;
    over("max_slippage_bps", s.max_slippage_bps, c.max_slippage_bps)?;
    if s.expires_unix_s <= s.not_before_unix_s {
        return Err(invalid("expires_unix_s"));
    }
    over(
        "lifetime_seconds",
        s.expires_unix_s - s.not_before_unix_s,
        c.max_lifetime_seconds,
    )?;
    // The mode's own limits: ALL takes new attempts within its input; EXIT_ONLY exits exactly
    // its named positions and takes no new attempt; SIM_ONLY neither.
    let attempts = [
        s.max_attempts_per_utc_day,
        s.max_attempts_campaign,
        s.max_attempts_grant,
    ];
    match s.mode {
        Mode::All => {
            if attempts.contains(&0) || s.max_input_per_attempt_usd_micro == 0 {
                return Err("FE06_SCOPE_MODE_LIMITS: ALL needs every attempt limit and the input per attempt".into());
            }
            if !s.exit_positions.is_empty() {
                return Err(
                    "FE06_SCOPE_MODE_LIMITS: ALL names no exit positions (its own attempts' exits)"
                        .into(),
                );
            }
        }
        Mode::ExitOnly => {
            if attempts.iter().any(|a| *a != 0)
                || s.exit_positions.is_empty()
                || s.max_exits_grant == 0
            {
                return Err("FE06_SCOPE_MODE_LIMITS: EXIT_ONLY takes no new attempt and names its positions and exits".into());
            }
        }
        Mode::SimOnly => {
            if attempts.iter().any(|a| *a != 0)
                || s.max_exits_grant != 0
                || !s.exit_positions.is_empty()
            {
                return Err("FE06_SCOPE_MODE_LIMITS: SIM_ONLY takes no attempt and no exit".into());
            }
        }
    }
    Ok(())
}

/// The scope's canonical bytes (the journal payload encoding) and their SHA-256 (64 hex digits):
/// the digest the operator confirms.
pub fn digest(s: &Scope) -> Result<String, String> {
    let bytes =
        crate::analytics::payload::encode(s).map_err(|e| format!("FE06_SCOPE_ENCODE: {e}"))?;
    Ok(journal::hex(&solana_sdk::hash::hash(&bytes).to_bytes()))
}

/// What the owner runs, against which a scope is checked before a grant and at every use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Running {
    pub genesis_hash: String,
    pub program: String,
    pub program_data_hash: String,
    /// `None`: not determinable (a dirty or unreadable source tree) — never a match.
    pub code_sha: Option<String>,
    pub binary_sha256: Option<String>,
    pub runtime_config_sha256: String,
    /// The configured signers as `<pubkey>:<ROLE>+<ROLE>`.
    pub signers: BTreeSet<String>,
    pub owner_epoch: u64,
}

/// P8-CTRL.3: the route classes in their order (`planner::RouteKind`); a scope's `route` is the
/// highest class it allows (PAID only after P7, as the planner requires).
pub const ROUTES: [&str; 4] = ["NO_SWAP", "LOCAL_PRECOMPUTED", "FREE_PUBLIC", "PAID"];

/// The class of a plan's route.
pub fn route_class(r: darkbot_project0::planner::RouteKind) -> &'static str {
    use darkbot_project0::planner::RouteKind;
    match r {
        RouteKind::NoSwap => ROUTES[0],
        RouteKind::LocalPrecomputedAllowed => ROUTES[1],
        RouteKind::FreePublicAllowed => ROUTES[2],
        RouteKind::PaidAllowed => ROUTES[3],
    }
}

/// P8-CTRL.3/.5: what the attempts and exits already booked against a grant have committed — the
/// attempts' capital (exposure) and worst-case loss, and the SOL every booked attempt and exit
/// may spend; UNKNOWN or sent alike (`one_shot::fe06_grant_totals`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GrantTotals {
    pub exposure_usd_micro: u64,
    pub loss_usd_micro: u64,
    pub spend_lamports: u64,
}

/// P8-CTRL.3/.5: what one new attempt does, as its grant's scope bounds it — the payload's facts
/// before its signature (`None`: unknown, never within a bound).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttemptUse {
    /// The plan's liquidation type (`RECEIVERSHIP`; `None`: a plan without its market identity).
    pub execution_class: Option<String>,
    pub funding_class: String,
    pub fee_payer: String,
    /// The plan's asset and liability bank (`None`: a plan without its market identity).
    pub banks: Option<[String; 2]>,
    pub mints: Vec<String>,
    pub input_usd_micro: u64,
    pub exposure_usd_micro: u64,
    pub max_loss_usd_micro: Option<u64>,
    /// The exact message's network fee (base and priority).
    pub fee_lamports: Option<u64>,
    /// The priority fee the payload itself requests (`tx::requested_priority_lamports`).
    pub priority_fee_lamports: Option<u64>,
    pub tip_lamports: u64,
    pub rent_lamports: Option<u64>,
    pub protocol_fee_lamports: u64,
    /// Every lamport the attempt debits (fee, tip, protocol fee, rent).
    pub spend_lamports: Option<u64>,
    /// The wallet's free SOL before the attempt: reconciled, less what other attempts hold and
    /// the policy reserve.
    pub sol_free_lamports: Option<u64>,
    /// What the grant has booked so far.
    pub booked: Option<GrantTotals>,
    pub slippage_bps: u64,
    pub route: &'static str,
    /// The canonical data source the state comes from (`None`: none canonical).
    pub data_source: Option<darkbot_solana::ingress::sources::SourceUnit>,
}

/// P8-CTRL.3/.5: what one exit does, as its grant's scope bounds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExitUse {
    /// `<attempt approval id>:<mint>` of the open position.
    pub position: String,
    pub mint: String,
    pub fee_payer: String,
    /// The exit's network fee — its whole SOL cost (a router swap of the position).
    pub fee_lamports: u64,
    pub priority_fee_lamports: Option<u64>,
    /// The wallet's free SOL before the exit (as for an attempt).
    pub sol_free_lamports: Option<u64>,
    /// What the grant has booked so far.
    pub booked: Option<GrantTotals>,
    pub slippage_bps: u64,
    pub route: &'static str,
    pub data_source: Option<darkbot_solana::ingress::sources::SourceUnit>,
    /// The position belongs to an attempt booked against this grant.
    pub booked_under_grant: bool,
}

/// What an egress does, for its scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Use {
    Attempt(AttemptUse),
    Exit(ExitUse),
}

fn outside(what: &str, value: impl std::fmt::Display, bound: impl std::fmt::Display) -> String {
    format!("FE06_SCOPE_USE: {what} {value} outside the grant's scope ({bound})")
}

fn unknown(what: &str) -> String {
    format!("FE06_SCOPE_USE_UNKNOWN: {what} unknown — never within a bound")
}

fn at_most(what: &str, value: u64, bound: u64) -> Result<(), String> {
    if value > bound {
        return Err(outside(what, value, format!("≤ {bound}")));
    }
    Ok(())
}

fn route_within(route: &str, s: &Scope) -> Result<(), String> {
    let rank = |r: &str| ROUTES.iter().position(|x| *x == r);
    match (rank(route), rank(&s.route)) {
        (Some(r), Some(max)) if r <= max => Ok(()),
        _ => Err(outside("route", route, format!("≤ {}", s.route))),
    }
}

fn slippage_within(slippage_bps: u64, s: &Scope) -> Result<(), String> {
    at_most("slippage_bps", slippage_bps, s.max_slippage_bps)?;
    // The least output a swap of this slippage accepts, in bps of its quote.
    let min_out = 10_000u64.saturating_sub(slippage_bps);
    if min_out < s.min_out_bps {
        return Err(outside(
            "min_out_bps",
            min_out,
            format!("≥ {}", s.min_out_bps),
        ));
    }
    Ok(())
}

fn source_within(
    source: &Option<darkbot_solana::ingress::sources::SourceUnit>,
    s: &Scope,
) -> Result<(), String> {
    let Some(unit) = source else {
        return Err(unknown("data_source"));
    };
    let bound = s
        .data_sources
        .iter()
        .any(|d| super::source_control::parse_unit(d).as_ref() == Ok(unit));
    if !bound {
        return Err(outside("data_source", format!("{unit:?}"), "data_sources"));
    }
    Ok(())
}

/// The grant's total of `what`: what it has booked plus this use, within `bound`.
fn total(what: &str, booked: u64, this: u64, bound: u64) -> Result<(), String> {
    let sum = booked.checked_add(this).ok_or_else(|| unknown(what))?;
    at_most(what, sum, bound)
}

/// P8-CTRL.5: the separate exit reserve of an attempt under `scope` — one exit's ceiling for
/// every mint the attempt can leave open (each of its plan's mints, WSOL too — a WSOL position is
/// unwrapped by its own exit — at least one);
/// `None`: a scope without a positive exit ceiling (no safe reserve: no new liquidation) or an
/// overflow. The one reserve the FE-06 check keeps free and the FE-04 reservation holds.
pub fn exit_reserve(scope: &Scope, mints: &[String]) -> Option<u64> {
    if scope.max_exit_cost_lamports == 0 {
        return None;
    }
    let open: BTreeSet<&str> = mints.iter().map(String::as_str).collect();
    scope
        .max_exit_cost_lamports
        .checked_mul(u64::try_from(open.len().max(1)).ok()?)
}

/// The wallet's free SOL after `spend`, against what must stay in it.
fn reserve_within(
    free: Option<u64>,
    spend: u64,
    keep: Option<u64>,
    s: &Scope,
) -> Result<(), String> {
    let free = free.ok_or_else(|| unknown("sol_free_lamports"))?;
    let keep = keep.ok_or_else(|| unknown("sol_reserve_lamports"))?;
    let left = free.checked_sub(spend);
    if left.is_none_or(|l| l < keep) {
        return Err(outside(
            "sol_after_spend_lamports",
            left.map_or_else(|| "below 0".to_string(), |l| l.to_string()),
            format!(
                "≥ {keep} (reserve {}, a later exit's cost {})",
                s.reserve_lamports,
                keep.saturating_sub(s.reserve_lamports)
            ),
        ));
    }
    Ok(())
}

/// P8-CTRL.3/.5: the use stays within its grant's scope — every class, identity, bank, mint,
/// cost, the grant's totals of exposure, loss and spend, the reserve, slippage, route and source
/// the scope bounds; an unknown fact is never within.
pub fn check_use(s: &Scope, u: &Use) -> Result<(), String> {
    let member = |what: &str, value: &str, set: &BTreeSet<String>| {
        if set.contains(value) {
            Ok(())
        } else {
            Err(outside(what, value, what))
        }
    };
    let known = |what: &str, v: Option<u64>| v.ok_or_else(|| unknown(what));
    match u {
        Use::Attempt(a) => {
            let class = a
                .execution_class
                .as_deref()
                .ok_or_else(|| unknown("execution_class"))?;
            member("execution_classes", class, &s.execution_classes)?;
            member("funding_classes", &a.funding_class, &s.funding_classes)?;
            if a.fee_payer != s.fee_payer {
                return Err(outside("fee_payer", &a.fee_payer, &s.fee_payer));
            }
            let Some(banks) = &a.banks else {
                return Err(unknown("banks"));
            };
            for b in banks {
                member("banks", b, &s.banks)?;
            }
            for m in &a.mints {
                member("mints", m, &s.mints)?;
            }
            // Each attempt's own bounds.
            at_most(
                "input_usd_micro",
                a.input_usd_micro,
                s.max_input_per_attempt_usd_micro,
            )?;
            let fee = known("fee_lamports", a.fee_lamports)?;
            at_most("fee_lamports", fee, s.max_fee_lamports)?;
            let priority = known("priority_fee_lamports", a.priority_fee_lamports)?;
            at_most(
                "priority_fee_lamports",
                priority,
                s.max_priority_fee_lamports,
            )?;
            at_most("tip_lamports", a.tip_lamports, s.max_tip_lamports)?;
            let rent = known("rent_lamports", a.rent_lamports)?;
            at_most("rent_lamports", rent, s.max_rent_lamports)?;
            at_most(
                "protocol_fee_lamports",
                a.protocol_fee_lamports,
                s.max_protocol_fee_lamports,
            )?;
            // The grant's totals: every attempt and exit booked against it, and this one.
            let loss = known("max_loss_usd_micro", a.max_loss_usd_micro)?;
            let spend = known("spend_lamports", a.spend_lamports)?;
            let booked = a.booked.ok_or_else(|| unknown("grant_totals"))?;
            total(
                "grant_exposure_usd_micro",
                booked.exposure_usd_micro,
                a.exposure_usd_micro,
                s.max_exposure_usd_micro,
            )?;
            total(
                "grant_loss_usd_micro",
                booked.loss_usd_micro,
                loss,
                s.max_loss_usd_micro,
            )?;
            total(
                "grant_spend_lamports",
                booked.spend_lamports,
                spend,
                s.max_spend_lamports,
            )?;
            // The reserve, and what the positions the attempt can leave cost to exit, stay in the
            // wallet after its whole debit (P8-CTRL.5: no new liquidation without them).
            let exit = exit_reserve(s, &a.mints).ok_or_else(|| {
                outside(
                    "max_exit_cost_lamports",
                    s.max_exit_cost_lamports,
                    "a positive exit ceiling (the attempt's exit reserve)",
                )
            })?;
            let keep = s.reserve_lamports.checked_add(exit);
            reserve_within(a.sol_free_lamports, spend, keep, s)?;
            slippage_within(a.slippage_bps, s)?;
            route_within(a.route, s)?;
            source_within(&a.data_source, s)
        }
        Use::Exit(e) => {
            // EXIT_ONLY: exactly the scope's open positions; ALL: those and the positions of
            // attempts booked against this grant.
            let named = s.exit_positions.contains(&e.position);
            if !(named || (s.mode == Mode::All && e.booked_under_grant)) {
                return Err(outside("exit_position", &e.position, "exit_positions"));
            }
            member("mints", &e.mint, &s.mints)?;
            if e.fee_payer != s.fee_payer {
                return Err(outside("fee_payer", &e.fee_payer, &s.fee_payer));
            }
            at_most("fee_lamports", e.fee_lamports, s.max_fee_lamports)?;
            let priority = known("priority_fee_lamports", e.priority_fee_lamports)?;
            at_most(
                "priority_fee_lamports",
                priority,
                s.max_priority_fee_lamports,
            )?;
            at_most(
                "exit_cost_lamports",
                e.fee_lamports,
                s.max_exit_cost_lamports,
            )?;
            let booked = e.booked.ok_or_else(|| unknown("grant_totals"))?;
            total(
                "grant_spend_lamports",
                booked.spend_lamports,
                e.fee_lamports,
                s.max_spend_lamports,
            )?;
            reserve_within(
                e.sol_free_lamports,
                e.fee_lamports,
                Some(s.reserve_lamports),
                s,
            )?;
            slippage_within(e.slippage_bps, s)?;
            route_within(e.route, s)?;
            source_within(&e.data_source, s)
        }
    }
}

/// P8-CTRL.3: a signer's roles — each known one once, in the canonical order (what the owner runs
/// names them so: `WalletSwitch::scope_entry`).
fn canonical_roles(roles: &str) -> bool {
    let known = [
        super::capability_control::ROLE_LIQUIDATOR,
        super::capability_control::ROLE_FEE_PAYER,
    ];
    let mut order = known.iter();
    !roles.is_empty() && roles.split('+').all(|r| order.any(|k| *k == r))
}

/// P8-CTRL.3: whether a scope follows the rules of its route class and signer roles that hold now
/// (one previewed under an earlier rule would be refused at its use).
fn current_rules(s: &Scope) -> bool {
    ROUTES.contains(&s.route.as_str())
        && s.signers.iter().all(|e| {
            e.split_once(':')
                .is_some_and(|(_, roles)| canonical_roles(roles))
        })
}

/// P8-CTRL.3: the scope binds exactly what runs now — the first difference refuses it.
pub fn check_running(s: &Scope, r: &Running, now_unix_s: u64) -> Result<(), String> {
    let differs = |what: &str| Err(format!("FE06_SCOPE_BINDING: {what} differs from what runs"));
    if s.genesis_hash != r.genesis_hash
        || s.genesis_hash != darkbot_project0::protocol::GENESIS_HASH_MAINNET
    {
        return differs("genesis");
    }
    if !s.programs.contains(&r.program) {
        return differs("program");
    }
    if s.program_data_hash != r.program_data_hash {
        return differs("program_data_hash");
    }
    if r.code_sha.as_deref() != Some(s.code_sha.as_str()) {
        return differs("code_sha");
    }
    if r.binary_sha256.as_deref() != Some(s.binary_sha256.as_str()) {
        return differs("binary_sha256");
    }
    if s.runtime_config_sha256 != r.runtime_config_sha256 {
        return differs("runtime_config_sha256");
    }
    if s.signers != r.signers {
        return differs("signers");
    }
    if s.owner_epoch != r.owner_epoch {
        return Err(format!(
            "FE06_SCOPE_EPOCH: the scope names owner epoch {}, this owner is epoch {}",
            s.owner_epoch, r.owner_epoch
        ));
    }
    if now_unix_s < s.not_before_unix_s {
        return Err("FE06_SCOPE_NOT_YET_VALID".into());
    }
    if now_unix_s >= s.expires_unix_s {
        return Err("FE06_SCOPE_EXPIRED".into());
    }
    Ok(())
}

/// The scope as the operator sees it before confirming its digest: every binding, every limit.
pub fn terminal(s: &Scope) -> Result<String, String> {
    let list = |v: &BTreeSet<String>| {
        if v.is_empty() {
            "NONE".to_string()
        } else {
            v.iter().cloned().collect::<Vec<_>>().join(",")
        }
    };
    Ok(format!(
        "PROJECT 0 FE-06 SCOPE digest={}\n  scope_id {} · mode {} · owner epoch {} · campaign {}\n  valid {} … {} (unix s)\n  cluster {} · genesis {}\n  programs {} · program_data {}\n  code {} · binary {} · config {}\n  signers {} · fee payer {}\n  classes {} · funding {}\n  banks {}\n  mints {}\n  exit positions {}\n  adapters {} · data sources {}\n  evidence {}\n  per attempt ≤ {} USD-micro · exposure ≤ {} USD-micro · loss ≤ {} USD-micro\n  spend ≤ {} lamports · fee ≤ {} · priority ≤ {} · tip ≤ {} · rent ≤ {} · protocol ≤ {} · exit ≤ {} · reserve {}\n  slippage ≤ {} bps · min out ≥ {} bps · route {}\n  attempts ≤ {}/UTC day · ≤ {}/campaign · ≤ {}/grant · exits ≤ {}/grant\n  A grant of this scope is a permission, never a command; it needs the operator's authorization.\n",
        digest(s)?,
        s.scope_id, s.mode.code(), s.owner_epoch, s.campaign_id,
        s.not_before_unix_s, s.expires_unix_s,
        s.cluster, s.genesis_hash,
        list(&s.programs), s.program_data_hash,
        s.code_sha, s.binary_sha256, s.runtime_config_sha256,
        list(&s.signers), s.fee_payer,
        list(&s.execution_classes), list(&s.funding_classes),
        list(&s.banks), list(&s.mints), list(&s.exit_positions),
        list(&s.submission_adapters), list(&s.data_sources), list(&s.evidence),
        s.max_input_per_attempt_usd_micro, s.max_exposure_usd_micro, s.max_loss_usd_micro,
        s.max_spend_lamports, s.max_fee_lamports, s.max_priority_fee_lamports, s.max_tip_lamports,
        s.max_rent_lamports, s.max_protocol_fee_lamports, s.max_exit_cost_lamports, s.reserve_lamports,
        s.max_slippage_bps, s.min_out_bps, s.route,
        s.max_attempts_per_utc_day, s.max_attempts_campaign, s.max_attempts_grant, s.max_exits_grant,
    ))
}

fn preview_path(project: &Path, digest: &str) -> PathBuf {
    project.join(STORE).join(format!("scope_{digest}.p0j"))
}

/// P8-CTRL.3: the previewed scope, stored under its digest (content-addressed: the same scope is
/// the same file; never an overwrite) — a grant names exactly this digest.
pub fn put_preview(project: &Path, s: &Scope) -> Result<String, String> {
    let d = digest(s)?;
    let dir = project.join(STORE);
    std::fs::create_dir_all(&dir).map_err(|e| format!("FE06_STORE_IO: {e}"))?;
    journal::write_typed_value(
        &preview_path(project, &d),
        journal::source::CAPABILITY_CONTROL,
        s,
    )
    .map_err(|e| format!("FE06_STORE_IO: {e}"))?;
    Ok(d)
}

/// The digests of the store's previews by their names alone (nothing decoded), in digest order.
pub fn preview_digests(project: &Path) -> Result<Vec<String>, String> {
    let dir = project.join(STORE);
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("FE06_STORE_IO: {e}")),
    };
    let mut out: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name();
            Some(
                name.to_str()?
                    .strip_prefix("scope_")?
                    .strip_suffix(".p0j")?
                    .to_string(),
            )
        })
        .collect();
    out.sort();
    Ok(out)
}

/// Every previewed scope of the store with its digest (each read like `preview`; an unreadable
/// entry is left out, never shown as a scope), in digest order.
pub fn previews(project: &Path) -> Result<Vec<(String, Scope)>, String> {
    Ok(preview_digests(project)?
        .into_iter()
        .filter_map(|d| preview(project, &d).ok().map(|s| (d, s)))
        .collect())
}

/// The previewed scope of `digest`; its bytes must still be the ones the digest names.
pub fn preview(project: &Path, digest: &str) -> Result<Scope, String> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err("FE06_DIGEST_INVALID: 64 lowercase hex digits".into());
    }
    let path = preview_path(project, digest);
    match std::fs::symlink_metadata(&path) {
        Ok(m) if m.file_type().is_file() => {}
        Ok(_) => return Err("FE06_STORE_UNSAFE: a scope is a regular file".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err("FE06_SCOPE_UNKNOWN: no previewed scope has this digest".into())
        }
        Err(e) => return Err(format!("FE06_STORE_IO: {e}")),
    }
    let s: Scope =
        journal::read_typed_value(&path).map_err(|e| format!("FE06_SCOPE_INVALID: {e}"))?;
    if self::digest(&s)? != digest {
        return Err("FE06_SCOPE_CHANGED: the stored scope is not the digest's".into());
    }
    // Review LOW: a scope previewed under an earlier rule is no preview any more.
    if !current_rules(&s) {
        return Err(
            "FE06_SCOPE_STALE: previewed under an earlier rule (route class or signer roles); preview it again"
                .into(),
        );
    }
    Ok(s)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const KEY: &str = "11111111111111111111111111111112";

    pub(crate) fn ceilings() -> Ceilings {
        Ceilings {
            max_exposure_per_attempt_usd_micro: 100_000_000,
            max_total_capital_usd_micro: 500_000_000,
            max_daily_loss_usd_micro: 50_000_000,
            max_tip_lamports: 1_000_000,
            max_slippage_bps: 100,
            max_lifetime_seconds: 86_400,
        }
    }

    /// What the default test scope of `signer` binds as the running identity.
    pub(crate) fn running(signer: &Pubkey) -> Running {
        Running {
            genesis_hash: darkbot_project0::protocol::GENESIS_HASH_MAINNET.into(),
            program: darkbot_project0::protocol::PROGRAM_ID_MAINNET.into(),
            program_data_hash: darkbot_project0::protocol::PROGRAM_DATA_REFERENCE_HASH.into(),
            code_sha: Some("c0de".into()),
            binary_sha256: Some("b".repeat(64)),
            runtime_config_sha256: "c".repeat(64),
            signers: [format!("{signer}:LIQUIDATOR+FEE_PAYER")].into(),
            owner_epoch: 3,
        }
    }

    /// An attempt within the default test scope of `signer` (its reserve and a later exit's
    /// cost left exactly).
    pub(crate) fn attempt_use(signer: &Pubkey) -> AttemptUse {
        AttemptUse {
            execution_class: Some("RECEIVERSHIP".into()),
            funding_class: "ZERO_PREFUND_SAME_TX".into(),
            fee_payer: signer.to_string(),
            banks: Some([KEY.into(), KEY.into()]),
            mints: vec![KEY.into()],
            input_usd_micro: 10_000_000,
            exposure_usd_micro: 10_000_000,
            max_loss_usd_micro: Some(1_000_000),
            fee_lamports: Some(100_000),
            priority_fee_lamports: Some(50_000),
            tip_lamports: 10_000,
            rent_lamports: Some(3_000_000),
            protocol_fee_lamports: 10_000,
            spend_lamports: Some(3_120_000),
            sol_free_lamports: Some(13_220_000),
            booked: Some(GrantTotals::default()),
            slippage_bps: 50,
            route: "FREE_PUBLIC",
            data_source: Some(darkbot_solana::ingress::sources::SourceUnit::WsRpcControl),
        }
    }

    /// An exit within the default test scope of `signer` (a position of the grant's attempt).
    pub(crate) fn exit_use(signer: &Pubkey) -> ExitUse {
        ExitUse {
            position: format!("attempt-1:{KEY}"),
            mint: KEY.into(),
            fee_payer: signer.to_string(),
            fee_lamports: 100_000,
            priority_fee_lamports: Some(0),
            sol_free_lamports: Some(10_100_000),
            booked: Some(GrantTotals::default()),
            slippage_bps: 50,
            route: "FREE_PUBLIC",
            data_source: Some(darkbot_solana::ingress::sources::SourceUnit::WsRpcControl),
            booked_under_grant: true,
        }
    }

    pub(crate) fn text(signer: &Pubkey, overrides: &[(&str, &str)]) -> String {
        let signer = signer.to_string();
        let fields = [
            ("schema", SCOPE_SCHEMA.to_string()),
            ("scope_id", "pilot-1".into()),
            ("mode", "ALL".into()),
            ("cluster", CLUSTER.into()),
            (
                "genesis_hash",
                darkbot_project0::protocol::GENESIS_HASH_MAINNET.into(),
            ),
            (
                "programs",
                darkbot_project0::protocol::PROGRAM_ID_MAINNET.into(),
            ),
            (
                "program_data_hash",
                darkbot_project0::protocol::PROGRAM_DATA_REFERENCE_HASH.into(),
            ),
            ("code_sha", "c0de".into()),
            ("binary_sha256", "b".repeat(64)),
            ("runtime_config_sha256", "c".repeat(64)),
            ("signers", format!("{signer}:LIQUIDATOR+FEE_PAYER")),
            ("fee_payer", signer.clone()),
            ("execution_classes", "RECEIVERSHIP".into()),
            ("funding_classes", "ZERO_PREFUND_SAME_TX".into()),
            ("banks", KEY.into()),
            ("mints", KEY.into()),
            ("exit_positions", "NONE".into()),
            ("submission_adapters", "rpc".into()),
            ("data_sources", "wss".into()),
            (
                "evidence",
                "provider:0123456789abcdef,qualification:run-1,fe01:frozen-1".into(),
            ),
            ("max_input_per_attempt_usd_micro", "10000000".into()),
            ("max_exposure_usd_micro", "10000000".into()),
            ("max_loss_usd_micro", "1000000".into()),
            ("max_spend_lamports", "50000000".into()),
            ("max_fee_lamports", "100000".into()),
            ("max_priority_fee_lamports", "50000".into()),
            ("max_tip_lamports", "10000".into()),
            ("max_rent_lamports", "3000000".into()),
            ("max_protocol_fee_lamports", "10000".into()),
            ("max_exit_cost_lamports", "100000".into()),
            ("reserve_lamports", "10000000".into()),
            ("max_slippage_bps", "50".into()),
            ("min_out_bps", "9950".into()),
            ("route", "FREE_PUBLIC".into()),
            ("campaign_id", "first-profit".into()),
            ("max_attempts_per_utc_day", "1".into()),
            ("max_attempts_campaign", "1".into()),
            ("max_attempts_grant", "1".into()),
            ("max_exits_grant", "1".into()),
            ("not_before_unix_s", "1000".into()),
            ("expires_unix_s", "4600".into()),
            ("owner_epoch", "3".into()),
        ];
        let mut out = String::new();
        for (k, v) in fields {
            match overrides.iter().find(|(o, _)| *o == k) {
                Some((_, "")) => {}
                Some((_, o)) => out.push_str(&format!("{k} = {o}\n")),
                None => out.push_str(&format!("{k} = {v}\n")),
            }
        }
        out
    }

    /// P8-CTRL.3: a scope is read with every field once, validated against the registry
    /// ceilings and its mode's limits; its digest is the SHA-256 of its canonical bytes and moves
    /// with any field.
    #[test]
    fn p8ctrl02_a_scope_is_complete_bounded_and_digested() {
        let signer = Pubkey::new_unique();
        let s = parse(text(&signer, &[]).as_bytes(), &ceilings()).unwrap();
        assert_eq!(
            (s.mode, s.owner_epoch, s.exit_positions.len()),
            (Mode::All, 3, 0)
        );
        let d = digest(&s).unwrap();
        assert_eq!(d.len(), 64);
        assert_eq!(digest(&s.clone()).unwrap(), d);
        let mut other = s.clone();
        other.max_tip_lamports += 1;
        assert_ne!(digest(&other).unwrap(), d);
        assert!(terminal(&s).unwrap().contains(&format!("digest={d}")));
        for (overrides, refusal) in [
            (
                vec![("max_fee_lamports", "")],
                "FE06_SCOPE_FIELD_MISSING: max_fee_lamports",
            ),
            (vec![("mode", "LIVE")], "FE06_SCOPE_FIELD_INVALID: mode"),
            (
                vec![("cluster", "devnet")],
                "FE06_SCOPE_FIELD_INVALID: cluster",
            ),
            (
                vec![("fee_payer", KEY)],
                "FE06_SCOPE_FIELD_INVALID: fee_payer",
            ),
            (
                vec![("signers", &format!("{signer}:BUYER"))],
                "FE06_SCOPE_FIELD_INVALID: signers",
            ),
            (
                vec![("evidence", "provider:0123456789abcdef")],
                "FE06_SCOPE_EVIDENCE_MISSING",
            ),
            (
                vec![("submission_adapters", "tpu")],
                "FE06_SCOPE_FIELD_INVALID: submission_adapters",
            ),
            (
                vec![("max_tip_lamports", "1000001")],
                "FE06_SCOPE_ABOVE_CEILING: max_tip_lamports",
            ),
            (
                vec![("max_input_per_attempt_usd_micro", "100000001")],
                "FE06_SCOPE_ABOVE_CEILING",
            ),
            (
                vec![("expires_unix_s", "1000")],
                "FE06_SCOPE_FIELD_INVALID: expires_unix_s",
            ),
            (
                vec![("expires_unix_s", "90000")],
                "FE06_SCOPE_ABOVE_CEILING: lifetime_seconds",
            ),
            (vec![("max_attempts_grant", "0")], "FE06_SCOPE_MODE_LIMITS"),
            (vec![("mode", "EXIT_ONLY")], "FE06_SCOPE_MODE_LIMITS"),
            (vec![("mode", "SIM_ONLY")], "FE06_SCOPE_MODE_LIMITS"),
            (
                vec![("owner_epoch", "0")],
                "FE06_SCOPE_FIELD_INVALID: owner_epoch",
            ),
        ] {
            let err = parse(text(&signer, &overrides).as_bytes(), &ceilings()).unwrap_err();
            assert!(err.starts_with(refusal), "{overrides:?}: {err}");
        }
        let extra = format!("{}unknown = 1\n", text(&signer, &[]));
        assert!(parse(extra.as_bytes(), &ceilings())
            .unwrap_err()
            .starts_with("FE06_SCOPE_FIELD_UNKNOWN"));
        assert!(parse(b"{\"mode\":\"ALL\"}", &ceilings())
            .unwrap_err()
            .starts_with("FE06_SCOPE_JSON_REFUSED"));
        // EXIT_ONLY: exactly its positions, no new attempt; SIM_ONLY: neither.
        let exit = parse(
            text(
                &signer,
                &[
                    ("mode", "EXIT_ONLY"),
                    ("exit_positions", &format!("attempt-1:{KEY}")),
                    ("max_attempts_per_utc_day", "0"),
                    ("max_attempts_campaign", "0"),
                    ("max_attempts_grant", "0"),
                ],
            )
            .as_bytes(),
            &ceilings(),
        )
        .unwrap();
        assert_eq!(exit.mode, Mode::ExitOnly);
        let sim = parse(
            text(
                &signer,
                &[
                    ("mode", "SIM_ONLY"),
                    ("max_attempts_per_utc_day", "0"),
                    ("max_attempts_campaign", "0"),
                    ("max_attempts_grant", "0"),
                    ("max_exits_grant", "0"),
                ],
            )
            .as_bytes(),
            &ceilings(),
        )
        .unwrap();
        assert_eq!(sim.mode, Mode::SimOnly);
    }

    /// P8-CTRL.3: the scope binds what runs — each binding alone refuses it, as do another epoch
    /// and a time outside its validity.
    #[test]
    fn p8ctrl02_a_scope_binds_what_runs() {
        let signer = Pubkey::new_unique();
        let s = parse(text(&signer, &[]).as_bytes(), &ceilings()).unwrap();
        let running = Running {
            genesis_hash: darkbot_project0::protocol::GENESIS_HASH_MAINNET.into(),
            program: darkbot_project0::protocol::PROGRAM_ID_MAINNET.into(),
            program_data_hash: darkbot_project0::protocol::PROGRAM_DATA_REFERENCE_HASH.into(),
            code_sha: Some("c0de".into()),
            binary_sha256: Some("b".repeat(64)),
            runtime_config_sha256: "c".repeat(64),
            signers: BTreeSet::from([format!("{signer}:LIQUIDATOR+FEE_PAYER")]),
            owner_epoch: 3,
        };
        assert_eq!(check_running(&s, &running, 1000), Ok(()));
        let cases: Vec<(Running, &str)> = vec![
            (
                Running {
                    program: KEY.into(),
                    ..running.clone()
                },
                "program",
            ),
            (
                Running {
                    program_data_hash: "x".into(),
                    ..running.clone()
                },
                "program_data_hash",
            ),
            (
                Running {
                    code_sha: None,
                    ..running.clone()
                },
                "code_sha",
            ),
            (
                Running {
                    binary_sha256: Some("a".repeat(64)),
                    ..running.clone()
                },
                "binary_sha256",
            ),
            (
                Running {
                    runtime_config_sha256: "d".repeat(64),
                    ..running.clone()
                },
                "runtime_config_sha256",
            ),
            (
                Running {
                    signers: BTreeSet::new(),
                    ..running.clone()
                },
                "signers",
            ),
            (
                Running {
                    genesis_hash: KEY.into(),
                    ..running.clone()
                },
                "genesis",
            ),
        ];
        for (r, what) in cases {
            assert_eq!(
                check_running(&s, &r, 1000),
                Err(format!("FE06_SCOPE_BINDING: {what} differs from what runs"))
            );
        }
        assert!(check_running(
            &s,
            &Running {
                owner_epoch: 4,
                ..running.clone()
            },
            1000
        )
        .unwrap_err()
        .starts_with("FE06_SCOPE_EPOCH"));
        assert_eq!(
            check_running(&s, &running, 999),
            Err("FE06_SCOPE_NOT_YET_VALID".into())
        );
        assert_eq!(
            check_running(&s, &running, 4600),
            Err("FE06_SCOPE_EXPIRED".into())
        );
    }

    /// P8-CTRL.3: a previewed scope is stored under its digest and read back only as exactly
    /// that scope; another digest, a changed file or a symlink is refused.
    #[test]
    fn p8ctrl02_a_preview_is_the_digests_scope() {
        let project = std::env::temp_dir().join(format!("p0-fe06-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        let s = parse(text(&Pubkey::new_unique(), &[]).as_bytes(), &ceilings()).unwrap();
        let d = put_preview(&project, &s).unwrap();
        assert_eq!(put_preview(&project, &s).unwrap(), d, "idempotent");
        assert_eq!(preview(&project, &d).unwrap(), s);
        assert!(preview(&project, &"0".repeat(64))
            .unwrap_err()
            .starts_with("FE06_SCOPE_UNKNOWN"));
        assert!(preview(&project, "xyz")
            .unwrap_err()
            .starts_with("FE06_DIGEST_INVALID"));
        // A file of another scope under this digest is refused.
        let mut other = s.clone();
        other.max_tip_lamports += 1;
        let path = preview_path(&project, &d);
        std::fs::remove_file(&path).unwrap();
        journal::create_typed_value(&path, journal::source::CAPABILITY_CONTROL, &other).unwrap();
        assert!(preview(&project, &d)
            .unwrap_err()
            .starts_with("FE06_SCOPE_CHANGED"));
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(project.join("elsewhere"), &path).unwrap();
        assert!(preview(&project, &d)
            .unwrap_err()
            .starts_with("FE06_STORE_UNSAFE"));
        let _ = std::fs::remove_dir_all(project);
    }

    /// P8-CTRL.3 (review LOW): a scope previewed under an earlier rule — a route that is no class,
    /// signer roles out of their order — is no preview any more (it would be refused at its use).
    #[test]
    fn p8ctrl06_stale_previews_are_no_previews() {
        let signer = Pubkey::new_unique();
        let d = std::env::temp_dir().join(format!("fe06_stale_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let good = parse(text(&signer, &[]).as_bytes(), &ceilings()).unwrap();
        let digest = put_preview(&d, &good).unwrap();
        assert_eq!(preview(&d, &digest), Ok(good.clone()));
        let earlier_route = Scope {
            route: "jupiter_v2".into(),
            ..good.clone()
        };
        let reordered = Scope {
            signers: [format!("{signer}:FEE_PAYER+LIQUIDATOR")].into(),
            ..good
        };
        for stale in [earlier_route, reordered] {
            let digest = put_preview(&d, &stale).unwrap();
            assert!(preview(&d, &digest)
                .unwrap_err()
                .starts_with("FE06_SCOPE_STALE"));
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// P8-CTRL.3/.5: an egress stays within its grant's scope — every fact the scope bounds; one
    /// outside or unknown refuses with its name (never a default, never a zero).
    #[test]
    fn p8ctrl05_the_exit_reserve_is_one_exit_ceiling_per_mint_it_can_leave_open() {
        use super::{check_use, exit_reserve, Use};
        use crate::execution::one_shot::exit::WSOL_MINT;
        let signer = Pubkey::new_unique();
        let scope_of =
            |o: &[(&str, &str)]| parse(text(&signer, o).as_bytes(), &ceilings()).unwrap();
        let s = scope_of(&[("max_exit_cost_lamports", "1000")]);
        let other = Pubkey::new_unique().to_string();
        assert_eq!(exit_reserve(&s, &[KEY.into()]), Some(1_000));
        assert_eq!(
            exit_reserve(
                &s,
                &[KEY.into(), other.clone(), WSOL_MINT.into(), KEY.into()]
            ),
            Some(3_000),
            "each distinct mint, WSOL too (a WSOL position is unwrapped by its own exit)"
        );
        assert_eq!(exit_reserve(&s, &[WSOL_MINT.into()]), Some(1_000));
        assert_eq!(exit_reserve(&s, &[]), Some(1_000), "at least one exit");
        // No positive exit ceiling: refused at the preview, and (a scope from elsewhere) no safe
        // reserve, no new liquidation.
        let refused = parse(
            text(&signer, &[("max_exit_cost_lamports", "0")]).as_bytes(),
            &ceilings(),
        )
        .unwrap_err();
        assert!(refused.contains("max_exit_cost_lamports"), "{refused}");
        let mut zero = s.clone();
        zero.max_exit_cost_lamports = 0;
        assert_eq!(exit_reserve(&zero, &[KEY.into()]), None);
        let refused = check_use(&zero, &Use::Attempt(attempt_use(&signer))).unwrap_err();
        assert!(refused.contains("max_exit_cost_lamports 0 "), "{refused}");
    }

    #[test]
    fn p8ctrl03_a_scope_bounds_every_fact_of_an_attempt_and_an_exit() {
        use super::{check_use, AttemptUse, ExitUse, GrantTotals, Use};
        use darkbot_solana::ingress::sources::SourceUnit;
        let signer = Pubkey::new_unique();
        let scope_of =
            |o: &[(&str, &str)]| parse(text(&signer, o).as_bytes(), &ceilings()).unwrap();
        let s = scope_of(&[]);
        let attempt = attempt_use(&signer);
        assert_eq!(check_use(&s, &Use::Attempt(attempt.clone())), Ok(()));
        let with = |f: &dyn Fn(&mut AttemptUse)| {
            let mut a = attempt.clone();
            f(&mut a);
            check_use(&s, &Use::Attempt(a))
        };
        assert_eq!(
            with(&|a| a.route = "NO_SWAP"),
            Ok(()),
            "a lower route class"
        );
        let other = Pubkey::new_unique().to_string();
        type Change<'a> = (&'a str, &'a dyn Fn(&mut AttemptUse));
        let outside: [Change; 20] = [
            ("execution_classes", &|a| {
                a.execution_class = Some("CLASSIC".into())
            }),
            ("funding_classes", &|a| a.funding_class = "INVENTORY".into()),
            ("fee_payer", &|a| a.fee_payer = other.clone()),
            ("banks", &|a| a.banks = Some([KEY.into(), other.clone()])),
            ("mints", &|a| a.mints.push(other.clone())),
            ("input_usd_micro", &|a| a.input_usd_micro += 1),
            ("fee_lamports", &|a| a.fee_lamports = Some(100_001)),
            ("priority_fee_lamports", &|a| {
                a.priority_fee_lamports = Some(50_001)
            }),
            ("tip_lamports", &|a| a.tip_lamports += 1),
            ("rent_lamports", &|a| a.rent_lamports = Some(3_000_001)),
            ("protocol_fee_lamports", &|a| a.protocol_fee_lamports += 1),
            // The grant's totals: this attempt and what it has booked (P8-CTRL.5 "gesamt").
            ("grant_exposure_usd_micro", &|a| a.exposure_usd_micro += 1),
            ("grant_loss_usd_micro", &|a| {
                a.max_loss_usd_micro = Some(1_000_001)
            }),
            ("grant_spend_lamports", &|a| {
                a.spend_lamports = Some(50_000_001)
            }),
            ("grant_exposure_usd_micro", &|a| {
                a.booked = Some(GrantTotals {
                    exposure_usd_micro: 1,
                    ..Default::default()
                })
            }),
            ("grant_spend_lamports", &|a| {
                a.booked = Some(GrantTotals {
                    spend_lamports: 46_880_001,
                    ..Default::default()
                })
            }),
            // The reserve and a later exit's cost stay after the whole debit.
            ("sol_after_spend_lamports", &|a| {
                a.sol_free_lamports = Some(13_219_999)
            }),
            ("slippage_bps", &|a| a.slippage_bps = 51),
            ("route", &|a| a.route = "PAID"),
            ("data_source", &|a| {
                a.data_source = Some(SourceUnit::BinaryGeyser {
                    provider: "other".into(),
                })
            }),
        ];
        for (what, f) in outside {
            let e = with(f).unwrap_err();
            assert!(
                e.starts_with(&format!("FE06_SCOPE_USE: {what} ")),
                "{what}: {e}"
            );
        }
        let unknown: [Change; 10] = [
            ("execution_class", &|a| a.execution_class = None),
            ("banks", &|a| a.banks = None),
            ("fee_lamports", &|a| a.fee_lamports = None),
            ("priority_fee_lamports", &|a| a.priority_fee_lamports = None),
            ("rent_lamports", &|a| a.rent_lamports = None),
            ("max_loss_usd_micro", &|a| a.max_loss_usd_micro = None),
            ("spend_lamports", &|a| a.spend_lamports = None),
            ("grant_totals", &|a| a.booked = None),
            ("sol_free_lamports", &|a| a.sol_free_lamports = None),
            ("data_source", &|a| a.data_source = None),
        ];
        for (what, f) in unknown {
            let e = with(f).unwrap_err();
            assert!(
                e.starts_with(&format!("FE06_SCOPE_USE_UNKNOWN: {what} ")),
                "{what}: {e}"
            );
        }
        // The least output a swap of this slippage accepts against the scope's minimum.
        let strict = scope_of(&[("min_out_bps", "9960")]);
        let e = check_use(&strict, &Use::Attempt(attempt.clone())).unwrap_err();
        assert!(e.starts_with("FE06_SCOPE_USE: min_out_bps 9950 "), "{e}");
        // Exits: ALL takes the positions of the grant's own attempts, EXIT_ONLY exactly the
        // named ones; both bound the exit's cost, mint, payer, slippage, route and source.
        let position = format!("attempt-1:{KEY}");
        let exit = exit_use(&signer);
        assert_eq!(check_use(&s, &Use::Exit(exit.clone())), Ok(()));
        let foreign = ExitUse {
            booked_under_grant: false,
            ..exit.clone()
        };
        let e = check_use(&s, &Use::Exit(foreign.clone())).unwrap_err();
        assert!(e.starts_with("FE06_SCOPE_USE: exit_position "), "{e}");
        let only = scope_of(&[
            ("mode", "EXIT_ONLY"),
            ("exit_positions", &position),
            ("max_attempts_per_utc_day", "0"),
            ("max_attempts_campaign", "0"),
            ("max_attempts_grant", "0"),
        ]);
        assert_eq!(check_use(&only, &Use::Exit(foreign)), Ok(()));
        let unnamed = ExitUse {
            position: format!("pilot-2:{KEY}"),
            ..exit.clone()
        };
        let e = check_use(&only, &Use::Exit(unnamed)).unwrap_err();
        assert!(e.starts_with("FE06_SCOPE_USE: exit_position "), "{e}");
        let poor = ExitUse {
            sol_free_lamports: Some(10_099_999),
            ..exit.clone()
        };
        let e = check_use(&s, &Use::Exit(poor)).unwrap_err();
        assert!(
            e.starts_with("FE06_SCOPE_USE: sol_after_spend_lamports "),
            "{e}"
        );
        let spent = ExitUse {
            booked: Some(GrantTotals {
                spend_lamports: 49_900_001,
                ..Default::default()
            }),
            ..exit.clone()
        };
        let e = check_use(&s, &Use::Exit(spent)).unwrap_err();
        assert!(
            e.starts_with("FE06_SCOPE_USE: grant_spend_lamports "),
            "{e}"
        );
        let cheap = scope_of(&[("max_exit_cost_lamports", "90000")]);
        let e = check_use(&cheap, &Use::Exit(exit.clone())).unwrap_err();
        assert!(e.starts_with("FE06_SCOPE_USE: exit_cost_lamports "), "{e}");
        let e = check_use(
            &s,
            &Use::Exit(ExitUse {
                priority_fee_lamports: None,
                ..exit
            }),
        )
        .unwrap_err();
        assert!(
            e.starts_with("FE06_SCOPE_USE_UNKNOWN: priority_fee_lamports "),
            "{e}"
        );
    }
}

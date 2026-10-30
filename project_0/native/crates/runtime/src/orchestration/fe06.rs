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
        max_exit_cost_lamports: f.number("max_exit_cost_lamports")?,
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
        let roles: Vec<&str> = roles.split('+').collect();
        let known = [
            super::capability_control::ROLE_LIQUIDATOR,
            super::capability_control::ROLE_FEE_PAYER,
        ];
        if roles.is_empty() || roles.iter().any(|r| !known.contains(r)) {
            return Err(invalid("signers"));
        }
        fee_payer_role |=
            key == s.fee_payer && roles.contains(&super::capability_control::ROLE_FEE_PAYER);
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
            ("route", "jupiter_v2".into()),
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
}

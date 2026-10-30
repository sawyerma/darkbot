//! DEC-10: the FE-04 approval file in the native `key = value` text of the operational config —
//! one strictly validated format for the attempt and the exit approval, never JSON. Every field is
//! required exactly once (unknown, duplicate or missing keys fail), integers are plain decimal
//! digits in the unit their key names, lists are comma-separated without empty or repeated entries,
//! and the result must pass the approval's own well-formedness. The approval binds code, binary,
//! program, wallet, budget, expiry and targets; it is spent once (`archive::single_use`) and the
//! `.p0j` wallet journal stays the only persistent authority. A historical JSON approval is read
//! only by the `historical` build and never authorizes. No signature is introduced (DEC-10).
use super::exit::ExitApproval;
use super::Approval;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The largest approval file read (a few hundred bytes are typical).
pub const APPROVAL_MAX_BYTES: u64 = 64 * 1024;
/// Schema line of an attempt approval.
pub const ATTEMPT_SCHEMA: &str = "Project0/FE04Approval/1";
/// Schema line of an exit approval.
pub const EXIT_SCHEMA: &str = "Project0/FE04ExitApproval/1";

/// One parsed approval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalFile {
    Attempt(Approval),
    Exit(ExitApproval),
}
impl ApprovalFile {
    pub fn kind(&self) -> &'static str {
        match self {
            ApprovalFile::Attempt(_) => "attempt",
            ApprovalFile::Exit(_) => "exit",
        }
    }
    pub fn approval_id(&self) -> &str {
        match self {
            ApprovalFile::Attempt(a) => &a.approval_id,
            ApprovalFile::Exit(e) => &e.approval_id,
        }
    }
    /// Well formed and not expired at `now_unix_s` (the approval's own check).
    pub fn check(&self, now_unix_s: u64) -> Result<(), String> {
        match self {
            ApprovalFile::Attempt(a) => a.check(now_unix_s),
            ApprovalFile::Exit(e) => e.check(now_unix_s),
        }
    }
}

/// The bytes of the approval file at `path`, read without following a symlink and without
/// blocking on a FIFO or device: the open handle must be a regular file within the bound.
pub fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "APPROVAL_UNREADABLE")?;
    let meta = file.metadata().map_err(|_| "APPROVAL_UNREADABLE")?;
    if !meta.is_file() {
        return Err("APPROVAL_UNREADABLE: not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(APPROVAL_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "APPROVAL_UNREADABLE")?;
    if bytes.len() as u64 > APPROVAL_MAX_BYTES {
        return Err("APPROVAL_UNREADABLE: larger than the approval bound".into());
    }
    Ok(bytes)
}

/// The native approval file at `path` (`read_bytes`, then `parse`).
pub fn read(path: &Path) -> Result<ApprovalFile, String> {
    parse(&read_bytes(path)?)
}

/// Whether the bytes are a JSON document (a historical approval).
pub fn is_json(bytes: &[u8]) -> bool {
    bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{')
}

/// The native approval text; a JSON approval is refused (it never authorizes).
pub fn parse(bytes: &[u8]) -> Result<ApprovalFile, String> {
    if is_json(bytes) {
        return Err("APPROVAL_JSON_REFUSED: an approval is the native key = value text; a JSON approval is historical and never authorizes".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "APPROVAL_UNREADABLE: not text")?;
    let mut f = Fields::of(text, "APPROVAL")?;
    let schema = f.raw("schema")?;
    let parsed = match schema.as_str() {
        ATTEMPT_SCHEMA => {
            let a = Approval {
                schema: 1,
                authorized: f.flag("authorized")?,
                approval_id: f.id("approval_id")?,
                code_sha: f.text("code_sha")?,
                binary_sha256: f.text("binary_sha256")?,
                wallet: f.text("wallet")?,
                program: f.text("program")?,
                program_data_hash: f.hash("program_data_hash")?,
                funding_class: f.text("funding_class")?,
                allowed_targets: f.list("allowed_targets")?,
                expires_unix_s: f.number("expires_unix_s")?,
                max_state_age_ms: f.number("max_state_age_ms")?,
                max_capital_usd_micro: f.positive("max_capital_usd_micro")?,
                max_total_sol_debit_lamports: f.positive("max_total_sol_debit_lamports")?,
                max_tip_lamports: f.number("max_tip_lamports")?,
                max_rent_lamports: f.number("max_rent_lamports")?,
                max_loss_usd_micro: f.number("max_loss_usd_micro")?,
                min_net_usd_micro: f.number("min_net_usd_micro")?,
                max_slippage_bps: f.number("max_slippage_bps")?,
            };
            if !super::well_formed(&a) {
                return Err("APPROVAL_INVALID: INVALID_OR_UNAUTHORIZED_APPROVAL".into());
            }
            ApprovalFile::Attempt(a)
        }
        EXIT_SCHEMA => {
            let e = ExitApproval {
                schema: 1,
                authorized: f.flag("authorized")?,
                approval_id: f.id("approval_id")?,
                attempt_approval_id: f.id("attempt_approval_id")?,
                code_sha: f.text("code_sha")?,
                binary_sha256: f.text("binary_sha256")?,
                wallet: f.text("wallet")?,
                mint: f.text("mint")?,
                route_program: f.text("route_program")?,
                expires_unix_s: f.number("expires_unix_s")?,
                max_state_age_ms: f.number("max_state_age_ms")?,
                min_out_lamports: f.number("min_out_lamports")?,
                max_slippage_bps: f.number("max_slippage_bps")?,
                max_fee_lamports: f.positive("max_fee_lamports")?,
            };
            if !super::exit::well_formed(&e) {
                return Err("APPROVAL_INVALID: INVALID_OR_UNAUTHORIZED_EXIT_APPROVAL".into());
            }
            ApprovalFile::Exit(e)
        }
        _ => return Err("APPROVAL_SCHEMA_UNKNOWN".into()),
    };
    f.finish()?;
    Ok(parsed)
}

/// The fields of one native `key = value` text (an approval, P8-CTRL.3 an FE-06 scope), each
/// taken exactly once; its refusals carry the text's own code prefix.
pub(crate) struct Fields {
    map: BTreeMap<String, String>,
    prefix: &'static str,
}
impl Fields {
    /// The fields of `text` (a malformed text: `<prefix>_INVALID`).
    pub(crate) fn of(text: &str, prefix: &'static str) -> Result<Self, String> {
        let map = darkbot_solana::runtime_config::key_values(text)
            .map_err(|e| format!("{prefix}_INVALID: {e}"))?
            .into_iter()
            .collect();
        Ok(Self { map, prefix })
    }
    fn invalid(&self, key: &str) -> String {
        format!("{}_FIELD_INVALID: {key}", self.prefix)
    }
    /// A field as written.
    pub(crate) fn raw(&mut self, key: &str) -> Result<String, String> {
        self.map
            .remove(key)
            .ok_or_else(|| format!("{}_FIELD_MISSING: {key}", self.prefix))
    }
    /// No field left unread (an unknown key is refused).
    pub(crate) fn finish(self) -> Result<(), String> {
        match self.map.keys().next() {
            Some(unknown) => Err(format!("{}_FIELD_UNKNOWN: {unknown}", self.prefix)),
            None => Ok(()),
        }
    }
    pub(crate) fn text(&mut self, key: &str) -> Result<String, String> {
        let v = self.raw(key)?;
        if v.is_empty() || v.chars().any(char::is_whitespace) {
            return Err(self.invalid(key));
        }
        Ok(v)
    }
    /// A plain decimal integer: digits only, no sign, no leading zero, within u64.
    pub(crate) fn number(&mut self, key: &str) -> Result<u64, String> {
        let v = self.text(key)?;
        if !v.bytes().all(|b| b.is_ascii_digit()) || (v.len() > 1 && v.starts_with('0')) {
            return Err(self.invalid(key));
        }
        v.parse().map_err(|_| self.invalid(key))
    }
    /// A capital, debit or fee cap: an approval granting nothing is not an approval.
    pub(crate) fn positive(&mut self, key: &str) -> Result<u64, String> {
        match self.number(key)? {
            0 => Err(self.invalid(key)),
            n => Ok(n),
        }
    }
    /// An approval id: the archive name of its attempt (`[A-Za-z0-9_-]{1,64}`).
    pub(crate) fn id(&mut self, key: &str) -> Result<String, String> {
        let v = self.text(key)?;
        if !super::archive::approval_id_valid(&v) {
            return Err(self.invalid(key));
        }
        Ok(v)
    }
    /// A ProgramData hash as the deployment pin observes it (base58 SHA-256).
    pub(crate) fn hash(&mut self, key: &str) -> Result<String, String> {
        let v = self.text(key)?;
        v.parse::<solana_sdk::hash::Hash>()
            .map_err(|_| self.invalid(key))?;
        Ok(v)
    }
    pub(crate) fn flag(&mut self, key: &str) -> Result<bool, String> {
        match self.text(key)?.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(self.invalid(key)),
        }
    }
    /// Comma-separated entries, none empty, none repeated.
    pub(crate) fn list(&mut self, key: &str) -> Result<BTreeSet<String>, String> {
        let v = self.text(key)?;
        let entries: Vec<&str> = v.split(',').collect();
        let set: BTreeSet<String> = entries.iter().map(|s| s.to_string()).collect();
        if entries.iter().any(|s| s.is_empty()) || set.len() != entries.len() {
            return Err(self.invalid(key));
        }
        Ok(set)
    }
    /// A list, or `NONE` for none.
    pub(crate) fn list_or_none(&mut self, key: &str) -> Result<BTreeSet<String>, String> {
        match self.map.get(key).map(String::as_str) {
            Some("NONE") => {
                self.map.remove(key);
                Ok(BTreeSet::new())
            }
            _ => self.list(key),
        }
    }
}

/// A historical JSON approval (before DEC-10), shown by `p0ctl approval show` of the `historical`
/// build only; it is never adopted and never authorizes.
#[cfg(feature = "historical")]
pub fn read_historical_json(bytes: &[u8]) -> Result<ApprovalFile, String> {
    let attempt = serde_json::from_slice::<Approval>(bytes);
    let exit = serde_json::from_slice::<ExitApproval>(bytes);
    match (attempt, exit) {
        (Ok(a), Err(_)) => Ok(ApprovalFile::Attempt(a)),
        (Err(_), Ok(e)) => Ok(ApprovalFile::Exit(e)),
        _ => Err("APPROVAL_UNREADABLE: neither an attempt nor an exit approval (schema 1)".into()),
    }
}

/// One binding of an approval: the value this process knows, a value it cannot determine (never
/// a match), or a binding this process does not check (the runtime does).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Bind {
    Known(String),
    #[default]
    Unknown,
    NotChecked,
}
impl Bind {
    pub fn of(value: Option<String>) -> Self {
        value.map_or(Bind::Unknown, Bind::Known)
    }
    /// `false` only for a mismatch or an unknown value.
    fn holds(&self, approved: &str) -> bool {
        match self {
            Bind::Known(v) => v == approved,
            Bind::Unknown => false,
            Bind::NotChecked => true,
        }
    }
}

/// The runtime's own ceilings an attempt approval must stay within (its freshness bound and the
/// Safety limits of the operational config): an approval granting more is not bound to it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ceilings {
    pub max_state_age_ms: u64,
    pub max_slippage_bps: u64,
    pub max_tip_lamports: u64,
    pub max_capital_usd_micro: u64,
    pub max_loss_usd_micro: u64,
}

/// What this process binds an approval to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApprovalBinding {
    pub wallet: Bind,
    pub program: Bind,
    /// The observed ProgramData hash of the deployment.
    pub program_data_hash: Bind,
    pub code_sha: Bind,
    pub binary_sha256: Bind,
    /// `None`: not checked by this process.
    pub ceilings: Option<Ceilings>,
    /// The longest an approval may still run (`fe04_approval_max_lifetime_seconds`).
    pub max_lifetime_seconds: u64,
}

/// The single-use state of an approval against the FE-04 journals of its wallet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Usage {
    /// Not checked (no journal configured or given): never VALID.
    Unchecked(String),
    /// Unspent and usable now.
    Free,
    /// CONSUMED, BLOCKED or INVALID with its reason.
    Refused(&'static str, String),
}

/// Single use from the wallet's FE-04 journals (archived and active attempts with their exits):
/// an attempt approval is spent once any attempt used its id and blocked while another attempt is
/// still active; an exit approval is spent once any exit used its id, needs the open position of
/// its own attempt and is blocked while another exit of it is unsettled.
pub fn usage(file: &ApprovalFile, ledger: &crate::execution::accounting::Ledger) -> Usage {
    match file {
        ApprovalFile::Attempt(a) => {
            if ledger
                .attempts
                .iter()
                .any(|r| r.approval_id == a.approval_id)
            {
                return Usage::Refused("CONSUMED", "APPROVAL_ALREADY_USED".into());
            }
            match ledger.attempts.iter().find(|r| !r.archived) {
                Some(active) => Usage::Refused(
                    "BLOCKED",
                    format!(
                        "ACTIVE_ATTEMPT:{}:{}",
                        active.approval_id,
                        active.state.name()
                    ),
                ),
                None => Usage::Free,
            }
        }
        ApprovalFile::Exit(e) => {
            if ledger
                .attempts
                .iter()
                .any(|r| r.exits.iter().any(|x| x.approval_id == e.approval_id))
            {
                return Usage::Refused("CONSUMED", "EXIT_APPROVAL_ALREADY_USED".into());
            }
            match ledger
                .attempts
                .iter()
                .find(|r| r.approval_id == e.attempt_approval_id)
            {
                None => Usage::Refused("INVALID", "EXIT_ATTEMPT_UNKNOWN".into()),
                Some(r) if r.archived => Usage::Refused("INVALID", "EXIT_ATTEMPT_ARCHIVED".into()),
                Some(r) if !r.open_positions.contains_key(&e.mint) => {
                    Usage::Refused("INVALID", "EXIT_NO_OPEN_POSITION".into())
                }
                // Single flight: another exit of the attempt is still unsettled.
                Some(r) => match r.exits.iter().find(|x| x.settlement.is_none()) {
                    Some(open) => Usage::Refused(
                        "BLOCKED",
                        format!(
                            "EXIT_ALREADY_OPEN:{}:{}",
                            open.approval_id,
                            open.state.name()
                        ),
                    ),
                    None => Usage::Free,
                },
            }
        }
    }
}

/// N-5: the secret-free approval status (Liquidator tab, `p0ctl approval show`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApprovalStatus {
    /// NONE | VALID | EXPIRED | CONSUMED | BLOCKED | INVALID | BINDING_MISMATCH.
    pub state: &'static str,
    /// The refusal code or the bindings that do not hold; empty when VALID or NONE.
    pub reason: String,
    /// attempt | exit; empty without an approval.
    pub kind: &'static str,
    pub approval_id: String,
    pub funding_class: String,
    pub expires_unix_s: u64,
    pub max_capital_usd_micro: u64,
    pub max_loss_usd_micro: u64,
    pub min_net_usd_micro: u64,
    pub max_slippage_bps: u64,
    pub allowed_targets: u64,
}

/// The one evaluation of an approval for CLI and runtime (`None`: none configured) at
/// `now_unix_s`: VALID only when it is well formed, unexpired, within its lifetime bound, checked
/// unspent and every binding this process checks holds. VALID still authorizes no LIVE action
/// (FE-06 is a separate approval).
pub fn status(
    read: Option<&Result<ApprovalFile, String>>,
    binding: &ApprovalBinding,
    now_unix_s: u64,
    usage: &Usage,
) -> ApprovalStatus {
    let file = match read {
        None => {
            return ApprovalStatus {
                state: "NONE",
                ..Default::default()
            }
        }
        Some(Err(code)) => {
            return ApprovalStatus {
                state: "INVALID",
                reason: code.clone(),
                ..Default::default()
            }
        }
        Some(Ok(file)) => file,
    };
    let mut s = ApprovalStatus {
        kind: file.kind(),
        approval_id: file.approval_id().to_string(),
        ..Default::default()
    };
    let mut broken = Vec::new();
    let (wallet, code_sha, binary_sha256) = match file {
        ApprovalFile::Attempt(a) => {
            s.funding_class = a.funding_class.clone();
            s.expires_unix_s = a.expires_unix_s;
            s.max_capital_usd_micro = a.max_capital_usd_micro;
            s.max_loss_usd_micro = a.max_loss_usd_micro;
            s.min_net_usd_micro = a.min_net_usd_micro;
            s.max_slippage_bps = a.max_slippage_bps;
            s.allowed_targets = a.allowed_targets.len() as u64;
            if !binding.program.holds(&a.program) {
                broken.push("program");
            }
            if !binding.program_data_hash.holds(&a.program_data_hash) {
                broken.push("program_data_hash");
            }
            if let Some(c) = &binding.ceilings {
                for (field, approved, ceiling) in [
                    ("max_state_age_ms", a.max_state_age_ms, c.max_state_age_ms),
                    ("max_slippage_bps", a.max_slippage_bps, c.max_slippage_bps),
                    ("max_tip_lamports", a.max_tip_lamports, c.max_tip_lamports),
                    (
                        "max_capital_usd_micro",
                        a.max_capital_usd_micro,
                        c.max_capital_usd_micro,
                    ),
                    (
                        "max_loss_usd_micro",
                        a.max_loss_usd_micro,
                        c.max_loss_usd_micro,
                    ),
                ] {
                    if approved > ceiling {
                        broken.push(field);
                    }
                }
            }
            (&a.wallet, &a.code_sha, &a.binary_sha256)
        }
        ApprovalFile::Exit(e) => {
            s.expires_unix_s = e.expires_unix_s;
            s.max_slippage_bps = e.max_slippage_bps;
            if let Some(c) = &binding.ceilings {
                if e.max_state_age_ms > c.max_state_age_ms {
                    broken.push("max_state_age_ms");
                }
                if e.max_slippage_bps > c.max_slippage_bps {
                    broken.push("max_slippage_bps");
                }
            }
            (&e.wallet, &e.code_sha, &e.binary_sha256)
        }
    };
    for (field, bind, approved) in [
        ("wallet", &binding.wallet, wallet),
        ("code_sha", &binding.code_sha, code_sha),
        ("binary_sha256", &binding.binary_sha256, binary_sha256),
    ] {
        if !bind.holds(approved) {
            broken.push(field);
        }
    }
    (s.state, s.reason) = if let Err(code) = file.check(now_unix_s) {
        if code.ends_with("APPROVAL_EXPIRED") {
            ("EXPIRED", code)
        } else {
            ("INVALID", code)
        }
    } else if s.expires_unix_s.saturating_sub(now_unix_s) > binding.max_lifetime_seconds {
        ("INVALID", "APPROVAL_LIFETIME_EXCEEDS_BOUND".into())
    } else {
        match usage {
            Usage::Unchecked(reason) => ("INVALID", reason.clone()),
            Usage::Refused(state, reason) => (*state, reason.clone()),
            Usage::Free if !broken.is_empty() => ("BINDING_MISMATCH", broken.join(",")),
            Usage::Free => ("VALID", String::new()),
        }
    };
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::accounting::{AttemptRecord, ExitRecord, Ledger, StepState};

    const HASH: &str = "11111111111111111111111111111111";
    fn key(n: u8) -> String {
        solana_sdk::pubkey::Pubkey::new_from_array([n; 32]).to_string()
    }
    fn attempt_text(expires: u64) -> String {
        format!(
            "# FE-04 attempt approval (DEC-10)\nschema = {ATTEMPT_SCHEMA}\nauthorized = true\napproval-id = pilot-1\ncode_sha = {}\nbinary_sha256 = {}\nwallet = {}\nprogram = {}\nprogram_data_hash = {HASH}\nfunding_class = NO_SWAP_INVENTORY\nallowed_targets = {},{}\nexpires_unix_s = {expires}\nmax_state_age_ms = 500\nmax_capital_usd_micro = 10000000\nmax_total_sol_debit_lamports = 50000\nmax_tip_lamports = 0\nmax_rent_lamports = 3000000\nmax_loss_usd_micro = 1000000\nmin_net_usd_micro = 100\nmax_slippage_bps = 50\n",
            "a".repeat(40),
            "b".repeat(64),
            key(1),
            key(2),
            key(3),
            key(4)
        )
    }
    fn exit_text() -> String {
        format!(
            "schema = {EXIT_SCHEMA}\nauthorized = true\napproval_id = exit-1\nattempt_approval_id = pilot-1\ncode_sha = {}\nbinary_sha256 = {}\nwallet = {}\nmint = {}\nroute_program = {}\nexpires_unix_s = 2000\nmax_state_age_ms = 500\nmin_out_lamports = 9000000\nmax_slippage_bps = 100\nmax_fee_lamports = 10000\n",
            "a".repeat(40),
            "b".repeat(64),
            key(1),
            key(4),
            key(5)
        )
    }
    /// The runtime's binding of `attempt_text` (all known, ceilings above the approval's).
    fn binding() -> ApprovalBinding {
        ApprovalBinding {
            wallet: Bind::Known(key(1)),
            program: Bind::Known(key(2)),
            program_data_hash: Bind::Known(HASH.into()),
            code_sha: Bind::Known("a".repeat(40)),
            binary_sha256: Bind::Known("b".repeat(64)),
            ceilings: Some(Ceilings {
                max_state_age_ms: 2_000,
                max_slippage_bps: 100,
                max_tip_lamports: 0,
                max_capital_usd_micro: 10_000_000,
                max_loss_usd_micro: 1_000_000,
            }),
            max_lifetime_seconds: 86_400,
        }
    }
    fn record(
        id: &str,
        archived: bool,
        exits: &[&str],
        open_mint: Option<String>,
    ) -> AttemptRecord {
        AttemptRecord {
            approval_id: id.into(),
            target: key(3),
            funding_class: "NO_SWAP_INVENTORY".into(),
            archived,
            state: StepState::Settled,
            settlement: None,
            exits: exits
                .iter()
                .map(|x| ExitRecord {
                    approval_id: x.to_string(),
                    mint: key(4),
                    state: StepState::Settled,
                    settlement: None,
                })
                .collect(),
            open_positions: open_mint
                .into_iter()
                .map(|m| {
                    (
                        m,
                        super::super::exit::Position {
                            units: 1,
                            mark_usd_micro: None,
                        },
                    )
                })
                .collect(),
            hold: None,
            fe06: None,
        }
    }

    /// DEC-10: the native text parses into the typed approval; JSON, bad ids, a zero cap and a
    /// malformed ProgramData hash never do; no file text reaches an error code.
    #[test]
    fn native_approval_parses_strictly_and_json_is_refused() {
        let ApprovalFile::Attempt(a) = parse(attempt_text(2000).as_bytes()).unwrap() else {
            panic!("attempt");
        };
        assert_eq!((a.schema, a.approval_id.as_str()), (1, "pilot-1"));
        assert_eq!((a.allowed_targets.len(), a.max_tip_lamports), (2, 0));
        assert_eq!(parse(exit_text().as_bytes()).unwrap().kind(), "exit");
        assert!(parse(b"  {\"schema\":1}")
            .unwrap_err()
            .starts_with("APPROVAL_JSON_REFUSED"));
        assert_eq!(
            parse(b"authorized = true\n").unwrap_err(),
            "APPROVAL_FIELD_MISSING: schema"
        );
        assert!(parse(&[0xff, 0xfe])
            .unwrap_err()
            .starts_with("APPROVAL_UNREADABLE"));
        for (from, to, code) in [
            (
                "approval-id = pilot-1",
                "approval-id = ../x",
                "APPROVAL_FIELD_INVALID: approval_id",
            ),
            (
                "approval-id = pilot-1",
                "approval-id = pilot.1",
                "APPROVAL_FIELD_INVALID: approval_id",
            ),
            (
                "max_capital_usd_micro = 10000000",
                "max_capital_usd_micro = 0",
                "APPROVAL_FIELD_INVALID: max_capital_usd_micro",
            ),
            (
                "max_total_sol_debit_lamports = 50000",
                "max_total_sol_debit_lamports = 0",
                "APPROVAL_FIELD_INVALID: max_total_sol_debit_lamports",
            ),
            (
                &format!("program_data_hash = {HASH}"),
                "program_data_hash = deployed",
                "APPROVAL_FIELD_INVALID: program_data_hash",
            ),
            (
                ATTEMPT_SCHEMA,
                "Project0/FE04Approval/9 secret-text",
                "APPROVAL_SCHEMA_UNKNOWN",
            ),
        ] {
            let text = attempt_text(2000).replace(from, to);
            assert_eq!(parse(text.as_bytes()).unwrap_err(), code, "{to}");
        }
        let zero_fee = exit_text().replace("max_fee_lamports = 10000", "max_fee_lamports = 0");
        assert!(parse(zero_fee.as_bytes()).is_err());
    }

    /// Single use from the ledger: an attempt approval is spent by any attempt with its id
    /// (archived or still active) and blocked by another active attempt; an exit approval is
    /// spent by any exit with its id and needs its attempt's open position.
    #[test]
    fn single_use_follows_the_ledger_for_attempt_and_exit() {
        let attempt = parse(attempt_text(2000).as_bytes()).unwrap();
        let exit = parse(exit_text().as_bytes()).unwrap();
        let ledger = |attempts: Vec<AttemptRecord>| Ledger { attempts };
        assert_eq!(usage(&attempt, &ledger(vec![])), Usage::Free);
        for archived in [true, false] {
            assert_eq!(
                usage(
                    &attempt,
                    &ledger(vec![record("pilot-1", archived, &[], None)])
                ),
                Usage::Refused("CONSUMED", "APPROVAL_ALREADY_USED".into())
            );
        }
        assert_eq!(
            usage(&attempt, &ledger(vec![record("other", false, &[], None)])),
            Usage::Refused("BLOCKED", "ACTIVE_ATTEMPT:other:SETTLED".into())
        );
        assert_eq!(
            usage(&attempt, &ledger(vec![record("other", true, &[], None)])),
            Usage::Free
        );
        let open = Some(key(4));
        assert_eq!(
            usage(
                &exit,
                &ledger(vec![record("pilot-1", false, &[], open.clone())])
            ),
            Usage::Free
        );
        let mut unsettled = record("pilot-1", false, &["exit-0"], open.clone());
        unsettled.exits[0].state = StepState::Unknown;
        assert_eq!(
            usage(&exit, &ledger(vec![unsettled])),
            Usage::Refused("BLOCKED", "EXIT_ALREADY_OPEN:exit-0:UNKNOWN".into())
        );
        for (records, code) in [
            (
                vec![record("pilot-1", false, &["exit-1"], open.clone())],
                "EXIT_APPROVAL_ALREADY_USED",
            ),
            (vec![], "EXIT_ATTEMPT_UNKNOWN"),
            (
                vec![record("pilot-1", true, &[], open.clone())],
                "EXIT_ATTEMPT_ARCHIVED",
            ),
            (
                vec![record("pilot-1", false, &[], None)],
                "EXIT_NO_OPEN_POSITION",
            ),
        ] {
            let Usage::Refused(_, reason) = usage(&exit, &ledger(records)) else {
                panic!("{code}");
            };
            assert_eq!(reason, code);
        }
    }

    /// The status: none, invalid, expired, beyond the lifetime bound, unchecked use, spent,
    /// blocked, foreign bindings or ceilings, valid; NotChecked bindings are skipped, Unknown
    /// ones never match. VALID still authorizes no LIVE action.
    #[test]
    fn status_states_follow_validity_lifetime_use_binding_and_ceilings() {
        let file = parse(attempt_text(2000).as_bytes());
        let b = binding();
        assert_eq!(status(None, &b, 1000, &Usage::Free).state, "NONE");
        let bad: Result<ApprovalFile, String> = Err("APPROVAL_FIELD_UNKNOWN: x".into());
        let s = status(Some(&bad), &b, 1000, &Usage::Free);
        assert_eq!(
            (s.state, s.reason.as_str()),
            ("INVALID", "APPROVAL_FIELD_UNKNOWN: x")
        );
        let s = status(Some(&file), &b, 1000, &Usage::Free);
        assert_eq!(
            (s.state, s.kind, s.approval_id.as_str()),
            ("VALID", "attempt", "pilot-1")
        );
        assert_eq!(
            (
                s.max_capital_usd_micro,
                s.allowed_targets,
                s.max_slippage_bps
            ),
            (10_000_000, 2, 50)
        );
        assert_eq!(status(Some(&file), &b, 2000, &Usage::Free).state, "EXPIRED");
        let far = parse(attempt_text(1000 + 86_401).as_bytes());
        let s = status(Some(&far), &b, 1000, &Usage::Free);
        assert_eq!(
            (s.state, s.reason.as_str()),
            ("INVALID", "APPROVAL_LIFETIME_EXCEEDS_BOUND")
        );
        let s = status(
            Some(&file),
            &b,
            1000,
            &Usage::Unchecked("FE04_JOURNAL_NOT_CONFIGURED".into()),
        );
        assert_eq!(
            (s.state, s.reason.as_str()),
            ("INVALID", "FE04_JOURNAL_NOT_CONFIGURED")
        );
        let spent = Usage::Refused("CONSUMED", "APPROVAL_ALREADY_USED".into());
        assert_eq!(status(Some(&file), &b, 1000, &spent).state, "CONSUMED");
        let foreign = ApprovalBinding {
            code_sha: Bind::Unknown,
            program: Bind::Known(key(9)),
            program_data_hash: Bind::Known("upgraded".into()),
            ceilings: Some(Ceilings {
                max_capital_usd_micro: 1,
                ..binding().ceilings.unwrap()
            }),
            ..binding()
        };
        let s = status(Some(&file), &foreign, 1000, &Usage::Free);
        assert_eq!(
            (s.state, s.reason.as_str()),
            (
                "BINDING_MISMATCH",
                "program,program_data_hash,max_capital_usd_micro,code_sha"
            )
        );
        let cli = ApprovalBinding {
            wallet: Bind::NotChecked,
            program: Bind::NotChecked,
            program_data_hash: Bind::NotChecked,
            ceilings: None,
            ..binding()
        };
        assert_eq!(status(Some(&file), &cli, 1000, &Usage::Free).state, "VALID");
    }
}

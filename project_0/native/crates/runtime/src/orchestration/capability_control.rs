//! DEC-13/P8-CTRL operator capability control (`p0ctl capability set …`): LIVE LIVE|OFF
//! (DEC-13), HOLD_B, JITO_BUNDLE and THRESHOLD_TRIGGER OFF|OBSERVE|ACTIVE (DEC-04/11/06), and the
//! P8-CTRL.1 economic switches — `WALLET:<pubkey>` per configured signer and ECON_SIGN, ECON_ARM,
//! ECON_SEND, ECON_ATTEMPT, ECON_LIVE, each DISABLED|ENABLED: a partial permission, never a
//! command, and no switch enables another. Typed, append-only request files beside the
//! source-control log, rung by the same doorbell and handed to the state owner by the same control
//! thread (`source_control::spawn`). The operator's value is the *requested* one; only the owner
//! decides the *effective* one and records it (`outcome_<seq>.p0j`, the durable receipt).
//! P8-CTRL.6: every request names a one-time request id and the owner epoch it was made against —
//! the same id with the same bytes is the same request (no second effect), with other bytes a
//! conflict; a request of another epoch is refused. Every owner start opens a fresh epoch
//! (`epoch_<n>.p0j`) with every switch DISABLED and LIVE requested OFF. LIVE is effective only
//! with every economic switch, every required wallet and a separate FE-06 authorization — this
//! build constructs none (`Fe06Authorization`), so no request ever makes a candidate reach a send.
use crate::analytics::journal;
use crate::analytics::payload::{
    CapabilityControlOutcome, CapabilityControlRequest, CapabilityOwnerEpoch,
};
use solana_sdk::pubkey::Pubkey;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// The controllable capability of FE-06 (DEC-13).
pub const LIVE: &str = "LIVE";
/// LIVE not requested — the value of every start.
pub const LIVE_OFF: &str = "OFF";
/// The values LIVE may be requested with.
pub const LIVE_VALUES: [&str; 2] = [LIVE_OFF, LIVE];
/// Why LIVE is not effective without FE-06 (B10).
pub const LIVE_BLOCK_REASON: &str = "FE-06/P8-LT: LIVE is not authorized (B10)";
/// P8-CTRL.1: the values of a wallet or economic switch; DISABLED at every start.
pub const DISABLED: &str = "DISABLED";
pub const ENABLED: &str = "ENABLED";
pub const SWITCH_VALUES: [&str; 2] = [DISABLED, ENABLED];
/// P8-CTRL.1: a real economic signature (every fee-payer, bundle, prerequisite, tip, exit signer).
pub const ECON_SIGN: &str = "ECON_SIGN";
/// P8-CTRL.1: claiming a currently valid FE-06 grant.
pub const ECON_ARM: &str = "ECON_ARM";
/// P8-CTRL.1: the economic handoff to any RPC/Jito/network adapter.
pub const ECON_SEND: &str = "ECON_SEND";
/// P8-CTRL.1: taking a new liquidation into the FE-04 admission.
pub const ECON_ATTEMPT: &str = "ECON_ATTEMPT";
/// P8-CTRL.1: the global economic main switch (also blocks new EXIT_ONLY egress when DISABLED).
pub const ECON_LIVE: &str = "ECON_LIVE";
/// The economic switches, in the order LIVE names a missing one.
pub const ECON: [&str; 5] = [ECON_LIVE, ECON_ARM, ECON_SIGN, ECON_SEND, ECON_ATTEMPT];
/// P8-CTRL.5: the switches an exit of an open position needs — every one but ECON_ATTEMPT,
/// which governs new liquidations only.
pub const ECON_EXIT: [&str; 4] = [ECON_LIVE, ECON_ARM, ECON_SIGN, ECON_SEND];
/// P8-CTRL.3: a grant of the previewed FE-06 scope its value names (its digest), and the
/// revocation of the grant in force of that digest.
pub const FE06_GRANT: &str = "FE06_GRANT";
pub const FE06_REVOKE: &str = "FE06_REVOKE";
/// Why no grant is taken while the operator authentication of a grant is not decided (P8-CTRL.3:
/// a confirmation alone is no cryptographic identity).
pub const FE06_AUTH_UNDECIDED: &str = "FE06_OPERATOR_AUTHORIZATION_UNDECIDED: a grant needs the operator's own authentication (P8-CTRL.3), not decided yet; no grant is taken";
/// P8-CTRL.1: the switch of one configured signer is `WALLET:<pubkey>`.
pub const WALLET_PREFIX: &str = "WALLET:";
/// P8-CTRL.1: the validated roles of a configured signer.
pub const ROLE_LIQUIDATOR: &str = "LIQUIDATOR";
pub const ROLE_FEE_PAYER: &str = "FEE_PAYER";

pub fn request_dir(project: &Path) -> PathBuf {
    project.join("data").join("capability-control")
}

fn request_path(dir: &Path, seq: u64) -> PathBuf {
    dir.join(format!("request_{seq:08}.p0j"))
}

fn outcome_path(dir: &Path, seq: u64) -> PathBuf {
    dir.join(format!("outcome_{seq:08}.p0j"))
}

fn epoch_path(dir: &Path, epoch: u64) -> PathBuf {
    dir.join(format!("epoch_{epoch:08}.p0j"))
}

fn numbered(dir: &Path, prefix: &str) -> Result<Vec<u64>, String> {
    safe_dir(dir)?;
    super::source_control::numbered(dir, prefix)
        .map_err(|e| e.replace("SOURCE_CONTROL_IO", "CAPABILITY_CONTROL_IO"))
}

fn sequences(dir: &Path) -> Result<Vec<u64>, String> {
    numbered(dir, "request_")
}

/// P8-CTRL.6: the control directory is a real directory (or not there yet), never a symlink — an
/// unsafe store grants nothing.
fn safe_dir(dir: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(dir) {
        Ok(m) if m.file_type().is_dir() => Ok(()),
        Ok(_) => Err(format!(
            "CAPABILITY_CONTROL_UNSAFE: {} is no directory",
            dir.display()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("CAPABILITY_CONTROL_IO: {e}")),
    }
}

/// P8-CTRL.6: a control file is a regular file, never a symlink.
fn regular(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_file() => Ok(()),
        Ok(_) => Err(format!(
            "CAPABILITY_CONTROL_UNSAFE: {} is no regular file",
            path.display()
        )),
        Err(e) => Err(format!("CAPABILITY_CONTROL_IO: {e}")),
    }
}

/// The controllable capabilities of fixed names and their values: LIVE (DEC-13), the B hold mode
/// (DEC-04), the bundle path (DEC-11), the threshold mode (DEC-06), inventory rebalancing (B01).
/// The economic switches (`ECON`) and the wallets (`WALLET:<pubkey>`) take `SWITCH_VALUES`.
pub const CAPABILITIES: [(&str, &[&str]); 5] = [
    (LIVE, &LIVE_VALUES),
    (
        crate::execution::hold_b::HOLD_B,
        &crate::execution::hold_b::HOLD_B_VALUES,
    ),
    (
        crate::execution::bundle_path::JITO_BUNDLE,
        &crate::execution::bundle_path::JITO_BUNDLE_VALUES,
    ),
    (
        super::threshold_mode::THRESHOLD_TRIGGER,
        &super::threshold_mode::VALUES,
    ),
    (
        crate::execution::inventory::INVENTORY_REBALANCING,
        &crate::execution::inventory::REBALANCE_VALUES,
    ),
];

/// P8-CTRL.1: the signer a `WALLET:<pubkey>` capability names (canonical base58 only); `None`:
/// no wallet capability.
pub fn wallet_of(capability: &str) -> Option<Result<Pubkey, String>> {
    let key = capability.strip_prefix(WALLET_PREFIX)?;
    Some(match Pubkey::from_str(key) {
        Ok(pk) if pk.to_string() == key => Ok(pk),
        _ => Err(format!(
            "CAPABILITY_WALLET_INVALID: {} (WALLET:<base58 pubkey>)",
            shown(capability, "")
        )),
    })
}

/// A request names a known capability and one of its values.
pub fn validate(capability: &str, requested: &str) -> Result<(), String> {
    if capability == FE06_GRANT || capability == FE06_REVOKE {
        return if requested.len() == 64
            && requested
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            Ok(())
        } else {
            Err(format!(
                "CAPABILITY_VALUE_INVALID: {} (the scope digest: 64 lowercase hex digits)",
                shown(capability, requested)
            ))
        };
    }
    let values: &[&str] = match wallet_of(capability) {
        Some(Err(e)) => return Err(e),
        Some(Ok(_)) => &SWITCH_VALUES,
        None if ECON.contains(&capability) => &SWITCH_VALUES,
        None => match CAPABILITIES.iter().find(|(c, _)| *c == capability) {
            Some((_, values)) => values,
            None => {
                return Err(format!(
                    "CAPABILITY_UNKNOWN: {} (LIVE|HOLD_B|JITO_BUNDLE|THRESHOLD_TRIGGER|INVENTORY_REBALANCING|ECON_SIGN|ECON_ARM|ECON_SEND|ECON_ATTEMPT|ECON_LIVE|WALLET:<pubkey>|FE06_GRANT|FE06_REVOKE)",
                    shown(capability, "")
                ))
            }
        },
    };
    if !values.contains(&requested) {
        return Err(format!(
            "CAPABILITY_VALUE_INVALID: {} ({})",
            shown(capability, requested),
            values.join("|")
        ));
    }
    Ok(())
}

/// P8-CTRL.6: a request id is 1–64 of `A-Za-z0-9_-` (the GUI's command id, or `new_request_id`).
pub fn validate_request_id(id: &str) -> Result<(), String> {
    if (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        Ok(())
    } else {
        Err(format!(
            "CAPABILITY_REQUEST_ID_INVALID: {} (1–64 of A-Za-z0-9_-)",
            shown(id, "")
        ))
    }
}

/// P8-CTRL.6: a fresh one-time request id (128 random bits).
pub fn new_request_id() -> Result<String, String> {
    journal::random16()
        .map(|b| journal::hex(&b))
        .map_err(|e| format!("CAPABILITY_CONTROL_IO: {e}"))
}

/// The capability configuration's version: the request log's head (0 before the first request).
pub fn head(dir: &Path) -> Result<u64, String> {
    Ok(sequences(dir)?.last().copied().unwrap_or(0))
}

/// P8-CTRL.6: the newest owner epoch of this project (0: no owner has opened one).
pub fn current_epoch(dir: &Path) -> Result<u64, String> {
    Ok(numbered(dir, "epoch_")?.last().copied().unwrap_or(0))
}

/// P8-CTRL.6: the owner's fresh epoch, opened at its start — the next number, published without
/// overwriting (of two owners racing for one number exactly one wins, the other takes the next).
/// Requests of every earlier epoch are refused from now on.
pub fn open_epoch(dir: &Path, owner_pid: u32, now_unix_s: Option<u64>) -> Result<u64, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("CAPABILITY_CONTROL_IO: {e}"))?;
    loop {
        let epoch = current_epoch(dir)? + 1;
        let value = CapabilityOwnerEpoch {
            epoch,
            owner_pid,
            opened_unix_s: now_unix_s,
        };
        match journal::create_typed_value(
            &epoch_path(dir, epoch),
            journal::source::CAPABILITY_CONTROL,
            &value,
        ) {
            Ok(()) => return Ok(epoch),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("CAPABILITY_CONTROL_IO: {e}")),
        }
    }
}

/// P8-CTRL.6: the owner process that opened `epoch` (its sealed epoch record).
pub fn epoch_owner(dir: &Path, epoch: u64) -> Result<u32, String> {
    let path = epoch_path(dir, epoch);
    regular(&path)?;
    match journal::read_typed_value::<CapabilityOwnerEpoch>(&path) {
        Ok(e) if e.epoch == epoch => Ok(e.owner_pid),
        Ok(_) => Err(format!(
            "CAPABILITY_EPOCH_INVALID: epoch_{epoch:08}.p0j names another epoch"
        )),
        Err(e) => Err(format!("CAPABILITY_EPOCH_INVALID: {e}")),
    }
}

/// One capability request as the operator states it (`write_request`).
#[derive(Clone, Copy, Debug)]
pub struct NewRequest<'a> {
    pub capability: &'a str,
    pub requested: &'a str,
    /// P8-CTRL.6: the one-time id of this operator action.
    pub request_id: &'a str,
    /// P8-CTRL.6: the owner epoch the operator's view showed.
    pub owner_epoch: u64,
    /// CTRL-03: the configuration version the operator's view showed (`None`: no CAS).
    pub expected: Option<u64>,
    pub now_unix_s: u64,
}

/// What `write_request` did: the request's sequence number, and whether the same request (same
/// id, same bytes) had been written already — then nothing new was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Written {
    pub seq: u64,
    pub replayed: bool,
}

/// A request, written only when the log's head is exactly `expected` (CTRL-03 CAS) and published
/// without overwriting (of two writers of one version exactly one wins). P8-CTRL.6: its id is
/// looked up first — the same id with the same bytes is that request again (a retry after a lost
/// answer finds its sequence number), with other bytes a conflict.
pub fn write_request(dir: &Path, r: &NewRequest) -> Result<Written, String> {
    validate(r.capability, r.requested)?;
    validate_request_id(r.request_id)?;
    if r.owner_epoch == 0 {
        return Err(
            "CAPABILITY_EPOCH_MISSING: no owner epoch; the request names the epoch its view showed"
                .into(),
        );
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("CAPABILITY_CONTROL_IO: {e}"))?;
    if let Some(prior) = request_with_id(dir, r.request_id)? {
        return same_request(&prior, r);
    }
    let current = head(dir)?;
    let conflict = |current: u64| {
        format!(
            "CAPABILITY_VERSION_CONFLICT: expected {}, current {current}; reload and retry",
            r.expected.map_or("none".into(), |e| e.to_string())
        )
    };
    if r.expected.is_some_and(|e| e != current) {
        return Err(conflict(current));
    }
    let seq = current + 1;
    let request = CapabilityControlRequest {
        seq,
        capability: r.capability.into(),
        requested: r.requested.into(),
        requested_unix_s: r.now_unix_s,
        request_id: r.request_id.into(),
        owner_epoch: r.owner_epoch,
    };
    match journal::create_typed_value(
        &request_path(dir, seq),
        journal::source::CAPABILITY_CONTROL,
        &request,
    ) {
        Ok(()) => {}
        // Of two writers of one version exactly one wins — the winner may be this very request
        // (the same id, a retry racing its original).
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return match request_with_id(dir, r.request_id)? {
                Some(prior) => same_request(&prior, r),
                None => Err(conflict(head(dir).unwrap_or(seq))),
            };
        }
        Err(e) => return Err(format!("CAPABILITY_CONTROL_IO: {e}")),
    }
    Ok(Written {
        seq,
        replayed: false,
    })
}

/// P8-CTRL.6: `r` against the written request of its id — the same request again (its sequence
/// number), the same action of an earlier epoch (stale: a new action needs a new id), or other
/// content under the id (a conflict).
fn same_request(prior: &CapabilityControlRequest, r: &NewRequest) -> Result<Written, String> {
    if (prior.capability.as_str(), prior.requested.as_str()) != (r.capability, r.requested) {
        return Err(format!(
            "CAPABILITY_REQUEST_ID_CONFLICT: request id {} is seq {} with other content",
            r.request_id, prior.seq
        ));
    }
    if prior.owner_epoch != r.owner_epoch {
        return Err(format!(
            "CAPABILITY_EPOCH_STALE: request id {} was seq {} of owner epoch {}, the owner is epoch {}; a new action needs a new request id",
            r.request_id, prior.seq, prior.owner_epoch, r.owner_epoch
        ));
    }
    Ok(Written {
        seq: prior.seq,
        replayed: true,
    })
}

/// The newest readable request with `id` (an unreadable one is nobody's: the owner refuses it).
fn request_with_id(dir: &Path, id: &str) -> Result<Option<CapabilityControlRequest>, String> {
    for seq in sequences(dir)?.into_iter().rev() {
        if let Ok(r) = request_at(dir, seq) {
            if r.request_id == id {
                return Ok(Some(r));
            }
        }
    }
    Ok(None)
}

/// Request `seq` as written (the control thread's input).
pub fn request_at(dir: &Path, seq: u64) -> Result<CapabilityControlRequest, String> {
    let path = request_path(dir, seq);
    regular(&path)?;
    match journal::read_typed_value::<CapabilityControlRequest>(&path) {
        Ok(r) if r.seq == seq => Ok(r),
        Ok(_) => Err("CAPABILITY_CONTROL_REQUEST_SEQ".into()),
        Err(e) => Err(format!("CAPABILITY_CONTROL_REQUEST_INVALID: {e}")),
    }
}

/// The sequence numbers of the requests after `handed`, ascending.
pub fn requests_after(dir: &Path, handed: u64) -> Result<Vec<u64>, String> {
    Ok(sequences(dir)?
        .into_iter()
        .filter(|s| *s > handed)
        .collect())
}

/// The newest request, if any.
pub fn latest_request(dir: &Path) -> Result<Option<CapabilityControlRequest>, String> {
    match sequences(dir)?.last() {
        None => Ok(None),
        Some(seq) => request_at(dir, *seq).map(Some),
    }
}

/// The owner: records its decision on request `outcome.seq` (the durable receipt).
pub fn write_outcome(dir: &Path, outcome: &CapabilityControlOutcome) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("CAPABILITY_CONTROL_IO: {e}"))?;
    safe_dir(dir)?;
    journal::write_typed_value(
        &outcome_path(dir, outcome.seq),
        journal::source::CAPABILITY_CONTROL,
        outcome,
    )
    .map_err(|e| format!("CAPABILITY_CONTROL_IO: {e}"))
}

/// The owner's decision on request `seq`, if it recorded one.
pub fn outcome_at(dir: &Path, seq: u64) -> Result<Option<CapabilityControlOutcome>, String> {
    let path = outcome_path(dir, seq);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        _ => regular(&path)?,
    }
    journal::read_typed_value(&path)
        .map(Some)
        .map_err(|e| format!("CAPABILITY_CONTROL_OUTCOME_INVALID: {e}"))
}

/// The newest decision of an owner, if any.
pub fn latest_outcome(dir: &Path) -> Result<Option<CapabilityControlOutcome>, String> {
    match numbered(dir, "outcome_")?.last() {
        None => Ok(None),
        Some(seq) => outcome_at(dir, *seq),
    }
}

/// P8-CTRL.6: the capabilities as the receipts of owner epoch `epoch` decided them — per
/// capability its newest applied decision, by name (`p0ctl capability status`). A capability
/// without one is at its start value (DISABLED, LIVE OFF).
pub fn epoch_decisions(dir: &Path, epoch: u64) -> Result<Vec<CapabilityControlOutcome>, String> {
    let mut by_capability = std::collections::BTreeMap::new();
    for seq in numbered(dir, "outcome_")? {
        if let Some(o) = outcome_at(dir, seq)? {
            if o.owner_epoch == epoch && o.applied && !o.capability.is_empty() {
                by_capability.insert(o.capability.clone(), o);
            }
        }
    }
    Ok(by_capability.into_values().collect())
}

/// DEC-13: the separate FE-06 authorization that opens LIVE — the pipeline mode, Safety LIVE, the
/// asset allowlist and the submitter together. No request, switch, FE-04 approval or
/// configuration grants it; P8-CTRL-02 makes it the owner's FE-06 grant — until then this build
/// reads none and no production path constructs one: LIVE stays BLOCKED (only the offline tests
/// hold one). It must open all of them at once — `OwnerCapabilities::fe06`, `ObserveOnly::open`
/// and Safety LIVE — never the mode alone (review L7).
#[derive(Debug, PartialEq, Eq)]
pub struct Fe06Authorization {
    /// The digest of its scope (P8-CTRL.3).
    digest: String,
    /// Its scope; `None` only for the offline tests' stand-in.
    scope: Option<Box<super::fe06::Scope>>,
    not_before_unix_s: u64,
    expires_unix_s: u64,
}

impl Fe06Authorization {
    /// The digest of its scope.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Its scope (`None`: the offline tests' stand-in).
    pub fn scope(&self) -> Option<&super::fe06::Scope> {
        self.scope.as_deref()
    }

    /// P8-CTRL.5: the separate exit reserve an attempt of `mints` under this grant holds with it
    /// (`fe06::exit_reserve`); `None`: no safe reserve — no new liquidation. Only the offline
    /// tests' stand-in (no scope, never part of a build) reserves none.
    pub fn exit_reserve_lamports(&self, mints: &[String]) -> Option<u64> {
        match self.scope() {
            Some(scope) => super::fe06::exit_reserve(scope, mints),
            #[cfg(test)]
            None => Some(0),
            #[cfg(not(test))]
            None => None,
        }
    }

    /// P8-CTRL.3: in force at `now_unix_s` — within its scope's validity (expired: no grant).
    pub fn in_force(&self, now_unix_s: u64) -> bool {
        (self.not_before_unix_s..self.expires_unix_s).contains(&now_unix_s)
    }

    /// The offline tests' stand-in of an FE-06 authorization, always in force (never part of a
    /// build).
    #[cfg(test)]
    pub(crate) fn offline_test() -> Self {
        Self {
            digest: "offline-test".into(),
            scope: None,
            not_before_unix_s: 0,
            expires_unix_s: u64::MAX,
        }
    }

    /// The offline tests' grant of `scope` (never part of a build).
    #[cfg(test)]
    pub(crate) fn offline_test_of(scope: super::fe06::Scope) -> Self {
        Self {
            digest: super::fe06::digest(&scope).unwrap(),
            not_before_unix_s: scope.not_before_unix_s,
            expires_unix_s: scope.expires_unix_s,
            scope: Some(Box::new(scope)),
        }
    }
}

/// P8-CTRL.1: the requested economic switches of an owner — DISABLED at its start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EconSwitches {
    enabled: [bool; 5],
}

impl EconSwitches {
    fn index(name: &str) -> Option<usize> {
        ECON.iter().position(|e| *e == name)
    }

    /// The switch `name` (`None`: no economic switch).
    pub fn get(&self, name: &str) -> Option<bool> {
        Self::index(name).map(|i| self.enabled[i])
    }

    /// Sets the switch `name`; `false`: no economic switch.
    pub fn set(&mut self, name: &str, enabled: bool) -> bool {
        match Self::index(name) {
            Some(i) => {
                self.enabled[i] = enabled;
                true
            }
            None => false,
        }
    }
}

/// P8-CTRL.1: one configured signer and its requested switch (DISABLED at the owner's start).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletSwitch {
    pub pubkey: Pubkey,
    /// Its validated roles (`ROLE_LIQUIDATOR`, `ROLE_FEE_PAYER`).
    pub roles: Vec<&'static str>,
    pub enabled: bool,
}

impl WalletSwitch {
    /// A configured signer, DISABLED.
    pub fn configured(pubkey: Pubkey, roles: &[&'static str]) -> Self {
        Self {
            pubkey,
            roles: roles.to_vec(),
            enabled: false,
        }
    }

    /// Its capability name (`WALLET:<pubkey>`).
    pub fn capability(&self) -> String {
        format!("{WALLET_PREFIX}{}", self.pubkey)
    }

    /// P8-CTRL.3: how a scope names it (`<pubkey>:<ROLE>+<ROLE>`, its roles in their order).
    pub fn scope_entry(&self) -> String {
        format!("{}:{}", self.pubkey, self.roles.join("+"))
    }
}

/// P8-CTRL.7 (review M5): no egress while a stop waits for the owner.
pub const STOP_PENDING: &str =
    "EGRESS_STOP_PENDING: a stop (DISABLE, LIVE OFF or a revocation) waits for the owner";

/// P8-CTRL.7 (review M5): the economic stops — a switch DISABLED, LIVE OFF, an FE-06 revocation —
/// the control thread has read from the request log and the owner has decided. While one waits
/// no real signature and no egress is permitted: a stop durably requested before an egress
/// prevents its start without waiting for the owner's next step (which may be behind a network
/// call). Counted by the control thread once per request when it reads it (review N3: also while
/// a full queue delays its hand-over), and by the owner once it has decided it.
#[derive(Debug, Default)]
pub struct StopSignal {
    queued: std::sync::atomic::AtomicU64,
    decided: std::sync::atomic::AtomicU64,
}

impl StopSignal {
    /// A stop the control thread read from the request log (counted once; it waits from then
    /// until the owner decided it, also while a full queue delays its hand-over).
    pub fn queue(&self) {
        self.queued
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    /// The owner decided a stop (applied or refused).
    pub fn decide(&self) {
        self.decided
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    /// A stop waits for the owner.
    pub fn pending(&self) -> bool {
        use std::sync::atomic::Ordering::SeqCst;
        self.decided.load(SeqCst) < self.queued.load(SeqCst)
    }
}

/// P8-CTRL.7: whether a request is an economic stop (one an egress never runs ahead of).
pub fn is_stop(capability: &str, requested: &str) -> bool {
    ((ECON.contains(&capability) || capability.starts_with(WALLET_PREFIX)) && requested == DISABLED)
        || (capability == LIVE && requested == LIVE_OFF)
        || capability == FE06_REVOKE
}

/// P8-CTRL.2: what LIVE's effective value rests on at one instant.
#[derive(Clone, Copy, Debug)]
pub struct LiveGates<'a> {
    pub requested: &'a str,
    pub econ: &'a EconSwitches,
    /// Every signer the economic path needs.
    pub wallets: &'a [WalletSwitch],
    pub fe06: Option<&'a Fe06Authorization>,
    /// The wall clock (unix s) a grant's validity is read at; `None`: unreadable — no grant.
    pub now_unix_s: Option<u64>,
    /// P8-CTRL.7: the owner epoch an egress permit binds (`None`: no capability control).
    pub epoch: Option<u64>,
    /// P8-CTRL.3: what the owner runs — a grant's scope binds exactly it (`None`: unknown, so
    /// no grant's scope is ever within it).
    pub running: Option<&'a super::fe06::Running>,
    /// P8-CTRL.3: the canonical data source the owner's state comes from (`None`: none).
    pub source: Option<&'a darkbot_solana::ingress::sources::SourceUnit>,
    /// P8-CTRL.7 (review M5): the stops waiting for the owner (`None`: no control thread).
    pub stop: Option<&'a std::sync::Arc<StopSignal>>,
}

/// P8-CTRL.3: a grant's scope against what runs now (no identity or no clock: refused).
fn scope_runs(g: &LiveGates, scope: &super::fe06::Scope, refusal: &str) -> Result<(), String> {
    let running = g
        .running
        .ok_or_else(|| format!("{refusal}: what the owner runs is unknown"))?;
    let now = g
        .now_unix_s
        .ok_or_else(|| format!("{refusal}: no readable clock"))?;
    super::fe06::check_running(scope, running, now)
}

/// P8-CTRL.3/.5: whether a scope of `mode` takes a use of `purpose` (a new attempt: ALL; an exit:
/// ALL or EXIT_ONLY; SIM_ONLY neither).
fn mode_takes(purpose: Purpose, mode: super::fe06::Mode) -> bool {
    use super::fe06::Mode;
    matches!(
        (purpose, mode),
        (Purpose::NewAttempt, Mode::All) | (Purpose::Exit, Mode::All | Mode::ExitOnly)
    )
}

/// The purpose of a use.
fn purpose_of(use_: &super::fe06::Use) -> Purpose {
    match use_ {
        super::fe06::Use::Attempt(_) => Purpose::NewAttempt,
        super::fe06::Use::Exit(_) => Purpose::Exit,
    }
}

/// The scope's name of a submission adapter.
fn adapter_name(path: darkbot_solana::submit::SubmissionPath) -> &'static str {
    match path {
        darkbot_solana::submit::SubmissionPath::Rpc => "rpc",
        darkbot_solana::submit::SubmissionPath::JitoSingle => "jito_single",
        darkbot_solana::submit::SubmissionPath::JitoBundle => "jito_bundle",
    }
}

/// P8-CTRL.3/.5/.7: what a grant's scope demands of one real signature or one egress of `use_`
/// over `path` — it names the adapter and bounds every fact of the use (that it takes the
/// purpose and binds what runs is LIVE's own gate, `decision`, which both permits require).
/// The one check of both permits (`refusal` names which).
fn scope_admits(
    scope: &super::fe06::Scope,
    use_: &super::fe06::Use,
    path: darkbot_solana::submit::SubmissionPath,
    refusal: &str,
) -> Result<(), String> {
    let adapter = adapter_name(path);
    if !scope.submission_adapters.contains(adapter) {
        return Err(format!(
            "{refusal}_ADAPTER_NOT_IN_SCOPE: {adapter} is no adapter of the grant's scope"
        ));
    }
    super::fe06::check_use(scope, use_)
}

/// P8-CTRL.2: LIVE's effective value for a new liquidation — BLOCKED while any gate is missing
/// (every economic switch, every required wallet, the FE-06 authorization), whatever is
/// requested, with the first missing gate as its reason and every other one named after it; else
/// the requested value.
pub fn live_decision(g: &LiveGates) -> (&'static str, String) {
    decision(g, Purpose::NewAttempt)
}

/// P8-CTRL.5: the same decision for the exit of an open position: ECON_ATTEMPT (new liquidations
/// only) is not one of its gates; ECON_LIVE, ARM, SIGN, SEND, the wallets and FE-06 are.
pub fn exit_decision(g: &LiveGates) -> (&'static str, String) {
    decision(g, Purpose::Exit)
}

/// P8-CTRL.3: the one reason of a grant outside its validity (LIVE, the status, `p0ctl`).
pub fn grant_not_in_force(digest: &str) -> String {
    format!("FE-06 grant {digest} not in force (expired, not yet valid or no readable clock)")
}

fn decision(g: &LiveGates, purpose: Purpose) -> (&'static str, String) {
    let switches: &[&str] = match purpose {
        Purpose::NewAttempt => &ECON,
        Purpose::Exit => &ECON_EXIT,
    };
    let mut missing: Vec<String> = switches
        .iter()
        .filter(|e| g.econ.get(e) != Some(true))
        .map(|e| format!("{e} {DISABLED}"))
        .collect();
    if g.wallets.is_empty() {
        missing.push("WALLET: no configured signer".into());
    }
    missing.extend(
        g.wallets
            .iter()
            .filter(|w| !w.enabled)
            .map(|w| format!("{} {DISABLED}", w.capability())),
    );
    match g.fe06 {
        None => missing.push(LIVE_BLOCK_REASON.into()),
        Some(a) if !g.now_unix_s.is_some_and(|now| a.in_force(now)) => {
            missing.push(grant_not_in_force(a.digest()))
        }
        // Review M3: a grant whose scope takes no such use, or does not bind what runs, opens
        // nothing — LIVE says so (and stays Shadow: SIM_ONLY evidence is never lost to it).
        Some(a) => {
            if let Some(scope) = a.scope() {
                if !mode_takes(purpose, scope.mode) {
                    missing.push(format!(
                        "FE-06 grant {} is {}: it takes no {purpose:?}",
                        a.digest(),
                        scope.mode.code()
                    ));
                }
                if let Err(e) = scope_runs(g, scope, "FE06_SCOPE_BINDING") {
                    missing.push(e);
                }
            }
        }
    }
    match missing.split_first() {
        None if g.requested == LIVE => (LIVE, String::new()),
        None => (LIVE_OFF, String::new()),
        Some((first, [])) => ("BLOCKED", first.clone()),
        Some((first, rest)) => (
            "BLOCKED",
            format!("{first} (missing too: {})", rest.join("; ")),
        ),
    }
}

/// P8-CTRL.7: what an egress is for — a new liquidation (every economic switch) or the exit of an
/// open position (every switch but ECON_ATTEMPT).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    NewAttempt,
    Exit,
}

/// P8-CTRL.7: the one-use permission of exactly one network egress — issued from the owner's
/// gates at the handoff (no CLI, test or other crate constructs one: its fields are private and
/// `issue` decides), bound to the owner epoch, the FE-06 grant (digest, validity, mode, adapters),
/// the payload's signatures and path and the candidate's deadline; consumed by the FE-04 transport
/// immediately before the adapter call, where anything that differs refuses the egress.
#[derive(Debug)]
pub struct EgressPermit {
    stop: Option<std::sync::Arc<StopSignal>>,
    epoch: Option<u64>,
    grant: String,
    grant_expires_unix_s: u64,
    purpose: Purpose,
    signatures: Vec<String>,
    path: darkbot_solana::submit::SubmissionPath,
    deadline: std::time::Instant,
}

impl EgressPermit {
    /// The permit of a payload (its signatures and path) under the gates now: LIVE effective for
    /// its use (a new attempt or an exit) with a grant in force whose scope takes this purpose
    /// and this adapter, binds exactly what runs and bounds the use (P8-CTRL.3/.5).
    pub fn issue(
        g: &LiveGates,
        use_: &super::fe06::Use,
        signatures: &[String],
        path: darkbot_solana::submit::SubmissionPath,
        deadline: std::time::Instant,
    ) -> Result<Self, String> {
        let purpose = purpose_of(use_);
        if g.stop.is_some_and(|s| s.pending()) {
            return Err(STOP_PENDING.into());
        }
        let (effective, reason) = match purpose {
            Purpose::NewAttempt => live_decision(g),
            Purpose::Exit => exit_decision(g),
        };
        if effective != LIVE {
            return Err(format!(
                "EGRESS_NOT_PERMITTED: LIVE {effective}{}{reason}",
                if reason.is_empty() { "" } else { " · " }
            ));
        }
        let grant = g.fe06.ok_or("EGRESS_NOT_PERMITTED: no FE-06 grant")?;
        let mut expires = u64::MAX;
        if let Some(scope) = grant.scope() {
            scope_admits(scope, use_, path, "EGRESS")?;
            expires = scope.expires_unix_s;
        }
        if signatures.is_empty() {
            return Err("EGRESS_PERMIT_PAYLOAD: no signature".into());
        }
        Ok(Self {
            stop: g.stop.cloned(),
            epoch: g.epoch,
            grant: grant.digest().to_string(),
            grant_expires_unix_s: expires,
            purpose,
            signatures: signatures.to_vec(),
            path,
            deadline,
        })
    }

    /// The grant the permit was issued under.
    pub fn grant(&self) -> &str {
        &self.grant
    }

    /// The owner epoch the permit binds.
    pub fn epoch(&self) -> Option<u64> {
        self.epoch
    }

    /// The transport's check immediately before the adapter call (it consumes the permit): the
    /// payload is the permit's, of its purpose, before its deadline and while its grant is in
    /// force at the handoff's wall clock.
    pub fn admit(
        self,
        purpose: Purpose,
        signatures: &[String],
        path: darkbot_solana::submit::SubmissionPath,
        now_unix_s: u64,
    ) -> Result<(), String> {
        if self.purpose != purpose || self.signatures != signatures || self.path != path {
            return Err("EGRESS_PERMIT_PAYLOAD: the permit is another payload's".into());
        }
        // Review M5: immediately before the adapter call a stop that waits still prevents it.
        // (Review N4, accepted: a stop first seen here leaves the durable reservation UNKNOWN —
        // nothing was sent, its non-landing settlement releases the holds, its booking counts.)
        if self.stop.as_ref().is_some_and(|s| s.pending()) {
            return Err(STOP_PENDING.into());
        }
        if std::time::Instant::now() >= self.deadline {
            return Err("EGRESS_PERMIT_DEADLINE".into());
        }
        if now_unix_s >= self.grant_expires_unix_s {
            return Err("EGRESS_PERMIT_GRANT_EXPIRED".into());
        }
        Ok(())
    }
}

/// P8-CTRL.4/.7: what a real signature is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignPurpose {
    /// The payload of a new liquidation (LIVE).
    NewAttempt,
    /// The exit of an open position (LIVE).
    Exit,
    /// A simulation only: needs a SIM_ONLY grant; never armed, never sent.
    Simulation,
}

/// P8-CTRL.4/.7: the one-use permission of one real signature with the wallet's own keys —
/// issued from the owner's gates (its fields are private: no CLI, test or other crate makes one)
/// for exactly one message and its signers: a new attempt or an exit while LIVE is effective for
/// it under a grant whose scope takes it, or a simulation under a SIM_ONLY grant in force with
/// ECON_SIGN and every signer ENABLED. Without one, a simulation is signed synthetically.
#[derive(Debug)]
pub struct SignPermit {
    purpose: SignPurpose,
    signers: Vec<Pubkey>,
    message: solana_sdk::hash::Hash,
}

impl SignPermit {
    /// The permit of one real signature of `message` by `signers` for `purpose`. Under a grant
    /// with a scope `bound` names the signature's use and adapter: the scope's whole check runs
    /// here, before any byte is signed (review H1 — no signed payload ever leaves for a
    /// simulation or a send that its scope would refuse).
    pub fn issue(
        g: &LiveGates,
        purpose: SignPurpose,
        signers: &[Pubkey],
        message: &solana_sdk::message::VersionedMessage,
        bound: Option<(&super::fe06::Use, darkbot_solana::submit::SubmissionPath)>,
    ) -> Result<Self, String> {
        use super::fe06::Mode;
        // Review N2: no real signature while a stop waits for the owner either.
        if g.stop.is_some_and(|s| s.pending()) {
            return Err(STOP_PENDING.into());
        }
        let scope = g.fe06.and_then(|a| a.scope());
        let unbound = "SIGN_SCOPE_USE: the signature's use is unknown";
        match purpose {
            SignPurpose::NewAttempt | SignPurpose::Exit => {
                let (effective, reason) = if purpose == SignPurpose::NewAttempt {
                    live_decision(g)
                } else {
                    exit_decision(g)
                };
                if effective != LIVE {
                    return Err(format!("SIGN_NOT_PERMITTED: LIVE {effective} · {reason}"));
                }
                if let Some(scope) = scope {
                    let (use_, path) = bound.ok_or(unbound)?;
                    let of = match purpose {
                        SignPurpose::NewAttempt => Purpose::NewAttempt,
                        _ => Purpose::Exit,
                    };
                    if purpose_of(use_) != of {
                        return Err("SIGN_SCOPE_USE: the use is another purpose's".into());
                    }
                    scope_admits(scope, use_, path, "SIGN")?;
                }
            }
            SignPurpose::Simulation => {
                if g.econ.get(ECON_SIGN) != Some(true) {
                    return Err("SIGN_NOT_PERMITTED: ECON_SIGN DISABLED".into());
                }
                let in_force = g
                    .fe06
                    .is_some_and(|a| g.now_unix_s.is_some_and(|now| a.in_force(now)));
                let Some(scope) = scope.filter(|s| s.mode == Mode::SimOnly && in_force) else {
                    return Err("SIGN_NOT_PERMITTED: no SIM_ONLY grant in force".into());
                };
                // A simulation is never handed to an adapter that sends: no adapter binds it;
                // what runs and every fact of its use do.
                let (use_, _) = bound.ok_or(unbound)?;
                scope_runs(g, scope, "SIGN_SCOPE_BINDING")?;
                super::fe06::check_use(scope, use_)?;
            }
        }
        for s in signers {
            if !g.wallets.iter().any(|w| w.pubkey == *s && w.enabled) {
                return Err(format!("SIGN_NOT_PERMITTED: {WALLET_PREFIX}{s} DISABLED"));
            }
        }
        if signers.is_empty() {
            return Err("SIGN_NOT_PERMITTED: no signer".into());
        }
        Ok(Self {
            purpose,
            signers: signers.to_vec(),
            message: solana_sdk::hash::hash(&message.serialize()),
        })
    }

    /// Its purpose.
    pub fn purpose(&self) -> SignPurpose {
        self.purpose
    }

    /// The real signature of exactly the permitted message by exactly the permitted signers.
    pub fn sign(
        self,
        tx: &mut solana_sdk::transaction::VersionedTransaction,
        keys: &[&solana_sdk::signature::Keypair],
    ) -> Result<(), String> {
        use solana_sdk::signature::Signer;
        if solana_sdk::hash::hash(&tx.message.serialize()) != self.message {
            return Err("SIGN_PERMIT_MESSAGE: the permit is another message's".into());
        }
        let keys_of: Vec<Pubkey> = keys.iter().map(|k| k.pubkey()).collect();
        if keys_of != self.signers {
            return Err("SIGN_PERMIT_SIGNERS: the permit is other signers'".into());
        }
        darkbot_solana::tx::sign_transaction(tx, keys).map_err(|e| e.to_string())
    }
}

/// The offline tests' gates of a fully enabled owner: every switch and `signer` ENABLED, the
/// FE-06 stand-in in force (never part of a build).
#[cfg(test)]
pub(crate) struct OfflineGates {
    pub econ: EconSwitches,
    pub wallets: Vec<WalletSwitch>,
    pub fe06: Fe06Authorization,
    /// P8-CTRL.3: what the default test scope (`fe06::tests::text`) of `signer` binds.
    pub running: super::fe06::Running,
    pub source: darkbot_solana::ingress::sources::SourceUnit,
}

#[cfg(test)]
impl OfflineGates {
    pub(crate) fn enabled(signer: Pubkey) -> Self {
        let mut econ = EconSwitches::default();
        for e in ECON {
            econ.set(e, true);
        }
        let mut w = WalletSwitch::configured(signer, &[ROLE_LIQUIDATOR, ROLE_FEE_PAYER]);
        w.enabled = true;
        Self {
            econ,
            wallets: vec![w],
            fe06: Fe06Authorization::offline_test(),
            running: super::fe06::tests::running(&signer),
            source: darkbot_solana::ingress::sources::SourceUnit::WsRpcControl,
        }
    }

    pub(crate) fn gates(&self) -> LiveGates<'_> {
        LiveGates {
            requested: LIVE,
            econ: &self.econ,
            wallets: &self.wallets,
            fe06: Some(&self.fe06),
            now_unix_s: Some(0),
            epoch: None,
            running: Some(&self.running),
            source: Some(&self.source),
            stop: None,
        }
    }
}

/// A request's operands as they may be shown (log, GUI): bounded, control characters escaped —
/// the request file is operator input, valid or not.
pub fn shown(capability: &str, requested: &str) -> String {
    let text = if requested.is_empty() {
        capability.to_string()
    } else {
        format!("{capability} {requested}")
    };
    let mut shown: String = text
        .chars()
        .take(64)
        .flat_map(char::escape_default)
        .collect();
    if text.chars().count() > 64 {
        shown.push('…');
    }
    shown
}

/// DEC-13/P8-CTRL.2: the pipeline mode of a detection — LIVE only while LIVE is effective, so a
/// candidate reaches no send boundary in this build, whatever was requested.
pub fn pipeline_mode(g: &LiveGates) -> crate::execution::pipeline::Mode {
    if live_decision(g).0 == LIVE {
        crate::execution::pipeline::Mode::Live
    } else {
        crate::execution::pipeline::Mode::Shadow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p0-cap-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn new<'a>(capability: &'a str, requested: &'a str, id: &'a str) -> NewRequest<'a> {
        NewRequest {
            capability,
            requested,
            request_id: id,
            owner_epoch: 1,
            expected: None,
            now_unix_s: 1,
        }
    }

    /// DEC-13: requests are validated, CAS-versioned and never overwritten; LIVE is effective
    /// for no request (BLOCKED, every missing gate named).
    #[test]
    fn dec13_live_requests_are_versioned_and_never_effective() {
        let dir = temp("dec13");
        assert_eq!(head(&dir).unwrap(), 0);
        assert!(validate("JITO", "LIVE")
            .unwrap_err()
            .starts_with("CAPABILITY_UNKNOWN"));
        assert!(validate(LIVE, "ON")
            .unwrap_err()
            .starts_with("CAPABILITY_VALUE_INVALID"));
        // DEC-11: the bundle path is a capability of its own values.
        for v in crate::execution::bundle_path::JITO_BUNDLE_VALUES {
            assert_eq!(
                validate(crate::execution::bundle_path::JITO_BUNDLE, v),
                Ok(())
            );
        }
        assert!(validate(crate::execution::bundle_path::JITO_BUNDLE, "LIVE")
            .unwrap_err()
            .starts_with("CAPABILITY_VALUE_INVALID"));
        // DEC-04: the B hold mode is a capability of its own values.
        for v in crate::execution::hold_b::HOLD_B_VALUES {
            assert_eq!(validate(crate::execution::hold_b::HOLD_B, v), Ok(()));
        }
        assert!(validate(crate::execution::hold_b::HOLD_B, "LIVE")
            .unwrap_err()
            .starts_with("CAPABILITY_VALUE_INVALID"));
        assert!(validate(LIVE, "OBSERVE")
            .unwrap_err()
            .starts_with("CAPABILITY_VALUE_INVALID"));
        let cas = |requested, id, expected| NewRequest {
            expected,
            ..new(LIVE, requested, id)
        };
        assert_eq!(
            write_request(&dir, &cas("LIVE", "a", Some(0))).unwrap(),
            Written {
                seq: 1,
                replayed: false
            }
        );
        assert!(write_request(&dir, &cas("OFF", "b", Some(0)))
            .unwrap_err()
            .starts_with("CAPABILITY_VERSION_CONFLICT"));
        assert_eq!(
            write_request(&dir, &cas("OFF", "c", Some(1))).unwrap().seq,
            2
        );
        assert_eq!(requests_after(&dir, 0).unwrap(), vec![1, 2]);
        assert_eq!(request_at(&dir, 1).unwrap().requested, "LIVE");
        assert_eq!(latest_request(&dir).unwrap().unwrap().requested, "OFF");
        assert_eq!(shown("LIVE", "LIVE"), "LIVE LIVE");
        assert_eq!(shown("LIVE\nX", "OFF"), "LIVE\\nX OFF");
        assert_eq!(shown(&"A".repeat(80), "OFF").chars().count(), 65);
        let econ = EconSwitches::default();
        let wallets = [WalletSwitch::configured(
            Pubkey::new_unique(),
            &[ROLE_FEE_PAYER],
        )];
        for requested in LIVE_VALUES {
            let g = LiveGates {
                requested,
                econ: &econ,
                wallets: &wallets,
                fe06: None,
                now_unix_s: Some(0),
                epoch: None,
                running: None,
                source: None,
                stop: None,
            };
            let (effective, reason) = live_decision(&g);
            assert_eq!(effective, "BLOCKED");
            assert!(reason.starts_with("ECON_LIVE DISABLED (missing too: ECON_ARM DISABLED;"));
            assert!(
                reason.ends_with(&format!("{LIVE_BLOCK_REASON})")),
                "{reason}"
            );
            assert_eq!(pipeline_mode(&g), crate::execution::pipeline::Mode::Shadow);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// P8-CTRL.1/.2: the switches take DISABLED|ENABLED; a wallet is named by its canonical
    /// pubkey; LIVE is effective only with every economic switch, every wallet and FE-06 — each
    /// missing gate alone keeps it BLOCKED and is named first.
    #[test]
    fn p8ctrl01_live_is_effective_only_with_every_gate() {
        let pk = Pubkey::new_unique();
        let wallet = format!("{WALLET_PREFIX}{pk}");
        for c in ECON.iter().copied().chain([wallet.as_str()]) {
            for v in SWITCH_VALUES {
                assert_eq!(validate(c, v), Ok(()));
            }
            assert!(validate(c, "ON")
                .unwrap_err()
                .starts_with("CAPABILITY_VALUE_INVALID"));
        }
        for bad in [
            "WALLET:",
            "WALLET:xyz",
            "WALLET:0",
            &format!("WALLET: {pk}"),
        ] {
            assert!(
                validate(bad, ENABLED)
                    .unwrap_err()
                    .starts_with("CAPABILITY_WALLET_INVALID"),
                "{bad}"
            );
        }
        assert!(validate("ECON_FOO", ENABLED)
            .unwrap_err()
            .starts_with("CAPABILITY_UNKNOWN"));
        assert_eq!(wallet_of(&wallet), Some(Ok(pk)));
        assert_eq!(wallet_of(LIVE), None);

        let fe06 = Fe06Authorization::offline_test();
        let mut econ = EconSwitches::default();
        for e in ECON {
            assert_eq!(econ.get(e), Some(false));
            assert!(econ.set(e, true));
        }
        assert!(!econ.set(LIVE, true));
        let mut wallets = vec![
            WalletSwitch::configured(pk, &[ROLE_LIQUIDATOR, ROLE_FEE_PAYER]),
            WalletSwitch::configured(Pubkey::new_unique(), &[ROLE_FEE_PAYER]),
        ];
        wallets.iter_mut().for_each(|w| w.enabled = true);
        let decide = |econ: &EconSwitches, wallets: &[WalletSwitch], fe06, requested| {
            live_decision(&LiveGates {
                requested,
                econ,
                wallets,
                fe06,
                now_unix_s: Some(0),
                epoch: None,
                running: None,
                source: None,
                stop: None,
            })
        };
        // Every gate: the requested value.
        assert_eq!(
            decide(&econ, &wallets, Some(&fe06), LIVE),
            (LIVE, String::new())
        );
        assert_eq!(
            decide(&econ, &wallets, Some(&fe06), LIVE_OFF),
            (LIVE_OFF, String::new())
        );
        // Each economic switch alone.
        for e in ECON {
            let mut one = econ;
            one.set(e, false);
            assert_eq!(
                decide(&one, &wallets, Some(&fe06), LIVE),
                ("BLOCKED", format!("{e} DISABLED"))
            );
        }
        // Each wallet alone, and no signer at all.
        for i in 0..wallets.len() {
            let mut one = wallets.clone();
            one[i].enabled = false;
            assert_eq!(
                decide(&econ, &one, Some(&fe06), LIVE),
                ("BLOCKED", format!("{} DISABLED", one[i].capability()))
            );
        }
        assert_eq!(
            decide(&econ, &[], Some(&fe06), LIVE),
            ("BLOCKED", "WALLET: no configured signer".to_string())
        );
        // FE-06 alone: the B10 reason, never the requested value.
        assert_eq!(
            decide(&econ, &wallets, None, LIVE),
            ("BLOCKED", LIVE_BLOCK_REASON.to_string())
        );
        // Two missing: the first, then the other.
        let mut two = econ;
        two.set(ECON_SEND, false);
        assert_eq!(
            decide(&two, &wallets, None, LIVE).1,
            format!("ECON_SEND DISABLED (missing too: {LIVE_BLOCK_REASON})")
        );
        assert_eq!(
            pipeline_mode(&LiveGates {
                requested: LIVE,
                econ: &econ,
                wallets: &wallets,
                fe06: Some(&fe06),
                now_unix_s: Some(0),
                epoch: None,
                running: None,
                source: None,
                stop: None,
            }),
            crate::execution::pipeline::Mode::Live
        );
    }

    /// P8-CTRL.6: a request names its id and epoch; the same id with the same bytes is the same
    /// request (no second file, its sequence number), with other bytes a conflict — also after a
    /// version moved on (a retry after a lost answer); an unsafe store is refused.
    #[test]
    fn p8ctrl01_requests_are_idempotent_by_id_and_bound_to_an_epoch() {
        let dir = temp("ids");
        assert!(write_request(&dir, &new(ECON_SIGN, ENABLED, ""))
            .unwrap_err()
            .starts_with("CAPABILITY_REQUEST_ID_INVALID"));
        assert!(write_request(&dir, &new(ECON_SIGN, ENABLED, "a/b"))
            .unwrap_err()
            .starts_with("CAPABILITY_REQUEST_ID_INVALID"));
        assert!(
            write_request(&dir, &new(ECON_SIGN, ENABLED, &"x".repeat(65)))
                .unwrap_err()
                .starts_with("CAPABILITY_REQUEST_ID_INVALID")
        );
        assert!(write_request(
            &dir,
            &NewRequest {
                owner_epoch: 0,
                ..new(ECON_SIGN, ENABLED, "id-1")
            }
        )
        .unwrap_err()
        .starts_with("CAPABILITY_EPOCH_MISSING"));
        assert_eq!(head(&dir).unwrap(), 0);
        let first = write_request(
            &dir,
            &NewRequest {
                expected: Some(0),
                ..new(ECON_SIGN, ENABLED, "id-1")
            },
        )
        .unwrap();
        assert_eq!(
            first,
            Written {
                seq: 1,
                replayed: false
            }
        );
        // The retry of the same action (its CAS is stale by now): the same request.
        assert_eq!(
            write_request(
                &dir,
                &NewRequest {
                    expected: Some(0),
                    ..new(ECON_SIGN, ENABLED, "id-1")
                }
            )
            .unwrap(),
            Written {
                seq: 1,
                replayed: true
            }
        );
        for other in [
            new(ECON_SIGN, DISABLED, "id-1"),
            new(ECON_ARM, ENABLED, "id-1"),
        ] {
            assert!(write_request(&dir, &other)
                .unwrap_err()
                .starts_with("CAPABILITY_REQUEST_ID_CONFLICT"));
        }
        assert_eq!(head(&dir).unwrap(), 1);
        let r = request_at(&dir, 1).unwrap();
        assert_eq!((r.request_id.as_str(), r.owner_epoch), ("id-1", 1));
        let id = new_request_id().unwrap();
        assert_eq!(id.len(), 32);
        assert_ne!(id, new_request_id().unwrap());
        assert_eq!(validate_request_id(&id), Ok(()));
        // A symlinked request file or directory is refused, never read.
        let target = dir.join("request_00000001.p0j");
        std::os::unix::fs::symlink(&target, dir.join("request_00000002.p0j")).unwrap();
        assert!(request_at(&dir, 2)
            .unwrap_err()
            .starts_with("CAPABILITY_CONTROL_UNSAFE"));
        let link = temp("ids-link");
        std::os::unix::fs::symlink(&dir, &link).unwrap();
        assert!(head(&link)
            .unwrap_err()
            .starts_with("CAPABILITY_CONTROL_UNSAFE"));
        let _ = std::fs::remove_file(link);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// P8-CTRL.6: every owner start opens the next epoch, never an earlier one; the receipts of
    /// an epoch are its decisions — per capability the newest applied one.
    #[test]
    fn p8ctrl01_epochs_are_fresh_and_receipts_are_the_decisions() {
        let dir = temp("epochs");
        assert_eq!(current_epoch(&dir).unwrap(), 0);
        assert_eq!(open_epoch(&dir, 7, Some(1)).unwrap(), 1);
        assert_eq!(open_epoch(&dir, 8, None).unwrap(), 2);
        assert_eq!(current_epoch(&dir).unwrap(), 2);
        let receipt =
            |seq, capability: &str, value: &str, applied, epoch| CapabilityControlOutcome {
                seq,
                request: shown(capability, value),
                applied,
                requested: value.into(),
                effective: value.into(),
                detail: String::new(),
                owner_pid: 8,
                decided_unix_s: None,
                capability: capability.into(),
                block_reason: String::new(),
                request_id: format!("id-{seq}"),
                owner_epoch: epoch,
            };
        for o in [
            receipt(1, ECON_SIGN, ENABLED, true, 1),
            receipt(2, ECON_SIGN, ENABLED, true, 2),
            receipt(3, ECON_ARM, ENABLED, true, 2),
            receipt(4, ECON_SIGN, DISABLED, true, 2),
            receipt(5, ECON_ARM, DISABLED, false, 2),
        ] {
            write_outcome(&dir, &o).unwrap();
        }
        let now: Vec<(String, String, u64)> = epoch_decisions(&dir, 2)
            .unwrap()
            .into_iter()
            .map(|o| (o.capability, o.requested, o.seq))
            .collect();
        assert_eq!(
            now,
            vec![
                (ECON_ARM.into(), ENABLED.into(), 3),
                (ECON_SIGN.into(), DISABLED.into(), 4)
            ]
        );
        assert_eq!(epoch_decisions(&dir, 3).unwrap(), vec![]);
        assert_eq!(outcome_at(&dir, 9).unwrap(), None);
        assert_eq!(latest_outcome(&dir).unwrap().unwrap().seq, 5);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// P8-CTRL.5 (review M1): an exit needs every gate but ECON_ATTEMPT — taking ATTEMPT back
    /// stops new liquidations, never the exit of an open position; ECON_LIVE stops both.
    #[test]
    fn p8ctrl01_exits_need_every_gate_but_attempt() {
        let fe06 = Fe06Authorization::offline_test();
        let mut econ = EconSwitches::default();
        ECON.iter().for_each(|e| {
            econ.set(e, true);
        });
        let mut w = WalletSwitch::configured(Pubkey::new_unique(), &[ROLE_FEE_PAYER]);
        w.enabled = true;
        let wallets = [w];
        fn at<'a>(
            econ: &'a EconSwitches,
            wallets: &'a [WalletSwitch],
            fe06: Option<&'a Fe06Authorization>,
        ) -> LiveGates<'a> {
            LiveGates {
                requested: LIVE,
                econ,
                wallets,
                fe06,
                now_unix_s: Some(0),
                epoch: None,
                running: None,
                source: None,
                stop: None,
            }
        }
        let gates = |econ| at(econ, &wallets, Some(&fe06));
        assert_eq!(exit_decision(&gates(&econ)).0, LIVE);
        let mut no_attempt = econ;
        no_attempt.set(ECON_ATTEMPT, false);
        assert_eq!(
            live_decision(&at(&no_attempt, &wallets, Some(&fe06))),
            ("BLOCKED", "ECON_ATTEMPT DISABLED".to_string())
        );
        assert_eq!(
            exit_decision(&at(&no_attempt, &wallets, Some(&fe06))),
            (LIVE, String::new())
        );
        for e in ECON_EXIT {
            let mut one = econ;
            one.set(e, false);
            assert_eq!(
                exit_decision(&at(&one, &wallets, Some(&fe06))),
                ("BLOCKED", format!("{e} DISABLED"))
            );
        }
        assert_eq!(
            exit_decision(&at(&econ, &wallets, None)).1,
            LIVE_BLOCK_REASON
        );
    }

    /// Review M3: the request and receipt frames DEC-13 wrote (schema 2) stay readable — without
    /// id and epoch (0: no owner admits them, no epoch's decision); the epoch record round-trips.
    #[test]
    fn p8ctrl01_schema_2_frames_stay_readable() {
        use crate::analytics::payload::{self, Payload};
        #[derive(serde::Serialize)]
        struct RequestV2 {
            seq: u64,
            capability: String,
            requested: String,
            requested_unix_s: u64,
        }
        #[derive(serde::Serialize)]
        struct OutcomeV2 {
            seq: u64,
            request: String,
            applied: bool,
            requested: String,
            effective: String,
            detail: String,
            owner_pid: u32,
            decided_unix_s: Option<u64>,
        }
        let r = payload::decode::<CapabilityControlRequest>(
            CapabilityControlRequest::KIND,
            2,
            &payload::encode_wire(&RequestV2 {
                seq: 4,
                capability: LIVE.into(),
                requested: LIVE.into(),
                requested_unix_s: 9,
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            (
                r.seq,
                r.capability.as_str(),
                r.requested_unix_s,
                r.request_id.as_str(),
                r.owner_epoch
            ),
            (4, LIVE, 9, "", 0)
        );
        let o = payload::decode::<CapabilityControlOutcome>(
            CapabilityControlOutcome::KIND,
            2,
            &payload::encode_wire(&OutcomeV2 {
                seq: 4,
                request: "LIVE LIVE".into(),
                applied: true,
                requested: LIVE.into(),
                effective: "BLOCKED".into(),
                detail: "d".into(),
                owner_pid: 7,
                decided_unix_s: Some(10),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            (
                o.seq,
                o.applied,
                o.effective.as_str(),
                o.owner_pid,
                o.capability.as_str(),
                o.owner_epoch
            ),
            (4, true, "BLOCKED", 7, "", 0)
        );
        assert!(payload::decode::<CapabilityControlRequest>(
            CapabilityControlRequest::KIND,
            1,
            &[]
        )
        .is_err());
        let e = CapabilityOwnerEpoch {
            epoch: 3,
            owner_pid: 5,
            opened_unix_s: None,
        };
        let bytes = payload::encode(&e).unwrap();
        assert_eq!(
            payload::decode::<CapabilityOwnerEpoch>(
                journal::kind::CAPABILITY_OWNER_EPOCH,
                CapabilityOwnerEpoch::SCHEMA,
                &bytes
            )
            .unwrap(),
            e
        );
        assert_eq!(
            payload::registered_schema(journal::kind::CAPABILITY_OWNER_EPOCH),
            Some(2)
        );
    }

    /// Review L4/L7: an epoch names the process that opened it; a same-id action of an earlier
    /// epoch is stale (not a conflict).
    #[test]
    fn p8ctrl01_epochs_name_their_owner_and_old_ids_are_stale() {
        let dir = temp("owner");
        assert_eq!(open_epoch(&dir, 41, None).unwrap(), 1);
        assert_eq!(epoch_owner(&dir, 1).unwrap(), 41);
        assert!(epoch_owner(&dir, 2).is_err());
        write_request(&dir, &new(ECON_SIGN, ENABLED, "id-1")).unwrap();
        let err = write_request(
            &dir,
            &NewRequest {
                owner_epoch: 2,
                ..new(ECON_SIGN, ENABLED, "id-1")
            },
        )
        .unwrap_err();
        assert!(err.starts_with("CAPABILITY_EPOCH_STALE"), "{err}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// P8-CTRL.3: a grant is in force only within its scope's validity and with a readable clock
    /// — expired, not yet valid or without a clock LIVE is BLOCKED, naming the grant; the grant
    /// capabilities take a scope digest.
    #[test]
    fn p8ctrl02_a_grant_is_in_force_only_within_its_validity() {
        use super::super::fe06;
        let signer = Pubkey::new_unique();
        let scope = fe06::parse(
            fe06::tests::text(&signer, &[]).as_bytes(),
            &fe06::tests::ceilings(),
        )
        .unwrap();
        let running = fe06::tests::running(&signer);
        let grant = Fe06Authorization::offline_test_of(scope.clone());
        assert_eq!(grant.digest(), fe06::digest(&scope).unwrap());
        assert_eq!(grant.scope(), Some(&scope));
        let mut econ = EconSwitches::default();
        ECON.iter().for_each(|e| {
            econ.set(e, true);
        });
        let mut w = WalletSwitch::configured(Pubkey::new_unique(), &[ROLE_FEE_PAYER]);
        w.enabled = true;
        let wallets = [w];
        let at = |now_unix_s| {
            live_decision(&LiveGates {
                requested: LIVE,
                econ: &econ,
                wallets: &wallets,
                fe06: Some(&grant),
                now_unix_s,
                epoch: None,
                running: Some(&running),
                source: None,
                stop: None,
            })
        };
        assert_eq!(at(Some(scope.not_before_unix_s)).0, LIVE);
        assert_eq!(at(Some(scope.expires_unix_s - 1)).0, LIVE);
        for now in [
            Some(scope.not_before_unix_s - 1),
            Some(scope.expires_unix_s),
            None,
        ] {
            let (effective, reason) = at(now);
            assert_eq!(effective, "BLOCKED");
            assert!(
                reason.contains(grant.digest()) && reason.contains("not in force"),
                "{reason}"
            );
        }
        let digest = "a".repeat(64);
        assert_eq!(validate(FE06_GRANT, &digest), Ok(()));
        assert_eq!(validate(FE06_REVOKE, &digest), Ok(()));
        for bad in ["", &"A".repeat(64), &"a".repeat(63), ENABLED] {
            assert!(validate(FE06_GRANT, bad)
                .unwrap_err()
                .starts_with("CAPABILITY_VALUE_INVALID"));
        }
    }

    /// P8-CTRL.7 (review M5/N2): the stops no real signature and no egress runs ahead of — a
    /// switch or a signer DISABLED, LIVE OFF, a revocation; never an enabling request, a grant or
    /// a mode; a stop waits from its read until the owner decided it.
    #[test]
    fn p8ctrl06_stops_are_counted_from_their_hand_over_to_their_decision() {
        let wallet = format!("{WALLET_PREFIX}{}", Pubkey::new_unique());
        for (capability, requested) in [
            (ECON_SEND, DISABLED),
            (ECON_LIVE, DISABLED),
            (wallet.as_str(), DISABLED),
            (LIVE, LIVE_OFF),
            (FE06_REVOKE, "d"),
        ] {
            assert!(is_stop(capability, requested), "{capability} {requested}");
        }
        for (capability, requested) in [
            (ECON_SEND, ENABLED),
            (wallet.as_str(), ENABLED),
            (LIVE, LIVE),
            (FE06_GRANT, "d"),
            ("HOLD_B", "OFF"),
        ] {
            assert!(!is_stop(capability, requested), "{capability} {requested}");
        }
        let s = StopSignal::default();
        assert!(!s.pending());
        s.queue();
        assert!(s.pending());
        s.decide();
        assert!(!s.pending());

        // A permit issued before a stop arrived is refused at its adapter call while it waits.
        let signer = Pubkey::new_unique();
        let offline = OfflineGates::enabled(signer);
        let stop = std::sync::Arc::new(StopSignal::default());
        let g = LiveGates {
            stop: Some(&stop),
            ..offline.gates()
        };
        let sigs = vec!["sig-1".to_string()];
        let use_ =
            super::super::fe06::Use::Attempt(super::super::fe06::tests::attempt_use(&signer));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let rpc = darkbot_solana::submit::SubmissionPath::Rpc;
        let permit = EgressPermit::issue(&g, &use_, &sigs, rpc, deadline).unwrap();
        stop.queue();
        assert_eq!(
            EgressPermit::issue(&g, &use_, &sigs, rpc, deadline).unwrap_err(),
            STOP_PENDING
        );
        // Review N2: no real signature either while it waits.
        let message = solana_sdk::message::VersionedMessage::Legacy(
            solana_sdk::message::Message::new(&[], Some(&signer)),
        );
        assert_eq!(
            SignPermit::issue(&g, SignPurpose::NewAttempt, &[signer], &message, None).unwrap_err(),
            STOP_PENDING
        );
        assert_eq!(
            permit.admit(Purpose::NewAttempt, &sigs, rpc, 0),
            Err(STOP_PENDING.into())
        );
    }

    /// P8-CTRL.7: an egress permit is issued only from a LIVE decision of its purpose with a grant
    /// whose scope takes the purpose and the adapter; it admits exactly its payload, purpose,
    /// path, deadline and grant validity — and only once (it is consumed).
    #[test]
    fn p8ctrl03_an_egress_permit_binds_its_payload_and_is_used_once() {
        use super::super::fe06;
        use darkbot_solana::submit::SubmissionPath;
        let signer = Pubkey::new_unique();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let sigs = vec!["sig-1".to_string()];
        let offline = OfflineGates::enabled(signer);
        // P8-CTRL.3: a use within the default test scope of each purpose.
        let use_of = |p: Purpose| match p {
            Purpose::NewAttempt => fe06::Use::Attempt(fe06::tests::attempt_use(&signer)),
            Purpose::Exit => fe06::Use::Exit(fe06::tests::exit_use(&signer)),
        };
        let issue = |g: &LiveGates, purpose, path| {
            EgressPermit::issue(g, &use_of(purpose), &sigs, path, deadline)
        };
        // Every gate: the permit of this payload.
        let p = issue(&offline.gates(), Purpose::NewAttempt, SubmissionPath::Rpc).unwrap();
        assert_eq!((p.grant(), p.epoch()), ("offline-test", None));
        assert_eq!(
            p.admit(Purpose::NewAttempt, &sigs, SubmissionPath::Rpc, 0),
            Ok(())
        );
        // LIVE not requested, a switch DISABLED, no grant: none.
        let off = LiveGates {
            requested: LIVE_OFF,
            ..offline.gates()
        };
        assert!(issue(&off, Purpose::NewAttempt, SubmissionPath::Rpc)
            .unwrap_err()
            .starts_with("EGRESS_NOT_PERMITTED: LIVE OFF"));
        let mut send_off = offline.econ;
        send_off.set(ECON_SEND, false);
        let g = LiveGates {
            econ: &send_off,
            ..offline.gates()
        };
        assert_eq!(
            issue(&g, Purpose::NewAttempt, SubmissionPath::Rpc).unwrap_err(),
            "EGRESS_NOT_PERMITTED: LIVE BLOCKED · ECON_SEND DISABLED"
        );
        let mut attempt_off = offline.econ;
        attempt_off.set(ECON_ATTEMPT, false);
        let g = LiveGates {
            econ: &attempt_off,
            ..offline.gates()
        };
        assert!(issue(&g, Purpose::NewAttempt, SubmissionPath::Rpc).is_err());
        assert!(
            issue(&g, Purpose::Exit, SubmissionPath::Rpc).is_ok(),
            "exits need no ATTEMPT"
        );
        let g = LiveGates {
            fe06: None,
            ..offline.gates()
        };
        assert!(issue(&g, Purpose::Exit, SubmissionPath::Rpc).is_err());
        // The scope's mode and adapters bound it.
        let scope = |overrides: &[(&str, &str)]| {
            fe06::parse(
                fe06::tests::text(&signer, overrides).as_bytes(),
                &fe06::tests::ceilings(),
            )
            .unwrap()
        };
        let all = Fe06Authorization::offline_test_of(scope(&[]));
        fn with<'a>(
            offline: &'a OfflineGates,
            a: &'a Fe06Authorization,
            now: u64,
        ) -> LiveGates<'a> {
            LiveGates {
                fe06: Some(a),
                now_unix_s: Some(now),
                ..offline.gates()
            }
        }
        // P8-CTRL.3: a scope that does not bind what runs, or an unknown identity: no permit.
        let mut elsewhere = offline.running.clone();
        elsewhere.binary_sha256 = Some("a".repeat(64));
        let g = LiveGates {
            running: Some(&elsewhere),
            ..with(&offline, &all, 1000)
        };
        assert!(issue(&g, Purpose::NewAttempt, SubmissionPath::Rpc)
            .unwrap_err()
            .starts_with(
                "EGRESS_NOT_PERMITTED: LIVE BLOCKED · FE06_SCOPE_BINDING: binary_sha256 differs"
            ));
        let g = LiveGates {
            running: None,
            ..with(&offline, &all, 1000)
        };
        assert_eq!(
            issue(&g, Purpose::NewAttempt, SubmissionPath::Rpc).unwrap_err(),
            "EGRESS_NOT_PERMITTED: LIVE BLOCKED · FE06_SCOPE_BINDING: what the owner runs is unknown"
        );
        let sim = Fe06Authorization::offline_test_of(scope(&[
            ("mode", "SIM_ONLY"),
            ("max_attempts_per_utc_day", "0"),
            ("max_attempts_campaign", "0"),
            ("max_attempts_grant", "0"),
            ("max_exits_grant", "0"),
        ]));
        let exit_only = Fe06Authorization::offline_test_of(scope(&[
            ("mode", "EXIT_ONLY"),
            (
                "exit_positions",
                "attempt-1:11111111111111111111111111111112",
            ),
            ("max_attempts_per_utc_day", "0"),
            ("max_attempts_campaign", "0"),
            ("max_attempts_grant", "0"),
        ]));
        assert!(issue(
            &with(&offline, &all, 1000),
            Purpose::NewAttempt,
            SubmissionPath::Rpc
        )
        .is_ok());
        assert!(issue(
            &with(&offline, &all, 1000),
            Purpose::NewAttempt,
            SubmissionPath::JitoBundle
        )
        .unwrap_err()
        .starts_with("EGRESS_ADAPTER_NOT_IN_SCOPE"));
        // Review M3: a scope that takes no such use keeps LIVE BLOCKED for it, naming the grant.
        for purpose in [Purpose::NewAttempt, Purpose::Exit] {
            let e = issue(&with(&offline, &sim, 1000), purpose, SubmissionPath::Rpc).unwrap_err();
            assert!(
                e.starts_with("EGRESS_NOT_PERMITTED: LIVE BLOCKED")
                    && e.contains(&format!(
                        "{} is SIM_ONLY: it takes no {purpose:?}",
                        sim.digest()
                    )),
                "{e}"
            );
        }
        let e = issue(
            &with(&offline, &exit_only, 1000),
            Purpose::NewAttempt,
            SubmissionPath::Rpc,
        )
        .unwrap_err();
        assert!(e.contains("is EXIT_ONLY: it takes no NewAttempt"), "{e}");
        assert!(issue(
            &with(&offline, &exit_only, 1000),
            Purpose::Exit,
            SubmissionPath::Rpc
        )
        .is_ok());
        // At the handoff: another payload, purpose or path, a passed deadline, an expired grant.
        let p = || {
            issue(
                &with(&offline, &all, 1000),
                Purpose::NewAttempt,
                SubmissionPath::Rpc,
            )
            .unwrap()
        };
        let other = vec!["sig-2".to_string()];
        for refused in [
            p().admit(Purpose::NewAttempt, &other, SubmissionPath::Rpc, 1000),
            p().admit(Purpose::Exit, &sigs, SubmissionPath::Rpc, 1000),
            p().admit(Purpose::NewAttempt, &sigs, SubmissionPath::JitoSingle, 1000),
        ] {
            assert_eq!(
                refused.unwrap_err(),
                "EGRESS_PERMIT_PAYLOAD: the permit is another payload's"
            );
        }
        assert_eq!(
            p().admit(Purpose::NewAttempt, &sigs, SubmissionPath::Rpc, 4600),
            Err("EGRESS_PERMIT_GRANT_EXPIRED".into())
        );
        let past = EgressPermit::issue(
            &with(&offline, &all, 1000),
            &use_of(Purpose::NewAttempt),
            &sigs,
            SubmissionPath::Rpc,
            std::time::Instant::now(),
        )
        .unwrap();
        assert_eq!(
            past.admit(Purpose::NewAttempt, &sigs, SubmissionPath::Rpc, 1000),
            Err("EGRESS_PERMIT_DEADLINE".into())
        );
        assert!(issue(&offline.gates(), Purpose::NewAttempt, SubmissionPath::Rpc).is_ok());
        assert!(EgressPermit::issue(
            &offline.gates(),
            &use_of(Purpose::NewAttempt),
            &[],
            SubmissionPath::Rpc,
            deadline
        )
        .unwrap_err()
        .starts_with("EGRESS_PERMIT_PAYLOAD"));
    }

    /// P8-CTRL.4/.7: a real signature only under a sign permit of its purpose — a new attempt or
    /// an exit while LIVE is effective for it (an EXIT_ONLY scope signs no new attempt), a
    /// simulation only under a SIM_ONLY grant with ECON_SIGN and the signer ENABLED; the permit
    /// signs exactly its message with exactly its signers. Without one a simulation is signed
    /// synthetically: the same wire bytes, never the wallet's key.
    #[test]
    fn p8ctrl03_a_real_signature_needs_its_sign_permit() {
        use super::super::fe06;
        use solana_sdk::signature::{Keypair, Signer};
        let payer = Keypair::new();
        let message = |n: u64| {
            solana_sdk::message::VersionedMessage::Legacy(solana_sdk::message::Message::new(
                &[solana_sdk::instruction::Instruction::new_with_bytes(
                    Pubkey::new_from_array([7; 32]),
                    &n.to_le_bytes(),
                    vec![solana_sdk::instruction::AccountMeta::new(
                        payer.pubkey(),
                        true,
                    )],
                )],
                Some(&payer.pubkey()),
            ))
        };
        let unsigned = |m: solana_sdk::message::VersionedMessage| {
            solana_sdk::transaction::VersionedTransaction {
                signatures: vec![Default::default(); 1],
                message: m,
            }
        };
        let offline = OfflineGates::enabled(payer.pubkey());
        let pk = [payer.pubkey()];
        // A new attempt under every gate: exactly this message, exactly this signer.
        let mut tx = unsigned(message(1));
        SignPermit::issue(
            &offline.gates(),
            SignPurpose::NewAttempt,
            &pk,
            &tx.message,
            None,
        )
        .unwrap()
        .sign(&mut tx, &[&payer])
        .unwrap();
        assert!(tx.verify_and_hash_message().is_ok());
        let p = SignPermit::issue(
            &offline.gates(),
            SignPurpose::NewAttempt,
            &pk,
            &message(1),
            None,
        )
        .unwrap();
        let mut other = unsigned(message(2));
        assert!(p
            .sign(&mut other, &[&payer])
            .unwrap_err()
            .starts_with("SIGN_PERMIT_MESSAGE"));
        let p = SignPermit::issue(
            &offline.gates(),
            SignPurpose::NewAttempt,
            &pk,
            &message(1),
            None,
        )
        .unwrap();
        let mut same = unsigned(message(1));
        assert!(p
            .sign(&mut same, &[&Keypair::new()])
            .unwrap_err()
            .starts_with("SIGN_PERMIT_SIGNERS"));
        // Not LIVE, a signer DISABLED: none.
        let off = LiveGates {
            requested: LIVE_OFF,
            ..offline.gates()
        };
        assert!(
            SignPermit::issue(&off, SignPurpose::NewAttempt, &pk, &message(1), None)
                .unwrap_err()
                .starts_with("SIGN_NOT_PERMITTED")
        );
        assert!(SignPermit::issue(
            &offline.gates(),
            SignPurpose::NewAttempt,
            &[Pubkey::new_unique()],
            &message(1),
            None
        )
        .unwrap_err()
        .starts_with("SIGN_NOT_PERMITTED: WALLET:"));
        // A simulation: never by a LIVE grant alone, only by a SIM_ONLY one in force.
        assert!(SignPermit::issue(
            &offline.gates(),
            SignPurpose::Simulation,
            &pk,
            &message(1),
            None
        )
        .unwrap_err()
        .starts_with("SIGN_NOT_PERMITTED: no SIM_ONLY grant"));
        let now = 2_000;
        let sim_scope = fe06::parse(
            fe06::tests::text(
                &payer.pubkey(),
                &[
                    ("mode", "SIM_ONLY"),
                    ("max_attempts_per_utc_day", "0"),
                    ("max_attempts_campaign", "0"),
                    ("max_attempts_grant", "0"),
                    ("max_exits_grant", "0"),
                ],
            )
            .as_bytes(),
            &fe06::tests::ceilings(),
        )
        .unwrap();
        let sim = Fe06Authorization::offline_test_of(sim_scope);
        let sim_use = fe06::Use::Attempt(fe06::tests::attempt_use(&payer.pubkey()));
        let rpc = darkbot_solana::submit::SubmissionPath::Rpc;
        let with_sim = LiveGates {
            fe06: Some(&sim),
            now_unix_s: Some(now),
            ..offline.gates()
        };
        assert_eq!(
            SignPermit::issue(
                &with_sim,
                SignPurpose::Simulation,
                &pk,
                &message(1),
                Some((&sim_use, rpc))
            )
            .unwrap()
            .purpose(),
            SignPurpose::Simulation
        );
        let mut sign_off = offline.econ;
        sign_off.set(ECON_SIGN, false);
        let g = LiveGates {
            econ: &sign_off,
            ..with_sim
        };
        assert!(SignPermit::issue(
            &g,
            SignPurpose::Simulation,
            &pk,
            &message(1),
            Some((&sim_use, rpc))
        )
        .unwrap_err()
        .starts_with("SIGN_NOT_PERMITTED: ECON_SIGN"));
        // A SIM_ONLY grant signs no attempt and no exit.
        for purpose in [SignPurpose::NewAttempt, SignPurpose::Exit] {
            assert!(
                SignPermit::issue(&with_sim, purpose, &pk, &message(1), Some((&sim_use, rpc)))
                    .is_err()
            );
        }
        // Review H1: under a grant with a scope the whole scope check runs before the wallet
        // signs — its use, its adapter, what runs; an unbound signature is refused.
        let all_scope = fe06::parse(
            fe06::tests::text(&payer.pubkey(), &[]).as_bytes(),
            &fe06::tests::ceilings(),
        )
        .unwrap();
        let all = Fe06Authorization::offline_test_of(all_scope);
        let with_all = LiveGates {
            fe06: Some(&all),
            now_unix_s: Some(now),
            ..offline.gates()
        };
        let attempt = fe06::tests::attempt_use(&payer.pubkey());
        let sign = |use_: &fe06::AttemptUse, path| {
            SignPermit::issue(
                &with_all,
                SignPurpose::NewAttempt,
                &pk,
                &message(1),
                Some((&fe06::Use::Attempt(use_.clone()), path)),
            )
        };
        assert!(sign(&attempt, rpc).is_ok());
        let costly = fe06::AttemptUse {
            fee_lamports: Some(100_001),
            ..attempt.clone()
        };
        assert!(sign(&costly, rpc)
            .unwrap_err()
            .starts_with("FE06_SCOPE_USE: fee_lamports 100001 "));
        assert!(
            sign(&attempt, darkbot_solana::submit::SubmissionPath::JitoSingle)
                .unwrap_err()
                .starts_with("SIGN_ADAPTER_NOT_IN_SCOPE: jito_single")
        );
        assert_eq!(
            SignPermit::issue(&with_all, SignPurpose::NewAttempt, &pk, &message(1), None)
                .unwrap_err(),
            "SIGN_SCOPE_USE: the signature's use is unknown"
        );
        let elsewhere = LiveGates {
            running: None,
            ..with_all
        };
        assert!(SignPermit::issue(
            &elsewhere,
            SignPurpose::NewAttempt,
            &pk,
            &message(1),
            Some((&fe06::Use::Attempt(attempt.clone()), rpc))
        )
        .unwrap_err()
        .starts_with(
            "SIGN_NOT_PERMITTED: LIVE BLOCKED · FE06_SCOPE_BINDING: what the owner runs is unknown"
        ));
        // The synthetic signature: the same wire length, filled, never verifying.
        let mut real = unsigned(message(1));
        let mut synthetic = unsigned(message(1));
        darkbot_solana::tx::sign_transaction(&mut real, &[&payer]).unwrap();
        darkbot_solana::tx::sign_synthetic(&mut synthetic, &Keypair::new()).unwrap();
        assert_eq!(
            bincode::serialize(&real).unwrap().len(),
            bincode::serialize(&synthetic).unwrap().len()
        );
        assert!(synthetic.signatures[0] != Default::default());
        assert!(synthetic.verify_and_hash_message().is_err());
    }
}

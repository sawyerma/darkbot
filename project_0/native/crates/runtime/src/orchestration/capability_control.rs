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
/// (DEC-04), the bundle path (DEC-11), the threshold mode (DEC-06). The economic switches
/// (`ECON`) and the wallets (`WALLET:<pubkey>`) take `SWITCH_VALUES`.
pub const CAPABILITIES: [(&str, &[&str]); 4] = [
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
    let values: &[&str] = match wallet_of(capability) {
        Some(Err(e)) => return Err(e),
        Some(Ok(_)) => &SWITCH_VALUES,
        None if ECON.contains(&capability) => &SWITCH_VALUES,
        None => match CAPABILITIES.iter().find(|(c, _)| *c == capability) {
            Some((_, values)) => values,
            None => {
                return Err(format!(
                    "CAPABILITY_UNKNOWN: {} (LIVE|HOLD_B|JITO_BUNDLE|THRESHOLD_TRIGGER|ECON_SIGN|ECON_ARM|ECON_SEND|ECON_ATTEMPT|ECON_LIVE|WALLET:<pubkey>)",
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
    for _ in 0..16 {
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
    Err("CAPABILITY_EPOCH_CONTENDED: no epoch number could be taken".into())
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
        return if (
            prior.capability.as_str(),
            prior.requested.as_str(),
            prior.owner_epoch,
        ) == (r.capability, r.requested, r.owner_epoch)
        {
            Ok(Written {
                seq: prior.seq,
                replayed: true,
            })
        } else {
            Err(format!(
                "CAPABILITY_REQUEST_ID_CONFLICT: request id {} is seq {} with other content",
                r.request_id, prior.seq
            ))
        };
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
    journal::create_typed_value(
        &request_path(dir, seq),
        journal::source::CAPABILITY_CONTROL,
        &request,
    )
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            conflict(head(dir).unwrap_or(seq))
        } else {
            format!("CAPABILITY_CONTROL_IO: {e}")
        }
    })?;
    Ok(Written {
        seq,
        replayed: false,
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
    _sealed: (),
}

impl Fe06Authorization {
    /// The offline tests' stand-in of an FE-06 authorization (never part of a build).
    #[cfg(test)]
    pub(crate) fn offline_test() -> Self {
        Self { _sealed: () }
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
}

/// P8-CTRL.2: what LIVE's effective value rests on at one instant.
#[derive(Clone, Copy, Debug)]
pub struct LiveGates<'a> {
    pub requested: &'a str,
    pub econ: &'a EconSwitches,
    /// Every signer the economic path needs.
    pub wallets: &'a [WalletSwitch],
    pub fe06: Option<&'a Fe06Authorization>,
}

/// P8-CTRL.2: LIVE's effective value — BLOCKED while any gate is missing (every economic switch,
/// every required wallet, the FE-06 authorization), whatever is requested, with the first missing
/// gate as its reason and every other one named after it; else the requested value.
pub fn live_decision(g: &LiveGates) -> (&'static str, String) {
    let mut missing: Vec<String> = ECON
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
    if g.fe06.is_none() {
        missing.push(LIVE_BLOCK_REASON.into());
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
                fe06: Some(&fe06)
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
            NewRequest {
                owner_epoch: 2,
                ..new(ECON_SIGN, ENABLED, "id-1")
            },
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
}

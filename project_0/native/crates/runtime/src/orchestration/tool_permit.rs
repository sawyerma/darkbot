//! DEC-17f (P8-CTRL-03d): the operator tools (`p0-prewarm-atas`, `p0-probe-costs`,
//! `p0-probe-v1`) sign and send under the owner's own permits — no second path. A tool's real
//! signature needs, in the epoch of the running owner, its durable receipts `WALLET:<signer>` and
//! `ECON_SIGN` ENABLED (a send also `ECON_SEND`), no stop waiting in the request log, its cost
//! within the tool's typed limit ([`Tool::cost_limit`]) and the operator's confirmation: the
//! operator key over the tool, the epoch with its nonce, the signers, the exact messages, whether
//! one is sent, its cost and limit ([`issue`]). No ECON_ATTEMPT, no ECON_ARM: a tool makes no
//! liquidation attempt. The gates hold again at every signature and immediately before the send.
//! Each permitted message is recorded once (`data/operator-tools/`) and never permitted again.
//! Without a permit a tool signs nothing (a dry run simulates unsigned).
use super::capability_control as cap;
use crate::analytics::journal;
use crate::analytics::payload::Payload;
use serde::{Deserialize, Serialize};
use solana_sdk::hash::Hash;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signature, Signer};
use std::path::{Path, PathBuf};

/// The store of the permitted tool uses (under a project; git-ignored, P0-026).
pub const STORE: &str = "data/operator-tools";
/// The domain of the operator's confirmation of a tool use (never a capability request's).
const DOMAIN: &[u8] = b"darkbot.p0.operator.tool.v1\0";
/// Why a tool may not sign or send.
pub const TOOL_NOT_PERMITTED: &str = "TOOL_NOT_PERMITTED";

/// An operator tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    PrewarmAtas,
    ProbeCosts,
    ProbeV1,
}

/// A tool's typed cost limit: the lamports its signed messages may cost — the prewarm's fee and
/// rent when it sends them; a probe's fee exposure (it never sends, but its signed messages reach
/// the provider for simulation, and one that landed would cost its fee). 0: none configured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolCost {
    pub max_lamports: u64,
    /// The tool sends (else it only simulates).
    pub sends: bool,
    /// The parameter that sets it.
    pub parameter: &'static str,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::PrewarmAtas => "p0-prewarm-atas",
            Tool::ProbeCosts => "p0-probe-costs",
            Tool::ProbeV1 => "p0-probe-v1",
        }
    }

    /// The tool's cost limit under `limits`: the prewarm sends within `prewarm_max_lamports`, a
    /// probe never sends and exposes at most `probe_max_lamports`.
    pub fn cost_limit(self, limits: &darkbot_solana::runtime_config::Limits) -> ToolCost {
        match self {
            Tool::PrewarmAtas => ToolCost {
                max_lamports: limits.prewarm_max_lamports,
                sends: true,
                parameter: "prewarm_max_lamports",
            },
            Tool::ProbeCosts | Tool::ProbeV1 => ToolCost {
                max_lamports: limits.probe_max_lamports,
                sends: false,
                parameter: "probe_max_lamports",
            },
        }
    }
}

/// What a tool asks to sign: its messages (each one's exact bytes) by its signers, one of them
/// sent or none, at its cost.
#[derive(Debug)]
pub struct ToolUse<'a> {
    pub tool: Tool,
    pub signers: &'a [Pubkey],
    pub messages: &'a [&'a [u8]],
    pub send: bool,
    pub cost_lamports: u64,
}

/// The record of one permitted tool use: what the operator confirmed, in which owner epoch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorToolUse {
    pub tool: String,
    pub owner_epoch: u64,
    pub owner_pid: u32,
    pub signers: Vec<String>,
    /// SHA-256 (hex) of each message, in the tool's order.
    pub messages_sha256: Vec<String>,
    pub send: bool,
    pub cost_lamports: u64,
    /// The tool's cost limit.
    pub cost_limit_lamports: u64,
    pub permitted_unix_s: u64,
    /// The operator's confirmation ([`statement`]).
    pub operator_signature: Vec<u8>,
}
impl Payload for OperatorToolUse {
    const KIND: u16 = journal::kind::OPERATOR_TOOL_USE;
}

/// The statement the operator confirms: the record without its signature, with the epoch's
/// nonce, under the tool domain.
fn statement(r: &OperatorToolUse, epoch_nonce: &str) -> Vec<u8> {
    let fields = (
        r.tool.as_str(),
        r.owner_epoch,
        epoch_nonce,
        &r.signers,
        &r.messages_sha256,
        r.send,
        r.cost_lamports,
        r.cost_limit_lamports,
        r.permitted_unix_s,
    );
    let mut out = DOMAIN.to_vec();
    out.extend(bincode::serialize(&fields).expect("statement fields serialize"));
    out
}

fn sha(bytes: &[u8]) -> String {
    journal::hex(&solana_sdk::hash::hash(bytes).to_bytes())
}

/// The running owner's epoch and pid, and the switches a use of `signers` (sent or not) needs in
/// it — from its durable receipts —, with no stop waiting for the owner.
fn gates(project: &Path, signers: &[Pubkey], send: bool) -> Result<(u64, u32), String> {
    use super::control::{self, OwnerState};
    let OwnerState::Running { pid, .. } = control::owner_state(&control::lock_path(project))?
    else {
        return Err(format!(
            "{TOOL_NOT_PERMITTED}: no runtime owner runs — every switch is DISABLED"
        ));
    };
    let dir = cap::request_dir(project);
    let epoch = cap::current_epoch(&dir)?;
    if epoch == 0 || cap::epoch_owner(&dir, epoch)? != pid {
        return Err(format!(
            "{TOOL_NOT_PERMITTED}: capability epoch {epoch} is not the running owner's (pid {pid})"
        ));
    }
    // The receipts' end first, then the decisions: a decision written in between is a request
    // after that end — seen below as a stop waiting (review M1).
    let decided_up_to = cap::latest_outcome(&dir)?.map_or(0, |o| o.seq);
    let decided = cap::epoch_decisions(&dir, epoch)?;
    let enabled = |c: &str| {
        decided
            .iter()
            .any(|o| o.capability == c && o.effective == cap::ENABLED)
    };
    let mut needed = vec![cap::ECON_SIGN.to_string()];
    if send {
        needed.push(cap::ECON_SEND.into());
    }
    needed.extend(signers.iter().map(|s| format!("{}{s}", cap::WALLET_PREFIX)));
    let missing: Vec<String> = needed
        .iter()
        .filter(|c| !enabled(c))
        .map(|c| format!("{c} DISABLED"))
        .collect();
    if !missing.is_empty() {
        return Err(format!("{TOOL_NOT_PERMITTED}: {}", missing.join("; ")));
    }
    // A stop waiting for the owner — nothing is signed or sent ahead of it: a request after the
    // receipts that is a stop (or unreadable: it may be one), or a stop of this epoch without its
    // receipt (one the owner applied but could not record stands applied, review L3).
    for seq in cap::requests_after(&dir, 0)? {
        let waits = match cap::request_at(&dir, seq) {
            Ok(r) if cap::is_stop(&r.capability, &r.requested) => {
                seq > decided_up_to
                    || (r.owner_epoch == epoch && cap::outcome_at(&dir, seq)?.is_none())
            }
            Ok(_) => false,
            Err(_) => seq > decided_up_to,
        };
        if waits {
            return Err(cap::STOP_PENDING.into());
        }
    }
    Ok((epoch, pid))
}

/// The one-use permission of a tool's real signatures of exactly its messages by exactly its
/// signers (its fields are private: only [`issue`] makes one), and of the send of one of them
/// where the use was a send.
#[derive(Debug)]
pub struct ToolPermit {
    project: PathBuf,
    epoch: u64,
    owner_pid: u32,
    signers: Vec<Pubkey>,
    /// The messages not signed yet.
    unsigned: Vec<Hash>,
    send: bool,
}

/// The operator's signature of a tool's statement ([`operator_confirmation`]).
pub type Confirmation<'a> = dyn Fn(&[u8]) -> Result<Vec<u8>, String> + 'a;

/// The permit of `u` at `now_unix_s` under `config` (the tool's active configuration): its cost
/// within the tool's limit, the running owner's switches for it, then the operator's
/// confirmation (`confirm` signs the statement — the Secure Enclave asks for the operator's
/// presence), verified against the configured key, the switches checked again after it, the use
/// recorded once.
pub fn issue(
    project: &Path,
    config: &darkbot_solana::runtime_config::RuntimeConfig,
    u: &ToolUse,
    now_unix_s: u64,
    confirm: &Confirmation,
) -> Result<ToolPermit, String> {
    if u.signers.is_empty() || u.messages.is_empty() {
        return Err(format!("{TOOL_NOT_PERMITTED}: no signer or no message"));
    }
    let limit = u.tool.cost_limit(&config.limits);
    let name = u.tool.name();
    if u.send && !limit.sends {
        return Err(format!("TOOL_COST_LIMIT: {name} never sends"));
    }
    if limit.max_lamports == 0 {
        return Err(format!(
            "TOOL_COST_LIMIT: {name} has no cost limit configured ({})",
            limit.parameter
        ));
    }
    if u.cost_lamports > limit.max_lamports {
        return Err(format!(
            "TOOL_COST_LIMIT: {name} costs {} lamports, its limit is {} ({})",
            u.cost_lamports, limit.max_lamports, limit.parameter
        ));
    }
    let (epoch, owner_pid) = gates(project, u.signers, u.send)?;
    // The operator is asked only with the configured, registered key (review M2).
    super::operator_auth::configured_key(project, &config.operator_key_sha256)?;
    let store = project.join(STORE);
    let path_of = |sha: &str| store.join(format!("{}.p0j", &sha[..32]));
    let nonce = cap::epoch_nonce(&cap::request_dir(project), epoch)?;
    let mut record = OperatorToolUse {
        tool: u.tool.name().into(),
        owner_epoch: epoch,
        owner_pid,
        signers: u.signers.iter().map(Pubkey::to_string).collect(),
        messages_sha256: u.messages.iter().map(|m| sha(m)).collect(),
        send: u.send,
        cost_lamports: u.cost_lamports,
        cost_limit_lamports: limit.max_lamports,
        permitted_unix_s: now_unix_s,
        operator_signature: vec![],
    };
    if let Some(sha) = record
        .messages_sha256
        .iter()
        .find(|m| path_of(m).symlink_metadata().is_ok())
    {
        return Err(format!(
            "TOOL_USE_REPLAYED: message {} was permitted before",
            &sha[..16]
        ));
    }
    // What the operator confirms, shown before the confirmation (review L7).
    eprintln!(
        "[{name}] owner epoch {epoch} · signers {} · {} message(s) {} · {} · cost {} of limit {} lamports ({})",
        record.signers.join(","),
        record.messages_sha256.len(),
        record
            .messages_sha256
            .iter()
            .map(|m| &m[..16])
            .collect::<Vec<_>>()
            .join(","),
        if u.send { "SENT" } else { "simulated only" },
        u.cost_lamports,
        limit.max_lamports,
        limit.parameter
    );
    let text = statement(&record, &nonce);
    let signature = confirm(&text)?;
    super::operator_auth::verify_message(project, &config.operator_key_sha256, &text, &signature)?;
    // The confirmation waited for the operator: the switches once more, in the same epoch.
    if gates(project, u.signers, u.send)? != (epoch, owner_pid) {
        return Err(format!(
            "{TOOL_NOT_PERMITTED}: the owner epoch changed during the confirmation"
        ));
    }
    record.operator_signature = signature;
    std::fs::create_dir_all(&store).map_err(|e| format!("TOOL_PERMIT_STORE: {e}"))?;
    // One record per message, written once: a message is never permitted twice (review L6).
    for sha in &record.messages_sha256 {
        journal::create_typed_value(&path_of(sha), journal::source::CAPABILITY_CONTROL, &record)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::AlreadyExists => format!(
                    "TOOL_USE_REPLAYED: message {} was permitted before",
                    &sha[..16]
                ),
                _ => format!("TOOL_PERMIT_STORE: {e}"),
            })?;
    }
    Ok(ToolPermit {
        project: project.to_path_buf(),
        epoch,
        owner_pid,
        signers: u.signers.to_vec(),
        unsigned: u
            .messages
            .iter()
            .map(|m| solana_sdk::hash::hash(m))
            .collect(),
        send: u.send,
    })
}

impl ToolPermit {
    /// The real signatures of one permitted message (each once) by exactly the permitted signers,
    /// in their order — while the gates still hold in the same owner epoch (review L4).
    pub fn sign(&mut self, message: &[u8], keys: &[&Keypair]) -> Result<Vec<Signature>, String> {
        let hash = solana_sdk::hash::hash(message);
        let at = self
            .unsigned
            .iter()
            .position(|h| *h == hash)
            .ok_or("TOOL_PERMIT_MESSAGE: not a permitted message, or signed already")?;
        if keys.iter().map(|k| k.pubkey()).collect::<Vec<_>>() != self.signers {
            return Err("TOOL_PERMIT_SIGNERS: the permit is other signers'".into());
        }
        self.holds()?;
        self.unsigned.remove(at);
        Ok(keys.iter().map(|k| k.sign_message(message)).collect())
    }

    /// The check immediately before the send (it consumes the permit): a permitted send, the same
    /// owner epoch, its switches still ENABLED, no stop waiting.
    pub fn admit_send(self) -> Result<(), String> {
        if !self.send {
            return Err("TOOL_PERMIT_NO_SEND: the permit is for simulations only".into());
        }
        self.holds()
    }

    /// The gates of the permit's use, in its owner epoch.
    fn holds(&self) -> Result<(), String> {
        if gates(&self.project, &self.signers, self.send)? != (self.epoch, self.owner_pid) {
            return Err(format!(
                "{TOOL_NOT_PERMITTED}: the owner epoch changed since the confirmation"
            ));
        }
        Ok(())
    }
}

/// The operator's confirmation of a tool's statement (`p0ctl`'s rule: a Secure Enclave key is
/// confirmed in a terminal only — no GUI, no pipe; a test key needs no presence).
pub fn operator_confirmation(
    project: &Path,
    tool: Tool,
) -> impl Fn(&[u8]) -> Result<Vec<u8>, String> + '_ {
    move |text| {
        use super::operator_auth;
        if operator_auth::load(project)?.signer == operator_auth::SECURE_ENCLAVE
            && !std::io::IsTerminal::is_terminal(&std::io::stdin())
        {
            return Err("OPERATOR_CONFIRM_NEEDS_TERMINAL: the operator confirms in a terminal (no GUI, no pipe)".into());
        }
        eprintln!(
            "[{}] confirm this use as the operator (Touch ID or the login password)…",
            tool.name()
        );
        operator_auth::sign(project, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::payload::CapabilityControlOutcome;
    use darkbot_solana::runtime_config::{Limits, RuntimeConfig};

    fn project(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "p0-tool-{tag}-{}-{}",
            std::process::id(),
            Pubkey::new_unique()
        ));
        std::fs::create_dir_all(p.join("data")).unwrap();
        p
    }

    /// A request of `capability value` in `epoch` (not decided yet).
    fn request(p: &Path, epoch: u64, capability: &str, value: &str) -> (u64, String) {
        let id = cap::new_request_id().unwrap();
        let w = cap::write_request(
            &cap::request_dir(p),
            &cap::NewRequest {
                capability,
                requested: value,
                request_id: &id,
                owner_epoch: epoch,
                expected: None,
                now_unix_s: 1,
            },
        )
        .unwrap();
        (w.seq, id)
    }

    /// The owner's decision of `capability value` (its request and its receipt) in `epoch`.
    fn decide(p: &Path, epoch: u64, capability: &str, value: &str) {
        let (seq, id) = request(p, epoch, capability, value);
        receipt(p, epoch, (seq, id), capability, value);
    }

    /// The owner's receipt of request `seq` (`id`): `capability value` applied.
    fn receipt(p: &Path, epoch: u64, (seq, id): (u64, String), capability: &str, value: &str) {
        cap::write_outcome(
            &cap::request_dir(p),
            &CapabilityControlOutcome {
                seq,
                request: cap::shown(capability, value),
                applied: true,
                requested: value.into(),
                effective: value.into(),
                detail: String::new(),
                owner_pid: std::process::id(),
                decided_unix_s: None,
                capability: capability.into(),
                block_reason: String::new(),
                request_id: id,
                owner_epoch: epoch,
            },
        )
        .unwrap();
    }

    /// The running owner (its lock) and its epoch.
    fn owner(p: &Path) -> (super::super::control::OwnerLock, u64) {
        let lock = super::super::control::acquire_owner(
            &super::super::control::default_lock_path(p),
            "tool_test",
            "shadow",
        )
        .unwrap();
        let (epoch, _) =
            cap::open_epoch_with_nonce(&cap::request_dir(p), std::process::id(), None).unwrap();
        (lock, epoch)
    }

    fn config(operator_key_sha256: &str, prewarm: u64, probe: u64) -> RuntimeConfig {
        RuntimeConfig {
            operator_key_sha256: operator_key_sha256.into(),
            limits: Limits {
                prewarm_max_lamports: prewarm,
                probe_max_lamports: probe,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn probe<'a>(signers: &'a [Pubkey], messages: &'a [&'a [u8]], cost: u64) -> ToolUse<'a> {
        ToolUse {
            tool: Tool::ProbeCosts,
            signers,
            messages,
            send: false,
            cost_lamports: cost,
        }
    }

    /// Before any confirmation: a cost beyond the tool's limit (none configured, above it, a
    /// probe that would send), no owner, an owner without the switches, a stop waiting — decided
    /// meanwhile, not decided yet, or applied without its receipt — or no configured operator key
    /// permits nothing, and the operator is never asked.
    #[test]
    fn dec17f_a_tool_needs_the_owners_switches_and_its_cost_limit() {
        let p = project("gates");
        let payer = Keypair::new();
        let signers = [payer.pubkey()];
        let messages: [&[u8]; 1] = [b"m"];
        let never = |_: &[u8]| -> Result<Vec<u8>, String> { panic!("no confirmation") };
        let refused = |u: &ToolUse, c: &RuntimeConfig| issue(&p, c, u, 1, &never).unwrap_err();
        let limited = config("", 10, 10);
        // The cost limits.
        let e = refused(&probe(&signers, &messages, 0), &config("", 10, 0));
        assert!(
            e.contains("no cost limit configured (probe_max_lamports)"),
            "{e}"
        );
        let e = refused(&probe(&signers, &messages, 11), &limited);
        assert!(e.contains("its limit is 10 (probe_max_lamports)"), "{e}");
        let sent = ToolUse {
            send: true,
            ..probe(&signers, &messages, 1)
        };
        assert!(refused(&sent, &limited).contains("never sends"));
        let prewarm = |cost| ToolUse {
            tool: Tool::PrewarmAtas,
            send: true,
            cost_lamports: cost,
            ..probe(&signers, &messages, 0)
        };
        assert!(refused(&prewarm(1), &config("", 0, 10)).contains("(prewarm_max_lamports)"));
        assert!(refused(&prewarm(11), &limited).contains("its limit is 10"));
        // The owner and its switches.
        let e = refused(&probe(&signers, &messages, 1), &limited);
        assert!(e.contains("no runtime owner runs"), "{e}");
        let (_lock, epoch) = owner(&p);
        let e = refused(&probe(&signers, &messages, 1), &limited);
        assert!(
            e.contains("ECON_SIGN DISABLED")
                && e.contains(&format!("WALLET:{} DISABLED", signers[0])),
            "{e}"
        );
        decide(&p, epoch, cap::ECON_SIGN, cap::ENABLED);
        decide(&p, epoch, &format!("WALLET:{}", signers[0]), cap::ENABLED);
        assert!(refused(&prewarm(10), &limited).contains("ECON_SEND DISABLED"));
        // Every gate holds: without a configured operator key the operator is never asked.
        let e = refused(&probe(&signers, &messages, 1), &limited);
        assert!(e.contains("no operator key configured"), "{e}");
        // A decided DISABLE takes the switch back.
        decide(&p, epoch, cap::ECON_SIGN, cap::DISABLED);
        assert!(refused(&probe(&signers, &messages, 1), &limited).contains("ECON_SIGN DISABLED"));
        decide(&p, epoch, cap::ECON_SIGN, cap::ENABLED);
        // A stop not decided yet holds everything …
        let (stop, _) = request(&p, epoch, cap::ECON_SEND, cap::DISABLED);
        assert_eq!(
            refused(&probe(&signers, &messages, 1), &limited),
            cap::STOP_PENDING
        );
        // … also once later requests are decided: its receipt was never written.
        decide(&p, epoch, cap::ECON_ARM, cap::DISABLED);
        assert!(
            cap::latest_outcome(&cap::request_dir(&p))
                .unwrap()
                .unwrap()
                .seq
                > stop
        );
        assert_eq!(
            refused(&probe(&signers, &messages, 1), &limited),
            cap::STOP_PENDING
        );
        let _ = std::fs::remove_dir_all(&p);
    }

    /// DEC-17f, the operator's confirmation (the registered test key): a confirmed use signs
    /// exactly its messages once each, by exactly its signers, while the gates hold; each message
    /// is recorded once and never permitted again, in no list; a send is admitted while its
    /// switches hold and refused once a stop waits; a stop during the confirmation, a declined
    /// one or another key's permits nothing.
    #[cfg(feature = "operator-test-signer")]
    #[test]
    fn dec17f_a_confirmed_tool_use_signs_its_messages_once() {
        use super::super::operator_auth;
        let p = project("confirm");
        let payer = Keypair::new();
        let signers = [payer.pubkey()];
        let key = operator_auth::register(&p, true).unwrap();
        let config = config(&operator_auth::key_sha256(&key.public_key), 10, 10);
        let (_lock, epoch) = owner(&p);
        for c in [cap::ECON_SIGN, cap::ECON_SEND] {
            decide(&p, epoch, c, cap::ENABLED);
        }
        decide(&p, epoch, &format!("WALLET:{}", signers[0]), cap::ENABLED);
        let confirm = operator_confirmation(&p, Tool::ProbeCosts);
        let messages: [&[u8]; 2] = [b"one", b"two"];
        // Declined, or confirmed with another registered key: nothing permitted, nothing recorded.
        let declined = |_: &[u8]| -> Result<Vec<u8>, String> { Err("OPERATOR_DECLINED".into()) };
        assert_eq!(
            issue(&p, &config, &probe(&signers, &messages, 1), 1, &declined).unwrap_err(),
            "OPERATOR_DECLINED"
        );
        let other = project("other-key");
        operator_auth::register(&other, true).unwrap();
        let foreign = |t: &[u8]| operator_auth::sign(&other, t);
        assert!(
            issue(&p, &config, &probe(&signers, &messages, 1), 1, &foreign)
                .unwrap_err()
                .starts_with(operator_auth::OPERATOR_AUTH_REQUIRED)
        );
        // A stop requested while the operator confirms: refused after the confirmation.
        let during = std::cell::RefCell::new(None);
        let stopping = |t: &[u8]| {
            *during.borrow_mut() = Some(request(&p, epoch, cap::ECON_SIGN, cap::DISABLED));
            operator_auth::sign(&p, t)
        };
        assert_eq!(
            issue(&p, &config, &probe(&signers, &messages, 1), 1, &stopping).unwrap_err(),
            cap::STOP_PENDING
        );
        let stopped = during.take().unwrap();
        receipt(&p, epoch, stopped, cap::ECON_SIGN, cap::DISABLED);
        decide(&p, epoch, cap::ECON_SIGN, cap::ENABLED);
        assert!(!p.join(STORE).exists() || std::fs::read_dir(p.join(STORE)).unwrap().count() == 0);
        // Confirmed: each message once, by exactly the permitted signer; no send.
        let mut permit = issue(&p, &config, &probe(&signers, &messages, 1), 1, &confirm).unwrap();
        let intruder = Keypair::new();
        assert!(permit
            .sign(b"one", &[&intruder])
            .unwrap_err()
            .starts_with("TOOL_PERMIT_SIGNERS"));
        let s = permit.sign(b"one", &[&payer]).unwrap();
        assert!(s[0].verify(payer.pubkey().as_ref(), b"one"));
        for again in [b"one".as_slice(), b"three"] {
            assert!(permit
                .sign(again, &[&payer])
                .unwrap_err()
                .starts_with("TOOL_PERMIT_MESSAGE"));
        }
        // A stop waiting holds the next signature; once decided — a switch a tool never needs
        // (ECON_ARM) — it signs.
        let waiting = request(&p, epoch, cap::ECON_ARM, cap::DISABLED);
        assert_eq!(
            permit.sign(b"two", &[&payer]).unwrap_err(),
            cap::STOP_PENDING
        );
        receipt(&p, epoch, waiting, cap::ECON_ARM, cap::DISABLED);
        permit.sign(b"two", &[&payer]).unwrap();
        assert!(permit
            .admit_send()
            .unwrap_err()
            .starts_with("TOOL_PERMIT_NO_SEND"));
        // No message is permitted twice, in any list.
        let mixed: [&[u8]; 2] = [b"four", b"one"];
        assert!(issue(&p, &config, &probe(&signers, &mixed, 1), 2, &confirm)
            .unwrap_err()
            .starts_with("TOOL_USE_REPLAYED"));
        // A send: admitted while its switches hold …
        let send = |message: &'static [u8]| ToolUse {
            tool: Tool::PrewarmAtas,
            send: true,
            cost_lamports: 10,
            signers: &signers,
            messages: Box::leak(Box::new([message])),
        };
        let permit = issue(&p, &config, &send(b"prewarm-1"), 3, &confirm).unwrap();
        permit.admit_send().unwrap();
        let record: OperatorToolUse = journal::read_typed_value(
            &p.join(STORE)
                .join(format!("{}.p0j", &sha(b"prewarm-1")[..32])),
        )
        .unwrap();
        assert_eq!(
            (
                record.tool.as_str(),
                record.owner_epoch,
                record.send,
                record.cost_lamports,
                record.cost_limit_lamports
            ),
            ("p0-prewarm-atas", epoch, true, 10, 10)
        );
        // … refused once a stop waits.
        let permit = issue(&p, &config, &send(b"prewarm-2"), 4, &confirm).unwrap();
        request(&p, epoch, cap::ECON_SEND, cap::DISABLED);
        assert_eq!(permit.admit_send().unwrap_err(), cap::STOP_PENDING);
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::remove_dir_all(&other);
    }
}

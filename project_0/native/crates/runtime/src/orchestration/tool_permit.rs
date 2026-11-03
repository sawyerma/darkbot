//! DEC-17f (P8-CTRL-03d): the operator tools (`p0-prewarm-atas`, `p0-probe-costs`,
//! `p0-probe-v1`) sign and send under the owner's own permits — no second path. A tool's real
//! signature needs, in the epoch of the running owner, its durable receipts `WALLET:<signer>` and
//! `ECON_SIGN` ENABLED (a send also `ECON_SEND`), no stop waiting in the request log, the tool's
//! typed cost limit ([`Tool::cost_limit`]) and the operator's confirmation: the operator key over
//! the tool, the epoch with its nonce, the signers, the exact messages, whether one is sent and
//! its cost ([`issue`]). No ECON_ATTEMPT, no ECON_ARM: a tool makes no liquidation attempt. Each
//! permit is recorded once (`data/operator-tools/`); the same messages are never permitted
//! twice. Without a permit a tool signs nothing (a dry run simulates unsigned).
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

/// A tool's typed cost limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolCost {
    /// It never sends: it signs for simulations only, at no cost.
    NoSend,
    /// It sends for at most this many lamports, fee and rent (0: no ceiling configured — no send).
    Lamports(u64),
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
    /// probe never sends.
    pub fn cost_limit(self, limits: &darkbot_solana::runtime_config::Limits) -> ToolCost {
        match self {
            Tool::PrewarmAtas => ToolCost::Lamports(limits.prewarm_max_lamports),
            Tool::ProbeCosts | Tool::ProbeV1 => ToolCost::NoSend,
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
    /// The tool's cost limit (0 for a tool that never sends).
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
    // A request the owner has not decided yet that is a stop (or unreadable: it may be one) —
    // nothing is signed or sent ahead of it.
    let decided_up_to = cap::latest_outcome(&dir)?.map_or(0, |o| o.seq);
    for seq in cap::requests_after(&dir, decided_up_to)? {
        match cap::request_at(&dir, seq) {
            Ok(r) if !cap::is_stop(&r.capability, &r.requested) => {}
            _ => return Err(cap::STOP_PENDING.into()),
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
    let limit = match (u.tool.cost_limit(&config.limits), u.send) {
        (ToolCost::NoSend, true) => {
            return Err(format!("TOOL_COST_LIMIT: {} never sends", u.tool.name()))
        }
        (ToolCost::NoSend, false) => 0,
        (ToolCost::Lamports(0), true) => {
            return Err(format!(
                "TOOL_COST_LIMIT: {} has no cost ceiling configured (prewarm_max_lamports)",
                u.tool.name()
            ))
        }
        (ToolCost::Lamports(max), true) if u.cost_lamports > max => return Err(format!(
            "TOOL_COST_LIMIT: {} costs {} lamports, its ceiling is {max} (prewarm_max_lamports)",
            u.tool.name(),
            u.cost_lamports
        )),
        (ToolCost::Lamports(max), _) => max,
    };
    let (epoch, owner_pid) = gates(project, u.signers, u.send)?;
    let nonce = cap::epoch_nonce(&cap::request_dir(project), epoch)?;
    let mut record = OperatorToolUse {
        tool: u.tool.name().into(),
        owner_epoch: epoch,
        owner_pid,
        signers: u.signers.iter().map(Pubkey::to_string).collect(),
        messages_sha256: u.messages.iter().map(|m| sha(m)).collect(),
        send: u.send,
        cost_lamports: if u.send { u.cost_lamports } else { 0 },
        cost_limit_lamports: limit,
        permitted_unix_s: now_unix_s,
        operator_signature: vec![],
    };
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
    let store = project.join(STORE);
    std::fs::create_dir_all(&store).map_err(|e| format!("TOOL_PERMIT_STORE: {e}"))?;
    let digest = sha(record.messages_sha256.concat().as_bytes());
    journal::create_typed_value(
        &store.join(format!("{}.p0j", &digest[..32])),
        journal::source::CAPABILITY_CONTROL,
        &record,
    )
    .map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => {
            "TOOL_USE_REPLAYED: these messages were permitted before".to_string()
        }
        _ => format!("TOOL_PERMIT_STORE: {e}"),
    })?;
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
    /// in their order.
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
        self.unsigned.remove(at);
        Ok(keys.iter().map(|k| k.sign_message(message)).collect())
    }

    /// The check immediately before the send (it consumes the permit): a permitted send, the same
    /// owner epoch, its switches still ENABLED, no stop waiting.
    pub fn admit_send(self) -> Result<(), String> {
        if !self.send {
            return Err("TOOL_PERMIT_NO_SEND: the permit is for simulations only".into());
        }
        if gates(&self.project, &self.signers, true)? != (self.epoch, self.owner_pid) {
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

    fn project(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "p0-tool-{tag}-{}-{}",
            std::process::id(),
            Pubkey::new_unique()
        ));
        std::fs::create_dir_all(p.join("data")).unwrap();
        p
    }

    /// The owner's decision of `capability value` (its request and its receipt) in `epoch`.
    fn decide(p: &Path, epoch: u64, capability: &str, value: &str) {
        let dir = cap::request_dir(p);
        let id = cap::new_request_id().unwrap();
        let w = cap::write_request(
            &dir,
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
        cap::write_outcome(
            &dir,
            &CapabilityControlOutcome {
                seq: w.seq,
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

    fn probe<'a>(signers: &'a [Pubkey], messages: &'a [&'a [u8]]) -> ToolUse<'a> {
        ToolUse {
            tool: Tool::ProbeCosts,
            signers,
            messages,
            send: false,
            cost_lamports: 0,
        }
    }

    /// Before any confirmation: no owner, or an owner without the switches, permits nothing;
    /// a probe never sends; a send needs ECON_SEND and the tool's ceiling.
    #[test]
    fn dec17f_a_tool_needs_the_owners_switches_and_its_cost_limit() {
        let p = project("gates");
        let payer = Keypair::new();
        let signers = [payer.pubkey()];
        let messages: [&[u8]; 1] = [b"m"];
        let config = darkbot_solana::runtime_config::RuntimeConfig::default();
        let never = |_: &[u8]| -> Result<Vec<u8>, String> { panic!("no confirmation") };
        let refused = |u: &ToolUse, config| issue(&p, config, u, 1, &never).unwrap_err();
        let e = refused(&probe(&signers, &messages), &config);
        assert!(e.contains("no runtime owner runs"), "{e}");
        let _lock = super::super::control::acquire_owner(
            &super::super::control::default_lock_path(&p),
            "tool_test",
            "shadow",
        )
        .unwrap();
        let (epoch, _) =
            cap::open_epoch_with_nonce(&cap::request_dir(&p), std::process::id(), None).unwrap();
        let e = refused(&probe(&signers, &messages), &config);
        assert!(
            e.contains("ECON_SIGN DISABLED")
                && e.contains(&format!("WALLET:{} DISABLED", signers[0])),
            "{e}"
        );
        decide(&p, epoch, cap::ECON_SIGN, cap::ENABLED);
        decide(&p, epoch, &format!("WALLET:{}", signers[0]), cap::ENABLED);
        // A probe never sends.
        let send = ToolUse {
            send: true,
            ..probe(&signers, &messages)
        };
        assert!(refused(&send, &config).starts_with("TOOL_COST_LIMIT"));
        // A prewarm send: no ceiling configured, above it, without ECON_SEND.
        let prewarm = |cost| ToolUse {
            tool: Tool::PrewarmAtas,
            send: true,
            cost_lamports: cost,
            ..probe(&signers, &messages)
        };
        assert!(refused(&prewarm(1), &config).contains("no cost ceiling"));
        let mut capped = config.clone();
        capped.limits.prewarm_max_lamports = 10;
        assert!(refused(&prewarm(11), &capped).contains("ceiling is 10"));
        assert!(refused(&prewarm(10), &capped).contains("ECON_SEND DISABLED"));
        // A later DISABLE decided takes the switch back; one not decided yet holds everything.
        decide(&p, epoch, cap::ECON_SIGN, cap::DISABLED);
        assert!(refused(&probe(&signers, &messages), &config).contains("ECON_SIGN DISABLED"));
        decide(&p, epoch, cap::ECON_SIGN, cap::ENABLED);
        let id = cap::new_request_id().unwrap();
        cap::write_request(
            &cap::request_dir(&p),
            &cap::NewRequest {
                capability: cap::ECON_SEND,
                requested: cap::DISABLED,
                request_id: &id,
                owner_epoch: epoch,
                expected: None,
                now_unix_s: 1,
            },
        )
        .unwrap();
        assert_eq!(
            refused(&probe(&signers, &messages), &config),
            cap::STOP_PENDING
        );
        let _ = std::fs::remove_dir_all(&p);
    }

    /// DEC-17f, the operator's confirmation (the registered test key): a confirmed use signs
    /// exactly its messages once each, by exactly its signers, is recorded once and never
    /// permitted again; a send is admitted only while its switches hold; a foreign or missing
    /// confirmation permits nothing.
    #[cfg(feature = "operator-test-signer")]
    #[test]
    fn dec17f_a_confirmed_tool_use_signs_its_messages_once() {
        use super::super::operator_auth;
        let p = project("confirm");
        let payer = Keypair::new();
        let signers = [payer.pubkey()];
        let key = operator_auth::register(&p, true).unwrap();
        let config = darkbot_solana::runtime_config::RuntimeConfig {
            operator_key_sha256: operator_auth::key_sha256(&key.public_key),
            limits: darkbot_solana::runtime_config::Limits {
                prewarm_max_lamports: 10,
                ..Default::default()
            },
            ..Default::default()
        };
        let _lock = super::super::control::acquire_owner(
            &super::super::control::default_lock_path(&p),
            "tool_test",
            "shadow",
        )
        .unwrap();
        let (epoch, _) =
            cap::open_epoch_with_nonce(&cap::request_dir(&p), std::process::id(), None).unwrap();
        for c in [cap::ECON_SIGN, cap::ECON_SEND] {
            decide(&p, epoch, c, cap::ENABLED);
        }
        decide(&p, epoch, &format!("WALLET:{}", signers[0]), cap::ENABLED);
        let confirm = operator_confirmation(&p, Tool::ProbeCosts);
        // Declined, or confirmed with another key: nothing permitted, nothing recorded.
        let messages: [&[u8]; 2] = [b"one", b"two"];
        let declined = |_: &[u8]| -> Result<Vec<u8>, String> { Err("OPERATOR_DECLINED".into()) };
        assert_eq!(
            issue(&p, &config, &probe(&signers, &messages), 1, &declined).unwrap_err(),
            "OPERATOR_DECLINED"
        );
        let foreign = |_: &[u8]| -> Result<Vec<u8>, String> { Ok(vec![0x30, 0x02, 0x01, 0x01]) };
        assert!(issue(&p, &config, &probe(&signers, &messages), 1, &foreign)
            .unwrap_err()
            .starts_with(operator_auth::OPERATOR_AUTH_REQUIRED));
        // Confirmed: each message once, by exactly the permitted signer; no send.
        let mut permit = issue(&p, &config, &probe(&signers, &messages), 1, &confirm).unwrap();
        let other = Keypair::new();
        assert!(permit
            .sign(b"one", &[&other])
            .unwrap_err()
            .starts_with("TOOL_PERMIT_SIGNERS"));
        let s = permit.sign(b"one", &[&payer]).unwrap();
        assert!(s[0].verify(payer.pubkey().as_ref(), b"one"));
        assert!(permit
            .sign(b"one", &[&payer])
            .unwrap_err()
            .starts_with("TOOL_PERMIT_MESSAGE"));
        assert!(permit
            .sign(b"three", &[&payer])
            .unwrap_err()
            .starts_with("TOOL_PERMIT_MESSAGE"));
        permit.sign(b"two", &[&payer]).unwrap();
        assert!(permit
            .admit_send()
            .unwrap_err()
            .starts_with("TOOL_PERMIT_NO_SEND"));
        // The same messages are never permitted twice.
        assert!(issue(&p, &config, &probe(&signers, &messages), 2, &confirm)
            .unwrap_err()
            .starts_with("TOOL_USE_REPLAYED"));
        // A send: admitted while its switches hold, refused once a stop waits.
        let tx: [&[u8]; 1] = [b"prewarm"];
        let send = |cost_lamports| ToolUse {
            tool: Tool::PrewarmAtas,
            send: true,
            cost_lamports,
            ..probe(&signers, &tx)
        };
        let permit = issue(&p, &config, &send(10), 3, &confirm).unwrap();
        let record = std::fs::read_dir(p.join(STORE))
            .unwrap()
            .filter_map(|e| journal::read_typed_value::<OperatorToolUse>(&e.unwrap().path()).ok())
            .find(|r| r.send)
            .unwrap();
        assert_eq!(
            (
                record.tool.as_str(),
                record.owner_epoch,
                record.cost_lamports,
                record.cost_limit_lamports
            ),
            ("p0-prewarm-atas", epoch, 10, 10)
        );
        let id = cap::new_request_id().unwrap();
        cap::write_request(
            &cap::request_dir(&p),
            &cap::NewRequest {
                capability: cap::ECON_SEND,
                requested: cap::DISABLED,
                request_id: &id,
                owner_epoch: epoch,
                expected: None,
                now_unix_s: 1,
            },
        )
        .unwrap();
        assert_eq!(permit.admit_send().unwrap_err(), cap::STOP_PENDING);
        let _ = std::fs::remove_dir_all(&p);
    }
}

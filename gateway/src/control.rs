//! Authenticated local control: Project 0's capture and analytics jobs, BSC's readiness job and
//! candidate request, and the requests of the native BSC owner (modes, wallet and economic
//! switches, limits, the LIVE grant and its revocation), each one fixed allowlisted `darkbotctl`
//! call. The gateway signs, sends and decides nothing: the native owners apply or refuse, and a
//! BSC LIVE grant is a permission whose every intent the owner admits again at its dispatch.
//! No arbitrary paths or shell commands.
use crate::{
    config::Limits,
    http::AppState,
    lifecycle::{classify, CaptureProcess},
    proto::{
        expected_version, CommandLookupResponse, CommandPhase, CommandRejectReason, CommandRequest,
        CommandResponse, CommandStatus, ProjectId,
    },
    session,
};
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{process::Command, sync::Mutex};

const START: &str = "project0.capture.start";
const STOP: &str = "project0.capture.stop";
const BSC_START: &str = "bsc.readiness.start";
const BSC_STOP: &str = "bsc.readiness.stop";
/// Offline G0.7 A/B candidate selection request; never arm, sign or send.
const BSC_CANDIDATE: &str = "bsc.candidate.select";
/// BSC source mode request (plan §5a.2, order V2 §5): the native owner applies or
/// refuses it; a requested LIVE is effective only while the owner's LIVE gate holds
/// (plan 28.10.). The gateway never arms, signs or sends.
const BSC_SOURCE: &str = "bsc.source.mode.set";
/// The five signal sources and the switchable runtime components of the
/// native BSC mode owner (`bsc_runtime source request`).
const BSC_SOURCE_IDS: [&str; 8] = [
    "BSC_INGRESS_GETH",
    "BSC_INGRESS_PUBLICNODE",
    "BSC_INGRESS_DRPC",
    "BSC_INGRESS_BLOCKREQ",
    "BSC_INGRESS_48CLUB",
    "BSC_STATE_C2",
    "BSC_EXEC",
    "BSC_STRATEGY_C",
];
const BSC_SOURCE_MODES: [&str; 3] = ["OFF", "SHADOW", "LIVE"];
/// Plan v9 §2.3: a venue's admission mode at the same native owner.
const BSC_VENUE: &str = "bsc.venue.mode.set";
const BSC_VENUE_IDS: [&str; 3] = [
    "BSC_VENUE_FLAP",
    "BSC_VENUE_FOUR_CLASSIC",
    "BSC_VENUE_OPENFOUR",
];
/// Plan 27.10.: the launcher's and the buyer's native ENABLE/DISABLE switch, with the
/// owner generation as `wallet_policy_version` CAS. A permission, never a signature or send.
const BSC_WALLET: &str = "bsc.wallet.set";
const BSC_WALLET_IDS: [&str; 2] = ["BSC_WALLET_LAUNCHER", "BSC_WALLET_BUYER"];
/// Plan 27.10.: the economic switches SIGN, ARM, SEND, ATTEMPT and LIVE at the same owner;
/// each is a permission its Mainnet path checks besides its own guard, never an action.
const BSC_ECONOMIC: &str = "bsc.economic.set";
const BSC_ECONOMIC_IDS: [&str; 5] = [
    "BSC_ECON_SIGN",
    "BSC_ECON_ARM",
    "BSC_ECON_SEND",
    "BSC_ECON_ATTEMPT",
    "BSC_ECON_LIVE",
];
const BSC_SWITCH_MODES: [&str; 2] = ["DISABLED", "ENABLED"];
/// Plan v9 §2.5: the global runtime mode at the same owner (a requested LIVE is effective
/// only while the owner's LIVE gate holds).
const BSC_RUNTIME: &str = "bsc.runtime.mode.set";
const BSC_RUNTIME_IDS: [&str; 1] = ["BSC_RUNTIME_MODE"];
const BSC_RUNTIME_MODES: [&str; 4] = ["OFF", "SHADOW", "PAPER", "LIVE"];
/// Plan 27.10. (8b4721bb): the persisted daily limit of economic BUY attempts (a decimal count
/// per UTC day) at the same owner; a limit, never a permission to execute.
const BSC_ATTEMPT_LIMIT: &str = "bsc.attempt.limit.set";
const BSC_ATTEMPT_LIMIT_IDS: [&str; 1] = ["BSC_BUY_ATTEMPTS_PER_DAY"];
/// Every request the native mode/switch owner (`bsc_runtime source request`) takes.
/// Plan 28.10.: the campaign's total BUY attempt limit (at most the runtime configuration's
/// BSC_BUY_ATTEMPTS_TOTAL_MAX, which the owner checks: Audit E 3.1), the operator's LIVE grant of
/// one confirmed scope and its revocation.
const BSC_ATTEMPT_TOTAL: &str = "bsc.attempt.total.set";
const BSC_ATTEMPT_TOTAL_IDS: [&str; 1] = ["BSC_BUY_ATTEMPTS_TOTAL"];
const BSC_LIVE_GRANT: &str = "bsc.live.grant";
const BSC_LIVE_REVOKE: &str = "bsc.live.revoke";
const BSC_LIVE_REVOKE_IDS: [&str; 1] = ["BSC_LIVE_GRANT"];
const BSC_LIVE_REVOKE_MODES: [&str; 1] = ["DISABLED"];
const BSC_LIVE_STRATEGIES: [&str; 3] = ["C3", "STRATEGY_C", "C3+STRATEGY_C"];
const BSC_LIVE_ADAPTERS: [&str; 2] = ["CLUB48_BUILDER_V1", "BLOCKRAZOR_BUILDER_V1"];

/// REV.1.0 R3b (Audit A): the operator's decisions of the LIVE ledger (`bsc_runtime live lot`,
/// `nonce-burn`, `ledger quarantine`), once per request, each with the state it was decided on as
/// CAS: a lot's RESUME, WRITE_OFF or EXIT_NOW (exit-now locked by `BSC_EXEC_LIVE_OPERATOR_EXIT`)
/// and an unresolved transaction's nonce burn (locked by `BSC_EXEC_LIVE_NONCE_BURN`) at the
/// ledger's version, the quarantine of a ledger entry that does not decode at the owner
/// generation. Facts the runtime acts on, never a release of its own.
const BSC_LIVE_LOT: &str = "bsc.live.lot";
const BSC_LIVE_LOT_DECISIONS: [&str; 3] = ["RESUME", "WRITE_OFF", "EXIT_NOW"];
const BSC_LIVE_NONCE_BURN: &str = "bsc.live.nonce_burn";
const BSC_LIVE_QUARANTINE: &str = "bsc.live.ledger.quarantine";
const BSC_LIVE_DECISIONS: [&str; 3] = [BSC_LIVE_LOT, BSC_LIVE_NONCE_BURN, BSC_LIVE_QUARANTINE];

/// Frontend package (06.11.): the operator's Strategy C settings at their native owner
/// (`darkbotctl bsc strategy set`, `bsc_runtime strategy set`): `<KEY>:<VALUE>` with a key from
/// the fixed set the LIVE tab sets and the settings generation of the displayed state as
/// `profile_version` CAS. Only the shape is checked here (one word of printable ASCII, or
/// nothing); the runtime configuration's own parser decides the value, its refusal is the
/// receipt. Effective at the strategy's next start; never a signature or send.
const BSC_STRATEGY_SET: &str = "bsc.strategy.set";
const BSC_STRATEGY_KEYS: [&str; 9] = [
    "BSC_STRATEGY_C_OBSERVATION_MS",
    "BSC_STRATEGY_C_FILTERS",
    "BSC_STRATEGY_C_INPUT_WEI",
    "BSC_STRATEGY_C_TARGET_NET_MULTIPLE",
    "BSC_STRATEGY_C_STOP_NET_MULTIPLE",
    "BSC_STRATEGY_C_HOLD_MS",
    "BSC_STRATEGY_C_MAX_OPEN",
    "BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL",
    "BSC_STRATEGY_C_CREATOR_SELL_EXIT",
];

const BSC_OWNER_ACTIONS: [&str; 8] = [
    BSC_SOURCE,
    BSC_VENUE,
    BSC_WALLET,
    BSC_ECONOMIC,
    BSC_RUNTIME,
    BSC_ATTEMPT_LIMIT,
    BSC_ATTEMPT_TOTAL,
    BSC_LIVE_REVOKE,
];

/// A daily limit as the owner parses it: a plain decimal within u32, no sign or leading zero.
fn per_day_ok(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_digit())
        && (s == "0" || !s.starts_with('0'))
        && s.parse::<u32>().is_ok()
}
/// B03/U-9: offline analytics jobs of the Project 0 job controller (`p0ctl job …`).
const BACKTEST_START: &str = "project0.analytics.backtest.start";
const OPTIMIZE_START: &str = "project0.analytics.optimize.start";
const JOB_CANCEL: &str = "project0.analytics.job.cancel";
/// H0 (Track H0): the historical liquidation report over every saved run (`p0ctl job start h0`).
const H0_START: &str = "project0.analytics.h0.start";
/// A08/F-02: the post-capture chain (verification, report, qualification) of a recorded run.
const VERIFY_START: &str = "project0.capture.verify.start";
/// S1-02: a data source's operating mode (`p0ctl source set`); the running owner decides.
const SOURCE_SET: &str = "project0.source.set";
/// DEC-13: a capability request (`p0ctl capability set`, LIVE); the running owner decides what
/// is effective — LIVE stays BLOCKED until a separate FE-06 authorization.
const CAPABILITY_SET: &str = "project0.capability.set";
/// DEC-07: the boot controller (`p0ctl boot …`): a new flow's offline preflight, the operator's
/// network approval, the start approval (each naming the token it approves) and a step's stop.
const BOOT_OFFLINE: &str = "project0.boot.offline";
const BOOT_NETWORK: &str = "project0.boot.network";
const BOOT_START: &str = "project0.boot.start";
const BOOT_CANCEL: &str = "project0.boot.cancel";
const BOOT_ACTIONS: [&str; 4] = [BOOT_OFFLINE, BOOT_NETWORK, BOOT_START, BOOT_CANCEL];
/// DEC-16: the standing autostart approval of an unbounded raw capture (`p0ctl autostart …`):
/// the preview of its binding, the grant confirming exactly that preview's digest, the revocation.
const AUTOSTART_PREVIEW: &str = "project0.autostart.preview";
const AUTOSTART_GRANT: &str = "project0.autostart.grant";
const AUTOSTART_REVOKE: &str = "project0.autostart.revoke";
const AUTOSTART_ACTIONS: [&str; 3] = [AUTOSTART_PREVIEW, AUTOSTART_GRANT, AUTOSTART_REVOKE];
/// DEC-18 (P0-020): the provider registry (`p0ctl provider …`) — a provider's add, update,
/// removal, enablement, priority, selection and qualification, a credential's set, rotation and
/// revocation; every change with the registry version its view showed (CAS) and its request id,
/// effective at the next controlled start.
const PROVIDER_ADD: &str = "project0.provider.add";
const PROVIDER_UPDATE: &str = "project0.provider.update";
const PROVIDER_REMOVE: &str = "project0.provider.remove";
const PROVIDER_ENABLE: &str = "project0.provider.enable";
const PROVIDER_DISABLE: &str = "project0.provider.disable";
const PROVIDER_PRIORITY: &str = "project0.provider.priority";
const PROVIDER_SELECT: &str = "project0.provider.select";
const PROVIDER_UNSELECT: &str = "project0.provider.unselect";
const PROVIDER_QUALIFY: &str = "project0.provider.qualify";
const PROVIDER_CREDENTIAL_SET: &str = "project0.provider.credential.set";
const PROVIDER_CREDENTIAL_ROTATE: &str = "project0.provider.credential.rotate";
const PROVIDER_CREDENTIAL_REVOKE: &str = "project0.provider.credential.revoke";
const PROVIDER_ACTIONS: [&str; 12] = [
    PROVIDER_ADD,
    PROVIDER_UPDATE,
    PROVIDER_REMOVE,
    PROVIDER_ENABLE,
    PROVIDER_DISABLE,
    PROVIDER_PRIORITY,
    PROVIDER_SELECT,
    PROVIDER_UNSELECT,
    PROVIDER_QUALIFY,
    PROVIDER_CREDENTIAL_SET,
    PROVIDER_CREDENTIAL_ROTATE,
    PROVIDER_CREDENTIAL_REVOKE,
];
/// DEC-18 (P0-019): the actions that carry a credential's secret — separated from the request
/// before any durable record (the ledger holds the request without it) and handed to the native
/// controller's standard input only.
const SECRET_ACTIONS: [&str; 2] = [PROVIDER_CREDENTIAL_SET, PROVIDER_CREDENTIAL_ROTATE];

/// DEC-18 §1.2: the provider fields' secret-free shape, the native descriptor's rules
/// (`darkbot_solana::providers`) applied before the ledger, so a key typed into an endpoint or a
/// name is refused and never recorded; the native controller decides every change.
mod provider_shape {
    /// 16 or more hex digits and nothing else (the form of many providers' keys).
    fn key_shaped(s: &str) -> bool {
        s.len() >= 16 && s.bytes().all(|b| b.is_ascii_hexdigit())
    }
    /// A provider or credential id: 1–32 of `[a-z0-9_-]`, never key-shaped, never read as an
    /// option of the controller (no leading `-`).
    pub fn id_ok(s: &str) -> bool {
        !s.is_empty()
            && s.len() <= 32
            && !s.starts_with('-')
            && !key_shaped(s)
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    }
    /// A token-like path segment (the descriptor's three rules).
    fn token_like(segment: &str) -> bool {
        let has = |s: &str, f: fn(&u8) -> bool| s.as_bytes().iter().any(f);
        let word = segment.split(['-', '_', '.']).any(|w| {
            w.len() >= 16 && has(w, u8::is_ascii_digit) && has(w, u8::is_ascii_alphabetic)
        });
        let joined: String = segment
            .chars()
            .filter(|c| !matches!(c, '-' | '_' | '.'))
            .collect();
        let hex = joined.len() >= 16
            && joined.bytes().all(|b| b.is_ascii_hexdigit())
            && has(&joined, u8::is_ascii_digit)
            && has(&joined, u8::is_ascii_alphabetic);
        let mixed = segment.len() >= 20
            && has(segment, u8::is_ascii_uppercase)
            && has(segment, u8::is_ascii_lowercase)
            && has(segment, u8::is_ascii_digit);
        word || hex || mixed
    }
    /// `host[:port]` (an IPv6 host in brackets) with a numeric port of at most 65535 — never a
    /// key in the port's place (the native URL parser refuses it, after the ledger).
    fn authority_ok(authority: &str) -> bool {
        let (host, port) = match authority.strip_prefix('[') {
            Some(v6) => match v6.split_once(']') {
                Some((host, "")) => (host, None),
                Some((host, rest)) => match rest.strip_prefix(':') {
                    Some(port) => (host, Some(port)),
                    None => return false,
                },
                None => return false,
            },
            None => match authority.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (authority, None),
            },
        };
        // An IPv6 host is an address (the native parser refuses anything else in brackets); any
        // other host is a DNS name or an IPv4 address of at most 253 characters.
        let host_ok = if authority.starts_with('[') {
            host.parse::<std::net::Ipv6Addr>().is_ok()
        } else {
            host.len() <= 253
                && host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
        };
        !host.is_empty()
            && host_ok
            && match port {
                Some(p) => {
                    !p.is_empty()
                        && p.bytes().all(|b| b.is_ascii_digit())
                        && p.parse::<u16>().is_ok()
                }
                None => true,
            }
    }
    /// An endpoint without user info, query, fragment, backslash (the native URL parser reads it
    /// as `/`) or token-like path segment, with a numeric port, at most 512 characters of visible
    /// ASCII.
    pub fn endpoint_ok(url: &str) -> bool {
        let Some((scheme, rest)) = url.split_once("://") else {
            return false;
        };
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        url.len() <= 512
            && url.bytes().all(|b| b.is_ascii_graphic())
            && !url.contains(['?', '#', '\\', '"', '<', '>', '^', '`', '{', '}', '|'])
            && ["https", "http", "wss", "ws"].contains(&scheme)
            && authority_ok(authority)
            && !path.split('/').any(token_like)
    }
    /// none | path | query:NAME | header:NAME (a name, never a key).
    pub fn auth_ok(auth: &str) -> bool {
        let name = |n: &str| {
            !n.is_empty()
                && n.len() <= 64
                && !key_shaped(n)
                && !token_like(n)
                && n.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        };
        match auth.split_once(':') {
            None => ["none", "path"].contains(&auth),
            Some(("query" | "header", n)) => name(n),
            _ => false,
        }
    }
    /// The plan's published name: 1–32 of `[a-z0-9_.-]`, never key-shaped, never read as an
    /// option (no leading `-`).
    pub fn entitlement_ok(s: &str) -> bool {
        !s.is_empty()
            && s.len() <= 32
            && !s.starts_with('-')
            && !key_shaped(s)
            && s.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')
            })
    }
    pub const KINDS: [&str; 5] = ["rpc", "wss", "geyser", "jupiter", "jito"];
}

/// A credential's secret in transit: its bytes overwritten when it is released, on every path
/// (volatile: never elided); never printed.
struct SecretBuf(Vec<u8>);

/// Overwrites `bytes` with zeros (volatile: never elided).
fn wipe(bytes: &mut [u8]) {
    for b in bytes.iter_mut() {
        // SAFETY: a valid, exclusive reference to an initialized byte.
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

impl Drop for SecretBuf {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

/// DEC-18 (P0-019): a command's request body, which may carry a credential's secret; overwritten
/// when the command returns, once no view of it remains (the ledger holds the request without the
/// secret). A body still shared elsewhere is left to its other owner.
struct WipedBody(Bytes);

impl Drop for WipedBody {
    fn drop(&mut self) {
        if let Ok(mut owned) = std::mem::take(&mut self.0).try_into_mut() {
            wipe(&mut owned);
            #[cfg(test)]
            WIPED_BODIES.with(|n| n.set(n.get() + 1));
        }
    }
}

#[cfg(test)]
thread_local! {
    /// The request bodies this thread overwrote (a test observes the wipe it cannot read back).
    static WIPED_BODIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// DEC-18 (P0-019, review L1): a credential command read from its request without copying the
/// secret — decoded from the shared `Bytes`, its secret stays a view of the request body (decoding
/// from a slice would allocate a copy that is freed unwiped).
fn credential_command(
    req: &CommandRequest,
) -> Option<crate::proto::project0::v1::ProviderCredentialCommand> {
    crate::proto::project0::v1::ProviderCredentialCommand::decode(req.params.clone()).ok()
}

/// DEC-18 (P0-019): `req` without its secret (the request every durable record holds) and the
/// secret, for a credential action; `None` when its parameters do not decode or carry none.
fn separate_secret(req: &CommandRequest) -> Option<(CommandRequest, Option<SecretBuf>)> {
    if !SECRET_ACTIONS.contains(&req.action.as_str()) {
        return Some((req.clone(), None));
    }
    let mut c = credential_command(req)?;
    // The native rule (`provider_registry` CREDENTIAL_SECRET_FORMAT): one line of visible bytes —
    // the controller reads its input up to the line end; the body limit bounds its length.
    if c.secret.is_empty() || c.secret.iter().any(u8::is_ascii_control) {
        return None;
    }
    // Room for the line end the controller's input takes: the buffer never reallocates (an
    // unwiped copy left behind).
    let mut buf = Vec::with_capacity(c.secret.len() + 1);
    buf.extend_from_slice(&c.secret);
    let secret = SecretBuf(buf);
    c.secret = Default::default();
    Some((
        CommandRequest {
            params: c.encode_to_vec().into(),
            ..req.clone()
        },
        Some(secret),
    ))
}

const JOB_ACTIONS: [&str; 14] = [
    BACKTEST_START,
    OPTIMIZE_START,
    JOB_CANCEL,
    H0_START,
    VERIFY_START,
    SOURCE_SET,
    CAPABILITY_SET,
    BOOT_OFFLINE,
    BOOT_NETWORK,
    BOOT_START,
    BOOT_CANCEL,
    AUTOSTART_PREVIEW,
    AUTOSTART_GRANT,
    AUTOSTART_REVOKE,
];
/// An online chain verification (index, strict, crosscheck; DEC-08) reads the chain with the
/// operator's provider evidence: a terminal action (`p0ctl job start post-capture
/// --provider-evidence`), never a GUI one.
fn online_history_read(req: &CommandRequest) -> bool {
    use crate::proto::project0::v1::PostCaptureJobCommand;
    req.action == VERIFY_START
        && PostCaptureJobCommand::decode(req.params.as_ref())
            .is_ok_and(|c| c.chain_verify != "off" && !c.offline)
}
fn allowed(project: i32, action: &str) -> bool {
    (project == ProjectId::Project0 as i32
        && ([START, STOP].contains(&action)
            || JOB_ACTIONS.contains(&action)
            || PROVIDER_ACTIONS.contains(&action)))
        || (project == ProjectId::Bsc as i32
            && ([BSC_START, BSC_STOP, BSC_CANDIDATE, BSC_LIVE_GRANT].contains(&action)
                || action == BSC_STRATEGY_SET
                || BSC_OWNER_ACTIONS.contains(&action)
                || BSC_LIVE_DECISIONS.contains(&action)))
}
/// The only accepted candidate request: a fixed profile as resource and the
/// selection generation of the last applied status as `profile_version` CAS.
fn candidate_request(req: &CommandRequest) -> Option<(&'static str, u64)> {
    let profile = match req.resource_id.as_str() {
        "SELF_A" => "SELF_A",
        "SELF_B" => "SELF_B",
        _ => return None,
    };
    match req.expected.as_ref()?.version? {
        expected_version::Version::ProfileVersion(generation) if req.params.is_empty() => {
            Some((profile, generation))
        }
        _ => None,
    }
}

/// The only accepted owner request: `<ID>:<MODE>` from the action's fixed sets as resource and
/// the generation of the displayed native state as CAS (`wallet_policy_version` for a wallet,
/// `config_version` otherwise); no parameters.
fn source_request(req: &CommandRequest) -> Option<(&'static str, String, u64)> {
    let (ids, modes): (&[&'static str], &[&'static str]) = match req.action.as_str() {
        BSC_SOURCE => (&BSC_SOURCE_IDS, &BSC_SOURCE_MODES),
        BSC_VENUE => (&BSC_VENUE_IDS, &BSC_SOURCE_MODES),
        BSC_WALLET => (&BSC_WALLET_IDS, &BSC_SWITCH_MODES),
        BSC_ECONOMIC => (&BSC_ECONOMIC_IDS, &BSC_SWITCH_MODES),
        BSC_RUNTIME => (&BSC_RUNTIME_IDS, &BSC_RUNTIME_MODES),
        BSC_ATTEMPT_LIMIT => (&BSC_ATTEMPT_LIMIT_IDS, &[]),
        BSC_ATTEMPT_TOTAL => (&BSC_ATTEMPT_TOTAL_IDS, &[]),
        BSC_LIVE_REVOKE => (&BSC_LIVE_REVOKE_IDS, &BSC_LIVE_REVOKE_MODES),
        _ => return None,
    };
    let (id, mode) = req.resource_id.split_once(':')?;
    let id = ids.iter().copied().find(|s| *s == id)?;
    let mode = if req.action == BSC_ATTEMPT_LIMIT || req.action == BSC_ATTEMPT_TOTAL {
        per_day_ok(mode).then(|| mode.to_string())?
    } else {
        modes.iter().copied().find(|m| *m == mode)?.to_string()
    };
    if !req.params.is_empty() {
        return None;
    }
    match (req.action.as_str(), req.expected.as_ref()?.version?) {
        (BSC_WALLET, expected_version::Version::WalletPolicyVersion(generation)) => {
            Some((id, mode, generation))
        }
        (
            BSC_SOURCE | BSC_VENUE | BSC_ECONOMIC | BSC_RUNTIME | BSC_ATTEMPT_LIMIT
            | BSC_ATTEMPT_TOTAL | BSC_LIVE_REVOKE,
            expected_version::Version::ConfigVersion(g),
        ) => Some((id, mode, g)),
        _ => None,
    }
}

/// The only accepted Strategy C setting: `<KEY>:<VALUE>` (see [`BSC_STRATEGY_SET`]) with the
/// `profile_version` generation and no parameters; the arguments of `darkbotctl bsc strategy set`
/// without the request id.
fn strategy_request(req: &CommandRequest) -> Option<Vec<String>> {
    if req.action != BSC_STRATEGY_SET || !req.params.is_empty() {
        return None;
    }
    let (id, value) = req.resource_id.split_once(':')?;
    let id = BSC_STRATEGY_KEYS.iter().copied().find(|k| *k == id)?;
    if !value.bytes().all(|b| b.is_ascii_graphic()) {
        return None;
    }
    match req.expected.as_ref()?.version? {
        expected_version::Version::ProfileVersion(g) => Some(
            ["bsc", "strategy", "set", id, value]
                .into_iter()
                .map(String::from)
                .chain([g.to_string()])
                .collect(),
        ),
        _ => None,
    }
}

/// A plain decimal wei amount (no sign, no leading zero), within 128 bits.
fn wei_ok(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_digit())
        && (s == "0" || !s.starts_with('0'))
        && s.parse::<u128>().is_ok()
}

/// Plan 28.10.: the only accepted LIVE grant: the fixed resource `BSC_LIVE_GRANT`, the scope
/// the operator confirmed as `BscLiveGrantCommand` (every value from the fixed sets or a plain
/// decimal, the scope's SHA-256 as shown) and the owner generation as CAS. The arguments of
/// `darkbotctl bsc live grant` without the request id. Audit E 4.8: only the shape is checked
/// here; the native owner alone decides the bounds (slippage, gas, limits, losses, attempts,
/// validity: `LiveGrant::validate`; its refusal is the receipt's detail, as in the terminal; an
/// empty source or adapter list already stops at `bsc-control.sh`), recomputes the scope digest
/// and refuses a grant whose digest differs.
fn grant_request(req: &CommandRequest) -> Option<Vec<String>> {
    use crate::proto::bsc::v1::BscLiveGrantCommand;
    if req.action != BSC_LIVE_GRANT || req.resource_id != "BSC_LIVE_GRANT" {
        return None;
    }
    let expected_version::Version::ConfigVersion(generation) = req.expected.as_ref()?.version?
    else {
        return None;
    };
    let c = BscLiveGrantCommand::decode(req.params.as_ref()).ok()?;
    // BSC3-069 and 31.10. order: the risk and the limits are part of every grant.
    let risk = c.risk?;
    let limits = c.limits?;
    let ingress =
        |s: &String| s.starts_with("BSC_INGRESS_") && BSC_SOURCE_IDS.contains(&s.as_str());
    let ok = BSC_VENUE_IDS.contains(&c.venue.as_str())
        && BSC_LIVE_STRATEGIES.contains(&c.strategies.as_str())
        && c.sources.iter().all(ingress)
        && c.adapters
            .iter()
            .all(|a| BSC_LIVE_ADAPTERS.contains(&a.as_str()))
        && wei_ok(&c.max_input_wei)
        && wei_ok(&c.max_spend_wei)
        && wei_ok(&limits.max_realized_loss_wei)
        && wei_ok(&limits.max_daily_realized_loss_wei)
        && c.scope_sha256.len() == 64
        && c.scope_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    ok.then(|| {
        vec![
            "bsc".into(),
            "live".into(),
            "grant".into(),
            c.venue,
            c.strategies,
            c.sources.join(","),
            c.adapters.join(","),
            c.max_input_wei,
            c.max_spend_wei,
            c.max_attempts.to_string(),
            c.expires_unix_ms.to_string(),
            // BSC3-058: a separate authorization of the exits alone.
            if c.exit_only { "EXIT_ONLY" } else { "ALL" }.into(),
            risk.max_buy_slippage_bps.to_string(),
            risk.max_exit_slippage_bps.to_string(),
            risk.max_gas_price_wei.to_string(),
            risk.max_buy_gas_limit.to_string(),
            risk.max_approve_gas_limit.to_string(),
            risk.max_sell_gas_limit.to_string(),
            limits.max_open_positions.to_string(),
            limits.max_realized_loss_wei,
            limits.max_daily_realized_loss_wei,
            limits.min_buy_interval_ms.to_string(),
            // R4 (BSC3-069): the operator confirmed the risk as an outlier.
            if risk.outlier_ack {
                "OUTLIER_ACK"
            } else {
                "NO_OUTLIER_ACK"
            }
            .into(),
            c.scope_sha256,
            generation.to_string(),
        ]
    })
}

/// R3b: the only accepted operator decision of the LIVE ledger: a lot
/// `0x<64 hex>:<RESUME|WRITE_OFF|EXIT_NOW>`, a transaction `0x<64 hex>` or a ledger entry's
/// sequence number (a plain decimal) as resource, no parameters, and as `config_version` CAS the
/// version of the displayed native state: the ledger's (its next entry's number) for a lot or a
/// burn, the owner generation for a quarantine. The arguments of `darkbotctl bsc live
/// lot|nonce-burn|ledger-quarantine` without the request id; the native ledger refuses, under its
/// operator lock, what does not hold (another version or generation, the lot not open, the switch
/// OFF, the transaction not unresolved, the entry decoding).
fn live_decision(req: &CommandRequest) -> Option<Vec<String>> {
    let hash = |s: &str| {
        s.strip_prefix("0x")
            .is_some_and(scope_digest_ok)
            .then(|| s.to_string())
    };
    let target = match req.action.as_str() {
        BSC_LIVE_LOT => {
            let (lot, what) = req.resource_id.split_once(':')?;
            let what = BSC_LIVE_LOT_DECISIONS.iter().find(|d| **d == what)?;
            vec!["lot".to_string(), hash(lot)?, what.to_string()]
        }
        BSC_LIVE_NONCE_BURN => vec!["nonce-burn".to_string(), hash(&req.resource_id)?],
        BSC_LIVE_QUARANTINE => {
            let seq = &req.resource_id;
            (wei_ok(seq) && seq.parse::<u64>().is_ok()).then_some(())?;
            vec!["ledger-quarantine".to_string(), seq.clone()]
        }
        _ => return None,
    };
    if !req.params.is_empty() {
        return None;
    }
    let expected_version::Version::ConfigVersion(generation) = req.expected.as_ref()?.version?
    else {
        return None;
    };
    Some(
        ["bsc", "live"]
            .into_iter()
            .map(String::from)
            .chain(target)
            .chain([generation.to_string()])
            .collect(),
    )
}

/// A run id under the project's run roots: never a path.
fn run_id_ok(s: &str) -> bool {
    (1..=128).contains(&s.len())
        && !s.starts_with(['.', '-'])
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
/// An exact USD decimal as `p0ctl` reads it: digits, at most 6 fraction digits.
fn usd_ok(s: &str) -> bool {
    let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
    (1..=15).contains(&whole.len())
        && whole.bytes().all(|b| b.is_ascii_digit())
        && (!s.contains('.') || (1..=6).contains(&fraction.len()))
        && fraction.bytes().all(|b| b.is_ascii_digit())
}
/// A funding class name (`FundingClass::as_str`); the controller checks it is one.
fn class_ok(s: &str) -> bool {
    (1..=32).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}
fn classes(set: &[String]) -> Option<String> {
    if set.len() > 8 || !set.iter().all(|c| class_ok(c)) {
        return None;
    }
    Some(if set.is_empty() {
        "all".into()
    } else {
        set.join(",")
    })
}
/// The options of one backtest point (`p0ctl backtest`'s flags).
fn point_args(c: &crate::proto::project0::v1::BacktestJobCommand) -> Option<Vec<String>> {
    if !run_id_ok(&c.run_id)
        || !usd_ok(&c.min_profit_usd)
        || !(1..=32).contains(&c.capital_usd.len())
        || c.capital_usd.contains(&0)
        || c.max_seconds == 0
        || c.max_ram_bytes == 0
    {
        return None;
    }
    let mut args = vec![
        "--run".to_string(),
        c.run_id.clone(),
        "--min-profit-usd".into(),
        c.min_profit_usd.clone(),
        "--capital-usd".into(),
        c.capital_usd
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(","),
        "--funding".into(),
        classes(&c.funding)?,
        "--max-seconds".into(),
        c.max_seconds.to_string(),
        "--max-ram-bytes".into(),
        c.max_ram_bytes.to_string(),
    ];
    if !c.verification_result.is_empty() {
        if !valid_job_id(&c.verification_result) {
            return None;
        }
        args.extend([
            "--verification-result".into(),
            c.verification_result.clone(),
        ]);
    }
    Some(args)
}
/// A job id of the controller (`[A-Za-z0-9-]`, at most 64).
fn valid_job_id(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && !s.starts_with('-')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
/// P8-CTRL.3: an FE-06 scope digest is 64 lowercase hex characters.
fn scope_digest_ok(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
/// P8-CTRL.1: a signer's public key as the owner names it (base58, 32 bytes: 32–44 characters);
/// the owner checks it is one of its configured signers.
fn base58_key_ok(s: &str) -> bool {
    (32..=44).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() && !matches!(b, b'0' | b'O' | b'I' | b'l'))
}
/// DEC-07: an approval names 16 lowercase hex characters of a readiness token.
fn token_ok(s: &str) -> bool {
    s.len() == 16
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
/// A base58 genesis pin (`p0ctl` compares it with the checked-in pin).
fn genesis_ok(s: &str) -> bool {
    (32..=44).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric())
}
/// DEC-07: the record id of an imported PROVIDER_EVIDENCE (16 lowercase hex digits); the boot
/// controller resolves it in its store and refuses an unknown one — a path is never passed on.
fn evidence_id_ok(s: &str) -> bool {
    s.len() == 16 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
/// A `run start --duration` value: digits and one unit (`p0ctl` checks it is a supported one).
fn duration_ok(s: &str) -> bool {
    if !s.is_ascii() {
        return false;
    }
    let (digits, unit) = s.split_at(s.len().saturating_sub(1));
    (1..=6).contains(&digits.len())
        && digits.bytes().all(|b| b.is_ascii_digit())
        && ["s", "m", "h", "d"].contains(&unit)
}
/// DEC-16: a boot flow's duration — a fixed one, or the unbounded horizon
/// `until-stopped:<window>:<runway>` of two durations (`p0ctl` checks window <= runway).
fn horizon_ok(s: &str) -> bool {
    match s.strip_prefix("until-stopped:") {
        Some(rest) => rest
            .split_once(':')
            .is_some_and(|(w, r)| duration_ok(w) && duration_ok(r)),
        None => duration_ok(s),
    }
}
fn capture_source_ok(s: &str) -> bool {
    s == "logs" || s == "transactions"
}
/// B03/U-9: the `darkbotctl p0 job …` arguments of an analytics job action — the typed
/// parameters mapped 1:1 onto the options of `p0ctl job`, which validates them again. A start's
/// job id is the request id; a cancel names its job as the resource.
fn job_args(req: &CommandRequest) -> Option<Vec<String>> {
    use crate::proto::project0::v1::{
        AutostartGrantCommand, AutostartPreviewCommand, BacktestJobCommand, BootNetworkCommand,
        BootOfflineCommand, BootStartCommand, CapabilityControlCommand, H0JobCommand,
        OptimizeJobCommand, PostCaptureJobCommand, ProviderDescriptorCommand,
        ProviderPriorityCommand, ProviderQualifyCommand, ProviderSelectCommand,
        SourceControlCommand,
    };
    // CTRL-03: a source change carries its CAS — the configuration version its view showed
    // (`LiquidatorState.source_config_version`), required here (review 5353cdcb); `p0ctl source
    // set --expect-version` checks it. No other action takes an expectation.
    let expect_version = match req.expected.as_ref().map(|e| e.version) {
        None => None,
        Some(Some(expected_version::Version::ConfigVersion(v)))
            if req.action == SOURCE_SET
                || req.action == CAPABILITY_SET
                || PROVIDER_ACTIONS.contains(&req.action.as_str()) =>
        {
            Some(v)
        }
        Some(_) => return None,
    };
    let start = |kind: &str| -> Vec<String> {
        ["p0", "job", "start", kind, "--id", &req.request_id]
            .map(String::from)
            .to_vec()
    };
    match req.action.as_str() {
        BACKTEST_START if req.resource_id.is_empty() => {
            let c = BacktestJobCommand::decode(req.params.as_ref()).ok()?;
            let mut args = start("backtest");
            args.extend(point_args(&c)?);
            Some(args)
        }
        H0_START if req.resource_id.is_empty() => {
            let c = H0JobCommand::decode(req.params.as_ref()).ok()?;
            if c.max_seconds == 0 || c.max_ram_bytes == 0 {
                return None;
            }
            let mut args = start("h0");
            if c.window_days > 0 {
                args.extend(["--window-days".into(), c.window_days.to_string()]);
            }
            args.extend([
                "--max-seconds".into(),
                c.max_seconds.to_string(),
                "--max-ram-bytes".into(),
                c.max_ram_bytes.to_string(),
            ]);
            Some(args)
        }
        OPTIMIZE_START if req.resource_id.is_empty() => {
            let c = OptimizeJobCommand::decode(req.params.as_ref()).ok()?;
            let mut args = start("optimize");
            args.extend(point_args(c.baseline.as_ref()?)?);
            if !(1..=64).contains(&c.min_profit_usd_grid.len())
                || !c.min_profit_usd_grid.iter().all(|v| usd_ok(v))
                || !(1..=16).contains(&c.funding_sets.len())
                || c.holdout_bps == 0
                || c.max_points == 0
            {
                return None;
            }
            let sets = c
                .funding_sets
                .iter()
                .map(|s| classes(&s.classes))
                .collect::<Option<Vec<_>>>()?;
            args.extend([
                "--min-profit-usd-grid".into(),
                c.min_profit_usd_grid.join(","),
                "--funding-sets".into(),
                sets.join(";"),
                "--holdout-bps".into(),
                c.holdout_bps.to_string(),
                "--max-points".into(),
                c.max_points.to_string(),
            ]);
            if c.save_profile {
                args.push("--save-profile".into());
            }
            Some(args)
        }
        VERIFY_START if req.resource_id.is_empty() => {
            let c = PostCaptureJobCommand::decode(req.params.as_ref()).ok()?;
            // DEC-08: off, or a history read (index, strict, crosscheck) from its checkpoint.
            let cached_read =
                matches!(c.chain_verify.as_str(), "index" | "strict" | "crosscheck") && c.offline;
            if !run_id_ok(&c.run_id)
                || !(c.chain_verify == "off" || cached_read)
                || (c.offline && c.chain_verify == "off")
                || c.max_seconds == 0
                || c.max_ram_bytes == 0
            {
                return None;
            }
            let mut args = start("post-capture");
            args.extend([
                "--run".into(),
                c.run_id,
                "--chain-verify".into(),
                c.chain_verify,
                "--max-seconds".into(),
                c.max_seconds.to_string(),
                "--max-ram-bytes".into(),
                c.max_ram_bytes.to_string(),
            ]);
            if c.offline {
                args.push("--offline".into());
            }
            if c.resume_verification {
                args.push("--resume-verification".into());
            }
            Some(args)
        }
        SOURCE_SET if req.resource_id.is_empty() => {
            let c = SourceControlCommand::decode(req.params.as_ref()).ok()?;
            let unit_ok = c.unit == "wss"
                || c.unit.strip_prefix("geyser:").is_some_and(|p| {
                    (1..=32).contains(&p.len())
                        && p.bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                });
            if !unit_ok || !["off", "observe", "active"].contains(&c.mode.as_str()) {
                return None;
            }
            let mut args: Vec<String> = ["p0", "source", "set", &c.unit, &c.mode]
                .map(String::from)
                .to_vec();
            args.extend(["--expect-version".into(), expect_version?.to_string()]);
            Some(args)
        }
        // DEC-13/DEC-04/DEC-11/DEC-06/B01/P6-MT.10: the capability and its requested value, with
        // the version the view showed: LIVE OFF|LIVE, HOLD_B, JITO_BUNDLE, THRESHOLD_TRIGGER,
        // INVENTORY_REBALANCING and LOCAL_FEE_MODEL OFF|OBSERVE|ACTIVE (the owner refuses
        // THRESHOLD_TRIGGER OFF → ACTIVE; INVENTORY_REBALANCING ACTIVE is never effective;
        // LOCAL_FEE_MODEL ACTIVE only in a sealed class). P8-CTRL.1/.3:
        // ECON_* and WALLET:<pubkey> DISABLED|ENABLED, FE06_GRANT/FE06_REVOKE a scope digest —
        // these state the owner epoch the view showed (P8-CTRL.6), and so does every other
        // request but a move to OFF (review M2: a view from before an owner restart never
        // re-arms LIVE or a mode in the new epoch); every change carries the command's request
        // id (the same action again is the same request).
        CAPABILITY_SET if req.resource_id.is_empty() => {
            let c = CapabilityControlCommand::decode(req.params.as_ref()).ok()?;
            let switch = ["DISABLED", "ENABLED"].contains(&c.requested.as_str());
            let (valid, economic) = match c.capability.as_str() {
                "LIVE" => (["OFF", "LIVE"].contains(&c.requested.as_str()), false),
                "HOLD_B"
                | "JITO_BUNDLE"
                | "THRESHOLD_TRIGGER"
                | "INVENTORY_REBALANCING"
                | "LOCAL_FEE_MODEL" => (
                    ["OFF", "OBSERVE", "ACTIVE"].contains(&c.requested.as_str()),
                    false,
                ),
                "ECON_SIGN" | "ECON_ARM" | "ECON_SEND" | "ECON_ATTEMPT" | "ECON_LIVE" => {
                    (switch, true)
                }
                "FE06_GRANT" | "FE06_REVOKE" => (scope_digest_ok(&c.requested), true),
                w => (
                    w.strip_prefix("WALLET:").is_some_and(base58_key_ok) && switch,
                    true,
                ),
            };
            let needs_epoch = economic || c.requested != "OFF";
            if !valid || (needs_epoch && c.owner_epoch == 0) || !valid_job_id(&req.request_id) {
                return None;
            }
            let mut args: Vec<String> = ["p0", "capability", "set", &c.capability, &c.requested]
                .map(String::from)
                .to_vec();
            args.extend(["--expect-version".into(), expect_version?.to_string()]);
            if c.owner_epoch > 0 {
                args.extend(["--expect-epoch".into(), c.owner_epoch.to_string()]);
            }
            args.extend(["--request-id".into(), req.request_id.clone()]);
            Some(args)
        }
        // DEC-18 (P0-020): a registry change — its shape here (a key in an endpoint or a name
        // is never recorded), its rules the native controller's; the registry version the view
        // showed and the request id always.
        action if PROVIDER_ACTIONS.contains(&action) => {
            use provider_shape::{auth_ok, endpoint_ok, entitlement_ok, id_ok, KINDS};
            if !valid_job_id(&req.request_id) {
                return None;
            }
            let none = req.params.is_empty();
            let mut args: Vec<String> = ["p0", "provider"].map(String::from).to_vec();
            let mut push = |items: &[&str]| args.extend(items.iter().map(|s| s.to_string()));
            match action {
                PROVIDER_ADD | PROVIDER_UPDATE if req.resource_id.is_empty() => {
                    let c = ProviderDescriptorCommand::decode(req.params.as_ref()).ok()?;
                    let add = action == PROVIDER_ADD;
                    if !id_ok(&c.id)
                        || (add
                            && (c.kind.is_none()
                                || c.endpoint.is_none()
                                || c.plan.is_none()
                                || c.entitlement.is_none()))
                        || (!add && c.disabled)
                    {
                        return None;
                    }
                    push(&[if add { "add" } else { "update" }, "--id", &c.id]);
                    if let Some(k) = &c.kind {
                        if !KINDS.contains(&k.as_str()) {
                            return None;
                        }
                        push(&["--kind", k]);
                    }
                    if let Some(e) = &c.endpoint {
                        if !endpoint_ok(e) {
                            return None;
                        }
                        push(&["--endpoint", e]);
                    }
                    if let Some(a) = &c.auth {
                        if !auth_ok(a) {
                            return None;
                        }
                        push(&["--auth", a]);
                    }
                    if let Some(cred) = &c.credential {
                        if !id_ok(cred) {
                            return None;
                        }
                        push(&["--credential", cred]);
                    }
                    if let Some(plan) = &c.plan {
                        if !["free", "unknown"].contains(&plan.as_str()) {
                            return None;
                        }
                        push(&["--plan", plan]);
                    }
                    if let Some(e) = &c.entitlement {
                        if !entitlement_ok(e) {
                            return None;
                        }
                        push(&["--entitlement", e]);
                    }
                    if let Some(n) = c.priority {
                        push(&["--priority", &n.to_string()]);
                    }
                    if let Some(n) = c.capacity_rps {
                        if n == 0 {
                            return None;
                        }
                        push(&["--capacity-rps", &n.to_string()]);
                    }
                    if c.disabled {
                        push(&["--disabled"]);
                    }
                }
                PROVIDER_REMOVE | PROVIDER_ENABLE | PROVIDER_DISABLE
                    if none && id_ok(&req.resource_id) =>
                {
                    let verb = action.rsplit('.').next()?;
                    push(&[verb, "--id", &req.resource_id]);
                }
                PROVIDER_PRIORITY if req.resource_id.is_empty() => {
                    let c = ProviderPriorityCommand::decode(req.params.as_ref()).ok()?;
                    if !id_ok(&c.id) {
                        return None;
                    }
                    push(&[
                        "priority",
                        "--id",
                        &c.id,
                        "--priority",
                        &c.priority.to_string(),
                    ]);
                }
                PROVIDER_SELECT if req.resource_id.is_empty() => {
                    let c = ProviderSelectCommand::decode(req.params.as_ref()).ok()?;
                    if !KINDS.contains(&c.kind.as_str()) || !id_ok(&c.id) {
                        return None;
                    }
                    push(&["select", "--kind", &c.kind, "--id", &c.id]);
                }
                PROVIDER_UNSELECT if none && KINDS.contains(&req.resource_id.as_str()) => {
                    push(&["unselect", "--kind", &req.resource_id]);
                }
                PROVIDER_QUALIFY if req.resource_id.is_empty() => {
                    let c = ProviderQualifyCommand::decode(req.params.as_ref()).ok()?;
                    let evidence_ok = c.evidence == "-"
                        || (c.evidence.len() == 16
                            && c.evidence
                                .bytes()
                                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
                    if !id_ok(&c.id) || !evidence_ok {
                        return None;
                    }
                    push(&["qualify", "--id", &c.id, "--evidence", &c.evidence]);
                }
                PROVIDER_CREDENTIAL_SET | PROVIDER_CREDENTIAL_ROTATE
                    if req.resource_id.is_empty() =>
                {
                    let c = credential_command(req)?;
                    if !id_ok(&c.credential) {
                        return None;
                    }
                    let verb = action.rsplit('.').next()?;
                    push(&["credential", verb, "--credential", &c.credential]);
                }
                PROVIDER_CREDENTIAL_REVOKE if none && id_ok(&req.resource_id) => {
                    push(&["credential", "revoke", "--credential", &req.resource_id]);
                }
                _ => return None,
            }
            args.extend([
                "--request-id".into(),
                req.request_id.clone(),
                "--expect-version".into(),
                expect_version?.to_string(),
            ]);
            Some(args)
        }
        JOB_CANCEL if req.params.is_empty() && valid_job_id(&req.resource_id) => Some(
            ["p0", "job", "cancel", "--id", &req.resource_id]
                .map(String::from)
                .to_vec(),
        ),
        // DEC-07: a new flow is the request id; its TTL is fixed here (the registry checks the
        // bounds), every later step names that flow and the token it approves.
        BOOT_OFFLINE if req.resource_id.is_empty() => {
            let c = BootOfflineCommand::decode(req.params.as_ref()).ok()?;
            if !["econ4h", "shadow", "market"].contains(&c.mode.as_str())
                || !horizon_ok(&c.duration)
            {
                return None;
            }
            let mut args: Vec<String> = [
                "p0",
                "boot",
                "offline",
                "--flow",
                &req.request_id,
                "--mode",
                &c.mode,
                "--duration",
                &c.duration,
            ]
            .map(String::from)
            .to_vec();
            if let Some(ttl) = c.ttl_seconds {
                args.extend([
                    "--".into(),
                    "--boot-token-ttl-seconds".into(),
                    ttl.to_string(),
                ]);
            }
            Some(args)
        }
        BOOT_NETWORK if req.resource_id.is_empty() => {
            let c = BootNetworkCommand::decode(req.params.as_ref()).ok()?;
            if !valid_job_id(&c.flow_id)
                || !token_ok(&c.approved_token)
                || !genesis_ok(&c.genesis_hash)
                || !evidence_id_ok(&c.provider_evidence)
                || !capture_source_ok(&c.capture_source)
            {
                return None;
            }
            Some(
                [
                    "p0",
                    "boot",
                    "network",
                    "--flow",
                    &c.flow_id,
                    "--approve",
                    &c.approved_token,
                    "--genesis-hash",
                    &c.genesis_hash,
                    "--provider-evidence",
                    &c.provider_evidence,
                    "--capture-source",
                    &c.capture_source,
                ]
                .map(String::from)
                .to_vec(),
            )
        }
        // DEC-07: project0.capture.start (the raw capture) is the start approval of a market
        // flow, the same native step as project0.boot.start.
        BOOT_START | START if req.resource_id.is_empty() => {
            let c = BootStartCommand::decode(req.params.as_ref()).ok()?;
            if !valid_job_id(&c.flow_id)
                || !token_ok(&c.approved_token)
                || !horizon_ok(&c.duration)
                || !capture_source_ok(&c.capture_source)
                || c.qualification.as_deref().is_some_and(|q| !token_ok(q))
                // Online strict verification needs the operator's provider evidence: a terminal
                // action (`p0ctl boot start --chain-verify strict --provider-evidence`), as for
                // project0.capture.verify.start.
                || c.chain_verify != "off"
            {
                return None;
            }
            Some(
                [
                    "p0",
                    "boot",
                    "start",
                    "--flow",
                    &c.flow_id,
                    "--approve",
                    &c.approved_token,
                    "--duration",
                    &c.duration,
                    "--capture-source",
                    &c.capture_source,
                    "--chain-verify",
                    &c.chain_verify,
                ]
                .map(String::from)
                .into_iter()
                // DEC-16: an unbounded flow starts with its qualification (`p0ctl` admits it).
                .chain(
                    c.qualification
                        .into_iter()
                        .flat_map(|q| ["--qualification".to_string(), q]),
                )
                .collect(),
            )
        }
        AUTOSTART_PREVIEW if req.resource_id.is_empty() => {
            let c = AutostartPreviewCommand::decode(req.params.as_ref()).ok()?;
            if !token_ok(&c.qualification)
                || ![
                    &c.checkpoint,
                    &c.minimum_runway,
                    &c.max_runtime,
                    &c.valid_for,
                ]
                .iter()
                .all(|d| duration_ok(d))
                || !genesis_ok(&c.genesis_hash)
                || !capture_source_ok(&c.capture_source)
            {
                return None;
            }
            Some(
                [
                    "p0",
                    "autostart",
                    "preview",
                    "--qualification",
                    &c.qualification,
                    "--checkpoint",
                    &c.checkpoint,
                    "--minimum-runway",
                    &c.minimum_runway,
                    "--genesis-hash",
                    &c.genesis_hash,
                    "--max-runtime",
                    &c.max_runtime,
                    "--valid-for",
                    &c.valid_for,
                    "--capture-source",
                    &c.capture_source,
                ]
                .map(String::from)
                .to_vec(),
            )
        }
        AUTOSTART_GRANT if req.resource_id.is_empty() => {
            let c = AutostartGrantCommand::decode(req.params.as_ref()).ok()?;
            token_ok(&c.confirm_digest).then(|| {
                ["p0", "autostart", "grant", "--confirm", &c.confirm_digest]
                    .map(String::from)
                    .to_vec()
            })
        }
        AUTOSTART_REVOKE if req.resource_id.is_empty() && req.params.is_empty() => {
            Some(["p0", "autostart", "revoke"].map(String::from).to_vec())
        }
        BOOT_CANCEL if req.params.is_empty() && valid_job_id(&req.resource_id) => Some(
            ["p0", "boot", "cancel", "--flow", &req.resource_id]
                .map(String::from)
                .to_vec(),
        ),
        _ => None,
    }
}
/// The native refusal of `p0ctl` (its last error line), bounded and redacted.
fn refusal(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let line = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("no reason reported");
    let code = redact(line.trim().trim_start_matches("p0ctl: "));
    let code: String = code.chars().take(200).collect();
    format!("Native controller refused: {code}")
}

/// The launchd control binary the capture/readiness state is read from (macOS).
pub const LAUNCHCTL: &str = "/bin/launchctl";

#[derive(Clone)]
pub struct Paths {
    pub ctl: PathBuf,
    pub rundir: PathBuf,
    pub logdir: PathBuf,
    pub label: String,
    /// CI-N1: the launchd query binary — `LAUNCHCTL` in production; tests pass a hermetic stand-in
    /// so they never depend on the host's launchd (and run where there is none, e.g. Linux CI).
    pub launchctl: PathBuf,
}
/// Time-bounded idempotency (Wire V1.3 §7), all bounds from `Limits` (`command_*`). Every open
/// record (PENDING / PENDING_RECONCILE) stays in `commands/`. At `command_ledger_records` the
/// oldest terminal receipt (COMPLETED / FAILED, by last update) moves to `commands/archive/`, which
/// replay and conflict checks still read. A receipt leaves the full archive only once its last
/// update is at least `command_retention_ms` old, the one issued earliest first, after
/// `retired_through` durably took its `issued_at_unix_ns`; with none that old, new commands are
/// refused instead of forgetting a younger receipt. A new request is accepted only with
/// `issued_at_unix_ns` within `command_clock_window_ms` of the gateway receive time, so a
/// forgotten request is always outside the window and never runs again; the mark refuses it too.
pub struct Control {
    paths: Option<Paths>,
    serial: Mutex<()>,
    capacity: usize,
    window_ns: u64,
    retention: Duration,
    /// `Limits::launchd_query_timeout_ms`: one read-only `launchctl list` (Project 0), one
    /// `bsc-status-reporter --readiness-status` (BSC, Audit E 4.13).
    launchd_query: Duration,
    /// `Limits::controller_timeout_ms`: one `darkbotctl` dispatch.
    controller: Duration,
}
#[derive(Clone, Serialize)]
pub struct ProcessSnapshot {
    pub state: String,
    pub run_id: String,
    pub pid: Option<u32>,
    pub reason: String,
    pub observed_unix_ms: u64,
    pub controller_available: bool,
}
#[derive(Debug, PartialEq, Deserialize)]
struct Record {
    request: Vec<u8>,
    response: Vec<u8>,
    phase: i32,
    detail: String,
    launch_id: String,
}
/// BSC-051: one ledger record, binary and versioned (`DBCMD001`): the phase (i32 LE), then the
/// request, response, detail and launch ID, each with a u32-LE length, then a CRC-32 (IEEE) of
/// everything before it. Records written before it (`<id>.json`) stay readable and are never
/// written again; the next update of such a record replaces it with its binary form.
mod ledger_record {
    use super::Record;
    use std::path::Path;
    pub const EXT: &str = "cmd";
    pub const LEGACY_EXT: &str = "json";
    const MAGIC: &[u8; 8] = b"DBCMD001";

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for b in bytes {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    pub fn encode(r: &Record) -> Result<Vec<u8>, ()> {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&r.phase.to_le_bytes());
        for field in [
            &r.request[..],
            &r.response,
            r.detail.as_bytes(),
            r.launch_id.as_bytes(),
        ] {
            out.extend_from_slice(&u32::try_from(field.len()).map_err(|_| ())?.to_le_bytes());
            out.extend_from_slice(field);
        }
        let crc = crc32(&out);
        out.extend_from_slice(&crc.to_le_bytes());
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Record, ()> {
        let body_len = bytes.len().checked_sub(4).ok_or(())?;
        let (body, crc) = bytes.split_at(body_len);
        if crc32(body).to_le_bytes() != crc || body.get(..8) != Some(&MAGIC[..]) {
            return Err(());
        }
        let mut rest = &body[8..];
        let mut take = |n: usize| -> Result<&[u8], ()> {
            let (head, tail) = (rest.get(..n).ok_or(())?, rest.get(n..).ok_or(())?);
            rest = tail;
            Ok(head)
        };
        let phase = i32::from_le_bytes(take(4)?.try_into().map_err(|_| ())?);
        let mut field = || -> Result<Vec<u8>, ()> {
            let n = u32::from_le_bytes(take(4)?.try_into().map_err(|_| ())?);
            Ok(take(usize::try_from(n).map_err(|_| ())?)?.to_vec())
        };
        let request = field()?;
        let response = field()?;
        let detail = String::from_utf8(field()?).map_err(|_| ())?;
        let launch_id = String::from_utf8(field()?).map_err(|_| ())?;
        if !rest.is_empty() {
            return Err(());
        }
        Ok(Record {
            request,
            response,
            phase,
            detail,
            launch_id,
        })
    }

    /// A record file of either generation, by its extension.
    pub fn read(path: &Path) -> Result<Record, ()> {
        let bytes = std::fs::read(path).map_err(|_| ())?;
        match path.extension().and_then(|x| x.to_str()) {
            Some(EXT) => decode(&bytes),
            Some(LEGACY_EXT) => serde_json::from_slice(&bytes).map_err(|_| ()),
            _ => Err(()),
        }
    }
}
/// BSC-053: the wall clock in ms; `None` when it is before 1970 (never time 0).
fn now_ms() -> Option<u64> {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(since.as_millis()).ok()
}
/// Stamps a process observation; without a clock it is UNKNOWN, never a state at time 0.
fn observed_now(mut snapshot: ProcessSnapshot) -> ProcessSnapshot {
    stamp(&mut snapshot, now_ms());
    snapshot
}
fn stamp(snapshot: &mut ProcessSnapshot, now: Option<u64>) {
    match now {
        Some(ms) => snapshot.observed_unix_ms = ms,
        None => {
            snapshot.state = "UNKNOWN".into();
            snapshot.reason = "gateway clock unavailable".into();
            snapshot.observed_unix_ms = 0;
        }
    }
}
fn text_file(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_owned()
}
fn field(text: &str, name: &str) -> Option<i32> {
    text.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&format!("\"{name}\" = "))
            .and_then(|s| s.trim_end_matches(';').parse().ok())
    })
}
impl Control {
    pub fn from_env(limits: &Limits) -> Self {
        let paths = (|| {
            Some(Paths {
                ctl: std::env::var_os("DARKBOT_CTL")?.into(),
                rundir: std::env::var_os("DARKBOT_RUNDIR")?.into(),
                logdir: std::env::var_os("DARKBOT_LOGDIR")?.into(),
                label: std::env::var("DARKBOT_CAPTURE_LABEL").ok()?,
                launchctl: LAUNCHCTL.into(),
            })
        })();
        Self::new(paths, limits)
    }
    pub fn new(paths: Option<Paths>, limits: &Limits) -> Self {
        Self {
            paths,
            serial: Mutex::new(()),
            capacity: limits.command_ledger_records,
            window_ns: limits.command_clock_window_ms.saturating_mul(1_000_000),
            retention: Duration::from_millis(limits.command_retention_ms),
            launchd_query: Duration::from_millis(limits.launchd_query_timeout_ms),
            controller: Duration::from_millis(limits.controller_timeout_ms),
        }
    }
    pub async fn observe(&self) -> ProcessSnapshot {
        let mut snapshot = ProcessSnapshot {
            state: "UNKNOWN".into(),
            run_id: String::new(),
            pid: None,
            reason: "Capture controller not configured".into(),
            observed_unix_ms: 0,
            controller_available: false,
        };
        let Some(p) = &self.paths else {
            return snapshot;
        };
        snapshot.controller_available = p.ctl.is_file();
        snapshot.run_id = text_file(&p.rundir.join("capture.launch"));
        // List is read-only. An execution/permission/timeout error is NEVER evidence of STOPPED.
        let output = tokio::time::timeout(
            self.launchd_query,
            Command::new(&p.launchctl)
                .args(["list", &p.label])
                .kill_on_drop(true)
                .output(),
        )
        .await;
        let Ok(Ok(out)) = output else {
            snapshot.reason = "launchd query unavailable".into();
            return snapshot;
        };
        let state = if !out.status.success() {
            let error = String::from_utf8_lossy(&out.stderr);
            if error.contains("Could not find service")
                || error.contains("Could not find specified service")
            {
                CaptureProcess::Stopped
            } else {
                CaptureProcess::Unknown
            }
        } else {
            let text = String::from_utf8_lossy(&out.stdout);
            snapshot.pid = field(&text, "PID").filter(|n| *n > 0).map(|n| n as u32);
            let stopping = text_file(&p.rundir.join("capture.stopping"));
            let user_stop = text_file(&p.rundir.join("capture.user_stopped"));
            let matched = !snapshot.run_id.is_empty()
                && (stopping == snapshot.run_id
                    || user_stop
                        .split_whitespace()
                        .any(|s| s == format!("launch={}", snapshot.run_id)));
            let result = text_file(&p.rundir.join("capture.result"));
            let completed = result
                .lines()
                .any(|s| s == format!("launch_id={}", snapshot.run_id))
                && result.lines().any(|s| {
                    matches!(
                        s,
                        "code=CAPTURE_COMPLETE" | "code=MARKET_CAPTURE_INCOMPLETE"
                    )
                });
            classify(
                snapshot.pid,
                field(&text, "LastExitStatus"),
                matched,
                completed,
            )
        };
        snapshot.state = state.as_str().into();
        snapshot.reason = format!("launchd: {}", p.label);
        observed_now(snapshot)
    }
    pub async fn observe_project(&self, project: i32) -> ProcessSnapshot {
        if project != ProjectId::Bsc as i32 {
            return self.observe().await;
        }
        let mut s = ProcessSnapshot {
            state: "UNKNOWN".into(),
            run_id: String::new(),
            pid: None,
            reason: "BSC controller unavailable".into(),
            observed_unix_ms: 0,
            controller_available: false,
        };
        let Some(p) = &self.paths else {
            return s;
        };
        let label = p
            .label
            .strip_suffix(".capture")
            .map(|x| format!("{x}.bsc.readiness"))
            .unwrap_or_default();
        if label.is_empty() {
            return s;
        }
        // Audit E 4.13: the job's state from its one native owner, the deployed reporter, as
        // its typed status (it reads launchd and the job's typed marks); no job file read here.
        // Without an answer within the launchd bound the job reads UNKNOWN, uncontrollable.
        let Some(current) = p.ctl.parent() else {
            return s;
        };
        let out = tokio::time::timeout(
            self.launchd_query,
            Command::new(current.join("bsc-status-reporter"))
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("DARKBOT_BSC_ROOT", current.join("bsc-root"))
                .env("DARKBOT_RUNDIR", &p.rundir)
                .env(
                    "DARKBOT_BSC_READINESS_BIN",
                    current.join("g07_pre_live_readiness"),
                )
                .args(["--readiness-status", &label])
                .kill_on_drop(true)
                .output(),
        )
        .await;
        let job = match out {
            Ok(Ok(out)) if out.status.success() => {
                crate::proto::bsc::v1::JobStatus::decode(out.stdout.as_slice()).ok()
            }
            _ => None,
        };
        if let Some(job) = job {
            s.state = job.state;
            s.run_id = job.launch_id;
            s.pid = job.pid;
            s.controller_available = job.controller_available.unwrap_or(false);
            s.reason = format!("launchd: {label}; result {}", job.result_code);
        }
        observed_now(s)
    }
    fn record_path(&self, id: &str) -> Option<PathBuf> {
        self.paths.as_ref().map(|p| {
            p.rundir
                .join("commands")
                .join(format!("{id}.{}", ledger_record::EXT))
        })
    }
    /// The record of `id`: binary before legacy, open ledger before archive. A file that exists
    /// but cannot be read or decoded makes the ledger unreadable (never "no record").
    fn load(&self, id: &str) -> Result<Option<Record>, ()> {
        let path = self.record_path(id).ok_or(())?;
        let dir = path.parent().ok_or(())?;
        for d in [dir.to_path_buf(), dir.join("archive")] {
            for ext in [ledger_record::EXT, ledger_record::LEGACY_EXT] {
                let p = d.join(format!("{id}.{ext}"));
                match fs::symlink_metadata(&p) {
                    Ok(_) => return ledger_record::read(&p).map(Some),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return Err(()),
                }
            }
        }
        Ok(None)
    }
    fn persist(&self, id: &str, record: &Record) -> Result<(), ()> {
        let path = self.record_path(id).ok_or(())?;
        let dir = path.parent().ok_or(())?;
        let legacy = dir.join(format!("{id}.{}", ledger_record::LEGACY_EXT));
        fs::create_dir_all(dir).map_err(|_| ())?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|_| ())?;
        if !path.exists() && !legacy.exists() && records(dir)?.len() >= self.capacity {
            archive_oldest_terminal(dir, self.capacity, self.retention)?;
        }
        let bytes = ledger_record::encode(record)?;
        let tmp = path.with_extension(format!("tmp-{}", session::random_token(8)));
        let written = (|| {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)
                .map_err(|_| ())?;
            f.write_all(&bytes).map_err(|_| ())?;
            f.sync_all().map_err(|_| ())?;
            fs::rename(&tmp, &path).map_err(|_| ())?;
            // The binary form replaces a legacy record of the same id.
            match fs::remove_file(&legacy) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(()),
                _ => {}
            }
            fs::File::open(dir)
                .and_then(|f| f.sync_all())
                .map_err(|_| ())
        })();
        if written.is_err() {
            let _ = fs::remove_file(tmp);
        }
        written
    }
    async fn execute(&self, req: &CommandRequest) -> CommandResponse {
        let _guard = self.serial.lock().await;
        let id = &req.request_id;
        if !valid_id(id) {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::InvalidParams,
                "request_id must be a UUID",
            );
        }
        if !allowed(req.project, &req.action) {
            return reply(
                id,
                CommandStatus::NotImplemented,
                CommandRejectReason::Forbidden,
                "Only managed Project 0 capture, its offline analytics jobs, BSC read-only readiness start/stop, the offline BSC candidate request, the BSC owner's source, venue, wallet, economic, limit and LIVE grant requests and the LIVE ledger's lot, nonce burn and quarantine decisions are available",
            );
        }
        let source = if req.action == BSC_LIVE_GRANT {
            match grant_request(req) {
                Some(args) => Some(args),
                None => {
                    return reply(
                        id,
                        CommandStatus::Rejected,
                        CommandRejectReason::InvalidParams,
                        "LIVE grant takes exactly the resource BSC_LIVE_GRANT, a BscLiveGrantCommand (venue, strategies, ingress sources and builder adapters from the fixed sets, plain decimal wei amounts, the execution risk and the limits, the confirmed scope SHA-256; every bound is the native owner's to decide) and the config_version generation",
                    );
                }
            }
        } else if req.action == BSC_STRATEGY_SET {
            match strategy_request(req) {
                Some(args) => Some(args),
                None => {
                    return reply(
                        id,
                        CommandStatus::Rejected,
                        CommandRejectReason::InvalidParams,
                        "Strategy C setting takes exactly <BSC_STRATEGY_C_OBSERVATION_MS|FILTERS|INPUT_WEI|TARGET_NET_MULTIPLE|STOP_NET_MULTIPLE|HOLD_MS|MAX_OPEN|CREATOR_SOLD_BEFORE_FILL|CREATOR_SELL_EXIT>:<value, printable ASCII without spaces> and the profile_version generation; the runtime configuration decides the value",
                    );
                }
            }
        } else if BSC_LIVE_DECISIONS.contains(&req.action.as_str()) {
            match live_decision(req) {
                Some(args) => Some(args),
                None => {
                    return reply(
                        id,
                        CommandStatus::Rejected,
                        CommandRejectReason::InvalidParams,
                        match req.action.as_str() {
                            BSC_LIVE_LOT => "LIVE lot decision takes exactly 0x<lot, 64 lowercase hex>:<RESUME|WRITE_OFF|EXIT_NOW> and the ledger version as config_version",
                            BSC_LIVE_NONCE_BURN => "LIVE nonce burn takes exactly 0x<transaction, 64 lowercase hex> and the ledger version as config_version",
                            _ => "LIVE ledger quarantine takes exactly <the entry's sequence number> and the config_version generation",
                        },
                    );
                }
            }
        } else if BSC_OWNER_ACTIONS.contains(&req.action.as_str()) {
            match source_request(req).map(|(owner_id, mode, g)| {
                ["bsc", "source", "request", owner_id]
                    .into_iter()
                    .map(String::from)
                    .chain([mode, g.to_string()])
                    .collect::<Vec<String>>()
            }) {
                Some(s) => Some(s),
                None => {
                    return reply(
                        id,
                        CommandStatus::Rejected,
                        CommandRejectReason::InvalidParams,
                        match req.action.as_str() {
                            BSC_WALLET => "Wallet request takes exactly <BSC_WALLET_LAUNCHER|BSC_WALLET_BUYER>:<DISABLED|ENABLED> and the wallet_policy_version generation",
                            BSC_ECONOMIC => "Economic request takes exactly <BSC_ECON_SIGN|ARM|SEND|ATTEMPT|LIVE>:<DISABLED|ENABLED> and the config_version generation",
                            BSC_VENUE => "Venue request takes exactly <BSC_VENUE_FLAP|FOUR_CLASSIC|OPENFOUR>:<OFF|SHADOW|LIVE> and the config_version generation",
                            BSC_RUNTIME => "Runtime request takes exactly BSC_RUNTIME_MODE:<OFF|SHADOW|PAPER|LIVE> and the config_version generation",
                            BSC_ATTEMPT_LIMIT => "Limit request takes exactly BSC_BUY_ATTEMPTS_PER_DAY:<decimal count per UTC day> and the config_version generation",
                            BSC_ATTEMPT_TOTAL => "Total limit request takes exactly BSC_BUY_ATTEMPTS_TOTAL:<decimal count for the campaign> and the config_version generation",
                            BSC_LIVE_REVOKE => "LIVE revoke takes exactly BSC_LIVE_GRANT:DISABLED and the config_version generation",
                            _ => "Source request takes exactly <SOURCE_ID>:<OFF|SHADOW|LIVE> and the config_version generation",
                        },
                    )
                }
            }
        } else {
            None
        };
        let candidate = if req.action == BSC_CANDIDATE {
            match candidate_request(req) {
                Some(c) => Some(c),
                None => {
                    return reply(
                        id,
                        CommandStatus::Rejected,
                        CommandRejectReason::InvalidParams,
                        "Candidate request takes exactly SELF_A or SELF_B and the profile_version generation",
                    )
                }
            }
        } else {
            None
        };
        if online_history_read(req) {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::GateBlocked,
                "An online chain verification needs the operator's provider evidence: p0ctl job start post-capture --provider-evidence FILE (terminal); the GUI verifies off, or index|strict|crosscheck from the checkpoint",
            );
        }
        // DEC-07: project0.capture.start with the start approval of a market flow is the boot
        // controller's start step; a bare one is refused before dispatch (below).
        let job = if JOB_ACTIONS.contains(&req.action.as_str())
            || PROVIDER_ACTIONS.contains(&req.action.as_str())
            || (req.action == START
                && req.project == ProjectId::Project0 as i32
                && !req.params.is_empty())
        {
            match job_args(req) {
                Some(args) => Some(args),
                None => {
                    return reply(
                        id,
                        CommandStatus::Rejected,
                        CommandRejectReason::InvalidParams,
                        if PROVIDER_ACTIONS.contains(&req.action.as_str()) {
                            "Provider request invalid (ids [a-z0-9_-]{1,32}, kind rpc|wss|geyser|jupiter|jito, an endpoint without a credential in it, auth none|path|query:NAME|header:NAME, plan free|unknown, the registry version and a credential's secret; never echoed)"
                        } else if req.action == CAPABILITY_SET {
                            "Capability request invalid (LIVE OFF|LIVE, HOLD_B, JITO_BUNDLE, THRESHOLD_TRIGGER, INVENTORY_REBALANCING or LOCAL_FEE_MODEL OFF|OBSERVE|ACTIVE, ECON_* or WALLET:<pubkey> ENABLED|DISABLED, FE06_GRANT|FE06_REVOKE <digest>; the configuration version, and the owner epoch for all but a move to OFF)"
                        } else if BOOT_ACTIONS.contains(&req.action.as_str()) || req.action == START
                        {
                            "Boot request invalid (mode econ4h|shadow|market, flow id, 16-character token, genesis pin, provider evidence id, capture source, duration, chain verify off)"
                        } else {
                            "Analytics job parameters invalid (run id, exact USD, capital, funding classes, budget, grid or job id)"
                        },
                    )
                }
            }
        } else {
            None
        };
        if candidate.is_none()
            && job.is_none()
            && source.is_none()
            && ((req.project != ProjectId::Bsc as i32 && !req.resource_id.is_empty())
                || (req.project == ProjectId::Bsc as i32
                    && !req.resource_id.is_empty()
                    && !valid_id(&req.resource_id))
                || req.expected.is_some()
                || !req.params.is_empty())
        {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::InvalidParams,
                "This capture action takes no resource, version or parameters",
            );
        }
        let Some(p) = &self.paths else {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::EngineUnavailable,
                "Capture controller not configured",
            );
        };
        // DEC-18 (P0-019): a credential's secret never enters the ledger — the request is
        // recorded without it, the secret goes to the controller's standard input only.
        let Some((stored, secret)) = separate_secret(req) else {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::InvalidParams,
                "Credential request invalid: a credential id and its secret",
            );
        };
        let encoded = stored.encode_to_vec();
        drop(stored);
        match self.load(id) {
            Ok(Some(record)) if record.request == encoded => {
                // A replay returns the stored receipt with the current ledger phase.
                return CommandResponse::decode(record.response.as_slice())
                    .map(|r| CommandResponse {
                        phase: record.phase,
                        ..r
                    })
                    .unwrap_or_else(|_| {
                        reply(
                            id,
                            CommandStatus::Rejected,
                            CommandRejectReason::EngineUnavailable,
                            "Command receipt unreadable",
                        )
                    });
            }
            Ok(Some(_)) => {
                return reply(
                    id,
                    CommandStatus::Rejected,
                    CommandRejectReason::IdempotencyConflict,
                    "request_id already belongs to a different payload",
                )
            }
            Err(_) => {
                return reply(
                    id,
                    CommandStatus::Rejected,
                    CommandRejectReason::EngineUnavailable,
                    "Command ledger unreadable",
                )
            }
            _ => {}
        }
        // Replay rule (see Control): a new request must be issued within the clock window and after
        // every forgotten receipt.
        if let Err((reason, detail)) = clock_window(
            req.issued_at_unix_ns,
            crate::overview::unix_ns(),
            self.window_ns,
        ) {
            return reply(id, CommandStatus::Rejected, reason, &detail);
        }
        let ledger = self
            .record_path(id)
            .and_then(|p| p.parent().map(Path::to_path_buf));
        match ledger.map(|d| retired_through(&d)) {
            Some(Ok(Some(mark))) if req.issued_at_unix_ns <= mark => {
                return reply(
                    id,
                    CommandStatus::Rejected,
                    CommandRejectReason::IdempotencyConflict,
                    "Expired: issued no later than a retired receipt; never executed again, issue a new command",
                )
            }
            Some(Err(())) | None => {
                return reply(
                    id,
                    CommandStatus::Rejected,
                    CommandRejectReason::EngineUnavailable,
                    "Command ledger unreadable",
                )
            }
            _ => {}
        }
        if let Some(args) = job {
            return self
                .dispatch_job(p, id, encoded, &req.action, args, secret)
                .await;
        }
        drop(secret);
        if let Some(source) = source {
            let ledger = BSC_LIVE_DECISIONS.contains(&req.action.as_str());
            return self.dispatch_source(p, id, encoded, source, ledger).await;
        }
        // DEC-07: a raw capture reads Mainnet, so it starts only through the boot controller with
        // the start approval of its READY market flow; without one nothing is called.
        if req.action == START && req.project == ProjectId::Project0 as i32 {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::GateBlocked,
                "BOOT_AUDIT_REQUIRED: a raw capture starts only through the boot flow (mode market): offline preflight, network approval, audit, start approval",
            );
        }
        let before = self.observe_project(req.project).await;
        // Audit E 4.13 review M1: an unknown BSC state (no answer of the job's owner) is no
        // launch change; it is refused below as unknown.
        if req.project == ProjectId::Bsc as i32
            && candidate.is_none()
            && before.state != "UNKNOWN"
            && req.resource_id != before.run_id
        {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::VersionConflict,
                "Readiness launch changed; refresh process state",
            );
        }
        if before.state == "UNKNOWN"
            || ([START, BSC_START].contains(&req.action.as_str()) && before.state == "STOPPING")
        {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::GateBlocked,
                "Capture state unknown or stopping; no start issued",
            );
        }
        let accepted = reply(
            id,
            CommandStatus::Accepted,
            CommandRejectReason::Unspecified,
            "Capture intent persisted; execution is tracked separately",
        );
        let mut record = Record {
            request: encoded,
            response: accepted.encode_to_vec(),
            phase: CommandPhase::PendingReconcile as i32,
            detail: "Controller dispatch pending; never automatically replayed after interruption"
                .into(),
            launch_id: before.run_id,
        };
        if self.persist(id, &record).is_err() {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::EngineUnavailable,
                "Cannot durably record command (ledger full within the idempotency window, or unavailable); retry later",
            );
        }
        let generation = candidate.map(|(_, g)| g.to_string()).unwrap_or_default();
        let args = if let Some((profile, _)) = candidate {
            vec!["bsc", "candidate", "request", profile, &generation, id]
        } else if req.action == BSC_START {
            vec!["bsc", "readiness", "start", &req.resource_id]
        } else if req.action == BSC_STOP {
            vec!["bsc", "readiness", "stop", &req.resource_id]
        } else if req.action == START {
            vec!["capture", "start-configured"]
        } else {
            vec!["capture", "stop", "--signal-only"]
        };
        let output = tokio::time::timeout(
            self.controller,
            Command::new(&p.ctl)
                .args(args)
                .env("DARKBOT_STAGED", "1")
                .env("DARKBOT_NO_OPEN", "1")
                .kill_on_drop(true)
                .output(),
        )
        .await;
        let after = self.observe_project(req.project).await;
        match output {
            Ok(Ok(out)) if out.status.success() && candidate.is_some() => {
                record.phase = CommandPhase::Completed as i32;
                record.detail = "Selection request recorded; the native store applies or rejects it at the next desktop bsc sync".into();
            }
            Ok(Ok(_)) if candidate.is_some() => {
                record.phase = CommandPhase::Failed as i32;
                record.detail = "Native candidate request refused (stale generation, unknown candidate, armed guard or a pending request)".into();
            }
            Ok(Ok(out)) if out.status.success() => {
                if [START, BSC_START].contains(&req.action.as_str()) {
                    record.launch_id = after.run_id;
                    record.phase = if after.state == "RUNNING"
                        || (req.action == BSC_START && after.state == "STOPPED")
                    {
                        CommandPhase::Completed
                    } else {
                        CommandPhase::Failed
                    } as i32;
                    record.detail = if after.state == "RUNNING"
                        || (req.action == BSC_START && after.state == "STOPPED")
                    {
                        "Managed process started (a short read-only job may already be complete)"
                    } else {
                        "Capture did not reach RUNNING; inspect logs"
                    }
                    .into();
                } else {
                    record.phase = match after.state.as_str() {
                        "STOPPED" => CommandPhase::Completed,
                        "FAILED" => CommandPhase::Failed,
                        _ => CommandPhase::Pending,
                    } as i32;
                    record.detail =
                        "Orderly stop requested; journal drain/seal remains owned by capture"
                            .into();
                }
            }
            Ok(Ok(_)) => {
                record.phase = CommandPhase::Failed as i32;
                record.detail =
                    "Controller rejected command; inspect Project 0 / gateway logs".into();
            }
            _ => {
                record.phase = CommandPhase::PendingReconcile as i32;
                record.detail =
                    "Controller outcome unknown; inspect process state before a new action".into();
            }
        }
        // Receipt remains durable even if this final update fails; a retry cannot redispatch it.
        if self.persist(id, &record).is_err() {
            tracing::error!(request_id=%id, "command outcome could not be persisted");
        }
        tracing::info!(request_id=%id, action=%req.action, phase=record.phase, "capture command");
        // The ledger phase known now; later progress is visible in the domain STATE.
        CommandResponse {
            phase: record.phase,
            ..accepted
        }
    }
    /// B03/U-9: an analytics job action through `darkbotctl p0 job …` (the same `p0ctl job`
    /// a terminal runs). No capture process is observed: COMPLETED means the controller recorded
    /// the job and started its worker (or signalled it to stop); the job's own progress and end
    /// are its state in project0.analytics.jobs, never this receipt.
    async fn dispatch_job(
        &self,
        p: &Paths,
        id: &str,
        encoded: Vec<u8>,
        action: &str,
        args: Vec<String>,
        secret: Option<SecretBuf>,
    ) -> CommandResponse {
        let accepted = reply(
            id,
            CommandStatus::Accepted,
            CommandRejectReason::Unspecified,
            if PROVIDER_ACTIONS.contains(&action) {
                "Provider change intent persisted; its receipt by request id, the registry in project0.analytics.jobs (providers)"
            } else {
                "Analytics job intent persisted; the job's progress is project0.analytics.jobs"
            },
        );
        let mut record = Record {
            request: encoded,
            response: accepted.encode_to_vec(),
            phase: CommandPhase::PendingReconcile as i32,
            detail: "Controller dispatch pending; never automatically replayed after interruption"
                .into(),
            launch_id: String::new(),
        };
        if self.persist(id, &record).is_err() {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::EngineUnavailable,
                "Cannot durably record command (ledger full within the idempotency window, or unavailable); retry later",
            );
        }
        let output = tokio::time::timeout(self.controller, async {
            let mut ctl = Command::new(&p.ctl);
            ctl.args(&args).kill_on_drop(true);
            // DEC-07: the boot controller audits the staged capture environment, as the
            // capture start does.
            if BOOT_ACTIONS.contains(&action)
                || AUTOSTART_ACTIONS.contains(&action)
                || action == START
            {
                ctl.env("DARKBOT_STAGED", "1");
            }
            let Some(mut secret) = secret else {
                // A controller never reads the gateway's own standard input.
                ctl.stdin(std::process::Stdio::null());
                return ctl.output().await;
            };
            // DEC-18 (P0-019): the secret through the controller's standard input only (never an
            // argument, the environment or a record); wiped when this ends.
            use tokio::io::AsyncWriteExt;
            ctl.stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            let mut child = ctl.spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                secret.0.push(b'\n');
                let written = stdin.write_all(&secret.0).await;
                drop(stdin);
                drop(secret);
                written?;
            }
            child.wait_with_output().await
        })
        .await;
        // DEC-18: the registry's receipt (`PROVIDER_RECEIPT …`, redacted): its outcome either way.
        let provider_receipt = |out: &std::process::Output| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .find(|l| l.starts_with("PROVIDER_RECEIPT "))
                .map(|l| redact(l.trim()).chars().take(300).collect::<String>())
        };
        let provider = PROVIDER_ACTIONS.contains(&action);
        match output {
            Ok(Ok(out)) if out.status.success() && provider => {
                record.phase = CommandPhase::Completed as i32;
                record.detail = format!(
                    "Provider change recorded, effective at the next controlled start (project0.analytics.jobs providers): {}",
                    provider_receipt(&out).unwrap_or_else(|| "no receipt line".into())
                );
            }
            Ok(Ok(out)) if provider => {
                record.phase = CommandPhase::Failed as i32;
                record.detail = provider_receipt(&out)
                    .map(|r| format!("Native controller refused: {r}"))
                    .unwrap_or_else(|| refusal(&out.stderr));
            }
            Ok(Ok(out)) if out.status.success() => {
                record.phase = CommandPhase::Completed as i32;
                record.detail = if action == JOB_CANCEL {
                    "Stop requested from the job's worker; it records CANCELLED in project0.analytics.jobs"
                } else if action == SOURCE_SET {
                    "Source request recorded and the runtime rung; the owner's decision follows in project0.liquidator.state"
                } else if action == CAPABILITY_SET {
                    "Capability request recorded and the runtime rung; the owner's decision follows in project0.liquidator.state (LIVE stays BLOCKED until FE-06)"
                } else if action == VERIFY_START {
                    "Post-capture job recorded and its worker started; the run's qualification follows in project0.analytics.jobs"
                } else if action == BOOT_OFFLINE {
                    "Boot flow recorded with its offline preflight; verdict, token and TTL follow in project0.analytics.jobs (boot)"
                } else if action == BOOT_NETWORK {
                    "Network audit approved and its worker started; record and TTL window follow in project0.analytics.jobs (boot)"
                } else if action == BOOT_START || action == START {
                    "Start approved and its worker started; the run follows in project0.analytics.jobs (boot)"
                } else if action == BOOT_CANCEL {
                    "Stop requested from the boot step's worker; it records CANCELLED in project0.analytics.jobs (boot)"
                } else if action == AUTOSTART_PREVIEW {
                    "Standing approval previewed; its binding and digest follow in project0.analytics.jobs (until_stopped.preview)"
                } else if action == AUTOSTART_GRANT {
                    "Standing approval granted for exactly the previewed binding; project0.analytics.jobs (until_stopped.autostart)"
                } else if action == AUTOSTART_REVOKE {
                    "Standing approval revoked; no autostart until a new grant"
                } else {
                    "Job recorded and its worker started; progress in project0.analytics.jobs"
                }
                .into();
            }
            Ok(Ok(out)) => {
                record.phase = CommandPhase::Failed as i32;
                record.detail = refusal(&out.stderr);
            }
            _ => {
                record.detail = if provider {
                    "Controller outcome unknown; inspect the provider registry in project0.analytics.jobs before a new request (this request id stays PendingReconcile here; the registry it changed or not: p0ctl provider status, project0.analytics.jobs)"
                } else if action == CAPABILITY_SET {
                    "Controller outcome unknown; inspect the LIVE capability in project0.liquidator.state before a new request"
                } else if BOOT_ACTIONS.contains(&action) || action == START {
                    "Controller outcome unknown; inspect the boot flow in project0.analytics.jobs before a new approval"
                } else if AUTOSTART_ACTIONS.contains(&action) {
                    "Controller outcome unknown; inspect the standing approval in project0.analytics.jobs (until_stopped) before a new request"
                } else {
                    "Controller outcome unknown; inspect project0.analytics.jobs before a new action"
                }
                .into();
            }
        }
        if self.persist(id, &record).is_err() {
            tracing::error!(request_id=%id, "command outcome could not be persisted");
        }
        tracing::info!(request_id=%id, %action, phase=record.phase, "analytics job command");
        CommandResponse {
            phase: record.phase,
            detail: record.detail,
            ..accepted
        }
    }
    /// One owner request through the native owner (`darkbotctl` with `args` and the request
    /// id); its one-line receipt becomes the detail.
    async fn dispatch_source(
        &self,
        p: &Paths,
        id: &str,
        encoded: Vec<u8>,
        args: Vec<String>,
        ledger: bool,
    ) -> CommandResponse {
        // R3b review N1: an operator's LIVE ledger decision is the native ledger's, not the
        // owner's.
        let accepted = reply(
            id,
            CommandStatus::Accepted,
            CommandRejectReason::Unspecified,
            match ledger {
                true => "LIVE ledger decision persisted; the native ledger decides",
                false => "Source request persisted; the native owner decides",
            },
        );
        let mut record = Record {
            request: encoded,
            response: accepted.encode_to_vec(),
            phase: CommandPhase::PendingReconcile as i32,
            detail: "Controller dispatch pending; never automatically replayed after interruption"
                .into(),
            launch_id: String::new(),
        };
        if self.persist(id, &record).is_err() {
            return reply(
                id,
                CommandStatus::Rejected,
                CommandRejectReason::EngineUnavailable,
                "Cannot durably record command (ledger full within the idempotency window, or unavailable); retry later",
            );
        }
        let output = tokio::time::timeout(
            self.controller,
            Command::new(&p.ctl)
                .args(&args)
                .arg(id)
                .env("DARKBOT_STAGED", "1")
                .env("DARKBOT_NO_OPEN", "1")
                .kill_on_drop(true)
                .output(),
        )
        .await;
        let receipt = |out: &std::process::Output| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .find(|l| l.starts_with("outcome="))
                .map(|l| redact(l.trim()).chars().take(300).collect::<String>())
        };
        match output {
            Ok(Ok(out)) if out.status.success() => {
                record.phase = CommandPhase::Completed as i32;
                record.detail =
                    receipt(&out).unwrap_or_else(|| "outcome=APPLIED (no receipt line)".into());
            }
            Ok(Ok(out)) => {
                record.phase = CommandPhase::Failed as i32;
                record.detail = receipt(&out).unwrap_or_else(|| refusal(&out.stderr));
            }
            _ => {
                record.detail = format!(
                    "Controller outcome unknown; {}",
                    match ledger {
                        true => "the native ledger decides",
                        false => "the native source state decides",
                    }
                );
            }
        }
        if self.persist(id, &record).is_err() {
            tracing::error!(request_id=%id, "command outcome could not be persisted");
        }
        tracing::info!(request_id=%id, phase=record.phase, request=%args[..args.len().min(4)].join(" "), ledger, "bsc native request");
        CommandResponse {
            phase: record.phase,
            detail: record.detail,
            ..accepted
        }
    }
    async fn lookup(&self, id: &str, project: i32) -> CommandLookupResponse {
        let _guard = self.serial.lock().await;
        let Some(mut record) = self.load(id).ok().flatten() else {
            return CommandLookupResponse {
                found: false,
                command_id: String::new(),
                phase: CommandPhase::Unknown as i32,
                detail: "No recorded command".into(),
            };
        };
        let request = CommandRequest::decode(record.request.as_slice()).ok();
        if request.as_ref().map(|r| r.project) != Some(project) {
            return CommandLookupResponse {
                found: false,
                command_id: String::new(),
                phase: CommandPhase::Unknown as i32,
                detail: "No recorded command for project".into(),
            };
        }
        if record.phase == CommandPhase::Pending as i32 {
            let state = self.observe_project(project).await;
            if project == ProjectId::Bsc as i32 && state.state == "UNKNOWN" {
                // Audit E 4.13 review M1: nothing known (no answer of the job's owner): the
                // stop stays pending, never closed as replaced.
            } else if state.run_id != record.launch_id {
                record.phase = CommandPhase::Completed as i32;
                record.detail = "Target launch replaced; no signal sent to replacement".into();
            } else if state.state == "STOPPED" || state.state == "FAILED" {
                record.phase = if state.state == "STOPPED" {
                    CommandPhase::Completed
                } else {
                    CommandPhase::Failed
                } as i32;
                record.detail = format!("Target capture {}", state.state);
            }
            let _ = self.persist(id, &record);
        }
        CommandLookupResponse {
            found: true,
            command_id: id.into(),
            phase: record.phase,
            detail: record.detail,
        }
    }
}
/// Replay rule (see Control): a new request is issued within `window_ns` of the gateway receive
/// time. Without a gateway clock (BSC-053) no request is inside the window.
fn clock_window(
    issued_at_unix_ns: i64,
    now: Option<i64>,
    window_ns: u64,
) -> Result<(), (CommandRejectReason, String)> {
    let Some(now) = now else {
        return Err((
            CommandRejectReason::EngineUnavailable,
            "Gateway clock unavailable; nothing was executed".into(),
        ));
    };
    if issued_at_unix_ns.abs_diff(now) > window_ns {
        return Err((
            CommandRejectReason::InvalidParams,
            format!(
                "issued_at differs from the gateway clock by more than {} ms (clock skew or a stale request); issue a new command",
                window_ns / 1_000_000
            ),
        ));
    }
    Ok(())
}
/// The ledger records of one directory with their last update time.
fn records(dir: &Path) -> Result<Vec<(SystemTime, PathBuf)>, ()> {
    Ok(fs::read_dir(dir)
        .map_err(|_| ())?
        .filter_map(Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x == ledger_record::EXT || x == ledger_record::LEGACY_EXT)
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect())
}
fn terminal(path: &Path) -> bool {
    ledger_record::read(path).is_ok_and(|r| {
        r.phase == CommandPhase::Completed as i32 || r.phase == CommandPhase::Failed as i32
    })
}
fn oldest_terminal(dir: &Path) -> Result<Option<PathBuf>, ()> {
    let mut all = records(dir)?;
    all.sort();
    Ok(all.into_iter().map(|(_, p)| p).find(|p| terminal(p)))
}
fn issued_at(path: &Path) -> Option<i64> {
    let record = ledger_record::read(path).ok()?;
    CommandRequest::decode(record.request.as_slice())
        .ok()
        .map(|r| r.issued_at_unix_ns)
}
const RETIRED_THROUGH: &str = "retired_through";
/// The latest `issued_at_unix_ns` of every receipt removed from the archive; None before the first.
fn retired_through(dir: &Path) -> Result<Option<i64>, ()> {
    match fs::read_to_string(dir.join("archive").join(RETIRED_THROUGH)) {
        Ok(t) => t.trim().parse().map(Some).map_err(|_| ()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    }
}
/// Removes the archived receipt issued earliest among those last updated at least `window` ago;
/// the mark is durable before the receipt is gone. None that old: refuse (Err), never forget early.
fn retire_earliest(dir: &Path, archive: &Path, window: Duration) -> Result<(), ()> {
    let cutoff = SystemTime::now().checked_sub(window).ok_or(())?;
    let (issued, victim) = records(archive)?
        .into_iter()
        .filter(|(t, p)| *t <= cutoff && terminal(p))
        .filter_map(|(t, p)| Some((issued_at(&p)?, t, p)))
        .min()
        .map(|(i, _, p)| (i, p))
        .ok_or(())?;
    let mark = retired_through(dir)?.map_or(issued, |m| m.max(issued));
    let path = archive.join(RETIRED_THROUGH);
    let tmp = path.with_extension(format!("tmp-{}", session::random_token(8)));
    let written = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|_| ())?;
        f.write_all(mark.to_string().as_bytes()).map_err(|_| ())?;
        f.sync_all().map_err(|_| ())?;
        fs::rename(&tmp, &path).map_err(|_| ())?;
        fs::File::open(archive)
            .and_then(|f| f.sync_all())
            .map_err(|_| ())
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
        return written;
    }
    fs::remove_file(victim).map_err(|_| ())
}
/// Frees one ledger slot without touching an open intent; see Control.
fn archive_oldest_terminal(dir: &Path, capacity: usize, window: Duration) -> Result<(), ()> {
    let source = oldest_terminal(dir)?.ok_or(())?;
    let archive = dir.join("archive");
    fs::create_dir_all(&archive).map_err(|_| ())?;
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o700)).map_err(|_| ())?;
    if records(&archive)?.len() >= capacity {
        retire_earliest(dir, &archive, window)?;
    }
    fs::rename(&source, archive.join(source.file_name().ok_or(())?)).map_err(|_| ())?;
    for d in [&archive, &dir.to_path_buf()] {
        fs::File::open(d)
            .and_then(|f| f.sync_all())
            .map_err(|_| ())?;
    }
    Ok(())
}
fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
fn reply(
    id: &str,
    status: CommandStatus,
    reject: CommandRejectReason,
    detail: &str,
) -> CommandResponse {
    CommandResponse {
        status: status as i32,
        request_id: id.into(),
        command_id: if status == CommandStatus::Accepted {
            id.into()
        } else {
            String::new()
        },
        reject_reason: reject as i32,
        detail: detail.into(),
        persisted: None,
        phase: CommandPhase::Unspecified as i32,
    }
}
fn authorized(app: &AppState, h: &HeaderMap, mutate: bool) -> Result<(), StatusCode> {
    session::check_fetch_site(h).map_err(|_| StatusCode::FORBIDDEN)?;
    if mutate || h.contains_key("origin") {
        session::check_origin(h, &app.cfg).map_err(|_| StatusCode::FORBIDDEN)?;
    }
    let token = session::cookie_value(h, &app.cfg.session_cookie)
        .filter(|t| app.sessions.validate(t))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if mutate {
        let expected = app.sessions.csrf_of(&token).ok_or(StatusCode::FORBIDDEN)?;
        let sent = h
            .get("x-csrf-token")
            .and_then(|s| s.to_str().ok())
            .ok_or(StatusCode::FORBIDDEN)?;
        if sent != expected {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(())
}
fn protobuf(msg: impl Message) -> Response {
    (
        [
            (CONTENT_TYPE, "application/x-protobuf"),
            (axum::http::header::CACHE_CONTROL, "no-store"),
        ],
        msg.encode_to_vec(),
    )
        .into_response()
}
pub async fn command(State(app): State<Arc<AppState>>, h: HeaderMap, body: Bytes) -> Response {
    let body = WipedBody(body);
    let refused = match authorized(&app, &h, true) {
        Err(s) => Some(s),
        Ok(())
            if h.get(CONTENT_TYPE).and_then(|v| v.to_str().ok())
                != Some("application/x-protobuf") =>
        {
            Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        }
        Ok(()) => None,
    };
    // The header values are views of the same read buffer as the body: released first, so the
    // body can be overwritten when it is released — after a refusal as after the command (best
    // effort — hyper's connection buffer may still hold the bytes; review L1, P0-020 nit).
    drop(h);
    if let Some(status) = refused {
        drop(body);
        return status.into_response();
    }
    let Ok(req) = CommandRequest::decode(body.0.clone()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    // Wire V1.3 §7, DEC-18 §1.3 Nr. 9: an accepted command runs on its own task — a connection
    // lost before the answer (a lost acknowledgement) never cuts the controller off; the client
    // finds the outcome by its request id. A gateway shutdown still ends it: its record stays
    // open (PENDING_RECONCILE after the restart), never dispatched again.
    use tracing::Instrument;
    let id = req.request_id.clone();
    let run = tokio::spawn(
        async move {
            let response = app.control.execute(&req).await;
            drop(req);
            drop(body);
            response
        }
        .instrument(tracing::Span::current()),
    );
    match run.await {
        Ok(response) => protobuf(response),
        Err(e) => {
            // Its record stays open: a lookup answers PENDING_RECONCILE, never a second run.
            tracing::error!(request_id = %id, error = %e, "command task ended without an answer");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
#[derive(Deserialize)]
pub struct Lookup {
    project: i32,
    request_id: String,
}
pub async fn lookup(
    State(app): State<Arc<AppState>>,
    h: HeaderMap,
    Query(q): Query<Lookup>,
) -> Response {
    if let Err(s) = authorized(&app, &h, false) {
        return s.into_response();
    }
    if ![ProjectId::Project0 as i32, ProjectId::Bsc as i32].contains(&q.project)
        || !valid_id(&q.request_id)
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    protobuf(app.control.lookup(&q.request_id, q.project).await)
}
pub async fn process(State(app): State<Arc<AppState>>, h: HeaderMap) -> Response {
    if let Err(s) = authorized(&app, &h, false) {
        return s.into_response();
    }
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(app.control.observe().await),
    )
        .into_response()
}
pub async fn bsc_process(State(app): State<Arc<AppState>>, h: HeaderMap) -> Response {
    if let Err(s) = authorized(&app, &h, false) {
        return s.into_response();
    }
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(app.control.observe_project(ProjectId::Bsc as i32).await),
    )
        .into_response()
}
#[derive(Deserialize)]
pub struct LogQuery {
    source: String,
}
#[derive(Serialize)]
struct LogPart {
    source: String,
    text: String,
}
fn redact(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if [
        "api-key=",
        "api_key=",
        "authorization:",
        "bearer ",
        "secret=",
        "token=",
        "password=",
    ]
    .iter()
    .any(|x| lower.contains(x))
    {
        return "[sensitive log line redacted]".into();
    }
    line.split_whitespace()
        .map(|s| {
            if s.contains("://") {
                "[endpoint redacted]"
            } else {
                s
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}
fn tail(logdir: &Path, name: &str) -> Result<String, ()> {
    let base = logdir.canonicalize().map_err(|_| ())?;
    let path = logdir.join(name).canonicalize().map_err(|_| ())?;
    if !path.starts_with(&base) {
        return Err(());
    }
    let mut f = fs::File::open(path).map_err(|_| ())?;
    let len = f.metadata().map_err(|_| ())?.len();
    if !f.metadata().map_err(|_| ())?.is_file() {
        return Err(());
    }
    f.seek(SeekFrom::Start(len.saturating_sub(65536)))
        .map_err(|_| ())?;
    let mut b = Vec::new();
    f.take(65536).read_to_end(&mut b).map_err(|_| ())?;
    let text = String::from_utf8_lossy(&b);
    let lines: Vec<_> = text.lines().collect();
    Ok(lines
        .iter()
        .skip(lines.len().saturating_sub(200))
        .map(|s| redact(s))
        .collect::<Vec<_>>()
        .join("\n"))
}
pub async fn logs(
    State(app): State<Arc<AppState>>,
    h: HeaderMap,
    Query(q): Query<LogQuery>,
) -> Response {
    if let Err(s) = authorized(&app, &h, false) {
        return s.into_response();
    }
    let names = match q.source.as_str() {
        "project0" => vec![("project0", "capture-latest.log")],
        "bsc" => vec![
            ("bsc-readiness", "bsc-readiness-latest.log"),
            ("bsc-reporter", "bsc-reporter.log"),
        ],
        "gateway" => vec![("gateway", "gateway.log")],
        "all" => vec![
            ("project0", "capture-latest.log"),
            ("gateway", "gateway.log"),
            ("bsc-readiness", "bsc-readiness-latest.log"),
            ("bsc-reporter", "bsc-reporter.log"),
        ],
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(paths) = &app.control.paths else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let dir = paths.logdir.clone();
    let result = tokio::task::spawn_blocking(move || {
        names
            .into_iter()
            .map(|(source, name)| LogPart {
                source: source.into(),
                text: tail(&dir, name).unwrap_or_else(|_| "Log not available".into()),
            })
            .collect::<Vec<_>>()
    })
    .await;
    match result {
        Ok(parts) => (
            [(axum::http::header::CACHE_CONTROL, "no-store")],
            Json(parts),
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// `source_request` with its mode as a static text, for comparing literals.
    fn sr(req: &CommandRequest) -> Option<(&'static str, &'static str, u64)> {
        source_request(req).map(|(id, mode, g)| (id, &*Box::leak(mode.into_boxed_str()), g))
    }

    /// CI-N1: a hermetic launchd stand-in that knows no job — what launchd answers for an absent
    /// label — so the tests never read the host's launchd and also run where none exists (Linux).
    /// Writes the executable script `script` at `path`. Its bytes reach the
    /// file through a short-lived `cp`: this process never holds a write
    /// descriptor of a file it executes, so a child another test forks
    /// meanwhile (it holds this process's descriptors until its exec) cannot
    /// make that exec fail ETXTBSY.
    fn executable(path: &Path, script: &str) {
        use std::os::unix::fs::PermissionsExt;
        let source = path.with_extension("source");
        fs::write(&source, script).unwrap();
        let copied = std::process::Command::new("/bin/cp")
            .arg(&source)
            .arg(path)
            .status()
            .unwrap();
        assert!(copied.success());
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Audit E 4.13: the deployed reporter's answer for a deployed, idle readiness job (the
    /// reporter derives it, its own tests): no launch, stopped, controllable.
    fn idle_reporter(dir: &Path) {
        let job = crate::proto::bsc::v1::JobStatus {
            job: "readiness".into(),
            state: "STOPPED".into(),
            controller_available: Some(true),
            ..Default::default()
        };
        fs::write(dir.join("job.bin"), job.encode_to_vec()).unwrap();
        executable(
            &dir.join("bsc-status-reporter"),
            &format!("#!/bin/sh\ncat '{}'\n", dir.join("job.bin").display()),
        );
    }
    fn absent_launchctl(dir: &Path) -> PathBuf {
        let path = dir.join("launchctl");
        executable(
            &path,
            "#!/bin/sh\necho \"Could not find service \\\"$2\\\" in domain for port\" >&2\nexit 113\n",
        );
        path
    }
    /// Ledger bounds come from `Limits` exactly as in production; only the capacity is small so
    /// that fsync-bound tests stay fast, and real-time tests shrink the clock window.
    const CAPACITY: usize = 16;
    fn limits() -> Limits {
        Limits {
            command_ledger_records: CAPACITY,
            ..Limits::default()
        }
    }
    /// Real-time ledger: a short clock window and a ledger small enough to flood well within it.
    /// The clock window of the real-time ledger tests: its retention (twice the window) must
    /// outlast filling the ledger, which costs process starts per command (controller and launchd
    /// stand-ins) and is slow on a loaded runner — 1 s left too little headroom (CI-N1).
    const REAL_TIME_WINDOW_MS: u64 = 3_000;
    fn short(window_ms: u64) -> Limits {
        Limits {
            command_clock_window_ms: window_ms,
            command_retention_ms: crate::config::min_command_retention_ms(window_ms),
            command_ledger_records: 4,
            ..Limits::default()
        }
    }
    fn window_ns(l: &Limits) -> i64 {
        l.command_clock_window_ms as i64 * 1_000_000
    }
    fn now() -> i64 {
        crate::overview::unix_ns_now()
    }
    fn ledger(name: &str) -> (Control, PathBuf) {
        let dir = std::env::temp_dir().join(format!("gw-ledger-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let paths = Paths {
            ctl: dir.join("ctl"),
            rundir: dir.clone(),
            logdir: dir.clone(),
            label: "test".into(),
            launchctl: absent_launchctl(&dir),
        };
        (Control::new(Some(paths), &limits()), dir)
    }
    fn uuid(n: usize) -> String {
        format!("00000000-0000-4000-8000-{n:012x}")
    }
    fn capture_start(id: &str) -> CommandRequest {
        CommandRequest {
            project: ProjectId::Project0 as i32,
            request_id: id.into(),
            action: START.into(),
            issued_at_unix_ns: 1,
            ..Default::default()
        }
    }
    /// The request `put(n)` records: long ago, issued in the order of n.
    fn stored(n: usize) -> CommandRequest {
        CommandRequest {
            issued_at_unix_ns: 1_000 + n as i64,
            ..capture_start(&uuid(n))
        }
    }
    /// A recorded receipt issued right after `x`, so that `x` is the one issued earliest.
    fn after(x: &CommandRequest, n: usize) -> CommandRequest {
        CommandRequest {
            issued_at_unix_ns: x.issued_at_unix_ns + 1 + n as i64,
            ..capture_start(&uuid(n))
        }
    }
    /// One recorded command for `request`, last updated long ago (strictly increasing with n).
    fn put_req(
        control: &Control,
        request: &CommandRequest,
        n: usize,
        phase: CommandPhase,
    ) -> Result<(), ()> {
        let id = &request.request_id;
        let record = Record {
            request: request.encode_to_vec(),
            response: reply(
                id,
                CommandStatus::Accepted,
                CommandRejectReason::Unspecified,
                "recorded",
            )
            .encode_to_vec(),
            phase: phase as i32,
            detail: String::new(),
            launch_id: String::new(),
        };
        control.persist(id, &record)?;
        age(control, id, 1_000_000 + n as u64);
        Ok(())
    }
    fn put(control: &Control, n: usize, phase: CommandPhase) -> Result<(), ()> {
        put_req(control, &stored(n), n, phase)
    }
    #[tokio::test]
    async fn a_full_ledger_archives_the_oldest_receipt_which_still_replays_and_conflicts() {
        let (control, dir) = ledger("full");
        // The oldest record is an open intent; the oldest receipt is the second one.
        put(&control, 0, CommandPhase::PendingReconcile).unwrap();
        for n in 1..CAPACITY {
            put(&control, n, CommandPhase::Completed).unwrap();
        }
        put(&control, CAPACITY, CommandPhase::Completed).unwrap();
        assert_eq!(records(&dir.join("commands")).unwrap().len(), CAPACITY);
        assert!(control.record_path(&uuid(0)).unwrap().exists());
        assert!(dir
            .join(format!("commands/archive/{}.cmd", uuid(1)))
            .exists());
        let replay = control.execute(&stored(1)).await;
        assert_eq!(replay.status, CommandStatus::Accepted as i32);
        assert_eq!(replay.phase, CommandPhase::Completed as i32);
        let other = CommandRequest {
            action: STOP.into(),
            ..stored(1)
        };
        let conflict = control.execute(&other).await;
        assert_eq!(
            conflict.reject_reason,
            CommandRejectReason::IdempotencyConflict as i32
        );
        fs::remove_dir_all(dir).unwrap();
    }
    /// A dispatching ledger whose stub controller counts executions; `restart` reopens it the way
    /// a new gateway process does (only the files carry over).
    struct Env {
        dir: PathBuf,
        runs: PathBuf,
        limits: Limits,
    }
    impl Env {
        fn new(name: &str) -> Self {
            Self::with(name, limits())
        }
        fn with(name: &str, limits: Limits) -> Self {
            // Distinct from ledger()'s directories: tests run in parallel.
            let dir = std::env::temp_dir().join(format!("gw-env-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join("bsc-root")).unwrap();
            fs::create_dir_all(dir.join("run")).unwrap();
            fs::write(dir.join("bsc-root/SNAPSHOT_SHA256"), "a").unwrap();
            fs::write(dir.join("g07_pre_live_readiness"), "").unwrap();
            idle_reporter(&dir);
            let runs = dir.join("runs");
            executable(
                &dir.join("darkbotctl"),
                &format!("#!/bin/sh\nprintf '%s\\n' \"$6\" >> '{}'\n", runs.display()),
            );
            Env { dir, runs, limits }
        }
        fn restart(&self) -> Control {
            let paths = Paths {
                ctl: self.dir.join("darkbotctl"),
                rundir: self.dir.join("run"),
                logdir: self.dir.clone(),
                label: "test.capture".into(),
                launchctl: absent_launchctl(&self.dir),
            };
            Control::new(Some(paths), &self.limits)
        }
        fn executions(&self) -> usize {
            fs::read_to_string(&self.runs)
                .unwrap_or_default()
                .lines()
                .count()
        }
        fn ledger(&self) -> PathBuf {
            self.dir.join("run/commands")
        }
        /// Waits until every receipt recorded so far is past the retention window.
        async fn outlast_retention(&self) {
            tokio::time::sleep(Duration::from_millis(
                self.limits.command_retention_ms + 100,
            ))
            .await;
        }
    }
    impl Drop for Env {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
    fn candidate(n: usize, issued_at_unix_ns: i64) -> CommandRequest {
        CommandRequest {
            project: ProjectId::Bsc as i32,
            request_id: uuid(n),
            action: BSC_CANDIDATE.into(),
            resource_id: "SELF_B".into(),
            expected: Some(crate::proto::ExpectedVersion {
                version: Some(expected_version::Version::ProfileVersion(7)),
            }),
            params: Default::default(),
            issued_at_unix_ns,
        }
    }
    fn age(control: &Control, id: &str, secs: u64) {
        fs::File::options()
            .write(true)
            .open(control.record_path(id).unwrap())
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(secs))
            .unwrap();
    }
    fn refused(r: &CommandResponse, reason: CommandRejectReason) -> bool {
        r.status == CommandStatus::Rejected as i32 && r.reject_reason == reason as i32
    }
    fn completed(r: &CommandResponse) -> bool {
        r.status == CommandStatus::Accepted as i32 && r.phase == CommandPhase::Completed as i32
    }
    /// BSC-051: the binary ledger record round-trips and refuses any damage.
    #[test]
    fn the_binary_ledger_record_round_trips_and_refuses_damage() {
        let r = Record {
            request: vec![1, 2, 3],
            response: vec![4],
            phase: CommandPhase::Completed as i32,
            detail: "done".into(),
            launch_id: "launch".into(),
        };
        let bytes = ledger_record::encode(&r).unwrap();
        assert_eq!(&bytes[..8], b"DBCMD001");
        assert_eq!(ledger_record::decode(&bytes), Ok(r));
        for cut in [0, 8, 12, bytes.len() - 1] {
            assert!(ledger_record::decode(&bytes[..cut]).is_err(), "{cut}");
        }
        for at in [0, 9, 13, bytes.len() - 1] {
            let mut flipped = bytes.clone();
            flipped[at] ^= 1;
            assert!(ledger_record::decode(&flipped).is_err(), "{at}");
        }
        let mut longer = bytes.clone();
        longer.insert(bytes.len() - 4, 0);
        assert!(ledger_record::decode(&longer).is_err());
    }
    /// BSC-051: a receipt written before the binary ledger (JSON) still replays and conflicts after
    /// a restart; its next write replaces it with the binary form; nothing writes JSON any more.
    #[tokio::test]
    async fn a_legacy_json_receipt_replays_and_is_replaced_by_its_binary_form() {
        let env = Env::new("legacy");
        let control = env.restart();
        let x = candidate(9_100, now());
        assert!(completed(&control.execute(&x).await));
        let cmd = env.ledger().join(format!("{}.cmd", x.request_id));
        let legacy = env.ledger().join(format!("{}.json", x.request_id));
        let record = ledger_record::read(&cmd).unwrap();
        let json = serde_json::json!({
            "request": record.request,
            "response": record.response,
            "phase": record.phase,
            "detail": record.detail,
            "launch_id": record.launch_id,
        });
        fs::write(&legacy, serde_json::to_vec(&json).unwrap()).unwrap();
        fs::remove_file(&cmd).unwrap();
        let control = env.restart();
        assert!(completed(&control.execute(&x).await));
        let other = CommandRequest {
            resource_id: "SELF_A".into(),
            ..x.clone()
        };
        assert!(refused(
            &control.execute(&other).await,
            CommandRejectReason::IdempotencyConflict
        ));
        assert_eq!(env.executions(), 1);
        control.persist(&x.request_id, &record).unwrap();
        assert!(cmd.exists() && !legacy.exists());
        assert_eq!(ledger_record::read(&cmd), Ok(record));
        assert!(fs::read_dir(env.ledger()).unwrap().all(|e| e
            .unwrap()
            .path()
            .extension()
            .is_none_or(|x| x != "json")));
    }
    /// BSC-051: a damaged record makes the ledger unreadable; it is never "no record".
    /// DEC-07: project0.capture.start without the start approval of a READY market boot flow
    /// is refused before any controller call (no process, no network read); with one it is the
    /// boot controller's start step (`darkbotctl p0 boot start`), called once.
    #[tokio::test]
    async fn dec07_a_raw_capture_start_needs_its_boot_approval() {
        use crate::proto::project0::v1::BootStartCommand;
        let env = Env::new("boot-gate");
        let control = env.restart();
        let start = |n: usize, params: Vec<u8>| CommandRequest {
            project: ProjectId::Project0 as i32,
            request_id: uuid(n),
            action: START.into(),
            resource_id: String::new(),
            expected: None,
            params: params.into(),
            issued_at_unix_ns: now(),
        };
        let bare = control.execute(&start(9_400, vec![])).await;
        assert!(refused(&bare, CommandRejectReason::GateBlocked));
        assert!(
            bare.detail.starts_with("BOOT_AUDIT_REQUIRED"),
            "{}",
            bare.detail
        );
        assert_eq!(env.executions(), 0, "nothing was called");
        let approval = BootStartCommand {
            flow_id: "flow-1".into(),
            approved_token: "fedcba9876543210".into(),
            duration: "1h".into(),
            capture_source: "logs".into(),
            chain_verify: "off".into(),
            qualification: None,
        };
        let mut wrong = approval.clone();
        wrong.approved_token = "not-a-token".into();
        assert!(refused(
            &control.execute(&start(9_401, wrong.encode_to_vec())).await,
            CommandRejectReason::InvalidParams
        ));
        assert_eq!(env.executions(), 0);
        let approved = control
            .execute(&start(9_402, approval.encode_to_vec()))
            .await;
        assert_eq!(
            approved.status,
            CommandStatus::Accepted as i32,
            "{}",
            approved.detail
        );
        assert_eq!(
            env.executions(),
            1,
            "the boot controller's start step, once"
        );
        // The network approval names its provider evidence by record id: a path in the command
        // is refused before any controller call.
        use crate::proto::project0::v1::BootNetworkCommand;
        let network = |n: usize, evidence: &str| CommandRequest {
            project: ProjectId::Project0 as i32,
            request_id: uuid(n),
            action: BOOT_NETWORK.into(),
            resource_id: String::new(),
            expected: None,
            params: BootNetworkCommand {
                flow_id: "flow-2".into(),
                approved_token: "0123456789abcdef".into(),
                genesis_hash: "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d".into(),
                provider_evidence: evidence.into(),
                capture_source: "logs".into(),
            }
            .encode_to_vec()
            .into(),
            issued_at_unix_ns: now(),
        };
        assert!(refused(
            &control
                .execute(&network(9_403, "/etc/provider_evidence.p0j"))
                .await,
            CommandRejectReason::InvalidParams
        ));
        assert_eq!(env.executions(), 1, "a path reaches no controller");
        let by_id = control.execute(&network(9_404, "89abcdef01234567")).await;
        assert_eq!(
            by_id.status,
            CommandStatus::Accepted as i32,
            "{}",
            by_id.detail
        );
        assert_eq!(env.executions(), 2);
    }
    #[tokio::test]
    async fn a_damaged_ledger_record_refuses_instead_of_running_again() {
        let env = Env::new("damaged");
        let control = env.restart();
        let x = candidate(9_200, now());
        assert!(completed(&control.execute(&x).await));
        let cmd = env.ledger().join(format!("{}.cmd", x.request_id));
        let mut bytes = fs::read(&cmd).unwrap();
        let n = bytes.len();
        bytes[n - 5] ^= 1;
        fs::write(&cmd, bytes).unwrap();
        assert!(refused(
            &env.restart().execute(&x).await,
            CommandRejectReason::EngineUnavailable
        ));
        assert_eq!(env.executions(), 1);
    }
    /// BSC-053: without the gateway clock no command is admitted and no process state is
    /// reported; never time 0.
    #[test]
    fn a_missing_gateway_clock_admits_nothing_and_observes_nothing() {
        let (reason, detail) = clock_window(now(), None, 1_000_000_000).unwrap_err();
        assert_eq!(reason, CommandRejectReason::EngineUnavailable);
        assert!(detail.contains("clock unavailable"), "{detail}");
        assert!(clock_window(now(), Some(now()), 1_000_000_000).is_ok());
        let mut s = ProcessSnapshot {
            state: "RUNNING".into(),
            run_id: "r".into(),
            pid: Some(1),
            reason: String::new(),
            observed_unix_ms: 7,
            controller_available: true,
        };
        stamp(&mut s, None);
        assert_eq!((s.state.as_str(), s.observed_unix_ms), ("UNKNOWN", 0));
        stamp(&mut s, Some(5));
        assert_eq!(s.observed_unix_ms, 5);
    }
    #[tokio::test]
    async fn an_archived_receipt_replays_after_a_restart_and_other_bytes_conflict() {
        let env = Env::new("restart");
        let control = env.restart();
        let x = candidate(9_000, now());
        assert!(completed(&control.execute(&x).await));
        age(&control, &x.request_id, 1);
        for n in 0..CAPACITY {
            put(&control, n, CommandPhase::Completed).unwrap();
        }
        assert!(env
            .ledger()
            .join(format!("archive/{}.cmd", x.request_id))
            .exists());
        let control = env.restart();
        assert!(completed(&control.execute(&x).await));
        for other in [
            CommandRequest {
                issued_at_unix_ns: x.issued_at_unix_ns + 1,
                ..x.clone()
            },
            CommandRequest {
                resource_id: "SELF_A".into(),
                ..x.clone()
            },
        ] {
            let r = control.execute(&other).await;
            assert!(
                refused(&r, CommandRejectReason::IdempotencyConflict),
                "{}",
                r.detail
            );
        }
        assert_eq!(env.executions(), 1);
    }
    #[tokio::test]
    async fn the_retired_through_mark_survives_restarts_and_crashes_fail_closed() {
        let env = Env::new("mark");
        let control = env.restart();
        let x = candidate(9_000, now());
        control.execute(&x).await;
        age(&control, &x.request_id, 1);
        for n in 0..2 * CAPACITY {
            put_req(&control, &after(&x, n), n, CommandPhase::Completed).unwrap();
        }
        let archive = env.ledger().join("archive");
        assert_eq!(
            retired_through(&env.ledger()),
            Ok(Some(x.issued_at_unix_ns))
        );
        // Crash before the atomic rename: a stray temporary file changes nothing.
        fs::write(archive.join("retired_through.tmp-crashed"), "999999999999").unwrap();
        let control = env.restart();
        assert!(refused(
            &control.execute(&x).await,
            CommandRejectReason::IdempotencyConflict
        ));
        // Crash after raising the mark but before removing the receipt: it still replays.
        let kept = after(&x, 0); // the earliest receipt still archived
        assert!(archive.join(format!("{}.cmd", kept.request_id)).exists());
        fs::write(
            archive.join(RETIRED_THROUGH),
            kept.issued_at_unix_ns.to_string(),
        )
        .unwrap();
        let replay = env.restart().execute(&kept).await;
        assert_eq!(
            replay.status,
            CommandStatus::Accepted as i32,
            "{}",
            replay.detail
        );
        // An unreadable mark refuses every new command instead of guessing.
        fs::write(archive.join(RETIRED_THROUGH), "not a number").unwrap();
        assert!(refused(
            &env.restart().execute(&candidate(9_001, now())).await,
            CommandRejectReason::EngineUnavailable
        ));
        assert_eq!(env.executions(), 1);
    }
    #[tokio::test]
    async fn new_commands_run_in_any_stamp_order_within_the_clock_window_and_never_outside() {
        let env = Env::new("clock");
        let control = env.restart();
        for n in 0..=2 * CAPACITY {
            put(&control, n, CommandPhase::Completed).unwrap();
        }
        let w = window_ns(&env.limits);
        let t = now();
        // Unordered stamps and a browser clock ahead or behind, all within the window.
        for (n, issued) in [
            (1, t - w / 6),
            (2, t - w / 3),
            (3, t + w / 15),
            (4, t - w / 2),
        ] {
            let r = control.execute(&candidate(10_000 + n, issued)).await;
            assert!(completed(&r), "{n}: {}", r.detail);
        }
        assert_eq!(env.executions(), 4);
        // Outside the window in either direction: refused, whatever the ledger holds.
        for (n, issued) in [
            (5, now() - w - 1_000_000_000),
            (6, now() + w + 1_000_000_000),
        ] {
            let r = control.execute(&candidate(10_000 + n, issued)).await;
            assert!(refused(&r, CommandRejectReason::InvalidParams), "{n}");
            assert!(r
                .detail
                .contains(&format!("{} ms", env.limits.command_clock_window_ms)));
        }
        assert_eq!(env.executions(), 4);
    }
    /// Contract limit (Wire V1.3 §7): once a receipt is forgotten its id is no longer known; the
    /// same id with a new time is other bytes and counts as a new intent, the forgotten one never.
    #[tokio::test]
    async fn after_its_retention_a_reused_id_with_a_new_time_is_a_new_intent() {
        let env = Env::new("reuse");
        let control = env.restart();
        let x = candidate(9_000, now());
        control.execute(&x).await;
        age(&control, &x.request_id, 1);
        for n in 0..2 * CAPACITY {
            put_req(&control, &after(&x, n), n, CommandPhase::Completed).unwrap();
        }
        let earlier = CommandRequest {
            issued_at_unix_ns: x.issued_at_unix_ns - 1,
            ..x.clone()
        };
        assert!(refused(
            &control.execute(&earlier).await,
            CommandRejectReason::IdempotencyConflict
        ));
        assert_eq!(env.executions(), 1);
        let later = CommandRequest {
            issued_at_unix_ns: now(),
            ..x.clone()
        };
        assert!(completed(&control.execute(&later).await));
        assert_eq!(env.executions(), 2);
    }
    #[tokio::test]
    async fn open_intents_survive_a_restart_and_are_never_dispatched_again() {
        let env = Env::new("open");
        let control = env.restart();
        put(&control, 0, CommandPhase::PendingReconcile).unwrap();
        for n in 1..CAPACITY {
            put(&control, n, CommandPhase::Completed).unwrap();
        }
        for n in CAPACITY..3 * CAPACITY {
            put(&control, n, CommandPhase::Failed).unwrap();
        }
        let control = env.restart();
        assert!(control.record_path(&uuid(0)).unwrap().exists());
        let replay = control.execute(&stored(0)).await;
        assert_eq!(replay.phase, CommandPhase::PendingReconcile as i32);
        let lookup = control.lookup(&uuid(0), ProjectId::Project0 as i32).await;
        assert!(lookup.found);
        assert_eq!(env.executions(), 0);
    }
    /// The receipt of an executed command leaves the bounded archive; the exact same request
    /// arriving again must not run a second time, while a fresh command still runs.
    #[tokio::test]
    async fn a_command_whose_receipt_left_the_archive_is_never_executed_again() {
        let env = Env::new("retired");
        let control = env.restart();
        let first = candidate(9_999, now());
        assert!(completed(&control.execute(&first).await));
        assert_eq!(env.executions(), 1);
        age(&control, &first.request_id, 1);
        for n in 0..2 * CAPACITY {
            put_req(&control, &after(&first, n), n, CommandPhase::Completed).unwrap();
        }
        assert!(
            control.load(&first.request_id).unwrap().is_none(),
            "receipt left the archive"
        );
        let replay = control.execute(&first).await;
        assert_eq!(
            (replay.status, replay.reject_reason),
            (
                CommandStatus::Rejected as i32,
                CommandRejectReason::IdempotencyConflict as i32
            ),
            "{}",
            replay.detail
        );
        assert_eq!(env.executions(), 1, "a retired command never runs twice");
        // No permanent refusal: a fresh command runs; a future-dated one is refused.
        assert!(completed(&control.execute(&candidate(10_000, now())).await));
        let w = window_ns(&env.limits);
        let future = control.execute(&candidate(10_001, now() + 2 * w)).await;
        assert_eq!(future.status, CommandStatus::Rejected as i32);
        assert_eq!(env.executions(), 2);
    }
    /// Counter-case 1, real time: within its retention the same id never runs twice, not even with
    /// a new time and the same action; a flood is refused instead of forgetting it early. After the
    /// retention its exact bytes are outside the clock window and the mark: never again. Only the
    /// same id with a new time is then a new intent (the documented contract limit).
    #[tokio::test]
    async fn within_its_retention_a_request_id_never_runs_twice() {
        let env = Env::with("case1", short(REAL_TIME_WINDOW_MS));
        let cap = env.limits.command_ledger_records;
        let control = env.restart();
        let x = candidate(9_000, now());
        assert!(completed(&control.execute(&x).await));
        for n in 0..2 * cap {
            control.execute(&candidate(20_000 + n, now())).await;
        }
        assert_eq!(
            env.executions(),
            2 * cap,
            "the last one was refused, nothing forgotten"
        );
        let same_action_new_time = CommandRequest {
            issued_at_unix_ns: now(),
            ..x.clone()
        };
        assert!(refused(
            &control.execute(&same_action_new_time).await,
            CommandRejectReason::IdempotencyConflict
        ));
        assert!(
            completed(&control.execute(&x).await),
            "replay = original receipt"
        );
        assert_eq!(env.executions(), 2 * cap);
        env.outlast_retention().await;
        assert!(completed(&control.execute(&candidate(30_000, now())).await));
        assert!(
            control.load(&x.request_id).unwrap().is_none(),
            "x forgotten only now"
        );
        assert!(refused(
            &control.execute(&x).await,
            CommandRejectReason::InvalidParams
        ));
        let reused = CommandRequest {
            issued_at_unix_ns: now(),
            ..x.clone()
        };
        assert!(completed(&control.execute(&reused).await));
        assert_eq!(env.executions(), 2 * cap + 2);
    }
    /// Counter-case 2, real time: requests stamped right below the upper clock bound fill the
    /// ledger; new commands are refused only while every receipt is younger than the retention,
    /// and afterwards the mark never blocks a current stamp.
    #[tokio::test]
    async fn stamps_at_the_upper_clock_bound_never_block_current_commands() {
        let l = short(REAL_TIME_WINDOW_MS);
        let env = Env::with("case2", l.clone());
        let cap = l.command_ledger_records;
        let control = env.restart();
        let upper = || now() + window_ns(&l) * 9 / 10;
        let mut accepted = 0;
        for n in 0..=2 * cap {
            let r = control.execute(&candidate(20_000 + n, upper())).await;
            accepted += usize::from(completed(&r));
        }
        assert_eq!(
            accepted,
            2 * cap,
            "the ledger is full of receipts inside the retention"
        );
        assert_eq!(retired_through(&env.ledger()), Ok(None));
        assert!(refused(
            &control.execute(&candidate(30_000, now())).await,
            CommandRejectReason::EngineUnavailable
        ));
        env.outlast_retention().await;
        assert!(completed(&control.execute(&candidate(30_001, now())).await));
        let mark = retired_through(&env.ledger()).unwrap().unwrap();
        assert!(
            mark < now() - window_ns(&l),
            "the mark stays behind every current stamp"
        );
        assert!(completed(&control.execute(&candidate(30_002, now())).await));
    }
    /// Counter-case 3, real time: a browser clock beyond the window is refused with the same
    /// reason before and after archiving (no ledger effect) and runs once its clock is corrected.
    #[tokio::test]
    async fn a_browser_clock_beyond_the_window_is_refused_regardless_of_archiving() {
        let env = Env::with("case3", short(REAL_TIME_WINDOW_MS));
        let control = env.restart();
        let hour = 3_600_000_000_000;
        let behind = control.execute(&candidate(40_000, now() - hour)).await;
        assert!(refused(&behind, CommandRejectReason::InvalidParams));
        for n in 0..2 * env.limits.command_ledger_records {
            control.execute(&candidate(40_001 + n, now())).await;
        }
        env.outlast_retention().await;
        assert!(completed(&control.execute(&candidate(41_000, now())).await));
        assert!(
            retired_through(&env.ledger()).unwrap().is_some(),
            "archiving happened"
        );
        let again = control.execute(&candidate(41_001, now() - hour)).await;
        assert!(refused(&again, CommandRejectReason::InvalidParams));
        assert_eq!(again.detail, behind.detail);
        assert!(completed(&control.execute(&candidate(41_002, now())).await));
    }
    #[test]
    fn a_ledger_of_open_intents_refuses_a_new_one_and_drops_nothing() {
        let (control, dir) = ledger("open");
        for n in 0..CAPACITY {
            put(&control, n, CommandPhase::Pending).unwrap();
        }
        assert!(put(&control, CAPACITY, CommandPhase::Completed).is_err());
        assert_eq!(records(&dir.join("commands")).unwrap().len(), CAPACITY);
        assert!(!dir.join("commands/archive").exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn the_archive_keeps_the_newest_receipts_and_drops_only_its_oldest() {
        let (control, dir) = ledger("archive");
        for n in 0..=2 * CAPACITY {
            put(&control, n, CommandPhase::Failed).unwrap();
        }
        assert_eq!(records(&dir.join("commands")).unwrap().len(), CAPACITY);
        assert_eq!(
            records(&dir.join("commands/archive")).unwrap().len(),
            CAPACITY
        );
        assert!(control.load(&uuid(0)).unwrap().is_none());
        assert!(control.load(&uuid(1)).unwrap().is_some());
        // The mark of the removed receipt is durable; its exact replay is refused, never run.
        assert_eq!(retired_through(&dir.join("commands")), Ok(Some(1_000)));
        assert!(control.load(&uuid(2 * CAPACITY)).unwrap().is_some());
        fs::remove_dir_all(dir).unwrap();
    }
    /// Plan 27.10.: the wallet, venue and economic switches are three more allowlisted actions
    /// of the same native owner: fixed IDs, their own modes, the wallet one with the
    /// `wallet_policy_version` CAS; each dispatches exactly `darkbotctl bsc source request`.
    /// Plan 28.10.: the LIVE grant reaches the native owner as exactly the confirmed scope and
    /// its digest with the owner generation as CAS; revoke and the campaign's total limit (at
    /// most ten) are owner requests; anything else never reaches the owner.
    #[tokio::test]
    async fn the_live_grant_revoke_and_total_limit_reach_the_native_owner() {
        use crate::proto::bsc::v1::{BscLiveGrantCommand, LiveGrantLimits, LiveGrantRisk};
        use crate::proto::expected_version::Version::{ConfigVersion, WalletPolicyVersion};
        let request =
            |action: &str, resource: &str, v: Option<expected_version::Version>| CommandRequest {
                project: ProjectId::Bsc as i32,
                request_id: "62345678-1234-1234-1234-123456789abc".into(),
                action: action.into(),
                resource_id: resource.into(),
                expected: v.map(|v| crate::proto::ExpectedVersion { version: Some(v) }),
                params: Default::default(),
                issued_at_unix_ns: crate::overview::unix_ns_now(),
            };
        let scope = BscLiveGrantCommand {
            venue: "BSC_VENUE_FLAP".into(),
            strategies: "C3+STRATEGY_C".into(),
            sources: vec!["BSC_INGRESS_PUBLICNODE".into()],
            adapters: vec!["CLUB48_BUILDER_V1".into()],
            max_input_wei: "1000000000000000".into(),
            max_spend_wei: "20000000000000000".into(),
            max_attempts: 2,
            expires_unix_ms: 1_793_000_000_000,
            scope_sha256: "ab".repeat(32),
            exit_only: false,
            risk: Some(LiveGrantRisk {
                max_buy_slippage_bps: 300,
                max_exit_slippage_bps: 500,
                max_gas_price_wei: 1_000_000_000,
                max_buy_gas_limit: 3_000_000,
                max_approve_gas_limit: 100_000,
                max_sell_gas_limit: 1_500_000,
                outlier_ack: false,
            }),
            limits: Some(LiveGrantLimits {
                max_open_positions: 3,
                max_realized_loss_wei: "5000000000000000".into(),
                max_daily_realized_loss_wei: "2000000000000000".into(),
                min_buy_interval_ms: 60_000,
            }),
        };
        let grant = |c: &BscLiveGrantCommand, resource: &str, v| {
            let mut r = request(BSC_LIVE_GRANT, resource, Some(v));
            r.params = c.encode_to_vec().into();
            r
        };
        assert_eq!(
            grant_request(&grant(&scope, "BSC_LIVE_GRANT", ConfigVersion(9))).unwrap(),
            [
                "bsc",
                "live",
                "grant",
                "BSC_VENUE_FLAP",
                "C3+STRATEGY_C",
                "BSC_INGRESS_PUBLICNODE",
                "CLUB48_BUILDER_V1",
                "1000000000000000",
                "20000000000000000",
                "2",
                "1793000000000",
                "ALL",
                "300",
                "500",
                "1000000000",
                "3000000",
                "100000",
                "1500000",
                "3",
                "5000000000000000",
                "2000000000000000",
                "60000",
                "NO_OUTLIER_ACK",
                &"ab".repeat(32),
                "9",
            ]
        );
        // R4 (BSC3-069): a risk confirmed as an outlier says so to the owner.
        let mut acked = scope.clone();
        acked.risk = Some(LiveGrantRisk {
            outlier_ack: true,
            ..scope.risk.unwrap()
        });
        let args = grant_request(&grant(&acked, "BSC_LIVE_GRANT", ConfigVersion(9))).unwrap();
        assert_eq!(args[22], "OUTLIER_ACK");
        // BSC3-058: an EXIT-ONLY grant says so to the owner.
        let exits = BscLiveGrantCommand {
            exit_only: true,
            ..scope.clone()
        };
        let args = grant_request(&grant(&exits, "BSC_LIVE_GRANT", ConfigVersion(9))).unwrap();
        assert_eq!(args[11], "EXIT_ONLY");
        for bad in [
            BscLiveGrantCommand {
                venue: "BSC_VENUE_X".into(),
                ..scope.clone()
            },
            BscLiveGrantCommand {
                sources: vec!["BSC_STATE_C2".into()],
                ..scope.clone()
            },
            BscLiveGrantCommand {
                adapters: vec!["BLOCKRAZOR_RPC_V1".into()],
                ..scope.clone()
            },
            BscLiveGrantCommand {
                max_input_wei: "01".into(),
                ..scope.clone()
            },
            BscLiveGrantCommand {
                scope_sha256: "AB".repeat(32),
                ..scope.clone()
            },
            // BSC3-069: no risk.
            BscLiveGrantCommand {
                risk: None,
                ..scope.clone()
            },
            // 31.10. order: no limits, a non-decimal loss.
            BscLiveGrantCommand {
                limits: None,
                ..scope.clone()
            },
            BscLiveGrantCommand {
                limits: Some(LiveGrantLimits {
                    max_daily_realized_loss_wei: "1e15".into(),
                    ..scope.limits.clone().unwrap()
                }),
                ..scope.clone()
            },
        ] {
            assert_eq!(
                grant_request(&grant(&bad, "BSC_LIVE_GRANT", ConfigVersion(9))),
                None
            );
        }
        // Audit E 4.8: a bound is the native owner's alone; such a grant reaches it (it refuses
        // these: the shared grant vector, sources::tests::the_grant_vector_is_the_owners).
        for bounded in [
            BscLiveGrantCommand {
                max_attempts: 0,
                ..scope.clone()
            },
            BscLiveGrantCommand {
                risk: Some(LiveGrantRisk {
                    max_buy_slippage_bps: 10_000,
                    ..scope.risk.unwrap()
                }),
                ..scope.clone()
            },
            BscLiveGrantCommand {
                risk: Some(LiveGrantRisk {
                    max_exit_slippage_bps: 10_000,
                    ..scope.risk.unwrap()
                }),
                ..scope.clone()
            },
            BscLiveGrantCommand {
                risk: Some(LiveGrantRisk {
                    max_gas_price_wei: 0,
                    ..scope.risk.unwrap()
                }),
                ..scope.clone()
            },
            BscLiveGrantCommand {
                risk: Some(LiveGrantRisk {
                    max_buy_gas_limit: 20_999,
                    ..scope.risk.unwrap()
                }),
                ..scope.clone()
            },
            BscLiveGrantCommand {
                risk: Some(LiveGrantRisk {
                    max_approve_gas_limit: 20_999,
                    ..scope.risk.unwrap()
                }),
                ..scope.clone()
            },
            BscLiveGrantCommand {
                risk: Some(LiveGrantRisk {
                    max_sell_gas_limit: 20_999,
                    ..scope.risk.unwrap()
                }),
                ..scope.clone()
            },
            BscLiveGrantCommand {
                limits: Some(LiveGrantLimits {
                    max_open_positions: 0,
                    ..scope.limits.clone().unwrap()
                }),
                ..scope.clone()
            },
            BscLiveGrantCommand {
                limits: Some(LiveGrantLimits {
                    max_realized_loss_wei: "0".into(),
                    ..scope.limits.clone().unwrap()
                }),
                ..scope.clone()
            },
            // The day's bound above the grant's.
            BscLiveGrantCommand {
                limits: Some(LiveGrantLimits {
                    max_daily_realized_loss_wei: "5000000000000001".into(),
                    ..scope.limits.clone().unwrap()
                }),
                ..scope.clone()
            },
        ] {
            assert!(grant_request(&grant(&bounded, "BSC_LIVE_GRANT", ConfigVersion(9))).is_some());
        }
        assert_eq!(
            grant_request(&grant(&scope, "BSC_LIVE_GRANT", WalletPolicyVersion(9))),
            None
        );
        assert_eq!(grant_request(&grant(&scope, "X", ConfigVersion(9))), None);
        assert_eq!(
            sr(&request(
                BSC_LIVE_REVOKE,
                "BSC_LIVE_GRANT:DISABLED",
                Some(ConfigVersion(4))
            )),
            Some(("BSC_LIVE_GRANT", "DISABLED", 4))
        );
        assert_eq!(
            sr(&request(
                BSC_LIVE_REVOKE,
                "BSC_LIVE_GRANT:ENABLED",
                Some(ConfigVersion(4))
            )),
            None
        );
        assert_eq!(
            sr(&request(
                BSC_ATTEMPT_TOTAL,
                "BSC_BUY_ATTEMPTS_TOTAL:10",
                Some(ConfigVersion(4))
            )),
            Some(("BSC_BUY_ATTEMPTS_TOTAL", "10", 4))
        );
        // Audit E 3.1: above the configured maximum only the owner refuses it.
        assert_eq!(
            sr(&request(
                BSC_ATTEMPT_TOTAL,
                "BSC_BUY_ATTEMPTS_TOTAL:11",
                Some(ConfigVersion(4))
            )),
            Some(("BSC_BUY_ATTEMPTS_TOTAL", "11", 4))
        );
        assert_eq!(
            sr(&request(
                BSC_ATTEMPT_TOTAL,
                "BSC_BUY_ATTEMPTS_TOTAL:010",
                Some(ConfigVersion(4))
            )),
            None
        );
        for action in [BSC_LIVE_GRANT, BSC_LIVE_REVOKE, BSC_ATTEMPT_TOTAL] {
            assert!(allowed(ProjectId::Bsc as i32, action));
            assert!(!allowed(ProjectId::Project0 as i32, action));
        }
        // Through the controller: the owner gets exactly these arguments and the request id.
        let dir = std::env::temp_dir().join(format!("gw-live-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("run")).unwrap();
        let ctl = dir.join("darkbotctl");
        let log = dir.join("argv");
        executable(
            &ctl,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\necho \"outcome=APPLIED source=BSC_LIVE_GRANT requested=GRANTED generation=10\"\n",
                log.display()
            ),
        );
        let control = Control::new(
            Some(Paths {
                ctl,
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "test.capture".into(),
                launchctl: absent_launchctl(&dir),
            }),
            &Limits::default(),
        );
        let applied = control
            .execute(&grant(&scope, "BSC_LIVE_GRANT", ConfigVersion(9)))
            .await;
        assert_eq!(
            (applied.status, applied.phase),
            (
                CommandStatus::Accepted as i32,
                CommandPhase::Completed as i32
            ),
            "{}",
            applied.detail
        );
        let argv = fs::read_to_string(&log).unwrap();
        assert!(
            argv.starts_with("bsc\nlive\ngrant\nBSC_VENUE_FLAP\n"),
            "{argv}"
        );
        assert!(
            argv.ends_with("9\n62345678-1234-1234-1234-123456789abc\n"),
            "{argv}"
        );
        // Review L3 (Audit E 4.8): a bound reaches the owner, and its refusal is the receipt's
        // detail, as in the terminal.
        let refusing_ctl = dir.join("darkbotctl-refusing");
        executable(
            &refusing_ctl,
            "#!/bin/sh\necho 'STOP: max_attempts must be at least 1' >&2\nexit 2\n",
        );
        let refusing = Control::new(
            Some(Paths {
                ctl: refusing_ctl,
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "test.capture".into(),
                launchctl: absent_launchctl(&dir),
            }),
            &Limits::default(),
        );
        let mut bounded = grant(
            &BscLiveGrantCommand {
                max_attempts: 0,
                ..scope.clone()
            },
            "BSC_LIVE_GRANT",
            ConfigVersion(9),
        );
        bounded.request_id = "82345678-1234-1234-1234-123456789abc".into();
        let refused = refusing.execute(&bounded).await;
        assert_eq!(
            refused.phase,
            CommandPhase::Failed as i32,
            "{}",
            refused.detail
        );
        assert!(
            refused
                .detail
                .contains("Native controller refused: STOP: max_attempts must be at least 1"),
            "{}",
            refused.detail
        );
        // A malformed grant never reaches the owner (a bound is the owner's: Audit E 4.8).
        fs::remove_file(&log).unwrap();
        let mut bad = grant(
            &BscLiveGrantCommand {
                max_input_wei: "01".into(),
                ..scope.clone()
            },
            "BSC_LIVE_GRANT",
            ConfigVersion(9),
        );
        bad.request_id = "72345678-1234-1234-1234-123456789abc".into();
        assert_eq!(
            control.execute(&bad).await.reject_reason,
            CommandRejectReason::InvalidParams as i32
        );
        assert!(!log.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    /// REV.1.0 R3b (Audit A): the operator's LIVE ledger decisions reach the native ledger as
    /// exactly a lot's RESUME, WRITE_OFF or EXIT_NOW or a transaction's nonce burn with the
    /// ledger's version as CAS, or an entry's quarantine with the owner generation; its receipt
    /// or refusal is the answer; anything else never reaches it.
    #[tokio::test]
    async fn the_live_ledger_decisions_reach_the_native_ledger_with_their_cas() {
        use crate::proto::expected_version::Version::{ConfigVersion, WalletPolicyVersion};
        let request =
            |action: &str, resource: &str, v: Option<expected_version::Version>, id: &str| {
                CommandRequest {
                    project: ProjectId::Bsc as i32,
                    request_id: id.into(),
                    action: action.into(),
                    resource_id: resource.into(),
                    expected: v.map(|v| crate::proto::ExpectedVersion { version: Some(v) }),
                    params: Default::default(),
                    issued_at_unix_ns: crate::overview::unix_ns_now(),
                }
            };
        const ID: &str = "82345678-1234-1234-1234-123456789abc";
        let lot = format!("0x{}", "ab".repeat(32));
        let decided = |action: &str, resource: &str| {
            live_decision(&request(action, resource, Some(ConfigVersion(7)), ID))
        };
        for what in BSC_LIVE_LOT_DECISIONS {
            assert_eq!(
                decided(BSC_LIVE_LOT, &format!("{lot}:{what}")).unwrap(),
                ["bsc", "live", "lot", lot.as_str(), what, "7"]
            );
        }
        assert_eq!(
            decided(BSC_LIVE_NONCE_BURN, &lot).unwrap(),
            ["bsc", "live", "nonce-burn", lot.as_str(), "7"]
        );
        assert_eq!(
            decided(BSC_LIVE_QUARANTINE, "4").unwrap(),
            ["bsc", "live", "ledger-quarantine", "4", "7"]
        );
        let upper = format!("0x{}", "AB".repeat(32));
        let short = format!("0x{}", "ab".repeat(31));
        let bare = "ab".repeat(32);
        for (action, resource) in [
            (BSC_LIVE_LOT, format!("{lot}:SELL")),
            (BSC_LIVE_LOT, format!("{lot}:resume")),
            (BSC_LIVE_LOT, lot.clone()),
            (BSC_LIVE_LOT, format!("{upper}:RESUME")),
            (BSC_LIVE_LOT, format!("{short}:RESUME")),
            (BSC_LIVE_LOT, format!("{bare}:RESUME")),
            (BSC_LIVE_NONCE_BURN, upper.clone()),
            (BSC_LIVE_NONCE_BURN, short.clone()),
            (BSC_LIVE_NONCE_BURN, bare.clone()),
            (BSC_LIVE_NONCE_BURN, format!("{lot}:RESUME")),
            (BSC_LIVE_QUARANTINE, String::new()),
            (BSC_LIVE_QUARANTINE, "04".into()),
            (BSC_LIVE_QUARANTINE, "-4".into()),
            (BSC_LIVE_QUARANTINE, "4.0".into()),
            (BSC_LIVE_QUARANTINE, "18446744073709551616".into()),
            (BSC_LIVE_REVOKE, lot.clone()),
        ] {
            assert_eq!(decided(action, &resource), None, "{action} {resource}");
        }
        assert_eq!(
            live_decision(&request(
                BSC_LIVE_NONCE_BURN,
                &lot,
                Some(WalletPolicyVersion(7)),
                ID
            )),
            None
        );
        assert_eq!(
            live_decision(&request(BSC_LIVE_NONCE_BURN, &lot, None, ID)),
            None
        );
        let mut with_params = request(BSC_LIVE_QUARANTINE, "4", Some(ConfigVersion(7)), ID);
        with_params.params = vec![1u8].into();
        assert_eq!(live_decision(&with_params), None);
        for action in BSC_LIVE_DECISIONS {
            assert!(allowed(ProjectId::Bsc as i32, action));
            assert!(!allowed(ProjectId::Project0 as i32, action));
        }
        // Through the controller: the ledger gets exactly these arguments and the request id;
        // its receipt line is the answer, its refusal too.
        let dir = std::env::temp_dir().join(format!("gw-live-ledger-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("run")).unwrap();
        let ctl = dir.join("darkbotctl");
        let log = dir.join("argv");
        executable(
            &ctl,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ncase \"$3\" in\n  nonce-burn) echo 'STOP: LIVE_NONCE_BURN_OFF: BSC_EXEC_LIVE_NONCE_BURN is OFF' >&2; exit 2 ;;\nesac\necho \"outcome=APPLIED lot=$4 decision=RESUMED fact=2 version=7 request_id=$7\"\n",
                log.display()
            ),
        );
        let control = Control::new(
            Some(Paths {
                ctl,
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "test.capture".into(),
                launchctl: absent_launchctl(&dir),
            }),
            &Limits::default(),
        );
        let applied = control
            .execute(&request(
                BSC_LIVE_LOT,
                &format!("{lot}:RESUME"),
                Some(ConfigVersion(7)),
                ID,
            ))
            .await;
        assert_eq!(
            (applied.status, applied.phase),
            (
                CommandStatus::Accepted as i32,
                CommandPhase::Completed as i32
            ),
            "{}",
            applied.detail
        );
        assert_eq!(
            applied.detail,
            format!("outcome=APPLIED lot={lot} decision=RESUMED fact=2 version=7 request_id={ID}")
        );
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            format!("bsc\nlive\nlot\n{lot}\nRESUME\n7\n{ID}\n")
        );
        let burn_id = "92345678-1234-1234-1234-123456789abc";
        let refused = control
            .execute(&request(
                BSC_LIVE_NONCE_BURN,
                &lot,
                Some(ConfigVersion(7)),
                burn_id,
            ))
            .await;
        assert_eq!(
            refused.phase,
            CommandPhase::Failed as i32,
            "{}",
            refused.detail
        );
        assert!(
            refused.detail.contains("LIVE_NONCE_BURN_OFF"),
            "{}",
            refused.detail
        );
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            format!("bsc\nlive\nnonce-burn\n{lot}\n7\n{burn_id}\n")
        );
        // A malformed decision never reaches the ledger.
        fs::remove_file(&log).unwrap();
        let bad = request(
            BSC_LIVE_QUARANTINE,
            "04",
            Some(ConfigVersion(7)),
            "a2345678-1234-1234-1234-123456789abc",
        );
        assert_eq!(
            control.execute(&bad).await.reject_reason,
            CommandRejectReason::InvalidParams as i32
        );
        assert!(!log.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn the_wallet_venue_and_economic_switches_reach_the_one_native_owner() {
        let request =
            |action: &str, resource: &str, v: Option<expected_version::Version>| CommandRequest {
                project: ProjectId::Bsc as i32,
                request_id: "32345678-1234-1234-1234-123456789abc".into(),
                action: action.into(),
                resource_id: resource.into(),
                expected: v.map(|v| crate::proto::ExpectedVersion { version: Some(v) }),
                params: Default::default(),
                issued_at_unix_ns: crate::overview::unix_ns_now(),
            };
        use expected_version::Version::{ConfigVersion, ProfileVersion, WalletPolicyVersion};
        for (action, resource, v, parsed) in [
            (
                BSC_WALLET,
                "BSC_WALLET_LAUNCHER:ENABLED",
                WalletPolicyVersion(3),
                Some(("BSC_WALLET_LAUNCHER", "ENABLED", 3)),
            ),
            (
                BSC_WALLET,
                "BSC_WALLET_BUYER:DISABLED",
                WalletPolicyVersion(4),
                Some(("BSC_WALLET_BUYER", "DISABLED", 4)),
            ),
            (
                BSC_WALLET,
                "BSC_WALLET_BUYER:DISABLED",
                ConfigVersion(4),
                None,
            ),
            (
                BSC_WALLET,
                "BSC_WALLET_BUYER:LIVE",
                WalletPolicyVersion(4),
                None,
            ),
            (
                BSC_WALLET,
                "BSC_ECON_SEND:ENABLED",
                WalletPolicyVersion(4),
                None,
            ),
            (
                BSC_ECONOMIC,
                "BSC_ECON_SEND:ENABLED",
                ConfigVersion(5),
                Some(("BSC_ECON_SEND", "ENABLED", 5)),
            ),
            (
                BSC_ECONOMIC,
                "BSC_WALLET_BUYER:ENABLED",
                ConfigVersion(5),
                None,
            ),
            (BSC_ECONOMIC, "BSC_ECON_SEND:SHADOW", ConfigVersion(5), None),
            (
                BSC_ECONOMIC,
                "BSC_ECON_SEND:ENABLED",
                ProfileVersion(5),
                None,
            ),
            (
                BSC_VENUE,
                "BSC_VENUE_FLAP:SHADOW",
                ConfigVersion(6),
                Some(("BSC_VENUE_FLAP", "SHADOW", 6)),
            ),
            (BSC_VENUE, "BSC_VENUE_FLAP:ENABLED", ConfigVersion(6), None),
            (BSC_SOURCE, "BSC_VENUE_FLAP:SHADOW", ConfigVersion(6), None),
            (
                BSC_SOURCE,
                "BSC_WALLET_BUYER:ENABLED",
                ConfigVersion(6),
                None,
            ),
            (
                BSC_RUNTIME,
                "BSC_RUNTIME_MODE:PAPER",
                ConfigVersion(7),
                Some(("BSC_RUNTIME_MODE", "PAPER", 7)),
            ),
            (
                BSC_RUNTIME,
                "BSC_RUNTIME_MODE:ENABLED",
                ConfigVersion(7),
                None,
            ),
            (BSC_SOURCE, "BSC_RUNTIME_MODE:PAPER", ConfigVersion(7), None),
            (
                BSC_ATTEMPT_LIMIT,
                "BSC_BUY_ATTEMPTS_PER_DAY:10",
                ConfigVersion(8),
                Some(("BSC_BUY_ATTEMPTS_PER_DAY", "10", 8)),
            ),
            (
                BSC_ATTEMPT_LIMIT,
                "BSC_BUY_ATTEMPTS_PER_DAY:0",
                ConfigVersion(8),
                Some(("BSC_BUY_ATTEMPTS_PER_DAY", "0", 8)),
            ),
            (
                BSC_ATTEMPT_LIMIT,
                "BSC_BUY_ATTEMPTS_PER_DAY:010",
                ConfigVersion(8),
                None,
            ),
            (
                BSC_ATTEMPT_LIMIT,
                "BSC_BUY_ATTEMPTS_PER_DAY:-1",
                ConfigVersion(8),
                None,
            ),
            (
                BSC_ATTEMPT_LIMIT,
                "BSC_BUY_ATTEMPTS_PER_DAY:4294967296",
                ConfigVersion(8),
                None,
            ),
            (
                BSC_ATTEMPT_LIMIT,
                "BSC_BUY_ATTEMPTS_PER_DAY:x",
                ConfigVersion(8),
                None,
            ),
            (
                BSC_ATTEMPT_LIMIT,
                "BSC_BUY_ATTEMPTS_PER_DAY:3",
                WalletPolicyVersion(8),
                None,
            ),
        ] {
            assert_eq!(
                sr(&request(action, resource, Some(v))),
                parsed,
                "{action} {resource}"
            );
        }
        let mut with_params = request(
            BSC_WALLET,
            "BSC_WALLET_BUYER:ENABLED",
            Some(WalletPolicyVersion(1)),
        );
        with_params.params = vec![1].into();
        assert_eq!(sr(&with_params), None);
        for action in [
            BSC_VENUE,
            BSC_WALLET,
            BSC_ECONOMIC,
            BSC_RUNTIME,
            BSC_ATTEMPT_LIMIT,
        ] {
            assert!(allowed(ProjectId::Bsc as i32, action));
            assert!(!allowed(ProjectId::Project0 as i32, action));
        }

        let dir = std::env::temp_dir().join(format!("gw-switch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("run")).unwrap();
        let ctl = dir.join("darkbotctl");
        let log = dir.join("argv");
        // A fake owner: applies an ENABLED request at generation 3, refuses anything else stale.
        executable(
            &ctl,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nif [ \"$6\" = 3 ]; then echo \"outcome=APPLIED source=$4 requested=$5 effective=$5 generation=4 request_id=$7\"; else echo \"outcome=REJECTED_STALE source=$4 requested=$5 effective=DISABLED generation=4 request_id=$7\"; exit 2; fi\n",
                log.display()
            ),
        );
        let control = Control::new(
            Some(Paths {
                ctl,
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "test.capture".into(),
                launchctl: absent_launchctl(&dir),
            }),
            &Limits::default(),
        );
        let req = request(
            BSC_WALLET,
            "BSC_WALLET_LAUNCHER:ENABLED",
            Some(WalletPolicyVersion(3)),
        );
        let applied = control.execute(&req).await;
        assert_eq!(
            (applied.status, applied.phase),
            (
                CommandStatus::Accepted as i32,
                CommandPhase::Completed as i32
            ),
            "{}",
            applied.detail
        );
        assert!(applied
            .detail
            .starts_with("outcome=APPLIED source=BSC_WALLET_LAUNCHER requested=ENABLED"));
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            "bsc\nsource\nrequest\nBSC_WALLET_LAUNCHER\nENABLED\n3\n32345678-1234-1234-1234-123456789abc\n"
        );
        // The same request again: the stored receipt, no second dispatch.
        fs::remove_file(&log).unwrap();
        assert_eq!(
            control.execute(&req).await.phase,
            CommandPhase::Completed as i32
        );
        assert!(!log.exists());
        // A stale generation is the owner's refusal: a failed receipt, nothing switched.
        let mut stale = request(
            BSC_ECONOMIC,
            "BSC_ECON_SEND:ENABLED",
            Some(ConfigVersion(2)),
        );
        stale.request_id = "42345678-1234-1234-1234-123456789abc".into();
        let refused = control.execute(&stale).await;
        assert_eq!(
            (refused.status, refused.phase),
            (CommandStatus::Accepted as i32, CommandPhase::Failed as i32)
        );
        assert!(
            refused.detail.starts_with("outcome=REJECTED_STALE"),
            "{}",
            refused.detail
        );
        // A malformed one never reaches the owner.
        fs::remove_file(&log).unwrap();
        let mut bad = request(
            BSC_WALLET,
            "BSC_WALLET_LAUNCHER:LIVE",
            Some(WalletPolicyVersion(3)),
        );
        bad.request_id = "52345678-1234-1234-1234-123456789abc".into();
        let rejected = control.execute(&bad).await;
        assert_eq!(rejected.status, CommandStatus::Rejected as i32);
        assert_eq!(
            rejected.reject_reason,
            CommandRejectReason::InvalidParams as i32
        );
        assert!(!log.exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn bsc_commands_are_strictly_read_only() {
        for action in [
            "g07",
            "arm",
            "probe",
            "send",
            "bsc.g07",
            "bsc.arm",
            "bsc.probe",
            "bsc.send",
            "bsc.dry.start",
            START,
        ] {
            assert!(!allowed(ProjectId::Bsc as i32, action));
        }
        assert!(allowed(ProjectId::Bsc as i32, BSC_START));
        assert!(allowed(ProjectId::Bsc as i32, BSC_STOP));
        assert!(allowed(ProjectId::Bsc as i32, BSC_CANDIDATE));
        assert!(!allowed(ProjectId::Project0 as i32, BSC_CANDIDATE));
        assert!(!allowed(ProjectId::Solana as i32, BSC_START));
    }
    /// DEC-07: the boot actions map 1:1 onto `p0ctl boot`: a new flow is the request id with its
    /// mode and TTL; the approvals need a flow id, a 16-character token and their typed options;
    /// the stop names its flow; anything else is refused before the controller.
    #[test]
    fn dec07_boot_actions_map_onto_p0ctl_boot_and_refuse_malformed_requests() {
        use crate::proto::project0::v1::{
            BootNetworkCommand, BootOfflineCommand, BootStartCommand,
        };
        use expected_version::Version::ConfigVersion;
        let req =
            |action: &str, resource: &str, params: Vec<u8>, version: Option<u64>| CommandRequest {
                project: ProjectId::Project0 as i32,
                request_id: "12345678-1234-1234-1234-123456789abc".into(),
                action: action.into(),
                resource_id: resource.into(),
                expected: version.map(|v| crate::proto::ExpectedVersion {
                    version: Some(ConfigVersion(v)),
                }),
                params: params.into(),
                issued_at_unix_ns: 0,
            };
        let offline = |mode: &str, ttl| {
            BootOfflineCommand {
                mode: mode.into(),
                ttl_seconds: ttl,
                duration: "1h".into(),
            }
            .encode_to_vec()
        };
        assert_eq!(
            job_args(&req(BOOT_OFFLINE, "", offline("econ4h", Some(90)), None)).unwrap(),
            [
                "p0",
                "boot",
                "offline",
                "--flow",
                "12345678-1234-1234-1234-123456789abc",
                "--mode",
                "econ4h",
                "--duration",
                "1h",
                "--",
                "--boot-token-ttl-seconds",
                "90"
            ]
        );
        assert_eq!(
            job_args(&req(BOOT_OFFLINE, "", offline("shadow", None), None))
                .unwrap()
                .len(),
            9
        );
        for duration in ["", "1é", "10x"] {
            let c = BootOfflineCommand {
                mode: "econ4h".into(),
                ttl_seconds: None,
                duration: duration.into(),
            };
            assert_eq!(
                job_args(&req(BOOT_OFFLINE, "", c.encode_to_vec(), None)),
                None,
                "{duration}"
            );
        }
        assert_eq!(
            job_args(&req(BOOT_OFFLINE, "", offline("live", None), None)),
            None
        );
        assert_eq!(
            job_args(&req(BOOT_OFFLINE, "", offline("econ4h", None), Some(1))),
            None,
            "a boot request takes no configuration version"
        );
        let network = |token: &str, evidence: &str| {
            BootNetworkCommand {
                flow_id: "flow-1".into(),
                approved_token: token.into(),
                genesis_hash: "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d".into(),
                provider_evidence: evidence.into(),
                capture_source: "logs".into(),
            }
            .encode_to_vec()
        };
        let ok = job_args(&req(
            BOOT_NETWORK,
            "",
            network("0123456789abcdef", "89abcdef01234567"),
            None,
        ))
        .unwrap();
        assert_eq!(ok[9..11], ["--provider-evidence", "89abcdef01234567"]);
        assert_eq!(
            ok[..7],
            [
                "p0",
                "boot",
                "network",
                "--flow",
                "flow-1",
                "--approve",
                "0123456789abcdef"
            ]
        );
        for (token, evidence) in [
            ("0123456789ABCDEF", "89abcdef01234567"),
            ("0123", "89abcdef01234567"),
            // DEC-07: the evidence is a record id, never a path.
            ("0123456789abcdef", "/data/evidence.p0j"),
            ("0123456789abcdef", "relative.p0j"),
            ("0123456789abcdef", "/a/../b.p0j"),
            ("0123456789abcdef", "89ABCDEF01234567"),
            ("0123456789abcdef", "89abcdef0123456"),
            ("0123456789abcdef", "/89abcdef0123456"),
        ] {
            assert_eq!(
                job_args(&req(BOOT_NETWORK, "", network(token, evidence), None)),
                None,
                "{token} {evidence}"
            );
        }
        let start = |duration: &str, verify: &str| {
            BootStartCommand {
                flow_id: "flow-1".into(),
                approved_token: "fedcba9876543210".into(),
                duration: duration.into(),
                capture_source: "transactions".into(),
                chain_verify: verify.into(),
                qualification: None,
            }
            .encode_to_vec()
        };
        assert_eq!(
            job_args(&req(BOOT_START, "", start("10m", "off"), None)).unwrap()[..3],
            ["p0", "boot", "start"]
        );
        for (duration, verify) in [
            ("10x", "off"),
            ("", "off"),
            ("1é", "off"),
            ("10m", "index"),
            ("10m", "strict"),
        ] {
            assert_eq!(
                job_args(&req(BOOT_START, "", start(duration, verify), None)),
                None,
                "{duration} {verify}"
            );
        }
        assert_eq!(
            job_args(&req(BOOT_CANCEL, "flow-1", vec![], None)).unwrap(),
            ["p0", "boot", "cancel", "--flow", "flow-1"]
        );
        assert_eq!(job_args(&req(BOOT_CANCEL, "../x", vec![], None)), None);
        for action in BOOT_ACTIONS {
            assert!(allowed(ProjectId::Project0 as i32, action));
            assert!(!allowed(ProjectId::Bsc as i32, action));
        }
    }
    /// DEC-16: an unbounded flow's horizon and its qualification reach `p0ctl boot`; the standing
    /// autostart approval maps 1:1 onto `p0ctl autostart preview|grant|revoke` with typed,
    /// checked options (the controller validates them again); anything else is refused.
    #[test]
    fn dec16_unbounded_boot_and_autostart_actions_map_onto_p0ctl() {
        use crate::proto::project0::v1::{
            AutostartGrantCommand, AutostartPreviewCommand, BootOfflineCommand, BootStartCommand,
        };
        let req = |action: &str, resource: &str, params: Vec<u8>| CommandRequest {
            project: ProjectId::Project0 as i32,
            request_id: "12345678-1234-1234-1234-123456789abc".into(),
            action: action.into(),
            resource_id: resource.into(),
            expected: None,
            params: params.into(),
            issued_at_unix_ns: 0,
        };
        let offline = |duration: &str| {
            BootOfflineCommand {
                mode: "market".into(),
                ttl_seconds: None,
                duration: duration.into(),
            }
            .encode_to_vec()
        };
        assert_eq!(
            job_args(&req(BOOT_OFFLINE, "", offline("until-stopped:1h:24h"))).unwrap()[6..],
            ["market", "--duration", "until-stopped:1h:24h"]
        );
        for bad in [
            "until-stopped",
            "until-stopped:1h",
            "until-stopped:1h:",
            "until-stopped:1x:24h",
            "until-stopped:1h:24h:1h",
            "inf",
        ] {
            assert_eq!(
                job_args(&req(BOOT_OFFLINE, "", offline(bad))),
                None,
                "{bad}"
            );
        }
        let start = |qualification: Option<&str>| {
            BootStartCommand {
                flow_id: "flow-1".into(),
                approved_token: "fedcba9876543210".into(),
                duration: "until-stopped:1h:24h".into(),
                capture_source: "logs".into(),
                chain_verify: "off".into(),
                qualification: qualification.map(Into::into),
            }
            .encode_to_vec()
        };
        let args = job_args(&req(BOOT_START, "", start(Some("0123456789abcdef")))).unwrap();
        assert_eq!(
            args[args.len() - 2..],
            ["--qualification", "0123456789abcdef"]
        );
        assert!(!job_args(&req(BOOT_START, "", start(None)))
            .unwrap()
            .contains(&"--qualification".to_string()));
        for bad in ["0123", "0123456789ABCDEF", "../0123456789ab"] {
            assert_eq!(
                job_args(&req(BOOT_START, "", start(Some(bad)))),
                None,
                "{bad}"
            );
        }
        let preview = |f: &dyn Fn(&mut AutostartPreviewCommand)| {
            let mut c = AutostartPreviewCommand {
                qualification: "0123456789abcdef".into(),
                checkpoint: "1h".into(),
                minimum_runway: "24h".into(),
                genesis_hash: "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d".into(),
                max_runtime: "7d".into(),
                valid_for: "30d".into(),
                capture_source: "logs".into(),
            };
            f(&mut c);
            job_args(&req(AUTOSTART_PREVIEW, "", c.encode_to_vec()))
        };
        assert_eq!(
            preview(&|_| {}).unwrap(),
            [
                "p0",
                "autostart",
                "preview",
                "--qualification",
                "0123456789abcdef",
                "--checkpoint",
                "1h",
                "--minimum-runway",
                "24h",
                "--genesis-hash",
                "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d",
                "--max-runtime",
                "7d",
                "--valid-for",
                "30d",
                "--capture-source",
                "logs"
            ]
        );
        let broken: [&dyn Fn(&mut AutostartPreviewCommand); 6] = [
            &|c| c.qualification = "x".into(),
            &|c| c.checkpoint = "1x".into(),
            &|c| c.max_runtime = String::new(),
            &|c| c.valid_for = "until-stopped".into(),
            &|c| c.genesis_hash = "../x".into(),
            &|c| c.capture_source = "blocks".into(),
        ];
        for (i, f) in broken.iter().enumerate() {
            assert_eq!(preview(f), None, "{i}");
        }
        let grant = |digest: &str| {
            job_args(&req(
                AUTOSTART_GRANT,
                "",
                AutostartGrantCommand {
                    confirm_digest: digest.into(),
                }
                .encode_to_vec(),
            ))
        };
        assert_eq!(
            grant("89abcdef01234567").unwrap(),
            ["p0", "autostart", "grant", "--confirm", "89abcdef01234567"]
        );
        assert_eq!(grant("89ABCDEF01234567"), None);
        assert_eq!(
            job_args(&req(AUTOSTART_REVOKE, "", vec![])).unwrap(),
            ["p0", "autostart", "revoke"]
        );
        assert_eq!(job_args(&req(AUTOSTART_REVOKE, "x", vec![])), None);
        assert_eq!(job_args(&req(AUTOSTART_REVOKE, "", vec![1])), None);
        for action in AUTOSTART_ACTIONS {
            assert!(allowed(ProjectId::Project0 as i32, action));
            assert!(!allowed(ProjectId::Bsc as i32, action));
        }
    }
    #[test]
    fn only_a_source_change_carries_the_configuration_version_cas() {
        use crate::proto::project0::v1::SourceControlCommand;
        use expected_version::Version::*;
        let req = |action: &str,
                   resource: &str,
                   params: Vec<u8>,
                   expected: Option<expected_version::Version>| CommandRequest {
            project: ProjectId::Project0 as i32,
            request_id: "12345678-1234-1234-1234-123456789abc".into(),
            action: action.into(),
            resource_id: resource.into(),
            expected: expected.map(|v| crate::proto::ExpectedVersion { version: Some(v) }),
            params: params.into(),
            issued_at_unix_ns: 0,
        };
        let source = SourceControlCommand {
            unit: "geyser:publicnode".into(),
            mode: "off".into(),
        }
        .encode_to_vec();
        let cas = job_args(&req(SOURCE_SET, "", source.clone(), Some(ConfigVersion(4)))).unwrap();
        assert_eq!(
            cas[3..],
            ["geyser:publicnode", "off", "--expect-version", "4"]
        );
        // Required (review 5353cdcb): a source change without its version is refused.
        assert_eq!(job_args(&req(SOURCE_SET, "", source.clone(), None)), None);
        assert_eq!(
            job_args(&req(SOURCE_SET, "", source, Some(ProfileVersion(4)))),
            None
        );
        // DEC-13/DEC-04/DEC-11/B01/P6-MT.10: a capability request carries its version; only
        // LIVE OFF|LIVE and HOLD_B/JITO_BUNDLE/THRESHOLD_TRIGGER/INVENTORY_REBALANCING/
        // LOCAL_FEE_MODEL OFF|OBSERVE|ACTIVE pass (P8-CTRL: and the economic switches, the
        // signers and FE-06).
        let capability = |capability: &str, requested: &str| {
            crate::proto::project0::v1::CapabilityControlCommand {
                capability: capability.into(),
                requested: requested.into(),
                owner_epoch: 0,
            }
            .encode_to_vec()
        };
        let in_epoch = |capability: &str, requested: &str, owner_epoch: u64| {
            crate::proto::project0::v1::CapabilityControlCommand {
                capability: capability.into(),
                requested: requested.into(),
                owner_epoch,
            }
            .encode_to_vec()
        };
        let live = job_args(&req(
            CAPABILITY_SET,
            "",
            in_epoch("LIVE", "LIVE", 3),
            Some(ConfigVersion(2)),
        ))
        .unwrap();
        assert_eq!(
            live,
            [
                "p0",
                "capability",
                "set",
                "LIVE",
                "LIVE",
                "--expect-version",
                "2",
                "--expect-epoch",
                "3",
                "--request-id",
                "12345678-1234-1234-1234-123456789abc"
            ]
        );
        // Review M2: every request but a move to OFF states the owner epoch — a view from
        // before an owner restart never re-arms LIVE or a mode in the new epoch.
        for (c, v) in [
            ("LIVE", "LIVE"),
            ("HOLD_B", "ACTIVE"),
            ("HOLD_B", "OBSERVE"),
            ("JITO_BUNDLE", "OBSERVE"),
            ("THRESHOLD_TRIGGER", "ACTIVE"),
            ("INVENTORY_REBALANCING", "OBSERVE"),
            ("LOCAL_FEE_MODEL", "ACTIVE"),
        ] {
            assert_eq!(
                job_args(&req(
                    CAPABILITY_SET,
                    "",
                    capability(c, v),
                    Some(ConfigVersion(2))
                )),
                None,
                "{c} {v} without its epoch"
            );
        }
        for c in [
            "LIVE",
            "HOLD_B",
            "JITO_BUNDLE",
            "THRESHOLD_TRIGGER",
            "INVENTORY_REBALANCING",
            "LOCAL_FEE_MODEL",
        ] {
            assert_eq!(
                job_args(&req(
                    CAPABILITY_SET,
                    "",
                    capability(c, "OFF"),
                    Some(ConfigVersion(2))
                ))
                .unwrap()[3..7],
                [c, "OFF", "--expect-version", "2"],
                "{c} OFF needs no epoch"
            );
        }
        // P8-CTRL.1/.3/.6: the economic, wallet and grant changes state their epoch.
        let digest = "ab".repeat(32);
        for (c, v) in [
            ("ECON_SIGN", "ENABLED"),
            ("ECON_LIVE", "DISABLED"),
            ("WALLET:11111111111111111111111111111112", "ENABLED"),
            ("FE06_GRANT", digest.as_str()),
            ("FE06_REVOKE", digest.as_str()),
        ] {
            assert_eq!(
                job_args(&req(
                    CAPABILITY_SET,
                    "",
                    in_epoch(c, v, 3),
                    Some(ConfigVersion(7))
                ))
                .unwrap()[3..],
                [
                    c,
                    v,
                    "--expect-version",
                    "7",
                    "--expect-epoch",
                    "3",
                    "--request-id",
                    "12345678-1234-1234-1234-123456789abc"
                ]
            );
            assert_eq!(
                job_args(&req(
                    CAPABILITY_SET,
                    "",
                    in_epoch(c, v, 0),
                    Some(ConfigVersion(7))
                )),
                None,
                "{c} without its epoch"
            );
        }
        for (c, v) in [
            ("ECON_SIGN", "ON"),
            ("ECON_FOO", "ENABLED"),
            ("WALLET:", "ENABLED"),
            ("WALLET:0OIl0OIl0OIl0OIl0OIl0OIl0OIl0OIl", "ENABLED"),
            ("WALLET:11111111111111111111111111111112", "ACTIVE"),
            ("FE06_GRANT", "ABCD"),
            ("FE06_REVOKE", &"A".repeat(64)),
        ] {
            assert_eq!(
                job_args(&req(
                    CAPABILITY_SET,
                    "",
                    in_epoch(c, v, 3),
                    Some(ConfigVersion(7))
                )),
                None,
                "{c} {v}"
            );
        }
        assert_eq!(
            job_args(&req(
                CAPABILITY_SET,
                "",
                in_epoch("HOLD_B", "ACTIVE", 3),
                Some(ConfigVersion(3)),
            ))
            .unwrap()[3..5],
            ["HOLD_B", "ACTIVE"]
        );
        assert_eq!(
            job_args(&req(
                CAPABILITY_SET,
                "",
                in_epoch("JITO_BUNDLE", "OBSERVE", 3),
                Some(ConfigVersion(4)),
            ))
            .unwrap()[3..7],
            ["JITO_BUNDLE", "OBSERVE", "--expect-version", "4"]
        );
        // DEC-06: the threshold mode's hot change.
        assert_eq!(
            job_args(&req(
                CAPABILITY_SET,
                "",
                in_epoch("THRESHOLD_TRIGGER", "ACTIVE", 3),
                Some(ConfigVersion(5)),
            ))
            .unwrap()[3..7],
            ["THRESHOLD_TRIGGER", "ACTIVE", "--expect-version", "5"]
        );
        // B01: inventory rebalancing's hot change.
        assert_eq!(
            job_args(&req(
                CAPABILITY_SET,
                "",
                in_epoch("INVENTORY_REBALANCING", "OBSERVE", 3),
                Some(ConfigVersion(6)),
            ))
            .unwrap()[3..7],
            ["INVENTORY_REBALANCING", "OBSERVE", "--expect-version", "6"]
        );
        // P6-MT.10: the local fee model's hot change.
        assert_eq!(
            job_args(&req(
                CAPABILITY_SET,
                "",
                in_epoch("LOCAL_FEE_MODEL", "ACTIVE", 3),
                Some(ConfigVersion(7)),
            ))
            .unwrap()[3..7],
            ["LOCAL_FEE_MODEL", "ACTIVE", "--expect-version", "7"]
        );
        for (c, v, expected) in [
            ("LIVE", "LIVE", None),
            ("JITO", "LIVE", Some(ConfigVersion(2))),
            ("LIVE", "ON", Some(ConfigVersion(2))),
            ("LIVE", "OBSERVE", Some(ConfigVersion(2))),
            ("HOLD_B", "LIVE", Some(ConfigVersion(2))),
            ("JITO_BUNDLE", "LIVE", Some(ConfigVersion(2))),
            ("JITO_BUNDLE", "OBSERVE", None),
            ("THRESHOLD_TRIGGER", "SWEEP", Some(ConfigVersion(2))),
            ("THRESHOLD_TRIGGER", "OBSERVE", None),
            ("INVENTORY_REBALANCING", "ON", Some(ConfigVersion(2))),
            ("INVENTORY_REBALANCING", "OBSERVE", None),
            ("LOCAL_FEE_MODEL", "ON", Some(ConfigVersion(2))),
            ("LOCAL_FEE_MODEL", "ACTIVE", None),
        ] {
            assert_eq!(
                job_args(&req(CAPABILITY_SET, "", capability(c, v), expected)),
                None
            );
        }
        assert!(allowed(ProjectId::Project0 as i32, CAPABILITY_SET));
        assert!(!allowed(ProjectId::Bsc as i32, CAPABILITY_SET));
        assert_eq!(
            job_args(&req(JOB_CANCEL, "job-1", vec![], None))
                .unwrap()
                .len(),
            5
        );
        assert_eq!(
            job_args(&req(JOB_CANCEL, "job-1", vec![], Some(ConfigVersion(1)))),
            None
        );
    }
    #[test]
    fn a_candidate_request_takes_only_a_fixed_profile_and_the_generation() {
        let request = |resource: &str,
                       expected: Option<expected_version::Version>,
                       params: &[u8]| CommandRequest {
            project: ProjectId::Bsc as i32,
            request_id: "12345678-1234-1234-1234-123456789abc".into(),
            action: BSC_CANDIDATE.into(),
            resource_id: resource.into(),
            expected: expected.map(|v| crate::proto::ExpectedVersion { version: Some(v) }),
            params: params.to_vec().into(),
            issued_at_unix_ns: 0,
        };
        use expected_version::Version::*;
        assert_eq!(
            candidate_request(&request("SELF_B", Some(ProfileVersion(3)), b"")),
            Some(("SELF_B", 3))
        );
        assert_eq!(
            candidate_request(&request("SELF_A", Some(ProfileVersion(0)), b"")),
            Some(("SELF_A", 0))
        );
        for bad in [
            request("SELF_C", Some(ProfileVersion(3)), b""),
            request("../SELF_A", Some(ProfileVersion(3)), b""),
            request("", Some(ProfileVersion(3)), b""),
            request("SELF_A", None, b""),
            request("SELF_A", Some(ConfigVersion(3)), b""),
            request("SELF_A", Some(ProfileVersion(3)), b"x"),
        ] {
            assert_eq!(candidate_request(&bad), None);
        }
    }
    #[tokio::test]
    async fn a_candidate_request_dispatches_one_fixed_native_command() {
        let dir = std::env::temp_dir().join(format!("gw-candidate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("bsc-root")).unwrap();
        fs::write(dir.join("bsc-root/SNAPSHOT_SHA256"), "a").unwrap();
        fs::write(dir.join("g07_pre_live_readiness"), "").unwrap();
        idle_reporter(&dir);
        let ctl = dir.join("darkbotctl");
        let log = dir.join("argv");
        executable(
            &ctl,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n[ \"$4\" = SELF_B ]\n",
                log.display()
            ),
        );
        let control = Control::new(
            Some(Paths {
                ctl,
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "test.capture".into(),
                launchctl: absent_launchctl(&dir),
            }),
            &Limits::default(),
        );
        fs::create_dir_all(dir.join("run")).unwrap();
        let mut req = CommandRequest {
            project: ProjectId::Bsc as i32,
            request_id: "12345678-1234-1234-1234-123456789abc".into(),
            action: BSC_CANDIDATE.into(),
            resource_id: "SELF_B".into(),
            expected: Some(crate::proto::ExpectedVersion {
                version: Some(expected_version::Version::ProfileVersion(7)),
            }),
            params: Default::default(),
            issued_at_unix_ns: crate::overview::unix_ns_now(),
        };
        let response = control.execute(&req).await;
        assert_eq!(
            response.status,
            CommandStatus::Accepted as i32,
            "{}",
            response.detail
        );
        // The response already carries the decided ledger phase; a replay returns it again.
        assert_eq!(response.phase, CommandPhase::Completed as i32);
        assert_eq!(
            control.execute(&req).await.phase,
            CommandPhase::Completed as i32
        );
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            "bsc\ncandidate\nrequest\nSELF_B\n7\n12345678-1234-1234-1234-123456789abc\n"
        );
        assert_eq!(
            control
                .lookup(&req.request_id, ProjectId::Bsc as i32)
                .await
                .phase,
            CommandPhase::Completed as i32
        );
        // A native refusal is a failed receipt, never a completed switch.
        req.request_id = "22345678-1234-1234-1234-123456789abc".into();
        req.resource_id = "SELF_A".into();
        let refused = control.execute(&req).await;
        assert_eq!(
            (refused.status, refused.phase),
            (CommandStatus::Accepted as i32, CommandPhase::Failed as i32)
        );
        assert_eq!(
            control
                .lookup(&req.request_id, ProjectId::Bsc as i32)
                .await
                .phase,
            CommandPhase::Failed as i32
        );
        fs::remove_dir_all(dir).unwrap();
    }
    /// Frontend package (06.11.): a Strategy C setting is exactly `<KEY>:<VALUE>` with a key from
    /// the fixed set, a value of printable ASCII without spaces (or none) and the displayed
    /// settings generation as `profile_version`; it reaches `darkbotctl bsc strategy set` with the
    /// request id, and the owner's receipt line is the detail (a refused value fails).
    #[tokio::test]
    async fn a_strategy_setting_reaches_the_native_owner_with_its_value() {
        let request = |id: &str, resource: &str, expected: Option<expected_version::Version>| {
            CommandRequest {
                project: ProjectId::Bsc as i32,
                request_id: id.into(),
                action: BSC_STRATEGY_SET.into(),
                resource_id: resource.into(),
                expected: expected.map(|v| crate::proto::ExpectedVersion { version: Some(v) }),
                params: Default::default(),
                issued_at_unix_ns: crate::overview::unix_ns_now(),
            }
        };
        let profile = |g| Some(expected_version::Version::ProfileVersion(g));
        let args = |v: &[&str]| Some(v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(
            strategy_request(&request(
                "x",
                "BSC_STRATEGY_C_FILTERS:buy_count:3:,unique_buyers:2:",
                profile(4)
            )),
            args(&[
                "bsc",
                "strategy",
                "set",
                "BSC_STRATEGY_C_FILTERS",
                "buy_count:3:,unique_buyers:2:",
                "4"
            ])
        );
        assert_eq!(
            strategy_request(&request("x", "BSC_STRATEGY_C_FILTERS:", profile(0))),
            args(&["bsc", "strategy", "set", "BSC_STRATEGY_C_FILTERS", "", "0"])
        );
        for (resource, expected) in [
            ("BSC_STRATEGY_C_MAX_OPEN", profile(4)),
            ("BSC_STRATEGY_C_GAS_PRICE_WEI:1", profile(4)),
            ("BSC_EXEC_INPUT_WEI:1", profile(4)),
            ("BSC_STRATEGY_C_MAX_OPEN:3 4", profile(4)),
            ("BSC_STRATEGY_C_MAX_OPEN:3\n", profile(4)),
            ("BSC_STRATEGY_C_MAX_OPEN:3", None),
            (
                "BSC_STRATEGY_C_MAX_OPEN:3",
                Some(expected_version::Version::ConfigVersion(4)),
            ),
        ] {
            assert_eq!(
                strategy_request(&request("x", resource, expected)),
                None,
                "{resource}"
            );
        }
        let mut with_params = request("x", "BSC_STRATEGY_C_MAX_OPEN:3", profile(4));
        with_params.params = vec![1].into();
        assert_eq!(strategy_request(&with_params), None);
        assert!(allowed(ProjectId::Bsc as i32, BSC_STRATEGY_SET));
        assert!(!allowed(ProjectId::Project0 as i32, BSC_STRATEGY_SET));

        let dir = std::env::temp_dir().join(format!("gw-strategy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("run")).unwrap();
        let ctl = dir.join("darkbotctl");
        let log = dir.join("argv");
        // The owner applies generation 4 and refuses the value 0.
        executable(
            &ctl,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n[ \"$5\" = 0 ] && {{ echo \"outcome=REJECTED_INVALID id=$4 value=$5 generation=4 request_id=$7 reason=out of range\"; exit 2; }}\necho \"outcome=APPLIED id=$4 value=$5 generation=5 request_id=$7\"\n",
                log.display()
            ),
        );
        let control = Control::new(
            Some(Paths {
                ctl,
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "test.capture".into(),
                launchctl: absent_launchctl(&dir),
            }),
            &Limits::default(),
        );
        let set = request(
            "42345678-1234-1234-1234-123456789abc",
            "BSC_STRATEGY_C_MAX_OPEN:3",
            profile(4),
        );
        let r = control.execute(&set).await;
        assert_eq!(
            (r.status, r.phase),
            (
                CommandStatus::Accepted as i32,
                CommandPhase::Completed as i32
            ),
            "{}",
            r.detail
        );
        assert!(
            r.detail
                .starts_with("outcome=APPLIED id=BSC_STRATEGY_C_MAX_OPEN value=3"),
            "{}",
            r.detail
        );
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            "bsc\nstrategy\nset\nBSC_STRATEGY_C_MAX_OPEN\n3\n4\n42345678-1234-1234-1234-123456789abc\n"
        );
        let refused = request(
            "52345678-1234-1234-1234-123456789abc",
            "BSC_STRATEGY_C_MAX_OPEN:0",
            profile(4),
        );
        let r = control.execute(&refused).await;
        assert_eq!(
            (r.status, r.phase),
            (CommandStatus::Accepted as i32, CommandPhase::Failed as i32),
            "{}",
            r.detail
        );
        assert!(
            r.detail.starts_with("outcome=REJECTED_INVALID"),
            "{}",
            r.detail
        );
        let r = control
            .execute(&request(
                "62345678-1234-1234-1234-123456789abc",
                "BSC_STRATEGY_C_MAX_OPEN:3",
                None,
            ))
            .await;
        assert_eq!(r.status, CommandStatus::Rejected as i32, "{}", r.detail);
        fs::remove_dir_all(dir).unwrap();
    }
    /// Order V2 §5: a BSC source request is exactly `<SOURCE_ID>:<MODE>` from the fixed sets with
    /// the displayed generation as `config_version`; it reaches the native owner through
    /// `darkbotctl bsc source request` without a readiness launch, and the owner's one-line
    /// receipt is the detail (APPLIED completes, REJECTED_STALE fails); nothing else passes.
    #[tokio::test]
    async fn a_source_request_reaches_the_native_owner_and_returns_its_receipt() {
        let request = |id: &str, resource: &str, expected: Option<expected_version::Version>| {
            CommandRequest {
                project: ProjectId::Bsc as i32,
                request_id: id.into(),
                action: BSC_SOURCE.into(),
                resource_id: resource.into(),
                expected: expected.map(|v| crate::proto::ExpectedVersion { version: Some(v) }),
                params: Default::default(),
                issued_at_unix_ns: crate::overview::unix_ns_now(),
            }
        };
        let config = |g| Some(expected_version::Version::ConfigVersion(g));
        // Parsing: only known sources and modes, only config_version, no parameters.
        assert_eq!(
            sr(&request("x", "BSC_INGRESS_DRPC:LIVE", config(4))),
            Some(("BSC_INGRESS_DRPC", "LIVE", 4))
        );
        // The C2 state component is switched by the same owner and action.
        assert_eq!(
            sr(&request("x", "BSC_STATE_C2:SHADOW", config(4))),
            Some(("BSC_STATE_C2", "SHADOW", 4))
        );
        assert_eq!(
            sr(&request("x", "BSC_EXEC:LIVE", config(5))),
            Some(("BSC_EXEC", "LIVE", 5))
        );
        assert_eq!(
            sr(&request("x", "BSC_STRATEGY_C:SHADOW", config(6))),
            Some(("BSC_STRATEGY_C", "SHADOW", 6))
        );
        assert_eq!(sr(&request("x", "BSC_STATE_C3:SHADOW", config(4))), None);
        for (resource, expected) in [
            ("BSC_INGRESS_DRPC", config(4)),
            ("BSC_INGRESS_OTHER:SHADOW", config(4)),
            ("BSC_INGRESS_DRPC:ARMED", config(4)),
            ("BSC_INGRESS_DRPC:SHADOW", None),
            (
                "BSC_INGRESS_DRPC:SHADOW",
                Some(expected_version::Version::ProfileVersion(4)),
            ),
        ] {
            assert_eq!(sr(&request("x", resource, expected)), None, "{resource}");
        }
        let mut with_params = request("x", "BSC_INGRESS_DRPC:SHADOW", config(4));
        with_params.params = vec![1].into();
        assert_eq!(sr(&with_params), None);
        assert!(allowed(ProjectId::Bsc as i32, BSC_SOURCE));
        assert!(!allowed(ProjectId::Project0 as i32, BSC_SOURCE));

        let dir = std::env::temp_dir().join(format!("gw-source-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("run")).unwrap();
        let ctl = dir.join("darkbotctl");
        let log = dir.join("argv");
        // The controller prints the owner's receipt line; generation 4 is current.
        executable(
            &ctl,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n[ \"$6\" = 4 ] && {{ echo \"outcome=APPLIED source=$4 requested=$5 effective=BLOCKED block_reason=ECONOMIC_LIVE_NOT_AUTHORIZED generation=5 request_id=$7\"; exit 0; }}\necho \"outcome=REJECTED_STALE source=$4 requested=$5 effective=OFF generation=5 request_id=$7\"; exit 2\n",
                log.display()
            ),
        );
        let control = Control::new(
            Some(Paths {
                ctl,
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "test.capture".into(),
                launchctl: absent_launchctl(&dir),
            }),
            &Limits::default(),
        );
        let live = request(
            "32345678-1234-1234-1234-123456789abc",
            "BSC_INGRESS_PUBLICNODE:LIVE",
            config(4),
        );
        let r = control.execute(&live).await;
        assert_eq!(
            (r.status, r.phase),
            (
                CommandStatus::Accepted as i32,
                CommandPhase::Completed as i32
            ),
            "{}",
            r.detail
        );
        assert!(
            r.detail.starts_with("outcome=APPLIED") && r.detail.contains("effective=BLOCKED"),
            "{}",
            r.detail
        );
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            "bsc\nsource\nrequest\nBSC_INGRESS_PUBLICNODE\nLIVE\n4\n32345678-1234-1234-1234-123456789abc\n"
        );
        // A replay returns the stored response with the decided phase, no second dispatch;
        // the receipt line is the ledger detail a lookup returns.
        fs::remove_file(&log).unwrap();
        assert_eq!(
            control.execute(&live).await.phase,
            CommandPhase::Completed as i32
        );
        assert!(!log.exists());
        let looked = control
            .lookup(&live.request_id, ProjectId::Bsc as i32)
            .await;
        assert_eq!(
            (looked.phase, looked.detail.as_str()),
            (CommandPhase::Completed as i32, r.detail.as_str())
        );
        // A stale generation is the owner's refusal: a failed receipt with its line.
        let stale = request(
            "42345678-1234-1234-1234-123456789abc",
            "BSC_INGRESS_PUBLICNODE:SHADOW",
            config(3),
        );
        let r = control.execute(&stale).await;
        assert_eq!(r.phase, CommandPhase::Failed as i32);
        assert!(
            r.detail.starts_with("outcome=REJECTED_STALE"),
            "{}",
            r.detail
        );
        // An invalid request never reaches the ledger or the controller.
        fs::remove_file(&log).unwrap();
        let bad = request(
            "52345678-1234-1234-1234-123456789abc",
            "BSC_INGRESS_PUBLICNODE:ARMED",
            config(4),
        );
        assert_eq!(
            control.execute(&bad).await.reject_reason,
            CommandRejectReason::InvalidParams as i32
        );
        assert!(!log.exists());
        fs::remove_dir_all(dir).unwrap();
    }
    /// DEC-18 (package-4 review F1, F3, F5, F9): the gateway's provider rules are the native
    /// descriptor's — a key in an endpoint's port or behind a backslash, an identifier that reads as
    /// an option, a secret with a control byte are refused before any record — and each of the
    /// twelve provider actions maps onto exactly its `p0ctl provider` arguments; none is a BSC one.
    #[test]
    fn dec18_provider_rules_are_the_native_ones_and_every_action_maps() {
        use crate::proto::expected_version::Version::ConfigVersion;
        use crate::proto::project0::v1::{
            ProviderCredentialCommand, ProviderDescriptorCommand, ProviderPriorityCommand,
            ProviderQualifyCommand, ProviderSelectCommand,
        };
        use provider_shape::{endpoint_ok, entitlement_ok, id_ok};
        // F1, L2: refused here as natively or more strictly (the native URL parser reads `\`
        // as `/`, a port as at most 65535, an IPv6 host as an address, percent-encodes `{` in a
        // path; a host has at most 253 characters) — nothing it refuses after the ledger passes.
        let long_host = format!("https://{}.example", "a".repeat(250));
        for bad in [
            "https://rpc.example\\LEAKEDKEY1a2b3c4d5e6f7g8h9",
            "https://rpc.example:0123456789abcdef0123",
            "https://rpc.example:",
            "https://rpc.example:65536",
            "https://[::1",
            "https://[::1]x",
            "https://[0123456789abcdef0123456789abcdef]",
            "https://rp<c.example",
            "https://rpc.example/abcdefghijklmno{",
            "https://rpc.example/a|b",
            &long_host,
        ] {
            assert!(!endpoint_ok(bad), "{bad}");
        }
        for good in [
            "https://mainnet.helius-rpc.com",
            "wss://mainnet.helius-rpc.com",
            "http://127.0.0.1:8899",
            "https://[::1]:8899",
            "https://x.example:443/v1/rpc",
        ] {
            assert!(endpoint_ok(good), "{good}");
        }
        // L1: a credential command is read from its request without copying the secret — it
        // stays a view of the request body (the one copy is the wiped `SecretBuf`).
        let body = Bytes::from(
            CommandRequest {
                project: ProjectId::Project0 as i32,
                request_id: "12345678-1234-1234-1234-123456789abc".into(),
                action: PROVIDER_CREDENTIAL_SET.into(),
                params: ProviderCredentialCommand {
                    credential: "helius".into(),
                    secret: b"VIEW-NOT-COPY".to_vec().into(),
                }
                .encode_to_vec()
                .into(),
                ..Default::default()
            }
            .encode_to_vec(),
        );
        let request = CommandRequest::decode(body.clone()).unwrap();
        let decoded = credential_command(&request).unwrap();
        assert_eq!(&decoded.secret[..], b"VIEW-NOT-COPY");
        assert!(
            body.as_ptr_range().contains(&decoded.secret.as_ptr()),
            "a view, not a copy"
        );
        // F9: an identifier or an entitlement never reads as an option of the controller.
        for bad in ["--project", "-x", "-"] {
            assert!(!id_ok(bad) && !entitlement_ok(bad), "{bad}");
        }
        let req = |action: &str, resource: &str, params: Vec<u8>| CommandRequest {
            project: ProjectId::Project0 as i32,
            request_id: "12345678-1234-1234-1234-123456789abc".into(),
            action: action.into(),
            resource_id: resource.into(),
            expected: Some(crate::proto::ExpectedVersion {
                version: Some(ConfigVersion(7)),
            }),
            params: params.into(),
            issued_at_unix_ns: 0,
        };
        assert_eq!(job_args(&req(PROVIDER_REMOVE, "--project", vec![])), None);
        // F3: a secret is one line of visible bytes (the controller reads up to the line end).
        let credential = |secret: &[u8]| {
            ProviderCredentialCommand {
                credential: "helius".into(),
                secret: secret.to_vec().into(),
            }
            .encode_to_vec()
        };
        for bad in [&b"AAAA\nBBBB"[..], b"AAAA\rB", b"A\0B", b"A\x7fB"] {
            let r = req(PROVIDER_CREDENTIAL_SET, "", credential(bad));
            assert!(separate_secret(&r).is_none(), "{bad:?}");
        }
        let r = req(PROVIDER_CREDENTIAL_ROTATE, "", credential(b"KEY-2"));
        assert!(separate_secret(&r).is_some_and(|(_, secret)| secret.is_some()));
        // F5: every action's arguments, the request id and the registry version last.
        let descriptor = ProviderDescriptorCommand {
            id: "rpc-a".into(),
            kind: Some("rpc".into()),
            endpoint: Some("https://rpc.example".into()),
            auth: Some("header:x-api-key".into()),
            credential: Some("cred-a".into()),
            priority: Some(2),
            capacity_rps: Some(5),
            plan: Some("free".into()),
            entitlement: Some("free-tier".into()),
            disabled: true,
        };
        let update = ProviderDescriptorCommand {
            id: "rpc-a".into(),
            endpoint: Some("https://rpc2.example".into()),
            ..Default::default()
        };
        let priority = ProviderPriorityCommand {
            id: "rpc-a".into(),
            priority: 3,
        };
        let select = ProviderSelectCommand {
            kind: "wss".into(),
            id: "wss-a".into(),
        };
        let qualify = ProviderQualifyCommand {
            id: "rpc-a".into(),
            evidence: "0123456789abcdef".into(),
        };
        let cases: [(&str, &str, Vec<u8>, &[&str]); 12] = [
            (
                PROVIDER_ADD,
                "",
                descriptor.encode_to_vec(),
                &[
                    "add",
                    "--id",
                    "rpc-a",
                    "--kind",
                    "rpc",
                    "--endpoint",
                    "https://rpc.example",
                    "--auth",
                    "header:x-api-key",
                    "--credential",
                    "cred-a",
                    "--plan",
                    "free",
                    "--entitlement",
                    "free-tier",
                    "--priority",
                    "2",
                    "--capacity-rps",
                    "5",
                    "--disabled",
                ],
            ),
            (
                PROVIDER_UPDATE,
                "",
                update.encode_to_vec(),
                &[
                    "update",
                    "--id",
                    "rpc-a",
                    "--endpoint",
                    "https://rpc2.example",
                ],
            ),
            (
                PROVIDER_REMOVE,
                "rpc-a",
                vec![],
                &["remove", "--id", "rpc-a"],
            ),
            (
                PROVIDER_ENABLE,
                "rpc-a",
                vec![],
                &["enable", "--id", "rpc-a"],
            ),
            (
                PROVIDER_DISABLE,
                "rpc-a",
                vec![],
                &["disable", "--id", "rpc-a"],
            ),
            (
                PROVIDER_PRIORITY,
                "",
                priority.encode_to_vec(),
                &["priority", "--id", "rpc-a", "--priority", "3"],
            ),
            (
                PROVIDER_SELECT,
                "",
                select.encode_to_vec(),
                &["select", "--kind", "wss", "--id", "wss-a"],
            ),
            (
                PROVIDER_UNSELECT,
                "jito",
                vec![],
                &["unselect", "--kind", "jito"],
            ),
            (
                PROVIDER_QUALIFY,
                "",
                qualify.encode_to_vec(),
                &["qualify", "--id", "rpc-a", "--evidence", "0123456789abcdef"],
            ),
            (
                PROVIDER_CREDENTIAL_SET,
                "",
                credential(b"KEY-1"),
                &["credential", "set", "--credential", "helius"],
            ),
            (
                PROVIDER_CREDENTIAL_ROTATE,
                "",
                credential(b"KEY-2"),
                &["credential", "rotate", "--credential", "helius"],
            ),
            (
                PROVIDER_CREDENTIAL_REVOKE,
                "helius",
                vec![],
                &["credential", "revoke", "--credential", "helius"],
            ),
        ];
        let tail = [
            "--request-id",
            "12345678-1234-1234-1234-123456789abc",
            "--expect-version",
            "7",
        ];
        for (action, resource, params, args) in cases {
            let expected: Vec<String> = ["p0", "provider"]
                .iter()
                .chain(args)
                .chain(&tail)
                .map(|s| s.to_string())
                .collect();
            assert_eq!(
                job_args(&req(action, resource, params)),
                Some(expected),
                "{action}"
            );
            assert!(allowed(ProjectId::Project0 as i32, action), "{action}");
            assert!(!allowed(ProjectId::Bsc as i32, action), "{action}");
        }
    }

    #[tokio::test]
    async fn dec18_provider_commands_map_onto_p0ctl_and_no_secret_is_recorded() {
        // DEC-18 (P0-019/P0-020): every registry change through the one controller with its CAS
        // and request id; a key in an endpoint or a name is refused before the ledger; a
        // credential's secret reaches only the controller's standard input — never the ledger,
        // an argument or the receipt; a replay never runs the controller again.
        use crate::proto::project0::v1::{
            ProviderCredentialCommand, ProviderDescriptorCommand, ProviderSelectCommand,
        };
        use crate::proto::{expected_version, ExpectedVersion};
        let dir = std::env::temp_dir().join(format!("gw-providers-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("run")).unwrap();
        let ctl = dir.join("darkbotctl");
        let (log, input, runs) = (dir.join("argv"), dir.join("stdin"), dir.join("runs"));
        executable(
            &ctl,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{log}'\necho run >> '{runs}'\ncat > '{input}'\nif [ \"$7\" = refused ]; then echo 'PROVIDER_RECEIPT request_id=x applied=false version=4 action=\"select rpc refused\" detail=\"PROVIDER_UNKNOWN: refused\"'; exit 2; fi\necho 'PROVIDER_RECEIPT request_id=x applied=true version=5 action=\"credential set helius\" detail=\"stored\"'\n",
                log = log.display(),
                runs = runs.display(),
                input = input.display()
            ),
        );
        let control = Control::new(
            Some(Paths {
                ctl,
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "com.darkbot.test.absent.capture".into(),
                launchctl: absent_launchctl(&dir),
            }),
            &Limits::default(),
        );
        let request =
            |n: u8, action: &str, resource: &str, params: Vec<u8>, version: Option<u64>| {
                CommandRequest {
                    project: ProjectId::Project0 as i32,
                    request_id: format!("{n}2345678-1234-1234-1234-123456789abc"),
                    action: action.into(),
                    resource_id: resource.into(),
                    expected: version.map(|v| ExpectedVersion {
                        version: Some(expected_version::Version::ConfigVersion(v)),
                    }),
                    params: params.into(),
                    issued_at_unix_ns: crate::overview::unix_ns_now(),
                }
            };
        let argv = || fs::read_to_string(&log).unwrap_or_default();
        let ledger = || {
            let mut all = Vec::new();
            for e in fs::read_dir(dir.join("run/commands")).unwrap() {
                let path = e.unwrap().path();
                if path.is_file() {
                    all.extend(fs::read(path).unwrap());
                }
            }
            all
        };
        let holds = |bytes: &[u8], needle: &str| {
            bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
        };
        let descriptor = |endpoint: &str, credential: &str| ProviderDescriptorCommand {
            id: "helius-rpc".into(),
            kind: Some("rpc".into()),
            endpoint: Some(endpoint.into()),
            auth: Some("query:api-key".into()),
            credential: Some(credential.into()),
            priority: Some(0),
            capacity_rps: Some(10),
            plan: Some("free".into()),
            entitlement: Some("helius-free".into()),
            disabled: false,
        };
        let add = descriptor("https://mainnet.helius-rpc.com", "helius").encode_to_vec();
        let r = control
            .execute(&request(1, PROVIDER_ADD, "", add.clone(), Some(4)))
            .await;
        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
        assert_eq!(
            argv(),
            "p0\nprovider\nadd\n--id\nhelius-rpc\n--kind\nrpc\n--endpoint\nhttps://mainnet.helius-rpc.com\n--auth\nquery:api-key\n--credential\nhelius\n--plan\nfree\n--entitlement\nhelius-free\n--priority\n0\n--capacity-rps\n10\n--request-id\n12345678-1234-1234-1234-123456789abc\n--expect-version\n4\n"
        );
        assert!(r.detail.contains("PROVIDER_RECEIPT"), "{}", r.detail);
        // The registry version is required (CAS).
        let r = control
            .execute(&request(2, PROVIDER_ADD, "", add, None))
            .await;
        assert_eq!(r.reject_reason, CommandRejectReason::InvalidParams as i32);
        // A key in an endpoint or typed into a name is refused before any record.
        for (n, params) in [
            (
                3,
                descriptor(
                    "https://mainnet.helius-rpc.com/?api-key=LEAKED-KEY-1",
                    "helius",
                ),
            ),
            (
                4,
                descriptor("https://user:LEAKED-KEY-1@rpc.example", "helius"),
            ),
            (
                5,
                descriptor("https://rpc.example/LEAKEDKEY1a2b3c4d5e6f7g8h9", "helius"),
            ),
            (
                6,
                descriptor(
                    "https://mainnet.helius-rpc.com",
                    "0123456789abcdef0123456789abcdef",
                ),
            ),
        ] {
            let r = control
                .execute(&request(
                    n,
                    PROVIDER_ADD,
                    "",
                    params.encode_to_vec(),
                    Some(4),
                ))
                .await;
            assert_eq!(
                r.reject_reason,
                CommandRejectReason::InvalidParams as i32,
                "{n}"
            );
            assert!(!r.detail.contains("LEAKED"), "{}", r.detail);
        }
        let l = ledger();
        assert!(!holds(&l, "LEAKED") && !holds(&l, "0123456789abcdef0123"));
        // A credential's secret: the controller's standard input only.
        let set = ProviderCredentialCommand {
            credential: "helius".into(),
            secret: b"SECRET-KEY-1".to_vec().into(),
        }
        .encode_to_vec();
        let set_request = request(7, PROVIDER_CREDENTIAL_SET, "", set, Some(4));
        let r = control.execute(&set_request).await;
        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
        assert_eq!(fs::read_to_string(&input).unwrap(), "SECRET-KEY-1\n");
        assert!(argv().contains("credential\nset\n--credential\nhelius\n"));
        assert!(!argv().contains("SECRET") && !r.detail.contains("SECRET"));
        assert!(
            !holds(&ledger(), "SECRET-KEY-1"),
            "the ledger holds no secret"
        );
        // The same request again is the stored receipt; the controller is not run again.
        let before = fs::read_to_string(&runs).unwrap().lines().count();
        let again = control.execute(&set_request).await;
        assert_eq!(again.phase, CommandPhase::Completed as i32);
        assert!(again.detail.starts_with("Provider change intent persisted"));
        assert_eq!(fs::read_to_string(&runs).unwrap().lines().count(), before);
        // A rotation: its arguments, its secret on the standard input only; afterwards the
        // gateway keeps no view of the request body, so the handler can overwrite it (F2).
        let rotate = ProviderCredentialCommand {
            credential: "helius".into(),
            secret: b"SECRET-KEY-2".to_vec().into(),
        };
        let body = Bytes::from(
            request(
                4,
                PROVIDER_CREDENTIAL_ROTATE,
                "",
                rotate.encode_to_vec(),
                Some(5),
            )
            .encode_to_vec(),
        );
        let decoded = CommandRequest::decode(body.clone()).unwrap();
        let r = control.execute(&decoded).await;
        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
        assert!(argv().starts_with("p0\nprovider\ncredential\nrotate\n--credential\nhelius\n"));
        assert_eq!(fs::read_to_string(&input).unwrap(), "SECRET-KEY-2\n");
        assert!(
            !holds(&ledger(), "SECRET-KEY-2"),
            "the ledger holds no secret"
        );
        drop(decoded);
        assert!(body.try_into_mut().is_ok(), "no view of the body remains");
        // A credential action without its secret is refused.
        let empty = ProviderCredentialCommand {
            credential: "helius".into(),
            secret: Default::default(),
        }
        .encode_to_vec();
        let r = control
            .execute(&request(8, PROVIDER_CREDENTIAL_ROTATE, "", empty, Some(4)))
            .await;
        assert_eq!(r.reject_reason, CommandRejectReason::InvalidParams as i32);
        // A refused change is FAILED with the registry's receipt.
        let refused = ProviderSelectCommand {
            kind: "rpc".into(),
            id: "refused".into(),
        }
        .encode_to_vec();
        let r = control
            .execute(&request(9, PROVIDER_SELECT, "", refused, Some(4)))
            .await;
        assert_eq!(r.phase, CommandPhase::Failed as i32, "{}", r.detail);
        assert!(
            r.detail.contains("PROVIDER_UNKNOWN: refused"),
            "{}",
            r.detail
        );
        // Remove, revoke and unselect name their resource; a kind is one of the five.
        let r = control
            .execute(&request(
                0,
                PROVIDER_CREDENTIAL_REVOKE,
                "helius",
                vec![],
                Some(5),
            ))
            .await;
        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
        assert!(argv().starts_with("p0\nprovider\ncredential\nrevoke\n--credential\nhelius\n"));
        let r = control
            .execute(&request(6, PROVIDER_UNSELECT, "ftp", vec![], Some(5)))
            .await;
        assert_eq!(r.reject_reason, CommandRejectReason::InvalidParams as i32);
        let _ = fs::remove_dir_all(&dir);
    }

    /// B03/U-9: an analytics job action maps its typed parameters 1:1 onto the options of
    /// `p0ctl job` (through `darkbotctl p0 job`), without observing a capture process; a native
    /// refusal is a FAILED receipt with its code; invalid parameters are rejected before the
    /// ledger and never dispatched.
    #[tokio::test]
    async fn analytics_job_commands_dispatch_the_p0ctl_job_options() {
        use crate::proto::project0::v1::{BacktestJobCommand, FundingSet, OptimizeJobCommand};
        let dir = std::env::temp_dir().join(format!("gw-jobs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("run")).unwrap();
        let ctl = dir.join("darkbotctl");
        let log = dir.join("argv");
        executable(
            &ctl,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n[ \"$3\" != cancel ] || {{ echo 'p0ctl: JOB_NOT_RUNNING: COMPLETED' >&2; exit 2; }}\n",
                log.display()
            ),
        );
        let control = Control::new(
            Some(Paths {
                ctl,
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "com.darkbot.test.absent.capture".into(),
                launchctl: absent_launchctl(&dir),
            }),
            &Limits::default(),
        );
        let point = BacktestJobCommand {
            run_id: "run-a".into(),
            min_profit_usd: "1.5".into(),
            capital_usd: vec![100, 1000],
            funding: vec![],
            max_seconds: 60,
            max_ram_bytes: 1_048_576,
            verification_result: String::new(),
        };
        let request = |n: u8, action: &str, resource: &str, params: Vec<u8>| CommandRequest {
            project: ProjectId::Project0 as i32,
            request_id: format!("{n}2345678-1234-1234-1234-123456789abc"),
            action: action.into(),
            resource_id: resource.into(),
            expected: None,
            params: params.into(),
            issued_at_unix_ns: crate::overview::unix_ns_now(),
        };
        let argv = || fs::read_to_string(&log).unwrap();
        let bt = request(1, BACKTEST_START, "", point.encode_to_vec());
        let r = control.execute(&bt).await;
        assert_eq!(
            (r.status, r.phase),
            (
                CommandStatus::Accepted as i32,
                CommandPhase::Completed as i32
            ),
            "{}",
            r.detail
        );
        assert_eq!(
            argv(),
            "p0\njob\nstart\nbacktest\n--id\n12345678-1234-1234-1234-123456789abc\n--run\nrun-a\n--min-profit-usd\n1.5\n--capital-usd\n100,1000\n--funding\nall\n--max-seconds\n60\n--max-ram-bytes\n1048576\n"
        );
        let opt = OptimizeJobCommand {
            baseline: Some(point.clone()),
            min_profit_usd_grid: vec!["1.5".into(), "2".into()],
            funding_sets: vec![
                FundingSet { classes: vec![] },
                FundingSet {
                    classes: vec!["NO_SWAP_INVENTORY".into(), "PREHELD_INVENTORY".into()],
                },
            ],
            holdout_bps: 3000,
            max_points: 4,
            save_profile: true,
        };
        let r = control
            .execute(&request(2, OPTIMIZE_START, "", opt.encode_to_vec()))
            .await;
        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
        assert!(argv().ends_with(
            "--min-profit-usd-grid\n1.5,2\n--funding-sets\nall;NO_SWAP_INVENTORY,PREHELD_INVENTORY\n--holdout-bps\n3000\n--max-points\n4\n--save-profile\n"
        ));
        // H0: every saved run; the window days only when given, the budget always.
        use crate::proto::project0::v1::H0JobCommand;
        let h0 = |window_days, max_seconds| H0JobCommand {
            window_days,
            max_seconds,
            max_ram_bytes: 1_048_576,
        };
        let h0_request = |id: char, resource: &str, c: H0JobCommand| CommandRequest {
            request_id: format!("{id}2345678-1234-1234-1234-123456789abc"),
            ..request(1, H0_START, resource, c.encode_to_vec())
        };
        let r = control.execute(&h0_request('c', "", h0(90, 60))).await;
        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
        assert_eq!(
            argv(),
            "p0\njob\nstart\nh0\n--id\nc2345678-1234-1234-1234-123456789abc\n--window-days\n90\n--max-seconds\n60\n--max-ram-bytes\n1048576\n"
        );
        let r = control.execute(&h0_request('d', "", h0(0, 60))).await;
        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
        assert!(!argv().contains("--window-days"), "the registry's window");
        for refused in [
            h0_request('e', "", h0(30, 0)),
            h0_request('f', "run-a", h0(30, 60)),
        ] {
            let r = control.execute(&refused).await;
            assert_eq!(r.status, CommandStatus::Rejected as i32, "{}", r.detail);
        }
        let cancel = request(
            3,
            JOB_CANCEL,
            "12345678-1234-1234-1234-123456789abc",
            vec![],
        );
        let r = control.execute(&cancel).await;
        assert_eq!(
            (r.status, r.phase),
            (CommandStatus::Accepted as i32, CommandPhase::Failed as i32)
        );
        assert_eq!(
            r.detail,
            "Native controller refused: JOB_NOT_RUNNING: COMPLETED"
        );
        assert_eq!(
            control
                .lookup(&cancel.request_id, ProjectId::Project0 as i32)
                .await
                .detail,
            r.detail
        );
        assert_eq!(
            argv(),
            "p0\njob\ncancel\n--id\n12345678-1234-1234-1234-123456789abc\n"
        );
        // A08/F-02, DEC-08: verification off, or index|strict|crosscheck from the checkpoint; an
        // online history read is gate-blocked.
        use crate::proto::project0::v1::PostCaptureJobCommand;
        let verify = PostCaptureJobCommand {
            run_id: "run-a".into(),
            chain_verify: "strict".into(),
            offline: true,
            resume_verification: true,
            max_seconds: 60,
            max_ram_bytes: 1_048_576,
        };
        let r = control
            .execute(&request(5, VERIFY_START, "", verify.encode_to_vec()))
            .await;
        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
        assert_eq!(
            argv(),
            "p0\njob\nstart\npost-capture\n--id\n52345678-1234-1234-1234-123456789abc\n--run\nrun-a\n--chain-verify\nstrict\n--max-seconds\n60\n--max-ram-bytes\n1048576\n--offline\n--resume-verification\n"
        );
        let online = PostCaptureJobCommand {
            offline: false,
            ..verify.clone()
        };
        let r = control
            .execute(&request(6, VERIFY_START, "", online.encode_to_vec()))
            .await;
        assert_eq!(
            (r.status, r.reject_reason),
            (
                CommandStatus::Rejected as i32,
                CommandRejectReason::GateBlocked as i32
            )
        );
        // DEC-08: index and crosscheck from their checkpoints run as strict does; online each is
        // gate-blocked.
        for (id, mode) in [("a", "index"), ("b", "crosscheck")] {
            let cached = PostCaptureJobCommand {
                chain_verify: mode.into(),
                ..verify.clone()
            };
            let mut req = request(5, VERIFY_START, "", cached.encode_to_vec());
            req.request_id = format!("{id}2345678-1234-1234-1234-123456789abc");
            let r = control.execute(&req).await;
            assert_eq!(
                r.phase,
                CommandPhase::Completed as i32,
                "{mode}: {}",
                r.detail
            );
            assert_eq!(
                argv(),
                format!("p0\njob\nstart\npost-capture\n--id\n{id}2345678-1234-1234-1234-123456789abc\n--run\nrun-a\n--chain-verify\n{mode}\n--max-seconds\n60\n--max-ram-bytes\n1048576\n--offline\n--resume-verification\n")
            );
            let mut req = request(
                6,
                VERIFY_START,
                "",
                PostCaptureJobCommand {
                    offline: false,
                    ..cached
                }
                .encode_to_vec(),
            );
            req.request_id = format!("{id}2345678-1234-1234-1234-000000000006");
            let r = control.execute(&req).await;
            assert_eq!(
                (r.status, r.reject_reason),
                (
                    CommandStatus::Rejected as i32,
                    CommandRejectReason::GateBlocked as i32
                ),
                "{mode}"
            );
        }
        // S1-02: a source mode request maps onto `p0ctl source set`.
        use crate::proto::project0::v1::SourceControlCommand;
        let source = SourceControlCommand {
            unit: "geyser:publicnode".into(),
            mode: "observe".into(),
        };
        let mut set = request(7, SOURCE_SET, "", source.encode_to_vec());
        set.expected = Some(crate::proto::ExpectedVersion {
            version: Some(expected_version::Version::ConfigVersion(0)),
        });
        let r = control.execute(&set).await;
        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
        assert_eq!(
            argv(),
            "p0\nsource\nset\ngeyser:publicnode\nobserve\n--expect-version\n0\n"
        );
        // Invalid parameters: rejected, no receipt, nothing dispatched.
        fs::remove_file(&log).unwrap();
        let bad = |c: BacktestJobCommand| c.encode_to_vec();
        for (n, req) in [
            (
                4,
                request(
                    4,
                    BACKTEST_START,
                    "",
                    bad(BacktestJobCommand {
                        run_id: "../x".into(),
                        ..point.clone()
                    }),
                ),
            ),
            (
                5,
                request(
                    5,
                    BACKTEST_START,
                    "",
                    bad(BacktestJobCommand {
                        min_profit_usd: "1e5".into(),
                        ..point.clone()
                    }),
                ),
            ),
            (
                6,
                request(
                    6,
                    BACKTEST_START,
                    "",
                    bad(BacktestJobCommand {
                        capital_usd: vec![0],
                        ..point.clone()
                    }),
                ),
            ),
            (
                7,
                request(
                    7,
                    BACKTEST_START,
                    "",
                    bad(BacktestJobCommand {
                        funding: vec!["all; rm".into()],
                        ..point.clone()
                    }),
                ),
            ),
            (
                8,
                request(
                    8,
                    BACKTEST_START,
                    "",
                    bad(BacktestJobCommand {
                        max_seconds: 0,
                        ..point.clone()
                    }),
                ),
            ),
            (
                9,
                request(9, BACKTEST_START, "run-a", point.encode_to_vec()),
            ),
            (1, request(1, JOB_CANCEL, "../job", vec![])),
            (2, request(2, JOB_CANCEL, "job-1", vec![1])),
            (
                3,
                request(
                    3,
                    OPTIMIZE_START,
                    "",
                    OptimizeJobCommand {
                        baseline: None,
                        ..opt.clone()
                    }
                    .encode_to_vec(),
                ),
            ),
            (4, request(4, BACKTEST_START, "", vec![0xFF, 0xFF])),
            (
                5,
                request(
                    5,
                    VERIFY_START,
                    "",
                    PostCaptureJobCommand {
                        chain_verify: "full".into(),
                        ..verify.clone()
                    }
                    .encode_to_vec(),
                ),
            ),
            (
                6,
                request(
                    6,
                    VERIFY_START,
                    "",
                    PostCaptureJobCommand {
                        chain_verify: "off".into(),
                        offline: true,
                        ..verify.clone()
                    }
                    .encode_to_vec(),
                ),
            ),
            (
                7,
                request(
                    7,
                    VERIFY_START,
                    "",
                    PostCaptureJobCommand {
                        run_id: "/abs".into(),
                        ..verify.clone()
                    }
                    .encode_to_vec(),
                ),
            ),
            (
                8,
                request(
                    8,
                    BACKTEST_START,
                    "",
                    bad(BacktestJobCommand {
                        run_id: "--project".into(),
                        ..point.clone()
                    }),
                ),
            ),
            (9, request(9, JOB_CANCEL, "-job", vec![])),
            (
                1,
                request(
                    1,
                    SOURCE_SET,
                    "",
                    SourceControlCommand {
                        unit: "geyser:../x".into(),
                        mode: "observe".into(),
                    }
                    .encode_to_vec(),
                ),
            ),
            (
                2,
                request(
                    2,
                    SOURCE_SET,
                    "",
                    SourceControlCommand {
                        unit: "wss".into(),
                        mode: "on".into(),
                    }
                    .encode_to_vec(),
                ),
            ),
        ] {
            // Fresh ids: none of these may reuse a receipt of the dispatched commands above.
            let mut req = req;
            req.request_id = format!("{n}2345678-1234-1234-1234-000000000000");
            let r = control.execute(&req).await;
            assert_eq!(
                (r.status, r.reject_reason),
                (
                    CommandStatus::Rejected as i32,
                    CommandRejectReason::InvalidParams as i32
                ),
                "{}",
                req.action
            );
            assert!(
                !control
                    .lookup(&req.request_id, ProjectId::Project0 as i32)
                    .await
                    .found
            );
        }
        assert!(!log.exists(), "nothing dispatched");
        fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn the_bsc_job_state_is_its_native_owners() {
        // Audit E 4.13: the reporter derives the job's state (its tests); the gateway shows its
        // typed answer as it is and passes it the run directory, the snapshot and the label.
        let dir = std::env::temp_dir().join(format!("gw-blocked-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("run")).unwrap();
        let job = crate::proto::bsc::v1::JobStatus {
            job: "readiness".into(),
            state: "RUNNING".into(),
            launch_id: "job-1".into(),
            started_unix_s: 1,
            ended_unix_s: 0,
            result_code: String::new(),
            controller_available: Some(true),
            pid: Some(7),
        };
        fs::write(dir.join("job.bin"), job.encode_to_vec()).unwrap();
        let control = Control::new(
            Some(Paths {
                ctl: dir.join("darkbotctl"),
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "com.darkbot.test.absent.capture".into(),
                launchctl: absent_launchctl(&dir),
            }),
            &Limits::default(),
        );
        // No reporter deployed: unknown and uncontrollable.
        let s = control.observe_project(ProjectId::Bsc as i32).await;
        assert_eq!(
            (s.state.as_str(), s.controller_available),
            ("UNKNOWN", false)
        );
        executable(
            &dir.join("bsc-status-reporter"),
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" \"$DARKBOT_RUNDIR\" \"$DARKBOT_BSC_ROOT\" > '{}'\ncat '{}'\n",
                dir.join("args.txt").display(),
                dir.join("job.bin").display()
            ),
        );
        let s = control.observe_project(ProjectId::Bsc as i32).await;
        assert_eq!(
            (
                s.state.as_str(),
                s.run_id.as_str(),
                s.pid,
                s.controller_available
            ),
            ("RUNNING", "job-1", Some(7), true)
        );
        assert_eq!(
            s.reason,
            "launchd: com.darkbot.test.absent.bsc.readiness; result "
        );
        assert_eq!(
            fs::read_to_string(dir.join("args.txt")).unwrap(),
            format!(
                "--readiness-status\ncom.darkbot.test.absent.bsc.readiness\n{}\n{}\n",
                dir.join("run").display(),
                dir.join("bsc-root").display()
            )
        );
        fs::remove_dir_all(dir).unwrap();
    }
    /// Audit E 4.13 review M1: while the job's owner gives no answer, a BSC stop is refused as
    /// unknown (never as a launch change) and a pending stop stays pending (never closed as
    /// replaced); with its answer for the launch the stop closes.
    #[tokio::test]
    async fn an_unanswered_bsc_job_state_decides_nothing() {
        let env = Env::new("bsc-unanswered");
        let control = env.restart();
        let launch = uuid(5);
        let answer = |state: &str| {
            let job = crate::proto::bsc::v1::JobStatus {
                job: "readiness".into(),
                state: state.into(),
                launch_id: launch.clone(),
                controller_available: Some(true),
                ..Default::default()
            };
            fs::write(env.dir.join("job.bin"), job.encode_to_vec()).unwrap();
        };
        let stop = |n: usize| CommandRequest {
            project: ProjectId::Bsc as i32,
            request_id: uuid(n),
            action: BSC_STOP.into(),
            resource_id: launch.clone(),
            expected: None,
            params: Default::default(),
            issued_at_unix_ns: now(),
        };
        let reporter = env.dir.join("bsc-status-reporter");
        fs::remove_file(&reporter).unwrap();
        let refused = control.execute(&stop(1)).await;
        assert_eq!(
            (refused.status, refused.reject_reason),
            (
                CommandStatus::Rejected as i32,
                CommandRejectReason::GateBlocked as i32
            ),
            "{}",
            refused.detail
        );
        assert_eq!(env.executions(), 0);
        idle_reporter(&env.dir);
        answer("RUNNING");
        let pending = control.execute(&stop(2)).await;
        assert_eq!(
            pending.phase,
            CommandPhase::Pending as i32,
            "{}",
            pending.detail
        );
        fs::remove_file(&reporter).unwrap();
        let kept = control.lookup(&uuid(2), ProjectId::Bsc as i32).await;
        assert_eq!(kept.phase, CommandPhase::Pending as i32, "{}", kept.detail);
        idle_reporter(&env.dir);
        answer("STOPPED");
        let closed = control.lookup(&uuid(2), ProjectId::Bsc as i32).await;
        assert_eq!(
            closed.phase,
            CommandPhase::Completed as i32,
            "{}",
            closed.detail
        );
    }
    /// Audit E 3.7: the launchd query and the controller dispatch wait exactly the configured
    /// `Limits`, no literal: a launchd stand-in slower than its bound reads UNKNOWN (never its
    /// late answer), a controller slower than its bound leaves the command PENDING_RECONCILE
    /// with an unknown outcome.
    #[tokio::test]
    async fn the_launchd_and_controller_waits_are_the_configured_limits() {
        let dir = std::env::temp_dir().join(format!("gw-bounds-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("run")).unwrap();
        let launchctl = dir.join("launchctl");
        executable(&launchctl, "#!/bin/sh\nsleep 1\necho '\"PID\" = 7;'\n");
        let ctl = dir.join("darkbotctl");
        executable(&ctl, "#!/bin/sh\nsleep 1\necho outcome=APPLIED\n");
        let control = Control::new(
            Some(Paths {
                ctl,
                rundir: dir.join("run"),
                logdir: dir.clone(),
                label: "test.capture".into(),
                launchctl,
            }),
            &Limits {
                launchd_query_timeout_ms: 100,
                controller_timeout_ms: 100,
                ..limits()
            },
        );
        let started = std::time::Instant::now();
        let s = control.observe().await;
        assert_eq!(
            (s.state.as_str(), s.reason.as_str(), s.pid),
            ("UNKNOWN", "launchd query unavailable", None)
        );
        assert!(started.elapsed() < Duration::from_millis(900));
        let req = CommandRequest {
            project: ProjectId::Bsc as i32,
            request_id: uuid(37),
            action: BSC_SOURCE.into(),
            resource_id: "BSC_INGRESS_DRPC:SHADOW".into(),
            expected: Some(crate::proto::ExpectedVersion {
                version: Some(expected_version::Version::ConfigVersion(4)),
            }),
            params: Default::default(),
            issued_at_unix_ns: crate::overview::unix_ns_now(),
        };
        let started = std::time::Instant::now();
        let r = control.execute(&req).await;
        assert!(started.elapsed() < Duration::from_millis(900));
        assert_eq!(
            (r.status, r.phase, r.detail.as_str()),
            (
                CommandStatus::Accepted as i32,
                CommandPhase::PendingReconcile as i32,
                "Controller outcome unknown; the native source state decides"
            )
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn rejects_path_ids_and_redacts_provider_credentials() {
        assert!(valid_id("12345678-1234-1234-1234-123456789abc"));
        assert!(!valid_id("../../capture"));
        assert!(!redact("error wss://provider/path-secret").contains("path-secret"));
        assert!(!redact("api-key=TOPSECRET").contains("TOPSECRET"));
    }

    /// DEC-18 (P0-020 nit): a command refused before it runs (401, 403, 415) overwrites its body
    /// as well — the header values that are views of the same read buffer are released first.
    #[tokio::test]
    async fn a_refused_command_body_is_overwritten_too() {
        use axum::extract::State;
        use axum::http::{HeaderName, HeaderValue};
        let addr: std::net::SocketAddr = "127.0.0.1:7".parse().unwrap();
        let origin = format!("http://{addr}");
        let app = crate::http::AppState::new(
            crate::config::GatewayConfig {
                bind: addr,
                static_dir: None,
                allowed_origins: vec![origin.clone()],
                allowed_hosts: vec![addr.to_string()],
                session_cookie: "darkbot_session".into(),
                limits: Limits::default(),
                source_stale_after_ms: 1000,
                source_detach_after_ms: 2000,
                source_reattach_backoff_ms: 50,
                overview_interval_ms: 60000,
                feeds: vec![],
            },
            Arc::new(crate::hub::StateHub::new(16)),
            crate::engine::SourceRegistry::new(),
        );
        let token = app.sessions.issue();
        let csrf = app.sessions.csrf_of(&token).unwrap();
        let cookie = format!("darkbot_session={token}");
        for (case, with_cookie, with_csrf, content_type, status) in [
            (
                "no session",
                false,
                false,
                "application/x-protobuf",
                StatusCode::UNAUTHORIZED,
            ),
            (
                "no csrf",
                true,
                false,
                "application/x-protobuf",
                StatusCode::FORBIDDEN,
            ),
            (
                "not protobuf",
                true,
                true,
                "application/json",
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ),
        ] {
            // One read buffer as hyper keeps it: the header values are views of the body's bytes.
            let read = Bytes::from(format!("{origin}{content_type}SECRET-BODY").into_bytes());
            let mut h = HeaderMap::new();
            let shared = |at: usize, len: usize| {
                HeaderValue::from_maybe_shared(read.slice(at..at + len)).unwrap()
            };
            h.insert(HeaderName::from_static("origin"), shared(0, origin.len()));
            h.insert(CONTENT_TYPE, shared(origin.len(), content_type.len()));
            if with_cookie {
                h.insert("cookie", HeaderValue::from_str(&cookie).unwrap());
            }
            if with_csrf {
                h.insert("x-csrf-token", HeaderValue::from_str(&csrf).unwrap());
            }
            let body = read.slice(origin.len() + content_type.len()..);
            drop(read);
            let before = WIPED_BODIES.with(|n| n.get());
            let response = command(State(Arc::clone(&app)), h, body).await;
            assert_eq!(response.status(), status, "{case}");
            assert_eq!(
                WIPED_BODIES.with(|n| n.get()),
                before + 1,
                "{case}: the refused body was overwritten"
            );
        }
    }
}

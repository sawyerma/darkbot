//! DEC-18 §3 (P0-022, C-11): the typed records of the provider pre-measurement — its plan
//! (`PROVIDER_PROBE_PLAN`) and its measurement (`PROVIDER_MEASUREMENT`) — and the
//! `PROVIDER_EVIDENCE` a measurement derives. Plain data, independent of the health engine, so
//! every feature set registers and reads the same kinds (`provider_measure` measures).
use crate::analytics::journal;
use crate::analytics::payload::Payload;
use serde::{Deserialize, Serialize};

/// The store of plans and measurements (under a project; git-ignored, P0-026).
pub const STORE: &str = "data/provider_measure";

/// A run path the pre-measurement qualifies (DEC-18 §3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Mode {
    /// The BOOT network audit (every run's).
    Boot,
    /// The 600-s econ4h calibration (C-05).
    Econ4h,
    /// The 30-min C0-MO raw capture, boot mode `market` (C-10).
    C0Mo,
}
impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Boot, Mode::Econ4h, Mode::C0Mo];
    pub fn name(self) -> &'static str {
        match self {
            Mode::Boot => "boot",
            Mode::Econ4h => "econ4h",
            Mode::C0Mo => "c0-mo",
        }
    }
    /// The run a boot flow of operating mode `mode` starts; `None`: a mode no measurement
    /// qualifies (shadow: C-07 is not granted).
    pub fn of_run(mode: &str) -> Option<Mode> {
        match mode {
            "econ4h" => Some(Mode::Econ4h),
            super::boot_audit::MARKET_MODE => Some(Mode::C0Mo),
            _ => None,
        }
    }
}

/// One read of a mode: an RPC method, or a subscription (its method and commitment).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Read {
    Rpc(String),
    Wss(String, String),
}
impl Read {
    pub fn text(&self) -> String {
        match self {
            Read::Rpc(m) => m.clone(),
            Read::Wss(m, c) => format!("{m}@{c}"),
        }
    }
}

/// What one mode reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeReads {
    pub mode: Mode,
    pub reads: Vec<Read>,
}

/// A provider of the measured pair: its registry id and secret-free binding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRef {
    pub id: String,
    pub binding: String,
}

/// The pre-measurement's bounds (`measure_*`, at most the C-11 approval's).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasureLimits {
    pub seconds: u64,
    pub rps: u64,
    pub attempts: u64,
    pub mib: u64,
    pub subscription_types: u64,
    pub subscriptions_active: u64,
    pub subscription_seconds: u64,
}
impl MeasureLimits {
    pub fn of(l: &darkbot_solana::runtime_config::Limits) -> Self {
        Self {
            seconds: l.measure_seconds,
            rps: l.measure_rps,
            attempts: l.measure_attempts,
            mib: l.measure_mib,
            subscription_types: l.measure_subscription_types,
            subscriptions_active: l.measure_subscriptions_active,
            subscription_seconds: l.measure_subscription_seconds,
        }
    }
    pub fn text(&self) -> String {
        format!(
            "seconds={} rps={} attempts={} mib={} subscription_types={} subscriptions_active={} subscription_seconds={}",
            self.seconds,
            self.rps,
            self.attempts,
            self.mib,
            self.subscription_types,
            self.subscriptions_active,
            self.subscription_seconds
        )
    }
}

/// One probe: a read, once, with the parameters it sends (shown to the operator).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Probe {
    pub read: Read,
    pub params: String,
}

/// The offline step: what is measured, against which pair, under which bounds and identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbePlan {
    pub plan_id: String,
    /// `None`: the wall clock was not readable.
    pub created_unix_ms: Option<u64>,
    pub registry_version: u64,
    pub rpc: ProviderRef,
    pub wss: ProviderRef,
    /// SHA-256 of the measurement's effective configuration (bindings and bounds included).
    pub config_digest: String,
    /// SHA-256 of the measuring executable.
    pub binary_sha256: String,
    pub limits: MeasureLimits,
    pub program: String,
    pub group: String,
    pub modes: Vec<ModeReads>,
    /// Every read once, in order (RPC first, then the subscriptions).
    pub probes: Vec<Probe>,
}
impl Payload for ProbePlan {
    const KIND: u16 = journal::kind::PROVIDER_PROBE_PLAN;
}

/// A read's result (DEC-18 §3.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Supported,
    /// The provider does not offer it (its error).
    MethodUnsupported(String),
    /// Not shown either way: no fixture, a timeout, a budget, an unexpected answer.
    NotProven(String),
    /// Refused: rate limit, missing permission, the bounds.
    Blocked(String),
}
impl Outcome {
    pub fn text(&self) -> String {
        match self {
            Outcome::Supported => "SUPPORTED".into(),
            Outcome::MethodUnsupported(r) => format!("METHOD_UNSUPPORTED {r}"),
            Outcome::NotProven(r) => format!("NOT_PROVEN {r}"),
            Outcome::Blocked(r) => format!("BLOCKED {r}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub read: Read,
    pub outcome: Outcome,
    /// RPC attempts it took and response bytes it read (a subscription: frames and bytes).
    pub attempts: u64,
    pub bytes: u64,
    pub elapsed_ms: u64,
}

/// A mode's verdict: SUPPORTED when every read it needs is; else its first read that is not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeVerdict {
    pub mode: Mode,
    pub outcome: Outcome,
}

/// The measurement (DEC-18 §3.4): its plan, every read's result, the modes' verdicts and what
/// the provider served — the capacity is what was measured (1 RPS, user decision 02.11.2026).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMeasurement {
    pub plan_id: String,
    pub plan_sha256: String,
    pub rpc: ProviderRef,
    pub wss: ProviderRef,
    pub started_unix_ms: Option<u64>,
    pub finished_unix_ms: Option<u64>,
    pub results: Vec<ProbeResult>,
    /// What each mode reads (the plan's, from the measuring code) and its verdict.
    pub mode_reads: Vec<ModeReads>,
    pub modes: Vec<ModeVerdict>,
    /// The market capture source the subscriptions were measured for (`logs`).
    pub capture_source: String,
    /// How the simulation probe was signed: `UNSIGNED_SIMULATION` (sigVerify off, never sent;
    /// no FE-02/FE-03 parity rests on it, DEC-18 §4.3).
    pub simulation_signing: String,
    pub rpc_attempts: u64,
    /// Request and response bytes of the RPC probes.
    pub rpc_bytes: u64,
    pub wss_frames: u64,
    pub wss_bytes: u64,
    /// A 429 (or its JSON-RPC form) was seen: no rate is qualified.
    pub rate_limited: bool,
    /// The rate the reads were served at without a refusal (0 when rate limited).
    pub allowed_rps: u64,
    /// RPC calls the provider answered.
    pub served_calls: u64,
}
impl Payload for ProviderMeasurement {
    const KIND: u16 = journal::kind::PROVIDER_MEASUREMENT;
}
impl ProviderMeasurement {
    pub fn verdict(&self, mode: Mode) -> Option<&Outcome> {
        self.modes
            .iter()
            .find(|v| v.mode == mode)
            .map(|v| &v.outcome)
    }
}

/// The `PROVIDER_EVIDENCE` a measurement (`digest`: its record's SHA-256) derives — the one
/// derivation its import and every admission check use: the RPC proof passes when every BOOT
/// read was served and nothing was rate limited, the WSS is FREE_ONLY-qualified when the BOOT
/// subscriptions were; the rate is the measured one (none when nothing was served), the calls the
/// served ones; no archive is qualified.
pub fn derive_evidence(
    m: &ProviderMeasurement,
    digest: &str,
) -> super::provider_evidence::ProviderEvidence {
    let boot = |want: fn(&Read) -> bool| {
        m.mode_reads
            .iter()
            .find(|r| r.mode == Mode::Boot)
            .is_some_and(|r| {
                r.reads.iter().filter(|r| want(r)).all(|r| {
                    m.results
                        .iter()
                        .any(|p| &p.read == r && p.outcome == Outcome::Supported)
                })
            })
    };
    super::provider_evidence::ProviderEvidence {
        providers: vec![super::provider_evidence::ProviderProof {
            policy: "FREE_ONLY".into(),
            passed: !m.rate_limited && boot(|r| matches!(r, Read::Rpc(_))),
            provider_binding: m.rpc.binding.clone(),
            measured_at_unix_s: m.finished_unix_ms.unwrap_or_default() as f64 / 1000.0,
            evidence_sha256: Some(digest.to_string()),
            allowed_rps: m.allowed_rps as f64,
            available_calls: m.served_calls,
        }],
        wss_provider_binding: Some(m.wss.binding.clone()),
        wss_free_only: !m.rate_limited && boot(|r| matches!(r, Read::Wss(..))),
        archive_qualified: false,
        source_sha256: digest.to_string(),
    }
}

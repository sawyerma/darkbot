//! The C3 production path in PAPER mode (order V2 §7.4, PHASENPLAN §11/§16):
//! a pending launch goes through the production venue decoder, the C2
//! chain-state snapshot, the venue economics and admission, the production
//! BUY build and signer, the Unit-4 intent lane, the production submission
//! plan of the selected adapter (the 48Club builder bundle of the original
//! launch and the BUY, BSC-008 = A), its request frame and the production
//! dispatch; it ends at the PAPER sink (`T_shadow_sink`, never `T_send`).
//!
//! Every stage is the production code; only the end differs: the adapter's
//! transport is the PAPER sink, which takes the complete request and has
//! no socket, stream or client (`paper`), and the signer is the PAPER
//! identity. The dispatch is the diagnostic one: a PAPER request is never a
//! release.
//!
//! Lanes: a PAPER transaction is never mined, so no inclusion proof will
//! ever resolve its intent, and a Unit-4 lane blocks behind an unresolved
//! head. The executor therefore arms a bounded pool of single-intent lanes
//! when it starts, behind one durable fence (created, written and synced at
//! arming, never on the entry path), and every PAPER BUY consumes one lane.
//! What production enforces across one session is enforced across the
//! pool: one session exposure cap for all of it, and one BUY per token.
//! Each PAPER BUY is the next BUY from the same canonical state, so every
//! lane starts at the same canonical nonce.
//!
//! Phase I: Strategy C's decisions take the same path. A PASS is a BUY of
//! the policy's input on the exact curve the strategy replayed
//! (`CurveState`, the C2 canonical snapshot), an exit trigger the approval
//! of the Portal and the native-output SELL of the position's tokens on its
//! shadow curve; each is admitted, signed by the PAPER identity, bound in a
//! lane and planned as the single raw transaction of the local node's
//! adapter (`SingleRawBuy`), whose transport is the sink. Their lanes are
//! their own pool behind the same fence: BUYs and approvals at the next
//! nonce, SELLs at the one after.
//!
//! Plan 28.10. (package 3): the executor is generic over its [`End`], the
//! one place PAPER and LIVE differ. Every stage up to the lane's final
//! binding check is this executor's; the end holds the sender's signer and
//! plans, frames and dispatches the prepared transaction. [`PaperEnd`] is
//! the PAPER identity and the sink over the diagnostic dispatch, as
//! before; the LIVE end (the operator's key, the release dispatch over
//! qualified transports) lives outside this module, which has no transport
//! but the sink.

use crate::paper::{PaperHandoff, PaperIdentity, PaperSink};
use crate::submission::send::{
    Armed, DispatchContext, DispatchError, FanoutReport, SendOutcome, dispatch_diagnostic,
};
use crate::submission::tls::{CLUB48_DOCUMENTED_HOST, HTTPS_PORT, ProviderTarget, TlsEndpoint};
use crate::submission::transport::{Clock, ConnectionState, Transport};
use crate::submission::{club48, local_geth};
use alloy_primitives::{Address, B256, U256};
use darkbot_bsc_trading::chain_state::SnapshotId;
use darkbot_bsc_trading::chain_state::canonical::{FinalityProof, InclusionProof, OrphanProof};
use darkbot_bsc_trading::economics::flap::native_curve20::{
    CurveState, PricedToken, QUALIFIED_MAINNET_INPUT_WEI, QualifiedNativeState, VenueBuyTerms,
    admit_curve_buy, admit_curve_sell, admit_native_buy,
};
use darkbot_bsc_trading::economics::policy::{BuyPolicy, ExitPolicy};
use darkbot_bsc_trading::intent::{
    AdapterId, AdapterSet, ArmedFence, IntentLane, IntentState, Ticket,
};
use darkbot_bsc_trading::signer::Signer;
use darkbot_bsc_trading::submission_policy::capability::{
    AdapterBinding, AdapterTable, CapabilityId, SubmissionMode,
};
use darkbot_bsc_trading::submission_policy::plan::{
    BudgetEnvelope, ChainHead, Launch, LocalBlockStartDeadline, MergePolicy, PlanInputs,
    PreSubmission, ProviderValidity, SubmissionPlan, TransportStartDeadline, TxRef,
};
use darkbot_bsc_trading::submission_policy::release::ReleaseProfile;
use darkbot_bsc_trading::venues::flap::binding::{ExpectedBuy, ExpectedExit, ExpectedTx};
use darkbot_bsc_trading::venues::flap::deployment::ValidatedDeployment;
use darkbot_bsc_trading::venues::flap::profile::{UnsupportedLaunch, classify_launch};
use darkbot_bsc_trading::venues::flap::recognize::{Recognized, recognize};
use std::collections::VecDeque;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// The release profile whose rules the PAPER plan follows.
pub const PAPER_PROFILE: ReleaseProfile = ReleaseProfile::BirthEntryV1;

/// Which PAPER paths take the PAPER sender's nonce sequence in one arm
/// (`BSC_EXEC_SENDER_PATHS`, Phase I gate M2). C3's BUYs are alternatives
/// at the next nonce n; Strategy C's intents are a sequence from n. With
/// both, the first to disclose n owns it: after a C3 BUY the strategy
/// starts at n+1; after a strategy intent every later C3 BUY of the arm is
/// refused NONCE_TAKEN. A path left out is refused PATH_OFF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SenderPaths {
    C3,
    StrategyC,
    Both,
}

impl SenderPaths {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "C3" => Ok(Self::C3),
            "STRATEGY_C" => Ok(Self::StrategyC),
            "C3+STRATEGY_C" => Ok(Self::Both),
            other => Err(format!(
                "BSC_EXEC_SENDER_PATHS {other:?}: C3, STRATEGY_C or C3+STRATEGY_C"
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::C3 => "C3",
            Self::StrategyC => "STRATEGY_C",
            Self::Both => "C3+STRATEGY_C",
        }
    }

    fn c3(self) -> bool {
        self != Self::StrategyC
    }

    fn strategy(self) -> bool {
        self != Self::C3
    }
}

/// The operating values of the PAPER path (`BSC_EXEC_*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecConfig {
    pub input_wei: u128,
    pub slippage_bps: u16,
    /// An exit's (the SELL's) slippage, its own limit (BSC3-069).
    pub exit_slippage_bps: u16,
    pub gas_price_wei: u64,
    pub gas_limit: u64,
    /// Cumulative exposure cap of the whole PAPER session (Unit 4, E5-1),
    /// shared by every lane of the pool.
    pub session_cap_wei: u128,
    /// Single-intent lanes armed per start.
    pub lanes: usize,
    /// Largest calldata the decoder accepts.
    pub max_calldata: usize,
    /// Latency samples kept for the status quantiles.
    pub latency_samples: usize,
    pub policy_generation: u64,
    /// Generation of the adapter capability table.
    pub capability_generation: u64,
    /// Blocks after the head the provider may include the bundle in
    /// (`maxBlockNumber` = head + this), within the profile's window.
    pub validity_blocks: u64,
    /// The transport start deadline after `T_ingress`, within the profile's
    /// limit.
    pub start_limit_ns: u64,
    /// Largest adapter request, in bytes.
    pub max_frame: usize,
    /// Phase I: single-intent lanes for Strategy C's PAPER intents, one per
    /// nonce of the sender's sequence (BUY n, approval n+1, SELL n+2).
    pub strategy_lanes: usize,
    /// The exposure cap of Strategy C's PAPER intents in one session (no
    /// money is involved; the strategy's own limits bound its entries).
    pub strategy_session_cap_wei: u128,
    /// Gas limits of an exit's approval and SELL.
    pub approve_gas_limit: u64,
    pub sell_gas_limit: u64,
    /// Phase I gate L-a: the oldest canonical head (its timestamp against
    /// now) a strategy intent is formed on; a follower catching up does
    /// not act on old blocks as if they were fresh.
    pub strategy_max_head_age_ms: u64,
    /// Phase I gate M2: the PAPER paths that take the sender.
    pub sender_paths: SenderPaths,
}

impl ExecConfig {
    /// The cross-key rules: an armed path must be able to admit a BUY.
    pub fn validate(&self) -> Result<(), String> {
        if self.input_wei != u128::from(QUALIFIED_MAINNET_INPUT_WEI) {
            return Err(format!(
                "BSC_EXEC_INPUT_WEI {} is not the witnessed input {QUALIFIED_MAINNET_INPUT_WEI}",
                self.input_wei
            ));
        }
        let gas = u128::from(self.gas_price_wei) * u128::from(self.gas_limit);
        let one = self
            .input_wei
            .checked_add(gas)
            .ok_or("BSC_EXEC_INPUT_WEI + gas overflows")?;
        if self.session_cap_wei < one {
            return Err(format!(
                "BSC_EXEC_SESSION_CAP_WEI {} is below one BUY ({one})",
                self.session_cap_wei
            ));
        }
        if self.lanes == 0 {
            return Err("BSC_EXEC_LANES must be at least 1".into());
        }
        if !(1..=PAPER_PROFILE.max_block_window()).contains(&self.validity_blocks) {
            return Err(format!(
                "BSC_EXEC_VALIDITY_BLOCKS {} outside 1..={}",
                self.validity_blocks,
                PAPER_PROFILE.max_block_window()
            ));
        }
        if !(1..=PAPER_PROFILE.max_transport_start_limit_ns()).contains(&self.start_limit_ns) {
            return Err(format!(
                "BSC_EXEC_START_LIMIT_NS {} outside 1..={}",
                self.start_limit_ns,
                PAPER_PROFILE.max_transport_start_limit_ns()
            ));
        }
        if self.capability_generation == 0 || self.policy_generation == 0 {
            return Err("BSC_EXEC generations start at 1".into());
        }
        if self.strategy_lanes == 0 {
            return Err("BSC_EXEC_STRATEGY_LANES must be at least 1".into());
        }
        if self.approve_gas_limit == 0 || self.sell_gas_limit == 0 {
            return Err(
                "BSC_EXEC_APPROVE_GAS_LIMIT and BSC_EXEC_SELL_GAS_LIMIT must be positive".into(),
            );
        }
        if self.strategy_max_head_age_ms == 0 {
            return Err("BSC_EXEC_STRATEGY_MAX_HEAD_AGE_MS must be positive".into());
        }
        Ok(())
    }

    /// One BUY's budget: the admitted input and the signed gas, nothing for
    /// a launch (the original is a third party's) or a provider.
    fn budget(&self) -> BudgetEnvelope {
        let gas = U256::from(self.gas_price_wei) * U256::from(self.gas_limit);
        let input = U256::from(self.input_wei);
        BudgetEnvelope {
            max_buy_input_wei: input,
            max_buy_gas_wei: gas,
            max_own_launch_wei: U256::ZERO,
            max_provider_payment_wei: U256::ZERO,
            max_total_wei: input + gas,
        }
    }
}

/// The one adapter slot of a PAPER lane: the selected builder adapter,
/// whose transport is the sink.
pub fn paper_adapter() -> AdapterId {
    // Slot 0 exists in every lane (`MAX_ADAPTERS` > 0).
    AdapterId::new(0).unwrap_or_else(|| unreachable!("adapter slot 0"))
}

/// Where a launch left the path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The decoder did not decode a `newTokenV6` launch (or the signed
    /// bytes were not a transaction).
    NotDecoded,
    /// The launch is not the qualified profile.
    Profile(UnsupportedLaunch),
    /// C2 holds no coherent snapshot for this candidate, or no head.
    NoSnapshot,
    /// The fresh-launch state or the admission refused.
    Admission,
    /// This session already bought the token.
    DuplicateToken,
    /// The BUY would exceed the session's exposure cap.
    SessionCap,
    /// The expectation could not be formed.
    Intent,
    /// Every armed lane is used.
    LanesExhausted,
    /// The lane refused (reserve, prepare).
    Lane,
    /// The build or signature failed.
    Build,
    /// The submission plan or its request frame was refused.
    Plan,
    /// The transport start deadline after `T_ingress` passed: before the
    /// economic admission (only the lane and the token are used) or at the
    /// pre-permit check after it (the exposure counts as well).
    Deadline,
    /// The dispatch did not hand the request to the sink.
    Dispatch,
    /// Phase I gate M1: an exit of a token without a handed-off PAPER BUY
    /// in this session.
    NotHeld,
    /// Phase I gate M3: a strategy nonce was reserved and not handed off;
    /// no later PAPER transaction of the sender could pass that hole.
    NonceGap,
    /// Phase I gate L-a: the canonical head is older than
    /// `strategy_max_head_age_ms`.
    StaleHead,
    /// Phase I gate L-d: a C3 BUY at the next nonce while a strategy
    /// intent holds it.
    NonceTaken,
    /// Phase I gate M2: the path does not take the sender in this arm
    /// (`BSC_EXEC_SENDER_PATHS`).
    PathOff,
    /// Plan 28.10.: the plan or a transport is not qualified for a release.
    Unqualified,
    /// Plan 28.10.: the native owner did not admit the LIVE intent now.
    Owner,
    /// BSC3-055: an end whose transactions reach the chain takes a nonce
    /// only when it is the sender's canonical next one (C2, clean): every
    /// lower nonce is consumed on chain. Nothing was consumed; the intent
    /// may come again once the lower nonce is.
    SenderNonce,
    /// BSC3-048: an open position's exit waits for the sender's nonce; no
    /// new BUY takes it first.
    ExitFirst,
}

impl Refusal {
    pub fn name(self) -> &'static str {
        match self {
            Self::NotDecoded => "NOT_DECODED",
            Self::Profile(_) => "PROFILE",
            Self::NoSnapshot => "NO_SNAPSHOT",
            Self::Admission => "ADMISSION",
            Self::DuplicateToken => "DUPLICATE_TOKEN",
            Self::SessionCap => "SESSION_CAP",
            Self::Intent => "INTENT",
            Self::LanesExhausted => "LANES_EXHAUSTED",
            Self::Lane => "LANE",
            Self::Build => "BUILD",
            Self::Plan => "PLAN",
            Self::Deadline => "DEADLINE",
            Self::Dispatch => "DISPATCH",
            Self::NotHeld => "NOT_HELD",
            Self::NonceGap => "NONCE_GAP",
            Self::StaleHead => "STALE_HEAD",
            Self::NonceTaken => "NONCE_TAKEN",
            Self::PathOff => "PATH_OFF",
            Self::Unqualified => "UNQUALIFIED",
            Self::Owner => "OWNER",
            Self::SenderNonce => "SENDER_NONCE",
            Self::ExitFirst => "EXIT_FIRST",
        }
    }
}

/// The local node's adapter slot of the PAPER table (Phase I): the single
/// raw transaction of a Strategy C intent, whose transport is the sink.
pub fn paper_raw_adapter() -> AdapterId {
    // Slot 1 exists in every lane (`MAX_ADAPTERS` > 1).
    AdapterId::new(1).unwrap_or_else(|| unreachable!("adapter slot 1"))
}

/// What a Strategy C PAPER transaction is (Phase I).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentKind {
    Buy,
    Approve,
    Sell,
}

impl IntentKind {
    pub fn code(self) -> u8 {
        match self {
            Self::Buy => 1,
            Self::Approve => 2,
            Self::Sell => 3,
        }
    }
}

/// The exact curve a Strategy C intent is admitted on
/// (`strategy::engine::Engine::curve_of`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrategyCurve {
    pub terms: PricedToken,
    pub reserve: U256,
    pub circulating: U256,
}

/// A Strategy C position whose BUY reached the sink: the tokens it is bound
/// to have bought (its minimum output) and its input (open exposure).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Held {
    token: [u8; 20],
    min_tokens: U256,
    input_wei: U256,
}

/// The C2 facts of a Strategy C intent: the canonical snapshot under
/// (policy, codehash) generations, and the head; and the wall time its age
/// is measured at (unix ms).
pub struct CanonicalContext<'a> {
    pub snapshot: &'a dyn Fn(u64, u64) -> Option<SnapshotId>,
    pub head: Option<ChainHead>,
    pub now_unix_ms: u64,
    /// The sender's canonical next nonce, when C2 knows it exactly (clean):
    /// an end whose transactions reach the chain takes no other.
    pub next_nonce: Option<u64>,
}

impl CanonicalContext<'_> {
    /// The head, if it is at most `max_ms` old (its timestamp is unix
    /// seconds, so the age is overstated by less than a second).
    fn fresh_head(&self, max_ms: u64) -> Result<ChainHead, Refusal> {
        let head = self.head.ok_or(Refusal::NoSnapshot)?;
        let age = self
            .now_unix_ms
            .saturating_sub(head.timestamp.saturating_mul(1_000));
        if age > max_ms {
            return Err(Refusal::StaleHead);
        }
        Ok(head)
    }
}

/// One Strategy C transaction that reached the executor's end (Phase I).
/// Its `times.ingress_ns` is the decision's record time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecIntent<H> {
    pub kind: IntentKind,
    pub token: [u8; 20],
    /// The canonical head block it was formed on.
    pub block: u64,
    pub handoff: H,
    pub times: StageTimes,
    /// The signed transaction (PAPER: a local test artefact, never sent).
    pub raw: Vec<u8>,
}

/// One Strategy C PAPER transaction that reached the sink.
pub type PaperIntent = ExecIntent<PaperHandoff>;

/// The stage times of one PAPER BUY on the runtime's monotonic clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageTimes {
    /// The source received the frame of the pending launch.
    pub ingress_ns: u64,
    pub decoded_ns: u64,
    pub admitted_ns: u64,
    pub signed_ns: u64,
    /// The lane holds the BUY (its final binding check done).
    pub prepared_ns: u64,
    /// Plan formed and the adapter request built.
    pub planned_ns: u64,
    /// The end took the complete request: `T_shadow_sink` (PAPER) or the
    /// release handoff boundary (LIVE).
    pub sink_ns: u64,
}

/// One C3 BUY that reached the executor's end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecBuy<H> {
    pub launch_hash: [u8; 32],
    pub token: [u8; 20],
    pub handoff: H,
    pub times: StageTimes,
    /// The signed BUY (PAPER: a local test artefact, never sent).
    pub raw: Vec<u8>,
}

/// One PAPER BUY that reached the sink.
pub type PaperBuy = ExecBuy<PaperHandoff>;

/// The executor's counters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecCounters {
    pub launches: u64,
    pub handoffs: u64,
    /// Refusals by stage and reason, in first-seen order.
    pub refusals: Vec<(Refusal, u64)>,
}

impl ExecCounters {
    fn refuse(&mut self, r: Refusal) {
        match self.refusals.iter_mut().find(|(k, _)| *k == r) {
            Some((_, n)) => *n += 1,
            None => self.refusals.push((r, 1)),
        }
    }

    /// Refusals of one stage, all profile reasons together.
    pub fn refused(&self, name: &str) -> u64 {
        self.refusals
            .iter()
            .filter(|(k, _)| k.name() == name)
            .map(|(_, n)| n)
            .sum()
    }
}

/// The latest `capacity` ingress→sink latencies (ns), for quantiles.
#[derive(Clone, Debug)]
pub struct Latencies {
    capacity: usize,
    samples: VecDeque<u64>,
    max: u64,
}

impl Latencies {
    fn new(capacity: usize) -> Self {
        Latencies {
            capacity: capacity.max(1),
            samples: VecDeque::with_capacity(capacity.max(1)),
            max: 0,
        }
    }

    fn push(&mut self, ns: u64) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(ns);
        self.max = self.max.max(ns);
    }

    /// (p50, p99, max over the run) of the kept samples; `None` without.
    /// Selection, not a full sort: linear in the kept samples, read only
    /// when the status is written.
    pub fn quantiles(&self) -> Option<(u64, u64, u64)> {
        if self.samples.is_empty() {
            return None;
        }
        let mut kept: Vec<u64> = self.samples.iter().copied().collect();
        let last = kept.len() - 1;
        let mut at = |p: usize| *kept.select_nth_unstable(last * p / 100).1;
        Some((at(50), at(99), self.max))
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// The PAPER sink as the adapter's transport: `write` hands the complete
/// request (the production frame with the signed BUY in its slot) to the
/// sink, read on the runtime clock. It owns no socket, stream or client;
/// nothing is read back.
struct SinkTransport<'a> {
    sink: &'a mut PaperSink,
    clock: &'a dyn Clock,
    tx_hash: [u8; 32],
    max_request_len: usize,
    state: ConnectionState,
    handoff: Option<PaperHandoff>,
}

impl Transport for SinkTransport<'_> {
    fn state(&self) -> ConnectionState {
        self.state
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // One request, taken whole.
        self.handoff = Some(self.sink.handoff(self.tx_hash, buf, self.clock.now_ns()));
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::ErrorKind::WouldBlock.into())
    }

    fn mark(&mut self, state: ConnectionState) {
        self.state = state;
    }

    fn max_request_len(&self) -> usize {
        self.max_request_len
    }
}

/// The armed path towards its end.
pub struct Executor<E: End> {
    end: E,
    deployment: ValidatedDeployment,
    terms: VenueBuyTerms,
    policy: BuyPolicy,
    config: ExecConfig,
    budget: BudgetEnvelope,
    lanes: Vec<IntentLane>,
    used: usize,
    /// Exposure dispatched in this session, against `session_cap_wei`.
    spent_wei: U256,
    /// Tokens bought in this session (at most one per lane).
    bought: Vec<[u8; 20]>,
    fence: PathBuf,
    scratch: Vec<u8>,
    raw: Vec<u8>,
    counters: ExecCounters,
    latencies: Latencies,
    /// Phase I: Strategy C's lanes, one per nonce of the PAPER sender's
    /// sequence from the arming's next nonce (gate M3: BUY n, approval
    /// n+1, SELL n+2, across positions), their open exposure, positions
    /// held and exits taken.
    strategy_lanes: Vec<IntentLane>,
    strategy_first_nonce: u64,
    strategy_used: usize,
    strategy_nonce_gap: bool,
    /// Gate L-d: one sender, one sequence. A C3 BUY disclosed at the next
    /// nonce n (its alternatives all share it) makes the strategy start
    /// at n+1 (its first lane skipped); a strategy intent holding n
    /// refuses C3's later BUYs (NONCE_TAKEN).
    c3_took_nonce: bool,
    strategy_skipped_first: bool,
    strategy_spent_wei: U256,
    strategy_bought: Vec<Held>,
    /// Approvals at the sink and the token amounts they approved.
    approved: Vec<([u8; 20], U256)>,
    exited: Vec<[u8; 20]>,
    exit_policy: ExitPolicy,
    strategy_counters: ExecCounters,
    /// The arming's next nonce: every C3 lane's nonce.
    first_nonce: u64,
    evidence: EvidenceCounters,
}

/// The armed PAPER path.
pub type PaperExecutor = Executor<PaperEnd>;

/// C2's evidence about one of the sender's nonces (plan 28.10.).
#[derive(Clone, Copy, Debug)]
pub enum NonceEvidence<'a> {
    Inclusion(&'a InclusionProof),
    Finality(&'a FinalityProof),
    Orphan(&'a OrphanProof),
}

/// What C2's evidence did to the executor's intents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvidenceCounters {
    pub included: u64,
    pub reverted: u64,
    /// Another transaction consumed an intent's nonce.
    pub replaced: u64,
    pub finalized: u64,
    pub orphaned: u64,
    /// A nonce of the sender no lane of this arm assigned (consumed by a
    /// transaction this executor never made).
    pub unassigned: u64,
}

/// Why the executor could not be armed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArmError {
    Config(String),
    Policy(String),
    Fence(String),
    Lane(String),
}

impl core::fmt::Display for ArmError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Config(m) => write!(f, "paper exec config: {m}"),
            Self::Policy(m) => write!(f, "paper exec policy: {m}"),
            Self::Fence(m) => write!(f, "paper exec fence: {m}"),
            Self::Lane(m) => write!(f, "paper exec lane: {m}"),
        }
    }
}

impl std::error::Error for ArmError {}

/// Creates, writes and syncs one fence file that did not exist, and syncs
/// its directory: the durable attestation behind the armed lanes.
fn durable_fence(path: &Path, bytes: &[u8]) -> Result<(), ArmError> {
    let e = |err: std::io::Error| ArmError::Fence(format!("{}: {err}", path.display()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(e)?;
    file.write_all(bytes).map_err(e)?;
    file.sync_all().map_err(e)?;
    if let Some(dir) = path.parent() {
        fs::File::open(dir).and_then(|d| d.sync_all()).map_err(e)?;
    }
    Ok(())
}

/// What one hand-over takes to its end.
#[derive(Clone, Copy, Debug)]
pub enum Submission<'a> {
    /// C3: the original launch's signed bytes, then the BUY.
    LaunchBundle(&'a [u8]),
    /// Strategy C: the transaction alone (a BUY, an approval or a SELL).
    Single,
}

/// One transaction its lane holds, bound to `expected` and `snapshot`, for
/// the end to plan, frame and dispatch.
pub struct Handover<'a> {
    pub lane: &'a mut IntentLane,
    pub ticket: Ticket,
    pub raw: &'a [u8],
    pub tx_hash: [u8; 32],
    pub expected: ExpectedTx<'a>,
    pub budget: &'a BudgetEnvelope,
    pub snapshot: &'a SnapshotId,
    pub head: ChainHead,
    pub clock: &'a dyn Clock,
    /// Where the transport start deadline runs from.
    pub start_ns: u64,
    pub submission: Submission<'a>,
    pub config: &'a ExecConfig,
}

/// What an end made of one hand-over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handed<H> {
    pub result: Result<H, Refusal>,
    /// Plan formed and the adapter request built (0: not reached).
    pub planned_ns: u64,
    /// The economic admission was consumed: the exposure counts.
    pub admitted: bool,
    /// A permit's write was started (or the dispatch broke after the
    /// economic admission): the bytes may be out, their nonce is taken and
    /// their exposure counts. Otherwise the nonce is taken again (gate N1)
    /// and nothing is spent (gate L2).
    pub disclosed: bool,
}

impl<H> Handed<H> {
    /// Refused before the dispatch: nothing consumed.
    pub fn refused(r: Refusal) -> Self {
        Handed {
            result: Err(r),
            planned_ns: 0,
            admitted: false,
            disclosed: false,
        }
    }
}

/// What a dispatch consumed: (the economic admission, a disclosure).
pub fn consumed_by(dispatched: &Result<FanoutReport, DispatchError>) -> (bool, bool) {
    match dispatched {
        Ok(report) => (
            report.admitted(),
            report.admitted() && !report.admitted_without_disclosure,
        ),
        Err(DispatchError::DispatchBinding(_)) => (true, true),
        Err(_) => (false, false),
    }
}

/// When an end took a hand-over, on the runtime's monotonic clock.
pub trait HandoffTime {
    fn at_ns(&self) -> u64;
}

impl HandoffTime for PaperHandoff {
    fn at_ns(&self) -> u64 {
        self.at_ns
    }
}

/// Where the executor's path ends (plan 28.10.): the sender's signer, and
/// the plan, frame and dispatch of each prepared transaction.
pub trait End {
    type Handoff: HandoffTime + Copy + core::fmt::Debug + PartialEq + Eq;
    /// The fence file prefix and the fence body's magic of this end.
    const FENCE: (&'static str, [u8; 8]);
    /// Whether this end's transactions reach the chain (BSC3-055). Then
    /// the sender's lanes together keep the single-sender order of the
    /// intent contract (Unit 4 invariant 3): a transaction is handed over
    /// only at the sender's canonical next nonce (C2), so at most one nonce
    /// of the sender has bytes out that the chain has not consumed, and an
    /// exit's SELL waits for its approval to be canonical. An end whose
    /// transactions never leave the process (PAPER) plans its sequence
    /// ahead.
    const CHAIN_ORDERED: bool;

    fn address(&self) -> [u8; 20];
    fn signer(&self) -> &Signer;
    fn hand_over(&mut self, h: Handover<'_>) -> Handed<Self::Handoff>;
}

/// The PAPER end: the PAPER identity signs; the 48Club bundle (C3) and the
/// local node's single raw transaction (Strategy C) are planned under the
/// PAPER profile and dispatched diagnostically to the sink. Never a
/// release.
pub struct PaperEnd {
    identity: PaperIdentity,
    table: AdapterTable,
    target: ProviderTarget,
    sink: PaperSink,
}

impl PaperEnd {
    /// The PAPER end of `chain_id` under the capability generation.
    pub fn new(
        identity: PaperIdentity,
        chain_id: u64,
        capability_generation: u64,
    ) -> Result<Self, ArmError> {
        let table = AdapterTable::new(
            chain_id,
            capability_generation,
            &[
                AdapterBinding {
                    adapter: paper_adapter(),
                    capability: CapabilityId::Club48BuilderV1,
                },
                // Phase I: Strategy C's single raw transactions.
                AdapterBinding {
                    adapter: paper_raw_adapter(),
                    capability: CapabilityId::LocalGethV1,
                },
            ],
        )
        .map_err(|e| ArmError::Config(e.to_string()))?;
        let target = ProviderTarget::club48(
            TlsEndpoint::new(CLUB48_DOCUMENTED_HOST, HTTPS_PORT)
                .map_err(|e| ArmError::Config(format!("{e:?}")))?,
        );
        Ok(PaperEnd {
            identity,
            table,
            target,
            sink: PaperSink::default(),
        })
    }

    pub fn sink(&self) -> &PaperSink {
        &self.sink
    }
}

impl End for PaperEnd {
    type Handoff = PaperHandoff;
    const FENCE: (&'static str, [u8; 8]) = ("paper", *b"DBPAPFN3");
    // A PAPER transaction never leaves the process.
    const CHAIN_ORDERED: bool = false;

    fn address(&self) -> [u8; 20] {
        self.identity.address()
    }

    fn signer(&self) -> &Signer {
        self.identity.signer()
    }

    fn hand_over(&mut self, h: Handover<'_>) -> Handed<PaperHandoff> {
        let Handover {
            lane,
            ticket,
            raw,
            tx_hash,
            expected,
            budget,
            snapshot,
            head,
            clock,
            start_ns,
            submission,
            config,
        } = h;
        let (adapter, mode, launch, merge) = match submission {
            Submission::LaunchBundle(launch_raw) => (
                paper_adapter(),
                SubmissionMode::PublicFullLaunchBundle,
                Launch::PublicOriginal(TxRef::of(launch_raw)),
                MergePolicy::NoMerge,
            ),
            Submission::Single => (
                paper_raw_adapter(),
                SubmissionMode::SingleRawBuy,
                Launch::None,
                MergePolicy::ProviderDefault,
            ),
        };
        let table = &self.table;
        let target = &self.target;
        let planned = (|| {
            let adapters = AdapterSet::new(&[adapter]).ok_or(Refusal::Plan)?;
            let last_block = head
                .number
                .checked_add(config.validity_blocks)
                .ok_or(Refusal::Plan)?;
            let bundle = matches!(submission, Submission::LaunchBundle(_));
            let plan = SubmissionPlan::new(
                table,
                expected,
                budget,
                PlanInputs {
                    ticket,
                    buy: TxRef::of(raw),
                    launch,
                    mode,
                    profile: PAPER_PROFILE,
                    adapters,
                    local_block_deadline: LocalBlockStartDeadline {
                        last_head_block: last_block,
                    },
                    provider_validity: bundle.then_some(ProviderValidity {
                        max_block_number: last_block,
                        min_timestamp: None,
                        max_timestamp: None,
                    }),
                    merge,
                    pre_submission: PreSubmission::No,
                    provider_payment_wei: U256::ZERO,
                },
            )
            .map_err(|_| Refusal::Plan)?;
            let frame = match submission {
                Submission::LaunchBundle(launch_raw) => club48::frame(
                    &plan,
                    table,
                    adapter,
                    &target.frame_endpoint(),
                    launch_raw,
                    config.max_frame,
                ),
                Submission::Single => local_geth::frame(&plan, table, adapter, config.max_frame),
            }
            .map_err(|_| Refusal::Plan)?;
            let deadline = TransportStartDeadline::for_profile(
                PAPER_PROFILE,
                clock.id(),
                start_ns,
                config.start_limit_ns,
            )
            .map_err(|_| Refusal::Plan)?;
            Ok((plan, frame, deadline))
        })();
        let (plan, mut frame, deadline) = match planned {
            Ok(p) => p,
            Err(r) => return Handed::refused(r),
        };
        let planned_ns = clock.now_ns();
        let mut transport = SinkTransport {
            sink: &mut self.sink,
            clock,
            tx_hash,
            max_request_len: config.max_frame,
            state: ConnectionState::Ready,
            handoff: None,
        };
        let dispatched = dispatch_diagnostic(
            lane,
            &plan,
            &DispatchContext {
                table,
                current: snapshot,
                head,
            },
            clock,
            &deadline,
            &mut [Armed {
                adapter,
                transport: &mut transport,
                frame: &mut frame,
            }],
        );
        let handoff = transport.handoff;
        let (admitted, disclosed) = consumed_by(&dispatched);
        let result = match dispatched {
            Err(_) => Err(Refusal::Dispatch),
            Ok(report) => match report.adapter(adapter).map(|r| r.outcome) {
                Some(SendOutcome::HandedOff) => handoff.ok_or(Refusal::Dispatch),
                Some(SendOutcome::SkippedDeadline(_)) => Err(Refusal::Deadline),
                _ => Err(Refusal::Dispatch),
            },
        };
        Handed {
            result,
            planned_ns,
            admitted,
            disclosed,
        }
    }
}

/// The executor parts one hand-over needs.
struct Parts<'a, E: End> {
    end: &'a mut E,
    lane: &'a mut IntentLane,
    config: &'a ExecConfig,
    scratch: &'a mut Vec<u8>,
}

/// Binds a signed transaction in its lane (its final binding check) and
/// hands it to the end. Returns the handoff, the prepared and planned
/// times, and what the dispatch consumed: (admission, disclosure).
#[allow(clippy::too_many_arguments)]
fn to_end<E: End>(
    parts: Parts<'_, E>,
    ticket: Ticket,
    raw: &[u8],
    expected: ExpectedTx<'_>,
    budget: &BudgetEnvelope,
    snapshot: &SnapshotId,
    head: ChainHead,
    clock: &dyn Clock,
    start_ns: u64,
    submission: Submission<'_>,
) -> (Result<E::Handoff, Refusal>, u64, u64, (bool, bool)) {
    let Parts {
        end,
        lane,
        config,
        scratch,
    } = parts;
    let tx_hash = match lane.prepare(ticket, raw, expected, snapshot, scratch) {
        Ok(h) => h,
        Err(_) => return (Err(Refusal::Lane), 0, 0, (false, false)),
    };
    let prepared_ns = clock.now_ns();
    let handed = end.hand_over(Handover {
        lane,
        ticket,
        raw,
        tx_hash: tx_hash.0,
        expected,
        budget,
        snapshot,
        head,
        clock,
        start_ns,
        submission,
        config,
    });
    (
        handed.result,
        prepared_ns,
        handed.planned_ns,
        (handed.admitted, handed.disclosed),
    )
}

/// One single-intent lane at `nonce` behind the arm's durable fence.
fn fenced_lane(
    chain_id: u64,
    sender: Address,
    nonce: u64,
    cap: u128,
) -> Result<IntentLane, ArmError> {
    let armed = ArmedFence::from_durable_startup(chain_id, sender, nonce, U256::from(cap));
    IntentLane::armed(armed, 1).map_err(|e| ArmError::Lane(format!("{e:?}")))
}

/// A runtime clock reading as a submission [`Clock`].
pub struct FnClock<F: Fn() -> u64> {
    pub id: u32,
    pub now: F,
}

impl<F: Fn() -> u64> Clock for FnClock<F> {
    fn id(&self) -> u32 {
        self.id
    }

    fn now_ns(&self) -> u64 {
        (self.now)()
    }
}

/// Where and as what one arm fences its lanes: the canonical next nonce
/// of the PAPER sender (a C2 nonce fact), the fence directory, the
/// process's session and this arm's number in it.
#[derive(Clone, Copy, Debug)]
pub struct Arming<'a> {
    pub next_nonce: u64,
    pub fence_dir: &'a Path,
    pub session: u64,
    pub arm: u64,
}

/// One verified pending launch: the hash of its signed bytes `raw`, its
/// calldata `input` to `to`, received at `ingress_ns`.
#[derive(Clone, Copy, Debug)]
pub struct PendingLaunch<'a> {
    pub hash: [u8; 32],
    pub raw: &'a [u8],
    pub to: Option<[u8; 20]>,
    pub input: &'a [u8],
    pub ingress_ns: u64,
}

/// The C2 facts one launch needs.
pub struct LaunchContext<'a> {
    /// The coherent snapshot of a candidate under (policy, codehash)
    /// generations.
    pub snapshot: &'a dyn Fn(B256, u64, u64) -> Option<SnapshotId>,
    /// The canonical head: number and timestamp (unix seconds).
    pub head: Option<ChainHead>,
    /// The sender's canonical next nonce, when C2 knows it exactly (clean):
    /// an end whose transactions reach the chain takes no other.
    pub next_nonce: Option<u64>,
}

impl Executor<PaperEnd> {
    /// Arms the PAPER path: the PAPER end (its adapter table and the
    /// selected builder target), then [`Executor::arm_end`].
    pub fn arm(
        identity: PaperIdentity,
        deployment: ValidatedDeployment,
        terms: VenueBuyTerms,
        config: ExecConfig,
        arming: Arming<'_>,
    ) -> Result<Self, ArmError> {
        config.validate().map_err(ArmError::Config)?;
        let chain_id = deployment.deployment().chain_id;
        let end = PaperEnd::new(identity, chain_id, config.capability_generation)?;
        Self::arm_end(end, deployment, terms, config, arming)
    }

    pub fn sink(&self) -> &PaperSink {
        self.end.sink()
    }
}

impl<E: End> Executor<E> {
    /// Arms the path towards `end`: the policy, then one durable fence
    /// `<end>-<session>-<arm>.fence` in `fence_dir` (unique per arm) and
    /// behind it `config.lanes` single-intent lanes at the sender's
    /// canonical `next_nonce` (a C2 nonce fact).
    pub fn arm_end(
        end: E,
        deployment: ValidatedDeployment,
        terms: VenueBuyTerms,
        config: ExecConfig,
        arming: Arming<'_>,
    ) -> Result<Self, ArmError> {
        let Arming {
            next_nonce,
            fence_dir,
            session,
            arm,
        } = arming;
        config.validate().map_err(ArmError::Config)?;
        let policy = BuyPolicy::new(
            config.policy_generation,
            U256::from(config.input_wei),
            config.slippage_bps,
            config.gas_price_wei,
            config.gas_limit,
        )
        .map_err(|e| ArmError::Policy(format!("{e:?}")))?;
        let exit_policy = ExitPolicy::new(
            config.policy_generation,
            config.exit_slippage_bps,
            config.gas_price_wei,
            config.approve_gas_limit,
            config.sell_gas_limit,
        )
        .map_err(|e| ArmError::Policy(format!("{e:?}")))?;
        let chain_id = deployment.deployment().chain_id;
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(fence_dir)
            .map_err(|e| ArmError::Fence(format!("{}: {e}", fence_dir.display())))?;
        let sender = Address::from(end.address());
        // The strategy's nonces: one lane per nonce of the sequence.
        next_nonce
            .checked_add(config.strategy_lanes as u64)
            .ok_or_else(|| ArmError::Config("the strategy nonces overflow".into()))?;
        let mut body = Vec::with_capacity(8 + 8 + 20 + 8 + 16 + 8 + 8 + 8 + 8 + 16);
        body.extend_from_slice(&E::FENCE.1);
        body.extend_from_slice(&chain_id.to_le_bytes());
        body.extend_from_slice(sender.as_slice());
        body.extend_from_slice(&next_nonce.to_le_bytes());
        body.extend_from_slice(&config.session_cap_wei.to_le_bytes());
        body.extend_from_slice(&(config.lanes as u64).to_le_bytes());
        body.extend_from_slice(&config.policy_generation.to_le_bytes());
        body.extend_from_slice(&config.capability_generation.to_le_bytes());
        body.extend_from_slice(&(config.strategy_lanes as u64).to_le_bytes());
        body.extend_from_slice(&config.strategy_session_cap_wei.to_le_bytes());
        let fence = fence_dir.join(format!("{}-{session}-{arm}.fence", E::FENCE.0));
        durable_fence(&fence, &body)?;
        let lane_at = |nonce: u64, cap: u128| fenced_lane(chain_id, sender, nonce, cap);
        // C3's pool: alternative BUYs from the same canonical state, all
        // at the next nonce. Strategy C's: its sequence, one nonce each.
        let pools = (
            (0..config.lanes)
                .map(|_| lane_at(next_nonce, config.session_cap_wei))
                .collect::<Result<Vec<_>, _>>(),
            (0..config.strategy_lanes as u64)
                .map(|i| lane_at(next_nonce + i, config.strategy_session_cap_wei))
                .collect::<Result<Vec<_>, _>>(),
        );
        let (lanes, strategy_lanes) = match pools {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                let _ = fs::remove_file(&fence);
                return Err(e);
            }
        };
        let samples = config.latency_samples;
        let budget = config.budget();
        Ok(Executor {
            end,
            deployment,
            terms,
            policy,
            budget,
            lanes,
            used: 0,
            spent_wei: U256::ZERO,
            bought: Vec::with_capacity(config.lanes),
            fence,
            scratch: Vec::with_capacity(512),
            raw: Vec::with_capacity(512),
            counters: ExecCounters::default(),
            latencies: Latencies::new(samples),
            strategy_used: 0,
            strategy_first_nonce: next_nonce,
            strategy_nonce_gap: false,
            c3_took_nonce: false,
            strategy_skipped_first: false,
            strategy_spent_wei: U256::ZERO,
            strategy_bought: Vec::with_capacity(strategy_lanes.len()),
            approved: Vec::with_capacity(strategy_lanes.len()),
            exited: Vec::with_capacity(strategy_lanes.len()),
            strategy_lanes,
            exit_policy,
            strategy_counters: ExecCounters::default(),
            first_nonce: next_nonce,
            evidence: EvidenceCounters::default(),
            config,
        })
    }

    /// Disarms: the lanes go and the arm's fence is removed (a PAPER BUY
    /// never leaves the process, so nothing waits for reconciliation; the
    /// LIVE owner disarms only what its reconciliation resolved).
    pub fn disarm(self) -> Result<(), String> {
        fs::remove_file(&self.fence).map_err(|e| format!("{}: {e}", self.fence.display()))
    }

    /// A launch the runtime could not hand over (its signed bytes are not a
    /// transaction): counted, never silent.
    pub fn refuse_undecodable(&mut self) {
        self.counters.launches += 1;
        self.counters.refuse(Refusal::NotDecoded);
    }

    /// One verified pending launch, its `T_ingress` on `clock`.
    pub fn on_launch(
        &mut self,
        launch: PendingLaunch<'_>,
        c2: &LaunchContext<'_>,
        clock: &dyn Clock,
    ) -> Result<ExecBuy<E::Handoff>, Refusal> {
        self.counters.launches += 1;
        let result = self.path(launch, c2, clock);
        match &result {
            Ok(buy) => {
                self.counters.handoffs += 1;
                self.latencies
                    .push(buy.times.sink_ns.saturating_sub(buy.times.ingress_ns));
            }
            Err(r) => self.counters.refuse(*r),
        }
        result
    }

    fn path(
        &mut self,
        launch: PendingLaunch<'_>,
        c2: &LaunchContext<'_>,
        clock: &dyn Clock,
    ) -> Result<ExecBuy<E::Handoff>, Refusal> {
        if !self.config.sender_paths.c3() {
            return Err(Refusal::PathOff);
        }
        let PendingLaunch {
            hash,
            raw: launch_raw,
            to,
            input,
            ingress_ns,
        } = launch;
        let portal = self.deployment.deployment().portal;
        let Recognized::LaunchV6(launch) = recognize(
            portal,
            to.map(Address::from),
            input,
            self.config.max_calldata,
        ) else {
            return Err(Refusal::NotDecoded);
        };
        let decoded_ns = clock.now_ns();
        let candidate = B256::from(hash);
        let classified =
            classify_launch(&self.deployment, candidate, &launch).map_err(Refusal::Profile)?;
        let snapshot = (c2.snapshot)(
            candidate,
            self.policy.generation(),
            self.deployment.codehash_generation(),
        )
        .ok_or(Refusal::NoSnapshot)?;
        let head = c2.head.ok_or(Refusal::NoSnapshot)?;
        // A pending launch: no trade of its token precedes it.
        let state = QualifiedNativeState::fresh_launch(&classified, &self.terms, 0, snapshot)
            .map_err(|_| Refusal::Admission)?;
        let admitted = admit_native_buy(&state, &self.policy).map_err(|_| Refusal::Admission)?;
        let admitted_ns = clock.now_ns();
        let token = admitted.token().into_array();
        if self.holds(&token) {
            return Err(Refusal::DuplicateToken);
        }
        let exposure = self.budget.max_total_wei;
        if self.spent_wei + exposure > U256::from(self.config.session_cap_wei) {
            return Err(Refusal::SessionCap);
        }
        if self.strategy_used > 0 && !self.strategy_skipped_first {
            return Err(Refusal::NonceTaken);
        }
        // BSC3-055: every C3 lane is at the arm's nonce; it goes only while
        // that is the sender's canonical next nonce.
        if E::CHAIN_ORDERED && c2.next_nonce != Some(self.first_nonce) {
            return Err(Refusal::SenderNonce);
        }
        let Some(lane) = self.lanes.get_mut(self.used) else {
            return Err(Refusal::LanesExhausted);
        };
        // The lane is consumed by this attempt, whatever happens next.
        self.used += 1;
        let ticket = lane.reserve(admitted.token()).map_err(|_| Refusal::Lane)?;
        // The token is taken at the reservation, as the production lane
        // burns it there: a replacement launch of it is refused.
        self.bought.push(token);
        let sender = Address::from(self.end.address());
        let expected = ExpectedBuy::from_admitted(
            &self.deployment,
            &admitted,
            &self.policy,
            sender,
            ticket.nonce(),
        )
        .map_err(|_| Refusal::Intent)?;
        expected
            .sign(self.end.signer(), &mut self.scratch, &mut self.raw)
            .map_err(|_| Refusal::Build)?;
        let signed_ns = clock.now_ns();
        let (handoff, prepared_ns, planned_ns, (admitted, disclosed)) = to_end(
            Parts {
                end: &mut self.end,
                lane,
                config: &self.config,
                scratch: &mut self.scratch,
            },
            ticket,
            &self.raw,
            ExpectedTx::Buy(&expected),
            &self.budget,
            &snapshot,
            head,
            clock,
            ingress_ns,
            Submission::LaunchBundle(launch_raw),
        );
        // The exposure is the session's once the economic admission was
        // consumed (`begin_send`), whatever followed.
        if admitted {
            self.spent_wei += exposure;
        }
        self.c3_took_nonce |= disclosed;
        let handoff = handoff?;
        Ok(ExecBuy {
            launch_hash: hash,
            token,
            handoff,
            times: StageTimes {
                ingress_ns,
                decoded_ns,
                admitted_ns,
                signed_ns,
                prepared_ns,
                planned_ns,
                sink_ns: handoff.at_ns(),
            },
            raw: self.raw.clone(),
        })
    }

    /// Phase I: a Strategy C PASS on `token` (`decided_ns`: the decision
    /// record's time, kept with the intent): a BUY of `input_wei` on the
    /// strategy's exact curve, through admission, the PAPER signature, the
    /// next strategy lane and the single-raw plan to the sink. The
    /// transport start deadline runs from now, when the decision is
    /// processed (gate H1), not from the record.
    pub fn on_strategy_buy(
        &mut self,
        token: [u8; 20],
        curve: StrategyCurve,
        input_wei: U256,
        c2: &CanonicalContext<'_>,
        clock: &dyn Clock,
        decided_ns: u64,
    ) -> Result<ExecIntent<E::Handoff>, Refusal> {
        self.strategy_counters.launches += 1;
        let start_ns = clock.now_ns();
        let result = self.strategy_buy(token, curve, input_wei, c2, clock, decided_ns, start_ns);
        match &result {
            Ok(_) => self.strategy_counters.handoffs += 1,
            Err(r) => self.strategy_counters.refuse(*r),
        }
        result
    }

    /// Phase I: a Strategy C exit trigger on `token`: the approval of the
    /// Portal and the native-output SELL on the position's shadow curve,
    /// at the next two strategy nonces. Only for a BUY of the token this
    /// session handed off (gate M1); the SELL sells at most that BUY's
    /// bound minimum output, the tokens it is sure to hold. The intents
    /// that reached the sink are returned even when a later one was
    /// refused (gate L3), with that refusal. A chain ordered end hands over
    /// the approval alone (no refusal: the exit is not over, see
    /// [`Executor::exited`]); the exit again, once the approval is
    /// canonical, hands over the SELL (BSC3-055).
    pub fn on_strategy_exit(
        &mut self,
        token: [u8; 20],
        tokens: U256,
        curve: StrategyCurve,
        c2: &CanonicalContext<'_>,
        clock: &dyn Clock,
        triggered_ns: u64,
    ) -> (Vec<ExecIntent<E::Handoff>>, Option<Refusal>) {
        self.strategy_counters.launches += 1;
        let start_ns = clock.now_ns();
        let mut out = Vec::with_capacity(2);
        let refused = self
            .strategy_exit(
                token,
                tokens,
                curve,
                c2,
                clock,
                triggered_ns,
                start_ns,
                &mut out,
            )
            .err();
        self.strategy_counters.handoffs += out.len() as u64;
        if let Some(r) = refused {
            self.strategy_counters.refuse(r);
        }
        (out, refused)
    }

    /// Whether this arm holds `token` already, by either strategy: one
    /// sender buys a token once (BSC3-055, Unit 4 invariant 2).
    fn holds(&self, token: &[u8; 20]) -> bool {
        self.bought.contains(token) || self.strategy_bought.iter().any(|h| h.token == *token)
    }

    /// The nonce the next strategy lane is at.
    pub fn strategy_next_nonce(&self) -> u64 {
        let skip = self.strategy_used == 0 && self.c3_took_nonce;
        self.strategy_first_nonce + (self.strategy_used + usize::from(skip)) as u64
    }

    /// Whether this arm's SELL of `token` may have left (its exit is over
    /// for this arm).
    pub fn exited(&self, token: &[u8; 20]) -> bool {
        self.exited.contains(token)
    }

    /// An approval of `token` for `amount` the chain included before this
    /// arm (BSC3-048): the exit sells without approving again.
    pub fn adopt_approval(&mut self, token: [u8; 20], amount: U256) {
        if !self.approved.iter().any(|(t, _)| *t == token) {
            self.approved.push((token, amount));
        }
    }

    /// Another transaction took the nonce of this arm's exit transaction
    /// of `token` (BSC3-048): it never happened, the exit is open again.
    /// A replaced approval approved nothing.
    pub fn reopen_exit(&mut self, token: [u8; 20], approval: bool) {
        self.exited.retain(|t| *t != token);
        if approval {
            self.approved.retain(|(t, _)| *t != token);
        }
    }

    /// Tokens whose exit's approval went and whose SELL did not (a chain
    /// ordered end's SELL waits for its approval to be canonical).
    pub fn exits_awaiting_sell(&self) -> Vec<[u8; 20]> {
        self.approved
            .iter()
            .map(|(t, _)| *t)
            .filter(|t| !self.exited.contains(t))
            .collect()
    }

    /// The next strategy lane: its nonce is the next of the PAPER
    /// sender's sequence (each lane one nonce above the last). Refused
    /// once a taken nonce was not handed off (a hole no later PAPER
    /// transaction could pass on chain). An end whose transactions reach
    /// the chain takes it only at the sender's canonical next nonce
    /// `canonical`; refused otherwise with nothing taken (BSC3-055).
    fn next_strategy_lane(&mut self, canonical: Option<u64>) -> Result<usize, Refusal> {
        if self.strategy_nonce_gap {
            return Err(Refusal::NonceGap);
        }
        if E::CHAIN_ORDERED {
            let next = self.strategy_next_nonce();
            let lane_left = next - self.strategy_first_nonce < self.strategy_lanes.len() as u64;
            if lane_left && canonical != Some(next) {
                return Err(Refusal::SenderNonce);
            }
        }
        if self.strategy_used == 0 && self.c3_took_nonce {
            self.strategy_used = 1;
            self.strategy_skipped_first = true;
        }
        if self.strategy_used >= self.strategy_lanes.len() {
            return Err(Refusal::LanesExhausted);
        }
        self.strategy_used += 1;
        Ok(self.strategy_used - 1)
    }

    /// A strategy intent refused with the lanes from `from` taken (gate
    /// N1). Nothing of it disclosed (no permit's write started): their
    /// nonces are free again, the lanes re-armed fresh behind the same
    /// fence. Its bytes may be out, or a lane broke: a hole, the
    /// strategy's intents stop.
    fn strategy_refused(&mut self, from: usize, disclosed: bool) {
        if disclosed {
            self.strategy_nonce_gap = true;
            return;
        }
        let chain_id = self.deployment.deployment().chain_id;
        let sender = Address::from(self.end.address());
        for index in from..self.strategy_used {
            let nonce = self.strategy_first_nonce + index as u64;
            match fenced_lane(
                chain_id,
                sender,
                nonce,
                self.config.strategy_session_cap_wei,
            ) {
                Ok(lane) => self.strategy_lanes[index] = lane,
                Err(_) => {
                    self.strategy_nonce_gap = true;
                    return;
                }
            }
        }
        self.strategy_used = from;
    }

    #[allow(clippy::too_many_arguments)]
    fn strategy_buy(
        &mut self,
        token: [u8; 20],
        curve: StrategyCurve,
        input_wei: U256,
        c2: &CanonicalContext<'_>,
        clock: &dyn Clock,
        decided_ns: u64,
        start_ns: u64,
    ) -> Result<ExecIntent<E::Handoff>, Refusal> {
        if !self.config.sender_paths.strategy() {
            return Err(Refusal::PathOff);
        }
        let policy = BuyPolicy::new(
            self.config.policy_generation,
            input_wei,
            self.config.slippage_bps,
            self.config.gas_price_wei,
            self.config.gas_limit,
        )
        .map_err(|_| Refusal::Admission)?;
        let snapshot = (c2.snapshot)(
            self.config.policy_generation,
            self.deployment.codehash_generation(),
        )
        .ok_or(Refusal::NoSnapshot)?;
        let head = c2.fresh_head(self.config.strategy_max_head_age_ms)?;
        let state = CurveState::tracked(
            Address::from(token),
            curve.terms,
            curve.reserve,
            curve.circulating,
            &self.terms,
            snapshot,
        )
        .map_err(|_| Refusal::Admission)?;
        let admitted = admit_curve_buy(&state, &policy).map_err(|_| Refusal::Admission)?;
        let admitted_ns = clock.now_ns();
        if self.holds(&token) {
            return Err(Refusal::DuplicateToken);
        }
        let gas = U256::from(self.config.gas_price_wei) * U256::from(self.config.gas_limit);
        let budget = BudgetEnvelope {
            max_buy_input_wei: input_wei,
            max_buy_gas_wei: gas,
            max_own_launch_wei: U256::ZERO,
            max_provider_payment_wei: U256::ZERO,
            max_total_wei: input_wei + gas,
        };
        if self.strategy_spent_wei + budget.max_total_wei
            > U256::from(self.config.strategy_session_cap_wei)
        {
            return Err(Refusal::SessionCap);
        }
        // A chain ordered end keeps a lane for every SELL its approval
        // waits for.
        if E::CHAIN_ORDERED {
            let taken = self.strategy_next_nonce() - self.strategy_first_nonce;
            let left = (self.strategy_lanes.len() as u64).saturating_sub(taken);
            if left <= self.exits_awaiting_sell().len() as u64 {
                return Err(Refusal::LanesExhausted);
            }
        }
        let index = self.next_strategy_lane(c2.next_nonce)?;
        let Ok(ticket) = self.strategy_lanes[index].reserve(admitted.token()) else {
            self.strategy_refused(index, true);
            return Err(Refusal::Lane);
        };
        let sender = Address::from(self.end.address());
        let built = ExpectedBuy::from_admitted(
            &self.deployment,
            &admitted,
            &policy,
            sender,
            ticket.nonce(),
        )
        .map_err(|_| Refusal::Intent)
        .and_then(|expected| {
            expected
                .sign(self.end.signer(), &mut self.scratch, &mut self.raw)
                .map(|_| expected)
                .map_err(|_| Refusal::Build)
        });
        let expected = match built {
            Ok(e) => e,
            Err(r) => {
                self.strategy_refused(index, false);
                return Err(r);
            }
        };
        let signed_ns = clock.now_ns();
        let (handoff, prepared_ns, planned_ns, (_, disclosed)) = to_end(
            Parts {
                end: &mut self.end,
                lane: &mut self.strategy_lanes[index],
                config: &self.config,
                scratch: &mut self.scratch,
            },
            ticket,
            &self.raw,
            ExpectedTx::Buy(&expected),
            &budget,
            &snapshot,
            head,
            clock,
            start_ns,
            Submission::Single,
        );
        // Gate L2: the exposure counts once the BUY may be out; a nonce
        // taken again (nothing disclosed) spends nothing.
        if disclosed {
            self.strategy_spent_wei += budget.max_total_wei;
        }
        let handoff = match handoff {
            Ok(h) => h,
            Err(r) => {
                self.strategy_refused(index, disclosed);
                return Err(r);
            }
        };
        self.strategy_bought.push(Held {
            token,
            min_tokens: admitted.min_output_amount(),
            input_wei,
        });
        Ok(ExecIntent {
            kind: IntentKind::Buy,
            token,
            block: head.number,
            handoff,
            times: StageTimes {
                ingress_ns: decided_ns,
                decoded_ns: start_ns,
                admitted_ns,
                signed_ns,
                prepared_ns,
                planned_ns,
                sink_ns: handoff.at_ns(),
            },
            raw: self.raw.clone(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn strategy_exit(
        &mut self,
        token: [u8; 20],
        tokens: U256,
        curve: StrategyCurve,
        c2: &CanonicalContext<'_>,
        clock: &dyn Clock,
        triggered_ns: u64,
        start_ns: u64,
        out: &mut Vec<ExecIntent<E::Handoff>>,
    ) -> Result<(), Refusal> {
        if !self.config.sender_paths.strategy() {
            return Err(Refusal::PathOff);
        }
        let held = self
            .strategy_bought
            .iter()
            .find(|h| h.token == token)
            .copied()
            .ok_or(Refusal::NotHeld)?;
        if self.exited.contains(&token) {
            return Err(Refusal::DuplicateToken);
        }
        // Gate L2: an approval already out (its SELL refused before) is not
        // signed again; the SELL sells at most what it approved.
        let approved = self
            .approved
            .iter()
            .find(|(t, _)| *t == token)
            .map(|(_, amount)| *amount);
        let snapshot = (c2.snapshot)(
            self.config.policy_generation,
            self.deployment.codehash_generation(),
        )
        .ok_or(Refusal::NoSnapshot)?;
        let head = c2.fresh_head(self.config.strategy_max_head_age_ms)?;
        let state = CurveState::tracked(
            Address::from(token),
            curve.terms,
            curve.reserve,
            curve.circulating,
            &self.terms,
            snapshot,
        )
        .map_err(|_| Refusal::Admission)?;
        // At most what the handed-off BUY is bound to have bought.
        let amount = tokens.min(held.min_tokens);
        let amount = approved.map_or(amount, |a| amount.min(a));
        let admitted =
            admit_curve_sell(&state, amount, &self.exit_policy).map_err(|_| Refusal::Admission)?;
        let admitted_ns = clock.now_ns();
        let price = U256::from(self.config.gas_price_wei);
        let approve_gas = if approved.is_some() {
            U256::ZERO
        } else {
            price * U256::from(self.config.approve_gas_limit)
        };
        let sell_gas = price * U256::from(self.config.sell_gas_limit);
        let budget = |gas: U256| BudgetEnvelope {
            max_buy_input_wei: U256::ZERO,
            max_buy_gas_wei: gas,
            max_own_launch_wei: U256::ZERO,
            max_provider_payment_wei: U256::ZERO,
            max_total_wei: gas,
        };
        if self.strategy_spent_wei + approve_gas + sell_gas
            > U256::from(self.config.strategy_session_cap_wei)
        {
            return Err(Refusal::SessionCap);
        }
        // BSC3-055: a chain ordered end's SELL waits for its approval to be
        // canonical; it comes back as an exit with the approval out.
        let defer_sell = E::CHAIN_ORDERED && approved.is_none();
        let needed = if approved.is_some() { 1 } else { 2 };
        if self.strategy_lanes.len() - self.strategy_used < needed {
            return Err(Refusal::LanesExhausted);
        }
        let approve_index = match approved {
            Some(_) => None,
            None => Some(self.next_strategy_lane(c2.next_nonce)?),
        };
        let sell_index = match defer_sell {
            true => None,
            false => Some(self.next_strategy_lane(c2.next_nonce)?),
        };
        let Some(first) = approve_index.or(sell_index) else {
            return Err(Refusal::Lane);
        };
        let mut tickets = [None, None];
        for (slot, index) in [approve_index, sell_index].into_iter().enumerate() {
            let Some(index) = index else { continue };
            match self.strategy_lanes[index].reserve(admitted.token()) {
                Ok(t) => tickets[slot] = Some(t),
                Err(_) => {
                    self.strategy_refused(first, true);
                    return Err(Refusal::Lane);
                }
            }
        }
        let [approve_ticket, sell_ticket] = tickets;
        // The approval at n, the SELL at n+1 (a SELL alone: n is its own
        // nonce minus one, the approval already out; an approval alone: the
        // SELL comes at n+1 later).
        let base = match (approve_ticket, sell_ticket) {
            (Some(a), Some(s)) if s.nonce() == a.nonce() + 1 => Some(a.nonce()),
            (Some(a), None) => Some(a.nonce()),
            (None, Some(s)) => s.nonce().checked_sub(1),
            _ => None,
        };
        let Some(base) = base else {
            self.strategy_refused(first, true);
            return Err(Refusal::Intent);
        };
        let sender = Address::from(self.end.address());
        let Ok(expected) = ExpectedExit::from_admitted(
            &self.deployment,
            &admitted,
            &self.exit_policy,
            sender,
            base,
        ) else {
            self.strategy_refused(first, false);
            return Err(Refusal::Intent);
        };
        let steps = [
            approve_index
                .zip(approve_ticket)
                .map(|(i, t)| (IntentKind::Approve, i, t, approve_gas)),
            sell_index
                .zip(sell_ticket)
                .map(|(i, t)| (IntentKind::Sell, i, t, sell_gas)),
        ];
        for (kind, index, ticket, gas) in steps.into_iter().flatten() {
            let built = match kind {
                IntentKind::Approve => {
                    expected.sign_approve(self.end.signer(), &mut self.scratch, &mut self.raw)
                }
                _ => expected.sign_sell(self.end.signer(), &mut self.scratch, &mut self.raw),
            };
            if built.is_err() {
                self.strategy_refused(index, false);
                return Err(Refusal::Build);
            }
            let signed_ns = clock.now_ns();
            let tx = match kind {
                IntentKind::Approve => ExpectedTx::Approve(&expected),
                _ => ExpectedTx::Sell(&expected),
            };
            let (handoff, prepared_ns, planned_ns, (_, disclosed)) = to_end(
                Parts {
                    end: &mut self.end,
                    lane: &mut self.strategy_lanes[index],
                    config: &self.config,
                    scratch: &mut self.scratch,
                },
                ticket,
                &self.raw,
                tx,
                &budget(gas),
                &snapshot,
                head,
                clock,
                start_ns,
                Submission::Single,
            );
            // Gate L2: gas counts once something may be out; a nonce taken
            // again spends nothing.
            if disclosed {
                self.strategy_spent_wei += gas;
            }
            let handoff = match handoff {
                Ok(h) => h,
                Err(r) => {
                    self.strategy_refused(index, disclosed);
                    // A SELL that may be out ends the position's exits.
                    if kind == IntentKind::Sell && disclosed {
                        self.exited.push(token);
                    }
                    return Err(r);
                }
            };
            if kind == IntentKind::Approve {
                self.approved.push((token, admitted.input_tokens()));
            }
            out.push(ExecIntent {
                kind,
                token,
                block: head.number,
                handoff,
                times: StageTimes {
                    ingress_ns: triggered_ns,
                    decoded_ns: start_ns,
                    admitted_ns,
                    signed_ns,
                    prepared_ns,
                    planned_ns,
                    sink_ns: handoff.at_ns(),
                },
                raw: self.raw.clone(),
            });
        }
        if defer_sell {
            // The approval is out; its SELL waits for it to be canonical
            // (the exit again then hands it over).
            return Ok(());
        }
        self.exited.push(token);
        // The position is closed: its input leaves the open exposure, the
        // gas spent stays (gate L1).
        self.strategy_spent_wei = self.strategy_spent_wei.saturating_sub(held.input_wei);
        Ok(())
    }

    /// Applies C2's evidence about one of the sender's nonces to the lanes
    /// that hold that nonce (plan 28.10.): every C3 lane and the first
    /// strategy lane at the arming's nonce, strategy lane `i` at its own.
    /// An inclusion of the intent's own transaction makes it `Included` or
    /// `Reverted`, another transaction `Replaced` (the lane stops); an
    /// orphan rewinds and stops it. Returns the state an inclusion gave the
    /// intent that was dispatched, if one was.
    pub fn apply_evidence(&mut self, evidence: NonceEvidence<'_>) -> Option<IntentState> {
        let sender = Address::from(self.end.address());
        let (proof_sender, nonce) = match evidence {
            NonceEvidence::Inclusion(p) => (p.sender(), p.nonce()),
            NonceEvidence::Finality(p) => (p.sender(), p.nonce()),
            NonceEvidence::Orphan(p) => (p.sender(), p.nonce()),
        };
        if proof_sender != sender {
            return None;
        }
        let strategy = nonce
            .checked_sub(self.strategy_first_nonce)
            .and_then(|i| usize::try_from(i).ok())
            .filter(|i| *i < self.strategy_lanes.len());
        let c3 = nonce == self.first_nonce;
        if strategy.is_none() && !c3 {
            if nonce >= self.first_nonce {
                self.evidence.unassigned += 1;
            }
            return None;
        }
        let lanes = self
            .lanes
            .iter_mut()
            .filter(|_| c3)
            .chain(strategy.and_then(|i| self.strategy_lanes.get_mut(i)));
        let mut ours = None;
        let mut applied = false;
        for lane in lanes {
            match evidence {
                NonceEvidence::Inclusion(p) => match lane.apply_inclusion(p) {
                    Ok(s @ (IntentState::Included | IntentState::Reverted)) => ours = Some(s),
                    Ok(s) => {
                        applied = true;
                        ours = ours.or(Some(s));
                    }
                    Err(_) => {}
                },
                NonceEvidence::Finality(p) => applied |= lane.apply_finality(p).is_ok(),
                NonceEvidence::Orphan(p) => applied |= lane.apply_orphan(p).is_ok(),
            }
        }
        let e = &mut self.evidence;
        match (evidence, ours) {
            (NonceEvidence::Inclusion(_), Some(IntentState::Included)) => e.included += 1,
            (NonceEvidence::Inclusion(_), Some(IntentState::Reverted)) => e.reverted += 1,
            (NonceEvidence::Inclusion(_), _) if applied => e.replaced += 1,
            (NonceEvidence::Finality(_), _) if applied => e.finalized += 1,
            (NonceEvidence::Orphan(_), _) if applied => e.orphaned += 1,
            _ => {}
        }
        ours
    }

    /// What C2's evidence did to this arm's intents.
    pub fn evidence(&self) -> EvidenceCounters {
        self.evidence
    }

    /// Plan 28.10.: a real position the chain confirmed, `tokens` received
    /// for `input_wei`: its exit sells at most these tokens, and a C3
    /// position (or one from before a restart) becomes exitable the same
    /// way as a Strategy C one.
    pub fn adopt(&mut self, token: [u8; 20], tokens: U256, input_wei: U256) {
        match self.strategy_bought.iter_mut().find(|h| h.token == token) {
            Some(h) => h.min_tokens = tokens,
            None => self.strategy_bought.push(Held {
                token,
                min_tokens: tokens,
                input_wei,
            }),
        }
    }

    /// Strategy C's PAPER counters (Phase I): intents asked for, handed
    /// off, refused by stage.
    pub fn strategy_counters(&self) -> &ExecCounters {
        &self.strategy_counters
    }

    /// Strategy lanes armed and left, the next strategy nonce, the open
    /// exposure plus the gas spent, and whether a nonce hole stopped the
    /// strategy's intents.
    pub fn strategy_lanes(&self) -> (usize, usize, u64, U256, bool) {
        (
            self.strategy_lanes.len(),
            self.strategy_lanes.len() - self.strategy_used,
            self.strategy_first_nonce + self.strategy_used as u64,
            self.strategy_spent_wei,
            self.strategy_nonce_gap,
        )
    }

    pub fn sender(&self) -> [u8; 20] {
        self.end.address()
    }

    /// The end the path hands over to.
    pub fn end(&self) -> &E {
        &self.end
    }

    /// The end, for its maintenance outside any hand-over.
    pub fn end_mut(&mut self) -> &mut E {
        &mut self.end
    }

    pub fn counters(&self) -> &ExecCounters {
        &self.counters
    }

    pub fn latencies(&self) -> &Latencies {
        &self.latencies
    }

    /// Lanes armed and lanes left.
    pub fn lanes(&self) -> (usize, usize) {
        (self.lanes.len(), self.lanes.len() - self.used)
    }

    /// Exposure dispatched in this session.
    pub fn spent_wei(&self) -> U256 {
        self.spent_wei
    }

    /// No further BUY can be admitted: every lane is used, or the session
    /// cap has no room for one more BUY.
    pub fn exhausted(&self) -> bool {
        self.used == self.lanes.len()
            || self.spent_wei + self.budget.max_total_wei > U256::from(self.config.session_cap_wei)
    }

    pub fn fence(&self) -> &Path {
        &self.fence
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use alloy_primitives::{hex, keccak256};
    use darkbot_bsc_trading::economics::flap::native_curve20::WITNESSED_MAINNET_TERMS;
    use darkbot_bsc_trading::signer::Signer;
    use darkbot_bsc_trading::transactions::legacy::{LegacyTx, decode_signed};
    use darkbot_bsc_trading::venues::flap::abi::{
        LaunchParamsV6, decode_swap_exact_input_native, encode_new_token_v6,
    };
    use darkbot_bsc_trading::venues::flap::deployment::{ObservedDeployment, bsc_mainnet_v5_27_0};
    use std::cell::Cell;

    const POLICY_GENERATION: u64 = 3;
    const CODEHASH_GENERATION: u64 = 5;
    const CLOCK_ID: u32 = 1;

    pub(crate) fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bsc-paper-exec-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    /// The current deployment (economics reconfirmed on it).
    pub(crate) fn validated() -> ValidatedDeployment {
        let d = bsc_mainnet_v5_27_0();
        d.validate(
            &ObservedDeployment {
                chain_id: d.chain_id,
                portal_version: d.portal_version,
                portal_runtime_sha256: d.portal_runtime_sha256,
                portal_implementation: d.portal_implementation,
                portal_implementation_runtime_sha256: d.portal_implementation_runtime_sha256,
                tax_v3_implementation_runtime_sha256: d.tax_v3_implementation_runtime_sha256,
            },
            CODEHASH_GENERATION,
        )
        .unwrap()
    }

    pub(crate) fn config(lanes: usize, buys: u128) -> ExecConfig {
        let gas_price_wei = 1_000_000_000;
        let gas_limit = 300_000;
        let one = 1_000_000_000_000_000 + u128::from(gas_price_wei) * u128::from(gas_limit);
        ExecConfig {
            // The amount covered by witnessed evidence.
            input_wei: 1_000_000_000_000_000,
            slippage_bps: 500,
            exit_slippage_bps: 500,
            gas_price_wei,
            gas_limit,
            session_cap_wei: one * buys,
            lanes,
            max_calldata: 4_096,
            latency_samples: 64,
            policy_generation: POLICY_GENERATION,
            capability_generation: 2,
            validity_blocks: 2,
            start_limit_ns: 1_000_000,
            max_frame: 16_384,
            strategy_lanes: lanes,
            strategy_session_cap_wei: 100_000_000_000_000_000,
            approve_gas_limit: 100_000,
            sell_gas_limit: 1_500_000,
            strategy_max_head_age_ms: 30_000,
            sender_paths: SenderPaths::Both,
        }
    }

    /// The witnessed native self-launch (the one qualified profile) with
    /// `salt`.
    fn calldata(salt: u8) -> Vec<u8> {
        let mut beneficiary = [0u8; 20];
        beneficiary[18] = 0xa1;
        beneficiary[19] = 0x1c;
        let mut s = hex!("09c4b20c7956530d4bf91b3569ed173abfce47d16d412d94dde5e6f4034c6eeb");
        s[31] ^= salt;
        let launch = LaunchParamsV6 {
            name: b"DarkBot G06A Economics",
            symbol: b"DBG06E",
            uri: b"ipfs://darkbot-g06a-economics",
            dex_thresh: 1,
            salt: B256::from(s),
            migrator_type: 1,
            quote_token: Address::ZERO,
            quote_amt: U256::ZERO,
            beneficiary: Address::from(beneficiary),
            permit: &[],
            extension_id: B256::ZERO,
            extension_data: &[],
            dex_id: 0,
            lp_fee_profile: 0,
            buy_tax: 100,
            sell_tax: 100,
            tax_duration: 31_536_000,
            anti_farmer_duration: 86_400,
            mkt_bps: 10_000,
            deflation_bps: 0,
            dividend_bps: 0,
            lp_bps: 0,
            minimum_share_balance: U256::ZERO,
            dividend_token: Address::ZERO,
            commission_receiver: Address::ZERO,
            token_version: 6,
        };
        let mut out = Vec::new();
        encode_new_token_v6(&launch, &mut out);
        out
    }

    /// A launch really signed by a third party: (hash, signed bytes, input).
    pub(crate) fn launch(salt: u8, nonce: u64) -> ([u8; 32], Vec<u8>, Vec<u8>) {
        let launcher = Signer::from_secret_bytes(&[0x5a; 32]).unwrap();
        let input = calldata(salt);
        let (mut scratch, mut raw) = (Vec::new(), Vec::new());
        launcher
            .sign_legacy(
                &LegacyTx {
                    chain_id: 56,
                    nonce,
                    gas_price: 1_000_000_000,
                    gas_limit: 6_000_000,
                    to: bsc_mainnet_v5_27_0().portal,
                    value: U256::ZERO,
                    input: &input,
                },
                &mut scratch,
                &mut raw,
            )
            .unwrap();
        (keccak256(&raw).0, raw, input)
    }

    fn snapshot(candidate: B256, policy: u64, codehash: u64) -> Option<SnapshotId> {
        Some(SnapshotId {
            canonical_block_hash: B256::repeat_byte(0xb1),
            ordered_state_cursor: 9,
            pending_candidate: candidate,
            policy_generation: policy,
            codehash_generation: codehash,
        })
    }

    pub(crate) fn c2(head: Option<ChainHead>) -> LaunchContext<'static> {
        LaunchContext {
            snapshot: &snapshot,
            head,
            next_nonce: None,
        }
    }

    /// [`c2`] with the sender's canonical next nonce `next` (a LIVE end).
    pub(crate) fn c2_next(head: Option<ChainHead>, next: u64) -> LaunchContext<'static> {
        LaunchContext {
            next_nonce: Some(next),
            ..c2(head)
        }
    }

    pub(crate) const HEAD: ChainHead = ChainHead {
        number: 129_900_000,
        timestamp: 1_793_000_000,
    };

    fn armed(name: &str, config: ExecConfig) -> (PaperExecutor, PathBuf) {
        let d = dir(name);
        let key = d.join("identity/paper.key");
        PaperIdentity::create(&key, Path::new("/nonexistent-repo")).unwrap();
        let identity = PaperIdentity::load(&key, Path::new("/nonexistent-repo"), &[]).unwrap();
        let exec = PaperExecutor::arm(
            identity,
            validated(),
            WITNESSED_MAINNET_TERMS,
            config,
            Arming {
                next_nonce: 0,
                fence_dir: &d.join("fences"),
                session: 1,
                arm: 0,
            },
        )
        .unwrap();
        (exec, d)
    }

    pub(crate) fn clock(start: u64, step: u64) -> FnClock<impl Fn() -> u64> {
        let now = Cell::new(start);
        FnClock {
            id: CLOCK_ID,
            now: move || {
                now.set(now.get() + step);
                now.get()
            },
        }
    }

    /// C3 (gate review H1): a really signed launch runs the whole production
    /// path: decoder, C2 snapshot, admission, BUY build and signature, lane,
    /// the submission plan of the 48Club bundle with the original launch,
    /// its request frame and the diagnostic dispatch, whose transport is the
    /// sink. The BUY is the admitted one; the sink took the whole request.
    #[test]
    fn a_signed_launch_reaches_the_sink_through_plan_frame_and_dispatch() {
        let (mut exec, d) = armed("path", config(2, 2));
        let portal = bsc_mainnet_v5_27_0().portal.into_array();
        let (hash, raw, input) = launch(0, 7);
        let buy = exec
            .on_launch(
                PendingLaunch {
                    hash,
                    raw: &raw,
                    to: Some(portal),
                    input: &input,
                    ingress_ns: 100,
                },
                &c2(Some(HEAD)),
                &clock(100, 7),
            )
            .unwrap();
        let signed = decode_signed(&buy.raw).unwrap();
        assert_eq!(signed.tx.chain_id, 56);
        assert_eq!(signed.tx.nonce, 0);
        assert_eq!(signed.tx.to.into_array(), portal);
        assert_eq!(signed.tx.value, U256::from(1_000_000_000_000_000u64));
        let call = decode_swap_exact_input_native(signed.tx.input).unwrap();
        // CREATE2 of the witnessed salt (G0.6a EVM crosscheck token).
        assert_eq!(
            call.output_token.into_array(),
            hex!("e51c4deb1038afcd95ef12d4a649cdbcf6fa7777")
        );
        assert_eq!(buy.token, call.output_token.into_array());
        // The request carries both transactions hex-encoded, and a head.
        assert!(buy.handoff.raw_len > 2 * (raw.len() + buy.raw.len()));
        let t = buy.times;
        assert!(t.ingress_ns < t.decoded_ns && t.decoded_ns < t.admitted_ns);
        assert!(t.admitted_ns < t.signed_ns && t.signed_ns < t.prepared_ns);
        assert!(t.prepared_ns < t.planned_ns);
        assert!(t.planned_ns < t.sink_ns);
        assert_eq!(exec.counters().handoffs, 1);
        assert_eq!(exec.lanes(), (2, 1));
        assert_eq!(exec.spent_wei(), exec.budget.max_total_wei);
        assert_eq!(
            exec.latencies().quantiles().map(|q| q.2),
            Some(t.sink_ns - 100)
        );
        // One fence for the arm, removed when it is disarmed.
        let fence = exec.fence().to_path_buf();
        assert!(fence.exists());
        exec.disarm().unwrap();
        assert!(!fence.exists());
        fs::remove_dir_all(&d).unwrap();
    }

    /// Gate review M1: the pool keeps one session exposure and one BUY per
    /// token, as one production session does (before: every lane its own
    /// cap and dedup, so a second BUY of the same token reached the sink).
    #[test]
    fn the_pool_keeps_one_session_cap_and_one_buy_per_token() {
        let portal = bsc_mainnet_v5_27_0().portal.into_array();
        let (mut exec, d) = armed("session", config(4, 2));
        let (h1, r1, i1) = launch(0, 1);
        let (h2, r2, i2) = launch(0, 2); // another launch of the same token
        let (h3, r3, i3) = launch(1, 3);
        let (h4, r4, i4) = launch(2, 4);
        let ok = |e: &mut PaperExecutor, h, r: &[u8], i: &[u8]| {
            e.on_launch(
                PendingLaunch {
                    hash: h,
                    raw: r,
                    to: Some(portal),
                    input: i,
                    ingress_ns: 100,
                },
                &c2(Some(HEAD)),
                &clock(100, 1),
            )
        };
        assert!(ok(&mut exec, h1, &r1, &i1).is_ok());
        assert_eq!(ok(&mut exec, h2, &r2, &i2), Err(Refusal::DuplicateToken));
        assert!(ok(&mut exec, h3, &r3, &i3).is_ok());
        // Two BUYs exhaust a cap of two, lanes left or not.
        assert_eq!(ok(&mut exec, h4, &r4, &i4), Err(Refusal::SessionCap));
        assert_eq!(exec.lanes(), (4, 2));
        let c = exec.counters();
        assert_eq!(
            (
                c.handoffs,
                c.refused("DUPLICATE_TOKEN"),
                c.refused("SESSION_CAP")
            ),
            (2, 1, 1)
        );
        exec.disarm().unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    /// Every other outcome is a counted refusal at its stage.
    #[test]
    fn refusals_are_counted_at_their_stage() {
        let (mut exec, d) = armed("refuse", config(2, 8));
        let portal = bsc_mainnet_v5_27_0().portal.into_array();
        let (hash, raw, input) = launch(0, 1);
        let ctx = c2(Some(HEAD));
        let clk = clock(100, 1);
        assert_eq!(
            exec.on_launch(
                PendingLaunch {
                    hash,
                    raw: &raw,
                    to: Some([7; 20]),
                    input: &input,
                    ingress_ns: 100
                },
                &ctx,
                &clk
            ),
            Err(Refusal::NotDecoded)
        );
        let foreign = hex::decode(
            include_str!("../../trading/fixtures/flap/launch_v6_native_split_tax_pending.hex")
                .trim(),
        )
        .unwrap();
        assert!(matches!(
            exec.on_launch(
                PendingLaunch {
                    hash,
                    raw: &raw,
                    to: Some(portal),
                    input: &foreign,
                    ingress_ns: 100
                },
                &ctx,
                &clk
            ),
            Err(Refusal::Profile(_))
        ));
        let none = LaunchContext {
            snapshot: &|_, _, _| None,
            head: Some(HEAD),
            next_nonce: None,
        };
        assert_eq!(
            exec.on_launch(
                PendingLaunch {
                    hash,
                    raw: &raw,
                    to: Some(portal),
                    input: &input,
                    ingress_ns: 100
                },
                &none,
                &clk
            ),
            Err(Refusal::NoSnapshot)
        );
        assert_eq!(
            exec.on_launch(
                PendingLaunch {
                    hash,
                    raw: &raw,
                    to: Some(portal),
                    input: &input,
                    ingress_ns: 100
                },
                &c2(None),
                &clk
            ),
            Err(Refusal::NoSnapshot)
        );
        // The bundle's launch must be the candidate the BUY was admitted
        // behind: other signed bytes are refused by the plan (one lane used).
        let (_, other_raw, _) = launch(0, 9);
        assert_eq!(
            exec.on_launch(
                PendingLaunch {
                    hash,
                    raw: &other_raw,
                    to: Some(portal),
                    input: &input,
                    ingress_ns: 100
                },
                &ctx,
                &clk
            ),
            Err(Refusal::Plan)
        );
        // The start deadline after T_ingress passed: no permit, no exposure.
        let late = clock(100 + 2_000_000, 1);
        let (h2, r2, i2) = launch(1, 2);
        assert_eq!(
            exec.on_launch(
                PendingLaunch {
                    hash: h2,
                    raw: &r2,
                    to: Some(portal),
                    input: &i2,
                    ingress_ns: 100
                },
                &ctx,
                &late
            ),
            Err(Refusal::Deadline)
        );
        assert_eq!(exec.spent_wei(), U256::ZERO);
        let (h3, r3, i3) = launch(2, 3);
        assert_eq!(
            exec.on_launch(
                PendingLaunch {
                    hash: h3,
                    raw: &r3,
                    to: Some(portal),
                    input: &i3,
                    ingress_ns: 100
                },
                &ctx,
                &clk
            ),
            Err(Refusal::LanesExhausted)
        );
        exec.refuse_undecodable();
        let c = exec.counters();
        assert_eq!((c.launches, c.handoffs), (8, 0));
        for (stage, n) in [
            ("NOT_DECODED", 2),
            ("PROFILE", 1),
            ("NO_SNAPSHOT", 2),
            ("PLAN", 1),
            ("DEADLINE", 1),
            ("LANES_EXHAUSTED", 1),
        ] {
            assert_eq!(c.refused(stage), n, "{stage}");
        }
        exec.disarm().unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    /// V2 §7.4 and gate review L7: the modules that hold signed PAPER bytes
    /// (the executor, the runtime controller, the sink) name no socket,
    /// stream, TLS connection, release dispatch or provider client; the
    /// executor's only transport is the sink. The controller's probe thread
    /// is given the endpoint, the bounds and the sender address only.
    #[test]
    fn no_module_with_paper_bytes_reaches_a_transport() {
        let code = |source: &'static str| &source[..source.find("#[cfg(test)]").unwrap()];
        let common = [
            "TcpStream",
            "UdpSocket",
            "UnixStream",
            "std::net",
            "tungstenite",
            "rustls",
            "Command::new",
            "WarmTlsConnection",
            "TlsConnectionOwner",
            "TlsTransport",
            "qualify_transport",
            "QualifiedTransport",
            "DiagnosticStreamTransport",
            "dispatch_release",
            "rpc_wire",
            "crate::wss",
        ];
        let executor = code(include_str!("paper_exec.rs"));
        let controller = code(include_str!("runtime_exec.rs"));
        let sink = code(include_str!("paper.rs"));
        for (name, source) in [
            ("paper_exec", executor),
            ("runtime_exec", controller),
            ("paper", sink),
        ] {
            for forbidden in common {
                assert!(!source.contains(forbidden), "{name}: {forbidden}");
            }
        }
        // The executor implements exactly one transport: the sink.
        assert_eq!(executor.matches("impl Transport for").count(), 1);
        assert!(executor.contains("impl Transport for SinkTransport"));
        // The controller dispatches nothing and its probe thread sees no
        // signed bytes.
        assert!(!controller.contains("crate::submission::send"));
        let probe = &controller[controller.find("fn spawn_probe").unwrap()..];
        let probe = &probe[..probe.find("\n    }\n").unwrap()];
        for forbidden in ["raw", "PaperBuy", "executor", "signer"] {
            assert!(!probe.contains(forbidden), "probe thread: {forbidden}");
        }
    }

    /// Gate review L3: keys that could never admit a BUY are refused at
    /// arming (before: the path showed ARMED and refused every launch).
    #[test]
    fn a_configuration_that_cannot_admit_is_refused_at_arming() {
        let good = config(1, 1);
        assert_eq!(good.validate(), Ok(()));
        for bad in [
            ExecConfig {
                input_wei: 2_000_000_000_000_000,
                ..good.clone()
            },
            ExecConfig {
                session_cap_wei: good.session_cap_wei - 1,
                ..good.clone()
            },
            ExecConfig {
                validity_blocks: PAPER_PROFILE.max_block_window() + 1,
                ..good.clone()
            },
            ExecConfig {
                validity_blocks: 0,
                ..good.clone()
            },
            ExecConfig {
                start_limit_ns: PAPER_PROFILE.max_transport_start_limit_ns() + 1,
                ..good.clone()
            },
            ExecConfig {
                lanes: 0,
                ..good.clone()
            },
        ] {
            assert!(bad.validate().is_err(), "{bad:?}");
        }
    }

    /// Gate review M2: every arm has its own fence, so the component can be
    /// armed again in the same session (before: `<session>-<lane>` collided
    /// and re-arming failed with "File exists").
    #[test]
    fn the_same_session_arms_again_with_a_new_fence() {
        let (exec, d) = armed("rearm", config(1, 1));
        let key = d.join("identity/paper.key");
        let load = || PaperIdentity::load(&key, Path::new("/nonexistent-repo"), &[]).unwrap();
        let arm = |n| {
            PaperExecutor::arm(
                load(),
                validated(),
                WITNESSED_MAINNET_TERMS,
                config(1, 1),
                Arming {
                    next_nonce: 0,
                    fence_dir: &d.join("fences"),
                    session: 1,
                    arm: n,
                },
            )
        };
        // The same arm twice is refused; the next arm is not.
        assert!(matches!(arm(0), Err(ArmError::Fence(_))));
        exec.disarm().unwrap();
        let again = arm(1).unwrap();
        assert_eq!(fs::read_dir(d.join("fences")).unwrap().count(), 1);
        again.disarm().unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    /// The canonical snapshot of C2 (Phase I: no pending candidate).
    fn canonical(policy: u64, codehash: u64) -> Option<SnapshotId> {
        snapshot(B256::ZERO, policy, codehash)
    }

    /// Now: one second after the test head.
    const NOW_MS: u64 = 1_793_000_001_000;

    pub(crate) fn strategy_c2(head: Option<ChainHead>) -> CanonicalContext<'static> {
        CanonicalContext {
            snapshot: &canonical,
            head,
            now_unix_ms: NOW_MS,
            next_nonce: None,
        }
    }

    /// [`strategy_c2`] with the sender's canonical next nonce `next` (a
    /// LIVE end).
    pub(crate) fn strategy_c2_next(
        head: Option<ChainHead>,
        next: u64,
    ) -> CanonicalContext<'static> {
        CanonicalContext {
            next_nonce: Some(next),
            ..strategy_c2(head)
        }
    }

    /// A curve after real BUYs of `wei` from the fresh curve, and the
    /// quote of a BUY of `input` on it.
    pub(crate) fn curve_after(wei: u128) -> StrategyCurve {
        use darkbot_bsc_trading::economics::flap::native_curve20::{
            HoldingCap, NativeBuyInputs, profile_terms, quote_native_buy,
        };
        let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
        let q = quote_native_buy(&NativeBuyInputs {
            token: terms,
            reserve_before: U256::ZERO,
            circulating_before: U256::ZERO,
            input_amount: U256::from(wei),
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: HoldingCap::None,
        })
        .unwrap();
        StrategyCurve {
            terms,
            reserve: q.reserve_after,
            circulating: q.supply_after,
        }
    }

    /// Phase I: a Strategy C PASS runs the production path to the sink: the
    /// curve admission, the BUY build and PAPER signature, a strategy lane,
    /// the local node's single-raw plan, its frame and the dispatch. The
    /// exit then signs the approval of the Portal (next nonce) and the
    /// native-output SELL of the position's tokens (the nonce after); both
    /// reach the sink. Nothing is sent.
    #[test]
    fn a_strategy_entry_and_exit_reach_the_sink() {
        use darkbot_bsc_trading::venues::flap::abi::decode_approve;
        use darkbot_bsc_trading::venues::flap::recognize::decode_swap_exact_input;
        let mut cfg = config(2, 2);
        cfg.strategy_lanes = 4;
        let (mut exec, d) = armed("strategy", cfg);
        let portal = bsc_mainnet_v5_27_0().portal;
        let token = [0xAB; 20];
        let input = U256::from(10_000_000_000_000_000u64);
        let curve = curve_after(1_000_000_000_000_000_000);
        let buy = exec
            .on_strategy_buy(
                token,
                curve,
                input,
                &strategy_c2(Some(HEAD)),
                &clock(500, 3),
                500,
            )
            .unwrap();
        assert_eq!((buy.kind, buy.token), (IntentKind::Buy, token));
        let signed = decode_signed(&buy.raw).unwrap();
        assert_eq!((signed.tx.chain_id, signed.tx.nonce), (56, 0));
        assert_eq!((signed.tx.to, signed.tx.value), (portal, input));
        let call = decode_swap_exact_input_native(signed.tx.input).unwrap();
        assert_eq!(call.output_token.into_array(), token);
        // The single raw transaction is the whole request.
        assert!(buy.handoff.raw_len >= buy.raw.len());
        let t = buy.times;
        assert!(t.ingress_ns <= t.admitted_ns && t.admitted_ns < t.signed_ns);
        assert!(t.signed_ns < t.prepared_ns && t.prepared_ns < t.planned_ns);
        assert!(t.planned_ns < t.sink_ns);
        // One BUY per token.
        assert_eq!(
            exec.on_strategy_buy(
                token,
                curve,
                input,
                &strategy_c2(Some(HEAD)),
                &clock(600, 3),
                600
            ),
            Err(Refusal::DuplicateToken)
        );
        // The exit of the tokens bought, on the curve with them in it.
        let tokens = call.min_output_amount;
        let after = StrategyCurve {
            reserve: curve.reserve + input,
            circulating: curve.circulating + tokens,
            ..curve
        };
        let curve_with_buy = {
            use darkbot_bsc_trading::economics::flap::native_curve20::{
                HoldingCap, NativeBuyInputs, quote_native_buy,
            };
            let q = quote_native_buy(&NativeBuyInputs {
                token: curve.terms,
                reserve_before: curve.reserve,
                circulating_before: curve.circulating,
                input_amount: input,
                protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
                tax_on_bonding_curve_enabled: true,
                caller_fee_exempt: false,
                holding_cap: HoldingCap::None,
            })
            .unwrap();
            StrategyCurve {
                reserve: q.reserve_after,
                circulating: q.supply_after,
                ..after
            }
        };
        let (exits, refused) = exec.on_strategy_exit(
            token,
            tokens,
            curve_with_buy,
            &strategy_c2(Some(HEAD)),
            &clock(900, 3),
            900,
        );
        assert_eq!(refused, None);
        let [approve, sell]: [PaperIntent; 2] = exits.try_into().unwrap();
        // Gate M3: the sender's sequence, BUY 0, approval 1, SELL 2.
        let a = decode_signed(&approve.raw).unwrap();
        assert_eq!(
            (approve.kind, a.tx.nonce, a.tx.to.into_array()),
            (IntentKind::Approve, 1, token)
        );
        assert_eq!(a.tx.value, U256::ZERO);
        assert_eq!(decode_approve(a.tx.input).unwrap(), (portal, tokens));
        let s = decode_signed(&sell.raw).unwrap();
        assert_eq!(
            (sell.kind, s.tx.nonce, s.tx.to),
            (IntentKind::Sell, 2, portal)
        );
        let swap = decode_swap_exact_input(s.tx.input).unwrap();
        assert!(swap.is_sell());
        assert_eq!(
            (swap.input_token.into_array(), swap.input_amount),
            (token, tokens)
        );
        assert!(sell.handoff.sequence > approve.handoff.sequence);
        // One exit per token; the counters and lanes say so.
        assert_eq!(
            exec.on_strategy_exit(
                token,
                tokens,
                curve_with_buy,
                &strategy_c2(Some(HEAD)),
                &clock(990, 3),
                990
            ),
            (Vec::new(), Some(Refusal::DuplicateToken))
        );
        assert_eq!(exec.strategy_counters().handoffs, 3);
        assert_eq!(exec.strategy_counters().refused("DUPLICATE_TOKEN"), 2);
        let (armed_lanes, left, next_nonce, spent, gap) = exec.strategy_lanes();
        assert_eq!((armed_lanes, left, next_nonce, gap), (4, 1, 3, false));
        // Gate L1: the closed position's input left the exposure; the gas
        // stays.
        assert!(spent > U256::ZERO && spent < input, "{spent}");
        // C3's own pool is untouched.
        assert_eq!(exec.lanes(), (2, 2));
        exec.disarm().unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    /// Phase I: without a coherent canonical snapshot or a head nothing is
    /// admitted; an entry above the strategy's session cap is refused.
    #[test]
    fn a_strategy_entry_needs_a_snapshot_and_room_in_its_cap() {
        let mut cfg = config(2, 2);
        cfg.strategy_session_cap_wei = 10_000_000_000_000_000;
        let (mut exec, d) = armed("strategy-refusals", cfg);
        let curve = curve_after(1_000_000_000_000_000_000);
        let input = U256::from(10_000_000_000_000_000u64);
        let none = CanonicalContext {
            snapshot: &|_, _| None,
            head: Some(HEAD),
            now_unix_ms: NOW_MS,
            next_nonce: None,
        };
        assert_eq!(
            exec.on_strategy_buy([1; 20], curve, input, &none, &clock(1, 1), 1),
            Err(Refusal::NoSnapshot)
        );
        assert_eq!(
            exec.on_strategy_buy([1; 20], curve, input, &strategy_c2(None), &clock(1, 1), 1),
            Err(Refusal::NoSnapshot)
        );
        // Gate L-a: a head older than the bound (30 s) is not acted on; a
        // follower catching up does not trade old blocks as fresh.
        let late = CanonicalContext {
            snapshot: &canonical,
            head: Some(HEAD),
            now_unix_ms: HEAD.timestamp * 1_000 + 30_001,
            next_nonce: None,
        };
        assert_eq!(
            exec.on_strategy_buy([1; 20], curve, input, &late, &clock(1, 1), 1),
            Err(Refusal::StaleHead)
        );
        // Input plus gas is above the cap.
        assert_eq!(
            exec.on_strategy_buy(
                [1; 20],
                curve,
                input,
                &strategy_c2(Some(HEAD)),
                &clock(1, 1),
                1
            ),
            Err(Refusal::SessionCap)
        );
        assert_eq!(exec.strategy_counters().handoffs, 0);
        assert_eq!(exec.strategy_lanes().1, 2);
        exec.disarm().unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    /// BSC3-069: the BUY policy and the exit policy each from their own
    /// configured slippage.
    #[test]
    fn the_exit_has_its_own_slippage() {
        let mut cfg = config(1, 1);
        cfg.slippage_bps = 50;
        cfg.exit_slippage_bps = 700;
        let (exec, d) = armed("exit-slippage", cfg);
        assert_eq!(
            (exec.policy.slippage_bps(), exec.exit_policy.slippage_bps()),
            (50, 700)
        );
        exec.disarm().unwrap();
        fs::remove_dir_all(d).unwrap();
    }

    /// Phase I gate M1: an exit of a token without a handed-off BUY is
    /// refused (before: a SELL of tokens never held was signed).
    #[test]
    fn no_exit_without_a_held_buy() {
        let (mut exec, d) = armed("strategy-not-held", config(2, 2));
        let curve = curve_after(1_000_000_000_000_000_000);
        let (out, refused) = exec.on_strategy_exit(
            [5; 20],
            U256::from(1_000u64),
            curve,
            &strategy_c2(Some(HEAD)),
            &clock(1, 1),
            1,
        );
        assert_eq!((out.len(), refused), (0, Some(Refusal::NotHeld)));
        assert_eq!(exec.strategy_lanes().1, 2);
        exec.disarm().unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    /// Phase I gate L-d: one sender, one sequence. A C3 BUY handed off at
    /// nonce 0 makes the strategy's BUY take nonce 1 (before: nonce 0 a
    /// second time), C3's alternatives keep nonce 0; with a strategy
    /// intent holding nonce 0 first, a C3 BUY is refused NONCE_TAKEN.
    #[test]
    fn c3_and_the_strategy_share_one_nonce_sequence() {
        let portal = bsc_mainnet_v5_27_0().portal.into_array();
        let curve = curve_after(1_000_000_000_000_000_000);
        let input = U256::from(10_000_000_000_000_000u64);
        let nonce = |raw: &[u8]| decode_signed(raw).unwrap().tx.nonce;
        let mut cfg = config(2, 2);
        cfg.strategy_lanes = 4;
        let c3 = |exec: &mut PaperExecutor, salt: u8| {
            let (hash, raw, call) = launch(salt, 7);
            exec.on_launch(
                PendingLaunch {
                    hash,
                    raw: &raw,
                    to: Some(portal),
                    input: &call,
                    ingress_ns: 100,
                },
                &c2(Some(HEAD)),
                &clock(100, 7),
            )
        };
        let (mut exec, d) = armed("shared-c3-first", cfg.clone());
        assert_eq!(nonce(&c3(&mut exec, 0).unwrap().raw), 0);
        let buy = exec
            .on_strategy_buy(
                [0xAB; 20],
                curve,
                input,
                &strategy_c2(Some(HEAD)),
                &clock(500, 3),
                500,
            )
            .unwrap();
        assert_eq!(nonce(&buy.raw), 1);
        let (_, left, next_nonce, _, gap) = exec.strategy_lanes();
        assert_eq!((left, next_nonce, gap), (2, 2, false));
        assert_eq!(nonce(&c3(&mut exec, 1).unwrap().raw), 0);
        exec.disarm().unwrap();
        fs::remove_dir_all(&d).unwrap();

        let (mut exec, d) = armed("shared-strategy-first", cfg);
        let buy = exec
            .on_strategy_buy(
                [0xAB; 20],
                curve,
                input,
                &strategy_c2(Some(HEAD)),
                &clock(500, 3),
                500,
            )
            .unwrap();
        assert_eq!(nonce(&buy.raw), 0);
        assert_eq!(c3(&mut exec, 0), Err(Refusal::NonceTaken));
        assert_eq!(exec.counters().refused("NONCE_TAKEN"), 1);
        assert_eq!(exec.lanes(), (2, 2));
        exec.disarm().unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    /// Phase I gate M2: a PAPER path left out of `sender_paths` is refused
    /// PATH_OFF and counted; it takes no lane and no nonce.
    #[test]
    fn a_path_left_out_does_not_take_the_sender() {
        let portal = bsc_mainnet_v5_27_0().portal.into_array();
        let curve = curve_after(1_000_000_000_000_000_000);
        let input = U256::from(10_000_000_000_000_000u64);
        let mut cfg = config(2, 2);
        cfg.sender_paths = SenderPaths::C3;
        let (mut exec, d) = armed("paths-c3", cfg.clone());
        assert_eq!(
            exec.on_strategy_buy(
                [1; 20],
                curve,
                input,
                &strategy_c2(Some(HEAD)),
                &clock(1, 1),
                1
            ),
            Err(Refusal::PathOff)
        );
        assert_eq!(exec.strategy_counters().refused("PATH_OFF"), 1);
        assert_eq!(exec.strategy_lanes().2, 0);
        exec.disarm().unwrap();
        fs::remove_dir_all(&d).unwrap();
        cfg.sender_paths = SenderPaths::StrategyC;
        let (mut exec, d) = armed("paths-strategy", cfg);
        let (hash, raw, call) = launch(0, 7);
        let launch = PendingLaunch {
            hash,
            raw: &raw,
            to: Some(portal),
            input: &call,
            ingress_ns: 100,
        };
        assert_eq!(
            exec.on_launch(launch, &c2(Some(HEAD)), &clock(100, 7)),
            Err(Refusal::PathOff)
        );
        assert_eq!(exec.counters().refused("PATH_OFF"), 1);
        assert_eq!(exec.lanes(), (2, 2));
        exec.disarm().unwrap();
        fs::remove_dir_all(&d).unwrap();
        assert_eq!(SenderPaths::parse("C3+STRATEGY_C"), Ok(SenderPaths::Both));
        assert!(SenderPaths::parse("C3,STRATEGY_C").is_err());
    }

    /// Phase I gate L2: an exit whose approval reached the sink and whose
    /// SELL missed its start deadline before the permit (the 15th clock
    /// reading) keeps the position: a later exit signs only the SELL, at
    /// the freed nonce, up to the approved amount, and closes it (before:
    /// exited for good, its input never released). Gas counts only for
    /// what may be out.
    #[test]
    fn a_refused_sell_after_its_approval_can_still_close_the_position() {
        use darkbot_bsc_trading::venues::flap::recognize::decode_swap_exact_input;
        let mut cfg = config(2, 2);
        cfg.strategy_lanes = 6;
        let gas = |limit: u64| U256::from(cfg.gas_price_wei) * U256::from(limit);
        let (approve_gas, sell_gas) = (gas(cfg.approve_gas_limit), gas(cfg.sell_gas_limit));
        let (mut exec, d) = armed("strategy-sell-retry", cfg.clone());
        let curve = curve_after(1_000_000_000_000_000_000);
        let input = U256::from(10_000_000_000_000_000u64);
        let c2 = strategy_c2(Some(HEAD));
        let nonce = |raw: &[u8]| decode_signed(raw).unwrap().tx.nonce;
        let buy = exec
            .on_strategy_buy([1; 20], curve, input, &c2, &clock(1, 1), 1)
            .unwrap();
        let after_buy = exec.strategy_lanes().3;
        let tokens = decode_swap_exact_input_native(decode_signed(&buy.raw).unwrap().tx.input)
            .unwrap()
            .min_output_amount;
        let exit_curve = curve_after(1_010_000_000_000_000_000);
        let (out, refused) = exec.on_strategy_exit(
            [1; 20],
            tokens,
            exit_curve,
            &c2,
            &clock_jump(1, 1, 15, 2_000_000),
            1,
        );
        assert_eq!(refused, Some(Refusal::Deadline));
        let [approve]: [PaperIntent; 1] = out.try_into().unwrap();
        assert_eq!(
            (approve.kind, nonce(&approve.raw)),
            (IntentKind::Approve, 1)
        );
        let (_, _, next_nonce, spent, gap) = exec.strategy_lanes();
        assert_eq!((next_nonce, gap), (2, false));
        assert_eq!(spent, after_buy + approve_gas);
        let (out, refused) =
            exec.on_strategy_exit([1; 20], tokens, exit_curve, &c2, &clock(1, 1), 1);
        assert_eq!(refused, None);
        let [sell]: [PaperIntent; 1] = out.try_into().unwrap();
        assert_eq!((sell.kind, nonce(&sell.raw)), (IntentKind::Sell, 2));
        let swap = decode_swap_exact_input(decode_signed(&sell.raw).unwrap().tx.input).unwrap();
        assert!(swap.is_sell() && swap.input_amount == tokens);
        // Closed: the input left the exposure, the gas of what went out stays.
        let (_, _, next_nonce, spent, _) = exec.strategy_lanes();
        assert_eq!(next_nonce, 3);
        assert_eq!(spent, after_buy - input + approve_gas + sell_gas);
        assert_eq!(
            exec.on_strategy_exit([1; 20], tokens, exit_curve, &c2, &clock(1, 1), 1)
                .1,
            Some(Refusal::DuplicateToken)
        );
        exec.disarm().unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    /// `step` apart, `jump` more at the reading number `at` (from 1).
    fn clock_jump(start: u64, step: u64, at: u64, jump: u64) -> FnClock<impl Fn() -> u64> {
        let (now, reads) = (Cell::new(start), Cell::new(0u64));
        FnClock {
            id: CLOCK_ID,
            now: move || {
                reads.set(reads.get() + 1);
                let extra = if reads.get() == at { jump } else { 0 };
                now.set(now.get() + step + extra);
                now.get()
            },
        }
    }

    /// Phase I gate M3/N1: a strategy intent refused before anything of it
    /// was disclosed (the start deadline missed at the pre-permit check, the
    /// 7th clock reading) frees its nonces: the next intent takes them and
    /// the held position still exits (before: a hole, every later intent
    /// NONCE_GAP). Refused after a permit's write started (the write check,
    /// the 8th reading), its bytes may be out: a hole, no later strategy
    /// intent is signed (no later PAPER transaction could pass it).
    #[test]
    fn a_nonce_is_taken_again_only_when_nothing_was_disclosed() {
        let mut cfg = config(2, 2);
        cfg.strategy_lanes = 6;
        let (mut exec, d) = armed("strategy-gap", cfg);
        let curve = curve_after(1_000_000_000_000_000_000);
        let input = U256::from(10_000_000_000_000_000u64);
        let c2 = strategy_c2(Some(HEAD));
        let nonce = |raw: &[u8]| decode_signed(raw).unwrap().tx.nonce;
        let a = exec
            .on_strategy_buy([1; 20], curve, input, &c2, &clock(1, 1), 1)
            .unwrap();
        assert_eq!(nonce(&a.raw), 0);
        let late = 2_000_000;
        assert_eq!(
            exec.on_strategy_buy([2; 20], curve, input, &c2, &clock_jump(1, 1, 7, late), 1),
            Err(Refusal::Deadline)
        );
        let (_, _, next_nonce, _, gap) = exec.strategy_lanes();
        assert_eq!((next_nonce, gap), (1, false));
        // The exit of A, first late before its approval's permit: nothing
        // out, its nonces and the token free; then on time.
        let tokens = decode_swap_exact_input_native(decode_signed(&a.raw).unwrap().tx.input)
            .unwrap()
            .min_output_amount;
        let exit_curve = curve_after(1_010_000_000_000_000_000);
        assert_eq!(
            exec.on_strategy_exit(
                [1; 20],
                tokens,
                exit_curve,
                &c2,
                &clock_jump(1, 1, 7, late),
                1
            ),
            (Vec::new(), Some(Refusal::Deadline))
        );
        let (exits, refused) =
            exec.on_strategy_exit([1; 20], tokens, exit_curve, &c2, &clock(1, 1), 1);
        assert_eq!(refused, None);
        let nonces: Vec<u64> = exits.iter().map(|i| nonce(&i.raw)).collect();
        assert_eq!(nonces, [1, 2]);
        // C: late at the write check, after its permit: a hole.
        assert_eq!(
            exec.on_strategy_buy([3; 20], curve, input, &c2, &clock_jump(1, 1, 8, late), 1),
            Err(Refusal::Dispatch)
        );
        assert_eq!(
            exec.on_strategy_buy([4; 20], curve, input, &c2, &clock(1, 1), 1),
            Err(Refusal::NonceGap)
        );
        let (_, left, next_nonce, _, gap) = exec.strategy_lanes();
        assert_eq!((left, next_nonce, gap), (2, 4, true));
        assert_eq!(exec.strategy_counters().handoffs, 3);
        exec.disarm().unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    /// A second end for the one executor (plan 28.10., package 3): it
    /// records what reaches it and answers as told, as the LIVE end may
    /// (handed off, refused before the write, or possibly sent).
    struct RecordingEnd {
        signer: Signer,
        seen: Vec<(bool, u64, [u8; 32], Vec<u8>)>,
        /// None: handed off. Some(disclosed): refused, the bytes possibly
        /// out when `disclosed`.
        fail: Option<bool>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Recorded(u64);

    impl HandoffTime for Recorded {
        fn at_ns(&self) -> u64 {
            self.0
        }
    }

    impl End for RecordingEnd {
        type Handoff = Recorded;
        const FENCE: (&'static str, [u8; 8]) = ("recording", *b"DBTSTFN1");
        const CHAIN_ORDERED: bool = false;

        fn address(&self) -> [u8; 20] {
            self.signer.address().into_array()
        }

        fn signer(&self) -> &Signer {
            &self.signer
        }

        fn hand_over(&mut self, h: Handover<'_>) -> Handed<Recorded> {
            let bundle = match h.submission {
                Submission::LaunchBundle(launch) => {
                    assert!(!launch.is_empty());
                    true
                }
                Submission::Single => false,
            };
            self.seen
                .push((bundle, h.ticket.nonce(), h.tx_hash, h.raw.to_vec()));
            let at = h.clock.now_ns();
            match self.fail {
                None => Handed {
                    result: Ok(Recorded(at)),
                    planned_ns: at,
                    admitted: true,
                    disclosed: true,
                },
                Some(disclosed) => Handed {
                    result: Err(Refusal::Dispatch),
                    planned_ns: at,
                    admitted: disclosed,
                    disclosed,
                },
            }
        }
    }

    fn recording(name: &str, fail: Option<bool>) -> (Executor<RecordingEnd>, PathBuf) {
        let d = dir(name);
        let end = RecordingEnd {
            signer: Signer::from_secret_bytes(&[0x11; 32]).unwrap(),
            seen: Vec::new(),
            fail,
        };
        let exec = Executor::arm_end(
            end,
            validated(),
            WITNESSED_MAINNET_TERMS,
            config(2, 2),
            Arming {
                next_nonce: 4,
                fence_dir: &d.join("fences"),
                session: 1,
                arm: 0,
            },
        )
        .unwrap();
        (exec, d)
    }

    /// Plan 28.10.: the one path (decoder, C2, admission, signature, lane,
    /// final binding check) reaches whichever end it is armed with; the
    /// end gets the prepared transaction, signed by its own signer, and
    /// what it takes is the BUY's handoff. Its fence is its own.
    #[test]
    fn the_one_path_reaches_any_end() {
        let (mut paper, pd) = armed("end-paper", config(2, 2));
        let (mut other, od) = recording("end-other", None);
        let portal = bsc_mainnet_v5_27_0().portal.into_array();
        let (hash, raw, input) = launch(0, 7);
        let pending = PendingLaunch {
            hash,
            raw: &raw,
            to: Some(portal),
            input: &input,
            ingress_ns: 100,
        };
        let a = paper
            .on_launch(pending, &c2(Some(HEAD)), &clock(100, 7))
            .unwrap();
        let b = other
            .on_launch(pending, &c2(Some(HEAD)), &clock(100, 7))
            .unwrap();
        let (x, y) = (
            decode_signed(&a.raw).unwrap(),
            decode_signed(&b.raw).unwrap(),
        );
        assert_eq!((x.tx.nonce, y.tx.nonce), (0, 4));
        assert_eq!((x.tx.to, x.tx.value), (y.tx.to, y.tx.value));
        assert_eq!(
            decode_swap_exact_input_native(x.tx.input)
                .unwrap()
                .output_token,
            decode_swap_exact_input_native(y.tx.input)
                .unwrap()
                .output_token
        );
        let from = darkbot_bsc_trading::signer::recover_legacy_sender(&y, 56, &mut Vec::new());
        assert_eq!(from.unwrap().into_array(), other.sender());
        assert_eq!(a.token, b.token);
        let seen = &other.end().seen;
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0], (true, 4, keccak256(&b.raw).0, b.raw.clone()));
        assert_eq!(b.times.sink_ns, b.handoff.0);
        assert_eq!(other.spent_wei(), other.budget.max_total_wei);
        // A Strategy C BUY is the transaction alone.
        let token = [0xAB; 20];
        let input = U256::from(10_000_000_000_000_000u64);
        let curve = curve_after(1_000_000_000_000_000_000);
        let intent = other
            .on_strategy_buy(
                token,
                curve,
                input,
                &strategy_c2(Some(HEAD)),
                &clock(500, 3),
                500,
            )
            .unwrap();
        let seen = &other.end().seen;
        assert_eq!((seen[1].0, seen[1].1), (false, 5));
        assert_eq!(seen[1].3, intent.raw);
        let fence = other.fence().to_path_buf();
        assert_eq!(fence.file_name().unwrap(), "recording-1-0.fence");
        assert_eq!(&fs::read(&fence).unwrap()[..8], b"DBTSTFN1");
        paper.disarm().unwrap();
        other.disarm().unwrap();
        fs::remove_dir_all(pd).unwrap();
        fs::remove_dir_all(od).unwrap();
    }

    /// What the end says it consumed decides, not the end's kind: a
    /// refusal before the write gives the nonce back and spends nothing; a
    /// transaction possibly out (LIVE: UNKNOWN) spends its exposure and
    /// stops the sender's sequence, with no retry.
    #[test]
    fn what_an_end_consumed_decides_nonce_and_exposure() {
        let token = [0xAB; 20];
        let input = U256::from(10_000_000_000_000_000u64);
        let curve = curve_after(1_000_000_000_000_000_000);
        let buy = |e: &mut Executor<RecordingEnd>, t: [u8; 20]| {
            e.on_strategy_buy(
                t,
                curve,
                input,
                &strategy_c2(Some(HEAD)),
                &clock(500, 3),
                500,
            )
        };
        let (mut clean, d1) = recording("end-clean", Some(false));
        assert_eq!(buy(&mut clean, token), Err(Refusal::Dispatch));
        assert_eq!(buy(&mut clean, [0xAC; 20]), Err(Refusal::Dispatch));
        let seen = &clean.end().seen;
        // The same nonce again, nothing spent, no hole.
        assert_eq!((seen[0].1, seen[1].1), (4, 4));
        let (_, _, next, spent, gap) = clean.strategy_lanes();
        assert_eq!((next, spent, gap), (4, U256::ZERO, false));
        let (mut unknown, d2) = recording("end-unknown", Some(true));
        assert_eq!(buy(&mut unknown, token), Err(Refusal::Dispatch));
        let (_, _, _, spent, gap) = unknown.strategy_lanes();
        assert!(gap && spent > U256::ZERO);
        assert_eq!(buy(&mut unknown, [0xAC; 20]), Err(Refusal::NonceGap));
        assert_eq!(unknown.end().seen.len(), 1);
        clean.disarm().unwrap();
        unknown.disarm().unwrap();
        fs::remove_dir_all(d1).unwrap();
        fs::remove_dir_all(d2).unwrap();
    }
}

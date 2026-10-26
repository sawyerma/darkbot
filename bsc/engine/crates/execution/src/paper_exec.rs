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

use crate::paper::{PaperHandoff, PaperIdentity, PaperSink};
use crate::submission::send::{
    Armed, DispatchContext, DispatchError, SendOutcome, dispatch_diagnostic,
};
use crate::submission::tls::{CLUB48_DOCUMENTED_HOST, HTTPS_PORT, ProviderTarget, TlsEndpoint};
use crate::submission::transport::{Clock, ConnectionState, Transport};
use crate::submission::{club48, local_geth};
use alloy_primitives::{Address, B256, U256};
use darkbot_bsc_trading::chain_state::SnapshotId;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    CurveState, PricedToken, QUALIFIED_MAINNET_INPUT_WEI, QualifiedNativeState, VenueBuyTerms,
    admit_curve_buy, admit_curve_sell, admit_native_buy,
};
use darkbot_bsc_trading::economics::policy::{BuyPolicy, ExitPolicy};
use darkbot_bsc_trading::intent::{AdapterId, AdapterSet, ArmedFence, IntentLane, Ticket};
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

/// The operating values of the PAPER path (`BSC_EXEC_*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecConfig {
    pub input_wei: u128,
    pub slippage_bps: u16,
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

/// One Strategy C PAPER transaction that reached the sink (Phase I). Its
/// `times.ingress_ns` is the decision's record time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaperIntent {
    pub kind: IntentKind,
    pub token: [u8; 20],
    /// The canonical head block it was formed on.
    pub block: u64,
    pub handoff: PaperHandoff,
    pub times: StageTimes,
    /// The signed transaction (a local test artefact, never sent).
    pub raw: Vec<u8>,
}

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
    /// The sink took the complete request: `T_shadow_sink`.
    pub sink_ns: u64,
}

/// One PAPER BUY that reached the sink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaperBuy {
    pub launch_hash: [u8; 32],
    pub token: [u8; 20],
    pub handoff: PaperHandoff,
    pub times: StageTimes,
    /// The signed BUY (a local test artefact, never sent).
    pub raw: Vec<u8>,
}

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

/// The armed PAPER path.
pub struct PaperExecutor {
    identity: PaperIdentity,
    deployment: ValidatedDeployment,
    terms: VenueBuyTerms,
    policy: BuyPolicy,
    config: ExecConfig,
    table: AdapterTable,
    budget: BudgetEnvelope,
    target: ProviderTarget,
    lanes: Vec<IntentLane>,
    used: usize,
    /// Exposure dispatched in this session, against `session_cap_wei`.
    spent_wei: U256,
    /// Tokens bought in this session (at most one per lane).
    bought: Vec<[u8; 20]>,
    fence: PathBuf,
    sink: PaperSink,
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
    exited: Vec<[u8; 20]>,
    exit_policy: ExitPolicy,
    strategy_counters: ExecCounters,
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

/// The executor parts one single raw transaction needs (Phase I).
struct RawSink<'a> {
    lane: &'a mut IntentLane,
    sink: &'a mut PaperSink,
    table: &'a AdapterTable,
    config: &'a ExecConfig,
    scratch: &'a mut Vec<u8>,
}

/// What one dispatch to the sink consumed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Consumed {
    /// The economic admission (`begin_send`): its exposure counts
    /// whatever followed.
    admission: bool,
    /// A permit's write was started (or the dispatch broke after the
    /// admission): the bytes may be out, their nonce is taken.
    disclosed: bool,
}

/// Binds a signed transaction in its lane, plans it as the single raw
/// transaction of the local node's adapter and dispatches it to the sink.
/// Returns the handoff, the prepared and planned times, and what the
/// dispatch consumed.
#[allow(clippy::too_many_arguments)]
fn raw_to_sink(
    parts: RawSink<'_>,
    ticket: Ticket,
    raw: &[u8],
    expected: ExpectedTx<'_>,
    budget: &BudgetEnvelope,
    snapshot: &SnapshotId,
    head: ChainHead,
    clock: &dyn Clock,
    start_ns: u64,
) -> (Result<PaperHandoff, Refusal>, u64, u64, Consumed) {
    let RawSink {
        lane,
        sink,
        table,
        config,
        scratch,
    } = parts;
    let tx_hash = match lane.prepare(ticket, raw, expected, snapshot, scratch) {
        Ok(h) => h,
        Err(_) => return (Err(Refusal::Lane), 0, 0, Consumed::default()),
    };
    let prepared_ns = clock.now_ns();
    let adapter = paper_raw_adapter();
    let planned = (|| {
        let adapters = AdapterSet::new(&[adapter]).ok_or(Refusal::Plan)?;
        let last_block = head
            .number
            .checked_add(config.validity_blocks)
            .ok_or(Refusal::Plan)?;
        let plan = SubmissionPlan::new(
            table,
            expected,
            budget,
            PlanInputs {
                ticket,
                buy: TxRef::of(raw),
                launch: Launch::None,
                mode: SubmissionMode::SingleRawBuy,
                profile: PAPER_PROFILE,
                adapters,
                local_block_deadline: LocalBlockStartDeadline {
                    last_head_block: last_block,
                },
                provider_validity: None,
                merge: MergePolicy::ProviderDefault,
                pre_submission: PreSubmission::No,
                provider_payment_wei: U256::ZERO,
            },
        )
        .map_err(|_| Refusal::Plan)?;
        let frame = local_geth::frame(&plan, table, adapter, config.max_frame)
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
        Err(r) => return (Err(r), prepared_ns, 0, Consumed::default()),
    };
    let planned_ns = clock.now_ns();
    let mut transport = SinkTransport {
        sink,
        clock,
        tx_hash: tx_hash.0,
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
    let consumed = match &dispatched {
        Ok(report) => Consumed {
            admission: report.admitted(),
            disclosed: report.admitted() && !report.admitted_without_disclosure,
        },
        Err(DispatchError::DispatchBinding(_)) => Consumed {
            admission: true,
            disclosed: true,
        },
        Err(_) => Consumed::default(),
    };
    let result = match dispatched {
        Err(_) => Err(Refusal::Dispatch),
        Ok(report) => match report.adapter(adapter).map(|r| r.outcome) {
            Some(SendOutcome::HandedOff) => handoff.ok_or(Refusal::Dispatch),
            Some(SendOutcome::SkippedDeadline(_)) => Err(Refusal::Deadline),
            _ => Err(Refusal::Dispatch),
        },
    };
    (result, prepared_ns, planned_ns, consumed)
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
}

impl PaperExecutor {
    /// Arms the PAPER path: the policy, the adapter table and the selected
    /// builder target, then one durable fence `paper-<session>-<arm>.fence`
    /// in `fence_dir` (unique per arm) and behind it `config.lanes`
    /// single-intent lanes at the sender's canonical `next_nonce` (a C2
    /// nonce fact).
    pub fn arm(
        identity: PaperIdentity,
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
            config.slippage_bps,
            config.gas_price_wei,
            config.approve_gas_limit,
            config.sell_gas_limit,
        )
        .map_err(|e| ArmError::Policy(format!("{e:?}")))?;
        let chain_id = deployment.deployment().chain_id;
        let table = AdapterTable::new(
            chain_id,
            config.capability_generation,
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
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(fence_dir)
            .map_err(|e| ArmError::Fence(format!("{}: {e}", fence_dir.display())))?;
        let sender = Address::from(identity.address());
        // The strategy's nonces: one lane per nonce of the sequence.
        next_nonce
            .checked_add(config.strategy_lanes as u64)
            .ok_or_else(|| ArmError::Config("the strategy nonces overflow".into()))?;
        let mut body = Vec::with_capacity(8 + 8 + 20 + 8 + 16 + 8 + 8 + 8 + 8 + 16);
        body.extend_from_slice(b"DBPAPFN3");
        body.extend_from_slice(&chain_id.to_le_bytes());
        body.extend_from_slice(sender.as_slice());
        body.extend_from_slice(&next_nonce.to_le_bytes());
        body.extend_from_slice(&config.session_cap_wei.to_le_bytes());
        body.extend_from_slice(&(config.lanes as u64).to_le_bytes());
        body.extend_from_slice(&config.policy_generation.to_le_bytes());
        body.extend_from_slice(&config.capability_generation.to_le_bytes());
        body.extend_from_slice(&(config.strategy_lanes as u64).to_le_bytes());
        body.extend_from_slice(&config.strategy_session_cap_wei.to_le_bytes());
        let fence = fence_dir.join(format!("paper-{session}-{arm}.fence"));
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
        Ok(PaperExecutor {
            identity,
            deployment,
            terms,
            policy,
            table,
            budget,
            target,
            lanes,
            used: 0,
            spent_wei: U256::ZERO,
            bought: Vec::with_capacity(config.lanes),
            fence,
            sink: PaperSink::default(),
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
            exited: Vec::with_capacity(strategy_lanes.len()),
            strategy_lanes,
            exit_policy,
            strategy_counters: ExecCounters::default(),
            config,
        })
    }

    /// Disarms: the lanes go and the arm's fence is removed (a PAPER BUY
    /// never leaves the process, so nothing waits for reconciliation).
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
    ) -> Result<PaperBuy, Refusal> {
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
    ) -> Result<PaperBuy, Refusal> {
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
        if self.bought.contains(&token) {
            return Err(Refusal::DuplicateToken);
        }
        let exposure = self.budget.max_total_wei;
        if self.spent_wei + exposure > U256::from(self.config.session_cap_wei) {
            return Err(Refusal::SessionCap);
        }
        if self.strategy_used > 0 && !self.strategy_skipped_first {
            return Err(Refusal::NonceTaken);
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
        let sender = Address::from(self.identity.address());
        let expected = ExpectedBuy::from_admitted(
            &self.deployment,
            &admitted,
            &self.policy,
            sender,
            ticket.nonce(),
        )
        .map_err(|_| Refusal::Intent)?;
        let tx_hash = expected
            .sign(self.identity.signer(), &mut self.scratch, &mut self.raw)
            .map_err(|_| Refusal::Build)?;
        let signed_ns = clock.now_ns();
        lane.prepare(ticket, &self.raw, &expected, &snapshot, &mut self.scratch)
            .map_err(|_| Refusal::Lane)?;
        let prepared_ns = clock.now_ns();
        let adapter = paper_adapter();
        let adapters = AdapterSet::new(&[adapter]).ok_or(Refusal::Plan)?;
        let last_block = head
            .number
            .checked_add(self.config.validity_blocks)
            .ok_or(Refusal::Plan)?;
        let plan = SubmissionPlan::new(
            &self.table,
            &expected,
            &self.budget,
            PlanInputs {
                ticket,
                buy: TxRef::of(&self.raw),
                launch: Launch::PublicOriginal(TxRef::of(launch_raw)),
                mode: SubmissionMode::PublicFullLaunchBundle,
                profile: PAPER_PROFILE,
                adapters,
                local_block_deadline: LocalBlockStartDeadline {
                    last_head_block: last_block,
                },
                provider_validity: Some(ProviderValidity {
                    max_block_number: last_block,
                    min_timestamp: None,
                    max_timestamp: None,
                }),
                merge: MergePolicy::NoMerge,
                pre_submission: PreSubmission::No,
                provider_payment_wei: U256::ZERO,
            },
        )
        .map_err(|_| Refusal::Plan)?;
        let mut frame = club48::frame(
            &plan,
            &self.table,
            adapter,
            &self.target.frame_endpoint(),
            launch_raw,
            self.config.max_frame,
        )
        .map_err(|_| Refusal::Plan)?;
        let deadline = TransportStartDeadline::for_profile(
            PAPER_PROFILE,
            clock.id(),
            ingress_ns,
            self.config.start_limit_ns,
        )
        .map_err(|_| Refusal::Plan)?;
        let planned_ns = clock.now_ns();
        let mut transport = SinkTransport {
            sink: &mut self.sink,
            clock,
            tx_hash: tx_hash.0,
            max_request_len: self.config.max_frame,
            state: ConnectionState::Ready,
            handoff: None,
        };
        let dispatched = dispatch_diagnostic(
            lane,
            &plan,
            &DispatchContext {
                table: &self.table,
                current: &snapshot,
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
        // The exposure is the session's once the economic admission was
        // consumed (`begin_send`), whatever followed.
        let (admitted, disclosed) = match &dispatched {
            Ok(report) => (
                report.admitted(),
                report.admitted() && !report.admitted_without_disclosure,
            ),
            Err(DispatchError::DispatchBinding(_)) => (true, true),
            Err(_) => (false, false),
        };
        if admitted {
            self.spent_wei += exposure;
        }
        self.c3_took_nonce |= disclosed;
        let report = dispatched.map_err(|_| Refusal::Dispatch)?;

        match report.adapter(adapter).map(|r| r.outcome) {
            Some(SendOutcome::HandedOff) => {}
            Some(SendOutcome::SkippedDeadline(_)) => return Err(Refusal::Deadline),
            _ => return Err(Refusal::Dispatch),
        }
        let handoff = handoff.ok_or(Refusal::Dispatch)?;
        Ok(PaperBuy {
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
                sink_ns: handoff.at_ns,
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
    ) -> Result<PaperIntent, Refusal> {
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
    /// refused (gate L3), with that refusal.
    pub fn on_strategy_exit(
        &mut self,
        token: [u8; 20],
        tokens: U256,
        curve: StrategyCurve,
        c2: &CanonicalContext<'_>,
        clock: &dyn Clock,
        triggered_ns: u64,
    ) -> (Vec<PaperIntent>, Option<Refusal>) {
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

    /// The next strategy lane: its nonce is the next of the PAPER
    /// sender's sequence (each lane one nonce above the last). Refused
    /// once a taken nonce was not handed off (a hole no later PAPER
    /// transaction could pass on chain).
    fn next_strategy_lane(&mut self) -> Result<usize, Refusal> {
        if self.strategy_nonce_gap {
            return Err(Refusal::NonceGap);
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
        let sender = Address::from(self.identity.address());
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

    /// An exit refused at the lane `from`; with nothing of it out (`none_out`
    /// and not `disclosed`) the token can be exited later.
    fn exit_refused(&mut self, token: [u8; 20], from: usize, disclosed: bool, none_out: bool) {
        self.strategy_refused(from, disclosed);
        if none_out && !disclosed {
            self.exited.retain(|t| *t != token);
        }
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
    ) -> Result<PaperIntent, Refusal> {
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
        if self.strategy_bought.iter().any(|h| h.token == token) {
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
        let index = self.next_strategy_lane()?;
        let Ok(ticket) = self.strategy_lanes[index].reserve(admitted.token()) else {
            self.strategy_refused(index, true);
            return Err(Refusal::Lane);
        };
        let sender = Address::from(self.identity.address());
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
                .sign(self.identity.signer(), &mut self.scratch, &mut self.raw)
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
        let (handoff, prepared_ns, planned_ns, consumed) = raw_to_sink(
            RawSink {
                lane: &mut self.strategy_lanes[index],
                sink: &mut self.sink,
                table: &self.table,
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
        );
        if consumed.admission {
            self.strategy_spent_wei += budget.max_total_wei;
        }
        let handoff = match handoff {
            Ok(h) => h,
            Err(r) => {
                self.strategy_refused(index, consumed.disclosed);
                return Err(r);
            }
        };
        self.strategy_bought.push(Held {
            token,
            min_tokens: admitted.min_output_amount(),
            input_wei,
        });
        Ok(PaperIntent {
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
                sink_ns: handoff.at_ns,
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
        out: &mut Vec<PaperIntent>,
    ) -> Result<(), Refusal> {
        let held = self
            .strategy_bought
            .iter()
            .find(|h| h.token == token)
            .copied()
            .ok_or(Refusal::NotHeld)?;
        if self.exited.contains(&token) {
            return Err(Refusal::DuplicateToken);
        }
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
        let admitted = admit_curve_sell(&state, tokens.min(held.min_tokens), &self.exit_policy)
            .map_err(|_| Refusal::Admission)?;
        let admitted_ns = clock.now_ns();
        let price = U256::from(self.config.gas_price_wei);
        let approve_gas = price * U256::from(self.config.approve_gas_limit);
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
        if self.strategy_lanes.len() - self.strategy_used < 2 {
            return Err(Refusal::LanesExhausted);
        }
        let approve_index = self.next_strategy_lane()?;
        let sell_index = self.next_strategy_lane()?;
        let mut tickets = [None, None];
        for (slot, index) in [approve_index, sell_index].into_iter().enumerate() {
            match self.strategy_lanes[index].reserve(admitted.token()) {
                Ok(t) => tickets[slot] = Some(t),
                Err(_) => {
                    self.strategy_refused(approve_index, true);
                    return Err(Refusal::Lane);
                }
            }
        }
        let [Some(approve_ticket), Some(sell_ticket)] = tickets else {
            self.strategy_refused(approve_index, true);
            return Err(Refusal::Lane);
        };
        if sell_ticket.nonce() != approve_ticket.nonce() + 1 {
            self.strategy_refused(approve_index, true);
            return Err(Refusal::Intent);
        }
        let sender = Address::from(self.identity.address());
        let Ok(expected) = ExpectedExit::from_admitted(
            &self.deployment,
            &admitted,
            &self.exit_policy,
            sender,
            approve_ticket.nonce(),
        ) else {
            self.strategy_refused(approve_index, false);
            return Err(Refusal::Intent);
        };
        // Taken from here: a refusal before the approval is out frees
        // the token for a later exit as well as its nonces.
        self.exited.push(token);
        for (kind, index, ticket, gas) in [
            (
                IntentKind::Approve,
                approve_index,
                approve_ticket,
                approve_gas,
            ),
            (IntentKind::Sell, sell_index, sell_ticket, sell_gas),
        ] {
            let built = match kind {
                IntentKind::Approve => {
                    expected.sign_approve(self.identity.signer(), &mut self.scratch, &mut self.raw)
                }
                _ => expected.sign_sell(self.identity.signer(), &mut self.scratch, &mut self.raw),
            };
            if built.is_err() {
                self.exit_refused(token, index, false, out.is_empty());
                return Err(Refusal::Build);
            }
            let signed_ns = clock.now_ns();
            let tx = match kind {
                IntentKind::Approve => ExpectedTx::Approve(&expected),
                _ => ExpectedTx::Sell(&expected),
            };
            let (handoff, prepared_ns, planned_ns, consumed) = raw_to_sink(
                RawSink {
                    lane: &mut self.strategy_lanes[index],
                    sink: &mut self.sink,
                    table: &self.table,
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
            );
            if consumed.admission {
                self.strategy_spent_wei += gas;
            }
            let handoff = match handoff {
                Ok(h) => h,
                Err(r) => {
                    self.exit_refused(token, index, consumed.disclosed, out.is_empty());
                    return Err(r);
                }
            };
            out.push(PaperIntent {
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
                    sink_ns: handoff.at_ns,
                },
                raw: self.raw.clone(),
            });
        }
        // The position is closed: its input leaves the open exposure, the
        // gas spent stays (gate L1).
        self.strategy_spent_wei = self.strategy_spent_wei.saturating_sub(held.input_wei);
        Ok(())
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
        self.identity.address()
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

    pub fn sink(&self) -> &PaperSink {
        &self.sink
    }

    pub fn fence(&self) -> &Path {
        &self.fence
    }
}

#[cfg(test)]
mod tests {
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

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bsc-paper-exec-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    /// The current deployment (economics reconfirmed on it).
    fn validated() -> ValidatedDeployment {
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

    fn config(lanes: usize, buys: u128) -> ExecConfig {
        let gas_price_wei = 1_000_000_000;
        let gas_limit = 300_000;
        let one = 1_000_000_000_000_000 + u128::from(gas_price_wei) * u128::from(gas_limit);
        ExecConfig {
            // The amount covered by witnessed evidence.
            input_wei: 1_000_000_000_000_000,
            slippage_bps: 500,
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
    fn launch(salt: u8, nonce: u64) -> ([u8; 32], Vec<u8>, Vec<u8>) {
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

    fn c2(head: Option<ChainHead>) -> LaunchContext<'static> {
        LaunchContext {
            snapshot: &snapshot,
            head,
        }
    }

    const HEAD: ChainHead = ChainHead {
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

    fn clock(start: u64, step: u64) -> FnClock<impl Fn() -> u64> {
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

    fn strategy_c2(head: Option<ChainHead>) -> CanonicalContext<'static> {
        CanonicalContext {
            snapshot: &canonical,
            head,
            now_unix_ms: NOW_MS,
        }
    }

    /// A curve after real BUYs of `wei` from the fresh curve, and the
    /// quote of a BUY of `input` on it.
    fn curve_after(wei: u128) -> StrategyCurve {
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
}

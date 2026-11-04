//! The LIVE path in the runtime (plan 28.10., package 5): the one executor
//! armed towards its LIVE end, under the owner's effective LIVE. Its
//! lifecycle, outside the entry path, in every reconcile pass:
//!
//! 1. wanted only while the owner's LIVE gate is clear now for the runtime,
//!    the grant's venue and its components (`Store::live_now`) and the LIVE
//!    qualification on file verifies; otherwise the path is disarmed and
//!    its key dropped, with the reason;
//! 2. the buyer must be a watched C2 sender; the live Flap deployment and
//!    terms are probed read-only with the buyer as caller, pinned to one
//!    block: a drift or other terms stop here; the buyer must hold one BUY
//!    (the configured input and its gas at the signed price);
//! 3. C2's clean next nonce of the buyer is the lanes' first nonce;
//! 4. the key is loaded under the `LiveNow` (a private file outside the
//!    repository, the buyer's address), the LIVE end built over its route,
//!    its connections established, the executor armed behind its own fence;
//! 5. while armed, every pass checks the gate again (a revoke, the expiry,
//!    a switch off or a restart disarms, visibly), keeps the connections
//!    and probes the deployment again every `BSC_EXEC_REPROBE_MS`.
//!
//! Every transaction whose bytes may have left is a fact of the LIVE
//! ledger at once; the canonical chain's receipts (status, gas, the
//! Portal's trade) and reorgs follow as facts, block by block; the
//! positions they make are adopted by the executor (its exits sell the
//! real tokens) and held by Strategy C (its exit rules on the real curve).
//!
//! A restart or a closed gate never forgets one (BSC3-024/041): the owner's
//! admission entries recover what the process could not write; an arm
//! needs the ledger readable and takes the buyer's next nonce only when no
//! transaction may still take it: every one resolved on chain (included,
//! reverted, or its nonce taken by another) or its bundle's window
//! (`BSC_EXEC_VALIDITY_BLOCKS`, written with its admission) over
//! (BSC3-048). Such a transaction stays unresolved, its spend counted,
//! until its nonce is taken; if it lands after all, the one that took its
//! place is REPLACED (by which transaction, in which block) and an exit it
//! replaced goes on. An open position's exit takes a free nonce before any
//! new BUY. An arm that ends with a transaction unresolved keeps its
//! fence; a later arm settles the fences of earlier ones.
//!
//! Launches reach it only as verified full pending transactions (the
//! runtime never hands it a hash-only sighting). Nothing is re-armed
//! automatically after a refusal: switching the scope off and on again
//! starts over, with a new fence.

use crate::chain_follower::{CanonicalEvent, FollowerLimits, RecoveryTarget};
use crate::live_end::{Connecting, LiveEnd, LiveHandoff, LiveRoute};
use crate::live_key::LiveKey;
use crate::live_ledger::{Book, Fact, FactKind, FactSource, Ledger, PositionState, fold};
use crate::live_qualification;
use crate::paper_exec::{Arming, ExecBuy, ExecIntent, Executor, NonceEvidence, Refusal};
use crate::runtime::exec::{ExecSetup, Prober, launch_to, live_probe, strategy_to};
use crate::runtime::state::{LaneEvidence, StateOwner};
use crate::sources::{
    ComponentId, LiveAdapter, LiveIntentKind, LiveStrategies, NonceCosts, NonceKey, NonceOrigin,
    SourceId, Store,
};
use crate::submission::tls::{AuthorizationToken, Q5A_TLS_POLICY, TlsSetup};
use crate::submission::transport::Clock;
use crate::venue_probe::VenueObservation;
use alloy_primitives::{Address, U256};
use darkbot_bsc_record::Draft;
use darkbot_bsc_record::schema::bsc_runtime::runtime_status::live as l;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    VenueBuyTerms, WITNESSED_MAINNET_TERMS,
};
use darkbot_bsc_trading::intent::IntentState;
use darkbot_bsc_trading::venues::flap::deployment::{ValidatedDeployment, bsc_mainnet_v5_27_2};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;
use std::time::{Duration, Instant};

/// Where the LIVE path's requests go (`BSC_LIVE_ROUTE`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveRouteConfig {
    /// The granted builders over TLS; the BlockRazor authorization from a
    /// private file, if BlockRazor is granted.
    Builders { blockrazor_token: Option<PathBuf> },
    /// The builder stand-in on 127.0.0.1:`port`: nothing leaves the host.
    Loopback { port: u16 },
}

/// BSC3-026: the stand-in route never runs on the account's owner state
/// (`canonical`, the one the GUI and the signing tools read): its attempts,
/// spend and ledger stay the test's. Fail-closed when that state cannot be
/// determined.
pub fn loopback_refused(state: &Path, canonical: Result<PathBuf, String>) -> Option<String> {
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    match canonical {
        Err(e) => Some(format!(
            "LOOPBACK_STATE_UNVERIFIED: the account's owner state is unknown ({e})"
        )),
        Ok(c) if real(&c) == real(state) => Some(format!(
            "LOOPBACK_ON_ACCOUNT_STATE: {} is the account's owner state",
            state.display()
        )),
        Ok(_) => None,
    }
}

impl LiveRouteConfig {
    /// `BUILDERS` or `LOOPBACK:<port>`.
    pub fn parse(text: &str, blockrazor_token: Option<PathBuf>) -> Result<Self, String> {
        if text == "BUILDERS" {
            return Ok(Self::Builders { blockrazor_token });
        }
        if blockrazor_token.is_some() {
            return Err("BSC_LIVE_BLOCKRAZOR_TOKEN_FILE needs BSC_LIVE_ROUTE=BUILDERS".into());
        }
        text.strip_prefix("LOOPBACK:")
            .and_then(|p| p.parse::<u16>().ok())
            .filter(|p| *p > 0)
            .map(|port| Self::Loopback { port })
            .ok_or_else(|| format!("BSC_LIVE_ROUTE {text:?}: BUILDERS or LOOPBACK:<port>"))
    }
}

/// The LIVE path's configuration (`BSC_LIVE_*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveSetup {
    /// The buyer's key file (absolute after `~` expansion).
    pub key_path: PathBuf,
    pub buyer: [u8; 20],
    pub route: LiveRouteConfig,
    pub connect_timeout: Duration,
}

/// Where the LIVE path stands.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Off,
    /// The owner's gate or the qualification refuses LIVE now.
    Blocked(String),
    NotWatched,
    WaitingC2,
    Probing,
    ProbeFailed(String),
    DeploymentDrift(String),
    TermsDrift(String),
    EconomicsUnqualified(String),
    Unfunded(String),
    WaitingNonce,
    KeyRefused(String),
    /// BSC3-025: the builders' connections are being established off the
    /// decision loop; a later pass arms once they are ready.
    Connecting,
    ConnectFailed(String),
    ArmFailed(String),
    Armed,
    /// Armed until the gate closed: the reason it ended.
    Ended(String),
}

/// BSC3-025: how long an end being connected waits for a pass that arms
/// it: Q5A's connect and handshake limits and its idle time, after which
/// its connections are of no use any more.
fn connecting_hold() -> Duration {
    Q5A_TLS_POLICY.connect_timeout + Q5A_TLS_POLICY.handshake_timeout + Q5A_TLS_POLICY.max_idle
}

impl Phase {
    /// A state only a closed and reopened gate leaves.
    fn terminal(&self) -> bool {
        matches!(
            self,
            Self::ProbeFailed(_)
                | Self::DeploymentDrift(_)
                | Self::TermsDrift(_)
                | Self::EconomicsUnqualified(_)
                | Self::KeyRefused(_)
                | Self::ArmFailed(_)
                | Self::Ended(_)
        )
    }

    fn name(&self) -> (&'static str, Option<&str>) {
        match self {
            Self::Off => ("OFF", None),
            Self::Blocked(r) => ("BLOCKED", Some(r)),
            Self::NotWatched => ("NOT_WATCHED", None),
            Self::WaitingC2 => ("WAITING_C2", None),
            Self::Probing => ("PROBING", None),
            Self::ProbeFailed(r) => ("PROBE_FAILED", Some(r)),
            Self::DeploymentDrift(r) => ("DEPLOYMENT_DRIFT", Some(r)),
            Self::TermsDrift(r) => ("TERMS_DRIFT", Some(r)),
            Self::EconomicsUnqualified(r) => ("ECONOMICS_UNQUALIFIED", Some(r)),
            Self::Unfunded(r) => ("UNFUNDED", Some(r)),
            Self::WaitingNonce => ("WAITING_NONCE", None),
            Self::KeyRefused(r) => ("KEY_REFUSED", Some(r)),
            Self::Connecting => ("CONNECTING", None),
            Self::ConnectFailed(r) => ("CONNECT_FAILED", Some(r)),
            Self::ArmFailed(r) => ("ARM_FAILED", Some(r)),
            Self::Armed => ("ARMED", None),
            Self::Ended(r) => ("ENDED", Some(r)),
        }
    }
}

/// What the ledger's new facts did to a position, for Strategy C.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionChange {
    /// The BUY was included: `tokens` for `spent_wei`, its block at
    /// `filled_ms`.
    Opened {
        token: [u8; 20],
        tokens: U256,
        spent_wei: U256,
        filled_ms: u64,
    },
    /// Its SELL was included, or its BUY reverted or left the chain.
    Ended { token: [u8; 20] },
}

/// What one block showed of a buyer transaction, until the block is
/// complete.
#[derive(Clone, Debug)]
struct Seen {
    tx_hash: [u8; 32],
    /// Its transaction index in the block (REV.1.0 R2: its placement).
    index: u64,
    succeeded: bool,
    gas_wei: Option<U256>,
    tokens: Option<U256>,
    eth_wei: Option<U256>,
    /// BSC3-076: the unresolved lease at its nonce this transaction is the
    /// BUY of (its fact lost in a crash after its dispatch), and the token
    /// the Portal traded in it.
    adopts: Option<Fact>,
    token: Option<[u8; 20]>,
    /// The Portal traded in it a BUY of the buyer's own (not a SELL, not
    /// another trader's).
    bought: bool,
}

/// REV.1.0 R2: where an included C3 BUY at `own_index` landed against its
/// token's launch (at `launch_index` in its block, none: not in it): no
/// token received NO_TOKENS, its launch not in its block OTHER_BLOCK,
/// another trader's BUY of the token between them LATE, else FIRST.
fn placement_of(
    launch_index: Option<u64>,
    own_index: u64,
    buys: &[([u8; 20], u64, [u8; 20])],
    token: [u8; 20],
    buyer: [u8; 20],
    received: bool,
) -> crate::live_ledger::Placement {
    use crate::live_ledger::{Placement, PlacementVerdict};
    let buys_before = launch_index.map_or(0, |launch| {
        buys.iter()
            .filter(|(t, i, trader)| {
                *t == token && *i > launch && *i < own_index && *trader != buyer
            })
            .count() as u64
    });
    let verdict = match (received, launch_index, buys_before) {
        (false, _, _) => PlacementVerdict::NoTokens,
        (true, None, _) => PlacementVerdict::OtherBlock,
        (true, Some(_), 0) => PlacementVerdict::First,
        (true, Some(_), _) => PlacementVerdict::Late,
    };
    Placement {
        verdict,
        launch_index,
        own_index,
        buys_before,
    }
}

/// The components a grant of `strategies` arms
/// ([`LiveStrategies::components`]).
fn components(strategies: LiveStrategies) -> Vec<ComponentId> {
    strategies.components()
}

/// The BlockRazor authorization from a private file outside `repo_root`,
/// held as the LIVE key is (R10: [`crate::paper::read_private_text`]).
fn token_from(path: &Path, repo_root: &Path) -> Result<AuthorizationToken, String> {
    let text = crate::paper::read_private_text(path, repo_root)
        .map_err(|e| crate::live_key::refused_as("BSC_LIVE_BLOCKRAZOR_TOKEN_FILE", e))?;
    AuthorizationToken::new(text.trim().to_string()).map_err(|e| format!("{e:?}"))
}

/// Why the executor configured as `x` may not sign under `grant`
/// (BSC3-069): beyond its execution risk, or a grant without one.
fn risk_refusal(
    grant: &crate::sources::LiveGrant,
    x: &crate::paper_exec::ExecConfig,
) -> Option<String> {
    let Some(risk) = grant.risk else {
        return Some(crate::sources::LIVE_GRANT_RISK_MISSING.into());
    };
    risk.admits(
        &crate::sources::SignedRisk {
            buy_slippage_bps: x.slippage_bps,
            exit_slippage_bps: x.exit_slippage_bps,
            gas_price_wei: x.gas_price_wei,
            buy_gas_limit: x.gas_limit,
            approve_gas_limit: x.approve_gas_limit,
            sell_gas_limit: x.sell_gas_limit,
        },
        grant.exit_only,
    )
    .err()
}

/// R4 (BSC3-069): why `grant` admits no BUY of the executor configured as
/// `x` as an outlier: its risk above a soft bound, not confirmed as one in
/// its scope, or confirmed with BSC_EXEC_LIVE_RISK_OUTLIER OFF. An
/// exit-only grant (no BUY) is none; exits go on either way.
fn outlier_refusal(
    grant: &crate::sources::LiveGrant,
    x: &crate::paper_exec::ExecConfig,
) -> Option<String> {
    let risk = grant.risk.filter(|_| !grant.exit_only)?;
    let outlier = risk.max_buy_slippage_bps > x.live_risk_soft_buy_slippage_bps
        || risk.max_exit_slippage_bps > x.live_risk_soft_exit_slippage_bps
        || risk.max_gas_price_wei > x.live_risk_soft_gas_price_wei;
    match (outlier, risk.outlier_ack, x.live_risk_outlier) {
        (true, false, _) => Some(crate::sources::LIVE_RISK_OUTLIER_UNACKED.into()),
        (true, true, false) => Some(crate::sources::LIVE_RISK_OUTLIER_LOCKED.into()),
        _ => None,
    }
}

fn wall_ms() -> u64 {
    // Unreadable: every grant counts as expired (BSC3-015: counted).
    crate::runtime::wall_or_expired(darkbot_bsc_record::observed_now())
}

/// R5 (BSC3-030): a lost exit step's price `prior` raised by `bps`, at most
/// `max`: the re-price, when above both `prior` and the arm's price `arm`
/// (a higher arm price, or another gas limit, are new bytes already; the
/// prior is derived with today's limits).
fn repriced(prior: u64, bps: u16, arm: u64, max: u64) -> Option<u64> {
    let raise = (u128::from(prior) * u128::from(bps)).div_ceil(10_000);
    let price = u64::try_from(u128::from(prior) + raise)
        .unwrap_or(u64::MAX)
        .min(max);
    (price > prior && price > arm).then_some(price)
}

/// Whether an armed path is renewed at the buyer's free nonce `free`
/// (BSC3-048), its strategy's next nonce being `cursor`: an earlier arm's
/// transaction took nonces this arm did not plan (`free` above `cursor`);
/// an unresolved transaction's window is over and this arm cannot take its
/// nonce again; or a transaction of this arm whose hand-over was UNKNOWN
/// (`gap`: its sequence stopped) is resolved and the free nonce is the
/// next one again.
fn rearm_due(free: u64, cursor: u64, gap: bool, unresolved_at_free: bool) -> bool {
    free > cursor || (free != cursor && unresolved_at_free) || (free == cursor && gap)
}

/// The runtime's LIVE path controller.
/// 01.11. (R1): who decides the open lots' exits, as the runtime last told.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ExitDeciders {
    undecided: Vec<[u8; 20]>,
    stranded: Vec<([u8; 20], &'static str)>,
    /// The curve reads that failed, and the last failure.
    faults: u64,
    fault: Option<String>,
}

/// REV.1.0 R3b: the runtime's receipt recovery of the ledger's unresolved
/// transactions (BSC3-024/041): each asked about and not resolved yet with
/// why, the transactions it resolved, its failed reads and the last
/// failure.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecoveryView {
    pub pending: Vec<([u8; 32], &'static str)>,
    pub recovered: u64,
    pub faults: u64,
    pub fault: Option<String>,
}

/// BSC3-075, Audit E 5.6: the launches waiting for a granted source, at
/// most `bound`, the oldest first out; membership and removal O(1). Each
/// held hash keeps the number of its arrival, and the arrival order its
/// entries (hash, number): an entry whose number is no longer the held one
/// (taken out, or held again since) lets nothing go at the front, and the
/// order drops such entries when it reaches twice the bound, so it never
/// holds more than that.
#[derive(Debug, Default)]
struct Awaiting {
    order: std::collections::VecDeque<([u8; 32], u64)>,
    held: std::collections::HashMap<[u8; 32], u64>,
    arrivals: u64,
}

impl Awaiting {
    /// Holds `hash` (once), first letting the oldest go at the bound.
    fn insert(&mut self, hash: [u8; 32], bound: usize) {
        let bound = bound.max(1);
        if self.held.contains_key(&hash) {
            return;
        }
        while self.held.len() >= bound {
            match self.order.pop_front() {
                Some((oldest, arrival)) => {
                    if self.held.get(&oldest) == Some(&arrival) {
                        self.held.remove(&oldest);
                    }
                }
                None => break,
            }
        }
        if self.order.len() >= 2 * bound {
            let held = &self.held;
            self.order.retain(|(h, n)| held.get(h) == Some(n));
        }
        self.arrivals += 1;
        self.order.push_back((hash, self.arrivals));
        self.held.insert(hash, self.arrivals);
    }

    /// Takes `hash` out; whether it was held.
    fn take(&mut self, hash: &[u8; 32]) -> bool {
        self.held.remove(hash).is_some()
    }

    fn clear(&mut self) {
        self.order.clear();
        self.held.clear();
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.held.len()
    }
}

pub struct LiveRuntime {
    setup: LiveSetup,
    exec: ExecSetup,
    repo_root: PathBuf,
    state_dir: PathBuf,
    fence_dir: PathBuf,
    /// The PAPER identity's address: never the LIVE key.
    paper_sender: Option<[u8; 20]>,
    phase: Phase,
    probe: Option<Receiver<Result<VenueObservation, String>>>,
    observation: Option<VenueObservation>,
    validated: Option<(ValidatedDeployment, VenueBuyTerms)>,
    executor: Option<Executor<LiveEnd>>,
    /// BSC3-025: an end whose connections are being established, and since
    /// when: kept across passes for at most [`connecting_hold`] (a closed
    /// gate or a stop drops it at once, its key with it).
    connecting: Option<(LiveEnd, Instant)>,
    /// BSC3-012: the SHA-256 of this process's build, read at the start.
    build: Result<[u8; 32], String>,
    codehash_generation: u64,
    arms: u64,
    last_probe: Option<Instant>,
    /// When an arming last failed in a way a later pass may get past.
    last_attempt: Option<Instant>,
    probes: u64,
    /// BSC3-056: the runtime's last verdict of the second source, handed to
    /// every end armed (`None`: no second source).
    confirmation: Option<crate::canonical_confirm::Confirmation>,
    /// R5 (BSC3-030): the public exit route's worker (started at its first
    /// exit step), the follower's bounds it is started with (the last
    /// tick's), and its counts across workers.
    public_exit: Option<crate::public_exit::PublicExit>,
    follower_limits: Option<FollowerLimits>,
    public_counts: crate::public_exit::PublicCounts,
    disarm_errors: u64,
    next_nonce: Option<u64>,
    /// Launches first seen by a source the grant does not name.
    source_refused: u64,
    /// Launches first seen by a granted source that is not effective LIVE
    /// (the operator holds it in SHADOW, say).
    source_not_live: u64,
    /// R4 (PHASENPLAN §20): launches while every LIVE source is public.
    public_only_refused: u64,
    /// R6: each source's health and observed capability at the runtime's
    /// last status pass (none before it: no non-public signal yet), and
    /// the launches refused while every non-public LIVE source was down.
    source_signals: Vec<(
        SourceId,
        crate::ingress::Health,
        crate::sources::SourceCapability,
    )>,
    source_unhealthy: u64,
    /// The grant's sources effective LIVE at the last tick (the owner's
    /// [`crate::sources::LiveNow::sources_live`]); none before a tick.
    sources_live: Vec<SourceId>,
    /// 01.11. (R1): who decides the open lots' exits (the runtime tells;
    /// none yet: every open lot undecided). No new BUY while an open lot is
    /// undecided or stranded (an exit-less position would grow).
    exit_deciders: Option<ExitDeciders>,
    /// Strategy C BUYs refused because the grant names no STRATEGY_C.
    strategy_buys_refused: u64,
    /// REV.1.0 R2: the providers' answers read and not yet journaled (here,
    /// not in the end: a disarm between a poll and the runtime's take drops
    /// none).
    answers: Vec<crate::live_end::Answer>,
    /// REV.1.0 R2: this block's launches (token, transaction index,
    /// creator) and other traders' BUYs of their tokens (token, index,
    /// trader): a C3 BUY's placement at the block's end.
    block_launches: Vec<([u8; 20], u64, Option<[u8; 20]>)>,
    block_buys: Vec<([u8; 20], u64, [u8; 20])>,
    /// BSC3-075: those launches, until a granted source sees them (at most
    /// as many as the ingress remembers, `BSC_RUNTIME_DEDUP_HASHES`: a later
    /// sighting comes only of a launch it still holds; the oldest first out).
    awaiting_granted: Awaiting,
    awaiting_bound: usize,
    /// BSC3-076: the attempt of the C3 BUY lease last returned unused,
    /// with its grant's generation: the next lease under that grant takes
    /// it (its slots are taken).
    lease_attempt: Option<(u64, [u8; 32])>,
    /// Leases the owner refused or that could not be returned, the last why.
    lease_refused: u64,
    lease_error: Option<String>,
    /// Transactions of the buyer's key at a recovered lease's nonce that
    /// were no BUY of it (BSC3-076 review H3): the lease replaced.
    lease_foreign: u64,
    /// 31.10. order: BUYs the grant's limits refused, per reason, and when
    /// this process last handed a BUY over (unix ms).
    limit_refused: Vec<(&'static str, u64)>,
    last_buy_ms: Option<u64>,
    /// When this process's LIVE path began (unix ms): a BUY still pending
    /// from before was handed over at the latest then.
    started_ms: u64,
    /// BSC3-030 review: strategy exits refused because the position's exits
    /// stopped for the operator, per reason.
    exits_stopped: Vec<(&'static str, u64)>,
    prober: Prober,
    /// The owner's boot session (tests: a fixed one) and wall clock.
    boot: Option<String>,
    wall: fn() -> u64,
    ledger: Ledger,
    /// The armed end's ledger facts already folded.
    folded_facts: u64,
    /// The block being applied: number, hash, time; its buyer transactions.
    block: Option<(u64, [u8; 32], u64)>,
    seen: Vec<Seen>,
    /// The ledger's fold, and why it could not be read.
    book: Book,
    ledger_error: Option<String>,
    /// A fact this process could not write: the book may miss it, so no
    /// arm follows and no fence settles until a restart recovers what the
    /// admission entries hold (BSC3-024/041).
    ledger_write_failed: Option<String>,
    /// The lot of each token reported open (to the executor and Strategy
    /// C), by its BUY's hash (BSC3-047): one per token at a time.
    opened: Vec<([u8; 20], [u8; 32])>,
    /// Open positions whose exit the strategy decided and whose SELL has
    /// not gone: the exit continues once the sender's nonce order allows
    /// (BSC3-055), across arms of this process.
    exits_wanted: Vec<[u8; 20]>,
    /// The buyer's transactions of the block being applied (hash, nonce):
    /// what took which nonce (BSC3-048).
    nonces_taken: Vec<([u8; 32], u64)>,
    /// Why the owner's gate refuses BUYs while it still admits the exits of
    /// open positions (BSC3-071).
    buys_closed: Option<String>,
    /// BUYs refused while an exit waits (BSC3-048), and while the gate
    /// admits exits only.
    exit_first: u64,
    buys_refused: u64,
    /// The buyer's balance as the last probe read it (BSC3-027).
    funds: Option<Funds>,
    /// BUYs the balance does not hold with their exit and the open exits.
    unfunded_refused: u64,
    /// BUYs refused since a provider refused one of the arm's (BSC3-051).
    provider_refused: u64,
    /// REV.1.0 R3: the ledger's facts the book folds (another process's,
    /// the operator's, are folded at the next reconcile).
    folded_len: usize,
    /// BSC3-068: the attempts whose slots went back in this process, and
    /// the returns that failed (tried again), the last why.
    returned: std::collections::BTreeSet<[u8; 32]>,
    slot_return_failed: u64,
    slot_return_error: Option<String>,
    /// R3: each lot's RESUMED facts acted on in this process, the lots
    /// whose EXIT_REQUESTED was, and those the operator exit switch held
    /// back; what the strategy is still to be told (lots no longer open,
    /// lots resumed).
    resumes_seen: Vec<([u8; 32], u64)>,
    exit_requests_done: std::collections::BTreeSet<[u8; 32]>,
    operator_exits_locked: u64,
    decided: (Vec<PositionChange>, Vec<[u8; 20]>),
    /// R3 (BSC3-045): the highest owner generation an entry of the ledger
    /// was quarantined at (unreadable: every one).
    quarantine: Option<u64>,
    /// R3b: the source of the chain facts written now (a recovered block's:
    /// RECEIPT; the follower's block: none), and the recovery's view.
    source: Option<FactSource>,
    recovery: RecoveryView,
    /// R3b: the requests the switch held back (counted once each); the
    /// burns refused at their hand-over (the burn wanted is the book's,
    /// [`Self::burn_target`]).
    burns_locked: std::collections::BTreeSet<[u8; 32]>,
    burns_refused: u64,
    /// BUYs refused because a burn went first.
    burn_first: u64,
}

/// The buyer's balance at a probe's block (number and hash): a
/// transaction of the ledger included or reverted in a later block, or not
/// yet, still comes out of it at its most.
#[derive(Clone, Copy, Debug)]
struct Funds {
    balance: U256,
    block: u64,
    hash: [u8; 32],
}

impl LiveRuntime {
    pub fn new(
        setup: LiveSetup,
        exec: ExecSetup,
        repo_root: &Path,
        state_dir: PathBuf,
        fence_dir: PathBuf,
        paper_sender: Option<[u8; 20]>,
    ) -> Self {
        Self::with_prober(
            setup,
            exec,
            repo_root,
            state_dir,
            fence_dir,
            paper_sender,
            live_probe,
        )
    }

    /// [`LiveRuntime::new`] with another read-only deployment probe (a
    /// synthetic deployment in the non-sending E2E).
    #[allow(clippy::too_many_arguments)]
    pub fn with_prober(
        setup: LiveSetup,
        exec: ExecSetup,
        repo_root: &Path,
        state_dir: PathBuf,
        fence_dir: PathBuf,
        paper_sender: Option<[u8; 20]>,
        prober: Prober,
    ) -> Self {
        let ledger = Ledger::open(&state_dir);
        LiveRuntime {
            setup,
            exec,
            repo_root: repo_root.to_path_buf(),
            state_dir,
            fence_dir,
            paper_sender,
            phase: Phase::Off,
            probe: None,
            observation: None,
            validated: None,
            executor: None,
            codehash_generation: 0,
            arms: 0,
            last_probe: None,
            last_attempt: None,
            connecting: None,
            build: live_qualification::running_build(),
            probes: 0,
            confirmation: None,
            public_exit: None,
            follower_limits: None,
            public_counts: crate::public_exit::PublicCounts::default(),
            disarm_errors: 0,
            next_nonce: None,
            source_refused: 0,
            source_not_live: 0,
            public_only_refused: 0,
            source_signals: Vec::new(),
            source_unhealthy: 0,
            sources_live: Vec::new(),
            exit_deciders: None,
            strategy_buys_refused: 0,
            answers: Vec::new(),
            block_launches: Vec::new(),
            block_buys: Vec::new(),
            awaiting_granted: Awaiting::default(),
            awaiting_bound: 0,
            lease_attempt: None,
            lease_refused: 0,
            lease_error: None,
            lease_foreign: 0,
            limit_refused: Vec::new(),
            last_buy_ms: None,
            started_ms: wall_ms(),
            exits_stopped: Vec::new(),
            prober,
            boot: None,
            wall: wall_ms,
            ledger,
            folded_facts: 0,
            block: None,
            seen: Vec::new(),
            book: Book::default(),
            ledger_error: None,
            ledger_write_failed: None,
            opened: Vec::new(),
            exits_wanted: Vec::new(),
            nonces_taken: Vec::new(),
            buys_closed: None,
            exit_first: 0,
            buys_refused: 0,
            funds: None,
            unfunded_refused: 0,
            provider_refused: 0,
            folded_len: 0,
            returned: Default::default(),
            slot_return_failed: 0,
            slot_return_error: None,
            resumes_seen: Vec::new(),
            exit_requests_done: Default::default(),
            operator_exits_locked: 0,
            decided: Default::default(),
            quarantine: None,
            source: None,
            recovery: RecoveryView::default(),
            burns_locked: Default::default(),
            burns_refused: 0,
            burn_first: 0,
        }
        .folded()
    }

    /// At the start, nothing armed: what the last process admitted and
    /// could not write is recovered, then the ledger folded.
    fn folded(mut self) -> Self {
        // R3 (BSC3-045): what a crashed append left unpublished goes.
        if let Err(e) = self.ledger.clear_pending() {
            self.ledger_error = Some(e);
        }
        let _ = self.reconciled(wall_ms());
        // R3: the lots open from before count as reported (their curve is
        // read and they are held again, R1): one that closes or is written
        // off ends for the strategy, before any reconcile too.
        for p in &self.book.positions {
            let open = self.book.open_lot(&p.token).is_some_and(|o| o.lot == p.lot);
            if open && !self.opened.iter().any(|(t, _)| *t == p.token) {
                self.opened.push((p.token, p.lot));
            }
        }
        self
    }

    /// With nothing armed: every admitted transaction is a ledger fact
    /// (recovered as UNKNOWN if the process ended before its outcome was
    /// written) and the ledger folds; otherwise why no arm may follow
    /// (BSC3-024/041). Which nonce an arm may take is [`Self::nonce_free`].
    fn reconciled(&mut self, now_ms: u64) -> Result<(), String> {
        let unreadable = |e: String| format!("{}: {e}", crate::sources::LIVE_LEDGER_UNREADABLE);
        let recovered = crate::sources::admitted_intents(&self.state_dir)
            .map(|admitted| self.ledger.recover(&admitted, now_ms));
        // The book as the ledger holds it, whatever else failed.
        self.refold();
        let recovered = recovered?.map_err(unreadable)?;
        if let Some(why) = self.ledger_fault() {
            return Err(why);
        }
        if let Some(tx) = recovered.unrecoverable.first() {
            return Err(format!(
                "{}: {} admitted without token and nonce, 0x{}",
                crate::sources::LIVE_RELEASE_UNRESOLVED,
                recovered.unrecoverable.len(),
                alloy_primitives::hex::encode(tx)
            ));
        }
        Ok(())
    }

    /// Whether a new transaction of the buyer may take C2's clean next
    /// nonce `next` at the canonical `head` (BSC3-041/048): every
    /// unresolved transaction of the ledger is at `next` and its window is
    /// known and over (no block after its last one includes its bundle; it
    /// stays unresolved, its spend counted, until the nonce is taken).
    /// Otherwise why not: a window that runs or is unknown, a lower nonce
    /// taken by a transaction not seen.
    fn nonce_free(&self, next: u64, head: Option<u64>) -> Result<(), String> {
        let blocking = self.book.unresolved().find(|t| {
            // R3b: a transaction of the buyer whose nonce is to be burnt, at
            // `next`, blocks nothing: the burn takes the nonce (review
            // LOW-1: every one requested there).
            let burnt = self.burnable(t) && t.released.nonce == next;
            !burnt && (t.released.nonce != next || !head.is_some_and(|head| t.window_over(head)))
        });
        let Some(t) = blocking else {
            return Ok(());
        };
        let (unresolved, n) = (crate::sources::LIVE_RELEASE_UNRESOLVED, t.released.nonce);
        let tx = alloy_primitives::hex::encode(t.released.tx_hash);
        Err(match t.released.max_block {
            _ if n != next => {
                format!("{unresolved}: nonce {n} (the next is {next}) not resolved, 0x{tx}")
            }
            Some(_) if t.orphaned => {
                format!("{unresolved}: nonce {n}, public since its block left the chain, 0x{tx}")
            }
            Some(last) => format!("{unresolved}: nonce {n}, its window to block {last}, 0x{tx}"),
            None => format!("{unresolved}: nonce {n}, its window unknown, 0x{tx}"),
        })
    }

    /// The nonce a new transaction of the buyer may take now: C2's clean
    /// next nonce when [`Self::nonce_free`] allows it and the ledger holds
    /// no nonce at or above it consumed.
    fn free_nonce(&self, owner: &StateOwner) -> Option<u64> {
        let next = owner.clean_next_nonce(&self.setup.buyer)?;
        let consumed = self.book.txs.iter().any(|t| {
            matches!(
                t.state,
                FactKind::Included | FactKind::Reverted | FactKind::Replaced
            ) && t.released.nonce >= next
        });
        let head = owner.head().map(|h| h.number);
        (!consumed && self.nonce_free(next, head).is_ok()).then_some(next)
    }

    /// An exit transaction (approval, SELL) whose window is over without
    /// inclusion: its position's exit is wanted again, at the same nonce
    /// (BSC3-048).
    fn want_expired_exits(&mut self, head: u64) {
        let expired: Vec<[u8; 20]> = self
            .book
            .unresolved()
            .filter(|t| t.released.intent.exit_step() && t.window_over(head))
            .map(|t| t.released.token)
            .collect();
        for token in expired {
            if self.position_open(&token) && !self.exits_wanted.contains(&token) {
                self.exits_wanted.push(token);
            }
        }
    }

    /// Whether the ledger holds `token`'s position open or exiting.
    fn position_open(&self, token: &[u8; 20]) -> bool {
        self.book.positions.iter().any(|p| {
            p.token == *token && matches!(p.state, PositionState::Open | PositionState::Exiting)
        })
    }

    /// 01.11. (R1): every token's open lot (its last lot open or exiting,
    /// as [`crate::live_ledger::Book::open_lot`]) as a position change:
    /// what its exit is held with. One pass over the ledger's positions.
    pub fn open_positions(&self) -> Vec<PositionChange> {
        let mut seen: Vec<[u8; 20]> = Vec::new();
        let mut out = Vec::new();
        for p in self.book.positions.iter().rev() {
            if seen.contains(&p.token) {
                continue;
            }
            if matches!(p.state, PositionState::Open | PositionState::Exiting) {
                seen.push(p.token);
                out.push(PositionChange::Opened {
                    token: p.token,
                    tokens: p.tokens,
                    spent_wei: p.input_wei + p.gas_wei,
                    filled_ms: p.opened_ms.unwrap_or_default(),
                });
            }
        }
        out
    }

    /// 01.11. (R1): the runtime's view of who decides the open lots' exits:
    /// `undecided` not held by the strategy yet (their curve being read),
    /// `stranded` never exitable by it (with why), and the curve reads'
    /// failures (`faults`, the last one). Until the runtime first tells it,
    /// every open lot counts undecided.
    pub fn set_exit_deciders(
        &mut self,
        undecided: Vec<[u8; 20]>,
        stranded: Vec<([u8; 20], &'static str)>,
        faults: u64,
        fault: Option<String>,
    ) {
        self.exit_deciders = Some(ExitDeciders {
            undecided,
            stranded,
            faults,
            fault,
        });
    }

    /// REV.1.0 R2: the creator of the launch of `token`'s open lot (its
    /// BUY's, from the ledger): the creator rule of its exit after a
    /// restart.
    pub fn launcher_of(&self, token: &[u8; 20]) -> Option<[u8; 20]> {
        let lot = self.book.open_lot(token)?.lot;
        self.book.tx(&lot)?.launcher
    }

    /// The Portal of the armed path's validated deployment (the curve
    /// reads of its open lots go there); none unarmed.
    pub fn portal(&self) -> Option<alloy_primitives::Address> {
        self.executor.as_ref().map(|x| x.portal())
    }

    /// Whether the path holds positions whose exits may still be needed:
    /// open, exiting or bought and not yet included, or an exit wanted
    /// (the runtime keeps the strategy, C2 and the sources on for them).
    pub fn holds_positions(&self) -> bool {
        self.positions_open()
            || self
                .book
                .positions
                .iter()
                .any(|p| p.state == PositionState::Pending)
    }

    /// Positions of the ledger still open (or exiting), or an exit wanted.
    fn positions_open(&self) -> bool {
        !self.exits_wanted.is_empty()
            || self
                .book
                .positions
                .iter()
                .any(|p| matches!(p.state, PositionState::Open | PositionState::Exiting))
    }

    /// The fences of earlier arms of this end (a crash, or an arm that
    /// ended with transactions unresolved), settled once a new arm may take
    /// the buyer's next nonce (everything they released resolved, or its
    /// window over: [`Self::nonce_free`]): renamed `.settled`, kept. The
    /// ledger keeps tracking what they released.
    fn settle_fences(&self) -> Result<(), String> {
        let prefix = format!("{}-", <LiveEnd as crate::paper_exec::End>::FENCE.0);
        let entries = match std::fs::read_dir(&self.fence_dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("{}: {e}", self.fence_dir.display())),
        };
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", self.fence_dir.display()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix) && name.ends_with(".fence") {
                let to = self.fence_dir.join(format!("{name}.settled"));
                std::fs::rename(entry.path(), &to)
                    .map_err(|e| format!("{}: {e}", entry.path().display()))?;
            }
        }
        std::fs::File::open(&self.fence_dir)
            .and_then(|d| d.sync_all())
            .map_err(|e| format!("{}: {e}", self.fence_dir.display()))
    }

    /// The ledger's fold now; an unreadable ledger is visible and counts
    /// nothing.
    fn refold(&mut self) {
        match self.ledger.facts() {
            Ok(facts) => {
                self.book = fold(&facts);
                self.folded_len = facts.len();
                self.ledger_error = None;
                self.quarantine = self.ledger.quarantined().unwrap_or(Some(u64::MAX));
            }
            Err(e) => {
                self.book = Book::default();
                self.ledger_error = Some(e);
            }
        }
    }

    fn append(&mut self, fact: &Fact) {
        if let Err(e) = self.ledger.append(fact, (self.wall)()) {
            self.ledger_write_failed = Some(e);
        }
    }

    /// Why the ledger cannot be trusted now: it does not read, or a fact
    /// of this process (or of the armed end) was not written.
    fn ledger_fault(&self) -> Option<String> {
        let end = self
            .executor
            .as_ref()
            .and_then(|x| x.end().ledger_failure());
        self.ledger_error
            .as_deref()
            .or(self.ledger_write_failed.as_deref())
            .or(end)
            .map(|e| format!("{}: {e}", crate::sources::LIVE_LEDGER_UNREADABLE))
    }

    /// A ledger fault ends an armed path at once (its fence stays); what
    /// the admission entries hold is recovered right away (BSC3-024/045).
    fn fail_closed_on_ledger(&mut self) {
        let Some(why) = self.ledger_fault() else {
            return;
        };
        if let Some(e) = self
            .executor
            .as_ref()
            .and_then(|x| x.end().ledger_failure())
        {
            self.ledger_write_failed = Some(e.to_string());
        }
        if self.executor.is_some() {
            self.close(why);
            let _ = self.reconciled((self.wall)());
        }
    }

    /// After a hand-over: the facts the armed end wrote are folded; a fact
    /// it could not write ends the arm.
    fn handed(&mut self) {
        let Some(x) = self.executor.as_ref() else {
            return;
        };
        let written = x.end().facts_written();
        if written != self.folded_facts {
            self.folded_facts = written;
            self.refold();
        }
        self.fail_closed_on_ledger();
    }

    /// REV.1.0 R3: facts another process wrote since the last fold (the
    /// operator's, `bsc_runtime live lot`/`adopt-g07`) are folded, and what
    /// they decided is acted on once: a lot no longer open is let go, an
    /// opened one reported; a RESUMED lot's exit opens again and is wanted
    /// (its curve read again, [`Self::take_decided`]); an EXIT_REQUESTED
    /// open lot's exit is wanted now when `BSC_EXEC_LIVE_OPERATOR_EXIT` is
    /// ON (else held back, counted). Outside the entry path.
    fn fold_decisions(&mut self) {
        let grown = self.ledger.fact_count().is_ok_and(|n| n != self.folded_len);
        if grown {
            self.refold();
        }
        // A quarantine another process wrote holds at once (its entry may
        // never have been read here).
        self.quarantine = self.ledger.quarantined().unwrap_or(Some(u64::MAX));
        // Review F3: every reconcile (a fold of another path may have taken
        // them in first); only what differs is reported.
        let changes = self.position_changes();
        self.decided.0.extend(changes);
        // An exit is wanted of an open lot only (one written off is none:
        // it holds no BUY back); a ledger that does not read says nothing.
        if self.ledger_error.is_none() {
            let book = &self.book;
            self.exits_wanted.retain(|t| book.open_lot(t).is_some());
        }
        // Each open lot: token, lot, its RESUMED count, exit requested, not
        // yet exiting.
        type Decided = ([u8; 20], [u8; 32], u64, bool, bool);
        let open: Vec<Decided> = self
            .book
            .positions
            .iter()
            .filter(|p| self.book.open_lot(&p.token).is_some_and(|o| o.lot == p.lot))
            .filter_map(|p| {
                let d = &self.book.tx(&p.lot)?.decided;
                Some((
                    p.token,
                    p.lot,
                    d.resumes,
                    d.exit_requested,
                    p.state == PositionState::Open,
                ))
            })
            .collect();
        for (token, lot, resumes, exit_requested, unsold) in open {
            let seen = self
                .resumes_seen
                .iter()
                .find(|(l, _)| *l == lot)
                .map_or(0, |(_, n)| *n);
            if resumes > seen {
                self.resumes_seen.retain(|(l, _)| *l != lot);
                self.resumes_seen.push((lot, resumes));
                let approval = !self.book.open_lot(&token).is_some_and(|p| p.approved);
                if let Some(x) = self.executor.as_mut() {
                    x.reopen_exit(token, approval);
                }
                self.decided.1.push(token);
                // An exit the strategy decided (it has steps) is wanted
                // again; a lot without one is read and held again first.
                let decided = self
                    .book
                    .txs
                    .iter()
                    .any(|t| t.lot == Some(lot) && t.released.intent.exit_step());
                if decided {
                    self.want_exit(token);
                }
            }
            if exit_requested && unsold && self.exit_requests_done.insert(lot) {
                match self.exec.exec.live_operator_exit {
                    true => self.want_exit(token),
                    false => self.operator_exits_locked += 1,
                }
            }
        }
        // R3b: a burn the operator requested is held back, counted, while
        // the switch is OFF (with it ON, [`Self::burn_target`]).
        if !self.exec.exec.live_nonce_burn {
            let requested: Vec<[u8; 32]> = self
                .book
                .unresolved()
                .filter(|t| t.decided.burn_requested && self.buyers(t))
                .map(|t| t.released.tx_hash)
                .collect();
            self.burns_locked.extend(requested);
        }
    }

    /// R3: what the operator decided since the last call, for the
    /// strategy: the lots no longer open (let go) or opened (held), and
    /// the tokens of lots resumed (their curve read again).
    pub fn take_decided(&mut self) -> (Vec<PositionChange>, Vec<[u8; 20]>) {
        std::mem::take(&mut self.decided)
    }

    /// BSC3-068: the slots of every attempt no byte of which left (each of
    /// its ledger facts WITHDRAWN, its BUY's own among them,
    /// [`Book::withdrawn_attempts`]; none after a quarantine) go back to
    /// the owner, once each in this process and again after a restart (a
    /// return a crash cut off is completed); one a lease holds stays (its
    /// BUY may still come). A return that fails is tried again.
    fn return_withdrawn(&mut self, store: &Store) {
        // Review F1: a quarantined entry may have been a BUY's own fact (a
        // lease's alone looks withdrawn): no slot goes back after one.
        if self.quarantine.is_some() {
            return;
        }
        let held: Vec<[u8; 32]> = self
            .executor
            .as_ref()
            .and_then(|x| x.end().lease().map(|l| l.attempt()))
            .into_iter()
            .chain(self.lease_attempt.map(|(_, a)| a))
            .collect();
        for attempt in self.book.withdrawn_attempts() {
            if self.returned.contains(&attempt) || held.contains(&attempt) {
                continue;
            }
            match store.return_attempt(&alloy_primitives::hex::encode(attempt)) {
                Ok(_) => {
                    self.returned.insert(attempt);
                }
                Err(why) => {
                    self.slot_return_failed += 1;
                    self.slot_return_error = Some(why);
                }
            }
        }
    }

    /// One canonical event: the buyer's transactions of a block become
    /// ledger facts once the block is complete (status, gas, the Portal's
    /// trade); a reorg's orphaned blocks take their inclusions back. What
    /// that did to the positions, for Strategy C.
    pub fn on_canonical(&mut self, event: &CanonicalEvent) -> Vec<PositionChange> {
        let buyer = self.setup.buyer;
        match event {
            CanonicalEvent::Block {
                number,
                hash,
                timestamp_ms,
                txs,
                ..
            } => {
                self.block = Some((*number, *hash, *timestamp_ms));
                self.block_launches.clear();
                self.block_buys.clear();
                self.nonces_taken = txs
                    .iter()
                    .filter(|t| t.sender == buyer)
                    .map(|t| (t.hash, t.nonce))
                    .collect();
                // BSC3-076: the buyer's transaction at the nonce of an
                // unresolved lease (recovered as UNKNOWN: the process ended
                // between its BUY's dispatch and its fact) is that BUY; only
                // the buyer's key signs at the buyer's nonce.
                // R3b review: of two at one nonce (a renewal whose return
                // failed), the one whose window holds the block.
                let lease_at = |nonce: u64| {
                    let leases = || {
                        self.book.unresolved().filter(|u| {
                            u.released.nonce == nonce
                                && u.released.intent == LiveIntentKind::Buy
                                && u.released.token == [0; 20]
                        })
                    };
                    leases()
                        .find(|u| u.released.max_block.is_some_and(|last| *number <= last))
                        .or_else(|| leases().next())
                        .map(|u| u.released.clone())
                };
                self.seen = txs
                    .iter()
                    .filter(|t| t.sender == buyer)
                    .filter_map(|t| {
                        let adopts = match self.book.tx(&t.hash) {
                            Some(_) => None,
                            None => Some(lease_at(t.nonce)?),
                        };
                        Some(Seen {
                            tx_hash: t.hash,
                            index: t.index,
                            succeeded: t.succeeded,
                            gas_wei: t.cost.map(|c| U256::from(c.wei())),
                            tokens: None,
                            eth_wei: None,
                            adopts,
                            token: None,
                            bought: false,
                        })
                    })
                    .collect();
                Vec::new()
            }
            CanonicalEvent::FlapTrade(t) if t.origin == buyer => {
                if let Some(s) = self.seen.iter_mut().find(|s| s.tx_hash == t.tx_hash) {
                    s.tokens = Some(U256::from_be_bytes(t.amount));
                    s.eth_wei = Some(U256::from_be_bytes(t.eth));
                    s.token = Some(t.token);
                    s.bought = !t.sell && t.trader == buyer;
                }
                Vec::new()
            }
            // REV.1.0 R2: another trader's BUY of a token launched in this
            // block (a C3 BUY's placement counts it, when it came first).
            CanonicalEvent::FlapTrade(t)
                if !t.sell && self.block_launches.iter().any(|l| l.0 == t.token) =>
            {
                self.block_buys.push((t.token, t.tx_index, t.trader));
                Vec::new()
            }
            CanonicalEvent::Launch(l) => {
                if let Some(token) = l.token {
                    self.block_launches.push((token, l.tx_index, l.creator));
                }
                Vec::new()
            }
            CanonicalEvent::BlockComplete { .. } => {
                let (Some(block), seen) = (self.block.take(), std::mem::take(&mut self.seen))
                else {
                    return Vec::new();
                };
                let taken = std::mem::take(&mut self.nonces_taken);
                let source = self.source;
                let mut facts: Vec<Fact> = Vec::new();
                for s in &seen {
                    // BSC3-076 review H3: adopted only as what the lease's
                    // BUY can be: in its window, a Portal BUY of the buyer's
                    // own of a token, within the lease's input (or reverted,
                    // no trade). Anything else of the buyer's key at that
                    // nonce is no BUY of the lease: it replaces the lease
                    // (below), counted for the operator.
                    let adoptable = |lease: &Fact| {
                        let in_window = lease.max_block.is_some_and(|last| block.0 <= last);
                        let input = lease.input_wei.unwrap_or_default();
                        let bought = s.bought
                            && s.token.is_some_and(|t| t != [0; 20])
                            && s.eth_wei.is_some_and(|eth| eth <= input);
                        in_window && (bought || !s.succeeded)
                    };
                    let released = match (&s.adopts, self.book.tx(&s.tx_hash)) {
                        (Some(lease), _) if !adoptable(lease) => {
                            self.lease_foreign += 1;
                            continue;
                        }
                        // The lease's BUY, released as the lease held it
                        // (its nonce, budget, window), its token the
                        // Portal's trade (none: reverted, no position).
                        (Some(lease), _) => {
                            let r = Fact {
                                kind: FactKind::Released,
                                tx_hash: s.tx_hash,
                                token: s.token.unwrap_or([0; 20]),
                                ..lease.clone()
                            };
                            facts.push(r.clone());
                            r
                        }
                        // R3b: a recovered block resolves only what is not
                        // resolved yet (the follower's facts stand).
                        (None, Some(tx))
                            if source.is_some()
                                && matches!(
                                    tx.state,
                                    FactKind::Included | FactKind::Reverted | FactKind::Replaced
                                ) =>
                        {
                            continue;
                        }
                        (None, Some(tx)) => tx.released.clone(),
                        (None, None) => continue,
                    };
                    let kind = if s.succeeded {
                        FactKind::Included
                    } else {
                        FactKind::Reverted
                    };
                    // REV.1.0 R2 review M2: an included BUY acquired only
                    // what the buyer's own Portal BUY of its token received;
                    // any other trade of the transaction acquired nothing.
                    let own_buy = s.bought
                        && s.token.is_some()
                        && (released.token == [0; 20] || s.token == Some(released.token));
                    let tokens = match (released.intent, s.succeeded) {
                        (LiveIntentKind::Buy, true) if own_buy => s.tokens,
                        (LiveIntentKind::Buy, true) => Some(U256::ZERO),
                        _ => s.tokens,
                    };
                    facts.push(Fact {
                        block: Some(block),
                        gas_wei: s.gas_wei,
                        tokens,
                        eth_wei: s.eth_wei,
                        source,
                        ..Fact::about(&released, kind)
                    });
                    // REV.1.0 R2: where an included C3 BUY landed against
                    // its launch, a durable fact of its own.
                    if s.succeeded
                        && released.intent == LiveIntentKind::Buy
                        && released.component == ComponentId::ExecC3
                    {
                        let token = s.token.unwrap_or(released.token);
                        let launch = self.block_launches.iter().find(|l| l.0 == token);
                        let placement = placement_of(
                            launch.map(|l| l.1),
                            s.index,
                            &self.block_buys,
                            token,
                            buyer,
                            own_buy && s.tokens.is_some_and(|n| !n.is_zero()),
                        );
                        facts.push(Fact {
                            block: Some(block),
                            placement: Some(placement),
                            launcher: launch.and_then(|l| l.2),
                            ..Fact::about(&released, FactKind::Placed)
                        });
                    }
                }
                // Another transaction of the buyer took an unresolved one's
                // nonce: it never happened. The fact names which one, and
                // where (BSC3-048).
                for (by, nonce) in &taken {
                    facts.extend(
                        self.book
                            .unresolved()
                            .filter(|t| t.released.nonce == *nonce && t.released.tx_hash != *by)
                            .map(|t| Fact {
                                block: Some(block),
                                by_tx: Some(*by),
                                source,
                                ..Fact::about(&t.released, FactKind::Replaced)
                            }),
                    );
                }
                self.record(&facts)
            }
            CanonicalEvent::Reorg { orphaned, .. }
            | CanonicalEvent::ReorgTooDeep { orphaned, .. } => {
                let facts: Vec<Fact> = self
                    .book
                    .txs
                    .iter()
                    .filter_map(|t| {
                        let inclusion = t.inclusion.as_ref()?;
                        let (n, h, _) = inclusion.block?;
                        orphaned.contains(&(n, h)).then(|| Fact {
                            block: inclusion.block,
                            ..Fact::about(&t.released, FactKind::Orphaned)
                        })
                    })
                    .collect();
                self.record(&facts)
            }
            _ => Vec::new(),
        }
    }

    /// REV.1.0 R3b (BSC3-024/041): what the receipt recovery asks the chain
    /// about at the followed `head`, keyed by the transaction's ledger
    /// hash: each unresolved transaction whose window is over, unknown or
    /// bounds nothing any more (its block left the chain). One whose hash
    /// is a transaction's: by its hash, its window's end when it bounds; a
    /// lease's BUY (never included, its hash never known): by its window
    /// (the search starting `BSC_EXEC_LIVE_LEASE_BLOCKS` before its last
    /// block). Never asked, named why: a lease without its window, a
    /// transaction of another sender than the buyer (its block's facts are
    /// the buyer's only).
    pub fn recovery_targets(
        &self,
        head: u64,
    ) -> Vec<([u8; 32], Result<RecoveryTarget, &'static str>)> {
        let lease_blocks = self.exec.exec.live_lease_blocks;
        let buyer = self.setup.buyer;
        self.book
            .unresolved()
            .filter(|t| t.orphaned || t.released.max_block.is_none_or(|last| head > last))
            .map(|t| {
                let r = &t.released;
                let sender = r.origin.map_or(buyer, |o| o.sender);
                if sender != buyer {
                    return (r.tx_hash, Err(crate::sources::LIVE_RECOVERY_OTHER_SENDER));
                }
                let lease = r.intent == LiveIntentKind::Buy && r.token == [0; 20] && !t.orphaned;
                let window_end = r.max_block.filter(|_| !t.orphaned);
                let target = match (lease, window_end) {
                    (false, _) => Ok(RecoveryTarget::Hash {
                        tx_hash: r.tx_hash,
                        sender,
                        nonce: r.nonce,
                        window_end,
                    }),
                    (true, Some(to)) => Ok(RecoveryTarget::Window {
                        sender,
                        nonce: r.nonce,
                        from: to.saturating_sub(lease_blocks),
                        to,
                    }),
                    (true, None) => Err(crate::sources::LIVE_RECOVERY_LEASE_WINDOW_UNKNOWN),
                };
                (r.tx_hash, target)
            })
            .collect()
    }

    /// REV.1.0 R3b: the events of a final block the recovery found for the
    /// unresolved transaction `key` (confirmed by the second source),
    /// applied between the follower's blocks as the follower's own: its
    /// chain facts name the source RECEIPT; a transaction resolved already
    /// stays as it is. None while a followed block is being applied (asked
    /// again before the next one).
    pub fn recover_block(
        &mut self,
        key: &[u8; 32],
        events: &[CanonicalEvent],
    ) -> Option<Vec<PositionChange>> {
        if self.block.is_some() || !self.seen.is_empty() {
            return None;
        }
        if !self.book.unresolved().any(|t| t.released.tx_hash == *key) {
            return Some(Vec::new());
        }
        let before = self.unresolved_hashes();
        self.source = Some(FactSource::Receipt);
        let mut changes = Vec::new();
        for event in events {
            changes.extend(self.on_canonical(event));
        }
        self.source = None;
        // A block that ended unfinished leaves nothing behind.
        self.block = None;
        self.seen.clear();
        self.nonces_taken.clear();
        self.block_launches.clear();
        self.block_buys.clear();
        self.count_resolved(&before);
        Some(changes)
    }

    fn unresolved_hashes(&self) -> Vec<[u8; 32]> {
        self.book.unresolved().map(|t| t.released.tx_hash).collect()
    }

    /// The transactions of `before` the recovery resolved.
    fn count_resolved(&mut self, before: &[[u8; 32]]) {
        let book = &self.book;
        let resolved = before
            .iter()
            .filter(|h| !book.unresolved().any(|t| t.released.tx_hash == **h))
            .count();
        self.recovery.recovered += resolved as u64;
    }

    /// REV.1.0 R3b: the runtime's recovery state, for the status.
    pub fn set_recovery(
        &mut self,
        pending: Vec<([u8; 32], &'static str)>,
        faults: u64,
        fault: Option<String>,
    ) {
        self.recovery.pending = pending;
        self.recovery.faults = faults;
        self.recovery.fault = fault;
    }

    pub fn recovery(&self) -> &RecoveryView {
        &self.recovery
    }

    /// What the book changed of the lots reported to the executor and the
    /// strategy: a token's reported lot ends when it is no longer the
    /// token's open lot; then the open lot is reported (a reorg may reopen
    /// an earlier lot of a token bought again, BSC3-047), adopted by the
    /// executor (its exit sells the real tokens).
    fn position_changes(&mut self) -> Vec<PositionChange> {
        // Re-check RC1: a ledger that does not read says nothing of the
        // lots (its empty fold would end every one for the strategy).
        if self.ledger_error.is_some() {
            return Vec::new();
        }
        let mut changes = Vec::new();
        let book = &self.book;
        self.opened.retain(|(token, lot)| {
            let still = book.open_lot(token).is_some_and(|p| p.lot == *lot);
            if !still {
                changes.push(PositionChange::Ended { token: *token });
            }
            still
        });
        for p in &self.book.positions {
            let reported = self.opened.iter().any(|(token, _)| *token == p.token);
            if reported || self.book.open_lot(&p.token).is_none_or(|o| o.lot != p.lot) {
                continue;
            }
            if let Some(x) = self.executor.as_mut() {
                x.adopt(p.token, p.tokens);
            }
            self.opened.push((p.token, p.lot));
            changes.push(PositionChange::Opened {
                token: p.token,
                tokens: p.tokens,
                spent_wei: p.input_wei + p.gas_wei,
                filled_ms: p.opened_ms.unwrap_or_default(),
            });
        }
        changes
    }

    /// Appends `facts`, folds the ledger again and reports what changed:
    /// a position opened is adopted by the executor (its exit sells the
    /// real tokens).
    fn record(&mut self, facts: &[Fact]) -> Vec<PositionChange> {
        if facts.is_empty() {
            return Vec::new();
        }
        for f in facts {
            self.append(f);
        }
        self.refold();
        let changes = self.position_changes();
        // An exit is wanted of an open position only (a ledger that does
        // not read says nothing, R3 re-check RC1).
        if self.ledger_error.is_none() {
            let book = &self.book;
            self.exits_wanted.retain(|t| book.open_lot(t).is_some());
        }
        // An approval the chain included: the armed exit sells its lot
        // without approving again (also one that landed after its window).
        for f in facts
            .iter()
            .filter(|f| f.kind == FactKind::Included && f.intent == LiveIntentKind::Approve)
        {
            let tokens = self.book.lot_of(&f.tx_hash).map(|p| p.tokens);
            if let (Some(x), Some(tokens)) = (self.executor.as_mut(), tokens) {
                x.adopt_approval(f.token, tokens);
            }
        }
        // An exit transaction another one replaced never happened: its
        // position's exit is open again (BSC3-048).
        for f in facts
            .iter()
            .filter(|f| f.kind == FactKind::Replaced && f.intent.exit_step())
        {
            // Our own exit step of the token took the nonce (a retry after
            // the window): the exit went on, nothing to reopen.
            let own = f.by_tx.and_then(|h| self.book.tx(&h)).is_some_and(|t| {
                t.released.token == f.token
                    && t.released.intent.exit_step()
                    && t.state == FactKind::Included
            });
            if !own && self.position_open(&f.token) && self.exit_stopped(&f.token).is_none() {
                let approval = f.intent == LiveIntentKind::Approve;
                if let Some(x) = self.executor.as_mut() {
                    x.reopen_exit(f.token, approval);
                }
                if !self.exits_wanted.contains(&f.token) {
                    self.exits_wanted.push(f.token);
                }
            }
        }
        // BSC3-030: an exit step of our own that reverted took its nonce
        // and did nothing: the exit of its still open position is open
        // again (a reverted approval approved nothing), a new decision at a
        // later nonce through its own owner admission, never the reverted
        // bytes again; not after a later exit step of the position was
        // included, and never past `exit_stopped` (review MEDIUM-1).
        for f in facts
            .iter()
            .filter(|f| f.kind == FactKind::Reverted && f.intent.exit_step())
        {
            if !self.position_open(&f.token) {
                continue;
            }
            let lot = self.book.lot_of(&f.tx_hash).map(|p| p.lot);
            let later = self.book.txs.iter().any(|t| {
                t.lot.is_some()
                    && t.lot == lot
                    && t.released.intent.exit_step()
                    && matches!(
                        t.state,
                        FactKind::Included | FactKind::Released | FactKind::Unknown
                    )
                    && t.released.nonce > f.nonce
            });
            if later {
                continue;
            }
            if self.exit_stopped(&f.token).is_some() {
                self.exits_wanted.retain(|t| *t != f.token);
                continue;
            }
            // Review LOW-3: an approval the chain included stays.
            let approval = f.intent == LiveIntentKind::Approve
                && !self.book.open_lot(&f.token).is_some_and(|p| p.approved);
            if let Some(x) = self.executor.as_mut() {
                x.reopen_exit(f.token, approval);
            }
            if !self.exits_wanted.contains(&f.token) {
                self.exits_wanted.push(f.token);
            }
        }
        self.fail_closed_on_ledger();
        changes
    }

    /// The ledger's fold now.
    pub fn book(&self) -> &Book {
        &self.book
    }

    /// The same path in a fixed boot session and wall clock (tests).
    #[cfg(test)]
    pub(crate) fn in_session(mut self, boot: &str, wall: fn() -> u64) -> Self {
        self.boot = Some(boot.into());
        self.wall = wall;
        self.started_ms = wall();
        self
    }

    pub fn buyer(&self) -> [u8; 20] {
        self.setup.buyer
    }

    /// Whether the LIVE path is armed (the status MODE is `LIVE`).
    pub fn armed(&self) -> bool {
        self.executor.is_some()
    }

    /// A clean runtime stop: the armed path ends, its key goes.
    pub fn shutdown(&mut self) {
        self.probe = None;
        self.stop(Phase::Off);
    }

    /// BSC3-076: while armed for C3 and a C3 BUY may come now (no exit
    /// first, funded, not closed), the owner's lease of it is held at the
    /// free nonce, renewed before half its window is gone; otherwise the
    /// lease held goes back unused. Outside the entry path (a reconcile).
    fn keep_c3_lease(&mut self, owner: Option<&StateOwner>) {
        let Some(x) = self.executor.as_ref() else {
            return;
        };
        let grant = x.end().armed().grant();
        let generation = x.end().armed().grant_generation();
        let next = owner.and_then(|o| self.free_nonce(o));
        // R4 (BSC3-042): only while this arm's C3 lanes can take the free
        // nonce (taken, the next C3 BUY waits for a new arm).
        let c3 = !grant.exit_only
            && matches!(grant.strategies, LiveStrategies::C3 | LiveStrategies::Both)
            && !self.public_only_locked()
            && next == Some(x.c3_nonce());
        let head = owner.and_then(|o| o.head()).map(|h| h.number);
        let may_buy = self.buy_refusal(self.one_buy(), owner, next).is_none();
        let (true, true, Some(nonce), Some(head)) = (c3, may_buy, next, head) else {
            self.yield_lease();
            return;
        };
        // As the executor hands the BUY over: the exits of the open
        // positions and its own (approval and SELL at the signed gas).
        let (approval, sell) = self.exit_gas();
        let (Ok(max_wei), Ok(exit_reserve_wei)) = (
            u128::try_from(self.one_buy()),
            u128::try_from(self.exit_reserve_besides(self.buyer_key(nonce)) + approval + sell),
        ) else {
            self.yield_lease();
            return;
        };
        let x = &self.exec.exec;
        let want = crate::live_end::LeaseWant {
            nonce,
            input_wei: x.input_wei,
            max_wei,
            exit_reserve_wei,
            head,
            validity_blocks: x.validity_blocks,
            lease_blocks: x.live_lease_blocks,
            attempt: self
                .lease_attempt
                .filter(|(g, _)| *g == generation)
                .map(|(_, a)| a),
        };
        let Some(end) = self.executor.as_mut().map(|x| x.end_mut()) else {
            return;
        };
        match end.hold_lease(&want) {
            Ok(()) => {
                self.lease_attempt = None;
                self.lease_error = None;
            }
            Err(why) => {
                self.lease_refused += 1;
                self.lease_error = Some(why);
            }
        }
    }

    /// BSC3-076 review H1: the attempt of the grant written at `generation`
    /// the next C3 BUY takes: the one this arm's lease holds, or the one
    /// kept from a lease returned under that grant. The runtime's reconcile
    /// reads the owner's gate besides it too.
    pub fn held_attempt(&self, generation: Option<u64>) -> Option<[u8; 32]> {
        let generation = generation?;
        let leased = self.executor.as_ref().and_then(|x| {
            (x.end().armed().grant_generation() == generation)
                .then(|| x.end().lease().map(|l| l.attempt()))
                .flatten()
        });
        leased.or(self
            .lease_attempt
            .filter(|(g, _)| *g == generation)
            .map(|(_, a)| a))
    }

    /// BSC3-076: the lease held goes back unused; its attempt is kept for
    /// the next lease of the same grant. One that cannot go back counts on
    /// (a restart recovers it as UNKNOWN at its nonce).
    fn yield_lease(&mut self) {
        let Some(x) = self.executor.as_mut() else {
            return;
        };
        let generation = x.end().armed().grant_generation();
        match x.end_mut().return_lease() {
            Ok(Some(attempt)) => self.lease_attempt = Some((generation, attempt)),
            Ok(None) => {}
            Err(why) => {
                self.lease_refused += 1;
                self.lease_error = Some(why);
            }
        }
    }

    /// The arm ends. Its fence stays while a transaction it may have
    /// released is unresolved or not known (BSC3-041): a later arm settles
    /// it only once the chain resolved them.
    fn disarm(&mut self) {
        // BSC3-051: the answers read since the last reconcile reach the
        // ledger before the end goes.
        if let Some(x) = self.executor.as_mut() {
            x.end_mut().record_responses();
        }
        // BSC3-076: its lease, unused, goes back before the end goes.
        self.yield_lease();
        let Some(x) = self.executor.take() else {
            return;
        };
        let unknown = x.end().ledger_failure().is_some() || self.ledger_fault().is_some();
        if unknown || self.book.unresolved().next().is_some() {
            drop(x);
        } else if x.disarm().is_err() {
            self.disarm_errors += 1;
        }
    }

    fn stop(&mut self, phase: Phase) {
        self.disarm();
        self.connecting = None;
        self.validated = None;
        self.phase = phase;
    }

    fn spawn_probe(&mut self, endpoint: &str, limits: &FollowerLimits) {
        let (tx, rx) = channel();
        let endpoint = endpoint.to_string();
        let limits = *limits;
        let prober = self.prober;
        let caller = Address::from(self.setup.buyer);
        let spawned = thread::Builder::new()
            .name("bsc-live-probe".into())
            .spawn(move || {
                let _ = tx.send(prober(&endpoint, &limits, &bsc_mainnet_v5_27_2(), caller));
            });
        match spawned {
            Ok(_) => {
                self.probe = Some(rx);
                self.last_probe = Some(Instant::now());
                self.probes += 1;
            }
            Err(e) => self.stop(Phase::ProbeFailed(format!("spawn: {e}"))),
        }
    }

    /// One BUY's most: the configured input and its gas at the signed price.
    fn one_buy(&self) -> U256 {
        let x = &self.exec.exec;
        U256::from(x.input_wei) + U256::from(x.gas_price_wei) * U256::from(x.gas_limit)
    }

    /// The most an exit step may go at: the signed gas price, or with the
    /// exit re-price ON (R5) the armed grant's risk maximum.
    fn exit_price(&self) -> u64 {
        let x = &self.exec.exec;
        let max = self
            .executor
            .as_ref()
            .and_then(|e| e.end().armed().grant().risk)
            .map(|r| r.max_gas_price_wei);
        match (x.live_exit_reprice_bps, max) {
            (0, _) | (_, None) => x.gas_price_wei,
            (_, Some(max)) => max.max(x.gas_price_wei),
        }
    }

    /// One position's exit at the most it may go at ([`Self::exit_price`]):
    /// its approval's gas and its SELL's.
    fn exit_gas(&self) -> (U256, U256) {
        let x = &self.exec.exec;
        let price = U256::from(self.exit_price());
        (
            price * U256::from(x.approve_gas_limit),
            price * U256::from(x.sell_gas_limit),
        )
    }

    /// What the executor's own count of a new BUY's exit (at the signed
    /// price) leaves out of [`Self::exit_gas`] (R5: a re-priced exit).
    fn own_exit_excess(&self) -> U256 {
        let x = &self.exec.exec;
        let excess = U256::from(self.exit_price() - x.gas_price_wei);
        excess * U256::from(x.approve_gas_limit) + excess * U256::from(x.sell_gas_limit)
    }

    /// The most the exits of the ledger's lots may still cost
    /// (BSC3-027/057): [`Self::exit_reserve_besides`] of no nonce.
    fn exit_reserve(&self) -> U256 {
        self.exit_reserve_besides(None)
    }

    /// The most the exits of the ledger's lots may still cost
    /// (BSC3-027/057): per lot pending or open its SELL and, unless an
    /// approval of it is out or included, its approval. The pending lots of
    /// the alternative BUYs of one nonce are one position at most (one of
    /// them lands): they count once, at the most of them; an open lot, and a
    /// pending one without its sender and chain, counts alone, in full.
    /// `besides`: the nonce a new BUY takes, whose own exit (the BUY adds it)
    /// covers that nonce's pending lots. An exiting lot's SELL is out (it
    /// counts in what was spent). None counted: the sum overflows.
    fn exit_reserve_besides(&self, besides: Option<NonceKey>) -> U256 {
        let (approval, sell) = self.exit_gas();
        let mut reserve = NonceCosts::default();
        for p in self
            .book
            .positions
            .iter()
            .filter(|p| matches!(p.state, PositionState::Pending | PositionState::Open))
        {
            // A lot is a BUY's (an exit step with none before it is not).
            let Some(buy) = self
                .book
                .tx(&p.lot)
                .filter(|t| t.released.intent == LiveIntentKind::Buy)
            else {
                continue;
            };
            let key = match p.state {
                PositionState::Pending => buy.released.key(),
                _ => None,
            };
            if key.is_some() && key == besides {
                continue;
            }
            let approved = self.book.txs.iter().any(|t| {
                t.lot == Some(p.lot)
                    && t.released.intent == LiveIntentKind::Approve
                    && matches!(
                        t.state,
                        FactKind::Released | FactKind::Unknown | FactKind::Included
                    )
            });
            let wei = match approved {
                true => sell,
                false => approval + sell,
            };
            if reserve.add(key, wei).is_none() {
                return U256::MAX;
            }
        }
        reserve.total().unwrap_or(U256::MAX)
    }

    /// The buyer's nonce `nonce` on the probed chain, as the owner's
    /// admission names it (none: nothing probed).
    /// Whether the end released a BUY (its bytes may have left) after its
    /// first `before` releases.
    fn released_buy_since(&self, before: usize) -> bool {
        self.executor.as_ref().is_some_and(|x| {
            x.end()
                .released()
                .get(before..)
                .is_some_and(|r| r.iter().any(|r| r.kind == LiveIntentKind::Buy))
        })
    }

    fn buyer_key(&self, nonce: u64) -> Option<NonceKey> {
        Some(NonceKey {
            origin: NonceOrigin {
                chain_id: self.observation.as_ref()?.chain_id,
                sender: self.setup.buyer,
            },
            nonce,
        })
    }

    fn observed(&mut self, result: Result<VenueObservation, String>) {
        let observation = match result {
            Ok(o) => o,
            Err(e) => return self.stop(Phase::ProbeFailed(e)),
        };
        let generation = self.codehash_generation + 1;
        let validated = bsc_mainnet_v5_27_2().validate(&observation.deployment(), generation);
        let terms = observation.terms;
        let balance = observation.caller_balance_wei;
        let probed_at = observation.block_number;
        let probed_hash = observation.block_hash;
        let o = &observation.observed;
        let seen = format!(
            "observed {} implementation {}",
            o.portal_version, o.portal_implementation
        );
        self.observation = Some(observation);
        match validated {
            Err(e) => self.stop(Phase::DeploymentDrift(format!("{e}: {seen}"))),
            Ok(v) if !v.economics_witnessed() => self.stop(Phase::EconomicsUnqualified(format!(
                "{seen} is decode-only"
            ))),
            Ok(_) if terms != WITNESSED_MAINNET_TERMS => {
                self.stop(Phase::TermsDrift(format!("{terms:?}")))
            }
            Ok(v) => {
                // BSC3-027: the balance holds a BUY, its exit and the exits
                // of what is open (each BUY is checked again before it goes,
                // [`Self::unfunded`]). Short of that while a position waits
                // for its exit, the path arms for the exits as long as one
                // exit step is paid for.
                let (approval, sell) = self.exit_gas();
                let open = self.exit_reserve();
                let need = self.one_buy() + approval + sell + open;
                self.funds = Some(Funds {
                    balance,
                    block: probed_at,
                    hash: probed_hash,
                });
                let exits = open > U256::ZERO || self.positions_open();
                // R3b review NIT-4: a burn wanted (of no lot) arms the path
                // on its own gas.
                let burn = self.burn_target().map(|_| self.burn_cost());
                let least = match exits {
                    true => Some(approval.min(sell)),
                    false => burn,
                };
                if balance < need && least.is_none_or(|least| balance < least) {
                    self.disarm();
                    self.validated = None;
                    self.phase = Phase::Unfunded(match (exits, burn) {
                        (true, _) => {
                            format!("{balance} wei, the exits of the open positions need {open}")
                        }
                        (false, Some(burn)) => {
                            format!("{balance} wei, the nonce burn needs {burn}")
                        }
                        (false, None) => format!("{balance} wei, a BUY with its exit needs {need}"),
                    });
                    return;
                }
                if self.executor.is_none() {
                    self.codehash_generation = generation;
                    self.validated = Some((v, terms));
                }
            }
        }
    }

    fn retry_due(&self) -> bool {
        self.last_attempt
            .is_none_or(|at| at.elapsed() >= self.exec.reprobe)
    }

    /// BSC3-056: the runtime's verdict of the second source this pass, for
    /// the armed end and every end armed later.
    pub fn set_confirmation(&mut self, verdict: Option<crate::canonical_confirm::Confirmation>) {
        self.confirmation = verdict;
        if let Some(x) = self.executor.as_mut() {
            x.end_mut().set_confirmation(verdict);
        }
        if let Some((end, _)) = self.connecting.as_mut() {
            end.set_confirmation(verdict);
        }
    }

    /// One reconcile step at `now_ms` (unix): `session` names this
    /// process's fences. Verifies the LIVE qualification itself.
    pub fn tick(
        &mut self,
        owner: Option<&StateOwner>,
        canonical: Option<(&str, &FollowerLimits)>,
        session: u64,
        now_ms: u64,
    ) {
        let qualification = live_qualification::view(&self.repo_root);
        self.tick_with(&qualification, owner, canonical, session, now_ms);
    }

    /// [`LiveRuntime::tick`] with the reconcile's verified LIVE
    /// qualification (`live_qualification::view`; BSC3-050: verified once
    /// per reconcile, never again here).
    pub fn tick_with(
        &mut self,
        qualification: &(live_qualification::Verified, Option<String>),
        owner: Option<&StateOwner>,
        canonical: Option<(&str, &FollowerLimits)>,
        session: u64,
        now_ms: u64,
    ) {
        if let Some((_, limits)) = canonical {
            self.follower_limits = Some(*limits);
        }
        for o in self.public_exit.iter().flat_map(|p| p.poll()) {
            self.public_counts.count(o);
        }
        // BSC3-025: an end being connected waits across passes that do not
        // come back to arming (review LOW-3: a flapping precondition never
        // restarts it), at most as long as Q5A lets its connections take
        // and stay idle.
        if self
            .connecting
            .as_ref()
            .is_some_and(|(_, since)| since.elapsed() > connecting_hold())
        {
            self.connecting = None;
        }
        // The gate, as it holds now.
        let opened = match &self.boot {
            Some(boot) => {
                Store::open_in_session(&self.state_dir, self.exec.owner, Ok(boot.clone()))
            }
            None => Store::open(&self.state_dir, self.exec.owner),
        };
        let store = match opened {
            // R4: its BUYs within the campaign's cumulative budget.
            Ok(s) => s.with_campaign_spend(self.exec.exec.live_spend_total_wei),
            Err(e) => return self.close(format!("owner state: {e}")),
        };
        // R3: the operator's decisions, then the slots of attempts that
        // never left (BSC3-068), before the gate reads the owner.
        self.fold_decisions();
        self.return_withdrawn(&store);
        let (verified, invalid) = qualification;
        if let Some(why) = invalid {
            return self.close(format!("LIVE_QUALIFICATION_INVALID: {why}"));
        }
        let (strategies, generation) = match store.current() {
            Ok(g) => (
                g.live.map(|grant| grant.strategies),
                g.live_set_by.map(|by| by.generation),
            ),
            Err(e) => return self.close(format!("owner state: {e}")),
        };
        let Some(strategies) = strategies else {
            return self.close(crate::sources::LIVE_GRANT_MISSING.into());
        };
        let components = components(strategies);
        // BSC3-076 review H1: the attempt the next C3 BUY takes (held by
        // this arm's lease, or kept from one returned under this grant)
        // already holds its slots: they refuse nothing.
        let held = self
            .held_attempt(generation)
            .map(alloy_primitives::hex::encode);
        // R4 (BSC3-059): a grant beyond the runtime's longest admits no
        // BUY; its exits go on.
        let max = self.exec.exec.live_grant_max_duration_ms;
        let live_now = store
            .live_now_holding(&components, &verified.view, now_ms, held.as_deref())
            .and_then(
                |n| match max > 0 && n.grant().expires_unix_ms.saturating_sub(now_ms) > max {
                    true => Err(crate::sources::LIVE_GRANT_DURATION_ABOVE_MAX.into()),
                    false => Ok(n),
                },
            )
            // R9 (BSC3-001): with a presence anchor, a BUY needs the grant
            // signed by its key; its exits go on.
            .and_then(|n| match self.exec.exec.live_presence_anchor {
                Some(anchor) if n.presence_signer() != Some(anchor) => {
                    Err(crate::sources::LIVE_PRESENCE_UNSIGNED.into())
                }
                _ => Ok(n),
            })
            // R8 (BSC3-049): a qualification older than the runtime's
            // bound admits no BUY; its exits go on.
            .and_then(|n| {
                let max = self.exec.exec.live_qualification_max_age_ms;
                match live_qualification::stale(verified, now_ms, max) {
                    Some(why) => Err(why.into()),
                    None => Ok(n),
                }
            })
            // R4: no room left in the campaign's budget for one BUY (the
            // repository's 0, or spent): BUYs close here, no lease is asked
            // for (the owner's admission holds the exact bound, exits
            // included); its exits go on.
            .and_then(|n| {
                let budget = U256::from(self.exec.exec.live_spend_total_wei);
                match crate::sources::spent_total(&self.state_dir) {
                    Err(_) => Err(crate::sources::LIVE_SPEND_UNREADABLE.into()),
                    Ok(spent) if U256::from(spent) + self.one_buy() > budget => {
                        Err(crate::sources::LIVE_CAMPAIGN_SPEND_EXHAUSTED.into())
                    }
                    Ok(_) => Ok(n),
                }
            })
            // R4: a BUY rests on the bound builder policy as it holds at
            // this pass: fresh, its builders verified, the signed gas
            // price at its floor or above (a refresh applies at the next
            // pass); its exits go on.
            .and_then(|n| self.builder_bound(now_ms).map(|()| n))
            // R4 (BSC3-069): an outlier grant admits BUYs only confirmed
            // and unlocked; its exits go on.
            .and_then(|n| match outlier_refusal(n.grant(), &self.exec.exec) {
                Some(why) => Err(why),
                None => Ok(n),
            });
        let live_now = match live_now {
            Ok(n) => {
                self.buys_closed = None;
                n
            }
            // BSC3-071: what only a BUY needs (its attempts, the grant's
            // room for another input) does not end the exits of open
            // positions under the same grant; BUYs stay refused.
            // R3b review MEDIUM-3: a burn wanted is an exit step's
            // admission too: the path arms for it.
            Err(why) if self.positions_open() || self.burn_target().is_some() => {
                match store.live_now_exits(&components, &verified.view, now_ms) {
                    Ok(n) => {
                        self.buys_closed = Some(why);
                        n
                    }
                    Err(why) => return self.close(why),
                }
            }
            Err(why) => return self.close(why),
        };
        self.sources_live = live_now.sources_live().to_vec();
        if self.executor.is_some()
            && let Some(why) = self.ledger_fault()
        {
            return self.close(why);
        }
        if let Some(head) = owner.and_then(|o| o.head()) {
            self.want_expired_exits(head.number);
        }
        if let Some(x) = self.executor.as_ref() {
            // Armed under another grant: this one ends, a new arm follows.
            // (The moment of the proof differs every pass; the grant decides.)
            let armed = x.end().armed();
            if armed.grant() != live_now.grant()
                || armed.grant_generation() != live_now.grant_generation()
            {
                return self.close(crate::sources::LIVE_GRANT_CHANGED.into());
            }
            // BSC3-012: an arm rests on the one qualification record it
            // was armed with; another one verified on file (rebuilt, or
            // changed while armed) ends it: what admits its intents never
            // changes under it.
            if x.end().qualification_sha256() != verified.sha256 {
                return self.close(crate::sources::LIVE_QUALIFICATION_CHANGED.into());
            }
            // BSC3-048: this arm cannot take the buyer's free nonce (its
            // window over, or taken by an earlier arm's transaction that
            // landed late): a new arm takes it (an unresolved transaction
            // stays so in the ledger, its spend counted, until its nonce
            // is taken; the new arm settles this arm's fence).
            // R4 (BSC3-042): with BSC_EXEC_LIVE_C3_REARM ON, a C3 BUY the
            // chain included at the arm's nonce (not reverted; the free
            // nonce past it, nothing unresolved) renews the arm for the
            // next C3 BUY.
            let reuse = owner.and_then(|o| self.free_nonce(o)).filter(|n| {
                rearm_due(
                    *n,
                    x.strategy_next_nonce(),
                    x.strategy_lanes().4,
                    self.book.unresolved().any(|t| t.released.nonce == *n),
                ) || (self.exec.exec.live_c3_rearm
                    && x.c3_took_nonce()
                    && *n > x.c3_nonce()
                    && self.book.txs.iter().any(|t| {
                        t.released.nonce == x.c3_nonce()
                            && t.released.intent == LiveIntentKind::Buy
                            && t.state == FactKind::Included
                    })
                    && self.book.unresolved().next().is_none())
            });
            match reuse {
                None => {
                    if let Some(x) = self.executor.as_mut() {
                        let _ = x.end_mut().maintain(self.setup.connect_timeout);
                    }
                    self.keep_c3_lease(owner);
                    self.phase = Phase::Armed;
                    self.reprobe(canonical);
                    return;
                }
                Some(_) => {
                    self.disarm();
                    self.phase = Phase::WaitingNonce;
                }
            }
        }
        if self.phase.terminal() {
            return;
        }
        // BSC3-024/041: no arm while a transaction an earlier arm (or
        // process) may have released can still take its nonce.
        if let Err(why) = self.reconciled(now_ms) {
            return self.close(why);
        }
        let (Some(owner), Some((endpoint, limits))) = (owner, canonical) else {
            self.phase = Phase::WaitingC2;
            return;
        };
        if !owner.watches(&self.setup.buyer) {
            self.phase = Phase::NotWatched;
            return;
        }
        if let Some(rx) = &self.probe {
            match rx.try_recv() {
                Err(TryRecvError::Empty) => {
                    self.phase = Phase::Probing;
                    return;
                }
                Err(TryRecvError::Disconnected) => {
                    self.probe = None;
                    return self.stop(Phase::ProbeFailed("probe thread ended".into()));
                }
                Ok(result) => {
                    self.probe = None;
                    self.observed(result);
                    if self.phase.terminal() {
                        return;
                    }
                }
            }
        }
        // The deployment, terms and balance an arm rests on are this
        // reprobe period's: after a long wait the probe runs again first.
        if self.validated.is_some()
            && self
                .last_probe
                .is_none_or(|at| at.elapsed() >= self.exec.reprobe)
        {
            self.spawn_probe(endpoint, limits);
            self.phase = Phase::Probing;
            return;
        }
        let Some((deployment, terms)) = self.validated else {
            if matches!(self.phase, Phase::Unfunded(_))
                && self
                    .last_probe
                    .is_some_and(|at| at.elapsed() < self.exec.reprobe)
            {
                return;
            }
            self.spawn_probe(endpoint, limits);
            self.phase = Phase::Probing;
            return;
        };
        let Some(next_nonce) = owner.clean_next_nonce(&self.setup.buyer) else {
            self.phase = Phase::WaitingNonce;
            return;
        };
        // C2 and the ledger agree: a nonce the ledger holds consumed is
        // consumed on chain (an orphan the ledger missed, or C2 behind it,
        // stops here until they agree).
        if let Some(t) = self.book.txs.iter().find(|t| {
            matches!(
                t.state,
                FactKind::Included | FactKind::Reverted | FactKind::Replaced
            ) && t.released.nonce >= next_nonce
        }) {
            let why = format!(
                "{}: nonce {} consumed in the ledger, open on chain (C2 next {next_nonce}), 0x{}",
                crate::sources::LIVE_RELEASE_UNRESOLVED,
                t.released.nonce,
                alloy_primitives::hex::encode(t.released.tx_hash)
            );
            // The validated probe stays: no new probe each pass while
            // they disagree.
            self.phase = Phase::Blocked(why);
            return;
        }
        // BSC3-041/048: no arm takes a nonce an unresolved transaction may
        // still take (the validated probe stays while it waits).
        if let Err(why) = self.nonce_free(next_nonce, owner.head().map(|h| h.number)) {
            self.phase = Phase::Blocked(why);
            return;
        }
        if matches!(self.phase, Phase::ConnectFailed(_)) && !self.retry_due() {
            return;
        }
        // BSC3-012: the LIVE path arms only in the build its qualification
        // names (before any key is read).
        if let Some(why) = live_qualification::build_refusal(&self.build, verified) {
            self.phase = Phase::Blocked(why);
            return;
        }
        // BSC3-069: the executor signs within the execution risk the grant
        // names (a grant without it is not clear at all).
        if let Some(why) = risk_refusal(live_now.grant(), &self.exec.exec) {
            self.phase = Phase::Blocked(why);
            return;
        }
        // BSC3-025: an end of this grant and this qualification whose
        // connections are being established is waited for, never built again.
        let (mut end, since) = match self.connecting.take() {
            Some((end, since))
                if end.armed().grant_generation() == live_now.grant_generation()
                    && end.armed().grant() == live_now.grant()
                    && end.qualification_sha256() == verified.sha256 =>
            {
                (end, since)
            }
            _ => {
                let mut refused: Vec<[u8; 20]> = self.paper_sender.into_iter().collect();
                // BSC3-026: the stand-in never runs on the account's owner
                // state and signs only with its state's test identity
                // (checked before any key file is read), never a
                // production account.
                if let LiveRouteConfig::Loopback { .. } = &self.setup.route {
                    if let Some(why) =
                        loopback_refused(store.dir(), crate::sources::canonical_state_dir())
                    {
                        return self.stop(Phase::ArmFailed(why));
                    }
                    if let Some(why) =
                        crate::live_key::loopback_key_refused(&self.setup.key_path, store.dir())
                    {
                        return self.stop(Phase::KeyRefused(why));
                    }
                    refused.extend_from_slice(&self.exec.refused_accounts);
                }
                // BSC3-026 review: on the stand-in route the identity's own
                // name is loaded (a link there is refused by the load).
                let key_path = match &self.setup.route {
                    LiveRouteConfig::Loopback { .. } => {
                        crate::live_key::loopback_identity(store.dir())
                    }
                    LiveRouteConfig::Builders { .. } => self.setup.key_path.clone(),
                };
                let key = match LiveKey::load(
                    &key_path,
                    &self.repo_root,
                    self.setup.buyer,
                    &refused,
                    &live_now,
                ) {
                    Ok(k) => k,
                    Err(e) => return self.stop(Phase::KeyRefused(e)),
                };
                let route = match &self.setup.route {
                    LiveRouteConfig::Loopback { port } => LiveRoute::Loopback { port: *port },
                    LiveRouteConfig::Builders { blockrazor_token } => {
                        let setup = match TlsSetup::web_pki_roots() {
                            Ok(s) => s,
                            Err(e) => {
                                return self.stop(Phase::ArmFailed(format!("TLS setup: {e:?}")));
                            }
                        };
                        let blockrazor = match blockrazor_token
                            .as_deref()
                            .map(|t| token_from(t, &self.repo_root))
                            .transpose()
                        {
                            Ok(t) => t,
                            Err(e) => return self.stop(Phase::ArmFailed(e)),
                        };
                        LiveRoute::Builders { setup, blockrazor }
                    }
                };
                let end = LiveEnd::new(
                    key,
                    live_now,
                    store,
                    verified.clone(),
                    route,
                    deployment.deployment().chain_id,
                    self.exec.exec.capability_generation,
                    Box::new(self.wall),
                );
                let mut end = match end {
                    Ok(e) => e,
                    Err(e) => return self.stop(Phase::ArmFailed(e)),
                };
                end.set_confirmation(self.confirmation);
                let failed = end.begin_connect(self.setup.connect_timeout);
                if !failed.is_empty() {
                    return self.connect_failed(end, failed);
                }
                (end, Instant::now())
            }
        };
        match end.connected() {
            Connecting::Pending => {
                self.connecting = Some((end, since));
                self.phase = Phase::Connecting;
                return;
            }
            Connecting::Failed(failed) => return self.connect_failed(end, failed),
            Connecting::Ready => {}
        }
        // Everything earlier arms released is resolved: their fences are.
        if let Err(e) = self.settle_fences() {
            return self.stop(Phase::ArmFailed(format!("fences: {e}")));
        }
        let armed = Executor::arm_end(
            end,
            deployment,
            terms,
            self.exec.exec.clone(),
            Arming {
                next_nonce,
                fence_dir: &self.fence_dir,
                session,
                arm: self.arms,
            },
        );
        self.arms += 1;
        match armed {
            Ok(mut executor) => {
                // Positions open from before (a restart, an earlier grant)
                // stay exitable with their real tokens.
                for p in self
                    .book
                    .positions
                    .iter()
                    .filter(|p| matches!(p.state, PositionState::Open | PositionState::Exiting))
                {
                    executor.adopt(p.token, p.tokens);
                    // Its lot's approval the chain included: no second one
                    // (an earlier lot's went with its SELL, BSC3-047).
                    if p.approved {
                        executor.adopt_approval(p.token, p.tokens);
                    }
                }
                self.folded_facts = 0;
                self.next_nonce = Some(next_nonce);
                self.executor = Some(executor);
                self.phase = Phase::Armed;
                // BSC3-076: a C3 BUY's lease from the arm on.
                self.keep_c3_lease(Some(owner));
            }
            Err(e) => self.stop(Phase::ArmFailed(e.to_string())),
        }
    }

    /// The end and its key go; a later pass tries again.
    fn connect_failed(&mut self, end: LiveEnd, failed: Vec<(LiveAdapter, String)>) {
        drop(end);
        self.last_attempt = Some(Instant::now());
        self.phase = Phase::ConnectFailed(
            failed
                .iter()
                .map(|(a, e)| format!("{}: {e}", a.name()))
                .collect::<Vec<_>>()
                .join("; "),
        );
    }

    /// The gate is closed: an armed path ends with the reason, an unarmed
    /// one shows it.
    /// [`crate::builder_policy::live_bound`] of the configured policy.
    fn builder_bound(&self, now_ms: u64) -> Result<(), String> {
        let x = &self.exec.exec;
        crate::builder_policy::live_bound(
            &self.repo_root,
            &x.live_builder_policy,
            x.gas_price_wei,
            now_ms,
        )
    }

    fn close(&mut self, why: String) {
        self.probe = None;
        self.connecting = None;
        // BSC3-075 review: a launch waits for a granted source of this arm.
        self.awaiting_granted.clear();
        if self.executor.is_some() {
            self.stop(Phase::Ended(why));
        } else {
            self.validated = None;
            self.phase = Phase::Blocked(why);
        }
    }

    fn reprobe(&mut self, canonical: Option<(&str, &FollowerLimits)>) {
        if let Some(rx) = &self.probe {
            match rx.try_recv() {
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.probe = None;
                    return self.stop(Phase::ProbeFailed("probe thread ended".into()));
                }
                Ok(result) => {
                    self.probe = None;
                    return self.observed(result);
                }
            }
        }
        if let Some((endpoint, limits)) = canonical
            && self
                .last_probe
                .is_none_or(|at| at.elapsed() >= self.exec.reprobe)
        {
            self.spawn_probe(endpoint, limits);
        }
    }

    /// BSC3-075: as many launches as the ingress remembers wait for a
    /// granted source after a source the grant does not name saw them first
    /// (the runtime's `BSC_RUNTIME_DEDUP_HASHES`).
    pub fn with_ingress_hashes(mut self, hashes: usize) -> Self {
        self.awaiting_bound = hashes;
        self
    }

    /// The armed LIVE path's verified full sighting of a Flap launch: its
    /// first (`first`) or a later one from another source.
    #[allow(clippy::too_many_arguments)]
    pub fn on_sighting(
        &mut self,
        source: SourceId,
        hash: [u8; 32],
        from: [u8; 20],
        first: bool,
        raw: &[u8],
        ingress_ns: u64,
        owner: &StateOwner,
        clock: &dyn Clock,
    ) -> Option<ExecBuy<LiveHandoff>> {
        match first {
            true => self.on_launch(source, hash, from, raw, ingress_ns, owner, clock),
            false => self.on_later(source, hash, from, raw, ingress_ns, owner, clock),
        }
    }

    /// BSC3-075: a later verified full sighting of a launch whose first
    /// sighting came from a source the grant does not name (or that is not
    /// effective LIVE): the first such sighting from a granted source
    /// effective LIVE triggers its BUY, once.
    #[allow(clippy::too_many_arguments)]
    pub fn on_later(
        &mut self,
        source: SourceId,
        hash: [u8; 32],
        from: [u8; 20],
        raw: &[u8],
        ingress_ns: u64,
        owner: &StateOwner,
        clock: &dyn Clock,
    ) -> Option<ExecBuy<LiveHandoff>> {
        self.executor.as_ref()?;
        let granted = self.sources_live.contains(&source);
        if !granted || !self.awaiting_granted.take(&hash) {
            return None;
        }
        self.on_launch(source, hash, from, raw, ingress_ns, owner, clock)
    }

    /// A verified full pending C1-scope launch first seen by `source`,
    /// when armed: only a source the grant names and the owner makes
    /// effective LIVE triggers a LIVE BUY.
    #[allow(clippy::too_many_arguments)]
    pub fn on_launch(
        &mut self,
        source: SourceId,
        hash: [u8; 32],
        from: [u8; 20],
        raw: &[u8],
        ingress_ns: u64,
        owner: &StateOwner,
        clock: &dyn Clock,
    ) -> Option<ExecBuy<LiveHandoff>> {
        let granted = self
            .executor
            .as_ref()?
            .end()
            .armed()
            .grant()
            .sources
            .contains(&source);
        if !self.sources_live.contains(&source) {
            // BSC3-075: neither its BUY nor a block for the granted
            // sources: the first of them that sees it still triggers.
            match granted {
                true => self.source_not_live += 1,
                false => self.source_refused += 1,
            }
            self.awaiting_granted.insert(hash, self.awaiting_bound);
            return None;
        }
        if self.public_only_locked() {
            // R6: a non-public LIVE source down is its own count.
            match self.sources_live.iter().any(|s| !s.public()) {
                true => self.source_unhealthy += 1,
                false => self.public_only_refused += 1,
            }
            return None;
        }
        self.poll_responses(clock);
        let next = self.free_nonce(owner);
        if self
            .buy_refused(self.one_buy(), Some(owner), next)
            .is_some()
        {
            return None;
        }
        let reserve = Some(
            self.exit_reserve_besides(next.and_then(|n| self.buyer_key(n)))
                + self.own_exit_excess(),
        );
        let x = self.executor.as_mut()?;
        let before = x.end().released().len();
        let buy = launch_to(x, hash, from, raw, ingress_ns, owner, next, reserve, clock);
        self.record_responses();
        // Review of the limits (LOW): a BUY whose bytes may have left counts
        // for the interval, handed over or UNKNOWN.
        if buy.is_some() || self.released_buy_since(before) {
            self.last_buy_ms = Some((self.wall)());
        }
        buy
    }

    /// What of the probed balance is left besides the nonce `besides` (a
    /// new BUY's), and the most that nonce's transactions may still cost:
    /// every transaction of the ledger not paid in the probe's block counts
    /// at its most, the alternatives of one (chain, sender, nonce) once, at
    /// the most of them (one of them lands), and none of them once one of
    /// them is paid there; a transaction without its sender and chain
    /// counts alone. Paid in the probe's block: included, reverted or
    /// replaced at or below it, while that block is the one C2 holds
    /// canonical at its number; with C2 holding another block there, or
    /// none (older than its window, not reached yet), nothing is paid. None:
    /// no balance read, or the sum overflows.
    fn left_besides(
        &self,
        owner: Option<&StateOwner>,
        besides: Option<NonceKey>,
    ) -> Option<(U256, U256)> {
        let funds = self.funds?;
        let consistent = owner.and_then(|o| o.canonical_hash(funds.block)) == Some(funds.hash);
        let paid = |t: &crate::live_ledger::Tx| {
            consistent
                && matches!(
                    t.state,
                    FactKind::Included | FactKind::Reverted | FactKind::Replaced
                )
                && t.inclusion
                    .as_ref()
                    .and_then(|f| f.block)
                    .is_some_and(|(n, _, _)| n <= funds.block)
        };
        let settled: std::collections::HashSet<NonceKey> = self
            .book
            .txs
            .iter()
            .filter(|t| paid(t))
            .filter_map(|t| t.released.key())
            .collect();
        let mut to_come = NonceCosts::default();
        for t in &self.book.txs {
            let counts = match t.state {
                FactKind::Released | FactKind::Unknown => true,
                FactKind::Included | FactKind::Reverted => !paid(t),
                FactKind::Withdrawn
                | FactKind::Replaced
                | FactKind::Orphaned
                | FactKind::Responded
                | FactKind::Placed
                | FactKind::Resumed
                | FactKind::WrittenOff
                | FactKind::ExitRequested
                | FactKind::Adopted
                | FactKind::BurnRequested => false,
            };
            let key = t.released.key();
            if !counts || key.is_some_and(|k| settled.contains(&k)) {
                continue;
            }
            to_come.add(key, t.released.max_wei.unwrap_or_default())?;
        }
        let group = besides.map_or(U256::ZERO, |k| to_come.of(&k));
        let others = to_come.total()?.checked_sub(group)?;
        Some((funds.balance.saturating_sub(others), group))
    }

    /// What of the probed balance is left ([`Self::left_besides`] of no
    /// nonce).
    #[cfg(test)]
    fn available(&self, owner: Option<&StateOwner>) -> Option<U256> {
        self.left_besides(owner, None)
            .map(|(left, group)| left.saturating_sub(group))
    }

    /// Why the buyer's balance does not hold a BUY of at most `most` at the
    /// nonce `next`, its exit and the exits of what is open (BSC3-027): the
    /// BUY and the alternatives of its nonce count once, at the most of
    /// them, and so do their exits.
    fn unfunded(
        &self,
        most: U256,
        owner: Option<&StateOwner>,
        next: Option<u64>,
    ) -> Option<String> {
        let (approval, sell) = self.exit_gas();
        let key = next.and_then(|n| self.buyer_key(n));
        let Some((left, group)) = self.left_besides(owner, key) else {
            return Some("no balance read".into());
        };
        let need = most.max(group) + approval + sell + self.exit_reserve_besides(key);
        (left < need)
            .then(|| format!("{left} wei left, a BUY with its exit and the open exits need {need}"))
    }

    /// Why no new BUY of at most `most` goes now (counted): an open
    /// position's exit waits for the buyer's nonce first (BSC3-048), the
    /// owner's gate admits only exits (BSC3-071), or the balance does not
    /// hold it with the exits (BSC3-027).
    fn buy_refused(
        &mut self,
        most: U256,
        owner: Option<&StateOwner>,
        next: Option<u64>,
    ) -> Option<Refusal> {
        let (refusal, stopped) = self.buy_refusal(most, owner, next)?;
        match (refusal, stopped) {
            (Refusal::ExitStopped, Some(why)) => self.count_stopped(why),
            (Refusal::ExitFirst, Some(crate::sources::LIVE_NONCE_BURN_FIRST)) => {
                self.burn_first += 1
            }
            (Refusal::ExitFirst, _) => self.exit_first += 1,
            (Refusal::Owner, _) => self.buys_refused += 1,
            (Refusal::Unfunded, _) => self.unfunded_refused += 1,
            (Refusal::Dispatch, _) => self.provider_refused += 1,
            (Refusal::GrantLimit, Some(why)) => {
                match self.limit_refused.iter_mut().find(|(w, _)| *w == why) {
                    Some((_, n)) => *n += 1,
                    None => self.limit_refused.push((why, 1)),
                }
            }
            _ => {}
        }
        Some(refusal)
    }

    /// 31.10. order: why the grant's limits refuse a new BUY at the nonce
    /// `next` now (the canonical `head`), if they do (all from the ledger's
    /// book, durable, and this process's hand-overs): no limits named; as
    /// many positions pending, open or exiting as allowed; the realized net
    /// loss of the grant's positions, or of the positions that closed on
    /// this UTC day, at its bound; the last BUY (handed over here, opened
    /// or failed on chain, or still pending from before this process: at
    /// the latest when it began) less than the interval ago. Review: a
    /// pending BUY whose bundle window is over at `next` is the new BUY's
    /// alternative (at most one of them lands), never another position.
    fn limit_refusal(&self, next: Option<u64>, head: Option<u64>) -> Option<&'static str> {
        use crate::sources::{
            LIVE_BUY_INTERVAL, LIVE_DAILY_LOSS_MAX, LIVE_GRANT_LIMITS_MISSING,
            LIVE_OPEN_POSITIONS_MAX, LIVE_REALIZED_LOSS_MAX, day_index,
        };
        let x = self.executor.as_ref()?;
        let generation = x.end().armed().grant_generation();
        let Some(limits) = x.end().armed().grant().limits else {
            return Some(LIVE_GRANT_LIMITS_MISSING);
        };
        let positions = &self.book.positions;
        let alternative = |p: &crate::live_ledger::Position| {
            p.state == PositionState::Pending
                && self.book.txs.iter().any(|t| {
                    t.released.tx_hash == p.lot
                        && matches!(t.state, FactKind::Released | FactKind::Unknown)
                        && Some(t.released.nonce) == next
                        && t.released
                            .key()
                            .is_none_or(|k| Some(k) == next.and_then(|n| self.buyer_key(n)))
                        && head.is_some_and(|h| t.window_over(h))
                })
        };
        let open = positions
            .iter()
            .filter(|p| {
                matches!(
                    p.state,
                    PositionState::Pending | PositionState::Open | PositionState::Exiting
                ) && !alternative(p)
            })
            .count();
        if open >= limits.max_open_positions as usize {
            return Some(LIVE_OPEN_POSITIONS_MAX);
        }
        let net = |p: &crate::live_ledger::Position| p.pnl_wei.unwrap_or(0);
        let now = (self.wall)();
        let lost = |net: i128, most: u128| net < 0 && net.unsigned_abs() >= most;
        // R3b review LOW-3: a nonce burn's gas is a loss of its grant and
        // its day too (of no lot).
        let burnt = |of: &dyn Fn(&crate::live_ledger::Tx) -> bool| {
            i128::try_from(self.book.burn_gas(of)).unwrap_or(i128::MAX)
        };
        let granted: i128 = positions
            .iter()
            .filter(|p| p.grant_generation == generation)
            .map(net)
            .fold(0, i128::saturating_add)
            .saturating_sub(burnt(&|t| t.released.grant_generation == generation));
        if lost(granted, limits.max_realized_loss_wei) {
            return Some(LIVE_REALIZED_LOSS_MAX);
        }
        let today = day_index(now);
        let daily: i128 = positions
            .iter()
            .filter(|p| p.closed_ms.is_some_and(|t| day_index(t) == today))
            .map(net)
            .fold(0, i128::saturating_add)
            .saturating_sub(burnt(&|t| {
                t.inclusion
                    .as_ref()
                    .and_then(|f| f.block)
                    .is_some_and(|b| day_index(b.2) == today)
            }));
        if lost(daily, limits.max_daily_realized_loss_wei) {
            return Some(LIVE_DAILY_LOSS_MAX);
        }
        if limits.min_buy_interval_ms > 0 {
            let pending = positions
                .iter()
                .any(|p| p.state == PositionState::Pending && !alternative(p));
            let failed = |p: &crate::live_ledger::Position| {
                (p.state == PositionState::Failed)
                    .then_some(p.closed_ms)
                    .flatten()
            };
            let last = positions
                .iter()
                .flat_map(|p| [p.opened_ms, failed(p)])
                .flatten()
                .chain(self.last_buy_ms)
                .chain(pending.then_some(self.started_ms))
                .max();
            if last.is_some_and(|t| now.saturating_sub(t) < limits.min_buy_interval_ms) {
                return Some(LIVE_BUY_INTERVAL);
            }
        }
        None
    }

    /// [`Self::buy_refused`] without counting (BSC3-076: whether a C3 BUY
    /// may come at all, for its lease), with the stopped exit's reason.
    fn buy_refusal(
        &self,
        most: U256,
        owner: Option<&StateOwner>,
        next: Option<u64>,
    ) -> Option<(Refusal, Option<&'static str>)> {
        // REV.1.0 R2 (PHASENPLAN 27.10.: stop after a bad placement): a C3
        // BUY of this grant that landed behind another trader's BUY,
        // outside its launch's block or without tokens stops new BUYs
        // until the operator grants anew (from the ledger: across arms and
        // restarts, an orphaned placement too); the exits go on. Before
        // any other reason (review L3: the status names it, not a passing
        // limit).
        let misplaced = self.executor.as_ref().is_some_and(|x| {
            self.book
                .misplaced
                .contains(&x.end().armed().grant_generation())
        });
        if misplaced {
            return Some((
                Refusal::GrantLimit,
                Some(crate::sources::LIVE_PLACEMENT_VIOLATED),
            ));
        }
        // R3 (BSC3-045): a ledger entry quarantined after the grant was
        // written may have said what holds BUYs back; a new grant decides.
        let quarantined = self.executor.as_ref().is_some_and(|x| {
            self.quarantine
                .is_some_and(|q| x.end().armed().grant_generation() <= q)
        });
        if quarantined {
            return Some((
                Refusal::GrantLimit,
                Some(crate::sources::LIVE_LEDGER_QUARANTINED),
            ));
        }
        // BSC3-030 review: while an open position's exits wait for the
        // operator, no new BUY (an exit failing for good would strand
        // every new position too).
        let stopped = self
            .book
            .positions
            .iter()
            .filter(|p| matches!(p.state, PositionState::Open | PositionState::Exiting))
            .find_map(|p| self.exit_stopped(&p.token));
        if let Some(why) = stopped {
            return Some((Refusal::ExitStopped, Some(why)));
        }
        // 01.11. (R1): a lot no strategy can exit (off the curve, outside
        // the priced scope, unreadable) waits for the operator; one not
        // held yet waits for its curve read (a restart, a gap, a reorg);
        // before the runtime told, every open lot is one of those.
        match &self.exit_deciders {
            Some(d) => {
                if let Some((_, why)) = d
                    .stranded
                    .iter()
                    .find(|(t, _)| self.book.open_lot(t).is_some())
                {
                    return Some((Refusal::ExitStopped, Some(why)));
                }
                if d.undecided.iter().any(|t| self.book.open_lot(t).is_some()) {
                    return Some((Refusal::ExitFirst, None));
                }
            }
            None if !self.open_positions().is_empty() => {
                return Some((Refusal::ExitFirst, None));
            }
            None => {}
        }
        // An exit step out, or (R3b) a nonce burn out: it decides first.
        let exit_out = self
            .book
            .unresolved()
            .any(|t| t.released.intent != LiveIntentKind::Buy);
        if !self.exits_wanted.is_empty() || exit_out {
            return Some((Refusal::ExitFirst, None));
        }
        // R3b: a nonce burn wanted goes first (its nonce is the next). Read
        // from the book as the nonce's exemption is (review LOW-N1: a
        // request folded in between ticks exempts nothing a BUY may take).
        if self.burn_target().is_some() {
            return Some((
                Refusal::ExitFirst,
                Some(crate::sources::LIVE_NONCE_BURN_FIRST),
            ));
        }
        if self.buys_closed.is_some() {
            return Some((Refusal::Owner, None));
        }
        let head = owner.and_then(|o| o.head()).map(|h| h.number);
        if let Some(why) = self.limit_refusal(next, head) {
            return Some((Refusal::GrantLimit, Some(why)));
        }
        if self.unfunded(most, owner, next).is_some() {
            return Some((Refusal::Unfunded, None));
        }
        // BSC3-051: a provider refused a BUY under this grant (the end's
        // answer, or the ledger's across arms and restarts): no further BUY
        // until the operator grants anew (nothing is sent again, no nonce
        // reused).
        let book = &self.book;
        let refused = self.executor.as_ref().is_some_and(|x| {
            let grant = x.end().armed().grant_generation();
            x.end().buy_refused().is_some()
                || book.txs.iter().any(|t| {
                    t.released.intent == LiveIntentKind::Buy
                        && t.released.grant_generation == grant
                        && t.responses
                            .iter()
                            .any(|r| r.ends_with(":REJECT") || r.ends_with(":RATE_LIMITED"))
                })
        });
        if refused {
            return Some((Refusal::Dispatch, None));
        }
        None
    }

    /// The providers' answers to what the armed end released, read without
    /// waiting and kept in memory (BSC3-051): before a decision, nothing
    /// written. [`Self::record_responses`] writes them.
    pub fn poll_responses(&mut self, clock: &dyn Clock) {
        if let Some(x) = self.executor.as_mut() {
            // BSC3-025 review: a connection replaced while armed is usable
            // as soon as it is established.
            x.end_mut().install_ready();
            x.poll_responses(clock);
            self.answers.extend(x.end_mut().take_answers());
        }
    }

    /// REV.1.0 R2: the answers read since the last call, with their times.
    pub fn take_answers(&mut self) -> Vec<crate::live_end::Answer> {
        std::mem::take(&mut self.answers)
    }

    /// The answers read so far as ledger facts, folded: after a hand-over
    /// or in the reconcile, never before one.
    pub fn record_responses(&mut self) {
        if let Some(x) = self.executor.as_mut() {
            x.end_mut().record_responses();
        }
        self.handed();
    }

    /// One Strategy C decision, when armed.
    pub fn on_strategy(
        &mut self,
        action: &crate::runtime_strategy::StrategyAction,
        owner: &StateOwner,
        clock: &dyn Clock,
        now_unix_ms: u64,
    ) -> Option<(Vec<ExecIntent<LiveHandoff>>, Option<Refusal>)> {
        let granted = self
            .executor
            .as_ref()?
            .end()
            .armed()
            .grant()
            .strategies
            .strategy_c();
        self.poll_responses(clock);
        let next = self.free_nonce(owner);
        if let crate::runtime_strategy::StrategyAction::Buy { input_wei, .. } = *action {
            // 01.11. (R1): Strategy C decides every lot's exits; its own
            // BUYs only under a grant naming it.
            if !granted {
                self.strategy_buys_refused += 1;
                return Some((Vec::new(), Some(Refusal::Owner)));
            }
            let x = &self.exec.exec;
            let most = input_wei + U256::from(x.gas_price_wei) * U256::from(x.gas_limit);
            if let Some(refusal) = self.buy_refused(most, Some(owner), next) {
                return Some((Vec::new(), Some(refusal)));
            }
        }
        // R1 review: an exit of a token without an open lot (a position the
        // strategy entered in its own PAPER book while the LIVE BUY was
        // refused) is no LIVE exit: refused before anything is given back.
        if let crate::runtime_strategy::StrategyAction::Exit { token, .. } = *action
            && self.book.open_lot(&token).is_none()
        {
            return Some((Vec::new(), Some(Refusal::NotHeld)));
        }
        // BSC3-030 review: a stopped exit waits for the operator.
        if let crate::runtime_strategy::StrategyAction::Exit { token, .. } = *action
            && let Some(why) = self.exit_stopped(&token)
        {
            self.count_stopped(why);
            return Some((Vec::new(), Some(Refusal::ExitStopped)));
        }
        // BSC3-076: the nonce a C3 BUY's lease holds goes to this intent:
        // the lease goes back unused first (the next pass leases anew).
        if self
            .executor
            .as_ref()
            .and_then(|x| x.end().lease())
            .is_some_and(|l| Some(l.nonce()) == next)
        {
            self.yield_lease();
        }
        let reserve = Some(
            self.exit_reserve_besides(next.and_then(|n| self.buyer_key(n)))
                + self.own_exit_excess(),
        );
        let repriced = match *action {
            crate::runtime_strategy::StrategyAction::Exit { token, .. } => {
                self.exit_repriced(&token, next, owner.head().map(|h| h.number))
            }
            crate::runtime_strategy::StrategyAction::Buy { .. } => None,
        };
        let before = self.executor.as_ref()?.end().released().len();
        let handed = strategy_to(
            self.executor.as_mut()?,
            action,
            owner,
            next,
            reserve,
            repriced,
            clock,
            now_unix_ms,
        );
        if let crate::runtime_strategy::StrategyAction::Exit { token, .. } = *action {
            self.want_exit(token);
        }
        self.send_public_exits(&handed.0);
        if handed
            .0
            .iter()
            .any(|i| i.kind == crate::paper_exec::IntentKind::Buy)
            || self.released_buy_since(before)
        {
            self.last_buy_ms = Some((self.wall)());
        }
        self.record_responses();
        Some(handed)
    }

    /// R5 (BSC3-030): with the public exit route ON, the exit steps handed
    /// to the builders also go to the public endpoint.
    fn send_public_exits(&mut self, handed: &[ExecIntent<LiveHandoff>]) {
        if !self.exec.exec.live_public_exit {
            return;
        }
        for i in handed.iter().filter(|i| {
            matches!(
                i.kind,
                crate::paper_exec::IntentKind::Approve | crate::paper_exec::IntentKind::Sell
            )
        }) {
            self.public_send(i.handoff.tx_hash, &i.raw);
        }
    }

    /// R5 (BSC3-030): whether the DEX exit takes `token`'s lot now (the head
    /// at `head`): ON, armed, the lot open, none of its exit steps out
    /// within its window (one lost after it goes again, as a strategy exit
    /// does), its exits not stopped for the operator.
    pub fn dex_exit_due(&self, token: &[u8; 20], head: u64) -> bool {
        self.exec.exec.live_dex_exit
            && self.executor.is_some()
            && self.book.open_lot(token).is_some()
            && self.exit_stopped(token).is_none()
            && !self.book.unresolved().any(|t| {
                t.released.token == *token && t.released.intent.exit_step() && !t.window_over(head)
            })
    }

    /// R5 (BSC3-030): what a DEX exit of `token`'s lot sells now: its
    /// ledger tokens as the armed executor sells them (none: not armed or
    /// not held).
    pub fn dex_exit_amount(&self, token: &[u8; 20]) -> Option<U256> {
        let tokens = self.book.open_lot(token)?.tokens;
        self.executor.as_ref()?.exit_amount(*token, tokens)
    }

    /// R5 (BSC3-030): the exit of `token`'s lot, its token migrated to its
    /// DEX, at the Portal's quote `quoted` of `tokens` (read at a canonical
    /// block): as an exit decision of the strategy, the Portal routing the
    /// SELL through its DEX router. None while not due
    /// ([`Self::dex_exit_due`]).
    pub fn on_dex_exit(
        &mut self,
        token: [u8; 20],
        tokens: U256,
        quoted: U256,
        owner: &StateOwner,
        clock: &dyn Clock,
        now_unix_ms: u64,
    ) -> Option<(Vec<ExecIntent<LiveHandoff>>, Option<Refusal>)> {
        if !owner
            .head()
            .is_some_and(|h| self.dex_exit_due(&token, h.number))
        {
            return None;
        }
        self.poll_responses(clock);
        let next = self.free_nonce(owner);
        if self
            .executor
            .as_ref()
            .and_then(|x| x.end().lease())
            .is_some_and(|l| Some(l.nonce()) == next)
        {
            self.yield_lease();
        }
        let repriced = self.exit_repriced(&token, next, owner.head().map(|h| h.number));
        let handed = crate::runtime::exec::dex_exit_to(
            self.executor.as_mut()?,
            token,
            tokens,
            quoted,
            owner,
            next,
            repriced,
            clock,
            now_unix_ms,
        );
        self.want_exit(token);
        self.send_public_exits(&handed.0);
        self.record_responses();
        Some(handed)
    }

    /// R5 (BSC3-030): hands an exit step's signed bytes to the public route,
    /// its worker started at the first (on the loopback route the
    /// stand-in, else the configured endpoint).
    fn public_send(&mut self, tx_hash: [u8; 32], raw: &[u8]) {
        use crate::public_exit::{PublicExit, PublicTarget};
        if self.public_exit.is_none() {
            let target = match &self.setup.route {
                LiveRouteConfig::Loopback { port } => PublicTarget::Loopback(*port),
                LiveRouteConfig::Builders { .. } => {
                    PublicTarget::Https(self.exec.exec.live_public_exit_rpc.clone())
                }
            };
            self.public_exit = self
                .follower_limits
                .and_then(|limits| PublicExit::spawn(&target, &limits).ok());
        }
        match self
            .public_exit
            .as_ref()
            .is_some_and(|p| p.send(tx_hash, raw))
        {
            true => self.public_counts.sent += 1,
            false => {
                self.public_exit = None;
                self.public_counts.unstarted += 1;
            }
        }
    }

    /// R5 (BSC3-030): the gas price `token`'s exit goes at from the buyer's
    /// free nonce `next` when not the arm's: with
    /// BSC_EXEC_LIVE_EXIT_REPRICE_BPS above 0, an exit step of it released
    /// at that nonce before, its window over at `head` and unresolved, goes
    /// at the highest such step's price raised by the bps, at most the
    /// grant's risk maximum (at it already: none, its same bytes are
    /// refused as LIVE_INTENT_SEEN).
    fn exit_repriced(&self, token: &[u8; 20], next: Option<u64>, head: Option<u64>) -> Option<u64> {
        let x = &self.exec.exec;
        let (next, head) = (next?, head?);
        if x.live_exit_reprice_bps == 0 {
            return None;
        }
        let max = self
            .executor
            .as_ref()?
            .end()
            .armed()
            .grant()
            .risk?
            .max_gas_price_wei;
        let prior = self
            .book
            .unresolved()
            .filter(|t| {
                t.released.token == *token
                    && t.released.intent.exit_step()
                    && t.released.nonce == next
                    && t.window_over(head)
            })
            .filter_map(|t| {
                let limit = U256::from(match t.released.intent {
                    LiveIntentKind::Approve => x.approve_gas_limit,
                    _ => x.sell_gas_limit,
                });
                let paid = t
                    .released
                    .max_wei?
                    .saturating_sub(t.released.input_wei.unwrap_or_default());
                u64::try_from((paid + limit - U256::from(1u8)) / limit).ok()
            })
            .max()?;
        repriced(prior, x.live_exit_reprice_bps, x.gas_price_wei, max)
    }

    /// R4 (PHASENPLAN §20): `BSC_EXEC_LIVE_PUBLIC_ONLY` is OFF and no
    /// non-public LIVE source is a signal now (R6: healthy and delivering
    /// full transactions at the runtime's last status pass): a sighting is
    /// no C3 BUY's signal.
    fn public_only_locked(&self) -> bool {
        !self.exec.exec.live_public_only
            && !self
                .sources_live
                .iter()
                .any(|s| !s.public() && self.signals(*s))
    }

    /// R6: whether `source` is healthy and delivers full transactions, as
    /// the runtime last told ([`Self::set_source_signals`]).
    fn signals(&self, source: SourceId) -> bool {
        self.source_signals.iter().any(|(s, h, c)| {
            *s == source
                && *h == crate::ingress::Health::Healthy
                && *c == crate::sources::SourceCapability::Full
        })
    }

    /// R6: each source's health and observed capability, from the runtime's
    /// status pass.
    pub fn set_source_signals(
        &mut self,
        signals: Vec<(
            SourceId,
            crate::ingress::Health,
            crate::sources::SourceCapability,
        )>,
    ) {
        self.source_signals = signals;
    }

    /// Review LOW-2: a transaction of the buyer (one written before its
    /// sender was recorded counts as the buyer's).
    fn buyers(&self, t: &crate::live_ledger::Tx) -> bool {
        t.released
            .origin
            .is_none_or(|o| o.sender == self.setup.buyer)
    }

    /// R3b: an unresolved transaction of the buyer whose nonce the operator
    /// wants burnt, with the switch ON.
    fn burnable(&self, t: &crate::live_ledger::Tx) -> bool {
        self.exec.exec.live_nonce_burn
            && t.decided.burn_requested
            && matches!(t.state, FactKind::Released | FactKind::Unknown)
            && self.buyers(t)
    }

    /// R3b: the burn wanted, the first [`Self::burnable`] transaction in
    /// the ledger's order. Every check reads it from the book when it
    /// checks: the nonce's exemption, a BUY's refusal, the arm, the funds,
    /// the status (review LOW-N1: never one of them a tick behind).
    fn burn_target(&self) -> Option<&crate::live_ledger::Tx> {
        // Nothing to scan with the switch OFF (a C3 entry asks it).
        if !self.exec.exec.live_nonce_burn {
            return None;
        }
        self.book.unresolved().find(|t| self.burnable(t))
    }

    /// A burn's spend at the configured gas price: its transfer gas.
    fn burn_cost(&self) -> U256 {
        U256::from(self.exec.exec.gas_price_wei)
            * U256::from(darkbot_bsc_trading::venues::flap::binding::TRANSFER_GAS)
    }

    /// Review MEDIUM-2: a burn of `nonce` at this gas price went before
    /// (its bytes would be the same: the owner admits them once; another
    /// only at another gas price).
    fn burn_spent(&self, nonce: u64) -> bool {
        let cost = self.burn_cost();
        self.book.txs.iter().any(|t| {
            t.released.intent == LiveIntentKind::Burn
                && t.released.nonce == nonce
                && t.released.max_wei == Some(cost)
        })
    }

    /// REV.1.0 R3b: the operator's nonce burn, armed, when one is wanted and
    /// its transaction's nonce is the buyer's next (C2's clean next nonce,
    /// the arm's next strategy lane): the buyer's plain transfer of
    /// nothing to itself goes through that lane as an exit step goes (the
    /// owner admits it; the caller journals it). A burn out holds the nonce
    /// while its window runs (it is unresolved at it); included, every
    /// transaction at its nonce is REPLACED by it. One that never landed
    /// (its window over, or withdrawn) is spent all the same: its bytes
    /// are never admitted again, another goes only at another gas price
    /// (`BSC_EXEC_GAS_PRICE_WEI` with `BSC_STRATEGY_C_GAS_PRICE_WEI`, which
    /// must equal it; NONCE_BURN_SPENT meanwhile). An exit at that nonce
    /// replaces the transaction as well. A refusal is counted and the burn
    /// asked again at the next pass. Requested transactions at two nonces
    /// (an orphaned approval and its SELL) are burnt one at a time: the
    /// higher blocks the path until the lower is taken (`nonce_free`).
    pub fn burn(
        &mut self,
        owner: &StateOwner,
        clock: &dyn Clock,
        now_unix_ms: u64,
    ) -> Vec<ExecIntent<LiveHandoff>> {
        let Some(nonce) = self.burn_target().map(|t| t.released.nonce) else {
            return Vec::new();
        };
        if self.burn_spent(nonce) {
            return Vec::new();
        }
        self.poll_responses(clock);
        let next = self.free_nonce(owner);
        let Some(x) = self.executor.as_mut() else {
            return Vec::new();
        };
        if next != Some(nonce) || x.strategy_next_nonce() != nonce {
            return Vec::new();
        }
        let (intent, refusal) = crate::runtime::exec::burn_to(x, owner, next, clock, now_unix_ms);
        if refusal.is_some() {
            self.burns_refused += 1;
        }
        self.record_responses();
        intent.into_iter().collect()
    }

    /// An exit the strategy decided stays wanted while its position is
    /// open and its SELL has not gone (a refusal that consumed nothing, an
    /// approval that waits for its nonce, an arm that ended in between).
    fn want_exit(&mut self, token: [u8; 20]) {
        let sold = self.executor.as_ref().is_some_and(|x| x.exited(&token));
        let open = self.position_open(&token);
        self.exits_wanted.retain(|t| *t != token);
        if open && !sold {
            self.exits_wanted.push(token);
        }
    }

    /// Counts one refusal because of a stopped exit, by its reason.
    fn count_stopped(&mut self, why: &'static str) {
        match self.exits_stopped.iter_mut().find(|(w, _)| *w == why) {
            Some((_, n)) => *n += 1,
            None => self.exits_stopped.push((why, 1)),
        }
    }

    /// BSC3-030 review: why the exits of `token`'s open position wait for
    /// the operator, if they do: its exit steps reverted
    /// `BSC_EXEC_LIVE_EXIT_REVERTS_MAX` times (counted in the ledger, so a
    /// restart counts them too), or the last of them used all its gas (the
    /// same limit fails again). Never an automatic repetition past it.
    fn exit_stopped(&self, token: &[u8; 20]) -> Option<&'static str> {
        let lot = self.book.open_lot(token)?.lot;
        // R3: the steps reverted before the operator's last RESUMED stop
        // nothing.
        let excused = self.book.tx(&lot).map(|t| &t.decided.excused);
        let reverted: Vec<&crate::live_ledger::Tx> = self
            .book
            .txs
            .iter()
            .filter(|t| {
                t.lot == Some(lot)
                    && t.released.intent.exit_step()
                    && t.state == FactKind::Reverted
                    && !excused.is_some_and(|e| e.contains(&t.released.tx_hash))
            })
            .collect();
        // Out of gas stops only while the step would go with no more gas
        // than it had (review LOW: a raised limit lets it go on).
        let exec = &self.exec.exec;
        let out_of_gas = reverted
            .iter()
            .max_by_key(|t| t.released.nonce)
            .is_some_and(|t| {
                let used = t.inclusion.as_ref().and_then(|f| f.gas_wei);
                let most = t
                    .released
                    .max_wei
                    .map(|m| m.saturating_sub(t.released.input_wei.unwrap_or_default()));
                let limit = match t.released.intent {
                    LiveIntentKind::Approve => exec.approve_gas_limit,
                    _ => exec.sell_gas_limit,
                };
                let now = U256::from(exec.gas_price_wei) * U256::from(limit);
                matches!((used, most), (Some(used), Some(most)) if used >= most && now <= most)
            });
        if out_of_gas {
            return Some(crate::sources::LIVE_EXIT_OUT_OF_GAS);
        }
        (reverted.len() as u64 >= self.exec.exec.live_exit_reverts_max)
            .then_some(crate::sources::LIVE_EXIT_REVERTS_EXHAUSTED)
    }

    /// Review of R4: an exit the strategy decided while this path is not
    /// armed (a re-arm, a reconnect) of a lot the ledger holds open stays
    /// wanted for the next arm ([`LiveRuntime::exits_to_continue`]), never
    /// PAPER's (the strategy decides it once). Whether it was this path's.
    pub fn want_unarmed_exit(&mut self, token: [u8; 20]) -> bool {
        if self.executor.is_some() || !self.position_open(&token) {
            return false;
        }
        self.want_exit(token);
        true
    }

    /// The wanted exits that may go on now (BSC3-055): armed, and the
    /// strategy's next nonce is the buyer's canonical next nonce (C2). The
    /// strategy's exit again, for each.
    pub fn exits_to_continue(&self, owner: &StateOwner) -> Vec<[u8; 20]> {
        let Some(x) = &self.executor else {
            return Vec::new();
        };
        if self.free_nonce(owner) != Some(x.strategy_next_nonce()) {
            return Vec::new();
        }
        self.exits_wanted
            .iter()
            .copied()
            .filter(|t| !x.exited(t) && self.exit_stopped(t).is_none())
            .collect()
    }

    /// C2's evidence about the buyer's nonces, applied to the armed lanes.
    /// (The ledger resolves its transactions from the follower's blocks,
    /// armed or not: [`LiveRuntime::on_canonical`].)
    pub fn apply_evidence(&mut self, evidence: &[LaneEvidence]) -> Vec<Option<IntentState>> {
        let outcomes: Vec<Option<IntentState>> = match self.executor.as_mut() {
            Some(x) => evidence
                .iter()
                .map(|e| {
                    x.apply_evidence(match e {
                        LaneEvidence::Inclusion(p) => NonceEvidence::Inclusion(p),
                        LaneEvidence::Finality(p) => NonceEvidence::Finality(p),
                        LaneEvidence::Orphan(p) => NonceEvidence::Orphan(p),
                    })
                })
                .collect(),
            None => Vec::new(),
        };
        outcomes
    }

    pub fn draft(&self) -> Draft {
        let (state, reason) = self.phase.name();
        let mut d = Draft::new(&l::SCHEMA);
        d.set(l::STATE, state)
            .set(
                l::BUYER,
                format!("0x{}", alloy_primitives::hex::encode(self.setup.buyer)).as_str(),
            )
            .set(
                l::ROUTE,
                match self.setup.route {
                    LiveRouteConfig::Builders { .. } => "BUILDERS",
                    LiveRouteConfig::Loopback { .. } => "LOOPBACK",
                },
            )
            .set(l::PROBES, self.probes);
        if let Some(r) = reason {
            d.set(l::REASON, r);
        }
        if let Some(o) = &self.observation {
            d.set(
                l::BALANCE_WEI,
                darkbot_bsc_record::Value::U256(o.caller_balance_wei.to_be_bytes()),
            );
        }
        let x = self.executor.as_ref();
        if let Some(n) = self.next_nonce.filter(|_| x.is_some()) {
            d.set(l::NEXT_NONCE, n);
        }
        if let Some(x) = x {
            let generation = x.end().armed().grant_generation();
            d.set(l::GRANT_GENERATION, generation);
            // BSC3-078: the tabs read the owner's gate besides it.
            if let Some(a) = self.held_attempt(Some(generation)) {
                d.set(l::LEASE_ATTEMPT, darkbot_bsc_record::Value::B256(a));
            }
            if let Some(r) = x.end().last_refusal() {
                d.set(l::LAST_REFUSAL, r);
            }
        }
        // R1: who decides the open lots' exits (before the runtime told:
        // every open lot undecided).
        let deciders = self.exit_deciders.clone().unwrap_or_else(|| ExitDeciders {
            undecided: self
                .open_positions()
                .iter()
                .filter_map(|p| match p {
                    PositionChange::Opened { token, .. } => Some(*token),
                    PositionChange::Ended { .. } => None,
                })
                .collect(),
            ..ExitDeciders::default()
        });
        for t in &deciders.undecided {
            d.push(
                l::EXIT_UNDECIDED,
                format!("0x{}", alloy_primitives::hex::encode(t)).as_str(),
            );
        }
        for (t, why) in &deciders.stranded {
            d.push(
                l::STRANDED,
                format!("0x{} {why}", alloy_primitives::hex::encode(t)).as_str(),
            );
        }
        d.set(l::LOT_READ_FAULTS, deciders.faults);
        if let Some(f) = &deciders.fault {
            d.set(l::LOT_READ_FAULT, f.as_str());
        }
        // R3b: the receipt recovery of the unresolved transactions.
        for (tx, why) in &self.recovery.pending {
            d.push(
                l::RECOVERY_PENDING,
                format!("0x{} {why}", alloy_primitives::hex::encode(tx)).as_str(),
            );
        }
        d.set(l::RECOVERED, self.recovery.recovered)
            .set(l::RECOVERY_FAULTS, self.recovery.faults);
        if let Some(f) = &self.recovery.fault {
            d.set(l::RECOVERY_FAULT, f.as_str());
        }
        let released = x.map_or(&[][..], |x| x.end().released());
        let ev = x.map(|x| x.evidence()).unwrap_or_default();
        d.set(l::RELEASED, released.len() as u64)
            .set(
                l::UNKNOWN,
                released.iter().filter(|r| !r.handed_off).count() as u64,
            )
            .set(l::INCLUDED, ev.included)
            .set(l::REVERTED, ev.reverted)
            .set(l::REPLACED, ev.replaced)
            .set(l::FINAL, ev.finalized)
            .set(l::ORPHANED, ev.orphaned);
        d.set(
            l::POSITIONS_OPEN,
            self.book.count(PositionState::Open) + self.book.count(PositionState::Exiting),
        )
        .set(l::POSITIONS_CLOSED, self.book.count(PositionState::Closed))
        .set(l::POSITIONS_FAILED, self.book.count(PositionState::Failed))
        .set(
            l::REALIZED_PNL_WEI,
            self.book.realized_pnl_wei.to_string().as_str(),
        )
        .set(
            l::GAS_WEI,
            darkbot_bsc_record::Value::U256(self.book.gas_wei.to_be_bytes()),
        );
        if let Some(e) = &self.ledger_error {
            d.set(l::LEDGER_ERROR, e.as_str());
        }
        for (name, n) in [
            ("SOURCE_NOT_GRANTED", self.source_refused),
            ("SOURCE_NOT_LIVE", self.source_not_live),
            ("SOURCE_PUBLIC_ONLY", self.public_only_refused),
            ("SOURCE_UNHEALTHY", self.source_unhealthy),
            ("STRATEGY_BUYS_NOT_GRANTED", self.strategy_buys_refused),
            ("EXIT_FIRST", self.exit_first),
            ("BUYS_CLOSED", self.buys_refused),
            ("BUYS_UNFUNDED", self.unfunded_refused),
            ("BUYS_PROVIDER_REFUSED", self.provider_refused),
            ("LEASE_REFUSED", self.lease_refused),
            ("LEASE_NONCE_TAKEN_BY_OTHER", self.lease_foreign),
            ("SLOT_RETURN_FAILED", self.slot_return_failed),
            ("OPERATOR_EXIT_LOCKED", self.operator_exits_locked),
            ("NONCE_BURN_LOCKED", self.burns_locked.len() as u64),
            ("NONCE_BURN_REFUSED", self.burns_refused),
            ("NONCE_BURN_FIRST", self.burn_first),
            ("NONCE_BURN_WANTED", u64::from(self.burn_target().is_some())),
            (
                "NONCE_BURN_SPENT",
                u64::from(
                    self.burn_target()
                        .is_some_and(|t| self.burn_spent(t.released.nonce)),
                ),
            ),
        ] {
            if n > 0 {
                d.push(l::REFUSALS, format!("{name}={n}").as_str());
            }
        }
        for (why, n) in &self.exits_stopped {
            d.push(l::REFUSALS, format!("{why}={n}").as_str());
        }
        for (why, n) in &self.limit_refused {
            d.push(l::REFUSALS, format!("{why}={n}").as_str());
        }
        for (r, n) in x.into_iter().flat_map(|x| {
            x.counters()
                .refusals
                .iter()
                .chain(x.strategy_counters().refusals.iter())
        }) {
            d.push(l::REFUSALS, format!("{}={n}", r.name()).as_str());
        }
        for (name, n) in x.map(|x| x.end().responses().named()).unwrap_or_default() {
            if n > 0 {
                d.push(l::RESPONSES, format!("{name}={n}").as_str());
            }
        }
        // R5: the public exit route's answers beside the builders'.
        for (name, n) in self.public_counts.named() {
            if n > 0 {
                d.push(l::RESPONSES, format!("{name}={n}").as_str());
            }
        }
        // BSC3-043: the LIVE path's own release latencies, never PAPER's.
        let latencies = x.map(|x| x.latencies());
        d.set(l::LATENCY_SAMPLES, latencies.map_or(0, |l| l.len() as u64));
        if let Some((p50, p99, max)) = latencies.and_then(|l| l.quantiles()) {
            d.set(l::LATENCY_P50_NS, p50)
                .set(l::LATENCY_P99_NS, p99)
                .set(l::LATENCY_MAX_NS, max);
        }
        d
    }
}

#[cfg(test)]
#[path = "runtime_live_tests.rs"]
mod tests;

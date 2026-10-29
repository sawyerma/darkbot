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

use crate::chain_follower::{CanonicalEvent, FollowerLimits};
use crate::live_end::{LiveEnd, LiveHandoff, LiveRoute};
use crate::live_key::LiveKey;
use crate::live_ledger::{Book, Fact, FactKind, Ledger, PositionState, fold};
use crate::live_qualification;
use crate::paper_exec::{Arming, ExecBuy, ExecIntent, Executor, NonceEvidence, Refusal};
use crate::runtime::exec::{ExecSetup, Prober, launch_to, live_probe, strategy_to};
use crate::runtime::state::{LaneEvidence, StateOwner};
use crate::sources::{
    ComponentId, KEEP_GENERATIONS, LiveIntentKind, LiveStrategies, SourceId, Store,
};
use crate::submission::tls::{AuthorizationToken, TlsSetup};
use crate::submission::transport::Clock;
use crate::venue_probe::VenueObservation;
use alloy_primitives::{Address, U256};
use darkbot_bsc_record::Draft;
use darkbot_bsc_record::schema::bsc_runtime::runtime_status::live as l;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    VenueBuyTerms, WITNESSED_MAINNET_TERMS,
};
use darkbot_bsc_trading::intent::IntentState;
use darkbot_bsc_trading::venues::flap::deployment::{ValidatedDeployment, bsc_mainnet_v5_27_0};
use std::os::unix::fs::PermissionsExt;
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
    ConnectFailed(String),
    ArmFailed(String),
    Armed,
    /// Armed until the gate closed: the reason it ended.
    Ended(String),
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
    succeeded: bool,
    gas_wei: Option<U256>,
    tokens: Option<U256>,
    eth_wei: Option<U256>,
}

/// The components a grant of `strategies` arms.
fn components(strategies: LiveStrategies) -> Vec<ComponentId> {
    let mut out = Vec::with_capacity(2);
    if strategies.c3() {
        out.push(ComponentId::ExecC3);
    }
    if strategies.strategy_c() {
        out.push(ComponentId::StrategyC);
    }
    out
}

/// The BlockRazor authorization from an owner-only file.
fn token_from(path: &Path) -> Result<AuthorizationToken, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 {
        return Err(format!("{} is not a private regular file", path.display()));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    AuthorizationToken::new(text.trim().to_string()).map_err(|e| format!("{e:?}"))
}

fn wall_ms() -> u64 {
    // Unreadable: every grant counts as expired (BSC3-015: counted).
    crate::runtime::wall_or_expired(darkbot_bsc_record::observed_now())
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
    codehash_generation: u64,
    arms: u64,
    last_probe: Option<Instant>,
    /// When an arming last failed in a way a later pass may get past.
    last_attempt: Option<Instant>,
    probes: u64,
    disarm_errors: u64,
    next_nonce: Option<u64>,
    /// Launches first seen by a source the grant does not name.
    source_refused: u64,
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
}

/// The buyer's balance at a probe's block: a transaction of the ledger
/// included or reverted in a later block, or not yet, still comes out of
/// it at its most.
#[derive(Clone, Copy, Debug)]
struct Funds {
    balance: U256,
    block: u64,
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
            probes: 0,
            disarm_errors: 0,
            next_nonce: None,
            source_refused: 0,
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
        }
        .folded()
    }

    /// At the start, nothing armed: what the last process admitted and
    /// could not write is recovered, then the ledger folded.
    fn folded(mut self) -> Self {
        let _ = self.reconciled(wall_ms());
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
        let blocking = self
            .book
            .unresolved()
            .find(|t| t.released.nonce != next || !head.is_some_and(|head| t.window_over(head)));
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
            .filter(|t| t.released.intent != LiveIntentKind::Buy && t.window_over(head))
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
                self.ledger_error = None;
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
                self.nonces_taken = txs
                    .iter()
                    .filter(|t| t.sender == buyer)
                    .map(|t| (t.hash, t.nonce))
                    .collect();
                self.seen = txs
                    .iter()
                    .filter(|t| t.sender == buyer && self.book.tx(&t.hash).is_some())
                    .map(|t| Seen {
                        tx_hash: t.hash,
                        succeeded: t.succeeded,
                        gas_wei: t.cost.map(|c| U256::from(c.wei())),
                        tokens: None,
                        eth_wei: None,
                    })
                    .collect();
                Vec::new()
            }
            CanonicalEvent::FlapTrade(t) if t.origin == buyer => {
                if let Some(s) = self.seen.iter_mut().find(|s| s.tx_hash == t.tx_hash) {
                    s.tokens = Some(U256::from_be_bytes(t.amount));
                    s.eth_wei = Some(U256::from_be_bytes(t.eth));
                }
                Vec::new()
            }
            CanonicalEvent::BlockComplete { .. } => {
                let (Some(block), seen) = (self.block.take(), std::mem::take(&mut self.seen))
                else {
                    return Vec::new();
                };
                let taken = std::mem::take(&mut self.nonces_taken);
                let mut facts: Vec<Fact> = seen
                    .iter()
                    .filter_map(|s| {
                        let tx = self.book.tx(&s.tx_hash)?;
                        let kind = if s.succeeded {
                            FactKind::Included
                        } else {
                            FactKind::Reverted
                        };
                        Some(Fact {
                            block: Some(block),
                            gas_wei: s.gas_wei,
                            tokens: s.tokens,
                            eth_wei: s.eth_wei,
                            ..Fact::about(&tx.released, kind)
                        })
                    })
                    .collect();
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
        // A token's reported lot ends when it is no longer the token's open
        // lot; then the open lot is reported (a reorg may reopen an earlier
        // lot of a token bought again, BSC3-047).
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
        // An exit is wanted of an open position only.
        let book = &self.book;
        self.exits_wanted.retain(|t| book.open_lot(t).is_some());
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
            .filter(|f| f.kind == FactKind::Replaced && f.intent != LiveIntentKind::Buy)
        {
            // Our own exit step of the token took the nonce (a retry after
            // the window): the exit went on, nothing to reopen.
            let own = f.by_tx.and_then(|h| self.book.tx(&h)).is_some_and(|t| {
                t.released.token == f.token
                    && t.released.intent != LiveIntentKind::Buy
                    && t.state == FactKind::Included
            });
            if !own && self.position_open(&f.token) {
                let approval = f.intent == LiveIntentKind::Approve;
                if let Some(x) = self.executor.as_mut() {
                    x.reopen_exit(f.token, approval);
                }
                if !self.exits_wanted.contains(&f.token) {
                    self.exits_wanted.push(f.token);
                }
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

    /// The arm ends. Its fence stays while a transaction it may have
    /// released is unresolved or not known (BSC3-041): a later arm settles
    /// it only once the chain resolved them.
    fn disarm(&mut self) {
        // BSC3-051: the answers read since the last reconcile reach the
        // ledger before the end goes.
        if let Some(x) = self.executor.as_mut() {
            x.end_mut().record_responses();
        }
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
                let _ = tx.send(prober(&endpoint, &limits, &bsc_mainnet_v5_27_0(), caller));
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

    /// One position's exit at the signed gas price, as the executor
    /// charges it: its approval's gas and its SELL's.
    fn exit_gas(&self) -> (U256, U256) {
        let x = &self.exec.exec;
        let price = U256::from(x.gas_price_wei);
        (
            price * U256::from(x.approve_gas_limit),
            price * U256::from(x.sell_gas_limit),
        )
    }

    /// The most the exits of the ledger's lots may still cost
    /// (BSC3-027/057): per lot pending or open its SELL and, unless an
    /// approval of it is out or included, its approval. An exiting lot's
    /// SELL is out (it counts in what was spent).
    fn exit_reserve(&self) -> U256 {
        let (approval, sell) = self.exit_gas();
        self.book
            .positions
            .iter()
            .filter(|p| matches!(p.state, PositionState::Pending | PositionState::Open))
            // A lot is a BUY's (an exit step with none before it is not).
            .filter(|p| {
                self.book
                    .tx(&p.lot)
                    .is_some_and(|t| t.released.intent == LiveIntentKind::Buy)
            })
            .map(|p| {
                let approved = self.book.txs.iter().any(|t| {
                    t.lot == Some(p.lot)
                        && t.released.intent == LiveIntentKind::Approve
                        && matches!(
                            t.state,
                            FactKind::Released | FactKind::Unknown | FactKind::Included
                        )
                });
                match approved {
                    true => sell,
                    false => approval + sell,
                }
            })
            .fold(U256::ZERO, |sum, wei| sum + wei)
    }

    fn observed(&mut self, result: Result<VenueObservation, String>) {
        let observation = match result {
            Ok(o) => o,
            Err(e) => return self.stop(Phase::ProbeFailed(e)),
        };
        let generation = self.codehash_generation + 1;
        let validated = bsc_mainnet_v5_27_0().validate(&observation.deployment(), generation);
        let terms = observation.terms;
        let balance = observation.caller_balance_wei;
        let probed_at = observation.block_number;
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
                });
                let exits = open > U256::ZERO || self.positions_open();
                if balance < need && !(exits && balance >= approval.min(sell)) {
                    self.disarm();
                    self.validated = None;
                    self.phase = Phase::Unfunded(match exits {
                        true => {
                            format!("{balance} wei, the exits of the open positions need {open}")
                        }
                        false => format!("{balance} wei, a BUY with its exit needs {need}"),
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
        // The gate, as it holds now.
        let opened = match &self.boot {
            Some(boot) => {
                Store::open_in_session(&self.state_dir, KEEP_GENERATIONS, Ok(boot.clone()))
            }
            None => Store::open(&self.state_dir, KEEP_GENERATIONS),
        };
        let store = match opened {
            Ok(s) => s,
            Err(e) => return self.close(format!("owner state: {e}")),
        };
        let (verified, invalid) = qualification;
        if let Some(why) = invalid {
            return self.close(format!("LIVE_QUALIFICATION_INVALID: {why}"));
        }
        let strategies = match store.current() {
            Ok(g) => g.live.map(|grant| grant.strategies),
            Err(e) => return self.close(format!("owner state: {e}")),
        };
        let Some(strategies) = strategies else {
            return self.close(crate::sources::LIVE_GRANT_MISSING.into());
        };
        let components = components(strategies);
        let live_now = match store.live_now(&components, &verified.view, now_ms) {
            Ok(n) => {
                self.buys_closed = None;
                n
            }
            // BSC3-071: what only a BUY needs (its attempts, the grant's
            // room for another input) does not end the exits of open
            // positions under the same grant; BUYs stay refused.
            Err(why) if self.positions_open() => {
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
            // BSC3-048: this arm cannot take the buyer's free nonce (its
            // window over, or taken by an earlier arm's transaction that
            // landed late): a new arm takes it (an unresolved transaction
            // stays so in the ledger, its spend counted, until its nonce
            // is taken; the new arm settles this arm's fence).
            let reuse = owner.and_then(|o| self.free_nonce(o)).filter(|n| {
                rearm_due(
                    *n,
                    x.strategy_next_nonce(),
                    x.strategy_lanes().4,
                    self.book.unresolved().any(|t| t.released.nonce == *n),
                )
            });
            match reuse {
                None => {
                    if let Some(x) = self.executor.as_mut() {
                        let _ = x.end_mut().maintain(self.setup.connect_timeout);
                    }
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
        let refused: Vec<[u8; 20]> = self.paper_sender.into_iter().collect();
        let key = match LiveKey::load(
            &self.setup.key_path,
            &self.repo_root,
            self.setup.buyer,
            &refused,
            &live_now,
        ) {
            Ok(k) => k,
            Err(e) => return self.stop(Phase::KeyRefused(e)),
        };
        let route = match &self.setup.route {
            LiveRouteConfig::Loopback { port } => {
                if let Some(why) =
                    loopback_refused(store.dir(), crate::sources::canonical_state_dir())
                {
                    return self.stop(Phase::ArmFailed(why));
                }
                LiveRoute::Loopback { port: *port }
            }
            LiveRouteConfig::Builders { blockrazor_token } => {
                let setup = match TlsSetup::web_pki_roots() {
                    Ok(s) => s,
                    Err(e) => return self.stop(Phase::ArmFailed(format!("TLS setup: {e:?}"))),
                };
                let blockrazor = match blockrazor_token.as_deref().map(token_from).transpose() {
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
        let failed = end.connect(self.setup.connect_timeout);
        if !failed.is_empty() {
            // The end and its key go; a later pass tries again.
            drop(end);
            self.last_attempt = Some(Instant::now());
            self.phase = Phase::ConnectFailed(
                failed
                    .iter()
                    .map(|(a, e)| format!("{}: {e}", a.name()))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
            return;
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
            }
            Err(e) => self.stop(Phase::ArmFailed(e.to_string())),
        }
    }

    /// The gate is closed: an armed path ends with the reason, an unarmed
    /// one shows it.
    fn close(&mut self, why: String) {
        self.probe = None;
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

    /// A verified full pending C1-scope launch first seen by `source`,
    /// when armed: only a source the grant names triggers a LIVE BUY.
    pub fn on_launch(
        &mut self,
        source: SourceId,
        hash: [u8; 32],
        raw: &[u8],
        ingress_ns: u64,
        owner: &StateOwner,
        clock: &dyn Clock,
    ) -> Option<ExecBuy<LiveHandoff>> {
        if !self
            .executor
            .as_ref()?
            .end()
            .armed()
            .grant()
            .sources
            .contains(&source)
        {
            self.source_refused += 1;
            return None;
        }
        self.poll_responses(clock);
        if self.buy_refused(self.one_buy()).is_some() {
            return None;
        }
        let next = self.free_nonce(owner);
        let reserve = Some(self.exit_reserve());
        let x = self.executor.as_mut()?;
        let buy = launch_to(x, hash, raw, ingress_ns, owner, next, reserve, clock);
        self.record_responses();
        buy
    }

    /// What of the probed balance is left: every transaction of the ledger
    /// not paid in the probe's block counts at its most.
    fn available(&self) -> Option<U256> {
        let funds = self.funds?;
        let to_come = self
            .book
            .txs
            .iter()
            .filter(|t| match t.state {
                FactKind::Released | FactKind::Unknown => true,
                FactKind::Included | FactKind::Reverted => t
                    .inclusion
                    .as_ref()
                    .and_then(|f| f.block)
                    .is_none_or(|(n, _, _)| n > funds.block),
                FactKind::Withdrawn
                | FactKind::Replaced
                | FactKind::Orphaned
                | FactKind::Responded => false,
            })
            .map(|t| t.released.max_wei.unwrap_or_default())
            .fold(U256::ZERO, |sum, wei| sum + wei);
        Some(funds.balance.saturating_sub(to_come))
    }

    /// Why the buyer's balance does not hold a BUY of at most `most`, its
    /// exit and the exits of what is open (BSC3-027).
    fn unfunded(&self, most: U256) -> Option<String> {
        let (approval, sell) = self.exit_gas();
        let need = most + approval + sell + self.exit_reserve();
        match self.available() {
            Some(left) if left >= need => None,
            Some(left) => Some(format!(
                "{left} wei left, a BUY with its exit and the open exits need {need}"
            )),
            None => Some("no balance read".into()),
        }
    }

    /// Why no new BUY of at most `most` goes now (counted): an open
    /// position's exit waits for the buyer's nonce first (BSC3-048), the
    /// owner's gate admits only exits (BSC3-071), or the balance does not
    /// hold it with the exits (BSC3-027).
    fn buy_refused(&mut self, most: U256) -> Option<Refusal> {
        let exit_out = self
            .book
            .unresolved()
            .any(|t| t.released.intent != LiveIntentKind::Buy);
        if !self.exits_wanted.is_empty() || exit_out {
            self.exit_first += 1;
            return Some(Refusal::ExitFirst);
        }
        if self.buys_closed.is_some() {
            self.buys_refused += 1;
            return Some(Refusal::Owner);
        }
        if self.unfunded(most).is_some() {
            self.unfunded_refused += 1;
            return Some(Refusal::Unfunded);
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
            self.provider_refused += 1;
            return Some(Refusal::Dispatch);
        }
        None
    }

    /// The providers' answers to what the armed end released, read without
    /// waiting and kept in memory (BSC3-051): before a decision, nothing
    /// written. [`Self::record_responses`] writes them.
    pub fn poll_responses(&mut self, clock: &dyn Clock) {
        if let Some(x) = self.executor.as_mut() {
            x.poll_responses(clock);
        }
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
        self.executor.as_ref()?;
        self.poll_responses(clock);
        if let crate::runtime_strategy::StrategyAction::Buy { input_wei, .. } = *action {
            let x = &self.exec.exec;
            let most = input_wei + U256::from(x.gas_price_wei) * U256::from(x.gas_limit);
            if let Some(refusal) = self.buy_refused(most) {
                return Some((Vec::new(), Some(refusal)));
            }
        }
        let next = self.free_nonce(owner);
        let reserve = Some(self.exit_reserve());
        let handed = strategy_to(
            self.executor.as_mut()?,
            action,
            owner,
            next,
            reserve,
            clock,
            now_unix_ms,
        );
        if let crate::runtime_strategy::StrategyAction::Exit { token, .. } = *action {
            self.want_exit(token);
        }
        self.record_responses();
        Some(handed)
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
            .filter(|t| !x.exited(t))
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
            d.set(l::GRANT_GENERATION, x.end().armed().grant_generation());
            if let Some(r) = x.end().last_refusal() {
                d.set(l::LAST_REFUSAL, r);
            }
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
            ("EXIT_FIRST", self.exit_first),
            ("BUYS_CLOSED", self.buys_refused),
            ("BUYS_UNFUNDED", self.unfunded_refused),
            ("BUYS_PROVIDER_REFUSED", self.provider_refused),
        ] {
            if n > 0 {
                d.push(l::REFUSALS, format!("{name}={n}").as_str());
            }
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
mod tests {
    use super::*;

    /// BSC3-026: the stand-in route runs only on a state that is not the
    /// account's owner state (also through another spelling of its path),
    /// and not when that state is unknown.
    #[test]
    fn loopback_never_runs_on_the_accounts_owner_state() {
        let base = std::env::temp_dir().join(format!("bsc-loopback-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let account = base.join("home/.darkbot/run/bsc-sources");
        let test = base.join("test/bsc-sources");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::create_dir_all(&test).unwrap();
        assert_eq!(loopback_refused(&test, Ok(account.clone())), None);
        let same = loopback_refused(&account, Ok(account.clone())).unwrap();
        assert!(same.starts_with("LOOPBACK_ON_ACCOUNT_STATE"), "{same}");
        let spelled = base.join("test/../home/.darkbot/run/bsc-sources");
        assert!(loopback_refused(&spelled, Ok(account.clone())).is_some());
        let unknown = loopback_refused(&test, Err("HOME differs".into())).unwrap();
        assert!(
            unknown.starts_with("LOOPBACK_STATE_UNVERIFIED"),
            "{unknown}"
        );
        std::fs::remove_dir_all(&base).unwrap();
    }
    use crate::chain_follower::{CanonicalEvent, CanonicalTx};
    use crate::live_end::tests::{club48_findings, key_file, stand_in};
    use crate::live_qualification::tests::{acquisition, fixture, venue_evidence};
    use crate::live_qualification::{LIVE_QUALIFICATION_PATH, Spec, build};
    use crate::paper_exec::FnClock;
    use crate::runtime::exec::tests::{HEAD, launch, limits, observation, owner, setup};
    use crate::sources::tests::{grant_of, live_ready_granting};
    use crate::sources::{
        Entity, LIVE_GRANT_MISSING, LiveAdapter, LiveGrant, Mode, VenueId, buy_usage,
    };
    use darkbot_bsc_record::Record;
    use darkbot_bsc_trading::venues::flap::deployment::FlapDeployment;
    use std::cell::Cell;
    use std::fs;

    const NOW: u64 = 1_793_059_200_000;

    fn fixed() -> u64 {
        NOW
    }

    fn funded(
        _: &str,
        _: &FollowerLimits,
        d: &FlapDeployment,
        _: Address,
    ) -> Result<VenueObservation, String> {
        let mut o = observation(d.portal_version, d.portal_implementation);
        o.caller_balance_wei = U256::from(10_000_000_000_000_000_000u128);
        Ok(o)
    }

    fn empty(
        _: &str,
        _: &FollowerLimits,
        d: &FlapDeployment,
        _: Address,
    ) -> Result<VenueObservation, String> {
        Ok(observation(d.portal_version, d.portal_implementation))
    }

    /// One BUY at the setup's gas (1.15e15 wei) fits, its exit (8e13) not.
    fn short(
        _: &str,
        _: &FollowerLimits,
        d: &FlapDeployment,
        _: Address,
    ) -> Result<VenueObservation, String> {
        let mut o = observation(d.portal_version, d.portal_implementation);
        o.caller_balance_wei = U256::from(1_200_000_000_000_000u128);
        Ok(o)
    }

    /// One C3-sized BUY with its exit fits, a Strategy C BUY of 1e16 with
    /// its exit (1.023e16) not.
    fn mid(
        _: &str,
        _: &FollowerLimits,
        d: &FlapDeployment,
        _: Address,
    ) -> Result<VenueObservation, String> {
        let mut o = observation(d.portal_version, d.portal_implementation);
        o.caller_balance_wei = U256::from(10_200_000_000_000_000u128);
        Ok(o)
    }

    /// One Strategy C BUY of 1e16 with its exit fits, a second one after it
    /// with both exits not.
    fn two(
        _: &str,
        _: &FollowerLimits,
        d: &FlapDeployment,
        _: Address,
    ) -> Result<VenueObservation, String> {
        let mut o = observation(d.portal_version, d.portal_implementation);
        o.caller_balance_wei = U256::from(20_400_000_000_000_000u128);
        Ok(o)
    }

    /// The exit of one open position (8e13 wei) fits, a BUY not.
    fn exits_only(
        _: &str,
        _: &FollowerLimits,
        d: &FlapDeployment,
        _: Address,
    ) -> Result<VenueObservation, String> {
        let mut o = observation(d.portal_version, d.portal_implementation);
        o.caller_balance_wei = U256::from(100_000_000_000_000u128);
        Ok(o)
    }

    fn record(x: &LiveRuntime) -> Record {
        darkbot_bsc_record::decode(&l::SCHEMA, &x.draft().encode().unwrap()).unwrap()
    }

    /// Ticks until the state is not `PROBING` (the probe thread answers).
    fn settle(x: &mut LiveRuntime, owner: &StateOwner) {
        let lim = limits();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            x.tick(Some(owner), Some(("https://probe.invalid", &lim)), 7, NOW);
            if record(x).text(l::STATE).unwrap() != "PROBING" || Instant::now() > deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Plan 28.10.: the LIVE path arms only under the owner's clear gate
    /// with its qualification on file, a funded buyer and C2's clean nonce;
    /// it takes only launches first seen by a granted source; C2's
    /// inclusion of its BUY resolves the lane; a revoked grant ends it and
    /// its key goes.
    #[test]
    fn the_live_path_arms_under_the_gate_and_ends_with_it() {
        let (state, store, now) = live_ready_granting("live-rt", |now| LiveGrant {
            max_input_wei: 10_000_000_000_000_000,
            max_spend_wei: 100_000_000_000_000_000,
            adapters: vec![LiveAdapter::Club48BuilderV1],
            ..grant_of(now + 3_600_000, LiveStrategies::C3)
        });
        assert_eq!(now, NOW);
        let root = fixture("live-rt", "PASS");
        let ev = || vec!["ev/measure.dbrec".to_string()];
        let spec = Spec {
            venues: vec![(VenueId::Flap, venue_evidence())],
            sources: vec![(SourceId::PublicNode, ev())],
            adapters: vec![(LiveAdapter::Club48BuilderV1, club48_findings(), ev())],
            acquisition: acquisition(),
            risk: Some(("operator".into(), 1, "synthetic scope".into(), ev())),
        };
        fs::create_dir_all(root.join("bsc/config")).unwrap();
        fs::write(
            root.join(LIVE_QUALIFICATION_PATH),
            build(&root, &spec, 1).unwrap(),
        )
        .unwrap();
        let d = std::env::temp_dir().join(format!("bsc-live-rt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let (key, buyer) = key_file(&d);
        let (port, requests) = stand_in();
        let live_setup = LiveSetup {
            key_path: key,
            buyer,
            route: LiveRouteConfig::Loopback { port },
            connect_timeout: Duration::from_secs(2),
        };
        let exec = setup(d.join("unused.key"), Duration::from_secs(3_600));
        let new = |prober: Prober| {
            LiveRuntime::with_prober(
                live_setup.clone(),
                exec.clone(),
                &root,
                state.clone(),
                d.join("fences"),
                None,
                prober,
            )
            .in_session("boot-1", fixed)
        };
        // Not watched by C2, then unfunded: nothing arms.
        let mut x = new(funded);
        let foreign = owner(&d.join("c2-other"), [7; 20]);
        settle(&mut x, &foreign);
        assert_eq!(record(&x).text(l::STATE).unwrap(), "NOT_WATCHED");
        let mut c2 = owner(&d, buyer);
        let mut poor = new(empty);
        settle(&mut poor, &c2);
        assert_eq!(record(&poor).text(l::STATE).unwrap(), "UNFUNDED");
        assert!(!poor.armed());
        // Funded: it waits for C2's clean nonce of the buyer, then arms.
        settle(&mut x, &c2);
        assert_eq!(record(&x).text(l::STATE).unwrap(), "WAITING_NONCE");
        c2.canonical(&CanonicalEvent::Block {
            number: HEAD,
            hash: [0xb1; 32],
            parent_hash: [0xb0; 32],
            timestamp_s: 1_793_000_000,
            timestamp_ms: 1_793_000_000_000,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs: Vec::new(),
        });
        c2.canonical(&CanonicalEvent::NonceFact {
            sender: buyer,
            next_nonce: 5,
            block_number: HEAD,
            block_hash: [0xb1; 32],
        });
        let _ = c2.take_evidence();
        settle(&mut x, &c2);
        let r = record(&x);
        assert_eq!(
            r.text(l::STATE).unwrap(),
            "ARMED",
            "{:?}",
            r.opt_text(l::REASON)
        );
        assert_eq!(r.opt_u64(l::NEXT_NONCE).unwrap(), Some(5));
        assert!(x.armed());
        // Only a granted source's verified full sighting triggers.
        let (hash, raw, launcher) = launch();
        c2.pending(hash, launcher, 3, 0, 10);
        let t = Cell::new(10u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        assert!(
            x.on_launch(SourceId::Drpc, hash, &raw, 10, &c2, &clock)
                .is_none()
        );
        let buy = x
            .on_launch(SourceId::PublicNode, hash, &raw, 10, &c2, &clock)
            .unwrap();
        let request = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            String::from_utf8(request)
                .unwrap()
                .contains("eth_sendBundle")
        );
        assert_eq!(buy_usage(&state, NOW).unwrap().used, 1);
        let r = record(&x);
        assert_eq!(r.u64(l::RELEASED).unwrap(), 1);
        assert!(
            r.texts(l::REFUSALS)
                .unwrap()
                .contains(&"SOURCE_NOT_GRANTED=1")
        );
        // BSC3-043: the released C3 BUY is the LIVE path's own latency
        // sample (ingress to handoff), apart from PAPER's.
        assert_eq!(r.u64(l::LATENCY_SAMPLES).unwrap(), 1);
        assert_eq!(
            r.opt_u64(l::LATENCY_P50_NS).unwrap(),
            r.opt_u64(l::LATENCY_MAX_NS).unwrap()
        );
        assert!(r.opt_u64(l::LATENCY_P50_NS).unwrap().is_some());
        // C2 includes the BUY at the buyer's nonce: the lane resolves it.
        c2.canonical(&CanonicalEvent::Block {
            number: HEAD + 1,
            hash: [0xb2; 32],
            parent_hash: [0xb1; 32],
            timestamp_s: 1_793_000_001,
            timestamp_ms: 1_793_000_001_000,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs: vec![CanonicalTx {
                hash: buy.handoff.tx_hash,
                index: 0,
                sender: buyer,
                nonce: 5,
                succeeded: true,
                cost: None,
            }],
        });
        let evidence = c2.take_evidence();
        assert_eq!(evidence.len(), 1);
        assert_eq!(
            x.apply_evidence(&evidence),
            vec![Some(IntentState::Included)]
        );
        assert_eq!(record(&x).u64(l::INCLUDED).unwrap(), 1);
        // BSC3-042: a second qualified launch after the C3 nonce was
        // included: the arm's C3 lanes all stand on that nonce. Under the
        // default cap it takes nothing: no owner slot, nothing sent.
        let (hash_b, raw_b, launcher_b) = crate::runtime::exec::tests::launch_n(1);
        c2.pending(hash_b, launcher_b, 4, 0, 20);
        let used = buy_usage(&state, NOW).unwrap().used;
        let second = x.on_launch(SourceId::PublicNode, hash_b, &raw_b, 20, &c2, &clock);
        assert!(second.is_none(), "{:?}", reason(&x));
        assert_eq!(buy_usage(&state, NOW).unwrap().used, used);
        assert!(requests.recv_timeout(Duration::from_millis(300)).is_err());
        assert!(
            record(&x)
                .texts(l::REFUSALS)
                .unwrap()
                .contains(&"SESSION_CAP=1")
        );
        // Revoked: the path ends at the next pass, the key goes.
        let owner_store = Store::open_in_session(&state, 8, Ok("boot-1".into())).unwrap();
        let g = owner_store.current().unwrap().generation;
        assert!(
            owner_store
                .request("revoke", Entity::LiveGrant, Mode::Disabled, g, NOW)
                .unwrap()
                .applied()
        );
        settle(&mut x, &c2);
        let r = record(&x);
        assert_eq!(
            (r.text(l::STATE).unwrap(), r.opt_text(l::REASON).unwrap()),
            ("ENDED", Some(LIVE_GRANT_MISSING))
        );
        assert!(!x.armed());
        drop(store);
        fs::remove_dir_all(d).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(state).unwrap();
    }

    /// BSC3-042 under a synthetic session cap ten times the default: after
    /// C3 BUY A's inclusion at the arm's nonce, a second qualified C3
    /// launch B passes the cap and is refused at the buyer's nonce (the
    /// arm's C3 lanes all stand on A's): no owner admission (no slot, no
    /// spend entry), no bytes on the wire.
    #[test]
    fn a_second_c3_launch_after_inclusion_takes_nothing_under_a_raised_cap() {
        let (state, store, now) = live_ready_granting("live-rt-cap", |now| LiveGrant {
            max_input_wei: 10_000_000_000_000_000,
            max_spend_wei: 100_000_000_000_000_000,
            adapters: vec![LiveAdapter::Club48BuilderV1],
            ..grant_of(now + 3_600_000, LiveStrategies::C3)
        });
        assert_eq!(now, NOW);
        let root = fixture("live-rt-cap", "PASS");
        let ev = || vec!["ev/measure.dbrec".to_string()];
        let spec = Spec {
            venues: vec![(VenueId::Flap, venue_evidence())],
            sources: vec![(SourceId::PublicNode, ev())],
            adapters: vec![(LiveAdapter::Club48BuilderV1, club48_findings(), ev())],
            acquisition: acquisition(),
            risk: Some(("operator".into(), 1, "synthetic scope".into(), ev())),
        };
        fs::create_dir_all(root.join("bsc/config")).unwrap();
        fs::write(
            root.join(LIVE_QUALIFICATION_PATH),
            build(&root, &spec, 1).unwrap(),
        )
        .unwrap();
        let d = std::env::temp_dir().join(format!("bsc-live-rt-cap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let (key, buyer) = key_file(&d);
        let (port, requests) = stand_in();
        let live_setup = LiveSetup {
            key_path: key,
            buyer,
            route: LiveRouteConfig::Loopback { port },
            connect_timeout: Duration::from_secs(2),
        };
        let mut exec = setup(d.join("unused.key"), Duration::from_secs(3_600));
        exec.exec.session_cap_wei *= 10;
        let mut x = LiveRuntime::with_prober(
            live_setup,
            exec,
            &root,
            state.clone(),
            d.join("fences"),
            None,
            funded,
        )
        .in_session("boot-1", fixed);
        let mut c2 = owner(&d, buyer);
        c2.canonical(&CanonicalEvent::Block {
            number: HEAD,
            hash: [0xb1; 32],
            parent_hash: [0xb0; 32],
            timestamp_s: 1_793_000_000,
            timestamp_ms: 1_793_000_000_000,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs: Vec::new(),
        });
        c2.canonical(&CanonicalEvent::NonceFact {
            sender: buyer,
            next_nonce: 5,
            block_number: HEAD,
            block_hash: [0xb1; 32],
        });
        let _ = c2.take_evidence();
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let t = Cell::new(10u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let (hash, raw, launcher) = launch();
        c2.pending(hash, launcher, 3, 0, 10);
        let buy = x
            .on_launch(SourceId::PublicNode, hash, &raw, 10, &c2, &clock)
            .unwrap();
        assert!(requests.recv_timeout(Duration::from_secs(2)).is_ok());
        // A is included at the arm's nonce: C2 and the LIVE path see it
        // (its lanes, its ledger); the buyer's next nonce is C2's n+1, past
        // the arm's C3 lanes.
        let included = CanonicalEvent::Block {
            number: HEAD + 1,
            hash: [0xb2; 32],
            parent_hash: [0xb1; 32],
            timestamp_s: 1_793_000_001,
            timestamp_ms: 1_793_000_001_000,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs: vec![CanonicalTx {
                hash: buy.handoff.tx_hash,
                index: 0,
                sender: buyer,
                nonce: 5,
                succeeded: true,
                cost: None,
            }],
        };
        c2.canonical(&included);
        let evidence = c2.take_evidence();
        assert_eq!(
            x.apply_evidence(&evidence),
            vec![Some(IntentState::Included)]
        );
        let _ = x.on_canonical(&included);
        let _ = x.on_canonical(&CanonicalEvent::BlockComplete {
            number: HEAD + 1,
            hash: [0xb2; 32],
        });
        assert_eq!(x.free_nonce(&c2), Some(6));
        let (used, spent) = (
            buy_usage(&state, NOW).unwrap().used,
            crate::sources::admitted_intents(&state).unwrap(),
        );
        assert_eq!((used, spent.len()), (1, 1));
        let (hash_b, raw_b, launcher_b) = crate::runtime::exec::tests::launch_n(1);
        c2.pending(hash_b, launcher_b, 4, 0, 20);
        let second = x.on_launch(SourceId::PublicNode, hash_b, &raw_b, 20, &c2, &clock);
        assert!(second.is_none(), "{:?}", reason(&x));
        assert_eq!(buy_usage(&state, NOW).unwrap().used, used);
        assert_eq!(crate::sources::admitted_intents(&state).unwrap(), spent);
        assert!(requests.recv_timeout(Duration::from_millis(300)).is_err());
        let r = record(&x);
        let refusals = r.texts(l::REFUSALS).unwrap();
        assert!(refusals.contains(&"SENDER_NONCE=1"), "{refusals:?}");
        assert!(!refusals.iter().any(|r| r.starts_with("SESSION_CAP")));
        assert_eq!(record(&x).u64(l::RELEASED).unwrap(), 1);
        drop(store);
        fs::remove_dir_all(d).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(state).unwrap();
    }

    /// Plan 28.10. package 6: a Strategy C BUY released over loopback, its
    /// receipt (gas, the Portal's trade) opens the position in the ledger;
    /// a reorg takes it back and a new inclusion opens it again; the exit's
    /// approval and SELL, signed by the buyer key and admitted by the
    /// owner, close it; PnL = proceeds - input - every gas. A restart reads
    /// the same book.
    #[test]
    fn a_real_position_opens_closes_and_is_accounted() {
        use crate::chain_follower::{CanonicalTrade, TxCost};
        use crate::live_ledger::PositionState;
        use crate::paper_exec::{IntentKind, StrategyCurve};
        use crate::runtime_strategy::StrategyAction;
        use darkbot_bsc_trading::economics::flap::native_curve20::{
            HoldingCap, NativeBuyInputs, PricedToken, profile_terms, quote_native_buy,
            quote_native_sell,
        };
        let (state, _store, _) = live_ready_granting("live-pos", |now| LiveGrant {
            max_input_wei: 10_000_000_000_000_000,
            max_spend_wei: 100_000_000_000_000_000,
            adapters: vec![LiveAdapter::Club48BuilderV1],
            ..grant_of(now + 3_600_000, LiveStrategies::StrategyC)
        });
        let root = fixture("live-pos", "PASS");
        let ev = || vec!["ev/measure.dbrec".to_string()];
        let spec = Spec {
            venues: vec![(VenueId::Flap, venue_evidence())],
            sources: vec![(SourceId::PublicNode, ev())],
            adapters: vec![(LiveAdapter::Club48BuilderV1, club48_findings(), ev())],
            acquisition: acquisition(),
            risk: Some(("operator".into(), 1, "synthetic scope".into(), ev())),
        };
        fs::create_dir_all(root.join("bsc/config")).unwrap();
        fs::write(
            root.join(LIVE_QUALIFICATION_PATH),
            build(&root, &spec, 1).unwrap(),
        )
        .unwrap();
        let d = std::env::temp_dir().join(format!("bsc-live-pos-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let (key, buyer) = key_file(&d);
        let (port, requests) = stand_in();
        let mut x = LiveRuntime::with_prober(
            LiveSetup {
                key_path: key,
                buyer,
                route: LiveRouteConfig::Loopback { port },
                connect_timeout: Duration::from_secs(2),
            },
            setup(d.join("unused.key"), Duration::from_secs(3_600)),
            &root,
            state.clone(),
            d.join("fences"),
            None,
            funded,
        )
        .in_session("boot-1", fixed);
        let mut c2 = owner(&d, buyer);
        let block = |n: u64, h: u8, parent: u8, txs| CanonicalEvent::Block {
            number: n,
            hash: [h; 32],
            parent_hash: [parent; 32],
            timestamp_s: 1_793_000_000 + (n - HEAD),
            timestamp_ms: 1_793_000_000_000 + (n - HEAD) * 1_000,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs,
        };
        c2.canonical(&block(HEAD, 0xb1, 0xb0, Vec::new()));
        c2.canonical(&CanonicalEvent::NonceFact {
            sender: buyer,
            next_nonce: 0,
            block_number: HEAD,
            block_hash: [0xb1; 32],
        });
        let _ = c2.take_evidence();
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", record(&x).opt_text(l::REASON));
        let reconnect = |x: &mut LiveRuntime| {
            let failed = x
                .executor
                .as_mut()
                .unwrap()
                .end_mut()
                .connect(Duration::from_secs(2));
            assert!(failed.is_empty());
        };
        let t = Cell::new(5_000_000_000u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let now_ms = 1_793_000_000_500;
        let token = [0xAB; 20];
        let input = U256::from(10_000_000_000_000_000u64);
        let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
        let fresh = quote_native_buy(&NativeBuyInputs {
            token: terms,
            reserve_before: U256::ZERO,
            circulating_before: U256::ZERO,
            input_amount: U256::from(1_000_000_000_000_000_000u128),
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: HoldingCap::None,
        })
        .unwrap();
        let curve = StrategyCurve {
            terms,
            reserve: fresh.reserve_after,
            circulating: fresh.supply_after,
        };
        let buy = StrategyAction::Buy {
            token,
            curve,
            input_wei: input,
            decided_ns: 0,
        };
        let (intents, refused) = x.on_strategy(&buy, &c2, &clock, now_ms).unwrap();
        assert_eq!((intents.len(), refused), (1, None));
        let buy_tx = intents[0].handoff.tx_hash;
        requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(x.book().unresolved().count(), 1);
        // Its receipt: included with its gas and the Portal's trade.
        let bought = quote_native_buy(&NativeBuyInputs {
            token: terms,
            reserve_before: curve.reserve,
            circulating_before: curve.circulating,
            input_amount: input,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: HoldingCap::None,
        })
        .unwrap();
        let tokens = bought.output_amount;
        let tx = |hash, nonce, gas: u64| CanonicalTx {
            hash,
            index: 0,
            sender: buyer,
            nonce,
            succeeded: true,
            cost: Some(TxCost {
                gas_used: gas,
                effective_gas_price: 50_000_000,
            }),
        };
        let trade = |hash, n: u64, h: u8, sell, amount: U256, eth: U256| {
            CanonicalEvent::FlapTrade(CanonicalTrade {
                block_number: n,
                block_hash: [h; 32],
                tx_index: 0,
                tx_hash: hash,
                log_index: 0,
                origin: buyer,
                sell,
                token,
                trader: buyer,
                amount: amount.to_be_bytes(),
                eth: eth.to_be_bytes(),
                fee: [0; 32],
                post_price: [0; 32],
            })
        };
        // As the runtime applies a canonical event: C2 first, its evidence
        // to the LIVE path, then the follower's facts.
        let apply = |x: &mut LiveRuntime, c2: &mut StateOwner, events: Vec<CanonicalEvent>| {
            let mut changes = Vec::new();
            for e in &events {
                c2.canonical(e);
                let evidence = c2.take_evidence();
                let _ = x.apply_evidence(&evidence);
                changes.extend(x.on_canonical(e));
            }
            changes
        };
        let complete = |n: u64, h: u8| CanonicalEvent::BlockComplete {
            number: n,
            hash: [h; 32],
        };
        let opened = PositionChange::Opened {
            token,
            tokens,
            spent_wei: input + U256::from(200_000u64 * 50_000_000),
            filled_ms: 1_793_000_001_000,
        };
        let changes = apply(
            &mut x,
            &mut c2,
            vec![
                block(HEAD + 1, 0xb2, 0xb1, vec![tx(buy_tx, 0, 200_000)]),
                trade(buy_tx, HEAD + 1, 0xb2, false, tokens, input),
                complete(HEAD + 1, 0xb2),
            ],
        );
        assert_eq!(changes, vec![opened]);
        // A reorg takes the block: the position is not open; included
        // again in the new chain's block, it is.
        let changes = apply(
            &mut x,
            &mut c2,
            vec![CanonicalEvent::Reorg {
                orphaned: vec![(HEAD + 1, [0xb2; 32])],
                new_head: (HEAD + 1, [0xc2; 32]),
            }],
        );
        assert_eq!(changes, vec![PositionChange::Ended { token }]);
        let changes = apply(
            &mut x,
            &mut c2,
            vec![
                block(HEAD + 1, 0xc2, 0xb1, vec![tx(buy_tx, 0, 200_000)]),
                trade(buy_tx, HEAD + 1, 0xc2, false, tokens, input),
                complete(HEAD + 1, 0xc2),
            ],
        );
        assert_eq!(changes, vec![opened]);
        // The exit: approval, then (on a ready connection) the SELL of the
        // real tokens, both signed by the buyer key.
        let held = StrategyCurve {
            reserve: bought.reserve_after,
            circulating: bought.supply_after,
            ..curve
        };
        let exit = StrategyAction::Exit {
            token,
            tokens,
            curve: held,
            decided_ns: 0,
        };
        reconnect(&mut x);
        let (out, refused) = x.on_strategy(&exit, &c2, &clock, now_ms).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, IntentKind::Approve);
        // BSC3-055: the SELL waits for the approval to be canonical.
        assert_eq!(refused, None);
        let approve_tx = out[0].handoff.tx_hash;
        requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
        let changes = apply(
            &mut x,
            &mut c2,
            vec![
                block(HEAD + 2, 0xc3, 0xc2, vec![tx(approve_tx, 1, 50_000)]),
                complete(HEAD + 2, 0xc3),
            ],
        );
        assert_eq!(changes, Vec::new());
        // The approval is canonical: the exit continues with the SELL.
        assert_eq!(x.exits_to_continue(&c2), vec![token]);
        reconnect(&mut x);
        let (out, refused) = x.on_strategy(&exit, &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None));
        assert_eq!(out[0].kind, IntentKind::Sell);
        let sell_tx = out[0].handoff.tx_hash;
        requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
        let positions = &x.book().positions;
        assert_eq!(positions[0].state, PositionState::Exiting);
        let proceeds = quote_native_sell(
            &darkbot_bsc_trading::economics::flap::native_curve20::NativeSellInputs {
                token: terms,
                reserve_before: held.reserve,
                circulating_before: held.circulating,
                input_amount: tokens,
                protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_sell_fee_bps,
                tax_on_bonding_curve_enabled: true,
                caller_fee_exempt: false,
            },
        )
        .unwrap()
        .output_amount;
        let changes = apply(
            &mut x,
            &mut c2,
            vec![
                block(HEAD + 3, 0xc4, 0xc3, vec![tx(sell_tx, 2, 150_000)]),
                trade(sell_tx, HEAD + 3, 0xc4, true, tokens, proceeds),
                complete(HEAD + 3, 0xc4),
            ],
        );
        assert_eq!(changes, vec![PositionChange::Ended { token }]);
        let gas = U256::from((200_000u64 + 50_000 + 150_000) * 50_000_000);
        let p = &x.book().positions[0];
        assert_eq!(p.state, PositionState::Closed);
        assert_eq!(p.gas_wei, gas);
        let pnl = i128::try_from(proceeds).unwrap()
            - i128::try_from(input).unwrap()
            - i128::try_from(gas).unwrap();
        assert_eq!(p.pnl_wei, Some(pnl));
        let r = record(&x);
        assert_eq!(r.u64(l::POSITIONS_CLOSED).unwrap(), 1);
        assert_eq!(r.text(l::REALIZED_PNL_WEI).unwrap(), pnl.to_string());
        // After a restart the ledger gives the same book.
        let restarted =
            crate::live_ledger::fold(&crate::live_ledger::Ledger::open(&state).facts().unwrap());
        assert_eq!(&restarted, x.book());
        x.shutdown();
        fs::remove_dir_all(d).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(state).unwrap();
    }

    /// A C3 LIVE scope on the loopback stand-in: the owner's state, the
    /// qualification's repository, the run directory, the setup and what
    /// the stand-in receives.
    struct Rig {
        state: PathBuf,
        root: PathBuf,
        d: PathBuf,
        buyer: [u8; 20],
        strategies: LiveStrategies,
        live: LiveSetup,
        exec: ExecSetup,
        requests: std::sync::mpsc::Receiver<Vec<u8>>,
    }

    impl Rig {
        fn new(tag: &str, strategies: LiveStrategies) -> Rig {
            let (state, _store, now) = live_ready_granting(tag, |now| LiveGrant {
                max_input_wei: 10_000_000_000_000_000,
                max_spend_wei: 100_000_000_000_000_000,
                adapters: vec![LiveAdapter::Club48BuilderV1],
                ..grant_of(now + 3_600_000, strategies)
            });
            assert_eq!(now, NOW);
            let root = fixture(tag, "PASS");
            let ev = || vec!["ev/measure.dbrec".to_string()];
            let spec = Spec {
                venues: vec![(VenueId::Flap, venue_evidence())],
                sources: vec![(SourceId::PublicNode, ev())],
                adapters: vec![(LiveAdapter::Club48BuilderV1, club48_findings(), ev())],
                acquisition: acquisition(),
                risk: Some(("operator".into(), 1, "synthetic scope".into(), ev())),
            };
            fs::create_dir_all(root.join("bsc/config")).unwrap();
            fs::write(
                root.join(LIVE_QUALIFICATION_PATH),
                build(&root, &spec, 1).unwrap(),
            )
            .unwrap();
            let d = std::env::temp_dir().join(format!("bsc-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&d);
            let (key, buyer) = key_file(&d);
            let (port, requests) = stand_in();
            Rig {
                exec: setup(d.join("unused.key"), Duration::from_secs(3_600)),
                live: LiveSetup {
                    key_path: key,
                    buyer,
                    route: LiveRouteConfig::Loopback { port },
                    connect_timeout: Duration::from_secs(2),
                },
                state,
                root,
                d,
                buyer,
                strategies,
                requests,
            }
        }

        /// [`Rig::new`] whose builder stand-in answers with `answers`.
        fn answering(
            tag: &str,
            strategies: LiveStrategies,
            answers: Vec<crate::live_end::tests::Answer>,
        ) -> Rig {
            let mut r = Rig::new(tag, strategies);
            let (port, requests) = crate::live_end::tests::answering(answers);
            r.live.route = LiveRouteConfig::Loopback { port };
            r.requests = requests;
            r
        }

        /// The LIVE path of one process in the boot session `boot`.
        fn runtime(&self, boot: &str) -> LiveRuntime {
            self.probed(boot, funded)
        }

        /// [`Rig::runtime`] whose probe sees what `prober` answers.
        fn probed(&self, boot: &str, prober: Prober) -> LiveRuntime {
            LiveRuntime::with_prober(
                self.live.clone(),
                self.exec.clone(),
                &self.root,
                self.state.clone(),
                self.d.join("fences"),
                None,
                prober,
            )
            .in_session(boot, fixed)
        }

        /// The operator authorizes LIVE again in `boot` (after a restart
        /// reset the switches or a revoke took the grant): every switch
        /// ENABLED and a new grant.
        fn authorize(&self, boot: &str, request: &str) {
            let s = Store::open_in_session(&self.state, 8, Ok(boot.into())).unwrap();
            s.restart(NOW).unwrap();
            for (i, id) in crate::sources::SwitchId::ALL.into_iter().enumerate() {
                let g = s.current().unwrap().generation;
                let r = s
                    .request(
                        &format!("{request}-{i}"),
                        Entity::Switch(id),
                        Mode::Enabled,
                        g,
                        NOW,
                    )
                    .unwrap();
                assert!(r.applied(), "{}", r.line());
            }
            let grant = LiveGrant {
                max_input_wei: 10_000_000_000_000_000,
                max_spend_wei: 100_000_000_000_000_000,
                adapters: vec![LiveAdapter::Club48BuilderV1],
                ..grant_of(NOW + 3_600_000, self.strategies)
            };
            let g = s.current().unwrap().generation;
            let r = s
                .grant_live(request, grant.clone(), grant.scope_sha256(), g, NOW)
                .unwrap();
            assert!(r.applied(), "{}", r.line());
        }

        fn fences(&self, suffix: &str) -> Vec<String> {
            let mut out: Vec<String> = fs::read_dir(self.d.join("fences"))
                .map(|e| {
                    e.filter_map(|e| e.ok())
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .filter(|n| n.ends_with(suffix))
                        .collect()
                })
                .unwrap_or_default();
            out.sort();
            out
        }

        fn clean(self) {
            fs::remove_dir_all(self.d).unwrap();
            fs::remove_dir_all(self.root).unwrap();
            fs::remove_dir_all(self.state).unwrap();
        }
    }

    /// C2 at `HEAD` with the buyer's next nonce `next`.
    fn c2_at(dir: &Path, buyer: [u8; 20], next: u64) -> StateOwner {
        let mut c2 = owner(dir, buyer);
        c2.canonical(&CanonicalEvent::Block {
            number: HEAD,
            hash: [0xb1; 32],
            parent_hash: [0xb0; 32],
            timestamp_s: 1_793_000_000,
            timestamp_ms: 1_793_000_000_000,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs: Vec::new(),
        });
        c2.canonical(&CanonicalEvent::NonceFact {
            sender: buyer,
            next_nonce: next,
            block_number: HEAD,
            block_hash: [0xb1; 32],
        });
        let _ = c2.take_evidence();
        c2
    }

    /// Block `n` (hash `[h; 32]`, parent `[parent; 32]`) with `txs`.
    fn block_of(n: u64, h: u8, parent: u8, txs: Vec<CanonicalTx>) -> CanonicalEvent {
        CanonicalEvent::Block {
            number: n,
            hash: [h; 32],
            parent_hash: [parent; 32],
            timestamp_s: 1_793_000_000 + (n - HEAD),
            timestamp_ms: 1_793_000_000_000 + (n - HEAD) * 1_000,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs,
        }
    }

    fn complete_of(n: u64, h: u8) -> CanonicalEvent {
        CanonicalEvent::BlockComplete {
            number: n,
            hash: [h; 32],
        }
    }

    /// `events` as the runtime applies them: C2 first, its evidence to the
    /// LIVE path, then the follower's facts.
    fn feed(
        x: &mut LiveRuntime,
        c2: &mut StateOwner,
        events: Vec<CanonicalEvent>,
    ) -> Vec<PositionChange> {
        let mut changes = Vec::new();
        for event in &events {
            c2.canonical(event);
            let evidence = c2.take_evidence();
            let _ = x.apply_evidence(&evidence);
            changes.extend(x.on_canonical(event));
        }
        changes
    }

    /// Block `HEAD + 1` with `txs`, as the runtime applies it.
    fn include(
        x: &mut LiveRuntime,
        c2: &mut StateOwner,
        txs: Vec<CanonicalTx>,
    ) -> Vec<PositionChange> {
        feed(
            x,
            c2,
            vec![
                block_of(HEAD + 1, 0xb2, 0xb1, txs),
                complete_of(HEAD + 1, 0xb2),
            ],
        )
    }

    fn reason(x: &LiveRuntime) -> (String, Option<String>) {
        let r = record(x);
        (
            r.text(l::STATE).unwrap().to_string(),
            r.opt_text(l::REASON).unwrap().map(str::to_string),
        )
    }

    /// BSC3-024/041: the process dies right after its BUY was handed to the
    /// builders. The restart knows the BUY (the owner's admission wrote it
    /// with its token and nonce before any byte could leave), refuses a new
    /// arm while the BUY's nonce may still be taken by it, opens the
    /// position once the chain includes it, and only then arms again, at
    /// the next nonce. Nothing leaves in between.
    #[test]
    fn a_release_lost_in_a_crash_is_recovered_and_blocks_the_next_arm() {
        let r = Rig::new("live-crash", LiveStrategies::C3);
        let mut x = r.runtime("boot-1");
        let c2 = c2_at(&r.d, r.buyer, 5);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let (hash, raw, launcher) = launch();
        let mut c2 = c2;
        c2.pending(hash, launcher, 3, 0, 10);
        let t = Cell::new(10u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        // The BUY leaves; the process is killed before anything else runs.
        let next = x.free_nonce(&c2);
        let reserve = Some(x.exit_reserve());
        let buy = launch_to(
            x.executor.as_mut().unwrap(),
            hash,
            &raw,
            10,
            &c2,
            next,
            reserve,
            &clock,
        )
        .unwrap();
        r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
        std::mem::forget(x);
        // The kill came between the dispatch and the outcome's fact: only
        // the owner's admission entry, durable before any byte left, holds
        // the BUY.
        for e in fs::read_dir(r.state.join("live-ledger")).unwrap() {
            fs::remove_file(e.unwrap().path()).unwrap();
        }
        // The restart: the BUY is a fact again, UNKNOWN and unresolved.
        r.authorize("boot-2", "after-crash");
        let mut x = r.runtime("boot-2");
        let unresolved: Vec<_> = x.book().unresolved().map(|t| t.released.tx_hash).collect();
        assert_eq!(unresolved, vec![buy.handoff.tx_hash]);
        let tx = &x.book().tx(&buy.handoff.tx_hash).unwrap().released;
        assert_eq!(
            (tx.kind, tx.token, tx.nonce),
            (crate::live_ledger::FactKind::Unknown, buy.token, 5)
        );
        // C2 restarted, the nonce still 5: no new arm while the BUY may
        // take it.
        let mut c2 = c2_at(&r.d.join("c2-restarted"), r.buyer, 5);
        settle(&mut x, &c2);
        assert!(!x.armed());
        let (state, why) = reason(&x);
        assert_eq!(state, "BLOCKED");
        assert!(
            why.as_deref()
                .unwrap()
                .starts_with(crate::sources::LIVE_RELEASE_UNRESOLVED),
            "{why:?}"
        );
        assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
        // The chain includes it: the position opens; the next arm takes
        // the next nonce.
        let changes = include(
            &mut x,
            &mut c2,
            vec![CanonicalTx {
                hash: buy.handoff.tx_hash,
                index: 0,
                sender: r.buyer,
                nonce: 5,
                succeeded: true,
                cost: None,
            }],
        );
        assert!(
            matches!(changes.as_slice(), [PositionChange::Opened { token, .. }] if *token == buy.token),
            "{changes:?}"
        );
        assert_eq!(x.book().unresolved().count(), 0);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        assert_eq!(record(&x).opt_u64(l::NEXT_NONCE).unwrap(), Some(6));
        // The crashed arm's fence is settled, not deleted.
        assert_eq!(r.fences(".fence").len(), 1);
        assert_eq!(r.fences(".fence.settled"), vec!["live-7-0.fence.settled"]);
        x.shutdown();
        r.clean();
    }

    /// BSC3-041: the gate closes while a released BUY is unresolved. The
    /// arm's fence stays; a new grant does not arm while the BUY may take
    /// its nonce; another transaction on that nonce (C2's inclusion
    /// evidence, applied while nothing is armed) resolves it as replaced,
    /// and then the path arms again.
    #[test]
    fn a_disarm_with_an_unresolved_release_keeps_its_fence_and_blocks_the_next_arm() {
        let r = Rig::new("live-unresolved", LiveStrategies::C3);
        let mut x = r.runtime("boot-1");
        let mut c2 = c2_at(&r.d, r.buyer, 5);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let (hash, raw, launcher) = launch();
        c2.pending(hash, launcher, 3, 0, 10);
        let t = Cell::new(10u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let buy = x
            .on_launch(SourceId::PublicNode, hash, &raw, 10, &c2, &clock)
            .unwrap();
        r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
        // Revoked with the BUY unresolved: the arm ends, its fence stays.
        let s = Store::open_in_session(&r.state, 8, Ok("boot-1".into())).unwrap();
        let g = s.current().unwrap().generation;
        assert!(
            s.request("revoke", Entity::LiveGrant, Mode::Disabled, g, NOW)
                .unwrap()
                .applied()
        );
        settle(&mut x, &c2);
        assert!(!x.armed());
        assert_eq!(r.fences(".fence"), vec!["live-7-0.fence"]);
        // The next pass with the gate closed.
        settle(&mut x, &c2);
        assert_eq!(
            reason(&x),
            ("BLOCKED".to_string(), Some(LIVE_GRANT_MISSING.to_string()))
        );
        // A new grant: still no arm.
        r.authorize("boot-1", "regrant");
        settle(&mut x, &c2);
        assert!(!x.armed());
        let (state, why) = reason(&x);
        assert_eq!(state, "BLOCKED");
        assert!(
            why.as_deref()
                .unwrap()
                .starts_with(crate::sources::LIVE_RELEASE_UNRESOLVED),
            "{why:?}"
        );
        // Another transaction of the buyer takes nonce 5: the BUY never
        // happened; nothing opens.
        let changes = include(
            &mut x,
            &mut c2,
            vec![CanonicalTx {
                hash: [0xee; 32],
                index: 0,
                sender: r.buyer,
                nonce: 5,
                succeeded: true,
                cost: None,
            }],
        );
        assert_eq!(changes, Vec::new());
        let tx = x.book().tx(&buy.handoff.tx_hash).unwrap();
        assert_eq!(tx.state, crate::live_ledger::FactKind::Replaced);
        // That block leaves the chain: nonce 5 is open again, and so is
        // the BUY; still no arm.
        let changes = feed(
            &mut x,
            &mut c2,
            vec![CanonicalEvent::Reorg {
                orphaned: vec![(HEAD + 1, [0xb2; 32])],
                new_head: (HEAD + 1, [0xc2; 32]),
            }],
        );
        assert_eq!(changes, Vec::new());
        let tx = x.book().tx(&buy.handoff.tx_hash).unwrap();
        assert_eq!(tx.state, crate::live_ledger::FactKind::Released);
        settle(&mut x, &c2);
        assert!(!x.armed());
        assert!(
            reason(&x)
                .1
                .unwrap()
                .starts_with(crate::sources::LIVE_RELEASE_UNRESOLVED)
        );
        // The new chain's block takes nonce 5 by the other one again.
        let other = CanonicalTx {
            hash: [0xee; 32],
            index: 0,
            sender: r.buyer,
            nonce: 5,
            succeeded: true,
            cost: None,
        };
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 1, 0xc2, 0xb1, vec![other]),
                CanonicalEvent::NonceFact {
                    sender: r.buyer,
                    next_nonce: 6,
                    block_number: HEAD + 1,
                    block_hash: [0xc2; 32],
                },
                complete_of(HEAD + 1, 0xc2),
            ],
        );
        let tx = x.book().tx(&buy.handoff.tx_hash).unwrap();
        assert_eq!(tx.state, crate::live_ledger::FactKind::Replaced);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        assert_eq!(record(&x).opt_u64(l::NEXT_NONCE).unwrap(), Some(6));
        assert_eq!(r.fences(".fence.settled"), vec!["live-7-0.fence.settled"]);
        x.shutdown();
        r.clean();
    }

    /// BSC3-055 (review HIGH): an exit refused for the sender's nonce order
    /// is not lost. Position A is open; B's BUY is out at nonce 1 and not
    /// canonical; A's exit would take nonce 2: nothing goes, the exit stays
    /// wanted. Once B is canonical it continues: A's approval, and once
    /// that is canonical, its SELL.
    #[test]
    fn an_exit_refused_for_the_nonce_order_continues_once_it_may() {
        use crate::chain_follower::{CanonicalTrade, TxCost};
        use crate::paper_exec::{IntentKind, Refusal, StrategyCurve};
        use crate::runtime_strategy::StrategyAction;
        use darkbot_bsc_trading::economics::flap::native_curve20::{
            HoldingCap, NativeBuyInputs, PricedToken, profile_terms, quote_native_buy,
        };
        let r = Rig::new("live-exit-order", LiveStrategies::StrategyC);
        let mut x = r.runtime("boot-1");
        let mut c2 = c2_at(&r.d, r.buyer, 0);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let t = Cell::new(5_000_000_000u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let now_ms = 1_793_000_000_500;
        let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
        let input = U256::from(10_000_000_000_000_000u64);
        let quote = |reserve, circulating, amount| {
            quote_native_buy(&NativeBuyInputs {
                token: terms,
                reserve_before: reserve,
                circulating_before: circulating,
                input_amount: amount,
                protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
                tax_on_bonding_curve_enabled: true,
                caller_fee_exempt: false,
                holding_cap: HoldingCap::None,
            })
            .unwrap()
        };
        let fresh = quote(
            U256::ZERO,
            U256::ZERO,
            U256::from(1_000_000_000_000_000_000u128),
        );
        let curve = StrategyCurve {
            terms,
            reserve: fresh.reserve_after,
            circulating: fresh.supply_after,
        };
        let bought = quote(curve.reserve, curve.circulating, input);
        let tokens = bought.output_amount;
        let held = StrategyCurve {
            reserve: bought.reserve_after,
            circulating: bought.supply_after,
            ..curve
        };
        let buy = |token| StrategyAction::Buy {
            token,
            curve,
            input_wei: input,
            decided_ns: 0,
        };
        let exit = |token| StrategyAction::Exit {
            token,
            tokens,
            curve: held,
            decided_ns: 0,
        };
        let buyer = r.buyer;
        let receipt = |hash, nonce| CanonicalTx {
            hash,
            index: 0,
            sender: buyer,
            nonce,
            succeeded: true,
            cost: Some(TxCost {
                gas_used: 200_000,
                effective_gas_price: 50_000_000,
            }),
        };
        let trade = |hash, n: u64, h: u8, token| {
            CanonicalEvent::FlapTrade(CanonicalTrade {
                block_number: n,
                block_hash: [h; 32],
                tx_index: 0,
                tx_hash: hash,
                log_index: 0,
                origin: buyer,
                sell: false,
                token,
                trader: buyer,
                amount: tokens.to_be_bytes(),
                eth: input.to_be_bytes(),
                fee: [0; 32],
                post_price: [0; 32],
            })
        };
        let reconnect = |x: &mut LiveRuntime| {
            let failed = x
                .executor
                .as_mut()
                .unwrap()
                .end_mut()
                .connect(Duration::from_secs(2));
            assert!(failed.is_empty());
        };
        let wait = Duration::from_secs(2);
        let silent = Duration::from_millis(300);
        let (a, b) = ([0xA1; 20], [0xB2; 20]);
        // A: bought at nonce 0 and included.
        let (out, refused) = x.on_strategy(&buy(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None));
        let a_tx = out[0].handoff.tx_hash;
        r.requests.recv_timeout(wait).unwrap();
        let changes = feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 1, 0xb2, 0xb1, vec![receipt(a_tx, 0)]),
                trade(a_tx, HEAD + 1, 0xb2, a),
                complete_of(HEAD + 1, 0xb2),
            ],
        );
        assert!(
            matches!(changes.as_slice(), [PositionChange::Opened { token, .. }] if *token == a),
            "{changes:?}"
        );
        // B: bought at nonce 1, not canonical yet.
        reconnect(&mut x);
        let (out, _) = x.on_strategy(&buy(b), &c2, &clock, now_ms).unwrap();
        assert_eq!(out.len(), 1);
        let b_tx = out[0].handoff.tx_hash;
        r.requests.recv_timeout(wait).unwrap();
        // A's exit triggers now: nothing goes, the exit stays wanted.
        reconnect(&mut x);
        let (out, refused) = x.on_strategy(&exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::SenderNonce)));
        assert!(r.requests.recv_timeout(silent).is_err());
        assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
        // B canonical: A's exit goes on, its approval at nonce 2.
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 2, 0xb3, 0xb2, vec![receipt(b_tx, 1)]),
                trade(b_tx, HEAD + 2, 0xb3, b),
                complete_of(HEAD + 2, 0xb3),
            ],
        );
        assert_eq!(x.exits_to_continue(&c2), vec![a]);
        let (out, refused) = x.on_strategy(&exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None));
        assert_eq!(out[0].kind, IntentKind::Approve);
        let approve = out[0].handoff.tx_hash;
        r.requests.recv_timeout(wait).unwrap();
        // The approval not canonical: the SELL waits.
        assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 3, 0xb4, 0xb3, vec![receipt(approve, 2)]),
                complete_of(HEAD + 3, 0xb4),
            ],
        );
        assert_eq!(x.exits_to_continue(&c2), vec![a]);
        reconnect(&mut x);
        let (out, refused) = x.on_strategy(&exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None));
        assert_eq!(out[0].kind, IntentKind::Sell);
        r.requests.recv_timeout(wait).unwrap();
        assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
        x.shutdown();
        r.clean();
    }

    /// BSC3-048: when an armed path is renewed at the free nonce, and when
    /// not (no loop: right after a renewal the cursor is the free nonce
    /// and nothing is unresolved at it or stopped).
    #[test]
    fn an_arm_is_renewed_only_where_it_cannot_go_on() {
        // Taken by an earlier arm's transaction that landed late.
        assert!(rearm_due(7, 5, false, false));
        // The window of the unresolved transaction at the free nonce over.
        assert!(rearm_due(5, 6, false, true));
        // An UNKNOWN hand-over resolved: the free nonce is the next again.
        assert!(rearm_due(6, 6, true, false));
        // Going on: the cursor is the free nonce, nothing stopped.
        assert!(!rearm_due(6, 6, false, false));
        // Its own transaction out (the free nonce below, nothing
        // unresolved at it: C2 not clean yet) or none free: nothing.
        assert!(!rearm_due(5, 6, false, false));
    }

    /// Strategy C on one fresh curve: its BUY and exit actions, a buyer's
    /// receipts and the Portal's BUY trades.
    struct Sc {
        curve: crate::paper_exec::StrategyCurve,
        held: crate::paper_exec::StrategyCurve,
        tokens: U256,
        input: U256,
        buyer: [u8; 20],
    }

    impl Sc {
        fn new(buyer: [u8; 20]) -> Sc {
            use darkbot_bsc_trading::economics::flap::native_curve20::{
                HoldingCap, NativeBuyInputs, PricedToken, profile_terms, quote_native_buy,
            };
            let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
            let input = U256::from(10_000_000_000_000_000u64);
            let quote = |reserve, circulating, amount| {
                quote_native_buy(&NativeBuyInputs {
                    token: terms,
                    reserve_before: reserve,
                    circulating_before: circulating,
                    input_amount: amount,
                    protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
                    tax_on_bonding_curve_enabled: true,
                    caller_fee_exempt: false,
                    holding_cap: HoldingCap::None,
                })
                .unwrap()
            };
            let fresh = quote(
                U256::ZERO,
                U256::ZERO,
                U256::from(1_000_000_000_000_000_000u128),
            );
            let curve = crate::paper_exec::StrategyCurve {
                terms,
                reserve: fresh.reserve_after,
                circulating: fresh.supply_after,
            };
            let bought = quote(curve.reserve, curve.circulating, input);
            Sc {
                curve,
                held: crate::paper_exec::StrategyCurve {
                    reserve: bought.reserve_after,
                    circulating: bought.supply_after,
                    ..curve
                },
                tokens: bought.output_amount,
                input,
                buyer,
            }
        }

        fn buy(&self, token: [u8; 20]) -> crate::runtime_strategy::StrategyAction {
            crate::runtime_strategy::StrategyAction::Buy {
                token,
                curve: self.curve,
                input_wei: self.input,
                decided_ns: 0,
            }
        }

        fn exit(&self, token: [u8; 20]) -> crate::runtime_strategy::StrategyAction {
            crate::runtime_strategy::StrategyAction::Exit {
                token,
                tokens: self.tokens,
                curve: self.held,
                decided_ns: 0,
            }
        }

        fn receipt(&self, hash: [u8; 32], nonce: u64) -> CanonicalTx {
            CanonicalTx {
                hash,
                index: 0,
                sender: self.buyer,
                nonce,
                succeeded: true,
                cost: Some(crate::chain_follower::TxCost {
                    gas_used: 200_000,
                    effective_gas_price: 50_000_000,
                }),
            }
        }

        fn trade(&self, hash: [u8; 32], n: u64, h: u8, token: [u8; 20]) -> CanonicalEvent {
            CanonicalEvent::FlapTrade(crate::chain_follower::CanonicalTrade {
                block_number: n,
                block_hash: [h; 32],
                tx_index: 0,
                tx_hash: hash,
                log_index: 0,
                origin: self.buyer,
                sell: false,
                token,
                trader: self.buyer,
                amount: self.tokens.to_be_bytes(),
                eth: self.input.to_be_bytes(),
                fee: [0; 32],
                post_price: [0; 32],
            })
        }
    }

    /// The exit reserve the owner's admission entry of `tx` records
    /// (BSC3-057).
    fn reserve_of(state: &Path, tx: [u8; 32]) -> U256 {
        use darkbot_bsc_record::schema::bsc_runtime::live_spend as ls;
        let name = format!("{}.dbrec", alloy_primitives::hex::encode(tx));
        let bytes = fs::read(state.join("live-spend").join(name)).unwrap();
        let entry = darkbot_bsc_record::decode(&ls::SCHEMA, &bytes).unwrap();
        U256::from_be_bytes(entry.opt_u256(ls::EXIT_RESERVE_WEI).unwrap().unwrap())
    }

    fn reconnect(x: &mut LiveRuntime) {
        let failed = x
            .executor
            .as_mut()
            .unwrap()
            .end_mut()
            .connect(Duration::from_secs(2));
        assert!(failed.is_empty());
    }

    /// BSC3-048: position A is open; B's BUY at nonce 1 is handed over and
    /// its bundle is lost. While B's window (its block plus
    /// `BSC_EXEC_VALIDITY_BLOCKS`) runs, nothing takes nonce 1 and A's exit
    /// waits; no new BUY goes first. After the window the exit takes the
    /// same nonce. Then B lands late after all, in place of the approval: B
    /// is included and its position opens, the approval is REPLACED (by
    /// B's hash, B's block) and A's exit goes on at the next nonce. Every
    /// attempt stays in the ledger.
    #[test]
    fn a_lost_bundle_frees_its_nonce_after_its_window_and_the_exit_takes_it() {
        use crate::live_ledger::FactKind;
        use crate::paper_exec::{IntentKind, Refusal};
        let r = Rig::new("live-window", LiveStrategies::StrategyC);
        let sc = Sc::new(r.buyer);
        let mut x = r.runtime("boot-1");
        let mut c2 = c2_at(&r.d, r.buyer, 0);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let t = Cell::new(5_000_000_000u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let now_ms = 1_793_000_000_500;
        let wait = Duration::from_secs(2);
        let silent = Duration::from_millis(300);
        let (a, b, c) = ([0xA1; 20], [0xB2; 20], [0xC3; 20]);
        // A: bought at nonce 0, included at HEAD + 1.
        let (out, _) = x.on_strategy(&sc.buy(a), &c2, &clock, now_ms).unwrap();
        let a_tx = out[0].handoff.tx_hash;
        r.requests.recv_timeout(wait).unwrap();
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 1, 0xb2, 0xb1, vec![sc.receipt(a_tx, 0)]),
                sc.trade(a_tx, HEAD + 1, 0xb2, a),
                complete_of(HEAD + 1, 0xb2),
            ],
        );
        // B: handed over at nonce 1 (head HEAD + 1, window to HEAD + 3); its
        // bundle is lost.
        reconnect(&mut x);
        let (out, _) = x.on_strategy(&sc.buy(b), &c2, &clock, now_ms).unwrap();
        assert_eq!(out.len(), 1);
        let b_tx = out[0].handoff.tx_hash;
        r.requests.recv_timeout(wait).unwrap();
        // BSC3-057: B left the grant room for A's exit and its own.
        let (approval, sell) = x.exit_gas();
        assert_eq!(
            reserve_of(&r.state, b_tx),
            (approval + sell) * U256::from(2u8)
        );
        // A's exit: nonce 1 is B's while its window runs.
        reconnect(&mut x);
        let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::SenderNonce)));
        for (n, h, p) in [(HEAD + 2, 0xb3, 0xb2), (HEAD + 3, 0xb4, 0xb3)] {
            feed(
                &mut x,
                &mut c2,
                vec![block_of(n, h, p, Vec::new()), complete_of(n, h)],
            );
            settle(&mut x, &c2);
            assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
        }
        assert!(r.requests.recv_timeout(silent).is_err());
        // HEAD + 4: B's window is over. The exit takes nonce 1; a new BUY
        // does not go first.
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 4, 0xb5, 0xb4, Vec::new()),
                complete_of(HEAD + 4, 0xb5),
            ],
        );
        // Measured: the local part of the wait, the new arm (key, link)
        // up to the exit's hand-over.
        let local = Instant::now();
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        assert_eq!(x.exits_to_continue(&c2), vec![a]);
        let (out, refused) = x.on_strategy(&sc.buy(c), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::ExitFirst)));
        let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None));
        assert_eq!(out[0].kind, IntentKind::Approve);
        let approve = out[0].handoff.tx_hash;
        eprintln!(
            "BSC3-048 window over -> exit handed over (local): {:?}",
            local.elapsed()
        );
        let signed = darkbot_bsc_trading::transactions::legacy::decode_signed(&out[0].raw).unwrap();
        assert_eq!(signed.tx.nonce, 1);
        r.requests.recv_timeout(wait).unwrap();
        // B lands late, in place of the approval.
        let changes = feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 5, 0xb6, 0xb5, vec![sc.receipt(b_tx, 1)]),
                sc.trade(b_tx, HEAD + 5, 0xb6, b),
                complete_of(HEAD + 5, 0xb6),
            ],
        );
        assert!(
            matches!(changes.as_slice(), [PositionChange::Opened { token, .. }] if *token == b),
            "{changes:?}"
        );
        let book = x.book();
        assert_eq!(book.tx(&b_tx).unwrap().state, FactKind::Included);
        let replaced = book.tx(&approve).unwrap();
        assert_eq!(replaced.state, FactKind::Replaced);
        let by = replaced.inclusion.as_ref().unwrap();
        assert_eq!(
            (by.by_tx, by.block.map(|b| b.0)),
            (Some(b_tx), Some(HEAD + 5))
        );
        // A's exit goes on at nonce 2.
        assert_eq!(x.exits_to_continue(&c2), vec![a]);
        reconnect(&mut x);
        let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None));
        assert_eq!(out[0].kind, IntentKind::Approve);
        let signed = darkbot_bsc_trading::transactions::legacy::decode_signed(&out[0].raw).unwrap();
        assert_eq!(signed.tx.nonce, 2);
        r.requests.recv_timeout(wait).unwrap();
        // Every attempt stays in the ledger: A, B, the replaced approval,
        // the new one.
        assert_eq!(x.book().txs.len(), 4);
        x.shutdown();
        r.clean();
    }

    /// BSC3-048 (review HIGH/MEDIUM): an exit step whose bundle is lost.
    /// While it is out and after its window, no BUY takes its nonce; the
    /// exit stays wanted. The same signed approval is never handed over a
    /// second time (no retry: the owner refuses it). It lands after all:
    /// the SELL follows without another approval. The SELL S1 is lost:
    /// after its window, on a new arm that knows the included approval,
    /// the same SELL is refused again, a SELL on the curve as it is now
    /// (another transaction) goes at S1's nonce; it closes the position and
    /// S1 is REPLACED by it.
    #[test]
    fn a_lost_exit_step_goes_again_at_its_nonce_before_any_buy() {
        use crate::chain_follower::CanonicalTrade;
        use crate::live_ledger::{FactKind, PositionState};
        use crate::paper_exec::{IntentKind, Refusal};
        let r = Rig::new("live-exit-retry", LiveStrategies::StrategyC);
        let sc = Sc::new(r.buyer);
        let mut x = r.runtime("boot-1");
        let mut c2 = c2_at(&r.d, r.buyer, 0);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let t = Cell::new(5_000_000_000u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let now_ms = 1_793_000_000_500;
        let wait = Duration::from_secs(2);
        let silent = Duration::from_millis(300);
        let (a, c) = ([0xA1; 20], [0xC3; 20]);
        let nonce = |raw: &[u8]| {
            darkbot_bsc_trading::transactions::legacy::decode_signed(raw)
                .unwrap()
                .tx
                .nonce
        };
        let empty = |x: &mut LiveRuntime, c2: &mut StateOwner, n: u64, h: u8| {
            feed(
                x,
                c2,
                vec![block_of(n, h, h - 1, Vec::new()), complete_of(n, h)],
            );
            settle(x, c2);
        };
        // A: bought at nonce 0, included at HEAD + 1.
        let (out, _) = x.on_strategy(&sc.buy(a), &c2, &clock, now_ms).unwrap();
        let a_tx = out[0].handoff.tx_hash;
        r.requests.recv_timeout(wait).unwrap();
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 1, 0xb2, 0xb1, vec![sc.receipt(a_tx, 0)]),
                sc.trade(a_tx, HEAD + 1, 0xb2, a),
                complete_of(HEAD + 1, 0xb2),
            ],
        );
        // The exit's approval P1 at nonce 1 (window to HEAD + 3) is lost.
        reconnect(&mut x);
        let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None));
        let p1 = out[0].handoff.tx_hash;
        assert_eq!(nonce(&out[0].raw), 1);
        r.requests.recv_timeout(wait).unwrap();
        // While an exit step is out, no BUY goes.
        let (out, refused) = x.on_strategy(&sc.buy(c), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::ExitFirst)));
        empty(&mut x, &mut c2, HEAD + 2, 0xb3);
        empty(&mut x, &mut c2, HEAD + 3, 0xb4);
        assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
        // P1's window is over: a new arm; the exit is wanted, no BUY goes;
        // the same approval is not handed over again.
        empty(&mut x, &mut c2, HEAD + 4, 0xb5);
        assert!(x.armed(), "{:?}", reason(&x));
        assert_eq!(x.exits_to_continue(&c2), vec![a]);
        let (out, refused) = x.on_strategy(&sc.buy(c), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::ExitFirst)));
        let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::Owner)));
        assert_eq!(
            x.executor.as_ref().unwrap().end().last_refusal(),
            Some(crate::sources::LIVE_INTENT_SEEN)
        );
        assert!(r.requests.recv_timeout(silent).is_err());
        // P1 lands after all: the SELL follows at nonce 2, no approval.
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 5, 0xb6, 0xb5, vec![sc.receipt(p1, 1)]),
                complete_of(HEAD + 5, 0xb6),
            ],
        );
        assert_eq!(x.book().tx(&p1).unwrap().state, FactKind::Included);
        // The next pass: this arm never planned nonce 1 taken; a new arm
        // takes nonce 2 and knows the included approval.
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        assert_eq!(record(&x).opt_u64(l::NEXT_NONCE).unwrap(), Some(2));
        assert_eq!(x.exits_to_continue(&c2), vec![a]);
        reconnect(&mut x);
        let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None));
        assert_eq!((out[0].kind, nonce(&out[0].raw)), (IntentKind::Sell, 2));
        let s1 = out[0].handoff.tx_hash;
        r.requests.recv_timeout(wait).unwrap();
        // S1 (window to HEAD + 7) is lost; no BUY meanwhile.
        empty(&mut x, &mut c2, HEAD + 6, 0xb7);
        empty(&mut x, &mut c2, HEAD + 7, 0xb8);
        assert!(r.requests.recv_timeout(silent).is_err());
        empty(&mut x, &mut c2, HEAD + 8, 0xb9);
        assert!(x.armed(), "{:?}", reason(&x));
        assert_eq!(x.exits_to_continue(&c2), vec![a]);
        let (out, refused) = x.on_strategy(&sc.buy(c), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::ExitFirst)));
        // The same SELL again: refused; on the curve as it is now (a later
        // trade moved it), another SELL goes at nonce 2, no approval.
        let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::Owner)));
        let moved = crate::runtime_strategy::StrategyAction::Exit {
            token: a,
            tokens: sc.tokens,
            curve: crate::paper_exec::StrategyCurve {
                reserve: sc.held.reserve + U256::from(1_000_000_000_000_000u64),
                ..sc.held
            },
            decided_ns: 0,
        };
        reconnect(&mut x);
        let (out, refused) = x.on_strategy(&moved, &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None));
        assert_eq!((out[0].kind, nonce(&out[0].raw)), (IntentKind::Sell, 2));
        let s2 = out[0].handoff.tx_hash;
        assert_ne!(s2, s1);
        r.requests.recv_timeout(wait).unwrap();
        // S2 included: the position closes, S1 is REPLACED by S2.
        let changes = feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 9, 0xba, 0xb9, vec![sc.receipt(s2, 2)]),
                CanonicalEvent::FlapTrade(CanonicalTrade {
                    block_number: HEAD + 9,
                    block_hash: [0xba; 32],
                    tx_index: 0,
                    tx_hash: s2,
                    log_index: 0,
                    origin: r.buyer,
                    sell: true,
                    token: a,
                    trader: r.buyer,
                    amount: sc.tokens.to_be_bytes(),
                    eth: sc.input.to_be_bytes(),
                    fee: [0; 32],
                    post_price: [0; 32],
                }),
                complete_of(HEAD + 9, 0xba),
            ],
        );
        assert_eq!(changes, vec![PositionChange::Ended { token: a }]);
        assert_eq!(x.book().positions[0].state, PositionState::Closed);
        let replaced = x.book().tx(&s1).unwrap();
        assert_eq!(replaced.state, FactKind::Replaced);
        assert_eq!(replaced.inclusion.as_ref().unwrap().by_tx, Some(s2));
        assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
        x.shutdown();
        r.clean();
    }

    /// BSC3-048 (review MEDIUM): a transaction whose block left the chain
    /// was public in that block and may be included at any height: its
    /// window bounds nothing, its nonce stays locked until it is taken.
    #[test]
    fn an_inclusion_that_left_the_chain_has_no_window() {
        let r = Rig::new("live-orphan-window", LiveStrategies::C3);
        let mut x = r.runtime("boot-1");
        let mut c2 = c2_at(&r.d, r.buyer, 5);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let (hash, raw, launcher) = launch();
        c2.pending(hash, launcher, 3, 0, 10);
        let t = Cell::new(10u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let buy = x
            .on_launch(SourceId::PublicNode, hash, &raw, 10, &c2, &clock)
            .unwrap();
        r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
        let tx = CanonicalTx {
            hash: buy.handoff.tx_hash,
            index: 0,
            sender: r.buyer,
            nonce: 5,
            succeeded: true,
            cost: None,
        };
        include(&mut x, &mut c2, vec![tx]);
        feed(
            &mut x,
            &mut c2,
            vec![CanonicalEvent::Reorg {
                orphaned: vec![(HEAD + 1, [0xb2; 32])],
                new_head: (HEAD + 1, [0xc2; 32]),
            }],
        );
        let mut parent = 0xb1;
        for k in 1..=6u8 {
            let h = 0xc1 + k;
            feed(
                &mut x,
                &mut c2,
                vec![
                    block_of(HEAD + u64::from(k), h, parent, Vec::new()),
                    complete_of(HEAD + u64::from(k), h),
                ],
            );
            parent = h;
        }
        let t = x.book().tx(&buy.handoff.tx_hash).unwrap();
        assert!(t.orphaned && !t.window_over(HEAD + 6));
        let why = x.nonce_free(5, Some(HEAD + 6)).unwrap_err();
        assert!(
            why.contains("public since its block left the chain"),
            "{why}"
        );
        assert_eq!(x.free_nonce(&c2), None);
        x.shutdown();
        r.clean();
    }

    /// BSC3-071: the day's BUYs are used up with positions open. The gate
    /// refuses BUYs, not the exits under the same grant: the path stays
    /// armed, a new BUY is refused (BUYS_CLOSED) before anything is signed,
    /// an exit goes. A revoked grant still ends everything.
    #[test]
    fn used_up_buys_close_buys_not_the_exits_of_open_positions() {
        use crate::paper_exec::{IntentKind, Refusal};
        let r = Rig::new("live-buys-closed", LiveStrategies::StrategyC);
        let sc = Sc::new(r.buyer);
        let mut x = r.runtime("boot-1");
        let mut c2 = c2_at(&r.d, r.buyer, 0);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let t = Cell::new(5_000_000_000u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let now_ms = 1_793_000_000_500;
        let wait = Duration::from_secs(2);
        let (a, b) = ([0xA1; 20], [0xB2; 20]);
        // Two BUYs (the day's limit), both included.
        let mut parent = 0xb1;
        for (k, token) in [(1u8, a), (2u8, b)] {
            let (out, _) = x.on_strategy(&sc.buy(token), &c2, &clock, now_ms).unwrap();
            assert_eq!(out.len(), 1, "{:?}", reason(&x));
            let tx = out[0].handoff.tx_hash;
            r.requests.recv_timeout(wait).unwrap();
            let (n, h) = (HEAD + u64::from(k), 0xb1 + k);
            feed(
                &mut x,
                &mut c2,
                vec![
                    block_of(n, h, parent, vec![sc.receipt(tx, u64::from(k - 1))]),
                    sc.trade(tx, n, h, token),
                    complete_of(n, h),
                ],
            );
            parent = h;
            reconnect(&mut x);
        }
        // The gate refuses BUYs now; the path stays armed for the exits.
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let (out, refused) = x
            .on_strategy(&sc.buy([0xC3; 20]), &c2, &clock, now_ms)
            .unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::Owner)));
        assert!(
            record(&x)
                .texts(l::REFUSALS)
                .unwrap()
                .contains(&"BUYS_CLOSED=1")
        );
        let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None));
        assert_eq!(out[0].kind, IntentKind::Approve);
        r.requests.recv_timeout(wait).unwrap();
        // Revoked: everything ends.
        let s = Store::open_in_session(&r.state, 8, Ok("boot-1".into())).unwrap();
        let g = s.current().unwrap().generation;
        assert!(
            s.request("revoke", Entity::LiveGrant, Mode::Disabled, g, NOW)
                .unwrap()
                .applied()
        );
        settle(&mut x, &c2);
        assert!(!x.armed());
        assert_eq!(
            reason(&x),
            ("ENDED".to_string(), Some(LIVE_GRANT_MISSING.to_string()))
        );
        r.clean();
    }

    /// BSC3-048: a restart inside B's window. The window is the admission
    /// entry's: the recovered fact knows it, nothing arms while it runs, and
    /// after it the arm takes nonce 1 again.
    #[test]
    fn a_restart_inside_the_window_waits_for_it() {
        let r = Rig::new("live-window-restart", LiveStrategies::StrategyC);
        let sc = Sc::new(r.buyer);
        let mut x = r.runtime("boot-1");
        let c2 = c2_at(&r.d, r.buyer, 0);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let t = Cell::new(5_000_000_000u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let now_ms = 1_793_000_000_500;
        let (out, _) = x
            .on_strategy(&sc.buy([0xA1; 20]), &c2, &clock, now_ms)
            .unwrap();
        assert_eq!(out.len(), 1);
        r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
        // Killed before anything else; the outcome's fact is lost too.
        std::mem::forget(x);
        for e in fs::read_dir(r.state.join("live-ledger")).unwrap() {
            fs::remove_file(e.unwrap().path()).unwrap();
        }
        r.authorize("boot-2", "after-restart");
        let mut x = r.runtime("boot-2");
        let mut c2 = c2_at(&r.d.join("c2-restarted"), r.buyer, 0);
        // HEAD + 1, HEAD + 2: the window (HEAD + 2) runs: no arm.
        for (n, h, p) in [(HEAD + 1, 0xb2, 0xb1), (HEAD + 2, 0xb3, 0xb2)] {
            feed(
                &mut x,
                &mut c2,
                vec![block_of(n, h, p, Vec::new()), complete_of(n, h)],
            );
            settle(&mut x, &c2);
            assert!(!x.armed());
            assert!(
                reason(&x)
                    .1
                    .unwrap()
                    .starts_with(crate::sources::LIVE_RELEASE_UNRESOLVED)
            );
        }
        // HEAD + 3: over; the arm takes nonce 0 again.
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 3, 0xb4, 0xb3, Vec::new()),
                complete_of(HEAD + 3, 0xb4),
            ],
        );
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        assert_eq!(record(&x).opt_u64(l::NEXT_NONCE).unwrap(), Some(0));
        // The BUY stays unresolved in the ledger until its nonce is taken.
        assert_eq!(x.book().unresolved().count(), 1);
        x.shutdown();
        r.clean();
    }

    /// BSC3-048: a transaction whose window is not known (an entry from
    /// before the window was written) keeps the arm blocked, however many
    /// blocks pass.
    #[test]
    fn an_unknown_window_keeps_the_arm_blocked() {
        let r = Rig::new("live-window-unknown", LiveStrategies::C3);
        crate::live_ledger::Ledger::open(&r.state)
            .append(
                &crate::live_ledger::Fact {
                    kind: crate::live_ledger::FactKind::Released,
                    tx_hash: [0x77; 32],
                    intent: crate::sources::LiveIntentKind::Buy,
                    component: ComponentId::ExecC3,
                    token: [0x55; 20],
                    nonce: 5,
                    grant_generation: 1,
                    input_wei: Some(U256::from(1u64)),
                    max_wei: Some(U256::from(2u64)),
                    block: None,
                    gas_wei: None,
                    tokens: None,
                    eth_wei: None,
                    max_block: None,
                    by_tx: None,
                    response: None,
                },
                1,
            )
            .unwrap();
        let mut x = r.runtime("boot-1");
        let mut c2 = c2_at(&r.d, r.buyer, 5);
        let mut parent = 0xb1;
        for k in 1..=8u8 {
            let h = 0xb1 + k;
            feed(
                &mut x,
                &mut c2,
                vec![
                    block_of(HEAD + u64::from(k), h, parent, Vec::new()),
                    complete_of(HEAD + u64::from(k), h),
                ],
            );
            parent = h;
        }
        settle(&mut x, &c2);
        assert!(!x.armed());
        assert!(
            reason(&x)
                .1
                .unwrap()
                .starts_with(crate::sources::LIVE_RELEASE_UNRESOLVED)
        );
        r.clean();
    }

    /// BSC3-024 (review MEDIUM): the end cannot write its BUY's fact (the
    /// ledger refuses writes): the arm ends at once and keeps its fence;
    /// this process does not arm again (a fact it wrote may be missing);
    /// the restart recovers the BUY from the owner's admission entry as
    /// UNKNOWN, and nothing arms while it is unresolved.
    #[test]
    fn a_fact_the_end_cannot_write_ends_the_arm_and_is_recovered() {
        let r = Rig::new("live-unwritable", LiveStrategies::C3);
        let mut x = r.runtime("boot-1");
        let mut c2 = c2_at(&r.d, r.buyer, 5);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let ledger = r.state.join("live-ledger");
        fs::create_dir_all(&ledger).unwrap();
        fs::set_permissions(&ledger, fs::Permissions::from_mode(0o500)).unwrap();
        let (hash, raw, launcher) = launch();
        c2.pending(hash, launcher, 3, 0, 10);
        let t = Cell::new(10u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let buy = x
            .on_launch(SourceId::PublicNode, hash, &raw, 10, &c2, &clock)
            .unwrap();
        r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!x.armed());
        let (state, why) = reason(&x);
        assert_eq!(state, "ENDED");
        assert!(
            why.as_deref()
                .unwrap()
                .starts_with(crate::sources::LIVE_LEDGER_UNREADABLE),
            "{why:?}"
        );
        assert_eq!(r.fences(".fence"), vec!["live-7-0.fence"]);
        // Writable again: this process still does not arm.
        fs::set_permissions(&ledger, fs::Permissions::from_mode(0o700)).unwrap();
        let s = Store::open_in_session(&r.state, 8, Ok("boot-1".into())).unwrap();
        let g = s.current().unwrap().generation;
        assert!(
            s.request("revoke", Entity::LiveGrant, Mode::Disabled, g, NOW)
                .unwrap()
                .applied()
        );
        settle(&mut x, &c2);
        r.authorize("boot-1", "regrant");
        settle(&mut x, &c2);
        assert!(!x.armed());
        assert!(
            reason(&x)
                .1
                .unwrap()
                .starts_with(crate::sources::LIVE_LEDGER_UNREADABLE)
        );
        assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
        std::mem::forget(x);
        // The restart: the BUY is a fact, UNKNOWN, and blocks the arm.
        r.authorize("boot-2", "after-restart");
        let mut x = r.runtime("boot-2");
        let tx = &x.book().tx(&buy.handoff.tx_hash).unwrap().released;
        assert_eq!(
            (tx.kind, tx.token, tx.nonce),
            (crate::live_ledger::FactKind::Unknown, buy.token, 5)
        );
        let c2 = c2_at(&r.d.join("c2-restarted"), r.buyer, 5);
        settle(&mut x, &c2);
        assert!(!x.armed());
        assert!(
            reason(&x)
                .1
                .unwrap()
                .starts_with(crate::sources::LIVE_RELEASE_UNRESOLVED)
        );
        r.clean();
    }

    /// BSC3-041/045: an entry of the ledger that does not read blocks the
    /// arm with its reason; nothing is folded from a partial ledger.
    #[test]
    fn an_unreadable_ledger_blocks_the_arm() {
        let r = Rig::new("live-unreadable", LiveStrategies::C3);
        let ledger = r.state.join("live-ledger");
        fs::create_dir_all(&ledger).unwrap();
        fs::write(ledger.join("00000000000000000000.dbrec"), b"torn").unwrap();
        let mut x = r.runtime("boot-1");
        let c2 = c2_at(&r.d, r.buyer, 5);
        settle(&mut x, &c2);
        assert!(!x.armed());
        let (state, why) = reason(&x);
        assert_eq!(state, "BLOCKED");
        assert!(
            why.as_deref()
                .unwrap()
                .starts_with(crate::sources::LIVE_LEDGER_UNREADABLE),
            "{why:?}"
        );
        assert!(r.fences(".fence").is_empty());
        r.clean();
    }

    /// BSC3-027: the buyer's balance must hold a BUY, its exit and the
    /// exits of what is open (approval and SELL at the signed gas). Enough
    /// for a BUY but not for its exit: nothing arms (UNFUNDED, the need
    /// named). With a position open and enough for its exit only: the path
    /// arms for the exit, a BUY is refused, the exit goes.
    #[test]
    fn a_buyer_short_of_the_exits_buys_nothing() {
        use crate::paper_exec::{IntentKind, Refusal};
        let r = Rig::new("live-short", LiveStrategies::StrategyC);
        let sc = Sc::new(r.buyer);
        let mut x = r.probed("boot-1", short);
        let c2 = c2_at(&r.d, r.buyer, 0);
        settle(&mut x, &c2);
        assert!(!x.armed());
        let (state, why) = reason(&x);
        assert_eq!(state, "UNFUNDED");
        assert!(why.as_deref().unwrap().contains("exit"), "{why:?}");
        x.shutdown();
        // A position of A is open (bought at nonce 0, included).
        let a = [0xA1; 20];
        let ledger = crate::live_ledger::Ledger::open(&r.state);
        let buy = crate::live_ledger::Fact {
            kind: crate::live_ledger::FactKind::Released,
            tx_hash: [0x31; 32],
            intent: crate::sources::LiveIntentKind::Buy,
            component: ComponentId::StrategyC,
            token: a,
            nonce: 0,
            grant_generation: 1,
            input_wei: Some(sc.input),
            max_wei: Some(sc.input),
            block: None,
            gas_wei: None,
            tokens: None,
            eth_wei: None,
            max_block: Some(HEAD + 2),
            by_tx: None,
            response: None,
        };
        ledger.append(&buy, 1).unwrap();
        let included = crate::live_ledger::Fact {
            block: Some((HEAD, [0xb1; 32], 1_793_000_000_000)),
            gas_wei: Some(U256::from(1u8)),
            tokens: Some(sc.tokens),
            eth_wei: Some(sc.input),
            ..crate::live_ledger::Fact::about(&buy, crate::live_ledger::FactKind::Included)
        };
        ledger.append(&included, 1).unwrap();
        // And one of B (nonce 1): both exits (1.6e14) are more than the
        // balance (1e14), one exit step is not; the path arms for them.
        let b = crate::live_ledger::Fact {
            tx_hash: [0x32; 32],
            token: [0xB2; 20],
            nonce: 1,
            ..buy.clone()
        };
        ledger.append(&b, 1).unwrap();
        let b_included = crate::live_ledger::Fact {
            tx_hash: b.tx_hash,
            token: b.token,
            nonce: 1,
            ..included.clone()
        };
        ledger.append(&b_included, 1).unwrap();
        let mut x = r.probed("boot-1", exits_only);
        let c2 = c2_at(&r.d.join("c2-open"), r.buyer, 2);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        // Each open lot's exit: approval 5e12 + SELL 7.5e13 at the setup's
        // gas.
        let (approval, sell) = x.exit_gas();
        assert_eq!(
            (approval, sell),
            (
                U256::from(5_000_000_000_000u64),
                U256::from(75_000_000_000_000u64)
            )
        );
        assert_eq!(x.exit_reserve(), (approval + sell) * U256::from(2u8));
        let t = Cell::new(5_000_000_000u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let now_ms = 1_793_000_000_500;
        let (out, refused) = x
            .on_strategy(&sc.buy([0xC3; 20]), &c2, &clock, now_ms)
            .unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::Unfunded)));
        assert!(
            record(&x)
                .texts(l::REFUSALS)
                .unwrap()
                .contains(&"BUYS_UNFUNDED=1")
        );
        let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None), "{:?}", reason(&x));
        assert_eq!(out[0].kind, IntentKind::Approve);
        r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
        // A's approval is out: its SELL and B's exit are left to reserve.
        assert_eq!(x.exit_reserve(), sell + approval + sell);
        x.shutdown();
        r.clean();
    }

    /// BSC3-051: a provider refuses a BUY. The answer is read before the
    /// next decision: the next BUY is refused before anything is signed
    /// (no attempt, nothing sent again), the status counts the answer and
    /// the refusal, the refused BUY stays unresolved in the ledger.
    #[test]
    fn a_buy_a_provider_refused_closes_the_arms_buys() {
        use crate::paper_exec::Refusal;
        let r = Rig::answering(
            "live-provider-reject",
            LiveStrategies::StrategyC,
            vec![crate::live_end::tests::Answer::Reject],
        );
        let sc = Sc::new(r.buyer);
        let mut x = r.runtime("boot-1");
        let c2 = c2_at(&r.d, r.buyer, 0);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let t = Cell::new(5_000_000_000u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let now_ms = 1_793_000_000_500;
        let (out, _) = x
            .on_strategy(&sc.buy([0xA1; 20]), &c2, &clock, now_ms)
            .unwrap();
        let first = out[0].handoff.tx_hash;
        r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while x.executor.as_ref().unwrap().end().buy_refused().is_none()
            && Instant::now() < deadline
        {
            x.poll_responses(&clock);
            std::thread::sleep(Duration::from_millis(5));
        }
        let used = buy_usage(&r.state, NOW).unwrap().used;
        let (out, refused) = x
            .on_strategy(&sc.buy([0xB2; 20]), &c2, &clock, now_ms)
            .unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::Dispatch)));
        assert_eq!(buy_usage(&r.state, NOW).unwrap().used, used);
        assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
        let status = record(&x);
        assert!(status.texts(l::RESPONSES).unwrap().contains(&"REJECT=1"));
        assert!(
            status
                .texts(l::REFUSALS)
                .unwrap()
                .contains(&"BUYS_PROVIDER_REFUSED=1")
        );
        x.record_responses();
        let tx = x.book().tx(&first).unwrap();
        assert_eq!(tx.state, crate::live_ledger::FactKind::Released);
        assert_eq!(tx.responses, vec!["CLUB48_BUILDER_V1:REJECT".to_string()]);
        std::mem::forget(x);
        // Another process under the same grant, once A's window is over
        // (it arms at nonce 0 again): the ledger keeps the BUYs closed
        // until the operator grants anew.
        let mut x = r.runtime("boot-1");
        let mut c2 = c2_at(&r.d.join("c2-again"), r.buyer, 0);
        let mut parent = 0xb1;
        for k in 1..=3u8 {
            let h = 0xd0 + k;
            feed(
                &mut x,
                &mut c2,
                vec![
                    block_of(HEAD + u64::from(k), h, parent, Vec::new()),
                    complete_of(HEAD + u64::from(k), h),
                ],
            );
            parent = h;
        }
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let (out, refused) = x
            .on_strategy(&sc.buy([0xC3; 20]), &c2, &clock, now_ms)
            .unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::Dispatch)));
        x.shutdown();
        r.clean();
    }

    /// BSC3-058: under the operator's separate EXIT-ONLY grant nothing arms
    /// without an open position (the reason names it); with one the path
    /// arms for its exit: a BUY is refused before anything is signed, the
    /// exit's approval goes.
    #[test]
    fn an_exit_only_grant_arms_for_the_exits_of_open_positions_only() {
        use crate::paper_exec::IntentKind;
        let r = Rig::new("live-exit-only", LiveStrategies::StrategyC);
        let sc = Sc::new(r.buyer);
        let s = Store::open_in_session(&r.state, 8, Ok("boot-1".into())).unwrap();
        let grant = LiveGrant {
            max_input_wei: 10_000_000_000_000_000,
            max_spend_wei: 100_000_000_000_000_000,
            adapters: vec![LiveAdapter::Club48BuilderV1],
            exit_only: true,
            ..grant_of(NOW + 3_600_000, LiveStrategies::StrategyC)
        };
        let g = s.current().unwrap().generation;
        let receipt = s
            .grant_live("exit-only", grant.clone(), grant.scope_sha256(), g, NOW)
            .unwrap();
        assert!(receipt.applied(), "{}", receipt.line());
        let mut x = r.runtime("boot-1");
        let c2 = c2_at(&r.d, r.buyer, 0);
        settle(&mut x, &c2);
        assert!(!x.armed());
        assert_eq!(
            reason(&x).1.as_deref(),
            Some(crate::sources::LIVE_GRANT_EXIT_ONLY)
        );
        x.shutdown();
        // A position of A is open (bought at nonce 0 before, included).
        let a = [0xA1; 20];
        let ledger = crate::live_ledger::Ledger::open(&r.state);
        let buy = crate::live_ledger::Fact {
            kind: crate::live_ledger::FactKind::Released,
            tx_hash: [0x51; 32],
            intent: crate::sources::LiveIntentKind::Buy,
            component: ComponentId::StrategyC,
            token: a,
            nonce: 0,
            grant_generation: 1,
            input_wei: Some(sc.input),
            max_wei: Some(sc.input),
            block: None,
            gas_wei: None,
            tokens: None,
            eth_wei: None,
            max_block: Some(HEAD + 2),
            by_tx: None,
            response: None,
        };
        ledger.append(&buy, 1).unwrap();
        let included = crate::live_ledger::Fact {
            block: Some((HEAD, [0xb1; 32], 1_793_000_000_000)),
            gas_wei: Some(U256::from(1u8)),
            tokens: Some(sc.tokens),
            eth_wei: Some(sc.input),
            ..crate::live_ledger::Fact::about(&buy, crate::live_ledger::FactKind::Included)
        };
        ledger.append(&included, 1).unwrap();
        let mut x = r.runtime("boot-1");
        let c2 = c2_at(&r.d.join("c2-open"), r.buyer, 1);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let t = Cell::new(5_000_000_000u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let now_ms = 1_793_000_000_500;
        let (out, refused) = x
            .on_strategy(&sc.buy([0xC3; 20]), &c2, &clock, now_ms)
            .unwrap();
        assert_eq!(
            (out.len(), refused),
            (0, Some(crate::paper_exec::Refusal::Owner))
        );
        assert!(
            record(&x)
                .texts(l::REFUSALS)
                .unwrap()
                .contains(&"BUYS_CLOSED=1")
        );
        let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None), "{:?}", reason(&x));
        assert_eq!(out[0].kind, IntentKind::Approve);
        r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
        x.shutdown();
        r.clean();
    }

    /// BSC3-072 (review LOW): a C3 BUY on the LIVE path exits through the
    /// strategy once its position opens: the strategy's cap must hold its
    /// exit, or the BUY is refused before anything is signed.
    #[test]
    fn a_c3_buy_needs_room_for_its_exit_in_the_strategy_cap() {
        let mut r = Rig::new("live-c3-exit-room", LiveStrategies::C3);
        let x = &r.exec.exec;
        let price = U256::from(x.gas_price_wei);
        let exit = price * U256::from(x.approve_gas_limit + x.sell_gas_limit);
        r.exec.exec.strategy_session_cap_wei = u128::try_from(exit).unwrap() - 1;
        let mut x = r.runtime("boot-1");
        let mut c2 = c2_at(&r.d, r.buyer, 5);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let (hash, raw, launcher) = launch();
        c2.pending(hash, launcher, 3, 0, 10);
        let t = Cell::new(10u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        assert!(
            x.on_launch(SourceId::PublicNode, hash, &raw, 10, &c2, &clock)
                .is_none()
        );
        assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
        assert!(
            record(&x)
                .texts(l::REFUSALS)
                .unwrap()
                .contains(&"SESSION_CAP=1")
        );
        x.shutdown();
        r.clean();
    }

    /// BSC3-027 (review MEDIUM): the probe's balance is of its block. A
    /// transaction the ledger holds included in a later block is not paid
    /// in it yet: it counts against what is left.
    #[test]
    fn what_a_later_block_included_is_not_paid_in_the_probed_balance() {
        use crate::paper_exec::Refusal;
        let r = Rig::new("live-later", LiveStrategies::StrategyC);
        let sc = Sc::new(r.buyer);
        let ledger = crate::live_ledger::Ledger::open(&r.state);
        let x = &r.exec.exec;
        let most = sc.input + U256::from(x.gas_price_wei) * U256::from(x.gas_limit);
        let buy = crate::live_ledger::Fact {
            kind: crate::live_ledger::FactKind::Released,
            tx_hash: [0x41; 32],
            intent: crate::sources::LiveIntentKind::Buy,
            component: ComponentId::StrategyC,
            token: [0xA1; 20],
            nonce: 0,
            grant_generation: 1,
            input_wei: Some(sc.input),
            max_wei: Some(most),
            block: None,
            gas_wei: None,
            tokens: None,
            eth_wei: None,
            max_block: Some(HEAD + 2),
            by_tx: None,
            response: None,
        };
        ledger.append(&buy, 1).unwrap();
        let included = crate::live_ledger::Fact {
            block: Some((HEAD + 1, [0xb2; 32], 1_793_000_001_000)),
            gas_wei: Some(U256::from(1u8)),
            tokens: Some(sc.tokens),
            eth_wei: Some(sc.input),
            ..crate::live_ledger::Fact::about(&buy, crate::live_ledger::FactKind::Included)
        };
        ledger.append(&included, 1).unwrap();
        // The probe reads the balance at HEAD, before A's block.
        let mut x = r.probed("boot-1", two);
        let c2 = c2_at(&r.d, r.buyer, 1);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let t = Cell::new(5_000_000_000u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        // B with its exit and A's needs more than the balance less A.
        let (out, refused) = x
            .on_strategy(&sc.buy([0xB2; 20]), &c2, &clock, 1_793_000_000_500)
            .unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::Unfunded)));
        assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
        x.shutdown();
        r.clean();
    }

    /// BSC3-027 (review MEDIUM): each BUY is checked against the probed
    /// balance with its own most (a Strategy C BUY is not C3-sized), its
    /// exit and the open exits, and what left since the probe counts: a
    /// balance that arms the path but does not hold this BUY refuses it
    /// (UNFUNDED); after a first BUY lands, a second one is checked
    /// against what is left.
    #[test]
    fn each_buy_needs_its_most_and_the_exits_in_what_is_left() {
        use crate::paper_exec::Refusal;
        let r = Rig::new("live-left", LiveStrategies::StrategyC);
        let sc = Sc::new(r.buyer);
        let t = Cell::new(5_000_000_000u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let now_ms = 1_793_000_000_500;
        let mut x = r.probed("boot-1", mid);
        let mut c2 = c2_at(&r.d, r.buyer, 0);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let (out, refused) = x
            .on_strategy(&sc.buy([0xA1; 20]), &c2, &clock, now_ms)
            .unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::Unfunded)));
        assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
        x.shutdown();
        // Enough for one: it goes and lands; the second is checked against
        // the balance less the first's most.
        let mut x = r.probed("boot-1", two);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let (out, refused) = x
            .on_strategy(&sc.buy([0xA1; 20]), &c2, &clock, now_ms)
            .unwrap();
        assert_eq!((out.len(), refused), (1, None), "{:?}", reason(&x));
        let first = out[0].handoff.tx_hash;
        r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
        let (approval, sell) = x.exit_gas();
        assert_eq!(reserve_of(&r.state, first), approval + sell);
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 1, 0xb2, 0xb1, vec![sc.receipt(first, 0)]),
                sc.trade(first, HEAD + 1, 0xb2, [0xA1; 20]),
                complete_of(HEAD + 1, 0xb2),
            ],
        );
        reconnect(&mut x);
        let (out, refused) = x
            .on_strategy(&sc.buy([0xB2; 20]), &c2, &clock, now_ms)
            .unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::Unfunded)));
        assert!(
            record(&x)
                .texts(l::REFUSALS)
                .unwrap()
                .contains(&"BUYS_UNFUNDED=1")
        );
        x.shutdown();
        r.clean();
    }

    /// BSC3-047: a token bought, exited and bought again in a later arm is
    /// a second lot, with its own tokens. A reorg that takes the first
    /// lot's SELL and the second lot's BUY reopens the first lot: the
    /// second ends before the first is reported open, and back. After a
    /// restart with the second lot open, its exit approves again (the
    /// first lot's approval went with its SELL), its own tokens; the book
    /// holds both lots, the first closed with its result.
    #[test]
    fn a_token_bought_again_approves_its_exit_again() {
        use crate::chain_follower::CanonicalTrade;
        use crate::live_ledger::PositionState;
        use crate::paper_exec::IntentKind;
        use darkbot_bsc_trading::transactions::legacy::decode_signed;
        use darkbot_bsc_trading::venues::flap::abi::decode_approve;
        use darkbot_bsc_trading::venues::flap::recognize::decode_swap_exact_input;
        let r = Rig::new("live-rebuy", LiveStrategies::StrategyC);
        let sc = Sc::new(r.buyer);
        let a = [0xA1; 20];
        let tokens2 = sc.tokens * U256::from(2u64);
        let t = Cell::new(5_000_000_000u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        let now_ms = 1_793_000_000_500;
        let wait = Duration::from_secs(2);
        let trade = |hash, n, h, sell, amount: U256| {
            CanonicalEvent::FlapTrade(CanonicalTrade {
                block_number: n,
                block_hash: [h; 32],
                tx_index: 0,
                tx_hash: hash,
                log_index: 0,
                origin: r.buyer,
                sell,
                token: a,
                trader: r.buyer,
                amount: amount.to_be_bytes(),
                eth: sc.input.to_be_bytes(),
                fee: [0; 32],
                post_price: [0; 32],
            })
        };
        let sold = |hash, n, h| trade(hash, n, h, true, sc.tokens);
        // Lot 1: bought at nonce 0, approved at 1, sold at 2.
        let mut x = r.runtime("boot-1");
        let mut c2 = c2_at(&r.d, r.buyer, 0);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let (out, _) = x.on_strategy(&sc.buy(a), &c2, &clock, now_ms).unwrap();
        let buy1 = out[0].handoff.tx_hash;
        r.requests.recv_timeout(wait).unwrap();
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 1, 0xb2, 0xb1, vec![sc.receipt(buy1, 0)]),
                sc.trade(buy1, HEAD + 1, 0xb2, a),
                complete_of(HEAD + 1, 0xb2),
            ],
        );
        reconnect(&mut x);
        let (out, _) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!(out[0].kind, IntentKind::Approve);
        let approve1 = out[0].handoff.tx_hash;
        r.requests.recv_timeout(wait).unwrap();
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 2, 0xb3, 0xb2, vec![sc.receipt(approve1, 1)]),
                complete_of(HEAD + 2, 0xb3),
            ],
        );
        reconnect(&mut x);
        let (out, _) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
        assert_eq!(out[0].kind, IntentKind::Sell);
        let sell1 = out[0].handoff.tx_hash;
        r.requests.recv_timeout(wait).unwrap();
        let changes = feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 3, 0xb4, 0xb3, vec![sc.receipt(sell1, 2)]),
                sold(sell1, HEAD + 3, 0xb4),
                complete_of(HEAD + 3, 0xb4),
            ],
        );
        assert_eq!(changes, vec![PositionChange::Ended { token: a }]);
        x.shutdown();
        // Lot 2: the next process buys the token again, at nonce 3.
        r.authorize("boot-2", "rebuy");
        let mut x = r.runtime("boot-2");
        let mut c2 = c2_at(&r.d.join("c2-2"), r.buyer, 3);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let (out, refused) = x.on_strategy(&sc.buy(a), &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None), "{:?}", reason(&x));
        let buy2 = out[0].handoff.tx_hash;
        r.requests.recv_timeout(wait).unwrap();
        let changes = feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 1, 0xd2, 0xb1, vec![sc.receipt(buy2, 3)]),
                trade(buy2, HEAD + 1, 0xd2, false, tokens2),
                complete_of(HEAD + 1, 0xd2),
            ],
        );
        let opened = |c: &PositionChange| match c {
            PositionChange::Opened { token, tokens, .. } if *token == a => Some(*tokens),
            _ => None,
        };
        assert_eq!(changes.len(), 1);
        assert_eq!(opened(&changes[0]), Some(tokens2));
        // A reorg takes SELL 1's and BUY 2's blocks: lot 2 ends, lot 1 is
        // open again (its SELL unresolved); included again, back.
        let changes = x.on_canonical(&CanonicalEvent::Reorg {
            orphaned: vec![(HEAD + 3, [0xb4; 32]), (HEAD + 1, [0xd2; 32])],
            new_head: (HEAD + 1, [0xe2; 32]),
        });
        assert_eq!(changes.len(), 2, "{changes:?}");
        assert_eq!(changes[0], PositionChange::Ended { token: a });
        assert_eq!(opened(&changes[1]), Some(sc.tokens));
        assert_eq!(x.book().positions[0].state, PositionState::Exiting);
        assert_eq!(x.book().positions[1].state, PositionState::Pending);
        let mut changes = Vec::new();
        for e in [
            block_of(
                HEAD + 1,
                0xe2,
                0xb1,
                vec![sc.receipt(sell1, 2), sc.receipt(buy2, 3)],
            ),
            sold(sell1, HEAD + 1, 0xe2),
            trade(buy2, HEAD + 1, 0xe2, false, tokens2),
            complete_of(HEAD + 1, 0xe2),
        ] {
            changes.extend(x.on_canonical(&e));
        }
        assert_eq!(changes.len(), 2, "{changes:?}");
        assert_eq!(changes[0], PositionChange::Ended { token: a });
        assert_eq!(opened(&changes[1]), Some(tokens2));
        x.shutdown();
        // A restart with lot 2 open: its exit approves first.
        r.authorize("boot-3", "lot-two");
        let mut x = r.runtime("boot-3");
        let mut c2 = c2_at(&r.d.join("c2-3"), r.buyer, 4);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let exit2 = crate::runtime_strategy::StrategyAction::Exit {
            token: a,
            tokens: tokens2,
            curve: sc.held,
            decided_ns: 0,
        };
        let (out, refused) = x.on_strategy(&exit2, &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None), "{:?}", reason(&x));
        assert_eq!(out[0].kind, IntentKind::Approve);
        let input = decode_signed(&out[0].raw).unwrap().tx.input;
        assert_eq!(decode_approve(input).unwrap().1, tokens2);
        let approve2 = out[0].handoff.tx_hash;
        r.requests.recv_timeout(wait).unwrap();
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + 1, 0xf2, 0xb1, vec![sc.receipt(approve2, 4)]),
                complete_of(HEAD + 1, 0xf2),
            ],
        );
        reconnect(&mut x);
        let (out, refused) = x.on_strategy(&exit2, &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (1, None), "{:?}", reason(&x));
        assert_eq!(out[0].kind, IntentKind::Sell);
        let input = decode_signed(&out[0].raw).unwrap().tx.input;
        assert_eq!(
            decode_swap_exact_input(input).unwrap().input_amount,
            tokens2
        );
        r.requests.recv_timeout(wait).unwrap();
        let book = x.book();
        assert_eq!(book.positions.len(), 2, "{:?}", book.positions);
        assert_eq!(book.positions[0].state, PositionState::Closed);
        assert_eq!(book.positions[1].state, PositionState::Exiting);
        assert_eq!(book.positions[1].tokens, tokens2);
        assert!(book.positions[1].approved && book.positions[0].approved);
        x.shutdown();
        r.clean();
    }
}

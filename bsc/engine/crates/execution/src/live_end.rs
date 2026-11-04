//! The LIVE end of the one executor (plan 28.10., package 4).
//!
//! The buyer's [`LiveKey`] signs. Each prepared transaction is planned
//! under its release profile through the granted builders (C3: the birth
//! entry bundle behind the original launch; Strategy C: the transaction
//! alone, a one-transaction bundle), qualified for release with the
//! verified LIVE qualification, framed by the builders' own framing, and
//! only then admitted by the native owner as it holds at that moment
//! ([`Store::admit_live`]: the switches and the gate, a BUY's daily and
//! campaign slot, the grant's budget, all durable before any byte can
//! leave). The release dispatch sends it over qualified transports.
//!
//! Nothing is retried. The owner's admission names the transaction's token
//! and nonce and is durable before any byte can leave (its write-ahead
//! record, BSC3-024); right after the dispatch its outcome is a fact of the
//! LIVE ledger: released (handed off), UNKNOWN (its handoff did not
//! complete) or withdrawn (no byte left). The executor stops the sender's
//! sequence on an UNKNOWN. The route is the builders' TLS connections or,
//! for a non-sending run, the loopback adapter on 127.0.0.1.

use crate::live_key::LiveKey;
use crate::live_ledger::{Fact, FactKind, Ledger};
use crate::live_qualification::{Verified, capability};
use crate::paper_exec::{
    End, Handed, HandoffTime, Handover, Refusal, Submission, consumed_by, plan_inputs,
};
use crate::sources::{
    BuyDay, ComponentId, LiveAdapter, LiveIntent, LiveIntentKind, LiveNow, NonceOrigin, OwnerMark,
    Store,
};
use crate::submission::adapter_frame;
use crate::submission::frame::Frame;
use crate::submission::loopback::{LoopbackTransport, loopback_response, qualify_loopback};
use crate::submission::response::{ResponseBuffer, ResponseOutcome};
use crate::submission::send::{
    DispatchContext, FanoutReport, Poll, ReleaseArmed, SendOutcome, dispatch_release,
    poll_release_response,
};
use crate::submission::tls::{
    AuthorizationToken, BLOCKRAZOR_DOCUMENTED_HOST, CLUB48_DOCUMENTED_HOST, HTTPS_PORT,
    ProviderTarget, Q5A_TLS_POLICY, TlsConnectionOwner, TlsEndpoint, TlsSetup, qualify_transport,
    response_transport,
};
use crate::submission::transport::{Clock, TransportQualifications};
use alloy_primitives::U256;
use darkbot_bsc_trading::intent::{AdapterId, IntentLane, MAX_ADAPTERS};
use darkbot_bsc_trading::signer::Signer;
use darkbot_bsc_trading::submission_policy::capability::{AdapterBinding, AdapterTable};
use darkbot_bsc_trading::submission_policy::plan::{SubmissionPlan, TransportStartDeadline};
use darkbot_bsc_trading::venues::flap::binding::ExpectedTx;
use std::time::Duration;

/// Where the LIVE end's requests go.
pub enum LiveRoute {
    /// The granted builders over their qualified TLS connections; the
    /// BlockRazor builder needs its authorization.
    Builders {
        setup: TlsSetup,
        blockrazor: Option<AuthorizationToken>,
    },
    /// The builders' frames to a stand-in on 127.0.0.1:`port`: nothing
    /// leaves the host.
    Loopback { port: u16 },
}

enum Link {
    Tls(Box<TlsConnectionOwner>),
    Loopback(LoopbackTransport),
}

struct Slot {
    adapter: AdapterId,
    live: LiveAdapter,
    target: ProviderTarget,
    link: Link,
    /// The released transaction whose request this connection carries and
    /// whose answer it awaits (an index of `released`).
    awaiting: Option<usize>,
    /// Holds one answer, as large as the largest request frame.
    buffer: Option<ResponseBuffer>,
}

/// The providers' answers to this end's releases (BSC3-051), by outcome;
/// NO_RESPONSE: the connection gave the request up or was replaced first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Responses {
    pub ack: u64,
    pub reject: u64,
    pub rate_limited: u64,
    pub protocol: u64,
    pub connection: u64,
    pub no_response: u64,
}

impl Responses {
    /// Each outcome with its count, as the status names them.
    pub fn named(&self) -> [(&'static str, u64); 6] {
        [
            ("ACK", self.ack),
            ("REJECT", self.reject),
            ("RATE_LIMITED", self.rate_limited),
            ("PROTOCOL", self.protocol),
            ("CONNECTION", self.connection),
            ("NO_RESPONSE", self.no_response),
        ]
    }
}

fn outcome_name(outcome: ResponseOutcome) -> &'static str {
    match outcome {
        ResponseOutcome::Ack => "ACK",
        ResponseOutcome::Reject { .. } => "REJECT",
        ResponseOutcome::RateLimited => "RATE_LIMITED",
        ResponseOutcome::Protocol(_) => "PROTOCOL",
        ResponseOutcome::Connection => "CONNECTION",
    }
}

/// When the release took a transaction: the first qualified handoff
/// boundary among the granted builders, and how many took it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveHandoff {
    pub tx_hash: [u8; 32],
    pub at_ns: u64,
    pub adapters: u8,
    /// REV.1.0 R2: the owner's attempt this transaction is (a launch's
    /// BUY its lease's, any other its admission's: the hash).
    pub attempt: [u8; 32],
    /// REV.1.0 R2: the dispatch's record (its clock, the frame selection,
    /// each adapter's outcome and boundaries), for the attempt's record.
    pub report: FanoutReport,
    /// REV.1.0 R2 review H2: the granted builder each adapter of the
    /// dispatch is (by its index), for the campaign's adapter scope.
    pub builders: [Option<LiveAdapter>; MAX_ADAPTERS],
}

/// REV.1.0 R2: one provider's answer to a released transaction, as read:
/// the adapter, the outcome and when it was classified (`None`: the
/// connection gave the request up, NO_RESPONSE).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Answer {
    pub tx_hash: [u8; 32],
    pub adapter: AdapterId,
    pub outcome: &'static str,
    pub at_ns: Option<u64>,
}

impl HandoffTime for LiveHandoff {
    fn at_ns(&self) -> u64 {
        self.at_ns
    }
}

/// One LIVE transaction whose bytes may have left: what reconciliation
/// (responses, receipts, inclusion) resolves. Never sent again. Its frames
/// (they may carry an authorization) never show in `Debug`.
pub struct Released {
    pub tx_hash: [u8; 32],
    pub kind: LiveIntentKind,
    pub component: ComponentId,
    pub token: [u8; 20],
    pub nonce: u64,
    /// The sender and chain of that nonce.
    pub origin: NonceOrigin,
    /// The signed native value, and with the gas limit at the signed price
    /// the most it can cost.
    pub input_wei: U256,
    pub max_wei: U256,
    /// Whether a builder took it whole; otherwise UNKNOWN.
    pub handed_off: bool,
    pub report: Option<FanoutReport>,
    pub frames: Vec<Frame>,
}

impl core::fmt::Debug for Released {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Released")
            .field("tx_hash", &self.tx_hash)
            .field("kind", &self.kind)
            .field("component", &self.component)
            .field("handed_off", &self.handed_off)
            .field("frames", &self.frames.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
thread_local! {
    /// Tests: the address this thread's builder connections go to (a local
    /// listener) instead of the documented hosts'; name and policy stay.
    pub(crate) static BUILDERS_AT: std::cell::Cell<Option<std::net::SocketAddr>> =
        const { std::cell::Cell::new(None) };
}

/// A builder's documented endpoint.
fn builder_endpoint(target: &ProviderTarget) -> TlsEndpoint {
    let endpoint = target.tls_endpoint().clone();
    #[cfg(test)]
    if let Some(address) = BUILDERS_AT.with(std::cell::Cell::get) {
        return endpoint.with_address(address);
    }
    endpoint
}

/// Where the adapters' connections begun by [`LiveEnd::begin_connect`]
/// stand (BSC3-025).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Connecting {
    /// Every one is ready.
    Ready,
    /// One at least is still being established; none failed.
    Pending,
    /// These failed, and why.
    Failed(Vec<(LiveAdapter, String)>),
}

/// The armed LIVE end.
pub struct LiveEnd {
    key: LiveKey,
    armed: LiveNow,
    store: Store,
    qualification: Verified,
    table: AdapterTable,
    slots: Vec<Slot>,
    /// Unix milliseconds, the owner's clock for each admission.
    wall_ms: Box<dyn Fn() -> u64 + Send>,
    released: Vec<Released>,
    last_refusal: Option<String>,
    /// The LIVE ledger under the owner's state, the facts this end wrote
    /// there and why the last one could not be written.
    ledger: Ledger,
    facts: u64,
    ledger_failure: Option<String>,
    responses: Responses,
    /// Why a provider refused one of this end's BUYs (REJECT, 429): no
    /// further BUY goes through this end (BSC3-051).
    buy_refused: Option<String>,
    /// The answers read and not yet written to the ledger: a poll only
    /// reads, the write waits until no hand-over is near.
    answered: Vec<Fact>,
    /// REV.1.0 R2: the answers read and not yet journaled, with their times.
    timed: Vec<Answer>,
    /// The chain this end signs for.
    chain_id: u64,
    /// BSC3-076: the next C3 BUY the owner admitted before its launch.
    lease: Option<Lease>,
    /// BSC3-056: whether the second source confirms the followed chain
    /// (the runtime's verdict of its last pass); `None` without one.
    confirmation: Option<crate::canonical_confirm::Confirmation>,
}

/// BSC3-076: the owner's admission of the next C3 BUY taken before its
/// launch ([`Store::lease_live`]): its nonce, the most it may cost, its
/// exit reserve and window, the owner's state and the UTC day it was taken
/// in (that day's slot is its own) and its spend entry.
#[derive(Clone, Debug)]
pub struct Lease {
    attempt: [u8; 32],
    entry: [u8; 32],
    day: BuyDay,
    nonce: u64,
    origin: NonceOrigin,
    input_wei: u128,
    max_wei: u128,
    exit_reserve_wei: u128,
    max_block: u64,
    mark: OwnerMark,
}

impl Lease {
    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    pub fn max_block(&self) -> u64 {
        self.max_block
    }

    /// The attempt its slots are taken under.
    pub fn attempt(&self) -> [u8; 32] {
        self.attempt
    }
}

/// What the next C3 BUY's lease must cover ([`LiveEnd::hold_lease`]).
#[derive(Clone, Copy, Debug)]
pub struct LeaseWant {
    pub nonce: u64,
    pub input_wei: u128,
    pub max_wei: u128,
    pub exit_reserve_wei: u128,
    /// The canonical head now.
    pub head: u64,
    /// The bundle window of a BUY (`BSC_EXEC_VALIDITY_BLOCKS`).
    pub validity_blocks: u64,
    /// The blocks one lease holds (`BSC_EXEC_LIVE_LEASE_BLOCKS`).
    pub lease_blocks: u64,
    /// The attempt of a lease returned unused before (its slots taken).
    pub attempt: Option<[u8; 32]>,
}

#[cfg(test)]
thread_local! {
    /// Set once a hand-over took its lease (a test's clock turns late then).
    pub(crate) static LEASE_BOUND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// 32 bytes of the operating system's randomness: a lease's name.
fn random32() -> Result<[u8; 32], String> {
    use std::io::Read;
    let mut b = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .map_err(|e| format!("randomness: {e}"))?;
    Ok(b)
}

impl core::fmt::Debug for LiveEnd {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LiveEnd")
            .field("key", &self.key)
            .field("grant_generation", &self.armed.grant_generation())
            .field("released", &self.released.len())
            .finish_non_exhaustive()
    }
}

fn wei(value: U256) -> Option<u128> {
    u128::try_from(value).ok()
}

impl LiveEnd {
    /// The LIVE end of the grant `armed`: one adapter slot per granted
    /// builder, each qualified by `qualification` (else refused), its
    /// connection over `route` not yet established.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        key: LiveKey,
        armed: LiveNow,
        store: Store,
        qualification: Verified,
        route: LiveRoute,
        chain_id: u64,
        capability_generation: u64,
        wall_ms: Box<dyn Fn() -> u64 + Send>,
    ) -> Result<Self, String> {
        let grant = armed.grant().clone();
        // BSC3-049: the acquisition proven is this key's.
        if qualification.acquisition_buyer != Some(key.address()) {
            return Err(format!(
                "{}: the proven acquisition's buyer is not this key",
                crate::sources::LIVE_ACQUISITION_UNPROVEN
            ));
        }
        let mut bindings = Vec::with_capacity(grant.adapters.len());
        let mut slots = Vec::with_capacity(grant.adapters.len());
        for (index, live) in grant.adapters.iter().enumerate() {
            let adapter = u8::try_from(index)
                .ok()
                .and_then(AdapterId::new)
                .ok_or("more granted adapters than adapter slots")?;
            let id = capability(*live);
            if !qualification.view.adapters.contains(live)
                || !qualification.adapters.iter().any(|q| q.id() == id)
            {
                return Err(format!(
                    "{}: {}",
                    crate::sources::LIVE_ADAPTER_UNQUALIFIED,
                    live.name()
                ));
            }
            bindings.push(AdapterBinding {
                adapter,
                capability: id,
            });
            let e = |e| format!("{}: {e:?}", live.name());
            let target = match live {
                LiveAdapter::Club48BuilderV1 => ProviderTarget::club48(
                    TlsEndpoint::new(CLUB48_DOCUMENTED_HOST, HTTPS_PORT).map_err(e)?,
                ),
                LiveAdapter::BlockRazorBuilderV1 => {
                    let token = match &route {
                        LiveRoute::Builders { blockrazor, .. } => blockrazor.clone(),
                        // The stand-in takes the frame as BlockRazor's;
                        // no credential reaches it.
                        LiveRoute::Loopback { .. } => {
                            AuthorizationToken::new("loopback".into()).ok()
                        }
                    }
                    .ok_or("BlockRazor needs its authorization")?;
                    ProviderTarget::blockrazor(
                        TlsEndpoint::new(BLOCKRAZOR_DOCUMENTED_HOST, HTTPS_PORT).map_err(e)?,
                        token,
                    )
                }
            };
            let link = match &route {
                LiveRoute::Builders { setup, .. } => Link::Tls(Box::new(
                    TlsConnectionOwner::new(
                        builder_endpoint(&target),
                        setup.clone(),
                        Q5A_TLS_POLICY,
                    )
                    .map_err(|e| format!("{}: {e:?}", live.name()))?,
                )),
                LiveRoute::Loopback { port } => Link::Loopback(LoopbackTransport::new(*port)),
            };
            slots.push(Slot {
                adapter,
                live: *live,
                target,
                link,
                awaiting: None,
                buffer: None,
            });
        }
        if slots.is_empty() {
            return Err("a LIVE grant names at least one adapter".into());
        }
        let table = AdapterTable::new(chain_id, capability_generation, &bindings)
            .map_err(|e| e.to_string())?;
        let ledger = Ledger::open(store.dir());
        Ok(LiveEnd {
            key,
            armed,
            store,
            qualification,
            table,
            slots,
            wall_ms,
            released: Vec::new(),
            last_refusal: None,
            ledger,
            facts: 0,
            ledger_failure: None,
            responses: Responses::default(),
            buy_refused: None,
            answered: Vec::new(),
            timed: Vec::new(),
            chain_id,
            lease: None,
            confirmation: None,
        })
    }

    /// BSC3-056: the runtime's verdict of the second source (`None`: no
    /// second source is configured).
    pub fn set_confirmation(&mut self, verdict: Option<crate::canonical_confirm::Confirmation>) {
        self.confirmation = verdict;
    }

    /// BSC3-076: the lease held.
    pub fn lease(&self) -> Option<&Lease> {
        self.lease.as_ref()
    }

    /// BSC3-076: holds a lease of the owner for the next C3 BUY as `want`
    /// asks, outside the entry path: the one held if it covers it and its
    /// window has half its blocks left and the owner's state is as it was;
    /// otherwise a new one under the same attempt (a renewal or another
    /// nonce: no second slot), then the one it replaces returned unused.
    pub fn hold_lease(&mut self, want: &LeaseWant) -> Result<(), String> {
        if let Some(l) = &self.lease {
            let covers = l.nonce == want.nonce
                && l.input_wei >= want.input_wei
                && l.max_wei >= want.max_wei
                && l.exit_reserve_wei >= want.exit_reserve_wei;
            let left = want
                .head
                .saturating_add(want.validity_blocks)
                .saturating_add(want.lease_blocks / 2)
                <= l.max_block;
            // Review M1: a lease holds only its own UTC day's slot; the next
            // day's BUY takes that day's.
            let today = BuyDay::at((self.wall_ms)()) == l.day;
            if covers && left && today && self.store.owner_mark().ok() == Some(l.mark) {
                return Ok(());
            }
        }
        let attempt = match (&self.lease, want.attempt) {
            (Some(l), _) => l.attempt,
            (None, Some(a)) => a,
            (None, None) => random32()?,
        };
        let id = random32()?;
        let max_block = want
            .head
            .checked_add(want.lease_blocks)
            .ok_or("the lease window overflows")?;
        let origin = NonceOrigin {
            chain_id: self.chain_id,
            sender: self.key.address(),
        };
        // The owner's state before the admission reads it: a request applied
        // in between leaves another mark at the launch (refused there).
        let mark = self.store.owner_mark()?;
        let now_ms = (self.wall_ms)();
        self.store.lease_live(
            &self.armed,
            &LiveIntent {
                id: &alloy_primitives::hex::encode(id),
                component: ComponentId::ExecC3,
                kind: LiveIntentKind::Buy,
                input_wei: want.input_wei,
                max_wei: want.max_wei,
                token: [0; 20],
                nonce: want.nonce,
                origin,
                max_block,
                exit_reserve_wei: want.exit_reserve_wei,
            },
            &alloy_primitives::hex::encode(attempt),
            &self.qualification.view,
            now_ms,
        )?;
        let replaced = self.lease.replace(Lease {
            attempt,
            entry: id,
            day: BuyDay::at(now_ms),
            nonce: want.nonce,
            origin,
            input_wei: want.input_wei,
            max_wei: want.max_wei,
            exit_reserve_wei: want.exit_reserve_wei,
            max_block,
            mark,
        });
        if let Some(old) = replaced {
            self.store.return_leases(&[old.entry])?;
        }
        Ok(())
    }

    /// BSC3-076: returns the lease held unused (an exit or Strategy C's BUY
    /// takes its nonce, or the arm ends): its attempt, for the next lease.
    /// Not returned (a write failed), it counts on; it is never used again.
    pub fn return_lease(&mut self) -> Result<Option<[u8; 32]>, String> {
        let Some(l) = self.lease.take() else {
            return Ok(None);
        };
        self.store.return_leases(&[l.entry])?;
        Ok(Some(l.attempt))
    }

    /// BSC3-076: the lease held for the launch's C3 BUY at `nonce` of
    /// `origin`, taken: in memory and one look at the owner's state, no
    /// read or write of a file. Refused (none, not this BUY's, the owner's
    /// state changed, the grant expired), it stays held.
    #[allow(clippy::too_many_arguments)]
    fn lease_for(
        held: &mut Option<Lease>,
        store: &Store,
        armed: &LiveNow,
        now_ms: u64,
        nonce: u64,
        origin: NonceOrigin,
        (input_wei, max_wei, exit_reserve_wei): (u128, u128, u128),
        last_block: u64,
    ) -> Result<Lease, String> {
        use crate::sources::{LIVE_GRANT_EXPIRED, LIVE_LEASE_MISSING, LIVE_LEASE_STALE};
        let Some(l) = held.as_ref() else {
            return Err(LIVE_LEASE_MISSING.into());
        };
        if armed.grant().expires_unix_ms <= now_ms {
            return Err(LIVE_GRANT_EXPIRED.into());
        }
        let stale = |what: &str| Err(format!("{LIVE_LEASE_STALE}: {what}"));
        if l.nonce != nonce || l.origin != origin {
            return stale("another nonce");
        }
        if input_wei > l.input_wei || max_wei > l.max_wei || exit_reserve_wei > l.exit_reserve_wei {
            return stale("less budget");
        }
        if last_block > l.max_block {
            return stale("its window is over");
        }
        if BuyDay::at(now_ms) != l.day {
            return stale("another UTC day");
        }
        if store.owner_mark().ok() != Some(l.mark) {
            return stale("the owner's state changed");
        }
        #[cfg(test)]
        LEASE_BOUND.with(|b| b.set(true));
        held.take().ok_or_else(|| LIVE_LEASE_MISSING.into())
    }

    /// Establishes every adapter's connection and waits for them (tools
    /// and tests; the runtime uses [`Self::begin_connect`]): which adapter
    /// failed and why.
    pub fn connect(&mut self, timeout: Duration) -> Vec<(LiveAdapter, String)> {
        let mut failed = self.begin_connect(timeout);
        for slot in &mut self.slots {
            if let Link::Tls(owner) = &mut slot.link
                && owner.connecting()
                && let Err(e) = owner.connect()
            {
                failed.push((slot.live, e.to_string()));
            }
        }
        failed
    }

    /// Begins every adapter's connection and returns at once (BSC3-025): a
    /// builder's TLS connection is established on a thread of its own, a
    /// loopback link (a local socket) here. Which adapter could not begin.
    pub fn begin_connect(&mut self, timeout: Duration) -> Vec<(LiveAdapter, String)> {
        let mut failed = Vec::new();
        for slot in &mut self.slots {
            let result = match &mut slot.link {
                Link::Tls(owner) => owner.begin_connect().map_err(|e| e.to_string()),
                Link::Loopback(t) => t.connect(timeout).map_err(|e| e.to_string()),
            };
            if let Err(e) = result {
                failed.push((slot.live, e));
            }
        }
        failed
    }

    /// Whether every adapter's connection begun by [`Self::begin_connect`]
    /// is ready, still being established, or failed (which and why);
    /// never waits.
    pub fn connected(&mut self) -> Connecting {
        use crate::submission::transport::{ConnectionState, Transport};
        let mut failed = Vec::new();
        let mut pending = false;
        for slot in &mut self.slots {
            match &mut slot.link {
                Link::Tls(owner) => match owner.poll_connect() {
                    None if owner.connecting() => pending = true,
                    Some(Err(e)) => failed.push((slot.live, e.to_string())),
                    // Installed now or before: ready, or not (review LOW-2).
                    None | Some(Ok(_)) if owner.state() != ConnectionState::Ready => {
                        failed.push((slot.live, format!("not ready: {:?}", owner.state())));
                    }
                    None | Some(Ok(_)) => {}
                },
                Link::Loopback(t) if t.state() != ConnectionState::Ready => {
                    failed.push((slot.live, "loopback link not ready".into()));
                }
                Link::Loopback(_) => {}
            }
        }
        match (failed.is_empty(), pending) {
            (false, _) => Connecting::Failed(failed),
            (true, true) => Connecting::Pending,
            (true, false) => Connecting::Ready,
        }
    }

    /// Installs every builder connection established meanwhile, never
    /// beginning one and never waiting (BSC3-025 review): before each
    /// decision, outside any hand-over, so a connection replaced while
    /// armed is usable as soon as its handshake completed, not only after
    /// the next maintenance pass. A failure is left to the maintenance.
    pub fn install_ready(&mut self) {
        for slot in &mut self.slots {
            if let Link::Tls(owner) = &mut slot.link
                && owner.connecting()
            {
                let _ = owner.poll_connect();
            }
        }
    }

    /// One maintenance pass outside any dispatch, never waiting for a
    /// connection (BSC3-025): a TLS owner probes, gives up and begins
    /// replacing its connection as its policy says, and installs one
    /// established meanwhile; a loopback link that is not ready connects
    /// again. Which adapter failed and why.
    pub fn maintain(&mut self, timeout: Duration) -> Vec<(LiveAdapter, String)> {
        use crate::submission::transport::{ConnectionState, Transport};
        let mut failed = Vec::new();
        for slot in &mut self.slots {
            let result = match &mut slot.link {
                Link::Tls(owner) => match owner.maintain() {
                    crate::submission::tls::Maintenance::Failed(e) => Err(e.to_string()),
                    _ => Ok(()),
                },
                Link::Loopback(t) if t.state() != ConnectionState::Ready => {
                    t.connect(timeout).map_err(|e| e.to_string())
                }
                Link::Loopback(_) => Ok(()),
            };
            if let Err(e) = result {
                failed.push((slot.live, e));
            }
        }
        failed
    }

    /// `BUILDERS` or `LOOPBACK`.
    pub fn route(&self) -> &'static str {
        match self.slots.first().map(|s| &s.link) {
            Some(Link::Loopback(_)) => "LOOPBACK",
            _ => "BUILDERS",
        }
    }

    /// The grant this end was armed under.
    pub fn armed(&self) -> &LiveNow {
        &self.armed
    }

    /// The SHA-256 of the LIVE qualification record this end was armed
    /// with (BSC3-012): every admission of the arm rests on it.
    pub fn qualification_sha256(&self) -> [u8; 32] {
        self.qualification.sha256
    }

    /// Every transaction whose bytes may have left, in dispatch order.
    pub fn released(&self) -> &[Released] {
        &self.released
    }

    /// The owner's or the release's last reason to refuse an intent.
    pub fn last_refusal(&self) -> Option<&str> {
        self.last_refusal.as_deref()
    }

    /// The ledger facts this end wrote (one per admitted intent).
    pub fn facts_written(&self) -> u64 {
        self.facts
    }

    /// Why the last admitted intent's fact could not be written: its
    /// admission entry recovers it once nothing is armed.
    pub fn ledger_failure(&self) -> Option<&str> {
        self.ledger_failure.as_deref()
    }

    /// The providers' answers to this end's releases so far (BSC3-051).
    pub fn responses(&self) -> Responses {
        self.responses
    }

    /// Why a provider refused one of this end's BUYs, if one did.
    pub fn buy_refused(&self) -> Option<&str> {
        self.buy_refused.as_deref()
    }

    /// REV.1.0 R2: the answers read since the last call, with their times
    /// (the runtime journals them as the attempts' records).
    pub fn take_answers(&mut self) -> Vec<Answer> {
        std::mem::take(&mut self.timed)
    }

    /// Writes the answers read since the last call as ledger facts
    /// (BSC3-051): outside any hand-over (file and directory syncs).
    pub fn record_responses(&mut self) {
        for fact in std::mem::take(&mut self.answered) {
            match self.ledger.append(&fact, (self.wall_ms)()) {
                Ok(_) => self.facts += 1,
                Err(e) => self.ledger_failure = Some(e),
            }
        }
    }

    fn refuse(&mut self, refusal: Refusal, why: String) -> Handed<LiveHandoff> {
        self.last_refusal = Some(why);
        Handed::refused(refusal)
    }
}

impl End for LiveEnd {
    type Handoff = LiveHandoff;
    const FENCE: (&'static str, [u8; 8]) = ("live", *b"DBLIVFN1");
    const CHAIN_ORDERED: bool = true;

    fn address(&self) -> [u8; 20] {
        self.key.address()
    }

    fn signer(&self) -> &Signer {
        self.key.signer()
    }

    /// BSC3-051: each connection awaiting the answer to a request of
    /// `lane` reads what arrived; a complete answer is recorded in the
    /// lane and the report, counted, and a ledger fact apart from what the
    /// chain makes of the transaction. A connection that no longer awaits
    /// it (given up after the policy's time, replaced) answered nothing. A
    /// BUY a provider refused closes this end's BUYs; nothing is sent again.
    fn poll_responses(&mut self, lane: &mut IntentLane, clock: &dyn Clock) {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let Some(at) = slot.awaiting else { continue };
            let Some(released) = self.released.get_mut(at) else {
                slot.awaiting = None;
                continue;
            };
            let Some(report) = released.report.as_mut() else {
                slot.awaiting = None;
                continue;
            };
            if report.ticket.is_none_or(|t| lane.state(t).is_err()) {
                continue;
            }
            let bound = match &mut slot.link {
                Link::Tls(owner) => response_transport(owner, &self.table, slot.adapter)
                    .ok()
                    .map(|q| q.into_parts().0),
                Link::Loopback(t) => loopback_response(t, &self.table, slot.adapter)
                    .ok()
                    .map(|(q, _)| q),
            };
            let generation = self.armed.grant_generation();
            let answered = |what: &str| Fact {
                kind: FactKind::Responded,
                tx_hash: released.tx_hash,
                intent: released.kind,
                component: released.component,
                token: released.token,
                nonce: released.nonce,
                grant_generation: generation,
                input_wei: None,
                max_wei: None,
                block: None,
                gas_wei: None,
                tokens: None,
                eth_wei: None,
                max_block: None,
                by_tx: None,
                response: Some(format!("{}:{what}", slot.live.name())),
                origin: Some(released.origin),
                launcher: None,
                placement: None,
                attempt: None,
                operator: None,
                source: None,
            };
            let (Some(transport), Some(buffer), Some(frame)) =
                (bound, slot.buffer.as_mut(), released.frames.get_mut(index))
            else {
                let fact = answered("NO_RESPONSE");
                slot.awaiting = None;
                // A part of an answer must not meet the next request.
                if let Some(buffer) = slot.buffer.as_mut() {
                    buffer.reset();
                }
                self.responses.no_response += 1;
                self.answered.push(fact);
                self.timed.push(Answer {
                    tx_hash: released.tx_hash,
                    adapter: slot.adapter,
                    outcome: "NO_RESPONSE",
                    at_ns: None,
                });
                continue;
            };
            let mut armed = ReleaseArmed { transport, frame };
            let Poll::Done(outcome) =
                poll_release_response(lane, &mut armed, buffer, clock, report)
            else {
                continue;
            };
            slot.awaiting = None;
            let r = &mut self.responses;
            match outcome {
                ResponseOutcome::Ack => r.ack += 1,
                ResponseOutcome::Reject { .. } => r.reject += 1,
                ResponseOutcome::RateLimited => r.rate_limited += 1,
                ResponseOutcome::Protocol(_) => r.protocol += 1,
                ResponseOutcome::Connection => r.connection += 1,
            }
            let refused = matches!(
                outcome,
                ResponseOutcome::Reject { .. } | ResponseOutcome::RateLimited
            );
            if refused && released.kind == LiveIntentKind::Buy && self.buy_refused.is_none() {
                self.buy_refused = Some(format!(
                    "{} {} 0x{}",
                    slot.live.name(),
                    outcome_name(outcome),
                    alloy_primitives::hex::encode(released.tx_hash)
                ));
            }
            let fact = answered(outcome_name(outcome));
            self.answered.push(fact);
            self.timed.push(Answer {
                tx_hash: released.tx_hash,
                adapter: slot.adapter,
                outcome: outcome_name(outcome),
                at_ns: report.adapter(slot.adapter).and_then(|r| r.t_response_ns),
            });
        }
    }

    fn hand_over(&mut self, h: Handover<'_>) -> Handed<LiveHandoff> {
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
            exit_reserve_wei,
            launcher,
        } = h;
        // BSC3-045: a fact the ledger did not take may be missing from the
        // book: nothing more is handed over (before the owner's admission:
        // no attempt, no spend).
        if let Some(e) = &self.ledger_failure {
            let why = format!("{}: {e}", crate::sources::LIVE_LEDGER_UNREADABLE);
            return self.refuse(Refusal::Owner, why);
        }
        let kind = match expected {
            ExpectedTx::Buy(_) => LiveIntentKind::Buy,
            ExpectedTx::Approve(_) => LiveIntentKind::Approve,
            ExpectedTx::Sell(_) => LiveIntentKind::Sell,
            ExpectedTx::Burn(_) => LiveIntentKind::Burn,
        };
        // BSC3-036: a canonical head older than the bound: the canonical
        // source is stalled or cut off; nothing is planned on it (before
        // the owner's admission: no attempt, no spend).
        let age_ms = (self.wall_ms)().saturating_sub(head.timestamp.saturating_mul(1_000));
        if age_ms > config.live_max_head_age_ms {
            let why = format!("the canonical head {} is {age_ms} ms old", head.number);
            return self.refuse(Refusal::StaleHead, why);
        }
        // BSC3-056: a BUY only on a chain the second source confirms (before
        // the owner's admission: no attempt, no spend; exits go on).
        // The verdict must cover the head the BUY is planned on (at most
        // its lag below it).
        if kind == LiveIntentKind::Buy
            && let Some(c) = self.confirmation
            && let Err(why) = c.confirms(head.number)
        {
            return self.refuse(Refusal::Unconfirmed, why);
        }
        // R7 (BSC3-056): the divergence quarantine (ON): while the second
        // source holds another block, nothing goes, exits neither.
        if config.live_divergence_quarantine
            && let Some(c) = self.confirmation
            && let crate::canonical_confirm::Verdict::Disagrees { height } = c.verdict
        {
            let why =
                format!("DIVERGENCE_QUARANTINE: the second source holds another block at {height}");
            return self.refuse(Refusal::Unconfirmed, why);
        }
        // BSC3-051: a provider refused one of this end's BUYs: no further
        // BUY through it (before the owner's admission; exits go on).
        if let Some(why) = self
            .buy_refused
            .as_ref()
            .filter(|_| kind == LiveIntentKind::Buy)
        {
            let why = format!("PROVIDER_REFUSED: {why}");
            return self.refuse(Refusal::Dispatch, why);
        }
        // BSC3-076: Strategy C's BUY and every exit act on a block, not in
        // the race after a launch's ingress.
        let block_driven = matches!(submission, Submission::Single);
        let component = match submission {
            Submission::LaunchBundle(_) => ComponentId::ExecC3,
            Submission::Single => ComponentId::StrategyC,
        };
        let profile = submission.profile();
        let ids: Vec<AdapterId> = self.slots.iter().map(|s| s.adapter).collect();
        // Audit E 4.2: the shared plan inputs, planned under the verified
        // LIVE qualification.
        let inputs = match plan_inputs(
            &self.table,
            &ids,
            ticket,
            raw,
            submission,
            head,
            config.validity_blocks,
        ) {
            Ok(inputs) => inputs,
            Err(e) => return self.refuse(Refusal::Plan, e.to_string()),
        };
        let last_block = inputs.local_block_deadline.last_head_block;
        let plan = match SubmissionPlan::new_with(
            &self.table,
            &self.qualification.adapters,
            expected,
            budget,
            inputs,
        ) {
            Ok(p) => p,
            Err(e) => return self.refuse(Refusal::Plan, format!("{e:?}")),
        };
        let release = match plan.qualify_for_release_with(&self.table, &self.qualification.adapters)
        {
            Ok(r) => r,
            Err(r) => return self.refuse(Refusal::Unqualified, format!("{:?}", r.blocker)),
        };
        let mut frames = Vec::with_capacity(self.slots.len());
        for slot in &self.slots {
            // Audit E 4.2: the shared framing, by the slot's kind in the table.
            let frame = adapter_frame(
                &plan,
                &self.table,
                slot.adapter,
                &slot.target.frame_endpoint(),
                submission.launch_raw(),
                config.max_frame,
            );
            match frame {
                Ok(f) => frames.push(f),
                Err(e) => return self.refuse(Refusal::Plan, e.to_string()),
            }
        }
        let deadline = match TransportStartDeadline::for_profile(
            profile,
            clock.id(),
            start_ns,
            config.start_limit_ns,
        ) {
            Ok(d) => d,
            Err(e) => return self.refuse(Refusal::Plan, format!("{e:?}")),
        };
        let planned_ns = clock.now_ns();
        let (Some(input_wei), Some(max_wei), Some(exit_reserve_wei)) = (
            wei(budget.max_buy_input_wei),
            wei(budget.max_total_wei),
            wei(exit_reserve_wei),
        ) else {
            return self.refuse(Refusal::Plan, "the budget exceeds 128 bits".into());
        };
        // BSC3-031: one allocation (was one `format!` per byte), the same
        // lowercase hex.
        let id = alloy_primitives::hex::encode(tx_hash);
        let mut qualifications = TransportQualifications::default();
        let mut transports = Vec::with_capacity(self.slots.len());
        let mut unqualified = None;
        for slot in &mut self.slots {
            let qualified = match &mut slot.link {
                Link::Tls(owner) => qualify_transport(owner, &self.table, slot.adapter)
                    .map(|q| q.into_parts())
                    .map_err(|e| e.to_string()),
                Link::Loopback(t) => {
                    qualify_loopback(t, &self.table, slot.adapter).map_err(|e| format!("{e:?}"))
                }
            };
            match qualified {
                Ok((transport, q)) => {
                    qualifications = qualifications.with(slot.adapter, q);
                    transports.push(transport);
                }
                Err(e) => {
                    unqualified = Some(format!("{}: {e}", slot.live.name()));
                    break;
                }
            }
        }
        if let Some(why) = unqualified {
            drop(transports);
            return self.refuse(Refusal::Unqualified, why);
        }
        // BSC3-068: the start deadline again, right before the owner's
        // admission: a late intent takes no attempt and no spend. The
        // dispatch checks it once more before its write; a miss in between
        // counts the attempt (its fact WITHDRAWN: no byte left).
        if let Err(e) = deadline.check(clock.id(), clock.now_ns()) {
            drop(transports);
            return self.refuse(Refusal::Deadline, format!("{e:?}"));
        }
        // The owner, as it holds now: nothing before this consumed an
        // attempt or a budget, and nothing after it can leave unadmitted.
        // Its entry, with the token and nonce, is the transaction's
        // write-ahead record (BSC3-024).
        let token = ticket.token().into_array();
        let nonce = ticket.nonce();
        // The nonce is the lane's, of its sender and chain: this key's.
        let origin = NonceOrigin {
            chain_id: ticket.chain_id(),
            sender: ticket.sender().into_array(),
        };
        if origin.sender != self.key.address() {
            drop(transports);
            return self.refuse(Refusal::Plan, "the lane's sender is not the key's".into());
        }
        // BSC3-076: a launch's BUY was admitted before its ingress, as a
        // lease: here only what is in memory and one look at the owner's
        // state; an intent on a block is admitted now (milliseconds).
        let admitted = match block_driven {
            true => self
                .store
                .admit_live(
                    &self.armed,
                    &LiveIntent {
                        id: &id,
                        component,
                        kind,
                        input_wei,
                        max_wei,
                        token,
                        nonce,
                        origin,
                        max_block: last_block,
                        exit_reserve_wei,
                    },
                    &self.qualification.view,
                    (self.wall_ms)(),
                )
                .map(|_| None),
            false => Self::lease_for(
                &mut self.lease,
                &self.store,
                &self.armed,
                (self.wall_ms)(),
                nonce,
                origin,
                (input_wei, max_wei, exit_reserve_wei),
                last_block,
            )
            .map(Some),
        };
        let lease = match admitted {
            Ok(lease) => lease,
            Err(why) => {
                drop(transports);
                return self.refuse(Refusal::Owner, why);
            }
        };
        // BSC3-076: the owner's durable admission (slot, spend, one flush)
        // takes milliseconds: an intent acting on a block starts its
        // transport within the limit of its admission, not of its decision
        // (checked above: a late decision took nothing). A launch's BUY was
        // admitted before its ingress and keeps the limit from there.
        // (A limit that could not be set from now keeps the one from the
        // decision: the dispatch then skips and the fact says so.)
        let deadline = match block_driven {
            true => TransportStartDeadline::for_profile(
                profile,
                clock.id(),
                clock.now_ns(),
                config.start_limit_ns,
            )
            .unwrap_or(deadline),
            false => deadline,
        };
        let mut armed: Vec<ReleaseArmed<'_>> = transports
            .into_iter()
            .zip(frames.iter_mut())
            .map(|(transport, frame)| ReleaseArmed { transport, frame })
            .collect();
        let dispatched = dispatch_release(
            lane,
            &release,
            &DispatchContext {
                table: &self.table,
                current: snapshot,
                head,
            },
            clock,
            &deadline,
            &qualifications,
            &mut armed,
        );
        drop(armed);
        let (admission, disclosed) = consumed_by(&dispatched);
        let result = match &dispatched {
            Err(e) => Err((Refusal::Dispatch, format!("{e:?}"))),
            Ok(report) => {
                let handed: Vec<u64> = ids
                    .iter()
                    .filter_map(|a| report.adapter(*a))
                    .filter(|r| r.outcome == SendOutcome::HandedOff)
                    .filter_map(|r| r.t_boundary_ns)
                    .collect();
                let late = ids
                    .iter()
                    .filter_map(|a| report.adapter(*a))
                    .any(|r| matches!(r.outcome, SendOutcome::SkippedDeadline(_)));
                match handed.iter().min() {
                    Some(at_ns) => Ok(LiveHandoff {
                        tx_hash,
                        at_ns: *at_ns,
                        adapters: handed.len() as u8,
                        attempt: lease.as_ref().map_or(tx_hash, |l| l.attempt),
                        report: *report,
                        builders: {
                            let mut b = [None; MAX_ADAPTERS];
                            for s in &self.slots {
                                b[s.adapter.index()] = Some(s.live);
                            }
                            b
                        },
                    }),
                    None if late && !disclosed => Err((
                        Refusal::Deadline,
                        "the transport start deadline passed".into(),
                    )),
                    None => Err((Refusal::Dispatch, "no builder took it whole".into())),
                }
            }
        };
        // The admitted intent's outcome, a ledger fact at once: off the
        // dispatch, before the executor goes on.
        let fact = Fact {
            kind: match (disclosed, result.is_ok()) {
                (false, _) => FactKind::Withdrawn,
                (true, true) => FactKind::Released,
                (true, false) => FactKind::Unknown,
            },
            tx_hash,
            intent: kind,
            component,
            token,
            nonce,
            grant_generation: self.armed.grant_generation(),
            input_wei: Some(budget.max_buy_input_wei),
            max_wei: Some(budget.max_total_wei),
            block: None,
            gas_wei: None,
            tokens: None,
            eth_wei: None,
            max_block: Some(last_block),
            by_tx: None,
            response: None,
            origin: Some(origin),
            launcher,
            placement: None,
            // R3 (BSC3-068): a BUY's attempt, its lease's own fact the same.
            attempt: (kind == LiveIntentKind::Buy)
                .then(|| lease.as_ref().map_or(tx_hash, |l| l.attempt)),
            operator: None,
            source: None,
        };
        let written = match self.ledger.append(&fact, (self.wall_ms)()) {
            Ok(_) => {
                self.facts += 1;
                true
            }
            Err(e) => {
                self.ledger_failure = Some(e);
                false
            }
        };
        // BSC3-076: the lease's entry was no transaction: a fact of its
        // own, only once the BUY's is written (a crash between, or the BUY's
        // not written, leaves it unresolved at the BUY's nonce: a restart
        // recovers it, never the BUY untracked).
        for entry in lease.iter().filter(|_| written).map(|l| l.entry) {
            let lease_fact = Fact {
                kind: FactKind::Withdrawn,
                tx_hash: entry,
                token: [0; 20],
                max_block: lease.as_ref().map(|l| l.max_block),
                ..fact.clone()
            };
            match self.ledger.append(&lease_fact, (self.wall_ms)()) {
                Ok(_) => self.facts += 1,
                Err(e) => self.ledger_failure = Some(e),
            }
        }
        if disclosed {
            self.released.push(Released {
                tx_hash,
                kind,
                component,
                token,
                nonce,
                origin,
                input_wei: budget.max_buy_input_wei,
                max_wei: budget.max_total_wei,
                handed_off: result.is_ok(),
                report: dispatched.as_ref().ok().copied(),
                frames,
            });
            // BSC3-051: each connection that took the request awaits its
            // answer, read into a buffer as large as the largest frame.
            let at = self.released.len() - 1;
            if let Some(report) = &self.released[at].report {
                for slot in &mut self.slots {
                    let handed = report
                        .adapter(slot.adapter)
                        .is_some_and(|r| r.outcome == SendOutcome::HandedOff);
                    if handed {
                        slot.awaiting = Some(at);
                        slot.buffer
                            .get_or_insert_with(|| ResponseBuffer::new(config.max_frame))
                            .reset();
                    }
                }
            }
        }
        let result = result.map_err(|(refusal, why)| {
            self.last_refusal = Some(why);
            refusal
        });
        Handed {
            result,
            planned_ns,
            admitted: admission,
            disclosed,
        }
    }
}

#[cfg(test)]
#[path = "live_end_tests.rs"]
pub(crate) mod tests;

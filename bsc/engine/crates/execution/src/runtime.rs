//! The BSC runtime process (C1, order V2 §5/§7.3): it observes the signal
//! sources the native source owner made effective, verifies and classifies
//! what they deliver in one [`IngressCore`], journals launch sightings and
//! source transitions, and rewrites its status record. Its PAPER paths sign
//! into the local PAPER sink and never send; its LIVE path (`live`) signs
//! and sends only while the owner's LIVE gate holds under the operator's
//! grant.
//!
//! With a canonical endpoint it also follows the canonical chain
//! (`chain_follower`) and joins every canonical launch with the sightings
//! (`canonical_join`): the G0.4 denominator. The census
//! (`bsc-runtime/g04-census.dbrec`) is replayed from the journals every
//! `census_ms` and at stop.
//!
//! Lifecycle: `run` until the stop file exists (written by `darkbotctl`),
//! then stops every adapter and the follower, joins the launches still
//! waiting, commits the journal and writes a final status and census.
//! A source without a configured endpoint, or without an adapter in this
//! build (the local geth Fast Lane needs a synced node), is `UNQUALIFIED`.
//!
//! With the component `BSC_STATE_C2` effective `SHADOW` (and a canonical
//! endpoint) the C2 chain-state owner ([`state`]) applies the canonical
//! inputs and the pending sightings of launch candidates and watched
//! senders. G0.4 counts only blocks mined after this process started; the
//! blocks a C2 resume backfills are counted apart.

use crate::canonical_join::{LaunchRef, LaunchTx, Ledger, Venue};
use crate::chain_follower::{
    CanonicalEvent, FollowerFault, FollowerLimits, NonceWatch, RequestError, is_unrebuildable,
    run_follower,
};
use crate::g04_census::{Census, census};
use crate::ingress::{Health, IngressCore, Launch, Observation, SourceStats};
use crate::paper_exec::{ExecConfig, FnClock, max_start_limit_ns, max_validity_blocks};
use crate::runtime_journal::{
    CaptureScope, CaptureStatus, EXIT_PAYLOAD, JournalSetup, RunIdentity, RuntimeJournal,
    STOP_PAYLOAD, SourceEvent, canonical_launch_payload, first_sighting_payload,
    flap_migrated_payload, flap_term_payload, flap_trade_payload, ingress_drop_payload,
    later_sighting_payload, launch_id_payload, orphaned_launch_payload, own_binary_sha256,
    paper_handoff_payload, pending_launch_payload, reorg_payload, resync_payload,
    source_event_payload, trade_gap_payload, window_payload,
};
use crate::sources::{ComponentId, Effective, Generation, SourceId, Store, VenueId};
use crate::wss::endpoint::Endpoint;
use crate::wss::{DisconnectReason, WssEvent, WssLimits, WssSource, run_source};
use darkbot_bsc_geth_ipc::protocol_v2::CLOCK_ID_MONOTONIC;
use darkbot_bsc_journal::RecordKind;
use darkbot_bsc_journal::storage::segments::SegmentPolicy;
use darkbot_bsc_journal::storage::writer::SyncPolicy;
use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
use darkbot_bsc_record::{Draft, observed_now};
use darkbot_bsc_trading::chain_state::engine::StateLimits;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, SyncSender, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[path = "runtime_state.rs"]
pub mod state;

#[path = "runtime_exec.rs"]
pub mod exec;

#[path = "runtime_live.rs"]
pub mod live;

// Audit E 5.13: the configuration and the canonical side, each its own file.
#[path = "runtime_config.rs"]
mod config;
pub use config::*;

#[path = "runtime_canonical.rs"]
mod canonical;
use canonical::*;

use exec::{ExecRuntime, ExecSetup};
use live::LiveRuntime;
use state::{CheckpointWriter, StateConfig, StateOwner};

/// REV.1.0 R6: the ingress queues, one bounded queue per source
/// (`BSC_RUNTIME_QUEUE_EVENTS` events each; was one queue shared by all):
/// a source's burst fills and drops only its own queue, never another
/// source's events (a burst still delays the others: one consumer). The
/// runtime takes the event received first among the queues' heads: receipt
/// order (the shared queue kept enqueue order, after each source's parse),
/// each source's own order kept.
struct IngressQueues {
    state: std::sync::Mutex<Queued>,
    ready: std::sync::Condvar,
    bound: usize,
}

struct Queued {
    queues: [std::collections::VecDeque<(u64, Event)>; SourceId::ALL.len()],
    /// The runtime waits for an event: a push wakes it (only then).
    waiting: bool,
}

impl IngressQueues {
    fn new(bound: usize) -> Arc<Self> {
        Arc::new(Self {
            state: std::sync::Mutex::new(Queued {
                queues: std::array::from_fn(|_| std::collections::VecDeque::new()),
                waiting: false,
            }),
            ready: std::sync::Condvar::new(),
            bound,
        })
    }

    /// Queues `event` of `source`, received at `at`; false when that
    /// source's queue is full (the event is dropped).
    fn push(&self, source: SourceId, at: u64, event: Event) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let queue = &mut state.queues[source.index()];
        if queue.len() >= self.bound {
            return false;
        }
        queue.push_back((at, event));
        let wake = state.waiting;
        drop(state);
        if wake {
            self.ready.notify_one();
        }
        true
    }

    /// The event received first among the queues' heads, waiting at most
    /// `wait` for one.
    fn pop(&self, wait: Duration) -> Option<(SourceId, u64, Event)> {
        let mut deadline = None;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            let first = state
                .queues
                .iter()
                .enumerate()
                .filter_map(|(i, q)| q.front().map(|(at, _)| (*at, i)))
                .min();
            if let Some((_, i)) = first {
                let (at, event) = state.queues[i].pop_front()?;
                return Some((SourceId::ALL[i], at, event));
            }
            let deadline = *deadline.get_or_insert_with(|| Instant::now() + wait);
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            state.waiting = true;
            state = self
                .ready
                .wait_timeout(state, left)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
            state.waiting = false;
        }
    }
}

/// One source event as it crosses from an adapter thread to the runtime.
enum Event {
    Full {
        hash: [u8; 32],
        raw: Vec<u8>,
        /// The verified signer and nonce (the C2 pending input).
        from: [u8; 20],
        nonce: u64,
    },
    HashOnly {
        hash: [u8; 32],
    },
    Rejected,
    Connected,
    /// R6 review HIGH: the local geth's arena is still open.
    Alive,
    Disconnected {
        rate_limited: bool,
    },
}

struct Adapter {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

/// Every LIVE transaction a Strategy C decision handed over, journaled.
fn journal_live(
    journal: &mut RuntimeJournal,
    intents: &[crate::paper_exec::ExecIntent<crate::live_end::LiveHandoff>],
) {
    for i in intents {
        journal.record(
            RecordKind::Runtime,
            i.times.sink_ns,
            crate::runtime_journal::live_release_payload(
                i.kind.code(),
                None,
                &i.handoff,
                [0; 32],
                i.token,
                i.block,
                &i.times,
                &i.raw,
            ),
        );
    }
}

/// REV.1.0 R3b: at a block's complete state, before any decision takes the
/// next nonce, the operator's nonce burn when one is wanted and the LIVE
/// path is armed ([`LiveRuntime::burn`]), journaled as its release.
fn burn_nonce(
    live: &mut Option<LiveRuntime>,
    owner: &Option<StateOwner>,
    clock: Clock,
    journal: &mut RuntimeJournal,
) {
    let (Some(l), Some(o)) = (live.as_mut(), owner.as_ref()) else {
        return;
    };
    if !l.armed() {
        return;
    }
    let now_unix_ms = wall_or_expired(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| u64::try_from(d.as_millis()).ok())
            .ok_or_else(|| "wall clock before 1970".to_string()),
    );
    let clk = FnClock {
        id: CLOCK_ID_MONOTONIC,
        now: || clock.now_ns(),
    };
    let intents = l.burn(o, &clk, now_unix_ms);
    journal_live(journal, &intents);
}

/// Phase I: Strategy C's decisions of the records just applied go to the
/// armed C3 executor (`BSC_EXEC` ARMED), on C2's canonical snapshot; each
/// PAPER transaction at the sink is journaled (`J`). Without an armed
/// executor they are counted (`paper_unarmed`), never executed.
fn strategy_to_paper(
    c: &mut Canonical,
    exec: &mut Option<ExecRuntime>,
    live: &mut Option<LiveRuntime>,
    owner: &Option<StateOwner>,
    clock: Clock,
    journal: &mut RuntimeJournal,
    block: u64,
) {
    let Some(s) = c.strategy.as_mut() else {
        return;
    };
    // Phase I gate M1: the moment decisions are acted on at the block's
    // complete state (SHADOW or PAPER), so a replay places them there.
    if s.has_pending() {
        journal.record(
            RecordKind::Runtime,
            clock.now_ns(),
            crate::runtime_strategy::action_payload(block),
        );
    }
    // Gate L-a: the head's age against the wall clock now; unreadable, every
    // head is too old.
    let now_unix_ms = wall_or_expired(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| u64::try_from(d.as_millis()).ok())
            .ok_or_else(|| "wall clock before 1970".to_string()),
    );
    let clk = FnClock {
        id: CLOCK_ID_MONOTONIC,
        now: || clock.now_ns(),
    };
    // BSC3-055: a LIVE exit whose approval is canonical now sells.
    if let (Some(l), Some(o)) = (live.as_mut(), owner.as_ref())
        && l.armed()
    {
        for token in l.exits_to_continue(o) {
            let Some(action) = s.exit_action(token, clock.now_ns()) else {
                continue;
            };
            if let Some((intents, _refused)) = l.on_strategy(&action, o, &clk, now_unix_ms) {
                journal_live(journal, &intents);
            }
        }
    }
    let mut actions = s.take_actions();
    // BSC3-048: on the LIVE path a block's exits go before its BUYs (a
    // stable order otherwise; PAPER keeps the decisions' order).
    if live.as_ref().is_some_and(LiveRuntime::armed) {
        actions.sort_by_key(|a| !matches!(a, crate::runtime_strategy::StrategyAction::Exit { .. }));
    }
    for action in actions {
        // Plan 28.10.: the LIVE path takes the decision when it is armed.
        if let (Some(l), Some(o)) = (live.as_mut(), owner.as_ref())
            && l.armed()
        {
            if let Some((intents, _refused)) = l.on_strategy(&action, o, &clk, now_unix_ms) {
                journal_live(journal, &intents);
            }
            continue;
        }
        // Review of R4: a LIVE lot's exit waits for the LIVE path's next
        // arm, never PAPER's.
        if let (Some(l), crate::runtime_strategy::StrategyAction::Exit { token, .. }) =
            (live.as_mut(), &action)
            && l.want_unarmed_exit(*token)
        {
            continue;
        }
        let handed = match (exec.as_mut(), owner.as_ref()) {
            (Some(x), Some(o)) => x.on_strategy(
                &action,
                o,
                &FnClock {
                    id: CLOCK_ID_MONOTONIC,
                    now: || clock.now_ns(),
                },
                now_unix_ms,
            ),
            _ => None,
        };
        match handed {
            // Every transaction at the sink is journaled, an exit's
            // approval also when its SELL was refused (refusals are the
            // executor's counters).
            Some((intents, _refused)) => {
                for i in &intents {
                    journal.record(
                        RecordKind::Runtime,
                        i.times.sink_ns,
                        crate::runtime_strategy::paper_intent_payload(i),
                    );
                }
            }
            None => s.counters.paper_unarmed += 1,
        }
    }
}

/// The owner's LIVE gate inputs of one reconcile pass (plan 28.10.): what
/// a requested LIVE reads as now. Without a LIVE path configured nothing
/// is read or qualified, so a requested LIVE stays BLOCKED.
struct LiveView {
    boot: Option<String>,
    now_ms: u64,
    daily: Option<crate::sources::BuyUsage>,
    total: Option<crate::sources::TotalUsage>,
    spent: Option<u128>,
    qualification: crate::sources::LiveQualification,
}

impl LiveView {
    /// The reconcile's view: the owner state besides the attempt `live`'s
    /// next C3 BUY takes under the grant of `g`.
    fn of(
        g: &Generation,
        state: &Path,
        qualification: Option<&crate::sources::LiveQualification>,
        now_ms: u64,
        live: Option<&LiveRuntime>,
        max: u32,
    ) -> Self {
        let held = live
            .and_then(|l| l.held_attempt(g.live_set_by.as_ref().map(|by| by.generation)))
            .map(alloy_primitives::hex::encode);
        Self::read(g, state, qualification, now_ms, held.as_deref(), max)
    }

    /// The owner state as it holds now; `qualification` is the reconcile's
    /// one verified LIVE qualification (BSC3-050: verified once per
    /// reconcile, the same result the LIVE path ticks with); `held` the
    /// attempt the LIVE path's next C3 BUY takes (BSC3-076 review H1: its
    /// own slots refuse nothing, as at the LIVE path's gate).
    fn read(
        g: &Generation,
        state: &Path,
        qualification: Option<&crate::sources::LiveQualification>,
        now_ms: u64,
        held: Option<&str>,
        max: u32,
    ) -> Self {
        let Some(qualification) = qualification else {
            return LiveView {
                boot: None,
                now_ms,
                daily: None,
                total: None,
                spent: None,
                qualification: Default::default(),
            };
        };
        let (daily, total) = crate::sources::usage_besides(state, now_ms, held, max);
        LiveView {
            boot: crate::sources::boot_session().ok(),
            now_ms,
            daily,
            total,
            spent: match &g.live_set_by {
                Some(by) => crate::sources::spent_by_grant(state, by.generation).ok(),
                None => Some(0),
            },
            qualification: qualification.clone(),
        }
    }

    /// What the owner makes effective of `e` now. While the LIVE path holds
    /// positions (`exits`), judged as for their exits (BSC3-058/071): the
    /// strategy that decides them, C2 and the sources stay on under a
    /// grant that admits no BUY (EXIT-ONLY, BUYs used up); the LIVE path
    /// and the owner still refuse every BUY.
    fn effective(&self, g: &Generation, e: crate::sources::Entity, exits: bool) -> Effective {
        let ctx = crate::sources::LiveContext {
            boot: self.boot.as_deref(),
            now_unix_ms: self.now_ms,
            daily: self.daily.as_ref(),
            total: self.total.as_ref(),
            spent_wei: self.spent,
            qualification: &self.qualification,
        };
        match exits {
            true => g.effective_exit_now(e, &ctx),
            false => g.effective_live_now(e, &ctx),
        }
    }
}

/// Monotonic nanoseconds since the runtime started (one epoch for all).
#[derive(Clone, Copy)]
struct Clock(Instant);

impl Clock {
    fn now_ns(self) -> u64 {
        self.ns_at(Instant::now())
    }

    /// `at` on this clock (an instant before its start reads 0).
    fn ns_at(self, at: Instant) -> u64 {
        u64::try_from(at.saturating_duration_since(self.0).as_nanos()).unwrap_or(u64::MAX)
    }
}

/// A monotonic reading and the wall clock (µs since the Unix epoch) read
/// next to it; refreshed at least once per pass, so mapping a fresh
/// monotonic time through it carries at most one pass of clock drift.
/// `None` when the wall clock could not be read (never 0).
#[derive(Clone, Copy)]
struct WallPair(Option<(u64, u64)>);

impl WallPair {
    fn read(clock: Clock) -> Self {
        let mono = clock.now_ns();
        let wall = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| u64::try_from(d.as_micros()).ok())
            .filter(|w| *w > 0);
        WallPair(wall.map(|w| (mono, w)))
    }

    /// The wall-clock estimate (µs) of the monotonic time `at_ns`.
    fn wall_us(self, at_ns: u64) -> Option<u64> {
        let (mono, wall) = self.0?;
        if at_ns >= mono {
            wall.checked_add((at_ns - mono) / 1_000)
        } else {
            wall.checked_sub((mono - at_ns) / 1_000)
        }
    }
}

/// Which paths the effective modes let run (plan v9 §2.3/§2.5, Phase H;
/// the §3b B cases).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Gates {
    /// Flap launches reach the C3 PAPER path and Strategy C only while the
    /// Flap venue is effective SHADOW (or LIVE).
    pub flap_admitted: bool,
    /// R12c: Four.meme Classic launches reach the C3 PAPER path only while
    /// that venue is effective SHADOW (its LIVE stays blocked).
    pub four_admitted: bool,
    /// R12d: OpenFour launches likewise, while the OpenFour venue is.
    pub openfour_admitted: bool,
    /// BSC_STRATEGY_C runs (SHADOW or LIVE) under a runtime mode of SHADOW,
    /// PAPER or LIVE, with a venue admitted; switched off, its ledger ends.
    pub strategy_on: bool,
    /// SC4: the venues whose launches it follows (each admitted).
    pub strategy_venues: crate::runtime_strategy::StrategyVenues,
    /// The C3 PAPER path: a venue admitted, runtime mode PAPER and
    /// BSC_EXEC SHADOW (its own effective mode, as the owner stores it).
    pub c3_wanted: bool,
    /// The C2 chain-state owner.
    pub c2_on: bool,
}

/// [`Gates`] of the effective modes `eff` and BSC_EXEC's stored effective
/// mode `exec`.
pub(crate) fn gates(eff: impl Fn(crate::sources::Entity) -> Effective, exec: Effective) -> Gates {
    let running = |e: Effective| matches!(e, Effective::Shadow | Effective::Live);
    let flap_admitted = running(eff(VenueId::Flap.into()));
    let four_admitted = running(eff(VenueId::FourClassic.into()));
    let openfour_admitted = running(eff(VenueId::OpenFour.into()));
    let runtime_mode = eff(crate::sources::Entity::Runtime);
    let strategy_venues = crate::runtime_strategy::StrategyVenues {
        flap: flap_admitted,
        four: four_admitted,
        openfour: openfour_admitted,
    };
    Gates {
        flap_admitted,
        four_admitted,
        openfour_admitted,
        strategy_venues,
        strategy_on: strategy_venues.any()
            && matches!(
                runtime_mode,
                Effective::Shadow | Effective::Paper | Effective::Live
            )
            && running(eff(ComponentId::StrategyC.into())),
        c3_wanted: (flap_admitted || four_admitted || openfour_admitted)
            && runtime_mode == Effective::Paper
            && exec == Effective::Shadow,
        c2_on: running(eff(ComponentId::StateC2.into())),
    }
}

impl Gates {
    /// Whether a first sighting on `venue` goes to the C3 PAPER path: its
    /// venue admitted.
    pub(crate) fn c3_admits(&self, venue: Venue) -> bool {
        match venue {
            Venue::Flap => self.flap_admitted,
            Venue::Four => self.four_admitted,
            Venue::OpenFour => self.openfour_admitted,
        }
    }
}

/// Which Four.meme BUYs the armed C3 path holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FourArmed {
    pub classic: bool,
    /// R12d.
    pub openfour: bool,
}

/// SC4: what a reconcile does with Strategy C.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StrategySwitch {
    /// Not running and not wanted: nothing to journal.
    Off,
    /// Wanted, not running: switched on with the admitted venues.
    Start,
    /// Running on other venues than those admitted now: off, then on.
    Restart,
    /// Running, not wanted: switched off.
    Stop,
    /// Wanted, its gas evidence refused: not started.
    Refused,
    /// Running on the admitted venues.
    Keep,
}

/// SC4: the switch of Strategy C for `on` (its gates), the venues it runs on
/// (`running`) and those admitted now, its gas evidence loaded or not.
pub(crate) fn strategy_switch(
    on: bool,
    running: Option<crate::runtime_strategy::StrategyVenues>,
    venues: crate::runtime_strategy::StrategyVenues,
    gas_ok: bool,
) -> StrategySwitch {
    match (on, running) {
        (false, None) => StrategySwitch::Off,
        (false, Some(_)) => StrategySwitch::Stop,
        (true, None) if gas_ok => StrategySwitch::Start,
        (true, None) => StrategySwitch::Refused,
        (true, Some(v)) if v != venues && gas_ok => StrategySwitch::Restart,
        (true, Some(_)) => StrategySwitch::Keep,
    }
}

/// Whether a venue's sighting may reach the C3 PAPER path: admitted by the
/// last reconcile's `gates`, and (R12c) a Four Classic one only while the
/// path holds the Four Classic BUY, (R12d) an OpenFour one only while it
/// holds the OpenFour BUY (`armed`).
fn c3_takes(venue: Venue, gates: Option<Gates>, armed: FourArmed) -> bool {
    gates.is_some_and(|g| g.c3_admits(venue))
        && match venue {
            Venue::Flap => true,
            Venue::Four => armed.classic,
            Venue::OpenFour => armed.openfour,
        }
}

/// Whether a verified sighting is a C2 pending candidate: every Flap
/// launch; (R12c/R12d) a Four one the C3 path may take (its BUY needs C2's
/// snapshot of it).
pub(crate) fn c2_candidate(
    decided: Option<Launch>,
    gates: Option<Gates>,
    armed: FourArmed,
) -> bool {
    decided.is_some_and(|l| match l.venue() {
        Venue::Flap => true,
        venue @ (Venue::Four | Venue::OpenFour) => c3_takes(venue, gates, armed),
    })
}

/// The launch a decision hands to the C3 PAPER path: a first verified
/// sighting the path may take.
pub(crate) fn c3_launch(
    decided: Option<Launch>,
    gates: Option<Gates>,
    armed: FourArmed,
) -> Option<[u8; 32]> {
    match decided {
        Some(Launch::First { hash, venue, .. }) if c3_takes(venue, gates, armed) => Some(hash),
        _ => None,
    }
}

fn spawn_adapter(
    source: SourceId,
    url: String,
    limits: WssLimits,
    queues: Arc<IngressQueues>,
    dropped: Arc<AtomicU64>,
    clock: Clock,
) -> Result<Adapter, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let handle = thread::Builder::new()
        .name(format!("bsc-src-{}", source.name()))
        .spawn(move || {
            let wss = WssSource {
                name: source.name(),
                url,
            };
            // T_ingress of a pending transaction is the receipt of its frame,
            // before the JSON parse and the signer recovery.
            let mut sink = |event: WssEvent, received: Instant| {
                let event = match event {
                    WssEvent::Connected => Event::Connected,
                    WssEvent::Full(tx) => Event::Full {
                        hash: tx.hash,
                        raw: tx.raw,
                        from: tx.from,
                        nonce: tx.nonce,
                    },
                    WssEvent::HashOnly(hash) => Event::HashOnly { hash },
                    WssEvent::Rejected(_) => Event::Rejected,
                    WssEvent::Disconnected(reason) => Event::Disconnected {
                        rate_limited: matches!(reason, DisconnectReason::RateLimited(_)),
                    },
                };
                if !queues.push(source, clock.ns_at(received), event) {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            };
            if let Err(e) = run_source(&wss, &limits, &flag, &mut sink) {
                // Refused before any connect: visible in the log and as a disconnect.
                eprintln!("{}: source refused its configuration: {e:?}", source.name());
                sink(
                    WssEvent::Disconnected(DisconnectReason::Stopped),
                    Instant::now(),
                );
            }
        })
        .map_err(|e| format!("spawn {} adapter: {e}", source.name()))?;
    Ok(Adapter { stop, handle })
}

/// REV.1.0 R6: the local geth's adapter: its arena polled on its own
/// thread, each event on the runtime's ingress queue as a WSS source's.
fn spawn_geth_adapter(
    arena: crate::geth_source::GethArena,
    queues: Arc<IngressQueues>,
    dropped: Arc<AtomicU64>,
    clock: Clock,
) -> Result<Adapter, String> {
    use crate::geth_source::GethEvent;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let source = SourceId::Geth;
    let handle = thread::Builder::new()
        .name(format!("bsc-src-{}", source.name()))
        .spawn(move || {
            let mut sink = |event: GethEvent, read: Instant| {
                let event = match event {
                    GethEvent::Connected => Event::Connected,
                    GethEvent::Full {
                        hash,
                        raw,
                        from,
                        nonce,
                    } => Event::Full {
                        hash,
                        raw,
                        from,
                        nonce,
                    },
                    GethEvent::Rejected => Event::Rejected,
                    GethEvent::Alive => Event::Alive,
                    GethEvent::Disconnected => Event::Disconnected {
                        rate_limited: false,
                    },
                };
                if !queues.push(source, clock.ns_at(read), event) {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            };
            crate::geth_source::run(&arena, crate::wss::BSC_MAINNET_CHAIN_ID, &flag, &mut sink);
        })
        .map_err(|e| format!("spawn {} adapter: {e}", source.name()))?;
    Ok(Adapter { stop, handle })
}

/// One pass of the C2 checkpoint (BSC3-039): the writes the writer
/// finished are counted; when one is `due` the window is handed to the
/// writer, never written on this loop, except at the stop: then here,
/// after the writer's writes and never beside one, durable before the
/// process ends.
fn checkpoint_pass(
    o: &mut StateOwner,
    writer: &CheckpointWriter,
    due: bool,
    stopping: bool,
    replace: impl Fn(&Path, &[u8]) -> Result<(), String>,
) {
    for written in writer.finished() {
        o.checkpoint_written(written);
    }
    match (due, stopping) {
        (true, true) => {
            o.checkpoint(|p, b| writer.write_now(p, b, &replace));
            for written in writer.finished() {
                o.checkpoint_written(written);
            }
        }
        (true, false) => writer.submit(o.checkpoint_bytes()),
        (false, _) => {}
    }
}

/// Writes `bytes` to `path` atomically (temp file in the same directory),
/// durable with its rename.
pub fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    replace_with(path, bytes, true)
}

/// [`replace`] without the flush to the device (BSC3-039): for the runtime
/// status, a display record rewritten every status interval on the main
/// loop. Still published by an atomic rename (a reader never sees a part
/// of it); after a host crash it may be the previous one or unreadable
/// until the next interval rewrites it. A full flush costs milliseconds
/// here (F_FULLFSYNC on macOS), each interval, while launches wait.
fn replace_volatile(path: &Path, bytes: &[u8]) -> Result<(), String> {
    replace_with(path, bytes, false)
}

#[cfg(test)]
thread_local! {
    /// Tests: the replacements this thread flushed to the device.
    static FLUSHED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn replace_with(path: &Path, bytes: &[u8], flush: bool) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    let _ = fs::remove_file(&tmp);
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| {
            std::io::Write::write_all(&mut f, bytes)?;
            if flush {
                #[cfg(test)]
                FLUSHED.with(|n| n.set(n.get() + 1));
                f.sync_all()?;
            }
            Ok(())
        })
        .and_then(|()| fs::rename(&tmp, path))
        .and_then(|()| {
            // BSC3-039 review: the rename itself durable (a stop followed
            // by a power loss kept the previous file): the directory
            // handed to the drive and the medium flushed.
            if flush {
                let dir = fs::File::open(path.parent().unwrap_or(Path::new(".")))?;
                darkbot_bsc_journal::durable::handed(&dir)?;
                darkbot_bsc_journal::durable::flush(&dir)?;
            }
            Ok(())
        });
    written.map_err(|e| format!("{}: {e}", path.display()))
}

/// The payload bound of the runtime journal; the census reads with the same.
pub fn journal_max_payload(config: &RuntimeConfig) -> usize {
    config.wss.max_message_bytes + 64
}

/// Journals each source's dropped-event total that grew since the last
/// `D` record and tells the journal the sum (Phase F gate H1: a loss before
/// the journal is a loss of the capture).
fn journal_drops(
    journal: &mut RuntimeJournal,
    dropped: &[Arc<AtomicU64>],
    journaled: &mut [u64; 5],
    at: u64,
) {
    for (i, d) in dropped.iter().enumerate().take(journaled.len()) {
        let total = d.load(Ordering::Relaxed);
        if total > journaled[i] {
            journal.record(
                RecordKind::Runtime,
                at,
                ingress_drop_payload(SourceId::ALL[i], total),
            );
            journaled[i] = total;
        }
    }
    journal.set_ingress_dropped(journaled.iter().sum());
}

/// The checkout's commit and whether its engine sources differ from it
/// (`git -C <repo> rev-parse HEAD`, `git status --porcelain -- bsc/engine`);
/// `None` where there is no checkout (Phase F gate M5: with the binary's own
/// SHA-256 in the manifest).
pub fn checkout_identity(repo_root: &Path) -> (Option<String>, Option<bool>) {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let head = git(&["rev-parse", "HEAD"]).filter(|h| h.len() == 40);
    let dirty = head
        .as_ref()
        .and_then(|_| git(&["status", "--porcelain", "--", "bsc/engine"]))
        .map(|s| !s.is_empty());
    (head, dirty)
}

/// The host a run's latencies are of: OS, architecture, logical CPUs.
pub fn host_label() -> String {
    format!(
        "{} {} cpus={}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map_or(0, |n| n.get())
    )
}

/// The bounds of a capacity preflight of the runtime's capture.
pub fn preflight_setup(config: &RuntimeConfig) -> crate::capture_preflight::PreflightSetup {
    crate::capture_preflight::PreflightSetup {
        duration_s: config.capture_duration_s,
        probe_bytes: config.preflight_probe_bytes,
        min_rates_s: config.preflight_min_rates_s,
        disk_reserve_bytes: config.disk_reserve_bytes,
        memory_budget_bytes: config.journal_budget_bytes as u64,
        queue_records: config.journal_queue as u64,
        max_payload: journal_max_payload(config),
        sync: journal_setup(config).sync,
    }
}

/// The bounds of the runtime's segmented capture.
pub fn journal_setup(config: &RuntimeConfig) -> JournalSetup {
    JournalSetup {
        queue: config.journal_queue,
        budget_bytes: config.journal_budget_bytes,
        max_payload: journal_max_payload(config),
        segment: SegmentPolicy {
            max_segment_bytes: config.journal_segment_bytes,
            max_segment_ns: config.journal_segment_ms.saturating_mul(1_000_000),
        },
        sync: SyncPolicy {
            every_records: config.journal_sync_records,
            every: Duration::from_millis(config.journal_sync_ms),
        },
    }
}

/// Replays the journals of `rundir` into `bsc-runtime/g04-census.dbrec`;
/// `active` is the journal the calling runtime still writes (`RUNNING`).
pub fn write_census(
    config: &RuntimeConfig,
    rundir: &Path,
    active: Option<&Path>,
) -> Result<Census, String> {
    let dir = rundir.join("bsc-runtime");
    let census = census(&dir.join("journal"), journal_max_payload(config), active)?;
    replace(
        &dir.join("g04-census.dbrec"),
        &census.encode(observed_now()?)?,
    )?;
    Ok(census)
}

/// Wall-clock reads that failed in this process (BSC3-015): each one
/// counted as expired time (`u64::MAX`: every grant expired, every head too
/// old), shown in the status, never silent.
static CLOCK_UNREADABLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The wall clock now (ms), or expired time when it cannot be read
/// (counted, [`CLOCK_UNREADABLE`]).
pub(crate) fn wall_or_expired(read: Result<u64, String>) -> u64 {
    read.unwrap_or_else(|_| {
        count_unreadable_clock();
        u64::MAX
    })
}

/// Counts one failed wall-clock read ([`CLOCK_UNREADABLE`]).
pub(crate) fn count_unreadable_clock() {
    CLOCK_UNREADABLE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// The failed wall-clock reads of this process so far.
pub(crate) fn unreadable_clock_reads() -> u64 {
    CLOCK_UNREADABLE.load(std::sync::atomic::Ordering::Relaxed)
}

/// The last G0.4 census refresh of this run (BSC3-016): `OK` or why it
/// failed, and when the last one was written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CensusRefresh {
    state: Option<String>,
    written_unix_ms: Option<u64>,
}

impl CensusRefresh {
    /// The census thread's end: its result, or that it panicked.
    fn ended(&mut self, joined: std::thread::Result<Result<u64, String>>) {
        match joined {
            Ok(Ok(ms)) => {
                self.state = Some("OK".into());
                self.written_unix_ms = Some(ms);
            }
            Ok(Err(e)) => self.state = Some(format!("FAILED: {e}")),
            Err(_) => self.state = Some("FAILED: the census thread panicked".into()),
        }
    }
}

/// The status record of this moment.
#[allow(clippy::too_many_arguments)]
fn status_bytes(
    generation: &Generation,
    stats: &[(SourceId, SourceStats, Health)],
    clock: Clock,
    started_ms: u64,
    observed_ms: u64,
    launches: u64,
    journal: &str,
    journal_records: u64,
    journal_lost: u64,
    capture: Option<&CaptureStatus>,
    canonical: Option<&Canonical>,
    owner: Option<&StateOwner>,
    exec: Option<&ExecRuntime>,
    live: Option<&LiveRuntime>,
    status_interval_ms: u64,
    bounds: (u64, crate::sources::OwnerLimits),
    census: &CensusRefresh,
) -> Result<Vec<u8>, String> {
    let now = clock.now_ns();
    let mut d = Draft::new(&rs::SCHEMA);
    d.set(rs::GENERATION, generation.generation)
        .set(rs::PROVENANCE_TOOL, "bsc_runtime")
        .set(rs::PROVENANCE_OBSERVED_UNIX_MS, observed_ms)
        .set(rs::STARTED_UNIX_MS, started_ms)
        .set(rs::LAUNCHES, launches)
        .set(rs::JOURNAL_PATH, journal)
        .set(rs::JOURNAL_RECORDS, journal_records)
        .set(rs::JOURNAL_LOST, journal_lost)
        .set(
            rs::MODE,
            // Plan 28.10.: LIVE while the LIVE path is armed.
            if live.is_some_and(LiveRuntime::armed) {
                "LIVE"
            } else {
                "READ_ONLY"
            },
        )
        .set(rs::STATUS_INTERVAL_MS, status_interval_ms)
        .set(rs::STATUS_STALE_INTERVALS, bounds.0)
        .set(
            rs::BUY_ATTEMPTS_TOTAL_MAX,
            u64::from(bounds.1.buy_attempts_total_max),
        );
    if let Some(state) = &census.state {
        d.set(rs::CENSUS_STATE, state.as_str());
    }
    d.set(rs::CLOCK_UNREADABLE, unreadable_clock_reads());
    if let Some(ms) = census.written_unix_ms {
        d.set(rs::CENSUS_WRITTEN_UNIX_MS, ms);
    }
    if let Some(c) = capture {
        use rs::capture as k;
        let w = &c.writer;
        let mut e = Draft::new(&k::SCHEMA);
        e.set(k::RECORDS, w.records)
            .set(k::BYTES, w.bytes)
            .set(k::LOST_RECORDS, w.lost_records)
            .set(k::SEGMENTS_SEALED, w.segments_sealed)
            .set(k::SYNCS, w.syncs)
            .set(k::BUDGET_MAX_BYTES, c.budget_max)
            .set(k::BUDGET_IN_USE_BYTES, c.budget_in_use)
            .set(k::BUDGET_PEAK_BYTES, c.budget_peak)
            .set(k::BUDGET_REFUSED, c.budget_refused)
            .set(k::WRITE_ERRORS, w.write_errors)
            .set(k::STATE, if w.failed { "FAILED" } else { "HEALTHY" });
        if let Some(v) = w.durable_sequence {
            e.set(k::DURABLE_SEQUENCE, v);
        }
        d.set(rs::CAPTURE, e.into_nested());
    }
    for (source, s, h) in stats {
        use rs::source as e;
        let mut row = Draft::new(&e::SCHEMA);
        row.set(e::SOURCE_ID, source.name())
            .set(e::EFFECTIVE, generation.source(*source).effective.name())
            .set(e::HEALTH, h.name())
            .set(e::CONNECTS, s.connects)
            .set(e::DISCONNECTS, s.disconnects)
            .set(e::RATE_LIMITED, s.rate_limited)
            .set(e::FULL, s.full)
            .set(e::HASH_ONLY, s.hash_only)
            .set(e::REJECTED, s.rejected)
            .set(e::DROPPED, s.dropped)
            .set(e::DUPLICATES, s.duplicates)
            .set(e::FIRST_ANY, s.first_any)
            .set(e::FIRST_FULL, s.first_full)
            .set(e::LAUNCH_SIGHTINGS, s.launch_sightings)
            .set(e::LAUNCH_FIRST, s.launch_first)
            .set(
                e::CAPABILITY,
                crate::ingress::observed_capability(*source, s).name(),
            );
        if let Some(at) = s.last_event_ns {
            row.set(e::LAST_EVENT_AGE_MS, now.saturating_sub(at) / 1_000_000);
        }
        if let Some(mean) = s.queue_mean_ns() {
            row.set(e::QUEUE_MAX_NS, s.queue_max_ns)
                .set(e::QUEUE_MEAN_NS, mean);
        }
        d.push(rs::SOURCES, row.into_nested());
    }
    if let Some(c) = canonical {
        d.set(rs::CANONICAL, c.draft(now).into_nested());
    }
    if let Some(o) = owner {
        d.set(rs::STATE, o.draft().into_nested());
    }
    if let Some(x) = exec {
        d.set(rs::EXEC, x.draft().into_nested());
    }
    if let Some(l) = live {
        d.set(rs::LIVE, l.draft().into_nested());
    }
    if let Some((c, cfg)) = canonical.and_then(|c| c.strategy_config.as_ref().map(|k| (c, k))) {
        d.set(
            rs::STRATEGY,
            crate::runtime_strategy::status_draft(
                c.strategy.as_ref(),
                c.strategy_refused.as_ref(),
                cfg,
                c.strategy_settings.as_ref(),
            )
            .into_nested(),
        );
    }
    d.encode().map_err(|e| e.to_string())
}

/// Frontend package (06.11.): starts Strategy C on `c` with the operator's
/// settings on file under `sources` (`keep`: with those it ran with, a
/// restart for other venues: new settings are effective at a switch on or a
/// runtime restart), journaled before its switch (`p`, then `U`: a replay
/// starts it with them); refused (`SETTINGS_REFUSED`: unreadable, invalid,
/// beyond the records' bound, or its `p` not journaled), it stays off and
/// the next pass tries again.
#[allow(clippy::too_many_arguments)]
fn start_strategy(
    c: &mut Canonical,
    journal: &mut RuntimeJournal,
    clock: Clock,
    config: &RuntimeConfig,
    sources: &Path,
    gas: &darkbot_bsc_trading::economics::network::GasEvidence,
    venues: crate::runtime_strategy::StrategyVenues,
    keep: Option<crate::strategy_settings::Settings>,
) {
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status::strategy as st;
    let started = match keep {
        Some(settings) => strategy_with(config, settings),
        None => strategy_start(config, sources),
    };
    let refuse = |c: &mut Canonical, why: String| {
        c.strategy_refused = Some((
            "SETTINGS_REFUSED",
            crate::strategy_settings::bounded(why, st::SCHEMA.field(st::REASON)),
        ))
    };
    let (settings, engine) = match started {
        Ok(s) => s,
        Err(why) => return refuse(c, why),
    };
    let values = crate::strategy_settings::effective_values(&config.source_text, &settings);
    if let Some((k, _)) = values
        .iter()
        .find(|(_, v)| v.len() > crate::strategy_settings::max_value_len())
    {
        return refuse(
            c,
            format!("STRATEGY_SETTINGS_INVALID: {k} is longer than the status holds"),
        );
    }
    // Review LOW 4: a start whose settings are not journaled would replay
    // with others.
    if settings.generation > 0
        && !journal.record(RecordKind::Runtime, clock.now_ns(), settings.journal())
    {
        return refuse(
            c,
            format!(
                "STRATEGY_SETTINGS_NOT_JOURNALED: generation {}",
                settings.generation
            ),
        );
    }
    journal.record(
        RecordKind::Runtime,
        clock.now_ns(),
        crate::runtime_strategy::switch_payload(true, venues),
    );
    c.strategy = Some(crate::runtime_strategy::StrategyRuntime::new(
        engine.clone(),
        gas.clone(),
        venues,
    ));
    c.strategy_settings = Some(crate::strategy_settings::Settings {
        generation: settings.generation,
        request_id: None,
        values,
    });
    c.strategy_taken = Some(settings);
    c.strategy_config = Some(engine);
    c.strategy_refused = None;
}

/// Frontend package (06.11.): the operator's Strategy C settings on file
/// under `sources` and the strategy's configuration with them in place.
fn strategy_start(
    config: &RuntimeConfig,
    sources: &Path,
) -> Result<
    (
        crate::strategy_settings::Settings,
        darkbot_bsc_trading::strategy::engine::Config,
    ),
    String,
> {
    let settings = crate::strategy_settings::current(sources)
        .map_err(|e| format!("STRATEGY_SETTINGS_UNREADABLE: {e}"))?;
    strategy_with(config, settings)
}

/// The strategy's configuration with `settings` in place (none: the file's).
fn strategy_with(
    config: &RuntimeConfig,
    settings: crate::strategy_settings::Settings,
) -> Result<
    (
        crate::strategy_settings::Settings,
        darkbot_bsc_trading::strategy::engine::Config,
    ),
    String,
> {
    if settings.generation == 0 {
        return Ok((settings, config.strategy.engine.clone()));
    }
    let engine = crate::strategy_settings::effective(&config.source_text, &settings)
        .map_err(|e| {
            format!(
                "STRATEGY_SETTINGS_INVALID: generation {}: {e}",
                settings.generation
            )
        })?
        .strategy
        .engine;
    Ok((settings, engine))
}

/// The stop request of the runtime in `rundir` (written by `darkbotctl` or
/// by the process's own SIGTERM/SIGINT handler).
pub fn stop_file(rundir: &Path) -> PathBuf {
    rundir.join("bsc.runtime.stop")
}

/// The sighting of a Flap launch that may trigger the armed LIVE path: the
/// first verified full one (`true`), or a later one from another source
/// (`false`, BSC3-075: it triggers when the first came from a source the
/// grant does not name).
fn live_sighting(decided: Option<Launch>) -> Option<(SourceId, [u8; 32], bool)> {
    match decided {
        Some(Launch::First {
            hash,
            source,
            venue: Venue::Flap,
            ..
        }) => Some((source, hash, true)),
        Some(Launch::Later {
            hash,
            source,
            venue: Venue::Flap,
            ..
        }) => Some((source, hash, false)),
        _ => None,
    }
}

/// Runs until `rundir/bsc.runtime.stop` exists.
pub fn run(
    config: &RuntimeConfig,
    sources_dir: &Path,
    rundir: &Path,
    repo_root: &Path,
) -> Result<(), String> {
    let store = Store::open(sources_dir, config.owner)?;
    let clock = Clock(Instant::now());
    let started_ms = observed_now()?;
    let dir = rundir.join("bsc-runtime");
    fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    let stop_file = stop_file(rundir);
    let status_path = dir.join("status.dbrec");
    let mut journal = RuntimeJournal::open(
        &dir.join("journal"),
        started_ms,
        u64::from(CLOCK_ID_MONOTONIC),
        journal_setup(config),
    )?;
    // The checkout at start may have moved since the build (a shared
    // checkout): the label says so; BUILD_SHA256 names the binary itself.
    let (checkout, dirty) = checkout_identity(repo_root);
    journal.set_identity(RunIdentity {
        host: host_label(),
        build: format!(
            "darkbot-bsc-execution {} checkout-at-start {}{}{}",
            env!("CARGO_PKG_VERSION"),
            checkout.as_deref().unwrap_or("unknown"),
            if dirty == Some(true) { "+dirty" } else { "" },
            // R11: a harness build (test CA trusted) names itself.
            if cfg!(feature = "loopback-harness") {
                " loopback-harness"
            } else {
                ""
            }
        ),
        config_sha256: config.source_sha256,
        build_sha256: own_binary_sha256(),
        build_dirty: dirty,
        capture_pending: config.capture_pending.name(),
    });
    // BSC3-033: the G0.4 pre-commitment on file at this start, first in
    // the journal (a crashed run keeps it); an unreadable one binds nothing.
    if let Ok(Some(p)) = crate::g04_criteria::Precommit::read(rundir) {
        journal.record(
            RecordKind::Runtime,
            clock.now_ns(),
            crate::runtime_journal::g04_precommit_payload(&p.sha256),
        );
    }
    let journal_rel: PathBuf = journal
        .path()
        .strip_prefix(rundir)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| journal.path().to_path_buf());
    let limits = config.wss;
    let ingress = IngressQueues::new(config.queue_events);
    let dropped: Vec<Arc<AtomicU64>> = SourceId::ALL
        .iter()
        .map(|_| Arc::new(AtomicU64::new(0)))
        .collect();
    let mut core = IngressCore::new(config.dedup_hashes, config.max_launch_calldata);
    // The dropped-event totals last journaled, per source (`D` records).
    let mut journaled_drops = [0u64; 5];
    // The periodic census in flight, if any.
    let mut census_job: Option<std::thread::JoinHandle<Result<u64, String>>> = None;
    let mut census_refresh = CensusRefresh::default();
    // Phase E review L3: the Four.meme drift guard in flight, and its next
    // run (at start, then every BSC_CANONICAL_FOUR_PROBE_MS).
    type FourProbe = Result<crate::venue_probe::FourObservation, crate::venue_probe::ProbeError>;
    let mut four_job: Option<std::thread::JoinHandle<FourProbe>> = None;
    let mut next_four = Instant::now();
    let mut adapters: HashMap<usize, Adapter> = HashMap::new();
    let mut rate_limited_last = [false; 5];
    // The owner's start (and launchd's crash restart): every wallet and
    // economic switch DISABLED before anything runs (plan 27.10.).
    let mut generation = store.restart(started_ms)?;
    // Strategy C's frozen gas evidence; refused, the strategy cannot run
    // (its status says why).
    let strategy_gas = config.strategy.gas(repo_root);
    // BSC3-079: no configured gas limit below the largest gas the frozen
    // evidence holds for its class (a transaction would run out of gas).
    exec_gas_covered(config)?;
    let (canonical_tx, canonical_rx) = sync_channel::<(u64, CanonicalEvent)>(config.queue_events);
    let canonical_rx = Some(canonical_rx);
    // The C3 PAPER path: its identity is loaded at start, and C2 watches its
    // sender (the follower reads that account's nonce facts).
    let mut exec = config
        .exec
        .clone()
        .map(|setup| ExecRuntime::new(setup, repo_root, dir.join("paper")));
    // Plan 28.10.: the LIVE path; its buyer is a watched C2 sender (config).
    let mut live = config
        .exec
        .clone()
        .zip(config.live.clone())
        .map(|(x, setup)| {
            LiveRuntime::new(
                setup,
                x,
                repo_root,
                sources_dir.to_path_buf(),
                dir.join("live"),
                exec.as_ref().and_then(ExecRuntime::sender),
            )
            .with_ingress_hashes(config.dedup_hashes)
        });
    let mut canonical_config = config.canonical.clone();
    if let (Some(c), Some(sender)) = (
        canonical_config.as_mut(),
        exec.as_ref().and_then(ExecRuntime::sender),
    ) {
        if !c.state.watched.contains(&sender) {
            c.state.watched.push(sender);
        }
        c.state.validate()?;
    }
    let mut canonical = canonical_config
        .as_ref()
        .map(|c| Canonical::new(c, started_ms));
    let checkpoint_path = dir.join("c2-checkpoint.bin");
    // BSC3-039: the periodic checkpoint is written off this loop.
    let checkpoints = canonical_config
        .as_ref()
        .map(|_| CheckpointWriter::spawn(checkpoint_path.clone(), Arc::new(replace)))
        .transpose()?;
    let c2_wanted =
        |g: &Generation| g.component(ComponentId::StateC2).effective == Effective::Shadow;
    // The C2 owner restores its window at start; the follower resumes after it.
    let mut owner = match &canonical_config {
        Some(c) if c2_wanted(&generation) => {
            Some(StateOwner::restore(&c.state, checkpoint_path.clone())?)
        }
        _ => None,
    };
    let mut follower = match &canonical_config {
        Some(c) => {
            let resume = owner
                .as_ref()
                .map(StateOwner::resume_window)
                .unwrap_or_default();
            Some(spawn_follower(c.clone(), resume, canonical_tx, clock)?)
        }
        None => {
            drop(canonical_tx);
            None
        }
    };
    // BSC3-056: the second source's headers, on their own thread.
    let (confirm_tx, confirm_rx) =
        sync_channel::<crate::chain_follower::ConfirmEvent>(config.queue_events);
    let mut confirmer = match canonical_config
        .as_ref()
        .and_then(|c| c.confirm.as_ref().map(|k| (c, k)))
    {
        Some((c, k)) => Some(spawn_confirmer(c, k, confirm_tx)?),
        None => {
            drop(confirm_tx);
            None
        }
    };
    if let Some(c) = canonical.as_mut() {
        c.confirm_rx = Some(confirm_rx);
    }
    let mut next_checkpoint = Instant::now()
        + Duration::from_millis(
            canonical_config
                .as_ref()
                .map_or(0, |c| c.state.checkpoint_ms),
        );
    let mut next_census = Instant::now() + Duration::from_millis(config.census_ms);
    let mut next_status = Instant::now();
    let mut next_reconcile = Instant::now();
    let reconcile_every = Duration::from_millis(config.reconcile_ms);
    let canonical_slice = Duration::from_millis(config.canonical_slice_ms);
    let status_every = Duration::from_millis(config.status_ms);
    let index = |s: SourceId| s.index();
    let mut stop_journaled = false;
    // Which sources run this pass (the reconcile sets it; LIVE included).
    let mut source_on = SourceId::ALL.map(|s| generation.source(s).effective == Effective::Shadow);
    // The last reconcile's gates (none before the first: nothing admitted).
    let mut last_gates: Option<Gates> = None;
    loop {
        let stopping = stop_file.exists();
        // One fresh monotonic/wall pair per pass for the sightings' wall time.
        let wall = WallPair::read(clock);
        if stopping && !stop_journaled {
            journal.record(RecordKind::Runtime, clock.now_ns(), STOP_PAYLOAD.to_vec());
            stop_journaled = true;
        }
        if stopping {
            // Every adapter and the follower stop in parallel; the joins
            // below then wait for the slowest one only.
            for a in adapters.values() {
                a.stop.store(true, Ordering::Release);
            }
            if let Some(f) = follower.as_ref() {
                f.stop.store(true, Ordering::Release);
            }
        }
        if Instant::now() >= next_reconcile || stopping {
            next_reconcile = Instant::now() + reconcile_every;
            if !stopping {
                generation = store.current()?;
                // Plan 28.10.: what the owner's LIVE gate makes effective
                // now; a requested LIVE runs only while it is clear.
                let now_ms = wall_or_expired(observed_now());
                // BSC3-050: the LIVE qualification verified once for this
                // reconcile, for the gate's view and the LIVE path alike.
                let qualified = live
                    .is_some()
                    .then(|| crate::live_qualification::view(repo_root));
                let live_view = LiveView::of(
                    &generation,
                    sources_dir,
                    qualified.as_ref().map(|(q, _)| &q.view),
                    now_ms,
                    live.as_ref(),
                    config.owner.buy_attempts_total_max,
                );
                let exits = live.as_ref().is_some_and(|l| l.holds_positions());
                let eff = |e: crate::sources::Entity| live_view.effective(&generation, e, exits);
                let running = |e: Effective| matches!(e, Effective::Shadow | Effective::Live);
                let reconciled = gates(eff, generation.component(ComponentId::ExecC3).effective);
                last_gates = Some(reconciled);
                let Gates {
                    flap_admitted: _,
                    four_admitted,
                    openfour_admitted,
                    strategy_on,
                    strategy_venues,
                    c3_wanted,
                    c2_on,
                } = reconciled;
                for source in SourceId::ALL {
                    source_on[index(source)] = running(eff(source.into()));
                }
                if let Some(c) = canonical.as_mut() {
                    // Each switch is journaled: a replay starts and ends its
                    // engine at the same record.
                    // SC4: other admitted venues start it over (off, then on
                    // with them), as a replay does.
                    let running = c.strategy.as_ref().map(|s| s.venues());
                    let switch = strategy_switch(
                        strategy_on,
                        running,
                        strategy_venues,
                        strategy_gas.is_ok(),
                    );
                    if matches!(switch, StrategySwitch::Stop | StrategySwitch::Restart) {
                        journal.record(
                            RecordKind::Runtime,
                            clock.now_ns(),
                            crate::runtime_strategy::switch_payload(false, strategy_venues),
                        );
                        c.strategy = None;
                        c.strategy_refused = None;
                    }
                    match (switch, &strategy_gas) {
                        (StrategySwitch::Start | StrategySwitch::Restart, Ok(gas)) => {
                            // While it stays switched on (a restart for
                            // other venues, a refused restart tried again)
                            // it keeps the settings it ran with (review M1);
                            // a switch on or a runtime restart takes those
                            // on file.
                            let keep = c.strategy_taken.clone();
                            start_strategy(
                                c,
                                &mut journal,
                                clock,
                                config,
                                sources_dir,
                                gas,
                                strategy_venues,
                                keep,
                            )
                        }
                        (StrategySwitch::Refused, Err(why)) => {
                            c.strategy_refused = Some(("GAS_EVIDENCE_REFUSED", why.clone()))
                        }
                        (StrategySwitch::Off, _) => {
                            c.strategy = None;
                            c.strategy_refused = None;
                        }
                        _ => {}
                    }
                    // The configuration of the running strategy, else the
                    // file's.
                    c.strategy_config = match (strategy_on, &c.strategy) {
                        (false, _) => None,
                        (true, Some(_)) => c.strategy_config.take(),
                        (true, None) => Some(config.strategy.engine.clone()),
                    };
                    if c.strategy.is_none() {
                        c.strategy_settings = None;
                    }
                    // Off, the next start takes the settings on file.
                    if !strategy_on {
                        c.strategy_taken = None;
                    }
                    // R1: a strategy switched on or off changes who decides
                    // the open lots' exits.
                    refresh_lot_deciders(c, &mut live);
                }
                if let (Some(c), Some(w)) = (&canonical_config, &checkpoints) {
                    // Switched off, the last window is written here, after
                    // the writer's writes (counted first) and never beside one.
                    if !c2_on && let Some(o) = owner.as_mut() {
                        for written in w.idle() {
                            o.checkpoint_written(written);
                        }
                    }
                    StateOwner::switch(&mut owner, c2_on, &c.state, &checkpoint_path, |p, b| {
                        w.write_now(p, b, replace)
                    })?;
                }
                let active_endpoint = canonical.as_ref().map_or(0, |c| c.active);
                if let Some(x) = exec.as_mut() {
                    let wanted = c3_wanted;
                    let follower = canonical_config
                        .as_ref()
                        // BSC3-036: the endpoint the follower follows now.
                        .map(|c| (c.endpoint_at(active_endpoint), &c.limits));
                    x.set_four_wanted(four_admitted, openfour_admitted);
                    x.tick(wanted, owner.as_ref(), follower, started_ms);
                }
                if let Some(l) = live.as_mut() {
                    let follower = canonical_config
                        .as_ref()
                        // BSC3-036: the endpoint the follower follows now.
                        .map(|c| (c.endpoint_at(active_endpoint), &c.limits));
                    // BSC3-051: the providers' answers first; a connection
                    // the maintenance would give up may hold one.
                    l.poll_responses(&FnClock {
                        id: CLOCK_ID_MONOTONIC,
                        now: || clock.now_ns(),
                    });
                    // REV.1.0 R2: each answer with its time, the attempts'
                    // record.
                    for a in l.take_answers() {
                        journal.record(
                            RecordKind::Runtime,
                            a.at_ns.unwrap_or_else(|| clock.now_ns()),
                            crate::runtime_journal::live_answer_payload(&a),
                        );
                    }
                    l.record_responses();
                    match &qualified {
                        Some(q) => l.tick_with(q, owner.as_ref(), follower, started_ms, now_ms),
                        None => l.tick(owner.as_ref(), follower, started_ms, now_ms),
                    }
                }
                // REV.1.0 R3: what the operator decided of the lots since
                // (`bsc_runtime live lot`, `live adopt-g07`): the strategy
                // lets a lot no longer open go and holds an opened one
                // (journaled first), a resumed lot's curve is read again.
                let (changes, resumed) =
                    live.as_mut().map(|l| l.take_decided()).unwrap_or_default();
                if let Some(c) = canonical.as_mut()
                    && (!changes.is_empty() || !resumed.is_empty())
                {
                    let at = clock.now_ns();
                    if let Some(s) = c.strategy.as_mut() {
                        for change in changes {
                            match change {
                                live::PositionChange::Opened {
                                    token,
                                    tokens,
                                    spent_wei,
                                    filled_ms,
                                    exits,
                                } => s.hold(
                                    token,
                                    tokens,
                                    spent_wei,
                                    filled_ms,
                                    exits,
                                    at,
                                    &mut journal,
                                ),
                                live::PositionChange::Ended { token } => {
                                    s.unhold(token, at, &mut journal)
                                }
                            }
                        }
                    }
                    for token in resumed {
                        c.lot.resume(token);
                    }
                    refresh_lot_deciders(c, &mut live);
                }
            }
            for source in SourceId::ALL {
                let i = index(source);
                let want = !stopping && source_on[i] && config.endpoint(source).is_some();
                match (want, adapters.contains_key(&i)) {
                    (true, false) => {
                        let url = config.endpoint(source).unwrap_or_default().to_string();
                        let a = match (source, &config.geth) {
                            // R6: the local geth over its arena.
                            (SourceId::Geth, Some(arena)) => spawn_geth_adapter(
                                arena.clone(),
                                Arc::clone(&ingress),
                                Arc::clone(&dropped[i]),
                                clock,
                            )?,
                            _ => spawn_adapter(
                                source,
                                url,
                                limits,
                                Arc::clone(&ingress),
                                Arc::clone(&dropped[i]),
                                clock,
                            )?,
                        };
                        adapters.insert(i, a);
                        journal.record(
                            RecordKind::Runtime,
                            clock.now_ns(),
                            source_event_payload(source, SourceEvent::Started),
                        );
                    }
                    (false, true) => {
                        // The adapter reports its own last Disconnected(Stopped).
                        if let Some(a) = adapters.remove(&i) {
                            a.stop.store(true, Ordering::Release);
                            let _ = a.handle.join();
                        }
                        journal.record(
                            RecordKind::Runtime,
                            clock.now_ns(),
                            source_event_payload(source, SourceEvent::Stopped),
                        );
                    }
                    (true, true) if adapters.get(&i).is_some_and(|a| a.handle.is_finished()) => {
                        // An adapter that ended on its own (refused config, panic) is
                        // disconnected and started again at the next reconcile.
                        if let Some(a) = adapters.remove(&i) {
                            let _ = a.handle.join();
                        }
                        core.observe(
                            source,
                            Observation::Disconnected {
                                rate_limited: false,
                            },
                            clock.now_ns(),
                            None,
                        );
                        journal.record(
                            RecordKind::Runtime,
                            clock.now_ns(),
                            source_event_payload(source, SourceEvent::Stopped),
                        );
                    }
                    _ => {}
                }
            }
        }
        // Events, bounded per pass so status and stop stay timely.
        let deadline = Instant::now() + reconcile_every;
        while Instant::now() < deadline {
            // The canonical events do not wait for the pass (Phase I gate
            // H1): drained before every ingress event and every slice.
            drain_canonical(
                &mut canonical,
                canonical_rx.as_ref(),
                &mut owner,
                &mut exec,
                &mut live,
                &mut journal,
                clock,
            );
            let wait = deadline
                .saturating_duration_since(Instant::now())
                .min(canonical_slice);
            let Some((source, at, event)) = ingress.pop(wait) else {
                continue;
            };
            // R6 (T_queue): its wait between receipt and this dequeue.
            core.queued(source, clock.now_ns().saturating_sub(at));
            let observation = match &event {
                Event::Full { hash, raw, .. } => Observation::Full { hash: *hash, raw },
                Event::HashOnly { hash } => Observation::HashOnly { hash: *hash },
                Event::Rejected => Observation::Rejected,
                Event::Alive => Observation::Alive,
                Event::Connected => {
                    rate_limited_last[index(source)] = false;
                    journal.record(
                        RecordKind::Runtime,
                        at,
                        source_event_payload(source, SourceEvent::Connected),
                    );
                    Observation::Connected
                }
                Event::Disconnected { rate_limited } => {
                    rate_limited_last[index(source)] = *rate_limited;
                    let kind = if *rate_limited {
                        SourceEvent::RateLimited
                    } else {
                        SourceEvent::Disconnected
                    };
                    journal.record(RecordKind::Runtime, at, source_event_payload(source, kind));
                    Observation::Disconnected {
                        rate_limited: *rate_limited,
                    }
                }
            };
            let decided = core.observe(source, observation, at, wall.wall_us(at));
            // R12c: a Four Classic sighting goes to C2 and C3 only while the
            // C3 path holds the Four Classic BUY; R12d: an OpenFour one
            // while it holds the OpenFour BUY.
            let four_armed = FourArmed {
                classic: exec.as_ref().is_some_and(ExecRuntime::four_armed),
                openfour: exec.as_ref().is_some_and(ExecRuntime::openfour_armed),
            };
            if let (
                Some(o),
                Event::Full {
                    hash, from, nonce, ..
                },
            ) = (owner.as_mut(), &event)
                && (c2_candidate(decided, last_gates, four_armed) || o.watches(from))
            {
                // Source lanes are indexed as in `SourceId::ALL` (five).
                o.pending(*hash, *from, *nonce, index(source) as u8, at);
            }
            // Plan 28.10.: on the LIVE path when it is armed, the first
            // verified full sighting from a granted source (never a
            // hash-only one: it carries no transaction); BSC3-075: also
            // when a source the grant does not name saw it first.
            if let (Some(l), Some(o), Some((by, hash, first)), Event::Full { raw, from, .. }) = (
                live.as_mut(),
                owner.as_ref(),
                live_sighting(decided),
                &event,
            ) && l.armed()
            {
                let now = FnClock {
                    id: CLOCK_ID_MONOTONIC,
                    now: || clock.now_ns(),
                };
                if let Some(buy) = l.on_sighting(by, hash, *from, first, raw, at, o, &now) {
                    journal.record(
                        RecordKind::Runtime,
                        buy.times.sink_ns,
                        crate::runtime_journal::live_release_payload(
                            0,
                            Some(by),
                            &buy.handoff,
                            buy.launch_hash,
                            buy.token,
                            0,
                            &buy.times,
                            &buy.raw,
                        ),
                    );
                }
            }
            // C3: the first verified sighting of a C1-scope launch on an
            // admitted venue runs the production path to the PAPER sink
            // (when armed).
            else if let (Some(x), Some(o), Some(hash), Event::Full { raw, from, .. }) = (
                exec.as_mut(),
                owner.as_ref(),
                c3_launch(decided, last_gates, four_armed),
                &event,
            ) && let Some(buy) = x.on_launch(
                hash,
                *from,
                raw,
                at,
                o,
                &FnClock {
                    id: CLOCK_ID_MONOTONIC,
                    now: || clock.now_ns(),
                },
            ) {
                journal.record(
                    RecordKind::Runtime,
                    buy.times.sink_ns,
                    paper_handoff_payload(&buy),
                );
            }
            match decided {
                Some(Launch::First { hash, source, .. }) => {
                    if let Event::Full { raw, .. } = &event {
                        journal.record(
                            RecordKind::RawSignedTx,
                            at,
                            first_sighting_payload(source, &hash, raw),
                        );
                    }
                }
                Some(Launch::Later {
                    hash,
                    source,
                    lag_ns,
                    ..
                }) => {
                    journal.record(
                        RecordKind::Runtime,
                        at,
                        later_sighting_payload(source, &hash, lag_ns),
                    );
                }
                // Not a launch: the pending capture scope decides.
                None => {
                    if let Event::Full { raw, hash, .. } = &event
                        && core.last_first_full()
                        && config.capture_pending.keeps(core.last_venue_target())
                    {
                        journal.record(
                            RecordKind::TxpoolAccepted,
                            at,
                            first_sighting_payload(source, hash, raw),
                        );
                    }
                }
            }
        }
        // Canonical events: every queued one, then the joins that are due.
        if let (Some(c), Some(crx)) = (canonical.as_mut(), canonical_rx.as_ref()) {
            if stopping && let Some(f) = follower.take() {
                // The follower ends within its read tick and request bound;
                // its events up to then are still processed.
                f.stop.store(true, Ordering::Release);
                loop {
                    match crx.recv_timeout(reconcile_every) {
                        Ok((at, event)) => apply_canonical(
                            c,
                            &mut owner,
                            &mut exec,
                            &mut live,
                            &mut journal,
                            clock,
                            at,
                            event,
                        ),
                        Err(RecvTimeoutError::Timeout) if !f.handle.is_finished() => {}
                        Err(_) => break,
                    }
                }
                let _ = f.handle.join();
            }
            while let Ok((at, event)) = crx.try_recv() {
                apply_canonical(
                    c,
                    &mut owner,
                    &mut exec,
                    &mut live,
                    &mut journal,
                    clock,
                    at,
                    event,
                );
            }
            // BSC3-056: the second source's headers, then whether it
            // confirms the followed chain now (the LIVE BUY gate; also
            // before each block's BUY, apply_canonical).
            c.refresh_confirmation(&mut live, wall_or_expired(observed_now()));
            if stopping && let Some(f) = confirmer.take() {
                f.stop.store(true, Ordering::Release);
                let _ = f.handle.join();
            }
            let now = if stopping { u64::MAX } else { clock.now_ns() };
            c.join(now, &core, &mut journal);
            if stopping && c.state != "STOPPED" {
                // A window ends only if it started (a block mined after the start).
                if let Some((n, h)) = c.head.filter(|_| c.first_journaled) {
                    journal.record(
                        RecordKind::Canonical,
                        clock.now_ns(),
                        window_payload(b'E', n, &h),
                    );
                }
                c.state = "STOPPED";
            } else if follower.as_ref().is_some_and(|f| f.handle.is_finished()) {
                c.state = "FAILING";
            }
        }
        if Instant::now() >= next_status || stopping {
            next_status = Instant::now() + status_every;
            journal_drops(&mut journal, &dropped, &mut journaled_drops, clock.now_ns());
            let now = clock.now_ns();
            let stats: Vec<_> = SourceId::ALL
                .iter()
                .map(|&s| {
                    let i = index(s);
                    let mut st = core.stats(s);
                    st.dropped += dropped[i].load(Ordering::Relaxed);
                    // R6 review HIGH-1: whether it runs this pass (the
                    // reconcile's view, LIVE included), not its stored mode.
                    let h = crate::ingress::source_health(
                        source_on[i],
                        config.endpoint(s).is_some(),
                        stopping,
                        &st,
                        rate_limited_last[i],
                        now,
                        config.quiet_ms * 1_000_000,
                    );
                    (s, st, h)
                })
                .collect();
            // R6: the LIVE path counts a non-public source a signal only while
            // it is healthy and delivers full transactions.
            if let Some(l) = live.as_mut() {
                l.set_source_signals(
                    stats
                        .iter()
                        .map(|(s, st, h)| {
                            let quiet = config.quiet_ms * 1_000_000;
                            (
                                *s,
                                *h,
                                crate::ingress::current_capability(*s, st, now, quiet),
                            )
                        })
                        .collect(),
                );
            }
            let observed = observed_now()?;
            let bytes = status_bytes(
                &generation,
                &stats,
                clock,
                started_ms,
                observed,
                core.launches(),
                &journal_rel.display().to_string(),
                journal.appended(),
                journal.lost(),
                Some(&journal.capture()),
                canonical.as_ref(),
                owner.as_ref(),
                exec.as_ref(),
                live.as_ref(),
                config.status_ms,
                (config.status_stale_intervals, config.owner),
                &census_refresh,
            )?;
            replace_volatile(&status_path, &bytes)?;
        }
        if let (Some(o), Some(w)) = (owner.as_mut(), &checkpoints) {
            let due = stopping || Instant::now() >= next_checkpoint;
            if due {
                next_checkpoint = Instant::now()
                    + Duration::from_millis(
                        canonical_config
                            .as_ref()
                            .map_or(0, |c| c.state.checkpoint_ms),
                    );
            }
            // At stop after the follower's last events (processed above).
            checkpoint_pass(o, w, due, stopping, replace);
        }
        // BSC3-016 review L5: a finished census reaches the status at once.
        if let Some(done) = census_job.take_if(|j| j.is_finished()) {
            census_refresh.ended(done.join());
        }
        if Instant::now() >= next_census && !stopping {
            next_census = Instant::now() + Duration::from_millis(config.census_ms);
            // A checkpoint of the head: a killed process's window still ends here.
            if let Some((n, h)) = canonical
                .as_ref()
                .filter(|c| c.first_journaled)
                .and_then(|c| c.head)
            {
                journal.record(
                    RecordKind::Canonical,
                    clock.now_ns(),
                    window_payload(b'K', n, &h),
                );
            }
            // The census replays every journal: on its own thread, one at a
            // time, never on this decision loop (Phase F gate M1). A census
            // failure is visible in the log; observation goes on.
            if census_job.as_ref().is_none_or(|j| j.is_finished()) {
                if let Some(done) = census_job.take() {
                    census_refresh.ended(done.join());
                }
                let (config, rundir, active) = (
                    config.clone(),
                    rundir.to_path_buf(),
                    journal.path().to_path_buf(),
                );
                census_job = std::thread::Builder::new()
                    .name("bsc-census".into())
                    .spawn(move || {
                        let written = write_census(&config, &rundir, Some(&active))
                            .and_then(|_| observed_now());
                        if let Err(e) = &written {
                            eprintln!("g04 census: {e}");
                        }
                        written
                    })
                    .map_err(|e| {
                        eprintln!("g04 census thread: {e}");
                        census_refresh.state = Some(format!("FAILED: no census thread: {e}"));
                    })
                    .ok();
            }
        }
        // Phase E review L3: the Four drift guard on its own thread; its
        // verdict is journaled and in the status, a drift makes every Four
        // token derivation UNKNOWN until a probe matches again.
        if let (Some(cc), Some(c)) = (canonical_config.as_ref(), canonical.as_mut()) {
            if let Some(job) = four_job.take_if(|j| j.is_finished()) {
                let current =
                    darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_r12d_131608421();
                let (code, block, verdict) = match job.join() {
                    Ok(Ok(o)) if current.validate(&o.observed, o.block_number).is_ok() => {
                        c.four_drift = false;
                        c.four_validated = current.validate(&o.observed, o.block_number).ok();
                        (
                            1,
                            o.block_number,
                            format!("MATCH at block {}", o.block_number),
                        )
                    }
                    Ok(Ok(o)) => {
                        c.four_drift = true;
                        c.four_validated = None;
                        let at = o.block_number;
                        let drifts: Vec<String> = current
                            .drifts(&o.observed)
                            .iter()
                            .map(ToString::to_string)
                            .collect();
                        (2, at, format!("DRIFT at block {at}: {}", drifts.join("; ")))
                    }
                    Ok(Err(e)) => (3, 0, format!("PROBE_FAILED: {e}")),
                    Err(_) => (3, 0, "PROBE_FAILED: the probe thread panicked".to_owned()),
                };
                c.four_deployment = verdict;
                journal.record(
                    RecordKind::Runtime,
                    clock.now_ns(),
                    crate::runtime_journal::four_probe_payload(code, block),
                );
            }
            if four_job.is_none() && Instant::now() >= next_four && !stopping {
                let (endpoint, limits) = (cc.endpoint_at(c.active).to_string(), cc.limits);
                four_job = std::thread::Builder::new()
                    .name("bsc-four-probe".into())
                    .spawn(move || {
                        crate::venue_probe::probe_four(
                            &endpoint,
                            &limits,
                            &darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_r12d_131608421(),
                        )
                    })
                    .map_err(|e| {
                        eprintln!("four probe thread: {e}");
                        // BSC3-016: shown, not only logged.
                        c.four_deployment = format!("PROBE_FAILED: no probe thread: {e}");
                    })
                    .ok();
                next_four = Instant::now() + Duration::from_millis(cc.four_probe_ms);
            }
        }
        if stopping && adapters.is_empty() {
            drop(ingress);
            // A clean stop disarms the PAPER path (its fence goes) and the
            // LIVE path (its key goes).
            if let Some(x) = exec.as_mut() {
                x.shutdown();
            }
            if let Some(l) = live.as_mut() {
                l.shutdown();
            }
            journal_drops(&mut journal, &dropped, &mut journaled_drops, clock.now_ns());
            journal.record(RecordKind::Runtime, clock.now_ns(), EXIT_PAYLOAD.to_vec());
            journal.finish()?;
            // The stop is consumed (Paket 8 review L7: a stop file left
            // behind stopped the next start at once).
            if let Err(e) = fs::remove_file(&stop_file)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                eprintln!("{}: {e}", stop_file.display());
            }
            // The last periodic census ends before the final one.
            if let Some(done) = census_job.take() {
                let _ = done.join();
            }
            if let Err(e) = write_census(config, rundir, None) {
                eprintln!("g04 census: {e}");
            }
            return Ok(());
        }
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

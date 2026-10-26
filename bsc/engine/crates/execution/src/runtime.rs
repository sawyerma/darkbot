//! The read-only BSC runtime process (C1, order V2 §5/§7.3): it observes the
//! signal sources the native source owner made effective `SHADOW`, verifies
//! and classifies what they deliver in one [`IngressCore`], journals launch
//! sightings and source transitions, and rewrites its status record. No
//! transaction is built, signed or sent anywhere in this program.
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

use crate::canonical_join::{LaunchRef, LaunchTx, Ledger};
use crate::chain_follower::{
    CanonicalEvent, FollowerFault, FollowerLimits, NonceWatch, RequestError, is_unrebuildable,
    run_follower,
};
use crate::g04_census::{Census, census};
use crate::ingress::{Health, IngressCore, Launch, Observation, SourceStats, health};
use crate::runtime_journal::{
    EXIT_PAYLOAD, RuntimeJournal, SourceEvent, canonical_launch_payload, first_sighting_payload,
    later_sighting_payload, orphaned_launch_payload, pending_launch_payload, reorg_payload,
    resync_payload, source_event_payload, window_payload,
};
use crate::sources::{ComponentId, Effective, Generation, SourceId, Store};
use crate::wss::endpoint::Endpoint;
use crate::wss::{DisconnectReason, WssEvent, WssLimits, WssSource, run_source};
use darkbot_bsc_journal::RecordKind;
use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
use darkbot_bsc_record::{Draft, observed_now};
use darkbot_bsc_trading::chain_state::engine::StateLimits;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[path = "runtime_state.rs"]
pub mod state;

use state::{StateConfig, StateOwner};

/// The validated runtime configuration (`bsc/config/ingress.public.env`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// The public WSS endpoint of each source, if it has one.
    pub endpoints: Vec<(SourceId, String)>,
    pub queue_events: usize,
    pub dedup_hashes: usize,
    /// A connected source silent this long is DEGRADED.
    pub quiet_ms: u64,
    pub journal_queue: usize,
    pub status_ms: u64,
    /// Every bound of the WSS adapters, validated by `WssLimits::validate`.
    pub wss: WssLimits,
    /// The canonical chain follower, when `BSC_CANONICAL_RPC` is set.
    pub canonical: Option<CanonicalConfig>,
    /// How often the census is replayed from the journals.
    pub census_ms: u64,
    /// The longest a pass waits for events before it reconciles sources,
    /// joins launches and checks the stop file.
    pub reconcile_ms: u64,
    /// How long the operator waits for a requested stop before it kills the
    /// process; at least the derived worst-case stop path.
    pub stop_timeout_ms: u64,
}

/// The canonical follower's endpoint and bounds (`BSC_CANONICAL_*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalConfig {
    /// `https://` without credentials.
    pub endpoint: String,
    pub limits: FollowerLimits,
    /// A launch is joined this long after its block was seen, so sightings
    /// still queued behind the block count.
    pub join_delay_ms: u64,
    /// The C2 chain-state owner's bounds (it needs the follower).
    pub state: StateConfig,
}

fn bounded(name: &str, value: &str, min: u64, max: u64) -> Result<u64, String> {
    let v: u64 = value
        .parse()
        .map_err(|_| format!("{name} is not an unsigned integer"))?;
    if !(min..=max).contains(&v) {
        return Err(format!("{name}={v} is outside {min}..={max}"));
    }
    Ok(v)
}

impl RuntimeConfig {
    /// Strict: every key known, every bound in range, endpoints `wss://`
    /// without credentials; anything else stops the runtime.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut values: HashMap<&str, &str> = HashMap::new();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("not KEY=VALUE: {line}"))?;
            if values.insert(k.trim(), v.trim()).is_some() {
                return Err(format!("{} is set twice", k.trim()));
            }
        }
        let mut endpoints = Vec::new();
        for source in [SourceId::PublicNode, SourceId::Drpc, SourceId::BlockReq] {
            let key = format!("{}_WSS", source.name());
            if let Some(url) = values.remove(key.as_str()) {
                // The adapter's own rule, so nothing it would refuse starts.
                Endpoint::parse(url).map_err(|e| format!("{key} is refused: {e:?}"))?;
                endpoints.push((source, url.to_string()));
            }
        }
        let canonical_rpc = values.remove("BSC_CANONICAL_RPC");
        let watched = values.remove("BSC_STATE_WATCHED");
        let mut take = |name: &str, min: u64, max: u64| -> Result<u64, String> {
            let v = values
                .remove(name)
                .ok_or_else(|| format!("{name} is required"))?;
            bounded(name, v, min, max)
        };
        let ms = Duration::from_millis;
        let canonical = match canonical_rpc {
            None => None,
            Some(url) => {
                let rest = url
                    .strip_prefix("https://")
                    .ok_or("BSC_CANONICAL_RPC must be https://")?;
                // The follower's own rule (the authority and path rules of wss://).
                Endpoint::parse(&format!("wss://{rest}"))
                    .map_err(|e| format!("BSC_CANONICAL_RPC is refused: {e:?}"))?;
                let limits = FollowerLimits {
                    poll: ms(take("BSC_CANONICAL_POLL_MS", 50, 60_000)?),
                    reorg_depth: take("BSC_CANONICAL_REORG_DEPTH", 1, 10_000)?,
                    max_catch_up: take("BSC_CANONICAL_MAX_CATCH_UP", 1, 100_000)?,
                    connect_timeout: ms(take("BSC_CANONICAL_CONNECT_TIMEOUT_MS", 100, 60_000)?),
                    handshake_timeout: ms(take(
                        "BSC_CANONICAL_HANDSHAKE_TIMEOUT_MS",
                        100,
                        120_000,
                    )?),
                    read_tick: ms(take("BSC_CANONICAL_READ_TICK_MS", 10, 5_000)?),
                    write_timeout: ms(take("BSC_CANONICAL_WRITE_TIMEOUT_MS", 100, 60_000)?),
                    request_timeout: ms(take("BSC_CANONICAL_REQUEST_TIMEOUT_MS", 100, 300_000)?),
                    max_response_bytes: take("BSC_CANONICAL_MAX_RESPONSE_BYTES", 65_536, 1 << 30)?
                        as usize,
                    backoff_initial: ms(take("BSC_CANONICAL_BACKOFF_INITIAL_MS", 100, 60_000)?),
                    backoff_max: ms(take("BSC_CANONICAL_BACKOFF_MAX_MS", 1_000, 3_600_000)?),
                    tx_attempts: take("BSC_CANONICAL_TX_ATTEMPTS", 1, 100)? as u32,
                };
                limits
                    .validate()
                    .map_err(|e| format!("canonical bounds: {e:?}"))?;
                let state = StateConfig {
                    limits: StateLimits {
                        max_txs: take("BSC_STATE_MAX_TXS", 1_024, 1 << 24)? as usize,
                        max_candidates_per_nonce: take(
                            "BSC_STATE_MAX_CANDIDATES_PER_NONCE",
                            1,
                            1_024,
                        )? as usize,
                        max_watched_senders: take("BSC_STATE_MAX_WATCHED_SENDERS", 1, 1_024)?
                            as usize,
                        // The follower's kept window: what it reports exactly
                        // is applied exactly.
                        reorg_window: limits.reorg_depth,
                        finality_depth: take("BSC_STATE_FINALITY_DEPTH", 1, 10_000)?,
                    },
                    watched: StateConfig::parse_watched(
                        watched.ok_or("BSC_STATE_WATCHED is required")?,
                    )?,
                    checkpoint_ms: take("BSC_STATE_CHECKPOINT_MS", 1_000, 3_600_000)?,
                    nonce_fact_blocks: take("BSC_STATE_NONCE_FACT_BLOCKS", 1, 100_000)?,
                };
                state.validate()?;
                Some(CanonicalConfig {
                    endpoint: url.to_string(),
                    limits,
                    join_delay_ms: take("BSC_CANONICAL_JOIN_DELAY_MS", 0, 600_000)?,
                    state,
                })
            }
        };
        if canonical.is_none() && watched.is_some() {
            return Err("BSC_STATE_WATCHED needs BSC_CANONICAL_RPC".into());
        }
        let wss = WssLimits {
            connect_timeout: ms(take("BSC_RUNTIME_CONNECT_TIMEOUT_MS", 100, 60_000)?),
            handshake_timeout: ms(take("BSC_RUNTIME_HANDSHAKE_TIMEOUT_MS", 100, 120_000)?),
            read_tick: ms(take("BSC_RUNTIME_READ_TICK_MS", 10, 5_000)?),
            write_timeout: ms(take("BSC_RUNTIME_WRITE_TIMEOUT_MS", 100, 60_000)?),
            idle_timeout: ms(take("BSC_RUNTIME_IDLE_TIMEOUT_MS", 1_000, 3_600_000)?),
            max_frame_bytes: take("BSC_RUNTIME_MAX_FRAME_BYTES", 1_024, 16 << 20)? as usize,
            max_message_bytes: take("BSC_RUNTIME_MAX_MESSAGE_BYTES", 65_536, 16 << 20)? as usize,
            backoff_initial: ms(take("BSC_RUNTIME_BACKOFF_INITIAL_MS", 100, 60_000)?),
            backoff_max: ms(take("BSC_RUNTIME_BACKOFF_MAX_MS", 1_000, 3_600_000)?),
            stable_after: ms(take("BSC_RUNTIME_STABLE_AFTER_MS", 1_000, 3_600_000)?),
        };
        let config = RuntimeConfig {
            endpoints,
            queue_events: take("BSC_RUNTIME_QUEUE_EVENTS", 64, 1 << 22)? as usize,
            dedup_hashes: take("BSC_RUNTIME_DEDUP_HASHES", 1024, 1 << 24)? as usize,
            quiet_ms: take("BSC_RUNTIME_QUIET_MS", 1000, 600_000)?,
            journal_queue: take("BSC_RUNTIME_JOURNAL_QUEUE", 16, 1 << 20)? as usize,
            status_ms: take("BSC_RUNTIME_STATUS_MS", 100, 60_000)?,
            wss,
            canonical,
            census_ms: take("BSC_RUNTIME_CENSUS_MS", 1_000, 86_400_000)?,
            reconcile_ms: take("BSC_RUNTIME_RECONCILE_MS", 10, 60_000)?,
            // Below the launchd ExitTimeOut (330 s) of the runtime job in
            // ops/bsc-control.sh, so a SIGTERM stop is never cut by SIGKILL.
            stop_timeout_ms: take("BSC_RUNTIME_STOP_TIMEOUT_MS", 1_000, 300_000)?,
        };
        config
            .wss
            .validate()
            .map_err(|e| format!("WSS bounds: {e:?}"))?;
        let needed = config.stop_bound_ms();
        if config.stop_timeout_ms < needed {
            return Err(format!(
                "BSC_RUNTIME_STOP_TIMEOUT_MS={} is below the derived stop path of {needed} ms",
                config.stop_timeout_ms
            ));
        }
        if let Some(unknown) = values.keys().next() {
            return Err(format!("unknown key {unknown}"));
        }
        Ok(config)
    }

    /// The worst-case stop path: one pass, then the adapters and the
    /// follower in parallel (a TCP connect in flight, then the closing write
    /// or one more read tick). Name resolution has no bound of its own.
    pub fn stop_bound_ms(&self) -> u64 {
        let ms = |d: Duration| u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
        let w = &self.wss;
        let adapters = ms(w.connect_timeout)
            .saturating_add(ms(w.write_timeout))
            .saturating_add(ms(w.read_tick));
        let follower = self.canonical.as_ref().map_or(0, |c| {
            ms(c.limits.connect_timeout).saturating_add(ms(c.limits.read_tick))
        });
        self.reconcile_ms.saturating_add(adapters.max(follower))
    }

    fn endpoint(&self, source: SourceId) -> Option<&str> {
        self.endpoints
            .iter()
            .find(|(s, _)| *s == source)
            .map(|(_, u)| u.as_str())
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
    Disconnected {
        rate_limited: bool,
    },
}

struct Adapter {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
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

/// The follower thread and its stop flag.
struct Follower {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

/// Every canonical event crosses to the runtime with its time on the
/// runtime clock (a block: when the follower first had its header). The
/// send blocks: the follower falls behind rather than losing an event.
fn spawn_follower(
    config: CanonicalConfig,
    resume: Vec<(u64, [u8; 32])>,
    tx: SyncSender<(u64, CanonicalEvent)>,
    clock: Clock,
) -> Result<Follower, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let handle = thread::Builder::new()
        .name("bsc-canonical".into())
        .spawn(move || {
            let mut sink = |event: CanonicalEvent| {
                let at = match &event {
                    CanonicalEvent::Block { observed, .. } => clock.ns_at(*observed),
                    _ => clock.now_ns(),
                };
                // Err only once the runtime dropped the receiver at stop.
                let _ = tx.send((at, event));
            };
            // The watched senders' nonce facts (the C2 owner may be switched
            // on at any time; without it they are not applied).
            let watch = NonceWatch {
                senders: config.state.watched.clone(),
                every_blocks: config.state.nonce_fact_blocks,
            };
            if let Err(e) = run_follower(
                &config.endpoint,
                &config.limits,
                &resume,
                &watch,
                &flag,
                &mut sink,
            ) {
                eprintln!("canonical follower refused its configuration: {e:?}");
            }
        })
        .map_err(|e| format!("spawn canonical follower: {e}"))?;
    Ok(Follower { stop, handle })
}

/// The runtime's view of the canonical chain and the launch join.
struct Canonical {
    state: &'static str,
    head: Option<(u64, [u8; 32])>,
    head_seen_ns: Option<u64>,
    /// The head block's time (header ms) and the local wall ms when the
    /// follower first held it.
    head_block_ms: Option<u64>,
    head_seen_unix_ms: Option<u64>,
    /// The observation window starts with the first block mined at or
    /// after this wall time (ms): this process's start. Earlier blocks (a
    /// C2 resume backfill, or a head mined before the sources connected)
    /// and their launches, reorgs and gaps are outside it.
    window_from_ms: u64,
    first_journaled: bool,
    backfill_blocks: u64,
    backfill_launches: u64,
    blocks_skipped: u64,
    blocks: u64,
    reorgs: u64,
    resyncs: u64,
    rpc_errors: u64,
    rate_limited: u64,
    ledger: Ledger,
}

impl Canonical {
    fn new(config: &CanonicalConfig, window_from_ms: u64) -> Self {
        Canonical {
            state: "CONNECTING",
            head: None,
            head_seen_ns: None,
            head_block_ms: None,
            head_seen_unix_ms: None,
            window_from_ms,
            first_journaled: false,
            backfill_blocks: 0,
            backfill_launches: 0,
            blocks_skipped: 0,
            blocks: 0,
            reorgs: 0,
            resyncs: 0,
            rpc_errors: 0,
            rate_limited: 0,
            ledger: Ledger::new(
                config.join_delay_ms.saturating_mul(1_000_000),
                config.limits.reorg_depth,
            ),
        }
    }

    fn orphan(&mut self, orphaned: &[(u64, [u8; 32])], at: u64, journal: &mut RuntimeJournal) {
        let hashes: Vec<[u8; 32]> = orphaned.iter().map(|(_, h)| *h).collect();
        for l in self.ledger.orphan(&hashes) {
            journal.record(RecordKind::Canonical, at, &orphaned_launch_payload(&l));
        }
    }

    fn event(&mut self, at: u64, event: CanonicalEvent, journal: &mut RuntimeJournal) {
        match event {
            CanonicalEvent::Block {
                number,
                hash,
                timestamp_ms,
                observed_unix_ms,
                ..
            } => {
                self.state = "FOLLOWING";
                self.head = Some((number, hash));
                self.head_seen_ns = Some(at);
                self.head_block_ms = Some(timestamp_ms);
                self.head_seen_unix_ms = observed_unix_ms;
                if !self.first_journaled {
                    if timestamp_ms < self.window_from_ms {
                        self.backfill_blocks += 1;
                        return;
                    }
                    self.first_journaled = true;
                    journal.record(
                        RecordKind::Canonical,
                        at,
                        &window_payload(b'H', number, &hash),
                    );
                }
                self.blocks += 1;
            }
            // Before the window: not part of the G0.4 counts (the C2 owner
            // has applied them already).
            CanonicalEvent::Launch(_) if !self.first_journaled => self.backfill_launches += 1,
            CanonicalEvent::Reorg { .. }
            | CanonicalEvent::ReorgTooDeep { .. }
            | CanonicalEvent::Gap { .. }
                if !self.first_journaled => {}
            CanonicalEvent::Launch(l) => {
                // Its block is the last one reported (launches follow it).
                let same = self.head.is_some_and(|(_, h)| h == l.block_hash);
                let launch = LaunchRef {
                    venue: l.venue,
                    block_number: l.block_number,
                    block_hash: l.block_hash,
                    tx_index: l.tx_index,
                    tx_hash: l.tx_hash,
                    token: l.token,
                    external_buys: l.external_buys_after,
                    c1_scope: l.c1_scope,
                    tx: match l.unverifiable {
                        None => LaunchTx::Verified,
                        Some(r) if is_unrebuildable(r) => LaunchTx::Unrebuildable,
                        Some(_) => LaunchTx::AnswerRefused,
                    },
                    block_ms: match self.head_block_ms {
                        Some(ms) if same => ms,
                        // Never reached: the follower reports a block before its launches.
                        _ => return,
                    },
                    block_seen_ns: self.head_seen_ns.filter(|_| same).unwrap_or(at),
                    block_seen_unix_ms: self.head_seen_unix_ms.filter(|_| same),
                };
                journal.record(RecordKind::Canonical, at, &pending_launch_payload(&launch));
                self.ledger.push(launch);
            }
            CanonicalEvent::Reorg { orphaned, new_head } => {
                self.reorgs += 1;
                let depth = u32::try_from(orphaned.len()).unwrap_or(u32::MAX);
                journal.record(
                    RecordKind::Reorg,
                    at,
                    &reorg_payload(depth, new_head.0, &new_head.1),
                );
                self.orphan(&orphaned, at, journal);
            }
            CanonicalEvent::ReorgTooDeep { orphaned, new_head } => {
                // The new chain below its head is not followed: those heights
                // are skipped (the orphaned ones are counted as orphaned).
                self.resyncs += 1;
                let from = orphaned.first().map_or(new_head.0, |(n, _)| *n);
                if new_head.0 > from {
                    self.blocks_skipped += new_head.0 - from;
                    journal.record(RecordKind::Reorg, at, &resync_payload(from, new_head.0 - 1));
                }
                self.orphan(&orphaned, at, journal);
            }
            CanonicalEvent::Gap { from, to } => {
                self.resyncs += 1;
                if to >= from {
                    self.blocks_skipped += to - from + 1;
                }
                journal.record(RecordKind::Reorg, at, &resync_payload(from, to));
            }
            // The C2 owner's reconciliation input; no G0.4 count.
            CanonicalEvent::NonceFact { .. } => {}
            CanonicalEvent::Fault(fault) => {
                self.rpc_errors += 1;
                let limited = matches!(
                    fault,
                    FollowerFault::Request {
                        error: RequestError::RateLimited(_),
                        ..
                    }
                );
                self.rate_limited += u64::from(limited);
                self.state = if limited { "RATE_LIMITED" } else { "FAILING" };
            }
        }
    }

    /// Joins the launches due at `now` (all of them with `u64::MAX`).
    fn join(&mut self, now: u64, core: &IngressCore, journal: &mut RuntimeJournal) {
        let head = self.head.map_or(0, |(n, _)| n);
        for j in self.ledger.join_due(now, head, |h| core.sighting(h)) {
            journal.record(
                RecordKind::Canonical,
                j.launch.block_seen_ns,
                &canonical_launch_payload(&j),
            );
        }
    }

    fn draft(&self, now: u64) -> Draft {
        use rs::canonical as e;
        let c = self.ledger.counters();
        let mut d = Draft::new(&e::SCHEMA);
        d.set(e::STATE, self.state)
            .set(e::BLOCKS, self.blocks)
            .set(e::REORGS, self.reorgs)
            .set(e::RESYNCS, self.resyncs)
            .set(e::RPC_ERRORS, self.rpc_errors)
            .set(e::RATE_LIMITED, self.rate_limited)
            .set(e::FLAP_LAUNCHES, c.flap_launches)
            .set(e::FOUR_LAUNCHES, c.four_launches)
            .set(e::ORPHANED_LAUNCHES, c.orphaned_launches)
            .set(e::SEEN_FULL_BEFORE, c.seen_full_before)
            .set(e::SEEN_HASH_ONLY_BEFORE, c.seen_hash_only_before)
            .set(e::SEEN_AFTER, c.seen_after)
            .set(e::MISSED, c.missed)
            .set(e::EXTERNAL_BUYS, c.external_buys)
            .set(e::BUYS_UNKNOWN, c.buys_unknown)
            .set(e::CLOCK_UNKNOWN, c.clock_unknown)
            .set(e::TX_UNVERIFIABLE, c.tx_unverifiable)
            .set(e::TX_ANSWER_REFUSED, c.tx_answer_refused)
            .set(e::C1_SCOPE_LAUNCHES, c.c1_scope_launches)
            .set(e::C1_SCOPE_FULL_BEFORE, c.c1_scope_full_before)
            .set(e::BLOCKS_SKIPPED, self.blocks_skipped)
            .set(e::WAITING, self.ledger.waiting() as u64)
            .set(e::BACKFILL_BLOCKS, self.backfill_blocks)
            .set(e::BACKFILL_LAUNCHES, self.backfill_launches);
        if let Some((n, _)) = self.head {
            d.set(e::HEAD, n);
        }
        let lag = self
            .head_seen_unix_ms
            .zip(self.head_block_ms)
            .and_then(|(seen, block)| {
                i64::try_from(seen)
                    .ok()?
                    .checked_sub(i64::try_from(block).ok()?)
            });
        if let Some(lag) = lag {
            d.set(e::HEAD_FETCH_LAG_MS, lag);
        }
        if let Some(at) = self.head_seen_ns {
            d.set(e::HEAD_AGE_MS, now.saturating_sub(at) / 1_000_000);
        }
        d
    }
}

fn spawn_adapter(
    source: SourceId,
    url: String,
    limits: WssLimits,
    tx: SyncSender<(SourceId, u64, Event)>,
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
            let mut sink = |event: WssEvent| {
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
                if let Err(TrySendError::Full(_)) = tx.try_send((source, clock.now_ns(), event)) {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            };
            if let Err(e) = run_source(&wss, &limits, &flag, &mut sink) {
                // Refused before any connect: visible in the log and as a disconnect.
                eprintln!("{}: source refused its configuration: {e:?}", source.name());
                sink(WssEvent::Disconnected(DisconnectReason::Stopped));
            }
        })
        .map_err(|e| format!("spawn {} adapter: {e}", source.name()))?;
    Ok(Adapter { stop, handle })
}

/// Writes `bytes` to `path` atomically (temp file in the same directory).
fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    let _ = fs::remove_file(&tmp);
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| std::io::Write::write_all(&mut f, bytes).and_then(|()| f.sync_all()))
        .and_then(|()| fs::rename(&tmp, path));
    written.map_err(|e| format!("{}: {e}", path.display()))
}

/// The payload bound of the runtime journal; the census reads with the same.
pub fn journal_max_payload(config: &RuntimeConfig) -> usize {
    config.wss.max_message_bytes + 64
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
    canonical: Option<&Canonical>,
    owner: Option<&StateOwner>,
    status_interval_ms: u64,
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
        .set(rs::MODE, "READ_ONLY")
        .set(rs::STATUS_INTERVAL_MS, status_interval_ms);
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
            .set(e::LAUNCH_FIRST, s.launch_first);
        if let Some(at) = s.last_event_ns {
            row.set(e::LAST_EVENT_AGE_MS, now.saturating_sub(at) / 1_000_000);
        }
        d.push(rs::SOURCES, row.into_nested());
    }
    if let Some(c) = canonical {
        d.set(rs::CANONICAL, c.draft(now).into_nested());
    }
    if let Some(o) = owner {
        d.set(rs::STATE, o.draft().into_nested());
    }
    d.encode().map_err(|e| e.to_string())
}

/// The stop request of the runtime in `rundir` (written by `darkbotctl` or
/// by the process's own SIGTERM/SIGINT handler).
pub fn stop_file(rundir: &Path) -> PathBuf {
    rundir.join("bsc.runtime.stop")
}

/// Runs until `rundir/bsc.runtime.stop` exists.
pub fn run(config: &RuntimeConfig, sources_dir: &Path, rundir: &Path) -> Result<(), String> {
    let store = Store::open(sources_dir, 64)?;
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
        config.journal_queue,
        journal_max_payload(config),
    )?;
    let journal_rel: PathBuf = journal
        .path()
        .strip_prefix(rundir)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| journal.path().to_path_buf());
    let limits = config.wss;
    let (tx, rx): (SyncSender<(SourceId, u64, Event)>, Receiver<_>) =
        sync_channel(config.queue_events);
    let dropped: Vec<Arc<AtomicU64>> = SourceId::ALL
        .iter()
        .map(|_| Arc::new(AtomicU64::new(0)))
        .collect();
    let mut core = IngressCore::new(config.dedup_hashes);
    let mut adapters: HashMap<usize, Adapter> = HashMap::new();
    let mut rate_limited_last = [false; 5];
    let mut generation = store.current()?;
    let (canonical_tx, canonical_rx) = sync_channel::<(u64, CanonicalEvent)>(config.queue_events);
    let canonical_rx = Some(canonical_rx);
    let mut canonical = config
        .canonical
        .as_ref()
        .map(|c| Canonical::new(c, started_ms));
    let checkpoint_path = dir.join("c2-checkpoint.bin");
    let c2_wanted =
        |g: &Generation| g.component(ComponentId::StateC2).effective == Effective::Shadow;
    // The C2 owner restores its window at start; the follower resumes after it.
    let mut owner = match &config.canonical {
        Some(c) if c2_wanted(&generation) => {
            Some(StateOwner::restore(&c.state, checkpoint_path.clone())?)
        }
        _ => None,
    };
    let mut follower = match &config.canonical {
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
    let mut next_checkpoint = Instant::now()
        + Duration::from_millis(
            config
                .canonical
                .as_ref()
                .map_or(0, |c| c.state.checkpoint_ms),
        );
    let mut next_census = Instant::now() + Duration::from_millis(config.census_ms);
    let mut next_status = Instant::now();
    let mut next_reconcile = Instant::now();
    let reconcile_every = Duration::from_millis(config.reconcile_ms);
    let status_every = Duration::from_millis(config.status_ms);
    let index = |s: SourceId| SourceId::ALL.iter().position(|x| *x == s).unwrap_or(0);
    loop {
        let stopping = stop_file.exists();
        // One fresh monotonic/wall pair per pass for the sightings' wall time.
        let wall = WallPair::read(clock);
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
                if let Some(c) = &config.canonical {
                    match (c2_wanted(&generation), owner.is_some()) {
                        (true, false) => {
                            owner = Some(StateOwner::fresh(&c.state, checkpoint_path.clone())?);
                        }
                        (false, true) => {
                            // Its last window stays the checkpoint for the next start.
                            if let Some(mut o) = owner.take() {
                                o.checkpoint(replace);
                            }
                        }
                        _ => {}
                    }
                }
            }
            for source in SourceId::ALL {
                let i = index(source);
                let want = !stopping
                    && generation.source(source).effective == Effective::Shadow
                    && config.endpoint(source).is_some();
                match (want, adapters.contains_key(&i)) {
                    (true, false) => {
                        let url = config.endpoint(source).unwrap_or_default().to_string();
                        let a = spawn_adapter(
                            source,
                            url,
                            limits,
                            tx.clone(),
                            Arc::clone(&dropped[i]),
                            clock,
                        )?;
                        adapters.insert(i, a);
                        journal.record(
                            RecordKind::Runtime,
                            clock.now_ns(),
                            &source_event_payload(source, SourceEvent::Started),
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
                            &source_event_payload(source, SourceEvent::Stopped),
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
                            &source_event_payload(source, SourceEvent::Stopped),
                        );
                    }
                    _ => {}
                }
            }
        }
        // Events, bounded per pass so status and stop stay timely.
        let deadline = Instant::now() + reconcile_every;
        while Instant::now() < deadline {
            let wait = deadline.saturating_duration_since(Instant::now());
            let (source, at, event) = match rx.recv_timeout(wait) {
                Ok(e) => e,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            let observation = match &event {
                Event::Full { hash, raw, .. } => Observation::Full { hash: *hash, raw },
                Event::HashOnly { hash } => Observation::HashOnly { hash: *hash },
                Event::Rejected => Observation::Rejected,
                Event::Connected => {
                    rate_limited_last[index(source)] = false;
                    journal.record(
                        RecordKind::Runtime,
                        at,
                        &source_event_payload(source, SourceEvent::Connected),
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
                    journal.record(RecordKind::Runtime, at, &source_event_payload(source, kind));
                    Observation::Disconnected {
                        rate_limited: *rate_limited,
                    }
                }
            };
            let decided = core.observe(source, observation, at, wall.wall_us(at));
            if let (
                Some(o),
                Event::Full {
                    hash, from, nonce, ..
                },
            ) = (owner.as_mut(), &event)
                && (decided.is_some() || o.watches(from))
            {
                // Source lanes are indexed as in `SourceId::ALL` (five).
                o.pending(*hash, *from, *nonce, index(source) as u8, at);
            }
            match decided {
                Some(Launch::First { hash, source, .. }) => {
                    if let Event::Full { raw, .. } = &event {
                        journal.record(
                            RecordKind::RawSignedTx,
                            at,
                            &first_sighting_payload(source, &hash, raw),
                        );
                    }
                }
                Some(Launch::Later {
                    hash,
                    source,
                    lag_ns,
                }) => {
                    journal.record(
                        RecordKind::Runtime,
                        at,
                        &later_sighting_payload(source, &hash, lag_ns),
                    );
                }
                None => {}
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
                        Ok((at, event)) => {
                            if let Some(o) = owner.as_mut() {
                                o.canonical(&event);
                            }
                            c.event(at, event, &mut journal)
                        }
                        Err(RecvTimeoutError::Timeout) if !f.handle.is_finished() => {}
                        Err(_) => break,
                    }
                }
                let _ = f.handle.join();
            }
            while let Ok((at, event)) = crx.try_recv() {
                if let Some(o) = owner.as_mut() {
                    o.canonical(&event);
                }
                c.event(at, event, &mut journal);
            }
            let now = if stopping { u64::MAX } else { clock.now_ns() };
            c.join(now, &core, &mut journal);
            if stopping && c.state != "STOPPED" {
                // A window ends only if it started (a block mined after the start).
                if let Some((n, h)) = c.head.filter(|_| c.first_journaled) {
                    journal.record(
                        RecordKind::Canonical,
                        clock.now_ns(),
                        &window_payload(b'E', n, &h),
                    );
                }
                c.state = "STOPPED";
            } else if follower.as_ref().is_some_and(|f| f.handle.is_finished()) {
                c.state = "FAILING";
            }
        }
        if Instant::now() >= next_status || stopping {
            next_status = Instant::now() + status_every;
            let now = clock.now_ns();
            let stats: Vec<_> = SourceId::ALL
                .iter()
                .map(|&s| {
                    let i = index(s);
                    let mut st = core.stats(s);
                    st.dropped += dropped[i].load(Ordering::Relaxed);
                    let h = match generation.source(s).effective {
                        Effective::Off | Effective::Blocked(_) => Health::Off,
                        Effective::Shadow if config.endpoint(s).is_none() => Health::Unqualified,
                        Effective::Shadow if stopping => Health::Off,
                        Effective::Shadow => {
                            health(&st, rate_limited_last[i], now, config.quiet_ms * 1_000_000)
                        }
                    };
                    (s, st, h)
                })
                .collect();
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
                canonical.as_ref(),
                owner.as_ref(),
                config.status_ms,
            )?;
            replace(&status_path, &bytes)?;
        }
        if let Some(o) = owner.as_mut()
            && (stopping || Instant::now() >= next_checkpoint)
        {
            // At stop after the follower's last events (processed above).
            next_checkpoint = Instant::now()
                + Duration::from_millis(
                    config
                        .canonical
                        .as_ref()
                        .map_or(0, |c| c.state.checkpoint_ms),
                );
            o.checkpoint(replace);
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
                    &window_payload(b'K', n, &h),
                );
            }
            // A census failure is visible in the log; observation goes on.
            if let Err(e) = write_census(config, rundir, Some(journal.path())) {
                eprintln!("g04 census: {e}");
            }
        }
        if stopping && adapters.is_empty() {
            drop(tx);
            journal.record(RecordKind::Runtime, clock.now_ns(), &EXIT_PAYLOAD);
            journal.finish()?;
            if let Err(e) = write_census(config, rundir, None) {
                eprintln!("g04 census: {e}");
            }
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = include_str!("../../../../config/ingress.public.env");

    #[test]
    fn the_config_is_strict() {
        let c = RuntimeConfig::parse(CONFIG).unwrap();
        assert_eq!(
            c.endpoint(SourceId::PublicNode),
            Some("wss://bsc-rpc.publicnode.com")
        );
        assert_eq!(c.endpoint(SourceId::Drpc), Some("wss://bsc.drpc.org"));
        assert_eq!(
            c.endpoint(SourceId::BlockReq),
            None,
            "no endpoint: UNQUALIFIED"
        );
        assert_eq!((c.reconcile_ms, c.stop_timeout_ms), (500, 15_000));
        // 500 + max(5000 + 5000 + 200, 5000 + 200)
        assert_eq!(c.stop_bound_ms(), 10_700);
        let canonical = c.canonical.as_ref().unwrap();
        assert_eq!(canonical.endpoint, "https://bsc-dataseed.bnbchain.org");
        assert_eq!(canonical.limits.reorg_depth, 64);
        // The C2 window is the follower's; no own account in READ_ONLY.
        assert_eq!(canonical.state.limits.reorg_window, 64);
        assert_eq!(canonical.state.limits.finality_depth, 16);
        assert!(canonical.state.watched.is_empty());
        let one = CONFIG.replace(
            "BSC_STATE_WATCHED=\n",
            "BSC_STATE_WATCHED=0x00000000000000000000000000000000000000aB, 0x0000000000000000000000000000000000000001\n",
        );
        let watched = RuntimeConfig::parse(&one)
            .unwrap()
            .canonical
            .unwrap()
            .state
            .watched;
        assert_eq!(
            (watched.len(), watched[0][19], watched[1][19]),
            (2, 0xab, 1)
        );
        // Without the endpoint the canonical and C2 bounds are unknown keys.
        assert!(RuntimeConfig::parse(&CONFIG.replace("BSC_CANONICAL_RPC=", "#")).is_err());
        let without: String = CONFIG
            .lines()
            .filter(|l| !l.starts_with("BSC_CANONICAL_") && !l.starts_with("BSC_STATE_"))
            .map(|l| format!("{l}\n"))
            .collect();
        assert_eq!(RuntimeConfig::parse(&without).unwrap().canonical, None);
        assert!(RuntimeConfig::parse(&format!("{without}BSC_STATE_WATCHED=\n")).is_err());
        for bad in [
            CONFIG.replace("wss://bsc-rpc", "ws://bsc-rpc"),
            CONFIG.replace("wss://bsc-rpc", "wss://user:pw@bsc-rpc"),
            CONFIG.replace("QUEUE_EVENTS=65536", "QUEUE_EVENTS=1"),
            CONFIG.replace("BSC_RUNTIME_STATUS_MS=1000\n", ""),
            format!("{CONFIG}BSC_RUNTIME_SURPRISE=1\n"),
            format!("{CONFIG}BSC_RUNTIME_QUIET_MS=5000\n"),
            CONFIG
                .replace("BACKOFF_INITIAL_MS=1000", "BACKOFF_INITIAL_MS=60000")
                .replace("BACKOFF_MAX_MS=60000", "BACKOFF_MAX_MS=1000"),
            CONFIG.replace("MAX_FRAME_BYTES=1048576", "MAX_FRAME_BYTES=2097152"),
            CONFIG.replace("https://bsc-dataseed", "http://bsc-dataseed"),
            CONFIG.replace("BSC_CANONICAL_JOIN_DELAY_MS=2000\n", ""),
            CONFIG.replace("BACKOFF_MAX_MS=30000", "BACKOFF_MAX_MS=100"),
            CONFIG.replace("BSC_RUNTIME_CENSUS_MS=60000\n", ""),
            // What the adapters would refuse is refused here (review LOW-5).
            CONFIG.replace("wss://bsc.drpc.org", "wss://bsc.drpc.org?key=1"),
            CONFIG.replace("wss://bsc.drpc.org", "wss://bsc.drpc.org:99999"),
            CONFIG.replace("wss://bsc.drpc.org", "wss://bsc_rpc.example"),
            CONFIG.replace(
                "https://bsc-dataseed.bnbchain.org",
                "https://bsc-dataseed.bnbchain.org?k=1",
            ),
            // A stop wait below the derived stop path.
            CONFIG.replace("STOP_TIMEOUT_MS=15000", "STOP_TIMEOUT_MS=1000"),
            CONFIG.replace("BSC_RUNTIME_RECONCILE_MS=500\n", ""),
            // C2: final only inside the reorg window; a slot no larger than
            // the table; every key required; watched accounts exact, once.
            CONFIG.replace("BSC_STATE_FINALITY_DEPTH=16", "BSC_STATE_FINALITY_DEPTH=65"),
            CONFIG.replace("BSC_STATE_MAX_TXS=262144", "BSC_STATE_MAX_TXS=1024")
                .replace("CANDIDATES_PER_NONCE=16", "CANDIDATES_PER_NONCE=1024")
                .replace("BSC_STATE_MAX_TXS=1024", "BSC_STATE_MAX_TXS=1023"),
            CONFIG.replace("BSC_STATE_CHECKPOINT_MS=30000\n", ""),
            CONFIG.replace("NONCE_FACT_BLOCKS=20", "NONCE_FACT_BLOCKS=0"),
            CONFIG.replace("BSC_STATE_WATCHED=\n", ""),
            CONFIG.replace("BSC_STATE_WATCHED=\n", "BSC_STATE_WATCHED=0x12\n"),
            CONFIG.replace(
                "BSC_STATE_WATCHED=\n",
                "BSC_STATE_WATCHED=00000000000000000000000000000000000000000001\n",
            ),
            CONFIG.replace(
                "BSC_STATE_WATCHED=\n",
                "BSC_STATE_WATCHED=0x000000000000000000000000000000000000000g\n",
            ),
            CONFIG.replace(
                "BSC_STATE_WATCHED=\n",
                "BSC_STATE_WATCHED=0x0000000000000000000000000000000000000001,0x0000000000000000000000000000000000000001\n",
            ),
            CONFIG.replace("MAX_WATCHED_SENDERS=8", "MAX_WATCHED_SENDERS=1").replace(
                "BSC_STATE_WATCHED=\n",
                "BSC_STATE_WATCHED=0x0000000000000000000000000000000000000001,0x0000000000000000000000000000000000000002\n",
            ),
        ] {
            assert!(RuntimeConfig::parse(&bad).is_err(), "{bad}");
        }
    }

    /// The follower's events, joined with the sightings, reach the status
    /// counters, the journal and the census alike; a reorg takes its
    /// launches out of all three.
    #[test]
    fn canonical_events_join_into_status_journal_and_census() {
        use crate::chain_follower::{CanonicalLaunch, Venue};
        use crate::wss::ProtocolFault;
        const BLOCK_MS: u64 = 1_792_000_000_000;
        let base = std::env::temp_dir().join(format!("bsc-runtime-canon-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let config = RuntimeConfig::parse(CONFIG).unwrap();
        let cc = config.canonical.clone().unwrap();
        let block_ms = |n: u8| BLOCK_MS + u64::from(n) * 450;
        // The process started at block 10's time.
        let mut c = Canonical::new(&cc, block_ms(10));
        let mut journal = RuntimeJournal::open(&base, 1, 64, journal_max_payload(&config)).unwrap();
        let mut core = IngressCore::new(64);
        let clock = Clock(Instant::now());
        let block = |n: u8| CanonicalEvent::Block {
            number: u64::from(n),
            hash: [n; 32],
            parent_hash: [n - 1; 32],
            timestamp_s: block_ms(n) / 1_000,
            timestamp_ms: block_ms(n),
            txs: Vec::new(),
            observed: clock.0,
            // The follower held the header 300 ms after the block time.
            observed_unix_ms: Some(block_ms(n) + 300),
        };
        let launch = |n: u8, tx: u8, buys, c1_scope| {
            CanonicalEvent::Launch(CanonicalLaunch {
                venue: Venue::Flap,
                block_number: u64::from(n),
                block_hash: [n; 32],
                tx_index: 0,
                tx_hash: [tx; 32],
                token: Some([tx; 20]),
                creator: None,
                external_buys_after: buys,
                first_external_buy_index: None,
                sender: Some([tx; 20]),
                nonce: Some(u64::from(tx)),
                c1_scope,
                unverifiable: None,
            })
        };
        // Launch 1: a hash 50 ms before its block time; launch 2: a full
        // sighting 100 ms after its block time but before the header fetch
        // (counted after, never before); launch 3: unseen.
        core.observe(
            SourceId::PublicNode,
            Observation::HashOnly { hash: [1; 32] },
            5,
            Some((block_ms(10) - 50) * 1_000),
        );
        core.observe(
            SourceId::PublicNode,
            Observation::HashOnly { hash: [2; 32] },
            6,
            Some((block_ms(11) + 100) * 1_000),
        );
        journal.record(
            RecordKind::Runtime,
            1,
            &source_event_payload(SourceId::PublicNode, SourceEvent::Connected),
        );
        // Mined before the process started (a C2 resume backfill): its
        // launch, the gap before it and the block are outside the window.
        c.event(9, CanonicalEvent::Gap { from: 5, to: 8 }, &mut journal);
        c.event(9, block(9), &mut journal);
        c.event(9, launch(9, 9, Some(0), true), &mut journal);
        assert_eq!(
            (
                c.backfill_blocks,
                c.backfill_launches,
                c.blocks,
                c.resyncs,
                c.blocks_skipped
            ),
            (1, 1, 0, 0, 0)
        );
        assert_eq!(c.ledger.waiting(), 0);
        c.event(10, block(10), &mut journal);
        c.event(10, launch(10, 1, Some(2), true), &mut journal);
        // A launch whose transaction the verifier refuses (e.g. a type it
        // does not rebuild): a counted per-launch fact, never a stall.
        let CanonicalEvent::Launch(mut refused) = launch(10, 4, Some(0), false) else {
            unreachable!()
        };
        refused.sender = None;
        refused.nonce = None;
        refused.unverifiable = Some(crate::wss::RejectReason::UnsupportedType(4));
        c.event(10, CanonicalEvent::Launch(refused), &mut journal);
        // A Four launch whose provider answers were refused every time:
        // counted for every venue.
        let CanonicalEvent::Launch(mut four) = launch(10, 5, None, false) else {
            unreachable!()
        };
        four.venue = Venue::Four;
        four.sender = None;
        four.nonce = None;
        four.unverifiable = Some(crate::wss::RejectReason::HashMismatch);
        c.event(10, CanonicalEvent::Launch(four), &mut journal);
        c.event(20, block(11), &mut journal);
        c.event(20, launch(11, 2, None, false), &mut journal);
        c.event(20, launch(11, 3, Some(0), true), &mut journal);
        assert_eq!(c.ledger.waiting(), 5);
        c.event(
            25,
            CanonicalEvent::Fault(FollowerFault::Request {
                call: crate::chain_follower::Call::BlockNumber,
                error: RequestError::Transport(DisconnectReason::Protocol(
                    ProtocolFault::MalformedJson,
                )),
            }),
            &mut journal,
        );
        assert_eq!(c.state, "FAILING");
        c.event(30, block(12), &mut journal);
        // Not yet due (join delay), then due.
        c.join(30, &core, &mut journal);
        assert_eq!(c.ledger.counters().flap_launches, 0);
        c.join(u64::MAX, &core, &mut journal);
        let k = c.ledger.counters();
        assert_eq!(
            (
                k.flap_launches,
                k.seen_hash_only_before,
                k.seen_after,
                k.missed,
                k.external_buys,
                k.buys_unknown,
                k.c1_scope_launches,
                k.tx_unverifiable,
                k.tx_answer_refused,
                k.four_launches
            ),
            (4, 1, 1, 2, 2, 1, 2, 1, 1, 1)
        );
        c.event(
            40,
            CanonicalEvent::Reorg {
                orphaned: vec![(11, [11; 32]), (12, [12; 32])],
                new_head: (12, [0xEE; 32]),
            },
            &mut journal,
        );
        c.event(50, CanonicalEvent::Gap { from: 13, to: 99 }, &mut journal);
        let k = c.ledger.counters();
        assert_eq!(
            (
                k.flap_launches,
                k.missed,
                k.orphaned_launches,
                k.c1_scope_launches,
                k.tx_unverifiable
            ),
            (2, 1, 2, 1, 1)
        );
        assert_eq!(
            (
                c.state,
                c.blocks,
                c.reorgs,
                c.resyncs,
                c.rpc_errors,
                c.blocks_skipped
            ),
            ("FOLLOWING", 3, 1, 1, 1, 87)
        );
        let status = status_bytes(
            &Generation::initial(),
            &[],
            clock,
            1,
            1,
            0,
            "j",
            0,
            0,
            Some(&c),
            None,
            config.status_ms,
        )
        .unwrap();
        let r = darkbot_bsc_record::decode(&rs::SCHEMA, &status).unwrap();
        assert!(
            r.opt_nested(rs::STATE).unwrap().is_none(),
            "no C2 owner, no state record"
        );
        assert_eq!(r.u64(rs::STATUS_INTERVAL_MS).unwrap(), config.status_ms);
        let n = r.nested(rs::CANONICAL).unwrap();
        assert_eq!(n.u64(rs::canonical::FLAP_LAUNCHES).unwrap(), 2);
        assert_eq!(n.u64(rs::canonical::TX_UNVERIFIABLE).unwrap(), 1);
        assert_eq!(n.u64(rs::canonical::TX_ANSWER_REFUSED).unwrap(), 1);
        assert_eq!(n.u64(rs::canonical::HEAD).unwrap(), 12);
        assert_eq!(n.u64(rs::canonical::BLOCKS_SKIPPED).unwrap(), 87);
        assert_eq!(n.u64(rs::canonical::C1_SCOPE_LAUNCHES).unwrap(), 1);
        assert_eq!(n.i64(rs::canonical::HEAD_FETCH_LAG_MS).unwrap(), 300);
        assert_eq!(n.u64(rs::canonical::BACKFILL_BLOCKS).unwrap(), 1);
        assert_eq!(n.u64(rs::canonical::BACKFILL_LAUNCHES).unwrap(), 1);
        journal.finish().unwrap();
        let census = census(&base, journal_max_payload(&config), None).unwrap();
        assert_eq!(
            (
                census.flap_launches,
                census.hash_only_before,
                census.orphaned_launches,
                census.c1_scope_launches,
                census.unjoined,
                census.blocks_skipped,
                census.tx_unverifiable,
                census.tx_answer_refused,
                census.four_launches
            ),
            (2, 1, 2, 1, 0, 87, 1, 1, 1)
        );
        let w = &census.windows[0];
        assert_eq!((w.first_block, w.reorgs, w.resyncs), (Some(10), 1, 1));
        // No clean end record in this unit test: a killed process's window.
        assert_eq!(w.state, "UNTERMINATED");
        assert_eq!(census.fetch_lag_ms_min, Some(300));
        fs::remove_dir_all(&base).unwrap();
    }

    /// With every source OFF the runtime connects nothing, writes its
    /// status (all OFF, READ_ONLY) and stops on the stop file with a
    /// committed journal.
    #[test]
    fn an_idle_runtime_writes_its_status_and_stops_cleanly() {
        let base = std::env::temp_dir().join(format!("bsc-runtime-idle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        // No canonical endpoint: this test opens no connection at all.
        let text: String = CONFIG
            .replace("STATUS_MS=1000", "STATUS_MS=100")
            .lines()
            .filter(|l| !l.starts_with("BSC_CANONICAL_") && !l.starts_with("BSC_STATE_"))
            .map(|l| format!("{l}\n"))
            .collect();
        let config = RuntimeConfig::parse(&text).unwrap();
        let sources = base.join("bsc-sources");
        Store::open(&sources, 8).unwrap().init(1).unwrap();
        let rundir = base.clone();
        let runner = thread::spawn(move || run(&config, &sources, &rundir));
        let status = base.join("bsc-runtime/status.dbrec");
        let started = Instant::now();
        while !status.exists() {
            assert!(started.elapsed() < Duration::from_secs(10), "no status");
            thread::sleep(Duration::from_millis(20));
        }
        fs::write(base.join("bsc.runtime.stop"), b"stop\n").unwrap();
        runner.join().unwrap().unwrap();
        let r = darkbot_bsc_record::decode(&rs::SCHEMA, &fs::read(&status).unwrap()).unwrap();
        assert_eq!(r.text(rs::MODE).unwrap(), "READ_ONLY");
        let rows = r.nested_all(rs::SOURCES).unwrap();
        assert_eq!(rows.len(), 5);
        assert!(
            rows.iter()
                .all(|row| row.text(rs::source::HEALTH).unwrap() == "OFF")
        );
        let journals: Vec<_> = fs::read_dir(base.join("bsc-runtime/journal"))
            .unwrap()
            .collect();
        assert_eq!(journals.len(), 1);
        fs::remove_dir_all(&base).unwrap();
    }
}

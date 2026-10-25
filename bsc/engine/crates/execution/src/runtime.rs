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

use crate::canonical_join::{LaunchRef, Ledger};
use crate::chain_follower::{
    CanonicalEvent, FollowerFault, FollowerLimits, RequestError, run_follower,
};
use crate::g04_census::{Census, census};
use crate::ingress::{Health, IngressCore, Launch, Observation, SourceStats, health};
use crate::runtime_journal::{
    RuntimeJournal, SourceEvent, canonical_launch_payload, first_sighting_payload,
    later_sighting_payload, orphaned_launch_payload, reorg_payload, resync_payload,
    source_event_payload, window_payload,
};
use crate::sources::{Effective, Generation, SourceId, Store};
use crate::wss::{DisconnectReason, WssEvent, WssLimits, WssSource, run_source};
use darkbot_bsc_journal::RecordKind;
use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
use darkbot_bsc_record::{Draft, observed_now};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

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
                let rest = url
                    .strip_prefix("wss://")
                    .ok_or_else(|| format!("{key} must be wss://"))?;
                if rest.is_empty() || rest.contains('@') || rest.contains(char::is_whitespace) {
                    return Err(format!("{key} is not a plain wss:// endpoint"));
                }
                endpoints.push((source, url.to_string()));
            }
        }
        let canonical_rpc = values.remove("BSC_CANONICAL_RPC");
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
                if rest.is_empty() || rest.contains('@') || rest.contains(char::is_whitespace) {
                    return Err("BSC_CANONICAL_RPC is not a plain https:// endpoint".into());
                }
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
                };
                limits
                    .validate()
                    .map_err(|e| format!("canonical bounds: {e:?}"))?;
                Some(CanonicalConfig {
                    endpoint: url.to_string(),
                    limits,
                    join_delay_ms: take("BSC_CANONICAL_JOIN_DELAY_MS", 0, 600_000)?,
                })
            }
        };
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
        };
        config
            .wss
            .validate()
            .map_err(|e| format!("WSS bounds: {e:?}"))?;
        if let Some(unknown) = values.keys().next() {
            return Err(format!("unknown key {unknown}"));
        }
        Ok(config)
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
    Full { hash: [u8; 32], raw: Vec<u8> },
    HashOnly { hash: [u8; 32] },
    Rejected,
    Connected,
    Disconnected { rate_limited: bool },
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
            if let Err(e) = run_follower(&config.endpoint, &config.limits, &flag, &mut sink) {
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
    first_journaled: bool,
    blocks: u64,
    reorgs: u64,
    resyncs: u64,
    rpc_errors: u64,
    rate_limited: u64,
    ledger: Ledger,
}

impl Canonical {
    fn new(config: &CanonicalConfig) -> Self {
        Canonical {
            state: "CONNECTING",
            head: None,
            head_seen_ns: None,
            first_journaled: false,
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
            CanonicalEvent::Block { number, hash, .. } => {
                if !self.first_journaled {
                    self.first_journaled = true;
                    journal.record(
                        RecordKind::Canonical,
                        at,
                        &window_payload(true, number, &hash),
                    );
                }
                self.state = "FOLLOWING";
                self.head = Some((number, hash));
                self.head_seen_ns = Some(at);
                self.blocks += 1;
            }
            CanonicalEvent::Launch(l) => self.ledger.push(LaunchRef {
                venue: l.venue,
                block_number: l.block_number,
                block_hash: l.block_hash,
                tx_index: l.tx_index,
                tx_hash: l.tx_hash,
                token: l.token,
                external_buys: l.external_buys_after,
                // Its block was the last one reported.
                block_seen_ns: match self.head {
                    Some((_, h)) if h == l.block_hash => self.head_seen_ns.unwrap_or(at),
                    _ => at,
                },
            }),
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
                self.resyncs += 1;
                let from = orphaned.first().map_or(new_head.0, |(n, _)| *n);
                journal.record(RecordKind::Reorg, at, &resync_payload(from, new_head.0));
                self.orphan(&orphaned, at, journal);
            }
            CanonicalEvent::Gap { from, to } => {
                self.resyncs += 1;
                journal.record(RecordKind::Reorg, at, &resync_payload(from, to));
            }
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
            .set(e::EXTERNAL_BUYS, c.external_buys);
        if let Some((n, _)) = self.head {
            d.set(e::HEAD, n);
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

/// Replays the journals of `rundir` into `bsc-runtime/g04-census.dbrec`.
pub fn write_census(config: &RuntimeConfig, rundir: &Path) -> Result<Census, String> {
    let dir = rundir.join("bsc-runtime");
    let census = census(&dir.join("journal"), journal_max_payload(config))?;
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
        .set(rs::MODE, "READ_ONLY");
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
    d.encode().map_err(|e| e.to_string())
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
    let stop_file = rundir.join("bsc.runtime.stop");
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
    let mut canonical = config.canonical.as_ref().map(Canonical::new);
    let mut follower = match &config.canonical {
        Some(c) => Some(spawn_follower(c.clone(), canonical_tx, clock)?),
        None => {
            drop(canonical_tx);
            None
        }
    };
    let mut next_census = Instant::now() + Duration::from_millis(config.census_ms);
    let mut next_status = Instant::now();
    let mut next_reconcile = Instant::now();
    let reconcile_every = Duration::from_millis(config.status_ms.min(500));
    let status_every = Duration::from_millis(config.status_ms);
    let index = |s: SourceId| SourceId::ALL.iter().position(|x| *x == s).unwrap_or(0);
    loop {
        let stopping = stop_file.exists();
        if Instant::now() >= next_reconcile || stopping {
            next_reconcile = Instant::now() + reconcile_every;
            if !stopping {
                generation = store.current()?;
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
                Event::Full { hash, raw } => Observation::Full { hash: *hash, raw },
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
            match core.observe(source, observation, at) {
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
                        Ok((at, event)) => c.event(at, event, &mut journal),
                        Err(RecvTimeoutError::Timeout) if !f.handle.is_finished() => {}
                        Err(_) => break,
                    }
                }
                let _ = f.handle.join();
            }
            while let Ok((at, event)) = crx.try_recv() {
                c.event(at, event, &mut journal);
            }
            let now = if stopping { u64::MAX } else { clock.now_ns() };
            c.join(now, &core, &mut journal);
            if stopping && c.state != "STOPPED" {
                if let Some((n, h)) = c.head {
                    journal.record(
                        RecordKind::Canonical,
                        clock.now_ns(),
                        &window_payload(false, n, &h),
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
            )?;
            replace(&status_path, &bytes)?;
        }
        if Instant::now() >= next_census && !stopping {
            next_census = Instant::now() + Duration::from_millis(config.census_ms);
            // A census failure is visible in the log; observation goes on.
            if let Err(e) = write_census(config, rundir) {
                eprintln!("g04 census: {e}");
            }
        }
        if stopping && adapters.is_empty() {
            drop(tx);
            journal.finish()?;
            if let Err(e) = write_census(config, rundir) {
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
        let canonical = c.canonical.as_ref().unwrap();
        assert_eq!(canonical.endpoint, "https://bsc-dataseed.bnbchain.org");
        assert_eq!(canonical.limits.reorg_depth, 64);
        // Without the endpoint the canonical bounds are unknown keys.
        assert!(RuntimeConfig::parse(&CONFIG.replace("BSC_CANONICAL_RPC=", "#")).is_err());
        let without: String = CONFIG
            .lines()
            .filter(|l| !l.starts_with("BSC_CANONICAL_"))
            .map(|l| format!("{l}\n"))
            .collect();
        assert_eq!(RuntimeConfig::parse(&without).unwrap().canonical, None);
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
        let base = std::env::temp_dir().join(format!("bsc-runtime-canon-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let config = RuntimeConfig::parse(CONFIG).unwrap();
        let cc = config.canonical.clone().unwrap();
        let mut c = Canonical::new(&cc);
        let mut journal = RuntimeJournal::open(&base, 1, 64, journal_max_payload(&config)).unwrap();
        let mut core = IngressCore::new(64);
        let clock = Clock(Instant::now());
        let block = |n: u8| CanonicalEvent::Block {
            number: u64::from(n),
            hash: [n; 32],
            parent_hash: [n - 1; 32],
            timestamp_s: 0,
            observed: clock.0,
        };
        let launch = |n: u8, tx: u8, buys| {
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
            })
        };
        // A verified sighting of launch 1 before its block; launch 2 unseen.
        core.observe(
            SourceId::PublicNode,
            Observation::HashOnly { hash: [1; 32] },
            5,
        );
        journal.record(
            RecordKind::Runtime,
            1,
            &source_event_payload(SourceId::PublicNode, SourceEvent::Connected),
        );
        c.event(10, block(10), &mut journal);
        c.event(10, launch(10, 1, Some(2)), &mut journal);
        c.event(20, block(11), &mut journal);
        c.event(20, launch(11, 2, None), &mut journal);
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
                k.missed,
                k.external_buys,
                k.buys_unknown
            ),
            (2, 1, 1, 2, 1)
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
        assert_eq!((k.flap_launches, k.missed, k.orphaned_launches), (1, 0, 1));
        assert_eq!(
            (c.state, c.blocks, c.reorgs, c.resyncs, c.rpc_errors),
            ("FOLLOWING", 3, 1, 1, 1)
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
        )
        .unwrap();
        let r = darkbot_bsc_record::decode(&rs::SCHEMA, &status).unwrap();
        let n = r.nested(rs::CANONICAL).unwrap();
        assert_eq!(n.u64(rs::canonical::FLAP_LAUNCHES).unwrap(), 1);
        assert_eq!(n.u64(rs::canonical::HEAD).unwrap(), 12);
        journal.finish().unwrap();
        let census = census(&base, journal_max_payload(&config)).unwrap();
        assert_eq!(
            (
                census.flap_launches,
                census.hash_only_before,
                census.orphaned_launches
            ),
            (1, 1, 1)
        );
        assert_eq!(census.windows[0].first_block, Some(10));
        assert_eq!(
            (census.windows[0].reorgs, census.windows[0].resyncs),
            (1, 1)
        );
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
            .filter(|l| !l.starts_with("BSC_CANONICAL_"))
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

//! The read-only BSC runtime process (C1, order V2 §5/§7.3): it observes the
//! signal sources the native source owner made effective `SHADOW`, verifies
//! and classifies what they deliver in one [`IngressCore`], journals launch
//! sightings and source transitions, and rewrites its status record. No
//! transaction is built, signed or sent anywhere in this program.
//!
//! Lifecycle: `run` until the stop file exists (written by `darkbotctl`),
//! then stops every adapter, commits the journal and writes a final status.
//! A source without a configured endpoint, or without an adapter in this
//! build (the local geth Fast Lane needs a synced node), is `UNQUALIFIED`.

use crate::ingress::{Health, IngressCore, Launch, Observation, SourceStats, health};
use crate::runtime_journal::{
    RuntimeJournal, SourceEvent, first_sighting_payload, later_sighting_payload,
    source_event_payload,
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
        let mut take = |name: &str, min: u64, max: u64| -> Result<u64, String> {
            let v = values
                .remove(name)
                .ok_or_else(|| format!("{name} is required"))?;
            bounded(name, v, min, max)
        };
        let ms = Duration::from_millis;
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
        u64::try_from(self.0.elapsed().as_nanos()).unwrap_or(u64::MAX)
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
        config.wss.max_message_bytes + 64,
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
            )?;
            replace(&status_path, &bytes)?;
        }
        if stopping && adapters.is_empty() {
            drop(tx);
            journal.finish()?;
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
        ] {
            assert!(RuntimeConfig::parse(&bad).is_err(), "{bad}");
        }
    }

    /// With every source OFF the runtime connects nothing, writes its
    /// status (all OFF, READ_ONLY) and stops on the stop file with a
    /// committed journal.
    #[test]
    fn an_idle_runtime_writes_its_status_and_stops_cleanly() {
        let base = std::env::temp_dir().join(format!("bsc-runtime-idle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let config =
            RuntimeConfig::parse(&CONFIG.replace("STATUS_MS=1000", "STATUS_MS=100")).unwrap();
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

//! Project_0 read-only status publisher. Engine owns the socket; gateway reads it.
pub mod env_file;
pub mod generation;
pub mod proto;

use darkbot_runner::status_tap::{
    AssessmentSnapshot, CapturePhase, MarketSnapshot, OperationalSnapshot, PhaseSnapshot,
    RunSnapshot, StatusSnapshot, StatusTap, StorageSample, TransportSnapshot,
    TransportStateSnapshot,
};
use prost::Message;
use proto::control::{
    engine_frame, EngineFrame, EngineHello, Heartbeat, ProjectId, StateFull, StateQuality, TopicId,
};
use proto::project0::{
    CapturePhase as WirePhase, CaptureState, Counter, EffectiveRecord, MarketAssessment,
    MarketCounters, OperationalConfig, OperationalParameter, PhaseStatus, ReasonCount,
    ReceiverTotal, RunInfo, SystemStatus, TransportStatus, WriterLane,
};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const SCHEMA_ID: &str = include_str!("../../../../../protocol/schema_id.txt");

pub fn schema_id() -> &'static str {
    SCHEMA_ID
}

#[derive(Clone, Debug)]
pub struct ReporterConfig {
    pub socket_path: PathBuf,
    pub generation_file: PathBuf,
    pub run_id: String,
    pub heartbeat: Duration,
    pub write_timeout: Duration,
    /// Minimum spacing of capture-state frames; newer states in between replace older ones.
    pub min_interval: Duration,
}

impl ReporterConfig {
    pub fn new(socket_path: PathBuf, generation_file: PathBuf, run_id: String) -> Self {
        Self {
            socket_path,
            generation_file,
            run_id,
            heartbeat: Duration::from_secs(5),
            write_timeout: Duration::from_secs(2),
            min_interval: Duration::from_millis(100),
        }
    }
}

#[derive(Default)]
pub struct Counters {
    pub connections: AtomicU64,
    pub frames_sent: AtomicU64,
    pub write_errors: AtomicU64,
    pub seq: AtomicU64,
}

pub struct ReporterHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    socket_path: PathBuf,
    lock_path: PathBuf,
    _lock: fs::File,
    pub generation: u64,
    pub counters: Arc<Counters>,
}

impl ReporterHandle {
    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.socket_path);
        let _ = fs::remove_file(&self.lock_path);
    }
}

impl Drop for ReporterHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- tap → contract

/// project0.status; `None` until the engine reported its operational configuration.
pub fn system_status(s: &StatusSnapshot) -> Option<SystemStatus> {
    s.operational.as_deref().map(|o| SystemStatus {
        operational: Some(operational_config(o)),
    })
}

/// project0.capture.state; blocks the engine has not reported stay unset.
pub fn capture_state(s: &StatusSnapshot) -> CaptureState {
    CaptureState {
        run: s.run.as_deref().map(run_info),
        phase: s.phase.as_ref().map(phase_status),
        transport: transport_status(s.transport.as_ref(), s.transport_state.as_deref()),
        market: s.market.as_ref().map(market_counters),
        assessment: s.assessment.as_deref().map(market_assessment),
        writer: s.storage.as_ref().map(writer_lane),
    }
}

fn run_info(r: &RunSnapshot) -> RunInfo {
    RunInfo {
        run_id: r.run_id.clone(),
        program_id: r.program_id.clone(),
        source: r.source.to_owned(),
        commitment: r.commitment.to_owned(),
        ws_host: r.ws_host.clone(),
        rpc_host: r.rpc_host.clone(),
        configured_duration_seconds: r.configured_duration_seconds,
        ready_timeout_seconds: r.ready_timeout_seconds,
        drain_timeout_seconds: r.drain_timeout_seconds,
        heartbeat_seconds: r.heartbeat_seconds,
        min_sample_liquidations: r.min_sample_liquidations,
        ingress_buffer_events: r.ingress_buffer_events,
        run_dir: r.run_dir.clone(),
        enrich_max_pending: r.enrich_max_pending,
        enrich_max_attempts: r.enrich_max_attempts,
        enrich_retry_ms: r.enrich_retry_ms,
        enrich_timeout_ms: r.enrich_timeout_ms,
        enrich_phase_max_seconds: r.enrich_phase_max_seconds,
        source_coverage_verified: r.source_coverage_verified,
        enrichment_available: r.enrichment_available,
        recovery_budget_ms: r.recovery_budget_ms,
        recovery_attempts: r.recovery_attempts,
        idle_timeout_ms: r.idle_timeout_ms,
        healthy_reset_ms: r.healthy_reset_ms,
        readiness: r.readiness.clone(),
        process_started_unix_s: r.process_started_unix_s,
    }
}

fn phase_status(p: &PhaseSnapshot) -> PhaseStatus {
    let phase = match p.phase {
        CapturePhase::WaitingForStream => WirePhase::WaitingForStream,
        CapturePhase::Capture => WirePhase::Capture,
        CapturePhase::Drain => WirePhase::Drain,
        CapturePhase::Finalize => WirePhase::Finalize,
        CapturePhase::Ended => WirePhase::Ended,
    };
    PhaseStatus {
        phase: phase as i32,
        changed_unix_ms: p.changed_unix_ms,
        stream_ready_unix_s: p.stream_ready_unix_s,
        stop_reason: p.stop_reason.to_owned(),
        drain_complete: p.drain_complete,
        write_error: p.write_error.clone(),
        end_result: p.end_result.clone(),
    }
}

fn transport_status(
    health: Option<&TransportSnapshot>,
    state: Option<&TransportStateSnapshot>,
) -> Option<TransportStatus> {
    if health.is_none() && state.is_none() {
        return None;
    }
    Some(TransportStatus {
        connected: health.map(|h| h.connected),
        last_frame_age_ns: health.and_then(|h| h.last_frame_age_ns),
        application_frames: health.map(|h| h.application_frames),
        last_slot: health.and_then(|h| h.last_slot),
        last_state: state.map(|s| s.state.clone()).unwrap_or_default(),
        last_code: state.map(|s| s.code.clone()).unwrap_or_default(),
        last_attempt: state.map(|s| s.attempt),
        last_valid_slot: state.and_then(|s| s.last_valid_slot),
        last_state_unix_ms: state.map(|s| s.at_unix_ms),
    })
}

fn market_counters(m: &MarketSnapshot) -> MarketCounters {
    MarketCounters {
        observing: m.observing,
        stream_ready: m.stream_ready,
        counters: m
            .stats
            .counters()
            .into_iter()
            .map(|(name, value)| Counter {
                name: name.to_owned(),
                value,
            })
            .collect(),
    }
}

fn market_assessment(a: &AssessmentSnapshot) -> MarketAssessment {
    MarketAssessment {
        verdict: a.verdict.to_owned(),
        coverage_complete: a.coverage_complete,
        decode_complete: a.decode_complete,
        enrichment_complete: a.enrichment_complete,
        incomplete_reasons: a
            .incomplete_reasons
            .iter()
            .map(|r| (*r).to_owned())
            .collect(),
        truncated_unqualified_reasons: a
            .truncated_unqualified_reasons
            .iter()
            .map(|(reason, count)| ReasonCount {
                reason: (*reason).to_owned(),
                count: *count,
            })
            .collect(),
        gap_records: a.gap_records,
        truncated_pending: a.truncated_pending,
        deferred_liquidations: a.deferred_liquidations,
        distinct_targets: a.distinct_targets,
        distinct_receivers: a.distinct_receivers,
        gross_edge_total_usd: a.gross_edge_total_usd,
        gross_edge_p50_usd: a.gross_edge_p50_usd,
        gross_edge_p90_usd: a.gross_edge_p90_usd,
        gross_edge_p99_usd: a.gross_edge_p99_usd,
        top_receivers: a
            .top_receivers
            .iter()
            .map(|r| ReceiverTotal {
                receiver: r.receiver.clone(),
                liquidation_events: r.liquidation_events,
                gross_edge_total_usd: r.gross_edge_total_usd,
            })
            .collect(),
        deployment_error: a.deployment_error.map(str::to_owned),
        assessed_unix_ms: a.assessed_unix_ms,
    }
}

/// `StorageStatus` → `WriterLane`, field by field, no derived values.
fn writer_lane(sample: &StorageSample) -> WriterLane {
    let w = &sample.status;
    WriterLane {
        sampled_unix_ms: sample.at_unix_ms,
        queue_bytes: w.queue_bytes,
        queue_peak_bytes: w.queue_peak_bytes,
        admission_limit_bytes: w.admission_limit_bytes,
        max_bytes: w.max_bytes,
        free_budget_bytes: w.free_budget_bytes,
        ingress_bytes_per_s: w.ingress_bytes_per_s,
        write_bytes_per_s: w.write_bytes_per_s,
        lag_frames: w.lag_frames,
        budget_exhaustion_s: w.budget_exhaustion_s,
        disk_free_bytes: w.disk_free_bytes,
        disk_reserve_bytes: w.disk_reserve_bytes,
        disk_exhaustion_s: w.disk_exhaustion_s,
    }
}

fn operational_config(o: &OperationalSnapshot) -> OperationalConfig {
    OperationalConfig {
        parameters: o
            .parameters
            .iter()
            .map(|p| OperationalParameter {
                field: p.field.to_owned(),
                unit: p.unit.to_owned(),
                value: p.value,
                default_value: p.default,
                env: p.env.to_owned(),
                source: p.source.clone(),
            })
            .collect(),
        effective: o.effective.as_ref().map(|e| EffectiveRecord {
            schema: e.schema.clone(),
            profile: e.profile.name().to_owned(),
            rpc_endpoint_count: e.rpc_endpoint_count,
            wss_endpoint_count: e.wss_endpoint_count,
            fallback: e.fallback,
            adaptive: e.adaptive,
            duration: e.duration.clone(),
            verification: e.verification.clone(),
            diagnostics: e.diagnostics,
            fe_mode: e.fe_mode.clone(),
            fe_targets: e.fe_targets.clone(),
            fe_group: e.fe_group.clone(),
            sources: e
                .sources
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            provider_capacity_qualified: e.provider_capacity_qualified,
            free_only: e.free_only,
            live_authorized: e.live_authorized,
            submission_authorized: e.submission_authorized,
        }),
    }
}

// ---------------------------------------------------------------- framing

fn frame(message: engine_frame::Msg) -> Vec<u8> {
    let body = EngineFrame { msg: Some(message) }.encode_to_vec();
    let mut output = Vec::with_capacity(body.len() + 4);
    output.extend_from_slice(&(body.len() as u32).to_be_bytes());
    output.extend_from_slice(&body);
    output
}

pub fn hello_frame(source_id: &str, generation: u64) -> Vec<u8> {
    frame(engine_frame::Msg::Hello(EngineHello {
        project: ProjectId::Project0 as i32,
        schema_id: schema_id().to_owned(),
        source_id: source_id.to_owned(),
        generation,
        topics: vec![
            TopicId::Project0Status as i32,
            TopicId::Project0CaptureState as i32,
        ],
    }))
}

pub fn state_frame<M: Message>(
    topic: TopicId,
    source_id: &str,
    generation: u64,
    seq: u64,
    value: &M,
) -> Vec<u8> {
    frame(engine_frame::Msg::State(StateFull {
        topic: topic as i32,
        generation,
        seq,
        source_timestamp_unix_ns: now_ns(),
        quality: StateQuality::Ok as i32,
        source_id: source_id.to_owned(),
        payload: value.encode_to_vec().into(),
    }))
}

pub fn heartbeat_frame(id: u64) -> Vec<u8> {
    frame(engine_frame::Msg::Heartbeat(Heartbeat {
        heartbeat_id: id,
        server_time_unix_ns: now_ns(),
        state_watermarks: vec![],
    }))
}

fn next_seq(counters: &Counters) -> Result<u64, String> {
    counters
        .seq
        .fetch_add(1, Ordering::Relaxed)
        .checked_add(1)
        .ok_or_else(|| "reporter sequence overflow".to_string())
}

/// Versions already sent in this session.
#[derive(Default)]
struct Sent {
    system: u64,
    capture: u64,
}

/// Sends each topic whose blocks changed since the last send. Returns whether a capture-state
/// frame was sent.
fn send_changes(
    snapshot: &StatusSnapshot,
    sent: &mut Sent,
    source_id: &str,
    generation: u64,
    counters: &Counters,
    send: &mut dyn FnMut(&[u8]) -> Result<(), String>,
) -> Result<bool, String> {
    if snapshot.system_version != sent.system {
        if let Some(value) = system_status(snapshot) {
            let seq = next_seq(counters)?;
            send(&state_frame(
                TopicId::Project0Status,
                source_id,
                generation,
                seq,
                &value,
            ))?;
        }
        sent.system = snapshot.system_version;
    }
    if snapshot.capture_version == sent.capture {
        return Ok(false);
    }
    let seq = next_seq(counters)?;
    send(&state_frame(
        TopicId::Project0CaptureState,
        source_id,
        generation,
        seq,
        &capture_state(snapshot),
    ))?;
    sent.capture = snapshot.capture_version;
    Ok(true)
}

// ---------------------------------------------------------------- socket lifecycle

pub fn start(cfg: ReporterConfig, tap: StatusTap) -> Result<ReporterHandle, String> {
    use std::os::unix::ffi::OsStrExt;
    let n = cfg.socket_path.as_os_str().as_bytes().len();
    if n >= 100 {
        return Err(format!(
            "reporter socket path too long ({n} bytes, macOS limit ~104): {}",
            cfg.socket_path.display()
        ));
    }
    let lock_path = cfg.generation_file.with_extension("generation.lock");
    let lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|e| format!("reporter generation already owned or inaccessible: {e}"))?;
    // Refuse replacing someone else's UNIX socket, including a live server.
    let listener = match UnixListener::bind(&cfg.socket_path) {
        Ok(listener) => listener,
        Err(e) => {
            let _ = fs::remove_file(&lock_path);
            return Err(format!("reporter bind: {e}"));
        }
    };
    let ready = (|| -> Result<u64, String> {
        fs::set_permissions(&cfg.socket_path, fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        generation::next(&cfg.generation_file)
    })();
    let generation = match ready {
        Ok(n) => n,
        Err(e) => {
            let _ = fs::remove_file(&cfg.socket_path);
            let _ = fs::remove_file(&lock_path);
            return Err(e);
        }
    };
    let stop = Arc::new(AtomicBool::new(false));
    let counters = Arc::new(Counters::default());
    let source_id = format!("project0-{}-{}", std::process::id(), now_ns());
    let (stop_thread, counts_thread, cfg_thread) = (stop.clone(), counters.clone(), cfg.clone());
    let join = thread::Builder::new()
        .name("p0-reporter".into())
        .spawn(move || {
            serve(
                listener,
                cfg_thread,
                tap,
                source_id,
                generation,
                stop_thread,
                counts_thread,
            );
        })
        .map_err(|e| e.to_string());
    let thread = match join {
        Ok(thread) => thread,
        Err(e) => {
            let _ = fs::remove_file(&cfg.socket_path);
            let _ = fs::remove_file(&lock_path);
            return Err(e);
        }
    };
    Ok(ReporterHandle {
        stop,
        thread: Some(thread),
        socket_path: cfg.socket_path,
        lock_path,
        _lock: lock,
        generation,
        counters,
    })
}

fn serve(
    listener: UnixListener,
    cfg: ReporterConfig,
    tap: StatusTap,
    source_id: String,
    generation: u64,
    stop: Arc<AtomicBool>,
    counts: Arc<Counters>,
) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                counts.connections.fetch_add(1, Ordering::Relaxed);
                if let Err(e) = session(stream, &cfg, &tap, &source_id, generation, &stop, &counts)
                {
                    counts.write_errors.fetch_add(1, Ordering::Relaxed);
                    eprintln!("[P0/REPORTER] session error: {e}");
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50))
            }
            Err(e) => {
                eprintln!("[P0/REPORTER] accept error: {e}");
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn session(
    mut stream: UnixStream,
    cfg: &ReporterConfig,
    tap: &StatusTap,
    source_id: &str,
    generation: u64,
    stop: &AtomicBool,
    counters: &Counters,
) -> Result<(), String> {
    stream
        .set_write_timeout(Some(cfg.write_timeout))
        .map_err(|e| e.to_string())?;
    let mut send = |bytes: &[u8]| -> Result<(), String> {
        stream.write_all(bytes).map_err(|e| e.to_string())?;
        counters.frames_sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    };
    send(&hello_frame(source_id, generation))?;
    let mut reader = tap.reader();
    let mut sent = Sent::default();
    let mut heartbeat_id = 0u64;
    let mut last_heartbeat_at = Instant::now();
    let mut last_capture_at: Option<Instant> = None;
    while !stop.load(Ordering::Acquire) {
        match reader.wait_changed(cfg.heartbeat.min(Duration::from_millis(500))) {
            Some(mut snapshot) => {
                // At most one capture state per min_interval; whatever arrived meanwhile wins.
                if snapshot.capture_version != sent.capture {
                    if let Some(at) = last_capture_at {
                        let since = at.elapsed();
                        if since < cfg.min_interval {
                            thread::sleep(cfg.min_interval - since);
                            if let Some(newer) = reader.wait_changed(Duration::ZERO) {
                                snapshot = newer;
                            }
                        }
                    }
                }
                if send_changes(
                    &snapshot, &mut sent, source_id, generation, counters, &mut send,
                )? {
                    last_capture_at = Some(Instant::now());
                }
                reader.delivered(snapshot.seq);
            }
            None => {}
        }
        // Heartbeats follow their own clock, including during continuous state traffic.
        if last_heartbeat_at.elapsed() >= cfg.heartbeat {
            heartbeat_id = heartbeat_id.saturating_add(1);
            send(&heartbeat_frame(heartbeat_id))?;
            last_heartbeat_at = Instant::now();
        }
    }
    // Shutdown: deliver the final state (e.g. phase ENDED) before the socket closes.
    if let Some(snapshot) = reader.wait_changed(Duration::ZERO) {
        if let Some(at) = last_capture_at {
            thread::sleep(cfg.min_interval.saturating_sub(at.elapsed()));
        }
        send_changes(
            &snapshot, &mut sent, source_id, generation, counters, &mut send,
        )?;
        reader.delivered(snapshot.seq);
    }
    Ok(())
}

pub fn config_from_env(
    run_id: &str,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<ReporterConfig>, String> {
    let Some(socket) = lookup("P0_REPORT_SOCKET").filter(|s| !s.trim().is_empty()) else {
        return Ok(None);
    };
    let generation_file = lookup("P0_REPORT_GENERATION_FILE")
        .filter(|s| !s.trim().is_empty())
        .ok_or("P0_REPORT_GENERATION_FILE required with P0_REPORT_SOCKET")?;
    let ms =
        |key: &str, default: u64, range: std::ops::RangeInclusive<u64>| -> Result<u64, String> {
            let v = lookup(key)
                .map(|v| v.trim().parse::<u64>().map_err(|e| format!("{key}: {e}")))
                .transpose()?
                .unwrap_or(default);
            if range.contains(&v) {
                Ok(v)
            } else {
                Err(format!("{key} out of range: {v}"))
            }
        };
    let heartbeat = ms("P0_REPORT_HEARTBEAT_MS", 5000, 500..=60_000)?;
    // At most 10 Hz, with room inside the capture's two-second final handoff budget.
    let min_interval = ms("P0_REPORT_MIN_INTERVAL_MS", 100, 100..=1_000)?;
    let mut config = ReporterConfig::new(
        socket.trim().into(),
        generation_file.trim().into(),
        run_id.to_owned(),
    );
    config.heartbeat = Duration::from_millis(heartbeat);
    config.min_interval = Duration::from_millis(min_interval);
    Ok(Some(config))
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use darkbot_runner::status_tap::{
        operational_snapshot_of, EffectiveConfig, Limits, Profile, ReceiverRank,
    };
    use std::path::Path;

    fn golden_snapshot() -> StatusSnapshot {
        let mut market = MarketSnapshot {
            observing: true,
            stream_ready: true,
            ..Default::default()
        };
        market.stats.unique_transactions = 12;
        market.stats.liquidation_events = 2;
        market.stats.gap_count = 3;
        market.stats.transport_http_429 = 7;
        market.stats.first_observed_slot = Some(400_000_000);
        market.stats.last_observed_slot = Some(400_000_123);
        let mut storage = StorageSample {
            at_unix_ms: 1_760_000_002_000,
            status: Default::default(),
        };
        storage.status.queue_bytes = 1;
        storage.status.queue_peak_bytes = u64::MAX;
        storage.status.disk_free_bytes = Some(10);
        storage.status.disk_reserve_bytes = 11;
        StatusSnapshot {
            seq: 9,
            system_version: 1,
            capture_version: 8,
            operational: Some(Arc::new(operational_snapshot_of(&Limits::default(), None))),
            run: Some(Arc::new(RunSnapshot {
                run_id: "golden-run".into(),
                program_id: "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA".into(),
                source: "logs",
                commitment: "finalized",
                ws_host: "wss://api.mainnet-beta.solana.com".into(),
                configured_duration_seconds: 180,
                process_started_unix_s: 1_760_000_000,
                ..Default::default()
            })),
            phase: Some(PhaseSnapshot {
                phase: CapturePhase::Capture,
                changed_unix_ms: 1_760_000_001_000,
                stream_ready_unix_s: Some(1_760_000_001),
                ..Default::default()
            }),
            transport: Some(TransportSnapshot {
                connected: true,
                last_frame_age_ns: Some(1_500_000),
                last_slot: Some(400_000_123),
                application_frames: 42,
            }),
            transport_state: Some(Arc::new(TransportStateSnapshot {
                state: "RECOVERED".into(),
                code: "WSS_HTTP_429".into(),
                attempt: 2,
                last_valid_slot: Some(400_000_100),
                at_unix_ms: 1_760_000_000_500,
            })),
            market: Some(market),
            assessment: Some(Arc::new(AssessmentSnapshot {
                verdict: "MARKET_DATA_INCOMPLETE",
                incomplete_reasons: vec!["GAP"],
                gap_records: 1,
                assessed_unix_ms: 1_760_000_002_000,
                ..Default::default()
            })),
            storage: Some(storage),
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Committed wire bytes, also decoded by the frontend contract test. Regenerate on purpose
    /// with `UPDATE_GOLDEN=1` and review the diff.
    fn check_golden(name: &str, bytes: &[u8]) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../protocol/golden/project0")
            .join(name);
        let encoded = hex(bytes);
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &encoded).unwrap();
            return;
        }
        let expected = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; generate with UPDATE_GOLDEN=1", path.display()));
        assert_eq!(expected.trim(), encoded, "{name}: wire bytes changed");
    }

    #[test]
    fn golden_frames_match_the_committed_wire_bytes() {
        let s = golden_snapshot();
        check_golden("capture_state.hex", &capture_state(&s).encode_to_vec());
        check_golden(
            "system_status.hex",
            &system_status(&s).unwrap().encode_to_vec(),
        );
    }

    #[test]
    fn unreported_blocks_are_not_invented() {
        let s = StatusSnapshot::default();
        let c = capture_state(&s);
        assert!(c.run.is_none() && c.phase.is_none() && c.transport.is_none());
        assert!(c.market.is_none() && c.assessment.is_none() && c.writer.is_none());
        assert!(system_status(&s).is_none());
    }

    #[test]
    fn writer_lane_carries_every_writer_field() {
        let mut sample = StorageSample {
            at_unix_ms: 1_700_000_000_000,
            status: Default::default(),
        };
        let w = &mut sample.status;
        w.queue_bytes = 1;
        w.queue_peak_bytes = 2;
        w.admission_limit_bytes = 3;
        w.max_bytes = 4;
        w.free_budget_bytes = 5;
        w.ingress_bytes_per_s = 6.5;
        w.write_bytes_per_s = 7.5;
        w.lag_frames = 8;
        w.budget_exhaustion_s = Some(9.0);
        w.disk_free_bytes = Some(10);
        w.disk_reserve_bytes = 11;
        w.disk_exhaustion_s = Some(12.0);
        let l = writer_lane(&sample);
        assert_eq!(l.sampled_unix_ms, 1_700_000_000_000);
        assert_eq!(
            (l.queue_bytes, l.queue_peak_bytes, l.admission_limit_bytes),
            (1, 2, 3)
        );
        assert_eq!((l.max_bytes, l.free_budget_bytes, l.lag_frames), (4, 5, 8));
        assert_eq!((l.ingress_bytes_per_s, l.write_bytes_per_s), (6.5, 7.5));
        assert_eq!(
            (l.budget_exhaustion_s, l.disk_free_bytes),
            (Some(9.0), Some(10))
        );
        assert_eq!(
            (l.disk_reserve_bytes, l.disk_exhaustion_s),
            (11, Some(12.0))
        );
    }

    #[test]
    fn every_market_counter_is_sent_by_engine_name() {
        let mut m = MarketSnapshot::default();
        m.stats.gap_count = 3;
        m.stats.transport_http_429 = 7;
        let c = market_counters(&m);
        assert_eq!(c.counters.len(), m.stats.counters().len());
        let get = |n: &str| c.counters.iter().find(|x| x.name == n).map(|x| x.value);
        assert_eq!(
            (get("gap_count"), get("transport_http_429")),
            (Some(3), Some(7))
        );
    }

    #[test]
    fn only_changed_topics_are_sent() {
        let mut s = golden_snapshot();
        let counters = Counters::default();
        let mut sent = Sent::default();
        let mut frames = 0;
        let mut count = |_: &[u8]| -> Result<(), String> {
            frames += 1;
            Ok(())
        };
        assert!(send_changes(&s, &mut sent, "t", 1, &counters, &mut count).unwrap());
        s.capture_version += 1;
        assert!(send_changes(&s, &mut sent, "t", 1, &counters, &mut count).unwrap());
        assert!(!send_changes(&s, &mut sent, "t", 1, &counters, &mut count).unwrap());
        drop(count);
        assert_eq!(frames, 3, "system once, capture twice");
    }

    #[test]
    fn worst_case_frames_stay_far_below_the_gateway_limit() {
        // Gateway: Limits::max_state_full_bytes = 262_144.
        let limits = Limits::default();
        let base = operational_snapshot_of(&limits, None);
        let effective = EffectiveConfig {
            schema: "Project0/OperationalConfig/2".into(),
            profile: Profile::Custom,
            limits: limits.clone(),
            rpc_endpoint_count: 4,
            wss_endpoint_count: 4,
            fallback: true,
            adaptive: true,
            duration: "24h".into(),
            verification: "crosscheck".into(),
            diagnostics: true,
            fe_mode: "full".into(),
            fe_targets: vec!["A45dVtuyLccxs1xpbVJsTNh7AdtAotAZi4UxEcyMQDMS".into(); 100],
            fe_group: "4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8".into(),
            sources: base
                .parameters
                .iter()
                .map(|p| (p.field.to_owned(), "config_file".to_owned()))
                .collect(),
            provider_capacity_qualified: false,
            free_only: true,
            live_authorized: false,
            submission_authorized: false,
        };
        let mut s = golden_snapshot();
        s.operational = Some(Arc::new(operational_snapshot_of(&limits, Some(&effective))));
        s.assessment = Some(Arc::new(AssessmentSnapshot {
            verdict: "MARKET_DATA_INCOMPLETE",
            incomplete_reasons: vec!["TRUNCATED_LIQUIDATION_MISMATCH"; 13],
            truncated_unqualified_reasons: vec![("PHASE_BUDGET_EXHAUSTED", u64::MAX); 10],
            top_receivers: (0..10)
                .map(|i| ReceiverRank {
                    receiver: format!("{i:0>44}"),
                    liquidation_events: u64::MAX,
                    gross_edge_total_usd: f64::MAX,
                })
                .collect(),
            deployment_error: Some("POST_CAPTURE_DEPLOYMENT_VERIFICATION_NOT_RUN"),
            ..Default::default()
        }));
        let system = system_status(&s).unwrap().encoded_len();
        let capture = capture_state(&s).encoded_len();
        assert!(system < 32 * 1024, "system status {system} bytes");
        assert!(capture < 8 * 1024, "capture state {capture} bytes");
    }
}

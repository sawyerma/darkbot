//! Project_0 read-only status publisher. Engine owns the socket; gateway reads it.
pub mod generation;
pub mod proto;
use darkbot_runner::status_tap::{StatusSnapshot, StatusTap};
use prost::Message;
use proto::control::{
    engine_frame, EngineFrame, EngineHello, Heartbeat, ProjectId, StateFull, StateQuality, TopicId,
};
use proto::project0::StatusProjection;
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
    time::{Duration, SystemTime, UNIX_EPOCH},
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
}
impl ReporterConfig {
    pub fn new(socket_path: PathBuf, generation_file: PathBuf, run_id: String) -> Self {
        Self {
            socket_path,
            generation_file,
            run_id,
            heartbeat: Duration::from_secs(5),
            write_timeout: Duration::from_secs(2),
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
pub fn projection(run_id: &str, snapshot: &StatusSnapshot) -> StatusProjection {
    StatusProjection {
        run_id: run_id.to_owned(),
        last_observed_slot: snapshot.market.as_ref().and_then(|m| m.last_observed_slot),
        last_frame_age_ns: snapshot
            .transport
            .as_ref()
            .and_then(|m| m.last_frame_age_ns),
        gap_count: snapshot.market.as_ref().map(|m| m.gap_count),
        reconnect_count: snapshot.market.as_ref().map(|m| m.reconnect_count),
        liquidation_events: snapshot.market.as_ref().map(|m| m.liquidation_events),
        queue_bytes: snapshot.storage.as_ref().map(|m| m.queue_bytes),
        lag_frames: snapshot.storage.as_ref().map(|m| m.lag_frames),
        market_verdict: snapshot
            .market
            .as_ref()
            .map(|m| m.live_verdict.to_owned())
            .unwrap_or_default(),
    }
}
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
        topics: vec![TopicId::Project0Status as i32],
    }))
}
pub fn state_frame(
    source_id: &str,
    generation: u64,
    seq: u64,
    value: &StatusProjection,
) -> Vec<u8> {
    frame(engine_frame::Msg::State(StateFull {
        topic: TopicId::Project0Status as i32,
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
    let mut heartbeat_id = 0u64;
    while !stop.load(Ordering::Acquire) {
        if let Some(snapshot) = reader.wait_changed(cfg.heartbeat.min(Duration::from_millis(500))) {
            let seq = counters
                .seq
                .fetch_add(1, Ordering::Relaxed)
                .checked_add(1)
                .ok_or("reporter sequence overflow")?;
            send(&state_frame(
                source_id,
                generation,
                seq,
                &projection(&cfg.run_id, &snapshot),
            ))?;
        } else {
            heartbeat_id = heartbeat_id.saturating_add(1);
            send(&heartbeat_frame(heartbeat_id))?;
        }
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
    let ms = lookup("P0_REPORT_HEARTBEAT_MS")
        .map(|v| {
            v.trim()
                .parse::<u64>()
                .map_err(|e| format!("P0_REPORT_HEARTBEAT_MS: {e}"))
        })
        .transpose()?
        .unwrap_or(5000);
    if !(500..=60_000).contains(&ms) {
        return Err(format!("P0_REPORT_HEARTBEAT_MS out of range: {ms}"));
    }
    let mut config = ReporterConfig::new(
        socket.trim().into(),
        generation_file.trim().into(),
        run_id.to_owned(),
    );
    config.heartbeat = Duration::from_millis(ms);
    Ok(Some(config))
}

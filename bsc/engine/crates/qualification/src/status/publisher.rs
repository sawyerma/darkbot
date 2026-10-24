use super::{
    projection,
    wire::{bsc::JobStatus, control::*},
};
use prost::Message;
use std::{
    fs,
    io::{self, Write},
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn kv(path: &Path, key: &str) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key}=")).map(str::to_owned))
        .unwrap_or_default()
}
/// Result codes of a run that ended on its own or on request. The native verdict
/// (PASS_READ_ONLY, BLOCKED, STOP) is the job result, never a process failure.
const COMPLETED: [&str; 4] = ["PASS_READ_ONLY", "BLOCKED", "STOP", "STOPPED"];
pub fn job(rundir: &Path, label: &str) -> JobStatus {
    let launch = kv(&rundir.join("bsc.readiness.launch"), "launch_id");
    let mut status = JobStatus {
        job: "readiness".into(),
        state: "UNKNOWN".into(),
        launch_id: launch.clone(),
        started_unix_s: kv(&rundir.join("bsc.readiness.launch"), "started_unix_s")
            .parse()
            .unwrap_or(0),
        ..Default::default()
    };
    let Ok(out) = std::process::Command::new("/bin/launchctl")
        .args(["list", label])
        .output()
    else {
        return status;
    };
    let stopped =
        kv(&rundir.join("bsc.readiness.stopping"), "launch_id") == launch && !launch.is_empty();
    let result = rundir.join("bsc.readiness.result");
    let matched = !launch.is_empty() && kv(&result, "launch_id") == launch;
    let completed = matched && COMPLETED.contains(&kv(&result, "result_code").as_str());
    let text = String::from_utf8_lossy(&out.stdout);
    let pid = text.lines().any(|l| l.trim().starts_with("\"PID\" = "));
    if out.status.success() {
        status.state = if pid {
            if stopped { "STOPPING" } else { "RUNNING" }
        } else {
            let code = text.lines().find_map(|l| {
                l.trim()
                    .strip_prefix("\"LastExitStatus\" = ")
                    .and_then(|s| s.trim_end_matches(';').parse::<i32>().ok())
            });
            if completed {
                "STOPPED"
            } else if code.is_some() || matched {
                "FAILED"
            } else {
                "UNKNOWN"
            }
        }
        .into();
    } else if String::from_utf8_lossy(&out.stderr).contains("Could not find service")
        || String::from_utf8_lossy(&out.stderr).contains("Could not find specified service")
    {
        status.state = if launch.is_empty() || completed {
            "STOPPED"
        } else {
            "FAILED"
        }
        .into();
    }
    let result = rundir.join("bsc.readiness.result");
    if !launch.is_empty() && kv(&result, "launch_id") == launch {
        status.ended_unix_s = kv(&result, "ended_unix_s").parse().unwrap_or(0);
        status.result_code = kv(&result, "result_code");
    }
    status
}
fn send(stream: &mut UnixStream, msg: engine_frame::Msg) -> io::Result<()> {
    let bytes = EngineFrame { msg: Some(msg) }.encode_to_vec();
    if bytes.len() > 262144 {
        return Err(io::Error::other("bounded feed frame exceeded"));
    }
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&bytes)
}
fn state<M: Message>(
    stream: &mut UnixStream,
    topic: TopicId,
    generation: u64,
    seq: u64,
    message: &M,
) -> io::Result<()> {
    send(
        stream,
        engine_frame::Msg::State(StateFull {
            topic: topic as i32,
            generation,
            seq,
            source_timestamp_unix_ns: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .min(i64::MAX as u128) as i64,
            quality: StateQuality::Ok as i32,
            source_id: "bsc-status-reporter".into(),
            payload: message.encode_to_vec(),
        }),
    )
}
/// The same controller test as the gateway: a deployed readiness binary and a source snapshot.
fn controller_available(root: &Path, readiness_bin: Option<&Path>) -> bool {
    readiness_bin.is_some_and(Path::is_file) && root.join("SNAPSHOT_SHA256").is_file()
}
pub fn serve(
    root: &Path,
    rundir: &Path,
    socket: &Path,
    label: &str,
    readiness_bin: Option<&Path>,
) -> io::Result<()> {
    fs::create_dir_all(rundir)?;
    fs::set_permissions(rundir, fs::Permissions::from_mode(0o700))?;
    if let Some(parent) = socket.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Ok(meta) = fs::symlink_metadata(socket) {
        if !meta.file_type().is_socket() || UnixStream::connect(socket).is_ok() {
            return Err(io::Error::other("reporter socket occupied"));
        }
        fs::remove_file(socket)?;
    }
    let listener = UnixListener::bind(socket)?;
    fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let generation_path = rundir.join("bsc.reporter.generation");
    let prior = match fs::read_to_string(&generation_path) {
        Ok(s) => s.trim().parse::<u64>().map_err(io::Error::other)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => 0,
        Err(e) => return Err(e),
    };
    let generation = prior
        .checked_add(1)
        .ok_or_else(|| io::Error::other("generation exhausted"))?
        .max(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .min(u64::MAX as u128) as u64,
        );
    let tmp = generation_path.with_extension("tmp");
    let mut f = fs::File::create(&tmp)?;
    write!(f, "{generation}")?;
    f.sync_all()?;
    fs::rename(tmp, &generation_path)?;
    fs::File::open(rundir)?.sync_all()?;
    let mut client: Option<UnixStream> = None;
    let mut seq = 0u64;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // An accepted socket inherits the listener's non-blocking mode on macOS; a frame
                // larger than the socket buffer must block (bounded by the write timeout).
                stream.set_nonblocking(false)?;
                stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                if send(
                    &mut stream,
                    engine_frame::Msg::Hello(EngineHello {
                        project: ProjectId::Bsc as i32,
                        schema_id: include_str!("../../../../../../protocol/schema_id.txt")
                            .trim()
                            .into(),
                        source_id: "bsc-status-reporter".into(),
                        generation,
                        topics: vec![
                            TopicId::BscStatus as i32,
                            TopicId::BscLaunchState as i32,
                            TopicId::BscMonitoringState as i32,
                            TopicId::BscWalletsState as i32,
                        ],
                    }),
                )
                .is_ok()
                {
                    client = Some(stream);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }
        if let Some(stream) = client.as_mut() {
            let snapshot_root = root.canonicalize()?;
            let mut s = projection::project(&snapshot_root, rundir);
            let mut readiness_job = job(rundir, label);
            readiness_job.controller_available = Some(controller_available(root, readiness_bin));
            s.status.jobs.push(readiness_job);
            seq = seq
                .checked_add(1)
                .ok_or_else(|| io::Error::other("sequence exhausted"))?;
            let sent = state(stream, TopicId::BscStatus, generation, seq, &s.status)
                .and_then(|_| state(stream, TopicId::BscLaunchState, generation, seq, &s.launch))
                .and_then(|_| {
                    state(
                        stream,
                        TopicId::BscMonitoringState,
                        generation,
                        seq,
                        &s.monitoring,
                    )
                })
                .and_then(|_| {
                    state(
                        stream,
                        TopicId::BscWalletsState,
                        generation,
                        seq,
                        &s.wallets,
                    )
                });
            if sent.is_err() {
                client = None;
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

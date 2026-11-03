use super::{projection, readiness_job, wire::control::*};
use prost::Message;
use std::{
    fs,
    io::{self, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Shared engine-feed limit (engine_feed.proto): a STATE payload of at most the gateway's
/// ServerLimits.max_state_full_bytes, a frame of at most that plus the envelope reserve.
pub(super) const MAX_STATE_BYTES: usize = 262_144;
const ENVELOPE_RESERVE: usize = 4096;
fn send(stream: &mut UnixStream, msg: engine_frame::Msg) -> io::Result<()> {
    let bytes = EngineFrame { msg: Some(msg) }.encode_to_vec();
    if bytes.len() > MAX_STATE_BYTES + ENVELOPE_RESERVE {
        return Err(io::Error::other("bounded feed frame exceeded"));
    }
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&bytes)
}
/// One topic state. A payload over MAX_STATE_BYTES is never cut: it is declared too large with
/// the same identity, the gateway shows FRAME_TOO_LARGE for this topic only and the connection
/// and the other topics carry on (no reconnect loop).
pub(super) fn state<M: Message>(
    stream: &mut UnixStream,
    topic: TopicId,
    generation: u64,
    seq: u64,
    message: &M,
) -> io::Result<()> {
    let source_timestamp_unix_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(i64::MAX as u128) as i64;
    let payload = message.encode_to_vec();
    let msg = if payload.len() > MAX_STATE_BYTES {
        engine_frame::Msg::StateTooLarge(StateTooLarge {
            topic: topic as i32,
            generation,
            seq,
            source_timestamp_unix_ns,
            source_id: "bsc-status-reporter".into(),
            payload_bytes: payload.len() as u64,
        })
    } else {
        engine_frame::Msg::State(StateFull {
            topic: topic as i32,
            generation,
            seq,
            source_timestamp_unix_ns,
            quality: StateQuality::Ok as i32,
            source_id: "bsc-status-reporter".into(),
            payload,
        })
    };
    send(stream, msg)
}
/// The socket directory must be a real directory of this user that neither group nor others
/// can write; otherwise another local user could plant or replace the reporter socket (E2-8).
fn trusted_socket_dir(dir: &Path, uid: u32) -> io::Result<()> {
    let meta = fs::symlink_metadata(dir)?;
    if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o022 != 0 {
        return Err(io::Error::other(format!(
            "reporter socket directory not trusted: {}",
            dir.display()
        )));
    }
    Ok(())
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
    if let Some(parent) = socket.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
        trusted_socket_dir(parent, fs::metadata(rundir)?.uid())?;
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
                            TopicId::BscLiveState as i32,
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
            // Audit E 4.13: the job's state from its one native owner.
            s.status.jobs.push(readiness_job::status(
                rundir,
                label,
                readiness_job::controller_available(root, readiness_bin),
            ));
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
                })
                .and_then(|_| state(stream, TopicId::BscLiveState, generation, seq, &s.live));
            if sent.is_err() {
                client = None;
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

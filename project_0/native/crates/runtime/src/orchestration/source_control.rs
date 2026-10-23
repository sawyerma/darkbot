//! S1-02 operator source control (`p0ctl source set`): typed, append-only request files, a signal
//! doorbell and the control thread that validates requests for the state owner.
//!
//! * `p0ctl source set <unit> <mode>` writes `data/source-control/request_<seq>.p0j` (a sealed
//!   single-value journal, never overwritten) and sends SIGUSR1 to the runtime owner (lock pid).
//! * The handler only writes one byte to a non-blocking self-pipe (async-signal-safe). The
//!   control thread blocks on the pipe, reads the newest request, validates it into a
//!   `SourceControl` or a stated refusal and hands it to the owner's bounded control queue, which
//!   rings the owner doorbell. Only the owner applies a control (atomically, `SourceArbiter`).
//! * The handler is installed at runtime start, before the owner lock exists, so a signal from
//!   `p0ctl` never meets the default action (terminate).
use crate::analytics::journal;
use crate::analytics::payload::SourceControlRequest;
use darkbot_common::channel::Producer;
use darkbot_common::lifecycle::Shutdown;
use darkbot_solana::ingress::sources::{OperatingMode, Qualification, SourceControl, SourceUnit};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};

/// The qualification of the long-tail WSS feed: the existing canonical baseline of the owner.
pub const WSS_BASELINE: &str = "LONG_TAIL_WSS_BASELINE";

pub fn request_dir(project: &Path) -> PathBuf {
    project.join("data").join("source-control")
}

fn request_path(dir: &Path, seq: u64) -> PathBuf {
    dir.join(format!("request_{seq:08}.p0j"))
}

/// The request sequence numbers present, ascending (partial and foreign files are ignored).
fn sequences(dir: &Path) -> Result<Vec<u64>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("SOURCE_CONTROL_IO: {e}")),
    };
    let mut seqs: Vec<u64> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name();
            let digits = name
                .to_str()?
                .strip_prefix("request_")?
                .strip_suffix(".p0j")?;
            (digits.len() == 8).then(|| digits.parse().ok()).flatten()
        })
        .collect();
    seqs.sort_unstable();
    Ok(seqs)
}

/// `wss` or `geyser:<provider>` (provider: 1–32 of `a-z0-9_-`).
pub fn parse_unit(s: &str) -> Result<SourceUnit, &'static str> {
    if s == "wss" {
        return Ok(SourceUnit::WsRpcControl);
    }
    match s.strip_prefix("geyser:") {
        Some(p)
            if (1..=32).contains(&p.len())
                && p.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-'
                }) =>
        {
            Ok(SourceUnit::BinaryGeyser { provider: p.into() })
        }
        _ => Err("SOURCE_UNIT_INVALID"),
    }
}

/// The control of `unit` in `mode` under this build's qualifications: the WSS feed is the
/// qualified baseline (ACTIVE or OFF); a Geyser unit has no qualification record (S1-RACE
/// NOT_RUN), so ACTIVE is refused — never a qualification from a mock or an operator flag.
pub fn control_of(unit: SourceUnit, mode: OperatingMode) -> Result<SourceControl, &'static str> {
    let qualification = match (&unit, mode) {
        // The WSS feed has no observer path in this build: canonical (ACTIVE) or OFF.
        (SourceUnit::WsRpcControl, OperatingMode::Observe) => {
            return Err("SOURCE_MODE_UNSUPPORTED")
        }
        (SourceUnit::WsRpcControl, _) => Qualification::Qualified {
            evidence: WSS_BASELINE.into(),
        },
        (_, OperatingMode::Active) => return Err("SOURCE_ACTIVE_REQUIRES_QUALIFICATION"),
        _ => Qualification::NotRun,
    };
    SourceControl::new(unit, mode, qualification)
}

/// `p0ctl`: writes the next request (sequence = newest + 1) after checking unit and mode.
pub fn write_request(dir: &Path, unit: &str, mode: &str, now_unix_s: u64) -> Result<u64, String> {
    parse_unit(unit)?;
    mode.parse::<OperatingMode>()?;
    std::fs::create_dir_all(dir).map_err(|e| format!("SOURCE_CONTROL_IO: {e}"))?;
    let seq = sequences(dir)?.last().map_or(1, |s| s + 1);
    let request = SourceControlRequest {
        seq,
        unit: unit.into(),
        mode: mode.into(),
        requested_unix_s: now_unix_s,
    };
    journal::write_typed_value(
        &request_path(dir, seq),
        journal::source::SOURCE_CONTROL,
        &request,
    )
    .map_err(|e| format!("SOURCE_CONTROL_IO: {e}"))?;
    Ok(seq)
}

/// The newest request, if any.
pub fn latest_request(dir: &Path) -> Result<Option<SourceControlRequest>, String> {
    match sequences(dir)?.last() {
        None => Ok(None),
        Some(seq) => journal::read_typed_value(&request_path(dir, *seq))
            .map(Some)
            .map_err(|e| format!("SOURCE_CONTROL_REQUEST_INVALID: {e}")),
    }
}

/// What the control thread hands the owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlMessage {
    Apply {
        seq: u64,
        control: SourceControl,
    },
    Refused {
        seq: u64,
        request: String,
        reason: String,
    },
}

/// The message of `request` (validated), or `None` when it is not newer than `applied`.
pub fn message_of(request: &SourceControlRequest, applied: u64) -> Option<ControlMessage> {
    if request.seq <= applied {
        return None;
    }
    let parsed = parse_unit(&request.unit).and_then(|unit| {
        let mode = request.mode.parse::<OperatingMode>()?;
        control_of(unit, mode)
    });
    Some(match parsed {
        Ok(control) => ControlMessage::Apply {
            seq: request.seq,
            control,
        },
        Err(reason) => ControlMessage::Refused {
            seq: request.seq,
            request: format!("{} {}", request.unit, request.mode),
            reason: reason.into(),
        },
    })
}

static PIPE_R: AtomicI32 = AtomicI32::new(-1);
static PIPE_W: AtomicI32 = AtomicI32::new(-1);

extern "C" fn on_control_signal(_: libc::c_int) {
    // Async-signal-safe: one write(2) to a non-blocking pipe; a full pipe already rang.
    let fd = PIPE_W.load(Ordering::SeqCst);
    if fd >= 0 {
        let byte = 1u8;
        // SAFETY: a valid pipe descriptor and a one-byte buffer that outlives the call.
        unsafe { libc::write(fd, (&byte as *const u8).cast(), 1) };
    }
}

/// Installs the SIGUSR1 doorbell once (idempotent): self-pipe, then the handler.
pub fn install_signal() -> Result<(), String> {
    if PIPE_W.load(Ordering::SeqCst) >= 0 {
        return Ok(());
    }
    let mut fds = [0; 2];
    // SAFETY: `fds` holds two descriptors; the flags only concern the new pipe; the handler
    // performs one write(2); `sa` is fully initialized.
    let rc = unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            return Err(format!(
                "SOURCE_CONTROL_PIPE: {}",
                std::io::Error::last_os_error()
            ));
        }
        for fd in fds {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
            libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK);
        }
        PIPE_R.store(fds[0], Ordering::SeqCst);
        PIPE_W.store(fds[1], Ordering::SeqCst);
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_control_signal as extern "C" fn(libc::c_int) as usize;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGUSR1, &sa, std::ptr::null_mut())
    };
    if rc != 0 {
        return Err(format!(
            "SOURCE_CONTROL_HANDLER_INSTALL: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// Blocks until the doorbell rang (drained) or `timeout`; `true` when it rang.
fn wait_signal(timeout_ms: i32) -> bool {
    let fd = PIPE_R.load(Ordering::SeqCst);
    if fd < 0 {
        std::thread::sleep(std::time::Duration::from_millis(timeout_ms as u64));
        return false;
    }
    let mut p = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: one valid pollfd.
    if unsafe { libc::poll(&mut p, 1, timeout_ms) } <= 0 {
        return false;
    }
    let mut buf = [0u8; 64];
    // SAFETY: non-blocking read into an owned buffer until the pipe is empty.
    while unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
    true
}

/// The control thread: on every doorbell the newest request newer than the last one handed
/// over goes to the owner. Requests written before the start are not applied (a new run starts
/// from its configuration). The 250-ms bound only lets the thread see the shutdown.
pub fn spawn(
    dir: PathBuf,
    owner: Producer<ControlMessage>,
    shutdown: Shutdown,
) -> Result<std::thread::JoinHandle<()>, String> {
    install_signal()?;
    let mut applied = sequences(&dir)?.last().copied().unwrap_or(0);
    std::thread::Builder::new()
        .name("p0-source-control".into())
        .spawn(move || {
            while !shutdown.is_requested() {
                if !wait_signal(250) {
                    continue;
                }
                let message = match latest_request(&dir) {
                    Ok(Some(r)) => message_of(&r, applied),
                    Ok(None) => None,
                    Err(reason) => Some(ControlMessage::Refused {
                        seq: applied,
                        request: "unreadable".into(),
                        reason,
                    }),
                };
                if let Some(m) = message {
                    let (ControlMessage::Apply { seq, .. } | ControlMessage::Refused { seq, .. }) =
                        &m;
                    applied = applied.max(*seq);
                    // Bounded: a full queue refuses the request visibly (the owner is behind).
                    let _ = owner.try_push(m);
                }
            }
        })
        .map_err(|e| format!("SOURCE_CONTROL_THREAD: {e}"))
}

/// `p0ctl`: rings the running runtime's doorbell.
pub fn signal(pid: u32) -> Result<(), String> {
    // SAFETY: kill(2) with SIGUSR1 to the pid of the runtime owner lock.
    if unsafe { libc::kill(pid as libc::pid_t, libc::SIGUSR1) } != 0 {
        return Err(format!(
            "SOURCE_CONTROL_SIGNAL: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_common::channel::bounded;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("p0_srcctl_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// Requests are append-only typed journals with increasing sequence numbers; unit and mode
    /// are parsed strictly; a Geyser ACTIVE request is refused (no qualification record), WSS
    /// carries its baseline qualification; an already handed-over request is not repeated.
    #[test]
    fn requests_are_typed_append_only_and_validated() {
        let d = dir("requests");
        assert_eq!(latest_request(&d).unwrap(), None);
        assert_eq!(write_request(&d, "geyser:publicnode", "observe", 7), Ok(1));
        assert_eq!(write_request(&d, "geyser:publicnode", "active", 8), Ok(2));
        assert_eq!(write_request(&d, "wss", "active", 9), Ok(3));
        assert_eq!(
            write_request(&d, "geyser:Public", "off", 9).unwrap_err(),
            "SOURCE_UNIT_INVALID"
        );
        assert_eq!(
            write_request(&d, "wss", "on", 9).unwrap_err(),
            "SOURCE_MODE_INVALID"
        );
        assert!(d.join("request_00000001.p0j").exists());
        let r = |seq: u64| -> SourceControlRequest {
            journal::read_typed_value(&request_path(&d, seq)).unwrap()
        };
        assert_eq!(
            message_of(&r(1), 0),
            Some(ControlMessage::Apply {
                seq: 1,
                control: SourceControl::new(
                    SourceUnit::BinaryGeyser {
                        provider: "publicnode".into()
                    },
                    OperatingMode::Observe,
                    Qualification::NotRun
                )
                .unwrap()
            })
        );
        assert_eq!(
            message_of(&r(2), 1),
            Some(ControlMessage::Refused {
                seq: 2,
                request: "geyser:publicnode active".into(),
                reason: "SOURCE_ACTIVE_REQUIRES_QUALIFICATION".into()
            })
        );
        assert!(matches!(
            message_of(&r(3), 2),
            Some(ControlMessage::Apply { control, .. }) if control.feeds_canonical_state()
        ));
        assert_eq!(message_of(&r(3), 3), None, "handed over once");
        assert_eq!(latest_request(&d).unwrap().map(|r| r.seq), Some(3));
        std::fs::remove_dir_all(d).unwrap();
    }

    /// The doorbell end to end in-process: a SIGUSR1 after a written request reaches the control
    /// thread through the self-pipe and the owner queue gets exactly that request; a request
    /// that existed before the start is never applied; the thread ends on shutdown.
    #[test]
    fn signal_hands_the_newest_request_to_the_owner_queue() {
        let d = dir("signal");
        write_request(&d, "geyser:publicnode", "observe", 1).unwrap();
        let (tx, rx) = bounded::<ControlMessage>("source-control", 4);
        let shutdown = Shutdown::new();
        let thread = spawn(d.clone(), tx, shutdown.clone()).unwrap();
        // SAFETY: the handler is installed; SIGUSR1 only writes to the self-pipe.
        unsafe { libc::raise(libc::SIGUSR1) };
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(
            rx.try_pop(),
            None,
            "a request older than the start is not applied"
        );
        write_request(&d, "geyser:publicnode", "off", 2).unwrap();
        let t = std::time::Instant::now();
        unsafe { libc::raise(libc::SIGUSR1) };
        let got = loop {
            if let Some(m) = rx.try_pop() {
                break m;
            }
            assert!(t.elapsed() < std::time::Duration::from_secs(5));
            std::thread::yield_now();
        };
        assert!(
            t.elapsed() < std::time::Duration::from_millis(200),
            "{:?}",
            t.elapsed()
        );
        assert!(matches!(
            got,
            ControlMessage::Apply { seq: 2, control } if control.mode == OperatingMode::Off
        ));
        shutdown.request();
        thread.join().unwrap();
        std::fs::remove_dir_all(d).unwrap();
    }
}

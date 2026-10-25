//! FEUI-01 control boundary of the one runtime (`p0-longtail`).
//!
//! * Runtime-owner lock: an exclusive `flock` on `P0_RUNTIME_LOCK` held for the whole process,
//!   with PID, run id and mode written into the file. A second owner (double start, second
//!   journal writer) is refused before any stream, journal or RPC exists; the kernel releases the
//!   lock on every exit, so a crash never leaves a live lock behind.
//! * Operator stop: SIGTERM/SIGINT only set a flag. The existing lifecycle polls it and stops
//!   through its ordered abort path (admission closed, drain, journals sealed INCOMPLETE, phase
//!   journal finalized, stop reason `OPERATOR_STOP`) — never a kill, never a second runtime.
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_stop_signal(_: libc::c_int) {
    // Async-signal-safe: one atomic store.
    STOP.store(true, Ordering::SeqCst);
}

/// Installs the stop handler for SIGTERM and SIGINT (idempotent).
pub fn install_stop_handler() -> Result<(), String> {
    for sig in [libc::SIGTERM, libc::SIGINT] {
        // SAFETY: the handler only performs an atomic store; `sa` is fully initialized.
        let rc = unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_stop_signal as extern "C" fn(libc::c_int) as usize;
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(sig, &sa, std::ptr::null_mut())
        };
        if rc != 0 {
            return Err(format!(
                "STOP_HANDLER_INSTALL: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

/// Whether an operator stop was requested (signal or `request_stop`).
pub fn stop_requested() -> bool {
    STOP.load(Ordering::SeqCst)
}

/// Same effect as a stop signal (tests and in-process callers).
pub fn request_stop() {
    STOP.store(true, Ordering::SeqCst);
}

/// Default lock path of a project directory (`project_0/data/.p0-runtime.lock`).
pub fn default_lock_path(project: &Path) -> PathBuf {
    project.join("data").join(".p0-runtime.lock")
}

/// The runtime-owner lock of a project: `P0_RUNTIME_LOCK`, else the default path.
pub fn lock_path(project: &Path) -> PathBuf {
    std::env::var_os("P0_RUNTIME_LOCK")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_lock_path(project))
}

/// Held exclusive runtime-owner lock (released when dropped or at process exit).
#[derive(Debug)]
pub struct OwnerLock {
    _file: File,
    pub path: PathBuf,
}

fn try_lock(file: &File) -> std::io::Result<bool> {
    // SAFETY: flock on an owned, open file descriptor.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(true);
    }
    let e = std::io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::EWOULDBLOCK) {
        Ok(false)
    } else {
        Err(e)
    }
}

/// E7: serializes status probes and owner acquisition. Both hold this blocking lock only for a
/// few syscalls (acquisition: test + record write; probe: record read + test), so a probe's
/// momentary exclusive test of the owner lock never overlaps a starting owner's attempt, and a
/// probe never reads a record while it is rewritten. The owner lock itself stays the only
/// mutual exclusion of runtimes; this guard never admits a second owner.
fn probe_guard(path: &Path) -> std::io::Result<File> {
    let guard = open(&path.with_extension("lock-probe"))?;
    loop {
        // SAFETY: flock on an owned, open file descriptor.
        if unsafe { libc::flock(guard.as_raw_fd(), libc::LOCK_EX) } == 0 {
            return Ok(guard);
        }
        let e = std::io::Error::last_os_error();
        if e.kind() != std::io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

fn open(path: &Path) -> std::io::Result<File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

/// Acquires the runtime-owner lock or refuses with the current holder.
pub fn acquire_owner(path: &Path, run_id: &str, mode: &str) -> Result<OwnerLock, String> {
    let _probe =
        probe_guard(path).map_err(|e| format!("RUNTIME_OWNER_LOCK_IO {}: {e}", path.display()))?;
    let mut file =
        open(path).map_err(|e| format!("RUNTIME_OWNER_LOCK_IO {}: {e}", path.display()))?;
    match try_lock(&file) {
        Ok(true) => {}
        Ok(false) => {
            let mut held = String::new();
            let _ = file.read_to_string(&mut held);
            return Err(format!(
                "RUNTIME_OWNER_LOCKED: another runtime owns {} ({})",
                path.display(),
                held.trim()
            ));
        }
        Err(e) => return Err(format!("RUNTIME_OWNER_LOCK_IO {}: {e}", path.display())),
    }
    // D08: the owner record never carries a start time of 0.
    let started = darkbot_common::timing::wall_now()
        .map_err(|code| format!("RUNTIME_OWNER_LOCK {code}"))?
        .as_secs();
    let line = format!(
        "pid={} run_id={} mode={} started_unix_s={}\n",
        std::process::id(),
        run_id,
        mode,
        started
    );
    (|| {
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(line.as_bytes())?;
        file.sync_all()
    })()
    .map_err(|e| format!("RUNTIME_OWNER_LOCK_IO {}: {e}", path.display()))?;
    Ok(OwnerLock {
        _file: file,
        path: path.into(),
    })
}

/// Holder of a runtime-owner lock as seen from another process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerState {
    Running {
        pid: u32,
        run_id: String,
        mode: String,
        started_unix_s: u64,
    },
    /// No live owner; `last` is the record of the previous owner, if any.
    NotRunning { last: Option<String> },
}

fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace()
        .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
}

/// Reads the lock without taking it over: a lock that can be taken has no live owner.
pub fn owner_state(path: &Path) -> Result<OwnerState, String> {
    if !path.exists() {
        return Ok(OwnerState::NotRunning { last: None });
    }
    let _probe = probe_guard(path).map_err(|e| format!("RUNTIME_OWNER_LOCK_IO: {e}"))?;
    let mut file = open(path).map_err(|e| format!("RUNTIME_OWNER_LOCK_IO: {e}"))?;
    let mut held = String::new();
    file.read_to_string(&mut held)
        .map_err(|e| format!("RUNTIME_OWNER_LOCK_IO: {e}"))?;
    let held = held.trim().to_string();
    match try_lock(&file).map_err(|e| format!("RUNTIME_OWNER_LOCK_IO: {e}"))? {
        // Taken just to probe; released when `file` drops at the end of this function.
        true => Ok(OwnerState::NotRunning {
            last: (!held.is_empty()).then_some(held),
        }),
        false => Ok(OwnerState::Running {
            pid: field(&held, "pid")
                .and_then(|v| v.parse().ok())
                .ok_or("RUNTIME_OWNER_RECORD_INVALID")?,
            run_id: field(&held, "run_id").unwrap_or("").into(),
            mode: field(&held, "mode").unwrap_or("").into(),
            started_unix_s: field(&held, "started_unix_s")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E8: the stop handler (no SA_RESTART) is safe for the blocking I/O of the runtime: with
    /// it installed, SIGTERMs delivered to a thread blocked in `read_exact` on a pipe and to a
    /// thread appending+fsyncing journal frames interrupt neither (std retries EINTR there); the
    /// stop flag is set. Runs in a child process so the signal disposition stays local.
    #[test]
    fn e8_stop_signals_never_break_blocking_reads_or_journal_writes() {
        use std::io::{Read, Write};
        use std::os::unix::thread::JoinHandleExt;
        if std::env::var_os("P0_E8_CHILD").is_none() {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "orchestration::control::tests::e8_stop_signals_never_break_blocking_reads_or_journal_writes",
                    "--test-threads=1",
                ])
                .env("P0_E8_CHILD", "1")
                .output()
                .unwrap();
            assert!(
                out.status.success() && String::from_utf8_lossy(&out.stdout).contains("1 passed"),
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            return;
        }
        install_stop_handler().unwrap();
        let (mut tx, mut rx) = std::os::unix::net::UnixStream::pair().unwrap();
        let reader = std::thread::spawn(move || {
            let mut buf = [0u8; 64];
            rx.read_exact(&mut buf).map(|()| buf)
        });
        let dir = std::env::temp_dir().join(format!("p0_e8_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("e8.p0j");
        let signalled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done = signalled.clone();
        // Appends + fsyncs until every signal was sent, so the signals hit an active writer.
        let writer = std::thread::spawn(move || -> std::io::Result<u64> {
            let mut w = crate::analytics::journal::Writer::create(&path)?;
            let mut frames = 0u64;
            while !done.load(std::sync::atomic::Ordering::Acquire) || frames < 10 {
                w.append_typed(
                    Default::default(),
                    &crate::analytics::payload::OwnerDiagnostic {
                        line: format!("e8 {frames}"),
                    },
                )?;
                w.sync()?;
                frames += 1;
            }
            w.seal(true)?;
            Ok(frames)
        });
        let targets = [reader.as_pthread_t(), writer.as_pthread_t()];
        for _ in 0..50 {
            for t in targets {
                // SAFETY: neither thread can exit before its signal phase ends (the reader waits
                // for the data sent below, the writer for `signalled`), and the handles stay
                // unjoined until then; SIGTERM only stores a flag.
                unsafe { libc::pthread_kill(t, libc::SIGTERM) };
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        signalled.store(true, std::sync::atomic::Ordering::Release);
        tx.write_all(&[7u8; 64]).unwrap();
        assert_eq!(reader.join().unwrap().unwrap(), [7u8; 64]);
        let frames = writer.join().unwrap().unwrap();
        assert!(frames >= 10, "{frames}");
        let chain = crate::storage::segments::verify_chain(&dir.join("e8.p0j")).unwrap();
        assert!(chain.sealed && chain.complete);
        assert!(stop_requested());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// E7: 1000 owner hand-overs while another thread probes the state continuously: a start is
    /// never refused by a probe, a probe never reads a half-written record, and while an owner
    /// holds the lock a second owner is still refused.
    #[test]
    fn e7_status_probes_never_fail_a_concurrent_start() {
        let d = std::env::temp_dir().join(format!("p0_ctl_e7_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let path = d.join("data/.p0-runtime.lock");
        drop(acquire_owner(&path, "seed", "shadow").unwrap());
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (probe_path, probe_stop) = (path.clone(), stop.clone());
        let prober = std::thread::spawn(move || {
            let mut probes = 0u64;
            while !probe_stop.load(std::sync::atomic::Ordering::Relaxed) {
                owner_state(&probe_path).expect("probe never fails");
                probes += 1;
            }
            probes
        });
        for i in 0..1000 {
            let owner = acquire_owner(&path, &format!("run{i}"), "shadow")
                .unwrap_or_else(|e| panic!("start {i} refused by a probe: {e}"));
            assert!(acquire_owner(&path, "second", "shadow")
                .unwrap_err()
                .starts_with("RUNTIME_OWNER_LOCKED"));
            assert!(matches!(
                owner_state(&path).unwrap(),
                OwnerState::Running { .. }
            ));
            drop(owner);
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(prober.join().unwrap() > 0);
        std::fs::remove_dir_all(d).unwrap();
    }

    /// A second owner is refused while the first holds the lock, the state is readable from
    /// outside, and the lock is free again once the holder is gone.
    #[test]
    fn owner_lock_refuses_second_owner_and_reports_holder() {
        let d = std::env::temp_dir().join(format!("p0_ctl_{}_{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&d);
        let path = d.join("data/.p0-runtime.lock");
        assert_eq!(
            owner_state(&path).unwrap(),
            OwnerState::NotRunning { last: None }
        );
        let held = acquire_owner(&path, "run_a", "econ4h").unwrap();
        // flock is per open file description: a second open in this process is refused too.
        let err = acquire_owner(&path, "run_b", "econ4h").unwrap_err();
        assert!(
            err.starts_with("RUNTIME_OWNER_LOCKED") && err.contains("run_id=run_a"),
            "{err}"
        );
        match owner_state(&path).unwrap() {
            OwnerState::Running {
                pid, run_id, mode, ..
            } => {
                assert_eq!(
                    (pid, run_id.as_str(), mode.as_str()),
                    (std::process::id(), "run_a", "econ4h")
                )
            }
            other => panic!("{other:?}"),
        }
        drop(held);
        match owner_state(&path).unwrap() {
            OwnerState::NotRunning { last: Some(l) } => assert!(l.contains("run_id=run_a")),
            other => panic!("{other:?}"),
        }
        let again = acquire_owner(&path, "run_c", "market").unwrap();
        drop(again);
        std::fs::remove_dir_all(d).unwrap();
    }
}

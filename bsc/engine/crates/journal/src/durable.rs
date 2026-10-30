//! BSC3-023: the two halves of a durable write, apart. `fsync(2)` hands a
//! file's (or a directory's) data and metadata to the drive; on macOS the
//! drive may still hold them in its cache, and may put them on its medium
//! in any order, until `F_FULLFSYNC` asks it to put everything it buffered
//! there. `File::sync_all` does both for every file (std issues
//! `F_FULLFSYNC` on Apple platforms), several milliseconds each. Writes
//! that must be durable together hand each one over with [`handed`], or
//! with [`ordered`] where a later write must never reach the medium
//! without it (a record's content before the link that publishes it), and
//! pay the medium flush once with [`flush`] after the last: when [`flush`]
//! returns, every write handed over before it is on the medium. Elsewhere
//! `fsync` already is durable and [`flush`] has nothing to add.

use std::fs::File;
use std::io;

/// What a call asks of the drive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Handed over (macOS `fsync`; elsewhere `fsync`, durable).
    Fsync,
    /// Handed over and ordered before every later write (macOS
    /// `F_BARRIERFSYNC`; elsewhere `fsync`, durable).
    Barrier,
    /// Everything buffered put on the medium (macOS `F_FULLFSYNC`;
    /// elsewhere nothing to do).
    FullFlush,
}

/// `file`'s data and metadata handed to the drive (on macOS: not yet on its
/// medium, see [`flush`]).
pub fn handed(file: &File) -> io::Result<()> {
    sys(file, Op::Fsync)
}

/// [`handed`], and on the medium before any write issued after it.
pub fn ordered(file: &File) -> io::Result<()> {
    sys(file, Op::Barrier)
}

/// Everything the drive buffered put on its medium (through `file`), so
/// every write [`handed`] over before is durable. Elsewhere every
/// handed-over write already is.
pub fn flush(file: &File) -> io::Result<()> {
    sys(file, Op::FullFlush)
}

fn sys(file: &File, op: Op) -> io::Result<()> {
    #[cfg(any(test, feature = "test-hooks"))]
    hooks::note(op);
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        let fd = file.as_raw_fd();
        // SAFETY: `fsync` and `fcntl(F_BARRIERFSYNC | F_FULLFSYNC)` (no
        // argument) on a descriptor `file` owns and keeps open.
        match op {
            Op::Fsync => retried(|| unsafe { libc::fsync(fd) }),
            Op::Barrier => retried(|| unsafe { libc::fcntl(fd, libc::F_BARRIERFSYNC) }),
            Op::FullFlush => retried(|| unsafe { libc::fcntl(fd, libc::F_FULLFSYNC) }),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        match op {
            Op::Fsync | Op::Barrier => file.sync_all(),
            Op::FullFlush => Ok(()),
        }
    }
}

/// A call returning -1 on failure, repeated while a signal interrupts it.
#[cfg(target_os = "macos")]
fn retried(mut call: impl FnMut() -> libc::c_int) -> io::Result<()> {
    loop {
        if call() != -1 {
            return Ok(());
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

/// Tests: what this thread asked of the drive, once [`hooks::record`] was
/// called on it (never otherwise, so nothing grows in a running process).
#[cfg(any(test, feature = "test-hooks"))]
pub mod hooks {
    use super::Op;
    use std::cell::RefCell;

    thread_local! {
        static OPS: RefCell<Option<Vec<Op>>> = const { RefCell::new(None) };
    }

    pub(super) fn note(op: Op) {
        OPS.with(|o| {
            if let Some(ops) = o.borrow_mut().as_mut() {
                ops.push(op);
            }
        });
    }

    /// Starts recording on this thread.
    pub fn record() {
        OPS.with(|o| *o.borrow_mut() = Some(Vec::new()));
    }

    /// What was asked since [`record`] or the last [`take`].
    pub fn take() -> Vec<Op> {
        OPS.with(|o| {
            o.borrow_mut()
                .as_mut()
                .map(std::mem::take)
                .unwrap_or_default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each half asks the drive for its own operation, on a real file and
    /// its directory, and the content stays.
    #[test]
    fn a_handed_over_write_is_flushed_with_its_content() {
        let dir = std::env::temp_dir().join(format!("bsc-durable-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("entry");
        std::fs::write(&path, b"entry").unwrap();
        let file = File::open(&path).unwrap();
        hooks::record();
        ordered(&file).unwrap();
        handed(&File::open(&dir).unwrap()).unwrap();
        flush(&File::open(&dir).unwrap()).unwrap();
        assert_eq!(hooks::take(), [Op::Barrier, Op::Fsync, Op::FullFlush]);
        assert_eq!(std::fs::read(&path).unwrap(), b"entry");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A failing call is an error, one interrupted by a signal is repeated.
    #[cfg(target_os = "macos")]
    #[test]
    fn an_interrupted_call_is_repeated_and_a_failed_one_is_an_error() {
        let mut calls = 0;
        retried(|| {
            calls += 1;
            if calls < 3 {
                // SAFETY: writes this thread's errno.
                unsafe { *libc::__error() = libc::EINTR };
                -1
            } else {
                0
            }
        })
        .unwrap();
        assert_eq!(calls, 3);
        let e = retried(|| {
            // SAFETY: writes this thread's errno.
            unsafe { *libc::__error() = libc::EIO };
            -1
        })
        .unwrap_err();
        assert_eq!(e.raw_os_error(), Some(libc::EIO));
    }
}

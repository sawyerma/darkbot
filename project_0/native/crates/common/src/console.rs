//! Bounded, non-blocking terminal output (D9, M2). Hot paths (capture loop, decoding, the owner
//! evidence writer) queue a line with `try_send`; one detached console thread prints it. A full
//! queue drops the line and counts it, so a blocked stderr never stalls ingress, a writer or a
//! shutdown, and nothing ever joins the console thread. Lines that must not be lost belong in a
//! journal, not here. F-16: the owner, the runtime start, the WSS ingress and the provider print
//! through it too (it lives here so the solana crate can use it); a process flushes it, bounded,
//! before it exits.
use std::io::Write;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc, Arc, Condvar, Mutex, OnceLock,
};
use std::time::{Duration, Instant};

/// Queued lines of the process console before further lines are dropped.
pub const CAPACITY: usize = 1024;

pub struct Console {
    tx: mpsc::SyncSender<String>,
    dropped: Arc<AtomicU64>,
    queued: AtomicU64,
    /// Lines written so far (the console thread signals every one).
    printed: Arc<(Mutex<u64>, Condvar)>,
}
impl Console {
    /// A console printing to `out` on its own detached thread.
    pub fn start<W: Write + Send + 'static>(capacity: usize, mut out: W) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::sync_channel::<String>(capacity);
        let printed = Arc::new((Mutex::new(0u64), Condvar::new()));
        let signal = printed.clone();
        std::thread::Builder::new()
            .name("p0-console".into())
            .spawn(move || {
                for line in rx {
                    let _ = writeln!(out, "{line}");
                    let (count, done) = &*signal;
                    *count.lock().unwrap_or_else(|e| e.into_inner()) += 1;
                    done.notify_all();
                }
            })?;
        Ok(Self {
            tx,
            dropped: Arc::new(AtomicU64::new(0)),
            queued: AtomicU64::new(0),
            printed,
        })
    }
    /// Never blocks: a full queue (or a gone console thread) drops the line and counts it.
    pub fn line(&self, text: String) {
        if self.tx.try_send(text).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        } else {
            self.queued.fetch_add(1, Ordering::AcqRel);
        }
    }
    /// Waits until every line queued so far is written, at most `timeout`; `false` if not.
    pub fn flush(&self, timeout: Duration) -> bool {
        let target = self.queued.load(Ordering::Acquire);
        let deadline = Instant::now() + timeout;
        let (count, done) = &*self.printed;
        let mut written = count.lock().unwrap_or_else(|e| e.into_inner());
        while *written < target {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            written = done
                .wait_timeout(written, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        true
    }
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

static PROCESS: OnceLock<Option<Console>> = OnceLock::new();
static UNSTARTED: AtomicU64 = AtomicU64::new(0);

fn process() -> Option<&'static Console> {
    PROCESS
        .get_or_init(|| Console::start(CAPACITY, std::io::stderr()).ok())
        .as_ref()
}
/// One line to stderr through the process console (never blocks the caller).
pub fn line(text: String) {
    match process() {
        Some(c) => c.line(text),
        None => {
            UNSTARTED.fetch_add(1, Ordering::Relaxed);
        }
    }
}
/// Waits, at most `timeout`, until the process console wrote every queued line (before an exit).
pub fn flush(timeout: Duration) -> bool {
    process().is_none_or(|c| c.flush(timeout))
}
/// Lines the process console dropped so far.
pub fn dropped() -> u64 {
    process().map_or(0, Console::dropped) + UNSTARTED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    /// A writer that blocks like a full pipe until the test releases it.
    struct Blocked(mpsc::Receiver<()>);
    impl Write for Blocked {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            let _ = self.0.recv();
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    /// F-16: a flush returns once every queued line is written (a process's last lines are not
    /// lost at its exit), and gives up at its bound on a blocked writer.
    #[test]
    fn flush_waits_for_the_queued_lines_within_its_bound() {
        struct Shared(Arc<Mutex<Vec<u8>>>);
        impl Write for Shared {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let out = Arc::new(Mutex::new(Vec::new()));
        let console = Console::start(64, Shared(out.clone())).unwrap();
        for i in 0..50 {
            console.line(format!("line {i}"));
        }
        assert!(console.flush(Duration::from_secs(5)));
        assert_eq!(
            String::from_utf8_lossy(&out.lock().unwrap())
                .lines()
                .count(),
            50
        );
        let (release, rx) = mpsc::channel();
        let blocked = Console::start(8, Blocked(rx)).unwrap();
        blocked.line("stuck".into());
        assert!(!blocked.flush(Duration::from_millis(50)));
        drop(release);
    }
    #[test]
    fn blocked_console_never_blocks_the_caller() {
        let (release, rx) = mpsc::channel();
        let console = Console::start(8, Blocked(rx)).unwrap();
        let t0 = Instant::now();
        for i in 0..1000 {
            console.line(format!("line {i}"));
        }
        assert!(t0.elapsed() < Duration::from_secs(1), "{:?}", t0.elapsed());
        assert!(console.dropped() >= 1000 - 8 - 1, "{}", console.dropped());
        drop(release);
    }
}

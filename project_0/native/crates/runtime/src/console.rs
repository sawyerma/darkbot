//! Bounded, non-blocking terminal output (D9, M2). Hot paths (capture loop, decoding, the owner
//! evidence writer) queue a line with `try_send`; one detached console thread prints it. A full
//! queue drops the line and counts it, so a blocked stderr never stalls ingress, a writer or a
//! shutdown, and nothing ever joins the console thread. Lines that must not be lost belong in a
//! journal, not here.
use std::io::Write;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc, Arc, OnceLock,
};

/// Queued lines of the process console before further lines are dropped.
pub const CAPACITY: usize = 1024;

pub struct Console {
    tx: mpsc::SyncSender<String>,
    dropped: Arc<AtomicU64>,
}
impl Console {
    /// A console printing to `out` on its own detached thread.
    pub fn start<W: Write + Send + 'static>(capacity: usize, mut out: W) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::sync_channel::<String>(capacity);
        std::thread::Builder::new()
            .name("p0-console".into())
            .spawn(move || {
                for line in rx {
                    let _ = writeln!(out, "{line}");
                }
            })?;
        Ok(Self {
            tx,
            dropped: Arc::new(AtomicU64::new(0)),
        })
    }
    /// Never blocks: a full queue (or a gone console thread) drops the line and counts it.
    pub fn line(&self, text: String) {
        if self.tx.try_send(text).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
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

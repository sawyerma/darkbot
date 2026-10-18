//! Composition root: the engine never depends on the reporter crate.
use darkbot_p0_reporter::{config_from_env, start, ReporterHandle};
use darkbot_runner::market::observer::{run_from_env, run_from_env_with};
use darkbot_runner::status_tap::StatusTap;
use std::{process::ExitCode, sync::atomic::Ordering};
fn main() -> ExitCode {
    let env = |k: &str| std::env::var(k).ok();
    let reporting = env("P0_REPORT_SOCKET").is_some_and(|s| !s.trim().is_empty());
    let result = if !reporting {
        run_from_env()
    } else {
        let tap = StatusTap::new();
        let tap_reporter = tap.clone();
        let mut handle: Option<ReporterHandle> = None;
        let result = run_from_env_with(Some(tap), |run_id, _| {
            let cfg = config_from_env(run_id, &env)?.ok_or("reporter configuration missing")?;
            let h = start(cfg, tap_reporter)?;
            eprintln!("[P0/REPORTER] socket={} generation={}", h.socket_path().display(), h.generation);
            handle = Some(h);
            Ok(())
        });
        if let Some(h) = handle {
            eprintln!("[P0/REPORTER] connections={} frames_sent={} write_errors={}",
                h.counters.connections.load(Ordering::Relaxed),
                h.counters.frames_sent.load(Ordering::Relaxed),
                h.counters.write_errors.load(Ordering::Relaxed));
            h.stop();
        }
        result
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => { eprintln!("{e}"); ExitCode::FAILURE }
    }
}

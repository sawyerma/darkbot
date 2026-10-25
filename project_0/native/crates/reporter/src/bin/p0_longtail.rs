//! Project_0 executable (Shadow/econ4h runtime): configuration and lifecycle are owned by
//! orchestration. Composition root: with P0_REPORT_SOCKET the status reporter starts once the
//! process holds the project's runtime lock, and the state owner publishes its projection into
//! the installed tap (P0-PROD-CTRL-04). Reporting is display only: a reporter that cannot start
//! (e.g. its socket served by another process) leaves the run unreported, never refused. The
//! engine never depends on the reporter crate.
use darkbot_p0_reporter::{config_from_env, start, ReporterHandle};
use darkbot_runner::status_tap::{self, StatusTap};
use std::time::Duration;

/// Starts the reporter and installs the tap; `None` (reported on stderr) leaves the run
/// unreported.
fn attach(tap: &StatusTap) -> Option<ReporterHandle> {
    let env = |k: &str| std::env::var(k).ok();
    let run_id = darkbot_runner::orchestration::run_id_from_env();
    let mut cfg = match config_from_env(&run_id, &env) {
        Ok(Some(cfg)) => cfg,
        Ok(None) => return None,
        Err(e) => {
            eprintln!("[P0/REPORTER] REPORTER_CONFIG_INVALID {e}: run unreported");
            return None;
        }
    };
    cfg.owner = true;
    match start(cfg, tap.clone()) {
        Ok(handle) => {
            if let Err(code) = status_tap::install(tap.clone()) {
                eprintln!("[P0/REPORTER] {code}: run unreported");
                handle.stop();
                return None;
            }
            eprintln!(
                "[P0/REPORTER] socket={} generation={}",
                handle.socket_path().display(),
                handle.generation
            );
            Some(handle)
        }
        Err(e) => {
            eprintln!("[P0/REPORTER] REPORTER_UNAVAILABLE {e}: run unreported");
            None
        }
    }
}

fn main() {
    let tap = StatusTap::new();
    let mut reporter = None;
    let result = darkbot_runner::orchestration::run_with(&mut || reporter = attach(&tap));
    if let Some(handle) = reporter {
        // The final owner state reaches the gateway before the socket closes, bounded by
        // `report_final_flush_ms` (D01).
        tap.flush(Duration::from_millis(
            status_tap::active_limits().report_final_flush_ms,
        ));
        handle.stop();
    }
    if let Err(e) = result {
        eprintln!(
            "p0-longtail: {}",
            darkbot_runner::orchestration::failure_report(e.as_ref())
        );
        std::process::exit(2);
    }
}

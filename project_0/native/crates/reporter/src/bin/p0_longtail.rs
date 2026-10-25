//! Project_0 executable (Shadow/econ4h runtime): configuration and lifecycle are owned by
//! orchestration. Composition root: with P0_REPORT_SOCKET the status reporter starts first and
//! the state owner publishes its projection into the installed tap (P0-PROD-CTRL-04); without it
//! the runtime runs unreported. The engine never depends on the reporter crate.
use darkbot_p0_reporter::{config_from_env, start};
use darkbot_runner::status_tap::{self, StatusTap};
use std::time::Duration;

/// Bound of the final status handoff before the reporter stops.
const FINAL_FLUSH: Duration = Duration::from_secs(2);

fn main() {
    let env = |k: &str| std::env::var(k).ok();
    let mut reporter = None;
    let tap = StatusTap::new();
    let run_id = darkbot_runner::orchestration::run_id_from_env();
    match config_from_env(&run_id, &env) {
        Ok(Some(mut cfg)) => {
            cfg.owner = true;
            match start(cfg, tap.clone()) {
                Ok(handle) => {
                    eprintln!(
                        "[P0/REPORTER] socket={} generation={}",
                        handle.socket_path().display(),
                        handle.generation
                    );
                    if let Err(code) = status_tap::install(tap.clone()) {
                        eprintln!("p0-longtail: {code}");
                        std::process::exit(2);
                    }
                    reporter = Some(handle);
                }
                Err(e) => {
                    eprintln!("p0-longtail: REPORTER_START_FAILED {e}");
                    std::process::exit(2);
                }
            }
        }
        Ok(None) => {}
        Err(e) => {
            eprintln!("p0-longtail: REPORTER_CONFIG_INVALID {e}");
            std::process::exit(2);
        }
    }
    let result = darkbot_runner::orchestration::run();
    if let Some(handle) = reporter {
        // The final owner state reaches the gateway before the socket closes.
        tap.flush(FINAL_FLUSH);
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

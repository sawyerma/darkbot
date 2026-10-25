//! B03/U-9: the Project 0 analytics publisher — project0.analytics.jobs without a capture or
//! runtime. It publishes the job store and run list of `P0_REPORT_ANALYTICS_PROJECT_DIR` on its
//! own socket (`P0_REPORT_SOCKET`, generation `P0_REPORT_GENERATION_FILE`), the gateway's
//! auxiliary `project0.analytics` feed, and nothing else; launchd keeps it alive (darkbotctl).
//! Its bounds (`analytics_*`) come from the typed operational configuration of its environment.
use darkbot_p0_reporter::{config_from_env, start};
use darkbot_runner::orchestration::control;
use darkbot_runner::status_tap::{self, StatusTap};
use std::process::ExitCode;
use std::time::Duration;

fn run() -> Result<(), String> {
    control::install_stop_handler()?;
    status_tap::activate_from_env()?;
    let mut cfg = config_from_env("project0-analytics", &|k| std::env::var(k).ok())?
        .ok_or("P0_REPORT_SOCKET required")?;
    if cfg.project_dir.is_none() {
        return Err("P0_REPORT_ANALYTICS_PROJECT_DIR required".into());
    }
    cfg.capture = false;
    let heartbeat = cfg.heartbeat;
    let handle = start(cfg, StatusTap::new())?;
    while !control::stop_requested() {
        std::thread::park_timeout(heartbeat.min(Duration::from_secs(1)));
    }
    handle.stop();
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("p0-analytics-reporter: {e}");
            ExitCode::from(2)
        }
    }
}

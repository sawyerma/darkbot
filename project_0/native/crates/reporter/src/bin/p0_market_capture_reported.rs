//! Composition root: the engine never depends on the reporter crate.
use darkbot_p0_reporter::{config_from_env, start, ReporterHandle};
use darkbot_runner::market::observer::{run_from_env, run_from_env_with};
use darkbot_runner::status_tap::StatusTap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{process::ExitCode, sync::atomic::Ordering};
fn main() -> ExitCode {
    // Reporter composition root: allow SIGTERM/SIGINT to drain and seal market journals.
    if let Err(e) = darkbot_runner::orchestration::control::install_stop_handler() {
        eprintln!("[P0/STOP] {e}");
        write_result(&Err("STOP_HANDLER_FAILED"), None, None);
        return ExitCode::FAILURE;
    }
    if let Some(spec) = std::env::var("P0_ENV_FILE")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        match darkbot_p0_reporter::env_file::load_list(&spec) {
            Ok(files) => {
                for f in files {
                    eprintln!(
                        "[P0/ENV] {} set={} kept_existing={}",
                        f.file.display(),
                        f.set.join(","),
                        f.kept_existing.join(",")
                    );
                }
            }
            Err(e) => {
                eprintln!("[P0/ENV] {e}");
                write_result(&Err("ENV_LOAD_FAILED"), None, None);
                return ExitCode::FAILURE;
            }
        }
    }
    let env = |k: &str| std::env::var(k).ok();
    let reporting = env("P0_REPORT_SOCKET").is_some_and(|s| !s.trim().is_empty());
    let mut run: Option<String> = None;
    let mut generation: Option<u64> = None;
    let result = if !reporting {
        run_from_env()
    } else {
        let tap = StatusTap::new();
        let tap_reporter = tap.clone();
        let mut handle: Option<ReporterHandle> = None;
        let result = run_from_env_with(Some(tap), |run_id, _| {
            run = Some(run_id.to_string());
            let cfg = config_from_env(run_id, &env)?.ok_or("reporter configuration missing")?;
            let h = start(cfg, tap_reporter)?;
            eprintln!(
                "[P0/REPORTER] socket={} generation={}",
                h.socket_path().display(),
                h.generation
            );
            handle = Some(h);
            Ok(())
        });
        if let Some(h) = handle {
            generation = Some(h.generation);
            eprintln!(
                "[P0/REPORTER] connections={} frames_sent={} write_errors={}",
                h.counters.connections.load(Ordering::Relaxed),
                h.counters.frames_sent.load(Ordering::Relaxed),
                h.counters.write_errors.load(Ordering::Relaxed)
            );
            h.stop();
        }
        result
    };
    write_result(&result, run.as_deref(), generation);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// Code-shaped token only (A–Z, 0–9, `_`): no URL, key or provider text can pass.
fn code_token(s: &str) -> Option<&str> {
    let t = s.split(':').next()?.trim();
    let shaped = (3..=64).contains(&t.len())
        && t.as_bytes()[0].is_ascii_uppercase()
        && t.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    shaped.then_some(t)
}

fn result_code<E: std::fmt::Display>(result: &Result<(), E>) -> String {
    match result {
        Ok(()) => "CAPTURE_COMPLETE".to_owned(),
        Err(e) => code_token(&e.to_string())
            .unwrap_or("RUN_FAILED")
            .to_owned(),
    }
}

fn safe_id(s: &str) -> &str {
    if s.is_empty() {
        return "none";
    }
    let ok = s.len() <= 96
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    if ok {
        s
    } else {
        "invalid"
    }
}

/// Final run result for the launcher (`P0_RESULT_FILE`), written once at process end: owner-only
/// (0600), atomically (temp + rename), tied to launch, run and reporter generation. A hard kill
/// leaves no file; the launcher then takes no decision from it.
fn write_result<E: std::fmt::Display>(
    result: &Result<(), E>,
    run: Option<&str>,
    generation: Option<u64>,
) {
    let Some(path) = std::env::var_os("P0_RESULT_FILE").map(std::path::PathBuf::from) else {
        return;
    };
    let launch = std::env::var("P0_LAUNCH_ID").unwrap_or_default();
    let ended = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let body = format!(
        "launch_id={}\nrun_id={}\ngeneration={}\nended_unix_s={ended}\ncode={}\n",
        safe_id(&launch),
        safe_id(run.unwrap_or("")),
        generation.map_or_else(|| "none".to_owned(), |g| g.to_string()),
        result_code(result),
    );
    if let Err(_) = persist_result(&path, &body) {
        eprintln!("[P0/RESULT] could not write the result file");
    }
}

fn persist_result(path: &std::path::Path, body: &str) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| {
            f.write_all(body.as_bytes())?;
            f.sync_all()
        })
        .and_then(|()| std::fs::rename(&tmp, path))
        .and_then(|()| {
            std::fs::File::open(path.parent().unwrap_or(std::path::Path::new(".")))?.sync_all()
        });
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_code_shaped_results_are_persisted() {
        assert_eq!(result_code::<String>(&Ok(())), "CAPTURE_COMPLETE");
        let not_ready =
            Err("MARKET_OBSERVER_STREAM_NOT_READY: no StreamReady within 120s".to_owned());
        assert_eq!(result_code(&not_ready), "MARKET_OBSERVER_STREAM_NOT_READY");
        assert_eq!(
            result_code(&Err("wss://h/?api-key=abc failed".to_owned())),
            "RUN_FAILED"
        );
        assert_eq!(
            result_code(&Err("reporter bind: Address already in use".to_owned())),
            "RUN_FAILED"
        );
    }

    #[test]
    fn atomic_result_is_private_and_replaces_previous_result() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("p0-result-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("capture.result");
        persist_result(&path, "launch_id=first\n").unwrap();
        persist_result(&path, "launch_id=second\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "launch_id=second\n"
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn identifiers_are_restricted() {
        assert_eq!(safe_id(""), "none");
        assert_eq!(safe_id("market-20261019T101500"), "market-20261019T101500");
        assert_eq!(safe_id("a/b?key=x"), "invalid");
    }
}

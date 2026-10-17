//! SK-06 capacity preflight CLI: measured data-rate basis (sealed typed market journals of earlier
//! captures), a write probe on the target volume, OS facts and operational budgets. Prints the
//! decision and keeps `capacity_preflight.p0j` in OUT. Exit 0 = start allowed, 2 = not allowed.
//!
//! `p0-capacity --duration SECS --out DIR [--probe-mib N] [--compat-json PATH] [JOURNAL.p0j...]`
//!
//! `--compat-json` additionally writes the run-configuration part the transitional Python run
//! store requires (`storage_preflight.json`); the native report is the record.
use darkbot_runner::storage::preflight::{run, Request};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = (|| -> Result<bool, String> {
        let (mut duration, mut out, mut probe_mib, mut compat) = (None, None, 64u64, None);
        let mut journals = Vec::new();
        let mut it = args.iter();
        while let Some(a) = it.next() {
            let mut value = || it.next().cloned().ok_or(format!("{a} needs a value"));
            match a.as_str() {
                "--duration" => {
                    duration = Some(value()?.parse::<u64>().map_err(|_| "DURATION_INVALID")?)
                }
                "--out" => out = Some(PathBuf::from(value()?)),
                "--probe-mib" => probe_mib = value()?.parse().map_err(|_| "PROBE_INVALID")?,
                "--compat-json" => compat = Some(PathBuf::from(value()?)),
                other => journals.push(PathBuf::from(other)),
            }
        }
        let (duration, out) = (
            duration.ok_or("--duration required")?,
            out.ok_or("--out required")?,
        );
        if duration == 0 {
            return Err("DURATION_INVALID".into());
        }
        let report = run(
            Request::from_active(duration, &out),
            &journals,
            probe_mib * 1024 * 1024,
        )?;
        print!("{}", report.terminal());
        if let Some(path) = compat {
            let v = serde_json::json!({
                "schema": "Project0/StoragePreflightCompat/1",
                "duration_seconds": duration,
                "passed": report.start_allowed,
                "verdict": report.verdict.as_str(),
                "free_bytes": report.environment.free_disk_bytes,
                "native_report": "capacity_preflight.p0j",
            });
            let mut f = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            serde_json::to_writer_pretty(&mut f, &v).map_err(|e| e.to_string())?;
        }
        Ok(report.start_allowed)
    })();
    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(2),
        Err(e) => {
            eprintln!("CAPACITY_PREFLIGHT_FAILED: {e}");
            std::process::exit(2);
        }
    }
}

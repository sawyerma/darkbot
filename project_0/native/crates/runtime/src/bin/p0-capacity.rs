//! SK-06 capacity preflight: measured data-rate basis (sealed journals of earlier runs), a write
//! and sync probe on the target volume, OS facts, operational budgets and the verifier budget of
//! the selected chain-verify mode. Prints the decision and keeps `capacity_preflight.p0j` in OUT.
//! Exit 0 = start allowed, 2 = not allowed or error. Read-only apart from OUT; no network.
//!
//! `p0-capacity --duration SECS --out DIR [--capture-source logs|transactions]
//!   [--chain-verify off|index|strict|crosscheck] [--verify-rps N] [--verify-calls N]
//!   [--verify-seconds N] [--verify-max-pages N] [--verify-bytes N] [--verify-disk-bytes N]
//!   [--basis-root DIR [--exclude-run DIR]] [--basis-run DIR]... [--probe-mib N]`
//!
//! `--basis-root` takes the newest valid runs under DIR (the run store root); `--basis-run`
//! names runs explicitly (newest first). The typed report is the only record (no JSON render).
use darkbot_runner::storage::preflight::{self, discover_runs, Request, VerifyMode};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = (|| -> Result<bool, String> {
        let (mut duration, mut out, mut probe_mib) = (None, None, 64u64);
        let (mut source, mut mode) = ("logs".to_string(), VerifyMode::Off);
        let (mut basis_root, mut exclude, mut runs) = (None, None, Vec::new());
        let mut verify: Vec<(String, u64)> = Vec::new();
        let mut it = args.iter();
        while let Some(a) = it.next() {
            let mut value = || it.next().cloned().ok_or(format!("{a} needs a value"));
            let number = |v: String| {
                v.parse::<u64>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or(format!("{a} must be a positive integer"))
            };
            match a.as_str() {
                "--duration" => duration = Some(number(value()?)?),
                "--out" => out = Some(PathBuf::from(value()?)),
                "--capture-source" => match value()?.as_str() {
                    s @ ("logs" | "transactions") => source = s.into(),
                    s => return Err(format!("CAPTURE_SOURCE_INVALID: {s}")),
                },
                "--chain-verify" => mode = VerifyMode::parse(&value()?)?,
                "--verify-rps"
                | "--verify-calls"
                | "--verify-seconds"
                | "--verify-max-pages"
                | "--verify-bytes"
                | "--verify-disk-bytes" => verify.push((a.clone(), number(value()?)?)),
                "--basis-root" => basis_root = Some(PathBuf::from(value()?)),
                "--exclude-run" => exclude = Some(PathBuf::from(value()?)),
                "--basis-run" => runs.push(PathBuf::from(value()?)),
                "--probe-mib" => probe_mib = number(value()?)?,
                other => return Err(format!("UNKNOWN_ARGUMENT: {other}")),
            }
        }
        let duration = duration.ok_or("--duration required")?;
        let out = out.ok_or("--out required")?;
        darkbot_runner::analytics::measurement::MeasurementPlan::new(duration)?;
        let mut req = Request::from_active(duration, &out, &source);
        req.verify.mode = mode;
        for (flag, n) in verify {
            let v = &mut req.verify;
            match flag.as_str() {
                "--verify-rps" => v.rps = n,
                "--verify-calls" => v.calls = n,
                "--verify-seconds" => v.seconds = n,
                "--verify-max-pages" => v.max_pages = n,
                "--verify-bytes" => v.bytes = n,
                _ => v.disk_bytes = n,
            }
        }
        if let Some(root) = basis_root {
            runs.extend(discover_runs(&root, exclude.as_deref()));
        }
        let report = preflight::run(req, &runs, probe_mib * 1024 * 1024)?;
        print!("{}", report.terminal());
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

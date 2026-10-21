//! Strict verifier CLI. Evidence is native `.p0j` (scope, RPC cache, batch journals, checkpoints,
//! results); the terminal output is human-readable.
//!
//! `p0-strict-verify JOB`            verify (resumable) and print the native report
//! `p0-strict-verify JOB --compat`   additionally render the legacy JSON checkpoints/views and
//!                                   print the legacy JSON summary (transitional: the pinned
//!                                   Python driver/report only; removed with the native chain, B7)
//! `p0-strict-verify --show DIR`     read and verify existing native evidence, no RPC
//!
//! `p0-strict-verify --run RUN [--mode strict|off] [--resume-verification] [--offline] [--verify-rps N]
//!     [--verify-calls N] [--verify-seconds N] [--verify-bytes N] [--verify-disk-bytes N]
//!     [--verify-ram-bytes N] [--verify-min-free-bytes N] [--provider-evidence FILE.p0j]`
//!                                   A3 native driver for a layout-V2 run: typed spec, capture
//!                                   evidence, attempt and result identities (no JSON job)
//!
//! JOB is the verification job file written by the transitional Python driver. Exit 0 = verified
//! (`--compat`: result rendered, whatever its status), 3 = verification incomplete, 2 = error.
use darkbot_runner::market_observer::verification::{read_report, render_compat, run, Job};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = (|| -> Result<(String, bool), String> {
        if let [flag, dir] = args.as_slice() {
            if flag == "--show" {
                let report = read_report(Path::new(dir));
                return Ok((report.terminal(), report.complete()));
            }
        }
        if args.first().map(String::as_str) == Some("--run") {
            return darkbot_runner::market_observer::strict_driver::cli(&args);
        }
        let (path, compat) = match args.as_slice() {
            [path] => (path, false),
            [path, flag] if flag == "--compat" => (path, true),
            _ => return Err("USAGE: p0-strict-verify JOB [--compat] | --show DIR".into()),
        };
        let job: Job = serde_json::from_reader(std::fs::File::open(path).map_err(|_| "JOB_IO")?)
            .map_err(|_| "JOB_INVALID")?;
        if std::env::var_os("P0_OPERATIONAL_CONFIG_FILE").is_some()
            || std::env::var_os("P0_PROFILE").is_some()
        {
            let resolved =
                darkbot_solana::runtime_config::Resolved::parse(&[], &|k| std::env::var(k).ok())?;
            for (k, v) in resolved.config.environment() {
                std::env::set_var(k, v);
            }
            resolved.activate()?;
        }
        darkbot_solana::provider::enable_measurement_budget();
        let output = job.output.clone();
        let compat_job = compat.then(|| job.clone());
        let result = run(job)?;
        if let Some(job) = compat_job {
            return render_compat(&job, &result).map(|v| (v.to_string(), true));
        }
        let report = read_report(&output);
        Ok((report.terminal(), report.complete()))
    })();
    match result {
        Ok((text, complete)) => {
            println!("{text}");
            // Native modes: an incomplete verification is exit 3 (the evidence stays resumable).
            if !complete {
                std::process::exit(3);
            }
        }
        Err(error) => {
            eprintln!("HISTORICAL_INCOMPLETE: {error}");
            std::process::exit(2);
        }
    }
}

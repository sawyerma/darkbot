//! Strict verifier CLI. Evidence is native `.p0j` (scope, RPC cache, batch journals, checkpoints,
//! results); the terminal output is human-readable.
//!
//! `p0-strict-verify JOB`            verify (resumable) and print the native report
//! `p0-strict-verify JOB --compat`   additionally render the legacy JSON checkpoints/views and
//!                                   print the legacy JSON summary (transitional: the pinned
//!                                   Python driver/report only; removed with the native chain, B7)
//! `p0-strict-verify --show DIR`     read and verify existing native evidence, no RPC
//!
//! `p0-strict-verify --run RUN [--resume-verification] [--offline] [--verify-rps N]
//!     [--verify-calls N] [--verify-seconds N] [--verify-bytes N] [--verify-disk-bytes N]
//!     [--verify-ram-bytes N] [--verify-min-free-bytes N]`
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
            return native_driver(&args);
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

/// A3: drive a strict verification of RUN natively (budgets default to the operational config).
fn native_driver(args: &[String]) -> Result<(String, bool), String> {
    use darkbot_runner::market_observer::strict_driver::{drive, DriverRequest, StrictBudget};
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
    let l = &darkbot_solana::runtime_config::active().limits;
    let mib = 1024 * 1024;
    let mut budget = StrictBudget {
        rps: l.verify_rps,
        max_calls: l.verify_calls,
        max_seconds: l.verify_seconds,
        max_bytes: l.verify_mib * mib,
        max_disk_bytes: l.verify_disk_mib * mib,
        max_ram_bytes: l.verify_ram_mib * mib,
        min_free_bytes: l.verify_free_mib * mib,
    };
    let (mut run, mut offline, mut resume) = (None, false, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut number = || -> Result<u64, String> {
            it.next()
                .and_then(|v| v.parse().ok())
                .filter(|n| *n > 0)
                .ok_or(format!("{a} needs a positive integer"))
        };
        match a.as_str() {
            "--run" => run = Some(it.next().ok_or("--run needs a directory")?.clone()),
            "--offline" => offline = true,
            "--resume-verification" => resume = true,
            "--verify-rps" => budget.rps = number()?,
            "--verify-calls" => budget.max_calls = number()?,
            "--verify-seconds" => budget.max_seconds = number()?,
            "--verify-bytes" => budget.max_bytes = number()?,
            "--verify-disk-bytes" => budget.max_disk_bytes = number()?,
            "--verify-ram-bytes" => budget.max_ram_bytes = number()?,
            "--verify-min-free-bytes" => budget.min_free_bytes = number()?,
            other => return Err(format!("UNKNOWN_ARGUMENT: {other}")),
        }
    }
    let outcome = drive(&DriverRequest {
        run: run.ok_or("--run required")?.into(),
        budget,
        offline,
        resume,
    })?;
    let text = format!(
        "{}verification_spec_id={}\nverification_attempt_id={}\nverification_result_id={}\nstatus={} reason={}\n",
        outcome.terminal,
        outcome.verification_spec_id,
        outcome.verification_attempt_id,
        outcome.verification_result_id.as_deref().unwrap_or("none"),
        outcome.status,
        outcome.reason.as_deref().unwrap_or("none")
    );
    Ok((text, outcome.status == "CANONICAL_VERIFIED"))
}

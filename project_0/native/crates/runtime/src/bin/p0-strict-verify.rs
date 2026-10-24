//! Strict verifier CLI. Evidence is native `.p0j` (scope, RPC cache, batch journals, checkpoints,
//! results); the terminal output is human-readable text. No JSON job and no JSON render
//! (JFI-01, P0-PROD-CTRL-02): the former Python driver handoff `JOB [--compat]` is removed.
//!
//! `p0-strict-verify --show DIR`     read and verify existing native evidence, no RPC
//!
//! `p0-strict-verify --run RUN [--mode strict|off] [--resume-verification] [--offline] [--verify-rps N]
//!     [--verify-calls N] [--verify-seconds N] [--verify-bytes N] [--verify-disk-bytes N]
//!     [--verify-ram-bytes N] [--verify-min-free-bytes N] [--provider-evidence FILE.p0j]`
//!                                   A3 native driver for a layout-V2 run (the same driver as
//!                                   `p0ctl verify`)
//!
//! Exit 0 = verified, 3 = verification incomplete, 2 = error.
use darkbot_runner::market_observer::verification::read_report;
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
        Err("USAGE: p0-strict-verify --run RUN … | --show DIR".into())
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

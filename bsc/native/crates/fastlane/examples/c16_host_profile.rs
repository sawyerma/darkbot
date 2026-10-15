//! C1.6 host/profile report.
//!
//! Prints the host profile evidence of this machine as one JSON object:
//! topology, its fingerprint, the profile hint, the profile selection and,
//! if requested, the outcome of a QoS request for this thread.
//!
//! ```text
//! c16_host_profile [--profile S|P] [--qos CLASS] [--out PATH]
//! ```
//!
//! `CLASS` is user-interactive, user-initiated, default, utility or
//! background. The report carries no timestamp, so two runs on the same host
//! produce the same topology section and fingerprint. A configured Profile P
//! is refused in C1 (exit code 2); it is never replaced by S.

use darkbot_fastlane::host::{HostProfileReport, ProfileRequest, QosClass, SchedulingProfile};
use std::env;
use std::fs;
use std::process::ExitCode;

fn usage(problem: &str) -> ExitCode {
    eprintln!("c16_host_profile: {problem}");
    eprintln!("usage: c16_host_profile [--profile S|P] [--qos CLASS] [--out PATH]");
    ExitCode::from(64)
}

fn main() -> ExitCode {
    let mut configured = None;
    let mut qos = None;
    let mut out = None;
    let mut args = env::args().skip(1);
    while let Some(flag) = args.next() {
        let Some(value) = args.next() else {
            return usage("missing value");
        };
        match (flag.as_str(), value.as_str()) {
            ("--profile", "S") => configured = Some(SchedulingProfile::S),
            ("--profile", "P") => configured = Some(SchedulingProfile::P),
            ("--qos", name) => match QosClass::parse(name) {
                Some(class) => qos = Some(class),
                None => return usage("unknown QoS class"),
            },
            ("--out", path) => out = Some(path.to_string()),
            _ => return usage("unknown argument"),
        }
    }
    let request = ProfileRequest {
        configured,
        parallel_builders_available: false,
    };
    let report = HostProfileReport::capture(request, qos);
    let json = report.to_json();
    match out {
        Some(path) => {
            if let Err(err) = fs::write(&path, format!("{json}\n")) {
                eprintln!("c16_host_profile: cannot write {path}: {err}");
                return ExitCode::from(74);
            }
        }
        None => println!("{json}"),
    }
    if report.selection.is_err() {
        eprintln!("C1_6_PROFILE_REFUSED parallel builders are not available in C1");
        return ExitCode::from(2);
    }
    eprintln!("C1_6_HOST_PROFILE_CAPTURED");
    ExitCode::SUCCESS
}

//! C1.6 host/profile report.
//!
//! Writes the host profile evidence of this machine as one binary record
//! (`darkbot.c1.host-profile.v1`, DBREC001): topology, its fingerprint, the
//! profile hint, the profile selection and, if requested, the outcome of a
//! QoS request for this thread. Stdout carries the derived text view.
//!
//! ```text
//! c16_host_profile [--profile S|P] [--qos CLASS] [--out PATH]
//! ```
//!
//! `CLASS` is user-interactive, user-initiated, default, utility or
//! background. `PATH` must not exist yet. The record carries no timestamp,
//! so two runs on the same host produce the same bytes. A configured
//! Profile P is refused in C1 (exit code 2, the record says so); it is never
//! replaced by S.

use darkbot_bsc_geth_ipc::host::{HostProfileReport, ProfileRequest, QosClass, SchedulingProfile};
use darkbot_bsc_record::decode;
use darkbot_bsc_record::schema::c1_qualification::host_profile;
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::process::ExitCode;

fn usage(problem: &str) -> ExitCode {
    eprintln!("c16_host_profile: {problem}");
    eprintln!("usage: c16_host_profile [--profile S|P] [--qos CLASS] [--out PATH]");
    ExitCode::from(64)
}

fn write_new(path: &str, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
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
    let bytes = match report.to_record().encode() {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("c16_host_profile: cannot encode the record: {err}");
            return ExitCode::from(70);
        }
    };
    if let Some(path) = &out
        && let Err(err) = write_new(path, &bytes)
    {
        eprintln!("c16_host_profile: cannot write {path}: {err}");
        return ExitCode::from(74);
    }
    match decode(&host_profile::SCHEMA, &bytes) {
        Ok(record) => print!("{}", record.render()),
        Err(err) => {
            eprintln!("c16_host_profile: the record does not decode: {err}");
            return ExitCode::from(70);
        }
    }
    if report.selection.is_err() {
        eprintln!("C1_6_PROFILE_REFUSED parallel builders are not available in C1");
        return ExitCode::from(2);
    }
    eprintln!("C1_6_HOST_PROFILE_CAPTURED");
    ExitCode::SUCCESS
}

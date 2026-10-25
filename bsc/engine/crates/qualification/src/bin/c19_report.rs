//! C1.9 native latency report: raw run evidence in, one
//! `darkbot.c1.native-latency.v1` record out, a derived text view on stdout.
//! See [`darkbot_bsc_qualification::c1_latency`].

use darkbot_bsc_qualification::{c1_common::Args, c1_latency};
use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(c1_latency::main_with(Args::parse(std::env::args().skip(1))))
}

//! C1.9b hook/path report (Testnet only), and the builder of its controlled
//! launches record. See [`darkbot_bsc_qualification::c1_hook_path`].

use darkbot_bsc_qualification::{c1_common::Args, c1_hook_path};
use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(c1_hook_path::main_with(Args::parse(
        std::env::args().skip(1),
    )))
}

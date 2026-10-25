//! C1.10 freeze: inputs record builder, evaluation, freeze and verification.
//! See [`darkbot_bsc_qualification::c1_freeze`].

use darkbot_bsc_qualification::c1_common::{Args, repo_root_head};
use darkbot_bsc_qualification::c1_freeze;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args = Args::parse(std::env::args().skip(1));
    // The geth checkout must be a repository of its own.
    ExitCode::from(c1_freeze::main_with(args, &repo_root_head))
}

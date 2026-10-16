//! Writes the binary acceptance contract of G0.7 attempt 1
//! (`V3_RELATIVE_PATH`) from its reviewed source values and the committed
//! input records, or with `--check` verifies the committed file is exactly
//! those bytes. Offline; run from the repository root. Writing the contract
//! arms nothing: the guard stays UNARMED until a separate, explicitly
//! approved arming commit compiles the file's SHA-256.

use darkbot_bsc_execution::acceptance_contract::{attempt1_bindings, attempt1_v3};
use darkbot_bsc_execution::acceptance_guard::{V3_RELATIVE_PATH, sha256_hex};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let check_only = match std::env::args().nth(1).as_deref() {
        None => false,
        Some("--check") => true,
        Some(other) => {
            eprintln!("unknown argument {other:?}; usage: g07_acceptance_contract [--check]");
            return ExitCode::from(2);
        }
    };
    let bytes = match attempt1_bindings(Path::new(".")).and_then(|b| attempt1_v3(&b)) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("STOP: {e}");
            return ExitCode::FAILURE;
        }
    };
    if check_only {
        return match fs::read(V3_RELATIVE_PATH) {
            Ok(on_disk) if on_disk == bytes => {
                println!("CONTRACT_V3_REPRODUCED={V3_RELATIVE_PATH} sha256={}", sha256_hex(&bytes));
                ExitCode::SUCCESS
            }
            Ok(_) => {
                eprintln!("STOP: {V3_RELATIVE_PATH} differs from its source values");
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("STOP: {V3_RELATIVE_PATH}: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(V3_RELATIVE_PATH)
        .and_then(|mut f| f.write_all(&bytes).and_then(|()| f.sync_all()));
    match written {
        Ok(()) => {
            println!("WROTE={V3_RELATIVE_PATH} sha256={}", sha256_hex(&bytes));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("STOP: create-new {V3_RELATIVE_PATH}: {e}");
            ExitCode::FAILURE
        }
    }
}

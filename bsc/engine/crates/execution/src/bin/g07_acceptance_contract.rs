//! The binary acceptance contract of G0.7 attempt 1 (`V3_RELATIVE_PATH`),
//! from its reviewed source values and the committed input records.
//! Offline; run from the repository root.
//!
//! - no argument: writes the proposal (`approved=false`) to a new file;
//! - `--check`: verifies the committed file is exactly the proposal;
//! - `--approved-sha`: prints the SHA-256 of the approved variant, writes
//!   nothing (the hash the user approves);
//! - `--write-approved`: replaces the file with the approved variant, only
//!   when `DARKBOT_G07_CONTRACT_APPROVED_SHA256` equals that hash.
//!
//! None of this arms anything: the guard stays UNARMED until a separate,
//! explicitly approved arming commit compiles the approved file's SHA-256.

use darkbot_bsc_execution::acceptance_contract::{Authorization, attempt1_bindings, attempt1_v3};
use darkbot_bsc_execution::acceptance_guard::{V3_RELATIVE_PATH, sha256_hex};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

fn contract(authorization: Authorization) -> Result<Vec<u8>, String> {
    attempt1_bindings(Path::new(".")).and_then(|b| attempt1_v3(&b, authorization))
}

fn stop(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("STOP: {message}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let mode = std::env::args().nth(1);
    match mode.as_deref() {
        None => match contract(Authorization::Proposed) {
            Ok(bytes) => {
                let written = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(V3_RELATIVE_PATH)
                    .and_then(|mut f| f.write_all(&bytes).and_then(|()| f.sync_all()));
                match written {
                    Ok(()) => {
                        println!("WROTE_PROPOSAL={V3_RELATIVE_PATH} sha256={}", sha256_hex(&bytes));
                        ExitCode::SUCCESS
                    }
                    Err(e) => stop(format!("create-new {V3_RELATIVE_PATH}: {e}")),
                }
            }
            Err(e) => stop(e),
        },
        Some("--check") => match (contract(Authorization::Proposed), fs::read(V3_RELATIVE_PATH)) {
            (Ok(bytes), Ok(on_disk)) if on_disk == bytes => {
                println!(
                    "CONTRACT_V3_PROPOSAL_REPRODUCED={V3_RELATIVE_PATH} sha256={}",
                    sha256_hex(&bytes)
                );
                ExitCode::SUCCESS
            }
            (Ok(_), Ok(_)) => stop(format!("{V3_RELATIVE_PATH} is not the proposal")),
            (Err(e), _) => stop(e),
            (_, Err(e)) => stop(format!("{V3_RELATIVE_PATH}: {e}")),
        },
        Some("--approved-sha") => match contract(Authorization::Approved) {
            Ok(bytes) => {
                println!("APPROVED_VARIANT_SHA256={}", sha256_hex(&bytes));
                println!("NOTHING_WRITTEN=true GUARD=UNARMED");
                ExitCode::SUCCESS
            }
            Err(e) => stop(e),
        },
        Some("--write-approved") => {
            let bytes = match contract(Authorization::Approved) {
                Ok(bytes) => bytes,
                Err(e) => return stop(e),
            };
            let sha = sha256_hex(&bytes);
            match std::env::var("DARKBOT_G07_CONTRACT_APPROVED_SHA256") {
                Ok(approved) if approved == sha => {}
                Ok(_) => return stop("DARKBOT_G07_CONTRACT_APPROVED_SHA256 does not match the approved variant; nothing written"),
                Err(_) => return stop("DARKBOT_G07_CONTRACT_APPROVED_SHA256 (the user-approved SHA-256) is required; nothing written"),
            }
            let staged = format!("{V3_RELATIVE_PATH}.approved-staging");
            let written = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staged)
                .and_then(|mut f| f.write_all(&bytes).and_then(|()| f.sync_all()))
                .and_then(|()| fs::rename(&staged, V3_RELATIVE_PATH));
            match written {
                Ok(()) => {
                    println!("WROTE_APPROVED={V3_RELATIVE_PATH} sha256={sha}");
                    println!("GUARD=UNARMED (arming needs a separate approved commit)");
                    ExitCode::SUCCESS
                }
                Err(e) => stop(format!("write {V3_RELATIVE_PATH}: {e}")),
            }
        }
        Some(other) => {
            eprintln!(
                "unknown argument {other:?}; usage: g07_acceptance_contract [--check | --approved-sha | --write-approved]"
            );
            ExitCode::from(2)
        }
    }
}

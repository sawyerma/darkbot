//! The binary acceptance contract of G0.7 attempt 1 (`V5_RELATIVE_PATH`),
//! from its reviewed source values, the records of the requalification DRY
//! (with its launch-path snapshot) and the public record of the candidate.
//! Offline; run from the repository root.
//!
//! - `<requal_dir> <builder_policy> <candidate_record>`: writes the v5
//!   proposal (`approved=false`) to a new file;
//! - `... --proposal-out=<new-path>`: exports a new inactive review
//!   proposal, preserving the existing operator file;
//! - `... --check`: verifies the file is exactly that proposal;
//! - `... --approved-sha`: prints the SHA-256 of the approved variant (the
//!   hash the user approves) and writes nothing;
//! - `... --write-approved`: replaces the file with the approved variant,
//!   only when `DARKBOT_G07_CONTRACT_APPROVED_SHA256` equals that hash;
//! - `<requal_dir> <builder_policy> --check-v4`: verifies the superseded v4
//!   proposal is still reproduced from its (pre-C1/R1) requalification;
//! - `--check-v3`: verifies the superseded v3 proposal is still reproduced.
//!
//! None of this arms anything: the guard stays UNARMED until a separate,
//! explicitly approved arming commit compiles the approved file's SHA-256.

use darkbot_bsc_execution::acceptance_contract::{
    Authorization, attempt1_bindings, attempt1_v3, attempt1_v4, attempt1_v4_evidence, attempt1_v5,
    attempt1_v5_evidence,
};
use darkbot_bsc_execution::acceptance_guard::{
    V3_RELATIVE_PATH, V4_RELATIVE_PATH, V5_RELATIVE_PATH, sha256_hex,
};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

fn stop(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("STOP: {message}");
    ExitCode::FAILURE
}

/// `path` holds exactly `bytes` (a reproduced proposal).
fn reproduced(label: &str, path: &str, bytes: Result<Vec<u8>, String>) -> ExitCode {
    match (bytes, fs::read(path)) {
        (Ok(bytes), Ok(on_disk)) if on_disk == bytes => {
            println!(
                "CONTRACT_{label}_PROPOSAL_REPRODUCED={path} sha256={}",
                sha256_hex(&bytes)
            );
            ExitCode::SUCCESS
        }
        (Ok(_), Ok(_)) => stop(format!("{path} is not the {label} proposal")),
        (Err(e), _) => stop(e),
        (_, Err(e)) => stop(format!("{path}: {e}")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--check-v3"] {
        let bytes = attempt1_bindings(Path::new("."))
            .and_then(|b| attempt1_v3(&b, Authorization::Proposed));
        return reproduced("V3", V3_RELATIVE_PATH, bytes);
    }
    if let [dir, policy, mode] = args.as_slice()
        && mode == "--check-v4"
    {
        let bytes = attempt1_v4_evidence(Path::new("."), dir, policy)
            .and_then(|e| attempt1_v4(&e, Authorization::Proposed));
        return reproduced("V4", V4_RELATIVE_PATH, bytes);
    }
    let (requal_dir, builder_policy, candidate, mode) = match args.as_slice() {
        [dir, policy, candidate] => (dir, policy, candidate, None),
        [dir, policy, candidate, mode] => (dir, policy, candidate, Some(mode.as_str())),
        _ => {
            eprintln!(
                "usage: g07_acceptance_contract <requal_dir> <builder_policy> <candidate_record> [--check | --approved-sha | --write-approved | --proposal-out=<path>] | <requal_dir> <builder_policy> --check-v4 | --check-v3"
            );
            return ExitCode::from(2);
        }
    };
    let evidence = match attempt1_v5_evidence(Path::new("."), requal_dir, builder_policy, candidate)
    {
        Ok(evidence) => evidence,
        Err(e) => return stop(e),
    };
    let contract = |authorization| attempt1_v5(&evidence, authorization);
    let create_new = |path: &str, bytes: &[u8]| {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()))
    };
    match mode {
        None => match contract(Authorization::Proposed) {
            Ok(bytes) => match create_new(V5_RELATIVE_PATH, &bytes) {
                Ok(()) => {
                    println!(
                        "WROTE_PROPOSAL={V5_RELATIVE_PATH} sha256={}",
                        sha256_hex(&bytes)
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => stop(format!("create-new {V5_RELATIVE_PATH}: {e}")),
            },
            Err(e) => stop(e),
        },
        Some(mode) if mode.starts_with("--proposal-out=") => {
            let out = &mode["--proposal-out=".len()..];
            match contract(Authorization::Proposed) {
                Ok(bytes) => match create_new(out, &bytes) {
                    Ok(()) => {
                        println!("PROPOSAL_SHA256={}\nGUARD=UNARMED", sha256_hex(&bytes));
                        ExitCode::SUCCESS
                    }
                    Err(e) => stop(e),
                },
                Err(e) => stop(e),
            }
        }
        Some("--check") => reproduced("V5", V5_RELATIVE_PATH, contract(Authorization::Proposed)),
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
                Ok(_) => {
                    return stop(
                        "DARKBOT_G07_CONTRACT_APPROVED_SHA256 does not match the approved variant; nothing written",
                    );
                }
                Err(_) => {
                    return stop(
                        "DARKBOT_G07_CONTRACT_APPROVED_SHA256 (the user-approved SHA-256) is required; nothing written",
                    );
                }
            }
            let staged = format!("{V5_RELATIVE_PATH}.approved-staging");
            match create_new(&staged, &bytes).and_then(|()| fs::rename(&staged, V5_RELATIVE_PATH)) {
                Ok(()) => {
                    println!("WROTE_APPROVED={V5_RELATIVE_PATH} sha256={sha}");
                    println!("GUARD=UNARMED (arming needs a separate approved commit)");
                    ExitCode::SUCCESS
                }
                Err(e) => stop(format!("write {V5_RELATIVE_PATH}: {e}")),
            }
        }
        Some(other) => {
            eprintln!("unknown mode {other:?}");
            ExitCode::from(2)
        }
    }
}

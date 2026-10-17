//! The binary acceptance contract of the requalified G0.7 attempt 1
//! (`V4_RELATIVE_PATH`), from its reviewed source values and the records of
//! the requalification DRY. Offline; run from the repository root.
//!
//! - `<requal_dir> <builder_policy>`: writes the v4 proposal
//!   (`approved=false`) to a new file;
//! - `<requal_dir> <builder_policy> --check`: verifies the committed file is
//!   exactly that proposal;
//! - `<requal_dir> <builder_policy> --approved-sha`: prints the SHA-256 of
//!   the approved variant (the hash the user approves) and writes nothing;
//! - `<requal_dir> <builder_policy> --write-approved`: replaces the file with
//!   the approved variant, only when `DARKBOT_G07_CONTRACT_APPROVED_SHA256`
//!   equals that hash;
//! - `--check-v3`: verifies the superseded v3 proposal is still reproduced.
//!
//! None of this arms anything: the guard stays UNARMED until a separate,
//! explicitly approved arming commit compiles the approved file's SHA-256.

use darkbot_bsc_execution::acceptance_contract::{
    Authorization, attempt1_bindings, attempt1_v3, attempt1_v4, attempt1_v4_evidence,
};
use darkbot_bsc_execution::acceptance_guard::{V3_RELATIVE_PATH, V4_RELATIVE_PATH, sha256_hex};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

fn stop(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("STOP: {message}");
    ExitCode::FAILURE
}

fn check_v3() -> ExitCode {
    let bytes = match attempt1_bindings(Path::new("."))
        .and_then(|b| attempt1_v3(&b, Authorization::Proposed))
    {
        Ok(bytes) => bytes,
        Err(e) => return stop(e),
    };
    match fs::read(V3_RELATIVE_PATH) {
        Ok(on_disk) if on_disk == bytes => {
            println!(
                "CONTRACT_V3_PROPOSAL_REPRODUCED={V3_RELATIVE_PATH} sha256={}",
                sha256_hex(&bytes)
            );
            ExitCode::SUCCESS
        }
        Ok(_) => stop(format!("{V3_RELATIVE_PATH} is not the v3 proposal")),
        Err(e) => stop(format!("{V3_RELATIVE_PATH}: {e}")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--check-v3"] {
        return check_v3();
    }
    let (requal_dir, builder_policy, mode) = match args.as_slice() {
        [dir, policy] => (dir, policy, None),
        [dir, policy, mode] => (dir, policy, Some(mode.as_str())),
        _ => {
            eprintln!(
                "usage: g07_acceptance_contract <requal_dir> <builder_policy> [--check | --approved-sha | --write-approved] | --check-v3"
            );
            return ExitCode::from(2);
        }
    };
    let evidence = match attempt1_v4_evidence(Path::new("."), requal_dir, builder_policy) {
        Ok(evidence) => evidence,
        Err(e) => return stop(e),
    };
    let contract = |authorization| attempt1_v4(&evidence, authorization);
    match mode {
        None => match contract(Authorization::Proposed) {
            Ok(bytes) => {
                let written = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(V4_RELATIVE_PATH)
                    .and_then(|mut f| f.write_all(&bytes).and_then(|()| f.sync_all()));
                match written {
                    Ok(()) => {
                        println!(
                            "WROTE_PROPOSAL={V4_RELATIVE_PATH} sha256={}",
                            sha256_hex(&bytes)
                        );
                        ExitCode::SUCCESS
                    }
                    Err(e) => stop(format!("create-new {V4_RELATIVE_PATH}: {e}")),
                }
            }
            Err(e) => stop(e),
        },
        Some("--check") => match (
            contract(Authorization::Proposed),
            fs::read(V4_RELATIVE_PATH),
        ) {
            (Ok(bytes), Ok(on_disk)) if on_disk == bytes => {
                println!(
                    "CONTRACT_V4_PROPOSAL_REPRODUCED={V4_RELATIVE_PATH} sha256={}",
                    sha256_hex(&bytes)
                );
                ExitCode::SUCCESS
            }
            (Ok(_), Ok(_)) => stop(format!("{V4_RELATIVE_PATH} is not the proposal")),
            (Err(e), _) => stop(e),
            (_, Err(e)) => stop(format!("{V4_RELATIVE_PATH}: {e}")),
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
            let staged = format!("{V4_RELATIVE_PATH}.approved-staging");
            let written = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staged)
                .and_then(|mut f| f.write_all(&bytes).and_then(|()| f.sync_all()))
                .and_then(|()| fs::rename(&staged, V4_RELATIVE_PATH));
            match written {
                Ok(()) => {
                    println!("WROTE_APPROVED={V4_RELATIVE_PATH} sha256={sha}");
                    println!("GUARD=UNARMED (arming needs a separate approved commit)");
                    ExitCode::SUCCESS
                }
                Err(e) => stop(format!("write {V4_RELATIVE_PATH}: {e}")),
            }
        }
        Some(other) => {
            eprintln!("unknown mode {other:?}");
            ExitCode::from(2)
        }
    }
}

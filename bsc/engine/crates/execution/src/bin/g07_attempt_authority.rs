//! The shared one-attempt authority of G0.7 (BSC-005), from the main
//! checkout's root:
//!
//! ```text
//! g07_attempt_authority status
//! g07_attempt_authority grant --attempt <attempt_id>
//! ```
//!
//! Both derive the binding from the v5 contract (`V5_RELATIVE_PATH`) and the
//! DRY record it binds: attempt ID, the contract's SHA-256 and the two signed
//! transaction hashes. `status` reads the remote ref only. `grant` is the
//! user's explicit authorization of exactly one attempt: only for an approved,
//! execution-ready contract, only with that contract's attempt ID typed out,
//! only when no authority of that attempt exists (create-only on `origin`).
//! Nothing here signs, sends or arms anything.

use darkbot_bsc_execution::acceptance_guard::{V5_RELATIVE_PATH, parse_v5};
use darkbot_bsc_execution::attempt_authority::{self, Binding, Git, State};
use darkbot_bsc_record::schema::{g06a::dry, g07::contract};
use darkbot_bsc_record::{decode, sha256};
use std::process::ExitCode;

/// The binding of the v5 contract on disk, and whether it is approved.
fn binding() -> Result<(Binding, bool), String> {
    let bytes = std::fs::read(V5_RELATIVE_PATH).map_err(|e| format!("{V5_RELATIVE_PATH}: {e}"))?;
    let snapshot = parse_v5(&bytes)?;
    let record = decode(&contract::V5, &bytes).map_err(|e| e.to_string())?;
    let dry_path = record
        .text(contract::DRY_RECORD_PATH)
        .map_err(|e| e.to_string())?;
    let dry_bytes = std::fs::read(dry_path).map_err(|e| format!("{dry_path}: {e}"))?;
    if hex(&sha256(&dry_bytes)) != snapshot.dry_evidence.record_sha256 {
        return Err("the DRY on disk is not the one the v5 contract binds".into());
    }
    let dry = decode(&dry::NATIVE, &dry_bytes).map_err(|e| format!("{dry_path}: {e}"))?;
    let word = |tag| dry.b256(tag).map_err(|e| e.to_string());
    Ok((
        Binding {
            attempt_id: snapshot.attempt_id.clone(),
            v5_sha256: sha256(&bytes),
            launch_tx_hash: word(dry::LAUNCH_TX_HASH)?,
            buy_tx_hash: word(dry::BUY_TX_HASH)?,
        },
        snapshot.approved && snapshot.execution_ready,
    ))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn run(args: &[String]) -> Result<(), String> {
    if !std::path::Path::new("bsc/engine/Cargo.toml").is_file() {
        return Err("run from the repository root".into());
    }
    let git = Git::origin();
    match args {
        [cmd] if cmd == "status" => {
            let (b, approved) = binding()?;
            let state = match attempt_authority::state(&git, &b)? {
                State::Missing => "MISSING".to_string(),
                State::Authorized => "AUTHORIZED".to_string(),
                State::Other(head) => format!("CONSUMED_OR_FOREIGN {head}"),
            };
            println!(
                "attempt_id={}\nbinding_sha256={}\nv5_approved={approved}\nauthority={state}",
                b.attempt_id,
                b.digest()
            );
            Ok(())
        }
        [cmd, flag, attempt] if cmd == "grant" && flag == "--attempt" => {
            let (b, approved) = binding()?;
            if !approved {
                return Err(
                    "the v5 contract is not the approved, execution-ready variant; no grant".into(),
                );
            }
            if *attempt != b.attempt_id {
                return Err("--attempt is not the v5 contract's attempt ID".into());
            }
            let commit = attempt_authority::grant(&git, &b)?;
            println!(
                "attempt_id={}\nbinding_sha256={}\nauthority=AUTHORIZED {commit}",
                b.attempt_id,
                b.digest()
            );
            Ok(())
        }
        _ => Err("usage: g07_attempt_authority status | grant --attempt <attempt_id>".into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("STOP: {e}");
            ExitCode::from(2)
        }
    }
}

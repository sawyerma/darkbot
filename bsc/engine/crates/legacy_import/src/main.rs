//! `g07_legacy_import`: writes the DBREC001 import of the frozen G0.7 legacy
//! inputs (records, PARITY.md, SHA256SUMS) to a new directory, or with
//! `--check` verifies that the committed import is reproduced byte for byte.
//! Run from the repository root. Offline: reads only local files.

use darkbot_bsc_legacy_import::{OUTPUT_DIR, check, import_all};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let root = Path::new(".");
    if !root.join("bsc/config").is_dir() {
        eprintln!("run from the repository root");
        return ExitCode::from(2);
    }
    let check_only = match std::env::args().nth(1).as_deref() {
        None => false,
        Some("--check") => true,
        Some(other) => {
            eprintln!("unknown argument {other:?}; usage: g07_legacy_import [--check]");
            return ExitCode::from(2);
        }
    };
    if check_only {
        return match check(root) {
            Ok(()) => {
                println!("IMPORT_REPRODUCED={OUTPUT_DIR}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("STOP: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let files = match import_all(root) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("STOP: {e}");
            return ExitCode::FAILURE;
        }
    };
    let dir = root.join(OUTPUT_DIR);
    if let Err(e) = fs::create_dir(&dir) {
        eprintln!("STOP: create {OUTPUT_DIR} (it must not exist yet): {e}");
        return ExitCode::FAILURE;
    }
    for (name, bytes) in files {
        let written = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(&name))
            .and_then(|mut f| f.write_all(&bytes).and_then(|()| f.sync_all()));
        if let Err(e) = written {
            eprintln!("STOP: write {OUTPUT_DIR}/{name}: {e}");
            return ExitCode::FAILURE;
        }
        println!("WROTE={OUTPUT_DIR}/{name}");
    }
    ExitCode::SUCCESS
}

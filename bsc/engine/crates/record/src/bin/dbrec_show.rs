//! Prints the derived human-readable view of DBREC001 records. The view is
//! for inspection only; the record file stays the source of truth.

use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let paths: Vec<String> = env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: dbrec_show <record>...");
        return ExitCode::from(2);
    }
    let mut failed = false;
    for path in paths {
        match fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| darkbot_bsc_record::schema::decode_any(&bytes).map_err(|e| e.0))
        {
            Ok(record) => print!("## {path}\n{}", record.render()),
            Err(e) => {
                eprintln!("{path}: {e}");
                failed = true;
            }
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

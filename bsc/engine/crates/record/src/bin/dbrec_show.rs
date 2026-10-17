//! Prints the derived human-readable view of DBREC001 records, or with
//! `--get <field> <record>` one field's plain value (one line per value of
//! a repeated field), so scripts read typed fields and never parse a view.
//! The record file stays the source of truth.

use darkbot_bsc_record::{plain, schema::decode_any};
use std::{env, fs, process::ExitCode};

fn get(field: &str, path: &str) -> ExitCode {
    let record = match fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|bytes| decode_any(&bytes).map_err(|e| e.0))
    {
        Ok(record) => record,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some(spec) = record.schema().fields.iter().find(|f| f.name == field) else {
        eprintln!("{path}: {} has no field {field}", record.schema().name);
        return ExitCode::FAILURE;
    };
    let values: Vec<String> = record.all(spec.tag).map(plain).collect();
    if values.is_empty() {
        eprintln!("{path}: {field} is absent");
        return ExitCode::from(3);
    }
    for value in values {
        println!("{value}");
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--get") {
        return match args.as_slice() {
            [_, field, path] => get(field, path),
            _ => {
                eprintln!("usage: dbrec_show --get <field> <record>");
                ExitCode::from(2)
            }
        };
    }
    if args.is_empty() {
        eprintln!("usage: dbrec_show <record>... | dbrec_show --get <field> <record>");
        return ExitCode::from(2);
    }
    let mut failed = false;
    for path in args {
        match fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| decode_any(&bytes).map_err(|e| e.0))
        {
            Ok(record) => print!("## {path}\n{}", record.render()),
            Err(e) => {
                eprintln!("{path}: {e}");
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

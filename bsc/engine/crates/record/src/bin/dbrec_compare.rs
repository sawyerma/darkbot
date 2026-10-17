//! Compares two DBREC001 records field by field (by tag; the import and
//! native variants of a DRY schema share their tags) and prints a Markdown
//! report: every field with both values, differences first. Provenance
//! (tags 1–4) is listed but never counted as a difference. Exit 0 with no
//! difference, 1 with differences, 2 on unreadable input.

use darkbot_bsc_record::{Record, plain, schema::decode_any};
use std::collections::BTreeSet;
use std::{env, fs, process::ExitCode};

fn load(path: &str) -> Result<Record, String> {
    let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    decode_any(&bytes).map_err(|e| format!("{path}: {e}"))
}

fn values(record: &Record, tag: u16) -> String {
    let v: Vec<String> = record.all(tag).map(plain).collect();
    if v.is_empty() {
        "(absent)".into()
    } else {
        v.join(" ; ")
    }
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [a_path, b_path] = args.as_slice() else {
        eprintln!("usage: dbrec_compare <record-a> <record-b>");
        return ExitCode::from(2);
    };
    let (a, b) = match (load(a_path), load(b_path)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let tags: BTreeSet<u16> = a
        .schema()
        .fields
        .iter()
        .chain(b.schema().fields)
        .map(|f| f.tag)
        .collect();
    let name = |tag: u16| {
        a.schema()
            .field(tag)
            .or_else(|| b.schema().field(tag))
            .map_or("?", |f| f.name)
    };
    let mut differ = Vec::new();
    let mut equal = Vec::new();
    for tag in tags {
        let (va, vb) = (values(&a, tag), values(&b, tag));
        let row = format!("| `{}` | {} | {} |", name(tag), cell(&va), cell(&vb));
        if tag <= 4 || va == vb {
            equal.push(row);
        } else {
            differ.push(row);
        }
    }
    println!(
        "# Comparison\n\n- A: `{a_path}` ({})\n- B: `{b_path}` ({})\n- Differences outside provenance: {}\n",
        a.schema().name,
        b.schema().name,
        differ.len()
    );
    println!("| Field | A | B |\n|---|---|---|");
    for row in differ.iter().chain(&equal) {
        println!("{row}");
    }
    if differ.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

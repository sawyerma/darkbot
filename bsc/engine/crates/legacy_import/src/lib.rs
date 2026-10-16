//! Offline, audited import of the immutable legacy JSON inputs of G0.7 into
//! DBREC001 records. The roadmap keeps historical hash-bound JSON untouched
//! as legacy evidence; an import it requires is offline, audited,
//! byte/field-verified and converted to the canonical binary format. This
//! crate is the only place in the workspace that reads that JSON.
//!
//! A legacy file is parsed only after its SHA-256 equals its frozen
//! binding. Every JSON leaf must be a field of the import schema (named by
//! its dotted JSON path), so an unmapped leaf stops the import; every field
//! is read back from the encoded record and compared with the legacy value.
//! The output is deterministic: a re-run reproduces the same bytes, which
//! `check` verifies against the committed import.

use darkbot_bsc_record::schema::{IMPORT_LEGACY_PATH, IMPORT_LEGACY_SHA256, IMPORT_PREFIX, g06a, g07};
use darkbot_bsc_record::{
    Draft, Field, Kind, Presence, Schema, Value, decode, hex, sha256, u256_from_decimal,
    u256_from_u128, u256_to_decimal,
};
use serde_json::Value as Json;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// One frozen legacy input: its repository path, the SHA-256 its bytes are
/// bound to, its import schema and the record file name.
pub struct Legacy {
    pub path: &'static str,
    pub sha256: &'static str,
    pub schema: &'static Schema,
    pub output: &'static str,
}

/// The inputs of G0.7 attempt 1, with the hashes its acceptance contract
/// binds (the DRY directory's SHA256SUMS for the DRY files).
pub const LEGACY: [Legacy; 6] = [
    Legacy {
        path: "bsc/native/evidence/g06a/g06a_final_dry_20261015T222241Z/dry_result.json",
        sha256: "0efe78a7faf1d817fae3cbfd8893f858a4386a4c2770755465508c668eeded6f",
        schema: &g06a::dry::SCHEMA,
        output: "dry_result.dbrec",
    },
    Legacy {
        path: "bsc/native/evidence/g06a/g06a_final_dry_20261015T222241Z/preflight.json",
        sha256: "3eb5563d9e0bfea9ce8cc4e3750e3152d81a1b7ea7eff91de78091087bf77e69",
        schema: &g06a::preflight::SCHEMA,
        output: "preflight.dbrec",
    },
    Legacy {
        path: "bsc/native/evidence/g06a/g06a_final_dry_20261015T222241Z/local_simulation.json",
        sha256: "9650dcc0e314db395948e4d9863f38b0a1f2b1001805d973c4e72e2877a64521",
        schema: &g06a::local_simulation::SCHEMA,
        output: "local_simulation.dbrec",
    },
    Legacy {
        path: "bsc/native/evidence/g06a/g06a_final_dry_20261015T222241Z/economics_crosscheck.json",
        sha256: "8cf02a301220ad232c59c4f80ebb9c5e173da0d308000d07a4a59ab8823bc652",
        schema: &g06a::economics::SCHEMA,
        output: "economics_crosscheck.dbrec",
    },
    Legacy {
        path: "bsc/config/g06a_builder_floor_addendum_2026-10-11.json",
        sha256: "ca76a93c432a9cee308b07e59170bac390e801caaa8e0697b0b31d405b9bcc87",
        schema: &g06a::builder_policy::SCHEMA,
        output: "builder_policy.dbrec",
    },
    Legacy {
        path: "bsc/config/g07_producer_witness_source_attempt1.json",
        sha256: "78660c7da9fbb8a2140daf1bca4187881d71f55a914c9fcddfc0e47f287116c4",
        schema: &g07::witness_source::SCHEMA,
        output: "producer_witness_source.dbrec",
    },
];

/// Where the import is written, relative to the repository root.
pub const OUTPUT_DIR: &str = "bsc/native/evidence/g06a/g07_binary_import_20261016";
pub const PARITY_REPORT: &str = "PARITY.md";
pub const CHECKSUMS: &str = "SHA256SUMS";

/// One imported file: the record bytes and its parity section.
pub struct Imported {
    pub legacy: &'static Legacy,
    pub bytes: Vec<u8>,
    pub report: String,
}

fn pointer(field: &Field) -> String {
    format!("/{}", field.name.replace('.', "/"))
}

/// The dotted paths of every leaf: scalars, nulls and arrays of scalars.
fn leaves(json: &Json, path: &str, out: &mut Vec<String>) -> Result<(), String> {
    match json {
        Json::Object(map) => {
            for (key, value) in map {
                if key.contains('.') || key.is_empty() {
                    return Err(format!("key {key:?} under {path:?} cannot be named"));
                }
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                leaves(value, &child, out)?;
            }
        }
        Json::Array(items) => {
            if items.iter().any(|i| i.is_object() || i.is_array()) {
                return Err(format!("{path}: arrays of objects or arrays are not imported"));
            }
            out.push(path.to_string());
        }
        _ => out.push(path.to_string()),
    }
    Ok(())
}

fn hex_bytes(text: &str, len: usize, prefix_required: bool) -> Option<Vec<u8>> {
    let digits = match text.strip_prefix("0x") {
        Some(d) => d,
        None if prefix_required => return None,
        None => text,
    };
    if digits.len() != len * 2 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..len)
        .map(|i| u8::from_str_radix(&digits[i * 2..i * 2 + 2], 16).ok())
        .collect()
}

fn decimal_u64(text: &str) -> Option<u64> {
    let word = u256_from_decimal(text)?;
    if word[..24].iter().any(|b| *b != 0) {
        return None;
    }
    let mut low = [0u8; 8];
    low.copy_from_slice(&word[24..]);
    Some(u64::from_be_bytes(low))
}

/// One legacy scalar as a typed value of `field`'s kind; anything not
/// exactly representable is refused.
fn convert(field: &Field, json: &Json) -> Result<Value, String> {
    let bad = || format!("{}: {json} is not a {:?}", field.name, field.kind);
    Ok(match field.kind {
        Kind::Bool => Value::Bool(json.as_bool().ok_or_else(bad)?),
        Kind::U64 => Value::U64(match json {
            Json::Number(n) => n.as_u64().ok_or_else(bad)?,
            Json::String(s) => decimal_u64(s).ok_or_else(bad)?,
            _ => return Err(bad()),
        }),
        Kind::I64 => Value::I64(json.as_i64().ok_or_else(bad)?),
        Kind::U256 => Value::U256(match json {
            Json::Number(n) => u256_from_u128(u128::from(n.as_u64().ok_or_else(bad)?)),
            Json::String(s) => u256_from_decimal(s).ok_or_else(bad)?,
            _ => return Err(bad()),
        }),
        Kind::Address => {
            let bytes = json
                .as_str()
                .and_then(|s| hex_bytes(s, 20, true))
                .ok_or_else(bad)?;
            let mut a = [0u8; 20];
            a.copy_from_slice(&bytes);
            Value::Address(a)
        }
        Kind::B256 => {
            let bytes = json
                .as_str()
                .and_then(|s| hex_bytes(s, 32, false))
                .ok_or_else(bad)?;
            let mut w = [0u8; 32];
            w.copy_from_slice(&bytes);
            Value::B256(w)
        }
        Kind::Text => Value::Text(json.as_str().ok_or_else(bad)?.to_string()),
        Kind::Bytes | Kind::Nested => return Err(format!("{}: not importable", field.name)),
    })
}

/// The legacy value in the normal form of its kind: hex in lowercase with a
/// `0x` prefix (hex case and the prefix are presentation), numbers as their
/// decimal digits whether the legacy file held a number or a string.
fn legacy_form(kind: Kind, json: &Json) -> String {
    match (kind, json) {
        (Kind::Address | Kind::B256, Json::String(s)) => {
            let lower = s.to_ascii_lowercase();
            if lower.starts_with("0x") {
                lower
            } else {
                format!("0x{lower}")
            }
        }
        (_, Json::String(s)) => s.clone(),
        (_, other) => other.to_string(),
    }
}

fn binary_form(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        Value::U64(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U256(w) => u256_to_decimal(w),
        Value::Address(a) => format!("0x{}", hex(a)),
        Value::B256(w) => format!("0x{}", hex(w)),
        Value::Bytes(b) => format!("0x{}", hex(b)),
        Value::Text(t) => t.clone(),
        Value::Nested(_) => "<nested>".into(),
    }
}

fn cell(text: &str) -> String {
    let short: String = text.chars().take(72).collect();
    let short = if short.len() < text.len() {
        format!("{short}…")
    } else {
        short
    };
    short.replace('|', "\\|").replace('\n', " ")
}

/// Imports one legacy file from the repository at `root`.
pub fn import(root: &Path, legacy: &'static Legacy) -> Result<Imported, String> {
    let raw = fs::read(root.join(legacy.path)).map_err(|e| format!("{}: {e}", legacy.path))?;
    let digest = sha256(&raw);
    if hex(&digest) != legacy.sha256 {
        return Err(format!(
            "{}: SHA-256 {} is not the bound {}",
            legacy.path,
            hex(&digest),
            legacy.sha256
        ));
    }
    let json: Json =
        serde_json::from_slice(&raw).map_err(|e| format!("{}: {e}", legacy.path))?;
    let schema = legacy.schema;
    let mut paths = Vec::new();
    leaves(&json, "", &mut paths)?;
    for path in &paths {
        if !schema.fields.iter().any(|f| f.name == path) {
            return Err(format!("{}: leaf {path} has no field in {}", legacy.path, schema.name));
        }
    }
    let legacy_fields: Vec<&Field> = schema
        .fields
        .iter()
        .filter(|f| !f.name.starts_with(IMPORT_PREFIX))
        .collect();

    let mut draft = Draft::new(schema);
    draft
        .set(IMPORT_LEGACY_SHA256, Value::B256(digest))
        .set(IMPORT_LEGACY_PATH, legacy.path);
    for field in &legacy_fields {
        match json.pointer(&pointer(field)) {
            None | Some(Json::Null) => {
                if field.presence == Presence::Required {
                    return Err(format!("{}: required {} is missing", legacy.path, field.name));
                }
            }
            Some(Json::Array(items)) if field.presence == Presence::Repeated => {
                for item in items {
                    draft.push(field.tag, convert(field, item)?);
                }
            }
            Some(value) if field.presence != Presence::Repeated => {
                draft.set(field.tag, convert(field, value)?);
            }
            Some(other) => return Err(format!("{}: {} is {other}", legacy.path, field.name)),
        }
    }
    let bytes = draft.encode().map_err(|e| e.0)?;

    // Parity: every field read back from the record equals the legacy leaf.
    let record = decode(schema, &bytes).map_err(|e| e.0)?;
    let mut rows = String::new();
    let mut compared = 0usize;
    for field in &legacy_fields {
        let binary: Vec<String> = record.all(field.tag).map(binary_form).collect();
        let original: Vec<String> = match json.pointer(&pointer(field)) {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Array(items)) => items.iter().map(|i| legacy_form(field.kind, i)).collect(),
            Some(value) => vec![legacy_form(field.kind, value)],
        };
        if binary != original {
            return Err(format!(
                "{}: {} differs after the round trip: legacy {original:?}, binary {binary:?}",
                legacy.path, field.name
            ));
        }
        compared += 1;
        let shown = |values: &[String]| {
            if values.is_empty() {
                "(absent: null)".to_string()
            } else {
                values.join(" ; ")
            }
        };
        let _ = writeln!(
            rows,
            "| `{}` | {:?} | {} | {} | equal |",
            field.name,
            field.kind,
            cell(&shown(&original)),
            cell(&shown(&binary))
        );
    }
    let mut report = String::new();
    let _ = writeln!(report, "## {}\n", legacy.output);
    let _ = writeln!(report, "- Legacy file: `{}`", legacy.path);
    let _ = writeln!(report, "- Legacy SHA-256 (bound): `{}`", legacy.sha256);
    let _ = writeln!(
        report,
        "- Record: `{}` (type {}, version {}), {} bytes, SHA-256 `{}`",
        schema.name,
        schema.record_type,
        schema.schema_version,
        bytes.len(),
        hex(&sha256(&bytes))
    );
    let _ = writeln!(
        report,
        "- JSON leaves: {}, all mapped; fields compared after decoding: {compared}, all equal\n",
        paths.len()
    );
    let _ = writeln!(report, "| Field | Kind | Legacy | Binary | Result |");
    let _ = writeln!(report, "|---|---|---|---|---|");
    report.push_str(&rows);
    Ok(Imported {
        legacy,
        bytes,
        report,
    })
}

/// The complete import: every record file, the parity report and the
/// checksum list, by file name.
pub fn import_all(root: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut files = Vec::new();
    let mut report = String::from(
        "# G0.7 legacy import: field parity\n\n\
         Generated by `g07_legacy_import` (crate `darkbot-bsc-legacy-import`); \
         `g07_legacy_import --check` reproduces every byte. The legacy JSON files are \
         unchanged and remain immutable legacy evidence; these DBREC001 records are the \
         binary inputs G0.7 reads. Hex values are compared in lowercase with a `0x` \
         prefix and numbers as decimal digits (presentation only); every other value \
         is compared exactly.\n\n",
    );
    for legacy in &LEGACY {
        let imported = import(root, legacy)?;
        report.push_str(&imported.report);
        report.push('\n');
        files.push((imported.legacy.output.to_string(), imported.bytes));
    }
    files.push((PARITY_REPORT.to_string(), report.into_bytes()));
    let mut sums = String::new();
    for (name, bytes) in &files {
        let _ = writeln!(sums, "{}  {name}", hex(&sha256(bytes)));
    }
    files.push((CHECKSUMS.to_string(), sums.into_bytes()));
    Ok(files)
}

/// Whether the committed import under `root` equals a fresh import, byte
/// for byte.
pub fn check(root: &Path) -> Result<(), String> {
    let dir = root.join(OUTPUT_DIR);
    for (name, bytes) in import_all(root)? {
        let on_disk = fs::read(dir.join(&name)).map_err(|e| format!("{OUTPUT_DIR}/{name}: {e}"))?;
        if on_disk != bytes {
            return Err(format!("{OUTPUT_DIR}/{name} differs from a fresh import"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_refuses_what_it_cannot_represent_exactly() {
        let u64f = Field::req(9, "n", Kind::U64);
        assert_eq!(convert(&u64f, &serde_json::json!("56")).unwrap(), Value::U64(56));
        assert_eq!(convert(&u64f, &serde_json::json!(56)).unwrap(), Value::U64(56));
        for bad in [
            serde_json::json!("056"),
            serde_json::json!("-1"),
            serde_json::json!(1.5),
            serde_json::json!("18446744073709551616"),
        ] {
            assert!(convert(&u64f, &bad).is_err(), "{bad}");
        }
        let addr = Field::req(9, "a", Kind::Address);
        assert!(convert(&addr, &serde_json::json!("e2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0")).is_err());
        assert!(convert(&addr, &serde_json::json!("0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de")).is_err());
        let word = Field::req(9, "w", Kind::B256);
        assert!(convert(&word, &serde_json::json!("0x00")).is_err());
        assert!(convert(&word, &serde_json::json!("1".repeat(64))).is_ok());
    }

    #[test]
    fn an_unmapped_leaf_or_another_hash_stops_the_import() {
        let dir = std::env::temp_dir().join(format!("darkbot-legacy-import-{}", std::process::id()));
        fs::create_dir_all(dir.join("x")).unwrap();
        let body = br#"{"schema":"s","attempt_id":"a","extra":1}"#;
        fs::write(dir.join("x/w.json"), body).unwrap();
        let leaked: &'static str = Box::leak(hex(&sha256(body)).into_boxed_str());
        let legacy: &'static Legacy = Box::leak(Box::new(Legacy {
            path: "x/w.json",
            sha256: leaked,
            schema: &g07::witness_source::SCHEMA,
            output: "w.dbrec",
        }));
        let e = import(&dir, legacy).err().unwrap();
        assert!(e.contains("leaf extra has no field"), "{e}");
        let wrong: &'static Legacy = Box::leak(Box::new(Legacy {
            sha256: "00",
            ..*legacy
        }));
        assert!(import(&dir, wrong).err().unwrap().contains("is not the bound"));
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The committed import equals a fresh import of the unchanged legacy
    /// files: the field-parity proof is reproducible.
    #[test]
    fn the_committed_import_is_reproduced_byte_for_byte() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        check(&root).unwrap();
    }
}

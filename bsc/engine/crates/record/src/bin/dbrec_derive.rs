//! Derives a native record from an existing one (an audited import or an
//! earlier native record) of the same field layout, e.g. a refreshed builder
//! policy: every field is carried by tag and name, the import provenance is
//! replaced by this tool's provenance, and each changed field is stated
//! explicitly with `--set name=value` (a repeated field takes one `--set`
//! per value) or removed with `--clear name`. The result is written to a new
//! file and its derived view printed.
//!
//! usage: dbrec_derive <input> <target-schema-name> <output> [--set name=value | --clear name]...

use darkbot_bsc_record::schema::{
    IMPORT_LEGACY_PATH, IMPORT_LEGACY_SHA256, PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL, all,
    decode_any,
};
use darkbot_bsc_record::{Draft, Field, Kind, Value, decode, hex, sha256, u256_from_decimal};
use std::collections::BTreeSet;
use std::io::Write;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

fn hex_bytes(text: &str, len: Option<usize>) -> Result<Vec<u8>, String> {
    let digits = text.strip_prefix("0x").ok_or("hex value needs 0x")?;
    if digits.len() % 2 != 0 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{text} is not hex"));
    }
    let bytes: Vec<u8> = (0..digits.len() / 2)
        .map(|i| u8::from_str_radix(&digits[i * 2..i * 2 + 2], 16).unwrap_or(0))
        .collect();
    match len {
        Some(n) if bytes.len() != n => Err(format!("{text} is not {n} bytes")),
        _ => Ok(bytes),
    }
}

/// A command-line value as the typed value of `field`.
fn parse_value(field: &Field, text: &str) -> Result<Value, String> {
    let bad = || format!("{}: {text:?} is not a {:?}", field.name, field.kind);
    Ok(match field.kind {
        Kind::Bool => match text {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => return Err(bad()),
        },
        Kind::U64 => Value::U64(text.parse().map_err(|_| bad())?),
        Kind::I64 => Value::I64(text.parse().map_err(|_| bad())?),
        Kind::U256 => Value::U256(u256_from_decimal(text).ok_or_else(bad)?),
        Kind::Address => Value::Address(hex_bytes(text, Some(20))?.try_into().map_err(|_| bad())?),
        Kind::B256 => Value::B256(hex_bytes(text, Some(32))?.try_into().map_err(|_| bad())?),
        Kind::Bytes => Value::Bytes(hex_bytes(text, None)?),
        Kind::Text => Value::Text(text.to_string()),
        Kind::Nested => return Err(format!("{}: nested fields cannot be set", field.name)),
    })
}

fn derive(args: &[String]) -> Result<(String, Vec<u8>), String> {
    let [input, target, output, edits @ ..] = args else {
        return Err("usage: dbrec_derive <input> <target-schema-name> <output> [--set name=value | --clear name]...".into());
    };
    let source = decode_any(&std::fs::read(input).map_err(|e| format!("{input}: {e}"))?)
        .map_err(|e| format!("{input}: {e}"))?;
    let schema = all()
        .into_iter()
        .find(|s| s.name == target)
        .ok_or_else(|| format!("unknown schema {target}"))?;
    if schema.field(PROVENANCE_TOOL).is_none() {
        return Err(format!("{target} is not a native schema with provenance"));
    }
    let mut d = Draft::new(schema);
    for (tag, value) in source.fields() {
        if [
            IMPORT_LEGACY_SHA256,
            IMPORT_LEGACY_PATH,
            PROVENANCE_TOOL,
            PROVENANCE_OBSERVED_UNIX_MS,
        ]
        .contains(tag)
        {
            continue;
        }
        let from = source.schema().field(*tag).map(|f| f.name);
        let to = schema.field(*tag).map(|f| f.name);
        if from != to {
            return Err(format!(
                "tag {tag} is {from:?} in the input but {to:?} in {target}"
            ));
        }
        d.push(*tag, value.clone());
    }
    let observed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_millis() as u64)
        .unwrap_or(0);
    d.set(PROVENANCE_TOOL, format!("dbrec_derive from {input}"))
        .set(PROVENANCE_OBSERVED_UNIX_MS, observed);
    let mut set_once = BTreeSet::new();
    let mut edits = edits.iter();
    while let Some(flag) = edits.next() {
        let operand = edits
            .next()
            .ok_or_else(|| format!("{flag} needs an operand"))?;
        let (name, text) = match flag.as_str() {
            "--set" => operand
                .split_once('=')
                .map(|(n, t)| (n, Some(t)))
                .ok_or_else(|| format!("--set {operand}: expected name=value"))?,
            "--clear" => (operand.as_str(), None),
            other => return Err(format!("unknown option {other}")),
        };
        let field = schema
            .fields
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| format!("{target} has no field {name}"))?;
        match text {
            None => {
                d.clear(field.tag);
            }
            Some(text) => {
                let value = parse_value(field, text)?;
                if field.presence == darkbot_bsc_record::Presence::Repeated {
                    if set_once.insert(field.tag) {
                        d.clear(field.tag);
                    }
                    d.push(field.tag, value);
                } else {
                    d.set(field.tag, value);
                }
            }
        }
    }
    let bytes = d.encode().map_err(|e| e.0)?;
    let view = decode(schema, &bytes).map_err(|e| e.0)?.render();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| format!("create-new {output}: {e}"))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("write {output}: {e}"))?;
    Ok((
        format!("{view}DERIVED={output} sha256={}\n", hex(&sha256(&bytes))),
        bytes,
    ))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match derive(&args) {
        Ok((report, _)) => {
            print!("{report}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("STOP: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_bsc_record::schema::g06a::builder_policy as b;

    #[test]
    fn values_are_typed_by_the_target_field() {
        let field = |name: &str| *b::NATIVE.fields.iter().find(|f| f.name == name).unwrap();
        assert_eq!(
            parse_value(&field("observed_date"), "2026-10-18").unwrap(),
            Value::Text("2026-10-18".into())
        );
        assert!(parse_value(&field("dry3_ready"), "yes").is_err());
        assert!(parse_value(&field("strictest_explicit_verified_floor_wei"), "05").is_err());
        assert!(parse_value(&field("strictest_explicit_verified_floor_wei"), "50000000").is_ok());
    }

    #[test]
    fn an_import_derives_to_its_native_variant_with_explicit_changes() {
        let dir = std::env::temp_dir().join(format!("darkbot-derive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let input = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../native/evidence/g06a/g07_binary_import_20261016/builder_policy.dbrec"
        );
        let out = dir.join("policy.dbrec");
        let args: Vec<String> = [
            input,
            "darkbot.g06a.builder-floor-policy.v2",
            out.to_str().unwrap(),
            "--set",
            "observed_date=2026-10-18",
            "--set",
            "non_claims=only one",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let (_, bytes) = derive(&args).unwrap();
        let r = decode(&b::NATIVE, &bytes).unwrap();
        assert_eq!(r.text(b::OBSERVED_DATE).unwrap(), "2026-10-18");
        assert_eq!(r.texts(45).unwrap(), ["only one"]);
        assert_eq!(r.bool(b::DRY3_READY).unwrap(), true);
        assert!(derive(&args).unwrap_err().contains("create-new"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

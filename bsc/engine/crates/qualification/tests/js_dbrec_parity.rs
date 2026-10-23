//! The JavaScript DBREC001 codec of the DRY tools (`bsc/tools/dbrec.cjs`)
//! against the Rust codec, the authority: on the values of the frozen DRY
//! records, JavaScript must produce byte-identical native records, decode
//! the binary preflight to the same values and refuse a tampered record;
//! both EthereumJS tools must set exactly the fields of their schemas.
//! Offline (needs `node` on PATH); no RPC, no key.

use darkbot_bsc_record::schema::{
    PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL,
    g06a::{economics, launch_path, local_simulation, preflight},
};
use darkbot_bsc_record::{Draft, Kind, Schema, decode, plain};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

const FROZEN: &str = "../../../native/evidence/g06a/g07_binary_import_20261016";
const TOOLS: &str = "../../../tools";

fn path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("darkbot-js-dbrec-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const DRIVER: &str = r#"
'use strict';
const fs = require('fs');
const dbrec = require(process.argv[2]);
const [mode, a, b, c] = process.argv.slice(3);
if (mode === 'encode') {
  // a: schema name, b: field lines (name TAB kind TAB value), c: output
  const values = {};
  for (const line of fs.readFileSync(b, 'utf8').split('\n').filter(Boolean)) {
    const [name, kind, repeated, text] = line.split('\t');
    const v = kind === 'Bool' ? text === 'true'
      : (kind === 'U64' || kind === 'U256' || kind === 'I64') ? BigInt(text) : text;
    if (repeated === 'R') (values[name] ||= []).push(v); else values[name] = v;
  }
  fs.writeFileSync(c, dbrec.encode(a, values));
} else if (mode === 'decode') {
  const r = dbrec.decode(a, fs.readFileSync(b));
  for (const [k, v] of Object.entries(r.fields)) console.log(`${k}\t${Array.isArray(v) ? v.join(' ; ') : v}`);
  console.log(`sha256\t${r.sha256}`);
} else if (mode === 'refuse') {
  try { dbrec.decode(a, fs.readFileSync(b)); console.log('ACCEPTED'); }
  catch (e) { console.log(`REFUSED ${e.message}`); }
}
"#;

fn node(dir: &Path, args: &[&str]) -> String {
    let driver = dir.join("driver.cjs");
    std::fs::write(&driver, DRIVER).unwrap();
    let codec = path(&format!("{TOOLS}/dbrec.cjs"));
    let out = Command::new("node")
        .arg(&driver)
        .arg(codec)
        .args(args)
        .output()
        .expect("node is required for this test");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// The native record Rust encodes from a frozen import record (same tags,
/// fixed provenance), and the same values as driver input lines.
fn native_from_frozen(
    import: &str,
    from: &'static Schema,
    to: &'static Schema,
) -> (Vec<u8>, String) {
    let record = decode(
        from,
        &std::fs::read(path(&format!("{FROZEN}/{import}"))).unwrap(),
    )
    .unwrap();
    let mut d = Draft::new(to);
    let mut lines = format!(
        "_provenance.tool\tText\t-\tparity-test\n_provenance.observed_unix_ms\tU64\t-\t1792231844873\n"
    );
    d.set(PROVENANCE_TOOL, "parity-test")
        .set(PROVENANCE_OBSERVED_UNIX_MS, 1_792_231_844_873u64);
    for (tag, value) in record.fields() {
        if *tag <= 2 {
            continue;
        }
        let field = to.field(*tag).unwrap();
        d.push(*tag, value.clone());
        let kind = format!("{:?}", field.kind);
        let repeated = if field.presence == darkbot_bsc_record::Presence::Repeated {
            "R"
        } else {
            "-"
        };
        lines.push_str(&format!(
            "{}\t{kind}\t{repeated}\t{}\n",
            field.name,
            plain(value)
        ));
    }
    (d.encode().unwrap(), lines)
}

#[test]
fn javascript_encodes_byte_identical_native_records() {
    let dir = scratch("encode");
    for (import, from, to) in [
        (
            "economics_crosscheck.dbrec",
            &economics::SCHEMA,
            &economics::NATIVE,
        ),
        (
            "local_simulation.dbrec",
            &local_simulation::SCHEMA,
            &local_simulation::NATIVE,
        ),
    ] {
        let (rust, lines) = native_from_frozen(import, from, to);
        let input = dir.join(format!("{import}.lines"));
        let output = dir.join(format!("{import}.js"));
        std::fs::write(&input, lines).unwrap();
        node(
            &dir,
            &[
                "encode",
                to.name,
                input.to_str().unwrap(),
                output.to_str().unwrap(),
            ],
        );
        let js = std::fs::read(&output).unwrap();
        assert_eq!(js, rust, "{}", to.name);
        assert!(decode(to, &js).is_ok());
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn javascript_decodes_the_preflight_and_refuses_tampered_records() {
    let dir = scratch("decode");
    let file = path(&format!("{FROZEN}/preflight.dbrec"));
    let bytes = std::fs::read(&file).unwrap();
    let rust = decode(&preflight::SCHEMA, &bytes).unwrap();
    let js = node(
        &dir,
        &["decode", preflight::SCHEMA.name, file.to_str().unwrap()],
    );
    for (tag, value) in rust.fields() {
        let name = preflight::SCHEMA.field(*tag).unwrap().name;
        assert!(
            js.contains(&format!("{name}\t{}\n", plain(value))),
            "{name}"
        );
    }
    assert!(js.contains(&format!(
        "sha256\t{}",
        darkbot_bsc_record::hex(&darkbot_bsc_record::sha256(&bytes))
    )));
    let mut tampered = bytes.clone();
    let middle = tampered.len() / 2;
    tampered[middle] ^= 1;
    let bad = dir.join("tampered.dbrec");
    std::fs::write(&bad, &tampered).unwrap();
    let out = node(
        &dir,
        &["refuse", preflight::SCHEMA.name, bad.to_str().unwrap()],
    );
    assert!(
        out.starts_with("REFUSED") && out.contains("checksum"),
        "{out}"
    );
    // Another schema's record is refused as well.
    let other = path(&format!("{FROZEN}/dry_result.dbrec"));
    let out = node(
        &dir,
        &["refuse", preflight::SCHEMA.name, other.to_str().unwrap()],
    );
    assert!(out.contains("wrong record type"), "{out}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The field names of a tool's `dbrec.encode(...)` object literal.
fn encoded_names(source: &str) -> BTreeSet<String> {
    let start = source.find("dbrec.encode(").expect("encode call");
    let body = &source[start..start + source[start..].find("\n  });").expect("end of encode call")];
    // Top-level keys only: the literal's members sit at four spaces.
    body.lines()
        .skip(1)
        .filter(|line| line.starts_with("    ") && !line.starts_with("     "))
        .filter_map(|line| {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix('\'') {
                return rest.split('\'').next().map(str::to_string);
            }
            let name: String = t
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()
                && (t[name.len()..].starts_with(':') || t[name.len()..].starts_with(',')))
            .then_some(name)
        })
        .collect()
}

#[test]
fn both_ethereumjs_tools_set_exactly_their_schema_fields() {
    for (tool, schema) in [
        ("g06a_economics_evm_crosscheck.cjs", &economics::NATIVE),
        ("g06a_local_simulate.cjs", &local_simulation::NATIVE),
        ("g06a_launch_path_trace.cjs", &launch_path::SCHEMA),
    ] {
        let source = std::fs::read_to_string(path(&format!("{TOOLS}/{tool}"))).unwrap();
        let names = encoded_names(&source);
        let expected: BTreeSet<String> = schema
            .fields
            .iter()
            .filter(|f| f.kind != Kind::Nested)
            .map(|f| f.name.to_string())
            .collect();
        assert_eq!(names, expected, "{tool}");
        // No JSON file is written; JSON stays on the RPC wire.
        assert!(!source.contains("writeFileSync"), "{tool}");
        let json_uses = source.matches("JSON.").count();
        assert_eq!(json_uses, 2, "{tool}: JSON only in the rpc() wire function");
    }
}

#[test]
fn the_secret_carrying_tools_keep_the_salt_calldata_and_created_addresses_local() {
    // v9.1 B2: offline node regression (fake provider) of the private mode,
    // including the fail-before control that shows the unprotected leak.
    let out = Command::new("node")
        .arg("--test")
        .arg(path(&format!("{TOOLS}/test_g06a_private_mode.cjs")))
        .output()
        .expect("node is required for this test");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    for tool in ["g06a_local_simulate.cjs", "g06a_launch_path_trace.cjs"] {
        let source = std::fs::read_to_string(path(&format!("{TOOLS}/{tool}"))).unwrap();
        assert!(source.contains("guardEgress([salt, "), "{tool}");
        assert!(source.contains("localCreations(evm, state, [token])"), "{tool}");
        for remote in ["'eth_call'", "'eth_estimateGas'"] {
            assert!(!source.contains(remote), "{tool}: {remote}");
        }
    }
}

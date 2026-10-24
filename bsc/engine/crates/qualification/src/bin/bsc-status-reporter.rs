//! Read-only local evidence publisher. Never reads secret environment files or sends chain RPC.
use darkbot_bsc_qualification::status::{projection, publisher};
use prost::Message;
use std::path::{Path, PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root =
        PathBuf::from(std::env::var_os("DARKBOT_BSC_ROOT").ok_or("DARKBOT_BSC_ROOT missing")?);
    let run = PathBuf::from(std::env::var_os("DARKBOT_RUNDIR").ok_or("DARKBOT_RUNDIR missing")?);
    if std::env::args().nth(1).as_deref() == Some("--field-inventory") {
        println!(
            "{}",
            serde_json::to_string_pretty(&field_inventory(&root)?)?
        );
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--validate-readiness") {
        let p = std::env::args().nth(2).ok_or("missing path")?;
        let bytes = std::fs::read(p)?;
        use darkbot_bsc_record::schema::g07::readiness;
        let decoded = darkbot_bsc_record::schema::decode_any(&bytes)?;
        if decoded.schema() != &readiness::SCHEMA {
            return Err("not a readiness record".into());
        }
        // The typed native verdict and mode, for the job wrapper's result.
        let status = decoded.text(readiness::STATUS)?;
        let mode = decoded.text(readiness::MODE)?;
        if !["PASS_READ_ONLY", "BLOCKED", "STOP"].contains(&status)
            || !["READ_ONLY_RPC", "OFFLINE_ZERO_EGRESS"].contains(&mode)
        {
            return Err("unknown readiness status or mode".into());
        }
        println!("status={status}\nmode={mode}");
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--snapshot") {
        use std::io::Write;
        std::io::stdout()
            .lock()
            .write_all(&projection::project(&root, &run).status.encode_to_vec())?;
        return Ok(());
    }
    let socket = PathBuf::from(
        std::env::var_os("DARKBOT_BSC_SOCKET").unwrap_or_else(|| "/tmp/darkbot/bsc.sock".into()),
    );
    let label = std::env::var("DARKBOT_BSC_READINESS_LABEL")
        .unwrap_or_else(|_| "com.darkbot.bsc.readiness".into());
    let readiness_bin = std::env::var_os("DARKBOT_BSC_READINESS_BIN").map(PathBuf::from);
    publisher::serve(&root, &run, &socket, &label, readiness_bin.as_deref())?;
    Ok(())
}

/// F0-VAR census (`frontend/contracts/bsc-native-fields.json`): every leaf field of every
/// registered record schema, nested fields as `parent[].name`. The sensitivity class is the
/// native verdict on what may ever leave the reporter; which public fields reach the browser,
/// and where, is decided per field in `frontend/contracts/bsc-fields.json`.
fn field_inventory(root: &Path) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    use darkbot_bsc_record::{Field, Kind, Presence, Schema, hex, schema, sha256};
    fn walk<'a>(
        s: &'a Schema,
        prefix: &str,
        chain: &str,
        out: &mut Vec<(String, String, &'a Field)>,
    ) {
        for f in s.fields {
            let presence = match f.presence {
                Presence::Required => "Required",
                Presence::Optional => "Optional",
                Presence::Repeated => "Repeated",
            };
            let chain = if chain.is_empty() {
                presence.to_string()
            } else {
                format!("{chain}>{presence}")
            };
            let name = format!("{prefix}{}", f.name);
            match (f.kind, f.nested) {
                (Kind::Nested, Some(n)) => walk(n, &format!("{name}[]."), &chain, out),
                _ => out.push((name, chain, f)),
            }
        }
    }
    fn unit(name: &str, kind: Kind) -> &'static str {
        match kind {
            Kind::Bool => return "boolean",
            Kind::Address => return "EVM_address",
            Kind::B256 => return "hash_32_bytes",
            Kind::Bytes => return "bytes",
            Kind::Text => return "text",
            _ => {}
        }
        let name = name.to_ascii_lowercase();
        let l = name.rsplit('.').next().unwrap_or(&name);
        if l.ends_with("_bps") {
            "basis_points"
        } else if name.contains("wei") || l == "value" {
            "wei"
        } else if l.contains("unix_ms") {
            "unix_milliseconds"
        } else if l.ends_with("unix_s") {
            "unix_seconds"
        } else if l.ends_with("_ns") || l.contains("_ns_") {
            "nanoseconds"
        } else if l.ends_with("_ms") {
            "milliseconds"
        } else if l.ends_with("_seconds") || l.ends_with("_s") {
            "seconds"
        } else if l.contains("timestamp") {
            "schema_native_timestamp"
        } else if l == "chain_id" {
            "chain_id"
        } else if l.contains("nonce") {
            "nonce"
        } else if l.contains("count")
            || l.contains("confirmations")
            || l.ends_with("_blocks")
            || matches!(
                l,
                "records"
                    | "headers_verified"
                    | "enqueued"
                    | "dropped_full"
                    | "rejected_oversize"
                    | "disconnected"
            )
        {
            "count"
        } else if l.ends_with("_bytes") || l.ends_with("_len") || l == "bytes" {
            "byte_count"
        } else if l.contains("price") || l.contains("fee") || l.contains("cost") {
            "wei"
        } else if l.contains("balance") || kind == Kind::U256 {
            "atomic_integer"
        } else if l.contains("gas") {
            "gas_units"
        } else if l.ends_with("block")
            || l.ends_with("block_number")
            || l.ends_with("head")
            || l.starts_with("scanned_")
        {
            "block_number"
        } else if kind == Kind::I64 {
            "signed_integer"
        } else {
            "integer"
        }
    }
    fn sensitivity(s: &Schema, name: &str, kind: Kind) -> &'static str {
        let leaf = name.rsplit('.').next().unwrap_or(name);
        if s == &schema::g06a::economics::PRIVATE {
            "PRIVATE_RECORD_FIELD_AUDIT_REQUIRED"
        } else if leaf == "salt" {
            "SECRET_NEVER_BROWSER"
        } else if kind == Kind::Bytes {
            "BINARY_EXCLUDED"
        } else if kind != Kind::Text {
            "PUBLIC"
        } else if name.contains("path") || leaf.ends_with("_dir") || name == "r0.record" {
            "TEXT_REPO_RELATIVE_PATH_ONLY"
        } else if matches!(
            leaf,
            "url"
                | "source"
                | "archive_source"
                | "chain_source"
                | "official_sources"
                | "read"
                | "error"
                | "chain_journal_error"
                | "rpc_error_message"
                | "rpc_result_text"
                | "http_status_line"
        ) {
            "TEXT_EXTERNAL_REDACTED"
        } else {
            "TEXT_PUBLIC_REDACTED"
        }
    }
    let files: [(&str, &[&Schema]); 4] = [
        ("g06a", schema::g06a::ALL),
        ("g07", schema::g07::ALL),
        ("g07_candidate", schema::g07_candidate::ALL),
        ("g08", schema::g08::ALL),
    ];
    let mut entries = Vec::new();
    for (file, schemas) in files {
        let path = format!("bsc/engine/crates/record/src/schema/{file}.rs");
        let hash = hex(&sha256(&std::fs::read(root.join(&path))?));
        for s in schemas {
            let mut leaves = Vec::new();
            walk(s, "", "", &mut leaves);
            for (name, presence, f) in leaves {
                entries.push(serde_json::json!({
                    "schema": s.name,
                    "record_type": s.record_type,
                    "schema_version": s.schema_version,
                    "symbol": name,
                    "type": format!("{:?}", f.kind),
                    "presence": presence,
                    "unit": unit(&name, f.kind),
                    "owner": "BSC",
                    "source_path": path,
                    "source_sha": hash,
                    "required_gate": "READ_ONLY_OBSERVATION",
                    "sensitivity": sensitivity(s, &name, f.kind),
                }));
            }
        }
    }
    Ok(entries)
}

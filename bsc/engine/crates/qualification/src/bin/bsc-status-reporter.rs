//! Read-only local evidence publisher. Never reads secret environment files or sends chain RPC.
use darkbot_bsc_qualification::status::{projection, publisher};
use prost::Message;
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root =
        PathBuf::from(std::env::var_os("DARKBOT_BSC_ROOT").ok_or("DARKBOT_BSC_ROOT missing")?);
    let run = PathBuf::from(std::env::var_os("DARKBOT_RUNDIR").ok_or("DARKBOT_RUNDIR missing")?);
    if std::env::args().nth(1).as_deref() == Some("--field-inventory") {
        use darkbot_bsc_record::{Kind, hex, schema, sha256};
        let projection_source = include_str!("../status/projection.rs");
        let schemas = [
            &schema::g06a::dry::SCHEMA,
            &schema::g06a::dry::NATIVE,
            &schema::g06a::preflight::SCHEMA,
            &schema::g06a::builder_policy::SCHEMA,
            &schema::g06a::builder_policy::NATIVE,
            &schema::g07::witness_source::SCHEMA,
            &schema::g07::contract::V4,
            &schema::g07::readiness::SCHEMA,
            &schema::g07::readiness::check::SCHEMA,
            &schema::g07::evidence::SCHEMA,
            &schema::g08::result::SCHEMA,
            &schema::g08::unresolved::SCHEMA,
            &schema::g07_candidate::candidate::SCHEMA,
            &schema::g07_candidate::selection::SCHEMA,
            &schema::g07_candidate::request::SCHEMA,
            &schema::g07_candidate::request_result::SCHEMA,
            &schema::g07_candidate::qualification::SCHEMA,
        ];
        let mut fields = Vec::new();
        for schema in schemas {
            let group = if schema.name.contains("g08") {
                "g08"
            } else if schema.name.contains("self-launch-candidate") {
                "g07_candidate"
            } else if schema.name.contains("g07") || schema.name.contains("witness-source") {
                "g07"
            } else {
                "g06a"
            };
            let path = format!("bsc/engine/crates/record/src/schema/{group}.rs");
            let hash = hex(&sha256(&std::fs::read(root.join(&path))?));
            for field in schema.fields {
                if matches!(field.kind, Kind::Bytes | Kind::Nested) {
                    continue;
                }
                if schema != &schema::g07::contract::V4
                    && schema != &schema::g08::result::SCHEMA
                    && schema != &schema::g08::unresolved::SCHEMA
                    && !projection_source.contains(&format!("\"{}\"", field.name))
                {
                    continue;
                }
                let unit = if field.name.contains("_wei") {
                    "wei"
                } else if field.name.contains("unix_ms") {
                    "unix_milliseconds"
                } else if field.name.contains("timestamp") {
                    "schema_native_timestamp"
                } else if field.kind == Kind::Address {
                    "EVM_address"
                } else if field.kind == Kind::B256 {
                    "hash_32_bytes"
                } else if field.kind == Kind::Bool {
                    "boolean"
                } else if field.kind == Kind::U256 {
                    "atomic_integer"
                } else if field.kind == Kind::U64 {
                    "integer_native_units"
                } else {
                    "text"
                };
                fields.push(serde_json::json!({"schema":schema.name,"symbol":field.name,"type":format!("{:?}",field.kind),"unit":unit,"owner":"BSC","source_path":path,"source_sha":hash,"required_gate":"READ_ONLY_OBSERVATION","sensitivity":"public; text redacted; binary transactions excluded"}));
            }
        }
        println!("{}", serde_json::to_string_pretty(&fields)?);
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--validate-readiness") {
        let p = std::env::args().nth(2).ok_or("missing path")?;
        let bytes = std::fs::read(p)?;
        let decoded = darkbot_bsc_record::schema::decode_any(&bytes)?;
        if decoded.schema() != &darkbot_bsc_record::schema::g07::readiness::SCHEMA {
            return Err("not a readiness record".into());
        }
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
    publisher::serve(&root, &run, &socket, &label)?;
    Ok(())
}

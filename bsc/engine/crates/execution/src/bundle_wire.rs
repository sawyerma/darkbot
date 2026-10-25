//! The one native definition of the G0.7 bundle semantics and builder wire
//! bodies (BSC-003, BSC_Bug_V2 P2-D1/P3-F2). The live runner
//! (builder_submission, chain_reads), the DRY projection
//! (g06a_self_launch_bundle) and the requalification use it; v5 creation,
//! the requalification and the runner right before its latch check a DRY's
//! bound projection against it fail-closed. A changed value changes the DRY
//! projection and the live bodies together and needs a new requalification
//! and v5 approval; K1/K2 (noMerge/positionFirst, +2) stay
//! PROVIDER_UNQUALIFIED.
//!
//! The 48Club `maxTimestamp` is set from the send head at the release; the
//! DRY projects it as 0, so a projection digest describes the template, not
//! the live body.

use darkbot_bsc_record::{Record, RecordError};

/// maxBlockNumber = send head + 2: the bundle is valid for send head + 1 ..= + 2.
pub const MAX_BLOCK_OFFSET: u64 = 2;
/// 48Club `noMerge` (documented only qualitatively; effect UNQUALIFIED).
pub const FORTYEIGHTCLUB_NO_MERGE: bool = true;
/// 48Club `positionFirst` ("strictly order by priorityFee", not top-of-block).
pub const FORTYEIGHTCLUB_POSITION_FIRST: bool = true;
/// The longest wait for a builder's complete response after the flush
/// (BSC-010): one absolute deadline, bound into the v5 contract text.
pub const BUILDER_RESPONSE_DEADLINE_MS: u64 = 15_000;

/// A JSON wire string: quoted, with `"`, `\` and control characters
/// escaped. The G0.7 fields are 0x-hex, which passes through unchanged.
pub fn wire_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The 48Club `eth_sendBundle` body of a G0.7 bundle: integer expiry block
/// and Unix-second expiry time, `noMerge` and `positionFirst`. A fixed
/// template (keys in the documented order) with the fields patched in.
pub fn club48_body(request_id: &str, txs: [&str; 2], max_block: u64, max_timestamp: u64) -> String {
    format!(
        r#"{{"id":{},"jsonrpc":"2.0","method":"eth_sendBundle","params":[{{"maxBlockNumber":{max_block},"maxTimestamp":{max_timestamp},"noMerge":{FORTYEIGHTCLUB_NO_MERGE},"positionFirst":{FORTYEIGHTCLUB_POSITION_FIRST},"txs":[{},{}]}}]}}"#,
        wire_string(request_id),
        wire_string(txs[0]),
        wire_string(txs[1])
    )
}

/// The BlockRazor `eth_sendBundle` body of a G0.7 bundle: integer expiry
/// block only.
pub fn blockrazor_body(request_id: &str, txs: [&str; 2], max_block: u64) -> String {
    format!(
        r#"{{"id":{},"jsonrpc":"2.0","method":"eth_sendBundle","params":[{{"maxBlockNumber":{max_block},"txs":[{},{}]}}]}}"#,
        wire_string(request_id),
        wire_string(txs[0]),
        wire_string(txs[1])
    )
}

/// The bound DRY's bundle projection is these semantics: the 48Club flags
/// and the expiry offset `max_block - send_head` (never its absolute blocks
/// against a later head). Works on the native and the imported DRY schema.
pub fn check_projection(dry: &Record) -> Result<(), String> {
    let schema = dry.schema();
    let tag = |name: &str| {
        schema
            .fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.tag)
            .ok_or_else(|| format!("{} has no {name}", schema.name))
    };
    let e = |x: RecordError| x.0;
    let no_merge = dry
        .bool(tag("bundle_projection.fortyeightclub_noMerge")?)
        .map_err(e)?;
    let position_first = dry
        .bool(tag("bundle_projection.fortyeightclub_positionFirst")?)
        .map_err(e)?;
    let send_head = dry.u64(tag("bundle_projection.send_head")?).map_err(e)?;
    let max_block = dry.u64(tag("bundle_projection.max_block")?).map_err(e)?;
    if (no_merge, position_first) != (FORTYEIGHTCLUB_NO_MERGE, FORTYEIGHTCLUB_POSITION_FIRST) {
        return Err(format!(
            "the DRY's 48Club projection (noMerge={no_merge}, positionFirst={position_first}) is not the compiled bundle semantics"
        ));
    }
    if max_block.checked_sub(send_head) != Some(MAX_BLOCK_OFFSET) {
        return Err(format!(
            "the DRY's expiry (max_block {max_block} - send_head {send_head}) is not the compiled bundle semantics (+{MAX_BLOCK_OFFSET})"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_bsc_record::schema::g06a::dry;
    use darkbot_bsc_record::{Draft, Value, decode};

    /// The frozen bodies (golden, byte for byte) and the frozen DRY's
    /// projection are these semantics; any single deviation is refused.
    #[test]
    fn the_wire_bodies_are_golden_and_a_deviating_projection_is_refused() {
        assert_eq!(
            club48_body("0xb2", ["0xaa", "0xbb"], 103, 1_002),
            r#"{"id":"0xb2","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":103,"maxTimestamp":1002,"noMerge":true,"positionFirst":true,"txs":["0xaa","0xbb"]}]}"#
        );
        assert_eq!(
            blockrazor_body("0xb2", ["0xaa", "0xbb"], 103),
            r#"{"id":"0xb2","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":103,"txs":["0xaa","0xbb"]}]}"#
        );
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../native/evidence/g06a/g07_binary_import_20261016/dry_result.dbrec");
        let frozen = decode(&dry::SCHEMA, &std::fs::read(path).unwrap()).unwrap();
        assert_eq!(check_projection(&frozen), Ok(()));
        let tag = |name: &str| {
            dry::SCHEMA
                .fields
                .iter()
                .find(|f| f.name == name)
                .unwrap()
                .tag
        };
        let head = frozen.u64(tag("bundle_projection.send_head")).unwrap();
        for (name, value) in [
            (
                "bundle_projection.fortyeightclub_noMerge",
                Value::Bool(false),
            ),
            (
                "bundle_projection.fortyeightclub_positionFirst",
                Value::Bool(false),
            ),
            ("bundle_projection.max_block", Value::U64(head + 1)),
            ("bundle_projection.max_block", Value::U64(head + 3)),
            ("bundle_projection.send_head", Value::U64(head + 3)),
        ] {
            let mut d = Draft::new(&dry::SCHEMA);
            for (t, v) in frozen.fields() {
                d.push(*t, v.clone());
            }
            d.set(tag(name), value);
            let edited = decode(&dry::SCHEMA, &d.encode().unwrap()).unwrap();
            assert!(
                check_projection(&edited)
                    .unwrap_err()
                    .contains("bundle semantics"),
                "{name}"
            );
        }
    }
}

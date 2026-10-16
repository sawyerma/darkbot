//! Re-evaluate saved FE-01 observations using the same production collector.
//! No RPC, signer or execution path is constructed by this module.
use super::fe_qualification::{self as fe, ObservedState};
use crate::analytics::journal::{self, kind, Reader, MANIFEST};
use darkbot_project0::funding::FundingClass;
use darkbot_solana::runtime_config::RuntimeConfig;
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;
use std::{fs, io::Read, path::Path};

/// Binary journal of one FE-01 observation (PERSIST-01) inside its output directory.
pub const FE01_JOURNAL: &str = "fe01.p0j";

/// Saved reads in order with their integrity record, from `fe01.p0j`; `None` for an
/// observation saved before PERSIST-01 (`read_NNNNNN.json`). Both at once are refused.
type SavedReads = Vec<(Value, Value)>;
fn saved_reads(directory: &Path, limit: u64, max_reads: u64) -> Result<Option<SavedReads>, String> {
    let path = directory.join(FE01_JOURNAL);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("FE_REPLAY_IO".into()),
        Ok(meta) if meta.file_type().is_symlink() => return Err("FE_REPLAY_SYMLINK".into()),
        Ok(meta) if !meta.is_file() => return Err("FE_REPLAY_JOURNAL".into()),
        Ok(meta) if meta.len() > limit => return Err("FE_REPLAY_BYTE_LIMIT".into()),
        Ok(_) => {}
    }
    let legacy = fs::read_dir(directory)
        .map_err(|_| "FE_REPLAY_DIRECTORY")?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "FE_REPLAY_DIRECTORY")?
        .iter()
        .any(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|s| s.starts_with("read_") && s.ends_with(".json"))
        });
    if legacy {
        return Err("FE_REPLAY_AMBIGUOUS_SOURCE".into());
    }
    let mut reader = Reader::open(&path).map_err(|_| "FE_REPLAY_JOURNAL")?;
    let (mut reads, mut total, mut configured, mut result) =
        (Vec::new(), journal::HEADER_LEN, false, false);
    while let Some(frame) = reader.next_frame().map_err(|_| "FE_REPLAY_JOURNAL")? {
        // Bound the entire archive, including configuration, result and manifest. Check
        // both initial length and consumed bytes so concurrent growth cannot bypass it.
        total = total
            .checked_add(frame.bytes)
            .filter(|n| *n <= limit)
            .ok_or("FE_REPLAY_BYTE_LIMIT")?;
        match frame.kind {
            kind::FE01_CONFIG if !configured => {
                configured = true;
            }
            kind::FE01_READ if configured && !result => {
                if reads.len() as u64 >= max_reads {
                    return Err("FE_REPLAY_CALL_LIMIT".into());
                }
                let integrity = json!({"journal_seq":frame.journal_seq,"frame_sha256":journal::hex(&frame.hash),"bytes":frame.bytes});
                reads.push((frame.legacy_value().map_err(|e| e.to_string())?, integrity));
            }
            kind::FE01_RESULT if configured && !result && !reads.is_empty() => {
                let value = frame.legacy_value().map_err(|e| e.to_string())?;
                if value["status"] == "INCOMPLETE" {
                    return Err("FE_REPLAY_INCOMPLETE".into());
                }
                if value["logical_reads"].as_u64() != Some(reads.len() as u64) {
                    return Err("FE_REPLAY_READ_COUNT".into());
                }
                result = true;
            }
            MANIFEST if result => {}
            kind::FE01_CONFIG | kind::FE01_READ | kind::FE01_RESULT | MANIFEST => {
                return Err("FE_REPLAY_JOURNAL_ORDER".into())
            }
            _ => return Err("FE_REPLAY_JOURNAL_KIND".into()),
        }
    }
    if !reader.sealed || !reader.complete || !result {
        return Err("FE_REPLAY_INCOMPLETE".into());
    }
    Ok(Some(reads))
}

pub fn first_class(name: &str) -> Result<FundingClass, String> {
    match name {
        "ZERO_PREFUND_SAME_TX" => Ok(FundingClass::ZeroPrefundSameTx),
        "NO_SWAP_INVENTORY" => Ok(FundingClass::NoSwapInventory),
        _ => Err("FE_ONE_CLASS_MUST_BE_ZERO_PREFUND_OR_NO_SWAP".into()),
    }
}
/// Bind an explicitly selected *qualification scope*, never a trading preference
/// or proof that the funding path is executable. Other product classes stay intact.
pub fn bind_scope(report: &mut Value, class: FundingClass) -> Result<(), String> {
    first_class(class.as_str())?;
    let funding_gap = match class {
        FundingClass::ZeroPrefundSameTx => "ZERO_PREFUND_EXECUTABLE_SAME_TX_FLOOR_NOT_PROVEN",
        _ => "NO_SWAP_CURRENT_INVENTORY_AND_REALIZABLE_EXIT_NOT_PROVEN",
    };
    report["qualification_scope"] = json!({"funding_class":class.as_str(),"selection_is_execution_qualification":false,"funding":"UNKNOWN","source_pin":fe::SOURCE_PIN});
    report["qualification_gaps"] = json!([
        "CURRENT_DEPLOYMENT_REPRODUCIBLE_SOURCE_PARITY_NOT_PROVEN",
        "CURRENT_CANDIDATE_STATE_FRESHNESS_NOT_QUALIFIED",
        funding_gap,
        "COMPLETE_CURRENT_TRANSACTION_SIMULATION_NOT_RUN",
        "CURRENT_CLUSTER_TRANSACTION_VERSION_SUPPORT_NOT_OBSERVED",
        "ALL_COSTS_AND_REALIZABLE_EXIT_NOT_PROVEN"
    ]);
    report["fe01"] = json!("INCOMPLETE");
    report["pilot_ready"] = json!(false);
    report["live_enabled"] = json!(false);
    report["submission_enabled"] = json!(false);
    Ok(())
}

/// Exactly match the saved request before decoding bytes with the existing decoder.
pub fn decode_saved(
    entry: Value,
    keys: &[Pubkey],
    floor: u64,
) -> Result<(u64, ObservedState), String> {
    let expected = json!([keys.iter().map(ToString::to_string).collect::<Vec<_>>(),{"encoding":"base64","commitment":"finalized","minContextSlot":floor}]);
    if entry["method"] != "getMultipleAccounts"
        || entry["params"] != expected
        || entry.get("error").is_some()
    {
        return Err("FE_REPLAY_REQUEST_OR_ERROR_MISMATCH".into());
    }
    fe::decode_batch(
        entry
            .get("result")
            .cloned()
            .ok_or("FE_REPLAY_RESULT_MISSING")?,
        keys,
        floor,
    )
}

pub fn replay(
    config: &RuntimeConfig,
    directory: &Path,
    class: FundingClass,
) -> Result<Value, String> {
    first_class(class.as_str())?;
    if config.fe_mode != "full" {
        return Err("FE_REPLAY_REQUIRES_FULL_MODE".into());
    }
    let limit = config
        .limits
        .fe_max_mib
        .min(config.limits.max_total_mib)
        .checked_mul(1024 * 1024)
        .ok_or("FE_REPLAY_BYTE_LIMIT")?;
    let saved = saved_reads(
        directory,
        limit,
        config.limits.fe_calls.min(config.limits.max_requests),
    )?;
    let mut total = 0u64;
    let mut reads = Vec::new();
    let mut report = fe::collect(config, |keys, floor| {
        if reads.len() as u64 >= config.limits.fe_calls.min(config.limits.max_requests) {
            return Err("FE_REPLAY_CALL_LIMIT".into());
        }
        if let Some(frames) = &saved {
            let (entry, integrity) = frames.get(reads.len()).ok_or("FE_REPLAY_READ_MISSING")?;
            let decoded = decode_saved(entry.clone(), keys, floor)?;
            total += integrity["bytes"].as_u64().unwrap_or(0);
            reads.push(integrity.clone());
            return Ok(decoded);
        }
        let name = format!("read_{:06}.json", reads.len() + 1);
        let path = directory.join(&name);
        if fs::symlink_metadata(&path)
            .map_err(|_| "FE_REPLAY_READ_MISSING")?
            .file_type()
            .is_symlink()
        {
            return Err("FE_REPLAY_SYMLINK".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(|_| "FE_REPLAY_READ_MISSING")?
            .take(limit.saturating_sub(total).saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|_| "FE_REPLAY_IO")?;
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or("FE_REPLAY_BYTE_LIMIT")?;
        if total > limit {
            return Err("FE_REPLAY_BYTE_LIMIT".into());
        }
        let entry = serde_json::from_slice(&bytes).map_err(|_| "FE_REPLAY_JSON")?;
        let decoded = decode_saved(entry, keys, floor)?;
        reads.push(json!({"file":name,"sha256":fe::sha(&bytes),"bytes":bytes.len()}));
        Ok(decoded)
    })?;
    // An extra saved read indicates a different/partial collection, not this replay.
    let saved_count = match &saved {
        Some(frames) => frames.len(),
        None => fs::read_dir(directory)
            .map_err(|_| "FE_REPLAY_DIRECTORY")?
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|s| s.starts_with("read_") && s.ends_with(".json"))
            })
            .count(),
    };
    if saved_count != reads.len() {
        return Err("FE_REPLAY_UNUSED_READS".into());
    }
    bind_scope(&mut report, class)?;
    let (source, integrity) = match saved {
        Some(_) => (
            "BINARY_JOURNAL",
            "CRC32_AND_CHAINED_SHA256_FRAMES_NOT_EXTERNAL_ATTESTATION",
        ),
        None => (
            "LEGACY_READ_FILES",
            "SHA256_OF_CONSUMED_BYTES_NOT_EXTERNAL_ATTESTATION",
        ),
    };
    report["replay"] = json!({"mode":"OFFLINE_SAVED_OBSERVATIONS","source":source,"reads":reads,"total_bytes":total,"freshness":"HISTORICAL_NOT_CURRENT","integrity":integrity});
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    #[test]
    fn saved_request_binding_rejects_wrong_target_context_encoding_and_errors() {
        let k = Pubkey::new_unique();
        let entry = json!({"method":"getMultipleAccounts","params":[[k.to_string()],{"encoding":"base64","commitment":"finalized","minContextSlot":5}],"result":{"context":{"slot":6},"value":[{"owner":k.to_string(),"lamports":1,"executable":false,"data":[base64::engine::general_purpose::STANDARD.encode([1,2,3]),"base64"]}]}});
        let (slot, state) = decode_saved(entry.clone(), &[k], 5).unwrap();
        assert_eq!(slot, 6);
        assert_eq!(state.0[&k].data, [1, 2, 3]);
        assert!(decode_saved(entry.clone(), &[Pubkey::new_unique()], 5).is_err());
        assert!(decode_saved(entry.clone(), &[k], 4).is_err());
        for field in ["encoding", "commitment"] {
            let mut bad = entry.clone();
            bad["params"][1][field] = json!("wrong");
            assert!(decode_saved(bad, &[k], 5).is_err());
        }
        let mut bad = entry;
        bad["error"] = json!("rpc failure");
        assert!(decode_saved(bad, &[k], 5).is_err());
    }
    #[test]
    fn one_class_scope_never_promotes_observation_or_unknown_funding() {
        for name in ["ZERO_PREFUND_SAME_TX", "NO_SWAP_INVENTORY"] {
            let class = first_class(name).unwrap();
            let mut v = json!({"source_parity":"UNKNOWN","targets":[{"funding":"UNKNOWN"}]});
            bind_scope(&mut v, class).unwrap();
            assert_eq!(v["qualification_scope"]["funding_class"], name);
            assert_eq!(v["qualification_scope"]["funding"], "UNKNOWN");
            assert_eq!(v["fe01"], "INCOMPLETE");
            assert_eq!(v["pilot_ready"], false);
            assert_eq!(v["targets"][0]["funding"], "UNKNOWN");
        }
        for name in [
            "UNKNOWN",
            "JIT_SHORTFALL",
            "FLASH_FUNDED",
            "",
            "ZERO_PREFUND_SAME_TX,NO_SWAP_INVENTORY",
        ] {
            assert!(first_class(name).is_err());
        }
        assert!(include_str!("../../../../Cargo.lock").contains(fe::SOURCE_PIN));
    }
}

#[cfg(test)]
mod archive_tests {
    use super::*;
    use crate::analytics::journal::Writer;
    fn archive(events: &[(u16, Value)], seal: Option<bool>) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("p0-fe-archive-{}", Pubkey::new_unique()));
        fs::create_dir(&dir).unwrap();
        let mut w = Writer::create(&dir.join(FE01_JOURNAL)).unwrap();
        for (k, v) in events {
            w.append_value(*k, v).unwrap();
        }
        if let Some(complete) = seal {
            w.seal(complete).unwrap();
        } else {
            w.sync().unwrap();
        }
        dir
    }
    fn events() -> Vec<(u16, Value)> {
        vec![
            (kind::FE01_CONFIG, json!({})),
            (kind::FE01_READ, json!({"result":{}})),
            (
                kind::FE01_RESULT,
                json!({"status":"OBSERVED","logical_reads":1}),
            ),
        ]
    }
    #[test]
    fn fe_replay_requires_complete_ordered_archive_and_exact_read_count() {
        let good = events();
        let mut wrong_count = good.clone();
        wrong_count[2].1["logical_reads"] = json!(2);
        let mut failed = good.clone();
        failed[2].1["status"] = json!("INCOMPLETE");
        let cases = vec![
            (good.clone(), None, "FE_REPLAY_INCOMPLETE"),
            (good.clone(), Some(false), "FE_REPLAY_INCOMPLETE"),
            (good[..2].to_vec(), None, "FE_REPLAY_INCOMPLETE"),
            (good[..2].to_vec(), Some(true), "FE_REPLAY_JOURNAL_ORDER"),
            (good[1..].to_vec(), Some(true), "FE_REPLAY_JOURNAL_ORDER"),
            (
                vec![good[0].clone(), good[0].clone()],
                Some(true),
                "FE_REPLAY_JOURNAL_ORDER",
            ),
            (
                vec![good[0].clone(), good[2].clone()],
                Some(true),
                "FE_REPLAY_JOURNAL_ORDER",
            ),
            (
                vec![
                    good[0].clone(),
                    good[1].clone(),
                    good[2].clone(),
                    good[1].clone(),
                ],
                Some(true),
                "FE_REPLAY_JOURNAL_ORDER",
            ),
            (wrong_count, Some(true), "FE_REPLAY_READ_COUNT"),
            (failed, Some(true), "FE_REPLAY_INCOMPLETE"),
            (
                vec![(kind::OWNER_RECORD, json!({}))],
                Some(true),
                "FE_REPLAY_JOURNAL_KIND",
            ),
        ];
        for (events, seal, error) in cases {
            let dir = archive(&events, seal);
            assert_eq!(saved_reads(&dir, 1_000_000, 10).unwrap_err(), error);
            fs::remove_dir_all(dir).unwrap();
        }
        let dir = archive(&good, Some(true));
        assert_eq!(saved_reads(&dir, 1_000_000, 1).unwrap().unwrap().len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn fe_replay_bounds_all_frames_and_reads_and_rejects_corruption() {
        let mut e = events();
        e[0].1 = json!({"padding":"x".repeat(8192)});
        let dir = archive(&e, Some(true));
        assert_eq!(
            saved_reads(&dir, 4096, 10).unwrap_err(),
            "FE_REPLAY_BYTE_LIMIT"
        );
        let path = dir.join(FE01_JOURNAL);
        let size = fs::metadata(&path).unwrap().len();
        assert!(saved_reads(&dir, size, 1).unwrap().is_some());
        assert_eq!(
            saved_reads(&dir, size, 0).unwrap_err(),
            "FE_REPLAY_CALL_LIMIT"
        );
        assert_eq!(
            saved_reads(&dir, size - 1, 10).unwrap_err(),
            "FE_REPLAY_BYTE_LIMIT"
        );
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&path, &bytes).unwrap();
        assert_eq!(
            saved_reads(&dir, size, 10).unwrap_err(),
            "FE_REPLAY_JOURNAL"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}

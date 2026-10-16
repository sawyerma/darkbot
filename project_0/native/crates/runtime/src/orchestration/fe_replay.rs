//! Re-evaluate saved FE-01 observations using the same production collector.
//! No RPC, signer or execution path is constructed by this module.
use super::fe_qualification::{self as fe, ObservedState};
use darkbot_project0::funding::FundingClass;
use darkbot_solana::runtime_config::RuntimeConfig;
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;
use std::{fs, io::Read, path::Path};

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
    let mut total = 0u64;
    let mut reads = Vec::new();
    let mut report = fe::collect(config, |keys, floor| {
        if reads.len() as u64 >= config.limits.fe_calls.min(config.limits.max_requests) {
            return Err("FE_REPLAY_CALL_LIMIT".into());
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
    let saved_count = fs::read_dir(directory)
        .map_err(|_| "FE_REPLAY_DIRECTORY")?
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|s| s.starts_with("read_") && s.ends_with(".json"))
        })
        .count();
    if saved_count != reads.len() {
        return Err("FE_REPLAY_UNUSED_READS".into());
    }
    bind_scope(&mut report, class)?;
    report["replay"] = json!({"mode":"OFFLINE_SAVED_OBSERVATIONS","reads":reads,"total_bytes":total,"freshness":"HISTORICAL_NOT_CURRENT","integrity":"SHA256_OF_CONSUMED_BYTES_NOT_EXTERNAL_ATTESTATION"});
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

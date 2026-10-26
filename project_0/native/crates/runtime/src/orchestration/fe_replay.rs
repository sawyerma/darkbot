//! Re-evaluate saved FE-01 observations using the same production collector.
//! No RPC, signer or execution path is constructed by this module.
#[cfg(feature = "historical")]
use super::fe_qualification::WireMultipleAccounts;
use super::fe_qualification::{self as fe, ObservedState};
use crate::analytics::journal::{self, kind, Reader, MANIFEST};
use crate::analytics::payload::{
    Fe01Read, Fe01ReadOutcome, Fe01Replay, Fe01ReplayRead, Fe01Report, Fe01Scope,
};
use darkbot_project0::funding::FundingClass;
use darkbot_solana::runtime_config::RuntimeConfig;
#[cfg(feature = "historical")]
use serde_json::Value;
use solana_sdk::pubkey::Pubkey;
#[cfg(feature = "historical")]
use std::io::Read;
use std::{fs, path::Path};

/// Binary journal of one FE-01 observation (PERSIST-01) inside its output directory.
pub const FE01_JOURNAL: &str = "fe01.p0j";
/// Binary journal of one offline replay (configuration and report), beside its freeze.
pub const FE01_REPLAY_JOURNAL: &str = "fe01_replay.p0j";

#[cfg(feature = "historical")]
/// Historical reader (JFI-03): one read saved before JFI-04 — a schema-1 FE01_READ frame or a
/// `read_NNNNNN.json` file `{method, params, result|error}` — as the typed read. The request must
/// be exactly a base64/finalized `getMultipleAccounts`; an error or an undecodable result is a
/// failed read. Nothing else in this module reads JSON (DEC-09: the `historical` build only).
pub fn historical_read(entry: &Value) -> Result<Fe01Read, String> {
    let mismatch = || "FE_REPLAY_REQUEST_OR_ERROR_MISMATCH".to_string();
    let options = &entry["params"][1];
    if entry["method"] != "getMultipleAccounts"
        || entry["params"].as_array().map(Vec::len) != Some(2)
        || options.as_object().map(|o| o.len()) != Some(3)
        || options["encoding"] != "base64"
        || options["commitment"] != "finalized"
    {
        return Err(mismatch());
    }
    let keys = entry["params"][0]
        .as_array()
        .ok_or_else(mismatch)?
        .iter()
        .map(|k| k.as_str().and_then(|k| k.parse().ok()).ok_or_else(mismatch))
        .collect::<Result<Vec<Pubkey>, _>>()?;
    let min_context_slot = options["minContextSlot"].as_u64().ok_or_else(mismatch)?;
    let outcome = match (entry.get("error"), entry.get("result")) {
        (None, Some(result)) => {
            match <WireMultipleAccounts as serde::Deserialize>::deserialize(result) {
                Ok(wire) => wire.outcome(),
                Err(_) => Fe01ReadOutcome::Failed {
                    reason: "FE_REPLAY_RESULT_DECODE".into(),
                },
            }
        }
        (None, None) => return Err("FE_REPLAY_RESULT_MISSING".into()),
        (Some(_), _) => Fe01ReadOutcome::Failed { reason: mismatch() },
    };
    Ok(Fe01Read {
        keys,
        min_context_slot,
        outcome,
    })
}

/// Saved reads in order with their integrity record, from `fe01.p0j`; `None` for an
/// observation saved before PERSIST-01 (`read_NNNNNN.json`). Both at once are refused.
type SavedReads = Vec<(Fe01Read, Fe01ReplayRead)>;
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
        let historical = frame.schema == 1;
        match frame.kind {
            kind::FE01_CONFIG if !configured => {
                configured = true;
            }
            kind::FE01_READ if configured && !result => {
                if reads.len() as u64 >= max_reads {
                    return Err("FE_REPLAY_CALL_LIMIT".into());
                }
                let read = if historical {
                    historical_frame_read(&frame)?
                } else {
                    frame.typed::<Fe01Read>().map_err(|e| e.to_string())?
                };
                let integrity = Fe01ReplayRead {
                    origin: format!("journal_seq={}", frame.journal_seq),
                    sha256: journal::hex(&frame.hash),
                    bytes: frame.bytes,
                };
                reads.push((read, integrity));
            }
            kind::FE01_RESULT if configured && !result && !reads.is_empty() => {
                let (status, logical_reads) = if historical {
                    let value = frame.legacy_tree().map_err(|e| e.to_string())?;
                    (
                        value
                            .get("status")
                            .and_then(journal::LegacyNode::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        value
                            .get("logical_reads")
                            .and_then(journal::LegacyNode::as_u64),
                    )
                } else {
                    let report = frame.typed::<Fe01Report>().map_err(|e| e.to_string())?;
                    (report.status, report.logical_reads)
                };
                if status == fe::INCOMPLETE {
                    return Err("FE_REPLAY_INCOMPLETE".into());
                }
                if logical_reads != Some(reads.len() as u64) {
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

/// A schema-1 FE01_READ frame (before JFI-04) as the typed read.
#[cfg(feature = "historical")]
fn historical_frame_read(frame: &journal::Frame) -> Result<Fe01Read, String> {
    historical_read(&frame.legacy_value().map_err(|e| e.to_string())?)
}
#[cfg(not(feature = "historical"))]
fn historical_frame_read(_: &journal::Frame) -> Result<Fe01Read, String> {
    Err(crate::analytics::renders::HISTORICAL_READER_NOT_BUILT.into())
}

/// One pre-PERSIST-01 `read_NNNNNN.json` (A14, `--legacy-reads`) with its integrity record,
/// bounded by the replay's byte budget.
#[cfg(feature = "historical")]
fn legacy_file_read(
    directory: &Path,
    n: usize,
    limit: u64,
    total: &mut u64,
) -> Result<(Fe01Read, Fe01ReplayRead), String> {
    let name = format!("read_{n:06}.json");
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
        .take(limit.saturating_sub(*total).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| "FE_REPLAY_IO")?;
    *total = total
        .checked_add(bytes.len() as u64)
        .ok_or("FE_REPLAY_BYTE_LIMIT")?;
    if *total > limit {
        return Err("FE_REPLAY_BYTE_LIMIT".into());
    }
    let entry: Value = serde_json::from_slice(&bytes).map_err(|_| "FE_REPLAY_JSON")?;
    let integrity = Fe01ReplayRead {
        origin: name,
        sha256: fe::sha(&bytes),
        bytes: bytes.len() as u64,
    };
    Ok((historical_read(&entry)?, integrity))
}
#[cfg(not(feature = "historical"))]
fn legacy_file_read(
    _: &Path,
    _: usize,
    _: u64,
    _: &mut u64,
) -> Result<(Fe01Read, Fe01ReplayRead), String> {
    Err(crate::analytics::renders::HISTORICAL_READER_NOT_BUILT.into())
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
pub fn bind_scope(report: &mut Fe01Report, class: FundingClass) -> Result<(), String> {
    first_class(class.as_str())?;
    let funding_gap = match class {
        FundingClass::ZeroPrefundSameTx => "ZERO_PREFUND_EXECUTABLE_SAME_TX_FLOOR_NOT_PROVEN",
        _ => "NO_SWAP_CURRENT_INVENTORY_AND_REALIZABLE_EXIT_NOT_PROVEN",
    };
    report.scope = Some(Fe01Scope {
        funding_class: class.as_str().into(),
        selection_is_execution_qualification: false,
        funding: "UNKNOWN".into(),
        source_pin: fe::SOURCE_PIN.into(),
        gaps: [
            "CURRENT_DEPLOYMENT_REPRODUCIBLE_SOURCE_PARITY_NOT_PROVEN",
            "CURRENT_CANDIDATE_STATE_FRESHNESS_NOT_QUALIFIED",
            funding_gap,
            "COMPLETE_CURRENT_TRANSACTION_SIMULATION_NOT_RUN",
            "CURRENT_CLUSTER_TRANSACTION_VERSION_SUPPORT_NOT_OBSERVED",
            "ALL_COSTS_AND_REALIZABLE_EXIT_NOT_PROVEN",
        ]
        .map(String::from)
        .into(),
    });
    report.fe01 = "INCOMPLETE".into();
    report.pilot_ready = false;
    report.live_enabled = false;
    report.submission_enabled = false;
    Ok(())
}

/// Exactly match the saved request before validating its accounts with the production check.
pub fn decode_saved(
    read: &Fe01Read,
    keys: &[Pubkey],
    floor: u64,
) -> Result<(u64, ObservedState), String> {
    if read.keys != keys || read.min_context_slot != floor {
        return Err("FE_REPLAY_REQUEST_OR_ERROR_MISMATCH".into());
    }
    if let Fe01ReadOutcome::Failed { .. } = read.outcome {
        return Err("FE_REPLAY_REQUEST_OR_ERROR_MISMATCH".into());
    }
    fe::observed(read)
}

pub fn replay(
    config: &RuntimeConfig,
    directory: &Path,
    class: FundingClass,
    legacy_reads: bool,
) -> Result<Fe01Report, String> {
    replay_frozen(config, directory, class, legacy_reads).map(|(report, _)| report)
}

/// The replay report and the historical one-class freeze of the same saved scope (FE-01). The
/// saved observation is the binary `fe01.p0j`; a pre-PERSIST-01 observation (`read_NNNNNN.json`
/// files) is a historical source, read only with `legacy_reads` (A14, `--legacy-reads`).
pub fn replay_frozen(
    config: &RuntimeConfig,
    directory: &Path,
    class: FundingClass,
    legacy_reads: bool,
) -> Result<
    (
        Fe01Report,
        Result<crate::analytics::payload::Fe01Freeze, String>,
    ),
    String,
> {
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
    if saved.is_none() && !legacy_reads {
        return Err(format!(
            "FE_REPLAY_JOURNAL_MISSING: no {FE01_JOURNAL} (a pre-PERSIST-01 read_NNNNNN.json observation is read only with --legacy-reads)"
        ));
    }
    let mut total = 0u64;
    let mut reads = Vec::new();
    let (mut report, freeze) = fe::collect_frozen(
        config,
        |keys, floor| {
            if reads.len() as u64 >= config.limits.fe_calls.min(config.limits.max_requests) {
                return Err("FE_REPLAY_CALL_LIMIT".into());
            }
            if let Some(frames) = &saved {
                let (read, integrity) = frames.get(reads.len()).ok_or("FE_REPLAY_READ_MISSING")?;
                let decoded = decode_saved(read, keys, floor)?;
                total += integrity.bytes;
                reads.push(integrity.clone());
                return Ok(decoded);
            }
            let (read, integrity) =
                legacy_file_read(directory, reads.len() + 1, limit, &mut total)?;
            let decoded = decode_saved(&read, keys, floor)?;
            reads.push(integrity);
            Ok(decoded)
        },
        class,
        true,
    )?;
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
    report.replay = Some(Fe01Replay {
        mode: "OFFLINE_SAVED_OBSERVATIONS".into(),
        source: source.into(),
        reads,
        total_bytes: total,
        freshness: "HISTORICAL_NOT_CURRENT".into(),
        integrity: integrity.into(),
    });
    Ok((report, freeze))
}

/// DEC-09: the standard build reads no historical FE-01 source: a pre-PERSIST-01 read file (even
/// with `--legacy-reads`) and a historical run manifest are refused with an explicit code.
#[cfg(all(test, not(feature = "historical")))]
mod dec09_tests {
    use super::*;
    #[test]
    fn dec09_standard_build_refuses_historical_fe01_sources() {
        let dir = std::env::temp_dir().join(format!("p0-dec09-fe01-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("read_000001.json"), b"{}").unwrap();
        let config = darkbot_solana::runtime_config::RuntimeConfig {
            fe_mode: "full".into(),
            ..Default::default()
        };
        let class = first_class("ZERO_PREFUND_SAME_TX").unwrap();
        let err = replay(&config, &dir, class, true).unwrap_err();
        assert!(
            err.contains(crate::analytics::renders::HISTORICAL_READER_NOT_BUILT),
            "{err}"
        );
        fs::write(dir.join("manifest.json"), br#"{"layout":"v2"}"#).unwrap();
        assert_eq!(
            crate::analytics::renders::detect(&dir).unwrap_err(),
            crate::analytics::renders::HISTORICAL_READER_NOT_BUILT
        );
        fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "historical")]
    use base64::Engine;
    #[cfg(feature = "historical")]
    use serde_json::json;
    #[cfg(feature = "historical")]
    /// A saved read is bound to its exact request (keys, floor, base64, finalized) and to a
    /// decodable answer, from a historical frame as from a typed one.
    #[test]
    fn saved_request_binding_rejects_wrong_target_context_encoding_and_errors() {
        let k = Pubkey::new_unique();
        let entry = json!({"method":"getMultipleAccounts","params":[[k.to_string()],{"encoding":"base64","commitment":"finalized","minContextSlot":5}],"result":{"context":{"slot":6},"value":[{"owner":k.to_string(),"lamports":1,"executable":false,"data":[base64::engine::general_purpose::STANDARD.encode([1,2,3]),"base64"]}]}});
        let read = historical_read(&entry).unwrap();
        let (slot, state) = decode_saved(&read, &[k], 5).unwrap();
        assert_eq!(slot, 6);
        assert_eq!(state.0[&k].data, [1, 2, 3]);
        assert!(decode_saved(&read, &[Pubkey::new_unique()], 5).is_err());
        assert!(decode_saved(&read, &[k], 4).is_err());
        for field in ["encoding", "commitment"] {
            let mut bad = entry.clone();
            bad["params"][1][field] = json!("wrong");
            assert!(historical_read(&bad).is_err());
        }
        let mut bad = entry.clone();
        bad["error"] = json!("rpc failure");
        assert!(decode_saved(&historical_read(&bad).unwrap(), &[k], 5).is_err());
        let mut bad = entry;
        bad["result"] = json!({"context":{"slot":6}});
        let undecodable = historical_read(&bad).unwrap();
        assert!(matches!(
            undecodable.outcome,
            Fe01ReadOutcome::Failed { .. }
        ));
        assert!(decode_saved(&undecodable, &[k], 5).is_err());
    }
    #[test]
    fn one_class_scope_never_promotes_observation_or_unknown_funding() {
        for name in ["ZERO_PREFUND_SAME_TX", "NO_SWAP_INVENTORY"] {
            let class = first_class(name).unwrap();
            let mut v = fe::incomplete("x".into());
            v.pilot_ready = true;
            bind_scope(&mut v, class).unwrap();
            let scope = v.scope.unwrap();
            assert_eq!(scope.funding_class, name);
            assert_eq!(scope.funding, "UNKNOWN");
            assert!(!scope.selection_is_execution_qualification);
            assert_eq!(scope.gaps.len(), 6);
            assert_eq!(v.fe01, "INCOMPLETE");
            assert!(!v.pilot_ready && !v.live_enabled && !v.submission_enabled);
        }
        for name in [
            "UNKNOWN",
            "JIT_SHORTFALL",
            "JIT_SHORTFALL_EXACT_IN",
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
    use crate::analytics::payload::Fe01Account;
    #[derive(Clone)]
    enum Event {
        Config,
        /// A read whose one account carries `data_len` bytes.
        Read(usize),
        Result(&'static str, u64),
        Foreign,
    }
    fn archive(events: &[Event], seal: Option<bool>) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("p0-fe-archive-{}", Pubkey::new_unique()));
        fs::create_dir(&dir).unwrap();
        let mut w = Writer::create(&dir.join(FE01_JOURNAL)).unwrap();
        let effective = darkbot_solana::runtime_config::Resolved::parse(&[], &|_| None)
            .unwrap()
            .effective();
        for e in events {
            match e {
                Event::Config => w.append_typed(
                    Default::default(),
                    &crate::analytics::payload::Fe01Config {
                        requested: effective.clone(),
                        effective: effective.clone(),
                        qualification_sha256: None,
                        execution: "READ_ONLY_NO_SIGNER".into(),
                    },
                ),
                Event::Read(n) => w.append_typed(
                    Default::default(),
                    &Fe01Read {
                        keys: vec![Pubkey::default()],
                        min_context_slot: 0,
                        outcome: Fe01ReadOutcome::Accounts {
                            context_slot: 1,
                            accounts: vec![Some(Fe01Account {
                                owner: Pubkey::default(),
                                lamports: 1,
                                executable: false,
                                data: vec![0; *n],
                            })],
                        },
                    },
                ),
                Event::Result(status, reads) => {
                    let mut report = fe::incomplete("x".into());
                    (report.status, report.logical_reads) = (status.to_string(), Some(*reads));
                    w.append_typed(Default::default(), &report)
                }
                // A foreign frame as found on disk; the writer itself refuses foreign kinds.
                Event::Foreign => {
                    w.test_append_historical(kind::OWNER_RECORD, &serde_json::json!({}))
                }
            }
            .unwrap();
        }
        if let Some(complete) = seal {
            w.seal(complete).unwrap();
        } else {
            w.sync().unwrap();
        }
        dir
    }
    fn events() -> Vec<Event> {
        vec![
            Event::Config,
            Event::Read(3),
            Event::Result(fe::OBSERVED, 1),
        ]
    }
    #[test]
    fn fe_replay_requires_complete_ordered_archive_and_exact_read_count() {
        let good = events();
        let mut wrong_count = good.clone();
        wrong_count[2] = Event::Result(fe::OBSERVED, 2);
        let mut failed = good.clone();
        failed[2] = Event::Result(fe::INCOMPLETE, 1);
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
            (vec![Event::Foreign], Some(true), "FE_REPLAY_JOURNAL_KIND"),
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
        e[1] = Event::Read(8192);
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

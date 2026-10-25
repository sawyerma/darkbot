//! Read-only qualification entrypoint. Existing Budget/Health/Sizing, no signer.
#[cfg(feature = "crate_health")]
fn run() -> Result<(), String> {
    use darkbot_runner::analytics::journal::Writer;
    use darkbot_runner::analytics::payload::{Fe01Config, Fe01ReadOutcome};
    use darkbot_runner::orchestration::fe_qualification::{self as fe, ObservedState};
    use darkbot_runner::orchestration::{fe_freeze, fe_replay};
    use darkbot_solana::{provider, runtime_config::Resolved};
    use solana_sdk::pubkey::Pubkey;
    use std::{
        fs,
        path::PathBuf,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    let mut args = std::env::args().skip(1);
    let mut operational = Vec::new();
    let mut execute = false;
    let mut output = None;
    let mut qualification = None;
    let mut replay = None;
    let mut against = None;
    let mut legacy_reads = false;
    let mut funding_class = "ZERO_PREFUND_SAME_TX".to_string();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--execute" => execute = true,
            "--legacy-reads" => legacy_reads = true,
            "--replay" => {
                replay = Some(
                    args.next()
                        .map(PathBuf::from)
                        .ok_or("FE_REPLAY_DIRECTORY_REQUIRED")?,
                )
            }
            "--funding-class" => funding_class = args.next().ok_or("FE_FUNDING_CLASS_REQUIRED")?,
            "--against" => {
                against = Some(
                    args.next()
                        .map(PathBuf::from)
                        .ok_or("FE_AGAINST_DIRECTORY_REQUIRED")?,
                )
            }
            "--output" => output = args.next().map(PathBuf::from),
            "--provider-qualification" => qualification = args.next().map(PathBuf::from),
            _ => operational.push(arg),
        }
    }
    if operational.iter().any(|s| s == "--help" || s == "-h") {
        println!("p0-fe-qualify --fe-mode smoke --execute --output NEW_DIRECTORY [operational options]   (P1A deployment/target observation)\np0-fe-qualify (--execute --provider-qualification FILE | --replay SAVED_DIRECTORY [--legacy-reads]) --output NEW_DIRECTORY --fe-mode full [--funding-class ZERO_PREFUND_SAME_TX|NO_SWAP_INVENTORY] [--against EARLIER_OUTPUT (freeze drift blocks)] [operational options]\n{}",Resolved::help());
        return Ok(());
    }
    let mut resolved = Resolved::parse(&operational, &|k| std::env::var(k).ok())?;
    if resolved.print {
        print!("{}", resolved.text());
        return Ok(());
    }
    if resolved.config.fe_mode == "smoke" {
        // The bounded deployment/target observation; explicit `--execute` only, no replay,
        // freeze or provider qualification (it reads three times through the provider budget).
        if !execute || replay.is_some() || against.is_some() || qualification.is_some() {
            return Err("FE_SMOKE_TAKES_ONLY_EXECUTE_AND_OUTPUT".into());
        }
        return smoke(resolved, output.ok_or("FE_OUTPUT_REQUIRED")?);
    }
    let funding_class = fe_replay::first_class(&funding_class)?;
    if execute == replay.is_some() {
        return Err("FE_EXACTLY_ONE_OF_EXECUTE_OR_REPLAY".into());
    }
    if (!execute && replay.is_none()) || resolved.config.fe_mode != "full" {
        return Err("FE_FULL_REQUIRES_EXPLICIT_EXECUTE_AND_FULL_MODE".into());
    }
    let output = output.ok_or("FE_OUTPUT_REQUIRED")?;
    if output.exists() {
        return Err("FE_OUTPUT_EXISTS_PRESERVE_EVIDENCE".into());
    }
    if let Some(directory) = replay {
        let (mut report, freeze) =
            fe_replay::replay_frozen(&resolved.config, &directory, funding_class, legacy_reads)?;
        fs::create_dir(&output).map_err(|_| "FE_OUTPUT_CREATE")?;
        // FE-01 freeze: its own typed journal; drift against an earlier freeze blocks.
        let (summary, unblocked) = fe_freeze::record(&output, Some(freeze), against.as_deref())?;
        report.freeze = Some(summary);
        let effective = resolved.effective();
        let config = Fe01Config {
            requested: effective.clone(),
            effective,
            qualification_sha256: None,
            execution: "OFFLINE_REPLAY_NO_RPC_NO_SIGNER".into(),
        };
        let mut journal = Writer::create(&output.join(fe_replay::FE01_REPLAY_JOURNAL))
            .map_err(|_| "FE_EVIDENCE_IO")?;
        record(&mut journal, &config, None)?;
        record(&mut journal, &report, None)?;
        journal.seal(true).map_err(|_| "FE_EVIDENCE_JOURNAL")?;
        persist(&output.join("result.txt"), &report.text())?;
        print!("{}", report.text());
        if !unblocked {
            return Err("FE_FREEZE_BLOCKED_EVIDENCE_SAVED".into());
        }
        return Ok(());
    }
    let url = resolved
        .config
        .rpc_url(&|k| std::env::var(k).ok())
        .ok_or("RPC_URL_MISSING")?;
    let requested = resolved.effective();
    let l = &mut resolved.config.limits;
    l.max_requests = l.max_requests.min(l.fe_calls);
    l.max_total_mib = l.max_total_mib.min(l.fe_max_mib);
    l.max_response_mib = l.max_response_mib.min(l.max_total_mib);
    l.wss_message_mib = l.wss_message_mib.min(l.max_total_mib);
    let config = resolved.config.clone();
    // Existing FREE_ONLY qualification shape; a configured URL is not capacity evidence.
    // Decoded at the file edge into the typed evidence (X2); no value tree is passed on.
    let proof = fe::ProviderEvidence::from_measured_json(
        &fs::read(qualification.ok_or("FREE_ONLY_PROVIDER_QUALIFICATION_REQUIRED")?)
            .map_err(|_| "FE_QUALIFICATION_IO")?,
    )
    .map_err(|_| "FE_QUALIFICATION_JSON")?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "CLOCK_INVALID")?
        .as_secs_f64();
    fe::validate_provider_evidence(&config, &url, &proof, now)?;
    fs::create_dir(&output).map_err(|_| "FE_OUTPUT_CREATE")?;
    // PERSIST-01/JFI-04: configuration, every saved read and the result are typed frames of one
    // binary journal; each frame is fsynced before the read is used or the next one is made.
    let mut evidence =
        Writer::create(&output.join(fe_replay::FE01_JOURNAL)).map_err(|_| "FE_EVIDENCE_IO")?;
    let operational = Fe01Config {
        requested,
        effective: resolved.effective(),
        qualification_sha256: Some(proof.source_sha256.clone()),
        execution: "READ_ONLY_NO_SIGNER".into(),
    };
    record(&mut evidence, &operational, None)?;
    resolved.activate()?;
    let deadline = Instant::now() + Duration::from_secs(config.limits.fe_seconds);
    let mut calls = 0u64;
    let result = {
        let mut read = |keys: &[Pubkey], floor: u64| -> Result<(u64, ObservedState), String> {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("FE_TIME_BUDGET".into());
            }
            calls += 1;
            let read = multiple_accounts(
                &url,
                remaining.min(Duration::from_millis(config.limits.rpc_timeout_ms)),
                keys,
                floor,
            );
            let slot = match &read.outcome {
                Fe01ReadOutcome::Accounts { context_slot, .. } => Some(*context_slot),
                Fe01ReadOutcome::Failed { .. } => None,
            };
            record(&mut evidence, &read, slot)?;
            if Instant::now() >= deadline {
                return Err("FE_TIME_BUDGET".into());
            }
            fe::observed(&read)
        };
        match fe::collect_frozen(&config, &mut read, funding_class, false) {
            Ok((report, freeze)) => (report, Some(freeze)),
            Err(e) => (fe::incomplete(e), None),
        }
    };
    let (mut result, freeze) = result;
    if Instant::now() >= deadline {
        result.status = fe::INCOMPLETE.into();
        result.reason = Some("FE_TIME_BUDGET".into());
    }
    fe_replay::bind_scope(&mut result, funding_class)?;
    let complete = result.status != fe::INCOMPLETE;
    let freeze = if complete { freeze } else { None };
    let (summary, unblocked) = fe_freeze::record(&output, freeze, against.as_deref())?;
    result.freeze = Some(summary);
    result.logical_reads = Some(calls);
    result.provider_actual = Some(provider::shared().snapshot());
    result.provider_cooldown = Some(provider::shared().cooldown());
    record(&mut evidence, &result, None)?;
    evidence.seal(complete).map_err(|_| "FE_EVIDENCE_JOURNAL")?;
    persist(&output.join("result.txt"), &result.text())?;
    print!("{}", result.text());
    if !complete {
        return Err("FE_FULL_OBSERVATION_INCOMPLETE_EVIDENCE_SAVED".into());
    }
    if !unblocked {
        return Err("FE_FREEZE_BLOCKED_EVIDENCE_SAVED".into());
    }
    Ok(())
}
/// One `getMultipleAccounts` through the provider budget: the JSON-RPC wire edge (JFI-02),
/// request params out, the typed answer in.
#[cfg(feature = "crate_health")]
fn multiple_accounts(
    url: &str,
    timeout: std::time::Duration,
    keys: &[solana_sdk::pubkey::Pubkey],
    floor: u64,
) -> darkbot_runner::analytics::payload::Fe01Read {
    use darkbot_runner::analytics::payload::{Fe01Read, Fe01ReadOutcome};
    use darkbot_runner::orchestration::fe_qualification as fe;
    use darkbot_solana::provider;
    use solana_client::rpc_request::RpcRequest;
    use solana_commitment_config::CommitmentConfig;
    let params = serde_json::json!([keys.iter().map(ToString::to_string).collect::<Vec<_>>(),{"encoding":"base64","commitment":"finalized","minContextSlot":floor}]);
    let rpc = provider::rpc_client(
        url.to_string(),
        timeout,
        CommitmentConfig::finalized(),
        provider::Workload::Bootstrap,
    );
    let outcome =
        match rpc.send::<fe::WireMultipleAccounts>(RpcRequest::GetMultipleAccounts, params) {
            Ok(wire) => wire.outcome(),
            Err(_) => Fe01ReadOutcome::Failed {
                reason: "FE_RPC_FAILURE".into(),
            },
        };
    Fe01Read {
        keys: keys.to_vec(),
        min_context_slot: floor,
        outcome,
    }
}

/// `--fe-mode smoke --execute --output DIR`: the P1A deployment/target observation. Every read
/// and the result are typed frames of `fe01_smoke.p0j` (each fsynced before it is used), the
/// text report beside it; INCOMPLETE keeps the evidence and exits non-zero.
#[cfg(feature = "crate_health")]
fn smoke(
    mut resolved: darkbot_solana::runtime_config::Resolved,
    output: std::path::PathBuf,
) -> Result<(), String> {
    use darkbot_runner::analytics::journal::Writer;
    use darkbot_runner::analytics::payload::Fe01Config;
    use darkbot_runner::orchestration::fe_qualification as fe;
    use darkbot_solana::provider;
    use std::time::{Duration, Instant};
    if output.exists() {
        return Err("FE_OUTPUT_EXISTS_PRESERVE_EVIDENCE".into());
    }
    let url = resolved
        .config
        .rpc_url(&|k| std::env::var(k).ok())
        .ok_or("RPC_URL_MISSING")?;
    let requested = resolved.effective();
    let l = &mut resolved.config.limits;
    l.max_requests = l.max_requests.min(l.fe_calls);
    l.max_total_mib = l.max_total_mib.min(l.fe_max_mib);
    l.max_response_mib = l.max_response_mib.min(l.max_total_mib);
    let config = resolved.config.clone();
    std::fs::create_dir(&output).map_err(|_| "FE_OUTPUT_CREATE")?;
    let mut evidence =
        Writer::create(&output.join(fe::FE01_SMOKE_JOURNAL)).map_err(|_| "FE_EVIDENCE_IO")?;
    let operational = Fe01Config {
        requested,
        effective: resolved.effective(),
        qualification_sha256: None,
        execution: "READ_ONLY_SMOKE_NO_SIGNER".into(),
    };
    record(&mut evidence, &operational, None)?;
    resolved.activate()?;
    let deadline = Instant::now() + Duration::from_secs(config.limits.fe_seconds);
    let mut journal_error = None;
    let mut result = fe::smoke(&config, |keys, floor| {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("FE_TIME_BUDGET".into());
        }
        let read = multiple_accounts(
            &url,
            remaining.min(Duration::from_millis(config.limits.rpc_timeout_ms)),
            keys,
            floor,
        );
        let slot = match &read.outcome {
            darkbot_runner::analytics::payload::Fe01ReadOutcome::Accounts {
                context_slot, ..
            } => Some(*context_slot),
            darkbot_runner::analytics::payload::Fe01ReadOutcome::Failed { .. } => None,
        };
        // A read is used only after its frame is durable.
        if let Err(e) = record(&mut evidence, &read, slot) {
            journal_error = Some(e.clone());
            return Err(e);
        }
        Ok(read)
    });
    if let Some(e) = journal_error {
        return Err(e);
    }
    result.provider_actual = Some(provider::shared().snapshot());
    let complete = result.status == fe::SMOKE_OBSERVED;
    record(&mut evidence, &result, None)?;
    evidence.seal(complete).map_err(|_| "FE_EVIDENCE_JOURNAL")?;
    persist(&output.join("result.txt"), &result.text())?;
    print!("{}", result.text());
    if !complete {
        return Err("FE_SMOKE_INCOMPLETE_EVIDENCE_SAVED".into());
    }
    Ok(())
}

/// One typed FE-01 frame, fsynced before the caller goes on.
#[cfg(feature = "crate_health")]
fn record<P: darkbot_runner::analytics::payload::Payload>(
    journal: &mut darkbot_runner::analytics::journal::Writer,
    payload: &P,
    slot: Option<u64>,
) -> Result<(), String> {
    use darkbot_runner::analytics::journal::{self, Provenance};
    use std::time::{SystemTime, UNIX_EPOCH};
    let provenance = Provenance {
        source: journal::source::FE01,
        source_seq: journal.next_seq(),
        slot,
        unix_ns: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|d| u64::try_from(d.as_nanos()).ok()),
        ..Default::default()
    };
    journal
        .append_typed(provenance, payload)
        .and_then(|_| journal.sync())
        .map(|_| ())
        .map_err(|_| "FE_EVIDENCE_JOURNAL".into())
}
/// The text report beside the journal (PERSIST-04: text, never JSON), never over existing bytes.
#[cfg(feature = "crate_health")]
fn persist(path: &std::path::Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "FE_EVIDENCE_IO")?;
    f.write_all(text.as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|_| "FE_EVIDENCE_SYNC".into())
}
fn main() {
    #[cfg(feature = "crate_health")]
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(2);
    }
    #[cfg(not(feature = "crate_health"))]
    {
        eprintln!("FE_FULL_REQUIRES_CRATE_HEALTH");
        std::process::exit(2);
    }
}

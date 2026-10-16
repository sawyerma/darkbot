//! Read-only qualification entrypoint. Existing Budget/Health/Sizing, no signer.
#[cfg(feature = "crate_health")]
fn run() -> Result<(), String> {
    use darkbot_runner::analytics::journal::{self, kind, Provenance, Writer};
    use darkbot_runner::orchestration::fe_qualification::{self as fe, ObservedState};
    use darkbot_solana::{provider, runtime_config::Resolved};
    use serde_json::{json, Value};
    use solana_client::rpc_request::RpcRequest;
    use solana_commitment_config::CommitmentConfig;
    use solana_sdk::pubkey::Pubkey;
    use std::{
        fs,
        io::Write,
        path::PathBuf,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    let mut args = std::env::args().skip(1);
    let mut operational = Vec::new();
    let mut execute = false;
    let mut output = None;
    let mut qualification = None;
    let mut replay = None;
    let mut funding_class = "ZERO_PREFUND_SAME_TX".to_string();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--execute" => execute = true,
            "--replay" => {
                replay = Some(
                    args.next()
                        .map(PathBuf::from)
                        .ok_or("FE_REPLAY_DIRECTORY_REQUIRED")?,
                )
            }
            "--funding-class" => funding_class = args.next().ok_or("FE_FUNDING_CLASS_REQUIRED")?,
            "--output" => output = args.next().map(PathBuf::from),
            "--provider-qualification" => qualification = args.next().map(PathBuf::from),
            _ => operational.push(arg),
        }
    }
    if operational.iter().any(|s| s == "--help" || s == "-h") {
        println!("p0-fe-qualify (--execute --provider-qualification FILE | --replay SAVED_DIRECTORY) --output NEW_DIRECTORY --fe-mode full [--funding-class ZERO_PREFUND_SAME_TX|NO_SWAP_INVENTORY] [operational options]\n{}",Resolved::help());
        return Ok(());
    }
    let mut resolved = Resolved::parse(&operational, &|k| std::env::var(k).ok())?;
    if resolved.print {
        println!("{}", resolved.json());
        return Ok(());
    }
    let funding_class = darkbot_runner::orchestration::fe_replay::first_class(&funding_class)?;
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
        let report = darkbot_runner::orchestration::fe_replay::replay(
            &resolved.config,
            &directory,
            funding_class,
        )?;
        fs::create_dir(&output).map_err(|_| "FE_OUTPUT_CREATE")?;
        persist(
            &output.join("operational_config.json"),
            &json!({"requested":resolved.json(),"execution":"OFFLINE_REPLAY_NO_RPC_NO_SIGNER"}),
        )?;
        persist(&output.join("result.json"), &report)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|_| "FE_EVIDENCE_JSON")?
        );
        return Ok(());
    }
    let url = resolved
        .config
        .rpc_url(&|k| std::env::var(k).ok())
        .ok_or("RPC_URL_MISSING")?;
    let requested = resolved.json();
    let l = &mut resolved.config.limits;
    l.max_requests = l.max_requests.min(l.fe_calls);
    l.max_total_mib = l.max_total_mib.min(l.fe_max_mib);
    l.max_response_mib = l.max_response_mib.min(l.max_total_mib);
    l.wss_message_mib = l.wss_message_mib.min(l.max_total_mib);
    let config = resolved.config.clone();
    // Existing FREE_ONLY qualification shape; a configured URL is not capacity evidence.
    let proof: Value = serde_json::from_slice(
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
    // PERSIST-01: configuration, every saved read and the result are frames of one binary
    // journal; each frame is fsynced before the read is decoded or the next one is made.
    let mut evidence =
        Writer::create(&output.join(darkbot_runner::orchestration::fe_replay::FE01_JOURNAL))
            .map_err(|_| "FE_EVIDENCE_IO")?;
    fn record(journal: &mut Writer, kind: u16, v: &Value, slot: Option<u64>) -> Result<(), String> {
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
            .append_legacy(kind, provenance, v)
            .and_then(|_| journal.sync())
            .map(|_| ())
            .map_err(|_| "FE_EVIDENCE_JOURNAL".into())
    }
    fn persist(path: &std::path::Path, v: &Value) -> Result<(), String> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| "FE_EVIDENCE_IO")?;
        serde_json::to_writer_pretty(&mut f, v).map_err(|_| "FE_EVIDENCE_JSON")?;
        f.write_all(b"\n")
            .and_then(|_| f.sync_all())
            .map_err(|_| "FE_EVIDENCE_SYNC".into())
    }
    let operational = json!({"requested":requested,"effective":resolved.json(),"qualification_sha256":fe::sha(&serde_json::to_vec(&proof).unwrap()),"execution":"READ_ONLY_NO_SIGNER"});
    record(&mut evidence, kind::FE01_CONFIG, &operational, None)?;
    persist(&output.join("operational_config.json"), &operational)?;
    resolved.activate()?;
    provider::enable_measurement_budget();
    let deadline = Instant::now() + Duration::from_secs(config.limits.fe_seconds);
    let mut calls = 0u64;
    let mut result = {
        let mut read = |keys: &[Pubkey], floor: u64| -> Result<(u64, ObservedState), String> {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("FE_TIME_BUDGET".into());
            }
            calls += 1;
            let params = json!([keys.iter().map(ToString::to_string).collect::<Vec<_>>(),{"encoding":"base64","commitment":"finalized","minContextSlot":floor}]);
            let rpc = provider::rpc_client(
                url.clone(),
                remaining.min(Duration::from_millis(config.limits.rpc_timeout_ms)),
                CommitmentConfig::finalized(),
                provider::Workload::Bootstrap,
            );
            let response: Result<Value, _> =
                rpc.send(RpcRequest::GetMultipleAccounts, params.clone());
            let entry = match &response {
                Ok(v) => json!({"method":"getMultipleAccounts","params":params,"result":v}),
                Err(_) => {
                    json!({"method":"getMultipleAccounts","params":params,"error":"RPC_FAILURE_REVIEW_PROVIDER_BUDGET"})
                }
            };
            let slot = entry
                .pointer("/result/context/slot")
                .and_then(Value::as_u64);
            record(&mut evidence, kind::FE01_READ, &entry, slot)?;
            if Instant::now() >= deadline {
                return Err("FE_TIME_BUDGET".into());
            }
            fe::decode_batch(response.map_err(|_| "FE_RPC_FAILURE")?, keys, floor)
        };
        fe::collect(&config,&mut read).unwrap_or_else(|e|json!({"schema":"Project0/FE01FullObservation/1","status":"INCOMPLETE","reason":e,"source_parity":"UNKNOWN","fe01":"INCOMPLETE","fe02":"NOT_RUN","pilot_ready":false,"live_enabled":false,"submission_enabled":false}))
    };
    if Instant::now() >= deadline {
        result["status"] = json!("INCOMPLETE");
        result["reason"] = json!("FE_TIME_BUDGET");
    }
    darkbot_runner::orchestration::fe_replay::bind_scope(&mut result, funding_class)?;
    result["logical_reads"] = json!(calls);
    result["provider_actual"] = provider::shared().snapshot();
    record(&mut evidence, kind::FE01_RESULT, &result, None)?;
    evidence
        .seal(result["status"] != "INCOMPLETE")
        .map_err(|_| "FE_EVIDENCE_JOURNAL")?;
    persist(&output.join("result.json"), &result)?;
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    if result["status"] == "INCOMPLETE" {
        return Err("FE_FULL_OBSERVATION_INCOMPLETE_EVIDENCE_SAVED".into());
    }
    Ok(())
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

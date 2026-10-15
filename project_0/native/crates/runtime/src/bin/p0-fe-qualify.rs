//! Read-only qualification entrypoint. Existing Budget/Health/Sizing, no signer.
#[cfg(feature = "crate_health")]
fn run() -> Result<(), String> {
    use darkbot_runner::fe_qualification::{self as fe, ObservedState};
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
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--execute" => execute = true,
            "--output" => output = args.next().map(PathBuf::from),
            "--provider-qualification" => qualification = args.next().map(PathBuf::from),
            _ => operational.push(arg),
        }
    }
    if operational.iter().any(|s| s == "--help" || s == "-h") {
        println!("p0-fe-qualify --execute --output NEW_DIRECTORY --provider-qualification FILE --fe-mode full [operational options]\n{}",Resolved::help());
        return Ok(());
    }
    let mut resolved = Resolved::parse(&operational, &|k| std::env::var(k).ok())?;
    if resolved.print {
        println!("{}", resolved.json());
        return Ok(());
    }
    if !execute || resolved.config.fe_mode != "full" {
        return Err("FE_FULL_REQUIRES_EXPLICIT_EXECUTE_AND_FULL_MODE".into());
    }
    let output = output.ok_or("FE_OUTPUT_REQUIRED")?;
    if output.exists() {
        return Err("FE_OUTPUT_EXISTS_PRESERVE_EVIDENCE".into());
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
    persist(
        &output.join("operational_config.json"),
        &json!({"requested":requested,"effective":resolved.json(),"qualification_sha256":fe::sha(&serde_json::to_vec(&proof).unwrap()),"execution":"READ_ONLY_NO_SIGNER"}),
    )?;
    resolved.activate()?;
    provider::enable_measurement_budget();
    let deadline = Instant::now() + Duration::from_secs(config.limits.fe_seconds);
    let mut calls = 0u64;
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
        let response: Result<Value, _> = rpc.send(RpcRequest::GetMultipleAccounts, params.clone());
        let entry = match &response {
            Ok(v) => json!({"method":"getMultipleAccounts","params":params,"result":v}),
            Err(_) => {
                json!({"method":"getMultipleAccounts","params":params,"error":"RPC_FAILURE_REVIEW_PROVIDER_BUDGET"})
            }
        };
        persist(&output.join(format!("read_{calls:06}.json")), &entry)?;
        if Instant::now() >= deadline {
            return Err("FE_TIME_BUDGET".into());
        }
        fe::decode_batch(response.map_err(|_| "FE_RPC_FAILURE")?, keys, floor)
    };
    let mut result=fe::collect(&config,&mut read).unwrap_or_else(|e|json!({"schema":"Project0/FE01FullObservation/1","status":"INCOMPLETE","reason":e,"source_parity":"UNKNOWN","fe01":"INCOMPLETE","fe02":"NOT_RUN","pilot_ready":false,"live_enabled":false,"submission_enabled":false}));
    if Instant::now() >= deadline {
        result["status"] = json!("INCOMPLETE");
        result["reason"] = json!("FE_TIME_BUDGET");
    }
    result["logical_reads"] = json!(calls);
    result["provider_actual"] = provider::shared().snapshot();
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

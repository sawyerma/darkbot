//! G0.7 controlled value-bearing BSC Mainnet self-launch + BUY qualification.
//!
//! This binary is deliberately fail-closed and is NOT a generic live trading runner.
//! It sends exactly one approved economic attempt: the byte-identical ordered
//! [launch_A, buy_B] pair to 48Club and BlockRazor in parallel. There is no public
//! mempool launch and no automatic retry. The compile-time v2 hash remains the
//! execution authority. Each attempt starts at most once (latch under
//! bsc/.local/g07/), and the bundle expiry comes from the canonical head read
//! immediately before the release.

use darkbot_bsc_execution::{
    acceptance_guard::{LiveGuardContext, verify_live_authorization},
    qualification_capture::CaptureSession,
};
use std::os::unix::fs::OpenOptionsExt;
use std::str::FromStr;
use std::sync::mpsc;

const CAPTURE_WINDOW_NS: u64 = 15_000_000_000;
/// Longest idle time of a prewarmed builder connection before the release.
const MAX_BUILDER_IDLE: Duration = Duration::from_secs(5);
/// Longest wait for the journal writer to append the records of the release.
const CAPTURE_APPEND_TIMEOUT: Duration = Duration::from_millis(50);

mod builder_submission;
mod chain_reads;
mod inputs;
mod shared;
mod transactions;

use builder_submission::*;
use chain_reads::*;
use inputs::*;
use shared::*;
use transactions::*;

fn main() {
    assert_eq!(
        env::var("DARKBOT_G07_MAINNET_SEND").as_deref(),
        Ok("YES"),
        "set DARKBOT_G07_MAINNET_SEND=YES only for the explicitly authorized G0.7 attempt"
    );
    assert!(
        env::var_os("DARKBOT_G07_CAPTURE_JOURNAL").is_none(),
        "DARKBOT_G07_CAPTURE_JOURNAL is retired: the journal path is fixed at {ATTEMPT_STATE_ROOT}/<attempt_id>/{CAPTURE_JOURNAL_FILE}"
    );

    let dry_path = req_env("DARKBOT_G07_DRY_EVIDENCE");
    let baseline_path = req_env("DARKBOT_G07_PREFLIGHT_EVIDENCE");
    let attempt_id = req_env("DARKBOT_G07_ATTEMPT_ID");
    let clock_id = req_env("DARKBOT_G07_CLOCK_ID");
    let boot_id = req_env("DARKBOT_G07_BOOT_ID");
    let producer_witness_contract_path = req_env("DARKBOT_G07_PRODUCER_WITNESS_CONTRACT");
    let builder_policy_path = req_env("DARKBOT_G07_BUILDER_POLICY_EVIDENCE");
    let evidence_path = req_env("DARKBOT_G07_EVIDENCE_PATH");
    // Checked here, before the latch and the guard, not first at framing.
    let blockrazor_auth = req_env("DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION");
    assert!(
        valid_header_value(&blockrazor_auth),
        "invalid BlockRazor Authorization value"
    );
    let rpc_endpoint =
        parse_rpc_endpoint(&req_env("BSC_MAINNET_RPC_HTTPS")).unwrap_or_else(|e| panic!("{e}"));
    validate_attempt_id(&attempt_id).unwrap_or_else(|e| panic!("{e}"));
    validate_evidence_target(&evidence_path)
        .unwrap_or_else(|e| panic!("DARKBOT_G07_EVIDENCE_PATH: {e}"));
    assert_repository_root();
    let (producer_witness_contract, producer_witness_contract_sha256) =
        validate_producer_witness_source(&producer_witness_contract_path, &attempt_id);
    let producer_witness_source =
        req_str(&producer_witness_contract, "/source_description").to_string();
    let producer_witness_source_ready = req_bool(&producer_witness_contract, "/source_ready");

    let (dry, dry_bytes) = load_json(&dry_path);
    let (baseline, baseline_bytes) = load_json(&baseline_path);
    assert_eq!(req_str(&dry, "/status"), "G0.6A_SELF_LAUNCH_BUNDLE_DRY");
    assert!(
        req_bool(&dry, "/dry_pass/all"),
        "DRY-1..DRY-4 are not all PASS"
    );

    let launcher_key = key_from_env("DARKBOT_G07_LAUNCHER_PRIVATE_KEY");
    let buyer_key = key_from_env("DARKBOT_G07_BUYER_PRIVATE_KEY");
    let launcher = signer_address(&launcher_key);
    let buyer = signer_address(&buyer_key);
    assert_ne!(
        launcher, buyer,
        "G0.7 requires distinct launcher and buyer wallets"
    );
    assert_eq!(
        norm(req_str(&dry, "/wallets/launcher")),
        norm(&launcher.to_string())
    );
    assert_eq!(
        norm(req_str(&dry, "/wallets/buyer")),
        norm(&buyer.to_string())
    );
    assert!(req_bool(&dry, "/wallets/distinct"));

    assert_eq!(req_u64(&dry, "/transactions/chain_id"), CHAIN_ID);
    assert_eq!(norm(req_str(&dry, "/transactions/portal")), norm(PORTAL));

    let gas_price = req_u64(&dry, "/transactions/gas_price_wei");
    let (_builder_policy, builder_policy_sha256) =
        validate_builder_policy(&builder_policy_path, gas_price);
    let launch_gas_limit = req_u64(&dry, "/transactions/launch_gas_limit");
    let buy_gas_limit = req_u64(&dry, "/transactions/buy_gas_limit");
    let launch_value = req_u128_string(&dry, "/transactions/launch_value_wei");
    let buy_value = req_u64_string(&dry, "/transactions/buy_value_wei");
    // G0.6a's frozen DRY harness uses the v2 maximum 100-bps policy.
    let slippage_bps = 100u64;
    let salt = B256::from_str(&req_env("DARKBOT_G07_SALT"))
        .expect("DARKBOT_G07_SALT must be the exact 32-byte DRY salt");
    let local_expected_output =
        U256::from_str(req_str(&dry, "/transactions/local_expected_output"))
            .expect("invalid DRY local_expected_output");
    assert!(local_expected_output > U256::ZERO);

    let current = run_current_preflight(
        launcher,
        buyer,
        gas_price,
        launch_gas_limit,
        buy_gas_limit,
        launch_value,
        buy_value,
    );
    let (launcher_nonce, buyer_nonce, preflight_head) =
        validate_current_preflight(&baseline, &current, launcher, buyer, gas_price);
    let preflight_head_hash = req_str(&current, "/latest_block_hash").to_string();
    let impl_addr = Address::from_str(req_str(&current, "/tax_v3_impl"))
        .expect("invalid current TAX_V3 implementation");
    let portal = Address::from_str(PORTAL).unwrap();
    let token = predicted_token(portal, impl_addr, salt);
    assert_eq!(
        norm(req_str(&dry, "/transactions/token")),
        norm(&token.to_string())
    );

    let min_output =
        local_expected_output * U256::from(10_000u64 - slippage_bps) / U256::from(10_000u64);
    assert!(min_output > U256::ZERO);
    assert_eq!(
        req_str(&dry, "/transactions/min_output"),
        min_output.to_string()
    );

    let launch_data = launch_calldata(launcher, salt);
    let buy_data = buy_calldata(token, U256::from(buy_value), min_output);
    let launch = sign_legacy(
        &launcher_key,
        launcher_nonce,
        gas_price,
        launch_gas_limit,
        portal,
        U256::from(launch_value),
        &launch_data,
    );
    let buy = sign_legacy(
        &buyer_key,
        buyer_nonce,
        gas_price,
        buy_gas_limit,
        portal,
        U256::from(buy_value),
        &buy_data,
    );
    let launch_hash = launch.hash.to_string();
    let buy_hash = buy.hash.to_string();
    assert_eq!(
        norm(&launch_hash),
        norm(req_str(&dry, "/transactions/launch_tx_hash")),
        "fresh Mainnet launch bytes do not equal the DRY-qualified launch bytes"
    );
    assert_eq!(
        norm(&buy_hash),
        norm(req_str(&dry, "/transactions/buy_tx_hash")),
        "fresh Mainnet BUY bytes do not equal the DRY-qualified BUY bytes"
    );

    let token_code_before = rpc("eth_getCode", json!([token.to_string(), "latest"]))
        .expect("token-code precheck failed");
    assert_eq!(
        token_code_before.as_str(),
        Some("0x"),
        "predicted token already exists"
    );

    let launch_estimate = hex_u64(
        &rpc(
            "eth_estimateGas",
            json!([{
                "from": launcher.to_string(),
                "to": PORTAL,
                "value": format!("0x{launch_value:x}"),
                "data": format!("0x{}", hex::encode(&launch_data))
            }]),
        )
        .expect("current launch gas estimate failed"),
    );
    assert!(
        launch_estimate <= launch_gas_limit,
        "current launch gas estimate exceeds signed DRY limit"
    );

    let reserved_attempt_wei = launch_value
        .checked_add(buy_value as u128)
        .and_then(|x| {
            x.checked_add((launch_gas_limit as u128 + buy_gas_limit as u128) * gas_price as u128)
        })
        .expect("attempt reserve overflow");

    let attempt_number = env_u64("DARKBOT_G07_ATTEMPT_NUMBER");
    let spent_budget_wei = env_u128("DARKBOT_G07_SPENT_BUDGET_WEI");

    let dry_evidence_sha256 = sha256_hex(&dry_bytes);
    let preflight_evidence_sha256 = sha256_hex(&baseline_bytes);

    // The head poller of the release, exercised here read-only: an endpoint
    // that cannot deliver the next block fresh enough stops the run before the
    // attempt is latched.
    let mut head_poller =
        HeadPoller::connect(&rpc_endpoint).unwrap_or_else(|e| panic!("G0.7 head poller: {e}"));
    let probe = head_poller
        .next_head(HEAD_TRANSITION_TIMEOUT)
        .unwrap_or_else(|e| panic!("G0.7 head probe: {e}"));
    let probe_window = bundle_window(
        preflight_head,
        &preflight_head_hash,
        &probe.head,
        probe.local_unix_ms,
        probe.excess_staleness_ms,
    )
    .unwrap_or_else(|e| panic!("G0.7 head probe: {e}"));

    let attempt_dir = latch_attempt(
        Path::new(ATTEMPT_STATE_ROOT),
        &attempt_id,
        &format!(
            "attempt_id={attempt_id}\npid={}\nunix_time_ms={}\nlaunch_tx_hash={launch_hash}\nbuy_tx_hash={buy_hash}\n",
            std::process::id(),
            unix_time_ms()
        ),
    )
    .unwrap_or_else(|e| panic!("G0.7 single-use latch: {e}"));
    let evidence_out = owner_uid(&attempt_dir)
        .and_then(|uid| EvidenceFile::create(&evidence_path, uid))
        .unwrap_or_else(|e| {
            let released = release_unsent_attempt(&attempt_dir, None, "pre-egress");
            panic!("G0.7 evidence file DARKBOT_G07_EVIDENCE_PATH: {e}; {released}")
        });
    let capture_path = attempt_dir.join(CAPTURE_JOURNAL_FILE);

    let mut capture = CaptureSession::prearm(
        &capture_path,
        &attempt_id,
        &clock_id,
        &boot_id,
        &producer_witness_source,
    )
    .unwrap_or_else(|e| {
        let released = release_unsent_attempt(&attempt_dir, Some(&evidence_out), "pre-egress");
        panic!("G0.7 capture pre-arm failed: {e}; {released}")
    });
    let capture_clock = capture.clock();
    let unsent = Unsent {
        attempt_dir: &attempt_dir,
        evidence: &evidence_out,
    };

    let contract = match verify_live_authorization(
        Path::new(V2_RELATIVE_PATH),
        &LiveGuardContext {
            launcher_address: launcher.to_string(),
            buyer_address: buyer.to_string(),
            chain_id: CHAIN_ID,
            portal_address: PORTAL.to_string(),
            launch_value_wei: launch_value,
            buy_value_wei: buy_value as u128,
            gas_price_wei: gas_price as u128,
            launch_gas_limit,
            buy_gas_limit,
            direct_builder_tip_wei: 0,
            spent_budget_wei,
            reserved_attempt_wei,
            attempt_number,
            dry_pass_all: true,
            capture_prearmed: true,
            producer_witness_contract_bound: true,
            dry_evidence_sha256: dry_evidence_sha256.clone(),
            preflight_evidence_sha256: preflight_evidence_sha256.clone(),
            builder_policy_evidence_sha256: builder_policy_sha256.clone(),
            producer_witness_source_contract_sha256: producer_witness_contract_sha256.clone(),
        },
    ) {
        Ok(contract) => contract,
        Err(e) => {
            let now = capture_clock.now_ns();
            let _ = capture.record_text("AUTHORIZATION_REFUSED", now, &format!("reason={e}"));
            let _ = capture.finish();
            eprintln!("{}", unsent.release("refused"));
            panic!("G0.7 live authorization refused: {e}");
        }
    };

    let evidence_binding = format!(
        "dry_sha256={}\npreflight_sha256={}\nbuilder_policy_sha256={}\nproducer_witness_contract_sha256={}\nlaunch_tx_hash={}\nbuy_tx_hash={}\ntoken={}",
        dry_evidence_sha256,
        preflight_evidence_sha256,
        builder_policy_sha256,
        producer_witness_contract_sha256,
        launch_hash,
        buy_hash,
        token
    );
    // Every stop from here to the release is pre-egress and releases the latch.
    let bound = capture
        .record_text("ATTEMPT_BOUND", capture_clock.now_ns(), &evidence_binding)
        .and_then(|()| {
            capture.record_raw_tx(
                "launch_A",
                capture_clock.now_ns(),
                &launch_hash,
                &launch.raw,
            )
        })
        .and_then(|()| capture.record_raw_tx("buy_B", capture_clock.now_ns(), &buy_hash, &buy.raw));
    if let Err(e) = bound {
        pre_egress_abort(capture, &capture_clock, &unsent, "journal attempt", &e)
    }

    // The evidence of the attempt, completed and written atomically right
    // after the release, after the send threads and after the receipt window.
    // Built here, off the path between the send-head read and the release.
    let mut evidence = json!({
        "schema":"darkbot.g07.mainnet-self-launch-buy.v2",
        "gate":"G0.7",
        "attempt_id":attempt_id,
        "classification":"PROVISIONAL_RELEASED__PROVIDER_RESULTS_PENDING__RECONCILE_G08__NO_AUTOMATIC_RETRY",
        "chain_id":CHAIN_ID,
        "portal":PORTAL,
        "acceptance_contract":{
            "path":V2_RELATIVE_PATH,
            "sha256":contract.sha256
        },
        "evidence_inputs":{
            "dry_path":dry_path,
            "dry_sha256":dry_evidence_sha256,
            "preflight_path":baseline_path,
            "preflight_sha256":preflight_evidence_sha256,
            "builder_policy_path":builder_policy_path,
            "builder_policy_sha256":builder_policy_sha256,
            "producer_witness_contract_path":producer_witness_contract_path,
            "producer_witness_contract_sha256":producer_witness_contract_sha256
        },
        "attempt":{
            "state_dir":attempt_dir.display().to_string(),
            "single_use_latch":attempt_dir.join(ATTEMPT_LATCH_FILE).display().to_string(),
            "number":attempt_number,
            "spent_before_wei":spent_budget_wei.to_string(),
            "reserved_worst_case_wei":reserved_attempt_wei.to_string()
        },
        "wallets":{
            "launcher":launcher.to_string(),
            "buyer":buyer.to_string(),
            "distinct":true,
            "launcher_nonce":launcher_nonce,
            "buyer_nonce":buyer_nonce
        },
        "preflight":{
            "latest_block_number":preflight_head,
            "latest_block_hash":current["latest_block_hash"],
            "portal_version":current["portal_version"],
            "portal_implementation":current["portal_implementation"],
            "tax_v3_impl":current["tax_v3_impl"],
            "network_gas_price_wei":current["gas_price_wei"]
        },
        "transactions":{
            "launch_tx_hash":launch_hash,
            "buy_tx_hash":buy_hash,
            "token":token.to_string(),
            "token_code_before_empty":true,
            "salt":salt.to_string(),
            "launch_value_wei":launch_value.to_string(),
            "buy_value_wei":buy_value.to_string(),
            "gas_price_wei":gas_price,
            "launch_gas_estimate":launch_estimate,
            "launch_gas_limit":launch_gas_limit,
            "buy_gas_limit":buy_gas_limit,
            "local_expected_output":local_expected_output.to_string(),
            "min_output":min_output.to_string(),
            "slippage_bps":slippage_bps
        },
        "capture":{
            "complete_local_journal":false,
            "status":"PROVIDER_RESULTS_PENDING",
            "journal_path":capture_path.display().to_string()
        },
        "capture_window":null,
        "timing":{
            "T0_to_T_buy":"UNKNOWN_PENDING_ORIGINAL_PRODUCER_WITNESS",
            "producer_witness_source":producer_witness_source,
            "producer_witness_source_ready":producer_witness_source_ready,
            "rule":"Local send/receipt/block timestamps are not producer execution timestamps. G0.8 producer-timing evaluation requires the original producing execution attempt; source_ready=false does not block G0.7 acquisition qualification."
        },
        "next_required_gate":"G0.8",
        "critical_rule":"DO NOT RETRY THIS ATTEMPT AUTOMATICALLY; reconcile canonical execution first."
    });

    // The RPC connection is checked (and re-established if it closed during
    // the guard phase) before the builder prewarm, so the builder connections
    // only idle for the wait for the next block.
    if let Err(e) = head_poller.latest() {
        pre_egress_abort(capture, &capture_clock, &unsent, "head poller", &e)
    }
    // TLS/DNS/handshake all happen before the economic write boundary.
    let mut club48_tls = match connect_tls(CLUB48_HOST) {
        Ok(stream) => stream,
        Err(e) => pre_egress_abort(capture, &capture_clock, &unsent, "48Club TLS prewarm", &e),
    };
    // The 48Club connection is the older one: it bounds the idle time.
    let club48_ready_at = Instant::now();
    let mut blockrazor_tls = match connect_tls(BLOCKRAZOR_HOST) {
        Ok(stream) => stream,
        Err(e) => pre_egress_abort(
            capture,
            &capture_clock,
            &unsent,
            "BlockRazor TLS prewarm",
            &e,
        ),
    };

    if let Err(e) = capture.record_text(
        "TRANSPORTS_PREWARMED",
        capture_clock.now_ns(),
        &format!("preflight_head={preflight_head}\npreflight_head_hash={preflight_head_hash}"),
    ) {
        pre_egress_abort(capture, &capture_clock, &unsent, "journal prewarm", &e)
    }

    // The send head is the next block, read as soon as the endpoint has it:
    // the bundle expiry comes from it, not from the preflight head read seconds
    // earlier.
    let fresh = match head_poller.next_head(HEAD_TRANSITION_TIMEOUT) {
        Ok(observation) => observation,
        Err(e) => pre_egress_abort(capture, &capture_clock, &unsent, "send head read", &e),
    };
    let window = match bundle_window(
        preflight_head,
        &preflight_head_hash,
        &fresh.head,
        fresh.local_unix_ms,
        fresh.excess_staleness_ms,
    ) {
        Ok(window) => window,
        Err(e) => pre_egress_abort(capture, &capture_clock, &unsent, "send head window", &e),
    };
    let send_head = fresh.head.number;
    let max_block = window.max_block;
    let launch_hex = format!("0x{}", hex::encode(&launch.raw));
    let buy_hex = format!("0x{}", hex::encode(&buy.raw));
    let request_id = buy_hash.clone();

    let txs = [launch_hex.as_str(), buy_hex.as_str()];
    let club48_body = club48_bundle_body(&request_id, txs, max_block, window.max_timestamp);
    let blockrazor_body = blockrazor_bundle_body(&buy_hash, txs, max_block);

    let club48_request = http_request(CLUB48_HOST, &club48_body, None);
    let blockrazor_request =
        http_request(BLOCKRAZOR_HOST, &blockrazor_body, Some(&blockrazor_auth));
    drop(blockrazor_auth);

    let journaled = capture
        .record_text(
            "FRESH_HEAD_BOUND",
            capture_clock.now_ns(),
            &format!(
                "send_head={send_head}\nsend_head_hash={}\nsend_head_milli_timestamp={}\nlocal_unix_ms={}\nhead_age_ms={}\nexcess_staleness_ms={}\nhead_read_ns={}\nmax_block={max_block}\nfortyeightclub_max_timestamp={}\npreflight_head={preflight_head}",
                fresh.head.hash,
                fresh.head.milli_timestamp,
                fresh.local_unix_ms,
                window.head_age_ms,
                fresh.excess_staleness_ms,
                fresh.read_ns,
                window.max_timestamp
            ),
        )
        .and_then(|()| {
            [
                (
                    "48Club/Puissant",
                    club48_body.as_str(),
                    club48_request.len(),
                ),
                (
                    "BlockRazor",
                    blockrazor_body.as_str(),
                    blockrazor_request.len(),
                ),
            ]
            .into_iter()
            .try_for_each(|(provider, body, request_len)| {
                capture.record_text(
                    "SUBMISSION_PREPARED",
                    capture_clock.now_ns(),
                    &format!(
                        "provider={provider}\nbody_sha256={}\nrequest_len={}\nauthorization_persisted=false\nbody={body}",
                        sha256_hex(body.as_bytes()),
                        request_len
                    ),
                )
            })
        })
        // The exact bodies are on file before any of their bytes leaves.
        .and_then(|()| capture.wait_appended(CAPTURE_APPEND_TIMEOUT));
    if let Err(e) = journaled {
        pre_egress_abort(capture, &capture_clock, &unsent, "journal submission", &e)
    }
    if club48_ready_at.elapsed() > MAX_BUILDER_IDLE {
        let idle = format!(
            "builder connections idled {} ms (limit {} ms)",
            club48_ready_at.elapsed().as_millis(),
            MAX_BUILDER_IDLE.as_millis()
        );
        pre_egress_abort(capture, &capture_clock, &unsent, "builder idle", &idle)
    }
    // A builder that closed its idle connection would swallow the bundle unseen.
    for (stage, stream) in [
        ("48Club liveness", &mut club48_tls),
        ("BlockRazor liveness", &mut blockrazor_tls),
    ] {
        if let Err(e) = transport_alive(stream) {
            pre_egress_abort(capture, &capture_clock, &unsent, stage, &e)
        }
    }

    // From the release on nothing may panic: the evidence of a value-bearing
    // attempt must be written whatever happens. Each sender waits for a go
    // signal before the barrier: if the other cannot be spawned, dropping the
    // signal ends it without a write and the run stops before the release.
    let barrier = Arc::new(Barrier::new(3));
    let mut release_marker = String::new();
    let mut release_evidence: Result<(), String> = Err("not written".into());
    let mut release_before_ns = 0u64;
    let mut release_after_ns = 0u64;
    let sent = thread::scope(|scope| {
        let (go48, wait48) = mpsc::channel::<()>();
        let (gobr, waitbr) = mpsc::channel::<()>();
        let b48 = Arc::clone(&barrier);
        let bbr = Arc::clone(&barrier);
        let c48_clock = capture_clock.clone();
        let br_clock = capture_clock.clone();
        let h48 = thread::Builder::new()
            .name("g07-send-48club".into())
            .spawn_scoped(scope, move || {
                wait48.recv().ok().map(|()| {
                    submit_provider(
                        "48Club/Puissant",
                        club48_tls,
                        club48_request,
                        b48,
                        c48_clock,
                    )
                })
            });
        let hbr = thread::Builder::new()
            .name("g07-send-blockrazor".into())
            .spawn_scoped(scope, move || {
                waitbr.recv().ok().map(|()| {
                    submit_provider(
                        "BlockRazor",
                        blockrazor_tls,
                        blockrazor_request,
                        bbr,
                        br_clock,
                    )
                })
            });
        let (h48, hbr) = match (h48, hbr) {
            (Ok(h48), Ok(hbr)) => (h48, hbr),
            (h48, hbr) => {
                drop((go48, gobr));
                let reasons: Vec<String> = [h48.err(), hbr.err()]
                    .into_iter()
                    .flatten()
                    .map(|e| e.to_string())
                    .collect();
                return Err(format!("send thread spawn failed: {}", reasons.join("; ")));
            }
        };
        // Each receiver lives in its spawned closure until that closure has
        // received, so neither send can fail; should the first fail anyway,
        // nothing has been released.
        if go48.send(()).is_err() {
            drop(gobr);
            return Err("48Club send thread ended before its go signal".to_string());
        }
        let _ = gobr.send(());
        release_before_ns = capture_clock.now_ns();
        barrier.wait();
        release_after_ns = capture_clock.now_ns();
        // Provisional evidence first, while the senders run (main is idle
        // here), so a kill or hang before the builders answer still leaves it
        // on file; then the RELEASED marker (the latch is already durable).
        set_fields(
            &mut evidence,
            vec![
                (
                    "bundle",
                    json!({
                        "tx_order":["launch_A","buy_B"],
                        "identical_raw_transactions_across_builders":true,
                        "send_head":send_head,
                        "max_block":max_block,
                        "fortyeightclub_maxTimestamp":window.max_timestamp,
                        "expiry_rule":format!("maxBlockNumber = send_head + {MAX_BLOCK_OFFSET} for both builders (the head+{MAX_BLOCK_OFFSET} bound); 48Club maxTimestamp = floor((send_head milliTimestamp + {MAX_BLOCK_OFFSET} * {BLOCK_INTERVAL_MS} ms) / 1000) + 1, the second after head+{MAX_BLOCK_OFFSET}'s nominal second: not a head+{MAX_BLOCK_OFFSET} bound by itself, it keeps a maxTimestamp-versus-current-time check from refusing the bundle while head+{MAX_BLOCK_OFFSET} is open"),
                        "send_head_source":{
                            "read":"first eth_getBlockByNumber(latest) showing a new block, polled over a pre-warmed keep-alive TLS connection after the builder TLS prewarm, immediately before the release",
                            "hash":fresh.head.hash,
                            "milli_timestamp":fresh.head.milli_timestamp,
                            "local_unix_ms_after_read":fresh.local_unix_ms,
                            "head_age_ms":window.head_age_ms,
                            "excess_staleness_ms":fresh.excess_staleness_ms,
                            "read_round_trip_ns":fresh.read_ns,
                            "max_head_age_ms":MAX_HEAD_AGE_MS,
                            "max_local_clock_lag_ms":MAX_LOCAL_CLOCK_LAG_MS,
                            "preflight_head":preflight_head,
                            "probe":{
                                "head":probe.head.number,
                                "head_age_ms":probe_window.head_age_ms,
                                "excess_staleness_ms":probe.excess_staleness_ms,
                                "read_round_trip_ns":probe.read_ns
                            }
                        },
                        "public_mempool_launch":false,
                        "automatic_retry":false,
                        "fortyeightclub_noMerge":true,
                        "fortyeightclub_positionFirst":true,
                        "blockrazor_noMerge":false,
                        "blockrazor_positionFirst":false
                    }),
                ),
                (
                    "local_send_boundaries",
                    json!({
                        "clock_id":clock_id,
                        "boot_id":boot_id,
                        "release_before_ns":release_before_ns,
                        "release_after_ns":release_after_ns,
                        "release_marker":"PENDING",
                        "fortyeightclub":{"status":"PENDING"},
                        "blockrazor":{"status":"PENDING"},
                        "any_valid_bytes_may_have_left":true,
                        "not_t0_to_buy":true
                    }),
                ),
            ],
        );
        release_evidence = write_evidence(&evidence_out, &evidence);
        release_marker = mark_released(&attempt_dir, release_after_ns);
        Ok((
            h48.join()
                .ok()
                .flatten()
                .unwrap_or_else(|| panicked_provider_result("48Club/Puissant")),
            hbr.join()
                .ok()
                .flatten()
                .unwrap_or_else(|| panicked_provider_result("BlockRazor")),
        ))
    });
    let (club48_result, blockrazor_result) = match sent {
        Ok(results) => results,
        Err(e) => pre_egress_abort(capture, &capture_clock, &unsent, "send threads", &e),
    };

    for result in [&club48_result, &blockrazor_result] {
        let body = format!(
            "provider={}\nwrite_started_ns={:?}\nwrite_flushed_ns={:?}\nbytes_may_have_left={}\nhttp_status={:?}\nresponse_body={}\nerror={:?}",
            result.provider,
            result.write_started_ns,
            result.write_flushed_ns,
            result.bytes_may_have_left,
            result.http_status.as_deref(),
            result.response_body.as_deref().unwrap_or(""),
            result.error.as_deref()
        );
        let _ = capture.record_text("PROVIDER_RESULT", capture_clock.now_ns(), &body);
    }

    let _ = capture.record_text(
        "BUNDLE_RELEASE_BRACKET",
        capture_clock.now_ns(),
        &format!("release_before_ns={release_before_ns}\nrelease_after_ns={release_after_ns}"),
    );

    let both_flushed =
        club48_result.write_flushed_ns.is_some() && blockrazor_result.write_flushed_ns.is_some();
    let any_possible_egress =
        club48_result.bytes_may_have_left || blockrazor_result.bytes_may_have_left;
    let flush_spread_ns = match (
        club48_result.write_flushed_ns,
        blockrazor_result.write_flushed_ns,
    ) {
        (Some(a), Some(b)) => Some(a.abs_diff(b)),
        _ => None,
    };

    set_fields(
        &mut evidence,
        vec![
            (
                "classification",
                json!(
                    "PROVISIONAL_RELEASED__RECEIPT_WINDOW_OPEN__RECONCILE_G08__NO_AUTOMATIC_RETRY"
                ),
            ),
            (
                "capture",
                json!({
                    "complete_local_journal":false,
                    "status":"RECEIPT_WINDOW_OPEN",
                    "journal_path":capture_path.display().to_string()
                }),
            ),
            (
                "local_send_boundaries",
                json!({
                    "clock_id":clock_id,
                    "boot_id":boot_id,
                    "release_before_ns":release_before_ns,
                    "release_after_ns":release_after_ns,
                    "release_marker":release_marker,
                    "fortyeightclub":provider_json(&club48_result),
                    "blockrazor":provider_json(&blockrazor_result),
                    "both_flush_completed":both_flushed,
                    "write_flush_boundary_spread_ns":flush_spread_ns,
                    "any_valid_bytes_may_have_left":any_possible_egress,
                    "not_t0_to_buy":true
                }),
            ),
        ],
    );
    // Provisional evidence with the provider results, so a kill or a hang in
    // the receipt window still leaves it on file.
    let provisional = write_evidence(&evidence_out, &evidence);
    let _ = capture.record_text(
        "PROVISIONAL_EVIDENCE",
        capture_clock.now_ns(),
        &format!(
            "path={evidence_path}\nat_release={release_evidence:?}\nafter_send_threads={provisional:?}"
        ),
    );
    let outcome = |written: &Result<(), String>| match written {
        Ok(()) => "written".to_string(),
        Err(e) => format!("NOT written ({e})"),
    };
    let _ = writeln!(
        std::io::stderr().lock(),
        "G0.7: bundle released; evidence at {evidence_path}: at the release {}, after the send threads {}; polling receipts for up to 30 s and keeping the journal open at least 15 s: do not interrupt",
        outcome(&release_evidence),
        outcome(&provisional)
    );

    // Keep the local journal alive conservatively. This is NOT T0 timing authority.
    // If both receipts become observable, retain at least another 15 seconds after
    // that observation. Canonical [T0,T0+15s] backfill is done by G0.8 from the
    // launch block, and strict sub-ms timing still requires the original producer.
    let receipt_poll_deadline = release_after_ns.saturating_add(30_000_000_000);
    let mut launch_receipt_observer = None;
    let mut buy_receipt_observer = None;
    let mut both_receipts_observed_ns = None;
    while capture_clock.now_ns() < receipt_poll_deadline {
        if launch_receipt_observer.as_ref().is_none_or(Value::is_null) {
            launch_receipt_observer =
                rpc("eth_getTransactionReceipt", json!([launch_hash.clone()])).ok();
        }
        if buy_receipt_observer.as_ref().is_none_or(Value::is_null) {
            buy_receipt_observer = rpc("eth_getTransactionReceipt", json!([buy_hash.clone()])).ok();
        }
        let launch_seen = launch_receipt_observer
            .as_ref()
            .is_some_and(|v| !v.is_null());
        let buy_seen = buy_receipt_observer.as_ref().is_some_and(|v| !v.is_null());
        if launch_seen && buy_seen {
            both_receipts_observed_ns = Some(capture_clock.now_ns());
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    let coverage_end_ns = both_receipts_observed_ns
        .map(|seen| seen.saturating_add(CAPTURE_WINDOW_NS))
        .unwrap_or(receipt_poll_deadline)
        .max(release_after_ns.saturating_add(CAPTURE_WINDOW_NS));
    while capture_clock.now_ns() < coverage_end_ns {
        thread::sleep(Duration::from_millis(20));
    }

    let post_window_head = rpc("eth_blockNumber", json!([])).ok().and_then(|v| {
        v.as_str()
            .and_then(|h| u64::from_str_radix(h.trim_start_matches("0x"), 16).ok())
    });
    let _ = capture.record_text(
        "LOCAL_CAPTURE_WINDOW_CLOSED",
        capture_clock.now_ns(),
        &format!(
            "release_before_ns={release_before_ns}\nrelease_after_ns={release_after_ns}\nminimum_post_observation_window_ns={CAPTURE_WINDOW_NS}\nboth_receipts_observed_ns={both_receipts_observed_ns:?}\ncoverage_end_ns={coverage_end_ns}\npost_window_head={post_window_head:?}\nlaunch_receipt_observer={}\nbuy_receipt_observer={}",
            launch_receipt_observer
                .as_ref()
                .map(Value::to_string)
                .unwrap_or_else(|| "UNAVAILABLE".into()),
            buy_receipt_observer
                .as_ref()
                .map(Value::to_string)
                .unwrap_or_else(|| "UNAVAILABLE".into())
        ),
    );

    let capture_summary = capture.finish();
    let capture_complete = capture_summary
        .as_ref()
        .map(|s| s.complete_local_journal)
        .unwrap_or(false);
    let capture_json = match &capture_summary {
        Ok(s) => json!({
            "journal_path": s.path.display().to_string(),
            "records": s.records,
            "first_sequence": s.first_sequence,
            "last_sequence": s.last_sequence,
            "bytes": s.bytes,
            "sha256": s.sha256_hex,
            "next_sequence": s.next_sequence,
            "enqueued": s.enqueued,
            "dropped_full": s.dropped_full,
            "rejected_oversize": s.rejected_oversize,
            "disconnected": s.disconnected,
            "complete_local_journal": s.complete_local_journal,
            "producer_witness_source": producer_witness_source.clone(),
            "producer_witness_contract_path": producer_witness_contract_path.clone(),
            "producer_witness_contract_sha256": producer_witness_contract_sha256.clone(),
            "builder_policy_path": builder_policy_path.clone(),
            "builder_policy_sha256": builder_policy_sha256.clone(),
            "producer_witness_source_ready": producer_witness_source_ready,
            "producer_witness_required_for_t0_to_buy": true
        }),
        Err(e) => json!({
            "complete_local_journal": false,
            "error": e,
            "producer_witness_source": producer_witness_source.clone(),
            "producer_witness_contract_path": producer_witness_contract_path.clone(),
            "producer_witness_contract_sha256": producer_witness_contract_sha256.clone(),
            "builder_policy_path": builder_policy_path.clone(),
            "builder_policy_sha256": builder_policy_sha256.clone(),
            "producer_witness_source_ready": producer_witness_source_ready,
            "producer_witness_required_for_t0_to_buy": true
        }),
    };

    set_fields(
        &mut evidence,
        vec![
            (
                "classification",
                json!(if capture_complete {
                    "SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY"
                } else {
                    "STOP_CAPTURE_INCOMPLETE__RECONCILE_G08__NO_AUTOMATIC_RETRY"
                }),
            ),
            ("capture", capture_json),
            (
                "capture_window",
                json!({
                    "both_receipts_observed_ns":both_receipts_observed_ns,
                    "coverage_end_ns":coverage_end_ns,
                    // G0.7 never reconciles the canonical window; G0.8 does.
                    "canonical_t0_plus_15s_reconciled_in_g08":false,
                    "canonical_t0_plus_15s_reconciliation":"PENDING_G08"
                }),
            ),
        ],
    );
    // File first (atomic replacement): stdout may be a closed pipe. Nothing
    // here may panic.
    let evidence_written = write_evidence(&evidence_out, &evidence);
    let _ = writeln!(std::io::stdout().lock(), "{}", evidence_text(&evidence));
    if let Err(e) = evidence_written {
        let _ = writeln!(
            std::io::stderr().lock(),
            "STOP: G0.7 evidence: {e} ({evidence_path}); stdout holds a copy; capture_complete={capture_complete}"
        );
        std::process::exit(3);
    }
    if !capture_complete {
        std::process::exit(2);
    }
}

fn evidence_text(evidence: &Value) -> String {
    serde_json::to_string_pretty(evidence)
        .unwrap_or_else(|e| format!("{{\"evidence_serialization_error\":\"{e}\"}}"))
}

fn set_fields(evidence: &mut Value, fields: Vec<(&str, Value)>) {
    if let Some(map) = evidence.as_object_mut() {
        for (key, value) in fields {
            map.insert(key.to_string(), value);
        }
    }
}

#[derive(Clone, Copy)]
enum G08Field {
    Text,
    Integer,
    Flag,
    Decimal,
}

/// The G0.7 evidence fields G0.8 reads
/// (crates/qualification/src/bin/g08_mainnet_verify.rs), with the type it
/// requires; a test keeps this list equal to G0.8's reads.
const G08_READS: [(&str, G08Field); 12] = [
    ("/schema", G08Field::Text),
    ("/chain_id", G08Field::Integer),
    ("/portal", G08Field::Text),
    ("/capture/complete_local_journal", G08Field::Flag),
    ("/transactions/launch_tx_hash", G08Field::Text),
    ("/transactions/buy_tx_hash", G08Field::Text),
    ("/transactions/token", G08Field::Text),
    ("/wallets/launcher", G08Field::Text),
    ("/wallets/buyer", G08Field::Text),
    ("/transactions/min_output", G08Field::Decimal),
    ("/transactions/buy_value_wei", G08Field::Decimal),
    ("/attempt_id", G08Field::Text),
];

/// Whether G0.8 can read this evidence: every field it reads present with
/// the type it requires, and the schema, chain and portal it asserts.
fn g08_readable(evidence: &Value) -> Result<(), String> {
    for (pointer, kind) in G08_READS {
        let value = evidence.pointer(pointer);
        let ok = match kind {
            G08Field::Text => value.is_some_and(Value::is_string),
            G08Field::Integer => value.and_then(Value::as_u64).is_some(),
            G08Field::Flag => value.is_some_and(Value::is_boolean),
            G08Field::Decimal => value
                .and_then(Value::as_str)
                .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit())),
        };
        if !ok {
            return Err(format!("G0.8 field {pointer} missing or of the wrong type"));
        }
    }
    let schema = evidence.pointer("/schema").and_then(Value::as_str);
    let chain = evidence.pointer("/chain_id").and_then(Value::as_u64);
    let portal = evidence
        .pointer("/portal")
        .and_then(Value::as_str)
        .map(norm);
    if schema != Some("darkbot.g07.mainnet-self-launch-buy.v2")
        || chain != Some(CHAIN_ID)
        || portal.as_deref() != Some(PORTAL)
    {
        return Err("G0.8 schema, chain or portal mismatch".into());
    }
    Ok(())
}

/// Atomically writes the evidence after checking that G0.8 can read it; an
/// unreadable evidence is still written, and both failures stay visible.
/// Never panics.
fn write_evidence(out: &EvidenceFile, evidence: &Value) -> Result<(), String> {
    let readable = g08_readable(evidence);
    let written = out.replace(format!("{}\n", evidence_text(evidence)).as_bytes());
    match (readable, written) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(r), Ok(())) => Err(format!("written, but not readable by G0.8: {r}")),
        (Ok(()), Err(w)) => Err(w),
        (Err(r), Err(w)) => Err(format!("{w}; not readable by G0.8 either: {r}")),
    }
}

/// Marks the attempt directory right after the release (off the critical
/// path): its latch must never be released by hand. Never panics.
fn mark_released(attempt_dir: &Path, release_after_ns: u64) -> String {
    let path = attempt_dir.join(RELEASED_MARKER_FILE);
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut f| {
            f.write_all(
                format!(
                    "bundle released; bytes may have left\nrelease_after_ns={release_after_ns}\n"
                )
                .as_bytes(),
            )
            .and_then(|()| f.sync_all())
        })
        .map_err(|e| e.to_string())
        .and_then(|()| sync_dir(attempt_dir));
    match written {
        Ok(()) => path.display().to_string(),
        Err(e) => format!("RELEASED marker failed: {}: {e}", path.display()),
    }
}

/// What a stop before the release hands back: the attempt's latch directory
/// and this run's evidence file.
struct Unsent<'a> {
    attempt_dir: &'a Path,
    evidence: &'a EvidenceFile,
}

impl Unsent<'_> {
    fn release(&self, outcome: &str) -> String {
        release_unsent_attempt(self.attempt_dir, Some(self.evidence), outcome)
    }
}

/// Terminal stop after the guard but before any bundle byte was written: the
/// journal is closed and the latch released (the attempt state is kept).
fn pre_egress_abort(
    mut capture: CaptureSession,
    clock: &CaptureClock,
    unsent: &Unsent,
    stage: &str,
    reason: &str,
) -> ! {
    let _ = capture.record_text(
        "PRE_EGRESS_ABORT",
        clock.now_ns(),
        &format!("stage={stage}\nreason={reason}\nbundle_bytes_written=false"),
    );
    let _ = capture.finish();
    let released = unsent.release("pre-egress");
    panic!("G0.7 {stage} failed; no valid bundle write attempted: {reason}; {released}");
}

#[cfg(test)]
mod tests {
    use super::*;

    const G08_SOURCE: &str =
        include_str!("../../../../qualification/src/bin/g08_mainnet_verify.rs");

    fn sample() -> Value {
        json!({
            "schema":"darkbot.g07.mainnet-self-launch-buy.v2",
            "chain_id":CHAIN_ID,
            "portal":PORTAL,
            "attempt_id":"g07-test",
            "capture":{"complete_local_journal":false},
            "transactions":{
                "launch_tx_hash":"0x01",
                "buy_tx_hash":"0x02",
                "token":"0xe51c4deb1038afcd95ef12d4a649cdbcf6fa7777",
                "min_output":"1",
                "buy_value_wei":"1000000000000"
            },
            "wallets":{
                "launcher":"0x3e7b487ae5e1c49ab1cf6222bdb5cf016e86aacd",
                "buyer":"0x9c8a0db41914beee52dd49790c4f087e252743de"
            }
        })
    }

    #[test]
    fn the_checked_fields_are_exactly_what_g08_reads() {
        let mut reads: Vec<&str> = G08_SOURCE
            .split("(&g07, \"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .collect();
        reads.sort_unstable();
        reads.dedup();
        let mut checked: Vec<&str> = G08_READS.iter().map(|(pointer, _)| *pointer).collect();
        checked.sort_unstable();
        assert_eq!(reads, checked);
        // G0.8 reads the G0.7 evidence only through those calls.
        assert!(!G08_SOURCE.contains("g07.pointer(") && !G08_SOURCE.contains("g07["));
    }

    #[test]
    fn g08_readable_requires_every_read_field_with_its_type() {
        assert!(g08_readable(&sample()).is_ok());
        for (pointer, _) in G08_READS {
            let mut broken = sample();
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            let holder = if parent.is_empty() {
                Some(&mut broken)
            } else {
                broken.pointer_mut(parent)
            };
            holder.unwrap().as_object_mut().unwrap().remove(key);
            assert!(g08_readable(&broken).is_err(), "{pointer}");
        }
        for (pointer, wrong) in [
            ("/chain_id", json!(97)),
            ("/schema", json!("darkbot.g07.mainnet-self-launch-buy.v1")),
            (
                "/portal",
                json!("0x0000000000000000000000000000000000000000"),
            ),
            ("/transactions/min_output", json!(1)),
            ("/transactions/buy_value_wei", json!("1e12")),
            ("/capture/complete_local_journal", json!("false")),
        ] {
            let mut broken = sample();
            *broken.pointer_mut(pointer).unwrap() = wrong;
            assert!(g08_readable(&broken).is_err(), "{pointer}");
        }
    }
}

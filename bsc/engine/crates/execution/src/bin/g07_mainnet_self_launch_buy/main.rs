//! G0.7 controlled value-bearing BSC Mainnet self-launch + BUY qualification.
//!
//! This binary is deliberately fail-closed and is NOT a generic live trading runner.
//! It sends exactly one approved economic attempt: the byte-identical ordered
//! [launch_A, buy_B] pair to 48Club and BlockRazor in parallel. There is no public
//! mempool launch and no automatic retry. The compile-time v5 hash remains the
//! execution authority. Each attempt starts at most once (latch under
//! bsc/.local/g07/), and the bundle expiry comes from the canonical head read
//! immediately before the release.
//!
//! Every input, the contract and the evidence are binary DBREC001 records;
//! JSON appears only on the external JSON-RPC wire (chain_reads,
//! builder_submission), decoded there into typed values.

use darkbot_bsc_execution::{
    acceptance_guard::{LiveGuardContext, V5_RELATIVE_PATH, verify_live_authorization},
    qualification_capture::{CaptureSession, CaptureSummary},
};
use darkbot_bsc_record::Value as RecordValue;
use schema::g07::evidence;
use std::os::unix::fs::OpenOptionsExt;
use std::str::FromStr;
use std::sync::mpsc;

const CAPTURE_WINDOW_NS: u64 = 15_000_000_000;
/// Longest idle time of a prewarmed builder connection before the release.
const MAX_BUILDER_IDLE: Duration = Duration::from_secs(5);
/// Longest wait for the journal writer to append the records of the release.
const CAPTURE_APPEND_TIMEOUT: Duration = Duration::from_millis(50);

mod builder_submission;
// Shared with g07_candidate and readiness; the runner takes only the
// selected, verified candidate and its salt from it.
#[allow(dead_code)]
#[path = "../g07_candidate/candidate.rs"]
mod candidate;
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
    let (producer_witness_contract, producer_witness_hashes) =
        validate_producer_witness_source(&producer_witness_contract_path, &attempt_id);
    let producer_witness_proof =
        darkbot_bsc_execution::acceptance_guard::WitnessSourceProof::check(
            &fs::read(&producer_witness_contract_path)
                .expect("re-read the producer witness source"),
            &attempt_id,
            PRODUCER_WITNESS_SCHEMA_PATH,
        )
        .unwrap_or_else(|e| panic!("{e}"));
    let producer_witness_source =
        field(producer_witness_contract.text(schema::g07::witness_source::SOURCE_DESCRIPTION))
            .to_string();
    let producer_witness_source_ready =
        field(producer_witness_contract.bool(schema::g07::witness_source::SOURCE_READY));

    use schema::g06a::dry as d;
    // The requalification DRY's native records (contract v5).
    let (dry, dry_hashes) = load_record(&dry_path, &[&d::NATIVE]);
    let (baseline, baseline_hashes) =
        load_record(&baseline_path, &[&schema::g06a::preflight::SCHEMA]);
    assert_eq!(field(dry.text(d::STATUS)), "G0.6A_SELF_LAUNCH_BUNDLE_DRY");
    // The guard's DRY-1..DRY-4 proof is made from the DRY bytes themselves.
    let dry_pass = darkbot_bsc_execution::acceptance_guard::DryPassProof::check(
        &fs::read(&dry_path).expect("re-read the DRY record"),
    )
    .expect("individual DRY checks failed");

    // SAFETY: standalone main is still single-threaded. Above are only local
    // environment/filesystem/record checks; no TLS, subprocess, capture worker
    // or head poller has started. Remove key ENV before any of those boundaries.
    let (launcher_key, buyer_key) = unsafe { take_startup_keys() };
    let launcher = signer_address(&launcher_key);
    let buyer = signer_address(&buyer_key);
    assert_ne!(
        launcher, buyer,
        "G0.7 requires distinct launcher and buyer wallets"
    );
    assert_eq!(Address::from(field(dry.address(d::LAUNCHER))), launcher);
    assert_eq!(Address::from(field(dry.address(d::BUYER))), buyer);
    assert!(field(dry.bool(d::DISTINCT)));

    let portal = Address::from_str(PORTAL).unwrap();
    assert_eq!(field(dry.u64(d::CHAIN_ID)), CHAIN_ID);
    assert_eq!(Address::from(field(dry.address(d::PORTAL))), portal);

    let gas_price = field(dry.u64(d::GAS_PRICE_WEI));
    let (builder_policy, builder_policy_hashes) =
        validate_builder_policy(&builder_policy_path, gas_price);
    let launch_gas_limit = field(dry.u64(d::LAUNCH_GAS_LIMIT));
    let buy_gas_limit = field(dry.u64(d::BUY_GAS_LIMIT));
    let launch_value = u128::try_from(U256::from_be_bytes(field(dry.u256(d::LAUNCH_VALUE_WEI))))
        .expect("DRY launch value exceeds u128");
    let buy_value = u64::try_from(U256::from_be_bytes(field(dry.u256(d::BUY_VALUE_WEI))))
        .expect("DRY BUY value exceeds u64");
    // G0.6a's frozen DRY harness uses the v2 maximum 100-bps policy.
    let slippage_bps = 100u64;
    // The salt is the selected candidate's, verified from the private store;
    // B's never comes from the environment, A's env value may only repeat it.
    let (selected, selected_salt) =
        candidate::selected_launch(Path::new(candidate::CANDIDATE_STATE_ROOT))
            .unwrap_or_else(|e| panic!("G0.7 candidate: {e}"));
    candidate::check_env_salt(
        &selected,
        &selected_salt,
        env::var("DARKBOT_G07_SALT").ok().as_deref(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let salt = selected_salt.launch_word();
    let local_expected_output = U256::from_be_bytes(field(dry.u256(d::LOCAL_EXPECTED_OUTPUT)));
    assert!(local_expected_output > U256::ZERO);

    // The Rust port of the read-only preflight: typed RPC reads, no helper
    // process and no JSON file.
    let current = run_current_preflight(
        launcher,
        buyer,
        gas_price,
        launch_gas_limit,
        buy_gas_limit,
        launch_value,
        buy_value,
    )
    .unwrap_or_else(|e| panic!("Mainnet read-only preflight failed: {e}"));
    let (launcher_nonce, buyer_nonce, preflight_head) =
        validate_current_preflight(&baseline, &current, launcher, buyer, gas_price);
    use schema::g06a::preflight as pf;
    let preflight_head_hash = format!(
        "0x{}",
        darkbot_bsc_record::hex(&field(current.b256(pf::LATEST_BLOCK_HASH)))
    );
    let impl_addr = Address::from(field(current.address(pf::TAX_V3_IMPL)));
    let token = predicted_token(portal, impl_addr, salt);
    assert_eq!(Address::from(field(dry.address(d::TOKEN))), token);
    assert_eq!(
        selected.token, token,
        "predicted token differs from the selected candidate"
    );
    // v9.1 C1/R1: the v5-bound evidence and launch-path snapshot, re-read at
    // the current preflight block; any code or selector drift stops before
    // signing and the latch.
    let (further, bound_launch_path) =
        darkbot_bsc_execution::acceptance_guard::verify_further_v5_evidence(
            Path::new("."),
            &fs::read(V5_RELATIVE_PATH).expect("read the v5 contract"),
        )
        .unwrap_or_else(|e| panic!("G0.7 v5 evidence: {e}"));
    let launch_path_current = match current_launch_path(&bound_launch_path, &current)
        .unwrap_or_else(|e| panic!("G0.7 launch-path re-read: {e}"))
    {
        darkbot_bsc_execution::launch_path::Current::Unchanged(proof) => proof,
        darkbot_bsc_execution::launch_path::Current::Drift(drift) => panic!(
            "G0.7 launch path drifted from the v5-bound snapshot: {}",
            drift.join("; ")
        ),
    };

    let min_output =
        local_expected_output * U256::from(10_000u64 - slippage_bps) / U256::from(10_000u64);
    assert!(min_output > U256::ZERO);
    assert_eq!(
        U256::from_be_bytes(field(dry.u256(d::MIN_OUTPUT))),
        min_output
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
        launch.hash,
        B256::from(field(dry.b256(d::LAUNCH_TX_HASH))),
        "fresh Mainnet launch bytes do not equal the DRY-qualified launch bytes"
    );
    assert_eq!(
        buy.hash,
        B256::from(field(dry.b256(d::BUY_TX_HASH))),
        "fresh Mainnet BUY bytes do not equal the DRY-qualified BUY bytes"
    );

    let token_code_before = rpc_code(token).expect("token-code precheck failed");
    assert!(
        token_code_before.is_empty(),
        "predicted token already exists"
    );

    // SELF_B: no external call may carry its launch calldata before the
    // release; the v5-bound local EVM simulation is the gas evidence and its
    // absence or failure STOPs. SELF_A keeps the historical estimate.
    let launch_estimate = match selected.profile {
        candidate::Profile::A => {
            rpc_estimate_gas(launcher, portal, U256::from(launch_value), &launch_data)
                .expect("current launch gas estimate failed")
        }
        candidate::Profile::B => further.local_launch_gas_used,
    };
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
    // BSC-003: the bound DRY's bundle projection is the compiled semantics.
    darkbot_bsc_execution::bundle_wire::check_projection(&dry)
        .unwrap_or_else(|e| panic!("G0.7 bundle semantics: {e}"));
    // BSC-004: the bound builder policy is fresh at the release boundary.
    darkbot_bsc_execution::builder_policy::require_fresh(&builder_policy, unix_time_ms())
        .unwrap_or_else(|e| panic!("G0.7 builder policy: {e}"));

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
        Path::new(V5_RELATIVE_PATH),
        &LiveGuardContext {
            attempt_id: attempt_id.clone(),
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
            dry_pass: dry_pass.clone(),
            capture: capture.prearmed_proof().ok(),
            producer_witness: Some(producer_witness_proof.clone()),
            dry_evidence: dry_hashes.clone(),
            preflight_evidence: baseline_hashes.clone(),
            builder_policy_evidence: builder_policy_hashes.clone(),
            producer_witness_source: producer_witness_hashes.clone(),
            candidate_profile: selected.profile.name().to_string(),
            candidate_id: hex::encode(selected.id),
            launch_path_current: Some(launch_path_current.clone()),
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
        "contract_sha256={}\nlaunch_path_record_sha256={}\nlaunch_path_verified_block_hash={}\ndry_record_sha256={}\ndry_legacy_sha256={}\npreflight_record_sha256={}\npreflight_legacy_sha256={}\nbuilder_policy_record_sha256={}\nbuilder_policy_legacy_sha256={}\nproducer_witness_source_record_sha256={}\nproducer_witness_source_legacy_sha256={}\nlaunch_tx_hash={}\nbuy_tx_hash={}\ntoken={}",
        contract.sha256,
        hex::encode(launch_path_current.record_sha256()),
        hex::encode(launch_path_current.block_hash()),
        dry_hashes.record_sha256,
        dry_hashes.legacy_sha256.as_deref().unwrap_or("none"),
        baseline_hashes.record_sha256,
        baseline_hashes.legacy_sha256.as_deref().unwrap_or("none"),
        builder_policy_hashes.record_sha256,
        builder_policy_hashes
            .legacy_sha256
            .as_deref()
            .unwrap_or("none"),
        producer_witness_hashes.record_sha256,
        producer_witness_hashes
            .legacy_sha256
            .as_deref()
            .unwrap_or("none"),
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
    let mut evidence = Draft::new(&evidence::SCHEMA);
    evidence
        .set(evidence::GATE, "G0.7")
        .set(evidence::ATTEMPT_ID, attempt_id.as_str())
        .set(
            evidence::CLASSIFICATION,
            "PROVISIONAL_RELEASED__PROVIDER_RESULTS_PENDING__RECONCILE_G08__NO_AUTOMATIC_RETRY",
        )
        .set(evidence::CHAIN_ID, CHAIN_ID)
        .set(evidence::PORTAL, RecordValue::Address(portal.into_array()))
        .set(evidence::NEXT_REQUIRED_GATE, "G0.8")
        .set(
            evidence::CRITICAL_RULE,
            "DO NOT RETRY THIS ATTEMPT AUTOMATICALLY; reconcile canonical execution first.",
        )
        .set(evidence::CONTRACT_PATH, V5_RELATIVE_PATH)
        .set(evidence::CONTRACT_SHA256, word(&contract.sha256))
        .set(evidence::DRY_PATH, dry_path.as_str())
        .set(evidence::DRY_SHA256, word(&dry_hashes.record_sha256))
        .set_opt(evidence::DRY_LEGACY_SHA256, opt_word(&dry_hashes.legacy_sha256))
        .set(evidence::PREFLIGHT_PATH, baseline_path.as_str())
        .set(evidence::PREFLIGHT_SHA256, word(&baseline_hashes.record_sha256))
        .set_opt(
            evidence::PREFLIGHT_LEGACY_SHA256,
            opt_word(&baseline_hashes.legacy_sha256),
        )
        .set(evidence::BUILDER_POLICY_PATH, builder_policy_path.as_str())
        .set(evidence::BUILDER_POLICY_SHA256, word(&builder_policy_hashes.record_sha256))
        .set_opt(
            evidence::BUILDER_POLICY_LEGACY_SHA256,
            opt_word(&builder_policy_hashes.legacy_sha256),
        )
        .set(evidence::WITNESS_SOURCE_PATH, producer_witness_contract_path.as_str())
        .set(evidence::WITNESS_SOURCE_SHA256, word(&producer_witness_hashes.record_sha256))
        .set_opt(
            evidence::WITNESS_SOURCE_LEGACY_SHA256,
            opt_word(&producer_witness_hashes.legacy_sha256),
        )
        .set(evidence::STATE_DIR, attempt_dir.display().to_string())
        .set(
            evidence::SINGLE_USE_LATCH,
            attempt_dir.join(ATTEMPT_LATCH_FILE).display().to_string(),
        )
        .set(evidence::ATTEMPT_NUMBER, attempt_number)
        .set(evidence::SPENT_BEFORE_WEI, RecordValue::u128(spent_budget_wei))
        .set(evidence::RESERVED_WORST_CASE_WEI, RecordValue::u128(reserved_attempt_wei))
        .set(evidence::LAUNCHER, RecordValue::Address(launcher.into_array()))
        .set(evidence::BUYER, RecordValue::Address(buyer.into_array()))
        .set(evidence::DISTINCT, true)
        .set(evidence::LAUNCHER_NONCE, launcher_nonce)
        .set(evidence::BUYER_NONCE, buyer_nonce)
        // The whole current preflight, as read before the attempt.
        .set(evidence::PREFLIGHT, RecordValue::Nested(current.fields().to_vec()))
        .set(evidence::LAUNCH_TX_HASH, RecordValue::B256(launch.hash.0))
        .set(evidence::BUY_TX_HASH, RecordValue::B256(buy.hash.0))
        .set(evidence::TOKEN, RecordValue::Address(token.into_array()))
        .set(evidence::TOKEN_CODE_BEFORE_EMPTY, true)
        .set(evidence::SALT, RecordValue::B256(salt.0))
        .set(evidence::LAUNCH_VALUE_WEI, RecordValue::u128(launch_value))
        .set(evidence::BUY_VALUE_WEI, RecordValue::u128(u128::from(buy_value)))
        .set(evidence::GAS_PRICE_WEI, gas_price)
        .set(evidence::LAUNCH_GAS_ESTIMATE, launch_estimate)
        .set(evidence::LAUNCH_GAS_LIMIT, launch_gas_limit)
        .set(evidence::BUY_GAS_LIMIT, buy_gas_limit)
        .set(
            evidence::LOCAL_EXPECTED_OUTPUT,
            RecordValue::U256(local_expected_output.to_be_bytes()),
        )
        .set(evidence::MIN_OUTPUT, RecordValue::U256(min_output.to_be_bytes()))
        .set(evidence::SLIPPAGE_BPS, slippage_bps)
        .set(evidence::CAPTURE_COMPLETE, false)
        .set(evidence::CAPTURE_STATUS, "PROVIDER_RESULTS_PENDING")
        .set(evidence::JOURNAL_PATH, capture_path.display().to_string())
        .set(evidence::T0_TO_T_BUY, "UNKNOWN_PENDING_ORIGINAL_PRODUCER_WITNESS")
        .set(evidence::WITNESS_SOURCE_DESCRIPTION, producer_witness_source.as_str())
        .set(evidence::WITNESS_SOURCE_READY, producer_witness_source_ready)
        .set(
            evidence::TIMING_RULE,
            "Local send/receipt/block timestamps are not producer execution timestamps. G0.8 producer-timing evaluation requires the original producing execution attempt; source_ready=false does not block G0.7 acquisition qualification.",
        );

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
    let Some(send_head_hash) = hex_word(&fresh.head.hash) else {
        pre_egress_abort(
            capture,
            &capture_clock,
            &unsent,
            "send head hash",
            &fresh.head.hash,
        )
    };
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
    let club48_body_sha256 = darkbot_bsc_record::sha256(club48_body.as_bytes());
    let blockrazor_body_sha256 = darkbot_bsc_record::sha256(blockrazor_body.as_bytes());

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
            // The typed request, not its wire body: the body is the shared
            // template of exactly these fields and the raw transactions
            // journaled above, so its SHA-256 identifies it.
            [
                (
                    "48Club/Puissant",
                    &club48_body_sha256,
                    club48_body.len(),
                    club48_request.len(),
                    Some(window.max_timestamp),
                ),
                (
                    "BlockRazor",
                    &blockrazor_body_sha256,
                    blockrazor_body.len(),
                    blockrazor_request.len(),
                    None,
                ),
            ]
            .into_iter()
            .try_for_each(|(provider, body_sha256, body_len, request_len, max_timestamp)| {
                capture.record_text(
                    "SUBMISSION_PREPARED",
                    capture_clock.now_ns(),
                    &format!(
                        "provider={provider}\nrequest_id={request_id}\ntx_order=launch_A,buy_B\nmax_block={max_block}\nmax_timestamp={max_timestamp:?}\nbody_sha256={}\nbody_len={body_len}\nrequest_len={request_len}\nauthorization_persisted=false",
                        darkbot_bsc_record::hex(body_sha256)
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
        let mut bundle = Draft::new(&evidence::bundle::SCHEMA);
        {
            use evidence::bundle as b;
            bundle
                .push(b::TX_ORDER, "launch_A")
                .push(b::TX_ORDER, "buy_B")
                .set(b::IDENTICAL_RAW_TXS, true)
                .set(b::SEND_HEAD, send_head)
                .set(b::MAX_BLOCK, max_block)
                .set(b::FORTYEIGHTCLUB_MAX_TIMESTAMP, window.max_timestamp)
                .set(b::EXPIRY_RULE, format!("maxBlockNumber = send_head + {MAX_BLOCK_OFFSET} for both builders (the head+{MAX_BLOCK_OFFSET} bound); 48Club maxTimestamp = floor((send_head milliTimestamp + {MAX_BLOCK_OFFSET} * {BLOCK_INTERVAL_MS} ms) / 1000) + 1, the second after head+{MAX_BLOCK_OFFSET}'s nominal second: not a head+{MAX_BLOCK_OFFSET} bound by itself, it keeps a maxTimestamp-versus-current-time check from refusing the bundle while head+{MAX_BLOCK_OFFSET} is open"))
                .set(b::SEND_HEAD_READ, "first eth_getBlockByNumber(latest) showing a new block, polled over a pre-warmed keep-alive TLS connection after the builder TLS prewarm, immediately before the release")
                .set(b::SEND_HEAD_HASH, RecordValue::B256(send_head_hash))
                .set(b::SEND_HEAD_MILLI_TIMESTAMP, fresh.head.milli_timestamp)
                .set(b::LOCAL_UNIX_MS_AFTER_READ, fresh.local_unix_ms)
                .set(b::HEAD_AGE_MS, window.head_age_ms)
                .set(b::EXCESS_STALENESS_MS, fresh.excess_staleness_ms)
                .set(b::READ_ROUND_TRIP_NS, fresh.read_ns)
                .set(b::MAX_HEAD_AGE_MS, MAX_HEAD_AGE_MS)
                .set(b::MAX_LOCAL_CLOCK_LAG_MS, MAX_LOCAL_CLOCK_LAG_MS)
                .set(b::PREFLIGHT_HEAD, preflight_head)
                .set(b::PROBE_HEAD, probe.head.number)
                .set(b::PROBE_HEAD_AGE_MS, probe_window.head_age_ms)
                .set(b::PROBE_EXCESS_STALENESS_MS, probe.excess_staleness_ms)
                .set(b::PROBE_READ_ROUND_TRIP_NS, probe.read_ns)
                .set(b::PUBLIC_MEMPOOL_LAUNCH, false)
                .set(b::AUTOMATIC_RETRY, false)
                .set(b::FORTYEIGHTCLUB_NO_MERGE, true)
                .set(b::FORTYEIGHTCLUB_POSITION_FIRST, true)
                .set(b::BLOCKRAZOR_NO_MERGE, false)
                .set(b::BLOCKRAZOR_POSITION_FIRST, false)
                .set(b::FORTYEIGHTCLUB_BODY_SHA256, RecordValue::B256(club48_body_sha256))
                .set(b::FORTYEIGHTCLUB_BODY_LEN, club48_body.len() as u64)
                .set(b::BLOCKRAZOR_BODY_SHA256, RecordValue::B256(blockrazor_body_sha256))
                .set(b::BLOCKRAZOR_BODY_LEN, blockrazor_body.len() as u64);
        }
        evidence.set(evidence::BUNDLE, bundle.into_nested()).set(
            evidence::SEND,
            send_boundaries(
                &clock_id,
                &boot_id,
                release_before_ns,
                release_after_ns,
                "PENDING",
                None,
            ),
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
        let answer = result.answer.as_ref();
        let body = format!(
            "provider={}\nwrite_started_ns={:?}\nwrite_flushed_ns={:?}\nbytes_may_have_left={}\nhttp_status_line={:?}\nresponse_sha256={:?}\nresponse_len={:?}\nrpc_result_hash={:?}\nrpc_result_text={:?}\nrpc_error_code={:?}\nrpc_error_message={:?}\nresponse_undecodable={:?}\nerror={:?}",
            result.provider,
            result.write_started_ns,
            result.write_flushed_ns,
            result.bytes_may_have_left,
            answer.map(|a| a.status_line.as_str()),
            answer.map(|a| darkbot_bsc_record::hex(&a.body_sha256)),
            answer.map(|a| a.body_len),
            answer
                .and_then(|a| a.result_hash)
                .map(|h| darkbot_bsc_record::hex(&h)),
            answer.and_then(|a| a.result_text.as_deref()),
            answer.and_then(|a| a.error_code),
            answer.and_then(|a| a.error_message.as_deref()),
            answer.and_then(|a| a.undecodable.as_deref()),
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

    evidence
        .set(
            evidence::CLASSIFICATION,
            "PROVISIONAL_RELEASED__RECEIPT_WINDOW_OPEN__RECONCILE_G08__NO_AUTOMATIC_RETRY",
        )
        .set(evidence::CAPTURE_STATUS, "RECEIPT_WINDOW_OPEN")
        .set(
            evidence::SEND,
            send_boundaries(
                &clock_id,
                &boot_id,
                release_before_ns,
                release_after_ns,
                &release_marker,
                Some(SendResults {
                    fortyeightclub: &club48_result,
                    blockrazor: &blockrazor_result,
                    both_flushed,
                    flush_spread_ns,
                    any_possible_egress,
                }),
            ),
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
    let mut launch_receipt_observer: Option<ReceiptSeen> = None;
    let mut buy_receipt_observer: Option<ReceiptSeen> = None;
    let mut both_receipts_observed_ns = None;
    while capture_clock.now_ns() < receipt_poll_deadline {
        if launch_receipt_observer.is_none() {
            launch_receipt_observer = rpc_receipt(&launch_hash).ok().flatten();
        }
        if buy_receipt_observer.is_none() {
            buy_receipt_observer = rpc_receipt(&buy_hash).ok().flatten();
        }
        if launch_receipt_observer.is_some() && buy_receipt_observer.is_some() {
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

    let post_window_head = rpc_block_number().ok();
    let _ = capture.record_text(
        "LOCAL_CAPTURE_WINDOW_CLOSED",
        capture_clock.now_ns(),
        &format!(
            "release_before_ns={release_before_ns}\nrelease_after_ns={release_after_ns}\nminimum_post_observation_window_ns={CAPTURE_WINDOW_NS}\nboth_receipts_observed_ns={both_receipts_observed_ns:?}\ncoverage_end_ns={coverage_end_ns}\npost_window_head={post_window_head:?}\nlaunch_receipt_observer={launch_receipt_observer:?}\nbuy_receipt_observer={buy_receipt_observer:?}"
        ),
    );

    let capture_summary = capture.finish();
    let capture_complete = capture_summary
        .as_ref()
        .map(|s| s.complete_local_journal)
        .unwrap_or(false);
    match &capture_summary {
        Ok(s) => {
            evidence
                .set(evidence::CAPTURE_STATUS, "CLOSED")
                .set(evidence::JOURNAL_PATH, s.path.display().to_string());
            match journal_summary(s) {
                Some(journal) => evidence.set(evidence::JOURNAL, journal),
                None => evidence.set(
                    evidence::CAPTURE_ERROR,
                    format!("journal SHA-256 is not 32 bytes: {}", s.sha256_hex),
                ),
            };
        }
        Err(e) => {
            evidence
                .set(evidence::CAPTURE_STATUS, "FAILED")
                .set(evidence::CAPTURE_ERROR, bounded(e, 4096));
        }
    }
    evidence
        .set(evidence::CAPTURE_COMPLETE, capture_complete)
        .set(
            evidence::CLASSIFICATION,
            if capture_complete {
                "SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY"
            } else {
                "STOP_CAPTURE_INCOMPLETE__RECONCILE_G08__NO_AUTOMATIC_RETRY"
            },
        )
        .set_opt(
            evidence::BOTH_RECEIPTS_OBSERVED_NS,
            both_receipts_observed_ns,
        )
        .set(evidence::COVERAGE_END_NS, coverage_end_ns)
        // G0.7 never reconciles the canonical window; G0.8 does.
        .set(evidence::CANONICAL_WINDOW_RECONCILED_IN_G08, false)
        .set(evidence::CANONICAL_WINDOW_RECONCILIATION, "PENDING_G08")
        .set(
            evidence::LAUNCH_RECEIPT_OBSERVED,
            launch_receipt_observer.is_some(),
        )
        .set(
            evidence::BUY_RECEIPT_OBSERVED,
            buy_receipt_observer.is_some(),
        )
        .set_opt(evidence::POST_WINDOW_HEAD, post_window_head);
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

/// The derived text view of the evidence for stdout (never parsed back).
/// Never panics.
fn evidence_text(evidence: &Draft) -> String {
    match evidence
        .encode()
        .and_then(|bytes| darkbot_bsc_record::decode(&evidence::SCHEMA, &bytes))
    {
        Ok(record) => record.render(),
        Err(e) => format!("EVIDENCE_NOT_ENCODABLE: {e}\n{evidence:?}"),
    }
}

/// The provider results of the release, after the send threads.
struct SendResults<'a> {
    fortyeightclub: &'a ProviderResult,
    blockrazor: &'a ProviderResult,
    both_flushed: bool,
    flush_spread_ns: Option<u64>,
    any_possible_egress: bool,
}

/// The local send boundaries of the release as a nested evidence record:
/// pending at the release, completed with the provider results after the
/// send threads. Never panics.
fn send_boundaries(
    clock_id: &str,
    boot_id: &str,
    release_before_ns: u64,
    release_after_ns: u64,
    release_marker: &str,
    results: Option<SendResults>,
) -> RecordValue {
    use evidence::send as b;
    let mut d = Draft::new(&b::SCHEMA);
    d.set(b::CLOCK_ID, bounded(clock_id, 4096))
        .set(b::BOOT_ID, bounded(boot_id, 4096))
        .set(b::RELEASE_BEFORE_NS, release_before_ns)
        .set(b::RELEASE_AFTER_NS, release_after_ns)
        .set(b::RELEASE_MARKER, bounded(release_marker, 4096))
        .set(b::NOT_T0_TO_BUY, true);
    match results {
        None => {
            d.set(b::PROVIDER_RESULTS_PENDING, true)
                .set(b::ANY_BYTES_MAY_HAVE_LEFT, true);
        }
        Some(r) => {
            d.set(b::PROVIDER_RESULTS_PENDING, false)
                .set(b::FORTYEIGHTCLUB, provider_record(r.fortyeightclub))
                .set(b::BLOCKRAZOR, provider_record(r.blockrazor))
                .set(b::BOTH_FLUSH_COMPLETED, r.both_flushed)
                .set_opt(b::FLUSH_SPREAD_NS, r.flush_spread_ns)
                .set(b::ANY_BYTES_MAY_HAVE_LEFT, r.any_possible_egress);
        }
    }
    d.into_nested()
}

/// The closed capture journal as a nested evidence record; `None` if its
/// SHA-256 is not a 32-byte hex word. Never panics.
fn journal_summary(s: &CaptureSummary) -> Option<RecordValue> {
    use evidence::journal as j;
    let sha256 = hex_word(&s.sha256_hex)?;
    let mut d = Draft::new(&j::SCHEMA);
    d.set(j::RECORDS, s.records)
        .set_opt(j::FIRST_SEQUENCE, s.first_sequence)
        .set_opt(j::LAST_SEQUENCE, s.last_sequence)
        .set(j::BYTES, s.bytes)
        .set(j::SHA256, RecordValue::B256(sha256))
        .set(j::NEXT_SEQUENCE, s.next_sequence)
        .set(j::ENQUEUED, s.enqueued)
        .set(j::DROPPED_FULL, s.dropped_full)
        .set(j::REJECTED_OVERSIZE, s.rejected_oversize)
        .set(j::DISCONNECTED, s.disconnected);
    Some(d.into_nested())
}

/// A 32-byte word from hex with or without `0x`. Never panics.
fn hex_word(text: &str) -> Option<[u8; 32]> {
    hex::decode(text.trim_start_matches("0x"))
        .ok()
        .and_then(|b| <[u8; 32]>::try_from(b).ok())
}

/// Atomically writes the evidence record after checking that it decodes as
/// the schema G0.8 reads. An encode failure preserves the last durable
/// binary record and reports the error. Never panics.
fn write_evidence(out: &EvidenceFile, evidence: &Draft) -> Result<(), String> {
    let encoded = evidence
        .encode()
        .and_then(|bytes| darkbot_bsc_record::decode(&evidence::SCHEMA, &bytes).map(|_| bytes));
    match encoded {
        Ok(bytes) => out.replace(&bytes),
        Err(e) => Err(format!(
            "not a record readable by G0.8; previous binary evidence preserved: {e}"
        )),
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

/// A SHA-256 in hex as a record word; read before the attempt is latched.
fn word(sha256: &str) -> RecordValue {
    RecordValue::B256(hex_word(sha256).unwrap_or_else(|| panic!("not a SHA-256: {sha256}")))
}

/// An optional SHA-256 (a legacy hash exists only for an audited import).
fn opt_word(sha256: &Option<String>) -> Option<RecordValue> {
    sha256.as_deref().map(word)
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_bsc_record::{Kind, Presence};
    use std::os::unix::fs::DirBuilderExt;

    const G08_SOURCE: &str =
        include_str!("../../../../qualification/src/bin/g08_mainnet_verify.rs");
    const PREFLIGHT_RECORD: &[u8] = include_bytes!(
        "../../../../../../native/evidence/g06a/g07_binary_import_20261016/preflight.dbrec"
    );

    /// The G0.7 evidence fields G0.8 reads.
    const G08_READS: [(&str, u16); 11] = [
        ("ATTEMPT_ID", evidence::ATTEMPT_ID),
        ("CHAIN_ID", evidence::CHAIN_ID),
        ("PORTAL", evidence::PORTAL),
        ("CAPTURE_COMPLETE", evidence::CAPTURE_COMPLETE),
        ("LAUNCH_TX_HASH", evidence::LAUNCH_TX_HASH),
        ("BUY_TX_HASH", evidence::BUY_TX_HASH),
        ("TOKEN", evidence::TOKEN),
        ("LAUNCHER", evidence::LAUNCHER),
        ("BUYER", evidence::BUYER),
        ("MIN_OUTPUT", evidence::MIN_OUTPUT),
        ("BUY_VALUE_WEI", evidence::BUY_VALUE_WEI),
    ];

    /// Every required field set (the nested preflight from the import).
    fn sample() -> Draft {
        let preflight =
            darkbot_bsc_record::decode(&schema::g06a::preflight::SCHEMA, PREFLIGHT_RECORD).unwrap();
        let mut d = Draft::new(&evidence::SCHEMA);
        for f in evidence::SCHEMA
            .fields
            .iter()
            .filter(|f| f.presence == Presence::Required)
        {
            let value = match f.kind {
                Kind::Bool => RecordValue::Bool(false),
                Kind::U64 => RecordValue::U64(1),
                Kind::I64 => RecordValue::I64(-1),
                Kind::U256 => RecordValue::u128(1),
                Kind::Address => RecordValue::Address([1; 20]),
                Kind::B256 => RecordValue::B256([2; 32]),
                Kind::Bytes => RecordValue::Bytes(vec![3]),
                Kind::Text => RecordValue::Text("x".into()),
                Kind::Nested => RecordValue::Nested(preflight.fields().to_vec()),
            };
            d.set(f.tag, value);
        }
        d
    }

    fn private_dir(tag: &str) -> PathBuf {
        let dir = fs::canonicalize(env::temp_dir())
            .unwrap()
            .join(format!("darkbot-g07-main-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        dir
    }

    #[test]
    fn the_checked_fields_are_exactly_what_g08_reads() {
        let mut reads: Vec<&str> = G08_SOURCE
            .split("(evidence::")
            .skip(1)
            .filter_map(|rest| rest.split(')').next())
            .collect();
        reads.sort_unstable();
        reads.dedup();
        let mut checked: Vec<&str> = G08_READS.iter().map(|(name, _)| *name).collect();
        checked.sort_unstable();
        assert_eq!(reads, checked);
        // A record that decodes always holds them: each is required.
        for (name, tag) in G08_READS {
            let f = evidence::SCHEMA.field(tag).unwrap();
            assert_eq!(f.presence, Presence::Required, "{name}");
        }
        assert!(G08_SOURCE.contains("decode(&evidence::SCHEMA"));
    }

    #[test]
    fn evidence_round_trips_and_needs_every_required_field() {
        let mut d = sample();
        let answered = ProviderResult {
            provider: "48Club/Puissant",
            write_started_ns: Some(1),
            write_flushed_ns: Some(2),
            bytes_may_have_left: true,
            answer: Some(decode_builder_answer(
                "HTTP/1.1 200 OK",
                br#"{"jsonrpc":"2.0","id":"0x1","error":{"code":-32000,"message":"nonce too high"}}"#,
            )),
            error: None,
        };
        let panicked = panicked_provider_result("BlockRazor");
        d.set(
            evidence::SEND,
            send_boundaries(
                "clock",
                "boot",
                1,
                2,
                "marker",
                Some(SendResults {
                    fortyeightclub: &answered,
                    blockrazor: &panicked,
                    both_flushed: false,
                    flush_spread_ns: None,
                    any_possible_egress: true,
                }),
            ),
        );
        let record = darkbot_bsc_record::decode(&evidence::SCHEMA, &d.encode().unwrap()).unwrap();
        let send = record.nested(evidence::SEND).unwrap();
        let club48 = send.nested(evidence::send::FORTYEIGHTCLUB).unwrap();
        use schema::g07::provider_result as p;
        assert_eq!(club48.i64(p::RPC_ERROR_CODE).unwrap(), -32000);
        assert_eq!(club48.text(p::RPC_ERROR_MESSAGE).unwrap(), "nonce too high");
        assert!(
            evidence_text(&d)
                .contains("local_send_boundaries.fortyeightclub.http_status_code = 200")
        );
        for f in evidence::SCHEMA
            .fields
            .iter()
            .filter(|f| f.presence == Presence::Required)
        {
            let mut broken = sample();
            broken.clear(f.tag);
            assert!(broken.encode().is_err(), "{}", f.name);
        }
    }

    #[test]
    fn evidence_encode_failure_preserves_the_last_binary_record() {
        let dir = private_dir("write");
        let path = dir.join("g07.dbrec");
        let out = EvidenceFile::create(path.to_str().unwrap(), owner_uid(&dir).unwrap()).unwrap();
        write_evidence(&out, &sample()).unwrap();
        let written = fs::read(&path).unwrap();
        assert!(darkbot_bsc_record::decode(&evidence::SCHEMA, &written).is_ok());
        let mut broken = sample();
        broken.clear(evidence::TOKEN);
        let e = write_evidence(&out, &broken).unwrap_err();
        assert!(e.contains("not a record readable by G0.8"), "{e}");
        assert_eq!(fs::read(&path).unwrap(), written);
        fs::remove_dir_all(&dir).unwrap();
    }
}

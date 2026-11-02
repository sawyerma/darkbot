//! Plan 28.10. package 8: the synthetic, guaranteed non-sending LIVE E2E.
//!
//! An isolated owner state is activated through the real `bsc_runtime` CLI
//! (switches, modes, limits, the LIVE qualification from synthetic evidence,
//! the grant of a confirmed scope); the runtime's LIVE path arms on a
//! synthetic deployment probe, a funded synthetic buyer key and C2's nonce;
//! a recorded launch is replayed through the production ingress core (as the
//! WSS adapter delivers verified full transactions); the BUY, the approval
//! and the SELL go only to a builder stand-in on 127.0.0.1; synthetic
//! canonical blocks carry their receipts (gas, the Portal's trades); the
//! ledger closes the position with its exact net result. Every missing
//! precondition blocks with its reason and nothing reaches the stand-in.
//! No key, endpoint or network of the real system is used: the route is the
//! loopback adapter (127.0.0.1 only), no follower or WSS adapter runs.

use alloy_primitives::{Address, B256, U256, hex, keccak256};
use darkbot_bsc_execution::canonical_join::Venue;
use darkbot_bsc_execution::chain_follower::{
    CanonicalEvent, CanonicalTrade, CanonicalTx, FollowerLimits, TxCost,
};
use darkbot_bsc_execution::ingress::{IngressCore, Launch, Observation};
use darkbot_bsc_execution::paper_exec::{
    ExecConfig, FnClock, IntentKind, SenderPaths, StrategyCurve,
};
use darkbot_bsc_execution::runtime::exec::ExecSetup;
use darkbot_bsc_execution::runtime::live::{
    LiveRouteConfig, LiveRuntime, LiveSetup, PositionChange,
};
use darkbot_bsc_execution::runtime::state::{StateConfig, StateOwner};
use darkbot_bsc_execution::runtime_strategy::StrategyAction;
use darkbot_bsc_execution::sources::SourceId;
use darkbot_bsc_execution::venue_probe::{
    CodeFingerprint, ObservedDeploymentData, VenueObservation,
};
use darkbot_bsc_record::schema::bsc_runtime::runtime_status::live as l;
use darkbot_bsc_record::{Draft, Kind, Presence, Schema, Value};
use darkbot_bsc_trading::chain_state::engine::StateLimits;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    HoldingCap, NativeBuyInputs, NativeSellInputs, PricedToken, WITNESSED_MAINNET_TERMS,
    profile_terms, quote_native_buy, quote_native_sell,
};
use darkbot_bsc_trading::signer::Signer;
use darkbot_bsc_trading::transactions::legacy::LegacyTx;
use darkbot_bsc_trading::venues::flap::abi::{LaunchParamsV6, encode_new_token_v6};
use darkbot_bsc_trading::venues::flap::deployment::{
    FlapDeployment, behaviour_reference, bsc_mainnet_v5_27_2,
};
use std::cell::Cell;
use std::fs;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const HEAD: u64 = 129_900_000;
const INPUT: u64 = 1_000_000_000_000_000;
const GAS_PRICE: u64 = 50_000_000;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bsc-live-e2e-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(&d)
        .unwrap();
    d
}

/// Every required field of `s` with a fixed value: a synthetic record.
fn filled(s: &'static Schema) -> Draft {
    let mut d = Draft::new(s);
    for f in s.fields.iter().filter(|f| f.presence == Presence::Required) {
        let v = match f.kind {
            Kind::Bool => Value::Bool(false),
            Kind::U64 => Value::U64(1),
            Kind::I64 => Value::I64(1),
            Kind::U256 => Value::U256([0; 32]),
            Kind::Address => Value::Address([1; 20]),
            Kind::B256 => Value::B256([2; 32]),
            Kind::Bytes => Value::Bytes(Vec::new()),
            Kind::Text => Value::Text("synthetic".into()),
            Kind::Nested => match f.nested {
                Some(n) => filled(n).into_nested(),
                None => continue,
            },
        };
        d.set(f.tag, v);
    }
    d
}

/// The synthetic buyer key's secret ([`buyer_key`]).
const BUYER_SECRET: [u8; 32] = [0x44; 32];

/// Synthetic typed evidence under `root`/`ev` (BSC3-049): the G0.1 freeze
/// of the pinned deployment, a G0.4 verdict `CONTINUE` in whose census
/// PublicNode delivered first, a G0.7 attempt of the synthetic buyer of
/// that deployment accepted by Club48, its G0.8 result of `verdict`, and one
/// measurement of no particular role.
fn write_evidence(root: &Path, ev: &str, verdict: &str) {
    use darkbot_bsc_record::schema::bsc_census::{census, verdict as gv};
    use darkbot_bsc_record::schema::bsc_flap_freeze::flap_freeze as ff;
    use darkbot_bsc_record::schema::bsc_flap_impl_parity::flap_impl_parity as fp;
    use darkbot_bsc_record::schema::{bsc_runtime, g06a, g07, g08};
    fs::create_dir_all(root.join(ev)).unwrap();
    fs::create_dir_all(root.join("bsc/config")).unwrap();
    let pin = bsc_mainnet_v5_27_2();
    let portal = Value::Address(pin.portal.into_array());
    let implementation = Value::Address(pin.portal_implementation.into_array());
    let mut upgrade = filled(&ff::upgrade_check::SCHEMA);
    upgrade
        .set(ff::upgrade_check::STATUS, "ONE_IMPLEMENTATION")
        .set(
            ff::upgrade_check::LAST_IMPLEMENTATION,
            implementation.clone(),
        );
    let mut f = filled(&ff::V2);
    f.set(ff::CHAIN_ID, pin.chain_id)
        .set(ff::PORTAL, portal.clone())
        .set(ff::PORTAL_VERSION, pin.portal_version)
        .set(ff::PIN_RECHECKED, true)
        .set(ff::UPGRADE_CHECK, upgrade.into_nested());
    fs::write(root.join(ev).join("freeze.dbrec"), f.encode().unwrap()).unwrap();
    // BSC3-065: the pinned implementation's behaviour parity with its
    // source-verified reference.
    let reference = behaviour_reference(&pin).unwrap();
    let mut p = filled(&fp::SCHEMA);
    p.set(fp::CHAIN_ID, pin.chain_id)
        .set(fp::PORTAL, portal.clone())
        .set(fp::OBSERVED_IMPLEMENTATION, implementation.clone())
        .set(
            fp::OBSERVED_RUNTIME_SHA256,
            Value::B256(pin.portal_implementation_runtime_sha256),
        )
        .set(fp::OBSERVED_VERSION, pin.portal_version)
        .set(
            fp::SOURCE_IMPLEMENTATION,
            Value::Address(reference.portal_implementation.into_array()),
        )
        .set(
            fp::SOURCE_RUNTIME_SHA256,
            Value::B256(reference.portal_implementation_runtime_sha256),
        )
        .set(fp::VERDICT, "BEHAVIOUR_PARITY")
        .set(fp::DIFFERENT, 0u64)
        .set(fp::PRODUCTION_SELECTORS_PRESENT, true)
        .set(fp::CHAIN_VERSION_CHECKED, true);
    // Per traded path the floor of cases executed under both, each
    // identical (the qualification counts them from the cases).
    let floor = darkbot_bsc_execution::live_qualification::PARITY_MIN_EXECUTED_PER_PATH;
    for (path, min) in [
        ("LAUNCH", fp::MIN_LAUNCHES),
        ("BUY", fp::MIN_BUYS),
        ("SELL", fp::MIN_SELLS),
        ("LAUNCH_THEN_BUY", fp::MIN_LAUNCH_BUYS),
    ] {
        for _ in 0..floor {
            let mut c = filled(&fp::case::SCHEMA);
            c.set(fp::case::CLASS, path)
                .set(fp::case::SOURCE_STATUS, 1u64)
                .set(fp::case::OBSERVED_STATUS, 1u64)
                .set(fp::case::OUTCOME, "IDENTICAL");
            p.push(fp::CASES, c.into_nested());
        }
        p.set(min, floor);
    }
    p.set(fp::IDENTICAL, 4 * floor)
        .set(fp::IDENTICAL_BUT_CREATED, 0u64);
    fs::write(root.join(ev).join("parity.dbrec"), p.encode().unwrap()).unwrap();
    let mut flap = filled(&gv::venue::SCHEMA);
    flap.set(gv::venue::VENUE, gv::RATED_VENUES[0])
        .set(gv::venue::VERDICT, gv::CONTINUE);
    let mut publicnode = filled(&census::source::SCHEMA);
    publicnode.set(census::source::SOURCE_ID, "BSC_INGRESS_PUBLICNODE");
    let mut g04 = filled(&gv::SCHEMA);
    g04.set(gv::CRITERIA_STATE, gv::CONFIRMED)
        .set(gv::VERDICT, gv::CONTINUE)
        .set(gv::PRECOMMIT_SHA256, Value::B256([3; 32]))
        .push(gv::VENUES, flap.into_nested())
        .push(gv::SOURCES, publicnode.into_nested());
    fs::write(root.join(ev).join("g04.dbrec"), g04.encode().unwrap()).unwrap();
    let buyer = Signer::from_secret_bytes(&BUYER_SECRET)
        .unwrap()
        .address()
        .into_array();
    let mut preflight = filled(&g06a::preflight::SCHEMA);
    preflight
        .set(g06a::preflight::CHAIN_ID, pin.chain_id)
        .set(g06a::preflight::PORTAL, portal.clone())
        .set(g06a::preflight::PORTAL_VERSION, pin.portal_version)
        .set(g06a::preflight::PORTAL_IMPLEMENTATION, implementation)
        .set(
            g06a::preflight::PORTAL_IMPLEMENTATION_CODE_SHA256,
            Value::B256(pin.portal_implementation_runtime_sha256),
        );
    let mut club48 = filled(&g07::provider_result::SCHEMA);
    club48
        .set(g07::provider_result::BYTES_MAY_HAVE_LEFT, true)
        .set(g07::provider_result::HTTP_STATUS_CODE, 200u64)
        .set(g07::provider_result::RPC_RESULT_HASH, Value::B256([7; 32]));
    let mut send = filled(&g07::evidence::send::SCHEMA);
    send.set(g07::evidence::send::FORTYEIGHTCLUB, club48.into_nested());
    let mut attempt = filled(&g07::evidence::SCHEMA);
    attempt
        .set(g07::evidence::GATE, "G0.7")
        .set(g07::evidence::ATTEMPT_ID, "synthetic-attempt")
        .set(g07::evidence::CHAIN_ID, pin.chain_id)
        .set(g07::evidence::PORTAL, portal)
        .set(g07::evidence::BUYER, Value::Address(buyer))
        .set(g07::evidence::BUY_TX_HASH, Value::B256([5; 32]))
        .set(g07::evidence::TOKEN, Value::Address([6; 20]))
        .set(g07::evidence::PREFLIGHT, preflight.into_nested())
        .set(g07::evidence::SEND, send.into_nested());
    let attempt = attempt.encode().unwrap();
    fs::write(root.join(ev).join("g07.dbrec"), &attempt).unwrap();
    let mut g = filled(&g08::result::SCHEMA);
    g.set(g08::result::GATE, "G0.8")
        .set(g08::result::VERDICT, verdict)
        .set(
            g08::result::G07_EVIDENCE_SHA256,
            Value::B256(darkbot_bsc_record::sha256(&attempt)),
        )
        .set(g08::result::ATTEMPT_ID, "synthetic-attempt")
        .set(g08::result::BUY_TX, Value::B256([5; 32]))
        .set(g08::result::TOKEN, Value::Address([6; 20]))
        .set(g08::result::BUYER, Value::Address(buyer));
    fs::write(root.join(ev).join("g08.dbrec"), g.encode().unwrap()).unwrap();
    let m = filled(&bsc_runtime::buy_attempt_slot::SCHEMA);
    fs::write(root.join(ev).join("measure.dbrec"), m.encode().unwrap()).unwrap();
}

/// A synthetic evidence root: a G0.8 result of `verdict`, one measurement.
fn evidence_root(tag: &str, verdict: &str) -> PathBuf {
    let root = scratch(tag);
    write_evidence(&root, "ev", verdict);
    root
}

/// The synthetic buyer key as the stand-in's test identity of the owner
/// state `state` (BSC3-026: the loopback route signs with no other key).
fn buyer_key(state: &Path) -> (PathBuf, [u8; 20]) {
    let path = darkbot_bsc_execution::live_key::loopback_identity(state);
    let secret = BUYER_SECRET;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut f| f.write_all(&secret))
        .unwrap();
    (
        path,
        Signer::from_secret_bytes(&secret)
            .unwrap()
            .address()
            .into_array(),
    )
}

/// The builder stand-in on 127.0.0.1: every complete HTTP request, whole.
fn stand_in() -> (u16, mpsc::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let tx = tx.clone();
            std::thread::spawn(move || {
                let (mut buf, mut chunk) = (Vec::new(), [0u8; 4096]);
                loop {
                    if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..end]).to_string();
                        let len: usize = head
                            .lines()
                            .find_map(|l| l.strip_prefix("Content-Length: "))
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(0);
                        if buf.len() >= end + 4 + len {
                            let rest = buf.split_off(end + 4 + len);
                            if tx.send(std::mem::replace(&mut buf, rest)).is_err() {
                                return;
                            }
                            continue;
                        }
                    }
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
            });
        }
    });
    (port, rx)
}

/// One `bsc_runtime` command: exit code and output.
fn cli(args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_bsc_runtime"))
        .args(args)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

const FINDINGS: &str = "PROPERTY:PRESERVES_ORDER=YES,PROPERTY:ATOMIC=YES,PROPERTY:SAME_BLOCK=YES,\
PROPERTY:EXTRACTS_TRANSACTIONS=NO,PROPERTY:PROVIDER_REBROADCAST=NO,PROPERTY:PROVIDER_PUBLIC_FALLBACK=NO,\
PROPERTY:REACHES_OTHER_BUILDERS=NO,PROPERTY:PUBLIC_MEMPOOL_EXPOSURE=NO,FEATURE:NO_MERGE,\
FEATURE:POSITION_FIRST,FEATURE:MAX_BLOCK_NUMBER,MODE:SINGLE_RAW,SEND_TIME";

/// The LIVE qualification of the synthetic scope, through the CLI.
fn qualify(root: &Path) {
    qualify_in(root, "ev", E2E_SCOPE);
}

/// BSC3-012: the build a qualification names: the browser E2E's runtime
/// (`DARKBOT_LIVE_E2E_RUNTIME`), else this test's process, which arms.
fn qualified_build() -> String {
    std::env::var("DARKBOT_LIVE_E2E_RUNTIME").unwrap_or_else(|_| {
        std::env::current_exe()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    })
}

fn qualify_in(root: &Path, ev: &str, (strategies, attempts): (&str, &str)) {
    let adapter = format!("CLUB48_BUILDER_V1@{FINDINGS}@{ev}/g07.dbrec");
    let venue = format!("BSC_VENUE_FLAP@{ev}/freeze.dbrec,{ev}/parity.dbrec");
    let source = format!("BSC_INGRESS_PUBLICNODE@{ev}/g04.dbrec");
    let g08 = format!("{ev}/g08.dbrec@{ev}/g07.dbrec");
    let risk = format!("{ev}/g08.dbrec");
    // BSC3-049: the risk is decided for the grant scope [`activate`] grants
    // (its validity aside).
    let (code, out) = cli(&[
        &["live", "scope"][..],
        &scope_args(strategies, attempts, "1"),
    ]
    .concat());
    assert_eq!(code, 0, "{out}");
    let risk_scope = out
        .lines()
        .find_map(|l| l.strip_prefix("risk_scope_sha256="))
        .unwrap()
        .to_string();
    let root_s = root.to_str().unwrap();
    let (code, out) = cli(&[
        "live",
        "qualify",
        "--root",
        root_s,
        "--runtime-binary",
        &qualified_build(),
        "--venue",
        &venue,
        "--source",
        &source,
        "--adapter",
        &adapter,
        "--acquisition",
        &g08,
        "--risk-by",
        "operator",
        "--risk-statement",
        "synthetic non-sending E2E scope",
        "--risk-scope-sha256",
        &risk_scope,
        "--risk-evidence",
        &risk,
    ]);
    assert_eq!(code, 0, "{out}");
}

/// The browser E2E's harness (`ops/tests/bsc_control_test.py`): a verified
/// LIVE qualification of synthetic evidence in its isolated snapshot root
/// `DARKBOT_LIVE_E2E_ROOT`, the evidence outside every directory the
/// reporter reads (never shown as a real result). Without the variable it
/// does nothing.
#[test]
#[ignore]
fn write_synthetic_qualification_for_the_browser_e2e() {
    let Some(root) = std::env::var_os("DARKBOT_LIVE_E2E_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    write_evidence(&root, "bsc/config/live_synthetic", "PASS");
    qualify_in(&root, "bsc/config/live_synthetic", BROWSER_SCOPE);
}

/// The strategies and BUY attempts of this test's grant ([`activate`]).
const E2E_SCOPE: (&str, &str) = ("C3+STRATEGY_C", "2");
/// Those the browser E2E grants (`ops/tests/bsc_control_browser.mjs`: the
/// grant form's default strategies, one attempt; the rest as here).
const BROWSER_SCOPE: (&str, &str) = ("C3", "1");

/// The synthetic LIVE grant's scope arguments with `strategies` and
/// `attempts`, valid until `expires`.
fn scope_args<'a>(strategies: &'a str, attempts: &'a str, expires: &'a str) -> [&'a str; 36] {
    [
        "--venue",
        "BSC_VENUE_FLAP",
        "--strategies",
        strategies,
        "--sources",
        "BSC_INGRESS_PUBLICNODE",
        "--adapters",
        "CLUB48_BUILDER_V1",
        "--max-input-wei",
        "2000000000000000",
        "--max-spend-wei",
        "50000000000000000",
        "--max-attempts",
        attempts,
        "--expires-unix-ms",
        expires,
        // BSC3-069: the execution risk, exactly the executor's.
        "--max-buy-slippage-bps",
        "100",
        "--max-exit-slippage-bps",
        "100",
        "--max-gas-price-wei",
        "50000000",
        "--max-buy-gas-limit",
        "3000000",
        "--max-approve-gas-limit",
        "100000",
        "--max-sell-gas-limit",
        "1500000",
        // 31.10. order: the positions and losses, none reached here.
        "--max-open-positions",
        "16",
        "--max-realized-loss-wei",
        "50000000000000000",
        "--max-daily-realized-loss-wei",
        "50000000000000000",
        "--min-buy-interval-ms",
        "0",
    ]
}

/// Activates the isolated owner `state` through the CLI, leaving out the
/// requests in `skip`; the grant expires at `expires`.
fn activate(state: &Path, skip: &[&str], expires: u64) {
    let state_s = state.to_str().unwrap();
    assert_eq!(cli(&["source", "init", "--state", state_s]).0, 0);
    let mut generation = 0u64;
    let request = |generation: &mut u64, id: &str, mode: &str| {
        let g = generation.to_string();
        let rid = format!("e2e-{generation}");
        let (code, out) = cli(&[
            "source",
            "request",
            id,
            mode,
            "--expect-generation",
            &g,
            "--request-id",
            &rid,
            "--state",
            state_s,
        ]);
        assert_eq!(code, 0, "{id} {mode}: {out}");
        *generation += 1;
    };
    let requests: [(&str, &str); 16] = [
        ("BSC_WALLET_LAUNCHER", "ENABLED"),
        ("BSC_WALLET_BUYER", "ENABLED"),
        ("BSC_ECON_SIGN", "ENABLED"),
        ("BSC_ECON_ARM", "ENABLED"),
        ("BSC_ECON_SEND", "ENABLED"),
        ("BSC_ECON_ATTEMPT", "ENABLED"),
        ("BSC_ECON_LIVE", "ENABLED"),
        ("BSC_RUNTIME_MODE", "LIVE"),
        ("BSC_EXEC", "LIVE"),
        ("BSC_STRATEGY_C", "LIVE"),
        ("BSC_STATE_C2", "LIVE"),
        ("BSC_VENUE_FLAP", "LIVE"),
        ("BSC_INGRESS_PUBLICNODE", "LIVE"),
        ("BSC_BUY_ATTEMPTS_PER_DAY", "2"),
        ("BSC_BUY_ATTEMPTS_TOTAL", "3"),
        ("GRANT", ""),
    ];
    for (id, mode) in requests {
        if skip.contains(&id) {
            continue;
        }
        if id == "GRANT" {
            let expires = expires.to_string();
            let scope = scope_args(E2E_SCOPE.0, E2E_SCOPE.1, &expires);
            let (code, out) = cli(&[&["live", "scope"][..], &scope].concat());
            assert_eq!(code, 0, "{out}");
            let sha = out
                .lines()
                .find_map(|l| l.strip_prefix("scope_sha256="))
                .unwrap()
                .to_string();
            let g = generation.to_string();
            let (code, out) = cli(&[
                &["live", "grant"][..],
                &scope,
                &[
                    "--scope-sha256",
                    &sha,
                    "--expect-generation",
                    &g,
                    "--request-id",
                    "e2e-grant",
                    "--state",
                    state_s,
                ],
            ]
            .concat());
            assert_eq!(code, 0, "{out}");
            generation += 1;
        } else {
            request(&mut generation, id, mode);
        }
    }
}

fn limits() -> FollowerLimits {
    FollowerLimits {
        poll: Duration::from_millis(100),
        reorg_depth: 8,
        max_catch_up: 8,
        connect_timeout: Duration::from_millis(200),
        handshake_timeout: Duration::from_millis(200),
        read_tick: Duration::from_millis(20),
        write_timeout: Duration::from_millis(200),
        request_timeout: Duration::from_millis(200),
        max_response_bytes: 1 << 20,
        backoff_initial: Duration::from_millis(100),
        backoff_max: Duration::from_millis(1_000),
        tx_attempts: 1,
        max_launch_calldata: 16_384,
        failover_after: 2,
        stall_after: Duration::from_secs(60),
    }
}

fn exec_setup() -> ExecSetup {
    ExecSetup {
        identity_path: PathBuf::from("/nonexistent/paper.key"),
        refused_accounts: Vec::new(),
        exec: ExecConfig {
            input_wei: u128::from(INPUT),
            slippage_bps: 100,
            exit_slippage_bps: 100,
            gas_price_wei: GAS_PRICE,
            gas_limit: 3_000_000,
            session_cap_wei: 1_150_000_000_000_000,
            lanes: 2,
            max_calldata: 16_384,
            latency_samples: 16,
            policy_generation: 1,
            capability_generation: 1,
            validity_blocks: 2,
            start_limit_ns: 1_000_000,
            max_frame: 65_536,
            strategy_lanes: 8,
            strategy_session_cap_wei: 100_000_000_000_000_000,
            approve_gas_limit: 100_000,
            sell_gas_limit: 1_500_000,
            strategy_max_head_age_ms: 30_000,
            live_max_head_age_ms: 86_400_000,
            live_exit_reverts_max: 2,
            live_lease_blocks: 200,
            live_operator_exit: false,
            live_nonce_burn: false,
            sender_paths: SenderPaths::Both,
        },
        reprobe: Duration::from_secs(3_600),
        reprobe_failures: 1,
    }
}

/// The synthetic deployment probe: the witnessed deployment and terms; the
/// buyer funded (or not).
fn observation(d: &FlapDeployment, balance: U256) -> VenueObservation {
    let fingerprint = |sha256| CodeFingerprint {
        sha256,
        keccak256: B256::ZERO,
        len: 1,
    };
    VenueObservation {
        block_number: HEAD,
        block_hash: [0xb1; 32],
        chain_id: d.chain_id,
        observed: ObservedDeploymentData {
            portal_version: d.portal_version.to_string(),
            portal_code: fingerprint(d.portal_runtime_sha256),
            portal_implementation: d.portal_implementation,
            portal_implementation_code: fingerprint(d.portal_implementation_runtime_sha256),
            tax_v3_implementation_code: fingerprint(d.tax_v3_implementation_runtime_sha256),
        },
        terms: WITNESSED_MAINNET_TERMS,
        caller_balance_wei: balance,
    }
}

fn funded(
    _: &str,
    _: &FollowerLimits,
    d: &FlapDeployment,
    _: Address,
) -> Result<VenueObservation, String> {
    Ok(observation(d, U256::from(10u64).pow(U256::from(18u8))))
}

fn unfunded(
    _: &str,
    _: &FollowerLimits,
    d: &FlapDeployment,
    _: Address,
) -> Result<VenueObservation, String> {
    Ok(observation(d, U256::ZERO))
}

/// The witnessed native self-launch, signed by a synthetic third party.
fn launch() -> ([u8; 32], Vec<u8>, [u8; 20]) {
    let mut beneficiary = [0u8; 20];
    beneficiary[18] = 0xa1;
    beneficiary[19] = 0x1c;
    let params = LaunchParamsV6 {
        name: b"DarkBot G06A Economics",
        symbol: b"DBG06E",
        uri: b"ipfs://darkbot-g06a-economics",
        dex_thresh: 1,
        salt: B256::from(hex!(
            "09c4b20c7956530d4bf91b3569ed173abfce47d16d412d94dde5e6f4034c6eeb"
        )),
        migrator_type: 1,
        quote_token: Address::ZERO,
        quote_amt: U256::ZERO,
        beneficiary: Address::from(beneficiary),
        permit: &[],
        extension_id: B256::ZERO,
        extension_data: &[],
        dex_id: 0,
        lp_fee_profile: 0,
        buy_tax: 100,
        sell_tax: 100,
        tax_duration: 31_536_000,
        anti_farmer_duration: 86_400,
        mkt_bps: 10_000,
        deflation_bps: 0,
        dividend_bps: 0,
        lp_bps: 0,
        minimum_share_balance: U256::ZERO,
        dividend_token: Address::ZERO,
        commission_receiver: Address::ZERO,
        token_version: 6,
    };
    let mut input = Vec::new();
    encode_new_token_v6(&params, &mut input);
    let launcher = Signer::from_secret_bytes(&[0x5a; 32]).unwrap();
    let (mut scratch, mut raw) = (Vec::new(), Vec::new());
    launcher
        .sign_legacy(
            &LegacyTx {
                chain_id: 56,
                nonce: 3,
                gas_price: 1_000_000_000,
                gas_limit: 6_000_000,
                to: bsc_mainnet_v5_27_2().portal,
                value: U256::ZERO,
                input: &input,
            },
            &mut scratch,
            &mut raw,
        )
        .unwrap();
    (keccak256(&raw).0, raw, launcher.address().into_array())
}

fn block(n: u64, h: u8, parent: u8, at_ms: u64, txs: Vec<CanonicalTx>) -> CanonicalEvent {
    CanonicalEvent::Block {
        number: n,
        hash: [h; 32],
        parent_hash: [parent; 32],
        timestamp_s: at_ms / 1_000,
        timestamp_ms: at_ms,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs,
    }
}

/// The C2 owner watching the buyer, anchored at a head of `at_ms` with the
/// buyer's next nonce 0.
fn c2(dir: &Path, buyer: [u8; 20], at_ms: u64) -> StateOwner {
    let mut o = StateOwner::fresh(
        &StateConfig {
            limits: StateLimits {
                max_txs: 64,
                max_candidates_per_nonce: 2,
                max_watched_senders: 2,
                reorg_window: 8,
                finality_depth: 2,
            },
            watched: vec![buyer],
            checkpoint_ms: 1_000,
            nonce_fact_blocks: 1,
        },
        dir.join("c2.bin"),
    )
    .unwrap();
    o.canonical(&block(HEAD, 0xb1, 0xb0, at_ms, Vec::new()));
    o.canonical(&CanonicalEvent::NonceFact {
        sender: buyer,
        next_nonce: 0,
        block_number: HEAD,
        block_hash: [0xb1; 32],
    });
    let _ = o.take_evidence();
    o
}

fn status(x: &LiveRuntime) -> (String, String) {
    let r = darkbot_bsc_record::decode(&l::SCHEMA, &x.draft().encode().unwrap()).unwrap();
    (
        r.text(l::STATE).unwrap().to_string(),
        r.opt_text(l::REASON)
            .unwrap()
            .unwrap_or_default()
            .to_string(),
    )
}

/// Ticks until the path leaves PROBING.
fn settle(x: &mut LiveRuntime, owner: &StateOwner) {
    let lim = limits();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        x.tick(
            Some(owner),
            Some(("https://probe.invalid", &lim)),
            1,
            now_ms(),
        );
        if status(x).0 != "PROBING" || Instant::now() > deadline {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[allow(clippy::too_many_arguments)]
fn runtime(
    key: &Path,
    buyer: [u8; 20],
    port: u16,
    root: &Path,
    state: &Path,
    fences: &Path,
    prober: darkbot_bsc_execution::runtime::exec::Prober,
) -> LiveRuntime {
    LiveRuntime::with_prober(
        LiveSetup {
            key_path: key.to_path_buf(),
            buyer,
            route: LiveRouteConfig::Loopback { port },
            connect_timeout: Duration::from_secs(2),
        },
        exec_setup(),
        root,
        state.to_path_buf(),
        fences.to_path_buf(),
        None,
        prober,
    )
}

/// Plan 28.10.: CLI activation -> effective LIVE -> a replayed launch ->
/// BUY -> receipt -> position -> exit -> approval + SELL -> PnL, everything
/// on synthetic keys, evidence and blocks, every request to 127.0.0.1 only;
/// each missing precondition blocks with its reason and sends nothing.
#[test]
fn live_buy_to_sell_is_synthetic_and_never_leaves_the_host() {
    let root = evidence_root("root", "PASS");
    qualify(&root);
    let home = scratch("home");
    let (port, requests) = stand_in();
    let state = home.join("owner");
    activate(&state, &[], now_ms() + 3_600_000);
    let (key, buyer) = buyer_key(&state);
    // BSC3-026: the CLI shows the state's test identity; it draws one
    // only in a state that has none.
    let (code, out) = cli(&[
        "live",
        "loopback-identity",
        "show",
        "--state",
        state.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(&format!("loopback_identity=0x{}", hex::encode(buyer))),
        "{out}"
    );
    let (code, out) = cli(&[
        "live",
        "loopback-identity",
        "init",
        "--state",
        state.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(code != 0 && out.contains("loopback-identity.key"), "{out}");
    // Under a HOME that is not the account's the account's state is
    // unknown: nothing is drawn.
    let fresh = home.join("fresh-state");
    fs::DirBuilder::new().mode(0o700).create(&fresh).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_bsc_runtime"))
        .args([
            "live",
            "loopback-identity",
            "init",
            "--state",
            fresh.to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
        ])
        .env("HOME", &home)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && text.contains("LOOPBACK_STATE_UNVERIFIED"),
        "{text}"
    );
    assert!(!darkbot_bsc_execution::live_key::loopback_identity(&fresh).exists());
    // The CLI says the gate is clear for this scope.
    let (code, out) = cli(&[
        "live",
        "status",
        "--state",
        state.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("qualification=VERIFIED") && out.contains("live=CLEAR"),
        "{out}"
    );

    // The runtime's LIVE path arms: funded buyer, C2's nonce, the key.
    let head_ms = now_ms();
    let mut owner = c2(&home, buyer, head_ms);
    let mut x = runtime(
        &key,
        buyer,
        port,
        &root,
        &state,
        &home.join("fences"),
        funded,
    );
    settle(&mut x, &owner);
    assert_eq!(status(&x), ("ARMED".into(), String::new()));

    // A recorded launch replayed through the production ingress core.
    let mut core = IngressCore::new(64, 16_384);
    let (hash, raw, launcher) = launch();
    let at = 1_000;
    let decided = core.observe(
        SourceId::PublicNode,
        Observation::Full { hash, raw: &raw },
        at,
        None,
    );
    let Some(Launch::First {
        hash: first,
        source,
        venue: Venue::Flap,
        ..
    }) = decided
    else {
        panic!("not a first Flap launch: {decided:?}");
    };
    owner.pending(first, launcher, 3, 1, at);
    let t = Cell::new(at);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let buy = x
        .on_launch(source, first, &raw, at, &owner, &clock)
        .expect("the LIVE BUY");
    let request =
        String::from_utf8(requests.recv_timeout(Duration::from_secs(2)).unwrap()).unwrap();
    assert!(request.contains("eth_sendBundle") && request.contains("\"positionFirst\":true"));

    // Its receipt: included, its gas, the Portal's trade.
    let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
    let q = quote_native_buy(&NativeBuyInputs {
        token: terms,
        reserve_before: U256::ZERO,
        circulating_before: U256::ZERO,
        input_amount: U256::from(INPUT),
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
        holding_cap: HoldingCap::None,
    })
    .unwrap();
    let tokens = q.output_amount;
    let token = buy.token;
    // Each transaction alone in its block.
    let tx = |hash, nonce, gas: u64| CanonicalTx {
        hash,
        index: 0,
        sender: buyer,
        nonce,
        succeeded: true,
        cost: Some(TxCost {
            gas_used: gas,
            effective_gas_price: GAS_PRICE,
        }),
    };
    let trade = |hash, n: u64, h: u8, sell, amount: U256, eth: U256| {
        CanonicalEvent::FlapTrade(CanonicalTrade {
            block_number: n,
            block_hash: [h; 32],
            tx_index: 0,
            tx_hash: hash,
            log_index: 0,
            origin: buyer,
            sell,
            token,
            trader: buyer,
            amount: amount.to_be_bytes(),
            eth: eth.to_be_bytes(),
            fee: [0; 32],
            post_price: [0; 32],
        })
    };
    let apply = |x: &mut LiveRuntime, owner: &mut StateOwner, events: Vec<CanonicalEvent>| {
        let mut changes = Vec::new();
        for e in &events {
            owner.canonical(e);
            let evidence = owner.take_evidence();
            x.apply_evidence(&evidence);
            changes.extend(x.on_canonical(e));
        }
        changes
    };
    let changes = apply(
        &mut x,
        &mut owner,
        vec![
            block(
                HEAD + 1,
                0xb2,
                0xb1,
                head_ms + 1_000,
                vec![tx(buy.handoff.tx_hash, 0, 180_000)],
            ),
            trade(
                buy.handoff.tx_hash,
                HEAD + 1,
                0xb2,
                false,
                tokens,
                U256::from(INPUT),
            ),
            CanonicalEvent::BlockComplete {
                number: HEAD + 1,
                hash: [0xb2; 32],
            },
        ],
    );
    assert!(
        matches!(changes.as_slice(), [PositionChange::Opened { tokens: t, .. }] if *t == tokens)
    );

    // The exit by the rules: the approval, then, once it is canonical
    // (BSC3-055: one nonce after the other), the SELL of the real tokens,
    // signed by the buyer key, each admitted by the owner.
    let held = StrategyCurve {
        terms,
        reserve: q.reserve_after,
        circulating: q.supply_after,
    };
    let exit = StrategyAction::Exit {
        token,
        tokens,
        curve: held,
        decided_ns: 0,
    };
    // A reconcile pass lies between (the runtime's): it renews the link the
    // BUY left awaiting an answer the stand-in never gives.
    settle(&mut x, &owner);
    let (out, refused) = x.on_strategy(&exit, &owner, &clock, now_ms()).unwrap();
    assert!(!out.is_empty(), "{refused:?} {:?}", status(&x));
    assert_eq!(out[0].kind, IntentKind::Approve);
    assert_eq!(out.len(), 1);
    // The exit is not over: its SELL waits for the approval's nonce.
    assert_eq!(refused, None);
    let approve_tx = out[0].handoff.tx_hash;
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(requests.recv_timeout(Duration::from_millis(200)).is_err());
    assert!(x.exits_to_continue(&owner).is_empty());
    // The approval is included: the exit continues with the SELL.
    let changes = apply(
        &mut x,
        &mut owner,
        vec![
            block(
                HEAD + 2,
                0xb3,
                0xb2,
                head_ms + 2_000,
                vec![tx(approve_tx, 1, 50_000)],
            ),
            CanonicalEvent::BlockComplete {
                number: HEAD + 2,
                hash: [0xb3; 32],
            },
        ],
    );
    assert!(changes.is_empty());
    assert_eq!(x.exits_to_continue(&owner), vec![token]);
    // The next pass keeps the connection (the stand-in answers nothing).
    settle(&mut x, &owner);
    let (out, refused) = x.on_strategy(&exit, &owner, &clock, now_ms()).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Sell);
    let sell_tx = out[0].handoff.tx_hash;
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let proceeds = quote_native_sell(&NativeSellInputs {
        token: terms,
        reserve_before: held.reserve,
        circulating_before: held.circulating,
        input_amount: tokens,
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_sell_fee_bps,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
    })
    .unwrap()
    .output_amount;
    let changes = apply(
        &mut x,
        &mut owner,
        vec![
            block(
                HEAD + 3,
                0xb4,
                0xb3,
                head_ms + 3_000,
                vec![tx(sell_tx, 2, 150_000)],
            ),
            trade(sell_tx, HEAD + 3, 0xb4, true, tokens, proceeds),
            CanonicalEvent::BlockComplete {
                number: HEAD + 3,
                hash: [0xb4; 32],
            },
        ],
    );
    assert_eq!(changes, vec![PositionChange::Ended { token }]);
    let gas = (180_000u128 + 50_000 + 150_000) * u128::from(GAS_PRICE);
    let pnl = i128::try_from(proceeds).unwrap() - i128::from(INPUT) - gas as i128;
    let (code, out) = cli(&["live", "positions", "--state", state.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("state=CLOSED") && out.contains(&format!("pnl_wei={pnl}")),
        "{out}"
    );
    assert!(out.contains(&format!("realized_pnl_wei={pnl}")), "{out}");
    // Three requests reached the stand-in: the BUY bundle, the approval,
    // the SELL. Nothing else, anywhere.
    assert!(requests.recv_timeout(Duration::from_millis(200)).is_err());
    x.shutdown();

    // Every missing precondition: BLOCKED with its reason, nothing sent.
    let cases: [(&str, &[&str], u64, Option<&str>); 9] = [
        (
            "econ-live",
            &["BSC_ECON_LIVE"],
            3_600_000,
            Some("ECONOMIC_LIVE_DISABLED"),
        ),
        (
            "buyer",
            &["BSC_WALLET_BUYER"],
            3_600_000,
            Some("WALLET_BUYER_DISABLED"),
        ),
        (
            "send",
            &["BSC_ECON_SEND"],
            3_600_000,
            Some("ECON_SEND_DISABLED"),
        ),
        ("grant", &["GRANT"], 3_600_000, Some("LIVE_GRANT_MISSING")),
        (
            "runtime",
            &["BSC_RUNTIME_MODE"],
            3_600_000,
            Some("LIVE_RUNTIME_NOT_REQUESTED"),
        ),
        (
            "daily",
            &["BSC_BUY_ATTEMPTS_PER_DAY"],
            3_600_000,
            Some("BUY_LIMIT_EXHAUSTED"),
        ),
        (
            "total",
            &["BSC_BUY_ATTEMPTS_TOTAL"],
            3_600_000,
            Some("ATTEMPT_TOTAL_EXHAUSTED"),
        ),
        (
            "component",
            &["BSC_EXEC", "BSC_STRATEGY_C"],
            3_600_000,
            None,
        ),
        ("expired", &[], 0, Some("LIVE_GRANT_EXPIRED")),
    ];
    for (tag, skip, valid_ms, reason) in cases {
        let state = home.join(format!("owner-{tag}"));
        activate(
            &state,
            skip,
            if valid_ms == 0 {
                1
            } else {
                now_ms() + valid_ms
            },
        );
        let (code, out) = cli(&[
            "live",
            "status",
            "--state",
            state.to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
        ]);
        if let Some(reason) = reason {
            assert_eq!(code, 2, "{tag}: {out}");
            assert!(out.contains(reason), "{tag}: {out}");
        }
        let mut blocked = runtime(
            &key,
            buyer,
            port,
            &root,
            &state,
            &home.join(format!("fences-{tag}")),
            funded,
        );
        settle(&mut blocked, &owner);
        let (s, why) = status(&blocked);
        assert_eq!(s, "BLOCKED", "{tag}: {why}");
        assert!(!blocked.armed(), "{tag}");
        assert!(
            blocked
                .on_launch(source, first, &raw, at, &owner, &clock)
                .is_none(),
            "{tag}"
        );
    }
    // An unqualified scope (no qualification on file, or evidence changed
    // since it was qualified) and an unfunded buyer never arm either.
    let bare = scratch("bare");
    let mut unqualified = runtime(
        &key,
        buyer,
        port,
        &bare,
        &state,
        &home.join("fences-bare"),
        funded,
    );
    settle(&mut unqualified, &owner);
    assert_eq!(status(&unqualified).0, "BLOCKED");
    let tampered = evidence_root("tampered", "PASS");
    qualify(&tampered);
    fs::write(tampered.join("ev/g04.dbrec"), b"changed").unwrap();
    let mut invalid = runtime(
        &key,
        buyer,
        port,
        &tampered,
        &state,
        &home.join("fences-t"),
        funded,
    );
    settle(&mut invalid, &owner);
    let (s, why) = status(&invalid);
    assert!(
        s == "BLOCKED" && why.starts_with("LIVE_QUALIFICATION_INVALID"),
        "{s} {why}"
    );
    let mut poor = runtime(
        &key,
        buyer,
        port,
        &root,
        &home.join("owner-econ-live"),
        &home.join("fences-p"),
        unfunded,
    );
    settle(&mut poor, &owner);
    assert!(!poor.armed());
    // A G0.8 result that is not a PASS qualifies nothing.
    let adapt = evidence_root("adapt", "ADAPT");
    let adapter = format!("CLUB48_BUILDER_V1@{FINDINGS}@ev/g07.dbrec");
    let (code, out) = cli(&[
        "live",
        "qualify",
        "--root",
        adapt.to_str().unwrap(),
        "--runtime-binary",
        &qualified_build(),
        "--adapter",
        &adapter,
        "--acquisition",
        "ev/g08.dbrec@ev/g07.dbrec",
    ]);
    assert!(
        code != 0 && out.contains("not an acquisition PASS"),
        "{out}"
    );
    // BSC3-049: a risk decision names the grant risk scope it is made for.
    let (code, out) = cli(&[
        "live",
        "qualify",
        "--root",
        adapt.to_str().unwrap(),
        "--runtime-binary",
        &qualified_build(),
        "--risk-by",
        "operator",
        "--risk-statement",
        "synthetic",
        "--risk-evidence",
        "ev/g08.dbrec",
    ]);
    assert!(code != 0 && out.contains("--risk-scope-sha256"), "{out}");
    // A launch first seen by a source the grant does not name never trades.
    let mut armed = runtime(
        &key,
        buyer,
        port,
        &root,
        &state,
        &home.join("fences-src"),
        funded,
    );
    let fresh = c2(&home.join("c2-src"), buyer, now_ms());
    settle(&mut armed, &fresh);
    assert!(
        armed
            .on_launch(SourceId::Drpc, first, &raw, at, &fresh, &clock)
            .is_none()
    );
    // Not one request more reached the stand-in.
    assert!(requests.recv_timeout(Duration::from_millis(300)).is_err());
    for d in [root, home, bare, tampered, adapt] {
        fs::remove_dir_all(d).unwrap();
    }
}

/// 01.11. (R1): `source init` (darkbotctl up, the desktop app) only puts
/// the owner state on file: a running owner's switches and LIVE grant stay
/// (before, it was the owner's reset and revoked a running LIVE path, its
/// exits included). `source reset` (a stop) ends them; `live idle` refuses
/// while a grant holds (what `darkbotctl deploy` checks before it restarts
/// the runtime) and passes once nothing LIVE holds.
#[test]
fn source_init_keeps_a_running_live_grant_and_a_reset_ends_it() {
    let state = scratch("owner-cli");
    activate(&state, &[], now_ms() + 3_600_000);
    let s = state.to_str().unwrap();
    let idle = || cli(&["live", "idle", "--state", s]);
    let live_switch = || cli(&["control", "require", "--state", s, "BSC_ECON_LIVE"]).0;
    let (code, out) = idle();
    assert_ne!(code, 0, "{out}");
    assert!(
        out.contains("LIVE_ACTIVE: grant=true positions_held=0 unresolved=0"),
        "{out}"
    );
    let (code, out) = cli(&["source", "init", "--state", s]);
    assert_eq!(code, 0, "{out}");
    assert!(idle().1.contains("live_grant=true"), "the grant stays");
    assert_eq!(live_switch(), 0, "the switches stay");
    let (code, out) = cli(&["source", "reset", "--state", s]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = idle();
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("live_grant=false positions_held=0 unresolved=0"),
        "{out}"
    );
    assert_ne!(live_switch(), 0, "a reset leaves every switch DISABLED");
    fs::remove_dir_all(state).unwrap();
}

/// REV.1.0 R3b (Audit A): the GUI's LIVE ledger decisions go through the one controller as the
/// terminal's: `darkbotctl bsc live lot|nonce-burn|ledger-quarantine` (`ops/bsc-control.sh`, in
/// its isolated test mode) hands this build's runtime a lot's decision and a nonce burn with the
/// ledger's version, a quarantine with the owner generation, the asking account as decider, the
/// owner state and the runtime's configuration (exit-now and the burn locked under the
/// repository's switches); a malformed one (a request ID that is a flag among them) never
/// reaches the runtime. The controller is the macOS desktop app's (launchd, shlock).
#[cfg(target_os = "macos")]
#[test]
fn the_controller_hands_the_operators_ledger_decisions_to_the_runtime() {
    use darkbot_bsc_execution::live_ledger::{Fact, FactKind, Ledger};
    use darkbot_bsc_execution::sources::{ComponentId, LiveIntentKind};
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let dir = PathBuf::from(format!(
        "/private/tmp/darkbot-test.r3b{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    // Review N5: gone also when an assertion fails.
    struct Gone(PathBuf);
    impl Drop for Gone {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _gone = Gone(dir.clone());
    let current = dir.join("app/current");
    fs::create_dir_all(&current).unwrap();
    fs::create_dir_all(dir.join("home/.darkbot")).unwrap();
    fs::write(dir.join(".isolated"), b"").unwrap();
    std::os::unix::fs::symlink(
        env!("CARGO_BIN_EXE_bsc_runtime"),
        current.join("bsc_runtime"),
    )
    .unwrap();
    let config = dir.join("app/bsc-source/bsc/config");
    fs::create_dir_all(&config).unwrap();
    fs::copy(
        repo.join("bsc/config/ingress.public.env"),
        config.join("ingress.public.env"),
    )
    .unwrap();
    let state = dir.join("run/bsc-sources");
    fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(&state)
        .unwrap();
    activate(&state, &[], now_ms() + 3_600_000);
    let ledger = Ledger::open(&state);
    let buy = Fact {
        kind: FactKind::Released,
        tx_hash: [2; 32],
        intent: LiveIntentKind::Buy,
        component: ComponentId::StrategyC,
        token: [0xB; 20],
        nonce: 0,
        grant_generation: 1,
        input_wei: Some(U256::from(1_000u64)),
        max_wei: Some(U256::from(1_300u64)),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(100),
        by_tx: None,
        response: None,
        origin: None,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    };
    ledger.append(&buy, 1).unwrap();
    ledger
        .append(
            &Fact {
                block: Some((5, [5; 32], 5_000)),
                gas_wei: Some(U256::from(100u64)),
                tokens: Some(U256::from(7u64)),
                eth_wei: Some(U256::from(1_000u64)),
                ..Fact::about(&buy, FactKind::Included)
            },
            2,
        )
        .unwrap();
    let g = darkbot_bsc_execution::sources::current_generation(&state)
        .unwrap()
        .generation
        .to_string();
    let version = || ledger.version().unwrap().to_string();
    let lot = format!("0x{}", hex::encode([2u8; 32]));
    let ctl = |args: &[&str]| {
        // The system bash (3.2) that launchd's PATH finds for the desktop
        // app, under the CI runner's locale: its collation once let A-F
        // through a bash range [a-f] (the controller checks character lists).
        let out = Command::new("/bin/bash")
            .arg(repo.join("ops/darkbotctl"))
            .args(args)
            .env("DARKBOT_TEST_DIR", &dir)
            .env("DARKBOT_TEST_PORT", "18787")
            .env("HOME", dir.join("home"))
            .env("LC_ALL", "en_US.UTF-8")
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    };
    let me = String::from_utf8(Command::new("id").arg("-un").output().unwrap().stdout).unwrap();
    // A malformed decision never reaches the runtime; nothing is written.
    let upper = format!("0x{}", hex::encode([0xABu8; 32]).to_uppercase());
    let v = version();
    for bad in [
        vec!["bsc", "live", "lot", &upper, "WRITE_OFF", &v, "r-1"],
        vec!["bsc", "live", "lot", &lot, "SELL", &v, "r-1"],
        vec!["bsc", "live", "lot", &lot, "WRITE_OFF", "x", "r-1"],
        // Review L3: a request ID is never a flag of the runtime's.
        vec!["bsc", "live", "lot", &lot, "WRITE_OFF", &v, "--decided-by"],
        vec!["bsc", "live", "lot", &lot, "WRITE_OFF", &v, "r_1"],
        vec!["bsc", "live", "nonce-burn", &lot, &v],
        vec!["bsc", "live", "ledger-quarantine", "4x", &g, "r-1"],
    ] {
        let (code, out) = ctl(&bad);
        assert_ne!(code, 0, "{bad:?}: {out}");
        assert!(out.contains("darkbotctl: "), "{bad:?}: {out}");
    }
    assert_eq!(ledger.facts().unwrap().len(), 2);
    // Review M1: exit-now stays locked under the repository's switch (OFF).
    let (code, out) = ctl(&["bsc", "live", "lot", &lot, "EXIT_NOW", &v, "r-exit"]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("LIVE_OPERATOR_EXIT_OFF"), "{out}");
    // The lot written off at the ledger's version, by the asking account.
    let (code, out) = ctl(&["bsc", "live", "lot", &lot, "WRITE_OFF", &v, "r-off"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(&format!(
            "outcome=APPLIED lot={lot} decision=WRITTEN_OFF fact=2 version={v} request_id=r-off"
        )),
        "{out}"
    );
    let facts = ledger.facts().unwrap();
    let off = facts.last().unwrap();
    assert_eq!(off.kind, FactKind::WrittenOff);
    assert_eq!(off.operator.as_ref().unwrap().decided_by, me.trim());
    // At the version it saw before: refused, nothing written.
    let (code, out) = ctl(&["bsc", "live", "lot", &lot, "RESUME", &v, "r-stale"]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("LIVE_REQUEST_STALE"), "{out}");
    // The nonce burn stays locked under the repository's switch (OFF).
    let (code, out) = ctl(&["bsc", "live", "nonce-burn", &lot, &version(), "r-burn"]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("LIVE_NONCE_BURN_OFF"), "{out}");
    assert_eq!(ledger.facts().unwrap().len(), 3);
    // A torn entry quarantined at the owner generation.
    fs::write(
        state.join("live-ledger").join(format!("{:020}.dbrec", 3)),
        b"torn",
    )
    .unwrap();
    let (code, out) = ctl(&["bsc", "live", "ledger-quarantine", "3", &g, "r-q"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(&format!(
            "outcome=APPLIED seq=3 decision=QUARANTINED generation={g} request_id=r-q"
        )),
        "{out}"
    );
}

/// REV.1.0 R3 (BSC3-045, R1's stranded lots): the operator's commands
/// through the binary: `live lot resume` writes its fact once per request,
/// `exit-now` is refused while the configuration's operator exit is OFF,
/// `write-off` ends the lot (`live positions` shows it lost); `live ledger
/// check` names an entry that does not decode and `quarantine` moves it
/// out, after which the ledger checks clean; `live adopt-g07` without a
/// configured LIVE buyer is refused.
#[test]
fn the_operators_lot_and_ledger_commands_write_their_facts_once() {
    use darkbot_bsc_execution::live_ledger::{Fact, FactKind, Ledger};
    use darkbot_bsc_execution::sources::{ComponentId, LiveIntentKind};
    let state = scratch("operator-cli");
    activate(&state, &[], now_ms() + 3_600_000);
    let s = state.to_str().unwrap();
    let config = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../config/ingress.public.env"
    );
    let ledger = Ledger::open(&state);
    let buy = Fact {
        kind: FactKind::Released,
        tx_hash: [1; 32],
        intent: LiveIntentKind::Buy,
        component: ComponentId::StrategyC,
        token: [0xA; 20],
        nonce: 0,
        grant_generation: 1,
        input_wei: Some(U256::from(1_000u64)),
        max_wei: Some(U256::from(1_300u64)),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(100),
        by_tx: None,
        response: None,
        origin: None,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    };
    ledger.append(&buy, 1).unwrap();
    let included = Fact {
        block: Some((5, [5; 32], 5_000)),
        gas_wei: Some(U256::from(100u64)),
        tokens: Some(U256::from(7u64)),
        eth_wei: Some(U256::from(1_000u64)),
        ..Fact::about(&buy, FactKind::Included)
    };
    ledger.append(&included, 2).unwrap();
    let lot = format!("0x{}", hex::encode([1u8; 32]));
    // R3b: a lot's decision and a nonce burn expect the ledger's version, a
    // quarantine the owner generation (each as the LIVE tab shows it).
    let version = || ledger.version().unwrap().to_string();
    let decide_at = |what: &str, id: &str, expected: &str| {
        cli(&[
            "live",
            "lot",
            what,
            "--lot",
            &lot,
            "--expect-version",
            expected,
            "--state",
            s,
            "--config",
            config,
            "--request-id",
            id,
            "--decided-by",
            "operator",
        ])
    };
    let decide = |what: &str, id: &str| decide_at(what, id, &version());
    let (code, out) = decide_at("resume", "resume-0", "3");
    assert_ne!(code, 0, "{out}");
    assert!(
        out.contains("LIVE_REQUEST_STALE: the ledger's version is 2, not 3"),
        "{out}"
    );
    let (code, out) = decide("resume", "resume-1");
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(&format!(
            "outcome=APPLIED lot={lot} decision=RESUMED fact=2 version=2 request_id=resume-1"
        )),
        "{out}"
    );
    // The same request again: its replay, though the ledger moved since.
    let (code, out) = decide_at("resume", "resume-1", "2");
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(&format!(
            "outcome=APPLIED lot={lot} decision=RESUMED replay=true request_id=resume-1"
        )),
        "{out}"
    );
    let (code, out) = decide("exit-now", "exit-1");
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("LIVE_OPERATOR_EXIT_OFF"), "{out}");
    let (code, out) = decide("write-off", "off-1");
    assert_eq!(code, 0, "{out}");
    let (_, out) = cli(&["live", "positions", "--state", s]);
    assert!(
        out.contains("state=WRITTEN_OFF") && out.contains("pnl_wei=-1100"),
        "{out}"
    );
    // A torn entry: the check names it, the operator quarantines it.
    let check = || cli(&["live", "ledger", "check", "--state", s]);
    let (code, out) = check();
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("entries=4 unreadable=0"), "{out}");
    fs::write(
        state.join("live-ledger").join(format!("{:020}.dbrec", 4)),
        b"torn",
    )
    .unwrap();
    let (code, out) = check();
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("unreadable seq=4"), "{out}");
    let g = darkbot_bsc_execution::sources::current_generation(&state)
        .unwrap()
        .generation;
    let quarantine = |expected: u64| {
        cli(&[
            "live",
            "ledger",
            "quarantine",
            "--seq",
            "4",
            "--expect-generation",
            &expected.to_string(),
            "--state",
            s,
            "--request-id",
            "q-4",
            "--decided-by",
            "operator",
        ])
    };
    let (code, out) = quarantine(g + 1);
    assert_ne!(code, 0, "{out}");
    assert!(
        out.contains(&format!(
            "LIVE_REQUEST_STALE: the owner generation is {g}, not {}",
            g + 1
        )),
        "{out}"
    );
    let (code, out) = quarantine(g);
    assert_eq!(code, 0, "{out}");
    let applied =
        format!("outcome=APPLIED seq=4 decision=QUARANTINED generation={g} request_id=q-4");
    assert!(out.contains(&applied), "{out}");
    let (code, out) = check();
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(&format!(
            "entries=4 unreadable=0 quarantined_at_generation={g}"
        )),
        "{out}"
    );
    // The owner generation moves on; the same quarantine again is its
    // replay at the generation recorded.
    let (code, out) = cli(&[
        "source",
        "request",
        "BSC_ECON_SIGN",
        "DISABLED",
        "--expect-generation",
        &g.to_string(),
        "--request-id",
        "e2e-bump",
        "--state",
        s,
    ]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = quarantine(g + 1);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(&applied), "{out}");
    let (code, out) = cli(&[
        "live",
        "adopt-g07",
        "--g08",
        "none.dbrec",
        "--capture",
        "none.journal",
        "--state",
        s,
        "--config",
        config,
        "--request-id",
        "adopt-1",
        "--decided-by",
        "operator",
    ]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("BSC_LIVE_BUYER is not configured"), "{out}");
    // R3b: a BUY whose window is unknown (its bytes may land at any height).
    let unknown = Fact {
        tx_hash: [9; 32],
        nonce: 1,
        max_block: None,
        ..buy.clone()
    };
    ledger.append(&unknown, 3).unwrap();
    let tx = format!("0x{}", hex::encode([9u8; 32]));
    let burn = |config: &str, expected: &str, id: &str| {
        cli(&[
            "live",
            "nonce-burn",
            "--tx",
            &tx,
            "--expect-version",
            expected,
            "--state",
            s,
            "--config",
            config,
            "--request-id",
            id,
            "--decided-by",
            "operator",
        ])
    };
    // Refused under the repository's switch (OFF).
    let (code, out) = burn(config, &version(), "burn-1");
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("LIVE_NONCE_BURN_OFF"), "{out}");
    // A configuration with the switch ON and its LIVE buyer: at another
    // version refused, at the ledger's written once.
    let buyer = "0x00000000000000000000000000000000000000aB";
    let on = fs::read_to_string(config)
        .unwrap()
        .replace(
            "BSC_STATE_WATCHED=\n",
            &format!("BSC_STATE_WATCHED={buyer}\n"),
        )
        .replace(
            "BSC_EXEC_LIVE_NONCE_BURN=OFF\n",
            "BSC_EXEC_LIVE_NONCE_BURN=ON\n",
        );
    let on_path = state.with_file_name(format!("burn-on-{}.env", std::process::id()));
    fs::write(
        &on_path,
        format!(
            "{on}BSC_LIVE_KEY=~/.darkbot/bsc/live/buyer.key\nBSC_LIVE_BUYER={buyer}\n\
             BSC_LIVE_ROUTE=LOOPBACK:18545\nBSC_LIVE_CONNECT_TIMEOUT_MS=5000\n"
        ),
    )
    .unwrap();
    let on_config = on_path.to_str().unwrap();
    let (code, out) = burn(on_config, "5", "burn-2");
    assert_ne!(code, 0, "{out}");
    assert!(
        out.contains("LIVE_REQUEST_STALE: the ledger's version is 6, not 5"),
        "{out}"
    );
    let (code, out) = burn(on_config, &version(), "burn-2");
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(&format!(
            "outcome=APPLIED tx={tx} decision=BURN_REQUESTED fact=6 version=6 request_id=burn-2"
        )),
        "{out}"
    );
    fs::remove_file(on_path).unwrap();
    fs::remove_dir_all(state).unwrap();
}

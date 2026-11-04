use super::*;
use alloy_primitives::{hex, keccak256};
use darkbot_bsc_trading::economics::flap::native_curve20::WITNESSED_MAINNET_TERMS;
use darkbot_bsc_trading::signer::Signer;
use darkbot_bsc_trading::transactions::legacy::{LegacyTx, decode_signed};
use darkbot_bsc_trading::venues::flap::abi::{
    LaunchParamsV6, decode_swap_exact_input_native, encode_new_token_v6,
};
use darkbot_bsc_trading::venues::flap::deployment::{ObservedDeployment, bsc_mainnet_v5_27_2};
use std::cell::Cell;

const POLICY_GENERATION: u64 = 3;
const CODEHASH_GENERATION: u64 = 5;
const CLOCK_ID: u32 = 1;

pub(crate) fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bsc-paper-exec-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    d
}

/// The current deployment (economics reconfirmed on it).
pub(crate) fn validated() -> ValidatedDeployment {
    let d = bsc_mainnet_v5_27_2();
    d.validate(
        &ObservedDeployment {
            chain_id: d.chain_id,
            portal_version: d.portal_version,
            portal_runtime_sha256: d.portal_runtime_sha256,
            portal_implementation: d.portal_implementation,
            portal_implementation_runtime_sha256: d.portal_implementation_runtime_sha256,
            tax_v3_implementation_runtime_sha256: d.tax_v3_implementation_runtime_sha256,
        },
        CODEHASH_GENERATION,
    )
    .unwrap()
}

pub(crate) fn config(lanes: usize, buys: u128) -> ExecConfig {
    let gas_price_wei = 1_000_000_000;
    let gas_limit = 300_000;
    let one = 1_000_000_000_000_000 + u128::from(gas_price_wei) * u128::from(gas_limit);
    ExecConfig {
        // The amount covered by witnessed evidence.
        input_wei: 1_000_000_000_000_000,
        slippage_bps: 500,
        exit_slippage_bps: 500,
        gas_price_wei,
        gas_limit,
        session_cap_wei: one * buys,
        lanes,
        max_calldata: 4_096,
        latency_samples: 64,
        policy_generation: POLICY_GENERATION,
        capability_generation: 2,
        validity_blocks: 2,
        start_limit_ns: 1_000_000,
        max_frame: 16_384,
        strategy_lanes: lanes,
        strategy_session_cap_wei: 100_000_000_000_000_000,
        approve_gas_limit: 100_000,
        sell_gas_limit: 1_500_000,
        strategy_max_head_age_ms: 30_000,
        live_max_head_age_ms: 86_400_000,
        live_exit_reverts_max: 2,
        live_lease_blocks: 200,
        live_operator_exit: false,
        live_nonce_burn: false,
        // The test rigs' sources are public: the variant ON (R4 tests OFF).
        live_public_only: true,
        live_grant_max_duration_ms: 0,
        live_qualification_max_age_ms: 0,
        live_presence_anchor: None,
        live_spend_total_wei: u128::from(u64::MAX),
        // No test grant is an outlier.
        live_risk_soft_buy_slippage_bps: 9_999,
        live_risk_soft_exit_slippage_bps: 9_999,
        live_risk_soft_gas_price_wei: u64::MAX,
        live_risk_outlier: false,
        live_c3_rearm: false,
        live_builder_policy: crate::builder_policy::TEST_POLICY.into(),
        live_exit_reprice_bps: 0,
        live_public_exit: false,
        live_public_exit_rpc: String::new(),
        live_dex_exit: false,
        live_divergence_quarantine: false,
        sender_paths: SenderPaths::Both,
    }
}

/// The witnessed native self-launch (the one qualified profile) with
/// `salt`.
fn calldata(salt: u8) -> Vec<u8> {
    let mut beneficiary = [0u8; 20];
    beneficiary[18] = 0xa1;
    beneficiary[19] = 0x1c;
    let mut s = hex!("09c4b20c7956530d4bf91b3569ed173abfce47d16d412d94dde5e6f4034c6eeb");
    s[31] ^= salt;
    let launch = LaunchParamsV6 {
        name: b"DarkBot G06A Economics",
        symbol: b"DBG06E",
        uri: b"ipfs://darkbot-g06a-economics",
        dex_thresh: 1,
        salt: B256::from(s),
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
    let mut out = Vec::new();
    encode_new_token_v6(&launch, &mut out);
    out
}

/// A launch really signed by a third party: (hash, signed bytes, input).
pub(crate) fn launch(salt: u8, nonce: u64) -> ([u8; 32], Vec<u8>, Vec<u8>) {
    let launcher = Signer::from_secret_bytes(&[0x5a; 32]).unwrap();
    let input = calldata(salt);
    let (mut scratch, mut raw) = (Vec::new(), Vec::new());
    launcher
        .sign_legacy(
            &LegacyTx {
                chain_id: 56,
                nonce,
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
    (keccak256(&raw).0, raw, input)
}

fn snapshot(candidate: B256, policy: u64, codehash: u64) -> Option<SnapshotId> {
    Some(SnapshotId {
        canonical_block_hash: B256::repeat_byte(0xb1),
        ordered_state_cursor: 9,
        pending_candidate: candidate,
        policy_generation: policy,
        codehash_generation: codehash,
    })
}

pub(crate) fn c2(head: Option<ChainHead>) -> LaunchContext<'static> {
    LaunchContext {
        snapshot: &snapshot,
        head,
        next_nonce: None,
        exit_reserve_wei: None,
    }
}

/// [`c2`] with the sender's canonical next nonce `next` (a LIVE end).
pub(crate) fn c2_next(head: Option<ChainHead>, next: u64) -> LaunchContext<'static> {
    LaunchContext {
        next_nonce: Some(next),
        exit_reserve_wei: None,
        ..c2(head)
    }
}

pub(crate) const HEAD: ChainHead = ChainHead {
    number: 129_900_000,
    timestamp: 1_793_000_000,
};

fn armed(name: &str, config: ExecConfig) -> (PaperExecutor, PathBuf) {
    let d = dir(name);
    let key = d.join("identity/paper.key");
    PaperIdentity::create(&key, Path::new("/nonexistent-repo")).unwrap();
    let identity = PaperIdentity::load(&key, Path::new("/nonexistent-repo"), &[]).unwrap();
    let exec = PaperExecutor::arm(
        identity,
        validated(),
        WITNESSED_MAINNET_TERMS,
        config,
        Arming {
            next_nonce: 0,
            fence_dir: &d.join("fences"),
            session: 1,
            arm: 0,
        },
    )
    .unwrap();
    (exec, d)
}

pub(crate) fn clock(start: u64, step: u64) -> FnClock<impl Fn() -> u64> {
    let now = Cell::new(start);
    FnClock {
        id: CLOCK_ID,
        now: move || {
            now.set(now.get() + step);
            now.get()
        },
    }
}

/// C3 (gate review H1): a really signed launch runs the whole production
/// path: decoder, C2 snapshot, admission, BUY build and signature, lane,
/// the submission plan of the 48Club bundle with the original launch,
/// its request frame and the diagnostic dispatch, whose transport is the
/// sink. The BUY is the admitted one; the sink took the whole request.
#[test]
fn a_signed_launch_reaches_the_sink_through_plan_frame_and_dispatch() {
    let (mut exec, d) = armed("path", config(2, 2));
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let (hash, raw, input) = launch(0, 7);
    let buy = exec
        .on_launch(
            PendingLaunch {
                hash,
                raw: &raw,
                to: Some(portal),
                input: &input,
                ingress_ns: 100,
            },
            &c2(Some(HEAD)),
            &clock(100, 7),
        )
        .unwrap();
    let signed = decode_signed(&buy.raw).unwrap();
    assert_eq!(signed.tx.chain_id, 56);
    assert_eq!(signed.tx.nonce, 0);
    assert_eq!(signed.tx.to.into_array(), portal);
    assert_eq!(signed.tx.value, U256::from(1_000_000_000_000_000u64));
    let call = decode_swap_exact_input_native(signed.tx.input).unwrap();
    // CREATE2 of the witnessed salt (G0.6a EVM crosscheck token).
    assert_eq!(
        call.output_token.into_array(),
        hex!("e51c4deb1038afcd95ef12d4a649cdbcf6fa7777")
    );
    assert_eq!(buy.token, call.output_token.into_array());
    // The request carries both transactions hex-encoded, and a head.
    assert!(buy.handoff.raw_len > 2 * (raw.len() + buy.raw.len()));
    let t = buy.times;
    assert!(t.ingress_ns < t.decoded_ns && t.decoded_ns < t.admitted_ns);
    assert!(t.admitted_ns < t.signed_ns && t.signed_ns < t.prepared_ns);
    assert!(t.prepared_ns < t.planned_ns);
    assert!(t.planned_ns < t.sink_ns);
    assert_eq!(exec.counters().handoffs, 1);
    assert_eq!(exec.lanes(), (2, 1));
    assert_eq!(exec.spent_wei(), exec.budget.max_total_wei);
    assert_eq!(
        exec.latencies().quantiles().map(|q| q.2),
        Some(t.sink_ns - 100)
    );
    // One fence for the arm, removed when it is disarmed.
    let fence = exec.fence().to_path_buf();
    assert!(fence.exists());
    exec.disarm().unwrap();
    assert!(!fence.exists());
    fs::remove_dir_all(&d).unwrap();
}

/// Gate review M1: the pool keeps one session exposure and one BUY per
/// token, as one production session does (before: every lane its own
/// cap and dedup, so a second BUY of the same token reached the sink).
#[test]
fn the_pool_keeps_one_session_cap_and_one_buy_per_token() {
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let (mut exec, d) = armed("session", config(4, 2));
    let (h1, r1, i1) = launch(0, 1);
    let (h2, r2, i2) = launch(0, 2); // another launch of the same token (R6: a conflict)
    let (h3, r3, i3) = launch(1, 3);
    let (h4, r4, i4) = launch(2, 4);
    let ok = |e: &mut PaperExecutor, h, r: &[u8], i: &[u8]| {
        e.on_launch(
            PendingLaunch {
                hash: h,
                raw: r,
                to: Some(portal),
                input: i,
                ingress_ns: 100,
            },
            &c2(Some(HEAD)),
            &clock(100, 1),
        )
    };
    assert!(ok(&mut exec, h1, &r1, &i1).is_ok());
    assert_eq!(ok(&mut exec, h2, &r2, &i2), Err(Refusal::TokenConflict));
    // The same launch again is a duplicate, not a conflict.
    assert_eq!(ok(&mut exec, h1, &r1, &i1), Err(Refusal::DuplicateToken));
    assert!(ok(&mut exec, h3, &r3, &i3).is_ok());
    // Two BUYs exhaust a cap of two, lanes left or not.
    assert_eq!(ok(&mut exec, h4, &r4, &i4), Err(Refusal::SessionCap));
    assert_eq!(exec.lanes(), (4, 2));
    let c = exec.counters();
    assert_eq!(
        (
            c.handoffs,
            c.refused("DUPLICATE_TOKEN"),
            c.refused("SESSION_CAP")
        ),
        (2, 1, 1)
    );
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// Every other outcome is a counted refusal at its stage.
#[test]
fn refusals_are_counted_at_their_stage() {
    let (mut exec, d) = armed("refuse", config(2, 8));
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let (hash, raw, input) = launch(0, 1);
    let ctx = c2(Some(HEAD));
    let clk = clock(100, 1);
    assert_eq!(
        exec.on_launch(
            PendingLaunch {
                hash,
                raw: &raw,
                to: Some([7; 20]),
                input: &input,
                ingress_ns: 100
            },
            &ctx,
            &clk
        ),
        Err(Refusal::NotDecoded)
    );
    let foreign = hex::decode(
        include_str!("../../trading/fixtures/flap/launch_v6_native_split_tax_pending.hex").trim(),
    )
    .unwrap();
    assert!(matches!(
        exec.on_launch(
            PendingLaunch {
                hash,
                raw: &raw,
                to: Some(portal),
                input: &foreign,
                ingress_ns: 100
            },
            &ctx,
            &clk
        ),
        Err(Refusal::Profile(_))
    ));
    let none = LaunchContext {
        snapshot: &|_, _, _| None,
        head: Some(HEAD),
        next_nonce: None,
        exit_reserve_wei: None,
    };
    assert_eq!(
        exec.on_launch(
            PendingLaunch {
                hash,
                raw: &raw,
                to: Some(portal),
                input: &input,
                ingress_ns: 100
            },
            &none,
            &clk
        ),
        Err(Refusal::NoSnapshot)
    );
    assert_eq!(
        exec.on_launch(
            PendingLaunch {
                hash,
                raw: &raw,
                to: Some(portal),
                input: &input,
                ingress_ns: 100
            },
            &c2(None),
            &clk
        ),
        Err(Refusal::NoSnapshot)
    );
    // The bundle's launch must be the candidate the BUY was admitted
    // behind: other signed bytes are refused by the plan (one lane used).
    let (_, other_raw, _) = launch(0, 9);
    assert_eq!(
        exec.on_launch(
            PendingLaunch {
                hash,
                raw: &other_raw,
                to: Some(portal),
                input: &input,
                ingress_ns: 100
            },
            &ctx,
            &clk
        ),
        Err(Refusal::Plan)
    );
    // The start deadline after T_ingress passed: no permit, no exposure.
    let late = clock(100 + 2_000_000, 1);
    let (h2, r2, i2) = launch(1, 2);
    assert_eq!(
        exec.on_launch(
            PendingLaunch {
                hash: h2,
                raw: &r2,
                to: Some(portal),
                input: &i2,
                ingress_ns: 100
            },
            &ctx,
            &late
        ),
        Err(Refusal::Deadline)
    );
    assert_eq!(exec.spent_wei(), U256::ZERO);
    let (h3, r3, i3) = launch(2, 3);
    assert_eq!(
        exec.on_launch(
            PendingLaunch {
                hash: h3,
                raw: &r3,
                to: Some(portal),
                input: &i3,
                ingress_ns: 100
            },
            &ctx,
            &clk
        ),
        Err(Refusal::LanesExhausted)
    );
    exec.refuse_undecodable();
    let c = exec.counters();
    assert_eq!((c.launches, c.handoffs), (8, 0));
    for (stage, n) in [
        ("NOT_DECODED", 2),
        ("PROFILE", 1),
        ("NO_SNAPSHOT", 2),
        ("PLAN", 1),
        ("DEADLINE", 1),
        ("LANES_EXHAUSTED", 1),
    ] {
        assert_eq!(c.refused(stage), n, "{stage}");
    }
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// V2 §7.4 and gate review L7: the modules that hold signed PAPER bytes
/// (the executor, the runtime controller, the sink) name no socket,
/// stream, TLS connection, release dispatch or provider client; the
/// executor's only transport is the sink. The controller's probe thread
/// is given the endpoint, the bounds and the sender address only.
#[test]
fn no_module_with_paper_bytes_reaches_a_transport() {
    let code = |source: &'static str| &source[..source.find("#[cfg(test)]").unwrap()];
    let common = [
        "TcpStream",
        "UdpSocket",
        "UnixStream",
        "std::net",
        "tungstenite",
        "rustls",
        "Command::new",
        "WarmTlsConnection",
        "TlsConnectionOwner",
        "TlsTransport",
        "qualify_transport",
        "QualifiedTransport",
        "DiagnosticStreamTransport",
        "dispatch_release",
        "rpc_wire",
        "crate::wss",
    ];
    let executor = code(include_str!("paper_exec.rs"));
    let controller = code(include_str!("runtime_exec.rs"));
    let sink = code(include_str!("paper.rs"));
    for (name, source) in [
        ("paper_exec", executor),
        ("runtime_exec", controller),
        ("paper", sink),
    ] {
        for forbidden in common {
            assert!(!source.contains(forbidden), "{name}: {forbidden}");
        }
    }
    // The executor implements exactly one transport: the sink.
    assert_eq!(executor.matches("impl Transport for").count(), 1);
    assert!(executor.contains("impl Transport for SinkTransport"));
    // The controller dispatches nothing and its probe thread sees no
    // signed bytes.
    assert!(!controller.contains("crate::submission::send"));
    let probe = &controller[controller.find("fn spawn_probe").unwrap()..];
    let probe = &probe[..probe.find("\n    }\n").unwrap()];
    for forbidden in ["raw", "PaperBuy", "executor", "signer"] {
        assert!(!probe.contains(forbidden), "probe thread: {forbidden}");
    }
}

/// Gate review L3: keys that could never admit a BUY are refused at
/// arming (before: the path showed ARMED and refused every launch).
#[test]
fn a_configuration_that_cannot_admit_is_refused_at_arming() {
    let good = config(1, 1);
    assert_eq!(good.validate(), Ok(()));
    for bad in [
        ExecConfig {
            input_wei: 2_000_000_000_000_000,
            ..good.clone()
        },
        ExecConfig {
            session_cap_wei: good.session_cap_wei - 1,
            ..good.clone()
        },
        ExecConfig {
            validity_blocks: max_validity_blocks() + 1,
            ..good.clone()
        },
        ExecConfig {
            validity_blocks: 0,
            ..good.clone()
        },
        ExecConfig {
            start_limit_ns: max_start_limit_ns() + 1,
            ..good.clone()
        },
        ExecConfig {
            lanes: 0,
            ..good.clone()
        },
    ] {
        assert!(bad.validate().is_err(), "{bad:?}");
    }
}

/// Gate review M2: every arm has its own fence, so the component can be
/// armed again in the same session (before: `<session>-<lane>` collided
/// and re-arming failed with "File exists").
#[test]
fn the_same_session_arms_again_with_a_new_fence() {
    let (exec, d) = armed("rearm", config(1, 1));
    let key = d.join("identity/paper.key");
    let load = || PaperIdentity::load(&key, Path::new("/nonexistent-repo"), &[]).unwrap();
    let arm = |n| {
        PaperExecutor::arm(
            load(),
            validated(),
            WITNESSED_MAINNET_TERMS,
            config(1, 1),
            Arming {
                next_nonce: 0,
                fence_dir: &d.join("fences"),
                session: 1,
                arm: n,
            },
        )
    };
    // The same arm twice is refused; the next arm is not.
    assert!(matches!(arm(0), Err(ArmError::Fence(_))));
    exec.disarm().unwrap();
    let again = arm(1).unwrap();
    assert_eq!(fs::read_dir(d.join("fences")).unwrap().count(), 1);
    again.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// The canonical snapshot of C2 (Phase I: no pending candidate).
fn canonical(policy: u64, codehash: u64) -> Option<SnapshotId> {
    snapshot(B256::ZERO, policy, codehash)
}

/// Now: one second after the test head.
const NOW_MS: u64 = 1_793_000_001_000;

pub(crate) fn strategy_c2(head: Option<ChainHead>) -> CanonicalContext<'static> {
    CanonicalContext {
        snapshot: &canonical,
        head,
        now_unix_ms: NOW_MS,
        next_nonce: None,
        exit_reserve_wei: None,
        exit_gas_price_wei: None,
    }
}

/// [`strategy_c2`] with the sender's canonical next nonce `next` (a
/// LIVE end).
pub(crate) fn strategy_c2_next(head: Option<ChainHead>, next: u64) -> CanonicalContext<'static> {
    CanonicalContext {
        next_nonce: Some(next),
        exit_reserve_wei: None,
        exit_gas_price_wei: None,
        ..strategy_c2(head)
    }
}

/// A curve after real BUYs of `wei` from the fresh curve, and the
/// quote of a BUY of `input` on it.
pub(crate) fn curve_after(wei: u128) -> StrategyCurve {
    use darkbot_bsc_trading::economics::flap::native_curve20::{
        HoldingCap, NativeBuyInputs, profile_terms, quote_native_buy,
    };
    let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
    let q = quote_native_buy(&NativeBuyInputs {
        token: terms,
        reserve_before: U256::ZERO,
        circulating_before: U256::ZERO,
        input_amount: U256::from(wei),
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
        holding_cap: HoldingCap::None,
    })
    .unwrap();
    StrategyCurve {
        terms,
        reserve: q.reserve_after,
        circulating: q.supply_after,
    }
}

/// Phase I: a Strategy C PASS runs the production path to the sink: the
/// curve admission, the BUY build and PAPER signature, a strategy lane,
/// the local node's single-raw plan, its frame and the dispatch. The
/// exit then signs the approval of the Portal (next nonce) and the
/// native-output SELL of the position's tokens (the nonce after); both
/// reach the sink. Nothing is sent.
#[test]
fn a_strategy_entry_and_exit_reach_the_sink() {
    use darkbot_bsc_trading::venues::flap::abi::decode_approve;
    use darkbot_bsc_trading::venues::flap::recognize::decode_swap_exact_input;
    let mut cfg = config(2, 2);
    cfg.strategy_lanes = 4;
    let (mut exec, d) = armed("strategy", cfg);
    let portal = bsc_mainnet_v5_27_2().portal;
    let token = [0xAB; 20];
    let input = U256::from(10_000_000_000_000_000u64);
    let curve = curve_after(1_000_000_000_000_000_000);
    let buy = exec
        .on_strategy_buy(
            token,
            [0; 20],
            curve,
            input,
            &strategy_c2(Some(HEAD)),
            &clock(500, 3),
            500,
        )
        .unwrap();
    assert_eq!((buy.kind, buy.token), (IntentKind::Buy, token));
    let signed = decode_signed(&buy.raw).unwrap();
    assert_eq!((signed.tx.chain_id, signed.tx.nonce), (56, 0));
    assert_eq!((signed.tx.to, signed.tx.value), (portal, input));
    let call = decode_swap_exact_input_native(signed.tx.input).unwrap();
    assert_eq!(call.output_token.into_array(), token);
    // The single raw transaction is the whole request.
    assert!(buy.handoff.raw_len >= buy.raw.len());
    let t = buy.times;
    assert!(t.ingress_ns <= t.admitted_ns && t.admitted_ns < t.signed_ns);
    assert!(t.signed_ns < t.prepared_ns && t.prepared_ns < t.planned_ns);
    assert!(t.planned_ns < t.sink_ns);
    // One BUY per token.
    assert_eq!(
        exec.on_strategy_buy(
            token,
            [0; 20],
            curve,
            input,
            &strategy_c2(Some(HEAD)),
            &clock(600, 3),
            600
        ),
        Err(Refusal::DuplicateToken)
    );
    // The exit of the tokens bought, on the curve with them in it.
    let tokens = call.min_output_amount;
    let after = StrategyCurve {
        reserve: curve.reserve + input,
        circulating: curve.circulating + tokens,
        ..curve
    };
    let curve_with_buy = {
        use darkbot_bsc_trading::economics::flap::native_curve20::{
            HoldingCap, NativeBuyInputs, quote_native_buy,
        };
        let q = quote_native_buy(&NativeBuyInputs {
            token: curve.terms,
            reserve_before: curve.reserve,
            circulating_before: curve.circulating,
            input_amount: input,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: HoldingCap::None,
        })
        .unwrap();
        StrategyCurve {
            reserve: q.reserve_after,
            circulating: q.supply_after,
            ..after
        }
    };
    let (exits, refused) = exec.on_strategy_exit(
        token,
        tokens,
        curve_with_buy,
        &strategy_c2(Some(HEAD)),
        &clock(900, 3),
        900,
    );
    assert_eq!(refused, None);
    let [approve, sell]: [PaperIntent; 2] = exits.try_into().unwrap();
    // Gate M3: the sender's sequence, BUY 0, approval 1, SELL 2.
    let a = decode_signed(&approve.raw).unwrap();
    assert_eq!(
        (approve.kind, a.tx.nonce, a.tx.to.into_array()),
        (IntentKind::Approve, 1, token)
    );
    assert_eq!(a.tx.value, U256::ZERO);
    assert_eq!(decode_approve(a.tx.input).unwrap(), (portal, tokens));
    let s = decode_signed(&sell.raw).unwrap();
    assert_eq!(
        (sell.kind, s.tx.nonce, s.tx.to),
        (IntentKind::Sell, 2, portal)
    );
    let swap = decode_swap_exact_input(s.tx.input).unwrap();
    assert!(swap.is_sell());
    assert_eq!(
        (swap.input_token.into_array(), swap.input_amount),
        (token, tokens)
    );
    assert!(sell.handoff.sequence > approve.handoff.sequence);
    // One exit per token; the counters and lanes say so.
    assert_eq!(
        exec.on_strategy_exit(
            token,
            tokens,
            curve_with_buy,
            &strategy_c2(Some(HEAD)),
            &clock(990, 3),
            990
        ),
        (Vec::new(), Some(Refusal::DuplicateToken))
    );
    assert_eq!(exec.strategy_counters().handoffs, 3);
    assert_eq!(exec.strategy_counters().refused("DUPLICATE_TOKEN"), 2);
    let (armed_lanes, left, next_nonce, spent, gap) = exec.strategy_lanes();
    assert_eq!((armed_lanes, left, next_nonce, gap), (4, 1, 3, false));
    // Gate L1: the closed position's input left the exposure; the gas
    // stays.
    assert!(spent > U256::ZERO && spent < input, "{spent}");
    // C3's own pool is untouched.
    assert_eq!(exec.lanes(), (2, 2));
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// Phase I: without a coherent canonical snapshot or a head nothing is
/// admitted; an entry above the strategy's session cap is refused.
#[test]
fn a_strategy_entry_needs_a_snapshot_and_room_in_its_cap() {
    let mut cfg = config(2, 2);
    cfg.strategy_session_cap_wei = 10_000_000_000_000_000;
    let (mut exec, d) = armed("strategy-refusals", cfg);
    let curve = curve_after(1_000_000_000_000_000_000);
    let input = U256::from(10_000_000_000_000_000u64);
    let none = CanonicalContext {
        snapshot: &|_, _| None,
        head: Some(HEAD),
        now_unix_ms: NOW_MS,
        next_nonce: None,
        exit_reserve_wei: None,
        exit_gas_price_wei: None,
    };
    assert_eq!(
        exec.on_strategy_buy([1; 20], [0; 20], curve, input, &none, &clock(1, 1), 1),
        Err(Refusal::NoSnapshot)
    );
    assert_eq!(
        exec.on_strategy_buy(
            [1; 20],
            [0; 20],
            curve,
            input,
            &strategy_c2(None),
            &clock(1, 1),
            1
        ),
        Err(Refusal::NoSnapshot)
    );
    // Gate L-a: a head older than the bound (30 s) is not acted on; a
    // follower catching up does not trade old blocks as fresh.
    let late = CanonicalContext {
        snapshot: &canonical,
        head: Some(HEAD),
        now_unix_ms: HEAD.timestamp * 1_000 + 30_001,
        next_nonce: None,
        exit_reserve_wei: None,
        exit_gas_price_wei: None,
    };
    assert_eq!(
        exec.on_strategy_buy([1; 20], [0; 20], curve, input, &late, &clock(1, 1), 1),
        Err(Refusal::StaleHead)
    );
    // Input plus gas is above the cap.
    assert_eq!(
        exec.on_strategy_buy(
            [1; 20],
            [0; 20],
            curve,
            input,
            &strategy_c2(Some(HEAD)),
            &clock(1, 1),
            1
        ),
        Err(Refusal::SessionCap)
    );
    assert_eq!(exec.strategy_counters().handoffs, 0);
    assert_eq!(exec.strategy_lanes().1, 2);
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// BSC3-072: the strategy's session cap keeps the room the exit of
/// what it buys needs (its approval and SELL at the signed gas): a BUY
/// whose exit would not fit is refused, before anything is signed
/// (before: the BUY went and its exit was refused SESSION_CAP, the
/// position stranded).
#[test]
fn a_buy_leaves_room_in_the_cap_for_its_exit() {
    let input = U256::from(10_000_000_000_000_000u64);
    let curve = curve_after(1_000_000_000_000_000_000);
    let token = [0xAB; 20];
    let at = |cap: U256| {
        let mut cfg = config(2, 2);
        cfg.strategy_lanes = 4;
        cfg.strategy_session_cap_wei = u128::try_from(cap).unwrap();
        cfg
    };
    let cfg = config(2, 2);
    let price = U256::from(cfg.gas_price_wei);
    let buy = input + price * U256::from(cfg.gas_limit);
    let exit = price * U256::from(cfg.approve_gas_limit + cfg.sell_gas_limit);
    // Room for the BUY, not for its exit.
    let (mut exec, d) = armed("strategy-exit-room", at(buy + exit - U256::from(1u8)));
    let bought = exec.on_strategy_buy(
        token,
        [0; 20],
        curve,
        input,
        &strategy_c2(Some(HEAD)),
        &clock(500, 3),
        500,
    );
    assert_eq!(bought.err(), Some(Refusal::SessionCap));
    assert_eq!(exec.strategy_counters().handoffs, 0);
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
    // Room for both: the BUY and then its exit go.
    let (mut exec, d) = armed("strategy-exit-room-2", at(buy + exit));
    let bought = exec
        .on_strategy_buy(
            token,
            [0; 20],
            curve,
            input,
            &strategy_c2(Some(HEAD)),
            &clock(500, 3),
            500,
        )
        .unwrap();
    let tokens = decode_swap_exact_input_native(decode_signed(&bought.raw).unwrap().tx.input)
        .unwrap()
        .min_output_amount;
    let (exits, refused) = exec.on_strategy_exit(
        token,
        tokens,
        curve,
        &strategy_c2(Some(HEAD)),
        &clock(900, 3),
        900,
    );
    assert_eq!((exits.len(), refused), (2, None));
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// BSC3-085: the exit reserve a BUY leaves room for is counted as the
/// positions change (BUY, approval, SELL, a reopened exit with or
/// without its approval, an adopted position and approval), always the
/// scan of every position (which every executor test also checks at
/// each recount).
#[test]
fn the_open_exits_are_counted_as_the_positions_change() {
    let input = U256::from(10_000_000_000_000_000u64);
    let curve = curve_after(1_000_000_000_000_000_000);
    let mut cfg = config(2, 2);
    cfg.strategy_lanes = 16;
    let price = U256::from(cfg.gas_price_wei);
    let (approval, sell) = (
        price * U256::from(cfg.approve_gas_limit),
        price * U256::from(cfg.sell_gas_limit),
    );
    let (mut exec, d) = armed("strategy-open-exits", cfg);
    let same = |exec: &PaperExecutor, want: U256, step: &str| {
        assert_eq!(exec.held_exit_reserve_scan(), want, "{step}: scan");
        assert_eq!(exec.held_exit_reserve(), want, "{step}: counted");
    };
    let mut held = Vec::new();
    for n in 1..=3u8 {
        let token = [n; 20];
        let bought = exec
            .on_strategy_buy(
                token,
                [0; 20],
                curve,
                input,
                &strategy_c2(Some(HEAD)),
                &clock(500, 3),
                500,
            )
            .unwrap();
        let tokens = decode_swap_exact_input_native(decode_signed(&bought.raw).unwrap().tx.input)
            .unwrap()
            .min_output_amount;
        held.push((token, tokens));
    }
    same(&exec, U256::from(3u8) * (approval + sell), "three bought");
    // An exit: its approval and SELL went, the position is closed.
    let (exits, refused) = exec.on_strategy_exit(
        held[0].0,
        held[0].1,
        curve,
        &strategy_c2(Some(HEAD)),
        &clock(900, 3),
        900,
    );
    assert_eq!((exits.len(), refused), (2, None));
    same(&exec, U256::from(2u8) * (approval + sell), "one exited");
    // Its SELL replaced, the approval stands: open again, approved.
    exec.reopen_exit(held[0].0, false);
    same(
        &exec,
        U256::from(2u8) * (approval + sell) + sell,
        "SELL reopened",
    );
    // The approval replaced too: open again, unapproved.
    exec.reopen_exit(held[0].0, true);
    same(
        &exec,
        U256::from(3u8) * (approval + sell),
        "approval reopened",
    );
    // An approval the chain holds for a held token, one for a token
    // not held (counted once it is adopted).
    exec.adopt_approval(held[1].0, held[1].1);
    same(
        &exec,
        U256::from(2u8) * (approval + sell) + sell,
        "approval adopted",
    );
    exec.adopt_approval([9; 20], U256::from(5u8));
    same(
        &exec,
        U256::from(2u8) * (approval + sell) + sell,
        "foreign approval",
    );
    exec.adopt([9; 20], U256::from(5u8));
    same(
        &exec,
        U256::from(2u8) * (approval + sell + sell),
        "position adopted",
    );
    // Adopted again: still one position.
    exec.adopt([9; 20], U256::from(6u8));
    same(
        &exec,
        U256::from(2u8) * (approval + sell + sell),
        "adopted again",
    );
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// BSC3-069: the BUY policy and the exit policy each from their own
/// configured slippage.
#[test]
fn the_exit_has_its_own_slippage() {
    let mut cfg = config(1, 1);
    cfg.slippage_bps = 50;
    cfg.exit_slippage_bps = 700;
    let (exec, d) = armed("exit-slippage", cfg);
    assert_eq!(
        (exec.policy.slippage_bps(), exec.exit_policy.slippage_bps()),
        (50, 700)
    );
    exec.disarm().unwrap();
    fs::remove_dir_all(d).unwrap();
}

/// Phase I gate M1: an exit of a token without a handed-off BUY is
/// refused (before: a SELL of tokens never held was signed).
#[test]
fn no_exit_without_a_held_buy() {
    let (mut exec, d) = armed("strategy-not-held", config(2, 2));
    let curve = curve_after(1_000_000_000_000_000_000);
    let (out, refused) = exec.on_strategy_exit(
        [5; 20],
        U256::from(1_000u64),
        curve,
        &strategy_c2(Some(HEAD)),
        &clock(1, 1),
        1,
    );
    assert_eq!((out.len(), refused), (0, Some(Refusal::NotHeld)));
    assert_eq!(exec.strategy_lanes().1, 2);
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// Phase I gate L-d: one sender, one sequence. A C3 BUY handed off at
/// nonce 0 makes the strategy's BUY take nonce 1 (before: nonce 0 a
/// second time), C3's alternatives keep nonce 0; with a strategy
/// intent holding nonce 0 first, a C3 BUY is refused NONCE_TAKEN.
#[test]
fn c3_and_the_strategy_share_one_nonce_sequence() {
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let curve = curve_after(1_000_000_000_000_000_000);
    let input = U256::from(10_000_000_000_000_000u64);
    let nonce = |raw: &[u8]| decode_signed(raw).unwrap().tx.nonce;
    let mut cfg = config(2, 2);
    cfg.strategy_lanes = 4;
    let c3 = |exec: &mut PaperExecutor, salt: u8| {
        let (hash, raw, call) = launch(salt, 7);
        exec.on_launch(
            PendingLaunch {
                hash,
                raw: &raw,
                to: Some(portal),
                input: &call,
                ingress_ns: 100,
            },
            &c2(Some(HEAD)),
            &clock(100, 7),
        )
    };
    let (mut exec, d) = armed("shared-c3-first", cfg.clone());
    assert_eq!(nonce(&c3(&mut exec, 0).unwrap().raw), 0);
    let buy = exec
        .on_strategy_buy(
            [0xAB; 20],
            [0; 20],
            curve,
            input,
            &strategy_c2(Some(HEAD)),
            &clock(500, 3),
            500,
        )
        .unwrap();
    assert_eq!(nonce(&buy.raw), 1);
    let (_, left, next_nonce, _, gap) = exec.strategy_lanes();
    assert_eq!((left, next_nonce, gap), (2, 2, false));
    assert_eq!(nonce(&c3(&mut exec, 1).unwrap().raw), 0);
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();

    let (mut exec, d) = armed("shared-strategy-first", cfg);
    let buy = exec
        .on_strategy_buy(
            [0xAB; 20],
            [0; 20],
            curve,
            input,
            &strategy_c2(Some(HEAD)),
            &clock(500, 3),
            500,
        )
        .unwrap();
    assert_eq!(nonce(&buy.raw), 0);
    assert_eq!(c3(&mut exec, 0), Err(Refusal::NonceTaken));
    assert_eq!(exec.counters().refused("NONCE_TAKEN"), 1);
    assert_eq!(exec.lanes(), (2, 2));
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// Phase I gate M2: a PAPER path left out of `sender_paths` is refused
/// PATH_OFF and counted; it takes no lane and no nonce.
#[test]
fn a_path_left_out_does_not_take_the_sender() {
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let curve = curve_after(1_000_000_000_000_000_000);
    let input = U256::from(10_000_000_000_000_000u64);
    let mut cfg = config(2, 2);
    cfg.sender_paths = SenderPaths::C3;
    let (mut exec, d) = armed("paths-c3", cfg.clone());
    assert_eq!(
        exec.on_strategy_buy(
            [1; 20],
            [0; 20],
            curve,
            input,
            &strategy_c2(Some(HEAD)),
            &clock(1, 1),
            1
        ),
        Err(Refusal::PathOff)
    );
    assert_eq!(exec.strategy_counters().refused("PATH_OFF"), 1);
    assert_eq!(exec.strategy_lanes().2, 0);
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
    cfg.sender_paths = SenderPaths::StrategyC;
    let (mut exec, d) = armed("paths-strategy", cfg);
    let (hash, raw, call) = launch(0, 7);
    let launch = PendingLaunch {
        hash,
        raw: &raw,
        to: Some(portal),
        input: &call,
        ingress_ns: 100,
    };
    assert_eq!(
        exec.on_launch(launch, &c2(Some(HEAD)), &clock(100, 7)),
        Err(Refusal::PathOff)
    );
    assert_eq!(exec.counters().refused("PATH_OFF"), 1);
    assert_eq!(exec.lanes(), (2, 2));
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
    assert_eq!(SenderPaths::parse("C3+STRATEGY_C"), Ok(SenderPaths::Both));
    assert!(SenderPaths::parse("C3,STRATEGY_C").is_err());
}

/// Phase I gate L2: an exit whose approval reached the sink and whose
/// SELL missed its start deadline before the permit (the 15th clock
/// reading) keeps the position: a later exit signs only the SELL, at
/// the freed nonce, up to the approved amount, and closes it (before:
/// exited for good, its input never released). Gas counts only for
/// what may be out.
#[test]
fn a_refused_sell_after_its_approval_can_still_close_the_position() {
    use darkbot_bsc_trading::venues::flap::recognize::decode_swap_exact_input;
    let mut cfg = config(2, 2);
    cfg.strategy_lanes = 6;
    let gas = |limit: u64| U256::from(cfg.gas_price_wei) * U256::from(limit);
    let (approve_gas, sell_gas) = (gas(cfg.approve_gas_limit), gas(cfg.sell_gas_limit));
    let (mut exec, d) = armed("strategy-sell-retry", cfg.clone());
    let curve = curve_after(1_000_000_000_000_000_000);
    let input = U256::from(10_000_000_000_000_000u64);
    let c2 = strategy_c2(Some(HEAD));
    let nonce = |raw: &[u8]| decode_signed(raw).unwrap().tx.nonce;
    let buy = exec
        .on_strategy_buy([1; 20], [0; 20], curve, input, &c2, &clock(1, 1), 1)
        .unwrap();
    let after_buy = exec.strategy_lanes().3;
    let tokens = decode_swap_exact_input_native(decode_signed(&buy.raw).unwrap().tx.input)
        .unwrap()
        .min_output_amount;
    let exit_curve = curve_after(1_010_000_000_000_000_000);
    let (out, refused) = exec.on_strategy_exit(
        [1; 20],
        tokens,
        exit_curve,
        &c2,
        &clock_jump(1, 1, 15, 2_000_000),
        1,
    );
    assert_eq!(refused, Some(Refusal::Deadline));
    let [approve]: [PaperIntent; 1] = out.try_into().unwrap();
    assert_eq!(
        (approve.kind, nonce(&approve.raw)),
        (IntentKind::Approve, 1)
    );
    let (_, _, next_nonce, spent, gap) = exec.strategy_lanes();
    assert_eq!((next_nonce, gap), (2, false));
    assert_eq!(spent, after_buy + approve_gas);
    let (out, refused) = exec.on_strategy_exit([1; 20], tokens, exit_curve, &c2, &clock(1, 1), 1);
    assert_eq!(refused, None);
    let [sell]: [PaperIntent; 1] = out.try_into().unwrap();
    assert_eq!((sell.kind, nonce(&sell.raw)), (IntentKind::Sell, 2));
    let swap = decode_swap_exact_input(decode_signed(&sell.raw).unwrap().tx.input).unwrap();
    assert!(swap.is_sell() && swap.input_amount == tokens);
    // Closed: the input left the exposure, the gas of what went out stays.
    let (_, _, next_nonce, spent, _) = exec.strategy_lanes();
    assert_eq!(next_nonce, 3);
    assert_eq!(spent, after_buy - input + approve_gas + sell_gas);
    assert_eq!(
        exec.on_strategy_exit([1; 20], tokens, exit_curve, &c2, &clock(1, 1), 1)
            .1,
        Some(Refusal::DuplicateToken)
    );
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// `step` apart, `jump` more at the reading number `at` (from 1).
fn clock_jump(start: u64, step: u64, at: u64, jump: u64) -> FnClock<impl Fn() -> u64> {
    let (now, reads) = (Cell::new(start), Cell::new(0u64));
    FnClock {
        id: CLOCK_ID,
        now: move || {
            reads.set(reads.get() + 1);
            let extra = if reads.get() == at { jump } else { 0 };
            now.set(now.get() + step + extra);
            now.get()
        },
    }
}

/// Phase I gate M3/N1: a strategy intent refused before anything of it
/// was disclosed (the start deadline missed at the pre-permit check, the
/// 7th clock reading) frees its nonces: the next intent takes them and
/// the held position still exits (before: a hole, every later intent
/// NONCE_GAP). Refused after a permit's write started (the write check,
/// the 8th reading), its bytes may be out: a hole, no later strategy
/// intent is signed (no later PAPER transaction could pass it).
#[test]
fn a_nonce_is_taken_again_only_when_nothing_was_disclosed() {
    let mut cfg = config(2, 2);
    cfg.strategy_lanes = 6;
    let (mut exec, d) = armed("strategy-gap", cfg);
    let curve = curve_after(1_000_000_000_000_000_000);
    let input = U256::from(10_000_000_000_000_000u64);
    let c2 = strategy_c2(Some(HEAD));
    let nonce = |raw: &[u8]| decode_signed(raw).unwrap().tx.nonce;
    let a = exec
        .on_strategy_buy([1; 20], [0; 20], curve, input, &c2, &clock(1, 1), 1)
        .unwrap();
    assert_eq!(nonce(&a.raw), 0);
    let late = 2_000_000;
    assert_eq!(
        exec.on_strategy_buy(
            [2; 20],
            [0; 20],
            curve,
            input,
            &c2,
            &clock_jump(1, 1, 7, late),
            1
        ),
        Err(Refusal::Deadline)
    );
    let (_, _, next_nonce, _, gap) = exec.strategy_lanes();
    assert_eq!((next_nonce, gap), (1, false));
    // The exit of A, first late before its approval's permit: nothing
    // out, its nonces and the token free; then on time.
    let tokens = decode_swap_exact_input_native(decode_signed(&a.raw).unwrap().tx.input)
        .unwrap()
        .min_output_amount;
    let exit_curve = curve_after(1_010_000_000_000_000_000);
    assert_eq!(
        exec.on_strategy_exit(
            [1; 20],
            tokens,
            exit_curve,
            &c2,
            &clock_jump(1, 1, 7, late),
            1
        ),
        (Vec::new(), Some(Refusal::Deadline))
    );
    let (exits, refused) = exec.on_strategy_exit([1; 20], tokens, exit_curve, &c2, &clock(1, 1), 1);
    assert_eq!(refused, None);
    let nonces: Vec<u64> = exits.iter().map(|i| nonce(&i.raw)).collect();
    assert_eq!(nonces, [1, 2]);
    // C: late at the write check, after its permit: a hole.
    assert_eq!(
        exec.on_strategy_buy(
            [3; 20],
            [0; 20],
            curve,
            input,
            &c2,
            &clock_jump(1, 1, 8, late),
            1
        ),
        Err(Refusal::Dispatch)
    );
    assert_eq!(
        exec.on_strategy_buy([4; 20], [0; 20], curve, input, &c2, &clock(1, 1), 1),
        Err(Refusal::NonceGap)
    );
    let (_, left, next_nonce, _, gap) = exec.strategy_lanes();
    assert_eq!((left, next_nonce, gap), (2, 4, true));
    assert_eq!(exec.strategy_counters().handoffs, 3);
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// A second end for the one executor (plan 28.10., package 3): it
/// records what reaches it and answers as told, as the LIVE end may
/// (handed off, refused before the write, or possibly sent).
struct RecordingEnd {
    signer: Signer,
    seen: Vec<(bool, u64, [u8; 32], Vec<u8>)>,
    /// None: handed off. Some(disclosed): refused, the bytes possibly
    /// out when `disclosed`.
    fail: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Recorded(u64);

impl HandoffTime for Recorded {
    fn at_ns(&self) -> u64 {
        self.0
    }
}

impl End for RecordingEnd {
    type Handoff = Recorded;
    const FENCE: (&'static str, [u8; 8]) = ("recording", *b"DBTSTFN1");
    const CHAIN_ORDERED: bool = false;

    fn address(&self) -> [u8; 20] {
        self.signer.address().into_array()
    }

    fn signer(&self) -> &Signer {
        &self.signer
    }

    fn hand_over(&mut self, h: Handover<'_>) -> Handed<Recorded> {
        let bundle = match h.submission {
            Submission::LaunchBundle(launch) => {
                assert!(!launch.is_empty());
                true
            }
            Submission::Single => false,
        };
        self.seen
            .push((bundle, h.ticket.nonce(), h.tx_hash, h.raw.to_vec()));
        let at = h.clock.now_ns();
        match self.fail {
            None => Handed {
                result: Ok(Recorded(at)),
                planned_ns: at,
                admitted: true,
                disclosed: true,
            },
            Some(disclosed) => Handed {
                result: Err(Refusal::Dispatch),
                planned_ns: at,
                admitted: disclosed,
                disclosed,
            },
        }
    }
}

fn recording(name: &str, fail: Option<bool>) -> (Executor<RecordingEnd>, PathBuf) {
    let d = dir(name);
    let end = RecordingEnd {
        signer: Signer::from_secret_bytes(&[0x11; 32]).unwrap(),
        seen: Vec::new(),
        fail,
    };
    let exec = Executor::arm_end(
        end,
        validated(),
        WITNESSED_MAINNET_TERMS,
        config(2, 2),
        Arming {
            next_nonce: 4,
            fence_dir: &d.join("fences"),
            session: 1,
            arm: 0,
        },
    )
    .unwrap();
    (exec, d)
}

/// Plan 28.10.: the one path (decoder, C2, admission, signature, lane,
/// final binding check) reaches whichever end it is armed with; the
/// end gets the prepared transaction, signed by its own signer, and
/// what it takes is the BUY's handoff. Its fence is its own.
#[test]
fn the_one_path_reaches_any_end() {
    let (mut paper, pd) = armed("end-paper", config(2, 2));
    let (mut other, od) = recording("end-other", None);
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let (hash, raw, input) = launch(0, 7);
    let pending = PendingLaunch {
        hash,
        raw: &raw,
        to: Some(portal),
        input: &input,
        ingress_ns: 100,
    };
    let a = paper
        .on_launch(pending, &c2(Some(HEAD)), &clock(100, 7))
        .unwrap();
    let b = other
        .on_launch(pending, &c2(Some(HEAD)), &clock(100, 7))
        .unwrap();
    let (x, y) = (
        decode_signed(&a.raw).unwrap(),
        decode_signed(&b.raw).unwrap(),
    );
    assert_eq!((x.tx.nonce, y.tx.nonce), (0, 4));
    assert_eq!((x.tx.to, x.tx.value), (y.tx.to, y.tx.value));
    assert_eq!(
        decode_swap_exact_input_native(x.tx.input)
            .unwrap()
            .output_token,
        decode_swap_exact_input_native(y.tx.input)
            .unwrap()
            .output_token
    );
    let from = darkbot_bsc_trading::signer::recover_legacy_sender(&y, 56, &mut Vec::new());
    assert_eq!(from.unwrap().into_array(), other.sender());
    assert_eq!(a.token, b.token);
    let seen = &other.end().seen;
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0], (true, 4, keccak256(&b.raw).0, b.raw.clone()));
    assert_eq!(b.times.sink_ns, b.handoff.0);
    assert_eq!(other.spent_wei(), other.budget.max_total_wei);
    // A Strategy C BUY is the transaction alone.
    let token = [0xAB; 20];
    let input = U256::from(10_000_000_000_000_000u64);
    let curve = curve_after(1_000_000_000_000_000_000);
    let intent = other
        .on_strategy_buy(
            token,
            [0; 20],
            curve,
            input,
            &strategy_c2(Some(HEAD)),
            &clock(500, 3),
            500,
        )
        .unwrap();
    let seen = &other.end().seen;
    assert_eq!((seen[1].0, seen[1].1), (false, 5));
    assert_eq!(seen[1].3, intent.raw);
    let fence = other.fence().to_path_buf();
    assert_eq!(fence.file_name().unwrap(), "recording-1-0.fence");
    assert_eq!(&fs::read(&fence).unwrap()[..8], b"DBTSTFN1");
    paper.disarm().unwrap();
    other.disarm().unwrap();
    fs::remove_dir_all(pd).unwrap();
    fs::remove_dir_all(od).unwrap();
}

/// What the end says it consumed decides, not the end's kind: a
/// refusal before the write gives the nonce back and spends nothing; a
/// transaction possibly out (LIVE: UNKNOWN) spends its exposure and
/// stops the sender's sequence, with no retry.
#[test]
fn what_an_end_consumed_decides_nonce_and_exposure() {
    let token = [0xAB; 20];
    let input = U256::from(10_000_000_000_000_000u64);
    let curve = curve_after(1_000_000_000_000_000_000);
    let buy = |e: &mut Executor<RecordingEnd>, t: [u8; 20]| {
        e.on_strategy_buy(
            t,
            [0; 20],
            curve,
            input,
            &strategy_c2(Some(HEAD)),
            &clock(500, 3),
            500,
        )
    };
    let (mut clean, d1) = recording("end-clean", Some(false));
    assert_eq!(buy(&mut clean, token), Err(Refusal::Dispatch));
    assert_eq!(buy(&mut clean, [0xAC; 20]), Err(Refusal::Dispatch));
    let seen = &clean.end().seen;
    // The same nonce again, nothing spent, no hole.
    assert_eq!((seen[0].1, seen[1].1), (4, 4));
    let (_, _, next, spent, gap) = clean.strategy_lanes();
    assert_eq!((next, spent, gap), (4, U256::ZERO, false));
    let (mut unknown, d2) = recording("end-unknown", Some(true));
    assert_eq!(buy(&mut unknown, token), Err(Refusal::Dispatch));
    let (_, _, _, spent, gap) = unknown.strategy_lanes();
    assert!(gap && spent > U256::ZERO);
    assert_eq!(buy(&mut unknown, [0xAC; 20]), Err(Refusal::NonceGap));
    assert_eq!(unknown.end().seen.len(), 1);
    clean.disarm().unwrap();
    unknown.disarm().unwrap();
    fs::remove_dir_all(d1).unwrap();
    fs::remove_dir_all(d2).unwrap();
}

use super::*;
use alloy_primitives::{hex, keccak256};
use darkbot_bsc_trading::economics::flap::native_curve20::{PricedToken, WITNESSED_MAINNET_TERMS};
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
        four: None,
        openfour: None,
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
        c3_candidates: crate::paper_exec::C3Candidates::SelfLaunch,
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
                from: [0; 20],
                to: Some(portal),
                value: &[],
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

/// R13 (FOREIGN_AUTO): a foreign launch the campaign saw (another identity,
/// 3 % taxes, a creator purchase in the launch) is no SELF launch (PROFILE);
/// with `BSC_EXEC_C3_CANDIDATES=FOREIGN_AUTO` it reaches the sink as the
/// BUY of its token (the CREATE2 of its salt: the receipt's), priced on the
/// curve its creator's purchase left.
#[test]
fn a_foreign_launch_is_a_c3_candidate_only_under_foreign_auto() {
    use darkbot_bsc_trading::economics::flap::native_curve20::{
        HoldingCap, NativeBuyInputs, PricedToken, WITNESSED_MAINNET_TERMS, profile_terms,
        quote_native_buy,
    };
    use darkbot_bsc_trading::economics::slippage::min_output;
    const FIXTURE: &str =
        include_str!("../../trading/fixtures/flap/foreign_launch_parity_20261106.txt");
    let rows = |kind: &'static str| {
        FIXTURE
            .lines()
            .map(|l| l.split(' ').collect::<Vec<_>>())
            .filter(move |f| f[0] == kind)
    };
    let u = |s: &str| U256::from_str_radix(s, 10).unwrap();
    // A sampled launch the receipts priced, with a creator purchase.
    let (c, f) = rows("CALLDATA")
        .filter(|c| c[2] == "OK" && c[3] != "0")
        .find_map(|c| {
            let f = rows("LAUNCH").find(|f| f[2] == c[1])?;
            (f[5] != "100").then_some((c, f))
        })
        .unwrap();
    let input = hex::decode(c[4]).unwrap();
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let (hash, raw, _) = launch(0, 7);
    let value_bytes = u(c[3]).to_be_bytes_trimmed_vec();
    let pending = PendingLaunch {
        hash,
        raw: &raw,
        from: [0; 20],
        to: Some(portal),
        value: &value_bytes,
        input: &input,
        ingress_ns: 100,
    };
    let (mut own, d) = armed("foreign-self", config(2, 2));
    assert!(matches!(
        own.on_launch(pending, &c2(Some(HEAD)), &clock(100, 7)),
        Err(Refusal::Profile(_))
    ));
    own.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
    let mut cfg = config(2, 2);
    cfg.c3_candidates = C3Candidates::ForeignAuto;
    let (mut exec, d) = armed("foreign-auto", cfg.clone());
    // Review M2: a value other than the creator purchase is no profile.
    assert!(matches!(
        exec.on_launch(
            PendingLaunch {
                value: &[0x01],
                ..pending
            },
            &c2(Some(HEAD)),
            &clock(100, 7)
        ),
        Err(Refusal::Profile(UnsupportedLaunch::CreatorValue))
    ));
    let buy = exec
        .on_launch(pending, &c2(Some(HEAD)), &clock(100, 7))
        .unwrap();
    let signed = decode_signed(&buy.raw).unwrap();
    let call = decode_swap_exact_input_native(signed.tx.input).unwrap();
    let token: [u8; 20] = hex::decode(&f[4][2..]).unwrap().try_into().unwrap();
    assert_eq!((buy.token, call.output_token.into_array()), (token, token));
    let quote = |reserve: U256, circulating: U256, input: U256| {
        quote_native_buy(&NativeBuyInputs {
            token: PricedToken::new(&profile_terms(f[5].parse().unwrap(), f[6].parse().unwrap()))
                .unwrap(),
            reserve_before: reserve,
            circulating_before: circulating,
            input_amount: input,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: HoldingCap::None,
        })
        .unwrap()
    };
    let creator = quote(U256::ZERO, U256::ZERO, u(f[10]));
    assert_eq!(creator.output_amount, u(f[11]));
    let ours = quote(
        creator.reserve_after,
        creator.supply_after,
        U256::from(cfg.input_wei),
    );
    assert_eq!(
        call.min_output_amount,
        min_output(ours.output_amount, cfg.slippage_bps).unwrap()
    );
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// R12c: a Four.meme Classic launch on the C3 path. Without the Four arm
/// it is NOT_DECODED; armed, a call value other than the launch's creator
/// value is no witnessed profile (PROFILE), a value that is no canonical
/// amount NOT_DECODED; the priced launch reaches the sink as its admitted
/// BUY (TokenManager2, the input as value, buyTokenAMAP of the launch's
/// token) in the 48Club bundle with the launch; the same launch again is a
/// duplicate.
#[test]
fn a_four_classic_launch_takes_the_four_arm_or_is_refused_at_its_stage() {
    use crate::runtime::exec::tests::{four_config, four_launch, four_observation};
    use darkbot_bsc_geth_ipc::ingress::decode_tx_target;
    use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_r12d_131608421;
    use darkbot_bsc_trading::venues::fourmeme::profile::UnsupportedClassicLaunch;
    use darkbot_bsc_trading::venues::fourmeme::recognize::decode_classic_buy_amap;
    let mut cfg = config(4, 4);
    cfg.four = Some(four_config());
    let (mut exec, d) = armed("four", cfg);
    let (hash, raw, _) = four_launch(9);
    let target = decode_tx_target(&raw).unwrap();
    let on = |e: &mut PaperExecutor, value: &[u8]| {
        e.on_launch(
            PendingLaunch {
                hash,
                raw: &raw,
                from: [0; 20],
                to: target.to,
                value,
                input: target.data,
                ingress_ns: 100,
            },
            &c2(Some(HEAD)),
            &clock(100, 1),
        )
    };
    assert_eq!(on(&mut exec, target.value).err(), Some(Refusal::NotDecoded));
    let four = bsc_mainnet_r12d_131608421();
    exec.arm_four(
        four.validate(&four_observation(&four).observed, CODEHASH_GENERATION)
            .unwrap(),
    )
    .unwrap();
    assert!(exec.four_armed());
    let other = (U256::from_be_slice(target.value) + U256::from(1)).to_be_bytes_trimmed_vec();
    assert_eq!(
        on(&mut exec, &other).err(),
        Some(Refusal::FourProfile(UnsupportedClassicLaunch::CreatorValue))
    );
    let padded = [&[0u8][..], target.value].concat();
    assert_eq!(on(&mut exec, &padded).err(), Some(Refusal::NotDecoded));
    let buy = on(&mut exec, target.value).unwrap();
    let signed = decode_signed(&buy.raw).unwrap();
    assert_eq!(signed.tx.to, four.token_manager2);
    assert_eq!(signed.tx.value, U256::from(1_000_000_000_000_000u64));
    assert_eq!(signed.tx.gas_limit, four_config().gas_limit);
    let call = decode_classic_buy_amap(signed.tx.input).unwrap();
    assert_eq!(call.token.into_array(), buy.token);
    assert_eq!(call.funds, signed.tx.value);
    assert!(call.min_amount > U256::ZERO);
    // The request carries the launch and the BUY.
    assert!(buy.handoff.raw_len > 2 * (raw.len() + buy.raw.len()));
    assert_eq!(
        on(&mut exec, target.value).err(),
        Some(Refusal::DuplicateToken)
    );
    let c = exec.counters();
    assert_eq!(
        (
            c.handoffs,
            c.refused("NOT_DECODED"),
            c.refused("PROFILE"),
            c.refused("DUPLICATE_TOKEN")
        ),
        (1, 2, 1, 1)
    );
    // The Four BUY's exposure: its input and its gas.
    assert_eq!(
        exec.spent_wei(),
        U256::from(1_000_000_000_000_000u64 + 1_000_000_000 * four_config().gas_limit)
    );
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// R12d: an OpenFour launch on the C3 path. Without the OpenFour arm it is
/// NOT_DECODED (it is no Flap launch); armed, a call value other than its
/// creator word is no witnessed profile (PROFILE); the priced launch
/// reaches the sink as its admitted exact BUY (Core, the input as value and
/// `maxPay`) of the token derived from the verified sender, another sender
/// another token; the same launch again is a duplicate.
#[test]
fn an_openfour_launch_takes_the_openfour_arm_or_is_refused_at_its_stage() {
    use crate::runtime::exec::tests::{four_observation, openfour_config, openfour_launch};
    use darkbot_bsc_geth_ipc::ingress::decode_tx_target;
    use darkbot_bsc_trading::venues::fourmeme::deployment::{
        bsc_mainnet_r12b_131568366, bsc_mainnet_r12d_131608421,
    };
    use darkbot_bsc_trading::venues::fourmeme::profile::UnsupportedOpenFourLaunch;
    use darkbot_bsc_trading::venues::fourmeme::recognize::{
        decode_openfour_buy, decode_openfour_create,
    };
    // Room for two OpenFour BUYs (input and 2M gas each).
    let mut cfg = config(4, 6);
    cfg.openfour = Some(openfour_config());
    let (mut exec, d) = armed("openfour", cfg);
    let (hash, raw, launcher) = openfour_launch(9);
    let target = decode_tx_target(&raw).unwrap();
    let on = |e: &mut PaperExecutor, from: [u8; 20], value: &[u8]| {
        e.on_launch(
            PendingLaunch {
                hash,
                raw: &raw,
                from,
                to: target.to,
                value,
                input: target.data,
                ingress_ns: 100,
            },
            &c2(Some(HEAD)),
            &clock(100, 1),
        )
    };
    assert_eq!(
        on(&mut exec, launcher, target.value).err(),
        Some(Refusal::NotDecoded)
    );
    // A pin without an executed bonding preset arms no OpenFour BUY.
    let earlier = bsc_mainnet_r12b_131568366();
    assert!(matches!(
        exec.arm_openfour(
            earlier
                .validate(&four_observation(&earlier).observed, CODEHASH_GENERATION)
                .unwrap()
        ),
        Err(ArmError::Config(_))
    ));
    assert!(!exec.openfour_armed());
    let four = bsc_mainnet_r12d_131608421();
    let validated = four
        .validate(&four_observation(&four).observed, CODEHASH_GENERATION)
        .unwrap();
    exec.arm_openfour(validated).unwrap();
    assert!(exec.openfour_armed() && !exec.four_armed());
    let other = (U256::from_be_slice(target.value) + U256::from(1)).to_be_bytes_trimmed_vec();
    assert_eq!(
        on(&mut exec, launcher, &other).err(),
        Some(Refusal::OpenFourProfile(
            UnsupportedOpenFourLaunch::CreatorValue
        ))
    );
    let launch = decode_openfour_create(target.data).unwrap();
    let token_of = |creator: [u8; 20]| {
        four.openfour_token(
            launch.preset_id,
            launch.init.token_salt,
            launch.request_id,
            Some(Address::from(creator)),
        )
        .unwrap()
    };
    let buy = on(&mut exec, launcher, target.value).unwrap();
    assert_eq!(buy.token, token_of(launcher).into_array());
    let signed = decode_signed(&buy.raw).unwrap();
    assert_eq!(signed.tx.to, four.openfour_core);
    assert_eq!(signed.tx.value, U256::from(1_000_000_000_000_000u64));
    assert_eq!(signed.tx.gas_limit, openfour_config().gas_limit);
    let call = decode_openfour_buy(signed.tx.input).unwrap();
    assert_eq!(call.token.into_array(), buy.token);
    assert_eq!(call.max_pay_amount, signed.tx.value);
    assert!(call.amount > U256::ZERO);
    // The request carries the launch and the BUY.
    assert!(buy.handoff.raw_len > 2 * (raw.len() + buy.raw.len()));
    assert_eq!(
        on(&mut exec, launcher, target.value).err(),
        Some(Refusal::DuplicateToken)
    );
    // The same calldata from another sender creates another token.
    let elsewhere = on(&mut exec, [0x11; 20], target.value).unwrap();
    assert_eq!(elsewhere.token, token_of([0x11; 20]).into_array());
    assert_ne!(elsewhere.token, buy.token);
    let c = exec.counters();
    assert_eq!(
        (
            c.handoffs,
            c.refused("NOT_DECODED"),
            c.refused("PROFILE"),
            c.refused("DUPLICATE_TOKEN")
        ),
        (2, 1, 1, 1)
    );
    // Each OpenFour BUY's exposure: its input and its gas.
    assert_eq!(
        exec.spent_wei(),
        U256::from(2 * (1_000_000_000_000_000u64 + 1_000_000_000 * openfour_config().gas_limit))
    );
    exec.disarm().unwrap();
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
                from: [0; 20],
                to: Some(portal),
                value: &[],
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
                from: [0; 20],
                to: Some([7; 20]),
                value: &[],
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
                from: [0; 20],
                to: Some(portal),
                value: &[],
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
                from: [0; 20],
                to: Some(portal),
                value: &[],
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
                from: [0; 20],
                to: Some(portal),
                value: &[],
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
                from: [0; 20],
                to: Some(portal),
                value: &[],
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
                from: [0; 20],
                to: Some(portal),
                value: &[],
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
                from: [0; 20],
                to: Some(portal),
                value: &[],
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

/// A Flap strategy curve's terms, reserve and circulating supply.
pub(crate) fn flap(c: StrategyCurve) -> (PricedToken, U256, U256) {
    match c {
        StrategyCurve::Flap {
            terms,
            reserve,
            circulating,
        } => (terms, reserve, circulating),
        StrategyCurve::Four(_) => panic!("a Flap curve"),
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
    StrategyCurve::Flap {
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
            None,
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
            600,
            None,
        ),
        Err(Refusal::DuplicateToken)
    );
    // The exit of the tokens bought, on the curve with them in it.
    let tokens = call.min_output_amount;
    let after = StrategyCurve::Flap {
        reserve: flap(curve).1 + input,
        circulating: flap(curve).2 + tokens,
        terms: flap(curve).0,
    };
    let curve_with_buy = {
        use darkbot_bsc_trading::economics::flap::native_curve20::{
            HoldingCap, NativeBuyInputs, quote_native_buy,
        };
        let q = quote_native_buy(&NativeBuyInputs {
            token: flap(curve).0,
            reserve_before: flap(curve).1,
            circulating_before: flap(curve).2,
            input_amount: input,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: HoldingCap::None,
        })
        .unwrap();
        StrategyCurve::Flap {
            reserve: q.reserve_after,
            circulating: q.supply_after,
            terms: flap(after).0,
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

/// SC4: the OpenFour token's curve at a witnessed later state (its OFSNAP
/// row) as Strategy C tracks it, its token and the executed 0.01 BNB
/// admission's amount there at 100 bps (its OFADMIT row).
pub(crate) fn openfour_strategy_state(
    block: &str,
) -> (
    darkbot_bsc_trading::strategy::book::FourCurve,
    [u8; 20],
    U256,
) {
    use darkbot_bsc_trading::economics::four_classic::LaunchTerms;
    use darkbot_bsc_trading::economics::openfour::{OpenFourFee, launch_curve};
    use darkbot_bsc_trading::strategy::book::{FourCurve, FourPost};
    const FIXTURE: &str =
        include_str!("../../trading/fixtures/fourmeme/openfour_parity_20261104.txt");
    let rows = |kind: &'static str| {
        FIXTURE
            .lines()
            .map(|l| l.split(' ').collect::<Vec<_>>())
            .filter(move |f| f[0] == kind)
    };
    let u = |s: &str| U256::from_str_radix(s, 10).unwrap();
    let l = rows("OFLAUNCH").next().unwrap();
    let start = launch_curve(&LaunchTerms {
        total_supply: u(l[8]),
        max_offers: u(l[9]),
        max_raising: u(l[10]),
    })
    .unwrap();
    let fee = OpenFourFee {
        protocol_bps: 50,
        dev_bps: 50,
    };
    let snap = rows("OFSNAP").find(|f| f[1] == block).unwrap();
    let curve = FourCurve::openfour(start, fee)
        .after(FourPost {
            remaining: u(snap[7]),
            raised: u(snap[8]),
        })
        .unwrap();
    let token: [u8; 20] = hex::decode(&snap[2][2..]).unwrap().try_into().unwrap();
    let admit = rows("OFADMIT")
        .find(|f| f[1] == block && f[3] == "10000000000000000" && f[4] == "100")
        .unwrap();
    assert_eq!(admit[8], "OK");
    (curve, token, u(admit[6]))
}

/// SC4: a Strategy C PASS on an OpenFour token: without the OpenFour arm
/// VENUE_UNARMED; armed, the exact-amount BUY its tracked curve admits
/// (the amount the executed admission bought at that state, 0.01 BNB as
/// value and `maxPay`), then its exit: approve(Core) and `Core.sell` of
/// those tokens at the next nonces, the minimum the slippage floor of the
/// exact net proceeds on the shadow curve.
#[test]
fn a_strategy_entry_and_exit_on_an_openfour_curve_reach_the_sink() {
    use crate::runtime::exec::tests::{four_observation, openfour_config};
    use darkbot_bsc_trading::economics::slippage::min_output;
    use darkbot_bsc_trading::venues::flap::abi::decode_approve;
    use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_r12d_131608421;
    use darkbot_bsc_trading::venues::fourmeme::recognize::{
        decode_openfour_buy, decode_openfour_sell,
    };
    // Room for one OpenFour BUY (input and 2M gas) in the session cap.
    let mut cfg = config(2, 4);
    cfg.strategy_lanes = 4;
    cfg.openfour = Some(openfour_config());
    let (mut exec, d) = armed("strategy-openfour", cfg.clone());
    let (curve, token, amount) = openfour_strategy_state("131615000");
    let input = U256::from(10_000_000_000_000_000u64);
    let buy = |e: &mut PaperExecutor| {
        e.on_strategy_buy(
            token,
            [0; 20],
            StrategyCurve::Four(curve),
            input,
            &strategy_c2(Some(HEAD)),
            &clock(500, 3),
            500,
            None,
        )
    };
    assert_eq!(buy(&mut exec).err(), Some(Refusal::VenueUnarmed));
    let four = bsc_mainnet_r12d_131608421();
    exec.arm_openfour(
        four.validate(&four_observation(&four).observed, CODEHASH_GENERATION)
            .unwrap(),
    )
    .unwrap();
    let bought = buy(&mut exec).unwrap();
    let signed = decode_signed(&bought.raw).unwrap();
    assert_eq!(
        (
            signed.tx.to,
            signed.tx.value,
            signed.tx.gas_limit,
            signed.tx.nonce
        ),
        (four.openfour_core, input, openfour_config().gas_limit, 0)
    );
    let call = decode_openfour_buy(signed.tx.input).unwrap();
    assert_eq!(
        (call.token.into_array(), call.amount, call.max_pay_amount),
        (token, amount, input)
    );
    let shadow = curve.shifted(amount).unwrap();
    // Review L5: the position's exit on a Classic curve has no Classic arm
    // here: VENUE_UNARMED before any lane.
    let classic = darkbot_bsc_trading::strategy::book::FourCurve::classic_at(
        darkbot_bsc_trading::economics::four_classic::ClassicCurve {
            k: U256::from(1u64),
            t: U256::from(2u64),
            offers: U256::from(3u64),
            sold: U256::from(4u64),
        },
        darkbot_bsc_trading::economics::four_classic::ClassicFee {
            rate_bps: 100,
            min_fee: U256::ZERO,
            tax_bps: 1_000,
        },
    )
    .unwrap();
    assert_eq!(
        exec.on_strategy_exit(
            token,
            amount,
            StrategyCurve::Four(classic),
            &strategy_c2(Some(HEAD)),
            &clock(800, 3),
            800,
        ),
        (Vec::new(), Some(Refusal::VenueUnarmed))
    );
    // Review L5: a re-priced exit is signed at its price and the venue's
    // limits.
    let repriced = 2 * cfg.gas_price_wei;
    let c2 = CanonicalContext {
        exit_gas_price_wei: Some(repriced),
        ..strategy_c2(Some(HEAD))
    };
    let (exits, refused) = exec.on_strategy_exit(
        token,
        amount,
        StrategyCurve::Four(shadow),
        &c2,
        &clock(900, 3),
        900,
    );
    assert_eq!(refused, None);
    let [approve, sell]: [PaperIntent; 2] = exits.try_into().unwrap();
    let a = decode_signed(&approve.raw).unwrap();
    assert_eq!(
        (approve.kind, a.tx.nonce, a.tx.to.into_array(), a.tx.value),
        (IntentKind::Approve, 1, token, U256::ZERO)
    );
    // B1: signed at the OpenFour exits' own gas limits (re-priced).
    let s = decode_signed(&sell.raw).unwrap();
    assert_eq!(
        (a.tx.gas_limit, s.tx.gas_limit),
        (
            openfour_config().approve_gas_limit,
            openfour_config().sell_gas_limit
        )
    );
    assert_eq!((a.tx.gas_price, s.tx.gas_price), (repriced, repriced));
    assert_eq!(
        decode_approve(a.tx.input).unwrap(),
        (four.openfour_core, amount)
    );
    let s = decode_signed(&sell.raw).unwrap();
    assert_eq!(
        (sell.kind, s.tx.nonce, s.tx.to, s.tx.value),
        (IntentKind::Sell, 2, four.openfour_core, U256::ZERO)
    );
    let call = decode_openfour_sell(s.tx.input).unwrap();
    let received = shadow.sell(amount).unwrap();
    assert_eq!(
        (
            call.token.into_array(),
            call.amount,
            call.min_receive_amount
        ),
        (
            token,
            amount,
            min_output(received, cfg.exit_slippage_bps).unwrap()
        )
    );
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// SC4: a Strategy C PASS on a Classic token at a witnessed state third-party
/// trades moved (its PROBE row of 0.01 BNB, the deepest executed one):
/// `buyTokenAMAP` of the input, its minimum the slippage floor of the tokens
/// the probe received there; one gwei of tokens deeper is not admitted (B2);
/// its exit sells the minimum in whole gwei units (as every witnessed
/// Classic trade), at the Classic exits' gas limits (B1).
#[test]
fn a_strategy_entry_and_exit_on_a_classic_curve_reach_the_sink() {
    use crate::runtime::exec::tests::{four_config, four_observation};
    use darkbot_bsc_trading::economics::four_classic::{ClassicCurve, ClassicFee, floor_gwei};
    use darkbot_bsc_trading::economics::slippage::min_output;
    use darkbot_bsc_trading::strategy::book::FourCurve;
    use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_r12d_131608421;
    use darkbot_bsc_trading::venues::fourmeme::profile::WITNESSED_MAX_OFFERS;
    use darkbot_bsc_trading::venues::fourmeme::recognize::{
        decode_classic_buy_amap, decode_classic_sell_min_funds,
    };
    const FIXTURE: &str =
        include_str!("../../trading/fixtures/fourmeme/classic_parity_20261104.txt");
    let u = |s: &str| U256::from_str_radix(s, 10).unwrap();
    // PROBE block token template feeSetting K T offers rate minFee funds tokens ...
    let f: Vec<&str> = FIXTURE
        .lines()
        .map(|l| l.split(' ').collect::<Vec<_>>())
        .find(|f| f[0] == "PROBE" && f[1] == "131139373" && f[10] == "10000000000000000")
        .unwrap();
    let four = bsc_mainnet_r12d_131608421();
    let validated = four
        .validate(&four_observation(&four).observed, CODEHASH_GENERATION)
        .unwrap();
    let offers = u(f[7]);
    let curve = FourCurve::classic_at(
        ClassicCurve {
            k: u(f[5]),
            t: u(f[6]),
            offers,
            sold: WITNESSED_MAX_OFFERS - offers,
        },
        ClassicFee {
            rate_bps: validated.venue_fee().rate_bps,
            min_fee: validated.venue_fee().min_fee,
            // The priced word's tax (10 %).
            tax_bps: 1_000,
        },
    )
    .unwrap();
    let token: [u8; 20] = hex::decode(&f[2][2..]).unwrap().try_into().unwrap();
    let input = U256::from(10_000_000_000_000_000u64);
    let mut cfg = config(2, 2);
    // Two positions: BUY, approval, SELL; BUY, SELL.
    cfg.strategy_lanes = 6;
    cfg.four = Some(four_config());
    let (mut exec, d) = armed("strategy-classic", cfg.clone());
    exec.arm_four(validated).unwrap();
    // Fee terms other than the validated deployment's are refused.
    let FourCurve::Classic { now, fee, .. } = curve else {
        panic!()
    };
    let other = FourCurve::classic_at(
        now,
        ClassicFee {
            rate_bps: fee.rate_bps + 1,
            ..fee
        },
    )
    .unwrap();
    assert_eq!(
        exec.on_strategy_buy(
            token,
            [0; 20],
            StrategyCurve::Four(other),
            input,
            &strategy_c2(Some(HEAD)),
            &clock(300, 3),
            300,
            None,
        )
        .err(),
        Some(Refusal::Admission(AdmissionCause::Terms))
    );
    // B2: a state one gwei of tokens deeper than the deepest executed one.
    let gwei = U256::from(1_000_000_000u64);
    let deeper = FourCurve::classic_at(
        ClassicCurve {
            k: now.k,
            t: now.t - gwei,
            offers: now.offers - gwei,
            sold: now.sold + gwei,
        },
        fee,
    )
    .unwrap();
    assert_eq!(
        exec.on_strategy_buy(
            token,
            [0; 20],
            StrategyCurve::Four(deeper),
            input,
            &strategy_c2(Some(HEAD)),
            &clock(350, 3),
            350,
            None,
        )
        .err(),
        Some(Refusal::Admission(AdmissionCause::Unwitnessed))
    );
    // B5: each admission refusal counted under its cause.
    let coded = exec.strategy_counters().coded();
    for code in ["ADMISSION_TERMS=1", "ADMISSION_UNWITNESSED=1"] {
        assert!(coded.iter().any(|c| c == code), "{coded:?}");
    }
    // The Classic arm is no OpenFour one.
    let (openfour, other, _) = openfour_strategy_state("131615000");
    assert_eq!(
        exec.on_strategy_buy(
            other,
            [0; 20],
            StrategyCurve::Four(openfour),
            input,
            &strategy_c2(Some(HEAD)),
            &clock(400, 3),
            400,
            None,
        )
        .err(),
        Some(Refusal::VenueUnarmed)
    );
    let bought = exec
        .on_strategy_buy(
            token,
            [0; 20],
            StrategyCurve::Four(curve),
            input,
            &strategy_c2(Some(HEAD)),
            &clock(500, 3),
            500,
            None,
        )
        .unwrap();
    let signed = decode_signed(&bought.raw).unwrap();
    assert_eq!(
        (signed.tx.to, signed.tx.value),
        (four.token_manager2, input)
    );
    let call = decode_classic_buy_amap(signed.tx.input).unwrap();
    let tokens = u(f[11]);
    assert_eq!(
        (call.token.into_array(), call.funds, call.min_amount),
        (
            token,
            input,
            min_output(tokens, four_config().slippage_bps).unwrap()
        )
    );
    let held = call.min_amount;
    let shadow = curve.shifted(tokens).unwrap();
    let (exits, refused) = exec.on_strategy_exit(
        token,
        tokens,
        StrategyCurve::Four(shadow),
        &strategy_c2(Some(HEAD)),
        &clock(900, 3),
        900,
    );
    assert_eq!(refused, None);
    let [approve, sell]: [PaperIntent; 2] = exits.try_into().unwrap();
    let a = decode_signed(&approve.raw).unwrap();
    assert_eq!((a.tx.to.into_array(), a.tx.nonce), (token, 1));
    let amount = floor_gwei(held);
    assert_eq!(
        darkbot_bsc_trading::venues::flap::abi::decode_approve(a.tx.input).unwrap(),
        (four.token_manager2, amount)
    );
    let s = decode_signed(&sell.raw).unwrap();
    assert_eq!((s.tx.to, s.tx.nonce), (four.token_manager2, 2));
    assert_eq!(
        (a.tx.gas_limit, s.tx.gas_limit),
        (
            four_config().approve_gas_limit,
            four_config().sell_gas_limit
        )
    );
    let call = decode_classic_sell_min_funds(s.tx.input).unwrap();
    assert_eq!(call.amount, amount);
    assert!(amount < held || held == amount);
    // minFunds: the slippage floor of the SELL's GROSS on the shadow
    // (TokenManager2 compares it before the fee and the tax).
    let FourCurve::Classic { now, fee, .. } = shadow else {
        panic!()
    };
    let gross = darkbot_bsc_trading::economics::four_classic::quote_sell(&now, &fee, amount)
        .unwrap()
        .gross;
    assert_eq!(
        call.min_funds,
        min_output(gross, cfg.exit_slippage_bps).unwrap()
    );
    // Review L5: a second moved token whose approval is already out (of an
    // amount in no whole gwei): its exit is the SELL alone, of the approved
    // amount floored to whole gwei.
    let g: Vec<&str> = FIXTURE
        .lines()
        .map(|l| l.split(' ').collect::<Vec<_>>())
        .find(|f| f[0] == "PROBE" && f[1] == "131743981" && f[10] == "10000000000000000")
        .unwrap();
    let offers = u(g[7]);
    let second = FourCurve::classic_at(
        ClassicCurve {
            k: u(g[5]),
            t: u(g[6]),
            offers,
            sold: WITNESSED_MAX_OFFERS - offers,
        },
        fee,
    )
    .unwrap();
    let token: [u8; 20] = hex::decode(&g[2][2..]).unwrap().try_into().unwrap();
    let bought = exec
        .on_strategy_buy(
            token,
            [0; 20],
            StrategyCurve::Four(second),
            input,
            &strategy_c2(Some(HEAD)),
            &clock(1_000, 3),
            1_000,
            None,
        )
        .unwrap();
    let held = decode_classic_buy_amap(decode_signed(&bought.raw).unwrap().tx.input)
        .unwrap()
        .min_amount;
    let approved = held - U256::from(123u64);
    exec.adopt_approval(token, approved);
    let tokens = u(g[11]);
    let (exits, refused) = exec.on_strategy_exit(
        token,
        tokens,
        StrategyCurve::Four(second.shifted(tokens).unwrap()),
        &strategy_c2(Some(HEAD)),
        &clock(1_100, 3),
        1_100,
    );
    assert_eq!(refused, None);
    let [sell]: [PaperIntent; 1] = exits.try_into().unwrap();
    let s = decode_signed(&sell.raw).unwrap();
    assert_eq!(sell.kind, IntentKind::Sell);
    assert_eq!(
        decode_classic_sell_min_funds(s.tx.input).unwrap().amount,
        floor_gwei(approved)
    );
    assert_ne!(floor_gwei(approved), approved);
    exec.disarm().unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// Review L5: a Four BUY's cap check keeps room for its own venue's exit
/// (the OpenFour approval and SELL, not the Flap one's): at exactly the BUY
/// and that exit it goes, one wei less is SESSION_CAP.
#[test]
fn a_four_buys_cap_keeps_room_for_its_venues_exit() {
    use crate::runtime::exec::tests::{four_observation, openfour_config};
    use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_r12d_131608421;
    let of = openfour_config();
    let input = 10_000_000_000_000_000u128;
    let (curve, token, _) = openfour_strategy_state("131615000");
    for (slack, admitted) in [(0u128, true), (1, false)] {
        let mut cfg = config(2, 4);
        cfg.strategy_lanes = 4;
        cfg.openfour = Some(of);
        assert!(of.sell_gas_limit < cfg.sell_gas_limit);
        let gas = |g: u64| u128::from(cfg.gas_price_wei) * u128::from(g);
        let exact = input + gas(of.gas_limit) + gas(of.approve_gas_limit) + gas(of.sell_gas_limit);
        cfg.strategy_session_cap_wei = exact - slack;
        let (mut exec, d) = armed(&format!("strategy-four-cap-{slack}"), cfg.clone());
        let four = bsc_mainnet_r12d_131608421();
        exec.arm_openfour(
            four.validate(&four_observation(&four).observed, CODEHASH_GENERATION)
                .unwrap(),
        )
        .unwrap();
        let got = exec.on_strategy_buy(
            token,
            [0; 20],
            StrategyCurve::Four(curve),
            U256::from(input),
            &strategy_c2(Some(HEAD)),
            &clock(100, 3),
            100,
            None,
        );
        match admitted {
            true => assert!(got.is_ok(), "{:?}", got.err()),
            false => assert_eq!(got.err(), Some(Refusal::SessionCap)),
        }
        exec.disarm().unwrap();
        fs::remove_dir_all(&d).unwrap();
    }
}

/// SC4 c (B1): the exit reserve is each open position's own venue's: a Flap
/// and an OpenFour position reserve the Flap and the OpenFour approval and
/// SELL at the signing price (the counted reserve is the scan's, asserted
/// at every change); the OpenFour approval out leaves its SELL reserved.
#[test]
fn the_exit_reserve_is_each_positions_venues() {
    use crate::runtime::exec::tests::{four_observation, openfour_config};
    use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_r12d_131608421;
    let mut cfg = config(2, 4);
    cfg.strategy_lanes = 8;
    cfg.openfour = Some(openfour_config());
    let (mut exec, d) = armed("strategy-reserve", cfg.clone());
    let four = bsc_mainnet_r12d_131608421();
    exec.arm_openfour(
        four.validate(&four_observation(&four).observed, CODEHASH_GENERATION)
            .unwrap(),
    )
    .unwrap();
    let price = U256::from(cfg.gas_price_wei);
    let wei = |gas: u64| price * U256::from(gas);
    let (flap_approve, flap_sell) = (cfg.approve_gas_limit, cfg.sell_gas_limit);
    let of = openfour_config();
    assert_ne!(
        (of.approve_gas_limit, of.sell_gas_limit),
        (flap_approve, flap_sell)
    );
    assert_eq!(exec.held_exit_reserve(), U256::ZERO);
    let flap = curve_after(1_000_000_000_000_000_000);
    exec.on_strategy_buy(
        [0xF1; 20],
        [0; 20],
        flap,
        U256::from(10_000_000_000_000_000u64),
        &strategy_c2(Some(HEAD)),
        &clock(100, 3),
        100,
        None,
    )
    .unwrap();
    assert_eq!(exec.held_exit_reserve(), wei(flap_approve) + wei(flap_sell));
    let (curve, token, amount) = openfour_strategy_state("131615000");
    exec.on_strategy_buy(
        token,
        [0; 20],
        StrategyCurve::Four(curve),
        U256::from(10_000_000_000_000_000u64),
        &strategy_c2(Some(HEAD)),
        &clock(200, 3),
        200,
        None,
    )
    .unwrap();
    let both =
        wei(flap_approve) + wei(flap_sell) + wei(of.approve_gas_limit) + wei(of.sell_gas_limit);
    assert_eq!(exec.held_exit_reserve(), both);
    exec.adopt_approval(token, amount);
    assert_eq!(exec.held_exit_reserve(), both - wei(of.approve_gas_limit));
    // SC4 d: a lot re-read at its venue owes that venue's exit; one of an
    // unconfigured venue (Classic here) the Flap one.
    exec.adopt(
        [0xA0; 20],
        U256::from(5u8),
        crate::sources::VenueId::OpenFour,
    );
    let adopted = both + wei(of.sell_gas_limit);
    assert_eq!(exec.held_exit_reserve(), adopted);
    exec.adopt(
        [0xA1; 20],
        U256::from(5u8),
        crate::sources::VenueId::FourClassic,
    );
    assert_eq!(
        exec.held_exit_reserve(),
        adopted + wei(flap_approve) + wei(flap_sell)
    );
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
        exec.on_strategy_buy([1; 20], [0; 20], curve, input, &none, &clock(1, 1), 1, None),
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
            1,
            None,
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
        exec.on_strategy_buy([1; 20], [0; 20], curve, input, &late, &clock(1, 1), 1, None),
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
            1,
            None,
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
        None,
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
            None,
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
                None,
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
    exec.adopt([9; 20], U256::from(5u8), crate::sources::VenueId::Flap);
    same(
        &exec,
        U256::from(2u8) * (approval + sell + sell),
        "position adopted",
    );
    // Adopted again: still one position.
    exec.adopt([9; 20], U256::from(6u8), crate::sources::VenueId::Flap);
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
                from: [0; 20],
                to: Some(portal),
                value: &[],
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
            None,
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
            None,
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
            1,
            None,
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
        from: [0; 20],
        to: Some(portal),
        value: &[],
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
        .on_strategy_buy([1; 20], [0; 20], curve, input, &c2, &clock(1, 1), 1, None)
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
        .on_strategy_buy([1; 20], [0; 20], curve, input, &c2, &clock(1, 1), 1, None)
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
            1,
            None,
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
            1,
            None,
        ),
        Err(Refusal::Dispatch)
    );
    assert_eq!(
        exec.on_strategy_buy([4; 20], [0; 20], curve, input, &c2, &clock(1, 1), 1, None),
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
        from: [0; 20],
        to: Some(portal),
        value: &[],
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
            None,
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
            None,
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

/// B5: refusals by code: an admission's with its cause, every other
/// refusal's by its name (each profile reason one PROFILE), each code once
/// with its sum, in first-seen order.
#[test]
fn refusals_are_coded_by_their_cause() {
    use darkbot_bsc_trading::venues::fourmeme::profile::UnsupportedClassicLaunch;
    let refusals = [
        (Refusal::Admission(AdmissionCause::Quote), 2),
        (Refusal::FourProfile(UnsupportedClassicLaunch::Layout), 1),
        (Refusal::Admission(AdmissionCause::Terms), 1),
        (
            Refusal::FourProfile(UnsupportedClassicLaunch::TotalSupply),
            3,
        ),
        (Refusal::Admission(AdmissionCause::Quote), 4),
        (Refusal::SessionCap, 1),
    ];
    assert_eq!(
        crate::paper_exec::coded(refusals.iter()),
        [
            "ADMISSION_QUOTE=6",
            "PROFILE=4",
            "ADMISSION_TERMS=1",
            "SESSION_CAP=1"
        ]
    );
    for (cause, code) in [
        (AdmissionCause::Stale, "ADMISSION_STALE"),
        (AdmissionCause::Terms, "ADMISSION_TERMS"),
        (AdmissionCause::Unwitnessed, "ADMISSION_UNWITNESSED"),
        (AdmissionCause::Quote, "ADMISSION_QUOTE"),
        (AdmissionCause::Slippage, "ADMISSION_SLIPPAGE"),
        (AdmissionCause::Policy, "ADMISSION_POLICY"),
    ] {
        assert_eq!(
            (
                Refusal::Admission(cause).code(),
                Refusal::Admission(cause).name()
            ),
            (code, "ADMISSION")
        );
    }
}

use alloy_primitives::{keccak256, Address, Bytes, U256, U512};
use alloy_sol_types::{sol, SolCall, SolValue};
use k256::ecdsa::SigningKey;
use serde_json::{json, Value};
use std::{
    env,
    fs,
    hint::black_box,
    os::unix::net::UnixDatagram,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Instant,
};

sol! {
    struct LaunchParams {
        string name;
        string symbol;
        string uri;
        uint8 dexThresh;
        bytes32 salt;
        uint8 migratorType;
        address quoteToken;
        uint256 quoteAmt;
        address beneficiary;
        bytes permit;
        bytes32 extensionId;
        bytes extensionData;
        uint8 dexId;
        uint8 lpFeeProfile;
        uint16 buyTax;
        uint16 sellTax;
        uint64 taxDuration;
        uint64 antiFarmerDuration;
        uint16 mktBps;
        uint16 deflationBps;
        uint16 dividendBps;
        uint16 lpBps;
        uint256 minimumShareBalance;
        address dividendToken;
        address commissionReceiver;
        uint8 tokenVersion;
    }

    function newTokenV6(LaunchParams params) external payable returns(address);

    struct SwapParams {
        address inputToken;
        address outputToken;
        uint256 inputAmount;
        uint256 minOutputAmount;
        bytes permit;
    }

    function swapExactInput(SwapParams params) external payable returns(uint256);
}

fn n(s: &str) -> U256 {
    U256::from_str(s).unwrap()
}

fn wide(x: U256) -> U512 {
    U512::from(x)
}

fn narrow(x: U512) -> U256 {
    assert!(x <= U512::from(U256::MAX));
    U256::from(x)
}

fn mul_div(a: U256, b: U256, d: U256) -> U256 {
    narrow(wide(a) * wide(b) / wide(d))
}

fn ceil_div(a: U256, b: U256) -> U256 {
    let q = a / b;
    if a % b == U256::ZERO {
        q
    } else {
        q + U256::ONE
    }
}

fn min_output(expected: U256, bps: u16) -> U256 {
    mul_div(
        expected,
        U256::from(10000u64 - bps as u64),
        U256::from(10000u64),
    )
}

fn rlp_bytes(b: &[u8]) -> Vec<u8> {
    if b.len() == 1 && b[0] < 0x80 {
        return b.to_vec();
    }

    if b.len() <= 55 {
        let mut out = Vec::with_capacity(1 + b.len());
        out.push(0x80 + b.len() as u8);
        out.extend_from_slice(b);
        return out;
    }

    let len_bytes = b.len().to_be_bytes();
    let first = len_bytes.iter().position(|x| *x != 0).unwrap();
    let lb = &len_bytes[first..];

    let mut out = Vec::with_capacity(1 + lb.len() + b.len());
    out.push(0xb7 + lb.len() as u8);
    out.extend_from_slice(lb);
    out.extend_from_slice(b);
    out
}

fn rlp_u64(v: u64) -> Vec<u8> {
    if v == 0 {
        return rlp_bytes(&[]);
    }

    let b = v.to_be_bytes();
    let first = b.iter().position(|x| *x != 0).unwrap();
    rlp_bytes(&b[first..])
}

fn rlp_u256(v: U256) -> Vec<u8> {
    if v == U256::ZERO {
        return rlp_bytes(&[]);
    }

    let b = v.to_be_bytes::<32>();
    let first = b.iter().position(|x| *x != 0).unwrap();
    rlp_bytes(&b[first..])
}

fn rlp_list(items: Vec<Vec<u8>>) -> Vec<u8> {
    let payload_len: usize = items.iter().map(Vec::len).sum();
    let mut payload = Vec::with_capacity(payload_len);

    for item in items {
        payload.extend_from_slice(&item);
    }

    if payload.len() <= 55 {
        let mut out = Vec::with_capacity(1 + payload.len());
        out.push(0xc0 + payload.len() as u8);
        out.extend_from_slice(&payload);
        return out;
    }

    let len_bytes = payload.len().to_be_bytes();
    let first = len_bytes.iter().position(|x| *x != 0).unwrap();
    let lb = &len_bytes[first..];

    let mut out = Vec::with_capacity(1 + lb.len() + payload.len());
    out.push(0xf7 + lb.len() as u8);
    out.extend_from_slice(lb);
    out.extend_from_slice(&payload);
    out
}

#[derive(Default)]
struct Sample {
    decode: u64,
    quote: u64,
    finalize: u64,
    sign: u64,
    serialize: u64,
    handoff: u64,
    ready: u64,
    total: u64,
}

struct Ctx {
    launch: Vec<u8>,
    portal: Address,
    init_hash: [u8; 32],
    sqrt: U256,
    liquidity: U256,
    input: U256,
    timestamp: U256,
    slippage_bps: u16,
    signer: SigningKey,
    expected_token: Address,
    expected_output: U256,
    expected_min: U256,
    expected_calldata: Vec<u8>,
}

fn run_once(
    ctx: &Ctx,
    sock: &UnixDatagram,
    nonce: u64,
    correctness_check: bool,
) -> Sample {
    let t0 = Instant::now();

    // 1. Minimal launch decode + scope qualification + CREATE2 token address.
    let lp = newTokenV6Call::abi_decode_validate(&ctx.launch)
        .unwrap()
        .params;

    assert_eq!(lp.tokenVersion, 6);
    assert_eq!(lp.buyTax, 100);
    assert_eq!(lp.sellTax, 100);

    let doge =
        Address::from_str("0xba2ae424d960c26247dd6c32edc70b295c744c43").unwrap();

    assert_eq!(lp.quoteToken, doge);

    let (mode, price, deadline, _signature) =
        <(U256, U256, U256, Bytes)>::abi_decode_params_validate(
            &lp.extensionData,
        )
        .unwrap();

    assert_eq!(mode, U256::ZERO);
    assert!(ctx.timestamp <= deadline);

    let mut create2 = Vec::with_capacity(85);
    create2.push(0xff);
    create2.extend_from_slice(ctx.portal.as_slice());
    create2.extend_from_slice(lp.salt.as_slice());
    create2.extend_from_slice(&ctx.init_hash);

    let digest = keccak256(&create2);
    let token = Address::from_slice(&digest.as_slice()[12..]);

    let t1 = Instant::now();

    // 2. Qualified G0.3 local quote/residual calculation.
    let net = mul_div(
        ctx.input,
        U256::from(997500u64),
        U256::from(1_000_000u64),
    );

    let next =
        ctx.sqrt + mul_div(net, U256::ONE << 96, ctx.liquidity);

    let quote = narrow(
        (wide(ctx.liquidity) << 96)
            * wide(next - ctx.sqrt)
            / wide(next)
            / wide(ctx.sqrt),
    );

    let protocol = quote / U256::from(100u64);
    let combined = quote / U256::from(50u64);
    let _tax = combined - protocol;

    let r = mul_div(
        n("3837000000000000000000"),
        n("1000000000000000000"),
        price,
    );

    let h = n("107036751000000000000000000");
    let total = n("1000000000000000000000000000");

    let k = mul_div(
        r,
        total + h,
        n("1000000000000000000"),
    );

    let supply = |q: U256| {
        total + h
            - ceil_div(
                k * n("1000000000000000000"),
                r + q * n("10000000000"),
            )
    };

    let creator_reserve =
        lp.quoteAmt - lp.quoteAmt / U256::from(50u64);

    let creator_supply = supply(creator_reserve);

    let output =
        supply(creator_reserve + quote - combined) - creator_supply;

    let minimum = min_output(output, ctx.slippage_bps);

    let t2 = Instant::now();

    // 3. BUY calldata + unsigned EIP-155 transaction.
    let calldata = swapExactInputCall {
        params: SwapParams {
            inputToken: Address::ZERO,
            outputToken: token,
            inputAmount: ctx.input,
            minOutputAmount: minimum,
            permit: Bytes::new(),
        },
    }
    .abi_encode();

    let unsigned = rlp_list(vec![
        rlp_u64(nonce),
        rlp_u64(310_000_000), // historical fixture gasPrice, pre-resident policy input
        rlp_u64(1_500_000),   // historical fixture gas limit, pre-resident policy input
        rlp_bytes(ctx.portal.as_slice()),
        rlp_u256(ctx.input),
        rlp_bytes(&calldata),
        rlp_u64(56),
        rlp_bytes(&[]),
        rlp_bytes(&[]),
    ]);

    let sighash = keccak256(&unsigned);

    let t3 = Instant::now();

    // 4. Warm in-process secp256k1 signer.
    let (sig, recovery_id) = ctx
        .signer
        .sign_prehash_recoverable(sighash.as_slice())
        .unwrap();

    let t4 = Instant::now();

    // 5. Final signed legacy transaction serialization.
    let sig_bytes = sig.to_bytes();
    let v = 35u64 + (2 * 56) + recovery_id.to_byte() as u64;

    let signed = rlp_list(vec![
        rlp_u64(nonce),
        rlp_u64(310_000_000),
        rlp_u64(1_500_000),
        rlp_bytes(ctx.portal.as_slice()),
        rlp_u256(ctx.input),
        rlp_bytes(&calldata),
        rlp_u64(v),
        rlp_u256(U256::from_be_slice(&sig_bytes[..32])),
        rlp_u256(U256::from_be_slice(&sig_bytes[32..])),
    ]);

    let t5 = Instant::now();

    if correctness_check {
        assert_eq!(token, ctx.expected_token);
        assert_eq!(output, ctx.expected_output);
        assert_eq!(minimum, ctx.expected_min);
        assert_eq!(calldata, ctx.expected_calldata);
    }

    black_box(&signed);

    // 6. Local adapter boundary only.
    // This is NOT a real BSC/geth/builder T_send.
    sock.send(&signed).unwrap();

    let t6 = Instant::now();

    Sample {
        decode: (t1 - t0).as_nanos() as u64,
        quote: (t2 - t1).as_nanos() as u64,
        finalize: (t3 - t2).as_nanos() as u64,
        sign: (t4 - t3).as_nanos() as u64,
        serialize: (t5 - t4).as_nanos() as u64,
        handoff: (t6 - t5).as_nanos() as u64,
        ready: (t5 - t0).as_nanos() as u64,
        total: (t6 - t0).as_nanos() as u64,
    }
}

fn stats(mut v: Vec<u64>) -> Value {
    v.sort_unstable();

    let at = |num: usize, den: usize| -> u64 {
        let idx = ((v.len() - 1) * num) / den;
        v[idx]
    };

    let sum: u128 = v.iter().map(|x| *x as u128).sum();

    json!({
        "p50_ns": at(50,100),
        "p95_ns": at(95,100),
        "p99_ns": at(99,100),
        "max_ns": *v.last().unwrap(),
        "mean_ns": (sum / v.len() as u128) as u64,
        "p50_us": at(50,100) as f64 / 1000.0,
        "p95_us": at(95,100) as f64 / 1000.0,
        "p99_us": at(99,100) as f64 / 1000.0,
        "max_us": *v.last().unwrap() as f64 / 1000.0
    })
}

fn main() {
    let args: Vec<String> = env::args().collect();

    assert!(
        args.len() == 3,
        "g05_local <g03_rust_inputs.json> <g03_rust_output.json>"
    );

    let fi: Value =
        serde_json::from_str(&fs::read_to_string(&args[1]).unwrap()).unwrap();

    let fo: Value =
        serde_json::from_str(&fs::read_to_string(&args[2]).unwrap()).unwrap();

    let launch =
        hex::decode(fi["launch_calldata"].as_str().unwrap().trim_start_matches("0x"))
            .unwrap();

    assert_eq!(
        keccak256(&launch).to_string(),
        fi["launch_calldata_keccak"].as_str().unwrap()
    );

    let init = hex::decode(
        "3d602d80600a3d3981f3363d3d373d3d3d363d73024f18294970b5c76c0691b87f138a03171564225af43d82803e903d91602b57fd5bf3"
    ).unwrap();

    let init_hash = keccak256(init);

    // Fixed NON-FUNDED test key used only to benchmark secp256k1 signing cost.
    let test_key = [0x11u8; 32];
    let signer = SigningKey::from_bytes((&test_key).into()).unwrap();

    let ctx = Ctx {
        launch,
        portal: Address::from_str(
            "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0",
        )
        .unwrap(),
        init_hash: init_hash.into(),
        sqrt: n(fi["sqrt_price_x96"].as_str().unwrap()),
        liquidity: n(fi["liquidity"].as_str().unwrap()),
        input: n(fi["native_input"].as_str().unwrap()),
        timestamp: n(fi["timestamp"].as_str().unwrap()),
        slippage_bps: fi["test_slippage_bps"]
            .as_u64()
            .unwrap()
            .try_into()
            .unwrap(),
        signer,
        expected_token: Address::from_str(
            fi["expected_token"].as_str().unwrap(),
        )
        .unwrap(),
        expected_output: n(fo["expected_output"].as_str().unwrap()),
        expected_min: n(fo["min_output"].as_str().unwrap()),
        expected_calldata: hex::decode(
            fo["calldata"].as_str().unwrap().trim_start_matches("0x"),
        )
        .unwrap(),
    };

    let (sender, receiver) = UnixDatagram::pair().unwrap();

    let running = Arc::new(AtomicBool::new(true));
    let rr = running.clone();

    let receiver_thread = thread::spawn(move || {
        let mut buf = [0u8; 4096];

        while rr.load(Ordering::Relaxed) {
            match receiver.recv(&mut buf) {
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });

    // Correctness check against G0.3 frozen output.
    run_once(&ctx, &sender, 0x032e, true);

    const WARMUP: usize = 5_000;
    const SAMPLES: usize = 50_000;

    for i in 0..WARMUP {
        black_box(run_once(
            &ctx,
            &sender,
            0x032e + (i as u64 & 0x3ff),
            false,
        ));
    }

    let mut decode = Vec::with_capacity(SAMPLES);
    let mut quote = Vec::with_capacity(SAMPLES);
    let mut finalize = Vec::with_capacity(SAMPLES);
    let mut sign = Vec::with_capacity(SAMPLES);
    let mut serialize = Vec::with_capacity(SAMPLES);
    let mut handoff = Vec::with_capacity(SAMPLES);
    let mut ready = Vec::with_capacity(SAMPLES);
    let mut total = Vec::with_capacity(SAMPLES);

    for i in 0..SAMPLES {
        let s = run_once(
            &ctx,
            &sender,
            0x032e + (i as u64 & 0x3ff),
            false,
        );

        decode.push(s.decode);
        quote.push(s.quote);
        finalize.push(s.finalize);
        sign.push(s.sign);
        serialize.push(s.serialize);
        handoff.push(s.handoff);
        ready.push(s.ready);
        total.push(s.total);
    }

    running.store(false, Ordering::Relaxed);
    let _ = sender.send(&[0]);
    let _ = receiver_thread.join();

    let total_stats = stats(total);

    let result = json!({
        "status": "G0.5_LOCAL_PREQUALIFICATION",
        "scope": "Pinned G0.3 Flap fixture; release build; local controllable path only",
        "architecture": std::env::consts::ARCH,
        "os": std::env::consts::OS,
        "warmup_samples": WARMUP,
        "measured_samples": SAMPLES,
        "correctness": {
            "g03_fixture_match": true,
            "expected_token": ctx.expected_token.to_string(),
            "expected_output": ctx.expected_output.to_string(),
            "expected_min_output": ctx.expected_min.to_string()
        },
        "stages": {
            "decode_and_create2": stats(decode),
            "quote_and_min_output": stats(quote),
            "tx_finalize_and_sighash": stats(finalize),
            "secp256k1_sign": stats(sign),
            "signed_tx_serialize": stats(serialize),
            "unix_datagram_handoff": stats(handoff),
            "T_pre_to_T_ready_local": stats(ready),
            "T_pre_to_local_handoff": total_stats
        },
        "target": {
            "local_prequalification_ns": 1_000_000,
            "final_real_T_send_target_ns": 1_000_000
        },
        "non_claims": [
            "UnixDatagram send-return is only a local adapter boundary",
            "This is NOT a real bsc-geth P2P T_send",
            "This is NOT a builder T_send",
            "This is NOT T_buy",
            "This does not close final G0.5"
        ]
    });

    println!("{}", serde_json::to_string_pretty(&result).unwrap());
}

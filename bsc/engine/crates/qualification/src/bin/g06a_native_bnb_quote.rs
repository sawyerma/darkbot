//! G0.6a local Curve-20 quote for the DRY toolchain (read-only, no network).
//!
//! Migrated from the retired Gate-0 harness. The arithmetic is the single
//! production owner `darkbot_bsc_trading::economics::flap::native_curve20` (a port
//! of the Gate-0 quote with identical operation order); the minimum output
//! is `darkbot_bsc_trading::economics::slippage::min_output`. It reads the
//! binary preflight record and writes the binary quote record (DBREC001, no
//! JSON); stdout is the record's derived view. Run from the repository root.

use alloy_primitives::U256;
use darkbot_bsc_record::schema::{
    PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL,
    g06a::{preflight as p, quote as q},
};
use darkbot_bsc_record::{Draft, Value, decode, sha256};
use darkbot_bsc_trading::economics::flap::native_curve20::{
    CURVE_ID_RH_BNB as CURVE_RH_BNB_ID, HoldingCap, NativeBuyInputs, PROFILE_DEX_SUPPLY_THRESHOLD,
    quote_native_buy,
};
use darkbot_bsc_trading::economics::slippage::min_output;
use std::io::Write;
use std::{env, fs};

const DEFAULT_BUY_VALUE_WEI: u64 = 1_000_000_000_000_000;
const SLIPPAGE_BPS: u64 = 100;

fn word(x: U256) -> Value {
    Value::U256(x.to_be_bytes())
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let [preflight_path, out_path] = args.as_slice() else {
        panic!("usage: g06a_native_bnb_quote <preflight.dbrec> <quote.dbrec>");
    };
    let bytes = fs::read(preflight_path).expect("cannot read preflight record");
    let v = decode(&p::SCHEMA, &bytes).expect("invalid preflight record");
    let u256 = |tag| U256::from_be_bytes(v.u256(tag).expect("preflight field"));

    assert_eq!(v.u64(p::CHAIN_ID).expect("chain_id"), 56);
    assert_eq!(u256(p::QUOTE_ENABLED), U256::from(1));
    assert_eq!(u256(p::QUOTE_DEFAULT_CURVE_ID), U256::from(20));
    assert_eq!(u256(p::QUOTE_ALTERNATIVE_CURVE_ID), U256::from(20));
    assert_eq!(u256(p::QUOTE_NATIVE_TO_QUOTE_SWAP_TYPE), U256::ZERO);
    assert_eq!(u256(p::QUOTE_DEX_ID), U256::ZERO);
    assert_eq!(u256(p::FEE_BUY_BPS), U256::from(100));
    assert!(v.bool(p::TAX_ON_BONDING_CURVE_ENABLED).expect("tax flag"));

    let buy_value = env::var("DARKBOT_G06A_BUY_VALUE_WEI")
        .ok()
        .map(|x| {
            x.parse::<u64>()
                .expect("DARKBOT_G06A_BUY_VALUE_WEI invalid")
        })
        .unwrap_or(DEFAULT_BUY_VALUE_WEI);

    let quote = quote_native_buy(&NativeBuyInputs {
        curve_id: CURVE_RH_BNB_ID,
        reserve_before: U256::ZERO,
        circulating_before: U256::ZERO,
        input_amount: U256::from(buy_value),
        protocol_fee_bps: 100,
        buy_tax_bps: 100,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
        holding_cap: HoldingCap::None,
        dex_supply_threshold: U256::from(PROFILE_DEX_SUPPLY_THRESHOLD),
    })
    .unwrap_or_else(|e| panic!("native Curve-20 quote failed: {e}"));
    let min_output = min_output(quote.output_amount, SLIPPAGE_BPS as u16)
        .unwrap_or_else(|e| panic!("minimum output failed: {e}"));

    let observed = darkbot_bsc_record::observed_now()
        .unwrap_or_else(|e| panic!("provenance clock: {e}; no quote record"));
    let mut d = Draft::new(&q::SCHEMA);
    d.set(PROVENANCE_TOOL, "g06a_native_bnb_quote")
        .set(PROVENANCE_OBSERVED_UNIX_MS, observed)
        .set(q::PREFLIGHT_RECORD_SHA256, Value::B256(sha256(&bytes)))
        .set(q::CURVE_ID, u64::from(CURVE_RH_BNB_ID))
        .set(q::BUY_VALUE_WEI, word(U256::from(buy_value)))
        .set(q::PROTOCOL_FEE_BPS, 100u64)
        .set(q::BUY_TAX_BPS, 100u64)
        .set(q::TAX_ON_BONDING_CURVE_ENABLED, true)
        .set(q::PROTOCOL_FEE_WEI, word(quote.protocol_fee))
        .set(q::BONDING_CURVE_TAX_WEI, word(quote.bonding_curve_tax))
        .set(q::COMBINED_FEE_WEI, word(quote.combined_fee))
        .set(q::NET_RESERVE_INPUT_WEI, word(quote.net_reserve_input))
        .set(q::RESERVE_AFTER_WEI, word(quote.reserve_after))
        .set(q::CIRCULATING_BEFORE, word(U256::ZERO))
        .set(q::EXPECTED_OUTPUT, word(quote.output_amount))
        .set(q::SLIPPAGE_BPS, SLIPPAGE_BPS)
        .set(q::MIN_OUTPUT, word(min_output))
        .set(q::NETWORK_ACTIONS, "NONE")
        .set(
            q::AUTHORITY,
            "Binary Mainnet preflight + exact Curve-20 fixed-point implementation; requires independent EVM cross-check before DRY-2 PASS",
        );
    let record = d.encode().expect("encode quote record");
    let view = decode(&q::SCHEMA, &record)
        .expect("decode quote record")
        .render();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out_path)
        .expect("create-new quote record");
    file.write_all(&record)
        .and_then(|()| file.sync_all())
        .expect("write quote record");
    print!("{view}");
}

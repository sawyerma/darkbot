//! G0.6a local Curve-20 quote for the DRY toolchain (read-only, no network).
//!
//! Migrated from the retired Gate-0 harness. The arithmetic is the single
//! production owner `darkbot_core::economics::flap::native_curve20` (a port
//! of the Gate-0 quote with identical operation order); the minimum output
//! is `darkbot_core::economics::slippage::min_output`. Inputs, output
//! fields and JSON layout are unchanged. Run from the repository root.

use alloy_primitives::U256;
use darkbot_core::economics::flap::native_curve20::{
    CURVE_ID_RH_BNB as CURVE_RH_BNB_ID, NativeBuyInputs, quote_native_buy,
};
use darkbot_core::economics::slippage::min_output;
use serde_json::Value;
use std::{env, fs};

const DEFAULT_PREFLIGHT: &str = "bsc/config/g06a_preflight_readonly.json";
const DEFAULT_BUY_VALUE_WEI: u64 = 1_000_000_000_000_000;
const SLIPPAGE_BPS: u64 = 100;

fn main() {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_PREFLIGHT.to_string());
    let v: Value = serde_json::from_str(&fs::read_to_string(&path).expect("cannot read preflight"))
        .expect("invalid preflight JSON");

    assert_eq!(
        v["schema"].as_str(),
        Some("darkbot.g06a.preflight-readonly.v2")
    );
    assert_eq!(v["chain_id"].as_str(), Some("56"));
    assert_eq!(
        v["native_quote_configuration"]["enabled"].as_str(),
        Some("1")
    );
    assert_eq!(
        v["native_quote_configuration"]["default_curve_id"].as_str(),
        Some("20")
    );
    assert_eq!(
        v["native_quote_configuration"]["alternative_curve_id"].as_str(),
        Some("20")
    );
    assert_eq!(
        v["native_quote_configuration"]["native_to_quote_swap_type"].as_str(),
        Some("0")
    );
    assert_eq!(
        v["native_quote_configuration"]["dex_id"].as_str(),
        Some("0")
    );
    assert_eq!(v["protocol_fee_bps"]["buy"].as_str(), Some("100"));
    assert_eq!(v["tax_on_bonding_curve_enabled"].as_bool(), Some(true));

    let buy_value = env::var("DARKBOT_G06A_BUY_VALUE_WEI")
        .ok()
        .map(|x| {
            x.parse::<u64>()
                .expect("DARKBOT_G06A_BUY_VALUE_WEI invalid")
        })
        .unwrap_or(DEFAULT_BUY_VALUE_WEI);

    let q = quote_native_buy(&NativeBuyInputs {
        curve_id: CURVE_RH_BNB_ID,
        reserve_before: U256::ZERO,
        circulating_before: U256::ZERO,
        input_amount: U256::from(buy_value),
        protocol_fee_bps: 100,
        buy_tax_bps: 100,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
    })
    .unwrap_or_else(|e| panic!("native Curve-20 quote failed: {e}"));
    let min_output = min_output(q.output_amount, SLIPPAGE_BPS as u16)
        .unwrap_or_else(|e| panic!("minimum output failed: {e}"));

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema": "darkbot.g06a.native-bnb-quote.v1",
            "preflight_path": path,
            "curve_id": CURVE_RH_BNB_ID,
            "buy_value_wei": buy_value.to_string(),
            "protocol_fee_bps": 100,
            "buy_tax_bps": 100,
            "tax_on_bonding_curve_enabled": true,
            "protocol_fee_wei": q.protocol_fee.to_string(),
            "bonding_curve_tax_wei": q.bonding_curve_tax.to_string(),
            "combined_fee_wei": q.combined_fee.to_string(),
            "net_reserve_input_wei": q.net_reserve_input.to_string(),
            "reserve_after_wei": q.reserve_after.to_string(),
            "circulating_before": "0",
            "expected_output": q.output_amount.to_string(),
            "slippage_bps": SLIPPAGE_BPS,
            "min_output": min_output.to_string(),
            "network_actions": "NONE",
            "authority": "Committed Mainnet preflight + exact Curve-20 fixed-point implementation; requires independent EVM cross-check before DRY-2 PASS"
        }))
        .unwrap()
    );
}

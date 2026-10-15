//! Read-only chain access before the guard: JSON-RPC reads and the current
//! read-only preflight compared against the frozen DRY baseline.

use super::*;

pub(crate) fn rpc(method: &str, params: Value) -> Result<Value, String> {
    let rpc = req_env("BSC_MAINNET_RPC_HTTPS");
    let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string();
    let out = Command::new("curl")
        .args([
            "-sS",
            &rpc,
            "-H",
            "Content-Type: application/json",
            "--data",
            &body,
        ])
        .output()
        .map_err(|e| format!("execute curl: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "curl {method}: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let v: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("invalid JSON-RPC response for {method}: {e}"))?;
    if let Some(err) = v.get("error") {
        return Err(format!("JSON-RPC {method}: {err}"));
    }
    v.get("result")
        .cloned()
        .ok_or_else(|| format!("JSON-RPC {method}: missing result"))
}

pub(crate) fn hex_u64(v: &Value) -> u64 {
    u64::from_str_radix(
        v.as_str()
            .expect("expected hex string")
            .trim_start_matches("0x"),
        16,
    )
    .expect("bad hex u64")
}

pub(crate) fn run_current_preflight(
    launcher: Address,
    buyer: Address,
    gas_price: u64,
    launch_gas_limit: u64,
    buy_gas_limit: u64,
    launch_value: u128,
    buy_value: u64,
) -> Value {
    let path = format!("/tmp/darkbot-g07-preflight-{}.json", std::process::id());
    let out = Command::new("node")
        .arg("bsc/tools/g06a_preflight_readonly.cjs")
        .arg(&path)
        .env("DARKBOT_G06A_LAUNCHER_ADDRESS", launcher.to_string())
        .env("DARKBOT_G06A_BUYER_ADDRESS", buyer.to_string())
        .env("DARKBOT_G06A_GAS_PRICE_WEI", gas_price.to_string())
        .env(
            "DARKBOT_G06A_LAUNCH_GAS_LIMIT",
            launch_gas_limit.to_string(),
        )
        .env("DARKBOT_G06A_BUY_GAS_LIMIT", buy_gas_limit.to_string())
        .env("DARKBOT_G06A_LAUNCH_VALUE_WEI", launch_value.to_string())
        .env("DARKBOT_G06A_BUY_VALUE_WEI", buy_value.to_string())
        .output()
        .expect("failed to execute G0.6a readonly Mainnet preflight");
    let _ = fs::remove_file(&path);
    assert!(
        out.status.success(),
        "Mainnet readonly preflight failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("readonly preflight emitted invalid JSON")
}

pub(crate) fn assert_same_preflight_field(baseline: &Value, current: &Value, ptr: &str) {
    let a = baseline
        .pointer(ptr)
        .unwrap_or_else(|| panic!("baseline preflight missing {ptr}"));
    let b = current
        .pointer(ptr)
        .unwrap_or_else(|| panic!("current preflight missing {ptr}"));
    assert_eq!(a, b, "preflight deployment/economics drift at {ptr}");
}

pub(crate) fn validate_current_preflight(
    baseline: &Value,
    current: &Value,
    launcher: Address,
    buyer: Address,
    signed_gas_price: u64,
) -> (u64, u64, u64) {
    assert_eq!(
        req_str(current, "/schema"),
        "darkbot.g06a.preflight-readonly.v2"
    );
    assert_eq!(req_u64_string(current, "/chain_id"), CHAIN_ID);
    assert_eq!(norm(req_str(current, "/portal")), norm(PORTAL));
    assert_eq!(
        norm(req_str(current, "/launcher_address")),
        norm(&launcher.to_string())
    );
    assert_eq!(
        norm(req_str(current, "/buyer_address")),
        norm(&buyer.to_string())
    );
    assert!(req_bool(current, "/wallet_qualification/addresses_bound"));
    assert!(req_bool(current, "/wallet_qualification/distinct"));
    assert!(req_bool(current, "/wallet_qualification/funding_pass"));

    for ptr in [
        "/chain_id",
        "/portal",
        "/portal_code_sha256",
        "/portal_version",
        "/portal_implementation",
        "/portal_implementation_code_sha256",
        "/tax_v3_impl",
        "/tax_v3_impl_code_sha256",
        "/native_quote_configuration",
        "/protocol_fee_bps",
        "/tax_on_bonding_curve_enabled",
    ] {
        assert_same_preflight_field(baseline, current, ptr);
    }

    let launcher_nonce = req_u64_string(current, "/launcher_pending_nonce");
    let buyer_nonce = req_u64_string(current, "/buyer_pending_nonce");

    let network_gas_price = req_u64_string(current, "/gas_price_wei");
    assert!(
        network_gas_price <= signed_gas_price,
        "current network gas price {network_gas_price} exceeds signed gas price {signed_gas_price}"
    );

    (
        launcher_nonce,
        buyer_nonce,
        req_u64_string(current, "/latest_block_number"),
    )
}

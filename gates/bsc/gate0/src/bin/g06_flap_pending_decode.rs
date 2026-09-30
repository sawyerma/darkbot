use alloy_primitives::{Address, Bytes, U256};
use alloy_sol_types::{sol, SolCall, SolValue};
use serde_json::{json, Value};
use std::{env, fs, str::FromStr};

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
}

fn main() {
    let args: Vec<String> = env::args().collect();
    assert_eq!(args.len(), 2, "g06_flap_pending_decode <pending_sample.json>");

    let d: Value = serde_json::from_str(&fs::read_to_string(&args[1]).unwrap()).unwrap();
    let tx = &d["pending_transaction"];
    assert!(!tx.is_null(), "no pending Flap launch sample in evidence");

    let to = Address::from_str(tx["to"].as_str().unwrap()).unwrap();
    let portal = Address::from_str("0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0").unwrap();
    assert_eq!(to, portal, "pending tx is not the frozen Flap Portal");

    let raw = hex::decode(tx["input"].as_str().unwrap().trim_start_matches("0x")).unwrap();
    let call = newTokenV6Call::abi_decode_validate(&raw).unwrap();
    let reencoded = call.abi_encode();
    assert_eq!(reencoded, raw, "ABI roundtrip mismatch");

    let lp = call.params;
    let doge = Address::from_str("0xba2ae424d960c26247dd6c32edc70b295c744c43").unwrap();
    let frozen_extension = "0x3526f6a0e790306d64c896eeda5ec66052ea17889e20632ff0cf512923153dad";

    let extension_hex = lp.extensionId.to_string();
    let exact_scope_match =
        lp.tokenVersion == 6
        && lp.quoteToken == doge
        && extension_hex.eq_ignore_ascii_case(frozen_extension);

    let extension_decode = <(U256, U256, U256, Bytes)>::abi_decode_params_validate(
        &lp.extensionData
    );

    let (extension_mode, extension_price, extension_deadline, extension_signature_len) =
        match extension_decode {
            Ok((mode, price, deadline, sig)) => (
                Some(mode.to_string()),
                Some(price.to_string()),
                Some(deadline.to_string()),
                Some(sig.len())
            ),
            Err(_) => (None, None, None, None)
        };

    println!("{}", serde_json::to_string_pretty(&json!({
        "status": "G0.6_FLAP_PENDING_FIELD_COMPLETENESS",
        "result": if exact_scope_match { "PASS_EXACT_FROZEN_SCOPE" } else { "PASS_DECODE_ONLY_SCOPE_MISMATCH" },
        "tx_hash": tx["hash"],
        "portal": to.to_string(),
        "abi_roundtrip": true,
        "selector": "0x8cb5772c",
        "decoded": {
            "name": lp.name,
            "symbol": lp.symbol,
            "uri": lp.uri,
            "dex_thresh": lp.dexThresh,
            "salt": lp.salt.to_string(),
            "migrator_type": lp.migratorType,
            "quote_token": lp.quoteToken.to_string(),
            "quote_amount": lp.quoteAmt.to_string(),
            "beneficiary": lp.beneficiary.to_string(),
            "permit_len": lp.permit.len(),
            "extension_id": extension_hex,
            "extension_data_len": lp.extensionData.len(),
            "dex_id": lp.dexId,
            "lp_fee_profile": lp.lpFeeProfile,
            "buy_tax": lp.buyTax,
            "sell_tax": lp.sellTax,
            "tax_duration": lp.taxDuration,
            "anti_farmer_duration": lp.antiFarmerDuration,
            "mkt_bps": lp.mktBps,
            "deflation_bps": lp.deflationBps,
            "dividend_bps": lp.dividendBps,
            "lp_bps": lp.lpBps,
            "minimum_share_balance": lp.minimumShareBalance.to_string(),
            "dividend_token": lp.dividendToken.to_string(),
            "commission_receiver": lp.commissionReceiver.to_string(),
            "token_version": lp.tokenVersion
        },
        "extension_tuple": {
            "mode": extension_mode,
            "price": extension_price,
            "deadline": extension_deadline,
            "signature_len": extension_signature_len
        },
        "frozen_scope_check": {
            "token_version_6": lp.tokenVersion == 6,
            "quote_token_doge": lp.quoteToken == doge,
            "extension_id_match": extension_hex.eq_ignore_ascii_case(frozen_extension),
            "exact_scope_match": exact_scope_match
        },
        "safety": {
            "network_access": false,
            "transaction_submission": false,
            "value_bearing_buy": false
        },
        "non_claims": [
            "Field completeness does not prove canonical coverage",
            "Field completeness does not prove inclusion, ordering or acquisition",
            "No T0, T_send or T_buy measurement"
        ]
    })).unwrap());
}

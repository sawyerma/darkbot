//! G0.3 correctness fixture. No network, keys, signing, sender or latency claim.
use alloy_primitives::{Address, Bytes, U256, U512, keccak256};
use alloy_sol_types::{SolCall, SolValue, sol};
use serde_json::{Value, json};
use std::{env, fs, str::FromStr};
sol! {
 struct LaunchParams {
  string name; string symbol; string uri; uint8 dexThresh; bytes32 salt; uint8 migratorType;
  address quoteToken; uint256 quoteAmt; address beneficiary; bytes permit;
  bytes32 extensionId; bytes extensionData; uint8 dexId; uint8 lpFeeProfile;
  uint16 buyTax; uint16 sellTax; uint64 taxDuration; uint64 antiFarmerDuration;
  uint16 mktBps; uint16 deflationBps; uint16 dividendBps; uint16 lpBps; uint256 minimumShareBalance;
  address dividendToken; address commissionReceiver; uint8 tokenVersion;
 }
 function newTokenV6(LaunchParams params) external payable returns(address);
 struct SwapParams {address inputToken; address outputToken; uint256 inputAmount; uint256 minOutputAmount; bytes permit;}
 function swapExactInput(SwapParams params) external payable returns(uint256);
}
fn n(s: &str) -> U256 {
    U256::from_str(s).unwrap()
}
fn wide(x: U256) -> U512 {
    U512::from(x)
}
fn narrow(x: U512) -> U256 {
    {
        assert!(
            x <= U512::from(U256::MAX),
            "overflow outside qualified scope"
        );
        U256::from(x)
    }
}
fn mul_div(a: U256, b: U256, d: U256) -> U256 {
    assert!(!d.is_zero());
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
    assert!(bps < 10000, "invalid test slippage policy");
    mul_div(expected, U256::from(10000 - bps), U256::from(10000))
}
fn main() {
    let args: Vec<String> = env::args().collect();
    assert!(args.len() == 2, "g03 <qualified fixture.json>");
    let f: Value = serde_json::from_str(&fs::read_to_string(&args[1]).unwrap()).unwrap();
    let get = |k: &str| f[k].as_str().unwrap();
    let raw = hex::decode(get("launch_calldata").trim_start_matches("0x")).unwrap();
    assert_eq!(
        keccak256(&raw).to_string(),
        get("launch_calldata_keccak"),
        "unqualified launch fixture"
    );
    let lp = newTokenV6Call::abi_decode_validate(&raw).unwrap().params;
    assert_eq!(lp.tokenVersion, 6);
    assert_eq!(lp.buyTax, 100);
    assert_eq!(lp.sellTax, 100);
    let doge = Address::from_str("0xba2ae424d960c26247dd6c32edc70b295c744c43").unwrap();
    assert_eq!(lp.quoteToken, doge);
    assert_eq!(lp.dividendToken, doge);
    let (mode, price, deadline, _signature) =
        <(U256, U256, U256, Bytes)>::abi_decode_params_validate(&lp.extensionData).unwrap();
    assert_eq!(mode, U256::ZERO);
    assert!(n(get("timestamp")) <= deadline);
    // Cryptographic role/signature admission is independently checked in the EVM verifier.
    let portal = Address::from_str("0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0").unwrap();
    let init=hex::decode("3d602d80600a3d3981f3363d3d373d3d3d363d73024f18294970b5c76c0691b87f138a03171564225af43d82803e903d91602b57fd5bf3").unwrap();
    let mut preimage = vec![255];
    preimage.extend(portal.as_slice());
    preimage.extend(lp.salt.as_slice());
    preimage.extend(keccak256(init));
    let digest = keccak256(preimage);
    let token = Address::from_slice(&digest.as_slice()[12..]);
    assert_eq!(token, Address::from_str(get("expected_token")).unwrap());
    // Fixed single-segment native->DOGE V3 fixture. No claim of arbitrary tick coverage.
    let sqrt = n(get("sqrt_price_x96"));
    let liquidity = n(get("liquidity"));
    let input = n(get("native_input"));
    assert_eq!(sqrt, n("88144784934153146812079208718554"));
    assert_eq!(liquidity, n("3883217927027149621"));
    assert_eq!(input, n("150000000000000000"));
    let net = mul_div(input, U256::from(997500), U256::from(1000000));
    let next = sqrt + mul_div(net, U256::ONE << 96, liquidity);
    let quote = narrow((wide(liquidity) << 96) * wide(next - sqrt) / wide(next) / wide(sqrt));
    let protocol = quote / U256::from(100);
    let combined = quote / U256::from(50);
    let tax = combined - protocol;
    let r = mul_div(n("3837000000000000000000"), n("1000000000000000000"), price);
    let h = n("107036751000000000000000000");
    let total = n("1000000000000000000000000000");
    let k = mul_div(r, total + h, n("1000000000000000000"));
    let supply =
        |q: U256| total + h - ceil_div(k * n("1000000000000000000"), r + q * n("10000000000"));
    let creator_reserve = lp.quoteAmt - lp.quoteAmt / U256::from(50);
    let creator_supply = supply(creator_reserve);
    let output = supply(creator_reserve + quote - combined) - creator_supply;
    let bps: u16 = f["test_slippage_bps"].as_u64().unwrap().try_into().unwrap();
    let minimum = min_output(output, bps);
    let calldata = swapExactInputCall {
        params: SwapParams {
            inputToken: Address::ZERO,
            outputToken: token,
            inputAmount: input,
            minOutputAmount: minimum,
            permit: Bytes::new(),
        },
    }
    .abi_encode();
    println!(
        "{}",
        json!({"scope":"Pinned G0.3 single-segment fixture; correctness only","token":token.to_string(),"portal":portal.to_string(),"value":input.to_string(),"quote_input":quote.to_string(),"sqrt_next":next.to_string(),"creator_reserve":creator_reserve.to_string(),"creator_supply":creator_supply.to_string(),"protocol_fee":protocol.to_string(),"tax":tax.to_string(),"expected_output":output.to_string(),"test_slippage_bps":bps,"min_output":minimum.to_string(),"calldata":format!("0x{}",hex::encode(calldata)),"no_production_policy":true})
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn minimum_keeps_exact_integer_bound() {
        let x = n("30669575743216741084405716");
        assert_eq!(min_output(x, 100), n("30362879985784573673561658"));
        assert_eq!(min_output(x, 0), x);
    }
    #[test]
    #[should_panic]
    fn reject_unbounded_slippage() {
        min_output(U256::from(100), 10000);
    }
    #[test]
    fn widened_product_does_not_wrap() {
        assert_eq!(mul_div(U256::MAX, U256::from(2), U256::from(2)), U256::MAX);
    }
}

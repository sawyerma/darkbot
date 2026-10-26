//! Phase G (Economics gate §6, levels 1–2): the production Flap quote
//! against the Portal's own quote at the same pinned state, read-only.
//!
//! The trade module that executes curve trades (`PORTAL_TRADE_V2`) has no
//! verified source; the curve library (`LibCurve`) of the source-verified
//! Portal v5.27.0 has. So the local BUY and SELL arithmetic
//! (`economics::flap::native_curve20`) is validated where the protocol
//! answers itself: `Portal.quoteExactInput` (an `eth_call` at the pin) for
//! live curve-20 native-quote tokens on the bonding curve, at a grid of BUY
//! inputs and SELL fractions of the circulating supply, each compared
//! exactly (no tolerance). Token state comes from `getTokenV8Safe` at the
//! same pin. Nothing is signed or sent.
#![forbid(unsafe_code)]

#[path = "g01_common/mod.rs"]
mod common;

use alloy_primitives::{Address, B256, U256};
use alloy_sol_types::{SolCall, sol};
use common::chain::bloom_has;
use common::edge::{self, CallOutcome, Endpoint, Pin};
use common::util::{number, rpc_from_config};
use darkbot_bsc_execution::rpc_wire::Limits;
use darkbot_bsc_qualification::c1_common::Args;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    CURVE_ID_RH_BNB, NativeBuyInputs, NativeSellInputs, WITNESSED_MAINNET_TERMS, curve20,
    quote_native_buy, quote_native_sell,
};
use darkbot_bsc_trading::venues::flap::deployment::bsc_mainnet_v5_27_0;
use darkbot_bsc_trading::venues::flap::events::PortalEventKind;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

sol! {
    struct QuoteExactInputParams {
        address inputToken;
        address outputToken;
        uint256 inputAmount;
    }
    function quoteExactInput(QuoteExactInputParams params) external returns (uint256 outputAmount);
    struct TokenStateV8Safe {
        uint8 status;
        uint256 reserve;
        uint256 circulatingSupply;
        uint256 price;
        uint8 tokenVersion;
        uint256 r;
        uint256 h;
        uint256 k;
        uint256 dexSupplyThresh;
        address quoteTokenAddress;
        bool nativeToQuoteSwapEnabled;
        bytes32 extensionID;
        uint256 buyTaxRate;
        uint256 sellTaxRate;
        address pool;
        uint256 progress;
        uint8 lpFeeProfile;
        uint8 dexId;
    }
    function getTokenV8Safe(address token) external view returns (TokenStateV8Safe state);
}

/// `TokenStatus.Tradable`: on the bonding curve.
const TRADABLE: u8 = 1;

/// Every tunable bound and its default (each overridable by its flag).
mod defaults {
    pub use crate::common::util::net::*;
    /// `--scan-blocks`: newest blocks searched for `TokenCreated`.
    pub const SCAN_BLOCKS: u64 = 3_000;
    /// `--max-tokens`: tokens quoted.
    pub const MAX_TOKENS: usize = 12;
}

/// BUY inputs (wei) and SELL sizes (parts per million of the circulating
/// supply) of the grid.
const BUY_WEI: [u128; 5] = [
    1_000_000_000_000,
    1_000_000_000_000_000,
    10_000_000_000_000_000,
    100_000_000_000_000_000,
    1_000_000_000_000_000_000,
];
const SELL_PPM: [u64; 5] = [1, 100, 10_000, 100_000, 500_000];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("g02_flap_parity: {e}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse(std::env::args().skip(1));
    args.only(&["rpc", "scan-blocks", "max-tokens", "fixture-out"])?;
    let fixture_out = args.opt("fixture-out")?.map(std::path::PathBuf::from);
    let mut lines: Vec<String> = Vec::new();
    let rpc_url = match args.opt("rpc")? {
        Some(u) => u.to_string(),
        None => rpc_from_config(Path::new(defaults::RPC_CONFIG))?,
    };
    let scan: u64 = number(&args, "scan-blocks", defaults::SCAN_BLOCKS)?;
    let max_tokens: usize = number(&args, "max-tokens", defaults::MAX_TOKENS)?;
    let rpc = Endpoint::new(
        &rpc_url,
        Limits {
            connect_timeout_s: defaults::CONNECT_TIMEOUT_S,
            max_time_s: defaults::MAX_TIME_S,
            compressed: true,
        },
        defaults::ATTEMPTS,
        Duration::from_millis(defaults::RETRY_PAUSE_MS),
    );
    edge::require_bsc(rpc.chain_id()?)?;
    let head = rpc.block_number()?;
    let pinned = head - defaults::CONFIRMATIONS;
    let pin: Pin = edge::pin_of(pinned, &rpc.header(pinned)?)?;
    let portal = bsc_mainnet_v5_27_0().portal;
    let topic = B256::from(PortalEventKind::TokenCreated.topic0());
    // Newest launches first.
    let mut tokens: Vec<Address> = Vec::new();
    let mut to = pinned;
    while to > pinned.saturating_sub(scan) && tokens.len() < max_tokens * 4 {
        let from = to.saturating_sub(defaults::HEADER_BATCH - 1);
        let headers = rpc.headers(from, to)?;
        for h in headers.iter().rev() {
            if !bloom_has(h, portal, topic) {
                continue;
            }
            for r in rpc.block_receipts(h)? {
                for l in r.logs.iter().filter(|l| l.address == portal) {
                    if l.topics.first() == Some(&topic) && l.data.len() >= 4 * 32 {
                        tokens.push(Address::from_slice(&l.data[3 * 32 + 12..4 * 32]));
                    }
                }
            }
        }
        to = from.saturating_sub(1);
    }
    println!(
        "pin {} {} ({} launches found)",
        pin.number,
        pin.hash,
        tokens.len()
    );
    let (mut quoted, mut matched, mut mismatched) = (0usize, 0usize, 0usize);
    for token in tokens {
        if quoted >= max_tokens {
            break;
        }
        let CallOutcome::Returned(raw) =
            rpc.call_at(portal, &getTokenV8SafeCall { token }.abi_encode(), &pin)?
        else {
            continue;
        };
        let s = getTokenV8SafeCall::abi_decode_returns(&raw).map_err(|e| e.to_string())?;
        let c = curve20();
        if s.status != TRADABLE
            || s.quoteTokenAddress != Address::ZERO
            || (s.r, s.h, s.k) != (c.r, c.h, c.k)
        {
            continue;
        }
        quoted += 1;
        let tax = |v: U256| u16::try_from(v).map_err(|_| format!("{token}: tax {v}"));
        let (buy_tax, sell_tax) = (tax(s.buyTaxRate)?, tax(s.sellTaxRate)?);
        println!(
            "token {token} reserve {} supply {} buyTax {buy_tax} sellTax {sell_tax} version {}",
            s.reserve, s.circulatingSupply, s.tokenVersion
        );
        let quote =
            |input: Address, output: Address, amount: U256| -> Result<Option<U256>, String> {
                let call = quoteExactInputCall {
                    params: QuoteExactInputParams {
                        inputToken: input,
                        outputToken: output,
                        inputAmount: amount,
                    },
                };
                Ok(match rpc.call_at(portal, &call.abi_encode(), &pin)? {
                    CallOutcome::Returned(r) => Some(
                        quoteExactInputCall::abi_decode_returns(&r).map_err(|e| e.to_string())?,
                    ),
                    CallOutcome::Reverted(_) => None,
                })
            };
        for wei in BUY_WEI {
            let amount = U256::from(wei);
            let chain = quote(Address::ZERO, token, amount)?;
            let local = quote_native_buy(&NativeBuyInputs {
                curve_id: CURVE_ID_RH_BNB,
                reserve_before: s.reserve,
                circulating_before: s.circulatingSupply,
                input_amount: amount,
                protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
                buy_tax_bps: buy_tax,
                tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
                caller_fee_exempt: false,
            })
            .map(|q| q.output_amount);
            let ok = chain == local.ok();
            if ok {
                matched += 1
            } else {
                mismatched += 1
            }
            println!(
                "  BUY  {amount} -> chain {chain:?} local {local:?} {}",
                if ok { "MATCH" } else { "MISMATCH" }
            );
            lines.push(sample(
                &pin, token, &s, buy_tax, sell_tax, "BUY", amount, chain,
            ));
        }
        for ppm in SELL_PPM {
            let amount = s.circulatingSupply * U256::from(ppm) / U256::from(1_000_000u64);
            if amount.is_zero() {
                continue;
            }
            let chain = quote(token, Address::ZERO, amount)?;
            let local = quote_native_sell(&NativeSellInputs {
                curve_id: CURVE_ID_RH_BNB,
                reserve_before: s.reserve,
                circulating_before: s.circulatingSupply,
                input_amount: amount,
                protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
                sell_tax_bps: sell_tax,
                tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
                caller_fee_exempt: false,
            })
            .map(|q| q.output_amount);
            let ok = chain == local.ok();
            if ok {
                matched += 1
            } else {
                mismatched += 1
            }
            println!(
                "  SELL {amount} -> chain {chain:?} local {local:?} {}",
                if ok { "MATCH" } else { "MISMATCH" }
            );
            lines.push(sample(
                &pin, token, &s, buy_tax, sell_tax, "SELL", amount, chain,
            ));
        }
    }
    println!("tokens {quoted}, samples matched {matched}, mismatched {mismatched}");
    if let Some(path) = fixture_out {
        let text = format!(
            "# Portal.quoteExactInput (eth_call, Portal 0x{}, v5.27.0) at the pinned block, with getTokenV8Safe at the same pin\n\
             # (curve-20 native-quote tokens on the bonding curve; bsc-dataseed, read-only; g02_flap_parity).\n\
             # <block> <block-hash> <token> <reserve-wei> <circulating> <buy-tax-bps> <sell-tax-bps> <BUY|SELL> <input> <output|REVERT>\n{}\n",
            alloy_primitives::hex::encode(portal),
            lines.join("\n")
        );
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if mismatched > 0 {
        return Err(format!("{mismatched} samples differ"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn sample(
    pin: &Pin,
    token: Address,
    s: &TokenStateV8Safe,
    buy_tax: u16,
    sell_tax: u16,
    side: &str,
    amount: U256,
    chain: Option<U256>,
) -> String {
    format!(
        "{} {} {} {} {} {buy_tax} {sell_tax} {side} {amount} {}",
        pin.number,
        alloy_primitives::hex::encode(pin.hash),
        alloy_primitives::hex::encode(token),
        s.reserve,
        s.circulatingSupply,
        chain.map_or("REVERT".to_string(), |v| v.to_string())
    )
}

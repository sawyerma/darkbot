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
//!
//! The per-origin holding cap (`maxBuyPerOrigin`, `buyQuotaOf` of the
//! call's origin, `nftGate`) is read at the same pin, and each token's
//! launch (selector, a cap or NFT gate set in the launch receipt) from its
//! `TokenCreated` receipt: a BUY above the origin's allowance fills only up
//! to it, which the local quote refuses (`QuoteError::HoldingCapExceeded`).
#![forbid(unsafe_code)]

#[path = "g01_common/mod.rs"]
mod common;

use alloy_primitives::{Address, B256, U256};
use alloy_sol_types::{SolCall, SolEvent, sol};
use common::chain::bloom_has;
use common::edge::{self, CallOutcome, Endpoint, Pin};
use common::util::{bounded, rpc_from_config};
use darkbot_bsc_execution::rpc_wire::Limits;
use darkbot_bsc_qualification::c1_common::Args;
use darkbot_bsc_trading::economics::flap::QuoteError;
use darkbot_bsc_trading::economics::flap::curve::Curve;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    HoldingCap, NativeBuyInputs, NativeSellInputs, PricedToken, TokenTerms, Unpriced,
    WITNESSED_MAINNET_TERMS, curve20, quote_native_buy, quote_native_sell,
};
use darkbot_bsc_trading::venues::flap::abi::TOKEN_VERSION_TAXED_V3;
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
    function maxBuyPerOrigin(address token) external view returns (uint16 bps, uint256 maxBuyAmount);
    function buyQuotaOf(address token, address origin) external view returns (uint256 bought, uint256 remaining);
    function nftGate(address token) external view returns (address nft);
    event FlapTokenMaxBuyPerOriginSet(address indexed token, uint16 bps, uint256 maxBuyAmount);
    event FlapNFTGated(address token, address nft);
}

/// `TokenStatus.Tradable`: on the bonding curve.
const TRADABLE: u8 = 1;

/// Every tunable bound and its default (each overridable by its flag).
mod defaults {
    pub use crate::common::util::net::*;
    /// `--scan-blocks` (1..=100000): newest blocks searched for `TokenCreated`.
    pub const SCAN_BLOCKS: u64 = 3_000;
    /// `--max-tokens` (1..=200): tokens quoted.
    pub const MAX_TOKENS: usize = 12;
}

/// The evidence grid (not a tunable of any decision): BUY inputs (wei) and
/// SELL sizes (parts per million of the circulating supply) from dust to
/// half the supply.
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
    args.only(&["rpc", "scan-blocks", "max-tokens", "tokens", "fixture-out"])?;
    let fixture_out = args.opt("fixture-out")?.map(std::path::PathBuf::from);
    let mut lines: Vec<String> = Vec::new();
    let rpc_url = match args.opt("rpc")? {
        Some(u) => u.to_string(),
        None => rpc_from_config(Path::new(defaults::RPC_CONFIG))?,
    };
    let scan: u64 = bounded(&args, "scan-blocks", defaults::SCAN_BLOCKS, 1, 100_000)?;
    let max_tokens: usize = bounded(&args, "max-tokens", defaults::MAX_TOKENS, 1, 200)?;
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
    let scanned = rpc.block_number()? - defaults::CONFIRMATIONS;
    let portal = bsc_mainnet_v5_27_0().portal;
    let topic = B256::from(PortalEventKind::TokenCreated.topic0());
    // Tokens named by `--tokens` first (their launch not scanned), then the
    // newest launches, each with what its launch receipt set.
    let mut tokens: Vec<(Address, Launch)> = Vec::new();
    for t in args.opt("tokens")?.into_iter().flat_map(|v| v.split(',')) {
        let token: Address = t.trim().parse().map_err(|e| format!("--tokens {t}: {e}"))?;
        tokens.push((
            token,
            Launch {
                tx: None,
                cap_bps: None,
                nft: None,
            },
        ));
    }
    let mut to = scanned;
    while to > scanned.saturating_sub(scan) && tokens.len() < max_tokens * 4 {
        let from = to.saturating_sub(defaults::HEADER_BATCH - 1);
        let headers = rpc.headers(from, to)?;
        for h in headers.iter().rev() {
            if !bloom_has(h, portal, topic) {
                continue;
            }
            for r in rpc.block_receipts(h)? {
                for l in r.logs.iter().filter(|l| l.address == portal) {
                    if l.topics.first() == Some(&topic) && l.data.len() >= 4 * 32 {
                        let token = Address::from_slice(&l.data[3 * 32 + 12..4 * 32]);
                        tokens.push((token, Launch::of(&r, portal, token)));
                    }
                }
            }
        }
        to = from.saturating_sub(1);
    }
    println!("scanned to {scanned} ({} launches found)", tokens.len());
    let (mut quoted, mut matched, mut mismatched, mut unsupported) = (0usize, 0, 0, 0);
    let mut skipped: Vec<String> = Vec::new();
    for (token, launch) in tokens {
        if quoted >= max_tokens {
            break;
        }
        // Each token at its own fresh pin: a public node keeps the state
        // of recent blocks only.
        let pinned = rpc.block_number()? - defaults::CONFIRMATIONS;
        let pin: Pin = edge::pin_of(pinned, &rpc.header(pinned)?)?;
        let CallOutcome::Returned(raw) =
            rpc.call_at(portal, &getTokenV8SafeCall { token }.abi_encode(), &pin)?
        else {
            continue;
        };
        let s = getTokenV8SafeCall::abi_decode_returns(&raw).map_err(|e| e.to_string())?;
        let view = |input: Vec<u8>| -> Result<Vec<u8>, String> {
            match rpc.call_at(portal, &input, &pin)? {
                CallOutcome::Returned(r) => Ok(r),
                CallOutcome::Reverted(_) => Err(format!("{token}: lens call reverted")),
            }
        };
        let nft = nftGateCall { token }.abi_encode();
        let nft = nftGateCall::abi_decode_returns(&view(nft)?).map_err(|e| e.to_string())?;
        let tax = |v: U256| u16::try_from(v).map_err(|_| format!("{token}: tax {v}"));
        let terms = TokenTerms {
            tradable: s.status == TRADABLE,
            version: s.tokenVersion,
            quote_token: s.quoteTokenAddress.into_array(),
            nft_gate: nft.into_array(),
            curve: Curve {
                r: s.r,
                h: s.h,
                k: s.k,
            },
            dex_supply_threshold: s.dexSupplyThresh,
            buy_tax_bps: tax(s.buyTaxRate)?,
            sell_tax_bps: tax(s.sellTaxRate)?,
        };
        // The production scope decides (`PricedToken::new`). A token off the
        // native curve 20 or off the curve has no Portal quote to compare:
        // recorded with its reason, never dropped silently. One on it but
        // outside the scope (another version, an NFT gate) is still sampled
        // against the version-6 rule, as evidence of the difference.
        let priced = PricedToken::new(&terms);
        let comparable = PricedToken::new(&TokenTerms {
            version: TOKEN_VERSION_TAXED_V3,
            nft_gate: [0; 20],
            ..terms
        });
        let local_token = match (priced, comparable) {
            (Ok(t), _) | (Err(Unpriced::Version(_) | Unpriced::NftGate), Ok(t)) => t,
            (Err(why), _) => {
                let line = format!(
                    "# skipped {token} at {}: {why:?} (status {} version {} quote {} curve20 {})",
                    pin.number,
                    s.status,
                    s.tokenVersion,
                    s.quoteTokenAddress,
                    terms.curve == curve20()
                );
                println!("{line}");
                skipped.push(line);
                continue;
            }
        };
        quoted += 1;
        let cap = maxBuyPerOriginCall { token }.abi_encode();
        let cap =
            maxBuyPerOriginCall::abi_decode_returns(&view(cap)?).map_err(|e| e.to_string())?;
        let quota = buyQuotaOfCall {
            token,
            origin: Address::ZERO,
        }
        .abi_encode();
        let quota = buyQuotaOfCall::abi_decode_returns(&view(quota)?).map_err(|e| e.to_string())?;
        let holding_cap = if cap.bps == 0 {
            HoldingCap::None
        } else {
            HoldingCap::Remaining(quota.remaining)
        };
        // The launch call: `portal:<selector>` when sent to the Portal,
        // `via:<selector>` when another contract launched through it,
        // `unscanned` for a token named by `--tokens`.
        let (selector, launch_cap, launch_nft) = match launch.tx {
            Some(hash) => {
                let tx = rpc.transaction(hash)?;
                (
                    format!(
                        "{}:{}",
                        if tx.to == Some(portal) {
                            "portal"
                        } else {
                            "via"
                        },
                        tx.input
                            .get(..4)
                            .map_or_else(|| "NONE".to_string(), alloy_primitives::hex::encode)
                    ),
                    launch.cap_bps.map_or("NONE".to_string(), |b| b.to_string()),
                    launch
                        .nft
                        .map_or("NONE".to_string(), alloy_primitives::hex::encode),
                )
            }
            None => (
                "unscanned".to_string(),
                "UNKNOWN".to_string(),
                "UNKNOWN".to_string(),
            ),
        };
        let scope = match priced {
            Ok(_) => "SUPPORTED",
            Err(Unpriced::Version(_)) => "UNSUPPORTED_VERSION",
            Err(Unpriced::NftGate) => "UNSUPPORTED_NFT_GATE",
            Err(_) => "UNSUPPORTED",
        };
        let token_facts = format!(
            "{} {} {} {selector} {launch_cap} {launch_nft} {} {} {scope}",
            cap.bps,
            quota.remaining,
            alloy_primitives::hex::encode(nft),
            s.tokenVersion,
            s.dexSupplyThresh,
        );
        let (buy_tax, sell_tax) = (terms.buy_tax_bps, terms.sell_tax_bps);
        println!(
            "token {token} pin {} reserve {} supply {} buyTax {buy_tax} sellTax {sell_tax} version {} dexSupplyThresh {} cap {token_facts}",
            pin.number, s.reserve, s.circulatingSupply, s.tokenVersion, s.dexSupplyThresh
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
                token: local_token,
                reserve_before: s.reserve,
                circulating_before: s.circulatingSupply,
                input_amount: amount,
                protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
                tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
                caller_fee_exempt: false,
                holding_cap,
            })
            .map(|q| q.output_amount);
            // A BUY above the allowance fills only up to it: the local quote
            // refuses it, the Portal answers the allowance.
            let ok = match (&local, holding_cap) {
                (Err(QuoteError::HoldingCapExceeded), HoldingCap::Remaining(r)) => chain == Some(r),
                _ => chain == local.ok(),
            };
            let verdict = count(scope, ok, &mut matched, &mut mismatched, &mut unsupported);
            println!("  BUY  {amount} -> chain {chain:?} local {local:?} {verdict}");
            lines.push(format!(
                "{} {token_facts}",
                sample(&pin, token, &s, buy_tax, sell_tax, "BUY", amount, chain)
            ));
        }
        for ppm in SELL_PPM {
            let amount = s.circulatingSupply * U256::from(ppm) / U256::from(1_000_000u64);
            if amount.is_zero() {
                continue;
            }
            let chain = quote(token, Address::ZERO, amount)?;
            let local = quote_native_sell(&NativeSellInputs {
                token: local_token,
                reserve_before: s.reserve,
                circulating_before: s.circulatingSupply,
                input_amount: amount,
                protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_sell_fee_bps,
                tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
                caller_fee_exempt: false,
            })
            .map(|q| q.output_amount);
            let ok = chain == local.ok();
            let verdict = count(scope, ok, &mut matched, &mut mismatched, &mut unsupported);
            println!("  SELL {amount} -> chain {chain:?} local {local:?} {verdict}");
            lines.push(format!(
                "{} {token_facts}",
                sample(&pin, token, &s, buy_tax, sell_tax, "SELL", amount, chain)
            ));
        }
    }
    println!(
        "tokens {quoted}, samples matched {matched}, mismatched {mismatched}, outside the supported scope {unsupported}, launches skipped {}",
        skipped.len()
    );
    if let Some(path) = fixture_out {
        let text = format!(
            "# Portal.quoteExactInput (eth_call, Portal 0x{}, v5.27.0) at a pinned block per token, with getTokenV8Safe at the same pin\n\
             # (curve-20 native-quote tokens on the bonding curve; bsc-dataseed, read-only; g02_flap_parity).\n\
             # maxBuyPerOrigin, buyQuotaOf(origin 0x0) and nftGate at the same pin; the launch selector and the cap / NFT gate set in the launch receipt;\n\
             # the token version and whether the production quote claims the token (SUPPORTED: version 6 without NFT gate).\n\
             # the scope is the production one (PricedToken::new); a sample outside it is quoted locally as if version 6 without a gate.\n\
             # <block> <block-hash> <token> <reserve-wei> <circulating> <buy-tax-bps> <sell-tax-bps> <BUY|SELL> <input> <output|REVERT> <cap-bps> <origin-allowance> <nft-gate> <launch-selector> <launch-cap-bps|NONE|UNKNOWN> <launch-nft|NONE|UNKNOWN> <token-version> <dex-supply-threshold> <SUPPORTED|UNSUPPORTED_VERSION|UNSUPPORTED_NFT_GATE|UNSUPPORTED>\n{}{}{}\n",
            alloy_primitives::hex::encode(portal),
            skipped.join("\n"),
            if skipped.is_empty() { "" } else { "\n" },
            lines.join("\n")
        );
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if mismatched > 0 {
        return Err(format!("{mismatched} samples differ"));
    }
    Ok(())
}

/// Counts one sample: a mismatch only inside the supported scope.
fn count(
    scope: &str,
    ok: bool,
    matched: &mut usize,
    mismatched: &mut usize,
    unsupported: &mut usize,
) -> &'static str {
    match (scope, ok) {
        ("SUPPORTED", true) => {
            *matched += 1;
            "MATCH"
        }
        ("SUPPORTED", false) => {
            *mismatched += 1;
            "MISMATCH"
        }
        _ => {
            *unsupported += 1;
            "UNSUPPORTED"
        }
    }
}

/// What a token's launch receipt set besides the launch itself (`tx` is
/// `None` for a token named by `--tokens`).
struct Launch {
    tx: Option<B256>,
    cap_bps: Option<u16>,
    nft: Option<Address>,
}

impl Launch {
    fn of(receipt: &common::edge::Receipt, portal: Address, token: Address) -> Self {
        let mut launch = Launch {
            tx: Some(receipt.transaction_hash),
            cap_bps: None,
            nft: None,
        };
        for l in receipt.logs.iter().filter(|l| l.address == portal) {
            match l.topics.first() {
                Some(t)
                    if *t == FlapTokenMaxBuyPerOriginSet::SIGNATURE_HASH
                        && l.topics.get(1) == Some(&token.into_word())
                        && l.data.len() >= 32 =>
                {
                    launch.cap_bps = Some(u16::from_be_bytes([l.data[30], l.data[31]]));
                }
                Some(t)
                    if *t == FlapNFTGated::SIGNATURE_HASH
                        && l.data.len() >= 64
                        && l.data[12..32] == token[..] =>
                {
                    launch.nft = Some(Address::from_slice(&l.data[44..64]));
                }
                _ => {}
            }
        }
        launch
    }
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

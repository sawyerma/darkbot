//! Phase G (Economics gate §6 level 3, §14): the production Flap quote
//! against executed curve trades, read-only.
//!
//! In the newest blocks, every `TokenBought`/`TokenSold` of the Portal
//! whose token has no earlier Portal trade in the same block has a
//! reconstructable pre-state: the token's state at the parent block
//! (`getTokenV8Safe`, the holding cap and allowance of the transaction's
//! origin, the NFT gate, at that pin). The local BUY (input = the event's
//! native amount) must give the event's token amount and protocol fee; the
//! local SELL (input = the event's token amount) the event's native amount
//! and protocol fee. A later trade of a token in the same block is counted,
//! not compared (its state is not reconstructable from public data).
//! The scope is the production one (`PricedToken::new`); a trade outside it
//! (another version, an NFT gate, a fee-exempt trader) is recorded, not
//! compared, and one off the native curve 20 is recorded with its reason.
//! `--windows` repeats the scan over several state windows of the node,
//! `--window-pause-ms` apart, into one fixture (each trade once).
//! Nothing is signed or sent.
#![forbid(unsafe_code)]

#[path = "g01_common/mod.rs"]
mod common;

use alloy_primitives::{Address, B256, U256};
use alloy_sol_types::{SolCall, sol};
use common::chain::bloom_has;
use common::edge::{self, CallOutcome, Endpoint, Pin};
use common::util::{bounded, rpc_from_config};
use darkbot_bsc_execution::rpc_wire::Limits;
use darkbot_bsc_qualification::c1_common::Args;
use darkbot_bsc_trading::economics::flap::curve::Curve;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    HoldingCap, NativeBuyInputs, NativeSellInputs, PricedToken, TokenTerms, Unpriced,
    WITNESSED_MAINNET_TERMS, curve20, quote_native_buy, quote_native_sell,
};
use darkbot_bsc_trading::venues::flap::abi::TOKEN_VERSION_TAXED_V3;
use darkbot_bsc_trading::venues::flap::deployment::bsc_mainnet_v5_27_0;
use darkbot_bsc_trading::venues::flap::events::{
    CurveTrade, PortalEvent, PortalEventKind, PortalLog, decode_portal_log,
};
use std::collections::HashSet;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

sol! {
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
}

/// `TokenStatus.Tradable`: on the bonding curve.
const TRADABLE: u8 = 1;

/// Every tunable bound and its default (each overridable by its flag).
mod defaults {
    pub use crate::common::util::net::*;
    /// `--blocks` (1..=1000): the newest blocks searched per window (a
    /// public node keeps the state of recent blocks only).
    pub const BLOCKS: u64 = 80;
    /// `--max-trades` (1..=10000): trades compared.
    pub const MAX_TRADES: usize = 200;
    /// `--windows` (1..=50) and `--window-pause-ms` (0..=600000).
    pub const WINDOWS: u64 = 1;
    pub const WINDOW_PAUSE_MS: u64 = 30_000;
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("g02_flap_executed: {e}");
            ExitCode::from(2)
        }
    }
}

/// The trades of one run, over all its windows.
#[derive(Default)]
struct Tally {
    lines: Vec<String>,
    skipped: Vec<String>,
    seen: HashSet<(B256, Address, &'static str)>,
    windows: Vec<String>,
    matched: usize,
    mismatched: usize,
    unsupported: usize,
    later: usize,
}

fn run() -> Result<(), String> {
    let args = Args::parse(std::env::args().skip(1));
    args.only(&[
        "rpc",
        "blocks",
        "max-trades",
        "windows",
        "window-pause-ms",
        "fixture-out",
    ])?;
    let fixture_out = args.opt("fixture-out")?.map(std::path::PathBuf::from);
    let rpc_url = match args.opt("rpc")? {
        Some(u) => u.to_string(),
        None => rpc_from_config(Path::new(defaults::RPC_CONFIG))?,
    };
    let blocks: u64 = bounded(&args, "blocks", defaults::BLOCKS, 1, 1_000)?;
    let max_trades: usize = bounded(&args, "max-trades", defaults::MAX_TRADES, 1, 10_000)?;
    let windows: u64 = bounded(&args, "windows", defaults::WINDOWS, 1, 50)?;
    let pause: u64 = bounded(
        &args,
        "window-pause-ms",
        defaults::WINDOW_PAUSE_MS,
        0,
        600_000,
    )?;
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
    let portal = bsc_mainnet_v5_27_0().portal;
    let mut t = Tally::default();
    for w in 0..windows {
        if w > 0 {
            std::thread::sleep(Duration::from_millis(pause));
        }
        window(&rpc, portal, blocks, max_trades, &mut t)?;
        if t.matched + t.mismatched + t.unsupported >= max_trades {
            break;
        }
    }
    println!(
        "windows {}: trades matched {}, mismatched {}, outside the supported scope {}, skipped {}, later in their block (not reconstructable) {}",
        t.windows.join(", "),
        t.matched,
        t.mismatched,
        t.unsupported,
        t.skipped.len(),
        t.later
    );
    if let Some(path) = fixture_out {
        let text = format!(
            "# Executed Flap curve trades (Portal 0x{}, v5.27.0; the node's state windows {}) against the local quote at the parent block's state\n\
             # (the token's first Portal trade in its block; getTokenV8Safe, maxBuyPerOrigin, buyQuotaOf(tx origin), nftGate at the parent; bsc-dataseed, read-only; g02_flap_executed).\n\
             # BUY: input = event native amount, expect event token amount and fee; SELL: input = event token amount, expect event native amount and fee.\n\
             # The scope is the production one (PricedToken::new); outside it a trade is recorded, not compared.\n\
             # <block> <block-hash> <tx-hash> <token> <BUY|SELL> <pre-reserve> <pre-circulating> <buy-tax-bps> <sell-tax-bps> <event-amount> <event-eth> <event-fee> <cap-bps> <origin-allowance> <nft-gate> <token-version> <dex-supply-threshold> <SUPPORTED|UNSUPPORTED_*> <MATCH|MISMATCH|NOT_COMPARED>\n{}{}{}\n",
            alloy_primitives::hex::encode(portal),
            t.windows.join(", "),
            t.skipped.join("\n"),
            if t.skipped.is_empty() { "" } else { "\n" },
            t.lines.join("\n")
        );
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if t.mismatched > 0 {
        return Err(format!("{} trades differ", t.mismatched));
    }
    Ok(())
}

/// One state window: the newest `blocks` blocks, newest first, until the
/// node's state ends (a pruned parent) or `max_trades` are compared.
fn window(
    rpc: &Endpoint,
    portal: Address,
    blocks: u64,
    max_trades: usize,
    t: &mut Tally,
) -> Result<(), String> {
    let topics = [
        B256::from(PortalEventKind::TokenBought.topic0()),
        B256::from(PortalEventKind::TokenSold.topic0()),
    ];
    let head = rpc.block_number()? - defaults::CONFIRMATIONS;
    let from = head.saturating_sub(blocks.saturating_sub(1));
    // The oldest block whose parent state the node still served.
    let mut oldest = head;
    let mut batch_to = head;
    'blocks: loop {
        let batch_from = batch_to
            .saturating_sub(defaults::HEADER_BATCH - 2)
            .max(from);
        // The batch with its oldest block's parent: at most HEADER_BATCH.
        let headers = rpc.headers(batch_from.saturating_sub(1), batch_to)?;
        // Newest first: the oldest state first leaves the node's window.
        for pair in headers.windows(2).rev() {
            let (parent, h) = (&pair[0], &pair[1]);
            if !topics.iter().any(|x| bloom_has(h, portal, *x)) {
                continue;
            }
            let pin: Pin = edge::pin_of(parent.number, parent)?;
            let mut traded: HashSet<Address> = HashSet::new();
            for r in rpc.block_receipts(h)? {
                for l in r.logs.iter().filter(|l| l.address == portal) {
                    let topics: Vec<[u8; 32]> = l.topics.iter().map(|x| x.0).collect();
                    let (side, trade) = match decode_portal_log(portal, l.address, &topics, &l.data)
                    {
                        PortalLog::Event(PortalEvent::TokenBought(x)) => ("BUY", x),
                        PortalLog::Event(PortalEvent::TokenSold(x)) => ("SELL", x),
                        _ => continue,
                    };
                    if !traded.insert(trade.token) {
                        t.later += 1;
                        continue;
                    }
                    if !t.seen.insert((r.transaction_hash, trade.token, side)) {
                        continue;
                    }
                    if t.matched + t.mismatched + t.unsupported >= max_trades {
                        break 'blocks;
                    }
                    // The node's state window ends at the first pruned parent:
                    // older blocks are not compared, never guessed.
                    let line = match compare(rpc, portal, &pin, h, &r, side, &trade) {
                        Ok(line) => line,
                        Err(e) if e.contains("missing trie node") => break 'blocks,
                        Err(e) => return Err(e),
                    };
                    oldest = h.number;
                    println!("{line}");
                    if line.starts_with('#') {
                        t.skipped.push(line);
                        continue;
                    }
                    match line.split_whitespace().last() {
                        Some("MATCH") => t.matched += 1,
                        Some("MISMATCH") => t.mismatched += 1,
                        _ => t.unsupported += 1,
                    }
                    t.lines.push(line);
                }
            }
        }
        if batch_from <= from {
            break;
        }
        batch_to = batch_from - 1;
    }
    t.windows.push(format!("{oldest}..={head}"));
    Ok(())
}

/// One trade against the local quote at `pin`: its fixture line, or a `#
/// skipped` line for a token off the native curve 20 at the parent block.
fn compare(
    rpc: &Endpoint,
    portal: Address,
    pin: &Pin,
    block: &edge::Header,
    receipt: &edge::Receipt,
    side: &str,
    trade: &CurveTrade,
) -> Result<String, String> {
    let token = trade.token;
    let view = |input: Vec<u8>| -> Result<Option<Vec<u8>>, String> {
        Ok(match rpc.call_at(portal, &input, pin)? {
            CallOutcome::Returned(r) => Some(r),
            CallOutcome::Reverted(_) => None,
        })
    };
    let skip = |why: String| {
        format!(
            "# skipped {} {} {side} {token}: {why}",
            block.number,
            alloy_primitives::hex::encode(receipt.transaction_hash)
        )
    };
    let Some(raw) = view(getTokenV8SafeCall { token }.abi_encode())? else {
        return Ok(skip("getTokenV8Safe reverted".into()));
    };
    let s = getTokenV8SafeCall::abi_decode_returns(&raw).map_err(|e| e.to_string())?;
    let decode_err = |e: alloy_sol_types::Error| e.to_string();
    let nft = view(nftGateCall { token }.abi_encode())?
        .ok_or_else(|| format!("{token}: nftGate reverted"))?;
    let nft = nftGateCall::abi_decode_returns(&nft).map_err(decode_err)?;
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
    let priced = PricedToken::new(&terms);
    // Outside the scope but on the native curve 20: quoted as if version 6
    // without a gate, recorded and never compared.
    let comparable = PricedToken::new(&TokenTerms {
        version: TOKEN_VERSION_TAXED_V3,
        nft_gate: [0; 20],
        ..terms
    });
    let local_token = match (priced, comparable) {
        (Ok(p), _) | (Err(Unpriced::Version(_) | Unpriced::NftGate), Ok(p)) => p,
        (Err(why), _) => {
            return Ok(skip(format!(
                "{why:?} (status {} version {} quote {} curve20 {})",
                s.status,
                s.tokenVersion,
                s.quoteTokenAddress,
                terms.curve == curve20()
            )));
        }
    };
    let cap = view(maxBuyPerOriginCall { token }.abi_encode())?
        .ok_or_else(|| format!("{token}: maxBuyPerOrigin reverted"))?;
    let cap = maxBuyPerOriginCall::abi_decode_returns(&cap).map_err(decode_err)?;
    let quota = view(
        buyQuotaOfCall {
            token,
            origin: receipt.from,
        }
        .abi_encode(),
    )?
    .ok_or_else(|| format!("{token}: buyQuotaOf reverted"))?;
    let quota = buyQuotaOfCall::abi_decode_returns(&quota).map_err(decode_err)?;
    // A fee-exempt trader pays no protocol fee: an unwitnessed branch.
    let scope = match priced {
        Ok(_) if trade.fee.is_zero() => "UNSUPPORTED_FEE_EXEMPT",
        Ok(_) => "SUPPORTED",
        Err(Unpriced::Version(_)) => "UNSUPPORTED_VERSION",
        Err(Unpriced::NftGate) => "UNSUPPORTED_NFT_GATE",
        Err(_) => "UNSUPPORTED",
    };
    let ok = match side {
        "BUY" => quote_native_buy(&NativeBuyInputs {
            token: local_token,
            reserve_before: s.reserve,
            circulating_before: s.circulatingSupply,
            input_amount: trade.eth,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: if cap.bps == 0 {
                HoldingCap::None
            } else {
                HoldingCap::Remaining(quota.remaining)
            },
        })
        .is_ok_and(|q| q.output_amount == trade.amount && q.protocol_fee == trade.fee),
        _ => quote_native_sell(&NativeSellInputs {
            token: local_token,
            reserve_before: s.reserve,
            circulating_before: s.circulatingSupply,
            input_amount: trade.amount,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_sell_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
        })
        .is_ok_and(|q| q.output_amount == trade.eth && q.protocol_fee == trade.fee),
    };
    let verdict = match (scope, ok) {
        ("SUPPORTED", true) => "MATCH",
        ("SUPPORTED", false) => "MISMATCH",
        _ => "NOT_COMPARED",
    };
    Ok(format!(
        "{} {} {} {} {side} {} {} {} {} {} {} {} {} {} {} {} {} {scope} {verdict}",
        block.number,
        alloy_primitives::hex::encode(block.hash),
        alloy_primitives::hex::encode(receipt.transaction_hash),
        alloy_primitives::hex::encode(token),
        s.reserve,
        s.circulatingSupply,
        terms.buy_tax_bps,
        terms.sell_tax_bps,
        trade.amount,
        trade.eth,
        trade.fee,
        cap.bps,
        quota.remaining,
        alloy_primitives::hex::encode(nft),
        s.tokenVersion,
        s.dexSupplyThresh,
    ))
}

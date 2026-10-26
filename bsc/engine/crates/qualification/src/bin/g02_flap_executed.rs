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
//! Nothing is signed or sent.
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
    CURVE_ID_RH_BNB, HoldingCap, NativeBuyInputs, NativeSellInputs, WITNESSED_MAINNET_TERMS,
    curve20, quote_native_buy, quote_native_sell,
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
    /// `--blocks`: the newest blocks searched (a public node keeps the
    /// state of recent blocks only).
    pub const BLOCKS: u64 = 60;
    /// `--max-trades`: trades compared.
    pub const MAX_TRADES: usize = 200;
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

fn run() -> Result<(), String> {
    let args = Args::parse(std::env::args().skip(1));
    args.only(&["rpc", "blocks", "max-trades", "fixture-out"])?;
    let fixture_out = args.opt("fixture-out")?.map(std::path::PathBuf::from);
    let rpc_url = match args.opt("rpc")? {
        Some(u) => u.to_string(),
        None => rpc_from_config(Path::new(defaults::RPC_CONFIG))?,
    };
    let blocks: u64 = number(&args, "blocks", defaults::BLOCKS)?;
    let max_trades: usize = number(&args, "max-trades", defaults::MAX_TRADES)?;
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
    let topics = [
        B256::from(PortalEventKind::TokenBought.topic0()),
        B256::from(PortalEventKind::TokenSold.topic0()),
    ];
    let head = rpc.block_number()? - defaults::CONFIRMATIONS;
    let from = head.saturating_sub(blocks.saturating_sub(1));
    let (mut lines, mut matched, mut mismatched, mut unsupported, mut later) =
        (Vec::new(), 0usize, 0usize, 0usize, 0usize);
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
            if !topics.iter().any(|t| bloom_has(h, portal, *t)) {
                continue;
            }
            let pin: Pin = edge::pin_of(parent.number, parent)?;
            let mut traded: HashSet<Address> = HashSet::new();
            for r in rpc.block_receipts(h)? {
                for l in r.logs.iter().filter(|l| l.address == portal) {
                    let t: Vec<[u8; 32]> = l.topics.iter().map(|t| t.0).collect();
                    let (side, trade) = match decode_portal_log(portal, l.address, &t, &l.data) {
                        PortalLog::Event(PortalEvent::TokenBought(x)) => ("BUY", x),
                        PortalLog::Event(PortalEvent::TokenSold(x)) => ("SELL", x),
                        _ => continue,
                    };
                    if !traded.insert(trade.token) {
                        later += 1;
                        continue;
                    }
                    if matched + mismatched + unsupported >= max_trades {
                        break 'blocks;
                    }
                    // The node's state window ends at the first pruned parent:
                    // older blocks are not compared, never guessed.
                    let line = match compare(&rpc, portal, &pin, h, &r, side, &trade) {
                        Ok(Some(line)) => line,
                        Ok(None) => continue,
                        Err(e) if e.contains("missing trie node") => break 'blocks,
                        Err(e) => return Err(e),
                    };
                    oldest = h.number;
                    match line.split_whitespace().last() {
                        Some("MATCH") => matched += 1,
                        Some("MISMATCH") => mismatched += 1,
                        _ => unsupported += 1,
                    }
                    println!("{line}");
                    lines.push(line);
                }
            }
        }
        if batch_from <= from {
            break;
        }
        batch_to = batch_from - 1;
    }
    println!(
        "blocks {oldest}..={head} (state window; asked from {from}): trades matched {matched}, mismatched {mismatched}, outside the supported scope {unsupported}, later in their block (not reconstructable) {later}"
    );
    if let Some(path) = fixture_out {
        let text = format!(
            "# Executed Flap curve trades (Portal 0x{}, v5.27.0, blocks {oldest}..={head}: the node's state window) against the local quote at the parent block's state\n\
             # (the token's first Portal trade in its block; getTokenV8Safe, maxBuyPerOrigin, buyQuotaOf(tx origin), nftGate at the parent; bsc-dataseed, read-only; g02_flap_executed).\n\
             # BUY: input = event native amount, expect event token amount and fee; SELL: input = event token amount, expect event native amount and fee.\n\
             # <block> <block-hash> <tx-hash> <token> <BUY|SELL> <pre-reserve> <pre-circulating> <buy-tax-bps> <sell-tax-bps> <event-amount> <event-eth> <event-fee> <cap-bps> <origin-allowance> <nft-gate> <token-version> <dex-supply-threshold> <SUPPORTED|UNSUPPORTED_*> <MATCH|MISMATCH|NOT_COMPARED>\n{}\n",
            alloy_primitives::hex::encode(portal),
            lines.join("\n")
        );
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if mismatched > 0 {
        return Err(format!("{mismatched} trades differ"));
    }
    Ok(())
}

/// One trade against the local quote at `pin`; `None` for a token that is
/// not on the curve-20 native curve at the parent block.
fn compare(
    rpc: &Endpoint,
    portal: Address,
    pin: &Pin,
    block: &edge::Header,
    receipt: &edge::Receipt,
    side: &str,
    trade: &CurveTrade,
) -> Result<Option<String>, String> {
    let token = trade.token;
    let view = |input: Vec<u8>| -> Result<Option<Vec<u8>>, String> {
        Ok(match rpc.call_at(portal, &input, pin)? {
            CallOutcome::Returned(r) => Some(r),
            CallOutcome::Reverted(_) => None,
        })
    };
    let Some(raw) = view(getTokenV8SafeCall { token }.abi_encode())? else {
        return Ok(None);
    };
    let s = getTokenV8SafeCall::abi_decode_returns(&raw).map_err(|e| e.to_string())?;
    let c = curve20();
    if s.status != TRADABLE
        || s.quoteTokenAddress != Address::ZERO
        || (s.r, s.h, s.k) != (c.r, c.h, c.k)
    {
        return Ok(None);
    }
    let decode_err = |e: alloy_sol_types::Error| e.to_string();
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
    let nft = view(nftGateCall { token }.abi_encode())?
        .ok_or_else(|| format!("{token}: nftGate reverted"))?;
    let nft = nftGateCall::abi_decode_returns(&nft).map_err(decode_err)?;
    let tax = |v: U256| u16::try_from(v).map_err(|_| format!("{token}: tax {v}"));
    let (buy_tax, sell_tax) = (tax(s.buyTaxRate)?, tax(s.sellTaxRate)?);
    // What the production quote does not claim: another token version, an
    // NFT-gated market, a fee-exempt trader (no protocol fee charged).
    let scope = if s.tokenVersion != TOKEN_VERSION_TAXED_V3 {
        "UNSUPPORTED_VERSION"
    } else if nft != Address::ZERO {
        "UNSUPPORTED_NFT_GATE"
    } else if trade.fee.is_zero() {
        "UNSUPPORTED_FEE_EXEMPT"
    } else {
        "SUPPORTED"
    };
    let ok = match side {
        "BUY" => quote_native_buy(&NativeBuyInputs {
            curve_id: CURVE_ID_RH_BNB,
            reserve_before: s.reserve,
            circulating_before: s.circulatingSupply,
            input_amount: trade.eth,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            buy_tax_bps: buy_tax,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: if cap.bps == 0 {
                HoldingCap::None
            } else {
                HoldingCap::Remaining(quota.remaining)
            },
            dex_supply_threshold: s.dexSupplyThresh,
        })
        .is_ok_and(|q| q.output_amount == trade.amount && q.protocol_fee == trade.fee),
        _ => quote_native_sell(&NativeSellInputs {
            curve_id: CURVE_ID_RH_BNB,
            reserve_before: s.reserve,
            circulating_before: s.circulatingSupply,
            input_amount: trade.amount,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            sell_tax_bps: sell_tax,
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
    Ok(Some(format!(
        "{} {} {} {} {side} {} {} {buy_tax} {sell_tax} {} {} {} {} {} {} {} {} {scope} {verdict}",
        block.number,
        alloy_primitives::hex::encode(block.hash),
        alloy_primitives::hex::encode(receipt.transaction_hash),
        alloy_primitives::hex::encode(token),
        s.reserve,
        s.circulatingSupply,
        trade.amount,
        trade.eth,
        trade.fee,
        cap.bps,
        quota.remaining,
        alloy_primitives::hex::encode(nft),
        s.tokenVersion,
        s.dexSupplyThresh,
    )))
}

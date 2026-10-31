//! Phase G (Economics gate §9, gate finding M1): a real sample of the gas
//! the Flap paths use, read-only, as frozen evidence of the counterfactual
//! cost (the largest observed gas used of each class).
//!
//! From the newest blocks backwards (or from `--before-block`, an older
//! window of the implementation's era: the largest gas of a class depends
//! on the window, BSC3-079), every included transaction sent
//! directly to the Portal with `swapExactInput` is classed by its receipt:
//! a `TokenBought` is a BUY (one that also graduated the token,
//! `LaunchedToDEX`, is BUY_MIGRATING, the refused migration path; one of a
//! token launched earlier in the same block is BUY_IN_LAUNCH_BLOCK, the C3
//! path's shape, its own class, BSC3-079), a
//! `TokenSold` a SELL; a reverted one by the
//! direction it attempted (input token native: REVERTED_BUY, else
//! REVERTED_SELL). Every successful `approve(Portal, …)` of a token the
//! Portal knows (`getTokenV8Safe` status not 0) is an APPROVE. Up to
//! `--per-class` receipts per class, each with its gas limit, gas used,
//! effective gas price and type, in the receipts fixture's line format.
//! The sample is of the production deployment's Portal implementation
//! (`bsc_mainnet_v5_27_2()`, BSC3-065/020): its EIP-1967 slot is checked at
//! the window's newest block and at its oldest sampled one (`--archive`, an
//! archive node: a public node keeps recent state only), and the scan stops at the
//! newest block whose receipts hold an `Upgraded(address)` of the Portal
//! (older blocks ran another implementation). Nothing is signed or sent.
#![forbid(unsafe_code)]

#[path = "g01_common/mod.rs"]
mod common;

use alloy_primitives::{Address, B256};
use alloy_sol_types::{SolCall, sol};
use common::bytecode::ImplementationCheck;
use common::edge::{self, CallOutcome, Endpoint, Pin};
use common::util::{bounded, rpc_from_config};
use darkbot_bsc_execution::rpc_wire::Limits;
use darkbot_bsc_qualification::c1_common::Args;
use darkbot_bsc_trading::venues::flap::abi::SWAP_EXACT_INPUT_SELECTOR;
use darkbot_bsc_trading::venues::flap::deployment::bsc_mainnet_v5_27_2;
use darkbot_bsc_trading::venues::flap::events::{
    PortalEvent, PortalEventKind, PortalLog, decode_portal_log,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

sol! {
    struct TokenStatusOnly {
        uint8 status;
    }
    function getTokenV8Safe(address token) external view returns (TokenStatusOnly state);
}

/// `approve(address,uint256)`.
const APPROVE: [u8; 4] = [0x09, 0x5e, 0xa7, 0xb3];

/// Every tunable bound and its default (each overridable by its flag).
mod defaults {
    pub use crate::common::util::net::*;
    /// `--blocks` (1..=20000): the newest blocks searched.
    pub const BLOCKS: u64 = 2_000;
    /// `--per-class` (1..=1000): receipts kept per class.
    pub const PER_CLASS: usize = 60;
    /// `--block-batch` (1..=100): full blocks per request.
    pub const BLOCK_BATCH: u64 = 20;
    /// `--min-rare` (0..=100): receipts each revert class needs before the
    /// scan may stop (0: the rare classes never hold it open).
    pub const MIN_RARE: usize = 1;
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("g02_gas_evidence: {e}");
            ExitCode::from(2)
        }
    }
}

/// A BUY of `token`: in its launch block (a token `launched` earlier in
/// the block) or any other.
fn buy_class(token: Option<Address>, launched: &BTreeSet<Address>) -> &'static str {
    if token.is_some_and(|t| launched.contains(&t)) {
        "BUY_IN_LAUNCH_BLOCK"
    } else {
        "BUY"
    }
}

/// What a Portal log says of a token, as the production decoder reads it.
enum Decoded {
    Created(Address),
    Bought(Address),
}

fn portal_event(portal: Address, l: &edge::Log) -> Option<Decoded> {
    let topics: Vec<[u8; 32]> = l.topics.iter().map(|t| t.0).collect();
    match decode_portal_log(portal, l.address, &topics, &l.data) {
        PortalLog::Event(PortalEvent::TokenCreated { token, .. }) => Some(Decoded::Created(token)),
        PortalLog::Event(PortalEvent::TokenBought(t)) => Some(Decoded::Bought(t.token)),
        _ => None,
    }
}

/// BSC3-079: for each receipt of a block, in order, the tokens the Portal
/// created in the block's earlier successful receipts (a receipt's own
/// creation is not earlier than itself).
fn launched_before(portal: Address, receipts: &[edge::Receipt]) -> Vec<BTreeSet<Address>> {
    let mut launched = BTreeSet::new();
    receipts
        .iter()
        .map(|r| {
            let before = launched.clone();
            if r.status == 1 {
                launched.extend(r.logs.iter().filter_map(|l| match portal_event(portal, l) {
                    Some(Decoded::Created(t)) => Some(t),
                    _ => None,
                }));
            }
            before
        })
        .collect()
}

fn word_address(data: &[u8], word: usize) -> Option<Address> {
    let w = data.get(word * 32..word * 32 + 32)?;
    w[..12]
        .iter()
        .all(|b| *b == 0)
        .then(|| Address::from_slice(&w[12..]))
}

fn run() -> Result<(), String> {
    let args = Args::parse(std::env::args().skip(1));
    args.only(&[
        "rpc",
        "archive",
        "blocks",
        "per-class",
        "block-batch",
        "min-rare",
        "fixture-out",
        "before-block",
    ])?;
    let fixture_out = args.opt("fixture-out")?.map(std::path::PathBuf::from);
    let rpc_url = match args.opt("rpc")? {
        Some(u) => u.to_string(),
        None => rpc_from_config(Path::new(defaults::RPC_CONFIG))?,
    };
    let blocks: u64 = bounded(&args, "blocks", defaults::BLOCKS, 1, 20_000)?;
    let per_class: usize = bounded(&args, "per-class", defaults::PER_CLASS, 1, 1_000)?;
    let batch: u64 = bounded(&args, "block-batch", defaults::BLOCK_BATCH, 1, 100)?;
    let min_rare: usize = bounded(&args, "min-rare", defaults::MIN_RARE, 0, 100)?;
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
    let archive = Endpoint::new(
        args.one("archive")?,
        Limits {
            connect_timeout_s: defaults::CONNECT_TIMEOUT_S,
            max_time_s: defaults::MAX_TIME_S,
            compressed: true,
        },
        defaults::ATTEMPTS,
        Duration::from_millis(defaults::RETRY_PAUSE_MS),
    );
    edge::require_bsc(archive.chain_id()?)?;
    let deployment = bsc_mainnet_v5_27_2();
    let portal = deployment.portal;
    let mut bound = ImplementationCheck::new(
        portal,
        deployment.portal_implementation,
        deployment.portal_implementation_runtime_sha256,
    );
    let upgraded = alloy_primitives::keccak256("Upgraded(address)");
    let bought = B256::from(PortalEventKind::TokenBought.topic0());
    let sold = B256::from(PortalEventKind::TokenSold.topic0());
    let migrated = B256::from(PortalEventKind::LaunchedToDex.topic0());
    // BSC3-079: an older window of the implementation's era is scanned
    // back from `--before-block` (exclusive); the implementation is read
    // at its newest and oldest block on the archive either way.
    let newest = rpc.block_number()? - defaults::CONFIRMATIONS;
    let head = match args.opt("before-block")? {
        None => newest,
        Some(b) => {
            let b: u64 = b.parse().map_err(|e| format!("--before-block: {e}"))?;
            if b == 0 || b > newest {
                return Err(format!("--before-block {b} is not at or below {newest}"));
            }
            b - 1
        }
    };
    let head_pin = edge::pin_of(head, &archive.header(head)?)?;
    if rpc.header(head)?.hash != head_pin.hash {
        return Err(format!(
            "block {head}: the endpoints answer different hashes"
        ));
    }
    bound.check(&archive, &head_pin)?;
    let mut upgrade_at: Option<u64> = None;
    let classes = [
        "BUY",
        "BUY_IN_LAUNCH_BLOCK",
        "SELL",
        "APPROVE",
        "REVERTED_BUY",
        "REVERTED_SELL",
        "BUY_MIGRATING",
    ];
    let mut found: BTreeMap<&str, Vec<String>> = classes.iter().map(|c| (*c, Vec::new())).collect();
    let mut flap_token: BTreeMap<Address, bool> = BTreeMap::new();
    // The reverts hold the scan open until `--min-rare` of each (a class
    // without evidence fails closed); migrations never do.
    let full = |m: &BTreeMap<&str, Vec<String>>| {
        ["BUY", "SELL", "APPROVE"]
            .iter()
            .all(|c| m.get(c).is_some_and(|v| v.len() >= per_class))
            && ["REVERTED_BUY", "REVERTED_SELL", "BUY_IN_LAUNCH_BLOCK"]
                .iter()
                .all(|c| m.get(c).is_some_and(|v| v.len() >= min_rare.min(per_class)))
    };
    let mut to = head;
    let floor = head.saturating_sub(blocks);
    while to > floor && !full(&found) && upgrade_at.is_none() {
        let from = to.saturating_sub(batch - 1).max(floor + 1);
        for b in rpc.full_blocks(from, to)?.iter().rev() {
            let header = rpc.header(b.number)?;
            if header.hash != b.hash {
                return Err(format!("block {}: header and body differ", b.number));
            }
            let receipts = rpc.block_receipts(&header)?;
            // An upgrade in this block: it and every older block ran (in
            // part) another implementation; none of them is sampled.
            if receipts.iter().any(|r| {
                r.logs
                    .iter()
                    .any(|l| l.address == portal && l.topics.first() == Some(&upgraded))
            }) {
                upgrade_at = Some(b.number);
                break;
            }
            // BSC3-079: the tokens launched earlier in this block (a BUY of
            // one is the C3 path's shape: the first trades of a token).
            let launched = launched_before(portal, &receipts);
            for ((tx, r), launched) in b.transactions.iter().zip(&receipts).zip(&launched) {
                if tx.hash != r.transaction_hash {
                    return Err(format!("block {}: receipts out of order", b.number));
                }
                let selector: Option<[u8; 4]> = tx.input.get(..4).and_then(|s| s.try_into().ok());
                let class = match (tx.to, selector) {
                    (Some(to), Some(s)) if to == portal && s == SWAP_EXACT_INPUT_SELECTOR => {
                        if r.status == 1 {
                            let has = |t: B256| {
                                r.logs
                                    .iter()
                                    .any(|l| l.address == portal && l.topics.first() == Some(&t))
                            };
                            match (has(bought), has(sold), has(migrated)) {
                                // A BUY that graduated the token is the
                                // migration path, refused by the quote.
                                (true, false, true) => "BUY_MIGRATING",
                                (true, false, false) => {
                                    let token =
                                        r.logs.iter().find_map(|l| match portal_event(portal, l) {
                                            Some(Decoded::Bought(t)) => Some(t),
                                            _ => None,
                                        });
                                    buy_class(token, launched)
                                }
                                (false, true, _) => "SELL",
                                _ => continue,
                            }
                        } else {
                            // swapExactInput((address input, …)): the tuple
                            // starts after its offset word.
                            match word_address(&tx.input[4..], 1) {
                                Some(a) if a == Address::ZERO => "REVERTED_BUY",
                                Some(_) => "REVERTED_SELL",
                                None => continue,
                            }
                        }
                    }
                    (Some(token), Some(s)) if s == APPROVE && r.status == 1 => {
                        if word_address(&tx.input[4..], 0) != Some(portal) {
                            continue;
                        }
                        let known = match flap_token.get(&token) {
                            Some(k) => *k,
                            None => {
                                // A fresh pin each time: a public node keeps recent state only.
                                let n = rpc.block_number()? - defaults::CONFIRMATIONS;
                                let latest: Pin = edge::pin_of(n, &rpc.header(n)?)?;
                                let k = match rpc.call_at(
                                    portal,
                                    &getTokenV8SafeCall { token }.abi_encode(),
                                    &latest,
                                )? {
                                    CallOutcome::Returned(raw) => {
                                        getTokenV8SafeCall::abi_decode_returns(&raw)
                                            .is_ok_and(|s| s.status != 0)
                                    }
                                    CallOutcome::Reverted(_) => false,
                                };
                                flap_token.insert(token, k);
                                k
                            }
                        };
                        if !known {
                            continue;
                        }
                        "APPROVE"
                    }
                    _ => continue,
                };
                let list = found.get_mut(class).ok_or("class")?;
                if list.len() >= per_class {
                    continue;
                }
                let (Some(gas_used), Some(price), Some(limit)) =
                    (r.gas_used, r.effective_gas_price, tx.gas)
                else {
                    return Err(format!("{}: gas facts missing", tx.hash));
                };
                list.push(format!(
                    "FLAP {class} {} {} {} {} {limit} {gas_used} {price} {} {} {}",
                    b.number,
                    alloy_primitives::hex::encode(b.hash),
                    alloy_primitives::hex::encode(tx.hash),
                    r.status,
                    tx.tx_type.unwrap_or(0),
                    alloy_primitives::hex::encode(tx.to.unwrap_or(Address::ZERO)),
                    alloy_primitives::hex::encode(selector.unwrap_or([0; 4])),
                ));
            }
        }
        to = upgrade_at.unwrap_or(from.saturating_sub(1));
    }
    // The oldest sampled block ran the pinned implementation too.
    let oldest = to.saturating_add(1);
    let oldest_pin = edge::pin_of(oldest, &archive.header(oldest)?)?;
    if rpc.header(oldest)?.hash != oldest_pin.hash {
        return Err(format!(
            "block {oldest}: the endpoints answer different hashes"
        ));
    }
    bound.check(&archive, &oldest_pin)?;
    let mut lines = Vec::new();
    for c in classes {
        let v = &found[c];
        let max = v
            .iter()
            .filter_map(|l| l.split_whitespace().nth(7)?.parse::<u64>().ok())
            .max();
        println!("FLAP {c}: {} receipts, largest gas used {max:?}", v.len());
        lines.extend(v.iter().cloned());
    }
    if let Some(path) = fixture_out {
        let counts: Vec<String> = classes
            .iter()
            .map(|c| format!("{c} {}", found[c].len()))
            .collect();
        let text = format!(
            "# Receipts of Portal-direct Flap transactions (eth_getBlockByNumber + eth_getBlockReceipts, bsc-dataseed, read-only; g02_gas_evidence),\n\
             # blocks {}..={head}, newest first, up to {per_class} per class: {}.\n\
             # Portal 0x{}, {}, {} (blocks {head} and {oldest}); no Upgraded(address) of the Portal in these blocks{}.\n\
             # swapExactInput to the Portal classed by its TokenBought/TokenSold, a reverted one by its input token; approve(Portal) of a token the Portal knows.\n\
             # <venue> <class> <block> <block-hash> <tx-hash> <status> <gas-limit> <gas-used> <effective-gas-price-wei> <tx-type> <to> <selector>\n{}\n",
            oldest,
            counts.join(", "),
            alloy_primitives::hex::encode(portal),
            deployment.portal_version,
            bound.line(),
            upgrade_at.map_or_else(String::new, |n| format!(
                " (the scan stopped at the upgrade in block {n})"
            )),
            lines.join("\n")
        );
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(a: Address) -> Vec<u8> {
        let mut w = vec![0u8; 12];
        w.extend_from_slice(a.as_slice());
        w
    }

    /// BSC3-079: a BUY of a token launched earlier in its block is its own
    /// class, read from the Portal's own logs by the production decoder.
    #[test]
    fn a_buy_in_its_tokens_launch_block_is_its_own_class() {
        let portal = bsc_mainnet_v5_27_2().portal;
        let token = Address::repeat_byte(0x77);
        let launched: BTreeSet<Address> = [token].into_iter().collect();
        assert_eq!(buy_class(Some(token), &launched), "BUY_IN_LAUNCH_BLOCK");
        assert_eq!(
            buy_class(Some(Address::repeat_byte(0x78)), &launched),
            "BUY"
        );
        assert_eq!(buy_class(None, &launched), "BUY");
        // TokenBought(ts, token, buyer, amount, eth, fee, postPrice).
        let mut data = vec![0u8; 32];
        data.extend(word(token));
        data.extend(word(Address::repeat_byte(0xb0)));
        data.extend(vec![0u8; 4 * 32]);
        let log = |address, data: Vec<u8>| edge::Log {
            log_index: 0,
            address,
            topics: vec![B256::from(PortalEventKind::TokenBought.topic0())],
            data,
        };
        assert!(matches!(
            portal_event(portal, &log(portal, data.clone())),
            Some(Decoded::Bought(t)) if t == token
        ));
        // Another contract's log of the same shape is no Portal event.
        assert!(portal_event(portal, &log(Address::repeat_byte(1), data)).is_none());
    }

    sol! {
        event TokenCreated(uint256 ts, address creator, uint256 nonce, address token, string name, string symbol, string meta);
    }

    /// BSC3-079 review: a token counts as launched for the receipts after
    /// the successful one whose Portal log created it — not for its own,
    /// not from a failed one or another contract's log of the same shape.
    #[test]
    fn a_token_is_launched_for_the_later_receipts_of_its_block() {
        use alloy_sol_types::SolEvent;
        let portal = bsc_mainnet_v5_27_2().portal;
        let created = |address, token| edge::Log {
            log_index: 0,
            address,
            topics: vec![B256::from(PortalEventKind::TokenCreated.topic0())],
            data: TokenCreated {
                ts: Default::default(),
                creator: Address::repeat_byte(0xc0),
                nonce: Default::default(),
                token,
                name: "N".into(),
                symbol: "S".into(),
                meta: "M".into(),
            }
            .encode_data(),
        };
        let receipt = |status, logs| edge::Receipt {
            block_number: 1,
            block_hash: B256::ZERO,
            transaction_index: 0,
            transaction_hash: B256::ZERO,
            from: Address::ZERO,
            to: Some(portal),
            contract_address: None,
            status,
            logs,
            gas_used: None,
            effective_gas_price: None,
        };
        let [a, b, c, d] = [0x71, 0x72, 0x73, 0x74].map(Address::repeat_byte);
        assert!(matches!(
            portal_event(portal, &created(portal, a)),
            Some(Decoded::Created(t)) if t == a
        ));
        let receipts = [
            receipt(1, vec![created(portal, a)]),
            receipt(1, vec![created(portal, b)]),
            receipt(0, vec![created(portal, c)]),
            receipt(1, vec![created(Address::repeat_byte(1), d)]),
            receipt(1, vec![]),
        ];
        let set = |t: &[Address]| t.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(
            launched_before(portal, &receipts),
            [
                set(&[]),
                set(&[a]),
                set(&[a, b]),
                set(&[a, b]),
                set(&[a, b])
            ]
        );
        assert_eq!(
            buy_class(Some(b), &launched_before(portal, &receipts)[1]),
            "BUY"
        );
    }
}

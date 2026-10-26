//! Launch detection on real BSC Mainnet blocks, and the venue table.
//!
//! Fixtures (`tests/fixtures/chain/`): the answers of
//! `https://bsc-dataseed.bnbchain.org` to `eth_getBlockByNumber(n, false)`
//! and `eth_getBlockReceipts(n)`, fetched on 2026-10-25 and stored as sent.
//! The blocks were found by scanning recent blocks' receipts for the two
//! launch topics.
//!
//! How every expected value was established, independently of this module:
//! - launch transaction, index, token and creator words: read from the raw
//!   receipts with a separate script (Python `json`, no ABI library);
//! - token: the address in the event's token word emitted the mint
//!   `Transfer(0x0 -> ...)` in the launch receipt (re-checked below from the
//!   JSON), and `eth_getCode(token, latest)` returned the 45-byte EIP-1167
//!   proxy `363d3d373d3d3d363d73...` for every token here;
//! - creator: the event's creator word. For a direct Portal call it is the
//!   transaction's `from` (block 129774331 index 1), via a launcher contract
//!   it is the transaction's `to` (block 129781248); in block 129774331
//!   index 26 a router calls the intermediate contract 0xd3f3...35ea, the
//!   creator word, which receives the creator's buy and passes it on to
//!   the sender. For Four.meme it is the `from` (re-checked below);
//! - external buys: Portal `TokenBought` logs of the token in successful
//!   receipts after the launch index. Block 129774331 has exactly one, at
//!   index 2 (tx 0x22851309...): buyer 0xf604...dbd8, transaction sender
//!   0x11c4...c06e (neither is the creator), and the same receipt carries
//!   the token's `Transfer` from the Portal to that buyer. Each launch
//!   transaction carries the creator's own `TokenBought`, which is not
//!   counted (re-checked below);
//! - transactions: the block fixtures (`block_<n>.full.json`) are the
//!   answers to `eth_getBlockByNumber(n, true)`, fetched on 2026-10-26; the
//!   launch objects in them were compared equal to the separately fetched
//!   `eth_getTransactionByHash` answers, and every object's hash and `from`
//!   to its receipt's. Sender and nonce are the object's `from`/`nonce`;
//!   the C1 scope is true
//!   only for block 129774331 index 1, whose `to` is the Portal and whose
//!   input starts with `newTokenV6` (`0x8cb5772c`); the others call a
//!   router (0xa853...3f6b), a launcher (0x8994...6666) or TokenManager2
//!   `createToken` (`0x519ebb10`) (re-checked below from the JSON).

use alloy_primitives::{hex, keccak256};
use serde_json::Value;

use super::follow::receipts_match;
use super::jsonrpc::{BlockHead, FullBlock, Receipt, parse_block, parse_receipts};
use super::launches::{
    FLAP_PORTAL, FOUR_TOKEN_MANAGER2, LAUNCH_EVENTS, ReceiptLaunch, detect, verify_object,
};
use super::{CanonicalLaunch, Field, RequestError, Venue};
use crate::wss::RejectReason;

const FLAP_A_BLOCK: &str = include_str!("../../tests/fixtures/chain/block_129774331.full.json");
const FLAP_A_RECEIPTS: &str =
    include_str!("../../tests/fixtures/chain/block_129774331.receipts.json");
const FLAP_B_BLOCK: &str = include_str!("../../tests/fixtures/chain/block_129781248.full.json");
const FLAP_B_RECEIPTS: &str =
    include_str!("../../tests/fixtures/chain/block_129781248.receipts.json");
const FOUR_BLOCK: &str = include_str!("../../tests/fixtures/chain/block_129781119.full.json");
const FOUR_RECEIPTS: &str =
    include_str!("../../tests/fixtures/chain/block_129781119.receipts.json");

const TRANSFER: [u8; 32] = hex!("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef");

/// The block and its receipts through the follower's own parse and
/// receipt check.
fn load(block: &str, receipts: &str, number: u64) -> (BlockHead, Vec<Receipt>) {
    let block = full(block, number).head;
    let receipts = parse_receipts(receipts.as_bytes(), 1).unwrap().unwrap();
    assert!(receipts_match(&block, &receipts));
    (block, receipts)
}

fn full(block: &str, number: u64) -> FullBlock {
    parse_block(block.as_bytes(), 1, number).unwrap().unwrap()
}

/// Each launch completed from its block's own transaction object, through
/// the follower's own conversion and the WSS verifier.
fn complete(block: &str, number: u64, launches: Vec<ReceiptLaunch>) -> Vec<CanonicalLaunch> {
    let objects = full(block, number).objects;
    launches
        .into_iter()
        .map(|launch| {
            let index = usize::try_from(launch.tx_index).unwrap();
            let verified = verify_object(&objects[index]).unwrap();
            launch.with_transaction(Ok(&verified))
        })
        .collect()
}

/// The raw transaction object at `index` of a block, read independently
/// of `jsonrpc`.
fn raw_tx(block: &str, index: u64) -> Value {
    let all: Value = serde_json::from_str(block).unwrap();
    all["result"]["transactions"][usize::try_from(index).unwrap()].clone()
}

/// Independent checks of the transaction facts against the raw object.
fn check_tx(block: &str, launch: &CanonicalLaunch) {
    let raw = raw_tx(block, launch.tx_index);
    assert_eq!(Some(address_of(&raw["from"])), launch.sender);
    assert_eq!(
        Some(u64::from_str_radix(&raw["nonce"].as_str().unwrap()[2..], 16).unwrap()),
        launch.nonce
    );
    let direct_v6 = address_of(&raw["to"]) == FLAP_PORTAL
        && raw["input"].as_str().unwrap().starts_with("0x8cb5772c");
    assert_eq!(direct_v6, launch.c1_scope);
}

/// The raw receipt at `index`, read independently of `jsonrpc`.
fn raw_receipt(receipts: &str, index: usize) -> Value {
    let all: Value = serde_json::from_str(receipts).unwrap();
    all["result"][index].clone()
}

fn address_of(v: &Value) -> [u8; 20] {
    hex::decode_to_array(v.as_str().unwrap()).unwrap()
}

fn topic_address(topic: &Value) -> [u8; 20] {
    let t: [u8; 32] = hex::decode_to_array(topic.as_str().unwrap()).unwrap();
    t[12..].try_into().unwrap()
}

/// Independent checks of one launch against its raw receipt: the token
/// emitted its mint in the launch receipt, and the creator's own buy in the
/// launch transaction exists (and is therefore not an external buy).
fn check_launch_receipt(receipts: &str, launch: &CanonicalLaunch) {
    let receipt = raw_receipt(receipts, usize::try_from(launch.tx_index).unwrap());
    assert_eq!(receipt["status"], "0x1");
    let tx_hash: [u8; 32] =
        hex::decode_to_array(receipt["transactionHash"].as_str().unwrap()).unwrap();
    assert_eq!(tx_hash, launch.tx_hash);
    let token = launch.token.unwrap();
    let logs = receipt["logs"].as_array().unwrap();
    let mint = logs.iter().any(|l| {
        let topics = l["topics"].as_array().unwrap();
        address_of(&l["address"]) == token
            && topics.len() == 3
            && hex::decode_to_array::<_, 32>(topics[0].as_str().unwrap()).unwrap() == TRANSFER
            && topic_address(&topics[1]) == [0u8; 20]
    });
    assert!(mint, "no mint of the token in the launch receipt");
    if launch.venue == Venue::Flap {
        let buy_topic = LAUNCH_EVENTS[0].buy.unwrap().topic0;
        let own_buy = logs.iter().any(|l| {
            let data = hex::decode(l["data"].as_str().unwrap()).unwrap();
            address_of(&l["address"]) == FLAP_PORTAL
                && hex::decode_to_array::<_, 32>(l["topics"][0].as_str().unwrap()).unwrap()
                    == buy_topic
                && data[44..64] == token
        });
        assert!(own_buy, "the launch transaction has no creator buy");
    }
}

#[test]
fn venue_topics_are_the_keccak_of_their_signatures() {
    for event in LAUNCH_EVENTS {
        assert_eq!(
            keccak256(event.signature).0,
            event.topic0,
            "{}",
            event.signature
        );
        if let Some(buy) = event.buy {
            assert_eq!(keccak256(buy.signature).0, buy.topic0, "{}", buy.signature);
        }
    }
    // The values the order names, as written there.
    let [flap, four] = LAUNCH_EVENTS;
    assert_eq!((flap.venue, four.venue), (Venue::Flap, Venue::Four));
    assert_eq!(
        flap.contract,
        hex!("e2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0")
    );
    assert_eq!(
        flap.topic0,
        hex!("504e7f360b2e5fe33cbaaae4c593bc55305328341bf79009e43e0e3b7f699603")
    );
    assert_eq!(
        four.contract,
        hex!("5c952063c7fc8610ffdb798152d69f0b9550762b")
    );
    assert_eq!(
        four.topic0,
        hex!("396d5e902b675b032348d3d2e9517ee8f0c4a926603fbc075d3d282ff00cad20")
    );
    assert_eq!(four.contract, FOUR_TOKEN_MANAGER2);
    assert!(four.buy.is_none());
}

#[test]
fn flap_block_with_two_launches_and_one_external_buy() {
    let (block, receipts) = load(FLAP_A_BLOCK, FLAP_A_RECEIPTS, 129_774_331);
    let hash = hex!("b46720d61774811f42474df7f45d98e2967509025fe10a5f67c9eb9f381e12ab");
    assert_eq!(block.header.hash, hash);
    assert_eq!(receipts.len(), 54);
    let launches = complete(FLAP_A_BLOCK, 129_774_331, detect(&block, &receipts));
    let expected = vec![
        CanonicalLaunch {
            venue: Venue::Flap,
            block_number: 129_774_331,
            block_hash: hash,
            tx_index: 1,
            tx_hash: hex!("e01868de612be81cd8b469bf00d0bace41220e038552b3f9e4d440c417f9f7ac"),
            token: Some(hex!("bbe126a97529fc7ace5c8a2167555f41df2d7777")),
            creator: Some(hex!("0fd35ec5ecbb70db9f163764c42a23f4bfbfb464")),
            external_buys_after: Some(1),
            first_external_buy_index: Some(2),
            sender: Some(hex!("0fd35ec5ecbb70db9f163764c42a23f4bfbfb464")),
            nonce: Some(13),
            c1_scope: true,
            unverifiable: None,
        },
        CanonicalLaunch {
            venue: Venue::Flap,
            block_number: 129_774_331,
            block_hash: hash,
            tx_index: 26,
            tx_hash: hex!("1d663bc62b1d2fdcda857eb27bb5e907b6ac20d82170a356e574fdba6618ae45"),
            token: Some(hex!("e5499b52f4f70ab9efd09880342a53f520dc7777")),
            creator: Some(hex!("d3f3fe1f286d13f95f4be5c1bfea5e1a9e6335ea")),
            external_buys_after: Some(0),
            first_external_buy_index: None,
            sender: Some(hex!("cbb8be2e4ef71b77f58b98334491e9bbd9888e34")),
            nonce: Some(1592),
            c1_scope: false,
            unverifiable: None,
        },
    ];
    assert_eq!(launches, expected);
    for launch in &launches {
        check_launch_receipt(FLAP_A_RECEIPTS, launch);
        check_tx(FLAP_A_BLOCK, launch);
    }
    // Direct Portal call: the creator is the sender.
    let first = raw_receipt(FLAP_A_RECEIPTS, 1);
    assert_eq!(address_of(&first["from"]), launches[0].creator.unwrap());
    assert_eq!(address_of(&first["to"]), FLAP_PORTAL);
    // The external buy: another sender, and the token reaches the buyer.
    let buy = raw_receipt(FLAP_A_RECEIPTS, 2);
    assert_eq!(
        buy["transactionHash"],
        "0x22851309bbb3bebf08bb6d648ac370e355d5f715edceb078b24dc26e5ca4265b"
    );
    assert_ne!(address_of(&buy["from"]), launches[0].creator.unwrap());
    let buyer = hex!("f604044f03bcac11f84a72aac138b5df3c47dbd8");
    let delivered = buy["logs"].as_array().unwrap().iter().any(|l| {
        let topics = l["topics"].as_array().unwrap();
        address_of(&l["address"]) == launches[0].token.unwrap()
            && topics.len() == 3
            && topic_address(&topics[1]) == FLAP_PORTAL
            && topic_address(&topics[2]) == buyer
    });
    assert!(delivered);
}

#[test]
fn flap_block_with_one_launch_and_no_external_buy() {
    let (block, receipts) = load(FLAP_B_BLOCK, FLAP_B_RECEIPTS, 129_781_248);
    let hash = hex!("f6690df0d75f2c36e6a9822bca1ff8fed6aba47eb176bda1ad801cffd5ff4d51");
    assert_eq!(block.header.hash, hash);
    let launches = complete(FLAP_B_BLOCK, 129_781_248, detect(&block, &receipts));
    let expected = CanonicalLaunch {
        venue: Venue::Flap,
        block_number: 129_781_248,
        block_hash: hash,
        tx_index: 32,
        tx_hash: hex!("a5d44b55fd7fd77b6c2a6b9fc1f451bd97f9b82ba17b876f2d6a7a75533d385a"),
        token: Some(hex!("9518d1c3c26e17fecb1ee2387d5bfa1d56437777")),
        creator: Some(hex!("8994ef9463667ee3bb7268758167c7e9468d6666")),
        external_buys_after: Some(0),
        first_external_buy_index: None,
        sender: Some(hex!("1b4ebe73585b94578dc055c6ac7e89333fdadcf7")),
        nonce: Some(4297),
        c1_scope: false,
        unverifiable: None,
    };
    assert_eq!(launches, vec![expected.clone()]);
    check_launch_receipt(FLAP_B_RECEIPTS, &expected);
    check_tx(FLAP_B_BLOCK, &expected);
    // Via a launcher contract: the creator is the called contract.
    let raw = raw_receipt(FLAP_B_RECEIPTS, 32);
    assert_eq!(address_of(&raw["to"]), expected.creator.unwrap());
}

#[test]
fn four_block_launch_has_token_and_creator_but_no_buy_count() {
    let (block, receipts) = load(FOUR_BLOCK, FOUR_RECEIPTS, 129_781_119);
    let hash = hex!("51d04ca917cbec94c520b6f68fea4e3093f3ee933d57bf35f13a895ff94dfbae");
    assert_eq!(block.header.hash, hash);
    let launches = complete(FOUR_BLOCK, 129_781_119, detect(&block, &receipts));
    let expected = CanonicalLaunch {
        venue: Venue::Four,
        block_number: 129_781_119,
        block_hash: hash,
        tx_index: 32,
        tx_hash: hex!("3163c268f047f99b5328f8e622348c1aeadde9445c49542ee8d081f286fa2354"),
        token: Some(hex!("173b4b11b28a2eddc757d81a2322501544b9ffff")),
        creator: Some(hex!("774352301321637bc2cad74e67d4089f36e15962")),
        external_buys_after: None,
        first_external_buy_index: None,
        sender: Some(hex!("774352301321637bc2cad74e67d4089f36e15962")),
        nonce: Some(262),
        c1_scope: false,
        unverifiable: None,
    };
    assert_eq!(launches, vec![expected.clone()]);
    check_launch_receipt(FOUR_RECEIPTS, &expected);
    check_tx(FOUR_BLOCK, &expected);
    assert!(
        raw_tx(FOUR_BLOCK, 32)["input"]
            .as_str()
            .unwrap()
            .starts_with("0x519ebb10")
    );
    let raw = raw_receipt(FOUR_RECEIPTS, 32);
    assert_eq!(address_of(&raw["from"]), expected.creator.unwrap());
    // The archived ABI's head is eight words: `name` and `symbol` start
    // right after it (offsets 0x100 and 0x140).
    let log = raw["logs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| {
            address_of(&l["address"]) == FOUR_TOKEN_MANAGER2
                && l["topics"]
                    == serde_json::json!([format!("0x{}", hex::encode(LAUNCH_EVENTS[1].topic0))])
        })
        .unwrap();
    let data = hex::decode(log["data"].as_str().unwrap()).unwrap();
    assert_eq!(data[3 * 32 + 30..4 * 32], [0x01, 0x00]);
    assert_eq!(data[4 * 32 + 30..5 * 32], [0x01, 0x40]);
}

#[test]
fn failed_receipts_and_other_contracts_are_not_launches() {
    let (block, receipts) = load(FLAP_B_BLOCK, FLAP_B_RECEIPTS, 129_781_248);
    let mut failed = receipts.clone();
    failed[32].succeeded = false;
    assert!(detect(&block, &failed).is_empty());
    let mut foreign = receipts;
    for log in &mut foreign[32].logs {
        if log.address == FLAP_PORTAL {
            log.address = FOUR_TOKEN_MANAGER2;
        }
    }
    assert!(detect(&block, &foreign).is_empty());
}

#[test]
fn malformed_words_are_unknown_never_guessed() {
    let (block, receipts) = load(FLAP_A_BLOCK, FLAP_A_RECEIPTS, 129_774_331);
    let buy_topic = LAUNCH_EVENTS[0].buy.unwrap().topic0;
    // A later buy event of the Portal with a wrong length: the count of
    // the first launch is unknown, not 0 or 1.
    let mut short_buy = receipts.clone();
    for log in &mut short_buy[2].logs {
        if log.address == FLAP_PORTAL && log.topics.first() == Some(&buy_topic) {
            log.data.truncate(6 * 32);
        }
    }
    let launches = detect(&block, &short_buy);
    assert_eq!(
        (
            launches[0].external_buys_after,
            launches[0].first_external_buy_index
        ),
        (None, None)
    );
    // A token word with a non-zero pad: the event is malformed for the
    // Flap decoder (Phase D), so token, creator and the buys are unknown.
    let launch_topic = LAUNCH_EVENTS[0].topic0;
    let mut dirty = receipts;
    for log in &mut dirty[1].logs {
        if log.address == FLAP_PORTAL && log.topics.first() == Some(&launch_topic) {
            log.data[3 * 32] = 1;
        }
    }
    let launches = detect(&block, &dirty);
    assert_eq!(launches.len(), 2);
    assert_eq!(
        (
            launches[0].token,
            launches[0].creator,
            launches[0].external_buys_after
        ),
        (None, None, None)
    );
    // The other launch of the block is untouched.
    assert!(launches[1].token.is_some() && launches[1].creator.is_some());
}

/// `timestamp_ms` of block 129774331: `timestamp` 0x6ade6eab = 1792962219 s,
/// `mixHash` 0x...c8 = 200 ms, so 1792962219 * 1000 + 200 = 1792962219200;
/// the node's own `milliTimestamp` field (0x1a174e04cc0) says the same for
/// every fixture header (read independently of `jsonrpc`).
#[test]
fn block_ms_of_the_fixture_headers() {
    let (block, _) = load(FLAP_A_BLOCK, FLAP_A_RECEIPTS, 129_774_331);
    assert_eq!(block.header.timestamp_s, 1_792_962_219);
    assert_eq!(block.header.block_ms, 1_792_962_219_200);
    for (header, number) in [
        (FLAP_A_BLOCK, 129_774_331),
        (FLAP_B_BLOCK, 129_781_248),
        (FOUR_BLOCK, 129_781_119),
    ] {
        let parsed = full(header, number).head;
        let raw: Value = serde_json::from_str(header).unwrap();
        let node_ms =
            u64::from_str_radix(&raw["result"]["milliTimestamp"].as_str().unwrap()[2..], 16)
                .unwrap();
        assert_eq!(parsed.header.block_ms, node_ms, "{number}");
    }
}

/// The launch transaction comes from the block's own objects: each bound
/// to the block and its position, its `from` cross-checked with the
/// receipt, its signed bytes verified by the WSS verifier.
#[test]
fn block_transactions_are_bound_cross_checked_and_verified() {
    let (block, receipts) = load(FLAP_A_BLOCK, FLAP_A_RECEIPTS, 129_774_331);
    // Every object's hash and sender are its receipt's (receipts_match).
    assert_eq!(block.txs.len(), 54);
    assert_eq!(
        (block.txs[1].from, block.txs[1].nonce),
        (hex!("0fd35ec5ecbb70db9f163764c42a23f4bfbfb464"), 13)
    );
    let tampered = |index: usize, edit: &dyn Fn(&mut Value)| {
        let mut answer: Value = serde_json::from_str(FLAP_A_BLOCK).unwrap();
        edit(&mut answer["result"]["transactions"][index]);
        answer.to_string()
    };
    let parse = |body: &str| parse_block(body.as_bytes(), 1, 129_774_331);
    // Not at its position, or of another block: refused.
    assert_eq!(
        parse(&tampered(1, &|tx| tx["transactionIndex"] = Value::from("0x2"))).err(),
        Some(RequestError::Shape(Field::TxBinding))
    );
    assert_eq!(
        parse(&tampered(1, &|tx| tx["blockNumber"] = Value::from("0x7bc32fc"))).err(),
        Some(RequestError::Shape(Field::TxBinding))
    );
    // A changed signed field breaks the hash.
    let renonced = parse(&tampered(1, &|tx| tx["nonce"] = Value::from("0xe")))
        .unwrap()
        .unwrap();
    assert!(receipts_match(&renonced.head, &receipts));
    assert_eq!(
        verify_object(&renonced.objects[1]).err(),
        Some(RejectReason::HashMismatch)
    );
    // A changed sender breaks the receipt cross-check and the signer.
    let resent = parse(&tampered(1, &|tx| {
        tx["from"] = Value::from("0x1111111111111111111111111111111111111111");
    }))
    .unwrap()
    .unwrap();
    assert!(!receipts_match(&resent.head, &receipts));
    assert_eq!(
        verify_object(&resent.objects[1]).err(),
        Some(RejectReason::SignerMismatch)
    );
    // A type the verifier does not rebuild is known at conversion.
    let retyped = parse(&tampered(26, &|tx| tx["type"] = Value::from("0x4")))
        .unwrap()
        .unwrap();
    assert_eq!(
        verify_object(&retyped.objects[26]).err(),
        Some(RejectReason::UnsupportedType(4))
    );
    assert!(verify_object(&retyped.objects[1]).is_ok());
}

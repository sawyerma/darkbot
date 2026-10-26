//! Phase D contract §11: the production decoder against the G0.1 Flap
//! refreeze (block 129849511) and against malformed input.

use alloy_primitives::{Address, U256, keccak256};

use super::abi::{AbiError, TOKEN_VERSION_TAXED_V3};
use super::deployment::bsc_mainnet_v5_27_0;
use super::events::{
    PortalEvent, PortalEventKind, PortalLog, ReceiptError, decode_portal_log, decode_receipt_logs,
};
use super::recognize::{PortalCall, Recognized, decode_swap_exact_input, recognize};
use crate::transactions::legacy::testdata::unhex;

/// Portal logs of the refreeze: `<block> <kind> <log index> <topic0> <data>`.
const LOGS: &str = include_str!("../../../fixtures/flap/freeze_v5_27_portal_logs.txt");
/// Portal calls of the refreeze: `<block> <kind> <selector> <input>`.
const CALLS: &str = include_str!("../../../fixtures/flap/freeze_v5_27_portal_calls.txt");
/// Pending V6 launches of the G0.4 coverage capture.
const PENDING_CREATOR_BUY: &str =
    include_str!("../../../fixtures/flap/launch_v6_native_creator_buy_pending.hex");
const PENDING_SPLIT_TAX: &str =
    include_str!("../../../fixtures/flap/launch_v6_native_split_tax_pending.hex");

const MAX: usize = 4096;

fn address(hex: &str) -> Address {
    Address::from_slice(&unhex(hex.trim_start_matches("0x")))
}

fn rows(text: &str) -> Vec<Vec<&str>> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split_whitespace().collect())
        .collect()
}

struct Log {
    block: u64,
    kind: String,
    index: u64,
    topic: [u8; 32],
    data: Vec<u8>,
}

fn logs() -> Vec<Log> {
    rows(LOGS)
        .into_iter()
        .map(|r| Log {
            block: r[0].parse().unwrap(),
            kind: r[1].to_owned(),
            index: r[2].parse().unwrap(),
            topic: unhex(r[3]).try_into().unwrap(),
            data: unhex(r[4]),
        })
        .collect()
}

fn portal() -> Address {
    bsc_mainnet_v5_27_0().portal
}

#[test]
fn every_topic_and_selector_is_keccak_of_its_verified_signature() {
    for kind in PortalEventKind::ALL {
        assert_eq!(
            kind.topic0(),
            keccak256(kind.signature().as_bytes()).0,
            "{kind:?}"
        );
        assert_eq!(PortalEventKind::from_topic0(&kind.topic0()), Some(kind));
    }
    for call in PortalCall::ALL {
        assert_eq!(
            call.selector(),
            keccak256(call.signature().as_bytes())[..4],
            "{call:?}"
        );
        assert_eq!(PortalCall::from_selector(call.selector()), Some(call));
    }
}

#[test]
fn every_frozen_portal_log_decodes_as_its_event() {
    let all = logs();
    assert_eq!(all.len(), 39);
    for log in &all {
        let topics = [log.topic];
        match decode_portal_log(portal(), portal(), &topics, &log.data) {
            PortalLog::Event(event) => {
                assert_eq!(
                    event.kind().topic0(),
                    log.topic,
                    "{} {}",
                    log.block,
                    log.index
                )
            }
            other => panic!("{} {}: {other:?}", log.block, log.index),
        }
    }
}

#[test]
fn launch_buy_and_sell_events_decode_field_exact() {
    let all = logs();
    let find = |block: u64, index: u64| {
        all.iter()
            .find(|l| l.block == block && l.index == index)
            .unwrap()
    };
    let decode = |log: &'_ Log| match decode_portal_log(portal(), portal(), &[log.topic], &log.data)
    {
        PortalLog::Event(e) => format!("{e:?}"),
        other => panic!("{other:?}"),
    };
    let created = find(129849039, 283);
    let PortalLog::Event(PortalEvent::TokenCreated {
        ts,
        creator,
        nonce,
        token,
        name,
        symbol,
        meta,
    }) = decode_portal_log(portal(), portal(), &[created.topic], &created.data)
    else {
        panic!("{}", decode(created));
    };
    assert_eq!(ts, U256::from(1_792_995_847u64));
    assert_eq!(creator, address("c11657b25392fd9908a625c4f267b00f3ab5b8a7"));
    assert_eq!(nonce, U256::from(3_613_684u64));
    assert_eq!(token, address("978d5c39bd7a6a2d813a065ab48be6a51b3f7777"));
    assert_eq!(name, "BNB狗".as_bytes());
    assert_eq!(symbol, "AI狗".as_bytes());
    assert_eq!(
        meta,
        b"bafkreiaapuao7dux3sd7xd233u5a2xqhml3ib3pkduoi43j5zbn7dx263e".as_slice()
    );
    let version = find(129849039, 286);
    assert!(matches!(
        decode_portal_log(portal(), portal(), &[version.topic], &version.data),
        PortalLog::Event(PortalEvent::TokenVersionSet { version, token: t })
            if version == TOKEN_VERSION_TAXED_V3 && t == token
    ));
    let bought = find(129849062, 159);
    let PortalLog::Event(PortalEvent::TokenBought(buy)) =
        decode_portal_log(portal(), portal(), &[bought.topic], &bought.data)
    else {
        panic!("{}", decode(bought));
    };
    assert_eq!(
        buy.token,
        address("68f5c1afa7039bc4bbb06c567e5adc8e6aaf7777")
    );
    assert_eq!(
        buy.trader,
        address("7828e9ef701a93edd2bdea0b05eb3bb75ecb1139")
    );
    assert_eq!(buy.amount, U256::from(525_795_340_927_075_280_392_402u128));
    assert_eq!(buy.eth, U256::from(3_023_447_329_729_851u64));
    assert_eq!(buy.fee, U256::from(30_234_473_297_298u64));
    assert_eq!(buy.post_price, U256::from(5_551_614_579u64));
    let sold = find(129849067, 79);
    let PortalLog::Event(PortalEvent::TokenSold(sell)) =
        decode_portal_log(portal(), portal(), &[sold.topic], &sold.data)
    else {
        panic!("{}", decode(sold));
    };
    assert_eq!(
        sell.token,
        address("7b1fd28895140601815ecf38de8224e397e47777")
    );
    assert_eq!(
        sell.trader,
        address("bb6f4ad554bfcaa8e292cabdd8a06fa95ec88e6c")
    );
    assert_eq!(sell.eth, U256::from(2_336_233_824_381_397_192u64));
    assert_eq!(sold.kind, "SELL");
}

#[test]
fn foreign_unknown_and_malformed_logs_fail_closed() {
    let all = logs();
    let created = all.iter().find(|l| l.index == 283).unwrap();
    let bought = all.iter().find(|l| l.index == 159).unwrap();
    // Another emitter: not a Portal event, whatever its topic.
    assert_eq!(
        decode_portal_log(
            portal(),
            Address::repeat_byte(7),
            &[created.topic],
            &created.data
        ),
        PortalLog::NotPortal
    );
    // An unknown Portal topic is counted, never guessed.
    assert!(matches!(
        decode_portal_log(portal(), portal(), &[[9; 32]], &created.data),
        PortalLog::Unrecognized(_)
    ));
    assert!(matches!(
        decode_portal_log(portal(), portal(), &[], &created.data),
        PortalLog::Unrecognized(_)
    ));
    let malformed = |topics: &[[u8; 32]], data: &[u8]| match decode_portal_log(
        portal(),
        portal(),
        topics,
        data,
    ) {
        PortalLog::Malformed(_, e) => e,
        other => panic!("{other:?}"),
    };
    // An indexed parameter, a truncated or extended static event.
    assert_eq!(
        malformed(&[bought.topic, [0; 32]], &bought.data),
        AbiError::NonCanonical
    );
    assert_eq!(
        malformed(&[bought.topic], &bought.data[..bought.data.len() - 1]),
        AbiError::Truncated
    );
    let mut longer = bought.data.clone();
    longer.extend_from_slice(&[0; 32]);
    assert_eq!(malformed(&[bought.topic], &longer), AbiError::TrailingBytes);
    // Dirty address padding.
    let mut dirty = bought.data.clone();
    dirty[32] = 1;
    assert_eq!(
        malformed(&[bought.topic], &dirty),
        AbiError::ValueOutOfRange
    );
    // A string offset that skips bytes, dirty string padding, bytes after
    // the last string, a truncated tail.
    let mut skip = created.data.clone();
    skip[4 * 32 + 30] ^= 1;
    assert_eq!(malformed(&[created.topic], &skip), AbiError::NonCanonical);
    let mut pad = created.data.clone();
    let last = pad.len() - 1;
    pad[last] = 1;
    assert_eq!(malformed(&[created.topic], &pad), AbiError::NonCanonical);
    let mut trailing = created.data.clone();
    trailing.extend_from_slice(&[0; 32]);
    assert_eq!(
        malformed(&[created.topic], &trailing),
        AbiError::TrailingBytes
    );
    assert_eq!(
        malformed(&[created.topic], &created.data[..created.data.len() - 32]),
        AbiError::Truncated
    );
}

#[test]
fn a_reverted_receipt_is_never_decoded() {
    let all = logs();
    let bought = all.iter().find(|l| l.index == 159).unwrap();
    let topics = [bought.topic];
    let one = [(portal(), &topics[..], bought.data.as_slice())];
    assert!(matches!(
        decode_receipt_logs(portal(), false, one).map(|_| ()),
        Err(ReceiptError::Reverted)
    ));
    let decoded: Vec<_> = decode_receipt_logs(portal(), true, one).unwrap().collect();
    assert!(matches!(
        decoded[..],
        [PortalLog::Event(PortalEvent::TokenBought(_))]
    ));
}

#[test]
fn frozen_portal_calls_and_pending_launches_are_recognized() {
    let calls = rows(CALLS);
    assert_eq!(calls.len(), 5);
    for row in calls {
        let input = unhex(row[3]);
        match (row[1], recognize(portal(), Some(portal()), &input, MAX)) {
            ("LAUNCH", Recognized::LaunchV6(launch)) => {
                assert_eq!(launch.token_version, TOKEN_VERSION_TAXED_V3)
            }
            ("BUY", Recognized::Swap(swap)) => assert!(swap.is_buy() && !swap.is_sell()),
            ("SELL", Recognized::Swap(swap)) => assert!(swap.is_sell() && !swap.is_buy()),
            (kind, other) => panic!("{} {kind}: {other:?}", row[0]),
        }
    }
    let buy = rows(CALLS)
        .into_iter()
        .find(|r| r[0] == "129849062")
        .map(|r| unhex(r[3]))
        .unwrap();
    let swap = decode_swap_exact_input(&buy).unwrap();
    assert_eq!(
        swap.output_token,
        address("68f5c1afa7039bc4bbb06c567e5adc8e6aaf7777")
    );
    assert_eq!(swap.input_amount, U256::from(3_023_447_329_729_851u64));
    assert_eq!(
        swap.min_output_amount,
        U256::from(515_279_434_108_533_774_784_553u128)
    );
    assert!(swap.permit.is_empty());
    for pending in [PENDING_CREATOR_BUY, PENDING_SPLIT_TAX] {
        let input = unhex(pending.trim());
        assert!(matches!(
            recognize(portal(), Some(portal()), &input, MAX),
            Recognized::LaunchV6(_)
        ));
    }
}

#[test]
fn other_destinations_unknown_and_unwitnessed_calls_fail_closed() {
    let launch = unhex(rows(CALLS)[0][3]);
    // A router or launcher, or a contract creation.
    assert_eq!(
        recognize(portal(), Some(Address::repeat_byte(0x66)), &launch, MAX),
        Recognized::NotPortal
    );
    assert_eq!(
        recognize(portal(), None, &launch, MAX),
        Recognized::NotPortal
    );
    // Too short for a selector, or a selector outside the table.
    assert_eq!(
        recognize(portal(), Some(portal()), &[0x8c, 0xb5], MAX),
        Recognized::UnknownSelector([0x8c, 0xb5, 0, 0])
    );
    assert_eq!(
        recognize(portal(), Some(portal()), &[1, 2, 3, 4, 5], MAX),
        Recognized::UnknownSelector([1, 2, 3, 4])
    );
    // Known but never witnessed: unsupported, not decoded.
    for call in PortalCall::ALL {
        if matches!(call, PortalCall::NewTokenV6 | PortalCall::SwapExactInput) {
            continue;
        }
        let mut input = call.selector().to_vec();
        input.extend_from_slice(&[0; 64]);
        assert_eq!(
            recognize(portal(), Some(portal()), &input, MAX),
            Recognized::Unsupported(call)
        );
    }
    // Oversized, truncated, extended or non-canonical decoded calls.
    assert_eq!(
        recognize(portal(), Some(portal()), &launch, launch.len() - 1),
        Recognized::Malformed(PortalCall::NewTokenV6, AbiError::Oversize)
    );
    assert!(matches!(
        recognize(portal(), Some(portal()), &launch[..launch.len() - 32], MAX),
        Recognized::Malformed(PortalCall::NewTokenV6, _)
    ));
    let buy = unhex(rows(CALLS)[2][3]);
    fn swap(input: &[u8]) -> Recognized<'_> {
        recognize(portal(), Some(portal()), input, MAX)
    }
    assert_eq!(
        swap(&buy[..buy.len() - 1]),
        Recognized::Malformed(PortalCall::SwapExactInput, AbiError::Truncated)
    );
    let mut extended = buy.clone();
    extended.extend_from_slice(&[0; 32]);
    assert_eq!(
        swap(&extended),
        Recognized::Malformed(PortalCall::SwapExactInput, AbiError::TrailingBytes)
    );
    let mut offset = buy.clone();
    offset[4 + 31] = 0x40;
    assert_eq!(
        swap(&offset),
        Recognized::Malformed(PortalCall::SwapExactInput, AbiError::NonCanonical)
    );
    // A permit with dirty padding.
    let mut permit = buy.clone();
    permit[4 + 6 * 32 + 31] = 1;
    permit.extend_from_slice(&[0xff; 32]);
    assert_eq!(
        swap(&permit),
        Recognized::Malformed(PortalCall::SwapExactInput, AbiError::NonCanonical)
    );
}

/// Recognition is deterministic and repeatable: the same bytes give the
/// same result (capture, replay, PAPER and LIVE call this one function).
#[test]
fn the_same_bytes_always_decode_the_same() {
    for row in rows(CALLS) {
        let input = unhex(row[3]);
        let first = format!("{:?}", recognize(portal(), Some(portal()), &input, MAX));
        let copy = input.clone();
        assert_eq!(
            first,
            format!("{:?}", recognize(portal(), Some(portal()), &copy, MAX))
        );
    }
}

/// An unknown token version decodes as found and is never admitted: the
/// profile refuses it (Phase D §11 "unknown version", fail closed).
#[test]
fn an_unknown_token_version_is_decoded_as_found_and_refused() {
    use super::deployment::testkit::validated_mainnet;
    use super::profile::{UnsupportedLaunch, classify_launch};
    let mut input = unhex(rows(CALLS)[0][3]);
    // Head word 25 of the launch tuple: `tokenVersion`.
    input[4 + 32 + 25 * 32 + 31] = 7;
    let Recognized::LaunchV6(launch) = recognize(portal(), Some(portal()), &input, MAX) else {
        panic!("not decoded");
    };
    assert_eq!(launch.token_version, 7);
    assert!(launch.require_taxed_v3().is_err());
    assert_eq!(
        classify_launch(&validated_mainnet(1), alloy_primitives::B256::ZERO, &launch),
        Err(UnsupportedLaunch::TokenVersion)
    );
}

/// Phase D §9: recognizer and full decode latency on this machine, per call
/// (run with `cargo test --release -p darkbot-bsc-trading flap_decoder_latency
/// -- --ignored --nocapture`). A local decoder metric, not a BUY claim.
#[test]
#[ignore = "latency measurement; run in release"]
fn flap_decoder_latency() {
    use std::time::Instant;
    let launch = unhex(rows(CALLS)[0][3]);
    let all = logs();
    let created = all.iter().find(|l| l.index == 283).unwrap();
    let report = |name: &str, mut samples: Vec<u64>| {
        samples.sort_unstable();
        let q = |p: f64| samples[((samples.len() as f64 - 1.0) * p) as usize];
        println!(
            "{name}: n={} p50={} p95={} p99={} p99.9={} p99.99={} max={} ns",
            samples.len(),
            q(0.5),
            q(0.95),
            q(0.99),
            q(0.999),
            q(0.9999),
            samples[samples.len() - 1]
        );
    };
    let portal = portal();
    const N: usize = 200_000;
    let mut recognizer = Vec::with_capacity(N);
    let mut full = Vec::with_capacity(N);
    let mut event = Vec::with_capacity(N);
    for _ in 0..N {
        let t = Instant::now();
        let hit = super::recognize::PortalCall::from_selector(launch[..4].try_into().unwrap());
        std::hint::black_box(hit);
        recognizer.push(t.elapsed().as_nanos() as u64);
        let t = Instant::now();
        let r = recognize(portal, Some(portal), std::hint::black_box(&launch), MAX);
        std::hint::black_box(&r);
        full.push(t.elapsed().as_nanos() as u64);
        let t = Instant::now();
        let e = decode_portal_log(portal, portal, &[created.topic], &created.data);
        std::hint::black_box(&e);
        event.push(t.elapsed().as_nanos() as u64);
    }
    report("selector lookup", recognizer);
    report("newTokenV6 recognize+decode", full);
    report("TokenCreated decode", event);
}

/// Phase D §6: the predicted token only where the derivation is proven;
/// the refreeze launch reproduces its canonical token.
#[test]
fn the_predicted_token_is_the_canonical_one_and_only_for_taxed_v3() {
    use super::recognize::predicted_token;
    let deployment = bsc_mainnet_v5_27_0();
    let input = unhex(rows(CALLS)[0][3]);
    let Recognized::LaunchV6(launch) = recognize(portal(), Some(portal()), &input, MAX) else {
        panic!("not decoded");
    };
    // TokenCreated of the same transaction (block 129849039, log 283).
    assert_eq!(
        predicted_token(&deployment, &launch),
        Some(address("978d5c39bd7a6a2d813a065ab48be6a51b3f7777"))
    );
    let mut other = input.clone();
    other[4 + 32 + 25 * 32 + 31] = 7;
    let Recognized::LaunchV6(other) = recognize(portal(), Some(portal()), &other, MAX) else {
        panic!("not decoded");
    };
    assert_eq!(predicted_token(&deployment, &other), None);
}

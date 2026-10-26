//! Phase E contract §11: the production Four.meme decoder against the
//! fixtures of the G0.1 Four freeze (block 129842711) and against malformed
//! input.

use alloy_primitives::{Address, B256, keccak256};

use super::abi::AbiError;
use super::deployment::{Family, FourDeployment, bsc_mainnet_g01_129842711};
use super::events::{
    FourEvent, FourEventKind, FourLog, ReceiptError, decode_four_log, decode_receipt_logs,
};
use super::recognize::{CallRole, FourCall, PendingToken, Recognized, pending_token, recognize};
use crate::transactions::legacy::testdata::unhex;
use crate::venues::flap;

/// Calls of the freeze's fixtures: `<family> <kind> <block> <block-hash>
/// <tx-hash> <status> <from> <to> <value> <input> <raw-signed-tx>`.
const CALLS: &str = include_str!("../../../fixtures/fourmeme/freeze_20261026_calls.txt");
/// Every log of their receipts: `<family> <kind> <block> <tx-hash>
/// <log-index> <emitter> <topics,…|-> <data|->`.
const LOGS: &str = include_str!("../../../fixtures/fourmeme/freeze_20261026_logs.txt");
/// Flap Portal calls of the D refreeze (shared-input check).
const FLAP_CALLS: &str = include_str!("../../../fixtures/flap/freeze_v5_27_portal_calls.txt");

const MAX: usize = 16_384;

fn rows(text: &str) -> Vec<Vec<&str>> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split_whitespace().collect())
        .collect()
}

fn address(hex: &str) -> Address {
    Address::from_slice(&unhex(hex.trim_start_matches("0x")))
}

struct Call {
    family: String,
    kind: String,
    tx: B256,
    from: Address,
    to: Address,
    input: Vec<u8>,
}

fn calls() -> Vec<Call> {
    rows(CALLS)
        .into_iter()
        .map(|r| Call {
            family: r[0].to_owned(),
            kind: r[1].to_owned(),
            tx: B256::from_slice(&unhex(r[4])),
            from: address(r[6]),
            to: address(r[7]),
            input: unhex(r[9]),
        })
        .collect()
}

/// One log as the receipt decoder takes it.
type LogRef<'a> = (Address, &'a [[u8; 32]], &'a [u8]);

struct Log {
    tx: B256,
    emitter: Address,
    topics: Vec<[u8; 32]>,
    data: Vec<u8>,
}

fn logs() -> Vec<Log> {
    rows(LOGS)
        .into_iter()
        .map(|r| Log {
            tx: B256::from_slice(&unhex(r[3])),
            emitter: address(r[5]),
            topics: if r[6] == "-" {
                Vec::new()
            } else {
                r[6].split(',')
                    .map(|t| unhex(t).try_into().unwrap())
                    .collect()
            },
            data: if r[7] == "-" { Vec::new() } else { unhex(r[7]) },
        })
        .collect()
}

fn d() -> FourDeployment {
    bsc_mainnet_g01_129842711()
}

/// The decoded Four events of one fixture transaction.
fn events_of(deployment: &FourDeployment, logs: &[Log], tx: B256) -> Vec<FourEvent<'static>> {
    logs.iter()
        .filter(|l| l.tx == tx)
        .filter_map(|l| {
            let data: &'static [u8] = Box::leak(l.data.clone().into_boxed_slice());
            match decode_four_log(deployment, l.emitter, &l.topics, data) {
                FourLog::Event(e) => Some(e),
                _ => None,
            }
        })
        .collect()
}

#[test]
fn every_selector_and_topic_is_keccak_of_its_signature() {
    for call in FourCall::ALL {
        assert_eq!(
            call.selector(),
            keccak256(call.signature().as_bytes())[..4],
            "{call:?}"
        );
        assert_eq!(
            FourCall::from_selector(call.family(), call.selector()),
            Some(call)
        );
    }
    for kind in FourEventKind::ALL {
        assert_eq!(
            kind.topic0(),
            keccak256(kind.signature().as_bytes()).0,
            "{kind:?}"
        );
        assert_eq!(
            FourEventKind::from_topic0(kind.family(), &kind.topic0()),
            Some(kind)
        );
    }
    // The launch selector is shared; each family resolves its own.
    let create = FourCall::ClassicCreateToken.selector();
    assert_eq!(
        FourCall::from_selector(Family::Classic, create),
        Some(FourCall::ClassicCreateToken)
    );
    assert_eq!(
        FourCall::from_selector(Family::OpenFour, create),
        Some(FourCall::OpenFourCreateToken)
    );
    // A Classic BUY selector is unknown to OpenFour and vice versa.
    assert_eq!(
        FourCall::from_selector(Family::OpenFour, FourCall::ClassicBuyTokenAmap.selector()),
        None
    );
    assert_eq!(
        FourCall::from_selector(Family::Classic, FourCall::OpenFourBuyByBudget.selector()),
        None
    );
}

/// Every fixture call decodes, and what it decodes equals what its own
/// receipt's event says; the pending token derivation names the token.
#[test]
fn every_fixture_call_decodes_and_agrees_with_its_event() {
    let deployment = d();
    let logs = logs();
    let calls = calls();
    assert_eq!(calls.len(), 9);
    for c in &calls {
        let events = events_of(&deployment, &logs, c.tx);
        let r = recognize(&deployment, Some(c.to), &c.input, MAX);
        match (c.family.as_str(), c.kind.as_str(), r) {
            ("CLASSIC", "LAUNCH", Recognized::ClassicLaunch(l)) => {
                let Some(FourEvent::TokenCreate(e)) = events
                    .iter()
                    .find(|e| e.kind() == FourEventKind::TokenCreate)
                    .copied()
                else {
                    panic!("no TokenCreate in {}", c.tx);
                };
                assert_eq!(l.request_id, e.request_id);
                assert_eq!(e.creator, c.from);
                assert_eq!(l.signature.len(), 65);
                assert_eq!(
                    pending_token(&deployment, &r),
                    PendingToken::Unconfirmed(e.token),
                    "{}",
                    c.tx
                );
            }
            ("OPENFOUR", "LAUNCH", Recognized::OpenFourLaunch(l)) => {
                let Some(FourEvent::TokenCreated(e)) = events
                    .iter()
                    .find(|e| e.kind() == FourEventKind::TokenCreated)
                    .copied()
                else {
                    panic!("no TokenCreated in {}", c.tx);
                };
                assert_eq!((l.request_id, l.preset_id), (e.request_id, e.preset_id));
                assert_eq!(
                    (l.init.name, l.init.symbol, l.init.token_uri),
                    (e.name, e.symbol, e.token_meta_uri)
                );
                assert_eq!(
                    (
                        l.init.max_supply,
                        l.init.sale_amount,
                        l.init.raise_amount,
                        l.init.quote_asset
                    ),
                    (e.max_supply, e.sale_amount, e.raise_amount, e.quote_asset)
                );
                assert_eq!(e.creator, c.from);
                assert_eq!(
                    pending_token(&deployment, &r),
                    PendingToken::Derived(e.token),
                    "{}",
                    c.tx
                );
                // The launch also moves its token's phase.
                assert!(events.iter().any(
                    |ev| matches!(ev, FourEvent::PhaseTransition { token, .. } if *token == e.token)
                ));
            }
            ("CLASSIC", "BUY", Recognized::ClassicBuy(b)) => {
                let Some(FourEvent::TokenPurchase(e)) = events
                    .iter()
                    .find(|e| e.kind() == FourEventKind::TokenPurchase)
                    .copied()
                else {
                    panic!("no TokenPurchase in {}", c.tx);
                };
                assert_eq!((b.token, e.account), (e.token, c.from));
                assert_eq!(pending_token(&deployment, &r), PendingToken::Unknown);
            }
            ("OPENFOUR", "BUY", Recognized::OpenFourBuy(b)) => {
                let Some(FourEvent::TradeExecutedBuy(e)) = events
                    .iter()
                    .find(|e| e.kind() == FourEventKind::TradeExecuted)
                    .copied()
                else {
                    panic!("no TradeExecuted in {}", c.tx);
                };
                assert_eq!((b.token, e.trader), (e.token, c.from));
                assert!(b.proof.is_empty());
            }
            (family, kind, other) => panic!("{family} {kind} {}: {other:?}", c.tx),
        }
    }
}

/// Every log of the fixtures' receipts: the Four contracts' witnessed
/// events decode, the one undeclared Classic topic is counted as
/// unrecognized, every other emitter is not Four.
#[test]
fn every_fixture_log_is_classified() {
    let deployment = d();
    let (mut events, mut unrecognized, mut not_four) = (Vec::new(), Vec::new(), 0);
    for l in &logs() {
        match decode_four_log(&deployment, l.emitter, &l.topics, &l.data) {
            FourLog::Event(e) => events.push(e.kind()),
            FourLog::Unrecognized(family, topic) => unrecognized.push((family, topic)),
            FourLog::NotFour => {
                assert!(deployment.family_of(l.emitter).is_none());
                not_four += 1;
            }
            other => panic!("{}: {other:?}", l.tx),
        }
    }
    let count = |k| events.iter().filter(|e| **e == k).count();
    assert_eq!(count(FourEventKind::TokenCreate), 4);
    assert_eq!(count(FourEventKind::TokenPurchase), 2);
    assert_eq!(count(FourEventKind::TokenCreated), 3);
    assert_eq!(count(FourEventKind::TradeExecuted), 1);
    assert_eq!(count(FourEventKind::PhaseTransition), 3);
    // `0x48063b12…` (one word) follows each Classic TokenPurchase; it is in
    // no published ABI and stays unrecognized.
    assert_eq!(unrecognized.len(), 2);
    assert!(
        unrecognized
            .iter()
            .all(|(f, t)| *f == Family::Classic && t[..4] == [0x48, 0x06, 0x3b, 0x12])
    );
    assert!(not_four > 100);
}

#[test]
fn a_reverted_receipt_is_never_decoded() {
    let deployment = d();
    let logs = logs();
    let list: Vec<LogRef<'_>> = logs
        .iter()
        .map(|l| (l.emitter, l.topics.as_slice(), l.data.as_slice()))
        .collect();
    assert!(matches!(
        decode_receipt_logs(&deployment, false, list.clone()),
        Err(ReceiptError::Reverted)
    ));
    let decoded = decode_receipt_logs(&deployment, true, list)
        .unwrap()
        .count();
    assert_eq!(decoded, logs.len());
}

fn fixture(family: &str, kind: &str) -> Call {
    calls()
        .into_iter()
        .find(|c| c.family == family && c.kind == kind)
        .unwrap()
}

/// Destination, selector and family: a call to anything else is not Four;
/// a selector of the other family is unknown; a known undecoded call is
/// unsupported.
#[test]
fn destination_and_selector_decide_first() {
    let deployment = d();
    let launch = fixture("CLASSIC", "LAUNCH").input;
    assert_eq!(
        recognize(&deployment, Some(Address::repeat_byte(7)), &launch, MAX),
        Recognized::NotFour
    );
    assert_eq!(
        recognize(&deployment, None, &launch, MAX),
        Recognized::NotFour
    );
    assert_eq!(
        recognize(
            &deployment,
            Some(deployment.openfour_registry),
            &launch,
            MAX
        ),
        Recognized::NotFour
    );
    let buy = fixture("CLASSIC", "BUY").input;
    assert_eq!(
        recognize(&deployment, Some(deployment.openfour_core), &buy, MAX),
        Recognized::UnknownSelector(Family::OpenFour, FourCall::ClassicBuyTokenAmap.selector())
    );
    assert_eq!(
        recognize(
            &deployment,
            Some(deployment.token_manager2),
            &[0x51, 0x9e],
            MAX
        ),
        Recognized::UnknownSelector(Family::Classic, [0x51, 0x9e, 0, 0])
    );
    // Every known call without a fixture: unsupported, whatever follows.
    for call in FourCall::ALL {
        let mut input = call.selector().to_vec();
        input.extend_from_slice(&[0u8; 96]);
        let r = recognize(
            &deployment,
            Some(deployment.contract(call.family())),
            &input,
            MAX,
        );
        match call {
            FourCall::ClassicCreateToken
            | FourCall::OpenFourCreateToken
            | FourCall::ClassicBuyTokenAmap
            | FourCall::OpenFourBuyByBudget => {
                assert!(!matches!(r, Recognized::Unsupported(_)), "{call:?}")
            }
            _ => assert_eq!(r, Recognized::Unsupported(call)),
        }
    }
    // The loop covered every SELL of both families (no witnessed SELL):
    // seven Classic `sellToken` overloads and OpenFour `sell`.
    assert_eq!(
        FourCall::ALL
            .iter()
            .filter(|c| c.role() == CallRole::Sell)
            .count(),
        8
    );
}

/// Oversized, truncated, trailing and non-canonical input fails closed.
#[test]
fn malformed_input_fails_closed() {
    let deployment = d();
    for (family, kind, call) in [
        ("CLASSIC", "LAUNCH", FourCall::ClassicCreateToken),
        ("OPENFOUR", "LAUNCH", FourCall::OpenFourCreateToken),
        ("CLASSIC", "BUY", FourCall::ClassicBuyTokenAmap),
        ("OPENFOUR", "BUY", FourCall::OpenFourBuyByBudget),
    ] {
        let c = fixture(family, kind);
        let to = Some(c.to);
        let input = &c.input;
        assert_eq!(
            recognize(&deployment, to, input, input.len() - 1),
            Recognized::Malformed(call, AbiError::Oversize),
            "{call:?}"
        );
        for cut in [5, input.len() / 2, input.len() - 1] {
            assert!(
                matches!(recognize(&deployment, to, &input[..cut], MAX), Recognized::Malformed(c2, _) if c2 == call),
                "{call:?} cut {cut}"
            );
        }
        let mut longer = input.clone();
        longer.extend_from_slice(&[0u8; 32]);
        assert!(
            matches!(recognize(&deployment, to, &longer, MAX), Recognized::Malformed(c2, AbiError::TrailingBytes) if c2 == call),
            "{call:?}"
        );
    }
    // An address word with dirty high bytes.
    let mut buy = fixture("CLASSIC", "BUY").input;
    buy[4] = 1;
    assert_eq!(
        recognize(&deployment, Some(deployment.token_manager2), &buy, MAX),
        Recognized::Malformed(FourCall::ClassicBuyTokenAmap, AbiError::ValueOutOfRange)
    );
    // createToken: a moved second offset is not canonical.
    let mut launch = fixture("CLASSIC", "LAUNCH").input;
    launch[4 + 63] += 32;
    assert!(matches!(
        recognize(&deployment, Some(deployment.token_manager2), &launch, MAX),
        Recognized::Malformed(FourCall::ClassicCreateToken, _)
    ));
}

/// Layouts the freeze did not witness are refused, never guessed: another
/// Classic `args` length, another OpenFour head.
#[test]
fn unwitnessed_layouts_are_refused() {
    let deployment = d();
    let launch = fixture("CLASSIC", "LAUNCH").input;
    let other = super::deployment::FourDeployment {
        classic_args_len: deployment.classic_args_len + 32,
        ..deployment
    };
    assert_eq!(
        recognize(&other, Some(deployment.token_manager2), &launch, MAX),
        Recognized::Malformed(FourCall::ClassicCreateToken, AbiError::UnwitnessedLayout)
    );
    // OpenFour: the TokenInitParams offset (createArgs head word 7) moved.
    let mut open = fixture("OPENFOUR", "LAUNCH").input;
    let at = 4 + 64 + 32 + 32 + 7 * 32 + 31;
    assert_eq!(open[at], 0x00);
    assert_eq!(open[at - 1], 0x01);
    open[at] = 0x20;
    assert_eq!(
        recognize(&deployment, Some(deployment.openfour_core), &open, MAX),
        Recognized::Malformed(FourCall::OpenFourCreateToken, AbiError::UnwitnessedLayout)
    );
}

/// Pre-execution token address (contract §3 critical rule): proven only
/// where the calldata selects frozen inputs; another preset is UNKNOWN; a
/// changed salt predicts another address, which the canonical event would
/// not confirm.
#[test]
fn pre_execution_token_address_is_proven_or_unknown() {
    let deployment = d();
    let c = fixture("OPENFOUR", "LAUNCH");
    let r = recognize(&deployment, Some(c.to), &c.input, MAX);
    let Recognized::OpenFourLaunch(l) = r else {
        panic!()
    };
    assert!(matches!(
        pending_token(&deployment, &r),
        PendingToken::Derived(_)
    ));
    let mut other_preset = c.input.clone();
    // createArgs head word 1 (presetId), last byte.
    other_preset[4 + 64 + 32 + 32 + 32 + 31] ^= 1;
    let r2 = recognize(&deployment, Some(c.to), &other_preset, MAX);
    let Recognized::OpenFourLaunch(l2) = r2 else {
        panic!()
    };
    assert_ne!(l2.preset_id, l.preset_id);
    assert_eq!(pending_token(&deployment, &r2), PendingToken::Unknown);
    // Classic: a candidate only; another salt is another candidate.
    let c = fixture("CLASSIC", "LAUNCH");
    let mut salted = c.input.clone();
    salted[4 + 64 + 32 + 2 * 32 + 31] ^= 1;
    let a = pending_token(
        &deployment,
        &recognize(&deployment, Some(c.to), &c.input, MAX),
    );
    let b = pending_token(
        &deployment,
        &recognize(&deployment, Some(c.to), &salted, MAX),
    );
    assert!(
        matches!((a, b), (PendingToken::Unconfirmed(x), PendingToken::Unconfirmed(y)) if x != y)
    );
    // Nothing else predicts a token.
    let buy = fixture("CLASSIC", "BUY");
    assert_eq!(
        pending_token(
            &deployment,
            &recognize(&deployment, Some(buy.to), &buy.input, MAX)
        ),
        PendingToken::Unknown
    );
}

/// Event layouts: topic count, data length, value ranges; the SELL form of
/// TradeExecuted and every unwitnessed event are unsupported.
#[test]
fn malformed_and_unwitnessed_events_fail_closed() {
    let deployment = d();
    let logs = logs();
    let find = |kind: FourEventKind| {
        logs.iter()
            .find(|l| l.topics.first() == Some(&kind.topic0()))
            .unwrap()
    };
    for kind in [
        FourEventKind::TokenCreate,
        FourEventKind::TokenPurchase,
        FourEventKind::TokenCreated,
        FourEventKind::TradeExecuted,
        FourEventKind::PhaseTransition,
    ] {
        let l = find(kind);
        let emitter = l.emitter;
        assert!(
            matches!(decode_four_log(&deployment, emitter, &l.topics, &l.data), FourLog::Event(e) if e.kind() == kind)
        );
        // One topic more or less.
        let mut more = l.topics.clone();
        more.push([0; 32]);
        assert_eq!(
            decode_four_log(&deployment, emitter, &more, &l.data),
            FourLog::Malformed(kind, AbiError::NonCanonical)
        );
        // Truncated and longer data.
        assert!(
            matches!(decode_four_log(&deployment, emitter, &l.topics, &l.data[..l.data.len() - 1]), FourLog::Malformed(k, _) if k == kind)
        );
        let mut longer = l.data.clone();
        longer.extend_from_slice(&[0; 32]);
        assert!(
            matches!(decode_four_log(&deployment, emitter, &l.topics, &longer), FourLog::Malformed(k, _) if k == kind)
        );
        // Another emitter is not Four.
        assert_eq!(
            decode_four_log(&deployment, Address::repeat_byte(9), &l.topics, &l.data),
            FourLog::NotFour
        );
    }
    // TradeExecuted: isBuy false is the unwitnessed SELL form; 2 is no bool.
    let t = find(FourEventKind::TradeExecuted);
    let mut sell = t.data.clone();
    sell[31] = 0;
    assert_eq!(
        decode_four_log(&deployment, t.emitter, &t.topics, &sell),
        FourLog::Unsupported(FourEventKind::TradeExecuted)
    );
    sell[31] = 2;
    assert_eq!(
        decode_four_log(&deployment, t.emitter, &t.topics, &sell),
        FourLog::Malformed(FourEventKind::TradeExecuted, AbiError::ValueOutOfRange)
    );
    // An indexed address topic with dirty high bytes.
    let mut dirty = t.topics.clone();
    dirty[1][0] = 1;
    assert_eq!(
        decode_four_log(&deployment, t.emitter, &dirty, &t.data),
        FourLog::Malformed(FourEventKind::TradeExecuted, AbiError::ValueOutOfRange)
    );
    // Unwitnessed events of the published ABIs.
    for kind in [
        FourEventKind::TokenSale,
        FourEventKind::LiquidityAdded,
        FourEventKind::TradeStop,
        FourEventKind::MigrateExecuted,
    ] {
        let emitter = deployment.contract(kind.family());
        assert_eq!(
            decode_four_log(&deployment, emitter, &[kind.topic0()], &[]),
            FourLog::Unsupported(kind)
        );
    }
    // A Classic topic emitted by OpenFour Core is unrecognized there.
    let create = find(FourEventKind::TokenCreate);
    assert_eq!(
        decode_four_log(
            &deployment,
            deployment.openfour_core,
            &create.topics,
            &create.data
        ),
        FourLog::Unrecognized(
            Family::OpenFour,
            B256::from(FourEventKind::TokenCreate.topic0())
        )
    );
    assert_eq!(
        decode_four_log(&deployment, deployment.token_manager2, &[], &[]),
        FourLog::Unrecognized(Family::Classic, B256::ZERO)
    );
}

/// Decoding is a pure function of the bytes: the same input decodes the
/// same way every time (replay identity, duplicate sightings).
#[test]
fn the_same_bytes_decode_the_same_way() {
    let deployment = d();
    for c in calls() {
        let a = recognize(&deployment, Some(c.to), &c.input, MAX);
        let copy = c.input.clone();
        let b = recognize(&deployment, Some(c.to), &copy, MAX);
        assert_eq!(a, b);
    }
}

/// Flap and Four share the input stream: each decoder refuses the other's
/// calls and logs (Flap Portal, TokenManager2 and OpenFour Core are three
/// addresses; the selector tables do not overlap).
#[test]
fn flap_and_four_share_inputs_without_crosstalk() {
    let deployment = d();
    let portal = flap::deployment::bsc_mainnet_v5_27_0().portal;
    for r in rows(FLAP_CALLS) {
        let input = unhex(r[3]);
        assert_eq!(
            recognize(&deployment, Some(portal), &input, MAX),
            Recognized::NotFour
        );
        for family in [Family::Classic, Family::OpenFour] {
            assert!(matches!(
                recognize(&deployment, Some(deployment.contract(family)), &input, MAX),
                Recognized::UnknownSelector(f, _) if f == family
            ));
        }
    }
    for c in calls() {
        assert_eq!(
            flap::recognize::recognize(portal, Some(c.to), &c.input, MAX),
            flap::recognize::Recognized::NotPortal
        );
        assert!(matches!(
            flap::recognize::recognize(portal, Some(portal), &c.input, MAX),
            flap::recognize::Recognized::UnknownSelector(_)
        ));
    }
    for l in logs() {
        assert_eq!(
            flap::events::decode_portal_log(portal, l.emitter, &l.topics, &l.data),
            flap::events::PortalLog::NotPortal
        );
    }
}

/// Phase E §9: recognizer and full decode latency on this machine, per call,
/// alone and with Flap enabled (run with `cargo test --release -p
/// darkbot-bsc-trading four_decoder_latency -- --ignored --nocapture`).
/// Each sample is the mean of a batch of `BATCH` calls, so a stage below
/// the timer resolution is still measured. The recognizer stage is
/// destination, selector and table lookup without a decode (a known,
/// unsupported SELL); the launches include the pending token derivation.
/// A local decoder metric, not a BUY claim.
#[test]
#[ignore = "latency measurement; run in release"]
fn four_decoder_latency() {
    use std::hint::black_box;
    use std::time::Instant;
    const SAMPLES: usize = 2_000;
    const BATCH: u64 = 1_000;
    let deployment = d();
    let portal = flap::deployment::bsc_mainnet_v5_27_0().portal;
    let classic = fixture("CLASSIC", "LAUNCH");
    let open = fixture("OPENFOUR", "LAUNCH");
    let classic_buy = fixture("CLASSIC", "BUY");
    let open_buy = fixture("OPENFOUR", "BUY");
    let mut sell = FourCall::ClassicSellToken.selector().to_vec();
    sell.extend_from_slice(&[0; 64]);
    let all = logs();
    let created = all
        .iter()
        .find(|l| l.topics.first() == Some(&FourEventKind::TokenCreated.topic0()))
        .unwrap();
    let create = all
        .iter()
        .find(|l| l.topics.first() == Some(&FourEventKind::TokenCreate.topic0()))
        .unwrap();
    let report = |name: &str, mut samples: Vec<u64>| {
        samples.sort_unstable();
        let q = |p: f64| samples[((samples.len() as f64 - 1.0) * p) as usize];
        println!(
            "{name}: {} batches of {BATCH}, per call p50={} p95={} p99={} p99.9={} p99.99={} max={} ns",
            samples.len(),
            q(0.5),
            q(0.95),
            q(0.99),
            q(0.999),
            q(0.9999),
            samples[samples.len() - 1]
        );
        // §9: a reproducible >1 ms decoder path is a defect.
        assert!(q(0.99) < 1_000_000, "{name} p99 above 1 ms");
    };
    let measure = |f: &dyn Fn()| -> Vec<u64> {
        (0..SAMPLES)
            .map(|_| {
                let t = Instant::now();
                for _ in 0..BATCH {
                    f();
                }
                t.elapsed().as_nanos() as u64 / BATCH
            })
            .collect()
    };
    let launch = |c: &Call| {
        let r = recognize(&deployment, Some(c.to), black_box(&c.input), MAX);
        black_box(pending_token(&deployment, &r));
    };
    report(
        "recognizer (no decode)",
        measure(&|| {
            black_box(recognize(
                &deployment,
                Some(deployment.token_manager2),
                black_box(&sell),
                MAX,
            ));
        }),
    );
    report(
        "Classic createToken recognize+decode+token",
        measure(&|| launch(&classic)),
    );
    report(
        "OpenFour createToken recognize+decode+token",
        measure(&|| launch(&open)),
    );
    report(
        "Classic buyTokenAMAP recognize+decode",
        measure(&|| {
            black_box(recognize(
                &deployment,
                Some(classic_buy.to),
                black_box(&classic_buy.input),
                MAX,
            ));
        }),
    );
    report(
        "OpenFour buyByBudget recognize+decode",
        measure(&|| {
            black_box(recognize(
                &deployment,
                Some(open_buy.to),
                black_box(&open_buy.input),
                MAX,
            ));
        }),
    );
    report(
        "TokenCreate decode",
        measure(&|| {
            black_box(decode_four_log(
                &deployment,
                create.emitter,
                &create.topics,
                black_box(&create.data),
            ));
        }),
    );
    report(
        "TokenCreated decode",
        measure(&|| {
            black_box(decode_four_log(
                &deployment,
                created.emitter,
                &created.topics,
                black_box(&created.data),
            ));
        }),
    );
    // The shared pending input: both decoders on every call (the Flap one
    // refuses a Four launch at the destination, and vice versa).
    report(
        "Flap+Four on an OpenFour launch",
        measure(&|| {
            black_box(flap::recognize::recognize(
                portal,
                Some(open.to),
                black_box(&open.input),
                MAX,
            ));
            launch(&open);
        }),
    );
}

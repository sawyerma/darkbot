//! Synthetic cases: they test the verdict rules only (master plan §13
//! J); none of them is an acquisition sample.
use super::*;
use crate::canonical_join::{FourToken, Joined, LaunchRef, LaunchTx, Venue, Visibility};
use crate::live_ledger::Placement;
use crate::runtime_journal::ReleasedAdapter;
use crate::sources::{NonceOrigin, SourceId};

const BUYER: [u8; 20] = [0xB0; 20];
/// The operator's own launcher wallet (another key than the buyer).
const OWN_LAUNCHER: [u8; 20] = [0xD1; 20];
const RUN: u64 = T0_MS - 3_600_000;
const CREATOR: [u8; 20] = [0xC1; 20];
const TOKEN: [u8; 20] = [0xA1; 20];
const LAUNCH_BLOCK: u64 = 100;
const T0_MS: u64 = 1_793_000_000_000;
const WINDOW_MS: u64 = MARKET_WINDOW_MS;

fn hash(n: u64) -> [u8; 32] {
    let mut h = [0x5a; 32];
    h[..8].copy_from_slice(&n.to_le_bytes());
    h
}

fn launch_tx(i: u8) -> [u8; 32] {
    [0x10 + i; 32]
}

fn buy_tx(i: u8) -> [u8; 32] {
    [0x60 + i; 32]
}

fn token(i: u8) -> [u8; 20] {
    let mut t = TOKEN;
    t[0] = 0xA0 + i;
    t
}

fn fact(kind: FactKind, tx: [u8; 32], tok: [u8; 20]) -> Fact {
    Fact {
        kind,
        tx_hash: tx,
        intent: LiveIntentKind::Buy,
        component: ComponentId::ExecC3,
        token: tok,
        nonce: u64::from(tx[0]),
        grant_generation: 1,
        input_wei: Some(U256::from(1_000_000u64)),
        max_wei: Some(U256::from(2_000_000u64)),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(LAUNCH_BLOCK + 2),
        by_tx: None,
        response: None,
        origin: Some(NonceOrigin {
            chain_id: 56,
            sender: BUYER,
        }),
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    }
}

/// An attempt's facts: released, and (when `block` is given) included
/// there with tokens and placed as `verdict`.
fn facts_of(
    i: u8,
    block: Option<u64>,
    tokens: u64,
    verdict: Option<PlacementVerdict>,
    written_ms: u64,
) -> Vec<(Fact, u64)> {
    let tx = buy_tx(i);
    let released = fact(FactKind::Released, tx, token(i));
    let mut out = vec![(released.clone(), written_ms)];
    if let Some(b) = block {
        let at = Some((b, hash(b), T0_MS + (b - LAUNCH_BLOCK) * 450));
        out.push((
            Fact {
                block: at,
                tokens: Some(U256::from(tokens)),
                eth_wei: Some(U256::from(1_000_000u64)),
                gas_wei: Some(U256::from(30_000u64)),
                ..Fact::about(&released, FactKind::Included)
            },
            written_ms + 1,
        ));
        if let Some(v) = verdict {
            out.push((
                Fact {
                    block: at,
                    launcher: Some(CREATOR),
                    placement: Some(Placement {
                        verdict: v,
                        launch_index: Some(0),
                        own_index: 1,
                        buys_before: 0,
                    }),
                    ..Fact::about(&released, FactKind::Placed)
                },
                written_ms + 2,
            ));
        }
    }
    out
}

fn adapter(boundary_ns: u64) -> ReleasedAdapter {
    ReleasedAdapter {
        adapter: 0,
        builder: Some(0),
        outcome: 0,
        boundary: 0,
        record_qualifies: true,
        transport_qualified: true,
        prepermit_ns: Some(boundary_ns - 30_000),
        write_check_ns: Some(boundary_ns - 20_000),
        boundary_ns: Some(boundary_ns),
        queue_ns: None,
    }
}

fn release(i: u8, misses: u8) -> LiveRelease {
    LiveRelease {
        kind: 0,
        trigger: Some(SourceId::PublicNode),
        tx_hash: buy_tx(i),
        attempt: [0xE0 + i; 32],
        launch_hash: launch_tx(i),
        token: token(i),
        block: 0,
        clock_id: 1,
        times: [1_000, 2_000, 3_000, 4_000, 5_000, 6_000, 7_000, 300_000],
        deadline_misses: misses,
        adapters: vec![adapter(300_000)],
        raw: vec![0xf8],
    }
}

fn launch_ref(i: u8, block: u64) -> LaunchRef {
    LaunchRef {
        venue: Venue::Flap,
        block_number: block,
        block_hash: hash(block),
        tx_index: 0,
        tx_hash: launch_tx(i),
        token: Some(token(i)),
        external_buys: None,
        c1_scope: true,
        venue_target: true,
        four_token: FourToken::NotApplicable,
        tx: LaunchTx::Verified,
        block_ms: T0_MS + (block - LAUNCH_BLOCK) * 450,
        block_seen_ns: 0,
        block_seen_unix_ms: None,
    }
}

fn trade(
    block: u64,
    tx_index: u64,
    tx: [u8; 32],
    tok: [u8; 20],
    sell: bool,
    amount: u64,
    eth: u64,
) -> Vec<u8> {
    let word = |v: u64| U256::from(v).to_be_bytes::<32>();
    rj::flap_trade_payload(&crate::chain_follower::CanonicalTrade {
        block_number: block,
        block_hash: hash(block),
        tx_index,
        tx_hash: tx,
        log_index: tx_index,
        origin: [0x77; 20],
        sell,
        token: tok,
        trader: [0x77; 20],
        amount: word(amount),
        eth: word(eth),
        fee: word(0),
        post_price: word(0),
    })
}

/// Blocks `LAUNCH_BLOCK..LAUNCH_BLOCK + n` 450 ms apart, the launches
/// of `attempts` in the first, their own BUYs at index 1.
fn journal(n: u64, attempts: &[u8]) -> Journaled {
    let mut j = Journaled::default();
    for b in LAUNCH_BLOCK..LAUNCH_BLOCK + n {
        let ms = T0_MS + (b - LAUNCH_BLOCK) * 450;
        j.record(
            RecordKind::Canonical,
            &crate::runtime_strategy::block_payload(b, &hash(b), ms),
            ms,
        );
    }
    for &i in attempts {
        j.record(
            RecordKind::Canonical,
            &rj::canonical_launch_payload(&Joined {
                launch: launch_ref(i, LAUNCH_BLOCK),
                visibility: Visibility::Missed,
            }),
            T0_MS,
        );
        j.record(
            RecordKind::Canonical,
            &crate::runtime_strategy::flap_launch_payload(LAUNCH_BLOCK, 0, &token(i), &CREATOR),
            T0_MS,
        );
        j.record(
            RecordKind::Canonical,
            &trade(
                LAUNCH_BLOCK,
                1,
                buy_tx(i),
                token(i),
                false,
                1_000,
                1_000_000,
            ),
            T0_MS,
        );
        j.releases.insert(
            buy_tx(i),
            Release {
                run: RUN,
                l: release(i, 0),
            },
        );
    }
    j.last_ms = Some(T0_MS + n * 450);
    // One run, sealed whole, of the plan's build, configuration and
    // host, from before the commitment to the journal's end.
    j.runs.push(Run {
        id: RUN,
        manifest: Some(sealed()),
        manifest_match: Some(true),
        records: n,
        last_ms: j.last_ms,
        blocks: Some((LAUNCH_BLOCK, LAUNCH_BLOCK + n - 1)),
        ..Run::default()
    });
    j
}

fn sealed() -> RunManifest {
    RunManifest {
        complete: true,
        started_ms: RUN,
        ended_ms: Some(T0_MS + 40 * 450),
        lost_records: 0,
        ingress_dropped: Some(0),
        build_sha256: Some([1; 32]),
        config_sha256: Some([2; 32]),
        host: Some("test-host".into()),
    }
}

/// A producer timing verdict for attempt `i` with its own G0.8 result
/// (the BUY in the launch's block).
fn witness(i: u8, verdict: &str, bound: u64) -> (String, Result<Witness, String>) {
    let id = attempt_id(&[0xE0 + i; 32]);
    (
        id.clone(),
        Ok(Witness {
            attempt_id: id.clone(),
            verdict: verdict.into(),
            strict_limit_ns: STRICT_LIMIT_NS,
            upper_bound_ns: bound,
            remaining_uncertainty_ns: 100,
            g08_sha256: [5; 32],
            g08: Some(Ok(G08Binding {
                sha256: [5; 32],
                verdict: "PASS".into(),
                attempt_id: id,
                buy_tx: buy_tx(i),
                launch_tx: launch_tx(i),
                buy_block_hash: hash(LAUNCH_BLOCK),
            })),
        }),
    )
}

fn plan(min_attempts: u64) -> Plan {
    Plan {
        build_sha256: [1; 32],
        config_sha256: [2; 32],
        chain_id: 56,
        host: "test-host".into(),
        adapters: vec![LiveAdapter::Club48BuilderV1],
        sources: vec![SourceId::PublicNode],
        self_launchers: vec![OWN_LAUNCHER],
        cells: vec![CellPlan {
            cell: EXTERNAL_FULL_LAUNCH,
            min_attempts,
            min_duration_s: 1,
        }],
        max_skip_ppm: 1_000_000,
        max_consecutive_misses: 10,
        min_covered_ppm: 900_000,
        uncovered_bounded: false,
        clock_error_bound_ns: 1_000,
        finality_blocks: 3,
        reorg_horizon_blocks: 2,
        amount_limit_wei: U256::from(10_000_000u64),
        cost_limit_wei: U256::from(100_000_000u64),
        release_scope: EXTERNAL_FULL_LAUNCH,
    }
}

fn evaluate_one(
    facts: &[(Fact, u64)],
    j: &Journaled,
    plan: Option<&Plan>,
    w: &Witnesses,
) -> Vec<Attempt> {
    attempts(
        facts,
        j,
        &Inputs {
            window_ms: WINDOW_MS,
            plan,
            witnesses: w,
            own_accounts: &[],
        },
    )
}

/// §13 J: 999,999 ns after the uncertainty is accepted, 1,000,000 ns
/// rejected; a PASS verdict whose bound is not below the limit is no
/// witness.
#[test]
fn the_strict_bound_passes_999_999_and_refuses_1_000_000() {
    let facts = facts_of(
        1,
        Some(LAUNCH_BLOCK),
        1_000,
        Some(PlacementVerdict::First),
        T0_MS,
    );
    let j = journal(40, &[1]);
    let p = plan(1);
    let timing = |verdict: &str, bound| {
        let w = HashMap::from([witness(1, verdict, bound)]);
        let a = evaluate_one(&facts, &j, Some(&p), &w).remove(0);
        (a.verdicts.timing, a.verdicts.verdict)
    };
    assert_eq!(timing("PASS", 999_999), (PASS, PASS));
    assert_eq!(timing("FAIL", 1_000_000), (FAIL, FAIL));
    assert_eq!(timing("PASS", 1_000_000), (UNKNOWN, UNKNOWN));
    assert_eq!(timing("FAIL", 999_999), (UNKNOWN, UNKNOWN));
}

/// A missing witness, one of another attempt, of another limit or with
/// more uncertainty than the manifest allows: UNKNOWN, never PASS.
#[test]
fn a_missing_or_foreign_witness_leaves_timing_unknown() {
    let facts = facts_of(
        1,
        Some(LAUNCH_BLOCK),
        1_000,
        Some(PlacementVerdict::First),
        T0_MS,
    );
    let j = journal(40, &[1]);
    let p = plan(1);
    let run = |w: Witnesses| {
        let a = evaluate_one(&facts, &j, Some(&p), &w).remove(0);
        (a.verdicts.timing, a.reasons)
    };
    let (t, why) = run(HashMap::new());
    assert_eq!(t, UNKNOWN);
    assert!(why.contains(&"NO_PRODUCER_WITNESS".to_string()));
    let (id, w) = witness(1, "PASS", 1);
    let foreign = Witness {
        attempt_id: attempt_id(&[9; 32]),
        ..w.clone().unwrap()
    };
    assert_eq!(run(HashMap::from([(id.clone(), Ok(foreign))])).0, UNKNOWN);
    let limit = Witness {
        strict_limit_ns: 2_000_000,
        ..w.clone().unwrap()
    };
    assert_eq!(run(HashMap::from([(id.clone(), Ok(limit))])).0, UNKNOWN);
    let loose = Witness {
        remaining_uncertainty_ns: 1_001,
        ..w.clone().unwrap()
    };
    assert_eq!(run(HashMap::from([(id.clone(), Ok(loose))])).0, UNKNOWN);
    assert_eq!(run(HashMap::from([(id, Err("torn".into()))])).0, UNKNOWN);
}

/// A send without acquisition never passes: included without tokens
/// FAIL, unresolved UNKNOWN, its window over without inclusion FAIL.
#[test]
fn a_send_without_acquisition_never_passes() {
    let j = journal(40, &[1]);
    let w = HashMap::from([witness(1, "PASS", 1)]);
    let p = plan(1);
    let no_tokens = facts_of(
        1,
        Some(LAUNCH_BLOCK),
        0,
        Some(PlacementVerdict::NoTokens),
        T0_MS,
    );
    let a = evaluate_one(&no_tokens, &j, Some(&p), &w).remove(0);
    assert_eq!((a.verdicts.acquisition, a.verdicts.verdict), (FAIL, FAIL));
    let pending = facts_of(1, None, 0, None, T0_MS);
    let mut short = journal(2, &[1]);
    short.blocks.retain(|n, _| *n <= LAUNCH_BLOCK + 1);
    let a = evaluate_one(&pending, &short, Some(&p), &w).remove(0);
    assert_eq!(
        (a.state, a.verdicts.acquisition, a.verdicts.verdict),
        ("RELEASED", UNKNOWN, UNKNOWN)
    );
    // Its window (to LAUNCH_BLOCK + 2) is over at the journal's head:
    // still unresolved, its bytes may execute later (review L1).
    let a = evaluate_one(&pending, &j, Some(&p), &w).remove(0);
    assert_eq!(
        (a.state, a.verdicts.acquisition, a.verdicts.verdict),
        ("DROPPED", UNKNOWN, UNKNOWN)
    );
    assert!(a.unresolved() && a.reasons.contains(&"WINDOW_OVER_UNRESOLVED".to_string()));
}

/// A same-block acquisition without its producer timing: UNKNOWN.
#[test]
fn a_same_block_acquisition_without_timing_is_unknown() {
    let facts = facts_of(
        1,
        Some(LAUNCH_BLOCK),
        1_000,
        Some(PlacementVerdict::First),
        T0_MS,
    );
    let a = evaluate_one(&facts, &journal(40, &[1]), Some(&plan(1)), &HashMap::new()).remove(0);
    let v = a.verdicts;
    assert_eq!(
        [
            v.acquisition,
            v.canonical,
            v.placement,
            v.capture,
            v.local,
            v.economics
        ],
        [PASS; 6]
    );
    assert_eq!((v.timing, v.verdict), (UNKNOWN, UNKNOWN));
}

/// A fast acquisition behind another trader's BUY fails; so does one
/// acknowledged and then included only after its launch's block.
#[test]
fn a_fast_acquisition_in_the_wrong_place_fails() {
    let j = journal(40, &[1]);
    // Its G0.8 result names the block the BUY landed in.
    let (id, mut wit) = witness(1, "PASS", 1);
    if let Ok(Witness {
        g08: Some(Ok(g)), ..
    }) = &mut wit
    {
        g.buy_block_hash = hash(LAUNCH_BLOCK + 1);
    }
    let w = HashMap::from([(id, wit)]);
    for v in [PlacementVerdict::Late, PlacementVerdict::OtherBlock] {
        let facts = facts_of(1, Some(LAUNCH_BLOCK + 1), 1_000, Some(v), T0_MS);
        let mut j = j.clone();
        j.answers.insert((buy_tx(1), 0), (0, Some(310_000)));
        let a = evaluate_one(&facts, &j, Some(&plan(1)), &w).remove(0);
        assert_eq!(
            (a.verdicts.timing, a.verdicts.placement, a.verdicts.verdict),
            (PASS, FAIL, FAIL)
        );
        assert_eq!(a.local.unwrap().ack_ns, Some(310_000));
        assert_eq!(a.adapters[0].answer, Some("ACK"));
    }
}

/// A violated local start deadline fails the attempt; no qualified
/// send time leaves it UNKNOWN.
#[test]
fn the_local_deadline_and_send_time_are_judged() {
    let facts = facts_of(
        1,
        Some(LAUNCH_BLOCK),
        1_000,
        Some(PlacementVerdict::First),
        T0_MS,
    );
    let w = HashMap::from([witness(1, "PASS", 1)]);
    let mut j = journal(40, &[1]);
    j.releases.insert(
        buy_tx(1),
        Release {
            run: RUN,
            l: release(1, 1),
        },
    );
    assert_eq!(
        evaluate_one(&facts, &j, Some(&plan(1)), &w)[0]
            .verdicts
            .local,
        FAIL
    );
    let mut r = release(1, 0);
    r.adapters[0].transport_qualified = false;
    j.releases.insert(buy_tx(1), Release { run: RUN, l: r });
    let a = evaluate_one(&facts, &j, Some(&plan(1)), &w).remove(0);
    assert_eq!(
        (a.verdicts.local, a.local.unwrap().send_ns),
        (UNKNOWN, None)
    );
    j.releases.clear();
    assert_eq!(
        evaluate_one(&facts, &j, Some(&plan(1)), &w)[0]
            .verdicts
            .local,
        UNKNOWN
    );
}

/// The market window: every trade of the token from the launch's
/// block to the last block within the window, the first other
/// trader's BUY (never the launch's own) and the price against it.
#[test]
fn the_window_counts_trades_and_prices_against_the_first_external_buy() {
    let mut j = journal(40, &[1]);
    // The launch's own dev BUY, an external BUY at twice our price, a
    // SELL, a trade after the window and one of another token.
    for p in [
        trade(
            LAUNCH_BLOCK,
            0,
            launch_tx(1),
            token(1),
            false,
            5_000,
            1_000_000,
        ),
        trade(
            LAUNCH_BLOCK + 1,
            0,
            [0x99; 32],
            token(1),
            false,
            500,
            1_000_000,
        ),
        trade(
            LAUNCH_BLOCK + 2,
            0,
            [0x98; 32],
            token(1),
            true,
            100,
            150_000,
        ),
        trade(LAUNCH_BLOCK + 35, 0, [0x97; 32], token(1), false, 1, 1),
        trade(LAUNCH_BLOCK + 1, 1, [0x96; 32], token(2), false, 1, 1),
    ] {
        j.record(RecordKind::Canonical, &p, T0_MS);
    }
    let facts = facts_of(
        1,
        Some(LAUNCH_BLOCK),
        1_000,
        Some(PlacementVerdict::First),
        T0_MS,
    );
    let w = evaluate_one(&facts, &j, Some(&plan(1)), &HashMap::new())
        .remove(0)
        .window;
    assert_eq!(w.why_incomplete, None);
    // 15 s at 450 ms: blocks 100..=133.
    assert_eq!(
        (w.launch_block, w.end_block),
        (Some(LAUNCH_BLOCK), Some(LAUNCH_BLOCK + 33))
    );
    assert_eq!((w.trades, w.buys, w.sells), (4, 3, 1));
    assert_eq!(w.buy_wei, U256::from(3_000_000u64));
    assert_eq!(w.sell_wei, U256::from(150_000u64));
    let (tx, block, index, price) = w.first_external.unwrap();
    assert_eq!((tx, block, index), ([0x99; 32], LAUNCH_BLOCK + 1, 0));
    assert_eq!(price, U256::from(2_000u64) * U256::from(E18));
    assert_eq!(w.own_price, Some(U256::from(1_000u64) * U256::from(E18)));
    assert_eq!(w.price_vs_first_external_ppm, Some(500_000));
}

/// A window is incomplete while a block in it is missing, a Portal log
/// in it was refused, it is not over, or its end is within the reorg
/// horizon; a reorg's replacement block replaces the orphan's trades.
#[test]
fn a_window_with_a_gap_or_an_unresolved_reorg_is_incomplete() {
    let facts = facts_of(
        1,
        Some(LAUNCH_BLOCK),
        1_000,
        Some(PlacementVerdict::First),
        T0_MS,
    );
    let why = |j: &Journaled| {
        evaluate_one(&facts, j, Some(&plan(1)), &HashMap::new())[0]
            .window
            .why_incomplete
    };
    let mut gap = journal(40, &[1]);
    gap.blocks.remove(&(LAUNCH_BLOCK + 5));
    assert_eq!(why(&gap), Some("BLOCK_NOT_JOURNALED"));
    let mut refused = journal(40, &[1]);
    refused
        .refused
        .insert((LAUNCH_BLOCK + 3, hash(LAUNCH_BLOCK + 3)));
    assert_eq!(why(&refused), Some("TRADE_LOG_REFUSED"));
    assert_eq!(why(&journal(20, &[1])), Some("WINDOW_NOT_OVER"));
    // Block 133 ends it, 134 is past it: the horizon (2) needs 135.
    assert_eq!(why(&journal(35, &[1])), Some("REORG_HORIZON_NOT_REACHED"));
    assert_eq!(why(&journal(36, &[1])), None);
    // A trade of an orphaned block 101 is not counted once its
    // replacement (another hash) is journaled.
    let mut reorg = journal(40, &[1]);
    reorg.record(
        RecordKind::Canonical,
        &trade(
            LAUNCH_BLOCK + 1,
            0,
            [0x99; 32],
            token(1),
            false,
            500,
            1_000_000,
        ),
        T0_MS,
    );
    let mut other = hash(LAUNCH_BLOCK + 1);
    other[31] ^= 1;
    reorg.blocks.insert(LAUNCH_BLOCK + 1, (other, T0_MS + 450));
    let w = evaluate_one(&facts, &reorg, Some(&plan(1)), &HashMap::new())
        .remove(0)
        .window;
    assert_eq!((w.trades, w.first_external), (1, None));
    let a = evaluate_one(&facts, &journal(10, &[1]), Some(&plan(1)), &HashMap::new()).remove(0);
    assert_eq!(a.verdicts.capture, UNKNOWN);
}

fn manifest_for(p: &Plan) -> Manifest {
    Manifest {
        reviewed: true,
        missing: Vec::new(),
        plan: Some(Ok(p.clone())),
        sha256: [7; 32],
    }
}

fn precommit(at: u64) -> Precommit {
    Precommit {
        manifest_sha256: [7; 32],
        decided_by: "operator".into(),
        committed_unix_ms: at,
        sha256: [8; 32],
    }
}

fn rate(
    attempts: &[Attempt],
    facts: &[(Fact, u64)],
    j: &Journaled,
    m: &Result<Manifest, String>,
    c: Option<&Result<Precommit, String>>,
) -> Campaign {
    campaign(
        attempts,
        facts,
        j,
        &Rating {
            manifest: Some(m),
            precommit: c,
            build_sha256: Some([1; 32]),
            config_sha256: [2; 32],
            follow_tolerance_ms: 30_000,
        },
    )
}

/// Three attempts, each PASS but the third (`third`'s bound): their
/// facts, the journal and the witnesses.
fn three(third: u64) -> (Vec<(Fact, u64)>, Journaled, Witnesses) {
    let mut facts = Vec::new();
    let mut w = HashMap::new();
    for (i, bound) in [(1, 1), (2, 1), (3, third)] {
        facts.extend(facts_of(
            i,
            Some(LAUNCH_BLOCK),
            1_000,
            Some(PlacementVerdict::First),
            T0_MS,
        ));
        let v = if bound < STRICT_LIMIT_NS {
            "PASS"
        } else {
            "FAIL"
        };
        w.extend([witness(i, v, bound)]);
    }
    (facts, journal(40, &[1, 2, 3]), w)
}

/// A synthetic campaign meeting every rule passes (rules only); one
/// slow attempt among fast ones fails it.
#[test]
fn one_slow_attempt_fails_the_campaign() {
    let p = plan(3);
    let m = Ok(manifest_for(&p));
    let c = Ok(precommit(T0_MS - 10_000));
    let (facts, j, w) = three(1);
    let a = evaluate_one(&facts, &j, Some(&p), &w);
    assert_eq!(
        a.last().unwrap().counters,
        Counters {
            attempts: 3,
            acquired: 3,
            passed: 3,
            failed: 0,
            undecided: 0
        }
    );
    let v = rate(&a, &facts, &j, &m, Some(&c));
    assert_eq!(
        (v.verdict, v.release),
        (CAMPAIGN_PASS, RELEASE_EVIDENCE_PASS),
        "{:?}",
        v.reasons
    );
    let (facts, j, w) = three(1_000_000);
    let a = evaluate_one(&facts, &j, Some(&p), &w);
    let v = rate(&a, &facts, &j, &m, Some(&c));
    assert_eq!(
        (v.verdict, v.release),
        (CAMPAIGN_FAIL, RELEASE_EVIDENCE_NOT_READY)
    );
    assert_eq!((v.passed, v.failed), (2, 1));
}

/// No admitted attempt never passes; nor does a campaign short of a
/// cell's attempts or duration; misses in a row above the limit fail.
#[test]
fn a_campaign_without_its_samples_never_passes() {
    let p = plan(3);
    let m = Ok(manifest_for(&p));
    let c = Ok(precommit(T0_MS - 10_000));
    let j = journal(40, &[1, 2]);
    let v = rate(&[], &[], &j, &m, Some(&c));
    assert_eq!(v.verdict, CAMPAIGN_OPEN);
    assert!(v.reasons.contains(&"NO_ADMITTED_ATTEMPT".to_string()));
    assert_eq!((v.observed, v.skipped, v.consecutive_misses), (2, 2, 2));
    let strict = Plan {
        max_consecutive_misses: 1,
        ..p.clone()
    };
    let v = rate(&[], &[], &j, &Ok(manifest_for(&strict)), Some(&c));
    assert_eq!(v.verdict, CAMPAIGN_FAIL);
    let (facts, j, w) = three(1);
    let a = evaluate_one(&facts, &j, Some(&p), &w);
    let more = Plan {
        cells: vec![CellPlan {
            cell: EXTERNAL_FULL_LAUNCH,
            min_attempts: 4,
            min_duration_s: 1,
        }],
        ..p.clone()
    };
    let v = rate(&a, &facts, &j, &Ok(manifest_for(&more)), Some(&c));
    assert_eq!(v.verdict, CAMPAIGN_OPEN);
    let longer = Plan {
        cells: vec![CellPlan {
            cell: EXTERNAL_FULL_LAUNCH,
            min_attempts: 3,
            min_duration_s: 3_600,
        }],
        ..p
    };
    let v = rate(&a, &facts, &j, &Ok(manifest_for(&longer)), Some(&c));
    assert_eq!(
        (v.verdict, v.cells[0].verdict),
        (CAMPAIGN_OPEN, CAMPAIGN_OPEN)
    );
}

/// No manifest, an unreviewed or incomplete one, one of another build
/// or configuration, no pre-commitment or a manifest changed after it:
/// CAMPAIGN_NOT_READY; attempts before the commitment are not the
/// campaign's.
#[test]
fn a_manifest_not_frozen_before_the_campaign_rates_nothing() {
    let p = plan(3);
    let (facts, j, w) = three(1);
    let a = evaluate_one(&facts, &j, Some(&p), &w);
    let c = Ok(precommit(T0_MS - 10_000));
    let none = campaign(
        &a,
        &facts,
        &j,
        &Rating {
            manifest: None,
            precommit: Some(&c),
            build_sha256: Some([1; 32]),
            config_sha256: [2; 32],
            follow_tolerance_ms: 30_000,
        },
    );
    assert_eq!(none.verdict, CAMPAIGN_NOT_READY);
    let proposed = Ok(Manifest {
        reviewed: false,
        plan: None,
        missing: vec!["BSC_CAMPAIGN_REVIEWER"],
        ..manifest_for(&p)
    });
    assert_eq!(
        rate(&a, &facts, &j, &proposed, Some(&c)).verdict,
        CAMPAIGN_NOT_READY
    );
    let other_build = Ok(manifest_for(&Plan {
        build_sha256: [9; 32],
        ..p.clone()
    }));
    assert_eq!(
        rate(&a, &facts, &j, &other_build, Some(&c)).verdict,
        CAMPAIGN_NOT_READY
    );
    let other_config = Ok(manifest_for(&Plan {
        config_sha256: [9; 32],
        ..p.clone()
    }));
    assert_eq!(
        rate(&a, &facts, &j, &other_config, Some(&c)).verdict,
        CAMPAIGN_NOT_READY
    );
    let m = Ok(manifest_for(&p));
    assert_eq!(rate(&a, &facts, &j, &m, None).verdict, CAMPAIGN_NOT_READY);
    let changed = Ok(Precommit {
        manifest_sha256: [6; 32],
        ..precommit(T0_MS - 10_000)
    });
    let v = rate(&a, &facts, &j, &m, Some(&changed));
    assert_eq!(v.verdict, CAMPAIGN_NOT_READY);
    assert!(
        v.reasons
            .contains(&"MANIFEST_CHANGED_AFTER_PRECOMMIT".to_string())
    );
    // Committed after the attempts this run directory journaled: its
    // commitment came after its results (review N2).
    let late = Ok(precommit(T0_MS + 1));
    let v = rate(&a, &facts, &j, &m, Some(&late));
    assert_eq!((v.verdict, v.written), (CAMPAIGN_NOT_READY, 0));
    assert!(
        v.reasons
            .contains(&"EARLIER_ATTEMPTS_IN_THIS_RUN_DIRECTORY: 3".to_string())
    );
}

/// Self-launch evidence never releases a third-party-launch product.
#[test]
fn self_launch_evidence_does_not_release_third_party_launches() {
    let (mut facts, j, w) = three(1);
    // Review H3: the operator's own launcher wallet, not the buyer's.
    for (f, _) in &mut facts {
        if f.launcher.is_some() {
            f.launcher = Some(OWN_LAUNCHER);
        }
    }
    let unlisted = Plan {
        self_launchers: Vec::new(),
        ..plan(3)
    };
    let cells = |p: Option<&Plan>| {
        evaluate_one(&facts, &j, p, &w)
            .iter()
            .map(|a| a.cell)
            .collect::<Vec<_>>()
    };
    assert_eq!(cells(Some(&unlisted)), [EXTERNAL_FULL_LAUNCH; 3]);
    assert_eq!(
        cells(None),
        [CELL_UNKNOWN; 3],
        "no manifest: not known to be another's"
    );
    let p = Plan {
        cells: vec![CellPlan {
            cell: SELF_LAUNCH,
            min_attempts: 3,
            min_duration_s: 1,
        }],
        ..plan(3)
    };
    let a = evaluate_one(&facts, &j, Some(&p), &w);
    assert!(a.iter().all(|a| a.cell == SELF_LAUNCH));
    let c = Ok(precommit(T0_MS - 10_000));
    let v = rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&c));
    assert_eq!(
        (v.verdict, v.release),
        (CAMPAIGN_PASS, RELEASE_EVIDENCE_NOT_READY)
    );
    let claims_self = Plan {
        release_scope: SELF_LAUNCH,
        ..p
    };
    let v = rate(&a, &facts, &j, &Ok(manifest_for(&claims_self)), Some(&c));
    assert_eq!(v.release, RELEASE_EVIDENCE_PASS);
}

/// Spending beyond the manifest's limits fails the campaign.
#[test]
fn spending_beyond_the_manifests_limits_fails_the_campaign() {
    let (facts, j, w) = three(1);
    let c = Ok(precommit(T0_MS - 10_000));
    for p in [
        Plan {
            amount_limit_wei: U256::from(999_999u64),
            ..plan(3)
        },
        Plan {
            cost_limit_wei: U256::from(3_000_000u64),
            ..plan(3)
        },
    ] {
        let a = evaluate_one(&facts, &j, Some(&p), &w);
        assert_eq!(
            rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&c)).verdict,
            CAMPAIGN_FAIL
        );
    }
}

const MANIFEST: &str = "BSC_CAMPAIGN_STATE=REVIEWED
BSC_CAMPAIGN_ID=c1
BSC_CAMPAIGN_BUILD_SHA256=0101010101010101010101010101010101010101010101010101010101010101
BSC_CAMPAIGN_CONFIG_SHA256=0x0202020202020202020202020202020202020202020202020202020202020202
BSC_CAMPAIGN_CHAIN_ID=56
BSC_CAMPAIGN_HOST=test-host
BSC_CAMPAIGN_ADAPTERS=CLUB48_BUILDER_V1
BSC_CAMPAIGN_SOURCES=BSC_INGRESS_PUBLICNODE
BSC_CAMPAIGN_SELF_LAUNCHERS=0xd1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1
BSC_CAMPAIGN_CELLS=EXTERNAL_FULL_LAUNCH
BSC_CAMPAIGN_EXTERNAL_FULL_LAUNCH_MIN_ATTEMPTS=3
BSC_CAMPAIGN_EXTERNAL_FULL_LAUNCH_MIN_DURATION_S=1
BSC_CAMPAIGN_SELF_LAUNCH_MIN_ATTEMPTS=
BSC_CAMPAIGN_SELF_LAUNCH_MIN_DURATION_S=
BSC_CAMPAIGN_SAMPLE_RATIONALE=r
BSC_CAMPAIGN_WORKLOAD=one launch at a time
BSC_CAMPAIGN_FAILURE_SCENARIOS=restart, reorg, provider refusal
BSC_CAMPAIGN_ELIGIBILITY=CANONICAL_FLAP_C1_SCOPE
BSC_CAMPAIGN_MAX_SKIP_PPM=1000000
BSC_CAMPAIGN_MAX_CONSECUTIVE_MISSES=10
BSC_CAMPAIGN_MIN_COVERED_PPM=900000
BSC_CAMPAIGN_UNCOVERED=OPEN
BSC_CAMPAIGN_PRODUCER_WITNESS=w
BSC_CAMPAIGN_CLOCK_ERROR_BOUND_NS=1000
BSC_CAMPAIGN_PLACEMENT=FIRST_AFTER_LAUNCH
BSC_CAMPAIGN_FINALITY_BLOCKS=3
BSC_CAMPAIGN_REORG_HORIZON_BLOCKS=2
BSC_CAMPAIGN_AUTHORIZATION_REF=a
BSC_CAMPAIGN_AMOUNT_LIMIT_WEI=10000000
BSC_CAMPAIGN_COST_LIMIT_WEI=100000000
BSC_CAMPAIGN_STOP_CONDITIONS=s
BSC_CAMPAIGN_REVIEWER=x
BSC_CAMPAIGN_RELEASE_SCOPE=EXTERNAL_FULL_LAUNCH
";

/// The manifest file is read strictly: every key once, none unknown; a
/// reviewed one names every value of an enabled cell; the repository's
/// file is a proposal that rates nothing.
#[test]
fn the_manifest_is_parsed_strictly() {
    let m = Manifest::parse(MANIFEST.as_bytes()).unwrap();
    assert!(m.missing.is_empty(), "{:?}", m.missing);
    assert_eq!(m.plan, Some(Ok(plan(3))));
    let edit = |from: &str, to: &str| Manifest::parse(MANIFEST.replace(from, to).as_bytes());
    assert!(
        edit(
            "BSC_CAMPAIGN_ID=c1\n",
            "BSC_CAMPAIGN_ID=c1\nBSC_CAMPAIGN_ID=c2\n"
        )
        .is_err()
    );
    assert!(edit("BSC_CAMPAIGN_ID=c1\n", "BSC_CAMPAIGN_ID=c1\nBSC_OTHER=1\n").is_err());
    assert!(edit("BSC_CAMPAIGN_ID=c1\n", "").is_err());
    let empty = edit("BSC_CAMPAIGN_REVIEWER=x", "BSC_CAMPAIGN_REVIEWER=").unwrap();
    assert_eq!(
        (empty.missing, empty.plan),
        (vec!["BSC_CAMPAIGN_REVIEWER"], None)
    );
    let both = edit(
        "CELLS=EXTERNAL_FULL_LAUNCH",
        "CELLS=EXTERNAL_FULL_LAUNCH,SELF_LAUNCH",
    )
    .unwrap();
    assert_eq!(both.missing.len(), 2);
    for (from, to) in [
        ("PLACEMENT=FIRST_AFTER_LAUNCH", "PLACEMENT=ANY"),
        ("MIN_ATTEMPTS=3", "MIN_ATTEMPTS=0"),
        ("MAX_SKIP_PPM=1000000", "MAX_SKIP_PPM=1000001"),
        ("AMOUNT_LIMIT_WEI=10000000", "AMOUNT_LIMIT_WEI=0"),
        ("CLOCK_ERROR_BOUND_NS=1000", "CLOCK_ERROR_BOUND_NS=1000001"),
        (
            "RELEASE_SCOPE=EXTERNAL_FULL_LAUNCH",
            "RELEASE_SCOPE=PRIVATE",
        ),
        ("UNCOVERED=OPEN", "UNCOVERED=SOMETIMES"),
    ] {
        assert!(matches!(edit(from, to).unwrap().plan, Some(Err(_))), "{to}");
    }
    // BOUNDED needs a floor above zero.
    let bounded = MANIFEST
        .replace("UNCOVERED=OPEN", "UNCOVERED=BOUNDED")
        .replace("MIN_COVERED_PPM=900000", "MIN_COVERED_PPM=0");
    let bounded = Manifest::parse(bounded.as_bytes()).unwrap().plan;
    assert!(matches!(bounded, Some(Err(_))), "{bounded:?}");
    let bounded = MANIFEST.replace("UNCOVERED=OPEN", "UNCOVERED=BOUNDED");
    let plan = Manifest::parse(bounded.as_bytes()).unwrap().plan;
    assert!(plan.unwrap().unwrap().uncovered_bounded);
    let proposed = edit("STATE=REVIEWED", "STATE=PROPOSED").unwrap();
    assert_eq!((proposed.reviewed, proposed.plan), (false, None));
    let repo =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../config/live_campaign.env");
    let repo = Manifest::parse(&std::fs::read(repo).unwrap()).unwrap();
    assert!(!repo.reviewed && repo.plan.is_none() && !repo.missing.is_empty());
}

/// A pre-commitment is written once, only for a reviewed and complete
/// manifest with valid values, and reads back.
#[test]
fn a_precommit_is_written_once_for_a_reviewed_manifest() {
    let dir = std::env::temp_dir().join(format!("bsc-live-precommit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let m = Manifest::parse(MANIFEST.as_bytes()).unwrap();
    let proposed = Manifest::parse(
        MANIFEST
            .replace("STATE=REVIEWED", "STATE=PROPOSED")
            .as_bytes(),
    )
    .unwrap();
    assert!(Precommit::write(&dir, &proposed, "op", 5).is_err());
    assert!(Precommit::write(&dir, &m, " ", 5).is_err());
    let p = Precommit::write(&dir, &m, "op", 5).unwrap();
    assert_eq!((p.manifest_sha256, p.committed_unix_ms), (m.sha256, 5));
    assert!(Precommit::write(&dir, &m, "op", 6).is_err());
    assert_eq!(Precommit::read(&dir).unwrap(), Some(p));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// An attempt's record and the campaign's verdict are whole records of
/// their schemas.
#[test]
fn the_records_round_trip() {
    let p = plan(3);
    let (facts, mut j, w) = three(1);
    j.answers.insert((buy_tx(1), 0), (0, Some(310_000)));
    let a = evaluate_one(&facts, &j, Some(&p), &w);
    let bytes = a[0].encode(9).unwrap();
    let r = darkbot_bsc_record::decode(&la::SCHEMA, &bytes).unwrap();
    assert_eq!(r.text(la::VERDICT).unwrap(), PASS);
    assert_eq!(r.u64(la::SELECT_TO_SEND_NS).unwrap(), 293_000);
    assert_eq!(r.u64(la::SEND_TO_ACK_NS).unwrap(), 10_000);
    assert_eq!(r.nested_all(la::ADAPTERS).unwrap().len(), 1);
    assert_eq!(r.u64(la::ATTEMPTS).unwrap(), 1);
    let v = rate(
        &a,
        &facts,
        &j,
        &Ok(manifest_for(&p)),
        Some(&Ok(precommit(T0_MS - 10_000))),
    );
    let r = darkbot_bsc_record::decode(&lv::SCHEMA, &v.encode(9).unwrap()).unwrap();
    assert_eq!(r.text(lv::VERDICT).unwrap(), CAMPAIGN_PASS);
    assert_eq!(r.nested_all(lv::CELLS).unwrap().len(), CELLS.len());
}

/// The green synthetic campaign of [`one_slow_attempt_fails_the_campaign`]
/// with `edit` applied to its journal and plan: its verdict.
fn green_with(edit: impl Fn(&mut Journaled, &mut Plan, &mut Vec<(Fact, u64)>)) -> Campaign {
    let (mut facts, mut j, w) = three(1);
    let mut p = plan(3);
    edit(&mut j, &mut p, &mut facts);
    let a = evaluate_one(&facts, &j, Some(&p), &w);
    let c = Ok(precommit(T0_MS - 10_000));
    rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&c))
}

fn reasons(c: &Campaign) -> String {
    c.reasons.join(" | ")
}

/// Review H1: a run not sealed whole (still written, incomplete, with
/// lost or dropped records, drops unknown), a gap in the followed chain
/// or a launch not verifiable leaves the campaign open; its duration is
/// the time its sealed runs cover, not the wall time.
#[test]
fn the_campaign_needs_every_run_sealed_whole_and_the_chain_without_a_gap() {
    assert_eq!(green_with(|_, _, _| {}).verdict, CAMPAIGN_PASS);
    let open = |why: &str, edit: &dyn Fn(&mut Journaled)| {
        let c = green_with(|j, _, _| edit(j));
        assert_eq!(c.verdict, CAMPAIGN_OPEN, "{why}: {}", reasons(&c));
        assert!(reasons(&c).contains(why), "{why}: {}", reasons(&c));
    };
    open("RUN_NOT_SEALED", &|j| j.runs[0].manifest = None);
    open("RUN_INCOMPLETE", &|j| {
        j.runs[0].manifest.as_mut().unwrap().complete = false;
    });
    open("RUN_LOST_RECORDS", &|j| {
        j.runs[0].manifest.as_mut().unwrap().lost_records = 1;
    });
    open("RUN_INGRESS_DROPPED", &|j| {
        j.runs[0].manifest.as_mut().unwrap().ingress_dropped = Some(2);
    });
    open("RUN_INGRESS_DROPS_UNKNOWN", &|j| {
        j.runs[0].manifest.as_mut().unwrap().ingress_dropped = None;
    });
    open("JOURNAL_UNREADABLE", &|j| {
        j.runs[0].read_error = Some("torn".into())
    });
    // A block after every window is missing: the chain has a gap.
    open("CANONICAL_GAP: run", &|j| {
        j.blocks.remove(&(LAUNCH_BLOCK + 37));
    });
    // A launch whose transaction could not be verified.
    open("LAUNCHES_UNVERIFIABLE: 1", &|j| {
        j.record(
            RecordKind::Canonical,
            &rj::canonical_launch_payload(&Joined {
                launch: LaunchRef {
                    tx: LaunchTx::Unrebuildable,
                    c1_scope: false,
                    tx_hash: [0x44; 32],
                    ..launch_ref(9, LAUNCH_BLOCK + 2)
                },
                visibility: Visibility::Missed,
            }),
            RUN,
        );
    });
    // Covered time is the chain time the run followed (17.55 s of
    // blocks), not its wall time: short of a two-minute cell.
    let c = green_with(|_, p, _| p.cells[0].min_duration_s = 120);
    assert_eq!((c.verdict, c.covered_s), (CAMPAIGN_OPEN, 17));
    assert!(reasons(&c).contains("CELL_MINIMUMS_NOT_MET"));
}

/// Review H2: a campaign run of another build, configuration or host,
/// or an attempt through a builder or from a source the manifest does
/// not name, rates nothing; an attempt without its release record
/// stays open.
#[test]
fn every_run_and_attempt_is_bound_to_the_frozen_scope() {
    let not_ready = |why: &str, edit: &dyn Fn(&mut Journaled, &mut Plan)| {
        let c = green_with(|j, p, _| edit(j, p));
        assert_eq!(c.verdict, CAMPAIGN_NOT_READY, "{why}: {}", reasons(&c));
        assert!(reasons(&c).contains(why), "{why}: {}", reasons(&c));
    };
    not_ready("RUN_OF_ANOTHER_BUILD", &|j, _| {
        j.runs[0].manifest.as_mut().unwrap().build_sha256 = Some([9; 32]);
    });
    not_ready("RUN_BUILD_UNKNOWN", &|j, _| {
        j.runs[0].manifest.as_mut().unwrap().build_sha256 = None;
    });
    not_ready("RUN_OF_ANOTHER_CONFIG", &|j, _| {
        j.runs[0].manifest.as_mut().unwrap().config_sha256 = Some([9; 32]);
    });
    not_ready("RUN_ON_ANOTHER_HOST", &|j, _| {
        j.runs[0].manifest.as_mut().unwrap().host = Some("other".into());
    });
    not_ready("ATTEMPT_OUTSIDE_FROZEN_SCOPE", &|_, p| {
        p.adapters = vec![LiveAdapter::BlockRazorBuilderV1];
    });
    not_ready("ATTEMPT_OUTSIDE_FROZEN_SCOPE", &|_, p| {
        p.sources = vec![SourceId::Drpc];
    });
    let c = green_with(|j, _, _| {
        j.releases.remove(&buy_tx(2));
    });
    assert_eq!(c.verdict, CAMPAIGN_OPEN, "{}", reasons(&c));
    assert!(reasons(&c).contains("ATTEMPT_WITHOUT_RELEASE_RECORD"));
}

/// Review M3: an inclusion the journaled chain does not hold at its
/// height is not canonical, however deep.
#[test]
fn an_inclusion_outside_the_journaled_chain_is_not_canonical() {
    let facts = facts_of(
        1,
        Some(LAUNCH_BLOCK),
        1_000,
        Some(PlacementVerdict::First),
        T0_MS,
    );
    let mut j = journal(40, &[1]);
    let mut other = hash(LAUNCH_BLOCK);
    other[31] ^= 1;
    j.blocks.insert(LAUNCH_BLOCK, (other, T0_MS));
    let a = evaluate_one(&facts, &j, Some(&plan(1)), &HashMap::new()).remove(0);
    assert_eq!(a.verdicts.canonical, UNKNOWN);
    assert!(
        a.reasons
            .contains(&"INCLUSION_NOT_IN_THE_JOURNALED_CHAIN".to_string())
    );
}

/// Review M4: a witness is of this execution only through its own
/// G0.8 result: without it, with another one, or with one of another
/// transaction, launch or block, the timing stays UNKNOWN.
#[test]
fn a_witness_counts_only_with_its_own_g08_result() {
    let facts = facts_of(
        1,
        Some(LAUNCH_BLOCK),
        1_000,
        Some(PlacementVerdict::First),
        T0_MS,
    );
    let j = journal(40, &[1]);
    let p = plan(1);
    let timing = |edit: &dyn Fn(&mut Witness)| {
        let (id, mut w) = witness(1, "PASS", 1);
        edit(w.as_mut().unwrap());
        let a = evaluate_one(&facts, &j, Some(&p), &HashMap::from([(id, w)])).remove(0);
        (a.verdicts.timing, a.reasons)
    };
    assert_eq!(timing(&|_| {}).0, PASS);
    let reason = |edit: &dyn Fn(&mut Witness), why: &str| {
        let (t, reasons) = timing(edit);
        assert_eq!(t, UNKNOWN, "{why}");
        assert!(reasons.contains(&why.to_string()), "{why}: {reasons:?}");
    };
    reason(&|w| w.g08 = None, "PRODUCER_WITNESS_WITHOUT_ITS_G08_RESULT");
    reason(
        &|w| w.g08_sha256 = [6; 32],
        "PRODUCER_WITNESS_OF_ANOTHER_G08_RESULT",
    );
    for field in 0..3 {
        reason(
            &|w| {
                let Some(Ok(g)) = w.g08.as_mut() else {
                    unreachable!()
                };
                match field {
                    0 => g.buy_tx = [9; 32],
                    1 => g.launch_tx = [9; 32],
                    _ => g.buy_block_hash = [9; 32],
                }
            },
            "G08_RESULT_OF_ANOTHER_EXECUTION",
        );
    }
}

/// Review L4, L5, L10: the report names the attempts before the
/// commitment, the planned and the selected count and why launches
/// were skipped; a Strategy C entry has no attempt verdict and counts
/// in no verdict counter.
#[test]
fn the_report_names_what_the_campaign_does_not_rate() {
    let (mut facts, mut j, w) = three(1);
    // One launch more, seen in full before its block, never attempted.
    j.record(
        RecordKind::Canonical,
        &rj::canonical_launch_payload(&Joined {
            launch: launch_ref(7, LAUNCH_BLOCK + 3),
            visibility: Visibility::FullBefore {
                source: SourceId::PublicNode,
                lead_us: 100,
            },
        }),
        RUN,
    );
    let strategy = Fact {
        component: ComponentId::StrategyC,
        ..fact(FactKind::Released, [0x99; 32], token(9))
    };
    facts.push((strategy, T0_MS));
    let p = plan(3);
    let a = evaluate_one(&facts, &j, Some(&p), &w);
    let s = a.iter().find(|a| a.kind == "STRATEGY_BUY").unwrap();
    assert_eq!(
        (s.verdicts.verdict, s.cell),
        (NOT_APPLICABLE, STRATEGY_ENTRY)
    );
    assert_eq!(
        s.counters,
        Counters {
            attempts: 4,
            acquired: 3,
            passed: 3,
            failed: 0,
            undecided: 0
        }
    );
    let late = Ok(precommit(T0_MS + 1));
    let v = rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&late));
    assert_eq!((v.before_precommit, v.written), (3, 0));
    let c = Ok(precommit(T0_MS - 10_000));
    let v = rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&c));
    assert_eq!((v.planned, v.observed, v.selected, v.skipped), (3, 4, 1, 1));
    assert_eq!(v.skip_reasons, ["FULL_BEFORE=1"]);
}

/// Review N1: a campaign run without a readable record (empty, its
/// reader refused) or whose segments are not its manifest's is no
/// covered run: the campaign stays open.
#[test]
fn a_run_without_readable_records_or_its_manifests_segments_is_no_coverage() {
    let open = |why: &str, edit: &dyn Fn(&mut Journaled)| {
        let c = green_with(|j, _, _| edit(j));
        assert_eq!(c.verdict, CAMPAIGN_OPEN, "{why}: {}", reasons(&c));
        assert!(reasons(&c).contains(why), "{why}: {}", reasons(&c));
    };
    open("RUN_EMPTY", &|j| {
        j.runs.push(Run {
            id: T0_MS + 30_000,
            manifest: Some(sealed()),
            manifest_match: Some(true),
            ..Run::default()
        });
    });
    open("JOURNAL_UNREADABLE", &|j| {
        j.runs.push(Run {
            id: T0_MS + 30_000,
            read_error: Some("no segment index".into()),
            ..Run::default()
        });
    });
    open("RUN_SEGMENTS_DIFFER_FROM_MANIFEST", &|j| {
        j.runs[0].manifest_match = Some(false);
    });
}

/// Review N3: heights no run followed (a restart) are no gap but time
/// no sealed run covers, bounded by the plan's coverage floor; a gap
/// while a run followed the chain stays one.
#[test]
fn a_restart_is_uncovered_time_and_a_gap_while_running_stays_open() {
    let split = |j: &mut Journaled| {
        // Run A to block 134 (every market window and the block after
        // it), down until 137, run B from 138.
        for n in 135..138 {
            j.blocks.remove(&(LAUNCH_BLOCK + n - 100));
        }
        let a_end = T0_MS + 34 * 450;
        let b_start = T0_MS + 38 * 450;
        j.runs[0].blocks = Some((LAUNCH_BLOCK, LAUNCH_BLOCK + 34));
        j.runs[0].last_ms = Some(a_end);
        j.runs[0].manifest.as_mut().unwrap().ended_ms = Some(a_end);
        j.runs.push(Run {
            id: b_start,
            manifest: Some(RunManifest {
                started_ms: b_start,
                ..sealed()
            }),
            manifest_match: Some(true),
            records: 2,
            last_ms: j.last_ms,
            blocks: Some((LAUNCH_BLOCK + 38, LAUNCH_BLOCK + 39)),
            ..Run::default()
        });
    };
    let c = green_with(|j, p, _| {
        split(j);
        p.min_covered_ppm = 0;
    });
    assert!(!reasons(&c).contains("CANONICAL_GAP"), "{}", reasons(&c));
    assert!(c.uncovered_s > 0 && c.covered_s > 0);
    // 26.2 of 28 s covered: below a floor of 95 %.
    let c = green_with(|j, p, _| {
        split(j);
        p.min_covered_ppm = 950_000;
    });
    assert_eq!(c.verdict, CAMPAIGN_FAIL, "{}", reasons(&c));
    assert!(reasons(&c).contains("COVERAGE_BELOW_FLOOR"));
}

/// Review N2: an earlier campaign's attempts (another run directory,
/// resolved) hold no new one: counted only; unresolved ones hold it
/// open.
#[test]
fn an_earlier_campaigns_attempts_hold_a_new_one_only_while_unresolved() {
    let (mut facts, mut j, w) = three(1);
    let p = plan(3);
    // A resolved attempt of an earlier campaign, journaled elsewhere.
    facts.extend(facts_of(
        5,
        Some(LAUNCH_BLOCK),
        1_000,
        Some(PlacementVerdict::First),
        T0_MS - 600_000,
    ));
    let a = evaluate_one(&facts, &j, Some(&p), &w);
    let c = Ok(precommit(T0_MS - 10_000));
    let v = rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&c));
    assert_eq!(
        (v.verdict, v.before_precommit),
        (CAMPAIGN_PASS, 1),
        "{}",
        reasons(&v)
    );
    // Unresolved, it may still execute.
    let mut pending = facts.clone();
    pending.retain(|(f, at)| !(*at < T0_MS - 10_000 && f.kind != FactKind::Released));
    let a = evaluate_one(&pending, &j, Some(&p), &w);
    let v = rate(&a, &pending, &j, &Ok(manifest_for(&p)), Some(&c));
    assert!(
        reasons(&v).contains("EARLIER_ATTEMPTS_UNRESOLVED: 1"),
        "{}",
        reasons(&v)
    );
    // Journaled in this run directory: its commitment came after it.
    j.releases.insert(
        buy_tx(5),
        Release {
            run: RUN,
            l: release(5, 0),
        },
    );
    let a = evaluate_one(&facts, &j, Some(&p), &w);
    let v = rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&c));
    assert_eq!(v.verdict, CAMPAIGN_NOT_READY);
}

/// Review LOW 1–5: the configuration's own accounts are self-launchers;
/// a witness of a G0.8 result other than PASS is none; a refused
/// release record of an earlier run is not the campaign's; an attempt
/// on an ineligible launch or through a part of the frozen builders
/// rates nothing.
#[test]
fn the_scope_checks_of_the_re_review_hold() {
    let (mut facts, j, w) = three(1);
    for (f, _) in &mut facts {
        if f.launcher.is_some() {
            f.launcher = Some([0x3E; 20]);
        }
    }
    let p = Plan {
        self_launchers: Vec::new(),
        ..plan(3)
    };
    let own = [[0x3E; 20]];
    let a = attempts(
        &facts,
        &j,
        &Inputs {
            window_ms: WINDOW_MS,
            plan: Some(&p),
            witnesses: &w,
            own_accounts: &own,
        },
    );
    assert!(a.iter().all(|a| a.cell == SELF_LAUNCH));
    let facts = facts_of(
        1,
        Some(LAUNCH_BLOCK),
        1_000,
        Some(PlacementVerdict::First),
        T0_MS,
    );
    let (id, mut wit) = witness(1, "PASS", 1);
    if let Ok(Witness {
        g08: Some(Ok(g)), ..
    }) = &mut wit
    {
        g.verdict = "ADAPT".into();
    }
    let a = evaluate_one(
        &facts,
        &journal(40, &[1]),
        Some(&plan(1)),
        &HashMap::from([(id, wit)]),
    );
    assert!(a[0].reasons.contains(&"G08_RESULT_NOT_PASS".to_string()));
    let c = green_with(|j, _, _| {
        j.releases_refused
            .push((RUN - 1, rj::LIVE_RELEASE_OTHER_LAYOUT));
        j.runs.insert(
            0,
            Run {
                id: RUN - 1,
                last_ms: Some(RUN),
                records: 1,
                ..Run::default()
            },
        );
    });
    assert!(
        !reasons(&c).contains("RELEASE_RECORD_REFUSED"),
        "{}",
        reasons(&c)
    );
    let c = green_with(|_, p, _| {
        p.adapters = vec![
            LiveAdapter::Club48BuilderV1,
            LiveAdapter::BlockRazorBuilderV1,
        ];
    });
    assert!(
        reasons(&c).contains("ATTEMPT_OUTSIDE_FROZEN_SCOPE"),
        "{}",
        reasons(&c)
    );
    let c = green_with(|j, _, _| {
        for l in &mut j.launches {
            if l.tx_hash == launch_tx(2) {
                l.c1_scope = false;
            }
        }
    });
    assert!(
        reasons(&c).contains("ATTEMPT_ON_AN_INELIGIBLE_LAUNCH"),
        "{}",
        reasons(&c)
    );
}

/// Review P1: coverage is the chain time a run followed: a sealed run
/// whose follower delivered no block, stopped long before the run
/// ended, or a later run starting after unfollowed heights holds an
/// OPEN campaign open (and is uncovered time for a BOUNDED one).
#[test]
fn coverage_is_the_chain_a_run_followed_not_its_wall_time() {
    // OPEN mode, no floor: what holds the campaign is the chain itself.
    let open = |why: &str, edit: &dyn Fn(&mut Journaled)| {
        let c = green_with(|j, p, _| {
            edit(j);
            p.min_covered_ppm = 0;
        });
        assert_eq!(c.verdict, CAMPAIGN_OPEN, "{why}: {}", reasons(&c));
        assert!(reasons(&c).contains(why), "{why}: {}", reasons(&c));
        c
    };
    // A whole run whose follower never delivered a block.
    open("RUN_FOLLOWED_NO_BLOCK", &|j| {
        j.runs.push(Run {
            id: T0_MS + 30_000,
            manifest: Some(sealed()),
            manifest_match: Some(true),
            records: 5,
            last_ms: Some(T0_MS + 3_600_000),
            ..Run::default()
        });
    });
    // The follower stopped nine hours before the run ended.
    let tail = |j: &mut Journaled| {
        let end = T0_MS + 9 * 3_600_000;
        j.runs[0].manifest.as_mut().unwrap().ended_ms = Some(end);
        j.runs[0].last_ms = Some(end);
        j.last_ms = Some(end);
    };
    open("RUN_FOLLOWER_STOPPED", &tail);
    // A later run whose follower began after heights no run followed.
    let head = |j: &mut Journaled| {
        j.runs.push(Run {
            id: T0_MS + 60_000,
            manifest: Some(RunManifest {
                started_ms: T0_MS + 60_000,
                ended_ms: Some(T0_MS + 70_000),
                ..sealed()
            }),
            manifest_match: Some(true),
            records: 1,
            last_ms: Some(T0_MS + 70_000),
            blocks: Some((LAUNCH_BLOCK + 150, LAUNCH_BLOCK + 150)),
            ..Run::default()
        });
        j.blocks.insert(
            LAUNCH_BLOCK + 150,
            (hash(LAUNCH_BLOCK + 150), T0_MS + 69_000),
        );
    };
    open("UNCOVERED_HEIGHTS: 110", &head);
    // BOUNDED: both are uncovered time, below the floor the campaign
    // fails.
    for edit in [&tail as &dyn Fn(&mut Journaled), &head] {
        let c = green_with(|j, p, _| {
            edit(j);
            p.uncovered_bounded = true;
        });
        assert_eq!(c.verdict, CAMPAIGN_FAIL, "{}", reasons(&c));
        assert!(
            reasons(&c).contains("COVERAGE_BELOW_FLOOR"),
            "{}",
            reasons(&c)
        );
    }
}

/// Review P2: the launches of the chain the follower caught up on
/// before a run's window (no run was up) are observed and skipped
/// (MISSED), so a restart within the catch-up keeps the chain whole.
#[test]
fn a_restarts_caught_up_launches_are_skips() {
    let c = green_with(|j, _, _| {
        let run_b = T0_MS + 50 * 450;
        // Run A to 134; run B caught up 135..=139 before its window.
        j.runs[0].blocks = Some((LAUNCH_BLOCK, LAUNCH_BLOCK + 34));
        for n in 135..140 {
            let b = LAUNCH_BLOCK + n - 100;
            j.blocks.remove(&b);
        }
        let mut b_run = Run {
            id: run_b,
            manifest: Some(RunManifest {
                started_ms: run_b,
                ..sealed()
            }),
            manifest_match: Some(true),
            ..Run::default()
        };
        for n in 135..140u64 {
            let b = LAUNCH_BLOCK + n - 100;
            let ms = T0_MS + (b - LAUNCH_BLOCK) * 450;
            let rec = rj::catchup_block_payload(b, &hash(b), ms);
            if let Some(h) = j.record(RecordKind::Runtime, &rec, run_b) {
                b_run.blocks = Some(b_run.blocks.map_or((h, h), |(x, y)| (x.min(h), y.max(h))));
            }
            b_run.records += 1;
        }
        let launch = crate::chain_follower::CanonicalLaunch {
            venue: Venue::Flap,
            block_number: LAUNCH_BLOCK + 36,
            block_hash: hash(LAUNCH_BLOCK + 36),
            tx_index: 0,
            tx_hash: [0x45; 32],
            token: Some(token(8)),
            creator: Some(CREATOR),
            external_buys_after: None,
            first_external_buy_index: None,
            sender: Some(CREATOR),
            nonce: Some(0),
            c1_scope: true,
            venue_target: true,
            four_token: FourToken::NotApplicable,
            unverifiable: None,
        };
        let rec = rj::catchup_launch_payload(&launch, T0_MS + 36 * 450);
        j.record(RecordKind::Runtime, &rec, run_b);
        b_run.last_ms = j.last_ms;
        j.runs.push(b_run);
    });
    assert!(
        !reasons(&c).contains("UNCOVERED_HEIGHTS"),
        "{}",
        reasons(&c)
    );
    assert_eq!((c.observed, c.skipped), (4, 1));
    assert_eq!(c.skip_reasons, ["MISSED=1"]);
    assert_eq!(c.verdict, CAMPAIGN_PASS, "{}", reasons(&c));
}

/// Review Q1: a market window reaching into the chain a later run
/// caught up on is complete with the catch-up's own trades (`d`), read
/// as the window's; a refused log of it (`e`) leaves it incomplete.
/// Review LOW 1: a run whose follower began long after it started is
/// named (OPEN).
#[test]
fn a_window_into_the_catch_up_uses_its_trades_and_a_late_follower_is_named() {
    let split = |j: &mut Journaled, refused: bool| {
        // Run A to block 120 (inside every window); run B caught up
        // 121..=139 with an external BUY in 125.
        let run_b = T0_MS + 60_000;
        j.runs[0].blocks = Some((LAUNCH_BLOCK, LAUNCH_BLOCK + 20));
        let a_end = T0_MS + 20 * 450;
        j.runs[0].last_ms = Some(a_end);
        j.runs[0].manifest.as_mut().unwrap().ended_ms = Some(a_end);
        let mut b_run = Run {
            id: run_b,
            manifest: Some(RunManifest {
                started_ms: run_b,
                ..sealed()
            }),
            manifest_match: Some(true),
            ..Run::default()
        };
        for n in 21..40u64 {
            let b = LAUNCH_BLOCK + n;
            j.blocks.remove(&b);
            let rec = rj::catchup_block_payload(b, &hash(b), T0_MS + n * 450);
            if let Some(h) = j.record(RecordKind::Runtime, &rec, run_b) {
                b_run.blocks = Some(b_run.blocks.map_or((h, h), |(x, y)| (x.min(h), y.max(h))));
            }
            b_run.records += 1;
        }
        let mut buy = trade(
            LAUNCH_BLOCK + 25,
            0,
            [0x99; 32],
            token(1),
            false,
            500,
            1_000_000,
        );
        buy[0] = b'd';
        j.record(RecordKind::Runtime, &buy, run_b);
        if refused {
            let mut gap = rj::trade_gap_payload(LAUNCH_BLOCK + 26, &hash(LAUNCH_BLOCK + 26), 0, 0);
            gap[0] = b'e';
            j.record(RecordKind::Runtime, &gap, run_b);
        }
        j.runs[0].manifest.as_mut().unwrap().ended_ms = Some(a_end);
        b_run.last_ms = Some(run_b);
        j.runs.push(b_run);
    };
    let (facts, mut j, w) = three(1);
    split(&mut j, false);
    let a = evaluate_one(&facts, &j, Some(&plan(3)), &w);
    let window = &a[0].window;
    assert_eq!(window.why_incomplete, None);
    assert_eq!(window.first_external.map(|f| f.1), Some(LAUNCH_BLOCK + 25));
    let (facts, mut j, w) = three(1);
    split(&mut j, true);
    let a = evaluate_one(&facts, &j, Some(&plan(3)), &w);
    assert_eq!(a[0].window.why_incomplete, Some("TRADE_LOG_REFUSED"));
    // The follower of a later run began a minute after its start.
    let c = green_with(|j, _, _| {
        j.runs.push(Run {
            id: T0_MS + 20_000,
            manifest: Some(RunManifest {
                started_ms: T0_MS + 20_000,
                ended_ms: Some(T0_MS + 90_000),
                ..sealed()
            }),
            manifest_match: Some(true),
            records: 1,
            last_ms: Some(T0_MS + 90_000),
            blocks: Some((LAUNCH_BLOCK + 40, LAUNCH_BLOCK + 40)),
            ..Run::default()
        });
        j.blocks
            .insert(LAUNCH_BLOCK + 40, (hash(LAUNCH_BLOCK + 40), T0_MS + 85_000));
    });
    assert!(reasons(&c).contains("RUN_FOLLOWER_LATE"), "{}", reasons(&c));
}

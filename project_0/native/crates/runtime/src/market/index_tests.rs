use super::super::truth::NativeEvidence;
use super::*;
use crate::analytics::journal::Provenance;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// A real `end_liquidation` receivership (v1, finalized `getTransaction` bytes).
const FIXTURE: &str = include_str!(
    "../../../../../docs/evidence/AUDIT_20261005/2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf.json"
);
const SLOT: u64 = 451_703_584;
const BT: i64 = 1_790_701_461;
const NS_U: u64 = 1_000_000_000;

fn program() -> Pubkey {
    Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET).unwrap()
}
fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}
fn fixture_signature() -> String {
    fixture()["transaction"]["signatures"][0]
        .as_str()
        .unwrap()
        .to_string()
}
/// The fixture moved to another identity (signature, slot, time, status).
fn variant(signature: &str, slot: u64, bt: i64, err: Value) -> Value {
    let mut tx = fixture();
    tx["transaction"]["signatures"][0] = json!(signature);
    tx["slot"] = json!(slot);
    tx["blockTime"] = json!(bt);
    tx["meta"]["err"] = err;
    tx
}
fn row(signature: &str, slot: u64, bt: i64, err: Value) -> Value {
    json!({"signature": signature, "slot": slot, "blockTime": bt, "err": err, "memo": null,
        "confirmationStatus": "finalized"})
}

/// The window `[BT - 100 s, BT + 100 s)` and its history: page 1 (head) `A` after the end, `E` at
/// the end second (fetched: coarse chain time), the real liquidation `F` and the failed `B`;
/// page 2 (at `B`) `C`, `D` before the start (the start is crossed).
struct History {
    a: String,
    e: String,
    f: String,
    b: String,
    c: String,
    d: String,
}
impl History {
    fn new() -> Self {
        let s = || Signature::new_unique().to_string();
        History {
            a: s(),
            e: s(),
            f: fixture_signature(),
            b: s(),
            c: s(),
            d: s(),
        }
    }
    fn failed() -> Value {
        json!({"InstructionError": [0, {"Custom": 1}]})
    }
    fn page(&self, before: Option<&str>) -> Value {
        match before {
            None => json!([
                row(&self.a, SLOT + 300, BT + 300, Value::Null),
                row(&self.e, SLOT + 100, BT + 100, Value::Null),
                row(&self.f, SLOT, BT, Value::Null),
                row(&self.b, SLOT - 5, BT - 5, Self::failed()),
            ]),
            Some(b) if b == self.b => json!([
                row(&self.c, SLOT - 200, BT - 200, Value::Null),
                row(&self.d, SLOT - 201, BT - 201, Value::Null),
            ]),
            Some(_) => json!([]),
        }
    }
    fn transaction(&self, signature: &str) -> Reply {
        if signature == self.f {
            Reply::Result(fixture())
        } else if signature == self.e {
            Reply::Result(variant(&self.e, SLOT + 100, BT + 100, Value::Null))
        } else if signature == self.b {
            Reply::Result(variant(&self.b, SLOT - 5, BT - 5, Self::failed()))
        } else {
            Reply::Result(Value::Null)
        }
    }
    fn answer(&self, method: &str, params: &Value) -> Reply {
        match method {
            "getSignaturesForAddress" => Reply::Result(self.page(params[1]["before"].as_str())),
            "getTransaction" => self.transaction(params[0].as_str().unwrap()),
            _ => Reply::Result(Value::Null),
        }
    }
}

enum Reply {
    Result(Value),
    Error(i64),
}

type Seen = Arc<Mutex<Vec<Value>>>;

/// Loopback JSON-RPC endpoint answering every request with `answer(method, params)`.
fn loopback(answer: impl Fn(&str, &Value) -> Reply + Send + 'static) -> (String, Seen) {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            log.lock().unwrap().push(request.clone());
            let body = match answer(request["method"].as_str().unwrap_or(""), &request["params"]) {
                Reply::Result(result) => {
                    json!({"jsonrpc": "2.0", "id": request["id"], "result": result})
                }
                Reply::Error(code) => json!({"jsonrpc": "2.0", "id": request["id"],
                    "error": {"code": code, "message": "fixture"}}),
            }
            .to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        }
    });
    (url, seen)
}

fn job(tag: &str) -> Job {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    Job {
        verification_spec_id: format!("index-{tag}"),
        output: std::env::temp_dir().join(format!("p0-index-{tag}-{stamp}")),
        start_ns: (BT as u64 - 100) * NS_U,
        end_ns: (BT as u64 + 100) * NS_U,
        program: darkbot_project0::protocol::PROGRAM_ID_MAINNET.into(),
        max_calls: 100,
        max_seconds: 60,
        max_bytes: 64 * 1024 * 1024,
        max_disk_bytes: 1024 * 1024 * 1024,
        max_ram_bytes: 64 * 1024 * 1024 * 1024,
        min_free_bytes: 0,
        rps: 1000,
        offline: false,
        cancelled: None,
    }
}

fn methods(seen: &Seen) -> Vec<String> {
    seen.lock()
        .unwrap()
        .iter()
        .map(|r| r["method"].as_str().unwrap().to_string())
        .collect()
}

/// The fixture's own events (the classifier's reading of the real transaction).
fn fixture_events() -> Vec<TruthEvent> {
    let tx = darkbot_solana::chain::decode_transaction(&fixture()).unwrap();
    truth::classify(&tx, &program(), Some(BT), None).unwrap()
}

/// A capture that received `F` live with its receivership row, and one live transaction the
/// index does not hold.
fn live_capture(h: &History) -> BotEvidence {
    let mut bot = BotEvidence::default();
    bot.live.insert((h.f.clone(), SLOT));
    bot.live_signatures.insert(h.f.clone());
    bot.live.insert(("bot-only".into(), 7));
    bot.live_signatures.insert("bot-only".into());
    for (rank, e) in fixture_events()
        .iter()
        .filter(|e| e.liquidation_type == LiquidationType::Receivership)
        .enumerate()
    {
        bot.event(
            &h.f,
            SLOT,
            BotEvent {
                source: BotSource::Live,
                index: rank as u32,
                kind: LiquidationType::Receivership,
                target: e.target.clone(),
                actor: e.liquidator.clone(),
                seized: e.seized_value_usd,
                repaid: e.repaid_value_usd,
                protocol_fee: e.protocol_fee_lamports,
            },
        );
    }
    bot.complete = true;
    bot
}

/// DEC-08: the INDEX pages the program's finalized history back over the window start through
/// the shared pager (second page at the first page's last signature), fetches every transaction
/// that may lie in the window (the end second too, never the rows outside), classifies with the
/// one classifier and compares with the capture: the window is INDEX_VERIFIED, the real
/// receivership MATCHes its live row, the failed transaction is in the window without events,
/// the capture's own live transaction the index lacks is BOT_ONLY — and nothing is promoted.
#[test]
fn dec08_index_pages_fetches_classifies_and_compares() {
    let h = Arc::new(History::new());
    let server = h.clone();
    let (url, seen) = loopback(move |m, p| server.answer(m, p));
    let j = job("verified");
    let bot = live_capture(&h);
    let r = run(&j, 10, Some(url), &bot).unwrap();
    assert_eq!(r.status, "INDEX_VERIFIED", "{r:?}");
    assert_eq!(r.reason, None);
    assert!(r.incomplete_reasons.is_empty());
    assert!(r.crossed_start && r.head_after_end);
    assert_eq!(
        (r.pages, r.indexed_signatures, r.in_window_transactions),
        (2, 6, 2)
    );
    assert_eq!((r.unresolved_transactions, r.unknown_transactions), (0, 0));
    let receiverships = fixture_events()
        .iter()
        .filter(|e| e.liquidation_type == LiquidationType::Receivership)
        .count() as u64;
    assert!(receiverships > 0);
    assert_eq!(r.qualified, [0, receiverships, 0, 0]);
    assert_eq!(r.observed, r.qualified);
    assert!(r.markers.iter().sum::<u64>() > 0, "the window's markers");
    assert_eq!(r.comparisons.len(), 1, "the liquidation transaction only");
    let c = &r.comparisons[0];
    assert_eq!(
        (c.transaction_status.as_str(), c.classification.as_str()),
        ("MATCH", "CLASSIFIED")
    );
    assert!(c.events.iter().all(|e| e.detection == "MATCH"
        && e.shadow_status == "SHADOW_NO_EVIDENCE"
        && e.matched.iter().any(|m| m.source == BotSource::Live)));
    assert_eq!(r.comparison_counts.get("MISSING_LIVE"), Some(&1), "B");
    assert_eq!(
        r.comparison_counts.get("MATCH"),
        Some(&(1 + receiverships)),
        "F and its events"
    );
    assert_eq!(r.bot_only, vec![("bot-only".to_string(), 7)]);
    assert_eq!(r.comparison_counts.get("BOT_ONLY_INDEX_MISSING"), Some(&1));
    assert!(!r.zero_liquidations_proven);
    assert!(!r.canonical_verified && !r.c0_e1_master_pass);
    // Two pages, the end second, F and B — never A, C or D (outside the window).
    assert_eq!((r.signature_calls, r.transaction_calls, r.calls), (2, 3, 5));
    let requests = seen.lock().unwrap().clone();
    assert_eq!(requests[1]["params"][1]["before"], json!(h.b));
    assert_eq!(requests[0]["params"][1]["limit"], json!(1000));
    assert_eq!(requests[0]["params"][1]["commitment"], json!("finalized"));
    let fetched: Vec<&str> = requests[2..]
        .iter()
        .map(|q| q["params"][0].as_str().unwrap())
        .collect();
    assert_eq!(fetched, [h.e.as_str(), h.f.as_str(), h.b.as_str()]);
    assert!(r.terminal().contains("CANONICAL_VERIFIED=false"));
    // Resume: every page and transaction is answered from the typed cache, without the network.
    let again = run(&j, 10, None, &bot).unwrap();
    assert_eq!(
        (again.status.as_str(), again.calls, again.signature_calls),
        ("INDEX_VERIFIED", 0, 0)
    );
    assert_eq!(
        (again.qualified, again.comparisons.clone()),
        (r.qualified, r.comparisons.clone())
    );
    assert_eq!(methods(&seen).len(), 5, "the resume read nothing");
    let results = std::fs::read_dir(j.output.join("results")).unwrap().count();
    assert_eq!(results, 2, "every attempt's result is kept");
    std::fs::remove_dir_all(&j.output).unwrap();
}

/// DEC-08: the page budget ends an attempt INDEX_PAGE_BUDGET (resumable, the held pages still
/// worked); the next attempt replays the cached page without counting it and verifies.
#[test]
fn dec08_index_page_budget_is_resumable() {
    let h = Arc::new(History::new());
    let server = h.clone();
    let (url, seen) = loopback(move |m, p| server.answer(m, p));
    let j = job("budget");
    let bot = live_capture(&h);
    let first = run(&j, 1, Some(url.clone()), &bot).unwrap();
    assert_eq!(
        (first.status.as_str(), first.reason.as_deref()),
        ("HISTORICAL_INCOMPLETE", Some("INDEX_PAGE_BUDGET"))
    );
    assert_eq!(
        first.incomplete_reasons,
        ["INDEX_PAGE_BUDGET", "INDEX_START_NOT_CROSSED"]
    );
    assert_eq!((first.signature_calls, first.transaction_calls), (1, 3));
    let second = run(&j, 1, Some(url), &bot).unwrap();
    assert_eq!(second.status, "INDEX_VERIFIED", "{second:?}");
    assert_eq!(
        (second.signature_calls, second.transaction_calls),
        (1, 0),
        "page 2 only"
    );
    assert_eq!(methods(&seen).len(), 5);
    std::fs::remove_dir_all(&j.output).unwrap();
}

/// DEC-08 (`job cancel`): a cancelled attempt ends HISTORICAL_INCOMPLETE `VERIFICATION_CANCELLED`
/// before any read, its result kept; the next attempt resumes and verifies.
#[test]
fn dec08_a_cancelled_index_attempt_is_resumable() {
    let h = Arc::new(History::new());
    let server = h.clone();
    let (url, seen) = loopback(move |m, p| server.answer(m, p));
    let mut j = job("cancelled");
    j.cancelled = Some(|| true);
    let bot = live_capture(&h);
    let r = run(&j, 10, Some(url.clone()), &bot).unwrap();
    assert_eq!(
        (r.status.as_str(), r.reason.as_deref(), r.calls),
        ("HISTORICAL_INCOMPLETE", Some("VERIFICATION_CANCELLED"), 0)
    );
    assert!(methods(&seen).is_empty());
    j.cancelled = None;
    assert_eq!(
        run(&j, 10, Some(url), &bot).unwrap().status,
        "INDEX_VERIFIED"
    );
    assert_eq!(
        std::fs::read_dir(j.output.join("results")).unwrap().count(),
        2
    );
    std::fs::remove_dir_all(&j.output).unwrap();
}

/// DEC-08 (pruning): a transaction the provider answers `null` or as pruned (-32009) stays
/// unresolved — HISTORICAL_INCOMPLETE with HISTORICAL_PAYLOAD_UNKNOWN, never zero — and is read
/// again by the next attempt (unresolved answers are not cached).
#[test]
fn dec08_index_pruned_transactions_stay_unresolved_until_resolved() {
    let h = Arc::new(History::new());
    let pruned = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let (server, gate) = (h.clone(), pruned.clone());
    let (url, _) = loopback(move |m, p| {
        let sig = p[0].as_str().unwrap_or("");
        if m == "getTransaction" && gate.load(std::sync::atomic::Ordering::SeqCst) {
            if sig == server.f {
                return Reply::Result(Value::Null);
            }
            if sig == server.b {
                return Reply::Error(-32009);
            }
        }
        server.answer(m, p)
    });
    let j = job("pruned");
    let bot = live_capture(&h);
    let first = run(&j, 10, Some(url.clone()), &bot).unwrap();
    assert_eq!(first.status, "HISTORICAL_INCOMPLETE");
    assert_eq!(first.reason, None);
    assert_eq!(first.unresolved_transactions, 2);
    assert_eq!(first.incomplete_reasons, ["HISTORICAL_PAYLOAD_UNKNOWN"]);
    assert_eq!(
        first.qualified, [0; 4],
        "nothing is qualified from a missing payload"
    );
    pruned.store(false, std::sync::atomic::Ordering::SeqCst);
    let second = run(&j, 10, Some(url), &bot).unwrap();
    assert_eq!(second.status, "INDEX_VERIFIED", "{second:?}");
    assert_eq!(
        (second.signature_calls, second.transaction_calls),
        (0, 2),
        "F and B again, E from the cache"
    );
    std::fs::remove_dir_all(&j.output).unwrap();
}

/// DEC-08: a history exhausted before the start is incomplete (resumable); an inconsistent one
/// (a rising slot) and a transaction that is not the indexed one end the attempt with their
/// reason — every attempt still writes its typed result.
#[test]
fn dec08_index_incomplete_histories_fail_closed_with_their_reason() {
    let h = Arc::new(History::new());
    let bot = live_capture(&h);
    let server = h.clone();
    let (url, _) = loopback(move |m, p| match (m, p[1]["before"].as_str()) {
        ("getSignaturesForAddress", Some(_)) => Reply::Result(json!([])),
        _ => server.answer(m, p),
    });
    let j = job("exhausted");
    let r = run(&j, 10, Some(url), &bot).unwrap();
    assert_eq!(
        (r.status.as_str(), r.reason.as_deref()),
        (
            "HISTORICAL_INCOMPLETE",
            Some("INDEX_HISTORY_EXHAUSTED_BEFORE_START")
        )
    );
    assert_eq!(r.transaction_calls, 3, "the held page is still worked");
    std::fs::remove_dir_all(&j.output).unwrap();

    let server = h.clone();
    let (url, _) = loopback(move |m, p| match (m, p[1]["before"].as_str()) {
        ("getSignaturesForAddress", Some(_)) => {
            Reply::Result(json!([row(&server.c, SLOT + 1, BT - 200, Value::Null)]))
        }
        _ => server.answer(m, p),
    });
    let j = job("order");
    let r = run(&j, 10, Some(url), &bot).unwrap();
    assert_eq!(r.reason.as_deref(), Some("INDEX_SLOT_ORDER"));
    assert_eq!(
        r.transaction_calls, 0,
        "an inconsistent history is not worked"
    );
    assert_eq!(
        std::fs::read_dir(j.output.join("results")).unwrap().count(),
        1
    );
    std::fs::remove_dir_all(&j.output).unwrap();

    let server = h.clone();
    let (url, _) = loopback(move |m, p| match m {
        "getTransaction" if p[0].as_str() == Some(server.f.as_str()) => {
            Reply::Result(variant(&server.f, SLOT + 1, BT, Value::Null))
        }
        _ => server.answer(m, p),
    });
    let j = job("identity");
    let r = run(&j, 10, Some(url), &bot).unwrap();
    assert_eq!(r.reason.as_deref(), Some("TRANSACTION_IDENTITY_MISMATCH"));
    assert_eq!(r.status, "HISTORICAL_INCOMPLETE");
    std::fs::remove_dir_all(&j.output).unwrap();

    // The transaction's status or time is not its index row's: the history is inconsistent.
    for (answer, reason) in [
        (
            variant(&h.f, SLOT, BT, History::failed()),
            "INDEX_TX_STATUS_MISMATCH",
        ),
        (
            variant(&h.f, SLOT, BT + 1, Value::Null),
            "INDEX_TX_TIME_MISMATCH",
        ),
    ] {
        let server = h.clone();
        let (url, _) = loopback(move |m, p| match m {
            "getTransaction" if p[0].as_str() == Some(server.f.as_str()) => {
                Reply::Result(answer.clone())
            }
            _ => server.answer(m, p),
        });
        let j = job(reason);
        let r = run(&j, 10, Some(url), &bot).unwrap();
        assert_eq!(r.reason.as_deref(), Some(reason));
        std::fs::remove_dir_all(&j.output).unwrap();
    }

    // An invalid job is refused before any read.
    let mut bad = job("config");
    bad.end_ns = bad.start_ns;
    assert_eq!(
        run(&bad, 10, None, &bot).unwrap_err(),
        "VERIFICATION_CONFIG_INVALID"
    );
    assert_eq!(
        run(&job("pages"), 0, None, &bot).unwrap_err(),
        "VERIFICATION_CONFIG_INVALID"
    );
}

fn truth_event(kind: LiquidationType, index: u32, target: &str, actor: &str) -> TruthEvent {
    TruthEvent {
        signature: "sig".into(),
        slot: 9,
        transaction_index: 0,
        event_index: index,
        liquidation_type: kind,
        target: target.into(),
        liquidator: actor.into(),
        liquidator_marginfi_account: None,
        asset_bank: None,
        liability_bank: None,
        asset_mint: None,
        liability_mint: None,
        seized_asset_native_exact: None,
        native_evidence: None,
        seized_value_usd: Some(105.),
        repaid_value_usd: Some(100.),
        gross_edge_usd: Some(5.),
        protocol_fee_lamports: Some(5000),
        transaction_fee_lamports: None,
        fee_payer: None,
        visible_payments: None,
        incremental_rent_lamports: None,
        priority_fee_lamports: None,
        winner_net_edge_usd: None,
        winner_net_edge_complete: false,
        economic_provenance: "FIXTURE".into(),
        block_time: Some(BT),
        live_t_rx_ns: None,
        discovery_source: "INDEX".into(),
        permissionless_profit_opportunity: false,
    }
}
fn bot_event(source: BotSource, index: u32, kind: LiquidationType, target: &str) -> BotEvent {
    BotEvent {
        source,
        index,
        kind,
        target: target.into(),
        actor: "actor".into(),
        seized: Some(105.),
        repaid: Some(100.),
        protocol_fee: Some(5000),
    }
}
fn indexed(events: Vec<TruthEvent>) -> IndexedTx {
    IndexedTx {
        signature: "sig".into(),
        slot: 9,
        block_time: Some(BT),
        membership: Membership::InWindow,
        boundary_second: false,
        classification: "CLASSIFIED",
        unknown_reason: None,
        events,
        program_events: [0; 4],
        markers: [0; 6],
    }
}

/// DEC-08 (`compare`, the pinned verifier's rules): canonical and payload rows match by the
/// all-kind event index, the observer's receivership rows by the receivership-only index; kind,
/// target/actor and amounts decide the status in that order; a live receivership without its
/// row is a missing record, another kind with only its payload is unproven; Shadow evidence is
/// exact only with the joined event's identity.
#[test]
fn dec08_compare_follows_the_pinned_matching_rules() {
    use LiquidationType::*;
    let tx = indexed(vec![
        truth_event(Classic, 0, "t0", "actor"),
        truth_event(Receivership, 1, "t1", "actor"),
        truth_event(ForcedDeleverage, 2, "t2", "actor"),
    ]);
    let mut bot = BotEvidence::default();
    bot.live.insert(("sig".into(), 9));
    bot.live_signatures.insert("sig".into());
    let mut payload_wrong_target = bot_event(BotSource::LivePayload, 1, Receivership, "x");
    payload_wrong_target.seized = None;
    let mut backfill_amount = bot_event(BotSource::Backfill, 0, Receivership, "t1");
    backfill_amount.repaid = Some(99.);
    for e in [
        bot_event(BotSource::Canonical, 0, Classic, "t0"),
        // The observer's first receivership row is the transaction's second event.
        bot_event(BotSource::Live, 0, Receivership, "t1"),
        payload_wrong_target,
        backfill_amount,
        bot_event(BotSource::Canonical, 2, Classic, "t2"),
        bot_event(BotSource::LivePayload, 2, ForcedDeleverage, "t2"),
    ] {
        bot.event("sig", 9, e);
    }
    // A repeated row of the same source, index and kind replaces the earlier one.
    bot.event("sig", 9, bot_event(BotSource::Canonical, 0, Classic, "t0"));
    bot.shadow
        .insert(("sig".into(), 9, "t1".into()), Some((1, Receivership)));
    // The classic's target joined to another kind at its index: no exact identity.
    bot.shadow
        .insert(("sig".into(), 9, "t0".into()), Some((0, Receivership)));
    bot.shadow.insert(("sig".into(), 9, "t2".into()), None);
    let c = compare(&tx, &bot);
    assert_eq!(c.transaction_status, "MATCH");
    let status = |e: &EventComparison, s: BotSource| {
        e.matched
            .iter()
            .find(|m| m.source == s)
            .map(|m| m.status.clone())
    };
    let [classic, receivership, deleverage] = &c.events[..] else {
        panic!("{c:?}")
    };
    assert_eq!(
        classic.matched.len(),
        1,
        "one canonical row (replaced, not doubled)"
    );
    assert_eq!(
        status(classic, BotSource::Canonical).as_deref(),
        Some("MATCH")
    );
    assert_eq!(
        classic.detection, "DETECTION_UNPROVEN",
        "a live classic without a payload"
    );
    assert_eq!(classic.shadow_status, "SHADOW_IDENTITY_UNKNOWN");
    assert_eq!(
        status(receivership, BotSource::Live).as_deref(),
        Some("MATCH")
    );
    assert_eq!(
        status(receivership, BotSource::LivePayload).as_deref(),
        Some("TARGET_OR_ACTOR_MISMATCH")
    );
    let payload = receivership
        .matched
        .iter()
        .find(|m| m.source == BotSource::LivePayload)
        .unwrap();
    assert_eq!(payload.unknown_amounts, ["seized_value_usd"]);
    let backfill = receivership
        .matched
        .iter()
        .find(|m| m.source == BotSource::Backfill)
        .unwrap();
    assert_eq!(
        (backfill.status.as_str(), backfill.different_amounts.clone()),
        ("AMOUNT_MISMATCH", vec!["repaid_value_usd".to_string()])
    );
    assert_eq!(receivership.detection, "MATCH", "the live row decides");
    assert_eq!(receivership.shadow_status, "SHADOW_MATCH");
    assert_eq!(
        status(deleverage, BotSource::Canonical).as_deref(),
        Some("TYPE_MISMATCH")
    );
    assert_eq!(
        deleverage.detection,
        "LIVE_PAYLOAD_PRESENT_DETECTION_UNPROVEN"
    );
    assert_eq!(deleverage.shadow_status, "SHADOW_IDENTITY_UNKNOWN");

    // A live transaction without the receivership row; a payload carries no native amounts.
    let mut bot = BotEvidence::default();
    bot.live.insert(("sig".into(), 9));
    let mut classic = truth_event(Classic, 0, "t0", "actor");
    classic.native_evidence = Some(NativeEvidence::Flows {
        scope: "s".into(),
        flows: vec![],
        units: "u".into(),
    });
    bot.event(
        "sig",
        9,
        bot_event(BotSource::LivePayload, 0, Classic, "t0"),
    );
    let c = compare(
        &indexed(vec![classic, truth_event(Receivership, 1, "t1", "actor")]),
        &bot,
    );
    assert_eq!(c.events[0].matched[0].status, "MATCH");
    assert!(c.events[0].matched[0]
        .unknown_amounts
        .contains(&"native_amount_semantics".to_string()));
    assert_eq!(
        c.events[1].detection,
        "MISSING_LIQUIDATION_RECORD_WITH_LIVE_TX"
    );

    // The same signature at another slot.
    let mut bot = BotEvidence::default();
    bot.live_signatures.insert("sig".into());
    assert_eq!(
        compare(&indexed(vec![]), &bot).transaction_status,
        "SLOT_MISMATCH"
    );
}

/// DEC-08 (`coverage_at`): an event the capture missed is a miss within the observed slot
/// interval, during a proven outage (a counted gap, acknowledged by a reconnect, closed by the
/// next slot boundary; or still open), at an unknown boundary (the second, the window edge, a
/// gap edge) or outside the proven live coverage.
#[test]
fn dec08_missed_events_are_attributed_by_the_live_coverage() {
    let s = |sec: i64| i128::from(sec) * NS;
    let mut cov = Coverage::default();
    cov.slot_boundary(s(BT - 50), 1);
    cov.gap(s(BT + 10));
    cov.slot_boundary(s(BT + 15), 2);
    cov.reconnected(s(BT + 20));
    cov.gap(s(BT + 21)); // a later gap before the boundary clears the acknowledgement
    cov.slot_boundary(s(BT + 22), 3);
    cov.reconnected(s(BT + 25));
    cov.slot_boundary(s(BT + 30), 4);
    cov.slot_boundary(s(BT + 50), 5);
    assert_eq!(
        (cov.first_live_ns, cov.last_live_ns, cov.last_live_slot),
        (Some(s(BT - 50)), Some(s(BT + 50)), Some(5))
    );
    assert_eq!(cov.gap_open, None);
    assert_eq!(
        cov.gaps,
        vec![(s(BT + 10), s(BT + 30))],
        "the later gap voided the first reconnect: one outage until the second"
    );
    assert_eq!(cov.at(Some(BT), false), "WITHIN_OBSERVED_SLOT_INTERVAL");
    assert_eq!(
        cov.at(Some(BT + 20), false),
        "PROVEN_WEBSOCKET_OUTAGE_INTERVAL"
    );
    assert_eq!(
        cov.at(Some(BT + 10), false),
        "TIME_BOUNDARY_UNKNOWN",
        "the gap edge"
    );
    assert_eq!(cov.at(Some(BT), true), "TIME_BOUNDARY_UNKNOWN");
    assert_eq!(cov.at(None, false), "TIME_BOUNDARY_UNKNOWN");
    assert_eq!(cov.at(Some(BT + 49), false), "LIVE_COVERAGE_UNPROVEN");
    assert_eq!(cov.at(Some(BT - 60), false), "LIVE_COVERAGE_UNPROVEN");
    cov.gap(s(BT + 60));
    assert_eq!(
        cov.at(Some(BT + 62), false),
        "PROVEN_WEBSOCKET_OUTAGE_INTERVAL"
    );

    // The detection of a liquidation the capture did not receive follows its coverage.
    let tx = indexed(vec![truth_event(
        LiquidationType::Receivership,
        0,
        "t",
        "a",
    )]);
    let mut bot = BotEvidence {
        coverage: cov.clone(),
        ..Default::default()
    };
    bot.coverage.gap_open = None;
    assert_eq!(
        compare(&tx, &bot).events[0].detection,
        "MISSING_LIQUIDATION_WITHIN_OBSERVED_INTERVAL"
    );
    let mut outage = indexed(vec![truth_event(
        LiquidationType::Receivership,
        0,
        "t",
        "a",
    )]);
    outage.block_time = Some(BT + 20);
    assert_eq!(
        compare(&outage, &bot).events[0].detection,
        "MISSING_DURING_OUTAGE"
    );
    bot.coverage = Coverage::default();
    let c = compare(&tx, &bot);
    assert_eq!(
        (
            c.transaction_status.as_str(),
            c.events[0].detection.as_str()
        ),
        ("MISSING_LIVE", "MISSING_LIVE_LIQUIDATION_COVERAGE_UNPROVEN")
    );
}

fn write_frames<P: Payload>(w: &mut journal::Writer, source: u32, rows: &[P]) {
    for r in rows {
        w.append_typed(
            Provenance {
                source,
                ..Default::default()
            },
            r,
        )
        .unwrap();
    }
}

/// DEC-08: the capture side of a native run is read from its typed records — live rows (with
/// their payload events; a failed payload is live without events, a recovered block is not
/// live), the observer's receivership rows (live or recovered; canonical truth rows are not a
/// row identity), the coverage (a counted gap, a reconnect, slot boundaries; an uncounted gap is
/// ignored), the ECON_CANONICAL joins (canonical events, exact Shadow only with an evaluation)
/// and the ECON_OPPORTUNITY rows (Shadow without identity, replacing a join of the same key).
#[test]
fn dec08_the_capture_side_is_imported_from_the_typed_run() {
    use super::super::rows::{
        MarketCoverage, MarketLiquidation, MarketTransaction, ReceiverRow, TxOrigin, WindowEvent,
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let run_dir = std::env::temp_dir().join(format!("p0-index-run-{stamp}"));
    assert_eq!(
        BotEvidence::from_run(&run_dir, &program()).unwrap_err(),
        "RUN_ARTIFACT_MISSING"
    );
    let paths = crate::run_layout::RunPaths::new(&run_dir, crate::run_layout::RunLayout::V2);
    std::fs::create_dir_all(paths.market_dir()).unwrap();
    std::fs::create_dir_all(paths.snapshot("")).unwrap();
    let f = fixture_signature();
    let tx = darkbot_solana::chain::decode_transaction(&fixture()).unwrap();
    let events = fixture_events();
    let logs = tx.meta.as_ref().unwrap().log_messages.clone().unwrap();
    let receiver = decode_receiver_events(&logs, &program()).events[0].clone();
    let mut w =
        journal::Writer::create(&paths.market_dir().join(super::super::MARKET_JOURNAL)).unwrap();
    write_frames(
        &mut w,
        source::MARKET,
        &[
            MarketTransaction {
                signature: f.clone(),
                slot: SLOT,
                origin: TxOrigin::TransactionSubscribe {
                    observed_unix_s: BT as u64,
                    t_rx_ns: 1,
                    tx: Ok(tx.clone()),
                },
            },
            MarketTransaction {
                signature: "failed-live".into(),
                slot: 3,
                origin: TxOrigin::LogsSubscribe {
                    observed_unix_s: BT as u64,
                    t_rx_ns: 2,
                    logs: darkbot_solana::chain::ChainLogs {
                        slot: 3,
                        signature: "failed-live".into(),
                        status: TxStatus::Failed(
                            solana_sdk::transaction::TransactionError::AccountInUse,
                        ),
                        logs: Some(logs.clone()),
                    },
                },
            },
            MarketTransaction {
                signature: "recovered".into(),
                slot: 4,
                origin: TxOrigin::GapBackfill {
                    transaction_index: 0,
                    tx: tx.clone(),
                },
            },
            MarketTransaction {
                signature: "truncated-live".into(),
                slot: 5,
                origin: TxOrigin::LogsSubscribe {
                    observed_unix_s: BT as u64,
                    t_rx_ns: 3,
                    logs: darkbot_solana::chain::ChainLogs {
                        slot: 5,
                        signature: "truncated-live".into(),
                        status: TxStatus::Success,
                        logs: Some({
                            let mut cut = logs.clone();
                            cut.push("Log truncated".into());
                            cut
                        }),
                    },
                },
            },
        ],
    );
    let liquidation = |t_rx_ns| {
        MarketLiquidation::Receiver(Box::new(ReceiverRow {
            signature: f.clone(),
            slot: SLOT,
            t_rx_ns,
            observed_unix_s: BT as u64,
            event: receiver.clone(),
            enrichment: Default::default(),
            enrichment_status: "COMPLETE".into(),
        }))
    };
    write_frames(
        &mut w,
        source::MARKET,
        &[
            liquidation(Some(5)),
            liquidation(None),
            MarketLiquidation::Canonical(Box::new(events[0].clone())),
        ],
    );
    let window = |s: i64, counted, event| MarketCoverage::Window {
        observed_unix_s: s as u64,
        t_rx_ns: None,
        observing: true,
        counted,
        event,
    };
    let boundary = |s: i64, slot| MarketCoverage::SlotBoundary {
        slot,
        t_rx_ns: 0,
        observed_unix_s: s as u64,
        start: None,
        end: None,
    };
    let gap = || WindowEvent::Gap {
        from_slot: None,
        to_slot: None,
    };
    write_frames(
        &mut w,
        source::MARKET,
        &[
            boundary(BT - 50, 10),
            window(BT - 40, false, gap()),
            window(BT - 30, true, gap()),
            window(BT - 29, true, WindowEvent::Reconnect { attempt: 1 }),
            boundary(BT - 20, 11),
            boundary(BT + 50, 12),
        ],
    );
    w.seal(true).unwrap();

    // The run's derived rows: a join with its evaluation, one without, and an opportunity of
    // the first join's target with a unique winner (it replaces the exact Shadow evidence).
    let mut reports =
        journal::Writer::create(&paths.snapshot(crate::analytics::econ::REPORT_JOURNAL)).unwrap();
    let rec = crate::analytics::econ::tests::rec();
    let joined = |market: TruthEvent, our| crate::analytics::econ::CanonicalJoin {
        market,
        target_seen: false,
        target_seen_without_slot: false,
        health_liquidatable: None,
        candidates_created: None,
        plan_validated: None,
        built: None,
        identity_status: "TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT",
        our,
        diagnostic: None,
        market_commitment: "finalized".into(),
        winner_slot_first_seen_ns: None,
        capital: vec![],
    };
    let mut other = events[0].clone();
    other.signature = "other".into();
    other.target = "other-target".into();
    write_frames(
        &mut reports,
        source::ECON,
        &[
            joined(events[0].clone(), Some(rec.clone())),
            joined(other.clone(), None),
        ],
    );
    let opportunity = |account: &str, market: bool| {
        let mut r = rec.clone();
        r.account = account.to_string();
        crate::analytics::econ::OpportunityRow {
            rec: r,
            winner: Some(crate::market::winner::Winner {
                slot: SLOT,
                identity: crate::market::winner::WinnerIdentity::Unique {
                    signature: f.clone(),
                    receiver: receiver.receiver.to_string(),
                    tx_version: "V1".into(),
                },
            }),
            market: market.then(|| crate::analytics::econ::MarketEvent {
                signature: f.clone(),
                slot: SLOT,
                t_rx_ns: Some(5),
                source: crate::analytics::econ::MarketEvidenceSource::Live,
                transaction_index: None,
                event: receiver.clone(),
            }),
            slot_seen: None,
            capital: vec![],
            market_commitment: "finalized".into(),
        }
    };
    // A winner alone is no key (the pinned rows nest its signature); a strongly joined market
    // event is, and it replaces the exact join of the same key.
    write_frames(
        &mut reports,
        source::ECON,
        &[
            opportunity("winner-only", false),
            opportunity(&events[0].target, true),
            crate::analytics::econ::OpportunityRow {
                winner: None,
                ..opportunity("weak-join", true)
            },
        ],
    );
    reports.seal(true).unwrap();

    let bot = BotEvidence::from_run(&run_dir, &program()).unwrap();
    assert!(bot.complete);
    assert!(bot.live.contains(&(f.clone(), SLOT)));
    assert!(bot.live.contains(&("failed-live".to_string(), 3)));
    assert!(!bot.live.contains(&("recovered".to_string(), 4)));
    assert!(!bot.events.contains_key(&("failed-live".to_string(), 3)));
    let mine = &bot.events[&(f.clone(), SLOT)];
    let count = |s: BotSource| mine.iter().filter(|e| e.source == s).count();
    let payload = decode_receiver_events(&logs, &program()).sequence.len();
    assert!(payload > 0);
    assert_eq!(count(BotSource::LivePayload), payload);
    assert_eq!((count(BotSource::Live), count(BotSource::Backfill)), (1, 1));
    assert_eq!(
        count(BotSource::Canonical),
        1,
        "the join's event, not the truth row"
    );
    assert_eq!(bot.events[&("other".to_string(), SLOT)].len(), 1);
    assert_eq!(
        bot.shadow.get(&(f.clone(), SLOT, events[0].target.clone())),
        Some(&None),
        "the opportunity row replaced the exact join"
    );
    assert!(!bot
        .shadow
        .contains_key(&("other".to_string(), SLOT, other.target.clone())));
    assert!(!bot
        .shadow
        .contains_key(&(f.clone(), SLOT, "winner-only".to_string())));
    assert!(
        !bot.shadow
            .contains_key(&(f.clone(), SLOT, "weak-join".to_string())),
        "a market event without a strong join (no unique winner) is no key"
    );
    assert!(bot.live.contains(&("truncated-live".to_string(), 5)));
    assert!(
        !bot.events.contains_key(&("truncated-live".to_string(), 5)),
        "a truncated live payload carries no events"
    );
    let s = |sec: i64| i128::from(sec) * NS;
    assert_eq!(bot.coverage.gaps, vec![(s(BT - 30), s(BT - 20))]);
    assert_eq!(
        (bot.coverage.first_live_ns, bot.coverage.last_live_slot),
        (Some(s(BT - 50)), Some(12))
    );
    std::fs::remove_dir_all(&run_dir).unwrap();
}

/// U-8 (PERSIST-04): the binary layouts of `INDEX_RESULT` (319) and `CROSSCHECK_RESULT` (321)
/// are pinned. A failing pin is a
/// layout change: bump `SCHEMA` and renew the pin — never silently.
#[test]
fn index_result_layout_is_pinned() {
    let mut counts = BTreeMap::new();
    counts.insert("MATCH".to_string(), 2);
    let r = IndexResult {
        schema: SCHEMA.into(),
        verification_spec_id: "spec".into(),
        status: "HISTORICAL_INCOMPLETE".into(),
        reason: Some("INDEX_PAGE_BUDGET".into()),
        incomplete_reasons: vec!["INDEX_PAGE_BUDGET".into()],
        window_start_ns: 1,
        window_end_ns: 2,
        crossed_start: false,
        head_after_end: true,
        pages: 3,
        indexed_signatures: 4,
        in_window_transactions: 5,
        unresolved_transactions: 6,
        unknown_transactions: 7,
        qualified: [1, 2, 3, 4],
        observed: [5, 6, 7, 8],
        markers: [1, 2, 3, 4, 5, 6],
        comparison_counts: counts,
        comparisons: vec![TxComparison {
            signature: "s".into(),
            slot: 9,
            transaction_status: "MATCH".into(),
            live_coverage: "WITHIN_OBSERVED_SLOT_INTERVAL".into(),
            classification: "CLASSIFIED".into(),
            events: vec![EventComparison {
                event_index: 0,
                kind: LiquidationType::Receivership,
                target: "t".into(),
                actor: "a".into(),
                detection: "MATCH".into(),
                matched: vec![SourceMatch {
                    source: BotSource::Live,
                    status: "MATCH".into(),
                    unknown_amounts: vec![],
                    different_amounts: vec![],
                }],
                shadow_status: "SHADOW_NO_EVIDENCE".into(),
            }],
        }],
        bot_only: vec![("b".into(), 1)],
        bot_import_complete: true,
        zero_liquidations_proven: false,
        first_live_ns: Some(1),
        last_live_ns: Some(2),
        last_live_slot: Some(3),
        open_gap_start_ns: None,
        calls: 10,
        bytes: 11,
        signature_calls: 4,
        transaction_calls: 6,
        max_pages: 50,
        canonical_verified: false,
        c0_e1_master_pass: false,
    };
    let crosscheck = CrosscheckResult {
        schema: CROSSCHECK_SCHEMA.into(),
        verification_spec_id: "spec".into(),
        status: "HISTORICAL_INCOMPLETE".into(),
        reason: Some("CROSSCHECK_DISAGREEMENT".into()),
        incomplete_reasons: vec!["CROSSCHECK_DISAGREEMENT".into()],
        primary: r.clone(),
        secondary: r.clone(),
        disagreements: vec![Disagreement {
            signature: "s".into(),
            slot: 9,
            field: "EVENTS".into(),
        }],
        canonical_verified: false,
        c0_e1_master_pass: false,
    };
    let bytes = payload::encode(&crosscheck).unwrap();
    let back: CrosscheckResult =
        payload::decode(CrosscheckResult::KIND, CrosscheckResult::SCHEMA, &bytes).unwrap();
    assert_eq!(back, crosscheck);
    assert_eq!(
        (
            CrosscheckResult::KIND,
            CrosscheckResult::SCHEMA,
            solana_sdk::hash::hash(&bytes).to_string().as_str()
        ),
        (321, 2, "CJ9mtmzkeukyFYpzvWLK1zjKTLCkun8UkjNrJVEAQJQv")
    );
    let bytes = payload::encode(&r).unwrap();
    let back: IndexResult =
        payload::decode(IndexResult::KIND, IndexResult::SCHEMA, &bytes).unwrap();
    assert_eq!(back, r);
    let hash = solana_sdk::hash::hash(&bytes).to_string();
    assert_eq!(
        (IndexResult::KIND, IndexResult::SCHEMA, hash.as_str()),
        (319, 2, "9nuochqhzfDT8ah9fzg2cjCYJYFaFfkoHi9NZtuneFDq")
    );
}

/// DEC-08 end to end through the one driver (offline, no network): a V2 run with its manifest,
/// CAPTURE window and market journal; the first INDEX attempt has no checkpoint
/// (`OFFLINE_CACHE_MISS`, HISTORICAL_INCOMPLETE, its result bound to the attempt); the typed
/// cache is the checkpoint — the resumed attempt (the CLI, `--mode index`) answers every read
/// from it and verifies the window; the spec is the `index` mode's, nothing is canonical.
#[test]
fn dec08_the_driver_runs_and_resumes_the_index() {
    use super::super::strict_driver::{self, DriverRequest, StrictBudget, VerifyMode};
    let (d, root, f, live, start_s, end_s) = driver_run(line!());
    let budget = StrictBudget {
        rps: 100,
        max_calls: 10,
        max_seconds: 30,
        max_bytes: 1 << 26,
        max_disk_bytes: 1 << 26,
        max_ram_bytes: 1 << 36,
        min_free_bytes: 1,
    };
    let first = strict_driver::drive(&DriverRequest {
        run: root.clone(),
        budget: budget.clone(),
        offline: true,
        resume: false,
        mode: VerifyMode::Index { max_pages: 5 },
        provider_evidence: None,
        cancelled: None,
    })
    .unwrap();
    assert_eq!(
        (first.status.as_str(), first.reason.as_deref()),
        ("HISTORICAL_INCOMPLETE", Some("OFFLINE_CACHE_MISS"))
    );
    assert!(first.verification_result_id.is_some());
    let out = root
        .canonicalize()
        .unwrap()
        .join("verification")
        .join(format!(
            "{}{}",
            strict_driver::NATIVE_PREFIX,
            first.verification_spec_id
        ));
    let spec: strict_driver::VerificationSpec =
        journal::read_typed_value(&out.join("spec.p0j")).unwrap();
    assert_eq!(spec.mode, "index");
    // The checkpoint: the head page (after the end, the liquidation), the page at its cursor
    // (before the start) and the transaction.
    let index = out.join("index");
    checkpoint(&index, &f, &live, start_s, end_s);
    let args: Vec<String> = [
        "--run",
        root.to_str().unwrap(),
        "--mode",
        "index",
        "--offline",
        "--resume-verification",
        "--verify-max-pages",
        "5",
        "--verify-rps",
        "100",
        "--verify-calls",
        "10",
        "--verify-seconds",
        "30",
        "--verify-bytes",
        &(1u64 << 26).to_string(),
        "--verify-disk-bytes",
        &(1u64 << 26).to_string(),
        "--verify-ram-bytes",
        &(1u64 << 36).to_string(),
        "--verify-min-free-bytes",
        "1",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let (text, verified) = strict_driver::cli(&args).unwrap();
    assert!(verified, "{text}");
    assert!(text.contains("status=INDEX_VERIFIED reason=none"), "{text}");
    assert!(text.contains("CANONICAL_VERIFIED=false"), "{text}");
    assert!(text.contains(&format!(
        "verification_spec_id={}",
        first.verification_spec_id
    )));
    let results: Vec<IndexResult> = std::fs::read_dir(index.join("results"))
        .unwrap()
        .map(|e| journal::read_typed_value(&e.unwrap().path()).unwrap())
        .collect();
    assert_eq!(results.len(), 2);
    let verified = results
        .iter()
        .find(|r| r.status == "INDEX_VERIFIED")
        .unwrap();
    assert_eq!(
        (
            verified.calls,
            verified.max_pages,
            verified.in_window_transactions
        ),
        (0, 5, 1)
    );
    assert_eq!(
        verified.comparison_counts.get("MATCH").copied(),
        Some(2),
        "tx + event"
    );
    assert_eq!(std::fs::read_dir(out.join("attempts")).unwrap().count(), 2);
    no_promotion(&root, &index.join("results"));
    // DEC-08: the Capture/Analytics state of the method, as its records hold it.
    let methods = super::super::verification_view::methods(&root);
    let [m] = &methods[..] else {
        panic!("{methods:?}")
    };
    assert_eq!(
        (m.method.as_str(), m.status.as_str(), m.attempts, m.running),
        ("index", "INDEX_VERIFIED", 2, false)
    );
    assert_eq!(
        (
            m.offline,
            m.provider_binding.as_ref(),
            m.calls,
            m.checkpoint_reads
        ),
        (true, None, Some(0), 3)
    );
    assert_eq!(
        m.qualified[1],
        ("RECEIVERSHIP".to_string(), verified.qualified[1])
    );
    assert_eq!(m.markers.len(), 6);
    // Event detections and transaction statuses apart (the result counts both under MATCH).
    assert_eq!(
        m.comparison,
        vec![("MATCH".to_string(), 1)],
        "the liquidation event"
    );
    assert_eq!(
        m.transactions,
        vec![("MATCH".to_string(), 1)],
        "its live transaction"
    );
    assert_eq!(
        verified.comparison_counts.get("MATCH").copied(),
        Some(2),
        "the result's own count is both"
    );
    assert_eq!((m.calls_total, m.bytes_total), (Some(0), Some(0)));
    // A started attempt that has not ended is RUNNING (its counts stay the newest result's).
    let running = out.join("attempts").join("z-running");
    std::fs::create_dir_all(&running).unwrap();
    let mut start: strict_driver::AttemptRecord = journal::read_typed_value(
        &std::fs::read_dir(out.join("attempts"))
            .unwrap()
            .flatten()
            .find(|e| e.path() != running)
            .unwrap()
            .path()
            .join("start.p0j"),
    )
    .unwrap();
    start.at_unix_ns = u64::MAX;
    journal::write_typed_value(&running.join("start.p0j"), source::STRICT, &start).unwrap();
    let methods = super::super::verification_view::methods(&root);
    assert_eq!(
        (
            methods[0].status.as_str(),
            methods[0].running,
            methods[0].attempts
        ),
        ("INTERRUPTED", false, 3),
        "no attempt holds the spec's lock: it ended without its record"
    );
    // While the driver holds the spec's writer lock the attempt is RUNNING.
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(out.join("writer.lock"))
        .unwrap();
    use std::os::fd::AsRawFd;
    // SAFETY: flock on the owned descriptor of the test's lock file.
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let methods = super::super::verification_view::methods(&root);
    assert_eq!(
        (methods[0].status.as_str(), methods[0].running),
        ("RUNNING", true)
    );
    assert_eq!(methods[0].qualified, m.qualified);
    drop(lock);
    let _ = std::fs::remove_dir_all(&d);
}

/// DEC-08 (no promotion): the report's verification is the strict result only — a verified
/// INDEX or CROSSCHECK result is never selected, and naming one is refused (the master gate's
/// own refusal of INDEX_VERIFIED/CROSSCHECK_VERIFIED is `econ_tests`' qualification test).
fn no_promotion(run: &std::path::Path, results: &std::path::Path) {
    let binding = super::super::strict_driver::bind_capture(run).unwrap();
    assert!(crate::analytics::report::select_result(&binding, None)
        .unwrap()
        .is_none());
    for e in std::fs::read_dir(results).unwrap() {
        let name = e.unwrap().file_name().into_string().unwrap();
        let id = name.strip_suffix(".p0j").unwrap();
        assert_eq!(
            crate::analytics::report::select_result(&binding, Some(id)).err(),
            Some("RESULT_NOT_FOUND_OR_AMBIGUOUS".to_string())
        );
    }
}

/// A V2 run with its manifest, CAPTURE window and a market journal that received the real
/// liquidation `f` live (moved into the window) with its receivership row: (dir, run, f, the
/// transaction's wire form, window start/end seconds).
fn driver_run(
    tag: u32,
) -> (
    std::path::PathBuf,
    std::path::PathBuf,
    String,
    Value,
    i64,
    i64,
) {
    use crate::analytics::measurement::{
        journal as phase, persist_manifest, MeasurementPlan, Phase,
    };
    use crate::run_layout::{RunLayout, RunPaths};
    let d = std::env::temp_dir().join(format!("p0_index_drv_{}_{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&d);
    let root = d.join("run_i");
    let paths = RunPaths::new(&root, RunLayout::V2);
    paths.create().unwrap();
    let plan = MeasurementPlan::new(60).unwrap();
    let clock = darkbot_common::timing::Clock::new();
    persist_manifest(&paths, "run_i", "test", &plan, clock).unwrap();
    let window = plan.capture_window(clock, 1_000 * NS_U).unwrap();
    let start_s = (window.capture_start_unix_ns / u128::from(NS_U)) as i64;
    let end_s = (window.capture_cutoff_unix_ns / u128::from(NS_U)) as i64;
    phase(&paths, clock, Phase::Capture(window)).unwrap();
    // The capture received the liquidation live (the fixture inside this window).
    let f = fixture_signature();
    let live = variant(&f, 1_000, start_s + 10, Value::Null);
    let tx = darkbot_solana::chain::decode_transaction(&live).unwrap();
    let logs = tx.meta.as_ref().unwrap().log_messages.clone().unwrap();
    let receiver = decode_receiver_events(&logs, &program()).events[0].clone();
    std::fs::create_dir_all(paths.market_dir()).unwrap();
    let mut w =
        journal::Writer::create(&paths.market_dir().join(super::super::MARKET_JOURNAL)).unwrap();
    write_frames(
        &mut w,
        source::MARKET,
        &[super::super::rows::MarketTransaction {
            signature: f.clone(),
            slot: 1_000,
            origin: super::super::rows::TxOrigin::TransactionSubscribe {
                observed_unix_s: (start_s + 10) as u64,
                t_rx_ns: 1,
                tx: Ok(tx),
            },
        }],
    );
    write_frames(
        &mut w,
        source::MARKET,
        &[super::super::rows::MarketLiquidation::Receiver(Box::new(
            super::super::rows::ReceiverRow {
                signature: f.clone(),
                slot: 1_000,
                t_rx_ns: Some(1),
                observed_unix_s: (start_s + 10) as u64,
                event: receiver,
                enrichment: Default::default(),
                enrichment_status: "COMPLETE".into(),
            },
        ))],
    );
    w.seal(true).unwrap();
    (d, root, f, live, start_s, end_s)
}

/// The INDEX checkpoint of `driver_run`'s window in `dir`: the head page (after the end, the
/// liquidation), the page at its cursor (before the start) and the transaction.
fn checkpoint(dir: &std::path::Path, f: &str, live: &Value, start_s: i64, end_s: i64) {
    let (a, c) = (Signature::new_unique(), Signature::new_unique());
    crate::market::observer::verification::cache_fixture(
        dir,
        ChainRead::Signatures {
            address: program(),
            before: None,
        },
        &json!([
            row(&a.to_string(), 2_000, end_s + 50, Value::Null),
            row(f, 1_000, start_s + 10, Value::Null)
        ]),
    );
    crate::market::observer::verification::cache_fixture(
        dir,
        ChainRead::Signatures {
            address: program(),
            before: Some(Signature::from_str(f).unwrap()),
        },
        &json!([row(&c.to_string(), 900, start_s - 50, Value::Null)]),
    );
    crate::market::observer::verification::cache_fixture(
        dir,
        ChainRead::Transaction {
            signature: Signature::from_str(f).unwrap(),
        },
        live,
    );
}

/// The two jobs of a CROSSCHECK (each its own output, cache and budget) and its result dir.
fn crosscheck_jobs(tag: &str) -> (Job, Job, std::path::PathBuf) {
    let base = job(tag);
    let out = base.output.join("crosscheck");
    let primary = Job {
        output: out.join("primary"),
        ..base.clone()
    };
    let secondary = Job {
        output: out.join("secondary"),
        ..base
    };
    (primary, secondary, out)
}

/// DEC-08 (CROSSCHECK): two independent sources reading the same history are
/// CROSSCHECK_VERIFIED — each its own INDEX_VERIFIED result, its own cache and calls, every
/// transaction of the window read identically — and nothing is promoted to canonical.
#[test]
fn dec08_crosscheck_verifies_two_agreeing_sources() {
    let h = Arc::new(History::new());
    let (one, two) = (h.clone(), h.clone());
    let (url_a, seen_a) = loopback(move |m, p| one.answer(m, p));
    let (url_b, seen_b) = loopback(move |m, p| two.answer(m, p));
    let (a, b, out) = crosscheck_jobs("cc-agree");
    let r = crosscheck(
        [&a, &b],
        10,
        [Some(url_a), Some(url_b)],
        &live_capture(&h),
        &out,
    )
    .unwrap();
    assert_eq!(r.status, "CROSSCHECK_VERIFIED", "{r:?}");
    assert!(r.reason.is_none() && r.incomplete_reasons.is_empty() && r.disagreements.is_empty());
    assert_eq!(
        (r.primary.status.as_str(), r.secondary.status.as_str()),
        ("INDEX_VERIFIED", "INDEX_VERIFIED")
    );
    assert_eq!(
        (r.primary.calls, r.secondary.calls),
        (5, 5),
        "each source read itself"
    );
    assert_eq!((methods(&seen_a).len(), methods(&seen_b).len()), (5, 5));
    assert_eq!(r.primary.qualified, r.secondary.qualified);
    assert!(!r.canonical_verified && !r.c0_e1_master_pass);
    assert!(!r.primary.canonical_verified && !r.secondary.canonical_verified);
    for dir in [
        a.output.join("results"),
        b.output.join("results"),
        out.join("results"),
    ] {
        assert_eq!(std::fs::read_dir(dir).unwrap().count(), 1);
    }
    assert!(r.terminal().contains("CROSSCHECK_VERIFIED"));
    std::fs::remove_dir_all(a.output.parent().unwrap().parent().unwrap()).unwrap();
}

/// DEC-08 (CROSSCHECK): a source that lacks a transaction of the window, reads it differently
/// (failed instead of the liquidation) or leaves it unresolved disagrees — HISTORICAL_INCOMPLETE
/// with every disagreement named, even when both indexes are complete on their own.
#[test]
fn dec08_crosscheck_names_every_disagreement() {
    let h = Arc::new(History::new());
    let one = h.clone();
    let (url_a, _) = loopback(move |m, p| one.answer(m, p));
    // The second source: B missing from its index, F failed, Y only in its index.
    let two = h.clone();
    let y = Signature::new_unique().to_string();
    let only = y.clone();
    let (url_b, _) = loopback(move |m, p| match (m, p[1]["before"].as_str()) {
        ("getSignaturesForAddress", None) => Reply::Result(json!([
            row(&two.a, SLOT + 300, BT + 300, Value::Null),
            row(&two.e, SLOT + 100, BT + 100, Value::Null),
            row(&two.f, SLOT, BT, History::failed()),
            row(&only, SLOT - 3, BT - 3, History::failed()),
        ])),
        ("getSignaturesForAddress", Some(b)) if b == only => Reply::Result(json!([
            row(&two.c, SLOT - 200, BT - 200, Value::Null),
            row(&two.d, SLOT - 201, BT - 201, Value::Null),
        ])),
        ("getTransaction", _) if p[0].as_str() == Some(two.f.as_str()) => {
            Reply::Result(variant(&two.f, SLOT, BT, History::failed()))
        }
        ("getTransaction", _) if p[0].as_str() == Some(only.as_str()) => {
            Reply::Result(variant(&only, SLOT - 3, BT - 3, History::failed()))
        }
        _ => two.answer(m, p),
    });
    let (a, b, out) = crosscheck_jobs("cc-disagree");
    let r = crosscheck(
        [&a, &b],
        10,
        [Some(url_a), Some(url_b.clone())],
        &live_capture(&h),
        &out,
    )
    .unwrap();
    assert_eq!(
        (r.primary.status.as_str(), r.secondary.status.as_str()),
        ("INDEX_VERIFIED", "INDEX_VERIFIED"),
        "both complete on their own"
    );
    assert_eq!(
        (r.status.as_str(), r.reason.as_deref()),
        ("HISTORICAL_INCOMPLETE", Some("CROSSCHECK_DISAGREEMENT"))
    );
    let fields = |sig: &str| -> Vec<&str> {
        r.disagreements
            .iter()
            .filter(|d| d.signature == sig)
            .map(|d| d.field.as_str())
            .collect()
    };
    assert_eq!(fields(&h.b), ["INDEXED_BY_PRIMARY_ONLY"]);
    assert_eq!(fields(&y), ["INDEXED_BY_SECONDARY_ONLY"]);
    assert_eq!(fields(&h.f), ["CLASSIFICATION", "PROGRAM_EVENTS", "EVENTS"]);
    assert!(fields(&h.e).is_empty(), "the boundary row agrees");
    std::fs::remove_dir_all(a.output.parent().unwrap().parent().unwrap()).unwrap();

    // An unresolved transaction of the second source: its own result is incomplete.
    let three = h.clone();
    let (url_c, _) = loopback(move |m, p| match m {
        "getTransaction" if p[0].as_str() == Some(three.f.as_str()) => Reply::Result(Value::Null),
        _ => three.answer(m, p),
    });
    let one = h.clone();
    let (url_a, _) = loopback(move |m, p| one.answer(m, p));
    let (a, b, out) = crosscheck_jobs("cc-unresolved");
    let r = crosscheck(
        [&a, &b],
        10,
        [Some(url_a), Some(url_c)],
        &live_capture(&h),
        &out,
    )
    .unwrap();
    assert_eq!(
        r.incomplete_reasons,
        [
            "CROSSCHECK_SECONDARY_INCOMPLETE(HISTORICAL_PAYLOAD_UNKNOWN)",
            "CROSSCHECK_DISAGREEMENT"
        ],
        "agreement on F is unprovable: {r:?}"
    );
    assert_eq!(r.secondary.unresolved_transactions, 1);
    assert_eq!(
        r.disagreements
            .iter()
            .map(|d| (d.signature.as_str(), d.field.as_str()))
            .collect::<Vec<_>>(),
        [(h.f.as_str(), "UNRESOLVED")]
    );
    std::fs::remove_dir_all(a.output.parent().unwrap().parent().unwrap()).unwrap();
}

/// DEC-08 end to end through the one driver (offline): `--mode crosscheck` without checkpoints
/// is incomplete on both sources; with both sources' checkpoints the resumed attempt is
/// CROSSCHECK_VERIFIED — never canonical.
#[test]
fn dec08_the_driver_runs_and_resumes_the_crosscheck() {
    use super::super::strict_driver;
    let (d, root, f, live, start_s, end_s) = driver_run(line!());
    let args = |resume: bool| -> Vec<String> {
        let mut a: Vec<String> = [
            "--run",
            root.to_str().unwrap(),
            "--mode",
            "crosscheck",
            "--offline",
            "--verify-max-pages",
            "5",
            "--verify-rps",
            "100",
            "--verify-calls",
            "10",
            "--verify-seconds",
            "30",
            "--verify-ram-bytes",
            &(1u64 << 36).to_string(),
            "--verify-min-free-bytes",
            "1",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        if resume {
            a.push("--resume-verification".into());
        }
        a
    };
    let (text, verified) = strict_driver::cli(&args(false)).unwrap();
    assert!(!verified);
    assert!(
        text.contains(
            "status=HISTORICAL_INCOMPLETE reason=CROSSCHECK_PRIMARY_INCOMPLETE(OFFLINE_CACHE_MISS)"
        ),
        "{text}"
    );
    let spec_id = text
        .lines()
        .find_map(|l| l.strip_prefix("verification_spec_id="))
        .unwrap()
        .to_string();
    let out = root
        .canonicalize()
        .unwrap()
        .join("verification")
        .join(format!("{}{spec_id}", strict_driver::NATIVE_PREFIX));
    let spec: strict_driver::VerificationSpec =
        journal::read_typed_value(&out.join("spec.p0j")).unwrap();
    assert_eq!(spec.mode, "crosscheck");
    for source in ["primary", "secondary"] {
        checkpoint(
            &out.join("crosscheck").join(source),
            &f,
            &live,
            start_s,
            end_s,
        );
    }
    let (text, verified) = strict_driver::cli(&args(true)).unwrap();
    assert!(verified, "{text}");
    assert!(
        text.contains("status=CROSSCHECK_VERIFIED reason=none"),
        "{text}"
    );
    assert!(text.contains("CANONICAL_VERIFIED=false"), "{text}");
    let results: Vec<CrosscheckResult> = std::fs::read_dir(out.join("crosscheck/results"))
        .unwrap()
        .map(|e| journal::read_typed_value(&e.unwrap().path()).unwrap())
        .collect();
    assert_eq!(results.len(), 2);
    assert!(results
        .iter()
        .any(|r| r.status == "CROSSCHECK_VERIFIED" && r.primary.calls == 0));
    no_promotion(&root, &out.join("crosscheck/results"));
    let methods = super::super::verification_view::methods(&root);
    let [m] = &methods[..] else {
        panic!("{methods:?}")
    };
    assert_eq!(
        (
            m.method.as_str(),
            m.status.as_str(),
            m.disagreements,
            m.checkpoint_reads
        ),
        ("crosscheck", "CROSSCHECK_VERIFIED", Some(0), 6)
    );
    assert_eq!(m.progress_done, 4, "two pages from each source");
    let _ = std::fs::remove_dir_all(&d);
}

/// DEC-08 (review H1): an offline history read (the GUI's "from the checkpoint", `--offline`)
/// continues the operator's online spec of the same definition — its checkpoint, never an
/// empty offline one; the run's view shows that spec verified. Without resume it is refused.
#[test]
fn dec08_an_offline_read_continues_the_online_checkpoint() {
    use super::super::strict_driver::{self, DriverRequest, StrictBudget, VerifyMode};
    let (d, root, f, live, start_s, end_s) = driver_run(line!());
    let budget = StrictBudget {
        rps: 100,
        max_calls: 10,
        max_seconds: 30,
        max_bytes: 1 << 26,
        max_disk_bytes: 1 << 26,
        max_ram_bytes: 1 << 36,
        min_free_bytes: 1,
    };
    let req = |resume| DriverRequest {
        run: root.clone(),
        budget: budget.clone(),
        offline: true,
        resume,
        mode: VerifyMode::Index { max_pages: 5 },
        provider_evidence: None,
        cancelled: None,
    };
    // No online spec yet: the offline read is its own (an empty checkpoint, incomplete).
    let first = strict_driver::drive(&req(false)).unwrap();
    assert_eq!(first.reason.as_deref(), Some("OFFLINE_CACHE_MISS"));
    // Review M2: an incomplete reading's counts are no facts — unknown, never 0.
    let view = super::super::verification_view::methods(&root);
    assert_eq!(view[0].status, "HISTORICAL_INCOMPLETE");
    assert!(view[0].qualified.is_empty() && view[0].markers.is_empty());
    assert!(view[0].comparison.is_empty() && view[0].transactions.is_empty());
    assert_eq!((view[0].unresolved, view[0].calls), (None, Some(0)));
    let verification = root.canonicalize().unwrap().join("verification");
    let offline: strict_driver::VerificationSpec = journal::read_typed_value(
        &verification
            .join(format!(
                "{}{}",
                strict_driver::NATIVE_PREFIX,
                first.verification_spec_id
            ))
            .join("spec.p0j"),
    )
    .unwrap();
    // The operator's online spec of the same definition (its own budget, a bound provider)
    // with its attempt and checkpoint.
    let online = strict_driver::VerificationSpec {
        provider_binding_sha256: Some([7; 32]),
        budget: StrictBudget {
            rps: 2,
            max_calls: 2000,
            ..budget.clone()
        },
        ..offline.clone()
    };
    let dir = verification.join(format!(
        "{}{}",
        strict_driver::NATIVE_PREFIX,
        online.id().unwrap()
    ));
    std::fs::create_dir_all(dir.join("attempts/online")).unwrap();
    journal::write_typed_value(&dir.join("spec.p0j"), source::STRICT, &online).unwrap();
    let attempt = strict_driver::AttemptRecord {
        verification_spec_id: online.id().unwrap(),
        verification_attempt_id: "online".into(),
        at_unix_ns: 1,
        offline: false,
        resume: false,
        verification_result_id: None,
        status: Some("HISTORICAL_INCOMPLETE".into()),
        reason: Some("RPC_RATE_LIMIT".into()),
        capture_evidence_digest: String::new(),
    };
    for name in ["start.p0j", "end.p0j"] {
        journal::write_typed_value(
            &dir.join("attempts/online").join(name),
            source::STRICT,
            &attempt,
        )
        .unwrap();
    }
    checkpoint(&dir.join("index"), &f, &live, start_s, end_s);
    assert_eq!(
        strict_driver::drive(&req(false)).unwrap_err(),
        "SPEC_EXISTS_USE_RESUME_VERIFICATION"
    );
    let resumed = strict_driver::drive(&req(true)).unwrap();
    assert_eq!(resumed.verification_spec_id, online.id().unwrap());
    assert_eq!(resumed.status, "INDEX_VERIFIED", "{}", resumed.terminal);
    let methods = super::super::verification_view::methods(&root);
    let [m] = &methods[..] else {
        panic!("{methods:?}")
    };
    assert_eq!(
        (
            m.spec_id.as_str(),
            m.status.as_str(),
            m.offline,
            m.provider_binding.is_some()
        ),
        (online.id().unwrap().as_str(), "INDEX_VERIFIED", true, true)
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// DEC-08 (review M3, M2): a checkpoint whose newest page does not reach past the window end is
/// incomplete (INDEX_HEAD_BEFORE_END, its counts no facts) and the next online attempt reads that
/// page again — the finalized index caught up, the window verifies from the rest of the
/// checkpoint; offline it stays as checkpointed.
#[test]
fn dec08_a_head_before_the_end_is_read_again_online() {
    let h = Arc::new(History::new());
    let caught_up = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (server, flag) = (h.clone(), caught_up.clone());
    let (url, seen) = loopback(move |m, p| match (m, p[1]["before"].as_str()) {
        ("getSignaturesForAddress", None) if !flag.load(std::sync::atomic::Ordering::SeqCst) => {
            Reply::Result(json!([
                row(&server.f, SLOT, BT, Value::Null),
                row(&server.b, SLOT - 5, BT - 5, History::failed()),
            ]))
        }
        _ => server.answer(m, p),
    });
    let j = job("head-lag");
    let bot = live_capture(&h);
    let first = run(&j, 10, Some(url.clone()), &bot).unwrap();
    assert_eq!(
        (first.status.as_str(), first.incomplete_reasons.clone()),
        (
            "HISTORICAL_INCOMPLETE",
            vec!["INDEX_HEAD_BEFORE_END".to_string()]
        )
    );
    assert!(first.crossed_start && !first.head_after_end);
    let offline = run(&j, 10, None, &bot).unwrap();
    assert_eq!(offline.incomplete_reasons, ["INDEX_HEAD_BEFORE_END"]);
    caught_up.store(true, std::sync::atomic::Ordering::SeqCst);
    let calls = methods(&seen).len();
    let second = run(&j, 10, Some(url), &bot).unwrap();
    assert_eq!(second.status, "INDEX_VERIFIED", "{second:?}");
    assert_eq!(
        (second.signature_calls, second.transaction_calls),
        (1, 1),
        "the head again, the end second's transaction; the rest from the checkpoint"
    );
    assert_eq!(methods(&seen).len(), calls + 2);
    std::fs::remove_dir_all(&j.output).unwrap();
}

/// DEC-08 (pinned `paginate`): a page crosses the start only when every row lies before it — a
/// page mixing rows of the window and before the start is followed by the next page.
#[test]
fn dec08_a_page_crosses_the_start_only_with_every_row_before_it() {
    let h = Arc::new(History::new());
    let server = h.clone();
    let (url, _) = loopback(move |m, p| match (m, p[1]["before"].as_str()) {
        ("getSignaturesForAddress", None) => Reply::Result(json!([
            row(&server.a, SLOT + 300, BT + 300, Value::Null),
            row(&server.f, SLOT, BT, Value::Null),
            row(&server.c, SLOT - 200, BT - 200, Value::Null),
        ])),
        ("getSignaturesForAddress", Some(b)) if b == server.c => {
            Reply::Result(json!([row(&server.d, SLOT - 201, BT - 201, Value::Null)]))
        }
        _ => server.answer(m, p),
    });
    let j = job("mixed");
    let r = run(&j, 10, Some(url), &live_capture(&h)).unwrap();
    assert_eq!(r.status, "INDEX_VERIFIED", "{r:?}");
    assert_eq!((r.pages, r.signature_calls), (2, 2));
    std::fs::remove_dir_all(&j.output).unwrap();
}

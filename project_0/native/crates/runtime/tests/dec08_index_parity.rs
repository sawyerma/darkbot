//! DEC-08 parity gate (PHASENPLAN DEC-08: before the Python productive dependency is removed): the
//! native INDEX reading of the committed A4 sources — the six real liquidation transactions and
//! their 60 documented mutations — against the pinned Python verifier on the same bytes. Every
//! difference must be one of the documented I-rules; the pinned verifier must be the pinned file.
//! The full A4 scope (2,917 sources) is the local evidence `docs/evidence/DEC08_INDEX_PARITY_20261028`.
use std::path::{Path, PathBuf};
use std::process::Command;

/// `sha256(operations/verification/audit_market_quick.py)` — the pinned verifier (no re-pin
/// without an explicit user decision).
const PINNED_VERIFIER_SHA256: &str =
    "6bdd2c50911295da06a9b9bdd68115531cbeceadd48789de1395bb0fa9285868";

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[test]
fn dec08_native_index_matches_the_pinned_verifier_on_the_committed_sources() {
    let project = project().canonicalize().unwrap();
    let verifier = project.join("operations/verification/audit_market_quick.py");
    let sha = darkbot_runner::analytics::journal::file_sha256(&verifier).unwrap();
    assert_eq!(
        darkbot_runner::analytics::journal::hex(&sha),
        PINNED_VERIFIER_SHA256,
        "the pinned verifier changed"
    );
    let mut tx: Vec<PathBuf> = std::fs::read_dir(project.join("docs/evidence/AUDIT_20261005"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json")
                && p.file_stem().is_some_and(|s| s.len() >= 80)
        })
        .collect();
    tx.sort();
    assert_eq!(tx.len(), 6, "the six real liquidation transactions");
    let out = Command::new("python3")
        .arg("-I")
        .arg(project.join("operations/qualification/parity_differential.py"))
        .arg(env!("CARGO_BIN_EXE_p0-parity"))
        .args(["index", "tx"])
        .args(&tx)
        .args(["--", "mut"])
        .args(&tx)
        .current_dir(&project)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("compared=66 kinds={'INDEX': 66} identical=39 explained=27 unexplained=0"),
        "{stdout}"
    );
    // S4-02: I7 — a 0.1.12 bankruptcy settlement (its 0.1.11 control agrees exactly).
    for rule in ["I2=E2", "I3=E3", "I4=E4", "I6_", "I7_"] {
        assert!(stdout.contains(rule), "{rule}: {stdout}");
    }
    // The A4 decode differential of the same sources (its E-rules, E7 for 0.1.12 bankruptcy).
    let out = Command::new("python3")
        .arg("-I")
        .arg(project.join("operations/qualification/parity_differential.py"))
        .arg(env!("CARGO_BIN_EXE_p0-parity"))
        .arg("tx")
        .args(&tx)
        .args(["--", "mut"])
        .args(&tx)
        .current_dir(&project)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("compared=66 kinds={'TX': 66} identical=12 explained=54 unexplained=0"),
        "{stdout}"
    );
    for rule in ["E1_", "E2_", "E3_", "E4_", "E5_", "E6_", "E7_"] {
        assert!(stdout.contains(rule), "{rule}: {stdout}");
    }
}

// ------------------------------------------------------------------------------------------
// Decision parity: the pinned Audit (import, paging, fetch, compare, report) against the native
// INDEX over the same typed run (its pinned historical views rendered from the typed journals)
// and the same provider answers.
// ------------------------------------------------------------------------------------------

use darkbot_runner::analytics::journal::{source, Provenance, Writer};
use darkbot_runner::analytics::measurement::{
    journal as phase, persist_manifest, MeasurementPlan, Phase, NS,
};
use darkbot_runner::market_observer::{index, rows, verification::Job};
use darkbot_runner::run_layout::{RunLayout, RunPaths};
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use std::str::FromStr;

const FIXTURE: &str = "docs/evidence/AUDIT_20261005/2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf.json";

fn program() -> Pubkey {
    Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET).unwrap()
}

/// The real liquidation moved to another identity (signature, slot, time, status).
fn variant(signature: &str, slot: u64, bt: i64, err: Value) -> Value {
    let mut tx: Value =
        serde_json::from_slice(&std::fs::read(project().join(FIXTURE)).unwrap()).unwrap();
    tx["transaction"]["signatures"][0] = json!(signature);
    tx["slot"] = json!(slot);
    tx["blockTime"] = json!(bt);
    tx["meta"]["err"] = err;
    tx
}
fn row(signature: &str, slot: u64, bt: i64, err: &Value) -> Value {
    json!({"signature": signature, "slot": slot, "blockTime": bt, "err": err, "memo": null,
        "confirmationStatus": "finalized"})
}
fn record(account: &str) -> darkbot_runner::analytics::record::OpportunityRecord {
    serde_json::from_value(json!({
        "timing": {"t_rx_ns": 10, "t_state_ns": 15, "t_detect_ns": 20, "t_ready_ns": 30,
            "t_sign_ns": 40, "t_handoff_ns": null},
        "rx_slot": 1, "detect_slot": 2, "selected_costs": null, "simulation_result": "PASS",
        "account": account, "trigger": "parity", "decision": "ShadowReady", "net_profit": null,
        "economics_known": false, "premium_bps": null, "t_profitable_unix_s": null,
        "tx_version": "V1", "serialized_bytes": 100, "locked_accounts": 3, "path": "RPC",
        "funding": null, "signature": null, "sign_slot": 3, "simulated_cu": 100,
        "candidate_costs": [], "exposure": null, "all_signatures": [], "bundle_id": null,
        "submitted_tip": null, "intervals_ns": [], "reason": null, "loss": null}))
    .unwrap()
}
fn frames<P: darkbot_runner::analytics::payload::Payload>(w: &mut Writer, src: u32, rows: &[P]) {
    for r in rows {
        w.append_typed(
            Provenance {
                source: src,
                ..Default::default()
            },
            r,
        )
        .unwrap();
    }
}

/// The history of a scenario: `A` after the end, `E` at the end second, the liquidations `F`
/// (received live) and `G` (not received), the failed `B`; `C`, `D` before the start.
struct History {
    a: String,
    e: String,
    f: String,
    g: String,
    b: String,
    c: String,
    d: String,
    start_s: i64,
    end_s: i64,
}
impl History {
    fn failed() -> Value {
        json!({"InstructionError": [0, {"Custom": 1}]})
    }
    fn tx(&self, which: &str) -> Value {
        let null = Value::Null;
        match which {
            "e" => variant(&self.e, 8_500, self.end_s, null),
            "f" => variant(&self.f, 8_000, self.start_s + 10, null),
            "g" => variant(&self.g, 7_900, self.start_s + 20, null),
            _ => variant(&self.b, 7_800, self.start_s + 30, Self::failed()),
        }
    }
    /// The provider's answers as fixture files (the pinned harness and the native loopback read
    /// the same files).
    fn provider(&self, dir: &Path, exhausted: bool) {
        std::fs::create_dir_all(dir).unwrap();
        let null = Value::Null;
        let head = json!([
            row(&self.a, 9_000, self.end_s + 30, &null),
            row(&self.e, 8_500, self.end_s, &null),
            row(&self.f, 8_000, self.start_s + 10, &null),
            row(&self.g, 7_900, self.start_s + 20, &null),
            row(&self.b, 7_800, self.start_s + 30, &Self::failed()),
        ]);
        let older = if exhausted {
            json!([])
        } else {
            json!([
                row(&self.c, 7_000, self.start_s - 50, &null),
                row(&self.d, 6_999, self.start_s - 51, &null),
            ])
        };
        let write = |name: String, v: &Value| {
            std::fs::write(dir.join(format!("{name}.json")), v.to_string()).unwrap()
        };
        write("signatures_head".into(), &head);
        write(format!("signatures_{}", self.b), &older);
        for (which, sig) in [
            ("e", &self.e),
            ("f", &self.f),
            ("g", &self.g),
            ("b", &self.b),
        ] {
            write(format!("transaction_{sig}"), &self.tx(which));
        }
    }
}

/// What the capture recorded besides the live liquidation `F`.
#[derive(Clone, Copy)]
struct Capture {
    /// Slot boundaries around the window (live coverage proven) …
    observed: bool,
    /// … with a counted gap, reconnected and closed, around `G`.
    outage_at_g: bool,
    /// An exact Shadow join of `F` (`ECON_CANONICAL` with its evaluation).
    join_f: bool,
    /// Shadow opportunities of `G`'s target with `G` as the unique winner: one strongly joined
    /// to `G`'s market event (evidence without an event identity), one with the winner alone
    /// (no key in the pinned rows).
    opportunity_g: bool,
}

/// A layout-V2 run with its manifest, CAPTURE window, market journal and derived rows, and its
/// pinned historical views rendered from the typed journals (the pinned verifier's input).
fn scenario(dir: &Path, capture: Capture) -> (PathBuf, History) {
    let root = dir.join("run");
    let paths = RunPaths::new(&root, RunLayout::V2);
    paths.create().unwrap();
    let plan = MeasurementPlan::new(60).unwrap();
    let clock = darkbot_common::timing::Clock::new();
    persist_manifest(&paths, "run_parity", "test", &plan, clock).unwrap();
    let window = plan.capture_window(clock, 1_000 * NS).unwrap();
    let (start_s, end_s) = (
        (window.capture_start_unix_ns / u128::from(NS)) as i64,
        (window.capture_cutoff_unix_ns / u128::from(NS)) as i64,
    );
    phase(&paths, clock, Phase::Capture(window)).unwrap();
    let s = || Signature::new_unique().to_string();
    let h = History {
        a: s(),
        e: s(),
        f: s(),
        g: s(),
        b: s(),
        c: s(),
        d: s(),
        start_s,
        end_s,
    };
    let f = darkbot_solana::chain::decode_transaction(&h.tx("f")).unwrap();
    let logs = f.meta.as_ref().unwrap().log_messages.clone().unwrap();
    let receiver = darkbot_runner::market_observer::decode_receiver_events(&logs, &program())
        .events[0]
        .clone();
    std::fs::create_dir_all(paths.market_dir()).unwrap();
    let mut w = Writer::create(
        &paths
            .market_dir()
            .join(darkbot_runner::market_observer::MARKET_JOURNAL),
    )
    .unwrap();
    let bot_only = s();
    frames(
        &mut w,
        source::MARKET,
        &[
            rows::MarketTransaction {
                signature: h.f.clone(),
                slot: 8_000,
                origin: rows::TxOrigin::TransactionSubscribe {
                    observed_unix_s: (start_s + 10) as u64,
                    t_rx_ns: 1,
                    tx: Ok(f.clone()),
                },
            },
            rows::MarketTransaction {
                signature: bot_only.clone(),
                slot: 100,
                origin: rows::TxOrigin::LogsSubscribe {
                    observed_unix_s: (start_s + 11) as u64,
                    t_rx_ns: 2,
                    logs: darkbot_solana::chain::ChainLogs {
                        slot: 100,
                        signature: bot_only.clone(),
                        status: darkbot_solana::chain::TxStatus::Success,
                        logs: Some(logs.clone()),
                    },
                },
            },
        ],
    );
    frames(
        &mut w,
        source::MARKET,
        &[rows::MarketLiquidation::Receiver(Box::new(
            rows::ReceiverRow {
                signature: h.f.clone(),
                slot: 8_000,
                t_rx_ns: Some(1),
                observed_unix_s: (start_s + 10) as u64,
                event: receiver,
                enrichment: Default::default(),
                enrichment_status: "COMPLETE".into(),
            },
        ))],
    );
    let boundary = |at: i64, slot| rows::MarketCoverage::SlotBoundary {
        slot,
        t_rx_ns: 0,
        observed_unix_s: at as u64,
        start: None,
        end: None,
    };
    let window_event = |at: i64, event| rows::MarketCoverage::Window {
        observed_unix_s: at as u64,
        t_rx_ns: None,
        observing: true,
        counted: true,
        event,
    };
    let mut coverage = Vec::new();
    if capture.observed {
        coverage.push(boundary(start_s - 5, 7_500));
    }
    if capture.outage_at_g {
        coverage.push(window_event(
            start_s + 15,
            rows::WindowEvent::Gap {
                from_slot: None,
                to_slot: None,
            },
        ));
        coverage.push(window_event(
            start_s + 18,
            rows::WindowEvent::Reconnect { attempt: 1 },
        ));
        coverage.push(boundary(start_s + 25, 7_950));
    }
    coverage.push(boundary(end_s + 5, 8_600));
    frames(&mut w, source::MARKET, &coverage);
    w.seal(true).unwrap();

    std::fs::create_dir_all(paths.snapshot("")).unwrap();
    let mut reports =
        Writer::create(&paths.snapshot(darkbot_runner::analytics::econ::REPORT_JOURNAL)).unwrap();
    let f_events = index::classify(&f, &program(), f.block_time).events;
    if capture.join_f {
        let market = f_events[0].clone();
        frames(
            &mut reports,
            source::ECON,
            &[darkbot_runner::analytics::econ::CanonicalJoin {
                our: Some(record(&market.target)),
                market,
                target_seen: true,
                target_seen_without_slot: false,
                health_liquidatable: Some(true),
                candidates_created: Some(true),
                plan_validated: Some(true),
                built: Some(true),
                identity_status: "TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT",
                diagnostic: None,
                market_commitment: "finalized".into(),
                winner_slot_first_seen_ns: None,
                capital: vec![],
            }],
        );
    }
    if capture.opportunity_g {
        let g = darkbot_solana::chain::decode_transaction(&h.tx("g")).unwrap();
        let event = darkbot_runner::market_observer::decode_receiver_events(
            g.meta.as_ref().unwrap().log_messages.as_ref().unwrap(),
            &program(),
        )
        .events[0]
            .clone();
        let opportunity =
            |account: String, joined: bool| darkbot_runner::analytics::econ::OpportunityRow {
                rec: record(&account),
                winner: Some(darkbot_runner::winner::Winner {
                    slot: 7_900,
                    identity: darkbot_runner::winner::WinnerIdentity::Unique {
                        signature: h.g.clone(),
                        receiver: event.receiver.to_string(),
                        tx_version: "V1".into(),
                    },
                }),
                market: joined.then(|| darkbot_runner::analytics::econ::MarketEvent {
                    signature: h.g.clone(),
                    slot: 7_900,
                    t_rx_ns: Some(3),
                    source: darkbot_runner::analytics::econ::MarketEvidenceSource::Live,
                    transaction_index: None,
                    event: event.clone(),
                }),
                slot_seen: None,
                capital: vec![],
                market_commitment: "finalized".into(),
            };
        frames(
            &mut reports,
            source::ECON,
            &[
                opportunity(f_events[0].target.clone(), true),
                opportunity(Pubkey::new_unique().to_string(), false),
            ],
        );
    }
    reports.seal(true).unwrap();
    // The pinned verifier reads the run's historical views: rendered from the typed journals by
    // the explicit offline export (the same renders the read-only view check verifies).
    let paths = darkbot_runner::analytics::renders::detect(&root).unwrap();
    for view in darkbot_runner::analytics::renders::views(&paths).unwrap() {
        if !view.journal.exists() {
            continue;
        }
        if let Some(bytes) = darkbot_runner::analytics::renders::render(&view).unwrap() {
            std::fs::create_dir_all(view.output.parent().unwrap()).unwrap();
            std::fs::write(&view.output, bytes).unwrap();
        }
    }
    (root, h)
}

/// Loopback JSON-RPC endpoint answering from the scenario's provider files (as the harness).
fn provider_endpoint(dir: PathBuf) -> String {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
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
            let q: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let read = |name: String| {
                std::fs::read(dir.join(format!("{name}.json")))
                    .ok()
                    .map(|b| serde_json::from_slice::<Value>(&b).unwrap())
            };
            let answer = match q["method"].as_str() {
                Some("getSignaturesForAddress") => {
                    let before = q["params"][1]["before"].as_str().unwrap_or("head");
                    json!({"jsonrpc": "2.0", "id": q["id"],
                        "result": read(format!("signatures_{before}")).unwrap_or(Value::Null)})
                }
                _ => {
                    let sig = q["params"][0].as_str().unwrap_or("");
                    match read(format!("error_{sig}")) {
                        Some(error) => json!({"jsonrpc": "2.0", "id": q["id"], "error": error}),
                        None => json!({"jsonrpc": "2.0", "id": q["id"],
                            "result": read(format!("transaction_{sig}")).unwrap_or(Value::Null)}),
                    }
                }
            }
            .to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                answer.len(),
                answer
            );
        }
    });
    url
}

/// The native INDEX attempt of a scenario, in the harness's canonical lines.
fn native_lines(root: &Path, url: &str, out: &Path, max_pages: u64) -> Vec<String> {
    let binding = darkbot_runner::market_observer::strict_driver::bind_capture(root).unwrap();
    let job = Job {
        verification_spec_id: "parity".into(),
        output: out.to_path_buf(),
        start_ns: binding.window.0,
        end_ns: binding.window.1,
        program: program().to_string(),
        max_calls: 100,
        max_seconds: 60,
        max_bytes: 64 << 20,
        max_disk_bytes: 1 << 30,
        max_ram_bytes: 64 << 30,
        min_free_bytes: 0,
        rps: 1000,
        offline: false,
        cancelled: None,
    };
    let bot = index::BotEvidence::from_run(root, &program()).unwrap();
    let r = index::run(&job, max_pages, Some(url.to_string()), &bot).unwrap();
    let flag = |b: bool| if b { "true" } else { "false" };
    let kinds = |n: &[u64; 4]| {
        index::KINDS
            .iter()
            .zip(n)
            .map(|(k, n)| format!("{}={n}", k.as_str()))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut lines = vec![
        format!(
            "STATUS status={} reason={} incomplete={}",
            r.status,
            r.reason.as_deref().unwrap_or("none"),
            r.incomplete_reasons.join(",")
        ),
        format!(
            "SCOPE crossed_start={} head_after_end={} pages={} indexed={} in_window={} unresolved={} unknown={}",
            flag(r.crossed_start),
            flag(r.head_after_end),
            r.pages,
            r.indexed_signatures,
            r.in_window_transactions,
            r.unresolved_transactions,
            r.unknown_transactions
        ),
        format!("QUALIFIED {}", kinds(&r.qualified)),
        format!("OBSERVED {}", kinds(&r.observed)),
        format!(
            "COUNTS {}",
            r.comparison_counts
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",")
        ),
        format!("ZERO proven={}", flag(r.zero_liquidations_proven)),
        format!(
            "BOT_ONLY {}",
            r.bot_only
                .iter()
                .map(|(s, n)| format!("{s}:{n}"))
                .collect::<Vec<_>>()
                .join(",")
        ),
    ];
    let mut compared: Vec<_> = r
        .comparisons
        .iter()
        .map(|c| {
            let events: Vec<String> = c
                .events
                .iter()
                .map(|e| {
                    let mut sources: Vec<String> = e
                        .matched
                        .iter()
                        .map(|m| format!("{}={}", m.source.name(), m.status))
                        .collect();
                    sources.sort();
                    format!(
                        "{}:{}:{}:{}:{}",
                        e.event_index,
                        e.kind.as_str(),
                        e.detection,
                        e.shadow_status,
                        sources.join("|")
                    )
                })
                .collect();
            (
                (c.signature.clone(), c.slot),
                format!(
                    "COMPARE {} {} tx={} coverage={} class={} events={}",
                    c.signature,
                    c.slot,
                    c.transaction_status,
                    c.live_coverage,
                    c.classification,
                    events.join(";")
                ),
            )
        })
        .collect();
    compared.sort();
    lines.extend(compared.into_iter().map(|(_, l)| l));
    lines
}

/// The pinned verifier's decision over the same run and provider files.
fn pinned_lines(root: &Path, provider: &Path, out: &Path, max_pages: u64) -> Vec<String> {
    let project = project().canonicalize().unwrap();
    let o = Command::new("python3")
        .arg("-I")
        .arg(project.join("operations/qualification/index_decision_parity.py"))
        .arg(root)
        .arg(provider)
        .arg(max_pages.to_string())
        .arg(out)
        .current_dir(&project)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .map(String::from)
        .collect()
}

/// D2: the pinned import reads live logs at `raw.value(.meta)`, where a rendered
/// `transactionSubscribe` row has none (its transaction is `raw.transaction`): the native INDEX
/// alone matches the live transaction's own payload events. Removed from the native line only
/// for the transactions received through `transactionSubscribe`, and only as MATCHes.
fn without_live_tx_payload(line: &str, live_tx: &[&str]) -> String {
    if !line.starts_with("COMPARE ") || !live_tx.iter().any(|s| line.contains(s)) {
        return line.to_string();
    }
    let (head, events) = line.split_once(" events=").unwrap();
    let events: Vec<String> = events
        .split(';')
        .map(|e| {
            let (left, sources) = e.rsplit_once(':').unwrap();
            let kept: Vec<&str> = sources
                .split('|')
                .filter(|s| {
                    let payload = s.starts_with("LIVE_PAYLOAD=");
                    assert!(!payload || *s == "LIVE_PAYLOAD=MATCH", "{line}");
                    !payload
                })
                .collect();
            format!("{left}:{}", kept.join("|"))
        })
        .collect();
    format!("{head} events={}", events.join(";"))
}

fn assert_parity(native: &[String], pinned: &[String], live_tx: &[&str], what: &str) {
    let native: Vec<String> = native
        .iter()
        .map(|l| without_live_tx_payload(l, live_tx))
        .collect();
    assert_eq!(native, pinned, "{what}");
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("p0_dec08_parity_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// DEC-08 decision parity, positive: a verified window — the live liquidation matches its
/// observer row, its canonical join and exact Shadow; the liquidation the capture missed inside
/// the observed slot interval, the failed transaction, the capture's own live transaction the
/// index lacks — identical lines from the pinned verifier and the native INDEX.
#[test]
fn dec08_decisions_match_the_pinned_verifier_on_a_verified_window() {
    let d = dir("verified");
    let (root, h) = scenario(
        &d,
        Capture {
            observed: true,
            outage_at_g: false,
            join_f: true,
            opportunity_g: false,
        },
    );
    h.provider(&d.join("provider"), false);
    let url = provider_endpoint(d.join("provider"));
    let native = native_lines(&root, &url, &d.join("native"), 10);
    let pinned = pinned_lines(&root, &d.join("provider"), &d.join("pinned"), 10);
    assert!(
        native[0].starts_with("STATUS status=INDEX_VERIFIED"),
        "{native:?}"
    );
    assert!(native
        .iter()
        .any(|l| l.contains("MISSING_LIQUIDATION_WITHIN_OBSERVED_INTERVAL")));
    assert!(native.iter().any(|l| l.contains("SHADOW_MATCH")));
    assert!(native
        .iter()
        .any(|l| l.starts_with("BOT_ONLY ") && l.len() > 9));
    assert_parity(&native, &pinned, &[&h.f], "verified window");
    let _ = std::fs::remove_dir_all(&d);
}

/// DEC-08 decision parity, negative: a page budget ends the attempt before the start
/// (INDEX_PAGE_BUDGET, START_NOT_CROSSED); the missed liquidation lies in a proven outage; the
/// Shadow evidence names no event identity — then the history exhausts before the start.
#[test]
fn dec08_decisions_match_the_pinned_verifier_on_incomplete_histories() {
    let d = dir("budget");
    let capture = Capture {
        observed: true,
        outage_at_g: true,
        join_f: false,
        opportunity_g: true,
    };
    let (root, h) = scenario(&d, capture);
    h.provider(&d.join("provider"), false);
    let url = provider_endpoint(d.join("provider"));
    let native = native_lines(&root, &url, &d.join("native"), 1);
    let pinned = pinned_lines(&root, &d.join("provider"), &d.join("pinned"), 1);
    assert!(
        native[0].contains(
            "reason=INDEX_PAGE_BUDGET incomplete=INDEX_PAGE_BUDGET,INDEX_START_NOT_CROSSED"
        ),
        "{native:?}"
    );
    assert!(native.iter().any(|l| l.contains("MISSING_DURING_OUTAGE")));
    assert!(native.iter().any(|l| l.contains("SHADOW_IDENTITY_UNKNOWN")));
    assert_parity(&native, &pinned, &[&h.f], "page budget");
    let _ = std::fs::remove_dir_all(&d);

    let d = dir("exhausted");
    let (root, h) = scenario(&d, capture);
    h.provider(&d.join("provider"), true);
    let url = provider_endpoint(d.join("provider"));
    let native = native_lines(&root, &url, &d.join("native"), 10);
    let pinned = pinned_lines(&root, &d.join("provider"), &d.join("pinned"), 10);
    assert!(
        native[0].contains("reason=INDEX_HISTORY_EXHAUSTED_BEFORE_START"),
        "{native:?}"
    );
    assert_parity(&native, &pinned, &[&h.f], "exhausted history");
    let _ = std::fs::remove_dir_all(&d);
}

/// DEC-08 decision parity, resume: a transaction the provider answers `null` is unresolved
/// (HISTORICAL_PAYLOAD_UNKNOWN) on both sides, and both resume from their own checkpoints to the
/// same verified window once it is answered. D1: a pruned answer (-32009) ends the pinned
/// attempt (RPC_PROVIDER_ERROR, the rest unread), the native one keeps it unresolved and works
/// the window — both HISTORICAL_INCOMPLETE, both resume to the same verified decision.
#[test]
fn dec08_decisions_match_the_pinned_verifier_through_unresolved_and_resume() {
    for pruned in [false, true] {
        let d = dir(if pruned { "pruned" } else { "null" });
        let (root, h) = scenario(
            &d,
            Capture {
                observed: true,
                outage_at_g: false,
                join_f: false,
                opportunity_g: false,
            },
        );
        let provider = d.join("provider");
        h.provider(&provider, false);
        let missing = provider.join(format!("transaction_{}.json", h.g));
        let kept = std::fs::read(&missing).unwrap();
        std::fs::remove_file(&missing).unwrap();
        let error = provider.join(format!("error_{}.json", h.g));
        if pruned {
            std::fs::write(&error, r#"{"code": -32009, "message": "fixture"}"#).unwrap();
        }
        let url = provider_endpoint(provider.clone());
        let native_first = native_lines(&root, &url, &d.join("native"), 10);
        let pinned_first = pinned_lines(&root, &provider, &d.join("pinned"), 10);
        if pruned {
            assert!(native_first[0].starts_with("STATUS status=HISTORICAL_INCOMPLETE reason=none incomplete=HISTORICAL_PAYLOAD_UNKNOWN"), "{native_first:?}");
            assert!(native_first[1].contains("unresolved=1"), "{native_first:?}");
            assert!(
                pinned_first[0]
                    .starts_with("STATUS status=HISTORICAL_INCOMPLETE reason=RPC_PROVIDER_ERROR"),
                "{pinned_first:?}"
            );
        } else {
            assert!(
                native_first[0].ends_with("incomplete=HISTORICAL_PAYLOAD_UNKNOWN"),
                "{native_first:?}"
            );
            assert_parity(&native_first, &pinned_first, &[&h.f], "null transaction");
        }
        let _ = std::fs::remove_file(&error);
        std::fs::write(&missing, kept).unwrap();
        let native_resumed = native_lines(&root, &url, &d.join("native"), 10);
        let pinned_resumed = pinned_lines(&root, &provider, &d.join("pinned"), 10);
        assert!(
            native_resumed[0].starts_with("STATUS status=INDEX_VERIFIED"),
            "{native_resumed:?}"
        );
        assert_parity(&native_resumed, &pinned_resumed, &[&h.f], "resumed");
        let _ = std::fs::remove_dir_all(&d);
    }
}

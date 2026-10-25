//! G0.8 reads the G0.7 evidence. The real G0.8 binary gets a binary G0.7
//! evidence record (DBREC001) holding every required field of the G0.7
//! schema, the nested preflight record included: final and provisional
//! evidence pass G0.8's evidence checks up to its first RPC (it then stops
//! for the missing RPC URL, so no network is touched); defective records
//! are refused before any RPC.

use darkbot_bsc_record::schema::{g06a, g07::evidence};
use darkbot_bsc_record::{Draft, Kind, Presence, Schema, Value, sha256};
use std::{fs, path::PathBuf, process::Command};

const G08: &str = env!("CARGO_BIN_EXE_g08_mainnet_verify");
const BEFORE_FIRST_RPC: &str = "BSC_MAINNET_RPC_HTTPS is required";

fn address(hex_digits: &str) -> Value {
    let raw: [u8; 20] = hex::decode(hex_digits).unwrap().try_into().unwrap();
    Value::Address(raw)
}

/// Every required field of `schema` with a neutral value, nested records
/// included.
fn filled(schema: &'static Schema) -> Draft {
    let mut d = Draft::new(schema);
    for f in schema
        .fields
        .iter()
        .filter(|f| f.presence == Presence::Required)
    {
        let value = match f.kind {
            Kind::Bool => Value::Bool(false),
            Kind::U64 => Value::U64(0),
            Kind::I64 => Value::I64(0),
            Kind::U256 => Value::U256([0; 32]),
            Kind::Address => Value::Address([0; 20]),
            Kind::B256 => Value::B256([0; 32]),
            Kind::Bytes => Value::Bytes(Vec::new()),
            Kind::Text => Value::Text("test".into()),
            Kind::Nested => filled(f.nested.unwrap()).into_nested(),
        };
        d.set(f.tag, value);
    }
    d
}

fn evidence_draft(classification: &str) -> Draft {
    let mut d = filled(&evidence::SCHEMA);
    d.set(evidence::GATE, "G0.7")
        .set(evidence::ATTEMPT_ID, "g07-compat-test")
        .set(evidence::CLASSIFICATION, classification)
        .set(evidence::CHAIN_ID, 56u64)
        .set(
            evidence::PORTAL,
            address("e2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0"),
        )
        .set(evidence::CAPTURE_COMPLETE, false)
        .set(evidence::LAUNCH_TX_HASH, Value::B256([0x01; 32]))
        .set(evidence::BUY_TX_HASH, Value::B256([0x02; 32]))
        .set(
            evidence::TOKEN,
            address("e51c4deb1038afcd95ef12d4a649cdbcf6fa7777"),
        )
        .set(
            evidence::LAUNCHER,
            address("3e7b487ae5e1c49ab1cf6222bdb5cf016e86aacd"),
        )
        .set(
            evidence::BUYER,
            address("9c8a0db41914beee52dd49790c4f087e252743de"),
        )
        .set(evidence::MIN_OUTPUT, Value::u128(1))
        .set(evidence::BUY_VALUE_WEI, Value::u128(1_000_000_000_000));
    d
}

/// Recomputes the trailing checksum of hand-edited bytes, so G0.8 reaches
/// the check under test.
fn reseal(mut bytes: Vec<u8>) -> Vec<u8> {
    let end = bytes.len() - 32;
    let digest = sha256(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
    bytes
}

fn g08_stderr(tag: &str, contents: &[u8]) -> String {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("darkbot-g08-compat-{tag}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("g07.dbrec");
    fs::write(&path, contents).unwrap();
    let out = Command::new(G08)
        .env_clear()
        .env("DARKBOT_G08_G07_EVIDENCE", &path)
        .env("DARKBOT_G08_CHAIN_JOURNAL", dir.join("chain.journal"))
        .env("DARKBOT_G08_EVIDENCE_PATH", dir.join("result.dbrec"))
        .output()
        .unwrap();
    // Nothing was captured: the journal is only created after the RPC reads.
    assert!(!dir.join("chain.journal").exists(), "{tag}");
    fs::remove_dir_all(&dir).unwrap();
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !stdout.is_empty() { assert!(stdout.contains("UNRESOLVED")); assert!(!stdout.contains("PASS")); }
    format!("{}{}", String::from_utf8_lossy(&out.stderr), stdout)
}

#[test]
fn g08_reads_final_and_provisional_g07_evidence_up_to_its_first_rpc() {
    for (tag, classification, capture_complete) in [
        (
            "final",
            "SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY",
            true,
        ),
        (
            "incomplete",
            "STOP_CAPTURE_INCOMPLETE__RECONCILE_G08__NO_AUTOMATIC_RETRY",
            false,
        ),
        (
            "provisional",
            "PROVISIONAL_RELEASED__RECEIPT_WINDOW_OPEN__RECONCILE_G08__NO_AUTOMATIC_RETRY",
            false,
        ),
        (
            "at-release",
            "PROVISIONAL_RELEASED__PROVIDER_RESULTS_PENDING__RECONCILE_G08__NO_AUTOMATIC_RETRY",
            false,
        ),
    ] {
        let mut d = evidence_draft(classification);
        d.set(evidence::CAPTURE_COMPLETE, capture_complete);
        if tag == "final" {
            // Final evidence carries the optional window fields as well.
            d.set(evidence::BOTH_RECEIPTS_OBSERVED_NS, 1u64)
                .set(evidence::COVERAGE_END_NS, 2u64)
                .set(evidence::CANONICAL_WINDOW_RECONCILED_IN_G08, false)
                .set(evidence::LAUNCH_RECEIPT_OBSERVED, true)
                .set(evidence::BUY_RECEIPT_OBSERVED, true);
        }
        let err = g08_stderr(tag, &d.encode().unwrap());
        assert!(err.contains(BEFORE_FIRST_RPC), "{tag}: {err}");
    }
}

#[test]
fn g08_refuses_defective_g07_evidence_before_any_rpc() {
    let full = evidence_draft("SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY")
        .encode()
        .unwrap();
    // Header: magic 0..8, format version 8..10, record type 10..12,
    // schema version 12..16; the trailing 32 bytes are the checksum.
    let mut record_type = full.clone();
    record_type[10] = 9;
    let mut schema_version = full.clone();
    schema_version[12] = 2;
    let mut flipped = full.clone();
    flipped[full.len() / 2] ^= 0x01;
    let mut checksum = full.clone();
    *checksum.last_mut().unwrap() ^= 0x80;
    let other_schema = filled(&g06a::preflight::SCHEMA).encode().unwrap();
    let mut testnet =
        evidence_draft("SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY");
    testnet.set(evidence::CHAIN_ID, 97u64);
    let mut other_portal =
        evidence_draft("SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY");
    other_portal.set(evidence::PORTAL, Value::Address([0x11; 20]));
    let legacy_json = br#"{"schema":"darkbot.g07.mainnet-self-launch-buy.v2","chain_id":56}"#;
    for (tag, broken, expected) in [
        ("empty", Vec::new(), "truncated"),
        ("truncated", full[..full.len() / 2].to_vec(), "checksum"),
        ("tail", full[..full.len() - 1].to_vec(), "checksum"),
        ("record-type", reseal(record_type), "record type 9"),
        ("schema-version", reseal(schema_version), "version 2"),
        ("flipped", flipped, "checksum"),
        ("checksum", checksum, "checksum"),
        ("other-schema", other_schema, "record type 3"),
        ("legacy-json", legacy_json.to_vec(), "not a DBREC001 record"),
        ("chain", testnet.encode().unwrap(), "56"),
        ("portal", other_portal.encode().unwrap(), "assertion"),
    ] {
        let err = g08_stderr(tag, &broken);
        assert!(
            !err.contains(BEFORE_FIRST_RPC),
            "{tag} reached the RPC: {err}"
        );
        assert!(err.contains(expected), "{tag}: {err}");
    }
}

/// A loopback JSON-RPC endpoint (plain HTTP, one connection per call, as
/// curl makes them): BSC chain ID, a receipt for the launch hash, a
/// permanent provider error for the BUY receipt. No network.
fn rpc_stub() -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        listener.set_nonblocking(false).unwrap();
        for _ in 0..8 {
            let Ok((mut s, _)) = listener.accept() else {
                return;
            };
            let mut buf = Vec::new();
            let mut tmp = [0u8; 4096];
            let body = loop {
                let n = s.read(&mut tmp).unwrap_or(0);
                if n == 0 {
                    break None;
                }
                buf.extend_from_slice(&tmp[..n]);
                let text = String::from_utf8_lossy(&buf).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            k.eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse::<usize>().ok())?
                        })
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + length {
                        break Some(text[end + 4..end + 4 + length].to_string());
                    }
                }
            };
            let Some(body) = body else { continue };
            let call: serde_json::Value = serde_json::from_str(&body).unwrap();
            let param = call["params"][0].as_str().unwrap_or("").to_string();
            let launch = format!("0x{}", "01".repeat(32));
            let result = match call["method"].as_str().unwrap() {
                "eth_chainId" => r#""result":"0x38""#.to_string(),
                "eth_getTransactionReceipt" if param == launch => format!(
                    r#""result":{{"blockNumber":"0x10","blockHash":"0x{}","transactionIndex":"0x0","transactionHash":"{launch}","from":"0x3e7b487ae5e1c49ab1cf6222bdb5cf016e86aacd","to":"0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0","contractAddress":null,"status":"0x1","gasUsed":"0x5208","cumulativeGasUsed":"0x5208","effectiveGasPrice":"0x2faf080","logs":[]}}"#,
                    "aa".repeat(32)
                ),
                _ => r#""error":{"code":-32602,"message":"stub"}"#.to_string(),
            };
            let payload = format!(r#"{{"jsonrpc":"2.0","id":1,{result}}}"#);
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );
        }
    });
    (url, server)
}

/// BSC-013 (BSC_Bug_V2 P4-G3): an abort after the launch receipt was
/// resolved records exactly that, not "no receipt seen".
#[test]
fn a_late_unresolved_abort_records_the_receipts_already_resolved() {
    use darkbot_bsc_record::schema::g08::unresolved as u;
    let dir: PathBuf =
        std::env::temp_dir().join(format!("darkbot-g08-abort-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("g07.dbrec");
    fs::write(
        &path,
        evidence_draft("SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY")
            .encode()
            .unwrap(),
    )
    .unwrap();
    let (url, server) = rpc_stub();
    let out = Command::new(G08)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("BSC_MAINNET_RPC_HTTPS", &url)
        .env("DARKBOT_G08_G07_EVIDENCE", &path)
        .env("DARKBOT_G08_CHAIN_JOURNAL", dir.join("chain.journal"))
        .env("DARKBOT_G08_EVIDENCE_PATH", dir.join("result.dbrec"))
        .output()
        .unwrap();
    drop(server);
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result =
        darkbot_bsc_record::decode(&u::SCHEMA, &fs::read(dir.join("result.dbrec")).unwrap())
            .unwrap();
    assert_eq!(result.text(u::VERDICT).unwrap(), "UNRESOLVED");
    assert!(
        result.text(u::REASON).unwrap().contains("receipt"),
        "{}",
        result.render()
    );
    assert!(
        result.bool(u::LAUNCH_RECEIPT_RESOLVED).unwrap(),
        "{}",
        result.render()
    );
    assert!(!result.bool(u::BUY_RECEIPT_RESOLVED).unwrap());
    assert!(!result.bool(u::AUTOMATIC_RETRY).unwrap());
    fs::remove_dir_all(&dir).unwrap();
}

/// V2 §2.3, for real: a G0.7 attempt whose transactions never reached the
/// chain. The real G0.8 verifier asks a real read-only BSC RPC
/// (`BSC_MAINNET_RPC_HTTPS`), finds no canonical receipt and records
/// UNRESOLVED with both receipts unresolved and no automatic retry; never a
/// BUY, a "not sent", an ADAPT or a PASS. With `DARKBOT_G08_E2E_OUT` the
/// record is kept there for the reporter→gateway→browser round trip
/// (ops/tests/bsc_control_test.py).
#[test]
#[ignore = "network: real read-only BSC RPC, about four minutes of receipt polling"]
fn g08_records_unresolved_for_a_never_sent_attempt_on_a_real_rpc() {
    use darkbot_bsc_record::schema::g08::unresolved as un;
    let rpc = std::env::var("BSC_MAINNET_RPC_HTTPS").expect("BSC_MAINNET_RPC_HTTPS");
    let dir: PathBuf =
        std::env::temp_dir().join(format!("darkbot-g08-never-sent-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let g07 = dir.join("g07.dbrec");
    let result = dir.join("unresolved.dbrec");
    let d = evidence_draft(
        "PROVISIONAL_RELEASED__RECEIPT_WINDOW_OPEN__RECONCILE_G08__NO_AUTOMATIC_RETRY",
    );
    fs::write(&g07, d.encode().unwrap()).unwrap();
    let out = Command::new(G08)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("BSC_MAINNET_RPC_HTTPS", rpc)
        .env("DARKBOT_G08_G07_EVIDENCE", &g07)
        .env("DARKBOT_G08_CHAIN_JOURNAL", dir.join("chain.journal"))
        .env("DARKBOT_G08_EVIDENCE_PATH", &result)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(2),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r = darkbot_bsc_record::decode(&un::SCHEMA, &fs::read(&result).unwrap()).unwrap();
    assert_eq!(r.text(un::VERDICT).unwrap(), "UNRESOLVED");
    assert_eq!(r.text(un::REASON).unwrap(), "SENT_TRANSACTION_UNRESOLVED");
    assert!(!r.bool(un::LAUNCH_RECEIPT_RESOLVED).unwrap());
    assert!(!r.bool(un::BUY_RECEIPT_RESOLVED).unwrap());
    assert!(!r.bool(un::AUTOMATIC_RETRY).unwrap());
    assert_eq!(r.text(un::T0_TO_BUY).unwrap(), "UNKNOWN");
    assert_eq!(
        r.text(un::EXPIRY_OBSERVATION_SCOPE).unwrap(),
        un::NON_CONCLUSIVE_SCOPE
    );
    // The real chain did not know the transactions (the read either says so
    // or failed and is reported); it never claims they were not sent.
    assert_ne!(r.opt_bool(un::LAUNCH_TX_KNOWN).unwrap(), Some(true));
    assert!(stdout.contains("UNRESOLVED") && !stdout.contains("PASS"));
    if let Some(keep) = std::env::var_os("DARKBOT_G08_E2E_OUT") {
        fs::copy(&result, PathBuf::from(keep).join("unresolved.dbrec")).unwrap();
    }
    fs::remove_dir_all(&dir).unwrap();
}

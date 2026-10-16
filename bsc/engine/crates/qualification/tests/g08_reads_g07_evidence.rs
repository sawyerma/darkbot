//! G0.8 reads the G0.7 evidence. The real G0.8 binary gets a minimal G0.7
//! evidence holding exactly the fields G0.7 checks before every write
//! (`g08_readable`): final and provisional evidence pass G0.8's evidence
//! checks up to its first RPC (it then stops for the missing RPC URL, so no
//! network is touched); defective evidence is refused before any RPC.

use std::{fs, path::PathBuf, process::Command};

const G08: &str = env!("CARGO_BIN_EXE_g08_mainnet_verify");
const BEFORE_FIRST_RPC: &str = "BSC_MAINNET_RPC_HTTPS is required";

fn evidence(classification: &str) -> String {
    serde_json::json!({
        "schema":"darkbot.g07.mainnet-self-launch-buy.v2",
        "gate":"G0.7",
        "attempt_id":"g07-compat-test",
        "classification":classification,
        "chain_id":56,
        "portal":"0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0",
        "capture":{"complete_local_journal":false},
        "transactions":{
            "launch_tx_hash":"0x01",
            "buy_tx_hash":"0x02",
            "token":"0xe51c4deb1038afcd95ef12d4a649cdbcf6fa7777",
            "min_output":"1",
            "buy_value_wei":"1000000000000"
        },
        "wallets":{
            "launcher":"0x3e7b487ae5e1c49ab1cf6222bdb5cf016e86aacd",
            "buyer":"0x9c8a0db41914beee52dd49790c4f087e252743de"
        }
    })
    .to_string()
}

fn g08_stderr(tag: &str, contents: &[u8]) -> String {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("darkbot-g08-compat-{tag}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("g07.json");
    fs::write(&path, contents).unwrap();
    let out = Command::new(G08)
        .env_clear()
        .env("DARKBOT_G08_G07_EVIDENCE", &path)
        .env("DARKBOT_G08_CHAIN_JOURNAL", dir.join("chain.journal"))
        .output()
        .unwrap();
    fs::remove_dir_all(&dir).unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn g08_reads_final_and_provisional_g07_evidence_up_to_its_first_rpc() {
    for (tag, classification) in [
        (
            "final",
            "SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY",
        ),
        (
            "incomplete",
            "STOP_CAPTURE_INCOMPLETE__RECONCILE_G08__NO_AUTOMATIC_RETRY",
        ),
        (
            "provisional",
            "PROVISIONAL_RELEASED__RECEIPT_WINDOW_OPEN__RECONCILE_G08__NO_AUTOMATIC_RETRY",
        ),
        (
            "at-release",
            "PROVISIONAL_RELEASED__PROVIDER_RESULTS_PENDING__RECONCILE_G08__NO_AUTOMATIC_RETRY",
        ),
    ] {
        let err = g08_stderr(tag, evidence(classification).as_bytes());
        assert!(err.contains(BEFORE_FIRST_RPC), "{tag}: {err}");
    }
}

#[test]
fn g08_refuses_defective_g07_evidence_before_any_rpc() {
    let full = evidence("SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY");
    let wrong_schema = full.replace(".v2\"", ".v1\"");
    let no_capture = full.replace("\"complete_local_journal\":false", "\"complete\":false");
    for (tag, broken) in [
        ("empty", &b""[..]),
        ("truncated", &full.as_bytes()[..full.len() / 2]),
        ("object", &b"{}"[..]),
        ("schema", wrong_schema.as_bytes()),
        ("capture", no_capture.as_bytes()),
    ] {
        let err = g08_stderr(tag, broken);
        assert!(
            !err.contains(BEFORE_FIRST_RPC),
            "{tag} reached the RPC: {err}"
        );
    }
}

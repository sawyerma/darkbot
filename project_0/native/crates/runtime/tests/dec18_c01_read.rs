//! DEC-18 §2 (P0-025, C-01 steps 5–6) over the real `p0ctl`: the deployment read against a
//! loopback RPC provider of the registry. The plan reads nothing; its run reads the program, its
//! ProgramData (upgrade authority, ELF), its IDL, the group and FeeState, every bank of the group
//! (a 0.1.11 bank among 0.1.12 ones), the oracle a bank observes and the token programs' bytecode
//! (loader v2 and v3) — finalized, once, within the plan's budget — and derives the parity
//! verdict: a mismatching IDL and a bank of another layout are FAIL, the build and the FE-01
//! observation NOT_RUN, so no PASS. Refused before any read: another token, a second run, a
//! rotated credential. An exhausted budget ends the reads (BLOCKED). A debug build only.
#![cfg(all(debug_assertions, feature = "crate_health"))]
use base64::Engine;
use darkbot_project0::protocol;
use darkbot_runner::orchestration::c01_read as c01;
use marginfi_type_crate::types::{Bank, FeeState, MarginfiGroup, OracleSetup};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

/// An account of the loopback chain: owner, executable, data.
type Fixture = (Pubkey, bool, Vec<u8>);

fn pk(s: &str) -> Pubkey {
    Pubkey::from_str(s).unwrap()
}

fn chain() -> (HashMap<Pubkey, Fixture>, Vec<(Pubkey, Fixture)>, Pubkey) {
    let program = pk(protocol::PROGRAM_ID_MAINNET);
    let upgradeable = pk(protocol::BPF_LOADER_UPGRADEABLE_ID);
    let loader2 = pk(protocol::BPF_LOADER_2_ID);
    let group = pk("4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8");
    let authority = Pubkey::new_unique();
    let program_account = |program: &Pubkey| {
        let mut d = vec![2, 0, 0, 0];
        d.extend_from_slice(protocol::program_data_address(program).as_ref());
        (upgradeable, true, d)
    };
    let program_data = |elf: &[u8]| {
        let mut d = vec![3, 0, 0, 0];
        d.extend_from_slice(&123u64.to_le_bytes());
        d.push(1);
        d.extend_from_slice(authority.as_ref());
        d.extend_from_slice(elf);
        d.extend_from_slice(&[0; 64]);
        (upgradeable, false, d)
    };
    let typed = |disc: [u8; 8], len: usize| {
        let mut d = disc.to_vec();
        d.resize(8 + len, 0);
        (program, false, d)
    };
    let mut accounts = HashMap::new();
    accounts.insert(program, program_account(&program));
    accounts.insert(
        protocol::program_data_address(&program),
        program_data(b"\x7fELF marginfi"),
    );
    // The IDL: discriminator, authority, length, zlib JSON.
    let idl = Pubkey::create_with_seed(
        &Pubkey::find_program_address(&[], &program).0,
        "anchor:idl",
        &program,
    )
    .unwrap();
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(br#"{"version":"0.1.12","instructions":[{"name":"lending_account_liquidate"},{"name":"marginfi_account_tag_liq_record"}]}"#).unwrap();
    let compressed = z.finish().unwrap();
    let mut d = vec![0u8; 8];
    d.extend_from_slice(authority.as_ref());
    d.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    d.extend_from_slice(&compressed);
    accounts.insert(idl, (program, false, d));
    accounts.insert(
        group,
        typed(
            protocol::DISC_MARGINFI_GROUP,
            std::mem::size_of::<MarginfiGroup>(),
        ),
    );
    accounts.insert(
        protocol::fee_state_pda(&program),
        typed(protocol::DISC_FEE_STATE, std::mem::size_of::<FeeState>()),
    );
    // The banks: one observing a Pyth push oracle, one without, one of the 0.1.11 layout.
    let oracle = Pubkey::new_unique();
    let mut observed: Bank = bytemuck::Zeroable::zeroed();
    observed.config.oracle_setup = OracleSetup::PythPushOracle;
    observed.config.oracle_keys[0] = oracle;
    let mut bank_data = protocol::DISC_BANK.to_vec();
    bank_data.extend_from_slice(bytemuck::bytes_of(&observed));
    let banks = vec![
        (Pubkey::new_unique(), (program, false, bank_data)),
        (
            Pubkey::new_unique(),
            typed(protocol::DISC_BANK, std::mem::size_of::<Bank>()),
        ),
        (
            Pubkey::new_unique(),
            typed(protocol::DISC_BANK, protocol::BANK_PAYLOAD_LEN_0_1_11),
        ),
    ];
    accounts.insert(oracle, (Pubkey::new_unique(), false, vec![7; 134]));
    // The token programs: loader v2 (Token, ATA) and loader v3 (Token-2022).
    for (key, elf) in [
        (protocol::TOKEN_PROGRAM_ID, b"\x7fELF token".to_vec()),
        (
            protocol::ASSOCIATED_TOKEN_PROGRAM_ID,
            b"\x7fELF ata".to_vec(),
        ),
    ] {
        accounts.insert(pk(key), (loader2, true, elf));
    }
    let t22 = pk(protocol::TOKEN_2022_PROGRAM_ID);
    accounts.insert(t22, program_account(&t22));
    accounts.insert(
        protocol::program_data_address(&t22),
        program_data(b"\x7fELF token-2022"),
    );
    (accounts, banks, group)
}

fn wire(a: &Fixture) -> serde_json::Value {
    serde_json::json!({
        "owner": a.0.to_string(), "lamports": 1, "executable": a.1, "rentEpoch": 0, "space": a.2.len(),
        "data": [base64::engine::general_purpose::STANDARD.encode(&a.2), "base64"]
    })
}

/// A loopback RPC provider serving the fixture chain; records each request's method and line.
fn rpc(seen: Arc<Mutex<Vec<(String, String)>>>) -> String {
    let (accounts, banks, _) = chain();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (accounts, banks) = (Arc::new(accounts), Arc::new(banks));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let (seen, accounts, banks) = (seen.clone(), accounts.clone(), banks.clone());
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    let mut length = 0usize;
                    loop {
                        let mut h = String::new();
                        if reader.read_line(&mut h).unwrap_or(0) == 0 {
                            return;
                        }
                        if h == "\r\n" {
                            break;
                        }
                        if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                            length = v.trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0; length];
                    if reader.read_exact(&mut body).is_err() {
                        return;
                    }
                    let req: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    let method = req["method"].as_str().unwrap_or_default().to_string();
                    seen.lock()
                        .unwrap()
                        .push((method.clone(), line.trim().to_string()));
                    assert_eq!(
                        req["params"][1]["commitment"], "finalized",
                        "finalized reads"
                    );
                    let result = match method.as_str() {
                        "getMultipleAccounts" => {
                            serde_json::json!({"context":{"slot":req["params"][1]["minContextSlot"].as_u64().unwrap_or(0).max(100)},"value":
                            req["params"][0].as_array().unwrap().iter().map(|k| {
                                accounts.get(&pk(k.as_str().unwrap())).map(wire)
                            }).collect::<Vec<_>>()})
                        }
                        "getProgramAccounts" => serde_json::json!({"context":{"slot":101},"value":
                            banks.iter().map(|(k, a)| serde_json::json!({"pubkey": k.to_string(), "account": wire(a)})).collect::<Vec<_>>()}),
                        _ => serde_json::Value::Null,
                    };
                    let reply =
                        serde_json::json!({"jsonrpc":"2.0","id":1,"result":result}).to_string();
                    let out = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{reply}",
                        reply.len()
                    );
                    if reader.get_mut().write_all(out.as_bytes()).is_err() {
                        return;
                    }
                }
            });
        }
    });
    url
}

struct Project {
    base: PathBuf,
    project: PathBuf,
    store: PathBuf,
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

fn run(p: &Project, args: &[&str], env: &[(&str, &str)], stdin: Option<&[u8]>) -> (bool, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_p0ctl"));
    c.arg("--project")
        .arg(&p.project)
        .args(args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("P0_PROJECT_DIR", &p.project)
        .env(darkbot_solana::providers::TEST_STORE_ENV, &p.store)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        c.env(k, v);
    }
    let mut child = c.spawn().unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        if let Some(bytes) = stdin {
            input.write_all(bytes).unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

fn setup(tag: &str) -> Project {
    let base = std::env::temp_dir().join(format!("p0_025_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (project, store) = (base.join("project"), base.join("keychain"));
    std::fs::create_dir_all(&project).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let url = rpc(seen.clone());
    let p = Project {
        base,
        project,
        store,
        seen,
    };
    let add = vec![
        "provider",
        "add",
        "--id",
        "loop-rpc",
        "--kind",
        "rpc",
        "--endpoint",
        &url,
        "--auth",
        "query:api-key",
        "--credential",
        "loop",
        "--plan",
        "free",
        "--entitlement",
        "loopback-free",
    ];
    let (ok, text) = run(&p, &add, &[], None);
    assert!(ok, "{text}");
    let (ok, text) = run(
        &p,
        &["provider", "credential", "set", "--credential", "loop"],
        &[],
        Some(b"C01-SECRET-1\n"),
    );
    assert!(ok, "{text}");
    let (ok, text) = run(
        &p,
        &["provider", "select", "--kind", "rpc", "--id", "loop-rpc"],
        &[],
        None,
    );
    assert!(ok, "{text}");
    p
}

/// `c01 plan` under `env`: its id and token.
fn plan(p: &Project, env: &[(&str, &str)]) -> (String, String, String) {
    let (ok, text) = run(p, &["c01", "plan"], env, None);
    assert!(ok, "{text}");
    let field = |k: &str| {
        text.split_whitespace()
            .find_map(|w| w.strip_prefix(k))
            .unwrap()
            .to_string()
    };
    (field("id="), field("token="), text)
}

const BUDGET: [(&str, &str); 1] = [("P0_FE_CALLS", "20")];

#[test]
fn p0_025_the_deployment_read_records_every_layer_and_never_declares_parity() {
    let p = setup("read");
    let (plan_id, token, text) = plan(&p, &BUDGET);
    assert!(
        text.contains("budget calls=20") && text.contains("read getProgramAccounts finalized"),
        "{text}"
    );
    assert!(p.seen.lock().unwrap().is_empty(), "the plan reads nothing");
    let (ok, text) = run(
        &p,
        &[
            "c01",
            "run",
            "--plan",
            &plan_id,
            "--approve",
            "0000000000000000",
        ],
        &BUDGET,
        None,
    );
    assert!(!ok && text.contains("C01_APPROVAL_MISMATCH"), "{text}");
    assert!(p.seen.lock().unwrap().is_empty());

    let (ok, text) = run(
        &p,
        &["c01", "run", "--plan", &plan_id, "--approve", &token],
        &BUDGET,
        None,
    );
    assert!(!ok, "no parity PASS without build and FE-01: {text}");
    let seen = p.seen.lock().unwrap().clone();
    assert_eq!(
        seen.iter().map(|(m, _)| m.as_str()).collect::<Vec<_>>(),
        [
            "getMultipleAccounts",
            "getMultipleAccounts",
            "getProgramAccounts",
            "getMultipleAccounts",
            "getMultipleAccounts",
            "getMultipleAccounts"
        ],
    );
    assert!(
        seen.iter()
            .all(|(_, l)| l.contains("?api-key=C01-SECRET-1 ")),
        "the bound credential on every read"
    );
    assert!(!text.contains("C01-SECRET"), "{text}");
    for line in [
        "gate DEPLOYMENT PASS",
        "gate PIN FAIL (informational) NOT_REFERENCE",
        "gate IDL FAIL",
        "gate GROUP PASS",
        "gate BANKS FAIL",
        "gate ORACLES PASS",
        "gate PROGRAMS PASS (informational)",
        "gate BUILD NOT_RUN",
        "gate FE01 NOT_RUN",
        "P1A_PARITY BLOCKED: IDL FAIL",
    ] {
        assert!(text.contains(line), "{line}: {text}");
    }
    assert!(
        text.contains("instructions=2") && text.contains("layout 0.1.11"),
        "{text}"
    );
    // The typed records and the stored bytecode.
    let dir = p.project.join(c01::STORE).join(&plan_id);
    let reads: c01::C01Reads =
        darkbot_runner::analytics::journal::read_typed_value(&dir.join("reads.p0j")).unwrap();
    assert!(reads.deployment.upgrade_authority.is_some() && reads.deployment.elf_len == Some(13));
    assert_eq!(reads.context_slots, vec![100, 100, 101, 101, 101, 101]);
    assert_eq!(reads.banks.len(), 3);
    assert_eq!(reads.banks.iter().filter(|b| b.decodes).count(), 2);
    assert_eq!(reads.dependencies.len(), 1);
    assert!(reads.dependencies[0].owner.is_some());
    for (name, elf) in [
        ("token", &b"\x7fELF token"[..]),
        ("token-2022", b"\x7fELF token-2022"),
        ("associated-token", b"\x7fELF ata"),
    ] {
        assert_eq!(
            std::fs::read(dir.join(format!("programs/{name}.so"))).unwrap(),
            elf,
            "{name}"
        );
    }
    let parity: c01::P1aParity =
        darkbot_runner::analytics::journal::read_typed_value(&dir.join("parity.p0j")).unwrap();
    assert!(parity.overall.starts_with("BLOCKED"));
    // A plan reads once.
    let before = p.seen.lock().unwrap().len();
    let (ok, text) = run(
        &p,
        &["c01", "run", "--plan", &plan_id, "--approve", &token],
        &BUDGET,
        None,
    );
    assert!(!ok && text.contains("C01_PLAN_USED"), "{text}");
    assert_eq!(p.seen.lock().unwrap().len(), before);
    let _ = std::fs::remove_dir_all(&p.base);
}

#[test]
fn p0_025_a_rotated_credential_needs_a_new_plan_and_the_budget_ends_the_reads() {
    let p = setup("budget");
    let (plan_id, token, _) = plan(&p, &BUDGET);
    let (ok, text) = run(
        &p,
        &["provider", "credential", "rotate", "--credential", "loop"],
        &[],
        Some(b"C01-SECRET-2\n"),
    );
    assert!(ok, "{text}");
    let (ok, text) = run(
        &p,
        &["c01", "run", "--plan", &plan_id, "--approve", &token],
        &BUDGET,
        None,
    );
    assert!(
        !ok && text.contains("C01_PLAN_DRIFT: the provider or its credential revision"),
        "{text}"
    );
    assert!(p.seen.lock().unwrap().is_empty());
    // Two calls allowed: the reads end at the third, recorded, never a pass.
    let budget = [("P0_FE_CALLS", "2")];
    let (plan_id, token, _) = plan(&p, &budget);
    let (ok, text) = run(
        &p,
        &["c01", "run", "--plan", &plan_id, "--approve", &token],
        &budget,
        None,
    );
    assert!(!ok, "{text}");
    assert!(text.contains("gate READS BLOCKED C01_BUDGET"), "{text}");
    assert!(text.contains("P1A_PARITY BLOCKED: READS BLOCKED"), "{text}");
    assert_eq!(p.seen.lock().unwrap().len(), 2);
    let _ = std::fs::remove_dir_all(&p.base);
}

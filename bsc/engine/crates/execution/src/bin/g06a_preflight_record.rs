//! Step DRY-0 (and R0) of a requalification DRY run, read-only: writes the
//! current Mainnet preflight as a native binary record through the existing
//! Rust preflight (`run_current_preflight`, the same reads G0.7 makes), and
//! optionally scans the Portal proxy's `Upgraded(address)` events since the
//! frozen DRY head. No signing, no builder, no secret is read; JSON exists
//! only on the RPC wire (chain_reads).
//!
//! Environment: BSC_MAINNET_RPC_HTTPS and the DARKBOT_G06A_* planned
//! parameters of the retired node preflight (addresses, gas price, limits,
//! values). Arguments: `--out <preflight.dbrec>` and optionally
//! `--upgrade-scan-from <block> --baseline <preflight.dbrec> --upgrade-out
//! <portal_upgrade.dbrec>`.
#![allow(dead_code, unused_imports)]
#[path = "g07_mainnet_self_launch_buy/builder_submission.rs"]
mod builder_submission;
#[path = "g07_mainnet_self_launch_buy/chain_reads.rs"]
mod chain_reads;
#[path = "g07_mainnet_self_launch_buy/inputs.rs"]
mod inputs;
#[path = "g07_mainnet_self_launch_buy/shared.rs"]
mod shared;
#[path = "g07_mainnet_self_launch_buy/transactions.rs"]
mod transactions;
use builder_submission::*;
use chain_reads::*;
use darkbot_bsc_record::schema::{
    PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL,
    g06a::{portal_upgrade as up, preflight as pf},
};
use darkbot_bsc_record::{Value as RecordValue, decode};
use inputs::*;
use shared::*;
use std::str::FromStr;
use transactions::*;

/// keccak256("Upgraded(address)"), the EIP-1967 proxy event.
const UPGRADED_TOPIC: &str = "bc7cd75a20ee27fd9adebab32041f755214dbc6bffa90cc0225b39da2e5c2d3b";
/// Blocks per eth_getLogs request.
const SCAN_CHUNK: u64 = 5_000;

fn env_address(name: &str) -> Address {
    Address::from_str(&req_env(name)).unwrap_or_else(|_| panic!("{name} is not an address"))
}

fn write_new(path: &str, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap_or_else(|e| panic!("create-new {path}: {e}"));
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .unwrap_or_else(|e| panic!("write {path}: {e}"));
}

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).map(|i| {
        args.get(i + 1)
            .cloned()
            .unwrap_or_else(|| panic!("{flag} needs a value"))
    })
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let known = [
        "--out",
        "--upgrade-scan-from",
        "--baseline",
        "--upgrade-out",
    ];
    for (i, a) in args.iter().enumerate() {
        assert!(
            i % 2 == 1 || known.contains(&a.as_str()),
            "unknown argument {a}"
        );
    }
    let out = arg(&args, "--out").expect("--out <preflight.dbrec> is required");
    let launcher = env_address("DARKBOT_G06A_LAUNCHER_ADDRESS");
    let buyer = env_address("DARKBOT_G06A_BUYER_ADDRESS");
    let current = run_current_preflight(
        launcher,
        buyer,
        env_u64("DARKBOT_G06A_GAS_PRICE_WEI"),
        env_u64("DARKBOT_G06A_LAUNCH_GAS_LIMIT"),
        env_u64("DARKBOT_G06A_BUY_GAS_LIMIT"),
        env_u128("DARKBOT_G06A_LAUNCH_VALUE_WEI"),
        env_u64("DARKBOT_G06A_BUY_VALUE_WEI"),
    )
    .unwrap_or_else(|e| panic!("read-only preflight failed: {e}"));
    let mut draft = Draft::new(&pf::SCHEMA);
    for (tag, value) in current.fields() {
        draft.push(*tag, value.clone());
    }
    let bytes = draft.encode().expect("encode preflight record");
    write_new(&out, &bytes);
    print!(
        "{}",
        decode(&pf::SCHEMA, &bytes)
            .expect("decode preflight")
            .render()
    );

    if let Some(from) = arg(&args, "--upgrade-scan-from") {
        let from: u64 = from
            .parse()
            .expect("--upgrade-scan-from must be a block number");
        let baseline_path =
            arg(&args, "--baseline").expect("--baseline <preflight.dbrec> is required with a scan");
        let upgrade_out =
            arg(&args, "--upgrade-out").expect("--upgrade-out is required with a scan");
        let baseline = decode(
            &pf::SCHEMA,
            &fs::read(&baseline_path).expect("read baseline"),
        )
        .expect("baseline preflight record");
        let to = field(current.u64(pf::LATEST_BLOCK_NUMBER));
        let proxy = Address::from_str(PORTAL).unwrap();
        let topic: [u8; 32] = hex::decode(UPGRADED_TOPIC).unwrap().try_into().unwrap();
        let mut d = Draft::new(&up::SCHEMA);
        let mut start = from;
        while start <= to {
            let end = start.saturating_add(SCAN_CHUNK - 1).min(to);
            for log in rpc_logs(proxy, topic, start, end)
                .unwrap_or_else(|e| panic!("upgrade scan {start}..={end}: {e}"))
            {
                let implementation = log
                    .topics
                    .get(1)
                    .expect("Upgraded log without implementation");
                let mut u = Draft::new(&up::upgrade::SCHEMA);
                u.set(up::upgrade::BLOCK, log.block)
                    .set(
                        up::upgrade::TRANSACTION_HASH,
                        RecordValue::B256(log.transaction_hash),
                    )
                    .set(up::upgrade::LOG_INDEX, log.log_index)
                    .set(
                        up::upgrade::IMPLEMENTATION,
                        RecordValue::Address(implementation[12..].try_into().unwrap()),
                    );
                d.push(up::UPGRADES, u.into_nested());
            }
            start = end + 1;
        }
        d.set(PROVENANCE_TOOL, "g06a_preflight_record")
            .set(PROVENANCE_OBSERVED_UNIX_MS, unix_time_ms())
            .set(up::PROXY, RecordValue::Address(proxy.into_array()))
            .set(up::SCANNED_FROM_BLOCK, from)
            .set(up::SCANNED_TO_BLOCK, to)
            .set(
                up::CURRENT_IMPLEMENTATION,
                RecordValue::Address(field(current.address(pf::PORTAL_IMPLEMENTATION))),
            )
            .set(
                up::BASELINE_IMPLEMENTATION,
                RecordValue::Address(field(baseline.address(pf::PORTAL_IMPLEMENTATION))),
            );
        let bytes = d.encode().expect("encode upgrade scan");
        write_new(&upgrade_out, &bytes);
        let scan = decode(&up::SCHEMA, &bytes).expect("decode upgrade scan");
        print!("{}", scan.render());
        // The scan outcome for the DRY script (tool output, not a record view).
        let upgrades = scan.nested_all(up::UPGRADES).expect("upgrades");
        let last = upgrades
            .last()
            .map(|u| {
                format!(
                    "0x{}",
                    darkbot_bsc_record::hex(&field(u.address(up::upgrade::IMPLEMENTATION)))
                )
            })
            .unwrap_or_default();
        println!(
            "UPGRADES={}\nLAST_UPGRADE_IMPLEMENTATION={last}",
            upgrades.len()
        );
    }
}

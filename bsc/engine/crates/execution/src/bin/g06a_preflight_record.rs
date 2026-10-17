//! Steps R0 and DRY-0 of a requalification DRY run, read-only. No signing,
//! no builder, no secret is read; JSON exists only on the RPC wire
//! (chain_reads, coverage::wire).
//!
//! R0 (`--upgrade-scan-only --from <block> --baseline <preflight> --out
//! <scan>`): every `Upgraded(address)` event of the Portal proxy from
//! `<block>` to the current head, with complete coverage (`coverage`):
//! hash-verified headers linked by parent hash, their logs blooms, and the
//! receipts of every bloom candidate verified against `receiptsRoot`, all on
//! the configured BSC_MAINNET_RPC_HTTPS. Both ends of the hash chain are
//! anchored on DARKBOT_G06A_ARCHIVE_RPC_HTTPS, which also reads the
//! implementation slot before and after each event. The scan takes long; it
//! runs before the time-bound DRY chain.
//!
//! DRY-0 (`--out <preflight> --upgrade-scan <scan> --upgrade-out <record>`):
//! extends the coverage close to the head, writes the current Mainnet
//! preflight through the existing Rust preflight (`run_current_preflight`,
//! the reads G0.7 makes), extends the coverage to exactly the preflight's
//! block (whose hash must end the chain) and writes the complete R0 record
//! from the scan's start to the preflight block.
//!
//! Environment: BSC_MAINNET_RPC_HTTPS, DARKBOT_G06A_ARCHIVE_RPC_HTTPS and
//! (DRY-0) the DARKBOT_G06A_* planned parameters of the retired node
//! preflight.
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
use darkbot_bsc_record::{Record, Value as RecordValue, decode};
use inputs::*;
use shared::*;
use std::str::FromStr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use transactions::*;

#[path = "g06a_preflight_record/coverage.rs"]
mod coverage;
use coverage::{Coverage, UpgradeSeen, cover};

/// keccak256("Upgraded(address)"), the EIP-1967 proxy event.
const UPGRADED_TOPIC: &str = "bc7cd75a20ee27fd9adebab32041f755214dbc6bffa90cc0225b39da2e5c2d3b";
/// The EIP-1967 implementation slot.
const IMPLEMENTATION_SLOT: &str =
    "360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc";
/// DRY-0 extends the coverage to this many blocks below the head before the
/// preflight, so that the preflight's block (on a load-balanced endpoint)
/// still lies beyond it.
const HEAD_MARGIN: u64 = 20;
const ATTEMPTS: u32 = 10;

fn word(text: &str) -> [u8; 32] {
    hex::decode(text).unwrap().try_into().unwrap()
}

fn env_address(name: &str) -> Address {
    Address::from_str(&req_env(name)).unwrap_or_else(|_| panic!("{name} is not an address"))
}

/// The host of an endpoint URL: what the record names as a source (never a
/// path or query, which may carry a key).
fn host(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("")
        .to_string()
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

/// One read with retries and backoff (public endpoints rate-limit).
fn with_retries<T>(what: &str, mut read: impl FnMut() -> Result<T, String>) -> T {
    let mut last = String::new();
    for attempt in 0..ATTEMPTS {
        match read() {
            Ok(value) => return value,
            Err(e) => last = e,
        }
        thread::sleep(Duration::from_millis(500u64 << attempt.min(5)));
    }
    panic!("{what} failed after {ATTEMPTS} attempts: {last}");
}

/// One upgrade, bound by the implementation slot before and after it.
fn verified_upgrade(seen: &UpgradeSeen, archive_url: &str) -> RecordValue {
    let proxy = Address::from_str(PORTAL).unwrap().into_array();
    let slot = |block: u64| -> [u8; 20] {
        with_retries("archive implementation slot", || {
            coverage::wire::storage_at(archive_url, proxy, word(IMPLEMENTATION_SLOT), block)
        })[12..]
            .try_into()
            .unwrap()
    };
    let mut u = Draft::new(&up::upgrade::SCHEMA);
    u.set(up::upgrade::BLOCK, seen.block)
        .set(up::upgrade::BLOCK_HASH, RecordValue::B256(seen.block_hash))
        .set(up::upgrade::TRANSACTION_HASH, RecordValue::B256(seen.transaction_hash))
        .set(up::upgrade::LOG_INDEX, seen.log_index)
        .set(up::upgrade::IMPLEMENTATION, RecordValue::Address(seen.implementation))
        .set(up::upgrade::RECEIPT_STATUS, seen.receipt_status)
        .set(up::upgrade::SLOT_BEFORE, RecordValue::Address(slot(seen.block - 1)))
        .set(up::upgrade::SLOT_AFTER, RecordValue::Address(slot(seen.block)));
    u.into_nested()
}

struct Sources {
    chain_url: String,
    archive_url: String,
}

fn sources() -> Sources {
    Sources {
        chain_url: req_env("BSC_MAINNET_RPC_HTTPS"),
        archive_url: req_env("DARKBOT_G06A_ARCHIVE_RPC_HTTPS"),
    }
}

/// Anchors both ends of the hash chain on the archive endpoint.
fn anchored(src: &Sources, c: &Coverage) {
    let at = |n: u64| with_retries(&format!("archive hash of {n}"), || coverage::wire::block_hash(&src.archive_url, n));
    assert_eq!(at(c.from - 1), c.first_parent_hash, "the archive disagrees on block {}", c.from - 1);
    assert_eq!(at(c.to), c.last_hash, "the archive disagrees on block {}", c.to);
}

/// The implementation slot at `at`, read on the archive endpoint (any block).
fn implementation_at(src: &Sources, at: u64) -> [u8; 20] {
    with_retries("implementation slot", || {
        coverage::wire::storage_at(
            &src.archive_url,
            Address::from_str(PORTAL).unwrap().into_array(),
            word(IMPLEMENTATION_SLOT),
            at,
        )
    })[12..]
        .try_into()
        .unwrap()
}

/// The R0 record of a verified coverage.
fn upgrade_record(src: &Sources, c: &Coverage, baseline: [u8; 20], current: [u8; 20]) -> Vec<u8> {
    let code = with_retries("current implementation code", || rpc_code(Address::from(current)));
    let mut d = Draft::new(&up::SCHEMA);
    for seen in &c.upgrades {
        d.push(up::UPGRADES, verified_upgrade(seen, &src.archive_url));
    }
    d.set(PROVENANCE_TOOL, "g06a_preflight_record")
        .set(PROVENANCE_OBSERVED_UNIX_MS, unix_time_ms())
        .set(up::PROXY, RecordValue::Address(Address::from_str(PORTAL).unwrap().into_array()))
        .set(up::SCANNED_FROM_BLOCK, c.from)
        .set(up::SCANNED_TO_BLOCK, c.to)
        .set(up::CURRENT_IMPLEMENTATION, RecordValue::Address(current))
        .set(up::BASELINE_IMPLEMENTATION, RecordValue::Address(baseline))
        .set(up::COVERAGE_METHOD, up::METHOD)
        .set(up::CHAIN_SOURCE, host(&src.chain_url))
        .set(up::ARCHIVE_SOURCE, host(&src.archive_url))
        .set(up::HEADERS_VERIFIED, c.headers)
        .set(up::BLOOM_CANDIDATE_BLOCKS, c.candidates)
        .set(
            up::CURRENT_IMPLEMENTATION_CODE_SHA256,
            RecordValue::B256(darkbot_bsc_record::sha256(&code)),
        )
        .set(up::CURRENT_IMPLEMENTATION_CODE_BYTES, code.len() as u64)
        .set(up::FIRST_PARENT_HASH, RecordValue::B256(c.first_parent_hash))
        .set(up::LAST_BLOCK_HASH, RecordValue::B256(c.last_hash));
    d.encode().expect("encode upgrade scan")
}

/// The coverage a scan record states (its upgrades as the record holds them).
fn stated_coverage(scan: &Record) -> Coverage {
    Coverage {
        from: field(scan.u64(up::SCANNED_FROM_BLOCK)),
        to: field(scan.u64(up::SCANNED_TO_BLOCK)),
        first_parent_hash: field(scan.b256(up::FIRST_PARENT_HASH)),
        last_hash: field(scan.b256(up::LAST_BLOCK_HASH)),
        headers: field(scan.u64(up::HEADERS_VERIFIED)),
        candidates: field(scan.u64(up::BLOOM_CANDIDATE_BLOCKS)),
        upgrades: scan
            .nested_all(up::UPGRADES)
            .expect("upgrades")
            .iter()
            .map(|u| UpgradeSeen {
                block: field(u.u64(up::upgrade::BLOCK)),
                block_hash: field(u.b256(up::upgrade::BLOCK_HASH)),
                transaction_hash: field(u.b256(up::upgrade::TRANSACTION_HASH)),
                log_index: field(u.u64(up::upgrade::LOG_INDEX)),
                implementation: field(u.address(up::upgrade::IMPLEMENTATION)),
                receipt_status: field(u.u64(up::upgrade::RECEIPT_STATUS)),
            })
            .collect(),
    }
}

fn print_scan(bytes: &[u8]) {
    let scan = decode(&up::SCHEMA, bytes).expect("decode upgrade scan");
    print!("{}", scan.render());
    // The scan outcome for the DRY script log (tool output, not a record view).
    println!(
        "UPGRADES={}",
        scan.nested_all(up::UPGRADES).expect("upgrades").len()
    );
}

fn scan_only(args: &[String]) {
    let from: u64 = arg(args, "--from")
        .expect("--from <block> is required")
        .parse()
        .expect("--from must be a block number");
    let out = arg(args, "--out").expect("--out <scan.dbrec> is required");
    let baseline = decode(
        &pf::SCHEMA,
        &fs::read(arg(args, "--baseline").expect("--baseline <preflight> is required"))
            .expect("read baseline"),
    )
    .expect("baseline preflight record");
    let src = sources();
    // `--to` bounds a verification run; the requalification scans to the head.
    let to = match arg(args, "--to") {
        Some(to) => to.parse().expect("--to must be a block number"),
        None => with_retries("head", rpc_block_number),
    };
    let c = cover(&src.chain_url, from, to);
    anchored(&src, &c);
    let bytes = upgrade_record(
        &src,
        &c,
        field(baseline.address(pf::PORTAL_IMPLEMENTATION)),
        implementation_at(&src, to),
    );
    write_new(&out, &bytes);
    print_scan(&bytes);
}

fn dry0(args: &[String]) {
    let out = arg(args, "--out").expect("--out <preflight.dbrec> is required");
    let scan_path = arg(args, "--upgrade-scan").expect("--upgrade-scan <scan.dbrec> is required");
    let upgrade_out = arg(args, "--upgrade-out").expect("--upgrade-out is required");
    let scan = decode(&up::SCHEMA, &fs::read(&scan_path).expect("read scan")).expect("scan record");
    assert_eq!(field(scan.text(up::COVERAGE_METHOD)), up::METHOD, "foreign coverage method");
    let src = sources();
    // Extend the coverage close to the head first, so that only a few blocks
    // remain after the preflight read.
    let mut c = stated_coverage(&scan);
    let head = with_retries("head", rpc_block_number);
    if head > c.to + HEAD_MARGIN {
        let next = cover(&src.chain_url, c.to + 1, head - HEAD_MARGIN);
        c = c.then(next);
    }

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
    print!("{}", decode(&pf::SCHEMA, &bytes).expect("decode preflight").render());

    // The coverage ends exactly at the preflight's block, whose hash ends
    // the chain.
    let preflight_head = field(current.u64(pf::LATEST_BLOCK_NUMBER));
    assert!(
        preflight_head > c.to,
        "the preflight block {preflight_head} is not beyond the coverage ({})",
        c.to
    );
    let last = cover(&src.chain_url, c.to + 1, preflight_head);
    c = c.then(last);
    assert_eq!(
        c.last_hash,
        field(current.b256(pf::LATEST_BLOCK_HASH)),
        "the preflight block hash does not end the verified chain"
    );
    anchored(&src, &c);
    let record = upgrade_record(
        &src,
        &c,
        field(scan.address(up::BASELINE_IMPLEMENTATION)),
        field(current.address(pf::PORTAL_IMPLEMENTATION)),
    );
    write_new(&upgrade_out, &record);
    print_scan(&record);
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let flags = ["--out", "--from", "--to", "--baseline", "--upgrade-scan", "--upgrade-out"];
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--upgrade-scan-only" => i += 1,
            flag if flags.contains(&flag) => i += 2,
            other => panic!("unknown argument {other}"),
        }
    }
    if args.iter().any(|a| a == "--upgrade-scan-only") {
        scan_only(&args);
    } else {
        dry0(&args);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_is_named_by_host_only() {
        assert_eq!(host("https://bsc-dataseed.bnbchain.org"), "bsc-dataseed.bnbchain.org");
        assert_eq!(host("https://user:key@node.example/v1/abc?k=1"), "node.example");
        assert_eq!(host("https://bsc-mainnet.public.blastapi.io"), "bsc-mainnet.public.blastapi.io");
    }
}

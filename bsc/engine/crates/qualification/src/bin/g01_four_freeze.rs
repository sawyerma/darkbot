//! G0.1 Four.meme contract freeze (order V2 Phase E / §15.3), read-only.
//!
//! For Four Classic (TokenManager2) and OpenFour (Registry/Core/Tools),
//! separately, this binary proves or marks UNKNOWN/UNSUPPORTED: contract
//! addresses and chain, implementation/proxy structure, deployment and
//! version, verified ABI, launch selector and calldata, BUY selectors and
//! parameters, token-address derivation, quote/state reads, and writes the
//! evidence as one binary record per family (DBREC001,
//! `darkbot.g01.four-freeze.v1`).
//!
//! Network: the free public BSC JSON-RPC (read-only; `eth_chainId` must be
//! 56) and, unless `--no-sourcify`, the free Sourcify v2 API (read-only GET).
//! No key is read, nothing is signed or sent. Every state read names the
//! reference block by hash (EIP-1898); the reference block is pinned by
//! number and hash and rechecked after the state reads. Free endpoints refuse
//! `eth_getLogs`, so canonical fixtures come from a bounded header-bloom scan
//! and `eth_getBlockReceipts` by block hash, parent-linked to the reference.
//!
//! Selectors, topics, code hashes and CREATE2 addresses are alloy's; they
//! are cross-checked against the archived 2026-09-27 G0.1 inputs (published
//! ABIs re-hashed, the independent `web3_sha3` reference), the dispatcher of
//! the implementation code and the canonical fixtures. JSON exists only in
//! `edge` (external wire and archived legacy files).
//!
//! The fixture scan ends at `--block` (default: `--confirmations` below the
//! head); the reference block is then re-pinned fresh unless `--block` is
//! given. Free endpoints keep state for only ~128 blocks, so a `--block`
//! older than that fails closed (missing trie node) and writes nothing.
//!
//! Run from the repository root:
//! `cargo run --locked -p darkbot-bsc-qualification --bin g01_four_freeze`
//! (flags: see `usage()`). Output: `bsc/native/evidence/g01/four_freeze_<UTC>/
//! {classic,openfour}.dbrec`; stdout is their derived view.
#![forbid(unsafe_code)]

#[path = "g01_four_freeze/abi.rs"]
mod abi;
#[path = "g01_common/mod.rs"]
mod common;
#[path = "g01_four_freeze/contracts.rs"]
mod contracts;

use abi::{CodeScan, Spec, SpecKind, addr, hex_bytes};
use alloy_primitives::{Address, B256, U256, keccak256};
use common::chain::{bloom_has, linked_headers};
use common::edge;
use common::sourcify::{SourcifyObs, lookup_sourcify};
use common::util::{bounded_text, number, rpc_from_config, utc_stamp};
use contracts::{
    CLASSIC_TOKEN_DEPLOYER, CLASSIC_TOKEN_TEMPLATE, CONTRACTS, CORE, CREATE_DEPLOYER,
    ContractInput, Family, HELPER3, REGISTRY, Resolution, Scope, TOKEN_MANAGER2,
};
use darkbot_bsc_execution::rpc_wire::Limits;
use darkbot_bsc_qualification::c1_common::{self, Args};
use darkbot_bsc_record::schema::bsc_four_freeze::four_freeze as ff;
use darkbot_bsc_record::{Draft, Value, decode, sha256};
use edge::{CallOutcome, Endpoint, Header, Log, Pin, Receipt, Tx};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

const TOOL: &str = "g01_four_freeze";

/// Every tunable bound and its default (each overridable by its flag). The
/// network bounds are shared with the other G0.1 freeze (`common::util::net`).
mod defaults {
    pub use crate::common::util::net::*;
    /// `--scan-blocks`: blocks searched for canonical fixtures.
    pub const SCAN_BLOCKS: u64 = 5000;
    /// `--receipt-block-budget`: blocks whose receipts may be fetched, and
    /// BUY candidate transactions that may be fetched.
    pub const RECEIPT_BLOCK_BUDGET: u64 = 300;
    /// `--launch-fixtures`: canonical launches kept per family.
    pub const LAUNCH_FIXTURES: usize = 3;
    /// `--probe-funds-wei`: the quote amount the read probes pass (0.001 BNB).
    pub const PROBE_FUNDS_WEI: u128 = 1_000_000_000_000_000;
}

fn usage() -> &'static str {
    "usage: g01_four_freeze [--rpc <url> | --rpc-config <file>] [--archive <dir>] [--out-root <dir>] \
     [--block <n>] [--confirmations <n>] [--scan-blocks <n>] [--header-batch <n>] \
     [--receipt-block-budget <n>] [--launch-fixtures <n>] [--rpc-connect-timeout-s <s>] \
     [--rpc-max-time-s <s>] [--rpc-attempts <n>] [--rpc-retry-pause-ms <ms>] \
     [--sourcify-base <url> | --no-sourcify] [--sourcify-max-bytes <n>] [--probe-funds-wei <n>]"
}

#[derive(Clone, Debug)]
struct Config {
    rpc_url: String,
    archive: PathBuf,
    out_root: PathBuf,
    block: Option<u64>,
    confirmations: u64,
    scan_blocks: u64,
    header_batch: u64,
    receipt_budget: u64,
    launch_fixtures: usize,
    limits: Limits,
    attempts: u32,
    retry_pause: Duration,
    sourcify: Option<String>,
    sourcify_max_bytes: u64,
    probe_funds: U256,
}

impl Config {
    fn parse(args: &Args) -> Result<Self, String> {
        args.only(&[
            "rpc",
            "rpc-config",
            "archive",
            "out-root",
            "block",
            "confirmations",
            "scan-blocks",
            "header-batch",
            "receipt-block-budget",
            "launch-fixtures",
            "rpc-connect-timeout-s",
            "rpc-max-time-s",
            "rpc-attempts",
            "rpc-retry-pause-ms",
            "sourcify-base",
            "no-sourcify",
            "sourcify-max-bytes",
            "probe-funds-wei",
        ])?;
        if !args.positional.is_empty() {
            return Err(usage().into());
        }
        let rpc_url = match args.opt("rpc")? {
            Some(url) => url.to_string(),
            None => match std::env::var(defaults::RPC_CONFIG_KEY) {
                Ok(url) if !url.is_empty() => url,
                _ => rpc_from_config(Path::new(
                    args.opt("rpc-config")?.unwrap_or(defaults::RPC_CONFIG),
                ))?,
            },
        };
        let no_sourcify = args.switch("no-sourcify")?;
        let sourcify = match (no_sourcify, args.opt("sourcify-base")?) {
            (true, Some(_)) => {
                return Err("--no-sourcify and --sourcify-base exclude each other".into());
            }
            (true, None) => None,
            (false, base) => Some(base.unwrap_or(defaults::SOURCIFY_BASE).to_string()),
        };
        let cfg = Self {
            rpc_url,
            archive: PathBuf::from(args.opt("archive")?.unwrap_or(contracts::ARCHIVE_DEFAULT)),
            out_root: PathBuf::from(args.opt("out-root")?.unwrap_or(defaults::OUT_ROOT)),
            block: args
                .opt("block")?
                .map(|b| b.parse().map_err(|_| format!("--block: not a number: {b}")))
                .transpose()?,
            confirmations: number(args, "confirmations", defaults::CONFIRMATIONS)?,
            scan_blocks: number(args, "scan-blocks", defaults::SCAN_BLOCKS)?,
            header_batch: number(args, "header-batch", defaults::HEADER_BATCH)?,
            receipt_budget: number(args, "receipt-block-budget", defaults::RECEIPT_BLOCK_BUDGET)?,
            launch_fixtures: number(args, "launch-fixtures", defaults::LAUNCH_FIXTURES)?,
            limits: Limits {
                connect_timeout_s: number(
                    args,
                    "rpc-connect-timeout-s",
                    defaults::CONNECT_TIMEOUT_S,
                )?,
                max_time_s: number(args, "rpc-max-time-s", defaults::MAX_TIME_S)?,
                compressed: true,
            },
            attempts: number(args, "rpc-attempts", defaults::ATTEMPTS)?,
            retry_pause: Duration::from_millis(number(
                args,
                "rpc-retry-pause-ms",
                defaults::RETRY_PAUSE_MS,
            )?),
            sourcify,
            sourcify_max_bytes: number(args, "sourcify-max-bytes", defaults::SOURCIFY_MAX_BYTES)?,
            probe_funds: U256::from(number(args, "probe-funds-wei", defaults::PROBE_FUNDS_WEI)?),
        };
        if cfg.scan_blocks == 0 || cfg.header_batch == 0 || cfg.launch_fixtures == 0 {
            return Err(
                "--scan-blocks, --header-batch and --launch-fixtures must be positive".into(),
            );
        }
        if cfg.sourcify_max_bytes > defaults::SOURCIFY_MAX_BYTES {
            return Err(format!(
                "--sourcify-max-bytes above the record bound {}",
                defaults::SOURCIFY_MAX_BYTES
            ));
        }
        Ok(cfg)
    }
}

fn main() -> ExitCode {
    let args = Args::parse(std::env::args().skip(1));
    let cfg = match Config::parse(&args) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("g01_four_freeze: {e}\n{}", usage());
            return ExitCode::from(2);
        }
    };
    match run(&cfg) {
        Ok(out) => {
            println!("{}", out.summary);
            println!("records: {}", out.dir.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("g01_four_freeze: FAILED, no record written: {e}");
            ExitCode::FAILURE
        }
    }
}

// ---- the archived 2026-09-27 inputs -------------------------------------------

struct Archive {
    root: PathBuf,
    /// (archive-relative path, role, sha256).
    inputs: Vec<(String, &'static str, [u8; 32])>,
    abis: BTreeMap<&'static str, Vec<edge::AbiEntry>>,
    keccak_reference: Vec<(String, B256)>,
    classic_block: u64,
    classic_block_hash: B256,
    classic_tx: Tx,
    classic_receipt: Receipt,
    classic_implementation: Address,
    openfour_slots: Vec<(Address, Address)>,
    codes: BTreeMap<&'static str, Vec<u8>>,
}

impl Archive {
    fn read(&mut self, path: &str, role: &'static str) -> Result<Vec<u8>, String> {
        let bytes = c1_common::read(&self.root.join(path))?;
        self.inputs.push((path.to_string(), role, sha256(&bytes)));
        Ok(bytes)
    }

    fn load(root: &Path) -> Result<Self, String> {
        let mut a = Self {
            root: root.to_path_buf(),
            inputs: Vec::new(),
            abis: BTreeMap::new(),
            keccak_reference: Vec::new(),
            classic_block: 0,
            classic_block_hash: B256::ZERO,
            classic_tx: Tx {
                hash: B256::ZERO,
                block_hash: None,
                block_number: None,
                transaction_index: None,
                from: Address::ZERO,
                to: None,
                input: vec![],
                value: U256::ZERO,
            },
            classic_receipt: Receipt {
                block_number: 0,
                block_hash: B256::ZERO,
                transaction_index: 0,
                transaction_hash: B256::ZERO,
                from: Address::ZERO,
                to: None,
                contract_address: None,
                status: 0,
                logs: vec![],
            },
            classic_implementation: Address::ZERO,
            openfour_slots: Vec::new(),
            codes: BTreeMap::new(),
        };
        for c in CONTRACTS {
            if let Some(path) = c.abi
                && !a.abis.contains_key(path)
            {
                let bytes = a.read(path, "PUBLISHED_ABI")?;
                a.abis.insert(path, edge::abi_entries(&bytes)?);
            }
            for path in [c.archived_code, c.archived_implementation_code]
                .into_iter()
                .flatten()
            {
                let text = a.read(path, "ARCHIVED_RUNTIME_CODE")?;
                let text = String::from_utf8(text).map_err(|_| format!("{path}: not text"))?;
                let digits = text.trim().trim_start_matches("0x");
                a.codes.insert(
                    path,
                    hex::decode(digits).map_err(|_| format!("{path}: not hex"))?,
                );
            }
        }
        let bytes = a.read(contracts::KECCAK_REFERENCE, "WEB3_SHA3_REFERENCE")?;
        a.keccak_reference = edge::keccak_reference(&bytes)?;
        let bytes = a.read(contracts::CLASSIC_FIXTURE_TX, "CLASSIC_LAUNCH_FIXTURE_TX")?;
        (a.classic_block, a.classic_block_hash, a.classic_tx) = edge::archived_transaction(&bytes)?;
        let bytes = a.read(
            contracts::CLASSIC_FIXTURE_RECEIPT,
            "CLASSIC_LAUNCH_FIXTURE_RECEIPT",
        )?;
        a.classic_receipt = edge::archived_receipt(&bytes)?;
        let bytes = a.read(
            contracts::CLASSIC_IMPLEMENTATION_SUMMARY,
            "CLASSIC_IMPLEMENTATION",
        )?;
        a.classic_implementation = edge::archived_classic_implementation(&bytes)?;
        let request = a.read(
            contracts::OPENFOUR_IMPLEMENTATION_SLOT_REQUEST,
            "OPENFOUR_SLOT_REQUEST",
        )?;
        let response = a.read(
            contracts::OPENFOUR_IMPLEMENTATION_SLOT_RESPONSE,
            "OPENFOUR_SLOT_RESPONSE",
        )?;
        a.openfour_slots = edge::archived_slot_reads(&request, &response)?;
        a.read(contracts::GETLOGS_REFUSAL, "GETLOGS_REFUSED_ON_FREE_RPC")?;
        Ok(a)
    }

    fn archived_implementation(&self, input: &ContractInput, address: Address) -> Option<Address> {
        if input.role == TOKEN_MANAGER2 {
            return Some(self.classic_implementation);
        }
        self.openfour_slots
            .iter()
            .find(|(proxy, _)| *proxy == address)
            .map(|(_, implementation)| *implementation)
    }
}

// ---- the fixture scan ---------------------------------------------------------------

fn fi(family: Family) -> usize {
    match family {
        Family::Classic => 0,
        Family::OpenFour => 1,
    }
}

const FAMILIES: [Family; 2] = [Family::Classic, Family::OpenFour];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Launch,
    Buy,
}

/// A canonical receipt log with its transaction.
#[derive(Clone, Debug)]
struct Found {
    family: Family,
    kind: Kind,
    source: &'static str,
    block_hash: B256,
    receipt: Receipt,
    log: Log,
    tx: Tx,
}

#[derive(Default)]
struct ScanOutcome {
    from: u64,
    search_to: u64,
    headers: u64,
    candidates: u64,
    fetched: u64,
    tx_fetched: u64,
    launches: [Vec<Found>; 2],
    buys: [Option<Found>; 2],
    launch_events: [u64; 2],
    indirect: [u64; 2],
    error: Option<String>,
}

/// Newest first: the receipts of every block whose bloom can hold a still
/// missing launch or BUY event of a family, within the budget.
fn scan(rpc: &Endpoint, cfg: &Config, headers: &[Header], targets: [Address; 2]) -> ScanOutcome {
    let mut out = ScanOutcome {
        from: headers.first().map_or(0, |h| h.number),
        search_to: headers.last().map_or(0, |h| h.number),
        headers: headers.len() as u64,
        ..ScanOutcome::default()
    };
    if let Err(e) = scan_into(rpc, cfg, headers, targets, &mut out) {
        out.error = Some(e);
    }
    out
}

fn scan_into(
    rpc: &Endpoint,
    cfg: &Config,
    headers: &[Header],
    targets: [Address; 2],
    out: &mut ScanOutcome,
) -> Result<(), String> {
    for header in headers.iter().rev() {
        let done = FAMILIES.iter().all(|f| {
            out.launches[fi(*f)].len() >= cfg.launch_fixtures && out.buys[fi(*f)].is_some()
        });
        if done || out.fetched >= cfg.receipt_budget {
            break;
        }
        let mut wants = Vec::new();
        for family in FAMILIES {
            let c = targets[fi(family)];
            if out.launches[fi(family)].len() < cfg.launch_fixtures
                && bloom_has(header, c, abi::launch_topic(family))
            {
                wants.push((family, Kind::Launch));
            }
            if out.buys[fi(family)].is_none() && bloom_has(header, c, abi::buy_topic(family)) {
                wants.push((family, Kind::Buy));
            }
        }
        if wants.is_empty() {
            continue;
        }
        out.candidates += 1;
        let mut receipts = rpc.block_receipts(header)?;
        out.fetched += 1;
        receipts.sort_by_key(|r| std::cmp::Reverse(r.transaction_index));
        for r in receipts.iter().filter(|r| r.status == 1) {
            for log in r.logs.iter().rev() {
                for &(family, kind) in &wants {
                    let c = targets[fi(family)];
                    if log.address != c {
                        continue;
                    }
                    let topic = log.topics.first().copied();
                    match kind {
                        Kind::Launch if topic == Some(abi::launch_topic(family)) => {
                            out.launch_events[fi(family)] += 1;
                            if r.to != Some(c) {
                                out.indirect[fi(family)] += 1;
                                continue;
                            }
                            let list = &out.launches[fi(family)];
                            if list.len() >= cfg.launch_fixtures
                                || list
                                    .iter()
                                    .any(|f| f.receipt.transaction_hash == r.transaction_hash)
                            {
                                continue;
                            }
                            let tx = rpc.transaction(r.transaction_hash)?;
                            edge::require_tx_at(&tx, r)?;
                            if tx.input.get(..4) != Some(&abi::launch_selector(family)[..]) {
                                continue;
                            }
                            out.launches[fi(family)].push(Found {
                                family,
                                kind,
                                source: "RECEIPT_SCAN",
                                block_hash: header.hash,
                                receipt: r.clone(),
                                log: log.clone(),
                                tx,
                            });
                        }
                        Kind::Buy if topic == Some(abi::buy_topic(family)) => {
                            if out.buys[fi(family)].is_some()
                                || r.to != Some(c)
                                || out.tx_fetched >= cfg.receipt_budget
                            {
                                continue;
                            }
                            match abi::bought(family, &log.topics, &log.data) {
                                Ok(ev) if ev.is_buy => {}
                                _ => continue,
                            }
                            let tx = rpc.transaction(r.transaction_hash)?;
                            out.tx_fetched += 1;
                            edge::require_tx_at(&tx, r)?;
                            let decoded = match family {
                                Family::Classic => abi::classic_buy(&tx.input),
                                Family::OpenFour => abi::openfour_buy(&tx.input),
                            };
                            if decoded.is_ok() {
                                out.buys[fi(family)] = Some(Found {
                                    family,
                                    kind,
                                    source: "RECEIPT_SCAN",
                                    block_hash: header.hash,
                                    receipt: r.clone(),
                                    log: log.clone(),
                                    tx,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
}

// ---- observations at the reference block -------------------------------------------

#[derive(Clone, Debug)]
struct ContractObs {
    input: &'static ContractInput,
    address: Address,
    resolution: String,
    code: Vec<u8>,
    implementation: Option<Address>,
    implementation_code: Option<Vec<u8>>,
    archived_address: Option<Address>,
    archived_implementation: Option<Address>,
    archived_code_hash: Option<B256>,
    archived_implementation_code_hash: Option<B256>,
    sourcify: Vec<SourcifyObs>,
}

impl ContractObs {
    /// The code whose dispatcher serves calls: the implementation's for a proxy.
    fn executing_code(&self) -> &[u8] {
        self.implementation_code.as_deref().unwrap_or(&self.code)
    }

    fn change(&self) -> String {
        let mut parts = Vec::new();
        if let Some(a) = self.archived_address
            && a != self.address
        {
            parts.push(format!("ADDRESS_CHANGED from {}", addr(a)));
        }
        if let Some(h) = self.archived_code_hash
            && h != keccak256(&self.code)
        {
            parts.push("CODE_CHANGED".to_string());
        }
        if let Some(a) = self.archived_implementation
            && Some(a) != self.implementation
        {
            parts.push(format!("IMPLEMENTATION_CHANGED from {}", addr(a)));
        }
        if let (Some(h), Some(code)) = (
            self.archived_implementation_code_hash,
            self.implementation_code.as_ref(),
        ) && h != keccak256(code)
        {
            parts.push("IMPLEMENTATION_CODE_CHANGED".to_string());
        }
        let archived = self.archived_address.is_some()
            || self.archived_code_hash.is_some()
            || self.archived_implementation.is_some();
        match (archived, parts.is_empty()) {
            (false, _) => "NOT_ARCHIVED".into(),
            (true, true) => "UNCHANGED".into(),
            (true, false) => parts.join("; "),
        }
    }
}

#[derive(Clone, Debug)]
struct ReadObs {
    read: abi::Read,
    dispatcher: String,
    /// `SUPPORTED` or `UNSUPPORTED` (absent from the executing dispatcher).
    status: &'static str,
    mutability: String,
    probe: String,
    output: Option<Vec<u8>>,
}

/// Helper3 `getTokenInfo(token)` words 0 and 1: (version, token manager).
type HelperInfo = Result<(U256, Address), String>;

/// Everything read at the reference block.
struct Observed {
    contracts: Vec<ContractObs>,
    /// Runtime code of every launch fixture token.
    token_codes: BTreeMap<Address, Vec<u8>>,
    /// OpenFour `getPresetTokenImpl(presetId)`.
    preset_templates: BTreeMap<U256, Result<Address, String>>,
    /// Classic Helper3 `getTokenInfo(token)`: (version, manager).
    helper_info: BTreeMap<Address, HelperInfo>,
    reads: [Vec<ReadObs>; 2],
    core_version: Option<u32>,
}

impl Observed {
    fn contract(&self, role: &str) -> Option<&ContractObs> {
        self.contracts.iter().find(|c| c.input.role == role)
    }
}

fn resolve_getter(
    rpc: &Endpoint,
    on: Address,
    signature: &str,
    pin: &Pin,
) -> Result<Address, String> {
    let call = abi::getter_call(signature).ok_or_else(|| format!("no getter {signature}"))?;
    match rpc.call_at(on, &call, pin)? {
        CallOutcome::Returned(out) => abi::address_return(&out),
        CallOutcome::Reverted(e) => Err(format!("{signature} reverted: {e}")),
    }
}

fn read_contracts(
    rpc: &Endpoint,
    archive: &Archive,
    pin: &Pin,
) -> Result<Vec<ContractObs>, String> {
    let slot = abi::eip1967_implementation_slot();
    let mut out: Vec<ContractObs> = Vec::new();
    for input in CONTRACTS {
        let (address, resolution, archived_address) = match input.resolution {
            Resolution::Table(a) => (a, format!("TABLE: {}", input.source), None),
            Resolution::Getter {
                on,
                signature,
                archived,
            } => {
                let on_address = out
                    .iter()
                    .find(|c| c.input.role == on)
                    .map(|c| c.address)
                    .ok_or_else(|| format!("{} resolves through {on}, not read yet", input.role))?;
                let a = resolve_getter(rpc, on_address, signature, pin)?;
                (
                    a,
                    format!("GETTER {on}.{signature} at the reference block"),
                    archived,
                )
            }
        };
        let code = rpc.code(address, pin)?;
        let (implementation, implementation_code) = if input.proxy {
            let word = rpc.storage(address, slot, pin)?;
            let implementation = Address::from_slice(&word[12..]);
            if word[..12] != [0u8; 12] || implementation == Address::ZERO {
                (None, None)
            } else {
                (Some(implementation), Some(rpc.code(implementation, pin)?))
            }
        } else {
            (None, None)
        };
        let archived_implementation =
            archive.archived_implementation(input, archived_address.unwrap_or(address));
        out.push(ContractObs {
            input,
            address,
            resolution,
            code,
            implementation,
            implementation_code,
            archived_address,
            archived_implementation: archived_implementation.filter(|_| input.proxy),
            archived_code_hash: input
                .archived_code
                .and_then(|p| archive.codes.get(p))
                .map(keccak256),
            archived_implementation_code_hash: input
                .archived_implementation_code
                .and_then(|p| archive.codes.get(p))
                .map(keccak256),
            sourcify: Vec::new(),
        });
    }
    Ok(out)
}

fn probe(rpc: &Endpoint, to: Address, input: &[u8], pin: &Pin) -> (String, Option<Vec<u8>>) {
    match rpc.call_at(to, input, pin) {
        Ok(CallOutcome::Returned(out)) => (format!("CALL_OK {} bytes", out.len()), Some(out)),
        Ok(CallOutcome::Reverted(e)) => (format!("REVERTED: {e}"), None),
        Err(e) => (format!("CALL_FAILED: {e}"), None),
    }
}

fn mutability(archive: &Archive, family: Family, signature: &str) -> String {
    CONTRACTS
        .iter()
        .filter(|c| c.family == family)
        .filter_map(|c| c.abi.and_then(|p| archive.abis.get(p)))
        .flatten()
        .find(|e| !e.event && e.signature == signature)
        .and_then(|e| e.mutability.clone())
        .unwrap_or_else(|| "UNKNOWN (not in an archived ABI)".into())
}

fn observe_reads(
    rpc: &Endpoint,
    archive: &Archive,
    contracts: &[ContractObs],
    family: Family,
    reads: Vec<abi::Read>,
    pin: &Pin,
) -> Vec<ReadObs> {
    reads
        .into_iter()
        .map(|read| {
            let contract = contracts.iter().find(|c| c.input.role == read.contract);
            let dispatcher = contract.map_or("NO_CONTRACT".into(), |c| {
                CodeScan::new(c.executing_code()).selector(read.selector)
            });
            let (probe_text, output) = match (&read.probe, contract) {
                (Some(input), Some(c)) => probe(rpc, c.address, input, pin),
                (None, _) => (
                    "NOT_PROBED: needs inputs the freeze does not have".into(),
                    None,
                ),
                (Some(_), None) => ("NOT_PROBED: no contract".into(), None),
            };
            ReadObs {
                mutability: mutability(archive, family, read.signature),
                read,
                status: if dispatcher.ends_with("_PRESENT") {
                    "SUPPORTED"
                } else {
                    "UNSUPPORTED"
                },
                dispatcher,
                probe: probe_text,
                output,
            }
        })
        .collect()
}

fn observe(
    rpc: &Endpoint,
    cfg: &Config,
    archive: &Archive,
    pin: &Pin,
    launches: &[Vec<Found>; 2],
) -> Result<Observed, String> {
    let contracts = read_contracts(rpc, archive, pin)?;
    let role = |r: &str| {
        contracts
            .iter()
            .find(|c| c.input.role == r)
            .map(|c| c.address)
            .ok_or_else(|| format!("{r} not read"))
    };
    let mut token_codes = BTreeMap::new();
    let mut helper_info = BTreeMap::new();
    let mut preset_templates = BTreeMap::new();
    for family in FAMILIES {
        for f in &launches[fi(family)] {
            let Ok(created) = abi::created(family, &f.log.topics, &f.log.data) else {
                continue;
            };
            token_codes.insert(created.token, rpc.code(created.token, pin)?);
            if family == Family::Classic {
                let call = abi::classic_reads(Some(created.token), U256::ZERO, None)
                    .into_iter()
                    .find(|r| r.contract == HELPER3)
                    .and_then(|r| r.probe)
                    .ok_or("no Helper3 getTokenInfo probe")?;
                let info = match rpc.call_at(role(HELPER3)?, &call, pin)? {
                    CallOutcome::Returned(out) => {
                        match (abi::u256_return(&out, 0), abi::u256_return(&out, 1)) {
                            (Some(version), Some(manager))
                                if manager < (U256::from(1u8) << 160) =>
                            {
                                Ok((
                                    version,
                                    Address::from_slice(&manager.to_be_bytes::<32>()[12..]),
                                ))
                            }
                            _ => Err(format!("getTokenInfo returned {} bytes", out.len())),
                        }
                    }
                    CallOutcome::Reverted(e) => Err(format!("getTokenInfo reverted: {e}")),
                };
                helper_info.insert(created.token, info);
            }
            if let Some(preset) = created.preset_id {
                let template = match rpc.call_at(
                    role(REGISTRY)?,
                    &abi::preset_token_impl_call(preset),
                    pin,
                )? {
                    CallOutcome::Returned(out) => abi::address_return(&out),
                    CallOutcome::Reverted(e) => Err(format!("getPresetTokenImpl reverted: {e}")),
                };
                preset_templates.insert(preset, template);
            }
        }
    }
    // Probe tokens: the newest launch of the family, else (Classic) the
    // archived fixture token.
    let newest = |family: Family| {
        launches[fi(family)]
            .iter()
            .find(|f| f.source == "RECEIPT_SCAN")
            .or_else(|| launches[fi(family)].first())
            .and_then(|f| abi::created(family, &f.log.topics, &f.log.data).ok())
    };
    let classic_token = newest(Family::Classic).map(|c| c.token);
    let ti = match classic_token {
        Some(token) => {
            let call = abi::classic_reads(Some(token), cfg.probe_funds, None)
                .into_iter()
                .next()
                .and_then(|r| r.probe)
                .ok_or("no _tokenInfos probe")?;
            match rpc.call_at(role(TOKEN_MANAGER2)?, &call, pin)? {
                CallOutcome::Returned(out) => abi::token_infos_returns(&out),
                CallOutcome::Reverted(_) => None,
            }
        }
        None => None,
    };
    let classic_reads = observe_reads(
        rpc,
        archive,
        &contracts,
        Family::Classic,
        abi::classic_reads(classic_token, cfg.probe_funds, ti),
        pin,
    );
    let openfour_created = newest(Family::OpenFour);
    let openfour_reads = observe_reads(
        rpc,
        archive,
        &contracts,
        Family::OpenFour,
        abi::openfour_reads(
            openfour_created.as_ref().map(|c| c.token),
            openfour_created.as_ref().and_then(|c| c.preset_id),
            cfg.probe_funds,
        ),
        pin,
    );
    let core_version = openfour_reads
        .iter()
        .find(|r| r.read.signature == "version()")
        .and_then(|r| r.output.as_deref())
        .and_then(|o| abi::version_return(o).ok());
    Ok(Observed {
        contracts,
        token_codes,
        preset_templates,
        helper_info,
        reads: [classic_reads, openfour_reads],
        core_version,
    })
}

// ---- fixtures and derivation -------------------------------------------------------

/// Prefix of checks that never decide a fixture (Helper3).
const VALIDATION_ONLY: &str = "validation_only.";

#[derive(Clone, Debug)]
struct FixtureView {
    found: Found,
    call_signature: String,
    event_signature: String,
    token: Address,
    account: Address,
    params: Vec<(String, String)>,
    checks: Vec<(String, bool, String)>,
    launch: Option<abi::Launch>,
}

impl FixtureView {
    /// Every check but the validation-only ones (Helper3) passed.
    fn passed(&self) -> bool {
        self.checks
            .iter()
            .filter(|(name, _, _)| !name.starts_with(VALIDATION_ONLY))
            .all(|(_, pass, _)| *pass)
    }
}

fn check(
    checks: &mut Vec<(String, bool, String)>,
    name: &str,
    pass: bool,
    detail: impl Into<String>,
) {
    checks.push((name.to_string(), pass, detail.into()));
}

fn view_launch(f: Found, contract: Address, role: &str) -> FixtureView {
    let mut checks = Vec::new();
    check(
        &mut checks,
        "receipt_status_success",
        f.receipt.status == 1,
        format!("status {}", f.receipt.status),
    );
    check(
        &mut checks,
        &format!("top_level_call_to_{role}"),
        f.tx.to == Some(contract),
        format!("to {:?}", f.tx.to.map(addr)),
    );
    check(
        &mut checks,
        &format!("event_emitted_by_{role}"),
        f.log.address == contract,
        addr(f.log.address),
    );
    let selector_ok = f.tx.input.get(..4) == Some(&abi::launch_selector(f.family)[..]);
    check(
        &mut checks,
        "selector_createToken",
        selector_ok,
        hex_bytes(f.tx.input.get(..4).unwrap_or_default()),
    );
    let launch = match f.family {
        Family::Classic => abi::classic_launch(&f.tx.input),
        Family::OpenFour => abi::openfour_launch(&f.tx.input),
    };
    check(
        &mut checks,
        "calldata_decodes",
        launch.is_ok(),
        launch
            .as_ref()
            .err()
            .cloned()
            .unwrap_or_else(|| "createToken(bytes,bytes)".into()),
    );
    let created = abi::created(f.family, &f.log.topics, &f.log.data);
    check(
        &mut checks,
        "event_decodes",
        created.is_ok(),
        created.as_ref().err().cloned().unwrap_or_default(),
    );
    let mut params = launch
        .as_ref()
        .map(|l| l.params.clone())
        .unwrap_or_default();
    let (mut token, mut account, mut event_signature) =
        (Address::ZERO, Address::ZERO, String::new());
    if let (Ok(l), Ok(c)) = (&launch, &created) {
        token = c.token;
        account = c.creator;
        event_signature = c.signature.to_string();
        check(
            &mut checks,
            "requestId_calldata_equals_event",
            l.request_id == c.request_id,
            format!("calldata {} event {}", l.request_id, c.request_id),
        );
        if f.family == Family::OpenFour {
            check(
                &mut checks,
                "presetId_calldata_equals_event",
                l.preset_id == c.preset_id,
                format!("calldata {:?} event {:?}", l.preset_id, c.preset_id),
            );
            check(
                &mut checks,
                "TokenInitParams_equal_event",
                l.init == c.init,
                "name, symbol, tokenUri=tokenMetaUri, maxSupply, saleAmount, raiseAmount, quoteAsset",
            );
        }
        params.push(("event.token".into(), addr(c.token)));
        params.push(("event.creator".into(), addr(c.creator)));
        params.push(("event.name".into(), c.name.clone()));
        params.push(("event.symbol".into(), c.symbol.clone()));
    }
    FixtureView {
        call_signature: "createToken(bytes,bytes)".into(),
        event_signature,
        token,
        account,
        params,
        checks,
        launch: launch.ok(),
        found: f,
    }
}

fn view_buy(f: Found, contract: Address, role: &str) -> FixtureView {
    let mut checks = Vec::new();
    check(
        &mut checks,
        "receipt_status_success",
        f.receipt.status == 1,
        format!("status {}", f.receipt.status),
    );
    check(
        &mut checks,
        &format!("top_level_call_to_{role}"),
        f.tx.to == Some(contract),
        format!("to {:?}", f.tx.to.map(addr)),
    );
    let decoded = match f.family {
        Family::Classic => abi::classic_buy(&f.tx.input),
        Family::OpenFour => abi::openfour_buy(&f.tx.input),
    };
    check(
        &mut checks,
        "calldata_decodes_as_BUY",
        decoded.is_ok(),
        decoded
            .as_ref()
            .map(|b| b.signature.to_string())
            .unwrap_or_else(|e| e.clone()),
    );
    let event = abi::bought(f.family, &f.log.topics, &f.log.data);
    check(
        &mut checks,
        "event_decodes",
        event.is_ok(),
        event.as_ref().err().cloned().unwrap_or_default(),
    );
    let (mut token, mut account, mut call_signature, mut event_signature) =
        (Address::ZERO, Address::ZERO, String::new(), String::new());
    let mut params = Vec::new();
    if let (Ok(b), Ok(e)) = (&decoded, &event) {
        token = e.token;
        account = e.account;
        call_signature = b.signature.to_string();
        event_signature = e.signature.to_string();
        params = b.params.clone();
        check(&mut checks, "event_is_buy", e.is_buy, "");
        check(
            &mut checks,
            "calldata_token_equals_event_token",
            b.token == e.token,
            format!("calldata {} event {}", addr(b.token), addr(e.token)),
        );
        let expected = b.recipient.unwrap_or(f.tx.from);
        check(
            &mut checks,
            "event_buyer_equals_recipient_or_sender",
            e.account == expected,
            format!(
                "event {} expected {} ({})",
                addr(e.account),
                addr(expected),
                if b.recipient.is_some() {
                    "calldata to"
                } else {
                    "tx.from"
                }
            ),
        );
    }
    FixtureView {
        call_signature,
        event_signature,
        token,
        account,
        params,
        checks,
        launch: None,
        found: f,
    }
}

#[derive(Clone, Debug)]
struct Attempt {
    tx: B256,
    salt: Option<B256>,
    template: Option<Address>,
    init_hash: Option<B256>,
    predicted: Option<Address>,
    actual: Address,
    observed_template: Option<Address>,
    matched: bool,
    detail: String,
}

#[derive(Clone, Debug)]
struct Derivation {
    status: &'static str,
    reason: String,
    deployer: Option<Address>,
    deployer_source: String,
    template: Option<Address>,
    template_source: String,
    salt_location: &'static str,
    attempts: Vec<Attempt>,
    invalidated_by: Vec<&'static str>,
}

fn attempt(
    fixture: &FixtureView,
    deployer: Option<Address>,
    template: Result<Address, String>,
    token_code: Option<&Vec<u8>>,
) -> Attempt {
    let observed_template = token_code.and_then(|c| abi::eip1167_target(c));
    let salt = fixture.launch.as_ref().map(|l| l.salt);
    let mut a = Attempt {
        tx: fixture.found.receipt.transaction_hash,
        salt,
        template: template.as_ref().ok().copied(),
        init_hash: None,
        predicted: None,
        actual: fixture.token,
        observed_template,
        matched: false,
        detail: String::new(),
    };
    match (deployer, template, salt) {
        (Some(d), Ok(t), Some(s)) => {
            let (init_hash, predicted) = abi::predict_clone(d, s, t);
            a.init_hash = Some(init_hash);
            a.predicted = Some(predicted);
            a.matched = predicted == fixture.token && observed_template == Some(t);
            a.detail = match (predicted == fixture.token, observed_template == Some(t)) {
                (true, true) => "CREATE2(deployer, calldata salt, EIP-1167 clone of template) equals the canonical token; its runtime clones that template".into(),
                (false, _) => "predicted address differs from the canonical token".into(),
                (true, false) => format!(
                    "address reproduces but the token runtime clones {:?}",
                    observed_template.map(addr)
                ),
            };
        }
        (None, _, _) => a.detail = "deployer unknown".into(),
        (_, Err(e), _) => a.detail = format!("template unknown: {e}"),
        (_, _, None) => a.detail = "salt not decoded from the calldata".into(),
    }
    a
}

fn derive(family: Family, launches: &[FixtureView], obs: &Observed) -> Derivation {
    let fixtures: Vec<&FixtureView> = launches.iter().filter(|f| f.passed()).collect();
    let mut d = match family {
        Family::Classic => {
            let deployer = obs.contract(CLASSIC_TOKEN_DEPLOYER);
            let template = obs.contract(CLASSIC_TOKEN_TEMPLATE);
            let scan = deployer.map(|c| CodeScan::new(&c.code));
            let embeds = match (&scan, template) {
                (Some(s), Some(t)) => s.embeds(t.address),
                _ => false,
            };
            let create2 = scan.as_ref().is_some_and(|s| s.create2);
            let tm2_creates = obs
                .contract(TOKEN_MANAGER2)
                .map(|c| CodeScan::new(c.executing_code()))
                .is_some_and(|s| s.create || s.create2);
            Derivation {
                status: "UNKNOWN",
                reason: String::new(),
                deployer: deployer.map(|c| c.address),
                deployer_source: format!(
                    "TABLE {CLASSIC_TOKEN_DEPLOYER}: runtime CREATE2 instruction {create2}, embeds the template {embeds}; TokenManager2 implementation creates contracts itself: {tm2_creates}"
                ),
                template: template.map(|c| c.address),
                template_source: format!("TABLE {CLASSIC_TOKEN_TEMPLATE} (immutable of the deployer runtime: {embeds})"),
                salt_location: abi::CLASSIC_SALT_LOCATION,
                attempts: fixtures
                    .iter()
                    .map(|f| {
                        attempt(
                            f,
                            deployer.map(|c| c.address),
                            template.map(|c| c.address).ok_or_else(|| "no template".to_string()),
                            obs.token_codes.get(&f.token),
                        )
                    })
                    .collect(),
                invalidated_by: vec![
                    "TokenManager2 implementation change (code hash)",
                    "TokenManager2's token deployer pointer (storage layout unverified: every canonical launch re-verifies it)",
                    "token deployer runtime change (code hash) or another template",
                    "launch modes that use another template or the deployer's CREATE path: UNKNOWN",
                    "createToken.args layout change (salt position)",
                ],
            }
            .with_gate(create2 && embeds, "the deployer runtime holds no CREATE2 instruction or not the template")
        }
        Family::OpenFour => {
            let deployer = obs.contract(CREATE_DEPLOYER);
            let template = fixtures
                .first()
                .and_then(|f| f.launch.as_ref())
                .and_then(|l| l.preset_id)
                .and_then(|p| obs.preset_templates.get(&p))
                .and_then(|r| r.as_ref().ok().copied());
            let create2 = deployer
                .map(|c| CodeScan::new(c.executing_code()))
                .is_some_and(|s| s.create2);
            Derivation {
                status: "UNKNOWN",
                reason: String::new(),
                deployer: deployer.map(|c| c.address),
                deployer_source: format!(
                    "OpenFourRegistry.createDeployer() at the reference block (implementation runtime CREATE2 instruction {create2})"
                ),
                template,
                template_source: "OpenFourRegistry.getPresetTokenImpl(presetId of the create calldata) at the reference block".into(),
                salt_location: abi::OPENFOUR_SALT_LOCATION,
                attempts: fixtures
                    .iter()
                    .map(|f| {
                        let preset = f.launch.as_ref().and_then(|l| l.preset_id);
                        let template = preset
                            .and_then(|p| obs.preset_templates.get(&p).cloned())
                            .unwrap_or_else(|| Err("preset not resolved".into()));
                        attempt(f, deployer.map(|c| c.address), template, obs.token_codes.get(&f.token))
                    })
                    .collect(),
                invalidated_by: vec![
                    "OpenFourRegistry.createDeployer() change or its implementation change",
                    "OpenFourRegistry.getPresetTokenImpl(presetId) change",
                    "OpenFourCore implementation change (code hash)",
                    "createArgs / TokenInitParams layout change",
                ],
            }
            .with_gate(create2, "the create deployer implementation holds no CREATE2 instruction")
        }
    };
    let n = d.attempts.len();
    let matched = d.attempts.iter().filter(|a| a.matched).count();
    if d.reason.is_empty() {
        if n > 0 && matched == n {
            d.status = "PROVEN";
            d.reason = format!(
                "{matched}/{n} canonical launches reproduce: token = CREATE2(deployer, salt from {}, keccak256(EIP-1167 creation code of the template)); the salt is in the pending calldata, deployer and template are pre-execution state",
                d.salt_location
            );
            if family == Family::Classic {
                d.reason.push_str(
                    "; scope: the template above only, other templates or the CREATE path stay UNKNOWN",
                );
            }
        } else if n == 0 {
            d.reason = "no verified canonical launch fixture to reproduce".into();
        } else {
            d.reason = format!("only {matched}/{n} canonical launches reproduce");
        }
    }
    d
}

impl Derivation {
    fn with_gate(mut self, ok: bool, why: &str) -> Self {
        if !ok {
            self.reason = why.to_string();
        }
        self
    }
}

// ---- selectors, properties and the record ------------------------------------------

#[derive(Clone, Debug)]
struct SelectorObs {
    spec: Spec,
    abi_archive: String,
    keccak_reference: String,
    dispatcher: String,
    fixture: &'static str,
}

fn selectors(
    family: Family,
    archive: &Archive,
    obs: &Observed,
    fixtures: &[FixtureView],
) -> Vec<SelectorObs> {
    abi::specs()
        .into_iter()
        .filter(|s| s.family == family)
        .map(|spec| {
            let contract = obs.contract(spec.contract);
            let abi_path = contracts::input(spec.contract).abi;
            let abi_archive = match abi_path.and_then(|p| archive.abis.get(p)) {
                None => "NO_ARCHIVED_ABI".to_string(),
                Some(entries) => match entries.iter().find(|e| {
                    e.signature == spec.signature && e.event == (spec.kind == SpecKind::Event)
                }) {
                    None => "NOT_IN_ARCHIVED_ABI".into(),
                    Some(e)
                        if keccak256(e.signature.as_bytes())[..spec.value.len()]
                            == spec.value[..] =>
                    {
                        format!("MATCH ({})", abi_path.unwrap_or_default())
                    }
                    Some(_) => "MISMATCH".into(),
                },
            };
            let keccak_reference = match archive
                .keccak_reference
                .iter()
                .find(|(label, _)| label == spec.signature)
            {
                None => "NOT_IN_REFERENCE".to_string(),
                Some((_, h)) if h[..spec.value.len()] == spec.value[..] => {
                    "MATCH (archived web3_sha3)".into()
                }
                Some(_) => "MISMATCH".into(),
            };
            let dispatcher = match (contract, spec.kind) {
                (None, _) => "NO_CONTRACT".to_string(),
                (Some(c), SpecKind::Function) => {
                    let selector: [u8; 4] = spec.value[..4].try_into().unwrap_or_default();
                    CodeScan::new(c.executing_code()).selector(selector)
                }
                (Some(c), SpecKind::Event) => {
                    if CodeScan::new(c.executing_code()).has_word(B256::from_slice(&spec.value)) {
                        "TOPIC_PUSH32_PRESENT".into()
                    } else {
                        "TOPIC_NOT_A_PUSH32".into()
                    }
                }
            };
            let seen = fixtures
                .iter()
                .filter(|f| f.passed())
                .any(|f| match spec.kind {
                    SpecKind::Function => f.found.tx.input.get(..4) == Some(&spec.value[..]),
                    SpecKind::Event => {
                        f.found.log.topics.first().map(|t| &t[..]) == Some(&spec.value[..])
                    }
                });
            SelectorObs {
                spec,
                abi_archive,
                keccak_reference,
                dispatcher,
                fixture: if seen {
                    "SEEN_IN_CANONICAL_FIXTURE"
                } else {
                    "NOT_OBSERVED"
                },
            }
        })
        .collect()
}

struct Common {
    chain_id: u64,
    pin: Pin,
    timestamp: u64,
    pin_rechecked: bool,
    endpoint_host: String,
    sourcify_host: Option<String>,
    tool_sha256: [u8; 32],
    git_head: Option<String>,
    archive_root: String,
    observed_ms: u64,
    receipt_budget: u64,
    linked_to: u64,
}

struct FamilyEval<'a> {
    family: Family,
    archive: &'a Archive,
    contracts: Vec<&'a ContractObs>,
    selectors: Vec<SelectorObs>,
    reads: &'a [ReadObs],
    fixtures: Vec<FixtureView>,
    derivation: Derivation,
    scan: &'a ScanOutcome,
    helper_info: Vec<(Address, HelperInfo)>,
    core_version: Option<u32>,
    sourcify_enabled: bool,
}

fn proven(ok: bool) -> &'static str {
    if ok { "PROVEN" } else { "UNKNOWN" }
}

fn surface_role(family: Family) -> &'static str {
    match family {
        Family::Classic => TOKEN_MANAGER2,
        Family::OpenFour => CORE,
    }
}

impl FamilyEval<'_> {
    fn launch_fixtures(&self) -> Vec<&FixtureView> {
        self.fixtures
            .iter()
            .filter(|f| f.found.kind == Kind::Launch)
            .collect()
    }

    fn buy_fixture(&self) -> Option<&FixtureView> {
        self.fixtures.iter().find(|f| f.found.kind == Kind::Buy)
    }

    fn launch_status(&self, budget: u64) -> String {
        let n = self
            .launch_fixtures()
            .iter()
            .filter(|f| f.found.source == "RECEIPT_SCAN")
            .count();
        if n > 0 {
            format!("FOUND {n}")
        } else if let Some(e) = &self.scan.error {
            format!("UNKNOWN: scan stopped: {e}")
        } else {
            format!(
                "UNKNOWN: no direct canonical launch within blocks {}..={} (receipt blocks fetched {}/{})",
                self.scan.from, self.scan.search_to, self.scan.fetched, budget
            )
        }
    }

    fn buy_status(&self) -> String {
        match (self.buy_fixture(), &self.scan.error) {
            (Some(_), _) => "FOUND".into(),
            (None, Some(e)) => format!("UNKNOWN: scan stopped: {e}"),
            (None, None) => format!(
                "UNKNOWN: no direct canonical BUY within blocks {}..={}",
                self.scan.from, self.scan.search_to
            ),
        }
    }

    fn selectors_with(&self, role: &str) -> Vec<&SelectorObs> {
        self.selectors
            .iter()
            .filter(|s| s.spec.role == role)
            .collect()
    }

    fn properties(&self, common: &Common) -> Vec<(String, &'static str, String)> {
        let mut out = Vec::new();
        let surface: Vec<&ContractObs> = self
            .contracts
            .iter()
            .copied()
            .filter(|c| c.input.scope != Scope::LegacyNotFrozen)
            .collect();
        // 1. Addresses and chain.
        let codes_ok = surface.iter().all(|c| !c.code.is_empty());
        let resolved: Vec<String> = surface
            .iter()
            .filter(|c| matches!(c.input.resolution, Resolution::Getter { .. }))
            .map(|c| format!("{}={} ({})", c.input.role, addr(c.address), c.resolution))
            .collect();
        out.push((
            "contract_addresses_and_chain".into(),
            proven(
                common.chain_id == edge::BSC_CHAIN_ID
                    && common.pin_rechecked
                    && !surface.is_empty()
                    && codes_ok,
            ),
            format!(
                "eth_chainId={} at block {} {} (pin rechecked {}); runtime code present at {}/{} contracts [{}]{}",
                common.chain_id,
                common.pin.number,
                common.pin.hash,
                common.pin_rechecked,
                surface.iter().filter(|c| !c.code.is_empty()).count(),
                surface.len(),
                surface
                    .iter()
                    .map(|c| format!("{} {} len {}", c.input.role, addr(c.address), c.code.len()))
                    .collect::<Vec<_>>()
                    .join(", "),
                if resolved.is_empty() { String::new() } else { format!("; resolved: {}", resolved.join(", ")) }
            ),
        ));
        // 2. Implementation / proxy structure.
        let proxies: Vec<&ContractObs> = surface
            .iter()
            .copied()
            .filter(|c| c.input.proxy && c.input.scope == Scope::FrozenSurface)
            .collect();
        // A proxy runtime is confirmed by its own Sourcify verification with
        // an agreeing proxy resolution, or by being byte-identical to such a
        // verified proxy runtime of this family.
        let verified_runtimes: Vec<(B256, &str, String)> = self
            .contracts
            .iter()
            .filter(|c| c.input.proxy)
            .filter_map(|c| {
                verified_proxy_source(c).map(|s| {
                    let name = s.facts.as_ref().ok().and_then(|f| f.contract_name.clone());
                    (keccak256(&c.code), c.input.role, name.unwrap_or_default())
                })
            })
            .collect();
        let mut proxy_ok = !proxies.is_empty();
        let mut lines = Vec::new();
        for c in &proxies {
            let source = c.sourcify.iter().find(|s| s.address == c.address);
            let twin = verified_runtimes
                .iter()
                .find(|(h, role, _)| *h == keccak256(&c.code) && *role != c.input.role);
            let confirmed = verified_proxy_source(c).is_some() || twin.is_some();
            let impl_code = c
                .implementation_code
                .as_ref()
                .is_some_and(|code| !code.is_empty());
            proxy_ok &= confirmed && impl_code;
            lines.push(format!(
                "{} -> EIP-1967 implementation {} (code {}), proxy source {}",
                c.input.role,
                c.implementation.map_or("NONE".into(), addr),
                c.implementation_code.as_ref().map_or(0, Vec::len),
                match (source.map(|s| &s.facts), twin) {
                    (Some(Ok(f)), _) if verified_proxy_source(c).is_some() => format!(
                        "{} {} on Sourcify, proxyResolution agrees",
                        f.status,
                        f.contract_name.clone().unwrap_or_default()
                    ),
                    (_, Some((h, role, name))) => format!(
                        "runtime keccak {h} identical to the Sourcify-verified {role} proxy runtime ({name}); own Sourcify status {}",
                        match source.map(|s| &s.facts) {
                            Some(Ok(f)) => f.status.clone(),
                            Some(Err(e)) => e.clone(),
                            None => "not looked up".into(),
                        }
                    ),
                    (Some(Ok(f)), None) => format!("not confirmed: Sourcify {}", f.status),
                    (Some(Err(e)), None) => format!("not confirmed: {e}"),
                    (None, None) if !self.sourcify_enabled => "not checked (--no-sourcify)".into(),
                    (None, None) => "not looked up".into(),
                }
            ));
        }
        out.push((
            "implementation_proxy_structure".into(),
            proven(proxy_ok),
            lines.join("; "),
        ));
        // 3. Deployment and version.
        let impl_hashes: Vec<String> = proxies
            .iter()
            .filter_map(|c| {
                c.implementation_code
                    .as_ref()
                    .map(|code| format!("{} impl keccak {}", c.input.role, keccak256(code)))
            })
            .collect();
        let deployments: Vec<String> = proxies
            .iter()
            .flat_map(|c| {
                c.sourcify
                    .iter()
                    .filter(|s| s.address == c.address)
                    .map(move |s| (c, s))
            })
            .map(|(c, s)| {
                format!(
                    "{} creation {}",
                    c.input.role,
                    match &s.facts {
                        Ok(f) if f.deployment_tx.is_some() => format!(
                            "tx {} block {:?}: {}",
                            f.deployment_tx.unwrap_or_default(),
                            f.deployment_block,
                            s.deployment_receipt
                        ),
                        Ok(_) => "UNKNOWN (Sourcify has no deployment)".into(),
                        Err(_) => "UNKNOWN".into(),
                    }
                )
            })
            .collect();
        let deployment_mismatch = proxies.iter().flat_map(|c| c.sourcify.iter()).any(|s| {
            s.deployment_receipt
                .starts_with("CONTRACT_ADDRESS_MISMATCH")
        });
        let (version_ok, version_text) = match self.family {
            Family::Classic => {
                let checked: Vec<String> = self
                    .helper_info
                    .iter()
                    .map(|(t, r)| match r {
                        Ok((v, m)) => format!("{}: version {v} manager {}", addr(*t), addr(*m)),
                        Err(e) => format!("{}: {e}", addr(*t)),
                    })
                    .collect();
                let tm2 = contracts::table_address(TOKEN_MANAGER2);
                let ok = !self.helper_info.is_empty()
                    && self.helper_info.iter().all(
                        |(_, r)| matches!(r, Ok((v, m)) if *v == U256::from(2u8) && *m == tm2),
                    );
                (
                    ok,
                    format!(
                        "version label 2 = TokenManager2 by Helper3.getTokenInfo (validation only) for {}/{} launch tokens [{}]",
                        self.helper_info.iter().filter(|(_, r)| matches!(r, Ok((v, m)) if *v == U256::from(2u8) && *m == tm2)).count(),
                        self.helper_info.len(),
                        checked.join(", ")
                    ),
                )
            }
            Family::OpenFour => (
                self.core_version.is_some(),
                format!(
                    "OpenFourCore.version() = {:?} at the reference block",
                    self.core_version
                ),
            ),
        };
        out.push((
            "deployment_and_version".into(),
            proven(version_ok && impl_hashes.len() == proxies.len() && !deployment_mismatch),
            format!(
                "{version_text}; {}; {}",
                impl_hashes.join(", "),
                deployments.join(", ")
            ),
        ));
        // 4. Verified ABI (source verification of the executing code).
        let mut verified = !proxies.is_empty();
        let mut lines = Vec::new();
        for c in &proxies {
            let target = c.implementation.unwrap_or(c.address);
            let s = c.sourcify.iter().find(|s| s.address == target);
            let wanted: Vec<&str> = self
                .selectors
                .iter()
                .filter(|x| x.spec.contract == c.input.role && x.spec.kind == SpecKind::Function)
                .map(|x| x.spec.signature)
                .collect();
            let ok = match s.map(|s| (&s.facts, s.verified())) {
                Some((Ok(f), true)) => wanted
                    .iter()
                    .all(|w| f.abi_signatures.iter().any(|a| a == w)),
                _ => false,
            };
            verified &= ok;
            lines.push(format!(
                "{} implementation {}: {}",
                c.input.role,
                addr(target),
                match s.map(|s| &s.facts) {
                    Some(Ok(f)) => format!(
                        "Sourcify {}{}",
                        f.status,
                        if ok {
                            ", frozen signatures in the verified ABI"
                        } else {
                            ""
                        }
                    ),
                    Some(Err(e)) => format!("Sourcify unavailable: {e}"),
                    None => "not looked up".into(),
                }
            ));
        }
        out.push((
            "verified_abi".into(),
            proven(verified),
            format!(
                "{}; BscScan not used (API key required); bytecode consistency of the published ABI: see abi_dispatcher_consistency",
                lines.join("; ")
            ),
        ));
        // 5. ABI/dispatcher consistency: the launch/BUY/SELL surface must be
        // in the executing dispatcher; every other published function absent
        // from it is UNSUPPORTED.
        let frozen: Vec<&SelectorObs> = self
            .selectors
            .iter()
            .filter(|s| {
                s.spec.kind == SpecKind::Function
                    && matches!(s.spec.role, "LAUNCH" | "BUY" | "SELL")
                    && contracts::input(s.spec.contract).scope == Scope::FrozenSurface
            })
            .collect();
        let present = frozen
            .iter()
            .filter(|s| s.dispatcher.ends_with("_PRESENT"))
            .count();
        let archived: Vec<&SelectorObs> = self
            .selectors
            .iter()
            .filter(|s| s.abi_archive != "NO_ARCHIVED_ABI")
            .collect();
        let abi_ok = archived.iter().all(|s| s.abi_archive.starts_with("MATCH"));
        let reference_ok = self
            .selectors
            .iter()
            .all(|s| s.keccak_reference != "MISMATCH");
        let unsupported: Vec<String> = self
            .selectors
            .iter()
            .filter(|s| s.spec.kind == SpecKind::Function && s.dispatcher == "ABSENT")
            .map(|s| format!("{}.{}", s.spec.contract, s.spec.signature))
            .collect();
        let published: Vec<String> = proxies
            .iter()
            .chain(
                surface
                    .iter()
                    .filter(|c| !c.input.proxy && c.input.abi.is_some()),
            )
            .map(|c| abi_coverage(c, self.archive).1)
            .collect();
        out.push((
            "abi_dispatcher_consistency".into(),
            proven(!frozen.is_empty() && present == frozen.len() && abi_ok && reference_ok),
            format!(
                "{present}/{} launch/BUY/SELL selectors in the executing dispatcher; {}/{} frozen signatures re-hash equal to the archived published ABI: {abi_ok}; archived web3_sha3 reference agrees: {reference_ok}; UNSUPPORTED (frozen signature absent from the executing dispatcher): [{}]; {}",
                frozen.len(),
                archived.iter().filter(|s| s.abi_archive.starts_with("MATCH")).count(),
                archived.len(),
                unsupported.join(", "),
                published.join("; ")
            ),
        ));
        // 6. Launch selector and calldata.
        let launch_sel = self.selectors_with("LAUNCH");
        let launch_present = launch_sel
            .iter()
            .filter(|s| s.spec.contract == surface_role(self.family))
            .all(|s| s.dispatcher.ends_with("_PRESENT"));
        let launches = self.launch_fixtures();
        let passed: Vec<&&FixtureView> = launches.iter().filter(|f| f.passed()).collect();
        out.push((
            "launch_selectors_and_calldata".into(),
            proven(launch_present && !passed.is_empty() && passed.len() == launches.len()),
            format!(
                "createToken(bytes,bytes) {} in {} dispatcher: {launch_present}; canonical launches verified {}/{} [{}]; scan: {}; calldata claims: {}",
                launch_sel.first().map_or(String::new(), |s| hex_bytes(&s.spec.value)),
                surface_role(self.family),
                passed.len(),
                launches.len(),
                launches
                    .iter()
                    .map(|f| format!("{} block {} ({})", f.found.receipt.transaction_hash, f.found.receipt.block_number, f.found.source))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.launch_status(common.receipt_budget),
                match self.family {
                    Family::Classic => "outer (bytes args, bytes signature); args word 0 = 0x20, word 1 = requestId (= event), word 2 = CREATE2 salt (reproduced); other args fields UNKNOWN",
                    Family::OpenFour => "outer (bytes createArgs, bytes signature); head: requestId, presetId (= event), five unnamed static words, TokenInitParams offset 0x100; TokenInitParams equal the TokenCreated fields; tokenSalt = CREATE2 salt (reproduced)",
                }
            ),
        ));
        // 7. BUY selectors and parameters.
        let buy_sel = self.selectors_with("BUY");
        let buy_present = buy_sel
            .iter()
            .filter(|s| s.dispatcher.ends_with("_PRESENT"))
            .count();
        let buy = self.buy_fixture();
        out.push((
            "buy_selectors_and_parameters".into(),
            proven(buy_present == buy_sel.len() && !buy_sel.is_empty() && buy.is_some_and(FixtureView::passed)),
            format!(
                "{buy_present}/{} BUY selectors in the dispatcher [{}]; canonical direct BUY: {}; token and buyer positions checked against the event, amount/limit semantics UNKNOWN (Phase G)",
                buy_sel.len(),
                buy_sel
                    .iter()
                    .map(|s| format!("{} {}", s.spec.signature, hex_bytes(&s.spec.value)))
                    .collect::<Vec<_>>()
                    .join(", "),
                match buy {
                    Some(f) => format!(
                        "{} tx {} block {} checks {}",
                        f.call_signature,
                        f.found.receipt.transaction_hash,
                        f.found.receipt.block_number,
                        if f.passed() { "PASS" } else { "FAIL" }
                    ),
                    None => self.buy_status(),
                }
            ),
        ));
        // 8. Token address derivation.
        out.push((
            "token_address_derivation".into(),
            self.derivation.status,
            self.derivation.reason.clone(),
        ));
        // 9. Quote and state requirements: the reads the executing code
        // serves (SUPPORTED) and the published ones it does not (UNSUPPORTED).
        let gating: Vec<&ReadObs> = self
            .reads
            .iter()
            .filter(|r| r.read.purpose != "VALIDATION")
            .collect();
        let list = |status: &str| {
            gating
                .iter()
                .filter(|r| r.status == status)
                .map(|r| format!("{}.{} [{}]", r.read.contract, r.read.signature, r.probe))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let quote_supported = gating
            .iter()
            .any(|r| r.read.purpose == "QUOTE" && r.status == "SUPPORTED");
        let failed = self
            .reads
            .iter()
            .any(|r| r.probe.starts_with("CALL_FAILED"));
        let contradiction = gating
            .iter()
            .any(|r| r.status == "UNSUPPORTED" && r.probe.starts_with("CALL_OK"));
        out.push((
            "quote_and_state_requirements".into(),
            proven(quote_supported && !failed && !contradiction),
            format!(
                "SUPPORTED (in the executing dispatcher): {}; UNSUPPORTED (published, absent from the executing dispatcher): [{}]; Helper3 reads are validation only; economic semantics of every output UNKNOWN (Phase G)",
                list("SUPPORTED"),
                list("UNSUPPORTED")
            ),
        ));
        // 10. Versioned evidence.
        out.push((
            "versioned_evidence".into(),
            "PROVEN",
            format!(
                "this record: {} (type {} v{}); archive inputs SHA-256-bound; Sourcify responses SHA-256-bound; tool SHA-256 {}; git HEAD {}",
                ff::SCHEMA.name,
                ff::SCHEMA.record_type,
                ff::SCHEMA.schema_version,
                B256::from(common.tool_sha256),
                common.git_head.as_deref().unwrap_or("UNKNOWN")
            ),
        ));
        out
    }
}

/// The Sourcify lookup that verifies `c` as a proxy of its EIP-1967
/// implementation.
fn verified_proxy_source(c: &ContractObs) -> Option<&SourcifyObs> {
    c.sourcify.iter().find(|s| s.address == c.address).filter(|s| {
        s.verified()
            && matches!((&s.facts, c.implementation), (Ok(f), Some(i)) if f.proxy_implementations.contains(&i))
    })
}

/// Functions of a contract's published ABI present in its executing code.
fn abi_coverage(c: &ContractObs, archive: &Archive) -> (Option<(u64, u64, Vec<String>)>, String) {
    let Some(path) = c.input.abi else {
        return (None, format!("{}: no archived ABI", c.input.role));
    };
    let entries = archive.abis.get(path).cloned().unwrap_or_default();
    let scan = CodeScan::new(c.executing_code());
    let functions: Vec<&edge::AbiEntry> = entries.iter().filter(|e| !e.event).collect();
    let absent: Vec<String> = functions
        .iter()
        .filter(|e| {
            let h = keccak256(e.signature.as_bytes());
            scan.selector([h[0], h[1], h[2], h[3]]) == "ABSENT"
        })
        .map(|e| e.signature.clone())
        .collect();
    let n = functions.len() as u64;
    let present = n - absent.len() as u64;
    (
        Some((n, present, absent.clone())),
        format!(
            "{} published ABI {path}: {present}/{n} functions in the dispatcher{}",
            c.input.role,
            if absent.is_empty() {
                String::new()
            } else {
                format!(" (absent: {})", absent.join(", "))
            }
        ),
    )
}

fn b256(v: B256) -> Value {
    Value::B256(v.0)
}

fn address(v: Address) -> Value {
    Value::Address(v.0.0)
}

fn sourcify_draft(s: &SourcifyObs) -> Draft {
    use ff::sourcify as k;
    let mut d = Draft::new(&k::SCHEMA);
    d.set(k::ADDRESS, address(s.address))
        .set(k::DEPLOYMENT_RECEIPT, s.deployment_receipt.as_str());
    match &s.facts {
        Ok(f) => {
            d.set(k::STATUS, f.status.as_str())
                .set_opt(k::CREATION_MATCH, f.creation_match.clone())
                .set_opt(k::RUNTIME_MATCH, f.runtime_match.clone())
                .set_opt(k::CONTRACT_NAME, f.contract_name.clone())
                .set_opt(k::COMPILER_VERSION, f.compiler_version.clone())
                .set_opt(k::DEPLOYMENT_TX, f.deployment_tx.map(b256))
                .set_opt(k::DEPLOYMENT_BLOCK, f.deployment_block)
                .set_opt(k::DEPLOYER, f.deployer.map(address));
            for i in &f.proxy_implementations {
                d.push(k::PROXY_IMPLEMENTATIONS, address(*i));
            }
            for sig in &f.abi_signatures {
                d.push(k::ABI_SIGNATURES, sig.as_str());
            }
        }
        Err(e) => {
            d.set(k::STATUS, "UNAVAILABLE").set(k::ERROR, e.as_str());
        }
    }
    if let Some(raw) = &s.response {
        d.set(k::RESPONSE_SHA256, Value::B256(sha256(raw)))
            .set(k::RESPONSE, Value::Bytes(raw.clone()));
    }
    d
}

fn contract_draft(c: &ContractObs, archive: &Archive) -> Draft {
    use ff::contract as k;
    let mut d = Draft::new(&k::SCHEMA);
    d.set(k::ROLE, c.input.role)
        .set(k::ADDRESS, address(c.address))
        .set(k::RESOLUTION, c.resolution.as_str())
        .set(k::SCOPE, c.input.scope.label())
        .set(k::CODE_KECCAK256, b256(keccak256(&c.code)))
        .set(k::CODE_LEN, c.code.len() as u64)
        .set(k::CODE_SHA256, Value::B256(sha256(&c.code)))
        .set_opt(k::IMPLEMENTATION, c.implementation.map(address))
        .set_opt(
            k::IMPLEMENTATION_CODE_KECCAK256,
            c.implementation_code
                .as_ref()
                .map(|code| b256(keccak256(code))),
        )
        .set_opt(
            k::IMPLEMENTATION_CODE_LEN,
            c.implementation_code.as_ref().map(|code| code.len() as u64),
        )
        .set_opt(k::ARCHIVED_ADDRESS, c.archived_address.map(address))
        .set_opt(
            k::ARCHIVED_IMPLEMENTATION,
            c.archived_implementation.map(address),
        )
        .set_opt(k::ARCHIVED_CODE_KECCAK256, c.archived_code_hash.map(b256))
        .set_opt(
            k::ARCHIVED_IMPLEMENTATION_CODE_KECCAK256,
            c.archived_implementation_code_hash.map(b256),
        )
        .set(k::CHANGE, c.change())
        .set(
            k::CREATE_OPCODES,
            CodeScan::new(c.executing_code()).create_opcodes(),
        );
    if let (Some((n, present, absent)), _) = abi_coverage(c, archive) {
        d.set_opt(k::ABI_SOURCE, c.input.abi)
            .set(k::ABI_FUNCTIONS, n)
            .set(k::ABI_FUNCTIONS_IN_DISPATCHER, present);
        for a in absent {
            d.push(k::ABI_FUNCTIONS_ABSENT, a);
        }
    }
    for s in &c.sourcify {
        d.push(k::SOURCIFY, sourcify_draft(s).into_nested());
    }
    d
}

fn fixture_draft(f: &FixtureView) -> Draft {
    use ff::fixture as k;
    let mut d = Draft::new(&k::SCHEMA);
    d.set(
        k::KIND,
        match f.found.kind {
            Kind::Launch => "LAUNCH",
            Kind::Buy => "BUY",
        },
    )
    .set(k::SOURCE, f.found.source)
    .set(k::TX_HASH, b256(f.found.receipt.transaction_hash))
    .set(k::BLOCK_NUMBER, f.found.receipt.block_number)
    .set(k::BLOCK_HASH, b256(f.found.block_hash))
    .set(k::TX_INDEX, f.found.receipt.transaction_index)
    .set(k::LOG_INDEX, f.found.log.log_index)
    .set(k::RECEIPT_STATUS, f.found.receipt.status)
    .set(k::FROM, address(f.found.tx.from))
    .set(k::TO, address(f.found.tx.to.unwrap_or_default()))
    .set(k::VALUE_WEI, Value::U256(f.found.tx.value.to_be_bytes()))
    .set(k::INPUT, Value::Bytes(f.found.tx.input.clone()))
    .set(
        k::SELECTOR,
        Value::Bytes(f.found.tx.input.get(..4).unwrap_or_default().to_vec()),
    )
    .set(k::CALL_SIGNATURE, f.call_signature.as_str())
    .set(k::EVENT_SIGNATURE, f.event_signature.as_str())
    .set(k::LOG_DATA, Value::Bytes(f.found.log.data.clone()))
    .set(k::TOKEN, address(f.token))
    .set(k::ACCOUNT, address(f.account));
    for t in &f.found.log.topics {
        d.push(k::LOG_TOPICS, b256(*t));
    }
    for (name, value) in &f.params {
        let mut p = Draft::new(&ff::param::SCHEMA);
        p.set(ff::param::NAME, name.as_str())
            .set(ff::param::VALUE, value.as_str());
        d.push(k::PARAMS, p.into_nested());
    }
    for (name, pass, detail) in &f.checks {
        let mut c = Draft::new(&ff::check::SCHEMA);
        c.set(ff::check::NAME, name.as_str())
            .set(ff::check::PASS, *pass)
            .set(ff::check::DETAIL, detail.as_str());
        d.push(k::CHECKS, c.into_nested());
    }
    d
}

fn derivation_draft(x: &Derivation) -> Draft {
    use ff::derivation as k;
    let mut d = Draft::new(&k::SCHEMA);
    d.set(k::STATUS, x.status)
        .set(k::REASON, x.reason.as_str())
        .set(k::METHOD, "CREATE2_EIP1167_CLONE")
        .set_opt(k::DEPLOYER, x.deployer.map(address))
        .set(k::DEPLOYER_SOURCE, x.deployer_source.as_str())
        .set_opt(k::TEMPLATE, x.template.map(address))
        .set(k::TEMPLATE_SOURCE, x.template_source.as_str())
        .set(k::SALT_LOCATION, x.salt_location);
    for a in &x.attempts {
        use ff::derivation_attempt as t;
        let mut e = Draft::new(&t::SCHEMA);
        e.set(t::FIXTURE_TX, b256(a.tx))
            .set_opt(t::SALT, a.salt.map(b256))
            .set_opt(t::TEMPLATE, a.template.map(address))
            .set_opt(t::INIT_CODE_HASH, a.init_hash.map(b256))
            .set_opt(t::PREDICTED, a.predicted.map(address))
            .set(t::ACTUAL, address(a.actual))
            .set_opt(t::OBSERVED_TEMPLATE, a.observed_template.map(address))
            .set(t::MATCH, a.matched)
            .set(t::DETAIL, a.detail.as_str());
        d.push(k::ATTEMPTS, e.into_nested());
    }
    for i in &x.invalidated_by {
        d.push(k::INVALIDATED_BY, *i);
    }
    d
}

const NON_CLAIMS: &[&str] = &[
    "READ_ONLY: no key read, nothing signed or sent",
    "NOT_GATE0_PASS: G0.1 evidence only; no economic or operational release",
    "ECONOMICS_SEMANTICS_UNKNOWN: quote outputs are not interpreted (Phase G)",
    "NOT_A_LAUNCH_CENSUS: the fixture scan is bounded and proves presence, never absence",
    "NO_PENDING_COVERAGE: canonical fixtures only; pending visibility is not measured",
    "HELPER3_VALIDATION_ONLY: never a hot-path dependency",
];

fn family_draft(
    eval: &FamilyEval,
    common: &Common,
    archive: &Archive,
) -> (Draft, Vec<(String, &'static str, String)>) {
    let mut d = Draft::new(&ff::SCHEMA);
    d.set(ff::FAMILY, eval.family.label())
        .set(ff::CHAIN_ID, common.chain_id)
        .set(ff::PROVENANCE_TOOL, TOOL)
        .set(ff::PROVENANCE_OBSERVED_UNIX_MS, common.observed_ms)
        .set(ff::REFERENCE_BLOCK, common.pin.number)
        .set(ff::REFERENCE_BLOCK_HASH, b256(common.pin.hash))
        .set(ff::REFERENCE_TIMESTAMP, common.timestamp)
        .set(ff::PIN_RECHECKED, common.pin_rechecked)
        .set(ff::ENDPOINT_HOST, common.endpoint_host.as_str())
        .set_opt(ff::SOURCIFY_HOST, common.sourcify_host.clone())
        .set(ff::TOOL_SHA256, Value::B256(common.tool_sha256))
        .set_opt(ff::GIT_HEAD, common.git_head.clone())
        .set(ff::ARCHIVE_ROOT, common.archive_root.as_str());
    for (path, role, sha) in &archive.inputs {
        let mut a = Draft::new(&ff::archive_input::SCHEMA);
        a.set(ff::archive_input::PATH, path.as_str())
            .set(ff::archive_input::ROLE, *role)
            .set(ff::archive_input::SHA256, Value::B256(*sha));
        d.push(ff::ARCHIVE_INPUTS, a.into_nested());
    }
    for c in &eval.contracts {
        d.push(ff::CONTRACTS, contract_draft(c, archive).into_nested());
    }
    for s in &eval.selectors {
        use ff::selector as k;
        let mut e = Draft::new(&k::SCHEMA);
        e.set(
            k::KIND,
            match s.spec.kind {
                SpecKind::Function => "FUNCTION",
                SpecKind::Event => "EVENT",
            },
        )
        .set(k::ROLE, s.spec.role)
        .set(k::CONTRACT, s.spec.contract)
        .set(k::SIGNATURE, s.spec.signature)
        .set(k::VALUE, Value::Bytes(s.spec.value.clone()))
        .set(
            k::DERIVATION,
            match s.spec.kind {
                SpecKind::Function => abi::SELECTOR_DERIVATION,
                SpecKind::Event => abi::TOPIC_DERIVATION,
            },
        )
        .set(k::ABI_ARCHIVE, s.abi_archive.as_str())
        .set(k::KECCAK_REFERENCE, s.keccak_reference.as_str())
        .set(k::DISPATCHER, s.dispatcher.as_str())
        .set(k::FIXTURE, s.fixture);
        d.push(ff::SELECTORS, e.into_nested());
    }
    for r in eval.reads {
        use ff::state_read as k;
        let mut e = Draft::new(&k::SCHEMA);
        e.set(k::CONTRACT, r.read.contract)
            .set(k::SIGNATURE, r.read.signature)
            .set(k::SELECTOR, Value::Bytes(r.read.selector.to_vec()))
            .set(k::PURPOSE, r.read.purpose)
            .set(k::MUTABILITY, r.mutability.as_str())
            .set(k::DISPATCHER, r.dispatcher.as_str())
            .set(k::PROBE, r.probe.as_str())
            .set_opt(k::PROBE_INPUT, r.read.probe.clone().map(Value::Bytes))
            .set_opt(k::OUTPUT, r.output.clone().map(Value::Bytes))
            .set(k::SEMANTICS, r.read.semantics)
            .set(k::STATUS, r.status);
        d.push(ff::STATE_READS, e.into_nested());
    }
    for f in &eval.fixtures {
        d.push(ff::FIXTURES, fixture_draft(f).into_nested());
    }
    d.set(
        ff::DERIVATION,
        derivation_draft(&eval.derivation).into_nested(),
    );
    {
        use ff::scan as k;
        let s = eval.scan;
        let mut e = Draft::new(&k::SCHEMA);
        e.set(k::METHOD, "HEADER_LOGSBLOOM(contract, topic0) THEN eth_getBlockReceipts(blockHash), NEWEST FIRST; direct top-level calls only")
            .set(k::FROM_BLOCK, s.from)
            .set(k::SEARCH_TO_BLOCK, s.search_to)
            .set(k::LINKED_TO_BLOCK, common.linked_to)
            .set(k::HEADERS, s.headers)
            .set(k::PARENT_LINKED, true)
            .set(k::BLOOM_CANDIDATE_BLOCKS, s.candidates)
            .set(k::RECEIPT_BLOCKS_FETCHED, s.fetched)
            .set(k::RECEIPT_BLOCK_BUDGET, common.receipt_budget)
            .set(k::LAUNCH_FIXTURE, eval.launch_status(common.receipt_budget))
            .set(k::BUY_FIXTURE, eval.buy_status())
            .set(k::LAUNCH_EVENTS_SEEN, s.launch_events[fi(eval.family)])
            .set(k::INDIRECT_LAUNCHES_SEEN, s.indirect[fi(eval.family)]);
        d.set(ff::SCAN, e.into_nested());
    }
    let properties = eval.properties(common);
    for (name, status, evidence) in &properties {
        let mut p = Draft::new(&ff::property::SCHEMA);
        p.set(ff::property::NAME, name.as_str())
            .set(ff::property::STATUS, *status)
            .set(ff::property::EVIDENCE, bounded_text(evidence));
        d.push(ff::PROPERTIES, p.into_nested());
    }
    let complete = properties.iter().all(|(_, s, _)| *s == "PROVEN");
    d.set(
        ff::FREEZE_STATUS,
        if complete { "COMPLETE" } else { "PARTIAL" },
    );
    for n in NON_CLAIMS {
        d.push(ff::NON_CLAIMS, *n);
    }
    (d, properties)
}

// ---- the run --------------------------------------------------------------------------

struct Outcome {
    dir: PathBuf,
    summary: String,
}

fn run(cfg: &Config) -> Result<Outcome, String> {
    let archive = Archive::load(&cfg.archive)?;
    let rpc = Endpoint::new(&cfg.rpc_url, cfg.limits, cfg.attempts, cfg.retry_pause);
    let chain_id = rpc.chain_id()?;
    edge::require_bsc(chain_id)?;

    // Fixture search anchor: the given block, else `confirmations` below the head.
    let head = rpc.block_number()?;
    let anchor = match cfg.block {
        Some(b) => b,
        None => head
            .checked_sub(cfg.confirmations)
            .ok_or("head below the confirmation depth")?,
    };
    let from = anchor.saturating_sub(cfg.scan_blocks - 1);
    eprintln!("g01_four_freeze: chain 56, head {head}; scanning headers {from}..={anchor}");
    let anchor_pin = edge::pin_of(anchor, &rpc.header(anchor)?)?;
    let registry = contracts::table_address(REGISTRY);
    let scan_core = resolve_getter(&rpc, registry, "openFourCore()", &anchor_pin)?;
    let mut headers = linked_headers(&rpc, from, anchor, cfg.header_batch)?;
    if headers.last().map(|h| h.hash) != Some(anchor_pin.hash) {
        return Err("the scanned headers do not end at the pinned anchor".into());
    }
    let targets = [contracts::table_address(TOKEN_MANAGER2), scan_core];
    let mut scanned = scan(&rpc, cfg, &headers, targets);
    eprintln!(
        "g01_four_freeze: {} candidate blocks, {} receipt blocks fetched{}",
        scanned.candidates,
        scanned.fetched,
        scanned
            .error
            .as_ref()
            .map_or(String::new(), |e| format!(", scan stopped: {e}"))
    );

    // The reference block: fresh (state of free endpoints reaches ~128
    // blocks back), parent-linked to the scanned window.
    let reference = match cfg.block {
        Some(b) => b,
        None => anchor.max(
            rpc.block_number()?
                .checked_sub(cfg.confirmations)
                .ok_or("head below the confirmation depth")?,
        ),
    };
    if reference > anchor {
        let extension = linked_headers(&rpc, anchor + 1, reference, cfg.header_batch)?;
        if extension.first().map(|h| h.parent_hash) != Some(anchor_pin.hash) {
            return Err("the headers after the scan do not link to its anchor".into());
        }
        headers.extend(extension);
    }
    let reference_header = headers.last().cloned().ok_or("no headers")?;
    let pin = edge::pin_of(reference, &reference_header)?;
    if rpc.header(reference)?.hash != pin.hash {
        return Err(format!("block {reference} changed while pinning (reorg)"));
    }
    eprintln!(
        "g01_four_freeze: reference block {} {}",
        pin.number, pin.hash
    );

    // The archived Classic fixture, re-read live.
    let archived = {
        let tx_hash: B256 = contracts::CLASSIC_FIXTURE_TX_HASH
            .parse()
            .map_err(|_| "bad fixture hash")?;
        let receipt = rpc.receipt(tx_hash)?;
        let tx = rpc.transaction(tx_hash)?;
        edge::require_tx_at(&tx, &receipt)?;
        let canonical = rpc.header(receipt.block_number)?;
        let log = receipt
            .logs
            .iter()
            .find(|l| {
                l.address == targets[0]
                    && l.topics.first() == Some(&abi::launch_topic(Family::Classic))
            })
            .cloned()
            .ok_or("archived fixture receipt has no TokenCreate log")?;
        (
            Found {
                family: Family::Classic,
                kind: Kind::Launch,
                source: "ARCHIVED_2026_09_27_REVERIFIED",
                block_hash: canonical.hash,
                receipt: receipt.clone(),
                log,
                tx,
            },
            canonical,
        )
    };
    scanned.launches[0].insert(0, archived.0.clone());

    // Every state read at the reference block, then the pin recheck.
    let mut obs = observe(&rpc, cfg, &archive, &pin, &scanned.launches)?;
    if obs.contract(CORE).map(|c| c.address) != Some(scan_core) {
        return Err("OpenFourCore changed between the scan anchor and the reference block".into());
    }
    let pin_rechecked = rpc.header(reference)?.hash == pin.hash;

    // Sourcify (not chain state): every address and implementation.
    if let Some(base) = &cfg.sourcify {
        for c in &mut obs.contracts {
            for a in [Some(c.address), c.implementation].into_iter().flatten() {
                c.sourcify.push(lookup_sourcify(
                    &rpc,
                    cfg.limits,
                    cfg.sourcify_max_bytes,
                    base,
                    a,
                ));
            }
        }
    }

    let observed_ms = darkbot_bsc_record::observed_now()?;
    let common = Common {
        chain_id,
        pin,
        timestamp: reference_header.timestamp,
        pin_rechecked,
        endpoint_host: rpc.host(),
        sourcify_host: cfg.sourcify.as_deref().map(edge::host),
        tool_sha256: c1_common::own_sha256()?,
        git_head: c1_common::git_head(Path::new(".")),
        archive_root: cfg.archive.display().to_string(),
        observed_ms,
        receipt_budget: cfg.receipt_budget,
        linked_to: reference,
    };

    let mut drafts = Vec::new();
    let mut summary = format!(
        "G0.1 Four freeze at block {} {} (chain {chain_id}, endpoint {})\n",
        pin.number, pin.hash, common.endpoint_host
    );
    for family in FAMILIES {
        let surface = obs
            .contract(surface_role(family))
            .map(|c| c.address)
            .ok_or("surface contract missing")?;
        let mut fixtures: Vec<FixtureView> = scanned.launches[fi(family)]
            .iter()
            .cloned()
            .map(|f| view_launch(f, surface, surface_role(family)))
            .collect();
        if family == Family::Classic {
            let (a, canonical) = (&archived.0, &archived.1);
            let archived_log = archive
                .classic_receipt
                .logs
                .iter()
                .find(|l| l.topics.first() == Some(&abi::launch_topic(Family::Classic)));
            let archived_created = archived_log
                .ok_or_else(|| "no TokenCreate log in the archived receipt".to_string())
                .and_then(|l| abi::created(Family::Classic, &l.topics, &l.data));
            if let Some(v) = fixtures.iter_mut().find(|f| f.found.source == a.source) {
                check(
                    &mut v.checks,
                    "archived_block_canonical",
                    canonical.hash == archive.classic_block_hash
                        && canonical.number == archive.classic_block,
                    format!(
                        "live block {} {} archived {} {}",
                        canonical.number,
                        canonical.hash,
                        archive.classic_block,
                        archive.classic_block_hash
                    ),
                );
                check(
                    &mut v.checks,
                    "live_calldata_equals_archived",
                    a.tx.input == archive.classic_tx.input,
                    format!("{} bytes", a.tx.input.len()),
                );
                check(
                    &mut v.checks,
                    "live_receipt_equals_archived",
                    a.receipt.transaction_index == archive.classic_receipt.transaction_index
                        && a.receipt.status == archive.classic_receipt.status
                        && Some(a.log.log_index) == archived_log.map(|l| l.log_index),
                    format!(
                        "index {} status {} log {}",
                        a.receipt.transaction_index, a.receipt.status, a.log.log_index
                    ),
                );
                check(
                    &mut v.checks,
                    "token_and_creator_equal_archived",
                    archived_created
                        .as_ref()
                        .is_ok_and(|c| c.token == v.token && c.creator == v.account),
                    archived_created
                        .as_ref()
                        .map(|c| {
                            format!(
                                "archived token {} creator {}",
                                addr(c.token),
                                addr(c.creator)
                            )
                        })
                        .unwrap_or_else(|e| e.clone()),
                );
            }
            let tm2 = contracts::table_address(TOKEN_MANAGER2);
            for v in fixtures.iter_mut() {
                if let Some(info) = obs.helper_info.get(&v.token) {
                    check(
                        &mut v.checks,
                        &format!("{VALIDATION_ONLY}helper3_getTokenInfo_version2_TokenManager2"),
                        matches!(info, Ok((ver, m)) if *ver == U256::from(2u8) && *m == tm2),
                        match info {
                            Ok((ver, m)) => format!("version {ver} manager {}", addr(*m)),
                            Err(e) => e.clone(),
                        },
                    );
                }
            }
        }
        if let Some(b) = scanned.buys[fi(family)].clone() {
            fixtures.push(view_buy(b, surface, surface_role(family)));
        }
        let launches: Vec<FixtureView> = fixtures
            .iter()
            .filter(|f| f.found.kind == Kind::Launch)
            .cloned()
            .collect();
        let derivation = derive(family, &launches, &obs);
        let eval = FamilyEval {
            family,
            archive: &archive,
            contracts: obs
                .contracts
                .iter()
                .filter(|c| c.input.family == family)
                .collect(),
            selectors: selectors(family, &archive, &obs, &fixtures),
            reads: &obs.reads[fi(family)],
            fixtures,
            derivation,
            scan: &scanned,
            helper_info: if family == Family::Classic {
                obs.helper_info
                    .iter()
                    .map(|(t, r)| (*t, r.clone()))
                    .collect()
            } else {
                Vec::new()
            },
            core_version: obs.core_version,
            sourcify_enabled: cfg.sourcify.is_some(),
        };
        let (draft, properties) = family_draft(&eval, &common, &archive);
        summary.push_str(&format!("\n{}:\n", family.label()));
        for (name, status, evidence) in &properties {
            summary.push_str(&format!(
                "  {status:<8} {name}: {}\n",
                bounded_text(evidence)
            ));
        }
        drafts.push((family, draft));
    }

    // Encode everything before anything is written.
    let mut encoded = Vec::new();
    for (family, draft) in &drafts {
        let bytes = draft
            .encode()
            .map_err(|e| format!("{}: {e}", family.label()))?;
        decode(&ff::SCHEMA, &bytes).map_err(|e| format!("{}: {e}", family.label()))?;
        encoded.push((family.label().to_lowercase(), bytes));
    }
    std::fs::create_dir_all(&cfg.out_root)
        .map_err(|e| format!("{}: {e}", cfg.out_root.display()))?;
    let dir = cfg
        .out_root
        .join(format!("four_freeze_{}", utc_stamp(observed_ms)));
    std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (name, bytes) in &encoded {
        let path = dir.join(format!("{name}.dbrec"));
        c1_common::write_new(&path, bytes)?;
        summary.push_str(&format!(
            "\n{} sha256 {} ({} bytes)",
            path.display(),
            B256::from(sha256(bytes)),
            bytes.len()
        ));
    }
    Ok(Outcome { dir, summary })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
    }

    fn archive() -> Archive {
        Archive::load(&repo_root().join(contracts::ARCHIVE_DEFAULT)).unwrap()
    }

    /// The archived canonical Classic launch reproduces offline: the salt
    /// of its pending calldata, the table deployer and template give the
    /// token of its TokenCreate event.
    #[test]
    fn the_archived_classic_launch_reproduces_its_token_offline() {
        let a = archive();
        let launch = abi::classic_launch(&a.classic_tx.input).unwrap();
        let log = a
            .classic_receipt
            .logs
            .iter()
            .find(|l| l.topics.first() == Some(&abi::launch_topic(Family::Classic)))
            .unwrap();
        let created = abi::created(Family::Classic, &log.topics, &log.data).unwrap();
        assert_eq!(launch.request_id, created.request_id);
        assert_eq!(created.symbol, "YING");
        let (_, predicted) = abi::predict_clone(
            contracts::table_address(CLASSIC_TOKEN_DEPLOYER),
            launch.salt,
            contracts::table_address(CLASSIC_TOKEN_TEMPLATE),
        );
        assert_eq!(predicted, created.token);
        assert_eq!(
            addr(predicted),
            "0xa89a24209a3ae8ba1ffb3f0567b69ef4765bffff"
        );
        // Another salt, deployer or template does not reproduce.
        let (_, other) = abi::predict_clone(
            contracts::table_address(TOKEN_MANAGER2),
            launch.salt,
            contracts::table_address(CLASSIC_TOKEN_TEMPLATE),
        );
        assert_ne!(other, created.token);
        let (_, other) = abi::predict_clone(
            contracts::table_address(CLASSIC_TOKEN_DEPLOYER),
            B256::from(U256::from_be_bytes(launch.salt.0) + U256::from(1u8)),
            contracts::table_address(CLASSIC_TOKEN_TEMPLATE),
        );
        assert_ne!(other, created.token);
    }

    /// Every frozen signature with an archived ABI is in it and re-hashes
    /// to alloy's value; every one in the archived web3_sha3 reference
    /// agrees with it.
    #[test]
    fn frozen_signatures_agree_with_the_archived_abis_and_reference() {
        let a = archive();
        let mut in_reference = 0;
        for spec in abi::specs() {
            if let Some(path) = contracts::input(spec.contract).abi {
                let entry = a.abis[path]
                    .iter()
                    .find(|e| e.signature == spec.signature)
                    .unwrap_or_else(|| panic!("{} not in {path}", spec.signature));
                assert_eq!(entry.event, spec.kind == SpecKind::Event);
                assert_eq!(
                    keccak256(entry.signature.as_bytes())[..spec.value.len()],
                    spec.value[..]
                );
            }
            if let Some((_, h)) = a.keccak_reference.iter().find(|(l, _)| l == spec.signature) {
                assert_eq!(h[..spec.value.len()], spec.value[..], "{}", spec.signature);
                in_reference += 1;
            }
        }
        // createToken (Classic and OpenFour share the signature), 4 BUY
        // variants, TokenCreate, TokenPurchase.
        assert_eq!(in_reference, 8);
    }

    /// The archived implementation runtimes hold the frozen launch, BUY and
    /// SELL selectors in their dispatchers (PUSH4), and not a foreign one.
    #[test]
    fn archived_implementation_dispatchers_hold_the_frozen_selectors() {
        let a = archive();
        let code = |p: &str| CodeScan::new(&a.codes[p]);
        let tm2 = code("fresh_primary/four.implementation.code.hex");
        let core = code("openfour/core.implementation.code.hex");
        let tools = code("openfour/tools.implementation.code.hex");
        for spec in abi::specs().iter().filter(|s| s.kind == SpecKind::Function) {
            let scan = match spec.contract {
                TOKEN_MANAGER2 => &tm2,
                CORE => &core,
                contracts::TOOLS => &tools,
                _ => continue,
            };
            if matches!(spec.role, "LAUNCH" | "BUY" | "SELL") {
                let selector: [u8; 4] = spec.value[..4].try_into().unwrap();
                assert_eq!(
                    scan.selector(selector),
                    "PUSH4_PRESENT",
                    "{}",
                    spec.signature
                );
            }
        }
        assert_eq!(tm2.selector([0xde, 0xad, 0xbe, 0xef]), "ABSENT");
        // TokenManager2 creates no contract itself.
        assert!(!tm2.create && !tm2.create2);
        // Its launch event topic is a PUSH32 of the runtime.
        assert!(tm2.has_word(abi::launch_topic(Family::Classic)));
    }

    /// A family without any observation fails closed: every property but
    /// the record's own versioning is UNKNOWN, and the record round-trips.
    #[test]
    fn an_empty_family_is_unknown_and_its_record_round_trips() {
        let a = archive();
        let scan = ScanOutcome {
            from: 1,
            search_to: 2,
            headers: 2,
            ..ScanOutcome::default()
        };
        let obs = Observed {
            contracts: Vec::new(),
            token_codes: BTreeMap::new(),
            preset_templates: BTreeMap::new(),
            helper_info: BTreeMap::new(),
            reads: [Vec::new(), Vec::new()],
            core_version: None,
        };
        let common = Common {
            chain_id: 56,
            pin: Pin {
                number: 2,
                hash: B256::repeat_byte(2),
            },
            timestamp: 1,
            pin_rechecked: true,
            endpoint_host: "bsc-dataseed.bnbchain.org".into(),
            sourcify_host: None,
            tool_sha256: [7; 32],
            git_head: None,
            archive_root: "archive".into(),
            observed_ms: 1,
            receipt_budget: 1,
            linked_to: 2,
        };
        let eval = FamilyEval {
            family: Family::OpenFour,
            archive: &a,
            contracts: Vec::new(),
            selectors: selectors(Family::OpenFour, &a, &obs, &[]),
            reads: &[],
            fixtures: Vec::new(),
            derivation: derive(Family::OpenFour, &[], &obs),
            scan: &scan,
            helper_info: Vec::new(),
            core_version: None,
            sourcify_enabled: false,
        };
        let (draft, properties) = family_draft(&eval, &common, &a);
        assert_eq!(properties.len(), 10);
        for (name, status, _) in &properties {
            let expected = if name == "versioned_evidence" {
                "PROVEN"
            } else {
                "UNKNOWN"
            };
            assert_eq!(*status, expected, "{name}");
        }
        let bytes = draft.encode().unwrap();
        let record = decode(&ff::SCHEMA, &bytes).unwrap();
        assert_eq!(record.text(ff::FREEZE_STATUS).unwrap(), "PARTIAL");
        assert!(
            record
                .nested(ff::SCAN)
                .unwrap()
                .text(ff::scan::LAUNCH_FIXTURE)
                .unwrap()
                .starts_with("UNKNOWN")
        );
        assert_eq!(
            record
                .nested(ff::DERIVATION)
                .unwrap()
                .text(ff::derivation::STATUS)
                .unwrap(),
            "UNKNOWN"
        );
        assert_eq!(draft.encode().unwrap(), bytes);
    }

    #[test]
    fn json_stays_inside_the_edge() {
        for (name, source) in [
            ("main", include_str!("g01_four_freeze.rs")),
            ("abi", include_str!("g01_four_freeze/abi.rs")),
            ("contracts", include_str!("g01_four_freeze/contracts.rs")),
            ("common", include_str!("g01_common/mod.rs")),
        ] {
            assert!(!source.contains(&["serde", "_json"].concat()), "{name}");
            assert!(!source.contains(&["json", "!"].concat()), "{name}");
        }
        let edge = include_str!("g01_common/edge.rs");
        for line in edge.lines() {
            if line.trim_start().starts_with("pub ") {
                assert!(!line.contains("Json"), "{line}");
            }
        }
        // The RPC goes through the shared stdin transport, never argv.
        assert!(edge.contains("rpc_wire::post("));
        for argv in ["\"--data\"", "\"--data-binary\"", "\"-H\""] {
            assert!(!edge.contains(argv));
        }
    }

    #[test]
    fn utc_stamps_are_civil_time() {
        assert_eq!(utc_stamp(0), "19700101T000000Z");
        assert_eq!(utc_stamp(1_792_231_935_242), "20261017T101215Z");
        assert_eq!(utc_stamp(951_782_400_000), "20000229T000000Z");
    }

    #[test]
    fn a_secret_file_is_never_read_as_rpc_config() {
        assert!(rpc_from_config(Path::new("bsc/secrets.env")).is_err());
        let ok = rpc_from_config(&repo_root().join(defaults::RPC_CONFIG)).unwrap();
        assert!(ok.starts_with("https://"));
    }

    /// The real freeze against the configured free endpoint (network).
    #[test]
    #[ignore = "live read-only BSC RPC and Sourcify"]
    fn live_freeze_against_the_free_rpc() {
        let out = std::env::temp_dir().join(format!("g01-four-freeze-live-{}", std::process::id()));
        let args = Args::parse([
            "--archive".to_string(),
            repo_root()
                .join(contracts::ARCHIVE_DEFAULT)
                .display()
                .to_string(),
            "--rpc-config".into(),
            repo_root().join(defaults::RPC_CONFIG).display().to_string(),
            "--out-root".into(),
            out.display().to_string(),
            "--scan-blocks".into(),
            "300".into(),
        ]);
        let cfg = Config::parse(&args).unwrap();
        let outcome = run(&cfg).unwrap();
        for name in ["classic", "openfour"] {
            let bytes = std::fs::read(outcome.dir.join(format!("{name}.dbrec"))).unwrap();
            let record = decode(&ff::SCHEMA, &bytes).unwrap();
            assert_eq!(record.u64(ff::CHAIN_ID).unwrap(), 56);
            assert!(record.bool(ff::PIN_RECHECKED).unwrap());
        }
        std::fs::remove_dir_all(out).unwrap();
    }
}

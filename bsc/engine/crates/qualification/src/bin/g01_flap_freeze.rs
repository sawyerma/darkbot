//! G0.1 / Phase D Flap refreeze (Phase D contract §3), read-only.
//!
//! Freezes the current Flap Portal deployment on BSC Mainnet against the
//! witnessed constants of the production code
//! (`trading::venues::flap::deployment::bsc_mainnet_v5_24_0`): chain id,
//! Portal proxy, EIP-1967 implementation and `version()`, keccak and SHA-256
//! code fingerprints of proxy, implementation and tax-V3 implementation,
//! Sourcify verification, the launch/BUY/SELL selectors and event topics
//! (alloy, cross-checked against the archived official IPortal sources, the
//! verified ABI, the implementation dispatcher and helper code), canonical
//! launch/BUY/SELL/DEX/revert fixtures, the live version/mode census and the
//! token-address reproduction per token version. One binary record
//! (DBREC001, `darkbot.g01.flap-freeze.v1`).
//!
//! Network: the free public BSC JSON-RPC (read-only; `eth_chainId` must be
//! 56) and, unless `--no-sourcify`, the free Sourcify v2 API. No key is read,
//! nothing is signed or sent. Every state read names the reference block by
//! hash (EIP-1898), rechecked after the state reads. The fixture scan reads
//! the receipts of blocks whose header bloom can hold a Portal event, newest
//! first, within `--scan-blocks` and `--receipt-block-budget`; it is
//! parent-linked to the reference block. Free endpoints keep state for only
//! ~128 blocks: a `--block` older than that fails closed and writes nothing.
//!
//! Run from the repository root:
//! `cargo run --locked -p darkbot-bsc-qualification --bin g01_flap_freeze`
//! (flags: `usage()`). Output: `bsc/native/evidence/g01/flap_freeze_<UTC>/
//! flap.dbrec`; stdout is a derived summary.
#![forbid(unsafe_code)]

#[path = "g01_flap_freeze/abi.rs"]
mod abi;
#[path = "g01_common/mod.rs"]
mod common;

use abi::{Spec, SpecKind, addr, hex_bytes, solidity};
use alloy_primitives::{Address, B256, keccak256};
use common::bytecode::{CodeScan, eip1167_target, eip1967_implementation_slot, predict_clone};
use common::chain::{bloom_has, linked_headers};
use common::edge::{self, CallOutcome, Endpoint, Header, Log, Pin, Receipt, Tx};
use common::sourcify::{SourcifyObs, lookup_sourcify};
use common::util::{bounded_text, number, rpc_from_config, utc_stamp};
use darkbot_bsc_execution::rpc_wire::Limits;
use darkbot_bsc_qualification::c1_common::{self, Args};
use darkbot_bsc_record::schema::bsc_flap_freeze::flap_freeze as ff;
use darkbot_bsc_record::schema::bsc_four_freeze::four_freeze as shared;
use darkbot_bsc_record::{Draft, Value, decode, sha256};
use darkbot_bsc_trading::venues::flap::abi as production;
use darkbot_bsc_trading::venues::flap::deployment::{FlapDeployment, bsc_mainnet_v5_24_0};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

const TOOL: &str = "g01_flap_freeze";

/// The archived evidence inputs (relative to `--archive`). Contract
/// addresses come from the witnessed deployment of the production code
/// (`bsc_mainnet_v5_24_0()`); helper contracts are discovered from the
/// implementation's immutables.
mod inputs {
    pub const ARCHIVE_DEFAULT: &str = "prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence";
    /// flap-sh/FlapVaultExample@5949cc7e `src/flap/IPortal.sol`.
    pub const IPORTAL_OFFICIAL: &str = "2026-09-28-g02-economics/sources/official_IPortal.sol";
    /// incjanta/Portal@bed46ef0 `src/interfaces/IPortal.sol`.
    pub const IPORTAL_INTERFACES: &str = "2026-09-28-g02-economics/sources/interfaces_IPortal.sol";
    /// The independent `web3_sha3` reference of the 2026-09-27 run.
    pub const KECCAK_REFERENCE: &str = "2026-09-27-g01/keccak_comparison.json";
    /// The decoded Gate-0 launch (block 124415357) re-verified live.
    pub const HISTORICAL_LAUNCH: &str = "2026-09-27-g01/flap_v6_fixture_decoded.json";
    /// The archived v5.24.0 implementation runtime (offline tests).
    #[cfg(test)]
    pub const ARCHIVED_IMPLEMENTATION_CODE: &str =
        "2026-09-27-g01/fresh_primary/flap.implementation.code.hex";
    /// The enum whose members name the token versions.
    pub const TOKEN_VERSION_ENUM: &str = "TokenVersion";
}

/// Every tunable bound and its default (each overridable by its flag). The
/// network bounds are shared with the other G0.1 freeze.
mod defaults {
    pub use crate::common::util::net::*;
    /// `--scan-blocks`: blocks searched for fixtures and the census.
    pub const SCAN_BLOCKS: u64 = 2000;
    /// `--receipt-block-budget`: blocks whose receipts may be fetched.
    pub const RECEIPT_BLOCK_BUDGET: u64 = 400;
    /// `--launch-fixtures`, `--buy-fixtures`: fully recorded fixtures (the
    /// order requires at least two of each).
    pub const LAUNCH_FIXTURES: usize = 3;
    pub const BUY_FIXTURES: usize = 2;
    /// `--sell-fixtures`, `--dex-fixtures`, `--reverted-fixtures`: recorded
    /// if any occur.
    pub const SELL_FIXTURES: usize = 1;
    pub const DEX_FIXTURES: usize = 1;
    pub const REVERTED_FIXTURES: usize = 1;
    /// `--max-helpers`: implementation immutables read as helper code.
    pub const MAX_HELPERS: usize = 32;
    /// `--revert-search-blocks`: extra blocks with Portal activity whose
    /// receipts may be read only to find a reverted Portal call (outside the
    /// census budget).
    pub const REVERT_SEARCH_BLOCKS: u64 = 150;
}

/// Levels of address immutables searched for helper code (the
/// implementation's own, and those of its helpers).
const HELPER_DEPTH: usize = 2;

/// The order's minimum fixture counts (Phase D refreeze).
const REQUIRED_LAUNCHES: usize = 2;
const REQUIRED_BUYS: usize = 2;

fn usage() -> &'static str {
    "usage: g01_flap_freeze [--rpc <url> | --rpc-config <file>] [--archive <dir>] [--out-root <dir>] \
     [--block <n>] [--confirmations <n>] [--scan-blocks <n>] [--header-batch <n>] \
     [--receipt-block-budget <n>] [--launch-fixtures <n>] [--buy-fixtures <n>] [--sell-fixtures <n>] \
     [--dex-fixtures <n>] [--reverted-fixtures <n>] [--revert-search-blocks <n>] [--max-helpers <n>] \
     [--rpc-connect-timeout-s <s>] \
     [--rpc-max-time-s <s>] [--rpc-attempts <n>] [--rpc-retry-pause-ms <ms>] \
     [--sourcify-base <url> | --no-sourcify] [--sourcify-max-bytes <n>]"
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
    buy_fixtures: usize,
    sell_fixtures: usize,
    dex_fixtures: usize,
    reverted_fixtures: usize,
    revert_search_blocks: u64,
    max_helpers: usize,
    limits: Limits,
    attempts: u32,
    retry_pause: Duration,
    sourcify: Option<String>,
    sourcify_max_bytes: u64,
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
            "buy-fixtures",
            "sell-fixtures",
            "dex-fixtures",
            "reverted-fixtures",
            "revert-search-blocks",
            "max-helpers",
            "rpc-connect-timeout-s",
            "rpc-max-time-s",
            "rpc-attempts",
            "rpc-retry-pause-ms",
            "sourcify-base",
            "no-sourcify",
            "sourcify-max-bytes",
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
        let sourcify = match (args.switch("no-sourcify")?, args.opt("sourcify-base")?) {
            (true, Some(_)) => {
                return Err("--no-sourcify and --sourcify-base exclude each other".into());
            }
            (true, None) => None,
            (false, base) => Some(base.unwrap_or(defaults::SOURCIFY_BASE).to_string()),
        };
        let cfg = Self {
            rpc_url,
            archive: PathBuf::from(args.opt("archive")?.unwrap_or(inputs::ARCHIVE_DEFAULT)),
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
            buy_fixtures: number(args, "buy-fixtures", defaults::BUY_FIXTURES)?,
            sell_fixtures: number(args, "sell-fixtures", defaults::SELL_FIXTURES)?,
            dex_fixtures: number(args, "dex-fixtures", defaults::DEX_FIXTURES)?,
            reverted_fixtures: number(args, "reverted-fixtures", defaults::REVERTED_FIXTURES)?,
            revert_search_blocks: number(
                args,
                "revert-search-blocks",
                defaults::REVERT_SEARCH_BLOCKS,
            )?,
            max_helpers: number(args, "max-helpers", defaults::MAX_HELPERS)?,
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
        };
        if cfg.scan_blocks == 0 || cfg.header_batch == 0 {
            return Err("--scan-blocks and --header-batch must be positive".into());
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
            eprintln!("g01_flap_freeze: {e}\n{}", usage());
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
            eprintln!("g01_flap_freeze: FAILED, no record written: {e}");
            ExitCode::FAILURE
        }
    }
}

// ---- the archived inputs ---------------------------------------------------------------

struct Archive {
    root: PathBuf,
    inputs: Vec<(String, &'static str, [u8; 32])>,
    official: solidity::Source,
    interfaces: solidity::Source,
    keccak_reference: Vec<(String, B256)>,
    /// (tx, salt, token version) of the decoded Gate-0 launch.
    historical: (B256, B256, u64),
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
            official: solidity::Source::default(),
            interfaces: solidity::Source::default(),
            keccak_reference: Vec::new(),
            historical: (B256::ZERO, B256::ZERO, 0),
        };
        let text =
            |b: Vec<u8>, p: &str| String::from_utf8(b).map_err(|_| format!("{p}: not UTF-8"));
        let b = a.read(inputs::IPORTAL_OFFICIAL, "PUBLISHED_IPORTAL_OFFICIAL")?;
        a.official = solidity::Source::parse(&text(b, inputs::IPORTAL_OFFICIAL)?)?;
        let b = a.read(inputs::IPORTAL_INTERFACES, "PUBLISHED_IPORTAL_INTERFACES")?;
        a.interfaces = solidity::Source::parse(&text(b, inputs::IPORTAL_INTERFACES)?)?;
        let b = a.read(inputs::KECCAK_REFERENCE, "WEB3_SHA3_REFERENCE")?;
        a.keccak_reference = edge::keccak_reference(&b)?;
        let b = a.read(inputs::HISTORICAL_LAUNCH, "GATE0_LAUNCH_FIXTURE_DECODED")?;
        a.historical = edge::archived_flap_launch(&b)?;
        Ok(a)
    }

    /// The archived name of a token version (enum `TokenVersion`).
    fn version_label(&self, version: Option<u64>) -> String {
        match version {
            None => "UNKNOWN (no TokenVersionSet)".into(),
            Some(v) => self
                .official
                .enums
                .get(inputs::TOKEN_VERSION_ENUM)
                .and_then(|m| m.get(v as usize))
                .cloned()
                .unwrap_or_else(|| format!("UNKNOWN_{v} (not in the archived TokenVersion enum)")),
        }
    }
}

// ---- the scan --------------------------------------------------------------------------

/// A canonical Portal log with its receipt and transaction.
#[derive(Clone, Debug)]
struct Hit {
    source: &'static str,
    block_hash: B256,
    receipt: Receipt,
    log: Option<Log>,
    tx: Tx,
}

#[derive(Default)]
struct ScanOutcome {
    from: u64,
    search_to: u64,
    headers: u64,
    candidates: u64,
    fetched: u64,
    /// Blocks read only for the revert search.
    revert_fetched: u64,
    census_complete: bool,
    launches: Vec<Hit>,
    /// Direct Portal calls; routed ones wait in `routed` until the end.
    buys: Vec<Hit>,
    sells: Vec<Hit>,
    dexes: Vec<Hit>,
    reverted: Vec<Hit>,
    topics: BTreeMap<B256, u64>,
    /// Routed BUY, SELL and DEX hits (`0`, `1`, `2`), used only where too
    /// few direct Portal calls were found.
    routed: [Vec<Hit>; 3],
    error: Option<String>,
}

fn scan(rpc: &Endpoint, cfg: &Config, portal: Address, headers: &[Header]) -> ScanOutcome {
    let mut out = ScanOutcome {
        from: headers.first().map_or(0, |h| h.number),
        search_to: headers.last().map_or(0, |h| h.number),
        headers: headers.len() as u64,
        census_complete: true,
        ..ScanOutcome::default()
    };
    if let Err(e) = scan_into(rpc, cfg, portal, headers, &mut out) {
        out.error = Some(e);
        out.census_complete = false;
    }
    let [buys, sells, dexes] = std::mem::take(&mut out.routed);
    for (list, routed, max) in [
        (&mut out.buys, buys, cfg.buy_fixtures),
        (&mut out.sells, sells, cfg.sell_fixtures),
        (&mut out.dexes, dexes, cfg.dex_fixtures),
    ] {
        let room = max.saturating_sub(list.len());
        list.extend(routed.into_iter().take(room));
    }
    out
}

fn scan_into(
    rpc: &Endpoint,
    cfg: &Config,
    portal: Address,
    headers: &[Header],
    out: &mut ScanOutcome,
) -> Result<(), String> {
    for header in headers.iter().rev() {
        let launch = bloom_has(header, portal, abi::token_created_topic());
        let wants =
            |have: usize, max: usize, topic: B256| have < max && bloom_has(header, portal, topic);
        let buy = wants(
            out.buys.len() + out.routed[0].len(),
            cfg.buy_fixtures,
            abi::token_bought_topic(),
        );
        let sell = wants(
            out.sells.len() + out.routed[1].len(),
            cfg.sell_fixtures,
            abi::token_sold_topic(),
        );
        let dex = wants(
            out.dexes.len() + out.routed[2].len(),
            cfg.dex_fixtures,
            abi::launched_to_dex_topic(),
        );
        // A reverted call leaves no log: blocks with Portal activity are
        // searched while a reverted fixture is missing.
        let reverted = out.reverted.len() < cfg.reverted_fixtures
            && header
                .logs_bloom
                .contains_input(alloy_primitives::BloomInput::Raw(portal.as_slice()));
        let main = launch || buy || sell || dex;
        let revert_only = reverted && out.revert_fetched < cfg.revert_search_blocks;
        if !(main || revert_only) {
            continue;
        }
        if main && out.fetched >= cfg.receipt_budget {
            if launch {
                out.census_complete = false;
            }
            continue;
        }
        out.candidates += 1;
        let mut receipts = rpc.block_receipts(header)?;
        if main {
            out.fetched += 1;
        } else {
            out.revert_fetched += 1;
        }
        receipts.sort_by_key(|r| std::cmp::Reverse(r.transaction_index));
        for r in &receipts {
            let mut tx: Option<Tx> = None;
            let fetch = |tx: &mut Option<Tx>| -> Result<Tx, String> {
                if tx.is_none() {
                    let t = rpc.transaction(r.transaction_hash)?;
                    edge::require_tx_at(&t, r)?;
                    *tx = Some(t);
                }
                Ok(tx.clone().unwrap_or_else(|| unreachable!()))
            };
            if r.status == 0 && r.to == Some(portal) && out.reverted.len() < cfg.reverted_fixtures {
                let t = fetch(&mut tx)?;
                out.reverted.push(Hit {
                    source: "RECEIPT_SCAN",
                    block_hash: header.hash,
                    receipt: r.clone(),
                    log: None,
                    tx: t,
                });
            }
            if r.status != 1 {
                continue;
            }
            for log in r.logs.iter().filter(|l| l.address == portal) {
                let Some(topic) = log.topics.first().copied() else {
                    continue;
                };
                *out.topics.entry(topic).or_default() += 1;
                // A direct trade: a top-level Portal call that launches nothing
                // (a launch's own purchase is not an external BUY).
                let direct = r.to == Some(portal)
                    && !r
                        .logs
                        .iter()
                        .any(|l| l.topics.first() == Some(&abi::token_created_topic()));
                let [routed_buys, routed_sells, routed_dexes] = &mut out.routed;
                let list = if topic == abi::token_created_topic() {
                    Some((&mut out.launches, usize::MAX))
                } else if topic == abi::token_bought_topic() {
                    Some((
                        if direct { &mut out.buys } else { routed_buys },
                        cfg.buy_fixtures,
                    ))
                } else if topic == abi::token_sold_topic() {
                    Some((
                        if direct { &mut out.sells } else { routed_sells },
                        cfg.sell_fixtures,
                    ))
                } else if topic == abi::launched_to_dex_topic() {
                    Some((
                        if direct { &mut out.dexes } else { routed_dexes },
                        cfg.dex_fixtures,
                    ))
                } else {
                    None
                };
                if let Some((list, max)) = list
                    && list.len() < max
                {
                    let t = fetch(&mut tx)?;
                    list.push(Hit {
                        source: "RECEIPT_SCAN",
                        block_hash: header.hash,
                        receipt: r.clone(),
                        log: Some(log.clone()),
                        tx: t,
                    });
                }
            }
        }
    }
    Ok(())
}

// ---- observations at the reference block --------------------------------------------

#[derive(Clone, Debug)]
struct ContractObs {
    role: String,
    address: Address,
    resolution: String,
    scope: &'static str,
    code: Vec<u8>,
    implementation: Option<Address>,
    implementation_code: Option<Vec<u8>>,
    sourcify: Vec<SourcifyObs>,
}

impl ContractObs {
    fn executing_code(&self) -> &[u8] {
        self.implementation_code.as_deref().unwrap_or(&self.code)
    }
}

struct Observed {
    proxy: ContractObs,
    implementation: ContractObs,
    version: Result<String, String>,
    tax_v3: ContractObs,
    helpers: Vec<ContractObs>,
    token_codes: BTreeMap<Address, Vec<u8>>,
}

fn observe(
    rpc: &Endpoint,
    cfg: &Config,
    deployment: &FlapDeployment,
    pin: &Pin,
    tokens: &BTreeSet<Address>,
) -> Result<Observed, String> {
    let portal = deployment.portal;
    let word = rpc.storage(portal, eip1967_implementation_slot(), pin)?;
    if word[..12] != [0u8; 12] || word[12..] == [0u8; 20] {
        return Err(format!("Portal EIP-1967 implementation slot holds {word}"));
    }
    let implementation = Address::from_slice(&word[12..]);
    let implementation_code = rpc.code(implementation, pin)?;
    let version = match rpc.call_at(portal, &abi::version_call(), pin)? {
        CallOutcome::Returned(out) => abi::version_return(&out),
        CallOutcome::Reverted(e) => Err(format!("version() reverted: {e}")),
    };
    let proxy = ContractObs {
        role: "PortalProxy".into(),
        address: portal,
        resolution: "trading::venues::flap::deployment::bsc_mainnet_v5_24_0().portal".into(),
        scope: "FROZEN_SURFACE",
        code: rpc.code(portal, pin)?,
        implementation: Some(implementation),
        implementation_code: Some(implementation_code.clone()),
        sourcify: Vec::new(),
    };
    let implementation_obs = ContractObs {
        role: "PortalImplementation".into(),
        address: implementation,
        resolution: "EIP-1967 implementation slot of the Portal at the reference block".into(),
        scope: "FROZEN_SURFACE",
        code: implementation_code.clone(),
        implementation: None,
        implementation_code: None,
        sourcify: Vec::new(),
    };
    let tax_v3 = ContractObs {
        role: "TaxV3Implementation".into(),
        address: deployment.tax_v3_implementation,
        resolution:
            "trading::venues::flap::deployment::bsc_mainnet_v5_24_0().tax_v3_implementation".into(),
        scope: "DERIVATION_INPUT",
        code: rpc.code(deployment.tax_v3_implementation, pin)?,
        implementation: None,
        implementation_code: None,
        sourcify: Vec::new(),
    };
    // Helpers: the address immutables that hold code, of the implementation
    // and (one level further) of those helpers, at most `--max-helpers`. A
    // value whose first four bytes are zero is a small constant, not an
    // address.
    let mut helpers: Vec<ContractObs> = Vec::new();
    let mut seen: BTreeSet<Address> = [portal, implementation, deployment.tax_v3_implementation]
        .into_iter()
        .collect();
    let mut level: Vec<(Address, Vec<u8>)> = vec![(implementation, implementation_code.clone())];
    for depth in 1..=HELPER_DEPTH {
        let mut next = Vec::new();
        for (parent, code) in &level {
            let found: Vec<Address> = CodeScan::new(code)
                .addresses()
                .copied()
                .filter(|a| a[..4] != [0u8; 4])
                .collect();
            for a in found {
                if helpers.len() >= cfg.max_helpers || !seen.insert(a) {
                    continue;
                }
                let code = rpc.code(a, pin)?;
                if code.is_empty() {
                    continue;
                }
                helpers.push(ContractObs {
                    role: if depth == 1 {
                        format!("PortalImplementationImmutable {}", addr(a))
                    } else {
                        format!("HelperImmutable {} of {}", addr(a), addr(*parent))
                    },
                    address: a,
                    resolution: format!(
                        "address immutable, depth {depth} below the Portal implementation"
                    ),
                    scope: "HELPER",
                    code: code.clone(),
                    implementation: None,
                    implementation_code: None,
                    sourcify: Vec::new(),
                });
                next.push((a, code));
            }
        }
        level = next;
    }
    let mut token_codes = BTreeMap::new();
    for t in tokens {
        token_codes.insert(*t, rpc.code(*t, pin)?);
    }
    Ok(Observed {
        proxy,
        implementation: implementation_obs,
        version,
        tax_v3,
        helpers,
        token_codes,
    })
}

// ---- launches, fixtures and the census -----------------------------------------------

#[derive(Clone, Debug)]
struct LaunchView {
    hit: Hit,
    token: Address,
    version: Option<u64>,
    quote: Option<Address>,
    direct: bool,
    call: String,
    decoded: Option<abi::LaunchCall>,
    template: Option<Address>,
    salt_source: String,
    salt: Option<B256>,
    init_hash: Option<B256>,
    predicted: Option<Address>,
    reproduced: bool,
    production: String,
}

/// Salt candidates of a routed launch: every 32-byte window of the pending
/// calldata (the router may embed the Portal call at any byte offset).
fn locate_salt(
    input: &[u8],
    portal: Address,
    template: Address,
    token: Address,
) -> Option<(usize, B256)> {
    let (init_hash, _) = predict_clone(portal, B256::ZERO, template);
    (0..input.len().saturating_sub(31))
        .map(|k| (k, B256::from_slice(&input[k..k + 32])))
        .find(|(_, salt)| portal.create2(*salt, init_hash) == token)
}

fn view_launch(
    hit: Hit,
    portal: Address,
    deployment: &FlapDeployment,
    token_codes: &BTreeMap<Address, Vec<u8>>,
) -> Option<LaunchView> {
    let log = hit.log.as_ref()?;
    let token = abi::created_token(&log.topics, &log.data)?;
    let logs: Vec<&Log> = hit
        .receipt
        .logs
        .iter()
        .filter(|l| l.address == portal)
        .collect();
    let version = logs
        .iter()
        .filter_map(|l| abi::version_set(&l.topics, &l.data))
        .find(|(t, _)| *t == token)
        .map(|(_, v)| u64::from(v));
    let quote = logs
        .iter()
        .filter_map(|l| abi::quote_set(&l.topics, &l.data))
        .find(|(t, _)| *t == token)
        .map(|(_, q)| q);
    let direct = hit.tx.to == Some(portal);
    let selector = hit.tx.input.get(..4).unwrap_or_default();
    let decoded = direct
        .then(|| abi::launch_call(&hit.tx.input).ok())
        .flatten();
    let call = match (direct, abi::function_name(selector)) {
        (true, Some(name)) => name.to_string(),
        (true, None) => format!("DIRECT unknown selector {}", hex_bytes(selector)),
        (false, _) => format!(
            "ROUTED {} selector {}",
            hit.tx.to.map_or("CREATE".into(), addr),
            hex_bytes(selector)
        ),
    };
    let template = token_codes.get(&token).and_then(|c| eip1167_target(c));
    let mut v = LaunchView {
        token,
        version,
        quote,
        direct,
        call,
        template,
        salt_source: "NOT_FOUND".into(),
        salt: None,
        init_hash: None,
        predicted: None,
        reproduced: false,
        production: "NOT_APPLICABLE".into(),
        decoded,
        hit,
    };
    let Some(template) = template else {
        v.salt_source = "NOT_FOUND: the token runtime is not an EIP-1167 clone".into();
        return Some(v);
    };
    match &v.decoded {
        Some(d) => {
            v.salt = Some(d.salt);
            v.salt_source = format!("DECODED {}.salt", d.function);
        }
        None => {
            if let Some((k, salt)) = locate_salt(&v.hit.tx.input, portal, template, token) {
                v.salt = Some(salt);
                v.salt_source = format!("CALLDATA_BYTE_OFFSET {k}");
            } else {
                v.salt_source =
                    "NOT_FOUND: no 32-byte window of the pending calldata reproduces the token"
                        .into();
            }
        }
    }
    if let Some(salt) = v.salt {
        let (init_hash, predicted) = predict_clone(portal, salt, template);
        v.init_hash = Some(init_hash);
        v.predicted = Some(predicted);
        v.reproduced = predicted == token;
        if template == deployment.tax_v3_implementation {
            v.production = if deployment.taxed_v3_token(salt) == token {
                "MATCH".into()
            } else {
                "MISMATCH".into()
            };
        }
    }
    Some(v)
}

type Checks = Vec<(String, bool, String)>;

fn check(checks: &mut Checks, name: &str, pass: bool, detail: impl Into<String>) {
    checks.push((name.to_string(), pass, detail.into()));
}

/// A recorded fixture: kind, the hit, its decoded call and checks.
#[derive(Clone, Debug)]
struct FixtureView {
    kind: &'static str,
    hit: Hit,
    call: String,
    token: Option<Address>,
    version: Option<u64>,
    params: Vec<(String, String)>,
    checks: Checks,
}

/// Prefix of checks that record evidence without deciding a fixture (the
/// token-address reproduction has its own property).
const INFORMATIONAL: &str = "derivation.";

impl FixtureView {
    fn passed(&self) -> bool {
        self.checks
            .iter()
            .filter(|(name, _, _)| !name.starts_with(INFORMATIONAL))
            .all(|(_, pass, _)| *pass)
    }
}

fn launch_fixture(v: &LaunchView, portal: Address) -> FixtureView {
    let mut checks = Vec::new();
    let r = &v.hit.receipt;
    check(
        &mut checks,
        "receipt_status_success",
        r.status == 1,
        format!("status {}", r.status),
    );
    check(
        &mut checks,
        "token_created_by_portal",
        v.hit.log.as_ref().is_some_and(|l| l.address == portal),
        "",
    );
    check(
        &mut checks,
        "token_version_set_for_the_token",
        v.version.is_some(),
        format!("{:?}", v.version),
    );
    if v.direct {
        check(
            &mut checks,
            "direct_calldata_decodes",
            v.decoded.is_some(),
            v.call.clone(),
        );
        if let Some(d) = &v.decoded
            && let Some(cv) = d.token_version
        {
            check(
                &mut checks,
                "calldata_tokenVersion_equals_event",
                Some(u64::from(cv)) == v.version,
                format!("calldata {cv} event {:?}", v.version),
            );
        }
        if v.hit.tx.input.get(..4) == Some(&production::NEW_TOKEN_V6_SELECTOR[..]) {
            let prod = production::decode_new_token_v6(&v.hit.tx.input, v.hit.tx.input.len());
            let agrees = match (&prod, &v.decoded) {
                (Ok(p), Some(d)) => p.salt == d.salt && Some(p.token_version) == d.token_version,
                _ => false,
            };
            check(
                &mut checks,
                "production_decode_new_token_v6_agrees",
                agrees,
                match prod {
                    Ok(p) => format!("salt {} tokenVersion {}", p.salt, p.token_version),
                    Err(e) => format!("{e:?}"),
                },
            );
        }
    }
    check(
        &mut checks,
        "token_runtime_is_eip1167_clone",
        v.template.is_some(),
        v.template.map_or("not a clone".into(), addr),
    );
    check(
        &mut checks,
        &format!("{INFORMATIONAL}token_address_reproduced"),
        v.reproduced,
        format!("{} -> {:?}", v.salt_source, v.predicted.map(addr)),
    );
    FixtureView {
        kind: "LAUNCH",
        hit: v.hit.clone(),
        call: v.call.clone(),
        token: Some(v.token),
        version: v.version,
        params: v
            .decoded
            .as_ref()
            .map(|d| d.params.clone())
            .unwrap_or_default(),
        checks,
    }
}

fn trade_fixture(kind: &'static str, hit: &Hit, portal: Address) -> FixtureView {
    let mut checks = Vec::new();
    let r = &hit.receipt;
    check(
        &mut checks,
        "receipt_status_success",
        r.status == 1,
        format!("status {}", r.status),
    );
    let event = hit
        .log
        .as_ref()
        .and_then(|l| abi::event_of(&l.topics, &l.data));
    let token = match &event {
        Some(Ok(e)) => e.token,
        _ => None,
    };
    check(
        &mut checks,
        "event_emitted_by_portal_and_decodes",
        hit.log.as_ref().is_some_and(|l| l.address == portal) && matches!(event, Some(Ok(_))),
        match &event {
            Some(Ok(e)) => e.signature.to_string(),
            Some(Err(e)) => e.clone(),
            None => "no frozen event".into(),
        },
    );
    let direct = hit.tx.to == Some(portal);
    let selector = hit.tx.input.get(..4).unwrap_or_default();
    let (call, params) = if direct {
        match abi::trade_call(&hit.tx.input) {
            Ok((name, params)) => {
                let named: Option<Address> = params
                    .iter()
                    .find(|(k, _)| match kind {
                        "SELL" => k == "token" || k == "inputToken",
                        _ => k == "token" || k == "outputToken",
                    })
                    .and_then(|(_, v)| v.parse().ok());
                check(
                    &mut checks,
                    "calldata_token_equals_event_token",
                    named.is_some() && named == token,
                    format!("calldata {:?} event {:?}", named.map(addr), token.map(addr)),
                );
                (name.to_string(), params)
            }
            Err(e) => {
                check(
                    &mut checks,
                    "direct_calldata_decodes_as_trade",
                    false,
                    e.clone(),
                );
                (format!("DIRECT {e}"), Vec::new())
            }
        }
    } else {
        (
            format!(
                "ROUTED {} selector {}",
                hit.tx.to.map_or("CREATE".into(), addr),
                hex_bytes(selector)
            ),
            Vec::new(),
        )
    };
    FixtureView {
        kind,
        hit: hit.clone(),
        call,
        token,
        version: None,
        params,
        checks,
    }
}

fn reverted_fixture(hit: &Hit, portal: Address) -> FixtureView {
    let mut checks = Vec::new();
    let r = &hit.receipt;
    check(
        &mut checks,
        "receipt_status_failure",
        r.status == 0,
        format!("status {}", r.status),
    );
    check(
        &mut checks,
        "no_logs_on_revert",
        r.logs.is_empty(),
        format!("{} logs", r.logs.len()),
    );
    check(
        &mut checks,
        "top_level_call_to_portal",
        hit.tx.to == Some(portal),
        "",
    );
    let selector = hit.tx.input.get(..4).unwrap_or_default();
    let (call, params) = match (
        abi::launch_call(&hit.tx.input),
        abi::trade_call(&hit.tx.input),
    ) {
        (Ok(l), _) => (l.function.to_string(), l.params),
        (_, Ok((name, params))) => (name.to_string(), params),
        _ => (
            abi::function_name(selector)
                .map_or(format!("selector {}", hex_bytes(selector)), str::to_string),
            Vec::new(),
        ),
    };
    FixtureView {
        kind: "REVERTED",
        hit: hit.clone(),
        call,
        token: None,
        version: None,
        params,
        checks,
    }
}

#[derive(Clone, Debug)]
struct VersionDerivation {
    version: Option<u64>,
    label: String,
    status: &'static str,
    template: Option<Address>,
    init_hash: Option<B256>,
    launches: u64,
    located: u64,
    reproduced: u64,
    reason: String,
}

fn derivations(launches: &[LaunchView], archive: &Archive) -> Vec<VersionDerivation> {
    let mut groups: BTreeMap<Option<u64>, Vec<&LaunchView>> = BTreeMap::new();
    for l in launches {
        groups.entry(l.version).or_default().push(l);
    }
    groups
        .into_iter()
        .map(|(version, ls)| {
            let templates: BTreeSet<Option<Address>> = ls.iter().map(|l| l.template).collect();
            let located = ls.iter().filter(|l| l.salt.is_some()).count() as u64;
            let reproduced = ls.iter().filter(|l| l.reproduced).count() as u64;
            let unlocated: BTreeSet<&str> = ls
                .iter()
                .filter(|l| l.salt.is_none())
                .map(|l| l.call.as_str())
                .collect();
            let template = (templates.len() == 1).then(|| templates.first().copied().flatten()).flatten();
            let ok = located > 0 && reproduced == located;
            let mut reason = if ok {
                format!(
                    "{reproduced}/{located} launches with the salt located in the pending calldata reproduce: token = CREATE2(Portal, salt, keccak256(EIP-1167 creation code of {}))",
                    template.map_or("the token's template".into(), addr)
                )
            } else if located == 0 {
                "no launch of this version has its salt in the pending calldata".to_string()
            } else {
                format!("only {reproduced}/{located} launches with a located salt reproduce")
            };
            if !unlocated.is_empty() {
                reason.push_str(&format!(
                    "; {} launches without the salt in the pending calldata (pre-execution address UNKNOWN): {}",
                    ls.len() as u64 - located,
                    unlocated.into_iter().collect::<Vec<_>>().join(", ")
                ));
            }
            if templates.len() > 1 {
                reason.push_str(&format!("; {} different templates", templates.len()));
            }
            VersionDerivation {
                label: archive.version_label(version),
                version,
                status: if ok { "PROVEN" } else { "UNKNOWN" },
                template,
                init_hash: template.map(|t| predict_clone(Address::ZERO, B256::ZERO, t).0),
                launches: ls.len() as u64,
                located,
                reproduced,
                reason,
            }
        })
        .collect()
}

/// (dimension, value) -> count over the scanned launches.
fn census(launches: &[LaunchView], archive: &Archive) -> BTreeMap<(&'static str, String), u64> {
    let mut out = BTreeMap::new();
    for l in launches.iter().filter(|l| l.hit.source == "RECEIPT_SCAN") {
        let path = if l.direct {
            format!("DIRECT {}", l.call)
        } else {
            l.call.clone()
        };
        let quote = match l.quote {
            Some(q) if q == Address::ZERO => "NATIVE".to_string(),
            Some(q) => addr(q),
            None => "UNKNOWN (no TokenQuoteSet)".into(),
        };
        let salt = l
            .salt_source
            .split(' ')
            .next()
            .unwrap_or("")
            .trim_end_matches(':')
            .to_string();
        for (dimension, value) in [
            ("CALL_PATH", path),
            (
                "TOKEN_VERSION",
                format!(
                    "{} {}",
                    l.version.map_or("?".into(), |v| v.to_string()),
                    archive.version_label(l.version)
                ),
            ),
            ("QUOTE_TOKEN", quote),
            ("TEMPLATE", l.template.map_or("NOT_EIP1167".into(), addr)),
            ("SALT_SOURCE", salt),
        ] {
            *out.entry((dimension, value)).or_default() += 1;
        }
    }
    out
}

// ---- selectors and properties -------------------------------------------------------------

#[derive(Clone, Debug)]
struct SelectorObs {
    spec: Spec,
    official: &'static str,
    interfaces: &'static str,
    verified: &'static str,
    reference: &'static str,
    found_in: String,
    live: u64,
}

fn selectors(
    archive: &Archive,
    obs: &Observed,
    verified_abi: Option<&BTreeSet<String>>,
    topics: &BTreeMap<B256, u64>,
    fetched_txs: &[&Tx],
    portal: Address,
) -> Vec<SelectorObs> {
    let impl_scan = CodeScan::new(&obs.implementation.code);
    let helper_scans: Vec<(Address, CodeScan)> = obs
        .helpers
        .iter()
        .map(|h| (h.address, CodeScan::new(&h.code)))
        .collect();
    abi::specs()
        .into_iter()
        .map(|spec| {
            let event = spec.kind == SpecKind::Event;
            let found_in = if event {
                let topic = B256::from_slice(&spec.value);
                let mut places = Vec::new();
                if impl_scan.has_word(topic) {
                    places.push("PortalImplementation".to_string());
                }
                for (a, s) in &helper_scans {
                    if s.has_word(topic) {
                        places.push(format!("helper {}", addr(*a)));
                    }
                }
                if places.is_empty() {
                    "NOT_FOUND".into()
                } else {
                    places.join(", ")
                }
            } else {
                let selector: [u8; 4] = spec.value[..4].try_into().unwrap_or_default();
                impl_scan.selector(selector)
            };
            let live = if event {
                topics
                    .get(&B256::from_slice(&spec.value))
                    .copied()
                    .unwrap_or(0)
            } else {
                fetched_txs
                    .iter()
                    .filter(|t| t.to == Some(portal) && t.input.get(..4) == Some(&spec.value[..]))
                    .count() as u64
            };
            let reference = match archive
                .keccak_reference
                .iter()
                .find(|(label, _)| label == spec.signature)
            {
                None => "NOT_IN_REFERENCE",
                Some((_, h)) if h[..spec.value.len()] == spec.value[..] => {
                    "MATCH (archived web3_sha3)"
                }
                Some(_) => "MISMATCH",
            };
            SelectorObs {
                official: if archive.official.declares(event, spec.signature) {
                    "MATCH"
                } else {
                    "NOT_IN_ARCHIVE"
                },
                interfaces: if archive.interfaces.declares(event, spec.signature) {
                    "MATCH"
                } else {
                    "NOT_IN_ARCHIVE"
                },
                verified: match verified_abi {
                    None => "UNAVAILABLE",
                    Some(sigs) if sigs.contains(spec.signature) => "IN_VERIFIED_ABI",
                    Some(_) => "NOT_IN_VERIFIED_ABI",
                },
                reference,
                found_in,
                live,
                spec,
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

/// A witnessed constant against the chain.
#[derive(Clone, Debug)]
struct ConstantObs {
    name: &'static str,
    witnessed: Option<Vec<u8>>,
    observed: Option<Vec<u8>>,
    witnessed_text: Option<String>,
    observed_text: Option<String>,
    detail: String,
}

impl ConstantObs {
    fn status(&self) -> &'static str {
        match (&self.observed, &self.observed_text) {
            (None, None) => "NOT_OBSERVED",
            _ if self.observed == self.witnessed && self.observed_text == self.witnessed_text => {
                "UNCHANGED"
            }
            _ => "CHANGED",
        }
    }
}

fn constants(
    deployment: &FlapDeployment,
    chain_id: u64,
    obs: &Observed,
    taxed_template: Option<Address>,
) -> Vec<ConstantObs> {
    let b = |x: &[u8]| Some(x.to_vec());
    let template_code_sha = taxed_template.and_then(|t| {
        if t == obs.tax_v3.address {
            Some(sha256(&obs.tax_v3.code).to_vec())
        } else {
            None
        }
    });
    vec![
        ConstantObs {
            name: "chain_id",
            witnessed: None,
            observed: None,
            witnessed_text: Some(deployment.chain_id.to_string()),
            observed_text: Some(chain_id.to_string()),
            detail: "eth_chainId".into(),
        },
        ConstantObs {
            name: "portal",
            witnessed: b(deployment.portal.as_slice()),
            observed: b(obs.proxy.address.as_slice()),
            witnessed_text: None,
            observed_text: None,
            detail: "the table address read".into(),
        },
        ConstantObs {
            name: "portal_version",
            witnessed: None,
            observed: None,
            witnessed_text: Some(deployment.portal_version.to_string()),
            observed_text: obs.version.as_ref().ok().cloned(),
            detail: match &obs.version {
                Ok(_) => "Portal.version() at the reference block".into(),
                Err(e) => e.clone(),
            },
        },
        ConstantObs {
            name: "portal_runtime_sha256",
            witnessed: b(&deployment.portal_runtime_sha256),
            observed: Some(sha256(&obs.proxy.code).to_vec()),
            witnessed_text: None,
            observed_text: None,
            detail: format!("proxy runtime keccak256 {}", keccak256(&obs.proxy.code)),
        },
        ConstantObs {
            name: "portal_implementation",
            witnessed: b(deployment.portal_implementation.as_slice()),
            observed: b(obs.implementation.address.as_slice()),
            witnessed_text: None,
            observed_text: None,
            detail: "EIP-1967 implementation slot".into(),
        },
        ConstantObs {
            name: "portal_implementation_runtime_sha256",
            witnessed: b(&deployment.portal_implementation_runtime_sha256),
            observed: Some(sha256(&obs.implementation.code).to_vec()),
            witnessed_text: None,
            observed_text: None,
            detail: format!(
                "implementation runtime keccak256 {}, {} bytes",
                keccak256(&obs.implementation.code),
                obs.implementation.code.len()
            ),
        },
        ConstantObs {
            name: "tax_v3_implementation",
            witnessed: b(deployment.tax_v3_implementation.as_slice()),
            observed: taxed_template.map(|t| t.to_vec()),
            witnessed_text: None,
            observed_text: None,
            detail: "the EIP-1167 template of the TOKEN_TAXED_V3 launches in the scan".into(),
        },
        ConstantObs {
            name: "tax_v3_implementation_runtime_sha256",
            witnessed: b(&deployment.tax_v3_implementation_runtime_sha256),
            observed: template_code_sha.or_else(|| Some(sha256(&obs.tax_v3.code).to_vec())),
            witnessed_text: None,
            observed_text: None,
            detail: format!(
                "runtime of {} keccak256 {}",
                addr(obs.tax_v3.address),
                keccak256(&obs.tax_v3.code)
            ),
        },
        ConstantObs {
            name: "tax_v3_init_code_hash",
            witnessed: b(deployment.tax_v3_init_code_hash.as_slice()),
            observed: taxed_template
                .map(|t| predict_clone(Address::ZERO, B256::ZERO, t).0.to_vec()),
            witnessed_text: None,
            observed_text: None,
            detail: "keccak256 of the EIP-1167 creation code of the observed template".into(),
        },
    ]
}

fn proven(ok: bool) -> &'static str {
    if ok { "PROVEN" } else { "UNKNOWN" }
}

struct Eval<'a> {
    common: &'a Common,
    obs: &'a Observed,
    constants: Vec<ConstantObs>,
    selectors: Vec<SelectorObs>,
    launches: Vec<LaunchView>,
    fixtures: Vec<FixtureView>,
    derivations: Vec<VersionDerivation>,
    census: BTreeMap<(&'static str, String), u64>,
    scan: &'a ScanOutcome,
    deployment: FlapDeployment,
    sourcify_enabled: bool,
}

impl Eval<'_> {
    fn properties(&self) -> Vec<(String, &'static str, String)> {
        let c = self.common;
        let o = self.obs;
        let mut out = Vec::new();
        out.push((
            "chain_and_portal_address".into(),
            proven(c.chain_id == edge::BSC_CHAIN_ID && c.pin_rechecked && !o.proxy.code.is_empty()),
            format!(
                "eth_chainId={} at block {} {} (pin rechecked {}); Portal {} runtime {} bytes",
                c.chain_id,
                c.pin.number,
                c.pin.hash,
                c.pin_rechecked,
                addr(o.proxy.address),
                o.proxy.code.len()
            ),
        ));
        let proxy_source = o
            .proxy
            .sourcify
            .iter()
            .find(|s| s.address == o.proxy.address);
        let proxy_ok = proxy_source.is_some_and(|s| {
            s.verified()
                && matches!(&s.facts, Ok(f) if f.proxy_implementations.contains(&o.implementation.address))
        });
        out.push((
            "implementation_and_version".into(),
            proven(proxy_ok && o.version.is_ok() && !o.implementation.code.is_empty()),
            format!(
                "EIP-1967 implementation {} ({} bytes); version() = {}; proxy source: {}",
                addr(o.implementation.address),
                o.implementation.code.len(),
                o.version.clone().unwrap_or_else(|e| e),
                match proxy_source.map(|s| &s.facts) {
                    Some(Ok(f)) => format!(
                        "Sourcify {} {}, proxyResolution agrees {proxy_ok}",
                        f.status,
                        f.contract_name.clone().unwrap_or_default()
                    ),
                    Some(Err(e)) => format!("unavailable: {e}"),
                    None if !self.sourcify_enabled => "not checked (--no-sourcify)".into(),
                    None => "not looked up".into(),
                }
            ),
        ));
        let impl_source = o.implementation.sourcify.first();
        let all_in_verified = self
            .selectors
            .iter()
            .all(|s| s.verified == "IN_VERIFIED_ABI");
        let archived_ok = self
            .selectors
            .iter()
            .all(|s| s.official == "MATCH" && s.interfaces == "MATCH" && s.reference != "MISMATCH");
        out.push((
            "verified_abi".into(),
            proven(impl_source.is_some_and(SourcifyObs::verified) && all_in_verified),
            format!(
                "implementation {}: {}; {}/{} frozen signatures in the verified ABI; both archived IPortal sources declare every frozen signature: {archived_ok}",
                addr(o.implementation.address),
                match impl_source.map(|s| &s.facts) {
                    Some(Ok(f)) => format!(
                        "Sourcify {} {} {}",
                        f.status,
                        f.contract_name.clone().unwrap_or_default(),
                        f.compiler_version.clone().unwrap_or_default()
                    ),
                    Some(Err(e)) => format!("Sourcify unavailable: {e}"),
                    None => "not looked up".into(),
                },
                self.selectors.iter().filter(|s| s.verified == "IN_VERIFIED_ABI").count(),
                self.selectors.len()
            ),
        ));
        let changed: Vec<String> = self
            .constants
            .iter()
            .filter(|k| k.status() == "CHANGED")
            .map(|k| k.name.to_string())
            .collect();
        let unchanged: Vec<String> = self
            .constants
            .iter()
            .filter(|k| k.status() == "UNCHANGED")
            .map(|k| k.name.to_string())
            .collect();
        let not_observed: Vec<String> = self
            .constants
            .iter()
            .filter(|k| k.status() == "NOT_OBSERVED")
            .map(|k| k.name.to_string())
            .collect();
        out.push((
            "code_hashes_and_constants".into(),
            proven(not_observed.is_empty()),
            format!(
                "against bsc_mainnet_v5_24_0(): CHANGED [{}]; UNCHANGED [{}]; NOT_OBSERVED [{}]",
                changed.join(", "),
                unchanged.join(", "),
                not_observed.join(", ")
            ),
        ));
        let role = |r: &str| -> Vec<&SelectorObs> {
            self.selectors.iter().filter(|s| s.spec.role == r).collect()
        };
        let presence = |list: &[&SelectorObs]| {
            list.iter()
                .map(|s| {
                    format!(
                        "{} {} {}",
                        s.spec.name,
                        hex_bytes(&s.spec.value),
                        s.found_in
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let launches = role("LAUNCH");
        let launch_fixtures: Vec<&FixtureView> = self
            .fixtures
            .iter()
            .filter(|f| f.kind == "LAUNCH")
            .collect();
        let launch_ok = launch_fixtures.iter().filter(|f| f.passed()).count();
        let v6 = launches
            .iter()
            .any(|s| s.spec.name == "newTokenV6" && s.found_in.ends_with("_PRESENT"));
        let v6_direct = launch_fixtures
            .iter()
            .any(|f| f.passed() && f.call == "newTokenV6");
        out.push((
            "launch_selectors".into(),
            proven(v6 && v6_direct && launch_ok >= REQUIRED_LAUNCHES),
            format!(
                "dispatcher: {}; two-step: {}; canonical launch fixtures passing {launch_ok}/{} (direct newTokenV6 with production decoder agreement: {v6_direct})",
                presence(&launches),
                presence(&role("LAUNCH_TWO_STEP")),
                launch_fixtures.len()
            ),
        ));
        let trades: Vec<&SelectorObs> = role("BUY").into_iter().chain(role("SELL")).collect();
        let buy_ok = self
            .fixtures
            .iter()
            .filter(|f| f.kind == "BUY" && f.passed())
            .count();
        let swap = trades
            .iter()
            .any(|s| s.spec.name == "swapExactInput" && s.found_in.ends_with("_PRESENT"));
        out.push((
            "buy_sell_selectors".into(),
            proven(swap && buy_ok >= REQUIRED_BUYS),
            format!(
                "dispatcher: {}; canonical TokenBought fixtures passing {buy_ok}",
                presence(&trades)
            ),
        ));
        let events = role("EVENT");
        let signatures_ok = events.iter().all(|s| {
            s.verified == "IN_VERIFIED_ABI"
                && s.official == "MATCH"
                && s.interfaces == "MATCH"
                && s.reference != "MISMATCH"
        });
        let witnessed = events
            .iter()
            .filter(|s| s.found_in != "NOT_FOUND" || s.live > 0)
            .count();
        out.push((
            "event_topics".into(),
            proven(signatures_ok),
            format!(
                "every topic is alloy's keccak256 of a signature in the verified ABI and both archived IPortal sources: {signatures_ok}; emission witnessed (PUSH32 in code or live log) {witnessed}/{}: {}",
                events.len(),
                events
                    .iter()
                    .map(|s| format!("{} [{}; live {}]", s.spec.name, s.found_in, s.live))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        ));
        let count = |k: &str| self.fixtures.iter().filter(|f| f.kind == k).count();
        let passing = |k: &str| {
            self.fixtures
                .iter()
                .filter(|f| f.kind == k && f.passed())
                .count()
        };
        out.push((
            "canonical_fixtures".into(),
            proven(passing("LAUNCH") >= REQUIRED_LAUNCHES && passing("BUY") >= REQUIRED_BUYS),
            format!(
                "passing/recorded: LAUNCH {}/{}, BUY {}/{}, SELL {}/{}, LAUNCHED_TO_DEX {}/{}, REVERTED {}/{}",
                passing("LAUNCH"),
                count("LAUNCH"),
                passing("BUY"),
                count("BUY"),
                passing("SELL"),
                count("SELL"),
                passing("LAUNCHED_TO_DEX"),
                count("LAUNCHED_TO_DEX"),
                passing("REVERTED"),
                count("REVERTED")
            ),
        ));
        let reverted = self
            .fixtures
            .iter()
            .find(|f| f.kind == "REVERTED" && f.passed());
        out.push((
            "revert_is_never_success".into(),
            proven(reverted.is_some()),
            match reverted {
                Some(f) => format!(
                    "tx {} block {}: status 0, no logs, call {}",
                    f.hit.receipt.transaction_hash, f.hit.receipt.block_number, f.call
                ),
                None => "no reverted direct Portal call in the fetched receipts".into(),
            },
        ));
        let taxed: Vec<&VersionDerivation> = self
            .derivations
            .iter()
            .filter(|d| d.version == Some(u64::from(production::TOKEN_VERSION_TAXED_V3)))
            .collect();
        let contradiction = self
            .launches
            .iter()
            .any(|l| l.salt.is_some() && !l.reproduced || l.production == "MISMATCH");
        out.push((
            "token_address_derivation".into(),
            proven(taxed.iter().any(|d| d.status == "PROVEN") && !contradiction),
            self.derivations
                .iter()
                .map(|d| {
                    format!(
                        "{} {}: {} — {}",
                        d.version.map_or("?".into(), |v| v.to_string()),
                        d.label,
                        d.status,
                        d.reason
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
        ));
        let taxed_launches: Vec<&LaunchView> = self
            .launches
            .iter()
            .filter(|l| l.version == Some(u64::from(production::TOKEN_VERSION_TAXED_V3)))
            .collect();
        let tax_in_use = !taxed_launches.is_empty()
            && taxed_launches
                .iter()
                .all(|l| l.template == Some(self.deployment.tax_v3_implementation));
        let embedders: Vec<String> = std::iter::once(&o.implementation)
            .chain(o.helpers.iter())
            .filter(|h| CodeScan::new(&h.code).embeds(self.deployment.tax_v3_implementation))
            .map(|h| h.role.clone())
            .collect();
        out.push((
            "tax_v3_implementation_in_use".into(),
            proven(tax_in_use),
            format!(
                "{}/{} TOKEN_TAXED_V3 launches clone {}; embedded as an immutable by [{}]",
                taxed_launches
                    .iter()
                    .filter(|l| l.template == Some(self.deployment.tax_v3_implementation))
                    .count(),
                taxed_launches.len(),
                addr(self.deployment.tax_v3_implementation),
                embedders.join(", ")
            ),
        ));
        out.push((
            "mode_census".into(),
            proven(self.scan.census_complete && self.scan.launches.iter().len() > 0),
            format!(
                "launches in blocks {}..={} (complete {}): {}",
                self.scan.from,
                self.scan.search_to,
                self.scan.census_complete,
                self.census
                    .iter()
                    .map(|((d, v), n)| format!("{d}={v}: {n}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        ));
        out.push((
            "pending_launch_samples".into(),
            "UNKNOWN",
            "not collected: a read-only canonical freeze over free RPC sees no pending transactions; pending samples are runtime capture evidence (C1/G0.4)".into(),
        ));
        out.push((
            "versioned_evidence".into(),
            "PROVEN",
            format!(
                "this record: {} (type {} v{}); archive inputs SHA-256-bound; Sourcify responses SHA-256-bound; tool SHA-256 {}; git HEAD {}",
                ff::SCHEMA.name,
                ff::SCHEMA.record_type,
                ff::SCHEMA.schema_version,
                B256::from(c.tool_sha256),
                c.git_head.as_deref().unwrap_or("UNKNOWN")
            ),
        ));
        out
    }
}

// ---- the record ------------------------------------------------------------------------------

fn b256(v: B256) -> Value {
    Value::B256(v.0)
}

fn address(v: Address) -> Value {
    Value::Address(v.0.0)
}

fn param_draft(name: &str, value: &str) -> Value {
    let mut p = Draft::new(&shared::param::SCHEMA);
    p.set(shared::param::NAME, name)
        .set(shared::param::VALUE, bounded_text(value));
    p.into_nested()
}

fn sourcify_draft(s: &SourcifyObs) -> Draft {
    use shared::sourcify as k;
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

fn contract_draft(c: &ContractObs) -> Draft {
    use shared::contract as k;
    let mut d = Draft::new(&k::SCHEMA);
    d.set(k::ROLE, c.role.as_str())
        .set(k::ADDRESS, address(c.address))
        .set(k::RESOLUTION, c.resolution.as_str())
        .set(k::SCOPE, c.scope)
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
        .set(k::CHANGE, "SEE_CONSTANTS")
        .set(
            k::CREATE_OPCODES,
            CodeScan::new(c.executing_code()).create_opcodes(),
        );
    for s in &c.sourcify {
        d.push(k::SOURCIFY, sourcify_draft(s).into_nested());
    }
    d
}

fn fixture_draft(f: &FixtureView, portal: Address) -> Draft {
    use ff::fixture as k;
    let r = &f.hit.receipt;
    let mut d = Draft::new(&k::SCHEMA);
    d.set(k::KIND, f.kind)
        .set(k::SOURCE, f.hit.source)
        .set(k::TX_HASH, b256(r.transaction_hash))
        .set(k::BLOCK_NUMBER, r.block_number)
        .set(k::BLOCK_HASH, b256(f.hit.block_hash))
        .set(k::TX_INDEX, r.transaction_index)
        .set(k::RECEIPT_STATUS, r.status)
        .set(k::FROM, address(f.hit.tx.from))
        .set_opt(k::TO, f.hit.tx.to.map(address))
        .set(k::VALUE_WEI, Value::U256(f.hit.tx.value.to_be_bytes()))
        .set(k::INPUT, Value::Bytes(f.hit.tx.input.clone()))
        .set(
            k::SELECTOR,
            Value::Bytes(f.hit.tx.input.get(..4).unwrap_or_default().to_vec()),
        )
        .set(k::CALL, f.call.as_str())
        .set_opt(k::LOG_INDEX, f.hit.log.as_ref().map(|l| l.log_index))
        .set_opt(k::TOKEN, f.token.map(address))
        .set_opt(k::TOKEN_VERSION, f.version);
    for l in r.logs.iter().filter(|l| l.address == portal) {
        use ff::log as g;
        let mut e = Draft::new(&g::SCHEMA);
        let decoded = abi::event_of(&l.topics, &l.data);
        e.set(g::LOG_INDEX, l.log_index)
            .set(
                g::SIGNATURE,
                match &decoded {
                    Some(Ok(ev)) => ev.signature.to_string(),
                    Some(Err(err)) => format!("UNDECODABLE: {err}"),
                    None => "UNFROZEN".into(),
                },
            )
            .set(g::DATA, Value::Bytes(l.data.clone()));
        for t in &l.topics {
            e.push(g::TOPICS, b256(*t));
        }
        if let Some(Ok(ev)) = decoded {
            for (name, value) in &ev.params {
                e.push(g::PARAMS, param_draft(name, value));
            }
        }
        d.push(k::LOGS, e.into_nested());
    }
    for (name, value) in &f.params {
        d.push(k::PARAMS, param_draft(name, value));
    }
    for (name, pass, detail) in &f.checks {
        let mut c = Draft::new(&shared::check::SCHEMA);
        c.set(shared::check::NAME, name.as_str())
            .set(shared::check::PASS, *pass)
            .set(shared::check::DETAIL, bounded_text(detail));
        d.push(k::CHECKS, c.into_nested());
    }
    d
}

const NON_CLAIMS: &[&str] = &[
    "READ_ONLY: no key read, nothing signed or sent",
    "NOT_GATE0_PASS: Phase D/G0.1 evidence only; no economic or operational release",
    "ECONOMICS_UNKNOWN: amounts, fees and prices are recorded, not interpreted (Phase G)",
    "NOT_A_COMPLETE_CHAIN_CENSUS: the census covers the scanned window only",
    "NO_PENDING_SAMPLES: canonical evidence only",
    "ROUTED_PATHS: a routed launch's salt location is evidence for that router call only",
];

fn record(eval: &Eval, archive: &Archive) -> (Draft, Vec<(String, &'static str, String)>) {
    let c = eval.common;
    let o = eval.obs;
    let portal = eval.deployment.portal;
    let mut d = Draft::new(&ff::SCHEMA);
    d.set(ff::CHAIN_ID, c.chain_id)
        .set(ff::PORTAL, address(portal))
        .set(ff::PROVENANCE_TOOL, TOOL)
        .set(ff::PROVENANCE_OBSERVED_UNIX_MS, c.observed_ms)
        .set(ff::REFERENCE_BLOCK, c.pin.number)
        .set(ff::REFERENCE_BLOCK_HASH, b256(c.pin.hash))
        .set(ff::REFERENCE_TIMESTAMP, c.timestamp)
        .set(ff::PIN_RECHECKED, c.pin_rechecked)
        .set(ff::ENDPOINT_HOST, c.endpoint_host.as_str())
        .set_opt(ff::SOURCIFY_HOST, c.sourcify_host.clone())
        .set(ff::TOOL_SHA256, Value::B256(c.tool_sha256))
        .set_opt(ff::GIT_HEAD, c.git_head.clone())
        .set(ff::ARCHIVE_ROOT, c.archive_root.as_str())
        .set_opt(ff::PORTAL_VERSION, o.version.as_ref().ok().cloned());
    for (path, role, sha) in &archive.inputs {
        let mut a = Draft::new(&shared::archive_input::SCHEMA);
        a.set(shared::archive_input::PATH, path.as_str())
            .set(shared::archive_input::ROLE, *role)
            .set(shared::archive_input::SHA256, Value::B256(*sha));
        d.push(ff::ARCHIVE_INPUTS, a.into_nested());
    }
    for k in &eval.constants {
        use ff::constant as t;
        let mut e = Draft::new(&t::SCHEMA);
        e.set(t::NAME, k.name)
            .set_opt(t::WITNESSED, k.witnessed.clone().map(Value::Bytes))
            .set_opt(t::OBSERVED, k.observed.clone().map(Value::Bytes))
            .set_opt(t::WITNESSED_TEXT, k.witnessed_text.clone())
            .set_opt(t::OBSERVED_TEXT, k.observed_text.clone())
            .set(t::STATUS, k.status())
            .set(t::DETAIL, k.detail.as_str());
        d.push(ff::CONSTANTS, e.into_nested());
    }
    for x in std::iter::once(&o.proxy)
        .chain([&o.implementation, &o.tax_v3])
        .chain(o.helpers.iter())
    {
        d.push(ff::CONTRACTS, contract_draft(x).into_nested());
    }
    for s in &eval.selectors {
        use ff::selector as t;
        let mut e = Draft::new(&t::SCHEMA);
        e.set(
            t::KIND,
            match s.spec.kind {
                SpecKind::Function => "FUNCTION",
                SpecKind::Event => "EVENT",
            },
        )
        .set(t::ROLE, s.spec.role)
        .set(t::SIGNATURE, s.spec.signature)
        .set(t::VALUE, Value::Bytes(s.spec.value.clone()))
        .set(
            t::DERIVATION,
            match s.spec.kind {
                SpecKind::Function => abi::SELECTOR_DERIVATION,
                SpecKind::Event => abi::TOPIC_DERIVATION,
            },
        )
        .set(t::ABI_OFFICIAL, s.official)
        .set(t::ABI_INTERFACES, s.interfaces)
        .set(t::VERIFIED_ABI, s.verified)
        .set(t::KECCAK_REFERENCE, s.reference)
        .set(t::FOUND_IN, bounded_text(&s.found_in))
        .set(t::LIVE_COUNT, s.live);
        d.push(ff::SELECTORS, e.into_nested());
    }
    for f in &eval.fixtures {
        d.push(ff::FIXTURES, fixture_draft(f, portal).into_nested());
    }
    for l in &eval.launches {
        use ff::launch as t;
        let mut e = Draft::new(&t::SCHEMA);
        e.set(t::TX_HASH, b256(l.hit.receipt.transaction_hash))
            .set(t::BLOCK_NUMBER, l.hit.receipt.block_number)
            .set(t::SOURCE, l.hit.source)
            .set_opt(t::TO, l.hit.tx.to.map(address))
            .set(
                t::SELECTOR,
                Value::Bytes(l.hit.tx.input.get(..4).unwrap_or_default().to_vec()),
            )
            .set(t::CALL, l.call.as_str())
            .set(t::TOKEN, address(l.token))
            .set_opt(t::TOKEN_VERSION, l.version)
            .set(t::TOKEN_VERSION_LABEL, archive.version_label(l.version))
            .set_opt(t::QUOTE_TOKEN, l.quote.map(address))
            .set_opt(t::TEMPLATE, l.template.map(address))
            .set(t::SALT_SOURCE, l.salt_source.as_str())
            .set_opt(t::SALT, l.salt.map(b256))
            .set_opt(t::INIT_CODE_HASH, l.init_hash.map(b256))
            .set_opt(t::PREDICTED, l.predicted.map(address))
            .set(t::REPRODUCED, l.reproduced)
            .set(t::PRODUCTION_DERIVATION, l.production.as_str());
        d.push(ff::LAUNCHES, e.into_nested());
    }
    for ((dimension, value), count) in &eval.census {
        use ff::census_bucket as t;
        let mut e = Draft::new(&t::SCHEMA);
        e.set(t::DIMENSION, *dimension)
            .set(t::VALUE, value.as_str())
            .set(t::COUNT, *count);
        d.push(ff::CENSUS, e.into_nested());
    }
    for v in &eval.derivations {
        use ff::derivation_version as t;
        let mut e = Draft::new(&t::SCHEMA);
        e.set_opt(t::TOKEN_VERSION, v.version)
            .set(t::LABEL, v.label.as_str())
            .set(t::STATUS, v.status)
            .set_opt(t::TEMPLATE, v.template.map(address))
            .set_opt(t::INIT_CODE_HASH, v.init_hash.map(b256))
            .set(t::LAUNCHES, v.launches)
            .set(t::SALT_LOCATED, v.located)
            .set(t::REPRODUCED, v.reproduced)
            .set(t::REASON, bounded_text(&v.reason));
        d.push(ff::DERIVATION, e.into_nested());
    }
    let labels = topic_labels(o);
    for (topic, count) in &eval.scan.topics {
        use ff::topic_count as t;
        let mut e = Draft::new(&t::SCHEMA);
        e.set(t::TOPIC, b256(*topic))
            .set(
                t::LABEL,
                labels
                    .get(topic)
                    .cloned()
                    .unwrap_or_else(|| "UNLABELLED".into()),
            )
            .set(t::COUNT, *count);
        d.push(ff::TOPICS, e.into_nested());
    }
    {
        use ff::scan as t;
        let s = eval.scan;
        let mut e = Draft::new(&t::SCHEMA);
        e.set(
            t::METHOD,
            "HEADER_LOGSBLOOM(Portal, topic0) THEN eth_getBlockReceipts(blockHash), NEWEST FIRST",
        )
        .set(t::FROM_BLOCK, s.from)
        .set(t::SEARCH_TO_BLOCK, s.search_to)
        .set(t::LINKED_TO_BLOCK, c.linked_to)
        .set(t::HEADERS, s.headers)
        .set(t::BLOOM_CANDIDATE_BLOCKS, s.candidates)
        .set(t::RECEIPT_BLOCKS_FETCHED, s.fetched)
        .set(t::RECEIPT_BLOCK_BUDGET, c.receipt_budget)
        .set(t::LAUNCH_CENSUS_COMPLETE, s.census_complete)
        .set(t::LAUNCHES, s.launches.len() as u64)
        .set(
            t::ROUTED_LAUNCHES,
            eval.launches
                .iter()
                .filter(|l| l.hit.source == "RECEIPT_SCAN" && !l.direct)
                .count() as u64,
        )
        .set(t::REVERTED_PORTAL_CALLS, s.reverted.len() as u64)
        .set_opt(t::ERROR, s.error.clone())
        .set(t::REVERT_SEARCH_BLOCKS_FETCHED, s.revert_fetched);
        d.set(ff::SCAN, e.into_nested());
    }
    let properties = eval.properties();
    for (name, status, evidence) in &properties {
        let mut p = Draft::new(&shared::property::SCHEMA);
        p.set(shared::property::NAME, name.as_str())
            .set(shared::property::STATUS, *status)
            .set(shared::property::EVIDENCE, bounded_text(evidence));
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

/// Topic -> event signature of the verified implementation ABI (and the
/// frozen events), for the topic census.
fn topic_labels(o: &Observed) -> BTreeMap<B256, String> {
    let mut out: BTreeMap<B256, String> = abi::specs()
        .into_iter()
        .filter(|s| s.kind == SpecKind::Event)
        .map(|s| (B256::from_slice(&s.value), s.signature.to_string()))
        .collect();
    if let Some(Ok(f)) = o.implementation.sourcify.first().map(|s| &s.facts) {
        for sig in &f.abi_signatures {
            out.entry(keccak256(sig.as_bytes()))
                .or_insert_with(|| format!("{sig} (verified ABI)"));
        }
    }
    out
}

// ---- the run ---------------------------------------------------------------------------------

struct Outcome {
    dir: PathBuf,
    summary: String,
}

fn run(cfg: &Config) -> Result<Outcome, String> {
    let deployment = bsc_mainnet_v5_24_0();
    let portal = deployment.portal;
    let archive = Archive::load(&cfg.archive)?;
    let rpc = Endpoint::new(&cfg.rpc_url, cfg.limits, cfg.attempts, cfg.retry_pause);
    let chain_id = rpc.chain_id()?;
    edge::require_bsc(chain_id)?;

    let head = rpc.block_number()?;
    let anchor = match cfg.block {
        Some(b) => b,
        None => head
            .checked_sub(cfg.confirmations)
            .ok_or("head below the confirmation depth")?,
    };
    let from = anchor.saturating_sub(cfg.scan_blocks - 1);
    eprintln!("g01_flap_freeze: chain 56, head {head}; scanning headers {from}..={anchor}");
    let anchor_pin = edge::pin_of(anchor, &rpc.header(anchor)?)?;
    let mut headers = linked_headers(&rpc, from, anchor, cfg.header_batch)?;
    if headers.last().map(|h| h.hash) != Some(anchor_pin.hash) {
        return Err("the scanned headers do not end at the pinned anchor".into());
    }
    let mut scanned = scan(&rpc, cfg, portal, &headers);
    eprintln!(
        "g01_flap_freeze: {} candidate blocks, {} receipt blocks fetched, {} launches{}",
        scanned.candidates,
        scanned.fetched,
        scanned.launches.len(),
        scanned
            .error
            .as_ref()
            .map_or(String::new(), |e| format!(", scan stopped: {e}"))
    );

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
        "g01_flap_freeze: reference block {} {}",
        pin.number, pin.hash
    );

    // The Gate-0 launch, re-read live (not part of the census).
    let (historical_tx, historical_salt, historical_version) = archive.historical;
    let historical = {
        let receipt = rpc.receipt(historical_tx)?;
        let tx = rpc.transaction(historical_tx)?;
        edge::require_tx_at(&tx, &receipt)?;
        let canonical = rpc.header(receipt.block_number)?;
        if canonical.hash != receipt.block_hash {
            return Err("the archived launch is not in the canonical block".into());
        }
        let log = receipt
            .logs
            .iter()
            .find(|l| l.address == portal && l.topics.first() == Some(&abi::token_created_topic()))
            .cloned();
        Hit {
            source: "ARCHIVED_2026_09_27_REVERIFIED",
            block_hash: canonical.hash,
            receipt,
            log,
            tx,
        }
    };
    scanned
        .launches
        .sort_by_key(|h| std::cmp::Reverse((h.receipt.block_number, h.receipt.transaction_index)));

    let tokens: BTreeSet<Address> = scanned
        .launches
        .iter()
        .chain(std::iter::once(&historical))
        .filter_map(|h| {
            h.log
                .as_ref()
                .and_then(|l| abi::created_token(&l.topics, &l.data))
        })
        .collect();
    let mut obs = observe(&rpc, cfg, &deployment, &pin, &tokens)?;
    let pin_rechecked = rpc.header(reference)?.hash == pin.hash;

    if let Some(base) = &cfg.sourcify {
        for c in [&mut obs.proxy, &mut obs.implementation, &mut obs.tax_v3] {
            c.sourcify.push(lookup_sourcify(
                &rpc,
                cfg.limits,
                cfg.sourcify_max_bytes,
                base,
                c.address,
            ));
        }
    }
    let verified_abi: Option<BTreeSet<String>> = match obs
        .implementation
        .sourcify
        .first()
        .map(|s| (&s.facts, s.verified()))
    {
        Some((Ok(f), true)) => Some(f.abi_signatures.iter().cloned().collect()),
        _ => None,
    };

    let launches: Vec<LaunchView> = std::iter::once(historical.clone())
        .chain(scanned.launches.iter().cloned())
        .filter_map(|h| view_launch(h, portal, &deployment, &obs.token_codes))
        .collect();
    let mut fixtures: Vec<FixtureView> = Vec::new();
    let scanned_launches: Vec<&LaunchView> = launches
        .iter()
        .filter(|l| l.hit.source == "RECEIPT_SCAN")
        .collect();
    let direct_v6 = scanned_launches
        .iter()
        .position(|l| l.direct && l.call == "newTokenV6");
    let order = direct_v6
        .into_iter()
        .chain((0..scanned_launches.len()).filter(|i| Some(*i) != direct_v6));
    for i in order.take(cfg.launch_fixtures) {
        fixtures.push(launch_fixture(scanned_launches[i], portal));
    }
    if let Some(h) = launches.iter().find(|l| l.hit.source != "RECEIPT_SCAN") {
        let mut f = launch_fixture(h, portal);
        check(
            &mut f.checks,
            "archived_salt_equal",
            h.decoded.as_ref().map(|d| d.salt) == Some(historical_salt),
            format!("archived {historical_salt}"),
        );
        check(
            &mut f.checks,
            "archived_token_version_equal",
            h.version == Some(historical_version),
            format!("archived {historical_version} live {:?}", h.version),
        );
        fixtures.push(f);
    }
    for h in &scanned.buys {
        fixtures.push(trade_fixture("BUY", h, portal));
    }
    for h in &scanned.sells {
        fixtures.push(trade_fixture("SELL", h, portal));
    }
    for h in &scanned.dexes {
        fixtures.push(trade_fixture("LAUNCHED_TO_DEX", h, portal));
    }
    for h in &scanned.reverted {
        fixtures.push(reverted_fixture(h, portal));
    }

    let taxed_templates: BTreeSet<Address> = launches
        .iter()
        .filter(|l| {
            l.hit.source == "RECEIPT_SCAN"
                && l.version == Some(u64::from(production::TOKEN_VERSION_TAXED_V3))
        })
        .filter_map(|l| l.template)
        .collect();
    let taxed_template = (taxed_templates.len() == 1)
        .then(|| taxed_templates.first().copied())
        .flatten();
    let fetched_txs: Vec<&Tx> = scanned
        .launches
        .iter()
        .chain(&scanned.buys)
        .chain(&scanned.sells)
        .chain(&scanned.dexes)
        .chain(&scanned.reverted)
        .map(|h| &h.tx)
        .collect();
    let selector_obs = selectors(
        &archive,
        &obs,
        verified_abi.as_ref(),
        &scanned.topics,
        &fetched_txs,
        portal,
    );

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
    let eval = Eval {
        common: &common,
        obs: &obs,
        constants: constants(&deployment, chain_id, &obs, taxed_template),
        selectors: selector_obs,
        derivations: derivations(&launches, &archive),
        census: census(&launches, &archive),
        launches,
        fixtures,
        scan: &scanned,
        deployment,
        sourcify_enabled: cfg.sourcify.is_some(),
    };
    let (draft, properties) = record(&eval, &archive);
    let bytes = draft.encode().map_err(|e| e.to_string())?;
    decode(&ff::SCHEMA, &bytes).map_err(|e| e.to_string())?;

    let mut summary = format!(
        "Flap freeze at block {} {} (chain {chain_id}, endpoint {}); Portal version {}\n",
        pin.number,
        pin.hash,
        common.endpoint_host,
        obs.version.clone().unwrap_or_else(|e| e)
    );
    for k in &eval.constants {
        summary.push_str(&format!(
            "  {:<12} {}: witnessed {} observed {}\n",
            k.status(),
            k.name,
            k.witnessed_text
                .clone()
                .or(k.witnessed.as_deref().map(hex_bytes))
                .unwrap_or_default(),
            k.observed_text
                .clone()
                .or(k.observed.as_deref().map(hex_bytes))
                .unwrap_or_default()
        ));
    }
    for (name, status, evidence) in &properties {
        summary.push_str(&format!(
            "  {status:<8} {name}: {}\n",
            bounded_text(evidence)
        ));
    }

    std::fs::create_dir_all(&cfg.out_root)
        .map_err(|e| format!("{}: {e}", cfg.out_root.display()))?;
    let dir = cfg
        .out_root
        .join(format!("flap_freeze_{}", utc_stamp(observed_ms)));
    std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join("flap.dbrec");
    c1_common::write_new(&path, &bytes)?;
    summary.push_str(&format!(
        "\n{} sha256 {} ({} bytes)",
        path.display(),
        B256::from(sha256(&bytes)),
        bytes.len()
    ));
    Ok(Outcome { dir, summary })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
    }

    fn archive() -> Archive {
        Archive::load(&repo_root().join(inputs::ARCHIVE_DEFAULT)).unwrap()
    }

    /// Every frozen signature is declared by both archived IPortal sources
    /// (re-derived without a compiler) and agrees with the archived
    /// web3_sha3 reference where it is listed.
    #[test]
    fn frozen_signatures_are_declared_by_both_archived_iportal_sources() {
        let a = archive();
        let mut referenced = 0;
        for s in abi::specs() {
            let event = s.kind == SpecKind::Event;
            assert!(
                a.official.declares(event, s.signature),
                "official: {}",
                s.signature
            );
            assert!(
                a.interfaces.declares(event, s.signature),
                "interfaces: {}",
                s.signature
            );
            if let Some((_, h)) = a.keccak_reference.iter().find(|(l, _)| l == s.signature) {
                assert_eq!(h[..s.value.len()], s.value[..], "{}", s.signature);
                referenced += 1;
            }
        }
        // newTokenV6, newTokenV7, swapExactInput, swapExactInputV3,
        // buyOnCreation, buy, TokenCreated, TokenBought.
        assert_eq!(referenced, 8);
        assert_eq!(a.version_label(Some(6)), "TOKEN_TAXED_V3");
        assert!(a.version_label(Some(99)).starts_with("UNKNOWN_99"));
    }

    /// The Gate-0 launch reproduces offline: its archived salt, the Portal
    /// and the tax-V3 template give the token, through alloy and through the
    /// production derivation alike.
    #[test]
    fn the_archived_gate0_launch_reproduces_offline() {
        let a = archive();
        let d = bsc_mainnet_v5_24_0();
        let (_, salt, version) = a.historical;
        assert_eq!(version, u64::from(production::TOKEN_VERSION_TAXED_V3));
        let (init_hash, token) = predict_clone(d.portal, salt, d.tax_v3_implementation);
        assert_eq!(init_hash, d.tax_v3_init_code_hash);
        assert_eq!(addr(token), "0xabe1fe16cf6bc48c9524713054efe6e954b17777");
        assert_eq!(d.taxed_v3_token(salt), token);
        // A routed launch carrying the salt at a byte offset is located there.
        let mut input = vec![0xaa; 7];
        input.extend_from_slice(salt.as_slice());
        input.extend_from_slice(&[0xbb; 9]);
        assert_eq!(
            locate_salt(&input, d.portal, d.tax_v3_implementation, token),
            Some((7, salt))
        );
        assert_eq!(
            locate_salt(&input[..30], d.portal, d.tax_v3_implementation, token),
            None
        );
    }

    /// The archived v5.24.0 implementation dispatcher holds the launch and
    /// BUY selectors the production code uses.
    #[test]
    fn the_archived_implementation_dispatcher_holds_the_frozen_selectors() {
        let text = std::fs::read_to_string(
            repo_root()
                .join(inputs::ARCHIVE_DEFAULT)
                .join(inputs::ARCHIVED_IMPLEMENTATION_CODE),
        )
        .unwrap();
        let code = alloy_primitives::hex::decode(text.trim().trim_start_matches("0x")).unwrap();
        assert_eq!(
            sha256(&code),
            bsc_mainnet_v5_24_0().portal_implementation_runtime_sha256
        );
        let scan = CodeScan::new(&code);
        for name in ["newTokenV6", "swapExactInput", "version"] {
            let s = abi::specs().into_iter().find(|s| s.name == name).unwrap();
            assert_eq!(
                scan.selector(s.value[..4].try_into().unwrap()),
                "PUSH4_PRESENT",
                "{name}"
            );
        }
    }

    /// Without observations every property but the record's own versioning
    /// is UNKNOWN, and the record round-trips.
    #[test]
    fn an_empty_observation_is_unknown_and_its_record_round_trips() {
        let a = archive();
        let empty = |role: &str| ContractObs {
            role: role.into(),
            address: Address::ZERO,
            resolution: "test".into(),
            scope: "FROZEN_SURFACE",
            code: Vec::new(),
            implementation: None,
            implementation_code: None,
            sourcify: Vec::new(),
        };
        let obs = Observed {
            proxy: empty("PortalProxy"),
            implementation: empty("PortalImplementation"),
            version: Err("not read".into()),
            tax_v3: empty("TaxV3Implementation"),
            helpers: Vec::new(),
            token_codes: BTreeMap::new(),
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
        let scan = ScanOutcome::default();
        let eval = Eval {
            common: &common,
            obs: &obs,
            constants: constants(&bsc_mainnet_v5_24_0(), 56, &obs, None),
            selectors: selectors(&a, &obs, None, &BTreeMap::new(), &[], Address::ZERO),
            launches: Vec::new(),
            fixtures: Vec::new(),
            derivations: derivations(&[], &a),
            census: BTreeMap::new(),
            scan: &scan,
            deployment: bsc_mainnet_v5_24_0(),
            sourcify_enabled: false,
        };
        let (draft, properties) = record(&eval, &a);
        for (name, status, _) in &properties {
            let expected = if name == "versioned_evidence" {
                "PROVEN"
            } else {
                "UNKNOWN"
            };
            assert_eq!(*status, expected, "{name}");
        }
        let bytes = draft.encode().unwrap();
        let r = decode(&ff::SCHEMA, &bytes).unwrap();
        assert_eq!(r.text(ff::FREEZE_STATUS).unwrap(), "PARTIAL");
        assert_eq!(draft.encode().unwrap(), bytes);
    }

    #[test]
    fn json_stays_inside_the_common_edge() {
        for (name, source) in [
            ("main", include_str!("g01_flap_freeze.rs")),
            ("abi", include_str!("g01_flap_freeze/abi.rs")),
        ] {
            assert!(!source.contains(&["serde", "_json"].concat()), "{name}");
            assert!(!source.contains(&["json", "!"].concat()), "{name}");
        }
    }

    /// The real freeze against the configured free endpoint (network).
    #[test]
    #[ignore = "live read-only BSC RPC and Sourcify"]
    fn live_freeze_against_the_free_rpc() {
        let out = std::env::temp_dir().join(format!("g01-flap-freeze-live-{}", std::process::id()));
        let args = Args::parse([
            "--archive".to_string(),
            repo_root()
                .join(inputs::ARCHIVE_DEFAULT)
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
        let bytes = std::fs::read(outcome.dir.join("flap.dbrec")).unwrap();
        let r = decode(&ff::SCHEMA, &bytes).unwrap();
        assert_eq!(r.u64(ff::CHAIN_ID).unwrap(), 56);
        assert!(r.bool(ff::PIN_RECHECKED).unwrap());
        std::fs::remove_dir_all(out).unwrap();
    }
}

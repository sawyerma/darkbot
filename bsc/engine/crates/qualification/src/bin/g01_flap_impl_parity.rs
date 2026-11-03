//! BSC3-065: the Flap Portal implementation the chain runs against the
//! source-verified one the production pin names, on the paths DarkBot
//! trades, read-only.
//!
//! The real Portal calls of a block range sent directly with `newTokenV6`
//! (LAUNCH) or `swapExactInput` (BUY, SELL; REVERTED by its receipt) are
//! collected first; at most `--per-class` of each class, spread evenly over
//! the class's calls in the range, are then each executed twice by the BSC node
//! (`eth_simulateV1`, no signature) on the state of its parent block, the
//! Portal's EIP-1967 implementation slot holding the source-verified
//! implementation once and the observed one once. In the same simulated
//! block the call is followed by the views of the state it left (the
//! Portal's `getTokenV8Safe` and `buyQuotaOf`, the token's `balanceOf` of
//! the sender), and each sampled launch is executed once more followed by
//! the BUY the C3 path sends for its token (LAUNCH_THEN_BUY: the production
//! encoder at `QUALIFIED_MAINNET_INPUT_WEI` from another sender, the views
//! before and after it). The two executions are compared exactly: status,
//! return data and logs of every call; a log emitter only one execution has
//! is accepted only paired one to one with one of the other's (in the order
//! of their first logs, each then also where a topic, data or return word
//! holds it), both without code at the parent block (contracts the call
//! created: their factory is an implementation immutable). Every other word
//! is compared as it is. Gas is compared and reported, never accepted as
//! equal. Only a case whose traded calls succeeded under both counts towards
//! a path's minimum, which is at least the LIVE qualification's floor. Each
//! parent block's own `version()` (no override) must be the observed
//! version. Both runtimes are read at one pinned block: dispatcher
//! selectors and embedded addresses only one of them has, and whether
//! every selector the production code builds or reads is in the observed
//! dispatcher. One binary record (DBREC001, `darkbot.g01.flap-impl-parity.v1`).
//!
//! Network: `--rpc`, a node that answers `eth_simulateV1` at the range's
//! parent blocks, and `--archive`, an EIP-1898 archive for code and storage
//! (both read-only; `eth_chainId` must be 56). No key is read, nothing is
//! signed or sent.
//!
//! Run from the repository root:
//! `cargo run --locked -p darkbot-bsc-qualification --bin g01_flap_impl_parity -- --rpc URL --archive URL --from-block N`
//! (flags: `usage()`). Output: `bsc/native/evidence/g01/flap_impl_parity_<UTC>/
//! flap_impl_parity.dbrec`; stdout is a derived summary.
#![forbid(unsafe_code)]

#[path = "g01_common/mod.rs"]
mod common;

use alloy_primitives::{Address, B256, U256};
use alloy_sol_types::{SolCall, sol};
use common::bytecode::{CodeScan, eip1967_implementation_slot, strip_metadata};
use common::edge::{self, Endpoint, Pin, SimCall, SimLog, SimResult};
use common::util::{bounded, utc_stamp};
use darkbot_bsc_execution::live_qualification::{PARITY_MIN_RANGE, parity_min_of};
use darkbot_bsc_execution::rpc_wire::Limits;
use darkbot_bsc_execution::sources::RUNTIME_CONFIG_PATH;
use darkbot_bsc_execution::venue_probe::PROBED_SELECTORS;
use darkbot_bsc_qualification::c1_common::{self, Args};
use darkbot_bsc_record::schema::bsc_flap_impl_parity::flap_impl_parity as fp;
use darkbot_bsc_record::{Draft, Value, decode, sha256};
use darkbot_bsc_trading::economics::flap::native_curve20::QUALIFIED_MAINNET_INPUT_WEI;
use darkbot_bsc_trading::venues::flap::abi::{
    NEW_TOKEN_V6_SELECTOR, SWAP_EXACT_INPUT_SELECTOR, encode_swap_exact_input_native,
};
use darkbot_bsc_trading::venues::flap::deployment::bsc_mainnet_v5_27_0;
use darkbot_bsc_trading::venues::flap::recognize::{Recognized, recognize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const TOOL: &str = "g01_flap_impl_parity";

sol! {
    function version() external view returns (string);
    // The views compared after a call (their return data raw) and the
    // Portal reads of the g02 qualification tools.
    function getTokenV8Safe(address token) external view;
    function buyQuotaOf(address token, address origin) external view;
    function balanceOf(address owner) external view;
    struct QuoteExactInputParams {
        address inputToken;
        address outputToken;
        uint256 inputAmount;
    }
    function quoteExactInput(QuoteExactInputParams params) external;
    function maxBuyPerOrigin(address token) external view;
    function nftGate(address token) external view;
}

/// The synthetic sender of the C3 BUY after a launch: an address nobody
/// holds a key of (Keccak-256 of this tool's purpose), funded by a balance
/// override.
fn c3_buyer() -> Address {
    Address::from_slice(
        &alloy_primitives::keccak256(b"darkbot.g01.flap-impl-parity.c3-buyer")[12..],
    )
}

/// Every tunable bound and its default (each overridable by its flag).
mod defaults {
    pub use crate::common::util::net::*;
    /// `--blocks` (1..=40000): blocks scanned from `--from-block`.
    pub const BLOCKS: u64 = 4_000;
    /// `--block-batch` (1..=100): full blocks per request.
    pub const BLOCK_BATCH: u64 = 20;
    /// `--per-class` (1..=1000): cases executed per class; the rest of a
    /// class is skipped (the frequent swaps do not crowd out launches).
    pub const PER_CLASS: u64 = 60;
    /// `--view-gas` (1..=10000000): the gas of each view after a call.
    pub const VIEW_GAS: u64 = 1_000_000;
    /// `--max-calldata` (the decoder's bound when classifying).
    pub const MAX_CALLDATA: usize = 4_096;
    /// `--sim-gas`: the gas a case without a gas limit is executed with.
    pub const SIM_GAS: u64 = 30_000_000;
    /// `--rate-limit-attempts` (1..=20), `--rate-limit-pause-ms`
    /// (1..=60000): a call a free endpoint answers with its rate limit
    /// (JSON-RPC or HTTP 429) is asked again, at most this often.
    pub const RATE_LIMIT_ATTEMPTS: u32 = 6;
    pub const RATE_LIMIT_PAUSE_MS: u64 = 2_000;
}

fn usage() -> &'static str {
    "g01_flap_impl_parity --rpc URL --archive URL --from-block N [--blocks N] [--block-batch N] \
     [--per-class N] [--min-launches N] [--min-buys N] [--min-sells N] [--min-launch-buys N] \
     [--view-gas N] [--observed-implementation 0x..] [--rate-limit-attempts N] [--rate-limit-pause-ms MS] [--out-root DIR]"
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{TOOL}: {e}\nusage: {}", usage());
            ExitCode::from(2)
        }
    }
}

/// The path of a case, as the production decoder and the receipt see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Launch,
    Buy,
    Sell,
    Reverted,
    Other,
    /// A sampled launch followed by the C3 BUY of its token.
    LaunchThenBuy,
}

impl Class {
    const ALL: [Class; 6] = [
        Class::Launch,
        Class::Buy,
        Class::Sell,
        Class::Reverted,
        Class::Other,
        Class::LaunchThenBuy,
    ];
    /// The traded paths and the order of `Config::min`.
    const PATHS: [Class; 4] = [Class::Launch, Class::Buy, Class::Sell, Class::LaunchThenBuy];

    fn name(self) -> &'static str {
        match self {
            Class::Launch => "LAUNCH",
            Class::Buy => "BUY",
            Class::Sell => "SELL",
            Class::Reverted => "REVERTED",
            Class::Other => "OTHER",
            Class::LaunchThenBuy => "LAUNCH_THEN_BUY",
        }
    }
}

/// The token a swap trades (the side that is not native).
fn swap_token(portal: Address, to: Option<Address>, input: &[u8]) -> Option<Address> {
    match recognize(portal, to, input, defaults::MAX_CALLDATA) {
        Recognized::Swap(s) if s.output_token != Address::ZERO => Some(s.output_token),
        Recognized::Swap(s) if s.input_token != Address::ZERO => Some(s.input_token),
        _ => None,
    }
}

/// At most `n` of `items`, spread evenly over them (all when there are no
/// more), in their order.
fn spread<T>(items: Vec<T>, n: usize) -> Vec<T> {
    let len = items.len();
    if len <= n {
        return items;
    }
    let picks: BTreeSet<usize> = (0..n).map(|i| i * len / n).collect();
    items
        .into_iter()
        .enumerate()
        .filter(|(i, _)| picks.contains(i))
        .map(|(_, t)| t)
        .collect()
}

fn class_of(portal: Address, to: Option<Address>, input: &[u8], status: u64) -> Class {
    if status != 1 {
        return Class::Reverted;
    }
    match recognize(portal, to, input, defaults::MAX_CALLDATA) {
        Recognized::LaunchV6(_) => Class::Launch,
        Recognized::Swap(s) if s.is_buy() => Class::Buy,
        Recognized::Swap(s)
            if s.output_token == Address::ZERO && s.input_token != Address::ZERO =>
        {
            Class::Sell
        }
        _ => Class::Other,
    }
}

/// The selectors a dispatcher compares the call's with: `PUSH4 x EQ`, on
/// the metadata-stripped code, in a linear sweep.
fn dispatcher_selectors(code: &[u8]) -> Vec<[u8; 4]> {
    let code = strip_metadata(code);
    let mut out = std::collections::BTreeSet::new();
    let mut pc = 0;
    while pc < code.len() {
        let op = code[pc];
        if (0x60..=0x7f).contains(&op) {
            let width = usize::from(op - 0x5f);
            let end = pc + 1 + width;
            if op == 0x63 && code.get(end) == Some(&0x14) {
                let mut s = [0u8; 4];
                s.copy_from_slice(&code[pc + 1..end]);
                out.insert(s);
            }
            pc = end;
        } else {
            pc += 1;
        }
    }
    out.into_iter().collect()
}

/// A log's canonical bytes: address, topic count, topics, data length, data.
fn log_bytes(logs: &[SimLog]) -> Vec<u8> {
    let mut out = Vec::new();
    for l in logs {
        out.extend_from_slice(l.address.as_slice());
        out.push(l.topics.len() as u8);
        for t in &l.topics {
            out.extend_from_slice(t.as_slice());
        }
        out.extend_from_slice(&(l.data.len() as u32).to_be_bytes());
        out.extend_from_slice(&l.data);
    }
    out
}

/// Every call's canonical bytes: status, return data length and bytes,
/// revert data length and bytes, log count and logs.
fn all_bytes(results: &[SimResult]) -> Vec<u8> {
    let mut out = Vec::new();
    for r in results {
        out.extend_from_slice(&r.status.to_be_bytes());
        out.extend_from_slice(&(r.return_data.len() as u32).to_be_bytes());
        out.extend_from_slice(&r.return_data);
        out.extend_from_slice(&(r.revert_data.len() as u32).to_be_bytes());
        out.extend_from_slice(&r.revert_data);
        out.extend_from_slice(&(r.logs.len() as u32).to_be_bytes());
        out.extend_from_slice(&log_bytes(&r.logs));
    }
    out
}

/// An address right-aligned in a word (twelve zero bytes first).
fn word_address(word: &[u8]) -> Option<Address> {
    (word.len() == 32 && word[..12] == [0u8; 12] && word[12..] != [0u8; 20])
        .then(|| Address::from_slice(&word[12..]))
}

/// The contracts that emit the logs, in the order of their first log. Only
/// an emitter is ever paired: a word that merely looks like an address (an
/// amount below 2^160) is compared as it is.
fn emitters(results: &[SimResult]) -> Vec<Address> {
    let mut out: Vec<Address> = Vec::new();
    for l in results.iter().flat_map(|r| &r.logs) {
        if !out.contains(&l.address) {
            out.push(l.address);
        }
    }
    out
}

/// `words` (32 bytes each) with every address of `map` replaced by its pair.
fn renamed_words(words: &[u8], map: &BTreeMap<Address, Address>) -> Vec<u8> {
    words
        .chunks(32)
        .flat_map(|w| match word_address(w).and_then(|a| map.get(&a)) {
            Some(b) => {
                let mut out = vec![0u8; 12];
                out.extend_from_slice(b.as_slice());
                out
            }
            None => w.to_vec(),
        })
        .collect()
}

/// `results` with every address of `map` replaced by its pair: the
/// emitter, a topic, a data or a return word that holds it.
fn renamed(results: &[SimResult], map: &BTreeMap<Address, Address>) -> Vec<SimResult> {
    results
        .iter()
        .map(|r| SimResult {
            return_data: renamed_words(&r.return_data, map),
            // A custom error: its selector, then its words.
            revert_data: match r.revert_data.split_at_checked(4) {
                Some((selector, words)) => [selector, &renamed_words(words, map)].concat(),
                None => r.revert_data.clone(),
            },
            logs: r
                .logs
                .iter()
                .map(|l| SimLog {
                    address: *map.get(&l.address).unwrap_or(&l.address),
                    topics: l
                        .topics
                        .iter()
                        .map(|t| B256::from_slice(&renamed_words(t.as_slice(), map)))
                        .collect(),
                    data: renamed_words(&l.data, map),
                })
                .collect(),
            ..r.clone()
        })
        .collect()
}

/// How the observed executions compare with the source's, call by call:
/// equal outright, equal once the emitters only one execution has are
/// paired (in the order of their first log; each then also where a topic,
/// data or return word holds it), or the first difference.
#[derive(Debug, PartialEq, Eq)]
enum Compared {
    Identical,
    Renamed(Vec<(Address, Address)>),
    Diff(String),
}

fn compare(source: &[SimResult], observed: &[SimResult]) -> Compared {
    if source.len() != observed.len() {
        return Compared::Diff(format!("{} calls vs {}", source.len(), observed.len()));
    }
    for (i, (s, o)) in source.iter().zip(observed).enumerate() {
        if s.status != o.status {
            return Compared::Diff(format!("call {i}: status {} vs {}", s.status, o.status));
        }
        if s.logs.len() != o.logs.len() {
            return Compared::Diff(format!(
                "call {i}: {} logs vs {}",
                s.logs.len(),
                o.logs.len()
            ));
        }
        if s.return_data.len() != o.return_data.len() {
            return Compared::Diff(format!("call {i}: return data"));
        }
        if s.revert_data.len() != o.revert_data.len() {
            return Compared::Diff(format!("call {i}: revert data"));
        }
    }
    if all_bytes(source) == all_bytes(observed) {
        return Compared::Identical;
    }
    let (s, o) = (emitters(source), emitters(observed));
    let only_s: Vec<Address> = s.iter().filter(|a| !o.contains(a)).copied().collect();
    let only_o: Vec<Address> = o.iter().filter(|a| !s.contains(a)).copied().collect();
    if only_s.is_empty() || only_s.len() != only_o.len() {
        let what = if source
            .iter()
            .zip(observed)
            .any(|(s, o)| s.return_data != o.return_data)
        {
            "return data"
        } else {
            "logs"
        };
        return Compared::Diff(format!(
            "{what} differ ({} vs {} emitters only one execution has)",
            only_s.len(),
            only_o.len()
        ));
    }
    let map: BTreeMap<Address, Address> =
        only_s.iter().copied().zip(only_o.iter().copied()).collect();
    if all_bytes(&renamed(source, &map)) == all_bytes(observed) {
        Compared::Renamed(only_s.into_iter().zip(only_o).collect())
    } else {
        Compared::Diff(
            "return data or logs differ beyond the emitters only one execution has".into(),
        )
    }
}

fn hash32(bytes: &[u8]) -> Value {
    Value::B256(alloy_primitives::keccak256(bytes).0)
}

fn word_of(a: Address) -> B256 {
    let mut w = [0u8; 32];
    w[12..].copy_from_slice(a.as_slice());
    B256::from(w)
}

fn version_of(raw: &[u8]) -> Result<String, String> {
    versionCall::abi_decode_returns(raw).map_err(|e| format!("version(): {e}"))
}

struct Config {
    rpc: String,
    archive: String,
    from: u64,
    blocks: u64,
    batch: u64,
    per_class: u64,
    /// Per `Class::PATHS`.
    min: [u64; 4],
    view_gas: u64,
    observed: Option<Address>,
    out_root: PathBuf,
    rate_limit: (u32, Duration),
    attempts: u32,
    limits: Limits,
    pause: Duration,
}

fn config() -> Result<Config, String> {
    let args = Args::parse(std::env::args().skip(1));
    args.only(&[
        "rpc",
        "archive",
        "from-block",
        "blocks",
        "block-batch",
        "per-class",
        "min-launches",
        "min-buys",
        "min-sells",
        "min-launch-buys",
        "view-gas",
        "observed-implementation",
        "rate-limit-attempts",
        "rate-limit-pause-ms",
        "out-root",
    ])?;
    let observed = match args.opt("observed-implementation")? {
        Some(a) => Some(
            a.parse::<Address>()
                .map_err(|e| format!("--observed-implementation: {e}"))?,
        ),
        None => None,
    };
    Ok(Config {
        rpc: args.one("rpc")?.to_string(),
        archive: args.one("archive")?.to_string(),
        from: args
            .one("from-block")?
            .parse()
            .map_err(|e| format!("--from-block: {e}"))?,
        blocks: bounded(&args, "blocks", defaults::BLOCKS, 1, 40_000)?,
        batch: bounded(&args, "block-batch", defaults::BLOCK_BATCH, 1, 100)?,
        per_class: bounded(&args, "per-class", defaults::PER_CLASS, 1, 1_000)?,
        min: minimums(&args, parity_floor()?)?,
        view_gas: bounded(&args, "view-gas", defaults::VIEW_GAS, 1, 10_000_000)?,
        observed,
        out_root: PathBuf::from(args.opt("out-root")?.unwrap_or(defaults::OUT_ROOT)),
        rate_limit: (
            bounded(
                &args,
                "rate-limit-attempts",
                defaults::RATE_LIMIT_ATTEMPTS,
                1,
                20,
            )?,
            Duration::from_millis(bounded(
                &args,
                "rate-limit-pause-ms",
                defaults::RATE_LIMIT_PAUSE_MS,
                1,
                60_000,
            )?),
        ),
        attempts: defaults::ATTEMPTS,
        limits: Limits {
            connect_timeout_s: defaults::CONNECT_TIMEOUT_S,
            max_time_s: defaults::MAX_TIME_S,
            compressed: true,
        },
        pause: Duration::from_millis(defaults::RETRY_PAUSE_MS),
    })
}

/// Audit E 3.2: the LIVE qualification's floor as the runtime configuration
/// of the repository root (the working directory) sets it.
fn parity_floor() -> Result<u64, String> {
    let text = std::fs::read_to_string(RUNTIME_CONFIG_PATH)
        .map_err(|e| format!("{RUNTIME_CONFIG_PATH}: {e}"))?;
    parity_min_of(&text)
}

/// The executed cases each traded path needs (`Class::PATHS`, `--min-launches`,
/// `--min-buys`, `--min-sells`, `--min-launch-buys`; fewer: INSUFFICIENT): at
/// least the LIVE qualification's `floor` (its default), a run may ask more
/// of itself within the key's range, never less.
fn minimums(args: &Args, floor: u64) -> Result<[u64; 4], String> {
    let mut out = [0u64; 4];
    for (m, flag) in
        out.iter_mut()
            .zip(["min-launches", "min-buys", "min-sells", "min-launch-buys"])
    {
        *m = bounded(args, flag, floor, floor, *PARITY_MIN_RANGE.end())?;
    }
    Ok(out)
}

/// `call` again while the endpoint answers its rate limit, at most
/// `attempts` times `pause` apart; every other answer is final.
fn patient<T>(
    (attempts, pause): (u32, Duration),
    mut call: impl FnMut() -> Result<T, String>,
) -> Result<T, String> {
    let mut n = 1;
    loop {
        match call() {
            Err(e) if n < attempts && rate_limited(&e) => {
                n += 1;
                std::thread::sleep(pause);
            }
            other => return other,
        }
    }
}

/// A JSON-RPC or HTTP answer that is the endpoint's rate limit.
fn rate_limited(error: &str) -> bool {
    error.contains("error 429") || error.contains("HTTP 429") || error.contains("status 429")
}

/// A Portal call of the range, before the sample.
#[derive(Clone)]
struct Candidate {
    class: Class,
    block: u64,
    block_hash: B256,
    parent: Pin,
    tx: edge::Tx,
    tx_index: u64,
    receipt_status: u64,
    receipt_gas_used: Option<u64>,
}

/// One executed case and how it compared.
struct Case {
    class: Class,
    draft: Draft,
    outcome: &'static str,
    /// Its traded calls succeeded under both implementations.
    executed: bool,
    gas: (u64, u64),
}

/// Whether a case's traded calls succeeded under both implementations: the
/// call, in a C3 sequence the launch (call 0) and its BUY (`subject`).
fn executed(s: &[SimResult], o: &[SimResult], subject: usize) -> bool {
    [0, subject].iter().all(|i| {
        s.get(*i)
            .zip(o.get(*i))
            .is_some_and(|(a, b)| a.status == 1 && b.status == 1)
    })
}

/// The verdict of the compared cases: any difference is `DIFF`; a parent
/// block not on the observed version, a production selector missing or a
/// path with fewer executed cases than its minimum is `INSUFFICIENT`.
fn verdict(
    different: u64,
    chain_version_checked: bool,
    production_present: bool,
    executed: &BTreeMap<Class, u64>,
    min: [u64; 4],
    observed_version: &str,
) -> (&'static str, String) {
    let short: Vec<String> = Class::PATHS
        .iter()
        .zip(min)
        .filter(|(c, m)| executed.get(c).copied().unwrap_or(0) < *m)
        .map(|(c, m)| {
            format!(
                "{} {} executed < {m}",
                c.name(),
                executed.get(c).copied().unwrap_or(0)
            )
        })
        .collect();
    if different > 0 {
        ("DIFF", format!("{different} case(s) differ"))
    } else if !chain_version_checked {
        (
            "INSUFFICIENT",
            format!("a parent block's version() is not {observed_version}"),
        )
    } else if !production_present {
        (
            "INSUFFICIENT",
            "a production selector is not in the observed dispatcher".into(),
        )
    } else if !short.is_empty() {
        (
            "INSUFFICIENT",
            format!("too few executed cases: {}", short.join(", ")),
        )
    } else {
        ("BEHAVIOUR_PARITY", String::new())
    }
}

fn run() -> Result<(), String> {
    let cfg = config()?;
    let observed_ms = darkbot_bsc_record::observed_now()?;
    let rpc = Endpoint::new(&cfg.rpc, cfg.limits, cfg.attempts, cfg.pause);
    let archive = Endpoint::new(&cfg.archive, cfg.limits, cfg.attempts, cfg.pause);
    edge::require_bsc(rpc.chain_id()?)?;
    edge::require_bsc(archive.chain_id()?)?;
    let pinned = bsc_mainnet_v5_27_0();
    let portal = pinned.portal;
    let source = pinned.portal_implementation;
    let slot = eip1967_implementation_slot();
    // Both runtimes at one pinned block both endpoints agree on.
    let head = archive.block_number()?.min(rpc.block_number()?) - defaults::CONFIRMATIONS;
    let code_pin: Pin = edge::pin_of(head, &archive.header(head)?)?;
    if rpc.header(head)?.hash != code_pin.hash {
        return Err(format!(
            "block {head}: the endpoints answer different hashes"
        ));
    }
    let observed = match cfg.observed {
        Some(a) => a,
        None => Address::from_slice(&archive.storage(portal, slot, &code_pin)?.0[12..]),
    };
    if observed == source {
        return Err(format!(
            "the Portal runs the pinned implementation {source} at block {head}: nothing to compare"
        ));
    }
    let source_code = archive.code(source, &code_pin)?;
    let observed_code = archive.code(observed, &code_pin)?;
    if sha256(&source_code) != pinned.portal_implementation_runtime_sha256 {
        return Err(format!(
            "{source} at block {head} is not the pinned runtime (SHA-256 differs)"
        ));
    }
    let version_call = SimCall {
        from: Address::ZERO,
        to: portal,
        value: U256::ZERO,
        data: versionCall {}.abi_encode(),
        gas: defaults::SIM_GAS,
    };
    let version_under = |pin: &Pin, implementation: Option<Address>| -> Result<String, String> {
        let storage: Vec<edge::StorageOverride> = implementation
            .map(|i| vec![(portal, slot, word_of(i))])
            .unwrap_or_default();
        let r = patient(cfg.rate_limit, || {
            rpc.simulate(pin, &[], &storage, std::slice::from_ref(&version_call))
        })?;
        version_of(&r[0].return_data)
    };
    let source_version = version_under(&code_pin, Some(source))?;
    let observed_version = version_under(&code_pin, Some(observed))?;
    if source_version != pinned.portal_version {
        return Err(format!(
            "{source} answers version() {source_version}, the pin names {}",
            pinned.portal_version
        ));
    }
    let (source_scan, observed_scan) = (CodeScan::new(&source_code), CodeScan::new(&observed_code));
    let (source_sel, observed_sel) = (
        dispatcher_selectors(&source_code),
        dispatcher_selectors(&observed_code),
    );
    // Every Portal selector production builds or probes, and those the
    // g02 qualification tools and these views read.
    let mut production: Vec<[u8; 4]> = vec![NEW_TOKEN_V6_SELECTOR, SWAP_EXACT_INPUT_SELECTOR];
    production.extend(PROBED_SELECTORS);
    production.extend([
        getTokenV8SafeCall::SELECTOR,
        buyQuotaOfCall::SELECTOR,
        quoteExactInputCall::SELECTOR,
        maxBuyPerOriginCall::SELECTOR,
        nftGateCall::SELECTOR,
    ]);
    production.sort();
    production.dedup();
    let production_present = production
        .iter()
        .all(|s| observed_scan.selector(*s) == "PUSH4_PRESENT" && observed_sel.contains(s));
    let source_addr: Vec<Address> = source_scan.addresses().copied().collect();
    let observed_addr: Vec<Address> = observed_scan.addresses().copied().collect();

    // Every call of the range, by class.
    let to_block = cfg.from.saturating_add(cfg.blocks - 1).min(head);
    if cfg.from > to_block || cfg.from == 0 {
        return Err(format!(
            "--from-block {} is not below the head {head}",
            cfg.from
        ));
    }
    let mut seen: BTreeMap<Class, Vec<Candidate>> = BTreeMap::new();
    let mut blocks_read = 0u64;
    let mut at = cfg.from;
    while at <= to_block {
        let last = at.saturating_add(cfg.batch - 1).min(to_block);
        let headers: BTreeMap<u64, edge::Header> =
            patient(cfg.rate_limit, || rpc.headers(at, last))?
                .into_iter()
                .map(|h| (h.number, h))
                .collect();
        for b in patient(cfg.rate_limit, || rpc.full_blocks(at, last))? {
            blocks_read += 1;
            let header = headers
                .get(&b.number)
                .cloned()
                .ok_or_else(|| format!("block {}: no header", b.number))?;
            if header.hash != b.hash {
                return Err(format!("block {}: header and body differ", b.number));
            }
            let calls: Vec<&edge::Tx> = b
                .transactions
                .iter()
                .filter(|t| {
                    t.to == Some(portal)
                        && matches!(
                            t.input.get(..4),
                            Some(s) if s == NEW_TOKEN_V6_SELECTOR || s == SWAP_EXACT_INPUT_SELECTOR
                        )
                })
                .collect();
            if calls.is_empty() {
                continue;
            }
            let receipts = patient(cfg.rate_limit, || rpc.block_receipts(&header))?;
            edge::require_receipts_of(&header, &receipts)?;
            let parent = Pin {
                number: b.number - 1,
                hash: header.parent_hash,
            };
            for tx in calls {
                let receipt = receipts
                    .iter()
                    .find(|r| r.transaction_hash == tx.hash)
                    .ok_or_else(|| format!("{}: no receipt", tx.hash))?;
                edge::require_tx_at(tx, receipt)?;
                let class = class_of(portal, tx.to, &tx.input, receipt.status);
                seen.entry(class).or_default().push(Candidate {
                    class,
                    block: b.number,
                    block_hash: b.hash,
                    parent,
                    tx: tx.clone(),
                    tx_index: receipt.transaction_index,
                    receipt_status: receipt.status,
                    receipt_gas_used: receipt.gas_used,
                });
            }
        }
        at = last + 1;
    }
    let seen_count: BTreeMap<Class, u64> = seen.iter().map(|(c, v)| (*c, v.len() as u64)).collect();
    let mut sample: Vec<Candidate> = seen
        .into_values()
        .flat_map(|v| spread(v, cfg.per_class as usize))
        .collect();
    sample.sort_by_key(|c| (c.block, c.tx_index));

    // Both executions of every sampled case, the views after it, and each
    // launch once more followed by the C3 BUY of its token.
    let views = |token: Address, holder: Address| -> Vec<SimCall> {
        [
            (portal, getTokenV8SafeCall { token }.abi_encode()),
            (
                portal,
                buyQuotaOfCall {
                    token,
                    origin: holder,
                }
                .abi_encode(),
            ),
            (token, balanceOfCall { owner: holder }.abi_encode()),
        ]
        .into_iter()
        .map(|(to, data)| SimCall {
            from: holder,
            to,
            value: U256::ZERO,
            data,
            gas: cfg.view_gas,
        })
        .collect()
    };
    let buyer = c3_buyer();
    let buy_value = U256::from(QUALIFIED_MAINNET_INPUT_WEI);
    let mut versions_checked: BTreeMap<u64, bool> = BTreeMap::new();
    let mut cases: Vec<Case> = Vec::new();
    let mut launch_buys_seen = 0u64;
    for c in &sample {
        let parent = c.parent;
        let checked = match versions_checked.get(&parent.number) {
            Some(v) => *v,
            None => {
                let v = version_under(&parent, None)? == observed_version;
                versions_checked.insert(parent.number, v);
                v
            }
        };
        let call = SimCall {
            from: c.tx.from,
            to: portal,
            value: c.tx.value,
            data: c.tx.input.clone(),
            gas: c.tx.gas.unwrap_or(defaults::SIM_GAS),
        };
        let under = |i: Address,
                     balances: &[(Address, U256)],
                     calls: &[SimCall]|
         -> Result<Vec<SimResult>, String> {
            let r = patient(cfg.rate_limit, || {
                rpc.simulate(&parent, balances, &[(portal, slot, word_of(i))], calls)
            })?;
            if r.len() != calls.len() {
                return Err(format!(
                    "{}: {} simulated results for {} calls",
                    c.tx.hash,
                    r.len(),
                    calls.len()
                ));
            }
            Ok(r)
        };
        let token = swap_token(portal, c.tx.to, &c.tx.input);
        let mut calls = vec![call.clone()];
        if let Some(t) = token {
            calls.extend(views(t, c.tx.from));
        }
        let (s, o) = (under(source, &[], &calls)?, under(observed, &[], &calls)?);
        let done = executed(&s, &o, 0);
        cases.push(case_of(
            &cfg,
            &archive,
            c,
            c.class,
            &s,
            &o,
            0,
            None,
            token,
            done,
            checked,
            &observed_version,
        )?);
        if c.class != Class::Launch {
            continue;
        }
        launch_buys_seen += 1;
        // The token the launch created (its return word), the same under
        // both; no token: no BUY to follow.
        let Some(t) = word_address(&s[0].return_data).filter(|t| o[0].return_data == word_of(*t).0)
        else {
            continue;
        };
        let buy = encode_swap_exact_input_native(t, buy_value, U256::from(1u8))
            .map_err(|e| format!("{}: the C3 BUY: {e:?}", c.tx.hash))?;
        let mut sequence = vec![call];
        sequence.extend(views(t, buyer));
        let subject = sequence.len();
        sequence.push(SimCall {
            from: buyer,
            to: portal,
            value: buy_value,
            data: buy.to_vec(),
            gas: defaults::SIM_GAS,
        });
        sequence.extend(views(t, buyer));
        // The buyer pays the amount (no gas price in the simulation).
        let funded = [(buyer, buy_value.saturating_mul(U256::from(2u8)))];
        let (s, o) = (
            under(source, &funded, &sequence)?,
            under(observed, &funded, &sequence)?,
        );
        let done = executed(&s, &o, subject);
        cases.push(case_of(
            &cfg,
            &archive,
            c,
            Class::LaunchThenBuy,
            &s,
            &o,
            subject,
            Some((buyer, buy_value)),
            Some(t),
            done,
            checked,
            &observed_version,
        )?);
    }

    // The verdict.
    let count = |c: Class| cases.iter().filter(|x| x.class == c).count() as u64;
    let ran: BTreeMap<Class, u64> = Class::ALL
        .iter()
        .map(|c| {
            (
                *c,
                cases.iter().filter(|x| x.class == *c && x.executed).count() as u64,
            )
        })
        .collect();
    let outcomes = |o: &str| cases.iter().filter(|c| c.outcome == o).count() as u64;
    let (identical, renamed_count, different) = (
        outcomes("IDENTICAL"),
        outcomes("IDENTICAL_BUT_CREATED"),
        outcomes("DIFF"),
    );
    let chain_version_checked =
        !versions_checked.is_empty() && versions_checked.values().all(|v| *v);
    let (verdict, detail) = verdict(
        different,
        chain_version_checked,
        production_present,
        &ran,
        cfg.min,
        &observed_version,
    );
    let detail = if verdict == "BEHAVIOUR_PARITY" {
        format!(
            "{} cases: {identical} identical, {renamed_count} identical once created contracts are paired; executed {}; gas compared separately",
            cases.len(),
            Class::PATHS
                .iter()
                .map(|c| format!("{} {}", c.name(), ran[c]))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        detail
    };

    let mut d = Draft::new(&fp::SCHEMA);
    d.set(fp::CHAIN_ID, edge::BSC_CHAIN_ID)
        .set(fp::PORTAL, Value::Address(portal.into_array()))
        .set(fp::PROVENANCE_TOOL, TOOL)
        .set(fp::PROVENANCE_OBSERVED_UNIX_MS, observed_ms)
        .set(fp::ENDPOINT_HOST, rpc.host())
        .set(fp::ARCHIVE_HOST, archive.host())
        .set(fp::TOOL_SHA256, Value::B256(c1_common::own_sha256()?))
        .set_opt(fp::GIT_HEAD, c1_common::git_head(std::path::Path::new(".")))
        .set(
            fp::SOURCE_IMPLEMENTATION,
            Value::Address(source.into_array()),
        )
        .set(fp::SOURCE_VERSION, source_version.as_str())
        .set(fp::SOURCE_RUNTIME_SHA256, Value::B256(sha256(&source_code)))
        .set(fp::SOURCE_CODE_LEN, source_code.len() as u64)
        .set(
            fp::OBSERVED_IMPLEMENTATION,
            Value::Address(observed.into_array()),
        )
        .set(fp::OBSERVED_VERSION, observed_version.as_str())
        .set(
            fp::OBSERVED_RUNTIME_SHA256,
            Value::B256(sha256(&observed_code)),
        )
        .set(fp::OBSERVED_CODE_LEN, observed_code.len() as u64)
        .set(fp::CODE_BLOCK, code_pin.number)
        .set(fp::CODE_BLOCK_HASH, Value::B256(code_pin.hash.0));
    for s in source_sel.iter().filter(|s| !observed_sel.contains(s)) {
        d.push(fp::SELECTORS_ONLY_SOURCE, Value::Bytes(s.to_vec()));
    }
    for s in observed_sel.iter().filter(|s| !source_sel.contains(s)) {
        d.push(fp::SELECTORS_ONLY_OBSERVED, Value::Bytes(s.to_vec()));
    }
    for s in &production {
        d.push(fp::PRODUCTION_SELECTORS, Value::Bytes(s.to_vec()));
    }
    d.set(fp::PRODUCTION_SELECTORS_PRESENT, production_present);
    for a in source_addr.iter().filter(|a| !observed_addr.contains(a)) {
        d.push(fp::IMMUTABLES_ONLY_SOURCE, Value::Address(a.into_array()));
    }
    for a in observed_addr.iter().filter(|a| !source_addr.contains(a)) {
        d.push(fp::IMMUTABLES_ONLY_OBSERVED, Value::Address(a.into_array()));
    }
    d.set(fp::FROM_BLOCK, cfg.from)
        .set(fp::TO_BLOCK, to_block)
        .set(fp::BLOCKS_READ, blocks_read)
        .set(fp::CHAIN_VERSION_CHECKED, chain_version_checked);
    let mut lines = Vec::new();
    for c in Class::ALL {
        let seen = if c == Class::LaunchThenBuy {
            launch_buys_seen
        } else {
            seen_count.get(&c).copied().unwrap_or(0)
        };
        let mut x = Draft::new(&fp::sample::SCHEMA);
        x.set(fp::sample::CLASS, c.name())
            .set(fp::sample::SEEN, seen)
            .set(fp::sample::SAMPLED, count(c))
            .set(fp::sample::EXECUTED, ran[&c]);
        d.push(fp::SAMPLES, x.into_nested());
        let gas: Vec<(u64, u64)> = cases
            .iter()
            .filter(|x| x.class == c)
            .map(|x| x.gas)
            .collect();
        if gas.is_empty() {
            continue;
        }
        let deltas: Vec<i64> = gas.iter().map(|(s, o)| *o as i64 - *s as i64).collect();
        let (min, max) = (
            deltas.iter().copied().min().unwrap_or(0),
            deltas.iter().copied().max().unwrap_or(0),
        );
        let equal = deltas.iter().filter(|x| **x == 0).count() as u64;
        let most = gas.iter().map(|(_, o)| *o).max().unwrap_or(0);
        lines.push(format!(
            "  {}: {seen} seen, {} sampled, {} executed; gas equal {equal}, delta {min}..={max}, largest observed {most}",
            c.name(),
            gas.len(),
            ran[&c]
        ));
        let mut g = Draft::new(&fp::gas::SCHEMA);
        g.set(fp::gas::CLASS, c.name())
            .set(fp::gas::CASES, gas.len() as u64)
            .set(fp::gas::EQUAL, equal)
            .set(fp::gas::MIN_DELTA, Value::I64(min))
            .set(fp::gas::MAX_DELTA, Value::I64(max))
            .set(fp::gas::MAX_OBSERVED_GAS, most);
        d.push(fp::GAS, g.into_nested());
    }
    d.set(fp::LAUNCHES, count(Class::Launch))
        .set(fp::BUYS, count(Class::Buy))
        .set(fp::SELLS, count(Class::Sell))
        .set(fp::REVERTED, count(Class::Reverted))
        .set(fp::OTHER, count(Class::Other))
        .set(fp::LAUNCH_BUYS, count(Class::LaunchThenBuy));
    let total = cases.len();
    for c in cases {
        d.push(fp::CASES, c.draft.into_nested());
    }
    d.set(fp::MIN_LAUNCHES, cfg.min[0])
        .set(fp::MIN_BUYS, cfg.min[1])
        .set(fp::MIN_SELLS, cfg.min[2])
        .set(fp::MIN_LAUNCH_BUYS, cfg.min[3])
        .set(fp::IDENTICAL, identical)
        .set(fp::IDENTICAL_BUT_CREATED, renamed_count)
        .set(fp::DIFFERENT, different)
        .set(fp::VERDICT, verdict)
        .set(fp::DETAIL, detail.as_str());
    for n in [
        "READ_ONLY: no key read, nothing signed or sent",
        "PARENT_STATE: both executions run on the parent block's state, not on the state at the call's position in its block",
        "NOT_SOURCE_VERIFIED: the observed implementation has no verified source; parity is of behaviour on the executed calls only",
        "STATE_BY_VIEWS: the state a call leaves is compared through the Portal's getTokenV8Safe and buyQuotaOf and the token's balanceOf of the sender in the same simulated block, not slot by slot",
        "C3_BY_SEQUENCE: the C3 BUY is the production encoder's at QUALIFIED_MAINNET_INPUT_WEI (minimum output 1) from one synthetic sender with a balance override, right after each sampled launch on its parent's state, whatever the launch's quote token and profile (wider than the launches classify_launch admits; the production C3 path itself is the economics record's); not the in-block competition",
        "CODE_REACHED_AS_EXECUTED: implementation and helper code is compared only where the sampled calls reach it",
        "GAS_IS_ITS_OWN_EVIDENCE: gas is compared, not equated; gas evidence is taken on the observed implementation",
        "NOT_A_CHAIN_CENSUS: at most --per-class cases per class, spread evenly over the class's calls in the scanned range (so as the range's activity, not its blocks)",
    ] {
        d.push(fp::NON_CLAIMS, n);
    }
    let bytes = d.encode().map_err(|e| e.to_string())?;
    decode(&fp::SCHEMA, &bytes).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cfg.out_root)
        .map_err(|e| format!("{}: {e}", cfg.out_root.display()))?;
    let dir = cfg
        .out_root
        .join(format!("flap_impl_parity_{}", utc_stamp(observed_ms)));
    std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join("flap_impl_parity.dbrec");
    c1_common::write_new(&path, &bytes)?;
    println!(
        "Flap implementation parity {source} ({source_version}) vs {observed} ({observed_version}), blocks {}..={to_block} ({blocks_read} read): {verdict}\n  {detail}\n  {total} cases; selectors only {source_version} {}, only {observed_version} {}; production selectors present {production_present}\n{}\n  {}",
        cfg.from,
        source_sel
            .iter()
            .filter(|s| !observed_sel.contains(s))
            .count(),
        observed_sel
            .iter()
            .filter(|s| !source_sel.contains(s))
            .count(),
        lines.join("\n"),
        path.display()
    );
    Ok(())
}

/// The record of one compared case: `subject` is the call whose execution
/// the case's fields hold (the launch's C3 BUY in a sequence); the
/// emitters only one execution has are proven created by the call (no code
/// at the parent) before they count as paired.
#[allow(clippy::too_many_arguments)]
fn case_of(
    cfg: &Config,
    archive: &Endpoint,
    c: &Candidate,
    class: Class,
    s: &[SimResult],
    o: &[SimResult],
    subject: usize,
    buy: Option<(Address, U256)>,
    token: Option<Address>,
    executed: bool,
    checked: bool,
    observed_version: &str,
) -> Result<Case, String> {
    let (ss, os) = (&s[subject], &o[subject]);
    let mut d = Draft::new(&fp::case::SCHEMA);
    d.set(fp::case::BLOCK_NUMBER, c.block)
        .set(fp::case::BLOCK_HASH, Value::B256(c.block_hash.0))
        .set(fp::case::PARENT_HASH, Value::B256(c.parent.hash.0))
        .set(fp::case::TX_INDEX, c.tx_index)
        .set(fp::case::TX_HASH, Value::B256(c.tx.hash.0))
        .set(fp::case::FROM, Value::Address(c.tx.from.into_array()))
        .set(fp::case::VALUE_WEI, Value::U256(c.tx.value.to_be_bytes()))
        .set(
            fp::case::SELECTOR,
            Value::Bytes(if buy.is_some() {
                SWAP_EXACT_INPUT_SELECTOR.to_vec()
            } else {
                c.tx.input[..4].to_vec()
            }),
        )
        .set(fp::case::CLASS, class.name())
        .set(fp::case::RECEIPT_STATUS, c.receipt_status)
        .set_opt(fp::case::RECEIPT_GAS_USED, c.receipt_gas_used)
        .set(fp::case::SOURCE_STATUS, ss.status)
        .set(fp::case::SOURCE_RETURN_KECCAK, hash32(&ss.return_data))
        .set(fp::case::SOURCE_LOGS, ss.logs.len() as u64)
        .set(fp::case::SOURCE_LOGS_KECCAK, hash32(&log_bytes(&ss.logs)))
        .set(fp::case::SOURCE_GAS, ss.gas_used)
        .set(fp::case::OBSERVED_STATUS, os.status)
        .set(fp::case::OBSERVED_RETURN_KECCAK, hash32(&os.return_data))
        .set(fp::case::OBSERVED_LOGS, os.logs.len() as u64)
        .set(fp::case::OBSERVED_LOGS_KECCAK, hash32(&log_bytes(&os.logs)))
        .set(fp::case::OBSERVED_GAS, os.gas_used)
        .set(
            fp::case::GAS_DELTA,
            Value::I64(os.gas_used as i64 - ss.gas_used as i64),
        )
        .set(fp::case::CALLS, s.len() as u64)
        .set(fp::case::SOURCE_ALL_KECCAK, hash32(&all_bytes(s)))
        .set(fp::case::OBSERVED_ALL_KECCAK, hash32(&all_bytes(o)))
        .set_opt(
            fp::case::TOKEN,
            token.map(|t| Value::Address(t.into_array())),
        );
    if let Some((buyer, value)) = buy {
        d.set(fp::case::PRECEDING_SOURCE_STATUS, s[0].status)
            .set(fp::case::PRECEDING_OBSERVED_STATUS, o[0].status)
            .set(fp::case::BUYER, Value::Address(buyer.into_array()))
            .set(fp::case::BUY_VALUE_WEI, Value::U256(value.to_be_bytes()));
    }
    let (outcome, detail) = match compare(s, o) {
        Compared::Identical => (
            "IDENTICAL",
            "status, return data and logs of every call equal".to_string(),
        ),
        Compared::Diff(why) => ("DIFF", why),
        Compared::Renamed(pairs) => {
            let mut created = true;
            for (a, b) in &pairs {
                let (ca, cb) = (
                    patient(cfg.rate_limit, || archive.code(*a, &c.parent))?,
                    patient(cfg.rate_limit, || archive.code(*b, &c.parent))?,
                );
                let proof = if ca.is_empty() && cb.is_empty() {
                    "CREATED_BY_THE_CALL".to_string()
                } else {
                    created = false;
                    format!(
                        "CODE_AT_PARENT source {} bytes, observed {} bytes",
                        ca.len(),
                        cb.len()
                    )
                };
                let mut r = Draft::new(&fp::renamed::SCHEMA);
                r.set(fp::renamed::SOURCE, Value::Address(a.into_array()))
                    .set(fp::renamed::OBSERVED, Value::Address(b.into_array()))
                    .set(fp::renamed::PROOF, proof);
                d.push(fp::case::RENAMED, r.into_nested());
            }
            if created {
                (
                    "IDENTICAL_BUT_CREATED",
                    format!("equal once {} created contract(s) are paired", pairs.len()),
                )
            } else {
                (
                    "DIFF",
                    "an emitter only one execution has had code before the call".into(),
                )
            }
        }
    };
    let detail = if checked {
        detail
    } else {
        format!("{detail}; the parent block's version() is not {observed_version}")
    };
    d.set(fp::case::OUTCOME, outcome)
        .set(fp::case::DETAIL, detail);
    Ok(Case {
        class,
        draft: d,
        outcome,
        executed,
        gas: (ss.gas_used, os.gas_used),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(address: u8, topics: Vec<B256>, data: Vec<u8>) -> SimLog {
        SimLog {
            address: Address::repeat_byte(address),
            topics,
            data,
        }
    }

    fn result(logs: Vec<SimLog>) -> SimResult {
        SimResult {
            status: 1,
            return_data: vec![7],
            gas_used: 100,
            logs,
            error: None,
            revert_data: Vec::new(),
        }
    }

    fn word(a: u8) -> Vec<u8> {
        word_of(Address::repeat_byte(a)).to_vec()
    }

    #[test]
    fn executions_compare_exactly_up_to_paired_created_addresses() {
        let one = |r: &SimResult| vec![r.clone()];
        let topic = B256::repeat_byte(0xee);
        let s = result(vec![
            log(
                0x11,
                vec![topic, word_of(Address::repeat_byte(0x11))],
                word(0x77),
            ),
            log(0x22, vec![topic], [word(0x11), word(0x22)].concat()),
        ]);
        assert_eq!(compare(&one(&s), &one(&s)), Compared::Identical);
        // The created contracts 0x11, 0x22 are 0x33, 0x44 in the other.
        let o = result(vec![
            log(
                0x33,
                vec![topic, word_of(Address::repeat_byte(0x33))],
                word(0x77),
            ),
            log(0x44, vec![topic], [word(0x33), word(0x44)].concat()),
        ]);
        assert_eq!(
            compare(&one(&s), &one(&o)),
            Compared::Renamed(vec![
                (Address::repeat_byte(0x11), Address::repeat_byte(0x33)),
                (Address::repeat_byte(0x22), Address::repeat_byte(0x44)),
            ])
        );
        // Not one to one: 0x11 is 0x33 once and 0x44 once.
        let crossed = result(vec![
            log(
                0x33,
                vec![topic, word_of(Address::repeat_byte(0x33))],
                word(0x77),
            ),
            log(0x44, vec![topic], [word(0x44), word(0x44)].concat()),
        ]);
        assert!(matches!(
            compare(&one(&s), &one(&crossed)),
            Compared::Diff(_)
        ));
        // Another value differs, even one that looks like an address (an
        // amount below 2^160): never paired.
        let value = result(vec![
            log(
                0x11,
                vec![topic, word_of(Address::repeat_byte(0x11))],
                word(0x78),
            ),
            log(0x22, vec![topic], [word(0x11), word(0x22)].concat()),
        ]);
        assert!(matches!(compare(&one(&s), &one(&value)), Compared::Diff(_)));
        // Paired emitters, and an amount word beside them differs.
        let amount = result(vec![
            log(
                0x33,
                vec![topic, word_of(Address::repeat_byte(0x33))],
                word(0x78),
            ),
            log(0x44, vec![topic], [word(0x33), word(0x44)].concat()),
        ]);
        assert!(matches!(
            compare(&one(&s), &one(&amount)),
            Compared::Diff(_)
        ));
        // Return data or status differ: never renamed.
        let mut other = s.clone();
        other.return_data = vec![8];
        assert!(
            matches!(compare(&one(&s), &one(&other)), Compared::Diff(w) if w.starts_with("return data"))
        );
        other = s.clone();
        other.status = 0;
        assert!(matches!(compare(&one(&s), &one(&other)), Compared::Diff(_)));
        // A log more.
        let mut more = s.clone();
        more.logs.push(log(0x55, vec![], vec![]));
        assert!(matches!(compare(&one(&s), &one(&more)), Compared::Diff(_)));
    }

    #[test]
    fn the_views_after_a_call_are_compared_up_to_the_same_pairs() {
        let topic = B256::repeat_byte(0xee);
        let view = |ret: Vec<u8>| SimResult {
            status: 1,
            return_data: ret,
            gas_used: 10,
            logs: Vec::new(),
            error: None,
            revert_data: Vec::new(),
        };
        let s = vec![
            result(vec![log(0x11, vec![topic], word(0x11))]),
            view([word(0x11), word(0x05)].concat()),
        ];
        // The view names the created contract: paired like the logs.
        let o = vec![
            result(vec![log(0x33, vec![topic], word(0x33))]),
            view([word(0x33), word(0x05)].concat()),
        ];
        assert_eq!(
            compare(&s, &o),
            Compared::Renamed(vec![(
                Address::repeat_byte(0x11),
                Address::repeat_byte(0x33)
            )])
        );
        // The state the view reads differs (a reserve, a balance).
        let state = vec![o[0].clone(), view([word(0x33), word(0x06)].concat())];
        assert!(matches!(compare(&s, &state), Compared::Diff(_)));
        // The same logs, a view differs: no emitter to pair, a difference.
        let only_view = vec![s[0].clone(), view([word(0x11), word(0x06)].concat())];
        assert!(
            matches!(compare(&s, &only_view), Compared::Diff(w) if w.starts_with("return data"))
        );
        // A view reverts in one, a call is missing.
        let mut reverted = s.clone();
        reverted[1].status = 0;
        assert!(matches!(compare(&s, &reverted), Compared::Diff(_)));
        // Both revert: equal only with the same revert data (a custom
        // error and its words; a created contract's word paired).
        let why = |error: [u8; 4], word_of: u8| {
            let mut r = s.clone();
            r[1].status = 0;
            r[1].return_data = Vec::new();
            r[1].revert_data = [error.as_slice(), &word(word_of)].concat();
            r
        };
        let (a, b) = (
            why([0x6a, 0x85, 0x59, 0x77], 0x05),
            why([0x6a, 0x85, 0x59, 0x77], 0x05),
        );
        assert_eq!(compare(&a, &b), Compared::Identical);
        let other_error = why([0x01, 0x02, 0x03, 0x04], 0x05);
        assert!(matches!(compare(&a, &other_error), Compared::Diff(_)));
        let other_word = why([0x6a, 0x85, 0x59, 0x77], 0x06);
        assert!(matches!(compare(&a, &other_word), Compared::Diff(_)));
        let mut created = why([0x6a, 0x85, 0x59, 0x77], 0x11);
        created[0] = result(vec![log(0x11, vec![topic], word(0x11))]);
        let mut paired = why([0x6a, 0x85, 0x59, 0x77], 0x33);
        paired[0] = result(vec![log(0x33, vec![topic], word(0x33))]);
        assert!(matches!(compare(&created, &paired), Compared::Renamed(_)));
        assert!(matches!(compare(&s, &s[..1]), Compared::Diff(_)));
    }

    #[test]
    fn the_sample_is_spread_evenly_over_the_range() {
        assert_eq!(spread((0..10).collect(), 3), vec![0, 3, 6]);
        assert_eq!(spread((0..2).collect(), 3), vec![0, 1]);
        assert_eq!(spread((0..61).collect::<Vec<u32>>(), 60).len(), 60);
        assert_eq!(spread::<u8>(Vec::new(), 3), Vec::<u8>::new());
    }

    #[test]
    fn only_executed_cases_count_and_the_minimum_is_at_least_the_floor() {
        let floor =
            parity_min_of(include_str!("../../../../../config/ingress.public.env")).unwrap();
        let full: BTreeMap<Class, u64> = Class::PATHS.iter().map(|c| (*c, floor)).collect();
        let min = [floor; 4];
        assert_eq!(
            verdict(0, true, true, &full, min, "v").0,
            "BEHAVIOUR_PARITY"
        );
        // Every BUY sampled, one fewer executed (it reverted under both).
        let mut short = full.clone();
        short.insert(Class::Buy, floor - 1);
        let (v, why) = verdict(0, true, true, &short, min, "v");
        assert_eq!(v, "INSUFFICIENT");
        assert!(
            why.contains(&format!("BUY {} executed", floor - 1)),
            "{why}"
        );
        // No C3 sequence executed.
        let mut no_c3 = full.clone();
        no_c3.remove(&Class::LaunchThenBuy);
        assert_eq!(verdict(0, true, true, &no_c3, min, "v").0, "INSUFFICIENT");
        assert_eq!(verdict(1, true, true, &full, min, "v").0, "DIFF");
        assert_eq!(verdict(0, false, true, &full, min, "v").0, "INSUFFICIENT");
        assert_eq!(verdict(0, true, false, &full, min, "v").0, "INSUFFICIENT");
        // A case counts only executed under both: in a C3 sequence the
        // launch and its BUY.
        let (ok, reverted) = (
            result(Vec::new()),
            SimResult {
                status: 0,
                ..result(Vec::new())
            },
        );
        assert!(executed(
            std::slice::from_ref(&ok),
            std::slice::from_ref(&ok),
            0
        ));
        assert!(!executed(
            std::slice::from_ref(&ok),
            std::slice::from_ref(&reverted),
            0
        ));
        assert!(!executed(
            std::slice::from_ref(&reverted),
            std::slice::from_ref(&reverted),
            0
        ));
        let seq =
            |launch: &SimResult, buy: &SimResult| vec![launch.clone(), ok.clone(), buy.clone()];
        assert!(executed(&seq(&ok, &ok), &seq(&ok, &ok), 2));
        assert!(!executed(&seq(&ok, &reverted), &seq(&ok, &ok), 2));
        assert!(!executed(&seq(&reverted, &ok), &seq(&reverted, &ok), 2));
        // A run may raise a minimum, never lower it below the floor.
        let args = |v: &[&str]| Args::parse(v.iter().map(|s| s.to_string()));
        assert_eq!(minimums(&args(&[]), floor).unwrap(), [floor; 4]);
        assert_eq!(
            minimums(&args(&["--min-buys", "40"]), floor).unwrap()[1],
            40
        );
        for flag in [
            "--min-launches",
            "--min-buys",
            "--min-sells",
            "--min-launch-buys",
        ] {
            assert!(minimums(&args(&[flag, "1"]), floor).is_err(), "{flag}");
        }
    }

    #[test]
    fn the_c3_buyer_is_derived_and_a_swap_names_its_token() {
        assert_eq!(c3_buyer(), c3_buyer());
        assert_ne!(c3_buyer(), Address::ZERO);
        let portal = bsc_mainnet_v5_27_0().portal;
        let token = Address::repeat_byte(0x77);
        let buy = encode_swap_exact_input_native(token, U256::from(5u8), U256::from(1u8)).unwrap();
        assert_eq!(swap_token(portal, Some(portal), &buy), Some(token));
        let sell = darkbot_bsc_trading::venues::flap::abi::encode_swap_exact_input_sell(
            token,
            U256::from(5u8),
            U256::from(1u8),
        )
        .unwrap();
        assert_eq!(swap_token(portal, Some(portal), &sell), Some(token));
        assert_eq!(swap_token(portal, Some(portal), &[0xef]), None);
    }

    #[test]
    fn only_a_rate_limit_is_asked_again_and_only_so_often() {
        let quick = (3, Duration::from_millis(1));
        let mut n = 0;
        let r: Result<u8, String> = patient(quick, || {
            n += 1;
            Err("JSON-RPC eth_getCode: error 429: exceeded".into())
        });
        assert!(r.is_err());
        assert_eq!(n, 3);
        n = 0;
        let r: Result<u8, String> = patient(quick, || {
            n += 1;
            if n < 2 { Err("HTTP 429".into()) } else { Ok(7) }
        });
        assert_eq!((r, n), (Ok(7), 2));
        n = 0;
        let r: Result<u8, String> = patient(quick, || {
            n += 1;
            Err("JSON-RPC eth_call: error -32000: execution reverted".into())
        });
        assert!(r.is_err());
        assert_eq!(n, 1);
    }

    #[test]
    fn the_dispatcher_selectors_are_push4_eq() {
        // PUSH4 a EQ, PUSH4 b GT (no selector), PUSH32 holding 63..14.
        let mut code = vec![0x63, 1, 2, 3, 4, 0x14, 0x63, 5, 6, 7, 8, 0x11, 0x7f];
        let mut word = [0u8; 32];
        word[0] = 0x63;
        word[5] = 0x14;
        code.extend_from_slice(&word);
        code.push(0x00);
        assert_eq!(dispatcher_selectors(&code), vec![[1, 2, 3, 4]]);
    }

    #[test]
    fn a_case_is_classed_by_the_production_decoder_and_its_receipt() {
        let portal = bsc_mainnet_v5_27_0().portal;
        assert_eq!(class_of(portal, Some(portal), &[0xef], 0), Class::Reverted);
        assert_eq!(
            class_of(portal, Some(portal), &SWAP_EXACT_INPUT_SELECTOR, 1),
            Class::Other
        );
    }
}

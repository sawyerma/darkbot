//! G0.6a native economics reconfirmed read-only on the current Flap Portal
//! (Phase D gate review HIGH-1).
//!
//! The witnessed G0.6a native launch (salt, launcher and buyer from the
//! committed G0.6a record; launch members of
//! `bsc/tools/g06a_economics_evm_crosscheck.cjs`, the profile of
//! `trading::venues::flap::profile`) is encoded with the production
//! `encode_new_token_v6`, recognized with the production `recognize`, its
//! token predicted with the production `predicted_token`. Then, on state
//! pinned to one current block, the launch followed by a native
//! `swapExactInput` BUY (production `encode_swap_exact_input_native`,
//! minimum output 1) is executed twice: by the BSC node (`eth_simulateV1`)
//! and by the independent local EVM of G0.6a
//! (`bsc/tools/g01_flap_econ_evm.cjs`: EthereumJS over eth_getProof,
//! eth_getCode and eth_getStorageAt state pinned to the same block hash).
//! Both are compared with `quote_native_buy` under the admission's witnessed
//! terms, for the admission's qualified input and the committed record's.
//!
//! The fee-exemption entry the startup probe reads
//! (`venue_probe::fee_exemption_key`, Phase D gate pass 2 LOW-A) is
//! witnessed twice: the Sourcify storage layout of the deployment's
//! implementation must name
//! `_feeExemptedTraders` (`mapping(address => bool)`) at
//! `FEE_EXEMPTION_MAPPING_SLOT`, and the node's qualified BUY with the
//! buyer's entry overridden to `true` must pay a different fee than without.
//!
//! Before anything else the live deployment must validate as
//! `bsc_mainnet_v5_27_0()`. No key is read; nothing is signed, sent or
//! persisted; caller balances exist only in the simulations. One binary
//! record: `bsc/native/evidence/g01/flap_econ_v5_27_<UTC>/flap_econ.dbrec`.
//!
//! Run from the repository root:
//! `cargo run --locked -p darkbot-bsc-qualification --bin g01_flap_econ`.
#![forbid(unsafe_code)]

#[path = "g01_common/mod.rs"]
mod common;

use alloy_primitives::{Address, B256, U256, keccak256};
use alloy_sol_types::{SolCall, sol};
use common::bytecode::{addr, eip1967_implementation_slot};
use common::edge::{
    self, CallOutcome, Endpoint, Pin, SimCall, SimResult, SourcifyLayout, StorageOverride,
};
use common::util::{bounded_text, number, rpc_from_config, utc_stamp};
use darkbot_bsc_execution::rpc_wire::Limits;
use darkbot_bsc_execution::venue_probe::{FEE_EXEMPTION_MAPPING_SLOT, fee_exemption_key};
use darkbot_bsc_qualification::c1_common::{self, Args};
use darkbot_bsc_record::schema::bsc_flap_econ::flap_econ as fe;
use darkbot_bsc_record::schema::bsc_four_freeze::four_freeze as shared;
use darkbot_bsc_record::schema::g06a::economics;
use darkbot_bsc_record::{Draft, Record, Value, decode, sha256};
use darkbot_bsc_trading::economics::flap::native_curve20::{
    HoldingCap, NativeBuyInputs, NativeBuyQuote, PricedToken, QUALIFIED_MAINNET_INPUT_WEI,
    WITNESSED_MAINNET_TERMS, profile_terms, quote_native_buy,
};
use darkbot_bsc_trading::venues::flap::abi::{
    LaunchParamsV6, encode_new_token_v6, encode_swap_exact_input_native,
};
use darkbot_bsc_trading::venues::flap::deployment::{
    FlapDeployment, ObservedDeployment, bsc_mainnet_v5_27_0,
};
use darkbot_bsc_trading::venues::flap::profile::classify_launch;
use darkbot_bsc_trading::venues::flap::recognize::{Recognized, predicted_token, recognize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

const TOOL: &str = "g01_flap_econ";

sol! {
    function balanceOf(address owner) external view returns (uint256);
    function version() external view returns (string);
}

/// The evidence inputs (relative to the repository root).
mod inputs {
    /// The committed G0.6a EVM crosscheck: salt, callers, token, the BUY
    /// amount it executed and its output (block 129392405, Portal v5.26.0).
    pub const G06A_RECORD: &str =
        "bsc/native/evidence/g06a/g06a_prepare_20261023T211704Z/economics_crosscheck.dbrec";
    /// The local EVM half (EthereumJS, the G0.6a method).
    pub const SIMULATOR: &str = "bsc/tools/g01_flap_econ_evm.cjs";
}

/// Every tunable bound and its default (each overridable by its flag).
mod defaults {
    pub use crate::common::util::net::*;
    /// `--launch-gas-margin-percent`: the launch gas limit above the node's
    /// estimate (G0.6a: 120 %).
    pub const LAUNCH_GAS_MARGIN_PERCENT: u64 = 20;
    /// `--buy-gas`, `--gas-price-wei`, `--caller-balance-wei`: G0.6a's BUY
    /// gas limit, gas price and local caller balance (100 BNB).
    pub const BUY_GAS: u64 = 3_000_000;
    pub const GAS_PRICE_WEI: u64 = 50_000_000;
    pub const CALLER_BALANCE_WEI: u128 = 100_000_000_000_000_000_000;
    /// `--balance-gas`: gas of the simulated `balanceOf`.
    pub const BALANCE_GAS: u64 = 500_000;
    /// `--node`: the Node.js program of the local EVM.
    pub const NODE: &str = "node";
    /// `--local-evm-rpc`: the local EVM's state endpoint; it must serve
    /// `eth_getProof` at a recent block (the free dataseed refuses it).
    pub const LOCAL_EVM_RPC: &str = "https://bsc-mainnet.public.blastapi.io";
}

/// The Portal's state variable the probe reads for the caller's exemption.
const EXEMPTION_VARIABLE: &str = "_feeExemptedTraders";
/// Its type in the storage layout.
const EXEMPTION_TYPE_LABEL: &str = "mapping(address => bool)";

fn usage() -> &'static str {
    "usage: g01_flap_econ [--rpc <url> | --rpc-config <file>] [--local-evm-rpc <url>] \
     [--g06a-record <dbrec>] [--simulator <cjs>] [--node <program>] [--out-root <dir>] \
     [--confirmations <n>] [--buy-value-wei <wei>]... [--launch-gas-margin-percent <n>] \
     [--buy-gas <n>] [--gas-price-wei <n>] [--caller-balance-wei <n>] [--balance-gas <n>] \
     [--rpc-connect-timeout-s <s>] [--rpc-max-time-s <s>] [--rpc-attempts <n>] \
     [--rpc-retry-pause-ms <ms>] [--sourcify-base <url>] [--sourcify-max-bytes <n>]"
}

#[derive(Clone, Debug)]
struct Config {
    rpc_url: String,
    local_evm_rpc: String,
    g06a_record: PathBuf,
    simulator: PathBuf,
    node: String,
    out_root: PathBuf,
    confirmations: u64,
    /// Explicit BUY amounts; empty: the qualified input and the committed one.
    buy_values: Vec<U256>,
    launch_gas_margin: u64,
    buy_gas: u64,
    gas_price: u64,
    caller_balance: U256,
    balance_gas: u64,
    limits: Limits,
    attempts: u32,
    retry_pause: Duration,
    sourcify_base: String,
    sourcify_max_bytes: u64,
}

impl Config {
    fn parse(args: &Args) -> Result<Self, String> {
        args.only(&[
            "rpc",
            "rpc-config",
            "local-evm-rpc",
            "g06a-record",
            "simulator",
            "node",
            "out-root",
            "confirmations",
            "buy-value-wei",
            "launch-gas-margin-percent",
            "buy-gas",
            "gas-price-wei",
            "caller-balance-wei",
            "balance-gas",
            "rpc-connect-timeout-s",
            "rpc-max-time-s",
            "rpc-attempts",
            "rpc-retry-pause-ms",
            "sourcify-base",
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
        let buy_values = args
            .many("buy-value-wei")
            .into_iter()
            .map(|v| {
                v.parse::<U256>()
                    .map_err(|_| format!("--buy-value-wei: not a number: {v}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            local_evm_rpc: args
                .opt("local-evm-rpc")?
                .unwrap_or(defaults::LOCAL_EVM_RPC)
                .to_string(),
            rpc_url,
            g06a_record: PathBuf::from(args.opt("g06a-record")?.unwrap_or(inputs::G06A_RECORD)),
            simulator: PathBuf::from(args.opt("simulator")?.unwrap_or(inputs::SIMULATOR)),
            node: args.opt("node")?.unwrap_or(defaults::NODE).to_string(),
            out_root: PathBuf::from(args.opt("out-root")?.unwrap_or(defaults::OUT_ROOT)),
            confirmations: number(args, "confirmations", defaults::CONFIRMATIONS)?,
            buy_values,
            launch_gas_margin: number(
                args,
                "launch-gas-margin-percent",
                defaults::LAUNCH_GAS_MARGIN_PERCENT,
            )?,
            buy_gas: number(args, "buy-gas", defaults::BUY_GAS)?,
            gas_price: number(args, "gas-price-wei", defaults::GAS_PRICE_WEI)?,
            caller_balance: U256::from(number(
                args,
                "caller-balance-wei",
                defaults::CALLER_BALANCE_WEI,
            )?),
            balance_gas: number(args, "balance-gas", defaults::BALANCE_GAS)?,
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
            sourcify_base: args
                .opt("sourcify-base")?
                .unwrap_or(defaults::SOURCIFY_BASE)
                .to_string(),
            sourcify_max_bytes: match number(
                args,
                "sourcify-max-bytes",
                defaults::SOURCIFY_MAX_BYTES,
            )? {
                n if n > defaults::SOURCIFY_MAX_BYTES => {
                    return Err(format!(
                        "--sourcify-max-bytes above the record bound {}",
                        defaults::SOURCIFY_MAX_BYTES
                    ));
                }
                n => n,
            },
        })
    }
}

fn main() -> ExitCode {
    let args = Args::parse(std::env::args().skip(1));
    let cfg = match Config::parse(&args) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("g01_flap_econ: {e}\n{}", usage());
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
            eprintln!("g01_flap_econ: FAILED, no record written: {e}");
            ExitCode::FAILURE
        }
    }
}

// ---- the committed G0.6a record -----------------------------------------------------

/// A field of `record` by its schema name.
fn named<'r>(record: &'r Record, name: &str) -> Result<&'r Value, String> {
    let field = record
        .schema()
        .fields
        .iter()
        .find(|f| f.name == name)
        .ok_or_else(|| format!("{}: no field {name}", record.schema().name))?;
    record
        .get(field.tag)
        .ok_or_else(|| format!("{}: {name} missing", record.schema().name))
}

fn b256_value(v: &Value) -> Result<B256, String> {
    match v {
        Value::B256(w) | Value::U256(w) => Ok(B256::from(*w)),
        other => Err(format!("{other:?} is not 32 bytes")),
    }
}

fn address_value(v: &Value) -> Result<Address, String> {
    match v {
        Value::Address(a) => Ok(Address::from(*a)),
        other => Err(format!("{other:?} is not an address")),
    }
}

fn u256_value(v: &Value) -> Result<U256, String> {
    match v {
        Value::U256(w) => Ok(U256::from_be_bytes(*w)),
        Value::U64(n) => Ok(U256::from(*n)),
        other => Err(format!("{other:?} is not an amount")),
    }
}

/// What the committed G0.6a crosscheck executed.
#[derive(Clone, Debug)]
struct G06a {
    path: String,
    sha256: [u8; 32],
    block: u64,
    salt: B256,
    token: Address,
    launcher: Address,
    buyer: Address,
    buy_value: U256,
    output: U256,
}

fn g06a(path: &Path) -> Result<G06a, String> {
    let bytes = c1_common::read(path)?;
    let r = decode(&economics::NATIVE, &bytes).map_err(|e| e.to_string())?;
    if !matches!(named(&r, "status")?, Value::Text(t) if t == "EVM_SEQUENCE_PASS") {
        return Err("the committed G0.6a record is not EVM_SEQUENCE_PASS".into());
    }
    Ok(G06a {
        path: path.display().to_string(),
        sha256: sha256(&bytes),
        block: match named(&r, "state_source.block_number")? {
            Value::U64(n) => *n,
            other => return Err(format!("block {other:?}")),
        },
        salt: b256_value(named(&r, "salt")?)?,
        token: address_value(named(&r, "token")?)?,
        launcher: address_value(named(&r, "launcher")?)?,
        buyer: address_value(named(&r, "buyer")?)?,
        buy_value: u256_value(named(&r, "buy_value_wei")?)?,
        output: u256_value(named(&r, "simulated_received_amount")?)?,
    })
}

/// The witnessed G0.6a native launch: the members of `launchCalldata()` in
/// `bsc/tools/g06a_economics_evm_crosscheck.cjs` (the profile the production
/// `classify_launch` admits), with the committed salt and launcher.
fn witnessed_launch(salt: B256, launcher: Address) -> LaunchParamsV6<'static> {
    LaunchParamsV6 {
        name: b"DarkBot G06A Economics",
        symbol: b"DBG06E",
        uri: b"ipfs://darkbot-g06a-economics",
        dex_thresh: 1,
        salt,
        migrator_type: 1,
        quote_token: Address::ZERO,
        quote_amt: U256::ZERO,
        beneficiary: launcher,
        permit: &[],
        extension_id: B256::ZERO,
        extension_data: &[],
        dex_id: 0,
        lp_fee_profile: 0,
        buy_tax: 100,
        sell_tax: 100,
        tax_duration: 31_536_000,
        anti_farmer_duration: 86_400,
        mkt_bps: 10_000,
        deflation_bps: 0,
        dividend_bps: 0,
        lp_bps: 0,
        minimum_share_balance: U256::ZERO,
        dividend_token: Address::ZERO,
        commission_receiver: Address::ZERO,
        token_version: 6,
    }
}

/// The admission's arithmetic: `quote_native_buy` under the witnessed venue
/// terms with the launch's buy tax, before any trade (what
/// `admit_native_buy` computes for its qualified input).
fn rust_quote(value: U256, buy_tax_bps: u16) -> Result<NativeBuyQuote, String> {
    quote_native_buy(&NativeBuyInputs {
        token: PricedToken::new(&profile_terms(buy_tax_bps, buy_tax_bps))
            .map_err(|e| format!("{e:?}"))?,
        reserve_before: U256::ZERO,
        circulating_before: U256::ZERO,
        input_amount: value,
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
        caller_fee_exempt: WITNESSED_MAINNET_TERMS.caller_fee_exempt,
        holding_cap: HoldingCap::None,
    })
    .map_err(|e| format!("{e:?}"))
}

// ---- the executions ------------------------------------------------------------------

/// The facts of one execution of launch + BUY.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Execution {
    status: String,
    launch_status: Option<u64>,
    token: Option<Address>,
    launch_gas: Option<u64>,
    buy_status: Option<u64>,
    buy_return: Option<U256>,
    balance: Option<U256>,
    bought_amount: Option<U256>,
    bought_eth: Option<U256>,
    bought_fee: Option<U256>,
    bought_post_price: Option<U256>,
    tax_paid: Option<U256>,
    buy_gas: Option<u64>,
    launch_calldata_keccak: Option<B256>,
    buy_calldata_keccak: Option<B256>,
    /// Where the execution's state came from.
    state_source: String,
    error: Option<String>,
}

fn word(data: &[u8], i: usize) -> Option<U256> {
    data.get(i * 32..i * 32 + 32)
        .map(|w| U256::from_be_slice(w))
}

fn token_bought_topic() -> B256 {
    keccak256("TokenBought(uint256,address,address,uint256,uint256,uint256,uint256)")
}

fn tax_paid_topics() -> [B256; 2] {
    [
        keccak256("TaxOnBondingCurvePaid(address,uint256)"),
        keccak256("TaxV2OnBondingCurvePaid(address,uint256)"),
    ]
}

fn node_execution(results: &[SimResult], portal: Address) -> Execution {
    let mut e = Execution {
        status: "SUCCESS".into(),
        state_source: "the node's eth_simulateV1 at the pinned block hash".into(),
        ..Execution::default()
    };
    let [launch, buy, balance] = results else {
        e.status = "INCOMPLETE".into();
        return e;
    };
    e.launch_status = Some(launch.status);
    e.launch_gas = Some(launch.gas_used);
    e.token = (launch.return_data.len() == 32 && launch.return_data[..12] == [0u8; 12])
        .then(|| Address::from_slice(&launch.return_data[12..]));
    e.buy_status = Some(buy.status);
    e.buy_gas = Some(buy.gas_used);
    e.buy_return = word(&buy.return_data, 0);
    e.balance = word(&balance.return_data, 0);
    for log in buy.logs.iter().filter(|l| l.address == portal) {
        match log.topics.first() {
            Some(t) if *t == token_bought_topic() => {
                e.bought_amount = word(&log.data, 3);
                e.bought_eth = word(&log.data, 4);
                e.bought_fee = word(&log.data, 5);
                e.bought_post_price = word(&log.data, 6);
            }
            Some(t) if tax_paid_topics().contains(t) => e.tax_paid = word(&log.data, 0),
            _ => {}
        }
    }
    if launch.status != 1 || buy.status != 1 {
        e.status = "REVERTED".into();
        e.error = launch.error.clone().or_else(|| buy.error.clone());
    }
    e
}

/// The local EVM's `KEY=value` lines (its output; never JSON).
fn local_execution(stdout: &str) -> Execution {
    let kv: BTreeMap<&str, &str> = stdout.lines().filter_map(|l| l.split_once('=')).collect();
    let u = |k: &str| kv.get(k).and_then(|v| v.parse::<U256>().ok());
    let n = |k: &str| kv.get(k).and_then(|v| v.parse::<u64>().ok());
    let b = |k: &str| kv.get(k).and_then(|v| v.parse::<B256>().ok());
    Execution {
        status: kv
            .get("STATUS")
            .map_or("NO_STATUS".into(), |s| s.to_string()),
        launch_status: None,
        token: kv.get("TOKEN").and_then(|v| v.parse().ok()),
        launch_gas: n("LAUNCH_GAS"),
        buy_status: None,
        buy_return: u("BUY_RETURN"),
        balance: u("BUYER_BALANCE"),
        bought_amount: u("BOUGHT_AMOUNT"),
        bought_eth: u("BOUGHT_ETH"),
        bought_fee: u("BOUGHT_FEE"),
        bought_post_price: None,
        tax_paid: u("TAX_PAID"),
        buy_gas: n("BUY_GAS"),
        launch_calldata_keccak: b("LAUNCH_CALLDATA_KECCAK"),
        buy_calldata_keccak: b("BUY_CALLDATA_KECCAK"),
        state_source: format!(
            "eth_getProof ({} accounts, codeHash = keccak256(eth_getCode)), eth_getCode, eth_getStorageAt at the pinned block hash",
            kv.get("PROVEN_ACCOUNTS").unwrap_or(&"NONE")
        ),
        error: None,
    }
}

struct Run<'a> {
    cfg: &'a Config,
    rpc: &'a Endpoint,
    pin: Pin,
    portal: Address,
    g06a: &'a G06a,
    launch: &'a [u8],
    launch_gas: u64,
    token: Address,
}

impl Run<'_> {
    /// The launch and the BUY by the node; `storage` overrides words of the
    /// pinned state for this simulation only.
    fn node(&self, value: U256, buy: &[u8], storage: &[StorageOverride]) -> Execution {
        let calls = [
            SimCall {
                from: self.g06a.launcher,
                to: self.portal,
                value: U256::ZERO,
                data: self.launch.to_vec(),
                gas: self.launch_gas,
            },
            SimCall {
                from: self.g06a.buyer,
                to: self.portal,
                value,
                data: buy.to_vec(),
                gas: self.cfg.buy_gas,
            },
            SimCall {
                from: self.g06a.buyer,
                to: self.token,
                value: U256::ZERO,
                data: balanceOfCall {
                    owner: self.g06a.buyer,
                }
                .abi_encode(),
                gas: self.cfg.balance_gas,
            },
        ];
        let balances = [
            (self.g06a.launcher, self.cfg.caller_balance),
            (self.g06a.buyer, self.cfg.caller_balance),
        ];
        match self.rpc.simulate(&self.pin, &balances, storage, &calls) {
            Ok(results) => node_execution(&results, self.portal),
            Err(e) => Execution {
                status: "NODE_UNAVAILABLE".into(),
                error: Some(e),
                ..Execution::default()
            },
        }
    }

    fn local(&self, value: U256) -> Execution {
        let mut node = Command::new(&self.cfg.node);
        node.env_clear();
        if let Some(path) = std::env::var_os("PATH") {
            node.env("PATH", path);
        }
        // The endpoint reaches the simulator in its environment, not argv.
        node.env("BSC_MAINNET_RPC_HTTPS", &self.cfg.local_evm_rpc)
            .arg(&self.cfg.simulator)
            .args(["--block", &self.pin.number.to_string()])
            .args(["--hash", &self.pin.hash.to_string()])
            .args(["--salt", &self.g06a.salt.to_string()])
            .args(["--launcher", &addr(self.g06a.launcher)])
            .args(["--buyer", &addr(self.g06a.buyer)])
            .args(["--buy-value", &value.to_string()])
            .args(["--launch-gas", &self.launch_gas.to_string()])
            .args(["--buy-gas", &self.cfg.buy_gas.to_string()])
            .args(["--balance-gas", &self.cfg.balance_gas.to_string()])
            .args(["--gas-price", &self.cfg.gas_price.to_string()])
            .args(["--caller-balance", &self.cfg.caller_balance.to_string()])
            .args([
                "--rpc-timeout-ms",
                &(self.cfg.limits.max_time_s * 1000).to_string(),
            ])
            .args(["--rpc-attempts", &self.cfg.attempts.to_string()])
            .args([
                "--rpc-retry-pause-ms",
                &self.cfg.retry_pause.as_millis().to_string(),
            ]);
        match node.output() {
            Ok(out) if out.status.success() => {
                local_execution(&String::from_utf8_lossy(&out.stdout))
            }
            Ok(out) => Execution {
                status: "LOCAL_EVM_FAILED".into(),
                error: Some(bounded_text(&String::from_utf8_lossy(&out.stderr))),
                ..Execution::default()
            },
            Err(e) => Execution {
                status: "LOCAL_EVM_UNAVAILABLE".into(),
                error: Some(format!("{}: {e}", self.cfg.node)),
                ..Execution::default()
            },
        }
    }
}

type Checks = Vec<(String, bool, String)>;

fn check(checks: &mut Checks, name: &str, pass: bool, detail: impl Into<String>) {
    checks.push((name.to_string(), pass, detail.into()));
}

fn show(v: Option<U256>) -> String {
    v.map_or("NONE".into(), |x| x.to_string())
}

/// The comparisons of one execution with the prediction and the quote.
fn compare(
    checks: &mut Checks,
    who: &str,
    e: &Execution,
    token: Address,
    value: U256,
    quote: Option<&NativeBuyQuote>,
) {
    check(
        checks,
        &format!("{who}.success"),
        e.status == "SUCCESS",
        format!(
            "{}; state: {}",
            e.error.as_deref().unwrap_or(&e.status),
            e.state_source
        ),
    );
    check(
        checks,
        &format!("{who}.token_equals_predicted"),
        e.token == Some(token),
        format!("{:?} predicted {}", e.token.map(addr), addr(token)),
    );
    let out = e.buy_return;
    check(
        checks,
        &format!("{who}.output_equals_balance_and_event"),
        out.is_some() && out == e.balance && out == e.bought_amount,
        format!(
            "return {} balance {} TokenBought.amount {}",
            show(out),
            show(e.balance),
            show(e.bought_amount)
        ),
    );
    check(
        checks,
        &format!("{who}.event_eth_equals_input"),
        e.bought_eth == Some(value),
        format!("TokenBought.eth {} input {value}", show(e.bought_eth)),
    );
    if let Some(q) = quote {
        check(
            checks,
            &format!("{who}.output_equals_quote_native_buy"),
            out == Some(q.output_amount),
            format!("{} vs {}", show(out), q.output_amount),
        );
        check(
            checks,
            &format!("{who}.fee_equals_protocol_fee"),
            e.bought_fee == Some(q.protocol_fee),
            format!(
                "TokenBought.fee {} vs protocol_fee {}",
                show(e.bought_fee),
                q.protocol_fee
            ),
        );
        check(
            checks,
            &format!("{who}.tax_equals_bonding_curve_tax"),
            e.tax_paid == Some(q.bonding_curve_tax),
            format!(
                "tax event {} vs bonding_curve_tax {}",
                show(e.tax_paid),
                q.bonding_curve_tax
            ),
        );
    }
}

struct Case {
    value: U256,
    purpose: &'static str,
    buy: Vec<u8>,
    quote: Result<NativeBuyQuote, String>,
    node: Execution,
    local: Execution,
    committed: Option<U256>,
    checks: Checks,
}

impl Case {
    fn passed(&self) -> bool {
        self.checks.iter().all(|(_, pass, _)| *pass)
    }
}

// ---- the fee-exemption entry ----------------------------------------------------------

/// The implementation's Sourcify storage layout as fetched.
struct LayoutObs {
    /// `EXACT_MATCH`, `MATCH`, `NO_MATCH` or `ERROR: ...`.
    status: String,
    response: Option<Vec<u8>>,
    layout: Option<SourcifyLayout>,
}

fn layout_of(cfg: &Config, implementation: Address) -> LayoutObs {
    let url = edge::sourcify_layout_url(&cfg.sourcify_base, edge::BSC_CHAIN_ID, implementation);
    let limits = Limits {
        compressed: false,
        ..cfg.limits
    };
    let error = |e: String, response| LayoutObs {
        status: format!("ERROR: {}", bounded_text(&e)),
        response,
        layout: None,
    };
    match edge::get(&url, limits, cfg.sourcify_max_bytes) {
        Err(e) => error(e, None),
        Ok((_, raw)) if raw.len() as u64 > cfg.sourcify_max_bytes => error(
            format!(
                "answer of {} bytes exceeds --sourcify-max-bytes {}",
                raw.len(),
                cfg.sourcify_max_bytes
            ),
            None,
        ),
        // Sourcify answers an unverified contract with 404 and a null match.
        Ok((200 | 404, raw)) => match edge::sourcify_layout(&raw, EXEMPTION_VARIABLE) {
            Ok(layout) => LayoutObs {
                status: layout.status.clone(),
                response: Some(raw),
                layout: Some(layout),
            },
            Err(e) => error(e, Some(raw)),
        },
        Ok((other, raw)) => error(format!("HTTP {other}"), Some(raw)),
    }
}

/// The witness of the entry the probe reads: the verified layout names
/// [`EXEMPTION_VARIABLE`] at the probe's slot, the buyer's live entry is
/// unset, and the node's BUY with the entry written pays another fee.
fn exemption_checks(
    layout: Option<&SourcifyLayout>,
    live_word: Option<B256>,
    base: &Execution,
    exempt: &Execution,
) -> Checks {
    let mut checks = Vec::new();
    check(
        &mut checks,
        "layout.implementation_verified",
        layout.is_some_and(|l| l.status != "NO_MATCH"),
        layout.map_or_else(
            || "no layout".to_string(),
            |l| {
                format!(
                    "{} runtime {:?} {:?}",
                    l.status, l.runtime_match, l.contract_name
                )
            },
        ),
    );
    let variable = layout.and_then(|l| l.variable.as_ref());
    check(
        &mut checks,
        "layout.variable_at_the_probe_slot",
        variable.is_some_and(|v| {
            v.slot == FEE_EXEMPTION_MAPPING_SLOT
                && v.offset == 0
                && v.type_label == EXEMPTION_TYPE_LABEL
                && v.encoding == "mapping"
        }),
        variable.map_or_else(
            || format!("{EXEMPTION_VARIABLE} absent"),
            |v| {
                format!(
                    "{} slot {} offset {} {} ({}) encoding {}; probe slot {FEE_EXEMPTION_MAPPING_SLOT}",
                    v.label, v.slot, v.offset, v.type_label, v.type_id, v.encoding
                )
            },
        ),
    );
    check(
        &mut checks,
        "live.buyer_entry_unset",
        live_word == Some(B256::ZERO),
        format!("{live_word:?}"),
    );
    for (name, e) in [
        ("node.buy_without_the_entry_succeeds", base),
        ("node.buy_with_the_entry_succeeds", exempt),
    ] {
        check(
            &mut checks,
            name,
            e.status == "SUCCESS" && e.bought_fee.is_some(),
            format!("{} fee {}", e.status, show(e.bought_fee)),
        );
    }
    check(
        &mut checks,
        "node.the_entry_changes_the_fee",
        matches!((base.bought_fee, exempt.bought_fee), (Some(a), Some(b)) if a != b),
        format!(
            "fee {} without, {} with the entry",
            show(base.bought_fee),
            show(exempt.bought_fee)
        ),
    );
    checks
}

// ---- the run ----------------------------------------------------------------------------

struct Outcome {
    dir: PathBuf,
    summary: String,
}

fn implementation(rpc: &Endpoint, portal: Address, pin: &Pin) -> Result<Address, String> {
    let w = rpc.storage(portal, eip1967_implementation_slot(), pin)?;
    if w[..12] != [0u8; 12] {
        return Err(format!("implementation slot holds {w}"));
    }
    Ok(Address::from_slice(&w[12..]))
}

fn run(cfg: &Config) -> Result<Outcome, String> {
    let deployment: FlapDeployment = bsc_mainnet_v5_27_0();
    let portal = deployment.portal;
    let g = g06a(&cfg.g06a_record)?;
    let simulator = c1_common::read(&cfg.simulator)?;
    let rpc = Endpoint::new(&cfg.rpc_url, cfg.limits, cfg.attempts, cfg.retry_pause);
    let chain_id = rpc.chain_id()?;
    edge::require_bsc(chain_id)?;
    let reference = rpc
        .block_number()?
        .checked_sub(cfg.confirmations)
        .ok_or("head below the confirmation depth")?;
    let pin = edge::pin_of(reference, &rpc.header(reference)?)?;
    eprintln!("g01_flap_econ: reference block {} {}", pin.number, pin.hash);

    // The live deployment must be exactly the v5.27.0 constants.
    let implementation = implementation(&rpc, portal, &pin)?;
    let implementation_code = rpc.code(implementation, &pin)?;
    let version = match rpc.call_at(portal, &versionCall {}.abi_encode(), &pin)? {
        CallOutcome::Returned(out) => {
            versionCall::abi_decode_returns(&out).map_err(|e| e.to_string())?
        }
        CallOutcome::Reverted(e) => return Err(format!("version() reverted: {e}")),
    };
    let observed = ObservedDeployment {
        chain_id,
        portal_version: &version,
        portal_runtime_sha256: sha256(&rpc.code(portal, &pin)?),
        portal_implementation: implementation,
        portal_implementation_runtime_sha256: sha256(&implementation_code),
        tax_v3_implementation_runtime_sha256: sha256(
            &rpc.code(deployment.tax_v3_implementation, &pin)?,
        ),
    };
    let validated = deployment.validate(&observed, 1);
    let validation = match &validated {
        Ok(_) => "VALID bsc_mainnet_v5_27_0()".to_string(),
        Err(e) => format!("INVALID: {e}"),
    };

    // The witnessed launch through the production encoder and decoder.
    let mut launch = Vec::new();
    let params = witnessed_launch(g.salt, g.launcher);
    encode_new_token_v6(&params, &mut launch);
    let (recognized, predicted, classified) =
        match recognize(portal, Some(portal), &launch, launch.len()) {
            Recognized::LaunchV6(p) => (
                format!("LaunchV6 tokenVersion {}", p.token_version),
                predicted_token(&deployment, &p),
                match &validated {
                    Ok(v) => match classify_launch(v, B256::ZERO, &p) {
                        Ok(c) => format!("Ok({:?})", c.profile()),
                        Err(e) => format!("Err({e:?})"),
                    },
                    Err(_) => "NOT_RUN: deployment invalid".into(),
                },
            ),
            other => (format!("{other:?}"), None, "NOT_RUN: not a LaunchV6".into()),
        };
    let token = predicted.unwrap_or(g.token);

    let estimate = rpc.estimate_gas(g.launcher, portal, U256::ZERO, &launch, &pin);
    let launch_gas = match &estimate {
        Ok(n) => n.saturating_mul(100 + cfg.launch_gas_margin) / 100,
        Err(_) => cfg.buy_gas.max(1) * 2,
    };
    let values: Vec<(U256, &'static str)> = if cfg.buy_values.is_empty() {
        vec![
            (
                U256::from(QUALIFIED_MAINNET_INPUT_WEI),
                "QUALIFIED_MAINNET_INPUT_WEI (admit_native_buy)",
            ),
            (g.buy_value, "the committed G0.6a record's amount"),
        ]
    } else {
        cfg.buy_values
            .iter()
            .map(|v| (*v, "--buy-value-wei"))
            .collect()
    };
    let r = Run {
        cfg,
        rpc: &rpc,
        pin,
        portal,
        g06a: &g,
        launch: &launch,
        launch_gas,
        token,
    };
    let mut cases = Vec::new();
    for (value, purpose) in values {
        let buy = encode_swap_exact_input_native(token, value, U256::from(1u8))
            .map_err(|e| format!("{e:?}"))?
            .to_vec();
        let quote = rust_quote(value, params.buy_tax);
        let node = r.node(value, &buy, &[]);
        let local = r.local(value);
        let committed = (value == g.buy_value).then_some(g.output);
        let mut checks = Vec::new();
        compare(
            &mut checks,
            "node",
            &node,
            token,
            value,
            quote.as_ref().ok(),
        );
        compare(
            &mut checks,
            "local_evm",
            &local,
            token,
            value,
            quote.as_ref().ok(),
        );
        check(
            &mut checks,
            "local_evm.launch_calldata_equals_production_encoding",
            local.launch_calldata_keccak == Some(keccak256(&launch)),
            format!(
                "{:?} vs {}",
                local.launch_calldata_keccak,
                keccak256(&launch)
            ),
        );
        check(
            &mut checks,
            "local_evm.buy_calldata_equals_production_encoding",
            local.buy_calldata_keccak == Some(keccak256(&buy)),
            format!("{:?} vs {}", local.buy_calldata_keccak, keccak256(&buy)),
        );
        if let Some(c) = committed {
            check(
                &mut checks,
                "node.output_equals_committed_g06a_output",
                node.buy_return == Some(c),
                format!("{} vs committed {c}", show(node.buy_return)),
            );
        }
        eprintln!(
            "g01_flap_econ: BUY {value}: node {} {}, local {} {}, quote {}",
            node.status,
            show(node.buy_return),
            local.status,
            show(local.buy_return),
            quote
                .as_ref()
                .map_or_else(|e| e.clone(), |q| q.output_amount.to_string())
        );
        cases.push(Case {
            value,
            purpose,
            buy,
            quote,
            node,
            local,
            committed,
            checks,
        });
    }
    // The fee-exemption entry the probe reads, witnessed on this
    // implementation (the qualified BUY, the buyer's entry written).
    // The layout of the implementation the production deployment names
    // (what the probe's slot is claimed for); the BUY runs on live state.
    let layout = layout_of(cfg, deployment.portal_implementation);
    let exemption_key = B256::from(fee_exemption_key(g.buyer));
    let live_word = rpc.storage(portal, exemption_key, &pin);
    let qualified_value = U256::from(QUALIFIED_MAINNET_INPUT_WEI);
    let exempt = cases.iter().find(|c| c.value == qualified_value).map(|c| {
        let node = r.node(
            qualified_value,
            &c.buy,
            &[(portal, exemption_key, B256::with_last_byte(1))],
        );
        Case {
            value: qualified_value,
            purpose: "QUALIFIED_MAINNET_INPUT_WEI with the buyer's _feeExemptedTraders entry overridden to true (node only)",
            buy: c.buy.clone(),
            quote: Err("NOT_APPLICABLE: quote_native_buy refuses an exempt caller".into()),
            node,
            local: Execution {
                status: "NOT_RUN: the entry witness is the node's".into(),
                ..Execution::default()
            },
            committed: None,
            checks: Vec::new(),
        }
    });
    let exemption = exemption_checks(
        layout.layout.as_ref(),
        live_word.as_ref().ok().copied(),
        &cases
            .iter()
            .find(|c| c.value == qualified_value)
            .map_or_else(Execution::default, |c| c.node.clone()),
        &exempt
            .as_ref()
            .map_or_else(Execution::default, |c| c.node.clone()),
    );
    let exemption_ok = exemption.iter().all(|(_, pass, _)| *pass);
    eprintln!(
        "g01_flap_econ: exemption entry: layout {}, fee {} without, {} with the entry: {}",
        layout.status,
        show(
            cases
                .iter()
                .find(|c| c.value == qualified_value)
                .and_then(|c| c.node.bought_fee)
        ),
        show(exempt.as_ref().and_then(|c| c.node.bought_fee)),
        if exemption_ok { "PASS" } else { "FAIL" }
    );
    let pin_rechecked = rpc.header(reference)?.hash == pin.hash;

    let qualified = cases.iter().find(|c| c.value == qualified_value);
    let token_ok = predicted == Some(g.token);
    let (status, detail) = match (validated.is_ok(), token_ok, pin_rechecked, qualified) {
        (false, _, _, _) => ("NOT_RECONFIRMED", validation.clone()),
        (_, _, false, _) => (
            "NOT_RECONFIRMED",
            "the reference block changed during the run".into(),
        ),
        (_, _, _, None) => (
            "NOT_RECONFIRMED",
            "the qualified input was not executed".into(),
        ),
        (true, ok, true, Some(c))
            if ok && c.passed() && cases.iter().all(Case::passed) && exemption_ok =>
        {
            (
                "RECONFIRMED",
                format!(
                    "on {version} ({}): token {} = predicted = committed; node and local EVM output {} = quote_native_buy; TokenBought.fee = protocol fee, tax event = bonding-curve tax; {EXEMPTION_VARIABLE} at slot {FEE_EXEMPTION_MAPPING_SLOT} (Sourcify layout) changes the node BUY's fee",
                    addr(implementation),
                    addr(token),
                    show(c.node.buy_return)
                ),
            )
        }
        (true, _, true, Some(c)) => {
            let failed: Vec<String> = cases
                .iter()
                .flat_map(|c| {
                    c.checks
                        .iter()
                        .filter(|(_, p, _)| !p)
                        .map(move |(n, _, d)| format!("{} {n}: {d}", c.value))
                })
                .chain(
                    exemption
                        .iter()
                        .filter(|(_, p, _)| !p)
                        .map(|(n, _, d)| format!("exemption {n}: {d}")),
                )
                .collect();
            let not_run = c.node.status != "SUCCESS" && c.local.status != "SUCCESS";
            (
                if not_run {
                    "NOT_RECONFIRMED"
                } else {
                    "MISMATCH"
                },
                format!(
                    "token predicted = committed {token_ok}; failed: {}",
                    failed.join("; ")
                ),
            )
        }
    };

    let observed_ms = darkbot_bsc_record::observed_now()?;
    let mut d = Draft::new(&fe::V2);
    let b = |v: B256| Value::B256(v.0);
    let a = |v: Address| Value::Address(v.0.0);
    d.set(fe::CHAIN_ID, chain_id)
        .set(fe::PORTAL, a(portal))
        .set(fe::PROVENANCE_TOOL, TOOL)
        .set(fe::PROVENANCE_OBSERVED_UNIX_MS, observed_ms)
        .set(fe::REFERENCE_BLOCK, pin.number)
        .set(fe::REFERENCE_BLOCK_HASH, b(pin.hash))
        .set(fe::PIN_RECHECKED, pin_rechecked)
        .set(fe::ENDPOINT_HOST, rpc.host())
        .set(fe::LOCAL_EVM_ENDPOINT_HOST, edge::host(&cfg.local_evm_rpc))
        .set(fe::TOOL_SHA256, Value::B256(c1_common::own_sha256()?))
        .set_opt(fe::GIT_HEAD, c1_common::git_head(Path::new(".")))
        .set(fe::SIMULATOR_PATH, cfg.simulator.display().to_string())
        .set(fe::SIMULATOR_SHA256, Value::B256(sha256(&simulator)))
        .set(fe::PORTAL_VERSION, version.as_str())
        .set(fe::IMPLEMENTATION, a(implementation))
        .set(
            fe::IMPLEMENTATION_RUNTIME_SHA256,
            Value::B256(sha256(&implementation_code)),
        )
        .set(fe::DEPLOYMENT_VALIDATION, validation.as_str())
        .set(fe::G06A_RECORD_PATH, g.path.as_str())
        .set(fe::G06A_RECORD_SHA256, Value::B256(g.sha256))
        .set(fe::G06A_RECORD_BLOCK, g.block)
        .set(fe::SALT, b(g.salt))
        .set(fe::LAUNCHER, a(g.launcher))
        .set(fe::BUYER, a(g.buyer))
        .set_opt(fe::PREDICTED_TOKEN, predicted.map(a))
        .set(fe::COMMITTED_TOKEN, a(g.token))
        .set(fe::LAUNCH_CALLDATA, Value::Bytes(launch.clone()))
        .set(fe::LAUNCH_CALLDATA_KECCAK, b(keccak256(&launch)))
        .set(fe::RECOGNIZED, recognized.as_str())
        .set(fe::CLASSIFIED, classified.as_str())
        .set_opt(
            fe::NODE_LAUNCH_GAS_ESTIMATE,
            estimate.as_ref().ok().copied(),
        )
        .set(fe::LAUNCH_GAS_LIMIT, launch_gas)
        .set(fe::STATUS, status)
        .set(fe::DETAIL, bounded_text(&detail));
    for c in &cases {
        d.push(fe::CASES, case_draft(c).into_nested());
    }
    d.set(fe::SOURCIFY_HOST, edge::host(&cfg.sourcify_base))
        .set(fe::EXEMPTION_SLOT, FEE_EXEMPTION_MAPPING_SLOT)
        .set(fe::EXEMPTION_KEY, b(exemption_key))
        .set_opt(
            fe::LIVE_EXEMPTION_WORD,
            live_word.as_ref().ok().map(|v| b(*v)),
        )
        .set(
            fe::LAYOUT_IMPLEMENTATION,
            a(deployment.portal_implementation),
        )
        .set(fe::LAYOUT_STATUS, layout.status.as_str())
        .set_opt(
            fe::LAYOUT_RESPONSE,
            layout.response.clone().map(Value::Bytes),
        )
        .set_opt(
            fe::LAYOUT_RESPONSE_SHA256,
            layout.response.as_ref().map(|r| Value::B256(sha256(r))),
        );
    if let Some(v) = layout.layout.as_ref().and_then(|l| l.variable.as_ref()) {
        d.set(fe::LAYOUT_LABEL, v.label.as_str())
            .set(fe::LAYOUT_SLOT, v.slot)
            .set(fe::LAYOUT_OFFSET, v.offset)
            .set(fe::LAYOUT_TYPE, format!("{} ({})", v.type_label, v.type_id))
            .set(fe::LAYOUT_ENCODING, v.encoding.as_str());
    }
    if let Some(c) = &exempt {
        d.set(fe::EXEMPT_CASE, case_draft(c).into_nested());
    }
    for (name, pass, detail) in &exemption {
        let mut x = Draft::new(&shared::check::SCHEMA);
        x.set(shared::check::NAME, name.as_str())
            .set(shared::check::PASS, *pass)
            .set(shared::check::DETAIL, bounded_text(detail));
        d.push(fe::EXEMPTION_CHECKS, x.into_nested());
    }
    for n in [
        "READ_ONLY: no key read; nothing signed, sent or persisted; caller balances exist only in the simulations",
        "SIMULATED_STATE: one block on top of the pinned block; not consensus or builder inclusion evidence",
        "NO_ADMISSION_CHANGE: evidence only; the production gate is changed only by the reviewed code",
        "ECONOMICS_SCOPE: the fresh native profile directly after its launch only (reserve 0, supply 0)",
        "ADMIT_NOT_CALLED: the admission's quote is quote_native_buy with exactly these inputs (WITNESSED_MAINNET_TERMS, reserve 0, supply 0, the launch's buy tax 100)",
        "EXEMPTION_SCOPE: which storage entry the BUY path reads as the caller's exemption, by the Sourcify layout and one node BUY with the entry overridden; no economics of an exempt caller (quote_native_buy refuses one)",
    ] {
        d.push(fe::NON_CLAIMS, n);
    }
    let bytes = d.encode().map_err(|e| e.to_string())?;
    decode(&fe::V2, &bytes).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cfg.out_root)
        .map_err(|e| format!("{}: {e}", cfg.out_root.display()))?;
    let dir = cfg
        .out_root
        .join(format!("flap_econ_v5_27_{}", utc_stamp(observed_ms)));
    std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join("flap_econ.dbrec");
    c1_common::write_new(&path, &bytes)?;
    let mut summary = format!(
        "Flap economics at block {} {} on {version} ({}): {status}\n  {detail}\n  deployment {validation}; recognize {recognized}; classify_launch {classified}; launch gas estimate {:?}, limit {launch_gas}\n",
        pin.number,
        pin.hash,
        addr(implementation),
        estimate
    );
    for c in &cases {
        summary.push_str(&format!(
            "  BUY {} ({}): quote {}; node {} {} fee {} tax {}; local {} {} fee {} tax {}; checks {}\n",
            c.value,
            c.purpose,
            c.quote.as_ref().map_or_else(|e| e.clone(), |q| format!("{} (fee {}, tax {})", q.output_amount, q.protocol_fee, q.bonding_curve_tax)),
            c.node.status,
            show(c.node.buy_return),
            show(c.node.bought_fee),
            show(c.node.tax_paid),
            c.local.status,
            show(c.local.buy_return),
            show(c.local.bought_fee),
            show(c.local.tax_paid),
            if c.passed() { "PASS" } else { "FAIL" }
        ));
    }
    summary.push_str(&format!(
        "{} sha256 {} ({} bytes)",
        path.display(),
        B256::from(sha256(&bytes)),
        bytes.len()
    ));
    Ok(Outcome { dir, summary })
}

/// One case as its nested record.
fn case_draft(c: &Case) -> Draft {
    let b = |v: B256| Value::B256(v.0);
    let a = |v: Address| Value::Address(v.0.0);
    let w = |v: U256| Value::U256(v.to_be_bytes());
    use fe::case as t;
    let mut e = Draft::new(&t::SCHEMA);
    let q = c.quote.as_ref().ok();
    e.set(t::BUY_VALUE_WEI, w(c.value))
        .set(t::PURPOSE, c.purpose)
        .set(t::BUY_CALLDATA, Value::Bytes(c.buy.clone()))
        .set_opt(t::RUST_EXPECTED_OUTPUT, q.map(|q| w(q.output_amount)))
        .set_opt(t::RUST_PROTOCOL_FEE, q.map(|q| w(q.protocol_fee)))
        .set_opt(t::RUST_BONDING_CURVE_TAX, q.map(|q| w(q.bonding_curve_tax)))
        .set_opt(t::RUST_COMBINED_FEE, q.map(|q| w(q.combined_fee)))
        .set_opt(t::RUST_NET_RESERVE_INPUT, q.map(|q| w(q.net_reserve_input)))
        .set_opt(t::NODE_LAUNCH_STATUS, c.node.launch_status)
        .set_opt(t::NODE_TOKEN, c.node.token.map(a))
        .set_opt(t::NODE_LAUNCH_GAS, c.node.launch_gas)
        .set_opt(t::NODE_BUY_STATUS, c.node.buy_status)
        .set_opt(t::NODE_BUY_RETURN, c.node.buy_return.map(w))
        .set_opt(t::NODE_BUYER_BALANCE, c.node.balance.map(w))
        .set_opt(t::NODE_BOUGHT_AMOUNT, c.node.bought_amount.map(w))
        .set_opt(t::NODE_BOUGHT_ETH, c.node.bought_eth.map(w))
        .set_opt(t::NODE_BOUGHT_FEE, c.node.bought_fee.map(w))
        .set_opt(t::NODE_BOUGHT_POST_PRICE, c.node.bought_post_price.map(w))
        .set_opt(t::NODE_TAX_PAID, c.node.tax_paid.map(w))
        .set_opt(t::NODE_BUY_GAS, c.node.buy_gas)
        .set_opt(t::NODE_ERROR, c.node.error.clone())
        .set(t::LOCAL_STATUS, bounded_text(&c.local.status))
        .set_opt(t::LOCAL_TOKEN, c.local.token.map(a))
        .set_opt(t::LOCAL_LAUNCH_GAS, c.local.launch_gas)
        .set_opt(t::LOCAL_BUY_RETURN, c.local.buy_return.map(w))
        .set_opt(t::LOCAL_BUYER_BALANCE, c.local.balance.map(w))
        .set_opt(t::LOCAL_BOUGHT_AMOUNT, c.local.bought_amount.map(w))
        .set_opt(t::LOCAL_BOUGHT_ETH, c.local.bought_eth.map(w))
        .set_opt(t::LOCAL_BOUGHT_FEE, c.local.bought_fee.map(w))
        .set_opt(t::LOCAL_TAX_PAID, c.local.tax_paid.map(w))
        .set_opt(t::LOCAL_BUY_GAS, c.local.buy_gas)
        .set_opt(
            t::LOCAL_LAUNCH_CALLDATA_KECCAK,
            c.local.launch_calldata_keccak.map(b),
        )
        .set_opt(
            t::LOCAL_BUY_CALLDATA_KECCAK,
            c.local.buy_calldata_keccak.map(b),
        )
        .set_opt(t::COMMITTED_OUTPUT, c.committed.map(w));
    for (name, pass, detail) in &c.checks {
        let mut x = Draft::new(&shared::check::SCHEMA);
        x.set(shared::check::NAME, name.as_str())
            .set(shared::check::PASS, *pass)
            .set(shared::check::DETAIL, bounded_text(detail));
        e.push(t::CHECKS, x.into_nested());
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
    }

    /// The committed G0.6a record gives the witnessed launch; the production
    /// encoder, decoder and derivation agree with it offline, and the
    /// admission's arithmetic reproduces the committed output.
    #[test]
    fn the_committed_g06a_launch_and_quote_reproduce_offline() {
        let g = g06a(&repo_root().join(inputs::G06A_RECORD)).unwrap();
        assert_eq!(addr(g.token), "0xe51c4deb1038afcd95ef12d4a649cdbcf6fa7777");
        let deployment = bsc_mainnet_v5_27_0();
        let mut launch = Vec::new();
        let params = witnessed_launch(g.salt, g.launcher);
        encode_new_token_v6(&params, &mut launch);
        let Recognized::LaunchV6(p) = recognize(
            deployment.portal,
            Some(deployment.portal),
            &launch,
            launch.len(),
        ) else {
            panic!("not recognized");
        };
        assert_eq!(predicted_token(&deployment, &p), Some(g.token));
        // The committed run's amount and output (G0.6a, v5.26.0 era).
        assert_eq!(
            rust_quote(g.buy_value, params.buy_tax)
                .unwrap()
                .output_amount,
            g.output
        );
        // The admission's qualified input: the Rust vector of the legacy
        // v5.24.0 crosscheck.
        let q = rust_quote(U256::from(QUALIFIED_MAINNET_INPUT_WEI), params.buy_tax).unwrap();
        assert_eq!(
            q.output_amount,
            U256::from(176_664_965_031_639_901_123_273u128)
        );
        assert_eq!(q.protocol_fee, U256::from(10_000_000_000_000u64));
        assert_eq!(q.bonding_curve_tax, U256::from(10_000_000_000_000u64));
    }

    /// The simulator's lines decode into an execution; anything else stays
    /// absent, never guessed.
    #[test]
    fn local_evm_lines_decode_or_stay_absent() {
        let e = local_execution(
            "LAUNCH_CALLDATA_KECCAK=0x674b93f94a75ba59e90a4757d4884b5bf9441300a3190e4043467800584e6378\nTOKEN=0xe51c4deb1038afcd95ef12d4a649cdbcf6fa7777\nBUY_RETURN=176664965031639901123273\nBOUGHT_FEE=10000000000000\nBUYER_BALANCE=x\nSTATUS=SUCCESS\n",
        );
        assert_eq!(e.status, "SUCCESS");
        assert_eq!(
            e.buy_return,
            Some(U256::from(176_664_965_031_639_901_123_273u128))
        );
        assert_eq!(e.balance, None);
        assert!(e.token.is_some() && e.launch_calldata_keccak.is_some());
        assert_eq!(local_execution("").status, "NO_STATUS");
    }

    /// A node result decodes the launch's token and the BUY's events; a
    /// revert is never a success.
    #[test]
    fn node_results_decode_and_a_revert_is_not_success() {
        let portal = bsc_mainnet_v5_27_0().portal;
        let token = Address::repeat_byte(0x77);
        let mut ret = vec![0u8; 12];
        ret.extend_from_slice(token.as_slice());
        let mut bought = vec![0u8; 7 * 32];
        bought[3 * 32 + 31] = 9;
        bought[4 * 32 + 31] = 5;
        bought[5 * 32 + 31] = 1;
        let ok = |status| SimResult {
            status,
            return_data: ret.clone(),
            gas_used: 1,
            logs: vec![common::edge::SimLog {
                address: portal,
                topics: vec![token_bought_topic()],
                data: bought.clone(),
            }],
            error: None,
        };
        let e = node_execution(&[ok(1), ok(1), ok(1)], portal);
        assert_eq!(
            (e.status.as_str(), e.token, e.bought_amount),
            ("SUCCESS", Some(token), Some(U256::from(9u8)))
        );
        let reverted = node_execution(&[ok(1), ok(0), ok(1)], portal);
        assert_eq!(reverted.status, "REVERTED");
        assert_eq!(node_execution(&[ok(1)], portal).status, "INCOMPLETE");
    }

    /// The exemption witness passes only when the verified layout names the
    /// probe's slot, the live entry is unset and writing it changes the fee.
    #[test]
    fn the_exemption_witness_needs_the_layout_slot_and_a_changed_fee() {
        let variable = common::edge::LayoutVariable {
            label: EXEMPTION_VARIABLE.into(),
            slot: FEE_EXEMPTION_MAPPING_SLOT,
            offset: 0,
            type_id: "t_mapping(t_address,t_bool)".into(),
            type_label: EXEMPTION_TYPE_LABEL.into(),
            encoding: "mapping".into(),
        };
        let layout = SourcifyLayout {
            status: "MATCH".into(),
            runtime_match: Some("match".into()),
            contract_name: Some("Portal".into()),
            variable: Some(variable.clone()),
        };
        let paid = |fee: u64| Execution {
            status: "SUCCESS".into(),
            bought_fee: Some(U256::from(fee)),
            ..Execution::default()
        };
        let passed = |c: &Checks| c.iter().all(|(_, p, _)| *p);
        let unset = Some(B256::ZERO);
        assert!(passed(&exemption_checks(
            Some(&layout),
            unset,
            &paid(10),
            &paid(0)
        )));
        // The same fee with the entry written: the slot is not the one read.
        assert!(!passed(&exemption_checks(
            Some(&layout),
            unset,
            &paid(10),
            &paid(10)
        )));
        // The layout places the variable elsewhere.
        let moved = SourcifyLayout {
            variable: Some(common::edge::LayoutVariable {
                slot: FEE_EXEMPTION_MAPPING_SLOT + 1,
                ..variable
            }),
            ..layout.clone()
        };
        assert!(!passed(&exemption_checks(
            Some(&moved),
            unset,
            &paid(10),
            &paid(0)
        )));
        // Unverified, no layout, a live exempt buyer, a failed BUY.
        let unverified = SourcifyLayout {
            status: "NO_MATCH".into(),
            ..layout.clone()
        };
        assert!(!passed(&exemption_checks(
            Some(&unverified),
            unset,
            &paid(10),
            &paid(0)
        )));
        assert!(!passed(&exemption_checks(None, unset, &paid(10), &paid(0))));
        assert!(!passed(&exemption_checks(
            Some(&layout),
            Some(B256::with_last_byte(1)),
            &paid(10),
            &paid(0)
        )));
        assert!(!passed(&exemption_checks(
            Some(&layout),
            unset,
            &paid(10),
            &Execution::default()
        )));
    }

    /// The real reconfirmation against the configured free endpoint.
    #[test]
    #[ignore = "live read-only BSC RPC and the local EVM toolset"]
    fn live_reconfirmation_against_the_free_rpc() {
        let out = std::env::temp_dir().join(format!("g01-flap-econ-live-{}", std::process::id()));
        let root = repo_root();
        let args = Args::parse([
            "--rpc-config".to_string(),
            root.join(defaults::RPC_CONFIG).display().to_string(),
            "--g06a-record".into(),
            root.join(inputs::G06A_RECORD).display().to_string(),
            "--simulator".into(),
            root.join(inputs::SIMULATOR).display().to_string(),
            "--out-root".into(),
            out.display().to_string(),
        ]);
        let cfg = Config::parse(&args).unwrap();
        let outcome = run(&cfg).unwrap();
        let bytes = std::fs::read(outcome.dir.join("flap_econ.dbrec")).unwrap();
        let r = decode(&fe::V2, &bytes).unwrap();
        assert_eq!(r.u64(fe::CHAIN_ID).unwrap(), 56);
        std::fs::remove_dir_all(out).unwrap();
    }
}

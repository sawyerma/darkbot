//! Read-only chain access: JSON-RPC reads, the current read-only preflight
//! compared against the frozen DRY baseline, and the head poller whose read of
//! the next block immediately before the release sets the bundle expiry.
//!
//! JSON exists here only on the RPC wire: requests are fixed templates of
//! hex fields, and every response is decoded in this module into typed
//! values (`rpc_quantity`, `rpc_data`, `rpc_receipt`, `parse_head`, the
//! preflight record); no JSON value leaves it.

use super::*;
use std::str::FromStr;

/// BSC block interval (BEP-619).
pub(crate) const BLOCK_INTERVAL_MS: u64 = 450;
/// maxBlockNumber = send head + 2: the bundle is valid for send head + 1 ..= + 2.
pub(crate) const MAX_BLOCK_OFFSET: u64 = 2;
/// The send head may be at most one interval old, so head+2 still has at least
/// one full interval left when the bundle is released.
pub(crate) const MAX_HEAD_AGE_MS: i64 = BLOCK_INTERVAL_MS as i64;
/// Blocks are broadcast at or after their timestamp. A head that looks younger
/// than this below zero means the local clock is behind, which would understate
/// every head age.
pub(crate) const MAX_LOCAL_CLOCK_LAG_MS: i64 = 150;
/// The release waits at most this long for the next block.
pub(crate) const HEAD_TRANSITION_TIMEOUT: Duration = Duration::from_secs(3);
const HEAD_POLL_MIN_PERIOD: Duration = Duration::from_millis(50);
/// A transient RPC error (429, 5xx, a reset) is polled through up to this many
/// times in a row before the wait fails.
const MAX_CONSECUTIVE_POLL_ERRORS: u32 = 3;
const HEAD_READ_TIMEOUT: Duration = Duration::from_secs(2);

/// Spawns a read-only helper with a cleared environment: the child sees only
/// PATH and what the caller passes explicitly, never the signing keys, the
/// builder token or any other variable of this process.
pub(crate) fn child_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.env_clear();
    if let Some(path) = env::var_os("PATH") {
        cmd.env("PATH", path);
    }
    cmd
}

/// The JSON-RPC wire boundary: posts one request (curl: connect 2 s, whole
/// call 5 s) and returns its `result`. Never panics and never hangs: it also
/// serves the receipt polling after the release. The wire value never
/// leaves this module; the typed readers below decode it.
fn rpc(method: &str, params: &str) -> Result<serde_json::Value, String> {
    let rpc = env::var("BSC_MAINNET_RPC_HTTPS")
        .map_err(|_| "BSC_MAINNET_RPC_HTTPS is required".to_string())?;
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":{},"params":{params}}}"#,
        wire_string(method)
    );
    // -q first: no .curlrc, whatever the environment.
    let out = child_command("curl")
        .args([
            "-q",
            "--connect-timeout",
            "2",
            "--max-time",
            "5",
            "-sS",
            &rpc,
            "-H",
            "Content-Type: application/json",
            "--data",
            &body,
        ])
        .output()
        .map_err(|e| format!("execute curl: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "curl {method}: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("invalid JSON-RPC response for {method}: {e}"))?;
    if let Some(err) = v.get("error") {
        return Err(format!("JSON-RPC {method}: {err}"));
    }
    v.get("result")
        .cloned()
        .ok_or_else(|| format!("JSON-RPC {method}: missing result"))
}

fn wire_quantity(v: &serde_json::Value, what: &str) -> Result<U256, String> {
    v.as_str()
        .and_then(|s| s.strip_prefix("0x"))
        .and_then(|h| U256::from_str_radix(h, 16).ok())
        .ok_or_else(|| format!("{what} is not a hex quantity"))
}

fn wire_data(v: &serde_json::Value, what: &str) -> Result<Vec<u8>, String> {
    v.as_str()
        .and_then(|s| s.strip_prefix("0x"))
        .and_then(|h| hex::decode(h).ok())
        .ok_or_else(|| format!("{what} is not hex data"))
}

fn small(q: U256, what: &str) -> Result<u64, String> {
    u64::try_from(q).map_err(|_| format!("{what} exceeds u64"))
}

/// A hex quantity result. Never panics.
pub(crate) fn rpc_quantity(method: &str, params: &str) -> Result<U256, String> {
    rpc(method, params).and_then(|v| wire_quantity(&v, method))
}

/// A hex quantity result that must fit a u64. Never panics.
pub(crate) fn rpc_u64(method: &str, params: &str) -> Result<u64, String> {
    rpc_quantity(method, params).and_then(|q| small(q, method))
}

/// A hex data result. Never panics.
pub(crate) fn rpc_data(method: &str, params: &str) -> Result<Vec<u8>, String> {
    rpc(method, params).and_then(|v| wire_data(&v, method))
}

/// What G0.7 records of an observed receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReceiptSeen {
    pub(crate) status: u64,
    pub(crate) block_number: u64,
    pub(crate) transaction_index: u64,
}

/// The receipt of `hash`, `None` while there is none. Never panics.
pub(crate) fn rpc_receipt(hash: &str) -> Result<Option<ReceiptSeen>, String> {
    let v = rpc(
        "eth_getTransactionReceipt",
        &format!("[{}]", wire_string(hash)),
    )?;
    if v.is_null() {
        return Ok(None);
    }
    let field = |name: &str| {
        v.get(name)
            .ok_or_else(|| format!("receipt lacks {name}"))
            .and_then(|x| wire_quantity(x, name))
            .and_then(|q| small(q, name))
    };
    Ok(Some(ReceiptSeen {
        status: field("status")?,
        block_number: field("blockNumber")?,
        transaction_index: field("transactionIndex")?,
    }))
}

/// The code at `address` (latest). Never panics.
pub(crate) fn rpc_code(address: Address) -> Result<Vec<u8>, String> {
    rpc_data(
        "eth_getCode",
        &format!(r#"[{},"latest"]"#, wire_string(&address.to_string())),
    )
}

/// The gas estimate of a call at latest. Never panics.
pub(crate) fn rpc_estimate_gas(
    from: Address,
    to: Address,
    value: U256,
    data: &[u8],
) -> Result<u64, String> {
    rpc_u64(
        "eth_estimateGas",
        &format!(
            r#"[{{"from":{},"to":{},"value":"0x{value:x}","data":"0x{}"}}]"#,
            wire_string(&from.to_string()),
            wire_string(&to.to_string()),
            hex::encode(data)
        ),
    )
}

/// The latest block number. Never panics.
pub(crate) fn rpc_block_number() -> Result<u64, String> {
    rpc_u64("eth_blockNumber", "[]")
}

/// The balance of `who` at latest. Never panics.
pub(crate) fn rpc_balance(who: Address) -> Result<U256, String> {
    rpc_quantity(
        "eth_getBalance",
        &format!(r#"[{},"latest"]"#, wire_string(&who.to_string())),
    )
}

/// The pending nonce of `who`. Never panics.
pub(crate) fn rpc_pending_nonce(who: Address) -> Result<u64, String> {
    rpc_u64(
        "eth_getTransactionCount",
        &format!(r#"[{},"pending"]"#, wire_string(&who.to_string())),
    )
}

/// One log of an `eth_getLogs` result, as a scan records it (used by the
/// requalification preflight writer, not by the attempt itself).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogSeen {
    pub(crate) block: u64,
    pub(crate) transaction_hash: [u8; 32],
    pub(crate) log_index: u64,
    pub(crate) topics: Vec<[u8; 32]>,
}

#[allow(dead_code)]
fn wire_word(v: &serde_json::Value, what: &str) -> Result<[u8; 32], String> {
    wire_data(v, what)
        .and_then(|b| <[u8; 32]>::try_from(b).map_err(|_| format!("{what} is not 32 bytes")))
}

/// The logs of `address` with first topic `topic0` in `from..=to`. Never
/// panics.
#[allow(dead_code)]
pub(crate) fn rpc_logs(
    address: Address,
    topic0: [u8; 32],
    from: u64,
    to: u64,
) -> Result<Vec<LogSeen>, String> {
    let v = rpc(
        "eth_getLogs",
        &format!(
            r#"[{{"address":{},"fromBlock":"0x{from:x}","toBlock":"0x{to:x}","topics":["0x{}"]}}]"#,
            wire_string(&address.to_string()),
            hex::encode(topic0)
        ),
    )?;
    let logs = v.as_array().ok_or("eth_getLogs result is not a list")?;
    logs.iter()
        .map(|log| {
            let field = |name: &str| log.get(name).ok_or_else(|| format!("log lacks {name}"));
            let quantity = |name: &str| {
                field(name)
                    .and_then(|x| wire_quantity(x, name))
                    .and_then(|x| small(x, name))
            };
            Ok(LogSeen {
                block: quantity("blockNumber")?,
                transaction_hash: wire_word(field("transactionHash")?, "transactionHash")?,
                log_index: quantity("logIndex")?,
                topics: field("topics")?
                    .as_array()
                    .ok_or("log topics are not a list")?
                    .iter()
                    .map(|t| wire_word(t, "topic"))
                    .collect::<Result<_, _>>()?,
            })
        })
        .collect()
}

/// Number and hash of the latest block.
fn latest_block() -> Result<(u64, [u8; 32]), String> {
    let block = rpc("eth_getBlockByNumber", r#"["latest",false]"#)?;
    let number = block
        .get("number")
        .ok_or("latest block lacks number")
        .map_err(str::to_string)
        .and_then(|n| wire_quantity(n, "latest block number"))
        .and_then(|n| small(n, "latest block number"))?;
    let hash = block
        .get("hash")
        .ok_or("latest block lacks hash")
        .map_err(str::to_string)
        .and_then(|h| wire_data(h, "latest block hash"))
        .and_then(|h| {
            <[u8; 32]>::try_from(h).map_err(|_| "latest block hash is not 32 bytes".into())
        })?;
    Ok((number, hash))
}

const TAX_V3_IMPL_EXPECTED: &str = "0x024f18294970b5c76c0691b87f138a0317156422";
// keccak256 selectors, independently frozen from the canonical signatures.
const GET_QUOTE_CONFIG_SELECTOR: &str = "0x26ef20d5"; // getQuoteTokenConfiguration(address)
const GET_FEE_RATE_SELECTOR: &str = "0x84e5eed0"; // getFeeRate()
const TAX_ON_BC_SELECTOR: &str = "0x1d31e153"; // enableTaxOnBondingCurve()
const VERSION_SELECTOR: &str = "0x54fd4d50"; // version()
const EIP1967_IMPLEMENTATION_SLOT: &str =
    "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc";
// The preflight's own ceilings (the frozen v2 values of the retired node
// preflight); the guard enforces the approved contract separately.
const MAX_LAUNCH_GAS_LIMIT: u64 = 6_603_270;
const MAX_BUY_GAS_LIMIT: u64 = 3_000_000;
const MAX_SIGNED_GAS_PRICE_WEI: u64 = 1_000_000_000;
const MAX_BUY_VALUE_WEI: u64 = 1_000_000_000_000_000;
const MAX_LAUNCH_VALUE_WEI: u128 = 0;
const MAX_ATTEMPT_RESERVE_WEI: u128 = 10_603_270_000_000_000;

/// The UTF-8 string an ABI-encoded `string` return value holds.
pub(crate) fn decode_abi_string(raw: &[u8]) -> Result<String, String> {
    let word = |at: usize| -> Result<usize, String> {
        raw.get(at..at + 32)
            .map(|w| U256::from_be_slice(w))
            .and_then(|w| usize::try_from(w).ok())
            .ok_or_else(|| "short or oversized ABI string".to_string())
    };
    let offset = word(0)?;
    let len = word(offset)?;
    let start = offset.checked_add(32).ok_or("bad ABI string offset")?;
    let end = start.checked_add(len).ok_or("bad ABI string length")?;
    let bytes = raw.get(start..end).ok_or("truncated ABI string")?;
    String::from_utf8(bytes.to_vec()).map_err(|_| "ABI string is not UTF-8".into())
}

/// The first `count` 32-byte words of an ABI return value.
pub(crate) fn abi_words(raw: &[u8], count: usize) -> Result<Vec<U256>, String> {
    if raw.len() < count * 32 {
        return Err(format!("ABI result shorter than {count} words"));
    }
    Ok(raw
        .chunks(32)
        .take(count)
        .map(U256::from_be_slice)
        .collect())
}

fn eth_call(to: &str, data: &str) -> Result<Vec<u8>, String> {
    rpc_data(
        "eth_call",
        &format!(
            r#"[{{"to":{},"data":{}}},"latest"]"#,
            wire_string(to),
            wire_string(data)
        ),
    )
}

fn code_at(address: &str, what: &str) -> Result<Vec<u8>, String> {
    let code = rpc_data(
        "eth_getCode",
        &format!(r#"[{},"latest"]"#, wire_string(address)),
    )?;
    if code.is_empty() {
        return Err(format!("{what} has no code at latest"));
    }
    Ok(code)
}

/// The read-only Mainnet preflight (the port of the retired node preflight
/// `bsc/tools/g06a_preflight_readonly.cjs`, field for field): deployment,
/// launch economics and the funding of the exact signed parameters, as a
/// preflight record comparable with the imported DRY baseline.
pub(crate) fn run_current_preflight(
    launcher: Address,
    buyer: Address,
    gas_price: u64,
    launch_gas_limit: u64,
    buy_gas_limit: u64,
    launch_value: u128,
    buy_value: u64,
) -> Result<Record, String> {
    use darkbot_bsc_record::Value as V;
    use schema::g06a::preflight as f;
    if gas_price > MAX_SIGNED_GAS_PRICE_WEI
        || launch_gas_limit > MAX_LAUNCH_GAS_LIMIT
        || buy_gas_limit > MAX_BUY_GAS_LIMIT
        || launch_value > MAX_LAUNCH_VALUE_WEI
        || buy_value > MAX_BUY_VALUE_WEI
    {
        return Err("planned preflight parameters exceed the frozen v2 ceilings".into());
    }
    let chain = rpc_u64("eth_chainId", "[]")?;
    if chain != CHAIN_ID {
        return Err(format!("wrong chain: expected {CHAIN_ID}, got {chain}"));
    }
    let (block_number, block_hash) = latest_block()?;
    let portal_code = code_at(PORTAL, "portal")?;
    let impl_code = code_at(TAX_V3_IMPL_EXPECTED, "tax-v3 implementation")?;
    let network_gas_price = rpc_quantity("eth_gasPrice", "[]")?;
    let portal_version = decode_abi_string(&eth_call(PORTAL, VERSION_SELECTOR)?)?;
    let slot = rpc_data(
        "eth_getStorageAt",
        &format!(
            r#"[{},{},"latest"]"#,
            wire_string(PORTAL),
            wire_string(EIP1967_IMPLEMENTATION_SLOT)
        ),
    )?;
    if slot.len() != 32 {
        return Err("invalid EIP-1967 implementation slot".into());
    }
    let portal_impl = Address::from_slice(&slot[12..]);
    if portal_impl == Address::ZERO {
        return Err("portal implementation slot is zero".into());
    }
    let portal_impl_code = code_at(&portal_impl.to_string(), "portal implementation")?;
    let quote = abi_words(
        &eth_call(
            PORTAL,
            &format!("{GET_QUOTE_CONFIG_SELECTOR}{}", "0".repeat(64)),
        )?,
        5,
    )?;
    let fees = abi_words(&eth_call(PORTAL, GET_FEE_RATE_SELECTOR)?, 2)?;
    let tax = abi_words(&eth_call(PORTAL, TAX_ON_BC_SELECTOR)?, 1)?;
    if quote[0] != U256::from(1u64) {
        return Err("native BNB quote token is not enabled".into());
    }
    if fees.iter().any(|bps| *bps >= U256::from(10_000u64)) {
        return Err("invalid protocol fee bps".into());
    }
    let account = |who: Address| -> Result<(u64, U256), String> {
        Ok((rpc_pending_nonce(who)?, rpc_balance(who)?))
    };
    let (launcher_nonce, launcher_balance) = account(launcher)?;
    let (buyer_nonce, buyer_balance) = account(buyer)?;

    let launcher_required =
        U256::from(launch_value) + U256::from(launch_gas_limit) * U256::from(gas_price);
    let buyer_required = U256::from(buy_value) + U256::from(buy_gas_limit) * U256::from(gas_price);
    let word = |x: U256| V::U256(x.to_be_bytes());
    let sha = |bytes: &[u8]| V::B256(darkbot_bsc_record::sha256(bytes));
    let count = |bytes: &[u8]| u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let mut d = Draft::new(&f::SCHEMA);
    d.set(f::CHAIN_ID, chain)
        .set(f::LATEST_BLOCK_NUMBER, block_number)
        .set(f::LATEST_BLOCK_HASH, V::B256(block_hash))
        .set(
            f::PORTAL,
            V::Address(
                Address::from_str(PORTAL)
                    .map_err(|e| e.to_string())?
                    .into_array(),
            ),
        )
        .set(f::PORTAL_CODE_BYTES, count(&portal_code))
        .set(f::PORTAL_CODE_SHA256, sha(&portal_code))
        .set(f::PORTAL_VERSION, portal_version)
        .set(
            f::PORTAL_IMPLEMENTATION,
            V::Address(portal_impl.into_array()),
        )
        .set(
            f::PORTAL_IMPLEMENTATION_CODE_BYTES,
            count(&portal_impl_code),
        )
        .set(f::PORTAL_IMPLEMENTATION_CODE_SHA256, sha(&portal_impl_code))
        .set(
            f::TAX_V3_IMPL,
            V::Address(
                Address::from_str(TAX_V3_IMPL_EXPECTED)
                    .map_err(|e| e.to_string())?
                    .into_array(),
            ),
        )
        .set(f::TAX_V3_IMPL_CODE_BYTES, count(&impl_code))
        .set(f::TAX_V3_IMPL_CODE_SHA256, sha(&impl_code))
        .set(f::GAS_PRICE_WEI, word(network_gas_price))
        .set(f::QUOTE_ENABLED, word(quote[0]))
        .set(f::QUOTE_DEFAULT_CURVE_ID, word(quote[1]))
        .set(f::QUOTE_ALTERNATIVE_CURVE_ID, word(quote[2]))
        .set(f::QUOTE_NATIVE_TO_QUOTE_SWAP_TYPE, word(quote[3]))
        .set(f::QUOTE_DEX_ID, word(quote[4]))
        .set(f::FEE_BUY_BPS, word(fees[0]))
        .set(f::FEE_SELL_BPS, word(fees[1]))
        .set(f::TAX_ON_BONDING_CURVE_ENABLED, tax[0] != U256::ZERO)
        .set(f::LAUNCHER_ADDRESS, V::Address(launcher.into_array()))
        .set(f::LAUNCHER_PENDING_NONCE, launcher_nonce)
        .set(f::LAUNCHER_BALANCE_WEI, word(launcher_balance))
        .set(f::BUYER_ADDRESS, V::Address(buyer.into_array()))
        .set(f::BUYER_PENDING_NONCE, buyer_nonce)
        .set(f::BUYER_BALANCE_WEI, word(buyer_balance))
        .set(f::ADDRESSES_BOUND, true)
        .set(f::DISTINCT, launcher != buyer)
        .set(f::LAUNCHER_REQUIRED_WEI, word(launcher_required))
        .set(f::BUYER_REQUIRED_WEI, word(buyer_required))
        .set(
            f::COMBINED_RESERVE_WEI,
            word(launcher_required + buyer_required),
        )
        .set(
            f::PROPOSAL_MAX_RESERVE_WEI,
            V::u128(MAX_ATTEMPT_RESERVE_WEI),
        )
        .set(f::PROFILE_GAS_PRICE_WEI, word(U256::from(gas_price)))
        .set(f::PROFILE_LAUNCH_GAS_LIMIT, launch_gas_limit)
        .set(f::PROFILE_BUY_GAS_LIMIT, buy_gas_limit)
        .set(f::PROFILE_LAUNCH_VALUE_WEI, V::u128(launch_value))
        .set(f::PROFILE_BUY_VALUE_WEI, word(U256::from(buy_value)))
        .set(
            f::LAUNCHER_SUFFICIENT,
            launcher_balance >= launcher_required,
        )
        .set(f::BUYER_SUFFICIENT, buyer_balance >= buyer_required)
        .set(
            f::FUNDING_PASS,
            launcher_balance >= launcher_required && buyer_balance >= buyer_required,
        )
        .set(f::NETWORK_ACTIONS, "READ_ONLY_RPC_ONLY")
        .set(f::SIGNED_TX_SUBMITTED, false)
        .set(f::BUNDLE_SUBMITTED, false);
    let bytes = d.encode().map_err(|e| e.0)?;
    darkbot_bsc_record::decode(&f::SCHEMA, &bytes).map_err(|e| e.0)
}

fn field_name(tag: u16) -> &'static str {
    schema::g06a::preflight::SCHEMA
        .field(tag)
        .map_or("?", |f| f.name)
}

pub(crate) fn validate_current_preflight(
    baseline: &Record,
    current: &Record,
    launcher: Address,
    buyer: Address,
    signed_gas_price: u64,
) -> (u64, u64, u64) {
    use schema::g06a::preflight as f;
    let address = |r: &Record, tag: u16| {
        r.address(tag)
            .map(Address::from)
            .unwrap_or_else(|e| panic!("{e}"))
    };
    let flag = |tag: u16| current.bool(tag).unwrap_or_else(|e| panic!("{e}"));
    let int = |tag: u16| current.u64(tag).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(int(f::CHAIN_ID), CHAIN_ID);
    assert_eq!(
        address(current, f::PORTAL),
        Address::from_str(PORTAL).unwrap()
    );
    assert_eq!(address(current, f::LAUNCHER_ADDRESS), launcher);
    assert_eq!(address(current, f::BUYER_ADDRESS), buyer);
    assert!(flag(f::ADDRESSES_BOUND));
    assert!(flag(f::DISTINCT));
    assert!(flag(f::FUNDING_PASS));

    for tag in f::DRIFT_CHECKED {
        let a = baseline
            .get(tag)
            .unwrap_or_else(|| panic!("baseline preflight missing {}", field_name(tag)));
        let b = current
            .get(tag)
            .unwrap_or_else(|| panic!("current preflight missing {}", field_name(tag)));
        assert_eq!(
            a,
            b,
            "preflight deployment/economics drift at {}",
            field_name(tag)
        );
    }

    let network_gas_price = current
        .u256(f::GAS_PRICE_WEI)
        .map(U256::from_be_bytes)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        network_gas_price <= U256::from(signed_gas_price),
        "current network gas price {network_gas_price} exceeds signed gas price {signed_gas_price}"
    );

    (
        int(f::LAUNCHER_PENDING_NONCE),
        int(f::BUYER_PENDING_NONCE),
        int(f::LATEST_BLOCK_NUMBER),
    )
}

pub(crate) fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// The HTTPS JSON-RPC endpoint, parsed once so the heads around the release
/// are read over an already established TLS connection. The URL may carry an
/// API key in its path: it is never printed (no Debug outside tests).
#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub(crate) struct RpcEndpoint {
    host: String,
    port: u16,
    path: String,
}

pub(crate) fn parse_rpc_endpoint(url: &str) -> Result<RpcEndpoint, String> {
    let url = url.trim();
    let rest = url
        .get(..8)
        .filter(|scheme| scheme.eq_ignore_ascii_case("https://"))
        .map(|_| &url[8..])
        .ok_or("BSC_MAINNET_RPC_HTTPS must be an https:// URL")?;
    let (authority, path) = rest.split_at(rest.find(['/', '?']).unwrap_or(rest.len()));
    if authority.contains(['@', '[', ']']) {
        return Err(
            "BSC_MAINNET_RPC_HTTPS: userinfo and IPv6 literals are not supported by the G0.7 head poller"
                .into(),
        );
    }
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (
            host,
            port.parse::<u16>()
                .map_err(|_| "BSC_MAINNET_RPC_HTTPS has an invalid port")?,
        ),
        None => (authority, HTTPS_PORT),
    };
    if host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        return Err("BSC_MAINNET_RPC_HTTPS has an invalid host".into());
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host == CLUB48_HOST || host == BLOCKRAZOR_HOST {
        return Err("BSC_MAINNET_RPC_HTTPS must not point at a bundle builder".into());
    }
    let path = match path {
        "" => "/".to_string(),
        p if p.starts_with('?') => format!("/{p}"),
        p => p.to_string(),
    };
    if path.contains('#') || !path.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err("BSC_MAINNET_RPC_HTTPS has an invalid path".into());
    }
    Ok(RpcEndpoint { host, port, path })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChainHead {
    pub(crate) number: u64,
    pub(crate) hash: String,
    pub(crate) milli_timestamp: u64,
}

/// Parses an `eth_getBlockByNumber` result. The millisecond time is derived
/// from `timestamp` and `mixHash` (BEP-520: the milliseconds sit in the low
/// bytes of mixHash) and must equal `milliTimestamp` where the node reports it.
pub(crate) fn parse_head(block: &serde_json::Value) -> Result<ChainHead, String> {
    let text = |name: &str| {
        block
            .get(name)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("head block lacks {name}"))
    };
    let quantity = |name: &str| -> Result<u64, String> {
        text(name)?
            .strip_prefix("0x")
            .and_then(|h| u64::from_str_radix(h, 16).ok())
            .ok_or_else(|| format!("head block {name} is not a hex quantity"))
    };
    let word = |name: &str| -> Result<[u8; 32], String> {
        text(name)?
            .strip_prefix("0x")
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
            .ok_or_else(|| format!("head block {name} is not 32 bytes"))
    };
    let number = quantity("number")?;
    let hash = format!("0x{}", hex::encode(word("hash")?));
    let mix = word("mixHash")?;
    if mix[..30].iter().any(|b| *b != 0) {
        return Err("head block mixHash carries no BEP-520 millisecond value".into());
    }
    let millis = u64::from(u16::from_be_bytes([mix[30], mix[31]]));
    if millis >= 1000 {
        return Err(format!("head block mixHash milliseconds {millis} >= 1000"));
    }
    let milli_timestamp = quantity("timestamp")?
        .checked_mul(1000)
        .and_then(|ms| ms.checked_add(millis))
        .ok_or("head block timestamp overflow")?;
    if block.get("milliTimestamp").is_some() && quantity("milliTimestamp")? != milli_timestamp {
        return Err("head block milliTimestamp disagrees with timestamp and mixHash".into());
    }
    Ok(ChainHead {
        number,
        hash,
        milli_timestamp,
    })
}

/// One head read of the poller.
#[derive(Debug, Clone)]
pub(crate) struct HeadObservation {
    pub(crate) head: ChainHead,
    /// Local wall clock right after the response.
    pub(crate) local_unix_ms: u64,
    /// Staleness above the freshest read of this poller, from the monotonic
    /// clock and block timestamps only: a lower bound on the true staleness
    /// whatever the local wall clock says.
    pub(crate) excess_staleness_ms: i64,
    pub(crate) read_ns: u64,
}

/// Keep-alive JSON-RPC connection for `latest` head reads. TLS is set up in
/// advance, so a read costs one round trip; a connection the server closed is
/// re-established once per read.
pub(crate) struct HeadPoller {
    endpoint: RpcEndpoint,
    stream: Option<Tls>,
    request: Vec<u8>,
    epoch: Instant,
    /// Minimum of (monotonic ms - head ms) over all reads.
    min_offset_ms: Option<i64>,
}

impl HeadPoller {
    pub(crate) fn connect(endpoint: &RpcEndpoint) -> Result<Self, String> {
        let body =
            r#"{"jsonrpc":"2.0","id":1,"method":"eth_getBlockByNumber","params":["latest",false]}"#;
        let host = if endpoint.port == HTTPS_PORT {
            endpoint.host.clone()
        } else {
            format!("{}:{}", endpoint.host, endpoint.port)
        };
        let request = format!(
            "POST {} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: darkbot-g07\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            endpoint.path,
            body.len()
        )
        .into_bytes();
        let mut poller = Self {
            endpoint: endpoint.clone(),
            stream: None,
            request,
            epoch: Instant::now(),
            min_offset_ms: None,
        };
        poller.reconnect()?;
        Ok(poller)
    }

    fn reconnect(&mut self) -> Result<(), String> {
        self.stream = None;
        let stream = connect_tls_port(&self.endpoint.host, self.endpoint.port)?;
        stream
            .sock
            .set_read_timeout(Some(HEAD_READ_TIMEOUT))
            .map_err(|e| format!("head poller timeout: {e}"))?;
        self.stream = Some(stream);
        Ok(())
    }

    fn round_trip(&mut self) -> Result<Vec<u8>, String> {
        if self.stream.is_none() {
            self.reconnect()?;
        }
        let Some(stream) = self.stream.as_mut() else {
            return Err("head poller has no connection".into());
        };
        let response = stream
            .write_all(&self.request)
            .and_then(|()| stream.flush())
            .map_err(|e| format!("head request write: {e}"))
            .and_then(|()| read_http_response(stream));
        let response = match response {
            Ok(response) => response,
            Err(e) => {
                self.stream = None;
                return Err(e);
            }
        };
        if response.close {
            self.stream = None;
        }
        if response.status.split_whitespace().nth(1) != Some("200") {
            return Err(format!("head request answered {}", response.status));
        }
        Ok(response.body)
    }

    pub(crate) fn latest(&mut self) -> Result<HeadObservation, String> {
        let started = Instant::now();
        let body = match self.round_trip() {
            Ok(body) => body,
            Err(first) => self
                .round_trip()
                .map_err(|retry| format!("{first}; retry on a new connection: {retry}"))?,
        };
        let read_ns = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let mono_ms = i64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(i64::MAX);
        let local_unix_ms = unix_time_ms();
        let v: serde_json::Value =
            serde_json::from_slice(&body).map_err(|e| format!("head response is not JSON: {e}"))?;
        if let Some(err) = v.get("error") {
            return Err(format!("head request JSON-RPC error: {err}"));
        }
        let head = parse_head(v.get("result").ok_or("head response lacks result")?)?;
        let offset_ms = i64::try_from(head.milli_timestamp)
            .ok()
            .and_then(|at| mono_ms.checked_sub(at))
            .ok_or("head offset overflow")?;
        let min_offset_ms = self.min_offset_ms.map_or(offset_ms, |m| m.min(offset_ms));
        self.min_offset_ms = Some(min_offset_ms);
        Ok(HeadObservation {
            head,
            local_unix_ms,
            excess_staleness_ms: offset_ms - min_offset_ms,
            read_ns,
        })
    }

    /// Waits for the next block: returns the first successful read whose head
    /// is newer than the first successful read of this wait, so the send head
    /// is about one poll (a round trip, at least 50 ms apart) older than this
    /// endpoint's view. The wait fails at the MAX_CONSECUTIVE_POLL_ERRORS-th
    /// failed read in a row. The deadline is checked between reads; one read,
    /// including a reconnect, is bounded only by the connection timeouts
    /// (seconds), so the wait can overrun the deadline by that much; main
    /// bounds the builder idle time separately.
    pub(crate) fn next_head(&mut self, timeout: Duration) -> Result<HeadObservation, String> {
        let deadline = Instant::now() + timeout;
        let mut current: Option<u64> = None;
        let mut errors = 0u32;
        loop {
            let poll_started = Instant::now();
            if poll_started >= deadline {
                return Err(match current {
                    Some(n) => format!("no block after {n} within {} ms", timeout.as_millis()),
                    None => format!("no head read succeeded within {} ms", timeout.as_millis()),
                });
            }
            match self.latest() {
                Ok(observation) => {
                    errors = 0;
                    match current {
                        Some(n) if observation.head.number > n => return Ok(observation),
                        Some(_) => {}
                        None => current = Some(observation.head.number),
                    }
                }
                Err(e) => {
                    errors += 1;
                    if errors >= MAX_CONSECUTIVE_POLL_ERRORS {
                        return Err(format!("{errors} head reads failed in a row; last: {e}"));
                    }
                }
            }
            if let Some(rest) = HEAD_POLL_MIN_PERIOD.checked_sub(poll_started.elapsed()) {
                thread::sleep(rest);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BundleWindow {
    pub(crate) max_block: u64,
    /// 48Club expiry in Unix seconds: the second after the nominal second of
    /// head+2. It is not the head+2 bound itself (that is maxBlockNumber, one
    /// second of block times holds two or three blocks); it keeps a "maxTimestamp
    /// must exceed the current block time / now" check from refusing the bundle
    /// while head+2 is open, and caps a 48Club that ignored maxBlockNumber at
    /// blocks stamped before (that second + 1).
    pub(crate) max_timestamp: u64,
    pub(crate) head_age_ms: i64,
}

/// Derives the bundle expiry from the send head. Refuses a head below the
/// preflight head, a head at the preflight height with another hash, a head
/// older than one interval (by the wall clock or, independent of it, by the
/// poller's monotonic staleness bound) and a local clock behind the chain.
pub(crate) fn bundle_window(
    preflight_number: u64,
    preflight_hash: &str,
    head: &ChainHead,
    local_unix_ms: u64,
    excess_staleness_ms: i64,
) -> Result<BundleWindow, String> {
    if head.number < preflight_number {
        return Err(format!(
            "send head {} is below the preflight head {preflight_number}",
            head.number
        ));
    }
    if head.number == preflight_number && !head.hash.eq_ignore_ascii_case(preflight_hash) {
        return Err(format!(
            "send head {} has the preflight height but another hash",
            head.number
        ));
    }
    let head_age_ms = i64::try_from(local_unix_ms)
        .ok()
        .zip(i64::try_from(head.milli_timestamp).ok())
        .and_then(|(now, at)| now.checked_sub(at))
        .ok_or("head age overflow")?;
    if head_age_ms > MAX_HEAD_AGE_MS {
        return Err(format!(
            "send head {} is {head_age_ms} ms old (limit {MAX_HEAD_AGE_MS} ms)",
            head.number
        ));
    }
    if head_age_ms < -MAX_LOCAL_CLOCK_LAG_MS {
        return Err(format!(
            "local clock is {} ms behind the head timestamp (limit {MAX_LOCAL_CLOCK_LAG_MS} ms); head ages cannot be judged",
            -head_age_ms
        ));
    }
    if excess_staleness_ms > MAX_HEAD_AGE_MS {
        return Err(format!(
            "send head {} is at least {excess_staleness_ms} ms staler than this endpoint's freshest read (limit {MAX_HEAD_AGE_MS} ms)",
            head.number
        ));
    }
    let max_block = head
        .number
        .checked_add(MAX_BLOCK_OFFSET)
        .ok_or("max block overflow")?;
    let max_timestamp = head
        .milli_timestamp
        .checked_add(MAX_BLOCK_OFFSET * BLOCK_INTERVAL_MS)
        .ok_or("max timestamp overflow")?
        / 1000
        + 1;
    Ok(BundleWindow {
        max_block,
        max_timestamp,
        head_age_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0xb8f3cec4f6f874338f00ca12a8679df9eb1485ee6f3b8b3d2bb800f89cc4cb01";

    use serde_json::json;

    fn block(number: u64, seconds: u64, millis: u16) -> serde_json::Value {
        json!({
            "number": format!("0x{number:x}"),
            "hash": HASH,
            "timestamp": format!("0x{seconds:x}"),
            "mixHash": format!("0x{:064x}", millis),
        })
    }

    fn head(number: u64, milli_timestamp: u64) -> ChainHead {
        ChainHead {
            number,
            hash: HASH.into(),
            milli_timestamp,
        }
    }

    #[test]
    fn rpc_endpoint_keeps_path_and_query_and_rejects_what_it_cannot_frame() {
        assert_eq!(
            parse_rpc_endpoint("https://BSC.Example.org/v1/key?x=1").unwrap(),
            RpcEndpoint {
                host: "bsc.example.org".into(),
                port: 443,
                path: "/v1/key?x=1".into()
            }
        );
        let ep = parse_rpc_endpoint("HTTPS://node.example:8545").unwrap();
        assert_eq!((ep.port, ep.path.as_str()), (8545, "/"));
        assert_eq!(
            parse_rpc_endpoint("https://n.example?k=v").unwrap().path,
            "/?k=v"
        );
        for bad in [
            "http://node.example/",
            "https://user:pw@node.example/",
            "https://[::1]/",
            "https://node.example:99999/",
            "https:///path",
            "https://node.example/a b",
            "https://node.example/#frag",
            "https://node_example/",
            "https://Puissant-Builder.48.club/",
            "https://puissant-builder.48.club./",
            "https://./",
            "https://virginia.builder.blockrazor.io:443/",
        ] {
            assert!(parse_rpc_endpoint(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn head_milliseconds_come_from_mix_hash_and_must_match_the_reported_value() {
        let h = parse_head(&block(127_865_760, 1_760_000_000, 650)).unwrap();
        assert_eq!(h, head(127_865_760, 1_760_000_000_650));
        let mut reported = block(1, 2, 3);
        reported["milliTimestamp"] = json!("0x7d3");
        assert_eq!(parse_head(&reported).unwrap().milli_timestamp, 2_003);
        reported["milliTimestamp"] = json!("0x7d4");
        assert!(parse_head(&reported).is_err());
        assert!(parse_head(&block(1, 2, 1000)).is_err());
        let mut garbage = block(1, 2, 3);
        garbage["mixHash"] = json!(format!("0x01{}", "0".repeat(62)));
        assert!(parse_head(&garbage).is_err());
        let mut missing = block(1, 2, 3);
        missing.as_object_mut().unwrap().remove("hash");
        assert!(parse_head(&missing).is_err());
    }

    #[test]
    fn window_is_send_head_plus_two_with_the_48club_second_after_head_plus_two() {
        let w = bundle_window(100, HASH, &head(101, 1_000_200), 1_000_500, 0).unwrap();
        assert_eq!(
            w,
            BundleWindow {
                max_block: 103,
                max_timestamp: 1_002,
                head_age_ms: 300
            }
        );
        // head+2 nominally at 1_000_999 ms, second 1_000: maxTimestamp 1_001.
        assert_eq!(
            bundle_window(100, HASH, &head(100, 1_000_099), 1_000_100, 0)
                .unwrap()
                .max_timestamp,
            1_001
        );
        // maxTimestamp always exceeds the second of the head and of head+1, +2 at
        // the nominal interval, whatever the millisecond phase.
        for phase in 0..1000u64 {
            let t = 1_000_000 + phase;
            let w = bundle_window(1, HASH, &head(2, t), t, 0).unwrap();
            for k in 0..=MAX_BLOCK_OFFSET {
                assert!(
                    w.max_timestamp > (t + k * BLOCK_INTERVAL_MS) / 1000,
                    "{phase} {k}"
                );
            }
            assert!(w.max_timestamp * 1000 <= t + MAX_BLOCK_OFFSET * BLOCK_INTERVAL_MS + 1000);
        }
        // Boundaries: exactly one interval old, the clock-lag limit and the
        // staleness bound limit pass.
        assert!(bundle_window(100, HASH, &head(100, 5_000), 5_450, 450).is_ok());
        assert!(bundle_window(100, HASH, &head(100, 5_000), 4_850, 0).is_ok());
    }

    #[test]
    fn window_refuses_stale_regressed_reorged_or_unjudgeable_heads() {
        assert!(bundle_window(100, HASH, &head(99, 5_000), 5_100, 0).is_err());
        let other = "0x".to_string() + &"1".repeat(64);
        assert!(bundle_window(100, &other, &head(100, 5_000), 5_100, 0).is_err());
        assert!(bundle_window(100, &other, &head(101, 5_000), 5_100, 0).is_ok());
        assert!(bundle_window(100, HASH, &head(100, 5_000), 5_451, 0).is_err());
        assert!(bundle_window(100, HASH, &head(100, 5_000), 4_849, 0).is_err());
        // A slow wall clock cannot hide staleness the monotonic bound sees.
        assert!(bundle_window(100, HASH, &head(100, 5_000), 5_100, 451).is_err());
    }

    #[test]
    fn child_processes_see_only_path() {
        let out = child_command("env").output().unwrap();
        assert!(out.status.success());
        let vars: Vec<String> = String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(|l| l.split('=').next().unwrap_or("").to_string())
            .collect();
        assert!(vars.iter().all(|k| k == "PATH"), "{vars:?}");
    }
}

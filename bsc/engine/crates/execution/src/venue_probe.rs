//! The read-only startup probe of the live Flap venue (C3).
//!
//! Before the PAPER path may admit any BUY, the runtime observes the live
//! deployment and the venue terms the native BUY depends on; it never
//! assumes them. [`probe_flap`] reads both from one pinned canonical block,
//! [`VenueObservation::deployment`] is the input of
//! [`FlapDeployment::validate`], and [`VenueObservation::terms`] is what
//! `QualifiedNativeState::fresh_launch` compares with the witnessed terms.
//!
//! Reads (the calls of the G0.7 read-only preflight, ported): `eth_chainId`,
//! which must be the deployment's chain; `eth_blockNumber` and
//! `eth_getBlockByNumber(head, false)`, the pin. Then, pinned to that
//! block's hash (EIP-1898 with `requireCanonical`; never "latest"): the
//! Portal proxy's code, its EIP-1967 implementation slot, the
//! implementation's code, `version()`, the tax-token V3 implementation's
//! code, `getQuoteTokenConfiguration(address(0))`, `getFeeRate()`,
//! `enableTaxOnBondingCurve()` and the caller's entry of the Portal's
//! fee-exemption mapping. Last, the block at the pinned height must still be
//! the pinned one, or the reads straddled a reorg.
//!
//! Transport: the chain follower's HTTPS JSON-RPC client (one keep-alive
//! rustls connection). [`FollowerLimits`] bounds every connect, request and
//! response; the probe has no bound of its own. JSON exists only at this
//! wire: each answer is decoded at once into typed values, strictly
//! (canonical hex, exact ABI lengths, every value in its type's range).
//! Anything else fails closed with a [`ProbeError`]. There is no retry: the
//! caller decides whether to probe again.
//!
//! Fingerprints are SHA-256 of the runtime code, like the deployment's
//! constants (the G0.6a preflight's `darkbot_bsc_record::sha256`). The
//! Keccak-256 (`EXTCODEHASH`) and the length come with them, for
//! information only.

use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use alloy_primitives::{Address, B256, U256, hex, keccak256};
use darkbot_bsc_trading::economics::fees::BPS_DENOMINATOR;
use darkbot_bsc_trading::economics::flap::native_curve20::VenueBuyTerms;
use darkbot_bsc_trading::venues::flap::deployment::{
    FlapDeployment, ObservedDeployment, RuntimeSha256,
};
use darkbot_bsc_trading::venues::fourmeme::deployment::{
    CLASSIC_TEMPLATES, FourDeployment, ObservedFour,
};
use serde_json::Value;

use crate::chain_follower::http::Client;
use crate::chain_follower::jsonrpc::{
    data_fixed, data_vec, envelope, parse_quantity, quantity_u64,
};
use crate::chain_follower::{
    Field, FollowerConfigError, FollowerLimits, RequestError, connect_bounds, https_endpoint,
};
use crate::wss::DisconnectReason;
use crate::wss::endpoint::{Endpoint, connect_tls, tls_config};

/// One ABI word.
const WORD: usize = 32;
/// An address in its ABI word.
const ADDRESS: usize = 20;

// Selectors (Keccak-256 of the signatures in the verified v5.27.0 ABI of the
// G0.1 Flap refreeze; the G0.7 preflight's constants).
/// `version()`.
const VERSION_SELECTOR: [u8; 4] = [0x54, 0xfd, 0x4d, 0x50];
/// `getQuoteTokenConfiguration(address)`.
const GET_QUOTE_TOKEN_CONFIGURATION_SELECTOR: [u8; 4] = [0x26, 0xef, 0x20, 0xd5];
/// `getFeeRate()`.
const GET_FEE_RATE_SELECTOR: [u8; 4] = [0x84, 0xe5, 0xee, 0xd0];
/// `enableTaxOnBondingCurve()`.
const ENABLE_TAX_ON_BONDING_CURVE_SELECTOR: [u8; 4] = [0x1d, 0x31, 0xe1, 0x53];
/// `getTokenV8Safe(address)`: a token's curve and terms (01.11., R1: the
/// curve a real position is held again on).
const GET_TOKEN_V8_SAFE_SELECTOR: [u8; 4] = [0x62, 0xfa, 0xfc, 0xca];
/// `nftGate(address)`.
const NFT_GATE_SELECTOR: [u8; 4] = [0x9b, 0x95, 0xf8, 0xdc];
/// The words of `getTokenV8Safe`'s static return struct.
const TOKEN_STATE_WORDS: usize = 18;
/// Every Portal selector the probe calls (BSC3-065: a behaviour parity
/// requires each in the observed dispatcher).
pub const PROBED_SELECTORS: [[u8; 4]; 4] = [
    VERSION_SELECTOR,
    GET_QUOTE_TOKEN_CONFIGURATION_SELECTOR,
    GET_FEE_RATE_SELECTOR,
    ENABLE_TAX_ON_BONDING_CURVE_SELECTOR,
];

/// EIP-1967 implementation slot: `keccak256("eip1967.proxy.implementation") - 1`.
const EIP1967_IMPLEMENTATION_SLOT: [u8; 32] =
    hex!("360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc");

/// The Portal's storage slot of `_feeExemptedTraders`
/// (`mapping(address => bool)`, keyed by the BUY's `msg.sender`). The
/// Portal ABI has no getter for it. Witnessed by
/// `bsc/native/evidence/g01/flap_econ_v5_27_20261031T122809Z/flap_econ.dbrec`
/// (`g01_flap_econ`, schema v2, BSC3-065): the Sourcify storage layout of
/// the verified v5.27.0 implementation places the variable here, and on the
/// live v5.27.2 state (block 130856906, the production pin validated) the
/// node's BUY with [`fee_exemption_key`] of the buyer set to `true` pays no
/// fee instead of the 1 % one (as before on 129892713,
/// `flap_econ_v5_27_20261026T115316Z`).
/// The G0.2 archive witness saw the v5.24.0 BUY path read the same mapping
/// (PC `0x2d09`).
pub const FEE_EXEMPTION_MAPPING_SLOT: u64 = 177;

/// The quote token `getQuoteTokenConfiguration` is asked for: native BNB.
const NATIVE_QUOTE_TOKEN: Address = Address::ZERO;

/// `getQuoteTokenConfiguration` returns `(enabled, defaultCurve,
/// alternativeCurve, nativeToQuoteSwapType, dexId)`, each one `uint8` word.
const QUOTE_CONFIGURATION_WORDS: usize = 5;
/// `getFeeRate` returns the BUY and the SELL fee in basis points.
const FEE_RATE_WORDS: usize = 2;

/// One read of the probe, in the order the probe makes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeRead {
    ChainId,
    BlockNumber,
    /// `eth_getBlockByNumber(head, false)`: the pin.
    Block,
    PortalCode,
    ImplementationSlot,
    ImplementationCode,
    Version,
    TaxImplementationCode,
    QuoteConfiguration,
    FeeRate,
    TaxOnBondingCurve,
    FeeExemption,
    /// `eth_getBalance` of the probed caller (the PAPER sender must hold
    /// nothing).
    CallerBalance,
    /// The pinned height read again after the state reads.
    BlockRecheck,
    /// The Four.meme drift guard (E review L3): a frozen contract's code,
    /// and a frozen proxy's EIP-1967 implementation slot.
    FourCode,
    FourImplementationSlot,
    /// 01.11. (R1): a held token's `getTokenV8Safe` and `nftGate`.
    TokenState,
    NftGate,
    /// REV.1.0 R5: the Portal's `quoteExactInput` of a migrated lot's SELL.
    DexQuote,
}

/// How a well-formed hex answer violates its ABI or storage type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbiFault {
    /// Not exactly the number of words of the return type.
    Length,
    /// A word outside its type: non-zero high bytes of an address, a
    /// `uint8` or a `uint16`.
    Range,
    /// A flag (a `bool`, or the `uint8` `enabled`) other than 0 or 1.
    Flag,
    /// A fee not below the basis-point denominator.
    FeeBps,
    /// A `string` whose offset, length or padding is not the canonical
    /// encoding.
    StringEncoding,
    /// A `string` that is not UTF-8.
    Utf8,
    /// The proxy's implementation slot holds the zero address.
    ZeroAddress,
}

/// Why the probe observed nothing. Every failure is final for this probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeError {
    /// The endpoint, the limits or the TLS configuration is refused before
    /// any connect.
    Config(FollowerConfigError),
    /// A request failed: transport, rate limit, HTTP, JSON, a JSON-RPC
    /// error, or a field of the wrong shape (malformed hex included).
    Request {
        read: ProbeRead,
        error: RequestError,
    },
    /// The node is on another chain than the deployment.
    WrongChain { expected: u64, observed: u64 },
    /// The node answered `null` for the block at the head it had reported.
    BlockUnavailable { read: ProbeRead, number: u64 },
    /// A contract the deployment requires has no code at the pinned block.
    NoCode(ProbeRead),
    /// A hex answer that violates its ABI or storage type.
    Abi { read: ProbeRead, fault: AbiFault },
    /// After the state reads, the pinned block is no longer the canonical
    /// block at its height.
    Reorg { number: u64 },
}

impl core::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Config(e) => write!(f, "venue probe: configuration refused: {e:?}"),
            Self::Request { read, error } => write!(f, "venue probe: {read:?}: {error:?}"),
            Self::WrongChain { expected, observed } => write!(
                f,
                "venue probe: wrong chain: expected {expected}, node reports {observed}"
            ),
            Self::BlockUnavailable { read, number } => {
                write!(f, "venue probe: {read:?}: block {number} is unavailable")
            }
            Self::NoCode(read) => write!(f, "venue probe: {read:?}: no code at the pinned block"),
            Self::Abi { read, fault } => write!(f, "venue probe: {read:?}: {fault:?}"),
            Self::Reorg { number } => write!(
                f,
                "venue probe: block {number} left the canonical chain during the reads"
            ),
        }
    }
}

impl std::error::Error for ProbeError {}

/// A contract's runtime code as the probe saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeFingerprint {
    /// The deployment's fingerprint: SHA-256 of the runtime code.
    pub sha256: RuntimeSha256,
    /// `EXTCODEHASH`, for information.
    pub keccak256: B256,
    /// Length in bytes, for information.
    pub len: usize,
}

impl CodeFingerprint {
    fn of(code: &[u8]) -> Self {
        Self {
            sha256: darkbot_bsc_record::sha256(code),
            keccak256: keccak256(code),
            len: code.len(),
        }
    }
}

/// The live deployment as read at the pinned block (the owned data of an
/// [`ObservedDeployment`], whose chain id is [`VenueObservation::chain_id`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedDeploymentData {
    /// `version()` of the Portal.
    pub portal_version: String,
    /// The Portal proxy's code.
    pub portal_code: CodeFingerprint,
    /// The implementation in the proxy's EIP-1967 slot.
    pub portal_implementation: Address,
    /// That implementation's code.
    pub portal_implementation_code: CodeFingerprint,
    /// The code of the deployment's tax-token V3 implementation.
    pub tax_v3_implementation_code: CodeFingerprint,
}

/// What [`probe_flap`] observed, all at one canonical block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VenueObservation {
    /// The pinned block: the head when the probe started.
    pub block_number: u64,
    pub block_hash: [u8; 32],
    /// `eth_chainId`, equal to the deployment's.
    pub chain_id: u64,
    pub observed: ObservedDeploymentData,
    /// The venue terms of a native BUY by the probed caller.
    pub terms: VenueBuyTerms,
    /// The caller's native balance at the pinned block.
    pub caller_balance_wei: U256,
}

impl VenueObservation {
    /// The observation as [`FlapDeployment::validate`] takes it.
    pub fn deployment(&self) -> ObservedDeployment<'_> {
        ObservedDeployment {
            chain_id: self.chain_id,
            portal_version: &self.observed.portal_version,
            portal_runtime_sha256: self.observed.portal_code.sha256,
            portal_implementation: self.observed.portal_implementation,
            portal_implementation_runtime_sha256: self.observed.portal_implementation_code.sha256,
            tax_v3_implementation_runtime_sha256: self.observed.tax_v3_implementation_code.sha256,
        }
    }
}

/// Observes the live Flap deployment and the venue terms of a native BUY by
/// `caller`, read-only, at one canonical block of `endpoint`
/// (`https://host[:port][/path]`; the caller supplies it, no endpoint is
/// compiled in). Every bound comes from `limits`: the connect and TLS
/// bounds, and per request `request_timeout` and `max_response_bytes`.
///
/// It reads and decodes only; it does not judge. The caller passes
/// [`VenueObservation::deployment`] to [`FlapDeployment::validate`] and
/// compares [`VenueObservation::terms`] with the witnessed terms.
pub fn probe_flap(
    endpoint: &str,
    limits: &FollowerLimits,
    deployment: &FlapDeployment,
    caller: Address,
) -> Result<VenueObservation, ProbeError> {
    let t = Target::of(endpoint, limits)?;
    let connect =
        |stop: &AtomicBool| connect_tls(&t.endpoint, &t.tls, &t.bounds, stop, Instant::now());
    let mut client = Client::new(connect, t.authority.clone(), t.path.clone(), limits);
    probe_with(&mut client, deployment, caller)
}

/// The HTTPS JSON-RPC target of the probe's reads: `endpoint`
/// (`https://host[:port][/path]`; `https://` and `wss://` share the
/// authority and path rules and the default port 443, as the follower
/// connects), validated with `limits` and the connect bounds they set.
struct Target {
    endpoint: Endpoint,
    tls: std::sync::Arc<rustls::ClientConfig>,
    authority: String,
    path: String,
    bounds: crate::wss::WssLimits,
}

impl Target {
    fn of(endpoint: &str, limits: &FollowerLimits) -> Result<Self, ProbeError> {
        let config = ProbeError::Config;
        let target = https_endpoint(endpoint).map_err(config)?;
        limits
            .validate()
            .map_err(|e| config(FollowerConfigError::Limits(e)))?;
        let tls = tls_config().map_err(|_| config(FollowerConfigError::Tls))?;
        let (authority, path) = target.authority_and_path();
        Ok(Target {
            endpoint: target,
            tls,
            authority,
            path,
            bounds: connect_bounds(limits),
        })
    }
}

/// [`probe_flap`] over a connected client (plain TCP in the tests).
fn probe_with<S, C>(
    client: &mut Client<S, C>,
    deployment: &FlapDeployment,
    caller: Address,
) -> Result<VenueObservation, ProbeError>
where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
{
    let mut wire = Wire {
        client,
        stop: AtomicBool::new(false),
        id: 0,
    };
    let chain_id = wire.quantity(ProbeRead::ChainId, "eth_chainId", "[]")?;
    if chain_id != deployment.chain_id {
        return Err(ProbeError::WrongChain {
            expected: deployment.chain_id,
            observed: chain_id,
        });
    }
    let block_number = wire.quantity(ProbeRead::BlockNumber, "eth_blockNumber", "[]")?;
    let block_hash = wire.block_hash(ProbeRead::Block, block_number)?;
    let at = pinned(&block_hash);

    let portal_code = wire.code(&at, ProbeRead::PortalCode, deployment.portal)?;
    let read = ProbeRead::ImplementationSlot;
    let slot = wire.storage(&at, read, deployment.portal, &EIP1967_IMPLEMENTATION_SLOT)?;
    let portal_implementation = word_address(&slot).map_err(abi(read))?;
    if portal_implementation == Address::ZERO {
        return Err(abi(read)(AbiFault::ZeroAddress));
    }
    let portal_implementation_code =
        wire.code(&at, ProbeRead::ImplementationCode, portal_implementation)?;
    let read = ProbeRead::Version;
    let portal_version = abi_string(&wire.call(&at, read, deployment.portal, &VERSION_SELECTOR)?)
        .map_err(abi(read))?;
    let tax_v3_implementation_code = wire.code(
        &at,
        ProbeRead::TaxImplementationCode,
        deployment.tax_v3_implementation,
    )?;
    let terms = venue_terms(&mut wire, &at, deployment.portal, caller)?;
    let caller_balance_wei = wire.balance(&at, ProbeRead::CallerBalance, caller)?;

    // Every pinned read was of one canonical block: it is still the block
    // at its height, or the reads straddled a reorg.
    if wire.block_hash(ProbeRead::BlockRecheck, block_number)? != block_hash {
        return Err(ProbeError::Reorg {
            number: block_number,
        });
    }
    Ok(VenueObservation {
        block_number,
        block_hash,
        chain_id,
        observed: ObservedDeploymentData {
            portal_version,
            portal_code,
            portal_implementation,
            portal_implementation_code,
            tax_v3_implementation_code,
        },
        terms,
        caller_balance_wei,
    })
}

/// 01.11. (R1): a Flap token's curve as the Portal held it at the end of
/// one canonical block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlapCurve {
    pub block_number: u64,
    pub block_hash: [u8; 32],
    pub terms: darkbot_bsc_trading::economics::flap::native_curve20::TokenTerms,
    pub reserve: U256,
    pub circulating: U256,
    /// REV.1.0 R5: the token migrated to its DEX (status DEX): off the
    /// curve, the Portal routes its SELL through its DEX router.
    pub dex_listed: bool,
}

/// REV.1.0 R5 (BSC3-030): the Portal's quote of a migrated token's SELL at
/// one canonical block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DexQuote {
    pub block_number: u64,
    pub block_hash: [u8; 32],
    /// The native output the Portal's `quoteExactInput` answered.
    pub output: U256,
}

/// REV.1.0 R5 (BSC3-030): the Portal `portal`'s quote of a native-output
/// SELL of `amount` of `token` at the block `number` whose hash is `hash`
/// (`quoteExactInput` pinned to that hash; the block still the one at its
/// height afterwards), read-only over `endpoint` as [`read_flap_curve`].
pub fn read_flap_dex_quote(
    endpoint: &str,
    limits: &FollowerLimits,
    portal: Address,
    token: Address,
    amount: U256,
    number: u64,
    hash: [u8; 32],
) -> Result<DexQuote, ProbeError> {
    let t = Target::of(endpoint, limits)?;
    let connect =
        |stop: &AtomicBool| connect_tls(&t.endpoint, &t.tls, &t.bounds, stop, Instant::now());
    let mut client = Client::new(connect, t.authority.clone(), t.path.clone(), limits);
    quote_with(&mut client, portal, token, amount, number, hash)
}

/// `quoteExactInput((address,address,uint256))` (REV.1.0 R5: the Portal's
/// own quote, also of a token it routes to its DEX).
const QUOTE_EXACT_INPUT_SELECTOR: [u8; 4] = [0xfc, 0x84, 0x7c, 0x2b];

/// The Portal's quote of a native-output SELL of `amount` of `token`: its
/// static tuple inline (the token, native, the amount).
fn dex_quote_calldata(token: Address, amount: U256) -> Vec<u8> {
    let mut calldata = QUOTE_EXACT_INPUT_SELECTOR.to_vec();
    calldata.extend_from_slice(&address_word(token));
    calldata.extend_from_slice(&address_word(Address::ZERO));
    calldata.extend_from_slice(&amount.to_be_bytes::<32>());
    calldata
}

/// [`read_flap_dex_quote`] over a connected client (plain TCP in the tests).
fn quote_with<S, C>(
    client: &mut Client<S, C>,
    portal: Address,
    token: Address,
    amount: U256,
    number: u64,
    hash: [u8; 32],
) -> Result<DexQuote, ProbeError>
where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
{
    let read = ProbeRead::DexQuote;
    let calldata = dex_quote_calldata(token, amount);
    let mut wire = Wire {
        client,
        stop: AtomicBool::new(false),
        id: 0,
    };
    let [output] =
        abi_words::<1>(&wire.call(&pinned(&hash), read, portal, &calldata)?).map_err(abi(read))?;
    if wire.block_hash(ProbeRead::BlockRecheck, number)? != hash {
        return Err(ProbeError::Reorg { number });
    }
    Ok(DexQuote {
        block_number: number,
        block_hash: hash,
        output: U256::from_be_bytes(output),
    })
}

/// 01.11. (R1): the curve of `token` as the Portal `portal` held it at the
/// block `number` whose hash is `hash` (`getTokenV8Safe`, `nftGate`, both
/// pinned to that hash), read-only over `endpoint` as [`probe_flap`] reads:
/// the curve a real LIVE position the strategy no longer follows is held
/// again on. The block must still be the one at its height after the
/// reads (otherwise [`ProbeError::Reorg`]); no other block stands in.
pub fn read_flap_curve(
    endpoint: &str,
    limits: &FollowerLimits,
    portal: Address,
    token: Address,
    number: u64,
    hash: [u8; 32],
) -> Result<FlapCurve, ProbeError> {
    let t = Target::of(endpoint, limits)?;
    let connect =
        |stop: &AtomicBool| connect_tls(&t.endpoint, &t.tls, &t.bounds, stop, Instant::now());
    let mut client = Client::new(connect, t.authority.clone(), t.path.clone(), limits);
    curve_with(&mut client, portal, token, number, hash)
}

/// [`read_flap_curve`] over a connected client (plain TCP in the tests).
fn curve_with<S, C>(
    client: &mut Client<S, C>,
    portal: Address,
    token: Address,
    number: u64,
    hash: [u8; 32],
) -> Result<FlapCurve, ProbeError>
where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
{
    use darkbot_bsc_trading::economics::flap::curve::Curve;
    use darkbot_bsc_trading::economics::flap::native_curve20::TokenTerms;
    let mut wire = Wire {
        client,
        stop: AtomicBool::new(false),
        id: 0,
    };
    let at = pinned(&hash);
    let mut calldata = GET_TOKEN_V8_SAFE_SELECTOR.to_vec();
    calldata.extend_from_slice(&address_word(token));
    let read = ProbeRead::TokenState;
    let w = abi_words::<TOKEN_STATE_WORDS>(&wire.call(&at, read, portal, &calldata)?)
        .map_err(abi(read))?;
    let fault = |f| abi(read)(f);
    let status = word_u8(&w[0]).map_err(fault)?;
    let dex_listed = status == darkbot_bsc_trading::venues::flap::TOKEN_STATUS_DEX;
    let terms = TokenTerms {
        tradable: status == darkbot_bsc_trading::venues::flap::TOKEN_STATUS_TRADABLE,
        version: word_u8(&w[4]).map_err(fault)?,
        quote_token: word_address(&w[9]).map_err(fault)?.into_array(),
        nft_gate: [0; 20],
        curve: Curve {
            r: U256::from_be_bytes(w[5]),
            h: U256::from_be_bytes(w[6]),
            k: U256::from_be_bytes(w[7]),
        },
        dex_supply_threshold: U256::from_be_bytes(w[8]),
        buy_tax_bps: word_bps(&w[12]).map_err(fault)?,
        sell_tax_bps: word_bps(&w[13]).map_err(fault)?,
    };
    let mut calldata = NFT_GATE_SELECTOR.to_vec();
    calldata.extend_from_slice(&address_word(token));
    let read = ProbeRead::NftGate;
    let [gate] = abi_words::<1>(&wire.call(&at, read, portal, &calldata)?).map_err(abi(read))?;
    let nft_gate = word_address(&gate).map_err(abi(read))?.into_array();
    if wire.block_hash(ProbeRead::BlockRecheck, number)? != hash {
        return Err(ProbeError::Reorg { number });
    }
    Ok(FlapCurve {
        block_number: number,
        block_hash: hash,
        terms: TokenTerms { nft_gate, ..terms },
        reserve: U256::from_be_bytes(w[1]),
        circulating: U256::from_be_bytes(w[2]),
        dex_listed,
    })
}

/// The live Four.meme deployment at one canonical block (Phase E review L3,
/// the drift guard): each frozen proxy's EIP-1967 implementation and its
/// code's keccak, every Classic pair's code; `FourDeployment::validate`
/// compares it with the pins (`drifts` names every difference). Limit
/// (Paket 8 review L2): code identity only, not TokenManager2's template
/// configuration; a further (deployer, template) pair under the same code
/// shows only afterwards, as a canonical Four mismatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FourObservation {
    pub block_number: u64,
    pub block_hash: [u8; 32],
    pub observed: ObservedFour,
}

/// Observes the live Four.meme deployment read-only at one canonical block
/// of `endpoint`, bounded by `limits`, as [`probe_flap`] does the Flap one.
pub fn probe_four(
    endpoint: &str,
    limits: &FollowerLimits,
    deployment: &FourDeployment,
) -> Result<FourObservation, ProbeError> {
    let t = Target::of(endpoint, limits)?;
    let connect =
        |stop: &AtomicBool| connect_tls(&t.endpoint, &t.tls, &t.bounds, stop, Instant::now());
    let mut client = Client::new(connect, t.authority.clone(), t.path.clone(), limits);
    probe_four_with(&mut client, deployment)
}

/// [`probe_four`] over a connected client (plain TCP in the tests).
fn probe_four_with<S, C>(
    client: &mut Client<S, C>,
    deployment: &FourDeployment,
) -> Result<FourObservation, ProbeError>
where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
{
    let mut wire = Wire {
        client,
        stop: AtomicBool::new(false),
        id: 0,
    };
    let chain_id = wire.quantity(ProbeRead::ChainId, "eth_chainId", "[]")?;
    if chain_id != deployment.chain_id {
        return Err(ProbeError::WrongChain {
            expected: deployment.chain_id,
            observed: chain_id,
        });
    }
    let block_number = wire.quantity(ProbeRead::BlockNumber, "eth_blockNumber", "[]")?;
    let block_hash = wire.block_hash(ProbeRead::Block, block_number)?;
    let at = pinned(&block_hash);
    // The proxies: their live implementation, then its code (also when it
    // is not the pinned one: the observation is the chain's, not a verdict).
    let mut implementation = |proxy: Address| -> Result<(Address, B256), ProbeError> {
        let read = ProbeRead::FourImplementationSlot;
        let slot = wire.storage(&at, read, proxy, &EIP1967_IMPLEMENTATION_SLOT)?;
        let live = word_address(&slot).map_err(abi(read))?;
        Ok((live, wire.code(&at, ProbeRead::FourCode, live)?.keccak256))
    };
    let tm2 = implementation(deployment.token_manager2)?;
    let core = implementation(deployment.openfour_core)?;
    let create_deployer = implementation(deployment.openfour_create_deployer)?;
    // The Classic pairs: each deployer's and template's code.
    let mut classic = [(B256::ZERO, B256::ZERO); CLASSIC_TEMPLATES];
    for (slot, t) in classic.iter_mut().zip(&deployment.classic_templates) {
        let deployer = wire.code(&at, ProbeRead::FourCode, t.deployer)?.keccak256;
        let template = wire.code(&at, ProbeRead::FourCode, t.template)?.keccak256;
        *slot = (deployer, template);
    }
    let observed = ObservedFour {
        chain_id,
        token_manager2_implementation: tm2.0,
        token_manager2_implementation_runtime_keccak256: tm2.1,
        openfour_core_implementation: core.0,
        openfour_core_implementation_runtime_keccak256: core.1,
        openfour_create_deployer_implementation: create_deployer.0,
        openfour_create_deployer_implementation_runtime_keccak256: create_deployer.1,
        classic_runtime_keccak256: classic,
    };
    if wire.block_hash(ProbeRead::BlockRecheck, block_number)? != block_hash {
        return Err(ProbeError::Reorg {
            number: block_number,
        });
    }
    Ok(FourObservation {
        block_number,
        block_hash,
        observed,
    })
}

/// The venue terms of a native BUY by `caller`, read at `at`.
fn venue_terms<S, C>(
    wire: &mut Wire<'_, S, C>,
    at: &str,
    portal: Address,
    caller: Address,
) -> Result<VenueBuyTerms, ProbeError>
where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
{
    let read = ProbeRead::QuoteConfiguration;
    let mut calldata = GET_QUOTE_TOKEN_CONFIGURATION_SELECTOR.to_vec();
    calldata.extend_from_slice(&address_word(NATIVE_QUOTE_TOKEN));
    let quote = abi_words::<QUOTE_CONFIGURATION_WORDS>(&wire.call(at, read, portal, &calldata)?)
        .map_err(abi(read))?;
    let native_quote_enabled = word_flag(&quote[0]).map_err(abi(read))?;
    let default_curve_id = word_u8(&quote[1]).map_err(abi(read))?;

    let read = ProbeRead::FeeRate;
    let [buy, sell] =
        abi_words::<FEE_RATE_WORDS>(&wire.call(at, read, portal, &GET_FEE_RATE_SELECTOR)?)
            .map_err(abi(read))?;
    let protocol_buy_fee_bps = word_bps(&buy).map_err(abi(read))?;
    let protocol_sell_fee_bps = word_bps(&sell).map_err(abi(read))?;

    let read = ProbeRead::TaxOnBondingCurve;
    let [tax] =
        abi_words::<1>(&wire.call(at, read, portal, &ENABLE_TAX_ON_BONDING_CURVE_SELECTOR)?)
            .map_err(abi(read))?;
    let tax_on_bonding_curve_enabled = word_flag(&tax).map_err(abi(read))?;

    let read = ProbeRead::FeeExemption;
    let exemption = wire.storage(at, read, portal, &fee_exemption_key(caller))?;
    let caller_fee_exempt = word_flag(&exemption).map_err(abi(read))?;

    Ok(VenueBuyTerms {
        native_quote_enabled,
        default_curve_id,
        protocol_buy_fee_bps,
        protocol_sell_fee_bps,
        tax_on_bonding_curve_enabled,
        caller_fee_exempt,
    })
}

fn abi(read: ProbeRead) -> impl Fn(AbiFault) -> ProbeError {
    move |fault| ProbeError::Abi { read, fault }
}

/// The EIP-1898 block parameter of exactly this block; the node must refuse
/// it when the block is not canonical. Never replaced by "latest".
fn pinned(hash: &[u8; 32]) -> String {
    format!(
        r#"{{"blockHash":"0x{}","requireCanonical":true}}"#,
        hex::encode(hash)
    )
}

/// The storage key of `caller` in the fee-exemption mapping:
/// `keccak256(abi.encode(caller, uint256(FEE_EXEMPTION_MAPPING_SLOT)))`.
pub fn fee_exemption_key(caller: Address) -> [u8; 32] {
    let mut preimage = [0u8; 2 * WORD];
    preimage[..WORD].copy_from_slice(&address_word(caller));
    preimage[WORD..].copy_from_slice(&u64_word(FEE_EXEMPTION_MAPPING_SLOT));
    keccak256(preimage).0
}

fn address_word(address: Address) -> [u8; WORD] {
    let mut word = [0u8; WORD];
    word[WORD - ADDRESS..].copy_from_slice(address.as_slice());
    word
}

fn u64_word(value: u64) -> [u8; WORD] {
    let mut word = [0u8; WORD];
    let bytes = value.to_be_bytes();
    word[WORD - bytes.len()..].copy_from_slice(&bytes);
    word
}

/// The JSON-RPC exchanges of one probe over its client.
struct Wire<'a, S, C> {
    client: &'a mut Client<S, C>,
    /// Never set: the probe is bounded by the limits alone.
    stop: AtomicBool,
    id: u64,
}

impl<S, C> Wire<'_, S, C>
where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
{
    /// Posts one request; the answer's body and the request's id.
    fn post(
        &mut self,
        read: ProbeRead,
        method: &str,
        params: &str,
    ) -> Result<(Vec<u8>, u64), ProbeError> {
        self.id += 1;
        let id = self.id;
        let body =
            format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"{method}","params":{params}}}"#);
        self.client
            .post(body.as_bytes(), &self.stop)
            .map(|answer| (answer, id))
            .map_err(|error| ProbeError::Request { read, error })
    }

    /// The `result` as a canonical quantity of at most 64 bits.
    fn quantity(&mut self, read: ProbeRead, method: &str, params: &str) -> Result<u64, ProbeError> {
        let (body, id) = self.post(read, method, params)?;
        parse_quantity(&body, id).map_err(|error| ProbeError::Request { read, error })
    }

    /// The `result` as a string, the only JSON value this module reads.
    fn text(&mut self, read: ProbeRead, method: &str, params: &str) -> Result<String, ProbeError> {
        let (body, id) = self.post(read, method, params)?;
        match envelope(&body, id).map_err(|error| ProbeError::Request { read, error })? {
            Value::String(text) => Ok(text),
            _ => Err(shape(read, Field::Result)),
        }
    }

    /// The hash of block `number` (`eth_getBlockByNumber(number, false)`),
    /// whose `number` must be the one asked for.
    fn block_hash(&mut self, read: ProbeRead, number: u64) -> Result<[u8; 32], ProbeError> {
        let (body, id) = self.post(
            read,
            "eth_getBlockByNumber",
            &format!(r#"["0x{number:x}",false]"#),
        )?;
        let block =
            match envelope(&body, id).map_err(|error| ProbeError::Request { read, error })? {
                Value::Null => return Err(ProbeError::BlockUnavailable { read, number }),
                Value::Object(block) => block,
                _ => return Err(shape(read, Field::Result)),
            };
        if block
            .get("number")
            .and_then(Value::as_str)
            .and_then(quantity_u64)
            != Some(number)
        {
            return Err(shape(read, Field::Number));
        }
        block
            .get("hash")
            .and_then(Value::as_str)
            .and_then(data_fixed::<32>)
            .ok_or_else(|| shape(read, Field::Hash))
    }

    /// The runtime code of `address` at `at`; it must not be empty.
    fn code(
        &mut self,
        at: &str,
        read: ProbeRead,
        address: Address,
    ) -> Result<CodeFingerprint, ProbeError> {
        let params = format!(r#"["0x{}",{at}]"#, hex::encode(address));
        let text = self.text(read, "eth_getCode", &params)?;
        let code = data_vec(&text).ok_or_else(|| shape(read, Field::Result))?;
        if code.is_empty() {
            return Err(ProbeError::NoCode(read));
        }
        Ok(CodeFingerprint::of(&code))
    }

    /// The native balance of `address` at `at` (a canonical quantity).
    fn balance(&mut self, at: &str, read: ProbeRead, address: Address) -> Result<U256, ProbeError> {
        let params = format!(r#"["0x{}",{at}]"#, hex::encode(address));
        let text = self.text(read, "eth_getBalance", &params)?;
        let digits = text
            .strip_prefix("0x")
            .filter(|d| !d.is_empty() && d.len() <= 64 && (*d == "0" || !d.starts_with('0')))
            .ok_or_else(|| shape(read, Field::Result))?;
        U256::from_str_radix(digits, 16).map_err(|_| shape(read, Field::Result))
    }

    /// The storage word of `address` at `key`, at `at`.
    fn storage(
        &mut self,
        at: &str,
        read: ProbeRead,
        address: Address,
        key: &[u8; 32],
    ) -> Result<[u8; 32], ProbeError> {
        let params = format!(
            r#"["0x{}","0x{}",{at}]"#,
            hex::encode(address),
            hex::encode(key)
        );
        let text = self.text(read, "eth_getStorageAt", &params)?;
        data_fixed::<32>(&text).ok_or_else(|| shape(read, Field::Result))
    }

    /// The return data of `eth_call` of `calldata` to `to`, at `at`.
    fn call(
        &mut self,
        at: &str,
        read: ProbeRead,
        to: Address,
        calldata: &[u8],
    ) -> Result<Vec<u8>, ProbeError> {
        let params = format!(
            r#"[{{"to":"0x{}","data":"0x{}"}},{at}]"#,
            hex::encode(to),
            hex::encode(calldata)
        );
        let text = self.text(read, "eth_call", &params)?;
        data_vec(&text).ok_or_else(|| shape(read, Field::Result))
    }
}

fn shape(read: ProbeRead, field: Field) -> ProbeError {
    ProbeError::Request {
        read,
        error: RequestError::Shape(field),
    }
}

/// Exactly `N` ABI words.
fn abi_words<const N: usize>(raw: &[u8]) -> Result<[[u8; WORD]; N], AbiFault> {
    if raw.len() != N * WORD {
        return Err(AbiFault::Length);
    }
    let mut words = [[0u8; WORD]; N];
    for (word, chunk) in words.iter_mut().zip(raw.chunks_exact(WORD)) {
        word.copy_from_slice(chunk);
    }
    Ok(words)
}

/// The low `N` bytes of a word whose other bytes are zero.
fn word_low<const N: usize>(word: &[u8; WORD]) -> Result<[u8; N], AbiFault> {
    let (high, low) = word.split_at(WORD - N);
    if high.iter().any(|b| *b != 0) {
        return Err(AbiFault::Range);
    }
    low.try_into().map_err(|_| AbiFault::Range)
}

fn word_address(word: &[u8; WORD]) -> Result<Address, AbiFault> {
    word_low::<ADDRESS>(word).map(Address::new)
}

fn word_u8(word: &[u8; WORD]) -> Result<u8, AbiFault> {
    word_low::<1>(word).map(|[b]| b)
}

/// A `bool`, or a `uint8` flag: exactly 0 or 1.
fn word_flag(word: &[u8; WORD]) -> Result<bool, AbiFault> {
    match word_u8(word)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(AbiFault::Flag),
    }
}

/// A fee in basis points, below the denominator.
fn word_bps(word: &[u8; WORD]) -> Result<u16, AbiFault> {
    let bps = u16::from_be_bytes(word_low::<2>(word)?);
    if bps >= BPS_DENOMINATOR {
        return Err(AbiFault::FeeBps);
    }
    Ok(bps)
}

/// A word as a length or offset that fits `usize`.
fn word_usize(word: &[u8]) -> Option<usize> {
    let word: &[u8; WORD] = word.try_into().ok()?;
    usize::try_from(u64::from_be_bytes(word_low::<8>(word).ok()?)).ok()
}

/// The one `string` return value, in its canonical encoding only: offset
/// one word, the length, the bytes zero-padded to whole words, nothing
/// after them.
fn abi_string(raw: &[u8]) -> Result<String, AbiFault> {
    let head = raw.get(..2 * WORD).ok_or(AbiFault::Length)?;
    let (offset, len) = head.split_at(WORD);
    if word_usize(offset) != Some(WORD) {
        return Err(AbiFault::StringEncoding);
    }
    let len = word_usize(len).ok_or(AbiFault::StringEncoding)?;
    let padded = len
        .div_ceil(WORD)
        .checked_mul(WORD)
        .ok_or(AbiFault::StringEncoding)?;
    let body = &raw[2 * WORD..];
    if body.len() != padded {
        return Err(AbiFault::StringEncoding);
    }
    let (text, padding) = body.split_at(len);
    if padding.iter().any(|b| *b != 0) {
        return Err(AbiFault::StringEncoding);
    }
    String::from_utf8(text.to_vec()).map_err(|_| AbiFault::Utf8)
}

#[cfg(test)]
mod tests {
    //! The probe against a scripted local HTTP/1.1 JSON-RPC node (plain
    //! TCP: the production HTTP client and this module's wire code, only
    //! without the TLS layer), and once against the free public endpoint.

    use super::*;
    use crate::wss::RateLimitSignal;
    use alloy_primitives::U256;
    use darkbot_bsc_trading::economics::flap::native_curve20::WITNESSED_MAINNET_TERMS;
    use darkbot_bsc_trading::venues::flap::deployment::{DeploymentError, bsc_mainnet_v5_27_2};
    use serde_json::json;
    use std::io::{self, BufRead, BufReader};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    const HEAD: u64 = 129_849_511;
    const HASH: [u8; 32] = [0xe3; 32];
    const PROXY_CODE: &[u8] = &[0x60, 0x80, 0x60, 0x40, 0x01];
    const IMPLEMENTATION_CODE: &[u8] = &[0x60, 0x80, 0x60, 0x40, 0x02];
    const TAX_CODE: &[u8] = &[0x60, 0x80, 0x60, 0x40, 0x03];
    /// The node's answer to a block parameter it does not support.
    const INVALID_PARAMS: i64 = -32602;
    /// The node's answer to a block it does not know.
    const UNKNOWN_BLOCK: i64 = -32000;

    fn caller() -> Address {
        Address::repeat_byte(0xca)
    }

    fn hx(bytes: &[u8]) -> String {
        format!("0x{}", hex::encode(bytes))
    }

    fn q(n: u64) -> String {
        format!("0x{n:x}")
    }

    /// ABI words of small values.
    fn words(values: &[u64]) -> Vec<u8> {
        values.iter().flat_map(|v| u64_word(*v)).collect()
    }

    /// The canonical ABI encoding of one `string`.
    fn encoded_string(text: &str) -> Vec<u8> {
        let mut out = u64_word(WORD as u64).to_vec();
        out.extend_from_slice(&u64_word(text.len() as u64));
        out.extend_from_slice(text.as_bytes());
        out.resize(2 * WORD + text.len().div_ceil(WORD) * WORD, 0);
        out
    }

    fn quote_calldata() -> Vec<u8> {
        let mut calldata = GET_QUOTE_TOKEN_CONFIGURATION_SELECTOR.to_vec();
        calldata.extend_from_slice(&address_word(NATIVE_QUOTE_TOKEN));
        calldata
    }

    /// The deployment whose fingerprints are those of the served codes (the
    /// real constants' codes cannot be served offline).
    fn served_deployment() -> FlapDeployment {
        FlapDeployment {
            portal_runtime_sha256: darkbot_bsc_record::sha256(PROXY_CODE),
            portal_implementation_runtime_sha256: darkbot_bsc_record::sha256(IMPLEMENTATION_CODE),
            tax_v3_implementation_runtime_sha256: darkbot_bsc_record::sha256(TAX_CODE),
            ..bsc_mainnet_v5_27_2()
        }
    }

    fn is_state_read(method: &str) -> bool {
        matches!(
            method,
            "eth_getCode" | "eth_getStorageAt" | "eth_call" | "eth_getBalance"
        )
    }

    /// A one-shot answer to the first request of a method whose params
    /// contain a needle.
    enum Fault {
        Rpc(i64),
        Result(Value),
    }

    /// A scripted node at one canonical block.
    struct Node {
        chain_id: u64,
        /// The hash at `HEAD` for the pin and for every later read of it.
        hashes: [[u8; 32]; 2],
        codes: Vec<(Address, Vec<u8>)>,
        storage: Vec<(Address, [u8; 32], [u8; 32])>,
        calls: Vec<(Address, Vec<u8>, Vec<u8>)>,
        fault: Option<(&'static str, String, Fault)>,
        /// Answers state reads only at a tag: a block-hash object is
        /// invalid params.
        refuse_pin: bool,
        /// The caller's balance (wei, at most u64 here).
        balance: u64,
        blocks_served: usize,
    }

    impl Node {
        /// The current mainnet venue with the served codes and the
        /// witnessed terms.
        fn mainnet() -> Self {
            let d = bsc_mainnet_v5_27_2();
            Self {
                chain_id: d.chain_id,
                hashes: [HASH; 2],
                codes: vec![
                    (d.portal, PROXY_CODE.to_vec()),
                    (d.portal_implementation, IMPLEMENTATION_CODE.to_vec()),
                    (d.tax_v3_implementation, TAX_CODE.to_vec()),
                ],
                storage: vec![
                    (
                        d.portal,
                        EIP1967_IMPLEMENTATION_SLOT,
                        address_word(d.portal_implementation),
                    ),
                    (d.portal, fee_exemption_key(caller()), [0; 32]),
                ],
                calls: vec![
                    (
                        d.portal,
                        VERSION_SELECTOR.to_vec(),
                        encoded_string("v5.27.2"),
                    ),
                    (d.portal, quote_calldata(), words(&[1, 20, 20, 0, 0])),
                    (d.portal, GET_FEE_RATE_SELECTOR.to_vec(), words(&[100, 100])),
                    (
                        d.portal,
                        ENABLE_TAX_ON_BONDING_CURVE_SELECTOR.to_vec(),
                        words(&[1]),
                    ),
                ],
                fault: None,
                refuse_pin: false,
                balance: 0,
                blocks_served: 0,
            }
        }

        fn set_storage(&mut self, address: Address, key: [u8; 32], value: [u8; 32]) {
            self.storage.retain(|(a, k, _)| (*a, *k) != (address, key));
            self.storage.push((address, key, value));
        }

        fn set_call(&mut self, to: Address, calldata: Vec<u8>, returned: Vec<u8>) {
            self.calls.retain(|(t, c, _)| (*t, c) != (to, &calldata));
            self.calls.push((to, calldata, returned));
        }

        fn answer(&mut self, method: &str, params: &Value) -> Result<Value, i64> {
            if is_state_read(method) {
                let at = params.as_array().unwrap().last().unwrap();
                if self.refuse_pin && at.is_object() {
                    return Err(INVALID_PARAMS);
                }
                let pin = json!({"blockHash": hx(&self.hashes[0]), "requireCanonical": true});
                if *at != pin {
                    return Err(UNKNOWN_BLOCK);
                }
            }
            let address = |v: &Value| v.as_str().unwrap().parse::<Address>().unwrap();
            let bytes = |v: &Value| hex::decode(&v.as_str().unwrap()[2..]).unwrap();
            match method {
                "eth_chainId" => Ok(json!(q(self.chain_id))),
                "eth_blockNumber" => Ok(json!(q(HEAD))),
                "eth_getBlockByNumber" => {
                    assert_eq!(params[1], false, "hashes only");
                    if params[0] != json!(q(HEAD)) {
                        return Ok(Value::Null);
                    }
                    let hash = self.hashes[self.blocks_served.min(1)];
                    self.blocks_served += 1;
                    Ok(json!({"number": q(HEAD), "hash": hx(&hash), "parentHash": hx(&[0x11; 32])}))
                }
                "eth_getCode" => {
                    let at = address(&params[0]);
                    let code = self.codes.iter().find(|(a, _)| *a == at);
                    Ok(json!(hx(code.map_or(&[][..], |(_, c)| c))))
                }
                "eth_getBalance" => Ok(json!(q(self.balance))),
                "eth_getStorageAt" => {
                    let (at, key) = (address(&params[0]), bytes(&params[1]));
                    let value = self
                        .storage
                        .iter()
                        .find(|(a, k, _)| *a == at && k[..] == key[..])
                        .map_or([0; 32], |(_, _, v)| *v);
                    Ok(json!(hx(&value)))
                }
                "eth_call" => {
                    let (to, data) = (address(&params[0]["to"]), bytes(&params[0]["data"]));
                    self.calls
                        .iter()
                        .find(|(t, c, _)| *t == to && *c == data)
                        .map(|(_, _, r)| json!(hx(r)))
                        .ok_or(UNKNOWN_BLOCK)
                }
                other => panic!("unexpected method {other}"),
            }
        }

        /// Serves one connection until the client leaves or the test ends;
        /// every request is recorded.
        fn connection(
            &mut self,
            tcp: TcpStream,
            done: &AtomicBool,
            seen: &mut Vec<(String, Value)>,
        ) {
            let mut reader = BufReader::new(tcp.try_clone().unwrap());
            let mut writer = tcp;
            while let Some(body) = read_request(&mut reader, done) {
                let request: Value = serde_json::from_slice(&body).unwrap();
                let id = request["id"].clone();
                let method = request["method"].as_str().unwrap().to_owned();
                let params = request["params"].clone();
                seen.push((method.clone(), params.clone()));
                let faulted = self.fault.as_ref().is_some_and(|(m, needle, _)| {
                    *m == method && params.to_string().contains(needle.as_str())
                });
                let answer = match self.fault.take_if(|_| faulted) {
                    Some((_, _, Fault::Rpc(code))) => Err(code),
                    Some((_, _, Fault::Result(result))) => Ok(result),
                    None => self.answer(&method, &params),
                };
                let message = match answer {
                    Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                    Err(code) => json!({"jsonrpc": "2.0", "id": id,
                        "error": {"code": code, "message": "scripted"}}),
                };
                let body = message.to_string();
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                let _ = writer.write_all([head.as_bytes(), body.as_bytes()].concat().as_slice());
            }
        }
    }

    /// One request's body; `None` when the client left or the test ended.
    fn read_request(reader: &mut BufReader<TcpStream>, done: &AtomicBool) -> Option<Vec<u8>> {
        let waiting = |e: &io::Error| {
            matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) && !done.load(Ordering::Acquire)
        };
        let mut length = None;
        let mut first = true;
        loop {
            let mut line = Vec::new();
            loop {
                match reader.read_until(b'\n', &mut line) {
                    Ok(_) => break,
                    Err(e) if waiting(&e) => {}
                    Err(_) => return None,
                }
            }
            if !line.ends_with(b"\n") {
                return None;
            }
            if first {
                assert!(line.starts_with(b"POST /rpc HTTP/1.1\r\n"));
                first = false;
                continue;
            }
            if line == b"\r\n" {
                break;
            }
            let text = String::from_utf8(line).unwrap().to_ascii_lowercase();
            if let Some(value) = text.strip_prefix("content-length:") {
                length = Some(value.trim().parse::<usize>().unwrap());
            }
        }
        let mut body = vec![0; length.unwrap()];
        let mut got = 0;
        while got < body.len() {
            match reader.read(&mut body[got..]) {
                Ok(0) => return None,
                Ok(n) => got += n,
                Err(e) if waiting(&e) => {}
                Err(_) => return None,
            }
        }
        Some(body)
    }

    struct Server {
        addr: SocketAddr,
        done: Arc<AtomicBool>,
        handle: JoinHandle<Vec<(String, Value)>>,
    }

    fn serve(mut node: Node) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let handle = {
            let done = Arc::clone(&done);
            thread::spawn(move || {
                let mut seen = Vec::new();
                while !done.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((tcp, _)) => {
                            tcp.set_nonblocking(false).unwrap();
                            tcp.set_read_timeout(Some(Duration::from_millis(20)))
                                .unwrap();
                            node.connection(tcp, &done, &mut seen);
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(e) => panic!("accept: {e}"),
                    }
                }
                seen
            })
        };
        Server { addr, done, handle }
    }

    fn limits() -> FollowerLimits {
        FollowerLimits {
            poll: Duration::from_millis(20),
            reorg_depth: 3,
            max_catch_up: 5,
            connect_timeout: Duration::from_secs(1),
            handshake_timeout: Duration::from_secs(1),
            read_tick: Duration::from_millis(5),
            write_timeout: Duration::from_secs(1),
            request_timeout: Duration::from_secs(5),
            max_response_bytes: 1 << 20,
            backoff_initial: Duration::from_millis(10),
            backoff_max: Duration::from_millis(40),
            tx_attempts: 2,
            max_launch_calldata: 16_384,
            failover_after: 2,
            stall_after: Duration::from_secs(60),
        }
    }

    /// The plain TCP connect of the tests, with the production socket bounds.
    fn connector(
        addr: SocketAddr,
        limits: FollowerLimits,
    ) -> impl FnMut(&AtomicBool) -> Result<TcpStream, DisconnectReason> {
        move |_stop: &AtomicBool| {
            let io = |e: io::Error| DisconnectReason::Io(e.kind());
            let tcp = TcpStream::connect_timeout(&addr, limits.connect_timeout).map_err(io)?;
            tcp.set_nodelay(true).map_err(io)?;
            tcp.set_read_timeout(Some(limits.read_tick)).map_err(io)?;
            tcp.set_write_timeout(Some(limits.write_timeout))
                .map_err(io)?;
            Ok(tcp)
        }
    }

    /// Runs the probe of `deployment` for [`caller`] against `node`; its
    /// result and every request the node saw, in order.
    fn run(
        node: Node,
        deployment: &FlapDeployment,
    ) -> (Result<VenueObservation, ProbeError>, Vec<(String, Value)>) {
        let server = serve(node);
        let limits = limits();
        let mut client = Client::new(
            connector(server.addr, limits),
            server.addr.to_string(),
            "/rpc".to_owned(),
            &limits,
        );
        let result = probe_with(&mut client, deployment, caller());
        drop(client);
        server.done.store(true, Ordering::Release);
        (result, server.handle.join().unwrap())
    }

    /// REV.1.0 R5: the quote's selector is its signature's; its static
    /// tuple inline: the token, native, the amount.
    #[test]
    fn the_quote_call_is_its_signature_and_its_static_tuple() {
        assert_eq!(
            alloy_primitives::keccak256(b"quoteExactInput((address,address,uint256))")[..4],
            QUOTE_EXACT_INPUT_SELECTOR
        );
        let token = Address::repeat_byte(0xab);
        let call = dex_quote_calldata(token, U256::from(7u8));
        assert_eq!(call.len(), 4 + 3 * WORD);
        assert_eq!(&call[4 + 12..4 + WORD], token.as_slice());
        assert_eq!(call[4 + WORD..4 + 2 * WORD], [0u8; WORD]);
        assert_eq!(U256::from_be_slice(&call[4 + 2 * WORD..]), U256::from(7u8));
    }

    /// REV.1.0 R5: a migrated token's SELL quote is the Portal's
    /// `quoteExactInput` answer at the pinned block; a reorg between the
    /// reads or a short answer refuse it.
    #[test]
    fn a_migrated_tokens_quote_is_read_at_its_block() {
        let d = bsc_mainnet_v5_27_2();
        let token = Address::repeat_byte(0x77);
        let amount = U256::from(5_000u64);
        let calldata = dex_quote_calldata(token, amount);
        let node = |answer: Vec<u8>| {
            let mut n = Node::mainnet();
            n.set_call(d.portal, calldata.clone(), answer);
            n
        };
        let read = |n: Node| {
            let server = serve(n);
            let limits = limits();
            let mut client = Client::new(
                connector(server.addr, limits),
                server.addr.to_string(),
                "/rpc".to_owned(),
                &limits,
            );
            let result = quote_with(&mut client, d.portal, token, amount, HEAD, HASH);
            drop(client);
            server.done.store(true, Ordering::Release);
            (result, server.handle.join().unwrap())
        };
        let word = U256::from(123_456u64).to_be_bytes::<32>().to_vec();
        let (result, seen) = read(node(word.clone()));
        assert_eq!(
            result.unwrap(),
            DexQuote {
                block_number: HEAD,
                block_hash: HASH,
                output: U256::from(123_456u64),
            }
        );
        let methods: Vec<&str> = seen.iter().map(|(m, _)| m.as_str()).collect();
        assert_eq!(methods, ["eth_call", "eth_getBlockByNumber"]);
        let mut moved = node(word);
        moved.hashes = [HASH, [0x44; 32]];
        moved.blocks_served = 1;
        assert_eq!(
            read(moved).0.unwrap_err(),
            ProbeError::Reorg { number: HEAD }
        );
        assert!(matches!(
            read(node(vec![0u8; 16])).0,
            Err(ProbeError::Abi {
                read: ProbeRead::DexQuote,
                ..
            })
        ));
    }

    /// 01.11. (R1): a held token's curve is read at its pinned block into
    /// the production terms (`TokenBook::seeded` takes them); off the curve
    /// it reads as not tradable; a reorg between the reads, a tax outside
    /// its range or a short answer refuse it, never another block's state.
    #[test]
    fn a_held_tokens_curve_is_read_at_its_block() {
        use darkbot_bsc_trading::economics::flap::native_curve20::curve20;
        let d = bsc_mainnet_v5_27_2();
        let token = Address::repeat_byte(0x77);
        let gate = Address::repeat_byte(0x99);
        let curve = curve20();
        let state = |status: u64, buy_tax: u64, words: usize| {
            let big = |v: U256| v.to_be_bytes::<32>().to_vec();
            let mut out = Vec::new();
            for w in [
                u64_word(status).to_vec(),
                big(U256::from(3_000_000_000_000_000_000u128)),
                big(U256::from(100_000_000_000_000_000_000_000u128)),
                u64_word(7).to_vec(),
                u64_word(6).to_vec(),
                big(curve.r),
                big(curve.h),
                big(curve.k),
                big(U256::from(800_000_000_000_000_000_000_000_000u128)),
                address_word(Address::ZERO).to_vec(),
                u64_word(0).to_vec(),
                [0u8; 32].to_vec(),
                u64_word(buy_tax).to_vec(),
                u64_word(100).to_vec(),
                address_word(Address::ZERO).to_vec(),
                u64_word(40).to_vec(),
                u64_word(0).to_vec(),
                u64_word(0).to_vec(),
            ]
            .into_iter()
            .take(words)
            {
                out.extend(w);
            }
            out
        };
        let call = |selector: [u8; 4]| {
            let mut c = selector.to_vec();
            c.extend_from_slice(&address_word(token));
            c
        };
        let node = |state: Vec<u8>| {
            let mut n = Node::mainnet();
            n.set_call(d.portal, call(GET_TOKEN_V8_SAFE_SELECTOR), state);
            n.set_call(
                d.portal,
                call(NFT_GATE_SELECTOR),
                address_word(gate).to_vec(),
            );
            n
        };
        let read = |n: Node| {
            let server = serve(n);
            let limits = limits();
            let mut client = Client::new(
                connector(server.addr, limits),
                server.addr.to_string(),
                "/rpc".to_owned(),
                &limits,
            );
            let result = curve_with(&mut client, d.portal, token, HEAD, HASH);
            drop(client);
            server.done.store(true, Ordering::Release);
            (result, server.handle.join().unwrap())
        };
        let (result, seen) = read(node(state(1, 100, 18)));
        let c = result.unwrap();
        assert_eq!((c.block_number, c.block_hash), (HEAD, HASH));
        assert!(c.terms.tradable);
        assert_eq!(c.terms.version, 6);
        assert_eq!(c.terms.curve, curve);
        assert_eq!((c.terms.buy_tax_bps, c.terms.sell_tax_bps), (100, 100));
        assert_eq!(c.terms.nft_gate, gate.into_array());
        assert_eq!(c.reserve, U256::from(3_000_000_000_000_000_000u128));
        assert_eq!(
            c.circulating,
            U256::from(100_000_000_000_000_000_000_000u128)
        );
        let methods: Vec<&str> = seen.iter().map(|(m, _)| m.as_str()).collect();
        assert_eq!(methods, ["eth_call", "eth_call", "eth_getBlockByNumber"]);
        assert!(!c.dex_listed);
        // Migrated: off the curve, listed on its DEX (R5).
        let migrated = read(node(state(4, 100, 18))).0.unwrap();
        assert!(!migrated.terms.tradable && migrated.dex_listed);
        // A reorg between the reads.
        let mut moved = node(state(1, 100, 18));
        moved.hashes = [HASH, [0x44; 32]];
        moved.blocks_served = 1;
        assert_eq!(
            read(moved).0.unwrap_err(),
            ProbeError::Reorg { number: HEAD }
        );
        // A tax at the denominator, a short answer.
        assert!(matches!(
            read(node(state(1, 10_000, 18))).0,
            Err(ProbeError::Abi {
                read: ProbeRead::TokenState,
                ..
            })
        ));
        assert!(matches!(
            read(node(state(1, 100, 17))).0,
            Err(ProbeError::Abi {
                read: ProbeRead::TokenState,
                fault: AbiFault::Length
            })
        ));
    }

    #[test]
    fn a_matching_deployment_validates_with_the_witnessed_terms() {
        let deployment = served_deployment();
        let (result, seen) = run(Node::mainnet(), &deployment);
        let observation = result.unwrap();
        assert_eq!(
            (
                observation.block_number,
                observation.block_hash,
                observation.chain_id
            ),
            (HEAD, HASH, 56)
        );
        let validated = deployment.validate(&observation.deployment(), 7).unwrap();
        assert_eq!(validated.codehash_generation(), 7);
        assert_eq!(observation.terms, WITNESSED_MAINNET_TERMS);
        assert_eq!(observation.observed.portal_version, "v5.27.2");
        assert_eq!(
            observation.observed.portal_code,
            CodeFingerprint {
                sha256: darkbot_bsc_record::sha256(PROXY_CODE),
                keccak256: keccak256(PROXY_CODE),
                len: PROXY_CODE.len(),
            }
        );
        // Each read once, in order; the caller's own exemption entry.
        let methods: Vec<&str> = seen.iter().map(|(m, _)| m.as_str()).collect();
        assert_eq!(
            methods,
            [
                "eth_chainId",
                "eth_blockNumber",
                "eth_getBlockByNumber",
                "eth_getCode",
                "eth_getStorageAt",
                "eth_getCode",
                "eth_call",
                "eth_getCode",
                "eth_call",
                "eth_call",
                "eth_call",
                "eth_getStorageAt",
                "eth_getBalance",
                "eth_getBlockByNumber",
            ]
        );
        assert_eq!(seen[11].1[1], json!(hx(&fee_exemption_key(caller()))));
        // The caller's balance, read pinned: the PAPER sender holds nothing.
        assert_eq!(seen[12].1[0], json!(format!("0x{}", hex::encode(caller()))));
        assert_eq!(observation.caller_balance_wei, U256::ZERO);
        let mut funded = Node::mainnet();
        funded.balance = 7;
        let (result, _) = run(funded, &deployment);
        assert_eq!(result.unwrap().caller_balance_wei, U256::from(7u64));
    }

    #[test]
    fn a_version_drift_is_observed_and_refused_by_validate() {
        let deployment = served_deployment();
        // A newer version, and the replaced v5.27.0 (BSC3-065: the old pin's
        // version no longer passes the current one).
        for version in ["v5.28.0", "v5.27.0"] {
            let mut node = Node::mainnet();
            node.set_call(
                deployment.portal,
                VERSION_SELECTOR.to_vec(),
                encoded_string(version),
            );
            let observation = run(node, &deployment).0.unwrap();
            assert_eq!(observation.observed.portal_version, version);
            assert_eq!(
                deployment.validate(&observation.deployment(), 1),
                Err(DeploymentError::PortalVersion)
            );
        }
    }

    #[test]
    fn an_implementation_change_is_observed_and_refused_by_validate() {
        let deployment = served_deployment();
        let upgraded = Address::repeat_byte(0x5a);
        let upgraded_code = [0x60, 0x80, 0x60, 0x40, 0x09];
        let mut node = Node::mainnet();
        node.set_storage(
            deployment.portal,
            EIP1967_IMPLEMENTATION_SLOT,
            address_word(upgraded),
        );
        node.codes.push((upgraded, upgraded_code.to_vec()));
        let (result, seen) = run(node, &deployment);
        let observation = result.unwrap();
        assert_eq!(observation.observed.portal_implementation, upgraded);
        assert_eq!(
            observation.observed.portal_implementation_code.sha256,
            darkbot_bsc_record::sha256(&upgraded_code)
        );
        // The code read is the slot's implementation, not the witnessed one.
        assert!(
            seen.iter()
                .any(|(m, p)| m == "eth_getCode" && p[0] == json!(hx(upgraded.as_slice())))
        );
        assert_eq!(
            deployment.validate(&observation.deployment(), 1),
            Err(DeploymentError::PortalImplementation)
        );
    }

    #[test]
    fn a_wrong_chain_fails_before_any_other_read() {
        let mut node = Node::mainnet();
        node.chain_id = 97;
        let (result, seen) = run(node, &served_deployment());
        assert_eq!(
            result,
            Err(ProbeError::WrongChain {
                expected: 56,
                observed: 97
            })
        );
        assert_eq!(seen.len(), 1);
    }

    #[test]
    fn an_rpc_error_fails_closed_and_nothing_is_read_after_it() {
        let mut node = Node::mainnet();
        node.fault = Some((
            "eth_call",
            hex::encode(GET_FEE_RATE_SELECTOR),
            Fault::Rpc(-32000),
        ));
        let (result, seen) = run(node, &served_deployment());
        assert_eq!(
            result,
            Err(ProbeError::Request {
                read: ProbeRead::FeeRate,
                error: RequestError::Rpc(-32000)
            })
        );
        assert_eq!(seen.len(), 10);

        let mut node = Node::mainnet();
        node.fault = Some((
            "eth_getStorageAt",
            hex::encode(EIP1967_IMPLEMENTATION_SLOT),
            Fault::Rpc(-32005),
        ));
        assert_eq!(
            run(node, &served_deployment()).0,
            Err(ProbeError::Request {
                read: ProbeRead::ImplementationSlot,
                error: RequestError::RateLimited(RateLimitSignal::RpcError(-32005))
            })
        );
    }

    #[test]
    fn malformed_hex_fails_closed() {
        let d = bsc_mainnet_v5_27_2();
        for (method, needle, bad, read) in [
            (
                "eth_chainId",
                String::new(),
                json!("0x038"),
                ProbeRead::ChainId,
            ),
            (
                "eth_getCode",
                hex::encode(d.portal),
                json!("0x60g0"),
                ProbeRead::PortalCode,
            ),
            (
                "eth_getCode",
                hex::encode(d.tax_v3_implementation),
                json!("0x608"),
                ProbeRead::TaxImplementationCode,
            ),
            (
                "eth_getStorageAt",
                hex::encode(EIP1967_IMPLEMENTATION_SLOT),
                json!(hx(&[0; 31])),
                ProbeRead::ImplementationSlot,
            ),
            (
                "eth_call",
                hex::encode(VERSION_SELECTOR),
                json!("0X00"),
                ProbeRead::Version,
            ),
            (
                "eth_call",
                hex::encode(ENABLE_TAX_ON_BONDING_CURVE_SELECTOR),
                json!(1),
                ProbeRead::TaxOnBondingCurve,
            ),
        ] {
            let mut node = Node::mainnet();
            node.fault = Some((method, needle, Fault::Result(bad)));
            assert_eq!(
                run(node, &served_deployment()).0,
                Err(ProbeError::Request {
                    read,
                    error: RequestError::Shape(Field::Result)
                }),
                "{read:?}"
            );
        }
    }

    #[test]
    fn values_outside_their_abi_or_storage_type_fail_closed() {
        let d = bsc_mainnet_v5_27_2();
        let mut padded = encoded_string("v5.27.2");
        *padded.last_mut().unwrap() = 1;
        let mut high = address_word(d.portal_implementation);
        high[0] = 1;
        type Edit = Box<dyn Fn(&mut Node)>;
        let cases: [(Edit, ProbeRead, AbiFault); 7] = [
            (
                Box::new(move |n| n.set_call(d.portal, VERSION_SELECTOR.to_vec(), padded.clone())),
                ProbeRead::Version,
                AbiFault::StringEncoding,
            ),
            (
                Box::new(move |n| n.set_storage(d.portal, EIP1967_IMPLEMENTATION_SLOT, high)),
                ProbeRead::ImplementationSlot,
                AbiFault::Range,
            ),
            (
                Box::new(move |n| n.set_storage(d.portal, EIP1967_IMPLEMENTATION_SLOT, [0; 32])),
                ProbeRead::ImplementationSlot,
                AbiFault::ZeroAddress,
            ),
            (
                Box::new(move |n| {
                    n.set_call(d.portal, quote_calldata(), words(&[2, 20, 20, 0, 0]))
                }),
                ProbeRead::QuoteConfiguration,
                AbiFault::Flag,
            ),
            (
                Box::new(move |n| {
                    n.set_call(
                        d.portal,
                        GET_FEE_RATE_SELECTOR.to_vec(),
                        words(&[10_000, 100]),
                    )
                }),
                ProbeRead::FeeRate,
                AbiFault::FeeBps,
            ),
            (
                Box::new(move |n| {
                    n.set_call(
                        d.portal,
                        GET_FEE_RATE_SELECTOR.to_vec(),
                        words(&[100, 100, 0]),
                    )
                }),
                ProbeRead::FeeRate,
                AbiFault::Length,
            ),
            (
                Box::new(move |n| {
                    n.set_storage(d.portal, fee_exemption_key(caller()), u64_word(2))
                }),
                ProbeRead::FeeExemption,
                AbiFault::Flag,
            ),
        ];
        for (edit, read, fault) in cases {
            let mut node = Node::mainnet();
            edit(&mut node);
            assert_eq!(
                run(node, &served_deployment()).0,
                Err(ProbeError::Abi { read, fault }),
                "{read:?}"
            );
        }
        // An exempt caller is observed, not refused: the terms differ.
        let mut node = Node::mainnet();
        node.set_storage(d.portal, fee_exemption_key(caller()), u64_word(1));
        let terms = run(node, &served_deployment()).0.unwrap().terms;
        assert!(terms.caller_fee_exempt);
        assert_ne!(terms, WITNESSED_MAINNET_TERMS);
    }

    #[test]
    fn every_state_read_is_pinned_and_nothing_falls_back_to_latest() {
        let (result, seen) = run(Node::mainnet(), &served_deployment());
        result.unwrap();
        let pin = json!({"blockHash": hx(&HASH), "requireCanonical": true});
        let state: Vec<_> = seen.iter().filter(|(m, _)| is_state_read(m)).collect();
        assert_eq!(state.len(), 10);
        for (method, params) in state {
            assert_eq!(params.as_array().unwrap().last().unwrap(), &pin, "{method}");
        }
        assert!(seen.iter().all(|(_, p)| !p.to_string().contains("latest")));

        // A node that refuses the pinned parameter fails the probe at the
        // first state read; nothing is asked again at a tag.
        let mut node = Node::mainnet();
        node.refuse_pin = true;
        let (result, seen) = run(node, &served_deployment());
        assert_eq!(
            result,
            Err(ProbeError::Request {
                read: ProbeRead::PortalCode,
                error: RequestError::Rpc(INVALID_PARAMS)
            })
        );
        assert_eq!(seen.len(), 4);
        assert!(seen.iter().all(|(_, p)| !p.to_string().contains("latest")));
    }

    #[test]
    fn a_reorg_during_the_reads_or_a_missing_code_fails_closed() {
        let mut node = Node::mainnet();
        node.hashes[1] = [0xe4; 32];
        assert_eq!(
            run(node, &served_deployment()).0,
            Err(ProbeError::Reorg { number: HEAD })
        );

        let tax = bsc_mainnet_v5_27_2().tax_v3_implementation;
        let mut node = Node::mainnet();
        node.codes.retain(|(a, _)| *a != tax);
        assert_eq!(
            run(node, &served_deployment()).0,
            Err(ProbeError::NoCode(ProbeRead::TaxImplementationCode))
        );
    }

    #[test]
    fn only_the_canonical_abi_string_is_accepted() {
        let good = encoded_string("v5.27.2");
        assert_eq!(abi_string(&good).unwrap(), "v5.27.2");
        assert_eq!(abi_string(&encoded_string("")).unwrap(), "");
        let mut offset = good.clone();
        offset[WORD - 1] = 0x40;
        let mut trailing = good.clone();
        trailing.extend_from_slice(&[0; WORD]);
        let mut long = good.clone();
        long[2 * WORD - 1] = 0x21;
        let mut huge = good.clone();
        huge[WORD] = 0xff;
        let mut not_utf8 = good.clone();
        not_utf8[2 * WORD] = 0xff;
        for (raw, fault) in [
            (&good[..WORD], AbiFault::Length),
            (&offset[..], AbiFault::StringEncoding),
            (&trailing[..], AbiFault::StringEncoding),
            (&long[..], AbiFault::StringEncoding),
            (&huge[..], AbiFault::StringEncoding),
            (&good[..good.len() - 1], AbiFault::StringEncoding),
            (&not_utf8[..], AbiFault::Utf8),
        ] {
            assert_eq!(abi_string(raw), Err(fault), "{}", hex::encode(raw));
        }
    }

    #[test]
    fn the_frozen_selectors_and_slots_are_their_derivations() {
        for (selector, signature) in [
            (VERSION_SELECTOR, "version()"),
            (
                GET_QUOTE_TOKEN_CONFIGURATION_SELECTOR,
                "getQuoteTokenConfiguration(address)",
            ),
            (GET_FEE_RATE_SELECTOR, "getFeeRate()"),
            (
                ENABLE_TAX_ON_BONDING_CURVE_SELECTOR,
                "enableTaxOnBondingCurve()",
            ),
            (GET_TOKEN_V8_SAFE_SELECTOR, "getTokenV8Safe(address)"),
            (NFT_GATE_SELECTOR, "nftGate(address)"),
        ] {
            assert_eq!(selector[..], keccak256(signature)[..4], "{signature}");
        }
        assert_eq!(
            U256::from_be_bytes(EIP1967_IMPLEMENTATION_SLOT) + U256::from(1u8),
            U256::from_be_bytes(keccak256("eip1967.proxy.implementation").0)
        );
        // The G0.2 archive witness of the fee-exemption entry the v5.24.0
        // BUY path read (`historical_state_decoded.json`
        // `portal_fee_exemption.key`, for the router as caller).
        assert_eq!(
            fee_exemption_key(Address::new(hex!(
                "43dd5eebeebead726fbc84e2511567d9827ae1bb"
            ))),
            hex!("c92fe1d364edee5515a4eb764784e51b24327983c85d50d181173244fcb17247")
        );
    }

    /// The Four deployment whose fingerprints are those of the served codes,
    /// and a node serving it (the frozen addresses, test codes).
    fn served_four() -> (FourDeployment, Node) {
        use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_r12b_131568366;
        let real = bsc_mainnet_r12b_131568366();
        let code = |n: u8| vec![0x60, 0x80, 0x60, 0x40, 0x10 + n];
        let mut d = real;
        d.token_manager2_implementation_runtime_keccak256 = keccak256(code(1));
        d.openfour_core_implementation_runtime_keccak256 = keccak256(code(2));
        d.openfour_create_deployer_implementation_runtime_keccak256 = keccak256(code(3));
        for (i, t) in d.classic_templates.iter_mut().enumerate() {
            t.deployer_runtime_keccak256 = keccak256(code(4 + 2 * i as u8));
            t.template_runtime_keccak256 = keccak256(code(5 + 2 * i as u8));
        }
        let mut node = Node::mainnet();
        node.codes = vec![
            (d.token_manager2_implementation, code(1)),
            (d.openfour_core_implementation, code(2)),
            (d.openfour_create_deployer_implementation, code(3)),
        ];
        for (i, t) in d.classic_templates.iter().enumerate() {
            node.codes.push((t.deployer, code(4 + 2 * i as u8)));
            node.codes.push((t.template, code(5 + 2 * i as u8)));
        }
        node.storage = [
            (d.token_manager2, d.token_manager2_implementation),
            (d.openfour_core, d.openfour_core_implementation),
            (
                d.openfour_create_deployer,
                d.openfour_create_deployer_implementation,
            ),
        ]
        .map(|(proxy, implementation)| {
            (
                proxy,
                EIP1967_IMPLEMENTATION_SLOT,
                address_word(implementation),
            )
        })
        .to_vec();
        (d, node)
    }

    fn run_four(
        node: Node,
        deployment: &FourDeployment,
    ) -> (Result<FourObservation, ProbeError>, Vec<(String, Value)>) {
        let server = serve(node);
        let limits = limits();
        let mut client = Client::new(
            connector(server.addr, limits),
            server.addr.to_string(),
            "/rpc".to_owned(),
            &limits,
        );
        let result = probe_four_with(&mut client, deployment);
        drop(client);
        server.done.store(true, Ordering::Release);
        (result, server.handle.join().unwrap())
    }

    /// Phase E review L3, the Four drift guard: the frozen deployment reads
    /// as frozen; an upgraded TokenManager2 and a changed Classic template
    /// are each named; every read pinned to one block, rechecked at the end.
    #[test]
    fn the_four_drift_guard_names_every_difference() {
        use darkbot_bsc_trading::venues::fourmeme::deployment::FourDrift;
        let (d, node) = served_four();
        let (result, seen) = run_four(node, &d);
        let o = result.unwrap();
        assert_eq!((o.block_number, o.block_hash), (HEAD, HASH));
        assert_eq!(d.drifts(&o.observed), []);
        assert!(d.validate(&o.observed, o.block_number).is_ok());
        let methods: Vec<&str> = seen.iter().map(|(m, _)| m.as_str()).collect();
        assert_eq!(
            methods.iter().filter(|m| **m == "eth_getStorageAt").count(),
            3
        );
        assert_eq!(methods.iter().filter(|m| **m == "eth_getCode").count(), 7);
        assert_eq!(methods.last(), Some(&"eth_getBlockByNumber"));
        let (_, mut node) = served_four();
        let upgraded = Address::repeat_byte(0x99);
        node.set_storage(
            d.token_manager2,
            EIP1967_IMPLEMENTATION_SLOT,
            address_word(upgraded),
        );
        node.codes[6].1 = vec![0x60, 0x80, 0xff];
        node.codes.push((upgraded, vec![0x60, 0x80, 0xaa]));
        let o = run_four(node, &d).0.unwrap();
        // The upgraded implementation, its code and the changed template are
        // named.
        assert_eq!(o.observed.token_manager2_implementation, upgraded);
        assert_eq!(
            d.drifts(&o.observed),
            [
                FourDrift::TokenManager2Implementation,
                FourDrift::TokenManager2Code,
                FourDrift::ClassicPair(1)
            ]
        );
    }

    /// The live Four.meme deployment against the freeze. Read-only; prints
    /// what differs. Run it with `BSC_FREE_ARCHIVE_RPC=<the free archive
    /// endpoint>`
    /// `cargo test -p darkbot-bsc-execution --features runtime --lib venue_probe::tests::live_four -- --ignored --nocapture`.
    #[test]
    #[ignore = "network: read-only probe of the Four.meme contracts at BSC_FREE_ARCHIVE_RPC"]
    fn live_four_deployment_is_the_frozen_one() {
        let rpc = std::env::var("BSC_FREE_ARCHIVE_RPC")
            .expect("BSC_FREE_ARCHIVE_RPC names the free archive endpoint");
        use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_r12b_131568366;
        let limits = FollowerLimits {
            request_timeout: Duration::from_secs(10),
            max_response_bytes: 16 << 20,
            connect_timeout: Duration::from_secs(5),
            handshake_timeout: Duration::from_secs(10),
            read_tick: Duration::from_millis(100),
            ..limits()
        };
        let o = probe_four(&rpc, &limits, &bsc_mainnet_r12b_131568366()).unwrap();
        let drifts = bsc_mainnet_r12b_131568366().drifts(&o.observed);
        println!("four at block {}: drift {drifts:?}", o.block_number);
        assert!(drifts.is_empty());
    }

    /// The current deployment and venue terms on BSC Mainnet. Read-only;
    /// prints what it saw. Run it with `BSC_FREE_ARCHIVE_RPC=<the free
    /// archive endpoint>`
    /// `cargo test -p darkbot-bsc-execution --features runtime --lib venue_probe::tests::live -- --ignored --nocapture`.
    #[test]
    #[ignore = "network: read-only probe of the Flap Portal at BSC_FREE_ARCHIVE_RPC"]
    fn live_probe_of_the_current_deployment() {
        let rpc = std::env::var("BSC_FREE_ARCHIVE_RPC")
            .expect("BSC_FREE_ARCHIVE_RPC names the free archive endpoint");
        let limits = FollowerLimits {
            poll: Duration::from_millis(200),
            reorg_depth: 16,
            max_catch_up: 64,
            connect_timeout: Duration::from_secs(5),
            handshake_timeout: Duration::from_secs(10),
            read_tick: Duration::from_millis(100),
            write_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(10),
            max_response_bytes: 16 << 20,
            backoff_initial: Duration::from_millis(250),
            backoff_max: Duration::from_secs(5),
            tx_attempts: 3,
            max_launch_calldata: 16_384,
            failover_after: 2,
            stall_after: Duration::from_secs(60),
        };
        // A fresh caller nobody can have exempted in advance.
        let entropy = format!("{:?}/{}", std::time::SystemTime::now(), std::process::id());
        let caller = Address::from_slice(&keccak256(entropy.as_bytes())[12..]);
        let deployment = bsc_mainnet_v5_27_2();
        let observation = probe_flap(&rpc, &limits, &deployment, caller).unwrap();
        let o = &observation.observed;
        let code = |c: &CodeFingerprint| {
            format!(
                "sha256 0x{} keccak256 {} ({} bytes)",
                hex::encode(c.sha256),
                c.keccak256,
                c.len
            )
        };
        println!(
            "pinned block {} 0x{} (chain {})",
            observation.block_number,
            hex::encode(observation.block_hash),
            observation.chain_id
        );
        println!("caller {caller}");
        println!("portal version {:?}", o.portal_version);
        println!("portal proxy code {}", code(&o.portal_code));
        println!(
            "portal implementation {} code {}",
            o.portal_implementation,
            code(&o.portal_implementation_code)
        );
        println!(
            "tax-v3 implementation code {}",
            code(&o.tax_v3_implementation_code)
        );
        println!(
            "validate(bsc_mainnet_v5_27_2): {:?}",
            deployment
                .validate(&observation.deployment(), 1)
                .map(|_| "OK")
        );
        let (t, w) = (observation.terms, WITNESSED_MAINNET_TERMS);
        println!("terms == WITNESSED_MAINNET_TERMS: {}", t == w);
        for (name, observed, witnessed) in [
            (
                "native_quote_enabled",
                t.native_quote_enabled.to_string(),
                w.native_quote_enabled.to_string(),
            ),
            (
                "default_curve_id",
                t.default_curve_id.to_string(),
                w.default_curve_id.to_string(),
            ),
            (
                "protocol_buy_fee_bps",
                t.protocol_buy_fee_bps.to_string(),
                w.protocol_buy_fee_bps.to_string(),
            ),
            (
                "tax_on_bonding_curve_enabled",
                t.tax_on_bonding_curve_enabled.to_string(),
                w.tax_on_bonding_curve_enabled.to_string(),
            ),
            (
                "caller_fee_exempt",
                t.caller_fee_exempt.to_string(),
                w.caller_fee_exempt.to_string(),
            ),
        ] {
            let verdict = if observed == witnessed {
                "equal"
            } else {
                "DIFFERS"
            };
            println!("  {name}: observed {observed}, witnessed {witnessed} ({verdict})");
        }
    }
}

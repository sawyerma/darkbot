//! Declarations shared by the G0.7 binary and the G0.7 builder probe. The
//! probe includes the G0.7 modules (framing, TLS, head poller, expiry window,
//! signer) by path, so it exercises exactly the code G0.7 sends with.

pub(crate) use alloy_primitives::{Address, B256, Bytes, U256, keccak256};
pub(crate) use alloy_sol_types::{SolCall, sol};
pub(crate) use darkbot_bsc_execution::{
    acceptance_guard::{InputHashes, V4_RELATIVE_PATH, sha256_hex},
    qualification_capture::CaptureClock,
};
pub(crate) use darkbot_bsc_record::{Draft, Record, schema};
pub(crate) use k256::ecdsa::SigningKey;
pub(crate) use rustls::{
    ClientConfig, ClientConnection, RootCertStore, StreamOwned, pki_types::ServerName,
};
pub(crate) use std::{
    env, fs,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) type Tls = StreamOwned<ClientConnection, TcpStream>;

pub(crate) const CHAIN_ID: u64 = 56;
pub(crate) const PORTAL: &str = "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0";
pub(crate) const CLUB48_HOST: &str = "puissant-builder.48.club";
pub(crate) const BLOCKRAZOR_HOST: &str = "virginia.builder.blockrazor.io";
pub(crate) const HTTPS_PORT: u16 = 443;
pub(crate) const PRODUCER_WITNESS_SCHEMA_PATH: &str =
    "bsc/config/producer_execution_witness_v1.schema.json";

sol! {
    struct LaunchParams {
        string name;
        string symbol;
        string uri;
        uint8 dexThresh;
        bytes32 salt;
        uint8 migratorType;
        address quoteToken;
        uint256 quoteAmt;
        address beneficiary;
        bytes permit;
        bytes32 extensionId;
        bytes extensionData;
        uint8 dexId;
        uint8 lpFeeProfile;
        uint16 buyTax;
        uint16 sellTax;
        uint64 taxDuration;
        uint64 antiFarmerDuration;
        uint16 mktBps;
        uint16 deflationBps;
        uint16 dividendBps;
        uint16 lpBps;
        uint256 minimumShareBalance;
        address dividendToken;
        address commissionReceiver;
        uint8 tokenVersion;
    }

    function newTokenV6(LaunchParams params) external payable returns(address);

    struct SwapParams {
        address inputToken;
        address outputToken;
        uint256 inputAmount;
        uint256 minOutputAmount;
        bytes permit;
    }

    function swapExactInput(SwapParams params) external payable returns(uint256);
}

#[derive(Clone)]
pub(crate) struct SignedLegacy {
    pub(crate) raw: Vec<u8>,
    pub(crate) hash: B256,
}

/// A builder's JSON-RPC answer, decoded at the transport boundary into
/// typed values; the raw body is not kept, only its SHA-256 and length.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BuilderAnswer {
    pub(crate) status_line: String,
    pub(crate) status_code: Option<u16>,
    pub(crate) body_sha256: [u8; 32],
    pub(crate) body_len: u64,
    /// A 32-byte `result` (a bundle hash).
    pub(crate) result_hash: Option<[u8; 32]>,
    /// Any other `result`, as bounded text.
    pub(crate) result_text: Option<String>,
    pub(crate) error_code: Option<i64>,
    pub(crate) error_message: Option<String>,
    /// Why the body could not be decoded as a JSON-RPC response.
    pub(crate) undecodable: Option<String>,
}

#[derive(Debug)]
pub(crate) struct ProviderResult {
    pub(crate) provider: &'static str,
    pub(crate) write_started_ns: Option<u64>,
    pub(crate) write_flushed_ns: Option<u64>,
    pub(crate) bytes_may_have_left: bool,
    pub(crate) answer: Option<BuilderAnswer>,
    pub(crate) error: Option<String>,
}

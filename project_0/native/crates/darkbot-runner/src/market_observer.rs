//! [C0-MO] Unfiltered read-only live market observer (`P0_MODE=market`).
//!
//! Records every Marginfi transaction delivered by the shared `WsRpcControlSource`, decodes
//! successful `LiquidationReceiverEvent`s from the program logs and aggregates realised market
//! gross edge (`seized_equity_usd - repaid_equity_usd`) per receiver.
//!
//! Read-only by construction: this module builds no wallet, no transaction, no health engine,
//! no candidate cache, no planner/routing/funding/economics object and never sends anything.
//! No DarkBot filter (Health, candidate admission, P1-C, F1, F8, routing, funding, profit) is
//! applied before raw evidence is written.
//!
//! Phases (r2):
//! 1. CAPTURE until the cutoff `t_rx(StreamReady) + duration` on the shared monotonic `Clock`.
//!    No RPC call ever runs in this phase.
//! 2. STOP + DRAIN: stop the ingress session, drain the channel; events with `t_rx <= cutoff`
//!    count, later events do not.
//! 3. ENRICHMENT (logs source only): bounded `getTransaction` for observed liquidations.
//!    Enrichment can never remove captured gross edge; its completeness is reported separately.
//!
//! Sources (explicit, never silently switched):
//! - `transactions` (default): Helius `transactionSubscribe` (full tx + meta; Helius
//!   Developer/Business/Professional). No `failed` filter, so successful and failed are delivered.
//! - `logs`: standard `logsSubscribe` with `mentions = [program]` (all plans).
//!
//! For neither source is it independently verified that transactions referencing the program
//! only through an address lookup table are delivered. Until that is proven, every run ends at
//! best as `MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED`; `MARKET_DATA_VALID` requires a verified source.
//!
//! Commitment defaults to `finalized` (market truth, latency irrelevant). The market session
//! sends a transport heartbeat (WebSocket Ping, default every 30 s). A `slotSubscribe` runs
//! alongside purely as slot/stream telemetry (`max_stream_silence_ms`).
//!
//! This measures MARKET EDGE only. It is not DarkBot executable edge, capture rate or PnL.

use crate::winner::{check_transaction_identity, end_liquidation_instructions, RpcTransaction};
use base64::Engine as _;
use darkbot_common::channel::bounded;
use darkbot_common::lifecycle::Shutdown;
use darkbot_common::timing::Clock;
use darkbot_solana::ingress::ws_rpc::{WsRpcConfig, WsRpcControlSource};
use darkbot_solana::ingress::{
    IngressEvent, IngressHalt, IngressSource, LogsObserved, SlotWatch, Subscription,
    TransactionObserved,
};
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[path = "market_gap.rs"]
mod gap;
pub use gap::GapConfig;
#[path = "market_reconcile.rs"]
mod reconcile;
#[path = "market_verification.rs"]
pub mod verification;
pub use reconcile::ReconcileConfig;
#[path = "market_paths.rs"]
mod paths;
#[path = "market_truth.rs"]
pub mod truth;

/// `sha256("event:LiquidationReceiverEvent")[..8]` (verified against the six audit fixtures).
pub const LIQUIDATION_RECEIVER_EVENT_DISCRIMINATOR: [u8; 8] = [40, 131, 224, 220, 151, 83, 24, 230];
/// 8 discriminator + 32 target + 32 receiver + f64 seized + f64 repaid + u32 flat fee.
pub const LIQUIDATION_RECEIVER_EVENT_LEN: usize = 92;
const COMPUTE_BUDGET_PROGRAM: &str = "ComputeBudget111111111111111111111111111111";
const PROGRAM_DATA_PREFIX: &str = "Program data: ";
const QUANTILE_METHOD: &str =
    "nearest-rank: value at index ceil(q*n)-1 of the ascending sort (median = lower median)";

type EnrichmentFetcher<'a> =
    Result<&'a mut dyn FnMut(&str) -> Result<Option<Value>, String>, String>;

// ------------------------------------------------------------------------------------------
// Configuration
// ------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketSource {
    Transactions,
    Logs,
}

impl MarketSource {
    pub fn as_str(self) -> &'static str {
        match self {
            MarketSource::Transactions => "transactions",
            MarketSource::Logs => "logs",
        }
    }

    /// Whether delivery of *all* program transactions (including references only via an address
    /// lookup table) is proven for this source. No source is proven today; this may only become
    /// `true` with an authoritative provider guarantee or a reproducible LUT-only delivery test.
    pub fn coverage_verified(self) -> bool {
        match self {
            MarketSource::Transactions | MarketSource::Logs => false,
        }
    }

    pub fn coverage_note(self) -> &'static str {
        match self {
            MarketSource::Transactions => {
                "transactionSubscribe accountInclude: delivery of transactions that reference the program only via an address lookup table is NOT independently verified (no failed/vote filter set)"
            }
            MarketSource::Logs => {
                "logsSubscribe mentions: delivery of transactions that reference the program only via an address lookup table is NOT verified"
            }
        }
    }
}

/// Bounded post-capture `getTransaction` enrichment (logs source only). PROVISIONAL defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnrichConfig {
    pub max_pending: usize,
    pub max_attempts: u32,
    pub retry_ms: u64,
    pub timeout_ms: u64,
    pub phase_max_secs: u64,
}

impl EnrichConfig {
    pub const DEFAULT: Self = Self {
        max_pending: 4_096,
        max_attempts: 5,
        retry_ms: 2_000,
        timeout_ms: 10_000,
        phase_max_secs: 900,
    };
}

#[derive(Clone, Debug)]
pub struct MarketConfig {
    pub ws_url: String,
    pub rpc_url: Option<String>,
    pub program: Pubkey,
    pub source: MarketSource,
    pub commitment: &'static str,
    pub duration_secs: u64,
    pub ready_timeout_secs: u64,
    pub drain_timeout_secs: u64,
    pub heartbeat_secs: u64,
    /// Below this many liquidation events the result is `INSUFFICIENT_SAMPLE`.
    pub min_sample_liquidations: u64,
    pub ingress_buffer: usize,
    pub run_id: String,
    pub run_dir: PathBuf,
    pub enrich: EnrichConfig,
    pub gap: GapConfig,
    pub reconcile: ReconcileConfig,
}

pub const DEFAULT_DURATION_SECS: u64 = 14_400;
pub const DEFAULT_READY_TIMEOUT_SECS: u64 = 120;
/// Must exceed the ingress idle timeout (60 s) plus its maximum reconnect backoff (30 s).
pub const DEFAULT_DRAIN_TIMEOUT_SECS: u64 = 120;
pub const DEFAULT_INGRESS_BUFFER: usize = 65_536;
/// Transport heartbeat interval; must stay below the ingress idle timeout.
pub const DEFAULT_HEARTBEAT_SECS: u64 = 30;
/// PROVISIONAL minimum number of liquidation events for distribution statistics.
pub const DEFAULT_MIN_SAMPLE_LIQUIDATIONS: u64 = 30;
/// Helius endpoints used only when `HELIUS_API_KEY` is set and no explicit URL is given.
/// `transactions` uses the Enhanced-WSS endpoint; acceptance of `transactionSubscribe` is never
/// assumed and must be proven by the subscription ACK in the smoke run.
pub const HELIUS_ENHANCED_WSS_MAINNET: &str = "wss://atlas-mainnet.helius-rpc.com/?api-key=";
pub const HELIUS_STANDARD_WSS_MAINNET: &str = "wss://mainnet.helius-rpc.com/?api-key=";
pub const HELIUS_RPC_MAINNET: &str = "https://mainnet.helius-rpc.com/?api-key=";
pub const DEFAULT_OUT_DIR: &str = "project_0/data/market";

impl MarketConfig {
    /// Pure parser (testable). Requires `P0_PROGRAM_ID` and either `P0_WS_URL` or `HELIUS_API_KEY`
    /// (explicit URLs always win). `P0_RPC_URL` is optional: without it, `logs` enrichment is
    /// unavailable and reported as such, but capture, summary and report still run.
    /// No keypair or wallet setting is read.
    pub fn from_lookup(
        get: &dyn Fn(&str) -> Option<String>,
        now_unix_s: u64,
    ) -> Result<Self, String> {
        let opt = |k: &str| {
            get(k)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let req = |k: &str| opt(k).ok_or_else(|| format!("missing required env: {k}"));
        let num = |k: &str, default: u64| -> Result<u64, String> {
            match opt(k) {
                Some(v) => v.parse::<u64>().map_err(|_| format!("invalid {k}={v}")),
                None => Ok(default),
            }
        };
        let program_s = req("P0_PROGRAM_ID")?;
        let program =
            Pubkey::from_str(&program_s).map_err(|e| format!("invalid P0_PROGRAM_ID: {e}"))?;
        let source = match opt("P0_MARKET_SOURCE").as_deref() {
            None | Some("transactions") => MarketSource::Transactions,
            Some("logs") => MarketSource::Logs,
            Some(other) => {
                return Err(format!(
                    "invalid P0_MARKET_SOURCE={other} (transactions|logs)"
                ))
            }
        };
        let commitment: &'static str = match opt("P0_MARKET_COMMITMENT").as_deref() {
            None | Some("finalized") => "finalized",
            Some("confirmed") => "confirmed",
            Some(other) => {
                return Err(format!(
                    "invalid P0_MARKET_COMMITMENT={other} (finalized|confirmed)"
                ))
            }
        };
        let helius_key = opt("HELIUS_API_KEY");
        let ws_url = match (opt("P0_WS_URL"), &helius_key) {
            (Some(url), _) => url,
            (None, Some(key)) => match source {
                MarketSource::Transactions => format!("{HELIUS_ENHANCED_WSS_MAINNET}{key}"),
                MarketSource::Logs => format!("{HELIUS_STANDARD_WSS_MAINNET}{key}"),
            },
            (None, None) => {
                return Err("missing required env: P0_WS_URL (or HELIUS_API_KEY)".into())
            }
        };
        let rpc_url = match (opt("P0_RPC_URL"), &helius_key, source) {
            (Some(url), _, _) => Some(url),
            (None, Some(key), MarketSource::Logs) => Some(format!("{HELIUS_RPC_MAINNET}{key}")),
            _ => None,
        };
        let min_sample_liquidations = num("P0_MARKET_MIN_SAMPLE", DEFAULT_MIN_SAMPLE_LIQUIDATIONS)?;
        let duration_secs = num("P0_MARKET_DURATION_SECS", DEFAULT_DURATION_SECS)?;
        let ready_timeout_secs = num("P0_MARKET_READY_TIMEOUT_SECS", DEFAULT_READY_TIMEOUT_SECS)?;
        let drain_timeout_secs = num("P0_MARKET_DRAIN_TIMEOUT_SECS", DEFAULT_DRAIN_TIMEOUT_SECS)?;
        let heartbeat_secs = num("P0_MARKET_HEARTBEAT_SECS", DEFAULT_HEARTBEAT_SECS)?;
        let ingress_buffer =
            usize::try_from(num("P0_INGRESS_BUFFER", DEFAULT_INGRESS_BUFFER as u64)?)
                .map_err(|_| "invalid P0_INGRESS_BUFFER".to_string())?;
        if duration_secs == 0
            || ready_timeout_secs == 0
            || drain_timeout_secs == 0
            || heartbeat_secs == 0
            || ingress_buffer == 0
        {
            return Err("P0_MARKET_DURATION_SECS, P0_MARKET_READY_TIMEOUT_SECS, P0_MARKET_DRAIN_TIMEOUT_SECS, P0_MARKET_HEARTBEAT_SECS and P0_INGRESS_BUFFER must be > 0".into());
        }
        let d = EnrichConfig::DEFAULT;
        let enrich = EnrichConfig {
            max_pending: usize::try_from(num(
                "P0_MARKET_ENRICH_MAX_PENDING",
                d.max_pending as u64,
            )?)
            .map_err(|_| "invalid P0_MARKET_ENRICH_MAX_PENDING".to_string())?,
            max_attempts: u32::try_from(num(
                "P0_MARKET_ENRICH_MAX_ATTEMPTS",
                u64::from(d.max_attempts),
            )?)
            .map_err(|_| "invalid P0_MARKET_ENRICH_MAX_ATTEMPTS".to_string())?,
            retry_ms: num("P0_MARKET_ENRICH_RETRY_MS", d.retry_ms)?,
            timeout_ms: num("P0_MARKET_ENRICH_TIMEOUT_MS", d.timeout_ms)?,
            phase_max_secs: num("P0_MARKET_ENRICH_PHASE_MAX_SECS", d.phase_max_secs)?,
        };
        if enrich.max_pending == 0
            || enrich.max_attempts == 0
            || enrich.timeout_ms == 0
            || enrich.phase_max_secs == 0
        {
            return Err("enrichment limits must be > 0".into());
        }
        let run_id = opt("P0_MARKET_RUN_ID")
            .unwrap_or_else(|| format!("market_{}", utc_compact(now_unix_s)));
        if !run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(format!("invalid P0_MARKET_RUN_ID={run_id}"));
        }
        let out_dir = opt("P0_MARKET_OUT_DIR").unwrap_or_else(|| DEFAULT_OUT_DIR.to_string());
        let run_dir = PathBuf::from(out_dir).join(&run_id);
        Ok(Self {
            ws_url,
            rpc_url,
            program,
            source,
            commitment,
            duration_secs,
            ready_timeout_secs,
            drain_timeout_secs,
            heartbeat_secs,
            min_sample_liquidations,
            ingress_buffer,
            run_id,
            run_dir,
            enrich,
            gap: GapConfig::from_lookup(get)?,
            reconcile: ReconcileConfig::from_lookup(get)?,
        })
    }
}

/// Market ingress configuration: source subscription (+ slotSubscribe as telemetry), market
/// commitment and the transport heartbeat. Shadow/Live keep `WsRpcConfig::new` defaults.
pub fn market_ws_config(cfg: &MarketConfig) -> Result<WsRpcConfig, String> {
    let sub = match cfg.source {
        MarketSource::Transactions => Subscription {
            transactions_mentioning: vec![cfg.program],
            slots: true,
            ..Subscription::default()
        },
        MarketSource::Logs => Subscription {
            logs_mentioning: vec![cfg.program],
            slots: true,
            ..Subscription::default()
        },
    };
    let mut ws = WsRpcConfig::new(cfg.ws_url.clone(), sub);
    ws.commitment = cfg.commitment;
    ws.recovery_budget = Some(Duration::from_millis(
        darkbot_solana::runtime_config::active()
            .limits
            .wss_recovery_ms,
    ));
    let heartbeat = Duration::from_secs(cfg.heartbeat_secs);
    if heartbeat >= ws.idle_timeout {
        return Err(format!(
            "P0_MARKET_HEARTBEAT_SECS={} must be below the ingress idle timeout ({}s)",
            cfg.heartbeat_secs,
            ws.idle_timeout.as_secs()
        ));
    }
    ws.heartbeat_interval = Some(heartbeat);
    Ok(ws)
}

/// Host part of an endpoint, without query string (API keys live in the query).
pub fn redact_url(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|host| format!("{}://{}", u.scheme(), host))
        })
        .unwrap_or_else(|| "INVALID_ENDPOINT".into())
}

// ------------------------------------------------------------------------------------------
// UTC formatting (no extra dependency)
// ------------------------------------------------------------------------------------------

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn utc_parts(unix_s: u64) -> (i64, u32, u32, u64, u64, u64) {
    let days = (unix_s / 86_400) as i64;
    let rem = unix_s % 86_400;
    let (y, m, d) = civil_from_days(days);
    (y, m, d, rem / 3_600, (rem % 3_600) / 60, rem % 60)
}

/// `YYYY-MM-DDTHH:MM:SSZ`
pub fn utc_iso(unix_s: u64) -> String {
    let (y, m, d, hh, mm, ss) = utc_parts(unix_s);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// `YYYYMMDDTHHMMSSZ`
pub fn utc_compact(unix_s: u64) -> String {
    let (y, m, d, hh, mm, ss) = utc_parts(unix_s);
    format!("{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}Z")
}

fn unix_now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ------------------------------------------------------------------------------------------
// LiquidationReceiverEvent decoder (fail closed, program-attributed)
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ReceiverEvent {
    pub event_index: u32,
    pub target: Pubkey,
    pub receiver: Pubkey,
    pub seized_equity_usd: f64,
    pub repaid_equity_usd: f64,
    pub protocol_flat_fee_lamports: u32,
}

impl ReceiverEvent {
    pub fn gross_edge_usd(&self) -> f64 {
        self.seized_equity_usd - self.repaid_equity_usd
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LogDecode {
    pub events: Vec<ReceiverEvent>,
    /// Other liquidation types decoded provisionally from the same attributed logs.
    pub additional_events: Vec<truth::ProvisionalLogEvent>,
    /// Marginfi-emitted data that could hide a receiver event: receiver discriminator with
    /// too-short/non-finite payload, or undecodable base64 while Marginfi is executing.
    pub decoder_errors: u64,
    /// Receiver-event-shaped data not emitted by the Marginfi program itself (spoof protection).
    pub foreign_program_events: u64,
    /// Undecodable `Program data:` lines emitted by other programs.
    pub base64_errors: u64,
    /// The runtime truncated the log; later events may be missing.
    pub truncated: bool,
}

enum Payload {
    NotReceiverEvent,
    Invalid,
    Event {
        target: Pubkey,
        receiver: Pubkey,
        seized: f64,
        repaid: f64,
        flat_fee: u32,
    },
}

fn parse_receiver_payload(b: &[u8]) -> Payload {
    if b.len() < 8 || b[..8] != LIQUIDATION_RECEIVER_EVENT_DISCRIMINATOR {
        return Payload::NotReceiverEvent;
    }
    if b.len() < LIQUIDATION_RECEIVER_EVENT_LEN {
        return Payload::Invalid;
    }
    let mut target = [0u8; 32];
    target.copy_from_slice(&b[8..40]);
    let mut receiver = [0u8; 32];
    receiver.copy_from_slice(&b[40..72]);
    let mut seized = [0u8; 8];
    seized.copy_from_slice(&b[72..80]);
    let mut repaid = [0u8; 8];
    repaid.copy_from_slice(&b[80..88]);
    let mut fee = [0u8; 4];
    fee.copy_from_slice(&b[88..92]);
    let (seized, repaid) = (f64::from_le_bytes(seized), f64::from_le_bytes(repaid));
    if !seized.is_finite() || !repaid.is_finite() {
        return Payload::Invalid;
    }
    Payload::Event {
        target: Pubkey::new_from_array(target),
        receiver: Pubkey::new_from_array(receiver),
        seized,
        repaid,
        flat_fee: u32::from_le_bytes(fee),
    }
}

/// Decode `LiquidationReceiverEvent`s from log messages. An event counts only when the
/// `Program data:` line was emitted while `program` is the currently executing program
/// (invoke/success/failed stack), so another program cannot inject fake receiver events.
pub fn decode_receiver_events(logs: &[String], program: &Pubkey) -> LogDecode {
    let program_s = program.to_string();
    let mut out = LogDecode::default();
    let mut stack: Vec<&str> = Vec::new();
    for line in logs {
        if line.starts_with("Log truncated") {
            out.truncated = true;
            break;
        }
        if let Some(data) = line.strip_prefix(PROGRAM_DATA_PREFIX) {
            let program_executing = stack.last().copied() == Some(program_s.as_str());
            let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data.trim()) else {
                if program_executing {
                    out.decoder_errors += 1;
                } else {
                    out.base64_errors += 1;
                }
                continue;
            };
            match parse_receiver_payload(&bytes) {
                Payload::NotReceiverEvent if program_executing => {
                    // Reuse the existing canonical event decoder: no second RPC scanner.
                    match truth::provisional_event(&bytes) {
                        Ok(Some(ev)) => out.additional_events.push(ev),
                        Ok(None) => {}
                        Err(_) => out.decoder_errors += 1,
                    }
                }
                Payload::NotReceiverEvent => {}
                Payload::Invalid if program_executing => out.decoder_errors += 1,
                Payload::Invalid => out.foreign_program_events += 1,
                Payload::Event {
                    target,
                    receiver,
                    seized,
                    repaid,
                    flat_fee,
                } => {
                    if !program_executing {
                        out.foreign_program_events += 1;
                        continue;
                    }
                    let event_index = out.events.len() as u32;
                    out.events.push(ReceiverEvent {
                        event_index,
                        target,
                        receiver,
                        seized_equity_usd: seized,
                        repaid_equity_usd: repaid,
                        protocol_flat_fee_lamports: flat_fee,
                    });
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("Program ") {
            let mut parts = rest.split_whitespace();
            let (Some(id), Some(word)) = (parts.next(), parts.next()) else {
                continue;
            };
            if word == "invoke" {
                stack.push(id);
            } else if (word == "success" || word.starts_with("failed"))
                && stack.last().copied() == Some(id)
            {
                stack.pop();
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------------------
// On-chain enrichment from a full transaction object ({transaction, meta, version})
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TokenDelta {
    pub mint: String,
    pub native_delta: i128,
    pub decimals: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TxEnrichment {
    pub meta_available: bool,
    pub tx_version: Option<String>,
    pub fee_payer: Option<String>,
    pub tx_fee_lamports: Option<u64>,
    pub receiver_lamport_delta: Option<i128>,
    pub receiver_token_deltas: Vec<TokenDelta>,
    pub token_deltas_complete: bool,
    pub cu_limit: Option<u32>,
    pub cu_price_micro_lamports: Option<u64>,
}

fn version_label(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => format!("v{n}"),
        _ => "unknown".to_string(),
    }
}

fn key_str(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::to_string)
        .or_else(|| v.get("pubkey").and_then(Value::as_str).map(str::to_string))
}

/// static accountKeys + meta.loadedAddresses.writable + meta.loadedAddresses.readonly.
/// `None` if lookups exist but loaded addresses are missing (indices would be ambiguous).
fn full_account_keys(message: &Value, meta: Option<&Value>) -> Option<Vec<String>> {
    let statics = message.get("accountKeys")?.as_array()?;
    let mut keys = Vec::with_capacity(statics.len());
    for k in statics {
        keys.push(key_str(k)?);
    }
    let has_lookups = message
        .get("addressTableLookups")
        .and_then(Value::as_array)
        .is_some_and(|l| !l.is_empty());
    match meta
        .and_then(|m| m.get("loadedAddresses"))
        .filter(|l| l.is_object())
    {
        Some(loaded) => {
            for part in ["writable", "readonly"] {
                if let Some(arr) = loaded.get(part).and_then(Value::as_array) {
                    for k in arr {
                        keys.push(key_str(k)?);
                    }
                }
            }
        }
        None if has_lookups => return None,
        None => {}
    }
    Some(keys)
}

fn lamport_delta(keys: &[String], meta: &Value, receiver: &str) -> Option<i128> {
    let mut positions = keys
        .iter()
        .enumerate()
        .filter(|(_, k)| k.as_str() == receiver)
        .map(|(i, _)| i);
    let i = positions.next()?;
    if positions.next().is_some() {
        return None; // not unique -> do not guess
    }
    let pre = meta.get("preBalances")?.as_array()?.get(i)?.as_u64()?;
    let post = meta.get("postBalances")?.as_array()?.get(i)?.as_u64()?;
    Some(i128::from(post) - i128::from(pre))
}

fn token_deltas(meta: &Value, receiver: &str) -> (Vec<TokenDelta>, bool) {
    let mut acc: BTreeMap<String, (i128, Option<u64>)> = BTreeMap::new();
    let mut complete = true;
    for (field, sign) in [("preTokenBalances", -1i128), ("postTokenBalances", 1i128)] {
        let Some(arr) = meta.get(field).and_then(Value::as_array) else {
            complete = false;
            continue;
        };
        for b in arr {
            if b.get("owner").and_then(Value::as_str) != Some(receiver) {
                continue;
            }
            let mint = b.get("mint").and_then(Value::as_str);
            let amount = b
                .pointer("/uiTokenAmount/amount")
                .and_then(Value::as_str)
                .and_then(|a| a.parse::<i128>().ok());
            let (Some(mint), Some(amount)) = (mint, amount) else {
                complete = false;
                continue;
            };
            let decimals = b.pointer("/uiTokenAmount/decimals").and_then(Value::as_u64);
            let entry = acc.entry(mint.to_string()).or_insert((0, decimals));
            entry.0 += sign * amount;
            if entry.1.is_none() {
                entry.1 = decimals;
            }
        }
    }
    let deltas = acc
        .into_iter()
        .map(|(mint, (native_delta, decimals))| TokenDelta {
            mint,
            native_delta,
            decimals,
        })
        .collect();
    (deltas, complete)
}

/// Legacy/v0 ComputeBudget instructions: 2 = SetComputeUnitLimit(u32), 3 = SetComputeUnitPrice(u64).
/// v1 transactions carry compute budget in their transaction config, not as instructions -> None.
fn compute_budget(message: &Value, keys: &[String]) -> (Option<u32>, Option<u64>) {
    let (mut limit, mut price) = (None, None);
    let Some(ixs) = message.get("instructions").and_then(Value::as_array) else {
        return (None, None);
    };
    for ix in ixs {
        let Some(pi) = ix.get("programIdIndex").and_then(Value::as_u64) else {
            continue;
        };
        let is_cb = usize::try_from(pi)
            .ok()
            .and_then(|i| keys.get(i))
            .map(String::as_str)
            == Some(COMPUTE_BUDGET_PROGRAM);
        if !is_cb {
            continue;
        }
        let Some(data) = ix
            .get("data")
            .and_then(Value::as_str)
            .and_then(|d| bs58::decode(d).into_vec().ok())
        else {
            continue;
        };
        match data.first() {
            Some(2) if data.len() >= 5 => {
                limit = Some(u32::from_le_bytes([data[1], data[2], data[3], data[4]]))
            }
            Some(3) if data.len() >= 9 => {
                let mut a = [0u8; 8];
                a.copy_from_slice(&data[1..9]);
                price = Some(u64::from_le_bytes(a));
            }
            _ => {}
        }
    }
    (limit, price)
}

/// Enrich one receiver from a full transaction object (`{transaction:{signatures,message}, meta, version}`,
/// i.e. `getTransaction` result or Helius `transactionNotification.result.transaction`).
pub fn enrich(tx: &Value, receiver: &Pubkey) -> TxEnrichment {
    let meta = tx.get("meta").filter(|m| m.is_object());
    let message = tx.pointer("/transaction/message").filter(|m| m.is_object());
    let receiver_s = receiver.to_string();
    let keys = message.and_then(|m| full_account_keys(m, meta));
    let mut e = TxEnrichment {
        meta_available: meta.is_some(),
        tx_version: tx.get("version").map(version_label),
        fee_payer: keys.as_ref().and_then(|k| k.first().cloned()),
        ..TxEnrichment::default()
    };
    if let Some(m) = meta {
        e.tx_fee_lamports = m.get("fee").and_then(Value::as_u64);
        if let Some(k) = &keys {
            e.receiver_lamport_delta = lamport_delta(k, m, &receiver_s);
        }
        let (deltas, complete) = token_deltas(m, &receiver_s);
        e.receiver_token_deltas = deltas;
        e.token_deltas_complete = complete;
    }
    if let (Some(msg), Some(k)) = (message, &keys) {
        let (limit, price) = compute_budget(msg, k);
        e.cu_limit = limit;
        e.cu_price_micro_lamports = price;
    }
    e
}

/// The fetched transaction must be exactly the observed one (first signature and slot).
pub fn verify_tx_identity(tx: &Value, signature: &str, slot: u64) -> Result<(), String> {
    match tx
        .pointer("/transaction/signatures/0")
        .and_then(Value::as_str)
    {
        Some(s) if s == signature => {}
        Some(s) => {
            return Err(format!(
                "signature mismatch requested={signature} returned={s}"
            ))
        }
        None => return Err("signature missing".into()),
    }
    match tx.get("slot").and_then(Value::as_u64) {
        Some(s) if s == slot => Ok(()),
        Some(s) => Err(format!("slot mismatch observed={slot} returned={s}")),
        None => Err("slot missing".into()),
    }
}

// ------------------------------------------------------------------------------------------
// Output sink
// ------------------------------------------------------------------------------------------

pub trait MarketSink {
    fn transaction(&mut self, row: &Value) -> std::io::Result<()>;
    fn liquidation(&mut self, row: &Value) -> std::io::Result<()>;
    fn coverage(&mut self, row: &Value) -> std::io::Result<()>;
    fn flush(&mut self) -> std::io::Result<()>;
}

pub struct JsonlSink {
    transactions: BufWriter<File>,
    liquidations: BufWriter<File>,
    coverage: BufWriter<File>,
}

fn create_new(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("cannot create {}: {e}", path.display()))
}

fn write_line(w: &mut BufWriter<File>, row: &Value) -> std::io::Result<()> {
    serde_json::to_writer(&mut *w, row).map_err(std::io::Error::other)?;
    w.write_all(b"\n")
}

impl JsonlSink {
    /// Never overwrites: every file is created with `create_new`.
    pub fn create(run_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            transactions: BufWriter::new(create_new(&run_dir.join("transactions.jsonl"))?),
            liquidations: BufWriter::new(create_new(&run_dir.join("liquidations.jsonl"))?),
            coverage: BufWriter::new(create_new(&run_dir.join("coverage.jsonl"))?),
        })
    }
}

impl MarketSink for JsonlSink {
    fn transaction(&mut self, row: &Value) -> std::io::Result<()> {
        write_line(&mut self.transactions, row)
    }
    fn liquidation(&mut self, row: &Value) -> std::io::Result<()> {
        write_line(&mut self.liquidations, row)
    }
    fn coverage(&mut self, row: &Value) -> std::io::Result<()> {
        write_line(&mut self.coverage, row)?;
        self.coverage.flush() // coverage events are rare and must survive a kill
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.transactions.flush()?;
        self.liquidations.flush()?;
        self.coverage.flush()
    }
}

// ------------------------------------------------------------------------------------------
// Observer core (pure bookkeeping; I/O only via MarketSink)
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct MarketStats {
    pub transactions_received: u64,
    pub unique_transactions: u64,
    pub duplicate_transactions: u64,
    pub successful_transactions: u64,
    pub failed_transactions: u64,
    pub liquidation_transactions: u64,
    pub liquidation_events: u64,
    pub provisional_classic_events: u64,
    pub provisional_deleverage_events: u64,
    pub provisional_bankruptcy_events: u64,
    pub failed_tx_receiver_events_ignored: u64,
    pub foreign_program_events: u64,
    pub decoder_error_count: u64,
    pub decoder_errors_successful_tx: u64,
    pub base64_error_count: u64,
    /// Successful transactions whose logs were truncated by the runtime (total).
    pub logs_truncated: u64,
    /// [r6] Truncated logs proven complete for liquidations via the F8 instruction truth.
    pub logs_truncated_qualified: u64,
    /// [r6] Truncated logs that could not be checked (fetch/parse/identity/...): fail closed.
    pub logs_truncated_unqualified: u64,
    /// [r6] Instruction end_liquidations and logged receiver events disagree: fail closed.
    pub logs_truncated_liquidation_mismatch: u64,
    pub truncated_instruction_end_liquidations: u64,
    pub truncated_log_end_liquidations: u64,
    pub log_messages_missing: u64,
    pub gap_count: u64,
    pub reconnect_count: u64,
    pub transport_reconnect_attempts: u64,
    pub transport_recovered: u64,
    pub transport_http_429: u64,
    pub ingress_overflow_count: u64,
    pub fatal_count: u64,
    pub rpc_fallback_count: u64,
    pub enrichment_requested: u64,
    pub enrichment_succeeded: u64,
    pub enrichment_failed: u64,
    pub enrichment_skipped_queue_full: u64,
    pub protocol_flat_fee_total_lamports: u64,
    pub tx_fee_total_lamports: u64,
    pub tx_fee_unknown_transactions: u64,
    pub events_outside_observation: u64,
    pub events_after_cutoff: u64,
    pub slot_notifications: u64,
    pub max_stream_silence_ms: u64,
    pub first_observed_slot: Option<u64>,
    pub last_observed_slot: Option<u64>,
}

/// [r6] A successful transaction with truncated logs awaiting instruction-based qualification.
struct TruncatedPending {
    signature: String,
    slot: u64,
    /// Receiver events decoded from the (truncated) logs as `(target, receiver)`.
    logged: Vec<(Pubkey, Pubkey)>,
}

/// [r6] Outcome of checking a truncated-log transaction against the F8 instruction truth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TruncationQualification {
    /// Instruction end_liquidations == logged events (multiset of target+receiver).
    Qualified { instruction_liquidations: usize },
    /// Instruction end_liquidations and logged events differ.
    Mismatch { instruction_liquidations: usize },
    /// Could not be checked; bounded reason code.
    Unqualified(&'static str),
}

/// [r6] Pure qualification of a fetched/payload transaction (typed F8 view) against the logged
/// receiver events. No second decoder: `end_liquidation_instructions` is the winner-parser truth.
pub fn qualify_truncated(
    tx_json: Value,
    signature: &str,
    slot: u64,
    program: &Pubkey,
    logged: &[(Pubkey, Pubkey)],
) -> TruncationQualification {
    let tx: RpcTransaction = match serde_json::from_value(tx_json) {
        Ok(t) => t,
        Err(_) => return TruncationQualification::Unqualified("TX_DECODE"),
    };
    if check_transaction_identity(&tx, signature, slot).is_err() {
        return TruncationQualification::Unqualified("IDENTITY_MISMATCH");
    }
    let Some(meta) = tx.meta.as_ref() else {
        return TruncationQualification::Unqualified("META_MISSING");
    };
    if meta.err.is_some() {
        return TruncationQualification::Unqualified("TX_FAILED_ON_CHAIN");
    }
    match end_liquidation_instructions(&tx, program) {
        Err(e) => TruncationQualification::Unqualified(e.code()),
        Ok(mut instruction) => {
            let mut logged = logged.to_vec();
            instruction.sort();
            logged.sort();
            if instruction == logged {
                TruncationQualification::Qualified {
                    instruction_liquidations: instruction.len(),
                }
            } else {
                TruncationQualification::Mismatch {
                    instruction_liquidations: instruction.len(),
                }
            }
        }
    }
}

struct DeferredLiquidation {
    signature: String,
    slot: u64,
    t_rx_ns: u64,
    observed_unix_s: u64,
    events: Vec<ReceiverEvent>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReceiverRank {
    pub receiver: String,
    pub liquidation_events: u64,
    pub gross_edge_total_usd: f64,
}

pub struct ObserverCore {
    econ: Option<crate::econ::Handle>,
    program: Pubkey,
    source: MarketSource,
    enrich_cfg: EnrichConfig,
    observing: bool,
    /// `(start_t_rx_ns, cutoff_t_rx_ns)` on the shared monotonic ingress clock.
    window: Option<(u64, u64)>,
    seen: HashSet<String>,
    liquidation_signatures: HashSet<String>,
    targets: HashSet<String>,
    edges: Vec<f64>,
    receivers: HashMap<String, (u64, f64)>,
    deferred: VecDeque<DeferredLiquidation>,
    truncated_pending: VecDeque<TruncatedPending>,
    truncated_unqualified_reasons: BTreeMap<&'static str, u64>,
    incomplete_reasons: BTreeSet<&'static str>,
    last_slot_frame_ns: Option<u64>,
    last_slot_progress_ns: Option<u64>,
    enrichment_init_error: Option<String>,
    gap_cfg: GapConfig,
    reconcile_cfg: ReconcileConfig,
    reconciliation: reconcile::Reconciliation,
    truth: truth::TruthLedger,
    deployment_before: Option<paths::Deployment>,
    deployment_after: Option<paths::Deployment>,
    deployment_error: Option<&'static str>,
    gaps: Vec<gap::MarketGap>,
    gap_stats: gap::GapStats,
    pub stats: MarketStats,
}

/// Final verdict. `MARKET_DATA_VALID` needs a complete capture AND a verified source.
pub fn market_verdict(
    stream_ready: bool,
    market_capture_complete: bool,
    source_coverage_verified: bool,
) -> &'static str {
    if !stream_ready {
        "MARKET_OBSERVER_STREAM_NOT_READY"
    } else if !market_capture_complete {
        "MARKET_DATA_INCOMPLETE"
    } else if !source_coverage_verified {
        "MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED"
    } else {
        "MARKET_DATA_VALID"
    }
}

/// Deterministic nearest-rank quantile on an ascending slice: index `ceil(q*n)-1`, clamped.
pub fn quantile_nearest_rank(sorted: &[f64], q: f64) -> Option<f64> {
    if sorted.is_empty() || !(0.0..=1.0).contains(&q) {
        return None;
    }
    let n = sorted.len();
    let rank = ((q * n as f64).ceil() as usize).clamp(1, n);
    Some(sorted[rank - 1])
}

fn ratio(part: f64, total: f64) -> Option<f64> {
    if total > 0.0 {
        Some(part / total)
    } else {
        None
    }
}

impl ObserverCore {
    pub fn new(program: Pubkey, source: MarketSource, enrich_cfg: EnrichConfig) -> Self {
        Self {
            econ: None,
            program,
            source,
            enrich_cfg,
            observing: false,
            window: None,
            seen: HashSet::new(),
            liquidation_signatures: HashSet::new(),
            targets: HashSet::new(),
            edges: Vec::new(),
            receivers: HashMap::new(),
            deferred: VecDeque::new(),
            truncated_pending: VecDeque::new(),
            truncated_unqualified_reasons: BTreeMap::new(),
            incomplete_reasons: BTreeSet::new(),
            last_slot_frame_ns: None,
            last_slot_progress_ns: None,
            enrichment_init_error: None,
            gap_cfg: GapConfig::default(),
            reconcile_cfg: ReconcileConfig::default(),
            reconciliation: reconcile::Reconciliation::default(),
            truth: truth::TruthLedger::default(),
            deployment_before: None,
            deployment_after: None,
            deployment_error: None,
            gaps: Vec::new(),
            gap_stats: gap::GapStats::default(),
            stats: MarketStats::default(),
        }
    }

    /// Open the observation window `[start_t_rx_ns, cutoff_t_rx_ns]`.
    pub fn start_observation(&mut self, start_t_rx_ns: u64, cutoff_t_rx_ns: u64) {
        self.observing = true;
        self.window = Some((start_t_rx_ns, cutoff_t_rx_ns));
    }

    pub fn stop_observation(&mut self) {
        self.observing = false;
    }

    pub fn is_observing(&self) -> bool {
        self.observing
    }

    pub fn has_deferred(&self) -> bool {
        !self.deferred.is_empty()
    }

    pub fn mark_incomplete(&mut self, reason: &'static str) {
        self.incomplete_reasons.insert(reason);
    }

    pub fn coverage_complete(&self) -> bool {
        self.gaps.len() as u64 == self.gap_stats.gap_recovered
            && self.stats.gap_count == self.gaps.iter().map(|g| g.raw_failures).sum::<u64>()
            && !self.incomplete_reasons.contains("GAP_STATE_CAPACITY")
            && self.stats.ingress_overflow_count == 0
            && self.stats.fatal_count == 0
    }

    /// [r6] Truncated logs no longer fail by themselves: only unqualified or mismatching ones do
    /// (and any still awaiting qualification).
    pub fn decode_complete(&self) -> bool {
        self.stats.log_messages_missing == 0
            && self.stats.decoder_errors_successful_tx == 0
            && self.stats.logs_truncated_unqualified == 0
            && self.stats.logs_truncated_liquidation_mismatch == 0
            && self.truncated_pending.is_empty()
    }

    pub fn has_truncated_pending(&self) -> bool {
        !self.truncated_pending.is_empty()
    }

    fn apply_truncation(&mut self, q: TruncationQualification) {
        match q {
            TruncationQualification::Qualified {
                instruction_liquidations,
            } => {
                self.stats.logs_truncated_qualified += 1;
                self.stats.truncated_instruction_end_liquidations +=
                    instruction_liquidations as u64;
            }
            TruncationQualification::Mismatch {
                instruction_liquidations,
            } => {
                self.stats.logs_truncated_liquidation_mismatch += 1;
                self.stats.truncated_instruction_end_liquidations +=
                    instruction_liquidations as u64;
                self.mark_incomplete("TRUNCATED_LIQUIDATION_MISMATCH");
            }
            TruncationQualification::Unqualified(code) => {
                self.stats.logs_truncated_unqualified += 1;
                *self.truncated_unqualified_reasons.entry(code).or_insert(0) += 1;
                self.mark_incomplete("LOGS_TRUNCATED_UNQUALIFIED");
            }
        }
    }

    /// Records a successful truncated transaction: counts and returns the logged pairs.
    fn note_truncated(&mut self, decoded: &LogDecode) -> Vec<(Pubkey, Pubkey)> {
        self.stats.logs_truncated += 1;
        self.stats.truncated_log_end_liquidations += decoded.events.len() as u64;
        decoded
            .events
            .iter()
            .map(|e| (e.target, e.receiver))
            .collect()
    }

    /// [r6] Post-capture qualification of truncated `logs`-source transactions (never during
    /// capture). Same bounded fetch contract as enrichment; failures fail closed.
    pub fn qualify_truncated_deferred(
        &mut self,
        fetch: &mut dyn FnMut(&str) -> Result<Option<Value>, String>,
        wait: &mut dyn FnMut(Duration),
        budget_left: &mut dyn FnMut() -> bool,
    ) {
        let retry = Duration::from_millis(self.enrich_cfg.retry_ms);
        let max_attempts = self.enrich_cfg.max_attempts;
        while let Some(p) = self.truncated_pending.pop_front() {
            let mut q = TruncationQualification::Unqualified("FETCH_FAILED");
            for attempt in 1..=max_attempts {
                if !budget_left() {
                    q = TruncationQualification::Unqualified("PHASE_BUDGET_EXHAUSTED");
                    break;
                }
                self.stats.rpc_fallback_count += 1;
                if let Ok(Some(tx)) = fetch(&p.signature) {
                    q = qualify_truncated(tx, &p.signature, p.slot, &self.program, &p.logged);
                    break;
                }
                if attempt < max_attempts {
                    wait(retry);
                }
            }
            self.apply_truncation(q);
        }
    }

    /// Every still-pending truncated transaction fails closed with `code`.
    pub fn fail_truncated_pending(&mut self, code: &'static str) {
        while self.truncated_pending.pop_front().is_some() {
            self.apply_truncation(TruncationQualification::Unqualified(code));
        }
    }

    pub fn enrichment_complete(&self) -> bool {
        self.stats.enrichment_failed == 0
            && self.stats.enrichment_succeeded == self.stats.enrichment_requested
    }

    /// Inside the window? `None` (no timestamp, e.g. channel overflow) counts while observing.
    fn in_window(&self, t_rx_ns: Option<u64>) -> bool {
        if !self.observing {
            return false;
        }
        match (self.window, t_rx_ns) {
            (Some((start, cutoff)), Some(t)) => t >= start && t <= cutoff,
            _ => true,
        }
    }

    /// Write a coverage event. GAP / RECONNECT / INGRESS_OVERFLOW / FATAL count only inside
    /// the observation window; outside they are recorded with `counted=false`.
    pub fn coverage(
        &mut self,
        sink: &mut dyn MarketSink,
        kind: &str,
        now_unix_s: u64,
        t_rx_ns: Option<u64>,
        detail: Value,
    ) -> std::io::Result<()> {
        let counted = self.in_window(t_rx_ns);
        if counted {
            match kind {
                "GAP" => {
                    self.stats.gap_count += 1;
                    self.record_gap(t_rx_ns, now_unix_s, &detail);
                    self.mark_incomplete("GAP");
                }
                "RECONNECT" => {
                    self.stats.reconnect_count += 1;
                    self.record_gap_reconnect(t_rx_ns, &detail);
                }
                "INGRESS_OVERFLOW" => {
                    self.stats.ingress_overflow_count += 1;
                    self.mark_incomplete("INGRESS_OVERFLOW");
                }
                "FATAL" => {
                    self.stats.fatal_count += 1;
                    self.mark_incomplete("FATAL");
                }
                _ => {}
            }
        }
        sink.coverage(&json!({
            "event": kind,
            "timestamp_utc": utc_iso(now_unix_s),
            "t_rx_ns": t_rx_ns,
            "observing": self.observing,
            "counted": counted,
            "detail": detail,
        }))
    }

    /// Slot notifications are telemetry; their spacing documents stream liveness.
    fn on_slot_frame(&mut self, t_rx_ns: u64) {
        if !self.in_window(Some(t_rx_ns)) {
            return;
        }
        self.stats.slot_notifications += 1;
        if let Some(prev) = self.last_slot_frame_ns {
            let gap_ms = t_rx_ns.saturating_sub(prev) / 1_000_000;
            self.stats.max_stream_silence_ms = self.stats.max_stream_silence_ms.max(gap_ms);
        }
        self.last_slot_frame_ns = Some(t_rx_ns);
    }

    /// Dedupe + window + counters. `false` = not part of the measurement.
    fn admit(&mut self, signature: &str, slot: u64, success: bool, t_rx_ns: u64) -> bool {
        if !self.observing {
            self.stats.events_outside_observation += 1;
            return false;
        }
        if let Some((start, cutoff)) = self.window {
            if t_rx_ns < start {
                self.stats.events_outside_observation += 1;
                return false;
            }
            if t_rx_ns > cutoff {
                self.stats.events_after_cutoff += 1;
                return false;
            }
        }
        let cap = self.econ.as_ref().map(|h| h.capacity()).unwrap_or(100_000);
        if self.truth.live_receipts.len() < cap || self.truth.live_receipts.contains_key(signature)
        {
            self.truth
                .live_receipts
                .entry(signature.to_owned())
                .or_insert((slot, t_rx_ns));
        } else {
            self.truth.retention_error = true;
        }
        self.admit_identity(signature, slot, success)
    }

    fn admit_identity(&mut self, signature: &str, slot: u64, success: bool) -> bool {
        if self
            .econ
            .as_ref()
            .is_some_and(|h| self.seen.len() >= h.capacity() && !self.seen.contains(signature))
        {
            self.mark_incomplete("C0_E1_MARKET_RETENTION_OVERFLOW");
            self.econ
                .as_ref()
                .unwrap()
                .shared
                .retention_overflow
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return false;
        }
        self.stats.transactions_received += 1;
        if !self.seen.insert(signature.to_string()) {
            self.stats.duplicate_transactions += 1;
            return false;
        }
        self.stats.unique_transactions += 1;
        if success {
            self.stats.successful_transactions += 1;
        } else {
            self.stats.failed_transactions += 1;
        }
        self.stats.first_observed_slot =
            Some(self.stats.first_observed_slot.map_or(slot, |s| s.min(slot)));
        self.stats.last_observed_slot =
            Some(self.stats.last_observed_slot.map_or(slot, |s| s.max(slot)));
        true
    }

    fn absorb_decode(&mut self, d: &LogDecode, success: bool) {
        self.stats.decoder_error_count += d.decoder_errors;
        self.stats.foreign_program_events += d.foreign_program_events;
        self.stats.base64_error_count += d.base64_errors;
        if success {
            if d.decoder_errors > 0 {
                self.stats.decoder_errors_successful_tx += d.decoder_errors;
                self.mark_incomplete("DECODER_ERROR");
            }
        } else {
            self.stats.failed_tx_receiver_events_ignored += d.events.len() as u64;
        }
    }

    fn aggregate(
        &mut self,
        signature: &str,
        slot: u64,
        t_rx_ns: Option<u64>,
        transaction_index: Option<u32>,
        events: &[ReceiverEvent],
    ) {
        if self.liquidation_signatures.insert(signature.to_string()) {
            self.stats.liquidation_transactions += 1;
        }
        for ev in events {
            if let Some(h) = &self.econ {
                h.emit(crate::econ::Evidence::Market(crate::econ::MarketEvent {
                    signature: signature.into(),
                    slot,
                    t_rx_ns,
                    source: if t_rx_ns.is_some() {
                        crate::econ::MarketEvidenceSource::Live
                    } else {
                        crate::econ::MarketEvidenceSource::GapBackfill
                    },
                    transaction_index,
                    event: ev.clone(),
                }));
            }
            if self
                .econ
                .as_ref()
                .is_some_and(|h| self.edges.len() >= h.capacity())
            {
                self.mark_incomplete("C0_E1_EVENT_RETENTION_OVERFLOW");
                self.econ
                    .as_ref()
                    .unwrap()
                    .shared
                    .retention_overflow
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                continue;
            }
            let edge = ev.gross_edge_usd();
            self.stats.liquidation_events += 1;
            self.stats.protocol_flat_fee_total_lamports += u64::from(ev.protocol_flat_fee_lamports);
            self.edges.push(edge);
            self.targets.insert(ev.target.to_string());
            let r = self
                .receivers
                .entry(ev.receiver.to_string())
                .or_insert((0, 0.0));
            r.0 += 1;
            r.1 += edge;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write_liquidation_rows(
        &mut self,
        sink: &mut dyn MarketSink,
        signature: &str,
        slot: u64,
        t_rx_ns: Option<u64>,
        observed_unix_s: u64,
        events: &[ReceiverEvent],
        tx: Option<&Value>,
        enrichment_status: &str,
    ) -> std::io::Result<()> {
        let mut fee_counted = false;
        for ev in events {
            let e = tx.map(|t| enrich(t, &ev.receiver)).unwrap_or_default();
            if !fee_counted {
                fee_counted = true;
                match e.tx_fee_lamports {
                    Some(fee) => self.stats.tx_fee_total_lamports += fee,
                    None => self.stats.tx_fee_unknown_transactions += 1,
                }
            }
            let token_deltas: Vec<Value> = e
                .receiver_token_deltas
                .iter()
                .map(|d| json!({"mint": d.mint, "native_delta": d.native_delta.to_string(), "decimals": d.decimals}))
                .collect();
            sink.liquidation(&json!({
                "observed_at_utc": t_rx_ns.map(|_| utc_iso(observed_unix_s)),
                "t_rx_ns": t_rx_ns,
                "source": if t_rx_ns.is_some() { "live" } else { "gap_backfill" },
                "live_timestamp_qualified": t_rx_ns.is_some(),
                "slot": slot,
                "signature": signature,
                "event_index": ev.event_index,
                "tx_version": e.tx_version,
                "target": ev.target.to_string(),
                "receiver": ev.receiver.to_string(),
                "seized_equity_usd": ev.seized_equity_usd,
                "repaid_equity_usd": ev.repaid_equity_usd,
                "gross_edge_usd": ev.gross_edge_usd(),
                "protocol_flat_fee_lamports": ev.protocol_flat_fee_lamports,
                "tx_fee_lamports": e.tx_fee_lamports,
                "fee_payer": e.fee_payer,
                "receiver_lamport_delta": e.receiver_lamport_delta.map(|d| d.to_string()),
                "receiver_token_deltas": token_deltas,
                "receiver_token_deltas_complete": e.token_deltas_complete,
                "cu_limit": e.cu_limit,
                "cu_price_micro_lamports": e.cu_price_micro_lamports,
                "transaction_success": true,
                "enrichment_status": enrichment_status,
            }))?;
        }
        Ok(())
    }

    /// Route one ingress event (everything except the first StreamReady, which opens the window).
    pub fn on_ingress_event(
        &mut self,
        ev: IngressEvent,
        now_unix_s: u64,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<()> {
        let before = self.econ.as_ref().map(|_| self.stats.clone());
        let at = ev.t_rx_ns();
        let result = match ev {
            IngressEvent::Health(h) => self.coverage(
                sink,
                "TRANSPORT_HEALTH",
                now_unix_s,
                Some(h.t_rx_ns),
                json!({"connected":h.connected,"last_application_frame_age_ns":h.last_frame_age_ns,
                    "application_frames":h.frames,"last_slot":h.last_slot}),
            ),
            IngressEvent::Transport {
                state,
                code,
                attempt,
                last_slot,
                t_rx_ns,
            } => {
                if state == "RECOVERING" {
                    self.stats.transport_reconnect_attempts += 1;
                }
                if state == "RECOVERED" {
                    self.stats.transport_recovered += 1;
                }
                if code == "WSS_HTTP_429" {
                    self.stats.transport_http_429 += 1;
                }
                if state == "INCOMPLETE_ABORT" {
                    self.mark_incomplete("WSS_RECOVERY_EXHAUSTED");
                }
                self.coverage(sink, "TRANSPORT_STATE", now_unix_s, Some(t_rx_ns),
                    json!({"state":state,"code":code,"attempt":attempt,"last_valid_slot":last_slot}))
            }
            IngressEvent::Reconnected {
                attempt, t_rx_ns, ..
            } => self.coverage(
                sink,
                "RECONNECT",
                now_unix_s,
                Some(t_rx_ns),
                json!({"attempt": attempt}),
            ),
            IngressEvent::Gap {
                from_slot,
                to_slot,
                t_rx_ns,
                ..
            } => self.coverage(
                sink,
                "GAP",
                now_unix_s,
                Some(t_rx_ns),
                json!({"from_slot": from_slot, "to_slot": to_slot}),
            ),
            IngressEvent::Fatal {
                detail, t_rx_ns, ..
            } => self.coverage(
                sink,
                "FATAL",
                now_unix_s,
                Some(t_rx_ns),
                json!({"detail": detail}),
            ),
            IngressEvent::Transaction(t) if self.source == MarketSource::Transactions => {
                self.on_transaction(&t, now_unix_s, sink)
            }
            IngressEvent::Logs(l) if self.source == MarketSource::Logs => {
                self.on_logs(&l, now_unix_s, sink)
            }
            IngressEvent::Slot(s) => {
                let progress = self
                    .reconciliation
                    .market_slot_end
                    .as_ref()
                    .is_none_or(|b| s.slot > b.slot);
                self.record_slot_boundary(s.slot, s.t_rx_ns, now_unix_s, sink)?;
                self.close_gap_ranges(s.slot, s.t_rx_ns, now_unix_s, sink)?;
                if let Some(h) = &self.econ {
                    h.emit(crate::econ::Evidence::Slot {
                        slot: s.slot,
                        t_rx_ns: s.t_rx_ns,
                    });
                }
                if progress && self.in_window(Some(s.t_rx_ns)) {
                    self.last_slot_progress_ns = Some(s.t_rx_ns);
                }
                self.on_slot_frame(s.t_rx_ns);
                Ok(())
            }
            _ => Ok(()),
        };
        if let (Some(h), Some(before)) = (&self.econ, before) {
            h.emit(crate::econ::Evidence::MarketCounters {
                at_ns: at,
                before,
                after: Box::new(self.stats.clone()),
            });
        }
        result
    }

    /// `transactions` source: the full payload is the primary raw evidence; no RPC.
    pub fn on_transaction(
        &mut self,
        t: &TransactionObserved,
        now_unix_s: u64,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<()> {
        let success = t.err.is_none();
        if !self.admit(&t.signature, t.slot, success, t.t_rx_ns) {
            return Ok(());
        }
        sink.transaction(&json!({
            "observed_at_utc": utc_iso(now_unix_s),
            "t_rx_ns": t.t_rx_ns,
            "slot": t.slot,
            "signature": t.signature,
            "tx_version": format!("{:?}", t.version),
            "success": success,
            "err": t.err,
            "source": "transactionSubscribe",
            "raw": t.raw,
        }))?;
        let tx = t.raw.get("transaction");
        let logs: Option<Vec<String>> = tx
            .and_then(|x| x.pointer("/meta/logMessages"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|l| l.as_str().map(str::to_string))
                    .collect()
            });
        let Some(logs) = logs else {
            if success {
                self.stats.log_messages_missing += 1;
                self.mark_incomplete("LOG_MESSAGES_MISSING");
            }
            return Ok(());
        };
        let decoded = decode_receiver_events(&logs, &self.program);
        self.absorb_decode(&decoded, success);
        if success && decoded.truncated {
            // [r6] the payload already carries the full transaction: qualify immediately, no RPC.
            let logged = self.note_truncated(&decoded);
            let q = match tx {
                Some(Value::Object(obj)) => {
                    let mut with_slot = obj.clone();
                    with_slot.insert("slot".to_string(), json!(t.slot));
                    qualify_truncated(
                        Value::Object(with_slot),
                        &t.signature,
                        t.slot,
                        &self.program,
                        &logged,
                    )
                }
                _ => TruncationQualification::Unqualified("TX_DECODE"),
            };
            self.apply_truncation(q);
        }
        if !success || decoded.events.is_empty() {
            return Ok(());
        }
        self.aggregate(
            &t.signature,
            t.slot,
            Some(t.t_rx_ns),
            t.tx_index_in_slot,
            &decoded.events,
        );
        self.stats.enrichment_requested += 1;
        let meta_available = tx.and_then(|x| x.get("meta")).is_some_and(Value::is_object);
        let status = if meta_available {
            self.stats.enrichment_succeeded += 1;
            "PAYLOAD"
        } else {
            self.stats.enrichment_failed += 1;
            "PAYLOAD_META_MISSING"
        };
        self.write_liquidation_rows(
            sink,
            &t.signature,
            t.slot,
            Some(t.t_rx_ns),
            now_unix_s,
            &decoded.events,
            tx,
            status,
        )
    }

    /// `logs` source: economics from logs immediately; enrichment deferred until after capture.
    pub fn on_logs(
        &mut self,
        l: &LogsObserved,
        now_unix_s: u64,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<()> {
        let success = l.err.is_none();
        if !self.admit(&l.signature, l.slot, success, l.t_rx_ns) {
            return Ok(());
        }
        sink.transaction(&json!({
            "observed_at_utc": utc_iso(now_unix_s),
            "t_rx_ns": l.t_rx_ns,
            "slot": l.slot,
            "signature": l.signature,
            "tx_version": Value::Null,
            "success": success,
            "err": l.err,
            "source": "logsSubscribe",
            "raw": l.raw,
        }))?;
        // A timeboxed diagnostic can be terminated before graceful finalization.
        // Make each admitted live signature visible on disk immediately rather
        // than letting the final BufWriter batch disappear on SIGTERM.
        sink.flush()?;
        if l.logs.is_empty() {
            if success {
                self.stats.log_messages_missing += 1;
                self.mark_incomplete("LOG_MESSAGES_MISSING");
            }
            return Ok(());
        }
        let decoded = decode_receiver_events(&l.logs, &self.program);
        self.absorb_decode(&decoded, success);
        if success {
            for ev in &decoded.additional_events {
                match ev.kind {
                    truth::LiquidationType::Classic => self.stats.provisional_classic_events += 1,
                    truth::LiquidationType::ForcedDeleverage => {
                        self.stats.provisional_deleverage_events += 1
                    }
                    truth::LiquidationType::BankruptcySettlement => {
                        self.stats.provisional_bankruptcy_events += 1
                    }
                    truth::LiquidationType::Receivership => {
                        unreachable!("original receiver decoder handles this type")
                    }
                }
                eprintln!(
                    "[MARKET/LIQUIDATION] provisional=true type={} slot={} signature={} target={} actor={} gross_usd={}",
                    ev.kind.as_str(), l.slot, l.signature, ev.target, ev.actor,
                    ev.gross_edge_usd.map(|v| format!("{v:.6}")).unwrap_or_else(|| "UNKNOWN".to_owned())
                );
            }
        }
        if success && decoded.truncated {
            // [r6] qualified after capture against the fetched transaction (bounded, counted).
            let logged = self.note_truncated(&decoded);
            if self.truncated_pending.len() >= self.enrich_cfg.max_pending {
                self.apply_truncation(TruncationQualification::Unqualified("QUEUE_FULL"));
            } else {
                self.truncated_pending.push_back(TruncatedPending {
                    signature: l.signature.clone(),
                    slot: l.slot,
                    logged,
                });
            }
        }
        if !success || decoded.events.is_empty() {
            return Ok(());
        }
        self.aggregate(&l.signature, l.slot, Some(l.t_rx_ns), None, &decoded.events);
        // The accounting ledger remains deferred for exact post-capture enrichment.
        // The operator must still see every successfully decoded live liquidation NOW.
        for ev in &decoded.events {
            eprintln!(
                "[MARKET/RECEIVERSHIP] provisional=true slot={} signature={} event={} target={} receiver={} gross_usd={:.6}",
                l.slot, l.signature, ev.event_index, ev.target, ev.receiver, ev.gross_edge_usd()
            );
        }
        self.stats.enrichment_requested += 1;
        if self.deferred.len() >= self.enrich_cfg.max_pending {
            self.stats.enrichment_failed += 1;
            self.stats.enrichment_skipped_queue_full += 1;
            return self.write_liquidation_rows(
                sink,
                &l.signature,
                l.slot,
                Some(l.t_rx_ns),
                now_unix_s,
                &decoded.events,
                None,
                "SKIPPED_QUEUE_FULL",
            );
        }
        self.deferred.push_back(DeferredLiquidation {
            signature: l.signature.clone(),
            slot: l.slot,
            t_rx_ns: l.t_rx_ns,
            observed_unix_s: now_unix_s,
            events: decoded.events,
        });
        Ok(())
    }

    /// Post-capture enrichment of every deferred liquidation. Never runs during capture.
    /// `fetch` = one `getTransaction` attempt, `wait` = pause between attempts,
    /// `budget_left` = phase time budget. Captured gross edge is never removed.
    pub fn enrich_deferred(
        &mut self,
        sink: &mut dyn MarketSink,
        fetch: &mut dyn FnMut(&str) -> Result<Option<Value>, String>,
        wait: &mut dyn FnMut(Duration),
        budget_left: &mut dyn FnMut() -> bool,
    ) -> std::io::Result<()> {
        let retry = Duration::from_millis(self.enrich_cfg.retry_ms);
        let max_attempts = self.enrich_cfg.max_attempts;
        while let Some(p) = self.deferred.pop_front() {
            let mut status = "UNRESOLVED";
            let mut resolved: Option<Value> = None;
            for attempt in 1..=max_attempts {
                if !budget_left() {
                    status = "SKIPPED_PHASE_TIMEOUT";
                    break;
                }
                self.stats.rpc_fallback_count += 1;
                if let Ok(Some(tx)) = fetch(&p.signature) {
                    if verify_tx_identity(&tx, &p.signature, p.slot).is_ok() {
                        resolved = Some(tx);
                    } else {
                        status = "IDENTITY_MISMATCH";
                    }
                    break;
                }
                if attempt < max_attempts {
                    wait(retry);
                }
            }
            match resolved {
                Some(tx) => {
                    self.stats.enrichment_succeeded += 1;
                    self.write_liquidation_rows(
                        sink,
                        &p.signature,
                        p.slot,
                        Some(p.t_rx_ns),
                        p.observed_unix_s,
                        &p.events,
                        Some(&tx),
                        "RPC_GET_TRANSACTION",
                    )?;
                }
                None => {
                    self.stats.enrichment_failed += 1;
                    self.write_liquidation_rows(
                        sink,
                        &p.signature,
                        p.slot,
                        Some(p.t_rx_ns),
                        p.observed_unix_s,
                        &p.events,
                        None,
                        status,
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Phase 3 entry point. Never fails the run because of the enrichment client: an init error
    /// is recorded (coverage + summary) and all deferred liquidations are written unenriched, so
    /// captured gross edge and the final report are always produced.
    pub fn enrichment_phase(
        &mut self,
        sink: &mut dyn MarketSink,
        fetcher: EnrichmentFetcher<'_>,
        wait: &mut dyn FnMut(Duration),
        budget_left: &mut dyn FnMut() -> bool,
        now_unix_s: u64,
    ) -> std::io::Result<()> {
        match fetcher {
            Ok(fetch) => self.enrich_deferred(sink, fetch, wait, budget_left),
            Err(e) => {
                self.coverage(
                    sink,
                    "ENRICHMENT_INIT_FAILED",
                    now_unix_s,
                    None,
                    json!({"error": e}),
                )?;
                self.enrichment_init_error = Some(e);
                self.write_deferred_unenriched(sink, "ENRICHMENT_INIT_FAILED")
            }
        }
    }

    /// Write every still-deferred liquidation without enrichment (economics already counted).
    pub fn write_deferred_unenriched(
        &mut self,
        sink: &mut dyn MarketSink,
        status: &str,
    ) -> std::io::Result<()> {
        while let Some(p) = self.deferred.pop_front() {
            self.stats.enrichment_failed += 1;
            self.write_liquidation_rows(
                sink,
                &p.signature,
                p.slot,
                Some(p.t_rx_ns),
                p.observed_unix_s,
                &p.events,
                None,
                status,
            )?;
        }
        Ok(())
    }

    /// Ranking by liquidation events desc, then gross edge desc, then receiver asc.
    pub fn receiver_ranking(&self) -> Vec<ReceiverRank> {
        let mut v: Vec<ReceiverRank> = self
            .receivers
            .iter()
            .map(|(r, (n, g))| ReceiverRank {
                receiver: r.clone(),
                liquidation_events: *n,
                gross_edge_total_usd: *g,
            })
            .collect();
        v.sort_by(|a, b| {
            b.liquidation_events
                .cmp(&a.liquidation_events)
                .then_with(|| b.gross_edge_total_usd.total_cmp(&a.gross_edge_total_usd))
                .then_with(|| a.receiver.cmp(&b.receiver))
        });
        v
    }

    pub fn summary(&self, cfg: &SummaryContext) -> Value {
        let s = &self.stats;
        let observed_end = self.window.map(|(start, cutoff)| {
            start
                .saturating_add((cfg.actual_observation_seconds.max(0.0) * 1e9) as u64)
                .min(cutoff)
        });
        let outage = observed_end.and_then(|end| self.transport_outage_ns(end));
        let mut sorted = self.edges.clone();
        sorted.sort_by(f64::total_cmp);
        let total: f64 = sorted.iter().sum();
        let n = sorted.len();
        let hours = cfg.actual_observation_seconds / 3_600.0;
        let per_hour = |x: f64| if hours > 0.0 { Some(x / hours) } else { None };
        let ranking = self.receiver_ranking();
        let total_events = s.liquidation_events as f64;
        let mut by_gross = ranking.clone();
        by_gross.sort_by(|a, b| {
            b.gross_edge_total_usd
                .total_cmp(&a.gross_edge_total_usd)
                .then_with(|| a.receiver.cmp(&b.receiver))
        });
        let top = ranking.first();
        let top3_events: u64 = ranking.iter().take(3).map(|r| r.liquidation_events).sum();
        let top3_gross: f64 = by_gross
            .iter()
            .take(3)
            .map(|r| r.gross_edge_total_usd)
            .sum();
        let coverage_complete = self.coverage_complete();
        let decode_complete = self.decode_complete();
        let receivership_transport_capture_complete = coverage_complete
            && decode_complete
            && cfg.full_duration_observed
            && cfg.drain_complete;
        // Master contract: transport completeness is no longer whole-market proof.
        // Until the full-slot/all-liquidation-path engine is qualified, fail closed.
        let full_slot_reconciliation_complete = self.reconciliation.complete;
        let liquidation_path_coverage_complete = paths::verified(
            self.deployment_before.as_ref(),
            self.deployment_after.as_ref(),
            self.reconciliation.market_slot_end.as_ref().map(|b| b.slot),
        );
        let market_capture_complete = cfg.stream_ready
            && cfg.full_duration_observed
            && cfg.drain_complete
            && full_slot_reconciliation_complete
            && !self.truth.retention_error
            && liquidation_path_coverage_complete;
        let source_coverage_verified =
            full_slot_reconciliation_complete && liquidation_path_coverage_complete;
        let enrichment_complete = self.enrichment_complete();
        let mut reasons: Vec<&str> = self.incomplete_reasons.iter().copied().collect();
        if !full_slot_reconciliation_complete {
            reasons.push(
                self.reconciliation
                    .error
                    .unwrap_or("FULL_SLOT_RECONCILIATION_PENDING"),
            );
        }
        if !liquidation_path_coverage_complete {
            reasons.push(
                self.deployment_error
                    .unwrap_or("AUDITED_DEPLOYMENT_CONTINUITY_UNPROVEN"),
            );
        }
        if cfg.stream_ready
            && !cfg.full_duration_observed
            && !reasons.contains(&"OBSERVATION_ENDED_EARLY")
        {
            reasons.push("OBSERVATION_ENDED_EARLY");
        }
        if cfg.stream_ready && !cfg.drain_complete && !reasons.contains(&"DRAIN_INCOMPLETE") {
            reasons.push("DRAIN_INCOMPLETE");
        }
        let verdict = market_verdict(
            cfg.stream_ready,
            market_capture_complete,
            source_coverage_verified,
        );
        let avg = if n > 0 { Some(total / n as f64) } else { None };
        let sample_status = if s.liquidation_events < cfg.min_sample_liquidations {
            "INSUFFICIENT_SAMPLE"
        } else {
            "SAMPLE_SUFFICIENT"
        };
        let receivers: Vec<Value> = ranking
            .iter()
            .map(|r| {
                json!({
                    "receiver": r.receiver,
                    "liquidation_events": r.liquidation_events,
                    "gross_edge_total_usd": r.gross_edge_total_usd,
                    "liquidation_share": ratio(r.liquidation_events as f64, total_events),
                    "gross_edge_share": ratio(r.gross_edge_total_usd, total),
                })
            })
            .collect();
        json!({
            "title": "PROJECT 0 MARKET OBSERVER — FINAL",
            "run_id": cfg.run_id,
            "source": self.source.as_str(),
            "commitment": cfg.commitment,
            "program": self.program.to_string(),
            "process_started_at": cfg.process_started_at,
            "stream_ready_at": cfg.stream_ready_at,
            "observation_started_at": cfg.observation_started_at,
            "observation_ended_at": cfg.observation_ended_at,
            "configured_duration_seconds": cfg.configured_duration_seconds,
            "actual_observation_seconds": cfg.actual_observation_seconds,
            "verdict": verdict,
            "market_capture_complete": market_capture_complete,
            "receivership_transport_capture_complete": receivership_transport_capture_complete,
            "full_slot_reconciliation_complete": full_slot_reconciliation_complete,
            "full_slot_reconciliation": self.reconciliation,
            "canonical_market_truth": self.truth.summary(self.reconciliation.first_block_time.zip(self.reconciliation.last_block_time).map(|(a,b)|(b-a) as f64)),
            "liquidation_path_coverage_complete": liquidation_path_coverage_complete,
            "liquidation_count_scope": "RECEIVERSHIP_ONLY_NOT_TOTAL_MARGINFI_MARKET",
            "live_source_coverage_verified": self.source.coverage_verified(),
            "deployment_continuity":{"before":self.deployment_before,"after":self.deployment_after,"error":self.deployment_error,"source_pin":"33c67987a661f59c6683a65485ebb4f0ef030b77","bytecode_source_equivalence":"NOT_ATTESTED; pinned source is contract authority; hash continuity against existing upgrade-watch reference"},
            "completeness_authority":"FULL_FINALIZED_SLOT_RECONCILIATION_AND_PATH_QUALIFICATION",
            "transport_issues_are_telemetry":true,
            "transport_incomplete_reasons":self.incomplete_reasons,
            "coverage_complete": coverage_complete,
            "decode_complete": decode_complete,
            "drain_complete": cfg.drain_complete,
            "live_coverage_status": if receivership_transport_capture_complete { "LIVE_COVERAGE_COMPLETE" } else { "LIVE_COVERAGE_INCOMPLETE" },
            "capture_clock_complete": cfg.stream_ready && cfg.full_duration_observed,
            "verification_status": if full_slot_reconciliation_complete { "CANONICAL_VERIFIED" } else if self.reconciliation.attempted { "HISTORICAL_INCOMPLETE" } else { "NOT_RUN" },
            "source_coverage_verified": source_coverage_verified,
            "source_coverage_note": if source_coverage_verified {"Finalized full-slot scan is authoritative; live WSS source completeness remains separately unverified"} else {self.source.coverage_note()},
            "incomplete_reasons": reasons,
            "first_observed_slot": s.first_observed_slot,
            "last_observed_slot": s.last_observed_slot,
            "transactions_received": s.transactions_received,
            "unique_transactions": s.unique_transactions,
            "duplicate_transactions": s.duplicate_transactions,
            "successful_transactions": s.successful_transactions,
            "failed_transactions": s.failed_transactions,
            "liquidation_transactions": s.liquidation_transactions,
            "liquidation_events": s.liquidation_events,
            "liquidations_per_hour": per_hour(total_events),
            "gross_edge_total_usd": total,
            "gross_edge_per_hour_usd": per_hour(total),
            "gross_edge_avg_usd": avg,
            "gross_edge_per_liquidation_usd": avg,
            "sample_status": sample_status,
            "sample_min_liquidation_events": cfg.min_sample_liquidations,
            "unique_targets": self.targets.len(),
            "enrichment_available": cfg.enrichment_available,
            "gross_edge_median_usd": quantile_nearest_rank(&sorted, 0.5),
            "gross_edge_p90_usd": quantile_nearest_rank(&sorted, 0.9),
            "gross_edge_p99_usd": quantile_nearest_rank(&sorted, 0.99),
            "gross_edge_max_usd": sorted.last().copied(),
            "gross_edge_min_usd": sorted.first().copied(),
            "quantile_method": QUANTILE_METHOD,
            "protocol_flat_fee_total_lamports": s.protocol_flat_fee_total_lamports,
            "tx_fee_total_lamports": s.tx_fee_total_lamports,
            "tx_fee_unknown_transactions": s.tx_fee_unknown_transactions,
            "fee_totals_complete": enrichment_complete && s.tx_fee_unknown_transactions == 0,
            "enrichment_complete": enrichment_complete,
            "enrichment_requested": s.enrichment_requested,
            "enrichment_succeeded": s.enrichment_succeeded,
            "enrichment_failed": s.enrichment_failed,
            "enrichment_skipped_queue_full": s.enrichment_skipped_queue_full,
            "enrichment_init_error": self.enrichment_init_error,
            "rpc_fallback_count": s.rpc_fallback_count,
            "unique_receivers": ranking.len(),
            "top_receiver": top.map(|r| r.receiver.clone()),
            "top_receiver_liquidation_share": top.and_then(|r| ratio(r.liquidation_events as f64, total_events)),
            "top_receiver_gross_edge_share": top.and_then(|r| ratio(r.gross_edge_total_usd, total)),
            "top3_receiver_liquidation_share": ratio(top3_events as f64, total_events),
            "top3_receiver_gross_edge_share": ratio(top3_gross, total),
            "gap_count": s.gap_count,
            "gap_recovery_attempted": self.gap_stats.gap_recovery_attempted,
            "gap_recovered": self.gap_stats.gap_recovered,
            "gap_unrecovered": (self.gaps.len() as u64).saturating_sub(self.gap_stats.gap_recovered),
            "gap_intervals": self.gaps.len(),
            "coverage_outage_seconds": outage.map(|ns| ns as f64/1e9),
            "transport_covered_seconds": outage.map(|ns| (cfg.actual_observation_seconds-ns as f64/1e9).max(0.0)),
            "transport_coverage_provenance":"LOCAL_SLOT_PROGRESS_AND_JOURNALED_OUTAGE_INTERVALS_NOT_SOURCE_COMPLETENESS",
            "last_live_slot_age_s": observed_end.zip(self.last_slot_progress_ns).map(|(end,last)| end.saturating_sub(last) as f64/1e9),
            "gap_count_raw": s.gap_count,
            "gaps_merged": self.gaps.iter().map(|g| g.raw_failures.saturating_sub(1)).sum::<u64>(),
            "provider_budget": darkbot_solana::provider::shared().snapshot(),
            "gap_slots_requested": self.gap_stats.gap_slots_requested,
            "gap_slots_fetched": self.gap_stats.gap_slots_fetched,
            "gap_slots_skipped_confirmed": self.gap_stats.gap_slots_skipped_confirmed,
            "gap_slots_failed": self.gap_stats.gap_slots_failed,
            "gap_transactions_scanned": self.gap_stats.gap_transactions_scanned,
            "gap_transactions_deduplicated": self.gap_stats.gap_transactions_deduplicated,
            "gap_transactions_recovered": self.gap_stats.gap_transactions_recovered,
            "gap_liquidation_transactions": self.gap_stats.gap_liquidation_transactions,
            "gap_liquidation_events": self.gap_stats.gap_liquidation_events,
            "gaps": self.gaps,
            "reconnect_count": s.reconnect_count,
            "reconnect_attempts": s.transport_reconnect_attempts,
            "reconnect_successful": s.transport_recovered,
            "wss_http_429_count": s.transport_http_429,
            "ingress_overflow_count": s.ingress_overflow_count,
            "fatal_count": s.fatal_count,
            "decoder_error_count": s.decoder_error_count,
            "decoder_errors_successful_tx": s.decoder_errors_successful_tx,
            "base64_error_count": s.base64_error_count,
            "foreign_program_events": s.foreign_program_events,
            "failed_tx_receiver_events_ignored": s.failed_tx_receiver_events_ignored,
            "logs_truncated": s.logs_truncated,
            "logs_truncated_total": s.logs_truncated,
            "logs_truncated_qualified": s.logs_truncated_qualified,
            "logs_truncated_unqualified": s.logs_truncated_unqualified,
            "logs_truncated_liquidation_mismatch": s.logs_truncated_liquidation_mismatch,
            "truncated_instruction_end_liquidations": s.truncated_instruction_end_liquidations,
            "truncated_log_end_liquidations": s.truncated_log_end_liquidations,
            "truncated_unqualified_reasons": self.truncated_unqualified_reasons,
            "log_messages_missing": s.log_messages_missing,
            "events_outside_observation": s.events_outside_observation,
            "events_after_cutoff": s.events_after_cutoff,
            "slot_notifications": s.slot_notifications,
            "max_stream_silence_ms": s.max_stream_silence_ms,
            "receivers": receivers,
            "non_claims": [
                "market gross edge only (seized - repaid, USD, protocol valuation)",
                "not DarkBot executable edge (P1-C/F1/F8)",
                "not DarkBot capture rate",
                "not DarkBot realized PnL (P8)",
                "no per-day/month/year extrapolation from a single run",
            ],
        })
    }
}

/// Run facts the core cannot know itself.
#[derive(Clone, Debug, Default)]
pub struct SummaryContext {
    pub run_id: String,
    pub commitment: String,
    pub process_started_at: String,
    pub stream_ready_at: Option<String>,
    pub observation_started_at: Option<String>,
    pub observation_ended_at: Option<String>,
    pub configured_duration_seconds: f64,
    pub actual_observation_seconds: f64,
    pub stream_ready: bool,
    pub full_duration_observed: bool,
    pub drain_complete: bool,
    pub min_sample_liquidations: u64,
    pub enrichment_available: bool,
}

const REPORT_KEYS: &[&str] = &[
    "run_id",
    "source",
    "commitment",
    "program",
    "configured_duration_seconds",
    "measurement_plan",
    "capture_window",
    "actual_observation_seconds",
    "verdict",
    "market_capture_complete",
    "coverage_complete",
    "decode_complete",
    "drain_complete",
    "source_coverage_verified",
    "source_coverage_note",
    "incomplete_reasons",
    "first_observed_slot",
    "last_observed_slot",
    "transactions_received",
    "unique_transactions",
    "duplicate_transactions",
    "successful_transactions",
    "failed_transactions",
    "liquidation_transactions",
    "liquidation_events",
    "sample_status",
    "liquidations_per_hour",
    "unique_targets",
    "gross_edge_total_usd",
    "gross_edge_per_hour_usd",
    "gross_edge_per_liquidation_usd",
    "gross_edge_median_usd",
    "gross_edge_p90_usd",
    "gross_edge_p99_usd",
    "gross_edge_max_usd",
    "gross_edge_min_usd",
    "protocol_flat_fee_total_lamports",
    "tx_fee_total_lamports",
    "fee_totals_complete",
    "enrichment_available",
    "enrichment_complete",
    "enrichment_requested",
    "enrichment_succeeded",
    "enrichment_failed",
    "unique_receivers",
    "top_receiver",
    "top_receiver_liquidation_share",
    "top_receiver_gross_edge_share",
    "top3_receiver_liquidation_share",
    "top3_receiver_gross_edge_share",
    "gap_count",
    "reconnect_count",
    "ingress_overflow_count",
    "decoder_error_count",
    "logs_truncated_total",
    "logs_truncated_qualified",
    "logs_truncated_unqualified",
    "logs_truncated_liquidation_mismatch",
    "truncated_instruction_end_liquidations",
    "truncated_log_end_liquidations",
    "truncated_unqualified_reasons",
    "rpc_fallback_count",
    "events_after_cutoff",
    "max_stream_silence_ms",
];

pub fn render_report(summary: &Value, files: &[(&str, &Path)]) -> String {
    let rule = "=".repeat(72);
    let mut out = format!("{rule}\nPROJECT 0 MARKET OBSERVER — FINAL\n\n");
    for k in REPORT_KEYS {
        let v = summary.get(*k).cloned().unwrap_or(Value::Null);
        let shown = match v {
            Value::String(s) => s,
            Value::Number(n) => n
                .as_f64()
                .filter(|_| n.is_f64())
                .map(|f| format!("{f:.6}"))
                .unwrap_or_else(|| n.to_string()),
            Value::Null => "null".to_string(),
            other => other.to_string(),
        };
        out.push_str(&format!("{k}={shown}\n"));
    }
    out.push('\n');
    for (name, path) in files {
        out.push_str(&format!("{name}={}\n", path.display()));
    }
    out.push_str(&rule);
    out.push('\n');
    out
}

/// Finalise: write any still-deferred liquidation, STOPPED, flush, `summary.json`, `final_report.txt`.
pub fn finalize(
    core: &mut ObserverCore,
    sink: &mut dyn MarketSink,
    ctx: &SummaryContext,
    run_dir: &Path,
    now_unix_s: u64,
    detail: Value,
) -> Result<(Value, String), String> {
    core.write_deferred_unenriched(sink, "NOT_ENRICHED")
        .map_err(|e| format!("liquidations write: {e}"))?;
    core.fail_truncated_pending("NOT_QUALIFIED");
    core.coverage(sink, "STOPPED", now_unix_s, None, detail)
        .map_err(|e| format!("coverage write: {e}"))?;
    sink.flush().map_err(|e| format!("flush: {e}"))?;
    let mut summary = core.summary(ctx);
    if let Some(h) = &core.econ {
        summary["measurement_plan"] =
            serde_json::to_value(&h.cfg.plan).map_err(|e| e.to_string())?;
        summary["capture_window"] = if let Some(cutoff) = h.cutoff() {
            h.cfg.plan.capture_window(
                h.clock,
                cutoff - h.cfg.plan.duration_seconds * crate::measurement::NS,
            )?
        } else {
            Value::Null
        };
    }
    let summary_path = run_dir.join("summary.json");
    let pretty = serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?;
    fs::write(&summary_path, pretty).map_err(|e| format!("summary.json: {e}"))?;
    let files = [
        ("transactions_jsonl", run_dir.join("transactions.jsonl")),
        ("liquidations_jsonl", run_dir.join("liquidations.jsonl")),
        ("coverage_jsonl", run_dir.join("coverage.jsonl")),
        ("summary_json", summary_path.clone()),
    ];
    let refs: Vec<(&str, &Path)> = files.iter().map(|(n, p)| (*n, p.as_path())).collect();
    let report = render_report(&summary, &refs);
    fs::write(run_dir.join("final_report.txt"), &report)
        .map_err(|e| format!("final_report.txt: {e}"))?;
    Ok((summary, report))
}

// ------------------------------------------------------------------------------------------
// Bounded post-capture getTransaction (logs source). No hidden retry, no RPC during capture.
// ------------------------------------------------------------------------------------------

struct RpcFetcher {
    client: reqwest::blocking::Client,
    url: String,
    commitment: &'static str,
}

impl RpcFetcher {
    fn new(url: String, timeout_ms: u64, commitment: &'static str) -> Result<Self, String> {
        let limits = &darkbot_solana::runtime_config::active().limits;
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_millis(limits.rpc_connect_timeout_ms))
            .timeout(Duration::from_millis(timeout_ms.min(limits.rpc_timeout_ms)))
            .build()
            .map_err(|e| format!("enrichment client: {}", e.without_url()))?;
        Ok(Self {
            client,
            url,
            commitment,
        })
    }

    fn get_transaction(&self, signature: &str) -> Result<Option<Value>, String> {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": "getTransaction",
            "params": [signature, {"encoding": "json", "commitment": self.commitment, "maxSupportedTransactionVersion": 1}]});
        let _permit = darkbot_solana::provider::shared()
            .acquire(
                darkbot_solana::provider::Workload::History,
                Instant::now()
                    + Duration::from_millis(
                        darkbot_solana::runtime_config::active()
                            .limits
                            .rpc_timeout_ms,
                    ),
            )
            .map_err(str::to_string)?;
        let budget = darkbot_solana::provider::shared();
        budget
            .account_bytes(serde_json::to_vec(&body).map_err(|_| "RPC_ENCODE")?.len())
            .map_err(str::to_string)?;
        let resp = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .map_err(|e| e.without_url().to_string())?;
        darkbot_solana::provider::observe_response(&resp, "getTransaction")
            .map_err(str::to_string)?;
        let bytes = darkbot_solana::provider::read_body(
            resp,
            darkbot_solana::runtime_config::active()
                .limits
                .max_response_mib
                * 1024
                * 1024,
            &budget,
        )
        .map_err(str::to_string)?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| "RPC_JSON")?;
        if let Some(err) = v.get("error") {
            let _ = err;
            return Err("RPC_REMOTE_ERROR".into());
        }
        Ok(v.get("result").filter(|r| !r.is_null()).cloned())
    }
}

// ------------------------------------------------------------------------------------------
// Production run
// ------------------------------------------------------------------------------------------

fn print_banner(cfg: &MarketConfig) {
    println!("PROJECT 0 — UNFILTERED LIVE MARKET OBSERVER");
    println!("mode=market");
    println!("read_only=true");
    println!("signing=false");
    println!("simulation=false");
    println!("submission=false");
    println!("health_filter=false");
    println!("candidate_filter=false");
    println!("economics_filter=false");
    println!("routing_filter=false");
    println!("funding_filter=false");
    println!("source={}", cfg.source.as_str());
    println!("commitment={}", cfg.commitment);
    println!(
        "source_coverage_verified={}",
        cfg.source.coverage_verified()
    );
    println!("heartbeat_seconds={}", cfg.heartbeat_secs);
    println!(
        "enrichment_available={}",
        cfg.source == MarketSource::Transactions || cfg.rpc_url.is_some()
    );
    println!("duration_seconds={}", cfg.duration_secs);
    println!("program={}", cfg.program);
    println!("ws_host={}", redact_url(&cfg.ws_url));
    if let Some(rpc) = &cfg.rpc_url {
        println!("rpc_host={}", redact_url(rpc));
    }
    println!("output={}", cfg.run_dir.display());
}

/// Facts handed from the capture/drain phases to the shared finishing path.
pub struct CaptureOutcome {
    pub process_started_unix_s: u64,
    pub ready_unix_s: Option<u64>,
    /// `(start_t_rx_ns, cutoff_t_rx_ns)` if the stream became ready.
    pub window: Option<(u64, u64)>,
    pub capture_end_ns: u64,
    pub observation_end_unix_s: u64,
    pub stop_reason: &'static str,
    pub drain_complete: bool,
    pub write_error: Option<String>,
}

/// Production finishing path used by `run()`: phase 3 (enrichment, logs only), summary and
/// report. A missing `P0_RPC_URL` or a failing enrichment client is recorded
/// (`ENRICHMENT_INIT_FAILED`, `enrichment_init_error`) and never prevents summary/report.
/// Returns `(summary, report, write_error)`.
pub fn finish_capture(
    cfg: &MarketConfig,
    core: &mut ObserverCore,
    sink: &mut dyn MarketSink,
    outcome: CaptureOutcome,
) -> Result<(Value, String, Option<String>), String> {
    let write_error = outcome.write_error;
    // Outage abort is not a request for a lengthy repair scan. Preserve evidence
    // for explicit resumable verification; stop capture/Shadow first.
    let verify_allowed = outcome.stop_reason == "TIMER";
    if !verify_allowed {
        core.mark_incomplete("TRANSPORT_CAPTURE_ABORT");
        core.reconciliation.error = Some("VERIFICATION_DEFERRED_AFTER_ABORT");
    }
    // Capture finalization is local-only. Historical repair/enrichment belongs
    // to the separately budgeted verifier after the immutable raw manifest.
    // Keep every pending gap/truncated record as evidence, never mark repaired.
    if verify_allowed {
        core.reconciliation.error = Some("VERIFICATION_NOT_RUN");
    }
    let ready = outcome.ready_unix_s;
    let ctx = SummaryContext {
        run_id: cfg.run_id.clone(),
        commitment: cfg.commitment.to_string(),
        process_started_at: utc_iso(outcome.process_started_unix_s),
        stream_ready_at: ready.map(utc_iso),
        observation_started_at: ready.map(utc_iso),
        observation_ended_at: ready.map(|_| utc_iso(outcome.observation_end_unix_s)),
        configured_duration_seconds: cfg.duration_secs as f64,
        actual_observation_seconds: outcome
            .window
            .map(|(start, cutoff)| {
                (outcome.capture_end_ns.min(cutoff).saturating_sub(start)) as f64 / 1e9
            })
            .unwrap_or(0.0),
        stream_ready: outcome.window.is_some(),
        full_duration_observed: outcome.stop_reason == "TIMER" && outcome.window.is_some(),
        drain_complete: outcome.drain_complete,
        min_sample_liquidations: cfg.min_sample_liquidations,
        enrichment_available: cfg.source == MarketSource::Transactions || cfg.rpc_url.is_some(),
    };
    if core.deployment_before.is_some() {
        core.deployment_error = Some("POST_CAPTURE_DEPLOYMENT_VERIFICATION_NOT_RUN");
    }
    let (summary, report) = finalize(
        core,
        sink,
        &ctx,
        &cfg.run_dir,
        unix_now_s(),
        json!({
            "reason": outcome.stop_reason, "drain_complete": outcome.drain_complete, "write_error": write_error.clone(),
        }),
    )?;
    Ok((summary, report, write_error))
}

pub fn run_from_env() -> Result<(), String> {
    let cfg = MarketConfig::from_lookup(&|k| std::env::var(k).ok(), unix_now_s())?;
    run(cfg)
}

pub fn run(cfg: MarketConfig) -> Result<(), String> {
    run_embedded(cfg, Clock::new(), None)
}

/// Existing C0-MO capture/drain/enrichment with an optional typed evidence tap.
pub fn run_embedded(
    cfg: MarketConfig,
    clock: Clock,
    econ: Option<crate::econ::Handle>,
) -> Result<(), String> {
    let process_started = unix_now_s();
    fs::create_dir_all(&cfg.run_dir)
        .map_err(|e| format!("create {}: {e}", cfg.run_dir.display()))?;
    let mut sink = JsonlSink::create(&cfg.run_dir)?;
    if econ.is_none() {
        print_banner(&cfg);
    }

    if let Some(h) = &econ {
        // No measurement clock or market ingress yet: initialization is separately
        // bounded and recorded, so a short run measures a running Shadow bot.
        eprintln!(
            "C0_E1_WARMUP_WAIT timeout_s={}",
            h.cfg.plan.budgets.bootstrap_seconds
        );
        sink.coverage(&json!({"event":"SHADOW_WARMUP_WAIT","timestamp_utc":utc_iso(unix_now_s())}))
            .map_err(|_| "WARMUP_EVIDENCE_WRITE".to_string())?;
        if let Err(e) =
            h.wait_shadow_ready(Duration::from_secs(h.cfg.plan.budgets.bootstrap_seconds))
        {
            let _ = sink.coverage(&json!({"event":"SHADOW_WARMUP_FAILED","error":e}));
            let _ = sink.flush();
            eprintln!("C0_E1_WARMUP_FAILED code={e}");
            return Err(e);
        }
        sink.coverage(
            &json!({"event":"SHADOW_WARMUP_READY","at_ns":clock.now_ns(),
            "timestamp_utc":utc_iso(unix_now_s())}),
        )
        .map_err(|_| "WARMUP_EVIDENCE_WRITE".to_string())?;
        crate::measurement::journal(
            &h.cfg.run_dir,
            clock,
            "WAIT_STREAM_READY",
            json!({"budget_seconds":h.cfg.plan.budgets.stream_ready_seconds}),
        )?;
        eprintln!("C0_E1_WARMUP_READY; starting market stream and measurement window");
    }

    let deployment_before = paths::inspect(cfg.rpc_url.as_deref(), &cfg.program);
    let (producer, consumer) = bounded::<IngressEvent>("market-ingress", cfg.ingress_buffer);
    let halt = IngressHalt::new();
    let slots = SlotWatch::new();
    let session = Shutdown::new();
    let src = WsRpcControlSource::new(market_ws_config(&cfg)?, clock);
    let session_for_thread = session.clone();
    let ingress = std::thread::Builder::new()
        .name("market-ingress-ws".into())
        .spawn(move || {
            if let Err(e) = src.run(producer, session_for_thread, halt, slots) {
                eprintln!("market ingress ended: {e}");
            }
        })
        .map_err(|e| format!("spawn ingress: {e}"))?;

    let mut core = ObserverCore::new(cfg.program, cfg.source, cfg.enrich);
    match deployment_before {
        Ok(v) => core.deployment_before = Some(v),
        Err(e) => core.deployment_error = Some(e),
    }
    core.econ = econ.clone();
    core.gap_cfg = cfg.gap;
    core.reconcile_cfg = cfg.reconcile;
    // Provisional all-path counts come from the existing program-attributed
    // WebSocket decoder. Canonical RPC happens only after Shadow is quiescent.
    let mut last_live_display = Instant::now() - Duration::from_secs(5);
    let mut checkpoint_index = 0usize;
    core.coverage(&mut sink, "START", process_started, None, json!({
        "source": cfg.source.as_str(), "commitment": cfg.commitment, "program": cfg.program.to_string(),
        "configured_duration_seconds": cfg.duration_secs, "ws_host": redact_url(&cfg.ws_url),
        "transport_policy":{"recovery_seconds":120,"maximum_failed_attempts":8,"slot_idle_seconds":60,"healthy_reset_seconds":10,"readiness":"ALL_ACKS_AND_NEW_SLOT_PROGRESS"},
        "provider_budget":darkbot_solana::provider::shared().snapshot(),
    }))
    .map_err(|e| format!("market evidence write failed: {e}"))?;

    let started = Instant::now();
    let ready_timeout = Duration::from_secs(cfg.ready_timeout_secs);
    let duration_ns = cfg.duration_secs.saturating_mul(1_000_000_000);
    let mut ready_unix_s: Option<u64> = None;
    let mut window: Option<(u64, u64)> = None;
    let mut last_overflow = 0u64;
    let mut last_flush = Instant::now();
    let mut stop_reason = "TIMER";
    let mut write_error: Option<String> = None;

    // ---- phase 1: CAPTURE (no RPC) ----
    loop {
        if econ
            .as_ref()
            .is_some_and(|h| h.shared.abort.load(std::sync::atomic::Ordering::Acquire))
        {
            stop_reason = "C0_E1_ABORT";
            break;
        }
        if let (Some(h), Some((start, cutoff))) = (&econ, window) {
            let due = h
                .cfg
                .plan
                .due(clock.now_ns().min(cutoff).saturating_sub(start));
            while checkpoint_index < due {
                checkpoint_index += 1;
                let seconds = h.cfg.plan.seconds(checkpoint_index);
                let summary = core.summary(&SummaryContext {
                    run_id: cfg.run_id.clone(),
                    commitment: cfg.commitment.into(),
                    configured_duration_seconds: seconds,
                    actual_observation_seconds: seconds,
                    stream_ready: true,
                    full_duration_observed: true,
                    drain_complete: false,
                    min_sample_liquidations: cfg.min_sample_liquidations,
                    ..Default::default()
                });
                h.emit(crate::econ::Evidence::MarketCheckpoint {
                    index: checkpoint_index,
                    summary,
                });
            }
        }
        if window.is_none() && started.elapsed() >= ready_timeout {
            stop_reason = "STREAM_NOT_READY";
            break;
        }
        if window.is_some_and(|(_, cutoff)| clock.now_ns() >= cutoff) {
            break;
        }
        if ingress.is_finished() {
            if let Err(e) = core.coverage(
                &mut sink,
                "FATAL",
                unix_now_s(),
                Some(clock.now_ns()),
                json!({"reason": "INGRESS_THREAD_ENDED"}),
            ) {
                write_error = Some(e.to_string());
            }
            stop_reason = "INGRESS_THREAD_ENDED";
            break;
        }
        let wait = window
            .map(|(_, cutoff)| Duration::from_nanos(cutoff.saturating_sub(clock.now_ns())))
            .unwrap_or(Duration::from_secs(1))
            .min(Duration::from_millis(250));
        let now_s = unix_now_s();
        let step = match consumer.pop_timeout(wait) {
            Some(IngressEvent::Reconnected {
                attempt, t_rx_ns, ..
            }) if window.is_none() => {
                let cutoff = t_rx_ns.saturating_add(duration_ns);
                window = Some((t_rx_ns, cutoff));
                ready_unix_s = Some((clock.unix_ns(t_rx_ns) / 1_000_000_000) as u64);
                core.start_observation(t_rx_ns, cutoff);
                if let Some(h) = &econ {
                    let evidence = h.cfg.plan.capture_window(clock, t_rx_ns)?;
                    crate::measurement::journal(&h.cfg.run_dir, clock, "CAPTURE", evidence)?;
                    h.start(t_rx_ns, cutoff);
                }
                core.coverage(&mut sink, "STREAM_READY", now_s, Some(t_rx_ns), json!({
                    "attempt": attempt, "observation_start_t_rx_ns": t_rx_ns, "observation_cutoff_t_rx_ns": cutoff,
                }))
            }
            Some(ev) => {
                let event_unix_s = (clock.unix_ns(ev.t_rx_ns()) / 1_000_000_000) as u64;
                core.on_ingress_event(ev, event_unix_s, &mut sink)
            }
            None => Ok(()),
        };
        if let Err(e) = step {
            write_error = Some(e.to_string());
            stop_reason = "WRITE_ERROR";
            break;
        }
        if last_live_display.elapsed() >= Duration::from_secs(2) {
            last_live_display = Instant::now();
            let gross: f64 = core.edges.iter().sum();
            eprintln!(
                "[MARKET/LIVE] phase={} tx={} success={} provisional_classic={} provisional_receivership={} provisional_deleverage={} provisional_bankruptcy={} receiver_gross_usd={:.6} gaps={} decoder_errors={} canonical_status=POST_CAPTURE_ONLY",
                if window.is_some() { "CAPTURE" } else { "WAITING_FOR_MARKET_STREAM" },
                core.stats.unique_transactions,
                core.stats.successful_transactions,
                core.stats.provisional_classic_events,
                core.stats.liquidation_events,
                core.stats.provisional_deleverage_events,
                core.stats.provisional_bankruptcy_events,
                gross,
                core.stats.gap_count,
                core.stats.decoder_error_count
            );
        }
        let overflow = consumer.stats().overflow;
        if overflow > last_overflow {
            last_overflow = overflow;
            if let Err(e) = core.coverage(
                &mut sink,
                "INGRESS_OVERFLOW",
                now_s,
                None,
                json!({"overflow_total": overflow}),
            ) {
                write_error = Some(e.to_string());
                stop_reason = "WRITE_ERROR";
                break;
            }
        }
        if last_flush.elapsed() >= Duration::from_secs(5) {
            last_flush = Instant::now();
            if let Err(e) = sink.flush() {
                write_error = Some(e.to_string());
                stop_reason = "WRITE_ERROR";
                break;
            }
        }
    }
    let capture_end_ns = clock.now_ns();
    if let Some(h) = &econ {
        crate::measurement::journal(
            &h.cfg.run_dir,
            clock,
            "DRAIN",
            json!({
            "stop_reason":stop_reason,"actual_capture_end_ns":capture_end_ns,
            "actual_capture_elapsed_ns":window.map(|(start,cutoff)|capture_end_ns.min(cutoff).saturating_sub(start)),
            "shadow_budget_seconds":h.cfg.plan.budgets.shadow_drain_seconds,
            "market_budget_seconds":h.cfg.plan.budgets.market_drain_seconds}),
        )?;
    }
    if stop_reason != "TIMER" {
        core.mark_incomplete("TRANSPORT_CAPTURE_ABORT");
        if let Some(h) = &econ {
            h.shared
                .market_failed
                .store(true, std::sync::atomic::Ordering::Release);
            h.shared
                .abort
                .store(true, std::sync::atomic::Ordering::Release);
        }
    }
    let observation_end_s = window
        .map(|(_, cutoff)| (clock.unix_ns(capture_end_ns.min(cutoff)) / 1_000_000_000) as u64)
        .unwrap_or_else(unix_now_s);

    // ---- phase 2: STOP ingress + DRAIN (events with t_rx <= cutoff still count) ----
    let _ = core.coverage(
        &mut sink,
        "STOP_REQUESTED",
        observation_end_s,
        None,
        json!({"reason": stop_reason}),
    );
    session.request();
    let mut drain_complete = false;
    let drain_deadline = Instant::now() + Duration::from_secs(cfg.drain_timeout_secs);
    while write_error.is_none() {
        let now_s = unix_now_s();
        match consumer.pop_timeout(Duration::from_millis(100)) {
            Some(ev) => {
                let event_unix_s = (clock.unix_ns(ev.t_rx_ns()) / 1_000_000_000) as u64;
                if let Err(e) = core.on_ingress_event(ev, event_unix_s, &mut sink) {
                    write_error = Some(e.to_string());
                }
            }
            None if ingress.is_finished() => {
                while let Some(ev) = consumer.try_pop() {
                    let event_unix_s = (clock.unix_ns(ev.t_rx_ns()) / 1_000_000_000) as u64;
                    if let Err(e) = core.on_ingress_event(ev, event_unix_s, &mut sink) {
                        write_error = Some(e.to_string());
                        break;
                    }
                }
                drain_complete = write_error.is_none();
                break;
            }
            None => {}
        }
        let overflow = consumer.stats().overflow;
        if overflow > last_overflow {
            last_overflow = overflow;
            let _ = core.coverage(
                &mut sink,
                "INGRESS_OVERFLOW",
                now_s,
                None,
                json!({"overflow_total": overflow, "phase": "drain"}),
            );
        }
        if Instant::now() >= drain_deadline {
            break;
        }
    }
    core.stop_observation();
    let _ = core.coverage(
        &mut sink,
        "DRAINED",
        unix_now_s(),
        None,
        json!({
            "drain_complete": drain_complete, "events_after_cutoff": core.stats.events_after_cutoff,
        }),
    );

    // A separate capture drain is insufficient: Shadow keeps winner RPCs alive
    // through its own drain. Release the heavy scan only after ALL workers exit.
    if let Some(h) = &econ {
        let barrier = h.wait_shadow_quiescent(Duration::from_secs(
            h.cfg.plan.budgets.shadow_drain_seconds + h.cfg.plan.budgets.quiescence_grace_seconds,
        ));
        let event = if barrier.is_ok() {
            "SHADOW_RPC_QUIESCENT"
        } else {
            "SHADOW_RPC_BARRIER_FAILED"
        };
        if let Err(e) = core.coverage(
            &mut sink,
            event,
            unix_now_s(),
            None,
            json!({"at_ns": clock.now_ns(), "error": barrier.as_ref().err()}),
        ) {
            write_error = Some(e.to_string());
        }
        if let Err(e) = barrier {
            write_error = Some(e);
        }
    }

    if let Some(h) = &econ {
        crate::measurement::journal(
            &h.cfg.run_dir,
            clock,
            "VERIFICATION",
            json!({
            "strict_seconds":h.cfg.plan.budgets.strict_seconds,"gap_seconds":h.cfg.plan.budgets.gap_seconds,
            "enrichment_seconds":h.cfg.plan.budgets.enrichment_seconds,"shadow_quiescent":write_error.is_none()}),
        )?;
    }
    // ---- phase 3 + summary + report: one shared production path ----
    let outcome = CaptureOutcome {
        process_started_unix_s: process_started,
        ready_unix_s,
        window,
        capture_end_ns,
        observation_end_unix_s: observation_end_s,
        stop_reason,
        drain_complete,
        write_error,
    };
    let (summary, report, write_error) = finish_capture(&cfg, &mut core, &mut sink, outcome)?;
    if ingress.is_finished() {
        let _ = ingress.join();
    }
    if let Some(h) = &econ {
        h.emit(crate::econ::Evidence::MarketFinal(summary));
    } else {
        print!("{report}");
    }
    if window.is_none() {
        let hint = if cfg.source == MarketSource::Transactions {
            " MARKET_OBSERVER_UNSUPPORTED_TRANSACTION_SUBSCRIBE suspected: Helius transactionSubscribe needs a Developer/Business/Professional plan; on the Free plan use P0_MARKET_SOURCE=logs."
        } else {
            ""
        };
        return Err(format!(
            "MARKET_OBSERVER_STREAM_NOT_READY: no StreamReady within {}s (see ingress errors above).{hint}",
            cfg.ready_timeout_secs
        ));
    }
    if let Some(e) = write_error {
        return Err(format!("market evidence write failed: {e}"));
    }
    if stop_reason != "TIMER" {
        return Err(format!("MARKET_CAPTURE_INCOMPLETE: {stop_reason}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    include!("market_gap_tests.rs");
    include!("market_reconcile_tests.rs");
    include!("market_truth_tests.rs");
    use super::*;
    use darkbot_solana::ingress::{SlotStatus, SlotUpdate, SourceKind, TxVersion};

    #[derive(Default)]
    struct VecSink {
        tx: Vec<Value>,
        liq: Vec<Value>,
        cov: Vec<Value>,
    }

    impl MarketSink for VecSink {
        fn transaction(&mut self, row: &Value) -> std::io::Result<()> {
            self.tx.push(row.clone());
            Ok(())
        }
        fn liquidation(&mut self, row: &Value) -> std::io::Result<()> {
            self.liq.push(row.clone());
            Ok(())
        }
        fn coverage(&mut self, row: &Value) -> std::io::Result<()> {
            self.cov.push(row.clone());
            Ok(())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn program() -> Pubkey {
        Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET).unwrap()
    }

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn data_line(target: Pubkey, receiver: Pubkey, seized: f64, repaid: f64, fee: u32) -> String {
        let mut b = LIQUIDATION_RECEIVER_EVENT_DISCRIMINATOR.to_vec();
        b.extend_from_slice(target.as_ref());
        b.extend_from_slice(receiver.as_ref());
        b.extend_from_slice(&seized.to_le_bytes());
        b.extend_from_slice(&repaid.to_le_bytes());
        b.extend_from_slice(&fee.to_le_bytes());
        format!("{PROGRAM_DATA_PREFIX}{}", b64(&b))
    }

    fn event_line() -> String {
        data_line(Pubkey::new_unique(), Pubkey::new_unique(), 2.0, 1.5, 1)
    }

    fn logs_with(p: &Pubkey, data: &[String]) -> Vec<String> {
        let mut v = vec![format!("Program {p} invoke [1]")];
        v.extend(data.iter().cloned());
        v.push(format!("Program {p} consumed 1000 of 200000 compute units"));
        v.push(format!("Program {p} success"));
        v
    }

    fn tx_event(
        sig: &str,
        slot: u64,
        logs: &[String],
        err: Option<&str>,
        t_rx_ns: u64,
    ) -> TransactionObserved {
        let meta_err = err
            .map(|e| json!({"InstructionError": [0, e]}))
            .unwrap_or(Value::Null);
        TransactionObserved {
            signature: sig.into(),
            slot,
            tx_index_in_slot: None,
            version: TxVersion::V0,
            err: err.map(str::to_string),
            source: SourceKind::WsRpcControl,
            t_rx_ns,
            raw: json!({"signature": sig, "slot": slot, "transaction": {
                "transaction": {"signatures": [sig], "message": {"accountKeys": [], "instructions": []}},
                "meta": {"err": meta_err, "fee": 5000, "logMessages": logs,
                         "preBalances": [], "postBalances": [], "preTokenBalances": [], "postTokenBalances": []},
                "version": 0}}),
        }
    }

    fn logs_event(
        sig: &str,
        slot: u64,
        logs: Vec<String>,
        err: Option<&str>,
        t_rx_ns: u64,
    ) -> LogsObserved {
        LogsObserved {
            signature: sig.into(),
            slot,
            err: err.map(str::to_string),
            logs: logs.clone(),
            source: SourceKind::WsRpcControl,
            t_rx_ns,
            raw: json!({"context": {"slot": slot}, "value": {"signature": sig, "err": null, "logs": logs}}),
        }
    }

    fn ctx(ready: bool, full: bool, secs: f64) -> SummaryContext {
        SummaryContext {
            run_id: "market_test".into(),
            commitment: "finalized".into(),
            stream_ready: ready,
            full_duration_observed: full,
            drain_complete: true,
            actual_observation_seconds: secs,
            configured_duration_seconds: 14_400.0,
            ..SummaryContext::default()
        }
    }

    fn open_core(source: MarketSource, enrich: EnrichConfig) -> ObserverCore {
        let mut core = ObserverCore::new(program(), source, enrich);
        core.start_observation(0, u64::MAX);
        core
    }

    // ---------------------------------------------------------------- decoder

    #[test]
    fn decodes_valid_receiver_event() {
        let (p, t, r) = (program(), Pubkey::new_unique(), Pubkey::new_unique());
        let d = decode_receiver_events(&logs_with(&p, &[data_line(t, r, 2.5, 2.0, 1_000_000)]), &p);
        assert_eq!(d.events.len(), 1);
        let e = &d.events[0];
        assert_eq!(
            (
                e.event_index,
                e.target,
                e.receiver,
                e.protocol_flat_fee_lamports
            ),
            (0, t, r, 1_000_000)
        );
        assert!((e.gross_edge_usd() - 0.5).abs() < 1e-12);
        assert_eq!(d.decoder_errors, 0);
    }

    #[test]
    fn wrong_discriminator_is_not_a_liquidation_event() {
        let p = program();
        let line = format!("{PROGRAM_DATA_PREFIX}{}", b64(&[7u8; 92]));
        let d = decode_receiver_events(&logs_with(&p, &[line]), &p);
        assert!(d.events.is_empty());
        assert_eq!(d.decoder_errors, 0);
    }

    #[test]
    fn short_non_finite_or_undecodable_marginfi_data_is_a_decoder_error() {
        let p = program();
        let mut short = LIQUIDATION_RECEIVER_EVENT_DISCRIMINATOR.to_vec();
        short.extend_from_slice(&[1u8; 40]);
        let short = format!("{PROGRAM_DATA_PREFIX}{}", b64(&short));
        let nan = data_line(Pubkey::new_unique(), Pubkey::new_unique(), f64::NAN, 1.0, 0);
        let bad_b64 = format!("{PROGRAM_DATA_PREFIX}!!!not-base64!!!");
        let d = decode_receiver_events(&logs_with(&p, &[short, nan, bad_b64]), &p);
        assert!(d.events.is_empty());
        assert_eq!((d.decoder_errors, d.base64_errors), (3, 0));
    }

    #[test]
    fn data_from_another_program_is_never_counted() {
        let p = program();
        let other = Pubkey::new_unique();
        let logs = vec![
            format!("Program {p} invoke [1]"),
            format!("Program {other} invoke [2]"),
            data_line(Pubkey::new_unique(), Pubkey::new_unique(), 100.0, 1.0, 0),
            format!("{PROGRAM_DATA_PREFIX}!!!"),
            format!("Program {other} success"),
            format!("Program {p} success"),
        ];
        let d = decode_receiver_events(&logs, &p);
        assert!(d.events.is_empty());
        assert_eq!(
            (d.foreign_program_events, d.base64_errors, d.decoder_errors),
            (1, 1, 0)
        );
    }

    #[test]
    fn truncated_logs_are_flagged() {
        let p = program();
        let mut logs = logs_with(&p, &[]);
        logs.push("Log truncated".into());
        assert!(decode_receiver_events(&logs, &p).truncated);
    }

    // ---------------------------------------------------------------- observer

    #[test]
    fn failed_transaction_is_recorded_but_not_a_liquidation() {
        let p = program();
        let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        core.on_transaction(
            &tx_event("f1", 10, &logs_with(&p, &[event_line()]), Some("Custom"), 1),
            0,
            &mut sink,
        )
        .unwrap();
        assert_eq!(sink.tx.len(), 1);
        assert_eq!(sink.tx[0]["success"], false);
        assert!(sink.liq.is_empty());
        assert_eq!(
            (
                core.stats.failed_transactions,
                core.stats.liquidation_events
            ),
            (1, 0)
        );
        assert_eq!(core.stats.failed_tx_receiver_events_ignored, 1);
    }

    #[test]
    fn duplicate_signature_counts_once() {
        let p = program();
        let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        let ev = tx_event("dup", 10, &logs_with(&p, &[event_line()]), None, 1);
        core.on_transaction(&ev, 0, &mut sink).unwrap();
        core.on_transaction(&ev, 0, &mut sink).unwrap();
        let s = &core.stats;
        assert_eq!(
            (
                s.transactions_received,
                s.unique_transactions,
                s.duplicate_transactions
            ),
            (2, 1, 1)
        );
        assert_eq!(
            (s.liquidation_events, sink.tx.len(), sink.liq.len()),
            (1, 1, 1)
        );
    }

    #[test]
    fn two_events_in_one_transaction_keep_distinct_indices() {
        let p = program();
        let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        core.on_transaction(
            &tx_event(
                "two",
                10,
                &logs_with(&p, &[event_line(), event_line()]),
                None,
                1,
            ),
            0,
            &mut sink,
        )
        .unwrap();
        assert_eq!(sink.liq.len(), 2);
        assert_eq!(
            (
                sink.liq[0]["event_index"].as_u64(),
                sink.liq[1]["event_index"].as_u64()
            ),
            (Some(0), Some(1))
        );
        assert_eq!(
            (
                core.stats.liquidation_transactions,
                core.stats.liquidation_events
            ),
            (1, 2)
        );
        assert_eq!(core.stats.tx_fee_total_lamports, 5_000); // once per transaction
        assert_eq!(
            (
                core.stats.enrichment_requested,
                core.stats.enrichment_succeeded
            ),
            (1, 1)
        );
    }

    #[test]
    fn v0_loaded_addresses_resolve_receiver_and_lookups_without_loaded_are_unknown() {
        let (payer, receiver, cb) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            COMPUTE_BUDGET_PROGRAM,
        );
        let limit = bs58::encode([2u8, 0x40, 0x42, 0x0f, 0x00]).into_string();
        let price = bs58::encode([3u8, 7, 0, 0, 0, 0, 0, 0, 0]).into_string();
        let tx = json!({"version": 0,
            "transaction": {"signatures": ["s"], "message": {
                "accountKeys": [payer.to_string(), cb],
                "addressTableLookups": [{"accountKey": Pubkey::new_unique().to_string(), "writableIndexes": [0], "readonlyIndexes": []}],
                "instructions": [{"programIdIndex": 1, "accounts": [], "data": limit},
                                 {"programIdIndex": 1, "accounts": [], "data": price}]}},
            "meta": {"err": null, "fee": 12_000,
                "loadedAddresses": {"writable": [receiver.to_string()], "readonly": [Pubkey::new_unique().to_string()]},
                "preBalances": [10, 1, 100, 5], "postBalances": [9, 1, 150, 5],
                "preTokenBalances": [{"accountIndex": 3, "mint": "M1", "owner": receiver.to_string(), "uiTokenAmount": {"amount": "10", "decimals": 6}}],
                "postTokenBalances": [{"accountIndex": 3, "mint": "M1", "owner": receiver.to_string(), "uiTokenAmount": {"amount": "35", "decimals": 6}},
                                      {"accountIndex": 4, "mint": "M2", "owner": receiver.to_string(), "uiTokenAmount": {"amount": "4", "decimals": 9}}]}});
        let e = enrich(&tx, &receiver);
        assert_eq!(e.receiver_lamport_delta, Some(50));
        assert_eq!(
            (e.tx_fee_lamports, e.fee_payer.as_deref()),
            (Some(12_000), Some(payer.to_string().as_str()))
        );
        assert_eq!(
            (e.cu_limit, e.cu_price_micro_lamports),
            (Some(1_000_000), Some(7))
        );
        assert_eq!(
            e.receiver_token_deltas,
            vec![
                TokenDelta {
                    mint: "M1".into(),
                    native_delta: 25,
                    decimals: Some(6)
                },
                TokenDelta {
                    mint: "M2".into(),
                    native_delta: 4,
                    decimals: Some(9)
                },
            ]
        );
        let mut no_loaded = tx.clone();
        no_loaded["meta"]
            .as_object_mut()
            .unwrap()
            .remove("loadedAddresses");
        let e = enrich(&no_loaded, &receiver);
        assert_eq!(
            (e.receiver_lamport_delta, e.fee_payer.clone()),
            (None, None)
        );
    }

    // [R2-2] Four-hour cutoff on the monotonic ingress clock, after the timer fired.
    #[test]
    fn cutoff_counts_queued_events_received_before_and_rejects_events_after() {
        let p = program();
        let mut core = ObserverCore::new(p, MarketSource::Transactions, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        core.start_observation(1_000, 2_000);
        // Timer has fired; these were still queued and are drained now.
        core.on_transaction(
            &tx_event("A", 10, &logs_with(&p, &[event_line()]), None, 1_999),
            0,
            &mut sink,
        )
        .unwrap();
        core.on_transaction(
            &tx_event("A2", 10, &logs_with(&p, &[]), None, 2_000),
            0,
            &mut sink,
        )
        .unwrap();
        core.on_transaction(
            &tx_event("B", 11, &logs_with(&p, &[event_line()]), None, 2_001),
            0,
            &mut sink,
        )
        .unwrap();
        assert_eq!(
            (
                core.stats.unique_transactions,
                core.stats.liquidation_events
            ),
            (2, 1)
        );
        assert_eq!(core.stats.events_after_cutoff, 1);
        // Coverage events after the cutoff do not taint the window either.
        core.coverage(&mut sink, "GAP", 0, Some(2_500), json!({}))
            .unwrap();
        assert!(core.coverage_complete());
        core.coverage(&mut sink, "GAP", 0, Some(1_500), json!({}))
            .unwrap();
        assert!(!core.coverage_complete());
    }

    #[test]
    fn gap_before_stream_ready_is_recorded_but_not_counted() {
        let mut core =
            ObserverCore::new(program(), MarketSource::Transactions, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        core.coverage(&mut sink, "GAP", 0, Some(5), json!({}))
            .unwrap();
        assert!(core.coverage_complete());
        assert_eq!(sink.cov[0]["counted"], false);
    }

    #[test]
    fn ingress_overflow_makes_data_incomplete() {
        let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        core.coverage(
            &mut sink,
            "INGRESS_OVERFLOW",
            1,
            None,
            json!({"overflow_total": 1}),
        )
        .unwrap();
        let s = core.summary(&ctx(true, true, 60.0));
        assert_eq!(
            (s["coverage_complete"].as_bool(), s["verdict"].as_str()),
            (Some(false), Some("MARKET_DATA_INCOMPLETE"))
        );
    }

    // [R2-4] A corrupted receiver event in a successful transaction is never VALID.
    #[test]
    fn decoder_error_in_successful_transaction_blocks_valid() {
        let p = program();
        let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        let mut short = LIQUIDATION_RECEIVER_EVENT_DISCRIMINATOR.to_vec();
        short.extend_from_slice(&[0u8; 10]);
        let line = format!("{PROGRAM_DATA_PREFIX}{}", b64(&short));
        core.on_transaction(
            &tx_event(
                "bad",
                10,
                &logs_with(&p, std::slice::from_ref(&line)),
                None,
                1,
            ),
            0,
            &mut sink,
        )
        .unwrap();
        assert!(!core.decode_complete());
        let s = core.summary(&ctx(true, true, 3_600.0));
        assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
        assert_eq!(s["decoder_errors_successful_tx"].as_u64(), Some(1));
        // The same corruption in a failed transaction is irrelevant for completeness.
        let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
        core.on_transaction(
            &tx_event("badf", 10, &logs_with(&p, &[line]), Some("Custom"), 1),
            0,
            &mut sink,
        )
        .unwrap();
        assert!(core.decode_complete());
    }

    // [R2-7] logs source can never claim unqualified MARKET_DATA_VALID.
    #[test]
    fn verdicts_by_source_and_without_liquidations() {
        let tx = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT)
            .summary(&ctx(true, true, 14_400.0));
        assert_eq!(tx["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
        assert_eq!(tx["source_coverage_verified"].as_bool(), Some(false));
        assert_eq!(
            tx["receivership_transport_capture_complete"].as_bool(),
            Some(true)
        );
        assert!(tx["gross_edge_median_usd"].is_null() && tx["top_receiver"].is_null());
        assert_eq!(tx["liquidations_per_hour"].as_f64(), Some(0.0));
        let logs = open_core(MarketSource::Logs, EnrichConfig::DEFAULT)
            .summary(&ctx(true, true, 14_400.0));
        assert_eq!(logs["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
        assert_eq!(logs["source_coverage_verified"].as_bool(), Some(false));
        assert!(logs["source_coverage_note"]
            .as_str()
            .unwrap()
            .contains("lookup table"));
        let early = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT)
            .summary(&ctx(true, false, 0.0));
        assert!(early["liquidations_per_hour"].is_null());
        assert_eq!(early["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
        let mut undrained = ctx(true, true, 14_400.0);
        undrained.drain_complete = false;
        let s = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT).summary(&undrained);
        assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
        assert!(s["incomplete_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r == "DRAIN_INCOMPLETE"));
        let not_ready = ObserverCore::new(program(), MarketSource::Logs, EnrichConfig::DEFAULT)
            .summary(&ctx(false, false, 0.0));
        assert_eq!(
            not_ready["verdict"].as_str(),
            Some("MARKET_OBSERVER_STREAM_NOT_READY")
        );
    }

    // [R2-8] Slot telemetry frames document stream liveness inside the window only.
    #[test]
    fn slot_telemetry_frames_measure_stream_silence() {
        let mut core = ObserverCore::new(program(), MarketSource::Logs, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        core.start_observation(0, 10_000_000_000);
        let slot = |t: u64| {
            IngressEvent::Slot(SlotUpdate {
                slot: 1,
                parent: None,
                status: SlotStatus::Processed,
                source: SourceKind::WsRpcControl,
                t_rx_ns: t,
            })
        };
        for t in [100_000_000, 500_000_000, 2_500_000_000, 20_000_000_000] {
            core.on_ingress_event(slot(t), 0, &mut sink).unwrap();
        }
        assert_eq!(
            (
                core.stats.slot_notifications,
                core.stats.max_stream_silence_ms
            ),
            (3, 2_000)
        );
    }

    #[test]
    fn events_before_observation_are_not_counted() {
        let p = program();
        let mut core = ObserverCore::new(p, MarketSource::Transactions, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        core.on_transaction(
            &tx_event("early", 1, &logs_with(&p, &[]), None, 1),
            0,
            &mut sink,
        )
        .unwrap();
        assert_eq!(
            (
                core.stats.transactions_received,
                core.stats.events_outside_observation
            ),
            (0, 1)
        );
        assert!(sink.tx.is_empty());
    }

    // ---------------------------------------------------------------- [R2-1/R2-9] deferred enrichment

    #[test]
    fn logs_capture_never_fetches_and_enrichment_runs_after_capture() {
        let p = program();
        let all = fixtures();
        let fx = &all[0];
        let real: Value = serde_json::from_str(fx.raw).unwrap();
        let real_logs: Vec<String> = real["meta"]["logMessages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap().to_string())
            .collect();
        let cfg = EnrichConfig {
            max_pending: 10,
            max_attempts: 3,
            retry_ms: 1,
            timeout_ms: 1,
            phase_max_secs: 1,
        };
        let mut core = open_core(MarketSource::Logs, cfg);
        let mut sink = VecSink::default();
        // CAPTURE: no fetch function exists on this path; gross edge counted immediately.
        core.on_logs(
            &logs_event(fx.signature, fx.slot, real_logs, None, 1),
            0,
            &mut sink,
        )
        .unwrap();
        core.on_logs(
            &logs_event("other", 7, logs_with(&p, &[event_line()]), None, 2),
            0,
            &mut sink,
        )
        .unwrap();
        assert_eq!(core.stats.liquidation_events, 2);
        assert!(sink.liq.is_empty());
        assert_eq!(core.stats.rpc_fallback_count, 0);
        // ENRICHMENT after capture.
        let (mut calls, mut waits) = (0u32, 0u32);
        core.enrich_deferred(
            &mut sink,
            &mut |sig: &str| {
                calls += 1;
                if sig == fx.signature {
                    Ok(Some(real.clone()))
                } else {
                    Err("http 429".into())
                }
            },
            &mut |_d: Duration| waits += 1,
            &mut || true,
        )
        .unwrap();
        assert_eq!((calls, waits), (1 + 3, 2));
        assert_eq!(sink.liq.len(), 2);
        assert_eq!(sink.liq[0]["enrichment_status"], "RPC_GET_TRANSACTION");
        assert_eq!(sink.liq[0]["tx_fee_lamports"].as_u64(), Some(fx.fee));
        assert_eq!(sink.liq[1]["enrichment_status"], "UNRESOLVED");
        let s = core.summary(&ctx(true, true, 3_600.0));
        assert_eq!(
            (
                s["enrichment_requested"].as_u64(),
                s["enrichment_succeeded"].as_u64(),
                s["enrichment_failed"].as_u64()
            ),
            (Some(2), Some(1), Some(1))
        );
        assert_eq!(
            (
                s["enrichment_complete"].as_bool(),
                s["fee_totals_complete"].as_bool()
            ),
            (Some(false), Some(false))
        );
        assert_eq!(s["liquidation_events"].as_u64(), Some(2)); // enrichment never removes gross edge
        assert_eq!(
            s["receivership_transport_capture_complete"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn enrichment_budget_identity_and_queue_limits_are_explicit() {
        let p = program();
        let cfg = EnrichConfig {
            max_pending: 1,
            max_attempts: 2,
            retry_ms: 1,
            timeout_ms: 1,
            phase_max_secs: 1,
        };
        let mut core = open_core(MarketSource::Logs, cfg);
        let mut sink = VecSink::default();
        core.on_logs(
            &logs_event("a", 5, logs_with(&p, &[event_line()]), None, 1),
            0,
            &mut sink,
        )
        .unwrap();
        core.on_logs(
            &logs_event("b", 6, logs_with(&p, &[event_line()]), None, 2),
            0,
            &mut sink,
        )
        .unwrap();
        assert_eq!(
            sink.liq.last().unwrap()["enrichment_status"],
            "SKIPPED_QUEUE_FULL"
        );
        // wrong transaction returned -> identity mismatch, no retry
        let mut calls = 0;
        core.enrich_deferred(
            &mut sink,
            &mut |_s: &str| {
                calls += 1;
                Ok(Some(
                    json!({"slot": 5, "transaction": {"signatures": ["zzz"]}}),
                ))
            },
            &mut |_d: Duration| {},
            &mut || true,
        )
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(
            sink.liq.last().unwrap()["enrichment_status"],
            "IDENTITY_MISMATCH"
        );
        // phase budget exhausted
        core.on_logs(
            &logs_event("c", 7, logs_with(&p, &[event_line()]), None, 3),
            0,
            &mut sink,
        )
        .unwrap();
        core.enrich_deferred(
            &mut sink,
            &mut |_s: &str| Ok(None),
            &mut |_d: Duration| {},
            &mut || false,
        )
        .unwrap();
        assert_eq!(
            sink.liq.last().unwrap()["enrichment_status"],
            "SKIPPED_PHASE_TIMEOUT"
        );
        assert_eq!(
            (
                core.stats.enrichment_requested,
                core.stats.enrichment_failed
            ),
            (3, 3)
        );
        assert_eq!(core.stats.liquidation_events, 3);
    }

    #[test]
    fn finalize_writes_summary_report_and_unenriched_rows() {
        let dir = std::env::temp_dir().join(format!(
            "c0mo_finalize_{}_{}",
            std::process::id(),
            utc_compact(unix_now_s())
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = program();
        let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
        let mut sink = JsonlSink::create(&dir).unwrap();
        core.on_logs(
            &logs_event(
                "x",
                9,
                logs_with(
                    &p,
                    &[data_line(
                        Pubkey::new_unique(),
                        Pubkey::new_unique(),
                        3.0,
                        1.0,
                        1,
                    )],
                ),
                None,
                1,
            ),
            0,
            &mut sink,
        )
        .unwrap();
        core.stop_observation();
        let (summary, report) = finalize(
            &mut core,
            &mut sink,
            &ctx(true, true, 600.0),
            &dir,
            0,
            json!({"reason": "TIMER"}),
        )
        .unwrap();
        assert_eq!(summary["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
        assert_eq!(summary["commitment"].as_str(), Some("finalized"));
        assert!(report.contains("verdict=MARKET_DATA_INCOMPLETE"));
        assert!(fs::read_to_string(dir.join("liquidations.jsonl"))
            .unwrap()
            .contains("NOT_ENRICHED"));
        assert_eq!(
            fs::read_to_string(dir.join("transactions.jsonl"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        assert!(fs::read_to_string(dir.join("coverage.jsonl"))
            .unwrap()
            .contains("STOPPED"));
        let parsed: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("summary.json")).unwrap()).unwrap();
        assert_eq!(parsed["liquidation_events"].as_u64(), Some(1));
        assert!(dir.join("final_report.txt").exists());
        assert!(JsonlSink::create(&dir).is_err()); // never overwrites a run directory
        let _ = fs::remove_dir_all(&dir);
    }

    // ---------------------------------------------------------------- config / safety

    #[test]
    fn market_config_needs_no_keypair_and_validates_source_and_commitment() {
        let env = |pairs: &[(&str, &str)]| {
            let m: HashMap<String, String> = pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            move |k: &str| m.get(k).cloned()
        };
        let base = [
            ("P0_WS_URL", "wss://h/?api-key=x"),
            (
                "P0_PROGRAM_ID",
                darkbot_project0::protocol::PROGRAM_ID_MAINNET,
            ),
        ];
        let c = MarketConfig::from_lookup(&env(&base), 0).unwrap();
        assert_eq!(
            (c.source, c.commitment, c.duration_secs),
            (MarketSource::Transactions, "finalized", 14_400)
        );
        assert_eq!(c.min_sample_liquidations, DEFAULT_MIN_SAMPLE_LIQUIDATIONS);
        assert_eq!(
            (c.rpc_url.clone(), c.drain_timeout_secs),
            (None, DEFAULT_DRAIN_TIMEOUT_SECS)
        );
        assert_eq!(c.run_id, "market_19700101T000000Z");
        let confirmed = MarketConfig::from_lookup(
            &env(&[base[0], base[1], ("P0_MARKET_COMMITMENT", "confirmed")]),
            0,
        )
        .unwrap();
        assert_eq!(confirmed.commitment, "confirmed");
        assert!(MarketConfig::from_lookup(
            &env(&[base[0], base[1], ("P0_MARKET_COMMITMENT", "processed")]),
            0
        )
        .is_err());
        // [R4-A1] logs without RPC is a valid configuration: enrichment unavailable, report still produced.
        let logs_no_rpc =
            MarketConfig::from_lookup(&env(&[base[0], base[1], ("P0_MARKET_SOURCE", "logs")]), 0)
                .unwrap();
        assert_eq!(
            (logs_no_rpc.source, logs_no_rpc.rpc_url.clone()),
            (MarketSource::Logs, None)
        );
        assert!(MarketConfig::from_lookup(&env(&[base[1]]), 0).is_err()); // neither P0_WS_URL nor HELIUS_API_KEY
        let logs = [
            base[0],
            base[1],
            ("P0_MARKET_SOURCE", "logs"),
            ("P0_RPC_URL", "https://r"),
            ("P0_MARKET_DURATION_SECS", "600"),
        ];
        let c = MarketConfig::from_lookup(&env(&logs), 0).unwrap();
        assert_eq!((c.source, c.duration_secs), (MarketSource::Logs, 600));
        assert!(MarketConfig::from_lookup(
            &env(&[base[0], base[1], ("P0_MARKET_SOURCE", "blocks")]),
            0
        )
        .is_err());
        assert!(MarketConfig::from_lookup(
            &env(&[base[0], base[1], ("P0_MARKET_DURATION_SECS", "0")]),
            0
        )
        .is_err());
        assert!(MarketConfig::from_lookup(
            &env(&[base[0], base[1], ("P0_MARKET_RUN_ID", "../x")]),
            0
        )
        .is_err());
        assert_eq!(redact_url("wss://h/?api-key=secret"), "wss://h");
        assert_eq!(
            redact_url("https://private:secret@host/PRIVATE_KEY?q=SECRET"),
            "https://host"
        );
    }

    #[test]
    fn market_mode_has_no_signing_simulation_or_submission_path() {
        let src = include_str!("market_observer.rs");
        for forbidden in [
            concat!("Key", "pair"),
            concat!("Sig", "ner"),
            concat!("Rpc", "Submitter"),
            concat!("Jito", "Submitter"),
            concat!("send_", "transaction"),
            concat!("sendTrans", "action"),
            concat!("simulate", "Transaction"),
            concat!("simulate_", "with"),
            concat!("Blockhash", "Cache"),
            concat!("Crate", "Health"),
            concat!("P0Receivership", "Builder"),
            concat!("crate::", "pipeline"),
        ] {
            assert!(
                !src.contains(forbidden),
                "market observer must not reference {forbidden}"
            );
        }
        let main = include_str!("main.rs");
        let branch = main
            .find("market_observer::run_from_env")
            .expect("market branch present");
        let run_fn = main.find("fn run()").expect("run()");
        let rpc_url = main
            .find(concat!("config::require(\"P0_", "RPC_URL\")"))
            .expect("rpc url read");
        assert!(run_fn < branch && branch < rpc_url);
    }

    // [R3-1] No unqualified source can produce MARKET_DATA_VALID.
    #[test]
    fn unverified_sources_never_yield_market_data_valid() {
        for source in [MarketSource::Transactions, MarketSource::Logs] {
            assert!(!source.coverage_verified());
            let s = open_core(source, EnrichConfig::DEFAULT).summary(&ctx(true, true, 14_400.0));
            assert_eq!(
                s["receivership_transport_capture_complete"].as_bool(),
                Some(true)
            );
            assert_ne!(s["verdict"].as_str(), Some("MARKET_DATA_VALID"));
            assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
        }
        assert_eq!(market_verdict(true, true, true), "MARKET_DATA_VALID");
        assert_eq!(
            market_verdict(true, true, false),
            "MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED"
        );
        assert_eq!(market_verdict(true, false, true), "MARKET_DATA_INCOMPLETE");
        assert_eq!(
            market_verdict(false, true, true),
            "MARKET_OBSERVER_STREAM_NOT_READY"
        );
    }

    // [R3-2] Market config enables the transport heartbeat; Shadow defaults stay untouched.
    #[test]
    fn market_ws_config_enables_heartbeat_and_shadow_default_does_not() {
        let mut cfg = MarketConfig::from_lookup(
            &|k: &str| match k {
                "P0_WS_URL" => Some("wss://h/?api-key=x".to_string()),
                "P0_PROGRAM_ID" => Some(darkbot_project0::protocol::PROGRAM_ID_MAINNET.to_string()),
                _ => None,
            },
            0,
        )
        .unwrap();
        let ws = market_ws_config(&cfg).unwrap();
        assert_eq!(
            ws.heartbeat_interval,
            Some(Duration::from_secs(DEFAULT_HEARTBEAT_SECS))
        );
        assert_eq!(ws.commitment, "finalized");
        assert!(ws.subscription.slots);
        assert_eq!(ws.subscription.transactions_mentioning, vec![cfg.program]);
        assert!(ws.subscription.logs_mentioning.is_empty());
        let shadow = WsRpcConfig::new("wss://h".into(), Subscription::default());
        assert_eq!(shadow.heartbeat_interval, None);
        assert_eq!(shadow.commitment, "confirmed");
        cfg.heartbeat_secs = shadow.idle_timeout.as_secs();
        assert!(market_ws_config(&cfg).is_err()); // heartbeat must stay below the idle timeout
    }

    // [R3-3] A failing enrichment client never loses gross edge or the final report.
    #[test]
    fn enrichment_init_failure_still_produces_summary_and_report() {
        let dir = std::env::temp_dir().join(format!(
            "c0mo_enrich_init_{}_{}",
            std::process::id(),
            utc_compact(unix_now_s())
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = program();
        let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
        let mut sink = JsonlSink::create(&dir).unwrap();
        core.on_logs(
            &logs_event(
                "x",
                9,
                logs_with(
                    &p,
                    &[data_line(
                        Pubkey::new_unique(),
                        Pubkey::new_unique(),
                        3.0,
                        1.0,
                        1,
                    )],
                ),
                None,
                1,
            ),
            0,
            &mut sink,
        )
        .unwrap();
        core.stop_observation();
        core.enrichment_phase(
            &mut sink,
            Err("enrichment client: tls init failed".into()),
            &mut |_d: Duration| {},
            &mut || false,
            0,
        )
        .unwrap();
        let (summary, report) = finalize(
            &mut core,
            &mut sink,
            &ctx(true, true, 600.0),
            &dir,
            0,
            json!({"reason": "TIMER"}),
        )
        .unwrap();
        assert_eq!(summary["liquidation_events"].as_u64(), Some(1));
        assert!((summary["gross_edge_total_usd"].as_f64().unwrap() - 2.0).abs() < 1e-12);
        assert_eq!(
            (
                summary["enrichment_complete"].as_bool(),
                summary["fee_totals_complete"].as_bool()
            ),
            (Some(false), Some(false))
        );
        assert_eq!(
            summary["enrichment_init_error"].as_str(),
            Some("enrichment client: tls init failed")
        );
        assert_eq!(
            summary["receivership_transport_capture_complete"].as_bool(),
            Some(true)
        );
        assert!(dir.join("summary.json").exists() && dir.join("final_report.txt").exists());
        assert!(report.contains("enrichment_complete=false"));
        assert!(fs::read_to_string(dir.join("liquidations.jsonl"))
            .unwrap()
            .contains("ENRICHMENT_INIT_FAILED"));
        assert!(fs::read_to_string(dir.join("coverage.jsonl"))
            .unwrap()
            .contains("ENRICHMENT_INIT_FAILED"));
        let _ = fs::remove_dir_all(&dir);
    }

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k: &str| m.get(k).cloned()
    }

    // [R4-A2]
    #[test]
    fn transactions_with_helius_key_uses_enhanced_mainnet_endpoint() {
        let pid = darkbot_project0::protocol::PROGRAM_ID_MAINNET;
        let c = MarketConfig::from_lookup(
            &env_of(&[("P0_PROGRAM_ID", pid), ("HELIUS_API_KEY", "K")]),
            0,
        )
        .unwrap();
        assert_eq!(c.ws_url, "wss://atlas-mainnet.helius-rpc.com/?api-key=K");
        assert_eq!(c.rpc_url, None); // transactions needs no enrichment RPC
        let l = MarketConfig::from_lookup(
            &env_of(&[
                ("P0_PROGRAM_ID", pid),
                ("HELIUS_API_KEY", "K"),
                ("P0_MARKET_SOURCE", "logs"),
            ]),
            0,
        )
        .unwrap();
        assert_eq!(l.ws_url, "wss://mainnet.helius-rpc.com/?api-key=K");
        assert_eq!(
            l.rpc_url.as_deref(),
            Some("https://mainnet.helius-rpc.com/?api-key=K")
        );
    }

    #[test]
    fn explicit_ws_url_overrides_helius_derived_url() {
        let pid = darkbot_project0::protocol::PROGRAM_ID_MAINNET;
        let c = MarketConfig::from_lookup(
            &env_of(&[
                ("P0_PROGRAM_ID", pid),
                ("HELIUS_API_KEY", "K"),
                ("P0_WS_URL", "wss://mine/?k=1"),
                ("P0_RPC_URL", "https://mine-rpc"),
            ]),
            0,
        )
        .unwrap();
        assert_eq!(c.ws_url, "wss://mine/?k=1");
        assert_eq!(c.rpc_url.as_deref(), Some("https://mine-rpc"));
    }

    // [R4-A1] Real entry path: env/config -> production finishing path (no test-only branch).
    #[test]
    fn logs_without_rpc_url_still_reaches_finalize_and_report() {
        let root = std::env::temp_dir().join(format!(
            "c0mo_norpc_{}_{}",
            std::process::id(),
            utc_compact(unix_now_s())
        ));
        let _ = fs::remove_dir_all(&root);
        let root_s = root.to_string_lossy().to_string();
        let pid = darkbot_project0::protocol::PROGRAM_ID_MAINNET;
        let cfg = MarketConfig::from_lookup(
            &env_of(&[
                ("P0_WS_URL", "wss://h"),
                ("P0_PROGRAM_ID", pid),
                ("P0_MARKET_SOURCE", "logs"),
                ("P0_MARKET_OUT_DIR", root_s.as_str()),
                ("P0_MARKET_RUN_ID", "norpc"),
            ]),
            0,
        )
        .unwrap();
        assert_eq!(cfg.rpc_url, None);
        fs::create_dir_all(&cfg.run_dir).unwrap();
        let mut sink = JsonlSink::create(&cfg.run_dir).unwrap();
        let mut core = ObserverCore::new(cfg.program, cfg.source, cfg.enrich);
        core.start_observation(0, 600_000_000_000);
        core.on_logs(
            &logs_event(
                "x",
                9,
                logs_with(
                    &cfg.program,
                    &[data_line(
                        Pubkey::new_unique(),
                        Pubkey::new_unique(),
                        3.0,
                        1.0,
                        1,
                    )],
                ),
                None,
                1,
            ),
            0,
            &mut sink,
        )
        .unwrap();
        core.stop_observation();
        let outcome = CaptureOutcome {
            process_started_unix_s: 0,
            ready_unix_s: Some(0),
            window: Some((0, 600_000_000_000)),
            capture_end_ns: 600_000_000_000,
            observation_end_unix_s: 600,
            stop_reason: "TIMER",
            drain_complete: true,
            write_error: None,
        };
        let (summary, report, write_error) =
            finish_capture(&cfg, &mut core, &mut sink, outcome).unwrap();
        assert_eq!(write_error, None);
        assert!(
            cfg.run_dir.join("summary.json").exists()
                && cfg.run_dir.join("final_report.txt").exists()
        );
        assert_eq!(summary["liquidation_events"].as_u64(), Some(1));
        assert!((summary["gross_edge_total_usd"].as_f64().unwrap() - 2.0).abs() < 1e-12);
        assert_eq!(summary["enrichment_available"].as_bool(), Some(false));
        assert_eq!(
            (
                summary["enrichment_complete"].as_bool(),
                summary["fee_totals_complete"].as_bool()
            ),
            (Some(false), Some(false))
        );
        assert!(summary["enrichment_init_error"].is_null());
        assert_eq!(summary["verification_status"], "NOT_RUN");
        assert_eq!(summary["full_slot_reconciliation"]["error"], "VERIFICATION_NOT_RUN");
        assert_eq!(
            summary["sample_status"].as_str(),
            Some("INSUFFICIENT_SAMPLE")
        );
        assert_eq!(summary["unique_targets"].as_u64(), Some(1));
        assert!((summary["actual_observation_seconds"].as_f64().unwrap() - 600.0).abs() < 1e-9);
        assert!(report.contains("sample_status=INSUFFICIENT_SAMPLE"));
        assert!(!fs::read_to_string(cfg.run_dir.join("coverage.jsonl"))
            .unwrap()
            .contains("ENRICHMENT_INIT_FAILED"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn quantiles_are_nearest_rank() {
        let v: Vec<f64> = (1..=10).map(f64::from).collect();
        assert_eq!(quantile_nearest_rank(&v, 0.5), Some(5.0));
        assert_eq!(quantile_nearest_rank(&v, 0.9), Some(9.0));
        assert_eq!(quantile_nearest_rank(&v, 0.99), Some(10.0));
        assert_eq!(quantile_nearest_rank(&v, 0.0), Some(1.0));
        assert_eq!(quantile_nearest_rank(&[4.2], 0.99), Some(4.2));
        assert_eq!(quantile_nearest_rank(&[], 0.5), None);
        assert_eq!(quantile_nearest_rank(&v, f64::NAN), None);
    }

    #[test]
    fn utc_formatting() {
        assert_eq!(utc_iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc_iso(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(utc_compact(951_782_400), "20000229T000000Z");
    }

    // ---------------------------------------------------------------- [r6] truncated-log qualification

    #[allow(clippy::too_many_arguments)]
    fn liq_tx_json(
        sig: &str,
        slot: u64,
        ixs: &[(Pubkey, Pubkey)],
        receiver_in_lut: bool,
        with_loaded: bool,
        logs: Vec<String>,
        failed: bool,
    ) -> Value {
        fn push(keys: &mut Vec<String>, k: String) -> usize {
            keys.push(k);
            keys.len() - 1
        }
        let mut statics: Vec<String> =
            vec![Pubkey::new_unique().to_string(), program().to_string()];
        let mut loaded_w: Vec<String> = Vec::new();
        let mut layouts: Vec<(usize, usize, usize, Result<usize, usize>, usize)> = Vec::new();
        for (t, r) in ixs {
            let ti = push(&mut statics, t.to_string());
            let rec = push(&mut statics, Pubkey::new_unique().to_string());
            let grp = push(&mut statics, Pubkey::new_unique().to_string());
            let recv = if receiver_in_lut {
                loaded_w.push(r.to_string());
                Err(loaded_w.len() - 1)
            } else {
                Ok(push(&mut statics, r.to_string()))
            };
            let fee = push(&mut statics, Pubkey::new_unique().to_string());
            layouts.push((ti, rec, grp, recv, fee));
        }
        let n_static = statics.len();
        let data = bs58::encode(darkbot_project0::protocol::ix::END_LIQUIDATION).into_string();
        let instructions: Vec<Value> = layouts
            .iter()
            .map(|(ti, rec, grp, recv, fee)| {
                let ri = match recv {
                    Ok(i) => *i,
                    Err(pos) => n_static + *pos,
                };
                json!({"programIdIndex": 1, "accounts": [ti, rec, grp, ri, fee], "data": data})
            })
            .collect();
        let lookups = if receiver_in_lut {
            json!([{"accountKey": Pubkey::new_unique().to_string(), "writableIndexes": [0], "readonlyIndexes": []}])
        } else {
            json!([])
        };
        let err = if failed {
            json!({"InstructionError": [0, "Custom"]})
        } else {
            Value::Null
        };
        let mut meta = json!({"err": err, "fee": 5000, "logMessages": logs, "preBalances": [], "postBalances": [],
            "preTokenBalances": [], "postTokenBalances": []});
        if with_loaded {
            meta["loadedAddresses"] = json!({"writable": loaded_w, "readonly": []});
        }
        json!({"slot": slot, "version": 0,
            "transaction": {"signatures": [sig], "message": {"accountKeys": statics, "instructions": instructions, "addressTableLookups": lookups}},
            "meta": meta})
    }

    /// Helius `transactionNotification.result` shape built from a getTransaction-shaped tx.
    fn as_observed(tx: &Value, sig: &str, slot: u64, failed: bool) -> TransactionObserved {
        let mut inner = tx.clone();
        inner.as_object_mut().unwrap().remove("slot");
        TransactionObserved {
            signature: sig.into(),
            slot,
            tx_index_in_slot: None,
            version: TxVersion::V0,
            err: if failed { Some("Custom".into()) } else { None },
            source: SourceKind::WsRpcControl,
            t_rx_ns: 1,
            raw: json!({"signature": sig, "slot": slot, "transaction": inner}),
        }
    }

    fn truncated_logs(events: &[(Pubkey, Pubkey)]) -> Vec<String> {
        let p = program();
        let mut v = vec![format!("Program {p} invoke [1]")];
        v.extend(events.iter().map(|(t, r)| data_line(*t, *r, 2.0, 1.0, 1)));
        v.push("Log truncated".into());
        v
    }

    fn observe_truncated(
        ixs: &[(Pubkey, Pubkey)],
        events: &[(Pubkey, Pubkey)],
        lut: bool,
        loaded: bool,
    ) -> ObserverCore {
        let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        let tx = liq_tx_json("sigT", 50, ixs, lut, loaded, truncated_logs(events), false);
        core.on_transaction(&as_observed(&tx, "sigT", 50, false), 0, &mut sink)
            .unwrap();
        core
    }

    #[test]
    fn truncated_logs_with_no_end_liquidation_can_be_qualified() {
        let core = observe_truncated(&[], &[], false, false);
        assert_eq!(
            (
                core.stats.logs_truncated,
                core.stats.logs_truncated_qualified,
                core.stats.logs_truncated_unqualified
            ),
            (1, 1, 0)
        );
        assert!(core.decode_complete());
        let s = core.summary(&ctx(true, true, 600.0));
        assert_eq!(
            s["receivership_transport_capture_complete"].as_bool(),
            Some(true)
        );
        assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE")); // coverage stays fail closed
        assert_eq!(s["source_coverage_verified"].as_bool(), Some(false));
        // logs source: deferred until after capture, then qualified against the fetched transaction
        let mut core = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        core.on_logs(
            &logs_event("sigL", 51, truncated_logs(&[]), None, 1),
            0,
            &mut sink,
        )
        .unwrap();
        assert!(!core.decode_complete()); // pending qualification is not complete
        let fetched = liq_tx_json("sigL", 51, &[], false, false, truncated_logs(&[]), false);
        core.qualify_truncated_deferred(
            &mut |_s: &str| Ok(Some(fetched.clone())),
            &mut |_d: Duration| {},
            &mut || true,
        );
        assert_eq!(core.stats.logs_truncated_qualified, 1);
        assert!(core.decode_complete());
    }

    #[test]
    fn truncated_logs_with_exact_instruction_event_parity_can_be_qualified() {
        let (t1, r1, t2, r2) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        // multiset parity, independent of order
        let core = observe_truncated(&[(t1, r1), (t2, r2)], &[(t2, r2), (t1, r1)], false, false);
        assert_eq!(core.stats.logs_truncated_qualified, 1);
        assert_eq!(
            (
                core.stats.truncated_instruction_end_liquidations,
                core.stats.truncated_log_end_liquidations
            ),
            (2, 2)
        );
        assert_eq!(core.stats.liquidation_events, 2);
        assert!(core.decode_complete());
    }

    #[test]
    fn truncated_logs_missing_liquidation_event_fail_closed() {
        let (t, r) = (Pubkey::new_unique(), Pubkey::new_unique());
        let core = observe_truncated(&[(t, r)], &[], false, false);
        assert_eq!(core.stats.logs_truncated_liquidation_mismatch, 1);
        assert!(!core.decode_complete());
        let s = core.summary(&ctx(true, true, 600.0));
        assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
        assert!(s["incomplete_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "TRUNCATED_LIQUIDATION_MISMATCH"));
        assert_eq!(s["liquidation_events"].as_u64(), Some(0)); // nothing invented
    }

    #[test]
    fn truncated_logs_wrong_target_or_receiver_fail_closed() {
        let (t, r) = (Pubkey::new_unique(), Pubkey::new_unique());
        let wrong_receiver =
            observe_truncated(&[(t, r)], &[(t, Pubkey::new_unique())], false, false);
        assert_eq!(wrong_receiver.stats.logs_truncated_liquidation_mismatch, 1);
        let wrong_target = observe_truncated(&[(t, r)], &[(Pubkey::new_unique(), r)], false, false);
        assert_eq!(wrong_target.stats.logs_truncated_liquidation_mismatch, 1);
        let duplicated = observe_truncated(&[(t, r)], &[(t, r), (t, r)], false, false);
        assert_eq!(duplicated.stats.logs_truncated_liquidation_mismatch, 1); // multiset, not set
        assert!(
            !wrong_receiver.decode_complete()
                && !wrong_target.decode_complete()
                && !duplicated.decode_complete()
        );
    }

    #[test]
    fn truncated_v0_loaded_addresses_are_resolved() {
        let (t, r) = (Pubkey::new_unique(), Pubkey::new_unique());
        let core = observe_truncated(&[(t, r)], &[(t, r)], true, true);
        assert_eq!(
            (
                core.stats.logs_truncated_qualified,
                core.stats.logs_truncated_liquidation_mismatch
            ),
            (1, 0)
        );
        assert!(core.decode_complete());
    }

    #[test]
    fn truncated_missing_loaded_addresses_fail_closed() {
        let (t, r) = (Pubkey::new_unique(), Pubkey::new_unique());
        let core = observe_truncated(&[(t, r)], &[(t, r)], true, false);
        assert_eq!(core.stats.logs_truncated_unqualified, 1);
        assert!(!core.decode_complete());
        let s = core.summary(&ctx(true, true, 600.0));
        assert_eq!(
            s["truncated_unqualified_reasons"]["LOADED_ADDRESSES_MISSING"].as_u64(),
            Some(1)
        );
        assert!(s["incomplete_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "LOGS_TRUNCATED_UNQUALIFIED"));
    }

    #[test]
    fn truncated_fetch_failure_or_wrong_transaction_fail_closed() {
        let mut core = open_core(
            MarketSource::Logs,
            EnrichConfig {
                max_attempts: 2,
                ..EnrichConfig::DEFAULT
            },
        );
        let mut sink = VecSink::default();
        core.on_logs(
            &logs_event("a", 1, truncated_logs(&[]), None, 1),
            0,
            &mut sink,
        )
        .unwrap();
        core.on_logs(
            &logs_event("b", 2, truncated_logs(&[]), None, 2),
            0,
            &mut sink,
        )
        .unwrap();
        let other = liq_tx_json("zzz", 2, &[], false, false, vec![], false);
        core.qualify_truncated_deferred(
            &mut |sig: &str| {
                if sig == "a" {
                    Err("http 429".into())
                } else {
                    Ok(Some(other.clone()))
                }
            },
            &mut |_d: Duration| {},
            &mut || true,
        );
        assert_eq!(core.stats.logs_truncated_unqualified, 2);
        let s = core.summary(&ctx(true, true, 600.0));
        assert_eq!(
            s["truncated_unqualified_reasons"]["FETCH_FAILED"].as_u64(),
            Some(1)
        );
        assert_eq!(
            s["truncated_unqualified_reasons"]["IDENTITY_MISMATCH"].as_u64(),
            Some(1)
        );
        // never qualified before finalize -> fail closed
        let mut late = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
        late.on_logs(
            &logs_event("c", 3, truncated_logs(&[]), None, 3),
            0,
            &mut sink,
        )
        .unwrap();
        late.fail_truncated_pending("NO_RPC_FOR_QUALIFICATION");
        assert_eq!(late.stats.logs_truncated_unqualified, 1);
    }

    #[test]
    fn failed_transaction_does_not_create_liquidation() {
        let (t, r) = (Pubkey::new_unique(), Pubkey::new_unique());
        let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        let tx = liq_tx_json(
            "f",
            9,
            &[(t, r)],
            false,
            false,
            truncated_logs(&[(t, r)]),
            true,
        );
        core.on_transaction(&as_observed(&tx, "f", 9, true), 0, &mut sink)
            .unwrap();
        assert_eq!(
            (core.stats.liquidation_events, core.stats.logs_truncated),
            (0, 0)
        );
        assert!(sink.liq.is_empty());
        assert!(core.decode_complete());
    }

    #[test]
    fn non_truncated_existing_behavior_unchanged() {
        let (t, r) = (Pubkey::new_unique(), Pubkey::new_unique());
        let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        let logs = logs_with(&program(), &[data_line(t, r, 2.0, 1.0, 1)]);
        let tx = liq_tx_json("n", 7, &[(t, r)], false, false, logs, false);
        core.on_transaction(&as_observed(&tx, "n", 7, false), 0, &mut sink)
            .unwrap();
        assert_eq!(core.stats.liquidation_events, 1);
        assert_eq!(
            (
                core.stats.logs_truncated,
                core.stats.logs_truncated_qualified,
                core.stats.truncated_instruction_end_liquidations
            ),
            (0, 0, 0)
        );
        assert!(core.decode_complete());
        let s = core.summary(&ctx(true, true, 600.0));
        assert_eq!(
            s["receivership_transport_capture_complete"].as_bool(),
            Some(true)
        );
    }

    // ---------------------------------------------------------------- real audit fixtures (6/6)

    struct Fx {
        raw: &'static str,
        signature: &'static str,
        slot: u64,
        gross: f64,
        receiver: &'static str,
        target: &'static str,
        fee: u64,
        cu_limit: Option<u32>,
        lamport_delta: i128,
        token_mints: usize,
    }

    fn fixtures() -> Vec<Fx> {
        vec![
            Fx { raw: include_str!("../../../../docs/evidence/AUDIT_20261005/2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf.json"), signature: "2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf", slot: 451703584, gross: 0.186822, receiver: "D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn", target: "3GqwCURBtJZfdQxMSyZ1VyWNSB5WXovg7Q8Q46HY7ihx", fee: 19000, cu_limit: None, lamport_delta: -1019000, token_mints: 2 },
            Fx { raw: include_str!("../../../../docs/evidence/AUDIT_20261005/38ZXvW6P5sGZmdzj4QbUPSdQnCnJEBHoXTh4vvvfitFPNDHRsgPjELCnFxxCfW2wygE8J6k2Yv8RJJXPUVchBm7F.json"), signature: "38ZXvW6P5sGZmdzj4QbUPSdQnCnJEBHoXTh4vvvfitFPNDHRsgPjELCnFxxCfW2wygE8J6k2Yv8RJJXPUVchBm7F", slot: 451704201, gross: 0.531769, receiver: "D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn", target: "AzQVTAtLxdaQUJzh7xZ7rj956tkeVNk4D4iuK5EVkxDi", fee: 19000, cu_limit: None, lamport_delta: -1019000, token_mints: 2 },
            Fx { raw: include_str!("../../../../docs/evidence/AUDIT_20261005/3ab7mnuTErWjRtS8iz4Nm619VNvm3FSw47VVvdjYnSLNzB2btCoreDsQqmzfb4Qt5oBA29saPQ98Rgy2nyAUExRB.json"), signature: "3ab7mnuTErWjRtS8iz4Nm619VNvm3FSw47VVvdjYnSLNzB2btCoreDsQqmzfb4Qt5oBA29saPQ98Rgy2nyAUExRB", slot: 451657746, gross: 0.18595, receiver: "TraceR2t18nnwkXgHoB54iPD5N4x2EJZC1uS55ggsH8", target: "6mzmLGPk7CeGCM7dRH8ZZpHsSzfy6aAYf47nbH8HuYy8", fee: 5000, cu_limit: Some(598236), lamport_delta: -1005000, token_mints: 2 },
            Fx { raw: include_str!("../../../../docs/evidence/AUDIT_20261005/59Qt6h1iPnEgfaDaEi5Y5jVvqHJCs4gzwvHqWW37wVJMHyThYjM6zsS8AmE9bvoxqdUj8AMnNbBuKHQtXysdnwNa.json"), signature: "59Qt6h1iPnEgfaDaEi5Y5jVvqHJCs4gzwvHqWW37wVJMHyThYjM6zsS8AmE9bvoxqdUj8AMnNbBuKHQtXysdnwNa", slot: 453063240, gross: 0.060183, receiver: "qbvAGHo9JpVRsuRem8rBCzLJJNV6ZDNv63qYHkcmNCV", target: "HMjc1LecbCMUqYG3x36gaCzxG5AJ7gJPkPB85fx1pPKK", fee: 5000, cu_limit: Some(1400000), lamport_delta: -1005000, token_mints: 2 },
            Fx { raw: include_str!("../../../../docs/evidence/AUDIT_20261005/C6JnpzGrv53y9dEExtpDYkzTbexLEu7oikPRNx8sCqf85Jvobc7UBpsaDSgGHzEgeB88HkVFqMkCkvs1ZqVWRwZ.json"), signature: "C6JnpzGrv53y9dEExtpDYkzTbexLEu7oikPRNx8sCqf85Jvobc7UBpsaDSgGHzEgeB88HkVFqMkCkvs1ZqVWRwZ", slot: 451635082, gross: 0.2098, receiver: "TraceR2t18nnwkXgHoB54iPD5N4x2EJZC1uS55ggsH8", target: "6mzmLGPk7CeGCM7dRH8ZZpHsSzfy6aAYf47nbH8HuYy8", fee: 5000, cu_limit: Some(779995), lamport_delta: -1005000, token_mints: 2 },
            Fx { raw: include_str!("../../../../docs/evidence/AUDIT_20261005/gLVXyfVZmizZqpF9752x3uhgJixW5Jqrp3Agjhhr6cNUnU5aQDBAxQ1CF9RWSpy2oB2K9EAroFJaGPTkz5vkUzB.json"), signature: "gLVXyfVZmizZqpF9752x3uhgJixW5Jqrp3Agjhhr6cNUnU5aQDBAxQ1CF9RWSpy2oB2K9EAroFJaGPTkz5vkUzB", slot: 451643526, gross: 0.1742, receiver: "D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn", target: "6mzmLGPk7CeGCM7dRH8ZZpHsSzfy6aAYf47nbH8HuYy8", fee: 19000, cu_limit: None, lamport_delta: -1019000, token_mints: 2 },
        ]
    }

    #[test]
    fn six_real_audit_liquidations_decode_enrich_and_verify_identity() {
        let p = program();
        for fx in fixtures() {
            let tx: Value = serde_json::from_str(fx.raw).unwrap();
            let logs: Vec<String> = tx["meta"]["logMessages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l.as_str().unwrap().to_string())
                .collect();
            let d = decode_receiver_events(&logs, &p);
            assert_eq!(d.events.len(), 1, "{}", fx.signature);
            assert_eq!(d.decoder_errors, 0);
            let e = &d.events[0];
            assert!(
                (e.gross_edge_usd() - fx.gross).abs() < 1e-6,
                "{} gross {}",
                fx.signature,
                e.gross_edge_usd()
            );
            assert_eq!(e.receiver.to_string(), fx.receiver);
            assert_eq!(e.target.to_string(), fx.target);
            assert_eq!(e.protocol_flat_fee_lamports, 1_000_000);
            let en = enrich(&tx, &e.receiver);
            assert_eq!(en.tx_fee_lamports, Some(fx.fee));
            assert_eq!(en.fee_payer.as_deref(), Some(fx.receiver));
            assert_eq!(en.cu_limit, fx.cu_limit);
            assert_eq!(en.receiver_lamport_delta, Some(fx.lamport_delta));
            assert_eq!(en.receiver_token_deltas.len(), fx.token_mints);
            assert!(en.token_deltas_complete);
            assert_eq!(verify_tx_identity(&tx, fx.signature, fx.slot), Ok(()));
            assert!(verify_tx_identity(&tx, fx.signature, fx.slot + 1).is_err());
        }
    }

    #[test]
    fn six_real_audit_liquidations_aggregate_into_market_summary() {
        let mut core = open_core(MarketSource::Transactions, EnrichConfig::DEFAULT);
        let mut sink = VecSink::default();
        for fx in fixtures() {
            let tx: Value = serde_json::from_str(fx.raw).unwrap();
            let ev = TransactionObserved {
                signature: fx.signature.into(),
                slot: fx.slot,
                tx_index_in_slot: None,
                version: TxVersion::V0,
                err: None,
                source: SourceKind::WsRpcControl,
                t_rx_ns: 1,
                raw: json!({"signature": fx.signature, "slot": fx.slot, "transaction": tx}),
            };
            core.on_transaction(&ev, 0, &mut sink).unwrap();
        }
        let s = core.summary(&ctx(true, true, 3_600.0));
        assert_eq!(s["verdict"].as_str(), Some("MARKET_DATA_INCOMPLETE"));
        assert_eq!(
            s["receivership_transport_capture_complete"].as_bool(),
            Some(true)
        );
        assert_eq!(
            (
                s["liquidation_events"].as_u64(),
                s["liquidation_transactions"].as_u64()
            ),
            (Some(6), Some(6))
        );
        let f = |k: &str| s[k].as_f64().unwrap();
        assert!((f("gross_edge_total_usd") - 1.348724).abs() < 1e-5);
        assert!((f("gross_edge_per_hour_usd") - 1.348724).abs() < 1e-5);
        assert!((f("gross_edge_median_usd") - 0.18595).abs() < 1e-6);
        assert!((f("gross_edge_p90_usd") - 0.531769).abs() < 1e-6);
        assert!((f("gross_edge_min_usd") - 0.060183).abs() < 1e-6);
        assert_eq!(
            s["top_receiver"].as_str(),
            Some("D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn")
        );
        assert!((f("top_receiver_liquidation_share") - 0.5).abs() < 1e-12);
        assert!((f("top_receiver_gross_edge_share") - 0.892791 / 1.348724).abs() < 1e-5);
        assert!((f("top3_receiver_liquidation_share") - 1.0).abs() < 1e-12);
        assert_eq!(s["unique_receivers"].as_u64(), Some(3));
        assert_eq!(
            s["protocol_flat_fee_total_lamports"].as_u64(),
            Some(6_000_000)
        );
        assert_eq!(s["tx_fee_total_lamports"].as_u64(), Some(72_000));
        assert_eq!(
            (
                s["enrichment_complete"].as_bool(),
                s["fee_totals_complete"].as_bool()
            ),
            (Some(true), Some(true))
        );
        assert!(sink.liq.iter().all(|r| r["enrichment_status"] == "PAYLOAD"));
    }
}

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

use crate::market::winner::{
    check_transaction_identity, end_liquidation_instructions, RpcTransaction,
};
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

#[path = "../market_gap.rs"]
mod gap;
pub use gap::GapConfig;
#[path = "../market_reconcile.rs"]
mod reconcile;
#[path = "../market_verification.rs"]
pub mod verification;
pub use reconcile::ReconcileConfig;
#[path = "deployment.rs"]
mod paths;
#[path = "../market_truth.rs"]
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

#[path = "configuration.rs"]
mod configuration;
pub use configuration::{market_ws_config, redact_url, EnrichConfig, MarketConfig, MarketSource};
#[path = "time.rs"]
mod time;
use time::unix_now_s;
pub use time::{utc_compact, utc_iso};
#[path = "decoding.rs"]
mod decoding;
pub use decoding::{decode_receiver_events, LogDecode, ReceiverEvent};

#[path = "enrichment.rs"]
mod enrichment;
use enrichment::full_account_keys;
pub use enrichment::{enrich, verify_tx_identity, TokenDelta, TxEnrichment};
#[path = "sink.rs"]
mod sink;
pub use sink::{JsonlSink, MarketSink};

#[path = "core.rs"]
mod core;
pub use core::{
    market_verdict, qualify_truncated, quantile_nearest_rank, MarketStats, ObserverCore,
    ReceiverRank, TruncationQualification,
};

#[path = "summary.rs"]
mod summary;
pub use summary::{finalize, render_report, SummaryContext};

#[path = "fetch.rs"]
mod fetch;
use fetch::RpcFetcher;
#[path = "capture.rs"]
mod capture;
pub use capture::{finish_capture, run, run_embedded, run_from_env, CaptureOutcome};

#[cfg(test)]
#[path = "observer_tests.rs"]
mod tests;

pub use configuration::{
    DEFAULT_DRAIN_TIMEOUT_SECS, DEFAULT_DURATION_SECS, DEFAULT_HEARTBEAT_SECS,
    DEFAULT_INGRESS_BUFFER, DEFAULT_MIN_SAMPLE_LIQUIDATIONS, DEFAULT_OUT_DIR,
    DEFAULT_READY_TIMEOUT_SECS, HELIUS_ENHANCED_WSS_MAINNET, HELIUS_RPC_MAINNET,
    HELIUS_STANDARD_WSS_MAINNET,
};

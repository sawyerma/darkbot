//! The read-only BSC runtime process (C1, order V2 §5/§7.3): it observes the
//! signal sources the native source owner made effective `SHADOW`, verifies
//! and classifies what they deliver in one [`IngressCore`], journals launch
//! sightings and source transitions, and rewrites its status record. No
//! transaction is built, signed or sent anywhere in this program.
//!
//! With a canonical endpoint it also follows the canonical chain
//! (`chain_follower`) and joins every canonical launch with the sightings
//! (`canonical_join`): the G0.4 denominator. The census
//! (`bsc-runtime/g04-census.dbrec`) is replayed from the journals every
//! `census_ms` and at stop.
//!
//! Lifecycle: `run` until the stop file exists (written by `darkbotctl`),
//! then stops every adapter and the follower, joins the launches still
//! waiting, commits the journal and writes a final status and census.
//! A source without a configured endpoint, or without an adapter in this
//! build (the local geth Fast Lane needs a synced node), is `UNQUALIFIED`.
//!
//! With the component `BSC_STATE_C2` effective `SHADOW` (and a canonical
//! endpoint) the C2 chain-state owner ([`state`]) applies the canonical
//! inputs and the pending sightings of launch candidates and watched
//! senders. G0.4 counts only blocks mined after this process started; the
//! blocks a C2 resume backfills are counted apart.

use crate::canonical_join::{LaunchRef, LaunchTx, Ledger, Venue};
use crate::chain_follower::{
    CanonicalEvent, FollowerFault, FollowerLimits, NonceWatch, RequestError, is_unrebuildable,
    run_follower,
};
use crate::g04_census::{Census, census};
use crate::ingress::{Health, IngressCore, Launch, Observation, SourceStats, health};
use crate::paper_exec::{ExecConfig, FnClock, PAPER_PROFILE};
use crate::runtime_journal::{
    CaptureScope, CaptureStatus, EXIT_PAYLOAD, JournalSetup, RunIdentity, RuntimeJournal,
    STOP_PAYLOAD, SourceEvent, canonical_launch_payload, first_sighting_payload,
    flap_migrated_payload, flap_term_payload, flap_trade_payload, ingress_drop_payload,
    later_sighting_payload, launch_id_payload, orphaned_launch_payload, own_binary_sha256,
    paper_handoff_payload, pending_launch_payload, reorg_payload, resync_payload,
    source_event_payload, trade_gap_payload, window_payload,
};
use crate::sources::{ComponentId, Effective, Generation, SourceId, Store, VenueId};
use crate::wss::endpoint::Endpoint;
use crate::wss::{DisconnectReason, WssEvent, WssLimits, WssSource, run_source};
use darkbot_bsc_geth_ipc::protocol_v2::CLOCK_ID_MONOTONIC;
use darkbot_bsc_journal::RecordKind;
use darkbot_bsc_journal::storage::segments::SegmentPolicy;
use darkbot_bsc_journal::storage::writer::SyncPolicy;
use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
use darkbot_bsc_record::{Draft, observed_now};
use darkbot_bsc_trading::chain_state::engine::StateLimits;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[path = "runtime_state.rs"]
pub mod state;

#[path = "runtime_exec.rs"]
pub mod exec;

#[path = "runtime_live.rs"]
pub mod live;

use exec::{ExecRuntime, ExecSetup};
use live::LiveRuntime;
use state::{CheckpointWriter, StateConfig, StateOwner};

/// Strategy C's configuration (`BSC_STRATEGY_C_*`): the engine's policy
/// and the frozen gas evidence files (repository-relative) its costs use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrategyConfig {
    pub engine: darkbot_bsc_trading::strategy::engine::Config,
    pub gas_evidence: Vec<String>,
}

impl StrategyConfig {
    /// The frozen gas evidence the strategy may read, compiled in: each
    /// file the economics snapshot binds, as this build's tree holds it
    /// (the snapshot guard keeps the tree equal to the frozen snapshot).
    pub const FROZEN_GAS_EVIDENCE: [(&'static str, &'static [u8]); 4] = [
        (
            "bsc/engine/crates/trading/fixtures/bsc/receipts_20261026.txt",
            include_bytes!("../../trading/fixtures/bsc/receipts_20261026.txt"),
        ),
        (
            "bsc/engine/crates/trading/fixtures/bsc/flap_gas_20261031.txt",
            include_bytes!("../../trading/fixtures/bsc/flap_gas_20261031.txt"),
        ),
        // BSC3-079: older windows of the v5.27.2 era (the largest gas of a
        // class depends on the window), with the launch-block BUY class.
        (
            "bsc/engine/crates/trading/fixtures/bsc/flap_gas_20261031_w130100000.txt",
            include_bytes!("../../trading/fixtures/bsc/flap_gas_20261031_w130100000.txt"),
        ),
        (
            "bsc/engine/crates/trading/fixtures/bsc/flap_gas_20261031_w130500000.txt",
            include_bytes!("../../trading/fixtures/bsc/flap_gas_20261031_w130500000.txt"),
        ),
    ];

    /// The gas evidence of the configured files under `repo_root`; each
    /// must be a file the economics snapshot binds with exactly its frozen
    /// content (H gate L9: a path alone is no binding), and at least one
    /// is named.
    pub fn gas(
        &self,
        repo_root: &Path,
    ) -> Result<darkbot_bsc_trading::economics::network::GasEvidence, String> {
        use darkbot_bsc_trading::economics::snapshot::BOUND_FILES;
        if self.gas_evidence.is_empty() {
            return Err("BSC_STRATEGY_C_GAS_EVIDENCE names no file".into());
        }
        let mut text = String::new();
        for p in &self.gas_evidence {
            if !BOUND_FILES.contains(&p.as_str()) {
                return Err(format!(
                    "{p}: not a file the economics snapshot binds (BSC_STRATEGY_C_GAS_EVIDENCE)"
                ));
            }
            let frozen = Self::FROZEN_GAS_EVIDENCE
                .iter()
                .find(|(f, _)| f == p)
                .map(|(_, bytes)| *bytes)
                .ok_or_else(|| format!("{p}: not frozen gas evidence of this build"))?;
            let path = repo_root.join(p);
            if std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))? != frozen {
                return Err(format!(
                    "{p}: differs from the frozen gas evidence of this build"
                ));
            }
            text.push_str(std::str::from_utf8(frozen).map_err(|e| format!("{p}: {e}"))?);
            text.push('\n');
        }
        darkbot_bsc_trading::economics::network::GasEvidence::parse(&text)
    }

    /// BSC3-079: all the frozen gas evidence of this build, from its
    /// compiled-in bytes (no file is read, none configured is needed).
    pub fn frozen_gas() -> Result<darkbot_bsc_trading::economics::network::GasEvidence, String> {
        let mut text = String::new();
        for (p, bytes) in Self::FROZEN_GAS_EVIDENCE {
            text.push_str(std::str::from_utf8(bytes).map_err(|e| format!("{p}: {e}"))?);
            text.push('\n');
        }
        darkbot_bsc_trading::economics::network::GasEvidence::parse(&text)
    }
}

/// BSC3-079: an executor's gas limits cover the frozen evidence of this
/// build, whatever Strategy C's configured files (and whether they load).
pub fn exec_gas_covered(config: &RuntimeConfig) -> Result<(), String> {
    match config.exec.as_ref() {
        Some(x) => gas_limits_cover(&x.exec, &StrategyConfig::frozen_gas()?),
        None => Ok(()),
    }
}

/// BSC3-079: each configured gas limit at least the largest gas used of
/// its class in `gas`: the BUY's of a BUY (any, and one in its token's
/// launch block, the C3 path's shape), the SELL's and the APPROVE's.
pub fn gas_limits_cover(
    exec: &crate::paper_exec::ExecConfig,
    gas: &darkbot_bsc_trading::economics::network::GasEvidence,
) -> Result<(), String> {
    use darkbot_bsc_trading::economics::network::{Action, PathClass, Venue};
    let largest = |action| {
        gas.class(PathClass {
            venue: Venue::Flap,
            action,
        })
        .map_or(0, |c| c.gas_used_max)
    };
    for (name, limit, actions) in [
        (
            "BSC_EXEC_GAS_LIMIT",
            exec.gas_limit,
            &[Action::Buy, Action::BuyInLaunchBlock][..],
        ),
        (
            "BSC_EXEC_SELL_GAS_LIMIT",
            exec.sell_gas_limit,
            &[Action::Sell][..],
        ),
        (
            "BSC_EXEC_APPROVE_GAS_LIMIT",
            exec.approve_gas_limit,
            &[Action::Approve][..],
        ),
    ] {
        for action in actions {
            let most = largest(*action);
            if limit < most {
                return Err(format!(
                    "{name} {limit} is below the largest {action:?} gas {most} of the frozen evidence"
                ));
            }
        }
    }
    Ok(())
}

/// The validated runtime configuration (`bsc/config/ingress.public.env`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// The public WSS endpoint of each source, if it has one.
    pub endpoints: Vec<(SourceId, String)>,
    pub queue_events: usize,
    pub dedup_hashes: usize,
    /// A connected source silent this long is DEGRADED.
    pub quiet_ms: u64,
    pub journal_queue: usize,
    /// The capture's byte budget, segment rotation and sync policy
    /// (`BSC_RUNTIME_JOURNAL_*`; Phase F storage).
    pub journal_budget_bytes: usize,
    pub journal_segment_bytes: u64,
    pub journal_segment_ms: u64,
    pub journal_sync_records: u64,
    pub journal_sync_ms: u64,
    /// Which pending transactions besides launches the journal keeps.
    pub capture_pending: CaptureScope,
    /// The capacity preflight of a scoped capture: its planned duration,
    /// the writer probe's bytes and the disk reserve.
    pub capture_duration_s: u64,
    pub preflight_probe_bytes: u64,
    /// The shortest sealed run whose rates the preflight measures.
    pub preflight_min_rates_s: u64,
    pub disk_reserve_bytes: u64,
    pub status_ms: u64,
    /// Every bound of the WSS adapters, validated by `WssLimits::validate`.
    pub wss: WssLimits,
    /// The canonical chain follower, when `BSC_CANONICAL_RPC` is set.
    pub canonical: Option<CanonicalConfig>,
    /// How often the census is replayed from the journals.
    pub census_ms: u64,
    /// The longest a pass waits for events before it reconciles sources,
    /// joins launches and checks the stop file.
    pub reconcile_ms: u64,
    /// The longest a queued canonical event waits for the ingress pass
    /// (`BSC_RUNTIME_CANONICAL_SLICE_MS`, at most the pass).
    pub canonical_slice_ms: u64,
    /// How long the operator waits for a requested stop before it kills the
    /// process; at least the derived worst-case stop path.
    pub stop_timeout_ms: u64,
    /// The C3 PAPER path (`BSC_EXEC_*`; it needs the canonical follower).
    pub exec: Option<ExecSetup>,
    /// The LIVE path (`BSC_LIVE_*`, plan 28.10.; it needs the executor's
    /// configuration and the buyer watched by C2).
    pub live: Option<live::LiveSetup>,
    /// The largest launch calldata the venue decoder accepts, at ingress
    /// and for the follower's C1 scope.
    pub max_launch_calldata: usize,
    /// Phase H: Strategy C (component `BSC_STRATEGY_C`).
    pub strategy: StrategyConfig,
    /// SHA-256 of the configuration text this was parsed from.
    pub source_sha256: [u8; 32],
}

/// The canonical follower's endpoint and bounds (`BSC_CANONICAL_*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalConfig {
    /// `https://` without credentials: the primary.
    pub endpoint: String,
    /// BSC3-056: the independent second source whose headers confirm the
    /// followed chain before a LIVE BUY (`BSC_CANONICAL_CONFIRM_*`).
    pub confirm: Option<ConfirmConfig>,
    /// BSC3-036: the endpoints asked in this order after the primary's
    /// `BSC_CANONICAL_FAILOVER_AFTER` failed steps
    /// (`BSC_CANONICAL_RPC_FALLBACK`, comma-separated, may be empty).
    pub fallbacks: Vec<String>,
    pub limits: FollowerLimits,
    /// A launch is joined this long after its block was seen, so sightings
    /// still queued behind the block count.
    pub join_delay_ms: u64,
    /// BSC3-062: how long before its block a launch's first sighting can
    /// be (`BSC_CANONICAL_JOIN_HORIZON_MS`): a launch without a remembered
    /// sighting is `Evicted`, not missed, while a hash first seen within it
    /// was forgotten for good.
    pub join_horizon_ms: u64,
    /// The C2 chain-state owner's bounds (it needs the follower).
    pub state: StateConfig,
    /// Phase E review L3: how often the Four.meme drift guard reads the
    /// live deployment against the freeze (`BSC_CANONICAL_FOUR_PROBE_MS`).
    pub four_probe_ms: u64,
    /// 01.11. (R1): at most this many curve reads of open LIVE lots at a
    /// time (`BSC_CANONICAL_LOT_READS_MAX`), and a lot read again no
    /// earlier than this after a failed or overtaken read
    /// (`BSC_CANONICAL_LOT_READ_RETRY_MS`).
    pub lot_reads_max: usize,
    pub lot_read_retry_ms: u64,
    /// A lot whose curve read is refused (an answer outside its ABI or no
    /// Portal state) this many times in a row is stranded
    /// (`BSC_CANONICAL_LOT_READ_REFUSED_MAX`): one bad answer, maybe from
    /// one endpoint, is read again.
    pub lot_read_refused_max: u32,
    /// REV.1.0 R3b (BSC3-024/041): at most this many receipt recoveries of
    /// unresolved LIVE transactions at a time
    /// (`BSC_CANONICAL_RECOVERY_READS_MAX`), and one asked again no earlier
    /// than this after an answer that decided nothing or a failure
    /// (`BSC_CANONICAL_RECOVERY_RETRY_MS`).
    pub recovery_reads_max: usize,
    pub recovery_retry_ms: u64,
    /// The search for the block where an unresolved transaction's nonce
    /// passes goes at most this many blocks below where it starts
    /// (`BSC_CANONICAL_RECOVERY_SEARCH_BLOCKS`): beyond, it is named.
    pub recovery_search_blocks: u64,
}

/// BSC3-056: the second source and how close it must be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfirmConfig {
    /// `https://`, of a host none of the followed endpoints has.
    pub endpoint: String,
    /// Its newest header at most this old (wall clock, ms).
    pub max_age_ms: u64,
    /// Its newest header at most this many blocks below the followed head.
    pub max_lag_blocks: u64,
}

impl CanonicalConfig {
    /// The primary and the fallbacks, in the order the follower asks them.
    pub fn endpoints(&self) -> Vec<String> {
        std::iter::once(self.endpoint.clone())
            .chain(self.fallbacks.iter().cloned())
            .collect()
    }

    /// The endpoint at `index` in that order (the follower's active one),
    /// the primary for any other index.
    pub fn endpoint_at(&self, index: usize) -> &str {
        index
            .checked_sub(1)
            .and_then(|i| self.fallbacks.get(i))
            .map_or(self.endpoint.as_str(), String::as_str)
    }
}

fn bounded(name: &str, value: &str, min: u64, max: u64) -> Result<u64, String> {
    let v: u64 = value
        .parse()
        .map_err(|_| format!("{name} is not an unsigned integer"))?;
    if !(min..=max).contains(&v) {
        return Err(format!("{name}={v} is outside {min}..={max}"));
    }
    Ok(v)
}

impl RuntimeConfig {
    /// Strict: every key known, every bound in range, endpoints `wss://`
    /// without credentials; anything else stops the runtime.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut values: HashMap<&str, &str> = HashMap::new();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("not KEY=VALUE: {line}"))?;
            if values.insert(k.trim(), v.trim()).is_some() {
                return Err(format!("{} is set twice", k.trim()));
            }
        }
        let mut endpoints = Vec::new();
        for source in [SourceId::PublicNode, SourceId::Drpc, SourceId::BlockReq] {
            let key = format!("{}_WSS", source.name());
            if let Some(url) = values.remove(key.as_str()) {
                // The adapter's own rule, so nothing it would refuse starts.
                Endpoint::parse(url).map_err(|e| format!("{key} is refused: {e:?}"))?;
                endpoints.push((source, url.to_string()));
            }
        }
        let canonical_rpc = values.remove("BSC_CANONICAL_RPC");
        let canonical_fallback = values.remove("BSC_CANONICAL_RPC_FALLBACK");
        let canonical_confirm = values.remove("BSC_CANONICAL_CONFIRM_RPC");
        let watched = values.remove("BSC_STATE_WATCHED");
        let paper_identity = values.remove("BSC_EXEC_PAPER_IDENTITY");
        let refused_accounts = values.remove("BSC_EXEC_REFUSED_ACCOUNTS");
        let sender_paths = values.remove("BSC_EXEC_SENDER_PATHS");
        let exit_slippage = values.remove("BSC_EXEC_EXIT_SLIPPAGE_BPS");
        let operator_exit = values.remove("BSC_EXEC_LIVE_OPERATOR_EXIT");
        let nonce_burn = values.remove("BSC_EXEC_LIVE_NONCE_BURN");
        let public_only = values.remove("BSC_EXEC_LIVE_PUBLIC_ONLY");
        let risk_outlier = values.remove("BSC_EXEC_LIVE_RISK_OUTLIER");
        let c3_rearm = values.remove("BSC_EXEC_LIVE_C3_REARM");
        let builder_policy = values.remove("BSC_EXEC_LIVE_BUILDER_POLICY");
        let live_key = values.remove("BSC_LIVE_KEY");
        let live_buyer = values.remove("BSC_LIVE_BUYER");
        let live_route = values.remove("BSC_LIVE_ROUTE");
        let live_token = values.remove("BSC_LIVE_BLOCKRAZOR_TOKEN_FILE");
        let capture_pending = CaptureScope::parse(
            values
                .remove("BSC_RUNTIME_CAPTURE_PENDING")
                .ok_or("BSC_RUNTIME_CAPTURE_PENDING is required")?,
        )?;
        let mut text_of = |name: &str| -> Result<String, String> {
            values
                .remove(name)
                .map(str::to_string)
                .ok_or_else(|| format!("{name} is required"))
        };
        let strategy_filters = text_of("BSC_STRATEGY_C_FILTERS")?;
        let strategy_target = text_of("BSC_STRATEGY_C_TARGET_NET_MULTIPLE")?;
        let strategy_stop = text_of("BSC_STRATEGY_C_STOP_NET_MULTIPLE")?;
        let strategy_creator = text_of("BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL")?;
        let strategy_gas = text_of("BSC_STRATEGY_C_GAS_EVIDENCE")?;
        let mut take = |name: &str, min: u64, max: u64| -> Result<u64, String> {
            let v = values
                .remove(name)
                .ok_or_else(|| format!("{name} is required"))?;
            bounded(name, v, min, max)
        };
        let ms = Duration::from_millis;
        let max_launch_calldata = take("BSC_RUNTIME_MAX_LAUNCH_CALLDATA", 1_024, 1 << 20)? as usize;
        let canonical = match canonical_rpc {
            None => None,
            Some(url) => {
                let rest = url
                    .strip_prefix("https://")
                    .ok_or("BSC_CANONICAL_RPC must be https://")?;
                // The follower's own rule (the authority and path rules of wss://).
                Endpoint::parse(&format!("wss://{rest}"))
                    .map_err(|e| format!("BSC_CANONICAL_RPC is refused: {e:?}"))?;
                let limits = FollowerLimits {
                    poll: ms(take("BSC_CANONICAL_POLL_MS", 50, 60_000)?),
                    reorg_depth: take("BSC_CANONICAL_REORG_DEPTH", 1, 10_000)?,
                    max_catch_up: take("BSC_CANONICAL_MAX_CATCH_UP", 1, 100_000)?,
                    connect_timeout: ms(take("BSC_CANONICAL_CONNECT_TIMEOUT_MS", 100, 60_000)?),
                    handshake_timeout: ms(take(
                        "BSC_CANONICAL_HANDSHAKE_TIMEOUT_MS",
                        100,
                        120_000,
                    )?),
                    read_tick: ms(take("BSC_CANONICAL_READ_TICK_MS", 10, 5_000)?),
                    write_timeout: ms(take("BSC_CANONICAL_WRITE_TIMEOUT_MS", 100, 60_000)?),
                    request_timeout: ms(take("BSC_CANONICAL_REQUEST_TIMEOUT_MS", 100, 300_000)?),
                    max_response_bytes: take("BSC_CANONICAL_MAX_RESPONSE_BYTES", 65_536, 1 << 30)?
                        as usize,
                    backoff_initial: ms(take("BSC_CANONICAL_BACKOFF_INITIAL_MS", 100, 60_000)?),
                    backoff_max: ms(take("BSC_CANONICAL_BACKOFF_MAX_MS", 1_000, 3_600_000)?),
                    tx_attempts: take("BSC_CANONICAL_TX_ATTEMPTS", 1, 100)? as u32,
                    max_launch_calldata,
                    failover_after: take("BSC_CANONICAL_FAILOVER_AFTER", 1, 100)? as u32,
                    stall_after: ms(take("BSC_CANONICAL_STALL_MS", 1_000, 600_000)?),
                };
                // BSC3-036: each fallback by the follower's own rule, none
                // the primary or repeated.
                // Never a path in an error (it may hold a provider's key).
                let key_of = |e: &str| crate::chain_follower::endpoint_key(e);
                let mut fallbacks: Vec<String> = Vec::new();
                let mut keys =
                    vec![key_of(url).map_err(|e| format!("BSC_CANONICAL_RPC is refused: {e:?}"))?];
                for (i, f) in canonical_fallback
                    .unwrap_or_default()
                    .split(',')
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .enumerate()
                {
                    let key = key_of(f).map_err(|e| {
                        format!(
                            "BSC_CANONICAL_RPC_FALLBACK entry {} is refused: {e:?}",
                            i + 1
                        )
                    })?;
                    if keys.contains(&key) {
                        return Err(format!("BSC_CANONICAL_RPC_FALLBACK names {} twice", key.0));
                    }
                    keys.push(key);
                    fallbacks.push(f.to_string());
                }
                // BSC3-056: independent of every followed endpoint (another
                // host), or none.
                let max_age_ms = take("BSC_CANONICAL_CONFIRM_MAX_AGE_MS", 100, 600_000)?;
                let max_lag_blocks = take("BSC_CANONICAL_CONFIRM_MAX_LAG_BLOCKS", 0, 64)?;
                let confirm = match canonical_confirm.map(str::trim).filter(|c| !c.is_empty()) {
                    None => None,
                    Some(c) => {
                        let (host, _) = key_of(c)
                            .map_err(|e| format!("BSC_CANONICAL_CONFIRM_RPC is refused: {e:?}"))?;
                        if keys.iter().any(|(h, _)| *h == host) {
                            return Err(format!(
                                "BSC_CANONICAL_CONFIRM_RPC {host} is a followed endpoint's host: no independent second source"
                            ));
                        }
                        Some(ConfirmConfig {
                            endpoint: c.to_string(),
                            max_age_ms,
                            max_lag_blocks,
                        })
                    }
                };
                limits
                    .validate()
                    .map_err(|e| format!("canonical bounds: {e:?}"))?;
                let state = StateConfig {
                    limits: StateLimits {
                        max_txs: take("BSC_STATE_MAX_TXS", 1_024, 1 << 24)? as usize,
                        max_candidates_per_nonce: take(
                            "BSC_STATE_MAX_CANDIDATES_PER_NONCE",
                            1,
                            1_024,
                        )? as usize,
                        max_watched_senders: take("BSC_STATE_MAX_WATCHED_SENDERS", 1, 1_024)?
                            as usize,
                        // The follower's kept window: what it reports exactly
                        // is applied exactly.
                        reorg_window: limits.reorg_depth,
                        finality_depth: take("BSC_STATE_FINALITY_DEPTH", 1, 10_000)?,
                    },
                    watched: StateConfig::parse_watched(
                        watched.ok_or("BSC_STATE_WATCHED is required")?,
                    )?,
                    checkpoint_ms: take("BSC_STATE_CHECKPOINT_MS", 1_000, 3_600_000)?,
                    nonce_fact_blocks: take("BSC_STATE_NONCE_FACT_BLOCKS", 1, 100_000)?,
                };
                state.validate()?;
                Some(CanonicalConfig {
                    endpoint: url.to_string(),
                    confirm,
                    fallbacks,
                    limits,
                    join_delay_ms: take("BSC_CANONICAL_JOIN_DELAY_MS", 0, 600_000)?,
                    join_horizon_ms: take("BSC_CANONICAL_JOIN_HORIZON_MS", 1_000, 86_400_000)?,
                    four_probe_ms: take("BSC_CANONICAL_FOUR_PROBE_MS", 60_000, 86_400_000)?,
                    lot_reads_max: usize::try_from(take("BSC_CANONICAL_LOT_READS_MAX", 1, 64)?)
                        .map_err(|_| "BSC_CANONICAL_LOT_READS_MAX")?,
                    lot_read_retry_ms: take("BSC_CANONICAL_LOT_READ_RETRY_MS", 100, 600_000)?,
                    lot_read_refused_max: u32::try_from(take(
                        "BSC_CANONICAL_LOT_READ_REFUSED_MAX",
                        1,
                        64,
                    )?)
                    .map_err(|_| "BSC_CANONICAL_LOT_READ_REFUSED_MAX")?,
                    recovery_reads_max: usize::try_from(take(
                        "BSC_CANONICAL_RECOVERY_READS_MAX",
                        1,
                        64,
                    )?)
                    .map_err(|_| "BSC_CANONICAL_RECOVERY_READS_MAX")?,
                    recovery_retry_ms: take("BSC_CANONICAL_RECOVERY_RETRY_MS", 100, 3_600_000)?,
                    recovery_search_blocks: take(
                        "BSC_CANONICAL_RECOVERY_SEARCH_BLOCKS",
                        1,
                        10_000_000,
                    )?,
                    state,
                })
            }
        };
        if canonical.is_none() && watched.is_some() {
            return Err("BSC_STATE_WATCHED needs BSC_CANONICAL_RPC".into());
        }
        let exec = match paper_identity {
            None => {
                if refused_accounts.is_some() {
                    return Err("BSC_EXEC_REFUSED_ACCOUNTS needs BSC_EXEC_PAPER_IDENTITY".into());
                }
                if sender_paths.is_some() {
                    return Err("BSC_EXEC_SENDER_PATHS needs BSC_EXEC_PAPER_IDENTITY".into());
                }
                if exit_slippage.is_some() {
                    return Err("BSC_EXEC_EXIT_SLIPPAGE_BPS needs BSC_EXEC_PAPER_IDENTITY".into());
                }
                if operator_exit.is_some() {
                    return Err("BSC_EXEC_LIVE_OPERATOR_EXIT needs BSC_EXEC_PAPER_IDENTITY".into());
                }
                if nonce_burn.is_some() {
                    return Err("BSC_EXEC_LIVE_NONCE_BURN needs BSC_EXEC_PAPER_IDENTITY".into());
                }
                if public_only.is_some() {
                    return Err("BSC_EXEC_LIVE_PUBLIC_ONLY needs BSC_EXEC_PAPER_IDENTITY".into());
                }
                if risk_outlier.is_some() {
                    return Err("BSC_EXEC_LIVE_RISK_OUTLIER needs BSC_EXEC_PAPER_IDENTITY".into());
                }
                if c3_rearm.is_some() {
                    return Err("BSC_EXEC_LIVE_C3_REARM needs BSC_EXEC_PAPER_IDENTITY".into());
                }
                if builder_policy.is_some() {
                    return Err("BSC_EXEC_LIVE_BUILDER_POLICY needs BSC_EXEC_PAPER_IDENTITY".into());
                }
                None
            }
            Some(_) if canonical.is_none() => {
                return Err("BSC_EXEC_PAPER_IDENTITY needs BSC_CANONICAL_RPC (C2)".into());
            }
            Some(path) => Some({
                let slippage_bps = take("BSC_EXEC_SLIPPAGE_BPS", 0, 9_999)? as u16;
                // BSC3-069: an exit's own limit; the BUY's when not set.
                let exit_slippage_bps = match exit_slippage {
                    Some(v) => bounded("BSC_EXEC_EXIT_SLIPPAGE_BPS", v, 0, 9_999)? as u16,
                    None => slippage_bps,
                };
                ExecSetup {
                    identity_path: ExecSetup::expand_home(path)?,
                    reprobe: ms(take("BSC_EXEC_REPROBE_MS", 1_000, 3_600_000)?),
                    reprobe_failures: take("BSC_EXEC_REPROBE_FAILURES", 0, 100)? as u32,
                    refused_accounts: StateConfig::parse_watched(
                        refused_accounts.ok_or("BSC_EXEC_REFUSED_ACCOUNTS is required")?,
                    )
                    .map_err(|e| e.replace("BSC_STATE_WATCHED", "BSC_EXEC_REFUSED_ACCOUNTS"))?,
                    exec: ExecConfig {
                        input_wei: u128::from(take("BSC_EXEC_INPUT_WEI", 1, u64::MAX)?),
                        slippage_bps,
                        exit_slippage_bps,
                        gas_price_wei: take("BSC_EXEC_GAS_PRICE_WEI", 1, 1_000_000_000_000)?,
                        gas_limit: take("BSC_EXEC_GAS_LIMIT", 21_000, 100_000_000)?,
                        session_cap_wei: u128::from(take("BSC_EXEC_SESSION_CAP_WEI", 1, u64::MAX)?),
                        lanes: take("BSC_EXEC_LANES", 1, 65_536)? as usize,
                        max_calldata: take("BSC_EXEC_MAX_CALLDATA", 1_024, 1 << 20)? as usize,
                        latency_samples: take("BSC_EXEC_LATENCY_SAMPLES", 1, 1 << 20)? as usize,
                        policy_generation: take(
                            "BSC_EXEC_POLICY_GENERATION",
                            1,
                            u64::from(u32::MAX),
                        )?,
                        capability_generation: take(
                            "BSC_EXEC_CAPABILITY_GENERATION",
                            1,
                            u64::from(u32::MAX),
                        )?,
                        validity_blocks: take(
                            "BSC_EXEC_VALIDITY_BLOCKS",
                            1,
                            PAPER_PROFILE.max_block_window(),
                        )?,
                        start_limit_ns: take(
                            "BSC_EXEC_START_LIMIT_NS",
                            1,
                            PAPER_PROFILE.max_transport_start_limit_ns(),
                        )?,
                        max_frame: take("BSC_EXEC_MAX_FRAME_BYTES", 4_096, 1 << 20)? as usize,
                        strategy_lanes: take("BSC_EXEC_STRATEGY_LANES", 1, 65_536)? as usize,
                        strategy_session_cap_wei: u128::from(take(
                            "BSC_EXEC_STRATEGY_SESSION_CAP_WEI",
                            1,
                            u64::MAX,
                        )?),
                        approve_gas_limit: take("BSC_EXEC_APPROVE_GAS_LIMIT", 21_000, 100_000_000)?,
                        sell_gas_limit: take("BSC_EXEC_SELL_GAS_LIMIT", 21_000, 100_000_000)?,
                        strategy_max_head_age_ms: take(
                            "BSC_EXEC_STRATEGY_MAX_HEAD_AGE_MS",
                            1_000,
                            600_000,
                        )?,
                        live_max_head_age_ms: take(
                            "BSC_EXEC_LIVE_MAX_HEAD_AGE_MS",
                            1_000,
                            600_000,
                        )?,
                        live_exit_reverts_max: take("BSC_EXEC_LIVE_EXIT_REVERTS_MAX", 1, 16)?,
                        live_lease_blocks: take("BSC_EXEC_LIVE_LEASE_BLOCKS", 2, 28_800)?,
                        live_operator_exit: match operator_exit {
                            Some("ON") => true,
                            Some("OFF") => false,
                            Some(v) => {
                                return Err(format!(
                                    "BSC_EXEC_LIVE_OPERATOR_EXIT={v} is neither ON nor OFF"
                                ));
                            }
                            None => return Err("BSC_EXEC_LIVE_OPERATOR_EXIT is required".into()),
                        },
                        live_nonce_burn: match nonce_burn {
                            Some("ON") => true,
                            Some("OFF") => false,
                            Some(v) => {
                                return Err(format!(
                                    "BSC_EXEC_LIVE_NONCE_BURN={v} is neither ON nor OFF"
                                ));
                            }
                            None => return Err("BSC_EXEC_LIVE_NONCE_BURN is required".into()),
                        },
                        live_public_only: match public_only {
                            Some("ON") => true,
                            Some("OFF") => false,
                            Some(v) => {
                                return Err(format!(
                                    "BSC_EXEC_LIVE_PUBLIC_ONLY={v} is neither ON nor OFF"
                                ));
                            }
                            None => return Err("BSC_EXEC_LIVE_PUBLIC_ONLY is required".into()),
                        },
                        live_grant_max_duration_ms: take(
                            "BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS",
                            0,
                            31_536_000_000,
                        )?,
                        live_spend_total_wei: u128::from(take(
                            "BSC_EXEC_LIVE_SPEND_TOTAL_WEI",
                            0,
                            u64::MAX,
                        )?),
                        live_risk_soft_buy_slippage_bps: u16::try_from(take(
                            "BSC_EXEC_LIVE_RISK_SOFT_BUY_SLIPPAGE_BPS",
                            0,
                            9_999,
                        )?)
                        .map_err(|_| "BSC_EXEC_LIVE_RISK_SOFT_BUY_SLIPPAGE_BPS out of range")?,
                        live_risk_soft_exit_slippage_bps: u16::try_from(take(
                            "BSC_EXEC_LIVE_RISK_SOFT_EXIT_SLIPPAGE_BPS",
                            0,
                            9_999,
                        )?)
                        .map_err(|_| "BSC_EXEC_LIVE_RISK_SOFT_EXIT_SLIPPAGE_BPS out of range")?,
                        live_risk_soft_gas_price_wei: take(
                            "BSC_EXEC_LIVE_RISK_SOFT_GAS_PRICE_WEI",
                            1,
                            u64::MAX,
                        )?,
                        live_risk_outlier: match risk_outlier {
                            Some("ON") => true,
                            Some("OFF") => false,
                            Some(v) => {
                                return Err(format!(
                                    "BSC_EXEC_LIVE_RISK_OUTLIER={v} is neither ON nor OFF"
                                ));
                            }
                            None => return Err("BSC_EXEC_LIVE_RISK_OUTLIER is required".into()),
                        },
                        live_c3_rearm: match c3_rearm {
                            Some("ON") => true,
                            Some("OFF") => false,
                            Some(v) => {
                                return Err(format!(
                                    "BSC_EXEC_LIVE_C3_REARM={v} is neither ON nor OFF"
                                ));
                            }
                            None => return Err("BSC_EXEC_LIVE_C3_REARM is required".into()),
                        },
                        live_builder_policy: match builder_policy {
                            Some(p)
                                if !p.is_empty()
                                    && std::path::Path::new(p)
                                        .components()
                                        .all(|c| matches!(c, std::path::Component::Normal(_))) =>
                            {
                                p.to_string()
                            }
                            Some(p) => {
                                return Err(format!(
                                    "BSC_EXEC_LIVE_BUILDER_POLICY={p} is not a repository-relative path"
                                ));
                            }
                            None => return Err("BSC_EXEC_LIVE_BUILDER_POLICY is required".into()),
                        },
                        sender_paths: crate::paper_exec::SenderPaths::parse(
                            sender_paths.ok_or("BSC_EXEC_SENDER_PATHS is required")?,
                        )?,
                    },
                }
            }),
        };
        if let Some(x) = &exec {
            // Cross-key rules: an armed path must be able to admit a BUY,
            // and take every launch the ingress counts as one.
            x.exec.validate()?;
            if x.exec.max_calldata < max_launch_calldata {
                return Err(format!(
                    "BSC_EXEC_MAX_CALLDATA {} is below BSC_RUNTIME_MAX_LAUNCH_CALLDATA {max_launch_calldata}",
                    x.exec.max_calldata
                ));
            }
            // R3b review: the receipt recovery's search reaches over a
            // lease's whole window.
            if let Some(c) = canonical.as_ref()
                && c.recovery_search_blocks < x.exec.live_lease_blocks
            {
                return Err(format!(
                    "BSC_CANONICAL_RECOVERY_SEARCH_BLOCKS {} is below BSC_EXEC_LIVE_LEASE_BLOCKS {}: a lease's BUY early in its window would never be found",
                    c.recovery_search_blocks, x.exec.live_lease_blocks
                ));
            }
        }
        let live = match (live_key, live_buyer, live_route) {
            (None, None, None) => {
                if live_token.is_some() {
                    return Err("BSC_LIVE_BLOCKRAZOR_TOKEN_FILE needs BSC_LIVE_ROUTE".into());
                }
                None
            }
            (Some(key), Some(buyer), Some(route)) => {
                if exec.is_none() {
                    return Err("BSC_LIVE_* needs the executor's BSC_EXEC_* keys".into());
                }
                // R1 review: every LIVE lot's exit goes Strategy C's path; a
                // sender without it could buy and never sell.
                if exec
                    .as_ref()
                    .is_some_and(|x| !x.exec.sender_paths.strategy())
                {
                    return Err("BSC_LIVE_* needs BSC_EXEC_SENDER_PATHS with STRATEGY_C: every LIVE lot's exit goes its path".into());
                }
                let [buyer] = <[[u8; 20]; 1]>::try_from(
                    StateConfig::parse_watched(buyer)
                        .map_err(|e| e.replace("BSC_STATE_WATCHED", "BSC_LIVE_BUYER"))?,
                )
                .map_err(|_| "BSC_LIVE_BUYER is one 0x address")?;
                if !canonical
                    .as_ref()
                    .is_some_and(|c| c.state.watched.contains(&buyer))
                {
                    return Err("BSC_LIVE_BUYER must be one of BSC_STATE_WATCHED (C2)".into());
                }
                // BSC3-056: no LIVE path without an independent second
                // source (its BUY gate is never silently off).
                if canonical.as_ref().is_none_or(|c| c.confirm.is_none()) {
                    return Err("BSC_LIVE_* needs BSC_CANONICAL_CONFIRM_RPC, an independent second source (BSC3-056)".into());
                }
                let token = live_token.map(ExecSetup::expand_home).transpose()?;
                Some(live::LiveSetup {
                    key_path: ExecSetup::expand_home(key)?,
                    buyer,
                    route: live::LiveRouteConfig::parse(route, token)?,
                    connect_timeout: ms(take("BSC_LIVE_CONNECT_TIMEOUT_MS", 100, 60_000)?),
                })
            }
            _ => return Err("BSC_LIVE_KEY, BSC_LIVE_BUYER and BSC_LIVE_ROUTE go together".into()),
        };
        let wss = WssLimits {
            connect_timeout: ms(take("BSC_RUNTIME_CONNECT_TIMEOUT_MS", 100, 60_000)?),
            handshake_timeout: ms(take("BSC_RUNTIME_HANDSHAKE_TIMEOUT_MS", 100, 120_000)?),
            read_tick: ms(take("BSC_RUNTIME_READ_TICK_MS", 10, 5_000)?),
            write_timeout: ms(take("BSC_RUNTIME_WRITE_TIMEOUT_MS", 100, 60_000)?),
            idle_timeout: ms(take("BSC_RUNTIME_IDLE_TIMEOUT_MS", 1_000, 3_600_000)?),
            max_frame_bytes: take("BSC_RUNTIME_MAX_FRAME_BYTES", 1_024, 16 << 20)? as usize,
            max_message_bytes: take("BSC_RUNTIME_MAX_MESSAGE_BYTES", 65_536, 16 << 20)? as usize,
            backoff_initial: ms(take("BSC_RUNTIME_BACKOFF_INITIAL_MS", 100, 60_000)?),
            backoff_max: ms(take("BSC_RUNTIME_BACKOFF_MAX_MS", 1_000, 3_600_000)?),
            stable_after: ms(take("BSC_RUNTIME_STABLE_AFTER_MS", 1_000, 3_600_000)?),
        };
        let config = RuntimeConfig {
            endpoints,
            queue_events: take("BSC_RUNTIME_QUEUE_EVENTS", 64, 1 << 22)? as usize,
            dedup_hashes: take("BSC_RUNTIME_DEDUP_HASHES", 1024, 1 << 24)? as usize,
            quiet_ms: take("BSC_RUNTIME_QUIET_MS", 1000, 600_000)?,
            journal_queue: take("BSC_RUNTIME_JOURNAL_QUEUE", 16, 1 << 20)? as usize,
            journal_budget_bytes: take("BSC_RUNTIME_JOURNAL_BUDGET_BYTES", 1 << 20, 1 << 32)?
                as usize,
            journal_segment_bytes: take("BSC_RUNTIME_JOURNAL_SEGMENT_BYTES", 1 << 20, 1 << 32)?,
            journal_segment_ms: take("BSC_RUNTIME_JOURNAL_SEGMENT_MS", 1_000, 86_400_000)?,
            journal_sync_records: take("BSC_RUNTIME_JOURNAL_SYNC_RECORDS", 1, 1 << 20)?,
            journal_sync_ms: take("BSC_RUNTIME_JOURNAL_SYNC_MS", 1, 60_000)?,
            capture_pending,
            capture_duration_s: take("BSC_RUNTIME_CAPTURE_DURATION_S", 60, 604_800)?,
            preflight_probe_bytes: take("BSC_RUNTIME_PREFLIGHT_PROBE_BYTES", 1 << 20, 1 << 32)?,
            preflight_min_rates_s: take("BSC_RUNTIME_PREFLIGHT_MIN_RATES_S", 1, 604_800)?,
            disk_reserve_bytes: take("BSC_RUNTIME_DISK_RESERVE_BYTES", 0, 1 << 40)?,
            status_ms: take("BSC_RUNTIME_STATUS_MS", 100, 60_000)?,
            wss,
            canonical,
            census_ms: take("BSC_RUNTIME_CENSUS_MS", 1_000, 86_400_000)?,
            reconcile_ms: take("BSC_RUNTIME_RECONCILE_MS", 10, 60_000)?,
            canonical_slice_ms: take("BSC_RUNTIME_CANONICAL_SLICE_MS", 1, 60_000)?,
            // Below the launchd ExitTimeOut (330 s) of the runtime job in
            // ops/bsc-control.sh, so a SIGTERM stop is never cut by SIGKILL.
            stop_timeout_ms: take("BSC_RUNTIME_STOP_TIMEOUT_MS", 1_000, 300_000)?,
            exec,
            live,
            max_launch_calldata,
            strategy: {
                use darkbot_bsc_trading::strategy::{filters::parse_rules, fixed};
                let places = take("BSC_STRATEGY_C_RATIO_PLACES", 0, 18)? as u32;
                let multiple = |key: &str, text: &str| {
                    fixed::parse(text, places)
                        .filter(|m| *m >= 0)
                        .ok_or_else(|| {
                            format!("{key}={text} is no non-negative decimal at {places} places")
                        })
                };
                StrategyConfig {
                    engine: darkbot_bsc_trading::strategy::engine::Config {
                        observation_ms: take("BSC_STRATEGY_C_OBSERVATION_MS", 0, 3_600_000)?,
                        ratio_places: places,
                        first_buyer_window: take("BSC_STRATEGY_C_FIRST_BUYER_WINDOW", 1, 100)?
                            as usize,
                        rules: parse_rules(&strategy_filters, places)?,
                        target_net_multiple: multiple(
                            "BSC_STRATEGY_C_TARGET_NET_MULTIPLE",
                            &strategy_target,
                        )?,
                        stop_net_multiple: multiple(
                            "BSC_STRATEGY_C_STOP_NET_MULTIPLE",
                            &strategy_stop,
                        )?,
                        hold_ms: take("BSC_STRATEGY_C_HOLD_MS", 1, 86_400_000)?,
                        max_open: take("BSC_STRATEGY_C_MAX_OPEN", 1, 1_000)? as usize,
                        input_wei: alloy_primitives::U256::from(take(
                            "BSC_STRATEGY_C_INPUT_WEI",
                            1,
                            u64::MAX,
                        )?),
                        fill_delay_blocks: take("BSC_STRATEGY_C_FILL_DELAY_BLOCKS", 1, 100)?,
                        creator_sold_before_fill: crate::runtime_strategy::creator_sold_policy(
                            &strategy_creator,
                        )?,
                        gas_price_wei: alloy_primitives::U256::from(take(
                            "BSC_STRATEGY_C_GAS_PRICE_WEI",
                            1,
                            1_000_000_000_000,
                        )?),
                    },
                    gas_evidence: strategy_gas
                        .split(',')
                        .map(|p| p.trim().to_string())
                        .filter(|p| !p.is_empty())
                        .collect(),
                }
            },
            source_sha256: darkbot_bsc_record::sha256(text.as_bytes()),
        };
        config
            .wss
            .validate()
            .map_err(|e| format!("WSS bounds: {e:?}"))?;
        journal_setup(&config).validate()?;
        if config.canonical_slice_ms > config.reconcile_ms {
            return Err(format!(
                "BSC_RUNTIME_CANONICAL_SLICE_MS {} is above BSC_RUNTIME_RECONCILE_MS {}",
                config.canonical_slice_ms, config.reconcile_ms
            ));
        }
        // Phase I gate L2: the strategy prices the network cost at the gas
        // price its PAPER intents are signed with.
        if let Some(x) = &config.exec
            && alloy_primitives::U256::from(x.exec.gas_price_wei)
                != config.strategy.engine.gas_price_wei
        {
            return Err(format!(
                "BSC_STRATEGY_C_GAS_PRICE_WEI {} differs from BSC_EXEC_GAS_PRICE_WEI {}",
                config.strategy.engine.gas_price_wei, x.exec.gas_price_wei
            ));
        }
        let needed = config.stop_bound_ms();
        if config.stop_timeout_ms < needed {
            return Err(format!(
                "BSC_RUNTIME_STOP_TIMEOUT_MS={} is below the derived stop path of {needed} ms",
                config.stop_timeout_ms
            ));
        }
        if let Some(unknown) = values.keys().next() {
            return Err(format!("unknown key {unknown}"));
        }
        Ok(config)
    }

    /// The worst-case stop path: one pass, then the adapters and the
    /// follower in parallel (a TCP connect in flight, then the closing write
    /// or one more read tick). Name resolution has no bound of its own.
    pub fn stop_bound_ms(&self) -> u64 {
        let ms = |d: Duration| u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
        let w = &self.wss;
        let adapters = ms(w.connect_timeout)
            .saturating_add(ms(w.write_timeout))
            .saturating_add(ms(w.read_tick));
        let follower = self.canonical.as_ref().map_or(0, |c| {
            ms(c.limits.connect_timeout).saturating_add(ms(c.limits.read_tick))
        });
        self.reconcile_ms.saturating_add(adapters.max(follower))
    }

    fn endpoint(&self, source: SourceId) -> Option<&str> {
        self.endpoints
            .iter()
            .find(|(s, _)| *s == source)
            .map(|(_, u)| u.as_str())
    }
}

/// One source event as it crosses from an adapter thread to the runtime.
enum Event {
    Full {
        hash: [u8; 32],
        raw: Vec<u8>,
        /// The verified signer and nonce (the C2 pending input).
        from: [u8; 20],
        nonce: u64,
    },
    HashOnly {
        hash: [u8; 32],
    },
    Rejected,
    Connected,
    Disconnected {
        rate_limited: bool,
    },
}

struct Adapter {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

/// Every LIVE transaction a Strategy C decision handed over, journaled.
fn journal_live(
    journal: &mut RuntimeJournal,
    intents: &[crate::paper_exec::ExecIntent<crate::live_end::LiveHandoff>],
) {
    for i in intents {
        journal.record(
            RecordKind::Runtime,
            i.times.sink_ns,
            crate::runtime_journal::live_release_payload(
                i.kind.code(),
                None,
                &i.handoff,
                [0; 32],
                i.token,
                i.block,
                &i.times,
                &i.raw,
            ),
        );
    }
}

/// REV.1.0 R3b: at a block's complete state, before any decision takes the
/// next nonce, the operator's nonce burn when one is wanted and the LIVE
/// path is armed ([`LiveRuntime::burn`]), journaled as its release.
fn burn_nonce(
    live: &mut Option<LiveRuntime>,
    owner: &Option<StateOwner>,
    clock: Clock,
    journal: &mut RuntimeJournal,
) {
    let (Some(l), Some(o)) = (live.as_mut(), owner.as_ref()) else {
        return;
    };
    if !l.armed() {
        return;
    }
    let now_unix_ms = wall_or_expired(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| u64::try_from(d.as_millis()).ok())
            .ok_or_else(|| "wall clock before 1970".to_string()),
    );
    let clk = FnClock {
        id: CLOCK_ID_MONOTONIC,
        now: || clock.now_ns(),
    };
    let intents = l.burn(o, &clk, now_unix_ms);
    journal_live(journal, &intents);
}

/// Phase I: Strategy C's decisions of the records just applied go to the
/// armed C3 executor (`BSC_EXEC` ARMED), on C2's canonical snapshot; each
/// PAPER transaction at the sink is journaled (`J`). Without an armed
/// executor they are counted (`paper_unarmed`), never executed.
fn strategy_to_paper(
    c: &mut Canonical,
    exec: &mut Option<ExecRuntime>,
    live: &mut Option<LiveRuntime>,
    owner: &Option<StateOwner>,
    clock: Clock,
    journal: &mut RuntimeJournal,
    block: u64,
) {
    let Some(s) = c.strategy.as_mut() else {
        return;
    };
    // Phase I gate M1: the moment decisions are acted on at the block's
    // complete state (SHADOW or PAPER), so a replay places them there.
    if s.has_pending() {
        journal.record(
            RecordKind::Runtime,
            clock.now_ns(),
            crate::runtime_strategy::action_payload(block),
        );
    }
    // Gate L-a: the head's age against the wall clock now; unreadable, every
    // head is too old.
    let now_unix_ms = wall_or_expired(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| u64::try_from(d.as_millis()).ok())
            .ok_or_else(|| "wall clock before 1970".to_string()),
    );
    let clk = FnClock {
        id: CLOCK_ID_MONOTONIC,
        now: || clock.now_ns(),
    };
    // BSC3-055: a LIVE exit whose approval is canonical now sells.
    if let (Some(l), Some(o)) = (live.as_mut(), owner.as_ref())
        && l.armed()
    {
        for token in l.exits_to_continue(o) {
            let Some(action) = s.exit_action(token, clock.now_ns()) else {
                continue;
            };
            if let Some((intents, _refused)) = l.on_strategy(&action, o, &clk, now_unix_ms) {
                journal_live(journal, &intents);
            }
        }
    }
    let mut actions = s.take_actions();
    // BSC3-048: on the LIVE path a block's exits go before its BUYs (a
    // stable order otherwise; PAPER keeps the decisions' order).
    if live.as_ref().is_some_and(LiveRuntime::armed) {
        actions.sort_by_key(|a| !matches!(a, crate::runtime_strategy::StrategyAction::Exit { .. }));
    }
    for action in actions {
        // Plan 28.10.: the LIVE path takes the decision when it is armed.
        if let (Some(l), Some(o)) = (live.as_mut(), owner.as_ref())
            && l.armed()
        {
            if let Some((intents, _refused)) = l.on_strategy(&action, o, &clk, now_unix_ms) {
                journal_live(journal, &intents);
            }
            continue;
        }
        // Review of R4: a LIVE lot's exit waits for the LIVE path's next
        // arm, never PAPER's.
        if let (Some(l), crate::runtime_strategy::StrategyAction::Exit { token, .. }) =
            (live.as_mut(), &action)
            && l.want_unarmed_exit(*token)
        {
            continue;
        }
        let handed = match (exec.as_mut(), owner.as_ref()) {
            (Some(x), Some(o)) => x.on_strategy(
                &action,
                o,
                &FnClock {
                    id: CLOCK_ID_MONOTONIC,
                    now: || clock.now_ns(),
                },
                now_unix_ms,
            ),
            _ => None,
        };
        match handed {
            // Every transaction at the sink is journaled, an exit's
            // approval also when its SELL was refused (refusals are the
            // executor's counters).
            Some((intents, _refused)) => {
                for i in &intents {
                    journal.record(
                        RecordKind::Runtime,
                        i.times.sink_ns,
                        crate::runtime_strategy::paper_intent_payload(i),
                    );
                }
            }
            None => s.counters.paper_unarmed += 1,
        }
    }
}

/// 01.11. (R1): tells the LIVE path who decides its open lots' exits now:
/// those the strategy holds on a tracked curve are decided, the stranded
/// ones are named, every other open lot is undecided (no strategy: all).
fn refresh_lot_deciders(c: &mut Canonical, live: &mut Option<LiveRuntime>) {
    let Some(l) = live.as_mut() else {
        return;
    };
    c.lot.open = l
        .open_positions()
        .into_iter()
        .filter_map(|p| match p {
            live::PositionChange::Opened { token, .. } => Some(token),
            live::PositionChange::Ended { .. } => None,
        })
        .collect();
    let lot = &mut c.lot;
    lot.stranded.retain(|(t, _)| lot.open.contains(t));
    lot.retry_at.retain(|(t, _)| lot.open.contains(t));
    lot.refused.retain(|(t, _)| lot.open.contains(t));
    let strategy = c.strategy.as_ref();
    let undecided = lot
        .open
        .iter()
        .copied()
        .filter(|t| {
            !strategy.is_some_and(|s| s.holds(t)) && !lot.stranded.iter().any(|(s, _)| s == t)
        })
        .collect();
    l.set_exit_deciders(
        undecided,
        lot.stranded.clone(),
        lot.faults,
        lot.fault.clone(),
    );
}

/// 01.11. (R1): at the end of block `block`, a curve read of each undecided
/// lot not read yet: once the observation window began (the strategy sees
/// the blocks), with a strategy to hold it and an armed path (its Portal),
/// at most `BSC_CANONICAL_LOT_READS_MAX` at a time, a lot whose read
/// failed not before `BSC_CANONICAL_LOT_READ_RETRY_MS` (an overtaken one at
/// once: the curve moved, nothing failed).
fn ask_lot_reads(c: &mut Canonical, live: &Option<LiveRuntime>, block: u64) {
    let (Some(l), Some(_), Some((number, hash))) = (live.as_ref(), c.strategy.as_ref(), c.head)
    else {
        return;
    };
    let Some(portal) = l.portal() else {
        return;
    };
    if number != block || !c.first_journaled {
        return;
    }
    let endpoint = c.lot.config.endpoint_at(c.active).to_string();
    let strategy = c.strategy.as_ref();
    let lot = &mut c.lot;
    let now = Instant::now();
    let wanted: Vec<[u8; 20]> = lot
        .open
        .iter()
        .copied()
        .filter(|t| {
            !strategy.is_some_and(|s| s.holds(t))
                && !lot.stranded.iter().any(|(s, _)| s == t)
                && !lot.reads.iter().any(|r| r.token == *t)
                && !lot.retry_at.iter().any(|(r, at)| r == t && *at > now)
        })
        .collect();
    for token in wanted {
        if lot.reads.len() >= lot.config.lot_reads_max {
            break;
        }
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let (read, endpoint, limits) = (lot.reader, endpoint.clone(), lot.config.limits);
        let spawned = std::thread::Builder::new()
            .name("bsc-lot-curve".into())
            .spawn(move || {
                use crate::venue_probe::ProbeError;
                let result = read(
                    &endpoint,
                    &limits,
                    portal,
                    alloy_primitives::Address::from(token),
                    number,
                    hash,
                )
                .map_err(|e| LotFault {
                    again: !matches!(e, ProbeError::Abi { .. } | ProbeError::Config(_)),
                    why: e.to_string(),
                });
                let _ = tx.send(result);
            });
        match spawned {
            Ok(_handle) => lot.reads.push(LotRead {
                token,
                block,
                hash,
                rx,
                overtaken: false,
                #[cfg(test)]
                handle: Some(_handle),
            }),
            Err(e) => {
                lot.faults += 1;
                lot.fault = Some(format!("curve read not started: {e}"));
                lot.retry_later(token);
            }
        }
    }
}

/// 01.11. (R1): before a block's events, the curve reads that came back:
/// one not overtaken holds its lot again (still open, not held; journaled
/// first, so a replay steps the same engine), or strands it off the curve
/// or outside the priced scope; a failed one counts and is asked again
/// after the backoff, a refused answer strands its lot only the
/// `BSC_CANONICAL_LOT_READ_REFUSED_MAX`-th time in a row; an overtaken one
/// is dropped (asked again at the next block's end). One still out stays.
fn hold_read_lots(
    c: &mut Canonical,
    live: &mut Option<LiveRuntime>,
    journal: &mut RuntimeJournal,
    at: u64,
) {
    use darkbot_bsc_trading::economics::flap::native_curve20::PricedToken;
    let (Some(l), Some(s)) = (live.as_ref(), c.strategy.as_mut()) else {
        return;
    };
    let lot = &mut c.lot;
    let reads = std::mem::take(&mut lot.reads);
    let open = l.open_positions();
    for r in reads {
        let answer = match r.rx.try_recv() {
            Ok(answer) => answer,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                lot.reads.push(r);
                continue;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(LotFault {
                again: true,
                why: "curve read ended without an answer".into(),
            }),
        };
        let curve = match answer {
            Ok(_) if r.overtaken => {
                lot.refused_in_a_row(r.token, false);
                continue;
            }
            Ok(curve) => {
                lot.refused_in_a_row(r.token, false);
                curve
            }
            Err(fault) => {
                lot.faults += 1;
                lot.fault = Some(format!(
                    "0x{}: {}",
                    alloy_primitives::hex::encode(r.token),
                    fault.why
                ));
                let refused = lot.refused_in_a_row(r.token, !fault.again);
                if !fault.again && refused >= lot.config.lot_read_refused_max {
                    lot.stranded
                        .push((r.token, crate::sources::LIVE_POSITION_UNREADABLE));
                } else {
                    lot.retry_later(r.token);
                }
                continue;
            }
        };
        let Some((tokens, spent_wei, filled_ms)) = open.iter().find_map(|p| match *p {
            live::PositionChange::Opened {
                token,
                tokens,
                spent_wei,
                filled_ms,
            } if token == r.token => Some((tokens, spent_wei, filled_ms)),
            _ => None,
        }) else {
            continue;
        };
        if s.holds(&r.token) {
            continue;
        }
        if !curve.terms.tradable {
            lot.stranded
                .push((r.token, crate::sources::LIVE_POSITION_OFF_CURVE));
            continue;
        }
        if PricedToken::new(&curve.terms).is_err() {
            lot.stranded
                .push((r.token, crate::sources::LIVE_POSITION_UNPRICED));
            continue;
        }
        s.reheld(
            &crate::runtime_strategy::Reheld {
                token: r.token,
                creator: l.launcher_of(&r.token).unwrap_or_default(),
                terms: curve.terms,
                reserve: curve.reserve,
                circulating: curve.circulating,
                tokens,
                spent_wei,
                filled_ms,
            },
            at,
            journal,
        );
    }
}

/// One canonical event: the C2 owner first, then the follower's records
/// (and the strategy fed from them); once the block's events are all
/// applied (`BlockComplete`), the strategy's decisions go to the PAPER
/// path on that complete state (Phase I gate M2).
#[allow(clippy::too_many_arguments)]
fn apply_canonical(
    c: &mut Canonical,
    owner: &mut Option<StateOwner>,
    exec: &mut Option<ExecRuntime>,
    live: &mut Option<LiveRuntime>,
    journal: &mut RuntimeJournal,
    clock: Clock,
    at: u64,
    event: CanonicalEvent,
) {
    let complete = match event {
        CanonicalEvent::BlockComplete { number, .. } => Some(number),
        _ => None,
    };
    // 01.11. (R1): the curve reads this event overtakes; before a block's
    // events, the reads that came back hold their lots again (so they see
    // every later trade).
    c.lot.overtake(&event);
    let concerns = c.lot.concerns(&event);
    let mut changed = false;
    if let CanonicalEvent::Block { .. } = event {
        hold_read_lots(c, live, journal, at);
        // R3b: the recovered blocks and nonces resolve their transactions
        // before the new block's events (they are older and final).
        changed = apply_recoveries(c, live, journal, at);
    }
    if let Some(o) = owner.as_mut() {
        o.canonical(&event);
        // Plan 28.10.: C2's evidence about the LIVE buyer resolves the
        // LIVE lanes' intents, journaled with what it did.
        let evidence = o.take_evidence();
        if let Some(l) = live.as_mut()
            && !evidence.is_empty()
        {
            for (e, outcome) in evidence.iter().zip(l.apply_evidence(&evidence)) {
                journal.record(
                    RecordKind::Runtime,
                    at,
                    crate::runtime_journal::lane_evidence_payload(e, outcome),
                );
            }
        }
    }
    // Plan 28.10.: the buyer's receipts and reorgs become ledger facts;
    // a position opened or ended is held or released by Strategy C (a
    // journaled input, so a replay steps the same engine).
    if let Some(l) = live.as_mut() {
        let changes = l.on_canonical(&event);
        changed |= tell_strategy(c, changes, at, journal);
    }
    c.event(at, event, journal);
    // R1: who decides the open lots' exits, as the event left it (a BUY of
    // this event's block, from the ingress or the strategy, reads it).
    if concerns || changed {
        refresh_lot_deciders(c, live);
    }
    // BSC3-056: the verdict including this event (a block's header), before
    // a BUY is planned on it.
    c.refresh_confirmation(live, wall_or_expired(observed_now()));
    if let Some(block) = complete {
        burn_nonce(live, owner, clock, journal);
        strategy_to_paper(c, exec, live, owner, clock, journal, block);
        ask_lot_reads(c, live, block);
        ask_recoveries(c, live, block);
    }
}

/// A position the LIVE ledger opened or ended, to Strategy C (held or
/// released, journaled); whether there was any.
fn tell_strategy(
    c: &mut Canonical,
    changes: Vec<live::PositionChange>,
    at: u64,
    journal: &mut RuntimeJournal,
) -> bool {
    let changed = !changes.is_empty();
    for change in changes {
        let Some(s) = c.strategy.as_mut() else {
            continue;
        };
        match change {
            live::PositionChange::Opened {
                token,
                tokens,
                spent_wei,
                filled_ms,
            } => s.hold(token, tokens, spent_wei, filled_ms, at, journal),
            live::PositionChange::Ended { token } => s.unhold(token, at, journal),
        }
    }
    changed
}

/// REV.1.0 R3b (BSC3-024/041): at the end of block `block`, a receipt
/// recovery of each unresolved LIVE transaction the follower cannot
/// resolve any more ([`LiveRuntime::recovery_targets`]) not asked yet: off
/// the loop, against the followed endpoint, every block it rests on final
/// and confirmed by the second source; at most
/// `BSC_CANONICAL_RECOVERY_READS_MAX` at a time, one asked again no earlier
/// than `BSC_CANONICAL_RECOVERY_RETRY_MS` after its last answer. Without a
/// second source nothing is recovered (each named as such).
fn ask_recoveries(c: &mut Canonical, live: &mut Option<LiveRuntime>, block: u64) {
    let (Some(l), Some((number, _))) = (live.as_mut(), c.head) else {
        return;
    };
    if number != block {
        return;
    }
    let targets = l.recovery_targets(number);
    let r = &mut c.recovery;
    // Only what is still unresolved is named or waited for.
    r.pending
        .retain(|(k, _)| targets.iter().any(|(t, _)| t == k));
    r.retry_at
        .retain(|(k, _)| targets.iter().any(|(t, _)| t == k));
    let primary = c.lot.config.endpoint_at(c.active);
    let now = Instant::now();
    for (key, target) in targets {
        let (target, second) = match (target, c.confirm.as_ref()) {
            // Never asked: named why.
            (Err(why), _) => {
                r.note(key, why);
                continue;
            }
            (Ok(_), None) => {
                r.note(key, crate::sources::LIVE_RECOVERY_NO_SECOND_SOURCE);
                continue;
            }
            (Ok(target), Some((_, confirm))) => (target, confirm.endpoint.clone()),
        };
        let asked = r.reads.iter().any(|x| x.key == key)
            || r.retry_at.iter().any(|(k, at)| *k == key && *at > now);
        if asked {
            continue;
        }
        if r.reads.len() >= r.reads_max {
            r.note(key, crate::sources::LIVE_RECOVERY_QUEUED);
            continue;
        }
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let (read, primary, limits, search) = (
            r.reader,
            primary.to_string(),
            c.lot.config.limits,
            r.search_blocks,
        );
        let spawned = std::thread::Builder::new()
            .name("bsc-live-recovery".into())
            .spawn(move || {
                let _ = tx.send(read(&primary, &second, &limits, search, number, target));
            });
        match spawned {
            Ok(_handle) => {
                r.note(key, crate::sources::LIVE_RECOVERY_ASKED);
                r.reads.push(RecoveryRead {
                    key,
                    rx,
                    #[cfg(test)]
                    handle: Some(_handle),
                });
            }
            Err(e) => r.failed(key, format!("recovery read not started: {e}")),
        }
    }
    l.set_recovery(r.pending.clone(), r.faults, r.fault.clone());
}

/// REV.1.0 R3b: before a block's events, the recoveries that came back: a
/// final block found resolves its transactions as the follower's block
/// would (source RECEIPT); anything else is named and asked again after
/// the backoff. Whether a position changed.
fn apply_recoveries(
    c: &mut Canonical,
    live: &mut Option<LiveRuntime>,
    journal: &mut RuntimeJournal,
    at: u64,
) -> bool {
    use crate::chain_follower::Recovered;
    let r = &mut c.recovery;
    let Some(l) = live.as_mut().filter(|_| !r.reads.is_empty()) else {
        return false;
    };
    let reads = std::mem::take(&mut r.reads);
    let mut changes = Vec::new();
    for read in reads {
        let answer = match read.rx.try_recv() {
            Ok(answer) => answer,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                r.reads.push(read);
                continue;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("recovery read ended without an answer".into())
            }
        };
        let key = read.key;
        match answer {
            Ok(Recovered::Block(events)) => match l.recover_block(&key, &events) {
                Some(c) => {
                    changes.extend(c);
                    r.pending.retain(|(k, _)| *k != key);
                }
                // A followed block is being applied: asked again.
                None => r.note(key, crate::sources::LIVE_RECOVERY_ASKED),
            },
            Ok(Recovered::Pending(why)) => r.note(key, why.name()),
            Err(why) => r.failed(key, why),
        }
        r.retry_later(key);
    }
    l.set_recovery(r.pending.clone(), r.faults, r.fault.clone());
    tell_strategy(c, changes, at, journal)
}

/// Every queued canonical event, now (Phase I gate H1: also between the
/// ingress events of a pass, every `BSC_RUNTIME_CANONICAL_SLICE_MS`).
fn drain_canonical(
    canonical: &mut Option<Canonical>,
    crx: Option<&std::sync::mpsc::Receiver<(u64, CanonicalEvent)>>,
    owner: &mut Option<StateOwner>,
    exec: &mut Option<ExecRuntime>,
    live: &mut Option<LiveRuntime>,
    journal: &mut RuntimeJournal,
    clock: Clock,
) {
    let (Some(c), Some(crx)) = (canonical.as_mut(), crx) else {
        return;
    };
    while let Ok((at, event)) = crx.try_recv() {
        apply_canonical(c, owner, exec, live, journal, clock, at, event);
    }
}

/// The owner's LIVE gate inputs of one reconcile pass (plan 28.10.): what
/// a requested LIVE reads as now. Without a LIVE path configured nothing
/// is read or qualified, so a requested LIVE stays BLOCKED.
struct LiveView {
    boot: Option<String>,
    now_ms: u64,
    daily: Option<crate::sources::BuyUsage>,
    total: Option<crate::sources::TotalUsage>,
    spent: Option<u128>,
    qualification: crate::sources::LiveQualification,
}

impl LiveView {
    /// The reconcile's view: the owner state besides the attempt `live`'s
    /// next C3 BUY takes under the grant of `g`.
    fn of(
        g: &Generation,
        state: &Path,
        qualification: Option<&crate::sources::LiveQualification>,
        now_ms: u64,
        live: Option<&LiveRuntime>,
    ) -> Self {
        let held = live
            .and_then(|l| l.held_attempt(g.live_set_by.as_ref().map(|by| by.generation)))
            .map(alloy_primitives::hex::encode);
        Self::read(g, state, qualification, now_ms, held.as_deref())
    }

    /// The owner state as it holds now; `qualification` is the reconcile's
    /// one verified LIVE qualification (BSC3-050: verified once per
    /// reconcile, the same result the LIVE path ticks with); `held` the
    /// attempt the LIVE path's next C3 BUY takes (BSC3-076 review H1: its
    /// own slots refuse nothing, as at the LIVE path's gate).
    fn read(
        g: &Generation,
        state: &Path,
        qualification: Option<&crate::sources::LiveQualification>,
        now_ms: u64,
        held: Option<&str>,
    ) -> Self {
        let Some(qualification) = qualification else {
            return LiveView {
                boot: None,
                now_ms,
                daily: None,
                total: None,
                spent: None,
                qualification: Default::default(),
            };
        };
        let (daily, total) = crate::sources::usage_besides(state, now_ms, held);
        LiveView {
            boot: crate::sources::boot_session().ok(),
            now_ms,
            daily,
            total,
            spent: match &g.live_set_by {
                Some(by) => crate::sources::spent_by_grant(state, by.generation).ok(),
                None => Some(0),
            },
            qualification: qualification.clone(),
        }
    }

    /// What the owner makes effective of `e` now. While the LIVE path holds
    /// positions (`exits`), judged as for their exits (BSC3-058/071): the
    /// strategy that decides them, C2 and the sources stay on under a
    /// grant that admits no BUY (EXIT-ONLY, BUYs used up); the LIVE path
    /// and the owner still refuse every BUY.
    fn effective(&self, g: &Generation, e: crate::sources::Entity, exits: bool) -> Effective {
        let ctx = crate::sources::LiveContext {
            boot: self.boot.as_deref(),
            now_unix_ms: self.now_ms,
            daily: self.daily.as_ref(),
            total: self.total.as_ref(),
            spent_wei: self.spent,
            qualification: &self.qualification,
        };
        match exits {
            true => g.effective_exit_now(e, &ctx),
            false => g.effective_live_now(e, &ctx),
        }
    }
}

/// Monotonic nanoseconds since the runtime started (one epoch for all).
#[derive(Clone, Copy)]
struct Clock(Instant);

impl Clock {
    fn now_ns(self) -> u64 {
        self.ns_at(Instant::now())
    }

    /// `at` on this clock (an instant before its start reads 0).
    fn ns_at(self, at: Instant) -> u64 {
        u64::try_from(at.saturating_duration_since(self.0).as_nanos()).unwrap_or(u64::MAX)
    }
}

/// A monotonic reading and the wall clock (µs since the Unix epoch) read
/// next to it; refreshed at least once per pass, so mapping a fresh
/// monotonic time through it carries at most one pass of clock drift.
/// `None` when the wall clock could not be read (never 0).
#[derive(Clone, Copy)]
struct WallPair(Option<(u64, u64)>);

impl WallPair {
    fn read(clock: Clock) -> Self {
        let mono = clock.now_ns();
        let wall = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| u64::try_from(d.as_micros()).ok())
            .filter(|w| *w > 0);
        WallPair(wall.map(|w| (mono, w)))
    }

    /// The wall-clock estimate (µs) of the monotonic time `at_ns`.
    fn wall_us(self, at_ns: u64) -> Option<u64> {
        let (mono, wall) = self.0?;
        if at_ns >= mono {
            wall.checked_add((at_ns - mono) / 1_000)
        } else {
            wall.checked_sub((mono - at_ns) / 1_000)
        }
    }
}

/// The follower thread and its stop flag.
struct Follower {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

/// Every canonical event crosses to the runtime with its time on the
/// runtime clock (a block: when the follower first had its header). The
/// send blocks: the follower falls behind rather than losing an event.
fn spawn_follower(
    config: CanonicalConfig,
    resume: Vec<(u64, [u8; 32])>,
    tx: SyncSender<(u64, CanonicalEvent)>,
    clock: Clock,
) -> Result<Follower, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let handle = thread::Builder::new()
        .name("bsc-canonical".into())
        .spawn(move || {
            let mut sink = |event: CanonicalEvent| {
                let at = match &event {
                    CanonicalEvent::Block { observed, .. } => clock.ns_at(*observed),
                    _ => clock.now_ns(),
                };
                // Err only once the runtime dropped the receiver at stop.
                let _ = tx.send((at, event));
            };
            // The watched senders' nonce facts (the C2 owner may be switched
            // on at any time; without it they are not applied).
            let watch = NonceWatch {
                senders: config.state.watched.clone(),
                every_blocks: config.state.nonce_fact_blocks,
            };
            if let Err(e) = run_follower(
                &config.endpoints(),
                &config.limits,
                &resume,
                &watch,
                &flag,
                &mut sink,
            ) {
                eprintln!("canonical follower refused its configuration: {e:?}");
            }
        })
        .map_err(|e| format!("spawn canonical follower: {e}"))?;
    Ok(Follower { stop, handle })
}

/// Which paths the effective modes let run (plan v9 §2.3/§2.5, Phase H;
/// the §3b B cases).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Gates {
    /// Flap launches reach the C3 PAPER path and Strategy C only while the
    /// Flap venue is effective SHADOW (or LIVE).
    pub flap_admitted: bool,
    /// BSC_STRATEGY_C runs (SHADOW or LIVE) under a runtime mode of SHADOW,
    /// PAPER or LIVE; switched off, its ledger ends.
    pub strategy_on: bool,
    /// The C3 PAPER path: runtime mode PAPER and BSC_EXEC SHADOW (its own
    /// effective mode, as the owner stores it).
    pub c3_wanted: bool,
    /// The C2 chain-state owner.
    pub c2_on: bool,
}

/// [`Gates`] of the effective modes `eff` and BSC_EXEC's stored effective
/// mode `exec`.
pub(crate) fn gates(eff: impl Fn(crate::sources::Entity) -> Effective, exec: Effective) -> Gates {
    let running = |e: Effective| matches!(e, Effective::Shadow | Effective::Live);
    let flap_admitted = running(eff(VenueId::Flap.into()));
    let runtime_mode = eff(crate::sources::Entity::Runtime);
    Gates {
        flap_admitted,
        strategy_on: flap_admitted
            && matches!(
                runtime_mode,
                Effective::Shadow | Effective::Paper | Effective::Live
            )
            && running(eff(ComponentId::StrategyC.into())),
        c3_wanted: flap_admitted && runtime_mode == Effective::Paper && exec == Effective::Shadow,
        c2_on: running(eff(ComponentId::StateC2.into())),
    }
}

/// BSC3-056: the second source's header thread (its events to `tx`).
fn spawn_confirmer(
    config: &CanonicalConfig,
    confirm: &ConfirmConfig,
    tx: SyncSender<crate::chain_follower::ConfirmEvent>,
) -> Result<Follower, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let (endpoint, limits) = (confirm.endpoint.clone(), config.limits);
    let handle = thread::Builder::new()
        .name("bsc-confirm".into())
        .spawn(move || {
            let mut sink = |event| {
                let _ = tx.send(event);
            };
            if let Err(e) =
                crate::chain_follower::run_confirmer(&endpoint, &limits, &flag, &mut sink)
            {
                eprintln!("second source refused its configuration: {e:?}");
            }
        })
        .map_err(|e| format!("spawn second source: {e}"))?;
    Ok(Follower { stop, handle })
}

/// The runtime's view of the canonical chain and the launch join.
struct Canonical {
    state: &'static str,
    /// Phase E review L3: the Four.meme drift guard's last verdict
    /// (`UNPROBED`, `MATCH at block n`, `DRIFT at block n: ...`,
    /// `PROBE_FAILED: ...`); while drifted, no Four token derivation is
    /// proven (journaled as UNKNOWN).
    four_deployment: String,
    four_drift: bool,
    head: Option<(u64, [u8; 32])>,
    head_seen_ns: Option<u64>,
    /// The head block's time (header ms) and the local wall ms when the
    /// follower first held it.
    head_block_ms: Option<u64>,
    head_seen_unix_ms: Option<u64>,
    /// The observation window starts with the first block mined at or
    /// after this wall time (ms): this process's start. Earlier blocks (a
    /// C2 resume backfill, or a head mined before the sources connected)
    /// and their launches, reorgs and gaps are outside it.
    window_from_ms: u64,
    first_journaled: bool,
    /// The window's first block (its `H` record); heights below it are
    /// never counted as skipped.
    window_first_block: u64,
    backfill_blocks: u64,
    backfill_launches: u64,
    /// BSC3-014: canonical launches whose block was not the last reported
    /// (no head to join them).
    launch_head_missing: u64,
    /// Launch calls included and reverted, and the recent ones (a reorg
    /// within the follower's window takes them out again).
    reverted_launches: u64,
    /// Phase H: Flap curve trades journaled, and trade logs refused.
    flap_trades: u64,
    flap_trades_undecodable: u64,
    reverted_recent: Vec<(u64, [u8; 32], [u8; 32], crate::canonical_join::Venue)>,
    /// The deepest reorg the follower reports (`BSC_CANONICAL_REORG_DEPTH`).
    reorg_window: u64,
    /// Phase H: Strategy C, fed from these records while `BSC_STRATEGY_C`
    /// runs (SHADOW).
    strategy: Option<crate::runtime_strategy::StrategyRuntime>,
    /// The strategy's configuration while its component is on, and why it
    /// cannot run (a refused gas evidence).
    strategy_config: Option<darkbot_bsc_trading::strategy::engine::Config>,
    strategy_refused: Option<String>,
    blocks_skipped: u64,
    blocks: u64,
    reorgs: u64,
    resyncs: u64,
    rpc_errors: u64,
    rate_limited: u64,
    /// BSC3-036: the hosts of the configured endpoints (never a path),
    /// the one followed now, the failovers and the stalled steps.
    hosts: Vec<String>,
    active: usize,
    failovers: u64,
    stalls: u64,
    /// BSC3-056: the followed chain's and the second source's recent
    /// headers, the second source's host and bounds, its failed steps.
    ours: crate::canonical_confirm::Ring,
    second: crate::canonical_confirm::Ring,
    confirm: Option<(String, ConfirmConfig)>,
    confirm_faults: u64,
    /// The second source's thread's events (set when it runs).
    confirm_rx: Option<std::sync::mpsc::Receiver<crate::chain_follower::ConfirmEvent>>,
    ledger: Ledger,
    /// BSC3-062: [`CanonicalConfig::join_horizon_ms`] in µs.
    join_horizon_us: u64,
    /// 01.11. (R1): the exit deciders of the open LIVE lots.
    lot: LotDeciders,
    /// REV.1.0 R3b: the receipt recoveries of unresolved LIVE transactions.
    recovery: Recoveries,
}

/// REV.1.0 R3b: what the chain says of one unresolved LIVE transaction at
/// the followed head (production: [`read_recovery`]): the followed
/// endpoint, the second source, the follower's bounds, the search's depth,
/// the head, the target.
type ChainRecovery = fn(
    &str,
    &str,
    &FollowerLimits,
    u64,
    u64,
    crate::chain_follower::RecoveryTarget,
) -> Result<crate::chain_follower::Recovered, String>;

/// The production recovery: one connection to each endpoint, the
/// follower's bounds on every request (a runtime that stops leaves it to
/// end by them).
fn read_recovery(
    primary: &str,
    second: &str,
    limits: &FollowerLimits,
    search_blocks: u64,
    head: u64,
    target: crate::chain_follower::RecoveryTarget,
) -> Result<crate::chain_follower::Recovered, String> {
    let never = AtomicBool::new(false);
    crate::chain_follower::recover_transaction(
        primary,
        second,
        limits,
        search_blocks,
        head,
        target,
        &never,
    )
    .map_err(|e| format!("{e:?}"))
}

/// REV.1.0 R3b (BSC3-024/041): the receipt recoveries of the ledger's
/// unresolved LIVE transactions: the reads out, when each may be asked
/// again, each one's last word (why it is not resolved), the failures.
struct Recoveries {
    reads_max: usize,
    retry_ms: u64,
    search_blocks: u64,
    reader: ChainRecovery,
    reads: Vec<RecoveryRead>,
    retry_at: Vec<([u8; 32], Instant)>,
    pending: Vec<([u8; 32], &'static str)>,
    faults: u64,
    fault: Option<String>,
}

struct RecoveryRead {
    key: [u8; 32],
    rx: std::sync::mpsc::Receiver<Result<crate::chain_follower::Recovered, String>>,
    #[cfg(test)]
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Recoveries {
    fn new(config: &CanonicalConfig) -> Self {
        Recoveries {
            reads_max: config.recovery_reads_max,
            retry_ms: config.recovery_retry_ms,
            search_blocks: config.recovery_search_blocks,
            reader: read_recovery,
            reads: Vec::new(),
            retry_at: Vec::new(),
            pending: Vec::new(),
            faults: 0,
            fault: None,
        }
    }

    /// `key`'s last word.
    fn note(&mut self, key: [u8; 32], why: &'static str) {
        match self.pending.iter_mut().find(|(k, _)| *k == key) {
            Some(p) => p.1 = why,
            None => self.pending.push((key, why)),
        }
    }

    fn failed(&mut self, key: [u8; 32], why: String) {
        self.faults += 1;
        self.fault = Some(format!("0x{}: {why}", alloy_primitives::hex::encode(key)));
        self.note(key, crate::sources::LIVE_RECOVERY_FAILED);
        self.retry_later(key);
    }

    fn retry_later(&mut self, key: [u8; 32]) {
        let at = Instant::now() + Duration::from_millis(self.retry_ms);
        self.retry_at.retain(|(k, _)| *k != key);
        self.retry_at.push((key, at));
    }

    /// Tests: waits until every read out has answered.
    #[cfg(test)]
    fn settle(&mut self) {
        for r in &mut self.reads {
            if let Some(h) = r.handle.take() {
                let _ = h.join();
            }
        }
    }
}

/// 01.11. (R1): reads a token's curve at one block (production:
/// [`crate::venue_probe::read_flap_curve`]).
type CurveReader = fn(
    &str,
    &FollowerLimits,
    alloy_primitives::Address,
    alloy_primitives::Address,
    u64,
    [u8; 32],
) -> Result<crate::venue_probe::FlapCurve, crate::venue_probe::ProbeError>;

/// 01.11. (R1): who decides the open LIVE lots' exits. A lot the strategy
/// does not hold (a restart, a gap or a reorg ended its tracking, or its
/// token was never followed) is read (its curve at a block's end, off the
/// loop, pinned to the block's hash) and held again on that curve before
/// the next block's events, while nothing moved the curve since (no trade,
/// term or migration of the token, no reorg or gap). Off the curve, outside
/// the priced scope or unreadable for good it is stranded. The LIVE path
/// refuses new BUYs while a lot is undecided or stranded.
struct LotDeciders {
    config: CanonicalConfig,
    reader: CurveReader,
    reads: Vec<LotRead>,
    /// The tokens of the open lots at the last refresh.
    open: Vec<[u8; 20]>,
    stranded: Vec<([u8; 20], &'static str)>,
    /// A token read again no earlier than this (after a failed read), and
    /// its refused answers in a row.
    retry_at: Vec<([u8; 20], Instant)>,
    refused: Vec<([u8; 20], u32)>,
    faults: u64,
    fault: Option<String>,
}

/// One curve read: the lot's token, the block it reads, its answer;
/// `overtaken` once the curve moved after that block.
struct LotRead {
    token: [u8; 20],
    block: u64,
    hash: [u8; 32],
    rx: std::sync::mpsc::Receiver<Result<crate::venue_probe::FlapCurve, LotFault>>,
    overtaken: bool,
    #[cfg(test)]
    handle: Option<std::thread::JoinHandle<()>>,
}

/// A failed curve read: `again` when a later read may succeed (transport,
/// a reorg between its reads), not when the answer itself is refused.
struct LotFault {
    again: bool,
    why: String,
}

impl LotDeciders {
    fn new(config: &CanonicalConfig) -> Self {
        LotDeciders {
            config: config.clone(),
            reader: crate::venue_probe::read_flap_curve,
            reads: Vec::new(),
            open: Vec::new(),
            stranded: Vec::new(),
            retry_at: Vec::new(),
            refused: Vec::new(),
            faults: 0,
            fault: None,
        }
    }

    /// The reads `event` overtakes: a trade, term or migration of their
    /// token, or any reorg, gap or refused Flap log; a block whose parent is
    /// not the block a read was of, for that read.
    fn overtake(&mut self, event: &CanonicalEvent) {
        let token = match event {
            CanonicalEvent::FlapTrade(t) => Some(t.token),
            CanonicalEvent::FlapTerm(t) => Some(t.token),
            CanonicalEvent::FlapMigrated { token, .. } => Some(*token),
            CanonicalEvent::Reorg { .. }
            | CanonicalEvent::ReorgTooDeep { .. }
            | CanonicalEvent::Gap { .. }
            | CanonicalEvent::FlapTradeUndecodable { .. } => {
                self.reads.iter_mut().for_each(|r| r.overtaken = true);
                None
            }
            CanonicalEvent::Block {
                number,
                parent_hash,
                ..
            } => {
                for r in &mut self.reads {
                    if r.block.checked_add(1) == Some(*number) && r.hash != *parent_hash {
                        r.overtaken = true;
                    }
                }
                None
            }
            _ => None,
        };
        if let Some(token) = token {
            for r in self.reads.iter_mut().filter(|r| r.token == token) {
                r.overtaken = true;
            }
        }
    }

    /// Whether `event` can change who decides an open lot's exit.
    fn concerns(&self, event: &CanonicalEvent) -> bool {
        match event {
            CanonicalEvent::FlapTrade(t) => self.open.contains(&t.token),
            CanonicalEvent::FlapTerm(t) => self.open.contains(&t.token),
            CanonicalEvent::FlapMigrated { token, .. } => self.open.contains(token),
            CanonicalEvent::Block { .. }
            | CanonicalEvent::BlockComplete { .. }
            | CanonicalEvent::Reorg { .. }
            | CanonicalEvent::ReorgTooDeep { .. }
            | CanonicalEvent::Gap { .. }
            | CanonicalEvent::FlapTradeUndecodable { .. } => true,
            _ => false,
        }
    }

    /// Tests: waits until every read out has answered.
    #[cfg(test)]
    fn settle(&mut self) {
        for r in &mut self.reads {
            if let Some(h) = r.handle.take() {
                let _ = h.join();
            }
        }
    }

    /// How many of `token`'s last reads in a row were refused answers, this
    /// one counted (`refused`): any other outcome, an answer or a failure
    /// in transit, ends the run (review L7).
    fn refused_in_a_row(&mut self, token: [u8; 20], refused: bool) -> u32 {
        if !refused {
            self.refused.retain(|(t, _)| *t != token);
            return 0;
        }
        match self.refused.iter_mut().find(|(t, _)| *t == token) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                self.refused.push((token, 1));
                1
            }
        }
    }

    fn retry_later(&mut self, token: [u8; 20]) {
        let at = Instant::now() + Duration::from_millis(self.config.lot_read_retry_ms);
        self.retry_at.retain(|(t, _)| *t != token);
        self.retry_at.push((token, at));
    }

    /// REV.1.0 R3: the operator resumed `token`'s lot: stranded no more,
    /// its curve is read again at the next block's end (no backoff, no
    /// refusal counted from before).
    fn resume(&mut self, token: [u8; 20]) {
        self.stranded.retain(|(t, _)| *t != token);
        self.retry_at.retain(|(t, _)| *t != token);
        self.refused.retain(|(t, _)| *t != token);
    }
}

impl Canonical {
    fn new(config: &CanonicalConfig, window_from_ms: u64) -> Self {
        Canonical {
            state: "CONNECTING",
            four_deployment: "UNPROBED".into(),
            four_drift: false,
            head: None,
            head_seen_ns: None,
            head_block_ms: None,
            head_seen_unix_ms: None,
            window_from_ms,
            first_journaled: false,
            window_first_block: 0,
            backfill_blocks: 0,
            backfill_launches: 0,
            launch_head_missing: 0,
            reverted_launches: 0,
            flap_trades: 0,
            flap_trades_undecodable: 0,
            reverted_recent: Vec::new(),
            reorg_window: config.limits.reorg_depth,
            strategy: None,
            strategy_config: None,
            strategy_refused: None,
            blocks_skipped: 0,
            blocks: 0,
            reorgs: 0,
            resyncs: 0,
            rpc_errors: 0,
            rate_limited: 0,
            hosts: config
                .endpoints()
                .iter()
                .map(|e| crate::chain_follower::endpoint_host(e).unwrap_or_default())
                .collect(),
            active: 0,
            failovers: 0,
            stalls: 0,
            ours: crate::canonical_confirm::Ring::new(
                usize::try_from(config.limits.reorg_depth.saturating_add(1)).unwrap_or(usize::MAX),
            ),
            second: crate::canonical_confirm::Ring::new(
                usize::try_from(config.limits.reorg_depth.saturating_add(1)).unwrap_or(usize::MAX),
            ),
            confirm: config.confirm.as_ref().map(|c| {
                (
                    crate::chain_follower::endpoint_host(&c.endpoint).unwrap_or_default(),
                    c.clone(),
                )
            }),
            confirm_faults: 0,
            confirm_rx: None,
            ledger: Ledger::new(
                config.join_delay_ms.saturating_mul(1_000_000),
                config.limits.reorg_depth,
            ),
            join_horizon_us: config.join_horizon_ms.saturating_mul(1_000),
            lot: LotDeciders::new(config),
            recovery: Recoveries::new(config),
        }
    }

    /// Journals a canonical record and feeds the same bytes to the
    /// strategy (its replay reads them back the same way).
    fn put(&mut self, kind: RecordKind, at: u64, payload: Vec<u8>, journal: &mut RuntimeJournal) {
        match self.strategy.as_mut() {
            Some(s) => {
                // The record precedes its outputs in the journal.
                journal.record(kind, at, payload.clone());
                s.record(kind, &payload, at, Some(journal));
            }
            None => {
                journal.record(kind, at, payload);
            }
        }
    }

    fn orphan(&mut self, orphaned: &[(u64, [u8; 32])], at: u64, journal: &mut RuntimeJournal) {
        let hashes: Vec<[u8; 32]> = orphaned.iter().map(|(_, h)| *h).collect();
        for l in self.ledger.orphan(&hashes) {
            journal.record(RecordKind::Canonical, at, orphaned_launch_payload(&l));
        }
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.reverted_recent)
            .into_iter()
            .partition(|(_, block, _, _)| hashes.contains(block));
        self.reverted_recent = kept;
        for (number, block, tx, venue) in gone {
            self.reverted_launches = self.reverted_launches.saturating_sub(1);
            journal.record(
                RecordKind::Canonical,
                at,
                launch_id_payload(b'O', venue, number, &block, &tx),
            );
        }
    }

    /// BSC3-056: a header or fault of the second source.
    fn confirm_event(&mut self, event: crate::chain_follower::ConfirmEvent) {
        match event {
            crate::chain_follower::ConfirmEvent::Header {
                number,
                hash,
                parent_hash,
                block_ms,
            } => self.second.push(number, hash, parent_hash, block_ms),
            crate::chain_follower::ConfirmEvent::Fault(_) => self.confirm_faults += 1,
        }
    }

    /// BSC3-056: whether the second source confirms the followed chain at
    /// `now_ms`, with the lag it is judged under; `None` without a second
    /// source.
    fn confirmation(&self, now_ms: u64) -> Option<crate::canonical_confirm::Confirmation> {
        self.confirm
            .as_ref()
            .map(|(_, c)| crate::canonical_confirm::Confirmation {
                verdict: crate::canonical_confirm::verdict(
                    &self.ours,
                    &self.second,
                    now_ms,
                    c.max_age_ms,
                    c.max_lag_blocks,
                ),
                max_lag_blocks: c.max_lag_blocks,
            })
    }

    /// BSC3-056: the second source's queued headers, then the verdict at
    /// `now_ms` handed to the LIVE path: before a block's BUY is planned
    /// (each canonical event) and every pass.
    fn refresh_confirmation(&mut self, live: &mut Option<LiveRuntime>, now_ms: u64) {
        while let Some(event) = self.confirm_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.confirm_event(event);
        }
        if let Some(l) = live.as_mut() {
            l.set_confirmation(self.confirmation(now_ms));
        }
    }

    fn event(&mut self, at: u64, event: CanonicalEvent, journal: &mut RuntimeJournal) {
        // BSC3-056: the followed chain's recent headers, before the window
        // too (a LIVE BUY compares at the head).
        match &event {
            CanonicalEvent::Block {
                number,
                hash,
                parent_hash,
                timestamp_ms,
                ..
            } => self.ours.push(*number, *hash, *parent_hash, *timestamp_ms),
            CanonicalEvent::Reorg { orphaned, .. }
            | CanonicalEvent::ReorgTooDeep { orphaned, .. } => self.ours.orphan(orphaned),
            _ => {}
        }
        match event {
            CanonicalEvent::Block {
                number,
                hash,
                timestamp_ms,
                observed_unix_ms,
                ..
            } => {
                self.state = "FOLLOWING";
                self.head = Some((number, hash));
                self.head_seen_ns = Some(at);
                self.head_block_ms = Some(timestamp_ms);
                self.head_seen_unix_ms = observed_unix_ms;
                if !self.first_journaled {
                    if timestamp_ms < self.window_from_ms {
                        self.backfill_blocks += 1;
                        // REV.1.0 R2 (review P2): the catch-up before the
                        // window, a runtime record no G0.4 count reads: the
                        // LIVE campaign's chain while no run was up.
                        journal.record(
                            RecordKind::Runtime,
                            at,
                            crate::runtime_journal::catchup_block_payload(
                                number,
                                &hash,
                                timestamp_ms,
                            ),
                        );
                        return;
                    }
                    self.first_journaled = true;
                    self.window_first_block = number;
                    journal.record(
                        RecordKind::Canonical,
                        at,
                        window_payload(b'H', number, &hash),
                    );
                }
                self.blocks += 1;
                self.put(
                    RecordKind::Canonical,
                    at,
                    crate::runtime_strategy::block_payload(number, &hash, timestamp_ms),
                    journal,
                );
            }
            // Before the window: not part of the G0.4 counts (the C2 owner
            // has applied them already).
            CanonicalEvent::Launch(l) if !self.first_journaled => {
                self.backfill_launches += 1;
                // REV.1.0 R2 (review P2): a launch while no run was up, for
                // the LIVE campaign's skips (no sighting could see it).
                if let Some(block_ms) = self
                    .head_block_ms
                    .filter(|_| self.head.is_some_and(|(_, h)| h == l.block_hash))
                {
                    journal.record(
                        RecordKind::Runtime,
                        at,
                        crate::runtime_journal::catchup_launch_payload(&l, block_ms),
                    );
                }
            }
            // REV.1.0 R2 (review Q1): the catch-up's Portal trades and
            // refused trade logs too (runtime records), so a market window
            // reaching into it is complete only with them; terms and
            // migrations before the window stay unjournaled.
            CanonicalEvent::FlapTrade(t) if !self.first_journaled => {
                journal.record(
                    RecordKind::Runtime,
                    at,
                    crate::runtime_journal::catchup_trade_payload(&t),
                );
            }
            CanonicalEvent::FlapTradeUndecodable {
                block_number,
                block_hash,
                tx_index,
                log_index,
            } if !self.first_journaled => {
                journal.record(
                    RecordKind::Runtime,
                    at,
                    crate::runtime_journal::catchup_trade_gap_payload(
                        block_number,
                        &block_hash,
                        tx_index,
                        log_index,
                    ),
                );
            }
            CanonicalEvent::FlapTerm(_) | CanonicalEvent::FlapMigrated { .. }
                if !self.first_journaled => {}
            CanonicalEvent::FlapTerm(t) => {
                self.put(RecordKind::Canonical, at, flap_term_payload(&t), journal);
            }
            CanonicalEvent::FlapMigrated {
                block_number,
                block_hash,
                tx_index,
                log_index,
                token,
            } => {
                self.put(
                    RecordKind::Canonical,
                    at,
                    flap_migrated_payload(block_number, &block_hash, tx_index, log_index, &token),
                    journal,
                );
            }
            // Phase H: every Flap curve trade of the window, for the
            // strategy and its replay.
            CanonicalEvent::FlapTrade(t) => {
                self.flap_trades += 1;
                self.put(RecordKind::Canonical, at, flap_trade_payload(&t), journal);
            }
            CanonicalEvent::FlapTradeUndecodable {
                block_number,
                block_hash,
                tx_index,
                log_index,
            } => {
                self.flap_trades_undecodable += 1;
                self.put(
                    RecordKind::Canonical,
                    at,
                    trade_gap_payload(block_number, &block_hash, tx_index, log_index),
                    journal,
                );
            }
            CanonicalEvent::RevertedLaunch { .. } if !self.first_journaled => {}
            CanonicalEvent::RevertedLaunch {
                venue,
                block_number,
                block_hash,
                tx_hash,
                ..
            } => {
                self.reverted_launches += 1;
                self.reverted_recent
                    .push((block_number, block_hash, tx_hash, venue));
                // Older than any reorg the follower reports: final.
                let floor = block_number.saturating_sub(self.reorg_window);
                self.reverted_recent.retain(|(n, ..)| *n >= floor);
                journal.record(
                    RecordKind::Canonical,
                    at,
                    launch_id_payload(b'V', venue, block_number, &block_hash, &tx_hash),
                );
            }
            CanonicalEvent::Reorg { .. }
            | CanonicalEvent::ReorgTooDeep { .. }
            | CanonicalEvent::Gap { .. }
                if !self.first_journaled => {}
            CanonicalEvent::Launch(l) => {
                // Its block is the last one reported (launches follow it).
                let same = self.head.is_some_and(|(_, h)| h == l.block_hash);
                // BSC3-014: without that head nothing joins it: counted and
                // journaled as received, so the census shows it unjoined.
                let Some(block_ms) = self.head_block_ms.filter(|_| same) else {
                    self.launch_head_missing += 1;
                    journal.record(
                        RecordKind::Canonical,
                        at,
                        crate::runtime_journal::received_launch_payload(
                            l.venue,
                            l.block_number,
                            &l.block_hash,
                            &l.tx_hash,
                        ),
                    );
                    return;
                };
                let launch = LaunchRef {
                    venue: l.venue,
                    block_number: l.block_number,
                    block_hash: l.block_hash,
                    tx_index: l.tx_index,
                    tx_hash: l.tx_hash,
                    token: l.token,
                    external_buys: l.external_buys_after,
                    c1_scope: l.c1_scope,
                    venue_target: l.venue_target,
                    // Drifted: the frozen derivation proves nothing.
                    four_token: match l.four_token {
                        crate::chain_follower::FourToken::Matched
                        | crate::chain_follower::FourToken::Mismatch
                        | crate::chain_follower::FourToken::Candidate
                            if self.four_drift =>
                        {
                            crate::chain_follower::FourToken::Unknown
                        }
                        t => t,
                    },
                    tx: match l.unverifiable {
                        None => LaunchTx::Verified,
                        Some(r) if is_unrebuildable(r) => LaunchTx::Unrebuildable,
                        Some(_) => LaunchTx::AnswerRefused,
                    },
                    block_ms,
                    block_seen_ns: self.head_seen_ns.filter(|_| same).unwrap_or(at),
                    block_seen_unix_ms: self.head_seen_unix_ms.filter(|_| same),
                };
                if let (crate::canonical_join::Venue::Flap, Some(token), Some(creator)) =
                    (l.venue, l.token, l.creator)
                {
                    self.put(
                        RecordKind::Canonical,
                        at,
                        crate::runtime_strategy::flap_launch_payload(
                            l.block_number,
                            l.tx_index,
                            &token,
                            &creator,
                        ),
                        journal,
                    );
                }
                journal.record(RecordKind::Canonical, at, pending_launch_payload(&launch));
                self.ledger.push(launch);
            }
            CanonicalEvent::Reorg { orphaned, new_head } => {
                self.reorgs += 1;
                let depth = u32::try_from(orphaned.len()).unwrap_or(u32::MAX);
                self.put(
                    RecordKind::Reorg,
                    at,
                    reorg_payload(depth, new_head.0, &new_head.1),
                    journal,
                );
                self.orphan(&orphaned, at, journal);
            }
            CanonicalEvent::ReorgTooDeep { orphaned, new_head } => {
                // The new chain below its head is not followed: those heights
                // are skipped (the orphaned ones are counted as orphaned).
                self.resyncs += 1;
                let from = orphaned
                    .first()
                    .map_or(new_head.0, |(n, _)| *n)
                    .max(self.window_first_block);
                // Whatever was skipped, no curve the strategy holds is
                // exact: the resync tells it, or (nothing skipped) `I`.
                if new_head.0 > from {
                    self.blocks_skipped += new_head.0 - from;
                    self.put(
                        RecordKind::Reorg,
                        at,
                        resync_payload(from, new_head.0 - 1),
                        journal,
                    );
                } else {
                    self.put(
                        RecordKind::Canonical,
                        at,
                        crate::runtime_strategy::incomplete_payload(new_head.0),
                        journal,
                    );
                }
                self.orphan(&orphaned, at, journal);
            }
            CanonicalEvent::Gap { from, to } => {
                self.resyncs += 1;
                // Heights below the window were never part of it.
                let from = from.max(self.window_first_block);
                if to >= from {
                    self.blocks_skipped += to - from + 1;
                    self.put(RecordKind::Reorg, at, resync_payload(from, to), journal);
                }
            }
            // The C2 owner's reconciliation input; no G0.4 count.
            CanonicalEvent::NonceFact { .. } => {}
            // Phase I: the runtime acts on the complete block (strategy).
            CanonicalEvent::BlockComplete { .. } => {}
            CanonicalEvent::Failover { to, .. } => {
                self.active = to;
                self.failovers += 1;
            }
            CanonicalEvent::Fault(FollowerFault::Stalled { .. }) => {
                self.stalls += 1;
                self.state = "STALLED";
            }
            CanonicalEvent::Fault(fault) => {
                self.rpc_errors += 1;
                let limited = matches!(
                    fault,
                    FollowerFault::Request {
                        error: RequestError::RateLimited(_),
                        ..
                    }
                );
                self.rate_limited += u64::from(limited);
                self.state = if limited { "RATE_LIMITED" } else { "FAILING" };
            }
        }
    }

    /// Joins the launches due at `now` (all of them with `u64::MAX`).
    fn join(&mut self, now: u64, core: &IngressCore, journal: &mut RuntimeJournal) {
        let head = self.head.map_or(0, |(n, _)| n);
        let horizon_us = self.join_horizon_us;
        let forgotten = |h: &[u8; 32], block_ms: u64| {
            core.maybe_forgotten(h, block_ms.saturating_mul(1_000).saturating_sub(horizon_us))
        };
        for j in self
            .ledger
            .join_due(now, head, |h| core.sighting(h), forgotten)
        {
            journal.record(
                RecordKind::Canonical,
                j.launch.block_seen_ns,
                canonical_launch_payload(&j),
            );
        }
    }

    fn draft(&self, now: u64) -> Draft {
        use rs::canonical as e;
        let c = self.ledger.counters();
        let mut d = Draft::new(&e::SCHEMA);
        d.set(e::STATE, self.state)
            .set(e::BLOCKS, self.blocks)
            .set(e::REORGS, self.reorgs)
            .set(e::RESYNCS, self.resyncs)
            .set(e::RPC_ERRORS, self.rpc_errors)
            .set(e::RATE_LIMITED, self.rate_limited)
            .set(
                e::ENDPOINT,
                self.hosts.get(self.active).cloned().unwrap_or_default(),
            )
            .set(e::FAILOVERS, self.failovers)
            .set(e::STALLS, self.stalls)
            .set_opt(
                e::CONFIRM_ENDPOINT,
                self.confirm.as_ref().map(|(host, _)| host.clone()),
            )
            .set_opt(
                e::CONFIRM_HEAD,
                self.confirm.as_ref().and(self.second.head()),
            )
            .set_opt(
                e::CONFIRM_STATE,
                self.confirmation(wall_or_expired(observed_now()))
                    .map(|c| c.verdict.name()),
            )
            .set(e::CONFIRM_FAULTS, self.confirm_faults)
            .set(e::FLAP_LAUNCHES, c.flap_launches)
            .set(e::FOUR_LAUNCHES, c.four_launches)
            .set(e::FOUR_PENDING_DECODED, c.four_pending_decoded)
            .set(e::FOUR_PENDING_UNKNOWN, c.four_pending_unknown)
            .set(e::FOUR_PENDING_MISMATCH, c.four_pending_mismatch)
            .set(e::FOUR_PENDING_CANDIDATE, c.four_pending_candidate)
            .set(e::FOUR_SEEN_FULL_BEFORE, c.four_seen_full_before)
            .set(e::FOUR_SEEN_HASH_ONLY_BEFORE, c.four_seen_hash_only_before)
            .set(e::FOUR_SEEN_AFTER, c.four_seen_after)
            .set(e::FOUR_MISSED, c.four_missed)
            .set(e::FOUR_EVICTED, c.four_evicted)
            .set(e::FOUR_CLOCK_UNKNOWN, c.four_clock_unknown)
            .set(e::LAUNCH_HEAD_MISSING, self.launch_head_missing)
            .set(e::FOUR_ACTIONABLE, c.four_actionable)
            .set(e::REVERTED_LAUNCHES, self.reverted_launches)
            .set(e::ORPHANED_LAUNCHES, c.orphaned_launches)
            .set(e::SEEN_FULL_BEFORE, c.seen_full_before)
            .set(e::SEEN_HASH_ONLY_BEFORE, c.seen_hash_only_before)
            .set(e::SEEN_AFTER, c.seen_after)
            .set(e::MISSED, c.missed)
            .set(e::EVICTED, c.evicted)
            .set(e::EXTERNAL_BUYS, c.external_buys)
            .set(e::BUYS_UNKNOWN, c.buys_unknown)
            .set(e::CLOCK_UNKNOWN, c.clock_unknown)
            .set(e::TX_UNVERIFIABLE, c.tx_unverifiable)
            .set(e::TX_ANSWER_REFUSED, c.tx_answer_refused)
            .set(e::C1_SCOPE_LAUNCHES, c.c1_scope_launches)
            .set(e::C1_SCOPE_FULL_BEFORE, c.c1_scope_full_before)
            .set(e::BLOCKS_SKIPPED, self.blocks_skipped)
            .set(e::WAITING, self.ledger.waiting() as u64)
            .set(e::BACKFILL_BLOCKS, self.backfill_blocks)
            .set(e::BACKFILL_LAUNCHES, self.backfill_launches)
            .set(e::FOUR_DEPLOYMENT, self.four_deployment.as_str());
        if let Some((n, _)) = self.head {
            d.set(e::HEAD, n);
        }
        let lag = self
            .head_seen_unix_ms
            .zip(self.head_block_ms)
            .and_then(|(seen, block)| {
                i64::try_from(seen)
                    .ok()?
                    .checked_sub(i64::try_from(block).ok()?)
            });
        if let Some(lag) = lag {
            d.set(e::HEAD_FETCH_LAG_MS, lag);
        }
        if let Some(at) = self.head_seen_ns {
            d.set(e::HEAD_AGE_MS, now.saturating_sub(at) / 1_000_000);
        }
        d
    }
}

fn spawn_adapter(
    source: SourceId,
    url: String,
    limits: WssLimits,
    tx: SyncSender<(SourceId, u64, Event)>,
    dropped: Arc<AtomicU64>,
    clock: Clock,
) -> Result<Adapter, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let handle = thread::Builder::new()
        .name(format!("bsc-src-{}", source.name()))
        .spawn(move || {
            let wss = WssSource {
                name: source.name(),
                url,
            };
            // T_ingress of a pending transaction is the receipt of its frame,
            // before the JSON parse and the signer recovery.
            let mut sink = |event: WssEvent, received: Instant| {
                let event = match event {
                    WssEvent::Connected => Event::Connected,
                    WssEvent::Full(tx) => Event::Full {
                        hash: tx.hash,
                        raw: tx.raw,
                        from: tx.from,
                        nonce: tx.nonce,
                    },
                    WssEvent::HashOnly(hash) => Event::HashOnly { hash },
                    WssEvent::Rejected(_) => Event::Rejected,
                    WssEvent::Disconnected(reason) => Event::Disconnected {
                        rate_limited: matches!(reason, DisconnectReason::RateLimited(_)),
                    },
                };
                if let Err(TrySendError::Full(_)) =
                    tx.try_send((source, clock.ns_at(received), event))
                {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            };
            if let Err(e) = run_source(&wss, &limits, &flag, &mut sink) {
                // Refused before any connect: visible in the log and as a disconnect.
                eprintln!("{}: source refused its configuration: {e:?}", source.name());
                sink(
                    WssEvent::Disconnected(DisconnectReason::Stopped),
                    Instant::now(),
                );
            }
        })
        .map_err(|e| format!("spawn {} adapter: {e}", source.name()))?;
    Ok(Adapter { stop, handle })
}

/// One pass of the C2 checkpoint (BSC3-039): the writes the writer
/// finished are counted; when one is `due` the window is handed to the
/// writer, never written on this loop, except at the stop: then here,
/// after the writer's writes and never beside one, durable before the
/// process ends.
fn checkpoint_pass(
    o: &mut StateOwner,
    writer: &CheckpointWriter,
    due: bool,
    stopping: bool,
    replace: impl Fn(&Path, &[u8]) -> Result<(), String>,
) {
    for written in writer.finished() {
        o.checkpoint_written(written);
    }
    match (due, stopping) {
        (true, true) => {
            o.checkpoint(|p, b| writer.write_now(p, b, &replace));
            for written in writer.finished() {
                o.checkpoint_written(written);
            }
        }
        (true, false) => writer.submit(o.checkpoint_bytes()),
        (false, _) => {}
    }
}

/// Writes `bytes` to `path` atomically (temp file in the same directory).
fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    replace_with(path, bytes, true)
}

/// [`replace`] without the flush to the device (BSC3-039): for the runtime
/// status, a display record rewritten every status interval on the main
/// loop. Still published by an atomic rename (a reader never sees a part
/// of it); after a host crash it may be the previous one or unreadable
/// until the next interval rewrites it. A full flush costs milliseconds
/// here (F_FULLFSYNC on macOS), each interval, while launches wait.
fn replace_volatile(path: &Path, bytes: &[u8]) -> Result<(), String> {
    replace_with(path, bytes, false)
}

#[cfg(test)]
thread_local! {
    /// Tests: the replacements this thread flushed to the device.
    static FLUSHED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn replace_with(path: &Path, bytes: &[u8], flush: bool) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    let _ = fs::remove_file(&tmp);
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| {
            std::io::Write::write_all(&mut f, bytes)?;
            if flush {
                #[cfg(test)]
                FLUSHED.with(|n| n.set(n.get() + 1));
                f.sync_all()?;
            }
            Ok(())
        })
        .and_then(|()| fs::rename(&tmp, path))
        .and_then(|()| {
            // BSC3-039 review: the rename itself durable (a stop followed
            // by a power loss kept the previous file): the directory
            // handed to the drive and the medium flushed.
            if flush {
                let dir = fs::File::open(path.parent().unwrap_or(Path::new(".")))?;
                darkbot_bsc_journal::durable::handed(&dir)?;
                darkbot_bsc_journal::durable::flush(&dir)?;
            }
            Ok(())
        });
    written.map_err(|e| format!("{}: {e}", path.display()))
}

/// The payload bound of the runtime journal; the census reads with the same.
pub fn journal_max_payload(config: &RuntimeConfig) -> usize {
    config.wss.max_message_bytes + 64
}

/// Journals each source's dropped-event total that grew since the last
/// `D` record and tells the journal the sum (Phase F gate H1: a loss before
/// the journal is a loss of the capture).
fn journal_drops(
    journal: &mut RuntimeJournal,
    dropped: &[Arc<AtomicU64>],
    journaled: &mut [u64; 5],
    at: u64,
) {
    for (i, d) in dropped.iter().enumerate().take(journaled.len()) {
        let total = d.load(Ordering::Relaxed);
        if total > journaled[i] {
            journal.record(
                RecordKind::Runtime,
                at,
                ingress_drop_payload(SourceId::ALL[i], total),
            );
            journaled[i] = total;
        }
    }
    journal.set_ingress_dropped(journaled.iter().sum());
}

/// The checkout's commit and whether its engine sources differ from it
/// (`git -C <repo> rev-parse HEAD`, `git status --porcelain -- bsc/engine`);
/// `None` where there is no checkout (Phase F gate M5: with the binary's own
/// SHA-256 in the manifest).
pub fn checkout_identity(repo_root: &Path) -> (Option<String>, Option<bool>) {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let head = git(&["rev-parse", "HEAD"]).filter(|h| h.len() == 40);
    let dirty = head
        .as_ref()
        .and_then(|_| git(&["status", "--porcelain", "--", "bsc/engine"]))
        .map(|s| !s.is_empty());
    (head, dirty)
}

/// The host a run's latencies are of: OS, architecture, logical CPUs.
pub fn host_label() -> String {
    format!(
        "{} {} cpus={}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map_or(0, |n| n.get())
    )
}

/// The bounds of a capacity preflight of the runtime's capture.
pub fn preflight_setup(config: &RuntimeConfig) -> crate::capture_preflight::PreflightSetup {
    crate::capture_preflight::PreflightSetup {
        duration_s: config.capture_duration_s,
        probe_bytes: config.preflight_probe_bytes,
        min_rates_s: config.preflight_min_rates_s,
        disk_reserve_bytes: config.disk_reserve_bytes,
        memory_budget_bytes: config.journal_budget_bytes as u64,
        queue_records: config.journal_queue as u64,
        max_payload: journal_max_payload(config),
        sync: journal_setup(config).sync,
    }
}

/// The bounds of the runtime's segmented capture.
pub fn journal_setup(config: &RuntimeConfig) -> JournalSetup {
    JournalSetup {
        queue: config.journal_queue,
        budget_bytes: config.journal_budget_bytes,
        max_payload: journal_max_payload(config),
        segment: SegmentPolicy {
            max_segment_bytes: config.journal_segment_bytes,
            max_segment_ns: config.journal_segment_ms.saturating_mul(1_000_000),
        },
        sync: SyncPolicy {
            every_records: config.journal_sync_records,
            every: Duration::from_millis(config.journal_sync_ms),
        },
    }
}

/// Replays the journals of `rundir` into `bsc-runtime/g04-census.dbrec`;
/// `active` is the journal the calling runtime still writes (`RUNNING`).
pub fn write_census(
    config: &RuntimeConfig,
    rundir: &Path,
    active: Option<&Path>,
) -> Result<Census, String> {
    let dir = rundir.join("bsc-runtime");
    let census = census(&dir.join("journal"), journal_max_payload(config), active)?;
    replace(
        &dir.join("g04-census.dbrec"),
        &census.encode(observed_now()?)?,
    )?;
    Ok(census)
}

/// Wall-clock reads that failed in this process (BSC3-015): each one
/// counted as expired time (`u64::MAX`: every grant expired, every head too
/// old), shown in the status, never silent.
static CLOCK_UNREADABLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The wall clock now (ms), or expired time when it cannot be read
/// (counted, [`CLOCK_UNREADABLE`]).
pub(crate) fn wall_or_expired(read: Result<u64, String>) -> u64 {
    read.unwrap_or_else(|_| {
        count_unreadable_clock();
        u64::MAX
    })
}

/// Counts one failed wall-clock read ([`CLOCK_UNREADABLE`]).
pub(crate) fn count_unreadable_clock() {
    CLOCK_UNREADABLE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// The failed wall-clock reads of this process so far.
pub(crate) fn unreadable_clock_reads() -> u64 {
    CLOCK_UNREADABLE.load(std::sync::atomic::Ordering::Relaxed)
}

/// The last G0.4 census refresh of this run (BSC3-016): `OK` or why it
/// failed, and when the last one was written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CensusRefresh {
    state: Option<String>,
    written_unix_ms: Option<u64>,
}

impl CensusRefresh {
    /// The census thread's end: its result, or that it panicked.
    fn ended(&mut self, joined: std::thread::Result<Result<u64, String>>) {
        match joined {
            Ok(Ok(ms)) => {
                self.state = Some("OK".into());
                self.written_unix_ms = Some(ms);
            }
            Ok(Err(e)) => self.state = Some(format!("FAILED: {e}")),
            Err(_) => self.state = Some("FAILED: the census thread panicked".into()),
        }
    }
}

/// The status record of this moment.
#[allow(clippy::too_many_arguments)]
fn status_bytes(
    generation: &Generation,
    stats: &[(SourceId, SourceStats, Health)],
    clock: Clock,
    started_ms: u64,
    observed_ms: u64,
    launches: u64,
    journal: &str,
    journal_records: u64,
    journal_lost: u64,
    capture: Option<&CaptureStatus>,
    canonical: Option<&Canonical>,
    owner: Option<&StateOwner>,
    exec: Option<&ExecRuntime>,
    live: Option<&LiveRuntime>,
    status_interval_ms: u64,
    census: &CensusRefresh,
) -> Result<Vec<u8>, String> {
    let now = clock.now_ns();
    let mut d = Draft::new(&rs::SCHEMA);
    d.set(rs::GENERATION, generation.generation)
        .set(rs::PROVENANCE_TOOL, "bsc_runtime")
        .set(rs::PROVENANCE_OBSERVED_UNIX_MS, observed_ms)
        .set(rs::STARTED_UNIX_MS, started_ms)
        .set(rs::LAUNCHES, launches)
        .set(rs::JOURNAL_PATH, journal)
        .set(rs::JOURNAL_RECORDS, journal_records)
        .set(rs::JOURNAL_LOST, journal_lost)
        .set(
            rs::MODE,
            // Plan 28.10.: LIVE while the LIVE path is armed.
            if live.is_some_and(LiveRuntime::armed) {
                "LIVE"
            } else {
                "READ_ONLY"
            },
        )
        .set(rs::STATUS_INTERVAL_MS, status_interval_ms);
    if let Some(state) = &census.state {
        d.set(rs::CENSUS_STATE, state.as_str());
    }
    d.set(rs::CLOCK_UNREADABLE, unreadable_clock_reads());
    if let Some(ms) = census.written_unix_ms {
        d.set(rs::CENSUS_WRITTEN_UNIX_MS, ms);
    }
    if let Some(c) = capture {
        use rs::capture as k;
        let w = &c.writer;
        let mut e = Draft::new(&k::SCHEMA);
        e.set(k::RECORDS, w.records)
            .set(k::BYTES, w.bytes)
            .set(k::LOST_RECORDS, w.lost_records)
            .set(k::SEGMENTS_SEALED, w.segments_sealed)
            .set(k::SYNCS, w.syncs)
            .set(k::BUDGET_MAX_BYTES, c.budget_max)
            .set(k::BUDGET_IN_USE_BYTES, c.budget_in_use)
            .set(k::BUDGET_PEAK_BYTES, c.budget_peak)
            .set(k::BUDGET_REFUSED, c.budget_refused)
            .set(k::WRITE_ERRORS, w.write_errors)
            .set(k::STATE, if w.failed { "FAILED" } else { "HEALTHY" });
        if let Some(v) = w.durable_sequence {
            e.set(k::DURABLE_SEQUENCE, v);
        }
        d.set(rs::CAPTURE, e.into_nested());
    }
    for (source, s, h) in stats {
        use rs::source as e;
        let mut row = Draft::new(&e::SCHEMA);
        row.set(e::SOURCE_ID, source.name())
            .set(e::EFFECTIVE, generation.source(*source).effective.name())
            .set(e::HEALTH, h.name())
            .set(e::CONNECTS, s.connects)
            .set(e::DISCONNECTS, s.disconnects)
            .set(e::RATE_LIMITED, s.rate_limited)
            .set(e::FULL, s.full)
            .set(e::HASH_ONLY, s.hash_only)
            .set(e::REJECTED, s.rejected)
            .set(e::DROPPED, s.dropped)
            .set(e::DUPLICATES, s.duplicates)
            .set(e::FIRST_ANY, s.first_any)
            .set(e::FIRST_FULL, s.first_full)
            .set(e::LAUNCH_SIGHTINGS, s.launch_sightings)
            .set(e::LAUNCH_FIRST, s.launch_first);
        if let Some(at) = s.last_event_ns {
            row.set(e::LAST_EVENT_AGE_MS, now.saturating_sub(at) / 1_000_000);
        }
        d.push(rs::SOURCES, row.into_nested());
    }
    if let Some(c) = canonical {
        d.set(rs::CANONICAL, c.draft(now).into_nested());
    }
    if let Some(o) = owner {
        d.set(rs::STATE, o.draft().into_nested());
    }
    if let Some(x) = exec {
        d.set(rs::EXEC, x.draft().into_nested());
    }
    if let Some(l) = live {
        d.set(rs::LIVE, l.draft().into_nested());
    }
    if let Some((c, cfg)) = canonical.and_then(|c| c.strategy_config.as_ref().map(|k| (c, k))) {
        d.set(
            rs::STRATEGY,
            crate::runtime_strategy::status_draft(
                c.strategy.as_ref(),
                c.strategy_refused.as_deref(),
                cfg,
            )
            .into_nested(),
        );
    }
    d.encode().map_err(|e| e.to_string())
}

/// The stop request of the runtime in `rundir` (written by `darkbotctl` or
/// by the process's own SIGTERM/SIGINT handler).
pub fn stop_file(rundir: &Path) -> PathBuf {
    rundir.join("bsc.runtime.stop")
}

/// The sighting of a Flap launch that may trigger the armed LIVE path: the
/// first verified full one (`true`), or a later one from another source
/// (`false`, BSC3-075: it triggers when the first came from a source the
/// grant does not name).
fn live_sighting(decided: Option<Launch>) -> Option<(SourceId, [u8; 32], bool)> {
    match decided {
        Some(Launch::First {
            hash,
            source,
            venue: Venue::Flap,
            ..
        }) => Some((source, hash, true)),
        Some(Launch::Later {
            hash,
            source,
            venue: Venue::Flap,
            ..
        }) => Some((source, hash, false)),
        _ => None,
    }
}

/// Runs until `rundir/bsc.runtime.stop` exists.
pub fn run(
    config: &RuntimeConfig,
    sources_dir: &Path,
    rundir: &Path,
    repo_root: &Path,
) -> Result<(), String> {
    let store = Store::open(sources_dir, crate::sources::KEEP_GENERATIONS)?;
    let clock = Clock(Instant::now());
    let started_ms = observed_now()?;
    let dir = rundir.join("bsc-runtime");
    fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    let stop_file = stop_file(rundir);
    let status_path = dir.join("status.dbrec");
    let mut journal = RuntimeJournal::open(
        &dir.join("journal"),
        started_ms,
        u64::from(CLOCK_ID_MONOTONIC),
        journal_setup(config),
    )?;
    // The checkout at start may have moved since the build (a shared
    // checkout): the label says so; BUILD_SHA256 names the binary itself.
    let (checkout, dirty) = checkout_identity(repo_root);
    journal.set_identity(RunIdentity {
        host: host_label(),
        build: format!(
            "darkbot-bsc-execution {} checkout-at-start {}{}",
            env!("CARGO_PKG_VERSION"),
            checkout.as_deref().unwrap_or("unknown"),
            if dirty == Some(true) { "+dirty" } else { "" }
        ),
        config_sha256: config.source_sha256,
        build_sha256: own_binary_sha256(),
        build_dirty: dirty,
        capture_pending: config.capture_pending.name(),
    });
    // BSC3-033: the G0.4 pre-commitment on file at this start, first in
    // the journal (a crashed run keeps it); an unreadable one binds nothing.
    if let Ok(Some(p)) = crate::g04_criteria::Precommit::read(rundir) {
        journal.record(
            RecordKind::Runtime,
            clock.now_ns(),
            crate::runtime_journal::g04_precommit_payload(&p.sha256),
        );
    }
    let journal_rel: PathBuf = journal
        .path()
        .strip_prefix(rundir)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| journal.path().to_path_buf());
    let limits = config.wss;
    let (tx, rx): (SyncSender<(SourceId, u64, Event)>, Receiver<_>) =
        sync_channel(config.queue_events);
    let dropped: Vec<Arc<AtomicU64>> = SourceId::ALL
        .iter()
        .map(|_| Arc::new(AtomicU64::new(0)))
        .collect();
    let mut core = IngressCore::new(config.dedup_hashes, config.max_launch_calldata);
    // The dropped-event totals last journaled, per source (`D` records).
    let mut journaled_drops = [0u64; 5];
    // The periodic census in flight, if any.
    let mut census_job: Option<std::thread::JoinHandle<Result<u64, String>>> = None;
    let mut census_refresh = CensusRefresh::default();
    // Phase E review L3: the Four.meme drift guard in flight, and its next
    // run (at start, then every BSC_CANONICAL_FOUR_PROBE_MS).
    type FourProbe = Result<crate::venue_probe::FourObservation, crate::venue_probe::ProbeError>;
    let mut four_job: Option<std::thread::JoinHandle<FourProbe>> = None;
    let mut next_four = Instant::now();
    let mut adapters: HashMap<usize, Adapter> = HashMap::new();
    let mut rate_limited_last = [false; 5];
    // The owner's start (and launchd's crash restart): every wallet and
    // economic switch DISABLED before anything runs (plan 27.10.).
    let mut generation = store.restart(started_ms)?;
    // Strategy C's frozen gas evidence; refused, the strategy cannot run
    // (its status says why).
    let strategy_gas = config.strategy.gas(repo_root);
    // BSC3-079: no configured gas limit below the largest gas the frozen
    // evidence holds for its class (a transaction would run out of gas).
    exec_gas_covered(config)?;
    let (canonical_tx, canonical_rx) = sync_channel::<(u64, CanonicalEvent)>(config.queue_events);
    let canonical_rx = Some(canonical_rx);
    // The C3 PAPER path: its identity is loaded at start, and C2 watches its
    // sender (the follower reads that account's nonce facts).
    let mut exec = config
        .exec
        .clone()
        .map(|setup| ExecRuntime::new(setup, repo_root, dir.join("paper")));
    // Plan 28.10.: the LIVE path; its buyer is a watched C2 sender (config).
    let mut live = config
        .exec
        .clone()
        .zip(config.live.clone())
        .map(|(x, setup)| {
            LiveRuntime::new(
                setup,
                x,
                repo_root,
                sources_dir.to_path_buf(),
                dir.join("live"),
                exec.as_ref().and_then(ExecRuntime::sender),
            )
            .with_ingress_hashes(config.dedup_hashes)
        });
    let mut canonical_config = config.canonical.clone();
    if let (Some(c), Some(sender)) = (
        canonical_config.as_mut(),
        exec.as_ref().and_then(ExecRuntime::sender),
    ) {
        if !c.state.watched.contains(&sender) {
            c.state.watched.push(sender);
        }
        c.state.validate()?;
    }
    let mut canonical = canonical_config
        .as_ref()
        .map(|c| Canonical::new(c, started_ms));
    let checkpoint_path = dir.join("c2-checkpoint.bin");
    // BSC3-039: the periodic checkpoint is written off this loop.
    let checkpoints = canonical_config
        .as_ref()
        .map(|_| CheckpointWriter::spawn(checkpoint_path.clone(), Arc::new(replace)))
        .transpose()?;
    let c2_wanted =
        |g: &Generation| g.component(ComponentId::StateC2).effective == Effective::Shadow;
    // The C2 owner restores its window at start; the follower resumes after it.
    let mut owner = match &canonical_config {
        Some(c) if c2_wanted(&generation) => {
            Some(StateOwner::restore(&c.state, checkpoint_path.clone())?)
        }
        _ => None,
    };
    let mut follower = match &canonical_config {
        Some(c) => {
            let resume = owner
                .as_ref()
                .map(StateOwner::resume_window)
                .unwrap_or_default();
            Some(spawn_follower(c.clone(), resume, canonical_tx, clock)?)
        }
        None => {
            drop(canonical_tx);
            None
        }
    };
    // BSC3-056: the second source's headers, on their own thread.
    let (confirm_tx, confirm_rx) =
        sync_channel::<crate::chain_follower::ConfirmEvent>(config.queue_events);
    let mut confirmer = match canonical_config
        .as_ref()
        .and_then(|c| c.confirm.as_ref().map(|k| (c, k)))
    {
        Some((c, k)) => Some(spawn_confirmer(c, k, confirm_tx)?),
        None => {
            drop(confirm_tx);
            None
        }
    };
    if let Some(c) = canonical.as_mut() {
        c.confirm_rx = Some(confirm_rx);
    }
    let mut next_checkpoint = Instant::now()
        + Duration::from_millis(
            canonical_config
                .as_ref()
                .map_or(0, |c| c.state.checkpoint_ms),
        );
    let mut next_census = Instant::now() + Duration::from_millis(config.census_ms);
    let mut next_status = Instant::now();
    let mut next_reconcile = Instant::now();
    let reconcile_every = Duration::from_millis(config.reconcile_ms);
    let canonical_slice = Duration::from_millis(config.canonical_slice_ms);
    let status_every = Duration::from_millis(config.status_ms);
    let index = |s: SourceId| s.index();
    let mut stop_journaled = false;
    // Which sources run this pass (the reconcile sets it; LIVE included).
    let mut source_on = SourceId::ALL.map(|s| generation.source(s).effective == Effective::Shadow);
    loop {
        let stopping = stop_file.exists();
        // One fresh monotonic/wall pair per pass for the sightings' wall time.
        let wall = WallPair::read(clock);
        if stopping && !stop_journaled {
            journal.record(RecordKind::Runtime, clock.now_ns(), STOP_PAYLOAD.to_vec());
            stop_journaled = true;
        }
        if stopping {
            // Every adapter and the follower stop in parallel; the joins
            // below then wait for the slowest one only.
            for a in adapters.values() {
                a.stop.store(true, Ordering::Release);
            }
            if let Some(f) = follower.as_ref() {
                f.stop.store(true, Ordering::Release);
            }
        }
        if Instant::now() >= next_reconcile || stopping {
            next_reconcile = Instant::now() + reconcile_every;
            if !stopping {
                generation = store.current()?;
                // Plan 28.10.: what the owner's LIVE gate makes effective
                // now; a requested LIVE runs only while it is clear.
                let now_ms = wall_or_expired(observed_now());
                // BSC3-050: the LIVE qualification verified once for this
                // reconcile, for the gate's view and the LIVE path alike.
                let qualified = live
                    .is_some()
                    .then(|| crate::live_qualification::view(repo_root));
                let live_view = LiveView::of(
                    &generation,
                    sources_dir,
                    qualified.as_ref().map(|(q, _)| &q.view),
                    now_ms,
                    live.as_ref(),
                );
                let exits = live.as_ref().is_some_and(|l| l.holds_positions());
                let eff = |e: crate::sources::Entity| live_view.effective(&generation, e, exits);
                let running = |e: Effective| matches!(e, Effective::Shadow | Effective::Live);
                let Gates {
                    flap_admitted: _,
                    strategy_on,
                    c3_wanted,
                    c2_on,
                } = gates(eff, generation.component(ComponentId::ExecC3).effective);
                for source in SourceId::ALL {
                    source_on[index(source)] = running(eff(source.into()));
                }
                if let Some(c) = canonical.as_mut() {
                    // Each switch is journaled: a replay starts and ends its
                    // engine at the same record.
                    match (strategy_on, c.strategy.is_some(), &strategy_gas) {
                        (true, false, Ok(gas)) => {
                            journal.record(
                                RecordKind::Runtime,
                                clock.now_ns(),
                                crate::runtime_strategy::switch_payload(true),
                            );
                            c.strategy = Some(crate::runtime_strategy::StrategyRuntime::new(
                                config.strategy.engine.clone(),
                                gas.clone(),
                            ));
                            c.strategy_refused = None;
                        }
                        (true, false, Err(why)) => c.strategy_refused = Some(why.clone()),
                        (false, running, _) => {
                            if running {
                                journal.record(
                                    RecordKind::Runtime,
                                    clock.now_ns(),
                                    crate::runtime_strategy::switch_payload(false),
                                );
                            }
                            c.strategy = None;
                            c.strategy_refused = None;
                        }
                        _ => {}
                    }
                    c.strategy_config = strategy_on.then(|| config.strategy.engine.clone());
                    // R1: a strategy switched on or off changes who decides
                    // the open lots' exits.
                    refresh_lot_deciders(c, &mut live);
                }
                if let (Some(c), Some(w)) = (&canonical_config, &checkpoints) {
                    // Switched off, the last window is written here, after
                    // the writer's writes (counted first) and never beside one.
                    if !c2_on && let Some(o) = owner.as_mut() {
                        for written in w.idle() {
                            o.checkpoint_written(written);
                        }
                    }
                    StateOwner::switch(&mut owner, c2_on, &c.state, &checkpoint_path, |p, b| {
                        w.write_now(p, b, replace)
                    })?;
                }
                let active_endpoint = canonical.as_ref().map_or(0, |c| c.active);
                if let Some(x) = exec.as_mut() {
                    let wanted = c3_wanted;
                    let follower = canonical_config
                        .as_ref()
                        // BSC3-036: the endpoint the follower follows now.
                        .map(|c| (c.endpoint_at(active_endpoint), &c.limits));
                    x.tick(wanted, owner.as_ref(), follower, started_ms);
                }
                if let Some(l) = live.as_mut() {
                    let follower = canonical_config
                        .as_ref()
                        // BSC3-036: the endpoint the follower follows now.
                        .map(|c| (c.endpoint_at(active_endpoint), &c.limits));
                    // BSC3-051: the providers' answers first; a connection
                    // the maintenance would give up may hold one.
                    l.poll_responses(&FnClock {
                        id: CLOCK_ID_MONOTONIC,
                        now: || clock.now_ns(),
                    });
                    // REV.1.0 R2: each answer with its time, the attempts'
                    // record.
                    for a in l.take_answers() {
                        journal.record(
                            RecordKind::Runtime,
                            a.at_ns.unwrap_or_else(|| clock.now_ns()),
                            crate::runtime_journal::live_answer_payload(&a),
                        );
                    }
                    l.record_responses();
                    match &qualified {
                        Some(q) => l.tick_with(q, owner.as_ref(), follower, started_ms, now_ms),
                        None => l.tick(owner.as_ref(), follower, started_ms, now_ms),
                    }
                }
                // REV.1.0 R3: what the operator decided of the lots since
                // (`bsc_runtime live lot`, `live adopt-g07`): the strategy
                // lets a lot no longer open go and holds an opened one
                // (journaled first), a resumed lot's curve is read again.
                let (changes, resumed) =
                    live.as_mut().map(|l| l.take_decided()).unwrap_or_default();
                if let Some(c) = canonical.as_mut()
                    && (!changes.is_empty() || !resumed.is_empty())
                {
                    let at = clock.now_ns();
                    if let Some(s) = c.strategy.as_mut() {
                        for change in changes {
                            match change {
                                live::PositionChange::Opened {
                                    token,
                                    tokens,
                                    spent_wei,
                                    filled_ms,
                                } => s.hold(token, tokens, spent_wei, filled_ms, at, &mut journal),
                                live::PositionChange::Ended { token } => {
                                    s.unhold(token, at, &mut journal)
                                }
                            }
                        }
                    }
                    for token in resumed {
                        c.lot.resume(token);
                    }
                    refresh_lot_deciders(c, &mut live);
                }
            }
            for source in SourceId::ALL {
                let i = index(source);
                let want = !stopping && source_on[i] && config.endpoint(source).is_some();
                match (want, adapters.contains_key(&i)) {
                    (true, false) => {
                        let url = config.endpoint(source).unwrap_or_default().to_string();
                        let a = spawn_adapter(
                            source,
                            url,
                            limits,
                            tx.clone(),
                            Arc::clone(&dropped[i]),
                            clock,
                        )?;
                        adapters.insert(i, a);
                        journal.record(
                            RecordKind::Runtime,
                            clock.now_ns(),
                            source_event_payload(source, SourceEvent::Started),
                        );
                    }
                    (false, true) => {
                        // The adapter reports its own last Disconnected(Stopped).
                        if let Some(a) = adapters.remove(&i) {
                            a.stop.store(true, Ordering::Release);
                            let _ = a.handle.join();
                        }
                        journal.record(
                            RecordKind::Runtime,
                            clock.now_ns(),
                            source_event_payload(source, SourceEvent::Stopped),
                        );
                    }
                    (true, true) if adapters.get(&i).is_some_and(|a| a.handle.is_finished()) => {
                        // An adapter that ended on its own (refused config, panic) is
                        // disconnected and started again at the next reconcile.
                        if let Some(a) = adapters.remove(&i) {
                            let _ = a.handle.join();
                        }
                        core.observe(
                            source,
                            Observation::Disconnected {
                                rate_limited: false,
                            },
                            clock.now_ns(),
                            None,
                        );
                        journal.record(
                            RecordKind::Runtime,
                            clock.now_ns(),
                            source_event_payload(source, SourceEvent::Stopped),
                        );
                    }
                    _ => {}
                }
            }
        }
        // Events, bounded per pass so status and stop stay timely.
        let deadline = Instant::now() + reconcile_every;
        while Instant::now() < deadline {
            // The canonical events do not wait for the pass (Phase I gate
            // H1): drained before every ingress event and every slice.
            drain_canonical(
                &mut canonical,
                canonical_rx.as_ref(),
                &mut owner,
                &mut exec,
                &mut live,
                &mut journal,
                clock,
            );
            let wait = deadline
                .saturating_duration_since(Instant::now())
                .min(canonical_slice);
            let (source, at, event) = match rx.recv_timeout(wait) {
                Ok(e) => e,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            let observation = match &event {
                Event::Full { hash, raw, .. } => Observation::Full { hash: *hash, raw },
                Event::HashOnly { hash } => Observation::HashOnly { hash: *hash },
                Event::Rejected => Observation::Rejected,
                Event::Connected => {
                    rate_limited_last[index(source)] = false;
                    journal.record(
                        RecordKind::Runtime,
                        at,
                        source_event_payload(source, SourceEvent::Connected),
                    );
                    Observation::Connected
                }
                Event::Disconnected { rate_limited } => {
                    rate_limited_last[index(source)] = *rate_limited;
                    let kind = if *rate_limited {
                        SourceEvent::RateLimited
                    } else {
                        SourceEvent::Disconnected
                    };
                    journal.record(RecordKind::Runtime, at, source_event_payload(source, kind));
                    Observation::Disconnected {
                        rate_limited: *rate_limited,
                    }
                }
            };
            let decided = core.observe(source, observation, at, wall.wall_us(at));
            if let (
                Some(o),
                Event::Full {
                    hash, from, nonce, ..
                },
            ) = (owner.as_mut(), &event)
                && (decided.is_some_and(|l| l.venue() == Venue::Flap) || o.watches(from))
            {
                // Source lanes are indexed as in `SourceId::ALL` (five).
                o.pending(*hash, *from, *nonce, index(source) as u8, at);
            }
            // Plan 28.10.: on the LIVE path when it is armed, the first
            // verified full sighting from a granted source (never a
            // hash-only one: it carries no transaction); BSC3-075: also
            // when a source the grant does not name saw it first.
            if let (Some(l), Some(o), Some((by, hash, first)), Event::Full { raw, .. }) = (
                live.as_mut(),
                owner.as_ref(),
                live_sighting(decided),
                &event,
            ) && l.armed()
            {
                let now = FnClock {
                    id: CLOCK_ID_MONOTONIC,
                    now: || clock.now_ns(),
                };
                if let Some(buy) = l.on_sighting(by, hash, first, raw, at, o, &now) {
                    journal.record(
                        RecordKind::Runtime,
                        buy.times.sink_ns,
                        crate::runtime_journal::live_release_payload(
                            0,
                            Some(by),
                            &buy.handoff,
                            buy.launch_hash,
                            buy.token,
                            0,
                            &buy.times,
                            &buy.raw,
                        ),
                    );
                }
            }
            // C3: the first verified sighting of a C1-scope launch runs the
            // production path to the PAPER sink (when armed).
            else if let (
                Some(x),
                Some(o),
                Some(Launch::First {
                    hash,
                    venue: Venue::Flap,
                    ..
                }),
                Event::Full { raw, .. },
            ) = (exec.as_mut(), owner.as_ref(), decided, &event)
                && let Some(buy) = x.on_launch(
                    hash,
                    raw,
                    at,
                    o,
                    &FnClock {
                        id: CLOCK_ID_MONOTONIC,
                        now: || clock.now_ns(),
                    },
                )
            {
                journal.record(
                    RecordKind::Runtime,
                    buy.times.sink_ns,
                    paper_handoff_payload(&buy),
                );
            }
            match decided {
                Some(Launch::First { hash, source, .. }) => {
                    if let Event::Full { raw, .. } = &event {
                        journal.record(
                            RecordKind::RawSignedTx,
                            at,
                            first_sighting_payload(source, &hash, raw),
                        );
                    }
                }
                Some(Launch::Later {
                    hash,
                    source,
                    lag_ns,
                    ..
                }) => {
                    journal.record(
                        RecordKind::Runtime,
                        at,
                        later_sighting_payload(source, &hash, lag_ns),
                    );
                }
                // Not a launch: the pending capture scope decides.
                None => {
                    if let Event::Full { raw, hash, .. } = &event
                        && core.last_first_full()
                        && config.capture_pending.keeps(core.last_venue_target())
                    {
                        journal.record(
                            RecordKind::TxpoolAccepted,
                            at,
                            first_sighting_payload(source, hash, raw),
                        );
                    }
                }
            }
        }
        // Canonical events: every queued one, then the joins that are due.
        if let (Some(c), Some(crx)) = (canonical.as_mut(), canonical_rx.as_ref()) {
            if stopping && let Some(f) = follower.take() {
                // The follower ends within its read tick and request bound;
                // its events up to then are still processed.
                f.stop.store(true, Ordering::Release);
                loop {
                    match crx.recv_timeout(reconcile_every) {
                        Ok((at, event)) => apply_canonical(
                            c,
                            &mut owner,
                            &mut exec,
                            &mut live,
                            &mut journal,
                            clock,
                            at,
                            event,
                        ),
                        Err(RecvTimeoutError::Timeout) if !f.handle.is_finished() => {}
                        Err(_) => break,
                    }
                }
                let _ = f.handle.join();
            }
            while let Ok((at, event)) = crx.try_recv() {
                apply_canonical(
                    c,
                    &mut owner,
                    &mut exec,
                    &mut live,
                    &mut journal,
                    clock,
                    at,
                    event,
                );
            }
            // BSC3-056: the second source's headers, then whether it
            // confirms the followed chain now (the LIVE BUY gate; also
            // before each block's BUY, apply_canonical).
            c.refresh_confirmation(&mut live, wall_or_expired(observed_now()));
            if stopping && let Some(f) = confirmer.take() {
                f.stop.store(true, Ordering::Release);
                let _ = f.handle.join();
            }
            let now = if stopping { u64::MAX } else { clock.now_ns() };
            c.join(now, &core, &mut journal);
            if stopping && c.state != "STOPPED" {
                // A window ends only if it started (a block mined after the start).
                if let Some((n, h)) = c.head.filter(|_| c.first_journaled) {
                    journal.record(
                        RecordKind::Canonical,
                        clock.now_ns(),
                        window_payload(b'E', n, &h),
                    );
                }
                c.state = "STOPPED";
            } else if follower.as_ref().is_some_and(|f| f.handle.is_finished()) {
                c.state = "FAILING";
            }
        }
        if Instant::now() >= next_status || stopping {
            next_status = Instant::now() + status_every;
            journal_drops(&mut journal, &dropped, &mut journaled_drops, clock.now_ns());
            let now = clock.now_ns();
            let stats: Vec<_> = SourceId::ALL
                .iter()
                .map(|&s| {
                    let i = index(s);
                    let mut st = core.stats(s);
                    st.dropped += dropped[i].load(Ordering::Relaxed);
                    let h = match generation.source(s).effective {
                        Effective::Off
                        | Effective::Blocked(_)
                        | Effective::Paper
                        | Effective::Disabled
                        | Effective::Enabled
                        | Effective::PerDay(_) => Health::Off,
                        Effective::Shadow | Effective::Live if config.endpoint(s).is_none() => {
                            Health::Unqualified
                        }
                        Effective::Shadow | Effective::Live if stopping => Health::Off,
                        Effective::Shadow | Effective::Live => {
                            health(&st, rate_limited_last[i], now, config.quiet_ms * 1_000_000)
                        }
                    };
                    (s, st, h)
                })
                .collect();
            let observed = observed_now()?;
            let bytes = status_bytes(
                &generation,
                &stats,
                clock,
                started_ms,
                observed,
                core.launches(),
                &journal_rel.display().to_string(),
                journal.appended(),
                journal.lost(),
                Some(&journal.capture()),
                canonical.as_ref(),
                owner.as_ref(),
                exec.as_ref(),
                live.as_ref(),
                config.status_ms,
                &census_refresh,
            )?;
            replace_volatile(&status_path, &bytes)?;
        }
        if let (Some(o), Some(w)) = (owner.as_mut(), &checkpoints) {
            let due = stopping || Instant::now() >= next_checkpoint;
            if due {
                next_checkpoint = Instant::now()
                    + Duration::from_millis(
                        canonical_config
                            .as_ref()
                            .map_or(0, |c| c.state.checkpoint_ms),
                    );
            }
            // At stop after the follower's last events (processed above).
            checkpoint_pass(o, w, due, stopping, replace);
        }
        // BSC3-016 review L5: a finished census reaches the status at once.
        if let Some(done) = census_job.take_if(|j| j.is_finished()) {
            census_refresh.ended(done.join());
        }
        if Instant::now() >= next_census && !stopping {
            next_census = Instant::now() + Duration::from_millis(config.census_ms);
            // A checkpoint of the head: a killed process's window still ends here.
            if let Some((n, h)) = canonical
                .as_ref()
                .filter(|c| c.first_journaled)
                .and_then(|c| c.head)
            {
                journal.record(
                    RecordKind::Canonical,
                    clock.now_ns(),
                    window_payload(b'K', n, &h),
                );
            }
            // The census replays every journal: on its own thread, one at a
            // time, never on this decision loop (Phase F gate M1). A census
            // failure is visible in the log; observation goes on.
            if census_job.as_ref().is_none_or(|j| j.is_finished()) {
                if let Some(done) = census_job.take() {
                    census_refresh.ended(done.join());
                }
                let (config, rundir, active) = (
                    config.clone(),
                    rundir.to_path_buf(),
                    journal.path().to_path_buf(),
                );
                census_job = std::thread::Builder::new()
                    .name("bsc-census".into())
                    .spawn(move || {
                        let written = write_census(&config, &rundir, Some(&active))
                            .and_then(|_| observed_now());
                        if let Err(e) = &written {
                            eprintln!("g04 census: {e}");
                        }
                        written
                    })
                    .map_err(|e| {
                        eprintln!("g04 census thread: {e}");
                        census_refresh.state = Some(format!("FAILED: no census thread: {e}"));
                    })
                    .ok();
            }
        }
        // Phase E review L3: the Four drift guard on its own thread; its
        // verdict is journaled and in the status, a drift makes every Four
        // token derivation UNKNOWN until a probe matches again.
        if let (Some(cc), Some(c)) = (canonical_config.as_ref(), canonical.as_mut()) {
            if let Some(job) = four_job.take_if(|j| j.is_finished()) {
                let (code, block, verdict) = match job.join() {
                    Ok(Ok(o)) if o.drift.is_empty() => {
                        c.four_drift = false;
                        (
                            1,
                            o.block_number,
                            format!("MATCH at block {}", o.block_number),
                        )
                    }
                    Ok(Ok(o)) => {
                        c.four_drift = true;
                        let at = o.block_number;
                        (
                            2,
                            at,
                            format!("DRIFT at block {at}: {}", o.drift.join("; ")),
                        )
                    }
                    Ok(Err(e)) => (3, 0, format!("PROBE_FAILED: {e}")),
                    Err(_) => (3, 0, "PROBE_FAILED: the probe thread panicked".to_owned()),
                };
                c.four_deployment = verdict;
                journal.record(
                    RecordKind::Runtime,
                    clock.now_ns(),
                    crate::runtime_journal::four_probe_payload(code, block),
                );
            }
            if four_job.is_none() && Instant::now() >= next_four && !stopping {
                let (endpoint, limits) = (cc.endpoint_at(c.active).to_string(), cc.limits);
                four_job = std::thread::Builder::new()
                    .name("bsc-four-probe".into())
                    .spawn(move || {
                        crate::venue_probe::probe_four(
                            &endpoint,
                            &limits,
                            &darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_g01_129842711(),
                        )
                    })
                    .map_err(|e| {
                        eprintln!("four probe thread: {e}");
                        // BSC3-016: shown, not only logged.
                        c.four_deployment = format!("PROBE_FAILED: no probe thread: {e}");
                    })
                    .ok();
                next_four = Instant::now() + Duration::from_millis(cc.four_probe_ms);
            }
        }
        if stopping && adapters.is_empty() {
            drop(tx);
            // A clean stop disarms the PAPER path (its fence goes) and the
            // LIVE path (its key goes).
            if let Some(x) = exec.as_mut() {
                x.shutdown();
            }
            if let Some(l) = live.as_mut() {
                l.shutdown();
            }
            journal_drops(&mut journal, &dropped, &mut journaled_drops, clock.now_ns());
            journal.record(RecordKind::Runtime, clock.now_ns(), EXIT_PAYLOAD.to_vec());
            journal.finish()?;
            // The stop is consumed (Paket 8 review L7: a stop file left
            // behind stopped the next start at once).
            if let Err(e) = fs::remove_file(&stop_file)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                eprintln!("{}: {e}", stop_file.display());
            }
            // The last periodic census ends before the final one.
            if let Some(done) = census_job.take() {
                let _ = done.join();
            }
            if let Err(e) = write_census(config, rundir, None) {
                eprintln!("g04 census: {e}");
            }
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BSC3-076 re-review HIGH: a lease that holds the grant's and the day's
    /// last slot keeps the runtime's reconcile on (the venue, the runtime,
    /// C2, C3 and the sources stay LIVE for its BUY); any other attempt
    /// still finds them used.
    #[test]
    fn a_lease_on_the_last_slot_keeps_the_reconcile_on() {
        use crate::sources::tests::{ORIGIN, grant_of, live_ready_limited, qualified_in};
        use crate::sources::{
            BUY_LIMIT_EXHAUSTED, LIVE_GRANT_ATTEMPTS_EXHAUSTED, LiveGrant, LiveIntent,
            LiveIntentKind, LiveStrategies,
        };
        let (dir, s, now) = live_ready_limited("runtime-lease-last", (1, 1), |now| LiveGrant {
            max_attempts: 1,
            ..grant_of(now + 3_600_000, LiveStrategies::C3)
        });
        let q = qualified_in(&s);
        let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
        let attempt = alloy_primitives::hex::encode([0xa0; 32]);
        let id = alloy_primitives::hex::encode([0xa1; 32]);
        let lease = LiveIntent {
            id: &id,
            component: ComponentId::ExecC3,
            kind: LiveIntentKind::Buy,
            input_wei: 1_000,
            max_wei: 1_000,
            token: [0; 20],
            nonce: 7,
            origin: ORIGIN,
            max_block: 900,
            exit_reserve_wei: 0,
        };
        s.lease_live(&armed, &lease, &attempt, &q, now).unwrap();
        let g = s.current().unwrap();
        let entities = [
            crate::sources::Entity::Runtime,
            VenueId::Flap.into(),
            ComponentId::StateC2.into(),
            ComponentId::ExecC3.into(),
            SourceId::PublicNode.into(),
        ];
        // The owner state's session (the switches are enabled in it).
        let view = |held: Option<&str>| LiveView {
            boot: Some("boot-1".into()),
            ..LiveView::read(&g, &dir, Some(&q), now, held)
        };
        let other = alloy_primitives::hex::encode([0xb0; 32]);
        let (held, others, none) = (view(Some(&attempt)), view(Some(&other)), view(None));
        for e in entities {
            assert_eq!(held.effective(&g, e, false), Effective::Live, "{e:?}");
            for view in [&others, &none] {
                assert!(
                    matches!(
                        view.effective(&g, e, false),
                        Effective::Blocked(r) if r == LIVE_GRANT_ATTEMPTS_EXHAUSTED || r == BUY_LIMIT_EXHAUSTED
                    ),
                    "{e:?}"
                );
            }
        }
        fs::remove_dir_all(dir).unwrap();
    }

    /// Review of the limits (LOW): a grant without limits admits no BUY
    /// but keeps the exit path of positions held under it (the reason is
    /// exempt for exits).
    #[test]
    fn a_grant_without_limits_keeps_the_exit_path_on() {
        use crate::sources::tests::{grant_of, live_ready_granting, qualified};
        use crate::sources::{LIVE_GRANT_LIMITS_MISSING, LiveGrant, LiveStrategies};
        let (dir, s, now) = live_ready_granting("runtime-no-limits", |now| LiveGrant {
            limits: None,
            ..grant_of(now + 3_600_000, LiveStrategies::StrategyC)
        });
        let g = s.current().unwrap();
        let view = LiveView {
            boot: Some("boot-1".into()),
            now_ms: now,
            daily: crate::sources::buy_usage(&dir, now).ok(),
            total: crate::sources::total_usage(&dir).ok(),
            spent: Some(0),
            qualification: qualified(),
        };
        for e in [
            crate::sources::Entity::Runtime,
            ComponentId::StrategyC.into(),
            ComponentId::StateC2.into(),
            VenueId::Flap.into(),
        ] {
            assert_eq!(
                view.effective(&g, e, false),
                Effective::Blocked(LIVE_GRANT_LIMITS_MISSING),
                "{e:?}"
            );
            assert_eq!(view.effective(&g, e, true), Effective::Live, "{e:?}");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    /// BSC3-058/071 (review MEDIUM): under a grant that admits no BUY the
    /// runtime keeps the entities requested LIVE effective for the exits
    /// of the positions the LIVE path holds (the strategy decides them, C2
    /// follows them); without positions they stay blocked by that reason.
    #[test]
    fn a_grant_without_buys_keeps_the_exit_path_on_while_positions_are_held() {
        use crate::sources::tests::{grant_of, live_ready_granting, qualified};
        use crate::sources::{LIVE_GRANT_EXIT_ONLY, LiveGrant, LiveStrategies};
        let (dir, s, now) = live_ready_granting("runtime-exit-only", |now| LiveGrant {
            exit_only: true,
            ..grant_of(now + 3_600_000, LiveStrategies::StrategyC)
        });
        let g = s.current().unwrap();
        let view = LiveView {
            boot: Some("boot-1".into()),
            now_ms: now,
            daily: crate::sources::buy_usage(&dir, now).ok(),
            total: crate::sources::total_usage(&dir).ok(),
            spent: Some(0),
            qualification: qualified(),
        };
        for e in [
            crate::sources::Entity::Runtime,
            ComponentId::StrategyC.into(),
            ComponentId::StateC2.into(),
            VenueId::Flap.into(),
        ] {
            assert_eq!(
                view.effective(&g, e, false),
                Effective::Blocked(LIVE_GRANT_EXIT_ONLY),
                "{e:?}"
            );
            assert_eq!(view.effective(&g, e, true), Effective::Live, "{e:?}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    const CONFIG: &str = include_str!("../../../../config/ingress.public.env");
    /// R4: the repository's bound builder policy, the frozen import.
    const POLICY: &str = "bsc/native/evidence/g06a/g07_binary_import_20261016/builder_policy.dbrec";
    /// The shipped refused accounts: the frozen G0.7 launcher and buyer.
    const REFUSED: &str = "BSC_EXEC_REFUSED_ACCOUNTS=0x3e7B487Ae5e1c49aB1cF6222BDb5cf016e86aacd,0x9c8a0Db41914beEE52Dd49790c4F087E252743DE\n";

    /// Plan 28.10.: the LIVE keys go together, need the executor's keys
    /// and a buyer C2 watches; the route is the builders or a loopback port.
    #[test]
    fn the_live_keys_go_together_and_need_a_watched_buyer() {
        assert_eq!(RuntimeConfig::parse(CONFIG).unwrap().live, None);
        let buyer = "0x00000000000000000000000000000000000000aB";
        let watched = CONFIG.replace(
            "BSC_STATE_WATCHED=\n",
            &format!("BSC_STATE_WATCHED={buyer}\n"),
        );
        let keys = |route: &str| {
            format!(
                "BSC_LIVE_KEY=~/.darkbot/bsc/live/buyer.key\nBSC_LIVE_BUYER={buyer}\n\
                 BSC_LIVE_ROUTE={route}\nBSC_LIVE_CONNECT_TIMEOUT_MS=5000\n"
            )
        };
        let live = RuntimeConfig::parse(&format!("{watched}{}", keys("LOOPBACK:18545")))
            .unwrap()
            .live
            .unwrap();
        assert_eq!(live.route, live::LiveRouteConfig::Loopback { port: 18545 });
        assert_eq!(live.buyer[19], 0xab);
        assert!(live.key_path.is_absolute() && live.key_path.ends_with("live/buyer.key"));
        let builders = RuntimeConfig::parse(&format!("{watched}{}", keys("BUILDERS")))
            .unwrap()
            .live
            .unwrap();
        assert_eq!(
            builders.route,
            live::LiveRouteConfig::Builders {
                blockrazor_token: None
            }
        );
        for bad in [
            // Not watched by C2.
            format!("{CONFIG}{}", keys("BUILDERS")),
            // Not all three.
            format!("{watched}BSC_LIVE_BUYER={buyer}\n"),
            format!("{watched}{}", keys("LOOPBACK:0")),
            format!("{watched}{}", keys("TLS")),
            format!(
                "{watched}{}BSC_LIVE_BLOCKRAZOR_TOKEN_FILE=/tmp/t\n",
                keys("LOOPBACK:1")
            ),
            format!("{watched}{}", keys("BUILDERS").replace("5000", "50")),
        ] {
            assert!(RuntimeConfig::parse(&bad).is_err(), "{bad}");
        }
        // BSC3-056: a LIVE path without an independent second source.
        let no_confirm = format!("{watched}{}", keys("BUILDERS")).replace(
            "BSC_CANONICAL_CONFIRM_RPC=https://bsc-rpc.publicnode.com\n",
            "BSC_CANONICAL_CONFIRM_RPC=\n",
        );
        let e = RuntimeConfig::parse(&no_confirm).unwrap_err();
        assert!(e.contains("BSC_CANONICAL_CONFIRM_RPC"), "{e}");
        // R1 review: a sender whose paths leave out Strategy C's could buy
        // and never sell.
        let no_exits = format!("{watched}{}", keys("BUILDERS")).replace(
            "BSC_EXEC_SENDER_PATHS=C3+STRATEGY_C\n",
            "BSC_EXEC_SENDER_PATHS=C3\n",
        );
        let e = RuntimeConfig::parse(&no_exits).unwrap_err();
        assert!(e.contains("BSC_EXEC_SENDER_PATHS with STRATEGY_C"), "{e}");
    }

    /// Messplan §3b B (offline, at the reconcile's gate): which paths the
    /// effective modes let run. Not named components and venues are OFF; a
    /// LIVE request without a grant is BLOCKED.
    #[test]
    fn the_runtime_mode_bounds_the_components_as_the_plan_says() {
        use crate::sources::Entity;
        use Effective::{Blocked, Live, Off, Paper, Shadow};
        #[derive(Clone, Copy)]
        struct Modes {
            runtime: Effective,
            flap: Effective,
            c2: Effective,
            exec: Effective,
            strategy: Effective,
        }
        let off = Modes {
            runtime: Off,
            flap: Off,
            c2: Off,
            exec: Off,
            strategy: Off,
        };
        let run = |m: Modes| {
            gates(
                |e| match e {
                    Entity::Runtime => m.runtime,
                    Entity::Venue(VenueId::Flap) => m.flap,
                    Entity::Component(ComponentId::StateC2) => m.c2,
                    Entity::Component(ComponentId::StrategyC) => m.strategy,
                    Entity::Component(ComponentId::ExecC3) => m.exec,
                    _ => Off,
                },
                m.exec,
            )
        };
        // (flap admitted, strategy, C3, C2)
        let g = |m: Modes| {
            let g = run(m);
            (g.flap_admitted, g.strategy_on, g.c3_wanted, g.c2_on)
        };
        let blocked = Blocked("ECONOMIC_LIVE_DISABLED");
        let cases = [
            // B01: everything OFF.
            ("B01", off, (false, false, false, false)),
            // B02: runtime OFF, C2 SHADOW (PublicNode SHADOW: a source).
            (
                "B02",
                Modes { c2: Shadow, ..off },
                (false, false, false, true),
            ),
            // B03: runtime SHADOW, Flap SHADOW, Strategy SHADOW: the ledger.
            (
                "B03",
                Modes {
                    runtime: Shadow,
                    flap: Shadow,
                    strategy: Shadow,
                    ..off
                },
                (true, true, false, false),
            ),
            // B04: runtime SHADOW, Flap, EXEC, C2 SHADOW: no C3 hand-off.
            (
                "B04",
                Modes {
                    runtime: Shadow,
                    flap: Shadow,
                    exec: Shadow,
                    c2: Shadow,
                    ..off
                },
                (true, false, false, true),
            ),
            // B05: runtime PAPER, Flap, EXEC, C2 SHADOW, Strategy OFF: C3.
            (
                "B05",
                Modes {
                    runtime: Paper,
                    flap: Shadow,
                    exec: Shadow,
                    c2: Shadow,
                    ..off
                },
                (true, false, true, true),
            ),
            // B06: runtime PAPER, Flap, Strategy SHADOW, EXEC OFF.
            (
                "B06",
                Modes {
                    runtime: Paper,
                    flap: Shadow,
                    strategy: Shadow,
                    ..off
                },
                (true, true, false, false),
            ),
            // B07: runtime PAPER, Flap, C2, EXEC, Strategy SHADOW: both.
            (
                "B07",
                Modes {
                    runtime: Paper,
                    flap: Shadow,
                    c2: Shadow,
                    exec: Shadow,
                    strategy: Shadow,
                },
                (true, true, true, true),
            ),
            // B08: as B07, Flap OFF: no Flap candidate reaches either.
            (
                "B08",
                Modes {
                    runtime: Paper,
                    c2: Shadow,
                    exec: Shadow,
                    strategy: Shadow,
                    ..off
                },
                (false, false, false, true),
            ),
            // B09: as B05, C2 OFF: C3 is wanted, it refuses without a C2
            // snapshot (WAITING_C2, runtime_exec).
            (
                "B09",
                Modes {
                    runtime: Paper,
                    flap: Shadow,
                    exec: Shadow,
                    ..off
                },
                (true, false, true, false),
            ),
            // B10: as B05, EXEC OFF: no C3.
            (
                "B10",
                Modes {
                    runtime: Paper,
                    flap: Shadow,
                    c2: Shadow,
                    ..off
                },
                (true, false, false, true),
            ),
            // B14/B15: a LIVE request without a grant is BLOCKED: nothing
            // of it runs.
            (
                "B14 runtime",
                Modes {
                    runtime: blocked,
                    flap: Shadow,
                    c2: Shadow,
                    exec: Shadow,
                    strategy: Shadow,
                },
                (true, false, false, true),
            ),
            (
                "B14 flap",
                Modes {
                    runtime: Paper,
                    flap: blocked,
                    c2: Shadow,
                    exec: Shadow,
                    strategy: Shadow,
                },
                (false, false, false, true),
            ),
            (
                "B14 exec",
                Modes {
                    runtime: Paper,
                    flap: Shadow,
                    c2: Shadow,
                    exec: blocked,
                    strategy: Shadow,
                },
                (true, true, false, true),
            ),
            (
                "B14 c2",
                Modes {
                    runtime: Paper,
                    flap: Shadow,
                    c2: blocked,
                    exec: Shadow,
                    strategy: Shadow,
                },
                (true, true, true, false),
            ),
            (
                "B14 strategy",
                Modes {
                    runtime: Paper,
                    flap: Shadow,
                    c2: Shadow,
                    exec: Shadow,
                    strategy: blocked,
                },
                (true, false, true, true),
            ),
            // Under a verified grant the LIVE path's modes run (plan 28.10.).
            (
                "LIVE",
                Modes {
                    runtime: Live,
                    flap: Live,
                    c2: Live,
                    exec: Off,
                    strategy: Live,
                },
                (true, true, false, true),
            ),
        ];
        for (id, m, want) in cases {
            assert_eq!(g(m), want, "{id}");
        }
        // The eight C2/EXEC/STRATEGY_C combinations under PAPER and Flap
        // SHADOW (Messplan §3b B, full sub-combination).
        for (c2, exec, strategy, want) in [
            (Off, Off, Off, (true, false, false, false)),
            (Off, Off, Shadow, (true, true, false, false)),
            (Off, Shadow, Off, (true, false, true, false)),
            (Off, Shadow, Shadow, (true, true, true, false)),
            (Shadow, Off, Off, (true, false, false, true)),
            (Shadow, Off, Shadow, (true, true, false, true)),
            (Shadow, Shadow, Off, (true, false, true, true)),
            (Shadow, Shadow, Shadow, (true, true, true, true)),
        ] {
            let m = Modes {
                runtime: Paper,
                flap: Shadow,
                c2,
                exec,
                strategy,
            };
            assert_eq!(g(m), want, "{c2:?} {exec:?} {strategy:?}");
        }
    }

    /// BSC3-079: the configured gas limits cover the frozen evidence the
    /// configuration names; one below a class's largest gas (a BUY in its
    /// token's launch block included) refuses the start.
    #[test]
    fn the_gas_limits_cover_the_frozen_evidence() {
        use darkbot_bsc_trading::economics::network::{Action, GasEvidence, PathClass, Venue};
        let config = RuntimeConfig::parse(CONFIG).unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let gas = config.strategy.gas(&repo).unwrap();
        let exec = config.exec.as_ref().unwrap().exec.clone();
        gas_limits_cover(&exec, &gas).unwrap();
        for (action, limit) in [
            (Action::Buy, exec.gas_limit),
            (Action::BuyInLaunchBlock, exec.gas_limit),
            (Action::Sell, exec.sell_gas_limit),
            (Action::Approve, exec.approve_gas_limit),
        ] {
            let mut over = GasEvidence::default();
            over.add(
                PathClass {
                    venue: Venue::Flap,
                    action,
                },
                limit + 1,
            )
            .unwrap();
            let e = gas_limits_cover(&exec, &over).unwrap_err();
            assert!(e.contains(&format!("{action:?} gas {}", limit + 1)), "{e}");
        }
    }

    /// BSC3-079 review: the start's check reads the build's frozen evidence,
    /// not the configured files: a configuration whose Strategy C evidence
    /// does not load still has its executor's gas limits checked.
    #[test]
    fn the_start_checks_the_gas_limits_whatever_the_strategy_evidence() {
        let mut config = RuntimeConfig::parse(CONFIG).unwrap();
        exec_gas_covered(&config).unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let configured = config.strategy.gas(&repo).unwrap();
        assert_eq!(configured, StrategyConfig::frozen_gas().unwrap());
        config.strategy.gas_evidence.clear();
        assert!(config.strategy.gas(&repo).is_err());
        exec_gas_covered(&config).unwrap();
        config.exec.as_mut().unwrap().exec.sell_gas_limit = 700_000;
        let e = exec_gas_covered(&config).unwrap_err();
        assert!(e.starts_with("BSC_EXEC_SELL_GAS_LIMIT 700000"), "{e}");
        config.exec = None;
        exec_gas_covered(&config).unwrap();
    }

    #[test]
    fn the_config_is_strict() {
        let c = RuntimeConfig::parse(CONFIG).unwrap();
        assert_eq!(
            c.endpoint(SourceId::PublicNode),
            Some("wss://bsc-rpc.publicnode.com")
        );
        assert_eq!(c.endpoint(SourceId::Drpc), Some("wss://bsc.drpc.org"));
        assert_eq!(
            c.endpoint(SourceId::BlockReq),
            None,
            "no endpoint: UNQUALIFIED"
        );
        assert_eq!((c.reconcile_ms, c.stop_timeout_ms), (500, 15_000));
        // 500 + max(5000 + 5000 + 200, 5000 + 200)
        assert_eq!(c.stop_bound_ms(), 10_700);
        let canonical = c.canonical.as_ref().unwrap();
        assert_eq!(canonical.endpoint, "https://bsc-dataseed.bnbchain.org");
        // BSC3-036: the fallbacks in order, then the primary for any other index.
        assert_eq!(
            canonical.endpoints(),
            [
                "https://bsc-dataseed.bnbchain.org",
                "https://bsc-dataseed1.defibit.io",
                "https://bsc-dataseed1.ninicoin.io"
            ]
        );
        assert_eq!(
            canonical.endpoint_at(2),
            "https://bsc-dataseed1.ninicoin.io"
        );
        assert_eq!(
            canonical.endpoint_at(9),
            "https://bsc-dataseed.bnbchain.org"
        );
        assert_eq!(
            (
                canonical.limits.failover_after,
                canonical.limits.stall_after
            ),
            (3, Duration::from_millis(5_000))
        );
        let fallback = |list: &str| {
            RuntimeConfig::parse(&CONFIG.replace(
                "BSC_CANONICAL_RPC_FALLBACK=https://bsc-dataseed1.defibit.io,https://bsc-dataseed1.ninicoin.io\n",
                &format!("BSC_CANONICAL_RPC_FALLBACK={list}\n"),
            ))
            .map(|c| c.canonical.unwrap().fallbacks)
        };
        assert_eq!(fallback(""), Ok(Vec::new()));
        // R3b: the recovery's bounds; its search over a lease's window.
        assert_eq!(
            (
                canonical.recovery_reads_max,
                canonical.recovery_retry_ms,
                canonical.recovery_search_blocks
            ),
            (2, 60_000, 28_800)
        );
        let search = |blocks: &str| {
            RuntimeConfig::parse(&CONFIG.replace(
                "BSC_CANONICAL_RECOVERY_SEARCH_BLOCKS=28800\n",
                &format!("BSC_CANONICAL_RECOVERY_SEARCH_BLOCKS={blocks}\n"),
            ))
            .map(|c| c.canonical.unwrap().recovery_search_blocks)
        };
        assert_eq!(search("200"), Ok(200), "the lease's window exactly");
        assert!(
            search("199")
                .unwrap_err()
                .contains("BSC_EXEC_LIVE_LEASE_BLOCKS")
        );
        for bad in ["0", "10000001", "x"] {
            assert!(search(bad).is_err(), "{bad}");
        }
        // BSC3-056: the second source, of a host none of the followed has.
        assert_eq!(
            canonical.confirm,
            Some(ConfirmConfig {
                endpoint: "https://bsc-rpc.publicnode.com".into(),
                max_age_ms: 3_000,
                max_lag_blocks: 2,
            })
        );
        let confirm = |rpc: &str| {
            RuntimeConfig::parse(&CONFIG.replace(
                "BSC_CANONICAL_CONFIRM_RPC=https://bsc-rpc.publicnode.com\n",
                &format!("BSC_CANONICAL_CONFIRM_RPC={rpc}\n"),
            ))
            .map(|c| c.canonical.unwrap().confirm)
        };
        assert_eq!(confirm(""), Ok(None));
        for bad in [
            "https://bsc-dataseed1.ninicoin.io",
            "https://bsc-dataseed.bnbchain.org/other",
            "http://bsc-rpc.publicnode.com",
            // The same host in other spellings (review LOW-3).
            "https://BSC-DATASEED1.NINICOIN.IO",
            "https://bsc-dataseed1.defibit.io:443/",
        ] {
            assert!(confirm(bad).is_err(), "{bad}");
        }
        for bad in [
            "http://bsc-dataseed1.defibit.io",
            "https://bsc-dataseed.bnbchain.org",
            "https://a.example,https://a.example",
            "https://a.example,https://A.EXAMPLE:443/",
        ] {
            assert!(fallback(bad).is_err(), "{bad}");
        }
        // An error names no path (it may hold a provider's key).
        let e = fallback("http://a.example/secret-key").unwrap_err();
        assert!(!e.contains("secret-key"), "{e}");
        assert_eq!(canonical.limits.reorg_depth, 64);
        // The C2 window is the follower's; no own account in READ_ONLY.
        assert_eq!(canonical.state.limits.reorg_window, 64);
        assert_eq!(canonical.state.limits.finality_depth, 16);
        assert!(canonical.state.watched.is_empty());
        let one = CONFIG.replace(
            "BSC_STATE_WATCHED=\n",
            "BSC_STATE_WATCHED=0x00000000000000000000000000000000000000aB, 0x0000000000000000000000000000000000000001\n",
        );
        let watched = RuntimeConfig::parse(&one)
            .unwrap()
            .canonical
            .unwrap()
            .state
            .watched;
        assert_eq!(
            (watched.len(), watched[0][19], watched[1][19]),
            (2, 0xab, 1)
        );
        // Without the endpoint the canonical and C2 bounds are unknown keys.
        assert!(RuntimeConfig::parse(&CONFIG.replace("BSC_CANONICAL_RPC=", "#")).is_err());
        let without: String = CONFIG
            .lines()
            .filter(|l| {
                !l.starts_with("BSC_CANONICAL_")
                    && !l.starts_with("BSC_STATE_")
                    && !l.starts_with("BSC_EXEC_")
            })
            .map(|l| format!("{l}\n"))
            .collect();
        assert_eq!(RuntimeConfig::parse(&without).unwrap().canonical, None);
        // The PAPER path needs C2: without the follower it is refused.
        assert!(
            RuntimeConfig::parse(&format!(
                "{without}BSC_EXEC_PAPER_IDENTITY=/tmp/x\nBSC_EXEC_REFUSED_ACCOUNTS=\n"
            ))
            .is_err()
        );
        let exec = c.exec.as_ref().unwrap();
        assert!(
            exec.identity_path
                .ends_with(".darkbot/bsc/paper_identity.key")
        );
        // Paket 8 review L4: the frozen G0.7 wallets are refused as PAPER
        // identity.
        assert_eq!(exec.refused_accounts.len(), 2);
        assert!(exec.identity_path.is_absolute());
        assert_eq!(
            (
                exec.exec.input_wei,
                exec.exec.gas_price_wei,
                exec.exec.gas_limit,
                exec.exec.slippage_bps
            ),
            (1_000_000_000_000_000, 50_000_000, 3_000_000, 100)
        );
        // BSC3-069: an exit's own slippage; the BUY's when not set; the
        // same bounds; only with the PAPER path.
        assert_eq!(exec.exec.exit_slippage_bps, exec.exec.slippage_bps);
        let exit =
            |v: &str| RuntimeConfig::parse(&format!("{CONFIG}BSC_EXEC_EXIT_SLIPPAGE_BPS={v}\n"));
        assert_eq!(
            exit("300").unwrap().exec.unwrap().exec.exit_slippage_bps,
            300
        );
        assert!(exit("10000").is_err());
        let no_exec: String = CONFIG
            .lines()
            .filter(|l| !l.starts_with("BSC_EXEC_"))
            .map(|l| format!("{l}\n"))
            .collect();
        assert_eq!(RuntimeConfig::parse(&no_exec).unwrap().exec, None);
        assert!(
            RuntimeConfig::parse(&format!("{no_exec}BSC_EXEC_EXIT_SLIPPAGE_BPS=300\n")).is_err()
        );
        assert!(RuntimeConfig::parse(&format!("{without}BSC_STATE_WATCHED=\n")).is_err());
        for bad in [
            CONFIG.replace("wss://bsc-rpc", "ws://bsc-rpc"),
            CONFIG.replace("wss://bsc-rpc", "wss://user:pw@bsc-rpc"),
            CONFIG.replace("QUEUE_EVENTS=65536", "QUEUE_EVENTS=1"),
            CONFIG.replace("BSC_RUNTIME_STATUS_MS=1000\n", ""),
            format!("{CONFIG}BSC_RUNTIME_SURPRISE=1\n"),
            format!("{CONFIG}BSC_RUNTIME_QUIET_MS=5000\n"),
            CONFIG
                .replace("BACKOFF_INITIAL_MS=1000", "BACKOFF_INITIAL_MS=60000")
                .replace("BACKOFF_MAX_MS=60000", "BACKOFF_MAX_MS=1000"),
            CONFIG.replace("MAX_FRAME_BYTES=1048576", "MAX_FRAME_BYTES=2097152"),
            CONFIG.replace("https://bsc-dataseed", "http://bsc-dataseed"),
            CONFIG.replace("BSC_CANONICAL_JOIN_DELAY_MS=2000\n", ""),
            CONFIG.replace("BACKOFF_MAX_MS=30000", "BACKOFF_MAX_MS=100"),
            CONFIG.replace("BSC_RUNTIME_CENSUS_MS=60000\n", ""),
            // What the adapters would refuse is refused here (review LOW-5).
            CONFIG.replace("wss://bsc.drpc.org", "wss://bsc.drpc.org?key=1"),
            CONFIG.replace("wss://bsc.drpc.org", "wss://bsc.drpc.org:99999"),
            CONFIG.replace("wss://bsc.drpc.org", "wss://bsc_rpc.example"),
            CONFIG.replace(
                "https://bsc-dataseed.bnbchain.org",
                "https://bsc-dataseed.bnbchain.org?k=1",
            ),
            // A stop wait below the derived stop path.
            CONFIG.replace("STOP_TIMEOUT_MS=15000", "STOP_TIMEOUT_MS=1000"),
            CONFIG.replace("BSC_RUNTIME_RECONCILE_MS=500\n", ""),
            // C2: final only inside the reorg window; a slot no larger than
            // the table; every key required; watched accounts exact, once.
            CONFIG.replace("BSC_STATE_FINALITY_DEPTH=16", "BSC_STATE_FINALITY_DEPTH=65"),
            CONFIG.replace("BSC_STATE_MAX_TXS=262144", "BSC_STATE_MAX_TXS=1024")
                .replace("CANDIDATES_PER_NONCE=16", "CANDIDATES_PER_NONCE=1024")
                .replace("BSC_STATE_MAX_TXS=1024", "BSC_STATE_MAX_TXS=1023"),
            CONFIG.replace("BSC_STATE_CHECKPOINT_MS=30000\n", ""),
            CONFIG.replace("NONCE_FACT_BLOCKS=20", "NONCE_FACT_BLOCKS=0"),
            // C3: every PAPER key required, bounded; refused accounts exact.
            CONFIG.replace("BSC_EXEC_LANES=64\n", ""),
            CONFIG.replace("BSC_RUNTIME_MAX_LAUNCH_CALLDATA=16384\n", ""),
            CONFIG.replace("MAX_LAUNCH_CALLDATA=16384", "MAX_LAUNCH_CALLDATA=1023"),
            CONFIG.replace(REFUSED, ""),
            CONFIG.replace(REFUSED, "BSC_EXEC_REFUSED_ACCOUNTS=0x12\n"),
            // Gate M2: the paths that take the sender, required and exact.
            CONFIG.replace("BSC_EXEC_SENDER_PATHS=C3+STRATEGY_C\n", ""),
            CONFIG.replace("BSC_EXEC_SENDER_PATHS=C3+STRATEGY_C", "BSC_EXEC_SENDER_PATHS=C3+C2"),
            CONFIG.replace("BSC_EXEC_SLIPPAGE_BPS=100", "BSC_EXEC_SLIPPAGE_BPS=10000"),
            CONFIG.replace("BSC_EXEC_GAS_LIMIT=3000000", "BSC_EXEC_GAS_LIMIT=20999"),
            // R3: the operator exit switch, required and exact.
            CONFIG.replace("BSC_EXEC_LIVE_OPERATOR_EXIT=OFF\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_OPERATOR_EXIT=OFF", "BSC_EXEC_LIVE_OPERATOR_EXIT=on"),
            // R3b: the nonce burn switch, required and exact.
            CONFIG.replace("BSC_EXEC_LIVE_NONCE_BURN=OFF\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_NONCE_BURN=OFF", "BSC_EXEC_LIVE_NONCE_BURN=1"),
            // R4: the public-only switch and the grant's longest, required and exact.
            CONFIG.replace("BSC_EXEC_LIVE_PUBLIC_ONLY=OFF\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_PUBLIC_ONLY=OFF", "BSC_EXEC_LIVE_PUBLIC_ONLY=yes"),
            CONFIG.replace("BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS=0\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_SPEND_TOTAL_WEI=0\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_RISK_OUTLIER=OFF\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_RISK_OUTLIER=OFF", "BSC_EXEC_LIVE_RISK_OUTLIER=On"),
            CONFIG.replace("BSC_EXEC_LIVE_RISK_SOFT_BUY_SLIPPAGE_BPS=100\n", ""),
            CONFIG.replace(
                "BSC_EXEC_LIVE_RISK_SOFT_BUY_SLIPPAGE_BPS=100",
                "BSC_EXEC_LIVE_RISK_SOFT_BUY_SLIPPAGE_BPS=10000",
            ),
            CONFIG.replace("BSC_EXEC_LIVE_RISK_SOFT_EXIT_SLIPPAGE_BPS=100\n", ""),
            CONFIG.replace(
                "BSC_EXEC_LIVE_RISK_SOFT_EXIT_SLIPPAGE_BPS=100",
                "BSC_EXEC_LIVE_RISK_SOFT_EXIT_SLIPPAGE_BPS=-1",
            ),
            CONFIG.replace("BSC_EXEC_LIVE_RISK_SOFT_GAS_PRICE_WEI=50000000\n", ""),
            CONFIG.replace(
                "BSC_EXEC_LIVE_RISK_SOFT_GAS_PRICE_WEI=50000000",
                "BSC_EXEC_LIVE_RISK_SOFT_GAS_PRICE_WEI=0",
            ),
            CONFIG.replace(
                "BSC_EXEC_LIVE_SPEND_TOTAL_WEI=0",
                "BSC_EXEC_LIVE_SPEND_TOTAL_WEI=18446744073709551616",
            ),
            // R4: the C3 re-arm switch and the bound builder policy,
            // required and exact (a repository-relative path).
            CONFIG.replace("BSC_EXEC_LIVE_C3_REARM=OFF\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_C3_REARM=OFF", "BSC_EXEC_LIVE_C3_REARM=off"),
            CONFIG.replace(&format!("BSC_EXEC_LIVE_BUILDER_POLICY={POLICY}\n"), ""),
            CONFIG.replace(POLICY, ""),
            CONFIG.replace(POLICY, "/tmp/builder_policy.dbrec"),
            CONFIG.replace(POLICY, "bsc/../builder_policy.dbrec"),
            CONFIG.replace(POLICY, "./builder_policy.dbrec"),
            CONFIG.replace(
                "BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS=0",
                "BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS=31536000001",
            ),
            CONFIG.replace("BSC_STATE_WATCHED=\n", ""),
            CONFIG.replace("BSC_STATE_WATCHED=\n", "BSC_STATE_WATCHED=0x12\n"),
            CONFIG.replace(
                "BSC_STATE_WATCHED=\n",
                "BSC_STATE_WATCHED=00000000000000000000000000000000000000000001\n",
            ),
            CONFIG.replace(
                "BSC_STATE_WATCHED=\n",
                "BSC_STATE_WATCHED=0x000000000000000000000000000000000000000g\n",
            ),
            CONFIG.replace(
                "BSC_STATE_WATCHED=\n",
                "BSC_STATE_WATCHED=0x0000000000000000000000000000000000000001,0x0000000000000000000000000000000000000001\n",
            ),
            CONFIG.replace("MAX_WATCHED_SENDERS=8", "MAX_WATCHED_SENDERS=1").replace(
                "BSC_STATE_WATCHED=\n",
                "BSC_STATE_WATCHED=0x0000000000000000000000000000000000000001,0x0000000000000000000000000000000000000002\n",
            ),
        ] {
            assert!(RuntimeConfig::parse(&bad).is_err(), "{bad}");
        }
    }

    /// REV.1.0 R3: the operator's exit-now is held back unless the operator
    /// switched `BSC_EXEC_LIVE_OPERATOR_EXIT` ON (the repository's OFF).
    #[test]
    fn the_operator_exit_is_off_unless_the_operator_sets_it_on() {
        let exit = |text: &str| {
            RuntimeConfig::parse(text)
                .unwrap()
                .exec
                .unwrap()
                .exec
                .live_operator_exit
        };
        assert!(!exit(CONFIG));
        assert!(exit(&CONFIG.replace(
            "BSC_EXEC_LIVE_OPERATOR_EXIT=OFF",
            "BSC_EXEC_LIVE_OPERATOR_EXIT=ON"
        )));
        // R3b: the nonce burn likewise.
        let burn = |text: &str| {
            RuntimeConfig::parse(text)
                .unwrap()
                .exec
                .unwrap()
                .exec
                .live_nonce_burn
        };
        assert!(!burn(CONFIG));
        assert!(burn(&CONFIG.replace(
            "BSC_EXEC_LIVE_NONCE_BURN=OFF",
            "BSC_EXEC_LIVE_NONCE_BURN=ON"
        )));
        // R4: a public source alone triggers no C3 BUY unless ON; no
        // grant's duration is bounded unless the operator names one.
        let exec = |text: &str| RuntimeConfig::parse(text).unwrap().exec.unwrap().exec;
        assert!(!exec(CONFIG).live_public_only);
        assert!(
            exec(&CONFIG.replace(
                "BSC_EXEC_LIVE_PUBLIC_ONLY=OFF",
                "BSC_EXEC_LIVE_PUBLIC_ONLY=ON"
            ))
            .live_public_only
        );
        assert_eq!(exec(CONFIG).live_grant_max_duration_ms, 0);
        // The campaign's budget: none in the repository (no BUY).
        assert_eq!(exec(CONFIG).live_spend_total_wei, 0);
        // R4 (BSC3-069): the soft bounds are the qualified values; an
        // outlier never arms in the repository.
        let x = exec(CONFIG);
        assert_eq!(
            (
                x.live_risk_soft_buy_slippage_bps,
                x.live_risk_soft_exit_slippage_bps,
                x.live_risk_soft_gas_price_wei,
                x.live_risk_outlier
            ),
            (100, 100, 50_000_000, false)
        );
        // R4: one C3 BUY per arm; the frozen import: no BUY.
        assert_eq!(
            (x.live_c3_rearm, x.live_builder_policy.as_str()),
            (false, POLICY)
        );
        assert_eq!(
            exec(&CONFIG.replace(
                "BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS=0",
                "BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS=3600000"
            ))
            .live_grant_max_duration_ms,
            3_600_000
        );
    }

    /// REV.1.0 R3: a lot the operator resumed is stranded no more and is
    /// read again at once (no backoff, no refused answer counted from
    /// before); another lot keeps its state.
    #[test]
    fn a_resumed_lot_is_read_again_at_once() {
        let config = RuntimeConfig::parse(CONFIG).unwrap().canonical.unwrap();
        let mut lot = LotDeciders::new(&config);
        let (a, b) = ([0xA1; 20], [0xB1; 20]);
        for t in [a, b] {
            lot.stranded
                .push((t, crate::sources::LIVE_POSITION_UNREADABLE));
            lot.retry_later(t);
            lot.refused_in_a_row(t, true);
        }
        lot.resume(a);
        assert_eq!(
            lot.stranded,
            vec![(b, crate::sources::LIVE_POSITION_UNREADABLE)]
        );
        assert!(lot.retry_at.iter().all(|(t, _)| *t == b));
        assert_eq!(lot.refused, vec![(b, 1)]);
    }

    /// Phase E review L3: while the drift guard says DRIFT, a Four launch the
    /// follower matched (or mismatched) against the frozen derivation counts
    /// as UNKNOWN: the freeze proves nothing about a drifted deployment.
    #[test]
    fn a_drifted_four_deployment_proves_no_token() {
        use crate::chain_follower::{CanonicalLaunch, FourToken, Venue};
        const BLOCK_MS: u64 = 1_792_000_000_000;
        let base =
            std::env::temp_dir().join(format!("bsc-runtime-four-drift-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let config = RuntimeConfig::parse(CONFIG).unwrap();
        let cc = config.canonical.clone().unwrap();
        let mut c = Canonical::new(&cc, BLOCK_MS);
        assert_eq!(c.four_deployment, "UNPROBED");
        let mut journal = RuntimeJournal::open(
            &base,
            1,
            1,
            JournalSetup::whole(64, journal_max_payload(&config)),
        )
        .unwrap();
        let core = IngressCore::new(64, config.max_launch_calldata);
        let block = CanonicalEvent::Block {
            number: 10,
            hash: [10; 32],
            parent_hash: [9; 32],
            timestamp_s: BLOCK_MS / 1_000,
            timestamp_ms: BLOCK_MS,
            txs: Vec::new(),
            observed: Instant::now(),
            observed_unix_ms: Some(BLOCK_MS + 300),
        };
        let four = |tx: u8| {
            CanonicalEvent::Launch(CanonicalLaunch {
                venue: Venue::Four,
                block_number: 10,
                block_hash: [10; 32],
                tx_index: u64::from(tx),
                tx_hash: [tx; 32],
                token: Some([tx; 20]),
                creator: None,
                external_buys_after: None,
                first_external_buy_index: None,
                sender: Some([tx; 20]),
                nonce: Some(0),
                c1_scope: false,
                venue_target: true,
                four_token: FourToken::Matched,
                unverifiable: None,
            })
        };
        c.event(10, block, &mut journal);
        c.event(10, four(1), &mut journal);
        c.four_drift = true;
        c.event(10, four(2), &mut journal);
        c.join(u64::MAX, &core, &mut journal);
        let k = c.ledger.counters();
        assert_eq!(
            (
                k.four_launches,
                k.four_pending_decoded,
                k.four_pending_unknown
            ),
            (2, 1, 1)
        );
        let _ = journal.finish();
        fs::remove_dir_all(&base).unwrap();
    }

    /// BSC3-050: the gate's view of a reconcile carries the one LIVE
    /// qualification verified for it (the LIVE path ticks with the same);
    /// without a LIVE path it reads none.
    #[test]
    fn the_reconcile_reads_its_one_verified_qualification() {
        let dir = std::env::temp_dir().join(format!("bsc-runtime-q-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let q = crate::sources::LiveQualification {
            venues: vec![crate::sources::VenueId::Flap],
            acquisition: true,
            ..Default::default()
        };
        let g = Generation::initial();
        assert_eq!(LiveView::read(&g, &dir, Some(&q), 1, None).qualification, q);
        assert_eq!(
            LiveView::read(&g, &dir, None, 1, None).qualification,
            crate::sources::LiveQualification::default()
        );
    }

    /// BSC3-014/038/016: a canonical launch of a block that is not the last
    /// one reported is counted and journaled as received (the census shows
    /// it unjoined), never joined; Four.meme launches are counted by
    /// visibility class; the status names the last census refresh.
    #[test]
    fn a_launch_without_its_head_and_four_classes_are_counted() {
        use crate::chain_follower::{CanonicalLaunch, Venue};
        const BLOCK_MS: u64 = 1_792_000_000_000;
        let base =
            std::env::temp_dir().join(format!("bsc-runtime-headless-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let config = RuntimeConfig::parse(CONFIG).unwrap();
        let cc = config.canonical.clone().unwrap();
        let block_ms = |n: u8| BLOCK_MS + u64::from(n) * 450;
        let mut c = Canonical::new(&cc, block_ms(10));
        let mut journal = RuntimeJournal::open(
            &base,
            1,
            1,
            JournalSetup::whole(64, journal_max_payload(&config)),
        )
        .unwrap();
        let core = IngressCore::new(64, config.max_launch_calldata);
        let clock = Clock(Instant::now());
        let block = |n: u8| CanonicalEvent::Block {
            number: u64::from(n),
            hash: [n; 32],
            parent_hash: [n - 1; 32],
            timestamp_s: block_ms(n) / 1_000,
            timestamp_ms: block_ms(n),
            txs: Vec::new(),
            observed: clock.0,
            observed_unix_ms: Some(block_ms(n) + 300),
        };
        let launch = |n: u8, tx: u8, venue| {
            CanonicalEvent::Launch(CanonicalLaunch {
                venue,
                block_number: u64::from(n),
                block_hash: [n; 32],
                tx_index: 0,
                tx_hash: [tx; 32],
                token: Some([tx; 20]),
                creator: None,
                external_buys_after: Some(0),
                first_external_buy_index: None,
                sender: Some([tx; 20]),
                nonce: Some(u64::from(tx)),
                c1_scope: false,
                venue_target: false,
                four_token: crate::chain_follower::FourToken::NotApplicable,
                unverifiable: None,
            })
        };
        c.event(10, block(10), &mut journal);
        c.event(11, block(11), &mut journal);
        // Block 10's launch after block 11 was reported: no head to join it.
        c.event(11, launch(10, 7, Venue::Flap), &mut journal);
        assert_eq!((c.launch_head_missing, c.ledger.waiting()), (1, 0));
        // A Four launch never seen: missed, as a Four class.
        let CanonicalEvent::Launch(mut four) = launch(11, 8, Venue::Four) else {
            unreachable!()
        };
        four.four_token = crate::chain_follower::FourToken::NotRecognized;
        c.event(11, CanonicalEvent::Launch(four), &mut journal);
        c.join(u64::MAX, &core, &mut journal);
        let k = c.ledger.counters();
        assert_eq!(
            (
                k.four_launches,
                k.four_missed,
                k.four_seen_full_before,
                k.missed
            ),
            (1, 1, 0, 0)
        );
        let refresh = {
            let mut r = CensusRefresh::default();
            r.ended(Ok(Err("disk full".into())));
            assert_eq!(r.state.as_deref(), Some("FAILED: disk full"));
            r.ended(Ok(Ok(42)));
            r
        };
        let status = status_bytes(
            &Generation::initial(),
            &[],
            clock,
            1,
            1,
            0,
            "j",
            0,
            0,
            None,
            Some(&c),
            None,
            None,
            None,
            config.status_ms,
            &refresh,
        )
        .unwrap();
        let r = darkbot_bsc_record::decode(&rs::SCHEMA, &status).unwrap();
        assert_eq!(r.text(rs::CENSUS_STATE).unwrap(), "OK");
        // BSC3-015: an unreadable clock is expired time, and counted.
        let before = r.u64(rs::CLOCK_UNREADABLE).unwrap();
        assert_eq!(wall_or_expired(Err("no clock".into())), u64::MAX);
        assert_eq!(wall_or_expired(Ok(7)), 7);
        assert!(CLOCK_UNREADABLE.load(std::sync::atomic::Ordering::Relaxed) > before);
        assert_eq!(r.u64(rs::CENSUS_WRITTEN_UNIX_MS).unwrap(), 42);
        let k = r.opt_nested(rs::CANONICAL).unwrap().unwrap();
        assert_eq!(k.u64(rs::canonical::LAUNCH_HEAD_MISSING).unwrap(), 1);
        assert_eq!(k.u64(rs::canonical::FOUR_MISSED).unwrap(), 1);
        journal.finish().unwrap();
        let census = census(&base, journal_max_payload(&config), None).unwrap();
        assert_eq!(census.unjoined, 1);
        fs::remove_dir_all(base).unwrap();
    }

    /// BSC3-036: a failover makes the next endpoint the one the status
    /// names and is counted; a stalled step is counted apart from the RPC
    /// errors and shows as STALLED.
    #[test]
    fn a_failover_and_a_stall_reach_the_status() {
        let base =
            std::env::temp_dir().join(format!("bsc-runtime-failover-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let config = RuntimeConfig::parse(CONFIG).unwrap();
        let cc = config.canonical.clone().unwrap();
        let mut c = Canonical::new(&cc, 0);
        let mut journal = RuntimeJournal::open(
            &base,
            1,
            1,
            JournalSetup::whole(64, journal_max_payload(&config)),
        )
        .unwrap();
        let status = |c: &Canonical| {
            let bytes = status_bytes(
                &Generation::initial(),
                &[],
                Clock(Instant::now()),
                1,
                1,
                0,
                "j",
                0,
                0,
                None,
                Some(c),
                None,
                None,
                None,
                config.status_ms,
                &CensusRefresh::default(),
            )
            .unwrap();
            let r = darkbot_bsc_record::decode(&rs::SCHEMA, &bytes).unwrap();
            let k = r.opt_nested(rs::CANONICAL).unwrap().unwrap();
            (
                k.text(rs::canonical::ENDPOINT).unwrap().to_string(),
                k.u64(rs::canonical::FAILOVERS).unwrap(),
                k.u64(rs::canonical::STALLS).unwrap(),
                k.u64(rs::canonical::RPC_ERRORS).unwrap(),
                k.text(rs::canonical::STATE).unwrap().to_string(),
            )
        };
        assert_eq!(
            status(&c),
            (
                "bsc-dataseed.bnbchain.org".into(),
                0,
                0,
                0,
                "CONNECTING".into()
            )
        );
        c.event(
            1,
            CanonicalEvent::Fault(FollowerFault::Stalled {
                head: 10,
                for_ms: 5_000,
            }),
            &mut journal,
        );
        c.event(
            2,
            CanonicalEvent::Failover {
                from: 0,
                to: 1,
                failed_steps: 3,
            },
            &mut journal,
        );
        assert_eq!(
            status(&c),
            ("bsc-dataseed1.defibit.io".into(), 1, 1, 0, "STALLED".into())
        );
        // BSC3-056: the second source: STALE before its first header, AGREED
        // once it holds the followed head's hash, DISAGREES on another.
        let confirm = |c: &Canonical| {
            let bytes = status_bytes(
                &Generation::initial(),
                &[],
                Clock(Instant::now()),
                1,
                1,
                0,
                "j",
                0,
                0,
                None,
                Some(c),
                None,
                None,
                None,
                config.status_ms,
                &CensusRefresh::default(),
            )
            .unwrap();
            let r = darkbot_bsc_record::decode(&rs::SCHEMA, &bytes).unwrap();
            let k = r.opt_nested(rs::CANONICAL).unwrap().unwrap();
            (
                k.opt_text(rs::canonical::CONFIRM_ENDPOINT)
                    .unwrap()
                    .map(str::to_string),
                k.opt_u64(rs::canonical::CONFIRM_HEAD).unwrap(),
                k.opt_text(rs::canonical::CONFIRM_STATE)
                    .unwrap()
                    .map(str::to_string),
            )
        };
        let host = Some("bsc-rpc.publicnode.com".to_string());
        assert_eq!(confirm(&c), (host.clone(), None, Some("STALE".into())));
        let now = observed_now().unwrap();
        c.event(
            3,
            CanonicalEvent::Block {
                number: 10,
                hash: [10; 32],
                parent_hash: [9; 32],
                timestamp_s: now / 1_000,
                timestamp_ms: now,
                txs: Vec::new(),
                observed: Instant::now(),
                observed_unix_ms: Some(now),
            },
            &mut journal,
        );
        let header = |hash: [u8; 32]| crate::chain_follower::ConfirmEvent::Header {
            number: 10,
            hash,
            parent_hash: [9; 32],
            block_ms: now,
        };
        c.confirm_event(header([10; 32]));
        assert_eq!(confirm(&c), (host.clone(), Some(10), Some("AGREED".into())));
        c.confirm_event(header([0xee; 32]));
        assert_eq!(confirm(&c), (host, Some(10), Some("DISAGREES".into())));
        // Without a second source: none of it.
        let mut none = cc.clone();
        none.confirm = None;
        assert_eq!(confirm(&Canonical::new(&none, 0)), (None, None, None));
        journal.finish().unwrap();
        fs::remove_dir_all(base).unwrap();
    }

    /// BSC3-062: a launch whose sighting the bounded ingress window forgot
    /// before its join is EVICTED (not evaluable) in the counters and the
    /// status, never MISSED; a launch never seen, with nothing forgotten
    /// that could have been it, stays MISSED.
    #[test]
    fn a_forgotten_sighting_is_evicted_not_missed() {
        use crate::chain_follower::{CanonicalLaunch, Venue};
        const BLOCK_MS: u64 = 1_792_000_000_000;
        let base = std::env::temp_dir().join(format!("bsc-runtime-evict-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let config = RuntimeConfig::parse(CONFIG).unwrap();
        let cc = config.canonical.clone().unwrap();
        assert!(cc.join_horizon_ms > 0);
        let mut c = Canonical::new(&cc, BLOCK_MS);
        let mut journal = RuntimeJournal::open(
            &base,
            1,
            1,
            JournalSetup::whole(64, journal_max_payload(&config)),
        )
        .unwrap();
        // A window of two sightings: launch 1's hash, seen 50 ms before its
        // block, leaves it before the join.
        let mut core = IngressCore::new(2, config.max_launch_calldata);
        for (n, hash) in [[1u8; 32], [7; 32], [8; 32]].into_iter().enumerate() {
            core.observe(
                SourceId::PublicNode,
                Observation::HashOnly { hash },
                n as u64,
                Some((BLOCK_MS - 50) * 1_000),
            );
        }
        assert_eq!(core.sighting(&[1; 32]), None);
        let clock = Clock(Instant::now());
        c.event(
            1,
            CanonicalEvent::Block {
                number: 10,
                hash: [10; 32],
                parent_hash: [9; 32],
                timestamp_s: BLOCK_MS / 1_000,
                timestamp_ms: BLOCK_MS,
                txs: Vec::new(),
                observed: clock.0,
                observed_unix_ms: Some(BLOCK_MS + 300),
            },
            &mut journal,
        );
        for tx in [1u8, 2] {
            c.event(
                1,
                CanonicalEvent::Launch(CanonicalLaunch {
                    venue: Venue::Flap,
                    block_number: 10,
                    block_hash: [10; 32],
                    tx_index: u64::from(tx),
                    tx_hash: [tx; 32],
                    token: Some([tx; 20]),
                    creator: None,
                    external_buys_after: Some(0),
                    first_external_buy_index: None,
                    sender: Some([tx; 20]),
                    nonce: Some(u64::from(tx)),
                    c1_scope: true,
                    venue_target: true,
                    four_token: crate::chain_follower::FourToken::NotApplicable,
                    unverifiable: None,
                }),
                &mut journal,
            );
        }
        c.join(u64::MAX, &core, &mut journal);
        let k = c.ledger.counters();
        assert_eq!((k.flap_launches, k.evicted, k.missed), (2, 1, 1));
        let status = status_bytes(
            &Generation::initial(),
            &[],
            clock,
            1,
            1,
            0,
            "j",
            0,
            0,
            None,
            Some(&c),
            None,
            None,
            None,
            config.status_ms,
            &CensusRefresh::default(),
        )
        .unwrap();
        let r = darkbot_bsc_record::decode(&rs::SCHEMA, &status).unwrap();
        let k = r.opt_nested(rs::CANONICAL).unwrap().unwrap();
        assert_eq!(
            (
                k.u64(rs::canonical::EVICTED).unwrap(),
                k.u64(rs::canonical::MISSED).unwrap(),
                k.u64(rs::canonical::FOUR_EVICTED).unwrap()
            ),
            (1, 1, 0)
        );
        journal.finish().unwrap();
        fs::remove_dir_all(base).unwrap();
    }

    /// The follower's events, joined with the sightings, reach the status
    /// counters, the journal and the census alike; a reorg takes its
    /// launches out of all three.
    #[test]
    fn canonical_events_join_into_status_journal_and_census() {
        use crate::chain_follower::{CanonicalLaunch, Venue};
        use crate::wss::ProtocolFault;
        const BLOCK_MS: u64 = 1_792_000_000_000;
        let base = std::env::temp_dir().join(format!("bsc-runtime-canon-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let config = RuntimeConfig::parse(CONFIG).unwrap();
        let cc = config.canonical.clone().unwrap();
        let block_ms = |n: u8| BLOCK_MS + u64::from(n) * 450;
        // The process started at block 10's time.
        let mut c = Canonical::new(&cc, block_ms(10));
        let mut journal = RuntimeJournal::open(
            &base,
            1,
            1,
            JournalSetup::whole(64, journal_max_payload(&config)),
        )
        .unwrap();
        let mut core = IngressCore::new(64, config.max_launch_calldata);
        let clock = Clock(Instant::now());
        let block = |n: u8| CanonicalEvent::Block {
            number: u64::from(n),
            hash: [n; 32],
            parent_hash: [n - 1; 32],
            timestamp_s: block_ms(n) / 1_000,
            timestamp_ms: block_ms(n),
            txs: Vec::new(),
            observed: clock.0,
            // The follower held the header 300 ms after the block time.
            observed_unix_ms: Some(block_ms(n) + 300),
        };
        let launch = |n: u8, tx: u8, buys, c1_scope| {
            CanonicalEvent::Launch(CanonicalLaunch {
                venue: Venue::Flap,
                block_number: u64::from(n),
                block_hash: [n; 32],
                tx_index: 0,
                tx_hash: [tx; 32],
                token: Some([tx; 20]),
                creator: None,
                external_buys_after: buys,
                first_external_buy_index: None,
                sender: Some([tx; 20]),
                nonce: Some(u64::from(tx)),
                c1_scope,
                venue_target: c1_scope,
                four_token: crate::chain_follower::FourToken::NotApplicable,
                unverifiable: None,
            })
        };
        // Launch 1: a hash 50 ms before its block time; launch 2: a full
        // sighting 100 ms after its block time but before the header fetch
        // (counted after, never before); launch 3: unseen.
        core.observe(
            SourceId::PublicNode,
            Observation::HashOnly { hash: [1; 32] },
            5,
            Some((block_ms(10) - 50) * 1_000),
        );
        core.observe(
            SourceId::PublicNode,
            Observation::HashOnly { hash: [2; 32] },
            6,
            Some((block_ms(11) + 100) * 1_000),
        );
        journal.record(
            RecordKind::Runtime,
            1,
            source_event_payload(SourceId::PublicNode, SourceEvent::Connected),
        );
        // Mined before the process started (a C2 resume backfill): its
        // launch, the gap before it and the block are outside the window.
        c.event(9, CanonicalEvent::Gap { from: 5, to: 8 }, &mut journal);
        c.event(9, block(9), &mut journal);
        c.event(9, launch(9, 9, Some(0), true), &mut journal);
        assert_eq!(
            (
                c.backfill_blocks,
                c.backfill_launches,
                c.blocks,
                c.resyncs,
                c.blocks_skipped
            ),
            (1, 1, 0, 0, 0)
        );
        assert_eq!(c.ledger.waiting(), 0);
        c.event(10, block(10), &mut journal);
        c.event(10, launch(10, 1, Some(2), true), &mut journal);
        // A launch whose transaction the verifier refuses (e.g. a type it
        // does not rebuild): a counted per-launch fact, never a stall.
        let CanonicalEvent::Launch(mut refused) = launch(10, 4, Some(0), false) else {
            unreachable!()
        };
        refused.sender = None;
        refused.nonce = None;
        refused.unverifiable = Some(crate::wss::RejectReason::UnsupportedType(4));
        c.event(10, CanonicalEvent::Launch(refused), &mut journal);
        // A Four launch whose provider answers were refused every time:
        // counted for every venue.
        let CanonicalEvent::Launch(mut four) = launch(10, 5, None, false) else {
            unreachable!()
        };
        four.venue = Venue::Four;
        four.sender = None;
        four.nonce = None;
        four.unverifiable = Some(crate::wss::RejectReason::HashMismatch);
        // As the follower decides it: refused bytes are no decoded launch.
        four.four_token = crate::chain_follower::FourToken::NotRecognized;
        c.event(10, CanonicalEvent::Launch(four), &mut journal);
        c.event(20, block(11), &mut journal);
        c.event(20, launch(11, 2, None, false), &mut journal);
        c.event(20, launch(11, 3, Some(0), true), &mut journal);
        assert_eq!(c.ledger.waiting(), 5);
        c.event(
            25,
            CanonicalEvent::Fault(FollowerFault::Request {
                call: crate::chain_follower::Call::BlockNumber,
                error: RequestError::Transport(DisconnectReason::Protocol(
                    ProtocolFault::MalformedJson,
                )),
            }),
            &mut journal,
        );
        assert_eq!(c.state, "FAILING");
        c.event(30, block(12), &mut journal);
        // Not yet due (join delay), then due.
        c.join(30, &core, &mut journal);
        assert_eq!(c.ledger.counters().flap_launches, 0);
        c.join(u64::MAX, &core, &mut journal);
        let k = c.ledger.counters();
        assert_eq!(
            (
                k.flap_launches,
                k.seen_hash_only_before,
                k.seen_after,
                k.missed,
                k.external_buys,
                k.buys_unknown,
                k.c1_scope_launches,
                k.tx_unverifiable,
                k.tx_answer_refused,
                k.four_launches
            ),
            (4, 1, 1, 2, 2, 1, 2, 1, 1, 1)
        );
        c.event(
            40,
            CanonicalEvent::Reorg {
                orphaned: vec![(11, [11; 32]), (12, [12; 32])],
                new_head: (12, [0xEE; 32]),
            },
            &mut journal,
        );
        c.event(50, CanonicalEvent::Gap { from: 13, to: 99 }, &mut journal);
        let k = c.ledger.counters();
        assert_eq!(
            (
                k.flap_launches,
                k.missed,
                k.orphaned_launches,
                k.c1_scope_launches,
                k.tx_unverifiable
            ),
            (2, 1, 2, 1, 1)
        );
        assert_eq!(
            (
                c.state,
                c.blocks,
                c.reorgs,
                c.resyncs,
                c.rpc_errors,
                c.blocks_skipped
            ),
            ("FOLLOWING", 3, 1, 1, 1, 87)
        );
        let status = status_bytes(
            &Generation::initial(),
            &[],
            clock,
            1,
            1,
            0,
            "j",
            0,
            0,
            Some(&CaptureStatus {
                budget_max: 64,
                budget_peak: 9,
                ..CaptureStatus::default()
            }),
            Some(&c),
            None,
            None,
            None,
            config.status_ms,
            &CensusRefresh::default(),
        )
        .unwrap();
        let r = darkbot_bsc_record::decode(&rs::SCHEMA, &status).unwrap();
        assert!(
            r.opt_nested(rs::STATE).unwrap().is_none(),
            "no C2 owner, no state record"
        );
        assert_eq!(r.u64(rs::STATUS_INTERVAL_MS).unwrap(), config.status_ms);
        // Phase F: the capture's storage health rides along.
        let k = r.nested(rs::CAPTURE).unwrap();
        assert_eq!(
            (
                k.u64(rs::capture::BUDGET_MAX_BYTES).unwrap(),
                k.u64(rs::capture::BUDGET_PEAK_BYTES).unwrap(),
                k.text(rs::capture::STATE).unwrap()
            ),
            (64, 9, "HEALTHY")
        );
        assert_eq!(k.opt_u64(rs::capture::DURABLE_SEQUENCE).unwrap(), None);
        let n = r.nested(rs::CANONICAL).unwrap();
        assert_eq!(n.u64(rs::canonical::FLAP_LAUNCHES).unwrap(), 2);
        assert_eq!(n.u64(rs::canonical::TX_UNVERIFIABLE).unwrap(), 1);
        assert_eq!(n.u64(rs::canonical::TX_ANSWER_REFUSED).unwrap(), 1);
        assert_eq!(n.u64(rs::canonical::HEAD).unwrap(), 12);
        assert_eq!(n.u64(rs::canonical::BLOCKS_SKIPPED).unwrap(), 87);
        assert_eq!(n.u64(rs::canonical::C1_SCOPE_LAUNCHES).unwrap(), 1);
        assert_eq!(n.i64(rs::canonical::HEAD_FETCH_LAG_MS).unwrap(), 300);
        assert_eq!(n.u64(rs::canonical::BACKFILL_BLOCKS).unwrap(), 1);
        assert_eq!(n.u64(rs::canonical::BACKFILL_LAUNCHES).unwrap(), 1);
        journal.finish().unwrap();
        let census = census(&base, journal_max_payload(&config), None).unwrap();
        assert_eq!(
            (
                census.flap_launches,
                census.hash_only_before,
                census.orphaned_launches,
                census.c1_scope_launches,
                census.unjoined,
                census.blocks_skipped,
                census.tx_unverifiable,
                census.tx_answer_refused,
                census.four_launches
            ),
            (2, 1, 2, 1, 0, 87, 1, 1, 1)
        );
        let w = &census.windows[0];
        assert_eq!((w.first_block, w.reorgs, w.resyncs), (Some(10), 1, 1));
        // No clean end record in this unit test: a killed process's window.
        assert_eq!(w.state, "UNTERMINATED");
        assert_eq!(census.fetch_lag_ms_min, Some(300));
        fs::remove_dir_all(&base).unwrap();
    }

    /// A too-deep reorg or a gap reaching below the window (a resume
    /// backfill) counts only the window's heights as skipped (review 4
    /// LOWs: before, 9 was counted although it precedes the window).
    #[test]
    fn a_too_deep_reorg_never_counts_heights_before_the_window() {
        const BLOCK_MS: u64 = 1_792_000_000_000;
        let base = std::env::temp_dir().join(format!("bsc-runtime-deep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let config = RuntimeConfig::parse(CONFIG).unwrap();
        let mut c = Canonical::new(config.canonical.as_ref().unwrap(), BLOCK_MS + 10);
        let mut journal = RuntimeJournal::open(
            &base,
            1,
            1,
            JournalSetup::whole(64, journal_max_payload(&config)),
        )
        .unwrap();
        let block = |n: u64| CanonicalEvent::Block {
            number: n,
            hash: [n as u8; 32],
            parent_hash: [n as u8 - 1; 32],
            timestamp_s: (BLOCK_MS + n) / 1_000,
            timestamp_ms: BLOCK_MS + n,
            txs: Vec::new(),
            observed: Instant::now(),
            observed_unix_ms: None,
        };
        c.event(1, block(9), &mut journal);
        c.event(2, block(10), &mut journal);
        c.event(3, block(11), &mut journal);
        assert_eq!(
            (c.backfill_blocks, c.blocks, c.window_first_block),
            (1, 2, 10)
        );
        c.event(
            4,
            CanonicalEvent::ReorgTooDeep {
                orphaned: vec![(9, [9; 32]), (10, [10; 32]), (11, [11; 32])],
                new_head: (14, [0xEE; 32]),
            },
            &mut journal,
        );
        // Heights 10..=13, not 9..=13.
        assert_eq!((c.resyncs, c.blocks_skipped), (1, 4));
        // A gap after a failed reorg can start below the window (the
        // follower restarts at the fork point): 9 is not counted either.
        c.event(5, CanonicalEvent::Gap { from: 9, to: 13 }, &mut journal);
        assert_eq!((c.resyncs, c.blocks_skipped), (2, 8));
        journal.finish().unwrap();
        fs::remove_dir_all(&base).unwrap();
    }

    /// BSC3-039: a due checkpoint is handed to the writer, the loop never
    /// waits for its write (it was written here, 10–34 ms measured for
    /// 479 KB, every 30 s); the next pass counts it; at the stop the loop
    /// writes the last window itself, after the writer's and never beside
    /// it.
    #[test]
    fn the_loop_hands_its_checkpoint_over_and_writes_only_the_last() {
        use darkbot_bsc_trading::chain_state::engine::StateLimits;
        let dir = std::env::temp_dir().join(format!("bsc-runtime-ckpt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c2-checkpoint.bin");
        let config = StateConfig {
            limits: StateLimits {
                max_txs: 1_024,
                max_candidates_per_nonce: 4,
                max_watched_senders: 4,
                reorg_window: 8,
                finality_depth: 2,
            },
            watched: Vec::new(),
            checkpoint_ms: 1_000,
            nonce_fact_blocks: 1,
        };
        let mut o = StateOwner::fresh(&config, path.clone()).unwrap();
        static WRITING: AtomicBool = AtomicBool::new(false);
        static OVERLAPPED: AtomicBool = AtomicBool::new(false);
        fn slow(path: &Path, bytes: &[u8]) -> Result<(), String> {
            if WRITING.swap(true, Ordering::SeqCst) {
                OVERLAPPED.store(true, Ordering::SeqCst);
            }
            thread::sleep(Duration::from_millis(600));
            let written = replace(path, bytes);
            WRITING.store(false, Ordering::SeqCst);
            written
        }
        let writer = CheckpointWriter::spawn(path.clone(), Arc::new(slow)).unwrap();
        let t = Instant::now();
        checkpoint_pass(&mut o, &writer, true, false, slow);
        // A liveness margin, not a timing claim: the write takes 600 ms.
        assert!(
            t.elapsed() < Duration::from_millis(400),
            "{:?}",
            t.elapsed()
        );
        assert_eq!(o.checkpoint_counts(), (0, 0));
        // Passes not due hand nothing over; one of them counts the write.
        let counted = Instant::now();
        while o.checkpoint_counts().0 == 0 {
            assert!(
                counted.elapsed() < Duration::from_secs(120),
                "never counted"
            );
            checkpoint_pass(&mut o, &writer, false, false, slow);
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(o.checkpoint_counts(), (1, 0));
        assert_eq!(fs::read(&path).unwrap(), o.checkpoint_bytes());
        // The stop: waits for the writer's write, then writes the last
        // window itself.
        checkpoint_pass(&mut o, &writer, true, false, slow);
        checkpoint_pass(&mut o, &writer, true, true, slow);
        assert_eq!(o.checkpoint_counts(), (3, 0));
        assert!(!OVERLAPPED.load(Ordering::SeqCst), "two writes at once");
        assert!(!dir.join("c2-checkpoint.tmp").exists());
        drop(writer);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// BSC3-039: a durable replacement is flushed to the device, the
    /// status's is not; both publish the whole new content.
    #[test]
    fn only_durable_records_are_flushed() {
        let dir = std::env::temp_dir().join(format!("bsc-runtime-flush-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("record.dbrec");
        use darkbot_bsc_journal::durable::{Op, hooks};
        let before = FLUSHED.with(std::cell::Cell::get);
        hooks::record();
        replace_volatile(&path, b"status").unwrap();
        assert_eq!(FLUSHED.with(std::cell::Cell::get), before);
        assert_eq!(hooks::take(), []);
        assert_eq!(fs::read(&path).unwrap(), b"status");
        replace(&path, b"census").unwrap();
        assert_eq!(FLUSHED.with(std::cell::Cell::get), before + 1);
        // Its rename too, on the medium (BSC3-039 review).
        assert_eq!(hooks::take(), [Op::Fsync, Op::FullFlush]);
        assert_eq!(fs::read(&path).unwrap(), b"census");
        assert!(!dir.join("record.tmp").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// With every source OFF the runtime connects nothing, writes its
    /// status (all OFF, READ_ONLY) and stops on the stop file with a
    /// committed journal.
    #[test]
    fn an_idle_runtime_writes_its_status_and_stops_cleanly() {
        let base = std::env::temp_dir().join(format!("bsc-runtime-idle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        // No canonical endpoint: this test opens no connection at all.
        let text: String = CONFIG
            .replace("STATUS_MS=1000", "STATUS_MS=100")
            .lines()
            .filter(|l| {
                !l.starts_with("BSC_CANONICAL_")
                    && !l.starts_with("BSC_STATE_")
                    && !l.starts_with("BSC_EXEC_")
            })
            .map(|l| format!("{l}\n"))
            .collect();
        let config = RuntimeConfig::parse(&text).unwrap();
        let sources = base.join("bsc-sources");
        Store::open(&sources, 8).unwrap().init(1).unwrap();
        let rundir = base.clone();
        // BSC3-039: the loop writes its status every interval, never
        // flushing it to the device (counted on the loop's own thread).
        let runner = thread::spawn(move || {
            let ran = run(&config, &sources, &rundir, Path::new("/nonexistent-repo"));
            (ran, FLUSHED.with(std::cell::Cell::get))
        });
        let status = base.join("bsc-runtime/status.dbrec");
        let started = Instant::now();
        // A liveness bound, not a timing claim: a loaded host starts slowly.
        while !status.exists() {
            assert!(started.elapsed() < Duration::from_secs(120), "no status");
            thread::sleep(Duration::from_millis(20));
        }
        fs::write(base.join("bsc.runtime.stop"), b"stop\n").unwrap();
        let (ran, flushed) = runner.join().unwrap();
        ran.unwrap();
        // The census at the stop is durable; the status, written every
        // interval, never is.
        assert_eq!(flushed, 1, "the loop flushes only its final census");
        // Paket 8 review L7: the stop is consumed; the next start runs.
        assert!(!base.join("bsc.runtime.stop").exists());
        let r = darkbot_bsc_record::decode(&rs::SCHEMA, &fs::read(&status).unwrap()).unwrap();
        assert_eq!(r.text(rs::MODE).unwrap(), "READ_ONLY");
        let rows = r.nested_all(rs::SOURCES).unwrap();
        assert_eq!(rows.len(), 5);
        assert!(
            rows.iter()
                .all(|row| row.text(rs::source::HEALTH).unwrap() == "OFF")
        );
        let journals: Vec<_> = fs::read_dir(base.join("bsc-runtime/journal"))
            .unwrap()
            .collect();
        assert_eq!(journals.len(), 1);
        fs::remove_dir_all(&base).unwrap();
    }
}

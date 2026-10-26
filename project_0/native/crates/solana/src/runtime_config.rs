//! Shared typed operational configuration. It cannot authorize money or submission.
use serde::{Deserialize, Serialize};

use std::{collections::BTreeMap, path::Path, sync::OnceLock};

/// D11: defaults the runtime's own typed configurations share with the registry below (one
/// authority: `MarketConfig`, `EnrichConfig`, `EconConfig` reference these, never a second
/// literal).
pub mod defaults {
    pub const INGRESS_QUEUE: u64 = 65_536;
    /// D11: the owner's recovery waits for the finalized-root catch-up before Snapshot B (the
    /// 2026-10-10 smoke exceeded 30 s; the wait never skips reconciliation), the real root stall
    /// and the absolute phase cap — the one default of every launcher (`P0_RECOVERY_ROOT_*`).
    pub const RECOVERY_ROOT_CATCHUP_SECS: u64 = 120;
    pub const RECOVERY_ROOT_STALL_SECS: u64 = 60;
    pub const RECOVERY_ROOT_HARD_MAX_SECS: u64 = 300;
    pub const STREAM_READY_SECONDS: u64 = 600;
    pub const MARKET_DRAIN_SECONDS: u64 = 120;
    pub const ENRICHMENT_SECONDS: u64 = 900;
    pub const MARKET_HEARTBEAT_SECONDS: u64 = 30;
    pub const MARKET_MIN_SAMPLE: u64 = 30;
    pub const RETENTION_ROWS: u64 = 262_144;
    pub const RETENTION_ROWS_MAX: u64 = 2_000_000;
    pub const ECON_QUEUE_CAPACITY: u64 = 8_192;
    pub const ECON_QUEUE_CAPACITY_MAX: u64 = 1_000_000;
    pub const ENRICH_MAX_PENDING: u64 = 4_096;
    pub const ENRICH_MAX_ATTEMPTS: u64 = 5;
    pub const ENRICH_RETRY_MS: u64 = 2_000;
    pub const ENRICH_TIMEOUT_MS: u64 = 10_000;
    /// The runs and the jobs one analytics projection lists at most: with every result at its
    /// largest (32 capital scenarios per score) the frame stays far below one STATE frame.
    pub const ANALYTICS_RUNS_MAX: u64 = 64;
    pub const ANALYTICS_JOBS_MAX: u64 = 16;
    /// The quote rounds one rebalancing observation may take at most (each round asks the quotes
    /// the plan still lacks).
    pub const REBALANCE_QUOTE_ROUNDS_MAX: u64 = 8;
    /// A slippage bound in basis points is a fraction of the whole (10 000 bps).
    pub const BPS_MAX: u64 = 10_000;
    /// The most accounts a Jupiter route may use (the route provider refuses more).
    pub const JUPITER_ACCOUNTS_MAX: u64 = 64;
}

macro_rules! parameters {
    ($( $name:ident = $default:expr, $unit:literal, $env:literal; )*) => {
        #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
        #[serde(deny_unknown_fields)]
        pub struct Limits { $(pub $name: u64,)* }
        impl Default for Limits { fn default() -> Self { Self { $($name: $default,)* } } }
        /// Every numeric operational parameter, typed (the one schema of CLI, env, files, help).
        pub const PARAMETERS: &[Parameter] = &[$(Parameter { field: stringify!($name), unit: $unit, default: $default, env: $env },)*];
        impl Limits {
            /// Value of a numeric parameter by field name.
            pub fn get(&self, field: &str) -> Option<u64> {
                match field { $(stringify!($name) => Some(self.$name),)* _ => None }
            }
            /// Sets a numeric parameter by field name; `false` for an unknown field.
            pub fn set(&mut self, field: &str, value: u64) -> bool {
                match field { $(stringify!($name) => { self.$name = value; true })* _ => false }
            }
        }
    }
}
parameters! {
    rpc_rps = 5, "requests/s", "P0_RPC_RPS";
    rpc_burst = 1, "requests", "P0_RPC_BURST";
    rpc_concurrency = 2, "requests", "P0_RPC_CONCURRENCY";
    rpc_queue = 64, "requests", "P0_RPC_QUEUE";
    rpc_timeout_ms = 30000, "ms", "P0_RPC_TIMEOUT_MS";
    rpc_connect_timeout_ms = 10000, "ms", "P0_RPC_CONNECT_TIMEOUT_MS";
    rpc_pool_idle_ms = 90000, "ms", "P0_RPC_POOL_IDLE_MS";
    rpc_tcp_keepalive_ms = 15000, "ms", "P0_RPC_TCP_KEEPALIVE_MS";
    rpc_attempts = 3, "attempts", "P0_RPC_ATTEMPTS";
    rpc_backoff_ms = 500, "ms", "P0_RPC_BACKOFF_MS";
    rpc_backoff_max_ms = 60000, "ms", "P0_RPC_BACKOFF_MAX_MS";
    max_response_mib = 512, "MiB", "P0_MAX_RESPONSE_MIB";
    max_total_mib = 4096, "MiB", "P0_MAX_TOTAL_MIB";
    max_requests = 1000000, "requests", "P0_MAX_REQUESTS";
    hotpath_rps = 5, "requests/s", "P0_HOTPATH_RPS";
    bootstrap_rps = 5, "requests/s", "P0_BOOTSTRAP_RPS";
    history_rps = 2, "requests/s", "P0_HISTORY_RPS";
    analytics_rps = 2, "requests/s", "P0_ANALYTICS_RPS";
    simulation_rps = 2, "requests/s", "P0_SIMULATION_RPS";
    fee_rps = 2, "requests/s", "P0_FEE_RPS";
    send_rps = 2, "requests/s", "P0_SEND_RPS";
    wss_rps = 1, "connections/s", "P0_WSS_RPS";
    wss_connections = 2, "connections", "P0_WSS_CONNECTIONS";
    wss_subscriptions = 65536, "subscriptions", "P0_WSS_SUBSCRIPTIONS";
    wss_message_mib = 64, "MiB", "P0_WSS_MESSAGE_MIB";
    wss_connect_timeout_ms = 10000, "ms", "P0_WSS_CONNECT_TIMEOUT_MS";
    wss_idle_timeout_ms = 60000, "ms", "P0_WSS_IDLE_TIMEOUT_MS";
    wss_reconnect_ms = 500, "ms", "P0_WSS_RECONNECT_MS";
    wss_reconnect_max_ms = 30000, "ms", "P0_WSS_RECONNECT_MAX_MS";
    wss_attempts = 8, "attempts", "P0_WSS_ATTEMPTS";
    wss_recovery_ms = 120000, "ms", "P0_WSS_RECOVERY_MS";
    wss_ack_ms_per_subscription = 5, "ms", "P0_WSS_ACK_MS_PER_SUBSCRIPTION";
    workers = 1, "threads", "RAYON_NUM_THREADS";
    ingress_queue = defaults::INGRESS_QUEUE, "events", "P0_INGRESS_BUFFER";
    worker_queue = 256, "jobs", "P0_REMOTE_WORKER_BUFFER";
    sweep_batch_max_events = 64, "events", "P0_SWEEP_BATCH_MAX_EVENTS";
    sweep_batch_max_age_ns = 5000000, "ns", "P0_SWEEP_BATCH_MAX_AGE_NS";
    ingress_burst_before_detect = 64, "events", "P0_INGRESS_BURST_BEFORE_DETECT";
    bootstrap_seconds = 600, "s", "P0_ECON_BOOTSTRAP_SECS";
    stream_ready_seconds = defaults::STREAM_READY_SECONDS, "s", "P0_MARKET_READY_TIMEOUT_SECS";
    drain_seconds = 180, "s", "P0_ECON_DRAIN_SECS";
    market_drain_seconds = defaults::MARKET_DRAIN_SECONDS, "s", "P0_MARKET_DRAIN_TIMEOUT_SECS";
    reconcile_seconds = 14400, "s", "P0_MARKET_RECONCILE_PHASE_MAX_SECS";
    gap_seconds = 180, "s", "P0_MARKET_GAP_PHASE_MAX_SECS";
    enrichment_seconds = defaults::ENRICHMENT_SECONDS, "s", "P0_MARKET_ENRICH_PHASE_MAX_SECS";
    snapshot_timeout_seconds = 90, "s", "P0_SNAPSHOT_RPC_TIMEOUT_SECS";
    quiescence_seconds = 150, "s", "P0_QUIESCENCE_SECS";
    shutdown_seconds = 180, "s", "P0_SHUTDOWN_SECS";
    verify_rps = 2, "requests/s", "P0_VERIFY_RPS";
    verify_calls = 2000, "requests", "P0_VERIFY_CALLS";
    verify_seconds = 300, "s", "P0_VERIFY_SECONDS";
    verify_mib = 256, "MiB", "P0_VERIFY_MIB";
    verify_disk_mib = 8192, "MiB", "P0_VERIFY_DISK_MIB";
    verify_ram_mib = 2048, "MiB", "P0_VERIFY_RAM_MIB";
    verify_free_mib = 1024, "MiB", "P0_VERIFY_FREE_MIB";
    verify_pages = 50, "pages", "P0_VERIFY_PAGES";
    verify_timeout_seconds = 15, "s", "P0_VERIFY_TIMEOUT_SECONDS";
    fe_snapshot_rounds = 8, "rounds", "P0_FE_SNAPSHOT_ROUNDS";
    fe_batch_size = 100, "accounts (RPC protocol maximum 100)", "P0_FE_BATCH_SIZE";
    fe_qualification_age_seconds = 86400, "s", "P0_FE_QUALIFICATION_AGE_SECONDS";
    fe_calls = 3, "requests", "P0_FE_CALLS";
    fe_seconds = 30, "s", "P0_FE_SECONDS";
    fe_max_mib = 16, "MiB", "P0_FE_MAX_MIB";
    boot_calls = 64, "requests", "P0_BOOT_CALLS";
    boot_seconds = 120, "s", "P0_BOOT_SECONDS";
    market_capture_initial_mib = 64, "MiB", "P0_MARKET_CAPTURE_INITIAL_MIB";
    market_capture_max_mib = 512, "MiB", "P0_MARKET_CAPTURE_MAX_MIB";
    system_reserve_mib = 4096, "MiB", "P0_SYSTEM_RESERVE_MIB";
    market_writer_batch_frames = 512, "frames", "P0_MARKET_WRITER_BATCH_FRAMES";
    market_writer_flush_ms = 1000, "ms", "P0_MARKET_WRITER_FLUSH_MS";
    market_writer_sync_ms = 60000, "ms", "P0_MARKET_WRITER_SYNC_MS";
    owner_evidence_sync_ms = 60000, "ms", "P0_OWNER_EVIDENCE_SYNC_MS";
    market_writer_stall_ms = 30000, "ms", "P0_MARKET_WRITER_STALL_MS";
    market_disk_reserve_mib = 2048, "MiB", "P0_MARKET_DISK_RESERVE_MIB";
    capacity_safety_percent = 50, "percent", "P0_CAPACITY_SAFETY_PERCENT";
    market_segment_mib = 256, "MiB", "P0_MARKET_SEGMENT_MIB";
    econ_segment_mib = 256, "MiB", "P0_ECON_SEGMENT_MIB";
    econ_retention_mib = 1024, "MiB", "P0_ECON_RETENTION_MIB";
    owner_status_ms = 1000, "ms", "P0_OWNER_STATUS_MS";
    blockhash_max_age_ms = 30000, "ms", "P0_BLOCKHASH_MAX_AGE_MS";
    fe04_settle_retry_ms = 10000, "ms", "P0_FE04_SETTLE_RETRY_MS";
    fe04_refresh_ms = 5000, "ms", "P0_FE04_REFRESH_MS";
    wss_healthy_reset_ms = 10000, "ms", "P0_WSS_HEALTHY_RESET_MS";
    jupiter_timeout_ms = 1500, "ms", "P0_JUPITER_TIMEOUT_MS";
    projection_requeue_max = 2, "requeues", "P0_PROJECTION_REQUEUE_MAX";
    lane_weight_l0 = 8, "credits", "P0_LANE_WEIGHT_L0";
    lane_weight_l1 = 3, "credits", "P0_LANE_WEIGHT_L1";
    lane_weight_l2 = 1, "credits", "P0_LANE_WEIGHT_L2";
    prepared_candidates = 1024, "templates", "P0_PREPARED_CANDIDATES";
    winner_watch_capacity = 8192, "watches", "P0_WINNER_WATCH_CAPACITY";
    winner_watch_ttl_seconds = 900, "s", "P0_WINNER_WATCH_TTL_S";
    winner_watch_initial_delay_seconds = 60, "s", "P0_WINNER_WATCH_INITIAL_DELAY_S";
    winner_watch_recheck_seconds = 5, "s", "P0_WINNER_WATCH_RECHECK_S";
    winner_watch_full_backoff_seconds = 1, "s", "P0_WINNER_WATCH_FULL_BACKOFF_S";
    deployment_request_ms = 10000, "ms", "P0_DEPLOYMENT_REQUEST_MS";
    deployment_deadline_ms = 30000, "ms", "P0_DEPLOYMENT_DEADLINE_MS";
    report_final_flush_ms = 2000, "ms", "P0_REPORT_FINAL_FLUSH_MS";
    market_heartbeat_seconds = defaults::MARKET_HEARTBEAT_SECONDS, "s", "P0_MARKET_HEARTBEAT_SECS";
    market_min_sample = defaults::MARKET_MIN_SAMPLE, "liquidations", "P0_MARKET_MIN_SAMPLE";
    market_retention = defaults::RETENTION_ROWS, "rows", "P0_MARKET_RETENTION";
    econ_retention = defaults::RETENTION_ROWS, "rows", "P0_ECON_RETENTION";
    econ_queue_capacity = defaults::ECON_QUEUE_CAPACITY, "rows", "P0_ECON_QUEUE_CAPACITY";
    enrich_max_pending = defaults::ENRICH_MAX_PENDING, "requests", "P0_MARKET_ENRICH_MAX_PENDING";
    enrich_max_attempts = defaults::ENRICH_MAX_ATTEMPTS, "attempts", "P0_MARKET_ENRICH_MAX_ATTEMPTS";
    enrich_retry_ms = defaults::ENRICH_RETRY_MS, "ms", "P0_MARKET_ENRICH_RETRY_MS";
    enrich_timeout_ms = defaults::ENRICH_TIMEOUT_MS, "ms", "P0_MARKET_ENRICH_TIMEOUT_MS";
    analytics_status_ms = 2000, "ms", "P0_ANALYTICS_STATUS_MS";
    analytics_runs_listed = 32, "runs", "P0_ANALYTICS_RUNS_LISTED";
    analytics_jobs_listed = 8, "jobs", "P0_ANALYTICS_JOBS_LISTED";
    rebalance_observe_ms = 60000, "ms", "P0_REBALANCE_OBSERVE_MS";
    rebalance_budget_usd_micro = 5000000, "USD-micro", "P0_REBALANCE_BUDGET_USD_MICRO";
    rebalance_quote_rounds = 3, "rounds", "P0_REBALANCE_QUOTE_ROUNDS";
    // DEC-05: the state owner's parameters, each a default at its call site before (revision 11).
    max_total_capital = 0, "USD-micro", "P0_MAX_TOTAL_CAPITAL";
    max_outstanding_inventory = 0, "USD-micro", "P0_MAX_INVENTORY";
    max_concurrent_positions = 1, "positions", "P0_MAX_CONCURRENT";
    max_exposure_per_attempt = 0, "USD-micro", "P0_MAX_EXPOSURE";
    max_daily_loss = 0, "USD-micro", "P0_MAX_DAILY_LOSS";
    max_failed_attempts_per_day = 0, "attempts", "P0_MAX_FAILED";
    max_tip = 0, "lamports", "P0_MAX_TIP";
    max_slippage_bps = 0, "bps", "P0_MAX_SLIPPAGE_BPS";
    max_outstanding_attempts = 1, "attempts", "P0_MAX_OUTSTANDING";
    min_profit_usd_micro = 0, "USD-micro", "P0_MIN_PROFIT";
    jupiter_max_accounts = 64, "accounts", "P0_JUPITER_MAX_ACCOUNTS";
    compute_unit_limit = 1400000, "compute units", "P0_CU_LIMIT";
    priority_micro_lamports_per_cu = 0, "micro-lamports/CU", "P0_PRIORITY_MICRO";
    loaded_accounts_data_size = 67108864, "bytes", "P0_LOADED_DATA";
    max_opportunity_age_ns = 2000000000, "ns", "P0_MAX_AGE_NS";
    execution_deadline_ns = 2000000000, "ns", "P0_DEADLINE_NS";
    integration_probe_limit_per_kind = 8, "probes", "P0_INTEGRATION_PROBE_LIMIT_PER_KIND";
    threshold_bootstrap_batch = 16, "accounts", "P0_THRESHOLD_BOOTSTRAP_BATCH";
    candidate_recheck_slots = 1, "slots", "P0_CANDIDATE_RECHECK_SLOTS";
    candidate_error_recheck_slots = 1, "slots", "P0_CANDIDATE_ERROR_RECHECK_SLOTS";
    candidate_repeat_soft_cap = 1024, "candidates", "P0_CANDIDATE_REPEAT_SOFT_CAP";
    rpc_candidates_per_drain = 4, "candidates", "P0_RPC_CANDIDATES_PER_DRAIN";
    candidate_network_deadline_ms = 5000, "ms", "P0_CANDIDATE_NETWORK_DEADLINE_MS";
    mt0_interval_seconds = 10, "s", "P0_MT0_INTERVAL_SECS";
    blockhash_refresh_ms = 10000, "ms", "P0_BLOCKHASH_REFRESH_MS";
    recovery_root_catchup_seconds = defaults::RECOVERY_ROOT_CATCHUP_SECS, "s", "P0_RECOVERY_ROOT_CATCHUP_SECS";
    recovery_root_stall_seconds = defaults::RECOVERY_ROOT_STALL_SECS, "s", "P0_RECOVERY_ROOT_STALL_SECS";
    recovery_root_hard_max_seconds = defaults::RECOVERY_ROOT_HARD_MAX_SECS, "s", "P0_RECOVERY_ROOT_HARD_MAX_SECS";
    // DEC-10: how far ahead an FE-04 approval may expire (revision 12).
    fe04_approval_max_lifetime_seconds = 86400, "s", "P0_FE04_APPROVAL_MAX_LIFETIME_SECS";
}
/// Layout revision of [`Limits`]: bumped with every change of its field list, since each typed
/// record that embeds it (`OPERATIONAL_CONFIG`, `RUN_MANIFEST`) changes its binary layout and its
/// payload schema with it. 1: up to `econ_retention_mib`; 2: `boot_calls`, `boot_seconds` (N1);
/// 3: `owner_evidence_sync_ms` (M2); 4: `wss_ack_ms_per_subscription` (N8); 5: `fee_rps`,
/// `send_rps` (V2-05); 6: `sweep_batch_max_events`, `sweep_batch_max_age_ns`,
/// `ingress_burst_before_detect` (V2-08); 7: `rpc_pool_idle_ms`, `rpc_tcp_keepalive_ms` (V2-07);
/// 8: `owner_status_ms` (owner status projection) and D01–D06/D11 — the owner, FE-04, WSS,
/// Jupiter, lane, winner-watch, deployment, reporter and market bounds that were literals or
/// duplicate defaults (from `owner_status_ms` on, appended); 9: `analytics_status_ms`,
/// `analytics_runs_listed`, `analytics_jobs_listed` (B03/U-9 analytics job projection); 10:
/// `rebalance_observe_ms`, `rebalance_budget_usd_micro`, `rebalance_quote_rounds` (B01); 11: the
/// state owner's parameters `max_total_capital` … `recovery_root_hard_max_seconds` and the typed
/// owner settings appended to [`EffectiveConfig`] (DEC-05, [`OWNER_SETTINGS_REVISION`]); 12:
/// `fe04_approval_max_lifetime_seconds` (DEC-10).
pub const LIMITS_REVISION: u16 = 12;

/// The first revision whose [`EffectiveConfig`] ends with the typed owner settings
/// (`threshold_trigger` … `source_geyser_provider`); an earlier record has none.
pub const OWNER_SETTINGS_REVISION: u16 = 11;

/// The oldest `Limits` layout an earlier record can be read in: revision 4 is checked against the
/// git history (the current field order filtered to its fields); earlier ones are not.
pub const LIMITS_READABLE_FROM: u16 = 4;

/// The revision each parameter entered the `Limits` layout. Fields only ever entered (inserted or
/// appended); none moved or left, so a layout is the current order filtered to its fields.
fn introduced(field: &str) -> u16 {
    let position = |f: &str| PARAMETERS.iter().position(|p| p.field == f);
    match field {
        "boot_calls" | "boot_seconds" => 2,
        "owner_evidence_sync_ms" => 3,
        "wss_ack_ms_per_subscription" => 4,
        "fee_rps" | "send_rps" => 5,
        "sweep_batch_max_events" | "sweep_batch_max_age_ns" | "ingress_burst_before_detect" => 6,
        "rpc_pool_idle_ms" | "rpc_tcp_keepalive_ms" => 7,
        f if position(f) >= position("fe04_approval_max_lifetime_seconds") => 12,
        f if position(f) >= position("max_total_capital") => 11,
        f if position(f) >= position("rebalance_observe_ms") => 10,
        f if position(f) >= position("analytics_status_ms") => 9,
        f if position(f) >= position("owner_status_ms") => 8,
        _ => 1,
    }
}

/// The parameter fields of layout `revision` in wire order; `None` outside the readable range.
pub fn fields_at(revision: u16) -> Option<Vec<&'static str>> {
    (LIMITS_READABLE_FROM..=LIMITS_REVISION)
        .contains(&revision)
        .then(|| {
            PARAMETERS
                .iter()
                .map(|p| p.field)
                .filter(|f| introduced(f) <= revision)
                .collect()
        })
}

impl Limits {
    /// The `Limits` of layout `revision` at the start of `bytes` (one little-endian u64 per field
    /// of that layout, as the fixint payload encoding writes them); a field the layout did not
    /// have keeps its built-in default. Returns the limits and the bytes after them.
    pub fn decode_revision(revision: u16, bytes: &[u8]) -> Option<(Limits, &[u8])> {
        let mut limits = Limits::default();
        let mut rest = bytes;
        for field in fields_at(revision)? {
            let (value, tail) = rest.split_first_chunk::<8>()?;
            limits.set(field, u64::from_le_bytes(*value));
            rest = tail;
        }
        Some((limits, rest))
    }

    /// `self` in layout `revision` (what that revision's writer produced): historical-reader
    /// fixtures.
    #[doc(hidden)]
    pub fn encode_revision(&self, revision: u16) -> Option<Vec<u8>> {
        Some(
            fields_at(revision)?
                .into_iter()
                .flat_map(|f| self.get(f).unwrap_or(0).to_le_bytes())
                .collect(),
        )
    }
}

/// Typed metadata of one numeric operational parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub field: &'static str,
    pub unit: &'static str,
    pub default: u64,
    pub env: &'static str,
}
impl Parameter {
    /// CLI name (`--rpc-rps`).
    pub fn name(&self) -> String {
        self.field.replace('_', "-")
    }
    /// The smallest valid value. A Safety ceiling of 0 admits nothing (fail-closed); a slippage,
    /// tip, priority fee or profit floor of 0 is a setting of its own; the probe limit is only
    /// required with `diagnostics` (`validate`). The recovery waits keep their former bounds.
    pub fn minimum(&self) -> u64 {
        match self.field {
            "max_total_capital"
            | "max_outstanding_inventory"
            | "max_concurrent_positions"
            | "max_exposure_per_attempt"
            | "max_daily_loss"
            | "max_failed_attempts_per_day"
            | "max_tip"
            | "max_slippage_bps"
            | "max_outstanding_attempts"
            | "min_profit_usd_micro"
            | "priority_micro_lamports_per_cu"
            | "integration_probe_limit_per_kind" => 0,
            "recovery_root_catchup_seconds" | "recovery_root_hard_max_seconds" => 30,
            "recovery_root_stall_seconds" => 5,
            _ => 1,
        }
    }
    pub fn maximum(&self) -> u64 {
        match self.field {
            "fe_batch_size" => 100,
            "market_retention" | "econ_retention" => defaults::RETENTION_ROWS_MAX,
            "econ_queue_capacity" => defaults::ECON_QUEUE_CAPACITY_MAX,
            "analytics_runs_listed" => defaults::ANALYTICS_RUNS_MAX,
            "analytics_jobs_listed" => defaults::ANALYTICS_JOBS_MAX,
            "rebalance_quote_rounds" => defaults::REBALANCE_QUOTE_ROUNDS_MAX,
            // DEC-05: the owner's types (u64 USD-micro/lamports/ns, the census's i64 profit
            // floor, a bps fraction, the route provider's account bound).
            "max_total_capital"
            | "max_outstanding_inventory"
            | "max_exposure_per_attempt"
            | "max_daily_loss"
            | "max_tip"
            | "priority_micro_lamports_per_cu"
            | "max_opportunity_age_ns"
            | "execution_deadline_ns" => u64::MAX,
            "min_profit_usd_micro" => i64::MAX as u64,
            "max_slippage_bps" => defaults::BPS_MAX,
            "jupiter_max_accounts" => defaults::JUPITER_ACCOUNTS_MAX,
            "recovery_root_catchup_seconds" => 300,
            "recovery_root_stall_seconds" => 180,
            "recovery_root_hard_max_seconds" => 900,
            _ => u64::from(u32::MAX),
        }
    }
    /// DEC-05/ENTSCHEIDUNGEN §3: the open measurement question that keeps this value
    /// provisional (B-2 worker count, B-3 sweep batch, B-4 lane weights, B-5 pool idle and
    /// keepalive); `None` for a value no measurement question covers.
    pub fn provisional(&self) -> Option<&'static str> {
        match self.field {
            "workers" => Some("B-2"),
            "sweep_batch_max_events" | "sweep_batch_max_age_ns" | "ingress_burst_before_detect" => {
                Some("B-3")
            }
            "lane_weight_l0" | "lane_weight_l1" | "lane_weight_l2" => Some("B-4"),
            "rpc_pool_idle_ms" | "rpc_tcp_keepalive_ms" => Some("B-5"),
            _ => None,
        }
    }
    /// When a change takes effect: every numeric parameter is read once at the start (DEC-05:
    /// restart is the rule; a hot change needs a decision of its own).
    pub fn change_rule(&self) -> &'static str {
        "restart"
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Smoke,
    Standard,
    Performance,
    Custom,
}
impl Profile {
    pub const ALL: [Profile; 4] = [
        Profile::Smoke,
        Profile::Standard,
        Profile::Performance,
        Profile::Custom,
    ];
    pub fn name(&self) -> &'static str {
        match self {
            Profile::Smoke => "smoke",
            Profile::Standard => "standard",
            Profile::Performance => "performance",
            Profile::Custom => "custom",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    pub profile: Profile,
    pub limits: Limits,
    pub rpc_endpoints: Vec<String>,
    pub wss_endpoints: Vec<String>,
    pub fallback: bool,
    pub adaptive: bool,
    pub duration: String,
    pub verification: String,
    pub diagnostics: bool,
    pub fe_mode: String,
    pub fe_targets: Vec<String>,
    pub fe_group: String,
    /// DEC-05: the owner's typed settings. `P0_USE_THRESHOLD_TRIGGER`: the threshold trigger
    /// instead of the R127 sweep (DEC-06 turns it into the threshold mode).
    pub threshold_trigger: bool,
    /// `P0_C0_CENSUS`: the C0 census on the startup sweep.
    pub c0_census: bool,
    /// `P0_SOURCE_GEYSER`: the start mode of the Geyser source unit (S1-02: off|observe; ACTIVE
    /// needs a qualification the source control refuses without).
    pub source_geyser: String,
    /// `P0_SOURCE_GEYSER_PROVIDER`: its provider.
    pub source_geyser_provider: String,
    /// DEC-03: the FREE_ONLY B sources of the FE-04 non-landing proof, `operator=URL` each
    /// (`P0_NON_LANDING_SOURCES`, comma separated; redacted like the endpoints). Two usable
    /// sources of different operators confirm a non-landing with C; fewer leave C alone.
    pub non_landing_sources: Vec<String>,
    /// DEC-04 (B-8): N, the finalized attempts with an exact B and no underestimation before B
    /// may be ACTIVE (`P0_HOLD_B_MIN_ATTEMPTS`). The user's decision, no default: unset keeps B
    /// in OBSERVE.
    pub hold_b_min_attempts: Option<u64>,
    /// DEC-11: the Jito block engine of the bundle path (`P0_JITO_BLOCK_ENGINE_URL`, redacted);
    /// unset: no Jito adapter is built. This build never sends (no FE-06 authorization).
    pub jito_block_engine_url: Option<String>,
    /// DEC-11: the bundle simulator (`P0_BUNDLE_SIM_RPC_URL`, redacted): a Jito-Solana RPC with
    /// `simulateBundle`; used only once qualified — this build holds no qualification.
    pub bundle_sim_rpc_url: Option<String>,
    /// DEC-11: the tip account of the bundle's tip transaction (`P0_JITO_TIP_ACCOUNT`, base58).
    pub jito_tip_account: Option<String>,
}
impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            profile: Profile::Standard,
            limits: Limits::default(),
            rpc_endpoints: vec![],
            wss_endpoints: vec![],
            non_landing_sources: vec![],
            hold_b_min_attempts: None,
            jito_block_engine_url: None,
            bundle_sim_rpc_url: None,
            jito_tip_account: None,
            fallback: false,
            adaptive: true,
            duration: "4h".into(),
            verification: "off".into(),
            diagnostics: false,
            fe_mode: "smoke".into(),
            fe_targets: vec![
                "A45dVtuyLccxs1xpbVJsTNh7AdtAotAZi4UxEcyMQDMS".into(),
                "64vknRpPQgoyLDvro5GZtYk39odDtkLQ99Ye71T3vzEk".into(),
            ],
            fe_group: "4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8".into(),
            threshold_trigger: false,
            c0_census: false,
            source_geyser: "off".into(),
            source_geyser_provider: crate::ingress::sources::PUBLICNODE_ID.into(),
        }
    }
}
/// The native `key = value` text of operator files (operational config, profiles, the FE-04
/// approval of DEC-10): unique keys (`-` read as `_`), `#` comment lines, no JSON.
pub fn key_values(text: &str) -> Result<Vec<(String, String)>, String> {
    parse_lines(text)
}
/// `key = value` lines: the native operational config and profile text format (JFI-01/02: a
/// config file is not a JSON interface). `#` starts a comment line; keys are CLI names
/// (`rpc-rps`) or field names (`rpc_rps`); values are written exactly as on the CLI.
fn parse_lines(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("config line {}: expected key = value", n + 1))?;
        let key = key.trim().replace('-', "_");
        if key.is_empty()
            || !key
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(format!("config line {}: invalid key", n + 1));
        }
        if !seen.insert(key.clone()) {
            return Err(format!("duplicate config key {key}"));
        }
        out.push((key, value.trim().to_string()));
    }
    Ok(out)
}
/// The one on/off parser of CLI, config file and environment (`true`/`false` as before).
fn on_off(v: &str) -> Result<bool, String> {
    match v {
        "true" | "on" => Ok(true),
        "false" | "off" => Ok(false),
        _ => Err("expected on/off".to_string()),
    }
}
fn number(key: &str, value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("invalid --{}", key.replace('_', "-")))
}

/// Typed profile overlay from `config/profiles/<profile>.conf`, embedded at build time. Only
/// limits, duration and the worker rule exist: endpoints, modes and approvals cannot be expressed.
#[derive(Debug)]
pub struct ProfileFile {
    pub schema: String,
    pub profile: Profile,
    pub description: String,
    pub duration: Option<String>,
    pub limits: BTreeMap<String, u64>,
    pub workers: Option<WorkerRule>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum WorkerRule {
    AvailableParallelism,
}
pub const PROFILE_FILES: [(&str, &str); 4] = [
    (
        "smoke",
        include_str!("../../../../config/profiles/smoke.conf"),
    ),
    (
        "standard",
        include_str!("../../../../config/profiles/standard.conf"),
    ),
    (
        "performance",
        include_str!("../../../../config/profiles/performance.conf"),
    ),
    (
        "custom",
        include_str!("../../../../config/profiles/custom.conf"),
    ),
];
impl ProfileFile {
    pub fn parse(text: &str) -> Result<Self, String> {
        let bad = || "invalid profile file".to_string();
        let (mut schema, mut profile, mut description) = (None, None, None);
        let (mut duration, mut workers, mut limits) = (None, None, BTreeMap::new());
        for (key, value) in parse_lines(text).map_err(|_| bad())? {
            match key.as_str() {
                "schema" => schema = Some(value),
                "profile" => profile = Some(Profile::parse(&value).ok_or_else(bad)?),
                "description" => description = Some(value),
                "duration" => duration = Some(value),
                "workers" if value == "available_parallelism" => {
                    workers = Some(WorkerRule::AvailableParallelism)
                }
                field if Limits::default().get(field).is_some() => {
                    limits.insert(field.to_string(), number(field, &value).map_err(|_| bad())?);
                }
                _ => return Err(bad()),
            }
        }
        if schema.as_deref() != Some("Project0/RuntimeProfile/2") {
            return Err(bad());
        }
        Ok(Self {
            schema: schema.unwrap_or_default(),
            profile: profile.ok_or_else(bad)?,
            description: description.ok_or_else(bad)?,
            duration,
            limits,
            workers,
        })
    }
    fn load(profile: &Profile) -> Self {
        let text = PROFILE_FILES
            .iter()
            .find(|(n, _)| *n == profile.name())
            .map(|(_, t)| *t)
            .expect("every profile has a file");
        Self::parse(text).expect("embedded profile files are validated by tests")
    }
}
#[derive(Clone, Debug)]
pub struct Resolved {
    pub config: RuntimeConfig,
    pub sources: BTreeMap<String, String>,
    pub print: bool,
    pub help: bool,
}
impl RuntimeConfig {
    fn profile(profile: Profile) -> Self {
        let mut c = Self {
            profile: profile.clone(),
            ..Self::default()
        };
        let file = ProfileFile::load(&profile);
        if let Some(duration) = file.duration {
            c.duration = duration;
        }
        for (field, value) in file.limits {
            assert!(
                c.limits.set(&field, value),
                "profile limits validated by tests"
            );
        }
        if file.workers == Some(WorkerRule::AvailableParallelism) {
            c.limits.workers = std::thread::available_parallelism().map_or(1, |n| n.get() as u64);
        }
        c
    }
    /// A typed on/off setting by field name (only the names `resolve` passes).
    fn flag(&mut self, field: &str) -> &mut bool {
        match field {
            "diagnostics" => &mut self.diagnostics,
            "threshold_trigger" => &mut self.threshold_trigger,
            "c0_census" => &mut self.c0_census,
            _ => unreachable!("no on/off setting {field}"),
        }
    }
    /// A typed text setting by field name (only the names `resolve` passes).
    fn text(&mut self, field: &str) -> &mut String {
        match field {
            "source_geyser" => &mut self.source_geyser,
            "source_geyser_provider" => &mut self.source_geyser_provider,
            _ => unreachable!("no text setting {field}"),
        }
    }
    /// Explicit operational endpoint wins over legacy verifier aliases, so
    /// provider qualification and the strict worker bind the same endpoint.
    pub fn rpc_url(&self, env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
        self.rpc_endpoints
            .first()
            .cloned()
            .or_else(|| env("P0_AUDIT_RPC_URL"))
            .or_else(|| env("P0_RPC_URL"))
    }
    /// DEC-11: a bundle-path setting by its field name.
    fn bundle_field(&mut self, field: &str) -> &mut Option<String> {
        match field {
            "jito_block_engine_url" => &mut self.jito_block_engine_url,
            "bundle_sim_rpc_url" => &mut self.bundle_sim_rpc_url,
            "jito_tip_account" => &mut self.jito_tip_account,
            _ => unreachable!("no bundle setting {field}"),
        }
    }
    /// DEC-03: the configured non-landing B sources as `(operator, URL)` — an operator label of
    /// 1–32 lowercase letters, digits or `-`, an `http(s)` URL with a host. Independence is by
    /// operator, so one URL is configured once and one host belongs to one operator.
    pub fn non_landing_source_list(&self) -> Result<Vec<(String, String)>, String> {
        let mut hosts: BTreeMap<String, String> = BTreeMap::new();
        let mut urls = std::collections::BTreeSet::new();
        self.non_landing_sources
            .iter()
            .map(|entry| {
                let (operator, url) = entry
                    .split_once('=')
                    .ok_or("non-landing source must be operator=URL")?;
                let label = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
                if operator.is_empty() || operator.len() > 32 || !operator.chars().all(label) {
                    return Err("non-landing source operator invalid".to_string());
                }
                let u = reqwest::Url::parse(url).map_err(|_| "non-landing source URL invalid")?;
                let Some(host) = u
                    .host_str()
                    .filter(|_| ["http", "https"].contains(&u.scheme()))
                else {
                    return Err("non-landing source URL invalid".into());
                };
                if !urls.insert(url.to_string())
                    || hosts
                        .insert(host.to_ascii_lowercase(), operator.to_string())
                        .is_some_and(|other| other != operator)
                {
                    return Err("non-landing source repeated or host of two operators".into());
                }
                Ok((operator.to_string(), url.to_string()))
            })
            .collect()
    }
    pub fn validate(&self) -> Result<(), String> {
        for p in PARAMETERS {
            if self
                .limits
                .get(p.field)
                .is_none_or(|n| n < p.minimum() || n > p.maximum())
            {
                return Err(format!(
                    "invalid --{}: expected {}..={}",
                    p.name(),
                    p.minimum(),
                    p.maximum()
                ));
            }
        }
        // DEC-05: the owner's cross-field rules (formerly checked at its call sites).
        if self.limits.recovery_root_hard_max_seconds < self.limits.recovery_root_catchup_seconds {
            return Err(
                "recovery-root-hard-max-seconds must be >= recovery-root-catchup-seconds".into(),
            );
        }
        if self.diagnostics && self.limits.integration_probe_limit_per_kind == 0 {
            return Err("integration-probe-limit-per-kind must be > 0 with diagnostics on".into());
        }
        self.source_geyser
            .parse::<crate::ingress::sources::OperatingMode>()
            .map_err(|_| "invalid source-geyser: expected off|observe|active")?;
        if self.source_geyser_provider.trim().is_empty() {
            return Err("source-geyser-provider is empty".into());
        }
        if self.limits.rpc_backoff_ms > self.limits.rpc_backoff_max_ms
            || self.limits.wss_reconnect_ms > self.limits.wss_reconnect_max_ms
        {
            return Err("backoff initial exceeds maximum".into());
        }
        if self.limits.boot_seconds > 300 {
            return Err("boot-seconds exceeds 300".into());
        }
        if self.limits.max_response_mib > self.limits.max_total_mib
            || self.limits.wss_message_mib > self.limits.max_total_mib
        {
            return Err("response limit exceeds total byte budget".into());
        }
        for (endpoints, schemes) in [
            (&self.rpc_endpoints, ["http", "https"]),
            (&self.wss_endpoints, ["ws", "wss"]),
        ] {
            for url in endpoints {
                let u = reqwest::Url::parse(url).map_err(|_| "invalid endpoint URL")?;
                if !schemes.contains(&u.scheme()) || u.host_str().is_none() {
                    return Err("endpoint scheme/host invalid".into());
                }
            }
        }
        self.non_landing_source_list()?;
        if self.hold_b_min_attempts == Some(0) {
            return Err("hold_b_min_attempts must be at least 1 (B-8)".into());
        }
        for (name, url) in [
            ("jito_block_engine_url", &self.jito_block_engine_url),
            ("bundle_sim_rpc_url", &self.bundle_sim_rpc_url),
        ] {
            if let Some(url) = url {
                let u = reqwest::Url::parse(url).map_err(|_| format!("{name} invalid"))?;
                if !["http", "https"].contains(&u.scheme()) || u.host_str().is_none() {
                    return Err(format!("{name} invalid"));
                }
            }
        }
        if let Some(tip) = &self.jito_tip_account {
            tip.parse::<solana_sdk::pubkey::Pubkey>()
                .map_err(|_| "jito_tip_account invalid".to_string())?;
        }
        if !["off", "index", "strict", "crosscheck"].contains(&self.verification.as_str()) {
            return Err("invalid verification mode".into());
        }
        if !["smoke", "full"].contains(&self.fe_mode.as_str()) {
            return Err("fe-mode must be smoke|full".into());
        }
        if self.fe_targets.is_empty() || self.limits.fe_batch_size > 100 {
            return Err("FE target scope empty or batch exceeds Solana RPC maximum 100".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for key in &self.fe_targets {
            key.parse::<solana_sdk::pubkey::Pubkey>()
                .map_err(|_| "invalid FE target pubkey")?;
            if !seen.insert(key) {
                return Err("duplicate FE target".into());
            }
        }
        self.fe_group
            .parse::<solana_sdk::pubkey::Pubkey>()
            .map_err(|_| "invalid FE group pubkey")?;
        if self.fe_mode == "smoke" && self.fe_targets.len() > 100 {
            return Err("smoke scope exceeds one getMultipleAccounts; use full".into());
        }
        duration_seconds(&self.duration)?;
        Ok(())
    }
    /// Legacy consumers are populated once, before threads start. Finance/mode/
    /// signing variables are intentionally absent from this mapping.
    pub fn environment(&self) -> BTreeMap<String, String> {
        let mut env: BTreeMap<String, String> = PARAMETERS
            .iter()
            .map(|p| {
                (
                    p.env.to_string(),
                    self.limits.get(p.field).unwrap_or(0).to_string(),
                )
            })
            .collect();
        if let Some(url) = self.rpc_endpoints.first() {
            env.insert("P0_RPC_URL".into(), url.clone());
        }
        if let Some(url) = self.wss_endpoints.first() {
            env.insert("P0_WS_URL".into(), url.clone());
        }
        env.insert("P0_MEASUREMENT_DURATION".into(), self.duration.clone());
        let seconds = duration_seconds(&self.duration)
            .expect("validated duration")
            .to_string();
        // DEC-05: the market observer's horizon is its own (`P0_MARKET_DURATION_SECS`, also
        // `until-stopped`); an econ4h run binds its market to the measurement plan.
        env.insert("P0_ECON_DURATION_SECS".into(), seconds);
        env.insert("P0_INTEGRATION_PROBE".into(), self.diagnostics.to_string());
        env
    }
}
/// The one duration parser of Project_0 (operational config, MeasurementPlan, `p0ctl`):
/// a positive integer with `s`, `m`, `h` or `d`, at most what four exact nanosecond snapshot
/// offsets can represent. Open-ended horizons are not durations: `until-stopped`/`inf` are
/// refused here, never mapped to a huge number (`RunHorizon::UntilStopped`, `p0ctl run start
/// --until-stopped`, is a typed horizon of its own).
pub fn duration_seconds(s: &str) -> Result<u64, String> {
    if matches!(
        s,
        "until-stopped" | "until_stopped" | "inf" | "infinite" | "unbounded"
    ) {
        return Err(
            "RUN_HORIZON_UNTIL_STOPPED_NOT_AVAILABLE: only fixed durations (s/m/h/d) are supported"
                .into(),
        );
    }
    let at = s.len().checked_sub(1).ok_or("empty duration")?;
    let (n, u) = s.split_at_checked(at).ok_or("invalid duration")?;
    let scale = match u {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86400,
        _ => return Err("duration requires s/m/h/d".into()),
    };
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return Err("duration must be an unsigned integer".into());
    }
    let n = n
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(scale))
        .filter(|n| *n > 0 && *n <= u64::MAX / 4 / 1_000_000_000)
        .ok_or("duration overflow/zero")?;
    Ok(n)
}
fn profile(s: &str) -> Result<Profile, String> {
    Profile::parse(s).ok_or_else(|| "profile must be smoke|standard|performance|custom".into())
}

/// DEC-05: the profile each operating mode resolves unless `P0_PROFILE`, a config file's
/// `profile` or `--profile` selects one — the values the mode ran with before. Shadow keeps one
/// worker (B-2: provisional, measurement open); econ4h ran on Rayon's default pool of one worker
/// per available CPU (`performance`); the market observer uses no worker pool.
pub fn mode_profile(mode: &str) -> Result<Profile, String> {
    match mode {
        "shadow" | "market" => Ok(Profile::Standard),
        "econ4h" => Ok(Profile::Performance),
        other => Err(format!(
            "OPERATING_MODE_UNKNOWN: {other} (shadow|econ4h|market)"
        )),
    }
}

/// An operational config file (`--config` / `P0_OPERATIONAL_CONFIG_FILE`): the external file
/// format decoded straight into typed optional overrides. Unknown keys fail closed.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    profile: Option<String>,
    limits: Option<BTreeMap<String, u64>>,
    rpc_endpoints: Option<Vec<String>>,
    wss_endpoints: Option<Vec<String>>,
    non_landing_sources: Option<Vec<String>>,
    hold_b_min_attempts: Option<u64>,
    jito_block_engine_url: Option<String>,
    bundle_sim_rpc_url: Option<String>,
    jito_tip_account: Option<String>,
    fallback: Option<bool>,
    adaptive: Option<bool>,
    duration: Option<String>,
    verification: Option<String>,
    diagnostics: Option<bool>,
    fe_mode: Option<String>,
    fe_targets: Option<Vec<String>>,
    fe_group: Option<String>,
    threshold_trigger: Option<bool>,
    c0_census: Option<bool>,
    source_geyser: Option<String>,
    source_geyser_provider: Option<String>,
}
impl ConfigFile {
    /// A config file: only the native text format (`key = value`). A JSON config is refused
    /// (JFI-01, P0-PROD-CTRL-02: no JSON configuration handoff exists).
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') {
            return Err("CONFIG_JSON_REFUSED: the operational config is the native text format (key = value)".into());
        }
        let text = std::str::from_utf8(bytes).map_err(|_| "config file is not text")?;
        Self::decode_text(text)
    }
    fn decode_text(text: &str) -> Result<Self, String> {
        let mut f = Self::default();
        let list = |v: &str| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect::<Vec<_>>()
        };
        for (key, value) in parse_lines(text)? {
            match key.as_str() {
                "profile" => f.profile = Some(value),
                "duration" => f.duration = Some(value),
                "verification" => f.verification = Some(value),
                "fe_mode" => f.fe_mode = Some(value),
                "fe_group" => f.fe_group = Some(value),
                "source_geyser" => f.source_geyser = Some(value),
                "source_geyser_provider" => f.source_geyser_provider = Some(value),
                "fallback" => f.fallback = Some(on_off(&value)?),
                "adaptive" => f.adaptive = Some(on_off(&value)?),
                "diagnostics" => f.diagnostics = Some(on_off(&value)?),
                "threshold_trigger" => f.threshold_trigger = Some(on_off(&value)?),
                "c0_census" => f.c0_census = Some(on_off(&value)?),
                "rpc_endpoints" => f.rpc_endpoints = Some(list(&value)),
                "wss_endpoints" => f.wss_endpoints = Some(list(&value)),
                "non_landing_sources" => f.non_landing_sources = Some(list(&value)),
                "hold_b_min_attempts" => {
                    f.hold_b_min_attempts = Some(number("hold_b_min_attempts", &value)?)
                }
                "jito_block_engine_url" => f.jito_block_engine_url = Some(value),
                "bundle_sim_rpc_url" => f.bundle_sim_rpc_url = Some(value),
                "jito_tip_account" => f.jito_tip_account = Some(value),
                "fe_targets" => f.fe_targets = Some(list(&value)),
                field if Limits::default().get(field).is_some() => {
                    f.limits
                        .get_or_insert_with(BTreeMap::new)
                        .insert(field.to_string(), number(field, &value)?);
                }
                other => return Err(format!("unknown config field {other}")),
            }
        }
        Ok(f)
    }
}
impl Resolved {
    /// The configuration of a tool outside the operating modes (profile `standard` unless one is
    /// selected). Precedence: profile < environment < config file < CLI. Unknown keys fail.
    pub fn parse(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        Self::resolve(None, args, env)
    }
    /// DEC-05: the configuration of operating mode `mode` (shadow|econ4h|market): its profile
    /// ([`mode_profile`]) unless `P0_PROFILE`, a config file or `--profile` selects one; the
    /// profile's source names the mode (`mode:econ4h`). Same precedence as [`Self::parse`].
    pub fn for_mode(
        mode: &str,
        args: &[String],
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Self, String> {
        Self::resolve(Some(mode), args, env)
    }
    fn resolve(
        mode: Option<&str>,
        args: &[String],
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Self, String> {
        // A blank variable is unset, as for every `config::require`/`parsed_or` reader.
        let env = |k: &str| env(k).filter(|v| !v.trim().is_empty());
        let mut cli = BTreeMap::new();
        let mut file = env("P0_OPERATIONAL_CONFIG_FILE");
        let (mut profile_name, profile_source) = match (env("P0_PROFILE"), mode) {
            (Some(name), _) => (name, "environment".to_string()),
            (None, Some(mode)) => (
                mode_profile(mode)?.name().to_string(),
                format!("mode:{mode}"),
            ),
            (None, None) => ("standard".to_string(), "profile:standard".to_string()),
        };
        let mut help = false;
        let mut print = false;
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--help" | "-h" => help = true,
                "--print-effective-config" => print = true,
                _ => {
                    let name = arg
                        .strip_prefix("--")
                        .ok_or("expected --parameter value")?
                        .replace('-', "_");
                    let value = it
                        .next()
                        .ok_or_else(|| format!("missing value for {arg}"))?
                        .clone();
                    if name == "config" {
                        file = Some(value);
                    } else {
                        if name == "profile" {
                            profile_name = value.clone();
                        }
                        if cli.insert(name, value).is_some() {
                            return Err("duplicate CLI parameter".into());
                        }
                    }
                }
            }
        }
        let file = if let Some(path) = file {
            if std::fs::metadata(Path::new(&path))
                .map_err(|_| "cannot stat config file")?
                .len()
                > 1024 * 1024
            {
                return Err("config file too large".into());
            }
            let bytes = std::fs::read(Path::new(&path)).map_err(|_| "cannot read config file")?;
            if bytes.len() > 1024 * 1024 {
                return Err("config file too large".into());
            }
            ConfigFile::decode(&bytes)?
        } else {
            ConfigFile::default()
        };
        if !cli.contains_key("profile") {
            if let Some(p) = &file.profile {
                profile_name = p.clone();
            }
        }
        let mut c = RuntimeConfig::profile(profile(&profile_name)?);
        let fe_mode = cli
            .get("fe_mode")
            .cloned()
            .or_else(|| file.fe_mode.clone())
            .or_else(|| env("P0_FE_MODE"))
            .unwrap_or_else(|| "smoke".into());
        c.fe_mode = fe_mode.clone();
        if fe_mode == "full" {
            c.limits.fe_calls = 1000;
            c.limits.fe_seconds = 300;
            c.limits.fe_max_mib = 512;
        }
        let mut sources = BTreeMap::new();
        for name in [
            "profile",
            "rpc_endpoints",
            "wss_endpoints",
            "non_landing_sources",
            "fallback",
            "adaptive",
            "duration",
            "verification",
            "diagnostics",
            "fe_mode",
            "fe_targets",
            "fe_group",
            "threshold_trigger",
            "c0_census",
            "source_geyser",
            "source_geyser_provider",
        ] {
            sources.insert(name.to_string(), format!("profile:{profile_name}"));
        }
        sources.insert("profile".into(), profile_source);
        if env("P0_FE_MODE").is_some() {
            sources.insert("fe_mode".into(), "environment".into());
        }
        for p in PARAMETERS {
            sources.insert(p.field.into(), format!("profile:{profile_name}"));
            if let Some(raw) = env(p.env) {
                let n = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid numeric environment field {}", p.field))?;
                c.limits.set(p.field, n);
                sources.insert(p.field.into(), "environment".into());
            }
        }
        if let Some(group) = env("P0_FE_GROUP").or_else(|| env("P0_GROUP_ID")) {
            c.fe_group = group;
            sources.insert("fe_group".into(), "environment".into());
        }
        if let Some(targets) = env("P0_FE_TARGETS") {
            c.fe_targets = targets.split(',').map(String::from).collect();
            sources.insert("fe_targets".into(), "environment".into());
        }
        if let Some(url) = env("P0_RPC_URL") {
            c.rpc_endpoints = vec![url];
            sources.insert("rpc_endpoints".into(), "environment".into());
        }
        if let Some(v) = env("P0_NON_LANDING_SOURCES").filter(|v| !v.trim().is_empty()) {
            c.non_landing_sources = v.split(',').map(|e| e.trim().to_string()).collect();
            sources.insert("non_landing_sources".into(), "environment".into());
        }
        if let Some(raw) = env("P0_HOLD_B_MIN_ATTEMPTS") {
            c.hold_b_min_attempts = Some(
                raw.trim()
                    .parse()
                    .map_err(|_| "invalid P0_HOLD_B_MIN_ATTEMPTS: expected a count".to_string())?,
            );
            sources.insert("hold_b_min_attempts".into(), "environment".into());
        }
        // DEC-11: the bundle path's endpoints and tip account.
        for (field, key) in [
            ("jito_block_engine_url", "P0_JITO_BLOCK_ENGINE_URL"),
            ("bundle_sim_rpc_url", "P0_BUNDLE_SIM_RPC_URL"),
            ("jito_tip_account", "P0_JITO_TIP_ACCOUNT"),
        ] {
            if let Some(v) = env(key).filter(|v| !v.trim().is_empty()) {
                *c.bundle_field(field) = Some(v.trim().to_string());
                sources.insert(field.into(), "environment".into());
            }
        }
        if let Some(url) = env("P0_WS_URL") {
            c.wss_endpoints = vec![url];
            sources.insert("wss_endpoints".into(), "environment".into());
        }
        // DEC-05: the owner's typed settings and the integration probe (`diagnostics`).
        for (field, key) in [
            ("diagnostics", "P0_INTEGRATION_PROBE"),
            ("threshold_trigger", "P0_USE_THRESHOLD_TRIGGER"),
            ("c0_census", "P0_C0_CENSUS"),
        ] {
            if let Some(raw) = env(key) {
                let on = on_off(&raw).map_err(|_| format!("invalid {key}: expected on/off"))?;
                *c.flag(field) = on;
                sources.insert(field.into(), "environment".into());
            }
        }
        for (field, key) in [
            ("source_geyser", "P0_SOURCE_GEYSER"),
            ("source_geyser_provider", "P0_SOURCE_GEYSER_PROVIDER"),
        ] {
            if let Some(value) = env(key) {
                *c.text(field) = value;
                sources.insert(field.into(), "environment".into());
            }
        }
        if let Some(duration) = env("P0_MEASUREMENT_DURATION")
            .or_else(|| env("P0_ECON_DURATION_SECS").map(|v| format!("{v}s")))
        {
            c.duration = duration;
            sources.insert("duration".into(), "environment".into());
        }
        // Config file over environment.
        if let Some(limits) = file.limits {
            for (f, value) in limits {
                if !c.limits.set(&f, value) {
                    return Err(format!("unknown config field {f}"));
                }
                sources.insert(f, "config_file".into());
            }
        }
        if let Some(p) = file.profile {
            c.profile = profile(&p).map_err(|_| "configuration type mismatch")?;
            sources.insert("profile".into(), "config_file".into());
        }
        // Empty endpoint arrays mean inherit local endpoints; private handoffs may be compiled
        // before the launcher's owner-only env.
        if let Some(v) = file.rpc_endpoints.filter(|v| !v.is_empty()) {
            c.rpc_endpoints = v;
            sources.insert("rpc_endpoints".into(), "config_file".into());
        }
        if let Some(v) = file.non_landing_sources.filter(|v| !v.is_empty()) {
            c.non_landing_sources = v;
            sources.insert("non_landing_sources".into(), "config_file".into());
        }
        if let Some(n) = file.hold_b_min_attempts {
            c.hold_b_min_attempts = Some(n);
            sources.insert("hold_b_min_attempts".into(), "config_file".into());
        }
        for (field, v) in [
            ("jito_block_engine_url", file.jito_block_engine_url),
            ("bundle_sim_rpc_url", file.bundle_sim_rpc_url),
            ("jito_tip_account", file.jito_tip_account),
        ] {
            if let Some(v) = v {
                *c.bundle_field(field) = Some(v);
                sources.insert(field.into(), "config_file".into());
            }
        }
        if let Some(v) = file.wss_endpoints.filter(|v| !v.is_empty()) {
            c.wss_endpoints = v;
            sources.insert("wss_endpoints".into(), "config_file".into());
        }
        macro_rules! file_field {
            ($($f:ident),*) => {$(
                if let Some(v) = file.$f {
                    c.$f = v;
                    sources.insert(stringify!($f).into(), "config_file".into());
                }
            )*};
        }
        file_field!(
            fallback,
            adaptive,
            duration,
            verification,
            diagnostics,
            fe_mode,
            fe_targets,
            fe_group,
            threshold_trigger,
            c0_census,
            source_geyser,
            source_geyser_provider
        );
        // CLI over everything.
        for (k, val) in cli {
            if Limits::default().get(&k).is_some() {
                let n = val
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --{}", k.replace('_', "-")))?;
                c.limits.set(&k, n);
            } else {
                let list = |v: &str| v.split(',').map(String::from).collect::<Vec<_>>();
                match k.as_str() {
                    "profile" => {
                        c.profile = profile(&val).map_err(|_| "configuration type mismatch")?
                    }
                    "duration" => c.duration = val.clone(),
                    "verification" => c.verification = val.clone(),
                    "fe_mode" => c.fe_mode = val.clone(),
                    "fe_group" => c.fe_group = val.clone(),
                    "rpc_endpoints" => c.rpc_endpoints = list(&val),
                    "non_landing_sources" => c.non_landing_sources = list(&val),
                    "hold_b_min_attempts" => {
                        c.hold_b_min_attempts =
                            Some(val.parse().map_err(|_| "configuration type mismatch")?)
                    }
                    "jito_block_engine_url" | "bundle_sim_rpc_url" | "jito_tip_account" => {
                        *c.bundle_field(&k) = Some(val.clone())
                    }
                    "wss_endpoints" => c.wss_endpoints = list(&val),
                    "fe_targets" => c.fe_targets = list(&val),
                    "fallback" => c.fallback = on_off(&val)?,
                    "adaptive" => c.adaptive = on_off(&val)?,
                    "diagnostics" => c.diagnostics = on_off(&val)?,
                    "threshold_trigger" | "c0_census" => *c.flag(&k) = on_off(&val)?,
                    "source_geyser" | "source_geyser_provider" => *c.text(&k) = val.clone(),
                    _ => return Err(format!("unknown --{}", k.replace('_', "-"))),
                }
            }
            sources.insert(k, "cli".into());
        }
        let config = c;
        config.validate()?;
        if fe_mode == "full" {
            for f in ["fe_calls", "fe_seconds", "fe_max_mib"] {
                if sources[f].starts_with("profile:") {
                    sources.insert(f.into(), "fe_mode:full".into());
                }
            }
        }
        Ok(Self {
            config,
            sources,
            print,
            help,
        })
    }
    pub fn activate(self) -> Result<(), String> {
        let effective = self.effective();
        install(self.config)?;
        EFFECTIVE
            .set(effective)
            .map_err(|_| "runtime report already initialized".to_string())
    }
    /// Typed, secret-free record of this configuration (JFI-04).
    pub fn effective(&self) -> EffectiveConfig {
        let c = &self.config;
        EffectiveConfig {
            schema: "Project0/OperationalConfig/2".into(),
            profile: c.profile.clone(),
            limits: c.limits.clone(),
            rpc_endpoint_count: c.rpc_endpoints.len() as u64,
            wss_endpoint_count: c.wss_endpoints.len() as u64,
            fallback: c.fallback,
            adaptive: c.adaptive,
            duration: c.duration.clone(),
            verification: c.verification.clone(),
            diagnostics: c.diagnostics,
            fe_mode: c.fe_mode.clone(),
            fe_targets: c.fe_targets.clone(),
            fe_group: c.fe_group.clone(),
            sources: self.sources.clone(),
            provider_capacity_qualified: false,
            free_only: true,
            live_authorized: false,
            submission_authorized: false,
            threshold_trigger: c.threshold_trigger,
            c0_census: c.c0_census,
            source_geyser: c.source_geyser.clone(),
            source_geyser_provider: c.source_geyser_provider.clone(),
        }
    }
    pub fn help() -> String {
        let mut s=String::from("p0-longtail [--profile smoke|standard|performance|custom] [--config FILE] [--duration 10m|1h|5h|24h]\n--rpc-endpoints URL[,URL] --wss-endpoints URL[,URL] (priority order; declared FREE_ONLY)\n--non-landing-sources OPERATOR=URL[,OPERATOR=URL] (P0_NON_LANDING_SOURCES; DEC-03 FREE_ONLY B sources)\n--hold-b-min-attempts N (P0_HOLD_B_MIN_ATTEMPTS; DEC-04 B-8, no default: B ACTIVE blocked while unset)\n--jito-block-engine-url URL --bundle-sim-rpc-url URL --jito-tip-account PUBKEY (P0_JITO_BLOCK_ENGINE_URL, P0_BUNDLE_SIM_RPC_URL, P0_JITO_TIP_ACCOUNT; DEC-11 bundle path, never sends)\n--fe-mode smoke|full --fe-targets PUBKEY[,PUBKEY] --fe-group PUBKEY\n--fallback on|off --adaptive on|off --diagnostics on|off (P0_INTEGRATION_PROBE) --verification off|index|strict|crosscheck --print-effective-config\n--threshold-trigger on|off (P0_USE_THRESHOLD_TRIGGER) --c0-census on|off (P0_C0_CENSUS) --source-geyser off|observe (P0_SOURCE_GEYSER) --source-geyser-provider ID (P0_SOURCE_GEYSER_PROVIDER)\nProfile per operating mode (P0_MODE) unless selected: shadow and market standard, econ4h performance.\nPrecedence: profile < environment < config file < CLI. All changes require restart.\nFinancial approval is separate. Profiles do not prove provider capacity.\n");
        for p in PARAMETERS {
            s += &format!(
                "--{} <u64> [{}; default={}; {}..={}]\n",
                p.name(),
                p.unit,
                p.default,
                p.minimum(),
                p.maximum()
            );
        }
        s
    }
    /// Redacted human-readable effective configuration (`--print-effective-config`): every
    /// value with its source, type/unit, default and range; endpoints only as counts.
    pub fn text(&self) -> String {
        let e = self.effective();
        let src = |f: &str| {
            e.sources
                .get(f)
                .map_or("profile", String::as_str)
                .to_string()
        };
        let mut s = String::from("Project_0 effective operational configuration (secret-free)\n");
        s += &format!(
            "profile = {} [source {}]\n",
            e.profile.name(),
            src("profile")
        );
        s += &format!(
            "rpc-endpoints = {} configured (redacted) [source {}]\n",
            e.rpc_endpoint_count,
            src("rpc_endpoints")
        );
        s += &format!(
            "wss-endpoints = {} configured (redacted) [source {}]\n",
            e.wss_endpoint_count,
            src("wss_endpoints")
        );
        s += &format!(
            "non-landing-sources = {} configured (redacted) [source {}]\n",
            self.config.non_landing_sources.len(),
            src("non_landing_sources")
        );
        s += &match self.config.hold_b_min_attempts {
            Some(n) => format!(
                "hold-b-min-attempts = {n} [source {}]\n",
                src("hold_b_min_attempts")
            ),
            None => "hold-b-min-attempts = undecided (B-8: B ACTIVE blocked)\n".to_string(),
        };
        for (name, field, set) in [
            (
                "jito-block-engine-url",
                "jito_block_engine_url",
                self.config.jito_block_engine_url.is_some(),
            ),
            (
                "bundle-sim-rpc-url",
                "bundle_sim_rpc_url",
                self.config.bundle_sim_rpc_url.is_some(),
            ),
        ] {
            s += &if set {
                format!("{name} = configured (redacted) [source {}]\n", src(field))
            } else {
                format!("{name} = unset\n")
            };
        }
        s += &match &self.config.jito_tip_account {
            Some(tip) => format!(
                "jito-tip-account = {tip} [source {}]\n",
                src("jito_tip_account")
            ),
            None => "jito-tip-account = unset\n".to_string(),
        };
        for (name, value) in [
            ("fallback", e.fallback.to_string()),
            ("adaptive", e.adaptive.to_string()),
            ("duration", e.duration.clone()),
            ("verification", e.verification.clone()),
            ("diagnostics", e.diagnostics.to_string()),
            ("fe_mode", e.fe_mode.clone()),
            ("fe_targets", e.fe_targets.join(",")),
            ("fe_group", e.fe_group.clone()),
            ("threshold_trigger", e.threshold_trigger.to_string()),
            ("c0_census", e.c0_census.to_string()),
            ("source_geyser", e.source_geyser.clone()),
            ("source_geyser_provider", e.source_geyser_provider.clone()),
        ] {
            s += &format!(
                "{} = {value} [source {}]\n",
                name.replace('_', "-"),
                src(name)
            );
        }
        for p in PARAMETERS {
            let provisional = p
                .provisional()
                .map(|q| format!("; provisional, measurement open ({q})"))
                .unwrap_or_default();
            s += &format!(
                "--{} = {} {} [default {}; {}..={}; env {}; source {}; {} required{provisional}]\n",
                p.name(),
                e.limits.get(p.field).unwrap_or(0),
                p.unit,
                p.default,
                p.minimum(),
                p.maximum(),
                p.env,
                src(p.field),
                p.change_rule()
            );
        }
        s += "free_only=true provider_capacity_qualified=false live_authorized=false submission_authorized=false\n";
        s
    }
}
static ACTIVE: OnceLock<RuntimeConfig> = OnceLock::new();
static EFFECTIVE: OnceLock<EffectiveConfig> = OnceLock::new();
/// The activated configuration's typed record (`None` when running on defaults).
pub fn effective() -> Option<&'static EffectiveConfig> {
    EFFECTIVE.get()
}

/// Secret-free effective operational configuration: resolved values (endpoint lists reduced to
/// their counts), the source of every value, and the fixed authorization facts. Persisted as a
/// typed journal record; never an authorization by itself.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectiveConfig {
    pub schema: String,
    pub profile: Profile,
    pub limits: Limits,
    pub rpc_endpoint_count: u64,
    pub wss_endpoint_count: u64,
    pub fallback: bool,
    pub adaptive: bool,
    pub duration: String,
    pub verification: String,
    pub diagnostics: bool,
    pub fe_mode: String,
    pub fe_targets: Vec<String>,
    pub fe_group: String,
    pub sources: BTreeMap<String, String>,
    pub provider_capacity_qualified: bool,
    pub free_only: bool,
    pub live_authorized: bool,
    pub submission_authorized: bool,
    /// DEC-05 (from [`OWNER_SETTINGS_REVISION`]): the owner's typed settings.
    pub threshold_trigger: bool,
    pub c0_census: bool,
    pub source_geyser: String,
    pub source_geyser_provider: String,
}
pub fn configured() -> bool {
    ACTIVE.get().is_some()
}
pub fn active() -> &'static RuntimeConfig {
    static DEFAULT: OnceLock<RuntimeConfig> = OnceLock::new();
    ACTIVE
        .get()
        .unwrap_or_else(|| DEFAULT.get_or_init(RuntimeConfig::default))
}
pub fn install(config: RuntimeConfig) -> Result<(), String> {
    config.validate()?;
    ACTIVE
        .set(config)
        .map_err(|_| "runtime config already initialized".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Resolved, String> {
        Resolved::parse(
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            &|_| None,
        )
    }
    #[test]
    fn profiles_and_overrides_are_ceilings_not_authorization() {
        for name in ["smoke", "standard", "performance", "custom"] {
            let r = parse(&[
                "--profile",
                name,
                "--rpc-rps",
                "50",
                "--rpc-concurrency",
                "8",
                "--duration",
                "25h",
            ])
            .unwrap();
            assert_eq!(r.config.limits.rpc_rps, 50);
            assert_eq!(r.config.limits.rpc_concurrency, 8);
            assert_eq!(duration_seconds(&r.config.duration).unwrap(), 90000);
            let e = r.effective();
            assert!(
                !(e.live_authorized || e.submission_authorized || e.provider_capacity_qualified)
            );
            assert!(!r.config.environment().contains_key("P0_MODE"));
        }
    }
    #[test]
    fn profile_files_are_the_only_profile_source_and_cannot_authorize() {
        // Values formerly hard-coded in RuntimeConfig::profile, now read from config/profiles/.
        let standard = RuntimeConfig::default();
        let smoke = RuntimeConfig::profile(Profile::Smoke);
        assert_eq!(
            (
                smoke.limits.rpc_rps,
                smoke.limits.rpc_burst,
                smoke.limits.rpc_concurrency
            ),
            (1, 1, 1)
        );
        assert_eq!(
            (
                smoke.limits.rpc_attempts,
                smoke.limits.max_response_mib,
                smoke.limits.max_total_mib
            ),
            (1, 16, 64)
        );
        assert_eq!(smoke.duration, "60s");
        let mut rest = smoke.limits.clone();
        (rest.rpc_rps, rest.rpc_burst, rest.rpc_concurrency) = (5, 1, 2);
        (rest.rpc_attempts, rest.max_response_mib, rest.max_total_mib) = (3, 512, 4096);
        assert_eq!(rest, standard.limits);
        for (profile, expected) in [(Profile::Standard, &standard), (Profile::Custom, &standard)] {
            let c = RuntimeConfig::profile(profile.clone());
            assert_eq!(
                (c.limits, c.duration, c.profile),
                (expected.limits.clone(), "4h".into(), profile)
            );
        }
        let performance = RuntimeConfig::profile(Profile::Performance);
        assert_eq!(
            performance.limits.workers,
            std::thread::available_parallelism().map_or(1, |n| n.get() as u64)
        );
        for (name, text) in PROFILE_FILES {
            let file = ProfileFile::parse(text).unwrap();
            assert_eq!(file.profile.name(), name);
            RuntimeConfig::profile(file.profile).validate().unwrap();
        }
        let head = "schema = Project0/RuntimeProfile/2\nprofile = smoke\ndescription = x\n";
        ProfileFile::parse(head).unwrap();
        for forbidden in [
            format!("{head}rpc-endpoints = https://x"),
            format!("{head}live = on"),
            format!("{head}submission_enabled = true"),
            format!("{head}min_profit = 0"),
            format!("{head}rpc-rps = 1\nrpc_rps = 2"),
            format!("{head}workers = all"),
            head.replace("/2", "/1"),
            head.replace("smoke", "live"),
            // The former JSON profile format is no longer a profile source.
            r#"{"schema":"Project0/RuntimeProfile/2","profile":"smoke","description":"x"}"#.into(),
        ] {
            assert!(ProfileFile::parse(&forbidden).is_err(), "{forbidden}");
        }
    }
    #[test]
    fn config_examples_parse_with_the_real_parser() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../config/examples");
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if !(name.starts_with("operational_") && name.ends_with(".conf")) {
                continue;
            }
            let env = |k: &str| (k == "P0_RPC_URL").then(|| "https://rpc.invalid/".to_string());
            let r =
                Resolved::parse(&["--config".into(), path.display().to_string()], &env).unwrap();
            assert_eq!(r.config.rpc_endpoints, ["https://rpc.invalid/"], "{name}");
            assert_eq!(r.sources["rpc_endpoints"], "environment", "{name}");
            let e = r.effective();
            assert!(!(e.live_authorized || e.submission_authorized));
            seen += 1;
        }
        assert_eq!(seen, 2);
    }
    /// DEC-11: the bundle path's settings from environment and CLI, validated (http(s) URLs with a
    /// host, a base58 tip account) and printed redacted (the tip account is public).
    #[test]
    fn dec11_bundle_settings_parse_validate_and_redact() {
        let env = |k: &str| match k {
            "P0_RPC_URL" => Some("https://rpc.invalid/".to_string()),
            "P0_JITO_BLOCK_ENGINE_URL" => Some("https://jito.invalid/SECRET".to_string()),
            "P0_BUNDLE_SIM_RPC_URL" => Some("https://sim.invalid/KEY".to_string()),
            _ => None,
        };
        let tip = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        let r = Resolved::parse(&["--jito-tip-account".into(), tip.clone()], &env).unwrap();
        assert_eq!(
            r.config.jito_block_engine_url.as_deref(),
            Some("https://jito.invalid/SECRET")
        );
        assert_eq!(
            r.config.bundle_sim_rpc_url.as_deref(),
            Some("https://sim.invalid/KEY")
        );
        assert_eq!(r.config.jito_tip_account.as_deref(), Some(tip.as_str()));
        assert_eq!(r.sources["jito_tip_account"], "cli");
        let printed = r.text();
        assert!(
            !printed.contains("SECRET") && !printed.contains("KEY"),
            "{printed}"
        );
        assert!(
            printed.contains("jito-block-engine-url = configured (redacted) [source environment]")
        );
        assert!(printed.contains(&format!("jito-tip-account = {tip} [source cli]")));
        let unset = Resolved::parse(&[], &|k: &str| {
            (k == "P0_RPC_URL").then(|| "https://rpc.invalid/".to_string())
        })
        .unwrap();
        assert!(unset.text().contains("bundle-sim-rpc-url = unset"));
        for bad in [
            ["--jito-block-engine-url", "ftp://jito.invalid"],
            ["--bundle-sim-rpc-url", "not a url"],
            ["--jito-tip-account", "not-a-key"],
        ] {
            assert!(parse(&bad).is_err(), "{bad:?}");
        }
        assert!(parse(&["--jito-tip-account", &tip]).is_ok());
    }
    /// DEC-04 (B-8): N has no default (undecided keeps B in OBSERVE), comes from environment,
    /// file or CLI, is at least 1 and is printed with its source.
    #[test]
    fn dec04_hold_b_min_attempts_has_no_default() {
        let env = |k: &str| (k == "P0_RPC_URL").then(|| "https://rpc.invalid/".to_string());
        let r = Resolved::parse(&[], &env).unwrap();
        assert_eq!(r.config.hold_b_min_attempts, None);
        assert!(r
            .text()
            .contains("hold-b-min-attempts = undecided (B-8: B ACTIVE blocked)"));
        let r = Resolved::parse(&["--hold-b-min-attempts".into(), "5".into()], &env).unwrap();
        assert_eq!(r.config.hold_b_min_attempts, Some(5));
        assert!(r.text().contains("hold-b-min-attempts = 5 [source cli]"));
        let env7 = |k: &str| match k {
            "P0_RPC_URL" => Some("https://rpc.invalid/".to_string()),
            "P0_HOLD_B_MIN_ATTEMPTS" => Some("7".to_string()),
            _ => None,
        };
        let r = Resolved::parse(&[], &env7).unwrap();
        assert_eq!(
            (
                r.config.hold_b_min_attempts,
                r.sources["hold_b_min_attempts"].as_str()
            ),
            (Some(7), "environment")
        );
        for bad in ["0", "-1", "x"] {
            assert!(parse(&["--hold-b-min-attempts", bad]).is_err(), "{bad}");
        }
    }
    /// DEC-03: the non-landing B sources (`operator=URL`) from environment and CLI in the common
    /// precedence, validated (a lowercase operator label of at most 32, an http(s) URL with a
    /// host) and printed only as a count.
    #[test]
    fn dec03_non_landing_sources_parse_validate_and_redact() {
        let env = |k: &str| match k {
            "P0_RPC_URL" => Some("https://rpc.invalid/".to_string()),
            "P0_NON_LANDING_SOURCES" => {
                Some("op-a=https://a.invalid/KEY, op-b=http://b.invalid".to_string())
            }
            _ => None,
        };
        let r = Resolved::parse(&[], &env).unwrap();
        assert_eq!(
            r.config.non_landing_source_list().unwrap(),
            [
                ("op-a".to_string(), "https://a.invalid/KEY".to_string()),
                ("op-b".to_string(), "http://b.invalid".to_string())
            ]
        );
        assert_eq!(r.sources["non_landing_sources"], "environment");
        let printed = r.text();
        assert!(!printed.contains("KEY"), "{printed}");
        assert!(
            printed.contains("non-landing-sources = 2 configured (redacted) [source environment]")
        );
        let r = Resolved::parse(
            &[
                "--non-landing-sources".into(),
                "op-c=https://c.invalid".into(),
            ],
            &env,
        )
        .unwrap();
        assert_eq!(r.config.non_landing_sources, ["op-c=https://c.invalid"]);
        assert_eq!(r.sources["non_landing_sources"], "cli");
        assert!(parse(&["--non-landing-sources", "op-1=https://a.invalid"]).is_ok());
        // One operator may run several endpoints of one host.
        assert!(parse(&[
            "--non-landing-sources",
            "op-a=https://a.invalid/1,op-a=https://a.invalid/2"
        ])
        .is_ok());
        let long = format!("{}=https://a.invalid", "a".repeat(33));
        for bad in [
            "https://a.invalid",
            "OP=https://a.invalid",
            "=https://a.invalid",
            "op=ftp://a.invalid",
            "op=not a url",
            long.as_str(),
            "op-a=https://a.invalid,op-a=https://a.invalid",
            "op-a=https://a.invalid/1,op-b=https://A.invalid/2",
        ] {
            assert!(parse(&["--non-landing-sources", bad]).is_err(), "{bad}");
        }
    }
    #[test]
    fn rejects_invalid_types_unknown_financial_keys_and_resource_ranges() {
        for args in [
            vec!["--rpc-rps", "0"],
            vec!["--rpc-concurrency", "-1"],
            vec!["--rpc-queue", "4294967296"],
            vec!["--duration", "+1s"],
            vec!["--duration", "18446744073709551615h"],
            vec!["--live", "on"],
            vec!["--rpc-rps", "2", "--rpc-rps", "3"],
            vec!["--rpc-backoff-ms", "100", "--rpc-backoff-max-ms", "1"],
            vec!["--verification", "unchecked"],
            vec!["--rpc-endpoints", "file:///tmp/no"],
            vec!["--boot-seconds", "301"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
    #[test]
    fn precedence_redaction_private_handoff_and_metadata() {
        let dir = std::env::temp_dir().join(format!("p0-runtime-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("input.conf");
        std::fs::write(
            &path,
            "# override\nprofile = smoke\nrpc-rps = 7\nrpc-endpoints = https://example.test/SECRET?key=PRIVATE\n",
        )
        .unwrap();
        let r = Resolved::parse(
            &[
                "--config".into(),
                path.display().to_string(),
                "--rpc-rps".into(),
                "11".into(),
            ],
            &|key| match key {
                "P0_RPC_RPS" => Some("5".into()),
                "P0_RPC_CONCURRENCY" => Some("4".into()),
                _ => None,
            },
        )
        .unwrap();
        assert_eq!(r.config.limits.rpc_rps, 11);
        assert_eq!(r.sources["rpc_rps"], "cli");
        assert_eq!(r.config.limits.rpc_concurrency, 4);
        assert_eq!(r.sources["rpc_concurrency"], "environment");
        assert_eq!(r.config.limits.rpc_attempts, 1);
        let printed = r.text();
        assert!(!printed.contains("SECRET"));
        assert!(!printed.contains("PRIVATE"));
        // JFI-01/P0-PROD-CTRL-02: no JSON configuration handoff exists; a JSON file is refused.
        std::fs::write(&path, br#"{"rpc_rps": 2}"#).unwrap();
        assert!(parse(&["--config", path.to_str().unwrap()])
            .unwrap_err()
            .starts_with("CONFIG_JSON_REFUSED"));
        assert!(parse(&["--export-environment", "x.json"]).is_err());
        for (text, err) in [
            ("rpc-rps = 2\nrpc_rps = 3", "duplicate config key rpc_rps"),
            ("live = on", "unknown config field live"),
            ("rpc-rps = many", "invalid --rpc-rps"),
            ("fallback = maybe", "expected on/off"),
            ("rpc-rps 2", "config line 1: expected key = value"),
        ] {
            std::fs::write(&path, text).unwrap();
            let got = parse(&["--config", path.to_str().unwrap()]).unwrap_err();
            assert_eq!(got, err, "{text}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    /// The terminal view is text: every parameter with its active value and source, endpoints
    /// only as counts, no secret and no machine document.
    #[test]
    fn effective_config_text_is_redacted_and_complete() {
        let r = parse(&[
            "--rpc-endpoints",
            "https://example.test/SECRET?key=PRIVATE",
            "--rpc-rps",
            "9",
        ])
        .unwrap();
        let t = r.text();
        assert!(!t.contains("SECRET") && !t.contains("PRIVATE") && !t.contains('{'));
        assert!(t.contains("rpc-endpoints = 1 configured (redacted) [source cli]"));
        assert!(t.contains("--rpc-rps = 9 requests/s [default 5; 1..=4294967295; env P0_RPC_RPS; source cli; restart required]"));
        for p in PARAMETERS {
            assert!(t.contains(&format!("--{} = ", p.name())), "{}", p.field);
        }
        assert!(t.contains("live_authorized=false submission_authorized=false"));
    }
    /// Every record that embeds `Limits` changes its binary layout with the field list: a new
    /// parameter must bump `LIMITS_REVISION` (and with it the payload schemas) and update this pin.
    #[test]
    fn limits_layout_is_pinned_to_its_revision() {
        let names = PARAMETERS
            .iter()
            .map(|p| p.field)
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(
            (
                LIMITS_REVISION,
                PARAMETERS.len(),
                solana_sdk::hash::hash(names.as_bytes()).to_string()
            ),
            (
                12,
                141,
                "5g2uUNCnr1jdNs7x6Snrt1SybJJekaRfAzvZUqAjjkf6".to_string()
            )
        );
    }
    /// V2-08 (P3-08/P4-04): the owner's sweep batch and ingress burst bounds are typed
    /// parameters — a profile/CLI/config value reaches `Limits` (before: unknown parameter, only
    /// a bare environment read with literal defaults in the owner setup).
    #[test]
    fn v2_08_sweep_and_burst_bounds_are_typed_parameters() {
        let r = parse(&[
            "--sweep-batch-max-events",
            "32",
            "--sweep-batch-max-age-ns",
            "2000000",
            "--ingress-burst-before-detect",
            "16",
        ])
        .unwrap();
        let l = &r.config.limits;
        assert_eq!(
            (
                l.sweep_batch_max_events,
                l.sweep_batch_max_age_ns,
                l.ingress_burst_before_detect
            ),
            (32, 2_000_000, 16)
        );
        let env = r.config.environment();
        for (k, v) in [
            ("P0_SWEEP_BATCH_MAX_EVENTS", "32"),
            ("P0_SWEEP_BATCH_MAX_AGE_NS", "2000000"),
            ("P0_INGRESS_BURST_BEFORE_DETECT", "16"),
        ] {
            assert!(env.iter().any(|(ek, ev)| ek == k && ev == v), "{k}");
        }
        assert!(parse(&["--sweep-batch-max-events", "0"]).is_err());
    }
    /// D01–D06/D11: the owner, FE-04, WSS, Jupiter, lane, winner-watch, deployment, reporter and
    /// market bounds are typed parameters — CLI values reach `Limits` and the exported
    /// environment (before: literals or a second default per consumer); each bound is enforced.
    #[test]
    fn d01_d06_operational_bounds_are_typed_parameters() {
        let r = parse(&[
            "--blockhash-max-age-ms",
            "15000",
            "--fe04-settle-retry-ms",
            "20000",
            "--wss-healthy-reset-ms",
            "4000",
            "--jupiter-timeout-ms",
            "900",
            "--lane-weight-l0",
            "5",
            "--market-retention",
            "1000",
        ])
        .unwrap();
        let l = &r.config.limits;
        assert_eq!(
            (
                l.blockhash_max_age_ms,
                l.fe04_settle_retry_ms,
                l.wss_healthy_reset_ms,
                l.jupiter_timeout_ms,
                l.lane_weight_l0,
                l.market_retention
            ),
            (15_000, 20_000, 4_000, 900, 5, 1_000)
        );
        let env = r.config.environment();
        for (k, v) in [
            ("P0_BLOCKHASH_MAX_AGE_MS", "15000"),
            ("P0_LANE_WEIGHT_L0", "5"),
            ("P0_MARKET_RETENTION", "1000"),
        ] {
            assert!(env.iter().any(|(ek, ev)| ek == k && ev == v), "{k}");
        }
        assert!(parse(&["--lane-weight-l1", "0"]).is_err());
        assert!(parse(&["--market-retention", "2000001"]).is_err());
        assert!(parse(&["--econ-queue-capacity", "1000001"]).is_err());
        // One default authority: the market's ready timeout is the registry's.
        assert_eq!(
            Limits::default().stream_ready_seconds,
            defaults::STREAM_READY_SECONDS
        );
    }
    #[test]
    fn explicit_endpoint_beats_legacy_audit_alias() {
        let r = parse(&["--rpc-endpoints", "https://explicit.invalid"]).unwrap();
        assert_eq!(
            r.config
                .rpc_url(&|_| Some("https://legacy.invalid".into()))
                .as_deref(),
            Some("https://explicit.invalid")
        );
        assert_eq!(
            RuntimeConfig::default()
                .rpc_url(&|k| (k == "P0_AUDIT_RPC_URL").then(|| "https://audit.invalid".into()))
                .as_deref(),
            Some("https://audit.invalid")
        );
    }
    #[test]
    fn full_mode_has_its_own_defaults_and_shared_explicit_budgets() {
        let smoke = parse(&[]).unwrap();
        assert_eq!(smoke.config.limits.fe_calls, 3);
        let full = parse(&["--fe-mode", "full"]).unwrap();
        assert_eq!(full.config.limits.fe_calls, 1000);
        assert_eq!(full.sources["fe_calls"], "fe_mode:full");
        let full = parse(&[
            "--fe-mode",
            "full",
            "--fe-calls",
            "1500",
            "--fe-seconds",
            "45",
            "--fe-batch-size",
            "17",
        ])
        .unwrap();
        assert_eq!(full.config.limits.fe_calls, 1500);
        assert_eq!(full.config.limits.fe_batch_size, 17);
        assert!(parse(&["--fe-mode", "full", "--fe-batch-size", "101"]).is_err());
        assert!(parse(&["--fe-targets", "invalid"]).is_err());
    }
    #[test]
    fn numeric_schema_covers_every_environment_consumer() {
        let r = parse(&[]).unwrap();
        let env = r.config.environment();
        let fields = serde_json::to_value(&r.config.limits).unwrap();
        assert_eq!(PARAMETERS.len(), fields.as_object().unwrap().len());
        for p in PARAMETERS {
            assert_eq!(
                env[p.env],
                r.config.limits.get(p.field).unwrap().to_string()
            );
            assert!(Resolved::help().contains(&p.name()));
        }
    }

    fn mode(mode: &str, args: &[&str], env: &[(&str, &str)]) -> Result<Resolved, String> {
        Resolved::for_mode(
            mode,
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            &|k| {
                env.iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            },
        )
    }

    /// DEC-05: every operating mode resolves exactly one profile, econ4h without arguments
    /// included; `P0_PROFILE`, a config file or `--profile` select another one.
    #[test]
    fn dec05_every_operating_mode_resolves_its_profile() {
        let cpus = std::thread::available_parallelism().map_or(1, |n| n.get() as u64);
        for (m, profile, workers) in [
            ("shadow", Profile::Standard, 1),
            ("econ4h", Profile::Performance, cpus),
            ("market", Profile::Standard, 1),
        ] {
            let r = mode(m, &[], &[]).unwrap();
            assert_eq!(r.config.profile, profile, "{m}");
            assert_eq!(r.config.limits.workers, workers, "{m}");
            assert_eq!(r.sources["profile"], format!("mode:{m}"));
            assert_eq!(r.sources["workers"], format!("profile:{}", profile.name()));
        }
        let r = mode("econ4h", &[], &[("P0_PROFILE", "standard")]).unwrap();
        assert_eq!(
            (r.config.profile, r.config.limits.workers),
            (Profile::Standard, 1)
        );
        assert_eq!(r.sources["profile"], "environment");
        let r = mode("shadow", &["--profile", "performance"], &[]).unwrap();
        assert_eq!(r.config.limits.workers, cpus);
        assert_eq!(r.sources["profile"], "cli");
        assert!(mode("live", &[], &[])
            .unwrap_err()
            .starts_with("OPERATING_MODE_UNKNOWN"));
        // A tool outside the modes keeps profile standard.
        assert_eq!(parse(&[]).unwrap().config.profile, Profile::Standard);
    }

    /// DEC-05 parity: the migration changes no effective value. The owner parameters keep the
    /// defaults of their former call sites (`config::parsed_or` in `state_owner/setup.rs` and the
    /// literal blockhash refresh at b3cce8ee); every other field keeps its revision-10 default;
    /// econ4h keeps Rayon's default pool (one worker per CPU, formerly unconfigured), Shadow
    /// one worker; the typed owner settings keep their former defaults.
    #[test]
    fn dec05_migration_keeps_every_effective_value() {
        let former = [
            ("max_total_capital", 0),
            ("max_outstanding_inventory", 0),
            ("max_concurrent_positions", 1),
            ("max_exposure_per_attempt", 0),
            ("max_daily_loss", 0),
            ("max_failed_attempts_per_day", 0),
            ("max_tip", 0),
            ("max_slippage_bps", 0),
            ("max_outstanding_attempts", 1),
            ("min_profit_usd_micro", 0),
            ("jupiter_max_accounts", 64),
            ("compute_unit_limit", 1_400_000),
            ("priority_micro_lamports_per_cu", 0),
            ("loaded_accounts_data_size", 64 * 1024 * 1024),
            ("max_opportunity_age_ns", 2_000_000_000),
            ("execution_deadline_ns", 2_000_000_000),
            ("integration_probe_limit_per_kind", 8),
            ("threshold_bootstrap_batch", 16),
            ("candidate_recheck_slots", 1),
            ("candidate_error_recheck_slots", 1),
            ("candidate_repeat_soft_cap", 1024),
            ("rpc_candidates_per_drain", 4),
            ("candidate_network_deadline_ms", 5_000),
            ("mt0_interval_seconds", 10),
            ("blockhash_refresh_ms", 10_000),
            ("recovery_root_catchup_seconds", 120),
            ("recovery_root_stall_seconds", 60),
            ("recovery_root_hard_max_seconds", 300),
        ];
        let added: Vec<_> = PARAMETERS
            .iter()
            .filter(|p| introduced(p.field) == 11)
            .map(|p| p.field)
            .collect();
        assert_eq!(added, former.map(|(f, _)| f));
        let cpus = std::thread::available_parallelism().map_or(1, |n| n.get() as u64);
        let unset = Limits::default();
        for (m, workers) in [("shadow", 1), ("econ4h", cpus)] {
            let r = mode(m, &[], &[("P0_MEASUREMENT_DURATION", "10m")]).unwrap();
            for (field, value) in former {
                assert_eq!(r.config.limits.get(field), Some(value), "{m} {field}");
            }
            // Revision 10 (before DEC-05): every field equals what that mode ran with.
            for field in fields_at(10).unwrap() {
                let before = if field == "workers" {
                    workers
                } else {
                    unset.get(field).unwrap()
                };
                assert_eq!(r.config.limits.get(field), Some(before), "{m} {field}");
            }
            let d = RuntimeConfig::default();
            assert_eq!(
                (
                    r.config.threshold_trigger,
                    r.config.c0_census,
                    r.config.diagnostics
                ),
                (false, false, false)
            );
            assert_eq!(r.config.source_geyser, "off");
            assert_eq!(r.config.source_geyser_provider, d.source_geyser_provider);
            assert_eq!(r.config.duration, "10m");
        }
    }

    /// DEC-05: the owner parameters are validated in the registry (formerly at their call sites
    /// or not at all); a blank variable is unset, as for `parsed_or`.
    #[test]
    fn dec05_owner_parameters_are_range_checked() {
        let ok = |env: &[(&str, &str)]| mode("shadow", &[], env);
        let r = ok(&[("P0_MAX_SLIPPAGE_BPS", "50"), ("P0_MAX_TOTAL_CAPITAL", "0")]).unwrap();
        assert_eq!(r.config.limits.max_slippage_bps, 50);
        assert_eq!(r.sources["max_slippage_bps"], "environment");
        assert_eq!(
            ok(&[("P0_MAX_SLIPPAGE_BPS", "  ")])
                .unwrap()
                .config
                .limits
                .max_slippage_bps,
            0
        );
        for (key, bad) in [
            ("P0_MAX_SLIPPAGE_BPS", "10001"),
            ("P0_MIN_PROFIT", "-1"),
            ("P0_MIN_PROFIT", "9223372036854775808"),
            ("P0_JUPITER_MAX_ACCOUNTS", "65"),
            ("P0_JUPITER_MAX_ACCOUNTS", "0"),
            ("P0_CANDIDATE_RECHECK_SLOTS", "0"),
            ("P0_MT0_INTERVAL_SECS", "0"),
            ("P0_CU_LIMIT", "4294967296"),
            ("P0_RECOVERY_ROOT_CATCHUP_SECS", "29"),
            ("P0_RECOVERY_ROOT_CATCHUP_SECS", "301"),
            ("P0_RECOVERY_ROOT_STALL_SECS", "4"),
            ("P0_RECOVERY_ROOT_HARD_MAX_SECS", "901"),
            ("P0_USE_THRESHOLD_TRIGGER", "yes"),
            ("P0_SOURCE_GEYSER", "on"),
        ] {
            assert!(ok(&[(key, bad)]).is_err(), "{key}={bad}");
        }
        // The hard recovery deadline is at least the catch-up wait.
        assert!(ok(&[("P0_RECOVERY_ROOT_HARD_MAX_SECS", "60")]).is_err());
        assert!(ok(&[
            ("P0_RECOVERY_ROOT_HARD_MAX_SECS", "60"),
            ("P0_RECOVERY_ROOT_CATCHUP_SECS", "30")
        ])
        .is_ok());
        // The probe limit matters only with the probe on.
        let off = [("P0_INTEGRATION_PROBE_LIMIT_PER_KIND", "0")];
        assert!(ok(&off).is_ok());
        assert!(ok(&[off[0], ("P0_INTEGRATION_PROBE", "true")]).is_err());
        let p = |f: &str| *PARAMETERS.iter().find(|p| p.field == f).unwrap();
        assert_eq!(
            (
                p("max_slippage_bps").minimum(),
                p("max_slippage_bps").maximum()
            ),
            (0, 10_000)
        );
        assert_eq!(p("rpc_rps").minimum(), 1);
    }

    /// DEC-05 precedence for the owner parameters and settings: profile < environment < config
    /// file < CLI, each value with its source.
    #[test]
    fn dec05_owner_values_follow_the_precedence() {
        let d = std::env::temp_dir().join(format!("p0_dec05_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join("owner.conf");
        std::fs::write(
            &file,
            "max-slippage-bps = 20\nthreshold-trigger = off\nsource-geyser = observe\n",
        )
        .unwrap();
        let file = file.to_str().unwrap();
        let env = [
            ("P0_MAX_SLIPPAGE_BPS", "10"),
            ("P0_USE_THRESHOLD_TRIGGER", "true"),
            ("P0_C0_CENSUS", "true"),
        ];
        let r = mode("shadow", &[], &env).unwrap();
        assert_eq!(
            (r.config.limits.max_slippage_bps, r.config.threshold_trigger),
            (10, true)
        );
        assert_eq!(r.sources["threshold_trigger"], "environment");
        let r = mode("shadow", &["--config", file], &env).unwrap();
        assert_eq!(
            (r.config.limits.max_slippage_bps, r.config.threshold_trigger),
            (20, false)
        );
        assert_eq!(
            (r.config.c0_census, r.config.source_geyser.as_str()),
            (true, "observe")
        );
        assert_eq!(r.sources["max_slippage_bps"], "config_file");
        assert_eq!(r.sources["c0_census"], "environment");
        let r = mode(
            "shadow",
            &[
                "--config",
                file,
                "--max-slippage-bps",
                "30",
                "--threshold-trigger",
                "on",
            ],
            &env,
        )
        .unwrap();
        assert_eq!(
            (r.config.limits.max_slippage_bps, r.config.threshold_trigger),
            (30, true)
        );
        assert_eq!(r.sources["threshold_trigger"], "cli");
        let e = r.effective();
        assert!(e.threshold_trigger && e.c0_census && e.source_geyser == "observe");
        assert!(r.text().contains("threshold-trigger = true [source cli]"));
        std::fs::remove_dir_all(d).unwrap();
    }

    /// DEC-05: `P0_INTEGRATION_PROBE` is the environment source of `diagnostics` (formerly an
    /// activated config overwrote it with `diagnostics=false`).
    #[test]
    fn dec05_integration_probe_environment_is_the_diagnostics_source() {
        let r = mode("econ4h", &[], &[("P0_INTEGRATION_PROBE", "true")]).unwrap();
        assert!(r.config.diagnostics);
        assert_eq!(r.sources["diagnostics"], "environment");
        assert_eq!(r.config.environment()["P0_INTEGRATION_PROBE"], "true");
        let r = mode(
            "econ4h",
            &["--diagnostics", "off"],
            &[("P0_INTEGRATION_PROBE", "true")],
        );
        assert!(!r.unwrap().config.diagnostics);
    }

    /// DEC-05: an activated config never overwrites the market observer's own horizon
    /// (`P0_MARKET_DURATION_SECS`, also `until-stopped`).
    #[test]
    fn dec05_activation_keeps_the_market_horizon() {
        let r = mode(
            "market",
            &[],
            &[("P0_MARKET_DURATION_SECS", "until-stopped")],
        )
        .unwrap();
        let env = r.config.environment();
        assert!(!env.contains_key("P0_MARKET_DURATION_SECS"));
        assert_eq!(env["P0_ECON_DURATION_SECS"], "14400");
    }

    /// DEC-05/§3: provisional values carry their measurement question; every parameter states
    /// its range and change rule.
    #[test]
    fn dec05_provisional_values_are_marked() {
        let q = |f: &str| {
            PARAMETERS
                .iter()
                .find(|p| p.field == f)
                .unwrap()
                .provisional()
        };
        assert_eq!(q("workers"), Some("B-2"));
        assert_eq!(q("sweep_batch_max_events"), Some("B-3"));
        assert_eq!(q("lane_weight_l1"), Some("B-4"));
        assert_eq!(q("rpc_pool_idle_ms"), Some("B-5"));
        assert_eq!(q("max_slippage_bps"), None);
        let text = mode("shadow", &[], &[]).unwrap().text();
        assert!(text.contains("--workers = 1 threads [default 1; 1..=4294967295; env RAYON_NUM_THREADS; source profile:standard; restart required; provisional, measurement open (B-2)]"));
        assert!(text.contains("--max-slippage-bps = 0 bps [default 0; 0..=10000;"));
    }
}

//! Validated market capture configuration.
use super::*;

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
        max_pending: darkbot_solana::runtime_config::defaults::ENRICH_MAX_PENDING as usize,
        max_attempts: darkbot_solana::runtime_config::defaults::ENRICH_MAX_ATTEMPTS as u32,
        retry_ms: darkbot_solana::runtime_config::defaults::ENRICH_RETRY_MS,
        timeout_ms: darkbot_solana::runtime_config::defaults::ENRICH_TIMEOUT_MS,
        phase_max_secs: darkbot_solana::runtime_config::defaults::ENRICHMENT_SECONDS,
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
    /// R24: distinct transactions/events a market-only capture retains (`P0_MARKET_RETENTION`);
    /// beyond it the capture is INCOMPLETE. An econ run uses its `P0_ECON_RETENTION` instead.
    pub retention: usize,
    pub run_id: String,
    pub run_dir: PathBuf,
    pub enrich: EnrichConfig,
    pub gap: GapConfig,
    pub reconcile: ReconcileConfig,
}

pub const DEFAULT_DURATION_SECS: u64 = 14_400;
// D11: the defaults of the typed registry (`runtime_config::defaults`), never a second literal.
use darkbot_solana::runtime_config::defaults;
pub const DEFAULT_READY_TIMEOUT_SECS: u64 = defaults::STREAM_READY_SECONDS;
/// Must exceed the ingress idle timeout (60 s) plus its maximum reconnect backoff (30 s).
pub const DEFAULT_DRAIN_TIMEOUT_SECS: u64 = defaults::MARKET_DRAIN_SECONDS;
pub const DEFAULT_INGRESS_BUFFER: usize = defaults::INGRESS_QUEUE as usize;
/// Transport heartbeat interval; must stay below the ingress idle timeout.
pub const DEFAULT_HEARTBEAT_SECS: u64 = defaults::MARKET_HEARTBEAT_SECONDS;
/// PROVISIONAL minimum number of liquidation events for distribution statistics.
pub const DEFAULT_MIN_SAMPLE_LIQUIDATIONS: u64 = defaults::MARKET_MIN_SAMPLE;
pub const DEFAULT_OUT_DIR: &str = "project_0/data/market";

/// The capture source `P0_MARKET_SOURCE` names (unset: transactions) — the one resolution the
/// capture, its boot admission and `p0ctl boot` share (DEC-07).
pub fn source_name(value: Option<&str>) -> Result<&'static str, String> {
    match value {
        None | Some("transactions") => Ok("transactions"),
        Some("logs") => Ok("logs"),
        Some(other) => Err(format!(
            "invalid P0_MARKET_SOURCE={other} (transactions|logs)"
        )),
    }
}

impl MarketConfig {
    /// Pure parser (testable). Requires `P0_PROGRAM_ID` and `P0_WS_URL`. `P0_RPC_URL` is
    /// optional: without it, `logs` enrichment is unavailable and reported as such, but capture,
    /// summary and report still run. DEC-18: both endpoints are the operational configuration's
    /// (its export of the bound providers — the registry's selection or a named loopback one;
    /// the credential is the transport's), never derived here from a key (§1.3.2: no URL
    /// resolution of the capture's own). No keypair, wallet setting or credential is read.
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
        let source = match source_name(opt("P0_MARKET_SOURCE").as_deref())? {
            "logs" => MarketSource::Logs,
            _ => MarketSource::Transactions,
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
        let ws_url = opt("P0_WS_URL").ok_or(
            "missing required env: P0_WS_URL (the selected WSS provider: p0ctl provider select)",
        )?;
        let rpc_url = opt("P0_RPC_URL");
        let min_sample_liquidations = num("P0_MARKET_MIN_SAMPLE", DEFAULT_MIN_SAMPLE_LIQUIDATIONS)?;
        let until_stopped = opt("P0_MARKET_DURATION_SECS").as_deref() == Some("until-stopped");
        // Zero is the status/record sentinel for an explicitly unbounded observation.
        let duration_secs = if until_stopped {
            0
        } else {
            num("P0_MARKET_DURATION_SECS", DEFAULT_DURATION_SECS)?
        };
        let ready_timeout_secs = num("P0_MARKET_READY_TIMEOUT_SECS", DEFAULT_READY_TIMEOUT_SECS)?;
        let drain_timeout_secs = num("P0_MARKET_DRAIN_TIMEOUT_SECS", DEFAULT_DRAIN_TIMEOUT_SECS)?;
        let heartbeat_secs = num("P0_MARKET_HEARTBEAT_SECS", DEFAULT_HEARTBEAT_SECS)?;
        let ingress_buffer =
            usize::try_from(num("P0_INGRESS_BUFFER", DEFAULT_INGRESS_BUFFER as u64)?)
                .map_err(|_| "invalid P0_INGRESS_BUFFER".to_string())?;
        let retention = usize::try_from(num(
            "P0_MARKET_RETENTION",
            crate::analytics::econ::DEFAULT_RETENTION as u64,
        )?)
        .map_err(|_| "invalid P0_MARKET_RETENTION".to_string())?;
        if retention == 0 || retention > crate::analytics::econ::MAX_RETENTION {
            return Err(format!(
                "P0_MARKET_RETENTION must be 1..={}",
                crate::analytics::econ::MAX_RETENTION
            ));
        }
        if (duration_secs == 0 && !until_stopped)
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
            retention,
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

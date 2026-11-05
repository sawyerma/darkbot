//! The runtime's validated configuration (`bsc/config/ingress.public.env`): the runtime, the
//! executor, Strategy C with its frozen gas evidence, the canonical follower and its second
//! source.

use super::*;

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
            "bsc/native/evidence/gas/receipts_20261026.txt",
            include_bytes!("../../../../native/evidence/gas/receipts_20261026.txt"),
        ),
        (
            "bsc/native/evidence/gas/flap_gas_20261031.txt",
            include_bytes!("../../../../native/evidence/gas/flap_gas_20261031.txt"),
        ),
        // BSC3-079: older windows of the v5.27.2 era (the largest gas of a
        // class depends on the window), with the launch-block BUY class.
        (
            "bsc/native/evidence/gas/flap_gas_20261031_w130100000.txt",
            include_bytes!("../../../../native/evidence/gas/flap_gas_20261031_w130100000.txt"),
        ),
        (
            "bsc/native/evidence/gas/flap_gas_20261031_w130500000.txt",
            include_bytes!("../../../../native/evidence/gas/flap_gas_20261031_w130500000.txt"),
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
/// launch block, the C3 path's shape), the SELL's and the APPROVE's; R12c:
/// a configured Four Classic BUY's of the Classic BUYs; R12d: a configured
/// OpenFour BUY's of the OpenFour BUYs; SC4 c (B1): each family's exit
/// approval and SELL of its APPROVE and SELL receipts.
pub fn gas_limits_cover(
    exec: &crate::paper_exec::ExecConfig,
    gas: &darkbot_bsc_trading::economics::network::GasEvidence,
) -> Result<(), String> {
    use darkbot_bsc_trading::economics::network::{Action, PathClass, Venue};
    let largest = |venue, action| {
        gas.class(PathClass { venue, action })
            .map_or(0, |c| c.gas_used_max)
    };
    let buys = &[Action::Buy, Action::BuyInLaunchBlock][..];
    // A configured Four.meme family's BUY of its BUYs; SC4 c (B1) its exits'
    // approval and SELL of its APPROVE and SELL receipts.
    let family =
        |f: Option<crate::paper_exec::FourExecConfig>, keys: [&'static str; 3], venue: Venue| {
            f.into_iter().flat_map(move |f| {
                [
                    (keys[0], f.gas_limit, venue, buys),
                    (keys[1], f.approve_gas_limit, venue, &[Action::Approve][..]),
                    (keys[2], f.sell_gas_limit, venue, &[Action::Sell][..]),
                ]
            })
        };
    let four = family(
        exec.four,
        [
            "BSC_EXEC_FOUR_GAS_LIMIT",
            "BSC_EXEC_FOUR_APPROVE_GAS_LIMIT",
            "BSC_EXEC_FOUR_SELL_GAS_LIMIT",
        ],
        Venue::FourClassic,
    );
    // R12d: a configured OpenFour family's likewise.
    let openfour = family(
        exec.openfour,
        [
            "BSC_EXEC_OPENFOUR_GAS_LIMIT",
            "BSC_EXEC_OPENFOUR_APPROVE_GAS_LIMIT",
            "BSC_EXEC_OPENFOUR_SELL_GAS_LIMIT",
        ],
        Venue::OpenFour,
    );
    for (name, limit, venue, actions) in [
        ("BSC_EXEC_GAS_LIMIT", exec.gas_limit, Venue::Flap, buys),
        (
            "BSC_EXEC_SELL_GAS_LIMIT",
            exec.sell_gas_limit,
            Venue::Flap,
            &[Action::Sell][..],
        ),
        (
            "BSC_EXEC_APPROVE_GAS_LIMIT",
            exec.approve_gas_limit,
            Venue::Flap,
            &[Action::Approve][..],
        ),
    ]
    .into_iter()
    .chain(four)
    .chain(openfour)
    {
        for action in actions {
            let most = largest(venue, *action);
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
    /// The public WSS endpoint of each source, if it has one (R6: the
    /// local geth's arena directory).
    pub endpoints: Vec<(SourceId, String)>,
    /// REV.1.0 R6: the local geth's C1.4 arena, if this build has one
    /// (`BSC_INGRESS_GETH_*`).
    pub geth: Option<crate::geth_source::GethArena>,
    /// The bound of each source's ingress queue and of the canonical and
    /// confirmation queues (`BSC_RUNTIME_QUEUE_EVENTS`).
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
    /// Audit E 3.3: after how many status intervals a reader takes the
    /// status as stale (written into every status record).
    pub status_stale_intervals: u64,
    /// Audit E 3.1/3.4: the owner's bounds.
    pub owner: crate::sources::OwnerLimits,
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

/// Audit E 3.12: the PAPER path refuses at least the frozen G0.7 launcher
/// and buyer (`acceptance_contract`), checked as the configuration is read;
/// the configuration may refuse more.
fn refused_g07_accounts(accounts: Vec<[u8; 20]>) -> Result<Vec<[u8; 20]>, String> {
    use crate::acceptance_contract::{BUYER, LAUNCHER};
    for (role, address) in [("launcher", LAUNCHER), ("buyer", BUYER)] {
        let account: [u8; 20] = alloy_primitives::hex::decode(address)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| format!("the G0.7 {role} {address} is no address"))?;
        if !accounts.contains(&account) {
            return Err(format!(
                "BSC_EXEC_REFUSED_ACCOUNTS must refuse the G0.7 {role} {address}"
            ));
        }
    }
    Ok(accounts)
}

/// R12c/R12d: a Four.meme BUY's keys (`<prefix>_INPUT_WEI`, `_SLIPPAGE_BPS`,
/// `_GAS_LIMIT`), SC4 c (B1) its exits' `_APPROVE_GAS_LIMIT` and
/// `_SELL_GAS_LIMIT`: all five or none.
fn four_exec(
    venue: crate::paper_exec::FourVenue,
    keys: [Option<&str>; 5],
) -> Result<Option<crate::paper_exec::FourExecConfig>, String> {
    let p = venue.keys();
    let gas = |key: &str, value: &str| bounded(&format!("{p}_{key}"), value, 21_000, 100_000_000);
    match keys {
        [None, None, None, None, None] => Ok(None),
        [
            Some(input),
            Some(slippage),
            Some(buy),
            Some(approve),
            Some(sell),
        ] => Ok(Some(crate::paper_exec::FourExecConfig {
            input_wei: u128::from(bounded(&format!("{p}_INPUT_WEI"), input, 1, u64::MAX)?),
            slippage_bps: bounded(&format!("{p}_SLIPPAGE_BPS"), slippage, 0, 9_999)? as u16,
            gas_limit: gas("GAS_LIMIT", buy)?,
            approve_gas_limit: gas("APPROVE_GAS_LIMIT", approve)?,
            sell_gas_limit: gas("SELL_GAS_LIMIT", sell)?,
        })),
        _ => Err(format!(
            "{p}_INPUT_WEI, {p}_SLIPPAGE_BPS, {p}_GAS_LIMIT, {p}_APPROVE_GAS_LIMIT and \
             {p}_SELL_GAS_LIMIT go together"
        )),
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
        let parity_min = values.remove(crate::live_qualification::PARITY_MIN_KEY);
        let owner = crate::sources::OwnerLimits::new(
            values.remove(crate::sources::OwnerLimits::BUY_ATTEMPTS_TOTAL_MAX),
            values.remove(crate::sources::OwnerLimits::KEEP_GENERATIONS),
        )?;
        let sender_paths = values.remove("BSC_EXEC_SENDER_PATHS");
        // R12c: the Four Classic BUY of the C3 path, SC4 c its exits' gas
        // limits: all five or none.
        let four_keys = [
            values.remove("BSC_EXEC_FOUR_INPUT_WEI"),
            values.remove("BSC_EXEC_FOUR_SLIPPAGE_BPS"),
            values.remove("BSC_EXEC_FOUR_GAS_LIMIT"),
            values.remove("BSC_EXEC_FOUR_APPROVE_GAS_LIMIT"),
            values.remove("BSC_EXEC_FOUR_SELL_GAS_LIMIT"),
        ];
        // R12d: the OpenFour BUY of the C3 path, likewise.
        let openfour_keys = [
            values.remove("BSC_EXEC_OPENFOUR_INPUT_WEI"),
            values.remove("BSC_EXEC_OPENFOUR_SLIPPAGE_BPS"),
            values.remove("BSC_EXEC_OPENFOUR_GAS_LIMIT"),
            values.remove("BSC_EXEC_OPENFOUR_APPROVE_GAS_LIMIT"),
            values.remove("BSC_EXEC_OPENFOUR_SELL_GAS_LIMIT"),
        ];
        let exit_slippage = values.remove("BSC_EXEC_EXIT_SLIPPAGE_BPS");
        let operator_exit = values.remove("BSC_EXEC_LIVE_OPERATOR_EXIT");
        let nonce_burn = values.remove("BSC_EXEC_LIVE_NONCE_BURN");
        let public_only = values.remove("BSC_EXEC_LIVE_PUBLIC_ONLY");
        let risk_outlier = values.remove("BSC_EXEC_LIVE_RISK_OUTLIER");
        let c3_rearm = values.remove("BSC_EXEC_LIVE_C3_REARM");
        let builder_policy = values.remove("BSC_EXEC_LIVE_BUILDER_POLICY");
        let public_exit = values.remove("BSC_EXEC_LIVE_PUBLIC_EXIT");
        let public_exit_rpc = values.remove("BSC_EXEC_LIVE_PUBLIC_EXIT_RPC");
        let dex_exit = values.remove("BSC_EXEC_LIVE_DEX_EXIT");
        let divergence = values.remove("BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE");
        let presence = values.remove("BSC_EXEC_LIVE_PRESENCE_ANCHOR");
        // REV.1.0 R10: the public config names every LIVE key, empty (no LIVE
        // path) until the operator sets them on the host.
        let mut live_value = |name: &str| values.remove(name).filter(|v| !v.is_empty());
        let live_key = live_value("BSC_LIVE_KEY");
        let live_buyer = live_value("BSC_LIVE_BUYER");
        let live_route = live_value("BSC_LIVE_ROUTE");
        let live_token = live_value("BSC_LIVE_BLOCKRAZOR_TOKEN_FILE");
        let live_connect = values.remove("BSC_LIVE_CONNECT_TIMEOUT_MS");
        let geth_arena = values.remove("BSC_INGRESS_GETH_ARENA");
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
                if !url.starts_with("https://") {
                    return Err("BSC_CANONICAL_RPC must be https://".into());
                }
                // The follower's own rule (Audit E 5.14: parsed as https://).
                Endpoint::parse_https(url)
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
                if public_exit.is_some() || public_exit_rpc.is_some() {
                    return Err("BSC_EXEC_LIVE_PUBLIC_EXIT needs BSC_EXEC_PAPER_IDENTITY".into());
                }
                if dex_exit.is_some() {
                    return Err("BSC_EXEC_LIVE_DEX_EXIT needs BSC_EXEC_PAPER_IDENTITY".into());
                }
                if parity_min.is_some() {
                    return Err(format!(
                        "{} needs BSC_EXEC_PAPER_IDENTITY",
                        crate::live_qualification::PARITY_MIN_KEY
                    ));
                }
                if divergence.is_some() {
                    return Err(
                        "BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE needs BSC_EXEC_PAPER_IDENTITY".into(),
                    );
                }
                if presence.is_some() {
                    return Err(
                        "BSC_EXEC_LIVE_PRESENCE_ANCHOR needs BSC_EXEC_PAPER_IDENTITY".into(),
                    );
                }
                if four_keys.iter().any(Option::is_some) {
                    return Err("BSC_EXEC_FOUR_* needs BSC_EXEC_PAPER_IDENTITY".into());
                }
                if openfour_keys.iter().any(Option::is_some) {
                    return Err("BSC_EXEC_OPENFOUR_* needs BSC_EXEC_PAPER_IDENTITY".into());
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
                // Audit E 3.2: the parity floor every verification of a LIVE
                // qualification reads from this file, checked with it.
                crate::live_qualification::parity_min(parity_min)?;
                ExecSetup {
                    identity_path: ExecSetup::expand_home(path)?,
                    owner,
                    reprobe: ms(take("BSC_EXEC_REPROBE_MS", 1_000, 3_600_000)?),
                    reprobe_failures: take("BSC_EXEC_REPROBE_FAILURES", 0, 100)? as u32,
                    refused_accounts: refused_g07_accounts(StateConfig::parse_watched(
                        refused_accounts.ok_or("BSC_EXEC_REFUSED_ACCOUNTS is required")?,
                    )
                    .map_err(|e| e.replace("BSC_STATE_WATCHED", "BSC_EXEC_REFUSED_ACCOUNTS"))?)?,
                    exec: ExecConfig {
                        four: four_exec(crate::paper_exec::FourVenue::Classic, four_keys)?,
                        openfour: four_exec(
                            crate::paper_exec::FourVenue::OpenFour,
                            openfour_keys,
                        )?,
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
                            max_validity_blocks(),
                        )?,
                        start_limit_ns: take(
                            "BSC_EXEC_START_LIMIT_NS",
                            1,
                            max_start_limit_ns(),
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
                        live_qualification_max_age_ms: take(
                            "BSC_EXEC_LIVE_QUALIFICATION_MAX_AGE_MS",
                            0,
                            31_536_000_000,
                        )?,
                        // R9 (BSC3-001): empty, OFF; else the operator's
                        // presence address.
                        live_presence_anchor: match presence {
                            None => {
                                return Err("BSC_EXEC_LIVE_PRESENCE_ANCHOR is required".into());
                            }
                            Some("") => None,
                            Some(a) => Some(
                                a.strip_prefix("0x")
                                    .filter(|h| h.len() == 40)
                                    .and_then(|h| alloy_primitives::hex::decode(h).ok())
                                    .and_then(|b| <[u8; 20]>::try_from(b).ok())
                                    .ok_or(
                                        "BSC_EXEC_LIVE_PRESENCE_ANCHOR is no 0x address of 20 bytes",
                                    )?,
                            ),
                        },
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
                        live_exit_reprice_bps: u16::try_from(take(
                            "BSC_EXEC_LIVE_EXIT_REPRICE_BPS",
                            0,
                            9_999,
                        )?)
                        .map_err(|_| "BSC_EXEC_LIVE_EXIT_REPRICE_BPS out of range")?,
                        live_public_exit: match (public_exit, public_exit_rpc) {
                            (Some("OFF"), Some(_)) => false,
                            (Some("ON"), Some(url)) if !url.is_empty() => true,
                            (Some("ON"), Some(_)) => {
                                return Err(
                                    "BSC_EXEC_LIVE_PUBLIC_EXIT=ON needs BSC_EXEC_LIVE_PUBLIC_EXIT_RPC"
                                        .into(),
                                );
                            }
                            (Some(v), Some(_)) => {
                                return Err(format!(
                                    "BSC_EXEC_LIVE_PUBLIC_EXIT={v} is neither ON nor OFF"
                                ));
                            }
                            _ => {
                                return Err("BSC_EXEC_LIVE_PUBLIC_EXIT and BSC_EXEC_LIVE_PUBLIC_EXIT_RPC are required".into());
                            }
                        },
                        live_public_exit_rpc: match public_exit_rpc {
                            Some(url) if !url.is_empty() => {
                                match crate::chain_follower::https_endpoint(url) {
                                    // Never the endpoint in an error (it may hold a key).
                                    Err(e) => {
                                        return Err(format!(
                                            "BSC_EXEC_LIVE_PUBLIC_EXIT_RPC is refused: {e:?}"
                                        ));
                                    }
                                    Ok(_) => url.to_string(),
                                }
                            }
                            url => url.unwrap_or_default().to_string(),
                        },
                        live_dex_exit: match dex_exit {
                            Some("ON") => true,
                            Some("OFF") => false,
                            Some(v) => {
                                return Err(format!(
                                    "BSC_EXEC_LIVE_DEX_EXIT={v} is neither ON nor OFF"
                                ));
                            }
                            None => return Err("BSC_EXEC_LIVE_DEX_EXIT is required".into()),
                        },
                        live_divergence_quarantine: match divergence {
                            Some("ON") => true,
                            Some("OFF") => false,
                            Some(v) => {
                                return Err(format!(
                                    "BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE={v} is neither ON nor OFF"
                                ));
                            }
                            None => {
                                return Err(
                                    "BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE is required".into()
                                );
                            }
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
        let live_connect_ms = |v: &str| bounded("BSC_LIVE_CONNECT_TIMEOUT_MS", v, 100, 60_000);
        let live = match (live_key, live_buyer, live_route) {
            (None, None, None) => {
                if live_token.is_some() {
                    return Err("BSC_LIVE_BLOCKRAZOR_TOKEN_FILE needs BSC_LIVE_ROUTE".into());
                }
                live_connect.map(live_connect_ms).transpose()?;
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
                    connect_timeout: ms(live_connect_ms(
                        live_connect.ok_or("BSC_LIVE_CONNECT_TIMEOUT_MS is required")?,
                    )?),
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
        let quiet_ms = take("BSC_RUNTIME_QUIET_MS", 1000, 600_000)?;
        // R6: the local geth's arena: all of its keys, or none (no geth
        // source in this build).
        let geth = match geth_arena {
            None => None,
            Some(dir) if !Path::new(dir).is_absolute() => {
                return Err("BSC_INGRESS_GETH_ARENA is no absolute path".into());
            }
            Some(dir) => {
                let dedup_sets = take("BSC_INGRESS_GETH_DEDUP_SETS", 1, 1 << 20)? as usize;
                if !dedup_sets.is_power_of_two() {
                    return Err("BSC_INGRESS_GETH_DEDUP_SETS is no power of two".into());
                }
                Some(crate::geth_source::GethArena {
                    dir: PathBuf::from(dir),
                    generation: take("BSC_INGRESS_GETH_GENERATION", 1, u64::MAX)?,
                    peer_lanes: take(
                        "BSC_INGRESS_GETH_PEER_LANES",
                        1,
                        darkbot_bsc_geth_ipc::ingress::C14_PEER_LANE_POOL_BOUND as u64,
                    )? as usize,
                    idle: Duration::from_micros(take("BSC_INGRESS_GETH_IDLE_US", 1, 1_000_000)?),
                    // No record becomes an intent here (the runtime's
                    // ingress decides): the poller's one slot stays free.
                    hot0: (1, dedup_sets),
                    // Review HIGH: alive twice per quiet bound (its lanes
                    // carry launches only); a closed arena tried again as a
                    // WSS source after its first backoff.
                    alive: Duration::from_millis(quiet_ms / 2),
                    retry: wss.backoff_initial,
                })
            }
        };
        if let Some(g) = &geth {
            endpoints.push((SourceId::Geth, g.dir.display().to_string()));
        }
        let config = RuntimeConfig {
            geth,
            endpoints,
            queue_events: take("BSC_RUNTIME_QUEUE_EVENTS", 64, 1 << 22)? as usize,
            dedup_hashes: take("BSC_RUNTIME_DEDUP_HASHES", 1024, 1 << 24)? as usize,
            quiet_ms,
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
            status_stale_intervals: take("BSC_RUNTIME_STATUS_STALE_INTERVALS", 2, 1_000)?,
            owner,
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
        // A6: the exit multiples, by the strategy's own check.
        config.strategy.engine.validate().map_err(|e| {
            format!(
                "BSC_STRATEGY_C_TARGET_NET_MULTIPLE/BSC_STRATEGY_C_STOP_NET_MULTIPLE: {e} \
                 (0 <= stop < 1x < target)"
            )
        })?;
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

    pub(super) fn endpoint(&self, source: SourceId) -> Option<&str> {
        self.endpoints
            .iter()
            .find(|(s, _)| *s == source)
            .map(|(_, u)| u.as_str())
    }
}

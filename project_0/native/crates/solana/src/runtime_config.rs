//! Shared typed operational configuration. It cannot authorize money or submission.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, sync::OnceLock};

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
    ingress_queue = 65536, "events", "P0_INGRESS_BUFFER";
    worker_queue = 256, "jobs", "P0_REMOTE_WORKER_BUFFER";
    bootstrap_seconds = 600, "s", "P0_ECON_BOOTSTRAP_SECS";
    stream_ready_seconds = 600, "s", "P0_MARKET_READY_TIMEOUT_SECS";
    drain_seconds = 180, "s", "P0_ECON_DRAIN_SECS";
    market_drain_seconds = 120, "s", "P0_MARKET_DRAIN_TIMEOUT_SECS";
    reconcile_seconds = 14400, "s", "P0_MARKET_RECONCILE_PHASE_MAX_SECS";
    gap_seconds = 180, "s", "P0_MARKET_GAP_PHASE_MAX_SECS";
    enrichment_seconds = 900, "s", "P0_MARKET_ENRICH_PHASE_MAX_SECS";
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
}
/// Layout revision of [`Limits`]: bumped with every change of its field list, since each typed
/// record that embeds it (`OPERATIONAL_CONFIG`, `RUN_MANIFEST`) changes its binary layout and its
/// payload schema with it. 1: up to `econ_retention_mib`; 2: `boot_calls`, `boot_seconds` (N1);
/// 3: `owner_evidence_sync_ms` (M2); 4: `wss_ack_ms_per_subscription` (N8).
pub const LIMITS_REVISION: u16 = 4;
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
    pub fn maximum(&self) -> u64 {
        if self.field == "fe_batch_size" {
            100
        } else {
            u64::from(u32::MAX)
        }
    }
}
/// Machine description of the numeric parameters (render of `PARAMETERS` for the pinned
/// `--export-environment` contract).
pub fn descriptors() -> Vec<Value> {
    PARAMETERS
        .iter()
        .map(|p| {
            json!({"name":p.name(),"field":p.field,"type":"u64","unit":p.unit,"default":p.default,
                "minimum":1,"maximum":p.maximum(),"env":p.env,"change":"restart_required"})
        })
        .collect()
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
}
impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            profile: Profile::Standard,
            limits: Limits::default(),
            rpc_endpoints: vec![],
            wss_endpoints: vec![],
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
        }
    }
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
    pub export: Option<std::path::PathBuf>,
    /// The config file was the pinned bridge's private JSON handoff (JFI-03 legacy edge).
    pub legacy_json_config: bool,
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
    /// Explicit operational endpoint wins over legacy verifier aliases, so
    /// provider qualification and the strict worker bind the same endpoint.
    pub fn rpc_url(&self, env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
        self.rpc_endpoints
            .first()
            .cloned()
            .or_else(|| env("P0_AUDIT_RPC_URL"))
            .or_else(|| env("P0_RPC_URL"))
    }
    pub fn validate(&self) -> Result<(), String> {
        for p in PARAMETERS {
            if self
                .limits
                .get(p.field)
                .is_none_or(|n| n == 0 || n > u64::from(u32::MAX))
            {
                return Err(format!("invalid --{}: expected 1..={}", p.name(), u32::MAX));
            }
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
        env.insert("P0_ECON_DURATION_SECS".into(), seconds.clone());
        env.insert("P0_MARKET_DURATION_SECS".into(), seconds);
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

/// An operational config file (`--config` / `P0_OPERATIONAL_CONFIG_FILE`): the external file
/// format decoded straight into typed optional overrides. Unknown keys fail closed.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    profile: Option<String>,
    limits: Option<BTreeMap<String, u64>>,
    rpc_endpoints: Option<Vec<String>>,
    wss_endpoints: Option<Vec<String>>,
    fallback: Option<bool>,
    adaptive: Option<bool>,
    duration: Option<String>,
    verification: Option<String>,
    diagnostics: Option<bool>,
    fe_mode: Option<String>,
    fe_targets: Option<Vec<String>>,
    fe_group: Option<String>,
}
impl ConfigFile {
    /// A config file: the native text format, or (JFI-03 legacy edge) the private JSON handoff
    /// the pinned Python bridge `operational_config.py` writes via `--export-environment` for
    /// its offline tools. The native launcher never produces or passes a JSON config.
    /// Returns the file and whether it was the legacy JSON handoff.
    fn decode(bytes: &[u8]) -> Result<(Self, bool), String> {
        if bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') {
            return Ok((Self::decode_legacy_json(bytes)?, true));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| "config file is not text")?;
        Ok((Self::decode_text(text)?, false))
    }
    fn decode_text(text: &str) -> Result<Self, String> {
        let mut f = Self::default();
        let on_off = |v: &str| match v {
            "true" | "on" => Ok(true),
            "false" | "off" => Ok(false),
            _ => Err("expected on/off".to_string()),
        };
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
                "fallback" => f.fallback = Some(on_off(&value)?),
                "adaptive" => f.adaptive = Some(on_off(&value)?),
                "diagnostics" => f.diagnostics = Some(on_off(&value)?),
                "rpc_endpoints" => f.rpc_endpoints = Some(list(&value)),
                "wss_endpoints" => f.wss_endpoints = Some(list(&value)),
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
    fn decode_legacy_json(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| {
            let text = e.to_string();
            match text
                .strip_prefix("unknown field `")
                .and_then(|r| r.split('`').next())
            {
                Some(field) => format!("unknown config field {field}"),
                None if e.is_syntax() || e.is_eof() => "invalid config JSON".into(),
                None if text.starts_with("invalid type") && text.contains("expected struct") => {
                    "config must be an object".into()
                }
                None => "configuration type mismatch".into(),
            }
        })
    }
}
impl Resolved {
    /// Precedence: profile < environment < config file < CLI. Unknown keys fail.
    pub fn parse(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        let mut cli = BTreeMap::new();
        let mut file = env("P0_OPERATIONAL_CONFIG_FILE");
        let mut export = None;
        let mut profile_name = env("P0_PROFILE").unwrap_or_else(|| "standard".into());
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
                    if name == "export_environment" {
                        export = Some(std::path::PathBuf::from(value));
                    } else if name == "config" {
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
        let (file, legacy_json_config) = if let Some(path) = file {
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
            (ConfigFile::default(), false)
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
            "fallback",
            "adaptive",
            "duration",
            "verification",
            "diagnostics",
            "fe_mode",
            "fe_targets",
            "fe_group",
        ] {
            sources.insert(name.to_string(), format!("profile:{profile_name}"));
        }
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
        if let Some(url) = env("P0_WS_URL") {
            c.wss_endpoints = vec![url];
            sources.insert("wss_endpoints".into(), "environment".into());
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
            fe_group
        );
        // CLI over everything.
        for (k, val) in cli {
            if Limits::default().get(&k).is_some() {
                let n = val
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --{}", k.replace('_', "-")))?;
                c.limits.set(&k, n);
            } else {
                let on_off = |v: &str| match v {
                    "true" | "on" => Ok(true),
                    "false" | "off" => Ok(false),
                    _ => Err("expected on/off".to_string()),
                };
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
                    "wss_endpoints" => c.wss_endpoints = list(&val),
                    "fe_targets" => c.fe_targets = list(&val),
                    "fallback" => c.fallback = on_off(&val)?,
                    "adaptive" => c.adaptive = on_off(&val)?,
                    "diagnostics" => c.diagnostics = on_off(&val)?,
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
            export,
            legacy_json_config,
        })
    }
    /// Productive native entry points (runtime start, launcher plan, native strict verification)
    /// take only the native text config; the JSON handoff stays inside the pinned bridge.
    pub fn native_only(self) -> Result<Self, String> {
        if self.legacy_json_config {
            return Err("CONFIG_JSON_REFUSED: native runs take the text config (key = value); a JSON config is only the private handoff of the pinned Python bridge (JFI-03)".into());
        }
        Ok(self)
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
        }
    }
    pub fn json(&self) -> Value {
        let mut values = serde_json::to_value(&self.config).unwrap();
        for f in ["rpc_endpoints", "wss_endpoints"] {
            values[f] = json!(vec![
                "REDACTED";
                if f == "rpc_endpoints" {
                    self.config.rpc_endpoints.len()
                } else {
                    self.config.wss_endpoints.len()
                }
            ]);
        }
        let active = serde_json::to_value(&self.config.limits).unwrap();
        let mut parameters = descriptors();
        let defaults = serde_json::to_value(RuntimeConfig::default()).unwrap();
        for (name, kind, allowed) in [
            ("fe_mode", "enum", json!(["smoke", "full"])),
            ("fe_group", "pubkey", json!("Solana pubkey")),
            (
                "fe_targets",
                "array<pubkey>",
                json!("unique nonempty target scope"),
            ),
        ] {
            parameters.push(json!({"name":name.replace('_',"-"),"field":name,"type":kind,"unit":"scope","default":defaults[name],"allowed":allowed,"active":values[name],"source":self.sources.get(name),"change":"restart_required"}));
        }
        for d in &mut parameters {
            let f = d["field"].as_str().unwrap().to_string();
            if !active[&f].is_null() {
                d["active"] = active[&f].clone();
            }
            d["source"] = json!(self.sources.get(&f));
        }
        // Defaults come from RuntimeConfig::default(), the single source; only type metadata lives here.
        for (name, kind, unit, allowed) in [
            (
                "profile",
                "enum",
                "profile",
                json!(["smoke", "standard", "performance", "custom"]),
            ),
            (
                "rpc_endpoints",
                "array<string>",
                "priority ordered HTTP URLs",
                json!(["http", "https"]),
            ),
            (
                "wss_endpoints",
                "array<string>",
                "priority ordered WSS URLs",
                json!(["ws", "wss"]),
            ),
            ("fallback", "bool", "enabled", json!([true, false])),
            ("adaptive", "bool", "enabled", json!([true, false])),
            (
                "duration",
                "duration",
                "s/m/h",
                json!({"min_seconds":1,"max_seconds":u64::MAX/4/1_000_000_000}),
            ),
        ] {
            parameters.push(json!({"name":name.replace('_',"-"),"field":name,"type":kind,"unit":unit,"default":defaults[name],"allowed":allowed,"active":values[name],"source":self.sources.get(name),"change":"restart_required"}));
        }
        for (name, kind, allowed) in [
            (
                "verification",
                "enum",
                json!(["off", "index", "strict", "crosscheck"]),
            ),
            ("diagnostics", "bool", json!([true, false])),
        ] {
            parameters.push(json!({"name":name,"field":name,"type":kind,"unit":"mode","default":defaults[name],"allowed":allowed,"active":values[name],"source":self.sources.get(name).cloned().unwrap_or_else(||"profile".into()),"change":"restart_required"}));
        }
        json!({"schema":"Project0/OperationalConfig/1","values":values,"parameters":parameters,"sources":self.sources,"provider_capacity_qualified":false,"free_only":true,"live_authorized":false,"submission_authorized":false})
    }
    /// Explicit secret-bearing machine handoff; never stdout, never overwrite.
    pub fn export_private(&self, path: &Path) -> Result<(), String> {
        use std::io::Write;
        #[cfg(unix)]
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut f = options
            .open(path)
            .map_err(|_| "config export exists or cannot be written")?;
        f.write_all(&serde_json::to_vec_pretty(&self.config).map_err(|_| "config encode failed")?)
            .and_then(|_| f.sync_all())
            .map_err(|_| "config export failed".into())
    }
    pub fn help() -> String {
        let mut s=String::from("p0-longtail [--profile smoke|standard|performance|custom] [--config FILE] [--duration 10m|1h|5h|24h]\n--rpc-endpoints URL[,URL] --wss-endpoints URL[,URL] (priority order; declared FREE_ONLY)\n--fe-mode smoke|full --fe-targets PUBKEY[,PUBKEY] --fe-group PUBKEY\n--fallback on|off --adaptive on|off --diagnostics on|off --verification off|index|strict|crosscheck --print-effective-config\nPrecedence: profile < environment < config file < CLI. All changes require restart.\nFinancial approval is separate. Profiles do not prove provider capacity.\n");
        for p in PARAMETERS {
            s += &format!(
                "--{} <u64> [{}; default={}; 1..={}]\n",
                p.name(),
                p.unit,
                p.default,
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
        for (name, value) in [
            ("fallback", e.fallback.to_string()),
            ("adaptive", e.adaptive.to_string()),
            ("duration", e.duration.clone()),
            ("verification", e.verification.clone()),
            ("diagnostics", e.diagnostics.to_string()),
            ("fe_mode", e.fe_mode.clone()),
            ("fe_targets", e.fe_targets.join(",")),
            ("fe_group", e.fe_group.clone()),
        ] {
            s += &format!(
                "{} = {value} [source {}]\n",
                name.replace('_', "-"),
                src(name)
            );
        }
        for p in PARAMETERS {
            s += &format!(
                "--{} = {} {} [default {}; 1..={}; env {}; source {}; restart required]\n",
                p.name(),
                e.limits.get(p.field).unwrap_or(0),
                p.unit,
                p.default,
                p.maximum(),
                p.env,
                src(p.field)
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
            for key in [
                "live_authorized",
                "submission_authorized",
                "provider_capacity_qualified",
            ] {
                assert_eq!(r.json()[key], false);
            }
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
            for key in ["live_authorized", "submission_authorized"] {
                assert_eq!(r.json()[key], false);
            }
            seen += 1;
        }
        assert_eq!(seen, 2);
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
        let report = r.json();
        let printed = report.to_string();
        assert!(!printed.contains("SECRET"));
        assert!(!printed.contains("PRIVATE"));
        for d in report["parameters"].as_array().unwrap() {
            assert!(!d["active"].is_null());
            assert!(!d["source"].is_null());
            assert_eq!(d["change"], "restart_required");
        }
        let export = dir.join("private.json");
        let _ = std::fs::remove_file(&export);
        r.export_private(&export).unwrap();
        assert!(r.export_private(&export).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&export).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        // The private JSON handoff of the pinned bridge (JFI-03 legacy edge) reads back exactly.
        let reread = parse(&["--config", export.to_str().unwrap()]).unwrap();
        assert_eq!(r.config, reread.config);
        // ... but productive native entry points refuse it; the text config passes.
        assert!(reread
            .native_only()
            .unwrap_err()
            .starts_with("CONFIG_JSON_REFUSED"));
        assert!(r.native_only().is_ok());
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
                4,
                71,
                "DqejpTXkqa3XVHsi21nU5HupQt2Ghuko261jNk4oRYoz".to_string()
            )
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
        assert_eq!(descriptors().len(), fields.as_object().unwrap().len());
        for d in descriptors() {
            let f = d["field"].as_str().unwrap();
            assert_eq!(env[d["env"].as_str().unwrap()], fields[f].to_string());
            assert!(Resolved::help().contains(d["name"].as_str().unwrap()));
        }
    }
}

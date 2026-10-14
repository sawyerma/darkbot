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
        pub fn descriptors() -> Vec<Value> { vec![$(json!({"name":stringify!($name).replace('_',"-"),"field":stringify!($name),"type":"u64","unit":$unit,"default":$default,"minimum":1,"maximum":u32::MAX,"env":$env,"change":"restart_required"}),)*] }
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
    fe_calls = 3, "requests", "P0_FE_CALLS";
    fe_seconds = 30, "s", "P0_FE_SECONDS";
    fe_max_mib = 16, "MiB", "P0_FE_MAX_MIB";
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Smoke,
    Standard,
    Performance,
    Custom,
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
        }
    }
}
#[derive(Clone, Debug)]
pub struct Resolved {
    pub config: RuntimeConfig,
    pub sources: BTreeMap<String, String>,
    pub print: bool,
    pub help: bool,
    pub export: Option<std::path::PathBuf>,
}
impl RuntimeConfig {
    fn profile(profile: Profile) -> Self {
        let mut c = Self {
            profile,
            ..Self::default()
        };
        match c.profile {
            Profile::Smoke => {
                c.limits.rpc_rps = 1;
                c.limits.rpc_burst = 1;
                c.limits.rpc_concurrency = 1;
                c.limits.rpc_attempts = 1;
                c.duration = "60s".into();
                c.limits.max_response_mib = 16;
                c.limits.max_total_mib = 64;
            }
            Profile::Performance => {
                c.limits.workers =
                    std::thread::available_parallelism().map_or(1, |n| n.get() as u64);
            }
            _ => {}
        }
        c
    }
    pub fn validate(&self) -> Result<(), String> {
        let limits = serde_json::to_value(&self.limits).map_err(|_| "CONFIG_ENCODE")?;
        for d in descriptors() {
            let f = d["field"].as_str().unwrap();
            if limits[f]
                .as_u64()
                .is_none_or(|n| n == 0 || n > u64::from(u32::MAX))
            {
                return Err(format!(
                    "invalid --{}: expected 1..={}",
                    d["name"].as_str().unwrap(),
                    u32::MAX
                ));
            }
        }
        if self.limits.rpc_backoff_ms > self.limits.rpc_backoff_max_ms
            || self.limits.wss_reconnect_ms > self.limits.wss_reconnect_max_ms
        {
            return Err("backoff initial exceeds maximum".into());
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
        duration_seconds(&self.duration)?;
        Ok(())
    }
    /// Legacy consumers are populated once, before threads start. Finance/mode/
    /// signing variables are intentionally absent from this mapping.
    pub fn environment(&self) -> BTreeMap<String, String> {
        let values = serde_json::to_value(&self.limits).unwrap();
        let mut env: BTreeMap<_, _> = descriptors()
            .iter()
            .map(|d| {
                (
                    d["env"].as_str().unwrap().into(),
                    values[d["field"].as_str().unwrap()].to_string(),
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
pub fn duration_seconds(s: &str) -> Result<u64, String> {
    let at = s.len().checked_sub(1).ok_or("empty duration")?;
    let (n, u) = s.split_at_checked(at).ok_or("invalid duration")?;
    let scale = match u {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        _ => return Err("duration requires s/m/h".into()),
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
    serde_json::from_value(json!(s))
        .map_err(|_| "profile must be smoke|standard|performance|custom".into())
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
            let v: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid config JSON")?;
            if !v.is_object() {
                return Err("config must be an object".into());
            }
            v
        } else {
            json!({})
        };
        if !cli.contains_key("profile") {
            if let Some(p) = file.get("profile") {
                profile_name = p.as_str().ok_or("invalid profile type")?.into();
            }
        }
        let base = RuntimeConfig::profile(profile(&profile_name)?);
        let mut v = serde_json::to_value(base).unwrap();
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
        ] {
            sources.insert(name.into(), format!("profile:{profile_name}"));
        }
        for d in descriptors() {
            let field = d["field"].as_str().unwrap();
            sources.insert(field.into(), format!("profile:{profile_name}"));
            if let Some(raw) = env(d["env"].as_str().unwrap()) {
                v["limits"][field] = json!(raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid numeric environment field {field}"))?);
                sources.insert(field.into(), "environment".into());
            }
        }
        for (f, e) in [
            ("rpc_endpoints", "P0_RPC_URL"),
            ("wss_endpoints", "P0_WS_URL"),
        ] {
            if let Some(url) = env(e) {
                v[f] = json!([url]);
                sources.insert(f.into(), "environment".into());
            }
        }
        if let Some(duration) = env("P0_MEASUREMENT_DURATION")
            .or_else(|| env("P0_ECON_DURATION_SECS").map(|v| format!("{v}s")))
        {
            v["duration"] = json!(duration);
            sources.insert("duration".into(), "environment".into());
        }
        for (k, val) in file.as_object().unwrap() {
            if k == "limits" {
                for (f, value) in val.as_object().ok_or("limits must be object")? {
                    if v["limits"].get(f).is_none() {
                        return Err(format!("unknown config field {f}"));
                    }
                    v["limits"][f] = value.clone();
                    sources.insert(f.clone(), "config_file".into());
                }
            } else {
                // Empty endpoint arrays mean inherit local endpoints; private
                // handoffs may be compiled before the launcher's owner-only env.
                if matches!(k.as_str(), "rpc_endpoints" | "wss_endpoints")
                    && val.as_array().is_some_and(|a| a.is_empty())
                {
                    continue;
                }
                if v.get(k).is_none() {
                    return Err(format!("unknown config field {k}"));
                }
                v[k] = val.clone();
                sources.insert(k.clone(), "config_file".into());
            }
        }
        for (k, val) in cli {
            if v["limits"].get(&k).is_some() {
                v["limits"][&k] = json!(val
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --{}", k.replace('_', "-")))?);
            } else {
                match k.as_str() {
                    "profile" | "duration" | "verification" => v[&k] = json!(val),
                    "rpc_endpoints" | "wss_endpoints" => {
                        v[&k] = json!(val.split(',').collect::<Vec<_>>())
                    }
                    "fallback" | "adaptive" | "diagnostics" => {
                        v[&k] = json!(match val.as_str() {
                            "true" | "on" => true,
                            "false" | "off" => false,
                            _ => return Err("expected on/off".into()),
                        })
                    }
                    _ => return Err(format!("unknown --{}", k.replace('_', "-"))),
                }
            }
            sources.insert(k, "cli".into());
        }
        let config: RuntimeConfig =
            serde_json::from_value(v).map_err(|_| "configuration type mismatch")?;
        config.validate()?;
        Ok(Self {
            config,
            sources,
            print,
            help,
            export,
        })
    }
    pub fn activate(self) -> Result<(), String> {
        let report = self.json();
        install(self.config)?;
        REPORT
            .set(report)
            .map_err(|_| "runtime report already initialized".to_string())
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
        for d in &mut parameters {
            let f = d["field"].as_str().unwrap().to_string();
            d["active"] = active[&f].clone();
            d["source"] = json!(self.sources.get(&f));
        }
        for (name, kind, unit, default, allowed) in [
            (
                "profile",
                "enum",
                "profile",
                json!("standard"),
                json!(["smoke", "standard", "performance", "custom"]),
            ),
            (
                "rpc_endpoints",
                "array<string>",
                "priority ordered HTTP URLs",
                json!([]),
                json!(["http", "https"]),
            ),
            (
                "wss_endpoints",
                "array<string>",
                "priority ordered WSS URLs",
                json!([]),
                json!(["ws", "wss"]),
            ),
            (
                "fallback",
                "bool",
                "enabled",
                json!(false),
                json!([true, false]),
            ),
            (
                "adaptive",
                "bool",
                "enabled",
                json!(true),
                json!([true, false]),
            ),
            (
                "duration",
                "duration",
                "s/m/h",
                json!("4h"),
                json!({"min_seconds":1,"max_seconds":u64::MAX/4/1_000_000_000}),
            ),
        ] {
            parameters.push(json!({"name":name.replace('_',"-"),"field":name,"type":kind,"unit":unit,"default":default,"allowed":allowed,"active":values[name],"source":self.sources.get(name),"change":"restart_required"}));
        }
        for (name, kind, default, allowed) in [
            (
                "verification",
                "enum",
                json!("off"),
                json!(["off", "index", "strict", "crosscheck"]),
            ),
            ("diagnostics", "bool", json!(false), json!([true, false])),
        ] {
            parameters.push(json!({"name":name,"field":name,"type":kind,"unit":"mode","default":default,"allowed":allowed,"active":values[name],"source":self.sources.get(name).cloned().unwrap_or_else(||"profile".into()),"change":"restart_required"}));
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
        let mut s=String::from("p0-longtail [--profile smoke|standard|performance|custom] [--config FILE] [--duration 10m|1h|5h|24h]\n--rpc-endpoints URL[,URL] --wss-endpoints URL[,URL] (priority order; declared FREE_ONLY)\n--fallback on|off --adaptive on|off --diagnostics on|off --verification off|index|strict|crosscheck --print-effective-config\nPrecedence: profile < environment < config file < CLI. All changes require restart.\nFinancial approval is separate. Profiles do not prove provider capacity.\n");
        for d in descriptors() {
            s += &format!(
                "--{} <u64> [{}; default={}; 1..={}]\n",
                d["name"].as_str().unwrap(),
                d["unit"].as_str().unwrap(),
                d["default"],
                u32::MAX
            );
        }
        s
    }
}
static ACTIVE: OnceLock<RuntimeConfig> = OnceLock::new();
static REPORT: OnceLock<Value> = OnceLock::new();
pub fn report() -> Option<&'static Value> {
    REPORT.get()
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
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
    #[test]
    fn precedence_redaction_private_handoff_and_metadata() {
        let dir = std::env::temp_dir().join(format!("p0-runtime-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("input.json");
        std::fs::write(&path,r#"{"profile":"smoke","limits":{"rpc_rps":7},"rpc_endpoints":["https://example.test/SECRET?key=PRIVATE"]}"#).unwrap();
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
        let reread = parse(&["--config", export.to_str().unwrap()]).unwrap();
        assert_eq!(r.config, reread.config);
        std::fs::remove_dir_all(dir).unwrap();
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

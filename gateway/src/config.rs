use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use thiserror::Error;

/// Messbudgets laut Wire V1.3 §5. Startkandidaten; nach Benchmark in CI fixieren.
#[derive(Debug, Clone)]
pub struct Limits {
    pub max_state_full_bytes: u32,
    pub heartbeat_interval_ms: u32,
    pub stale_after_missed_heartbeats: u32,
    pub unsubscribe_grace_ms: u32,
    pub max_catchup_retries: u32,
    pub handshake_timeout_ms: u64,
    pub max_subscriptions_per_client: usize,
    pub max_clients: usize,
    pub max_client_frame_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_state_full_bytes: 262_144,
            heartbeat_interval_ms: 15_000,
            stale_after_missed_heartbeats: 2,
            unsubscribe_grace_ms: 2_000,
            max_catchup_retries: 3,
            handshake_timeout_ms: 5_000,
            max_subscriptions_per_client: 32,
            max_clients: 16,
            max_client_frame_bytes: 4_096,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub bind: SocketAddr,
    /// Produktions-SPA (frontend/dist). None = kein Static-Serving (Dev über Vite-Proxy).
    pub static_dir: Option<PathBuf>,
    pub allowed_origins: Vec<String>,
    pub allowed_hosts: Vec<String>,
    pub session_cookie: String,
    pub limits: Limits,
    /// Quelle ohne Nachricht länger als stale => STALE-Overlay; länger als detach => DETACHED.
    pub source_stale_after_ms: u64,
    pub source_detach_after_ms: u64,
    pub source_reattach_backoff_ms: u64,
    pub overview_interval_ms: u64,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid {key}: {value}")]
    Invalid { key: &'static str, value: String },
    #[error("non-loopback bind {0} requires DARKBOT_GATEWAY_ALLOW_NON_LOOPBACK=1")]
    NonLoopback(SocketAddr),
}

impl GatewayConfig {
    /// Umgebungsvariablen DARKBOT_GATEWAY_*. Defaults sind loopback-only.
    pub fn from_env() -> Result<Self, ConfigError> {
        let bind: SocketAddr = env_or("DARKBOT_GATEWAY_BIND", "127.0.0.1:8787")
            .parse()
            .map_err(|_| ConfigError::Invalid { key: "DARKBOT_GATEWAY_BIND", value: env_or("DARKBOT_GATEWAY_BIND", "") })?;

        let loopback = match bind.ip() {
            IpAddr::V4(v4) => v4.is_loopback(),
            IpAddr::V6(v6) => v6.is_loopback(),
        };
        if !loopback && env_or("DARKBOT_GATEWAY_ALLOW_NON_LOOPBACK", "0") != "1" {
            return Err(ConfigError::NonLoopback(bind));
        }

        let port = bind.port();
        let default_hosts = format!("127.0.0.1:{port},localhost:{port}");
        let default_origins = format!("http://127.0.0.1:{port},http://localhost:{port}");

        let static_dir = std::env::var("DARKBOT_GATEWAY_STATIC_DIR").ok().map(PathBuf::from);

        Ok(Self {
            bind,
            static_dir,
            allowed_origins: csv(&env_or("DARKBOT_GATEWAY_ALLOWED_ORIGINS", &default_origins)),
            allowed_hosts: csv(&env_or("DARKBOT_GATEWAY_ALLOWED_HOSTS", &default_hosts)),
            session_cookie: env_or("DARKBOT_GATEWAY_SESSION_COOKIE", "darkbot_session"),
            limits: Limits::default(),
            source_stale_after_ms: parse_u64("DARKBOT_GATEWAY_SOURCE_STALE_MS", 10_000)?,
            source_detach_after_ms: parse_u64("DARKBOT_GATEWAY_SOURCE_DETACH_MS", 30_000)?,
            source_reattach_backoff_ms: parse_u64("DARKBOT_GATEWAY_SOURCE_BACKOFF_MS", 2_000)?,
            overview_interval_ms: parse_u64("DARKBOT_GATEWAY_OVERVIEW_INTERVAL_MS", 5_000)?,
        })
    }

    pub fn origin_allowed(&self, origin: &str) -> bool {
        self.allowed_origins.iter().any(|o| o.eq_ignore_ascii_case(origin))
    }

    pub fn host_allowed(&self, host: &str) -> bool {
        self.allowed_hosts.iter().any(|h| h.eq_ignore_ascii_case(host))
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn csv(s: &str) -> Vec<String> {
    s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}

fn parse_u64(key: &'static str, default: u64) -> Result<u64, ConfigError> {
    match std::env::var(key) {
        Ok(v) => v.parse().map_err(|_| ConfigError::Invalid { key, value: v }),
        Err(_) => Ok(default),
    }
}

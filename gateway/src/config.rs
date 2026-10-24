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
    pub feeds: Vec<FeedSpec>,
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
            .map_err(|_| ConfigError::Invalid {
                key: "DARKBOT_GATEWAY_BIND",
                value: env_or("DARKBOT_GATEWAY_BIND", ""),
            })?;

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

        let static_dir = std::env::var("DARKBOT_GATEWAY_STATIC_DIR")
            .ok()
            .map(PathBuf::from);

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
            feeds: parse_feeds(&env_or("DARKBOT_GATEWAY_FEEDS", ""))?,
        })
    }

    pub fn origin_allowed(&self, origin: &str) -> bool {
        self.allowed_origins
            .iter()
            .any(|o| o.eq_ignore_ascii_case(origin))
    }

    /// The browser reaches the gateway only through TLS (terminated in front of it) when every
    /// allowed origin is HTTPS; the session cookie then carries `Secure`.
    pub fn secure_cookie(&self) -> bool {
        only_https(&self.allowed_origins)
    }

    pub fn host_allowed(&self, host: &str) -> bool {
        self.allowed_hosts
            .iter()
            .any(|h| h.eq_ignore_ascii_case(host))
    }
}

fn only_https(origins: &[String]) -> bool {
    !origins.is_empty()
        && origins
            .iter()
            .all(|o| o.to_ascii_lowercase().starts_with("https://"))
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn parse_u64(key: &'static str, default: u64) -> Result<u64, ConfigError> {
    match std::env::var(key) {
        Ok(v) => v
            .parse()
            .map_err(|_| ConfigError::Invalid { key, value: v }),
        Err(_) => Ok(default),
    }
}

#[derive(Debug, Clone)]
pub struct FeedSpec {
    pub project: crate::proto::ProjectId,
    pub label: &'static str,
    pub socket: PathBuf,
}
pub fn parse_feeds(input: &str) -> Result<Vec<FeedSpec>, ConfigError> {
    use crate::proto::ProjectId;
    let mut out: Vec<FeedSpec> = Vec::new();
    for entry in input.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (name, path) = entry.split_once('=').ok_or_else(|| ConfigError::Invalid {
            key: "DARKBOT_GATEWAY_FEEDS",
            value: entry.into(),
        })?;
        let (project, label) = match name.trim() {
            "project0" => (ProjectId::Project0, "Project 0"),
            "bsc" => (ProjectId::Bsc, "BSC"),
            "solana" => (ProjectId::Solana, "Solana"),
            _ => {
                return Err(ConfigError::Invalid {
                    key: "DARKBOT_GATEWAY_FEEDS",
                    value: entry.into(),
                })
            }
        };
        if path.trim().is_empty()
            || !path.trim().starts_with('/')
            || out.iter().any(|f| f.project == project)
        {
            return Err(ConfigError::Invalid {
                key: "DARKBOT_GATEWAY_FEEDS",
                value: entry.into(),
            });
        }
        out.push(FeedSpec {
            project,
            label,
            socket: PathBuf::from(path.trim()),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::only_https;
    #[test]
    fn the_cookie_is_secure_only_behind_https_origins() {
        let o = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(!only_https(&o(&[
            "http://127.0.0.1:8787",
            "http://localhost:8787"
        ])));
        assert!(!only_https(&o(&[
            "https://darkbot.example",
            "http://127.0.0.1:8787"
        ])));
        assert!(!only_https(&o(&[])));
        assert!(only_https(&o(&[
            "https://darkbot.example",
            "HTTPS://ops.example"
        ])));
    }
}

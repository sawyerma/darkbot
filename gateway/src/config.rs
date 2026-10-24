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
    /// Command idempotency (Wire V1.3 §7): accepted distance between a new command's
    /// `issued_at_unix_ns` and the gateway receive time (clock offset plus delivery delay).
    /// Default: the client's own staleness threshold, heartbeat interval × missed heartbeats.
    pub command_clock_window_ms: u64,
    /// Minimum age (since its last update) before a receipt may be forgotten. At least
    /// `min_command_retention_ms(command_clock_window_ms)`, else the replay rule would not hold.
    pub command_retention_ms: u64,
    /// Command records kept before the oldest receipts move to the archive; also the archive size.
    pub command_ledger_records: usize,
}

/// A request stamped within the window and forgotten no earlier than this after its receipt can
/// never be accepted again: its stamp is then older than the window. Hence twice the window.
pub fn min_command_retention_ms(command_clock_window_ms: u64) -> u64 {
    command_clock_window_ms.saturating_mul(2)
}

impl Default for Limits {
    fn default() -> Self {
        let heartbeat_interval_ms = 15_000;
        let stale_after_missed_heartbeats = 2;
        let command_clock_window_ms =
            u64::from(heartbeat_interval_ms) * u64::from(stale_after_missed_heartbeats);
        Self {
            max_state_full_bytes: 262_144,
            heartbeat_interval_ms,
            stale_after_missed_heartbeats,
            unsubscribe_grace_ms: 2_000,
            max_catchup_retries: 3,
            handshake_timeout_ms: 5_000,
            max_subscriptions_per_client: 32,
            max_clients: 16,
            max_client_frame_bytes: 4_096,
            command_clock_window_ms,
            command_retention_ms: min_command_retention_ms(command_clock_window_ms),
            command_ledger_records: 1024,
        }
    }
}

impl Limits {
    /// DARKBOT_GATEWAY_COMMAND_* overrides; a retention below twice the window is refused.
    fn with_command_env(mut self) -> Result<Self, ConfigError> {
        const WINDOW: &str = "DARKBOT_GATEWAY_COMMAND_CLOCK_WINDOW_MS";
        const RETENTION: &str = "DARKBOT_GATEWAY_COMMAND_RETENTION_MS";
        const RECORDS: &str = "DARKBOT_GATEWAY_COMMAND_LEDGER_RECORDS";
        self.command_clock_window_ms = parse_u64(WINDOW, self.command_clock_window_ms)?;
        let min = min_command_retention_ms(self.command_clock_window_ms);
        self.command_retention_ms = parse_u64(RETENTION, min)?;
        self.command_ledger_records =
            usize::try_from(parse_u64(RECORDS, self.command_ledger_records as u64)?).unwrap_or(0);
        let invalid = |key, value: u64| ConfigError::Invalid {
            key,
            value: value.to_string(),
        };
        if self.command_clock_window_ms == 0 {
            return Err(invalid(WINDOW, 0));
        }
        if self.command_retention_ms < min {
            return Err(invalid(RETENTION, self.command_retention_ms));
        }
        if self.command_ledger_records == 0 {
            return Err(invalid(RECORDS, 0));
        }
        Ok(self)
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

        if !bind_allowed(
            bind.ip(),
            env_or("DARKBOT_GATEWAY_ALLOW_NON_LOOPBACK", "0") == "1",
        ) {
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
            limits: Limits::default().with_command_env()?,
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

/// Loopback always; any other address only with the explicit opt-in.
fn bind_allowed(ip: IpAddr, allow_non_loopback: bool) -> bool {
    let loopback = match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    };
    loopback || allow_non_loopback
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
    use super::{bind_allowed, min_command_retention_ms, only_https, Limits};
    #[test]
    fn the_command_window_follows_the_configured_liveness_and_retention_covers_twice_it() {
        let l = Limits::default();
        assert_eq!(
            l.command_clock_window_ms,
            u64::from(l.heartbeat_interval_ms) * u64::from(l.stale_after_missed_heartbeats)
        );
        assert_eq!(
            l.command_retention_ms,
            min_command_retention_ms(l.command_clock_window_ms)
        );
        // Overrides are read from the environment; a retention below twice the window is refused.
        std::env::set_var("DARKBOT_GATEWAY_COMMAND_CLOCK_WINDOW_MS", "1000");
        std::env::set_var("DARKBOT_GATEWAY_COMMAND_RETENTION_MS", "1999");
        assert!(Limits::default().with_command_env().is_err());
        std::env::set_var("DARKBOT_GATEWAY_COMMAND_RETENTION_MS", "2000");
        std::env::set_var("DARKBOT_GATEWAY_COMMAND_LEDGER_RECORDS", "64");
        let o = Limits::default().with_command_env().unwrap();
        assert_eq!(
            (
                o.command_clock_window_ms,
                o.command_retention_ms,
                o.command_ledger_records
            ),
            (1000, 2000, 64)
        );
        for k in ["WINDOW", "RETENTION", "LEDGER_RECORDS"] {
            let key = format!(
                "DARKBOT_GATEWAY_COMMAND_{}",
                if k == "WINDOW" {
                    "CLOCK_WINDOW_MS"
                } else if k == "RETENTION" {
                    "RETENTION_MS"
                } else {
                    k
                }
            );
            std::env::remove_var(key);
        }
    }
    #[test]
    fn a_non_loopback_bind_needs_the_explicit_opt_in() {
        use std::net::IpAddr;
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert!(bind_allowed(ip("127.0.0.1"), false));
        assert!(bind_allowed(ip("::1"), false));
        assert!(!bind_allowed(ip("0.0.0.0"), false));
        assert!(!bind_allowed(ip("192.168.1.20"), false));
        assert!(bind_allowed(ip("192.168.1.20"), true));
    }
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

//! Command-line, configuration and text helpers shared by the G0.1 freeze
//! binaries.

use darkbot_bsc_qualification::c1_common::Args;
use std::path::Path;

/// The network bounds and their defaults (each overridable by its flag).
pub mod net {
    /// `--rpc-config`: the public readiness configuration (never a secret file).
    pub const RPC_CONFIG: &str = "bsc/config/readiness.public.env";
    pub const RPC_CONFIG_KEY: &str = "BSC_READINESS_RPC_HTTPS";
    /// `--out-root`.
    pub const OUT_ROOT: &str = "bsc/native/evidence/g01";
    /// `--confirmations`: the reference block is this far below the head.
    pub const CONFIRMATIONS: u64 = 5;
    /// `--header-batch`: headers per JSON-RPC batch (the public dataseed
    /// answers 100).
    pub const HEADER_BATCH: u64 = 100;
    /// `--rpc-connect-timeout-s`, `--rpc-max-time-s`, `--rpc-attempts`,
    /// `--rpc-retry-pause-ms`.
    pub const CONNECT_TIMEOUT_S: u32 = 5;
    pub const MAX_TIME_S: u32 = 30;
    pub const ATTEMPTS: u32 = 3;
    pub const RETRY_PAUSE_MS: u64 = 500;
    /// `--sourcify-base`, `--sourcify-max-bytes` (the record bound).
    pub const SOURCIFY_BASE: &str = "https://sourcify.dev/server";
    pub const SOURCIFY_MAX_BYTES: u64 = 1024 * 1024;
}

pub fn number<T: std::str::FromStr>(args: &Args, flag: &str, default: T) -> Result<T, String> {
    match args.opt(flag)? {
        Some(v) => v
            .parse()
            .map_err(|_| format!("--{flag}: not a number: {v}")),
        None => Ok(default),
    }
}

/// A numeric flag inside `min..=max` (every tunable has its bounds).
pub fn bounded<T: std::str::FromStr + PartialOrd + std::fmt::Display + Copy>(
    args: &Args,
    flag: &str,
    default: T,
    min: T,
    max: T,
) -> Result<T, String> {
    let v = number(args, flag, default)?;
    if v < min || v > max {
        return Err(format!("--{flag} {v} is outside {min}..={max}"));
    }
    Ok(v)
}

/// `BSC_READINESS_RPC_HTTPS` of a public `KEY=VALUE` file.
pub fn rpc_from_config(path: &Path) -> Result<String, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase());
    if name.is_none_or(|n| n.contains("secret")) {
        return Err(format!(
            "refusing {}: not a public configuration",
            path.display()
        ));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    text.lines()
        .filter_map(|l| l.trim().split_once('='))
        .find(|(k, _)| k.trim() == net::RPC_CONFIG_KEY)
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("{} has no {}", path.display(), net::RPC_CONFIG_KEY))
}

/// Text fields are bounded by the record format.
pub fn bounded_text(text: &str) -> String {
    let max = darkbot_bsc_record::DEFAULT_MAX_LEN as usize;
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max - 20;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{} [... truncated]", &text[..end])
}

/// `YYYYMMDDTHHMMSSZ` of a Unix time in milliseconds (proleptic Gregorian).
pub fn utc_stamp(unix_ms: u64) -> String {
    let secs = unix_ms / 1000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        rem / 3600,
        (rem / 60) % 60,
        rem % 60
    )
}

//! UTC formatting for capture and persisted evidence.
use super::*;

// ------------------------------------------------------------------------------------------
// UTC formatting (no extra dependency)
// ------------------------------------------------------------------------------------------

pub(super) fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

pub(super) fn utc_parts(unix_s: u64) -> (i64, u32, u32, u64, u64, u64) {
    let days = (unix_s / 86_400) as i64;
    let rem = unix_s % 86_400;
    let (y, m, d) = civil_from_days(days);
    (y, m, d, rem / 3_600, (rem % 3_600) / 60, rem % 60)
}

/// `YYYY-MM-DDTHH:MM:SSZ`
pub fn utc_iso(unix_s: u64) -> String {
    let (y, m, d, hh, mm, ss) = utc_parts(unix_s);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// `YYYYMMDDTHHMMSSZ`
pub fn utc_compact(unix_s: u64) -> String {
    let (y, m, d, hh, mm, ss) = utc_parts(unix_s);
    format!("{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}Z")
}

pub(super) fn unix_now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

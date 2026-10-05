//! What the build of the harness is bound to, and wall-clock timestamps.

use std::time::{SystemTime, UNIX_EPOCH};

use darkbot_runner::inclusion::tls::TLS_TRANSPORT_IMPLEMENTATION;

use crate::json::Json;

/// The baseline of the frozen production code this qualification is for.
pub const EXPECTED_BASELINE: &str = "92fa5204777e566acbaccf111721d9295ffb3a90";
/// The rustls version the transport must be built with.
pub const EXPECTED_RUSTLS: &str = "0.23.45";
/// The ring version the transport must be built with.
pub const EXPECTED_RING: &str = "0.17.14";
/// Name and version of this harness.
pub const HARNESS: &str = "u5_tls_qualify v1";

/// The build of the harness: source, toolchain, locked TLS crates,
/// profile, platform, transport. "unknown" wherever the build could not
/// tell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// The baseline the build script was written for.
    pub baseline: String,
    /// The commit the harness was built from.
    pub git_head: String,
    /// Whether tracked files differed from that commit: true, false or
    /// unknown.
    pub git_dirty: String,
    /// Whether the baseline is an ancestor of that commit.
    pub baseline_ancestor: String,
    /// Whether core, wire and runner sources are unchanged since the
    /// baseline, committed and in the working tree.
    pub frozen_sources_unchanged: String,
    /// `rustc --version` of the build.
    pub rustc: String,
    /// `cargo --version` of the build.
    pub cargo: String,
    /// rustls version in the lock file.
    pub rustls: String,
    /// ring version in the lock file.
    pub ring: String,
    /// Cargo profile: release or debug.
    pub profile: String,
    /// Operating system.
    pub os: String,
    /// Architecture.
    pub arch: String,
    /// The transport implementation id.
    pub transport_implementation: String,
    /// The harness.
    pub harness: String,
}

impl Identity {
    /// The identity this binary was built with.
    pub fn of_this_build() -> Self {
        Self {
            baseline: env!("Q5A_BUILD_BASELINE").to_owned(),
            git_head: env!("Q5A_BUILD_GIT_HEAD").to_owned(),
            git_dirty: env!("Q5A_BUILD_GIT_DIRTY").to_owned(),
            baseline_ancestor: env!("Q5A_BUILD_BASELINE_ANCESTOR").to_owned(),
            frozen_sources_unchanged: env!("Q5A_BUILD_FROZEN_UNCHANGED").to_owned(),
            rustc: env!("Q5A_BUILD_RUSTC").to_owned(),
            cargo: env!("Q5A_BUILD_CARGO").to_owned(),
            rustls: env!("Q5A_BUILD_RUSTLS").to_owned(),
            ring: env!("Q5A_BUILD_RING").to_owned(),
            profile: env!("Q5A_BUILD_PROFILE").to_owned(),
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            transport_implementation: TLS_TRANSPORT_IMPLEMENTATION.to_owned(),
            harness: HARNESS.to_owned(),
        }
    }

    /// The JSON form.
    pub fn to_json(&self) -> Json {
        Json::object([
            ("baseline", Json::str(&self.baseline)),
            ("git_head", Json::str(&self.git_head)),
            ("git_dirty", Json::str(&self.git_dirty)),
            ("baseline_ancestor", Json::str(&self.baseline_ancestor)),
            (
                "frozen_sources_unchanged",
                Json::str(&self.frozen_sources_unchanged),
            ),
            ("rustc", Json::str(&self.rustc)),
            ("cargo", Json::str(&self.cargo)),
            ("rustls", Json::str(&self.rustls)),
            ("ring", Json::str(&self.ring)),
            ("profile", Json::str(&self.profile)),
            ("os", Json::str(&self.os)),
            ("arch", Json::str(&self.arch)),
            (
                "transport_implementation",
                Json::str(&self.transport_implementation),
            ),
            ("harness", Json::str(&self.harness)),
        ])
    }
}

/// The current wall-clock time as RFC 3339, UTC, to the second.
pub fn now_rfc3339() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    rfc3339(seconds)
}

/// `seconds` since the Unix epoch as RFC 3339, UTC, to the second.
pub fn rfc3339(seconds: u64) -> String {
    let days = seconds / 86_400;
    let rest = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// Year, month and day of `days` since 1970-01-01 (H. Hinnant's algorithm).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

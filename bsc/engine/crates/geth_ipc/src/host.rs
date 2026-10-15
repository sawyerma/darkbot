//! C1.6 host profile: topology evidence, macOS QoS and the initial
//! scheduling-profile hint.
//!
//! macOS on Apple silicon is the development and C1 diagnostic host. This
//! module records what the host reports about itself and can ask macOS for a
//! QoS class on a permanent latency-critical thread. Three limits are part of
//! its contract:
//!
//! - Every value is either read from the host or reported as unavailable
//!   with a reason. Nothing is guessed and no fallback changes behaviour
//!   silently.
//! - QoS is advisory. It is not CPU pinning and promises no performance core.
//! - Topology yields a [`ProfileHint`]: the first experiment to run. It never
//!   qualifies a profile. Measured latency decides, and a result on this host
//!   says nothing about the dedicated Linux target of C3.
//!
//! Profile S keeps the qualified Unit-5 builder execution sequential.
//! Profile P runs parallel builder workers; those exist only with Unit-5
//! Runner Rev4 (C3), so in C1 a request for P is refused, not downgraded.

use std::fmt::Write as _;

use crate::v2::crc32c;

/// A value read from the host, or the reason it could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Probe<T> {
    Value(T),
    Unavailable(&'static str),
}

impl<T> Probe<T> {
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Value(value) => Some(value),
            Self::Unavailable(_) => None,
        }
    }

    fn from_option(value: Option<T>, reason: &'static str) -> Self {
        match value {
            Some(value) => Self::Value(value),
            None => Self::Unavailable(reason),
        }
    }
}

/// One Apple performance level (`hw.perflevelN.*`). Level 0 is the
/// highest-performance level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerfLevel {
    pub index: u32,
    pub name: Probe<String>,
    pub physical_cpus: Probe<u64>,
    pub logical_cpus: Probe<u64>,
    pub l1d_cache_bytes: Probe<u64>,
    pub l1i_cache_bytes: Probe<u64>,
    pub l2_cache_bytes: Probe<u64>,
    pub cpus_per_l2: Probe<u64>,
}

/// What the host reports about CPUs, caches and memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostTopology {
    pub os: &'static str,
    pub arch: &'static str,
    pub model: Probe<String>,
    pub cpu_brand: Probe<String>,
    pub os_release: Probe<String>,
    pub physical_cpus: Probe<u64>,
    pub logical_cpus: Probe<u64>,
    pub cache_line_bytes: Probe<u64>,
    pub page_bytes: Probe<u64>,
    pub memory_bytes: Probe<u64>,
    pub perf_levels: Probe<Vec<PerfLevel>>,
}

/// Where topology values come from. The host implementation reads `sysctl`;
/// tests supply fixed values.
pub trait HostSource {
    fn text(&self, name: &str) -> Option<String>;
    fn number(&self, name: &str) -> Option<u64>;
}

const NO_SYSCTL: &str = "sysctl name not available on this host";
const NO_PERF_LEVELS: &str = "hw.nperflevels not available: no Apple performance levels";
const TOO_MANY_PERF_LEVELS: &str = "hw.nperflevels is implausible";

impl HostTopology {
    /// Reads the topology of this host.
    pub fn capture() -> Self {
        Self::capture_with(&SystemSource)
    }

    /// Reads the topology from `source`.
    pub fn capture_with(source: &dyn HostSource) -> Self {
        let number = |name: &str| Probe::from_option(source.number(name), NO_SYSCTL);
        let text = |name: &str| Probe::from_option(source.text(name), NO_SYSCTL);
        let perf_levels = match source.number("hw.nperflevels") {
            None => Probe::Unavailable(NO_PERF_LEVELS),
            Some(count) if count > 8 => Probe::Unavailable(TOO_MANY_PERF_LEVELS),
            Some(count) => {
                let levels = (0..count as u32).map(|index| {
                    let key = |field: &str| format!("hw.perflevel{index}.{field}");
                    PerfLevel {
                        index,
                        name: text(&key("name")),
                        physical_cpus: number(&key("physicalcpu")),
                        logical_cpus: number(&key("logicalcpu")),
                        l1d_cache_bytes: number(&key("l1dcachesize")),
                        l1i_cache_bytes: number(&key("l1icachesize")),
                        l2_cache_bytes: number(&key("l2cachesize")),
                        cpus_per_l2: number(&key("cpusperl2")),
                    }
                });
                Probe::Value(levels.collect())
            }
        };
        Self {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            model: text("hw.model"),
            cpu_brand: text("machdep.cpu.brand_string"),
            os_release: text("kern.osrelease"),
            physical_cpus: number("hw.physicalcpu"),
            logical_cpus: number("hw.logicalcpu"),
            cache_line_bytes: number("hw.cachelinesize"),
            page_bytes: number("hw.pagesize"),
            memory_bytes: number("hw.memsize"),
            perf_levels,
        }
    }

    /// Physical CPUs of performance level 0, if the host reports levels.
    pub fn performance_cpus(&self) -> Option<u64> {
        let level = self.perf_levels.value()?.first()?;
        level.physical_cpus.value().copied()
    }

    /// The topology as canonical JSON: fixed key order, no whitespace
    /// variation, no timestamps. The same host yields the same bytes.
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(1024);
        out.push('{');
        json_text(&mut out, "os", &Probe::Value(self.os.to_string()));
        json_text(&mut out, "arch", &Probe::Value(self.arch.to_string()));
        json_text(&mut out, "model", &self.model);
        json_text(&mut out, "cpu_brand", &self.cpu_brand);
        json_text(&mut out, "os_release", &self.os_release);
        json_number(&mut out, "physical_cpus", &self.physical_cpus);
        json_number(&mut out, "logical_cpus", &self.logical_cpus);
        json_number(&mut out, "cache_line_bytes", &self.cache_line_bytes);
        json_number(&mut out, "page_bytes", &self.page_bytes);
        json_number(&mut out, "memory_bytes", &self.memory_bytes);
        out.push_str("\"perf_levels\":");
        match &self.perf_levels {
            Probe::Unavailable(reason) => json_unavailable(&mut out, reason),
            Probe::Value(levels) => {
                out.push('[');
                for (position, level) in levels.iter().enumerate() {
                    if position > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "{{\"index\":{},", level.index);
                    json_text(&mut out, "name", &level.name);
                    json_number(&mut out, "physical_cpus", &level.physical_cpus);
                    json_number(&mut out, "logical_cpus", &level.logical_cpus);
                    json_number(&mut out, "l1d_cache_bytes", &level.l1d_cache_bytes);
                    json_number(&mut out, "l1i_cache_bytes", &level.l1i_cache_bytes);
                    json_number(&mut out, "l2_cache_bytes", &level.l2_cache_bytes);
                    json_number(&mut out, "cpus_per_l2", &level.cpus_per_l2);
                    close_object(&mut out);
                }
                out.push(']');
            }
        }
        out.push('}');
        out
    }

    /// CRC32C of [`HostTopology::to_json`]: the topology identity bound into
    /// measurement evidence.
    pub fn fingerprint(&self) -> u32 {
        crc32c(self.to_json().as_bytes())
    }
}

fn json_escape(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ch if (ch as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

fn json_unavailable(out: &mut String, reason: &str) {
    out.push_str("{\"unavailable\":");
    json_escape(out, reason);
    out.push('}');
}

fn json_text(out: &mut String, key: &str, value: &Probe<String>) {
    let _ = write!(out, "\"{key}\":");
    match value {
        Probe::Value(text) => json_escape(out, text),
        Probe::Unavailable(reason) => json_unavailable(out, reason),
    }
    out.push(',');
}

fn json_number(out: &mut String, key: &str, value: &Probe<u64>) {
    let _ = write!(out, "\"{key}\":");
    match value {
        Probe::Value(number) => {
            let _ = write!(out, "{number}");
        }
        Probe::Unavailable(reason) => json_unavailable(out, reason),
    }
    out.push(',');
}

/// Replaces the trailing comma of an object's last member with `}`.
fn close_object(out: &mut String) {
    if out.ends_with(',') {
        out.pop();
    }
    out.push('}');
}

// ---------------------------------------------------------------------------
// sysctl
// ---------------------------------------------------------------------------

/// The host's `sysctl` values. On hosts without `sysctlbyname` every name is
/// unavailable.
pub struct SystemSource;

#[cfg(target_os = "macos")]
fn sysctl_raw(name: &str) -> Option<Vec<u8>> {
    let name = std::ffi::CString::new(name).ok()?;
    let mut len: libc::size_t = 0;
    let null = std::ptr::null_mut();
    let rc = unsafe { libc::sysctlbyname(name.as_ptr(), null, &mut len, null, 0) };
    if rc != 0 || len == 0 || len > 4096 {
        return None;
    }
    let mut buf = vec![0u8; len];
    let out = buf.as_mut_ptr().cast();
    let rc = unsafe { libc::sysctlbyname(name.as_ptr(), out, &mut len, null, 0) };
    if rc != 0 || len > buf.len() {
        return None;
    }
    buf.truncate(len);
    Some(buf)
}

#[cfg(not(target_os = "macos"))]
fn sysctl_raw(_name: &str) -> Option<Vec<u8>> {
    None
}

impl HostSource for SystemSource {
    fn text(&self, name: &str) -> Option<String> {
        let raw = sysctl_raw(name)?;
        let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
        let text = std::str::from_utf8(&raw[..end]).ok()?;
        (!text.is_empty()).then(|| text.to_string())
    }

    fn number(&self, name: &str) -> Option<u64> {
        let raw = sysctl_raw(name)?;
        match raw.len() {
            4 => {
                let mut bytes = [0u8; 4];
                bytes.copy_from_slice(&raw);
                Some(u64::from(u32::from_ne_bytes(bytes)))
            }
            8 => {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&raw);
                Some(u64::from_ne_bytes(bytes))
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// QoS
// ---------------------------------------------------------------------------

/// The macOS QoS classes a permanent thread may request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QosClass {
    UserInteractive,
    UserInitiated,
    Default,
    Utility,
    Background,
}

impl QosClass {
    /// The `qos_class_t` value of `<sys/qos.h>`.
    pub fn raw(self) -> u32 {
        match self {
            Self::UserInteractive => 0x21,
            Self::UserInitiated => 0x19,
            Self::Default => 0x15,
            Self::Utility => 0x11,
            Self::Background => 0x09,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::UserInteractive => "user-interactive",
            Self::UserInitiated => "user-initiated",
            Self::Default => "default",
            Self::Utility => "utility",
            Self::Background => "background",
        }
    }

    /// Parses a configured class name; `None` for an unknown name.
    pub fn parse(name: &str) -> Option<Self> {
        let classes = [
            Self::UserInteractive,
            Self::UserInitiated,
            Self::Default,
            Self::Utility,
            Self::Background,
        ];
        classes.into_iter().find(|class| class.name() == name)
    }
}

/// The result of a QoS request for the calling thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QosOutcome {
    /// macOS accepted the request and reports this class for the thread.
    Applied(QosClass),
    /// macOS accepted the request but reports another raw class.
    AppliedButObserved { requested: QosClass, observed: u32 },
    /// The request failed with this error number; scheduling is unchanged.
    Failed { requested: QosClass, errno: i32 },
    /// This host has no thread QoS; scheduling is unchanged.
    Unsupported(&'static str),
}

impl QosOutcome {
    pub fn describe(self) -> String {
        match self {
            Self::Applied(class) => format!("applied:{}", class.name()),
            Self::AppliedButObserved {
                requested,
                observed,
            } => format!("applied:{}:observed-raw:{observed:#x}", requested.name()),
            Self::Failed { requested, errno } => {
                format!("failed:{}:errno:{errno}", requested.name())
            }
            Self::Unsupported(reason) => format!("unsupported:{reason}"),
        }
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
    fn pthread_get_qos_class_np(
        thread: libc::pthread_t,
        qos_class: *mut u32,
        relative_priority: *mut i32,
    ) -> i32;
}

/// Requests `class` for the calling thread. For permanent latency-critical
/// threads only, called once when the thread starts; never per BUY. The
/// request is advisory: it is not CPU pinning.
#[cfg(target_os = "macos")]
pub fn set_current_thread_qos(class: QosClass) -> QosOutcome {
    let rc = unsafe { pthread_set_qos_class_self_np(class.raw(), 0) };
    if rc != 0 {
        return QosOutcome::Failed {
            requested: class,
            errno: rc,
        };
    }
    let mut observed = 0u32;
    let mut relative = 0i32;
    let thread = unsafe { libc::pthread_self() };
    let rc = unsafe { pthread_get_qos_class_np(thread, &mut observed, &mut relative) };
    if rc == 0 && observed != class.raw() {
        return QosOutcome::AppliedButObserved {
            requested: class,
            observed,
        };
    }
    QosOutcome::Applied(class)
}

/// Requests `class` for the calling thread. This host has no thread QoS, so
/// nothing changes and the outcome says so.
#[cfg(not(target_os = "macos"))]
pub fn set_current_thread_qos(_class: QosClass) -> QosOutcome {
    QosOutcome::Unsupported("thread QoS classes exist on macOS only")
}

// ---------------------------------------------------------------------------
// Scheduling profiles
// ---------------------------------------------------------------------------

/// The two benchmarkable scheduling profiles of the C1 plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedulingProfile {
    /// Sequential builder execution; only C1 local-geth work is parallel.
    S,
    /// Parallel builder workers (Unit-5 Runner Rev4, C3).
    P,
}

impl SchedulingProfile {
    pub fn name(self) -> &'static str {
        match self {
            Self::S => "S",
            Self::P => "P",
        }
    }
}

/// The first experiment the topology suggests. It is a hint and nothing
/// else: it qualifies no profile and no core count passes or fails anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileHint {
    pub profile: SchedulingProfile,
    pub basis: &'static str,
    pub performance_cpus: Option<u64>,
}

/// Permanent latency-critical Rust threads of Profile P: HOT0 and one
/// worker per builder transport (48Club, BlockRazor).
const PROFILE_P_CRITICAL_THREADS: u64 = 3;

/// Suggests the profile to measure first.
pub fn initial_profile_hint(topology: &HostTopology) -> ProfileHint {
    let performance_cpus = topology.performance_cpus();
    let (profile, basis) = match performance_cpus {
        None => (
            SchedulingProfile::S,
            "performance levels unavailable; start with the sequential profile",
        ),
        Some(cpus) if cpus > PROFILE_P_CRITICAL_THREADS => (
            SchedulingProfile::P,
            "performance level 0 has more CPUs than Profile P has critical threads",
        ),
        Some(_) => (
            SchedulingProfile::S,
            "performance level 0 has no CPU to spare for Profile P's critical threads",
        ),
    };
    ProfileHint {
        profile,
        basis,
        performance_cpus,
    }
}

/// What the configuration asks for and what the build can run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileRequest {
    /// The configured profile; `None` follows the topology hint.
    pub configured: Option<SchedulingProfile>,
    /// Whether parallel builder workers exist in this build. `false` in C1.
    pub parallel_builders_available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileSource {
    Configured,
    TopologyHint,
    /// The hint was P, but this build has no parallel builders.
    HintNotRunnable,
}

/// The profile to run, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileSelection {
    pub profile: SchedulingProfile,
    pub source: ProfileSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileError {
    /// Profile P was configured but this build has no parallel builders.
    /// The configured profile is never replaced silently.
    ParallelBuildersUnavailable,
}

/// Chooses the profile to run. An explicit configuration wins over the hint
/// and is either honoured or refused. Without one, the hint is followed where
/// the build can run it; a P hint on a build without parallel builders runs S
/// and the selection says so.
pub fn select_profile(
    request: ProfileRequest,
    hint: ProfileHint,
) -> Result<ProfileSelection, ProfileError> {
    let parallel = request.parallel_builders_available;
    let runnable = |profile| profile == SchedulingProfile::S || parallel;
    match request.configured {
        Some(profile) if runnable(profile) => Ok(ProfileSelection {
            profile,
            source: ProfileSource::Configured,
        }),
        Some(_) => Err(ProfileError::ParallelBuildersUnavailable),
        None if runnable(hint.profile) => Ok(ProfileSelection {
            profile: hint.profile,
            source: ProfileSource::TopologyHint,
        }),
        None => Ok(ProfileSelection {
            profile: SchedulingProfile::S,
            source: ProfileSource::HintNotRunnable,
        }),
    }
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

/// The host/profile evidence of C1.6.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostProfileReport {
    pub topology: HostTopology,
    pub hint: ProfileHint,
    pub selection: Result<ProfileSelection, ProfileError>,
    /// The QoS outcome of the thread that built the report, if requested.
    pub qos: Option<QosOutcome>,
}

impl HostProfileReport {
    /// Captures the host and resolves the profile. `qos` is applied to the
    /// calling thread.
    pub fn capture(request: ProfileRequest, qos: Option<QosClass>) -> Self {
        let topology = HostTopology::capture();
        let hint = initial_profile_hint(&topology);
        Self {
            hint,
            selection: select_profile(request, hint),
            qos: qos.map(set_current_thread_qos),
            topology,
        }
    }

    /// The report as JSON. The topology part and its fingerprint are stable
    /// for a host; the report carries no timestamp.
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(2048);
        out.push_str("{\"schema\":\"darkbot.c1.host_profile.v1\",");
        let _ = write!(out, "\"topology\":{},", self.topology.to_json());
        let fingerprint = self.topology.fingerprint();
        let _ = write!(
            out,
            "\"topology_fingerprint_crc32c\":\"{fingerprint:#010x}\","
        );
        out.push_str("\"profile_hint\":{");
        let _ = write!(out, "\"profile\":\"{}\",", self.hint.profile.name());
        out.push_str("\"basis\":");
        json_escape(&mut out, self.hint.basis);
        out.push_str(",\"hint_only\":true},");
        out.push_str("\"profile_selection\":");
        match self.selection {
            Ok(selection) => {
                let source = match selection.source {
                    ProfileSource::Configured => "configured",
                    ProfileSource::TopologyHint => "topology-hint",
                    ProfileSource::HintNotRunnable => "hint-not-runnable-in-this-build",
                };
                let profile = selection.profile.name();
                let _ = write!(
                    out,
                    "{{\"profile\":\"{profile}\",\"source\":\"{source}\"}},"
                );
            }
            Err(ProfileError::ParallelBuildersUnavailable) => {
                out.push_str("{\"error\":\"parallel-builders-unavailable\"},");
            }
        }
        out.push_str("\"qos\":");
        match self.qos {
            Some(outcome) => json_escape(&mut out, &outcome.describe()),
            None => out.push_str("\"not-requested\""),
        }
        out.push_str(",\"qos_is_advisory_not_pinning\":true,");
        out.push_str("\"host_role\":\"development-and-c1-diagnostic\",");
        out.push_str("\"non_claims\":[");
        out.push_str("\"topology selects an initial experiment only\",");
        out.push_str("\"no core count qualifies a profile\",");
        out.push_str("\"a result on this host does not qualify the Linux C3 target\"]}");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Fixed {
        text: BTreeMap<&'static str, &'static str>,
        number: BTreeMap<&'static str, u64>,
    }

    impl HostSource for Fixed {
        fn text(&self, name: &str) -> Option<String> {
            self.text.get(name).map(|value| value.to_string())
        }

        fn number(&self, name: &str) -> Option<u64> {
            self.number.get(name).copied()
        }
    }

    /// A host with `performance` performance and four efficiency CPUs.
    fn apple(performance: u64) -> Fixed {
        let mut host = Fixed::default();
        host.text.insert("hw.model", "Mac16,7");
        host.text.insert("machdep.cpu.brand_string", "Apple M4 Pro");
        host.text.insert("kern.osrelease", "24.6.0");
        host.text.insert("hw.perflevel0.name", "Performance");
        host.text.insert("hw.perflevel1.name", "Efficiency");
        host.number.insert("hw.physicalcpu", performance + 4);
        host.number.insert("hw.logicalcpu", performance + 4);
        host.number.insert("hw.cachelinesize", 128);
        host.number.insert("hw.pagesize", 16_384);
        host.number.insert("hw.memsize", 48 << 30);
        host.number.insert("hw.nperflevels", 2);
        host.number.insert("hw.perflevel0.physicalcpu", performance);
        host.number.insert("hw.perflevel0.logicalcpu", performance);
        host.number.insert("hw.perflevel0.l1dcachesize", 131_072);
        host.number.insert("hw.perflevel0.l1icachesize", 196_608);
        host.number.insert("hw.perflevel0.l2cachesize", 16_777_216);
        host.number.insert("hw.perflevel0.cpusperl2", 5);
        host.number.insert("hw.perflevel1.physicalcpu", 4);
        host.number.insert("hw.perflevel1.logicalcpu", 4);
        host
    }

    const C1_BUILD: ProfileRequest = ProfileRequest {
        configured: None,
        parallel_builders_available: false,
    };

    #[test]
    fn apple_performance_levels_and_caches_are_recorded() {
        let topology = HostTopology::capture_with(&apple(10));
        assert_eq!(topology.cache_line_bytes, Probe::Value(128));
        assert_eq!(topology.cpu_brand.value().unwrap(), "Apple M4 Pro");
        let levels = topology.perf_levels.value().unwrap();
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0].name.value().unwrap(), "Performance");
        assert_eq!(levels[0].physical_cpus, Probe::Value(10));
        assert_eq!(levels[0].l2_cache_bytes, Probe::Value(16_777_216));
        assert_eq!(levels[0].cpus_per_l2, Probe::Value(5));
        // A value the host does not report is unavailable, never zero.
        assert_eq!(levels[1].l2_cache_bytes, Probe::Unavailable(NO_SYSCTL));
        assert_eq!(topology.performance_cpus(), Some(10));
    }

    #[test]
    fn the_report_is_reproducible_for_the_same_host() {
        let first = HostTopology::capture_with(&apple(10));
        let second = HostTopology::capture_with(&apple(10));
        assert_eq!(first.to_json(), second.to_json());
        assert_eq!(first.fingerprint(), second.fingerprint());
        let other = HostTopology::capture_with(&apple(12));
        assert_ne!(first.fingerprint(), other.fingerprint());
        let json = first.to_json();
        assert!(json.starts_with("{\"os\":"));
        assert!(json.contains("\"cache_line_bytes\":128,"));
        assert!(json.contains("\"name\":\"Performance\",\"physical_cpus\":10,"));
        assert!(json.contains("\"l2_cache_bytes\":{\"unavailable\":"));
        assert!(json.ends_with("}]}"));
        assert_eq!(json.matches('{').count(), json.matches('}').count());
    }

    #[test]
    fn a_host_without_sysctl_degrades_explicitly() {
        let topology = HostTopology::capture_with(&Fixed::default());
        assert_eq!(topology.model, Probe::Unavailable(NO_SYSCTL));
        assert_eq!(topology.physical_cpus, Probe::Unavailable(NO_SYSCTL));
        assert_eq!(topology.perf_levels, Probe::Unavailable(NO_PERF_LEVELS));
        assert_eq!(topology.performance_cpus(), None);
        let hint = initial_profile_hint(&topology);
        assert_eq!(hint.profile, SchedulingProfile::S);
        assert_eq!(hint.performance_cpus, None);
        assert!(
            topology
                .to_json()
                .contains("\"perf_levels\":{\"unavailable\":")
        );

        let mut implausible = Fixed::default();
        implausible.number.insert("hw.nperflevels", 4_000);
        let topology = HostTopology::capture_with(&implausible);
        let expected = Probe::Unavailable(TOO_MANY_PERF_LEVELS);
        assert_eq!(topology.perf_levels, expected);
    }

    #[test]
    fn json_strings_are_escaped() {
        let mut host = Fixed::default();
        host.text.insert("hw.model", "a\"b\\c\nd");
        let json = HostTopology::capture_with(&host).to_json();
        assert!(json.contains("\"model\":\"a\\\"b\\\\c\\u000ad\","));
    }

    #[test]
    fn the_hint_follows_topology_and_qualifies_nothing() {
        let small = initial_profile_hint(&HostTopology::capture_with(&apple(3)));
        let large = initial_profile_hint(&HostTopology::capture_with(&apple(10)));
        assert_eq!(small.profile, SchedulingProfile::S);
        assert_eq!(large.profile, SchedulingProfile::P);
        assert_eq!(small.performance_cpus, Some(3));
        assert_eq!(large.performance_cpus, Some(10));
        // The hint never overrides a configured profile, on either host.
        for hint in [small, large] {
            let request = ProfileRequest {
                configured: Some(SchedulingProfile::S),
                ..C1_BUILD
            };
            let selection = select_profile(request, hint).unwrap();
            assert_eq!(selection.profile, SchedulingProfile::S);
            assert_eq!(selection.source, ProfileSource::Configured);
        }
    }

    #[test]
    fn profile_p_is_refused_not_downgraded_without_parallel_builders() {
        let hint = initial_profile_hint(&HostTopology::capture_with(&apple(10)));
        let configured_p = ProfileRequest {
            configured: Some(SchedulingProfile::P),
            ..C1_BUILD
        };
        assert_eq!(
            select_profile(configured_p, hint),
            Err(ProfileError::ParallelBuildersUnavailable)
        );
        // Without a configuration the P hint cannot run in C1: S, labelled.
        let selection = select_profile(C1_BUILD, hint).unwrap();
        assert_eq!(selection.profile, SchedulingProfile::S);
        assert_eq!(selection.source, ProfileSource::HintNotRunnable);
        // A build with parallel builders honours both.
        let rev4 = ProfileRequest {
            configured: Some(SchedulingProfile::P),
            parallel_builders_available: true,
        };
        assert_eq!(
            select_profile(rev4, hint).unwrap().source,
            ProfileSource::Configured
        );
        let follow = ProfileRequest {
            configured: None,
            parallel_builders_available: true,
        };
        let followed = select_profile(follow, hint).unwrap();
        assert_eq!(followed.profile, SchedulingProfile::P);
        assert_eq!(followed.source, ProfileSource::TopologyHint);
    }

    #[test]
    fn qos_class_names_and_raw_values_round_trip() {
        for name in [
            "user-interactive",
            "user-initiated",
            "default",
            "utility",
            "background",
        ] {
            assert_eq!(QosClass::parse(name).unwrap().name(), name);
        }
        assert_eq!(QosClass::parse("realtime"), None);
        assert_eq!(QosClass::UserInteractive.raw(), 0x21);
        assert_eq!(QosClass::Background.raw(), 0x09);
        let failed = QosOutcome::Failed {
            requested: QosClass::Utility,
            errno: 1,
        };
        assert_eq!(failed.describe(), "failed:utility:errno:1");
    }

    #[test]
    fn qos_on_a_permanent_thread_is_applied_or_explicitly_unsupported() {
        let outcome = std::thread::spawn(|| set_current_thread_qos(QosClass::UserInitiated))
            .join()
            .unwrap();
        // macOS accepts the request; what it reports back is evidence, not a
        // test condition. Elsewhere the request is explicitly unsupported.
        let accepted = matches!(
            outcome,
            QosOutcome::Applied(_) | QosOutcome::AppliedButObserved { .. }
        );
        assert_eq!(accepted, cfg!(target_os = "macos"), "{outcome:?}");
        if !accepted {
            assert!(matches!(outcome, QosOutcome::Unsupported(_)));
        }
    }

    #[test]
    fn the_host_report_captures_this_host_without_guessing() {
        let report = HostProfileReport::capture(C1_BUILD, None);
        assert_eq!(report.topology.os, std::env::consts::OS);
        assert_eq!(report.qos, None);
        // C1 has no parallel builders, so whatever the hint, S runs.
        assert_eq!(report.selection.unwrap().profile, SchedulingProfile::S);
        let json = report.to_json();
        assert!(json.starts_with("{\"schema\":\"darkbot.c1.host_profile.v1\","));
        assert!(json.contains("\"hint_only\":true"));
        assert!(json.contains("\"qos\":\"not-requested\""));
        assert!(json.contains("does not qualify the Linux C3 target"));
        assert_eq!(json.matches('{').count(), json.matches('}').count());
        // Captured twice, the topology identity is the same.
        let again = HostProfileReport::capture(C1_BUILD, None);
        assert_eq!(report.topology.fingerprint(), again.topology.fingerprint());
    }
}

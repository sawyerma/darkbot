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
//!
//! The evidence is a typed binary record (`darkbot.c1.host-profile.v1`,
//! [`HostProfileReport::to_record`]); JSON is never an output. The topology
//! identity is the CRC32C of a fixed canonical byte string, kept unchanged
//! so that every recorded fingerprint stays valid.

use std::fmt::Write as _;

use darkbot_bsc_record::schema::c1_qualification::host_profile as record;
use darkbot_bsc_record::{Draft, Value};

use crate::protocol_v2::crc32c;

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

    /// The fingerprint preimage: one canonical byte string of the topology
    /// with a fixed key order, no whitespace variation and no timestamp, so
    /// the same host yields the same bytes. Its layout is frozen (it is the
    /// JSON-shaped string the C1.6 fingerprints of 2026-10 were taken over);
    /// it identifies the topology and is never written as evidence.
    pub(crate) fn fingerprint_preimage(&self) -> String {
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

    /// CRC32C of the fingerprint preimage: the topology identity bound into
    /// measurement evidence.
    pub fn fingerprint(&self) -> u32 {
        crc32c(self.fingerprint_preimage().as_bytes())
    }

    /// The topology as the nested value of a host-profile record: each
    /// probe is its value or the reason it is unavailable.
    pub fn to_record_value(&self) -> Value {
        use record::topology as t;
        let mut draft = Draft::new(&t::SCHEMA);
        draft.set(t::OS, self.os).set(t::ARCH, self.arch);
        text_probe(&mut draft, t::MODEL, t::MODEL_UNAVAILABLE, &self.model);
        text_probe(
            &mut draft,
            t::CPU_BRAND,
            t::CPU_BRAND_UNAVAILABLE,
            &self.cpu_brand,
        );
        text_probe(
            &mut draft,
            t::OS_RELEASE,
            t::OS_RELEASE_UNAVAILABLE,
            &self.os_release,
        );
        let numbers = [
            (
                t::PHYSICAL_CPUS,
                t::PHYSICAL_CPUS_UNAVAILABLE,
                &self.physical_cpus,
            ),
            (
                t::LOGICAL_CPUS,
                t::LOGICAL_CPUS_UNAVAILABLE,
                &self.logical_cpus,
            ),
            (
                t::CACHE_LINE_BYTES,
                t::CACHE_LINE_BYTES_UNAVAILABLE,
                &self.cache_line_bytes,
            ),
            (t::PAGE_BYTES, t::PAGE_BYTES_UNAVAILABLE, &self.page_bytes),
            (
                t::MEMORY_BYTES,
                t::MEMORY_BYTES_UNAVAILABLE,
                &self.memory_bytes,
            ),
        ];
        for (tag, unavailable, probe) in numbers {
            number_probe(&mut draft, tag, unavailable, probe);
        }
        match &self.perf_levels {
            Probe::Unavailable(reason) => {
                draft.set(t::PERF_LEVELS_UNAVAILABLE, *reason);
            }
            Probe::Value(levels) => {
                for level in levels {
                    draft.push(t::PERF_LEVELS, level.to_record_value());
                }
            }
        }
        draft.into_nested()
    }
}

impl PerfLevel {
    fn to_record_value(&self) -> Value {
        use record::perf_level as l;
        let mut draft = Draft::new(&l::SCHEMA);
        draft.set(l::INDEX, u64::from(self.index));
        text_probe(&mut draft, l::NAME, l::NAME_UNAVAILABLE, &self.name);
        let numbers = [
            (
                l::PHYSICAL_CPUS,
                l::PHYSICAL_CPUS_UNAVAILABLE,
                &self.physical_cpus,
            ),
            (
                l::LOGICAL_CPUS,
                l::LOGICAL_CPUS_UNAVAILABLE,
                &self.logical_cpus,
            ),
            (
                l::L1D_CACHE_BYTES,
                l::L1D_CACHE_BYTES_UNAVAILABLE,
                &self.l1d_cache_bytes,
            ),
            (
                l::L1I_CACHE_BYTES,
                l::L1I_CACHE_BYTES_UNAVAILABLE,
                &self.l1i_cache_bytes,
            ),
            (
                l::L2_CACHE_BYTES,
                l::L2_CACHE_BYTES_UNAVAILABLE,
                &self.l2_cache_bytes,
            ),
            (
                l::CPUS_PER_L2,
                l::CPUS_PER_L2_UNAVAILABLE,
                &self.cpus_per_l2,
            ),
        ];
        for (tag, unavailable, probe) in numbers {
            number_probe(&mut draft, tag, unavailable, probe);
        }
        draft.into_nested()
    }
}

fn text_probe(draft: &mut Draft, tag: u16, unavailable: u16, probe: &Probe<String>) {
    match probe {
        Probe::Value(text) => draft.set(tag, text.as_str()),
        Probe::Unavailable(reason) => draft.set(unavailable, *reason),
    };
}

fn number_probe(draft: &mut Draft, tag: u16, unavailable: u16, probe: &Probe<u64>) {
    match probe {
        Probe::Value(number) => draft.set(tag, *number),
        Probe::Unavailable(reason) => draft.set(unavailable, *reason),
    };
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

impl ProfileSource {
    pub fn name(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::TopologyHint => "topology-hint",
            Self::HintNotRunnable => "hint-not-runnable-in-this-build",
        }
    }
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

impl ProfileError {
    pub fn name(self) -> &'static str {
        match self {
            Self::ParallelBuildersUnavailable => "parallel-builders-unavailable",
        }
    }
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

    /// The report as a `darkbot.c1.host-profile.v1` record. It carries no
    /// timestamp: the same host and request yield the same bytes.
    pub fn to_record(&self) -> Draft {
        let mut draft = Draft::new(&record::SCHEMA);
        draft
            .set(record::TOPOLOGY, self.topology.to_record_value())
            .set(
                record::TOPOLOGY_FINGERPRINT_CRC32C,
                u64::from(self.topology.fingerprint()),
            )
            .set(record::HINT_PROFILE, self.hint.profile.name())
            .set(record::HINT_BASIS, self.hint.basis)
            .set(record::HINT_ONLY, true);
        match self.selection {
            Ok(selection) => {
                draft
                    .set(record::SELECTION_PROFILE, selection.profile.name())
                    .set(record::SELECTION_SOURCE, selection.source.name());
            }
            Err(error) => {
                draft.set(record::SELECTION_ERROR, error.name());
            }
        }
        let qos = self
            .qos
            .map_or_else(|| "not-requested".to_string(), QosOutcome::describe);
        draft
            .set(record::QOS, qos)
            .set(record::QOS_IS_ADVISORY_NOT_PINNING, true)
            .set(record::HOST_ROLE, HOST_ROLE);
        for claim in NON_CLAIMS {
            draft.push(record::NON_CLAIMS, claim);
        }
        draft
    }
}

/// The role of the host this module profiles.
pub const HOST_ROLE: &str = "development-and-c1-diagnostic";

/// What a host profile never claims.
pub const NON_CLAIMS: [&str; 3] = [
    "topology selects an initial experiment only",
    "no core count qualifies a profile",
    "a result on this host does not qualify the Linux C3 target",
];

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
        assert_eq!(first.fingerprint_preimage(), second.fingerprint_preimage());
        assert_eq!(first.fingerprint(), second.fingerprint());
        let other = HostTopology::capture_with(&apple(12));
        assert_ne!(first.fingerprint(), other.fingerprint());
        // The preimage layout is frozen: it defines every recorded fingerprint.
        let preimage = first.fingerprint_preimage();
        assert!(preimage.starts_with("{\"os\":"));
        assert!(preimage.contains("\"cache_line_bytes\":128,"));
        assert!(preimage.contains("\"name\":\"Performance\",\"physical_cpus\":10,"));
        assert!(preimage.contains("\"l2_cache_bytes\":{\"unavailable\":"));
        assert!(preimage.ends_with("}]}"));
        assert_eq!(preimage.matches('{').count(), preimage.matches('}').count());
    }

    /// The host of the C1.9 run of 2026-10-10, exactly as recorded in
    /// `bsc/native/evidence/c1/c1-9-native-latency-20261010T230358-host_profile.json`
    /// (Apple M4 Max, 12 performance and 4 efficiency CPUs).
    fn recorded_m4_max() -> HostTopology {
        let mut host = Fixed::default();
        host.text.insert("hw.model", "Mac16,6");
        host.text.insert("machdep.cpu.brand_string", "Apple M4 Max");
        host.text.insert("kern.osrelease", "24.6.0");
        host.text.insert("hw.perflevel0.name", "Performance");
        host.text.insert("hw.perflevel1.name", "Efficiency");
        let numbers = [
            ("hw.physicalcpu", 16),
            ("hw.logicalcpu", 16),
            ("hw.cachelinesize", 128),
            ("hw.pagesize", 16_384),
            ("hw.memsize", 137_438_953_472),
            ("hw.nperflevels", 2),
            ("hw.perflevel0.physicalcpu", 12),
            ("hw.perflevel0.logicalcpu", 12),
            ("hw.perflevel0.l1dcachesize", 131_072),
            ("hw.perflevel0.l1icachesize", 196_608),
            ("hw.perflevel0.l2cachesize", 16_777_216),
            ("hw.perflevel0.cpusperl2", 6),
            ("hw.perflevel1.physicalcpu", 4),
            ("hw.perflevel1.logicalcpu", 4),
            ("hw.perflevel1.l1dcachesize", 65_536),
            ("hw.perflevel1.l1icachesize", 131_072),
            ("hw.perflevel1.l2cachesize", 4_194_304),
            ("hw.perflevel1.cpusperl2", 4),
        ];
        for (name, value) in numbers {
            host.number.insert(name, value);
        }
        let mut topology = HostTopology::capture_with(&host);
        // The recorded host's OS and architecture, whatever host runs the test.
        topology.os = "macos";
        topology.arch = "aarch64";
        topology
    }

    /// Golden: the binary migration keeps the topology identity. The same
    /// topology yields the `topology_fingerprint_crc32c` recorded in
    /// `c1-9-native-latency-20261010T230358-host_profile.json`.
    #[test]
    fn the_recorded_c19_host_keeps_its_fingerprint_0xfc5501a8() {
        let topology = recorded_m4_max();
        assert_eq!(topology.fingerprint(), 0xfc55_01a8);
        let preimage = topology.fingerprint_preimage();
        let expected = concat!(
            "{\"os\":\"macos\",\"arch\":\"aarch64\",\"model\":\"Mac16,6\",",
            "\"cpu_brand\":\"Apple M4 Max\",\"os_release\":\"24.6.0\",",
            "\"physical_cpus\":16,\"logical_cpus\":16,\"cache_line_bytes\":128,",
            "\"page_bytes\":16384,\"memory_bytes\":137438953472,\"perf_levels\":[",
            "{\"index\":0,\"name\":\"Performance\",\"physical_cpus\":12,\"logical_cpus\":12,",
            "\"l1d_cache_bytes\":131072,\"l1i_cache_bytes\":196608,",
            "\"l2_cache_bytes\":16777216,\"cpus_per_l2\":6},",
            "{\"index\":1,\"name\":\"Efficiency\",\"physical_cpus\":4,\"logical_cpus\":4,",
            "\"l1d_cache_bytes\":65536,\"l1i_cache_bytes\":131072,",
            "\"l2_cache_bytes\":4194304,\"cpus_per_l2\":4}]}"
        );
        assert_eq!(preimage, expected);
    }

    #[test]
    fn the_host_profile_record_is_typed_reproducible_and_keeps_the_id() {
        use darkbot_bsc_record::decode;
        let topology = recorded_m4_max();
        let hint = initial_profile_hint(&topology);
        let configured_s = ProfileRequest {
            configured: Some(SchedulingProfile::S),
            ..C1_BUILD
        };
        let report = HostProfileReport {
            selection: select_profile(configured_s, hint),
            hint,
            qos: None,
            topology,
        };
        let bytes = report.to_record().encode().unwrap();
        assert_eq!(bytes, report.to_record().encode().unwrap());
        assert_eq!(&bytes[..8], b"DBREC001");
        let decoded = decode(&record::SCHEMA, &bytes).unwrap();
        assert_eq!(
            decoded.u64(record::TOPOLOGY_FINGERPRINT_CRC32C).unwrap(),
            0xfc55_01a8
        );
        assert_eq!(decoded.text(record::HINT_PROFILE).unwrap(), "P");
        assert!(decoded.bool(record::HINT_ONLY).unwrap());
        assert_eq!(decoded.text(record::SELECTION_PROFILE).unwrap(), "S");
        assert_eq!(
            decoded.text(record::SELECTION_SOURCE).unwrap(),
            "configured"
        );
        assert_eq!(decoded.opt_text(record::SELECTION_ERROR).unwrap(), None);
        assert_eq!(decoded.text(record::QOS).unwrap(), "not-requested");
        assert_eq!(decoded.texts(record::NON_CLAIMS).unwrap(), NON_CLAIMS);
        let topology = decoded.nested(record::TOPOLOGY).unwrap();
        assert_eq!(
            topology.text(record::topology::CPU_BRAND).unwrap(),
            "Apple M4 Max"
        );
        let levels = topology.nested_all(record::topology::PERF_LEVELS).unwrap();
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[1].u64(record::perf_level::CPUS_PER_L2).unwrap(), 4);

        // An unavailable value is its reason, never a zero; a refusal is recorded.
        let bare = HostTopology::capture_with(&Fixed::default());
        let refused = HostProfileReport {
            hint: initial_profile_hint(&bare),
            selection: Err(ProfileError::ParallelBuildersUnavailable),
            qos: Some(QosOutcome::Unsupported("test host")),
            topology: bare,
        };
        let decoded = decode(&record::SCHEMA, &refused.to_record().encode().unwrap()).unwrap();
        assert_eq!(
            decoded.text(record::SELECTION_ERROR).unwrap(),
            "parallel-builders-unavailable"
        );
        assert_eq!(decoded.opt_text(record::SELECTION_PROFILE).unwrap(), None);
        assert_eq!(decoded.text(record::QOS).unwrap(), "unsupported:test host");
        let topology = decoded.nested(record::TOPOLOGY).unwrap();
        assert_eq!(
            topology.opt_u64(record::topology::PHYSICAL_CPUS).unwrap(),
            None
        );
        assert_eq!(
            topology
                .text(record::topology::PHYSICAL_CPUS_UNAVAILABLE)
                .unwrap(),
            NO_SYSCTL
        );
        assert_eq!(
            topology
                .text(record::topology::PERF_LEVELS_UNAVAILABLE)
                .unwrap(),
            NO_PERF_LEVELS
        );
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
                .fingerprint_preimage()
                .contains("\"perf_levels\":{\"unavailable\":")
        );

        let mut implausible = Fixed::default();
        implausible.number.insert("hw.nperflevels", 4_000);
        let topology = HostTopology::capture_with(&implausible);
        let expected = Probe::Unavailable(TOO_MANY_PERF_LEVELS);
        assert_eq!(topology.perf_levels, expected);
    }

    #[test]
    fn preimage_strings_are_escaped() {
        let mut host = Fixed::default();
        host.text.insert("hw.model", "a\"b\\c\nd");
        let preimage = HostTopology::capture_with(&host).fingerprint_preimage();
        assert!(preimage.contains("\"model\":\"a\\\"b\\\\c\\u000ad\","));
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
        let bytes = report.to_record().encode().unwrap();
        let decoded = darkbot_bsc_record::decode(&record::SCHEMA, &bytes).unwrap();
        assert!(decoded.bool(record::HINT_ONLY).unwrap());
        assert_eq!(decoded.text(record::QOS).unwrap(), "not-requested");
        let claims = decoded.texts(record::NON_CLAIMS).unwrap();
        assert!(claims.contains(&"a result on this host does not qualify the Linux C3 target"));
        // Captured twice, the topology identity is the same.
        let again = HostProfileReport::capture(C1_BUILD, None);
        assert_eq!(report.topology.fingerprint(), again.topology.fingerprint());
    }
}

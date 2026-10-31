//! P6-MT.12 (Apple-M4 runtime): the host a run executes on, read at its start — the CPU
//! topology without a fixed core count (`hw.nperflevels` and each performance level's logical
//! CPUs, CPUs per L2 and L2 size, `hw.logicalcpu`, `hw.activecpu`, `hw.cachelinesize`), whether the
//! process runs natively on arm64 or translated (Rosetta: `sysctl.proc_translated`), the macOS
//! version and model, and power and thermal state (the system's low-power-mode and
//! thermal-pressure notification states) — and the QoS class of each thread role (PHASENPLAN:
//! Owner/Ingress/FAST/Remote USER_INITIATED, Evidence UTILITY; no claimed P-/E-core affinity; no
//! FAST pool exists yet — route, fee and simulation run on the owner).
//! A qualifying M4 run is native arm64 macOS, never under Rosetta (`HostEvidence::qualifying`).
//! The sources are the kernel's sysctls and libnotify's states (libSystem, linked by every
//! process): no new dependency, nothing on the network. On another OS a source is absent
//! (`None`), never guessed.
use serde::{Deserialize, Serialize};

/// One performance level (`hw.perflevelN.*`), in the kernel's order (0: the fastest).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerfLevel {
    pub name: Option<String>,
    pub logical_cpus: Option<u32>,
    pub physical_cpus: Option<u32>,
    pub cpus_per_l2: Option<u32>,
    pub l2_cache_bytes: Option<u64>,
}

/// P6-MT.12 (kind `HOST_EVIDENCE`): the host of a run at its start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostEvidence {
    /// The architecture this binary was built for (`std::env::consts::ARCH`).
    pub build_arch: String,
    /// `hw.optional.arm64`: the machine is an arm64 Mac.
    pub machine_arm64: Option<bool>,
    /// `sysctl.proc_translated`: this process runs under Rosetta.
    pub translated: Option<bool>,
    pub os: String,
    pub os_version: Option<String>,
    pub model: Option<String>,
    pub perf_levels: Vec<PerfLevel>,
    pub logical_cpus: Option<u32>,
    pub active_cpus: Option<u32>,
    pub cache_line_bytes: Option<u64>,
    pub low_power_mode: Option<bool>,
    /// The thermal pressure level (0 nominal … 4), `com.apple.system.thermalpressurelevel`.
    pub thermal_pressure: Option<u64>,
    /// Threads whose QoS class the kernel refused so far (`set_thread_qos`).
    pub qos_refusals: u64,
}

impl crate::analytics::payload::Payload for HostEvidence {
    const KIND: u16 = crate::analytics::journal::kind::HOST_EVIDENCE;
}

/// A run on a host that is not a native arm64 Mac qualifies nothing for M4.
pub const NOT_M4: &str = "M4_HOST_NOT_NATIVE: a qualifying M4 run is native arm64 macOS";
/// A run under Rosetta qualifies nothing (PHASENPLAN P6-MT.12).
pub const ROSETTA: &str = "M4_HOST_ROSETTA: a qualifying M4 run is never translated (Rosetta)";

impl HostEvidence {
    /// Whether a run on this host can qualify for M4: native arm64 macOS, not translated; `Err`:
    /// why not (an unknown source is never a pass).
    pub fn qualifying(&self) -> Result<(), &'static str> {
        if self.translated != Some(false) {
            return Err(if self.translated == Some(true) {
                ROSETTA
            } else {
                NOT_M4
            });
        }
        if self.os != "macos" || self.build_arch != "aarch64" || self.machine_arm64 != Some(true) {
            return Err(NOT_M4);
        }
        Ok(())
    }

    /// One line for the owner's log and status.
    pub fn line(&self) -> String {
        let levels: Vec<String> = self
            .perf_levels
            .iter()
            .map(|l| {
                format!(
                    "{}={}cpu/l2 {}×{}B",
                    l.name.as_deref().unwrap_or("?"),
                    shown(l.logical_cpus),
                    shown(l.cpus_per_l2),
                    shown(l.l2_cache_bytes)
                )
            })
            .collect();
        format!(
            "host {} {} {} · build {} · translated {} · levels [{}] · logical {} active {} · line {}B · low power {} · thermal {} · qos refused {} · {}",
            self.os,
            self.os_version.as_deref().unwrap_or("?"),
            self.model.as_deref().unwrap_or("?"),
            self.build_arch,
            shown(self.translated),
            levels.join(", "),
            shown(self.logical_cpus),
            shown(self.active_cpus),
            shown(self.cache_line_bytes),
            shown(self.low_power_mode),
            shown(self.thermal_pressure),
            self.qos_refusals,
            match self.qualifying() {
                Ok(()) => "M4_QUALIFYING_HOST",
                Err(why) => why,
            }
        )
    }
}

fn shown<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "?".into(), |v| v.to_string())
}

/// The host now.
pub fn read() -> HostEvidence {
    let levels = sys::u32("hw.nperflevels").unwrap_or(0);
    HostEvidence {
        build_arch: std::env::consts::ARCH.into(),
        machine_arm64: sys::u32("hw.optional.arm64").map(|v| v == 1),
        translated: sys::translated(),
        os: std::env::consts::OS.into(),
        os_version: sys::string("kern.osproductversion"),
        model: sys::string("hw.model"),
        perf_levels: (0..levels)
            .map(|n| PerfLevel {
                name: sys::string(&format!("hw.perflevel{n}.name")),
                logical_cpus: sys::u32(&format!("hw.perflevel{n}.logicalcpu")),
                physical_cpus: sys::u32(&format!("hw.perflevel{n}.physicalcpu")),
                cpus_per_l2: sys::u32(&format!("hw.perflevel{n}.cpusperl2")),
                l2_cache_bytes: sys::u64(&format!("hw.perflevel{n}.l2cachesize")),
            })
            .collect(),
        logical_cpus: sys::u32("hw.logicalcpu"),
        active_cpus: active_cpus(),
        cache_line_bytes: sys::u64("hw.cachelinesize"),
        low_power_mode: low_power_mode(),
        thermal_pressure: thermal_pressure(),
        qos_refusals: qos_refusals(),
    }
}

/// `hw.activecpu` now (P6-MT.13 `active_cpu`).
pub fn active_cpus() -> Option<u32> {
    sys::u32("hw.activecpu")
}

/// Whether low-power mode is on now.
pub fn low_power_mode() -> Option<bool> {
    sys::notify_state("com.apple.system.lowpowermode").map(|s| s != 0)
}

/// The thermal pressure level now (P6-MT.13 `thermal_state`).
pub fn thermal_pressure() -> Option<u64> {
    sys::notify_state("com.apple.system.thermalpressurelevel")
}

/// The role of a thread for its QoS class (PHASENPLAN P6-MT.12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadRole {
    Owner,
    Ingress,
    Remote,
    Evidence,
}

/// Threads whose QoS class the kernel refused (each keeps its default), in the host record.
static QOS_REFUSED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Sets the calling thread's QoS class for `role` (macOS; elsewhere nothing to set); a refusal
/// is counted (`qos_refusals`), the thread keeps its default.
pub fn set_thread_qos(role: ThreadRole) {
    if sys::set_qos(role).is_err() {
        QOS_REFUSED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// The QoS refusals of this process so far.
pub fn qos_refusals() -> u64 {
    QOS_REFUSED.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(target_os = "macos")]
mod sys {
    use std::ffi::{c_char, c_int, CString};

    fn raw(name: &str, out: *mut libc::c_void, len: &mut usize) -> bool {
        let Ok(name) = CString::new(name) else {
            return false;
        };
        // SAFETY: the out buffer of `len` bytes is the caller's; the name is NUL-terminated.
        unsafe { libc::sysctlbyname(name.as_ptr(), out, len, std::ptr::null_mut(), 0) == 0 }
    }

    pub(super) fn u32(name: &str) -> Option<u32> {
        let mut v = 0u32;
        let mut len = std::mem::size_of::<u32>();
        (raw(name, &mut v as *mut u32 as *mut libc::c_void, &mut len)
            && len == std::mem::size_of::<u32>())
        .then_some(v)
    }

    pub(super) fn u64(name: &str) -> Option<u64> {
        let mut v = 0u64;
        let mut len = std::mem::size_of::<u64>();
        // A kernel value of 32 bits is widened; any other length is no value.
        if !raw(name, &mut v as *mut u64 as *mut libc::c_void, &mut len) {
            return None;
        }
        match len {
            8 => Some(v),
            4 => Some(u64::from(v as u32)),
            _ => None,
        }
    }

    pub(super) fn string(name: &str) -> Option<String> {
        let mut len = 0usize;
        if !raw(name, std::ptr::null_mut(), &mut len) || len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len];
        if !raw(name, buf.as_mut_ptr() as *mut libc::c_void, &mut len) {
            return None;
        }
        buf.truncate(len);
        while buf.last() == Some(&0) {
            buf.pop();
        }
        String::from_utf8(buf).ok()
    }

    /// `sysctl.proc_translated`: absent on a Mac without Rosetta's interface (an Intel Mac),
    /// where nothing is translated.
    pub(super) fn translated() -> Option<bool> {
        match u32("sysctl.proc_translated") {
            Some(v) => Some(v == 1),
            None if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) => {
                Some(false)
            }
            None => None,
        }
    }

    // libnotify (libSystem): the state of a system notification name.
    extern "C" {
        fn notify_register_check(name: *const c_char, out_token: *mut c_int) -> u32;
        fn notify_get_state(token: c_int, state64: *mut u64) -> u32;
        fn notify_cancel(token: c_int) -> u32;
    }
    const NOTIFY_STATUS_OK: u32 = 0;

    pub(super) fn notify_state(name: &str) -> Option<u64> {
        let name = CString::new(name).ok()?;
        let mut token: c_int = 0;
        // SAFETY: a NUL-terminated name and an out token we own; the token is cancelled below.
        if unsafe { notify_register_check(name.as_ptr(), &mut token) } != NOTIFY_STATUS_OK {
            return None;
        }
        let mut state = 0u64;
        // SAFETY: a registered token and an out value we own.
        let ok = unsafe { notify_get_state(token, &mut state) } == NOTIFY_STATUS_OK;
        // SAFETY: the token registered above.
        unsafe { notify_cancel(token) };
        ok.then_some(state)
    }

    pub(super) fn set_qos(role: super::ThreadRole) -> Result<(), i32> {
        use super::ThreadRole as R;
        let class = match role {
            R::Owner | R::Ingress | R::Remote => libc::qos_class_t::QOS_CLASS_USER_INITIATED,
            R::Evidence => libc::qos_class_t::QOS_CLASS_UTILITY,
        };
        // SAFETY: sets the calling thread's own QoS class.
        match unsafe { libc::pthread_set_qos_class_self_np(class, 0) } {
            0 => Ok(()),
            rc => Err(rc),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod sys {
    pub(super) fn u32(_name: &str) -> Option<u32> {
        None
    }
    pub(super) fn u64(_name: &str) -> Option<u64> {
        None
    }
    pub(super) fn string(_name: &str) -> Option<String> {
        None
    }
    pub(super) fn translated() -> Option<bool> {
        None
    }
    pub(super) fn notify_state(_name: &str) -> Option<u64> {
        None
    }
    pub(super) fn set_qos(_role: super::ThreadRole) -> Result<(), i32> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native() -> HostEvidence {
        HostEvidence {
            build_arch: "aarch64".into(),
            machine_arm64: Some(true),
            translated: Some(false),
            os: "macos".into(),
            os_version: Some("15.6.1".into()),
            model: Some("Mac16,6".into()),
            perf_levels: Vec::new(),
            logical_cpus: Some(16),
            active_cpus: Some(16),
            cache_line_bytes: Some(128),
            low_power_mode: Some(false),
            thermal_pressure: Some(0),
            qos_refusals: 0,
        }
    }

    /// Only native arm64 macOS qualifies: Rosetta never, and an unknown source is never a pass.
    #[test]
    fn only_a_native_arm64_mac_is_a_qualifying_host() {
        assert_eq!(native().qualifying(), Ok(()));
        let with = |f: fn(&mut HostEvidence)| {
            let mut h = native();
            f(&mut h);
            h.qualifying()
        };
        assert_eq!(with(|h| h.translated = Some(true)), Err(ROSETTA));
        assert_eq!(with(|h| h.translated = None), Err(NOT_M4));
        assert_eq!(with(|h| h.os = "linux".into()), Err(NOT_M4));
        assert_eq!(with(|h| h.build_arch = "x86_64".into()), Err(NOT_M4));
        assert_eq!(with(|h| h.machine_arm64 = None), Err(NOT_M4));
        assert!(native().line().ends_with("M4_QUALIFYING_HOST"));
    }

    /// On this Mac every source is read: the levels' logical CPUs sum to `hw.logicalcpu`, the
    /// test binary runs natively, power and thermal states are known.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_host_is_read_from_the_kernel() {
        let h = read();
        assert!(!h.perf_levels.is_empty(), "{h:?}");
        let logical: u32 = h.perf_levels.iter().filter_map(|l| l.logical_cpus).sum();
        assert_eq!(Some(logical), h.logical_cpus, "{h:?}");
        assert!(h
            .perf_levels
            .iter()
            .all(|l| l.name.is_some() && l.cpus_per_l2.is_some() && l.l2_cache_bytes.is_some()));
        assert!(h.cache_line_bytes.is_some() && h.active_cpus.is_some());
        assert!(h.os_version.is_some() && h.model.is_some());
        assert!(h.low_power_mode.is_some() && h.thermal_pressure.is_some());
        if cfg!(target_arch = "aarch64") {
            assert_eq!((h.translated, h.qualifying()), (Some(false), Ok(())));
        }
    }

    /// A thread's role sets its QoS class (read back from the kernel).
    #[cfg(target_os = "macos")]
    #[test]
    fn a_thread_role_sets_its_qos_class() {
        for (role, class) in [
            (
                ThreadRole::Owner,
                libc::qos_class_t::QOS_CLASS_USER_INITIATED,
            ),
            (ThreadRole::Evidence, libc::qos_class_t::QOS_CLASS_UTILITY),
        ] {
            let refused = qos_refusals();
            let got = std::thread::spawn(move || {
                set_thread_qos(role);
                let mut class = libc::qos_class_t::QOS_CLASS_UNSPECIFIED;
                let mut priority = 0;
                // SAFETY: this thread's own handle and out values it owns.
                unsafe {
                    libc::pthread_get_qos_class_np(libc::pthread_self(), &mut class, &mut priority)
                };
                class
            })
            .join()
            .unwrap();
            assert_eq!(got as u32, class as u32, "{role:?}");
            assert_eq!(qos_refusals(), refused);
        }
    }
}

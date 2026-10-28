//! Integrity boot audit before a run (frontend STARTUP_BACKTEST_OPTIMIZATION BOOT-01…12).
//!
//! Offline scope checks what is provable without network: software/schema identity, journal
//! codec and payload registry, pinned marker/related discriminators, runtime-owner lock, unsealed
//! journals of earlier runs, host memory/disk reserves and clock, the observe-only safety flags,
//! the public wallet identity and the measurement scope. Every check that needs a Mainnet read
//! (cluster genesis, WSS/provider, ProgramData, group/bank/oracle state, dynamic PDAs, oracle
//! freshness) stays NOT_RUN in this scope — never PASS — so an offline audit can never be a
//! network READY. The verdict is bound to a readiness token over the audited identities; a start
//! recomputes the token (TOCTOU) and refuses on any drift.
use super::control;
use crate::analytics::journal;
use crate::analytics::payload;
use crate::market::observer::parity::MARKERS;
use crate::market::observer::related::RelatedKind;
use crate::storage::budget::{free_disk_bytes, physical_memory_bytes, MIB};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckState {
    Pass,
    Warn,
    Blocked,
    Unknown,
    NotRun,
    NotApplicable,
}
impl CheckState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Blocked => "BLOCKED",
            Self::Unknown => "UNKNOWN",
            Self::NotRun => "NOT_RUN",
            Self::NotApplicable => "NOT_APPLICABLE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootCheck {
    pub id: String,
    pub name: String,
    pub critical: bool,
    pub state: CheckState,
    pub expected: Option<String>,
    pub observed: Option<String>,
    pub source: String,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditVerdict {
    /// Every critical check passed, network checks included.
    Ready,
    /// Every offline check passed; the network checks were not run (no network READY).
    OfflineReadyNetworkNotRun,
    Blocked,
}
impl AuditVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::OfflineReadyNetworkNotRun => "OFFLINE_READY_NETWORK_NOT_RUN",
            Self::Blocked => "BLOCKED",
        }
    }
}

/// The identities a readiness token binds; recomputed before a start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditedIdentity {
    pub source_head: Option<String>,
    /// SHA-256 of `git status --porcelain -- project_0` (uncommitted changes are part of it).
    pub source_dirty_sha256: Option<String>,
    pub runtime_config_sha256: String,
    pub journal_version: u16,
    pub discriminator_table_sha256: String,
    pub live_enabled: bool,
    pub submission_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootAudit {
    pub scope: String,
    pub checks: Vec<BootCheck>,
    pub identity: AuditedIdentity,
    pub verdict: AuditVerdict,
    /// SHA-256 over the audited identity; valid only while every identity is unchanged.
    pub readiness_token: String,
}

impl BootAudit {
    pub fn terminal(&self) -> String {
        let mut out = format!(
            "PROJECT 0 BOOT AUDIT scope={} verdict={} token={}\n",
            self.scope,
            self.verdict.as_str(),
            &self.readiness_token[..16]
        );
        for c in &self.checks {
            out.push_str(&format!(
                "  {:<8} {:<26} {:<14} {}\n",
                c.id,
                c.name,
                c.state.as_str(),
                c.reason
            ));
        }
        if self.verdict == AuditVerdict::OfflineReadyNetworkNotRun {
            out.push_str("  network checks NOT_RUN: run `p0ctl doctor --scope network` only with explicit Mainnet read permission\n");
        }
        out
    }
}

fn sha(bytes: &[u8]) -> String {
    journal::hex(&solana_sdk::hash::hash(bytes).to_bytes())
}

/// R4: minimum and worst case of the BOOT reads of the configured scope.
#[cfg(feature = "crate_health")]
fn boot_read_bounds(
    c: &darkbot_solana::runtime_config::RuntimeConfig,
) -> (Result<u64, String>, Result<u64, String>) {
    (
        super::boot_network::required_reads(c),
        super::boot_network::worst_case_reads(c),
    )
}
/// Without the health engine there is no FE scope and no network audit: the budget is unknown,
/// never PASS.
#[cfg(not(feature = "crate_health"))]
fn boot_read_bounds(
    _: &darkbot_solana::runtime_config::RuntimeConfig,
) -> (Result<u64, String>, Result<u64, String>) {
    let e = || Err("BOOT_SCOPE_NEEDS_CRATE_HEALTH".to_string());
    (e(), e())
}

fn git(project: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

/// FE-04: the commit this project's binaries are built from — its HEAD, only while the project
/// tree is clean (`status --porcelain -- .` empty); an approval's `code_sha` binds nothing else.
pub fn clean_source_head(project: &Path) -> Option<String> {
    let dirty = git(project, &["status", "--porcelain", "--", "."])?;
    if !dirty.is_empty() {
        return None;
    }
    git(project, &["rev-parse", "HEAD"]).map(|b| String::from_utf8_lossy(&b).trim().to_string())
}

/// Pinned discriminator table: six markers and four related instructions in fixed order.
fn discriminator_table() -> Vec<u8> {
    let mut t = Vec::new();
    for (d, name) in MARKERS {
        t.extend(d);
        t.extend(name.as_bytes());
    }
    for k in RelatedKind::ALL {
        t.extend(k.discriminator());
        t.extend(k.name().as_bytes());
    }
    t
}

/// Observe-only safety flags as the launcher would pass them (`P0_LIVE_ENABLED`,
/// `P0_SUBMISSION_ENABLED`; anything but unset/empty/false/0 counts as enabled).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SafetyFlags {
    pub live_enabled: bool,
    pub submission_enabled: bool,
}
impl SafetyFlags {
    pub fn from_env() -> Self {
        let on = |name: &str| {
            !matches!(
                std::env::var(name).ok().as_deref(),
                None | Some("") | Some("false") | Some("0")
            )
        };
        Self {
            live_enabled: on("P0_LIVE_ENABLED"),
            submission_enabled: on("P0_SUBMISSION_ENABLED"),
        }
    }
}

/// Identities of this project and process now.
pub fn identity(project: &Path, flags: SafetyFlags) -> Result<AuditedIdentity, String> {
    identity_with(project, flags, darkbot_solana::runtime_config::active())
}

/// Identities of this project with an explicit operational config.
pub fn identity_with(
    project: &Path,
    flags: SafetyFlags,
    config: &darkbot_solana::runtime_config::RuntimeConfig,
) -> Result<AuditedIdentity, String> {
    let config = bincode::serialize(config).map_err(|e| format!("RUNTIME_CONFIG_DIGEST: {e}"))?;
    Ok(AuditedIdentity {
        source_head: git(project, &["rev-parse", "HEAD"])
            .map(|b| String::from_utf8_lossy(&b).trim().to_string()),
        source_dirty_sha256: git(project, &["status", "--porcelain", "--", "."]).map(|b| sha(&b)),
        runtime_config_sha256: sha(&config),
        journal_version: journal::VERSION,
        discriminator_table_sha256: sha(&discriminator_table()),
        live_enabled: flags.live_enabled,
        submission_enabled: flags.submission_enabled,
    })
}

/// The operational config of a `p0ctl run start` of operating mode `mode` (DEC-05: through its
/// profile): its operational arguments (after `--`) plus the run duration as CLI value. One rule
/// for p0ctl and for the runtime it launches (X1), which re-resolves it from its own arguments
/// and environment.
pub fn start_config(
    mode: &str,
    operational: &[String],
    duration: &str,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<darkbot_solana::runtime_config::Resolved, String> {
    let mut args = operational.to_vec();
    args.extend(["--duration".into(), duration.into()]);
    darkbot_solana::runtime_config::Resolved::for_mode(mode, &args, env)
}

/// X1: the identity of a runtime launched by `p0ctl run start` (`args` = its operational
/// arguments, `P0_MEASUREMENT_DURATION` = the launcher's `--duration`, `P0_MODE` its mode),
/// compared with the approved network audit before admission.
pub fn launched_identity(
    project: &Path,
    flags: SafetyFlags,
    args: &[String],
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<AuditedIdentity, String> {
    let duration = env("P0_MEASUREMENT_DURATION")
        .ok_or("BOOT_NETWORK_AUDIT_MISSING: P0_MEASUREMENT_DURATION")?;
    let mode = env("P0_MODE").ok_or("BOOT_NETWORK_AUDIT_MISSING: P0_MODE")?;
    let resolved = start_config(&mode, args, &duration, env)
        .map_err(|e| format!("BOOT_IDENTITY_DRIFT: runtime_config ({e})"))?;
    identity_with(project, flags, &resolved.config)
}

pub fn token(identity: &AuditedIdentity) -> Result<String, String> {
    bincode::serialize(identity)
        .map(|b| sha(&b))
        .map_err(|e| format!("READINESS_TOKEN: {e}"))
}

/// Inputs of the offline audit (paths are explicit so tests never touch the project tree).
pub struct OfflineInputs {
    pub project: PathBuf,
    pub runtime_lock: PathBuf,
    /// Run roots scanned for unsealed journals of earlier runs.
    pub run_roots: Vec<PathBuf>,
    /// E4/E6: root the next run is written to (`Err` = not resolvable, the launcher refuses too).
    pub run_output_root: Result<PathBuf, String>,
    /// X3: identity of the control and runtime executables (`binary_identity`).
    pub binary: Result<String, String>,
    pub wallet_public_env: PathBuf,
    pub max_capture_bytes: u64,
    pub system_reserve_bytes: u64,
    pub disk_reserve_bytes: u64,
    pub flags: SafetyFlags,
}

impl OfflineInputs {
    pub fn for_project(project: &Path) -> Self {
        let w = crate::storage::writer::WriterConfig::from_active();
        Self {
            project: project.into(),
            runtime_lock: std::env::var_os("P0_RUNTIME_LOCK")
                .map(PathBuf::from)
                .unwrap_or_else(|| control::default_lock_path(project)),
            run_roots: ["data/runs", "data/econ4h", "data/market"]
                .iter()
                .map(|r| project.join(r))
                .collect(),
            run_output_root: econ_run_root(project, &|k| std::env::var(k).ok()),
            binary: binary_identity(),
            wallet_public_env: project.join("config/wallet.public.env"),
            max_capture_bytes: w.max_bytes,
            system_reserve_bytes: w.system_reserve_bytes,
            disk_reserve_bytes: w.disk_reserve_bytes,
            flags: SafetyFlags::from_env(),
        }
    }
}

/// E6: root the next econ4h run is written to, resolved like the launcher's pinned
/// `operations/capture/run_store.resolve_store` (not changed here): an explicit
/// `P0_ECON_OUT_DIR` (`~/` expanded, relative to the working directory), else `data/econ4h` for
/// `P0_RUN_STORE=legacy` and `data/runs` otherwise; any other store value is refused.
pub fn econ_run_root(
    project: &Path,
    get: &dyn Fn(&str) -> Option<String>,
) -> Result<PathBuf, String> {
    let store = get("P0_RUN_STORE").filter(|v| !v.is_empty());
    let default = match store.as_deref() {
        None | Some("runs") => project.join("data/runs"),
        Some("legacy") => project.join("data/econ4h"),
        Some(_) => return Err("RUN_STORE_INVALID".into()),
    };
    let Some(explicit) = get("P0_ECON_OUT_DIR").filter(|v| !v.is_empty()) else {
        return Ok(default);
    };
    let explicit = match (explicit.strip_prefix("~/"), get("HOME")) {
        (Some(rest), Some(home)) => Path::new(&home).join(rest),
        _ => PathBuf::from(explicit),
    };
    if explicit.is_absolute() {
        Ok(explicit)
    } else {
        std::env::current_dir()
            .map(|d| d.join(explicit))
            .map_err(|_| "RUN_ROOT_CWD".into())
    }
}

/// E6: the directory whose volume holds `root`: its nearest existing ancestor (a missing run
/// directory is created below it), canonicalized so a symlink names its real target.
fn volume_dir(root: &Path) -> Option<PathBuf> {
    root.ancestors().find(|a| a.exists())?.canonicalize().ok()
}

/// E4: active write probe on the run volume: a uniquely named probe journal is created, written,
/// synced, sealed, read back with the strict reader and removed again. It proves that this
/// volume accepts and syncs a journal now, not that the whole run will fit (capacity preflight).
fn writer_probe(dir: &Path) -> Result<String, String> {
    writer_probe_with(dir, &|_| {})
}

fn writer_probe_with(dir: &Path, fault: &dyn Fn(&mut journal::Writer)) -> Result<String, String> {
    let started = std::time::Instant::now();
    let unique = darkbot_common::timing::wall_now()
        .map_err(String::from)?
        .as_nanos();
    let path = dir.join(format!(
        ".p0-boot-probe-{}-{unique}.p0j",
        std::process::id()
    ));
    let lines = ["BOOT_WRITER_PROBE frame=0", "BOOT_WRITER_PROBE frame=1"];
    let io = |step: &str, e: std::io::Error| format!("WRITER_PROBE_{step} {:?}", e.kind());
    let result = (|| -> Result<(), String> {
        let mut w = journal::Writer::create(&path).map_err(|e| io("CREATE", e))?;
        fault(&mut w);
        for (i, line) in lines.iter().enumerate() {
            let p = journal::Provenance {
                source_seq: i as u64,
                ..Default::default()
            };
            w.append_typed(
                p,
                &payload::OwnerDiagnostic {
                    line: (*line).into(),
                },
            )
            .map_err(|e| io("WRITE", e))?;
        }
        w.sync().map_err(|e| io("SYNC", e))?;
        w.seal(true).map_err(|e| io("SEAL", e))?;
        drop(w);
        let mut r = journal::Reader::open(&path).map_err(|e| io("READBACK", e))?;
        let mut read = Vec::new();
        while let Some(f) = r.next_frame().map_err(|e| io("READBACK", e))? {
            if f.kind != journal::MANIFEST {
                let d: payload::OwnerDiagnostic = f.typed().map_err(|e| io("READBACK", e))?;
                read.push(d.line);
            }
        }
        if r.sealed && r.complete && read == lines {
            Ok(())
        } else {
            Err("WRITER_PROBE_READBACK_MISMATCH".into())
        }
    })();
    let removed = !path.exists()
        || (std::fs::remove_file(&path).is_ok()
            && std::fs::File::open(dir).and_then(|d| d.sync_all()).is_ok());
    let leftover = if removed {
        String::new()
    } else {
        format!("; probe file left: {}", path.display())
    };
    match result {
        Ok(()) if removed => Ok(format!(
            "create/write/sync/seal/readback/remove in {} ms on {}",
            started.elapsed().as_millis(),
            dir.display()
        )),
        Ok(()) => Err(format!("WRITER_PROBE_REMOVE{leftover}")),
        Err(e) => Err(format!("{e}{leftover}")),
    }
}

/// E5: journals screened per audit at most; beyond it the doctor reports the cap, never a PASS.
const SEAL_SCREEN_MAX_FILES: usize = 4096;

/// Not sealed journals (crash or still written) among the newest runs of each root, from the
/// bounded structural screening (`journal::screen_seal`), plus whether the file cap was reached.
fn unsealed_journals(roots: &[PathBuf]) -> (Vec<(PathBuf, journal::SealScreen)>, usize, bool) {
    let mut out = Vec::new();
    let mut screened = 0usize;
    let mut capped = false;
    for root in roots {
        for run in crate::storage::preflight::discover_runs(root, None)
            .into_iter()
            .take(3)
        {
            let mut stack = vec![run];
            while let Some(dir) = stack.pop() {
                for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().and_then(|x| x.to_str()) == Some("p0j")
                        && !e.file_name().to_string_lossy().starts_with('.')
                    {
                        if screened == SEAL_SCREEN_MAX_FILES {
                            capped = true;
                            continue;
                        }
                        screened += 1;
                        let screen = journal::screen_seal(&p);
                        if !matches!(screen, journal::SealScreen::Sealed { .. }) {
                            out.push((p, screen));
                        }
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    (out, screened, capped)
}

/// Offline audit; every network-scope check is NOT_RUN.
pub fn offline(inputs: &OfflineInputs) -> Result<BootAudit, String> {
    use CheckState::*;
    let id = identity(&inputs.project, inputs.flags)?;
    let mut checks = Vec::new();
    let mut add =
        |id: &str, name: &str, critical: bool, state: CheckState, source: &str, reason: String| {
            checks.push(BootCheck {
                id: id.into(),
                name: name.into(),
                critical,
                state,
                expected: None,
                observed: None,
                source: source.into(),
                reason,
            })
        };
    // BOOT-01 software / schema
    match (&id.source_head, &id.source_dirty_sha256) {
        (Some(h), Some(_)) => add(
            "BOOT-01",
            "source_identity",
            true,
            Pass,
            "git",
            format!("HEAD {}", &h[..h.len().min(12)]),
        ),
        _ => add(
            "BOOT-01",
            "source_identity",
            true,
            Unknown,
            "git",
            "source identity not readable".into(),
        ),
    }
    let registry_ok = journal::kind::ALL
        .iter()
        .all(|k| payload::registered_schema(*k).is_some() != payload::LEGACY_WRITABLE.contains(k));
    add(
        "BOOT-01",
        "journal_codec_registry",
        true,
        if registry_ok { Pass } else { Blocked },
        "analytics::journal/payload",
        format!(
            "journal v{}, every kind typed xor migration path: {registry_ok}",
            journal::VERSION
        ),
    );
    let anchor =
        |name: &str| solana_sdk::hash::hash(format!("global:{name}").as_bytes()).to_bytes();
    let pinned = MARKERS.iter().all(|(d, n)| anchor(n)[..8] == d[..])
        && RelatedKind::ALL
            .iter()
            .all(|k| anchor(k.name())[..8] == k.discriminator()[..]);
    add(
        "BOOT-01",
        "discriminators",
        true,
        if pinned { Pass } else { Blocked },
        "pinned source 33c67987",
        "six markers and four related instructions match the pinned Anchor rule".into(),
    );
    // X3: the executables a start would admit (control + runtime), present and hashed.
    match &inputs.binary {
        Ok(digest) => add(
            "BOOT-01",
            "runtime_binary",
            true,
            Pass,
            "p0ctl + p0-longtail",
            format!("sha256 {}", &digest[..digest.len().min(12)]),
        ),
        Err(e) => add(
            "BOOT-01",
            "runtime_binary",
            true,
            Blocked,
            "p0ctl + p0-longtail",
            e.clone(),
        ),
    }
    // BOOT-02 host / processes
    match control::owner_state(&inputs.runtime_lock) {
        Ok(control::OwnerState::NotRunning { .. }) => add(
            "BOOT-02",
            "runtime_owner",
            true,
            Pass,
            "runtime lock",
            "no live runtime owner".into(),
        ),
        Ok(control::OwnerState::Running { pid, run_id, .. }) => add(
            "BOOT-02",
            "runtime_owner",
            true,
            Blocked,
            "runtime lock",
            format!("RUNTIME_OWNER_LOCKED pid={pid} run_id={run_id}"),
        ),
        Err(e) => add("BOOT-02", "runtime_owner", true, Unknown, "runtime lock", e),
    }
    let (unsealed, screened, capped) = unsealed_journals(&inputs.run_roots);
    add(
        "BOOT-02",
        "journal_seal_state",
        false,
        if unsealed.is_empty() && !capped {
            Pass
        } else {
            Warn
        },
        "newest runs per root, structural seal screening (no chain verification)",
        match unsealed.first() {
            None if capped => format!(
                "{screened} journals screened, all sealed; cap {SEAL_SCREEN_MAX_FILES} reached, further journals not screened"
            ),
            None => format!("{screened} journals screened, all sealed"),
            Some((path, screen)) => format!(
                "{} of {screened} journals not sealed (crash evidence, kept unchanged){}, e.g. {} ({:?})",
                unsealed.len(),
                if capped { "; screening cap reached" } else { "" },
                path.display(),
                screen
            ),
        },
    );
    match physical_memory_bytes() {
        Some(p) if inputs.max_capture_bytes + inputs.system_reserve_bytes <= p => add(
            "BOOT-02",
            "memory_reserve",
            true,
            Pass,
            "OS",
            format!("{} MiB physical", p / MIB),
        ),
        Some(p) => add(
            "BOOT-02",
            "memory_reserve",
            true,
            Blocked,
            "OS",
            format!("{} MiB physical < capture max + system reserve", p / MIB),
        ),
        None => add(
            "BOOT-02",
            "memory_reserve",
            true,
            Unknown,
            "OS",
            "physical memory unknown".into(),
        ),
    }
    // E6: reserve and E4 write probe on the volume of the run root actually written to.
    let volume = match &inputs.run_output_root {
        Ok(root) => {
            volume_dir(root).ok_or_else(|| format!("no existing ancestor of {}", root.display()))
        }
        Err(e) => Err(e.clone()),
    };
    match volume.as_ref().map(|v| (v, free_disk_bytes(v))) {
        Ok((v, Some(f))) if f > inputs.disk_reserve_bytes => add(
            "BOOT-02",
            "disk_reserve",
            true,
            Pass,
            "OS statvfs, run volume",
            format!(
                "{} MiB free > reserve on {} (sizing: capacity preflight)",
                f / MIB,
                v.display()
            ),
        ),
        Ok((v, Some(f))) => add(
            "BOOT-02",
            "disk_reserve",
            true,
            Blocked,
            "OS statvfs, run volume",
            format!("{} MiB free ≤ disk reserve on {}", f / MIB, v.display()),
        ),
        Ok((v, None)) => add(
            "BOOT-02",
            "disk_reserve",
            true,
            Unknown,
            "OS statvfs, run volume",
            format!("free disk unknown on {}", v.display()),
        ),
        Err(e) => add(
            "BOOT-02",
            "disk_reserve",
            true,
            Unknown,
            "run root",
            format!("run volume unresolved: {e}"),
        ),
    }
    match &volume {
        Ok(v) => match writer_probe(v) {
            Ok(reason) => add(
                "BOOT-02",
                "journal_writer_probe",
                true,
                Pass,
                "journal::Writer/Reader",
                reason,
            ),
            Err(reason) => add(
                "BOOT-02",
                "journal_writer_probe",
                true,
                Blocked,
                "journal::Writer/Reader",
                reason,
            ),
        },
        Err(e) => add(
            "BOOT-02",
            "journal_writer_probe",
            true,
            Unknown,
            "run root",
            format!("run volume unresolved: {e}"),
        ),
    }
    // R4: the BOOT read budget against the minimum and the worst case of the configured scope
    // (config arithmetic only; the network audit itself stays NOT_RUN here).
    let c = darkbot_solana::runtime_config::active();
    let calls = c.limits.boot_calls.min(c.limits.max_requests);
    match boot_read_bounds(c) {
        (Ok(min), Ok(worst)) => add(
            "BOOT-02",
            "boot_read_budget",
            false,
            if calls >= worst { Pass } else { Warn },
            "runtime config",
            if calls < min {
                format!("boot_calls={calls} < minimum {min}: the network audit is refused (BOOT_BUDGET_TOO_SMALL)")
            } else if calls < worst {
                format!("boot_calls={calls} covers the minimum {min} but not the worst case {worst}: dynamic dependencies may block the network audit (fail-closed)")
            } else {
                format!("boot_calls={calls} >= worst case {worst} (minimum {min})")
            },
        ),
        (Err(e), _) | (_, Err(e)) => add(
            "BOOT-02",
            "boot_read_budget",
            false,
            Unknown,
            "runtime config",
            e,
        ),
    }
    // D08: an unavailable wall clock is named, never shown as unix 0.
    let wall = darkbot_common::timing::wall_now().map(|d| d.as_secs());
    // 2026-10-01T00:00:00Z: a wall clock before this cannot be the host's real time.
    add(
        "BOOT-02",
        "clock",
        true,
        if wall.is_ok_and(|w| w >= 1_790_812_800) {
            Pass
        } else {
            Blocked
        },
        "OS",
        match wall {
            Ok(w) => format!("unix {w}; monotonic clock is the process Clock"),
            Err(code) => format!("{code}; monotonic clock is the process Clock"),
        },
    );
    // BOOT-03…08 need Mainnet reads.
    for (id_, name, what) in [
        (
            "BOOT-03",
            "cluster",
            "getGenesisHash/getVersion/slot of the configured cluster",
        ),
        (
            "BOOT-04",
            "wss_provider",
            "WSS connect, ACK, slot liveness, 429, FREE_ONLY quota, archive",
        ),
        (
            "BOOT-05",
            "program_identity",
            "program owner/executable, ProgramData slot/authority/hash vs pin",
        ),
        (
            "BOOT-06",
            "protocol_state",
            "group/bank/mint/oracle relations at one slot",
        ),
        (
            "BOOT-07",
            "dynamic_addresses",
            "PDAs and dynamic accounts derived from parent state",
        ),
        ("BOOT-08", "oracles", "oracle identity, age, confidence"),
    ] {
        add(
            id_,
            name,
            true,
            NotRun,
            "network",
            format!("NOT_RUN offline: {what}"),
        );
    }
    // BOOT-09 wallet / safety
    add(
        "BOOT-09",
        "observe_only",
        true,
        if !id.live_enabled && !id.submission_enabled {
            Pass
        } else {
            Blocked
        },
        "environment",
        format!(
            "P0_LIVE_ENABLED={} P0_SUBMISSION_ENABLED={}",
            id.live_enabled, id.submission_enabled
        ),
    );
    let wallet = std::fs::read_to_string(&inputs.wallet_public_env)
        .ok()
        .and_then(|t| {
            t.lines().find_map(|l| {
                l.trim()
                    .strip_prefix("P0_EXPECTED_PUBKEY=")
                    .map(|v| v.trim_matches('"').to_string())
            })
        });
    match wallet
        .as_deref()
        .map(str::parse::<solana_sdk::pubkey::Pubkey>)
    {
        Some(Ok(k)) => add(
            "BOOT-09",
            "wallet_public_identity",
            false,
            Pass,
            "config/wallet.public.env",
            format!("public key {k} (no private key read)"),
        ),
        Some(Err(_)) => add(
            "BOOT-09",
            "wallet_public_identity",
            false,
            Blocked,
            "config/wallet.public.env",
            "invalid public key".into(),
        ),
        None => add(
            "BOOT-09",
            "wallet_public_identity",
            false,
            Unknown,
            "config/wallet.public.env",
            "not configured (observe runs need none)".into(),
        ),
    }
    // BOOT-10 measurement scope
    add(
        "BOOT-10",
        "measurement_scope",
        true,
        Pass,
        "market_truth, related",
        "six markers, four classes, four related instructions registered; source coverage stays PROVISIONAL (coverage_verified=false)".into(),
    );
    // BOOT-11 mode-dependent execution readiness
    add(
        "BOOT-11",
        "execution_readiness",
        false,
        NotApplicable,
        "mode",
        "observe-only run: no signer, route or submission check".into(),
    );
    let token = token(&id)?;
    // BOOT-12 binding
    add(
        "BOOT-12",
        "readiness_binding",
        true,
        Pass,
        "token",
        "start recomputes the token and refuses on drift".into(),
    );
    let blocked = checks
        .iter()
        .any(|c| c.critical && matches!(c.state, Blocked | Unknown));
    let verdict = if blocked {
        AuditVerdict::Blocked
    } else if checks.iter().any(|c| c.state == NotRun) {
        AuditVerdict::OfflineReadyNetworkNotRun
    } else {
        AuditVerdict::Ready
    };
    Ok(BootAudit {
        scope: "offline".into(),
        checks,
        identity: id,
        verdict,
        readiness_token: token,
    })
}

/// BOOT-12 TOCTOU: the audited identity must be unchanged at start.
pub fn recheck(audit: &BootAudit, project: &Path, flags: SafetyFlags) -> Result<(), String> {
    let now = identity(project, flags)?;
    if token(&now)? != audit.readiness_token {
        let mut drift = Vec::new();
        let (a, b) = (&audit.identity, &now);
        if a.source_head != b.source_head || a.source_dirty_sha256 != b.source_dirty_sha256 {
            drift.push("source");
        }
        if a.runtime_config_sha256 != b.runtime_config_sha256 {
            drift.push("runtime_config");
        }
        if a.live_enabled != b.live_enabled || a.submission_enabled != b.submission_enabled {
            drift.push("safety_flags");
        }
        if a.journal_version != b.journal_version
            || a.discriminator_table_sha256 != b.discriminator_table_sha256
        {
            drift.push("schema");
        }
        return Err(format!("BOOT_TOCTOU_DRIFT: {}", drift.join(",")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(d: &Path) -> OfflineInputs {
        OfflineInputs {
            project: d.into(),
            runtime_lock: d.join("data/.p0-runtime.lock"),
            run_roots: vec![d.join("data/runs")],
            run_output_root: Ok(d.join("data/runs")),
            binary: Ok("fixture-binary-digest".into()),
            wallet_public_env: d.join("config/wallet.public.env"),
            max_capture_bytes: 512 * MIB,
            system_reserve_bytes: 1024 * MIB,
            disk_reserve_bytes: MIB,
            flags: SafetyFlags {
                live_enabled: false,
                submission_enabled: false,
            },
        }
    }

    /// Offline audit: network checks NOT_RUN (never READY), a live runtime owner and enabled
    /// LIVE flags block, an unsealed earlier journal warns, and the token detects drift.
    #[test]
    fn offline_audit_blocks_and_binds_identity() {
        let d = std::env::temp_dir().join(format!("p0_boot_{}_{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("data/runs/r1")).unwrap();
        std::fs::create_dir_all(d.join("config")).unwrap();
        let git = |args: &[&str]| {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(&d)
                .args(args)
                .output()
                .unwrap()
                .status
                .success())
        };
        git(&["init", "-q"]);
        // As in the project: run data is ignored, so writing it is not a source change.
        std::fs::write(d.join(".gitignore"), b"data/\n").unwrap();
        git(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "t",
        ]);
        std::fs::write(
            d.join("config/wallet.public.env"),
            format!(
                "P0_EXPECTED_PUBKEY={}\n",
                solana_sdk::pubkey::Pubkey::new_unique()
            ),
        )
        .unwrap();
        let a = offline(&inputs(&d)).unwrap();
        assert_eq!(
            a.verdict,
            AuditVerdict::OfflineReadyNetworkNotRun,
            "{}",
            a.terminal()
        );
        assert_eq!(
            a.checks
                .iter()
                .filter(|c| c.state == CheckState::NotRun)
                .count(),
            6
        );
        let off = inputs(&d).flags;
        assert!(recheck(&a, &d, off).is_ok());
        // Unsealed journal of an earlier run: warning, kept unchanged.
        let mut w = journal::Writer::create(&d.join("data/runs/r1/market.p0j")).unwrap();
        w.sync().unwrap();
        let b = offline(&inputs(&d)).unwrap();
        assert!(b
            .checks
            .iter()
            .any(|c| c.name == "journal_seal_state" && c.state == CheckState::Warn));
        drop(w);
        // A live owner blocks.
        let lock =
            control::acquire_owner(&d.join("data/.p0-runtime.lock"), "r2", "econ4h").unwrap();
        assert_eq!(offline(&inputs(&d)).unwrap().verdict, AuditVerdict::Blocked);
        drop(lock);
        // Enabled LIVE blocks and drifts the token; so does a source change.
        let live = SafetyFlags {
            live_enabled: true,
            submission_enabled: false,
        };
        let mut i = inputs(&d);
        i.flags = live;
        assert_eq!(offline(&i).unwrap().verdict, AuditVerdict::Blocked);
        assert_eq!(
            recheck(&a, &d, live),
            Err("BOOT_TOCTOU_DRIFT: safety_flags".into())
        );
        std::fs::write(d.join("new_file.rs"), b"x").unwrap();
        assert_eq!(
            recheck(&a, &d, off),
            Err("BOOT_TOCTOU_DRIFT: source".into())
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    fn temp(tag: u32) -> PathBuf {
        let d = std::env::temp_dir().join(format!("p0_boot_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn probe_files(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".p0-boot-probe-")
            })
            .count()
    }

    /// E6: the run root is resolved like the launcher's run store, and the reserve is measured
    /// on the volume of that root (nearest existing ancestor, symlink resolved), not on data/.
    #[test]
    fn run_volume_resolves_like_the_launcher() {
        let project = Path::new("/p0");
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert_eq!(
            econ_run_root(project, &env(&[])),
            Ok(project.join("data/runs"))
        );
        assert_eq!(
            econ_run_root(project, &env(&[("P0_RUN_STORE", "legacy")])),
            Ok(project.join("data/econ4h"))
        );
        assert_eq!(
            econ_run_root(
                project,
                &env(&[("P0_RUN_STORE", "runs"), ("P0_ECON_OUT_DIR", "")])
            ),
            Ok(project.join("data/runs"))
        );
        assert!(econ_run_root(project, &env(&[("P0_RUN_STORE", "x")])).is_err());
        assert_eq!(
            econ_run_root(project, &env(&[("P0_ECON_OUT_DIR", "/vol/runs")])),
            Ok(PathBuf::from("/vol/runs"))
        );
        assert_eq!(
            econ_run_root(
                project,
                &env(&[("P0_ECON_OUT_DIR", "~/r"), ("HOME", "/home/u")])
            ),
            Ok(PathBuf::from("/home/u/r"))
        );

        let d = temp(line!());
        let elsewhere = temp(line!());
        std::os::unix::fs::symlink(&elsewhere, d.join("link")).unwrap();
        // Missing run directory: its nearest existing ancestor, through the symlink.
        assert_eq!(
            volume_dir(&d.join("link/not/yet")),
            Some(elsewhere.canonicalize().unwrap())
        );
        let mut i = inputs(&d);
        i.run_output_root = Ok(d.join("link/runs"));
        let a = offline(&i).unwrap();
        let disk = a.checks.iter().find(|c| c.name == "disk_reserve").unwrap();
        assert_eq!(disk.state, CheckState::Pass, "{}", a.terminal());
        assert!(
            disk.reason
                .contains(&elsewhere.canonicalize().unwrap().display().to_string()),
            "{}",
            disk.reason
        );
        // An unresolvable run root is no PASS: both run-volume checks are UNKNOWN and block.
        i.run_output_root = Err("RUN_STORE_INVALID".into());
        let b = offline(&i).unwrap();
        for name in ["disk_reserve", "journal_writer_probe"] {
            let c = b.checks.iter().find(|c| c.name == name).unwrap();
            assert_eq!(c.state, CheckState::Unknown, "{name}");
        }
        assert_eq!(b.verdict, AuditVerdict::Blocked);
        std::fs::remove_dir_all(&d).unwrap();
        std::fs::remove_dir_all(&elsewhere).unwrap();
    }

    /// X3: the offline BOOT-01/02 matrix. Each local fault yields exactly its check state and the
    /// verdict follows the criticality; the six network checks stay NOT_RUN offline and an
    /// offline-only audit is never READY.
    #[test]
    fn boot_01_02_offline_matrix() {
        let d = temp(line!());
        std::fs::create_dir_all(d.join("data/runs/r1")).unwrap();
        std::fs::create_dir_all(d.join("config")).unwrap();
        let git = |args: &[&str]| {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(&d)
                .args(args)
                .output()
                .unwrap()
                .status
                .success())
        };
        git(&["init", "-q"]);
        std::fs::write(d.join(".gitignore"), b"data/\n").unwrap();
        git(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "t",
        ]);
        let state = |a: &BootAudit, name: &str| {
            a.checks
                .iter()
                .find(|c| c.name == name)
                .unwrap_or_else(|| panic!("{name} missing"))
                .state
        };
        let not_run = |a: &BootAudit| {
            a.checks
                .iter()
                .filter(|c| c.state == CheckState::NotRun)
                .count()
        };
        // Healthy local host: offline-ready, network NOT_RUN, never READY.
        let ok = offline(&inputs(&d)).unwrap();
        for name in [
            "journal_codec_registry",
            "discriminators",
            "runtime_binary",
            "runtime_owner",
            "journal_seal_state",
            "disk_reserve",
            "journal_writer_probe",
            "clock",
        ] {
            assert_eq!(
                state(&ok, name),
                CheckState::Pass,
                "{name}: {}",
                ok.terminal()
            );
        }
        assert_eq!(
            (ok.verdict, not_run(&ok)),
            (AuditVerdict::OfflineReadyNetworkNotRun, 6)
        );
        // Missing runtime executable: BOOT-01 blocks.
        let mut i = inputs(&d);
        i.binary = Err("BOOT_RUNTIME_BINARY_MISSING".into());
        let a = offline(&i).unwrap();
        assert_eq!(
            (state(&a, "runtime_binary"), a.verdict),
            (CheckState::Blocked, AuditVerdict::Blocked)
        );
        assert_eq!(
            binary_identity().err().as_deref(),
            Some("BOOT_RUNTIME_BINARY_MISSING"),
            "the test executable has no p0-longtail next to it"
        );
        // A corrupt journal of an earlier run (foreign version/magic): crash evidence, WARN.
        std::fs::write(d.join("data/runs/r1/market.p0j"), [b'x'; 200]).unwrap();
        let a = offline(&inputs(&d)).unwrap();
        assert_eq!(state(&a, "journal_seal_state"), CheckState::Warn);
        assert_eq!(a.verdict, AuditVerdict::OfflineReadyNetworkNotRun);
        std::fs::remove_file(d.join("data/runs/r1/market.p0j")).unwrap();
        // A live runtime owner blocks.
        let lock =
            control::acquire_owner(&d.join("data/.p0-runtime.lock"), "r2", "econ4h").unwrap();
        let a = offline(&inputs(&d)).unwrap();
        assert_eq!(
            (state(&a, "runtime_owner"), a.verdict),
            (CheckState::Blocked, AuditVerdict::Blocked)
        );
        drop(lock);
        // Disk reserve beyond the free bytes of the run volume blocks.
        let mut i = inputs(&d);
        i.disk_reserve_bytes = u64::MAX;
        let a = offline(&i).unwrap();
        assert_eq!(
            (state(&a, "disk_reserve"), a.verdict),
            (CheckState::Blocked, AuditVerdict::Blocked)
        );
        // Capture maximum plus system reserve beyond physical memory blocks (or is UNKNOWN).
        let mut i = inputs(&d);
        i.max_capture_bytes = u64::MAX / 2;
        i.system_reserve_bytes = u64::MAX / 2;
        let a = offline(&i).unwrap();
        assert!(matches!(
            state(&a, "memory_reserve"),
            CheckState::Blocked | CheckState::Unknown
        ));
        assert_eq!(a.verdict, AuditVerdict::Blocked);
        // Enabled LIVE flags block (observe-only scope).
        let mut i = inputs(&d);
        i.flags.live_enabled = true;
        assert_eq!(offline(&i).unwrap().verdict, AuditVerdict::Blocked);
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// R4: the doctor reports the BOOT read budget against the minimum and the worst case of the
    /// configured scope (non-critical; PASS only at or above the worst case).
    #[cfg(feature = "crate_health")]
    #[test]
    fn boot_read_budget_is_reported_against_the_worst_case() {
        let d = temp(line!());
        let a = offline(&inputs(&d)).unwrap();
        let c = a
            .checks
            .iter()
            .find(|c| c.name == "boot_read_budget")
            .unwrap();
        assert!(!c.critical);
        let cfg = darkbot_solana::runtime_config::active();
        let worst = super::super::boot_network::worst_case_reads(cfg).unwrap();
        let calls = cfg.limits.boot_calls.min(cfg.limits.max_requests);
        let expected = if calls >= worst {
            CheckState::Pass
        } else {
            CheckState::Warn
        };
        assert_eq!(c.state, expected, "{}", c.reason);
        assert!(c.reason.contains(&worst.to_string()), "{}", c.reason);
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// Without the health engine there is no FE scope: the BOOT read budget is unknown, never PASS.
    #[cfg(not(feature = "crate_health"))]
    #[test]
    fn boot_read_budget_is_unknown_without_the_health_engine() {
        let d = temp(line!());
        let a = offline(&inputs(&d)).unwrap();
        let c = a
            .checks
            .iter()
            .find(|c| c.name == "boot_read_budget")
            .unwrap();
        assert!(!c.critical);
        assert_eq!(c.state, CheckState::Unknown);
        assert_eq!(c.reason, "BOOT_SCOPE_NEEDS_CRATE_HEALTH");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// E4: the write probe creates, writes, syncs, seals, reads back and removes a journal on the
    /// run volume; a read-only volume or a failing sync blocks, and no probe file stays behind.
    #[test]
    fn writer_probe_passes_and_fails_closed() {
        use std::os::unix::fs::PermissionsExt;
        let d = temp(line!());
        let ok = writer_probe(&d).unwrap();
        assert!(ok.contains("readback"), "{ok}");
        assert_eq!(probe_files(&d), 0);

        let synced = writer_probe_with(&d, &|w| w.test_fail_next_sync(std::io::ErrorKind::Other));
        assert!(
            synced
                .as_ref()
                .is_err_and(|e| e.starts_with("WRITER_PROBE_SYNC")),
            "{synced:?}"
        );
        assert_eq!(probe_files(&d), 0, "a failed probe is removed as well");

        let ro = d.join("ro");
        std::fs::create_dir(&ro).unwrap();
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o500)).unwrap();
        let denied = writer_probe(&ro);
        assert!(
            denied
                .as_ref()
                .is_err_and(|e| e.starts_with("WRITER_PROBE_CREATE")),
            "{denied:?}"
        );
        // In the audit a read-only run volume blocks the start.
        let mut i = inputs(&d);
        i.run_output_root = Ok(ro.join("runs"));
        let a = offline(&i).unwrap();
        let probe = a
            .checks
            .iter()
            .find(|c| c.name == "journal_writer_probe")
            .unwrap();
        assert_eq!(probe.state, CheckState::Blocked, "{}", a.terminal());
        assert_eq!(a.verdict, AuditVerdict::Blocked);
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// E5: the doctor's seal screening reads the header and a bounded tail only. It recognizes
    /// sealed/unsealed/torn/zero-filled/foreign files; it is no chain verification — a
    /// manipulated middle frame is found by the full reader, not by the screening.
    #[test]
    fn seal_screening_is_bounded_and_structural_only() {
        use journal::{screen_seal, SealScreen};
        let d = temp(line!());
        let line = |n: usize| payload::OwnerDiagnostic {
            line: "x".repeat(n),
        };
        let write = |name: &str, frames: usize, size: usize, seal: Option<bool>| {
            let path = d.join(name);
            let mut w = journal::Writer::create(&path).unwrap();
            for i in 0..frames {
                let p = journal::Provenance {
                    source_seq: i as u64,
                    ..Default::default()
                };
                w.append_typed(p, &line(size)).unwrap();
            }
            w.sync().unwrap();
            if let Some(complete) = seal {
                w.seal(complete).unwrap();
            }
            path
        };
        let sealed = write("sealed.p0j", 3, 10, Some(true));
        assert_eq!(screen_seal(&sealed), SealScreen::Sealed { complete: true });
        let incomplete = write("incomplete.p0j", 3, 10, Some(false));
        assert_eq!(
            screen_seal(&incomplete),
            SealScreen::Sealed { complete: false }
        );
        assert_eq!(
            screen_seal(&write("open.p0j", 3, 10, None)),
            SealScreen::Unsealed
        );
        assert_eq!(
            screen_seal(&write("empty.p0j", 0, 0, None)),
            SealScreen::Unsealed
        );
        // Large journal (frames far beyond the window): still decided from its tail.
        let large = write("large.p0j", 64, 60 * 1024, Some(true));
        assert!(std::fs::metadata(&large).unwrap().len() > 30 * journal::SEAL_SCREEN_TAIL);
        assert_eq!(screen_seal(&large), SealScreen::Sealed { complete: true });
        // A last regular frame longer than the window is never taken for a seal.
        let long_tail = write("long_tail.p0j", 1, 100 * 1024, None);
        assert_eq!(
            screen_seal(&long_tail),
            SealScreen::Undecodable("TAIL_UNDECODABLE")
        );
        // Torn manifest and zero-filled tail.
        let torn = write("torn.p0j", 3, 10, Some(true));
        let len = std::fs::metadata(&torn).unwrap().len();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&torn)
            .unwrap()
            .set_len(len - 7)
            .unwrap();
        assert_eq!(
            screen_seal(&torn),
            SealScreen::Undecodable("TAIL_UNDECODABLE")
        );
        let zeros = write("zeros.p0j", 3, 10, None);
        let mut z = std::fs::OpenOptions::new()
            .append(true)
            .open(&zeros)
            .unwrap();
        std::io::Write::write_all(&mut z, &[0u8; 4096]).unwrap();
        assert_eq!(
            screen_seal(&zeros),
            SealScreen::Undecodable("TAIL_UNDECODABLE")
        );
        std::fs::write(d.join("foreign.p0j"), [b'x'; 200]).unwrap();
        assert_eq!(
            screen_seal(&d.join("foreign.p0j")),
            SealScreen::Undecodable("JOURNAL_VERSION_OR_MAGIC")
        );
        std::fs::write(d.join("short.p0j"), b"P0J").unwrap();
        assert_eq!(
            screen_seal(&d.join("short.p0j")),
            SealScreen::Undecodable("HEADER_TRUNCATED")
        );
        // Manipulated middle frame: the screening still sees the intact manifest (structural
        // only); the full reader refuses the chain.
        let mut bytes = std::fs::read(&sealed).unwrap();
        let first_payload = journal::HEADER_LEN as usize + 4 + 120;
        bytes[first_payload] ^= 0xFF;
        std::fs::write(&sealed, &bytes).unwrap();
        assert_eq!(screen_seal(&sealed), SealScreen::Sealed { complete: true });
        let mut r = journal::Reader::open(&sealed).unwrap();
        let full = loop {
            match r.next_frame() {
                Ok(Some(_)) => continue,
                other => break other,
            }
        };
        assert!(
            full.is_err(),
            "the full reader detects the manipulated frame"
        );
        std::fs::remove_dir_all(&d).unwrap();
    }
}

/// Network evidence uses the same canonical typed journal, never a JSON primary file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountProof {
    pub address: String,
    pub owner: String,
    pub data_sha256: String,
    pub bytes: u64,
    pub executable: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkAudit {
    pub audit: BootAudit,
    pub observed_at: u64,
    pub expires_at: u64,
    pub slot: u64,
    pub genesis: String,
    pub rpc_binding: String,
    pub wss_binding: String,
    pub binary_sha256: String,
    pub provider_proof_sha256: String,
    pub capture_source: String,
    pub accounts: Vec<AccountProof>,
    pub state_hash: String,
    /// DEC-07: the TTL this record was issued with (`expires_at − observed_at`, from its oldest
    /// network read) and the source of that value (`boot_token_ttl_seconds`); covered by the
    /// readiness token, so no later configuration extends it.
    pub ttl_seconds: u64,
    pub ttl_source: String,
}
impl payload::Payload for NetworkAudit {
    const KIND: u16 = journal::kind::BOOT_NETWORK;
    /// DEC-07: 3 — the TTL and its source. Schema 2 stays readable (a fixed 60 s TTL); its token
    /// covers that earlier layout, so such a record never admits a start.
    const SCHEMA: u16 = payload::TYPED + 1;
    fn decode_earlier(schema: u16, bytes: &[u8]) -> Option<std::io::Result<Self>> {
        (schema == payload::TYPED)
            .then(|| payload::decode_wire::<NetworkAuditV2>(bytes).map(NetworkAudit::from))
    }
}
/// The record before DEC-07 (schema 2): no TTL field.
#[derive(Deserialize)]
struct NetworkAuditV2 {
    audit: BootAudit,
    observed_at: u64,
    expires_at: u64,
    slot: u64,
    genesis: String,
    rpc_binding: String,
    wss_binding: String,
    binary_sha256: String,
    provider_proof_sha256: String,
    capture_source: String,
    accounts: Vec<AccountProof>,
    state_hash: String,
}
/// The source of a TTL read from a record written before DEC-07.
pub const TTL_SOURCE_PRE_DEC07: &str = "pre_dec07";
impl From<NetworkAuditV2> for NetworkAudit {
    fn from(v: NetworkAuditV2) -> Self {
        Self {
            ttl_seconds: v.expires_at.saturating_sub(v.observed_at),
            ttl_source: TTL_SOURCE_PRE_DEC07.into(),
            audit: v.audit,
            observed_at: v.observed_at,
            expires_at: v.expires_at,
            slot: v.slot,
            genesis: v.genesis,
            rpc_binding: v.rpc_binding,
            wss_binding: v.wss_binding,
            binary_sha256: v.binary_sha256,
            provider_proof_sha256: v.provider_proof_sha256,
            capture_source: v.capture_source,
            accounts: v.accounts,
            state_hash: v.state_hash,
        }
    }
}
impl NetworkAudit {
    /// DEC-07: `at` lies within the record's own TTL — the one it was issued with, within the
    /// user's 10..=120 s, counted from its oldest network read (`observed_at`).
    fn within_ttl(&self, at: u64) -> bool {
        use darkbot_solana::runtime_config::defaults::{
            BOOT_TOKEN_TTL_MAX_SECONDS, BOOT_TOKEN_TTL_MIN_SECONDS,
        };
        (BOOT_TOKEN_TTL_MIN_SECONDS..=BOOT_TOKEN_TTL_MAX_SECONDS).contains(&self.ttl_seconds)
            && self.observed_at.checked_add(self.ttl_seconds) == Some(self.expires_at)
            && (self.observed_at..self.expires_at).contains(&at)
    }

    pub fn validate_start(
        &self,
        current: &AuditedIdentity,
        binary: &str,
        now: u64,
        source: &str,
    ) -> Result<(), String> {
        self.verify_record()?;
        if !self.within_ttl(now) {
            return Err("BOOT_NETWORK_EXPIRED".into());
        }
        if current != &self.audit.identity
            || binary != self.binary_sha256
            || source != self.capture_source
        {
            return Err("BOOT_NETWORK_TOCTOU".into());
        }
        Ok(())
    }

    /// The record itself: a READY network audit with every network check PASS whose readiness
    /// token still covers its content (no tampering).
    fn verify_record(&self) -> Result<(), String> {
        if self.audit.verdict != AuditVerdict::Ready
            || self.audit.scope != "network"
            || self.audit.checks.iter().any(|c| {
                c.critical && !matches!(c.state, CheckState::Pass | CheckState::NotApplicable)
            })
        {
            return Err("BOOT_NETWORK_NOT_READY".into());
        }
        for id in [
            "BOOT-03", "BOOT-04", "BOOT-05", "BOOT-06", "BOOT-07", "BOOT-08",
        ] {
            if self
                .audit
                .checks
                .iter()
                .filter(|c| c.id == id && c.critical && c.state == CheckState::Pass)
                .count()
                != 1
            {
                return Err("BOOT_NETWORK_CHECK_MISSING".into());
            }
        }
        let mut copy = self.clone();
        copy.audit.readiness_token.clear();
        let actual = sha(&bincode::serialize(&copy).map_err(|_| "BOOT_TOKEN")?);
        if actual != self.audit.readiness_token {
            return Err("BOOT_NETWORK_TOKEN".into());
        }
        Ok(())
    }

    /// X1: the runtime started by `p0ctl run start` binds itself to the approved record before
    /// admission, without any network read. The wall clock is not re-checked here (build,
    /// preflight and StreamReady lie between approval and bootstrap); instead the approval
    /// itself must have happened inside the record's TTL, for exactly this record (operator
    /// token prefix), by this code, config, safety flags, executables, capture source and the
    /// pinned genesis.
    pub fn validate_admission(
        &self,
        approved_at: u64,
        approved_token: &str,
        current: &AuditedIdentity,
        binary: &str,
        source: &str,
    ) -> Result<(), String> {
        self.verify_record()?;
        if approved_token.len() != 16 || !self.audit.readiness_token.starts_with(approved_token) {
            return Err("BOOT_NETWORK_TOKEN".into());
        }
        if !self.within_ttl(approved_at) {
            return Err("BOOT_NETWORK_EXPIRED".into());
        }
        let (a, b) = (&self.audit.identity, current);
        let mut drift = Vec::new();
        if a.source_head != b.source_head || a.source_dirty_sha256 != b.source_dirty_sha256 {
            drift.push("source");
        }
        if a.runtime_config_sha256 != b.runtime_config_sha256 {
            drift.push("runtime_config");
        }
        if a.live_enabled != b.live_enabled || a.submission_enabled != b.submission_enabled {
            drift.push("safety_flags");
        }
        if a.journal_version != b.journal_version
            || a.discriminator_table_sha256 != b.discriminator_table_sha256
        {
            drift.push("schema");
        }
        if binary != self.binary_sha256 {
            drift.push("binary");
        }
        if source != self.capture_source {
            drift.push("capture_source");
        }
        if self.genesis != darkbot_project0::protocol::GENESIS_HASH_MAINNET {
            drift.push("genesis");
        }
        if drift.is_empty() {
            Ok(())
        } else {
            Err(format!("BOOT_IDENTITY_DRIFT: {}", drift.join(",")))
        }
    }

    /// X1: binds the audited accounts to the state the runtime itself read at bootstrap (no
    /// extra read). Bound and required: ProgramData (owner, length, data hash, executable from
    /// the startup read), every account the runtime lists as required dependency (group, fee
    /// state, clock, oracles, mints, integration accounts) and every audited bank — a missing
    /// one or a different owner/length is drift. Data other than ProgramData legitimately changes
    /// within the TTL and is not compared. The program account is always outside the runtime's
    /// scope; other audited accounts outside it (liquidity vaults, targets without liabilities)
    /// are bound when present and otherwise reported as outside the runtime scope, never as a
    /// pass of their identity.
    pub fn bind_state(
        &self,
        program: &solana_sdk::pubkey::Pubkey,
        program_data: &solana_sdk::pubkey::Pubkey,
        program_data_executable: bool,
        required: &std::collections::HashSet<solana_sdk::pubkey::Pubkey>,
        state: &dyn darkbot_project0::health::AccountSource,
    ) -> Result<IdentityBinding, String> {
        use std::str::FromStr;
        let bank_bytes = (8 + darkbot_project0::protocol::BANK_PAYLOAD_LEN_PINNED) as u64;
        let mut binding = IdentityBinding::default();
        let mut drift = Vec::new();
        for proof in &self.accounts {
            let key = solana_sdk::pubkey::Pubkey::from_str(&proof.address)
                .map_err(|_| "BOOT_IDENTITY_RECORD_INVALID".to_string())?;
            // The executable program account is never part of the runtime's state.
            if key == *program {
                binding.outside_scope.push(proof.address.clone());
                continue;
            }
            let bank = proof.owner == program.to_string() && proof.bytes == bank_bytes;
            let in_scope = key == *program_data || required.contains(&key) || bank;
            match state.account(&key) {
                None if in_scope => drift.push(format!("{key}:missing")),
                None => binding.outside_scope.push(proof.address.clone()),
                Some((owner, _, data)) => {
                    if owner.to_string() != proof.owner {
                        drift.push(format!("{key}:owner"));
                    } else if data.len() as u64 != proof.bytes {
                        drift.push(format!("{key}:length"));
                    } else if key == *program_data
                        && (sha(data) != proof.data_sha256
                            || proof.executable != program_data_executable)
                    {
                        drift.push(format!("{key}:program_data"));
                    } else if key != *program_data && proof.executable {
                        drift.push(format!("{key}:executable"));
                    } else {
                        binding.bound += 1;
                    }
                }
            }
        }
        if drift.is_empty() {
            Ok(binding)
        } else {
            drift.sort();
            Err(format!("BOOT_IDENTITY_DRIFT: {}", drift.join(",")))
        }
    }
}

/// Result of [`NetworkAudit::bind_state`]: accounts bound and audited accounts outside the
/// runtime's scope (reported, never counted as bound).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdentityBinding {
    pub bound: usize,
    pub outside_scope: Vec<String>,
}

/// The fixed code of an admission refusal (X1), for the owner's terminal report.
pub fn admission_code(e: &str) -> &'static str {
    for code in [
        "BOOT_IDENTITY_DRIFT",
        "BOOT_NETWORK_AUDIT_MISSING",
        "BOOT_NETWORK_EVIDENCE_INVALID",
        "BOOT_NETWORK_EXPIRED",
        "BOOT_NETWORK_TOKEN",
        "BOOT_NETWORK_NOT_READY",
        "BOOT_NETWORK_CHECK_MISSING",
    ] {
        if e.starts_with(code) {
            return code;
        }
    }
    "BOOT_ADMISSION_REFUSED"
}

/// X1: what `p0ctl run start` hands the runtime it launches (environment of the launcher).
pub const ENV_NETWORK_AUDIT: &str = "P0_NETWORK_AUDIT";
pub const ENV_NETWORK_AUDIT_TOKEN: &str = "P0_NETWORK_AUDIT_TOKEN";
pub const ENV_NETWORK_AUDIT_APPROVED_AT: &str = "P0_NETWORK_AUDIT_APPROVED_AT";

/// X1: the network audit a runtime start is bound to. Every start reads Mainnet, so it must
/// carry record, token and approval time and pass [`NetworkAudit::validate_admission`];
/// anything missing or invalid refuses the start before any network read (DEC-07: no start
/// without its record — the former unlaunched path without one is gone).
pub fn admission_record(
    get: &dyn Fn(&str) -> Option<String>,
    current: &dyn Fn() -> Result<AuditedIdentity, String>,
    binary: &dyn Fn() -> Result<String, String>,
) -> Result<NetworkAudit, String> {
    let missing = || "BOOT_NETWORK_AUDIT_MISSING".to_string();
    let path = get(ENV_NETWORK_AUDIT)
        .filter(|v| !v.is_empty())
        .ok_or_else(missing)?;
    let token = get(ENV_NETWORK_AUDIT_TOKEN).ok_or_else(missing)?;
    let approved_at = get(ENV_NETWORK_AUDIT_APPROVED_AT)
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or_else(missing)?;
    let record: NetworkAudit =
        journal::read_typed_value(Path::new(&path)).map_err(|_| "BOOT_NETWORK_EVIDENCE_INVALID")?;
    let source = runtime_capture_source(get)
        .map_err(|e| format!("BOOT_IDENTITY_DRIFT: capture_source ({e})"))?;
    record.validate_admission(approved_at, &token, &current()?, &binary()?, source)?;
    Ok(record)
}

/// The capture source a Shadow run is audited with: its owner subscribes to accounts and
/// captures no market stream of its own choice (`p0ctl boot` and `run start` refuse another).
pub const SHADOW_CAPTURE_SOURCE: &str = "logs";

/// The capture source of the started runtime: a Shadow run's is fixed; econ4h and the raw
/// capture resolve `P0_MARKET_SOURCE` exactly as their market capture does.
pub fn runtime_capture_source(
    get: &dyn Fn(&str) -> Option<String>,
) -> Result<&'static str, String> {
    if get("P0_MODE").as_deref() == Some("shadow") {
        return Ok(SHADOW_CAPTURE_SOURCE);
    }
    crate::market_observer::source_name(get("P0_MARKET_SOURCE").as_deref())
}

/// Bind both the control executable and the actual runtime it admits.
pub fn binary_identity() -> Result<String, String> {
    binary_identity_for("econ4h")
}

/// The runtime a boot flow of operating mode `mode` admits, beside `p0ctl`: the raw capture
/// (`p0-market-capture-reported`, DEC-07 boot mode `market`) or the runtime (`p0-longtail`).
pub fn binary_identity_for(mode: &str) -> Result<String, String> {
    let ctl = std::env::current_exe().map_err(|_| "BOOT_BINARY")?;
    let runtime = if mode == MARKET_MODE {
        "p0-market-capture-reported"
    } else {
        "p0-longtail"
    };
    binary_identity_of(&ctl, &ctl.parent().ok_or("BOOT_BINARY")?.join(runtime))
}

/// DEC-07: the boot mode of a standalone raw capture.
pub const MARKET_MODE: &str = "market";
/// DEC-07: a standalone raw capture without its admitted boot flow.
pub const BOOT_AUDIT_REQUIRED: &str = "BOOT_AUDIT_REQUIRED";
/// DEC-07: an approved raw-capture start admits one capture; a second start of it is refused.
pub const BOOT_ADMISSION_CONSUMED: &str = "BOOT_ADMISSION_CONSUMED";

/// DEC-07: the admission of a standalone raw capture — every one reads Mainnet, so it starts
/// only admitted to its approved network audit of boot mode `market` (`p0ctl boot start`
/// through the desktop launcher): record, token and approval time inside the record's TTL,
/// this code, configuration, safety flags, executables and capture source, and exactly the
/// audited duration; the capture's own clock inside the record's TTL (it starts right after its
/// approval), and once: the admission is consumed. Anything else ends before any read; without
/// a record BOOT_AUDIT_REQUIRED.
pub fn market_admission(
    get: &dyn Fn(&str) -> Option<String>,
) -> Result<MarketAdmission, &'static str> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "BOOT_CLOCK")?
        .as_secs();
    market_admission_with(
        get,
        &|| {
            let project = get("P0_PROJECT_DIR")
                .ok_or_else(|| "BOOT_NETWORK_AUDIT_MISSING: P0_PROJECT_DIR".to_string())?;
            launched_identity(Path::new(&project), SafetyFlags::from_env(), &[], get)
        },
        &runtime_binary_identity,
        &|| {
            let horizon = get("P0_MEASUREMENT_DURATION")
                .ok_or_else(|| "BOOT_NETWORK_AUDIT_MISSING: P0_MEASUREMENT_DURATION".to_string())?;
            start_config(MARKET_MODE, &[], &horizon, get).map(|r| r.config)
        },
        now,
    )
}

/// DEC-16: the qualification an unbounded raw capture is admitted with (its id in the store).
pub const ENV_UNTIL_STOPPED_QUALIFICATION: &str = "P0_UNTIL_STOPPED_QUALIFICATION";

/// An admitted raw capture: its approved network record and, for an unbounded run, what its
/// rolling windows check against.
#[derive(Clone, Debug)]
pub struct MarketAdmission {
    pub record: NetworkAudit,
    pub rolling: Option<super::until_stopped::Rolling>,
}

/// `market_admission` with the identity, the executables and the configuration of the running
/// capture. DEC-16: an unbounded horizon (`until-stopped:W:R`, the capture `until-stopped`) is
/// admitted only by `RunHorizon::admit` with its qualification
/// (`P0_UNTIL_STOPPED_QUALIFICATION`) bound to exactly this capture.
pub(crate) fn market_admission_with(
    get: &dyn Fn(&str) -> Option<String>,
    current: &dyn Fn() -> Result<AuditedIdentity, String>,
    binary: &dyn Fn() -> Result<String, String>,
    config: &dyn Fn() -> Result<darkbot_solana::runtime_config::RuntimeConfig, String>,
    now: u64,
) -> Result<MarketAdmission, &'static str> {
    use darkbot_solana::runtime_config::{horizon, Horizon, UNTIL_STOPPED};
    if get("P0_MODE").as_deref() != Some(MARKET_MODE)
        || get(ENV_NETWORK_AUDIT).is_none_or(|v| v.is_empty())
    {
        return Err(BOOT_AUDIT_REQUIRED);
    }
    let audited = get("P0_MEASUREMENT_DURATION").ok_or(BOOT_AUDIT_REQUIRED)?;
    let horizon = horizon(&audited).map_err(|_| BOOT_AUDIT_REQUIRED)?;
    let market = get("P0_MARKET_DURATION_SECS");
    let same = match horizon {
        Horizon::Fixed { seconds } => market.and_then(|v| v.parse::<u64>().ok()) == Some(seconds),
        Horizon::UntilStopped { .. } => market.as_deref() == Some(UNTIL_STOPPED),
    };
    if !same {
        return Err("BOOT_IDENTITY_DRIFT");
    }
    let identity = current().map_err(|e| admission_code(&e))?;
    let record =
        admission_record(get, &|| Ok(identity.clone()), binary).map_err(|e| admission_code(&e))?;
    let approved_at = get(ENV_NETWORK_AUDIT_APPROVED_AT)
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or(BOOT_AUDIT_REQUIRED)?;
    if approved_at > now || !record.within_ttl(now) {
        return Err("BOOT_NETWORK_EXPIRED");
    }
    let rolling = match horizon {
        Horizon::Fixed { .. } => None,
        Horizon::UntilStopped { .. } => Some(rolling_admission(
            get,
            MARKET_MODE,
            horizon,
            identity,
            binary().map_err(|e| admission_code(&e))?,
            &|| config().map_err(|_| "BOOT_IDENTITY_DRIFT"),
            now,
        )?),
    };
    // Once: the approval is consumed beside its record before any read (a relaunch of the same
    // job, or its values copied, is refused).
    let path = std::path::PathBuf::from(get(ENV_NETWORK_AUDIT).ok_or(BOOT_AUDIT_REQUIRED)?);
    let consumed = path.with_extension("admitted");
    let mut marker = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&consumed)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(BOOT_ADMISSION_CONSUMED)
        }
        Err(_) => return Err("BOOT_NETWORK_EVIDENCE_INVALID"),
    };
    use std::io::Write;
    marker
        .write_all(format!("pid={} admitted_unix_s={now}\n", std::process::id()).as_bytes())
        .and_then(|()| marker.sync_all())
        .map_err(|_| "BOOT_NETWORK_EVIDENCE_INVALID")?;
    Ok(MarketAdmission { record, rolling })
}

/// DEC-16: the admission of an unbounded run of `mode` (horizon `until-stopped:W:R`): its
/// qualification (`P0_UNTIL_STOPPED_QUALIFICATION`) bound to exactly this run — identity,
/// executables, window, runway — admitted by `RunHorizon::admit`; for the raw capture also the
/// standing approval it autostarts under and the crashed run it continues. Every other mode
/// refuses both (its autostart and its continuation after a crash do not exist).
pub(crate) fn rolling_admission(
    get: &dyn Fn(&str) -> Option<String>,
    mode: &str,
    horizon: darkbot_solana::runtime_config::Horizon,
    identity: AuditedIdentity,
    binary_sha256: String,
    config: &dyn Fn() -> Result<darkbot_solana::runtime_config::RuntimeConfig, &'static str>,
    now: u64,
) -> Result<super::until_stopped::Rolling, &'static str> {
    let darkbot_solana::runtime_config::Horizon::UntilStopped {
        checkpoint_seconds,
        minimum_runway_seconds,
    } = horizon
    else {
        return Err(BOOT_AUDIT_REQUIRED);
    };
    let gated = crate::analytics::measurement::UNTIL_STOPPED_GATED;
    let refuse = |why: String| {
        crate::console::line(format!("{gated}: {why}"));
        gated
    };
    let id = get(ENV_UNTIL_STOPPED_QUALIFICATION)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| refuse("no qualification (P0_UNTIL_STOPPED_QUALIFICATION)".into()))?;
    let project = PathBuf::from(get("P0_PROJECT_DIR").ok_or(BOOT_AUDIT_REQUIRED)?);
    let q = super::until_stopped::get(&project, &id).map_err(refuse)?;
    let binding = super::until_stopped::Binding {
        mode: mode.into(),
        identity,
        binary_sha256,
        checkpoint_seconds,
        minimum_runway_seconds,
    };
    crate::analytics::measurement::RunHorizon::of(horizon)
        .admit(Some(&q), &binding, now)
        .map_err(refuse)?;
    let config = config()?;
    let autostart = get(super::autostart::ENV_AUTOSTART).filter(|v| !v.is_empty());
    let resume = get(super::until_stopped::ENV_RESUME).filter(|v| !v.is_empty());
    // D-5: only the raw capture autostarts (a qualified unbounded run is no permission for an
    // unattended restart of an owner that holds a signer); a crashed run of a mode with a known
    // run layout is continued by an approved start.
    if mode != MARKET_MODE && autostart.is_some() {
        crate::console::line(format!("{gated}: {mode} has no autostart"));
        return Err(gated);
    }
    if resume.is_some() && super::until_stopped_view::layout(mode).is_none() {
        crate::console::line(format!("{gated}: {mode} has no continuation after a crash"));
        return Err(gated);
    }
    // An autostarted capture runs under its standing approval: exactly this run with its
    // qualification and window budget, unexpired, for at most its maximum runtime.
    let (max_runtime_seconds, autostart) = match autostart {
        None => (None, None),
        Some(digest) => {
            let refuse = |why: String| {
                crate::console::line(why);
                "AUTOSTART_NOT_PERMITTED"
            };
            let grant = super::autostart::in_force(&project, &digest).map_err(refuse)?;
            if grant.qualification_id != id {
                return Err(refuse(
                    "AUTOSTART_NOT_PERMITTED: the run names another qualification".into(),
                ));
            }
            grant
                .permits(&binding, &super::autostart::WindowBudget::of(&config), now)
                .map_err(refuse)?;
            (Some(grant.max_runtime_seconds), Some(digest))
        }
    };
    Ok(super::until_stopped::Rolling {
        project,
        binding,
        qualification_id: id,
        config,
        max_runtime_seconds,
        resume,
        autostart,
    })
}

/// The same identity seen from the runtime (X1): `p0ctl` next to the running executable.
pub fn runtime_binary_identity() -> Result<String, String> {
    let runtime = std::env::current_exe().map_err(|_| "BOOT_BINARY")?;
    let ctl = runtime.parent().ok_or("BOOT_BINARY")?.join("p0ctl");
    binary_identity_of(&ctl, &runtime)
}

/// Digest over the control and the runtime executable, in that order.
pub fn binary_identity_of(ctl: &Path, runtime: &Path) -> Result<String, String> {
    let (ctl, runtime) = (ctl.to_path_buf(), runtime.to_path_buf());
    let mut digests = Vec::new();
    for p in [ctl, runtime] {
        digests.extend(
            solana_sdk::hash::hash(&std::fs::read(p).map_err(|_| "BOOT_RUNTIME_BINARY_MISSING")?)
                .to_bytes(),
        );
    }
    Ok(sha(&digests))
}

/// X1 test fixture: a READY network record (BOOT-03..08 PASS, pinned genesis, binary "binary",
/// capture source "logs", 60 s TTL from 1000) over `accounts`, sealed by its readiness token.
#[cfg(test)]
pub(crate) fn network_record_fixture(accounts: Vec<AccountProof>) -> NetworkAudit {
    let check = |id: &str| BootCheck {
        id: id.into(),
        name: "fixture".into(),
        critical: true,
        state: CheckState::Pass,
        expected: None,
        observed: None,
        source: "fixture".into(),
        reason: "fixture".into(),
    };
    let mut n = NetworkAudit {
        audit: BootAudit {
            scope: "network".into(),
            checks: [
                "BOOT-03", "BOOT-04", "BOOT-05", "BOOT-06", "BOOT-07", "BOOT-08",
            ]
            .iter()
            .map(|id| check(id))
            .collect(),
            identity: AuditedIdentity {
                source_head: Some("head".into()),
                source_dirty_sha256: Some("dirty".into()),
                runtime_config_sha256: "config".into(),
                journal_version: journal::VERSION,
                discriminator_table_sha256: "table".into(),
                live_enabled: false,
                submission_enabled: false,
            },
            verdict: AuditVerdict::Ready,
            readiness_token: String::new(),
        },
        observed_at: 1000,
        expires_at: 1060,
        slot: 100,
        genesis: darkbot_project0::protocol::GENESIS_HASH_MAINNET.into(),
        rpc_binding: "rpc".into(),
        wss_binding: "wss".into(),
        binary_sha256: "binary".into(),
        provider_proof_sha256: "proof".into(),
        capture_source: "logs".into(),
        accounts,
        state_hash: "state".into(),
        ttl_seconds: 60,
        ttl_source: "builtin_default".into(),
    };
    n.reseal();
    n
}

/// X1 test fixture: the proof of one account as the network audit records it.
#[cfg(test)]
pub(crate) fn proof_of(
    key: &solana_sdk::pubkey::Pubkey,
    owner: &solana_sdk::pubkey::Pubkey,
    data: &[u8],
    executable: bool,
) -> AccountProof {
    AccountProof {
        address: key.to_string(),
        owner: owner.to_string(),
        data_sha256: sha(data),
        bytes: data.len() as u64,
        executable,
    }
}

#[cfg(test)]
impl NetworkAudit {
    /// Recomputes the readiness token after a fixture edit (a consistent, re-signed record).
    pub(crate) fn reseal(&mut self) {
        self.audit.readiness_token.clear();
        self.audit.readiness_token = sha(&bincode::serialize(self).unwrap());
    }
}

#[cfg(test)]
#[path = "boot_audit_x1_tests.rs"]
mod x1_tests;

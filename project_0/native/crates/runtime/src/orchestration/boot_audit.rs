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

fn git(project: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
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
    let config = bincode::serialize(darkbot_solana::runtime_config::active())
        .map_err(|e| format!("RUNTIME_CONFIG_DIGEST: {e}"))?;
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
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
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
    match (
        super::boot_network::required_reads(c),
        super::boot_network::worst_case_reads(c),
    ) {
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
    let wall = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 2026-10-01T00:00:00Z: a wall clock before this cannot be the host's real time.
    add(
        "BOOT-02",
        "clock",
        true,
        if wall >= 1_790_812_800 { Pass } else { Blocked },
        "OS",
        format!("unix {wall}; monotonic clock is the process Clock"),
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
}
impl payload::Payload for NetworkAudit {
    const KIND: u16 = journal::kind::BOOT_NETWORK;
}
impl NetworkAudit {
    pub fn validate_start(
        &self,
        current: &AuditedIdentity,
        binary: &str,
        now: u64,
        source: &str,
    ) -> Result<(), String> {
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
        if now < self.observed_at
            || now >= self.expires_at
            || self.expires_at.saturating_sub(self.observed_at) > 60
        {
            return Err("BOOT_NETWORK_EXPIRED".into());
        }
        if current != &self.audit.identity
            || binary != self.binary_sha256
            || source != self.capture_source
        {
            return Err("BOOT_NETWORK_TOCTOU".into());
        }
        let mut copy = self.clone();
        copy.audit.readiness_token.clear();
        let actual = sha(&bincode::serialize(&copy).map_err(|_| "BOOT_TOKEN")?);
        if actual != self.audit.readiness_token {
            return Err("BOOT_NETWORK_TOKEN".into());
        }
        Ok(())
    }
}

/// Bind both the control executable and the actual runtime it admits.
pub fn binary_identity() -> Result<String, String> {
    let ctl = std::env::current_exe().map_err(|_| "BOOT_BINARY")?;
    let runtime = ctl.parent().ok_or("BOOT_BINARY")?.join("p0-longtail");
    let mut digests = Vec::new();
    for p in [ctl, runtime] {
        digests.extend(
            solana_sdk::hash::hash(&std::fs::read(p).map_err(|_| "BOOT_RUNTIME_BINARY_MISSING")?)
                .to_bytes(),
        );
    }
    Ok(sha(&digests))
}

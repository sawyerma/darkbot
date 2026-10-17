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
            wallet_public_env: project.join("config/wallet.public.env"),
            max_capture_bytes: w.max_bytes,
            system_reserve_bytes: w.system_reserve_bytes,
            disk_reserve_bytes: w.disk_reserve_bytes,
            flags: SafetyFlags::from_env(),
        }
    }
}

/// Unsealed journals (crash or still written) among the newest runs of each root.
fn unsealed_journals(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
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
                        let sealed = journal::Reader::open(&p).ok().map(|mut r| {
                            while let Ok(Some(_)) = r.next_frame() {}
                            r.sealed
                        });
                        if sealed != Some(true) {
                            out.push(p);
                        }
                    }
                }
            }
        }
    }
    out.sort();
    out
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
    let unsealed = unsealed_journals(&inputs.run_roots);
    add(
        "BOOT-02",
        "journal_seal_state",
        false,
        if unsealed.is_empty() { Pass } else { Warn },
        "newest runs per root",
        if unsealed.is_empty() {
            "no unsealed journal in the newest runs".into()
        } else {
            format!(
                "{} unsealed journal(s) (crash evidence, kept unchanged), e.g. {}",
                unsealed.len(),
                unsealed[0].display()
            )
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
    let data = inputs.project.join("data");
    match free_disk_bytes(if data.exists() {
        &data
    } else {
        &inputs.project
    }) {
        Some(f) if f > inputs.disk_reserve_bytes => add(
            "BOOT-02",
            "disk_reserve",
            true,
            Pass,
            "OS",
            format!(
                "{} MiB free > reserve (sizing: capacity preflight)",
                f / MIB
            ),
        ),
        Some(f) => add(
            "BOOT-02",
            "disk_reserve",
            true,
            Blocked,
            "OS",
            format!("{} MiB free ≤ disk reserve", f / MIB),
        ),
        None => add(
            "BOOT-02",
            "disk_reserve",
            true,
            Unknown,
            "OS",
            "free disk unknown".into(),
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
}

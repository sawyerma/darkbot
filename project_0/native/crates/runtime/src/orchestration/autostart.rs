//! DEC-16: the standing approval for the autostart of an unbounded run ("Dauerfreigabe") —
//! separate from the qualification (a qualified unbounded run is no permission to restart
//! unattended) and from LIVE, which stays disabled. One typed, append-only store of grants and
//! revocations: a grant is bound to the run (mode, operational arguments and horizon, identity
//! with its configuration hash, executables, window and runway), its qualification, provider
//! evidence and genesis pin, its capture source, the window budget, the maximum runtime of each
//! autostarted run and its own expiry. The operator confirms exactly that binding (its digest)
//! before it is recorded; a revocation ends it at once. An autostart still runs the whole boot
//! flow (offline audit, network audit, start) and the capture admits itself under the grant.
use super::until_stopped::{Binding, Qualification};
use crate::analytics::journal::{self, kind};
use crate::analytics::measurement::RunHorizon;
use crate::analytics::payload::Payload;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const STORE: &str = "data/until_stopped/autostart";
/// The digest of the standing approval an autostarted capture runs under.
pub const ENV_AUTOSTART: &str = "P0_UNTIL_STOPPED_AUTOSTART";
/// The reason an autostarted run ends in order at its maximum runtime.
pub const MAX_RUNTIME: &str = "UNTIL_STOPPED_MAX_RUNTIME";

/// The provider budget of one window the grant was given for (the run's configuration).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowBudget {
    pub rpc_rps: u64,
    pub max_requests: u64,
    pub max_total_mib: u64,
}
impl WindowBudget {
    pub fn of(config: &darkbot_solana::runtime_config::RuntimeConfig) -> Self {
        Self {
            rpc_rps: config.limits.rpc_rps,
            max_requests: config.limits.max_requests,
            max_total_mib: config.limits.max_total_mib,
        }
    }
}

/// What a standing approval permits: autostarts of exactly this unbounded run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub binding: Binding,
    pub operational: Vec<String>,
    /// The run's horizon as audited (`until-stopped:W:R`; its text is part of the config hash).
    pub duration: String,
    pub qualification_id: String,
    pub provider_evidence_id: String,
    pub genesis_hash: String,
    pub capture_source: String,
    pub budget: WindowBudget,
    /// The longest an autostarted run runs before it ends in order.
    pub max_runtime_seconds: u64,
    /// The last second the grant permits an autostart.
    pub expires_unix_s: u64,
}

/// The operator's terms of a standing approval.
pub struct Terms {
    pub genesis_hash: String,
    pub capture_source: String,
    pub max_runtime_seconds: u64,
    pub expires_unix_s: u64,
}

impl Grant {
    /// The grant for the unbounded run `current` (`duration`, `operational`) under its
    /// qualification `q` (`qualification_id`) with the window budget `budget`: only a run its
    /// qualification admits now, an autostarted run of at least one window, and an expiry after
    /// now and within its provider evidence's freshness.
    #[allow(clippy::too_many_arguments)]
    pub fn for_run(
        q: &Qualification,
        qualification_id: &str,
        current: &Binding,
        operational: &[String],
        duration: &str,
        budget: WindowBudget,
        terms: Terms,
        now: u64,
    ) -> Result<Self, String> {
        RunHorizon::of(darkbot_solana::runtime_config::horizon(duration)?)
            .admit(Some(q), current, now)
            .map_err(|gaps| format!("AUTOSTART_UNQUALIFIED: {gaps}"))?;
        let (Some(evidence), Some(fresh_until)) =
            (q.evidence_id.clone(), q.evidence_valid_until_unix_s)
        else {
            return Err("AUTOSTART_UNQUALIFIED: no provider evidence".into());
        };
        terms
            .genesis_hash
            .parse::<solana_sdk::hash::Hash>()
            .map_err(|_| format!("AUTOSTART_GENESIS_INVALID: {}", terms.genesis_hash))?;
        if terms.max_runtime_seconds < current.checkpoint_seconds {
            return Err(format!(
                "AUTOSTART_MAX_RUNTIME_BELOW_WINDOW: {} s < window {} s",
                terms.max_runtime_seconds, current.checkpoint_seconds
            ));
        }
        if terms.expires_unix_s <= now {
            return Err("AUTOSTART_EXPIRED: the expiry is not after now".into());
        }
        if terms.expires_unix_s > fresh_until {
            return Err(format!(
                "AUTOSTART_EXPIRY_BEYOND_EVIDENCE: the provider evidence stays fresh until {fresh_until}"
            ));
        }
        Ok(Self {
            binding: current.clone(),
            operational: operational.to_vec(),
            duration: duration.into(),
            qualification_id: qualification_id.into(),
            provider_evidence_id: evidence,
            genesis_hash: terms.genesis_hash,
            capture_source: terms.capture_source,
            budget,
            max_runtime_seconds: terms.max_runtime_seconds,
            expires_unix_s: terms.expires_unix_s,
        })
    }

    /// The digest the operator confirms (16 hex digits over the whole binding).
    pub fn digest(&self) -> Result<String, String> {
        let bytes = bincode::serialize(self).map_err(|e| format!("AUTOSTART_DIGEST: {e}"))?;
        Ok(journal::hex(&solana_sdk::hash::hash(&bytes).to_bytes())[..16].to_string())
    }

    /// Operator text: the binding shown before it is confirmed.
    pub fn terminal(&self) -> Result<String, String> {
        let b = &self.binding;
        Ok(format!(
            "PROJECT 0 AUTOSTART APPROVAL digest={}\n  mode               {}\n  config_sha256      {}\n  binary_sha256      {}\n  horizon            {} (window {} s, runway {} s)\n  operational        {}\n  qualification      {}\n  provider_evidence  {}\n  genesis            {}\n  capture_source     {}\n  budget per window  rpc_rps {} · max_requests {} · max_total_mib {}\n  max_runtime        {} s\n  expires_unix_s     {}\n",
            self.digest()?,
            b.mode,
            b.identity.runtime_config_sha256,
            b.binary_sha256,
            self.duration,
            b.checkpoint_seconds,
            b.minimum_runway_seconds,
            if self.operational.is_empty() {
                "-".to_string()
            } else {
                self.operational.join(" ")
            },
            self.qualification_id,
            self.provider_evidence_id,
            self.genesis_hash,
            self.capture_source,
            self.budget.rpc_rps,
            self.budget.max_requests,
            self.budget.max_total_mib,
            self.max_runtime_seconds,
            self.expires_unix_s
        ))
    }

    /// Whether this grant permits an autostart of `current` (with the window budget `budget`)
    /// at `now`; otherwise every reason.
    pub fn permits(
        &self,
        current: &Binding,
        budget: &WindowBudget,
        now: u64,
    ) -> Result<(), String> {
        let mut gaps = Vec::new();
        if now > self.expires_unix_s {
            gaps.push("expired".to_string());
        }
        if &self.binding != current {
            gaps.push("the run's binding changed".into());
        }
        if &self.budget != budget {
            gaps.push("the window budget changed".into());
        }
        if gaps.is_empty() {
            Ok(())
        } else {
            Err(format!("AUTOSTART_NOT_PERMITTED: {}", gaps.join("; ")))
        }
    }
}

/// One event of the store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    Granted {
        grant: Box<Grant>,
        granted_unix_s: u64,
    },
    Revoked {
        revoked_unix_s: u64,
    },
    /// An autostart under the grant `digest` started the boot flow `flow_id`.
    Used {
        digest: String,
        flow_id: String,
        started_unix_s: u64,
    },
}
impl Payload for Event {
    const KIND: u16 = kind::UNTIL_STOPPED_AUTOSTART;
}

fn path_of(project: &Path, seq: u64) -> PathBuf {
    project.join(STORE).join(format!("{seq:06}.p0j"))
}

/// Every event in order (`seq`, event).
pub fn events(project: &Path) -> Result<Vec<(u64, Event)>, String> {
    let dir = project.join(STORE);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("AUTOSTART_STORE: {e}")),
    };
    let mut seqs = Vec::new();
    for entry in entries {
        let name = entry
            .map_err(|e| format!("AUTOSTART_STORE: {e}"))?
            .file_name();
        let Some(seq) = name
            .to_str()
            .and_then(|n| n.strip_suffix(".p0j"))
            .filter(|n| n.len() == 6 && n.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|n| n.parse::<u64>().ok())
        else {
            continue;
        };
        seqs.push(seq);
    }
    seqs.sort_unstable();
    // The store is a gapless sequence from 1: a missing event fails closed.
    if seqs.iter().enumerate().any(|(i, s)| *s != i as u64 + 1) {
        return Err("AUTOSTART_STORE_GAP".into());
    }
    seqs.into_iter()
        .map(|seq| {
            journal::read_typed_value(&path_of(project, seq))
                .map(|e| (seq, e))
                .map_err(|_| format!("AUTOSTART_EVENT_INVALID: {seq:06}"))
        })
        .collect()
}

/// Appends `event` as the next one (never over an existing event).
pub fn record(project: &Path, event: &Event) -> Result<u64, String> {
    let io = |e: std::io::Error| format!("AUTOSTART_STORE: {e}");
    let seq = events(project)?.len() as u64 + 1;
    let dir = project.join(STORE);
    std::fs::create_dir_all(&dir).map_err(io)?;
    let temp = dir.join(format!(".{seq:06}.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&temp);
    let stored = (|| {
        journal::write_typed_value(&temp, journal::source::UNTIL_STOPPED, event).map_err(io)?;
        std::fs::hard_link(&temp, path_of(project, seq)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                "AUTOSTART_STORE_RACE: another event was recorded first".to_string()
            } else {
                io(e)
            }
        })?;
        std::fs::File::open(&dir)
            .and_then(|d| d.sync_all())
            .map_err(io)
    })();
    let _ = std::fs::remove_file(&temp);
    stored.map(|()| seq)
}

/// The grant in force (`seq`, grant, granted at): the last grant or revocation decides — a
/// revocation ends it; uses never change it.
pub fn current(project: &Path) -> Result<Option<(u64, Grant, u64)>, String> {
    for (seq, event) in events(project)?.into_iter().rev() {
        match event {
            Event::Granted {
                grant,
                granted_unix_s,
            } => return Ok(Some((seq, *grant, granted_unix_s))),
            Event::Revoked { .. } => return Ok(None),
            Event::Used { .. } => {}
        }
    }
    Ok(None)
}

/// The grant in force whose digest is `digest` (an autostart names the grant it runs under).
pub fn in_force(project: &Path, digest: &str) -> Result<Grant, String> {
    let (_, grant, _) =
        current(project)?.ok_or("AUTOSTART_NOT_GRANTED: no standing approval in force")?;
    if grant.digest()? != digest {
        return Err("AUTOSTART_NOT_PERMITTED: another standing approval is in force".into());
    }
    Ok(grant)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::until_stopped::tests::{binding, qualified};

    const GENESIS: &str = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d";

    fn terms(max_runtime_seconds: u64, expires_unix_s: u64) -> Terms {
        Terms {
            genesis_hash: GENESIS.into(),
            capture_source: "logs".into(),
            max_runtime_seconds,
            expires_unix_s,
        }
    }
    fn budget() -> WindowBudget {
        WindowBudget::of(&darkbot_solana::runtime_config::RuntimeConfig::default())
    }
    fn grant() -> Grant {
        let b = binding(3_600, 86_400);
        let q = qualified(&b);
        Grant::for_run(
            &q,
            "0123456789abcdef",
            &b,
            &[],
            "until-stopped:1h:24h",
            budget(),
            terms(86_400, 2_000),
            1_500,
        )
        .unwrap()
    }

    /// DEC-16: a standing approval is given only for a run its qualification admits now, for at
    /// least one window and until at most its provider evidence's freshness.
    #[test]
    fn dec16_a_grant_is_given_only_for_an_admitted_run_within_its_evidence() {
        let b = binding(3_600, 86_400);
        let q = qualified(&b);
        let given = |q: &Qualification, current: &Binding, t: Terms, now: u64| {
            Grant::for_run(
                q,
                "id",
                current,
                &[],
                "until-stopped:1h:24h",
                budget(),
                t,
                now,
            )
        };
        let g = given(&q, &b, terms(3_600, 2_000), 1_500).unwrap();
        assert_eq!(
            (g.provider_evidence_id.as_str(), g.genesis_hash.as_str()),
            ("0123456789abcdef", GENESIS)
        );
        let mut open = q.clone();
        open.capacity.state = super::super::until_stopped::PartState::NotRun;
        let e = given(&open, &b, terms(3_600, 2_000), 1_500).unwrap_err();
        assert!(e.starts_with("AUTOSTART_UNQUALIFIED"), "{e}");
        let mut other = b.clone();
        other.binary_sha256 = "other".into();
        let e = given(&q, &other, terms(3_600, 2_000), 1_500).unwrap_err();
        assert!(e.starts_with("AUTOSTART_UNQUALIFIED"), "{e}");
        let e = given(&q, &b, terms(3_599, 2_000), 1_500).unwrap_err();
        assert!(e.starts_with("AUTOSTART_MAX_RUNTIME_BELOW_WINDOW"), "{e}");
        let e = given(&q, &b, terms(3_600, 1_500), 1_500).unwrap_err();
        assert!(e.starts_with("AUTOSTART_EXPIRED"), "{e}");
        let e = given(&q, &b, terms(3_600, 2_001), 1_500).unwrap_err();
        assert!(e.starts_with("AUTOSTART_EXPIRY_BEYOND_EVIDENCE"), "{e}");
        let mut t = terms(3_600, 2_000);
        t.genesis_hash = "not-a-hash".into();
        let e = given(&q, &b, t, 1_500).unwrap_err();
        assert!(e.starts_with("AUTOSTART_GENESIS_INVALID"), "{e}");
    }

    /// DEC-16: the store is append-only and gapless; the last grant or revocation decides — a
    /// grant is in force until a later revocation, uses do not change it; a gap fails closed.
    #[test]
    fn dec16_the_autostart_store_keeps_every_grant_and_revocation_in_order() {
        let d = std::env::temp_dir().join(format!("p0_autostart_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(current(&d), Ok(None));
        let g = grant();
        let digest = g.digest().unwrap();
        assert!(in_force(&d, &digest)
            .unwrap_err()
            .starts_with("AUTOSTART_NOT_GRANTED"));
        let granted = Event::Granted {
            grant: Box::new(g.clone()),
            granted_unix_s: 1_000,
        };
        assert_eq!(record(&d, &granted), Ok(1));
        assert_eq!(current(&d), Ok(Some((1, g.clone(), 1_000))));
        assert_eq!(in_force(&d, &digest), Ok(g.clone()));
        assert!(in_force(&d, "0000000000000000")
            .unwrap_err()
            .contains("another standing approval"));
        let used = Event::Used {
            digest: digest.clone(),
            flow_id: "f1".into(),
            started_unix_s: 1_100,
        };
        assert_eq!(record(&d, &used), Ok(2));
        assert_eq!(current(&d), Ok(Some((1, g.clone(), 1_000))));
        assert_eq!(
            record(
                &d,
                &Event::Revoked {
                    revoked_unix_s: 2_000
                }
            ),
            Ok(3)
        );
        assert_eq!(current(&d), Ok(None));
        assert!(in_force(&d, &digest)
            .unwrap_err()
            .starts_with("AUTOSTART_NOT_GRANTED"));
        assert_eq!(events(&d).unwrap().len(), 3);
        std::fs::remove_file(path_of(&d, 2)).unwrap();
        assert_eq!(current(&d).unwrap_err(), "AUTOSTART_STORE_GAP");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// DEC-16: a grant permits an autostart only of exactly its run, with its window budget,
    /// through its expiry; its digest names the whole binding.
    #[test]
    fn dec16_a_grant_permits_only_its_bound_run_until_it_expires() {
        let g = grant();
        let (b, budget) = (g.binding.clone(), g.budget.clone());
        assert_eq!(g.permits(&b, &budget, 2_000), Ok(()));
        assert!(g
            .permits(&b, &budget, 2_001)
            .unwrap_err()
            .contains("expired"));
        let mut other = b.clone();
        other.identity.runtime_config_sha256 = "other".into();
        assert!(g
            .permits(&other, &budget, 1)
            .unwrap_err()
            .contains("binding changed"));
        let mut more = budget.clone();
        more.max_requests += 1;
        assert!(g
            .permits(&b, &more, 1)
            .unwrap_err()
            .contains("window budget changed"));
        let digest = g.digest().unwrap();
        assert_eq!(digest.len(), 16);
        for change in 0..4 {
            let mut h = g.clone();
            match change {
                0 => h.max_runtime_seconds += 1,
                1 => h.expires_unix_s += 1,
                2 => h.provider_evidence_id = "x".into(),
                _ => h.operational.push("--x".into()),
            }
            assert_ne!(h.digest().unwrap(), digest, "{change}");
        }
        assert!(g.terminal().unwrap().contains(&digest));
    }
}

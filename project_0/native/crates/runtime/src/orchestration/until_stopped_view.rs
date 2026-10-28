//! DEC-16: what the Capture tab shows of unbounded runs (topic project0.analytics.jobs, beside
//! the boot flow): the standing autostart approval in force and a previewed one awaiting its
//! digest, the newest qualification of the raw capture with its open parts, and the newest
//! unbounded raw capture's windows with its final status once sealed. Values as the stores hold
//! them — expiry and evidence freshness are compared with the viewer's clock, and every native
//! gate decides again at the start; nothing here admits anything.
use super::autostart::{self, Grant};
use super::until_stopped::{self as us, PartState, Qualification, RunRecord, WindowCheck};
use crate::analytics::journal::kind;
use std::path::{Path, PathBuf};

/// A standing approval (in force, or previewed) with its digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantView {
    pub digest: String,
    pub grant: Grant,
    /// When it was granted (its event), respectively previewed.
    pub recorded_unix_s: u64,
}

/// The newest qualification of an unbounded raw capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualificationView {
    pub id: String,
    pub qualification: Qualification,
    /// Every part that did not pass, as `name: STATE · reason`.
    pub open: Vec<String>,
}

/// The verdict of a closed window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastWindow {
    pub verdict: String,
    pub detail: String,
    pub ended_unix_s: u64,
}

/// The newest unbounded raw capture: its admission and its windows so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunView {
    pub run_id: String,
    pub record: RunRecord,
    pub windows: u64,
    pub last: Option<LastWindow>,
    /// The window verdict that halted the run.
    pub halted: Option<String>,
    pub sealed: bool,
    pub complete: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UntilStoppedView {
    pub autostart: Option<GrantView>,
    pub preview: Option<GrantView>,
    pub qualification: Option<QualificationView>,
    pub run: Option<RunView>,
    /// Why a store was not readable (its part is then absent, never a guess).
    pub errors: Vec<String>,
}

fn grant_view(grant: Grant, recorded_unix_s: u64) -> Result<GrantView, String> {
    Ok(GrantView {
        digest: grant.digest()?,
        grant,
        recorded_unix_s,
    })
}

/// Entries of `dir` newest first (modification time, then name).
fn newest_first(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut all: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    all.sort_by(|a, b| b.cmp(a));
    all.into_iter().map(|(_, p)| p).collect()
}

/// The newest qualification of `mode` (the store's content-addressed records, newest written
/// first).
fn newest_qualification(project: &Path, mode: &str) -> Option<QualificationView> {
    for path in newest_first(&project.join(us::STORE)) {
        let Some(id) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".p0j"))
        else {
            continue;
        };
        let Ok(q) = us::get(project, id) else {
            continue;
        };
        if q.binding.mode != mode {
            continue;
        }
        let open = q
            .parts()
            .iter()
            .filter(|(_, p)| p.state != PartState::Pass)
            .map(|(name, p)| format!("{name}: {} · {}", p.state.as_str(), p.reason))
            .collect();
        return Some(QualificationView {
            id: id.to_string(),
            qualification: q,
            open,
        });
    }
    None
}

/// The record journal of a Shadow run in its directory (`P0_RECORD_PATH`, `capture.sh shadow`).
pub const SHADOW_RECORD: &str = "opportunities.p0j";

/// DEC-16: where the unbounded runs of `mode` live below the project and the journal a crashed
/// run leaves unsealed in its directory beside its windows journal; `None`: the mode's runs
/// are never continued after a crash.
pub fn layout(mode: &str) -> Option<(&'static str, &'static str)> {
    match mode {
        super::boot_audit::MARKET_MODE => {
            Some(("data/market", crate::market_observer::MARKET_JOURNAL))
        }
        "shadow" => Some(("data/shadow", SHADOW_RECORD)),
        _ => None,
    }
}

/// DEC-16: the crashed run a new approved start of `binding` with `qualification_id` continues:
/// the newest unbounded run of its mode when it never ended (its windows journal unsealed),
/// never halted and was admitted as exactly this run; otherwise the start is a new run.
pub fn resumable(project: &Path, binding: &us::Binding, qualification_id: &str) -> Option<String> {
    let (root, journal) = layout(&binding.mode)?;
    resumable_in(&project.join(root), journal, binding, qualification_id)
}

/// [`resumable`] among the runs below `root`, whose crash leaves `journal` unsealed.
pub fn resumable_in(
    root: &Path,
    journal: &str,
    binding: &us::Binding,
    qualification_id: &str,
) -> Option<String> {
    newest_run(root)
        .filter(|r| {
            !r.sealed
                && r.halted.is_none()
                && &r.record.binding == binding
                && r.record.qualification_id == qualification_id
                && unsealed(&root.join(&r.run_id).join(journal))
        })
        .map(|r| r.run_id)
}

/// Whether the chain at `first` exists and was never sealed (a capture that ended sealed it).
fn unsealed(first: &Path) -> bool {
    let Ok(mut r) = crate::storage::segments::ChainReader::open(first) else {
        return false;
    };
    while let Ok(Some(_)) = r.next_frame() {}
    !r.sealed
}

/// The newest run under `root` with a windows journal, read as far as it is written.
fn newest_run(root: &Path) -> Option<RunView> {
    for dir in newest_first(root) {
        let first = dir.join(us::WINDOWS_JOURNAL);
        let Ok(mut r) = crate::storage::segments::ChainReader::open(&first) else {
            continue;
        };
        let (mut record, mut windows, mut last) = (None, 0, None);
        // A running capture appends while this reads: what is readable so far.
        while let Ok(Some(f)) = r.next_frame() {
            match f.kind {
                kind::UNTIL_STOPPED_RUN if record.is_none() => record = f.typed().ok(),
                kind::UNTIL_STOPPED_WINDOW => {
                    if let Ok(w) = f.typed::<WindowCheck>() {
                        windows += 1;
                        last = Some(LastWindow {
                            verdict: w.verdict,
                            detail: w.detail,
                            ended_unix_s: w.ended_unix_s,
                        });
                    }
                }
                _ => {}
            }
        }
        let record = record?;
        let halted = last
            .as_ref()
            .filter(|w| us::halts(&w.verdict))
            .map(|w| w.verdict.clone());
        return Some(RunView {
            run_id: dir.file_name()?.to_string_lossy().into_owned(),
            record,
            windows,
            last,
            halted,
            sealed: r.sealed,
            complete: r.complete,
        });
    }
    None
}

/// The projection of `project`'s unbounded raw captures.
pub fn projection(project: &Path) -> UntilStoppedView {
    let mut v = UntilStoppedView::default();
    match autostart::current(project) {
        Ok(current) => {
            v.autostart = current.and_then(|(_, g, at)| match grant_view(g, at) {
                Ok(view) => Some(view),
                Err(e) => {
                    v.errors.push(e);
                    None
                }
            })
        }
        Err(e) => v.errors.push(e),
    }
    match autostart::preview(project) {
        Ok(p) => {
            v.preview = p.and_then(|p| match grant_view(p.grant, p.previewed_unix_s) {
                Ok(view) => Some(view),
                Err(e) => {
                    v.errors.push(e);
                    None
                }
            })
        }
        Err(e) => v.errors.push(e),
    }
    v.qualification = newest_qualification(project, super::boot_audit::MARKET_MODE);
    v.run = newest_run(&project.join("data/market"));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::until_stopped::tests::{binding, qualified};

    /// DEC-16: the projection shows the store as it is — the grant in force and a preview with
    /// their digests, the newest raw-capture qualification with its open parts, and the newest
    /// unbounded run's windows, its halt and its seal.
    #[test]
    fn dec16_the_projection_shows_grant_preview_qualification_and_run_as_stored() {
        let d = std::env::temp_dir().join(format!("p0_us_view_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(projection(&d), UntilStoppedView::default());
        let mut b = binding(60, 120);
        b.mode = "market".into();
        let mut q = qualified(&b);
        q.capacity = us::Part {
            state: PartState::NotRun,
            reason: "no runway measurement".into(),
        };
        let id = us::put(&d, &q).unwrap();
        let mut econ = qualified(&binding(60, 120));
        econ.binding.mode = "econ4h".into();
        econ.qualified_at_unix_s += 1;
        us::put(&d, &econ).unwrap();
        let grant = Grant {
            binding: b.clone(),
            operational: vec![],
            duration: "until-stopped:1m:2m".into(),
            qualification_id: id.clone(),
            provider_evidence_id: "0123456789abcdef".into(),
            genesis_hash: "g".into(),
            capture_source: "logs".into(),
            budget: autostart::WindowBudget::of(
                &darkbot_solana::runtime_config::RuntimeConfig::default(),
            ),
            max_runtime_seconds: 600,
            expires_unix_s: 9_000,
        };
        autostart::put_preview(
            &d,
            &autostart::Preview {
                grant: grant.clone(),
                previewed_unix_s: 1_000,
            },
        )
        .unwrap();
        let run = d.join("data/market/r1");
        std::fs::create_dir_all(&run).unwrap();
        let mut w = us::RollingWindows::start(&run, b.clone(), &id, 1_000, 0).unwrap();
        let v = projection(&d);
        let qv = v.qualification.unwrap();
        assert_eq!(
            (qv.id.as_str(), qv.qualification.binding.mode.as_str()),
            (id.as_str(), "market")
        );
        assert_eq!(
            qv.open,
            vec!["capacity: NOT_RUN · no runway measurement".to_string()]
        );
        let p = v.preview.unwrap();
        assert_eq!(
            (p.digest, p.recorded_unix_s),
            (grant.digest().unwrap(), 1_000)
        );
        assert!(v.autostart.is_none());
        let r = v.run.unwrap();
        assert_eq!((r.run_id.as_str(), r.windows, r.sealed), ("r1", 0, false));
        assert_eq!(r.record.qualification_id, id);
        // Never ended, never halted, admitted as exactly this run: a start continues it — not
        // without its market journal, nor once that was sealed.
        assert_eq!(resumable(&d, &b, &id), None, "no market journal");
        let journal = run.join(crate::market_observer::MARKET_JOURNAL);
        let mut sealed = crate::storage::segments::SegmentedWriter::create(&journal, 0).unwrap();
        sealed.seal(false).unwrap();
        assert_eq!(resumable(&d, &b, &id), None, "a sealed market journal");
        std::fs::remove_file(&journal).unwrap();
        drop(crate::storage::segments::SegmentedWriter::create(&journal, 0).unwrap());
        assert_eq!(resumable(&d, &b, &id), Some("r1".into()));
        assert_eq!(resumable(&d, &b, "fedcba9876543210"), None);
        let mut other = b.clone();
        other.binary_sha256 = "other".into();
        assert_eq!(resumable(&d, &other, &id), None);
        // Another mode's runs live elsewhere: a Shadow binding never continues a raw capture.
        let mut shadow = b.clone();
        shadow.mode = "shadow".into();
        assert_eq!(resumable(&d, &shadow, &id), None);
        // Granted: in force; a closed window that halted, then the seal.
        autostart::record(
            &d,
            &autostart::Event::Granted {
                grant: Box::new(grant.clone()),
                granted_unix_s: 1_100,
            },
        )
        .unwrap();
        autostart::clear_preview(&d).unwrap();
        let empty = d.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let config = darkbot_solana::runtime_config::RuntimeConfig::default();
        let check = w
            .close(
                &config,
                "logs",
                us::Measured {
                    now: 1_060,
                    bytes: 0,
                    run_volume: &empty,
                    environment: crate::storage::preflight::Environment::default(),
                    provider: darkbot_solana::provider::WindowUsage {
                        requests: 0,
                        bytes: 0,
                    },
                    evidence: None,
                    sealed: None,
                },
            )
            .unwrap();
        assert_ne!(check.verdict, us::CONTINUE);
        assert_eq!(
            resumable(&d, &b, &id),
            None,
            "a halted run is never continued"
        );
        w.finish(false).unwrap();
        let v = projection(&d);
        let a = v.autostart.unwrap();
        assert_eq!(
            (a.digest, a.recorded_unix_s),
            (grant.digest().unwrap(), 1_100)
        );
        assert!(v.preview.is_none());
        let r = v.run.unwrap();
        assert_eq!((r.windows, r.sealed, r.complete), (1, true, false));
        assert_eq!(r.halted.as_deref(), Some(check.verdict.as_str()));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// DEC-16 (stage 3): a crashed unbounded Shadow run is continued by a start of exactly its
    /// binding and qualification while its record journal is unsealed; an econ4h run is never
    /// continued (no layout yet), nor a run of another mode.
    #[test]
    fn dec16_resumable_continues_only_an_unsealed_unhalted_run_of_its_mode() {
        let d = std::env::temp_dir().join(format!("p0_us_resumable_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let mut b = binding(60, 120);
        b.mode = "shadow".into();
        let id = "0123456789abcdef";
        let run = d.join("data/shadow/s1");
        std::fs::create_dir_all(&run).unwrap();
        let w = us::RollingWindows::start(&run, b.clone(), id, 1_000, 0).unwrap();
        assert_eq!(resumable(&d, &b, id), None, "no record journal");
        drop(
            crate::storage::segments::SegmentedWriter::create(&run.join(SHADOW_RECORD), 0).unwrap(),
        );
        assert_eq!(resumable(&d, &b, id), Some("s1".into()));
        let mut market = b.clone();
        market.mode = "market".into();
        assert_eq!(resumable(&d, &market, id), None, "another mode's runs");
        let mut econ = b.clone();
        econ.mode = "econ4h".into();
        assert_eq!(resumable(&d, &econ, id), None);
        assert_eq!(layout("econ4h"), None);
        w.finish(true).unwrap();
        assert_eq!(
            resumable(&d, &b, id),
            None,
            "a run that ended is never continued"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}

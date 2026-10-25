//! Run directory layout of an econ4h capture. V1 keeps every historical file name at the run
//! root; V2 is `manifest.json`, `config.json`, `capture/`, `snapshots/`, `verification/`,
//! `reports/` and `logs/`. The Python contract `operations/capture/run_store.py` (ARTIFACTS) maps the same
//! logical artifacts; both tables must stay identical.
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunLayout {
    V1,
    V2,
}
impl RunLayout {
    /// `P0_RUN_LAYOUT` unset or `v1` keeps the historical layout; only the launcher selects `v2`.
    pub fn from_lookup(get: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        match get("P0_RUN_LAYOUT").as_deref() {
            None | Some("v1") => Ok(Self::V1),
            Some("v2") => Ok(Self::V2),
            Some(_) => Err("invalid P0_RUN_LAYOUT".into()),
        }
    }
    pub fn tag(self) -> &'static str {
        match self {
            Self::V1 => "Project0/RunLayout/1",
            Self::V2 => "Project0/RunLayout/2",
        }
    }
    /// Standalone C0-MO runs have no pre-capture manifest and therefore stay V1.
    pub fn standalone_market(get: &dyn Fn(&str) -> Option<String>) -> Result<(), String> {
        match Self::from_lookup(get)? {
            Self::V1 => Ok(()),
            Self::V2 => Err("P0_RUN_LAYOUT=v2 is only supported for econ4h runs".into()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunPaths {
    pub root: PathBuf,
    pub layout: RunLayout,
}
impl RunPaths {
    pub fn new(root: &Path, layout: RunLayout) -> Self {
        Self {
            root: root.to_path_buf(),
            layout,
        }
    }
    fn under(&self, dir: &str, name: &str) -> PathBuf {
        match self.layout {
            RunLayout::V1 => self.root.join(name),
            RunLayout::V2 => self.root.join(dir).join(name),
        }
    }
    pub fn manifest(&self) -> PathBuf {
        match self.layout {
            RunLayout::V1 => self.root.join("measurement_manifest.json"),
            RunLayout::V2 => self.root.join("manifest.json"),
        }
    }
    pub fn phases(&self) -> PathBuf {
        self.capture("measurement_phases.jsonl")
    }
    /// Raw capture streams: shadow evaluations/offsets, phase journal, market data.
    pub fn capture(&self, name: &str) -> PathBuf {
        self.under("capture", name)
    }
    /// Derived capture-time results: hourly snapshots, final economics, canonical join.
    pub fn snapshot(&self, name: &str) -> PathBuf {
        self.under("snapshots", name)
    }
    pub fn market_dir(&self) -> PathBuf {
        self.capture("market")
    }
    pub fn create(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        if self.layout == RunLayout::V2 {
            std::fs::create_dir_all(self.root.join("capture"))?;
            std::fs::create_dir_all(self.root.join("snapshots"))?;
        }
        Ok(())
    }
}

/// The run roots of a project directory (`p0ctl`, the analytics job controller and its
/// projection read the same ones).
pub const RUN_ROOTS: [&str; 4] = ["data/runs", "data/econ4h", "data/market", "data/shadow"];

/// A run directory given as a path (it contains `/`), or the one run named `run_id` under the
/// roots of `project` (`None`: missing or ambiguous). A bare name is never resolved against the
/// working directory.
pub fn find_run(project: &Path, run_id: &str) -> Option<PathBuf> {
    if run_id.contains('/') {
        let direct = Path::new(run_id);
        return direct.is_dir().then(|| direct.to_path_buf());
    }
    let found: Vec<_> = RUN_ROOTS
        .iter()
        .map(|r| project.join(r).join(run_id))
        .filter(|p| p.is_dir())
        .collect();
    (found.len() == 1).then(|| found[0].clone())
}

/// One run directory under a project's run roots.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RunDir {
    pub run_id: String,
    pub root: &'static str,
    pub path: PathBuf,
}

/// Every run directory under the roots of `project`, by id, then root (unreadable roots are
/// skipped).
pub fn runs(project: &Path) -> Vec<RunDir> {
    let mut runs: Vec<RunDir> = RUN_ROOTS
        .iter()
        .filter_map(|root| Some((root, std::fs::read_dir(project.join(root)).ok()?)))
        .flat_map(|(root, entries)| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(move |e| RunDir {
                    run_id: e.file_name().to_string_lossy().into_owned(),
                    root,
                    path: e.path(),
                })
        })
        .collect();
    runs.sort();
    runs
}

/// A run with native records (a typed run manifest).
pub fn native_run(run: &Path) -> bool {
    run.join("capture/run_manifest.p0j").is_file() || run.join("run_manifest.p0j").is_file()
}

/// The native readers read layout V2 (`capture/…`). A native run in layout V1 (its typed
/// manifest at the run root) has no native reader and is refused with its own code, never
/// misread as unfinished; new econ4h runs are always written in layout V2.
pub fn native_layout(run: &Path) -> Result<(), String> {
    if run.join("run_manifest.p0j").is_file() && !run.join("capture/run_manifest.p0j").is_file() {
        return Err("NATIVE_LAYOUT_V1_UNSUPPORTED: a native run in layout V1 has no native reader (new econ4h runs are layout V2)".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Review M5: a bare run id is resolved under the project's run roots only, never against
    /// the working directory (a directory `src` exists in the test's working directory).
    #[test]
    fn a_bare_run_id_never_resolves_against_the_working_directory() {
        let project = std::env::temp_dir().join(format!("p0_runs_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        std::fs::create_dir_all(project.join("data/econ4h/r1")).unwrap();
        assert!(std::path::Path::new("src").is_dir());
        assert_eq!(super::find_run(&project, "src"), None);
        assert_eq!(
            super::find_run(&project, "r1"),
            Some(project.join("data/econ4h/r1"))
        );
        let direct = project.join("data/econ4h/r1").display().to_string();
        assert_eq!(
            super::find_run(&project, &direct),
            Some(direct.clone().into())
        );
        let _ = std::fs::remove_dir_all(&project);
    }

    use super::*;

    #[test]
    fn layout_selection_fails_closed() {
        let env = |v: Option<&'static str>| {
            move |k: &str| {
                (k == "P0_RUN_LAYOUT")
                    .then_some(v)
                    .flatten()
                    .map(String::from)
            }
        };
        assert_eq!(RunLayout::from_lookup(&env(None)), Ok(RunLayout::V1));
        assert_eq!(RunLayout::from_lookup(&env(Some("v1"))), Ok(RunLayout::V1));
        assert_eq!(RunLayout::from_lookup(&env(Some("v2"))), Ok(RunLayout::V2));
        for bad in ["", "2", "V2", "v3"] {
            assert!(RunLayout::from_lookup(&env(Some(bad))).is_err(), "{bad}");
        }
        assert_eq!(RunLayout::V2.tag(), "Project0/RunLayout/2");
        assert!(RunLayout::standalone_market(&env(None)).is_ok());
        assert!(RunLayout::standalone_market(&env(Some("v2"))).is_err());
        assert!(RunLayout::standalone_market(&env(Some("x"))).is_err());
    }

    #[test]
    fn artifact_paths_match_the_python_run_store_table() {
        let root = Path::new("/r");
        let rel = |p: PathBuf| p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
        for (layout, expected) in [
            (
                RunLayout::V1,
                [
                    "measurement_manifest.json",
                    "measurement_phases.jsonl",
                    "market",
                    "shadow_evaluations.jsonl",
                    "shadow_offsets.bin",
                    "hour_01.json",
                    "economics_final.json",
                    "opportunity_economics.jsonl",
                    "canonical_market_shadow.jsonl",
                ],
            ),
            (
                RunLayout::V2,
                [
                    "manifest.json",
                    "capture/measurement_phases.jsonl",
                    "capture/market",
                    "capture/shadow_evaluations.jsonl",
                    "capture/shadow_offsets.bin",
                    "snapshots/hour_01.json",
                    "snapshots/economics_final.json",
                    "snapshots/opportunity_economics.jsonl",
                    "snapshots/canonical_market_shadow.jsonl",
                ],
            ),
        ] {
            let p = RunPaths::new(root, layout);
            let actual = [
                rel(p.manifest()),
                rel(p.phases()),
                rel(p.market_dir()),
                rel(p.capture("shadow_evaluations.jsonl")),
                rel(p.capture("shadow_offsets.bin")),
                rel(p.snapshot("hour_01.json")),
                rel(p.snapshot("economics_final.json")),
                rel(p.snapshot("opportunity_economics.jsonl")),
                rel(p.snapshot("canonical_market_shadow.jsonl")),
            ];
            assert_eq!(actual, expected);
        }
    }
}

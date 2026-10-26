//! Read-only check of the historical views of a run against its binary journals (PERSIST-04,
//! JFI-03: an isolated offline decoder). An existing view must be byte-identical to its render;
//! nothing is ever written (P0-PROD-CTRL-02: no JSON view of a journal is produced). DEC-09: the
//! renders and the check exist only in the `historical` build; the standard build knows the
//! layout and the view paths (the strict driver's derived-file exclusion).
#[cfg(feature = "historical")]
use super::journal::MANIFEST;
use super::journal::{self, kind};
use crate::run_layout::{RunLayout, RunPaths};
#[cfg(feature = "historical")]
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// One compact JSON value per line.
    Lines,
    /// The newest matching frame as pretty JSON plus newline (`econ::atomic_json`).
    Pretty,
}
#[derive(Clone, Debug)]
pub struct View {
    pub journal: PathBuf,
    pub kind: u16,
    /// For `ECON_SNAPSHOT`: the hour index whose newest frame is the view.
    pub index: Option<u64>,
    pub output: PathBuf,
    pub shape: Shape,
    /// Views assembled from several typed records instead of one frame kind.
    pub special: Option<Special>,
}
/// Pre-capture configuration documents of the pinned run store, rendered from typed records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Special {
    /// `config.json` of a layout-V2 run.
    RunConfig,
    /// One `<part>.json` of a layout-V1 run.
    ConfigPart(&'static str),
}

/// Layout of an existing run: from its typed run manifest; historical runs without one are
/// decided like the Python run store (V2 only with its `manifest.json` tag).
pub fn detect(root: &Path) -> Result<RunPaths, String> {
    use super::measurement::{RunManifest, RUN_MANIFEST_JOURNAL};
    for layout in [RunLayout::V2, RunLayout::V1] {
        let paths = RunPaths::new(root, layout);
        let typed = paths.capture(RUN_MANIFEST_JOURNAL);
        if typed.exists() {
            let m: RunManifest = journal::read_typed_value(&typed)
                .map_err(|e| format!("RUN_MANIFEST_INVALID: {e}"))?;
            let expected = (layout == RunLayout::V2).then(|| layout.tag().to_string());
            if m.layout != expected {
                return Err("RUN_LAYOUT_INVALID".into());
            }
            return Ok(paths);
        }
    }
    // Historical runs (before the typed manifest) are recognized like the pinned run store: V2
    // only with the `layout` tag of their pinned `manifest.json` (read-only, JFI-03; DEC-09: the
    // `historical` build only).
    let manifest = root.join("manifest.json");
    if !manifest.exists() {
        return Ok(RunPaths::new(root, RunLayout::V1));
    }
    historical_layout(root, &manifest)
}
#[cfg(feature = "historical")]
fn historical_layout(root: &Path, manifest: &Path) -> Result<RunPaths, String> {
    #[derive(serde::Deserialize)]
    struct HistoricalManifest {
        layout: Option<String>,
    }
    let historical: HistoricalManifest = serde_json::from_slice(
        &std::fs::read(manifest).map_err(|e| format!("RUN_LAYOUT_INVALID: {e}"))?,
    )
    .map_err(|_| "RUN_LAYOUT_INVALID".to_string())?;
    match historical.layout.as_deref() {
        Some(tag) if tag == RunLayout::V2.tag() => Ok(RunPaths::new(root, RunLayout::V2)),
        _ => Err("RUN_LAYOUT_UNSUPPORTED".into()),
    }
}
#[cfg(not(feature = "historical"))]
fn historical_layout(_: &Path, _: &Path) -> Result<RunPaths, String> {
    Err(HISTORICAL_READER_NOT_BUILT.into())
}

/// DEC-09: what the standard build answers where only a historical JSON reader could read on.
pub const HISTORICAL_READER_NOT_BUILT: &str =
    "HISTORICAL_READER_NOT_BUILT: a historical JSON source is read only by the build with --features historical";

/// Every view the pinned readers consume, with the journal it is rendered from.
pub fn views(paths: &RunPaths) -> Result<Vec<View>, String> {
    use super::measurement::{PLAN_JOURNAL, RUN_MANIFEST_JOURNAL};
    let view = |journal: &Path, kind, output: PathBuf, shape| View {
        journal: journal.to_path_buf(),
        kind,
        index: None,
        output,
        shape,
        special: None,
    };
    let lines = |journal: &Path, kind, output: PathBuf| view(journal, kind, output, Shape::Lines);
    let mut views = vec![
        view(
            &paths.capture(RUN_MANIFEST_JOURNAL),
            kind::RUN_MANIFEST,
            paths.manifest(),
            Shape::Pretty,
        ),
        lines(
            &journal::binary_path(&paths.phases()),
            kind::MEASUREMENT_PHASE,
            paths.phases(),
        ),
    ];
    let special = |journal: PathBuf, output: PathBuf, special| View {
        journal,
        kind: 0,
        index: None,
        output,
        shape: Shape::Pretty,
        special: Some(special),
    };
    match paths.layout {
        RunLayout::V2 => views.push(special(
            paths.capture(PLAN_JOURNAL),
            paths.root.join("config.json"),
            Special::RunConfig,
        )),
        RunLayout::V1 => {
            for (part, journal) in [
                ("measurement_plan", PLAN_JOURNAL),
                (
                    "storage_preflight",
                    crate::storage::preflight::REPORT_JOURNAL,
                ),
                ("candidate_manifest", RUN_MANIFEST_JOURNAL),
            ] {
                views.push(special(
                    paths.capture(journal),
                    paths.root.join(format!("{part}.json")),
                    Special::ConfigPart(part),
                ));
            }
        }
    }
    // econ4h runs keep the market capture in `market/`; standalone C0-MO runs at the root.
    let market = [paths.market_dir(), paths.root.clone()]
        .into_iter()
        .filter(|dir| dir.join(crate::market::observer::MARKET_JOURNAL).exists())
        .collect::<Vec<_>>();
    if market.len() > 1 {
        return Err("MARKET_JOURNAL_AMBIGUOUS".into());
    }
    for dir in market {
        let journal = dir.join(crate::market::observer::MARKET_JOURNAL);
        for (kind, name) in crate::market::observer::MARKET_VIEWS {
            views.push(lines(&journal, kind, dir.join(name)));
        }
        views.push(view(
            &dir.join(crate::market::observer::MARKET_SUMMARY_JOURNAL),
            kind::MARKET_SUMMARY,
            dir.join("summary.json"),
            Shape::Pretty,
        ));
    }
    let reports = paths.snapshot(super::econ::REPORT_JOURNAL);
    views.push(lines(
        &reports,
        kind::ECON_CANONICAL,
        paths.snapshot("canonical_market_shadow.jsonl"),
    ));
    views.push(lines(
        &reports,
        kind::ECON_OPPORTUNITY,
        paths.snapshot("opportunity_economics.jsonl"),
    ));
    for index in 1..=super::measurement::SNAPSHOT_COUNT as u64 {
        views.push(View {
            journal: reports.clone(),
            kind: kind::ECON_SNAPSHOT,
            index: Some(index),
            output: paths.snapshot(&format!("hour_{index:02}.json")),
            shape: Shape::Pretty,
            special: None,
        });
    }
    views.push(View {
        journal: reports,
        kind: kind::ECON_FINAL,
        index: None,
        output: paths.snapshot("economics_final.json"),
        shape: Shape::Pretty,
        special: None,
    });
    Ok(views)
}

#[cfg(feature = "historical")]
/// Deterministic bytes of one view; `None` when a pretty view has no frame yet.
/// `config.json` (V2) or one config part (V1) of the pinned run store, from typed records:
/// the plan journal, the capacity report and the run manifest beside it. Parts the native
/// launcher has no record for are `null` (optional in the run store contract).
fn render_config(view: &View, special: Special) -> io::Result<Option<Vec<u8>>> {
    use super::measurement::{MeasurementPlan, RunManifest, PLAN_JOURNAL, RUN_MANIFEST_JOURNAL};
    let dir = view.journal.parent().unwrap_or(Path::new("."));
    let plan = dir.join(PLAN_JOURNAL);
    let capacity = dir.join(crate::storage::preflight::REPORT_JOURNAL);
    let manifest = dir.join(RUN_MANIFEST_JOURNAL);
    if !(plan.exists() && capacity.exists() && manifest.exists()) {
        return Ok(None);
    }
    let value = |e: serde_json::Error| io::Error::other(e);
    let m: RunManifest = journal::read_typed_value(&manifest)?;
    let p: MeasurementPlan = journal::read_typed_value(&plan)?;
    let mut parts = serde_json::Map::new();
    parts.insert(
        "measurement_plan".into(),
        serde_json::to_value(&p).map_err(value)?,
    );
    parts.insert(
        "storage_preflight".into(),
        serde_json::to_value(storage_preflight_render(&capacity)?).map_err(value)?,
    );
    parts.insert(
        "candidate_manifest".into(),
        serde_json::json!({"head": m.head, "binary_sha256": m.binary_sha256,
            "source_sha256": m.source_sha256, "live_enabled": m.live_enabled,
            "submission_enabled": m.submission_enabled}),
    );
    let doc = match special {
        Special::ConfigPart(part) => parts.remove(part).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "RUN_CONFIG_PART_UNKNOWN")
        })?,
        Special::RunConfig => {
            let mut doc = serde_json::Map::new();
            doc.insert("schema".into(), "Project0/RunConfig/1".into());
            doc.insert("layout".into(), RunLayout::V2.tag().into());
            doc.insert("run_id".into(), m.run_id.clone().into());
            for part in [
                "measurement_plan",
                "measurement_config",
                "operational_config",
                "verification_preflight",
                "storage_preflight",
                "candidate_manifest",
            ] {
                doc.insert(
                    part.into(),
                    parts.remove(part).unwrap_or(serde_json::Value::Null),
                );
            }
            serde_json::Value::Object(doc)
        }
    };
    let mut text = serde_json::to_vec_pretty(&doc).map_err(value)?;
    text.push(b'\n');
    Ok(Some(text))
}

#[cfg(feature = "historical")]
pub fn render(view: &View) -> io::Result<Option<Vec<u8>>> {
    if let Some(special) = view.special {
        return render_config(view, special);
    }
    // SK-07: a segmented journal renders across its verified chain.
    let mut reader = crate::storage::segments::ChainReader::open(&view.journal)?;
    let mut out = Vec::new();
    let mut newest = None;
    while let Some(frame) = reader.next_frame()? {
        if frame.kind == MANIFEST || frame.kind != view.kind {
            continue;
        }
        // Historical value trees and typed market frames have views (explicit offline export).
        let value = super::payload::legacy_view(&frame)?;
        match view.shape {
            Shape::Lines => {
                serde_json::to_writer(&mut out, &value).map_err(io::Error::other)?;
                out.push(b'\n');
            }
            Shape::Pretty => match view.index {
                Some(index) if value["index"].as_u64() == Some(index) => {
                    newest = Some(value["report"].clone())
                }
                Some(_) => {}
                None => newest = Some(value),
            },
        }
    }
    Ok(match view.shape {
        Shape::Lines => Some(out),
        Shape::Pretty => match newest {
            Some(value) => {
                let mut text = serde_json::to_vec_pretty(&value).map_err(io::Error::other)?;
                text.push(b'\n');
                Some(text)
            }
            None => None,
        },
    })
}

#[cfg(feature = "historical")]
/// Outcome per view: `MATCHES`, `MISSING` (no view file) or `NO_FRAME`. Any existing view that
/// differs from its render is an error; nothing is written.
pub fn check(paths: &RunPaths) -> Result<Vec<(PathBuf, &'static str)>, String> {
    let mut outcome = Vec::new();
    for view in views(paths)? {
        if !view.journal.exists() {
            continue;
        }
        let bytes = render(&view)
            .map_err(|e| format!("JOURNAL_INVALID {}: {e}", view.journal.display()))?;
        let status = match (bytes, view.output.exists()) {
            (None, false) => "NO_FRAME",
            (None, true) => return Err(format!("RENDER_WITHOUT_FRAME {}", view.output.display())),
            (Some(bytes), true) => {
                let existing = std::fs::read(&view.output).map_err(|e| e.to_string())?;
                if existing != bytes {
                    return Err(format!("RENDER_MISMATCH {}", view.output.display()));
                }
                "MATCHES"
            }
            (Some(_), false) => "MISSING",
        };
        outcome.push((view.output, status));
    }
    Ok(outcome)
}

#[cfg(feature = "historical")]
/// `storage_preflight` part of a historical run configuration view, derived from the native
/// capacity report (read-only check of historical views only).
#[derive(serde::Serialize)]
struct StoragePreflightView<'a> {
    schema: &'static str,
    native_report: &'a str,
    native_report_sha256: String,
    duration_seconds: u64,
    verdict: &'static str,
    start_allowed: bool,
    free_bytes: Option<u64>,
    required_bytes: Option<u64>,
}

#[cfg(feature = "historical")]
/// The `storage_preflight` part of a sealed capacity report.
fn storage_preflight_render(report: &Path) -> io::Result<StoragePreflightView<'static>> {
    use crate::storage::preflight::CapacityReport;
    let r: CapacityReport = journal::read_typed_value(report)?;
    Ok(StoragePreflightView {
        schema: "Project0/StoragePreflightRender/1",
        native_report: crate::storage::preflight::REPORT_JOURNAL,
        native_report_sha256: journal::hex(&journal::file_sha256(report)?),
        duration_seconds: r.request.duration_s,
        verdict: r.verdict.as_str(),
        start_allowed: r.start_allowed,
        free_bytes: r.environment.free_disk_bytes,
        required_bytes: r.disk_need_bytes,
    })
}

//! Legacy views of a run, rendered from its binary journals (PERSIST-04). The pinned readers
//! keep reading these files; the journals stay primary. A view is created only if missing and
//! never overwritten; an existing view must be byte-identical to its render.
use super::journal::{self, kind, Reader, MANIFEST};
use crate::run_layout::{RunLayout, RunPaths};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// One compact JSON value per line (`journal::export`).
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
}

/// Layout of an existing run, decided like the Python run store: V2 only with its manifest tag.
pub fn detect(root: &Path) -> Result<RunPaths, String> {
    let manifest = root.join("manifest.json");
    if !manifest.exists() {
        return Ok(RunPaths::new(root, RunLayout::V1));
    }
    let value: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&manifest).map_err(|e| format!("RUN_LAYOUT_INVALID: {e}"))?,
    )
    .map_err(|_| "RUN_LAYOUT_INVALID".to_string())?;
    match value["layout"].as_str() {
        Some(tag) if tag == RunLayout::V2.tag() => Ok(RunPaths::new(root, RunLayout::V2)),
        _ => Err("RUN_LAYOUT_UNSUPPORTED".into()),
    }
}

/// Every view the pinned readers consume, with the journal it is rendered from.
pub fn views(paths: &RunPaths) -> Result<Vec<View>, String> {
    let lines = |journal: &Path, kind, output: PathBuf| View {
        journal: journal.to_path_buf(),
        kind,
        index: None,
        output,
        shape: Shape::Lines,
    };
    let mut views = vec![lines(
        &journal::binary_path(&paths.phases()),
        kind::MEASUREMENT_PHASE,
        paths.phases(),
    )];
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
        });
    }
    views.push(View {
        journal: reports,
        kind: kind::ECON_FINAL,
        index: None,
        output: paths.snapshot("economics_final.json"),
        shape: Shape::Pretty,
    });
    Ok(views)
}

/// Deterministic bytes of one view; `None` when a pretty view has no frame yet.
pub fn render(view: &View) -> io::Result<Option<Vec<u8>>> {
    let mut reader = Reader::open(&view.journal)?;
    let mut out = Vec::new();
    let mut newest = None;
    while let Some(frame) = reader.next_frame()? {
        if frame.kind == MANIFEST || frame.kind != view.kind {
            continue;
        }
        // Views exist only for historical value-tree kinds (schema 1).
        let value = frame.legacy_value()?;
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

/// Outcome per view: `MATCHES`, `CREATED`, `MISSING` (not created) or `NO_FRAME`.
/// Any existing view that differs from its render is an error; nothing is ever overwritten.
pub fn ensure(paths: &RunPaths, create: bool) -> Result<Vec<(PathBuf, &'static str)>, String> {
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
            (Some(_), false) if !create => "MISSING",
            (Some(bytes), false) => {
                write_new(&view.output, &bytes)
                    .map_err(|e| format!("RENDER_WRITE {}: {e}", view.output.display()))?;
                "CREATED"
            }
        };
        outcome.push((view.output, status));
    }
    Ok(outcome)
}

/// Fresh file only, complete or absent: written beside the target and linked into place.
fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let partial = path.with_file_name(format!(
        ".{}.partial-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("view"),
        std::process::id()
    ));
    let written = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&partial)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::hard_link(&partial, path)
    })();
    let _ = std::fs::remove_file(&partial);
    written
}

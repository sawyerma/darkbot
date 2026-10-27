//! DEC-16: the economics of an unbounded run (econ4h `until-stopped:W:R`) — windows of the
//! plan's duration measured one after another by the same worker, owners and capture, with the
//! same metrics and reports as a fixed run: four snapshots per window in its own report journal
//! (`snapshots/windows/wNNNNNN/economics_reports.p0j`), its opportunity rows, a sealed segment of
//! the evaluation journal, and one typed window record (`snapshots/economics_windows.p0j`). A
//! closed window stays provisional (no canonical finalization, no ECON_FINAL); evidence that
//! arrives after its window closed is counted as late, never attributed back to it (user
//! decision 27.10.). The run ends at its stop with a partial last window.
use super::*;
use std::collections::VecDeque;

/// The windows journal of an unbounded run (beside the snapshots).
pub const WINDOWS_JOURNAL: &str = "economics_windows.p0j";

/// How a window ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowStatus {
    /// At its boundary, with the verdict of its gates (`until_stopped::check_window`).
    Closed { verdict: String },
    /// At the run's end before its boundary.
    Partial { reason: String },
}

/// `ECON_WINDOW` frame: one measured window of an unbounded run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EconWindow {
    pub window: u64,
    pub from_ns: u64,
    pub end_ns: u64,
    pub from_unix_ns: u128,
    pub end_unix_ns: u128,
    pub status: WindowStatus,
    /// Snapshots written for the window (provisional).
    pub snapshots: u64,
    pub opportunities: u64,
    /// The window's report journal (relative to the run) and its sealed root.
    pub report_journal: String,
    pub report_root_sha256: String,
    /// The segment of the evaluation journal that holds the window's evaluations.
    pub evaluations: crate::storage::segments::SegmentRef,
    /// Evidence received after its window had closed (counted, never attributed back).
    pub late_evidence: u64,
    /// Evidence of a later window that found the bounded buffer full (counted, then dropped).
    pub pending_overflow: u64,
    /// The run's shared failure counters during the window.
    pub analytics_drops: u64,
    pub retention_overflow: u64,
    pub writer_errors: u64,
}
impl crate::analytics::payload::Payload for EconWindow {
    const KIND: u16 = kind::ECON_WINDOW;
}

/// The worker's state of an unbounded run.
pub(super) struct RollingState {
    /// The window now open (1-based).
    pub window: u64,
    pending: VecDeque<Evidence>,
    pending_overflow: u64,
    late: u64,
    journal: crate::storage::segments::SegmentedWriter,
    counters_at_open: [u64; 3],
    /// The verdict that halted the run (a closed window's checks).
    halted: Option<String>,
}

impl RollingState {
    /// The worker failed: the run ends incomplete with this reason (the first one stays).
    pub fn fail(&mut self, why: String) {
        self.halted.get_or_insert(why);
    }
    pub fn create(paths: &crate::run_layout::RunPaths) -> Result<Self, String> {
        Ok(Self {
            window: 1,
            pending: VecDeque::new(),
            pending_overflow: 0,
            late: 0,
            journal: crate::storage::segments::SegmentedWriter::create(
                &paths.snapshot(WINDOWS_JOURNAL),
                0,
            )
            .map_err(|e| format!("ECON_WINDOWS_JOURNAL: {e}"))?,
            counters_at_open: [0; 3],
            halted: None,
        })
    }
}

/// The report journal of window `window` (created on demand).
pub(super) fn report_journal(
    paths: &crate::run_layout::RunPaths,
    window: u64,
) -> Result<journal::Writer, String> {
    let path = window_report_path(paths, window);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("ECON_WINDOW_REPORTS: {e}"))?;
    }
    journal::Writer::create(&path).map_err(|e| format!("ECON_WINDOW_REPORTS: {e}"))
}

fn window_report_path(paths: &crate::run_layout::RunPaths, window: u64) -> PathBuf {
    paths.snapshot(&format!("windows/w{window:06}/{REPORT_JOURNAL}"))
}

/// The receive time evidence is attributed by (`None`: the open window's own evidence).
fn evidence_time(e: &Evidence) -> Option<u64> {
    match e {
        Evidence::Market(m) if matches!(m.source, MarketEvidenceSource::Live) => m.t_rx_ns,
        Evidence::Slot { t_rx_ns, .. } => Some(*t_rx_ns),
        Evidence::Shadow(rec) => rec.timing.t_rx_ns,
        Evidence::MarketCounters { at_ns, .. } => Some(*at_ns),
        _ => None,
    }
}

impl Worker {
    /// The measurement window now open: a fixed run's one window, an unbounded run's open grid
    /// window (ending at its stop when that comes first).
    pub(super) fn window(&self) -> Option<(u64, u64)> {
        let start = self.h.shared.start_ns.load(Ordering::Acquire);
        let Some(r) = &self.rolling else {
            return self.h.cutoff().map(|cutoff| (start, cutoff));
        };
        let width = self.h.shared.window_ns.load(Ordering::Acquire);
        if width == 0 {
            return None;
        }
        let from = start + (r.window - 1) * width;
        let end = from + width;
        Some((from, self.h.cutoff().map_or(end, |c| c.min(end))))
    }

    /// The live hour of a receive time in the open window (`None` outside it).
    pub(super) fn window_hour(&self, ns: u64) -> Option<usize> {
        match &self.rolling {
            None => self.h.hour(ns),
            Some(r) => self
                .h
                .window_of(ns)
                .filter(|(n, ..)| *n == r.window)
                .and_then(|_| self.h.hour(ns)),
        }
    }

    /// Routes evidence of an unbounded run by its window: a later window's waits (bounded) until
    /// its window opens; an earlier window's is counted late and taken like any evidence outside
    /// the window. `Some` is the evidence to take now.
    pub(super) fn route(&mut self, e: Evidence) -> Option<Evidence> {
        let Some(t) = evidence_time(&e) else {
            return Some(e);
        };
        let Some((n, ..)) = self.h.window_of(t) else {
            return Some(e);
        };
        let capacity = self.h.cfg.queue_capacity;
        let r = self.rolling.as_mut()?;
        if n < r.window {
            r.late += 1;
        }
        if n <= r.window {
            return Some(e);
        }
        if r.pending.len() < capacity {
            r.pending.push_back(e);
        } else {
            r.pending_overflow += 1;
            self.over_limit();
        }
        None
    }

    /// The window `window` closed at its boundary: every quadrant written (provisional), its
    /// rows, report journal and evaluation segment sealed and recorded, then the next window
    /// opens and takes the evidence that waited for it.
    pub(super) fn close_window(
        &mut self,
        window: u64,
        boundary_ns: u64,
        verdict: String,
    ) -> Result<(), String> {
        let open = self
            .rolling
            .as_ref()
            .map(|r| r.window)
            .ok_or("ECON_WINDOW_NOT_ROLLING")?;
        if window != open {
            // Never measured on: the run stops and ends incomplete with this reason.
            let why = format!("ECON_WINDOW_ORDER: closed {window}, open {open}");
            crate::console::line(why.clone());
            if let Some(r) = self.rolling.as_mut() {
                r.fail(why);
            }
            self.h.shared.request_abort();
            return Ok(());
        }
        while self.snapshots < SNAPSHOT_COUNT {
            let i = self.snapshots + 1;
            self.write_snapshot(i, false)?;
            self.snapshots = i;
        }
        let held = verdict == crate::orchestration::until_stopped::CONTINUE;
        let closed = verdict.clone();
        self.end_window(WindowStatus::Closed { verdict }, boundary_ns)?;
        let paths = self.h.cfg.paths();
        let r = self.rolling.as_mut().ok_or("ECON_WINDOW_NOT_ROLLING")?;
        if !held {
            r.halted = Some(closed);
        }
        r.window += 1;
        self.reports = report_journal(&paths, r.window)?;
        let waiting = std::mem::take(&mut r.pending);
        for e in waiting {
            self.accept(e)?;
        }
        Ok(())
    }

    /// Records the open window as ended at `end_ns` and resets the window's retained state.
    fn end_window(&mut self, status: WindowStatus, end_ns: u64) -> Result<(), String> {
        let (from, _) = self.window().ok_or("ECON_WINDOW_NOT_STARTED")?;
        let rows: Vec<OpportunityRow> = self
            .selected()
            .into_iter()
            .map(|(_, rec)| self.opportunity(rec))
            .collect();
        for row in &rows {
            self.append_typed_report(row)?;
        }
        let sealed = self
            .reports
            .seal(true)
            .map_err(|e| format!("ECON_WINDOW_REPORTS_SEAL: {e}"))?;
        let evaluations = self
            .evaluations
            .rotate_now()
            .map_err(|e| format!("SHADOW_JOURNAL_ROTATE: {e}"))?;
        self.evaluations_at.clear();
        let shared = &self.h.shared;
        let counters = [
            shared.drops.load(Ordering::Relaxed),
            shared.retention_overflow.load(Ordering::Relaxed),
            shared.writer_errors.load(Ordering::Relaxed),
        ];
        let paths = self.h.cfg.paths();
        let clock = self.h.clock;
        let snapshots = self.snapshots as u64;
        let r = self.rolling.as_mut().ok_or("ECON_WINDOW_NOT_ROLLING")?;
        let report = window_report_path(&paths, r.window);
        let record = EconWindow {
            window: r.window,
            from_ns: from,
            end_ns,
            from_unix_ns: clock.unix_ns(from),
            end_unix_ns: clock.unix_ns(end_ns),
            status,
            snapshots,
            opportunities: rows.len() as u64,
            report_journal: report
                .strip_prefix(&self.h.cfg.run_dir)
                .unwrap_or(&report)
                .display()
                .to_string(),
            report_root_sha256: journal::hex(&sealed.hash),
            evaluations,
            late_evidence: std::mem::take(&mut r.late),
            pending_overflow: std::mem::take(&mut r.pending_overflow),
            analytics_drops: counters[0].saturating_sub(r.counters_at_open[0]),
            retention_overflow: counters[1].saturating_sub(r.counters_at_open[1]),
            writer_errors: counters[2].saturating_sub(r.counters_at_open[2]),
        };
        r.counters_at_open = counters;
        let provenance = Provenance {
            source: journal::source::ECON,
            source_seq: record.window,
            ..Default::default()
        };
        r.journal
            .append_typed(provenance, &record)
            .and_then(|_| r.journal.sync())
            .map_err(|e| format!("ECON_WINDOWS_JOURNAL: {e}"))?;
        crate::console::line(format!(
            "[C0_E1/WINDOW] window={} status={:?} snapshots={} opportunities={} late={}",
            record.window,
            record.status,
            record.snapshots,
            record.opportunities,
            record.late_evidence
        ));
        // The next window starts empty; the last checkpoint of this one is its baseline.
        self.market.clear();
        self.canonical_market.clear();
        self.canonical_joined.clear();
        self.slots.clear();
        self.shadow.clear();
        self.shadow_heads.clear();
        self.winners.clear();
        let baseline = self.checkpoints.remove(&SNAPSHOT_COUNT);
        self.checkpoints.clear();
        self.retained_bytes = 0;
        if let Some(c) = baseline {
            self.retained_bytes = retained_size(&c);
            self.checkpoints.insert(0, c);
        }
        self.market_counters = Default::default();
        self.evaluation_counts = [0; SNAPSHOT_COUNT];
        self.snapshots = 0;
        Ok(())
    }

    /// The end of an unbounded run (its stop or its abort): the open window is recorded as
    /// partial with every quadrant it reached, the evaluation and windows journals are sealed —
    /// complete only for an ordered stop without dropped evidence.
    pub(super) fn finish_rolling(mut self) -> Result<(), String> {
        let shared = &self.h.shared;
        let ordered = shared.operator_stop.load(Ordering::Acquire)
            && !shared.market_failed.load(Ordering::Acquire)
            && !shared.shadow_failed.load(Ordering::Acquire);
        let halted = self.rolling.as_ref().and_then(|r| r.halted.clone());
        let ordered = ordered && halted.is_none();
        let reason = if let Some(verdict) = &halted {
            verdict.as_str()
        } else if ordered {
            "OPERATOR_STOP"
        } else if shared.market_failed.load(Ordering::Acquire) {
            "MARKET_FAILED"
        } else if shared.shadow_failed.load(Ordering::Acquire) {
            "SHADOW_FAILED"
        } else {
            "ABORTED"
        };
        let complete = ordered && shared.drops.load(Ordering::Relaxed) == 0;
        let reason = reason.to_string();
        if let Some((start, end)) = self.window() {
            // A run that ended before its window's end ends that window now.
            let end = end.min(self.h.clock.now_ns().max(start));
            let due = self
                .h
                .cfg
                .plan
                .due(self.h.clock.now_ns().min(end).saturating_sub(start));
            while self.snapshots < due {
                let i = self.snapshots + 1;
                self.write_snapshot(i, false)?;
                self.snapshots = i;
            }
            self.end_window(
                WindowStatus::Partial {
                    reason: reason.clone(),
                },
                end,
            )?;
        }
        let started = self.window().is_some();
        self.evaluations
            .seal(complete)
            .map_err(|e| format!("SHADOW_JOURNAL_SEAL: {e}"))?;
        let paths = self.h.cfg.paths();
        let r = self.rolling.as_mut().ok_or("ECON_WINDOW_NOT_ROLLING")?;
        r.journal
            .seal(complete)
            .map_err(|e| format!("ECON_WINDOWS_JOURNAL: {e}"))?;
        // A run whose windows never started leaves no window: its empty report journal goes.
        if !started {
            let _ = std::fs::remove_file(window_report_path(&paths, r.window));
        }
        crate::console::line(format!(
            "C0_E1_UNTIL_STOPPED_END reason={reason} windows={} complete={complete}; windows stay provisional (no canonical finalization)",
            r.window
        ));
        if ordered {
            Ok(())
        } else {
            Err(format!("C0_E1_UNTIL_STOPPED_INCOMPLETE: {reason}"))
        }
    }
}

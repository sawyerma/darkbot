//! DEC-16 (stage 3): the rolling windows of an unbounded Shadow owner run.
//!
//! A Shadow-only run drives the mode-neutral `until_stopped::RollingWindows` itself: the first
//! window opens at the owner's first tradable READY; every window is the run's checkpoint long
//! whatever the readiness within it (its ready time is recorded); its gates are re-checked at
//! its end (`check_window`) and a violated gate is the run's safe HALT, final for the run.
//! An econ4h run's windows are the econ grid, whose gates its market capture checks; the owner
//! follows that grid with its own journals and checks the integrity of every segment it sealed
//! (a failure aborts the run).
//!
//! In both, at a window's end the owner journals the window's counters (`OwnerWindow`, the last
//! event of the window's evidence segment) and asks its evidence writer to seal that segment —
//! without waiting and without a drain: candidates, watches and workers go on, what crosses the
//! boundary is counted (D-1).
use super::*;
use crate::analytics::evidence::{OwnerCounters, OwnerWindow, RotatedSegment, Rotation};
use crate::orchestration::until_stopped::{self as us, Rolling, RollingWindows};
use crate::storage::preflight;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::TryRecvError;

/// A window's end whose evidence segment the writer has not sealed yet.
struct Pending {
    ended_unix_s: u64,
    /// The provider window it ended (a Shadow-only run's; an econ run's capture ends its own).
    provider: Option<darkbot_solana::provider::WindowUsage>,
    rotation: Rotation,
    requested: std::time::Instant,
}

/// A window's end in progress: its segment's seal, then its check off the owner thread (D-1:
/// detection never waits for a window's I/O).
enum Boundary {
    Sealing(Pending),
    Checking(std::thread::JoinHandle<Checked>),
}

/// What a window's check returned: a Shadow-only run's windows (back from the check) and its
/// verdict, an econ run's segment integrity; `Err` is the code the run fails with.
struct Checked {
    windows: Option<RollingWindows>,
    ended_unix_s: u64,
    outcome: Result<Option<us::WindowCheck>, (&'static str, String)>,
}

const ROTATION_STALLED: &str = "UNTIL_STOPPED_OWNER_ROTATION_STALLED";
const CHECK_PANICKED: &str = "UNTIL_STOPPED_WINDOW_CHECK_PANICKED";

/// DEC-16: the run directory of a Shadow run whose record journal is `record_path`
/// (`P0_RECORD_PATH`); a continuation names exactly this run.
pub(crate) fn resume_dir(
    record_path: &std::path::Path,
    resume: &str,
) -> Result<std::path::PathBuf, String> {
    let dir = record_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    if dir.file_name().and_then(|n| n.to_str()) != Some(resume) {
        return Err(format!(
            "UNTIL_STOPPED_RESUME_DRIFT: the start continues {resume}, its record is {}",
            record_path.display()
        ));
    }
    Ok(dir)
}

/// DEC-16 (D-3 for Shadow): the windows of the crashed unbounded Shadow run `rolling` continues
/// in `run_dir`: only the same admitted run that never halted; the window the crash interrupted
/// — from its last recorded end to `now_s` — is checked like any window before anything is read
/// (`CRASH_RECOVERED`, or its HALT, which ends the run here and stays final).
pub(crate) fn resume_windows(
    rolling: &Rolling,
    run_dir: &std::path::Path,
    now_s: u64,
    environment: fn(&std::path::Path) -> preflight::Environment,
    budget: &darkbot_solana::provider::Budget,
) -> Result<(RollingWindows, us::WindowCheck), String> {
    let (windows, check) = RollingWindows::resume(
        run_dir,
        rolling.binding.clone(),
        &rolling.qualification_id,
        &rolling.config,
        super::super::boot_audit::SHADOW_CAPTURE_SOURCE,
        us::Measured {
            now: now_s,
            bytes: us::run_bytes(run_dir),
            run_volume: run_dir,
            environment: environment(run_dir),
            // What this process used so far (nothing read yet); the crashed one's use ended
            // with it.
            provider: budget.roll_window(),
            evidence: us::newest_evidence(
                &rolling.project,
                &rolling.config,
                &rolling.binding,
                now_s.saturating_add(rolling.binding.checkpoint_seconds),
            ),
            sealed: None,
        },
    )
    .map_err(|e| format!("UNTIL_STOPPED_RESUME: {e}"))?;
    crate::console::line(format!(
        "[UNTIL_STOPPED] window={} verdict={} {}",
        check.window, check.verdict, check.detail
    ));
    if let Some(verdict) = windows.halted().map(str::to_string) {
        rolling.suspend_autostart(&verdict, now_s);
        if let Err(e) = windows.finish(false) {
            crate::console::line(format!("UNTIL_STOPPED_WINDOWS_INCOMPLETE {e}"));
        }
        return Err(format!("{}: {verdict}", us::HALT_STOP));
    }
    Ok((windows, check))
}

/// Whose windows these are.
// One value per run, held in place by its owner (never moved on a hot path).
#[allow(clippy::large_enum_variant)]
enum Driver {
    /// A Shadow-only run: the owner's own rolling windows, checked here.
    Own {
        rolling: Box<Rolling>,
        windows: Option<RollingWindows>,
        /// The provider budget whose window a boundary ends (production: the shared one every
        /// RPC client of the run accounts against).
        budget: std::sync::Arc<darkbot_solana::provider::Budget>,
    },
    /// An econ4h run: the econ grid, checked by its capture.
    Econ,
}

pub(crate) struct OwnerWindows {
    driver: Driver,
    run_dir: std::path::PathBuf,
    /// The window now open (0 before the first); the MT0 journal follows it.
    open: u64,
    mt0_window: std::sync::Arc<AtomicU64>,
    pending: Option<Boundary>,
    environment: fn(&std::path::Path) -> preflight::Environment,
    /// How long a window's end may wait for its sealed segment (`market_writer_stall_ms`).
    stall: Duration,
    started_unix_s: u64,
    ready_ns: u64,
    last_ns: Option<u64>,
    /// Why a Shadow-only run stops: an operator stop, its HALT, the end of its standing
    /// approval or its maximum runtime.
    stop: Option<&'static str>,
    /// A window that could not be journaled or checked: the run ends INCOMPLETE.
    failure: Option<&'static str>,
}

impl OwnerWindows {
    /// The windows of a Shadow-only run admitted as `rolling`.
    pub(crate) fn new(
        rolling: Rolling,
        run_dir: std::path::PathBuf,
        mt0_window: std::sync::Arc<AtomicU64>,
    ) -> Self {
        let stall = Duration::from_millis(rolling.config.limits.market_writer_stall_ms);
        Self::with(
            Driver::Own {
                rolling: Box::new(rolling),
                windows: None,
                budget: darkbot_solana::provider::shared(),
            },
            run_dir,
            mt0_window,
            stall,
        )
    }

    /// The windows of a crashed Shadow-only run continued by [`resume_windows`]: the next
    /// window is open from `now_s` (the MT0 journal follows it).
    pub(crate) fn resumed(
        rolling: Rolling,
        run_dir: std::path::PathBuf,
        mt0_window: std::sync::Arc<AtomicU64>,
        windows: RollingWindows,
        now_ns: u64,
        now_s: u64,
    ) -> Self {
        let mut this = Self::new(rolling, run_dir, mt0_window);
        this.open = windows.window();
        this.mt0_window.store(this.open, Ordering::Release);
        if let Driver::Own { windows: w, .. } = &mut this.driver {
            *w = Some(windows);
        }
        this.started_unix_s = now_s;
        this.last_ns = Some(now_ns);
        this
    }

    /// The owner's side of an econ4h run's windows (the econ grid; `stall` as its writers').
    pub(crate) fn econ(
        run_dir: std::path::PathBuf,
        mt0_window: std::sync::Arc<AtomicU64>,
        stall: Duration,
    ) -> Self {
        Self::with(Driver::Econ, run_dir, mt0_window, stall)
    }

    fn with(
        driver: Driver,
        run_dir: std::path::PathBuf,
        mt0_window: std::sync::Arc<AtomicU64>,
        stall: Duration,
    ) -> Self {
        Self {
            driver,
            run_dir,
            open: 0,
            mt0_window,
            pending: None,
            environment: preflight::environment,
            stall,
            started_unix_s: 0,
            ready_ns: 0,
            last_ns: None,
            stop: None,
            failure: None,
        }
    }

    /// The host facts a test checks its windows against (production: the run volume's).
    #[cfg(test)]
    pub(crate) fn with_environment(
        mut self,
        environment: fn(&std::path::Path) -> preflight::Environment,
    ) -> Self {
        self.environment = environment;
        self
    }

    /// The provider budget a test's windows end (production: the shared one).
    #[cfg(test)]
    pub(crate) fn with_budget(
        mut self,
        budget: std::sync::Arc<darkbot_solana::provider::Budget>,
    ) -> Self {
        if let Driver::Own { budget: b, .. } = &mut self.driver {
            *b = budget;
        }
        self
    }

    /// Why the run stops, once it does.
    pub(crate) fn stop_reason(&self) -> Option<&'static str> {
        self.stop
    }

    /// The operator's stop (or another ordered end) of the run; the first reason stays.
    pub(crate) fn request_stop(&mut self, reason: &'static str) {
        self.stop.get_or_insert(reason);
    }

    /// Whether the open window ends at `now_ns`/`now_s` (its counters are taken then).
    pub(crate) fn due(
        &self,
        now_ns: u64,
        now_s: u64,
        econ: Option<&crate::analytics::econ::Handle>,
    ) -> bool {
        if self.pending.is_some() || self.open == 0 {
            return false;
        }
        match &self.driver {
            Driver::Own { windows, .. } => windows.as_ref().is_some_and(|w| w.due(now_s)),
            Driver::Econ => econ
                .and_then(|h| h.window_of(now_ns))
                .is_some_and(|(k, _, _)| k > self.open),
        }
    }

    /// How the run ended for its result: a failed window or a HALT is an error, every other
    /// end is the run's own.
    pub(crate) fn result(&self) -> Result<(), &'static str> {
        if let Some(code) = self.failure {
            return Err(code);
        }
        if self.stop == Some(us::HALT_STOP) {
            return Err(us::HALT_STOP);
        }
        Ok(())
    }

    fn fail(&mut self, code: &'static str) -> Result<Option<&'static str>, &'static str> {
        self.failure.get_or_insert(code);
        Err(code)
    }

    /// Opens the first window: a Shadow-only run's at its first ready, an econ run's once its
    /// grid started (the window `now_ns` falls in).
    fn open_first(
        &mut self,
        ready: bool,
        now_ns: u64,
        now_s: u64,
        econ: Option<&crate::analytics::econ::Handle>,
    ) -> Result<Option<&'static str>, &'static str> {
        let opened = match &mut self.driver {
            Driver::Own {
                rolling, windows, ..
            } => {
                if !ready {
                    return Ok(None);
                }
                match RollingWindows::start(
                    &self.run_dir,
                    rolling.binding.clone(),
                    &rolling.qualification_id,
                    now_s,
                    0,
                ) {
                    Ok(w) => {
                        let first = w.window();
                        *windows = Some(w);
                        first
                    }
                    Err(e) => {
                        crate::console::line(format!("UNTIL_STOPPED_WINDOWS {e}"));
                        return self.fail("UNTIL_STOPPED_WINDOWS_WRITE_FAILED");
                    }
                }
            }
            Driver::Econ => match econ
                .filter(|h| h.rolling())
                .and_then(|h| h.window_of(now_ns))
            {
                Some((k, _, _)) => k,
                None => return Ok(None),
            },
        };
        self.open = opened;
        self.mt0_window.store(opened, Ordering::Release);
        self.started_unix_s = now_s;
        self.last_ns = Some(now_ns);
        Ok(None)
    }

    /// One owner pass (`ready`: the owner's tradable readiness, `now_ns` its monotonic and
    /// `now_s` its wall clock; `counters` taken when a window ends; `econ`: an econ run's
    /// handle): opens the first window, ends a due window and checks a window once its segment
    /// is sealed. `Ok(Some(reason))` stops the run; `Err` is a window that failed (the run is
    /// INCOMPLETE).
    pub(crate) fn tick(
        &mut self,
        ready: bool,
        now_ns: u64,
        now_s: u64,
        evidence: &mut OwnerEvidenceWriter,
        counters: Option<OwnerCounters>,
        econ: Option<&crate::analytics::econ::Handle>,
    ) -> Result<Option<&'static str>, &'static str> {
        if let Some(code) = self.failure {
            return Err(code);
        }
        if self.open == 0 {
            if self.stop.is_some() {
                return Ok(self.stop);
            }
            return self.open_first(ready, now_ns, now_s, econ);
        }
        if let Some(last) = self.last_ns.replace(now_ns) {
            if ready {
                self.ready_ns += now_ns.saturating_sub(last);
            }
        }
        if self.stop.is_some() {
            return Ok(self.stop);
        }
        match self.pending.take() {
            None => {}
            Some(Boundary::Sealing(p)) => {
                return match p.rotation.try_recv() {
                    Ok(Ok(sealed)) => self.spawn_check(p, sealed),
                    Ok(Err(code)) => self.fail(code),
                    Err(TryRecvError::Empty) if p.requested.elapsed() <= self.stall => {
                        self.pending = Some(Boundary::Sealing(p));
                        Ok(None)
                    }
                    Err(TryRecvError::Empty) => self.fail(ROTATION_STALLED),
                    Err(TryRecvError::Disconnected) => self.fail("OWNER_EVIDENCE_DISCONNECTED"),
                };
            }
            Some(Boundary::Checking(h)) if !h.is_finished() => {
                self.pending = Some(Boundary::Checking(h));
                return Ok(None);
            }
            Some(Boundary::Checking(h)) => {
                return match h.join() {
                    Ok(c) => self.checked(c),
                    Err(_) => self.fail(CHECK_PANICKED),
                };
            }
        }
        if self.due(now_ns, now_s, econ) {
            let record = OwnerWindow {
                window: self.open,
                started_unix_s: self.started_unix_s,
                ended_unix_s: now_s,
                ready_ns: std::mem::take(&mut self.ready_ns),
                end: None,
                counters: counters.unwrap_or_default(),
            };
            if let Err(e) = evidence.emit(OwnerEvidence::Window(record)) {
                return self.fail(e.0);
            }
            let rotation = match evidence.rotate() {
                Ok(r) => r,
                Err(e) => return self.fail(e.0),
            };
            let provider = match &self.driver {
                Driver::Own { budget, .. } => Some(budget.roll_window()),
                Driver::Econ => None,
            };
            self.pending = Some(Boundary::Sealing(Pending {
                ended_unix_s: now_s,
                provider,
                rotation,
                requested: std::time::Instant::now(),
            }));
            self.open += 1;
            self.mt0_window.store(self.open, Ordering::Release);
            self.started_unix_s = now_s;
            return Ok(None);
        }
        if let Driver::Own {
            rolling,
            windows: Some(w),
            ..
        } = &self.driver
        {
            if rolling.max_runtime_reached(w, now_s) {
                self.stop = Some(super::super::autostart::MAX_RUNTIME);
            }
        }
        Ok(self.stop)
    }

    /// Starts the check of the window that ended at `p` against its sealed evidence segment off
    /// the owner thread: a Shadow-only run's gates (`check_window`, which re-reads the segment),
    /// an econ run's segment integrity.
    fn spawn_check(
        &mut self,
        p: Pending,
        sealed: RotatedSegment,
    ) -> Result<Option<&'static str>, &'static str> {
        let run_dir = self.run_dir.clone();
        let environment = self.environment;
        let job: Box<dyn FnOnce() -> Checked + Send> = match &mut self.driver {
            Driver::Own {
                rolling, windows, ..
            } => {
                let Some(mut windows) = windows.take() else {
                    return self.fail("UNTIL_STOPPED_WINDOWS_WRITE_FAILED");
                };
                let rolling = (**rolling).clone();
                Box::new(move || {
                    let next_end = p
                        .ended_unix_s
                        .saturating_add(rolling.binding.checkpoint_seconds);
                    let outcome = windows
                        .close(
                            &rolling.config,
                            super::super::boot_audit::SHADOW_CAPTURE_SOURCE,
                            us::Measured {
                                now: p.ended_unix_s,
                                bytes: us::run_bytes(&run_dir),
                                run_volume: &run_dir,
                                environment: environment(&run_dir),
                                provider: p.provider.unwrap_or(
                                    darkbot_solana::provider::WindowUsage {
                                        requests: 0,
                                        bytes: 0,
                                    },
                                ),
                                evidence: us::newest_evidence(
                                    &rolling.project,
                                    &rolling.config,
                                    &rolling.binding,
                                    next_end,
                                ),
                                sealed: Some((&sealed.sealed, sealed.root_sha256)),
                            },
                        )
                        .map(Some)
                        .map_err(|e| ("UNTIL_STOPPED_WINDOWS_WRITE_FAILED", e.to_string()));
                    Checked {
                        windows: Some(windows),
                        ended_unix_s: p.ended_unix_s,
                        outcome,
                    }
                })
            }
            Driver::Econ => Box::new(move || Checked {
                windows: None,
                ended_unix_s: p.ended_unix_s,
                outcome: us::segment_intact(&sealed.sealed, sealed.root_sha256)
                    .map(|()| None)
                    .map_err(|e| ("UNTIL_STOPPED_SEGMENT_INTEGRITY", e)),
            }),
        };
        match std::thread::Builder::new()
            .name("p0-owner-window".into())
            .spawn(job)
        {
            Ok(h) => {
                self.pending = Some(Boundary::Checking(h));
                Ok(None)
            }
            Err(_) => self.fail("UNTIL_STOPPED_WINDOW_CHECK_SPAWN"),
        }
    }

    /// Takes a window's check: a Shadow-only run's windows back, its verdict journaled (its
    /// HALT, or the end of its standing approval, stops the run), an econ run's integrity.
    fn checked(&mut self, c: Checked) -> Result<Option<&'static str>, &'static str> {
        if let (Driver::Own { windows, .. }, Some(w)) = (&mut self.driver, c.windows) {
            *windows = Some(w);
        }
        let check = match c.outcome {
            Ok(None) => return Ok(self.stop),
            Ok(Some(check)) => check,
            Err((code, e)) => {
                crate::console::line(format!("{code} {e}"));
                return self.fail(code);
            }
        };
        crate::console::line(format!(
            "[UNTIL_STOPPED] window={} verdict={} {}",
            check.window, check.verdict, check.detail
        ));
        if let Driver::Own { rolling, .. } = &self.driver {
            if let Some(reason) = rolling.after_window(&check) {
                if let Some(code) = check.halt_code() {
                    rolling.suspend_autostart(code, c.ended_unix_s);
                }
                self.stop = Some(reason);
            }
        }
        Ok(self.stop)
    }

    /// DEC-16 (review M-2): settles a window's end still in progress — its segment's seal
    /// (within the stall budget), then its check — so the run's result and its ordered-end
    /// rule include that window (the owner loop has ended).
    pub(crate) fn settle(&mut self) {
        if let Some(Boundary::Sealing(p)) =
            self.pending.take_if(|b| matches!(b, Boundary::Sealing(_)))
        {
            let left = self.stall.saturating_sub(p.requested.elapsed());
            match p.rotation.recv_timeout(left) {
                Ok(Ok(sealed)) => {
                    let _ = self.spawn_check(p, sealed);
                }
                Ok(Err(code)) => {
                    let _ = self.fail(code);
                }
                Err(_) => {
                    let _ = self.fail(ROTATION_STALLED);
                }
            }
        }
        if let Some(Boundary::Checking(h)) = self.pending.take() {
            let _ = match h.join() {
                Ok(c) => self.checked(c),
                Err(_) => self.fail(CHECK_PANICKED),
            };
        }
    }

    /// The run's end: a window's end still in progress is settled first, then the open
    /// window's record carries the run's end — its stop, its failure, or `end` (how the owner
    /// loop ended otherwise).
    pub(crate) fn end(
        &mut self,
        end: &'static str,
        now_s: u64,
        evidence: &mut OwnerEvidenceWriter,
        counters: OwnerCounters,
    ) {
        self.settle();
        if self.open == 0 {
            return;
        }
        let record = OwnerWindow {
            window: self.open,
            started_unix_s: self.started_unix_s,
            ended_unix_s: now_s,
            ready_ns: std::mem::take(&mut self.ready_ns),
            end: Some(self.failure.or(self.stop).unwrap_or(end).to_string()),
            counters,
        };
        if let Err(e) = evidence.emit(OwnerEvidence::Window(record)) {
            let _ = self.fail(e.0);
        }
    }

    /// The reason the run ended by, for its ordered-end rule.
    pub(crate) fn reason(&self) -> &'static str {
        self.failure.or(self.stop).unwrap_or("RUN_END")
    }

    /// Seals a Shadow-only run's windows journal: complete only for a run that ended in order
    /// and never halted or failed a window (an econ run's is its capture's).
    pub(crate) fn finish(mut self, in_order: bool) {
        self.settle();
        if let Driver::Own {
            windows: Some(w), ..
        } = self.driver
        {
            if let Err(e) = w.finish(in_order && self.failure.is_none()) {
                crate::console::line(format!("UNTIL_STOPPED_WINDOWS_INCOMPLETE {e}"));
            }
        }
    }
}

impl ShadowOwner {
    /// DEC-16: the owner's cumulative counters and what is in flight now, for a window's record.
    pub(super) fn owner_counters(&self) -> OwnerCounters {
        let watches = self.winner_tracker.stats();
        let winner_jobs = self.remote.winner_jobs.stats();
        let projection_jobs = self.remote.projection_jobs.stats();
        OwnerCounters {
            updates: self.counts.iter().sum(),
            ingress_overflow: self.consumer.stats().overflow,
            candidates: self.next_candidate_id,
            winner_watch_resolved: watches.resolved,
            winner_watch_expired: watches.expired,
            winner_watch_open: watches.len as u64,
            remote_winner_queue: winner_jobs.sent.saturating_sub(winner_jobs.received),
            remote_projection_queue: projection_jobs
                .sent
                .saturating_sub(projection_jobs.received),
            detect_queue: self.detect_queue.len() as u64,
            lane_queue: self.oracle_lanes.len() as u64,
            blockhash_parked: self.blockhash_parking.len() as u64,
        }
    }

    /// DEC-16: one lifecycle pass of an unbounded run's windows; `false` once the run stops.
    pub(super) fn until_stopped_tick(&mut self, ready: bool) -> bool {
        let Some(mut w) = self.windows.take() else {
            return true;
        };
        let now_ns = self.clock.now_ns();
        let now_s = (self.clock.unix_ns(now_ns) / 1_000_000_000) as u64;
        let counters = w
            .due(now_ns, now_s, self.econ.as_ref())
            .then(|| self.owner_counters());
        let go_on = matches!(
            w.tick(
                ready,
                now_ns,
                now_s,
                &mut self.evidence,
                counters,
                self.econ.as_ref()
            ),
            Ok(None)
        );
        self.windows = Some(w);
        go_on
    }

    /// DEC-16 (review M-2): after the owner loop, the window's end still in progress is settled
    /// and a HALT or a failed window is the run's result (before its end is recorded).
    pub(super) fn settle_windows(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(w) = self.windows.as_mut() else {
            return Ok(());
        };
        w.settle();
        w.result()
            .map_err(|code| Box::new(OwnerTerminal(code)) as Box<dyn std::error::Error>)
    }

    /// DEC-16 (D-7): the ordered end of an unbounded Shadow run — no new candidate, the session
    /// stopped, the winner watches drained within the drain bound (`drain_seconds`); what is
    /// still open then is counted in the run's last window record, never awaited further.
    pub(super) fn drain_watches(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.ingress_session.request();
        let deadline = std::time::Instant::now()
            + Duration::from_secs(
                darkbot_solana::runtime_config::active()
                    .limits
                    .drain_seconds,
            );
        loop {
            self.evidence.check()?;
            self.remote_apply()?;
            if self.winner_tracker.stats().len == 0 {
                return Ok(());
            }
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Ok(());
            }
            self.await_remote(Some(left));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::journal::{self, kind};
    use crate::orchestration::until_stopped::tests::{binding, evidence_for};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn host(free: u64, pressure: Option<i32>) -> preflight::Environment {
        preflight::Environment {
            free_disk_bytes: Some(free),
            physical_bytes: None,
            memory_pressure: pressure,
            swap_used_bytes: None,
            cores: 1,
            load_1m: None,
        }
    }
    fn plenty(_: &Path) -> preflight::Environment {
        host(1 << 50, None)
    }
    fn full(_: &Path) -> preflight::Environment {
        host(0, None)
    }
    fn pressed(_: &Path) -> preflight::Environment {
        host(1 << 50, Some(crate::storage::budget::PRESSURE_CRITICAL))
    }

    struct Run {
        dir: PathBuf,
        run_dir: PathBuf,
        evidence: OwnerEvidenceWriter,
        windows: OwnerWindows,
        mt0: Arc<AtomicU64>,
        budget: Arc<darkbot_solana::provider::Budget>,
    }

    /// An unbounded Shadow run of 60-s windows (120-s runway) in a fresh project, its own
    /// provider budget, host facts from `environment`; `evidence` imports provider evidence
    /// that covers its runway.
    fn run(tag: &str, environment: fn(&Path) -> preflight::Environment, evidence: bool) -> Run {
        let dir =
            std::env::temp_dir().join(format!("p0_owner_windows_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config = darkbot_solana::runtime_config::RuntimeConfig {
            rpc_endpoints: vec!["https://rpc.invalid".into()],
            ..Default::default()
        };
        let mut b = binding(60, 120);
        b.mode = "shadow".into();
        if evidence {
            let file = dir.join("evidence.p0j");
            journal::write_typed_value(&file, 0, &evidence_for(&config, &b, 1_000.0)).unwrap();
            crate::orchestration::provider_evidence::import(&dir, &file).unwrap();
        }
        let run_dir = dir.join("data/shadow/r1");
        std::fs::create_dir_all(&run_dir).unwrap();
        let (evidence, _) = OwnerEvidenceWriter::start_rolling(
            run_dir.join("opportunities.p0j"),
            run_dir.join("recovery.p0j"),
            64,
            Duration::from_secs(60),
            0,
            false,
        )
        .unwrap();
        let mt0 = Arc::new(AtomicU64::new(0));
        let budget = Arc::new(darkbot_solana::provider::Budget::default());
        let rolling = Rolling {
            project: dir.clone(),
            binding: b,
            qualification_id: "0123456789abcdef".into(),
            config,
            max_runtime_seconds: None,
            resume: None,
            autostart: None,
        };
        let windows = OwnerWindows::new(rolling, run_dir.clone(), mt0.clone())
            .with_environment(environment)
            .with_budget(budget.clone());
        Run {
            dir,
            run_dir,
            evidence,
            windows,
            mt0,
            budget,
        }
    }
    impl Run {
        fn tick(&mut self, ready: bool, now_s: u64) -> Result<Option<&'static str>, &'static str> {
            let now_ns = now_s * 1_000_000_000;
            let counters = self
                .windows
                .due(now_ns, now_s, None)
                .then(OwnerCounters::default);
            self.windows
                .tick(ready, now_ns, now_s, &mut self.evidence, counters, None)
        }
        /// Ticks at `now_s` until the window that ended has been re-checked.
        fn settle(&mut self, now_s: u64) -> Result<Option<&'static str>, &'static str> {
            for _ in 0..1_000 {
                let r = self.tick(true, now_s);
                if self.windows.pending.is_none() {
                    return r;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            panic!("the window's segment was never sealed");
        }
    }
    /// The typed frames of `kind` in the chain at `first`, and whether it is sealed complete.
    fn chain<T: crate::analytics::payload::Payload>(first: &Path, kind: u16) -> (Vec<T>, bool) {
        let mut r = crate::storage::segments::ChainReader::open(first).unwrap();
        let mut out = Vec::new();
        while let Some(f) = r.next_frame().unwrap() {
            if f.kind == kind {
                out.push(f.typed().unwrap());
            }
        }
        (out, r.sealed && r.complete)
    }

    /// DEC-16 (D-4, D-1): an unbounded Shadow owner opens its first window at its first tradable
    /// READY; a window is its checkpoint long and counts the time it was ready; at its end the
    /// window's record is the last event of the sealed evidence segment, the provider window is
    /// ended and the gates are re-checked against that segment; an operator stop ends the run in
    /// order with the open window's record carrying the stop.
    #[test]
    fn dec16_an_unbounded_shadow_owner_rolls_windows_from_its_first_ready() {
        let mut r = run("rolls", plenty, true);
        assert_eq!(r.tick(false, 1_000), Ok(None));
        assert!(
            !r.run_dir.join(us::WINDOWS_JOURNAL).exists(),
            "no window before the first ready"
        );
        assert_eq!(r.tick(true, 1_001), Ok(None));
        assert_eq!(r.mt0.load(Ordering::Acquire), 1);
        r.budget.account_bytes(1_234).unwrap();
        assert_eq!(r.tick(false, 1_030), Ok(None));
        assert_eq!(r.tick(true, 1_060), Ok(None));
        assert!(!r.windows.due(1_060_000_000_000, 1_060, None));
        assert!(r.windows.due(1_061_000_000_000, 1_061, None));
        assert_eq!(r.tick(true, 1_061), Ok(None));
        assert_eq!(
            r.mt0.load(Ordering::Acquire),
            2,
            "MT0 segments for window 2 now"
        );
        assert!(
            !r.windows.due(1_200_000_000_000, 1_200, None),
            "one window's end at a time"
        );
        assert_eq!(r.settle(1_061), Ok(None));
        let (checks, _) = chain::<us::WindowCheck>(
            &r.run_dir.join(us::WINDOWS_JOURNAL),
            kind::UNTIL_STOPPED_WINDOW,
        );
        assert_eq!(checks.len(), 1);
        let c = &checks[0];
        assert_eq!(
            (
                c.window,
                c.verdict.as_str(),
                c.started_unix_s,
                c.ended_unix_s
            ),
            (1, us::CONTINUE, 1_001, 1_061)
        );
        assert_eq!(
            c.provider.bytes, 1_234,
            "the window's provider use, its budget window ended"
        );
        assert_eq!(r.budget.roll_window().bytes, 0);
        assert_eq!(c.sealed.as_ref().unwrap().file, "opportunities.p0j");
        r.windows.request_stop(us::OPERATOR_STOP);
        assert_eq!(r.tick(true, 1_070), Ok(Some(us::OPERATOR_STOP)));
        let counters = OwnerCounters {
            updates: 7,
            ..Default::default()
        };
        r.windows
            .end("RUN_COMPLETE", 1_070, &mut r.evidence, counters);
        r.evidence.finish(None).unwrap();
        assert_eq!(
            (r.windows.reason(), r.windows.result()),
            (us::OPERATOR_STOP, Ok(()))
        );
        r.windows.finish(true);
        let first = r.run_dir.join("opportunities.p0j");
        let (records, _) = chain::<OwnerWindow>(&first, kind::OWNER_WINDOW);
        assert_eq!(records.len(), 2);
        assert_eq!(
            (
                records[0].window,
                records[0].end.as_deref(),
                records[0].started_unix_s,
                records[0].ended_unix_s
            ),
            (1, None, 1_001, 1_061)
        );
        assert_eq!(
            records[0].ready_ns, 31_000_000_000,
            "only the ready time of window 1"
        );
        assert_eq!(
            (
                records[1].window,
                records[1].end.as_deref(),
                records[1].counters.updates,
                records[1].ready_ns
            ),
            (2, Some(us::OPERATOR_STOP), 7, 9_000_000_000)
        );
        // The boundary sealed window 1's segment after its record.
        let mut seg = journal::Reader::open(&first).unwrap();
        let mut last = None;
        while let Some(f) = seg.next_frame().unwrap() {
            if f.kind == kind::OWNER_WINDOW {
                last = Some(f.typed::<OwnerWindow>().unwrap().window);
            }
        }
        assert!(seg.sealed && last == Some(1));
        let (_, complete) = chain::<us::WindowCheck>(
            &r.run_dir.join(us::WINDOWS_JOURNAL),
            kind::UNTIL_STOPPED_WINDOW,
        );
        assert!(complete, "an ordered end completes the windows journal");
        let _ = std::fs::remove_dir_all(&r.dir);
    }

    /// DEC-16: a window whose gates do not hold — runway exhausted, memory critical, no provider
    /// evidence — is the run's safe HALT with its code: no further window, the windows journal
    /// never complete, the run's result the HALT.
    #[test]
    fn dec16_a_shadow_window_that_fails_a_gate_halts_the_owner() {
        for (tag, environment, evidence, code) in [
            (
                "runway",
                full as fn(&Path) -> preflight::Environment,
                true,
                "UNTIL_STOPPED_RUNWAY_EXHAUSTED",
            ),
            ("memory", pressed, true, "UNTIL_STOPPED_MEMORY_CRITICAL"),
            (
                "evidence",
                plenty,
                false,
                "UNTIL_STOPPED_PROVIDER_EVIDENCE_MISSING",
            ),
        ] {
            let mut r = run(tag, environment, evidence);
            r.tick(true, 2_000).unwrap();
            assert_eq!(r.tick(true, 2_060), Ok(None));
            assert_eq!(r.settle(2_060), Ok(Some(us::HALT_STOP)), "{tag}");
            assert_eq!(
                r.tick(true, 2_200),
                Ok(Some(us::HALT_STOP)),
                "a HALT is final"
            );
            assert_eq!(r.windows.result(), Err(us::HALT_STOP));
            r.windows.end(
                "RUN_COMPLETE",
                2_200,
                &mut r.evidence,
                OwnerCounters::default(),
            );
            r.evidence.finish(None).unwrap();
            r.windows.finish(true);
            let (checks, complete) = chain::<us::WindowCheck>(
                &r.run_dir.join(us::WINDOWS_JOURNAL),
                kind::UNTIL_STOPPED_WINDOW,
            );
            assert_eq!(
                checks
                    .iter()
                    .map(|c| c.verdict.as_str())
                    .collect::<Vec<_>>(),
                [code],
                "{tag}"
            );
            assert!(
                !complete,
                "{tag}: a halted run's windows journal is never complete"
            );
            let (records, _) =
                chain::<OwnerWindow>(&r.run_dir.join("opportunities.p0j"), kind::OWNER_WINDOW);
            assert_eq!(
                records.last().unwrap().end.as_deref(),
                Some(us::HALT_STOP),
                "{tag}"
            );
            let _ = std::fs::remove_dir_all(&r.dir);
        }
    }

    /// DEC-16: a window whose record or rotation the evidence writer refuses fails the run (its
    /// windows journal never complete), never a silent window.
    #[test]
    fn dec16_a_shadow_window_the_evidence_writer_refuses_fails_the_run() {
        let mut r = run("refused", plenty, true);
        r.tick(true, 3_000).unwrap();
        r.evidence.finish(None).unwrap();
        assert_eq!(r.tick(true, 3_060), Err("OWNER_EVIDENCE_CLOSED"));
        assert_eq!(
            r.tick(true, 3_061),
            Err("OWNER_EVIDENCE_CLOSED"),
            "the failure stays"
        );
        assert_eq!(r.windows.result(), Err("OWNER_EVIDENCE_CLOSED"));
        r.windows.finish(true);
        let (_, complete) = chain::<us::WindowCheck>(
            &r.run_dir.join(us::WINDOWS_JOURNAL),
            kind::UNTIL_STOPPED_WINDOW,
        );
        assert!(!complete);
        let _ = std::fs::remove_dir_all(&r.dir);
    }

    /// The windows journal's verdicts, and whether it is sealed and complete.
    fn verdicts(run_dir: &Path) -> (Vec<(u64, String)>, bool, bool) {
        let mut r = crate::storage::segments::ChainReader::open(&run_dir.join(us::WINDOWS_JOURNAL))
            .unwrap();
        let mut out = Vec::new();
        while let Some(f) = r.next_frame().unwrap() {
            if f.kind == kind::UNTIL_STOPPED_WINDOW {
                let c: us::WindowCheck = f.typed().unwrap();
                out.push((c.window, c.verdict));
            }
        }
        (out, r.sealed, r.complete)
    }

    /// A run whose window 1 closed (`environment`), then a crash: the windows journal and the
    /// evidence stay unsealed (nothing ends in order).
    fn crashed(tag: &str, environment: fn(&Path) -> preflight::Environment) -> Run {
        let mut r = run(tag, environment, true);
        r.tick(true, 1_000).unwrap();
        r.tick(true, 1_060).unwrap();
        let _ = r.settle(1_060);
        r
    }

    /// DEC-16 (D-3 for Shadow): a crashed unbounded Shadow run continues as itself — the window
    /// the crash interrupted is checked before anything is read (CRASH_RECOVERED) and the
    /// windows go on from the restart; another run directory, binding or qualification never
    /// continues it, and the continued chain is never complete.
    #[test]
    fn dec16_a_crashed_shadow_run_continues_the_same_run() {
        let r = crashed("resume", plenty);
        let Run {
            dir,
            run_dir,
            evidence,
            windows,
            budget,
            ..
        } = r;
        let rolling = match &windows.driver {
            Driver::Own { rolling, .. } => (**rolling).clone(),
            Driver::Econ => unreachable!(),
        };
        drop(windows);
        drop(evidence);
        let record = run_dir.join(crate::orchestration::until_stopped_view::SHADOW_RECORD);
        assert_eq!(resume_dir(&record, "r1").unwrap(), run_dir);
        assert!(resume_dir(&record, "r2")
            .unwrap_err()
            .starts_with("UNTIL_STOPPED_RESUME_DRIFT"));
        let mut other = rolling.clone();
        other.binding.checkpoint_seconds = 120;
        assert!(resume_windows(&other, &run_dir, 1_100, plenty, &budget)
            .err()
            .unwrap()
            .contains("UNTIL_STOPPED_RESUME_DRIFT"));
        let mut foreign = rolling.clone();
        foreign.qualification_id = "fedcba9876543210".into();
        assert!(resume_windows(&foreign, &run_dir, 1_100, plenty, &budget)
            .err()
            .unwrap()
            .contains("UNTIL_STOPPED_RESUME_DRIFT"));
        let (resumed, check) = resume_windows(&rolling, &run_dir, 1_100, plenty, &budget).unwrap();
        assert_eq!(
            (
                check.window,
                check.verdict.as_str(),
                check.started_unix_s,
                check.ended_unix_s
            ),
            (2, us::CRASH_RECOVERED, 1_060, 1_100)
        );
        let mt0 = Arc::new(AtomicU64::new(0));
        let mut w = OwnerWindows::resumed(
            rolling,
            run_dir.clone(),
            mt0.clone(),
            resumed,
            1_100_000_000_000,
            1_100,
        )
        .with_environment(plenty)
        .with_budget(budget);
        assert_eq!(
            mt0.load(Ordering::Acquire),
            3,
            "window 3 is open from the restart"
        );
        let (mut evidence, _) = OwnerEvidenceWriter::start_rolling(
            run_dir.join("continued.p0j"),
            run_dir.join("recovery.p0j"),
            64,
            Duration::from_secs(60),
            0,
            false,
        )
        .unwrap();
        let at = |now_s: u64, w: &mut OwnerWindows, e: &mut OwnerEvidenceWriter| {
            let now_ns = now_s * 1_000_000_000;
            let counters = w.due(now_ns, now_s, None).then(OwnerCounters::default);
            w.tick(true, now_ns, now_s, e, counters, None)
        };
        assert_eq!(at(1_159, &mut w, &mut evidence), Ok(None));
        assert!(!w.due(1_159_000_000_000, 1_159, None));
        assert_eq!(at(1_160, &mut w, &mut evidence), Ok(None));
        for _ in 0..1_000 {
            at(1_160, &mut w, &mut evidence).unwrap();
            if w.pending.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        w.request_stop(us::OPERATOR_STOP);
        w.end(
            "RUN_COMPLETE",
            1_170,
            &mut evidence,
            OwnerCounters::default(),
        );
        evidence.finish(None).unwrap();
        w.finish(true);
        let (seen, sealed, complete) = verdicts(&run_dir);
        assert_eq!(
            seen,
            [
                (1, us::CONTINUE.to_string()),
                (2, us::CRASH_RECOVERED.to_string()),
                (3, us::CONTINUE.to_string())
            ]
        );
        assert!(sealed && !complete, "a continued chain is never complete");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEC-16: a crashed run whose interrupted window fails its gates halts before anything is
    /// read (the windows journal sealed, never complete) and is never continued again; a run
    /// that halted before its crash is never continued either.
    #[test]
    fn dec16_a_crashed_shadow_run_whose_interrupted_window_fails_halts_before_any_read() {
        let r = crashed("resume_halt", plenty);
        let (dir, run_dir, budget) = (r.dir.clone(), r.run_dir.clone(), r.budget.clone());
        let rolling = match &r.windows.driver {
            Driver::Own { rolling, .. } => (**rolling).clone(),
            Driver::Econ => unreachable!(),
        };
        drop(r);
        let err = resume_windows(&rolling, &run_dir, 1_100, full, &budget)
            .err()
            .unwrap();
        assert_eq!(
            err,
            format!("{}: UNTIL_STOPPED_RUNWAY_EXHAUSTED", us::HALT_STOP)
        );
        let (seen, sealed, complete) = verdicts(&run_dir);
        assert_eq!(seen.last().unwrap().1, "UNTIL_STOPPED_RUNWAY_EXHAUSTED");
        assert!(sealed && !complete);
        assert!(resume_windows(&rolling, &run_dir, 1_200, plenty, &budget).is_err());
        let _ = std::fs::remove_dir_all(&dir);
        // A run that halted at a window and then crashed (its journal unsealed).
        let r = crashed("resume_halted", full);
        let (dir, run_dir, budget) = (r.dir.clone(), r.run_dir.clone(), r.budget.clone());
        let rolling = match &r.windows.driver {
            Driver::Own { rolling, .. } => (**rolling).clone(),
            Driver::Econ => unreachable!(),
        };
        assert_eq!(r.windows.result(), Err(us::HALT_STOP));
        drop(r);
        assert!(resume_windows(&rolling, &run_dir, 1_100, plenty, &budget)
            .err()
            .unwrap()
            .contains(us::HALTED));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEC-16: an unbounded econ4h run's owner follows the econ grid (its capture checks the
    /// windows): its first window is the grid window it is in once the grid started; a window's
    /// end (a boundary belongs to the earlier window) seals the owner's segment after the
    /// window's record and checks that segment's integrity — a broken one fails the run.
    #[cfg(any(feature = "historical", feature = "crate_health"))]
    #[test]
    fn dec16_an_unbounded_econ4h_owner_cuts_its_journals_on_the_econ_grid() {
        for broken in [false, true] {
            let dir = std::env::temp_dir().join(format!(
                "p0_owner_econ_windows_{}_{broken}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let first = dir.join("opportunities.p0j");
            let (mut evidence, _) = OwnerEvidenceWriter::start_rolling(
                first.clone(),
                dir.join("recovery.p0j"),
                64,
                Duration::from_secs(60),
                0,
                false,
            )
            .unwrap();
            let (h, _rx) = crate::analytics::econ::test_handle(16);
            let mt0 = Arc::new(AtomicU64::new(0));
            let mut w = OwnerWindows::econ(dir.clone(), mt0.clone(), Duration::from_secs(5));
            let tick = |w: &mut OwnerWindows, ns: u64, e: &mut OwnerEvidenceWriter| {
                let counters = w
                    .due(ns, ns / 1_000_000_000, Some(&h))
                    .then(OwnerCounters::default);
                w.tick(true, ns, ns / 1_000_000_000, e, counters, Some(&h))
            };
            assert_eq!(tick(&mut w, 5_000_000_000, &mut evidence), Ok(None));
            assert_eq!(
                mt0.load(Ordering::Acquire),
                0,
                "no window before the grid starts"
            );
            let start = 10_000_000_000;
            h.start_rolling(start);
            let width = h.shared.window_ns.load(Ordering::Acquire);
            assert_eq!(tick(&mut w, start + 1, &mut evidence), Ok(None));
            assert_eq!(mt0.load(Ordering::Acquire), 1);
            assert!(
                !w.due(start + width, 0, Some(&h)),
                "the boundary belongs to window 1"
            );
            assert_eq!(tick(&mut w, start + width + 1, &mut evidence), Ok(None));
            assert_eq!(mt0.load(Ordering::Acquire), 2);
            if broken {
                // The writer sealed window 1's segment once its successor exists.
                let next = crate::storage::segments::segment_path(&first, 2);
                let waiting = std::time::Instant::now();
                while !next.exists() {
                    assert!(waiting.elapsed() < Duration::from_secs(5));
                    std::thread::sleep(Duration::from_millis(2));
                }
                let mut bytes = std::fs::read(&first).unwrap();
                let at = bytes.len() / 2;
                bytes[at] ^= 0xff;
                std::fs::write(&first, bytes).unwrap();
            }
            let mut outcome = Ok(None);
            for _ in 0..1_000 {
                outcome = tick(&mut w, start + width + 2, &mut evidence);
                if w.pending.is_none() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            if broken {
                assert_eq!(outcome, Err("UNTIL_STOPPED_SEGMENT_INTEGRITY"));
                assert_eq!(w.result(), Err("UNTIL_STOPPED_SEGMENT_INTEGRITY"));
                let _ = evidence.finish(None);
            } else {
                assert_eq!(outcome, Ok(None));
                w.end("RUN_COMPLETE", 99, &mut evidence, OwnerCounters::default());
                evidence.finish(None).unwrap();
                assert_eq!(w.result(), Ok(()));
                let (records, complete) = chain::<OwnerWindow>(&first, kind::OWNER_WINDOW);
                assert!(complete);
                assert_eq!(
                    records
                        .iter()
                        .map(|r| (r.window, r.end.clone()))
                        .collect::<Vec<_>>(),
                    [(1, None), (2, Some("RUN_COMPLETE".to_string()))]
                );
            }
            w.finish(true);
            assert!(
                !dir.join(us::WINDOWS_JOURNAL).exists(),
                "an econ run's windows are its capture's"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

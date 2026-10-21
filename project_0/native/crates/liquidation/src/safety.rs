//! P8-LT: Controlled-LIVE Safety-Schicht. Limits sind Konfiguration (kein Wert im Code).
//!
//! Jeder Versuch durchläuft `check()` vor dem Handoff. Jeder Ausgang wird mit
//! `record_outcome()` reconciled. Verletzung eines Limits → HALT der Execution-Klasse;
//! Freigabe nur manuell (Konfigurationsänderung + Neustart), kein Auto-Reset.

use darkbot_common::error::HaltReason;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_total_capital: u64,
    pub max_outstanding_inventory: u64,
    pub max_concurrent_positions: u32,
    pub max_exposure_per_attempt: u64,
    pub max_daily_loss: u64,
    pub max_failed_attempts_per_day: u32,
    pub max_tip: u64,
    pub max_slippage_bps: u64,
    pub max_outstanding_attempts: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deny {
    Halted(HaltReason),
    TotalCapital,
    Inventory,
    ConcurrentPositions,
    AssetNotAllowed,
    ExecutionClassNotAllowed,
    Exposure,
    DailyLoss,
    FailedAttempts,
    Tip,
    Slippage,
    Outstanding,
    LiveDisabled,
}

/// Q2: a safety ledger transition that would break an invariant. It halts the safety layer
/// (`HaltReason::LedgerInvariant`) and changes nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedgerViolation {
    OutstandingUnderflow,
    OpenPositionsUnderflow,
    CapitalReleaseExceedsReserved,
    InventoryReleaseExceedsReserved,
    InventoryNegative,
    InventoryOverflow,
    PnlOverflow,
    CounterOverflow,
}
impl LedgerViolation {
    pub const fn code(self) -> &'static str {
        match self {
            Self::OutstandingUnderflow => "LEDGER_OUTSTANDING_UNDERFLOW",
            Self::OpenPositionsUnderflow => "LEDGER_OPEN_POSITIONS_UNDERFLOW",
            Self::CapitalReleaseExceedsReserved => "LEDGER_CAPITAL_RELEASE_EXCEEDS_RESERVED",
            Self::InventoryReleaseExceedsReserved => "LEDGER_INVENTORY_RELEASE_EXCEEDS_RESERVED",
            Self::InventoryNegative => "LEDGER_INVENTORY_NEGATIVE",
            Self::InventoryOverflow => "LEDGER_INVENTORY_OVERFLOW",
            Self::PnlOverflow => "LEDGER_PNL_OVERFLOW",
            Self::CounterOverflow => "LEDGER_COUNTER_OVERFLOW",
        }
    }
}

/// The counters after releasing one attempt's reservation (computed before anything applies).
struct Released {
    outstanding: u32,
    open_positions: u32,
    capital_in_use: u64,
    inventory_reserved: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DayStats {
    pub day_index: u64,
    pub realized_pnl: i128,
    pub failed_attempts: u32,
    pub attempts: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Allow {
    pub assets: std::collections::HashSet<solana_sdk::pubkey::Pubkey>,
    pub execution_classes: std::collections::HashSet<&'static str>,
}

pub struct Safety {
    limits: Limits,
    allow: Allow,
    live_enabled: bool,
    halted: Option<HaltReason>,
    outstanding: u32,
    open_positions: u32,
    capital_in_use: u64,
    inventory_outstanding: u64,
    /// B15: worst-case inventory every in-flight attempt may still add (held until its outcome).
    inventory_reserved: u64,
    day: DayStats,
}

impl Safety {
    pub fn new(limits: Limits, allow: Allow, live_enabled: bool) -> Self {
        Safety {
            limits,
            allow,
            live_enabled,
            halted: None,
            outstanding: 0,
            open_positions: 0,
            capital_in_use: 0,
            inventory_outstanding: 0,
            inventory_reserved: 0,
            day: DayStats::default(),
        }
    }

    pub fn halt(&mut self, reason: HaltReason) {
        if self.halted.is_none() {
            self.halted = Some(reason);
        }
    }

    pub fn halted(&self) -> Option<HaltReason> {
        self.halted
    }

    /// Q2: the day's realized loss reached the limit — `-pnl >= max` without negating the PnL,
    /// which overflows at `i128::MIN` (a release build would wrap it and never halt).
    fn daily_loss_reached(&self) -> bool {
        self.day.realized_pnl <= -i128::from(self.limits.max_daily_loss)
    }

    fn roll_day(&mut self, now_unix_s: u64) {
        let idx = now_unix_s / 86_400;
        if idx != self.day.day_index {
            self.day = DayStats {
                day_index: idx,
                ..Default::default()
            };
        }
    }

    fn validate_limits(
        &self,
        exposure: u64,
        new_inventory: u64,
        tip: u64,
        slippage_bps: u64,
        assets: &[solana_sdk::pubkey::Pubkey],
        execution_class: &'static str,
    ) -> Result<(), Deny> {
        if let Some(r) = self.halted {
            return Err(Deny::Halted(r));
        }
        if !self.live_enabled {
            return Err(Deny::LiveDisabled);
        }
        if !self.allow.execution_classes.contains(execution_class) {
            return Err(Deny::ExecutionClassNotAllowed);
        }
        if assets.iter().any(|a| !self.allow.assets.contains(a)) {
            return Err(Deny::AssetNotAllowed);
        }
        if self
            .capital_in_use
            .checked_add(exposure)
            .is_none_or(|v| v > self.limits.max_total_capital)
        {
            return Err(Deny::TotalCapital);
        }
        // B15: held inventory plus the worst case of every in-flight attempt plus this attempt.
        let inventory = self
            .inventory_outstanding
            .checked_add(self.inventory_reserved)
            .and_then(|v| v.checked_add(new_inventory));
        if inventory.is_none_or(|v| v > self.limits.max_outstanding_inventory) {
            return Err(Deny::Inventory);
        }
        if self.open_positions >= self.limits.max_concurrent_positions {
            return Err(Deny::ConcurrentPositions);
        }
        if exposure > self.limits.max_exposure_per_attempt {
            return Err(Deny::Exposure);
        }
        if tip > self.limits.max_tip {
            return Err(Deny::Tip);
        }
        if slippage_bps > self.limits.max_slippage_bps {
            return Err(Deny::Slippage);
        }
        if self.day.failed_attempts >= self.limits.max_failed_attempts_per_day {
            return Err(Deny::FailedAttempts);
        }
        if self.daily_loss_reached() {
            return Err(Deny::DailyLoss);
        }
        if self.outstanding >= self.limits.max_outstanding_attempts {
            return Err(Deny::Outstanding);
        }
        Ok(())
    }

    /// Nicht-reservierende Prüfung für Candidate-Auswahl. Rollt nur die Tagesstatistik;
    /// Kapital/Outstanding bleiben unverändert. So kann P4A bei Safety-Verstoß auf eine
    /// kapitalärmere Funding-Klasse zurückfallen, statt die gesamte Opportunity zu verwerfen.
    /// `new_inventory`: worst-case inventory this attempt can leave behind (B15).
    #[allow(clippy::too_many_arguments)]
    pub fn preview(
        &mut self,
        now_unix_s: u64,
        exposure: u64,
        new_inventory: u64,
        tip: u64,
        slippage_bps: u64,
        assets: &[solana_sdk::pubkey::Pubkey],
        execution_class: &'static str,
    ) -> Result<(), Deny> {
        self.roll_day(now_unix_s);
        self.validate_limits(
            exposure,
            new_inventory,
            tip,
            slippage_bps,
            assets,
            execution_class,
        )
    }

    /// Reserves capital and the worst-case inventory hold of the attempt (B15).
    #[allow(clippy::too_many_arguments)]
    pub fn check(
        &mut self,
        now_unix_s: u64,
        exposure: u64,
        new_inventory: u64,
        tip: u64,
        slippage_bps: u64,
        assets: &[solana_sdk::pubkey::Pubkey],
        execution_class: &'static str,
    ) -> Result<(), Deny> {
        self.roll_day(now_unix_s);
        self.validate_limits(
            exposure,
            new_inventory,
            tip,
            slippage_bps,
            assets,
            execution_class,
        )?;
        // Q2: the limits above bound every sum; a sum that still does not fit is a ledger
        // invariant violation (halt), never a saturated value.
        let next = (|| {
            Some((
                self.outstanding.checked_add(1)?,
                self.open_positions.checked_add(1)?,
                self.capital_in_use.checked_add(exposure)?,
                self.inventory_reserved.checked_add(new_inventory)?,
                self.day.attempts.checked_add(1)?,
            ))
        })();
        let Some((outstanding, open, capital, reserved, attempts)) = next else {
            self.halt(HaltReason::LedgerInvariant);
            return Err(Deny::Halted(HaltReason::LedgerInvariant));
        };
        self.outstanding = outstanding;
        self.open_positions = open;
        self.capital_in_use = capital;
        self.inventory_reserved = reserved;
        self.day.attempts = attempts;
        Ok(())
    }

    /// Reconciled-Ergebnis eines Versuchs. `realized_pnl` ist der **echte** Netto-Wert aus
    /// Token-Deltas, Fees, Tips — nicht der geplante.
    /// `exposure_released`: freigegebenes Kapital; `inventory_delta`: gehaltenes Collateral (NO_SWAP) ±;
    /// `inventory_hold`: the worst-case hold `check()` reserved for this attempt (B15).
    /// [FIX-24] `realized_pnl: None` = UNKNOWN → Buchhaltung bleibt offen und HALT(UnknownEconomics):
    /// ein unbekannter Ausgang hält Reservierungen und darf das Tagesbudget nicht besser aussehen lassen.
    #[must_use = "a ledger violation halts the safety layer; the caller must surface it"]
    pub fn record_outcome(
        &mut self,
        now_unix_s: u64,
        landed_success: bool,
        realized_pnl: Option<i128>,
        exposure_released: u64,
        inventory_delta: i128,
        inventory_hold: u64,
    ) -> Result<(), LedgerViolation> {
        self.roll_day(now_unix_s);
        // UNKNOWN cannot release capital/inventory or reduce outstanding count.
        // A later complete reconciliation may close it, but never clears HALT.
        let Some(pnl) = realized_pnl else {
            self.halt(HaltReason::UnknownEconomics);
            return Ok(());
        };
        // Q2: one checked transaction — computed completely, applied only if every step holds.
        let released = self.released(exposure_released, inventory_hold)?;
        let inventory = i128::from(self.inventory_outstanding)
            .checked_add(inventory_delta)
            .ok_or(LedgerViolation::InventoryOverflow)
            .and_then(|v| u64::try_from(v).map_err(|_| LedgerViolation::InventoryNegative));
        let inventory = self.violation(inventory)?;
        let pnl = self.violation(
            self.day
                .realized_pnl
                .checked_add(pnl)
                .ok_or(LedgerViolation::PnlOverflow),
        )?;
        let failed = if landed_success {
            self.day.failed_attempts
        } else {
            self.violation(
                self.day
                    .failed_attempts
                    .checked_add(1)
                    .ok_or(LedgerViolation::CounterOverflow),
            )?
        };
        self.apply_release(released);
        self.inventory_outstanding = inventory;
        self.day.realized_pnl = pnl;
        self.day.failed_attempts = failed;
        if self.daily_loss_reached() {
            self.halt(HaltReason::LimitBreached); // [FIX-12]
        }
        if self.day.failed_attempts >= self.limits.max_failed_attempts_per_day {
            self.halt(HaltReason::LimitBreached); // [FIX-12]
        }
        Ok(())
    }

    /// Q2: the counters after releasing one attempt's reservation, or the violation (halts).
    fn released(
        &mut self,
        exposure: u64,
        inventory_hold: u64,
    ) -> Result<Released, LedgerViolation> {
        let next = (|| {
            Ok(Released {
                outstanding: self
                    .outstanding
                    .checked_sub(1)
                    .ok_or(LedgerViolation::OutstandingUnderflow)?,
                open_positions: self
                    .open_positions
                    .checked_sub(1)
                    .ok_or(LedgerViolation::OpenPositionsUnderflow)?,
                capital_in_use: self
                    .capital_in_use
                    .checked_sub(exposure)
                    .ok_or(LedgerViolation::CapitalReleaseExceedsReserved)?,
                inventory_reserved: self
                    .inventory_reserved
                    .checked_sub(inventory_hold)
                    .ok_or(LedgerViolation::InventoryReleaseExceedsReserved)?,
            })
        })();
        self.violation(next)
    }

    fn apply_release(&mut self, r: Released) {
        self.outstanding = r.outstanding;
        self.open_positions = r.open_positions;
        self.capital_in_use = r.capital_in_use;
        self.inventory_reserved = r.inventory_reserved;
    }

    /// Q2: a violated step halts the safety layer (sticky) and is reported; nothing is applied.
    fn violation<T>(&mut self, step: Result<T, LedgerViolation>) -> Result<T, LedgerViolation> {
        if step.is_err() {
            self.halt(HaltReason::LedgerInvariant);
        }
        step
    }

    pub fn day(&self) -> DayStats {
        self.day
    }

    /// In-flight-Kapital: `check()` reserviert `exposure`; dieser Pfad gibt es bei einem Fehlschlag
    /// **vor** Submission (Build/Sim/Deadline) deterministisch frei, ohne Tagesstatistik zu verändern.
    /// Q2: checked like `record_outcome`; a violation halts and releases nothing.
    #[must_use = "a ledger violation halts the safety layer; the caller must surface it"]
    pub fn release_without_outcome(
        &mut self,
        exposure: u64,
        inventory_hold: u64,
    ) -> Result<(), LedgerViolation> {
        let released = self.released(exposure, inventory_hold)?;
        self.apply_release(released);
        Ok(())
    }

    pub fn capital_in_use(&self) -> u64 {
        self.capital_in_use
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lim() -> Limits {
        Limits {
            max_total_capital: 1_000,
            max_outstanding_inventory: 1_000,
            max_concurrent_positions: 5,
            max_exposure_per_attempt: 100,
            max_daily_loss: 50,
            max_failed_attempts_per_day: 2,
            max_tip: 10,
            max_slippage_bps: 100,
            max_outstanding_attempts: 1,
        }
    }
    fn allow() -> Allow {
        let mut a = Allow::default();
        a.execution_classes.insert("longtail");
        a
    }

    /// Every ledger counter and the day statistics of `s` (Q2 full-state comparisons).
    fn ledger(s: &Safety) -> (u32, u32, u64, u64, u64, DayStats, Option<HaltReason>) {
        (
            s.outstanding,
            s.open_positions,
            s.capital_in_use,
            s.inventory_outstanding,
            s.inventory_reserved,
            s.day,
            s.halted,
        )
    }

    /// Q2 (21.10.): the daily-loss limit holds at the extreme — a realized loss of `i128::MIN`
    /// halts (`record_outcome`) and denies (`preview`/`check`) instead of overflowing the
    /// negation (debug: panic; release: wrap to `i128::MIN` and never halt); the boundary is
    /// unchanged (exactly the limit halts, one less does not).
    #[test]
    fn q2_extreme_daily_loss_halts_without_negation_overflow() {
        let mut s = Safety::new(lim(), allow(), true);
        s.check(0, 50, 0, 1, 1, &[], "longtail").unwrap();
        assert_eq!(
            s.record_outcome(0, false, Some(i128::MIN), 50, 0, 0),
            Ok(())
        );
        assert_eq!(s.halted(), Some(HaltReason::LimitBreached));
        let mut s = Safety::new(lim(), allow(), true);
        s.day.realized_pnl = i128::MIN;
        assert_eq!(
            s.preview(0, 1, 0, 1, 1, &[], "longtail"),
            Err(Deny::DailyLoss)
        );
        for (pnl, reached) in [(-50, true), (-49, false), (0, false), (i128::MAX, false)] {
            let mut s = Safety::new(lim(), allow(), true);
            s.day.realized_pnl = pnl;
            assert_eq!(s.daily_loss_reached(), reached, "{pnl}");
        }
        let mut s = Safety::new(
            Limits {
                max_daily_loss: u64::MAX,
                ..lim()
            },
            allow(),
            true,
        );
        s.day.realized_pnl = -i128::from(u64::MAX) + 1;
        assert!(!s.daily_loss_reached());
        s.day.realized_pnl -= 1;
        assert!(s.daily_loss_reached());
    }

    /// Q2 (21.10.): a failed ledger transition changes nothing — no capital, inventory, PnL,
    /// failure-count or outstanding change — also across UTC midnight, where the only change of a
    /// timed transition (`record_outcome`) is the day roll every timed call at that instant makes
    /// (the time-driven UTC-day contract): the state equals that of an identical ledger whose
    /// next call at the same time was a preview. `release_without_outcome` has no time and never
    /// touches the day statistics.
    #[test]
    fn q2_failed_transitions_leave_no_partial_change_across_midnight() {
        let day0 = 86_400 - 1;
        let day1 = 86_400;
        type Op = fn(&mut Safety, u64) -> Result<(), LedgerViolation>;
        // (operation, violation, also a violation after midnight, timed: rolls the day)
        let violations: [(Op, LedgerViolation, bool, bool); 5] = [
            (
                |s, t| s.record_outcome(t, true, Some(0), 80, 0, 0),
                LedgerViolation::CapitalReleaseExceedsReserved,
                true,
                true,
            ),
            (
                |s, t| s.record_outcome(t, true, Some(0), 50, 0, 11),
                LedgerViolation::InventoryReleaseExceedsReserved,
                true,
                true,
            ),
            (
                |s, t| s.record_outcome(t, true, Some(0), 50, -5, 10),
                LedgerViolation::InventoryNegative,
                true,
                true,
            ),
            (
                |s, t| s.record_outcome(t, false, Some(i128::MIN), 50, 0, 10),
                // Only on the same day: after the roll the day's PnL starts at 0 again.
                LedgerViolation::PnlOverflow,
                false,
                true,
            ),
            (
                |s, _| s.release_without_outcome(51, 10),
                LedgerViolation::CapitalReleaseExceedsReserved,
                true,
                false,
            ),
        ];
        for (at, label) in [(day0, "same day"), (day1, "after UTC midnight")] {
            for (i, (op, expected, after_midnight, timed)) in violations.iter().enumerate() {
                if at == day1 && !after_midnight {
                    continue;
                }
                // A ledger with one reserved attempt and a realized loss earlier that day.
                let mut s = Safety::new(
                    Limits {
                        max_outstanding_attempts: 3,
                        ..lim()
                    },
                    allow(),
                    true,
                );
                s.check(day0, 50, 0, 1, 1, &[], "longtail").unwrap();
                assert_eq!(s.record_outcome(day0, false, Some(-7), 50, 0, 0), Ok(()));
                s.check(day0, 50, 10, 1, 1, &[], "longtail").unwrap();
                let mut reference = Safety::new(
                    Limits {
                        max_outstanding_attempts: 3,
                        ..lim()
                    },
                    allow(),
                    true,
                );
                reference.check(day0, 50, 0, 1, 1, &[], "longtail").unwrap();
                assert_eq!(
                    reference.record_outcome(day0, false, Some(-7), 50, 0, 0),
                    Ok(())
                );
                reference
                    .check(day0, 50, 10, 1, 1, &[], "longtail")
                    .unwrap();
                // The reference makes only the call every timed caller at `at` makes: the day
                // roll.
                if *timed {
                    let _ = reference.preview(at, 1, 0, 1, 1, &[], "longtail");
                }
                assert_eq!(op(&mut s, at), Err(*expected), "{label} {i}");
                let (o, p, c, io, ir, day, halted) = ledger(&s);
                let (ro, rp, rc, rio, rir, rday, _) = ledger(&reference);
                assert_eq!(
                    (o, p, c, io, ir, day),
                    (ro, rp, rc, rio, rir, rday),
                    "{label} {i}"
                );
                assert_eq!(halted, Some(HaltReason::LedgerInvariant), "{label} {i}");
            }
        }
    }

    #[test]
    fn limits_and_halts() {
        let mut s = Safety::new(lim(), allow(), true);
        assert_eq!(
            s.check(0, 200, 0, 1, 1, &[], "longtail"),
            Err(Deny::Exposure)
        );
        assert_eq!(s.check(0, 50, 0, 11, 1, &[], "longtail"), Err(Deny::Tip));
        assert_eq!(
            s.check(0, 50, 0, 1, 101, &[], "longtail"),
            Err(Deny::Slippage)
        );
        assert_eq!(
            s.check(0, 50, 0, 1, 1, &[], "race"),
            Err(Deny::ExecutionClassNotAllowed)
        );
        assert_eq!(
            s.check(
                0,
                50,
                0,
                1,
                1,
                &[solana_sdk::pubkey::Pubkey::new_unique()],
                "longtail"
            ),
            Err(Deny::AssetNotAllowed)
        );
        assert_eq!(s.check(0, 50, 0, 1, 1, &[], "longtail"), Ok(()));
        assert_eq!(
            s.check(0, 50, 0, 1, 1, &[], "longtail"),
            Err(Deny::Outstanding)
        );
        s.record_outcome(0, false, Some(-30), 50, 0, 0).unwrap();
        assert_eq!(s.check(0, 50, 0, 1, 1, &[], "longtail"), Ok(()));
        s.release_without_outcome(50, 0).unwrap();
        assert_eq!(s.capital_in_use(), 0);
        s.check(0, 50, 0, 1, 1, &[], "longtail").unwrap();
        s.record_outcome(0, false, Some(-30), 50, 0, 0).unwrap(); // Tagesverlust 60 ≥ 50 → HALT
        assert!(matches!(
            s.check(0, 50, 0, 1, 1, &[], "longtail"),
            Err(Deny::Halted(_))
        ));
    }

    /// Q2 (bugs_b): every ledger transition is checked. A double release, a release above the
    /// reservation, an inventory that would turn negative and a PnL overflow halt the safety
    /// layer and change nothing (the holds stay) — never a silent clamp or wrap.
    #[test]
    fn q2_ledger_violations_halt_and_change_nothing() {
        let halted = Some(HaltReason::LedgerInvariant);
        // Double release of one attempt.
        let mut s = Safety::new(lim(), allow(), true);
        s.check(0, 50, 0, 1, 1, &[], "longtail").unwrap();
        assert_eq!(s.record_outcome(0, true, Some(0), 50, 0, 0), Ok(()));
        assert!(s.halted().is_none() && s.capital_in_use() == 0);
        assert_eq!(
            s.record_outcome(0, true, Some(0), 50, 0, 0),
            Err(LedgerViolation::OutstandingUnderflow)
        );
        assert_eq!(s.halted(), halted, "double release halts");
        // Release above the reserved capital: halts, the reservation stays.
        let mut s = Safety::new(lim(), allow(), true);
        s.check(0, 50, 0, 1, 1, &[], "longtail").unwrap();
        assert_eq!(
            s.release_without_outcome(80, 0),
            Err(LedgerViolation::CapitalReleaseExceedsReserved)
        );
        assert_eq!(s.halted(), halted, "release above reservation halts");
        assert_eq!(s.capital_in_use(), 50, "reservation unchanged");
        // Inventory hold released above its reservation.
        let mut s = Safety::new(lim(), allow(), true);
        s.check(0, 50, 10, 1, 1, &[], "longtail").unwrap();
        assert_eq!(
            s.release_without_outcome(50, 11),
            Err(LedgerViolation::InventoryReleaseExceedsReserved)
        );
        assert_eq!(s.capital_in_use(), 50);
        // Inventory delta below zero: halts instead of clamping to 0.
        let mut s = Safety::new(lim(), allow(), true);
        s.check(0, 50, 0, 1, 1, &[], "longtail").unwrap();
        assert_eq!(
            s.record_outcome(0, true, Some(0), 50, -5, 0),
            Err(LedgerViolation::InventoryNegative)
        );
        assert_eq!(s.halted(), halted, "negative inventory halts");
        assert_eq!(s.capital_in_use(), 50, "nothing applied");
        // Realized PnL overflow: halts instead of wrapping or panicking.
        let mut s = Safety::new(
            Limits {
                max_daily_loss: u64::MAX,
                max_outstanding_attempts: 2,
                ..lim()
            },
            allow(),
            true,
        );
        s.check(0, 50, 0, 1, 1, &[], "longtail").unwrap();
        s.check(0, 50, 0, 1, 1, &[], "longtail").unwrap();
        assert_eq!(s.record_outcome(0, true, Some(i128::MAX), 50, 0, 0), Ok(()));
        assert_eq!(
            s.record_outcome(0, true, Some(1), 50, 0, 0),
            Err(LedgerViolation::PnlOverflow)
        );
        assert_eq!(s.halted(), halted, "pnl overflow halts");
        assert_eq!(s.capital_in_use(), 50, "the second release not applied");
        assert!(matches!(
            s.check(0, 1, 0, 1, 1, &[], "longtail"),
            Err(Deny::Halted(HaltReason::LedgerInvariant))
        ));
        // A regular outcome after a valid reservation still balances to zero.
        let mut s = Safety::new(lim(), allow(), true);
        s.check(0, 50, 10, 1, 1, &[], "longtail").unwrap();
        assert_eq!(s.record_outcome(0, true, Some(3), 50, 4, 10), Ok(()));
        assert_eq!(s.capital_in_use(), 0);
        assert!(s.halted().is_none());
    }

    #[test]
    fn preview_does_not_reserve_capital() {
        let mut s = Safety::new(lim(), allow(), true);
        assert_eq!(s.preview(0, 50, 0, 1, 1, &[], "longtail"), Ok(()));
        assert_eq!(s.capital_in_use(), 0);
        assert_eq!(s.check(0, 50, 0, 1, 1, &[], "longtail"), Ok(()));
        assert_eq!(s.capital_in_use(), 50);
    }

    #[test]
    fn unknown_outcome_keeps_hold_until_complete_reconciliation() {
        let mut s = Safety::new(lim(), allow(), true);
        s.check(0, 50, 0, 1, 1, &[], "longtail").unwrap();
        s.record_outcome(0, true, None, 50, 0, 0).unwrap();
        assert_eq!(s.capital_in_use(), 50);
        assert_eq!(s.outstanding, 1);
        assert_eq!(s.open_positions, 1);
        assert_eq!(s.halted(), Some(HaltReason::UnknownEconomics));
        s.record_outcome(0, true, Some(5), 50, 0, 0).unwrap();
        assert_eq!(s.capital_in_use(), 0);
        assert_eq!(s.outstanding, 0);
        assert_eq!(s.halted(), Some(HaltReason::UnknownEconomics));
    }

    /// B15: the inventory limit covers held inventory, the worst-case hold of every in-flight
    /// attempt and the new attempt; a hold is released exactly once by its outcome.
    #[test]
    fn b15_inventory_limit_counts_worst_case_new_and_in_flight_holds() {
        let mut l = lim();
        l.max_outstanding_inventory = 100;
        l.max_outstanding_attempts = 3;
        let mut s = Safety::new(l, allow(), true);
        // Exactly at the limit is allowed, one more is not.
        assert_eq!(s.preview(0, 10, 100, 1, 1, &[], "longtail"), Ok(()));
        assert_eq!(
            s.preview(0, 10, 101, 1, 1, &[], "longtail"),
            Err(Deny::Inventory)
        );
        // An in-flight hold counts against a concurrent attempt (never reserved twice).
        s.check(0, 10, 60, 1, 1, &[], "longtail").unwrap();
        assert_eq!(
            s.check(0, 10, 41, 1, 1, &[], "longtail"),
            Err(Deny::Inventory)
        );
        s.check(0, 10, 40, 1, 1, &[], "longtail").unwrap();
        assert_eq!(
            s.preview(0, 10, 1, 1, 1, &[], "longtail"),
            Err(Deny::Inventory)
        );
        // A release before handoff returns its hold; a known outcome returns its hold and books
        // the actually held inventory instead.
        s.release_without_outcome(10, 40).unwrap();
        assert_eq!(s.inventory_reserved, 60);
        s.record_outcome(0, true, Some(1), 10, 60, 60).unwrap();
        assert_eq!((s.inventory_reserved, s.inventory_outstanding), (0, 60));
        assert_eq!(
            s.preview(0, 10, 41, 1, 1, &[], "longtail"),
            Err(Deny::Inventory)
        );
        assert_eq!(s.preview(0, 10, 40, 1, 1, &[], "longtail"), Ok(()));
        // UNKNOWN keeps the hold (and halts).
        let mut u = Safety::new(l, allow(), true);
        u.check(0, 10, 70, 1, 1, &[], "longtail").unwrap();
        u.record_outcome(0, true, None, 10, 0, 70).unwrap();
        assert_eq!(u.inventory_reserved, 70);
        assert!(u.halted().is_some());
        // An unrepresentable worst case is a denial, never a wrap.
        assert_eq!(
            s.preview(0, 10, u64::MAX, 1, 1, &[], "longtail"),
            Err(Deny::Inventory)
        );
    }

    #[test]
    fn live_disabled_and_day_roll() {
        let mut s = Safety::new(lim(), allow(), false);
        assert_eq!(
            s.check(0, 1, 0, 1, 1, &[], "longtail"),
            Err(Deny::LiveDisabled)
        );
        let mut s = Safety::new(lim(), allow(), true);
        s.check(0, 1, 0, 1, 1, &[], "longtail").unwrap();
        s.record_outcome(0, false, Some(-10), 1, 0, 0).unwrap();
        assert_eq!(s.day().failed_attempts, 1);
        s.check(86_400, 1, 0, 1, 1, &[], "longtail").unwrap();
        assert_eq!(s.day().failed_attempts, 0, "neuer Tag");
    }
}

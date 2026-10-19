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
        if self.capital_in_use.saturating_add(exposure) > self.limits.max_total_capital {
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
        if -self.day.realized_pnl >= self.limits.max_daily_loss as i128 {
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
        self.outstanding += 1;
        self.open_positions += 1;
        self.capital_in_use = self.capital_in_use.saturating_add(exposure);
        self.inventory_reserved = self.inventory_reserved.saturating_add(new_inventory);
        self.day.attempts += 1;
        Ok(())
    }

    /// Reconciled-Ergebnis eines Versuchs. `realized_pnl` ist der **echte** Netto-Wert aus
    /// Token-Deltas, Fees, Tips — nicht der geplante.
    /// `exposure_released`: freigegebenes Kapital; `inventory_delta`: gehaltenes Collateral (NO_SWAP) ±;
    /// `inventory_hold`: the worst-case hold `check()` reserved for this attempt (B15).
    /// [FIX-24] `realized_pnl: None` = UNKNOWN → Buchhaltung bleibt offen und HALT(UnknownEconomics):
    /// ein unbekannter Ausgang hält Reservierungen und darf das Tagesbudget nicht besser aussehen lassen.
    pub fn record_outcome(
        &mut self,
        now_unix_s: u64,
        landed_success: bool,
        realized_pnl: Option<i128>,
        exposure_released: u64,
        inventory_delta: i128,
        inventory_hold: u64,
    ) {
        self.roll_day(now_unix_s);
        // UNKNOWN cannot release capital/inventory or reduce outstanding count.
        // A later complete reconciliation may close it, but never clears HALT.
        if realized_pnl.is_none() {
            self.halt(HaltReason::UnknownEconomics);
            return;
        }
        self.outstanding = self.outstanding.saturating_sub(1);
        self.open_positions = self.open_positions.saturating_sub(1);
        self.capital_in_use = self.capital_in_use.saturating_sub(exposure_released);
        self.inventory_reserved = self.inventory_reserved.saturating_sub(inventory_hold);
        self.inventory_outstanding =
            (self.inventory_outstanding as i128 + inventory_delta).max(0) as u64;
        match realized_pnl {
            Some(p) => self.day.realized_pnl += p,
            None => self.halt(HaltReason::UnknownEconomics),
        }
        if !landed_success {
            self.day.failed_attempts += 1;
        }
        if -self.day.realized_pnl >= self.limits.max_daily_loss as i128 {
            self.halt(HaltReason::LimitBreached); // [FIX-12]
        }
        if self.day.failed_attempts >= self.limits.max_failed_attempts_per_day {
            self.halt(HaltReason::LimitBreached); // [FIX-12]
        }
    }

    pub fn day(&self) -> DayStats {
        self.day
    }

    /// In-flight-Kapital: `check()` reserviert `exposure`; dieser Pfad gibt es bei einem Fehlschlag
    /// **vor** Submission (Build/Sim/Deadline) deterministisch frei, ohne Tagesstatistik zu verändern.
    pub fn release_without_outcome(&mut self, exposure: u64, inventory_hold: u64) {
        self.outstanding = self.outstanding.saturating_sub(1);
        self.open_positions = self.open_positions.saturating_sub(1);
        self.capital_in_use = self.capital_in_use.saturating_sub(exposure);
        self.inventory_reserved = self.inventory_reserved.saturating_sub(inventory_hold);
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
        s.record_outcome(0, false, Some(-30), 50, 0, 0);
        assert_eq!(s.check(0, 50, 0, 1, 1, &[], "longtail"), Ok(()));
        s.release_without_outcome(50, 0);
        assert_eq!(s.capital_in_use(), 0);
        s.check(0, 50, 0, 1, 1, &[], "longtail").unwrap();
        s.record_outcome(0, false, Some(-30), 50, 0, 0); // Tagesverlust 60 ≥ 50 → HALT
        assert!(matches!(
            s.check(0, 50, 0, 1, 1, &[], "longtail"),
            Err(Deny::Halted(_))
        ));
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
        s.record_outcome(0, true, None, 50, 0, 0);
        assert_eq!(s.capital_in_use(), 50);
        assert_eq!(s.outstanding, 1);
        assert_eq!(s.open_positions, 1);
        assert_eq!(s.halted(), Some(HaltReason::UnknownEconomics));
        s.record_outcome(0, true, Some(5), 50, 0, 0);
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
        // UNKNOWN keeps the hold; a release before handoff and a known outcome return it.
        s.record_outcome(0, true, None, 10, 0, 40);
        assert_eq!(s.inventory_reserved, 100);
        s.release_without_outcome(10, 40);
        assert_eq!(s.inventory_reserved, 60);
        s.halted = None;
        s.record_outcome(0, true, Some(1), 10, 60, 60);
        assert_eq!((s.inventory_reserved, s.inventory_outstanding), (0, 60));
        assert_eq!(
            s.preview(0, 10, 41, 1, 1, &[], "longtail"),
            Err(Deny::Inventory)
        );
        assert_eq!(s.preview(0, 10, 40, 1, 1, &[], "longtail"), Ok(()));
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
        s.record_outcome(0, false, Some(-10), 1, 0, 0);
        assert_eq!(s.day().failed_attempts, 1);
        s.check(86_400, 1, 0, 1, 1, &[], "longtail").unwrap();
        assert_eq!(s.day().failed_attempts, 0, "neuer Tag");
    }
}

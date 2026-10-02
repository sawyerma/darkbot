//! [M3] Virtuelles Shadow-Ledger (P6-LT). Aggregiert pro Tag; jede Zeile `pnl_complete=false`.
//! Kein Einfluss auf Entscheidungen. Nie einen ungemessenen Wert als 0: Felder sind Zähler
//! (gemessen) oder Summen über **bekannte** erwartete Nettos.

use crate::record::{Decision, LossReason, OpportunityRecord};
use darkbot_common::histogram::Histogram;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Default, Serialize, Clone)]
pub struct DaySummary {
    pub day_index: u64,
    pub account_updates: u64,
    pub health_checked: u64,
    pub liquidatable: u64,
    pub unsupported_fail_closed: u64,
    pub plans_built: u64,
    pub plans_rejected: u64,
    pub sim_pass: u64,
    pub sim_fail: u64,
    pub shadow_ready: u64,
    pub shadow_technical_ready: u64,
    pub shadow_technical_sim_failed: u64,
    pub shadow_wins: u64,
    /// [FIX-45] gleicher Slot wie der Gewinner: kein Win, kein Verlust.
    pub same_slot_unknown: u64,
    pub losses: HashMap<String, u64>,
    pub expected_net_wins_sum: i128,
    pub expected_net_wins_count: u64,
    pub pnl_complete: bool,
    pub lat_rx_detect: String,
    pub lat_detect_ready: String,
    pub lat_ready_sign: String,
}

pub struct ShadowLedger {
    day: DaySummary,
    h_rx_detect: Histogram,
    h_detect_ready: Histogram,
    h_ready_sign: Histogram,
}

impl Default for ShadowLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl ShadowLedger {
    pub fn new() -> Self {
        ShadowLedger { day: DaySummary { pnl_complete: false, ..Default::default() }, h_rx_detect: Histogram::new(), h_detect_ready: Histogram::new(), h_ready_sign: Histogram::new() }
    }

    fn roll(&mut self, now_unix_s: u64) -> Option<DaySummary> {
        let idx = now_unix_s / 86_400;
        if idx != self.day.day_index {
            let mut done = self.finish();
            done.day_index = self.day.day_index;
            self.day = DaySummary { day_index: idx, pnl_complete: false, ..Default::default() };
            self.h_rx_detect.reset();
            self.h_detect_ready.reset();
            self.h_ready_sign.reset();
            return if done.account_updates > 0 || done.health_checked > 0 { Some(done) } else { None };
        }
        None
    }

    pub fn on_account_update(&mut self, now_unix_s: u64) -> Option<DaySummary> {
        let r = self.roll(now_unix_s);
        self.day.account_updates += 1;
        r
    }

    pub fn on_health(&mut self, liquidatable: bool, unsupported: bool) {
        self.day.health_checked += 1;
        if liquidatable { self.day.liquidatable += 1; }
        if unsupported { self.day.unsupported_fail_closed += 1; }
    }

    pub fn on_record(&mut self, rec: &OpportunityRecord, sim_ok: Option<bool>) {
        match rec.decision {
            Decision::ShadowReady => { self.day.shadow_ready += 1; self.day.plans_built += 1; }
            Decision::ShadowTechnicalReady => { self.day.shadow_technical_ready += 1; self.day.plans_built += 1; }
            Decision::ShadowSimFailed => { self.day.plans_built += 1; }
            Decision::ShadowTechnicalSimFailed => { self.day.shadow_technical_sim_failed += 1; self.day.plans_built += 1; }
            Decision::Skip | Decision::Unavailable => { self.day.plans_rejected += 1; }
            _ => {}
        }
        match sim_ok { Some(true) => self.day.sim_pass += 1, Some(false) => self.day.sim_fail += 1, None => {} }
        for (name, v) in &rec.intervals_ns {
            if let Some(v) = v {
                match name.as_str() {
                    "T_rx→T_detect" => self.h_rx_detect.record(*v),
                    "T_detect→T_ready" => self.h_detect_ready.record(*v),
                    "T_ready→T_sign" => self.h_ready_sign.record(*v),
                    _ => {}
                }
            }
        }
    }

    pub fn on_same_slot(&mut self) { self.day.same_slot_unknown += 1; }

    pub fn on_race(&mut self, win: bool, loss: Option<LossReason>, expected_net: Option<i128>) {
        if win {
            self.day.shadow_wins += 1;
            if let Some(n) = expected_net { self.day.expected_net_wins_sum += n; self.day.expected_net_wins_count += 1; }
        } else if let Some(l) = loss {
            *self.day.losses.entry(format!("{:?}", l)).or_insert(0) += 1;
        }
    }

    pub fn finish(&self) -> DaySummary {
        let mut d = self.day.clone();
        d.lat_rx_detect = self.h_rx_detect.summary().to_string();
        d.lat_detect_ready = self.h_detect_ready.summary().to_string();
        d.lat_ready_sign = self.h_ready_sign.summary().to_string();
        d.pnl_complete = false;
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_and_rolls() {
        let mut l = ShadowLedger::new();
        assert!(l.on_account_update(10).is_none());
        l.on_health(true, false);
        l.on_race(true, None, Some(120));
        l.on_race(false, Some(LossReason::Ingress), None);
        let d = l.finish();
        assert_eq!(d.liquidatable, 1);
        assert_eq!(d.shadow_wins, 1);
        assert_eq!(d.expected_net_wins_sum, 120);
        assert_eq!(d.losses.get("Ingress"), Some(&1));
        assert!(!d.pnl_complete);
        let rolled = l.on_account_update(10 + 86_400).expect("Tageswechsel liefert Zusammenfassung");
        assert_eq!(rolled.shadow_wins, 1);
    }
}

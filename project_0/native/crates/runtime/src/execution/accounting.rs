//! FE-04/FE-07 finalized accounting: the cumulative realized result of every own attempt of a
//! wallet, read only from its FE-04 journals (archived attempts and the active one) — the single
//! financial authority. Realized is the attempt's SOL delta and token outflows plus its exits'
//! proceeds; open inventory is a mark, never realized; anything not settled is UNKNOWN. No
//! profit is claimed without every attempt and exit settled, no position left and a positive
//! cumulative sum.
use crate::execution::one_shot::{exit::ExitSettlement, exit::Position, settlement::Settlement};
use std::collections::BTreeMap;

/// The state of one attempt or exit in its journal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    /// Durably reserved, not handed off (a restart makes it UNKNOWN).
    Reserved,
    /// Handed off; the outcome is not finalized.
    Unknown,
    /// Finalized and consistent; the booking still waits (e.g. a value).
    Finalized,
    Settled,
}
impl StepState {
    pub fn name(self) -> &'static str {
        match self {
            StepState::Reserved => "RESERVED",
            StepState::Unknown => "UNKNOWN",
            StepState::Finalized => "FINALIZED_UNBOOKED",
            StepState::Settled => "SETTLED",
        }
    }
}

/// One exit of an attempt's open inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExitRecord {
    pub approval_id: String,
    pub mint: String,
    pub state: StepState,
    pub settlement: Option<ExitSettlement>,
}

/// One own attempt with its exits, as its journal holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttemptRecord {
    pub approval_id: String,
    pub target: String,
    pub funding_class: String,
    pub archived: bool,
    pub state: StepState,
    pub settlement: Option<Settlement>,
    pub exits: Vec<ExitRecord>,
    /// The open inventory after the settled exits.
    pub open_positions: BTreeMap<String, Position>,
}

/// FE-07: the cumulative decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// No attempt landed (none made, or every one proven not landed): nothing was paid.
    NoTrade,
    FirstRealizedProfitEvidenced,
    Loss,
    /// Something is not settled, a position is open or a sum overflowed.
    Unknown(Vec<String>),
}
impl Verdict {
    pub fn name(&self) -> &'static str {
        match self {
            Verdict::NoTrade => "NO_TRADE",
            Verdict::FirstRealizedProfitEvidenced => "FIRST_REALIZED_PROFIT_EVIDENCED",
            Verdict::Loss => "LOSS",
            Verdict::Unknown(_) => "UNKNOWN",
        }
    }
}

/// Sums over the settled steps only (an unsettled step is in `Verdict::Unknown`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub attempts: usize,
    pub archived: usize,
    pub landed_attempts: usize,
    pub settled_exits: usize,
    /// `None` on overflow.
    pub realized_net_usd_micro: Option<i128>,
    pub fee_lamports: Option<u64>,
    pub open_inventory_mark_usd_micro: Option<i128>,
}

/// Every own attempt of one wallet, oldest archived first, the active one last.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    pub attempts: Vec<AttemptRecord>,
}

impl Ledger {
    /// Reads the archive and the active journal (read-only, no wallet lock). A journal that
    /// cannot be read is an error, never an empty ledger.
    pub fn load(directory: &std::path::Path, wallet: &str) -> Result<Self, String> {
        crate::execution::one_shot::archive::records(directory, wallet)
            .map(|attempts| Self { attempts })
    }

    pub fn totals(&self) -> Totals {
        let mut t = Totals {
            attempts: self.attempts.len(),
            archived: self.attempts.iter().filter(|a| a.archived).count(),
            realized_net_usd_micro: Some(0),
            fee_lamports: Some(0),
            open_inventory_mark_usd_micro: Some(0),
            ..Default::default()
        };
        for a in &self.attempts {
            if let Some(s) = &a.settlement {
                t.landed_attempts += usize::from(s.landed);
                t.realized_net_usd_micro = t
                    .realized_net_usd_micro
                    .and_then(|r| r.checked_add(s.realized_net_usd_micro));
                t.fee_lamports = t.fee_lamports.and_then(|f| f.checked_add(s.fee_lamports));
            }
            for e in &a.exits {
                if let Some(x) = &e.settlement {
                    t.settled_exits += 1;
                    t.realized_net_usd_micro = t
                        .realized_net_usd_micro
                        .and_then(|r| r.checked_add(x.realized_usd_micro));
                    t.fee_lamports = t.fee_lamports.and_then(|f| f.checked_add(x.fee_lamports));
                }
            }
            for p in a.open_positions.values() {
                t.open_inventory_mark_usd_micro = t
                    .open_inventory_mark_usd_micro
                    .and_then(|m| m.checked_add(p.mark_usd_micro));
            }
        }
        t
    }

    /// FE-07: `FIRST_REALIZED_PROFIT_EVIDENCED` only with every attempt and exit settled, no
    /// position left and a positive cumulative realized net; `NO_TRADE` when nothing landed.
    pub fn verdict(&self) -> Verdict {
        let mut unknown = Vec::new();
        for a in &self.attempts {
            if a.state != StepState::Settled {
                unknown.push(format!("ATTEMPT_{}:{}", a.state.name(), a.approval_id));
            }
            for e in &a.exits {
                if e.state != StepState::Settled {
                    unknown.push(format!("EXIT_{}:{}", e.state.name(), e.approval_id));
                }
            }
            for (mint, p) in &a.open_positions {
                if p.units != 0 {
                    unknown.push(format!("OPEN_INVENTORY:{}:{mint}", a.approval_id));
                }
            }
        }
        let totals = self.totals();
        let Some(realized) = totals.realized_net_usd_micro else {
            unknown.push("REALIZED_OVERFLOW".into());
            return Verdict::Unknown(unknown);
        };
        if !unknown.is_empty() {
            return Verdict::Unknown(unknown);
        }
        if totals.landed_attempts == 0 && realized == 0 {
            return Verdict::NoTrade;
        }
        if realized > 0 {
            Verdict::FirstRealizedProfitEvidenced
        } else {
            Verdict::Loss
        }
    }

    /// The terminal report (text lines, offline): every attempt and exit, the sums, the verdict.
    pub fn report(&self) -> Vec<String> {
        let opt = |v: Option<i128>| v.map_or("UNKNOWN".to_string(), |v| v.to_string());
        let mut out = Vec::new();
        for a in &self.attempts {
            let s = a.settlement.as_ref();
            out.push(format!(
                "attempt={} archived={} state={} class={} target={} landed={} success={} realized_net_usd_micro={} fee_lamports={} exits={} open_positions={}",
                a.approval_id,
                a.archived,
                a.state.name(),
                a.funding_class,
                a.target,
                s.map_or("-".into(), |s| s.landed.to_string()),
                s.map_or("-".into(), |s| s.landed_success.to_string()),
                opt(s.map(|s| s.realized_net_usd_micro)),
                s.map_or("UNKNOWN".into(), |s| s.fee_lamports.to_string()),
                a.exits.len(),
                a.open_positions.values().filter(|p| p.units != 0).count(),
            ));
            for e in &a.exits {
                let x = e.settlement.as_ref();
                out.push(format!(
                    "exit={} attempt={} mint={} state={} sold_units={} realized_usd_micro={}",
                    e.approval_id,
                    a.approval_id,
                    e.mint,
                    e.state.name(),
                    x.map_or("-".into(), |x| x.sold_units.to_string()),
                    opt(x.map(|x| x.realized_usd_micro)),
                ));
            }
        }
        let t = self.totals();
        out.push(format!(
            "attempts={} archived={} landed_attempts={} settled_exits={}",
            t.attempts, t.archived, t.landed_attempts, t.settled_exits
        ));
        out.push(format!(
            "cumulative_realized_net_usd_micro={} fee_lamports={} open_inventory_mark_usd_micro={}",
            opt(t.realized_net_usd_micro),
            t.fee_lamports.map_or("UNKNOWN".into(), |f| f.to_string()),
            opt(t.open_inventory_mark_usd_micro),
        ));
        let verdict = self.verdict();
        out.push(match &verdict {
            Verdict::Unknown(reasons) => format!("verdict=UNKNOWN reasons={}", reasons.join(",")),
            v => format!("verdict={}", v.name()),
        });
        out
    }
}

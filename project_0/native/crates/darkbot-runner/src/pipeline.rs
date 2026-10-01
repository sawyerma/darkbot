//! Die Kette. Jede Stufe setzt ihren Timing-Punkt; Fehler sind Records, keine Panics.

use crate::record::{Decision, LossReason, OpportunityRecord};
use darkbot_common::timing::{Clock, Stage, Timeline};
use darkbot_project0::economics::{self, CandidateInput, Evaluation, FailedCostMatrix, Path};
use darkbot_project0::health::{HealthEngine, HealthError};
use darkbot_project0::planner::{self, Allowlist, RouteKind, Step};
use darkbot_project0::safety::Safety;
use darkbot_project0::execution::Freshness;
use darkbot_solana::state::blockhash::BlockhashCache;
use darkbot_solana::submit::{SubmissionAdapter, SubmissionPath};
use darkbot_solana::tx::{self, BuildVersion, Resources, TxLimits, TxPlan};
use solana_sdk::address_lookup_table::AddressLookupTableAccount;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Shadow,
    Live,
}

/// Was der Receivership-Builder (P5-06) liefern muss. Implementierung gegen das
/// marginfi-Crate/IDL; hier nur der Contract.
pub struct ReceivershipPlan {
    pub steps: Vec<Step>,
    pub instructions: Vec<Instruction>,
    pub route: RouteKind,
    pub input: CandidateInput,
    pub premium_bps: u64,
    pub exposure: u64,
    pub tip: u64,
    pub slippage_bps: u64,
    /// Beteiligte Asset-Mints (Safety-Allowlist).
    pub assets: Vec<Pubkey>,
    /// TX-Klasse ist Bestandteil des Execution-Candidates; v0 kann Route-LUTs nutzen.
    pub tx_version: BuildVersion,
    pub lookup_tables: Vec<AddressLookupTableAccount>,
}

pub trait ReceivershipBuilder {
    /// Liefert **alle** technisch machbaren Funding-/Route-Candidates. Keine harte
    /// Funding-Präferenz im Builder; P4A + TX-Feasibility entscheiden danach.
    fn plans(&self, account: &Pubkey, now_unix_s: u64) -> Result<Vec<ReceivershipPlan>, String>;
}

pub struct Pipeline<'a, H: HealthEngine, B: ReceivershipBuilder, S: SubmissionAdapter> {
    pub mode: Mode,
    pub clock: &'a Clock,
    pub health: &'a H,
    pub builder: &'a B,
    pub submitter: &'a S,
    pub safety: &'a mut Safety,
    pub blockhash: &'a BlockhashCache,
    pub payer: &'a Keypair,
    pub allowlist: &'a Allowlist,
    pub failed_costs: &'a FailedCostMatrix,
    pub limits: TxLimits,
    pub resources: Resources,
    pub min_profit: i128,
    pub blockhash_max_age_ns: u64,
    pub p7_passed: bool,
    pub freshness: Freshness,
}

impl<'a, H: HealthEngine, B: ReceivershipBuilder, S: SubmissionAdapter> Pipeline<'a, H, B, S> {
    /// Ein Kandidat, ein Trigger → ein Record. T_rx wird vom Aufrufer gesetzt.
    pub fn evaluate(&mut self, account: Pubkey, trigger: &str, t_rx_ns: u64, now_unix_s: u64, slot: u64, state_valid: bool) -> OpportunityRecord {
        let mut tl = Timeline::new();
        tl.mark(Stage::Rx, t_rx_ns);
        let mut rec = OpportunityRecord {
            account: account.to_string(),
            trigger: trigger.to_string(),
            decision: Decision::Skip,
            net_profit: None,
            premium_bps: None,
            t_profitable_unix_s: None,
            tx_version: None,
            serialized_bytes: None,
            locked_accounts: None,
            path: None,
            funding: None,
            signature: None,
            intervals_ns: vec![],
            reason: None,
            loss: None,
        };

        if !state_valid {
            rec.loss = Some(LossReason::State);
            rec.reason = Some("state not tradable".into());
            return rec.with_timeline(&tl);
        }

        // Health (P3)
        let hv = match self.health.health(&account, now_unix_s as i64, slot) {
            Ok(h) => h,
            Err(HealthError::Unavailable) => {
                rec.decision = Decision::Unavailable;
                rec.loss = Some(LossReason::Health);
                rec.reason = Some("health engine unavailable".into());
                return rec.with_timeline(&tl);
            }
            Err(e) => {
                rec.loss = Some(LossReason::Health);
                rec.reason = Some(e.to_string());
                return rec.with_timeline(&tl);
            }
        };
        if !hv.liquidatable {
            rec.reason = Some("not liquidatable".into());
            return rec.with_timeline(&tl);
        }
        tl.mark(Stage::State, self.clock.now_ns());

        // P5 + P4A: Builder liefert alle technisch machbaren Funding-/Route-Klassen.
        // Es gibt bewusst **keine** feste Funding-Präferenz. Erst Protocol-Validität,
        // deterministische Economics und echte TX-Baubarkeit entscheiden.
        let plans = match self.builder.plans(&account, now_unix_s) {
            Ok(p) if !p.is_empty() => p,
            Ok(_) => {
                rec.loss = Some(LossReason::Routing);
                rec.reason = Some("no feasible execution candidates".into());
                return rec.with_timeline(&tl);
            }
            Err(e) => {
                rec.decision = if e.contains("unavailable") { Decision::Unavailable } else { Decision::Skip };
                rec.loss = Some(LossReason::Routing);
                rec.reason = Some(e);
                return rec.with_timeline(&tl);
            }
        };

        let Some(bh) = self.blockhash.fresh(self.clock.now_ns(), self.blockhash_max_age_ns) else {
            rec.loss = Some(LossReason::Build);
            rec.reason = Some("blockhash stale".into());
            return rec.with_timeline(&tl);
        };

        let mut best: Option<(ReceivershipPlan, darkbot_solana::tx::BuiltTx, Evaluation)> = None;
        let mut rejected = Vec::new();
        for plan in plans {
            if let Err(e) = planner::validate_receivership(&plan.steps, self.allowlist, plan.route, self.p7_passed) {
                rejected.push(format!("{}: plan invalid: {:?}", plan.input.funding.as_str(), e));
                continue;
            }

            let eval = economics::evaluate(&plan.input, self.failed_costs);
            // Shadow muss den finalen Pfad messen können, auch wenn LIVE absichtlich deaktiviert
            // ist. Safety ist dort Evidence, kein Send-Gate. Erst LIVE previewt die Limits
            // nicht-reservierend, damit bei einem Safety-Verstoß eine andere Funding-Klasse
            // derselben Opportunity gewählt werden kann.
            let safety_valid = match self.mode {
                Mode::Shadow => true,
                Mode::Live => match self.safety.preview(
                    now_unix_s,
                    plan.exposure,
                    plan.tip,
                    plan.slippage_bps,
                    &plan.assets,
                    "longtail",
                ) {
                    Ok(()) => true,
                    Err(deny) => {
                        rejected.push(format!(
                            "{}: safety {:?}",
                            plan.input.funding.as_str(),
                            deny
                        ));
                        false
                    }
                },
            };
            if !economics::actionable(eval, self.min_profit, true, state_valid, safety_valid) {
                rejected.push(format!("{}: economics {:?}", plan.input.funding.as_str(), eval));
                continue;
            }

            let txplan = TxPlan {
                version: plan.tx_version,
                payer: tx::payer_of(self.payer),
                instructions: plan.instructions.clone(),
                lookup_tables: plan.lookup_tables.clone(),
                resources: self.resources,
            };
            let built = match tx::build(&txplan, bh.hash, &self.limits) {
                Ok(b) => b,
                Err(e) => {
                    rejected.push(format!("{}: tx infeasible: {}", plan.input.funding.as_str(), e));
                    continue;
                }
            };

            let replace = best.as_ref().map_or(true, |(_, _, current)| {
                economics::compare_evaluations(eval, *current).is_gt()
            });
            if replace {
                best = Some((plan, built, eval));
            }
        }

        let Some((plan, mut built, eval)) = best else {
            rec.loss = Some(LossReason::EconomicSkip);
            rec.reason = Some(if rejected.is_empty() {
                "no actionable execution candidate".into()
            } else {
                rejected.join(" | ")
            });
            return rec.with_timeline(&tl);
        };

        rec.premium_bps = Some(plan.premium_bps);
        rec.funding = Some(plan.input.funding.as_str().to_string());
        if let Evaluation::Known { net, .. } = eval {
            rec.net_profit = Some(net);
        }
        tl.mark(Stage::Detect, self.clock.now_ns());

        rec.serialized_bytes = Some(built.serialized_len);
        rec.locked_accounts = Some(built.locked_accounts);
        rec.tx_version = Some(format!("{:?}", built.version));
        tl.mark(Stage::Ready, self.clock.now_ns());

        if let Err(e) = tx::sign(&mut built, &[self.payer]) {
            rec.loss = Some(LossReason::Build);
            rec.reason = Some(e.to_string());
            return rec.with_timeline(&tl);
        }
        tl.mark(Stage::Sign, self.clock.now_ns());

        // Drift-Muster: stale Opportunity / Deadline → nicht senden (Record bleibt für P6).
        let t_detect = tl.get(Stage::Detect).unwrap_or(t_rx_ns);
        if self.freshness.deadline_passed(t_detect, self.clock.now_ns()) {
            rec.loss = Some(LossReason::Build);
            rec.reason = Some("execution deadline passed".into());
            return rec.with_timeline(&tl);
        }
        // Send-Boundary
        if self.mode == Mode::Shadow {
            rec.decision = Decision::ShadowReady;
            return rec.with_timeline(&tl);
        }
        if let Err(d) = self.safety.check(now_unix_s, plan.exposure, plan.tip, plan.slippage_bps, &plan.assets, "longtail") {
            rec.decision = Decision::DeniedBySafety;
            rec.reason = Some(format!("{:?}", d));
            return rec.with_timeline(&tl);
        }
        let t_handoff = self.clock.now_ns();
        match self.submitter.submit(&built.tx, t_handoff) {
            Ok(ack) => {
                tl.mark(Stage::Handoff, t_handoff);
                rec.decision = Decision::Submitted;
                rec.path = Some(ack.path.as_str().to_string());
                rec.signature = Some(ack.signature.to_string());
            }
            Err(e) => {
                self.safety.record_outcome(now_unix_s, false, 0, plan.exposure, 0);
                rec.loss = Some(LossReason::Submission);
                rec.reason = Some(e.to_string());
            }
        }
        let _ = Path::Rpc;
        let _ = SubmissionPath::Rpc;
        rec.with_timeline(&tl)
    }

    /// Landing-Reconciliation (S3-06) → Safety. Muss für **jede** Submission genau einmal
    /// aufgerufen werden, sonst bleibt `outstanding` belegt.
    pub fn reconcile_outcome(&mut self, now_unix_s: u64, landed_success: bool, realized_pnl: i128, exposure_released: u64, inventory_delta: i128) {
        self.safety.record_outcome(now_unix_s, landed_success, realized_pnl, exposure_released, inventory_delta);
    }
}

//! [C0-02][C0-FIX-01] Opt-in market census (`P0_C0_CENSUS=1`).
//!
//! No second scan and no owner-side classification: the R127 startup sweep already evaluates every
//! debtor of the group once, in parallel, against one state view and one canonical chain clock. For
//! the armed batch it also emits one compact row per account; the owner moves the rows into a
//! single-slot bounded queue and a worker classifies and aggregates them. A full queue, a coverage
//! gap, a clock or FeeState error ends the attempt INCOMPLETE with its reason and keeps the request
//! for the next READY; only a complete census retires it. The cost model is validated at setup.
#![cfg_attr(not(feature = "crate_health"), allow(dead_code))]
use super::*;
use darkbot_common::channel::{Consumer, Producer};
use darkbot_project0::census::{classify, Census, CensusModel, CensusRow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CensusConfig {
    realizable_bps: u64,
    fixed_cost_usd_micro: u64,
    min_profit_usd_micro: i128,
}

pub(super) struct CensusJob {
    attempt: u32,
    clock: ChainClock,
    model: CensusModel,
    expected_accounts: usize,
    owner_batch_ms: u128,
    rows: Vec<CensusRow>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CensusOutcome {
    Complete { attempt: u32, line: String },
    Incomplete { attempt: u32, reason: String },
}

pub(super) struct CensusRequest {
    config: CensusConfig,
    attempt: u32,
    /// `Some(expected accounts)`: the next sweep batch is the census batch.
    pub(super) armed: Option<usize>,
    in_flight: bool,
    complete: bool,
    jobs: Producer<CensusJob>,
    results: Consumer<CensusOutcome>,
}

impl CensusRequest {
    /// Arm for the startup sweep of a READY, unless a census is running or already complete.
    pub(super) fn arm(&mut self, expected_accounts: usize) {
        if !self.complete && !self.in_flight {
            self.armed = Some(expected_accounts);
        }
    }
}

pub(super) fn config() -> Result<Option<CensusConfig>, Box<dyn std::error::Error>> {
    if !config::parsed_or("P0_C0_CENSUS", false)? {
        return Ok(None);
    }
    let c = CensusConfig {
        realizable_bps: config::require_parsed("P0_C0_REALIZABLE_BPS")?,
        fixed_cost_usd_micro: config::require_parsed("P0_C0_FIXED_COST_USD_MICRO")?,
        min_profit_usd_micro: config::require_parsed("P0_C0_MIN_PROFIT_USD_MICRO")?,
    };
    // Everything but the FeeState fee, which is read from canonical state at READY.
    CensusModel::new(
        0,
        c.realizable_bps,
        c.fixed_cost_usd_micro,
        c.min_profit_usd_micro,
    )?;
    Ok(Some(c))
}

pub(super) fn spawn(
    config: CensusConfig,
    shutdown: Shutdown,
) -> Result<CensusRequest, Box<dyn std::error::Error>> {
    let (jobs, job_rx) = bounded::<CensusJob>("c0-census-jobs", 1);
    let (result_tx, results) = bounded::<CensusOutcome>("c0-census-results", 1);
    std::thread::Builder::new()
        .name("c0-census".into())
        .spawn(move || {
            while !shutdown.is_requested() {
                let Some(job) = job_rx.pop_timeout(Duration::from_millis(250)) else {
                    continue;
                };
                // Single job in flight: the owner submits again only after taking this outcome.
                if result_tx.try_push(run(job)).is_err() {
                    return;
                }
            }
        })?;
    Ok(CensusRequest {
        config,
        attempt: 0,
        armed: None,
        in_flight: false,
        complete: false,
        jobs,
        results,
    })
}

/// Worker body: classification and aggregation over the rows of one batch.
pub(super) fn run(job: CensusJob) -> CensusOutcome {
    if job.rows.len() != job.expected_accounts {
        return CensusOutcome::Incomplete {
            attempt: job.attempt,
            reason: format!(
                "COVERAGE rows={} expected={}",
                job.rows.len(),
                job.expected_accounts
            ),
        };
    }
    let started = std::time::Instant::now();
    let now = job.clock.unix_s();
    let limit = darkbot_solana::tx::TxLimits::default().max_locked_accounts as u16;
    let mut census = Census::default();
    for row in &job.rows {
        census.add(row, classify(row, now, &job.model), now, limit);
    }
    let m = job.model;
    CensusOutcome::Complete {
        attempt: job.attempt,
        line: format!(
            "slot={} record_slot={} unix={} clock=CHAIN_SYSVAR fee_bps={} realizable_bps={} fixed_cost_usd_micro={} min_profit_usd_micro={} classes=MODEL_NOT_EXECUTABLE locked_account_limit={} owner_batch_ms={} worker_ms={} {}",
            job.clock.slot, job.clock.record_slot, now, m.fee_state_max_bps, m.realizable_bps,
            m.fixed_cost_usd_micro, m.min_profit_usd_micro, limit, job.owner_batch_ms,
            started.elapsed().as_millis(), census
        ),
    }
}

#[cfg(feature = "crate_health")]
impl ShadowOwner {
    /// Owner side of the armed sweep batch: one FeeState read and one non-blocking queue push.
    pub(super) fn submit_c0_census(
        &mut self,
        clock: Result<ChainClock, &'static str>,
        rows: Vec<CensusRow>,
        owner_batch: Duration,
    ) {
        let fee = {
            let view = StateView(&self.st);
            darkbot_project0::sizing::ProtocolSizer {
                state: &view,
                program: self.program,
                group: self.group,
                headroom_bps: darkbot_project0::sizing::DEFAULT_HEADROOM_BPS,
            }
            .fee_snapshot()
            .map(|f| f.liquidation_max_bps)
        };
        let Some(req) = self.c0_census.as_mut() else {
            return;
        };
        let Some(expected_accounts) = req.armed.take() else {
            return;
        };
        req.attempt += 1;
        let attempt = req.attempt;
        let c = req.config;
        let submitted = (|| -> Result<(), String> {
            let clock = clock.map_err(str::to_string)?;
            let model = CensusModel::new(
                fee.map_err(|e| format!("FEE_STATE {e}"))?,
                c.realizable_bps,
                c.fixed_cost_usd_micro,
                c.min_profit_usd_micro,
            )?;
            req.jobs
                .try_push(CensusJob {
                    attempt,
                    clock,
                    model,
                    expected_accounts,
                    owner_batch_ms: owner_batch.as_millis(),
                    rows,
                })
                .map_err(|_| "QUEUE_FULL".to_string())
        })();
        match submitted {
            Ok(()) => req.in_flight = true,
            Err(reason) => owner_log!(
                self,
                "c0_census: INCOMPLETE attempt={attempt} reason={reason} — retried at next READY"
            ),
        }
    }

    pub(super) fn poll_c0_census(&mut self) {
        let Some(req) = self.c0_census.as_mut() else {
            return;
        };
        let Some(outcome) = req.results.try_pop() else {
            return;
        };
        req.in_flight = false;
        match outcome {
            CensusOutcome::Complete { attempt, line } => {
                req.complete = true;
                owner_log!(self, "c0_census: COMPLETE attempt={attempt} {line}");
            }
            CensusOutcome::Incomplete { attempt, reason } => owner_log!(
                self,
                "c0_census: INCOMPLETE attempt={attempt} reason={reason} — retried at next READY"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_project0::census::AccountFacts;
    use darkbot_project0::health::HealthView;

    fn job(rows: usize, expected: usize) -> CensusJob {
        let facts = AccountFacts {
            active_banks: 1,
            liability_banks: 1,
            oracle_setups: 1 << 3,
            kamino: false,
            drift: false,
            juplend: false,
            emode_banks: 0,
            observation_accounts: 3,
            costly_positions: None,
            premium_active: None,
        };
        let one = 1i128 << 48;
        let health = HealthView {
            maintenance_health: one,
            maint_assets: 0,
            maint_liabs: 100 * one,
            equity_assets: 200 * one,
            equity_liabilities: 100 * one,
            is_bad_debt: false,
            closeout_allowed: false,
            liquidatable: false,
        };
        CensusJob {
            attempt: 1,
            clock: decode_clock(&solana_sdk::sysvar::id(), &clock_bytes(7, 1_700_000_000), 7)
                .unwrap(),
            model: CensusModel::new(500, 10_000, 1, 0).unwrap(),
            expected_accounts: expected,
            owner_batch_ms: 0,
            rows: vec![
                CensusRow {
                    health: Ok(health),
                    premium_decay_bits_per_s: Ok(0),
                    facts: Ok(facts),
                };
                rows
            ],
        }
    }

    fn clock_bytes(slot: u64, unix: i64) -> Vec<u8> {
        let mut d = slot.to_le_bytes().to_vec();
        d.extend_from_slice(&[0u8; 24]);
        d.extend_from_slice(&unix.to_le_bytes());
        d
    }

    #[test]
    fn worker_reports_complete_census_and_coverage_gaps_as_incomplete() {
        match run(job(2, 2)) {
            CensusOutcome::Complete { attempt: 1, line } => {
                assert!(line.contains("clock=CHAIN_SYSVAR"), "{line}");
                assert!(line.contains("classes=MODEL_NOT_EXECUTABLE"), "{line}");
                assert!(line.contains("accounts=2 healthy=2/"), "{line}");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            run(job(1, 2)),
            CensusOutcome::Incomplete {
                attempt: 1,
                reason: "COVERAGE rows=1 expected=2".into()
            }
        );
    }

    #[test]
    fn chain_clock_rejects_foreign_short_zero_and_future_clocks() {
        let sysvar = solana_sdk::sysvar::id();
        let ok = decode_clock(&sysvar, &clock_bytes(7, 1_700_000_000), 9).unwrap();
        assert_eq!(
            (ok.slot, ok.unix_timestamp, ok.record_slot),
            (7, 1_700_000_000, 9)
        );
        assert_eq!(
            decode_clock(&Pubkey::new_unique(), &clock_bytes(7, 1), 9),
            Err("CHAIN_CLOCK_OWNER")
        );
        assert_eq!(
            decode_clock(&sysvar, &clock_bytes(7, 1)[..39], 9),
            Err("CHAIN_CLOCK_LAYOUT")
        );
        assert_eq!(
            decode_clock(&sysvar, &clock_bytes(7, 0), 9),
            Err("CHAIN_CLOCK_TIME")
        );
        assert_eq!(
            decode_clock(&sysvar, &clock_bytes(10, 1), 9),
            Err("CHAIN_CLOCK_SLOT_AHEAD_OF_RECORD")
        );
    }
}

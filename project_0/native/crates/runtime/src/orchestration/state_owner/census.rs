//! [C0-02][C0-FIX-01][C0 Ticket 2] Opt-in market census (`P0_C0_CENSUS=1`, R127 sweep mode only).
//!
//! No second scan and no owner-side classification. The population — every debtor of the group
//! plus every group member the pinned crate cannot decode — is collected in the reverse-index scan
//! at READY. The armed R127 startup sweep evaluates each debtor exactly once against one state view
//! and one canonical chain clock (debtors outside the dirty-bank fan-out are added to that batch)
//! and emits one row per debtor; the owner moves rows and population into a single-slot bounded
//! queue. The worker proves identity coverage (missing / extra / duplicate accounts, not counts),
//! classifies, aggregates and writes every row plus one result frame as typed `.p0j` frames.
//! Any gap, a full queue, a clock or FeeState error ends the attempt INCOMPLETE with its reason
//! (journal sealed INCOMPLETE) and keeps the request for the next READY.
#![cfg_attr(not(feature = "crate_health"), allow(dead_code))]
use super::*;
use crate::analytics::journal::{self as journal_mod, Provenance as JournalProvenance};
use crate::analytics::payload::{C0CensusAccount, C0CensusResult};
use darkbot_common::channel::{Consumer, Producer, Wakeup};
use darkbot_project0::census::{classify, Census, CensusModel, CensusRow, Unsupported};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CensusConfig {
    realizable_bps: u64,
    fixed_cost_usd_micro: u64,
    min_profit_usd_micro: i128,
    /// Attempt journals: `<dir>/<stem>.c0_census.attempt<N>.p0j`, never overwritten.
    journal_dir: PathBuf,
    journal_stem: String,
}

/// One evaluated debtor: account, record slot, row.
pub(super) type CensusRowOf = (Pubkey, Option<u64>, CensusRow);

pub(super) struct CensusJob {
    attempt: u32,
    group: Pubkey,
    clock: ChainClock,
    model: CensusModel,
    /// Sorted, duplicate-free population (debtors ∪ undecodable members).
    expected: Vec<Pubkey>,
    owner_batch_ms: u128,
    rows: Vec<CensusRowOf>,
    journal: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CensusOutcome {
    Complete { attempt: u32, line: String },
    Incomplete { attempt: u32, reason: String },
}

pub(super) struct CensusRequest {
    config: CensusConfig,
    attempt: u32,
    /// The population of the next sweep batch, which is the census batch.
    pub(super) armed: Option<CensusPopulation>,
    in_flight: bool,
    complete: bool,
    jobs: Producer<CensusJob>,
    results: Consumer<CensusOutcome>,
}

impl CensusRequest {
    /// A READY may collect a population unless a census is running or already complete.
    pub(super) fn wants_population(&self) -> bool {
        !self.complete && !self.in_flight
    }
    /// A23-01: an outcome is queued for the owner.
    pub(super) fn outcome_pending(&self) -> bool {
        self.results.pending()
    }
    pub(super) fn arm(&mut self, population: CensusPopulation) {
        if self.wants_population() {
            self.armed = Some(population);
        }
    }
}

pub(super) fn config(
    journal_path: &std::path::Path,
    use_threshold_trigger: bool,
) -> Result<Option<CensusConfig>, Box<dyn std::error::Error>> {
    if !config::parsed_or("P0_C0_CENSUS", false)? {
        return Ok(None);
    }
    // The census rides on the R127 startup sweep; threshold-trigger mode has none, and a census
    // must not create a second detection pass.
    if use_threshold_trigger {
        return Err("C0_CENSUS_REQUIRES_R127_SWEEP_MODE: unset P0_USE_THRESHOLD_TRIGGER".into());
    }
    let c = CensusConfig {
        realizable_bps: config::require_parsed("P0_C0_REALIZABLE_BPS")?,
        fixed_cost_usd_micro: config::require_parsed("P0_C0_FIXED_COST_USD_MICRO")?,
        min_profit_usd_micro: config::require_parsed("P0_C0_MIN_PROFIT_USD_MICRO")?,
        journal_dir: journal_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf(),
        journal_stem: journal_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("C0_CENSUS_JOURNAL_PATH")?
            .to_string(),
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

/// `wake`: the owner's doorbell, rung by the outcome (A23-01).
pub(super) fn spawn(
    config: CensusConfig,
    shutdown: Shutdown,
    wake: &Wakeup,
) -> Result<CensusRequest, Box<dyn std::error::Error>> {
    let (jobs, job_rx) = bounded::<CensusJob>("c0-census-jobs", 1);
    let (result_tx, results) = bounded::<CensusOutcome>("c0-census-results", 1);
    let result_tx = result_tx.with_wakeup(wake);
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

/// Identity coverage of the rows against the expected population: (missing, extra, duplicates).
pub(super) fn coverage(expected: &[Pubkey], rows: &mut [CensusRowOf]) -> (u64, u64, u64) {
    rows.sort_unstable_by_key(|(pk, _, _)| pk.to_bytes());
    let duplicates = rows.windows(2).filter(|w| w[0].0 == w[1].0).count() as u64;
    let (mut i, mut j, mut missing, mut extra) = (0, 0, 0u64, 0u64);
    while i < expected.len() || j < rows.len() {
        match (expected.get(i), rows.get(j).map(|r| r.0)) {
            (Some(e), Some(r)) if *e == r => {
                i += 1;
                j += 1;
                while rows.get(j).is_some_and(|x| x.0 == r) {
                    j += 1;
                }
            }
            (Some(e), Some(r)) if e.to_bytes() < r.to_bytes() => {
                missing += 1;
                i += 1;
            }
            (Some(_), None) => {
                missing += 1;
                i += 1;
            }
            _ => {
                extra += 1;
                j += 1;
            }
        }
    }
    (missing, extra, duplicates)
}

/// Worker body: coverage proof, classification, aggregation and the typed attempt journal.
pub(super) fn run(mut job: CensusJob) -> CensusOutcome {
    let started = std::time::Instant::now();
    let now = job.clock.unix_s();
    let limit = darkbot_solana::tx::TxLimits::default().max_locked_accounts as u16;
    let (missing, extra, duplicates) = coverage(&job.expected, &mut job.rows);
    let gap = (missing, extra, duplicates) != (0, 0, 0);
    let mut census = Census::default();
    let written = (|| -> std::io::Result<[u8; 32]> {
        let mut w = journal_mod::Writer::create(&job.journal)?;
        for (account, record_slot, row) in &job.rows {
            let class = classify(row, now, &job.model);
            census.add(row, class, now, limit);
            let p = JournalProvenance {
                source: journal_mod::source::C0_CENSUS_RUNTIME,
                source_seq: w.next_seq(),
                slot: *record_slot,
                ..Default::default()
            };
            w.append_typed(
                p,
                &C0CensusAccount {
                    attempt: job.attempt,
                    account: *account,
                    group: job.group,
                    record_slot: *record_slot,
                    row: *row,
                    class,
                },
            )?;
        }
        let result = C0CensusResult {
            attempt: job.attempt,
            group: job.group,
            clock_slot: job.clock.slot,
            clock_unix_timestamp: job.clock.unix_timestamp,
            clock_record_slot: job.clock.record_slot,
            model: job.model,
            expected_accounts: job.expected.len() as u64,
            rows: job.rows.len() as u64,
            missing,
            extra,
            duplicates,
            complete: !gap,
            reason: gap.then(|| "COVERAGE".to_string()),
            census: census.clone(),
            owner_batch_ms: job.owner_batch_ms.min(u64::MAX as u128) as u64,
            worker_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        };
        let p = JournalProvenance {
            source: journal_mod::source::C0_CENSUS_RUNTIME,
            source_seq: w.next_seq(),
            slot: Some(job.clock.slot),
            ..Default::default()
        };
        w.append_typed(p, &result)?;
        Ok(w.seal(!gap)?.hash)
    })();
    let journal = job.journal.display();
    match written {
        Err(e) => CensusOutcome::Incomplete {
            attempt: job.attempt,
            reason: format!("JOURNAL {e} path={journal}"),
        },
        Ok(_) if gap => CensusOutcome::Incomplete {
            attempt: job.attempt,
            reason: format!(
                "COVERAGE expected={} rows={} missing={missing} extra={extra} duplicates={duplicates} journal={journal}",
                job.expected.len(),
                job.rows.len()
            ),
        },
        Ok(root) => {
            let m = job.model;
            CensusOutcome::Complete {
                attempt: job.attempt,
                line: format!(
                    "slot={} record_slot={} unix={} clock=CHAIN_SYSVAR fee_bps={} realizable_bps={} fixed_cost_usd_micro={} min_profit_usd_micro={} classes=MODEL_NOT_EXECUTABLE expected={} locked_account_limit={} owner_batch_ms={} worker_ms={} journal={journal} root_sha256={} {}",
                    job.clock.slot, job.clock.record_slot, now, m.fee_state_max_bps, m.realizable_bps,
                    m.fixed_cost_usd_micro, m.min_profit_usd_micro, job.expected.len(), limit,
                    job.owner_batch_ms, started.elapsed().as_millis(), journal_mod::hex(&root), census
                ),
            }
        }
    }
}

#[cfg(feature = "crate_health")]
impl ShadowOwner {
    /// Owner side of the armed sweep batch: one FeeState read and one non-blocking queue push.
    pub(super) fn submit_c0_census(
        &mut self,
        clock: Result<ChainClock, &'static str>,
        mut rows: Vec<CensusRowOf>,
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
        let group = self.group;
        let Some(req) = self.c0_census.as_mut() else {
            return;
        };
        let Some(population) = req.armed.take() else {
            return;
        };
        req.attempt += 1;
        let attempt = req.attempt;
        // Undecodable members are census rows with their reason, never dropped.
        for pk in &population.undecodable {
            let record_slot = self.st.get(pk).map(|r| r.slot);
            rows.push((
                *pk,
                record_slot,
                CensusRow {
                    health: Err(Unsupported::AccountLayout),
                    premium_decay_bits_per_s: Err(Unsupported::AccountLayout),
                    facts: Err(Unsupported::AccountLayout),
                },
            ));
        }
        let mut expected: Vec<Pubkey> = population
            .debtors
            .iter()
            .chain(&population.undecodable)
            .copied()
            .collect();
        expected.sort_unstable_by_key(|pk| pk.to_bytes());
        expected.dedup();
        let c = &req.config;
        let journal = c
            .journal_dir
            .join(format!("{}.c0_census.attempt{attempt}.p0j", c.journal_stem));
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
                    group,
                    clock,
                    model,
                    expected,
                    owner_batch_ms: owner_batch.as_millis(),
                    rows,
                    journal,
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

    fn clock_bytes(slot: u64, unix: i64) -> Vec<u8> {
        let mut d = slot.to_le_bytes().to_vec();
        d.extend_from_slice(&[0u8; 24]);
        d.extend_from_slice(&unix.to_le_bytes());
        d
    }

    fn row() -> CensusRow {
        let one = 1i128 << 48;
        CensusRow {
            health: Ok(HealthView {
                maintenance_health: one,
                maint_assets: 0,
                maint_liabs: 100 * one,
                equity_assets: 200 * one,
                equity_liabilities: 100 * one,
                is_bad_debt: false,
                closeout_allowed: false,
                liquidatable: false,
            }),
            premium_decay_bits_per_s: Ok(0),
            facts: Ok(AccountFacts {
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
            }),
        }
    }

    fn job(dir: &std::path::Path, expected: &[Pubkey], rows: &[Pubkey]) -> CensusJob {
        CensusJob {
            attempt: 1,
            group: Pubkey::new_unique(),
            clock: decode_clock(&solana_sdk::sysvar::id(), &clock_bytes(7, 1_700_000_000), 7)
                .unwrap(),
            model: CensusModel::new(500, 10_000, 1, 0).unwrap(),
            expected: {
                let mut e = expected.to_vec();
                e.sort_unstable_by_key(|pk| pk.to_bytes());
                e
            },
            owner_batch_ms: 0,
            rows: rows.iter().map(|pk| (*pk, Some(5), row())).collect(),
            journal: dir.join(format!("census-{}.p0j", Pubkey::new_unique())),
        }
    }

    fn frames(path: &std::path::Path) -> (Vec<C0CensusAccount>, C0CensusResult, bool) {
        let mut r = journal_mod::Reader::open(path).unwrap();
        let mut rows = Vec::new();
        let mut result = None;
        while let Some(f) = r.next_frame().unwrap() {
            match f.kind {
                journal_mod::kind::C0_CENSUS_ROW => rows.push(f.typed().unwrap()),
                journal_mod::kind::C0_CENSUS_RESULT => result = Some(f.typed().unwrap()),
                _ => {}
            }
        }
        (rows, result.unwrap(), r.complete)
    }

    /// Identity coverage, not counts: an equal number of rows with a missing account and an
    /// extra one is a gap, as are duplicates; a complete census journals every account.
    #[test]
    fn census_proves_identity_coverage_and_journals_typed_rows() {
        let dir = std::env::temp_dir().join(format!("p0-c0-{}", Pubkey::new_unique()));
        std::fs::create_dir(&dir).unwrap();
        let (a, b, c) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let complete = job(&dir, &[a, b], &[b, a]);
        let path = complete.journal.clone();
        match run(complete) {
            CensusOutcome::Complete { attempt: 1, line } => {
                assert!(
                    line.contains("classes=MODEL_NOT_EXECUTABLE expected=2"),
                    "{line}"
                );
                assert!(line.contains("accounts=2 healthy=2/"), "{line}");
            }
            other => panic!("{other:?}"),
        }
        let (rows, result, sealed_complete) = frames(&path);
        let mut accounts: Vec<_> = rows.iter().map(|r| r.account).collect();
        accounts.sort_unstable_by_key(|pk| pk.to_bytes());
        let mut want = vec![a, b];
        want.sort_unstable_by_key(|pk| pk.to_bytes());
        assert_eq!(accounts, want);
        assert!(rows.iter().all(|r| r.record_slot == Some(5)));
        assert!(result.complete && sealed_complete);
        assert_eq!(
            (result.expected_accounts, result.rows, result.missing),
            (2, 2, 0)
        );

        // Same count, wrong identity: one missing, one extra.
        let swapped = job(&dir, &[a, b], &[a, c]);
        let path = swapped.journal.clone();
        match run(swapped) {
            CensusOutcome::Incomplete { reason, .. } => {
                assert!(
                    reason.contains("missing=1 extra=1 duplicates=0"),
                    "{reason}"
                )
            }
            other => panic!("{other:?}"),
        }
        let (_, result, sealed_complete) = frames(&path);
        assert!(!result.complete && !sealed_complete);
        // Duplicate evaluation of one account.
        match run(job(&dir, &[a, b], &[a, a, b])) {
            CensusOutcome::Incomplete { reason, .. } => {
                assert!(
                    reason.contains("missing=0 extra=0 duplicates=1"),
                    "{reason}"
                )
            }
            other => panic!("{other:?}"),
        }
        std::fs::remove_dir_all(dir).unwrap();
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

    /// Membership: debtor of the group, undecodable member (group prefix readable), and
    /// non-members (other group, no liability).
    #[test]
    fn census_membership_counts_debtors_and_undecodable_members_only() {
        use marginfi_type_crate::types::{MarginfiAccount, WrappedI80F48};
        let shares = |bits: i128| WrappedI80F48 {
            value: bits.to_le_bytes(),
        };
        let group = Pubkey::new_unique();
        let encode = |acc: &MarginfiAccount| {
            let mut d = darkbot_project0::protocol::DISC_MARGINFI_ACCOUNT.to_vec();
            d.extend_from_slice(bytemuck::bytes_of(acc));
            d
        };
        let mut debtor: MarginfiAccount = bytemuck::Zeroable::zeroed();
        debtor.group = group;
        debtor.lending_account.balances[0].active = 1;
        debtor.lending_account.balances[0].bank_pk = Pubkey::new_unique();
        debtor.lending_account.balances[0].liability_shares = shares(1 << 48);
        assert_eq!(
            census_member(&encode(&debtor), &group),
            Some(CensusMember::Debtor)
        );
        let mut lender = debtor;
        lender.lending_account.balances[0].liability_shares = shares(0);
        assert_eq!(census_member(&encode(&lender), &group), None);
        let mut foreign = debtor;
        foreign.group = Pubkey::new_unique();
        assert_eq!(census_member(&encode(&foreign), &group), None);
        let truncated = &encode(&debtor)[..100];
        assert_eq!(
            census_member(truncated, &group),
            Some(CensusMember::Undecodable)
        );
    }

    /// Local sizing of the census batch (`--ignored`): worker time, journal bytes and row memory
    /// for the historical group population (82 782 debtors, 4 h run 2026-10-08). Synthetic rows.
    #[test]
    #[ignore]
    fn c0_census_worker_cost_at_historical_population() {
        let dir = std::env::temp_dir().join(format!("p0-c0-bench-{}", Pubkey::new_unique()));
        std::fs::create_dir(&dir).unwrap();
        let accounts: Vec<Pubkey> = (0..82_782).map(|_| Pubkey::new_unique()).collect();
        let j = job(&dir, &accounts, &accounts);
        let path = j.journal.clone();
        let row_bytes = std::mem::size_of::<CensusRowOf>() * j.rows.len();
        let started = std::time::Instant::now();
        let outcome = run(j);
        let elapsed = started.elapsed();
        assert!(
            matches!(outcome, CensusOutcome::Complete { .. }),
            "{outcome:?}"
        );
        println!(
            "C0_CENSUS_WORKER rows=82782 worker_ms={} journal_bytes={} row_memory_bytes={row_bytes}",
            elapsed.as_millis(),
            std::fs::metadata(&path).unwrap().len()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

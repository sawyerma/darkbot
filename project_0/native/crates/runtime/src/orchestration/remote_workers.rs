//! [FIX-108][R4] Remote RPC work isolated from the ingress/detection loop.
//!
//! Winner lookup and the FE-04 settlement own their own RpcClient instances (C02: the
//! settlement is the one reconcile path). The state thread only sends/receives bounded native
//! work items.

use crate::analytics::record::LossReason;
use crate::market::winner::{find_winner, Winner};
use darkbot_common::channel::{bounded, ChannelError, Consumer, Producer, Wakeup};
use darkbot_common::lifecycle::Shutdown;
use darkbot_solana::submit::rpc::RpcSubmitter;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::{hash::Hash, transaction::VersionedTransaction};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct WinnerWatch {
    /// [F1] Monotonic revision assigned by the `WinnerTracker`; travels through the worker so a
    /// stale completion can be recognised. Callers pass 0.
    pub revision: u64,
    pub account: Pubkey,
    pub rx_slot: u64,
    pub detect_slot: u64,
    pub sign_slot: Option<u64>,
    pub expected_net: Option<i128>,
    pub shadow_ready: bool,
    pub our_loss: Option<LossReason>,
    pub created_unix_s: u64,
    pub next_check_unix_s: u64,
}

/// B01 OBSERVE: one rebalance quote, asked off the owner (the route provider's ExactIn quote).
#[cfg(feature = "crate_health")]
pub struct RebalanceQuoteWork {
    pub key: crate::execution::inventory::QuoteKey,
    pub sell_mint: Pubkey,
    pub buy_mint: Pubkey,
    /// The observation that asked (`RebalanceQueues::generation`).
    pub generation: u64,
}

/// The quote's on-chain enforced minimum output: `Ok(None)` without a route (or a route for
/// another amount), `Err` when the provider was not reachable.
#[cfg(feature = "crate_health")]
pub struct RebalanceQuoteResult {
    pub key: crate::execution::inventory::QuoteKey,
    pub minimum_out: Result<Option<u64>, String>,
    /// The asking observation's generation: an answer to another one is dropped.
    pub generation: u64,
}

/// DEC-06 ACTIVE: one sweep cross-check sample — an account the threshold trigger did not wake on
/// an oracle tick, with every account its exact health reads, copied from the owner's state at
/// that tick (the evaluation runs off the owner on an immutable copy).
#[cfg(feature = "crate_health")]
pub struct CrossCheckWork {
    pub generation: u64,
    pub account: Pubkey,
    pub group: Pubkey,
    pub slot: u64,
    pub unix_timestamp: i64,
    pub input_revision: u64,
    /// The keys the evaluation reads and their canonical revisions at the tick (E9).
    pub keys: Vec<Pubkey>,
    pub fingerprint: Hash,
    /// (key, owner, lamports, data) of each key.
    pub accounts: Vec<(Pubkey, Pubkey, u64, Vec<u8>)>,
}

/// The cross-check's exact evaluation of a sample: liquidatable or not, or why it failed.
#[cfg(feature = "crate_health")]
pub struct CrossCheckResult {
    pub generation: u64,
    pub account: Pubkey,
    pub slot: u64,
    pub input_revision: u64,
    pub keys: Vec<Pubkey>,
    pub fingerprint: Hash,
    pub liquidatable: Result<bool, String>,
}

/// The owner's ends of the cross-check queues.
#[cfg(feature = "crate_health")]
pub struct CrossCheckQueues {
    pub jobs: Producer<CrossCheckWork>,
    pub results: Consumer<CrossCheckResult>,
}

/// The sample's copied accounts as the health engine's account source.
#[cfg(feature = "crate_health")]
struct CopiedState(std::collections::HashMap<Pubkey, (Pubkey, u64, Vec<u8>)>);
#[cfg(feature = "crate_health")]
impl darkbot_project0::health::AccountSource for CopiedState {
    fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
        self.0.get(key).map(|(o, l, d)| (*o, *l, d.as_slice()))
    }
}

/// DEC-06: the exact health of a cross-check sample on its copied accounts (the worker's work).
#[cfg(feature = "crate_health")]
pub fn crosscheck_evaluate(work: CrossCheckWork) -> CrossCheckResult {
    use darkbot_project0::health::HealthEngine;
    let state = CopiedState(
        work.accounts
            .into_iter()
            .map(|(k, o, l, d)| (k, (o, l, d)))
            .collect(),
    );
    let engine = darkbot_project0::health::crate_impl::CrateHealth {
        state: &state,
        group: work.group,
    };
    let liquidatable = engine
        .health(&work.account, work.unix_timestamp, work.slot)
        .map(|h| h.liquidatable)
        .map_err(|e| e.to_string());
    CrossCheckResult {
        generation: work.generation,
        account: work.account,
        slot: work.slot,
        input_revision: work.input_revision,
        keys: work.keys,
        fingerprint: work.fingerprint,
        liquidatable,
    }
}

/// The owner's ends of the rebalance quote queues (present while bands exist).
#[cfg(feature = "crate_health")]
pub struct RebalanceQueues {
    pub jobs: Producer<RebalanceQuoteWork>,
    pub results: Consumer<RebalanceQuoteResult>,
    /// The owner's current observation: the worker skips a job of another one (asked before a
    /// mode change or an earlier observation), so no provider is called for it.
    pub generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

pub struct WinnerResult {
    pub watch: WinnerWatch,
    pub result: Result<Option<Winner>, String>,
}

pub struct ProjectionWork {
    pub account: Pubkey,
    pub base_slot: u64,
    pub base_account_hash: Hash,
    pub t_rx_ns: u64,
    pub tx: VersionedTransaction,
    pub targets: Vec<Pubkey>,
    /// E9: dependency set and its canonical revisions at request time (passed back unchanged).
    pub dependencies: Vec<Pubkey>,
    pub dependency_fingerprint: Hash,
    /// E9: the newest canonical slot of the candidate and its dependencies; the simulation must
    /// not be older.
    pub min_context_slot: u64,
    /// P6-MT.2 (MT5a): the candidate identity this projection prepares for — its generations
    /// and the dependency readset at request time — passed back unchanged for the owner's
    /// central result policy.
    pub identity: crate::execution::identity::CandidateIdentity,
}

pub struct ProjectionResultMessage {
    pub account: Pubkey,
    pub base_slot: u64,
    pub base_account_hash: Hash,
    pub t_rx_ns: u64,
    pub dependencies: Vec<Pubkey>,
    pub dependency_fingerprint: Hash,
    pub identity: crate::execution::identity::CandidateIdentity,
    pub result: Result<darkbot_solana::submit::rpc::PostAccountsSimulation, ProjectionFailure>,
}

/// E9: a failed projection — typed whether the node was behind the required context (a retry
/// can succeed once it caught up) — and its error text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionFailure {
    pub context_behind: bool,
    pub error: String,
}

/// [R126] One integration-Bank refresh projection is enough to obtain a fresh pricing state
/// before selecting account thresholds. This prevents stale integration pricing from fanning
/// out to every account using the Bank.
pub struct OraclePriceProjectionWork {
    pub bank: Pubkey,
    pub base_slot: u64,
    pub base_bank_hash: Hash,
    pub tx: VersionedTransaction,
    pub targets: Vec<Pubkey>,
}

pub struct OraclePriceProjectionResultMessage {
    pub bank: Pubkey,
    pub base_slot: u64,
    pub base_bank_hash: Hash,
    pub result: Result<darkbot_solana::submit::rpc::PostAccountsSimulation, String>,
}

/// FE-04/C3: the settlement reads of one UNKNOWN attempt or exit (its subject and observed
/// signatures); `exit` names the exit (its approval id), `None` the attempt.
pub struct Fe04SettlementWork {
    pub subject: crate::execution::one_shot::settlement::SettlementSubject,
    pub observed: crate::execution::one_shot::settlement::Observed,
    pub exit: Option<String>,
}

pub struct Fe04SettlementResult {
    pub evidence: crate::execution::one_shot::settlement::SettlementEvidence,
    /// The work's `exit`: the result applies only to what it was read for.
    pub exit: Option<String>,
}

/// Test seam of `RemoteWorkers::detached`: the producers a test drives as the workers, the
/// FE-04 settlement job queue it serves, and the space doorbell.
#[cfg(all(test, feature = "crate_health"))]
pub(crate) struct Detached {
    pub(crate) projection: Producer<ProjectionResultMessage>,
    pub(crate) oracle: Producer<OraclePriceProjectionResultMessage>,
    pub(crate) fe04_jobs: Consumer<Fe04SettlementWork>,
    pub(crate) fe04_results: Producer<Fe04SettlementResult>,
    pub(crate) space: Wakeup,
}

pub struct RemoteWorkers {
    threads: Vec<std::thread::JoinHandle<()>>,
    pub winner_jobs: Producer<WinnerWatch>,
    pub winner_results: Consumer<WinnerResult>,
    pub projection_jobs: Producer<ProjectionWork>,
    pub projection_results: Consumer<ProjectionResultMessage>,
    pub oracle_price_projection_jobs: Producer<OraclePriceProjectionWork>,
    pub oracle_price_projection_results: Consumer<OraclePriceProjectionResultMessage>,
    pub fe04_settlement_jobs: Producer<Fe04SettlementWork>,
    pub fe04_settlement_results: Consumer<Fe04SettlementResult>,
    /// B01 OBSERVE: the rebalance quote queues, once `start_rebalance_quoter` ran.
    #[cfg(feature = "crate_health")]
    pub rebalance: Option<RebalanceQueues>,
    /// DEC-06: the ACTIVE cross-check queues, once `start_threshold_crosscheck` ran.
    #[cfg(feature = "crate_health")]
    pub crosscheck: Option<CrossCheckQueues>,
    /// The doorbells of later attached workers (the owner's `wake`, the workers' `space`).
    #[cfg(feature = "crate_health")]
    wake: Wakeup,
    #[cfg(feature = "crate_health")]
    space: Wakeup,
    #[cfg(feature = "crate_health")]
    capacity: usize,
}

/// A23-08: liveness bound of one wait on a full result queue. Progress is event-driven: the
/// owner's pop and a shutdown request both ring `space`; the bound only re-checks a closed queue.
const FULL_WAIT_BOUND: Duration = Duration::from_secs(1);

/// A23-01/A23-08: a result queue — its pushes ring the owner's doorbell `wake`, the owner's pops
/// ring `space` (the doorbell of workers waiting on a full queue).
fn result_queue<T>(
    name: &'static str,
    capacity: usize,
    wake: &Wakeup,
    space: &Wakeup,
) -> (Producer<T>, Consumer<T>) {
    let (tx, rx) = bounded::<T>(name, capacity);
    (tx.with_wakeup(wake), rx.with_space_wakeup(space))
}

/// Pushes a worker result; a full queue waits for the owner's next pop (its space doorbell)
/// instead of a 1-ms poll, and gives up only on shutdown or a closed queue.
pub(super) fn push_result<T>(
    tx: &Producer<T>,
    mut value: T,
    shutdown: &Shutdown,
    space: &Wakeup,
) -> bool {
    loop {
        if shutdown.is_requested() {
            return false;
        }
        let token = space.token();
        match tx.try_push(value) {
            Ok(()) => return true,
            Err(ChannelError::Full(v)) => {
                value = v;
                let _ = space.wait(token, FULL_WAIT_BOUND);
            }
            Err(ChannelError::Disconnected(_)) => return false,
        }
    }
}

impl RemoteWorkers {
    /// The market scanner must not start while an in-flight shadow RPC remains alive.
    /// Result queues stop accepting completions on shutdown, so a full queue cannot
    /// deadlock this barrier after the state-thread consumer has stopped.
    pub fn wait_stopped(self, timeout: Duration) -> Result<(), String> {
        let deadline = std::time::Instant::now() + timeout;
        while self.threads.iter().any(|thread| !thread.is_finished()) {
            if std::time::Instant::now() >= deadline {
                return Err("SHADOW_REMOTE_QUIESCENCE_TIMEOUT".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        for thread in self.threads {
            thread
                .join()
                .map_err(|_| "SHADOW_REMOTE_WORKER_PANIC".to_string())?;
        }
        Ok(())
    }

    /// `wake`: the owner's doorbell, rung by every completed result (A23-01).
    pub fn start(
        rpc_url: String,
        program: Pubkey,
        shutdown: &Shutdown,
        capacity: usize,
        wake: &Wakeup,
    ) -> std::io::Result<Self> {
        // A23-08: rung by every owner pop of a result queue and by the shutdown request.
        let space = Wakeup::default();
        shutdown.ring_on_request(&space);
        let (winner_jobs, winner_job_rx) = bounded::<WinnerWatch>("winner-rpc-jobs", capacity);
        let (winner_done_tx, winner_results) =
            result_queue::<WinnerResult>("winner-rpc-results", capacity, wake, &space);
        let (projection_jobs, projection_job_rx) =
            bounded::<ProjectionWork>("projection-rpc-jobs", capacity);
        let (projection_done_tx, projection_results) = result_queue::<ProjectionResultMessage>(
            "projection-rpc-results",
            capacity,
            wake,
            &space,
        );
        let (oracle_price_projection_jobs, oracle_price_projection_job_rx) =
            bounded::<OraclePriceProjectionWork>("oracle-price-projection-rpc-jobs", capacity);
        let (oracle_price_projection_done_tx, oracle_price_projection_results) =
            result_queue::<OraclePriceProjectionResultMessage>(
                "oracle-price-projection-rpc-results",
                capacity,
                wake,
                &space,
            );
        // FE-04/C3: settlement reads (finalized receipts, non-landing proof), off the owner.
        let (fe04_settlement_jobs, fe04_settlement_job_rx) =
            bounded::<Fe04SettlementWork>("fe04-settlement-jobs", 1);
        let (fe04_settlement_done_tx, fe04_settlement_results) =
            result_queue::<Fe04SettlementResult>("fe04-settlement-results", 1, wake, &space);

        let mut threads = Vec::new();
        let fe04_shutdown = shutdown.clone();
        let fe04_space = space.clone();
        let fe04_rpc_url = rpc_url.clone();
        threads.push(
            std::thread::Builder::new()
                .name("fe04-settlement-rpc".into())
                .spawn(move || {
                    let rpc = darkbot_solana::provider::rpc_client(
                        fe04_rpc_url.clone(),
                        Duration::from_millis(
                            darkbot_solana::runtime_config::active()
                                .limits
                                .rpc_timeout_ms,
                        ),
                        Default::default(),
                        // C5: financial reconciliation, the prioritized workload.
                        darkbot_solana::provider::Workload::Reconcile,
                    );
                    // DEC-03: the non-landing sources (C on this endpoint, the configured B).
                    let sources =
                        crate::execution::one_shot::non_landing::NonLandingSources::configured(
                            fe04_rpc_url,
                            darkbot_solana::runtime_config::active(),
                        );
                    while !fe04_shutdown.is_requested() {
                        let Some(work) =
                            fe04_settlement_job_rx.pop_timeout(Duration::from_millis(250))
                        else {
                            continue;
                        };
                        let evidence =
                            crate::execution::one_shot::settlement::fetch_settlement_evidence(
                                &rpc,
                                sources.as_ref().map_err(String::as_str),
                                &work.subject,
                                &work.observed,
                            );
                        if !push_result(
                            &fe04_settlement_done_tx,
                            Fe04SettlementResult {
                                evidence,
                                exit: work.exit,
                            },
                            &fe04_shutdown,
                            &fe04_space,
                        ) {
                            return;
                        }
                    }
                })?,
        );
        let winner_rpc_url = rpc_url.clone();
        let winner_shutdown = shutdown.clone();
        let winner_space = space.clone();
        threads.push(
            std::thread::Builder::new()
                .name("winner-rpc".into())
                .spawn(move || {
                    let rpc = darkbot_solana::provider::rpc_client(
                        winner_rpc_url,
                        Duration::from_millis(
                            darkbot_solana::runtime_config::active()
                                .limits
                                .rpc_timeout_ms,
                        ),
                        Default::default(),
                        darkbot_solana::provider::Workload::Winner,
                    );
                    while !winner_shutdown.is_requested() {
                        let Some(watch) = winner_job_rx.pop_timeout(Duration::from_millis(250))
                        else {
                            continue;
                        };
                        let result = find_winner(
                            &rpc,
                            &program,
                            &watch.account,
                            watch.rx_slot.saturating_sub(2),
                        );
                        if !push_result(
                            &winner_done_tx,
                            WinnerResult { watch, result },
                            &winner_shutdown,
                            &winner_space,
                        ) {
                            return;
                        }
                    }
                })?,
        );

        let projection_shutdown = shutdown.clone();
        let projection_space = space.clone();
        let projection_rpc_url = rpc_url.clone();
        threads.push(
            std::thread::Builder::new()
                .name("projection-rpc".into())
                .spawn(move || {
                    let submitter = RpcSubmitter::new(projection_rpc_url, true);
                    while !projection_shutdown.is_requested() {
                        let Some(work) = projection_job_rx.pop_timeout(Duration::from_millis(250))
                        else {
                            continue;
                        };
                        let account = work.account;
                        let base_slot = work.base_slot;
                        let base_account_hash = work.base_account_hash;
                        let t_rx_ns = work.t_rx_ns;
                        let dependencies = work.dependencies.clone();
                        let dependency_fingerprint = work.dependency_fingerprint;
                        let identity = work.identity.clone();
                        let mt0_start = std::time::Instant::now(); // [MT0]
                        let result = submitter
                            .simulate_with_post_accounts(
                                &work.tx,
                                &work.targets,
                                work.min_context_slot,
                            )
                            .map_err(|e| ProjectionFailure {
                                context_behind: matches!(
                                    e,
                                    darkbot_solana::submit::rpc::ProjectionError::ContextBehind { .. }
                                ),
                                error: e.to_string(),
                            });
                        let mt0_outcome = match &result {
                            Ok(_) => darkbot_common::mt0::RemoteOutcome::Ok,
                            Err(e) => darkbot_common::mt0::RemoteOutcome::classify_text(&e.error),
                        };
                        darkbot_common::mt0::MT0.record_remote(
                            darkbot_common::mt0::RemoteClass::IntegrationProjection,
                            mt0_start.elapsed(),
                            mt0_outcome,
                        );
                        if !push_result(
                            &projection_done_tx,
                            ProjectionResultMessage {
                                account,
                                base_slot,
                                base_account_hash,
                                t_rx_ns,
                                dependencies,
                                dependency_fingerprint,
                                identity,
                                result,
                            },
                            &projection_shutdown,
                            &projection_space,
                        ) {
                            return;
                        }
                    }
                })?,
        );

        let oracle_projection_shutdown = shutdown.clone();
        let oracle_projection_space = space.clone();
        let oracle_projection_rpc_url = rpc_url.clone();
        threads.push(
            std::thread::Builder::new()
                .name("oracle-price-projection-rpc".into())
                .spawn(move || {
                    let submitter = RpcSubmitter::new(oracle_projection_rpc_url, true);
                    while !oracle_projection_shutdown.is_requested() {
                        let Some(work) =
                            oracle_price_projection_job_rx.pop_timeout(Duration::from_millis(250))
                        else {
                            continue;
                        };
                        let bank = work.bank;
                        let base_slot = work.base_slot;
                        let base_bank_hash = work.base_bank_hash;
                        let mt0_start = std::time::Instant::now(); // [MT0]
                                                                   // E9: the bank's projection is not older than its canonical bank.
                        let result = submitter
                            .simulate_with_post_accounts(&work.tx, &work.targets, work.base_slot)
                            .map_err(|e| e.to_string());
                        let mt0_outcome = match &result {
                            Ok(_) => darkbot_common::mt0::RemoteOutcome::Ok,
                            Err(e) => darkbot_common::mt0::RemoteOutcome::classify_text(e),
                        };
                        darkbot_common::mt0::MT0.record_remote(
                            darkbot_common::mt0::RemoteClass::OracleProjection,
                            mt0_start.elapsed(),
                            mt0_outcome,
                        );
                        if !push_result(
                            &oracle_price_projection_done_tx,
                            OraclePriceProjectionResultMessage {
                                bank,
                                base_slot,
                                base_bank_hash,
                                result,
                            },
                            &oracle_projection_shutdown,
                            &oracle_projection_space,
                        ) {
                            return;
                        }
                    }
                })?,
        );

        Ok(Self {
            threads,
            winner_jobs,
            winner_results,
            projection_jobs,
            projection_results,
            oracle_price_projection_jobs,
            oracle_price_projection_results,
            fe04_settlement_jobs,
            fe04_settlement_results,
            #[cfg(feature = "crate_health")]
            rebalance: None,
            #[cfg(feature = "crate_health")]
            crosscheck: None,
            #[cfg(feature = "crate_health")]
            wake: wake.clone(),
            #[cfg(feature = "crate_health")]
            space,
            #[cfg(feature = "crate_health")]
            capacity,
        })
    }

    /// B01 OBSERVE: starts the rebalance quote worker — the route provider's ExactIn quote of
    /// each asked rebalance, bounded by `jupiter_timeout_ms`, off the owner; stopped and joined
    /// with the other workers.
    #[cfg(feature = "crate_health")]
    pub fn start_rebalance_quoter(
        &mut self,
        routes: crate::execution::route::JupiterV2RouteProvider,
        shutdown: &Shutdown,
    ) -> std::io::Result<()> {
        use crate::execution::builder::p0::RouteProvider;
        let (jobs, job_rx) = bounded::<RebalanceQuoteWork>("rebalance-quote-jobs", self.capacity);
        let (done_tx, results) = result_queue::<RebalanceQuoteResult>(
            "rebalance-quote-results",
            self.capacity,
            &self.wake,
            &self.space,
        );
        let (shutdown, space) = (shutdown.clone(), self.space.clone());
        let generation = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let current = generation.clone();
        self.threads.push(
            std::thread::Builder::new()
                .name("rebalance-quotes".into())
                .spawn(move || {
                    while !shutdown.is_requested() {
                        let Some(work) = job_rx.pop_timeout(Duration::from_millis(250)) else {
                            continue;
                        };
                        if work.generation != current.load(std::sync::atomic::Ordering::Acquire) {
                            continue;
                        }
                        let deadline = std::time::Instant::now()
                            + Duration::from_millis(
                                darkbot_solana::runtime_config::active()
                                    .limits
                                    .jupiter_timeout_ms,
                            );
                        let amount_in = work.key.2;
                        let minimum_out = routes
                            .quote_exact_in(
                                &work.sell_mint,
                                &work.buy_mint,
                                amount_in,
                                &[],
                                deadline,
                            )
                            .map(|route| {
                                route
                                    .filter(|r| r.amount_in == amount_in)
                                    .and_then(|r| r.minimum_executable_out)
                            });
                        let result = RebalanceQuoteResult {
                            key: work.key,
                            minimum_out,
                            generation: work.generation,
                        };
                        if !push_result(&done_tx, result, &shutdown, &space) {
                            return;
                        }
                    }
                })?,
        );
        self.rebalance = Some(RebalanceQueues {
            jobs,
            results,
            generation,
        });
        Ok(())
    }

    /// DEC-06: starts the ACTIVE cross-check worker — the exact health of each sample on its
    /// copied accounts, off the owner (bounded queues; a full one skips the sample); stopped and
    /// joined with the other workers.
    #[cfg(feature = "crate_health")]
    pub fn start_threshold_crosscheck(&mut self, shutdown: &Shutdown) -> std::io::Result<()> {
        let (jobs, job_rx) = bounded::<CrossCheckWork>("threshold-crosscheck-jobs", self.capacity);
        let (done_tx, results) = result_queue::<CrossCheckResult>(
            "threshold-crosscheck-results",
            self.capacity,
            &self.wake,
            &self.space,
        );
        let (shutdown, space) = (shutdown.clone(), self.space.clone());
        self.threads.push(
            std::thread::Builder::new()
                .name("threshold-crosscheck".into())
                .spawn(move || {
                    while !shutdown.is_requested() {
                        let Some(work) = job_rx.pop_timeout(Duration::from_millis(250)) else {
                            continue;
                        };
                        if !push_result(&done_tx, crosscheck_evaluate(work), &shutdown, &space) {
                            return;
                        }
                    }
                })?,
        );
        self.crosscheck = Some(CrossCheckQueues { jobs, results });
        Ok(())
    }

    /// Test seam: the cross-check queues without a worker; the test answers the jobs.
    #[cfg(all(test, feature = "crate_health"))]
    pub(crate) fn detached_crosscheck(
        &mut self,
    ) -> (Consumer<CrossCheckWork>, Producer<CrossCheckResult>) {
        let (jobs, job_rx) = bounded("threshold-crosscheck-jobs", self.capacity);
        let (done_tx, results) = result_queue(
            "threshold-crosscheck-results",
            self.capacity,
            &self.wake,
            &self.space,
        );
        self.crosscheck = Some(CrossCheckQueues { jobs, results });
        (job_rx, done_tx)
    }

    /// Test seam: the rebalance quote queues without a worker; the test answers the jobs.
    #[cfg(all(test, feature = "crate_health"))]
    pub(crate) fn detached_rebalance(
        &mut self,
    ) -> (Consumer<RebalanceQuoteWork>, Producer<RebalanceQuoteResult>) {
        let (jobs, job_rx) = bounded("rebalance-quote-jobs", self.capacity);
        let (done_tx, results) = result_queue(
            "rebalance-quote-results",
            self.capacity,
            &self.wake,
            &self.space,
        );
        self.rebalance = Some(RebalanceQueues {
            jobs,
            results,
            generation: Default::default(),
        });
        (job_rx, done_tx)
    }

    /// Test seam (A23-01): the production result queues without worker threads; the test drives
    /// them as the workers (`Detached`).
    #[cfg(all(test, feature = "crate_health"))]
    pub(crate) fn detached(
        shutdown: &Shutdown,
        capacity: usize,
        wake: &Wakeup,
    ) -> (Self, Detached) {
        let space = Wakeup::default();
        shutdown.ring_on_request(&space);
        let (projection_done_tx, projection_results) =
            result_queue("projection-rpc-results", capacity, wake, &space);
        let (oracle, oracle_price_projection_results) = result_queue(
            "oracle-price-projection-rpc-results",
            capacity,
            wake,
            &space,
        );
        let (fe04_settlement_jobs, fe04_jobs) = bounded("fe04-settlement-jobs", 1);
        let (fe04_results, fe04_settlement_results) =
            result_queue("fe04-settlement-results", 1, wake, &space);
        let workers = Self {
            threads: Vec::new(),
            winner_jobs: bounded("winner-rpc-jobs", capacity).0,
            winner_results: result_queue("winner-rpc-results", capacity, wake, &space).1,
            projection_jobs: bounded("projection-rpc-jobs", capacity).0,
            projection_results,
            oracle_price_projection_jobs: bounded("oracle-price-projection-rpc-jobs", capacity).0,
            oracle_price_projection_results,
            fe04_settlement_jobs,
            fe04_settlement_results,
            #[cfg(feature = "crate_health")]
            rebalance: None,
            #[cfg(feature = "crate_health")]
            crosscheck: None,
            #[cfg(feature = "crate_health")]
            wake: wake.clone(),
            #[cfg(feature = "crate_health")]
            space: space.clone(),
            #[cfg(feature = "crate_health")]
            capacity,
        };
        (
            workers,
            Detached {
                projection: projection_done_tx,
                oracle,
                fe04_jobs,
                fe04_results,
                space,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A23-08: a worker blocked on a full result queue resumes on the owner's pop and stops on
    /// the shutdown request, both far inside the 1-s liveness bound (no poll decides either).
    #[test]
    fn a23_08_full_result_queue_resumes_on_pop_and_stops_on_shutdown() {
        let space = Wakeup::default();
        let (tx, rx) = bounded("stop-test", 1);
        let rx = rx.with_space_wakeup(&space);
        tx.try_push(1_u64).unwrap();
        let shutdown = Shutdown::new();
        shutdown.ring_on_request(&space);
        let (stop, worker_space) = (shutdown.clone(), space.clone());
        let worker = std::thread::spawn(move || {
            let pushed = push_result(&tx, 2, &stop, &worker_space);
            (pushed, push_result(&tx, 3, &stop, &worker_space))
        });
        while rx.stats().overflow == 0 {
            std::thread::yield_now();
        }
        let popped = std::time::Instant::now();
        assert_eq!(rx.try_pop(), Some(1));
        while rx.stats().sent < 2 {
            std::thread::yield_now();
        }
        let resumed = popped.elapsed();
        while rx.stats().overflow < 2 {
            std::thread::yield_now();
        }
        let requested = std::time::Instant::now();
        shutdown.request();
        assert_eq!(worker.join().unwrap(), (true, false));
        let stopped = requested.elapsed();
        assert_eq!(
            rx.try_pop(),
            Some(2),
            "order kept, nothing lost or duplicated"
        );
        assert_eq!(rx.try_pop(), None);
        for d in [resumed, stopped] {
            assert!(d < Duration::from_millis(200), "{d:?}");
        }
    }

    #[test]
    fn remote_workers_confirm_actual_thread_exit() {
        let shutdown = Shutdown::new();
        let workers = RemoteWorkers::start(
            "http://127.0.0.1:1".into(),
            Pubkey::new_unique(),
            &shutdown,
            1,
            &Wakeup::default(),
        )
        .unwrap();
        shutdown.request();
        workers.wait_stopped(Duration::from_secs(2)).unwrap();
    }

    #[test]
    fn winner_watch_keeps_retry_deadline() {
        let w = WinnerWatch {
            revision: 0,
            account: Pubkey::new_unique(),
            rx_slot: 1,
            detect_slot: 2,
            sign_slot: Some(3),
            expected_net: Some(4),
            shadow_ready: true,
            our_loss: None,
            created_unix_s: 5,
            next_check_unix_s: 65,
        };
        assert_eq!(w.next_check_unix_s, 65);
        assert!(w.shadow_ready);
    }
}

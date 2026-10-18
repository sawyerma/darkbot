//! [FIX-108][R4] Remote RPC work isolated from the ingress/detection loop.
//!
//! Winner lookup and submission reconciliation own their own RpcClient instances.
//! The state thread only sends/receives bounded native work items.

use crate::analytics::record::LossReason;
use crate::execution::pending::{try_reconcile, PendingSubmission, ReconcileResult};
use crate::market::winner::{find_winner, Winner};
use darkbot_common::channel::{bounded, ChannelError, Consumer, Producer};
use darkbot_common::lifecycle::Shutdown;
use darkbot_common::timing::Clock;
use darkbot_solana::submit::rpc::RpcSubmitter;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::{hash::Hash, transaction::VersionedTransaction};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

pub struct WinnerResult {
    pub watch: WinnerWatch,
    pub result: Result<Option<Winner>, String>,
}

pub struct ReconcileWork {
    pub submission: PendingSubmission,
    pub next_check_unix_s: u64,
}

pub struct ReconcileResultMessage {
    pub work: ReconcileWork,
    /// `Err` = the reconcile could not run (wall clock unreadable); never a fabricated time.
    pub result: Result<Option<ReconcileResult>, &'static str>,
}

/// [P8] Reconcile timeouts are measured against the real wall clock. An unreadable clock is an
/// error for the owner (financial HALT), never epoch 0 handed to `try_reconcile`.
fn reconcile_at(
    now_unix_s: Result<u64, &'static str>,
    run: impl FnOnce(u64) -> Option<ReconcileResult>,
) -> Result<Option<ReconcileResult>, &'static str> {
    now_unix_s.map(run)
}

pub struct ProjectionWork {
    pub account: Pubkey,
    pub base_slot: u64,
    pub base_account_hash: Hash,
    pub t_rx_ns: u64,
    pub tx: VersionedTransaction,
    pub targets: Vec<Pubkey>,
}

pub struct ProjectionResultMessage {
    pub account: Pubkey,
    pub base_slot: u64,
    pub base_account_hash: Hash,
    pub t_rx_ns: u64,
    pub result: Result<darkbot_solana::submit::rpc::PostAccountsSimulation, String>,
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

pub struct RemoteWorkers {
    threads: Vec<std::thread::JoinHandle<()>>,
    pub winner_jobs: Producer<WinnerWatch>,
    pub winner_results: Consumer<WinnerResult>,
    pub reconcile_jobs: Producer<ReconcileWork>,
    pub reconcile_results: Consumer<ReconcileResultMessage>,
    pub projection_jobs: Producer<ProjectionWork>,
    pub projection_results: Consumer<ProjectionResultMessage>,
    pub oracle_price_projection_jobs: Producer<OraclePriceProjectionWork>,
    pub oracle_price_projection_results: Consumer<OraclePriceProjectionResultMessage>,
}

fn push_result<T>(tx: &Producer<T>, mut value: T, shutdown: &Shutdown) -> bool {
    loop {
        if shutdown.is_requested() {
            return false;
        }
        match tx.try_push(value) {
            Ok(()) => return true,
            Err(ChannelError::Full(v)) => {
                value = v;
                std::thread::sleep(Duration::from_millis(1));
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

    pub fn start(
        rpc_url: String,
        program: Pubkey,
        shutdown: &Shutdown,
        capacity: usize,
    ) -> std::io::Result<Self> {
        let (winner_jobs, winner_job_rx) = bounded::<WinnerWatch>("winner-rpc-jobs", capacity);
        let (winner_done_tx, winner_results) =
            bounded::<WinnerResult>("winner-rpc-results", capacity);
        let (reconcile_jobs, reconcile_job_rx) =
            bounded::<ReconcileWork>("reconcile-rpc-jobs", capacity);
        let (reconcile_done_tx, reconcile_results) =
            bounded::<ReconcileResultMessage>("reconcile-rpc-results", capacity);
        let (projection_jobs, projection_job_rx) =
            bounded::<ProjectionWork>("projection-rpc-jobs", capacity);
        let (projection_done_tx, projection_results) =
            bounded::<ProjectionResultMessage>("projection-rpc-results", capacity);
        let (oracle_price_projection_jobs, oracle_price_projection_job_rx) =
            bounded::<OraclePriceProjectionWork>("oracle-price-projection-rpc-jobs", capacity);
        let (oracle_price_projection_done_tx, oracle_price_projection_results) =
            bounded::<OraclePriceProjectionResultMessage>(
                "oracle-price-projection-rpc-results",
                capacity,
            );

        let mut threads = Vec::new();
        let winner_rpc_url = rpc_url.clone();
        let winner_shutdown = shutdown.clone();
        threads.push(
            std::thread::Builder::new()
                .name("winner-rpc".into())
                .spawn(move || {
                    let rpc = darkbot_solana::provider::rpc_client(
                        winner_rpc_url,
                        Duration::from_secs(30),
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
                        ) {
                            return;
                        }
                    }
                })?,
        );

        let reconcile_shutdown = shutdown.clone();
        let reconcile_rpc_url = rpc_url.clone();
        threads.push(
            std::thread::Builder::new()
                .name("reconcile-rpc".into())
                .spawn(move || {
                    let rpc = darkbot_solana::provider::rpc_client(
                        reconcile_rpc_url,
                        Duration::from_secs(30),
                        Default::default(),
                        darkbot_solana::provider::Workload::Winner,
                    );
                    let clock = Clock::new();
                    while !reconcile_shutdown.is_requested() {
                        let Some(work) = reconcile_job_rx.pop_timeout(Duration::from_millis(250))
                        else {
                            continue;
                        };
                        let now_s = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .map_err(|_| "WALL_CLOCK_BEFORE_UNIX_EPOCH");
                        let result = reconcile_at(now_s, |now_s| {
                            try_reconcile(&rpc, None, &work.submission, now_s, clock.now_ns(), 90)
                        });
                        if !push_result(
                            &reconcile_done_tx,
                            ReconcileResultMessage { work, result },
                            &reconcile_shutdown,
                        ) {
                            return;
                        }
                    }
                })?,
        );

        let projection_shutdown = shutdown.clone();
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
                        let mt0_start = std::time::Instant::now(); // [MT0]
                        let result = submitter
                            .simulate_with_post_accounts(&work.tx, &work.targets)
                            .map_err(|e| e.to_string());
                        let mt0_outcome = match &result {
                            Ok(_) => darkbot_common::mt0::RemoteOutcome::Ok,
                            Err(e) => darkbot_common::mt0::RemoteOutcome::classify_text(e),
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
                                result,
                            },
                            &projection_shutdown,
                        ) {
                            return;
                        }
                    }
                })?,
        );

        let oracle_projection_shutdown = shutdown.clone();
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
                        let result = submitter
                            .simulate_with_post_accounts(&work.tx, &work.targets)
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
            reconcile_jobs,
            reconcile_results,
            projection_jobs,
            projection_results,
            oracle_price_projection_jobs,
            oracle_price_projection_results,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_result_queue_does_not_block_shutdown() {
        let (tx, _rx) = bounded("stop-test", 1);
        tx.try_push(1_u64).unwrap();
        let shutdown = Shutdown::new();
        let stop = shutdown.clone();
        let worker = std::thread::spawn(move || push_result(&tx, 2, &stop));
        shutdown.request();
        assert!(!worker.join().unwrap());
    }

    #[test]
    fn remote_workers_confirm_actual_thread_exit() {
        let shutdown = Shutdown::new();
        let workers = RemoteWorkers::start(
            "http://127.0.0.1:1".into(),
            Pubkey::new_unique(),
            &shutdown,
            1,
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

    #[test]
    fn p8_unreadable_wall_clock_is_an_error_not_epoch_zero() {
        let mut called = false;
        let r = super::reconcile_at(Err("WALL_CLOCK_BEFORE_UNIX_EPOCH"), |_| {
            called = true;
            None
        });
        assert_eq!(r.err(), Some("WALL_CLOCK_BEFORE_UNIX_EPOCH"));
        assert!(!called, "try_reconcile must not run with a fabricated time");
        let mut seen = None;
        assert!(super::reconcile_at(Ok(1_700_000_000), |t| {
            seen = Some(t);
            None
        })
        .is_ok_and(|r| r.is_none()));
        assert_eq!(seen, Some(1_700_000_000));
    }
}

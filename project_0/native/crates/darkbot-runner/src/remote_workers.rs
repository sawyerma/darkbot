//! [FIX-108][R4] Remote RPC work isolated from the ingress/detection loop.
//!
//! Winner lookup and submission reconciliation own their own RpcClient instances.
//! The state thread only sends/receives bounded native work items.

use crate::pending::{try_reconcile, PendingSubmission, ReconcileResult};
use crate::record::LossReason;
use crate::winner::{find_winner, Winner};
use darkbot_solana::submit::rpc::RpcSubmitter;
use solana_sdk::{account::Account, transaction::VersionedTransaction};
use darkbot_common::channel::{bounded, ChannelError, Consumer, Producer};
use darkbot_common::lifecycle::Shutdown;
use darkbot_common::timing::Clock;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct WinnerWatch {
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
    pub result: Option<ReconcileResult>,
}

pub struct ProjectionWork {
    pub account: Pubkey,
    pub base_slot: u64,
    pub tx: VersionedTransaction,
    pub targets: Vec<Pubkey>,
}

pub struct ProjectionResultMessage {
    pub account: Pubkey,
    pub base_slot: u64,
    pub result: Result<(u64, Vec<(Pubkey, Account)>, Option<u64>), String>,
}

pub struct RemoteWorkers {
    pub winner_jobs: Producer<WinnerWatch>,
    pub winner_results: Consumer<WinnerResult>,
    pub reconcile_jobs: Producer<ReconcileWork>,
    pub reconcile_results: Consumer<ReconcileResultMessage>,
    pub projection_jobs: Producer<ProjectionWork>,
    pub projection_results: Consumer<ProjectionResultMessage>,
}

fn push_result<T>(tx: &Producer<T>, mut value: T) -> bool {
    loop {
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
    pub fn start(
        rpc_url: String,
        program: Pubkey,
        shutdown: &Shutdown,
        capacity: usize,
    ) -> std::io::Result<Self> {
        let (winner_jobs, winner_job_rx) = bounded::<WinnerWatch>("winner-rpc-jobs", capacity);
        let (winner_done_tx, winner_results) = bounded::<WinnerResult>("winner-rpc-results", capacity);
        let (reconcile_jobs, reconcile_job_rx) = bounded::<ReconcileWork>("reconcile-rpc-jobs", capacity);
        let (reconcile_done_tx, reconcile_results) =
            bounded::<ReconcileResultMessage>("reconcile-rpc-results", capacity);
        let (projection_jobs, projection_job_rx) =
            bounded::<ProjectionWork>("projection-rpc-jobs", capacity);
        let (projection_done_tx, projection_results) =
            bounded::<ProjectionResultMessage>("projection-rpc-results", capacity);

        let winner_rpc_url = rpc_url.clone();
        let winner_shutdown = shutdown.clone();
        std::thread::Builder::new().name("winner-rpc".into()).spawn(move || {
            let rpc = RpcClient::new(winner_rpc_url);
            while !winner_shutdown.is_requested() {
                let Some(watch) = winner_job_rx.pop_timeout(Duration::from_millis(250)) else {
                    continue;
                };
                let result = find_winner(
                    &rpc,
                    &program,
                    &watch.account,
                    watch.rx_slot.saturating_sub(2),
                );
                if !push_result(&winner_done_tx, WinnerResult { watch, result }) {
                    return;
                }
            }
        })?;

        let reconcile_shutdown = shutdown.clone();
        std::thread::Builder::new().name("reconcile-rpc".into()).spawn(move || {
            let rpc = RpcClient::new(rpc_url);
            let clock = Clock::new();
            while !reconcile_shutdown.is_requested() {
                let Some(work) = reconcile_job_rx.pop_timeout(Duration::from_millis(250)) else {
                    continue;
                };
                let now_s = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let result = try_reconcile(
                    &rpc,
                    None,
                    &work.submission,
                    now_s,
                    clock.now_ns(),
                    90,
                );
                if !push_result(
                    &reconcile_done_tx,
                    ReconcileResultMessage { work, result },
                ) {
                    return;
                }
            }
        })?;

        let projection_shutdown = shutdown.clone();
        let projection_rpc_url = rpc_url.clone();
        std::thread::Builder::new()
            .name("projection-rpc".into())
            .spawn(move || {
                let submitter = RpcSubmitter::new(projection_rpc_url, true);
                while !projection_shutdown.is_requested() {
                    let Some(work) =
                        projection_job_rx.pop_timeout(Duration::from_millis(250))
                    else {
                        continue;
                    };
                    let account = work.account;
                    let base_slot = work.base_slot;
                    let result = submitter
                        .simulate_with_post_accounts(&work.tx, &work.targets)
                        .map_err(|e| e.to_string());
                    if !push_result(
                        &projection_done_tx,
                        ProjectionResultMessage {
                            account,
                            base_slot,
                            result,
                        },
                    ) {
                        return;
                    }
                }
            })?;

        Ok(Self {
            winner_jobs,
            winner_results,
            reconcile_jobs,
            reconcile_results,
            projection_jobs,
            projection_results,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winner_watch_keeps_retry_deadline() {
        let w = WinnerWatch {
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

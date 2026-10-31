//! P6-MT.13: the RPC budget records per workload the calls in flight at each grant, its own
//! admissions queued at each new one and every refusal — in a process of its own, so the
//! process-wide MT0 registry counts exactly this test's admissions.
use darkbot_common::mt0::{AdmissionClass, MT0};
use darkbot_solana::provider::{Budget, Workload};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn p6_mt13_the_budget_records_inflight_queue_depth_and_refusals_per_workload() {
    let history = || {
        MT0.mt13()
            .admission
            .into_iter()
            .find(|a| a.class == AdmissionClass::History.name())
            .unwrap()
    };
    let mut c = darkbot_solana::runtime_config::RuntimeConfig::default();
    (
        c.limits.rpc_rps,
        c.limits.rpc_burst,
        c.limits.rpc_concurrency,
    ) = (1_000, 10, 1);
    c.limits.history_rps = 1_000;
    let b = Arc::new(Budget::configured(&c));
    let held = b
        .acquire(Workload::History, Instant::now() + Duration::from_secs(1))
        .unwrap();
    // The one slot is held: a second admission queues and is refused at its deadline.
    assert!(b
        .acquire(
            Workload::History,
            Instant::now() + Duration::from_millis(30)
        )
        .is_err());
    drop(held);
    let h = history();
    assert_eq!(
        (
            h.inflight.count,
            h.inflight.max,
            h.queue_depth.count,
            h.queue_depth.max,
            h.denied
        ),
        (1, Some(1), 2, Some(1), 1),
        "{h:?}"
    );
    // Every other workload saw nothing.
    assert!(MT0
        .mt13()
        .admission
        .iter()
        .filter(|a| a.class != AdmissionClass::History.name())
        .all(|a| a.inflight.count == 0 && a.queue_depth.count == 0 && a.denied == 0));
}

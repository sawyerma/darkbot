// A3 (included in the canonical `regression` tests): the offline join admits exactly the
// evaluations the live chain admits (`Worker::hour`).

/// A3: an evaluation before the capture start or after the cutoff never joins, live or offline
/// (NOT_DETECTED, no exact plan match); an in-window one joins on both sides; the joined facts
/// and the matching counts are identical. Without the window the offline join would count the
/// out-of-window evaluation.
#[test]
fn a3_offline_join_admits_only_the_live_window() {
    // The test worker's window is [1, 4_000_000_001] with a 4 s plan.
    for (t_rx, inside) in [(0u64, false), (5_000_000_000, false), (2_000_000_000, true)] {
        let mut w = worker();
        let mut r = identified();
        r.timing.t_rx_ns = Some(t_rx);
        w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
        let event = row(&r, 10, 0).market;
        w.canonical_market.push(event.clone());
        w.finalize_canonical().unwrap();
        let live = w.canonical_joined[0].clone();
        let plan = w.h.cfg.plan.clone();
        let shadow = w.h.cfg.run_dir.join("shadow_evaluations.jsonl");
        let offline = |capture| {
            join(
                vec![event.clone()],
                &shadow,
                std::time::Instant::now() + Duration::from_secs(10),
                &JoinContext {
                    market_commitment: &w.commitment,
                    slot_first_seen: None,
                    capture,
                },
            )
            .unwrap()
            .rows
            .remove(0)
        };
        let filtered = offline(Some(CaptureFilter {
            plan: &plan,
            start_ns: 1,
            cutoff_ns: 4_000_000_001,
        }));
        assert_eq!(joined_facts(&live), joined_facts(&filtered), "t_rx {t_rx}");
        assert_eq!(
            summarize(&[&live], 4., 0, true).matching,
            summarize(&[&filtered], 4., 0, true).matching
        );
        assert_eq!((live.target_seen, live.our.is_some()), (inside, inside));
        if !inside {
            assert_eq!(joined_facts(&live).loss_reason, "NOT_DETECTED");
            assert!(offline(None).target_seen, "unfiltered offline join diverges");
        }
    }
}

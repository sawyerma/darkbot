// Included in observer tests: exercise the production full-slot reconciler.
fn canonical_core(lo: u64, hi: u64) -> (ObserverCore, VecSink) {
    let mut c = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    c.reconcile_cfg.max_attempts = 1;
    c.reconcile_cfg.finality_wait_secs = 0;
    let mut s = VecSink::default();
    gap_slot_frame(&mut c, &mut s, lo, 10);
    gap_slot_frame(&mut c, &mut s, hi, 20);
    c.stop_observation();
    (c, s)
}
fn canonical_block(slot: u64, parent: u64, txs: Vec<Value>) -> Value {
    json!({"blockhash":format!("block-{slot}"),"previousBlockhash":format!("block-{parent}"),
        "parentSlot":parent,"blockTime":1000+slot,"transactions":txs})
}
fn canonical_tx(sig: &str, slot: u64) -> Value {
    let mut tx = gap_liquidation(sig, slot, false);
    tx["meta"]["innerInstructions"] = json!([]);
    tx
}
#[test]
fn full_slot_every_produced_block_and_transaction_are_scanned() {
    let (mut c, mut s) = canonical_core(10, 12);
    let mut fetched = vec![];
    c.reconcile_slots(
        &mut |m, p, _| {
            Ok(match m {
                "getSlot" => json!(12),
                "getBlocks" => json!([10, 11, 12]),
                "getBlock" => {
                    let slot = p[0].as_u64().unwrap();
                    fetched.push(slot);
                    canonical_block(
                        slot,
                        slot - 1,
                        vec![canonical_tx(&format!("s{slot}"), slot)],
                    )
                }
                _ => panic!("unexpected method"),
            })
        },
        &mut s,
    )
    .unwrap();
    assert!(c.reconciliation.complete, "{:?}", c.reconciliation.error);
    assert_eq!(fetched, vec![10, 11, 12]);
    assert_eq!(c.reconciliation.transactions_scanned, 3);
    assert_eq!(c.stats.liquidation_events, 3);
    assert!(s
        .tx
        .iter()
        .any(|v| v["source"] == "full_slot_reconciliation"));
    // Slot proof alone cannot qualify the as-yet incomplete path audit/decoders.
    assert_eq!(
        c.summary(&ctx(true, true, 4.0))["market_capture_complete"],
        false
    );
}
#[test]
fn full_slot_skipped_slots_require_finalized_parent_proof_including_tail() {
    let (mut c, mut s) = canonical_core(10, 13);
    c.reconcile_slots(
        &mut |m, p, _| {
            Ok(match m {
                "getSlot" => json!(14),
                "getBlocks" => json!([10, 12, 14]),
                "getBlock" => {
                    let slot = p[0].as_u64().unwrap();
                    canonical_block(slot, if slot == 10 { 9 } else { slot - 2 }, vec![])
                }
                _ => panic!(),
            })
        },
        &mut s,
    )
    .unwrap();
    assert!(c.reconciliation.complete);
    assert_eq!(c.reconciliation.slots_skipped_confirmed, 2);
    assert_eq!(c.reconciliation.slots_fetched, 2);
    assert_eq!(c.reconciliation.slots_unresolved, 0);
}
#[test]
fn full_slot_missing_produced_block_cannot_masquerade_as_skipped() {
    for bad_hash in [false, true] {
        let (mut c, mut s) = canonical_core(10, 12);
        c.reconcile_slots(
            &mut |m, p, _| {
                Ok(match m {
                    "getSlot" => json!(12),
                    "getBlocks" => json!([10, 12]),
                    "getBlock" => {
                        let slot = p[0].as_u64().unwrap();
                        let mut b = canonical_block(slot, slot - 1, vec![]);
                        if bad_hash && slot == 12 {
                            b["parentSlot"] = json!(10);
                            b["previousBlockhash"] = json!("wrong");
                        }
                        b
                    }
                    _ => panic!(),
                })
            },
            &mut s,
        )
        .unwrap();
        assert!(!c.reconciliation.complete);
        assert_eq!(
            c.reconciliation.error,
            Some("FINALIZED_PARENT_CHAIN_MISMATCH")
        );
        assert_eq!(c.reconciliation.slots_skipped_confirmed, 0);
    }
}
#[test]
fn full_slot_unproven_tail_rpc_failure_and_future_window_fail_closed() {
    for case in 0..3 {
        let (mut c, mut s) = canonical_core(10, 12);
        c.reconcile_slots(
            &mut |m, p, _| match m {
                "getSlot" => Ok(json!(if case == 2 { 11 } else { 12 })),
                "getBlocks" => Ok(json!([10])),
                "getBlock" if case == 1 => Err("RPC_TIMEOUT"),
                "getBlock" => Ok(canonical_block(p[0].as_u64().unwrap(), 9, vec![])),
                _ => panic!(),
            },
            &mut s,
        )
        .unwrap();
        assert!(!c.reconciliation.complete);
        assert_eq!(
            c.reconciliation.error,
            Some(
                [
                    "TRAILING_SLOT_UNPROVEN",
                    "RPC_TIMEOUT",
                    "WINDOW_NOT_FINALIZED"
                ][case]
            )
        );
    }
}
#[test]
fn full_slot_cannot_run_in_capture_or_without_boundaries() {
    let mut c = open_core(MarketSource::Logs, EnrichConfig::DEFAULT);
    let mut s = VecSink::default();
    assert!(c
        .reconcile_slots(&mut |_, _, _| panic!("hotpath RPC"), &mut s)
        .is_err());
    c.stop_observation();
    c.reconcile_slots(&mut |_, _, _| panic!("no window"), &mut s)
        .unwrap();
    assert_eq!(
        c.reconciliation.error,
        Some("CANONICAL_SLOT_BOUNDARY_MISSING")
    );
}
#[test]
fn full_slot_repeat_and_live_duplicate_do_not_double_count() {
    let (mut c, mut s) = canonical_core(10, 10);
    let tx = canonical_tx("live", 10);
    c.start_observation(0, 100);
    c.on_transaction(&as_observed(&tx, "live", 10, false), 0, &mut s)
        .unwrap();
    c.stop_observation();
    c.reconcile_slots(
        &mut |m, _, _| {
            Ok(match m {
                "getSlot" => json!(10),
                "getBlocks" => json!([10]),
                "getBlock" => canonical_block(10, 9, vec![tx.clone()]),
                _ => panic!(),
            })
        },
        &mut s,
    )
    .unwrap();
    c.reconcile_slots(&mut |_, _, _| panic!("repeat RPC"), &mut s)
        .unwrap();
    assert!(c.reconciliation.complete);
    assert_eq!(c.stats.liquidation_events, 1);
}
#[test]
fn full_slot_live_duplicate_still_validates_instruction_event_parity() {
    let (mut c, mut s) = canonical_core(10, 10);
    let mut tx = canonical_tx("live", 10);
    c.start_observation(0, 100);
    c.on_transaction(&as_observed(&tx, "live", 10, false), 0, &mut s)
        .unwrap();
    c.stop_observation();
    tx["meta"]["logMessages"] = json!([]);
    c.reconcile_slots(
        &mut |m, _, _| {
            Ok(match m {
                "getSlot" => json!(10),
                "getBlocks" => json!([10]),
                "getBlock" => canonical_block(10, 9, vec![tx.clone()]),
                _ => panic!(),
            })
        },
        &mut s,
    )
    .unwrap();
    assert!(!c.reconciliation.complete);
    assert_eq!(
        c.reconciliation.error,
        Some("RECEIVER_INSTRUCTION_EVENT_MISMATCH")
    );
}
#[test]
fn full_transaction_rejects_unsupported_version_and_bad_inner_index() {
    for case in 0..3 {
        let mut tx = canonical_tx("a", 10);
        if case == 0 {
            tx["version"] = json!(2);
        }
        if case == 1 {
            tx["meta"]["innerInstructions"] = Value::Null;
        }
        if case == 2 {
            tx["meta"]["innerInstructions"] = json!([{"index":999,"instructions":[]}]);
        }
        assert!(reconcile::validate_full_transaction(&tx).is_err());
    }
}
#[test]
fn full_transaction_checks_loaded_address_counts_even_when_present() {
    let mut tx = canonical_tx("a", 10);
    tx["version"] = json!(0);
    tx["transaction"]["message"]["addressTableLookups"] = json!([{"accountKey":Pubkey::new_unique().to_string(),"writableIndexes":[0],"readonlyIndexes":[]}]);
    tx["meta"]["loadedAddresses"] = json!({"writable":[],"readonly":[]});
    assert!(matches!(
        reconcile::validate_full_transaction(&tx),
        Err("LOADED_ADDRESS_COUNT_MISMATCH")
    ));
    tx["meta"]["loadedAddresses"]["writable"] = json!([Pubkey::new_unique().to_string()]);
    assert!(reconcile::validate_full_transaction(&tx).is_ok());
}
#[test]
fn full_slot_budget_and_large_range_are_bounded() {
    let (mut c, mut s) = canonical_core(10, 5010);
    c.reconcile_cfg.max_slots = 5000;
    c.reconcile_slots(&mut |_, _, _| panic!("budget"), &mut s)
        .unwrap();
    assert_eq!(c.reconciliation.error, Some("RECONCILIATION_SLOT_BUDGET"));
    let (mut c, mut s) = canonical_core(10, 5009);
    c.reconcile_slots(
        &mut |m, p, _| {
            Ok(match m {
                "getSlot" => json!(5009),
                "getBlocks" => json!((10..=5009).collect::<Vec<u64>>()),
                "getBlock" => {
                    let slot = p[0].as_u64().unwrap();
                    canonical_block(slot, slot - 1, vec![])
                }
                _ => panic!(),
            })
        },
        &mut s,
    )
    .unwrap();
    assert!(c.reconciliation.complete);
    assert_eq!(c.reconciliation.slots_fetched, 5000);
    assert!(c.seen.is_empty());
}

#[test]
fn full_slot_transient_rate_limit_retries_without_skipping_block() {
    let (mut c, mut s) = canonical_core(10, 10);
    c.reconcile_cfg.max_attempts = 2;
    let mut calls = 0;
    c.reconcile_slots(
        &mut |m, _, _| match m {
            "getSlot" => Ok(json!(10)),
            "getBlocks" => Ok(json!([10])),
            "getBlock" => {
                calls += 1;
                if calls == 1 {
                    Err("RPC_RATE_LIMIT")
                } else {
                    Ok(canonical_block(10, 9, vec![]))
                }
            }
            _ => panic!(),
        },
        &mut s,
    )
    .unwrap();
    assert_eq!(calls, 2);
    assert!(c.reconciliation.complete);
    assert_eq!(c.reconciliation.slots_fetched, 1);
    assert_eq!(c.reconciliation.slots_skipped_confirmed, 0);
}

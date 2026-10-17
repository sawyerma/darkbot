fn classic_truth_fixture() -> Value {
    use sha256_fixture_discriminator as disc;
    let keys: Vec<_> = (0..7).map(|_| Pubkey::new_unique()).collect();
    let mut data = disc("event:LendingAccountLiquidateEvent").to_vec();
    data.push(1);
    data.extend(keys[4].to_bytes()); // header signer
    for i in [3, 4, 0, 5, 4, 1, 6, 2, 6] {
        data.extend(keys[i].to_bytes());
    }
    for n in [
        -10f64, -5., 1000., 500., 0., 0., 100., 900., 410., 100., 0., 0.,
    ] {
        data.extend(n.to_le_bytes());
    }
    let line = format!(
        "Program data: {}",
        base64::engine::general_purpose::STANDARD.encode(data)
    );
    let mut ix = disc("global:lending_account_liquidate").to_vec();
    ix.extend(100u64.to_le_bytes());
    ix.extend([2, 2]);
    let mut tx = canonical_tx("classic", 10);
    let mut key_strings: Vec<_> = keys.iter().map(ToString::to_string).collect();
    key_strings.push(program().to_string());
    tx["transaction"]["message"]["accountKeys"] = json!(key_strings);
    tx["transaction"]["message"]["instructions"] = json!([{"programIdIndex":7,"accounts":[0,1,2,3,4,5],"data":bs58::encode(ix).into_string()}]);
    tx["meta"]["logMessages"] = json!(logs_with(&program(), &[line]));
    tx["meta"]["fee"] = json!(7345);
    tx["transactionIndex"] = json!(0);
    tx
}
// Values derived independently from the pinned Anchor namespace/name definitions.
fn sha256_fixture_discriminator(name: &str) -> [u8; 8] {
    match name {
        "event:LendingAccountLiquidateEvent" => [0xa6, 0xa0, 0xf9, 0x9a, 0xb7, 0x27, 0x17, 0xf2],
        "global:lending_account_liquidate" => [0xd6, 0xa9, 0x97, 0xd5, 0xfb, 0xa7, 0x56, 0xdb],
        _ => panic!("unknown fixture"),
    }
}
/// Production order: decode the RPC transaction at the wire boundary, then classify it.
fn classify_raw(
    raw: &Value,
    block_time: Option<i64>,
    live: Option<u64>,
) -> Result<Vec<truth::TruthEvent>, &'static str> {
    let tx = darkbot_solana::chain::decode_transaction(raw).map_err(|e| e.0)?;
    truth::classify(&tx, &program(), block_time, live)
}

#[test]
fn truth_classic_decodes_native_amount_and_liability_deposit_cost() {
    let tx = classic_truth_fixture();
    let events = classify_raw(&tx, Some(1234), None).unwrap();
    assert_eq!(events.len(), 1);
    let e = &events[0];
    assert_eq!(e.liquidation_type, truth::LiquidationType::Classic);
    assert_eq!(e.seized_asset_native_exact.as_deref(), Some("100"));
    let Some(truth::NativeEvidence::Classic {
        liquidator_liability_cost_native_f64: cost,
        target_debt_reduction_native_f64: reduction,
        insurance_component_native_f64: insurance,
        ..
    }) = &e.native_evidence
    else {
        panic!("classic native evidence expected: {:?}", e.native_evidence)
    };
    assert_eq!((*cost, *reduction, *insurance), (100., 90., 10.));
    assert_eq!(e.gross_edge_usd, None); // no historical price, no current-price substitute
    assert_eq!(e.transaction_fee_lamports, Some(7345));
    assert_eq!(e.winner_net_edge_usd, None);
    assert!(!e.winner_net_edge_complete);
    assert_eq!(e.live_t_rx_ns, None);
    assert_eq!(e.discovery_source, "RECONCILIATION");
}
#[test]
fn truth_classic_failed_normal_and_mismatched_events() {
    let mut tx = classic_truth_fixture();
    tx["meta"]["err"] = json!({"InstructionError":[0,{"Custom":1}]});
    assert!(classify_raw(&tx, None, None).unwrap().is_empty());
    tx["meta"]["err"] = Value::Null;
    tx["meta"]["logMessages"] = json!([]);
    assert!(classify_raw(&tx, None, None).is_err());
    tx["transaction"]["message"]["instructions"] = json!([]);
    assert!(classify_raw(&tx, None, None).unwrap().is_empty());
}
#[test]
fn truth_classic_loaded_program_and_cpi_are_resolved() {
    let mut tx = classic_truth_fixture();
    tx["version"] = json!(0);
    tx["transaction"]["message"]["accountKeys"]
        .as_array_mut()
        .unwrap()
        .pop();
    tx["transaction"]["message"]["addressTableLookups"] = json!([{"accountKey":Pubkey::new_unique().to_string(),"writableIndexes":[],"readonlyIndexes":[0]}]);
    tx["meta"]["loadedAddresses"] = json!({"writable":[],"readonly":[program().to_string()]});
    assert_eq!(classify_raw(&tx, None, None).unwrap().len(), 1);
    let ix = tx["transaction"]["message"]["instructions"][0].clone();
    tx["transaction"]["message"]["instructions"] =
        json!([{"programIdIndex":0,"accounts":[],"data":""}]);
    tx["meta"]["innerInstructions"] = json!([{"index":0,"instructions":[ix]}]);
    assert_eq!(classify_raw(&tx, None, None).unwrap().len(), 1);
}
#[test]
fn truth_multiple_classic_events_in_one_transaction_are_not_collapsed() {
    let mut tx = classic_truth_fixture();
    let ix = tx["transaction"]["message"]["instructions"][0].clone();
    tx["transaction"]["message"]["instructions"]
        .as_array_mut()
        .unwrap()
        .push(ix);
    let lines = tx["meta"]["logMessages"].as_array().unwrap().clone();
    tx["meta"]["logMessages"]
        .as_array_mut()
        .unwrap()
        .extend(lines);
    let events = classify_raw(&tx, None, Some(42)).unwrap();
    assert_eq!(events.len(), 2);
    assert_ne!(events[0].event_index, events[1].event_index);
    assert_eq!(events[0].discovery_source, "BOTH");
}
#[test]
fn truth_receivership_reuses_six_real_f8_fixtures() {
    for fx in fixtures() {
        let mut tx: Value = serde_json::from_str(fx.raw).unwrap();
        tx["transactionIndex"] = json!(0);
        let events = classify_raw(&tx, None, None).unwrap();
        assert_eq!(events.len(), 1, "{}", fx.signature);
        assert_eq!(
            events[0].liquidation_type,
            truth::LiquidationType::Receivership
        );
        assert!((events[0].gross_edge_usd.unwrap() - fx.gross).abs() < 0.001);
        assert_eq!(events[0].transaction_fee_lamports, Some(fx.fee));
        assert_eq!(events[0].winner_net_edge_usd, None);
    }
}
#[test]
fn truth_priority_uses_requested_limit_not_consumed_units() {
    let mut tx = classic_truth_fixture();
    let keys = tx["transaction"]["message"]["accountKeys"]
        .as_array_mut()
        .unwrap();
    let index = keys.len();
    keys.push(json!(COMPUTE_BUDGET_PROGRAM));
    let mut limit = vec![2];
    limit.extend(100000u32.to_le_bytes());
    let mut price = vec![3];
    price.extend(10001u64.to_le_bytes());
    for data in [limit, price] {
        tx["transaction"]["message"]["instructions"].as_array_mut().unwrap().push(json!({"programIdIndex":index,"accounts":[],"data":bs58::encode(data).into_string()}));
    }
    tx["meta"]["computeUnitsConsumed"] = json!(100);
    let events = classify_raw(&tx, None, None).unwrap();
    assert_eq!(events[0].priority_fee_lamports, Some(1001));
    assert_eq!(events[0].transaction_fee_lamports, Some(7345));
}

fn administrative_truth_fixture(bankruptcy: bool) -> Value {
    let keys: Vec<_> = (0..5).map(|_| Pubkey::new_unique()).collect();
    let mut data = if bankruptcy {
        vec![0xa6, 0x4d, 0x29, 0x8c, 0x24, 0x5e, 0x0a, 0x39]
    } else {
        vec![0xa1, 0x08, 0x6c, 0xcc, 0xd1, 0xc6, 0x0c, 0x1e]
    };
    if bankruptcy {
        data.push(1);
        for i in [1, 3, 4, 0, 2, 4] {
            data.extend(keys[i].to_bytes());
        }
        for n in [100f64, 60., 40.] {
            data.extend(n.to_le_bytes());
        }
    } else {
        for i in [0, 3] {
            data.extend(keys[i].to_bytes());
        }
        for n in [11f64, 10.] {
            data.extend(n.to_le_bytes());
        }
    }
    let ix = if bankruptcy {
        vec![0xa2, 0x0b, 0x38, 0x8b, 0x5a, 0x80, 0x46, 0xad]
    } else {
        vec![0x72, 0x0e, 0xfa, 0x8f, 0xfc, 0x68, 0xd6, 0xd1]
    };
    let mut tx = canonical_tx("admin", 10);
    let mut key_strings: Vec<_> = keys.iter().map(ToString::to_string).collect();
    key_strings.push(program().to_string());
    tx["transaction"]["message"]["accountKeys"] = json!(key_strings);
    tx["transaction"]["message"]["instructions"] =
        json!([{"programIdIndex":5,"accounts":[0,1,2,3],"data":bs58::encode(ix).into_string()}]);
    tx["meta"]["logMessages"] = json!(logs_with(
        &program(),
        &[format!(
            "Program data: {}",
            base64::engine::general_purpose::STANDARD.encode(data)
        )]
    ));
    tx["transactionIndex"] = json!(0);
    tx
}
#[test]
fn truth_deleverage_and_bankruptcy_are_separate_from_permissionless_profit() {
    for bankruptcy in [false, true] {
        let e = classify_raw(&administrative_truth_fixture(bankruptcy), None, None)
            .unwrap()
            .remove(0);
        assert!(!e.permissionless_profit_opportunity);
        if bankruptcy {
            assert_eq!(
                e.liquidation_type,
                truth::LiquidationType::BankruptcySettlement
            );
            assert!(matches!(
                e.native_evidence,
                Some(truth::NativeEvidence::Bankruptcy { bad_debt_native_f64, .. }) if bad_debt_native_f64 == 100.
            ));
            assert_eq!(e.gross_edge_usd, None);
        } else {
            assert_eq!(e.liquidation_type, truth::LiquidationType::ForcedDeleverage);
            assert_eq!(e.gross_edge_usd, Some(1.));
            assert_eq!(e.protocol_fee_lamports, Some(0)); // source-proven zero
        }
    }
}
#[test]
fn truth_foreign_emitter_and_failed_cpi_never_produce_liquidation() {
    let mut tx = classic_truth_fixture();
    let logs = tx["meta"]["logMessages"].as_array().unwrap().clone();
    tx["meta"]["logMessages"] = json!(logs
        .iter()
        .map(|v| v
            .as_str()
            .unwrap()
            .replace(&program().to_string(), &Pubkey::new_unique().to_string()))
        .collect::<Vec<_>>());
    assert!(classify_raw(&tx, None, None).is_err());
    let mut logs = logs;
    *logs.last_mut().unwrap() = json!(format!(
        "Program {} failed: custom program error",
        program()
    ));
    tx["meta"]["logMessages"] = json!(logs);
    assert!(matches!(
        classify_raw(&tx, None, None),
        Err("RELEVANT_CPI_FAILED")
    ));
}
#[test]
fn truth_aggregate_all_types_preserves_known_and_unknown() {
    let mut ledger = truth::TruthLedger::default();
    for tx in [
        classic_truth_fixture(),
        administrative_truth_fixture(false),
        administrative_truth_fixture(true),
    ] {
        ledger.events.extend(classify_raw(&tx, None, None).unwrap());
    }
    let mut rx = canonical_tx("rx", 10);
    rx["transactionIndex"] = json!(0);
    ledger.events.extend(classify_raw(&rx, None, None).unwrap());
    let s = view(&ledger.summary(Some(3600.)));
    assert_eq!(s["groups"]["ALL"]["liquidation_events"], 4);
    assert_eq!(s["groups"]["ALL"]["gross_edge_usd"]["count_known"], 2);
    assert_eq!(s["groups"]["ALL"]["gross_edge_usd"]["count_unknown"], 2);
    assert_eq!(s["groups"]["ALL"]["gross_edge_usd"]["known_total"], 6.);
    assert_eq!(
        s["groups"]["CLASSIC"]["gross_edge_usd"]["known_total"],
        Value::Null
    );
    assert_eq!(
        s["groups"]["ALL"]["winner_net_edge_usd"]["known_total"],
        Value::Null
    );
}

#[test]
fn truth_mixed_real_receiver_and_source_classic_preserves_event_order() {
    // Actual finalized receiver fixture combined with pinned-source Classic bytes.
    // This is a composition test, not a claim of a real mixed transaction.
    let fx = &fixtures()[0];
    let mut real: Value = serde_json::from_str(fx.raw).unwrap();
    let classic = classic_truth_fixture();
    let keys_of = |v: &Value| -> Vec<String> {
        darkbot_solana::chain::decode_entry(v, v["slot"].as_u64().unwrap_or(0), None, None)
            .unwrap()
            .account_keys()
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect()
    };
    let mut combined = keys_of(&real);
    let base = combined.len() as u64;
    combined.extend(keys_of(&classic));
    let mut ix = classic["transaction"]["message"]["instructions"][0].clone();
    ix["programIdIndex"] = json!(ix["programIdIndex"].as_u64().unwrap() + base);
    for a in ix["accounts"].as_array_mut().unwrap() {
        *a = json!(a.as_u64().unwrap() + base);
    }
    real["transaction"]["message"]["accountKeys"] = json!(combined);
    real["transaction"]["message"]["addressTableLookups"] = json!([]);
    real["meta"]["loadedAddresses"] = json!({"writable":[],"readonly":[]});
    real["transaction"]["message"]["instructions"]
        .as_array_mut()
        .unwrap()
        .push(ix);
    real["meta"]["logMessages"]
        .as_array_mut()
        .unwrap()
        .extend(classic["meta"]["logMessages"].as_array().unwrap().clone());
    real["transactionIndex"] = json!(0);
    let events = classify_raw(&real, None, None).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].liquidation_type,
        truth::LiquidationType::Receivership
    );
    assert_eq!(events[0].event_index, 0);
    assert_eq!(events[1].liquidation_type, truth::LiquidationType::Classic);
    assert_eq!(events[1].event_index, 1);
}

#[test]
fn live_nonreceiver_events_reuse_existing_canonical_decoder() {
    for (tx, kind) in [
        (classic_truth_fixture(), truth::LiquidationType::Classic),
        (
            administrative_truth_fixture(false),
            truth::LiquidationType::ForcedDeleverage,
        ),
        (
            administrative_truth_fixture(true),
            truth::LiquidationType::BankruptcySettlement,
        ),
    ] {
        let logs = tx["meta"]["logMessages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let observed = decode_receiver_events(&logs, &program());
        assert_eq!(observed.decoder_errors, 0);
        assert!(observed.events.is_empty());
        assert_eq!(observed.additional_events.len(), 1);
        assert_eq!(observed.additional_events[0].kind, kind);
        let canonical = classify_raw(&tx, None, None).unwrap();
        assert_eq!(
            observed.additional_events[0].gross_edge_usd,
            canonical[0].gross_edge_usd
        );
    }
}

#[test]
fn live_nonreceiver_events_ignore_foreign_emitter() {
    let tx = classic_truth_fixture();
    let line = tx["meta"]["logMessages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|v| v.starts_with(PROGRAM_DATA_PREFIX))
        .unwrap()
        .to_owned();
    let foreign_logs = logs_with(&Pubkey::new_unique(), &[line]);
    let observed = decode_receiver_events(&foreign_logs, &program());
    assert_eq!(observed.decoder_errors, 0);
    assert!(observed.additional_events.is_empty());
}

#[test]
fn live_nonreceiver_corrupt_known_event_fails_closed() {
    let mut bytes = vec![166, 160, 249, 154, 183, 39, 23, 242];
    bytes.push(1);
    let data = format!(
        "Program data: {}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
    let observed = decode_receiver_events(&logs_with(&program(), &[data]), &program());
    assert_eq!(observed.decoder_errors, 1);
    assert!(observed.additional_events.is_empty());
}

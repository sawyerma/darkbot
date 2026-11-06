use super::*;

fn released(n: u8, intent: LiveIntentKind, token: u8, input: u64, most: u64) -> Fact {
    Fact {
        kind: FactKind::Released,
        tx_hash: [n; 32],
        intent,
        component: ComponentId::StrategyC,
        token: [token; 20],
        nonce: u64::from(n),
        grant_generation: 7,
        input_wei: Some(U256::from(input)),
        max_wei: Some(U256::from(most)),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(100),
        by_tx: None,
        response: None,
        origin: Some(crate::sources::tests::ORIGIN),
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
        venue: None,
        exits: None,
    }
}

/// SC4 d: a released fact keeps the venue its transaction trades on and
/// reads it back; its lot is that venue's; a lot of a fact before it is
/// Flap's.
#[test]
fn a_fact_keeps_its_venue_and_its_lot_is_that_venues() {
    let dir = std::env::temp_dir().join(format!("bsc-live-ledger-venue-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let ledger = Ledger::open(&dir);
    let four = Fact {
        venue: Some(crate::sources::VenueId::OpenFour),
        ..released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300)
    };
    let older = released(2, LiveIntentKind::Buy, 0xB, 1_000, 1_300);
    for f in [&four, &older] {
        ledger.append(f, 1).unwrap();
    }
    let book = fold(&ledger.facts().unwrap());
    assert_eq!(
        book.tx(&four.tx_hash).map(|t| t.released.venue),
        Some(Some(crate::sources::VenueId::OpenFour))
    );
    let venue = |token: u8| {
        book.positions
            .iter()
            .find(|p| p.token == [token; 20])
            .map(|p| p.venue)
    };
    assert_eq!(
        (venue(0xA), venue(0xB)),
        (
            Some(crate::sources::VenueId::OpenFour),
            Some(crate::sources::VenueId::Flap)
        )
    );
    fs::remove_dir_all(&dir).unwrap();
}

/// Decision 06.11. (review M1): a Strategy C BUY's release keeps its
/// entry's exit rules (v6) and reads them back; a v5 fact reads without
/// them; rules named in part, refused by the strategy's check, or on
/// another fact than a Strategy C BUY refuse the ledger.
#[test]
fn a_strategy_buy_keeps_its_exit_rules() {
    use darkbot_bsc_trading::strategy::engine::ExitTerms;
    let dir = std::env::temp_dir().join(format!("bsc-live-ledger-exits-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let rules = ExitTerms {
        ratio_places: 4,
        target_net_multiple: 20_000,
        stop_net_multiple: 5_000,
        hold_ms: 600_000,
        creator_sell_exit: false,
    };
    let buy = Fact {
        exits: Some(rules),
        ..released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300)
    };
    Ledger::open(&dir).append(&buy, 1).unwrap();
    let book = fold(&Ledger::open(&dir).facts().unwrap());
    assert_eq!(book.tx(&buy.tx_hash).unwrap().released.exits, Some(rules));
    // Its inclusion names none; the lot keeps its release's.
    assert_eq!(Fact::about(&buy, FactKind::Included).exits, None);
    let raw = |seq: u64,
               schema: &'static darkbot_bsc_record::Schema,
               intent: &str,
               more: &dyn Fn(&mut Draft)| {
        let mut d = Draft::new(schema);
        d.set(k::KIND, "RELEASED")
            .set(k::TX_HASH, Value::B256([seq as u8; 32]))
            .set(k::PROVENANCE_TOOL, "t")
            .set(k::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(k::INTENT, intent)
            .set(k::COMPONENT, "BSC_STRATEGY_C")
            .set(k::TOKEN, Value::Address([0xB; 20]))
            .set(k::NONCE, seq)
            .set(k::GRANT_GENERATION, 7u64);
        more(&mut d);
        let path = dir.join("live-ledger").join(format!("{seq:020}.dbrec"));
        fs::write(&path, d.encode().unwrap()).unwrap();
        path
    };
    let all = |d: &mut Draft, stop: u128| {
        d.set(k::EXIT_RATIO_PLACES, 4u64)
            .set(k::EXIT_TARGET_NET_MULTIPLE, Value::u128(20_000))
            .set(k::EXIT_STOP_NET_MULTIPLE, Value::u128(stop))
            .set(k::EXIT_HOLD_MS, 600_000u64)
            .set(k::EXIT_CREATOR_SELL, false);
    };
    let v5 = raw(1, &k::V5, "BUY", &|_| {});
    assert_eq!(Ledger::open(&dir).facts().unwrap()[1].exits, None);
    fs::remove_file(v5).unwrap();
    type Refused<'a> = (&'a dyn Fn(&mut Draft), &'a str, &'a str);
    let refused: [Refused<'_>; 3] = [
        (
            &|d| {
                d.set(k::EXIT_HOLD_MS, 1u64);
            },
            "BUY",
            "named whole",
        ),
        (&|d| all(d, 10_000), "BUY", "below 1x"),
        (&|d| all(d, 5_000), "SELL", "only on a Strategy C BUY"),
    ];
    for (more, intent, why) in refused {
        let path = raw(1, &k::SCHEMA, intent, more);
        let e = Ledger::open(&dir).facts().unwrap_err();
        assert!(e.contains(why), "{why}: {e}");
        fs::remove_file(path).unwrap();
    }
    fs::remove_dir_all(&dir).unwrap();
}

/// Budget fix: a fact names its nonce's sender and chain and reads them
/// back (its alternatives group by them); a fact of an older build
/// reads without them (grouped with none); one naming only one of them
/// refuses the ledger.
#[test]
fn a_fact_keeps_the_sender_and_chain_of_its_nonce() {
    let dir = std::env::temp_dir().join(format!("bsc-live-origin-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let ledger = Ledger::open(&dir);
    let buy = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
    ledger.append(&buy, 1).unwrap();
    let raw = |schema: &'static darkbot_bsc_record::Schema, seq: u64, chain: Option<u64>| {
        let mut d = Draft::new(schema);
        d.set(k::KIND, "RELEASED")
            .set(k::TX_HASH, Value::B256([seq as u8 + 1; 32]))
            .set(k::PROVENANCE_TOOL, "test")
            .set(k::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(k::INTENT, "BUY")
            .set(k::COMPONENT, "BSC_EXEC")
            .set(k::TOKEN, Value::Address([0xB; 20]))
            .set(k::NONCE, 1u64)
            .set(k::GRANT_GENERATION, 7u64)
            .set_opt(k::CHAIN_ID, chain);
        let path = dir.join("live-ledger").join(format!("{seq:020}.dbrec"));
        fs::write(path, d.encode().unwrap()).unwrap();
    };
    raw(&k::V1, 1, None);
    let facts = ledger.facts().unwrap();
    assert_eq!(facts[0], buy);
    assert_eq!(
        facts[0].key(),
        Some(NonceKey {
            origin: crate::sources::tests::ORIGIN,
            nonce: 1
        })
    );
    assert_eq!((facts[1].origin, facts[1].key()), (None, None));
    raw(&k::SCHEMA, 2, Some(56));
    assert!(ledger.facts().unwrap_err().contains("sender and chain"));
    // A sender without its chain neither.
    let mut d = Draft::new(&k::SCHEMA);
    d.set(k::KIND, "RELEASED")
        .set(k::TX_HASH, Value::B256([9; 32]))
        .set(k::PROVENANCE_TOOL, "test")
        .set(k::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
        .set(k::INTENT, "BUY")
        .set(k::COMPONENT, "BSC_EXEC")
        .set(k::TOKEN, Value::Address([0xB; 20]))
        .set(k::NONCE, 1u64)
        .set(k::GRANT_GENERATION, 7u64)
        .set(k::SENDER, Value::Address([0xb0; 20]));
    let path = dir.join("live-ledger").join(format!("{:020}.dbrec", 2));
    fs::write(path, d.encode().unwrap()).unwrap();
    assert!(ledger.facts().unwrap_err().contains("sender and chain"));
    fs::remove_dir_all(dir).unwrap();
}

/// REV.1.0 R2: a `PLACED` fact keeps the placement and the launch's
/// creator and reads them back; the fold puts them on its transaction
/// (an orphaned inclusion takes the placement back); a v2 entry (before
/// them) reads without them; a placement named in part refuses the
/// ledger.
#[test]
fn a_placement_and_its_launcher_read_back_and_fold() {
    let dir = std::env::temp_dir().join(format!("bsc-live-placed-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let ledger = Ledger::open(&dir);
    let buy = Fact {
        component: ComponentId::ExecC3,
        ..released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300)
    };
    ledger.append(&buy, 1).unwrap();
    let block = Some((100, [0x10; 32], 5));
    let included = Fact {
        block,
        tokens: Some(U256::from(7u8)),
        eth_wei: Some(U256::from(1_000u64)),
        gas_wei: Some(U256::ZERO),
        ..Fact::about(&buy, FactKind::Included)
    };
    let placed = Fact {
        block,
        launcher: Some([0xC1; 20]),
        placement: Some(Placement {
            verdict: PlacementVerdict::Late,
            launch_index: Some(0),
            own_index: 2,
            buys_before: 1,
        }),
        ..Fact::about(&buy, FactKind::Placed)
    };
    ledger.append(&included, 2).unwrap();
    ledger.append(&placed, 3).unwrap();
    let facts = ledger.facts().unwrap();
    assert_eq!(facts[2], placed);
    let book = fold(&facts);
    let t = book.tx(&buy.tx_hash).unwrap();
    assert_eq!(t.placement, placed.placement);
    // The BUY path's question: this grant has a misplaced BUY.
    assert_eq!(book.misplaced, [buy.grant_generation].into());
    assert_eq!(t.launcher, Some([0xC1; 20]));
    assert_eq!(t.state, FactKind::Included);
    let orphaned = Fact {
        block,
        ..Fact::about(&buy, FactKind::Orphaned)
    };
    let mut again = facts.clone();
    again.push(orphaned);
    let again = fold(&again);
    assert_eq!(again.tx(&buy.tx_hash).unwrap().placement, None);
    // The halt stays: re-enabling is the operator's new grant (L2).
    assert_eq!(again.misplaced, [buy.grant_generation].into());
    // A v2 entry (no launcher, no placement) still reads.
    let mut d = Draft::new(&k::V2);
    d.set(k::KIND, "RELEASED")
        .set(k::TX_HASH, Value::B256([9; 32]))
        .set(k::PROVENANCE_TOOL, "test")
        .set(k::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
        .set(k::INTENT, "BUY")
        .set(k::COMPONENT, "BSC_EXEC")
        .set(k::TOKEN, Value::Address([0xB; 20]))
        .set(k::NONCE, 2u64)
        .set(k::GRANT_GENERATION, 7u64);
    fs::write(
        dir.join("live-ledger").join(format!("{:020}.dbrec", 4)),
        d.encode().unwrap(),
    )
    .unwrap();
    let facts = ledger.facts().unwrap();
    assert_eq!((facts[3].launcher, facts[3].placement), (None, None));
    // A placement named in part refuses the ledger.
    let mut d = Draft::new(&k::SCHEMA);
    d.set(k::KIND, "PLACED")
        .set(k::TX_HASH, Value::B256([1; 32]))
        .set(k::PROVENANCE_TOOL, "test")
        .set(k::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
        .set(k::INTENT, "BUY")
        .set(k::COMPONENT, "BSC_EXEC")
        .set(k::TOKEN, Value::Address([0xA; 20]))
        .set(k::NONCE, 1u64)
        .set(k::GRANT_GENERATION, 7u64)
        .set(k::PLACEMENT, "LATE");
    fs::write(
        dir.join("live-ledger").join(format!("{:020}.dbrec", 5)),
        d.encode().unwrap(),
    )
    .unwrap();
    assert!(ledger.facts().unwrap_err().contains("placement"));
    // A PLACED fact without its placement, or a placement in another
    // fact, refuses the ledger too (review L9).
    for (kind, placed) in [("PLACED", false), ("INCLUDED", true)] {
        let mut d = Draft::new(&k::SCHEMA);
        d.set(k::KIND, kind)
            .set(k::TX_HASH, Value::B256([1; 32]))
            .set(k::PROVENANCE_TOOL, "test")
            .set(k::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(k::INTENT, "BUY")
            .set(k::COMPONENT, "BSC_EXEC")
            .set(k::TOKEN, Value::Address([0xA; 20]))
            .set(k::NONCE, 1u64)
            .set(k::GRANT_GENERATION, 7u64);
        if placed {
            d.set(k::PLACEMENT, "FIRST")
                .set(k::OWN_INDEX, 1u64)
                .set(k::BUYS_BEFORE, 0u64);
        }
        fs::write(
            dir.join("live-ledger").join(format!("{:020}.dbrec", 5)),
            d.encode().unwrap(),
        )
        .unwrap();
        let read = Ledger::open(&dir).facts();
        assert!(
            read.unwrap_err().contains("exactly in a PLACED fact"),
            "{kind}"
        );
    }
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-046: a handle decodes each published entry once; a later read
/// decodes only the new ones (published entries never change: one
/// rewritten after it was read is not read again). When the entries on
/// disk are not the ones it read followed by new ones (one went, or
/// appeared before them), it reads all of them again, and an entry
/// that does not decode refuses the ledger as before.
#[test]
fn the_ledger_decodes_only_entries_it_has_not_read() {
    let dir = std::env::temp_dir().join(format!("bsc-live-incremental-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let ledger = Ledger::open(&dir);
    let facts: Vec<Fact> = (1..=3u8)
        .map(|n| released(n, LiveIntentKind::Buy, n, 1_000, 1_300))
        .collect();
    ledger.append(&facts[0], 1).unwrap();
    ledger.append(&facts[1], 1).unwrap();
    assert_eq!(ledger.facts().unwrap(), facts[..2]);
    let entry = |seq: u64| dir.join("live-ledger").join(format!("{seq:020}.dbrec"));
    fs::write(entry(0), b"not a record").unwrap();
    ledger.append(&facts[2], 1).unwrap();
    assert_eq!(ledger.facts().unwrap(), facts);
    // A clone shares what was read; a new handle reads it all.
    assert_eq!(ledger.clone().facts().unwrap(), facts);
    assert!(Ledger::open(&dir).facts().is_err());
    // An entry went: all is read again, the damaged one refuses.
    fs::remove_file(entry(1)).unwrap();
    assert!(ledger.facts().is_err());
    fs::remove_dir_all(dir).unwrap();
}

fn included(of: &Fact, block: u8, gas: u64, tokens: u64, eth: u64) -> Fact {
    Fact {
        block: Some((u64::from(block), [block; 32], 1_000 * u64::from(block))),
        gas_wei: Some(U256::from(gas)),
        tokens: (tokens > 0).then(|| U256::from(tokens)),
        eth_wei: (eth > 0).then(|| U256::from(eth)),
        ..Fact::about(of, FactKind::Included)
    }
}

/// Plan 28.10.: the facts, written in order and read back after a
/// restart, fold to the same positions: a round trip's PnL is proceeds
/// minus input minus every gas; an orphaned inclusion is unresolved
/// again; a reverted BUY is a failed position whose gas is lost; an
/// UNKNOWN stays visible; an undecodable entry refuses the ledger.
#[test]
fn the_ledger_folds_to_positions_and_net_result() {
    let dir = std::env::temp_dir().join(format!("bsc-live-ledger-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let ledger = Ledger::open(&dir);
    let buy = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
    let approve = released(2, LiveIntentKind::Approve, 0xA, 0, 100);
    let sell = released(3, LiveIntentKind::Sell, 0xA, 0, 150);
    let failed = released(4, LiveIntentKind::Buy, 0xB, 1_000, 1_300);
    let unknown = Fact {
        kind: FactKind::Unknown,
        ..released(5, LiveIntentKind::Buy, 0xC, 1_000, 1_300)
    };
    let facts = [
        buy.clone(),
        included(&buy, 10, 250, 5_000, 950),
        // A reorg takes the BUY's block; it is included anew.
        Fact {
            block: Some((10, [10; 32], 10_000)),
            ..Fact::about(&buy, FactKind::Orphaned)
        },
        included(&buy, 11, 260, 5_000, 950),
        approve.clone(),
        included(&approve, 12, 40, 0, 0),
        sell.clone(),
        included(&sell, 13, 90, 5_000, 1_500),
        failed.clone(),
        Fact {
            gas_wei: Some(U256::from(300)),
            block: Some((14, [14; 32], 14_000)),
            ..Fact::about(&failed, FactKind::Reverted)
        },
        unknown.clone(),
    ];
    for f in &facts {
        ledger.append(f, 1).unwrap();
    }
    // Read back (a restart): the same facts, the same fold.
    let read = Ledger::open(&dir).facts().unwrap();
    assert_eq!(read, facts);
    let book = fold(&read);
    let a = book
        .positions
        .iter()
        .find(|p| p.token == [0xA; 20])
        .unwrap();
    assert_eq!(a.state, PositionState::Closed);
    assert_eq!(
        (a.tokens, a.input_wei),
        (U256::from(5_000), U256::from(1_000))
    );
    assert_eq!(a.gas_wei, U256::from(260 + 40 + 90));
    assert_eq!(a.pnl_wei, Some(1_500 - 1_000 - 390));
    assert_eq!(a.opened_ms, Some(11_000));
    let b = book
        .positions
        .iter()
        .find(|p| p.token == [0xB; 20])
        .unwrap();
    assert_eq!((b.state, b.pnl_wei), (PositionState::Failed, Some(-300)));
    let c = book
        .positions
        .iter()
        .find(|p| p.token == [0xC; 20])
        .unwrap();
    assert_eq!(c.state, PositionState::Pending);
    assert_eq!(book.unresolved().count(), 1);
    assert_eq!(book.realized_pnl_wei, 110 - 300);
    assert_eq!(book.gas_wei, U256::from(260 + 40 + 90 + 300));
    // Open after the BUY only; EXITING once its SELL is released.
    let open = fold(&read[..4]);
    assert_eq!(open.positions[0].state, PositionState::Open);
    let exiting = fold(&read[..7]);
    assert_eq!(exiting.positions[0].state, PositionState::Exiting);
    // A cost the receipt did not state counts at its most.
    let unstated = fold(&[buy.clone(), Fact::about(&buy, FactKind::Included)]);
    assert_eq!(unstated.gas_wei, U256::from(300));
    // An entry that does not decode refuses the ledger.
    fs::write(dir.join("live-ledger/00000000000000000099.dbrec"), b"x").unwrap();
    assert!(Ledger::open(&dir).facts().is_err());
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-045: an entry's write ends in the middle (an I/O error, or the
/// process ends and leaves its temporary file). It is no entry: the
/// ledger reads as before, the next fact takes the next entry.
#[test]
fn an_entry_cut_off_in_its_write_is_no_entry() {
    let dir = std::env::temp_dir().join(format!("bsc-live-cut-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let ledger = Ledger::open(&dir);
    let buy = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
    let lost = released(2, LiveIntentKind::Buy, 0xB, 1_000, 1_300);
    let next = released(3, LiveIntentKind::Buy, 0xC, 1_000, 1_300);
    ledger.append(&buy, 1).unwrap();
    let crashed = Ledger {
        cut: Some(9),
        ..Ledger::open(&dir)
    };
    assert!(crashed.append(&lost, 1).is_err());
    // What a process that ended there leaves.
    fs::write(dir.join("live-ledger/pending-1-0.tmp"), b"torn").unwrap();
    let files = || fs::read_dir(dir.join("live-ledger")).unwrap().count();
    let left = files();
    assert_eq!(ledger.facts().unwrap(), vec![buy.clone()]);
    assert_eq!(ledger.append(&next, 1).unwrap(), 1);
    assert_eq!(ledger.facts().unwrap(), vec![buy, next]);
    assert_eq!(files(), left + 1);
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-047: a token bought again after its exit is a lot of its own
/// (the BUY's hash): each lot keeps its tokens, input, gas and result;
/// the exit steps count in their lot; the net result is the closed
/// lots'. A BUY another transaction replaced never happened: no lot.
/// A reorg and a restart fold the same.
#[test]
fn a_token_bought_again_is_a_lot_of_its_own() {
    let dir = std::env::temp_dir().join(format!("bsc-live-lots-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let ledger = Ledger::open(&dir);
    let buy1 = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
    let approve1 = released(2, LiveIntentKind::Approve, 0xA, 0, 100);
    let sell1 = released(3, LiveIntentKind::Sell, 0xA, 0, 150);
    // A BUY of token B is lost; BUY 2 of A takes its nonce.
    let lost = released(4, LiveIntentKind::Buy, 0xB, 3_000, 3_300);
    let buy2 = released(5, LiveIntentKind::Buy, 0xA, 2_000, 2_300);
    let approve2 = released(6, LiveIntentKind::Approve, 0xA, 0, 100);
    let facts = [
        buy1.clone(),
        included(&buy1, 10, 250, 5_000, 1_000),
        approve1.clone(),
        included(&approve1, 11, 40, 0, 0),
        sell1.clone(),
        included(&sell1, 12, 90, 5_000, 1_500),
        lost.clone(),
        buy2.clone(),
        Fact {
            block: Some((13, [13; 32], 13_000)),
            by_tx: Some(buy2.tx_hash),
            ..Fact::about(&lost, FactKind::Replaced)
        },
        included(&buy2, 13, 260, 7_000, 2_000),
        // A reorg takes BUY 2's block; it is included anew.
        Fact {
            block: Some((13, [13; 32], 13_000)),
            ..Fact::about(&buy2, FactKind::Orphaned)
        },
        included(&buy2, 14, 270, 7_000, 2_000),
        approve2.clone(),
        included(&approve2, 15, 45, 0, 0),
    ];
    for f in &facts {
        ledger.append(f, 1).unwrap();
    }
    let book = fold(&Ledger::open(&dir).facts().unwrap());
    assert_eq!(book.positions.len(), 2, "{:?}", book.positions);
    let (first, second) = (&book.positions[0], &book.positions[1]);
    assert_eq!((first.lot, second.lot), (buy1.tx_hash, buy2.tx_hash));
    assert_eq!(first.state, PositionState::Closed);
    assert_eq!(
        (first.tokens, first.input_wei, first.gas_wei),
        (
            U256::from(5_000),
            U256::from(1_000),
            U256::from(250 + 40 + 90)
        )
    );
    assert_eq!(first.pnl_wei, Some(1_500 - 1_000 - 380));
    assert_eq!(second.state, PositionState::Open);
    assert_eq!(
        (second.tokens, second.input_wei, second.gas_wei),
        (U256::from(7_000), U256::from(2_000), U256::from(270 + 45))
    );
    assert_eq!((second.opened_ms, second.pnl_wei), (Some(14_000), None));
    // Each lot knows its own approval.
    assert!(first.approved && second.approved);
    let without = fold(&Ledger::open(&dir).facts().unwrap()[..12]);
    assert!(!without.positions[1].approved);
    assert_eq!(book.lot_of(&approve2.tx_hash).unwrap().lot, buy2.tx_hash);
    assert_eq!(book.lot_of(&approve1.tx_hash).unwrap().lot, buy1.tx_hash);
    assert!(book.lot_of(&lost.tx_hash).is_none());
    assert_eq!(book.realized_pnl_wei, 1_500 - 1_000 - 380);
    assert_eq!(book.gas_wei, U256::from(250 + 40 + 90 + 270 + 45));
    assert_eq!(book.count(PositionState::Pending), 0);
    assert!(!first.proceeds_unknown);
    // R5: lot 2's SELL included without the Portal's trade event (DEX
    // routed): closed, its proceeds unknown, counted as 0 (the worst
    // case: the loss limits see it).
    let sell2 = released(7, LiveIntentKind::Sell, 0xA, 0, 150);
    for f in [sell2.clone(), included(&sell2, 16, 80, 7_000, 0)] {
        ledger.append(&f, 1).unwrap();
    }
    let book = fold(&Ledger::open(&dir).facts().unwrap());
    let second = &book.positions[1];
    assert_eq!(second.state, PositionState::Closed);
    assert!(second.proceeds_unknown);
    assert_eq!(
        (second.proceeds_wei, second.pnl_wei),
        (U256::ZERO, Some(-2_000 - (270 + 45 + 80)))
    );
    assert_eq!(
        book.realized_pnl_wei,
        1_500 - 1_000 - 380 - 2_000 - (270 + 45 + 80)
    );
    fs::remove_dir_all(dir).unwrap();
}

fn admitted(n: u8, token: Option<[u8; 20]>) -> Admitted {
    Admitted {
        tx_hash: [n; 32],
        kind: LiveIntentKind::Buy,
        component: ComponentId::ExecC3,
        grant_generation: 7,
        input_wei: 1_000,
        max_wei: 1_300,
        token,
        nonce: token.map(|_| u64::from(n)),
        max_block: token.map(|_| 100),
        origin: token.map(|_| crate::sources::tests::ORIGIN),
        lease: false,
        venue: None,
    }
}

/// BSC3-024: an admitted transaction the ledger has no fact of (the
/// process ended between its admission and its outcome) becomes an
/// UNKNOWN fact, once; one without token and nonce cannot and is named.
/// A withdrawn transaction (no byte left) is resolved, with no position
/// and no cost.
#[test]
fn admitted_transactions_without_a_fact_are_recovered_as_unknown() {
    let dir = std::env::temp_dir().join(format!("bsc-live-recover-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let ledger = Ledger::open(&dir);
    let known = Fact::admitted(&admitted(1, Some([0xA; 20])), FactKind::Released).unwrap();
    let withdrawn = Fact::admitted(&admitted(2, Some([0xB; 20])), FactKind::Withdrawn).unwrap();
    ledger.append(&known, 1).unwrap();
    ledger.append(&withdrawn, 1).unwrap();
    let all = [
        admitted(1, Some([0xA; 20])),
        admitted(2, Some([0xB; 20])),
        admitted(3, Some([0xC; 20])),
        admitted(4, None),
    ];
    let r = ledger.recover(&all, 2).unwrap();
    assert_eq!(r.unknown, 1);
    assert_eq!(r.unrecoverable, vec![[4; 32]]);
    let facts = ledger.facts().unwrap();
    assert_eq!(facts.len(), 3);
    assert_eq!(
        facts[2],
        Fact::admitted(&admitted(3, Some([0xC; 20])), FactKind::Unknown).unwrap()
    );
    // Once only.
    assert_eq!(ledger.recover(&all, 3).unwrap().unknown, 0);
    let book = fold(&facts);
    let unresolved: Vec<[u8; 32]> = book.unresolved().map(|t| t.released.tx_hash).collect();
    assert_eq!(unresolved, vec![[1; 32], [3; 32]]);
    assert_eq!(book.tx(&[2; 32]).unwrap().state, FactKind::Withdrawn);
    assert!(book.positions.iter().all(|p| p.token != [0xB; 20]));
    assert_eq!(book.gas_wei, U256::ZERO);
    fs::remove_dir_all(dir).unwrap();
}

// ---- REV.1.0 R3 ----

fn r3_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bsc-live-r3-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn operator(request_id: &str, at_ms: u64) -> Option<Operator> {
    Some(Operator {
        request_id: request_id.into(),
        decided_by: "operator".into(),
        evidence: None,
        at_ms,
    })
}

/// The operator's `kind` of the lot `of`, as `bsc_runtime live lot`
/// writes it, at `at_ms`.
fn decision(of: &Fact, kind: FactKind, request_id: &str, at_ms: u64) -> Fact {
    Fact {
        operator: operator(request_id, at_ms),
        ..Fact::about(of, kind)
    }
}

fn reverted(of: &Fact, block: u8, gas: u64) -> Fact {
    Fact {
        block: Some((u64::from(block), [block; 32], 1_000 * u64::from(block))),
        gas_wei: Some(U256::from(gas)),
        ..Fact::about(of, FactKind::Reverted)
    }
}

/// R3: a v4 fact keeps its attempt, its operator (with the entry's
/// time) and its source; a v3 entry reads without them; an operator on
/// a fact not the operator's, evidence outside an adoption (or an
/// adoption without it), a source on no chain fact and (R3b) a source
/// other than RECEIPT refuse the ledger.
#[test]
fn a_v4_fact_keeps_its_attempt_operator_and_source_and_a_v3_entry_still_reads() {
    let dir = r3_dir("v4");
    let ledger = Ledger::open(&dir);
    let buy = Fact {
        attempt: Some([9; 32]),
        ..released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300)
    };
    let found = Fact {
        source: Some(FactSource::Receipt),
        ..included(&buy, 5, 100, 7, 1_000)
    };
    let resumed = decision(&buy, FactKind::Resumed, "req-1", 40);
    ledger.append(&buy, 10).unwrap();
    ledger.append(&found, 20).unwrap();
    ledger.append(&resumed, 40).unwrap();
    assert_eq!(ledger.facts().unwrap(), vec![buy.clone(), found, resumed]);
    let raw = |seq: u64,
               schema: &'static darkbot_bsc_record::Schema,
               kind: &str,
               more: &dyn Fn(&mut Draft)| {
        let mut d = Draft::new(schema);
        d.set(k::KIND, kind)
            .set(k::TX_HASH, Value::B256([1; 32]))
            .set(k::PROVENANCE_TOOL, "test")
            .set(k::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(k::INTENT, "BUY")
            .set(k::COMPONENT, "BSC_EXEC")
            .set(k::TOKEN, Value::Address([0xA; 20]))
            .set(k::NONCE, 1u64)
            .set(k::GRANT_GENERATION, 7u64);
        more(&mut d);
        let path = dir.join("live-ledger").join(format!("{seq:020}.dbrec"));
        fs::write(&path, d.encode().unwrap()).unwrap();
        path
    };
    // A fresh handle each time: a handle keeps what it read (BSC3-046).
    let v3 = raw(3, &k::V3, "RELEASED", &|_| {});
    let facts = Ledger::open(&dir).facts().unwrap();
    assert_eq!(
        (facts[3].attempt, &facts[3].operator, facts[3].source),
        (None, &None, None)
    );
    fs::remove_file(v3).unwrap();
    // SC4 d review H1: a v4 fact (before the venue) reads as Flap's.
    let v4 = raw(3, &k::V4, "RELEASED", &|d| {
        d.set(k::ATTEMPT, Value::B256([4; 32]));
    });
    let facts = Ledger::open(&dir).facts().unwrap();
    assert_eq!((facts[3].attempt, facts[3].venue), (Some([4; 32]), None));
    fs::remove_file(v4).unwrap();
    type Case<'a> = (&'a str, &'a dyn Fn(&mut Draft), &'a str);
    let refused: [Case<'_>; 6] = [
        (
            "RELEASED",
            &|d| {
                d.set(k::REQUEST_ID, "r").set(k::DECIDED_BY, "o");
            },
            "operator's fact",
        ),
        ("RESUMED", &|_| {}, "operator's fact"),
        (
            "RESUMED",
            &|d| {
                d.set(k::REQUEST_ID, "r")
                    .set(k::DECIDED_BY, "o")
                    .set(k::EVIDENCE_SHA256, Value::B256([3; 32]));
            },
            "operator's fact",
        ),
        (
            "ADOPTED",
            &|d| {
                d.set(k::REQUEST_ID, "r").set(k::DECIDED_BY, "o");
            },
            "operator's fact",
        ),
        (
            "RELEASED",
            &|d| {
                d.set(k::SOURCE, "RECEIPT");
            },
            "chain fact",
        ),
        (
            "INCLUDED",
            &|d| {
                d.set(k::SOURCE, "NONCE")
                    .set(k::BLOCK_NUMBER, 5u64)
                    .set(k::BLOCK_HASH, Value::B256([5; 32]))
                    .set(k::BLOCK_MS, 7u64);
            },
            "source",
        ),
    ];
    for (kind, more, why) in refused {
        let path = raw(3, &k::SCHEMA, kind, more);
        let e = Ledger::open(&dir).facts().unwrap_err();
        assert!(e.contains(why), "{kind}: {e}");
        fs::remove_file(path).unwrap();
    }
    // A request named without who decided it is no operator at all.
    let path = raw(3, &k::SCHEMA, "RESUMED", &|d| {
        d.set(k::REQUEST_ID, "r");
    });
    assert!(
        Ledger::open(&dir)
            .facts()
            .unwrap_err()
            .contains("whole or not at all")
    );
    fs::remove_file(path).unwrap();
    assert_eq!(Ledger::open(&dir).facts().unwrap().len(), 3);
    fs::remove_dir_all(dir).unwrap();
}

/// REV.1.0 R3b: a nonce burn is requested once per request, only with
/// the switch ON, only of an unresolved transaction of the buyer whose
/// bytes may still land (its window unknown, or its block left the
/// chain); its
/// fact names the operator and folds into the transaction. A burn holds
/// no lot: its gas counts in the ledger's gas and its realized result.
#[test]
fn a_burn_is_requested_once_of_a_transaction_whose_bytes_may_still_land() {
    let dir = r3_dir("burn");
    let ledger = Ledger::open(&dir);
    let unknown = Fact {
        max_block: None,
        ..released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300)
    };
    let bounded = released(2, LiveIntentKind::Buy, 0xB, 1_000, 1_300);
    let orphaned = released(3, LiveIntentKind::Sell, 0xC, 0, 300);
    let inclusion = included(&orphaned, 5, 0, 7, 0);
    for f in [&unknown, &bounded, &orphaned, &inclusion] {
        ledger.append(f, 10).unwrap();
    }
    ledger
        .append(
            &Fact {
                block: inclusion.block,
                ..Fact::about(&orphaned, FactKind::Orphaned)
            },
            11,
        )
        .unwrap();
    let me = Some(crate::sources::tests::ORIGIN.sender);
    assert_eq!(
        ledger.request_burn(
            unknown.tx_hash,
            me,
            "b-1",
            "operator",
            false,
            ledger.version().unwrap(),
            20
        ),
        Err(LIVE_NONCE_BURN_OFF.into())
    );
    for tx in [bounded.tx_hash, [0x99; 32]] {
        let e = ledger
            .request_burn(
                tx,
                me,
                "b-x",
                "operator",
                true,
                ledger.version().unwrap(),
                20,
            )
            .unwrap_err();
        assert!(e.starts_with(LIVE_BURN_REFUSED), "{e}");
    }
    // Review LOW-2: another sender's transaction is not the buyer's to
    // burn; without a configured buyer nothing is.
    let e = ledger
        .request_burn(
            unknown.tx_hash,
            Some([0xEE; 20]),
            "b-o",
            "operator",
            true,
            ledger.version().unwrap(),
            20,
        )
        .unwrap_err();
    assert!(e.starts_with(LIVE_BURN_REFUSED), "{e}");
    assert_eq!(
        ledger.request_burn(
            unknown.tx_hash,
            None,
            "b-n",
            "operator",
            true,
            ledger.version().unwrap(),
            20
        ),
        Err("BSC_LIVE_BUYER is not configured".into())
    );
    let first = ledger
        .request_burn(
            unknown.tx_hash,
            me,
            "b-1",
            "operator",
            true,
            ledger.version().unwrap(),
            20,
        )
        .unwrap();
    assert!(first.is_some());
    assert_eq!(
        ledger.request_burn(
            unknown.tx_hash,
            me,
            "b-1",
            "operator",
            true,
            ledger.version().unwrap(),
            21
        ),
        Ok(None)
    );
    assert!(
        ledger
            .request_burn(
                orphaned.tx_hash,
                me,
                "b-1",
                "operator",
                true,
                ledger.version().unwrap(),
                21
            )
            .unwrap_err()
            .starts_with(LIVE_REQUEST_REUSED)
    );
    assert!(
        ledger
            .request_burn(
                orphaned.tx_hash,
                me,
                "b-2",
                "operator",
                true,
                ledger.version().unwrap(),
                22
            )
            .unwrap()
            .is_some(),
        "a transaction once included in a block that left the chain"
    );
    let facts = Ledger::open(&dir).facts().unwrap();
    let fact = facts
        .iter()
        .find(|f| f.kind == FactKind::BurnRequested && f.tx_hash == unknown.tx_hash)
        .unwrap();
    assert_eq!(
        fact.operator
            .as_ref()
            .map(|o| (o.request_id.as_str(), o.decided_by.as_str())),
        Some(("b-1", "operator"))
    );
    let book = fold(&facts);
    assert!(book.tx(&unknown.tx_hash).unwrap().decided.burn_requested);
    assert!(!book.tx(&bounded.tx_hash).unwrap().decided.burn_requested);
    // The burn itself: no lot, its gas in the gas and the result.
    let burn = Fact {
        token: [0; 20],
        ..released(4, LiveIntentKind::Burn, 0, 0, 21)
    };
    let lots = book.positions.len();
    let burnt = fold(&[facts, vec![burn.clone(), included(&burn, 6, 21, 0, 0)]].concat());
    assert_eq!(burnt.positions.len(), lots);
    assert_eq!(burnt.tx(&burn.tx_hash).unwrap().lot, None);
    assert_eq!(burnt.gas_wei, book.gas_wei + U256::from(21u64));
    assert_eq!(burnt.realized_pnl_wei, book.realized_pnl_wei - 21);
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-068: an attempt is returned only when every BUY fact of it (its
/// transaction's and its lease's) is WITHDRAWN: one released, unknown,
/// or included later keeps its slots; an exit is no attempt; a v3 BUY
/// without its attempt is its own (a leased one names no slot).
#[test]
fn an_attempt_whose_every_fact_is_withdrawn_is_returned_and_no_other() {
    let withdrawn = |n: u8, attempt: Option<[u8; 32]>, token: u8| Fact {
        kind: FactKind::Withdrawn,
        component: ComponentId::ExecC3,
        attempt,
        ..released(n, LiveIntentKind::Buy, token, 1_000, 1_300)
    };
    let (a, b, c) = ([0xA1; 32], [0xB1; 32], [0xC1; 32]);
    let late = withdrawn(7, Some(c), 0xC);
    let public = withdrawn(11, Some([0xE1; 32]), 0xF);
    let facts = vec![
        // A: the BUY and its lease's own fact, both withdrawn.
        withdrawn(1, Some(a), 0xA),
        withdrawn(2, Some(a), 0),
        // B: the BUY released, its lease's fact withdrawn.
        Fact {
            attempt: Some(b),
            ..released(3, LiveIntentKind::Buy, 0xB, 1_000, 1_300)
        },
        withdrawn(4, Some(b), 0),
        // A v3 BUY of its own, withdrawn.
        withdrawn(5, None, 0xD),
        // An admission recovered as UNKNOWN.
        Fact {
            kind: FactKind::Unknown,
            attempt: Some([6; 32]),
            ..released(6, LiveIntentKind::Buy, 0xE, 1_000, 1_300)
        },
        // C: withdrawn, then included (never: it would keep its slots).
        late.clone(),
        included(&late, 9, 100, 7, 1_000),
        // Re-check RC2: E withdrawn, included, its block orphaned: its
        // bytes were public, it keeps its slots.
        public.clone(),
        included(&public, 12, 100, 7, 1_000),
        Fact::about(&public, FactKind::Orphaned),
        // A withdrawn SELL.
        Fact {
            kind: FactKind::Withdrawn,
            ..released(8, LiveIntentKind::Sell, 0xA, 0, 300)
        },
        // Review F1: a lease's fact alone (its BUY's quarantined).
        withdrawn(10, Some([0xD1; 32]), 0),
    ];
    let returned = fold(&facts).withdrawn_attempts();
    assert_eq!(returned, vec![a, [5; 32]]);
}

/// R3b review M2/L1: an operator's new request expects the state it
/// was decided on, checked under the operator's lock after its own
/// replay: a lot's decision and a nonce burn the ledger's version (any
/// fact since refuses it, LIVE_REQUEST_STALE, nothing written), a
/// quarantine the owner generation (read under the lock); the same
/// request again is its replay whatever moved since (a quarantine's
/// answers its recorded generation).
#[test]
fn an_operators_request_expects_the_state_it_was_decided_on() {
    let dir = r3_dir("cas");
    let ledger = Ledger::open(&dir);
    let buy = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
    ledger.append(&buy, 1).unwrap();
    ledger.append(&included(&buy, 5, 100, 7, 1_000), 2).unwrap();
    let unknown = Fact {
        max_block: None,
        ..released(2, LiveIntentKind::Buy, 0xB, 1_000, 1_300)
    };
    ledger.append(&unknown, 3).unwrap();
    let (lot, seen) = ([1; 32], ledger.version().unwrap());
    assert_eq!(seen, 3);
    // A fact since the operator saw the ledger: refused, nothing written.
    ledger
        .append(&released(3, LiveIntentKind::Approve, 0xA, 0, 300), 4)
        .unwrap();
    let stale = ledger
        .decide(lot, LotDecision::Resume, "r-1", "operator", false, seen, 10)
        .unwrap_err();
    assert_eq!(
        stale,
        format!("{LIVE_REQUEST_STALE}: the ledger's version is 4, not 3")
    );
    let me = Some(crate::sources::tests::ORIGIN.sender);
    let stale = ledger
        .request_burn(unknown.tx_hash, me, "b-1", "operator", true, seen, 10)
        .unwrap_err();
    assert!(stale.starts_with(LIVE_REQUEST_STALE), "{stale}");
    assert_eq!(ledger.facts().unwrap().len(), 4);
    // At the current version; then the same request again is its
    // replay, though the ledger moved.
    assert_eq!(
        ledger.decide(lot, LotDecision::Resume, "r-1", "operator", false, 4, 11),
        Ok(Some(4))
    );
    assert_eq!(
        ledger.decide(lot, LotDecision::Resume, "r-1", "operator", false, 4, 12),
        Ok(None)
    );
    assert_eq!(
        ledger.request_burn(unknown.tx_hash, me, "b-1", "operator", true, 5, 13),
        Ok(Some(5))
    );
    assert_eq!(
        ledger.request_burn(unknown.tx_hash, me, "b-1", "operator", true, 0, 14),
        Ok(None)
    );
    // A quarantine at another owner generation: refused, no record;
    // the generation read under the lock is recorded and answered
    // again on its replay.
    fs::write(
        dir.join("live-ledger").join(format!("{:020}.dbrec", 6)),
        b"torn",
    )
    .unwrap();
    let stale = ledger
        .quarantine(6, "q-6", "operator", 7, || Ok(8), 20)
        .unwrap_err();
    assert_eq!(
        stale,
        format!("{LIVE_REQUEST_STALE}: the owner generation is 8, not 7")
    );
    assert_eq!(ledger.quarantined(), Ok(None));
    assert_eq!(
        ledger.quarantine(6, "q-6", "operator", 8, || Ok(8), 21),
        Ok(8)
    );
    assert_eq!(
        ledger.quarantine(6, "q-6", "operator", 9, || Ok(9), 22),
        Ok(8)
    );
    assert_eq!(ledger.quarantined(), Ok(Some(8)));
    fs::remove_dir_all(dir).unwrap();
}

/// R3 (`bsc_runtime live lot`): each decision of an open lot is written
/// once per request (the same again changes nothing, another under its
/// ID is refused); write-off and exit-now refuse while a step of the
/// exit is out, exit-now while the operator exit is OFF; a lot not open
/// and a request without a safe ID or a decider are refused.
#[test]
fn the_operator_decides_a_lot_once_per_request() {
    let dir = r3_dir("decide");
    let ledger = Ledger::open(&dir);
    let buy = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
    ledger.append(&buy, 1).unwrap();
    ledger.append(&included(&buy, 5, 100, 7, 1_000), 2).unwrap();
    let lot = [1; 32];
    let seq = ledger
        .decide(
            lot,
            LotDecision::Resume,
            "req-1",
            "operator",
            false,
            ledger.version().unwrap(),
            10,
        )
        .unwrap();
    assert_eq!(seq, Some(2));
    assert_eq!(
        ledger.decide(
            lot,
            LotDecision::Resume,
            "req-1",
            "operator",
            false,
            ledger.version().unwrap(),
            11
        ),
        Ok(None)
    );
    let reused = ledger
        .decide(
            lot,
            LotDecision::WriteOff,
            "req-1",
            "operator",
            false,
            ledger.version().unwrap(),
            12,
        )
        .unwrap_err();
    assert!(reused.starts_with(LIVE_REQUEST_REUSED), "{reused}");
    assert_eq!(
        ledger.decide(
            lot,
            LotDecision::ExitNow,
            "req-2",
            "operator",
            false,
            ledger.version().unwrap(),
            13
        ),
        Err(LIVE_OPERATOR_EXIT_OFF.to_string())
    );
    assert_eq!(
        ledger
            .decide(
                lot,
                LotDecision::ExitNow,
                "req-2",
                "operator",
                true,
                ledger.version().unwrap(),
                14
            )
            .unwrap(),
        Some(3)
    );
    let facts = ledger.facts().unwrap();
    assert_eq!(facts[2].kind, FactKind::Resumed);
    assert_eq!(facts[2].operator, operator("req-1", 10));
    assert_eq!(facts[3].kind, FactKind::ExitRequested);
    assert_eq!((facts[3].tx_hash, facts[3].token), (lot, [0xA; 20]));
    // Its APPROVE is out (the lot still open): a write-off waits for it.
    ledger
        .append(&released(5, LiveIntentKind::Approve, 0xA, 0, 300), 15)
        .unwrap();
    let e = ledger
        .decide(
            lot,
            LotDecision::WriteOff,
            "req-8",
            "operator",
            false,
            ledger.version().unwrap(),
            16,
        )
        .unwrap_err();
    assert!(e.starts_with(LIVE_LOT_EXIT_OUT), "{e}");
    // Its SELL is out: write-off and exit-now wait for it, resume not.
    ledger
        .append(&released(2, LiveIntentKind::Sell, 0xA, 0, 300), 20)
        .unwrap();
    for (what, id) in [
        (LotDecision::WriteOff, "req-3"),
        (LotDecision::ExitNow, "req-4"),
    ] {
        let e = ledger
            .decide(
                lot,
                what,
                id,
                "operator",
                true,
                ledger.version().unwrap(),
                21,
            )
            .unwrap_err();
        assert!(e.starts_with(LIVE_LOT_EXIT_OUT), "{e}");
    }
    assert!(
        ledger
            .decide(
                lot,
                LotDecision::Resume,
                "req-5",
                "operator",
                false,
                ledger.version().unwrap(),
                22
            )
            .unwrap()
            .is_some()
    );
    for not_open in [[9; 32], [2; 32]] {
        let e = ledger
            .decide(
                not_open,
                LotDecision::Resume,
                "req-6",
                "operator",
                false,
                ledger.version().unwrap(),
                23,
            )
            .unwrap_err();
        assert!(e.starts_with(LIVE_LOT_NOT_OPEN), "{e}");
    }
    assert!(
        ledger
            .decide(
                lot,
                LotDecision::Resume,
                "bad id!",
                "operator",
                false,
                ledger.version().unwrap(),
                24
            )
            .is_err()
    );
    assert!(
        ledger
            .decide(
                lot,
                LotDecision::Resume,
                "req-7",
                " ",
                false,
                ledger.version().unwrap(),
                24
            )
            .is_err()
    );
    assert_eq!(ledger.facts().unwrap().len(), 7);
    fs::remove_dir_all(dir).unwrap();
}

/// R3: a lot written off with nothing out is lost on the day of its
/// fact (input and gas, no exit, no open lot); one whose SELL is out
/// is decided by that SELL (a race with the decision): included, it
/// closes with its proceeds.
#[test]
fn a_written_off_lot_is_lost_on_its_day_and_a_sell_out_decides_first() {
    let dir = r3_dir("writeoff");
    let ledger = Ledger::open(&dir);
    let a = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
    let b = released(2, LiveIntentKind::Buy, 0xB, 1_000, 1_300);
    let sell = released(3, LiveIntentKind::Sell, 0xB, 0, 300);
    for (f, at) in [
        (a.clone(), 1),
        (included(&a, 5, 100, 7, 1_000), 2),
        (b.clone(), 3),
        (included(&b, 6, 100, 9, 1_000), 4),
        (sell.clone(), 5),
        (decision(&a, FactKind::WrittenOff, "off-a", 50_000), 50_000),
        (decision(&b, FactKind::WrittenOff, "off-b", 50_001), 50_001),
    ] {
        ledger.append(&f, at).unwrap();
    }
    let book = fold(&ledger.facts().unwrap());
    let lot_a = &book.positions[0];
    assert_eq!(lot_a.state, PositionState::WrittenOff);
    assert_eq!(
        (lot_a.pnl_wei, lot_a.closed_ms),
        (Some(-1_100), Some(50_000))
    );
    assert!(book.open_lot(&[0xA; 20]).is_none());
    assert_eq!(book.positions[1].state, PositionState::Exiting);
    assert_eq!(book.realized_pnl_wei, -1_100);
    ledger
        .append(&included(&sell, 8, 50, 9, 2_500), 60_000)
        .unwrap();
    let book = fold(&ledger.facts().unwrap());
    let lot_b = &book.positions[1];
    assert_eq!(lot_b.state, PositionState::Closed);
    assert_eq!(lot_b.pnl_wei, Some(2_500 - 1_000 - 150));
    fs::remove_dir_all(dir).unwrap();
}

/// R3: a RESUMED lot's exit steps reverted before it stop nothing; one
/// reverted after it counts.
#[test]
fn a_resumed_lot_excuses_only_the_reverts_before_it() {
    let buy = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
    let approve = released(2, LiveIntentKind::Approve, 0xA, 0, 300);
    let sell = released(3, LiveIntentKind::Sell, 0xA, 0, 300);
    let again = released(4, LiveIntentKind::Sell, 0xA, 0, 300);
    let book = fold(&[
        buy.clone(),
        included(&buy, 5, 100, 7, 1_000),
        approve.clone(),
        reverted(&approve, 6, 50),
        sell.clone(),
        reverted(&sell, 7, 50),
        decision(&buy, FactKind::Resumed, "resume-1", 10),
        again.clone(),
        reverted(&again, 8, 50),
    ]);
    let decided = &book.tx(&[1; 32]).unwrap().decided;
    assert_eq!(decided.resumes, 1);
    assert_eq!(decided.excused, vec![[2; 32], [3; 32]]);
    assert_eq!(book.positions[0].state, PositionState::Open);
}

/// A G0.8 result (v7) of the buyer's G0.7 BUY of `token` and its chain
/// capture: the BUY (nonce 3, value 1 000, gas limit 200 000 at 2 wei)
/// and its launch in block 50, 20 confirmations, `tokens` received;
/// `edit` changes the result last.
fn g08_evidence(buyer: [u8; 20], tokens: u64, edit: &dyn Fn(&mut Draft)) -> (Vec<u8>, Vec<u8>) {
    use darkbot_bsc_journal::{CaptureRecord, JournalWriter, RecordKind};
    use darkbot_bsc_record::schema::g08::{
        chain_block as cb, chain_transaction as ct, result as g,
    };
    let filled = crate::live_qualification::tests::filled;
    let word = |n: u64| Value::U256(U256::from(n).to_be_bytes());
    let mut block = filled(&cb::SCHEMA);
    block
        .set(cb::NUMBER, 50u64)
        .set(cb::HASH, Value::B256([0x50; 32]))
        .set(cb::TIMESTAMP, 1_700u64)
        .set(cb::MILLI_TIMESTAMP, 1_700_250u64);
    let tx = |hash: [u8; 32], from: [u8; 20], nonce: u64| {
        let mut t = filled(&ct::SCHEMA);
        t.set(ct::BLOCK_NUMBER, 50u64)
            .set(ct::BLOCK_HASH, Value::B256([0x50; 32]))
            .set(ct::HASH, Value::B256(hash))
            .set(ct::FROM, Value::Address(from))
            .set(ct::NONCE, nonce)
            .set(ct::VALUE, word(1_000))
            .set(ct::GAS, 200_000u64)
            .set(ct::GAS_PRICE, word(2));
        t
    };
    let mut writer = JournalWriter::new(Vec::new());
    for (n, d) in [
        block,
        tx([0x1A; 32], [0xC1; 20], 0),
        tx([0xB0; 32], buyer, 3),
    ]
    .iter()
    .enumerate()
    {
        writer
            .append(&CaptureRecord {
                sequence: n as u64,
                kind: RecordKind::Canonical,
                timestamp_ns: 1,
                payload: d.encode().unwrap().into_boxed_slice(),
            })
            .unwrap();
    }
    let capture = writer.into_inner();
    let mut journal = filled(&g::chain_journal::SCHEMA);
    journal.set(
        g::chain_journal::SHA256,
        Value::B256(darkbot_bsc_record::sha256(&capture)),
    );
    let mut r = filled(&g::SCHEMA);
    r.set(g::BUY_STATUS, 1u64)
        .set(g::CANONICAL, true)
        .set(g::ACQUISITION_EVIDENCE_COMPLETE, true)
        .set(g::POSITIVE_ATTRIBUTABLE_ACQUISITION, true)
        .set(g::BUYER, Value::Address(buyer))
        .set(g::TOKEN, Value::Address([0xA; 20]))
        .set(g::CONFIRMATIONS_AFTER_LAUNCH_BLOCK, 20u64)
        .set(g::CURRENT_HEAD, 70u64)
        .set(g::BUY_SCOPE_OK, true)
        .set(g::BUY_RECEIPT_TOKEN_NET, word(tokens))
        .set(g::BUY_TX, Value::B256([0xB0; 32]))
        .set(g::LAUNCH_TX, Value::B256([0x1A; 32]))
        .set(g::BUY_BLOCK, 50u64)
        .set(g::BUY_BLOCK_HASH, Value::B256([0x50; 32]))
        .set(g::BUY_VALUE_WEI, word(1_000))
        .set(g::BUY_GAS_COST_WEI, word(150_000))
        .set(g::CHAIN_JOURNAL, journal.into_nested());
    edit(&mut r);
    (r.encode().unwrap(), capture)
}

/// R3 (`bsc_runtime live adopt-g07`): the buyer's G0.7 BUY of a G0.8
/// result, its capture matching, becomes an included lot of no grant
/// (its tokens, input, gas, the launch's creator, the result's hash as
/// evidence), once per request; another buyer, a launch not final, a
/// capture not the result's, no tokens, a failed BUY or a value the
/// capture does not show are refused.
#[test]
fn a_g07_buy_is_adopted_from_its_g08_result_and_capture_once() {
    let dir = r3_dir("adopt");
    let ledger = Ledger::open(&dir);
    let buyer = [0xBB; 20];
    let (result, capture) = g08_evidence(buyer, 7, &|_| {});
    let buy = g07_buy(&result, &capture, buyer, 15).unwrap();
    assert_eq!(
        buy,
        G07Buy {
            tx_hash: [0xB0; 32],
            token: [0xA; 20],
            buyer,
            launcher: [0xC1; 20],
            nonce: 3,
            value_wei: U256::from(1_000u64),
            max_wei: U256::from(1_000u64 + 400_000),
            gas_wei: U256::from(150_000u64),
            tokens: U256::from(7u64),
            block: (50, [0x50; 32], 1_700_250),
            evidence: darkbot_bsc_record::sha256(&result),
        }
    );
    assert_eq!(
        ledger.adopt(&buy, 56, "adopt-1", "operator", 100),
        Ok(Some(0))
    );
    assert_eq!(ledger.adopt(&buy, 56, "adopt-1", "operator", 101), Ok(None));
    let again = ledger
        .adopt(&buy, 56, "adopt-2", "operator", 102)
        .unwrap_err();
    assert!(again.contains("in the ledger already"), "{again}");
    // Another BUY of a token with an open lot would hide that lot.
    let other = G07Buy {
        tx_hash: [0xB1; 32],
        ..buy.clone()
    };
    let hidden = ledger
        .adopt(&other, 56, "adopt-3", "operator", 103)
        .unwrap_err();
    assert!(hidden.contains("open lot already"), "{hidden}");
    let book = fold(&ledger.facts().unwrap());
    let p = &book.positions[0];
    assert_eq!(
        (
            p.state,
            p.tokens,
            p.input_wei,
            p.gas_wei,
            p.grant_generation,
            p.component
        ),
        (
            PositionState::Open,
            U256::from(7u64),
            U256::from(1_000u64),
            U256::from(150_000u64),
            0,
            ComponentId::ExecC3
        )
    );
    assert_eq!(book.tx(&[0xB0; 32]).unwrap().launcher, Some([0xC1; 20]));
    assert!(book.unresolved().next().is_none());
    // Its block left the chain (never beyond finality): not known again.
    let mut orphaned = ledger.facts().unwrap();
    orphaned.push(Fact::about(&orphaned[0], FactKind::Orphaned));
    let book = fold(&orphaned);
    assert_eq!(book.tx(&[0xB0; 32]).unwrap().state, FactKind::Unknown);
    assert_eq!(book.unresolved().count(), 1);
    let refused = |result: &[u8], capture: &[u8], buyer, finality| {
        g07_buy(result, capture, buyer, finality).unwrap_err()
    };
    assert!(refused(&result, &capture, [0xCC; 20], 15).contains("buyer"));
    assert!(refused(&result, &capture, buyer, 21).contains("launch's block is not final"));
    let mut torn = capture.clone();
    torn.push(0);
    assert!(refused(&result, &torn, buyer, 15).contains("not the result's"));
    use darkbot_bsc_record::schema::g08::result as g;
    let word = |n: u64| Value::U256(U256::from(n).to_be_bytes());
    type Case<'a> = (&'a dyn Fn(&mut Draft), &'a str);
    let cases: [Case<'_>; 7] = [
        (
            &|d: &mut Draft| {
                d.set(g::CURRENT_HEAD, 60u64);
            },
            "BUY's block is not final",
        ),
        (
            &|d: &mut Draft| {
                d.set(g::BUY_SCOPE_OK, false);
            },
            "out of scope",
        ),
        (&|_: &mut Draft| {}, "no tokens"),
        (
            &|d: &mut Draft| {
                d.set(g::BUY_STATUS, 0u64);
            },
            "did not succeed",
        ),
        (
            &|d: &mut Draft| {
                d.set(g::BUY_VALUE_WEI, word(999));
            },
            "value",
        ),
        (
            &|d: &mut Draft| {
                d.set(g::POSITIVE_ATTRIBUTABLE_ACQUISITION, false);
            },
            "attributable",
        ),
        (
            &|d: &mut Draft| {
                d.set(g::CANONICAL, false);
            },
            "canonical",
        ),
    ];
    for (edit, why) in cases {
        let tokens = if why == "no tokens" { 0 } else { 7 };
        let (r, c) = g08_evidence(buyer, tokens, edit);
        let e = refused(&r, &c, buyer, 15);
        assert!(
            e.contains(why) && e.starts_with(LIVE_ADOPTION_REFUSED),
            "{why}: {e}"
        );
    }
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-045: a start clears the temporary file a crashed append left,
/// never one a living writer holds; appends go on.
#[test]
fn a_crashed_appends_temporary_file_is_cleared_and_a_living_writers_kept() {
    let dir = r3_dir("pending");
    let ledger = Ledger::open(&dir);
    ledger
        .append(&released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300), 1)
        .unwrap();
    let live = dir.join("live-ledger");
    let dead = live.join("pending-1-0.tmp");
    let held = live.join("pending-2-0.tmp");
    fs::write(&dead, b"cut").unwrap();
    fs::write(&held, b"half").unwrap();
    let writer = fs::File::open(&held).unwrap();
    writer.lock().unwrap();
    assert_eq!(ledger.clear_pending(), Ok(1));
    assert!(!dead.exists() && held.exists());
    drop(writer);
    assert_eq!(ledger.clear_pending(), Ok(1));
    assert!(!held.exists());
    ledger
        .append(&released(2, LiveIntentKind::Buy, 0xB, 1_000, 1_300), 2)
        .unwrap();
    assert_eq!(ledger.facts().unwrap().len(), 2);
    let names: Vec<String> = fs::read_dir(&live)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(names.is_empty(), "{names:?}");
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-045 (`bsc_runtime live ledger check|quarantine`): an entry that
/// does not decode is named by the check and moved out by the
/// operator, once per request (its record first, the entry whole beside
/// it); the ledger then reads; a readable fact is never quarantined; the
/// quarantine's generation is reported (an unreadable record: every
/// one).
#[test]
fn an_unreadable_entry_is_quarantined_once_and_the_ledger_reads_again() {
    let dir = r3_dir("quarantine");
    let ledger = Ledger::open(&dir);
    let buy = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
    ledger.append(&buy, 1).unwrap();
    let bad = dir.join("live-ledger").join(format!("{:020}.dbrec", 1));
    fs::write(&bad, b"torn entry").unwrap();
    assert!(ledger.facts().is_err());
    let (entries, unreadable) = ledger.check().unwrap();
    assert_eq!((entries, unreadable.len(), unreadable[0].0), (2, 1, 1));
    assert_eq!(ledger.quarantined(), Ok(None));
    let e = ledger
        .quarantine(0, "q-0", "operator", 7, || Ok(7), 100)
        .unwrap_err();
    assert!(e.contains("never quarantined"), "{e}");
    assert!(
        ledger
            .quarantine(5, "q-5", "operator", 7, || Ok(7), 100)
            .is_err()
    );
    ledger
        .quarantine(1, "q-1", "operator", 7, || Ok(7), 100)
        .unwrap();
    assert_eq!(ledger.facts().unwrap(), vec![buy]);
    assert_eq!(ledger.check().unwrap(), (1, Vec::new()));
    assert_eq!(ledger.quarantined(), Ok(Some(7)));
    let qdir = dir.join("live-ledger-quarantine");
    assert_eq!(
        fs::read(qdir.join(format!("{:020}.entry", 1))).unwrap(),
        b"torn entry"
    );
    ledger
        .quarantine(1, "q-1", "operator", 7, || Ok(7), 101)
        .unwrap();
    // A crash after the record, before the move: the rerun moves it.
    let entry = qdir.join(format!("{:020}.entry", 1));
    fs::rename(&entry, &bad).unwrap();
    ledger
        .quarantine(1, "q-1", "operator", 7, || Ok(7), 101)
        .unwrap();
    assert!(entry.exists() && !bad.exists());
    let reused = ledger
        .quarantine(1, "q-2", "operator", 7, || Ok(7), 102)
        .unwrap_err();
    assert!(reused.starts_with(LIVE_REQUEST_REUSED), "{reused}");
    // The next append never takes the quarantined number.
    assert_eq!(
        ledger.append(&released(2, LiveIntentKind::Buy, 0xB, 1_000, 1_300), 3),
        Ok(2)
    );
    // Run again, the request moves only the bytes it recorded: an entry
    // of another writer at that number stays, the moved one is kept.
    let other = fs::read(dir.join("live-ledger").join(format!("{:020}.dbrec", 2))).unwrap();
    fs::write(&bad, &other).unwrap();
    ledger
        .quarantine(1, "q-1", "operator", 7, || Ok(7), 103)
        .unwrap();
    assert_eq!(fs::read(&bad).unwrap(), other);
    assert_eq!(
        fs::read(qdir.join(format!("{:020}.entry", 1))).unwrap(),
        b"torn entry"
    );
    fs::remove_file(&bad).unwrap();
    fs::write(qdir.join(format!("{:020}.dbrec", 9)), b"x").unwrap();
    assert_eq!(ledger.quarantined(), Ok(Some(u64::MAX)));
    fs::remove_dir_all(dir).unwrap();
}

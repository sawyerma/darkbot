//! Rust↔TS golden bytes of the prepared BSC V8 contracts (BSC-V8-09, FE-V8-17): training,
//! Strategy C settings, trade history, wallets, WATCH and the bounded query envelope. No native
//! producer exists yet; these bytes pin the wire shape the backend implements against: u64/i64
//! limits, signed and >u128 decimal wei, absent vs present-zero optionals, enums and oneofs.
//! frontend/src/features/bsc/api/bsc-v8-golden.test.ts decodes the same bytes and re-encodes
//! them byte for byte.
use darkbot_gateway::proto::bsc::v1::{
    bsc_query_request, bsc_query_response, Availability, BscQueryRequest, BscQueryResponse,
    BscWatchState, ChainTxStatus, CreatorSellExit, CreatorSoldBeforeFill, DataQuality, Evidence,
    ExitReason, ExitReasonSource, LiveStrategySelectCommand, OwnerProvenance, PerformanceMetric,
    PerformancePoint, PerformanceScope, PerformanceSeries, RequalificationState,
    SettingsApplyStatus, SourcedTime, StrategyCSettings, StrategyCSettingsSetAllCommand,
    StrategyCSettingsState, StrategyFeatureSpec, StrategyFilterRule, TimeSource, TradeCounts,
    TradeDetail, TradeHistoryPage, TradeHistoryQuery, TradePlacement, TradeRow, TradeSighting,
    TradeSort, TradeStatus, TradeTransaction, TrainingProfile, TrainingProfileStatus,
    TrainingProfiles, TrainingResult, TrainingRun, TrainingRunMode, TrainingRunPage,
    TrainingRunPlan, TrainingRunStartCommand, TrainingRunStatus, TrainingRunStopKind,
    TransferStatus, WalletBalanceSnapshot, WalletDestination, WalletHistoryPage, WalletMovement,
    WalletMovementKind, WalletRole, WalletSweepPolicy, WalletSweepPolicySetCommand,
    WalletTransferRequest, WalletTransferState, WalletTreasuryState, WatchSubjectKind,
    WatchSubjectState, WatchVerdict,
};
use prost::Message;
use std::{fs, path::PathBuf};

/// 2^256 − 1: the largest wei amount, beyond every native integer type of the browser.
const U256_MAX: &str =
    "115792089237316195423570985008687907853269984665640564039457584007913129639935";
const LOT: &str = "0x3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a";
const TOKEN: &str = "0x6161616161616161616161616161616161616161";
const FEATURES: [&str; 15] = [
    "buy_count",
    "sell_count",
    "buy_volume_bnb",
    "sell_volume_bnb",
    "net_buy_flow_bnb",
    "unique_buyers",
    "unique_buyer_ratio",
    "largest_buy_bnb",
    "buyer_volume_concentration",
    "first5_buyer_concentration",
    "first_sell_latency_ms",
    "price_change",
    "max_drawdown",
    "dev_initial_buy",
    "curve_progress_pct",
];

fn check(name: &str, bytes: Vec<u8>) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../protocol/golden/bsc/v8");
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>() + "\n";
    let path = dir.join(format!("{name}.hex"));
    if std::env::var_os("UPDATE_BSC_V8_GOLDENS").is_some() {
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, &hex).unwrap();
    } else {
        assert_eq!(
            fs::read_to_string(&path).unwrap_or_default(),
            hex,
            "{name}: regenerate with UPDATE_BSC_V8_GOLDENS=1 after a reviewed wire change"
        );
    }
}

fn provenance(owner: &str, quality: DataQuality) -> Option<OwnerProvenance> {
    Some(OwnerProvenance {
        owner: owner.into(),
        owner_version: u64::MAX,
        observed_unix_ms: 1_793_000_000_000,
        evidence: Some(Evidence {
            path: "sources/live-ledger".into(),
            sha256: "ab".repeat(32),
            observed_unix_s: 1_793_000_000,
        }),
        quality: quality as i32,
        quality_reasons: if quality == DataQuality::Verified {
            vec![]
        } else {
            vec!["RECEIPT_WINDOW_OPEN".into()]
        },
        provenance: "NATIVE_CREATED".into(),
    })
}

fn block_time(unix_ms: Option<u64>, block: Option<u64>) -> Option<SourcedTime> {
    Some(SourcedTime {
        unix_ms,
        source: TimeSource::BlockHeader as i32,
        block_number: block,
    })
}

fn settings() -> StrategyCSettings {
    StrategyCSettings {
        input_wei: Some(U256_MAX.into()),
        target_net_multiple: Some("1.50".into()),
        stop_net_multiple: Some("0.80".into()),
        observation_ms: Some(0),
        hold_ms: Some(86_400_000),
        max_open: Some(1_000),
        creator_sold_before_fill: CreatorSoldBeforeFill::TxLands as i32,
        creator_sell_exit: CreatorSellExit::Enabled as i32,
        // Every feature, in native order: min only, max only, both, neither, disabled.
        filters: FEATURES
            .iter()
            .enumerate()
            .map(|(i, f)| StrategyFilterRule {
                feature: (*f).into(),
                enabled: i % 5 != 4,
                min: (i % 5 == 0 || i % 5 == 2).then(|| {
                    if *f == "net_buy_flow_bnb" {
                        "-0.000000000000000001".into()
                    } else {
                        "3".into()
                    }
                }),
                max: (i % 5 == 1 || i % 5 == 2).then(|| "0.15".into()),
            })
            .collect(),
        first_buyer_window: Some(100),
        ratio_places: Some(18),
        fill_delay_blocks: None,
        gas_price_wei: Some("0".into()),
    }
}

fn trade_row(closed: bool) -> TradeRow {
    TradeRow {
        lot: LOT.into(),
        token: TOKEN.into(),
        status: if closed {
            TradeStatus::Closed
        } else {
            TradeStatus::Open
        } as i32,
        venue: "BSC_VENUE_FLAP".into(),
        component: "BSC_STRATEGY_C".into(),
        input_wei: "100000000000000000".into(),
        proceeds_wei: closed.then(|| "0".into()),
        proceeds_unknown: closed,
        pnl_wei: closed.then(|| format!("-{U256_MAX}")),
        roi_ppm: closed.then_some(i64::MIN),
        buy_time: block_time(Some(1_793_000_000_000), Some(u64::MAX)),
        sell_time: block_time(closed.then_some(1_793_000_028_000), None),
        hold_ms: closed.then_some(0),
        first_source: if closed {
            "BSC_INGRESS_GETH".into()
        } else {
            String::new()
        },
        lead_before_block_us: closed.then_some(i64::MIN),
        detect_to_send_ns: closed.then_some(u64::MAX),
        send_to_ack_ns: None,
        gas_wei: "0".into(),
        fees_wei: None,
        placement: if closed {
            TradePlacement::First
        } else {
            TradePlacement::Unknown
        } as i32,
        exit_reason: if closed {
            ExitReason::Target
        } else {
            ExitReason::None
        } as i32,
        exit_reason_source: if closed {
            ExitReasonSource::StrategyJournal
        } else {
            ExitReasonSource::Unknown
        } as i32,
        exit_reason_detail: String::new(),
        buy_tx_hash: LOT.into(),
        sell_tx_hash: if closed {
            "0x".to_owned() + &"b1".repeat(32)
        } else {
            String::new()
        },
        run: closed.then_some(u64::MAX),
        ledger_version: u64::MAX,
        quality: if closed {
            DataQuality::Verified
        } else {
            DataQuality::Unresolved
        } as i32,
        quality_reasons: vec![],
    }
}

#[test]
fn bsc_v8_contract_golden_bytes() {
    check(
        "query_trades_request",
        BscQueryRequest {
            request_id: "123e4567-e89b-12d3-a456-426614174000".into(),
            query: Some(bsc_query_request::Query::Trades(TradeHistoryQuery {
                statuses: vec![TradeStatus::Open as i32, TradeStatus::Pending as i32],
                gains: Some(false),
                venues: vec!["BSC_VENUE_FLAP".into()],
                from_unix_ms: Some(0),
                to_unix_ms: Some(u64::MAX),
                search: "0xab".into(),
                sort: TradeSort::ActiveFirst as i32,
                cursor: "c1".into(),
                limit: u32::MAX,
            })),
        }
        .encode_to_vec(),
    );
    check(
        "query_not_implemented",
        BscQueryResponse {
            request_id: "123e4567-e89b-12d3-a456-426614174000".into(),
            availability: Availability::NotImplemented as i32,
            unavailable_reason: "BSC-V8-10".into(),
            result: None,
        }
        .encode_to_vec(),
    );
    check(
        "trade_page",
        BscQueryResponse {
            request_id: "r".into(),
            availability: Availability::Available as i32,
            unavailable_reason: String::new(),
            result: Some(bsc_query_response::Result::Trades(TradeHistoryPage {
                availability: Availability::Available as i32,
                unavailable_reason: String::new(),
                provenance: provenance("bsc_runtime live ledger", DataQuality::Incomplete),
                rows: vec![trade_row(true), trade_row(false)],
                next_cursor: "c2".into(),
                has_more: true,
                total: u64::MAX,
                counts: Some(TradeCounts {
                    pending: 0,
                    running: 1,
                    wins: 0,
                    losses: 1,
                    flat: 0,
                    failed: 0,
                    written_off: u64::MAX,
                }),
            })),
        }
        .encode_to_vec(),
    );
    check(
        "trade_detail",
        TradeDetail {
            availability: Availability::Available as i32,
            unavailable_reason: String::new(),
            provenance: provenance("bsc_runtime live ledger", DataQuality::Verified),
            row: Some(trade_row(true)),
            transactions: vec![TradeTransaction {
                tx_hash: LOT.into(),
                intent: "BUY".into(),
                nonce: u64::MAX,
                state: "INCLUDED".into(),
                block_number: Some(0),
                block_time: block_time(Some(1_793_000_000_000), Some(0)),
                gas_wei: Some("0".into()),
                source: "RECEIPT".into(),
                released: Some(SourcedTime {
                    unix_ms: Some(1_792_999_999_900),
                    source: TimeSource::LocalWall as i32,
                    block_number: None,
                }),
            }],
            sightings: vec![TradeSighting {
                source: "BSC_INGRESS_PUBLICNODE".into(),
                capability: "HASH_ONLY".into(),
                seen: Some(SourcedTime {
                    unix_ms: Some(1_793_000_000_100),
                    source: TimeSource::LocalWall as i32,
                    block_number: None,
                }),
                lead_before_block_us: Some(-100_000),
                first_verified: false,
            }],
            launch_hash: "0x".to_owned() + &"c1".repeat(32),
            launch_block: Some(u64::MAX),
            launch_time: block_time(Some(1_793_000_000_000), Some(u64::MAX)),
            clock_id: Some(u64::MAX),
            t0_exact: false,
            t_detect_ns: Some(0),
            t_select_ns: None,
            t_send_ns: Some(u64::MAX),
            t_ack_ns: None,
            launch_index: Some(0),
            own_index: Some(1),
            buys_before: Some(0),
            tokens: Some(U256_MAX.into()),
            grant_generation: Some(u64::MAX),
            config_sha256: "cd".repeat(32),
            decisions: vec!["RESUME".into(), "WRITE_OFF".into()],
        }
        .encode_to_vec(),
    );
    check(
        "strategy_settings_state",
        StrategyCSettingsState {
            availability: Availability::Available as i32,
            unavailable_reason: String::new(),
            provenance: provenance("bsc_runtime strategy settings", DataQuality::Verified),
            stored: Some(settings()),
            stored_revision: u64::MAX,
            stored_config_sha256: "ef".repeat(32),
            running: None,
            running_revision: None,
            running_config_sha256: String::new(),
            apply_status: SettingsApplyStatus::NotRunning as i32,
            refusal_reason: String::new(),
            request_id: "req".into(),
            features: FEATURES
                .iter()
                .enumerate()
                .map(|(i, f)| StrategyFeatureSpec {
                    name: (*f).into(),
                    index: i as u32,
                    unit: if f.ends_with("_bnb") { "bnb" } else { "count" }.into(),
                    places: if f.ends_with("_bnb") { 18 } else { 0 },
                    signed: *f == "net_buy_flow_bnb" || *f == "price_change",
                    filterable: true,
                })
                .collect(),
        }
        .encode_to_vec(),
    );
    check(
        "strategy_set_all_command",
        StrategyCSettingsSetAllCommand {
            settings: Some(settings()),
        }
        .encode_to_vec(),
    );
    check(
        "training_profiles",
        TrainingProfiles {
            availability: Availability::Available as i32,
            unavailable_reason: String::new(),
            provenance: provenance("bsc_runtime training profiles", DataQuality::Verified),
            profiles: vec![
                TrainingProfile {
                    profile_id: "p1".into(),
                    name: "Fast Entry".into(),
                    revision: u64::MAX,
                    settings: Some(settings()),
                    config_sha256: "01".repeat(32),
                    venues: vec!["BSC_VENUE_FLAP".into(), "BSC_VENUE_OPENFOUR".into()],
                    created_unix_ms: 1,
                    updated_unix_ms: u64::MAX,
                    status: TrainingProfileStatus::Active as i32,
                    run_ids: vec!["run-2".into(), "run-1".into()],
                    in_use: true,
                    in_use_reason: "TRAINING_RUN_RUNNING".into(),
                },
                TrainingProfile {
                    profile_id: "p2".into(),
                    name: "Archived".into(),
                    revision: 0,
                    settings: Some(StrategyCSettings::default()),
                    config_sha256: String::new(),
                    venues: vec![],
                    created_unix_ms: 0,
                    updated_unix_ms: 0,
                    status: TrainingProfileStatus::Archived as i32,
                    run_ids: vec![],
                    in_use: false,
                    in_use_reason: String::new(),
                },
            ],
            catalog_version: 7,
            live_profile_id: "p1".into(),
            live_profile_revision: Some(0),
        }
        .encode_to_vec(),
    );
    check(
        "training_run_page",
        TrainingRunPage {
            availability: Availability::Available as i32,
            unavailable_reason: String::new(),
            provenance: provenance("bsc_runtime training runs", DataQuality::Incomplete),
            runs: vec![
                TrainingRun {
                    run_id: "run-2".into(),
                    profile_id: "p1".into(),
                    profile_revision: u64::MAX,
                    config_sha256: "01".repeat(32),
                    mode: TrainingRunMode::Paper as i32,
                    venues: vec!["BSC_VENUE_FLAP".into()],
                    plan: Some(TrainingRunPlan {
                        stop_kind: TrainingRunStopKind::UntilStopped as i32,
                        duration_ms: None,
                        max_launches: None,
                    }),
                    started_unix_ms: Some(1_793_000_000_000),
                    ended_unix_ms: None,
                    status: TrainingRunStatus::Running as i32,
                    stop_reason: String::new(),
                    launches_processed: u64::MAX,
                    result: Some(TrainingResult {
                        evaluated: 37,
                        passed: 16,
                        rejected: 17,
                        data_incomplete: 4,
                        not_tracked: 0,
                        unresolved: 1,
                        entered: 16,
                        exits_target: 11,
                        exits_stop: 3,
                        exits_creator_sell: 1,
                        exits_timeout: 0,
                        wins: 11,
                        losses: 4,
                        gain_wei: U256_MAX.into(),
                        loss_wei: "0".into(),
                        net_wei: None,
                        open_positions: 1,
                        sources: vec!["BSC_INGRESS_GETH".into()],
                        observable_ms: Some(0),
                        lost_records: 0,
                        ingress_dropped: u64::MAX,
                        quality: DataQuality::Incomplete as i32,
                        quality_reasons: vec!["JOURNAL_NOT_SEALED".into()],
                    }),
                    provenance: provenance("bsc_runtime training runs", DataQuality::Unresolved),
                    request_id: "req-2".into(),
                },
                TrainingRun {
                    run_id: "run-1".into(),
                    profile_id: "p1".into(),
                    profile_revision: 0,
                    config_sha256: String::new(),
                    mode: TrainingRunMode::Replay as i32,
                    venues: vec![],
                    plan: Some(TrainingRunPlan {
                        stop_kind: TrainingRunStopKind::Duration as i32,
                        duration_ms: Some(3_600_000),
                        max_launches: Some(0),
                    }),
                    started_unix_ms: Some(0),
                    ended_unix_ms: Some(0),
                    status: TrainingRunStatus::Completed as i32,
                    stop_reason: "PLAN_DURATION".into(),
                    launches_processed: 0,
                    result: Some(TrainingResult {
                        net_wei: Some("-1".into()),
                        ..TrainingResult::default()
                    }),
                    provenance: None,
                    request_id: String::new(),
                },
            ],
            next_cursor: String::new(),
            has_more: false,
        }
        .encode_to_vec(),
    );
    check(
        "training_run_start_command",
        TrainingRunStartCommand {
            profile_id: "p1".into(),
            mode: TrainingRunMode::Shadow as i32,
            venues: vec!["BSC_VENUE_FOUR_CLASSIC".into()],
            plan: Some(TrainingRunPlan {
                stop_kind: TrainingRunStopKind::LaunchCount as i32,
                duration_ms: None,
                max_launches: Some(100),
            }),
            replay_source: String::new(),
        }
        .encode_to_vec(),
    );
    check(
        "live_strategy_select_command",
        LiveStrategySelectCommand {
            profile_id: "p1".into(),
            profile_revision: u64::MAX,
            config_sha256: "01".repeat(32),
        }
        .encode_to_vec(),
    );
    let transfer = WalletTransferState {
        request_id: "sweep-1".into(),
        command_id: "cmd-1".into(),
        source_role: WalletRole::Buyer as i32,
        source_address: TOKEN.into(),
        destination_address: "0x8282828282828282828282828282828282828282".into(),
        chain_id: 56,
        amount_wei: U256_MAX.into(),
        fee_wei: None,
        nonce: Some(0),
        tx_hash: LOT.into(),
        status: TransferStatus::Unknown as i32,
        failure_reason: "HANDOFF_INCOMPLETE".into(),
        created_unix_ms: 1,
        updated_unix_ms: u64::MAX,
        policy_version: Some(u64::MAX),
    };
    check(
        "wallet_history_page",
        WalletHistoryPage {
            availability: Availability::Available as i32,
            unavailable_reason: String::new(),
            provenance: provenance("bsc wallet owner", DataQuality::Verified),
            movements: vec![
                WalletMovement {
                    movement_id: "m1".into(),
                    role: WalletRole::Launcher as i32,
                    wallet_address: TOKEN.into(),
                    chain_id: 56,
                    kind: WalletMovementKind::Deposit as i32,
                    counterparty_address: "0x8282828282828282828282828282828282828282".into(),
                    counterparty_role: WalletRole::External as i32,
                    amount_wei: U256_MAX.into(),
                    fee_wei: "0".into(),
                    tx_hash: LOT.into(),
                    nonce: None,
                    block_number: Some(u64::MAX),
                    block_time: block_time(Some(1_793_000_000_000), Some(u64::MAX)),
                    status: ChainTxStatus::Final as i32,
                    origin: "CHAIN_BACKFILL".into(),
                    lot: String::new(),
                    transfer_request_id: String::new(),
                    quality: DataQuality::Verified as i32,
                },
                WalletMovement {
                    movement_id: "m2".into(),
                    role: WalletRole::Buyer as i32,
                    kind: WalletMovementKind::Withdrawal as i32,
                    amount_wei: format!("-{U256_MAX}"),
                    nonce: Some(u64::MAX),
                    status: ChainTxStatus::Unknown as i32,
                    origin: "TRANSFER_OWNER".into(),
                    transfer_request_id: "sweep-1".into(),
                    quality: DataQuality::Unresolved as i32,
                    ..WalletMovement::default()
                },
            ],
            next_cursor: "w2".into(),
            has_more: true,
            deposited_wei: U256_MAX.into(),
            withdrawn_wei: U256_MAX.into(),
        }
        .encode_to_vec(),
    );
    check(
        "treasury_state",
        WalletTreasuryState {
            availability: Availability::Available as i32,
            unavailable_reason: String::new(),
            provenance: provenance("bsc wallet owner", DataQuality::Stale),
            balances: vec![WalletBalanceSnapshot {
                role: WalletRole::Buyer as i32,
                address: TOKEN.into(),
                chain_id: 56,
                balance_wei: U256_MAX.into(),
                reserved_wei: None,
                available_wei: Some("0".into()),
                block_number: Some(u64::MAX),
                observed: Some(SourcedTime {
                    unix_ms: Some(1_793_000_000_000),
                    source: TimeSource::LocalWall as i32,
                    block_number: Some(u64::MAX),
                }),
                source: "LIVE_PROBE".into(),
                quality: DataQuality::Stale as i32,
            }],
            destinations: vec![WalletDestination {
                destination_id: "d1".into(),
                label: "MetaMask".into(),
                address: "0x8282828282828282828282828282828282828282".into(),
                chain_id: 56,
                confirmed: true,
                created_unix_ms: 1,
            }],
            sweep: Some(WalletSweepPolicy {
                enabled: true,
                source_role: WalletRole::Buyer as i32,
                destination_id: "d1".into(),
                destination_address: "0x8282828282828282828282828282828282828282".into(),
                threshold_wei: U256_MAX.into(),
                retain_wei: "250000000000000000".into(),
                policy_version: u64::MAX,
                last_execution: Some(transfer.clone()),
                request_id: "pol-1".into(),
            }),
            transfers: vec![transfer],
            wallet_policy_version: u64::MAX,
        }
        .encode_to_vec(),
    );
    check(
        "wallet_transfer_request",
        WalletTransferRequest {
            source_role: WalletRole::Launcher as i32,
            destination_id: "d1".into(),
            destination_address: "0x8282828282828282828282828282828282828282".into(),
            chain_id: 56,
            amount_wei: "1".into(),
            max_fee_wei: U256_MAX.into(),
        }
        .encode_to_vec(),
    );
    check(
        "sweep_set_command",
        WalletSweepPolicySetCommand {
            enabled: false,
            source_role: WalletRole::Buyer as i32,
            destination_id: "d1".into(),
            threshold_wei: "1000000000000000000".into(),
            retain_wei: "0".into(),
        }
        .encode_to_vec(),
    );
    check(
        "performance_series",
        PerformanceSeries {
            availability: Availability::Available as i32,
            unavailable_reason: String::new(),
            provenance: provenance("bsc_runtime live ledger", DataQuality::Verified),
            metric: PerformanceMetric::RealizedNetPnl as i32,
            scope: PerformanceScope::Live as i32,
            scope_id: String::new(),
            points: vec![
                PerformancePoint {
                    at: block_time(Some(0), Some(0)),
                    value_wei: "0".into(),
                    fact_id: String::new(),
                    quality: DataQuality::Verified as i32,
                },
                PerformancePoint {
                    at: block_time(Some(u64::MAX), None),
                    value_wei: format!("-{U256_MAX}"),
                    fact_id: LOT.into(),
                    quality: DataQuality::Unresolved as i32,
                },
            ],
            opening_wei: None,
        }
        .encode_to_vec(),
    );
    check(
        "watch_not_implemented",
        BscWatchState {
            availability: Availability::NotImplemented as i32,
            unavailable_reason: "REV.2.0 WATCH (BSC-V8-18)".into(),
            provenance: None,
            subjects: vec![WatchSubjectState {
                subject_id: "BSC_VENUE_FLAP".into(),
                kind: WatchSubjectKind::VenueDeployment as i32,
                bound: String::new(),
                observed: String::new(),
                verdict: WatchVerdict::Unknown as i32,
                requalification: RequalificationState::Unspecified as i32,
                latched: false,
                source: String::new(),
                observed_unix_ms: None,
                drift_since_unix_ms: None,
                reason: "WATCH_NOT_IMPLEMENTED".into(),
            }],
            recent_events: vec![],
        }
        .encode_to_vec(),
    );
}

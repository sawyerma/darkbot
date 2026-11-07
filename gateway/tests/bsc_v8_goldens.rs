//! Rust↔TS golden bytes of the prepared BSC V8 contracts (BSC-V8-09, FE-V8-17): training,
//! Strategy C settings, trade history, wallets, WATCH and the bounded query envelope. No native
//! producer exists yet; these bytes pin the wire shape the backend implements against: u64/i64
//! limits, signed and >u128 decimal wei, absent vs present-zero optionals, enums and oneofs.
//! frontend/src/features/bsc/api/bsc-v8-golden.test.ts decodes the same bytes and re-encodes
//! them byte for byte.
use darkbot_gateway::proto::bsc::v1::{
    bsc_query_request, bsc_query_response, policy_amount, wallet_transfer_prepare_command,
    Availability, BscQueryRequest, BscQueryResponse, BscWatchState, ChainTxStatus, ControlSwitch,
    CreatorSellExit, CreatorSoldBeforeFill, DataQuality, Evidence, ExitReason, ExitReasonSource,
    FiatQuote, LiveAttempt, LiveStrategySelectCommand, LiveTransaction, MainPayoutPolicy,
    MainPayoutPolicySetCommand, OwnerProvenance, PerformanceMetric, PerformancePoint,
    PerformanceScope, PerformanceSeries, PolicyAmount, PolicyUnit, ProfitTransferPolicy,
    ProfitTransferPolicySetCommand, QueryRefusal, ReplaySource, ReplaySourcePage, ReplayVerdict,
    RequalificationState, SettingsApplyStatus, SourcedTime, StrategyCSettableValues,
    StrategyCSettings, StrategyCSettingsSetAllCommand, StrategyCSettingsState, StrategyFeatureSpec,
    StrategyFilterRule, StrategySettingSpec, TimeSource, TradeCounts, TradeDetail, TradeExitTerms,
    TradeHistoryPage, TradeHistoryQuery, TradePlacement, TradeRow, TradeSighting, TradeSort,
    TradeStatus, TradeTransaction, TrainingProfile, TrainingProfileStatus, TrainingProfiles,
    TrainingResult, TrainingRun, TrainingRunMode, TrainingRunPage, TrainingRunPlan,
    TrainingRunStartCommand, TrainingRunStatus, TrainingRunStopKind, TransferStatus, WalletAccount,
    WalletAuthorization, WalletAuthorizationBounds, WalletAuthorizationScope,
    WalletBalanceSnapshot, WalletDestination, WalletDestinationConfirmCommand,
    WalletDestinationSetCommand, WalletFundTradingCommand, WalletHistoryPage, WalletMovement,
    WalletMovementKind, WalletPayoutCommand, WalletRole, WalletTransferAuthorizeCommand,
    WalletTransferPrepareCommand, WalletTransferReceipt, WalletTransferState, WalletTreasuryState,
    WatchSubjectKind, WatchSubjectState, WatchVerdict,
};
use prost::Message;
use std::{fs, path::PathBuf};

/// 2^256 − 1: the largest wei amount, beyond every native integer type of the browser.
const U256_MAX: &str =
    "115792089237316195423570985008687907853269984665640564039457584007913129639935";
/// The canonical scope texts (wallet.proto) and their SHA-256: the vectors both the native owner
/// and the browser reproduce byte for byte (frontend model/wallets.test.ts builds the same texts).
const SCOPE_TRANSFER: &str = include_str!("../../protocol/golden/bsc/v8/scope_wallet_transfer.txt");
const SCOPE_TRANSFER_SHA: &str = "9bbf9226da8df80e8999ce018d4ca5fff4c2447acb19e4057e210729b8ef0a4d";
const SCOPE_PROFIT_SHA: &str = "36f5e77e4de86b52131b9c194a595406170022e29a948aeb0b687c95c644b40d";
const SCOPE_PAYOUT_SHA: &str = "d4eceffb071ad47b72ef74a47915456026c14d2d3180e26455ecda0e5b25eb16";
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

/// The settable values of a command: settings() without the configuration-owned ones.
fn values() -> StrategyCSettableValues {
    let s = settings();
    StrategyCSettableValues {
        input_wei: s.input_wei,
        target_net_multiple: s.target_net_multiple,
        stop_net_multiple: s.stop_net_multiple,
        observation_ms: s.observation_ms,
        hold_ms: s.hold_ms,
        max_open: s.max_open,
        creator_sold_before_fill: s.creator_sold_before_fill,
        creator_sell_exit: s.creator_sell_exit,
        filters: s.filters,
        first_buyer_window: s.first_buyer_window,
    }
}

/// A native switch as the owner holds it.
fn switch(id: &str, effective: &str, block_reason: &str) -> Option<ControlSwitch> {
    Some(ControlSwitch {
        id: id.into(),
        requested: "ENABLED".into(),
        effective: effective.into(),
        block_reason: block_reason.into(),
        generation: u64::MAX,
        request_id: "sw-1".into(),
        set_generation: Some(0),
    })
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
        quality_reasons: if closed {
            vec![]
        } else {
            vec!["GAS_UPPER_BOUND".into(), "CLOCK_SUSPECT".into()]
        },
        trigger_source: if closed {
            "BSC_INGRESS_GETH".into()
        } else {
            String::new()
        },
        fees_reason: "FEES_NOT_ATTRIBUTED".into(),
        adopted: !closed,
        evidence_sha256: if closed {
            String::new()
        } else {
            "ee".repeat(32)
        },
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
            refusal: QueryRefusal::Unspecified as i32,
            refusal_detail: String::new(),
        }
        .encode_to_vec(),
    );
    check(
        "query_refused",
        BscQueryResponse {
            request_id: "123e4567-e89b-12d3-a456-426614174000".into(),
            availability: Availability::Available as i32,
            unavailable_reason: String::new(),
            result: None,
            refusal: QueryRefusal::CursorStale as i32,
            refusal_detail: "SNAPSHOT_COMPACTED".into(),
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
                realized_net_pnl_wei: Some(format!("-{U256_MAX}")),
                burn_gas_wei: None,
            })),
            refusal: QueryRefusal::Unspecified as i32,
            refusal_detail: String::new(),
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
                block_time: block_time(Some(1_793_000_000_000), Some(0)),
                gas_wei: Some("0".into()),
                released: Some(SourcedTime {
                    unix_ms: Some(1_792_999_999_900),
                    source: TimeSource::LocalWall as i32,
                    block_number: None,
                }),
                tx: Some(LiveTransaction {
                    tx_hash: LOT.into(),
                    intent: "BUY".into(),
                    token: TOKEN.into(),
                    nonce: u64::MAX,
                    state: "INCLUDED".into(),
                    block_number: Some(0),
                    source: "RECEIPT".into(),
                    burn_requested: false,
                    burnable: false,
                }),
                orphaned: true,
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
            grant_generation: Some(u64::MAX),
            decisions: vec!["RESUME".into(), "WRITE_OFF".into()],
            attempt: Some(LiveAttempt {
                ordinal: 1,
                tx: LOT.into(),
                kind: "STRATEGY_BUY".into(),
                cell: "STRATEGY_ENTRY".into(),
                state: "INCLUDED".into(),
                verdict: "NOT_APPLICABLE".into(),
                token: TOKEN.into(),
                launch_hash: String::new(),
                clock_id: Some(u64::MAX),
                t_ingress_ns: Some(0),
                t_send_ns: Some(u64::MAX),
                launch_index: Some(0),
                own_index: Some(1),
                buys_before: Some(0),
                tokens: Some(U256_MAX.into()),
                t0_exact: false,
                window_ms: 15_000,
                ..LiveAttempt::default()
            }),
            exit_terms: Some(TradeExitTerms {
                ratio_places: 18,
                target_net_multiple: "1.50".into(),
                stop_net_multiple: "0.80".into(),
                hold_ms: u64::MAX,
                creator_sell_exit: CreatorSellExit::Disabled as i32,
            }),
            settings_generation: Some(u64::MAX),
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
            effective: Some(StrategyCSettings {
                fill_delay_blocks: Some(0),
                ..settings()
            }),
            setting_specs: vec![
                StrategySettingSpec {
                    key: "BSC_STRATEGY_C_INPUT_WEI".into(),
                    unit: "wei".into(),
                    min: Some("1".into()),
                    max: Some(U256_MAX.into()),
                    places: 0,
                },
                StrategySettingSpec {
                    key: "BSC_STRATEGY_C_FIRST_BUYER_WINDOW".into(),
                    unit: "count".into(),
                    min: Some("1".into()),
                    max: None,
                    places: 0,
                },
            ],
        }
        .encode_to_vec(),
    );
    check(
        "strategy_set_all_command",
        StrategyCSettingsSetAllCommand {
            values: Some(values()),
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
                    config_sha256: "01".repeat(32),
                    venues: vec!["BSC_VENUE_FLAP".into(), "BSC_VENUE_OPENFOUR".into()],
                    created_unix_ms: 1,
                    updated_unix_ms: u64::MAX,
                    status: TrainingProfileStatus::Active as i32,
                    run_ids: vec!["run-2".into(), "run-1".into()],
                    in_use: true,
                    in_use_reason: "TRAINING_RUN_RUNNING".into(),
                    values: Some(values()),
                    request_id: "prof-1".into(),
                },
                TrainingProfile {
                    profile_id: "p2".into(),
                    name: "Archived".into(),
                    revision: 0,
                    config_sha256: String::new(),
                    venues: vec![],
                    created_unix_ms: 0,
                    updated_unix_ms: 0,
                    status: TrainingProfileStatus::Archived as i32,
                    run_ids: vec![],
                    in_use: false,
                    in_use_reason: String::new(),
                    values: Some(StrategyCSettableValues::default()),
                    request_id: String::new(),
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
                        slot_limit: 2,
                        creator_sold: 1,
                        cancelled: u64::MAX,
                        cancelled_creator_sold: 1,
                        cancelled_not_tracked: 0,
                        cancelled_quote: 1,
                        cancelled_cost: 0,
                        superseded: 0,
                        paper_unpriced: 1,
                        open_at_end: 0,
                        replay_verdict: ReplayVerdict::NotRun as i32,
                        config_match: None,
                    }),
                    provenance: provenance("bsc_runtime training runs", DataQuality::Unresolved),
                    request_id: "req-2".into(),
                    replay_source: String::new(),
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
                    status: TrainingRunStatus::Interrupted as i32,
                    stop_reason: "RUNTIME_RESTART".into(),
                    launches_processed: 0,
                    result: Some(TrainingResult {
                        net_wei: Some("-1".into()),
                        replay_verdict: ReplayVerdict::Differs as i32,
                        config_match: Some(false),
                        ..TrainingResult::default()
                    }),
                    provenance: None,
                    request_id: String::new(),
                    replay_source: "rec-run-1".into(),
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
            expected_settings_generation: u64::MAX,
        }
        .encode_to_vec(),
    );
    check(
        "replay_source_page",
        ReplaySourcePage {
            availability: Availability::Available as i32,
            unavailable_reason: String::new(),
            provenance: provenance("bsc_runtime training runs", DataQuality::Verified),
            sources: vec![ReplaySource {
                source_id: "rec-run-1".into(),
                kind: "RECORDED_RUN".into(),
                run_id: "run-1".into(),
                from_block: Some(0),
                to_block: Some(u64::MAX),
                from: block_time(Some(0), Some(0)),
                to: block_time(None, Some(u64::MAX)),
                records: u64::MAX,
                config_sha256: "01".repeat(32),
                quality: DataQuality::Incomplete as i32,
                quality_reasons: vec!["JOURNAL_GAP".into()],
            }],
            next_cursor: String::new(),
            has_more: false,
        }
        .encode_to_vec(),
    );
    const MAIN: &str = "0x4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d";
    const EXTERNAL: &str = "0x8282828282828282828282828282828282828282";
    const SECOND: &str = "0x9393939393939393939393939393939393939393";
    let authorization = |sha: &str, reason: &str| WalletAuthorization {
        scope_text: "darkbot.bsc.wallet-profit-policy.v1\n".into(),
        scope_sha256: sha.repeat(32),
        expires_unix_ms: u64::MAX,
        max_per_transfer_wei: U256_MAX.into(),
        max_total_wei: U256_MAX.into(),
        used_total_wei: "0".into(),
        request_id: "auth-1".into(),
        block_reason: reason.into(),
    };
    let profit_transfer = WalletTransferState {
        request_id: "auto-a-1".into(),
        command_id: "intent-a-1".into(),
        kind: WalletMovementKind::ProfitTransfer as i32,
        source_role: WalletRole::Buyer as i32,
        source_address: TOKEN.into(),
        destination_role: WalletRole::Main as i32,
        destination_address: MAIN.into(),
        chain_id: 56,
        amount_wei: U256_MAX.into(),
        nonce: Some(0),
        tx_hash: LOT.into(),
        status: TransferStatus::Unknown as i32,
        failure_reason: "HANDOFF_INCOMPLETE".into(),
        created_unix_ms: 1,
        updated_unix_ms: u64::MAX,
        policy_version: Some(u64::MAX),
        trigger_id: "settle-7".into(),
        receipt: None,
        fee_estimate_wei: Some("21000000000000".into()),
        reserve_after_wei: None,
        scope_text: "darkbot.bsc.wallet-profit-policy.v1\n".into(),
        scope_sha256: "a1".repeat(32),
        expires_unix_ms: None,
        max_block: None,
        authorize_request_id: String::new(),
    };
    let payout = WalletTransferState {
        request_id: "auto-b-1".into(),
        command_id: "intent-b-1".into(),
        kind: WalletMovementKind::PayoutAuto as i32,
        source_role: WalletRole::Main as i32,
        source_address: MAIN.into(),
        destination_role: WalletRole::External as i32,
        destination_address: EXTERNAL.into(),
        chain_id: 56,
        amount_wei: "1".into(),
        nonce: Some(u64::MAX),
        tx_hash: LOT.into(),
        status: TransferStatus::Confirmed as i32,
        failure_reason: String::new(),
        created_unix_ms: 1,
        updated_unix_ms: 2,
        policy_version: Some(1),
        trigger_id: "balance-9".into(),
        receipt: Some(WalletTransferReceipt {
            request_id: "auto-b-1".into(),
            status: TransferStatus::Confirmed as i32,
            tx_hash: LOT.into(),
            block_number: Some(u64::MAX),
            block_time: block_time(Some(1_793_000_000_000), Some(u64::MAX)),
            gas_used: Some(21_000),
            fee_wei: Some("0".into()),
            reason: String::new(),
        }),
        fee_estimate_wei: None,
        reserve_after_wei: Some(U256_MAX.into()),
        scope_text: "darkbot.bsc.wallet-payout-policy.v1\n".into(),
        scope_sha256: "b2".repeat(32),
        expires_unix_ms: None,
        max_block: Some(u64::MAX),
        authorize_request_id: String::new(),
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
                    role: WalletRole::Main as i32,
                    wallet_address: MAIN.into(),
                    chain_id: 56,
                    kind: WalletMovementKind::Deposit as i32,
                    counterparty_address: EXTERNAL.into(),
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
                    policy_version: None,
                },
                WalletMovement {
                    movement_id: "m2".into(),
                    role: WalletRole::Buyer as i32,
                    kind: WalletMovementKind::ProfitTransfer as i32,
                    counterparty_role: WalletRole::Main as i32,
                    amount_wei: format!("-{U256_MAX}"),
                    nonce: Some(u64::MAX),
                    status: ChainTxStatus::Unknown as i32,
                    origin: "TREASURY_OWNER".into(),
                    transfer_request_id: "auto-a-1".into(),
                    quality: DataQuality::Unresolved as i32,
                    policy_version: Some(u64::MAX),
                    ..WalletMovement::default()
                },
            ],
            next_cursor: "w2".into(),
            has_more: true,
            deposited_total_wei: Some(U256_MAX.into()),
            paid_out_total_wei: None,
        }
        .encode_to_vec(),
    );
    let snapshot = |role: WalletRole, address: &str, balance: &str| WalletBalanceSnapshot {
        role: role as i32,
        address: address.into(),
        chain_id: 56,
        balance_wei: balance.into(),
        reserved_wei: None,
        available_wei: Some("0".into()),
        block_number: Some(u64::MAX),
        observed: Some(SourcedTime {
            unix_ms: Some(1_793_000_000_000),
            source: TimeSource::LocalWall as i32,
            block_number: Some(u64::MAX),
        }),
        source: "TREASURY_OWNER".into(),
        quality: DataQuality::Stale as i32,
    };
    check(
        "treasury_state",
        WalletTreasuryState {
            availability: Availability::Available as i32,
            unavailable_reason: String::new(),
            provenance: provenance("bsc wallet owner", DataQuality::Stale),
            accounts: vec![
                WalletAccount {
                    role: WalletRole::Main as i32,
                    address: MAIN.into(),
                    chain_id: 56,
                    balance: Some(snapshot(WalletRole::Main, MAIN, U256_MAX)),
                    profit_received_wei: Some(U256_MAX.into()),
                    other_capital_wei: Some("0".into()),
                    next_nonce: Some(u64::MAX),
                    owner_key: true,
                    control: switch("BSC_WALLET_MAIN", "ENABLED", ""),
                    ..WalletAccount::default()
                },
                WalletAccount {
                    role: WalletRole::Buyer as i32,
                    address: TOKEN.into(),
                    chain_id: 56,
                    balance: Some(snapshot(WalletRole::Buyer, TOKEN, "1")),
                    capital_basis_wei: Some(U256_MAX.into()),
                    realized_net_pnl_wei: Some(format!("-{U256_MAX}")),
                    transferred_profit_wei: Some("0".into()),
                    untransferred_profit_wei: Some("0".into()),
                    next_nonce: None,
                    owner_key: true,
                    control: switch("BSC_WALLET_BUYER", "BLOCKED", "RESET_BY_RESTART"),
                    ..WalletAccount::default()
                },
                WalletAccount {
                    role: WalletRole::Launcher as i32,
                    reason: "NOT_CONFIGURED".into(),
                    ..WalletAccount::default()
                },
            ],
            destinations: vec![
                WalletDestination {
                    destination_id: "d1".into(),
                    label: "MetaMask".into(),
                    address: EXTERNAL.into(),
                    chain_id: 56,
                    confirmed: true,
                    created_unix_ms: 1,
                    in_use: false,
                },
                WalletDestination {
                    destination_id: "d2".into(),
                    label: "Ledger".into(),
                    address: SECOND.into(),
                    chain_id: 56,
                    confirmed: false,
                    created_unix_ms: u64::MAX,
                    in_use: false,
                },
            ],
            profit_policy: Some(ProfitTransferPolicy {
                enabled: true,
                threshold_realized_net_profit_wei: "100000000000000000".into(),
                amount: Some(PolicyAmount {
                    amount: Some(policy_amount::Amount::ShareBps(10_000)),
                }),
                buyer_min_capital_wei: U256_MAX.into(),
                buyer_reserve_wei: "0".into(),
                policy_version: u64::MAX,
                settlement_cursor: "settle-7".into(),
                request_id: "pol-a-1".into(),
                last_transfer: Some(profit_transfer.clone()),
                untransferred_profit_wei: Some("0".into()),
                to_threshold_wei: None,
                effective_version: Some(u64::MAX),
                block_reason: String::new(),
                authorization: Some(authorization("a1", "")),
                net_since_last_settlement_wei: Some(format!("-{U256_MAX}")),
                settled_ledger_version: Some(u64::MAX),
                finality_depth: Some(15),
            }),
            payout_policy: Some(MainPayoutPolicy {
                enabled: false,
                destination_id: "d1".into(),
                destination_address: EXTERNAL.into(),
                unit: PolicyUnit::Bnb as i32,
                threshold_available_main_wei: U256_MAX.into(),
                threshold_eur: None,
                amount: Some(PolicyAmount {
                    amount: Some(policy_amount::Amount::FixedWei("0".into())),
                }),
                retain_wei: "250000000000000000".into(),
                policy_version: 1,
                trigger_cursor: "balance-9".into(),
                request_id: "pol-b-1".into(),
                last_payout: Some(payout.clone()),
                available_main_wei: None,
                effective_version: Some(0),
                block_reason: "DISABLED".into(),
                authorization: Some(authorization("b2", "RESET_BY_RESTART")),
            }),
            transfers: vec![profit_transfer, payout],
            treasury_version: u64::MAX,
            eur_quote: Some(FiatQuote {
                currency: "EUR".into(),
                fiat_per_bnb: "612.40".into(),
                observed: Some(SourcedTime {
                    unix_ms: Some(1_793_000_000_000),
                    source: TimeSource::LocalWall as i32,
                    block_number: None,
                }),
                source: "NATIVE_PRICE_OWNER".into(),
                quality: DataQuality::Stale as i32,
                max_age_ms: 60_000,
            }),
            transfer_fee_estimate_wei: Some("21000000000000".into()),
            decided_request_ids: vec!["pol-b-1".into(), "auth-1".into()],
            authorization_bounds: Some(WalletAuthorizationBounds {
                max_per_transfer_wei: "1000000000000000000".into(),
                max_total_wei: "5000000000000000000".into(),
                max_validity_ms: 604_800_000,
                draft_validity_ms: 600_000,
            }),
        }
        .encode_to_vec(),
    );
    check(
        "transfer_draft",
        WalletTransferState {
            request_id: "fund-1".into(),
            command_id: "draft-f-1".into(),
            kind: WalletMovementKind::CapitalFunding as i32,
            source_role: WalletRole::Main as i32,
            source_address: MAIN.into(),
            destination_role: WalletRole::Buyer as i32,
            destination_address: TOKEN.into(),
            chain_id: 56,
            amount_wei: "500000000000000000".into(),
            nonce: Some(7),
            status: TransferStatus::Draft as i32,
            created_unix_ms: 1_793_000_000_000,
            updated_unix_ms: 1_793_000_000_000,
            fee_estimate_wei: Some("21000000000000".into()),
            reserve_after_wei: Some("479000000000000000".into()),
            scope_text: SCOPE_TRANSFER.into(),
            scope_sha256: SCOPE_TRANSFER_SHA.into(),
            expires_unix_ms: Some(1_793_000_600_000),
            ..WalletTransferState::default()
        }
        .encode_to_vec(),
    );
    check(
        "destination_set_command",
        WalletDestinationSetCommand {
            label: "MetaMask".into(),
            address: EXTERNAL.into(),
            chain_id: 56,
            remove: false,
        }
        .encode_to_vec(),
    );
    check(
        "destination_confirm_command",
        WalletDestinationConfirmCommand {
            destination_id: "d2".into(),
            address: SECOND.into(),
        }
        .encode_to_vec(),
    );
    check(
        "transfer_prepare_fund_command",
        WalletTransferPrepareCommand {
            transfer: Some(wallet_transfer_prepare_command::Transfer::FundTrading(
                WalletFundTradingCommand {
                    amount_wei: U256_MAX.into(),
                    max_fee_wei: "0".into(),
                    chain_id: 56,
                    buyer_address: TOKEN.into(),
                },
            )),
        }
        .encode_to_vec(),
    );
    check(
        "transfer_prepare_payout_command",
        WalletTransferPrepareCommand {
            transfer: Some(wallet_transfer_prepare_command::Transfer::Payout(
                WalletPayoutCommand {
                    destination_id: "d1".into(),
                    destination_address: EXTERNAL.into(),
                    chain_id: 56,
                    amount_wei: "1".into(),
                    max_fee_wei: U256_MAX.into(),
                },
            )),
        }
        .encode_to_vec(),
    );
    check(
        "transfer_authorize_command",
        WalletTransferAuthorizeCommand {
            scope_sha256: SCOPE_TRANSFER_SHA.into(),
        }
        .encode_to_vec(),
    );
    check(
        "profit_policy_set_command",
        ProfitTransferPolicySetCommand {
            enabled: true,
            threshold_realized_net_profit_wei: "0".into(),
            amount: Some(PolicyAmount {
                amount: Some(policy_amount::Amount::ShareBps(1)),
            }),
            buyer_min_capital_wei: U256_MAX.into(),
            buyer_reserve_wei: "1".into(),
            authorization: Some(WalletAuthorizationScope {
                expires_unix_ms: u64::MAX,
                max_per_transfer_wei: "1".into(),
                max_total_wei: U256_MAX.into(),
                scope_sha256: SCOPE_PROFIT_SHA.into(),
            }),
        }
        .encode_to_vec(),
    );
    check(
        "payout_policy_set_command",
        MainPayoutPolicySetCommand {
            enabled: true,
            destination_id: "d1".into(),
            unit: PolicyUnit::Eur as i32,
            threshold_available_main_wei: String::new(),
            threshold_eur: Some("1000.5".into()),
            amount: Some(PolicyAmount {
                amount: Some(policy_amount::Amount::FixedEur("0.01".into())),
            }),
            retain_wei: U256_MAX.into(),
            destination_address: EXTERNAL.into(),
            authorization: Some(WalletAuthorizationScope {
                expires_unix_ms: 1_793_000_000_000,
                max_per_transfer_wei: "1000000000000000000".into(),
                max_total_wei: "5000000000000000000".into(),
                scope_sha256: SCOPE_PAYOUT_SHA.into(),
            }),
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
            truncated: true,
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

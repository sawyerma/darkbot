-- DarkBot migration 0001: initial schema.
-- Rules (contract persistence.*): forward-only, DDL only, no version literals, no data rows,
-- no BEGIN/COMMIT: the migration runner owns one atomic transaction that applies this DDL
-- together with the schema_version row it writes from the active contract.
-- Enum literals appear only inside CHECK constraints; the Phase-0 linter proves that
-- every CHECK below equals the corresponding enumeration in the active contract.

CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY,
    contract_version TEXT NOT NULL,
    migration_name TEXT NOT NULL,
    migration_sha256 TEXT NOT NULL,
    applied_at_utc_ns INTEGER NOT NULL
);

CREATE TABLE provider_schema_versions (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    schema_name TEXT NOT NULL,
    version_hash TEXT NOT NULL,
    fixture_manifest_json TEXT NOT NULL,
    created_at_utc_ns INTEGER NOT NULL,
    UNIQUE (provider, version_hash)
);

CREATE TABLE model_versions (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('COST', 'LATENCY', 'VENUE_TRANSITION', 'FEATURE_SET', 'RISK_POLICY', 'SPLIT_PROTOCOL', 'BOOTSTRAP_PROTOCOL', 'RESEARCH_ENTRY', 'PROTOCOL_CONFIG', 'FEED_PRICING', 'OPERATIONAL_LIMITS')),
    content_json TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    source TEXT NOT NULL,
    effective_from_utc_ns INTEGER,
    created_at_utc_ns INTEGER NOT NULL,
    UNIQUE (kind, content_sha256)
);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    persistent_session_sequence INTEGER NOT NULL UNIQUE,
    mode TEXT NOT NULL CHECK (mode IN ('SHADOW', 'BACKTEST', 'PAPER', 'LIVE')),
    final_state TEXT CHECK (final_state IN ('STARTING_RECONCILIATION', 'RUNNING', 'ENTRY_HALT', 'EMERGENCY_EXIT', 'MANUAL_REVIEW_REQUIRED', 'STOPPED')),
    contract_version TEXT NOT NULL,
    code_commit TEXT NOT NULL,
    provider_schema_version_id TEXT,
    protocol_config_model_id TEXT,
    protocol_config_read_slot INTEGER,
    protocol_config_read_utc_ns INTEGER,
    protocol_config_rpc_url_without_secret TEXT,
    feed_pricing_model_id TEXT,
    operational_limits_model_id TEXT,
    active_strategy_version_id TEXT,
    started_at_utc_ns INTEGER NOT NULL,
    started_at_monotonic_ns INTEGER NOT NULL,
    ended_at_utc_ns INTEGER,
    data_complete INTEGER NOT NULL DEFAULT 1 CHECK (data_complete IN (0, 1)),
    FOREIGN KEY (provider_schema_version_id) REFERENCES provider_schema_versions(id),
    FOREIGN KEY (protocol_config_model_id) REFERENCES model_versions(id),
    FOREIGN KEY (feed_pricing_model_id) REFERENCES model_versions(id),
    FOREIGN KEY (operational_limits_model_id) REFERENCES model_versions(id),
    FOREIGN KEY (active_strategy_version_id) REFERENCES strategy_versions(id)
);

CREATE TABLE strategy_versions (
    id TEXT PRIMARY KEY,
    version_hash TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK (status IN ('DRAFT', 'VALIDATED', 'FROZEN', 'ACTIVE', 'RETIRED')),
    candidate_name TEXT NOT NULL,
    cloned_from_version_id TEXT,
    feature_set_model_id TEXT,
    risk_policy_model_id TEXT,
    cost_model_id TEXT,
    latency_model_id TEXT,
    venue_transition_model_id TEXT,
    configuration_json TEXT NOT NULL,
    created_at_utc_ns INTEGER NOT NULL,
    frozen_at_utc_ns INTEGER,
    activated_at_utc_ns INTEGER,
    FOREIGN KEY (cloned_from_version_id) REFERENCES strategy_versions(id),
    FOREIGN KEY (feature_set_model_id) REFERENCES model_versions(id),
    FOREIGN KEY (risk_policy_model_id) REFERENCES model_versions(id),
    FOREIGN KEY (cost_model_id) REFERENCES model_versions(id),
    FOREIGN KEY (latency_model_id) REFERENCES model_versions(id),
    FOREIGN KEY (venue_transition_model_id) REFERENCES model_versions(id)
);

CREATE TABLE provider_observations (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    persistent_session_sequence INTEGER NOT NULL,
    provider_receive_sequence INTEGER NOT NULL,
    provider_received_monotonic_ns INTEGER NOT NULL,
    receive_offset_ns_from_session_start INTEGER NOT NULL,
    utc_received_ns INTEGER NOT NULL,
    chain_slot INTEGER,
    payload_sha256 TEXT NOT NULL,
    redacted_payload_json TEXT NOT NULL,
    parse_status TEXT NOT NULL CHECK (parse_status IN ('PARSED', 'IGNORED', 'PARSE_ERROR')),
    parse_error TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id),
    UNIQUE (session_id, provider_receive_sequence)
);

CREATE TABLE launches (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    mint TEXT NOT NULL,
    symbol TEXT,
    name TEXT,
    creator TEXT,
    pool TEXT,
    quote_mint TEXT,
    quote_scope TEXT NOT NULL CHECK (quote_scope IN ('SOL_WSOL', 'OBSERVE_ONLY')),
    status TEXT NOT NULL CHECK (status IN ('OBSERVING', 'DECIDED', 'POSITION', 'RAW_TRACKING', 'EVICTED')),
    create_observation_id INTEGER NOT NULL,
    t0_receive_offset_ns INTEGER NOT NULL,
    accepted INTEGER NOT NULL CHECK (accepted IN (0, 1)),
    acceptance_reason TEXT NOT NULL,
    curve_address_derived TEXT,
    curve_address_provider TEXT,
    curve_address_match INTEGER CHECK (curve_address_match IN (0, 1)),
    initial_virtual_sol_reserves INTEGER,
    initial_virtual_token_reserves INTEGER,
    initial_real_token_reserves INTEGER,
    initial_buy_token_base_units INTEGER,
    initial_buy_quote_lamports INTEGER,
    assigned_horizon_ms INTEGER NOT NULL CHECK (assigned_horizon_ms > 0),
    horizon_cohort TEXT NOT NULL,
    subscribe_sent_offset_ns INTEGER,
    subscribe_ack_offset_ns INTEGER,
    first_trade_received_offset_ns INTEGER,
    coverage_start_offset_ns INTEGER,
    coverage_state TEXT NOT NULL CHECK (coverage_state IN ('UNKNOWN_WARMUP', 'OBSERVED', 'GAP')),
    migrated INTEGER NOT NULL DEFAULT 0 CHECK (migrated IN (0, 1)),
    migration_event_id INTEGER,
    data_complete INTEGER NOT NULL DEFAULT 1 CHECK (data_complete IN (0, 1)),
    FOREIGN KEY (session_id) REFERENCES sessions(id),
    FOREIGN KEY (create_observation_id) REFERENCES provider_observations(id),
    FOREIGN KEY (migration_event_id) REFERENCES events(id),
    UNIQUE (session_id, mint),
    CHECK (coverage_state = 'UNKNOWN_WARMUP' OR coverage_start_offset_ns IS NOT NULL),
    CHECK (subscribe_ack_offset_ns IS NULL OR subscribe_sent_offset_ns IS NOT NULL)
);

CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    launch_id TEXT,
    observation_id INTEGER NOT NULL,
    provider_event_key TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('CREATE', 'BUY', 'SELL', 'MIGRATION')),
    mint TEXT NOT NULL,
    transaction_signature TEXT,
    provider_event_index INTEGER,
    actor TEXT,
    quote_lamports INTEGER,
    token_base_units INTEGER,
    virtual_sol_reserves INTEGER,
    virtual_token_reserves INTEGER,
    persistent_session_sequence INTEGER NOT NULL,
    provider_receive_sequence INTEGER NOT NULL,
    receive_offset_ns_from_session_start INTEGER NOT NULL,
    utc_received_ns INTEGER NOT NULL,
    chain_slot INTEGER,
    late INTEGER NOT NULL DEFAULT 0 CHECK (late IN (0, 1)),
    ambiguous INTEGER NOT NULL DEFAULT 0 CHECK (ambiguous IN (0, 1)),
    data_complete INTEGER NOT NULL DEFAULT 1 CHECK (data_complete IN (0, 1)),
    FOREIGN KEY (session_id) REFERENCES sessions(id),
    FOREIGN KEY (launch_id) REFERENCES launches(id),
    FOREIGN KEY (observation_id) REFERENCES provider_observations(id),
    UNIQUE (session_id, provider_event_key),
    CHECK (event_type = 'MIGRATION' OR launch_id IS NOT NULL)
);

CREATE TABLE event_observations (
    id INTEGER PRIMARY KEY,
    event_id INTEGER NOT NULL,
    observation_id INTEGER NOT NULL,
    arrival_rank INTEGER NOT NULL CHECK (arrival_rank >= 1),
    FOREIGN KEY (event_id) REFERENCES events(id),
    FOREIGN KEY (observation_id) REFERENCES provider_observations(id),
    UNIQUE (observation_id),
    UNIQUE (event_id, arrival_rank)
);

CREATE TABLE data_quality_intervals (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    launch_id TEXT,
    start_offset_ns INTEGER NOT NULL,
    end_offset_ns INTEGER,
    reason TEXT NOT NULL CHECK (reason IN ('SUBSCRIPTION_WARMUP', 'RECONNECT', 'SEQUENCE_GAP', 'STALE', 'DECODE_ERROR', 'SUBSCRIPTION_ERROR', 'FEED_BUDGET_STOP', 'STORAGE_LOSS', 'PROCESS_RESTART')),
    details_json TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id),
    FOREIGN KEY (launch_id) REFERENCES launches(id),
    CHECK (end_offset_ns IS NULL OR end_offset_ns >= start_offset_ns),
    CHECK (reason <> 'SUBSCRIPTION_WARMUP' OR launch_id IS NOT NULL)
);

CREATE TABLE feed_usage (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    utc_day TEXT NOT NULL,
    message_count INTEGER NOT NULL CHECK (message_count >= 0),
    paid_message_count INTEGER NOT NULL CHECK (paid_message_count >= 0),
    estimated_cost_lamports INTEGER NOT NULL CHECK (estimated_cost_lamports >= 0),
    budget_lamports INTEGER NOT NULL CHECK (budget_lamports >= 0),
    updated_at_utc_ns INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id),
    UNIQUE (session_id, utc_day)
);

CREATE TABLE health_samples (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    sampled_at_utc_ns INTEGER NOT NULL,
    sampled_at_offset_ns INTEGER NOT NULL,
    interval_ms INTEGER NOT NULL CHECK (interval_ms > 0),
    system_state TEXT NOT NULL CHECK (system_state IN ('STARTING_RECONCILIATION', 'RUNNING', 'ENTRY_HALT', 'EMERGENCY_EXIT', 'MANUAL_REVIEW_REQUIRED', 'STOPPED')),
    storage_state TEXT NOT NULL CHECK (storage_state IN ('OK', 'DEGRADED', 'CRITICAL')),
    eventloop_lag_p50_us INTEGER NOT NULL,
    eventloop_lag_p95_us INTEGER NOT NULL,
    eventloop_lag_p99_us INTEGER NOT NULL,
    receive_to_tracker_p50_us INTEGER,
    receive_to_tracker_p95_us INTEGER,
    receive_to_tracker_p99_us INTEGER,
    deadline_jitter_p50_us INTEGER,
    deadline_jitter_p95_us INTEGER,
    deadline_jitter_p99_us INTEGER,
    provider_connected INTEGER NOT NULL CHECK (provider_connected IN (0, 1)),
    provider_message_age_ms INTEGER,
    provider_reconnects_delta INTEGER NOT NULL CHECK (provider_reconnects_delta >= 0),
    provider_messages_delta INTEGER NOT NULL CHECK (provider_messages_delta >= 0),
    paid_messages_delta INTEGER NOT NULL CHECK (paid_messages_delta >= 0),
    feed_cost_lamports_delta INTEGER NOT NULL CHECK (feed_cost_lamports_delta >= 0),
    queue_bulk_depth INTEGER NOT NULL CHECK (queue_bulk_depth >= 0),
    queue_critical_depth INTEGER NOT NULL CHECK (queue_critical_depth >= 0),
    active_trackers INTEGER NOT NULL CHECK (active_trackers >= 0),
    memory_rss_bytes INTEGER NOT NULL CHECK (memory_rss_bytes >= 0),
    db_size_bytes INTEGER NOT NULL CHECK (db_size_bytes >= 0),
    wal_size_bytes INTEGER NOT NULL CHECK (wal_size_bytes >= 0),
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE TABLE backtest_runs (
    id TEXT PRIMARY KEY,
    strategy_version_id TEXT NOT NULL,
    dataset_sha256 TEXT NOT NULL,
    dataset_start_utc_ns INTEGER NOT NULL,
    dataset_end_utc_ns INTEGER NOT NULL,
    code_commit TEXT NOT NULL,
    contract_version TEXT NOT NULL,
    backtest_protocol_version TEXT NOT NULL,
    split_protocol_model_id TEXT NOT NULL,
    bootstrap_protocol_model_id TEXT NOT NULL,
    research_entry_model_id TEXT NOT NULL,
    random_seed TEXT NOT NULL,
    edge_state TEXT CHECK (edge_state IN ('NO_CANDIDATE', 'OOS_FAIL', 'INSUFFICIENT_EDGE_EVIDENCE', 'EDGE_CHECK_PASS')),
    non_evaluable_state TEXT CHECK (non_evaluable_state IN ('DATA_INCOMPLETE', 'PROTOCOL_UNFROZEN')),
    oos_opened INTEGER NOT NULL DEFAULT 0 CHECK (oos_opened IN (0, 1)),
    report_json_path TEXT,
    report_sha256 TEXT,
    trades_csv_path TEXT,
    started_at_utc_ns INTEGER NOT NULL,
    completed_at_utc_ns INTEGER,
    FOREIGN KEY (strategy_version_id) REFERENCES strategy_versions(id),
    FOREIGN KEY (split_protocol_model_id) REFERENCES model_versions(id),
    FOREIGN KEY (bootstrap_protocol_model_id) REFERENCES model_versions(id),
    FOREIGN KEY (research_entry_model_id) REFERENCES model_versions(id)
);

CREATE TABLE backtest_splits (
    id INTEGER PRIMARY KEY,
    backtest_run_id TEXT NOT NULL,
    split_name TEXT NOT NULL CHECK (split_name IN ('TRAIN', 'VALIDATION', 'OOS', 'PAPER_PARITY')),
    start_utc_ns INTEGER NOT NULL,
    end_utc_ns INTEGER NOT NULL,
    launches_total INTEGER NOT NULL CHECK (launches_total >= 0),
    launches_evaluable INTEGER NOT NULL CHECK (launches_evaluable >= 0),
    launches_excluded_json TEXT NOT NULL,
    completed_trades INTEGER NOT NULL CHECK (completed_trades >= 0),
    min_required_trades INTEGER NOT NULL CHECK (min_required_trades >= 0),
    metrics_json TEXT NOT NULL,
    FOREIGN KEY (backtest_run_id) REFERENCES backtest_runs(id),
    UNIQUE (backtest_run_id, split_name),
    CHECK (end_utc_ns > start_utc_ns)
);

CREATE TABLE candidate_evaluations (
    id INTEGER PRIMARY KEY,
    backtest_run_id TEXT NOT NULL,
    config_hash TEXT NOT NULL,
    params_json TEXT NOT NULL,
    validation_metrics_json TEXT NOT NULL,
    worst_stress_ci_lower_lamports INTEGER,
    eligible INTEGER NOT NULL CHECK (eligible IN (0, 1)),
    rejection_reason TEXT,
    rank INTEGER,
    selected INTEGER NOT NULL DEFAULT 0 CHECK (selected IN (0, 1)),
    evaluated_at_utc_ns INTEGER NOT NULL,
    FOREIGN KEY (backtest_run_id) REFERENCES backtest_runs(id),
    UNIQUE (backtest_run_id, config_hash)
);

CREATE TABLE oos_locks (
    id INTEGER PRIMARY KEY,
    dataset_sha256 TEXT NOT NULL,
    oos_start_utc_ns INTEGER NOT NULL,
    oos_end_utc_ns INTEGER NOT NULL,
    backtest_run_id TEXT NOT NULL,
    opened_at_utc_ns INTEGER NOT NULL,
    opened_by TEXT NOT NULL,
    FOREIGN KEY (backtest_run_id) REFERENCES backtest_runs(id),
    UNIQUE (dataset_sha256, oos_start_utc_ns, oos_end_utc_ns)
);

CREATE TABLE feature_snapshots (
    id INTEGER PRIMARY KEY,
    launch_id TEXT NOT NULL,
    session_id TEXT,
    backtest_run_id TEXT,
    feature_set_model_id TEXT NOT NULL,
    computed_in TEXT NOT NULL CHECK (computed_in IN ('LIVE', 'REPLAY')),
    decision_deadline_offset_ns INTEGER NOT NULL,
    coverage_state TEXT NOT NULL CHECK (coverage_state IN ('UNKNOWN_WARMUP', 'OBSERVED', 'GAP')),
    coverage_start_offset_ns INTEGER,
    features_json TEXT NOT NULL,
    features_sha256 TEXT NOT NULL,
    data_complete INTEGER NOT NULL CHECK (data_complete IN (0, 1)),
    FOREIGN KEY (launch_id) REFERENCES launches(id),
    FOREIGN KEY (session_id) REFERENCES sessions(id),
    FOREIGN KEY (backtest_run_id) REFERENCES backtest_runs(id),
    FOREIGN KEY (feature_set_model_id) REFERENCES model_versions(id),
    CHECK ((session_id IS NULL) <> (backtest_run_id IS NULL)),
    CHECK ((computed_in = 'LIVE' AND session_id IS NOT NULL) OR (computed_in = 'REPLAY' AND backtest_run_id IS NOT NULL))
);

CREATE TABLE decisions (
    id TEXT PRIMARY KEY,
    session_id TEXT,
    backtest_run_id TEXT,
    launch_id TEXT NOT NULL,
    strategy_version_id TEXT NOT NULL,
    feature_snapshot_id INTEGER NOT NULL,
    decision_deadline_offset_ns INTEGER NOT NULL,
    decided_at_monotonic_ns INTEGER,
    action TEXT NOT NULL CHECK (action IN ('PASS', 'REJECT_FILTER', 'REJECT_RISK', 'DATA_INCOMPLETE')),
    first_failed_filter TEXT,
    filter_evaluations_json TEXT NOT NULL,
    risk_evaluation_json TEXT NOT NULL,
    data_quality_flags_json TEXT NOT NULL,
    configuration_hash TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id),
    FOREIGN KEY (backtest_run_id) REFERENCES backtest_runs(id),
    FOREIGN KEY (launch_id) REFERENCES launches(id),
    FOREIGN KEY (strategy_version_id) REFERENCES strategy_versions(id),
    FOREIGN KEY (feature_snapshot_id) REFERENCES feature_snapshots(id),
    CHECK ((session_id IS NULL) <> (backtest_run_id IS NULL))
);

CREATE TABLE risk_reservations (
    id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL UNIQUE,
    simulated INTEGER NOT NULL CHECK (simulated IN (0, 1)),
    state TEXT NOT NULL CHECK (state IN ('RESERVED', 'CONVERTED_TO_POSITION', 'RELEASED')),
    quote_lamports INTEGER NOT NULL CHECK (quote_lamports >= 0),
    fee_budget_lamports INTEGER NOT NULL CHECK (fee_budget_lamports >= 0),
    reserved_at_utc_ns INTEGER NOT NULL,
    released_at_utc_ns INTEGER,
    release_reason TEXT,
    FOREIGN KEY (decision_id) REFERENCES decisions(id),
    CHECK ((state = 'RELEASED') = (released_at_utc_ns IS NOT NULL))
);

CREATE TABLE orders (
    id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL,
    risk_reservation_id TEXT NOT NULL,
    simulated INTEGER NOT NULL CHECK (simulated IN (0, 1)),
    side TEXT NOT NULL CHECK (side IN ('BUY', 'SELL')),
    mint TEXT NOT NULL,
    venue TEXT NOT NULL CHECK (venue IN ('PUMP_CURVE', 'PUMPSWAP')),
    input_mint TEXT NOT NULL,
    output_mint TEXT NOT NULL,
    input_amount_base_units INTEGER NOT NULL CHECK (input_amount_base_units > 0),
    limit_amount_base_units INTEGER NOT NULL CHECK (limit_amount_base_units >= 0),
    max_slippage_bps INTEGER NOT NULL CHECK (max_slippage_bps >= 0),
    state TEXT NOT NULL CHECK (state IN ('BUY_RESERVED', 'BUY_SIGNED', 'BUY_INTENT_DURABLE', 'BUY_SUBMITTED', 'BUY_PROCESSED', 'BUY_CONFIRMED', 'BUY_FAILED', 'BUY_EXPIRED', 'BUY_UNKNOWN_RECONCILE', 'SELL_SIGNED', 'SELL_INTENT_DURABLE', 'SELL_SUBMITTED', 'SELL_PROCESSED', 'SELL_CONFIRMED', 'SELL_FAILED', 'SELL_EXPIRED', 'SELL_UNKNOWN_RECONCILE')),
    created_at_utc_ns INTEGER NOT NULL,
    FOREIGN KEY (decision_id) REFERENCES decisions(id),
    FOREIGN KEY (risk_reservation_id) REFERENCES risk_reservations(id),
    CHECK ((side = 'BUY' AND state LIKE 'BUY_%') OR (side = 'SELL' AND state LIKE 'SELL_%'))
);

CREATE TABLE order_attempts (
    id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL,
    attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
    simulated INTEGER NOT NULL CHECK (simulated IN (0, 1)),
    signature TEXT,
    blockhash TEXT,
    last_valid_block_height INTEGER,
    serialized_transaction BLOB,
    decision_monotonic_ns INTEGER,
    signed_monotonic_ns INTEGER,
    intent_durable_monotonic_ns INTEGER,
    submitted_monotonic_ns INTEGER,
    confirmed_monotonic_ns INTEGER,
    landed_slot INTEGER,
    state TEXT NOT NULL CHECK (state IN ('BUY_RESERVED', 'BUY_SIGNED', 'BUY_INTENT_DURABLE', 'BUY_SUBMITTED', 'BUY_PROCESSED', 'BUY_CONFIRMED', 'BUY_FAILED', 'BUY_EXPIRED', 'BUY_UNKNOWN_RECONCILE', 'SELL_SIGNED', 'SELL_INTENT_DURABLE', 'SELL_SUBMITTED', 'SELL_PROCESSED', 'SELL_CONFIRMED', 'SELL_FAILED', 'SELL_EXPIRED', 'SELL_UNKNOWN_RECONCILE')),
    meta_error_json TEXT,
    base_fee_lamports INTEGER,
    priority_fee_lamports INTEGER,
    tip_lamports INTEGER,
    sol_delta_lamports INTEGER,
    token_delta_base_units INTEGER,
    reconciled_at_utc_ns INTEGER,
    FOREIGN KEY (order_id) REFERENCES orders(id),
    UNIQUE (order_id, attempt_number),
    UNIQUE (signature),
    CHECK (simulated = 1 OR state NOT IN ('BUY_SUBMITTED', 'BUY_PROCESSED', 'BUY_CONFIRMED', 'SELL_SUBMITTED', 'SELL_PROCESSED', 'SELL_CONFIRMED') OR signature IS NOT NULL)
);

CREATE TABLE positions (
    id TEXT PRIMARY KEY,
    launch_id TEXT NOT NULL,
    strategy_version_id TEXT NOT NULL,
    simulated INTEGER NOT NULL CHECK (simulated IN (0, 1)),
    buy_order_id TEXT NOT NULL,
    sell_order_id TEXT,
    state TEXT NOT NULL CHECK (state IN ('PENDING_BUY', 'OPEN', 'EXITING', 'CLOSED', 'UNKNOWN_RECONCILE')),
    venue_at_entry TEXT NOT NULL CHECK (venue_at_entry IN ('PUMP_CURVE', 'PUMPSWAP')),
    venue_at_exit TEXT CHECK (venue_at_exit IN ('PUMP_CURVE', 'PUMPSWAP')),
    token_amount_base_units INTEGER NOT NULL CHECK (token_amount_base_units >= 0),
    entry_cost_lamports INTEGER NOT NULL CHECK (entry_cost_lamports >= 0),
    cost_breakdown_json TEXT NOT NULL,
    exit_value_lamports INTEGER,
    net_pnl_lamports INTEGER,
    exit_reason TEXT CHECK (exit_reason IN ('TARGET', 'STOP', 'TIMEOUT', 'MIGRATION_EXIT', 'EMERGENCY', 'UNRESOLVED_ZERO')),
    opened_at_utc_ns INTEGER NOT NULL,
    closed_at_utc_ns INTEGER,
    FOREIGN KEY (launch_id) REFERENCES launches(id),
    FOREIGN KEY (strategy_version_id) REFERENCES strategy_versions(id),
    FOREIGN KEY (buy_order_id) REFERENCES orders(id),
    FOREIGN KEY (sell_order_id) REFERENCES orders(id),
    UNIQUE (buy_order_id),
    CHECK ((state = 'CLOSED') = (closed_at_utc_ns IS NOT NULL)),
    CHECK (state <> 'CLOSED' OR (exit_reason IS NOT NULL AND net_pnl_lamports IS NOT NULL))
);

CREATE TABLE outcomes (
    id INTEGER PRIMARY KEY,
    launch_id TEXT NOT NULL,
    backtest_run_id TEXT,
    research_entry_model_id TEXT NOT NULL,
    research_horizon_ms INTEGER NOT NULL CHECK (research_horizon_ms > 0),
    horizon_cohort TEXT NOT NULL,
    outcome_json TEXT NOT NULL,
    migration_resolved INTEGER CHECK (migration_resolved IN (0, 1)),
    lower_bound_migration_loss INTEGER NOT NULL DEFAULT 0 CHECK (lower_bound_migration_loss IN (0, 1)),
    completed_at_utc_ns INTEGER NOT NULL,
    FOREIGN KEY (launch_id) REFERENCES launches(id),
    FOREIGN KEY (backtest_run_id) REFERENCES backtest_runs(id),
    FOREIGN KEY (research_entry_model_id) REFERENCES model_versions(id)
);

CREATE TABLE system_events (
    id INTEGER PRIMARY KEY,
    session_id TEXT,
    event_type TEXT NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('INFO', 'WARN', 'CRITICAL')),
    utc_ns INTEGER NOT NULL,
    details_json TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

-- Partial unique indexes: SQLite treats NULL as distinct inside UNIQUE constraints, so
-- LIVE rows (session_id set) and REPLAY rows (backtest_run_id set) get their own key each.
CREATE UNIQUE INDEX ux_feature_snapshots_live
    ON feature_snapshots (launch_id, feature_set_model_id, decision_deadline_offset_ns)
    WHERE computed_in = 'LIVE';
CREATE UNIQUE INDEX ux_feature_snapshots_replay
    ON feature_snapshots (launch_id, feature_set_model_id, decision_deadline_offset_ns, backtest_run_id)
    WHERE computed_in = 'REPLAY';
CREATE UNIQUE INDEX ux_decisions_live
    ON decisions (launch_id, strategy_version_id, decision_deadline_offset_ns, session_id)
    WHERE session_id IS NOT NULL;
CREATE UNIQUE INDEX ux_decisions_replay
    ON decisions (launch_id, strategy_version_id, decision_deadline_offset_ns, backtest_run_id)
    WHERE backtest_run_id IS NOT NULL;
CREATE UNIQUE INDEX ux_outcomes_live
    ON outcomes (launch_id, research_entry_model_id)
    WHERE backtest_run_id IS NULL;
CREATE UNIQUE INDEX ux_outcomes_replay
    ON outcomes (launch_id, research_entry_model_id, backtest_run_id)
    WHERE backtest_run_id IS NOT NULL;

CREATE INDEX idx_observations_replay
    ON provider_observations (persistent_session_sequence, receive_offset_ns_from_session_start, provider_receive_sequence);
CREATE INDEX idx_events_replay
    ON events (persistent_session_sequence, receive_offset_ns_from_session_start, provider_receive_sequence);
CREATE INDEX idx_events_mint_replay
    ON events (mint, receive_offset_ns_from_session_start, provider_receive_sequence);
CREATE INDEX idx_events_launch
    ON events (launch_id, receive_offset_ns_from_session_start);
CREATE INDEX idx_event_observations_event
    ON event_observations (event_id, arrival_rank);
CREATE INDEX idx_launches_session_t0
    ON launches (session_id, t0_receive_offset_ns);
CREATE INDEX idx_launches_status
    ON launches (status);
CREATE INDEX idx_data_quality_session
    ON data_quality_intervals (session_id, start_offset_ns);
CREATE INDEX idx_data_quality_launch
    ON data_quality_intervals (launch_id, start_offset_ns);
CREATE INDEX idx_health_samples_session
    ON health_samples (session_id, sampled_at_offset_ns);
CREATE INDEX idx_feature_snapshots_launch
    ON feature_snapshots (launch_id, decision_deadline_offset_ns);
CREATE INDEX idx_decisions_launch_deadline
    ON decisions (launch_id, decision_deadline_offset_ns);
CREATE INDEX idx_decisions_run
    ON decisions (backtest_run_id, action);
CREATE INDEX idx_order_attempts_state
    ON order_attempts (state);
CREATE INDEX idx_positions_state
    ON positions (state);
CREATE INDEX idx_outcomes_launch
    ON outcomes (launch_id);
CREATE INDEX idx_system_events_time
    ON system_events (utc_ns);

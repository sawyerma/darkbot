-- DarkBot Phase-1 schema (SPEC 1.3.2 database_specification). Legacy contract schema kept unmodified under migrations/legacy/ (table-name collision: positions/decisions/feature_snapshots); new campaign DBs use this schema only.
-- Types: UTC ISO-8601 TEXT, monotonic INTEGER ns (one clock epoch per run), money/tokens INTEGER base units,
-- multiples TEXT decimal strings, JSON as canonical TEXT, bools 0/1. NULL money = UNKNOWN, never 0.
CREATE TABLE IF NOT EXISTS phase1_schema_version (
    version INTEGER PRIMARY KEY,
    spec_version TEXT NOT NULL,
    applied_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS config_snapshots (
    config_hash TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    canonical_json TEXT NOT NULL,
    created_utc TEXT NOT NULL,
    source TEXT NOT NULL,
    valid_from_slot INTEGER,          -- NULL: not slot-bound
    verified_utc TEXT                 -- NULL: not externally verified
);

CREATE TABLE IF NOT EXISTS runs (
    run_id TEXT PRIMARY KEY,
    campaign_id TEXT NOT NULL,
    mode TEXT NOT NULL,
    started_utc TEXT NOT NULL,
    ended_utc TEXT,                   -- NULL while running / after crash (marked INTERRUPTED on next open)
    code_revision TEXT NOT NULL,
    spec_hash TEXT NOT NULL,
    config_hash TEXT NOT NULL REFERENCES config_snapshots(config_hash) ON DELETE RESTRICT,
    protocol_snapshot_id TEXT REFERENCES config_snapshots(config_hash) ON DELETE RESTRICT,
    host_clock_epoch_id TEXT NOT NULL,
    status TEXT NOT NULL,
    quality_status TEXT NOT NULL,
    start_balance_lamports INTEGER NOT NULL,
    end_balance_lamports INTEGER,     -- NULL until run end
    parent_run_id TEXT,               -- replay/research source run
    dataset_hash TEXT,
    readiness TEXT
);

CREATE TABLE IF NOT EXISTS market_events (
    canonical_event_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    signature TEXT NOT NULL,
    instruction_index INTEGER NOT NULL,
    slot INTEGER,                     -- NULL when provider gave none
    mint TEXT NOT NULL,
    event_type TEXT NOT NULL,
    actor TEXT,
    creator TEXT,
    token_program TEXT,
    virtual_sol_reserves INTEGER,     -- NULL: not present in event (never 0 substitute)
    virtual_token_reserves INTEGER,
    real_sol_reserves INTEGER,
    real_token_reserves INTEGER,
    first_seen_mono_ns INTEGER NOT NULL,
    actionable_mono_ns INTEGER,       -- NULL: never actionable
    winner_feed_id TEXT NOT NULL,
    source_observation_id TEXT NOT NULL REFERENCES feed_observations(observation_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    commitment TEXT,
    quality_status TEXT NOT NULL,
    quote_mint_raw TEXT,
    quote_field_origin TEXT,
    quote_layout_hash TEXT,
    normalized_quote_asset TEXT,
    quote_scope_status TEXT,
    venue_state TEXT NOT NULL,
    canonical_coverage_state TEXT NOT NULL,
    UNIQUE(run_id, signature, instruction_index, mint, event_type)
);

CREATE TABLE IF NOT EXISTS feed_observations (
    observation_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    feed_id TEXT NOT NULL,
    connection_id TEXT NOT NULL,
    receive_mono_ns INTEGER NOT NULL,
    receive_utc TEXT NOT NULL,
    decode_done_mono_ns INTEGER,
    slot INTEGER,
    signature TEXT,
    instruction_index INTEGER,
    mint TEXT,
    event_type TEXT,
    canonical_event_id TEXT REFERENCES market_events(canonical_event_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    completeness TEXT NOT NULL,
    validation_status TEXT NOT NULL,
    raw_payload TEXT,                 -- stored once per raw frame (first row), NULL on sibling rows
    raw_payload_hash TEXT NOT NULL,   -- computed by the writer thread
    arrival_sequence INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_feed_obs_event ON feed_observations(run_id, canonical_event_id, feed_id, receive_mono_ns);
CREATE INDEX IF NOT EXISTS idx_feed_obs_time ON feed_observations(run_id, receive_mono_ns);
CREATE INDEX IF NOT EXISTS idx_market_events_mint ON market_events(run_id, mint, first_seen_mono_ns);

CREATE TABLE IF NOT EXISTS fee_snapshots (
    fee_snapshot_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    source_type TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    source_method TEXT NOT NULL,
    source_account_addresses TEXT NOT NULL,
    source_slot INTEGER,
    observed_utc TEXT NOT NULL,
    received_mono_ns INTEGER NOT NULL,
    commitment TEXT,
    protocol_program_id TEXT,
    idl_version_or_hash TEXT,
    raw_values TEXT NOT NULL,
    normalized_fee_components TEXT NOT NULL,
    rounding_rules TEXT NOT NULL,
    applicability TEXT NOT NULL,
    freshness_policy_hash TEXT NOT NULL,
    validation_status TEXT NOT NULL,
    config_hash TEXT NOT NULL REFERENCES config_snapshots(config_hash) ON DELETE RESTRICT,
    snapshot_hash TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS research_variants (
    variant_id TEXT PRIMARY KEY,
    campaign_id TEXT NOT NULL,
    parent_config_hash TEXT NOT NULL REFERENCES config_snapshots(config_hash) ON DELETE RESTRICT,
    variant_config_hash TEXT NOT NULL REFERENCES config_snapshots(config_hash) ON DELETE RESTRICT,
    dataset_id TEXT NOT NULL,
    split TEXT NOT NULL,
    registered_utc TEXT NOT NULL,
    seed INTEGER NOT NULL,
    selection_status TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS decisions (
    decision_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    mint TEXT NOT NULL,
    event_id TEXT REFERENCES market_events(canonical_event_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    policy_hash TEXT NOT NULL REFERENCES config_snapshots(config_hash) ON DELETE RESTRICT,
    receive_mono_ns INTEGER,
    data_ready_mono_ns INTEGER,
    decision_done_mono_ns INTEGER NOT NULL,
    intent_enqueued_mono_ns INTEGER,  -- NULL when no intent was enqueued (SKIP)
    action TEXT NOT NULL,
    reason TEXT NOT NULL,
    position_id TEXT REFERENCES positions(position_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    quote_state_event_id TEXT REFERENCES market_events(canonical_event_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    quote_age_ns INTEGER,
    quote_snapshot_id TEXT REFERENCES quote_snapshots(quote_snapshot_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    arrival_sequence INTEGER
);

CREATE TABLE IF NOT EXISTS positions (
    position_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    variant_id TEXT REFERENCES research_variants(variant_id) ON DELETE RESTRICT,
    mint TEXT NOT NULL,
    creator TEXT,
    policy_hash TEXT NOT NULL REFERENCES config_snapshots(config_hash) ON DELETE RESTRICT,
    state TEXT NOT NULL,
    entry_decision_id TEXT NOT NULL REFERENCES decisions(decision_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    entry_fill_attempt_id TEXT REFERENCES execution_attempts(attempt_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    token_balance_base_units INTEGER NOT NULL,
    target_net_multiple TEXT NOT NULL,
    open_mono_ns INTEGER,
    close_mono_ns INTEGER,
    exit_reason TEXT,
    net_pnl_lamports INTEGER,         -- NULL while open/unresolved (never 0 substitute)
    quality_status TEXT NOT NULL,
    research_status TEXT NOT NULL,
    timeout_due_mono_ns INTEGER,
    research_end_mono_ns INTEGER,
    unresolved_reason TEXT,
    venue_state TEXT NOT NULL,
    canonical_provider_gap_flag INTEGER NOT NULL,
    entry_economic_cost_lamports INTEGER,
    locked_rent_lamports INTEGER NOT NULL DEFAULT 0,
    total_fees_lamports INTEGER NOT NULL DEFAULT 0,
    origin_trigger TEXT,
    entry_fill_mono_ns INTEGER
);

CREATE TABLE IF NOT EXISTS quote_snapshots (
    quote_snapshot_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    position_id TEXT REFERENCES positions(position_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    quote_state_event_id TEXT NOT NULL REFERENCES market_events(canonical_event_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    fee_snapshot_id TEXT NOT NULL REFERENCES fee_snapshots(fee_snapshot_id) ON DELETE RESTRICT,
    side TEXT NOT NULL,
    created_mono_ns INTEGER NOT NULL,
    knowledge_cutoff_mono_ns INTEGER NOT NULL,
    curve_input_lamports INTEGER,
    gross_curve_output_lamports INTEGER,
    token_amount_base_units INTEGER NOT NULL,
    protocol_fee_total_lamports INTEGER NOT NULL,
    swap_wallet_debit_lamports INTEGER,
    swap_wallet_credit_lamports INTEGER,
    network_fee_total_lamports INTEGER NOT NULL,
    tip_lamports INTEGER NOT NULL,
    rent_delta_lamports INTEGER NOT NULL,
    irrecoverable_cost_lamports INTEGER NOT NULL,
    slippage_limit_bps INTEGER NOT NULL,
    min_output_base_units INTEGER,
    max_input_base_units INTEGER,
    quote_age_ns INTEGER NOT NULL,
    quote_model_hash TEXT NOT NULL REFERENCES config_snapshots(config_hash) ON DELETE RESTRICT,
    rounding_version TEXT NOT NULL,
    quality_status TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS execution_attempts (
    attempt_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    position_id TEXT NOT NULL REFERENCES positions(position_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    side TEXT NOT NULL,
    attempt_number INTEGER NOT NULL,
    adapter TEXT NOT NULL,
    status TEXT NOT NULL,
    submit_mono_ns INTEGER NOT NULL,
    scheduled_fill_mono_ns INTEGER NOT NULL,
    callback_actual_mono_ns INTEGER,
    modeled_fill_mono_ns INTEGER,
    rpc_ack_mono_ns INTEGER,          -- LIVE only: NULL in PAPER
    chain_observed_mono_ns INTEGER,   -- LIVE only: NULL in PAPER
    transaction_signature TEXT,       -- LIVE only: NULL in PAPER
    model_hash TEXT NOT NULL REFERENCES config_snapshots(config_hash) ON DELETE RESTRICT,
    quote_state_event_id TEXT REFERENCES market_events(canonical_event_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    amount_in_base_units INTEGER,
    amount_out_base_units INTEGER,
    slippage_bps INTEGER NOT NULL,
    failure_reason TEXT,
    fees_charged_lamports INTEGER,    -- NULL = UNKNOWN status, never zero
    fee_snapshot_id TEXT REFERENCES fee_snapshots(fee_snapshot_id) ON DELETE RESTRICT,
    quote_snapshot_id TEXT REFERENCES quote_snapshots(quote_snapshot_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    message_template_hash TEXT,
    cost_model_hash TEXT NOT NULL REFERENCES config_snapshots(config_hash) ON DELETE RESTRICT,
    signature_count INTEGER,
    compute_unit_limit INTEGER,
    compute_unit_price_micro_lamports INTEGER,
    protocol_fee_lamports INTEGER,
    creator_fee_lamports INTEGER,
    other_protocol_fee_lamports INTEGER,
    base_fee_lamports INTEGER,
    priority_fee_lamports INTEGER,
    network_fee_total_lamports INTEGER,
    sender_tip_lamports INTEGER,
    rent_locked_lamports INTEGER,
    rent_refunded_lamports INTEGER,
    irrecoverable_account_cost_lamports INTEGER,
    cost_evidence_status TEXT NOT NULL,
    fee_reconciliation_status TEXT NOT NULL,
    entry_curve_input_lamports INTEGER,
    attempt_decision_id TEXT NOT NULL REFERENCES decisions(decision_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    fill_evidence_status TEXT NOT NULL,
    proxy_event_id TEXT REFERENCES market_events(canonical_event_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    proxy_signature TEXT,
    proxy_slot INTEGER,
    proxy_receive_sequence INTEGER,
    proxy_receive_mono_ns INTEGER,
    proxy_age_ns INTEGER,
    next_observed_event_id_after_modeled_fill TEXT REFERENCES market_events(canonical_event_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    fill_commitment TEXT,
    UNIQUE(run_id, position_id, side, attempt_number)
);

CREATE TABLE IF NOT EXISTS ledger (
    ledger_entry_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    position_id TEXT REFERENCES positions(position_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    attempt_id TEXT REFERENCES execution_attempts(attempt_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    mono_ns INTEGER NOT NULL,
    utc TEXT NOT NULL,
    kind TEXT NOT NULL,
    cash_delta_lamports INTEGER NOT NULL,
    locked_rent_delta_lamports INTEGER NOT NULL,
    token_delta_base_units INTEGER NOT NULL,
    economic_cost_lamports INTEGER NOT NULL,
    provenance TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS quality_events (
    quality_event_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    feed_id TEXT,
    start_mono_ns INTEGER NOT NULL,
    end_mono_ns INTEGER,
    kind TEXT NOT NULL,
    severity TEXT NOT NULL,
    scope TEXT NOT NULL,
    lost_records_count INTEGER,       -- NULL = unknown loss count
    details TEXT NOT NULL,
    canonical_gap INTEGER NOT NULL,
    coverage_version TEXT NOT NULL,
    recovery_status TEXT NOT NULL,
    evidence_observation_ids TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS reports (
    report_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    campaign_id TEXT NOT NULL,
    created_utc TEXT NOT NULL,
    query_version TEXT NOT NULL,
    dataset_hash TEXT NOT NULL,
    config_hash TEXT NOT NULL REFERENCES config_snapshots(config_hash) ON DELETE RESTRICT,
    report_json TEXT NOT NULL,
    qualification_status TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS feature_snapshots (
    feature_snapshot_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    mint TEXT NOT NULL,
    feature_set_id TEXT NOT NULL,
    feature_contract_hash TEXT NOT NULL REFERENCES config_snapshots(config_hash) ON DELETE RESTRICT,
    cutoff_mono_ns INTEGER NOT NULL,
    computed_mono_ns INTEGER NOT NULL,
    source_event_ids_json TEXT NOT NULL,
    features_json TEXT NOT NULL,
    null_reasons_json TEXT NOT NULL,
    coverage_version TEXT NOT NULL,
    quality_status TEXT NOT NULL,
    UNIQUE(run_id, mint, feature_set_id, cutoff_mono_ns)
);

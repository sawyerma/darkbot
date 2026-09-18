"""Domain types (LEAN_V1 section 4): static enums, row dataclasses of the active tables, events, write ops.

Enums and rows are static classes; test_domain proves that every enum equals the contract
enumeration (values and order) and every row's fields equal persistence.required_columns.
"""

from __future__ import annotations

import enum
import uuid
from dataclasses import dataclass, fields
from decimal import Decimal
from pathlib import Path
from typing import Any

from darkbot.cli import ActiveContract, ContractError, find_project_root, load_active_contract

NANOSECONDS_PER_SECOND = 1_000_000_000
NANOSECONDS_PER_MILLISECOND = 1_000_000
LAMPORTS_PER_SOL = 1_000_000_000


def load_contract(start: Path | None = None) -> ActiveContract:
    """Resolve the project root from `start` (a file or directory inside the project) and load the active contract."""
    root = find_project_root(start)
    active, errors = load_active_contract(root)
    if active is None or errors:
        raise ContractError("; ".join(errors) or "active contract unavailable")
    return active


# --------------------------------------------------------------------------- enums (static, tested against enumerations)


class Mode(str, enum.Enum):
    SHADOW = "SHADOW"
    BACKTEST = "BACKTEST"
    PAPER = "PAPER"
    LIVE = "LIVE"


class SystemState(str, enum.Enum):
    STARTING_RECONCILIATION = "STARTING_RECONCILIATION"
    RUNNING = "RUNNING"
    ENTRY_HALT = "ENTRY_HALT"
    EMERGENCY_EXIT = "EMERGENCY_EXIT"
    MANUAL_REVIEW_REQUIRED = "MANUAL_REVIEW_REQUIRED"
    STOPPED = "STOPPED"


class ParseStatus(str, enum.Enum):
    PARSED = "PARSED"
    IGNORED = "IGNORED"
    PARSE_ERROR = "PARSE_ERROR"


class QuoteScope(str, enum.Enum):
    SOL_WSOL = "SOL_WSOL"
    OBSERVE_ONLY = "OBSERVE_ONLY"


class LaunchStatus(str, enum.Enum):
    OBSERVING = "OBSERVING"
    DECIDED = "DECIDED"
    POSITION = "POSITION"
    RAW_TRACKING = "RAW_TRACKING"
    EVICTED = "EVICTED"


class CoverageState(str, enum.Enum):
    UNKNOWN_WARMUP = "UNKNOWN_WARMUP"
    OBSERVED = "OBSERVED"
    GAP = "GAP"


class EventType(str, enum.Enum):
    CREATE = "CREATE"
    BUY = "BUY"
    SELL = "SELL"
    MIGRATION = "MIGRATION"


class DataQualityReason(str, enum.Enum):
    SUBSCRIPTION_WARMUP = "SUBSCRIPTION_WARMUP"
    RECONNECT = "RECONNECT"
    SEQUENCE_GAP = "SEQUENCE_GAP"
    STALE = "STALE"
    DECODE_ERROR = "DECODE_ERROR"
    SUBSCRIPTION_ERROR = "SUBSCRIPTION_ERROR"
    FEED_BUDGET_STOP = "FEED_BUDGET_STOP"
    STORAGE_LOSS = "STORAGE_LOSS"
    PROCESS_RESTART = "PROCESS_RESTART"


class SnapshotOrigin(str, enum.Enum):
    LIVE = "LIVE"
    REPLAY = "REPLAY"


class DecisionAction(str, enum.Enum):
    PASS = "PASS"
    REJECT_FILTER = "REJECT_FILTER"
    REJECT_RISK = "REJECT_RISK"
    DATA_INCOMPLETE = "DATA_INCOMPLETE"


class Severity(str, enum.Enum):
    INFO = "INFO"
    WARN = "WARN"
    CRITICAL = "CRITICAL"


class StorageState(str, enum.Enum):
    OK = "OK"
    DEGRADED = "DEGRADED"
    CRITICAL = "CRITICAL"


class ModelKind(str, enum.Enum):
    COST = "COST"
    LATENCY = "LATENCY"
    VENUE_TRANSITION = "VENUE_TRANSITION"
    FEATURE_SET = "FEATURE_SET"
    RISK_POLICY = "RISK_POLICY"
    SPLIT_PROTOCOL = "SPLIT_PROTOCOL"
    BOOTSTRAP_PROTOCOL = "BOOTSTRAP_PROTOCOL"
    RESEARCH_ENTRY = "RESEARCH_ENTRY"
    PROTOCOL_CONFIG = "PROTOCOL_CONFIG"
    FEED_PRICING = "FEED_PRICING"
    OPERATIONAL_LIMITS = "OPERATIONAL_LIMITS"


class StrategyVersionState(str, enum.Enum):
    DRAFT = "DRAFT"
    VALIDATED = "VALIDATED"
    FROZEN = "FROZEN"
    ACTIVE = "ACTIVE"
    RETIRED = "RETIRED"


ENUM_CONTRACT_NAMES: dict[type[enum.Enum], str] = {
    Mode: "modes",
    SystemState: "system_states",
    ParseStatus: "parse_statuses",
    QuoteScope: "quote_scopes",
    LaunchStatus: "launch_statuses",
    CoverageState: "coverage_states",
    EventType: "event_types",
    DataQualityReason: "data_quality_reasons",
    SnapshotOrigin: "feature_snapshot_origins",
    DecisionAction: "decision_actions",
    Severity: "severities",
    StorageState: "storage_states",
    ModelKind: "model_kinds",
    StrategyVersionState: "strategy_version_states",
}


# --------------------------------------------------------------------------- rows (fields == required_columns)


@dataclass(frozen=True, slots=True)
class SessionRow:
    id: str
    persistent_session_sequence: int
    mode: str
    final_state: str | None
    contract_version: str
    code_commit: str
    provider_schema_version_id: str | None
    protocol_config_model_id: str | None
    protocol_config_read_slot: int | None
    protocol_config_read_utc_ns: int | None
    protocol_config_rpc_url_without_secret: str | None
    feed_pricing_model_id: str | None
    operational_limits_model_id: str | None
    active_strategy_version_id: str | None
    started_at_utc_ns: int
    started_at_monotonic_ns: int
    ended_at_utc_ns: int | None
    data_complete: int


@dataclass(frozen=True, slots=True)
class ProviderObservationRow:
    id: int
    session_id: str
    persistent_session_sequence: int
    provider_receive_sequence: int
    provider_received_monotonic_ns: int
    receive_offset_ns_from_session_start: int
    utc_received_ns: int
    chain_slot: int | None
    payload_sha256: str
    redacted_payload_json: str
    parse_status: str
    parse_error: str | None


@dataclass(frozen=True, slots=True)
class LaunchRow:
    id: str
    session_id: str
    mint: str
    symbol: str | None
    name: str | None
    creator: str | None
    pool: str | None
    quote_mint: str | None
    quote_scope: str
    status: str
    create_observation_id: int
    t0_receive_offset_ns: int
    accepted: int
    acceptance_reason: str
    curve_address_derived: str | None
    curve_address_provider: str | None
    curve_address_match: int | None
    initial_virtual_sol_reserves: int | None
    initial_virtual_token_reserves: int | None
    initial_real_token_reserves: int | None
    initial_buy_token_base_units: int | None
    initial_buy_quote_lamports: int | None
    assigned_horizon_ms: int
    horizon_cohort: str
    subscribe_sent_offset_ns: int | None
    subscribe_ack_offset_ns: int | None
    first_trade_received_offset_ns: int | None
    coverage_start_offset_ns: int | None
    coverage_state: str
    migrated: int
    migration_event_id: int | None
    data_complete: int


@dataclass(frozen=True, slots=True)
class EventRow:
    id: int
    session_id: str
    launch_id: str | None
    observation_id: int
    provider_event_key: str
    event_type: str
    mint: str
    transaction_signature: str | None
    provider_event_index: int | None
    actor: str | None
    quote_lamports: int | None
    token_base_units: int | None
    virtual_sol_reserves: int | None
    virtual_token_reserves: int | None
    persistent_session_sequence: int
    provider_receive_sequence: int
    receive_offset_ns_from_session_start: int
    utc_received_ns: int
    chain_slot: int | None
    late: int
    ambiguous: int
    data_complete: int


@dataclass(frozen=True, slots=True)
class DataQualityIntervalRow:
    id: int
    session_id: str
    launch_id: str | None
    start_offset_ns: int
    end_offset_ns: int | None
    reason: str
    details_json: str


@dataclass(frozen=True, slots=True)
class FeatureSnapshotRow:
    id: int
    launch_id: str
    session_id: str | None
    backtest_run_id: str | None
    feature_set_model_id: str
    computed_in: str
    decision_deadline_offset_ns: int
    coverage_state: str
    coverage_start_offset_ns: int | None
    features_json: str
    features_sha256: str
    data_complete: int


@dataclass(frozen=True, slots=True)
class DecisionRow:
    id: str
    session_id: str | None
    backtest_run_id: str | None
    launch_id: str
    strategy_version_id: str
    feature_snapshot_id: int
    decision_deadline_offset_ns: int
    decided_at_monotonic_ns: int | None
    action: str
    first_failed_filter: str | None
    filter_evaluations_json: str
    risk_evaluation_json: str
    data_quality_flags_json: str
    configuration_hash: str


@dataclass(frozen=True, slots=True)
class SystemEventRow:
    id: int
    session_id: str | None
    event_type: str
    severity: str
    utc_ns: int
    details_json: str


@dataclass(frozen=True, slots=True)
class ModelVersionRow:
    id: str
    kind: str
    content_json: str
    content_sha256: str
    source: str
    effective_from_utc_ns: int | None
    created_at_utc_ns: int


@dataclass(frozen=True, slots=True)
class StrategyVersionRow:
    id: str
    version_hash: str
    status: str
    candidate_name: str
    cloned_from_version_id: str | None
    feature_set_model_id: str | None
    risk_policy_model_id: str | None
    cost_model_id: str | None
    latency_model_id: str | None
    venue_transition_model_id: str | None
    configuration_json: str
    created_at_utc_ns: int
    frozen_at_utc_ns: int | None
    activated_at_utc_ns: int | None


@dataclass(frozen=True, slots=True)
class BacktestRunRow:
    id: str
    strategy_version_id: str
    dataset_sha256: str
    dataset_start_utc_ns: int
    dataset_end_utc_ns: int
    code_commit: str
    contract_version: str
    backtest_protocol_version: str
    split_protocol_model_id: str
    bootstrap_protocol_model_id: str
    research_entry_model_id: str
    random_seed: str
    edge_state: str | None
    non_evaluable_state: str | None
    oos_opened: int
    report_json_path: str | None
    report_sha256: str | None
    trades_csv_path: str | None
    started_at_utc_ns: int
    completed_at_utc_ns: int | None


ROW_TABLE: dict[type, str] = {
    SessionRow: "sessions",
    ProviderObservationRow: "provider_observations",
    LaunchRow: "launches",
    EventRow: "events",
    DataQualityIntervalRow: "data_quality_intervals",
    FeatureSnapshotRow: "feature_snapshots",
    DecisionRow: "decisions",
    SystemEventRow: "system_events",
    ModelVersionRow: "model_versions",
    StrategyVersionRow: "strategy_versions",
    BacktestRunRow: "backtest_runs",
}

INTEGER_PK_TABLES = ("provider_observations", "events", "data_quality_intervals", "feature_snapshots", "system_events")


def row_columns(row_type: type) -> tuple[str, ...]:
    return tuple(field.name for field in fields(row_type))


def new_text_id() -> str:
    return uuid.uuid4().hex


# --------------------------------------------------------------------------- write ops


@dataclass(frozen=True, slots=True)
class InsertOp:
    row: Any


@dataclass(frozen=True, slots=True)
class UpdateOp:
    table: str
    key: dict[str, int | str]
    values: dict[str, int | str | None]


WriteOp = InsertOp | UpdateOp


# --------------------------------------------------------------------------- events


@dataclass(frozen=True, slots=True)
class NormalizedEvent:
    """One CREATE/BUY/SELL event as the tracker and the replay see it (integers only, monotonic offsets)."""

    event_type: EventType
    mint: str
    signature: str
    event_index: int
    actor: str | None
    quote_lamports: int | None
    token_base_units: int | None
    virtual_sol_reserves: int | None
    virtual_token_reserves: int | None
    real_token_reserves: int | None
    receive_offset_ns: int
    utc_received_ns: int
    chain_slot: int | None
    receive_sequence: int
    name: str | None = None
    symbol: str | None = None
    creator: str | None = None
    bonding_curve: str | None = None
    token_total_supply: int | None = None

    @property
    def provider_event_key(self) -> str:
        return f"chain|{self.signature}|{self.event_type.value}|{self.mint}|{self.event_index}"


def feature_names(contract: dict[str, Any]) -> tuple[str, ...]:
    return tuple(contract["feature_contract_v1"]["definitions"])


def decimal_str(value: Decimal | None) -> str | None:
    return None if value is None else str(value)

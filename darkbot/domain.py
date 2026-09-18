"""Shared lean dataclasses and enums (spec simple_project_structure.domain.py).

No I/O, no strategy work, no imports of other DarkBot runtime modules.
Money and token amounts are integers in base units; multiples are Decimal.
Times: *_mono_ns are time.monotonic_ns values of ONE clock epoch; *_utc are ISO-8601 strings.
"""

from __future__ import annotations

import enum
from dataclasses import dataclass, field
from decimal import Decimal
from typing import Any

LAMPORTS_PER_SOL = 1_000_000_000
NS_PER_MS = 1_000_000
NS_PER_S = 1_000_000_000
BASIS_POINTS = 10_000


class Mode(str, enum.Enum):
    PAPER = "PAPER"
    REPLAY = "REPLAY"
    DOCTOR = "DOCTOR"
    LIVE = "LIVE"  # never runnable in phase 1


class EventType(str, enum.Enum):
    CREATE = "CREATE"
    BUY = "BUY"
    SELL = "SELL"
    COMPLETE = "COMPLETE"
    MIGRATE = "MIGRATE"
    HINT_CREATE = "HINT_CREATE"
    HINT_MIGRATION = "HINT_MIGRATION"


class Completeness(str, enum.Enum):
    COMPLETE = "COMPLETE"
    PARTIAL = "PARTIAL"
    HINT = "HINT"
    FAILED_TX = "FAILED_TX"
    UNDECODABLE = "UNDECODABLE"


class ValidationStatus(str, enum.Enum):
    VALID = "VALID"
    UNMATCHED = "UNMATCHED"
    DUPLICATE = "DUPLICATE"
    STALE = "STALE"
    CONFLICT = "CONFLICT"
    DECODE_ERROR = "DECODE_ERROR"
    UNKNOWN_VARIANT = "UNKNOWN_VARIANT"


class QuoteScopeStatus(str, enum.Enum):
    SOL_WSOL = "SOL_WSOL"
    UNSUPPORTED_QUOTE = "UNSUPPORTED_QUOTE"
    QUOTE_UNKNOWN = "QUOTE_UNKNOWN"
    QUOTE_UNKNOWN_LAYOUT_UNVERIFIED = "QUOTE_UNKNOWN_LAYOUT_UNVERIFIED"


class VenueState(str, enum.Enum):
    BONDING_CURVE_ACTIVE = "BONDING_CURVE_ACTIVE"
    CURVE_COMPLETE = "CURVE_COMPLETE"
    MIGRATION_SUSPECTED = "MIGRATION_SUSPECTED"
    MIGRATED_UNSUPPORTED = "MIGRATED_UNSUPPORTED"
    VENUE_UNKNOWN = "VENUE_UNKNOWN"


class CoverageState(str, enum.Enum):
    OBSERVED = "OBSERVED"
    UNKNOWN_WARMUP = "UNKNOWN_WARMUP"
    GAP = "GAP"
    RECOVERED_STATE_ONLY = "RECOVERED_STATE_ONLY"


class PositionState(str, enum.Enum):
    DETECTED = "DETECTED"
    INELIGIBLE = "INELIGIBLE"
    ENTRY_INTENT = "ENTRY_INTENT"
    ENTRY_PENDING = "ENTRY_PENDING"
    OPEN = "OPEN"
    EXIT_PENDING = "EXIT_PENDING"
    CLOSED = "CLOSED"
    ENTRY_FAILED = "ENTRY_FAILED"
    UNRESOLVED = "UNRESOLVED"


OCCUPIED_SLOT_STATES = (PositionState.ENTRY_INTENT, PositionState.ENTRY_PENDING, PositionState.OPEN, PositionState.EXIT_PENDING, PositionState.UNRESOLVED)


class ExitReason(str, enum.Enum):
    CREATOR_SELL = "CREATOR_SELL"
    TARGET = "TARGET"
    TIMEOUT = "TIMEOUT"


class DecisionAction(str, enum.Enum):
    ENTRY = "ENTRY"
    SKIP = "SKIP"
    EXIT = "EXIT"
    EXIT_RETRY = "EXIT_RETRY"
    ENTRY_CANCEL = "ENTRY_CANCEL"
    CANCEL_INTENT = "CANCEL_INTENT"


class Side(str, enum.Enum):
    BUY = "BUY"
    SELL = "SELL"


class AttemptStatus(str, enum.Enum):
    SUBMITTED = "SUBMITTED"
    FILLED = "FILLED"
    SLIPPAGE_REJECTED = "SLIPPAGE_REJECTED"
    CURVE_UNAVAILABLE = "CURVE_UNAVAILABLE"
    VENUE_INVALID = "VENUE_INVALID"
    QUOTE_STALE = "QUOTE_STALE"
    CANCELLED_BEFORE_SUBMIT = "CANCELLED_BEFORE_SUBMIT"
    UNKNOWN = "UNKNOWN"


class FillEvidence(str, enum.Enum):
    OBSERVED_STATE_PROXY = "OBSERVED_STATE_PROXY"
    CHAIN_EXECUTION_OBSERVED = "CHAIN_EXECUTION_OBSERVED"
    UNKNOWN = "UNKNOWN"
    NOT_APPLICABLE = "NOT_APPLICABLE"


class CostEvidence(str, enum.Enum):
    KNOWN_RULE = "KNOWN_RULE"
    CONFIGURED = "CONFIGURED"
    MODELED = "MODELED"
    MEASURED = "MEASURED"
    UNKNOWN = "UNKNOWN"
    VALID_ZERO = "VALID_ZERO"
    NOT_APPLICABLE = "NOT_APPLICABLE"


class QualificationStatus(str, enum.Enum):
    INSUFFICIENT_DATA = "INSUFFICIENT_DATA"
    UNQUALIFIED_DATA = "UNQUALIFIED_DATA"
    UNQUALIFIED_COSTS = "UNQUALIFIED_COSTS"
    NEGATIVE = "NEGATIVE"
    POSITIVE_UNDER_MODEL = "POSITIVE_UNDER_MODEL"


class Readiness(str, enum.Enum):
    NOT_READY = "NOT_READY"
    CAPTURE_READY = "CAPTURE_READY"
    ECONOMIC_PAPER_READY = "ECONOMIC_PAPER_READY"


# --------------------------------------------------------------------------- feed / market


@dataclass(frozen=True, slots=True)
class RawEnvelope:
    """One received feed message; receive_mono_ns is captured before parsing."""

    feed_id: str
    connection_id: str
    receive_mono_ns: int
    receive_utc: str
    arrival_sequence: int
    payload: str


@dataclass(frozen=True, slots=True)
class NormalizedEvent:
    event_type: EventType
    mint: str | None
    instruction_index: int
    actor: str | None = None
    creator: str | None = None
    sol_amount: int | None = None
    token_amount: int | None = None
    virtual_sol_reserves: int | None = None
    virtual_token_reserves: int | None = None
    real_sol_reserves: int | None = None
    real_token_reserves: int | None = None
    token_total_supply: int | None = None
    token_program: str | None = None
    quote_mint_raw: str | None = None
    quote_field_origin: str | None = None
    quote_layout_hash: str | None = None
    normalized_quote_asset: str | None = None
    quote_scope_status: QuoteScopeStatus | None = None
    extra: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class NormalizedEnvelope:
    signature: str | None
    slot: int | None
    success: bool | None
    events: tuple[NormalizedEvent, ...]
    completeness: Completeness
    provenance: str
    decode_done_mono_ns: int
    decoder_version: str
    error: str | None = None


@dataclass(slots=True)
class MarketState:
    """Per-mint RAM state; mutated only by the StateOwner through the market reducer."""

    mint: str
    creator: str | None = None
    create_event_id: str | None = None
    create_receive_mono_ns: int | None = None
    create_slot: int | None = None
    token_program: str | None = None
    quote_scope_status: QuoteScopeStatus = QuoteScopeStatus.QUOTE_UNKNOWN
    venue_state: VenueState = VenueState.VENUE_UNKNOWN
    coverage_state: CoverageState = CoverageState.UNKNOWN_WARMUP
    virtual_sol_reserves: int | None = None
    virtual_token_reserves: int | None = None
    real_sol_reserves: int | None = None
    real_token_reserves: int | None = None
    token_total_supply: int | None = None
    initial_real_token_reserves: int | None = None
    initial_virtual_token_reserves: int | None = None
    initial_virtual_sol_reserves: int | None = None
    initial_buy_tokens: int | None = None
    last_state_event_id: str | None = None
    last_state_slot: int | None = None
    last_state_sequence: int = 0
    last_state_receive_mono_ns: int | None = None
    creator_sell_event_id: str | None = None
    creator_sell_receive_mono_ns: int | None = None
    creator_sell_slot: int | None = None
    creator_sell_sequence: int | None = None
    unsupported_fee_mode: str | None = None
    creator_fee_bps: int = 0
    quote_uncertain: bool = False
    quote_conflict_mono_ns: int | None = None
    event_count: int = 0
    seen_keys: dict[str, tuple[tuple, bool]] = field(default_factory=dict)  # canonical id -> (content fingerprint, seen COMPLETE)
    create_completeness: Completeness | None = None
    decode_gap_since: int | None = None  # a PARTIAL/unknown Pump event touched this mint: state unknowable until a new COMPLETE state
    decode_gap_intervals: list[tuple[int, int | None]] = field(default_factory=list)
    trade_log_truncated: bool = False
    trade_log_truncated_from_mono_ns: int | None = None
    migration_evidence: dict[str, Any] = field(default_factory=dict)
    # bounded history of (receive_mono_ns, sequence, event_id, slot, vsol, vtok, rsol, rtok) for proxy selection at modeled fill time
    state_history: list[tuple[int, int, str, int | None, int, int, int | None, int | None]] = field(default_factory=list)
    # bounded trade log for background research features: (type, receive_mono_ns, actor, sol_amount, vsol, vtok, rtok, in_create_tx, event_id)
    trade_log: list[tuple[str, int, str, int, int | None, int | None, int | None, bool, str]] = field(default_factory=list)
    venue_change_mono_ns: int | None = None

    @property
    def curve_known(self) -> bool:
        return self.virtual_sol_reserves is not None and self.virtual_token_reserves is not None and self.virtual_sol_reserves > 0 and self.virtual_token_reserves > 0


@dataclass(frozen=True, slots=True)
class CurveState:
    """Immutable curve snapshot used by the QuoteEngine (OBSERVED_STATE_PROXY source)."""

    event_id: str
    signature: str | None
    slot: int | None
    receive_sequence: int
    receive_mono_ns: int
    virtual_sol_reserves: int
    virtual_token_reserves: int
    real_sol_reserves: int | None
    real_token_reserves: int | None
    venue_state: VenueState
    creator_fee_bps: int = 0
    creator_fee_applicable: bool = True


# --------------------------------------------------------------------------- fees / quotes


@dataclass(frozen=True, slots=True)
class FeeSnapshot:
    fee_snapshot_id: str
    source_type: str
    source_provider: str
    source_method: str
    source_account_addresses: tuple[str, ...]
    source_slot: int | None
    observed_utc: str
    received_mono_ns: int
    commitment: str | None
    protocol_program_id: str | None
    idl_version_or_hash: str | None
    raw_values: dict[str, Any]
    normalized_fee_components: dict[str, Any]
    rounding_rules: str
    applicability: str
    freshness_policy_hash: str
    validation_status: str
    config_hash: str
    max_age_ns: int
    evidence: CostEvidence

    def is_fresh(self, now_mono_ns: int) -> bool:
        return self.validation_status == "VALID" and now_mono_ns - self.received_mono_ns <= self.max_age_ns


@dataclass(frozen=True, slots=True)
class CostBasis:
    """Complete cost inputs valid at one point in time (all snapshots fresh) or None components -> UNKNOWN."""

    protocol: FeeSnapshot | None
    network: FeeSnapshot | None
    rent: FeeSnapshot | None
    priority_policy: FeeSnapshot | None

    @property
    def complete(self) -> bool:
        return all(s is not None for s in (self.protocol, self.network, self.rent, self.priority_policy))


@dataclass(frozen=True, slots=True)
class QuoteSnapshot:
    quote_snapshot_id: str
    side: Side
    quote_state_event_id: str
    fee_snapshot_id: str
    created_mono_ns: int
    knowledge_cutoff_mono_ns: int
    curve_input_lamports: int | None
    gross_curve_output_lamports: int | None
    token_amount_base_units: int
    protocol_fee_lamports: int
    creator_fee_lamports: int
    other_protocol_fee_lamports: int
    protocol_fee_total_lamports: int
    swap_wallet_debit_lamports: int | None
    swap_wallet_credit_lamports: int | None
    network_fee_total_lamports: int
    base_fee_lamports: int
    priority_fee_lamports: int
    tip_lamports: int
    rent_delta_lamports: int
    irrecoverable_cost_lamports: int
    slippage_limit_bps: int
    min_output_base_units: int | None
    max_input_base_units: int | None
    quote_age_ns: int
    quote_model_hash: str
    rounding_version: str
    quality_status: str
    signature_count: int
    compute_unit_limit: int
    compute_unit_price_micro_lamports: int
    cost_evidence_status: str
    unspent_budget_lamports: int | None = None
    capped_by_real_reserves: bool = False

    @property
    def entry_cash_debit(self) -> int:
        assert self.swap_wallet_debit_lamports is not None
        return self.swap_wallet_debit_lamports + self.network_fee_total_lamports + self.tip_lamports + self.rent_delta_lamports + self.irrecoverable_cost_lamports

    @property
    def entry_economic_cost(self) -> int:
        assert self.swap_wallet_debit_lamports is not None
        return self.swap_wallet_debit_lamports + self.network_fee_total_lamports + self.tip_lamports + self.irrecoverable_cost_lamports

    @property
    def sell_cash_credit(self) -> int:
        assert self.swap_wallet_credit_lamports is not None
        return self.swap_wallet_credit_lamports - self.network_fee_total_lamports - self.tip_lamports - self.rent_delta_lamports - self.irrecoverable_cost_lamports


@dataclass(frozen=True, slots=True)
class InvalidQuote:
    reason: str
    detail: str = ""


# --------------------------------------------------------------------------- intents / outcomes / ledger


@dataclass(frozen=True, slots=True)
class Intent:
    intent_id: str
    position_id: str
    mint: str
    side: Side
    attempt_number: int
    decision_id: str
    quote: QuoteSnapshot
    cost_basis_ids: dict[str, str]
    delay_ns: int
    decision_done_mono_ns: int
    origin_trigger: ExitReason | None = None


@dataclass(frozen=True, slots=True)
class AttemptHandle:
    attempt_id: str
    intent: Intent
    submit_mono_ns: int
    scheduled_fill_mono_ns: int


@dataclass(frozen=True, slots=True)
class ExecutionOutcome:
    attempt_id: str
    intent: Intent
    submit_mono_ns: int
    scheduled_fill_mono_ns: int
    callback_actual_mono_ns: int


@dataclass(frozen=True, slots=True)
class LedgerRecord:
    ledger_entry_id: str
    run_id: str
    position_id: str | None
    attempt_id: str | None
    mono_ns: int
    utc: str
    kind: str
    cash_delta_lamports: int
    locked_rent_delta_lamports: int
    token_delta_base_units: int
    economic_cost_lamports: int
    provenance: str


@dataclass(slots=True)
class PositionRecord:
    position_id: str
    run_id: str
    mint: str
    creator: str | None
    policy_hash: str
    state: PositionState
    entry_decision_id: str
    variant_id: str | None = None
    entry_fill_attempt_id: str | None = None
    token_balance_base_units: int = 0
    target_net_multiple: str = "1.5"
    open_mono_ns: int | None = None
    close_mono_ns: int | None = None
    exit_reason: ExitReason | None = None
    net_pnl_lamports: int | None = None
    quality_status: str = "OK"
    research_status: str = "OPEN"
    timeout_due_mono_ns: int | None = None
    research_end_mono_ns: int | None = None
    unresolved_reason: str | None = None
    venue_state: VenueState = VenueState.BONDING_CURVE_ACTIVE
    canonical_provider_gap_flag: bool = False
    # runtime-only bookkeeping (not persisted directly)
    entry_economic_cost_lamports: int = 0
    entry_cash_debit_lamports: int = 0
    locked_rent_lamports: int = 0
    reserved_cash_lamports: int = 0
    exit_reserve_lamports: int = 0
    failed_attempt_cost_lamports: int = 0
    exit_attempts: int = 0
    entry_attempts: int = 0
    origin_trigger: ExitReason | None = None
    latched_triggers: list[tuple[ExitReason, int]] = field(default_factory=list)
    exit_intent_pending: bool = False
    submitted_entry: bool = False
    exhausted: bool = False
    entry_fill_mono_ns: int | None = None
    total_fees_lamports: int = 0
    realized_credit_lamports: int = 0
    protocol_fee_lamports: int = 0
    network_fee_lamports: int = 0
    tip_lamports: int = 0
    open_intent_id: str | None = None
    retry_not_before_mono_ns: int | None = None


def decimal_str(value: Decimal | None) -> str | None:
    return None if value is None else format(value, "f")


def utc_iso(utc_ns: int) -> str:
    import datetime as _dt

    seconds, nanos = divmod(utc_ns, NS_PER_S)
    base = _dt.datetime.fromtimestamp(seconds, tz=_dt.timezone.utc)
    return base.strftime("%Y-%m-%dT%H:%M:%S") + f".{nanos // 1000:06d}Z"

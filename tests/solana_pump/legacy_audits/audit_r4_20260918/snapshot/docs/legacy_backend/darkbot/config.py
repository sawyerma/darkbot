"""Typed configuration: operational Limits and StrategyConfig (LEAN_V1 section 6).

No strategy, risk, cost or operational value appears as a literal anywhere else in the
runtime. Limits come from the contract's operational_limits_v1 initial values plus the
chain-specific values; StrategyConfig comes from a JSON file and is validated against the
contract's ranges. Both are frozen and hashed (sha256 of canonical JSON) so that every
session and every backtest references exactly one configuration.
"""

from __future__ import annotations

import os
from dataclasses import asdict, dataclass, fields
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Any

from darkbot.cli import canonical_json, load_json, sha256_json

ENV_CHAIN_WS_URL = "DARKBOT_CHAIN_WS_URL"
ENV_PUMPPORTAL_WS_URL = "DARKBOT_PUMPPORTAL_WS_URL"
COMMITMENT_LEVELS = ("processed", "confirmed")
WEBSOCKET_SCHEMES = ("ws://", "wss://")
BORSH_TAIL_TYPES = ("bool", "u64", "i64", "pubkey", "string")
WEBSOCKET_MAX_MESSAGE_BYTES = 32 * 1024 * 1024  # transport buffer bound for log notifications (technical constant, not a trading value)


class ConfigError(ValueError):
    """A configuration value is missing, mistyped or outside the contract's range."""


# --------------------------------------------------------------------------- operational limits


@dataclass(frozen=True, slots=True)
class Limits:
    """Operational values (never trading values). Source: contract operational_limits_v1 + chain settings."""

    bulk_batch_max_rows: int
    bulk_flush_interval_ms: int
    queue_bulk_warning_rows: int
    queue_bulk_critical_rows: int
    queue_bulk_hard_limit_rows: int
    critical_ack_timeout_ms: int
    busy_timeout_ms: int
    wal_autocheckpoint_pages: int
    tracker_limit: int
    stream_stale_timeout_ms: int
    reconnect_backoff_initial_ms: int
    reconnect_backoff_max_ms: int
    websocket_ping_interval_ms: int
    health_sample_interval_ms: int
    websocket_max_message_bytes: int
    chain_program_id: str
    chain_commitment: str
    chain_ws_url: str
    pumpportal_ws_url: str | None
    research_horizon_ms: int
    runtime_snapshot_ms: tuple[int, ...]
    observation_ms: int
    synchronous: str
    create_event_tail_layout: tuple[tuple[str, str], ...]
    known_pump_event_names: tuple[str, ...]

    def contract_limits_content(self, contract: dict[str, Any]) -> dict[str, int]:
        """The OPERATIONAL_LIMITS model content: exactly the contract's keys, taken from this instance."""
        keys = list(contract["operational_limits_v1"]["keys"])
        own = asdict(self)
        return {key: own[key] if key in own else int(contract["operational_limits_v1"]["keys"][key]["initial"]) for key in keys}


def _websocket_url(value: str | None, env_name: str, flag: str) -> str:
    url = value or os.environ.get(env_name)
    if not url:
        raise ConfigError(f"websocket url missing: pass {flag} or set {env_name}")
    if not url.startswith(WEBSOCKET_SCHEMES):
        raise ConfigError(f"{flag}: {url!r} is not a websocket endpoint (expected ws:// or wss://)")
    return url


def _tail_layout(contract: dict[str, Any]) -> tuple[tuple[str, str], ...]:
    """Fields of the CreateEvent after token_total_supply, pinned in the contract once a real capture proved them.

    An empty layout means unverified: the decoder keeps the tail as raw bytes and the quote mint stays unknown,
    which makes every launch OBSERVE_ONLY (data is collected, nothing is tradable) until the layout is pinned.
    """
    layout = contract["venue_contract"].get("protocol_definition_v1", {}).get("event_layouts", {}).get("CreateEvent", {}).get("tail", [])
    fields: list[tuple[str, str]] = []
    for entry in layout:
        if not (isinstance(entry, list) and len(entry) == 2 and entry[1] in BORSH_TAIL_TYPES):
            raise ConfigError(f"CreateEvent tail layout entry {entry!r} must be [field_name, type in {BORSH_TAIL_TYPES}]")
        fields.append((str(entry[0]), str(entry[1])))
    return tuple(fields)


def load_limits(contract: dict[str, Any], chain_ws_url: str | None = None, commitment: str | None = None, pumpportal_ws_url: str | None = None, pumpportal: bool = False, mode: str = "SHADOW") -> Limits:
    initial = {name: int(spec["initial"]) for name, spec in contract["operational_limits_v1"]["keys"].items()}
    url = _websocket_url(chain_ws_url, ENV_CHAIN_WS_URL, "--chain-ws-url")
    level = commitment or COMMITMENT_LEVELS[0]
    if level not in COMMITMENT_LEVELS:
        raise ConfigError(f"chain commitment must be one of {COMMITMENT_LEVELS}")
    strategy = contract["strategy"]
    observation_ms = int(strategy["observation_ms_default"])
    snapshots = tuple(sorted(set(int(x) for x in strategy["runtime_snapshot_ms_v1"]) | {observation_ms}))
    synchronous = contract["persistence"]["synchronous"]["SHADOW_BACKTEST" if mode in ("SHADOW", "BACKTEST") else "PAPER_LIVE_CRITICAL"]
    limit_fields = {field.name for field in fields(Limits)}
    limits = Limits(
        **{name: value for name, value in initial.items() if name in limit_fields},
        websocket_max_message_bytes=WEBSOCKET_MAX_MESSAGE_BYTES,
        chain_program_id=contract["venue_contract"]["program_ids"]["pump"],
        chain_commitment=level,
        chain_ws_url=url,
        pumpportal_ws_url=_websocket_url(pumpportal_ws_url, ENV_PUMPPORTAL_WS_URL, "--pumpportal-ws-url") if pumpportal else None,
        research_horizon_ms=int(contract["multi_coin_concurrency"]["research_horizon_ms_initial_candidate"]),
        runtime_snapshot_ms=snapshots,
        observation_ms=observation_ms,
        synchronous=synchronous,
        create_event_tail_layout=_tail_layout(contract),
        known_pump_event_names=tuple(contract["venue_contract"].get("protocol_definition_v1", {}).get("known_event_names", ())),
    )
    if not limits.queue_bulk_warning_rows < limits.queue_bulk_critical_rows < limits.queue_bulk_hard_limit_rows:
        raise ConfigError("queue limits must satisfy warning < critical < hard_limit")
    if limits.reconnect_backoff_initial_ms > limits.reconnect_backoff_max_ms:
        raise ConfigError("reconnect backoff initial must be <= max")
    return limits


# --------------------------------------------------------------------------- strategy


@dataclass(frozen=True, slots=True)
class FilterRule:
    feature: str
    min: Decimal | None
    max: Decimal | None


@dataclass(frozen=True, slots=True)
class StrategyConfig:
    """Strategy, exit and execution-cost assumptions for one candidate; frozen and hashed (strategy_versions.version_hash).

    Protocol and creator fees are deliberately absent: they come from a session-bound, read-only
    PROTOCOL_CONFIG/FeeConfig snapshot and are never editable by a human or a strategy.
    """

    candidate_name: str
    observation_ms: int
    filters: tuple[FilterRule, ...]
    trade_amount_lamports: int
    target_net_multiple: Decimal
    stop_net_multiple: Decimal
    max_hold_ms: int
    base_fee_lamports: int
    priority_fee_lamports: int
    max_slippage_bps: int
    entry_latency_ms: int

    def content(self) -> dict[str, Any]:
        return {
            "candidate_name": self.candidate_name,
            "observation_ms": self.observation_ms,
            "filters": [{"feature": rule.feature, "min": rule.min, "max": rule.max} for rule in self.filters],
            "trade_amount_lamports": self.trade_amount_lamports,
            "target_net_multiple": self.target_net_multiple,
            "stop_net_multiple": self.stop_net_multiple,
            "max_hold_ms": self.max_hold_ms,
            "base_fee_lamports": self.base_fee_lamports,
            "priority_fee_lamports": self.priority_fee_lamports,
            "max_slippage_bps": self.max_slippage_bps,
            "entry_latency_ms": self.entry_latency_ms,
        }

    def version_hash(self) -> str:
        return sha256_json(self.content())

    def canonical(self) -> str:
        return canonical_json(self.content())


STRATEGY_INT_FIELDS = ("observation_ms", "trade_amount_lamports", "max_hold_ms", "base_fee_lamports", "priority_fee_lamports", "max_slippage_bps", "entry_latency_ms")
STRATEGY_DECIMAL_FIELDS = ("target_net_multiple", "stop_net_multiple")
BASIS_POINTS_PER_UNIT = 10_000


def _decimal(value: Any, name: str) -> Decimal:
    if isinstance(value, bool) or isinstance(value, float):
        raise ConfigError(f"{name} must be an exact decimal (JSON number or string), not {type(value).__name__}")
    try:
        return Decimal(value)
    except (InvalidOperation, TypeError, ValueError) as exc:
        raise ConfigError(f"{name} is not a decimal: {value!r}") from exc


def strategy_from_content(content: Any, contract: dict[str, Any]) -> StrategyConfig:
    """Build and validate a StrategyConfig from parsed JSON; every range comes from the contract."""
    if not isinstance(content, dict):
        raise ConfigError("strategy config must be a JSON object")
    expected = set(StrategyConfig.__dataclass_fields__)
    if set(content) != expected:
        raise ConfigError(f"strategy config fields must be exactly {sorted(expected)}; missing {sorted(expected - set(content))}, unknown {sorted(set(content) - expected)}")
    values: dict[str, Any] = {"candidate_name": content["candidate_name"]}
    if not isinstance(values["candidate_name"], str) or not values["candidate_name"].strip():
        raise ConfigError("candidate_name must be a non-empty string")
    for name in STRATEGY_INT_FIELDS:
        value = content[name]
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            raise ConfigError(f"{name} must be a non-negative int")
        values[name] = value
    for name in STRATEGY_DECIMAL_FIELDS:
        values[name] = _decimal(content[name], name)
    low, high = (int(x) for x in contract["strategy"]["observation_ms_range"])
    if not low <= values["observation_ms"] <= high:
        raise ConfigError(f"observation_ms must be within {low}..{high}")
    if values["trade_amount_lamports"] < 1 or values["max_hold_ms"] < 1:
        raise ConfigError("trade_amount_lamports and max_hold_ms must be >= 1")
    if not values["target_net_multiple"] > 1:
        raise ConfigError("target_net_multiple must be > 1")
    if not 0 <= values["stop_net_multiple"] < 1:
        raise ConfigError("stop_net_multiple must be within [0, 1)")
    if values["max_slippage_bps"] > BASIS_POINTS_PER_UNIT:
        raise ConfigError(f"max_slippage_bps must be <= {BASIS_POINTS_PER_UNIT}")
    if "protocol_fee_bps" in content or "creator_fee_bps" in content:
        raise ConfigError("protocol_fee_bps/creator_fee_bps are read-only FeeConfig data (PROTOCOL_CONFIG model), never strategy fields")
    features = contract["feature_contract_v1"]
    filterable = [name for name in features["definitions"] if name != features["boolean_feature"]]
    raw_filters = content["filters"]
    if not isinstance(raw_filters, list) or len(raw_filters) > int(contract["strategy"]["max_filters"]):
        raise ConfigError(f"filters must be a list of at most {contract['strategy']['max_filters']} rules")
    rules: list[FilterRule] = []
    seen: set[str] = set()
    for index, rule in enumerate(raw_filters):
        if not isinstance(rule, dict) or set(rule) != {"feature", "min", "max"}:
            raise ConfigError(f"filters[{index}] must be {{feature, min, max}}")
        feature = rule["feature"]
        if feature not in filterable or feature in seen:
            raise ConfigError(f"filters[{index}].feature {feature!r} is not a filterable feature or is duplicated")
        seen.add(feature)
        low_bound = None if rule["min"] is None else _decimal(rule["min"], f"filters[{index}].min")
        high_bound = None if rule["max"] is None else _decimal(rule["max"], f"filters[{index}].max")
        if low_bound is not None and high_bound is not None and low_bound > high_bound:
            raise ConfigError(f"filters[{index}]: min > max")
        rules.append(FilterRule(feature, low_bound, high_bound))
    return StrategyConfig(filters=tuple(rules), **values)


def load_strategy(path: Path, contract: dict[str, Any]) -> StrategyConfig:
    return strategy_from_content(load_json(path), contract)

# --------------------------------------------------------------------------- protocol fee snapshot


def load_protocol_fee_config(path: Path):
    """Load one read-only Pump FeeConfig snapshot for deterministic execution modeling.

    The snapshot is not strategy configuration. It records protocol-owned fee tiers observed
    on-chain and is injected into ``PaperFillModel`` by BACKTEST/PAPER/LIVE expectation code.
    """
    from darkbot.execution import FeeRates, FeeTier, ProtocolFeeConfig

    content = load_json(path)
    if not isinstance(content, dict):
        raise ConfigError("protocol fee config must be a JSON object")
    expected = {"account", "read_slot", "read_utc_ns", "fee_tiers"}
    if set(content) != expected:
        raise ConfigError(
            f"protocol fee config fields must be exactly {sorted(expected)}; "
            f"missing {sorted(expected - set(content))}, unknown {sorted(set(content) - expected)}"
        )
    account = content["account"]
    if not isinstance(account, str) or not account.strip():
        raise ConfigError("protocol fee config account must be a non-empty string")
    for name in ("read_slot", "read_utc_ns"):
        value = content[name]
        if value is not None and (isinstance(value, bool) or not isinstance(value, int) or value < 0):
            raise ConfigError(f"protocol fee config {name} must be null or a non-negative int")
    raw_tiers = content["fee_tiers"]
    if not isinstance(raw_tiers, list) or not raw_tiers:
        raise ConfigError("protocol fee config fee_tiers must be a non-empty list")
    tiers = []
    tier_fields = {"market_cap_lamports_threshold", "lp_fee_bps", "protocol_fee_bps", "creator_fee_bps"}
    for index, raw in enumerate(raw_tiers):
        if not isinstance(raw, dict) or set(raw) != tier_fields:
            raise ConfigError(f"fee_tiers[{index}] must contain exactly {sorted(tier_fields)}")
        values = {}
        for name in tier_fields:
            value = raw[name]
            if isinstance(value, bool) or not isinstance(value, int) or value < 0:
                raise ConfigError(f"fee_tiers[{index}].{name} must be a non-negative int")
            values[name] = value
        tiers.append(
            FeeTier(
                values["market_cap_lamports_threshold"],
                FeeRates(values["lp_fee_bps"], values["protocol_fee_bps"], values["creator_fee_bps"]),
            )
        )
    return ProtocolFeeConfig(tuple(tiers), account, content["read_slot"], content["read_utc_ns"])

"""Strict JSON runtime/research configuration (spec: config.py, module_interfaces.config_validation).

Unknown keys are rejected, units/enums/positivity are checked, secrets are resolved only from
the environment (never stored), and canonical hashes bind every policy to config_snapshots.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
from dataclasses import dataclass, field
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Any

LEGACY_FORBIDDEN_KEYS = ("max_hold_ms", "trade_amount_lamports", "stop_net_multiple", "entry_latency_ms", "max_slippage_bps", "candidate_name")
SECRET_PATTERN = re.compile(r"api-key=(?!\$\{)[A-Za-z0-9\-]{8,}")


class ConfigError(ValueError):
    pass


def canonical_json(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def config_hash(value: Any) -> str:
    return sha256_text(canonical_json(value))


def _no_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for key, value in pairs:
        if key in out:
            raise ConfigError(f"duplicate key '{key}'")
        out[key] = value
    return out


def load_json_strict(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=_no_duplicates)
    except json.JSONDecodeError as exc:
        raise ConfigError(f"{path}: invalid JSON: {exc}") from exc
    except FileNotFoundError as exc:
        raise ConfigError(f"{path}: file not found") from exc


# --------------------------------------------------------------------------- schema
# type codes: int, int+ (>0), int0 (>=0), dec (Decimal string), dec+ , bool, str, null|X, list[X], enum:A|B, obj:<name>, any
S = dict

RUNTIME_SCHEMA: dict[str, Any] = {
    "config_version": "str",
    "spec_version": "enum:1.3.2",
    "mode": "enum:PAPER",
    "live_broadcast_enabled": "enum:False",
    "monthly_paid_services_allowed": "enum:False",
    "strategy": S(
        strategy_id="enum:FAST_CREATOR_HOLD_V1|LEGACY_5BUY_5UNIQUE_V1|LEGACY_BACKEND_FILTER_V1", observation_ms="int0", filters="list[any]", entry_trigger="enum:FIRST_ACTIONABLE_CREATE", target_net_multiple="dec+",
        target_exit_enabled="bool", target_is_proven_optimal="bool", ordinary_stop_enabled="enum:False", creator_sell_exit_enabled="bool", entry_amount_lamports="int0",
        sizing_mode="enum:FIXED_SOL|USD_SWAP_BUDGET", policy_frozen_per_position="enum:True", same_event_exit_priority="list[str]", strategy_timeout_ms="int+",
        timeout_policy="enum:FORCE_EXIT", timeout_clock_origin="enum:SUCCESSFUL_ENTRY_FILL",
        entry_amount_semantics="enum:CURVE_INPUT_EXCLUDING_ALL_FEES_AND_RENT|SWAP_BUDGET_INCLUDING_PROTOCOL_FEES|TOTAL_SLOT_BUDGET_INCLUDING_ALL_COSTS", entry_amount_is_usd_pegged="bool",
        entry_reentry_policy="enum:ONE_SUBMITTED_ENTRY_PER_MINT_PER_CAMPAIGN", quote_asset_policy="enum:SOL_ONLY",
    ),
    "fx": S(enabled="bool", source_url="enum:https://api.coinbase.com/v2/prices/SOL-USD/spot", refresh_interval_ms="int+", max_age_ms="int+", request_timeout_ms="int+"),
    "portfolio": S(initial_paper_balance_lamports="int0", initial_paper_balance_mode="enum:FIXED_LAMPORTS|USD_PER_SLOT", slot_capital_usd="dec+", max_open_positions="int+", allow_borrowing="enum:False", reserve_exit_costs="enum:True",
                   occupied_slot_states="list[str]", unresolved_with_possible_exposure_occupies_slot="enum:True"),
    "latency_model": S(entry_delay_ms="int0", exit_delay_ms="int0", label="str", zero_delay_run_can_qualify_edge="enum:False", stage_breakdown_required="enum:True",
                       measured_distribution_id="null|str", stress_delay_ms="list[int0]", scenario_unit="enum:MILLISECONDS"),
    "execution": S(message_templates="any", research_stress=S(network_fee_multiplier="dec+", additional_adverse_price_bps="int0", exit_reject_rate="dec", random_seed="int0"), max_exit_attempts="int+", exit_retry_backoff_ms="list[int+]", quote_max_age_ms="int+", slippage_bps="int0",
                   unknown_status_policy="enum:RECONCILE_BEFORE_RESUBMIT", retries_must_advance_time="enum:True", entry_max_attempts="enum:1",
                   exit_retries_after_exhaustion="str", fill_evidence_status="enum:OBSERVED_STATE_PROXY", supported_venue="enum:PUMP_BONDING_CURVE",
                   pumpswap_execution_enabled="enum:False", message_format="enum:LEGACY_OR_V0_COMPUTE_BUDGET", signature_count="int+",
                   priority_policy=S(compute_unit_limit="int+", price_source="enum:RECENT_PRIORITIZATION_FEES_PERCENTILE", percentile="dec+",
                                     min_compute_unit_price_micro_lamports="int0", max_compute_unit_price_micro_lamports="int+", lookback_slots="int+"),
                   token_account_policy=S(assume_new_token_account_per_position="bool", close_token_account_on_full_exit="bool", spl_token_account_size_bytes="int+", token_2022_supported="bool", profiles="any")),
    "costs": S(protocol_snapshot_id="null|str", base_fee_lamports="null|int0", priority_fee_lamports="null|int0", sender_tip_lamports="int0", account_rent_lamports="null|int0",
               irrecoverable_account_cost_lamports="null|int0", unverified_fee_snapshot_blocks_qualified_pnl="enum:True",
               resolution_mode="enum:AUTO_RESOLVE_AND_PERSIST_BEFORE_ECONOMIC_PAPER", null_means="str", sender_tip_provenance="enum:NOT_APPLICABLE|CONFIGURED",
               irrecoverable_account_cost_provenance="str", base_fee_rule="str", lamports_per_signature="int+",
               snapshot_refresh_policy=S(protocol_refresh_interval_ms="int+", protocol_max_age_ms="int+", network_fee_refresh_interval_ms="int+", network_fee_max_age_ms="int+",
                                         rent_refresh_interval_ms="int+", rent_max_age_ms="int+", status="str", rule="str", min_poll_interval_ms="int+")),
    "storage": S(engine="enum:sqlite", journal_mode="enum:WAL", writer_threads="enum:1", queue_capacity_records="int+", batch_max_records="int+", flush_interval_ms="int+",
                 report_interval_ms="int+", raw_retention_policy="str", on_queue_overflow="enum:MARK_GAP_AND_PAUSE_NEW_ENTRIES", on_disk_error="enum:MARK_RUN_INVALID_AND_PAUSE_NEW_ENTRIES",
                 state_history_per_mint="int+", inactive_mint_retention_ms="int+", max_tracked_mints="int+", priority_headroom_records="int+", writer_locked_retries="int+", fk_merge_max_batches="int+",
                 trade_log_per_mint="int+", telemetry_samples_max="int+", fee_history_max="int+", error_text_max_chars="int+", status_latency_window="int+", writer_shutdown_timeout_ms="int+", writer_busy_timeout_ms="int+", writer_retry_backoff_ms="int+", signature_history_max="int+", dedup_keys_per_mint="int+", undecodable_points_max="int+"),
    "connectivity": S(reconnect_initial_ms="int+", reconnect_max_ms="int+", jitter_fraction="dec", subscription_ack_timeout_ms="int+", liveness_check_interval_ms="int+",
                      position_state_stale_ms="int+", pumpportal_required="bool", pumpportal_api_key_env="str", websocket_max_message_bytes="int+", websocket_ping_interval_ms="int+", rpc_http_timeout_ms="int+"),
    "feeds": "list[obj:feed]",
    "rpc_http": S(endpoint_template="str", endpoint_env="list[str]", fallback_endpoint_template="null|str", fallback_endpoint_env="list[str]", commitment="enum:processed|confirmed", max_requests_per_minute="int+"),
    "protocol": S(program_id="str", idl_source="str", idl_version_or_hash="str", decoder_version="str", supported_variants="list[str]", events="any",
                  create_event_tail_layouts="list[any]", unpinned_create_tail_policy="enum:QUOTE_UNKNOWN_LAYOUT_UNVERIFIED", unpinned_tail_policy="str", wsol_mint="str", native_sol_curve_sentinel="str",
                  spl_token_program="str", token_2022_program="str", token_2022_create_profile="any",
                  fee_config=S(address="str", program_id="str", idl_version_or_hash="str", layout_after_discriminator="list[any]", verification_status="str", rule="enum:PUMP_SOL_MARKET_CAP_TIERS", unsupported_modes="list[str]"),
                  global_account=S(address="str", layout_after_discriminator="list[any]", verification_status="str", max_reasonable_bps="int+", discriminator_source="enum:ANCHOR_SHA256_account:Global_FIRST_8_BYTES"),
                  fee_rounding="enum:CEIL_BPS", curve_rounding_version="str"),
    "research": S(target_grid="list[dec+]", pilot_min_unique_creates="int+", chronological_split="list[dec+]", bootstrap_confidence="dec+", bootstrap_resamples="int+", random_seed="int0",
                  minimum_independent_blocks_for_qualification="int+", adaptive_targets_enabled="enum:False",
                  execution_stress=S(network_fee_multipliers="list[dec+]", additional_adverse_price_bps="list[int0]", failure_scenario="str", conservative_exit_reject_rate="dec", interpretation="str", reject_rate_applies_to="str"),
                  research_horizon_ms="int+", research_horizon_policy="enum:CENSOR", research_horizon_clock_origin="enum:FIRST_VALID_CREATE_RECEIPT",
                  portfolio_position_limits_to_compare="list[int+]", selection_portfolio_position_limit="int+", feature_snapshot_offsets_ms="list[int0]",
                  active_feature_set="enum:FEATURE_SET_V2_CANONICAL_COVERAGE", legacy_feature_export_enabled="bool", max_unresolved_exposure_for_positive_qualification_lamports="int0",
                  feature_ratio_decimal_places="int+", feature_first_buyer_window="int+", block_length_ms="int+", independent_delay_scenarios="any"),
    "doctor": S(max_duration_ms="int+", minimum_valid_create_samples="int+", minimum_valid_trade_samples="int+", broadcast_allowed="enum:False", economic_intents_allowed="enum:False", min_free_disk_bytes="int+"),
}
FEED_SCHEMA = S(id="str", enabled="bool", required="bool", transport="enum:logsSubscribe|pumpportal", commitment="null|enum:processed|confirmed", endpoint_template="str",
                endpoint_env="list[str]", provider="str", region="null|str", provides="list[str]", independence_note="str")
RESEARCH_SCHEMA = S(config_version="str", spec_version="enum:1.3.2", dataset_freeze="any", target_grid="list[dec+]", entry_delay_ms="list[int0]", exit_delay_ms="list[int0]",
                    joint_delay_scenarios="list[any]", network_fee_multipliers="list[dec+]", additional_adverse_price_bps="list[int0]", conservative_exit_reject_rate="dec",
                    portfolio_position_limits="list[int+]", selection_portfolio_position_limit="int+", chronological_split="list[dec+]", block_length_ms="int+",
                    bootstrap_confidence="dec+", bootstrap_resamples="int+", random_seed="int0", minimum_independent_blocks_for_qualification="int+", test_split_unlocked="bool", policy_freeze_note="str",
                    independent_delay_scenarios="any", split_unit="enum:BLOCK", forward_test="bool", baseline_target="dec+", baseline_preference_fraction="dec+")


def _dec(value: Any, label: str, positive: bool) -> Decimal:
    if not isinstance(value, str):
        raise ConfigError(f"{label}: decimal values must be strings, got {type(value).__name__}")
    try:
        parsed = Decimal(value)
    except InvalidOperation as exc:
        raise ConfigError(f"{label}: not a decimal string: {value!r}") from exc
    if not parsed.is_finite():
        raise ConfigError(f"{label}: decimal must be finite")
    if positive and parsed <= 0:
        raise ConfigError(f"{label}: must be > 0")
    return parsed


def _check(value: Any, spec: Any, label: str) -> None:
    if isinstance(spec, dict):
        if not isinstance(value, dict):
            raise ConfigError(f"{label}: expected object")
        unknown = sorted(set(value) - set(spec))
        if unknown:
            for key in unknown:
                if key in LEGACY_FORBIDDEN_KEYS:
                    raise ConfigError(f"{label}.{key}: legacy key not allowed under spec 1.3.2 (use strategy_timeout_ms/entry_amount_lamports/ordinary_stop_enabled semantics)")
            raise ConfigError(f"{label}: unknown keys {unknown}")
        missing = sorted(set(spec) - set(value))
        if missing:
            raise ConfigError(f"{label}: missing keys {missing}")
        for key, sub in spec.items():
            _check(value[key], sub, f"{label}.{key}")
        return
    if spec == "any":
        return
    if spec.startswith("null|"):
        if value is None:
            return
        return _check(value, spec[5:], label)
    if spec.startswith("list["):
        if not isinstance(value, list):
            raise ConfigError(f"{label}: expected list")
        inner = spec[5:-1]
        for index, item in enumerate(value):
            _check(item, FEED_SCHEMA if inner == "obj:feed" else inner, f"{label}[{index}]")
        return
    if spec.startswith("enum:"):
        allowed = spec[5:].split("|")
        if set(allowed) <= {"True", "False"} and type(value) is not bool:
            raise ConfigError(f"{label}: expected boolean")
        if all(a.isdigit() for a in allowed) and type(value) is not int:
            raise ConfigError(f"{label}: expected integer")
        text = str(value) if not isinstance(value, bool) else ("True" if value else "False")
        if isinstance(value, bool) and text not in allowed:
            raise ConfigError(f"{label}: must be {allowed[0]}")
        if text not in allowed or (isinstance(value, (int, float)) and not isinstance(value, bool) and str(value) not in allowed):
            raise ConfigError(f"{label}: value {value!r} not in {allowed}")
        return
    if spec == "bool":
        if not isinstance(value, bool):
            raise ConfigError(f"{label}: expected bool")
        return
    if spec == "str":
        if not isinstance(value, str) or not value:
            raise ConfigError(f"{label}: expected non-empty string")
        return
    if spec in ("int", "int+", "int0"):
        if not isinstance(value, int) or isinstance(value, bool):
            raise ConfigError(f"{label}: expected integer (no floats for money/time)")
        if spec == "int+" and value <= 0:
            raise ConfigError(f"{label}: must be > 0")
        if spec == "int0" and value < 0:
            raise ConfigError(f"{label}: must be >= 0")
        return
    if spec in ("dec", "dec+"):
        _dec(value, label, spec == "dec+")
        return
    raise ConfigError(f"{label}: unknown schema spec {spec}")


def _find_secrets(value: Any, label: str, errors: list[str]) -> None:
    if isinstance(value, dict):
        for key, sub in value.items():
            if key.lower().endswith(("api_key", "apikey", "secret", "private_key")) and sub not in (None, ""):
                errors.append(f"{label}.{key}: secret-like value must not be stored in config; use an environment variable name")
            _find_secrets(sub, f"{label}.{key}", errors)
    elif isinstance(value, list):
        for index, sub in enumerate(value):
            _find_secrets(sub, f"{label}[{index}]", errors)
    elif isinstance(value, str) and SECRET_PATTERN.search(value):
        errors.append(f"{label}: looks like an embedded credential; use ${{ENV_NAME}} placeholders")


@dataclass(frozen=True)
class FeedConfig:
    id: str
    enabled: bool
    required: bool
    transport: str
    commitment: str | None
    endpoint_template: str
    endpoint_env: tuple[str, ...]
    provider: str
    provides: tuple[str, ...]


@dataclass
class RuntimeConfig:
    path: Path
    raw: dict[str, Any]
    canonical: str
    config_hash: str
    policy_hash: str
    cost_model_hash: str
    freshness_policy_hash: str
    quote_model_hash: str
    feature_contract_hash: str
    coverage_version_hash: str
    protocol_hash: str = ""
    strategy_config_hash: str = ""
    feeds: tuple[FeedConfig, ...] = field(default_factory=tuple)

    def __getitem__(self, key: str) -> Any:
        return self.raw[key]

    @property
    def strategy(self) -> dict[str, Any]:
        return self.raw["strategy"]

    @property
    def execution(self) -> dict[str, Any]:
        return self.raw["execution"]

    @property
    def portfolio(self) -> dict[str, Any]:
        return self.raw["portfolio"]

    @property
    def costs(self) -> dict[str, Any]:
        return self.raw["costs"]

    @property
    def storage(self) -> dict[str, Any]:
        return self.raw["storage"]

    @property
    def connectivity(self) -> dict[str, Any]:
        return self.raw["connectivity"]

    @property
    def latency_model(self) -> dict[str, Any]:
        return self.raw["latency_model"]

    @property
    def research(self) -> dict[str, Any]:
        return self.raw["research"]

    @property
    def protocol(self) -> dict[str, Any]:
        return self.raw["protocol"]

    @property
    def doctor(self) -> dict[str, Any]:
        return self.raw["doctor"]

    @property
    def strategy_id(self) -> str:
        return self.strategy["strategy_id"]

    def policy_content(self) -> dict[str, Any]:
        return {"strategy": self.strategy, "portfolio": self.portfolio, "execution": self.execution, "latency_model": self.latency_model}

    def cost_model_content(self) -> dict[str, Any]:
        return {"costs": self.costs, "execution": self.execution, "protocol": {"fee_rounding": self.protocol["fee_rounding"], "curve_rounding_version": self.protocol["curve_rounding_version"], "program_id": self.protocol["program_id"]}}

    def with_overrides(self, **sections: dict[str, Any]) -> "RuntimeConfig":
        """Research variants: copy with replaced strategy/latency/execution/portfolio values, re-validated and re-hashed."""
        raw = json.loads(json.dumps(self.raw))
        for section, values in sections.items():
            raw[section].update(values)
        return build_runtime_config(self.path, raw)

    def snapshot_kinds(self) -> dict[str, tuple[str, dict[str, Any]]]:
        """kind -> (hash, content) for every config_snapshots row that must exist before a run starts."""
        return {
            "RUNTIME_CONFIG": (self.config_hash, self.raw),
            "STRATEGY": (self.strategy_config_hash, self.strategy),
            "POLICY": (self.policy_hash, self.policy_content()),
            "COST_MODEL": (self.cost_model_hash, self.cost_model_content()),
            "FRESHNESS_POLICY": (self.freshness_policy_hash, self.costs["snapshot_refresh_policy"]),
            "QUOTE_MODEL": (self.quote_model_hash, {"curve_rounding_version": self.protocol["curve_rounding_version"], "fee_rounding": self.protocol["fee_rounding"], "slippage_bps": self.execution["slippage_bps"], "quote_max_age_ms": self.execution["quote_max_age_ms"], "research_stress": self.execution["research_stress"]}),
            "FEATURE_CONTRACT": (self.feature_contract_hash, feature_contract_content(self.research)),
            "COVERAGE_VERSION": (self.coverage_version_hash, {"version": "CANONICAL_COVERAGE_V2"}),
            "PROTOCOL": (self.protocol_hash, self.protocol),
        }


def feature_contract_content(research: dict[str, Any]) -> dict[str, Any]:
    from darkbot.strategy import FEATURE_NAMES  # local import keeps config free of strategy at import time

    return {"feature_set_v1": "FEATURE_CONTRACT_V1_LEGACY", "feature_set_v2": research["active_feature_set"], "names": list(FEATURE_NAMES),
            "ratio_decimal_places": research["feature_ratio_decimal_places"], "first_buyer_window": research["feature_first_buyer_window"],
            "offsets_ms": research["feature_snapshot_offsets_ms"]}


def _validate_layout(fields: Any, label: str) -> None:
    if not isinstance(fields, list):
        raise ConfigError(f"{label}: expected field list")
    names = set()
    def kind(t: Any) -> None:
        if isinstance(t, str) and t in {"pubkey", "bool", "string", "u8", "u16", "u64", "u128", "i64"}:
            return
        if isinstance(t, dict) and len(t) == 1:
            if "struct" in t:
                _validate_layout(t["struct"], label)
                return
            if "vec" in t:
                kind(t["vec"])
                return
            if "array" in t and isinstance(t["array"], list) and len(t["array"]) == 2 and type(t["array"][1]) is int and t["array"][1] > 0:
                kind(t["array"][0])
                return
        raise ConfigError(f"{label}: unsupported Borsh type {t!r}")
    for f in fields:
        if not isinstance(f, list) or len(f) != 2 or not isinstance(f[0], str) or f[0] in names:
            raise ConfigError(f"{label}: invalid or duplicate field")
        names.add(f[0])
        kind(f[1])


def _cross_checks(raw: dict[str, Any]) -> None:
    ex, st, po, co = raw["execution"], raw["strategy"], raw["portfolio"], raw["costs"]
    if raw["fx"]["max_age_ms"] < raw["fx"]["refresh_interval_ms"]:
        raise ConfigError("fx.max_age_ms must cover refresh_interval_ms")
    events = raw["protocol"]["events"]
    if not isinstance(events, dict) or set(events) != {"CreateEvent", "TradeEvent", "CompleteEvent", "CompletePumpAmmMigrationEvent"}:
        raise ConfigError("protocol.events: exactly the four supported event definitions are required")
    for name, event in events.items():
        _check(event, S(prefix_fields="list[any]", tail_policy="str", tail_layouts="list[any]"), "protocol.events." + name)
        _validate_layout(event["prefix_fields"], name)
        for layout in event["tail_layouts"]:
            if not isinstance(layout, dict) or set(layout) - {"layout_id", "tail_fields", "quote_mint_field", "verification_status", "note"} or not {"layout_id", "tail_fields", "quote_mint_field", "verification_status"} <= set(layout):
                raise ConfigError(f"protocol.events.{name}: invalid tail layout keys")
            _validate_layout(layout["tail_fields"], name)
    _validate_layout(raw["protocol"]["global_account"]["layout_after_discriminator"], "Global")
    _validate_layout(raw["protocol"]["fee_config"]["layout_after_discriminator"], "FeeConfig")
    for layout in raw["protocol"]["create_event_tail_layouts"]:
        _validate_layout(layout.get("tail_fields"), "CreateEvent tail")
    profiles = ex["token_account_policy"]["profiles"]
    if set(profiles) != {raw["protocol"]["spl_token_program"], raw["protocol"]["token_2022_program"]}:
        raise ConfigError("token account profiles require exactly SPL and Token-2022")
    for profile in profiles.values():
        _check(profile, S(profile_id="str", account_size_bytes="int+", instruction_version="enum:v2"), "token account profile")
    profile = raw["protocol"]["token_2022_create_profile"]
    _check(profile, S(profile_id="str", account_size_bytes="int+", instruction="enum:CreateV2", required_token_instructions="list[str]", allowed_token_instructions="list[str]", source="str"), "Token-2022 create profile")
    if not set(profile["required_token_instructions"]) <= set(profile["allowed_token_instructions"]):
        raise ConfigError("required Token-2022 instructions must be allowed")
    account = profiles[raw["protocol"]["token_2022_program"]]
    if (profile["profile_id"], profile["account_size_bytes"]) != (account["profile_id"], account["account_size_bytes"]):
        raise ConfigError("creation evidence and cost account profiles differ")
    if set(ex["message_templates"]) != {"buy", "sell"}:
        raise ConfigError("execution.message_templates requires buy and sell")
    for name, template in ex["message_templates"].items():
        _check(template, S(discriminator="list[int0]", accounts="list[any]", args="list[any]"), "message_templates." + name)
        if len(template["discriminator"]) != 8 or any(x > 255 for x in template["discriminator"]):
            raise ConfigError("message discriminator must be eight bytes")
    if not 0 <= Decimal(ex["research_stress"]["exit_reject_rate"]) <= 1 or ex["research_stress"]["additional_adverse_price_bps"] > 10000:
        raise ConfigError("execution.research_stress rate/bps outside supported bounds")
    if len(ex["exit_retry_backoff_ms"]) != ex["max_exit_attempts"] - 1:
        raise ConfigError("execution.exit_retry_backoff_ms must contain max_exit_attempts-1 positive values")
    if Decimal(st["target_net_multiple"]) <= 1:
        raise ConfigError("strategy.target_net_multiple must be > 1")
    legacy_backend = st["strategy_id"] == "LEGACY_BACKEND_FILTER_V1"
    expected_priority = ["CREATOR_SELL", "TARGET", "STOP", "TIMEOUT"] if legacy_backend else ["CREATOR_SELL", "TARGET", "TIMEOUT"]
    if st["same_event_exit_priority"] != expected_priority:
        raise ConfigError("strategy.same_event_exit_priority must be " + ",".join(expected_priority))
    if legacy_backend:
        _check_legacy_backend_strategy(st)
    if st["strategy_id"] == "FAST_CREATOR_HOLD_V1" and st["observation_ms"] != 0:
        raise ConfigError("strategy.observation_ms must be 0 (no artificial observation wait)")
    if st["strategy_id"] == "FAST_CREATOR_HOLD_V1":
        if st["sizing_mode"] == "FIXED_SOL":
            if st["entry_amount_lamports"] <= 0:
                raise ConfigError("strategy.entry_amount_lamports must be > 0 for FIXED_SOL")
            if st["entry_amount_semantics"] != "CURVE_INPUT_EXCLUDING_ALL_FEES_AND_RENT":
                raise ConfigError("strategy.entry_amount_semantics invalid for FIXED_SOL")
            if st["entry_amount_is_usd_pegged"]:
                raise ConfigError("strategy.entry_amount_is_usd_pegged must be false for FIXED_SOL")
        else:
            if Decimal(st["trade_size_usd"]) != Decimal("10") or st["entry_amount_lamports"] != 0:
                raise ConfigError("FAST_CREATOR_HOLD_V1 USD profile requires 10 USD slot sizing")
            if st["entry_amount_semantics"] != "TOTAL_SLOT_BUDGET_INCLUDING_ALL_COSTS" or not st["entry_amount_is_usd_pegged"]:
                raise ConfigError("FAST_CREATOR_HOLD_V1 USD profile requires total USD slot-budget semantics")
    if st["filters"] and not legacy_backend:
        raise ConfigError("strategy.filters must be empty in phase 1 (no predictive filters)")
    if po["occupied_slot_states"] != ["ENTRY_INTENT", "ENTRY_PENDING", "OPEN", "EXIT_PENDING", "UNRESOLVED"]:
        raise ConfigError("portfolio.occupied_slot_states deviates from spec")
    if po["initial_paper_balance_mode"] == "USD_PER_SLOT":
        if po["initial_paper_balance_lamports"] != 0:
            raise ConfigError("USD_PER_SLOT requires initial_paper_balance_lamports=0 (derived from receipt-time FX)")
        if st["sizing_mode"] != "USD_SWAP_BUDGET" or not raw["fx"]["enabled"]:
            raise ConfigError("USD_PER_SLOT requires USD_SWAP_BUDGET sizing and fx.enabled=true")
        if Decimal(po["slot_capital_usd"]) != Decimal(st["trade_size_usd"]):
            raise ConfigError("portfolio.slot_capital_usd must equal strategy.trade_size_usd")
    elif po["initial_paper_balance_lamports"] <= st["entry_amount_lamports"]:
        raise ConfigError("FIXED_LAMPORTS initial balance must exceed entry_amount_lamports plus fees/reserve")
    if co["sender_tip_lamports"] != 0 and co["sender_tip_provenance"] != "CONFIGURED":
        raise ConfigError("costs.sender_tip_lamports != 0 requires sender_tip_provenance CONFIGURED")
    if co["sender_tip_lamports"] == 0 and co["sender_tip_provenance"] != "NOT_APPLICABLE":
        raise ConfigError("costs.sender_tip_lamports == 0 requires sender_tip_provenance NOT_APPLICABLE")
    if co["irrecoverable_account_cost_lamports"] is None and not co["irrecoverable_account_cost_provenance"].startswith("VALID_ZERO"):
        raise ConfigError("costs.irrecoverable_account_cost_lamports is NULL (unresolved) without a VALID_ZERO provenance: resolve it or declare VALID_ZERO_*")
    if co["irrecoverable_account_cost_lamports"] not in (None, 0) and co["irrecoverable_account_cost_provenance"].startswith("VALID_ZERO"):
        raise ConfigError("costs.irrecoverable_account_cost_provenance VALID_ZERO contradicts a non-zero value")
    if co["lamports_per_signature"] != 5000:
        raise ConfigError("costs.lamports_per_signature must match the Solana base-fee rule (5000) or be re-verified with the rule name")
    rp = co["snapshot_refresh_policy"]
    for kind in ("protocol", "network_fee", "rent"):
        if rp[f"{kind}_max_age_ms"] < rp[f"{kind}_refresh_interval_ms"]:
            raise ConfigError(f"costs.snapshot_refresh_policy.{kind}_max_age_ms must be >= refresh interval")
    if raw["connectivity"]["reconnect_max_ms"] < raw["connectivity"]["reconnect_initial_ms"]:
        raise ConfigError("connectivity.reconnect_max_ms must be >= reconnect_initial_ms")
    if not 0 <= raw["execution"]["slippage_bps"] <= 10000:
        raise ConfigError("execution.slippage_bps must be between 0 and 10000")
    if not 0 < Decimal(raw["research"]["bootstrap_confidence"]) < 1:
        raise ConfigError("research.bootstrap_confidence must be between 0 and 1")
    if not 0 <= Decimal(raw["research"]["execution_stress"]["conservative_exit_reject_rate"]) <= 1:
        raise ConfigError("research conservative_exit_reject_rate must be between 0 and 1")
    if raw["storage"]["fee_history_max"] < 4:
        raise ConfigError("storage.fee_history_max must retain at least the four current fee components")
    split = [Decimal(x) for x in raw["research"]["chronological_split"]]
    if len(split) != 3 or sum(split) != 1:
        raise ConfigError("research.chronological_split must be three fractions summing to 1")
    pp = ex["priority_policy"]
    if pp["max_compute_unit_price_micro_lamports"] < pp["min_compute_unit_price_micro_lamports"]:
        raise ConfigError("execution.priority_policy max < min")
    if not (Decimal(0) < Decimal(pp["percentile"]) <= 1):
        raise ConfigError("execution.priority_policy.percentile must be in (0,1]")
    feeds = raw["feeds"]
    ids = [f["id"] for f in feeds]
    if len(set(ids)) != len(ids):
        raise ConfigError("feeds: duplicate ids")
    pump = [f for f in feeds if f["transport"] == "pumpportal"]
    if len(pump) > 1:
        raise ConfigError("feeds: at most one pumpportal connection allowed")
    if not any(f["transport"] == "logsSubscribe" and f["enabled"] for f in feeds):
        raise ConfigError("feeds: at least one enabled logsSubscribe RPC feed is required (PumpPortal cannot replace trade coverage)")
    for feed in feeds:
        for env in feed["endpoint_env"]:
            if "${" + env + "}" not in feed["endpoint_template"]:
                raise ConfigError(f"feeds[{feed['id']}]: endpoint_env {env} not referenced in endpoint_template")
    for layout in raw["protocol"]["create_event_tail_layouts"]:
        for key in ("layout_id", "tail_fields", "quote_mint_field", "implied_quote_asset", "verification_status", "token_program"):
            if key not in layout:
                raise ConfigError(f"protocol.create_event_tail_layouts[{layout.get('layout_id')}]: missing {key}")


def _check_legacy_backend_strategy(st: dict[str, Any]) -> None:
    """Strategy C = the old backend's policy. Free parameters are exactly the old StrategyConfig fields; what the old
    PAPER runtime hard-wired (creator exit, target exit, swap-budget sizing) is not configurable here either."""
    from darkbot.strategies.strategy_c_legacy_backend import parse_filters

    max_filters = 8  # old contract strategy.max_filters
    if len(st["filters"]) > max_filters:
        raise ConfigError(f"strategy.filters has {len(st['filters'])} rules, maximum is {max_filters}")
    try:
        parse_filters(st["filters"])
    except ValueError as exc:
        raise ConfigError(f"strategy.{exc}") from exc
    stop = Decimal(str(st["stop_net_multiple"]))
    if not Decimal(0) <= stop < Decimal(1):
        raise ConfigError("strategy.stop_net_multiple must be within [0, 1)")
    if st["ordinary_stop_enabled"] != (stop > 0):
        raise ConfigError("strategy.ordinary_stop_enabled must be true exactly when stop_net_multiple > 0 (stop 0 still exits at net value <= 0)")
    if not st["creator_sell_exit_enabled"] or not st["target_exit_enabled"]:
        raise ConfigError("LEGACY_BACKEND_FILTER_V1: creator-sell exit and target exit were hard-wired in the old backend and must stay enabled")
    if st["entry_amount_semantics"] != "SWAP_BUDGET_INCLUDING_PROTOCOL_FEES":
        raise ConfigError("LEGACY_BACKEND_FILTER_V1: trade amount is a swap budget including pump/creator fees (old trade_amount_lamports)")
    if st["sizing_mode"] == "FIXED_SOL":
        if st["entry_amount_lamports"] <= 0 or st["entry_amount_is_usd_pegged"]:
            raise ConfigError("LEGACY_BACKEND_FILTER_V1 FIXED_SOL requires entry_amount_lamports > 0 and entry_amount_is_usd_pegged=false")
    elif st["entry_amount_lamports"] != 0 or not st["entry_amount_is_usd_pegged"]:
        raise ConfigError("LEGACY_BACKEND_FILTER_V1 USD_SWAP_BUDGET requires entry_amount_lamports=0 and entry_amount_is_usd_pegged=true")


def build_runtime_config(path: Path, raw: dict[str, Any]) -> RuntimeConfig:
    raw = json.loads(json.dumps(raw))
    # Backward-compatible interpretation of pre-strategy captures: FX collection was disabled.
    raw.setdefault("fx", {"enabled": False, "source_url": "https://api.coinbase.com/v2/prices/SOL-USD/spot",
                          "refresh_interval_ms": 10000, "max_age_ms": 30000, "request_timeout_ms": 5000})
    raw.get("strategy", {}).setdefault("strategy_id", "FAST_CREATOR_HOLD_V1")
    raw.get("strategy", {}).setdefault("target_exit_enabled", True)
    raw.setdefault("portfolio", {})
    raw["portfolio"].setdefault("initial_paper_balance_mode", "FIXED_LAMPORTS")
    raw["portfolio"].setdefault("slot_capital_usd", "10")
    schema = {**RUNTIME_SCHEMA}
    if raw.get("strategy", {}).get("sizing_mode") == "USD_SWAP_BUDGET":
        schema["strategy"] = {**RUNTIME_SCHEMA["strategy"], "trade_size_usd": "dec+"}
    if raw.get("strategy", {}).get("strategy_id") == "LEGACY_5BUY_5UNIQUE_V1":
        schema["strategy"] = {**schema["strategy"], "min_buy_count": "int+", "min_unique_buyers": "int+",
                              "stop_net_multiple": "dec", "sizing_mode": "enum:USD_SWAP_BUDGET", "entry_amount_lamports": "enum:0",
                              "entry_amount_semantics": "enum:SWAP_BUDGET_INCLUDING_PROTOCOL_FEES", "entry_amount_is_usd_pegged": "enum:True"}
    if raw.get("strategy", {}).get("strategy_id") == "LEGACY_BACKEND_FILTER_V1":
        schema["strategy"] = {**schema["strategy"], "candidate_name": "str", "stop_net_multiple": "dec", "ordinary_stop_enabled": "bool"}
    _check(raw, schema, "config")
    if raw["strategy"]["strategy_id"] == "LEGACY_5BUY_5UNIQUE_V1":
        st = raw["strategy"]
        required = {"observation_ms": 1000, "min_buy_count": 5, "min_unique_buyers": 5, "ordinary_stop_enabled": False,
                    "creator_sell_exit_enabled": False, "strategy_timeout_ms": 60000}
        if any(st.get(k) != v for k, v in required.items()) or any(Decimal(str(st.get(k, "-1"))) != Decimal(v) for k, v in
                {"trade_size_usd": "10", "target_net_multiple": "1.05", "stop_net_multiple": "0"}.items()):
            raise ConfigError("LEGACY_5BUY_5UNIQUE_V1 policy parameters are frozen")
    _cross_checks(raw)
    errors: list[str] = []
    _find_secrets(raw, "config", errors)
    if errors:
        raise ConfigError("; ".join(errors))
    canonical = canonical_json(raw)
    feeds = tuple(FeedConfig(f["id"], f["enabled"], f["required"], f["transport"], f["commitment"], f["endpoint_template"], tuple(f["endpoint_env"]), f["provider"], tuple(f["provides"])) for f in raw["feeds"])
    cfg = RuntimeConfig(path=path, raw=raw, canonical=canonical, config_hash=sha256_text(canonical), policy_hash="", cost_model_hash="", freshness_policy_hash="", quote_model_hash="", feature_contract_hash="", coverage_version_hash="", feeds=feeds)
    cfg.strategy_config_hash = config_hash(cfg.strategy)
    cfg.policy_hash = config_hash(cfg.policy_content())
    cfg.cost_model_hash = config_hash(cfg.cost_model_content())
    cfg.freshness_policy_hash = config_hash(raw["costs"]["snapshot_refresh_policy"])
    cfg.quote_model_hash = config_hash({"curve_rounding_version": raw["protocol"]["curve_rounding_version"], "fee_rounding": raw["protocol"]["fee_rounding"], "slippage_bps": raw["execution"]["slippage_bps"], "quote_max_age_ms": raw["execution"]["quote_max_age_ms"], "research_stress": raw["execution"]["research_stress"]})
    cfg.feature_contract_hash = config_hash(feature_contract_content(raw["research"]))
    cfg.coverage_version_hash = config_hash({"version": "CANONICAL_COVERAGE_V2"})
    cfg.protocol_hash = config_hash(raw["protocol"])
    return cfg


def load_local_env(path: Path) -> None:
    """Load KEY=VALUE pairs from a local .env without overwriting process env."""
    if not path.exists():
        return

    for lineno, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[7:].strip()
        if "=" not in line:
            raise ConfigError(f"{path}:{lineno}: expected KEY=VALUE")

        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()

        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key):
            raise ConfigError(f"{path}:{lineno}: invalid environment variable name")

        if len(value) >= 2 and value[0] == value[-1] and value[0] in ("'", '"'):
            value = value[1:-1]

        os.environ.setdefault(key, value)


def load_runtime_config(path: Path) -> RuntimeConfig:
    config_path = path.resolve()
    project_env = config_path.parent.parent / ".env"
    load_local_env(project_env)
    raw = load_json_strict(path)
    if not isinstance(raw, dict):
        raise ConfigError(f"{path}: top-level must be an object")
    return build_runtime_config(path, raw)


def load_research_config(path: Path) -> dict[str, Any]:
    raw = load_json_strict(path)
    _check(raw, RESEARCH_SCHEMA, "research")
    split = [Decimal(x) for x in raw["chronological_split"]]
    if len(split) != 3 or sum(split) != 1:
        raise ConfigError("research.chronological_split must be three fractions summing to 1")
    if not 0 < Decimal(raw["bootstrap_confidence"]) < 1 or not 0 <= Decimal(raw["conservative_exit_reject_rate"]) <= 1 or not 0 < Decimal(raw["baseline_preference_fraction"]) <= 1:
        raise ConfigError("research confidence/reject/preference fraction outside supported bounds")
    if any(b > 10000 for b in raw["additional_adverse_price_bps"]):
        raise ConfigError("research adverse price bps must not exceed 10000")
    errors: list[str] = []
    _find_secrets(raw, "research", errors)
    if errors:
        raise ConfigError("; ".join(errors))
    return raw


# --------------------------------------------------------------------------- secrets


@dataclass(frozen=True)
class ResolvedEndpoint:
    feed_id: str
    url: str | None
    missing_env: tuple[str, ...]

    @property
    def available(self) -> bool:
        return self.url is not None


def redact(text: str) -> str:
    text = re.sub(r"api-key=[^&\s\"']+", "api-key=***", text)
    return re.sub(r"([/=?])[0-9a-fA-F-]{24,}", r"\1***", text)  # key-like path/query segments in URLs; plain hashes stay readable


def resolve_template(template: str, env_names: tuple[str, ...], environ: dict[str, str] | None = None) -> tuple[str | None, tuple[str, ...]]:
    environ = os.environ if environ is None else environ
    missing = tuple(name for name in env_names if not environ.get(name))
    if missing:
        return None, missing
    url = template
    for name in env_names:
        url = url.replace("${" + name + "}", environ[name])
    return url, ()


def resolve_feed_endpoints(cfg: RuntimeConfig, environ: dict[str, str] | None = None) -> dict[str, ResolvedEndpoint]:
    out: dict[str, ResolvedEndpoint] = {}
    for feed in cfg.feeds:
        url, missing = resolve_template(feed.endpoint_template, feed.endpoint_env, environ)
        out[feed.id] = ResolvedEndpoint(feed.id, url, missing)
    return out


def resolve_rpc_http(cfg: RuntimeConfig, environ: dict[str, str] | None = None) -> ResolvedEndpoint:
    rpc = cfg.raw["rpc_http"]
    url, missing = resolve_template(rpc["endpoint_template"], tuple(rpc["endpoint_env"]), environ)
    if url is None and rpc.get("fallback_endpoint_template"):
        url2, missing2 = resolve_template(rpc["fallback_endpoint_template"], tuple(rpc["fallback_endpoint_env"]), environ)
        if url2 is not None:
            return ResolvedEndpoint("rpc_http", url2, ())
        return ResolvedEndpoint("rpc_http", None, tuple(sorted(set(missing) | set(missing2))))
    return ResolvedEndpoint("rpc_http", url, missing)

"""domain/config tests: static enums and rows equal the contract; configuration is typed and range-checked."""

from __future__ import annotations

import copy
import unittest
from decimal import Decimal
from pathlib import Path
from typing import Any

from darkbot import domain
from darkbot.cli import resolve_enumerations
from darkbot.config import ConfigError, load_limits, strategy_from_content
from darkbot.domain import ENUM_CONTRACT_NAMES, ROW_TABLE, row_columns

CONTRACT = domain.load_contract(Path(__file__)).document


class EnumAndRowTests(unittest.TestCase):
    def test_enums_equal_contract_enumerations(self) -> None:
        enumerations = resolve_enumerations(CONTRACT)
        for enum_type, name in ENUM_CONTRACT_NAMES.items():
            with self.subTest(enum=name):
                self.assertEqual([member.value for member in enum_type], enumerations[name])

    def test_rows_equal_required_columns(self) -> None:
        columns = CONTRACT["persistence"]["required_columns"]
        for row_type, table in ROW_TABLE.items():
            with self.subTest(table=table):
                self.assertEqual(list(row_columns(row_type)), columns[table])
        for table in domain.INTEGER_PK_TABLES:
            self.assertIn(table, ROW_TABLE.values())

    def test_feature_names_are_the_contract_features(self) -> None:
        self.assertEqual(len(domain.feature_names(CONTRACT)), 17)


class LimitsTests(unittest.TestCase):
    def test_limits_come_from_contract_initial_values(self) -> None:
        limits = load_limits(CONTRACT, chain_ws_url="wss://selftest.invalid")
        initial = {name: int(spec["initial"]) for name, spec in CONTRACT["operational_limits_v1"]["keys"].items()}
        self.assertEqual(limits.tracker_limit, initial["tracker_limit"])
        self.assertEqual(limits.contract_limits_content(CONTRACT), initial)
        self.assertEqual(limits.chain_program_id, CONTRACT["venue_contract"]["program_ids"]["pump"])
        self.assertEqual(limits.runtime_snapshot_ms, tuple(sorted(set(CONTRACT["strategy"]["runtime_snapshot_ms_v1"]) | {CONTRACT["strategy"]["observation_ms_default"]})))
        self.assertIsNone(limits.pumpportal_ws_url)

    def test_missing_url_and_bad_commitment_are_rejected(self) -> None:
        with self.assertRaises(ConfigError):
            load_limits(CONTRACT, chain_ws_url="", commitment="processed")
        with self.assertRaises(ConfigError):
            load_limits(CONTRACT, chain_ws_url="wss://x", commitment="finalized")


class StrategyConfigTests(unittest.TestCase):
    def content(self) -> dict[str, Any]:
        return {
            "candidate_name": "selftest",
            "observation_ms": CONTRACT["strategy"]["observation_ms_default"],
            "filters": [{"feature": "buy_count", "min": 8, "max": None}, {"feature": "unique_buyer_ratio", "min": "0.6", "max": None}],
            "trade_amount_lamports": 50_000_000,
            "target_net_multiple": "2.0",
            "stop_net_multiple": "0.65",
            "max_hold_ms": 30_000,
            "base_fee_lamports": 5_000,
            "priority_fee_lamports": 100_000,
            "max_slippage_bps": 300,
            "entry_latency_ms": 800,
        }

    def test_valid_config_loads_and_hashes_deterministically(self) -> None:
        config = strategy_from_content(self.content(), CONTRACT)
        self.assertEqual(config.filters[1].min, Decimal("0.6"))
        self.assertEqual(config.version_hash(), strategy_from_content(self.content(), CONTRACT).version_hash())
        self.assertEqual(config.target_net_multiple, Decimal("2.0"))

    def test_invalid_configs_are_rejected(self) -> None:
        cases = {
            "observation below range": {"observation_ms": int(CONTRACT["strategy"]["observation_ms_range"][0]) - 1},
            "target not above one": {"target_net_multiple": "1"},
            "stop not below one": {"stop_net_multiple": "1"},
            "float money": {"target_net_multiple": 2.0},
            "operator string": {"filters": [{"feature": "buy_count", "operator": ">=", "threshold": "7"}]},
            "unknown feature": {"filters": [{"feature": "nope", "min": 1, "max": None}]},
            "boolean feature": {"filters": [{"feature": CONTRACT["feature_contract_v1"]["boolean_feature"], "min": None, "max": None}]},
            "min above max": {"filters": [{"feature": "buy_count", "min": 9, "max": 3}]},
            "slippage above 100 percent": {"max_slippage_bps": 10_001},
        }
        for name, change in cases.items():
            with self.subTest(case=name):
                content = copy.deepcopy(self.content())
                content.update(change)
                with self.assertRaises(ConfigError):
                    strategy_from_content(content, CONTRACT)
        content = self.content()
        del content["max_hold_ms"]
        with self.assertRaises(ConfigError):
            strategy_from_content(content, CONTRACT)
        with self.assertRaises(ConfigError):
            strategy_from_content({**self.content(), "protocol_fee_bps": 100}, CONTRACT)

    def test_pumpportal_requires_a_websocket_endpoint(self) -> None:
        with self.assertRaises(ConfigError):
            load_limits(CONTRACT, chain_ws_url="wss://x", pumpportal=True, pumpportal_ws_url="https://pumpportal.fun/data-api/real-time/")
        limits = load_limits(CONTRACT, chain_ws_url="wss://x", pumpportal=True, pumpportal_ws_url="wss://pumpportal.fun/api/data")
        self.assertEqual(limits.pumpportal_ws_url, "wss://pumpportal.fun/api/data")
        self.assertEqual(limits.create_event_tail_layout, (("token_program", "pubkey"), ("is_mayhem_mode", "bool"), ("is_cashback_enabled", "bool"), ("quote_mint", "pubkey"), ("virtual_quote_reserves", "u64")))


if __name__ == "__main__":
    unittest.main()

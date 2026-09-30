"""Phase-0 tests.

These tests never reference a contract version or file name. They resolve the active
contract through the pointer file and prove that the linters read the contract by
tampering with copies and expecting rejection. Parametrised cases use subTest instead
of near-identical test functions (engineering_rules.test_rules).
"""

from __future__ import annotations

import copy
import json
import re
import shutil
import sqlite3
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path
from typing import Any

from darkbot.cli import (
    ContractError,
    apply_migration,
    canonical_json,
    find_project_root,
    fixture_hashes,
    load_active_contract,
    load_frontend_contract,
    load_json,
    sha256_file,
    sha256_json,
    validate_contract,
    validate_fixtures,
    validate_frontend_contract,
    validate_migration,
    validate_model_content,
    validate_pyproject_dependencies,
    validate_repository,
    validate_strategy_configuration,
)

PROJECT_ROOT = find_project_root(Path(__file__))
ACTIVE, POINTER_ERRORS = load_active_contract(PROJECT_ROOT)
assert ACTIVE is not None, POINTER_ERRORS
CONTRACT = ACTIVE.document
MIGRATION = PROJECT_ROOT / CONTRACT["persistence"]["initial_migration"]
FIXTURES = PROJECT_ROOT / CONTRACT["phase0_fixture_contract"]["fixture_root"]
FRONTEND, FRONTEND_ERRORS = load_frontend_contract(ACTIVE)
assert FRONTEND is not None, FRONTEND_ERRORS


class ActiveContractTests(unittest.TestCase):
    def test_pointer_resolves_and_hash_matches(self) -> None:
        self.assertEqual(POINTER_ERRORS, [])
        self.assertEqual(ACTIVE.sha256, sha256_file(ACTIVE.path))

    def test_contract_passes_linter(self) -> None:
        self.assertEqual(validate_contract(CONTRACT, ACTIVE.path), [])

    def test_repository_layout_matches_contract(self) -> None:
        errors = validate_repository(ACTIVE)
        allowed = [error for error in errors if error.startswith("generated file missing")]
        self.assertEqual(errors, allowed, "only the generated lock file may be missing in a fresh checkout")

    def test_frontend_contract_passes_check(self) -> None:
        self.assertEqual(validate_frontend_contract(FRONTEND, CONTRACT), [])


class ContractTamperTests(unittest.TestCase):
    """Each tamper must be rejected; otherwise the linter is a smoke test, not a linter."""

    def tampered(self, mutate: Any) -> list[str]:
        document = copy.deepcopy(CONTRACT)
        mutate(document)
        return validate_contract(document, ACTIVE.path)

    def test_tampers_are_rejected(self) -> None:
        def duplicate_main_class(document: dict[str, Any]) -> None:
            document["architecture"]["main_classes"].append(dict(document["architecture"]["main_classes"][0]))

        def forbidden_engine(document: dict[str, Any]) -> None:
            document["technology"]["database_file"] = document["simplicity_contract"]["forbidden_runtime_components"][0] + "://localhost/darkbot"

        def unlock_money_gate(document: dict[str, Any]) -> None:
            document["gates"][document["phase0_rules"]["gates_locked_at_phase0"][-1]]["status"] = "READY"

        def unknown_enumeration(document: dict[str, Any]) -> None:
            column = next(iter(document["persistence"]["state_columns"]))
            document["persistence"]["state_columns"][column] = "no_such_enumeration"

        def drop_oracle_feature(document: dict[str, Any]) -> None:
            document["phase0_fixture_contract"]["oracle_features"].pop()

        def swap_startup_order(document: dict[str, Any]) -> None:
            steps = document["startup_self_check"]["steps"]
            first = next(step for step in steps if step["code"] == "PROTOCOL_CONFIG_UNREADABLE")
            second = next(step for step in steps if step["code"] == "MODEL_MISSING")
            first["order"], second["order"] = second["order"], first["order"]

        def limits_violate_cross_field(document: dict[str, Any]) -> None:
            keys = document["operational_limits_v1"]["keys"]
            keys["queue_bulk_warning_rows"]["initial"] = keys["queue_bulk_critical_rows"]["initial"] + 1

        def schema_field_without_type(document: dict[str, Any]) -> None:
            del document["model_content_schemas"]["RISK_POLICY"]["max_open_positions"]["type"]

        def limits_key_missing_from_schema(document: dict[str, Any]) -> None:
            del document["model_content_schemas"]["OPERATIONAL_LIMITS"]["tracker_limit"]

        def dependency_duplicated(document: dict[str, Any]) -> None:
            document["technology"]["runtime_dependencies"].append(dict(document["technology"]["runtime_dependencies"][0]))

        tampers = {
            "duplicate main class": duplicate_main_class,
            "forbidden engine": forbidden_engine,
            "empty hard rules": lambda document: document.__setitem__("hard_rules", []),
            "empty tree": lambda document: document["repository_structure"].__setitem__("tree", []),
            "unlocked money gate": unlock_money_gate,
            "unregistered UNSET": lambda document: document["strategy"].__setitem__("observation_ms_default", "UNSET_NEW"),
            "non-frozen status": lambda document: document["document"].__setitem__("status", "DRAFT"),
            "state column without enumeration": unknown_enumeration,
            "secret in contract": lambda document: document["technology"].__setitem__("api_key", "x"),
            "feature without oracle": drop_oracle_feature,
            "startup order violated": swap_startup_order,
            "initial limits violate cross-field constraint": limits_violate_cross_field,
            "schema field without type": schema_field_without_type,
            "operational limit key missing from schema": limits_key_missing_from_schema,
            "runtime dependency duplicated": dependency_duplicated,
        }
        for name, mutate in tampers.items():
            with self.subTest(tamper=name):
                self.assertTrue(self.tampered(mutate), f"{name} was accepted")


class ModelValueTests(unittest.TestCase):
    """Typed model values: the ranges live only in the contract and are enforced generically."""

    def limits(self) -> dict[str, int]:
        return {name: spec["initial"] for name, spec in CONTRACT["operational_limits_v1"]["keys"].items()}

    def test_initial_limits_pass(self) -> None:
        self.assertEqual(validate_model_content("OPERATIONAL_LIMITS", self.limits(), CONTRACT), [])

    def test_invalid_values_are_rejected(self) -> None:
        def mutate(**changes: Any) -> dict[str, Any]:
            content = self.limits()
            content.update(changes)
            return content

        cases = {
            "wrong type": mutate(tracker_limit="2000"),
            "bool is not int": mutate(tracker_limit=True),
            "below minimum": mutate(busy_timeout_ms=0),
            "null where not nullable": mutate(rpc_timeout_ms=None),
            "UNSET value": mutate(rpc_timeout_ms="UNSET_LATER"),
            "extra key": mutate(extra_key=1),
            "cross-field warning >= critical": mutate(queue_bulk_warning_rows=self.limits()["queue_bulk_critical_rows"]),
            "cross-field backoff initial > max": mutate(reconnect_backoff_initial_ms=self.limits()["reconnect_backoff_max_ms"] + 1),
        }
        cases["missing key"] = {name: value for name, value in self.limits().items() if name != "tracker_limit"}
        for name, content in cases.items():
            with self.subTest(case=name):
                self.assertTrue(validate_model_content("OPERATIONAL_LIMITS", content, CONTRACT), f"{name} was accepted")

    def test_quantiles_and_probabilities(self) -> None:
        latency = {
            "status": "ASSUMED",
            "decision_to_pre_buy_read_us_quantiles": {"p50": 10, "p95": 20, "p99": 30},
            "pre_buy_read_us_quantiles": {"p50": 10, "p95": 20, "p99": 30},
            "post_read_to_submit_us_quantiles": {"p50": 10, "p95": 20, "p99": 30},
            "submit_to_landing_us_quantiles": {"p50": 10, "p95": 20, "p99": 30},
            "drop_or_expiry_probability": Decimal("0.05"),
            "stress_latency_offsets_ms": [0, 100, 250],
            "source": "selftest",
        }
        self.assertEqual(validate_model_content("LATENCY", latency, CONTRACT), [])
        for name, change in {"p95 above p99": {"pre_buy_read_us_quantiles": {"p50": 10, "p95": 40, "p99": 30}}, "probability above one": {"drop_or_expiry_probability": Decimal("1.5")}, "negative stress offset": {"stress_latency_offsets_ms": [-1]}, "unknown status": {"status": "GUESSED"}}.items():
            with self.subTest(case=name):
                self.assertTrue(validate_model_content("LATENCY", dict(latency, **change), CONTRACT))

    def test_split_shares_must_sum_to_one(self) -> None:
        split = {"train_share": Decimal("0.5"), "validation_share": Decimal("0.2"), "oos_share": Decimal("0.3"), "embargo_ms": 600000, "min_trades_train": 500, "min_trades_validation": 200, "min_trades_oos": 300, "dataset_sha256": "a" * 64, "registered_at_utc_ns": 1}
        self.assertEqual(validate_model_content("SPLIT_PROTOCOL", split, CONTRACT), [])
        self.assertTrue(validate_model_content("SPLIT_PROTOCOL", dict(split, oos_share=Decimal("0.4")), CONTRACT))

    def strategy(self) -> dict[str, Any]:
        strategy = CONTRACT["strategy"]
        return {
            "candidate_name": "selftest",
            "observation_ms": strategy["observation_ms_default"],
            "snapshot_ms": sorted(set(strategy["runtime_snapshot_ms_v1"]) | {strategy["observation_ms_default"]}),
            "feature_set_model_id": "fs",
            "filters": [{"feature": "unique_buyer_ratio", "min": Decimal("0.6"), "max": None}, {"feature": "buy_count", "min": 7, "max": None}],
            "trade_amount_usd_ref": Decimal("10.0"),
            "sol_usd_rate": Decimal("200"),
            "sol_usd_source": "selftest",
            "sol_usd_time_utc": "2026-09-11T00:00:00Z",
            "trade_amount_lamports": 50_000_000,
            "target_net_multiple": Decimal("2.0"),
            "stop_net_multiple": Decimal("0.5"),
            "max_hold_timeout_ms": 300000,
            "risk_policy_model_id": "r",
            "cost_model_id": "c",
            "latency_model_id": "l",
            "venue_transition_model_id": "v",
        }

    def test_strategy_configuration(self) -> None:
        self.assertEqual(validate_strategy_configuration(self.strategy(), CONTRACT), [])
        strategy = CONTRACT["strategy"]
        cases = {
            "observation below contract range": {"observation_ms": strategy["observation_ms_range"][0] - 1},
            "snapshot list not derived": {"snapshot_ms": [strategy["observation_ms_default"]]},
            "stop not below target": {"stop_net_multiple": Decimal("2.5")},
            "operator string instead of min/max": {"filters": [{"feature": "buy_count", "operator": ">=", "threshold": "7"}]},
            "unknown feature": {"filters": [{"feature": "no_such_feature", "min": 1, "max": None}]},
            "boolean feature is not filterable": {"filters": [{"feature": CONTRACT["feature_contract_v1"]["boolean_feature"], "min": None, "max": None}]},
            "min above max": {"filters": [{"feature": "buy_count", "min": 9, "max": 3}]},
            "too many filters": {"filters": [{"feature": name, "min": None, "max": None} for name in list(CONTRACT["feature_contract_v1"]["definitions"])[: strategy["max_filters"] + 1]]},
        }
        for name, change in cases.items():
            with self.subTest(case=name):
                self.assertTrue(validate_strategy_configuration(dict(self.strategy(), **change), CONTRACT), f"{name} was accepted")


class MigrationTests(unittest.TestCase):
    def test_migration_passes_linter(self) -> None:
        self.assertEqual(validate_migration(MIGRATION, CONTRACT), [])

    def _write_variant(self, transform: Any) -> Path:
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        variant = directory / MIGRATION.name
        variant.write_text(transform(MIGRATION.read_text(encoding="utf-8")), encoding="utf-8")
        return variant

    def test_missing_index_rejected(self) -> None:
        variant = self._write_variant(lambda sql: re.sub(r"CREATE INDEX [^;]+;", "", sql, count=1))
        self.assertTrue(any("indexes differ" in error for error in validate_migration(variant, CONTRACT)))

    def test_missing_check_enum_rejected(self) -> None:
        def strip_first_check(sql: str) -> str:
            column, enum = next(iter(CONTRACT["persistence"]["state_columns"].items()))
            name = column.split(".")[1]
            return re.sub(rf"({name} TEXT(?: NOT NULL)?) CHECK \({name} IN \([^)]*\)\)", r"\1", sql, count=1)

        variant = self._write_variant(strip_first_check)
        self.assertTrue(any("has no CHECK" in error for error in validate_migration(variant, CONTRACT)))

    def test_extra_table_rejected(self) -> None:
        variant = self._write_variant(lambda sql: sql + "\nCREATE TABLE legacy_shim (id INTEGER PRIMARY KEY);\n")
        self.assertTrue(any("tables differ" in error for error in validate_migration(variant, CONTRACT)))

    def test_version_literal_in_sql_rejected(self) -> None:
        variant = self._write_variant(lambda sql: sql + f"\n-- {CONTRACT['document']['version']}\n")
        self.assertTrue(any("version literal" in error for error in validate_migration(variant, CONTRACT)))

    def test_transaction_statements_in_sql_rejected(self) -> None:
        variant = self._write_variant(lambda sql: "BEGIN IMMEDIATE;\n" + sql + "\nCOMMIT;\n")
        self.assertTrue(any("BEGIN/COMMIT" in error for error in validate_migration(variant, CONTRACT)))

    def test_probe_removed_from_contract_is_detected_by_tamper(self) -> None:
        """A schema that accepts a forbidden row must fail: drop one CHECK and expect the probe to fire."""
        variant = self._write_variant(lambda sql: sql.replace("CHECK ((session_id IS NULL) <> (backtest_run_id IS NULL))\n);", "CHECK (1 = 1)\n);", 1))
        errors = validate_migration(variant, CONTRACT)
        self.assertTrue(any("probe" in error and "accepted" in error for error in errors), errors)

    def test_runner_records_schema_version_from_contract(self) -> None:
        connection = sqlite3.connect(":memory:")
        self.addCleanup(connection.close)
        apply_migration(connection, MIGRATION.read_text(encoding="utf-8"), CONTRACT["document"]["version"], MIGRATION.name, applied_at_utc_ns=1)
        self.assertEqual(connection.execute("SELECT contract_version, migration_name FROM schema_version").fetchone(), (CONTRACT["document"]["version"], MIGRATION.name))
        self.assertFalse(connection.in_transaction)

    def test_ddl_and_schema_version_are_atomic(self) -> None:
        """A failure after the DDL leaves nothing behind: no 'tables created but schema_version missing' state."""
        connection = sqlite3.connect(":memory:")
        self.addCleanup(connection.close)
        broken = MIGRATION.read_text(encoding="utf-8") + "\nCREATE TABLE schema_version (x INTEGER);\n"
        with self.assertRaises(sqlite3.Error):
            apply_migration(connection, broken, CONTRACT["document"]["version"], MIGRATION.name, applied_at_utc_ns=1)
        self.assertEqual(connection.execute("SELECT count(*) FROM sqlite_master").fetchone(), (0,))
        self.assertFalse(connection.in_transaction)


class RepositoryTests(unittest.TestCase):
    def _pyproject(self, dependencies: list[str], requires: str | None = None) -> Path:
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        path = directory / "pyproject.toml"
        deps = ",\n".join(f'  "{item}"' for item in dependencies)
        path.write_text(f'[project]\nname = "x"\nversion = "0"\nrequires-python = "{requires or CONTRACT["technology"]["python_requirement"]}"\ndependencies = [\n{deps}\n]\n', encoding="utf-8")
        return path

    def test_pyproject_must_match_runtime_dependencies(self) -> None:
        expected = [item["pypi_name"] for item in CONTRACT["technology"]["runtime_dependencies"]]
        self.assertEqual(validate_pyproject_dependencies(self._pyproject([f"{name}>=1" for name in expected]), CONTRACT), [])
        cases = {
            "missing package": expected[1:],
            "extra package": expected + ["requests"],
            "duplicated package": expected + [expected[0]],
        }
        for name, dependencies in cases.items():
            with self.subTest(case=name):
                self.assertTrue(validate_pyproject_dependencies(self._pyproject(dependencies), CONTRACT), f"{name} was accepted")
        self.assertTrue(validate_pyproject_dependencies(self._pyproject(expected, requires=">=3.8"), CONTRACT))


class FrontendContractTests(unittest.TestCase):
    def tampered(self, mutate: Any) -> list[str]:
        document = copy.deepcopy(FRONTEND)
        mutate(document)
        return validate_frontend_contract(document, CONTRACT)

    def test_drift_is_rejected(self) -> None:
        bindings = FRONTEND["backend_bindings"]
        enum_path = next(iter(bindings["enumeration_bindings"]))

        def set_path(document: dict[str, Any], dotted: str, value: Any) -> None:
            *parents, last = dotted.split(".")
            target: Any = document
            for part in parents:
                target = target[int(part)] if isinstance(target, list) else target[part]
            target[last] = value

        cases = {
            "value outside enumeration": lambda document: set_path(document, enum_path, "null | DATA_INCOMPLETE | MIGRATION_UNRESOLVED"),
            "removed field reappears": lambda document: set_path(document, bindings["forbidden_fields"][0], "bool"),
            "operator string in filter rule": lambda document: set_path(document, bindings["filter_fields"] + ".operator", ">= | <="),
            "second loss streak semantic": lambda document: set_path(document, bindings["risk_policy_fields"] + ".max_consecutive_losses", "int"),
            "wrong fixed value": lambda document: set_path(document, next(iter(bindings["fixed_values"])), "something else"),
        }
        for name, mutate in cases.items():
            with self.subTest(case=name):
                self.assertTrue(self.tampered(mutate), f"{name} was accepted")


class FixtureGateTests(unittest.TestCase):
    """The gate must reject missing, fabricated and tampered fixtures, and accept the real capture once present."""

    def setUp(self) -> None:
        self.spec = CONTRACT["phase0_fixture_contract"]
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        (self.root / self.spec["provider_dir"]).mkdir()
        (self.root / self.spec["feature_dir"]).mkdir()

    def write(self, relative: str, value: Any) -> Path:
        path = self.root / relative
        path.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")
        return path

    def manifest_path(self) -> Path:
        return self.root / self.spec["provider_dir"] / self.spec["manifest_file"]

    def cases_path(self) -> Path:
        return self.root / self.spec["feature_dir"] / self.spec["feature_cases_file"]

    def synthetic_capture(self, curve_inputs: dict[str, Any] | None = None, reserves: bool = False) -> None:
        """A structurally complete capture flagged synthetic_selftest; only used to test the validator."""
        spec = self.spec
        keys = spec["field_map_keys"]
        field_map = {key: key for key in keys}
        tx_values = {"create": "create", "buy": "buy", "sell": "sell"}
        mint = "SELFTESTMINT"
        create = {"mint": mint, "tx_type": "create", "signature": "sigC", "trader": "dev", "sol_amount": "1", "token_amount": "1", "v_sol_reserves": "30", "v_token_reserves": "1000"}
        trades = []
        for index in range(spec["minimum_trade_events"]):
            side = "sell" if index % 5 == 4 else "buy"
            trades.append({"mint": mint, "tx_type": side, "signature": f"sig{index}", "trader": f"t{index % 3}", "sol_amount": "0.1", "token_amount": "10", "v_sol_reserves": "30", "v_token_reserves": "1000"})
        migration = {"mint": mint, "tx_type": "migrate", "signature": "sigM"}
        files = spec["provider_files"]
        provider_dir = self.root / spec["provider_dir"]
        self.write(f"{spec['provider_dir']}/{files['create']}", create)
        self.write(f"{spec['provider_dir']}/{files['trades']}", trades)
        self.write(f"{spec['provider_dir']}/{files['migration']}", migration)
        shape = {files["create"]: sorted(create), files["trades"]: sorted({key for trade in trades for key in trade}), files["migration"]: sorted(migration)}
        limit = spec["minimum_trade_events"]
        price = CONTRACT["market_data_source_v1"]["feed_pricing_observed"]["lamports_per_paid_message"]
        checks = {name: True for name in spec["capture_validity_checks"] if name != "rule"}
        manifest = {
            "provider": spec["provider"],
            "schema_version": sha256_json(shape),
            "captured_at_utc": "selftest",
            "source": "selftest",
            "capture_mint": mint,
            "field_map": field_map,
            "tx_type_values": tx_values,
            "redactions": [],
            "offsets_time_basis": spec["offsets_time_basis_value"],
            "warmup_measurement": {"subscribe_sent_utc_ns": 1, "subscribe_sent_offset_ns": 0, "first_trade_received_utc_ns": 2, "first_trade_received_offset_ns": 50_000_000, "subscribe_ack_offset_ns": None, "ack_message_raw": None, "ack_candidates_raw": []},
            "feed_cost": {"spend_cap_sol": str(Decimal(limit * price) / Decimal(spec["lamports_per_sol"])), "lamports_per_paid_message": price, "lamports_per_sol": spec["lamports_per_sol"], "paid_message_limit": limit, "paid_messages_at_unsubscribe": limit, "paid_message_count": limit, "estimated_cost_lamports": limit * price},
            "trade_subscription_stop_reason": spec["trade_subscription_stop_reasons"][0],
            "capture_stop_reason": spec["capture_stop_reasons"][0],
            "migration_status": spec["migration_statuses"][1],
            "migration_mint_matches_capture": True,
            "capture_validity": {"valid": True, "checks": checks, "failed": []},
            "synthetic_selftest": True,
            "files": {name: sha256_file(provider_dir / name) for name in files.values()},
        }
        manifest_path = self.write(f"{spec['provider_dir']}/{spec['manifest_file']}", manifest)
        self.raw = [create, *trades]
        inputs = {name: None for name in spec["curve_inputs_fields"] if name != "rule"}
        inputs.update({"token_decimals_source": "selftest: not read", "protocol_config_source": "selftest: not read", "create_virtual_sol_reserves": 30_000_000_000})
        if curve_inputs:
            inputs.update(curve_inputs)
        self.curve_inputs = inputs
        events = []
        for index, trade in enumerate(trades):
            vtok = 1000 - 10 * (index + 1) if reserves else None
            events.append({"raw_sha256": sha256_json(trade), "receive_offset_ms": str(50 * (index + 1)), "event_type": "BUY" if trade["tx_type"] == "buy" else "SELL", "actor": trade["trader"], "quote_lamports": 100_000_000, "token_base_units": 10 if reserves else None, "virtual_sol_reserves": 30_000_000_000 + 100_000_000 * (index + 1), "virtual_token_reserves": vtok})
        self.events = events
        cases = []
        deadlines = sorted(set(CONTRACT["strategy"]["runtime_snapshot_ms_v1"]) | set(CONTRACT["strategy"]["research_replay_snapshot_ms"]))
        for deadline in deadlines:
            expected, nulls = self.expected(events, deadline)
            cases.append({"case_id": f"selftest_{deadline}", "deadline_ms": deadline, "coverage_state_at_deadline": "OBSERVED", "data_quality_interval_intersects": False, "normalized_events": events, "expected": expected, "null_reasons": nulls, "derivation": "selftest oracle"})
        self.write(f"{spec['feature_dir']}/{spec['feature_cases_file']}", {"feature_set_version": CONTRACT["feature_contract_v1"]["version"], "source_manifest_sha256": sha256_file(manifest_path), "curve_inputs": inputs, "cases": cases})

    def expected(self, events: list[dict[str, Any]], deadline: int) -> tuple[dict[str, Any], dict[str, str]]:
        """Independent reference values: E-features by hand, R/C-features from the curve inputs (or null), Q from the case."""
        visible = [event for event in events if Decimal(event["receive_offset_ms"]) <= deadline]
        buys = [event for event in visible if event["event_type"] == "BUY"]
        sells = [event for event in visible if event["event_type"] == "SELL"]
        lamports = Decimal(self.spec["lamports_per_sol"])
        quantum = Decimal(1).scaleb(-self.spec["ratio_decimal_places"])
        buy_volume = sum(Decimal(event["quote_lamports"]) for event in buys)
        sell_volume = sum(Decimal(event["quote_lamports"]) for event in sells)
        per_buyer: dict[str, Decimal] = {}
        order: list[str] = []
        for event in buys:
            per_buyer[event["actor"]] = per_buyer.get(event["actor"], Decimal(0)) + Decimal(event["quote_lamports"])
            if event["actor"] not in order:
                order.append(event["actor"])
        expected: dict[str, Any] = {name: None for name in CONTRACT["feature_contract_v1"]["definitions"]}
        nulls: dict[str, str] = {}
        expected.update({
            "buy_count": len(buys),
            "sell_count": len(sells),
            "buy_volume_sol": str(buy_volume / lamports),
            "sell_volume_sol": str(sell_volume / lamports),
            "net_buy_flow_sol": str((buy_volume - sell_volume) / lamports),
            "unique_buyers": len(per_buyer),
            "unique_buyer_ratio": str((Decimal(len(per_buyer)) / Decimal(len(buys))).quantize(quantum)) if buys else None,
            "largest_buy_sol": str(max(Decimal(event["quote_lamports"]) for event in buys) / lamports) if buys else None,
            "buyer_volume_concentration": str(sum(((volume / buy_volume).quantize(quantum) ** 2 for volume in per_buyer.values()), Decimal(0)).quantize(quantum)) if buy_volume else None,
            "first5_buyer_concentration": str((sum(per_buyer[buyer] for buyer in order[: self.spec["first_buyer_window"]]) / buy_volume).quantize(quantum)) if buy_volume else None,
            "first_sell_latency_ms": sells[0]["receive_offset_ms"] if sells else None,
            "provider_gap_flag": False,
        })
        curve = self.curve_inputs
        last = visible[-1] if visible else None
        vsol_t = last["virtual_sol_reserves"] if last else curve["create_virtual_sol_reserves"]
        vtok_t = last["virtual_token_reserves"] if last else curve["create_virtual_token_reserves"]
        if curve["create_virtual_token_reserves"] is not None and vtok_t is not None:
            price_t0 = (Decimal(curve["create_virtual_sol_reserves"]) / Decimal(curve["create_virtual_token_reserves"])).quantize(quantum)
            price_t = (Decimal(vsol_t) / Decimal(vtok_t)).quantize(quantum)
            expected["price_change"] = str((price_t / price_t0 - 1).quantize(quantum))
            peak, worst = price_t0, Decimal(0)
            for event in visible:
                current = (Decimal(event["virtual_sol_reserves"]) / Decimal(event["virtual_token_reserves"])).quantize(quantum)
                peak = max(peak, current)
                worst = max(worst, (1 - current / peak).quantize(quantum))
            expected["max_drawdown"] = str(worst)
            real = vtok_t - (curve["initial_virtual_token_reserves"] - curve["initial_real_token_reserves"])
            expected["curve_progress_pct"] = str((100 * (1 - Decimal(real) / Decimal(curve["initial_real_token_reserves"]))).quantize(quantum))
            quote = curve["research_quote_amount_lamports"]
            tokens_out = vtok_t - (vsol_t * vtok_t) // (vsol_t + quote)
            average = (Decimal(quote) / Decimal(tokens_out)).quantize(quantum)
            expected["research_estimated_price_impact"] = str((average / price_t - 1).quantize(quantum))
            expected["dev_initial_buy"] = str((Decimal(curve["initial_buy_token_base_units"]) / Decimal(curve["token_total_supply"])).quantize(quantum))
        for name, value in expected.items():
            if value is None:
                nulls[name] = "not derivable in selftest"
        return expected, nulls

    def test_missing_fixtures_are_rejected(self) -> None:
        errors = validate_fixtures(self.root, CONTRACT)
        self.assertEqual(len(errors), 1)
        self.assertIn("missing real fixtures", errors[0])

    def test_fabricated_fixtures_are_rejected(self) -> None:
        files = self.spec["provider_files"]
        for name in files.values():
            self.write(f"{self.spec['provider_dir']}/{name}", {"dummy": True})
        self.write(f"{self.spec['provider_dir']}/{self.spec['manifest_file']}", {"provider": self.spec["provider"], "schema_version": "x", "captured_at_utc": "x", "source": "x", "files": {}})
        self.write(f"{self.spec['feature_dir']}/{self.spec['feature_cases_file']}", {"cases": [{"case_id": "fake", "deadline_ms": 1000, "normalized_events": [], "expected": {name: None for name in CONTRACT["feature_contract_v1"]["definitions"]}}]})
        self.assertTrue(validate_fixtures(self.root, CONTRACT, allow_synthetic=True))

    def test_synthetic_capture_never_passes_the_gate(self) -> None:
        self.synthetic_capture()
        errors = validate_fixtures(self.root, CONTRACT)
        self.assertTrue(any("synthetic" in error for error in errors), errors)

    def test_validator_accepts_consistent_capture_in_selftest_mode(self) -> None:
        self.synthetic_capture()
        self.assertEqual(validate_fixtures(self.root, CONTRACT, allow_synthetic=True), [])

    def test_curve_features_are_recomputed_by_the_oracle(self) -> None:
        inputs = {"token_decimals": 6, "create_virtual_token_reserves": 1000, "initial_virtual_token_reserves": 1000, "initial_real_token_reserves": 800, "token_total_supply": 1_000_000, "initial_buy_token_base_units": 25_000, "research_quote_amount_lamports": 50_000_000}
        self.synthetic_capture(curve_inputs=inputs, reserves=True)
        self.assertEqual(validate_fixtures(self.root, CONTRACT, allow_synthetic=True), [])
        document = json.loads(self.cases_path().read_text(encoding="utf-8"))
        for feature in ("price_change", "max_drawdown", "curve_progress_pct", "research_estimated_price_impact", "dev_initial_buy", "provider_gap_flag"):
            with self.subTest(feature=feature):
                tampered = copy.deepcopy(document)
                case = tampered["cases"][0]
                case["expected"][feature] = True if feature == "provider_gap_flag" else "0.5"
                self.cases_path().write_text(json.dumps(tampered), encoding="utf-8")
                self.assertTrue(any("oracle" in error and feature in error for error in validate_fixtures(self.root, CONTRACT, allow_synthetic=True)), feature)

    def test_manifest_tampers_are_rejected(self) -> None:
        self.synthetic_capture()
        original = json.loads(self.manifest_path().read_text(encoding="utf-8"))
        cases = {
            "hash mismatch": None,
            "utc offsets": {"offsets_time_basis": "utc_ns"},
            "ack assumed while unproven": {"warmup_measurement": dict(original["warmup_measurement"], ack_message_raw='{"message": "ok"}')},
            "missing warmup": "drop:warmup_measurement",
            "paid limit above spend cap": {"feed_cost": dict(original["feed_cost"], paid_message_limit=original["feed_cost"]["paid_message_limit"] + 1)},
            "subscription not closed at limit": {"feed_cost": dict(original["feed_cost"], paid_messages_at_unsubscribe=original["feed_cost"]["paid_message_limit"] + 1, paid_message_count=original["feed_cost"]["paid_message_limit"] + 1, estimated_cost_lamports=(original["feed_cost"]["paid_message_limit"] + 1) * original["feed_cost"]["lamports_per_paid_message"])},
            "migration status contradicts contract rule": {"migration_status": self.spec["migration_statuses"][0]},
            "unknown stop reason": {"capture_stop_reason": "BECAUSE"},
            "capture reported invalid": {"capture_validity": dict(original["capture_validity"], valid=False)},
        }
        for name, change in cases.items():
            with self.subTest(case=name):
                manifest = copy.deepcopy(original)
                if change is None:
                    path = self.root / self.spec["provider_dir"] / self.spec["provider_files"]["trades"]
                    trades = json.loads(path.read_text(encoding="utf-8"))
                    trades[0]["sol_amount"] = "999"
                    path.write_text(json.dumps(trades), encoding="utf-8")
                elif isinstance(change, str):
                    del manifest[change.split(":")[1]]
                else:
                    manifest.update(change)
                self.manifest_path().write_text(json.dumps(manifest), encoding="utf-8")
                self.assertTrue(validate_fixtures(self.root, CONTRACT, allow_synthetic=True), f"{name} was accepted")
                if change is None:
                    self.synthetic_capture()

    def test_feature_case_tampers_are_rejected(self) -> None:
        self.synthetic_capture()
        original = json.loads(self.cases_path().read_text(encoding="utf-8"))

        def invented_origin(document: dict[str, Any]) -> None:
            document["cases"][0]["normalized_events"][0]["raw_sha256"] = sha256_json({"invented": True})

        def wrong_count(document: dict[str, Any]) -> None:
            document["cases"][0]["expected"]["buy_count"] = 99

        def assumed_decimals(document: dict[str, Any]) -> None:
            document["cases"][0]["normalized_events"][0]["token_base_units"] = 10_000_000

        def missing_coverage(document: dict[str, Any]) -> None:
            del document["cases"][0]["coverage_state_at_deadline"]

        def gap_flag_ignored(document: dict[str, Any]) -> None:
            document["cases"][0]["data_quality_interval_intersects"] = True

        def null_without_reason(document: dict[str, Any]) -> None:
            document["cases"][0]["null_reasons"].pop("price_change")

        for name, mutate in {"normalized event without raw origin": invented_origin, "wrong expected value": wrong_count, "token amounts although decimals unknown": assumed_decimals, "coverage field missing": missing_coverage, "provider_gap_flag ignores interval": gap_flag_ignored, "null without reason": null_without_reason}.items():
            with self.subTest(case=name):
                document = copy.deepcopy(original)
                mutate(document)
                self.cases_path().write_text(json.dumps(document), encoding="utf-8")
                self.assertTrue(validate_fixtures(self.root, CONTRACT, allow_synthetic=True), f"{name} was accepted")

    def test_real_fixtures_are_accepted_when_present(self) -> None:
        errors = validate_fixtures(FIXTURES, CONTRACT)
        if errors and errors[0].startswith("missing real fixtures"):
            self.skipTest("real capture fixtures not recorded yet (gate PAID_FIXTURE_CAPTURE)")
        self.assertEqual(errors, [])

    def test_fixture_hashes_tool_is_the_single_hash_definition(self) -> None:
        self.synthetic_capture()
        report = fixture_hashes(PROJECT_ROOT, self.root)
        manifest = json.loads(self.manifest_path().read_text(encoding="utf-8"))
        self.assertEqual(report["schema_version"], manifest["schema_version"])
        self.assertEqual(report["files"], manifest["files"])
        self.assertEqual(report["objects"]["create"]["raw_sha256"], sha256_json(self.raw[0]))
        self.assertEqual([item["raw_sha256"] for item in report["objects"]["trades"]], [sha256_json(trade) for trade in self.raw[1:]])


class DecimalHashingTests(unittest.TestCase):
    def test_decimal_precision_survives_load_and_hash(self) -> None:
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        path = directory / "raw.json"
        digits = "31.3160142901521860001"
        path.write_text('{"vSolInBondingCurve": ' + digits + '}', encoding="utf-8")
        value = load_json(path)
        self.assertEqual(str(value["vSolInBondingCurve"]), digits)
        self.assertIn(digits, canonical_json(value))
        self.assertNotEqual(canonical_json(value), canonical_json({"vSolInBondingCurve": Decimal("31.316014290152186")}))

    def test_binary_float_never_enters_hashing(self) -> None:
        with self.assertRaises(ContractError):
            canonical_json({"amount": 0.1})


if __name__ == "__main__":
    unittest.main()

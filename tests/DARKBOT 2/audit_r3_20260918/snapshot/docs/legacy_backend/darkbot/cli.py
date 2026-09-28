"""Phase-0 verification CLI: contract linter, migration linter, model-value validator,
frontend-contract check and fixture provenance gate.

Every configurable value this module checks (limits, ranges, feature lists, field
names) is read from the active contract at runtime; the tests prove that by tampering
with the contract and expecting failures. Structural literals are allowed and expected
(engineering_rules.static_allowed): the feature oracle implements the 17 formulas by
name, the migration runner names the schema_version table, and unit constants are
module constants.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sqlite3
import sys
import time
from dataclasses import dataclass, field
from decimal import ROUND_HALF_EVEN, Decimal, InvalidOperation
from pathlib import Path
from typing import Any, Callable, Iterator

# --------------------------------------------------------------------------- generic helpers


class ContractError(ValueError):
    """Raised for structural problems in JSON documents (duplicate keys, wrong shapes)."""


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ContractError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def load_json(path: Path) -> Any:
    """Load JSON with duplicate-key rejection and exact decimal numbers (never binary floats)."""
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle, object_pairs_hook=_reject_duplicate_keys, parse_float=Decimal)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_json(value: Any) -> str:
    """Deterministic serialization for content hashes: sorted keys, no whitespace, Decimal as number literal."""
    if isinstance(value, dict):
        return "{" + ",".join(f"{json.dumps(str(key), ensure_ascii=False)}:{canonical_json(value[key])}" for key in sorted(value)) + "}"
    if isinstance(value, list):
        return "[" + ",".join(canonical_json(item) for item in value) + "]"
    if isinstance(value, Decimal):
        return str(value)
    if isinstance(value, bool) or value is None or isinstance(value, (int, str)):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, float):
        raise ContractError("binary float reached canonical serialization; load JSON through load_json")
    raise ContractError(f"unsupported JSON value type {type(value).__name__}")


def sha256_json(value: Any) -> str:
    return hashlib.sha256(canonical_json(value).encode("utf-8")).hexdigest()


def get_path(document: Any, dotted: str) -> Any:
    """Resolve a dotted path such as 'system_states.states' inside a JSON document."""
    current = document
    for part in dotted.split("."):
        if isinstance(current, dict) and part in current:
            current = current[part]
        elif isinstance(current, list) and part.isdigit() and int(part) < len(current):
            current = current[int(part)]
        else:
            raise KeyError(dotted)
    return current


def walk(value: Any, path: str = "") -> Iterator[tuple[str, Any]]:
    """Yield (dotted_path, leaf_value) for every scalar in a JSON document."""
    if isinstance(value, dict):
        for key, item in value.items():
            yield from walk(item, f"{path}.{key}" if path else str(key))
    elif isinstance(value, list):
        for index, item in enumerate(value):
            yield from walk(item, f"{path}.{index}" if path else str(index))
    else:
        yield path, value


# --------------------------------------------------------------------------- active contract


@dataclass
class ActiveContract:
    project_root: Path
    path: Path
    sha256: str
    document: dict[str, Any]


POINTER_FILE_NAME_KEY = "document.active_pointer_file"


def find_project_root(start: Path | None = None) -> Path:
    """Walk upwards from `start` (default: this file) until a directory holds the pointer file.

    The pointer file name is not hard-coded; the first candidate that contains a
    `file=` line whose target is a JSON file naming the same pointer is accepted.
    """
    current = (start or Path(__file__)).resolve()
    if current.is_file():
        current = current.parent
    for candidate in (current, *current.parents):
        for entry in candidate.iterdir():
            if entry.is_file() and entry.suffix == "" and entry.name.isupper():
                pointer = _parse_pointer(entry)
                if pointer is not None and (candidate / pointer["file"]).is_file():
                    return candidate
    raise FileNotFoundError("no project root with an active-contract pointer found")


def _parse_pointer(path: Path) -> dict[str, str] | None:
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError):
        return None
    values: dict[str, str] = {}
    for line in lines:
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        values[key.strip()] = value.strip()
    if "file" not in values or "sha256" not in values:
        return None
    return values


def load_active_contract(project_root: Path) -> tuple[ActiveContract | None, list[str]]:
    errors: list[str] = []
    pointers = [
        entry
        for entry in project_root.iterdir()
        if entry.is_file() and entry.suffix == "" and entry.name.isupper() and _parse_pointer(entry)
    ]
    if len(pointers) != 1:
        return None, [f"expected exactly one active-contract pointer file in {project_root}, found {len(pointers)}"]
    pointer_path = pointers[0]
    pointer = _parse_pointer(pointer_path) or {}
    contract_path = project_root / pointer["file"]
    if not contract_path.is_file():
        return None, [f"pointer {pointer_path.name} references missing file {pointer['file']}"]
    actual_hash = sha256_file(contract_path)
    if actual_hash != pointer["sha256"]:
        errors.append(f"contract sha256 mismatch: pointer says {pointer['sha256'][:12]}..., file is {actual_hash[:12]}...")
    try:
        document = load_json(contract_path)
    except (OSError, json.JSONDecodeError, ContractError) as exc:
        return None, errors + [f"contract cannot be loaded: {exc}"]
    if not isinstance(document, dict):
        return None, errors + ["contract must be a JSON object"]
    declared_pointer = get_path(document, POINTER_FILE_NAME_KEY) if _has_path(document, POINTER_FILE_NAME_KEY) else None
    if declared_pointer != pointer_path.name:
        errors.append(f"contract declares pointer file {declared_pointer!r} but the pointer is {pointer_path.name!r}")
    return ActiveContract(project_root, contract_path, actual_hash, document), errors


def _has_path(document: Any, dotted: str) -> bool:
    try:
        get_path(document, dotted)
        return True
    except KeyError:
        return False


# --------------------------------------------------------------------------- enumerations


def resolve_enumerations(contract: dict[str, Any]) -> dict[str, list[str]]:
    """Resolve `enumerations`: each entry is a list, {"$ref": path} or {"$concat": [paths]}."""
    resolved: dict[str, list[str]] = {}
    for name, spec in contract.get("enumerations", {}).items():
        if isinstance(spec, list):
            values = list(spec)
        elif isinstance(spec, dict) and "$ref" in spec:
            values = list(get_path(contract, spec["$ref"]))
        elif isinstance(spec, dict) and "$concat" in spec:
            values = [item for ref in spec["$concat"] for item in get_path(contract, ref)]
        else:
            raise ContractError(f"enumeration {name} has an unsupported specification")
        if not values or not all(isinstance(item, str) for item in values):
            raise ContractError(f"enumeration {name} must be a non-empty list of strings")
        if len(set(values)) != len(values):
            raise ContractError(f"enumeration {name} contains duplicates")
        resolved[name] = values
    return resolved


# --------------------------------------------------------------------------- contract linter

VERSION_LITERAL = re.compile(r"\b\d+\.\d+\.\d+-MVP\b")


def validate_contract(contract: dict[str, Any], contract_path: Path | None = None) -> list[str]:
    """Check the internal invariants the contract declares about itself."""
    errors: list[str] = []

    def require(condition: bool, message: str) -> None:
        if not condition:
            errors.append(message)

    try:
        document = contract["document"]
        version = document["version"]
        require(isinstance(version, str) and VERSION_LITERAL.fullmatch(version) is not None, "document.version must look like MAJOR.MINOR.PATCH-MVP")
        rules = contract["phase0_rules"]
        require(document["status"] == rules["required_status"], "contract must carry the frozen status before Phase 0")
        if contract_path is not None:
            require(version.replace("-MVP", "") in contract_path.name, "document.version must appear in the contract file name")
            require(document["supersedes_for_implementation"] != contract_path.name, "a contract cannot supersede itself")
        changelog = contract["changelog"]
        require(isinstance(changelog, list) and changelog and changelog[-1]["version"] == version, "last changelog entry must match document.version")
        provenance_paths = [entry.get("path", "") for entry in contract["provenance"].values() if isinstance(entry, dict)]
        require(any(document["supersedes_for_implementation"] in path for path in provenance_paths), "superseded contract must be listed in provenance with its sha256")

        project_root = contract["project"]["project_root"]
        require(isinstance(project_root, str) and project_root.startswith("/"), "project.project_root must be absolute")
        exempt_prefixes = tuple(rules["absolute_path_exemptions"])
        for path, value in walk(contract):
            if isinstance(value, str) and value.startswith("/") and not path.startswith(exempt_prefixes):
                require(value.startswith(project_root), f"absolute path outside project_root at {path}")
        persistence = contract["persistence"]
        require(
            contract["technology"]["database_file"] == f"{project_root}/{persistence['database_relative_path']}",
            "technology.database_file must equal project_root/persistence.database_relative_path",
        )
        require(contract["repository_structure"]["root"] == project_root, "repository_structure.root must equal project.project_root")

        architecture = contract["architecture"]
        names = [item["name"] for item in architecture["main_classes"]]
        require(len(names) == architecture["main_class_count"], "number of main classes must equal architecture.main_class_count")
        require(len(set(names)) == len(names), "main classes must be unique")

        simplicity = contract["simplicity_contract"]
        require(simplicity["long_lived_processes"] == 1, "exactly one long-lived process")
        require(simplicity["databases"] == 1, "exactly one database")
        require(persistence["engine"].startswith(simplicity["database"]), "persistence.engine must match simplicity_contract.database")
        require("WAL" in persistence["engine"], "persistence engine must be WAL mode")
        forbidden = [item.lower() for item in simplicity["forbidden_runtime_components"]]
        for path, value in walk(contract["technology"]):
            if isinstance(value, str):
                for item in forbidden:
                    require(item not in value.lower(), f"forbidden runtime component {item!r} referenced at technology.{path}")
        require(not any(get_path(contract, f"simplicity_contract.{flag}") for flag in simplicity["must_be_false"]), "a simplicity flag that must be false is true")

        require(contract["project"]["ready_for_real_money"] is False, "real money must remain disabled")
        gates = contract["gates"]
        for gate in rules["gates_locked_at_phase0"]:
            require(gates[gate]["status"] == "LOCKED", f"gate {gate} must be LOCKED at Phase 0")
        allowed_status = set(rules["allowed_gate_statuses"])
        for gate, spec in gates.items():
            require(spec.get("status") in allowed_status, f"gate {gate} has an unknown status")
        require(
            contract["confirmed_user_decisions"]["frontend_host_target"].startswith("UNSET"),
            "public frontend host must remain unset before canary",
        )

        hard_rules = contract["hard_rules"]
        require(isinstance(hard_rules, list) and len(hard_rules) >= rules["minimum_hard_rules"], "hard_rules incomplete")
        require(all(isinstance(rule, str) and rule.strip() for rule in hard_rules), "hard_rules must be non-empty strings")

        tree = contract["repository_structure"]["tree"]
        require(isinstance(tree, list) and tree, "repository tree must not be empty")
        for entry in tree:
            require(isinstance(entry, dict) and {"path", "phase"} <= set(entry), "each tree entry needs path and phase")

        tables = persistence["required_tables"]
        require(len(set(tables)) == len(tables), "required_tables contains duplicates")
        require(set(persistence["required_columns"]) == set(tables), "required_columns must cover exactly required_tables")
        enumerations = resolve_enumerations(contract)
        for column, enum_name in persistence["state_columns"].items():
            table, _, name = column.partition(".")
            require(table in persistence["required_columns"] and name in persistence["required_columns"].get(table, []), f"state column {column} is not a required column")
            require(enum_name in enumerations, f"state column {column} references unknown enumeration {enum_name}")
        require(len(set(persistence["required_indexes"])) == len(persistence["required_indexes"]), "required_indexes contains duplicates")

        features = contract["feature_contract_v1"]
        tuple_len = len(features["definition_tuple"])
        for name, definition in features["definitions"].items():
            require(isinstance(definition, list) and len(definition) == tuple_len and all(isinstance(x, str) for x in definition), f"feature {name} must be a {tuple_len}-tuple of strings")
        oracle_features = list(contract["phase0_fixture_contract"]["oracle_features"])
        require(len(set(oracle_features)) == len(oracle_features), "oracle_features contains duplicates")
        require(set(oracle_features) == set(features["definitions"]), "every defined feature needs a Phase-0 oracle and vice versa (all 17 features are validated independently)")
        require(features["boolean_feature"] in features["definitions"], "boolean_feature must be a defined feature")

        errors.extend(_validate_model_schemas(contract))

        startup = contract["startup_self_check"]
        orders = [step["order"] for step in startup["steps"]]
        require(orders == list(range(1, len(orders) + 1)), "startup_self_check.steps must be numbered 1..n without gaps")
        order_by_code = {step["code"]: step["order"] for step in startup["steps"]}
        for first, second in startup["order_invariants"]:
            require(first in order_by_code and second in order_by_code and order_by_code[first] < order_by_code[second], f"startup_self_check: {first} must run before {second}")

        dependencies = contract["technology"]["runtime_dependencies"]
        require(isinstance(dependencies, list) and dependencies and all({"import_name", "pypi_name"} <= set(item) for item in dependencies), "technology.runtime_dependencies must list import_name and pypi_name per package")
        require(len({item["pypi_name"] for item in dependencies}) == len(dependencies), "technology.runtime_dependencies contains duplicate packages")

        model_kinds = set(enumerations["model_kinds"])
        for mode, kinds in persistence["required_models_by_mode"].items():
            require(mode in enumerations["modes"], f"required_models_by_mode lists unknown mode {mode}")
            require(set(kinds) <= model_kinds, f"required_models_by_mode.{mode} references unknown model kinds")

        exclusivity = contract["operating_modes"]["exclusivity"]
        for pair in exclusivity["forbidden_pairs"]:
            require(set(pair) <= set(enumerations["modes"]) and len(pair) == 2, f"operating_modes.exclusivity pair {pair} is not a pair of modes")

        registry = set(contract["blocking_user_decisions"]["unset_registry"])
        found_unset = {path for path, value in walk(contract) if isinstance(value, str) and value.startswith("UNSET")}
        for path in sorted(found_unset - registry):
            errors.append(f"UNSET value at {path} is not registered in blocking_user_decisions.unset_registry")
        for path in sorted(registry - found_unset):
            errors.append(f"unset_registry lists {path} but no UNSET value exists there")

        engineering = contract["engineering_rules"]
        for flag in rules["engineering_flags_that_must_be_true"]:
            require(engineering.get(flag) is True, f"engineering_rules.{flag} must be true")

        secret_errors = find_secrets(contract, "contract")
        errors.extend(secret_errors)
    except (KeyError, TypeError, ValueError, ContractError) as exc:
        errors.append(f"missing or malformed contract field: {exc!r}")
    return errors


# --------------------------------------------------------------------------- model value schemas

QUANTILE_KEYS = ("p50", "p95", "p99")
CROSS_FIELD_SUM = "sum_equals"


def _resolve_bound(value: Any, contract: dict[str, Any]) -> Any:
    """A schema bound is a number or {"$ref": "<contract path>"} so that a range exists exactly once in the contract."""
    if isinstance(value, dict) and set(value) == {"$ref"}:
        return get_path(contract, value["$ref"])
    return value


def _is_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _is_decimal_like(value: Any) -> bool:
    return _is_int(value) or isinstance(value, Decimal) or (isinstance(value, str) and _is_money_safe(value))


def _check_scalar(value: Any, spec: dict[str, Any], contract: dict[str, Any], label: str) -> list[str]:
    """Type and range of one scalar (or one list item) against a field spec."""
    kind = spec["type"]
    errors: list[str] = []
    if kind == "int":
        if not _is_int(value):
            return [f"{label} must be int, got {type(value).__name__}"]
        number: Decimal | int = value
    elif kind == "decimal":
        if not _is_decimal_like(value):
            return [f"{label} must be a decimal (exact JSON number or decimal string), got {value!r}"]
        number = Decimal(value)
    elif kind == "str":
        if not isinstance(value, str):
            return [f"{label} must be str"]
        if "enum" in spec and value not in spec["enum"]:
            errors.append(f"{label} must be one of {spec['enum']}, got {value!r}")
        if "pattern" in spec and re.fullmatch(spec["pattern"], value) is None:
            errors.append(f"{label} does not match {spec['pattern']}")
        return errors
    elif kind == "bool":
        return [] if isinstance(value, bool) else [f"{label} must be bool"]
    elif kind == "object":
        return [] if isinstance(value, dict) else [f"{label} must be an object"]
    else:
        return [f"{label}: unsupported schema type {kind!r}"]
    for bound_key, exclusive_key, comparison in (("min", "min_exclusive", lambda a, b: a < b), ("max", "max_exclusive", lambda a, b: a > b)):
        if bound_key not in spec:
            continue
        bound = Decimal(_resolve_bound(spec[bound_key], contract))
        exclusive = bool(spec.get(exclusive_key, False))
        if comparison(number, bound) or (exclusive and number == bound):
            errors.append(f"{label} = {value} violates {bound_key}{' (exclusive)' if exclusive else ''} {bound}")
    return errors


def _check_field(value: Any, spec: dict[str, Any], contract: dict[str, Any], label: str) -> list[str]:
    if value is None:
        return [] if spec.get("nullable", False) else [f"{label} must not be null"]
    kind = spec["type"]
    if kind.startswith("list[") and kind.endswith("]"):
        if not isinstance(value, list):
            return [f"{label} must be a list"]
        item_spec = dict(spec, type=kind[5:-1])
        errors: list[str] = []
        for index, item in enumerate(value):
            errors.extend(_check_scalar(item, item_spec, contract, f"{label}[{index}]"))
        return errors
    if kind == "quantiles":
        if not isinstance(value, dict) or tuple(value) != QUANTILE_KEYS:
            return [f"{label} must be an object with exactly the keys {list(QUANTILE_KEYS)} in order"]
        errors = []
        for key in QUANTILE_KEYS:
            if not _is_int(value[key]) or value[key] < 0:
                errors.append(f"{label}.{key} must be a non-negative int")
        if not errors and not (value["p50"] <= value["p95"] <= value["p99"]):
            errors.append(f"{label} must satisfy p50 <= p95 <= p99")
        return errors
    return _check_scalar(value, spec, contract, label)


def _check_cross_constraints(constraints: list[Any], content: dict[str, Any], label: str) -> list[str]:
    errors: list[str] = []
    for constraint in constraints:
        if len(constraint) >= 3 and constraint[-2] == CROSS_FIELD_SUM:
            names, target = constraint[:-2], Decimal(constraint[-1])
            values = [content.get(name) for name in names]
            if all(value is not None for value in values):
                total = sum((Decimal(value) for value in values), Decimal(0))
                if total != target:
                    errors.append(f"{label}: {' + '.join(names)} = {total}, must equal {target}")
            continue
        left, operator, right = constraint
        a, b = content.get(left), content.get(right)
        if a is None or b is None:
            continue
        a, b = Decimal(a), Decimal(b)
        ok = a < b if operator == "<" else a <= b if operator == "<=" else None
        if ok is None:
            errors.append(f"{label}: unsupported cross-field operator {operator!r}")
        elif not ok:
            errors.append(f"{label}: {left} ({a}) must be {operator} {right} ({b})")
    return errors


def validate_typed_content(fields: dict[str, Any], constraints: list[Any], content: Any, contract: dict[str, Any], label: str) -> list[str]:
    """Generic typed-value check: exact key set, per-field type/range/nullability, cross-field constraints."""
    if not isinstance(content, dict):
        return [f"{label} must be a JSON object"]
    errors: list[str] = []
    missing, extra = sorted(set(fields) - set(content)), sorted(set(content) - set(fields))
    if missing:
        errors.append(f"{label} missing fields {missing}")
    if extra:
        errors.append(f"{label} has unknown fields {extra}")
    for name, spec in fields.items():
        if name not in content:
            continue
        value = content[name]
        if isinstance(value, str) and value.startswith("UNSET"):
            errors.append(f"{label}.{name} is UNSET")
            continue
        errors.extend(_check_field(value, spec, contract, f"{label}.{name}"))
    if not errors:
        errors.extend(_check_cross_constraints(constraints, content, label))
    return errors


def validate_model_content(kind: str, content: Any, contract: dict[str, Any]) -> list[str]:
    """model_versions.content_json for `kind` against model_content_schemas (used by `model add` and the startup self-check)."""
    schemas = contract["model_content_schemas"]
    if kind not in schemas or kind in ("rule", "spec_keys", "cross_field_constraints"):
        return [f"unknown model kind {kind!r}"]
    return validate_typed_content(schemas[kind], schemas["cross_field_constraints"].get(kind, []), content, contract, kind)


def validate_strategy_configuration(content: Any, contract: dict[str, Any]) -> list[str]:
    """strategy_versions.configuration_json against strategy_version_schema, including the typed filter rules."""
    schema = contract["strategy_version_schema"]
    errors = validate_typed_content(schema["fields"], schema["cross_field_constraints"], content, contract, "strategy")
    if not isinstance(content, dict) or not isinstance(content.get("filters"), list):
        if isinstance(content, dict) and "filters" in content and content["filters"] is not None:
            errors.append("strategy.filters must be a list of filter rules")
        return errors
    strategy = contract["strategy"]
    features = contract["feature_contract_v1"]
    filterable = [name for name in features["definitions"] if name != features["boolean_feature"]]
    filters = content["filters"]
    if len(filters) > strategy["max_filters"]:
        errors.append(f"strategy.filters has {len(filters)} rules, maximum is {strategy['max_filters']}")
    seen: set[str] = set()
    for index, rule in enumerate(filters):
        label = f"strategy.filters[{index}]"
        errors.extend(validate_typed_content(schema["filter_rule_fields"], [], rule, contract, label))
        if not isinstance(rule, dict):
            continue
        feature = rule.get("feature")
        if feature not in filterable:
            errors.append(f"{label}.feature {feature!r} is not a filterable feature")
        elif feature in seen:
            errors.append(f"{label}.feature {feature!r} is filtered twice")
        seen.add(str(feature))
        low, high = rule.get("min"), rule.get("max")
        if low is not None and high is not None and _is_decimal_like(low) and _is_decimal_like(high) and Decimal(low) > Decimal(high):
            errors.append(f"{label}: min {low} > max {high}")
    if isinstance(content.get("observation_ms"), int) and isinstance(content.get("snapshot_ms"), list):
        expected = sorted(set(strategy["runtime_snapshot_ms_v1"]) | {content["observation_ms"]})
        if content["snapshot_ms"] != expected:
            errors.append(f"strategy.snapshot_ms must be {expected} (runtime_snapshot_rule)")
    return errors


def _validate_model_schemas(contract: dict[str, Any]) -> list[str]:
    """The schemas themselves: shape of every field spec, and the contract's own initial limits satisfy them."""
    errors: list[str] = []
    schemas = contract["model_content_schemas"]
    spec_keys = schemas["spec_keys"]
    required, optional, types = set(spec_keys["required"]), set(spec_keys["optional"]), set(spec_keys["types"])
    operators = set(spec_keys["cross_field_operators"])
    enumerations = resolve_enumerations(contract)
    schema_sets = {kind: schemas[kind] for kind in enumerations["model_kinds"] if kind in schemas}
    for kind in enumerations["model_kinds"]:
        if kind not in schemas:
            errors.append(f"model_content_schemas lacks a schema for {kind}")
    strategy_schema = contract["strategy_version_schema"]
    schema_sets["strategy_version_schema.fields"] = strategy_schema["fields"]
    schema_sets["strategy_version_schema.filter_rule_fields"] = strategy_schema["filter_rule_fields"]
    for kind, fields in schema_sets.items():
        if not isinstance(fields, dict) or not fields:
            errors.append(f"schema {kind} must be a non-empty object of field specs")
            continue
        for name, spec in fields.items():
            label = f"{kind}.{name}"
            if not isinstance(spec, dict) or not required <= set(spec):
                errors.append(f"{label} spec lacks {sorted(required)}")
                continue
            if not set(spec) <= required | optional:
                errors.append(f"{label} spec has unknown keys {sorted(set(spec) - required - optional)}")
            base = spec["type"][5:-1] if spec["type"].startswith("list[") else spec["type"]
            if spec["type"] not in types or (spec["type"].startswith("list[") and base not in types):
                errors.append(f"{label} has unknown type {spec['type']!r}")
            for bound in ("min", "max"):
                if bound in spec:
                    try:
                        Decimal(_resolve_bound(spec[bound], contract))
                    except (KeyError, InvalidOperation, TypeError, ValueError):
                        errors.append(f"{label}.{bound} is not a number or resolvable $ref")
            if "enum" in spec and (not isinstance(spec["enum"], list) or not spec["enum"]):
                errors.append(f"{label}.enum must be a non-empty list")
    for kind, constraints in schemas["cross_field_constraints"].items():
        fields = schemas.get(kind, {})
        for constraint in constraints:
            names = constraint[:-2] if constraint[-2] == CROSS_FIELD_SUM else (constraint[0], constraint[2])
            operator = constraint[-2] if constraint[-2] == CROSS_FIELD_SUM else constraint[1]
            if operator not in operators or not all(name in fields for name in names):
                errors.append(f"cross_field_constraints.{kind}: {constraint} references unknown field or operator")
    limits = contract["operational_limits_v1"]["keys"]
    if set(limits) != set(schemas.get("OPERATIONAL_LIMITS", {})):
        errors.append("operational_limits_v1.keys must equal model_content_schemas.OPERATIONAL_LIMITS")
    else:
        for name, spec in limits.items():
            if spec.get("type") != schemas["OPERATIONAL_LIMITS"][name]["type"] or spec.get("unit") != schemas["OPERATIONAL_LIMITS"][name]["unit"]:
                errors.append(f"operational_limits_v1.keys.{name} type/unit differ from the schema")
        errors.extend(validate_model_content("OPERATIONAL_LIMITS", {name: spec["initial"] for name, spec in limits.items()}, contract))
    feed = contract["market_data_source_v1"]["feed_pricing_observed"]
    if not _is_int(feed["lamports_per_paid_message"]) or feed["lamports_per_paid_message"] < 0:
        errors.append("feed_pricing_observed.lamports_per_paid_message must be a non-negative int")
    return errors


# --------------------------------------------------------------------------- secrets

SECRET_KEY_NAMES = {"private_key", "privatekey", "seed", "seed_phrase", "mnemonic", "api_key", "apikey", "authorization", "secret", "password"}
SECRET_VALUE_PATTERNS = [re.compile(pattern, re.IGNORECASE) for pattern in (r"api[-_]?key\s*=", r"\bbearer\s+[a-z0-9._-]{16,}")]


def find_secrets(value: Any, label: str) -> list[str]:
    """Report secret-looking keys and values (query-string keys, bearer tokens).

    Shape alone cannot separate a base58 private key from a transaction signature, so
    raw payload values are not pattern-matched; key names and URL secrets are.
    """
    errors: list[str] = []
    for path, leaf in walk(value):
        last = path.rsplit(".", 1)[-1].lower()
        if last in SECRET_KEY_NAMES:
            errors.append(f"{label}: forbidden secret-like key at {path}")
        if isinstance(leaf, str) and any(pattern.search(leaf) for pattern in SECRET_VALUE_PATTERNS):
            errors.append(f"{label}: secret-like value at {path}")
    return errors


# --------------------------------------------------------------------------- migration linter

CHECK_ENUM = re.compile(r"(\w+)\s+TEXT(?:\s+NOT NULL)?(?:\s+DEFAULT\s+\S+)?\s+CHECK\s*\(\s*\1\s+IN\s*\(([^)]*)\)\s*\)", re.IGNORECASE)


TRANSACTION_STATEMENT = re.compile(r"^\s*(BEGIN|COMMIT|END|ROLLBACK)\b", re.IGNORECASE | re.MULTILINE)


def _sql_literal(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def apply_migration(connection: sqlite3.Connection, sql: str, contract_version: str, migration_name: str, applied_at_utc_ns: int | None = None) -> None:
    """Migration runner: DDL and the schema_version row are one atomic transaction.

    The migration file contains DDL only (no BEGIN/COMMIT); the runner owns the
    transaction, so a crash between the DDL and the schema_version insert leaves
    either both or nothing. Values are inlined as validated literals because the
    whole script must run inside one executescript transaction.
    """
    if TRANSACTION_STATEMENT.search(sql):
        raise ContractError(f"migration {migration_name} contains BEGIN/COMMIT; the runner owns the transaction")
    if VERSION_LITERAL.fullmatch(contract_version) is None:
        raise ContractError(f"contract version {contract_version!r} is not a version string")
    version = int(migration_name.split("_", 1)[0])
    applied_at = applied_at_utc_ns if applied_at_utc_ns is not None else time.time_ns()
    insert = (
        "INSERT INTO schema_version (version, contract_version, migration_name, migration_sha256, applied_at_utc_ns) VALUES "
        f"({version}, {_sql_literal(contract_version)}, {_sql_literal(migration_name)}, {_sql_literal(hashlib.sha256(sql.encode('utf-8')).hexdigest())}, {applied_at});"
    )
    script = "BEGIN IMMEDIATE;\n" + sql.rstrip().rstrip(";") + ";\n" + insert + "\nCOMMIT;\n"
    try:
        connection.executescript(script)
    except sqlite3.Error:
        if connection.in_transaction:
            connection.execute("ROLLBACK")
        raise


def validate_migration(sql_path: Path, contract: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    try:
        sql = sql_path.read_text(encoding="utf-8")
    except OSError as exc:
        return [f"migration cannot be read: {exc}"]
    persistence = contract["persistence"]
    if VERSION_LITERAL.search(sql):
        errors.append("migration contains a contract version literal; schema_version rows are written by the runner")
    if re.search(r"\bINSERT\s+INTO\b", sql, re.IGNORECASE):
        errors.append("migration must not insert data rows")
    if TRANSACTION_STATEMENT.search(sql):
        errors.append("migration must not contain BEGIN/COMMIT; DDL and schema_version are one transaction owned by the runner")
    errors.extend(_check_migration_atomicity(sql, contract, sql_path.name))
    connection = sqlite3.connect(":memory:")
    try:
        connection.execute("PRAGMA foreign_keys = ON")
        try:
            apply_migration(connection, sql, contract["document"]["version"], sql_path.name, applied_at_utc_ns=0)
        except (sqlite3.Error, ContractError) as exc:
            return errors + [f"migration failed to apply: {exc}"]
        rows = connection.execute("SELECT version, contract_version FROM schema_version").fetchall()
        if rows != [(int(sql_path.name.split('_', 1)[0]), contract["document"]["version"])]:
            errors.append(f"schema_version row unexpected: {rows}")

        actual_tables = {row[0] for row in connection.execute("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")}
        expected_tables = set(persistence["required_tables"])
        if actual_tables != expected_tables:
            errors.append(f"tables differ: missing={sorted(expected_tables - actual_tables)}, extra={sorted(actual_tables - expected_tables)}")

        for table, columns in persistence["required_columns"].items():
            if table not in actual_tables:
                continue
            actual_columns = [row[1] for row in connection.execute(f"PRAGMA table_info({table})")]
            if actual_columns != list(columns):
                errors.append(f"columns of {table} differ from contract: {actual_columns} != {list(columns)}")

        enumerations = resolve_enumerations(contract)
        table_sql = {row[0]: row[1] for row in connection.execute("SELECT name, sql FROM sqlite_master WHERE type = 'table'")}
        for column, enum_name in persistence["state_columns"].items():
            table, _, name = column.partition(".")
            found = None
            for match in CHECK_ENUM.finditer(table_sql.get(table, "")):
                if match.group(1) == name:
                    found = {item.strip().strip("'") for item in match.group(2).split(",")}
            expected = set(enumerations[enum_name])
            if found is None:
                errors.append(f"{column} has no CHECK ... IN (...) constraint")
            elif found != expected:
                errors.append(f"{column} CHECK values differ from enumeration {enum_name}: {sorted(found ^ expected)}")

        actual_indexes = {row[0] for row in connection.execute("SELECT name FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL")}
        expected_indexes = set(persistence["required_indexes"])
        if actual_indexes != expected_indexes:
            errors.append(f"indexes differ: missing={sorted(expected_indexes - actual_indexes)}, extra={sorted(actual_indexes - expected_indexes)}")

        errors.extend(_run_constraint_probes(connection, contract))
    finally:
        connection.close()
    return errors


def _check_migration_atomicity(sql: str, contract: dict[str, Any], migration_name: str) -> list[str]:
    """Prove runner + file are atomic: a failing statement after the DDL must leave no table and no schema_version row."""
    if TRANSACTION_STATEMENT.search(sql):
        return []
    connection = sqlite3.connect(":memory:")
    try:
        broken = sql.rstrip().rstrip(";") + ";\nCREATE TABLE schema_version (duplicate_definition INTEGER);"
        try:
            apply_migration(connection, broken, contract["document"]["version"], migration_name, applied_at_utc_ns=0)
        except sqlite3.Error:
            leftovers = connection.execute("SELECT count(*) FROM sqlite_master").fetchone()[0]
            return [] if leftovers == 0 and not connection.in_transaction else [f"migration is not atomic: {leftovers} objects survived a failed run"]
        return ["migration atomicity probe did not fail as expected"]
    finally:
        connection.close()


def _run_constraint_probes(connection: sqlite3.Connection, contract: dict[str, Any]) -> list[str]:
    """Prove that the schema rejects what the contract forbids. Every probe is rolled back."""
    errors: list[str] = []
    probes: list[dict[str, Any]] = contract["persistence"]["constraint_probes"]
    for probe in probes:
        connection.execute(f"PRAGMA foreign_keys = {'ON' if probe.get('foreign_keys', True) else 'OFF'}")
        connection.execute("SAVEPOINT probe")
        rejected = False
        try:
            for statement in probe["setup"]:
                connection.execute(statement)
            for statement in probe["must_fail"]:
                try:
                    connection.execute(statement)
                except sqlite3.IntegrityError:
                    rejected = True
        except sqlite3.Error as exc:
            errors.append(f"probe {probe['name']} setup failed: {exc}")
        finally:
            connection.execute("ROLLBACK TO probe")
            connection.execute("RELEASE probe")
        if not rejected and not any(error.startswith(f"probe {probe['name']}") for error in errors):
            errors.append(f"probe {probe['name']} was accepted by the schema")
    connection.execute("PRAGMA foreign_keys = ON")
    return errors


# --------------------------------------------------------------------------- fixture gate


@dataclass
class NormalizedEvent:
    raw_sha256: str
    receive_offset_ms: Decimal
    event_type: str
    actor: str | None
    quote_lamports: int | None
    token_base_units: int | None
    virtual_sol_reserves: int | None
    virtual_token_reserves: int | None


@dataclass
class FixtureBundle:
    manifest: dict[str, Any]
    create: dict[str, Any]
    trades: list[dict[str, Any]]
    migration: dict[str, Any]
    raw_hashes: set[str] = field(default_factory=set)


def _is_money_safe(value: Any) -> bool:
    """Money and reserve values must be int, Decimal (exact JSON number) or decimal strings; binary floats are rejected."""
    if isinstance(value, bool):
        return False
    if isinstance(value, (int, Decimal)):
        return True
    if isinstance(value, str):
        try:
            Decimal(value)
            return True
        except InvalidOperation:
            return False
    return False


def _to_decimal(value: Any) -> Decimal:
    return Decimal(value) if not isinstance(value, float) else Decimal(repr(value))


def validate_fixtures(root: Path, contract: dict[str, Any], allow_synthetic: bool = False) -> list[str]:
    spec = contract["phase0_fixture_contract"]
    provider_dir = root / spec["provider_dir"]
    feature_dir = root / spec["feature_dir"]
    files = spec["provider_files"]
    manifest_path = provider_dir / spec["manifest_file"]
    cases_path = feature_dir / spec["feature_cases_file"]
    required = [manifest_path, cases_path, *(provider_dir / files[key] for key in ("create", "trades", "migration"))]
    missing = [str(path.relative_to(root)) for path in required if not path.is_file()]
    if missing:
        return ["missing real fixtures: " + ", ".join(missing)]

    errors: list[str] = []
    try:
        manifest = load_json(manifest_path)
        create = load_json(provider_dir / files["create"])
        trades = load_json(provider_dir / files["trades"])
        migration = load_json(provider_dir / files["migration"])
    except (OSError, json.JSONDecodeError, ContractError) as exc:
        return [f"invalid fixture file: {exc}"]
    if not isinstance(manifest, dict):
        return ["manifest must be an object"]
    bundle = FixtureBundle(manifest, create if isinstance(create, dict) else {}, trades if isinstance(trades, list) else [], migration if isinstance(migration, dict) else {})
    errors.extend(_validate_manifest(bundle, provider_dir, contract, allow_synthetic))
    if errors:
        return errors
    errors.extend(_validate_raw_payloads(bundle, contract))
    if errors:
        return errors
    errors.extend(_validate_feature_cases(cases_path, manifest_path, bundle, contract))
    errors.extend(find_secrets({"manifest": manifest, "create": create, "trades": trades, "migration": migration}, "fixtures"))
    return errors


def _validate_manifest(bundle: FixtureBundle, provider_dir: Path, contract: dict[str, Any], allow_synthetic: bool) -> list[str]:
    errors: list[str] = []
    spec = contract["phase0_fixture_contract"]
    manifest = bundle.manifest
    for name in spec["manifest_required_fields"]:
        if name not in manifest or manifest[name] in ("", None):
            errors.append(f"manifest missing {name}")
    if manifest.get("synthetic_selftest") not in (False, None) and not allow_synthetic:
        errors.append("manifest is marked synthetic_selftest; synthetic captures never satisfy the Phase-0 gate")
    if manifest.get("provider") != spec["provider"]:
        errors.append(f"manifest provider must be {spec['provider']!r}")
    errors.extend(_validate_warmup(manifest.get("warmup_measurement"), spec, contract))
    if manifest.get("offsets_time_basis") != spec["offsets_time_basis_value"]:
        errors.append(f"manifest offsets_time_basis must be {spec['offsets_time_basis_value']!r}: relative capture times come from time.monotonic_ns only")
    feed_cost = manifest.get("feed_cost")
    if not isinstance(feed_cost, dict) or set(feed_cost) != set(spec["feed_cost_fields"]):
        errors.append(f"manifest feed_cost must define exactly {spec['feed_cost_fields']}")
    else:
        for name in spec["feed_cost_fields"]:
            if name == "spend_cap_sol":
                if not _is_money_safe(feed_cost[name]) or Decimal(feed_cost[name]) < 0:
                    errors.append("manifest feed_cost.spend_cap_sol must be a non-negative decimal")
            elif not _is_int(feed_cost[name]) or feed_cost[name] < 0:
                errors.append(f"manifest feed_cost.{name} must be a non-negative integer")
        if not any(error.startswith("manifest feed_cost") for error in errors):
            lamports_per_sol = Decimal(spec["lamports_per_sol"])
            allowed = int(Decimal(feed_cost["spend_cap_sol"]) * lamports_per_sol // Decimal(feed_cost["lamports_per_paid_message"])) if feed_cost["lamports_per_paid_message"] else feed_cost["paid_message_limit"]
            if feed_cost["paid_message_limit"] > allowed:
                errors.append(f"manifest feed_cost.paid_message_limit {feed_cost['paid_message_limit']} exceeds the spend cap ({allowed} paid messages)")
            if feed_cost["paid_messages_at_unsubscribe"] > feed_cost["paid_message_limit"]:
                errors.append("manifest feed_cost.paid_messages_at_unsubscribe exceeds paid_message_limit: the paid subscription was not stopped at the limit")
            if feed_cost["paid_message_count"] < feed_cost["paid_messages_at_unsubscribe"]:
                errors.append("manifest feed_cost.paid_message_count cannot be below paid_messages_at_unsubscribe")
            if feed_cost["estimated_cost_lamports"] != feed_cost["paid_message_count"] * feed_cost["lamports_per_paid_message"]:
                errors.append("manifest feed_cost.estimated_cost_lamports must equal paid_message_count * lamports_per_paid_message")
    if manifest.get("trade_subscription_stop_reason") not in spec["trade_subscription_stop_reasons"]:
        errors.append(f"manifest trade_subscription_stop_reason must be one of {spec['trade_subscription_stop_reasons']}")
    if manifest.get("capture_stop_reason") not in spec["capture_stop_reasons"]:
        errors.append(f"manifest capture_stop_reason must be one of {spec['capture_stop_reasons']}")
    if manifest.get("migration_status") not in spec["migration_statuses"]:
        errors.append(f"manifest migration_status must be one of {spec['migration_statuses']}")
    if not isinstance(manifest.get("migration_mint_matches_capture"), bool):
        errors.append("manifest migration_mint_matches_capture must be a bool")
    validity = manifest.get("capture_validity")
    check_names = [name for name in spec["capture_validity_checks"] if name != "rule"]
    if not isinstance(validity, dict) or validity.get("valid") is not True or not isinstance(validity.get("checks"), dict) or set(validity["checks"]) != set(check_names):
        errors.append(f"manifest capture_validity must report valid=true with every check in {check_names}")
    elif not all(validity["checks"][name] is True for name in check_names):
        errors.append("manifest capture_validity lists a failed check; the capture must not pass the gate")
    field_map = manifest.get("field_map")
    if not isinstance(field_map, dict) or set(field_map) != set(spec["field_map_keys"]):
        errors.append(f"manifest field_map must define exactly {spec['field_map_keys']}")
    tx_values = manifest.get("tx_type_values")
    if not isinstance(tx_values, dict) or set(tx_values) != set(spec["tx_type_keys"]):
        errors.append(f"manifest tx_type_values must define exactly {spec['tx_type_keys']}")
    declared = manifest.get("files")
    if not isinstance(declared, dict):
        errors.append("manifest files must map file names to sha256 hashes")
    else:
        for key, name in spec["provider_files"].items():
            actual = sha256_file(provider_dir / name)
            if declared.get(name) != actual:
                errors.append(f"manifest hash mismatch for {name} ({key})")
    if errors:
        return errors
    shape = {name: sorted(_field_names(getattr(bundle, key))) for key, name in spec["provider_files"].items()}
    expected_schema_version = sha256_json(shape)
    if manifest.get("schema_version") != expected_schema_version:
        errors.append(f"manifest schema_version must be the sha256 of the payload field-name shape: {expected_schema_version}")
    return errors


def _field_names(value: Any) -> set[str]:
    if isinstance(value, dict):
        return set(value)
    if isinstance(value, list):
        names: set[str] = set()
        for item in value:
            if isinstance(item, dict):
                names |= set(item)
        return names
    return set()


def _validate_warmup(warmup: Any, spec: dict[str, Any], contract: dict[str, Any]) -> list[str]:
    """warmup_measurement: monotonic offsets are mandatory ints; ack fields stay null until the contract proves an ack semantic."""
    fields = spec["warmup_measurement_fields"]
    if not isinstance(warmup, dict) or set(warmup) != set(fields):
        return [f"manifest warmup_measurement must define exactly {fields}"]
    errors: list[str] = []
    ack_proven = contract["market_data_source_v1"]["subscription_coverage"]["subscription_ack_proven"] is True
    for name in fields:
        value = warmup[name]
        if name in ("subscribe_ack_offset_ns", "ack_message_raw"):
            if not ack_proven and value is not None:
                errors.append(f"manifest warmup_measurement.{name} must be null while subscription_ack_proven=false (ack_rule)")
            elif ack_proven and (value is None or (name == "ack_message_raw" and not isinstance(value, str)) or (name == "subscribe_ack_offset_ns" and not _is_int(value))):
                errors.append(f"manifest warmup_measurement.{name} must be set once subscription_ack_proven=true")
        elif name == "ack_candidates_raw":
            if not isinstance(value, list) or not all(isinstance(item, dict) and {"offset_ns", "utc_ns", "raw"} <= set(item) for item in value):
                errors.append("manifest warmup_measurement.ack_candidates_raw must be a list of {offset_ns, utc_ns, raw}")
        elif name.endswith("_offset_ns"):
            if not _is_int(value) or value < 0:
                errors.append(f"manifest warmup_measurement.{name} must be a non-negative integer (monotonic offset from t0)")
        elif not _is_int(value):
            errors.append(f"manifest warmup_measurement.{name} must be an integer")
    if not errors and warmup["first_trade_received_offset_ns"] < warmup["subscribe_sent_offset_ns"]:
        errors.append("manifest warmup_measurement: first trade cannot precede subscribe_sent")
    return errors


def classify_migration_frame(obj: Any, field_map: dict[str, str], tx_values: dict[str, str], contract: dict[str, Any]) -> str | None:
    """The one migration rule (market_data_source_v1.migration_detection_v1), shared by capture.py and the fixture gate.

    Returns a migration_statuses value or None when the frame is not a migration candidate at all.
    """
    if not isinstance(obj, dict) or field_map["mint"] not in obj or not obj.get(field_map["signature"]):
        return None
    if obj.get(field_map["tx_type"]) in set(tx_values.values()):
        return None
    detection = contract["market_data_source_v1"]["migration_detection_v1"]
    statuses = contract["phase0_fixture_contract"]["migration_statuses"]
    if detection["status"] == "VALIDATED" and detection["validated_required_fields"]:
        marker = detection["validated_marker"]
        if all(name in obj for name in detection["validated_required_fields"]) and (marker is None or obj.get(marker["field"]) == marker["value"]):
            return statuses[0]
    return statuses[1]


def _validate_raw_payloads(bundle: FixtureBundle, contract: dict[str, Any]) -> list[str]:
    spec = contract["phase0_fixture_contract"]
    errors: list[str] = []
    manifest = bundle.manifest
    field_map = manifest["field_map"]
    tx_values = manifest["tx_type_values"]
    mint_key, type_key, signature_key = field_map["mint"], field_map["tx_type"], field_map["signature"]
    capture_mint = manifest.get("capture_mint")
    if not bundle.create:
        errors.append("create fixture must be one non-empty raw provider object")
    else:
        if bundle.create.get(mint_key) != capture_mint:
            errors.append("create fixture mint does not match manifest capture_mint")
        if bundle.create.get(type_key) != tx_values["create"]:
            errors.append("create fixture tx_type is not the create value")
        bundle.raw_hashes.add(sha256_json(bundle.create))
    if len(bundle.trades) < spec["minimum_trade_events"]:
        errors.append(f"trades fixture needs at least {spec['minimum_trade_events']} raw trade objects of one coin")
    sides: set[str] = set()
    for index, trade in enumerate(bundle.trades):
        if not isinstance(trade, dict) or not trade:
            errors.append(f"trade {index} must be a non-empty object")
            continue
        if trade.get(mint_key) != capture_mint:
            errors.append(f"trade {index} mint does not match capture_mint")
        side = trade.get(type_key)
        if side not in (tx_values["buy"], tx_values["sell"]):
            errors.append(f"trade {index} has tx_type {side!r} outside buy/sell")
        else:
            sides.add(side)
        if not trade.get(signature_key):
            errors.append(f"trade {index} has no transaction signature")
        for amount_key in (field_map["sol_amount"], field_map["token_amount"], field_map["v_sol_reserves"], field_map["v_token_reserves"]):
            if amount_key not in trade:
                errors.append(f"trade {index} lacks field {amount_key}")
        bundle.raw_hashes.add(sha256_json(trade))
    if bundle.trades and sides != {tx_values["buy"], tx_values["sell"]}:
        errors.append("trades fixture must contain at least one buy and one sell of the captured coin")
    status = classify_migration_frame(bundle.migration, field_map, tx_values, contract)
    declared = manifest.get("migration_status")
    if not bundle.migration:
        errors.append("migration fixture must be one non-empty raw provider object")
    elif status is None:
        errors.append("migration fixture does not satisfy migration_detection_v1.candidate_rule (mint + signature, no create/buy/sell tx_type)")
    elif declared != status:
        errors.append(f"manifest migration_status {declared!r} does not match the contract rule ({status})")
    elif bundle.migration.get(mint_key) != capture_mint and manifest.get("migration_mint_matches_capture") is not False:
        errors.append("migration fixture is for another mint; manifest must state migration_mint_matches_capture=false")
    elif bundle.migration.get(mint_key) == capture_mint and manifest.get("migration_mint_matches_capture") is not True:
        errors.append("migration fixture is for the captured mint; manifest must state migration_mint_matches_capture=true")
    return errors


def _validate_feature_cases(cases_path: Path, manifest_path: Path, bundle: FixtureBundle, contract: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    spec = contract["phase0_fixture_contract"]
    features = contract["feature_contract_v1"]
    try:
        document = load_json(cases_path)
    except (OSError, json.JSONDecodeError, ContractError) as exc:
        return [f"invalid feature fixtures: {exc}"]
    if not isinstance(document, dict):
        return ["feature_cases must be an object"]
    if set(document) != set(spec["feature_cases_document_fields"]):
        errors.append(f"feature_cases must define exactly {spec['feature_cases_document_fields']}")
    if document.get("feature_set_version") != features["version"]:
        errors.append(f"feature_cases feature_set_version must be {features['version']}")
    if document.get("source_manifest_sha256") != sha256_file(manifest_path):
        errors.append("feature_cases source_manifest_sha256 does not match the provider manifest")
    curve_inputs, curve_errors = _parse_curve_inputs(document.get("curve_inputs"), spec)
    errors.extend(curve_errors)
    cases = document.get("cases")
    if not isinstance(cases, list) or not cases:
        return errors + ["feature_cases.cases must be a non-empty array"]
    deadlines = set(contract["strategy"]["runtime_snapshot_ms_v1"]) | set(contract["strategy"]["research_replay_snapshot_ms"])
    feature_names = set(features["definitions"])
    coverage_states = set(resolve_enumerations(contract)["coverage_states"])
    covered: dict[int, int] = {deadline: 0 for deadline in deadlines}
    for index, case in enumerate(cases):
        label = f"case {case.get('case_id', index)!r}" if isinstance(case, dict) else f"case {index}"
        if not isinstance(case, dict) or not case.get("case_id"):
            errors.append(f"{label} must be an object with case_id")
            continue
        if set(case) != set(spec["feature_case_fields"]):
            errors.append(f"{label} must define exactly {spec['feature_case_fields']}")
            continue
        deadline = case.get("deadline_ms")
        if deadline not in deadlines:
            errors.append(f"{label} deadline_ms {deadline!r} is not a contract snapshot time")
            continue
        if case.get("coverage_state_at_deadline") not in coverage_states or not isinstance(case.get("data_quality_interval_intersects"), bool):
            errors.append(f"{label} needs coverage_state_at_deadline from coverage_states and a bool data_quality_interval_intersects")
            continue
        events, event_errors = _parse_normalized_events(case.get("normalized_events"), bundle, contract, label, curve_inputs)
        errors.extend(event_errors)
        expected = case.get("expected")
        if not isinstance(expected, dict) or set(expected) != feature_names:
            errors.append(f"{label} expected must contain exactly the contract features")
            continue
        null_reasons = case.get("null_reasons", {})
        for name, value in expected.items():
            if value is None and not null_reasons.get(name):
                errors.append(f"{label} feature {name} is null without a null_reason")
            if value is not None and name != features["boolean_feature"] and not _is_money_safe(value):
                errors.append(f"{label} feature {name} must be int or decimal string, not {type(value).__name__}")
            if name == features["boolean_feature"] and not isinstance(value, bool):
                errors.append(f"{label} feature {name} must be a bool")
        if not isinstance(case.get("derivation"), str) or not case["derivation"].strip():
            errors.append(f"{label} needs a derivation note describing the independent expected-value source")
        if event_errors or curve_errors:
            continue
        if events:
            covered[deadline] += 1
        errors.extend(_check_oracle(events, Decimal(deadline), expected, contract, label, curve_inputs, case))
    for deadline, count in covered.items():
        if count == 0:
            errors.append(f"deadline {deadline}ms has no case with normalized events")
    return errors


def _parse_curve_inputs(raw: Any, spec: dict[str, Any]) -> tuple[dict[str, Any], list[str]]:
    """curve_inputs carry the R/C-feature inputs with provenance; every number is int or null, never assumed."""
    fields = [name for name in spec["curve_inputs_fields"] if name != "rule"]
    if not isinstance(raw, dict) or set(raw) != set(fields):
        return {}, [f"feature_cases.curve_inputs must define exactly {fields}"]
    errors: list[str] = []
    for name in fields:
        value = raw[name]
        if name.endswith("_source"):
            if not isinstance(value, str) or not value.strip():
                errors.append(f"curve_inputs.{name} must be a non-empty provenance text")
        elif value is not None and (not _is_int(value) or value < 0):
            errors.append(f"curve_inputs.{name} must be a non-negative integer or null")
    decimals = raw.get("token_decimals")
    if decimals is not None and _is_int(decimals) and not (0 <= decimals <= 18):
        errors.append("curve_inputs.token_decimals must be within 0..18")
    if decimals is None and raw.get("create_virtual_token_reserves") is not None:
        errors.append("curve_inputs.create_virtual_token_reserves must be null while token_decimals is null")
    return ({} if errors else dict(raw)), errors


def _parse_normalized_events(raw: Any, bundle: FixtureBundle, contract: dict[str, Any], label: str, curve_inputs: dict[str, Any] | None = None) -> tuple[list[NormalizedEvent], list[str]]:
    errors: list[str] = []
    events: list[NormalizedEvent] = []
    if not isinstance(raw, list):
        return events, [f"{label} needs normalized_events as an array"]
    enumerations = resolve_enumerations(contract)
    event_types = set(enumerations[contract["phase0_fixture_contract"]["event_type_enumeration"]])
    decimals_unknown = curve_inputs is not None and curve_inputs.get("token_decimals") is None
    for index, item in enumerate(raw):
        if not isinstance(item, dict):
            errors.append(f"{label} normalized event {index} must be an object")
            continue
        raw_hash = item.get("raw_sha256")
        if raw_hash not in bundle.raw_hashes:
            errors.append(f"{label} normalized event {index} does not reference a raw fixture object by sha256")
        event_type = item.get("event_type")
        if event_type not in event_types:
            errors.append(f"{label} normalized event {index} has event_type {event_type!r} outside the contract enumeration")
        for key in ("quote_lamports", "token_base_units", "virtual_sol_reserves", "virtual_token_reserves"):
            if item.get(key) is not None and not _is_int(item[key]):
                errors.append(f"{label} normalized event {index}.{key} must be an integer of base units")
        if decimals_unknown and (item.get("token_base_units") is not None or item.get("virtual_token_reserves") is not None):
            errors.append(f"{label} normalized event {index}: token amounts must be null while curve_inputs.token_decimals is null (no assumed decimals)")
        offset = item.get("receive_offset_ms")
        if not _is_money_safe(offset):
            errors.append(f"{label} normalized event {index}.receive_offset_ms must be int or decimal string")
            continue
        events.append(
            NormalizedEvent(
                str(raw_hash), Decimal(offset), str(event_type), item.get("actor"), item.get("quote_lamports"), item.get("token_base_units"),
                item.get("virtual_sol_reserves"), item.get("virtual_token_reserves"),
            )
        )
    return events, errors


def _check_oracle(
    events: list[NormalizedEvent], deadline_ms: Decimal, expected: dict[str, Any], contract: dict[str, Any], label: str,
    curve_inputs: dict[str, Any] | None = None, case: dict[str, Any] | None = None,
) -> list[str]:
    """Independent reference for all 17 features (E from events, R/C from curve_inputs, Q from the case). features.py must never import this."""
    spec = contract["phase0_fixture_contract"]
    places = spec["ratio_decimal_places"]
    lamports_per_sol = Decimal(spec["lamports_per_sol"])
    buy_type, sell_type = spec["oracle_event_types"]["buy"], spec["oracle_event_types"]["sell"]
    quantum = Decimal(1).scaleb(-places)
    curve = curve_inputs or {}
    observed_state = resolve_enumerations(contract)["coverage_states"][1]

    def ratio(numerator: Decimal, denominator: Decimal) -> Decimal:
        return (numerator / denominator).quantize(quantum, rounding=ROUND_HALF_EVEN)

    def price(vsol: int | None, vtok: int | None) -> Decimal | None:
        if vsol is None or vtok is None or vsol <= 0 or vtok <= 0:
            return None
        return ratio(Decimal(vsol), Decimal(vtok))

    visible = sorted((event for event in events if event.receive_offset_ms <= deadline_ms), key=lambda event: event.receive_offset_ms)
    buys = [event for event in visible if event.event_type == buy_type]
    sells = [event for event in visible if event.event_type == sell_type]
    buy_volume = sum((Decimal(event.quote_lamports or 0) for event in buys), Decimal(0))
    sell_volume = sum((Decimal(event.quote_lamports or 0) for event in sells), Decimal(0))
    per_buyer: dict[str, Decimal] = {}
    first_buyers: list[str] = []
    for event in buys:
        actor = event.actor or ""
        per_buyer[actor] = per_buyer.get(actor, Decimal(0)) + Decimal(event.quote_lamports or 0)
        if actor not in first_buyers:
            first_buyers.append(actor)

    price_t0 = price(curve.get("create_virtual_sol_reserves"), curve.get("create_virtual_token_reserves"))
    reserve_events = [event for event in visible if event.virtual_sol_reserves is not None and event.virtual_token_reserves is not None]
    last_vsol, last_vtok = (reserve_events[-1].virtual_sol_reserves, reserve_events[-1].virtual_token_reserves) if reserve_events else (curve.get("create_virtual_sol_reserves"), curve.get("create_virtual_token_reserves"))
    price_t = price(last_vsol, last_vtok)

    def price_change() -> Decimal | None:
        if price_t0 is None or price_t is None:
            return None
        return (price_t / price_t0 - 1).quantize(quantum, rounding=ROUND_HALF_EVEN)

    def max_drawdown() -> Decimal | None:
        if price_t0 is None:
            return None
        peak, worst = price_t0, Decimal(0)
        for event in reserve_events:
            current = price(event.virtual_sol_reserves, event.virtual_token_reserves)
            if current is None:
                return None
            peak = max(peak, current)
            worst = max(worst, (1 - current / peak).quantize(quantum, rounding=ROUND_HALF_EVEN))
        return worst

    def dev_initial_buy() -> Decimal | None:
        initial_buy, supply = curve.get("initial_buy_token_base_units"), curve.get("token_total_supply")
        return None if initial_buy is None or not supply else ratio(Decimal(initial_buy), Decimal(supply))

    def curve_progress_pct() -> Decimal | None:
        initial_virtual, initial_real = curve.get("initial_virtual_token_reserves"), curve.get("initial_real_token_reserves")
        if last_vtok is None or initial_virtual is None or not initial_real:
            return None
        real = Decimal(last_vtok - (initial_virtual - initial_real))
        return (100 * (1 - real / Decimal(initial_real))).quantize(quantum, rounding=ROUND_HALF_EVEN)

    def research_impact() -> Decimal | None:
        quote = curve.get("research_quote_amount_lamports")
        if quote is None or price_t is None or last_vsol is None or last_vtok is None:
            return None
        tokens_out = last_vtok - (last_vsol * last_vtok) // (last_vsol + quote)
        if tokens_out <= 0:
            return None
        average = ratio(Decimal(quote), Decimal(tokens_out))
        return (average / price_t - 1).quantize(quantum, rounding=ROUND_HALF_EVEN)

    def provider_gap_flag() -> bool:
        if case is None:
            return True
        return case["coverage_state_at_deadline"] != observed_state or bool(case["data_quality_interval_intersects"])

    oracle: dict[str, Callable[[], Any]] = {
        "buy_count": lambda: len(buys),
        "sell_count": lambda: len(sells),
        "buy_volume_sol": lambda: buy_volume / lamports_per_sol,
        "sell_volume_sol": lambda: sell_volume / lamports_per_sol,
        "net_buy_flow_sol": lambda: (buy_volume - sell_volume) / lamports_per_sol,
        "unique_buyers": lambda: len(per_buyer),
        "unique_buyer_ratio": lambda: ratio(Decimal(len(per_buyer)), Decimal(len(buys))) if buys else None,
        "largest_buy_sol": lambda: max(Decimal(event.quote_lamports or 0) for event in buys) / lamports_per_sol if buys else None,
        "buyer_volume_concentration": lambda: sum((ratio(volume, buy_volume) ** 2 for volume in per_buyer.values()), Decimal(0)).quantize(quantum, rounding=ROUND_HALF_EVEN) if buy_volume else None,
        "first5_buyer_concentration": lambda: ratio(sum((per_buyer[buyer] for buyer in first_buyers[: spec["first_buyer_window"]]), Decimal(0)), buy_volume) if buy_volume else None,
        "first_sell_latency_ms": lambda: sells[0].receive_offset_ms if sells else None,
        "price_change": price_change,
        "max_drawdown": max_drawdown,
        "dev_initial_buy": dev_initial_buy,
        "curve_progress_pct": curve_progress_pct,
        "research_estimated_price_impact": research_impact,
        "provider_gap_flag": provider_gap_flag,
    }
    errors: list[str] = []
    for name in spec["oracle_features"]:
        if name not in oracle:
            errors.append(f"{label}: no oracle implementation for {name}")
            continue
        reference = oracle[name]()
        given = expected.get(name)
        if reference is None and given is None:
            continue
        if reference is None or given is None:
            errors.append(f"{label} feature {name}: oracle says {reference!r}, fixture says {given!r}")
            continue
        try:
            if isinstance(reference, bool):
                same = given is reference
            elif isinstance(reference, int):
                same = int(given) == reference
            else:
                same = Decimal(reference) == _to_decimal(given)
        except (InvalidOperation, ValueError, TypeError):
            same = False
        if not same:
            errors.append(f"{label} feature {name}: oracle {reference} != fixture {given}")
    return errors


# --------------------------------------------------------------------------- repository checks


def validate_pyproject_dependencies(pyproject_path: Path, contract: dict[str, Any]) -> list[str]:
    """[project].dependencies must name exactly technology.runtime_dependencies (pypi names); checked before `uv lock`."""
    import tomllib

    try:
        data = tomllib.loads(pyproject_path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as exc:
        return [f"pyproject.toml cannot be read: {exc}"]
    declared: list[str] = []
    for requirement in data.get("project", {}).get("dependencies", []):
        name = re.split(r"[\s<>=!~\[;]", requirement.strip(), maxsplit=1)[0].lower()
        declared.append(name)
    expected = [item["pypi_name"].lower() for item in contract["technology"]["runtime_dependencies"]]
    errors: list[str] = []
    for name in expected:
        if name not in declared:
            errors.append(f"pyproject.toml lacks runtime dependency {name} (technology.runtime_dependencies)")
    for name in declared:
        if name not in expected:
            errors.append(f"pyproject.toml declares {name}, which technology.runtime_dependencies does not allow")
    if len(set(declared)) != len(declared):
        errors.append("pyproject.toml declares a dependency twice")
    requires = data.get("project", {}).get("requires-python")
    if requires != contract["technology"]["python_requirement"]:
        errors.append(f"pyproject.toml requires-python {requires!r} differs from technology.python_requirement")
    return errors


def validate_repository(active: ActiveContract) -> list[str]:
    """Phase-0 files exist, pyproject dependencies match the contract, no version literals in code, lockfile present."""
    errors: list[str] = []
    contract = active.document
    root = active.project_root
    for entry in contract["repository_structure"]["tree"]:
        if entry["phase"] != 0:
            continue
        path = root / entry["path"]
        if entry["path"].endswith("/") or entry.get("captured"):
            continue
        if entry.get("generated"):
            if not path.is_file():
                errors.append(f"generated file missing: {entry['path']} ({entry.get('how', 'see contract')})")
            continue
        if not path.is_file():
            errors.append(f"phase-0 file missing: {entry['path']}")
    code_root = root / contract["repository_structure"]["backend_root"]
    contract_name_stem = contract["document"]["name"]
    for path in sorted(code_root.rglob("*.py")):
        text = path.read_text(encoding="utf-8")
        if VERSION_LITERAL.search(text):
            errors.append(f"version literal in code: {path.relative_to(root)}")
        if contract_name_stem + "_v" in text:
            errors.append(f"contract file name literal in code: {path.relative_to(root)}")
    pyproject = code_root / "pyproject.toml"
    if pyproject.is_file():
        errors.extend(validate_pyproject_dependencies(pyproject, contract))
    lock = root / contract["technology"]["dependency_lock"]
    if lock.is_file():
        head = lock.read_text(encoding="utf-8", errors="replace")[:200]
        if "version" not in head:
            errors.append("dependency lock file does not look like a uv lock file")
    return errors


# --------------------------------------------------------------------------- frontend contract check

PIPE_LIST = re.compile(r"^[A-Z0-9_]+(?:\s*\|\s*[A-Z0-9_]+)+$")


def load_frontend_contract(active: ActiveContract) -> tuple[dict[str, Any] | None, list[str]]:
    path = active.project_root / active.document["frontend_contract"]["specification_file"]
    if not path.is_file():
        return None, [f"frontend contract file missing: {path.name}"]
    try:
        document = load_json(path)
    except (OSError, json.JSONDecodeError, ContractError) as exc:
        return None, [f"frontend contract cannot be loaded: {exc}"]
    if not isinstance(document, dict):
        return None, ["frontend contract must be a JSON object"]
    return document, []


def validate_frontend_contract(frontend: dict[str, Any], contract: dict[str, Any]) -> list[str]:
    """Machine check of the frontend spec against the backend contract via its declared backend_bindings."""
    errors: list[str] = []

    def require(condition: bool, message: str) -> None:
        if not condition:
            errors.append(message)

    try:
        bindings = frontend["backend_bindings"]
        enumerations = resolve_enumerations(contract)
        require(frontend["document"]["belongs_to"] == contract["document"]["name"], "frontend contract must belong to the MVP contract")
        for frontend_path, spec in bindings["enumeration_bindings"].items():
            value = get_path(frontend, frontend_path)
            if not isinstance(value, str) or PIPE_LIST.match(value.replace("null", "NULL")) is None:
                errors.append(f"frontend {frontend_path} must be a 'A | B | C' list to bind an enumeration")
                continue
            tokens = [token.strip() for token in value.split("|")]
            allowed = set(enumerations[spec["enumeration"]]) | set(spec.get("extra", []))
            unknown = [token for token in tokens if token not in allowed]
            require(not unknown, f"frontend {frontend_path} uses values outside enumerations.{spec['enumeration']}: {unknown}")
            if spec.get("complete", False):
                require(set(tokens) >= set(enumerations[spec["enumeration"]]), f"frontend {frontend_path} must list every value of enumerations.{spec['enumeration']}")
        risk_fields = get_path(frontend, bindings["risk_policy_fields"])
        risk_schema = [name for name in contract["model_content_schemas"]["RISK_POLICY"]]
        require([name for name in risk_fields if name != "rule"] == risk_schema, "frontend risk policy fields must equal model_content_schemas.RISK_POLICY in order")
        filter_fields = get_path(frontend, bindings["filter_fields"])
        require(set(filter_fields) == set(contract["strategy_version_schema"]["filter_rule_fields"]), "frontend filter rule fields must equal strategy_version_schema.filter_rule_fields (no operator strings)")
        alerts = get_path(frontend, bindings["alerts_fields"])
        require(set(alerts) == set(bindings["alerts_expected_fields"]), f"frontend alert rows must expose exactly {bindings['alerts_expected_fields']}")
        require(set(bindings["alerts_expected_fields"]) - {"code", "message"} <= set(contract["persistence"]["required_columns"]["system_events"]), "alert fields other than code/message must be system_events columns")
        feature_reference = get_path(frontend, bindings["feature_columns"])
        require(isinstance(feature_reference, str) and "feature_contract_v1" in feature_reference, "frontend feature columns must reference feature_contract_v1, never a copied list")
        for path in bindings["forbidden_fields"]:
            require(not _has_path(frontend, path), f"frontend still declares removed field {path}")
        for path, expected_value in bindings["fixed_values"].items():
            require(_has_path(frontend, path) and get_path(frontend, path) == expected_value, f"frontend {path} must be {expected_value!r}")
        require(frontend["document"]["version"] in contract["frontend_contract"]["specification_file"], "frontend document.version must appear in the specification file name")
        require(frontend["changelog"][-1]["version"] == frontend["document"]["version"], "frontend changelog must end with document.version")
        pages = [page.lower() for page in frontend["document"]["pages"]]
        require(not any("scanner" in page and "ehemals" not in page for page in pages), "no scanner page: Launches + OBSERVING filter suffices")
        strategy_fields = get_path(frontend, bindings["strategy_exit_fields"])
        require(not ({"exit_retry_rule", "max_consecutive_losses"} & set(strategy_fields)), "exit retry and loss streak live in RISK_POLICY only")
    except (KeyError, TypeError, ValueError, ContractError) as exc:
        errors.append(f"missing or malformed frontend contract field: {exc!r}")
    return errors


# --------------------------------------------------------------------------- CLI


def _print_result(name: str, errors: list[str]) -> None:
    print(f"{name}: {'PASS' if not errors else 'FAIL'}")
    for error in errors:
        print(f"  - {error}")


def phase0_check(project_root: Path | None = None) -> int:
    root = project_root or find_project_root()
    active, pointer_errors = load_active_contract(root)
    _print_result("ACTIVE_CONTRACT", pointer_errors)
    if active is None:
        print("PHASE_0 NOT_READY")
        return 1
    contract = active.document
    contract_errors = validate_contract(contract, active.path)
    _print_result("CONTRACT", contract_errors)
    if contract_errors:
        print("PHASE_0 NOT_READY")
        return 1
    migration_errors = validate_migration(root / contract["persistence"]["initial_migration"], contract)
    _print_result("MIGRATION", migration_errors)
    repository_errors = validate_repository(active)
    _print_result("REPOSITORY", repository_errors)
    frontend, frontend_errors = load_frontend_contract(active)
    if frontend is not None:
        frontend_errors.extend(validate_frontend_contract(frontend, contract))
    _print_result("FRONTEND_CONTRACT", frontend_errors)
    fixture_errors = validate_fixtures(root / contract["phase0_fixture_contract"]["fixture_root"], contract)
    _print_result("REAL_FIXTURES", fixture_errors)
    ready = not (pointer_errors or migration_errors or repository_errors or frontend_errors or fixture_errors)
    print("PHASE_0 PASS" if ready else "PHASE_0 NOT_READY")
    return 0 if ready else 1


def fixture_hashes(project_root: Path | None = None, fixtures_root: Path | None = None) -> dict[str, Any]:
    """The single hash definition for fixtures: file hashes, schema_version and raw_sha256 per raw object."""
    root = project_root or find_project_root()
    active, errors = load_active_contract(root)
    if active is None:
        raise ContractError("; ".join(errors))
    spec = active.document["phase0_fixture_contract"]
    provider_dir = (fixtures_root or root / spec["fixture_root"]) / spec["provider_dir"]
    files = spec["provider_files"]
    result: dict[str, Any] = {"files": {}, "objects": {}, "schema_version": None}
    shape: dict[str, list[str]] = {}
    for key, name in files.items():
        path = provider_dir / name
        if not path.is_file():
            result["files"][name] = None
            continue
        result["files"][name] = sha256_file(path)
        value = load_json(path)
        shape[name] = sorted(_field_names(value))
        if isinstance(value, list):
            result["objects"][key] = [{"index": index, "raw_sha256": sha256_json(item)} for index, item in enumerate(value)]
        else:
            result["objects"][key] = {"raw_sha256": sha256_json(value)}
    if len(shape) == len(files):
        result["schema_version"] = sha256_json(shape)
    manifest_path = provider_dir / spec["manifest_file"]
    result["manifest_sha256"] = sha256_file(manifest_path) if manifest_path.is_file() else None
    return result


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="darkbot")
    subparsers = parser.add_subparsers(dest="command", required=True)
    phase0 = subparsers.add_parser("phase0-check", help="validate active contract, schema, repository and real fixtures")
    phase0.add_argument("--project-root", type=Path, default=None, help="defaults to the directory holding the active-contract pointer")
    hashes = subparsers.add_parser("fixture-hashes", help="print file hashes, schema_version and raw_sha256 of every raw fixture object")
    hashes.add_argument("--project-root", type=Path, default=None)
    hashes.add_argument("--fixtures", type=Path, default=None, help="fixture root; defaults to the contract's fixture_root")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if args.command == "phase0-check":
        return phase0_check(args.project_root)
    if args.command == "fixture-hashes":
        print(json.dumps(fixture_hashes(args.project_root, args.fixtures), indent=2))
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())

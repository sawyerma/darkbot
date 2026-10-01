"""Phase-0 fixture capture: records one real coin sequence from the provider WebSocket.

Provider field names and tx-type values are command-line arguments, never code. Messages
are stored byte-for-byte; hashes and the manifest are produced with the same functions the
Phase-0 gate uses (cli.load_json, cli.sha256_json, cli.sha256_file), so the capture is the
single origin of every fixture value.

Guarantees (phase0_fixture_contract.capture_rules):
- the paid subscription is closed in the same loop iteration in which the paid-message
  limit (from capture_spend_cap_sol) or the trade window is reached;
- all relative times come from time.monotonic_ns(); UTC is audit only;
- no frame is interpreted as a subscription ack; unknown frames are ack candidates;
- migration frames are classified by the contract's migration_detection_v1 rule only;
- EXIT_OK is returned only for a capture that satisfies capture_validity_checks.

The protocol logic lives in CaptureSession (pure, testable offline); capture() only does I/O.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import time
from dataclasses import dataclass, field
from decimal import Decimal
from pathlib import Path
from typing import Any

from darkbot.cli import ContractError, classify_migration_frame, find_project_root, load_active_contract, load_json, sha256_file, sha256_json

EXIT_OK, EXIT_NO_CREATE, EXIT_NO_MIGRATION, EXIT_TOO_FEW_TRADES, EXIT_USAGE, EXIT_CONNECTION = 0, 2, 3, 4, 5, 6
NANOSECONDS_PER_MILLISECOND = 1_000_000
NANOSECONDS_PER_SECOND = 1_000_000_000

# Stop reasons and check names are stable enum-like values; test_capture proves they are the contract's.
SUB_STOP_PAID_LIMIT, SUB_STOP_WINDOW, SUB_STOP_CONNECTION, SUB_STOP_CAPTURE_ENDED, SUB_STOP_NEVER = (
    "PAID_MESSAGE_LIMIT", "TRADE_WINDOW_ELAPSED", "CONNECTION_CLOSED", "CAPTURE_ENDED", "NOT_SUBSCRIBED",
)
STOP_DONE, STOP_MIGRATION_WAIT, STOP_CREATE_WAIT, STOP_CONNECTION, STOP_MIGRATION_ONLY = (
    "TRADES_AND_MIGRATION_DONE", "MIGRATION_WAIT_ELAPSED", "CREATE_WAIT_ELAPSED", "CONNECTION_CLOSED", "MIGRATION_ONLY_DONE",
)
MIGRATION_NOT_OBSERVED = "NOT_OBSERVED"
CHECK_CREATE, CHECK_MIN_TRADES, CHECK_SIDES, CHECK_WARMUP, CHECK_HASHES, CHECK_MIGRATION = (
    "VALID_CREATE", "MINIMUM_TRADES", "BUY_AND_SELL", "WARMUP_MEASURED", "RAW_HASHES", "MIGRATION_STATUS_EXPLICIT",
)
KIND_CREATE, KIND_BUY, KIND_SELL, KIND_MIGRATION_CANDIDATE, KIND_OTHER = "create", "buy", "sell", "migration_candidate", "other"


@dataclass(frozen=True)
class CaptureConfig:
    url: str
    out: Path
    tx_type_field: str
    create_value: str
    buy_value: str
    sell_value: str
    mint_field: str
    pool_field: str
    pool_value: str
    signature_field: str
    trader_field: str
    sol_field: str
    token_field: str
    vsol_field: str
    vtoken_field: str
    spend_cap_sol: Decimal
    lamports_per_paid_message: int
    lamports_per_sol: int
    max_paid_messages: int
    minimum_trade_events: int
    create_wait_seconds: int
    trade_seconds: int
    migration_wait_seconds: int
    ping_interval_ms: int
    migration_only: bool

    def field_map(self) -> dict[str, str]:
        return {
            "mint": self.mint_field,
            "tx_type": self.tx_type_field,
            "signature": self.signature_field,
            "trader": self.trader_field,
            "sol_amount": self.sol_field,
            "token_amount": self.token_field,
            "v_sol_reserves": self.vsol_field,
            "v_token_reserves": self.vtoken_field,
        }

    def tx_type_values(self) -> dict[str, str]:
        return {"create": self.create_value, "buy": self.buy_value, "sell": self.sell_value}


def max_paid_messages_from_cap(spend_cap_sol: Decimal, lamports_per_sol: int, lamports_per_paid_message: int) -> int:
    """floor(cap_sol * lamports_per_sol / lamports_per_paid_message); the only place the cap becomes a message count."""
    if lamports_per_paid_message <= 0:
        raise ContractError("lamports_per_paid_message must be positive to derive a paid-message limit")
    return int(Decimal(spend_cap_sol) * Decimal(lamports_per_sol) // Decimal(lamports_per_paid_message))


@dataclass
class Captured:
    text: str
    utc_ns: int
    mono_ns: int
    obj: dict[str, Any]


def classify(obj: Any, config: CaptureConfig, contract: dict[str, Any]) -> str:
    """create | buy | sell | migration_candidate | other; migration candidates follow the contract's migration_detection_v1 rule."""
    if not isinstance(obj, dict):
        return KIND_OTHER
    tx_type = obj.get(config.tx_type_field)
    if tx_type == config.create_value:
        return KIND_CREATE
    if tx_type == config.buy_value:
        return KIND_BUY
    if tx_type == config.sell_value:
        return KIND_SELL
    if classify_migration_frame(obj, config.field_map(), config.tx_type_values(), contract) is not None:
        return KIND_MIGRATION_CANDIDATE
    return KIND_OTHER


def parse(text: str) -> dict[str, Any] | None:
    try:
        value = json.loads(text, parse_float=Decimal)
    except json.JSONDecodeError:
        return None
    return value if isinstance(value, dict) else None


def _frame(method: str, keys: list[str] | None = None) -> str:
    payload: dict[str, Any] = {"method": method}
    if keys is not None:
        payload["keys"] = keys
    return json.dumps(payload)


@dataclass
class CaptureSession:
    """The capture protocol without I/O: feed messages and clock ticks, collect outbound frames."""

    config: CaptureConfig
    contract: dict[str, Any]
    started_mono_ns: int
    create: Captured | None = None
    migration: Captured | None = None
    trades: list[Captured] = field(default_factory=list)
    ack_candidates: list[Captured] = field(default_factory=list)
    subscribe_sent_utc_ns: int = 0
    subscribe_sent_mono_ns: int = 0
    subscribed: bool = False
    paid_message_count: int = 0
    paid_messages_at_unsubscribe: int = 0
    trade_subscription_stop_reason: str = SUB_STOP_NEVER
    capture_stop_reason: str | None = None

    # ------------------------------------------------------------------ inputs
    def initial_frames(self) -> list[str]:
        frames = [_frame("subscribeMigration")]
        if not self.config.migration_only:
            frames.append(_frame("subscribeNewToken"))
        return frames

    def on_message(self, text: str, utc_ns: int, mono_ns: int) -> list[str]:
        obj = parse(text)
        kind = classify(obj, self.config, self.contract) if obj is not None else KIND_OTHER
        config = self.config
        if kind == KIND_MIGRATION_CANDIDATE:
            if self.migration is None:
                self.migration = Captured(text, utc_ns, mono_ns, obj or {})
            if config.migration_only:
                self.capture_stop_reason = STOP_MIGRATION_ONLY
                return []
            return self._finish_if_complete()
        if config.migration_only:
            return []
        if self.create is None:
            if kind == KIND_CREATE and obj is not None and obj.get(config.pool_field) == config.pool_value and obj.get(config.mint_field):
                self.create = Captured(text, utc_ns, mono_ns, obj)
                self.subscribe_sent_utc_ns = utc_ns
                self.subscribe_sent_mono_ns = mono_ns
                self.subscribed = True
                return [_frame("subscribeTokenTrade", [obj[config.mint_field]])]
            return []
        if kind in (KIND_BUY, KIND_SELL) and obj is not None:
            frames: list[str] = []
            if self.subscribed or self.subscribe_sent_mono_ns:
                self.paid_message_count += 1  # every buy/sell frame after subscribe is billed, stored or not
            if self.subscribed and obj.get(config.mint_field) == self.create.obj.get(config.mint_field):
                self.trades.append(Captured(text, utc_ns, mono_ns, obj))
            if self.subscribed and self.paid_message_count >= config.max_paid_messages:
                frames.extend(self.stop_trade_subscription(SUB_STOP_PAID_LIMIT))
            return frames + self._finish_if_complete()
        if kind == KIND_OTHER and self.subscribe_sent_mono_ns and mono_ns >= self.subscribe_sent_mono_ns:
            self.ack_candidates.append(Captured(text, utc_ns, mono_ns, obj or {}))
        return []

    def on_tick(self, mono_ns: int) -> list[str]:
        """Time-based transitions; called when the receive timeout fires."""
        config = self.config
        elapsed_ns = mono_ns - self.started_mono_ns
        frames: list[str] = []
        if self.subscribed and mono_ns - self.subscribe_sent_mono_ns >= config.trade_seconds * NANOSECONDS_PER_SECOND:
            frames.extend(self.stop_trade_subscription(SUB_STOP_WINDOW))
        if self.create is None and not config.migration_only and elapsed_ns >= config.create_wait_seconds * NANOSECONDS_PER_SECOND:
            self.capture_stop_reason = STOP_CREATE_WAIT
            return frames
        if elapsed_ns >= config.migration_wait_seconds * NANOSECONDS_PER_SECOND:
            frames.extend(self.stop_trade_subscription(SUB_STOP_CAPTURE_ENDED))
            self.capture_stop_reason = STOP_MIGRATION_WAIT
            return frames
        return frames + self._finish_if_complete()

    def on_connection_closed(self) -> None:
        if self.subscribed:
            self.subscribed = False
            self.paid_messages_at_unsubscribe = self.paid_message_count
            self.trade_subscription_stop_reason = SUB_STOP_CONNECTION
        self.capture_stop_reason = STOP_CONNECTION

    def timeout_seconds(self, mono_ns: int) -> float:
        """Seconds until the next time-based transition (never negative)."""
        config = self.config
        deadlines = [self.started_mono_ns + config.migration_wait_seconds * NANOSECONDS_PER_SECOND]
        if self.create is None and not config.migration_only:
            deadlines.append(self.started_mono_ns + config.create_wait_seconds * NANOSECONDS_PER_SECOND)
        if self.subscribed:
            deadlines.append(self.subscribe_sent_mono_ns + config.trade_seconds * NANOSECONDS_PER_SECOND)
        return max(0.0, (min(deadlines) - mono_ns) / NANOSECONDS_PER_SECOND)

    def done(self) -> bool:
        return self.capture_stop_reason is not None

    # ------------------------------------------------------------------ transitions
    def stop_trade_subscription(self, reason: str) -> list[str]:
        if not self.subscribed or self.create is None:
            return []
        self.subscribed = False
        self.paid_messages_at_unsubscribe = self.paid_message_count
        self.trade_subscription_stop_reason = reason
        return [_frame("unsubscribeTokenTrade", [self.create.obj[self.config.mint_field]])]

    def _finish_if_complete(self) -> list[str]:
        if self.create is not None and not self.subscribed and self.migration is not None and self.capture_stop_reason is None:
            self.capture_stop_reason = STOP_DONE
        return []

    # ------------------------------------------------------------------ results
    def migration_status(self) -> str:
        if self.migration is None:
            return MIGRATION_NOT_OBSERVED
        status = classify_migration_frame(self.migration.obj, self.config.field_map(), self.config.tx_type_values(), self.contract)
        return status if status is not None else MIGRATION_NOT_OBSERVED

    def assess(self) -> tuple[dict[str, Any], int]:
        """capture_validity block and the exit code; EXIT_OK only when every contract check passes."""
        config = self.config
        sides = {trade.obj.get(config.tx_type_field) for trade in self.trades}
        migration_status = self.migration_status()
        checks = {
            CHECK_CREATE: self.create is not None,
            CHECK_MIN_TRADES: len(self.trades) >= config.minimum_trade_events,
            CHECK_SIDES: {config.buy_value, config.sell_value} <= sides,
            CHECK_WARMUP: bool(self.subscribe_sent_mono_ns) and bool(self.trades),
            CHECK_HASHES: self.create is not None,
            CHECK_MIGRATION: migration_status != MIGRATION_NOT_OBSERVED,
        }
        failed = [name for name, passed in checks.items() if not passed]
        validity = {"valid": not failed, "checks": checks, "failed": failed}
        if not checks[CHECK_CREATE]:
            return validity, EXIT_NO_CREATE
        if not (checks[CHECK_MIN_TRADES] and checks[CHECK_SIDES] and checks[CHECK_WARMUP]):
            return validity, EXIT_TOO_FEW_TRADES
        if not checks[CHECK_MIGRATION]:
            return validity, EXIT_NO_MIGRATION
        if self.capture_stop_reason == STOP_CONNECTION:
            return validity, EXIT_CONNECTION
        return validity, EXIT_OK

    def summary(self, exit_code: int, validity: dict[str, Any]) -> dict[str, Any]:
        return {
            "exit_code": exit_code,
            "valid": validity["valid"],
            "failed_checks": validity["failed"],
            "capture_stop_reason": self.capture_stop_reason,
            "trade_subscription_stop_reason": self.trade_subscription_stop_reason,
            "trades": len(self.trades),
            "paid_message_count": self.paid_message_count,
            "paid_message_limit": self.config.max_paid_messages,
            "migration_status": self.migration_status(),
        }


def write_manifest(session: CaptureSession, provider_files: dict[str, str], manifest_name: str, validity: dict[str, Any]) -> Path:
    """Write raw files, manifest (all contract fields), trades.offsets.json (monotonic) and raw_hashes.json."""
    config = session.config
    create = session.create
    if create is None:
        raise ContractError("cannot write a manifest without a create event")
    out = config.out
    trades = session.trades
    migration = session.migration
    (out / provider_files["create"]).write_text(create.text, encoding="utf-8")
    (out / provider_files["trades"]).write_text("[\n" + ",\n".join(item.text for item in trades) + "\n]\n", encoding="utf-8")
    if migration is not None:
        (out / provider_files["migration"]).write_text(migration.text, encoding="utf-8")
    shape: dict[str, list[str]] = {}
    for name in provider_files.values():
        path = out / name
        if path.is_file():
            value = load_json(path)
            names = set(value) if isinstance(value, dict) else {key for item in value for key in item}
            shape[name] = sorted(names)
    capture_mint = create.obj.get(config.mint_field)
    first_trade = trades[0] if trades else None
    manifest: dict[str, Any] = {
        "provider": "pumpportal",
        "schema_version": sha256_json(shape) if len(shape) == len(provider_files) else None,
        "captured_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(create.utc_ns // NANOSECONDS_PER_SECOND)),
        "source": config.url.split("?", 1)[0],
        "capture_mint": capture_mint,
        "field_map": config.field_map(),
        "tx_type_values": config.tx_type_values(),
        "redactions": [],
        "offsets_time_basis": "monotonic_ns",
        "warmup_measurement": {
            "subscribe_sent_utc_ns": session.subscribe_sent_utc_ns,
            "subscribe_sent_offset_ns": session.subscribe_sent_mono_ns - create.mono_ns,
            "first_trade_received_utc_ns": first_trade.utc_ns if first_trade else 0,
            "first_trade_received_offset_ns": first_trade.mono_ns - create.mono_ns if first_trade else 0,
            "subscribe_ack_offset_ns": None,
            "ack_message_raw": None,
            "ack_candidates_raw": [{"offset_ns": item.mono_ns - create.mono_ns, "utc_ns": item.utc_ns, "raw": item.text} for item in session.ack_candidates],
        },
        "feed_cost": {
            "spend_cap_sol": str(config.spend_cap_sol),
            "lamports_per_paid_message": config.lamports_per_paid_message,
            "lamports_per_sol": config.lamports_per_sol,
            "paid_message_limit": config.max_paid_messages,
            "paid_messages_at_unsubscribe": session.paid_messages_at_unsubscribe,
            "paid_message_count": session.paid_message_count,
            "estimated_cost_lamports": session.paid_message_count * config.lamports_per_paid_message,
        },
        "capture_windows": {"create_wait_seconds": config.create_wait_seconds, "trade_seconds": config.trade_seconds, "migration_wait_seconds": config.migration_wait_seconds},
        "trade_subscription_stop_reason": session.trade_subscription_stop_reason,
        "capture_stop_reason": session.capture_stop_reason,
        "migration_status": session.migration_status(),
        "migration_mint_matches_capture": bool(migration is not None and migration.obj.get(config.mint_field) == capture_mint),
        "capture_validity": validity,
        "files": {name: sha256_file(out / name) for name in provider_files.values() if (out / name).is_file()},
    }
    path = out / manifest_name
    path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    offsets = {str(index): str(Decimal(item.mono_ns - create.mono_ns) / Decimal(NANOSECONDS_PER_MILLISECOND)) for index, item in enumerate(trades)}
    (out / "trades.offsets.json").write_text(json.dumps(offsets, indent=2) + "\n", encoding="utf-8")
    hashes = {"create": sha256_json(create.obj), "trades": [sha256_json(item.obj) for item in trades]}
    (out / "raw_hashes.json").write_text(json.dumps(hashes, indent=2) + "\n", encoding="utf-8")
    return path


async def capture(config: CaptureConfig, contract: dict[str, Any], provider_files: dict[str, str], manifest_name: str) -> int:
    import websockets  # imported here so the Phase-0 linter and tests do not require the dependency

    url = config.url
    api_key = os.environ.get("DARKBOT_PUMPPORTAL_API_KEY")
    if api_key:
        url = f"{url}{'&' if '?' in url else '?'}api-key={api_key}"
    config.out.mkdir(parents=True, exist_ok=True)
    session = CaptureSession(config, contract, time.monotonic_ns())
    with (config.out / "capture_audit.jsonl").open("a", encoding="utf-8") as audit:

        def log(direction: str, text: str, utc_ns: int, mono_ns: int) -> None:
            audit.write(json.dumps({"utc_ns": utc_ns, "mono_offset_ns": mono_ns - session.started_mono_ns, "direction": direction, "text": text}) + "\n")

        async def send_all(ws: Any, frames: list[str]) -> None:
            for frame in frames:
                await ws.send(frame)
                log("out", frame, time.time_ns(), time.monotonic_ns())

        try:
            async with websockets.connect(url, ping_interval=config.ping_interval_ms / 1000) as ws:
                await send_all(ws, session.initial_frames())
                while not session.done():
                    now = time.monotonic_ns()
                    try:
                        text = await asyncio.wait_for(ws.recv(), timeout=session.timeout_seconds(now))
                    except asyncio.TimeoutError:
                        await send_all(ws, session.on_tick(time.monotonic_ns()))
                        continue
                    mono_ns, utc_ns = time.monotonic_ns(), time.time_ns()
                    if isinstance(text, bytes):
                        text = text.decode("utf-8")
                    log("in", text, utc_ns, mono_ns)
                    await send_all(ws, session.on_message(text, utc_ns, mono_ns))
                if session.subscribed:
                    await send_all(ws, session.stop_trade_subscription(SUB_STOP_CAPTURE_ENDED))
        except websockets.ConnectionClosed:
            session.on_connection_closed()
    validity, exit_code = session.assess()
    if config.migration_only:
        if session.migration is not None:
            (config.out / provider_files["migration"]).write_text(session.migration.text, encoding="utf-8")
            exit_code = EXIT_OK
        else:
            exit_code = EXIT_NO_MIGRATION
    elif session.create is not None:
        write_manifest(session, provider_files, manifest_name, validity)
    print(json.dumps(session.summary(exit_code, validity)))
    return exit_code


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="darkbot.capture", description="record one real coin sequence as Phase-0 fixtures")
    parser.add_argument("--url", required=True)
    parser.add_argument("--out", type=Path, default=None, help="defaults to the contract's provider fixture dir")
    for name in ("tx-type-field", "create-value", "buy-value", "sell-value", "mint-field", "pool-field", "pool-value", "signature-field", "trader-field", "sol-field", "token-field", "vsol-field", "vtoken-field"):
        parser.add_argument(f"--{name}", required=True)
    parser.add_argument("--max-paid-messages", type=int, default=None, help="optional lower limit than the contract spend cap allows; never higher")
    parser.add_argument("--create-wait-seconds", type=int, default=None, help="defaults to phase0_fixture_contract.capture_windows")
    parser.add_argument("--trade-seconds", type=int, default=None, help="defaults to phase0_fixture_contract.capture_windows")
    parser.add_argument("--migration-wait-seconds", type=int, default=None, help="defaults to phase0_fixture_contract.capture_windows")
    parser.add_argument("--migration-only", action="store_true")
    return parser


def config_from_contract(args: argparse.Namespace, contract: dict[str, Any], root: Path) -> CaptureConfig:
    """Resolve every operational value from the contract; CLI arguments may only override windows or lower the limit."""
    spec = contract["phase0_fixture_contract"]
    cap = spec["capture_spend_cap_sol"]
    if isinstance(cap, str) and cap.startswith("UNSET"):
        raise ContractError("phase0_fixture_contract.capture_spend_cap_sol is UNSET: complete T0.3 (new contract version) before capturing")
    feed = contract["market_data_source_v1"]["feed_pricing_observed"]
    limit = max_paid_messages_from_cap(Decimal(cap), spec["lamports_per_sol"], feed["lamports_per_paid_message"])
    if args.max_paid_messages is not None:
        if args.max_paid_messages > limit:
            raise ContractError(f"--max-paid-messages {args.max_paid_messages} exceeds the spend cap ({limit} paid messages)")
        limit = args.max_paid_messages
    if limit < spec["minimum_trade_events"]:
        raise ContractError(f"paid-message limit {limit} is below minimum_trade_events {spec['minimum_trade_events']}; raise the spend cap")
    windows = spec["capture_windows"]
    limits = contract["operational_limits_v1"]["keys"]
    return CaptureConfig(
        url=args.url,
        out=args.out or root / spec["fixture_root"] / spec["provider_dir"],
        tx_type_field=args.tx_type_field, create_value=args.create_value, buy_value=args.buy_value, sell_value=args.sell_value,
        mint_field=args.mint_field, pool_field=args.pool_field, pool_value=args.pool_value,
        signature_field=args.signature_field, trader_field=args.trader_field, sol_field=args.sol_field, token_field=args.token_field,
        vsol_field=args.vsol_field, vtoken_field=args.vtoken_field,
        spend_cap_sol=Decimal(cap), lamports_per_paid_message=feed["lamports_per_paid_message"], lamports_per_sol=spec["lamports_per_sol"],
        max_paid_messages=limit, minimum_trade_events=spec["minimum_trade_events"],
        create_wait_seconds=args.create_wait_seconds if args.create_wait_seconds is not None else windows["create_wait_seconds"],
        trade_seconds=args.trade_seconds if args.trade_seconds is not None else windows["trade_seconds"],
        migration_wait_seconds=args.migration_wait_seconds if args.migration_wait_seconds is not None else windows["migration_wait_seconds"],
        ping_interval_ms=limits["websocket_ping_interval_ms"]["initial"],
        migration_only=args.migration_only,
    )


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    root = find_project_root()
    active, errors = load_active_contract(root)
    if active is None:
        raise ContractError("; ".join(errors))
    spec = active.document["phase0_fixture_contract"]
    try:
        config = config_from_contract(args, active.document, root)
    except ContractError as exc:
        print(str(exc), file=sys.stderr)
        return EXIT_USAGE
    return asyncio.run(capture(config, active.document, spec["provider_files"], spec["manifest_file"]))


if __name__ == "__main__":
    sys.exit(main())

"""Deterministic replay of recorded SHADOW sessions.

Replay is intentionally offline: it reads one recorded session from SQLite, rebuilds the
same ``NormalizedEvent`` stream, advances ``ReplayClock``/``ReplayScheduler`` and feeds
exactly the same ``MarketDataService`` used by live SHADOW.  It performs no RPC, HTTP or
WebSocket access and never changes the source database.

Stage 2's acceptance criterion is strict parity: for every LIVE feature snapshot in the
selected session, replay must produce byte-identical canonical ``features_json`` at the
same launch/deadline.  A mismatch is reported as an error; it is never rounded away.
"""

from __future__ import annotations

import argparse
import base64
import json
import sqlite3
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from darkbot.cli import canonical_json
from darkbot.clock import ReplayClock, ReplayScheduler
from darkbot.config import Limits, load_limits
from darkbot.domain import DataQualityReason, EventType, NormalizedEvent, SnapshotOrigin, load_contract
from darkbot.market import PROGRAM_DATA_PREFIX, MarketDataService, PumpEventDecoder
from darkbot.storage import StorageReader

EXIT_OK = 0
EXIT_PARITY = 4
EXIT_DATA = 5


class ReplayError(RuntimeError):
    """The recorded dataset is incomplete or internally inconsistent for deterministic replay."""


@dataclass(frozen=True, slots=True)
class ReplaySession:
    """Minimal immutable session metadata required to reconstruct recorded monotonic time."""

    id: str
    sequence: int
    started_at_utc_ns: int
    started_at_monotonic_ns: int
    ended_at_utc_ns: int | None
    data_complete: bool

    @property
    def end_offset_ns(self) -> int | None:
        """Recorded session duration on the same UTC clock used for persisted receive times."""
        if self.ended_at_utc_ns is None:
            return None
        return max(0, self.ended_at_utc_ns - self.started_at_utc_ns)


@dataclass(frozen=True, slots=True)
class ReplayParityReport:
    """Result of comparing replayed snapshots with the recorded LIVE snapshots."""

    session_id: str
    launches: int
    events: int
    live_snapshots: int
    replay_snapshots: int
    matched: int
    missing: int
    unexpected: int
    mismatched: int

    @property
    def passed(self) -> bool:
        return self.missing == 0 and self.unexpected == 0 and self.mismatched == 0 and self.live_snapshots == self.replay_snapshots

    def as_dict(self) -> dict[str, int | str | bool]:
        return {
            "session_id": self.session_id,
            "launches": self.launches,
            "events": self.events,
            "live_snapshots": self.live_snapshots,
            "replay_snapshots": self.replay_snapshots,
            "matched": self.matched,
            "missing": self.missing,
            "unexpected": self.unexpected,
            "mismatched": self.mismatched,
            "passed": self.passed,
        }


class ReplayEngine:
    """Rebuild a recorded session through the live tracker/feature path.

    Responsibilities:
    - read immutable session/launch/event facts from SQLite;
    - reconstruct CREATE metadata that is not duplicated in the ``events`` table from the
      stored provider observation;
    - advance replay time causally and feed ``MarketDataService``;
    - compare canonical replay snapshots against recorded LIVE snapshots.

    It does not evaluate strategy filters, simulate fills, mutate SQLite or contact any
    provider.  Those are later stages.
    """

    def __init__(self, db_path: Path, contract: dict[str, Any], limits: Limits) -> None:
        self._db_path = db_path
        self._contract = contract
        self._limits = limits
        self._reader = StorageReader(db_path, limits)
        self._reader.connection.row_factory = sqlite3.Row
        self._decoder = PumpEventDecoder(limits.create_event_tail_layout, limits.chain_program_id, limits.known_pump_event_names)

    def close(self) -> None:
        self._reader.close()

    def latest_shadow_session_id(self) -> str:
        row = self._reader.connection.execute(
            "SELECT id FROM sessions WHERE mode = 'SHADOW' ORDER BY persistent_session_sequence DESC LIMIT 1"
        ).fetchone()
        if row is None:
            raise ReplayError("database contains no SHADOW session")
        return str(row["id"])

    def run_parity(self, session_id: str) -> ReplayParityReport:
        session = self.load_session(session_id)
        live = self._load_live_snapshots(session_id)
        if not live:
            raise ReplayError(f"session {session_id} contains no LIVE feature snapshots")
        feature_set_ids = {key[2] for key in live}
        if len(feature_set_ids) != 1:
            raise ReplayError(f"session {session_id} has {len(feature_set_ids)} LIVE feature-set versions; stage-2 parity expects exactly one")
        feature_set_model_id = next(iter(feature_set_ids))

        clock = ReplayClock(session.started_at_monotonic_ns, session.started_at_utc_ns)
        scheduler = ReplayScheduler()
        replayed: dict[tuple[str, int, str], str] = {}

        def capture_snapshot(tracker: Any, deadline_offset_ns: int, features: dict[str, Any]) -> None:
            replayed[(tracker.launch_id, deadline_offset_ns, feature_set_model_id)] = canonical_json(features)

        service = MarketDataService(
            self._contract,
            self._limits,
            clock,
            scheduler,
            writer=None,
            ids=None,
            session_id=session.id,
            session_sequence=session.sequence,
            feature_set_model_id=feature_set_model_id,
            origin=SnapshotOrigin.REPLAY,
            on_snapshot=capture_snapshot,
        )

        launch_context = self.load_launch_context(session_id)
        event_count = 0
        timeline = self.timeline(session_id)
        for offset, kind, payload in timeline:
            # Fire timers strictly before the next recorded fact.  At an identical offset,
            # the recorded fact is applied first and the deadline second.
            if offset > 0:
                scheduler.fire_due(session.started_at_monotonic_ns + offset - 1)
            clock.advance_to(offset)
            if kind == "quality_start":
                service.apply_replay_quality_boundary(payload, True)
            elif kind == "quality_end":
                service.apply_replay_quality_boundary(payload, False)
            else:
                row = payload
                event = self.normalized_event(row, launch_context)
                if event.event_type == EventType.CREATE:
                    context = launch_context[str(row["launch_id"])]
                    service.on_normalized_event(
                        event,
                        observation_id=int(row["observation_id"]),
                        launch_id=str(row["launch_id"]),
                        initial_reserves=(context["initial_virtual_sol_reserves"], context["initial_virtual_token_reserves"], context["initial_real_token_reserves"]),
                        quote_mint=context["quote_mint"],
                    )
                else:
                    service.on_normalized_event(event, observation_id=int(row["observation_id"]), launch_id=str(row["launch_id"]))
                event_count += 1
            scheduler.fire_due(session.started_at_monotonic_ns + offset)

        max_deadline = max(key[1] for key in live)
        clock.advance_to(max(clock.current_offset_ns, max_deadline))
        scheduler.fire_due(session.started_at_monotonic_ns + max_deadline)

        live_keys, replay_keys = set(live), set(replayed)
        common = live_keys & replay_keys
        mismatched = sum(live[key] != replayed[key] for key in common)
        matched = len(common) - mismatched
        return ReplayParityReport(
            session_id=session.id,
            launches=len(launch_context),
            events=event_count,
            live_snapshots=len(live),
            replay_snapshots=len(replayed),
            matched=matched,
            missing=len(live_keys - replay_keys),
            unexpected=len(replay_keys - live_keys),
            mismatched=mismatched,
        )

    def load_session(self, session_id: str) -> ReplaySession:
        row = self._reader.connection.execute(
            "SELECT id, persistent_session_sequence, started_at_utc_ns, started_at_monotonic_ns, ended_at_utc_ns, data_complete FROM sessions WHERE id = ?",
            (session_id,),
        ).fetchone()
        if row is None:
            raise ReplayError(f"session not found: {session_id}")
        return ReplaySession(
            str(row["id"]),
            int(row["persistent_session_sequence"]),
            int(row["started_at_utc_ns"]),
            int(row["started_at_monotonic_ns"]),
            None if row["ended_at_utc_ns"] is None else int(row["ended_at_utc_ns"]),
            bool(row["data_complete"]),
        )

    def _load_live_snapshots(self, session_id: str) -> dict[tuple[str, int, str], str]:
        rows = self._reader.connection.execute(
            """
            SELECT launch_id, decision_deadline_offset_ns, feature_set_model_id, features_json
            FROM feature_snapshots
            WHERE session_id = ? AND computed_in = 'LIVE'
            ORDER BY launch_id, decision_deadline_offset_ns
            """,
            (session_id,),
        ).fetchall()
        return {(str(row["launch_id"]), int(row["decision_deadline_offset_ns"]), str(row["feature_set_model_id"])): str(row["features_json"]) for row in rows}

    def load_launch_context(self, session_id: str) -> dict[str, dict[str, Any]]:
        rows = self._reader.connection.execute(
            """
            SELECT id, mint, name, symbol, creator, curve_address_provider, quote_mint, quote_scope, accepted,
                   t0_receive_offset_ns, initial_virtual_sol_reserves, initial_virtual_token_reserves,
                   initial_real_token_reserves, initial_buy_token_base_units, create_observation_id,
                   migrated, data_complete
            FROM launches WHERE session_id = ?
            """,
            (session_id,),
        ).fetchall()
        context: dict[str, dict[str, Any]] = {}
        for row in rows:
            item = dict(row)
            item["token_total_supply"] = self._token_total_supply(int(row["create_observation_id"]), str(row["mint"]))
            context[str(row["id"])] = item
        return context

    def _token_total_supply(self, observation_id: int, mint: str) -> int | None:
        row = self._reader.connection.execute(
            "SELECT redacted_payload_json FROM provider_observations WHERE id = ?", (observation_id,)
        ).fetchone()
        if row is None:
            raise ReplayError(f"create observation missing: {observation_id}")
        payload = json.loads(str(row["redacted_payload_json"]))
        frames = payload.get("program_data")
        if not isinstance(frames, list):
            raise ReplayError(f"create observation {observation_id} has no program_data list")
        decoded, _ = self._decoder.decode_logs([PROGRAM_DATA_PREFIX + str(frame) for frame in frames])
        create = next((item for item in decoded if item.kind == "create" and item.fields.get("mint") == mint), None)
        if create is None:
            # Older Stage-1 databases can still be replayed if the stored frame is present
            # but was categorized differently.  Inspect raw frames directly by discriminator.
            for frame in frames:
                try:
                    raw = base64.b64decode(str(frame), validate=True)
                except Exception as exc:  # noqa: BLE001 - converted to explicit dataset error below
                    raise ReplayError(f"invalid base64 in create observation {observation_id}: {exc}") from exc
                if raw[:8] == self._decoder.create_discriminator:
                    fields = self._decoder.decode_create(raw[8:], self._limits.create_event_tail_layout)
                    if fields.get("mint") == mint:
                        return int(fields["token_total_supply"])
            raise ReplayError(f"CreateEvent for mint {mint} not recoverable from observation {observation_id}")
        value = create.fields.get("token_total_supply")
        return None if value is None else int(value)

    def quality_intervals(self, session_id: str) -> tuple[tuple[int, int | None, DataQualityReason], ...]:
        """Recorded provider-gap intervals used to reject execution windows with missing data."""
        rows = self._reader.connection.execute(
            "SELECT start_offset_ns, end_offset_ns, reason FROM data_quality_intervals WHERE session_id = ? ORDER BY id",
            (session_id,),
        ).fetchall()
        return tuple(
            (int(row["start_offset_ns"]), None if row["end_offset_ns"] is None else int(row["end_offset_ns"]), DataQualityReason(str(row["reason"])))
            for row in rows
        )

    def timeline(self, session_id: str) -> list[tuple[int, str, Any]]:
        """Merge market events and recorded gap boundaries into one deterministic offset timeline."""
        items: list[tuple[int, int, str, Any]] = []
        for row in self.iter_events(session_id):
            items.append((int(row["receive_offset_ns_from_session_start"]), 1, "event", row))
        intervals = self._reader.connection.execute(
            "SELECT start_offset_ns, end_offset_ns, reason FROM data_quality_intervals WHERE session_id = ? ORDER BY id",
            (session_id,),
        ).fetchall()
        for row in intervals:
            reason = DataQualityReason(str(row["reason"]))
            items.append((int(row["start_offset_ns"]), 0, "quality_start", reason))
            if row["end_offset_ns"] is not None:
                items.append((int(row["end_offset_ns"]), 0, "quality_end", reason))
        items.sort(key=lambda item: (item[0], item[1]))
        return [(offset, kind, payload) for offset, _priority, kind, payload in items]

    def iter_events(self, session_id: str):
        return self._reader.connection.execute(
            """
            SELECT * FROM events
            WHERE session_id = ? AND event_type IN ('CREATE', 'BUY', 'SELL')
            ORDER BY receive_offset_ns_from_session_start,
                     provider_receive_sequence,
                     COALESCE(provider_event_index, -1), id
            """,
            (session_id,),
        )

    @staticmethod
    def normalized_event(row: sqlite3.Row | dict[str, Any], launches: dict[str, dict[str, Any]]) -> NormalizedEvent:
        launch_id = str(row["launch_id"])
        context = launches.get(launch_id)
        if context is None:
            raise ReplayError(f"event {row['id']} references unknown launch {launch_id}")
        event_type = EventType(str(row["event_type"]))
        virtual_token_reserves = None if row["virtual_token_reserves"] is None else int(row["virtual_token_reserves"])
        initial_virtual = context.get("initial_virtual_token_reserves")
        initial_real = context.get("initial_real_token_reserves")
        real_token_reserves = None
        if virtual_token_reserves is not None and initial_virtual is not None and initial_real is not None:
            real_token_reserves = int(initial_real) - (int(initial_virtual) - virtual_token_reserves)
            if real_token_reserves < 0:
                raise ReplayError(f"event {row['id']} reconstructs negative real token reserves")
        return NormalizedEvent(
            event_type=event_type,
            mint=str(row["mint"]),
            signature=str(row["transaction_signature"] or ""),
            event_index=int(row["provider_event_index"] or 0),
            actor=None if row["actor"] is None else str(row["actor"]),
            quote_lamports=None if row["quote_lamports"] is None else int(row["quote_lamports"]),
            token_base_units=None if row["token_base_units"] is None else int(row["token_base_units"]),
            virtual_sol_reserves=None if row["virtual_sol_reserves"] is None else int(row["virtual_sol_reserves"]),
            virtual_token_reserves=virtual_token_reserves,
            real_token_reserves=real_token_reserves,
            receive_offset_ns=int(row["receive_offset_ns_from_session_start"]),
            utc_received_ns=int(row["utc_received_ns"]),
            chain_slot=None if row["chain_slot"] is None else int(row["chain_slot"]),
            receive_sequence=int(row["provider_receive_sequence"]),
            name=str(context["name"]) if event_type == EventType.CREATE and context["name"] is not None else None,
            symbol=str(context["symbol"]) if event_type == EventType.CREATE and context["symbol"] is not None else None,
            creator=str(context["creator"]) if event_type == EventType.CREATE and context["creator"] is not None else None,
            bonding_curve=str(context["curve_address_provider"]) if event_type == EventType.CREATE and context["curve_address_provider"] is not None else None,
            token_total_supply=context["token_total_supply"] if event_type == EventType.CREATE else None,
        )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="darkbot.replay", description="Replay one recorded SHADOW session and prove LIVE == REPLAY feature parity.")
    parser.add_argument("--db", type=Path, required=True)
    parser.add_argument("--session-id", default=None, help="SHADOW session id; default = latest SHADOW session")
    parser.add_argument("--project-root", type=Path, default=None)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    active = load_contract(args.project_root)
    limits = load_limits(active.document, chain_ws_url="wss://replay.invalid", mode="BACKTEST")
    engine = ReplayEngine(args.db, active.document, limits)
    try:
        session_id = args.session_id or engine.latest_shadow_session_id()
        report = engine.run_parity(session_id)
    except ReplayError as exc:
        print(json.dumps({"event": "REPLAY_ERROR", "error": str(exc)}), file=sys.stderr)
        return EXIT_DATA
    finally:
        engine.close()
    print(json.dumps({"event": "REPLAY_PARITY", **report.as_dict()}, sort_keys=True))
    return EXIT_OK if report.passed else EXIT_PARITY


if __name__ == "__main__":
    raise SystemExit(main())

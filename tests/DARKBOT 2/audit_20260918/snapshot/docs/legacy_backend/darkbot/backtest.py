"""Offline Stage-3 backtest over recorded SHADOW data.

The backtest reuses the recorded-event reader, ``MarketDataService``/``FeatureCalculator``,
``StrategyEngine`` and ``PaperFillModel``. It never contacts RPC/HTTP/WebSocket providers and
never mutates the source database. The first goal is a deterministic current-data research
loop, not a profitability claim.
"""

from __future__ import annotations

import argparse
import bisect
import json
import statistics
import sys
from dataclasses import asdict, dataclass, replace
from decimal import Decimal
from pathlib import Path
from typing import Any

from darkbot.cli import canonical_json
from darkbot.clock import ReplayClock, ReplayScheduler
from darkbot.config import Limits, StrategyConfig, load_limits, load_protocol_fee_config, load_strategy
from darkbot.domain import NANOSECONDS_PER_MILLISECOND, DecisionAction, EventType, QuoteScope, SnapshotOrigin, load_contract
from darkbot.execution import (
    AttemptStatus,
    ExitAttempt,
    ExitSignal,
    LatencyAssumptions,
    NetworkCostAssumptions,
    ObservedCurveState,
    PaperFillModel,
    ProtocolFeeConfig,
)
from darkbot.market import DEFAULT_PUBKEY, MarketDataService
from darkbot.replay import ReplayEngine, ReplayError, ReplaySession
from darkbot.strategy import StrategyEngine

EXIT_OK = 0
EXIT_DATA = 5


class BacktestError(RuntimeError):
    """The source session or explicit backtest assumptions are insufficient for evaluation."""


@dataclass(frozen=True, slots=True)
class BacktestPolicy:
    """Small Stage-3 execution policy; no portfolio optimization or live control lives here."""

    max_open_positions: int
    account_rent_lamports: int
    exit_max_sell_attempts: int
    exit_retry_backoff_ms: int

    def __post_init__(self) -> None:
        if self.max_open_positions < 1:
            raise BacktestError("max_open_positions must be >= 1")
        if self.account_rent_lamports < 0:
            raise BacktestError("account_rent_lamports must be >= 0")
        if self.exit_max_sell_attempts < 1:
            raise BacktestError("exit_max_sell_attempts must be >= 1")
        if self.exit_retry_backoff_ms < 0:
            raise BacktestError("exit_retry_backoff_ms must be >= 0")


@dataclass(frozen=True, slots=True)
class CurvePoint:
    """One observed post-event bonding-curve state, ordered by receive time."""

    state: ObservedCurveState
    event_type: EventType


@dataclass(frozen=True, slots=True)
class Candidate:
    """One strategy PASS with complete execution data through its maximum modeled lifetime."""

    launch_id: str
    mint: str
    decision_offset_ns: int
    features: dict[str, Any]
    points: tuple[CurvePoint, ...]


@dataclass(frozen=True, slots=True)
class TradeResult:
    """One candidate execution outcome after entry and optional position exit."""

    launch_id: str
    mint: str
    entry_status: AttemptStatus
    exit_reason: str | None
    exit_status: AttemptStatus | None
    decision_offset_ns: int
    entry_fill_offset_ns: int
    close_offset_ns: int
    tokens: int
    curve_pnl_before_fees_lamports: int
    pump_fees_lamports: int
    network_fees_lamports: int
    account_rent_lamports: int
    net_pnl_lamports: int

    @property
    def closed_position(self) -> bool:
        return self.entry_status == AttemptStatus.FILLED and self.exit_status == AttemptStatus.FILLED


@dataclass(frozen=True, slots=True)
class BacktestReport:
    """Compact Stage-3 metrics; all money values are lamports and therefore auditable."""

    session_id: str
    strategy_hash: str
    assumptions: dict[str, int | str]
    launches_seen: int
    launches_tradable: int
    rejected_filter: int
    data_incomplete_at_decision: int
    unsupported_quote: int
    excluded_incomplete_horizon: int
    excluded_execution_data: int
    candidates_passed: int
    skipped_slot_busy: int
    entries_attempted: int
    entries_filled: int
    entries_slippage_rejected: int
    entries_curve_unavailable: int
    positions_closed: int
    target_exits: int
    stop_exits: int
    timeout_exits: int
    unresolved_zero: int
    curve_pnl_before_fees_lamports: int
    pump_fees_lamports: int
    network_fees_lamports: int
    account_rent_lamports: int
    net_pnl_lamports: int
    net_expectancy_lamports: str | None
    win_rate: str | None
    profit_factor: str | None
    median_trade_lamports: str | None
    max_realized_drawdown_lamports: int

    def as_dict(self) -> dict[str, Any]:
        return asdict(self)


class BacktestEngine:
    """Coordinate recorded replay, strategy decisions, slot gating and the shared fill model.

    It does not implement curve arithmetic, feature formulas or provider decoding. Those stay
    in their owning modules. This class only orders already-recorded facts chronologically.
    """

    def __init__(
        self,
        db_path: Path,
        contract: dict[str, Any],
        limits: Limits,
        strategy: StrategyConfig,
        protocol_fees: ProtocolFeeConfig,
        policy: BacktestPolicy,
    ) -> None:
        self._contract = contract
        self._limits = limits
        self._strategy = strategy
        self._policy = policy
        self._reader = ReplayEngine(db_path, contract, limits)
        network = NetworkCostAssumptions(strategy.base_fee_lamports, strategy.priority_fee_lamports)
        # LEAN_V1 currently exposes one aggregate latency value. Until a separate measured
        # exit model exists, apply the same explicit assumption symmetrically to entry/exit.
        latency = LatencyAssumptions(0, 0, strategy.entry_latency_ms, strategy.entry_latency_ms, "ASSUMED")
        self._protocol_fees = protocol_fees
        self._fill = PaperFillModel(protocol_fees, network, latency)
        self._strategy_engine = StrategyEngine(strategy)

    def close(self) -> None:
        self._reader.close()

    def latest_shadow_session_id(self) -> str:
        return self._reader.latest_shadow_session_id()

    def run(self, session_id: str) -> BacktestReport:
        session = self._reader.load_session(session_id)
        if not session.data_complete or session.end_offset_ns is None:
            raise BacktestError("SHADOW session must be stopped cleanly with data_complete=1")
        launches = self._reader.load_launch_context(session_id)
        snapshots = self._replay_decision_snapshots(session, launches)
        points = self._curve_points(session_id, launches)
        intervals = self._reader.quality_intervals(session_id)

        rejected_filter = 0
        data_incomplete = 0
        unsupported_quote = 0
        incomplete_horizon = 0
        execution_data = 0
        passed = 0
        candidates: list[Candidate] = []

        for launch_id, context in sorted(launches.items(), key=lambda item: int(item[1]["t0_receive_offset_ns"])):
            if not bool(context["accepted"]) or str(context["quote_scope"]) != QuoteScope.SOL_WSOL.value:
                unsupported_quote += 1
                continue

            snapshot = snapshots.get(launch_id)
            if snapshot is None:
                data_incomplete += 1
                continue
            decision = self._strategy_engine.evaluate(snapshot)
            if decision.action == DecisionAction.DATA_INCOMPLETE:
                data_incomplete += 1
                continue
            if decision.action != DecisionAction.PASS:
                rejected_filter += 1
                continue

            passed += 1
            launch_points = points.get(launch_id, ())
            if not launch_points:
                execution_data += 1
                continue
            decision_offset = int(context["t0_receive_offset_ns"]) + self._strategy.observation_ms * NANOSECONDS_PER_MILLISECOND
            required_end = self._required_end_offset(decision_offset)
            if required_end > session.end_offset_ns:
                incomplete_horizon += 1
                continue
            if self._overlaps_gap(decision_offset, required_end, intervals):
                execution_data += 1
                continue
            if bool(context.get("migrated")):
                # Stage 3 has no PumpSwap exit yet; a migrated curve cannot yield a trustworthy
                # bonding-curve outcome without migration timing, so exclude rather than guess.
                execution_data += 1
                continue
            candidates.append(Candidate(launch_id, str(context["mint"]), decision_offset, snapshot, launch_points))

        results, skipped = self._simulate_candidates(candidates)
        return self._summarize(
            session_id,
            launches,
            rejected_filter,
            data_incomplete,
            unsupported_quote,
            incomplete_horizon,
            execution_data,
            passed,
            skipped,
            results,
        )

    def _replay_decision_snapshots(self, session: ReplaySession, launches: dict[str, dict[str, Any]]) -> dict[str, dict[str, Any]]:
        observation = self._strategy.observation_ms
        replay_limits = replace(self._limits, runtime_snapshot_ms=(observation,), observation_ms=observation)
        clock = ReplayClock(session.started_at_monotonic_ns, session.started_at_utc_ns)
        scheduler = ReplayScheduler()
        snapshots: dict[str, dict[str, Any]] = {}

        def capture(tracker: Any, _deadline: int, features: dict[str, Any]) -> None:
            snapshots[tracker.launch_id] = dict(features)

        service = MarketDataService(
            self._contract,
            replay_limits,
            clock,
            scheduler,
            writer=None,
            ids=None,
            session_id=session.id,
            session_sequence=session.sequence,
            feature_set_model_id="BACKTEST_REPLAY",
            origin=SnapshotOrigin.REPLAY,
            research_quote_lamports=self._strategy.trade_amount_lamports,
            on_snapshot=capture,
        )
        for offset, kind, payload in self._reader.timeline(session.id):
            if offset > 0:
                scheduler.fire_due(session.started_at_monotonic_ns + offset - 1)
            clock.advance_to(offset)
            if kind == "quality_start":
                service.apply_replay_quality_boundary(payload, True)
            elif kind == "quality_end":
                service.apply_replay_quality_boundary(payload, False)
            else:
                row = payload
                event = self._reader.normalized_event(row, launches)
                if event.event_type == EventType.CREATE:
                    context = launches[str(row["launch_id"])]
                    service.on_normalized_event(
                        event,
                        observation_id=int(row["observation_id"]),
                        launch_id=str(row["launch_id"]),
                        initial_reserves=(
                            context["initial_virtual_sol_reserves"],
                            context["initial_virtual_token_reserves"],
                            context["initial_real_token_reserves"],
                        ),
                        quote_mint=context["quote_mint"],
                    )
                else:
                    service.on_normalized_event(event, observation_id=int(row["observation_id"]), launch_id=str(row["launch_id"]))
            scheduler.fire_due(session.started_at_monotonic_ns + offset)
        end_offset = session.end_offset_ns
        assert end_offset is not None
        clock.advance_to(max(clock.current_offset_ns, end_offset))
        scheduler.fire_due(session.started_at_monotonic_ns + end_offset)
        return snapshots

    def _curve_points(self, session_id: str, launches: dict[str, dict[str, Any]]) -> dict[str, tuple[CurvePoint, ...]]:
        points: dict[str, list[CurvePoint]] = {}
        for row in self._reader.iter_events(session_id):
            launch_id = str(row["launch_id"])
            context = launches[launch_id]
            event = self._reader.normalized_event(row, launches)
            if None in (event.virtual_sol_reserves, event.virtual_token_reserves, event.real_token_reserves, context.get("token_total_supply")):
                continue
            creator = context.get("creator")
            creator_enabled = creator not in (None, "", DEFAULT_PUBKEY)
            state = ObservedCurveState(
                virtual_sol_reserves=int(event.virtual_sol_reserves),
                virtual_token_reserves=int(event.virtual_token_reserves),
                real_token_reserves=int(event.real_token_reserves),
                fee_market_cap_supply=int(context["token_total_supply"]),
                creator_fee_enabled=creator_enabled,
                receive_offset_ns=event.receive_offset_ns,
                signature=event.signature,
                slot=event.chain_slot,
                receive_sequence=event.receive_sequence,
            )
            points.setdefault(launch_id, []).append(CurvePoint(state, event.event_type))
        return {launch_id: tuple(items) for launch_id, items in points.items()}

    def _required_end_offset(self, decision_offset_ns: int) -> int:
        entry_fill = self._fill.entry_fill_offset_ns(decision_offset_ns)
        timeout = entry_fill + self._strategy.max_hold_ms * NANOSECONDS_PER_MILLISECOND
        retry_spacing = self._policy.exit_retry_backoff_ms * NANOSECONDS_PER_MILLISECOND
        exit_latency = self._fill.latency.exit_total_ms * NANOSECONDS_PER_MILLISECOND
        attempts = self._policy.exit_max_sell_attempts
        return timeout + attempts * exit_latency + max(0, attempts - 1) * retry_spacing

    @staticmethod
    def _overlaps_gap(start: int, end: int, intervals: tuple[tuple[int, int | None, Any], ...]) -> bool:
        for gap_start, gap_end, _reason in intervals:
            effective_end = end if gap_end is None else gap_end
            if gap_start <= end and effective_end >= start:
                return True
        return False

    def _simulate_candidates(self, candidates: list[Candidate]) -> tuple[list[TradeResult], int]:
        active_until: list[int] = []
        results: list[TradeResult] = []
        skipped = 0
        for candidate in sorted(candidates, key=lambda item: (item.decision_offset_ns, item.launch_id)):
            active_until = [offset for offset in active_until if offset > candidate.decision_offset_ns]
            if len(active_until) >= self._policy.max_open_positions:
                skipped += 1
                continue
            result = self._simulate_candidate(candidate)
            results.append(result)
            active_until.append(result.close_offset_ns)
        return results, skipped

    def _simulate_candidate(self, candidate: Candidate) -> TradeResult:
        decision_state = self._state_at(candidate.points, candidate.decision_offset_ns)
        if decision_state is None:
            raise BacktestError(f"candidate {candidate.launch_id} has no state at decision")
        entry_fill_offset = self._fill.entry_fill_offset_ns(candidate.decision_offset_ns)
        fill_state = self._state_at(candidate.points, entry_fill_offset)
        if fill_state is None:
            raise BacktestError(f"candidate {candidate.launch_id} has no state at entry fill")
        entry = self._fill.simulate_entry(
            decision_state,
            fill_state,
            candidate.decision_offset_ns,
            self._strategy.trade_amount_lamports,
            self._strategy.max_slippage_bps,
            account_rent_lamports=self._policy.account_rent_lamports,
        )
        if not entry.filled:
            return TradeResult(
                candidate.launch_id,
                candidate.mint,
                entry.status,
                None,
                None,
                candidate.decision_offset_ns,
                entry.fill_offset_ns,
                entry.fill_offset_ns,
                0,
                0,
                0,
                entry.network_fee_lamports,
                0,
                -entry.network_fee_lamports,
            )

        tokens = entry.fill_quote.tokens_out
        timeout_trigger = entry.fill_offset_ns + self._strategy.max_hold_ms * NANOSECONDS_PER_MILLISECOND
        trigger_reason = "TIMEOUT"
        trigger_offset = timeout_trigger
        for point in candidate.points:
            offset = point.state.receive_offset_ns
            if offset <= entry.fill_offset_ns:
                continue
            if offset > timeout_trigger:
                break
            signal = self._fill.exit_signal(
                point.state,
                tokens,
                entry.entry_cost_lamports,
                self._strategy.target_net_multiple,
                self._strategy.stop_net_multiple,
            )
            if signal != ExitSignal.HOLD:
                trigger_reason = signal.value
                trigger_offset = offset
                break

        exit_attempt, failed_network, final_fill = self._attempt_exit(candidate.points, trigger_offset, tokens)
        if exit_attempt is None and trigger_reason != "TIMEOUT" and final_fill < timeout_trigger:
            exit_attempt, extra_failed, final_fill = self._attempt_exit(candidate.points, timeout_trigger, tokens)
            failed_network += extra_failed
            trigger_reason = "TIMEOUT"

        buy_fees = entry.fill_quote.fees.total_lamports
        if exit_attempt is None:
            network = entry.network_fee_lamports + failed_network
            return TradeResult(
                candidate.launch_id,
                candidate.mint,
                entry.status,
                "UNRESOLVED_ZERO",
                None,
                candidate.decision_offset_ns,
                entry.fill_offset_ns,
                final_fill,
                tokens,
                0,
                buy_fees,
                network,
                entry.account_rent_lamports,
                -entry.entry_cost_lamports - failed_network,
            )

        sell_fees = exit_attempt.fill_quote.fees.total_lamports
        network = entry.network_fee_lamports + failed_network + exit_attempt.network_fee_lamports
        curve_pnl = exit_attempt.fill_quote.gross_sol_lamports - entry.fill_quote.net_sol_lamports
        net_pnl = exit_attempt.net_return_lamports - failed_network - entry.entry_cost_lamports
        return TradeResult(
            candidate.launch_id,
            candidate.mint,
            entry.status,
            trigger_reason,
            exit_attempt.status,
            candidate.decision_offset_ns,
            entry.fill_offset_ns,
            exit_attempt.fill_offset_ns,
            tokens,
            curve_pnl,
            buy_fees + sell_fees,
            network,
            entry.account_rent_lamports,
            net_pnl,
        )

    def _attempt_exit(self, points: tuple[CurvePoint, ...], first_trigger_offset: int, tokens: int) -> tuple[ExitAttempt | None, int, int]:
        trigger_offset = first_trigger_offset
        failed_network = 0
        final_fill = first_trigger_offset
        backoff_ns = self._policy.exit_retry_backoff_ms * NANOSECONDS_PER_MILLISECOND
        for _attempt_index in range(self._policy.exit_max_sell_attempts):
            trigger_state = self._state_at(points, trigger_offset)
            if trigger_state is None:
                return None, failed_network, final_fill
            fill_offset = self._fill.exit_fill_offset_ns(trigger_offset)
            fill_state = self._state_at(points, fill_offset)
            if fill_state is None:
                return None, failed_network, final_fill
            attempt = self._fill.simulate_exit(trigger_state, fill_state, trigger_offset, tokens, self._strategy.max_slippage_bps)
            final_fill = attempt.fill_offset_ns
            if attempt.filled:
                return attempt, failed_network, final_fill
            failed_network += attempt.network_fee_lamports
            trigger_offset = attempt.fill_offset_ns + backoff_ns
        return None, failed_network, final_fill

    @staticmethod
    def _state_at(points: tuple[CurvePoint, ...], offset_ns: int) -> ObservedCurveState | None:
        offsets = [point.state.receive_offset_ns for point in points]
        index = bisect.bisect_right(offsets, offset_ns) - 1
        return None if index < 0 else points[index].state

    def _summarize(
        self,
        session_id: str,
        launches: dict[str, dict[str, Any]],
        rejected_filter: int,
        data_incomplete: int,
        unsupported_quote: int,
        incomplete_horizon: int,
        execution_data: int,
        passed: int,
        skipped: int,
        results: list[TradeResult],
    ) -> BacktestReport:
        failed_entries = [item for item in results if item.entry_status != AttemptStatus.FILLED]
        filled_entries = [item for item in results if item.entry_status == AttemptStatus.FILLED]
        closed = [item for item in filled_entries if item.closed_position]
        unresolved = [item for item in filled_entries if not item.closed_position]
        pnl_events = sorted(((item.close_offset_ns, item.net_pnl_lamports) for item in results), key=lambda item: item[0])
        cumulative = 0
        peak = 0
        max_drawdown = 0
        for _offset, pnl in pnl_events:
            cumulative += pnl
            peak = max(peak, cumulative)
            max_drawdown = max(max_drawdown, peak - cumulative)

        closed_pnls = [item.net_pnl_lamports for item in closed]
        all_pnls = [item.net_pnl_lamports for item in results]
        gains = sum(value for value in all_pnls if value > 0)
        losses = -sum(value for value in all_pnls if value < 0)
        total_net = sum(all_pnls)
        expectancy = None if not closed else str(Decimal(total_net) / Decimal(len(closed)))
        win_rate = None if not closed else str(Decimal(sum(value > 0 for value in closed_pnls)) / Decimal(len(closed)))
        profit_factor = None if losses == 0 else str(Decimal(gains) / Decimal(losses))
        median_trade = None if not closed_pnls else str(Decimal(str(statistics.median(closed_pnls))))

        return BacktestReport(
            session_id=session_id,
            strategy_hash=self._strategy.version_hash(),
            assumptions={
                "protocol_fee_account": self._protocol_fees.account,
                "entry_latency_ms": self._fill.latency.entry_total_ms,
                "exit_latency_ms": self._fill.latency.exit_total_ms,
                "max_open_positions": self._policy.max_open_positions,
                "account_rent_lamports": self._policy.account_rent_lamports,
                "exit_max_sell_attempts": self._policy.exit_max_sell_attempts,
                "exit_retry_backoff_ms": self._policy.exit_retry_backoff_ms,
            },
            launches_seen=len(launches),
            launches_tradable=sum(bool(item["accepted"]) and str(item["quote_scope"]) == QuoteScope.SOL_WSOL.value for item in launches.values()),
            rejected_filter=rejected_filter,
            data_incomplete_at_decision=data_incomplete,
            unsupported_quote=unsupported_quote,
            excluded_incomplete_horizon=incomplete_horizon,
            excluded_execution_data=execution_data,
            candidates_passed=passed,
            skipped_slot_busy=skipped,
            entries_attempted=len(results),
            entries_filled=len(filled_entries),
            entries_slippage_rejected=sum(item.entry_status == AttemptStatus.SLIPPAGE_REJECTED for item in failed_entries),
            entries_curve_unavailable=sum(item.entry_status == AttemptStatus.CURVE_UNAVAILABLE for item in failed_entries),
            positions_closed=len(closed),
            target_exits=sum(item.exit_reason == ExitSignal.TARGET.value and item.closed_position for item in closed),
            stop_exits=sum(item.exit_reason == ExitSignal.STOP.value and item.closed_position for item in closed),
            timeout_exits=sum(item.exit_reason == "TIMEOUT" and item.closed_position for item in closed),
            unresolved_zero=len(unresolved),
            curve_pnl_before_fees_lamports=sum(item.curve_pnl_before_fees_lamports for item in results),
            pump_fees_lamports=sum(item.pump_fees_lamports for item in results),
            network_fees_lamports=sum(item.network_fees_lamports for item in results),
            account_rent_lamports=sum(item.account_rent_lamports for item in results),
            net_pnl_lamports=total_net,
            net_expectancy_lamports=expectancy,
            win_rate=win_rate,
            profit_factor=profit_factor,
            median_trade_lamports=median_trade,
            max_realized_drawdown_lamports=max_drawdown,
        )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="darkbot.backtest", description="Stage-3 deterministic backtest over one recorded SHADOW session.")
    parser.add_argument("--db", type=Path, required=True)
    parser.add_argument("--strategy", type=Path, required=True)
    parser.add_argument("--protocol-fee-config", type=Path, required=True)
    parser.add_argument("--session-id", default=None, help="SHADOW session id; default = latest SHADOW session")
    parser.add_argument("--max-open-positions", type=int, default=1)
    parser.add_argument("--account-rent-lamports", type=int, default=0)
    parser.add_argument("--exit-max-sell-attempts", type=int, default=1)
    parser.add_argument("--exit-retry-backoff-ms", type=int, default=0)
    parser.add_argument("--project-root", type=Path, default=None)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    active = load_contract(args.project_root)
    limits = load_limits(active.document, chain_ws_url="wss://backtest.invalid", mode="BACKTEST")
    try:
        strategy = load_strategy(args.strategy, active.document)
        protocol_fees = load_protocol_fee_config(args.protocol_fee_config)
        policy = BacktestPolicy(
            args.max_open_positions,
            args.account_rent_lamports,
            args.exit_max_sell_attempts,
            args.exit_retry_backoff_ms,
        )
        engine = BacktestEngine(args.db, active.document, limits, strategy, protocol_fees, policy)
        try:
            session_id = args.session_id or engine.latest_shadow_session_id()
            report = engine.run(session_id)
        finally:
            engine.close()
    except (BacktestError, ReplayError, ValueError) as exc:
        print(canonical_json({"event": "BACKTEST_ERROR", "error": str(exc)}), file=sys.stderr)
        return EXIT_DATA
    print(canonical_json({"event": "BACKTEST_REPORT", **report.as_dict()}))
    return EXIT_OK


if __name__ == "__main__":
    raise SystemExit(main())

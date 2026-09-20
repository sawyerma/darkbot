"""Strategy-free market-path analysis for DarkBot capture/PAPER recordings.

The analyzer reconstructs canonical Pump events from stored raw observations, then evaluates
counterfactual entry/exit paths offline. It never mutates strategy code and never broadcasts.

Outputs are explicitly in-sample diagnostics. Any selected rule must be validated on a fresh,
untouched capture period.
"""

from __future__ import annotations

import csv
import json
import shutil
import sqlite3
import statistics
import tempfile
import zipfile
from dataclasses import dataclass
from decimal import Decimal
from pathlib import Path
from typing import Any, Iterable

from darkbot.config import ConfigError, RuntimeConfig, build_runtime_config
from darkbot.domain import CostBasis, CoverageState, CurveState, InvalidQuote, QuoteScopeStatus, RawEnvelope, Side, VenueState
from darkbot.execution import ExecutionPolicy, QuoteEngine
from darkbot.market import MarketReducer, Normalizer, PumpDecoder
from darkbot.replay import load_replay_input
from darkbot.storage import open_connection


CHECKPOINTS_MS = (100, 250, 500, 1000, 2000, 3000, 5000, 7500, 10000, 15000, 20000, 30000, 45000, 60000)
HARD_EXITS_MS = (3000, 5000, 7500, 10000, 15000, 20000, 30000, 45000, 60000)
MIN_HOLDS_MS = (3000, 5000, 7500, 10000, 15000)
TARGETS = tuple(Decimal("1.05") + Decimal("0.05") * i for i in range(40))  # 1.05 .. 3.00


@dataclass
class PathPoint:
    mint: str
    event_type: str
    t_ms: float
    receive_mono_ns: int
    actor: str | None
    creator: str | None
    sol_amount: int | None
    token_amount: int | None
    virtual_sol_reserves: int
    virtual_token_reserves: int
    real_sol_reserves: int | None
    real_token_reserves: int | None
    creator_fee_bps: int
    token_program: str | None
    account_profile_id: str | None
    venue_state: str
    quote_scope_status: str
    net_multiple: Decimal | None = None
    net_pnl_lamports: int | None = None
    exit_value_lamports: int | None = None
    exit_invalid_reason: str | None = None


def _materialize_source(source: Path) -> tuple[Path, tempfile.TemporaryDirectory[str] | None]:
    if source.suffix.lower() != ".zip":
        if not source.exists():
            raise ConfigError(f"edge-analysis source not found: {source}")
        return source, None
    if not source.exists():
        raise ConfigError(f"edge-analysis source ZIP not found: {source}")
    tmp = tempfile.TemporaryDirectory(prefix="darkbot_edge_")
    root = Path(tmp.name)
    with zipfile.ZipFile(source) as zf:
        zf.extractall(root)
    candidates = sorted(
        p for p in root.rglob("*")
        if p.is_file() and p.suffix.lower() in (".sqlite", ".sqlite3", ".db")
    )
    if not candidates:
        tmp.cleanup()
        raise ConfigError(f"edge-analysis ZIP contains no SQLite database: {source}")
    # Prefer the largest DB: capture databases dwarf metadata/test DBs.
    db = max(candidates, key=lambda p: p.stat().st_size)
    return db, tmp


def _latest_run(conn: sqlite3.Connection, campaign: str | None = None) -> str:
    if campaign:
        row = conn.execute(
            "SELECT run_id FROM runs WHERE campaign_id=? ORDER BY started_utc DESC LIMIT 1", (campaign,)
        ).fetchone()
    else:
        row = conn.execute(
            "SELECT run_id FROM runs WHERE mode IN ('CAPTURE','PAPER') ORDER BY started_utc DESC LIMIT 1"
        ).fetchone()
    if row is None:
        raise ConfigError("edge-analysis: no CAPTURE/PAPER run found")
    return row[0]


def _runtime_config(conn: sqlite3.Connection, run_id: str, source: Path) -> RuntimeConfig:
    row = conn.execute(
        """SELECT c.canonical_json
           FROM runs r JOIN config_snapshots c ON c.config_hash=r.config_hash
           WHERE r.run_id=?""",
        (run_id,),
    ).fetchone()
    if row is None:
        raise ConfigError(f"edge-analysis: runtime config snapshot missing for {run_id}")
    return build_runtime_config(source.with_suffix(".snapshot.json"), json.loads(row[0]))


def _table_columns(conn: sqlite3.Connection, table: str) -> set[str]:
    return {r[1] for r in conn.execute(f"PRAGMA table_info({table})")}


def _baseline_reproduction(conn: sqlite3.Connection, run_id: str) -> dict[str, Any]:
    tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")}
    if "positions" not in tables:
        return {"status": "NO_POSITIONS_TABLE"}
    rows = conn.execute("SELECT * FROM positions WHERE run_id=?", (run_id,)).fetchall()
    if not rows:
        return {"status": "NO_TRADES_IN_SOURCE_RUN", "positions": 0}
    cols = _table_columns(conn, "positions")
    pnl = [r["net_pnl_lamports"] for r in rows if "net_pnl_lamports" in cols and r["net_pnl_lamports"] is not None]
    exits: dict[str, int] = {}
    states: dict[str, int] = {}
    for r in rows:
        if "exit_reason" in cols and r["exit_reason"]:
            exits[r["exit_reason"]] = exits.get(r["exit_reason"], 0) + 1
        if "state" in cols:
            states[r["state"]] = states.get(r["state"], 0) + 1
    wins = sum(1 for x in pnl if x > 0)
    return {
        "status": "REPRODUCED_FROM_PERSISTED_SOURCE_ROWS",
        "positions": len(rows),
        "resolved": len(pnl),
        "realized_net_pnl_lamports": sum(pnl),
        "wins": wins,
        "losses": sum(1 for x in pnl if x < 0),
        "win_rate": wins / len(pnl) if pnl else None,
        "expectancy_per_resolved_lamports": sum(pnl) / len(pnl) if pnl else None,
        "states": states,
        "exits": exits,
    }


def _basis_at(fees: list[Any], t: int) -> CostBasis:
    latest: dict[str, Any] = {}
    mapping = {
        "PROTOCOL_GLOBAL_ACCOUNT": "protocol",
        "NETWORK_FEE": "network",
        "RENT_EXEMPTION": "rent",
        "PRIORITY_POLICY": "priority_policy",
    }
    for snap in fees:
        if snap.received_mono_ns > t:
            break
        latest[mapping[snap.source_type]] = snap
    return CostBasis(latest.get("protocol"), latest.get("network"), latest.get("rent"), latest.get("priority_policy"))


def _fx_at(fx: list[Any], t: int) -> Any | None:
    latest = None
    for snap in fx:
        if snap.received_mono_ns > t:
            break
        latest = snap
    return latest


def _curve_from_event(ev: Any, st: Any, raw: RawEnvelope, cid: str) -> CurveState | None:
    if not ev.virtual_sol_reserves or not ev.virtual_token_reserves:
        return None
    if st.quote_scope_status != QuoteScopeStatus.SOL_WSOL:
        return None
    profile = st.account_profile_id
    return CurveState(
        cid, cid.split("|")[1] if "|" in cid else None, None, 0, raw.receive_mono_ns,
        ev.virtual_sol_reserves, ev.virtual_token_reserves, ev.real_sol_reserves, ev.real_token_reserves,
        st.venue_state, st.creator_fee_bps, st.creator is not None,
        ev.token_program or st.token_program, profile,
    )


def _entry_quote(cfg: RuntimeConfig, engine: QuoteEngine, state: CurveState, basis: CostBasis,
                 fx: Any | None, t: int) -> Any:
    coverage = CoverageState.OBSERVED
    sizing = cfg.strategy.get("sizing_mode")
    if sizing == "USD_SWAP_BUDGET":
        if fx is None:
            return InvalidQuote("FX_UNAVAILABLE")
        if cfg.strategy.get("entry_amount_semantics") == "TOTAL_SLOT_BUDGET_INCLUDING_ALL_COSTS":
            budget = fx.budget(str(cfg.portfolio.get("slot_capital_usd", cfg.strategy.get("trade_size_usd", "10"))),
                               t, cfg["fx"]["max_age_ms"])
            if not budget:
                return InvalidQuote("FX_STALE_OR_INVALID")
            reserve = engine.exit_reserve_lamports(basis, t, state)
            if reserve is None:
                return InvalidQuote("EXIT_RESERVE_UNAVAILABLE")
            return engine.quote_buy_total_slot_budget(state, basis, t, "edge_entry", coverage, budget, reserve)
        budget = fx.budget(str(cfg.strategy.get("trade_size_usd", "10")), t, cfg["fx"]["max_age_ms"])
        if not budget:
            return InvalidQuote("FX_STALE_OR_INVALID")
        return engine.quote_buy_swap_budget(state, basis, t, "edge_entry", coverage, budget)
    return engine.quote_buy(state, basis, t, "edge_entry", coverage, budget=int(cfg.strategy["entry_amount_lamports"]))


def _exit_value(engine: QuoteEngine, state: CurveState, basis: CostBasis, t: int, tokens: int) -> tuple[int | None, str | None]:
    q = engine.quote_sell(state, basis, t, "edge_exit", CoverageState.OBSERVED, tokens, 0)
    if isinstance(q, InvalidQuote):
        return None, q.reason
    assert q.swap_wallet_credit_lamports is not None
    return q.swap_wallet_credit_lamports - q.network_fee_total_lamports - q.tip_lamports, None


def _reconstruct_paths(conn: sqlite3.Connection, run_id: str, cfg: RuntimeConfig) -> tuple[dict[str, list[PathPoint]], dict[str, Any]]:
    replay = load_replay_input(conn, run_id, cfg)
    feeds = {f.id: f.transport for f in cfg.feeds}
    decoder = PumpDecoder(cfg.protocol)
    normalizer = Normalizer(decoder)
    reducer = MarketReducer(
        f"edge_{run_id}",
        max(cfg.storage["state_history_per_mint"], 1024),
        max(cfg.storage["max_tracked_mints"], 10000),
        max(cfg.storage["trade_log_per_mint"], 4096),
        max(cfg.storage["undecodable_points_max"], 8192),
    )
    reducer.signature_history_max = max(cfg.storage["signature_history_max"], 100000)
    reducer.dedup_keys_per_mint = max(cfg.storage["dedup_keys_per_mint"], 10000)
    reducer.set_quote_constants(cfg.protocol["wsol_mint"], cfg.protocol["native_sol_curve_sentinel"])

    paths: dict[str, list[PathPoint]] = {}
    creates: dict[str, int] = {}
    canonical = 0
    decode_errors = 0

    for receive, seq, feed_id, conn_id, utc, payload in replay.observations:
        raw = RawEnvelope(feed_id, conn_id, receive, utc, seq, payload)
        env = normalizer.normalize(raw, feeds.get(feed_id, "logsSubscribe"), 0)
        if env is None:
            continue
        if env.completeness.value in ("UNDECODABLE", "PARTIAL"):
            decode_errors += 1
        result = reducer.apply(env, raw)
        for ea in result.events:
            if not ea.is_new or ea.canonical_event_id is None or ea.event.mint is None:
                continue
            ev = ea.event
            mint = ev.mint
            st = reducer.states[mint]
            if ev.event_type.value == "CREATE":
                creates.setdefault(mint, receive)
            if mint not in creates:
                continue
            if ev.event_type.value not in ("CREATE", "BUY", "SELL", "COMPLETE", "MIGRATE"):
                continue
            canonical += 1
            if ev.virtual_sol_reserves and ev.virtual_token_reserves:
                paths.setdefault(mint, []).append(PathPoint(
                    mint=mint, event_type=ev.event_type.value,
                    t_ms=(receive - creates[mint]) / 1_000_000,
                    receive_mono_ns=receive, actor=ev.actor, creator=st.creator,
                    sol_amount=ev.sol_amount, token_amount=ev.token_amount,
                    virtual_sol_reserves=ev.virtual_sol_reserves,
                    virtual_token_reserves=ev.virtual_token_reserves,
                    real_sol_reserves=ev.real_sol_reserves,
                    real_token_reserves=ev.real_token_reserves,
                    creator_fee_bps=st.creator_fee_bps,
                    token_program=ev.token_program or st.token_program,
                    account_profile_id=st.account_profile_id,
                    venue_state=st.venue_state.value,
                    quote_scope_status=st.quote_scope_status.value,
                ))

    return paths, {
        "raw_observations": len(replay.observations),
        "unique_creates": len(creates),
        "mints_with_state_path": len(paths),
        "canonical_state_points": canonical,
        "partial_or_undecodable_frames": decode_errors,
        "dataset_hash": replay.dataset_hash,
        "fee_snapshots": len(replay.fee_snapshots),
        "fx_snapshots": len(replay.fx_snapshots),
        "_replay": replay,
    }


def _evaluate_paths(paths: dict[str, list[PathPoint]], cfg: RuntimeConfig, replay: Any) -> tuple[list[dict[str, Any]], list[PathPoint]]:
    engine = QuoteEngine(ExecutionPolicy.from_config(cfg))
    summaries: list[dict[str, Any]] = []
    flat: list[PathPoint] = []
    fees = sorted(replay.fee_snapshots, key=lambda s: s.received_mono_ns)
    fx = sorted(replay.fx_snapshots, key=lambda s: s.received_mono_ns)

    for mint, points in sorted(paths.items()):
        points.sort(key=lambda p: p.receive_mono_ns)
        entry_idx = None
        entry_q = None
        entry_state = None
        entry_reason = "NO_VALID_ENTRY_STATE"
        for i, p in enumerate(points):
            if p.venue_state != VenueState.BONDING_CURVE_ACTIVE.value or p.quote_scope_status != QuoteScopeStatus.SOL_WSOL.value:
                continue
            state = CurveState(
                f"edge|{mint}|{i}", mint, None, i, p.receive_mono_ns,
                p.virtual_sol_reserves, p.virtual_token_reserves, p.real_sol_reserves, p.real_token_reserves,
                VenueState.BONDING_CURVE_ACTIVE, p.creator_fee_bps, p.creator is not None,
                p.token_program, p.account_profile_id,
            )
            basis = _basis_at(fees, p.receive_mono_ns)
            q = _entry_quote(cfg, engine, state, basis, _fx_at(fx, p.receive_mono_ns), p.receive_mono_ns)
            if not isinstance(q, InvalidQuote):
                entry_idx, entry_q, entry_state = i, q, state
                entry_reason = None
                break
            entry_reason = q.reason

        if entry_q is None or entry_idx is None or entry_state is None:
            summaries.append({"mint": mint, "entry_status": "UNAVAILABLE", "entry_reason": entry_reason})
            flat.extend(points)
            continue

        entry_cost = entry_q.entry_economic_cost
        tokens = entry_q.token_amount_base_units
        entry_t = points[entry_idx].receive_mono_ns
        entry_t_ms = points[entry_idx].t_ms
        creator = points[entry_idx].creator
        target_hits: dict[str, float] = {}
        creator_buys: list[float] = []
        creator_sells: list[tuple[float, Decimal | None]] = []

        for j, p in enumerate(points[entry_idx:]):
            if p.venue_state != VenueState.BONDING_CURVE_ACTIVE.value:
                p.exit_invalid_reason = "VENUE_NOT_ACTIVE"
                continue
            state = CurveState(
                f"edge|{mint}|{entry_idx+j}", mint, None, entry_idx+j, p.receive_mono_ns,
                p.virtual_sol_reserves, p.virtual_token_reserves, p.real_sol_reserves, p.real_token_reserves,
                VenueState.BONDING_CURVE_ACTIVE, p.creator_fee_bps, p.creator is not None,
                p.token_program, p.account_profile_id,
            )
            value, reason = _exit_value(engine, state, _basis_at(fees, p.receive_mono_ns), p.receive_mono_ns, tokens)
            p.exit_invalid_reason = reason
            if value is not None and entry_cost > 0:
                p.exit_value_lamports = value
                p.net_pnl_lamports = value - entry_cost
                p.net_multiple = Decimal(value) / Decimal(entry_cost)
                for target in TARGETS:
                    key = format(target, "f")
                    if key not in target_hits and p.net_multiple >= target:
                        target_hits[key] = p.t_ms - entry_t_ms
            if creator and p.actor == creator:
                if p.event_type == "BUY" and p.receive_mono_ns > entry_t:
                    creator_buys.append(p.t_ms - entry_t_ms)
                elif p.event_type == "SELL" and p.receive_mono_ns > entry_t:
                    creator_sells.append((p.t_ms - entry_t_ms, p.net_multiple))

        valid = [p for p in points[entry_idx:] if p.net_multiple is not None]
        peak = max(valid, key=lambda p: p.net_multiple) if valid else None
        trough = min(valid, key=lambda p: p.net_multiple) if valid else None
        checkpoints: dict[str, Any] = {}
        for ms in CHECKPOINTS_MS:
            prior = [p for p in valid if (p.t_ms - entry_t_ms) <= ms]
            after = [p for p in valid if (p.t_ms - entry_t_ms) >= ms]
            mark = prior[-1] if prior else None
            nxt = after[0] if after else None
            checkpoints[str(ms)] = {
                "mark_multiple": str(mark.net_multiple) if mark else None,
                "mark_age_ms": (ms - (mark.t_ms - entry_t_ms)) if mark else None,
                "next_event_multiple": str(nxt.net_multiple) if nxt else None,
                "next_event_delay_ms": ((nxt.t_ms - entry_t_ms) - ms) if nxt else None,
            }

        summaries.append({
            "mint": mint,
            "entry_status": "OK",
            "entry_t_from_create_ms": entry_t_ms,
            "entry_event_type": points[entry_idx].event_type,
            "entry_cost_lamports": entry_cost,
            "entry_tokens": tokens,
            "creator": creator,
            "peak_net_multiple": str(peak.net_multiple) if peak else None,
            "peak_t_from_entry_ms": (peak.t_ms - entry_t_ms) if peak else None,
            "trough_net_multiple": str(trough.net_multiple) if trough else None,
            "trough_t_from_entry_ms": (trough.t_ms - entry_t_ms) if trough else None,
            "creator_rebuys": len(creator_buys),
            "first_creator_rebuy_ms": creator_buys[0] if creator_buys else None,
            "creator_sells": len(creator_sells),
            "first_creator_sell_ms": creator_sells[0][0] if creator_sells else None,
            "multiple_at_first_creator_sell": str(creator_sells[0][1]) if creator_sells and creator_sells[0][1] is not None else None,
            "highest_target_hit": max((k for k in target_hits), key=Decimal) if target_hits else None,
            "first_target_hit_ms": target_hits,
            "checkpoints": checkpoints,
        })
        flat.extend(points)

    return summaries, flat


def _event_for_rule(points: list[PathPoint], entry_t_ms: float, *, target: Decimal | None = None,
                    dev: bool = False, hard_ms: int | None = None, min_hold_ms: int = 0,
                    exit_if_not_profitable_after_hold: bool = False) -> PathPoint | None:
    creator = next((p.creator for p in points if p.creator), None)
    valid = [p for p in points if p.net_multiple is not None and (p.t_ms - entry_t_ms) >= min_hold_ms]
    for p in valid:
        age = p.t_ms - entry_t_ms
        if target is not None and p.net_multiple is not None and p.net_multiple >= target:
            return p
        if dev and creator and p.actor == creator and p.event_type == "SELL":
            return p
        if exit_if_not_profitable_after_hold and p.net_multiple is not None and p.net_multiple <= 1:
            return p
        if hard_ms is not None and age >= hard_ms:
            return p
    return None


def _rule_sweep(summaries: list[dict[str, Any]], flat: list[PathPoint]) -> list[dict[str, Any]]:
    by_mint: dict[str, list[PathPoint]] = {}
    for p in flat:
        by_mint.setdefault(p.mint, []).append(p)
    entry_time = {s["mint"]: s.get("entry_t_from_create_ms") for s in summaries if s.get("entry_status") == "OK"}

    rules: list[tuple[str, Decimal | None, bool, int | None, int, bool]] = []
    rules.append(("DEV_ONLY", None, True, None, 0, False))
    for hard in HARD_EXITS_MS:
        rules.append((f"TIME_{hard}MS", None, False, hard, 0, False))
    for target in TARGETS:
        ts = format(target, "f")
        rules.append((f"TARGET_{ts}", target, False, None, 0, False))
        rules.append((f"TARGET_{ts}_OR_DEV", target, True, None, 0, False))
        for hard in HARD_EXITS_MS:
            rules.append((f"TARGET_{ts}_OR_TIME_{hard}MS", target, False, hard, 0, False))
            rules.append((f"TARGET_{ts}_OR_DEV_OR_TIME_{hard}MS", target, True, hard, 0, False))
    for hold in MIN_HOLDS_MS:
        for hard in HARD_EXITS_MS:
            if hard >= hold:
                rules.append((f"HOLD_{hold}MS_EXIT_LOSER_OR_TIME_{hard}MS", None, False, hard, hold, True))

    out: list[dict[str, Any]] = []
    for name, target, dev, hard, hold, loser in rules:
        pnls: list[int] = []
        multiples: list[Decimal] = []
        unresolved = 0
        for mint, t0 in entry_time.items():
            pts = sorted(by_mint.get(mint, []), key=lambda p: p.receive_mono_ns)
            chosen = _event_for_rule(pts, t0, target=target, dev=dev, hard_ms=hard, min_hold_ms=hold,
                                     exit_if_not_profitable_after_hold=loser)
            if chosen is None or chosen.net_pnl_lamports is None or chosen.net_multiple is None:
                unresolved += 1
                continue
            pnls.append(chosen.net_pnl_lamports)
            multiples.append(chosen.net_multiple)
        wins = sum(1 for x in pnls if x > 0)
        out.append({
            "rule": name, "target": format(target, "f") if target is not None else None,
            "dev_exit": dev, "hard_exit_ms": hard, "min_hold_ms": hold,
            "exit_if_not_profitable_after_hold": loser,
            "resolved": len(pnls), "unresolved": unresolved, "wins": wins,
            "win_rate": wins / len(pnls) if pnls else None,
            "net_pnl_lamports": sum(pnls),
            "expectancy_lamports": sum(pnls) / len(pnls) if pnls else None,
            "median_multiple": str(statistics.median(multiples)) if multiples else None,
        })
    return out


def run_edge_analysis(source: Path, out_dir: Path, *, run_id: str | None = None, campaign: str | None = None) -> dict[str, Any]:
    db_path, temp = _materialize_source(source)
    conn = open_connection(db_path, read_only=True)
    try:
        selected = run_id or _latest_run(conn, campaign)
        cfg = _runtime_config(conn, selected, db_path)
        baseline = _baseline_reproduction(conn, selected)
        paths, capture = _reconstruct_paths(conn, selected, cfg)
        replay = capture.pop("_replay")
        summaries, flat = _evaluate_paths(paths, cfg, replay)
        sweep = _rule_sweep(summaries, flat)

        valid_summaries = [s for s in summaries if s.get("entry_status") == "OK"]
        peak_times = [s["peak_t_from_entry_ms"] for s in valid_summaries if s.get("peak_t_from_entry_ms") is not None]
        peak_mult = [Decimal(s["peak_net_multiple"]) for s in valid_summaries if s.get("peak_net_multiple") is not None]
        ranked = sorted(
            sweep,
            key=lambda r: (r["unresolved"], -(r["expectancy_lamports"] if r["expectancy_lamports"] is not None else -10**30)),
        )
        best_resolved = max((r for r in sweep if r["unresolved"] == 0), key=lambda r: r["expectancy_lamports"], default=None)
        report = {
            "source": str(source),
            "database_used": str(db_path),
            "run_id": selected,
            "config_hash": cfg.config_hash,
            "strategy_id_in_source_config": cfg.strategy_id,
            "analysis_status": "IN_SAMPLE_DIAGNOSTIC_ONLY",
            "warning": "Do not promote the best rule directly. Freeze one candidate, then validate on a fresh untouched CAPTURE run.",
            "baseline_reproduction": baseline,
            "capture": capture,
            "launches_with_hypothetical_entry": len(valid_summaries),
            "peak_time_ms": {
                "median": statistics.median(peak_times) if peak_times else None,
                "p25": sorted(peak_times)[len(peak_times)//4] if peak_times else None,
                "p75": sorted(peak_times)[(3*len(peak_times))//4] if peak_times else None,
            },
            "peak_multiple": {
                "median": str(statistics.median(peak_mult)) if peak_mult else None,
                "max": str(max(peak_mult)) if peak_mult else None,
            },
            "best_fully_resolved_rule": best_resolved,
            "top_rules_preview": ranked[:25],
        }

        out_dir.mkdir(parents=True, exist_ok=True)
        (out_dir / "edge_summary.json").write_text(json.dumps(report, indent=2, sort_keys=True), encoding="utf-8")
        (out_dir / "baseline_reproduction.json").write_text(json.dumps(baseline, indent=2, sort_keys=True), encoding="utf-8")

        with (out_dir / "per_mint_summary.csv").open("w", newline="", encoding="utf-8") as fh:
            cols = [
                "mint","entry_status","entry_reason","entry_t_from_create_ms","entry_event_type","entry_cost_lamports","entry_tokens",
                "creator","peak_net_multiple","peak_t_from_entry_ms","trough_net_multiple","trough_t_from_entry_ms",
                "creator_rebuys","first_creator_rebuy_ms","creator_sells","first_creator_sell_ms",
                "multiple_at_first_creator_sell","highest_target_hit"
            ]
            w = csv.DictWriter(fh, fieldnames=cols)
            w.writeheader()
            for s in summaries:
                w.writerow({k: s.get(k) for k in cols})

        with (out_dir / "per_mint_paths.csv").open("w", newline="", encoding="utf-8") as fh:
            cols = [
                "mint","event_type","t_ms","receive_mono_ns","actor","creator","sol_amount","token_amount",
                "virtual_sol_reserves","virtual_token_reserves","real_sol_reserves","real_token_reserves",
                "creator_fee_bps","venue_state","quote_scope_status","net_multiple","net_pnl_lamports","exit_invalid_reason"
            ]
            w = csv.DictWriter(fh, fieldnames=cols)
            w.writeheader()
            for p in flat:
                w.writerow({
                    "mint":p.mint,"event_type":p.event_type,"t_ms":p.t_ms,"receive_mono_ns":p.receive_mono_ns,
                    "actor":p.actor,"creator":p.creator,"sol_amount":p.sol_amount,"token_amount":p.token_amount,
                    "virtual_sol_reserves":p.virtual_sol_reserves,"virtual_token_reserves":p.virtual_token_reserves,
                    "real_sol_reserves":p.real_sol_reserves,"real_token_reserves":p.real_token_reserves,
                    "creator_fee_bps":p.creator_fee_bps,"venue_state":p.venue_state,"quote_scope_status":p.quote_scope_status,
                    "net_multiple":str(p.net_multiple) if p.net_multiple is not None else None,
                    "net_pnl_lamports":p.net_pnl_lamports,"exit_invalid_reason":p.exit_invalid_reason,
                })

        with (out_dir / "combined_exit_rules.csv").open("w", newline="", encoding="utf-8") as fh:
            cols = ["rule","target","dev_exit","hard_exit_ms","min_hold_ms","exit_if_not_profitable_after_hold",
                    "resolved","unresolved","wins","win_rate","net_pnl_lamports","expectancy_lamports","median_multiple"]
            w = csv.DictWriter(fh, fieldnames=cols)
            w.writeheader()
            w.writerows(sweep)

        with (out_dir / "checkpoint_multiples.csv").open("w", newline="", encoding="utf-8") as fh:
            cols = ["mint","checkpoint_ms","mark_multiple","mark_age_ms","next_event_multiple","next_event_delay_ms"]
            w = csv.DictWriter(fh, fieldnames=cols)
            w.writeheader()
            for s in valid_summaries:
                for ms, v in s["checkpoints"].items():
                    w.writerow({"mint":s["mint"],"checkpoint_ms":ms,**v})

        with (out_dir / "target_hits.csv").open("w", newline="", encoding="utf-8") as fh:
            w = csv.writer(fh)
            w.writerow(["mint","target","first_hit_ms"])
            for s in valid_summaries:
                for target, t in s["first_target_hit_ms"].items():
                    w.writerow([s["mint"], target, t])

        return report
    finally:
        conn.close()
        if temp is not None:
            temp.cleanup()

"""Outside the hot path: aggregate latencies, costs, ledger/PnL, quality and uncertainty from a
committed SQLite snapshot and write report.json, summary.md and CSVs (spec: reporting.py).
The Reporter never mutates decisions or ledger.
"""

from __future__ import annotations

import csv
import json
import math
import random
import sqlite3
import statistics
import time
from collections import defaultdict
from decimal import Decimal
from pathlib import Path
from typing import Any

from darkbot.config import canonical_json
from darkbot.domain import LAMPORTS_PER_SOL, NS_PER_MS, QualificationStatus, utc_iso

QUERY_VERSION = "REPORT_QUERY_V1"
COUNTERFACTUAL_NOTE = ("Historischer Fremd-Orderflow reagiert nicht auf unseren hypothetischen Trade (Gegenfaktualitaetsannahme). Simulierte Inklusion ist eine Annahme, keine gemessene Landung. "
                       "Fill-Evidenz ist OBSERVED_STATE_PROXY. Keine Zahl behauptet global frueheste Erkennung, sicheren Fill oder garantierten Profit.")


def percentiles(values: list[int]) -> dict[str, Any]:
    n = len(values)
    if n == 0:
        return {"n": 0, "min": None, "p50": None, "p95": None, "p99": None, "max": None, "p99_low_confidence": True}
    s = sorted(values)

    def nr(p: float) -> int:
        return s[max(1, math.ceil(p * n)) - 1]

    return {"n": n, "min": s[0], "p50": nr(0.5), "p95": nr(0.95), "p99": nr(0.99), "max": s[-1], "p99_low_confidence": n < 100}


def _rows(conn: sqlite3.Connection, sql: str, params: tuple = ()) -> list[dict[str, Any]]:
    conn.row_factory = sqlite3.Row
    return [dict(r) for r in conn.execute(sql, params).fetchall()]


def block_bootstrap(samples: list[tuple[int, int]], resamples: int, seed: int, confidence: Decimal) -> dict[str, Any]:
    """samples: (block_id, value). Resample whole blocks with replacement; return mean and CI of the mean per-sample value."""
    blocks: dict[int, list[int]] = defaultdict(list)
    for b, v in samples:
        blocks[b].append(v)
    keys = sorted(blocks)
    if len(keys) < 2:
        return {"blocks": len(keys), "mean": (statistics.fmean(v for _, v in samples) if samples else None), "ci_low": None, "ci_high": None, "status": "INSUFFICIENT_BLOCKS", "seed": seed, "resamples": resamples}
    rng = random.Random(seed)
    means: list[float] = []
    for _ in range(resamples):
        chosen = [keys[rng.randrange(len(keys))] for _ in keys]
        vals = [v for k in chosen for v in blocks[k]]
        means.append(sum(vals) / len(vals))
    means.sort()
    alpha = (1 - float(confidence)) / 2
    lo = means[max(0, math.ceil(alpha * len(means)) - 1)]
    hi = means[min(len(means) - 1, math.ceil((1 - alpha) * len(means)) - 1)]
    return {"blocks": len(keys), "mean": sum(v for _, v in samples) / len(samples), "ci_low": lo, "ci_high": hi, "status": "OK", "seed": seed, "resamples": resamples}


class Reporter:
    def __init__(self, conn: sqlite3.Connection, cfg_research: dict[str, Any] | None = None) -> None:
        self.conn = conn
        self.research = cfg_research or {}

    def build(self, run_id: str) -> dict[str, Any]:
        c = self.conn
        run = _rows(c, "SELECT * FROM runs WHERE run_id=?", (run_id,))
        if not run:
            raise ValueError(f"run {run_id} not found")
        run = run[0]
        cfg = json.loads(_rows(c, "SELECT canonical_json FROM config_snapshots WHERE config_hash=?", (run["config_hash"],))[0]["canonical_json"])
        positions = _rows(c, "SELECT * FROM positions WHERE run_id=? ORDER BY position_id", (run_id,))
        attempts = _rows(c, "SELECT * FROM execution_attempts WHERE run_id=? ORDER BY submit_mono_ns, attempt_id", (run_id,))
        ledger = _rows(c, "SELECT * FROM ledger WHERE run_id=? ORDER BY mono_ns, ledger_entry_id", (run_id,))
        decisions = _rows(c, "SELECT * FROM decisions WHERE run_id=? ORDER BY decision_done_mono_ns, decision_id", (run_id,))
        quality = _rows(c, "SELECT * FROM quality_events WHERE run_id=? ORDER BY start_mono_ns", (run_id,))
        fees = _rows(c, "SELECT fee_snapshot_id, source_type, validation_status, normalized_fee_components, observed_utc FROM fee_snapshots WHERE run_id=? ORDER BY received_mono_ns", (run_id,))
        creates = _rows(c, "SELECT canonical_event_id, mint, first_seen_mono_ns, actionable_mono_ns, winner_feed_id, quote_scope_status, canonical_coverage_state FROM market_events WHERE run_id=? AND event_type='CREATE' ORDER BY first_seen_mono_ns", (run_id,))
        obs = _rows(c, "SELECT feed_id, canonical_event_id, receive_mono_ns, decode_done_mono_ns, event_type, completeness, validation_status, mint FROM feed_observations WHERE run_id=? ORDER BY receive_mono_ns", (run_id,))
        initial = run["start_balance_lamports"]

        # ---------------- economics
        by_state: dict[str, int] = defaultdict(int)
        for p in positions:
            by_state[p["state"]] += 1
        closed = [p for p in positions if p["state"] == "CLOSED"]
        failed_entries = [p for p in positions if p["state"] == "ENTRY_FAILED"]
        exposed = [p for p in positions if p["state"] in ("OPEN", "EXIT_PENDING", "ENTRY_PENDING", "ENTRY_INTENT", "UNRESOLVED")]
        resolved = closed + failed_entries
        realized = sum(p["net_pnl_lamports"] for p in resolved if p["net_pnl_lamports"] is not None)
        exposed_cost = sum((p["entry_economic_cost_lamports"] or 0) for p in exposed)
        exposed_fees_unknown_cost = sum(1 for a in attempts if a["fees_charged_lamports"] is None and a["status"] not in ("SUBMITTED",))
        conservative_lower_bound = realized - exposed_cost - sum((p["total_fees_lamports"] or 0) for p in exposed if p["entry_economic_cost_lamports"] is None)
        pnls = [p["net_pnl_lamports"] for p in closed]
        wins = [v for v in pnls if v > 0]
        losses = [v for v in pnls if v < 0]
        gross_loss = -sum(losses)
        profit_factor = None if gross_loss == 0 else sum(wins) / gross_loss
        cash_sum = sum(l["cash_delta_lamports"] for l in ledger)
        rent_sum = sum(l["locked_rent_delta_lamports"] for l in ledger)
        fee_kinds = {"protocol_fee_lamports": 0, "creator_fee_lamports": 0, "network_fee_total_lamports": 0, "sender_tip_lamports": 0, "rent_locked_lamports": 0, "rent_refunded_lamports": 0}
        evidence: dict[str, int] = defaultdict(int)
        fill_evidence: dict[str, int] = defaultdict(int)
        for a in attempts:
            evidence[a["cost_evidence_status"]] += 1
            fill_evidence[a["fill_evidence_status"]] += 1
            if a["status"] == "FILLED":
                for k in ("protocol_fee_lamports", "creator_fee_lamports", "network_fee_total_lamports", "sender_tip_lamports", "rent_locked_lamports", "rent_refunded_lamports"):
                    fee_kinds[k] += a[k] or 0
            elif a["status"] not in ("SUBMITTED", "UNKNOWN", "CANCELLED_BEFORE_SUBMIT") and a["fees_charged_lamports"] is not None:
                fee_kinds["network_fee_total_lamports"] += a["fees_charged_lamports"]
        equity, peak, dd = initial, initial, 0
        for p in sorted(resolved, key=lambda p: (p["close_mono_ns"] or 0)):
            equity += p["net_pnl_lamports"] or 0
            peak = max(peak, equity)
            dd = max(dd, peak - equity)
        holds = [p["close_mono_ns"] - p["open_mono_ns"] for p in closed if p["open_mono_ns"] and p["close_mono_ns"]]
        exits = {r: sum(1 for p in closed if p["exit_reason"] == r) for r in ("CREATOR_SELL", "TARGET", "TIMEOUT")}
        ineligible: dict[str, int] = defaultdict(int)
        for d in decisions:
            if d["action"] == "SKIP":
                ineligible[d["reason"]] += 1
        run_span_ns = (max((o["receive_mono_ns"] for o in obs), default=0) - min((o["receive_mono_ns"] for o in obs), default=0)) if obs else 0
        obs_end = max((o["receive_mono_ns"] for o in obs), default=0)
        obs_start = min((o["receive_mono_ns"] for o in obs), default=0)
        run_start = run["started_mono_ns"] if run.get("started_mono_ns") is not None else obs_start  # recorded run window, not the first/last frame
        run_end = run["ended_mono_ns"] if run.get("ended_mono_ns") is not None else obs_end
        window_ns = max(0, run_end - run_start)
        entry_submit = {a["position_id"]: a["submit_mono_ns"] for a in attempts if a["side"] == "BUY" and a["attempt_number"] == 1}
        busy_ns = 0
        for p in positions:  # one interval per position: from entry submit (capital reserved) to close/fail/run end; never double counted
            start = entry_submit.get(p["position_id"], p["open_mono_ns"])
            if start is None:
                continue
            end = p["close_mono_ns"] if p["close_mono_ns"] is not None else run_end
            busy_ns += max(0, end - start)
        slots = max(1, int(cfg["portfolio"]["max_open_positions"]))
        block_len = self.research.get("block_length_ms", cfg["research"]["block_length_ms"]) * NS_PER_MS
        decision_time = {d["decision_id"]: d["receive_mono_ns"] for d in decisions}
        blocks = [(int((p["open_mono_ns"] or decision_time.get(p["entry_decision_id"]) or 0) // block_len), p["net_pnl_lamports"]) for p in resolved if p["net_pnl_lamports"] is not None]  # failed intents count too
        boot = block_bootstrap(blocks, int(cfg["research"]["bootstrap_resamples"]), int(cfg["research"]["random_seed"]), Decimal(cfg["research"]["bootstrap_confidence"]))
        economic = {
            "positions_by_state": dict(by_state), "entry_intents": len(positions), "entry_fills": sum(1 for a in attempts if a["side"] == "BUY" and a["status"] == "FILLED"), "entry_failed": len(failed_entries),
            "exits": exits, "exit_failed_attempts": sum(1 for a in attempts if a["side"] == "SELL" and a["status"] not in ("FILLED", "SUBMITTED", "UNKNOWN")),
            "retry_exhausted": sum(1 for p in positions if p["unresolved_reason"] == "EXIT_RETRIES_EXHAUSTED"), "unresolved": by_state.get("UNRESOLVED", 0),
            "unresolved_by_reason": _count(positions, "unresolved_reason"), "censored": sum(1 for p in positions if (p["research_status"] or "").startswith("CENSORED")),
            "open_at_end": sum(1 for p in positions if p["state"] in ("OPEN", "EXIT_PENDING", "ENTRY_PENDING")),
            "realized_net_pnl_lamports": realized, "realized_net_pnl_sol": str(Decimal(realized) / LAMPORTS_PER_SOL), "unrealized_status": "UNKNOWN_EQUITY_INTERVAL" if exposed else "NONE",
            "unresolved_exposure_entry_cost_lamports": exposed_cost, "conservative_lower_bound_pnl_lamports_token_residual_zero": conservative_lower_bound,
            "locked_rent_lamports_end": rent_sum, "cash_reconciliation": {"initial": initial, "ledger_cash_sum": cash_sum, "end_cash_per_ledger": initial + cash_sum, "run_end_balance": run["end_balance_lamports"],
                                                                         "reconciled": run["end_balance_lamports"] is None or run["end_balance_lamports"] == initial + cash_sum},
            "expectancy_per_intent_lamports": (realized / len(resolved)) if resolved else None, "expectancy_per_fill_lamports": (sum(pnls) / len(pnls)) if pnls else None,
            "resolved_intents": len(resolved), "unresolved_fraction": (len(exposed) / len(positions)) if positions else None, "median_pnl_lamports": statistics.median(pnls) if pnls else None,
            "win_rate": (len(wins) / len(pnls)) if pnls else None, "avg_win_lamports": (sum(wins) / len(wins)) if wins else None, "avg_loss_lamports": (sum(losses) / len(losses)) if losses else None,
            "profit_factor": profit_factor, "profit_factor_special_case": None if gross_loss else ("NO_LOSS_SAMPLE" if pnls else "NO_TRADES"),
            "max_drawdown_lamports_on_close_time_equity": dd, "capital_utilization_fraction": min(1.0, busy_ns / (window_ns * slots)) if window_ns else None, "run_window_ns": window_ns,
            "fees_by_kind_lamports": fee_kinds, "fee_share_of_entry_cost": (sum(fee_kinds[k] for k in ("protocol_fee_lamports", "network_fee_total_lamports", "sender_tip_lamports")) / sum(p["entry_economic_cost_lamports"] for p in closed)) if closed and sum(p["entry_economic_cost_lamports"] for p in closed) else None,
            "cost_evidence_counts": dict(evidence), "fill_evidence_counts": dict(fill_evidence), "attempts_with_unknown_costs": exposed_fees_unknown_cost,
            "quote_age_ns": percentiles([a["proxy_age_ns"] for a in attempts if a["proxy_age_ns"] is not None]), "hold_duration_ns": percentiles(holds),
            "time_to_entry_ns": percentiles([a["modeled_fill_mono_ns"] - d["receive_mono_ns"] for a in attempts for d in decisions if a["side"] == "BUY" and a["status"] == "FILLED" and d["decision_id"] == a["attempt_decision_id"] and d["receive_mono_ns"] is not None]),
            "per_attempt_costs": [{"attempt_id": a["attempt_id"], "side": a["side"], "status": a["status"], "fees_charged_lamports": a["fees_charged_lamports"], "protocol_fee_lamports": a["protocol_fee_lamports"], "creator_fee_lamports": a["creator_fee_lamports"],
                                   "network_fee_total_lamports": a["network_fee_total_lamports"], "priority_fee_lamports": a["priority_fee_lamports"], "sender_tip_lamports": a["sender_tip_lamports"], "rent_locked_lamports": a["rent_locked_lamports"],
                                   "cost_evidence_status": a["cost_evidence_status"], "fee_reconciliation_status": a["fee_reconciliation_status"], "fee_snapshot_id": a["fee_snapshot_id"], "fill_evidence_status": a["fill_evidence_status"]} for a in attempts],
            "block_bootstrap_per_intent_pnl": boot, "daily_pnl": self._daily(resolved, run, run_start, decision_time),
        }

        run_span_ns = window_ns
        # ---------------- latency
        entry_decisions = [d for d in decisions if d["action"] == "ENTRY"]
        latency = {
            "class_legend": {"MEASURED_LOCAL": "same clock epoch, local CPU path", "MEASURED_RELATIVE": "feed vs feed, no absolute chain latency", "SIMULATED": "declared model delay", "UNKNOWN": "missing timestamp -> NULL"},
            "receive_to_decode_ns": percentiles([o["decode_done_mono_ns"] - o["receive_mono_ns"] for o in obs if o["decode_done_mono_ns"] is not None]),
            "decode_to_actionable_ns": percentiles([cr["actionable_mono_ns"] - o["decode_done_mono_ns"] for cr in creates if cr["actionable_mono_ns"] for o in obs if o["canonical_event_id"] == cr["canonical_event_id"] and o["feed_id"] == cr["winner_feed_id"] and o["decode_done_mono_ns"]][:5000]),
            "ready_to_decision_ns": percentiles([d["decision_done_mono_ns"] - d["data_ready_mono_ns"] for d in decisions if d["data_ready_mono_ns"] is not None]),
            "receive_to_decision_ns": percentiles([d["decision_done_mono_ns"] - d["receive_mono_ns"] for d in entry_decisions if d["receive_mono_ns"] is not None]),
            "decision_to_enqueue_ns": percentiles([d["intent_enqueued_mono_ns"] - d["decision_done_mono_ns"] for d in decisions if d["intent_enqueued_mono_ns"] is not None]),
            "callback_lateness_ns": percentiles([max(0, a["callback_actual_mono_ns"] - a["scheduled_fill_mono_ns"]) for a in attempts if a["callback_actual_mono_ns"] is not None]),
            "modeled_entry_delay_ns": {"class": "SIMULATED", "value": cfg["latency_model"]["entry_delay_ms"] * NS_PER_MS}, "modeled_exit_delay_ns": {"class": "SIMULATED", "value": cfg["latency_model"]["exit_delay_ms"] * NS_PER_MS},
            "first_hint_to_actionable_ns": percentiles(self._hint_to_actionable(obs, creates)),
            "submit_to_ack_ns": {"class": "FUTURE_LIVE_MEASURED_NOT_FILL", "n": 0}, "submit_to_observed_chain_execution_ns": {"class": "FUTURE_LIVE_OBSERVATION_INCLUDES_RETURN_FEED_DELAY", "n": 0},
            "feeds": self._feed_report(obs, creates, cfg),
        }
        # ---------------- quality
        canonical_gaps = [q for q in quality if q["canonical_gap"]]
        gap_positions = [p["position_id"] for p in positions if p["canonical_provider_gap_flag"]]
        migration_exposures = [p["position_id"] for p in positions if p["state"] == "UNRESOLVED" and (p["unresolved_reason"] or "").startswith(("MIGRAT", "CURVE_COMPLETE"))]
        quote_scope = _count(creates, "quote_scope_status")
        qual = {
            "quality_events": len(quality), "canonical_gap_events": len(canonical_gaps), "provider_disconnects": sum(1 for q in quality if q["kind"] == "PROVIDER_DISCONNECT"), "recorder_overflows": sum(1 for q in quality if q["kind"] == "RECORDER_OVERFLOW"),
            "internal_errors": sum(1 for q in quality if q["kind"].endswith("INTERNAL_ERROR")), "positions_with_canonical_gap": gap_positions, "migration_or_curve_complete_exposures": migration_exposures,
            "sol_only_rejects": {k: v for k, v in ineligible.items() if k in ("UNSUPPORTED_QUOTE",)}, "quote_unknown_rejects": {k: v for k, v in ineligible.items() if k.startswith("QUOTE_UNKNOWN")},
            "create_quote_scope_counts": quote_scope, "ineligible_by_reason": dict(ineligible), "coverage_version": "CANONICAL_COVERAGE_V2", "feature_sets": _count(_rows(c, "SELECT feature_set_id FROM feature_snapshots WHERE run_id=?", (run_id,)), "feature_set_id"),
            "fee_snapshots": [{"id": f["fee_snapshot_id"], "type": f["source_type"], "status": f["validation_status"]} for f in fees][-8:],
            "fee_snapshot_status_counts": _count(fees, "validation_status"), "run_status": run["status"], "run_quality_status": run["quality_status"],
        }
        status, reasons = self._qualify(cfg, economic, qual, fees, attempts, positions, boot)
        return {"query_version": QUERY_VERSION, "run_id": run_id, "campaign_id": run["campaign_id"], "mode": run["mode"], "config_hash": run["config_hash"], "code_revision": run["code_revision"], "spec_hash": run["spec_hash"],
                "dataset_hash": run["dataset_hash"], "host_clock_epoch_id": run["host_clock_epoch_id"], "created_utc": utc_iso(time.time_ns()), "creates_observed_union": len(creates), "creates_actionable": sum(1 for cr in creates if cr["actionable_mono_ns"]),
                "economic": economic, "latency": latency, "quality": qual, "qualification_status": status.value, "qualification_reasons": reasons,
                "model_assumptions": {"fill_evidence": "OBSERVED_STATE_PROXY", "latency_model": cfg["latency_model"], "slippage_bps": cfg["execution"]["slippage_bps"], "target_net_multiple": cfg["strategy"]["target_net_multiple"],
                                      "counterfactual_note": COUNTERFACTUAL_NOTE, "exclusions": {"decisions_skipped": dict(ineligible)}}}

    @staticmethod
    def _daily(resolved: list[dict[str, Any]], run: dict[str, Any], run_start_mono: int, decision_time: dict[str, int | None]) -> dict[str, Any]:
        """Net PnL per UTC day of the close (or failed-entry decision): day = run start UTC + (event mono - first observation mono)."""
        import datetime as _dt

        try:  # full precision anchor (microseconds) of the recorded run start, paired with started_mono_ns
            text = run["started_utc"].rstrip("Z")
            start = _dt.datetime.strptime(text, "%Y-%m-%dT%H:%M:%S.%f" if "." in text else "%Y-%m-%dT%H:%M:%S").replace(tzinfo=_dt.timezone.utc)
        except ValueError:
            start = None
        out: dict[str, int] = defaultdict(int)
        for p in resolved:
            mono = p["close_mono_ns"] or decision_time.get(p["entry_decision_id"]) or run_start_mono
            day = (start + _dt.timedelta(microseconds=(mono - run_start_mono) // 1000)).strftime("%Y-%m-%d") if start else "UNKNOWN"
            out[day] += p["net_pnl_lamports"] or 0
        return {"note": "net PnL per UTC day of position close (failed entries: decision day), derived from run start UTC plus monotonic offset; no extrapolation", "by_day": dict(out)}

    @staticmethod
    def _hint_to_actionable(obs: list[dict[str, Any]], creates: list[dict[str, Any]]) -> list[int]:
        hints: dict[str, int] = {}
        for o in obs:
            if o["event_type"] == "HINT_CREATE" and o["mint"] and o["mint"] not in hints:
                hints[o["mint"]] = o["receive_mono_ns"]
        out = []
        for cr in creates:
            if cr["actionable_mono_ns"] and cr["mint"] in hints:
                out.append(cr["actionable_mono_ns"] - min(hints[cr["mint"]], cr["first_seen_mono_ns"]))
        return out

    @staticmethod
    def _feed_report(obs: list[dict[str, Any]], creates: list[dict[str, Any]], cfg: dict[str, Any]) -> dict[str, Any]:
        feeds = [f["id"] for f in cfg["feeds"]]
        by_event: dict[str, dict[str, int]] = defaultdict(dict)
        hint_by_mint: dict[str, dict[str, int]] = defaultdict(dict)
        per_feed = {f: {"count": 0, "decode_errors": 0, "matched_creates": 0, "actionable_wins": 0, "hint_wins": 0, "ties": 0, "relative_arrival_ns": [], "missing": 0} for f in feeds}
        for o in obs:
            f = o["feed_id"]
            if f not in per_feed:
                continue
            per_feed[f]["count"] += 1
            if o["completeness"] in ("UNDECODABLE", "PARTIAL"):
                per_feed[f]["decode_errors"] += 1
            if o["canonical_event_id"] and o["event_type"] == "CREATE":
                by_event[o["canonical_event_id"]].setdefault(f, o["receive_mono_ns"])
            if o["event_type"] == "HINT_CREATE" and o["mint"]:
                hint_by_mint[o["mint"]].setdefault(f, o["receive_mono_ns"])
        create_by_mint = {cr["mint"]: cr for cr in creates}
        pairwise: dict[str, dict[str, Any]] = {}
        for cr in creates:
            arrivals = dict(by_event.get(cr["canonical_event_id"], {}))
            for f, t in hint_by_mint.get(cr["mint"], {}).items():
                arrivals.setdefault(f, t)
            if not arrivals:
                continue
            earliest = min(arrivals.values())
            winners = [f for f, t in arrivals.items() if t == earliest]
            for f in feeds:
                if f in arrivals:
                    per_feed[f]["relative_arrival_ns"].append(arrivals[f] - earliest)
                    if f in by_event.get(cr["canonical_event_id"], {}):
                        per_feed[f]["matched_creates"] += 1
                else:
                    per_feed[f]["missing"] += 1
            if len(winners) == 1:
                per_feed[winners[0]]["hint_wins"] += 1
            else:
                for w in winners:
                    per_feed[w]["ties"] += 1
            if cr["actionable_mono_ns"]:
                per_feed[cr["winner_feed_id"]]["actionable_wins"] += 1
            fl = sorted(arrivals)
            for i, a in enumerate(fl):
                for b in fl[i + 1 :]:
                    key = f"{a}__vs__{b}"
                    entry = pairwise.setdefault(key, {"common_creates": 0, "a_first": 0, "b_first": 0, "ties": 0, "diff_ns_a_minus_b": []})
                    entry["common_creates"] += 1
                    diff = arrivals[a] - arrivals[b]
                    entry["diff_ns_a_minus_b"].append(diff)
                    if diff < 0:
                        entry["a_first"] += 1
                    elif diff > 0:
                        entry["b_first"] += 1
                    else:
                        entry["ties"] += 1
        out = {}
        for f, s in per_feed.items():
            out[f] = {**{k: v for k, v in s.items() if k != "relative_arrival_ns"}, "relative_arrival_ns": percentiles(s["relative_arrival_ns"]), "class": "MEASURED_RELATIVE"}
        for k, e in pairwise.items():
            e["diff_ns_a_minus_b"] = percentiles(e["diff_ns_a_minus_b"])
        return {"per_feed": out, "pairwise_common_creates": pairwise, "coverage_denominator": "UNION_OF_LOCALLY_OBSERVED_CREATES (no global mainnet completeness claim)", "tie_rule": "same mono ns = tie; operative feed-id tiebreak not counted as win"}

    @staticmethod
    def _qualify(cfg: dict[str, Any], econ: dict[str, Any], qual: dict[str, Any], fees: list[dict[str, Any]], attempts: list[dict[str, Any]], positions: list[dict[str, Any]], boot: dict[str, Any]) -> tuple[QualificationStatus, list[str]]:
        reasons: list[str] = []
        valid_kinds = {f["source_type"] for f in fees if f["validation_status"] == "VALID"}
        stale_or_unknown = sum(v for k, v in econ["cost_evidence_counts"].items() if "UNKNOWN" in k or "STALE" in k)
        if not {"PROTOCOL_GLOBAL_ACCOUNT", "NETWORK_FEE", "RENT_EXEMPTION", "PRIORITY_POLICY"} <= valid_kinds or stale_or_unknown or qual["ineligible_by_reason"].get("UNQUALIFIED_COSTS") or any(a["fees_charged_lamports"] is None and a["status"] not in ("SUBMITTED",) for a in attempts):
            reasons.append("fee rules not fully verified or attempt costs unknown")
            return QualificationStatus.UNQUALIFIED_COSTS, reasons
        if qual["canonical_gap_events"] or qual["positions_with_canonical_gap"] or qual["migration_or_curve_complete_exposures"] or qual["recorder_overflows"] or qual["internal_errors"] or qual["run_quality_status"] not in ("OK",):
            reasons.append("canonical gaps, migration exposure, recorder overflow or internal errors present")
            return QualificationStatus.UNQUALIFIED_DATA, reasons
        if econ["unresolved_exposure_entry_cost_lamports"] > cfg["research"]["max_unresolved_exposure_for_positive_qualification_lamports"]:
            reasons.append("unresolved exposure exceeds configured maximum")
            return QualificationStatus.UNQUALIFIED_DATA, reasons
        if econ["resolved_intents"] == 0 or boot["blocks"] < cfg["research"]["minimum_independent_blocks_for_qualification"]:
            reasons.append(f"resolved intents={econ['resolved_intents']} blocks={boot['blocks']} < minimum {cfg['research']['minimum_independent_blocks_for_qualification']}")
            return QualificationStatus.INSUFFICIENT_DATA, reasons
        if econ["expectancy_per_intent_lamports"] is None or econ["expectancy_per_intent_lamports"] <= 0 or boot["ci_low"] is None or boot["ci_low"] <= 0:
            reasons.append("expectancy per intent or bootstrap lower bound not > 0")
            return QualificationStatus.NEGATIVE, reasons
        if cfg["latency_model"]["entry_delay_ms"] == 0 and cfg["latency_model"]["exit_delay_ms"] == 0 and not cfg["latency_model"]["zero_delay_run_can_qualify_edge"]:
            reasons.append("zero-delay diagnostic run cannot qualify a positive edge; stress scenarios required")
            return QualificationStatus.INSUFFICIENT_DATA, reasons
        reasons.append("single-run expectancy positive under the declared model; POSITIVE_UNDER_MODEL requires the research forward split with stress gates (target_selection R7)")
        return QualificationStatus.INSUFFICIENT_DATA, reasons


def _count(rows: list[dict[str, Any]], key: str) -> dict[str, int]:
    out: dict[str, int] = defaultdict(int)
    for r in rows:
        if r.get(key) is not None:
            out[str(r[key])] += 1
    return dict(out)


def comparable(report: dict[str, Any]) -> dict[str, Any]:
    """Report without generation time (T19 comparison)."""
    return {k: v for k, v in report.items() if k != "created_utc"}


def write_outputs(report: dict[str, Any], conn: sqlite3.Connection, out_dir: Path) -> list[Path]:
    out_dir.mkdir(parents=True, exist_ok=True)
    files: list[Path] = []
    p = out_dir / "report.json"
    p.write_text(json.dumps(report, indent=2, sort_keys=True, default=str), encoding="utf-8")
    files.append(p)
    positions = _rows(conn, "SELECT * FROM positions WHERE run_id=? ORDER BY position_id", (report["run_id"],))
    p = out_dir / "positions.csv"
    with p.open("w", newline="", encoding="utf-8") as fh:
        cols = ["position_id", "mint", "state", "exit_reason", "origin_trigger", "net_pnl_lamports", "entry_economic_cost_lamports", "total_fees_lamports", "locked_rent_lamports", "open_mono_ns", "close_mono_ns", "research_status", "quality_status", "unresolved_reason", "venue_state", "canonical_provider_gap_flag"]
        w = csv.DictWriter(fh, fieldnames=cols)
        w.writeheader()
        for row in positions:
            w.writerow({k: row.get(k) for k in cols})
    files.append(p)
    p = out_dir / "latency_by_feed.csv"
    with p.open("w", newline="", encoding="utf-8") as fh:
        w = csv.writer(fh)
        w.writerow(["feed_id", "count", "matched_creates", "hint_wins", "actionable_wins", "ties", "missing", "decode_errors", "rel_n", "rel_min_ns", "rel_p50_ns", "rel_p95_ns", "rel_p99_ns", "rel_max_ns", "class"])
        for f, s in report["latency"]["feeds"]["per_feed"].items():
            r = s["relative_arrival_ns"]
            w.writerow([f, s["count"], s["matched_creates"], s["hint_wins"], s["actionable_wins"], s["ties"], s["missing"], s["decode_errors"], r["n"], r["min"], r["p50"], r["p95"], r["p99"], r["max"], s["class"]])
    files.append(p)
    p = out_dir / "summary.md"
    p.write_text(summary_markdown(report), encoding="utf-8")
    files.append(p)
    return files


def summary_markdown(r: dict[str, Any]) -> str:
    e, l, q = r["economic"], r["latency"], r["quality"]
    lines = [f"# DarkBot Phase-1 PAPER Bericht – Run {r['run_id']}", "", f"Kampagne: {r['campaign_id']} | Modus: {r['mode']} | Config-Hash: {r['config_hash'][:16]} | Query: {r['query_version']}", "",
             f"**Qualifikationsstatus: {r['qualification_status']}** – {'; '.join(r['qualification_reasons'])}", "",
             "## Umfang", f"- Beobachtete CREATEs (lokale Union): {r['creates_observed_union']}, davon actionable: {r['creates_actionable']}", f"- Entry-Intents: {e['entry_intents']}, Fills: {e['entry_fills']}, Entry-Fehler: {e['entry_failed']}",
             f"- Exits: Creator-Sell {e['exits']['CREATOR_SELL']}, Target {e['exits']['TARGET']}, Timeout {e['exits']['TIMEOUT']}; Exit-Fehlversuche {e['exit_failed_attempts']}, Retry-Erschoepfungen {e['retry_exhausted']}",
             f"- Offen am Ende: {e['open_at_end']}, UNRESOLVED: {e['unresolved']} {e['unresolved_by_reason']}, zensiert: {e['censored']}", f"- Ineligible nach Grund: {q['ineligible_by_reason']}", "",
             "## Wirtschaft (Modell, OBSERVED_STATE_PROXY)", f"- Realisiertes Netto-PnL: {e['realized_net_pnl_lamports']} Lamports ({e['realized_net_pnl_sol']} SOL)",
             f"- Unrealized: {e['unrealized_status']}; ungeklaerte Exposure (Entry-Kosten): {e['unresolved_exposure_entry_cost_lamports']} Lamports; konservative Untergrenze (Token-Restwert 0): {e['conservative_lower_bound_pnl_lamports_token_residual_zero']}",
             f"- Erwartungswert je Intent: {e['expectancy_per_intent_lamports']}, je Fill: {e['expectancy_per_fill_lamports']}, Median-PnL: {e['median_pnl_lamports']}, Gewinnrate: {e['win_rate']}",
             f"- Profit Factor: {e['profit_factor']} {e['profit_factor_special_case'] or ''}; Max Drawdown: {e['max_drawdown_lamports_on_close_time_equity']}; Kapitalauslastung: {e['capital_utilization_fraction']}",
             f"- Gebuehren nach Art: {e['fees_by_kind_lamports']}; Kosten-Evidenz: {e['cost_evidence_counts']}; Fill-Evidenz: {e['fill_evidence_counts']}",
             f"- Cash-Reconciliation: {e['cash_reconciliation']}", f"- Block-Bootstrap (Intent-PnL): {e['block_bootstrap_per_intent_pnl']}", "",
             "## Latenz (lokal gemessen, gleiche Clock-Epoch)", f"- receive→decode: {l['receive_to_decode_ns']}", f"- receive→decision (Entries): {l['receive_to_decision_ns']}", f"- decision→enqueue: {l['decision_to_enqueue_ns']}",
             f"- Callback-Lateness: {l['callback_lateness_ns']}", f"- Modellverzoegerung Entry/Exit (SIMULATED): {l['modeled_entry_delay_ns']['value']} / {l['modeled_exit_delay_ns']['value']} ns", "",
             "## Feeds (relativ, Union-Nenner, keine globale Vollstaendigkeit)"]
    for f, s in l["feeds"]["per_feed"].items():
        lines.append(f"- {f}: n={s['count']}, matched={s['matched_creates']}, hint-wins={s['hint_wins']}, actionable-wins={s['actionable_wins']}, ties={s['ties']}, missing={s['missing']}, rel p50/p95={s['relative_arrival_ns']['p50']}/{s['relative_arrival_ns']['p95']} ns")
    lines += ["", "## Datenqualitaet", f"- Quality-Events: {q['quality_events']}, kanonische Gaps: {q['canonical_gap_events']}, Provider-Disconnects: {q['provider_disconnects']}, Recorder-Overflows: {q['recorder_overflows']}, interne Fehler: {q['internal_errors']}",
              f"- Positionen mit kanonischem Gap: {q['positions_with_canonical_gap']}; Migrations-/Curve-complete-Exposures: {q['migration_or_curve_complete_exposures']}", f"- Quote-Scope der CREATEs: {q['create_quote_scope_counts']}",
              f"- Fee-Snapshots: {q['fee_snapshot_status_counts']}", "", "## Annahmen", f"- {r['model_assumptions']['counterfactual_note']}", f"- Latenzmodell: {r['model_assumptions']['latency_model']['label']}; Slippage-Grenze {r['model_assumptions']['slippage_bps']} bps ist eine Ausfuehrungsgrenze, keine gezahlte Gebuehr."]
    return "\n".join(lines) + "\n"


def persist_report(conn: sqlite3.Connection, report: dict[str, Any], report_id: str) -> None:
    conn.execute("INSERT OR REPLACE INTO reports(report_id, run_id, campaign_id, created_utc, query_version, dataset_hash, config_hash, report_json, qualification_status) VALUES (?,?,?,?,?,?,?,?,?)",
                 (report_id, report["run_id"], report["campaign_id"], report["created_utc"], report["query_version"], report["dataset_hash"] or "", report["config_hash"], canonical_json(json.loads(json.dumps(report, default=str))), report["qualification_status"]))

#!/usr/bin/env python3
"""Phase-A one-off strict replay runner. Read-only source, NEW output DB, production load_replay_input/run_replay,
no monkeypatching of decoder/reducer/evidence/fees. Writes a manifest with exact lamport economics and
run-id-independent semantic digests for before/after comparison.

Usage: python3 strict_replay.py <repo_root> <source_sqlite> <source_run_id> <config_json> <slots> <output_sqlite> <manifest_json> <label>
"""
from __future__ import annotations

import hashlib
import json
import platform
import re
import sqlite3
import sys
import time
from pathlib import Path

ROOT = Path(sys.argv[1]).resolve()
SRC = Path(sys.argv[2]).resolve()
SRC_RUN = sys.argv[3]
CFG_PATH = Path(sys.argv[4]).resolve()
SLOTS = int(sys.argv[5])
OUT_DB = Path(sys.argv[6]).resolve()
MANIFEST = Path(sys.argv[7]).resolve()
LABEL = sys.argv[8]
sys.path.insert(0, str(ROOT))

from darkbot.config import canonical_json, load_runtime_config, sha256_text  # noqa: E402
from darkbot.replay import digest, load_replay_input, run_replay  # noqa: E402
from darkbot.reporting import Reporter  # noqa: E402
from darkbot.storage import apply_migrations, open_connection  # noqa: E402


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def strip_run(value, run_id: str):
    """Documented run-specific mapping: remove the generated replay run id prefix from ids; nothing else is ignored."""
    if isinstance(value, str):
        return value.replace(run_id + "_", "").replace(run_id + "|", "").replace(run_id, "<RUN>")
    return value


def rows(conn: sqlite3.Connection, sql: str, params: tuple) -> list[dict]:
    conn.row_factory = sqlite3.Row
    return [dict(r) for r in conn.execute(sql, params)]


def main() -> None:
    if OUT_DB.exists():
        raise SystemExit(f"output db must be new: {OUT_DB}")
    t0 = time.monotonic()
    cfg_base = load_runtime_config(CFG_PATH)
    cfg = cfg_base.with_overrides(portfolio=cfg_base.portfolio_limit_overrides(SLOTS))
    source = open_connection(SRC, read_only=True)
    try:
        frozen = load_replay_input(source, SRC_RUN, cfg)
        source_quality = source.execute("SELECT quality_status FROM runs WHERE run_id=?", (SRC_RUN,)).fetchone()[0]
    finally:
        source.close()
    out = open_connection(OUT_DB)
    apply_migrations(out)
    new_run_id = f"phase_a_{LABEL}_replay_{frozen.dataset_hash[:12]}"
    result = run_replay(out, frozen, cfg, new_run_id, "phase_a_parity", persist=True, persist_observations=True)
    owner = result.owner
    if owner.disk_error or owner.recorder.overflowed or owner.counters.internal_errors:
        raise SystemExit("replay storage or engine failure")
    report = Reporter(out).build(new_run_id)
    R = new_run_id

    events = {}
    for r in rows(out, "SELECT * FROM market_events WHERE run_id=? ORDER BY canonical_event_id", (R,)):
        key = f"{r['signature']}|{r['instruction_index']}|{r['mint']}|{r['event_type']}"
        events[key] = {k: strip_run(v, R) for k, v in r.items() if k not in ("canonical_event_id", "run_id", "source_observation_id")}
    decisions = []
    for r in rows(out, "SELECT * FROM decisions WHERE run_id=? ORDER BY receive_mono_ns, arrival_sequence, decision_id", (R,)):
        decisions.append({k: strip_run(v, R) for k, v in r.items() if k not in ("run_id",)})
    attempts = []
    for r in rows(out, "SELECT * FROM execution_attempts WHERE run_id=? ORDER BY submit_mono_ns, attempt_id", (R,)):
        attempts.append({k: strip_run(v, R) for k, v in r.items() if k not in ("run_id",)})
    fills = [a for a in attempts if a["status"] == "FILLED"]
    fee_evidence = {}
    for r in rows(out, "SELECT fee_snapshot_id, source_type, source_provider, source_method, received_mono_ns, normalized_fee_components, validation_status, evidence, snapshot_hash FROM fee_snapshots WHERE run_id=? ORDER BY received_mono_ns, fee_snapshot_id", (R,)):
        fee_evidence[strip_run(r["fee_snapshot_id"], R)] = {k: strip_run(v, R) for k, v in r.items() if k != "fee_snapshot_id"}
    selected_fee_evidence = sorted({strip_run(a["fee_snapshot_id"], R) for a in attempts if a.get("fee_snapshot_id")})
    ledger = []
    for r in rows(out, "SELECT * FROM ledger WHERE run_id=? ORDER BY mono_ns, ledger_entry_id", (R,)):
        ledger.append({k: strip_run(v, R) for k, v in r.items() if k not in ("run_id",)})
    positions = []
    for r in rows(out, "SELECT * FROM positions WHERE run_id=? ORDER BY position_id", (R,)):
        positions.append({k: strip_run(v, R) for k, v in r.items() if k not in ("run_id",)})
    quotes = []
    for r in rows(out, "SELECT * FROM quote_snapshots WHERE run_id=? ORDER BY quote_snapshot_id", (R,)):
        quotes.append({k: strip_run(v, R) for k, v in r.items() if k not in ("run_id",)})
    quality = []
    for r in rows(out, "SELECT kind, severity, scope, feed_id, start_mono_ns, end_mono_ns, canonical_gap, recovery_status FROM quality_events WHERE run_id=? ORDER BY quality_event_id", (R,)):
        quality.append({k: strip_run(v, R) for k, v in r.items()})
    econ = report["economic"]
    ram = digest(owner)
    ram["attempts"] = [{**a, "quote_state": strip_run(a["quote_state"], R)} for a in ram["attempts"]]
    out.close()

    semantic = {"events": events, "decisions": decisions, "attempts": attempts, "fills": fills, "selected_fee_evidence": selected_fee_evidence, "fee_evidence": fee_evidence,
                "ledger": ledger, "positions": positions, "quote_snapshots": quotes, "quality_events": quality, "ram_digest": ram}
    exact = {
        "realized_net_pnl_lamports": econ["realized_net_pnl_lamports"],
        "pessimistic_pnl_lamports_token_residual_zero": econ["conservative_lower_bound_pnl_lamports_token_residual_zero"],
        "unresolved_exposure_entry_cost_lamports": econ["unresolved_exposure_entry_cost_lamports"],
        "locked_rent_lamports_end": econ["locked_rent_lamports_end"],
        "fees_by_kind_lamports": econ["fees_by_kind_lamports"],
        "cash_reconciliation": econ["cash_reconciliation"],
        "ram_cash_lamports": ram["cash"], "ram_reserved_lamports": ram["reserved"], "ram_locked_rent_lamports": ram["locked_rent"], "ram_ledger_cash_sum": ram["ledger_cash_sum"],
        "sum_position_net_pnl_lamports": sum(p["net_pnl_lamports"] or 0 for p in positions),
        "counts": {"entry_intents": econ["entry_intents"], "entry_fills": econ["entry_fills"], "entry_failed": econ["entry_failed"], "exits": econ["exits"], "unresolved": econ["unresolved"], "censored": econ["censored"],
                   "positions_by_state": econ["positions_by_state"], "unresolved_by_reason": econ["unresolved_by_reason"], "events": len(events), "decisions": len(decisions), "attempts": len(attempts), "fills": len(fills), "ledger": len(ledger)},
        "qualification_status": report["qualification_status"],
    }
    manifest = {
        "label": LABEL, "baseline_commit": "05971e4ed35a8a98349e298d2de4e61f8584fc5b", "python": platform.python_version(), "platform": platform.platform(),
        "source_db": str(SRC), "source_db_sha256": sha256_file(SRC), "source_run_id": SRC_RUN, "source_quality": source_quality, "dataset_hash": frozen.dataset_hash,
        "config_path": str(CFG_PATH), "config_hash_file": cfg_base.config_hash, "effective_config_hash": cfg.config_hash, "portfolio_limit_override": SLOTS, "effective_config_canonical_sha256": sha256_text(cfg.canonical),
        "config_content": cfg.raw, "strategy_config_hash": cfg.strategy_config_hash, "policy_hash": cfg.policy_hash, "cost_model_hash": cfg.cost_model_hash, "freshness_policy_hash": cfg.freshness_policy_hash,
        "replay_parameters": {"mode": "REPLAY", "persist": True, "persist_observations": True, "compute_features": False, "end_mono_ns": None, "drain_ns": None, "stress_fee_multiplier": "1", "stress_adverse_bps": 0, "mint_filter": None, "stress_reject_rate": "0", "stress_seed": 0, "replay_callback_lateness": True, "execution_delay_provider": None},
        "runner_path": str(Path(__file__).resolve()), "runner_sha256": sha256_file(Path(__file__).resolve()), "command": " ".join(sys.argv), "output_db": str(OUT_DB), "replay_run_id": new_run_id,
        "exact_lamports": exact, "semantic_sha256": sha256_text(canonical_json(semantic)), "semantic": semantic, "replay_wall_s": time.monotonic() - t0,
    }
    MANIFEST.write_text(json.dumps(manifest, indent=1, sort_keys=True, default=str) + "\n")
    print(json.dumps({"label": LABEL, "dataset_hash": frozen.dataset_hash, "replay_run_id": new_run_id, "exact_lamports": exact, "semantic_sha256": manifest["semantic_sha256"], "wall_s": manifest["replay_wall_s"]}, indent=1, default=str))


if __name__ == "__main__":
    main()

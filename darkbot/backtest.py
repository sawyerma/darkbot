"""Research orchestration (spec: backtest.py): dataset freeze, pre-registered variants, chronological
splits with embargo, replay per portfolio/scenario through the same engine, statistical evaluation.
No trading decisions are implemented here.
"""

from __future__ import annotations

import csv
import json
import sqlite3
from dataclasses import dataclass
from decimal import Decimal
from pathlib import Path
from datetime import datetime, timezone
import time
from typing import Any

from darkbot.config import RuntimeConfig, canonical_json, config_hash, load_research_config
from darkbot.domain import NS_PER_MS, utc_iso
from darkbot.replay import ReplayInput, load_replay_input, run_replay
from darkbot.reporting import block_bootstrap
from darkbot.storage import upsert_config_snapshot


@dataclass(frozen=True)
class Variant:
    variant_id: str
    target: str
    entry_delay_ms: int
    exit_delay_ms: int
    fee_multiplier: str
    adverse_bps: int
    portfolio_limit: int
    paired: bool
    reject_rate: str = "0"


def embargo_ns(cfg: RuntimeConfig, research: dict[str, Any]) -> int:
    max_delay = max(max(research["entry_delay_ms"]), max(research["exit_delay_ms"]))
    return (cfg.strategy["strategy_timeout_ms"] + 2 * max_delay + sum(cfg.execution["exit_retry_backoff_ms"])) * NS_PER_MS


def chronological_split(create_times: list[tuple[str, int]], fractions: list[Decimal], embargo: int, block_length_ns: int | None = None) -> dict[str, set[str]]:
    """Chronological split on whole time blocks (session/day blocks of block_length): blocks are assigned in time order until the cumulative
    launch fraction is reached, so dependent launches of one block never straddle two splits. Launches whose horizon (embargo) crosses a
    split boundary are purged from the earlier split (their outcome would leak into the next window)."""
    ordered = sorted(create_times, key=lambda x: (x[1], x[0]))
    n = len(ordered)
    splits: dict[str, set[str]] = {"train": set(), "validation": set(), "test": set()}
    if n == 0:
        return splits
    block_length = block_length_ns or 1
    blocks: dict[int, list[tuple[str, int]]] = {}
    for mint, t in ordered:
        blocks.setdefault(t // block_length, []).append((mint, t))
    targets = [fractions[0], fractions[0] + fractions[1]]
    names = ["train", "validation", "test"]
    seen = 0
    current = 0
    boundaries: list[int] = []
    for block_id in sorted(blocks):
        if current < 2 and Decimal(seen) / Decimal(n) >= targets[current]:
            current += 1
            boundaries.append(block_id * block_length)
        for mint, t in blocks[block_id]:
            splits[names[current]].add(mint)
        seen += len(blocks[block_id])
    for boundary in boundaries:
        for name in names:
            for mint, t in list(ordered):
                if mint in splits[name] and boundary - embargo <= t < boundary:
                    splits[name].discard(mint)
    return splits


def delay_scenarios(research: dict[str, Any]) -> list[tuple[int, int]]:
    """Pre-registered latency scenarios: joint pairs plus independent one-sided entry/exit sweeps (research.independent_delay_scenarios)."""
    scenarios = [tuple(x) for x in research["joint_delay_scenarios"]]
    ind = research.get("independent_delay_scenarios") or {}
    if ind:
        scenarios += [(e, 0) for e in ind.get("entry_delay_ms", [])] + [(0, x) for x in ind.get("exit_delay_ms", [])]
    out: list[tuple[int, int]] = []
    for sc in scenarios:
        if sc not in out:
            out.append(sc)
    return out


def register_variants(research: dict[str, Any], cfg: RuntimeConfig) -> list[Variant]:
    variants: list[Variant] = []
    rates = ["0", str(research["conservative_exit_reject_rate"])]
    for target in research["target_grid"]:
        for ed, xd in delay_scenarios(research):
            for fm in research["network_fee_multipliers"]:
                for ab in research["additional_adverse_price_bps"]:
                    for rr in rates:
                        base = f"t{target}_e{ed}_x{xd}_f{fm}_a{ab}_r{rr}"
                        variants.append(Variant(f"{base}_paired", target, ed, xd, fm, ab, 0, True, rr))
                        for limit in research["portfolio_position_limits"]:
                            variants.append(Variant(f"{base}_L{limit}", target, ed, xd, fm, ab, limit, False, rr))
    return variants


def variant_config(cfg: RuntimeConfig, v: Variant, launches: int, seed: int = 0) -> RuntimeConfig:
    portfolio = {"max_open_positions": max(1, launches) if v.paired else v.portfolio_limit}
    if v.paired:
        portfolio["initial_paper_balance_lamports"] = cfg.portfolio["initial_paper_balance_lamports"] * max(1, launches)
    return cfg.with_overrides(strategy={"target_net_multiple": v.target}, latency_model={"entry_delay_ms": v.entry_delay_ms, "exit_delay_ms": v.exit_delay_ms}, portfolio=portfolio, execution={"research_stress": {"network_fee_multiplier": v.fee_multiplier, "additional_adverse_price_bps": v.adverse_bps, "exit_reject_rate": v.reject_rate, "random_seed": seed}})


def evaluate_variant(conn: sqlite3.Connection, replay_input: ReplayInput, cfg: RuntimeConfig, v: Variant, mints: set[str], campaign: str, research: dict[str, Any], split: str, persist: bool, stage: str = "freeze", cutoff_mono_ns: int | None = None) -> dict[str, Any]:
    vcfg = variant_config(cfg, v, len(mints), research["random_seed"])
    variant_snapshot_hash = config_hash({"runtime_config_hash": vcfg.config_hash,
                                         "stress_fee_multiplier": v.fee_multiplier,
                                         "stress_adverse_bps": v.adverse_bps,
                                         "stress_reject_rate": v.reject_rate, "random_seed": research["random_seed"],
                                         "stage": stage})
    scope = f"{replay_input.source_run_id}_{replay_input.dataset_hash[:12]}_{stage}"  # recording + frozen dataset + stage (freeze|forward): never collides
    run_id = f"research_{campaign}_{scope}_{split}_{v.variant_id}"
    variant_key = f"{scope}:{split}:{v.variant_id}"
    if persist:
        upsert_config_snapshot(conn, "POLICY", vcfg.policy_hash, vcfg.policy_content(), "research_variant", utc_iso(0))
        upsert_config_snapshot(conn, "RUNTIME_CONFIG", vcfg.config_hash, vcfg.raw, "research_variant", utc_iso(0))
        upsert_config_snapshot(conn, "RESEARCH_STRESS", variant_snapshot_hash,
                               {"runtime_config_hash": vcfg.config_hash, "fee_multiplier": v.fee_multiplier,
                                "adverse_bps": v.adverse_bps, "reject_rate": v.reject_rate, "random_seed": research["random_seed"], "stage": stage},
                               "research_variant", utc_iso(0))
        conn.execute("INSERT OR IGNORE INTO research_variants(variant_id,campaign_id,parent_config_hash,variant_config_hash,dataset_id,split,registered_utc,seed,selection_status) VALUES (?,?,?,?,?,?,?,?,?)",
                     (variant_key, campaign, cfg.config_hash, variant_snapshot_hash, replay_input.dataset_hash, split, utc_iso(0), research["random_seed"], "PRE_REGISTERED"))
        if conn.execute("SELECT 1 FROM runs WHERE run_id=?", (run_id,)).fetchone():
            raise ValueError(f"research run {run_id} already exists: re-running the same variant on the same recording is not allowed (delete or use a new campaign)")
    result = run_replay(conn, replay_input, vcfg, run_id, campaign, mode="RESEARCH_REPLAY", variant_id=variant_key if persist else None, persist=persist, drain_ns=embargo_ns(cfg, research),
                        stress_fee_multiplier=Decimal(v.fee_multiplier), stress_adverse_bps=v.adverse_bps, mint_filter=mints, stress_reject_rate=Decimal(v.reject_rate), stress_seed=research["random_seed"], end_mono_ns=cutoff_mono_ns)
    owner = result.owner
    resolved = [p for p in owner.positions.values() if p.net_pnl_lamports is not None]
    block_len = research["block_length_ms"] * NS_PER_MS
    samples = [((replay_input.start_utc_ns + owner.first_create_receive[p.mint] - replay_input.start_mono_ns) // block_len, p.net_pnl_lamports) for p in resolved]
    boot = block_bootstrap(samples, research["bootstrap_resamples"], research["random_seed"], Decimal(research["bootstrap_confidence"]))
    exposed = [p for p in owner.positions.values() if p.net_pnl_lamports is None]
    return {"variant_id": v.variant_id, "variant_config_hash": variant_snapshot_hash, "split": split, "target": v.target, "entry_delay_ms": v.entry_delay_ms, "exit_delay_ms": v.exit_delay_ms, "fee_multiplier": v.fee_multiplier, "adverse_bps": v.adverse_bps, "reject_rate": v.reject_rate,
            "portfolio_limit": "paired" if v.paired else v.portfolio_limit, "launches": len(mints), "intents": len(owner.positions), "resolved": len(resolved), "unresolved": len(exposed),
            "net_pnl_lamports": sum(p.net_pnl_lamports for p in resolved), "expectancy_per_intent": (sum(p.net_pnl_lamports for p in resolved) / len(resolved)) if resolved else None,
            "bootstrap": boot, "exits": {"CREATOR_SELL": sum(1 for p in resolved if p.exit_reason and p.exit_reason.value == "CREATOR_SELL"), "TARGET": sum(1 for p in resolved if p.exit_reason and p.exit_reason.value == "TARGET"), "TIMEOUT": sum(1 for p in resolved if p.exit_reason and p.exit_reason.value == "TIMEOUT")},
            "entry_failed": owner.counters.entry_failed, "exit_exhausted": owner.counters.exit_exhausted, "censored": owner.counters.censored, "unresolved_reasons": {p.unresolved_reason: 1 for p in exposed if p.unresolved_reason},
            "status": variant_status(v, split, boot, exposed, research, cfg),
            "run_id": run_id}


def variant_status(v: Variant, split: str, boot: dict[str, Any], exposed: list[Any], research: dict[str, Any], cfg: RuntimeConfig) -> str:
    """qualification_rule: POSITIVE_UNDER_MODEL is only possible on untouched forward/test data, with non-zero declared delays, without unresolved exposure,
    with enough independent blocks and a lower bootstrap bound > 0. Everything in-sample is at most a CANDIDATE."""
    if boot["status"] != "OK" or boot["blocks"] < research["minimum_independent_blocks_for_qualification"]:
        return "INSUFFICIENT_DATA"
    max_exposure = cfg.research["max_unresolved_exposure_for_positive_qualification_lamports"]
    if exposed and (any(p.entry_economic_cost_lamports is None or p.state.value in ("UNRESOLVED", "ENTRY_PENDING", "EXIT_PENDING") and p.entry_fill_attempt_id is None for p in exposed)
                    or sum((p.entry_economic_cost_lamports or 0) for p in exposed) > max_exposure or (max_exposure == 0 and exposed)):
        return "UNQUALIFIED_DATA_UNRESOLVED_EXPOSURE"  # unknown cost of an exposed position is never zero
    if boot["ci_low"] is None or boot["ci_low"] <= 0:
        return "NEGATIVE"
    if v.entry_delay_ms == 0 and v.exit_delay_ms == 0 and not cfg.latency_model["zero_delay_run_can_qualify_edge"]:
        return "DIAGNOSTIC_ZERO_DELAY_CANNOT_QUALIFY"
    if split in ("train", "validation"):
        return "CANDIDATE_IN_SAMPLE_NOT_QUALIFYING"
    return "HOLDOUT_POSITIVE_PENDING_FORWARD" if split == "test" else "POSITIVE_UNDER_MODEL_PENDING_STRESS"


def apply_stress_gate(outputs: list[dict[str, Any]], candidate_target: str | None = None, required_stress: int | None = None) -> None:
    """A forward/test candidate qualifies only if (a) a candidate target was FROZEN before the forward evaluation and matches, and (b) ALL declared stress
    siblings (fee multipliers, adverse bps, conservative reject rate; same target/delays/limit) exist, have enough data and are positive."""
    source = [dict(o) for o in outputs]  # gate decisions never depend on earlier output mutation
    for o in outputs:
        if o.get("status") != "POSITIVE_UNDER_MODEL_PENDING_STRESS":
            continue
        if candidate_target is None:
            o["status"] = "NO_FROZEN_CANDIDATE"
            continue
        if o.get("target") != candidate_target:
            o["status"] = "NOT_PREREGISTERED_CANDIDATE"
            continue
        siblings = [x for x in source if x.get("split") == o["split"] and x.get("target") == o["target"] and x.get("entry_delay_ms") == o["entry_delay_ms"] and x.get("exit_delay_ms") == o["exit_delay_ms"]
                    and x.get("portfolio_limit") == o["portfolio_limit"] and x.get("source_run_id") == o.get("source_run_id") ]
        stressed: dict[tuple, dict[str, Any]] = {}
        for x in siblings:  # one entry per UNIQUE declared stress scenario of the same recording; duplicates never count twice
            key = (str(x.get("fee_multiplier")), int(x.get("adverse_bps", 0)), str(x.get("reject_rate", "0")))
            if key != ("1.0", 0, "0") and key not in stressed:
                stressed[key] = x
        if not stressed or (required_stress is not None and len(stressed) < required_stress):
            o["status"] = "INSUFFICIENT_DATA_NO_STRESS_VARIANTS"
        elif any(not str(x.get("status", "")).startswith(("NEGATIVE", "POSITIVE", "CANDIDATE")) for x in stressed.values()):
            o["status"] = "INSUFFICIENT_DATA_IN_STRESS_VARIANTS"  # INSUFFICIENT_*, UNQUALIFIED_*, DIAGNOSTIC_* siblings block
        elif all((x.get("bootstrap", {}).get("ci_low") or 0) > 0 and not x.get("unresolved") for x in stressed.values()):
            o["status"] = "POSITIVE_UNDER_MODEL"
        else:
            o["status"] = "NEGATIVE_UNDER_STRESS"


def select_candidate(rows: list[dict[str, Any]], baseline: str, preference_fraction: Decimal) -> dict[str, Any]:
    """Validation split, selection limit, baseline scenario: pick the target with the best lower CI bound; prefer the simpler 1.5 baseline when within 10%."""
    cands = [r for r in rows if r["split"] == "validation" and r.get("status") == "CANDIDATE_IN_SAMPLE_NOT_QUALIFYING"
             and r["fee_multiplier"] == "1.0" and r["adverse_bps"] == 0 and str(r.get("reject_rate", "0")) == "0"
             and r.get("bootstrap", {}).get("ci_low") is not None]
    if not cands:
        return {"selected_target": baseline, "reason": "INSUFFICIENT_DATA: no validation variant with a bootstrap interval; baseline kept", "candidates": []}
    best = max(cands, key=lambda r: r["bootstrap"]["ci_low"])
    base = next((r for r in cands if r["target"] == baseline and r["entry_delay_ms"] == best["entry_delay_ms"] and r["exit_delay_ms"] == best["exit_delay_ms"]), None)
    if base is not None and best["bootstrap"]["ci_low"] > 0 and base["bootstrap"]["ci_low"] >= preference_fraction * Decimal(str(best["bootstrap"]["ci_low"])):
        return {"selected_target": baseline, "reason": "baseline within 10% of best lower bound; simpler stable choice preferred", "best": best["variant_id"], "positive": base["bootstrap"]["ci_low"] > 0}
    return {"selected_target": best["target"], "reason": "best validation lower bound", "best": best["variant_id"], "positive": best["bootstrap"]["ci_low"] > 0}


def run_research(conn: sqlite3.Connection, campaign: str, cfg: RuntimeConfig, research_path: Path, out_dir: Path, source_run_ids: list[str] | None = None, persist: bool = True, max_variants: int | None = None) -> dict[str, Any]:
    research = load_research_config(research_path)
    conn.row_factory = sqlite3.Row
    runs = source_run_ids or [r[0] for r in conn.execute("SELECT run_id FROM runs WHERE campaign_id=? AND mode='PAPER' ORDER BY started_utc", (campaign,))]
    if not runs:
        raise ValueError("no PAPER runs for campaign")
    outputs: list[dict[str, Any]] = []
    dataset_hashes: list[str] = []
    selection_limit = research["selection_portfolio_position_limit"]
    variants = register_variants(research, cfg)
    if max_variants:
        variants = variants[:max_variants]
    inputs = {run_id: load_replay_input(conn, run_id, cfg) for run_id in runs}
    dataset_hashes = [inputs[r].dataset_hash for r in runs]
    dataset_hash = config_hash({"runs": runs, "hashes": dataset_hashes, "freeze": research["dataset_freeze"]})
    # Only explicit operational switches may differ after preregistration.
    research_policy = {k: v for k, v in research.items() if k not in ("test_split_unlocked", "forward_test", "policy_freeze_note")}
    freeze_key = config_hash({"campaign": campaign, "kind": "POLICY_FREEZE"})
    frozen_row = conn.execute("SELECT canonical_json FROM config_snapshots WHERE config_hash=?", (freeze_key,)).fetchone()
    frozen_content = json.loads(frozen_row[0]) if frozen_row else None
    frozen_target = frozen_content.get("selected_target") if frozen_content else None
    stage = "forward" if research["forward_test"] else "holdout" if research["test_split_unlocked"] else "freeze"
    if stage != "freeze":
        if not frozen_target:
            raise ValueError("POLICY_FREEZE_REQUIRED: freeze a validation candidate before holdout/forward evaluation")
        if frozen_content.get("runtime_config_hash") != cfg.config_hash or frozen_content.get("research_policy_hash") != config_hash(research_policy):
            raise ValueError("POLICY_FREEZE_MISMATCH: missing or changed runtime/research configuration")
        if stage == "holdout" and frozen_content.get("dataset") != dataset_hash:
            raise ValueError("POLICY_FREEZE_DATASET_MISMATCH: holdout must use exactly the frozen dataset")
        if stage == "forward":
            if not conn.execute("SELECT 1 FROM config_snapshots WHERE config_hash=?", (config_hash({"freeze": freeze_key, "stage": "holdout"}),)).fetchone():
                raise ValueError("FORWARD_REQUIRES_COMPLETED_HOLDOUT")
            frozen_at = frozen_content.get("frozen_at_utc")
            if not frozen_at or any(conn.execute("SELECT started_utc FROM runs WHERE run_id=?", (r,)).fetchone()[0] <= frozen_at for r in runs):
                raise ValueError("FORWARD_REQUIRES_NEW_POST_FREEZE_RECORDINGS")
            if set(runs) & set(frozen_content.get("source_runs", [])):
                raise ValueError("FORWARD_REUSES_DEVELOPMENT_RECORDING")
        evaluation_key = config_hash({"freeze": freeze_key, "stage": stage, **({"dataset": dataset_hash} if stage == "forward" else {})})
        if conn.execute("SELECT 1 FROM config_snapshots WHERE config_hash=?", (evaluation_key,)).fetchone():
            raise ValueError("EVALUATION_ALREADY_CONSUMED: holdout/forward dataset may be evaluated once")
        variants = [v for v in variants if v.target == frozen_target]
    # Campaign chronology uses UTC anchors; monotonic clock epochs from different runs are incomparable.
    creates_by_run = {}
    utc_offsets = {}
    global_creates = []
    for rid in runs:
        run = conn.execute("SELECT started_utc, started_mono_ns FROM runs WHERE run_id=?", (rid,)).fetchone()
        dt = datetime.fromisoformat(run["started_utc"].replace("Z", "+00:00")) - datetime(1970, 1, 1, tzinfo=timezone.utc)
        anchor = (dt.days * 86400 + dt.seconds) * 1_000_000_000 + dt.microseconds * 1000
        epoch = run["started_mono_ns"] if run["started_mono_ns"] is not None else inputs[rid].start_mono_ns
        utc_offsets[rid] = anchor - epoch
        creates_by_run[rid] = [(r["mint"], r["first_seen"]) for r in conn.execute("SELECT mint, MIN(actionable_mono_ns) AS first_seen FROM market_events WHERE run_id=? AND event_type='CREATE' AND actionable_mono_ns IS NOT NULL GROUP BY mint", (rid,))]
        global_creates.extend((rid + "|" + mint, anchor + mono - epoch) for mint, mono in creates_by_run[rid])
    global_splits = chronological_split(global_creates, [Decimal(x) for x in research["chronological_split"]], embargo_ns(cfg, research), research["block_length_ms"] * NS_PER_MS)
    for run_id in runs:
        rin = inputs[run_id]
        splits = {name: {mint for mint, _ in creates_by_run[run_id] if run_id + "|" + mint in ids} for name, ids in global_splits.items()}
        if stage == "forward":
            splits = {"forward": {mint for mint, _ in creates_by_run[run_id]}}
        for split_name in (("forward",) if stage == "forward" else ("test",) if stage == "holdout" else ("train", "validation", "test")):
            mints = splits[split_name]
            if split_name == "test" and not research["test_split_unlocked"]:
                outputs.append({"variant_id": "ALL", "split": "test", "status": "LOCKED_UNTIL_POLICY_FREEZE", "launches": len(mints), "run_id": run_id})
                continue
            if not mints:
                continue
            # No earlier split may consume observations from the next split.
            later = {"train": ("validation", "test"), "validation": ("test",)}.get(split_name, ())
            future = [t for key, t in global_creates if any(key in global_splits[n] for n in later)]
            cutoff = min(rin.end_mono_ns, min(future) - utc_offsets[run_id] - 1) if future else rin.end_mono_ns
            for v in variants:
                outputs.append({**evaluate_variant(conn, rin, cfg, v, mints, campaign, research, split_name, persist, stage, cutoff), "source_run_id": run_id})
    selection_limit_rows = [o for o in outputs if o.get("portfolio_limit") == selection_limit]
    pre_selection = select_candidate(selection_limit_rows, research["baseline_target"], Decimal(research["baseline_preference_fraction"]))
    if max_variants is not None:
        pre_selection["positive"] = False
        pre_selection["reason"] = "DIAGNOSTIC_VARIANT_SUBSET: incomplete preregistered evaluation cannot freeze a candidate"
    if stage == "freeze" and persist and pre_selection.get("positive") and frozen_row is None:
        # freeze the validation candidate BEFORE any forward evaluation; the record is immutable (INSERT OR IGNORE)
        upsert_config_snapshot(conn, "POLICY_FREEZE", freeze_key, {"campaign": campaign, "dataset": dataset_hash, "selected_target": pre_selection["selected_target"], "runtime_config_hash": cfg.config_hash, "research_config": research, "research_policy_hash": config_hash(research_policy), "source_runs": runs, "frozen_at_utc": utc_iso(time.time_ns()), "stage": "VALIDATION", "note": "frozen before test split unlock"}, "research_freeze", utc_iso(0))
        frozen_target = pre_selection["selected_target"]
    # every declared stress scenario (fee multipliers x adverse bps x reject rates) except the baseline must be present and positive
    required_stress = len(research["network_fee_multipliers"]) * len(research["additional_adverse_price_bps"]) * 2 - 1
    apply_stress_gate(outputs, frozen_target if stage == "forward" else None, required_stress)
    if stage != "freeze" and persist:
        upsert_config_snapshot(conn, "EVALUATION_COMPLETED", evaluation_key, {"dataset": dataset_hash, "stage": stage}, "research", utc_iso(time.time_ns()))
    if persist:
        upsert_config_snapshot(conn, "DATASET", dataset_hash, {"runs": runs, "hashes": dataset_hashes, "freeze": research["dataset_freeze"]}, "research_freeze", utc_iso(0))
    selection = {**pre_selection, "frozen_target": frozen_target, "stage": stage, "would_freeze": bool(pre_selection.get("positive"))}
    limit10 = [o for o in outputs if o.get("portfolio_limit") == 10]
    report = {"campaign": campaign, "dataset_hash": dataset_hash, "source_runs": runs, "variants_registered": len(variants), "embargo_ns": embargo_ns(cfg, research), "research_config": research,
              "selection_for_limit_1": selection, "limit_10_note": "Limit 10 is a scenario, not a LIVE release; separate temporal validation required before switching", "limit_10_results": len(limit10),
              "results": outputs, "forward_test_note": "After any adjustment the previous test split is development data; a new untouched forward PAPER period is required (R7).",
              "qualification_rule": "POSITIVE_UNDER_MODEL only with lower bootstrap bound > 0 on untouched forward data, declared stress scenarios survived, no unresolved exposure; never LIVE_PROFIT_PROVEN"}
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "research_report.json").write_text(json.dumps(report, indent=2, sort_keys=True, default=str), encoding="utf-8")
    with (out_dir / "target_comparison.csv").open("w", newline="", encoding="utf-8") as fh:
        cols = ["split", "variant_id", "target", "entry_delay_ms", "exit_delay_ms", "fee_multiplier", "adverse_bps", "reject_rate", "portfolio_limit", "launches", "intents", "resolved", "unresolved", "net_pnl_lamports", "expectancy_per_intent", "ci_low", "ci_high", "blocks", "status"]
        w = csv.writer(fh)
        w.writerow(cols)
        for o in outputs:
            b = o.get("bootstrap", {})
            w.writerow([o.get(c) if c not in ("ci_low", "ci_high", "blocks") else b.get(c) for c in cols])
    return report

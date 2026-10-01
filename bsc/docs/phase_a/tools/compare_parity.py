#!/usr/bin/env python3
"""Compare two strict_replay manifests: exact integer lamports and every semantic section (run-id-independent keys)."""
from __future__ import annotations

import json
import sys

a = json.load(open(sys.argv[1]))
b = json.load(open(sys.argv[2]))
out_path = sys.argv[3] if len(sys.argv) > 3 else None
report = {"a": a["label"], "b": b["label"], "same_source_db_sha256": a["source_db_sha256"] == b["source_db_sha256"], "same_dataset_hash": a["dataset_hash"] == b["dataset_hash"],
          "same_runner_sha256": a["runner_sha256"] == b["runner_sha256"], "same_effective_config_hash": a["effective_config_hash"] == b["effective_config_hash"],
          "same_file_config_hash": a["config_hash_file"] == b["config_hash_file"], "semantic_sha256_equal": a["semantic_sha256"] == b["semantic_sha256"], "sections": {}, "exact_lamports_equal": a["exact_lamports"] == b["exact_lamports"], "diffs": []}
for key in ("realized_net_pnl_lamports", "pessimistic_pnl_lamports_token_residual_zero", "unresolved_exposure_entry_cost_lamports", "locked_rent_lamports_end", "fees_by_kind_lamports", "cash_reconciliation", "ram_cash_lamports", "ram_reserved_lamports", "ram_locked_rent_lamports", "ram_ledger_cash_sum", "sum_position_net_pnl_lamports", "counts", "qualification_status"):
    va, vb = a["exact_lamports"][key], b["exact_lamports"][key]
    report["sections"][f"exact:{key}"] = {"a": va, "b": vb, "equal": va == vb}
    if va != vb:
        report["diffs"].append(f"exact:{key}")
for sec in ("events", "decisions", "attempts", "fills", "selected_fee_evidence", "fee_evidence", "ledger", "positions", "quote_snapshots", "quality_events", "ram_digest"):
    sa, sb = a["semantic"][sec], b["semantic"][sec]
    eq = sa == sb
    entry = {"equal": eq, "size_a": len(sa), "size_b": len(sb)}
    if not eq:
        if isinstance(sa, dict) and isinstance(sb, dict):
            only_a = sorted(set(sa) - set(sb))[:20]
            only_b = sorted(set(sb) - set(sa))[:20]
            changed = [k for k in sa if k in sb and sa[k] != sb[k]][:20]
            entry.update({"only_a": only_a, "only_b": only_b, "changed_keys": changed, "first_change": {"key": changed[0], "a": sa[changed[0]], "b": sb[changed[0]]} if changed else None})
        else:
            first = next((i for i, (x, y) in enumerate(zip(sa, sb)) if x != y), None)
            entry.update({"first_index_differs": first, "a": sa[first] if first is not None else None, "b": sb[first] if first is not None else None})
        report["diffs"].append(f"semantic:{sec}")
    report["sections"][f"semantic:{sec}"] = entry
report["PARITY"] = "PASS" if (report["exact_lamports_equal"] and not report["diffs"] and report["same_dataset_hash"] and report["same_source_db_sha256"] and report["same_runner_sha256"]) else "FAIL"
text = json.dumps(report, indent=1, default=str)
if out_path:
    open(out_path, "w").write(text + "\n")
print(json.dumps({k: v for k, v in report.items() if k not in ("sections",)}, indent=1, default=str))
print("sections:", {k: v["equal"] for k, v in report["sections"].items()})

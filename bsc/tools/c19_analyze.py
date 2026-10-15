#!/usr/bin/env python3
"""C1.9 native latency report from raw samples.

Reads the raw evidence of `c19_native_latency.sh` and writes one JSON report.
It derives every number from raw per-sample timestamps of one clock domain.

It never adds component percentiles into an end-to-end figure, never reports
a percentile in place of the maximum, and reports p99.9 / p99.99 only when the
sample count supports them. Warmup rows stay in the raw evidence and are
excluded from the statistics; the window is the one fixed in the manifest
before the run. The report states measurements and integrity. It does not
decide the C1.9 gate.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import subprocess
import sys
from pathlib import Path

SCHEMA = "darkbot.c1.native_latency.v1"
MISS_NS = 1_000_000
BANDS_NS = (200_000, 500_000, 1_000_000)
P999_MIN_SAMPLES = 1_000
P9999_MIN_SAMPLES = 10_000
NON_CLAIMS = [
    "T0_TO_T_BUY_LT_1MS = NOT_PROVEN",
    "MAINNET_INGRESS_COVERAGE = NOT_PROVEN",
    "PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED",
    "LIVE = BLOCKED",
    "a local geth handoff is not acquisition",
    "macOS/M4 evidence does not qualify the Linux C3 target",
]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def sha256_tree(path: Path, suffix: str) -> str:
    digest = hashlib.sha256()
    for item in sorted(path.rglob("*" + suffix)):
        digest.update(str(item.relative_to(path)).encode())
        digest.update(b"\0")
        digest.update(item.read_bytes())
    return digest.hexdigest()


def git_head(path: Path) -> str:
    try:
        out = subprocess.run(["git", "-C", str(path), "rev-parse", "HEAD"],
                             check=True, capture_output=True, text=True)
        return out.stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unavailable"


def git_dirty(path: Path) -> str:
    try:
        out = subprocess.run(["git", "-C", str(path), "status", "--porcelain", "--untracked-files=no"],
                             check=True, capture_output=True, text=True)
        return "clean" if not out.stdout.strip() else "dirty"
    except (OSError, subprocess.CalledProcessError):
        return "unavailable"


def read_key_values(path: Path) -> dict[str, str]:
    out = {}
    for line in path.read_text().splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            out[key] = value
    return out


def percentile(ordered: list[int], q: float) -> int:
    """Nearest-rank percentile of an ascending list."""
    rank = max(1, math.ceil(q * len(ordered)))
    return ordered[rank - 1]


def describe(values: list[int]) -> dict:
    """Statistics of one directly measured per-sample quantity, in ns."""
    if not values:
        return {"samples": 0, "note": "no samples"}
    ordered = sorted(values)
    count = len(ordered)
    out = {
        "samples": count,
        "min_ns": ordered[0],
        "mean_ns": sum(ordered) / count,
        "p50_ns": percentile(ordered, 0.50),
        "p95_ns": percentile(ordered, 0.95),
        "p99_ns": percentile(ordered, 0.99),
        "p99_9_ns": percentile(ordered, 0.999) if count >= P999_MIN_SAMPLES else None,
        "p99_99_ns": percentile(ordered, 0.9999) if count >= P9999_MIN_SAMPLES else None,
        "max_ns": ordered[-1],
        "misses_ge_1ms": sum(1 for value in ordered if value >= MISS_NS),
        "bands": {
            "lt_200us": sum(1 for value in ordered if value < BANDS_NS[0]),
            "200us_to_lt_500us": sum(1 for value in ordered if BANDS_NS[0] <= value < BANDS_NS[1]),
            "500us_to_lt_1ms": sum(1 for value in ordered if BANDS_NS[1] <= value < BANDS_NS[2]),
            "ge_1ms": sum(1 for value in ordered if value >= BANDS_NS[2]),
        },
    }
    if count < P999_MIN_SAMPLES:
        out["p99_9_note"] = f"not reported: {count} samples < {P999_MIN_SAMPLES}"
    if count < P9999_MIN_SAMPLES:
        out["p99_99_note"] = f"not reported: {count} samples < {P9999_MIN_SAMPLES}"
    return out


def load_ingress(path: Path) -> list[dict]:
    rows = []
    with path.open(newline="") as handle:
        for index, row in enumerate(csv.DictReader(handle)):
            rows.append({
                "index": index,
                "phase": row["phase"],
                "lane_id": int(row["lane_id"]),
                "lane_sequence": int(row["lane_sequence"]),
                "tx_hash": row["tx_hash"],
                "decoded": int(row["t_packet_decoded_ns"]),
                "ingress": int(row["t_ingress_ns"]),
                "l1_pre": int(row["l1_pre_commit_ns"]),
                "l1_post": int(row["l1_post_commit_ns"]),
                "l2": int(row["l2_acquire_ns"]),
                "queue_depth": int(row["queue_depth"]),
            })
    return rows


def load_outbound(path: Path) -> list[dict]:
    rows = []
    with path.open(newline="") as handle:
        for row in csv.DictReader(handle):
            rows.append({
                "tx_hash": row["tx_hash"],
                "egress_sequence": int(row["egress_sequence"]),
                "commit": int(row["egress_commit_ns"]),
                "origin": row["origin"],
                "worker": int(row["worker"]),
                "status": row["status"],
                "selected": int(row["selected"]),
                "queued": int(row["queued"]),
                "observed": int(row["coordinator_observed_ns"]),
                "third": int(row["third_ns"]),
                "event": int(row["event_ns"]),
                "peer": row["peer"],
            })
    return rows


def check_ingress(rows: list[dict], manifest: dict[str, str], failures: list[str]) -> list[dict]:
    """Integrity of the ingress samples; returns the qualification window."""
    peers = int(manifest["peers"])
    warmup = int(manifest["warmup_per_peer"])
    samples = int(manifest["samples_per_peer"])
    if manifest.get("clock_id") != "1":
        failures.append(f"clock_id {manifest.get('clock_id')!r} is not CLOCK_MONOTONIC (1)")
    lanes = [int(value) for value in manifest["lane_ids"].split(",")]
    if len(lanes) != peers:
        failures.append("manifest lane_ids does not list one lane per peer")
    by_lane: dict[int, list[dict]] = {lane: [] for lane in lanes}
    for row in rows:
        if row["lane_id"] not in by_lane:
            failures.append(f"sample {row['index']} on lane {row['lane_id']} outside the manifest")
            continue
        by_lane[row["lane_id"]].append(row)
    window = []
    for lane, lane_rows in by_lane.items():
        if len(lane_rows) != warmup + samples:
            failures.append(f"lane {lane}: {len(lane_rows)} samples, manifest fixes {warmup + samples}")
        for position, row in enumerate(lane_rows):
            if row["lane_sequence"] != position:
                failures.append(f"lane {lane}: sequence {row['lane_sequence']} at position {position} (gap or reorder)")
                break
            expected = "warmup" if position < warmup else "qualification"
            if row["phase"] != expected:
                failures.append(f"lane {lane} sequence {position}: phase {row['phase']}, window fixes {expected}")
                break
            if expected == "qualification":
                window.append(row)
    hashes = [row["tx_hash"] for row in rows]
    if len(set(hashes)) != len(hashes):
        failures.append("duplicate transaction hash among the samples")
    for row in rows:
        ordered = 0 < row["decoded"] <= row["ingress"] <= row["l1_pre"] <= row["l1_post"]
        if not ordered or row["l1_pre"] > row["l2"]:
            failures.append(f"sample {row['index']}: boundary order violated "
                            f"(decoded={row['decoded']} ingress={row['ingress']} l1_pre={row['l1_pre']} "
                            f"l1_post={row['l1_post']} l2={row['l2']})")
            break
    return window


def outbound_metrics(rows: list[dict], ingress: list[dict], window: list[dict],
                     workers: int, failures: list[str]) -> dict:
    """Per-BUY handoff quantities of the qualification window."""
    if workers == 0:
        return {"enabled": False}
    window_indices = {row["index"] for row in window}
    by_buy: dict[int, list[dict]] = {}
    for row in rows:
        by_buy.setdefault(row["egress_sequence"], []).append(row)
    observed, first, each, all_selected, send = [], [], [], [], []
    ingress_to_commit, ingress_to_first = [], []
    status_counts: dict[str, int] = {}
    partial = 0
    for index in sorted(window_indices):
        buy = by_buy.get(index)
        if buy is None:
            failures.append(f"BUY {index}: no outbound evidence")
            continue
        summaries = [row for row in buy if row["status"] == "DISPATCHED"]
        if len(summaries) != 1:
            failures.append(f"BUY {index}: {len(summaries)} DISPATCHED records")
            continue
        summary = summaries[0]
        slots = [row for row in buy if row["status"] != "DISPATCHED"]
        if len(slots) != workers or sorted(row["worker"] for row in slots) != list(range(workers)):
            failures.append(f"BUY {index}: evidence does not cover every worker slot exactly once")
            continue
        commit = summary["commit"]
        if not commit <= summary["observed"]:
            failures.append(f"BUY {index}: coordinator observed before the egress commit")
            continue
        observed.append(summary["observed"] - commit)
        handed = [row for row in slots if row["status"] == "HANDED_OFF"]
        for row in slots:
            status_counts[row["status"]] = status_counts.get(row["status"], 0) + 1
        for row in handed:
            each.append(row["event"] - commit)
            send.append(row["event"] - row["third"])
        launch = ingress[index]
        ingress_to_commit.append(commit - launch["ingress"])
        if handed:
            first_event = min(row["event"] for row in handed)
            first.append(first_event - commit)
            ingress_to_first.append(first_event - launch["ingress"])
        if summary["selected"] > 0 and len(handed) == summary["selected"]:
            all_selected.append(max(row["event"] for row in handed) - commit)
        else:
            partial += 1
    return {
        "enabled": True,
        "workers": workers,
        "buys_in_window": len(window_indices),
        "worker_slot_outcomes": status_counts,
        "buys_without_all_selected_handoffs": partial,
        "egress_commit_to_coordinator_observed": describe(observed),
        "egress_commit_to_first_selected_peer_handoff": describe(first),
        "egress_commit_to_each_selected_peer_handoff": describe(each),
        "egress_commit_to_all_selected_peer_handoffs": describe(all_selected),
        "worker_send_call_duration": describe(send),
        "prepared_buy_path": {
            "scope": "harness path with a prepared signed BUY: scope match, one launch copy, seal, "
                     "egress commit. It excludes venue decode beyond the scope match, state, economics, "
                     "admission, build and sign, and it is not the complete local reaction.",
            "T_ingress_to_egress_commit": describe(ingress_to_commit),
            "T_ingress_to_first_selected_peer_handoff": describe(ingress_to_first),
        },
        "handoff_boundary": "return of the worker's send call (harness: one write to a loopback TCP "
                            "socket with TCP_NODELAY); not remote receipt, inclusion or execution",
    }


def check_counters(go: dict, rust: dict[str, str], total: int, failures: list[str]) -> dict:
    peers = go.get("ingress", {}).get("peers", {})
    chain = go.get("chain", {})
    outbound = go.get("outbound", {})
    counters = {
        "peer_published": peers.get("published"),
        "peer_queue_full": peers.get("queue_full"),
        "peer_oversize": peers.get("oversize"),
        "peer_refused": peers.get("refused"),
        "uncovered_candidates": peers.get("uncovered_candidates"),
        "gap_total": go.get("ingress", {}).get("gap_total"),
        "chain_published": chain.get("published"),
        "chain_lost": chain.get("lost_records"),
        "chain_ownership_violations": chain.get("ownership_violations"),
        "outbound_queue_full_hol": outbound.get("queue_full"),
        "outbound_invalid_egress": outbound.get("invalid_egress"),
        "outbound_evidence_lost": outbound.get("coordinator_evidence_lost"),
        "hot0_lane_faults": int(rust.get("hot0_lane_faults", -1)),
        "hot0_invalid_records": int(rust.get("hot0_invalid_records", -1)),
        "hot0_dropped": int(rust.get("hot0_dropped", -1)),
        "hot0_chain_gaps": int(rust.get("hot0_chain_gaps", -1)),
        "hot0_allocations_in_qualification_window": int(rust.get("hot0_allocations_in_qualification_window", -1)),
        "hot0_locks": "none in the receive path by construction; not instrumented",
    }
    if peers.get("published") != total:
        failures.append(f"geth published {peers.get('published')} peer records, samples {total}")
    must_be_zero = ("peer_queue_full", "peer_oversize", "peer_refused", "uncovered_candidates", "gap_total",
                    "chain_lost", "chain_ownership_violations", "hot0_lane_faults", "hot0_invalid_records",
                    "hot0_dropped", "hot0_chain_gaps")
    for name in must_be_zero:
        if counters[name] != 0:
            failures.append(f"{name} = {counters[name]}, must be 0")
    return counters


def build_report(args: argparse.Namespace) -> dict:
    failures: list[str] = []
    manifest = read_key_values(Path(args.manifest))
    ingress = load_ingress(Path(args.raw_ingress))
    outbound = load_outbound(Path(args.raw_outbound))
    go = json.loads(Path(args.go_snapshot).read_text())
    rust = read_key_values(Path(args.rust_summary))
    host = json.loads(Path(args.host_profile).read_text())
    window = check_ingress(ingress, manifest, failures)
    counters = check_counters(go, rust, len(ingress), failures)
    workers = int(manifest.get("outbound_workers", "0"))
    repo, geth = Path(args.repo), Path(args.geth)
    tools = Path(__file__).resolve().parent
    metrics = outbound_metrics(outbound, ingress, window, workers, failures)
    mode = manifest.get("outbound_mode", "unclassified")
    production = mode == "production_devp2p_selected_peer_handoff" and workers > 0
    if mode not in ("production_devp2p_selected_peer_handoff", "loopback_microbenchmark"):
        failures.append("missing or unknown outbound measurement mode")
    return {
        "outbound_mode": mode,
        "c1_9_production_path_present": production,
        "purpose": manifest.get("purpose", "unclassified"),
        "c1_9_qualification_eligible": production and not failures and manifest.get("purpose") == "qualification",
        "schema": SCHEMA,
        "result": "MEASURED",
        "integrity": "FAIL" if failures else "PASS",
        "integrity_failures": failures,
        "gate": "NOT_DECIDED_BY_THIS_TOOL",
        "clock": {
            "clock_id": manifest.get("clock_id"),
            "domain": "CLOCK_MONOTONIC of one host; geth (Go) and Rust read the same clock",
        },
        "window": {
            "fixed_before_run": True,
            "peers": int(manifest["peers"]),
            "warmup_per_peer": int(manifest["warmup_per_peer"]),
            "samples_per_peer": int(manifest["samples_per_peer"]),
            "warmup_samples_retained_in_raw": sum(1 for row in ingress if row["phase"] == "warmup"),
            "qualification_samples": len(window),
            "load_model": manifest.get("load_model"),
        },
        "l1_semantics": {
            "pre_commit": "clock read after the slot commit, before write_sequence Release",
            "post_commit": "clock read after write_sequence Release",
            "note": "L1 lies inside this bracket; T_ingress -> L2 is measured directly",
        },
        "ingress_metrics": {
            "T_packet_decoded_to_T_ingress": describe([row["ingress"] - row["decoded"] for row in window]),
            "T_ingress_to_L1_pre_commit_lower_bound": describe([row["l1_pre"] - row["ingress"] for row in window]),
            "T_ingress_to_L1_post_commit_upper_bound": describe([row["l1_post"] - row["ingress"] for row in window]),
            "L1_commit_bracket_width": describe([row["l1_post"] - row["l1_pre"] for row in window]),
            "L1_to_L2_lower_bound": describe([max(0, row["l2"] - row["l1_post"]) for row in window]),
            "L1_to_L2_upper_bound": describe([row["l2"] - row["l1_pre"] for row in window]),
            "T_ingress_to_L2": describe([row["l2"] - row["ingress"] for row in window]),
            "queue_depth_max": max((row["queue_depth"] for row in window), default=0),
        },
        "outbound_metrics": metrics,
        "counters": counters,
        "identity": {
            "darkbot_commit": git_head(repo),
            "darkbot_tree": git_dirty(repo),
            "pinned_geth_commit": git_head(geth),
            "go_test_binary_sha256": sha256_file(Path(args.go_bin)),
            "rust_consumer_binary_sha256": sha256_file(Path(args.rust_bin)),
            "go_overlay_sources_sha256": sha256_tree(repo / "bsc/geth/overlay/darkbot/fastlane", ".go"),
            "rust_fastlane_sources_sha256": sha256_tree(repo / "bsc/engine/crates/geth_ipc", ".rs"),
            "overlay_tool_sha256": sha256_file(tools / "c1_apply_geth_overlay.py"),
            "harness_script_sha256": sha256_file(tools / "c19_native_latency.sh"),
            "analyzer_sha256": sha256_file(Path(__file__).resolve()),
            "production_bridge_sha256": sha256_file(tools / "c19_prepare_production.py"),
            "production_peer_harness_sha256": sha256_file(tools / "c19_production_peer.go.txt"),
            "config_manifest_sha256": sha256_file(Path(args.manifest)),
            "host_profile_sha256": sha256_file(Path(args.host_profile)),
            "topology_fingerprint_crc32c": host.get("topology_fingerprint_crc32c"),
            "profile_selection": host.get("profile_selection"),
            "raw_ingress_sha256": sha256_file(Path(args.raw_ingress)),
            "raw_outbound_sha256": sha256_file(Path(args.raw_outbound)),
            "go_snapshot_sha256": sha256_file(Path(args.go_snapshot)),
        },
        "rules": [
            "every statistic is over directly measured per-sample differences of one clock",
            "no end-to-end figure is built by adding component percentiles",
            "max is reported for every quantity; no percentile stands in for it",
            f"a miss is a value >= {MISS_NS} ns",
            f"p99.9 needs >= {P999_MIN_SAMPLES} samples, p99.99 >= {P9999_MIN_SAMPLES}",
        ],
        "non_claims": NON_CLAIMS,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("raw-ingress", "raw-outbound", "manifest", "go-snapshot", "rust-summary", "host-profile",
                 "repo", "geth", "go-bin", "rust-bin", "output"):
        parser.add_argument("--" + name, required=True)
    args = parser.parse_args()
    report = build_report(args)
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n")
    outbound = report["outbound_metrics"]
    print(f"integrity={report['integrity']}")
    print(f"max_T_ingress_to_L2_ns={report['ingress_metrics']['T_ingress_to_L2'].get('max_ns')}")
    if outbound.get("enabled"):
        first = outbound["egress_commit_to_first_selected_peer_handoff"]
        print(f"max_egress_commit_to_first_handoff_ns={first.get('max_ns')}")
    for failure in report["integrity_failures"]:
        print(f"C19_INTEGRITY_FAIL {failure}", file=sys.stderr)
    return 1 if report["integrity_failures"] else 0


if __name__ == "__main__":
    sys.exit(main())

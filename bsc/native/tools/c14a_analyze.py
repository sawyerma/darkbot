#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import statistics
import subprocess
from pathlib import Path


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def sha256_overlay(path: Path) -> str:
    h = hashlib.sha256()
    for file in sorted(path.glob("*.go")):
        h.update(file.name.encode())
        h.update(b"\0")
        h.update(file.read_bytes())
        h.update(b"\0")
    return h.hexdigest()


def command(*args: str, cwd: Path | None = None) -> str:
    return subprocess.check_output(args, cwd=cwd, text=True).strip()


def percentile(values: list[int], q: float) -> int:
    ordered = sorted(values)
    index = max(0, min(len(ordered) - 1, math.ceil(q * len(ordered)) - 1))
    return ordered[index]


def describe(values: list[int]) -> dict[str, int | float]:
    return {
        "min_ns": min(values),
        "mean_ns": statistics.fmean(values),
        "p50_ns": percentile(values, 0.50),
        "p95_ns": percentile(values, 0.95),
        "p99_ns": percentile(values, 0.99),
        "p99_9_ns": percentile(values, 0.999),
        "max_ns": max(values),
    }


def read_key_values(path: Path) -> dict[str, str]:
    out: dict[str, str] = {}
    for line in path.read_text().splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            out[key] = value
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-dir", required=True)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--geth", required=True)
    parser.add_argument("--go-bin", required=True)
    parser.add_argument("--rust-bin", required=True)
    parser.add_argument("--raw-evidence", required=True)
    parser.add_argument("--go-log", required=True)
    parser.add_argument("--rust-log", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    run_dir = Path(args.run_dir)
    repo = Path(args.repo)
    geth = Path(args.geth)
    raw_evidence = Path(args.raw_evidence)
    manifest = read_key_values(run_dir / "manifest.txt")

    with (run_dir / "raw.csv").open(newline="") as f:
        rows = list(csv.DictReader(f))
    all_samples = [
        {
            key: int(value) if key != "hash" else value
            for key, value in row.items()
        }
        for row in rows
    ]
    if not all_samples:
        raise SystemExit("C1.4a: no samples")

    peers = int(manifest["peers"])
    warmup_per_peer = int(manifest.get("warmup_per_peer", "0"))
    samples_per_peer = int(manifest["samples_per_peer"])
    qualification_end = warmup_per_peer + samples_per_peer

    warmup_samples = [
        s for s in all_samples
        if s["sequence"] < warmup_per_peer
    ]
    samples = [
        s for s in all_samples
        if warmup_per_peer <= s["sequence"] < qualification_end
    ]

    packet_to_ingress = [s["t_ingress_ns"] - s["t_packet_decoded_ns"] for s in samples]
    ingress_to_l1_pre = [s["l1_pre_commit_ns"] - s["t_ingress_ns"] for s in samples]
    ingress_to_l1_post = [s["l1_post_commit_ns"] - s["t_ingress_ns"] for s in samples]
    commit_bracket = [s["l1_post_commit_ns"] - s["l1_pre_commit_ns"] for s in samples]
    l1_to_l2_lower = [max(0, s["l2_acquire_ns"] - s["l1_post_commit_ns"]) for s in samples]
    l1_to_l2_upper = [s["l2_acquire_ns"] - s["l1_pre_commit_ns"] for s in samples]
    ingress_to_l2 = [s["l2_acquire_ns"] - s["t_ingress_ns"] for s in samples]
    queue_depth = [s["queue_depth"] for s in samples]

    expected_warmup = peers * warmup_per_peer
    expected_qualification = peers * samples_per_peer
    expected_total = expected_warmup + expected_qualification
    snapshot = json.loads((run_dir / "go_snapshot.json").read_text())
    with (run_dir / "lane_stats.csv").open(newline="") as f:
        lane_stats = list(csv.DictReader(f))

    failures: list[str] = []
    if len(all_samples) != expected_total:
        failures.append(
            f"raw_sample_count={len(all_samples)} expected_total={expected_total}"
        )
    if len(warmup_samples) != expected_warmup:
        failures.append(
            f"warmup_sample_count={len(warmup_samples)} expected={expected_warmup}"
        )
    if len(samples) != expected_qualification:
        failures.append(
            f"qualification_sample_count={len(samples)} expected={expected_qualification}"
        )
    if snapshot.get("gap_total") != 0:
        failures.append(f"gap_total={snapshot.get('gap_total')}")
    if snapshot.get("live_covered") != int(manifest["peers"]):
        failures.append(f"live_covered={snapshot.get('live_covered')}")
    if snapshot.get("live_uncovered") != 0:
        failures.append(f"live_uncovered={snapshot.get('live_uncovered')}")

    peer = snapshot.get("peers", {})
    for key in (
        "queue_full",
        "oversize",
        "refused",
        "serialize_errors",
        "clock_errors",
        "uncovered_candidates",
    ):
        if peer.get(key, 0) != 0:
            failures.append(f"peers.{key}={peer.get(key)}")

    per_lane_expected = warmup_per_peer + samples_per_peer
    if len(lane_stats) != int(manifest["peers"]):
        failures.append(f"lane_stats={len(lane_stats)}")
    for lane in lane_stats:
        lane_id = lane["lane_id"]
        for key in ("dropped_full", "oversize", "corrupt_records", "quarantine_reason"):
            if int(lane[key]) != 0:
                failures.append(f"lane {lane_id} {key}={lane[key]}")
        if int(lane["write_sequence"]) != per_lane_expected:
            failures.append(f"lane {lane_id} write_sequence={lane['write_sequence']}")
        if int(lane["read_sequence"]) != per_lane_expected:
            failures.append(f"lane {lane_id} read_sequence={lane['read_sequence']}")

    for index, sample in enumerate(all_samples):
        if not (
            0 < sample["t_packet_decoded_ns"] <= sample["t_ingress_ns"]
            <= sample["l1_pre_commit_ns"] <= sample["l1_post_commit_ns"]
        ):
            failures.append(f"sample {index} invalid producer timestamp order")
            break
        if sample["l1_pre_commit_ns"] > sample["l2_acquire_ns"]:
            failures.append(f"sample {index} L2 precedes pre-commit bracket")
            break

    max_ingress_to_l2 = max(ingress_to_l2)
    if failures:
        classification = "FAIL_INTEGRITY"
    elif max_ingress_to_l2 < 200_000:
        classification = "GREEN_STRONG_HEADROOM"
    elif max_ingress_to_l2 < 500_000:
        classification = "GREEN_FEASIBLE"
    elif max_ingress_to_l2 < 1_000_000:
        classification = "AMBER_LOW_HEADROOM"
    else:
        classification = "RED_ARCHITECTURE_FAIL"

    bands = {
        "lt_200us": sum(value < 200_000 for value in ingress_to_l2),
        "200_to_499_999ns": sum(200_000 <= value < 500_000 for value in ingress_to_l2),
        "500_to_999_999ns": sum(500_000 <= value < 1_000_000 for value in ingress_to_l2),
        "ge_1ms": sum(value >= 1_000_000 for value in ingress_to_l2),
    }

    output = Path(args.output)
    try:
        raw_ref = str(raw_evidence.relative_to(repo))
    except ValueError:
        raw_ref = str(raw_evidence)

    evidence = {
        "schema": "darkbot.c1.c1_4a_early_latency.v1",
        "gate": "C1.4a",
        "classification": classification,
        "authoritative_metric": "max T_ingress -> L2",
        "sample_count": len(samples),
        "warmup_sample_count": len(warmup_samples),
        "raw_sample_count": len(all_samples),
        "qualification_window": {
            "warmup_per_peer": warmup_per_peer,
            "qualified_samples_per_peer": samples_per_peer,
            "classification_uses_sequences": f"{warmup_per_peer}..{qualification_end - 1}",
            "warmup_retained_in_raw_evidence": True,
            "reason": "exclude explicit pre-window cold-start initialization from the steady-state C1.4a feasibility classification; cold-start evidence remains retained",
        },
        "clock_domain": "CLOCK_MONOTONIC shared by Go unix.ClockGettime and Rust libc::clock_gettime",
        "measurement_instrumentation": "disposable pinned-geth worktree; production C1.4 overlay remains unchanged",
        "l1_semantics": {
            "definition": "SPSC write_sequence Release commit",
            "measurement": "bracketed by CLOCK_MONOTONIC reads immediately before and immediately after the write_sequence atomic store",
            "note": "classification does not depend on L1 interpolation; T_ingress -> L2 is measured directly",
        },
        "configuration": {
            "generation": int(manifest["generation"]),
            "active_peer_lanes": int(manifest["peers"]),
            "warmup_per_peer": warmup_per_peer,
            "samples_per_peer": samples_per_peer,
            "peer_lane_capacity": int(manifest["peer_lane_capacity"]),
            "peer_lane_slot_stride": int(manifest["peer_lane_slot_stride"]),
            "lane_ids": [int(v) for v in manifest["lane_ids"].split(",")],
            "clock_id": int(manifest["clock_id"]),
            "load_model": manifest.get("load_model", "unspecified"),
            "manifest_sha256": sha256_file(run_dir / "manifest.txt"),
        },
        "metrics": {
            "T_packet_decoded_to_T_ingress": describe(packet_to_ingress),
            "T_ingress_to_L1_pre_commit_lower_bound": describe(ingress_to_l1_pre),
            "T_ingress_to_L1_post_commit_upper_bound": describe(ingress_to_l1_post),
            "L1_commit_bracket_width": describe(commit_bracket),
            "L1_to_L2_lower_bound": describe(l1_to_l2_lower),
            "L1_to_L2_upper_bound": describe(l1_to_l2_upper),
            "T_ingress_to_L2": describe(ingress_to_l2),
            "queue_depth_at_L2": describe(queue_depth),
        },
        "deadline_bands_T_ingress_to_L2": bands,
        "integrity": {
            "pass": not failures,
            "failures": failures,
            "go_snapshot": snapshot,
            "rust_lane_stats": lane_stats,
        },
        "identity": {
            "darkbot_commit": command("git", "-C", str(repo), "rev-parse", "HEAD"),
            "pinned_geth_commit": command("git", "-C", str(geth), "rev-parse", "HEAD"),
            "go_test_binary_sha256": sha256_file(Path(args.go_bin)),
            "rust_test_binary_sha256": sha256_file(Path(args.rust_bin)),
            "go_overlay_sources_sha256": sha256_overlay(repo / "bsc/native/geth/overlay/darkbot/fastlane"),
            "measurement_geth_v2_sha256": sha256_file(geth / "darkbot/fastlane/v2.go"),
            "measurement_probe_sha256": sha256_file(geth / "darkbot/fastlane/c14a_probe.go"),
            "measurement_go_test_sha256": sha256_file(geth / "darkbot/fastlane/c14a_latency_test.go"),
            "rust_v2_source_sha256": sha256_file(repo / "bsc/native/crates/darkbot-fastlane/src/v2.rs"),
            "rust_consumer_source_sha256": sha256_file(
                repo / "bsc/native/crates/darkbot-fastlane/examples/c14a_consumer.rs"
            ),
            "harness_script_sha256": sha256_file(repo / "bsc/native/tools/c14a_early_latency.sh"),
            "analyzer_sha256": sha256_file(repo / "bsc/native/tools/c14a_analyze.py"),
            "go_version": command("go", "version"),
            "rustc_version": command("rustc", "--version"),
            "cargo_version": command("cargo", "--version"),
        },
        "artifacts": {
            "raw_samples": raw_ref,
            "raw_samples_sha256": sha256_file(raw_evidence),
            "go_log": str(Path(args.go_log).name),
            "rust_log": str(Path(args.rust_log).name),
        },
        "commands": [
            "go test -c -o <temporary-c14a-go.test> ./darkbot/fastlane",
            "cargo build -p darkbot-fastlane --release --example c14a_consumer --locked --offline",
            "bsc/native/tools/c14a_early_latency.sh",
        ],
        "non_claims": {
            "T0_TO_T_BUY_LT_1MS": "NOT_PROVEN",
            "MAINNET_INGRESS_COVERAGE": "NOT_PROVEN",
            "PROVIDER_ORDERING_PLACEMENT": "UNQUALIFIED",
            "LIVE": "BLOCKED",
        },
    }

    output.write_text(json.dumps(evidence, indent=2) + "\n")
    print(classification)
    print(f"samples={len(samples)} max_T_ingress_to_L2_ns={max_ingress_to_l2}")
    if failures:
        for failure in failures:
            print(f"integrity_failure={failure}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

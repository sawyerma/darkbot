#!/usr/bin/env python3
"""C1.9b hook/path-early qualification on Testnet.

For controlled, known launches it measures, per source and peer,

    known launch handoff -> packet decode -> T_ingress

from two raw inputs:

  --launches      JSON written by the controlled launcher: for every launch
                  its transaction hash and the handoff timestamp;
  --observations  CSV of every EARLY_TX_SEEN record the Fast Lane carried,
                  written by the `c19b_capture` example.

Both must be timestamps of the same clock (CLOCK_MONOTONIC, clock id 1, of
the same host and boot); otherwise the differences are meaningless and the
tool refuses. It refuses anything that is not a Testnet run. The result
characterizes how late the selected hook/path is. It does not prove Mainnet
peer coverage, private-flow visibility or a Mainnet arrival advantage, and it
authorizes no value-bearing action.
"""

from __future__ import annotations

import argparse
import csv
import json
import sys
from pathlib import Path

from c19_analyze import describe, sha256_file

SCHEMA = "darkbot.c1.hook_path.v1"
TESTNET_CHAIN_IDS = {97: "bsc-testnet"}
SOURCES = ("eth-broadcast", "eth-pooled")
NON_CLAIMS = [
    "MAINNET_INGRESS_COVERAGE = NOT_PROVEN",
    "a Testnet hook/path result is not a Mainnet coverage or arrival-advantage result",
    "private-flow visibility is not measured",
    "T0_TO_T_BUY_LT_1MS = NOT_PROVEN",
    "PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED",
    "LIVE = BLOCKED",
]


class Refused(Exception):
    """The inputs cannot support a hook/path measurement."""


def load_launches(path: Path) -> dict:
    data = json.loads(path.read_text())
    chain_id = data.get("chain_id")
    if chain_id not in TESTNET_CHAIN_IDS:
        raise Refused(f"chain_id {chain_id!r} is not a Testnet; C1.9b measures Testnet only")
    if data.get("clock_id") != 1 or not data.get("host_boot_id"):
        raise Refused("launch handoffs need clock_id 1 (CLOCK_MONOTONIC) and the host boot id")
    if not data.get("handoff_boundary"):
        raise Refused("the launcher must state what its handoff timestamp marks")
    launches = data.get("launches") or []
    if not launches:
        raise Refused("no controlled launches")
    seen = set()
    for launch in launches:
        tx_hash = str(launch.get("tx_hash", "")).lower().removeprefix("0x")
        if len(tx_hash) != 64 or tx_hash in seen:
            raise Refused(f"launch hash {launch.get('tx_hash')!r} is malformed or repeated")
        if not isinstance(launch.get("handoff_ns"), int) or launch["handoff_ns"] <= 0:
            raise Refused(f"launch {tx_hash}: handoff_ns missing")
        seen.add(tx_hash)
        launch["tx_hash"] = tx_hash
    return data


def load_observations(path: Path, boot_id: str) -> list[dict]:
    rows = []
    with path.open(newline="") as handle:
        for row in csv.DictReader(handle):
            if row["clock_id"] != "1":
                raise Refused(f"observation with clock_id {row['clock_id']}; only CLOCK_MONOTONIC compares")
            if row["host_boot_id"] != boot_id:
                raise Refused("observations and launch handoffs come from different hosts or boots")
            if row["source"] not in SOURCES:
                raise Refused(f"observation source {row['source']!r} is not a peer ingress source")
            decoded, ingress = int(row["t_packet_decoded_ns"]), int(row["t_ingress_ns"])
            if not 0 < decoded <= ingress:
                raise Refused(f"observation of {row['tx_hash']}: T_packet_decoded after T_ingress")
            rows.append({
                "tx_hash": row["tx_hash"].lower(),
                "peer": row["peer"].lower(),
                "source": row["source"],
                "decoded": decoded,
                "ingress": ingress,
            })
    return rows


def analyze(launch_data: dict, observations: list[dict]) -> dict:
    by_hash: dict[str, list[dict]] = {}
    for row in observations:
        by_hash.setdefault(row["tx_hash"], []).append(row)
    per_launch = []
    groups: dict[tuple[str, str], dict[str, list[int]]] = {}
    earliest_ingress, earliest_decode = [], []
    missed, before_handoff = [], []
    first_source_counts = {source: 0 for source in SOURCES}
    for launch in launch_data["launches"]:
        tx_hash, handoff = launch["tx_hash"], launch["handoff_ns"]
        rows = sorted(by_hash.get(tx_hash, []), key=lambda row: row["ingress"])
        if not rows:
            missed.append(tx_hash)
            per_launch.append({"tx_hash": tx_hash, "handoff_ns": handoff, "observed": False, "observations": []})
            continue
        if rows[0]["decoded"] < handoff:
            # The launch was on the wire before the stated handoff: the
            # handoff boundary or the clock is wrong for this launch.
            before_handoff.append(tx_hash)
            per_launch.append({"tx_hash": tx_hash, "handoff_ns": handoff, "observed": True,
                               "excluded": "observed before the stated handoff", "observations": rows})
            continue
        first_source_counts[rows[0]["source"]] += 1
        earliest_ingress.append(rows[0]["ingress"] - handoff)
        earliest_decode.append(min(row["decoded"] for row in rows) - handoff)
        for row in rows:
            group = groups.setdefault((row["source"], row["peer"]), {"decode": [], "ingress": [], "validation": [],
                                                                     "behind_first": []})
            group["decode"].append(row["decoded"] - handoff)
            group["ingress"].append(row["ingress"] - handoff)
            group["validation"].append(row["ingress"] - row["decoded"])
            group["behind_first"].append(row["ingress"] - rows[0]["ingress"])
        per_launch.append({"tx_hash": tx_hash, "handoff_ns": handoff, "observed": True,
                           "first_source": rows[0]["source"], "first_peer": rows[0]["peer"],
                           "handoff_to_first_ingress_ns": rows[0]["ingress"] - handoff,
                           "observations": rows})
    launches = len(launch_data["launches"])
    usable = launches - len(missed) - len(before_handoff)
    sources = []
    for (source, peer), group in sorted(groups.items()):
        sources.append({
            "source": source,
            "peer": peer,
            "launches_observed": len(group["ingress"]),
            "launches_not_observed": usable - len(group["ingress"]),
            "handoff_to_packet_decode": describe(group["decode"]),
            "handoff_to_T_ingress": describe(group["ingress"]),
            "packet_decode_to_T_ingress": describe(group["validation"]),
            "T_ingress_behind_first_source": describe(group["behind_first"]),
        })
    by_kind = {}
    for source in SOURCES:
        values = [value for (kind, _), group in groups.items() if kind == source for value in group["ingress"]]
        by_kind[source] = {"handoff_to_T_ingress": describe(values), "first_for_launches": first_source_counts[source]}
    return {
        "schema": SCHEMA,
        "result": "MEASURED",
        "network": TESTNET_CHAIN_IDS[launch_data["chain_id"]],
        "chain_id": launch_data["chain_id"],
        "clock": {"clock_id": 1, "host_boot_id": launch_data["host_boot_id"], "domain": "CLOCK_MONOTONIC, one host"},
        "handoff_boundary": launch_data["handoff_boundary"],
        "launches": launches,
        "launches_observed": usable,
        "launches_missed": len(missed),
        "missed_tx_hashes": missed,
        "launches_excluded_observed_before_handoff": before_handoff,
        "handoff_to_earliest_packet_decode": describe(earliest_decode),
        "handoff_to_earliest_T_ingress": describe(earliest_ingress),
        "broadcast_vs_pooled": by_kind,
        "per_source_and_peer": sources,
        "per_launch": per_launch,
        "gate": "NOT_DECIDED_BY_THIS_TOOL",
        "non_claims": NON_CLAIMS,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--launches", required=True)
    parser.add_argument("--observations", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    try:
        launch_data = load_launches(Path(args.launches))
        observations = load_observations(Path(args.observations), launch_data["host_boot_id"])
    except Refused as reason:
        print(f"C19B_REFUSED {reason}", file=sys.stderr)
        return 2
    report = analyze(launch_data, observations)
    report["inputs"] = {
        "launches_sha256": sha256_file(Path(args.launches)),
        "observations_sha256": sha256_file(Path(args.observations)),
        "analyzer_sha256": sha256_file(Path(__file__).resolve()),
    }
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n")
    print(f"launches={report['launches']} observed={report['launches_observed']} missed={report['launches_missed']}")
    print("C1_9B_MEASUREMENT_COMPLETE (Testnet hook/path only; MAINNET_INGRESS_COVERAGE = NOT_PROVEN)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

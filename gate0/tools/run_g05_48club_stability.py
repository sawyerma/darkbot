#!/usr/bin/env python3
import hashlib
import json
import subprocess
import sys
from collections import Counter
from pathlib import Path

RUNS = 10
BIN = Path("gate0/target/release/g05_remote_e2e")
INPUTS = Path("gate0/evidence/g03_rust_inputs.json")
OUTPUTS = Path("gate0/evidence/g03_rust_output.json")
OUTDIR = Path("gate0/evidence/g05_48club_builder_stability")

def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()

def main() -> int:
    for p in (BIN, INPUTS, OUTPUTS):
        if not p.exists():
            raise SystemExit(f"missing required file: {p}")

    OUTDIR.mkdir(parents=True, exist_ok=True)

    rows = []
    aggregate_errors = Counter()
    total_samples = 0
    total_over_1ms = 0

    for i in range(1, RUNS + 1):
        print(f"[{i}/{RUNS}] running 48Club G0.5 stability sample...", flush=True)
        proc = subprocess.run(
            [str(BIN), str(INPUTS), str(OUTPUTS), "--48club-builder"],
            check=True,
            text=True,
            capture_output=True,
        )
        data = json.loads(proc.stdout)

        if data.get("status") != "G0.5_48CLUB_BUILDER_TRANSPORT_PREQUALIFICATION":
            raise SystemExit(f"unexpected status in run {i}: {data.get('status')}")

        run_path = OUTDIR / f"run_{i:02d}.json"
        run_path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")

        total = data["stages"]["T_pre_to_remote_TLS_TCP_handoff"]
        samples = int(data["measured_samples"])
        over = int(total.get("over_1ms_count", 0))

        total_samples += samples
        total_over_1ms += over
        aggregate_errors.update(data.get("json_rpc_error_histogram", {}))

        rows.append({
            "run": i,
            "p50_us": total["p50_us"],
            "p95_us": total["p95_us"],
            "p99_us": total["p99_us"],
            "max_us": total["max_us"],
            "over_1ms_count": over,
            "samples": samples,
            "sha256": sha256(run_path),
        })

    summary = {
        "status": "G0.5_48CLUB_BUILDER_STABILITY",
        "runs": RUNS,
        "total_samples": total_samples,
        "total_over_1ms": total_over_1ms,
        "fraction_over_1ms": total_over_1ms / total_samples if total_samples else None,
        "best_p99_us": min(r["p99_us"] for r in rows),
        "worst_p99_us": max(r["p99_us"] for r in rows),
        "worst_max_us": max(r["max_us"] for r in rows),
        "runs_with_p99_under_1ms": sum(r["p99_us"] < 1000.0 for r in rows),
        "runs_with_max_under_1ms": sum(r["max_us"] < 1000.0 for r in rows),
        "json_rpc_error_histogram": dict(aggregate_errors),
        "run_summaries": rows,
        "non_claims": [
            "Invalid wire transaction only",
            "No valid or value-bearing BUY sent",
            "Measures T_pre through warm 48Club TLS/TCP handoff",
            "Does not prove builder inclusion, ordering, acquisition or T_buy",
        ],
    }

    summary_path = OUTDIR / "summary.json"
    summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")

    print(json.dumps(summary, indent=2, sort_keys=True))
    print(f"summary_sha256: {sha256(summary_path)}")
    return 0

if __name__ == "__main__":
    sys.exit(main())

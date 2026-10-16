#!/usr/bin/env python3
"""Timed R126 Mainnet Shadow qualification with automatic stop and evaluation.

The observation timer starts only after threshold_index_ready. The wrapper launches the
existing send-disabled scripts/capture.sh shadow in its own process group, tees terminal output,
stops the whole group with SIGINT after the requested observation window, and writes a compact
machine-readable + human-readable report.

No LIVE/send setting is changed here; capture.sh shadow remains the authority and forces
P0_MODE=shadow and P0_ZERO_PREFUND_QUALIFIED=false.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import signal
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

BASELINE_MAX_PENDING = 6578
BASELINE_DEPENDENCY_UPDATES = 7
BASELINE_CANDIDATE_HITS = 15535

THRESHOLD_RE = re.compile(
    r"threshold_index_ready:\s+tracked=(?P<tracked>\d+)\s+fallback=(?P<fallback>\d+)"
)
STATS_RE = re.compile(
    r"oracle_trigger_stats:\s+"
    r"threshold_ready=(?P<threshold_ready>true|false)\s+"
    r"tracked=(?P<tracked>\d+)\s+"
    r"fallback=(?P<fallback>\d+)\s+"
    r"bootstrap_remaining=(?P<bootstrap_remaining>\d+)\s+"
    r"dependency_updates=(?P<dependency_updates>\d+)\s+"
    r"candidate_hits=(?P<candidate_hits>\d+)\s+"
    r"enqueued=(?P<enqueued>\d+)\s+"
    r"coalesced=(?P<coalesced>\d+)\s+"
    r"price_fallbacks=(?P<price_fallbacks>\d+)\s+"
    r"pending=(?P<pending>\d+)\s+"
    r"max_pending=(?P<max_pending>\d+)\s+"
    r"detect_queue=(?P<detect_queue>\d+)\s+"
    r"bank_projection_pending=(?P<bank_projection_pending>\d+)\s+"
    r"bank_projection_submitted=(?P<bank_projection_submitted>\d+)\s+"
    r"bank_projection_ready=(?P<bank_projection_ready>\d+)\s+"
    r"bank_projection_failed=(?P<bank_projection_failed>\d+)\s+"
    r"bank_projection_stale=(?P<bank_projection_stale>\d+)"
    r"(?:\s+ingress_backlog=(?P<ingress_backlog>\d+)\s+"
    r"ingress_since_detect=(?P<ingress_since_detect>\d+))?"
)
CHANNEL_RE = re.compile(
    r'channel=ChannelStats\s*\{\s*name:\s*"ingress",\s*'
    r"capacity:\s*(?P<capacity>\d+),\s*"
    r"sent:\s*(?P<sent>\d+),\s*"
    r"overflow:\s*(?P<overflow>\d+),\s*"
    r"received:\s*(?P<received>\d+)\s*\}"
)


def utc_stamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def parse_kv(match: re.Match[str]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for key, value in match.groupdict().items():
        if value is None:
            continue
        if value in ("true", "false"):
            out[key] = value == "true"
        else:
            out[key] = int(value)
    return out


# Record kinds of the binary opportunity journal that formed the former opportunities.jsonl
# (records, opportunities, reconcile rows); the daily summary (103) was a separate file.
RECORD_KINDS = {100, 101, 102}


def journal_frames(path: Path):
    """Frames of a binary journal through the Rust reader (`p0-journal frames`); the format
    is never decoded a second time here. Yields (kind, value)."""
    project_dir = Path(__file__).resolve().parents[2]
    binary = project_dir / "native" / "target" / "release" / "p0-journal"
    if not binary.exists():
        subprocess.run(
            ["cargo", "build", "--release", "--locked", "--manifest-path",
             str(project_dir / "native" / "Cargo.toml"), "--features", "crate_health txv1", "--bin", "p0-journal"],
            check=True,
        )
    proc = subprocess.run([str(binary), "frames", str(path)], capture_output=True, text=True, check=False)
    if proc.returncode:
        raise RuntimeError(f"journal unreadable: {proc.stderr.strip()[-300:]}")
    for line in proc.stdout.splitlines():
        frame = json.loads(line)
        yield frame["kind"], frame["value"]


def count_jsonl(path: Path) -> dict[str, int]:
    """Counts over the record journal `path` (binary, PERSIST-01). A missing journal stays
    visible as `journal_present: false` instead of silently looking like an empty run."""
    counts = {
        "journal_present": path.exists(),
        "lines": 0,
        "oracle_threshold_records": 0,
        "integration_projection_records": 0,
        "projection_ready": 0,
        "projection_failed": 0,
        "projection_stale": 0,
    }
    if not path.exists():
        return counts
    for kind, obj in journal_frames(path):
        if kind not in RECORD_KINDS or not isinstance(obj, dict):
            continue
        counts["lines"] += 1
        if obj.get("trigger") == "oracle_threshold":
            counts["oracle_threshold_records"] += 1
        if obj.get("trigger") == "integration_projection":
            counts["integration_projection_records"] += 1
        projection = obj.get("projection")
        if isinstance(projection, dict):
            status = projection.get("status")
            if status == "ready":
                counts["projection_ready"] += 1
            elif status == "failed":
                counts["projection_failed"] += 1
            elif status == "stale":
                counts["projection_stale"] += 1
    return counts


def analyze(log_path: Path, record_path: Path, run_meta: dict[str, Any]) -> dict[str, Any]:
    threshold_samples: list[dict[str, Any]] = []
    stats_samples: list[dict[str, Any]] = []
    channel_samples: list[dict[str, Any]] = []
    line_counts = {
        "bank_projection_pending_lines": 0,
        "bank_projection_coalesced_lines": 0,
        "bank_projection_fallback_lines": 0,
        "threshold_bootstrap_fallback_lines": 0,
        "program_upgrade_halt_lines": 0,
        "sink_overflow_lines": 0,
    }

    if log_path.exists():
        with log_path.open("r", encoding="utf-8", errors="replace") as fh:
            for line in fh:
                if m := THRESHOLD_RE.search(line):
                    threshold_samples.append(parse_kv(m))
                if m := STATS_RE.search(line):
                    stats_samples.append(parse_kv(m))
                if m := CHANNEL_RE.search(line):
                    channel_samples.append(parse_kv(m))
                if "oracle_bank_projection_pending:" in line:
                    line_counts["bank_projection_pending_lines"] += 1
                if "oracle_bank_projection_coalesced:" in line:
                    line_counts["bank_projection_coalesced_lines"] += 1
                if "oracle_bank_projection_fallback:" in line:
                    line_counts["bank_projection_fallback_lines"] += 1
                if "threshold_bootstrap_fallback:" in line:
                    line_counts["threshold_bootstrap_fallback_lines"] += 1
                if "PROGRAM_UPGRADE HALT" in line:
                    line_counts["program_upgrade_halt_lines"] += 1
                if "ingress protocol: sink overflow" in line:
                    line_counts["sink_overflow_lines"] += 1

    latest_stats = stats_samples[-1] if stats_samples else {}
    latest_threshold = threshold_samples[-1] if threshold_samples else {}
    max_overflow = max((s["overflow"] for s in channel_samples), default=0)
    max_pending_observed = max((s["max_pending"] for s in stats_samples), default=None)
    final_pending = latest_stats.get("pending")
    dep_updates = latest_stats.get("dependency_updates", 0)
    candidate_hits = latest_stats.get("candidate_hits", 0)
    candidates_per_update = (
        candidate_hits / dep_updates if dep_updates else None
    )
    projection_submitted = latest_stats.get("bank_projection_submitted", 0)
    projection_ready = latest_stats.get("bank_projection_ready", 0)
    projection_failed = latest_stats.get("bank_projection_failed", 0)
    projection_stale = latest_stats.get("bank_projection_stale", 0)
    projection_terminal = projection_ready + projection_failed + projection_stale
    projection_ready_ratio = (
        projection_ready / projection_terminal if projection_terminal else None
    )

    jsonl = count_jsonl(record_path)

    reasons: list[str] = []
    verdict = "PASS"

    if run_meta.get("exit_reason") in {"startup_timeout", "early_exit", "sink_overflow"}:
        verdict = "FAIL"
        reasons.append(f"runner ended before qualification window: {run_meta.get('exit_reason')}")
    if not latest_threshold:
        verdict = "FAIL"
        reasons.append("threshold_index_ready was not observed")
    if line_counts["program_upgrade_halt_lines"]:
        verdict = "FAIL"
        reasons.append("program upgrade halt observed")
    if max_overflow > 0:
        verdict = "FAIL"
        reasons.append(f"ingress overflow observed: {max_overflow}")
    if line_counts["sink_overflow_lines"]:
        verdict = "FAIL"
        reasons.append(
            f"WSS sink overflow/reconnect observed: {line_counts['sink_overflow_lines']}"
        )

    if verdict != "FAIL":
        if not stats_samples or dep_updates == 0:
            verdict = "PARTIAL"
            reasons.append("no Oracle dependency-update statistics observed")
        elif max_pending_observed is None:
            verdict = "PARTIAL"
            reasons.append("no max_pending measurement observed")
        elif max_pending_observed >= BASELINE_MAX_PENDING:
            verdict = "FAIL"
            reasons.append(
                f"max_pending {max_pending_observed} did not improve R125b baseline "
                f"{BASELINE_MAX_PENDING}"
            )
        elif final_pending is not None and final_pending != 0:
            verdict = "PARTIAL"
            reasons.append(f"Oracle queue not drained at final sample: pending={final_pending}")

        # R126's specific optimization is only proven if at least one stale Bank used the
        # projection path and at least one such projection produced a usable fresh price.
        if projection_submitted == 0:
            if verdict == "PASS":
                verdict = "PARTIAL"
            reasons.append("no stale integration Bank projection was exercised")
        elif projection_ready == 0:
            if verdict == "PASS":
                verdict = "PARTIAL"
            reasons.append(
                "Bank projection path was exercised but produced no ready projected price"
            )

    if verdict == "PASS" and not reasons:
        reasons.append(
            "threshold ready; Oracle updates observed; ingress overflow=0; queue drained; "
            "max_pending improved versus R125b; stale Bank projection produced ready pricing"
        )

    max_pending_improvement = None
    if max_pending_observed is not None:
        max_pending_improvement = BASELINE_MAX_PENDING - max_pending_observed

    return {
        "run": "R126",
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "verdict": verdict,
        "reasons": reasons,
        "run_meta": run_meta,
        "baseline_r125b": {
            "max_pending": BASELINE_MAX_PENDING,
            "dependency_updates": BASELINE_DEPENDENCY_UPDATES,
            "candidate_hits": BASELINE_CANDIDATE_HITS,
            "candidates_per_update": BASELINE_CANDIDATE_HITS / BASELINE_DEPENDENCY_UPDATES,
        },
        "threshold": latest_threshold,
        "oracle": {
            "latest": latest_stats,
            "samples": len(stats_samples),
            "max_pending_observed": max_pending_observed,
            "max_pending_improvement_abs": max_pending_improvement,
            "candidates_per_update": candidates_per_update,
        },
        "bank_projection": {
            "submitted": projection_submitted,
            "ready": projection_ready,
            "failed": projection_failed,
            "stale": projection_stale,
            "terminal_ready_ratio": projection_ready_ratio,
            **line_counts,
        },
        "ingress": {
            "samples": len(channel_samples),
            "max_overflow": max_overflow,
            "latest": channel_samples[-1] if channel_samples else {},
        },
        "records": jsonl,
    }


def human_report(report: dict[str, Any]) -> str:
    o = report["oracle"]
    bp = report["bank_projection"]
    ingress = report["ingress"]
    threshold = report["threshold"]
    latest = o["latest"]
    ratio = bp["terminal_ready_ratio"]
    ratio_text = "n/a" if ratio is None else f"{ratio:.1%}"
    cpu = o["candidates_per_update"]
    cpu_text = "n/a" if cpu is None else f"{cpu:.1f}"
    improvement = o["max_pending_improvement_abs"]
    improvement_text = "n/a" if improvement is None else str(improvement)

    lines = [
        "R126 AUTOMATIC QUALIFICATION",
        f"verdict={report['verdict']}",
        f"reason={' | '.join(report['reasons'])}",
        "",
        f"threshold_ready={bool(threshold)} tracked={threshold.get('tracked', 'n/a')} "
        f"fallback={threshold.get('fallback', 'n/a')}",
        f"dependency_updates={latest.get('dependency_updates', 'n/a')}",
        f"candidate_hits={latest.get('candidate_hits', 'n/a')}",
        f"candidates_per_update={cpu_text}",
        f"pending_final={latest.get('pending', 'n/a')}",
        f"max_pending={o['max_pending_observed']} "
        f"(R125b={BASELINE_MAX_PENDING}, improvement={improvement_text})",
        f"price_fallbacks={latest.get('price_fallbacks', 'n/a')}",
        f"coalesced={latest.get('coalesced', 'n/a')}",
        "",
        f"bank_projection_submitted={bp['submitted']}",
        f"bank_projection_ready={bp['ready']}",
        f"bank_projection_failed={bp['failed']}",
        f"bank_projection_stale={bp['stale']}",
        f"bank_projection_ready_ratio={ratio_text}",
        f"bank_projection_fallback_lines={bp['bank_projection_fallback_lines']}",
        "",
        f"ingress_max_overflow={ingress['max_overflow']}",
        f"sink_overflow_lines={bp['sink_overflow_lines']}",
        f"ingress_backlog_final={latest.get('ingress_backlog', 'n/a')}",
        f"oracle_threshold_records={report['records']['oracle_threshold_records']}",
        f"terminal_log={report['run_meta']['log_path']}",
        f"evidence_json={report['run_meta']['report_json_path']}",
    ]
    return "\n".join(lines) + "\n"


def stop_group(proc: subprocess.Popen[str], grace_seconds: float = 15.0) -> None:
    if proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGINT)
    except ProcessLookupError:
        return
    deadline = time.monotonic() + grace_seconds
    while proc.poll() is None and time.monotonic() < deadline:
        time.sleep(0.1)
    if proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    deadline = time.monotonic() + 5.0
    while proc.poll() is None and time.monotonic() < deadline:
        time.sleep(0.1)
    if proc.poll() is None:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run and automatically evaluate the R126 Mainnet Shadow qualification."
    )
    parser.add_argument(
        "--observe-minutes",
        type=float,
        default=15.0,
        help="minutes to observe AFTER threshold_index_ready (default: 15)",
    )
    parser.add_argument(
        "--startup-timeout-minutes",
        type=float,
        default=12.0,
        help="maximum minutes allowed to reach threshold_index_ready (default: 12)",
    )
    parser.add_argument(
        "--bootstrap-batch",
        type=int,
        default=16,
        help="P0_THRESHOLD_BOOTSTRAP_BATCH (default: 16)",
    )
    args = parser.parse_args()

    if args.observe_minutes <= 0 or args.startup_timeout_minutes <= 0:
        parser.error("durations must be > 0")
    if args.bootstrap_batch <= 0:
        parser.error("--bootstrap-batch must be > 0")

    project_dir = Path(__file__).resolve().parents[2]
    repo_root = project_dir.parent
    shadow_dir = project_dir / "data" / "shadow"
    shadow_dir.mkdir(parents=True, exist_ok=True)

    stamp = utc_stamp()
    prefix = shadow_dir / f"r126_{stamp}"
    log_path = Path(f"{prefix}_terminal.log")
    # Binary record journal; it also holds the daily summary (kind 103).
    record_path = Path(f"{prefix}_opportunities.p0j")
    report_json_path = Path(f"{prefix}_report.json")
    report_txt_path = Path(f"{prefix}_report.txt")

    env = os.environ.copy()
    env["P0_RECORD_PATH"] = str(record_path.relative_to(repo_root))
    env["P0_THRESHOLD_BOOTSTRAP_BATCH"] = str(args.bootstrap_batch)

    command = [str(project_dir / "scripts" / "capture.sh"), "shadow"]
    ready_event = threading.Event()
    overflow_event = threading.Event()
    stop_reader = threading.Event()

    with log_path.open("w", encoding="utf-8", buffering=1) as log:
        proc = subprocess.Popen(
            command,
            cwd=repo_root,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
            start_new_session=True,
        )

        def reader() -> None:
            assert proc.stdout is not None
            for line in proc.stdout:
                sys.stdout.write(line)
                sys.stdout.flush()
                log.write(line)
                if "threshold_index_ready:" in line:
                    ready_event.set()
                if "ingress protocol: sink overflow" in line:
                    overflow_event.set()
            stop_reader.set()

        thread = threading.Thread(target=reader, name="r126-log-reader", daemon=True)
        thread.start()

        started = time.monotonic()
        startup_deadline = started + args.startup_timeout_minutes * 60.0
        exit_reason = "unknown"
        ready_at: float | None = None

        try:
            while proc.poll() is None and not ready_event.is_set():
                if overflow_event.is_set():
                    exit_reason = "sink_overflow"
                    print("\n[R126] sink overflow detected; stopping immediately")
                    stop_group(proc)
                    break
                if time.monotonic() >= startup_deadline:
                    exit_reason = "startup_timeout"
                    stop_group(proc)
                    break
                time.sleep(0.2)

            if ready_event.is_set() and proc.poll() is None:
                ready_at = time.monotonic()
                observation_seconds = args.observe_minutes * 60.0
                print(
                    f"\n[R126] threshold ready; automatic observation window "
                    f"{args.observe_minutes:g} min started"
                )
                deadline = ready_at + observation_seconds
                while proc.poll() is None and time.monotonic() < deadline:
                    if overflow_event.is_set():
                        exit_reason = "sink_overflow"
                        print("\n[R126] sink overflow detected; stopping immediately")
                        stop_group(proc)
                        break
                    time.sleep(0.2)
                if proc.poll() is None and not overflow_event.is_set():
                    exit_reason = "observation_complete"
                    print("\n[R126] observation complete; sending SIGINT to Shadow process group")
                    stop_group(proc)
                else:
                    exit_reason = "early_exit"
            elif proc.poll() is not None and exit_reason == "unknown":
                exit_reason = "early_exit"
        except KeyboardInterrupt:
            exit_reason = "user_interrupt"
            print("\n[R126] interrupted by user; stopping process group and evaluating partial run")
            stop_group(proc)
        finally:
            if proc.poll() is None:
                stop_group(proc)
            thread.join(timeout=5.0)

    return_code = proc.poll()
    run_meta = {
        "stamp": stamp,
        "observe_minutes_requested": args.observe_minutes,
        "startup_timeout_minutes": args.startup_timeout_minutes,
        "threshold_ready_observed": ready_event.is_set(),
        "observation_seconds_actual": (
            None if ready_at is None else max(0.0, time.monotonic() - ready_at)
        ),
        "exit_reason": exit_reason,
        "process_return_code": return_code,
        "log_path": str(log_path.relative_to(repo_root)),
        "record_path": str(record_path.relative_to(repo_root)),
        "report_json_path": str(report_json_path.relative_to(repo_root)),
        "report_txt_path": str(report_txt_path.relative_to(repo_root)),
    }

    report = analyze(log_path, record_path, run_meta)
    report_json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    text_report = human_report(report)
    report_txt_path.write_text(text_report, encoding="utf-8")

    print("\n" + "=" * 72)
    print(text_report, end="")
    print("=" * 72)
    print(
        f"Send me only this compact report if you want review:\n"
        f"  cat {report_txt_path.relative_to(repo_root)}"
    )

    return 0 if report["verdict"] in {"PASS", "PARTIAL"} else 3


if __name__ == "__main__":
    raise SystemExit(main())

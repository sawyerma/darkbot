#!/usr/bin/env python3
"""Timed R127 Long-Tail qualification for the affected-Bank exact-health sweep."""

from __future__ import annotations
import argparse, json, os, re, signal, subprocess, sys, threading, time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

R126B_MAX_PENDING = 78214

CHANNEL_RE = re.compile(
    r'channel=ChannelStats\s*\{\s*name:\s*"ingress",\s*'
    r"capacity:\s*(?P<capacity>\d+),\s*sent:\s*(?P<sent>\d+),\s*"
    r"overflow:\s*(?P<overflow>\d+),\s*received:\s*(?P<received>\d+)\s*\}"
)

def stamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")

def parse_stats(line: str) -> dict[str, Any]:
    if "oracle_trigger_stats:" not in line:
        return {}
    tail = line.split("oracle_trigger_stats:", 1)[1].strip()
    out: dict[str, Any] = {}
    for token in tail.split():
        if "=" not in token:
            continue
        k, v = token.split("=", 1)
        if v in ("true", "false"):
            out[k] = v == "true"
        else:
            try:
                out[k] = int(v)
            except ValueError:
                out[k] = v
    return out

def stop_group(proc: subprocess.Popen[str]) -> None:
    if proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGINT)
    except ProcessLookupError:
        return
    deadline = time.monotonic() + 15
    while proc.poll() is None and time.monotonic() < deadline:
        time.sleep(0.1)
    if proc.poll() is None:
        try:
            os.killpg(proc.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass

def analyze(log_path: Path, meta: dict[str, Any]) -> dict[str, Any]:
    stats: list[dict[str, Any]] = []
    channels: list[dict[str, int]] = []
    ready = 0
    sink_overflow = 0
    projection_pending = 0
    projection_fallback = 0
    with log_path.open("r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            if "r127_health_sweep_ready:" in line:
                ready += 1
            if "ingress protocol: sink overflow" in line:
                sink_overflow += 1
            if "oracle_bank_projection_pending:" in line:
                projection_pending += 1
            if "oracle_bank_projection_fallback:" in line:
                projection_fallback += 1
            s = parse_stats(line)
            if s:
                stats.append(s)
            m = CHANNEL_RE.search(line)
            if m:
                channels.append({k: int(v) for k, v in m.groupdict().items()})

    latest = stats[-1] if stats else {}
    max_pending = max((int(s.get("max_pending", 0)) for s in stats), default=0)
    max_ingress_backlog = max((int(s.get("ingress_backlog", 0)) for s in stats), default=0)
    max_overflow = max((s["overflow"] for s in channels), default=0)
    checks = int(latest.get("sweep_checks", 0))
    healthy = int(latest.get("sweep_healthy", 0))
    liquidatable = int(latest.get("sweep_liquidatable", 0))
    errors = int(latest.get("sweep_errors", 0))
    filtered_ratio = healthy / checks if checks else None
    final_pending = int(latest.get("pending", 0)) if latest else None
    final_backlog = int(latest.get("ingress_backlog", 0)) if latest else None
    forwarded = int(latest.get("sweep_forwarded", 0))
    batches = int(latest.get("sweep_batches", 0))
    refs = int(latest.get("sweep_account_refs", 0))
    unique_accounts = int(latest.get("sweep_unique_accounts", 0))
    max_unique_batch = int(latest.get("sweep_max_unique_batch", 0))
    max_dirty_banks = int(latest.get("sweep_max_dirty_banks", 0))
    batch_dedupe_ratio = (1.0 - unique_accounts / refs) if refs else None
    cache_entries = int(latest.get("candidate_cache_entries", 0))
    cache_suppressed = int(latest.get("candidate_cache_suppressed", 0))
    cache_softcap_suppressed = int(latest.get("candidate_cache_softcap_suppressed", 0))
    cache_transition_forwards = int(latest.get("candidate_cache_transition_forwards", 0))
    cache_revision_forwards = int(latest.get("candidate_cache_revision_forwards", 0))
    candidate_pending_coalesced = int(latest.get("candidate_pending_coalesced", 0))
    candidate_revision_coalesced = int(latest.get("candidate_revision_coalesced", 0))
    candidate_physical_enqueues = int(latest.get("candidate_physical_enqueues", 0))
    cache_periodic_forwards = int(latest.get("candidate_cache_periodic_forwards", 0))
    cache_forced_forwards = int(latest.get("candidate_cache_forced_forwards", 0))
    repeat_wait_slots_max = int(latest.get("candidate_repeat_wait_slots_max", 0))
    repeat_wait_slots_1 = int(latest.get("candidate_repeat_wait_slots_1", 0))
    repeat_wait_slots_2_3 = int(latest.get("candidate_repeat_wait_slots_2_3", 0))
    repeat_wait_slots_4_7 = int(latest.get("candidate_repeat_wait_slots_4_7", 0))
    repeat_wait_slots_8_15 = int(latest.get("candidate_repeat_wait_slots_8_15", 0))
    repeat_wait_slots_ge_16 = int(latest.get("candidate_repeat_wait_slots_ge_16", 0))
    fairness_rotations = int(latest.get("candidate_fairness_rotations", 0))
    periodic_due_but_deferred = int(latest.get("candidate_periodic_due_but_deferred", 0))
    dependency_content_unchanged = int(latest.get("dependency_content_unchanged", 0))
    dependency_content_changed = int(latest.get("dependency_content_changed", 0))
    detect_health_calls = int(latest.get("detect_health_calls", 0))
    detect_health_ns_total = int(latest.get("detect_health_ns_total", 0))
    detect_health_ns_max = int(latest.get("detect_health_ns_max", 0))
    detect_pipeline_calls = int(latest.get("detect_pipeline_calls", 0))
    detect_pipeline_ns_total = int(latest.get("detect_pipeline_ns_total", 0))
    detect_pipeline_ns_max = int(latest.get("detect_pipeline_ns_max", 0))
    pipeline_planning_calls = int(latest.get("pipeline_planning_calls", 0))
    pipeline_planning_ns_total = int(latest.get("pipeline_planning_ns_total", 0))
    pipeline_planning_ns_max = int(latest.get("pipeline_planning_ns_max", 0))
    pipeline_fee_calls = int(latest.get("pipeline_fee_calls", 0))
    pipeline_fee_ns_total = int(latest.get("pipeline_fee_ns_total", 0))
    pipeline_fee_ns_max = int(latest.get("pipeline_fee_ns_max", 0))
    pipeline_simulation_calls = int(latest.get("pipeline_simulation_calls", 0))
    pipeline_simulation_ns_total = int(latest.get("pipeline_simulation_ns_total", 0))
    pipeline_simulation_ns_max = int(latest.get("pipeline_simulation_ns_max", 0))
    cache_total_decisions = cache_suppressed + forwarded
    cache_suppression_ratio = (
        cache_suppressed / cache_total_decisions if cache_total_decisions else None
    )

    verdict = "PASS"
    reasons: list[str] = []
    if meta["exit_reason"] in {"startup_timeout", "early_exit", "sink_overflow"}:
        verdict = "FAIL"
        reasons.append(f"runner exit={meta['exit_reason']}")
    if ready == 0:
        verdict = "FAIL"
        reasons.append("r127_health_sweep_ready not observed")
    if sink_overflow or max_overflow:
        verdict = "FAIL"
        reasons.append(f"ingress overflow: sink_lines={sink_overflow} counter={max_overflow}")
    if not stats or int(latest.get("dependency_updates", 0)) == 0:
        verdict = "FAIL"
        reasons.append("no Oracle/bank dependency updates were recognized; trigger coverage unproven")
    if checks == 0:
        if verdict != "FAIL":
            verdict = "PARTIAL"
        reasons.append("health sweep not exercised")
    if bool(latest.get("threshold_trigger", True)):
        verdict = "FAIL"
        reasons.append("legacy threshold trigger unexpectedly enabled")
    if batches == 0:
        verdict = "FAIL"
        reasons.append("dependency batching was not exercised")
    if refs > 0 and unique_accounts >= refs:
        if verdict != "FAIL":
            verdict = "PARTIAL"
        reasons.append("batching produced no account-level deduplication")
    if max_pending >= R126B_MAX_PENDING:
        verdict = "FAIL"
        reasons.append(f"max_pending={max_pending} did not improve R126b={R126B_MAX_PENDING}")
    elif max_pending > 10000:
        if verdict != "FAIL":
            verdict = "PARTIAL"
        reasons.append(f"max_pending still high after batching: {max_pending}")
    if final_pending is not None and final_pending > 1000:
        if verdict != "FAIL":
            verdict = "PARTIAL"
        reasons.append(f"queue still materially backlogged at stop: pending={final_pending}")
    if final_backlog is not None and final_backlog > 1000:
        verdict = "FAIL"
        reasons.append(f"ingress backlog too high: {final_backlog}")
    if cache_entries == 0:
        verdict = "FAIL"
        reasons.append("candidate cache was not populated")
    # After FIX-134 every causal revision is correctness work and may legitimately
    # make logical cache suppression near zero. The production fanout gate is now
    # whether those revision-forwards collapse into bounded physical queue tokens.
    if cache_suppressed == 0 and candidate_pending_coalesced == 0:
        if verdict != "FAIL":
            verdict = "PARTIAL"
        reasons.append("neither candidate cache nor candidate single-flight coalescing reduced repeated work")
    required_fix135_keys = {
        "candidate_cache_revision_forwards",
        "candidate_pending_coalesced",
        "candidate_revision_coalesced",
        "candidate_physical_enqueues",
        "candidate_repeat_wait_slots_max",
        "candidate_fairness_rotations",
        "candidate_periodic_due_but_deferred",
    }
    missing_fix135_keys = sorted(k for k in required_fix135_keys if k not in latest)
    if missing_fix135_keys:
        verdict = "FAIL"
        reasons.append("missing FIX-134/135 telemetry: " + ",".join(missing_fix135_keys))
    if cache_transition_forwards == 0 and cache_forced_forwards == 0 and cache_revision_forwards == 0:
        if verdict != "FAIL":
            verdict = "PARTIAL"
        reasons.append("no candidate transition/forced/revision forwarding observed")
    if verdict == "PASS" and not reasons:
        reasons.append("lossless ingress; batching active; candidate cache suppresses repeats; downstream queue bounded and draining")

    return {
        "run": "R127",
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "verdict": verdict,
        "reasons": reasons,
        "meta": meta,
        "latest": latest,
        "samples": len(stats),
        "max_pending": max_pending,
        "r126b_max_pending_baseline": R126B_MAX_PENDING,
        "max_ingress_backlog": max_ingress_backlog,
        "sink_overflow_lines": sink_overflow,
        "ingress_overflow_counter": max_overflow,
        "sweep": {
            "checks": checks,
            "healthy": healthy,
            "liquidatable": liquidatable,
            "errors": errors,
            "healthy_filter_ratio": filtered_ratio,
            "forwarded": forwarded,
            "batches": batches,
            "account_refs": refs,
            "unique_accounts": unique_accounts,
            "batch_dedupe_ratio": batch_dedupe_ratio,
            "max_unique_batch": max_unique_batch,
            "max_dirty_banks": max_dirty_banks,
            "candidate_cache_entries": cache_entries,
            "candidate_cache_suppressed": cache_suppressed,
            "candidate_cache_softcap_suppressed": cache_softcap_suppressed,
            "candidate_cache_transition_forwards": cache_transition_forwards,
            "candidate_cache_revision_forwards": cache_revision_forwards,
            "candidate_pending_coalesced": candidate_pending_coalesced,
            "candidate_revision_coalesced": candidate_revision_coalesced,
            "candidate_physical_enqueues": candidate_physical_enqueues,
            "candidate_cache_periodic_forwards": cache_periodic_forwards,
            "candidate_cache_forced_forwards": cache_forced_forwards,
            "candidate_repeat_wait_slots_max": repeat_wait_slots_max,
            "candidate_repeat_wait_slots_1": repeat_wait_slots_1,
            "candidate_repeat_wait_slots_2_3": repeat_wait_slots_2_3,
            "candidate_repeat_wait_slots_4_7": repeat_wait_slots_4_7,
            "candidate_repeat_wait_slots_8_15": repeat_wait_slots_8_15,
            "candidate_repeat_wait_slots_ge_16": repeat_wait_slots_ge_16,
            "candidate_fairness_rotations": fairness_rotations,
            "candidate_periodic_due_but_deferred": periodic_due_but_deferred,
            "dependency_content_unchanged": dependency_content_unchanged,
            "dependency_content_changed": dependency_content_changed,
            "detect_health_calls": detect_health_calls,
            "detect_health_ns_total": detect_health_ns_total,
            "detect_health_ns_max": detect_health_ns_max,
            "detect_pipeline_calls": detect_pipeline_calls,
            "detect_pipeline_ns_total": detect_pipeline_ns_total,
            "detect_pipeline_ns_max": detect_pipeline_ns_max,
            "pipeline_planning_calls": pipeline_planning_calls,
            "pipeline_planning_ns_total": pipeline_planning_ns_total,
            "pipeline_planning_ns_max": pipeline_planning_ns_max,
            "pipeline_fee_calls": pipeline_fee_calls,
            "pipeline_fee_ns_total": pipeline_fee_ns_total,
            "pipeline_fee_ns_max": pipeline_fee_ns_max,
            "pipeline_simulation_calls": pipeline_simulation_calls,
            "pipeline_simulation_ns_total": pipeline_simulation_ns_total,
            "pipeline_simulation_ns_max": pipeline_simulation_ns_max,
            "candidate_cache_suppression_ratio": cache_suppression_ratio,
        },
        "legacy_bank_projection_lines": {
            "pending": projection_pending,
            "fallback": projection_fallback,
        },
    }

def render(r: dict[str, Any]) -> str:
    s=r["sweep"]; latest=r["latest"]
    ratio="n/a" if s["healthy_filter_ratio"] is None else f"{s['healthy_filter_ratio']:.1%}"
    dedupe="n/a" if s["batch_dedupe_ratio"] is None else f"{s['batch_dedupe_ratio']:.1%}"
    cache_ratio="n/a" if s["candidate_cache_suppression_ratio"] is None else f"{s['candidate_cache_suppression_ratio']:.1%}"
    return "\n".join([
        "R127 AUTOMATIC QUALIFICATION",
        f"verdict={r['verdict']}",
        f"reason={' | '.join(r['reasons'])}",
        "",
        f"threshold_trigger={latest.get('threshold_trigger','n/a')}",
        f"dependency_updates={latest.get('dependency_updates','n/a')}",
        f"sweep_checks={s['checks']}",
        f"sweep_healthy={s['healthy']}",
        f"sweep_liquidatable={s['liquidatable']}",
        f"sweep_errors={s['errors']}",
        f"healthy_filter_ratio={ratio}",
        f"sweep_batches={s['batches']}",
        f"sweep_account_refs={s['account_refs']}",
        f"sweep_unique_accounts={s['unique_accounts']}",
        f"batch_dedupe_ratio={dedupe}",
        f"sweep_forwarded={s['forwarded']}",
        f"sweep_max_unique_batch={s['max_unique_batch']}",
        f"sweep_max_dirty_banks={s['max_dirty_banks']}",
        f"candidate_cache_entries={s['candidate_cache_entries']}",
        f"candidate_cache_suppressed={s['candidate_cache_suppressed']}",
        f"candidate_cache_suppression_ratio={cache_ratio}",
        f"candidate_cache_softcap_suppressed={s['candidate_cache_softcap_suppressed']}",
        f"candidate_cache_transition_forwards={s['candidate_cache_transition_forwards']}",
        f"candidate_cache_revision_forwards={s['candidate_cache_revision_forwards']}",
        f"candidate_pending_coalesced={s['candidate_pending_coalesced']}",
        f"candidate_revision_coalesced={s['candidate_revision_coalesced']}",
        f"candidate_physical_enqueues={s['candidate_physical_enqueues']}",
        f"candidate_cache_periodic_forwards={s['candidate_cache_periodic_forwards']}",
        f"candidate_cache_forced_forwards={s['candidate_cache_forced_forwards']}",
        f"candidate_repeat_wait_slots_max={s['candidate_repeat_wait_slots_max']}",
        f"candidate_repeat_wait_slots_1={s['candidate_repeat_wait_slots_1']}",
        f"candidate_repeat_wait_slots_2_3={s['candidate_repeat_wait_slots_2_3']}",
        f"candidate_repeat_wait_slots_4_7={s['candidate_repeat_wait_slots_4_7']}",
        f"candidate_repeat_wait_slots_8_15={s['candidate_repeat_wait_slots_8_15']}",
        f"candidate_repeat_wait_slots_ge_16={s['candidate_repeat_wait_slots_ge_16']}",
        f"candidate_fairness_rotations={s['candidate_fairness_rotations']}",
        f"candidate_periodic_due_but_deferred={s['candidate_periodic_due_but_deferred']}",
        f"dependency_content_unchanged={s['dependency_content_unchanged']}",
        f"dependency_content_changed={s['dependency_content_changed']}",
        f"detect_health_calls={s['detect_health_calls']}",
        f"detect_health_ns_total={s['detect_health_ns_total']}",
        f"detect_health_ns_max={s['detect_health_ns_max']}",
        f"detect_pipeline_calls={s['detect_pipeline_calls']}",
        f"detect_pipeline_ns_total={s['detect_pipeline_ns_total']}",
        f"detect_pipeline_ns_max={s['detect_pipeline_ns_max']}",
        f"pipeline_planning_calls={s['pipeline_planning_calls']}",
        f"pipeline_planning_ns_total={s['pipeline_planning_ns_total']}",
        f"pipeline_planning_ns_max={s['pipeline_planning_ns_max']}",
        f"pipeline_fee_calls={s['pipeline_fee_calls']}",
        f"pipeline_fee_ns_total={s['pipeline_fee_ns_total']}",
        f"pipeline_fee_ns_max={s['pipeline_fee_ns_max']}",
        f"pipeline_simulation_calls={s['pipeline_simulation_calls']}",
        f"pipeline_simulation_ns_total={s['pipeline_simulation_ns_total']}",
        f"pipeline_simulation_ns_max={s['pipeline_simulation_ns_max']}",
        "",
        f"pending_final={latest.get('pending','n/a')}",
        f"max_pending={r['max_pending']} (R126b={R126B_MAX_PENDING})",
        f"coalesced={latest.get('coalesced','n/a')}",
        f"ingress_backlog_final={latest.get('ingress_backlog','n/a')}",
        f"ingress_backlog_max={r['max_ingress_backlog']}",
        f"ingress_overflow_counter={r['ingress_overflow_counter']}",
        f"sink_overflow_lines={r['sink_overflow_lines']}",
        "",
        f"terminal_log={r['meta']['log_path']}",
        f"evidence_json={r['meta']['report_json_path']}",
    ])+"\n"

def main() -> int:
    ap=argparse.ArgumentParser()
    ap.add_argument("--observe-minutes", type=float, default=10.0)
    ap.add_argument("--startup-timeout-minutes", type=float, default=12.0)
    args=ap.parse_args()
    if args.observe_minutes <= 0 or args.startup_timeout_minutes <= 0:
        ap.error("durations must be > 0")

    project_dir=Path(__file__).resolve().parents[2]
    root=project_dir.parent
    out=project_dir/"data"/"shadow"; out.mkdir(parents=True, exist_ok=True)
    st=stamp(); prefix=out/f"r127_{st}"
    log=Path(f"{prefix}_terminal.log")
    record=Path(f"{prefix}_opportunities.jsonl")
    summary=Path(f"{prefix}_summary.jsonl")
    report_json=Path(f"{prefix}_report.json")
    report_txt=Path(f"{prefix}_report.txt")

    env=os.environ.copy()
    env["P0_RECORD_PATH"]=str(record.relative_to(root))
    env["P0_SUMMARY_PATH"]=str(summary.relative_to(root))
    env["P0_USE_THRESHOLD_TRIGGER"]="false"

    ready=threading.Event()
    overflow=threading.Event()
    proc=subprocess.Popen(
        [str(project_dir/"scripts"/"capture.sh"),"shadow"], cwd=root, env=env,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1,
        start_new_session=True,
    )
    with log.open("w",encoding="utf-8",buffering=1) as fh:
        def reader() -> None:
            assert proc.stdout is not None
            for line in proc.stdout:
                sys.stdout.write(line); sys.stdout.flush(); fh.write(line)
                if "r127_health_sweep_ready:" in line: ready.set()
                if "ingress protocol: sink overflow" in line: overflow.set()
        th=threading.Thread(target=reader,daemon=True); th.start()
        exit_reason="unknown"; ready_at=None
        try:
            startup_deadline=time.monotonic()+args.startup_timeout_minutes*60
            while proc.poll() is None and not ready.is_set():
                if overflow.is_set():
                    exit_reason="sink_overflow"; stop_group(proc); break
                if time.monotonic()>=startup_deadline:
                    exit_reason="startup_timeout"; stop_group(proc); break
                time.sleep(.2)
            if ready.is_set() and proc.poll() is None:
                ready_at=time.monotonic()
                print(f"\n[R127] health sweep ready; observing {args.observe_minutes:g} min")
                deadline=ready_at+args.observe_minutes*60
                while proc.poll() is None and time.monotonic()<deadline:
                    if overflow.is_set():
                        exit_reason="sink_overflow"
                        print("\n[R127] sink overflow detected; stopping")
                        stop_group(proc); break
                    time.sleep(.2)
                if proc.poll() is None and not overflow.is_set():
                    exit_reason="observation_complete"
                    print("\n[R127] observation complete; stopping")
                    stop_group(proc)
                elif exit_reason=="unknown":
                    exit_reason="early_exit"
            elif proc.poll() is not None and exit_reason=="unknown":
                exit_reason="early_exit"
        except KeyboardInterrupt:
            exit_reason="user_interrupt"; stop_group(proc)
        finally:
            if proc.poll() is None: stop_group(proc)
            th.join(timeout=5)

    meta={
        "stamp":st,
        "exit_reason":exit_reason,
        "observe_minutes_requested":args.observe_minutes,
        "threshold_ready_observed":ready.is_set(),
        "observation_seconds_actual":None if ready_at is None else max(0,time.monotonic()-ready_at),
        "process_return_code":proc.poll(),
        "log_path":str(log.relative_to(root)),
        "record_path":str(record.relative_to(root)),
        "summary_path":str(summary.relative_to(root)),
        "report_json_path":str(report_json.relative_to(root)),
        "report_txt_path":str(report_txt.relative_to(root)),
    }
    rep=analyze(log,meta)
    report_json.write_text(json.dumps(rep,indent=2,sort_keys=True)+"\n",encoding="utf-8")
    txt=render(rep); report_txt.write_text(txt,encoding="utf-8")
    print("\n"+"="*72); print(txt,end=""); print("="*72)
    print(f"Send only: cat {report_txt.relative_to(root)}")
    return 0 if rep["verdict"] in {"PASS","PARTIAL"} else 3

if __name__=="__main__":
    raise SystemExit(main())

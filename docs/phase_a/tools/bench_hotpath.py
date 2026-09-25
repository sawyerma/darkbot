#!/usr/bin/env python3
"""Phase-A one-off wall-clock hot-path benchmark.

Exercises the PRODUCTION processing path with the real host clock: FeedAdapter (production, with the
production FakeWebSocket test transport injected through its `connect` parameter), LiveClock, LiveScheduler,
Recorder + SqliteWriter thread, PaperExecutionAdapter and StateOwner constructed exactly like Application.run().
Fee snapshots are published through the same StateOwner.on_fee_snapshots entry point Application uses.
Input: the frozen synthetic capture (feed_observations raw payloads), time-compressed by COMPRESS.
Metrics are read back from the persisted production telemetry (feed_observations, decisions, execution_attempts).

Usage: python3 bench_hotpath.py <repo_root> <source_sqlite> <source_run_id> <config_json> <slots> <compress> <output_sqlite> <result_json> <label>
"""
from __future__ import annotations

import asyncio
import hashlib
import json
import platform
import sqlite3
import statistics
import sys
import time
from pathlib import Path

ROOT = Path(sys.argv[1]).resolve()
SRC = Path(sys.argv[2]).resolve()
SRC_RUN = sys.argv[3]
CFG_PATH = Path(sys.argv[4]).resolve()
SLOTS = int(sys.argv[5])
COMPRESS = float(sys.argv[6])
OUT_DB = Path(sys.argv[7]).resolve()
RESULT = Path(sys.argv[8]).resolve()
LABEL = sys.argv[9]
sys.path.insert(0, str(ROOT))

from darkbot.capture import FakeWebSocket, FeedAdapter, fake_connect_factory  # noqa: E402
from darkbot.clock import LiveClock, LiveScheduler  # noqa: E402
from darkbot.config import load_runtime_config  # noqa: E402
from darkbot.domain import CostBasis, CostEvidence, FeeSnapshot, utc_iso  # noqa: E402
from darkbot.paper import PaperExecutionAdapter, StateOwner  # noqa: E402
from darkbot.storage import Recorder, SqliteWriter, apply_migrations, open_connection, upsert_config_snapshot  # noqa: E402

PROGRAM = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def fee_snapshots(cfg, run_id, clock, seq):
    rp = cfg.costs["snapshot_refresh_policy"]
    mono = clock.monotonic_ns()
    cu_price = 10_000
    prio = -(-(cfg.execution["priority_policy"]["compute_unit_limit"] * cu_price) // 1_000_000)

    def snap(kind, stype, method, norm, max_age_ms):
        return FeeSnapshot(f"{run_id}_fee_{kind}_{seq}", stype, "SYNTHETIC_FIXTURE", method, (), 1, utc_iso(clock.utc_ns()), mono, "processed", PROGRAM, cfg.protocol["idl_version_or_hash"], {"synthetic": True, "seq": seq}, norm, cfg.protocol["fee_rounding"], "SYNTHETIC", cfg.freshness_policy_hash, "VALID", cfg.cost_model_hash, max_age_ms * 1_000_000, CostEvidence.MEASURED)
    return [snap("protocol", "PROTOCOL_GLOBAL_ACCOUNT", "getAccountInfo", {"protocol_fee_bps": 100, "creator_fee_bps": 30, "other_protocol_fee_bps": 0}, rp["protocol_max_age_ms"]),
            snap("network", "NETWORK_FEE", "getFeeForMessage", {"base_fee_lamports": 5000, "priority_fee_lamports": prio, "network_fee_total_lamports": 5000 + prio}, rp["network_fee_max_age_ms"]),
            snap("rent", "RENT_EXEMPTION", "getMinimumBalanceForRentExemption", {"token_account_rent_lamports": 2_039_280, "account_size_bytes": 165}, rp["rent_max_age_ms"]),
            snap("priority", "PRIORITY_POLICY", "getRecentPrioritizationFees", {"compute_unit_price_micro_lamports": cu_price, "compute_unit_limit": cfg.execution["priority_policy"]["compute_unit_limit"]}, rp["network_fee_max_age_ms"])]


def pct(samples: list[int]) -> dict:
    if not samples:
        return {"count": 0}
    s = sorted(samples)
    n = len(s)

    def q(p: float) -> int:
        return s[min(n - 1, max(0, int(round(p * (n - 1)))))]
    return {"count": n, "min_ms": s[0] / 1e6, "p50_ms": q(0.50) / 1e6, "p95_ms": q(0.95) / 1e6, "p99_ms": q(0.99) / 1e6, "max_ms": s[-1] / 1e6, "mean_ms": statistics.fmean(s) / 1e6}


async def main_async() -> dict:
    cfg_base = load_runtime_config(CFG_PATH)
    cfg = cfg_base.with_overrides(portfolio=cfg_base.portfolio_limit_overrides(SLOTS))
    src = open_connection(SRC, read_only=True)
    src.row_factory = sqlite3.Row
    frames_by_feed: dict[str, list] = {}
    t_first = None
    for r in src.execute("SELECT receive_mono_ns, feed_id, raw_payload FROM feed_observations WHERE run_id=? AND raw_payload IS NOT NULL ORDER BY receive_mono_ns, arrival_sequence, feed_id", (SRC_RUN,)):
        if t_first is None:
            t_first = r["receive_mono_ns"]
        frames_by_feed.setdefault(r["feed_id"], []).append((r["receive_mono_ns"], r["raw_payload"]))
    src.close()
    # time-compressed pacing: per feed, sleep the compressed inter-arrival gap before each frame (production FakeWebSocket tuple form)
    scripted: dict[str, list] = {}
    for fid, lst in frames_by_feed.items():
        prev = t_first
        out = []
        for mono, payload in lst:
            gap = max(0.0, (mono - prev) / 1e9 / COMPRESS)
            out.append((gap, payload) if gap > 0 else payload)
            prev = mono
        scripted[fid] = out
    if OUT_DB.exists():
        raise SystemExit(f"output db must be new: {OUT_DB}")
    loop = asyncio.get_running_loop()
    clock = LiveClock()
    run_id = f"phase_a_bench_{LABEL}_{clock.epoch_id[-6:]}"
    conn = open_connection(OUT_DB)
    apply_migrations(conn)
    now_utc = clock.utc_iso()
    for kind, (h, content) in cfg.snapshot_kinds().items():
        upsert_config_snapshot(conn, kind, h, content, str(cfg.path), now_utc)
    conn.execute("INSERT INTO runs(run_id,campaign_id,mode,started_utc,code_revision,spec_hash,config_hash,protocol_snapshot_id,host_clock_epoch_id,status,quality_status,start_balance_lamports,readiness,started_mono_ns) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                 (run_id, "phase_a_bench", "PAPER", now_utc, "phase_a", "bench", cfg.config_hash, cfg.protocol_hash, clock.epoch_id, "RUNNING", "OK", cfg.portfolio["initial_paper_balance_lamports"], "NOT_READY", clock.monotonic_ns()))
    conn.execute("UPDATE runs SET strategy_id=?,strategy_config_hash=? WHERE run_id=?", (cfg.strategy_id, cfg.strategy_config_hash, run_id))
    recorder = Recorder(cfg.storage["queue_capacity_records"], cfg.storage["priority_headroom_records"])
    errors: list[str] = []
    writer = SqliteWriter(conn, recorder, cfg.storage["batch_max_records"], cfg.storage["flush_interval_ms"], errors.append, cfg.storage["writer_locked_retries"], cfg.storage["fk_merge_max_batches"],
                          quarantine_path=OUT_DB.with_suffix(".quarantine.jsonl"), busy_timeout_ms=cfg.storage["writer_busy_timeout_ms"], retry_backoff_ms=cfg.storage["writer_retry_backoff_ms"])
    writer.start()
    scheduler = LiveScheduler(loop, clock)
    adapter = PaperExecutionAdapter(clock, scheduler, "PAPER", bool(cfg["live_broadcast_enabled"]), cfg.latency_model["entry_delay_ms"] * 1_000_000, cfg.latency_model["exit_delay_ms"] * 1_000_000)
    basis_holder = {"basis": CostBasis(None, None, None, None)}
    owner = StateOwner(cfg, run_id, clock, scheduler, recorder, adapter, lambda: basis_holder["basis"], "PAPER", economic_intents_allowed=True,
                       feature_sink=lambda job: job(), prior_submitted_mints=set(), capture_only=False)
    fee_seq = 0

    def publish_fees():
        nonlocal fee_seq
        fee_seq += 1
        snaps = fee_snapshots(cfg, run_id, clock, fee_seq)
        basis_holder["basis"] = CostBasis(*snaps)
        for s in snaps:
            recorder.try_append(("fee_snapshot", (s, run_id)))
        owner.on_fee_snapshots(snaps)

    publish_fees()
    stop = asyncio.Event()

    async def fee_loop():
        while not stop.is_set():
            await asyncio.sleep(5.0)
            publish_fees()

    feeds = {}
    for feed in cfg.feeds:
        if not feed.enabled or feed.id not in scripted:
            continue
        ack = json.dumps({"jsonrpc": "2.0", "id": 1, "result": 7}) if feed.transport == "logsSubscribe" else None
        ws = FakeWebSocket(scripted[feed.id], ack=ack)
        feeds[feed.id] = FeedAdapter(feed, "wss://synthetic.invalid/bench", dict(cfg.connectivity, reconnect_initial_ms=1, reconnect_max_ms=1), cfg.protocol["program_id"], clock, owner.on_raw, owner.on_feed_health, owner.on_transport_error, fake_connect_factory([ws]))
    depth_max = 0

    async def depth_probe():
        nonlocal depth_max
        while not stop.is_set():
            depth_max = max(depth_max, recorder.depth())
            await asyncio.sleep(0.05)

    wall0 = time.monotonic()
    tasks = [loop.create_task(f.run(), name=f"feed-{fid}") for fid, f in feeds.items()]
    aux = [loop.create_task(fee_loop()), loop.create_task(depth_probe())]
    # each feed adapter stops itself when its scripted session is exhausted (no more sessions -> ConnectionError -> reconnect loop); stop after the last frame is delivered
    total = sum(len(v) for v in scripted.values())
    while sum(f.stats.messages for f in feeds.values()) < total and time.monotonic() - wall0 < 3600:
        await asyncio.sleep(0.2)
    await asyncio.sleep(2.0)  # let due modeled-fill callbacks fire
    for f in feeds.values():
        f.stop()
    for t in tasks:
        t.cancel()
    await asyncio.gather(*tasks, return_exceptions=True)
    stop.set()
    await asyncio.gather(*aux, return_exceptions=True)
    wall_feed = time.monotonic() - wall0
    end_mono = clock.monotonic_ns()
    owner.stop_new_entries = True
    owner.finalize(end_mono)
    stopped = await asyncio.to_thread(writer.stop, cfg.storage["writer_shutdown_timeout_ms"] / 1000)
    if not stopped:
        await asyncio.to_thread(writer.wait_until_stopped)
    conn.execute("UPDATE runs SET status='COMPLETED', ended_utc=?, ended_mono_ns=?, end_balance_lamports=? WHERE run_id=?", (clock.utc_iso(), end_mono, owner.portfolio.cash_lamports + owner.portfolio.reserved_lamports, run_id))
    conn.commit()
    conn.row_factory = sqlite3.Row
    rec_dec = [r["decode_done_mono_ns"] - r["receive_mono_ns"] for r in conn.execute("SELECT receive_mono_ns, decode_done_mono_ns FROM feed_observations WHERE run_id=? AND decode_done_mono_ns IS NOT NULL AND raw_payload IS NOT NULL", (run_id,))]
    decs = [dict(r) for r in conn.execute("SELECT action, receive_mono_ns, data_ready_mono_ns, decision_done_mono_ns, intent_enqueued_mono_ns FROM decisions WHERE run_id=?", (run_id,))]
    rec_decision = [d["decision_done_mono_ns"] - d["receive_mono_ns"] for d in decs]
    ready_decision = [d["decision_done_mono_ns"] - d["data_ready_mono_ns"] for d in decs]
    entry = [d for d in decs if d["action"] == "ENTRY" and d["intent_enqueued_mono_ns"] is not None]
    dec_enq = [d["intent_enqueued_mono_ns"] - d["decision_done_mono_ns"] for d in entry]
    rec_enq = [d["intent_enqueued_mono_ns"] - d["receive_mono_ns"] for d in entry]
    queue_wait = [r["callback_actual_mono_ns"] - r["scheduled_fill_mono_ns"] for r in conn.execute("SELECT scheduled_fill_mono_ns, callback_actual_mono_ns FROM execution_attempts WHERE run_id=? AND side='BUY' AND callback_actual_mono_ns IS NOT NULL", (run_id,))]
    actions = {r[0]: r[1] for r in conn.execute("SELECT action, COUNT(*) FROM decisions WHERE run_id=? GROUP BY action", (run_id,))}
    reasons = {r[0]: r[1] for r in conn.execute("SELECT reason, COUNT(*) FROM decisions WHERE run_id=? AND action='SKIP' GROUP BY reason", (run_id,))}
    conn.close()
    result = {
        "label": LABEL, "baseline_commit": "05971e4ed35a8a98349e298d2de4e61f8584fc5b", "python": platform.python_version(), "platform": platform.platform(), "cpu": platform.processor() or open("/proc/cpuinfo").read().split("model name")[1].split("\n")[0].split(":")[1].strip(),
        "source_db": str(SRC), "source_db_sha256": sha256_file(SRC), "source_run_id": SRC_RUN, "config_path": str(CFG_PATH), "config_hash_file": cfg_base.config_hash, "effective_config_hash": cfg.config_hash, "slots": SLOTS, "compress_factor": COMPRESS,
        "runner_sha256": sha256_file(Path(__file__).resolve()), "command": " ".join(sys.argv), "writer_enabled": True, "writer": {"committed_records": writer.committed_records, "failures": writer.failures, "quarantined_records": writer.quarantined_records, "recorder_depth_max": depth_max, "recorder_overflowed": recorder.overflowed},
        "feeds": {fid: {"messages": f.stats.messages, "connections": f.stats.connections, "reconnects": f.stats.reconnects} for fid, f in feeds.items()}, "frames_total": total, "feed_wall_s": wall_feed,
        "owner": {"raw_messages": owner.counters.raw_messages, "creates": owner.counters.creates, "entry_intents": owner.counters.entry_intents, "entry_fills": owner.counters.entry_fills, "internal_errors": owner.counters.internal_errors, "decode_errors": owner.counters.decode_errors, "duplicates": owner.counters.duplicates},
        "decisions_by_action": actions, "skip_reasons": reasons,
        "metrics": {"receive_to_decode": pct(rec_dec), "receive_to_decision": pct(rec_decision), "ready_to_decision": pct(ready_decision), "decision_to_enqueue": pct(dec_enq), "receive_to_enqueue": pct(rec_enq), "queue_wait_entry_callback_lateness": pct(queue_wait)},
        "metric_definitions": {"receive_to_decode": "feed_observations.decode_done_mono_ns - receive_mono_ns (all decoded frames, all feeds)", "receive_to_decision": "decisions.decision_done_mono_ns - receive_mono_ns (every CREATE decision incl. SKIP)",
                               "ready_to_decision": "decisions.decision_done_mono_ns - data_ready_mono_ns", "decision_to_enqueue": "decisions.intent_enqueued_mono_ns - decision_done_mono_ns (ENTRY only)",
                               "receive_to_enqueue": "decisions.intent_enqueued_mono_ns - receive_mono_ns (ENTRY only)", "queue_wait_entry_callback_lateness": "execution_attempts BUY: callback_actual_mono_ns - scheduled_fill_mono_ns (asyncio LiveScheduler lateness of the enqueued PAPER intent)"},
        "raw_samples": {"receive_to_decode_ns": rec_dec, "receive_to_decision_ns": rec_decision, "ready_to_decision_ns": ready_decision, "decision_to_enqueue_ns": dec_enq, "receive_to_enqueue_ns": rec_enq, "queue_wait_ns": queue_wait},
    }
    if errors:
        result["writer_errors"] = errors
    return result


def main() -> None:
    result = asyncio.run(main_async())
    RESULT.write_text(json.dumps(result, indent=1, default=str) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "raw_samples"}, indent=1, default=str))


if __name__ == "__main__":
    main()

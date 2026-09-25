#!/usr/bin/env python3
"""Phase-A one-off: generate a deterministic SYNTHETIC Solana/Pump.fun CAPTURE recording through the
production StateOwner(capture_only=True) + Recorder + SqliteWriter path (no monkeypatching).

This dataset is the SYNTHETIC REGRESSION PARITY source. It is NOT the clean-v2 production dataset.
Run once BEFORE the Phase-A code change, then freeze the file (SHA-256) and never regenerate it.

Usage: python3 gen_source_capture.py <repo_root> <output_sqlite> [seed]
"""
from __future__ import annotations

import base64
import hashlib
import json
import random
import sys
import time
from pathlib import Path

ROOT = Path(sys.argv[1]).resolve()
OUT = Path(sys.argv[2]).resolve()
SEED = int(sys.argv[3]) if len(sys.argv) > 3 else 20260924
sys.path.insert(0, str(ROOT))

from darkbot.clock import ReplayClock  # noqa: E402
from darkbot.config import load_runtime_config  # noqa: E402
from darkbot.domain import CostEvidence, FeeSnapshot, RawEnvelope, utc_iso  # noqa: E402
from darkbot.market import b58encode, encode_fields, event_discriminator  # noqa: E402
from darkbot.paper import PaperExecutionAdapter, StateOwner  # noqa: E402
from darkbot.storage import Recorder, SqliteWriter, apply_migrations, mark_interrupted_runs, open_connection, upsert_config_snapshot  # noqa: E402

CONFIG = ROOT / "config" / "strategy_c_launch_sniper_0ms_15x.json"
PROGRAM = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
T0 = 1_000_000_000_000  # mono ns epoch start (synthetic epoch)
UTC0 = 1_780_000_000_000_000_000
DURATION_NS = 30 * 60 * 1_000_000_000
V_TOK0, V_SOL0, R_TOK0, SUPPLY = 1_073_000_000_000_000, 30_000_000_000, 793_100_000_000_000, 1_000_000_000_000_000
CREATE_LAYOUT = [["name", "string"], ["symbol", "string"], ["uri", "string"], ["mint", "pubkey"], ["bonding_curve", "pubkey"], ["user", "pubkey"], ["creator", "pubkey"], ["timestamp", "i64"], ["virtual_token_reserves", "u64"], ["virtual_sol_reserves", "u64"], ["real_token_reserves", "u64"], ["token_total_supply", "u64"]]
TRADE_LAYOUT = [["mint", "pubkey"], ["sol_amount", "u64"], ["token_amount", "u64"], ["is_buy", "bool"], ["user", "pubkey"], ["timestamp", "i64"], ["virtual_sol_reserves", "u64"], ["virtual_token_reserves", "u64"], ["real_sol_reserves", "u64"], ["real_token_reserves", "u64"]]
COMPLETE_LAYOUT = [["user", "pubkey"], ["mint", "pubkey"], ["bonding_curve", "pubkey"], ["timestamp", "i64"]]


def pk(name: str) -> str:
    return b58encode(hashlib.sha256(name.encode()).digest())


def create_logs(mint, creator, curve, state, initial_buy):
    payload = event_discriminator("CreateEvent") + encode_fields([("name", "T"), ("symbol", "T"), ("uri", "u"), ("mint", mint), ("bonding_curve", curve), ("user", creator), ("creator", creator), ("timestamp", 1), ("virtual_token_reserves", state["vtok"]), ("virtual_sol_reserves", state["vsol"]), ("real_token_reserves", state["rtok"]), ("token_total_supply", SUPPLY)], CREATE_LAYOUT)
    logs = [f"Program {PROGRAM} invoke [1]", "Program log: Instruction: Create", "Program data: " + base64.b64encode(payload).decode()]
    if initial_buy:
        logs.append(trade_line(mint, creator, initial_buy, True, state))
    logs.append(f"Program {PROGRAM} success")
    return logs


def trade_line(mint, user, sol, is_buy, state):
    """Apply a constant-product trade to `state` and return the Program data log line (state mutated)."""
    if is_buy:
        tok = sol * state["vtok"] // (state["vsol"] + sol)
        tok = min(tok, state["rtok"])
        state["vsol"] += sol; state["vtok"] -= tok; state["rtok"] -= tok
    else:
        tok = sol  # for sells `sol` carries the token amount
        tok = min(tok, state["held"].get(user, 0))
        sol = tok * state["vsol"] // (state["vtok"] + tok)
        state["vsol"] -= sol; state["vtok"] += tok; state["rtok"] += tok
    if is_buy:
        state["held"][user] = state["held"].get(user, 0) + tok
    else:
        state["held"][user] = state["held"].get(user, 0) - tok
    payload = event_discriminator("TradeEvent") + encode_fields([("mint", mint), ("sol_amount", sol), ("token_amount", tok), ("is_buy", is_buy), ("user", user), ("timestamp", 1), ("virtual_sol_reserves", state["vsol"]), ("virtual_token_reserves", state["vtok"]), ("real_sol_reserves", max(0, state["vsol"] - V_SOL0)), ("real_token_reserves", state["rtok"])], TRADE_LAYOUT)
    return "Program data: " + base64.b64encode(payload).decode()


def trade_logs(line):
    return [f"Program {PROGRAM} invoke [1]", line, f"Program {PROGRAM} success"]


def complete_logs(mint, user, curve):
    payload = event_discriminator("CompleteEvent") + encode_fields([("user", user), ("mint", mint), ("bonding_curve", curve), ("timestamp", 1)], COMPLETE_LAYOUT)
    return [f"Program {PROGRAM} invoke [1]", "Program data: " + base64.b64encode(payload).decode(), f"Program {PROGRAM} success"]


def notification(signature, slot, logs, err=None):
    return json.dumps({"jsonrpc": "2.0", "method": "logsNotification", "params": {"subscription": 1, "result": {"context": {"slot": slot}, "value": {"signature": signature, "err": err, "logs": logs}}}})


def build_timeline(rng: random.Random):
    """(mono_ns, signature, slot, logs, err, hint_json_or_None) ordered by mono_ns; the same tx is emitted on both feeds by the driver."""
    events = []
    n_launches = 300
    slot = 400_000_000
    sig_n = 0

    def sig():
        nonlocal sig_n
        sig_n += 1
        return b58encode(hashlib.sha256(f"sig{sig_n}".encode()).digest() + hashlib.sha256(f"sig{sig_n}b".encode()).digest())

    buyers = [pk(f"buyer{i}") for i in range(400)]
    for i in range(n_launches):
        t_create = T0 + int(rng.uniform(0.5, 29.5) * 60 * 1_000_000_000)
        mint, creator, curve = pk(f"mint{i}"), pk(f"creator{i}"), pk(f"curve{i}")
        state = {"vsol": V_SOL0, "vtok": V_TOK0, "rtok": R_TOK0, "held": {}}
        initial_buy = int(rng.uniform(0.02, 2.0) * 1e9) if rng.random() < 0.85 else 0
        logs = create_logs(mint, creator, curve, state, initial_buy)
        hint = json.dumps({"signature": "pp", "mint": mint, "txType": "create", "traderPublicKey": creator, "pool": "pump"}) if rng.random() < 0.5 else None
        events.append((t_create, sig(), slot, logs, None, hint))
        n_trades = int(rng.expovariate(1 / 40))
        t = t_create
        creator_sell_at = None
        r = rng.random()
        if r < 0.35:
            creator_sell_at = t_create + int(rng.uniform(0.05, 5.0) * 1e9)
        elif r < 0.60:
            creator_sell_at = t_create + int(rng.uniform(5.0, 400.0) * 1e9)
        complete_at = t_create + int(rng.uniform(20, 500) * 1e9) if rng.random() < 0.04 else None
        for k in range(n_trades):
            t += int(rng.expovariate(1 / (0.25 * 1e9)) * (1 + k / 20))
            if t - t_create > 600 * 1e9:
                break
            user = rng.choice(buyers[: 20 + i % 50])
            if rng.random() < 0.65 or not state["held"].get(user):
                line = trade_line(mint, user, int(rng.uniform(0.005, 0.8) * 1e9), True, state)
            else:
                line = trade_line(mint, user, int(state["held"][user] * rng.uniform(0.2, 1.0)), False, state)
            if rng.random() < 0.03:
                events.append((t, sig(), slot + (t - T0) // 400_000_000, trade_logs(line), {"InstructionError": [0, "Custom"]}, None))  # failed tx
            else:
                events.append((t, sig(), slot + (t - T0) // 400_000_000, trade_logs(line), None, None))
            if creator_sell_at is not None and t >= creator_sell_at and state["held"].get(creator):
                events.append((creator_sell_at if creator_sell_at > t_create else t, sig(), slot + (t - T0) // 400_000_000, trade_logs(trade_line(mint, creator, state["held"][creator], False, state)), None, None))
                creator_sell_at = None
            if complete_at is not None and t >= complete_at:
                events.append((t, sig(), slot + (t - T0) // 400_000_000, complete_logs(mint, user, curve), None, None))
                complete_at = None
                break
        if creator_sell_at is not None and state["held"].get(creator) and creator_sell_at - t_create < 600 * 1e9:
            events.append((creator_sell_at, sig(), slot + (creator_sell_at - T0) // 400_000_000, trade_logs(trade_line(mint, creator, state["held"][creator], False, state)), None, None))
    # a few undecodable / unattributable frames (unknown program data) spread over the run
    for j in range(25):
        t = T0 + int(rng.uniform(1, 29) * 60 * 1e9)
        events.append((t, sig(), slot, [f"Program {PROGRAM} invoke [1]", "Program data: " + base64.b64encode(b"\x00" * 24).decode(), f"Program {PROGRAM} success"], None, None))
    events.sort(key=lambda e: e[0])
    return events


def make_fee_snapshots(cfg, run_id, mono, seq):
    rp = cfg.costs["snapshot_refresh_policy"]
    cu_price = 10_000 + (seq % 7) * 500
    prio = -(-(cfg.execution["priority_policy"]["compute_unit_limit"] * cu_price) // 1_000_000)

    def snap(kind, stype, method, norm, max_age_ms):
        return FeeSnapshot(f"{run_id}_fee_{kind}_{seq}", stype, "SYNTHETIC_FIXTURE", method, (), 1, utc_iso(UTC0 + (mono - T0)), mono, "processed", PROGRAM, cfg.protocol["idl_version_or_hash"], {"synthetic": True, "seq": seq}, norm, cfg.protocol["fee_rounding"], "SYNTHETIC", cfg.freshness_policy_hash, "VALID", cfg.cost_model_hash, max_age_ms * 1_000_000, CostEvidence.MEASURED)
    return {
        "protocol": snap("protocol", "PROTOCOL_GLOBAL_ACCOUNT", "getAccountInfo", {"protocol_fee_bps": 100, "creator_fee_bps": 30, "other_protocol_fee_bps": 0}, rp["protocol_max_age_ms"]),
        "network": snap("network", "NETWORK_FEE", "getFeeForMessage", {"base_fee_lamports": 5000, "priority_fee_lamports": prio, "network_fee_total_lamports": 5000 + prio}, rp["network_fee_max_age_ms"]),
        "rent": snap("rent", "RENT_EXEMPTION", "getMinimumBalanceForRentExemption", {"token_account_rent_lamports": 2_039_280, "account_size_bytes": 165}, rp["rent_max_age_ms"]),
        "priority_policy": snap("priority", "PRIORITY_POLICY", "getRecentPrioritizationFees", {"compute_unit_price_micro_lamports": cu_price, "compute_unit_limit": cfg.execution["priority_policy"]["compute_unit_limit"]}, rp["network_fee_max_age_ms"]),
    }


def main() -> None:
    if OUT.exists():
        raise SystemExit(f"refusing to overwrite frozen dataset {OUT}")
    rng = random.Random(SEED)
    cfg = load_runtime_config(CONFIG)
    campaign = "phase_a_synth_capture"
    run_id = f"{campaign}_capture_synthetic_seed{SEED}"
    clock = ReplayClock("epoch_phase_a_synth", T0, UTC0)
    conn = open_connection(OUT)
    applied = apply_migrations(conn)
    mark_interrupted_runs(conn, campaign, clock.utc_iso())
    now_utc = clock.utc_iso()
    for kind, (h, content) in cfg.snapshot_kinds().items():
        upsert_config_snapshot(conn, kind, h, content, str(cfg.path), now_utc)
    conn.execute("INSERT INTO runs(run_id,campaign_id,mode,started_utc,code_revision,spec_hash,config_hash,protocol_snapshot_id,host_clock_epoch_id,status,quality_status,start_balance_lamports,readiness,started_mono_ns) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                 (run_id, campaign, "CAPTURE", now_utc, "phase_a_baseline_05971e4", "synthetic", cfg.config_hash, cfg.protocol_hash, clock.epoch_id, "RUNNING", "OK", cfg.portfolio["initial_paper_balance_lamports"], "NOT_READY", clock.monotonic_ns()))
    conn.execute("UPDATE runs SET strategy_id=?,strategy_config_hash=? WHERE run_id=?", (cfg.strategy_id, cfg.strategy_config_hash, run_id))
    recorder = Recorder(cfg.storage["queue_capacity_records"], cfg.storage["priority_headroom_records"])
    errors: list[str] = []
    writer = SqliteWriter(conn, recorder, cfg.storage["batch_max_records"], cfg.storage["flush_interval_ms"], errors.append, cfg.storage["writer_locked_retries"], cfg.storage["fk_merge_max_batches"])
    from darkbot.clock import ReplayScheduler
    scheduler = ReplayScheduler()
    adapter = PaperExecutionAdapter(clock, scheduler, "PAPER", False, 0, 0)
    fee_holder = {"basis": None}
    owner = StateOwner(cfg, run_id, clock, scheduler, recorder, adapter, lambda: fee_holder["basis"], "PAPER", economic_intents_allowed=False, capture_only=True)

    def publish_fees(mono, seq):
        snaps = make_fee_snapshots(cfg, run_id, mono, seq)
        from darkbot.domain import CostBasis
        fee_holder["basis"] = CostBasis(snaps["protocol"], snaps["network"], snaps["rent"], snaps["priority_policy"])
        lst = list(snaps.values())
        for s in lst:
            recorder.try_append(("fee_snapshot", (s, run_id)))
        owner.on_fee_snapshots(lst)

    timeline = build_timeline(rng)
    # feed health: both feeds ready at start; existing_rpc drops for 40s at +7min (covered); both down 2s at +18min (canonical gap)
    health = [(T0 + 1_000_000, "helius_standard", True), (T0 + 2_000_000, "existing_rpc", True), (T0 + 3_000_000, "pumpportal", True),
              (T0 + 7 * 60 * 10**9, "existing_rpc", False), (T0 + 7 * 60 * 10**9 + 40 * 10**9, "existing_rpc", True),
              (T0 + 18 * 60 * 10**9, "existing_rpc", False), (T0 + 18 * 60 * 10**9 + 10**8, "helius_standard", False),
              (T0 + 18 * 60 * 10**9 + 2 * 10**9, "helius_standard", True), (T0 + 18 * 60 * 10**9 + 3 * 10**9, "existing_rpc", True)]
    down_until = {"helius_standard": None, "existing_rpc": None}
    hi = 0
    seq = {"helius_standard": 0, "existing_rpc": 0, "pumpportal": 0}
    conn_n = {"helius_standard": 1, "existing_rpc": 1, "pumpportal": 1}
    fee_seq = 0
    next_fee = {"protocol": T0, "network": T0, "rent": T0}
    frames = []  # (mono, feed, payload)
    for mono, signature, slot, logs, err, hint in timeline:
        payload = notification(signature, slot, logs, err)
        frames.append((mono, "helius_standard", payload))
        frames.append((mono + int(rng.gauss(180, 60) * 1e6), "existing_rpc", payload))
        if hint is not None:
            frames.append((mono + int(rng.uniform(-150, 250) * 1e6), "pumpportal", hint))
    frames = [f for f in frames if f[0] <= T0 + DURATION_NS - 1_000_000]  # the recorded run window ends at T0 + 30 min
    frames.sort(key=lambda f: f[0])
    writer.start()
    t_start = time.monotonic()
    n = 0
    for mono, feed, payload in frames:
        if mono < T0 + 5_000_000:
            mono = T0 + 5_000_000
        while hi < len(health) and health[hi][0] <= mono:
            h_mono, fid, ready = health[hi]
            clock.advance_to(max(clock.monotonic_ns(), h_mono))
            if not ready:
                conn_n[fid] += 1
            owner.on_feed_health(fid, ready, h_mono, "synthetic")
            if fid in down_until:
                down_until[fid] = None if ready else True
            hi += 1
        clock.advance_to(max(clock.monotonic_ns(), mono))
        if mono >= next_fee["network"]:
            fee_seq += 1
            publish_fees(mono, fee_seq)
            next_fee["network"] = mono + cfg.costs["snapshot_refresh_policy"]["network_fee_refresh_interval_ms"] * 1_000_000
        if feed in down_until and down_until[feed]:
            continue  # a disconnected feed delivers nothing
        seq[feed] += 1
        owner.on_raw(RawEnvelope(feed, f"{feed}-c{conn_n[feed]}", mono, utc_iso(clock.utc_ns()), seq[feed], payload))
        n += 1
        if n % 5000 == 0:
            owner.maybe_evict(mono)
    end_mono = T0 + DURATION_NS
    clock.advance_to(end_mono)
    owner.finalize(end_mono)
    writer.stop(60)
    writer.wait_until_stopped()
    if errors:
        raise SystemExit(f"writer errors: {errors}")
    conn.execute("UPDATE runs SET status=?, ended_utc=?, ended_mono_ns=?, end_balance_lamports=?, quality_status=?, readiness=? WHERE run_id=?",
                 ("COMPLETED", clock.utc_iso(), end_mono, owner.portfolio.cash_lamports + owner.portfolio.reserved_lamports, "OK" if not (recorder.overflowed or owner.counters.internal_errors) else "DEGRADED", "CAPTURE_READY", run_id))
    conn.commit()
    stats = {"run_id": run_id, "campaign": campaign, "seed": SEED, "config": str(CONFIG.relative_to(ROOT)), "config_hash": cfg.config_hash, "migrations_applied": applied,
             "frames_delivered": n, "frames_generated": len(frames), "tx_events": len(timeline), "owner_status": owner.status(), "counters": owner.counters.__dict__,
             "writer": {"committed_records": writer.committed_records, "failures": writer.failures, "quarantined": writer.quarantined_records},
             "generation_wall_s": time.monotonic() - t_start}
    conn.close()
    (OUT.with_suffix(".generation.json")).write_text(json.dumps(stats, indent=2, default=str) + "\n")
    print(json.dumps({k: v for k, v in stats.items() if k not in ("owner_status",)}, default=str))


if __name__ == "__main__":
    main()

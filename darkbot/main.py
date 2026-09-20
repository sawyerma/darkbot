"""Wires configuration, recorder, fee cache, feed adapters and StateOwner; start/stop, readiness and
the shared async lifecycle; doctor probe orchestration (spec: main.py). No second trading logic.
"""

from __future__ import annotations

import asyncio
import concurrent.futures
import os
import platform
import shutil
import signal
import sys
import time
from pathlib import Path
from typing import Any

from darkbot import __version__
from darkbot.capture import FeedAdapter
from darkbot.clock import LiveClock, LiveScheduler
from darkbot.config import ConfigError, RuntimeConfig, load_runtime_config, redact, resolve_feed_endpoints, resolve_rpc_http, sha256_text
from darkbot.domain import Readiness, utc_iso
from darkbot.fees import FeeResolver, http_rpc
from darkbot.paper import PaperExecutionAdapter, StateOwner
from darkbot.storage import REQUIRED_TABLES, Recorder, SqliteWriter, apply_migrations, integrity_check, mark_interrupted_runs, open_connection, prior_campaign_mints, upsert_config_snapshot, write_readback_test

SPEC_PATH = Path(__file__).resolve().parent.parent / "docs" / "SPEC.json"


def code_revision() -> str:
    return os.environ.get("DARKBOT_CODE_REVISION", f"darkbot-{__version__}")


def spec_hash() -> str:
    return sha256_text(SPEC_PATH.read_text(encoding="utf-8")) if SPEC_PATH.exists() else "SPEC_MISSING"


def economic_profile_samples(owner: Any, basis: Any, now: int, limit: int | None = None) -> int:
    """Economic readiness requires an observed supported curve and side-scoped costs.

    Captured unsupported/unknown profiles cannot certify economic PAPER readiness.
    This diagnostic runs outside the BUY critical section and never submits orders.
    """
    from darkbot.domain import Completeness, InvalidQuote, QuoteScopeStatus, Side
    from darkbot.execution import fee_terms
    if not basis.complete or not all(s.is_fresh(now) for s in (basis.protocol, basis.network, basis.rent, basis.priority_policy)):
        return 0
    count = 0
    for st in owner.reducer.states.values():
        if (st.create_completeness != Completeness.COMPLETE or st.quote_scope_status != QuoteScopeStatus.SOL_WSOL
                or st.unsupported_fee_mode or st.quote_uncertain or st.decode_gap_since is not None):
            continue
        if st.token_program not in (owner.cfg.protocol["spl_token_program"], owner.cfg.protocol["token_2022_program"]):
            continue
        curve = owner._curve(st)
        if curve and all(not isinstance(fee_terms(basis, owner.policy, now, curve, side), InvalidQuote) for side in (Side.BUY, Side.SELL)):
            count += 1
            if limit is not None and count >= limit:
                return count
    return count


class Application:
    """One PAPER (or DOCTOR) run: one process, one StateOwner, bounded queues, one SQLite writer thread."""

    def __init__(self, cfg: RuntimeConfig, db_path: Path, campaign: str, mode: str = "PAPER", max_duration_ms: int | None = None, economic: bool = True, status_stream: Any = None) -> None:
        self.cfg, self.db_path, self.campaign, self.mode = cfg, db_path, campaign, mode
        self._economic_sample_count_at_stop: int | None = None
        self.max_duration_ms = max_duration_ms
        self.economic = economic
        self.status_stream = status_stream or sys.stderr
        self.clock = LiveClock()
        self.run_id = f"{campaign}_{mode.lower()}_{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}_{self.clock.epoch_id[-6:]}"
        self.stop_event = asyncio.Event()
        self.stop_reason = ""
        self.feeds: dict[str, FeedAdapter] = {}
        self.owner: StateOwner | None = None
        self.resolver: FeeResolver | None = None
        self.writer: SqliteWriter | None = None
        self.recorder = Recorder(cfg.storage["queue_capacity_records"], cfg.storage["priority_headroom_records"])
        self.conn = None
        self.readiness = Readiness.NOT_READY
        self._loop: asyncio.AbstractEventLoop | None = None
        self.executor = concurrent.futures.ThreadPoolExecutor(max_workers=1, thread_name_prefix="features")

    # ------------------------------------------------------------------ setup
    def open_storage(self) -> dict[str, Any]:
        self.conn = open_connection(self.db_path)
        applied = apply_migrations(self.conn)
        interrupted = mark_interrupted_runs(self.conn, self.campaign, self.clock.utc_iso())
        now_utc = self.clock.utc_iso()
        for kind, (h, content) in self.cfg.snapshot_kinds().items():
            upsert_config_snapshot(self.conn, kind, h, content, str(self.cfg.path), now_utc)
        self.conn.execute("INSERT INTO runs(run_id,campaign_id,mode,started_utc,code_revision,spec_hash,config_hash,protocol_snapshot_id,host_clock_epoch_id,status,quality_status,start_balance_lamports,readiness,started_mono_ns) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                          (self.run_id, self.campaign, self.mode, now_utc, code_revision(), spec_hash(), self.cfg.config_hash, self.cfg.protocol_hash, self.clock.epoch_id, "RUNNING", "OK", self.cfg.portfolio["initial_paper_balance_lamports"], self.readiness.value, self.clock.monotonic_ns()))
        self.conn.execute("UPDATE runs SET strategy_id=?,strategy_config_hash=? WHERE run_id=?", (self.cfg.strategy_id, self.cfg.strategy_config_hash, self.run_id))
        return {"migrations": applied, "interrupted_previous_runs": interrupted, "integrity": integrity_check(self.conn)}

    def _disk_error(self, message: str) -> None:
        """Called from the writer thread: never touch the StateOwner here; hand over to the event-loop thread (single-owner rule)."""
        loop = self._loop
        if loop is not None and self.owner is not None:
            loop.call_soon_threadsafe(self.owner.on_disk_error, message)

    async def run(self) -> dict[str, Any]:
        loop = asyncio.get_running_loop()
        self._loop = loop
        storage_info = self.open_storage()
        assert self.conn is not None
        self.writer = SqliteWriter(self.conn, self.recorder, self.cfg.storage["batch_max_records"], self.cfg.storage["flush_interval_ms"], self._disk_error, self.cfg.storage["writer_locked_retries"], self.cfg.storage["fk_merge_max_batches"],
                                   quarantine_path=self.db_path.with_suffix(".quarantine.jsonl"), busy_timeout_ms=self.cfg.storage["writer_busy_timeout_ms"], retry_backoff_ms=self.cfg.storage["writer_retry_backoff_ms"])
        prior_mints = prior_campaign_mints(self.conn, self.campaign)
        self.writer.start()
        scheduler = LiveScheduler(loop, self.clock)
        adapter = PaperExecutionAdapter(self.clock, scheduler, "PAPER", bool(self.cfg["live_broadcast_enabled"]), self.cfg.latency_model["entry_delay_ms"] * 1_000_000, self.cfg.latency_model["exit_delay_ms"] * 1_000_000)
        rpc_ep = resolve_rpc_http(self.cfg)
        rpc = http_rpc(rpc_ep.url, self.cfg.connectivity["rpc_http_timeout_ms"] / 1000) if rpc_ep.available else None
        self.resolver = FeeResolver(self.cfg, rpc, self.clock, self.run_id, "rpc_http" if rpc_ep.available else "NONE")
        self.owner = StateOwner(self.cfg, self.run_id, self.clock, scheduler, self.recorder, adapter, lambda: self.resolver.cache.basis(), "PAPER", economic_intents_allowed=self.economic,
                                feature_sink=lambda job: loop.run_in_executor(self.executor, job), prior_submitted_mints=prior_mints if self.mode == "PAPER" else None)
        endpoints = resolve_feed_endpoints(self.cfg)
        feed_status: dict[str, str] = {}
        for feed in self.cfg.feeds:
            ep = endpoints[feed.id]
            if not feed.enabled:
                feed_status[feed.id] = "DISABLED"
                continue
            if not ep.available:
                feed_status[feed.id] = f"DISABLED_OPTIONAL missing env {ep.missing_env}" if not feed.required else f"FAILED missing env {ep.missing_env}"
                continue
            self.feeds[feed.id] = FeedAdapter(feed, ep.url, self.cfg.connectivity, self.cfg.protocol["program_id"], self.clock, self.owner.on_raw, self.owner.on_feed_health, self.owner.on_transport_error)
            feed_status[feed.id] = "STARTING"
        tasks = [loop.create_task(f.run(), name=f"feed-{fid}") for fid, f in self.feeds.items()]
        fee_task = loop.create_task(self.resolver.run_background(self._on_fee_snapshots, self.stop_event), name="fees")
        fx_task = None
        if self.cfg["fx"]["enabled"]:
            from darkbot.fx import run_fx_background
            fx_task = loop.create_task(run_fx_background(self.cfg["fx"], self.clock, self.owner.on_fx_snapshot, self.stop_event), name="sol-usd-evidence")
        status_task = loop.create_task(self._status_loop(), name="status")
        for sig in (signal.SIGINT, signal.SIGTERM):
            try:
                loop.add_signal_handler(sig, lambda s=sig: self.request_stop(f"signal {s.name}"))
            except (NotImplementedError, RuntimeError):
                pass
        if self.max_duration_ms:
            loop.call_later(self.max_duration_ms / 1000, lambda: self.request_stop("max_duration"))
        if not self.feeds:
            self.request_stop("no feed could be started (missing endpoint env or all feeds disabled)")
        await self.stop_event.wait()
        # controlled shutdown: no new entries, stop feeds, flush recorder, mark run
        self.owner.stop_new_entries = True
        for f in self.feeds.values():
            f.stop()
        for t in tasks:
            t.cancel()
        await asyncio.gather(*tasks, fee_task, *([fx_task] if fx_task else []), return_exceptions=True)
        status_task.cancel()
        await asyncio.gather(status_task, return_exceptions=True)
        for fid in self.feeds:
            self.owner._quality("FEED_STATS", "INFO", "FEED", canonical_gap=False, details=self._feed_report(fid), lost=None, feed_id=fid, end=self.clock.monotonic_ns(), recovery="RUN_END")
        readiness = self._capture_end_readiness()
        self.owner.finalize(self.clock.monotonic_ns())
        self.executor.shutdown(wait=True)
        stopped = await asyncio.to_thread(self.writer.stop, self.cfg.storage["writer_shutdown_timeout_ms"] / 1000)
        if not stopped:
            # Never use or close the connection while its sole writer is alive.
            await asyncio.to_thread(self.writer.wait_until_stopped)
        status = "INTERRUPTED" if "signal" in self.stop_reason else "COMPLETED"
        quality = "OK" if not (self.recorder.overflowed or self.writer.disk_error or self.owner.counters.internal_errors) else "DEGRADED"
        self.conn.execute("UPDATE runs SET status=?, ended_utc=?, ended_mono_ns=?, end_balance_lamports=?, quality_status=?, readiness=? WHERE run_id=?",
                          (status, self.clock.utc_iso(), self.clock.monotonic_ns(), self.owner.portfolio.cash_lamports + self.owner.portfolio.reserved_lamports, quality, readiness.value, self.run_id))
        summary = {"run_id": self.run_id, "status": status, "stop_reason": self.stop_reason, "quality": quality, "readiness": readiness.value, "feeds_started": len(self.feeds), "feeds": {fid: self._feed_report(fid) for fid in self.feeds},
                   "feed_setup": feed_status, "owner": self.owner.status(), "cash_reconciliation": self.owner.cash_reconciliation(), "writer": {"committed_records": self.writer.committed_records, "failures": self.writer.failures, "fk_holds": self.writer.fk_holds, "quarantined_records": self.writer.quarantined_records, "disk_error": self.writer.disk_error},
                   "fee_errors": self.resolver.errors[-10:], "storage": storage_info, "db": str(self.db_path), "quarantined_records": self.writer.quarantined_records, "prior_campaign_mints": len(prior_mints), "economic_profile_samples_at_run_end": self._economic_sample_count_at_stop}
        self.conn.close()
        return summary

    def _feed_report(self, fid: str) -> dict[str, Any]:
        s = self.feeds[fid].stats
        return {"messages": s.messages, "reconnects": s.reconnects, "connections": s.connections, "acks": s.acks, "ack_failures": s.ack_failures, "transport_errors": s.transport_errors, "ready": s.ready, "last_error": redact(s.last_error or "") or None}

    def _capture_end_readiness(self) -> Readiness:
        # Feeds and execution callbacks have stopped; count once before finalize evicts trackers.
        assert self.owner is not None and self.resolver is not None
        self._economic_sample_count_at_stop = economic_profile_samples(self.owner, self.resolver.cache.basis(), self.clock.monotonic_ns())
        return self.compute_readiness()

    def compute_readiness(self) -> Readiness:
        assert self.owner is not None and self.resolver is not None
        eligible_ready = any(self.owner.feed_ready.get(f) for f in self.owner.eligible_trade_feeds)
        if not eligible_ready or self.recorder.overflowed or (self.writer and self.writer.disk_error):
            return Readiness.NOT_READY
        basis = self.resolver.cache.basis()
        now = self.clock.monotonic_ns()
        samples = self._economic_sample_count_at_stop
        if samples is None:
            samples = economic_profile_samples(self.owner, basis, now, limit=1)
        if samples and self.owner.capital_initialized:
            return Readiness.ECONOMIC_PAPER_READY
        return Readiness.CAPTURE_READY

    def _on_fee_snapshots(self, snapshots: list[Any]) -> None:
        for s in snapshots:
            self.recorder.try_append(("fee_snapshot", (s, self.run_id)))
        if self.owner is not None:
            self.owner.on_fee_snapshots(snapshots)

    def request_stop(self, reason: str) -> None:
        if not self.stop_event.is_set():
            self.stop_reason = reason
            self.stop_event.set()

    async def _status_loop(self) -> None:
        interval = self.cfg.storage["report_interval_ms"] / 1000
        while not self.stop_event.is_set():
            await asyncio.sleep(interval)
            if self.owner is None:
                continue
            self.owner.maybe_evict(self.clock.monotonic_ns())
            s = self.owner.status()
            tele = self.owner.telemetry["receive_to_decision_ns"][-self.cfg.storage["status_latency_window"] :]
            p50 = sorted(tele)[len(tele) // 2] if tele else None
            p95 = sorted(tele)[max(0, int(len(tele) * 0.95) - 1)] if tele else None
            feeds = ",".join(f"{k}:{'UP' if v else 'DOWN'}" for k, v in s["feeds"].items())
            readiness = self.compute_readiness().value
            print(f"[status {utc_iso(self.clock.utc_ns())}] {readiness} feeds={feeds} raw={s['raw']} creates={s['creates']} actionable={s['actionable']} intents={s['intents']} fills={s['fills']} open={s['open']} unresolved={s['unresolved']} "
                  f"pnl={s['realized_pnl']} cash={s['cash']} gap={s['gap_open']} q={s['recorder_depth']}/{s['recorder_overflow']} lat_p50/p95_ns={p50}/{p95} err={s['internal_errors']}", file=self.status_stream, flush=True)


# --------------------------------------------------------------------------- doctor


def count_live_creator_sells(conn: Any, run_id: str) -> int:
    """D04 live evidence: SELL events whose actor is the creator recorded on the mint's CREATE (trade rows carry the creator too)."""
    return conn.execute("SELECT COUNT(*) FROM market_events s JOIN market_events c ON c.run_id=s.run_id AND c.mint=s.mint AND c.event_type='CREATE' WHERE s.run_id=? AND s.event_type='SELL' AND s.actor=c.creator", (run_id,)).fetchone()[0]


def doctor_fixture_checks(cfg: RuntimeConfig) -> dict[str, Any]:
    """D04 fixture part: creator-sell, completion and migration decoders on synthetic IDL-layout fixtures (separate from live counts)."""
    from darkbot.clock import ReplayClock, ReplayScheduler
    from darkbot.domain import EventType, VenueState
    from darkbot.market import MarketReducer, Normalizer, PumpDecoder, b58encode
    from darkbot.domain import RawEnvelope
    import base64, json as _json  # noqa: E401

    from darkbot.market import encode_fields, event_discriminator

    def pk(n: str) -> str:
        return b58encode(n.encode()[:32].ljust(32, b"\0"))

    mint, creator = pk("mint"), pk("creator")
    dec = PumpDecoder(cfg.protocol)
    norm = Normalizer(dec)
    red = MarketReducer("doctor", 8, 100)
    ev = cfg.protocol["events"]

    def frame(name: str, fields: list[tuple[str, Any]], sig: str, slot: int) -> RawEnvelope:
        payload = event_discriminator(name) + encode_fields(fields, ev[name]["prefix_fields"])
        logs = [f"Program {cfg.protocol['program_id']} invoke [1]", "Program data: " + base64.b64encode(payload).decode(), f"Program {cfg.protocol['program_id']} success"]
        return RawEnvelope("fixture", "c", slot, "", slot, _json.dumps({"method": "logsNotification", "params": {"result": {"context": {"slot": slot}, "value": {"signature": sig, "err": None, "logs": logs}}}}))

    results = {}
    c = frame("CreateEvent", [("name", "d"), ("symbol", "d"), ("uri", "d"), ("mint", mint), ("bonding_curve", pk("curve")), ("user", creator), ("creator", creator), ("timestamp", 1), ("virtual_token_reserves", 10**15), ("virtual_sol_reserves", 3 * 10**10), ("real_token_reserves", 7 * 10**14), ("token_total_supply", 10**15)], "s1", 1)
    r = red.apply(norm.normalize(c, "logsSubscribe", 1), c)
    results["create_decoded"] = bool(r.new_creates)
    s = frame("TradeEvent", [("mint", mint), ("sol_amount", 10**8), ("token_amount", 10**12), ("is_buy", False), ("user", creator), ("timestamp", 2), ("virtual_sol_reserves", 3 * 10**10 - 10**8), ("virtual_token_reserves", 10**15 + 10**12), ("real_sol_reserves", 0), ("real_token_reserves", 7 * 10**14 + 10**12)], "s2", 2)
    r = red.apply(norm.normalize(s, "logsSubscribe", 2), s)
    results["creator_sell_detected"] = mint in r.creator_sells
    comp = frame("CompleteEvent", [("user", creator), ("mint", mint), ("bonding_curve", pk("curve")), ("timestamp", 3)], "s3", 3)
    r = red.apply(norm.normalize(comp, "logsSubscribe", 3), comp)
    results["complete_detected"] = red.states[mint].venue_state == VenueState.CURVE_COMPLETE
    mig = frame("CompletePumpAmmMigrationEvent", [("user", creator), ("mint", mint)], "s4", 4)
    r = red.apply(norm.normalize(mig, "logsSubscribe", 4), mig)
    results["migration_detected"] = red.states[mint].venue_state == VenueState.MIGRATED_UNSUPPORTED
    results["all_pass"] = all(results.values())
    return results


async def run_doctor(cfg: RuntimeConfig, db_path: Path, campaign: str = "doctor") -> dict[str, Any]:
    d = cfg.doctor
    checks: dict[str, dict[str, Any]] = {}
    # D01
    conn = open_connection(db_path)
    try:
        migrations = apply_migrations(conn)
        integ = integrity_check(conn)
        readback = write_readback_test(conn)
        conn.close()
        du = shutil.disk_usage(db_path.parent)
        pkgs = {}
        try:
            import websockets  # type: ignore

            pkgs["websockets"] = getattr(websockets, "__version__", "unknown")
        except Exception as exc:  # noqa: BLE001
            pkgs["websockets"] = f"MISSING ({type(exc).__name__})"
        d01_pass = readback and integ["integrity_check"] == "ok" and not integ["foreign_key_check"] and set(REQUIRED_TABLES) <= set(integ["tables"]) and du.free > d["min_free_disk_bytes"] and not pkgs["websockets"].startswith("MISSING")
        checks["D01"] = {"status": "PASS" if d01_pass else "FAIL", "python": sys.version, "executable": sys.executable, "os": platform.platform(), "packages": pkgs, "config_hash": cfg.config_hash, "mode": cfg["mode"],
                         "live_broadcast_enabled": cfg["live_broadcast_enabled"], "monthly_paid_services_allowed": cfg["monthly_paid_services_allowed"], "disk_free_bytes": du.free, "migrations": migrations, "integrity": integ, "write_readback": readback}
    except Exception as exc:  # noqa: BLE001
        checks["D01"] = {"status": "FAIL", "error": f"{type(exc).__name__}: {exc}"}
    # D04 fixtures (offline)
    fixtures = doctor_fixture_checks(cfg)
    # D02/D03/D05/D06 need the live probe
    env_ok = resolve_feed_endpoints(cfg)
    rpc = resolve_rpc_http(cfg)
    app = Application(cfg, db_path, campaign, mode="DOCTOR", max_duration_ms=d["max_duration_ms"], economic=False)
    summary = await app.run()
    owner_status = summary["owner"]
    feed_states = {}
    for feed in cfg.feeds:
        rep = summary["feeds"].get(feed.id)
        if rep is None:
            feed_states[feed.id] = summary["feed_setup"].get(feed.id, "DISABLED")
        else:
            if rep["acks"] and rep["ack_failures"] == 0:
                feed_states[feed.id] = "READY" if rep["messages"] else "READY_NO_MESSAGES_INCONCLUSIVE"
            else:
                feed_states[feed.id] = "FAILED" if feed.required else f"DISABLED_OPTIONAL ({rep['last_error'] or 'no ack'})"
    eligible_ready = any(str(feed_states.get(f.id, "")).startswith("READY") for f in cfg.feeds if f.transport == "logsSubscribe")
    checks["D02"] = {"status": "PASS" if eligible_ready else "FAIL", "feeds": feed_states, "missing_env": {k: v.missing_env for k, v in env_ok.items() if v.missing_env}, "rpc_http": "SET" if rpc.available else f"MISSING {rpc.missing_env}",
                     "paid_subscriptions": "NONE (only logsSubscribe / subscribeNewToken / subscribeMigration)"}
    reader = open_connection(db_path, read_only=True)
    creates_observed = reader.execute("SELECT COUNT(*) FROM market_events WHERE run_id=? AND event_type='CREATE'", (app.run_id,)).fetchone()[0]
    creates = reader.execute("SELECT COUNT(*) FROM market_events WHERE run_id=? AND event_type='CREATE' AND actionable_mono_ns IS NOT NULL", (app.run_id,)).fetchone()[0]
    trades = reader.execute("""
        SELECT COUNT(*)
        FROM market_events AS m
        JOIN feed_observations AS o
          ON o.observation_id = m.source_observation_id
        WHERE m.run_id=?
          AND m.event_type IN ('BUY','SELL')
          AND o.validation_status='VALID'
          AND m.virtual_sol_reserves IS NOT NULL
    """, (app.run_id,)).fetchone()[0]
    scope = {r[0]: r[1] for r in reader.execute("SELECT quote_scope_status, COUNT(*) FROM market_events WHERE run_id=? AND event_type='CREATE' GROUP BY quote_scope_status", (app.run_id,))}
    tails = [r[0] for r in reader.execute("SELECT DISTINCT quote_layout_hash FROM market_events WHERE run_id=? AND event_type='CREATE'", (app.run_id,))]
    live_sells = count_live_creator_sells(reader, app.run_id)
    live_completes = reader.execute("SELECT COUNT(*) FROM market_events WHERE run_id=? AND event_type IN ('COMPLETE','MIGRATE')", (app.run_id,)).fetchone()[0]
    obs_count, lat = reader.execute("SELECT COUNT(*), AVG(decode_done_mono_ns - receive_mono_ns) FROM feed_observations WHERE run_id=? AND decode_done_mono_ns IS NOT NULL", (app.run_id,)).fetchone()
    fee_rows = [dict(r) for r in reader.execute("SELECT source_type, validation_status, fee_snapshot_id FROM fee_snapshots WHERE run_id=? ORDER BY received_mono_ns", (app.run_id,))]
    reader.close()
    d03 = creates >= d["minimum_valid_create_samples"] and trades >= d["minimum_valid_trade_samples"]
    checks["D03"] = {"status": "PASS" if d03 else ("INCONCLUSIVE" if eligible_ready else "FAIL"), "create_samples_actionable_sol": creates, "create_samples_observed_total": creates_observed, "trade_samples": trades, "sol_scope_counts": scope, "create_tail_layouts_seen": tails, "decoder_version": cfg.protocol["decoder_version"],
                     "idl": cfg.protocol["idl_version_or_hash"], "observations": obs_count, "avg_receive_to_decode_ns": lat, "note": "unpinned CreateEvent tails stay QUOTE_UNKNOWN_LAYOUT_UNVERIFIED (observe-only) until pinned in config.protocol.create_event_tail_layouts"}
    checks["D04"] = {"status": "PASS" if fixtures["all_pass"] else "FAIL", "fixture_checks": fixtures, "live_creator_sell_count": live_sells, "live_completion_or_migration_count": live_completes,
                     "live_evidence": "NOT_OBSERVED" if (live_sells + live_completes) == 0 else "OBSERVED", "note": "fixture success is not proof of live coverage"}
    latest = {}
    for r in fee_rows:
        latest[r["source_type"]] = r
    d05 = {"PROTOCOL_GLOBAL_ACCOUNT", "NETWORK_FEE", "RENT_EXEMPTION", "PRIORITY_POLICY"} <= {k for k, v in latest.items() if v["validation_status"] == "VALID"}
    supported_samples = summary.get("economic_profile_samples_at_run_end", 0)
    d05 = d05 and supported_samples > 0
    decoded = reader2 = None
    try:
        reader2 = open_connection(db_path, read_only=True)
        row = reader2.execute("SELECT raw_values, validation_status FROM fee_snapshots WHERE run_id=? AND source_type='PROTOCOL_GLOBAL_ACCOUNT' ORDER BY received_mono_ns DESC LIMIT 1", (app.run_id,)).fetchone()
        decoded = {"raw_values": row["raw_values"], "validation_status": row["validation_status"]} if row else None
        reader2.close()
    except Exception:  # noqa: BLE001
        pass
    checks["D05"] = {"status": "PASS" if d05 else "FAIL", "snapshots": latest, "cost_status": "VERIFIED" if d05 else "UNQUALIFIED_COSTS", "fee_errors": summary["fee_errors"], "no_transfers": True, "observed_economic_profile_samples": supported_samples,
                     "fee_probe_scope": "getFeeForMessage verifies network fees, not transaction execution or landing",
                     "global_account_verification_status": cfg.protocol["global_account"]["verification_status"], "decoded_global_account": decoded,
                     "note": "layout must be marked VERIFIED_* in config.protocol.global_account after checking decoded values against the official IDL; until then protocol fees stay LAYOUT_UNVERIFIED"}
    # D06: recorder/quality/report/offline replay of the same probe
    d06: dict[str, Any] = {"status": "FAIL"}
    try:
        from darkbot.replay import load_replay_input, run_replay, digest
        from darkbot.reporting import Reporter

        conn = open_connection(db_path)
        report = Reporter(conn).build(app.run_id)
        rin = load_replay_input(conn, app.run_id, cfg)
        res = run_replay(conn, rin, cfg, f"{app.run_id}_replay", campaign, mode="DOCTOR_REPLAY", persist=False)
        res2 = run_replay(conn, rin, cfg, f"{app.run_id}_replay2", campaign, mode="DOCTOR_REPLAY", persist=False)
        conn.close()
        reproducible = res.digest == res2.digest
        d06 = {"status": "PASS" if reproducible and summary["writer"]["committed_records"] > 0 and not summary["writer"]["disk_error"] else "FAIL", "persisted_observations": obs_count, "replay_reproducible": reproducible,
               "replay_creates": res.owner.counters.creates, "positions_booked_in_doctor": len(app.owner.positions) if app.owner else None, "report_qualification": report["qualification_status"], "writer": summary["writer"]}
    except Exception as exc:  # noqa: BLE001
        d06 = {"status": "FAIL", "error": f"{type(exc).__name__}: {exc}"}
    checks["D06"] = d06
    if checks["D01"]["status"] != "PASS" or not eligible_ready:
        readiness = Readiness.NOT_READY
    elif all(checks.get(cid, {}).get("status") == "PASS" for cid in ("D01", "D02", "D03", "D04", "D05", "D06")):
        readiness = Readiness.ECONOMIC_PAPER_READY
    else:
        readiness = Readiness.CAPTURE_READY
    # Persist the final integration gate, not only the earlier runtime cost/feed readiness.
    summary["runtime_readiness"] = summary.get("readiness")
    summary["readiness"] = readiness.value
    conn = open_connection(db_path)
    try:
        conn.execute("UPDATE runs SET readiness=? WHERE run_id=?", (readiness.value, app.run_id))
    finally:
        conn.close()
    return {"readiness": readiness.value, "run_id": app.run_id, "checks": checks, "summary": {k: v for k, v in summary.items() if k not in ("storage",)}, "note": "Short probe proves integration only, not completeness, reliability, landing latency or expectancy."}


def run_paper(cfg_path: Path, db_path: Path, campaign: str, max_open_positions: int | None = None) -> dict[str, Any]:
    cfg = load_runtime_config(cfg_path)
    if max_open_positions is not None:
        if max_open_positions <= 0:
            raise ConfigError("--max-open-positions must be > 0")
        cfg = cfg.with_overrides(portfolio={"max_open_positions": max_open_positions})
    app = Application(cfg, db_path, campaign, mode="PAPER")
    return asyncio.run(app.run())


def run_doctor_cli(cfg_path: Path, db_path: Path | None = None) -> dict[str, Any]:
    cfg = load_runtime_config(cfg_path)
    db = db_path or (cfg_path.resolve().parent.parent / "data" / "doctor.sqlite")
    return asyncio.run(run_doctor(cfg, db))

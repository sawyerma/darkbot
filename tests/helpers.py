"""Deterministic fixtures and harness shared by the T01–T52 tests (no network, no wall clock)."""

from __future__ import annotations

import base64
import json
import sqlite3
from pathlib import Path
from typing import Any

from darkbot.clock import ReplayClock, ReplayScheduler
from darkbot.config import RuntimeConfig, build_runtime_config, load_json_strict
from darkbot.domain import CostEvidence, CostBasis, FeeSnapshot, RawEnvelope, utc_iso
from darkbot.fees import FeeCache
from darkbot.fx import FxSnapshot
from darkbot.market import encode_fields, event_discriminator
from darkbot.paper import PaperExecutionAdapter, StateOwner
from darkbot.storage import Recorder, SqliteWriter, apply_migrations, open_connection, upsert_config_snapshot

ROOT = Path(__file__).resolve().parent.parent
PROGRAM = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"


def _pubkey(s: str) -> str:
    """Deterministic valid 32-byte pubkey from a short test name."""
    from darkbot.market import b58encode
    return b58encode(s.encode("ascii")[:32].ljust(32, b"\0"))


MINT_A, MINT_B, CREATOR, BUYER, BUYER2, CURVE = (_pubkey(n) for n in ("mintA", "mintB", "creator", "buyer", "buyer2", "curve"))
# Pump initial curve constants (technical fixture values, not trading parameters)
V_TOK0, V_SOL0, R_TOK0, SUPPLY = 1_073_000_000_000_000, 30_000_000_000, 793_100_000_000_000, 1_000_000_000_000_000
T0 = 1_000_000_000_000  # mono ns epoch start


def base_config(**overrides: dict[str, Any]) -> RuntimeConfig:
    raw = load_json_strict(ROOT / "config" / "solana_pump" / "phase1.json")
    for section, values in overrides.items():
        raw[section].update(values)
    return build_runtime_config(ROOT / "config" / "solana_pump" / "phase1.json", raw)


def create_frame_logs(mint: str, creator: str, vsol: int = V_SOL0, vtok: int = V_TOK0, rtok: int = R_TOK0, tail: bytes = b"", initial_buy: tuple[int, int] | None = None, extra_trades: list[dict[str, Any]] | None = None, creator_sell: dict[str, Any] | None = None) -> list[str]:
    layout = [["name", "string"], ["symbol", "string"], ["uri", "string"], ["mint", "pubkey"], ["bonding_curve", "pubkey"], ["user", "pubkey"], ["creator", "pubkey"], ["timestamp", "i64"], ["virtual_token_reserves", "u64"], ["virtual_sol_reserves", "u64"], ["real_token_reserves", "u64"], ["token_total_supply", "u64"]]
    payload = event_discriminator("CreateEvent") + encode_fields([("name", "T"), ("symbol", "T"), ("uri", "u"), ("mint", mint), ("bonding_curve", _pubkey(CURVE)), ("user", creator), ("creator", creator), ("timestamp", 1), ("virtual_token_reserves", vtok), ("virtual_sol_reserves", vsol), ("real_token_reserves", rtok), ("token_total_supply", SUPPLY)], layout) + tail
    logs = [f"Program {PROGRAM} invoke [1]", "Program log: Instruction: Create", "Program data: " + base64.b64encode(payload).decode()]
    state = {"vsol": vsol, "vtok": vtok, "rtok": rtok}
    if initial_buy is not None:
        sol, tok = initial_buy
        state["vsol"] += sol; state["vtok"] -= tok; state["rtok"] -= tok
        logs.append(trade_log_line(mint, sol, tok, True, creator, state["vsol"], state["vtok"], state["rtok"]))
    for t in extra_trades or []:
        logs.append(trade_log_line(mint, t["sol"], t["tok"], t["is_buy"], t["user"], t["vsol"], t["vtok"], t["rtok"]))
    if creator_sell is not None:
        t = creator_sell
        logs.append(trade_log_line(mint, t["sol"], t["tok"], False, creator, t["vsol"], t["vtok"], t["rtok"]))
    logs.append(f"Program {PROGRAM} success")
    return logs


def trade_log_line(mint: str, sol: int, tok: int, is_buy: bool, user: str, vsol: int, vtok: int, rtok: int, tail: bytes = b"") -> str:
    layout = [["mint", "pubkey"], ["sol_amount", "u64"], ["token_amount", "u64"], ["is_buy", "bool"], ["user", "pubkey"], ["timestamp", "i64"], ["virtual_sol_reserves", "u64"], ["virtual_token_reserves", "u64"], ["real_sol_reserves", "u64"], ["real_token_reserves", "u64"]]
    payload = event_discriminator("TradeEvent") + encode_fields([("mint", mint), ("sol_amount", sol), ("token_amount", tok), ("is_buy", is_buy), ("user", user), ("timestamp", 1), ("virtual_sol_reserves", vsol), ("virtual_token_reserves", vtok), ("real_sol_reserves", max(0, vsol - V_SOL0)), ("real_token_reserves", rtok)], layout) + tail
    return "Program data: " + base64.b64encode(payload).decode()


def trade_frame_logs(mint: str, sol: int, tok: int, is_buy: bool, user: str, vsol: int, vtok: int, rtok: int, tail: bytes = b"") -> list[str]:
    return [f"Program {PROGRAM} invoke [1]", trade_log_line(mint, sol, tok, is_buy, user, vsol, vtok, rtok, tail), f"Program {PROGRAM} success"]


def complete_frame_logs(mint: str, user: str = BUYER) -> list[str]:
    layout = [["user", "pubkey"], ["mint", "pubkey"], ["bonding_curve", "pubkey"], ["timestamp", "i64"]]
    payload = event_discriminator("CompleteEvent") + encode_fields([("user", user), ("mint", mint), ("bonding_curve", _pubkey(CURVE)), ("timestamp", 1)], layout)
    return [f"Program {PROGRAM} invoke [1]", "Program data: " + base64.b64encode(payload).decode(), f"Program {PROGRAM} success"]


def migrate_frame_logs(mint: str, user: str = BUYER) -> list[str]:
    layout = [["user", "pubkey"], ["mint", "pubkey"]]
    payload = event_discriminator("CompletePumpAmmMigrationEvent") + encode_fields([("user", user), ("mint", mint)], layout) + b"\0" * 40
    return [f"Program {PROGRAM} invoke [1]", "Program data: " + base64.b64encode(payload).decode(), f"Program {PROGRAM} success"]


def logs_notification(signature: str, slot: int, logs: list[str], err: Any = None) -> str:
    return json.dumps({"jsonrpc": "2.0", "method": "logsNotification", "params": {"subscription": 1, "result": {"context": {"slot": slot}, "value": {"signature": signature, "err": err, "logs": logs}}}})


def pumpportal_create(mint: str, signature: str = "ppsig") -> str:
    return json.dumps({"signature": signature, "mint": mint, "txType": "create", "traderPublicKey": "x", "pool": "pump"})


def pumpportal_migrate(mint: str) -> str:
    return json.dumps({"signature": "ppmig", "mint": mint, "txType": "migrate", "pool": "pump-amm"})


def fee_basis(cfg: RuntimeConfig, mono: int, protocol_bps: int = 100, creator_bps: int = 30, cu_price: int = 10_000, rent: int = 2_039_280, status: str = "VALID", run_id: str = "run") -> FeeCache:
    def snap(kind: str, stype: str, method: str, norm: dict[str, Any], ev: CostEvidence, max_age_ms: int) -> FeeSnapshot:
        return FeeSnapshot(f"{run_id}_fee_{kind}", stype, "FIXTURE", method, (), 1, utc_iso(0), mono, "processed", PROGRAM, "fixture", {"fixture": True}, norm, cfg.protocol["fee_rounding"], "FIXTURE", cfg.freshness_policy_hash, status, cfg.cost_model_hash, max_age_ms * 1_000_000, ev)
    prio = -(-(cfg.execution["priority_policy"]["compute_unit_limit"] * cu_price) // 1_000_000)
    rp = cfg.costs["snapshot_refresh_policy"]
    return FeeCache(
        protocol=snap("protocol", "PROTOCOL_GLOBAL_ACCOUNT", "getAccountInfo", {"protocol_fee_bps": protocol_bps, "creator_fee_bps": creator_bps, "other_protocol_fee_bps": 0}, CostEvidence.MEASURED, rp["protocol_max_age_ms"]),
        network=snap("network", "NETWORK_FEE", "getFeeForMessage", {"base_fee_lamports": 5000, "priority_fee_lamports": prio, "network_fee_total_lamports": 5000 + prio}, CostEvidence.MEASURED, rp["network_fee_max_age_ms"]),
        rent=snap("rent", "RENT_EXEMPTION", "getMinimumBalanceForRentExemption", {"token_account_rent_lamports": rent, "account_size_bytes": 165}, CostEvidence.MEASURED, rp["rent_max_age_ms"]),
        priority_policy=snap("priority", "PRIORITY_POLICY", "getRecentPrioritizationFees", {"compute_unit_price_micro_lamports": cu_price, "compute_unit_limit": cfg.execution["priority_policy"]["compute_unit_limit"]}, CostEvidence.MEASURED, rp["network_fee_max_age_ms"]),
    )


class Harness:
    """StateOwner + ReplayClock/Scheduler + Recorder + in-memory SQLite writer; feeds frames deterministically."""

    def __init__(self, cfg: RuntimeConfig | None = None, run_id: str = "run", fees: FeeCache | None = None, db_path: str = ":memory:", mode: str = "PAPER", economic: bool = True, campaign: str = "camp") -> None:
        self.cfg = cfg or base_config()
        self.run_id = run_id
        self.clock = ReplayClock("epoch_test", T0, 1_700_000_000_000_000_000)
        self.scheduler = ReplayScheduler()
        self.recorder = Recorder(self.cfg.storage["queue_capacity_records"], self.cfg.storage["priority_headroom_records"])
        self.conn = sqlite3.connect(db_path, check_same_thread=False, isolation_level=None) if db_path == ":memory:" else open_connection(Path(db_path))
        self.conn.execute("PRAGMA foreign_keys=ON")
        self.conn.row_factory = sqlite3.Row
        apply_migrations(self.conn)
        for kind, (h, content) in self.cfg.snapshot_kinds().items():
            upsert_config_snapshot(self.conn, kind, h, content, "test", utc_iso(0))
        from darkbot.storage import prior_campaign_mints
        prior = prior_campaign_mints(self.conn, campaign) if mode == "PAPER" else set()
        self.conn.execute("INSERT INTO runs(run_id,campaign_id,mode,started_utc,code_revision,spec_hash,config_hash,host_clock_epoch_id,status,quality_status,start_balance_lamports,started_mono_ns) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
                          (run_id, campaign, mode, utc_iso(1_700_000_000_000_000_000), "test", "spec", self.cfg.config_hash, "epoch_test", "RUNNING", "OK", self.cfg.portfolio["initial_paper_balance_lamports"], T0))
        self.conn.execute("UPDATE runs SET strategy_id=?,strategy_config_hash=? WHERE run_id=?", (self.cfg.strategy_id, self.cfg.strategy_config_hash, run_id))
        self.writer = SqliteWriter(self.conn, self.recorder, 500, 100, self._disk_error, self.cfg.storage["writer_locked_retries"], self.cfg.storage["fk_merge_max_batches"])
        self.fees = fees if fees is not None else fee_basis(self.cfg, T0, run_id=run_id)
        for s in (self.fees.protocol, self.fees.network, self.fees.rent, self.fees.priority_policy):
            if s is not None:
                self.recorder.try_append(("fee_snapshot", (s, run_id)))
        self.adapter = PaperExecutionAdapter(self.clock, self.scheduler, mode, False, self.cfg.latency_model["entry_delay_ms"] * 1_000_000, self.cfg.latency_model["exit_delay_ms"] * 1_000_000)
        self.owner = StateOwner(self.cfg, run_id, self.clock, self.scheduler, self.recorder, self.adapter, lambda: self.fees.basis(), mode, economic_intents_allowed=economic, prior_submitted_mints=prior)
        if self.cfg.portfolio["initial_paper_balance_mode"] == "USD_PER_SLOT":
            self.owner.on_fx_snapshot(FxSnapshot.from_payload('{"data":{"base":"SOL","currency":"USD","amount":"100"}}',
                                                              self.cfg["fx"]["source_url"], self.clock.utc_iso(), T0))
        # emulate the background fee resolver: snapshots are re-read every network_fee_refresh_interval (same ids, new receive time) unless auto_refresh is off
        self.auto_refresh = fees is None
        self.refresh_interval_ns = self.cfg.costs["snapshot_refresh_policy"]["network_fee_refresh_interval_ms"] * 1_000_000
        self.last_refresh = self.fees.network.received_mono_ns if self.fees.network else T0
        self.owner.on_fee_snapshots([x for x in (self.fees.protocol, self.fees.network, self.fees.rent, self.fees.priority_policy) if x is not None])
        self.seq = {}
        self.disk_errors: list[str] = []
        for f in self.cfg.feeds:
            self.owner.on_feed_health(f.id, True, T0)

    def _disk_error(self, msg: str) -> None:
        self.disk_errors.append(msg)
        self.owner.on_disk_error(msg)

    def refresh_fees(self, now: int) -> None:
        """Background resolver emulation: same fee rule re-read at `now` -> new immutable snapshot (new id, persisted), exactly like Application._on_fee_snapshots."""
        from darkbot.domain import FeeSnapshot as _FS
        self.refresh_n = getattr(self, "refresh_n", 0) + 1
        def clone(x):
            base_id = x.fee_snapshot_id.split("_r")[0]
            return _FS(**{**{k: getattr(x, k) for k in x.__slots__}, "received_mono_ns": now, "fee_snapshot_id": f"{base_id}_r{self.refresh_n}"})
        new = FeeCache(*[(clone(x) if x is not None else None) for x in (self.fees.protocol, self.fees.network, self.fees.rent, self.fees.priority_policy)])
        self.fees = new
        self.last_refresh = now
        snaps = [x for x in (new.protocol, new.network, new.rent, new.priority_policy) if x is not None]
        for x in snaps:
            self.recorder.try_append(("fee_snapshot", (x, self.run_id)))
        self.owner.on_fee_snapshots(snaps)

    def _tick_fees(self, now: int) -> None:
        if self.auto_refresh and now - self.last_refresh >= self.refresh_interval_ns:
            self.refresh_fees(now)

    def advance(self, mono: int, late_ns: int = 0, strict: bool = False) -> None:
        """Fire timers due up to `mono` (strictly before when strict), then move the clock; late_ns models late callbacks (>=).
        Fee snapshots are refreshed on the way exactly like the live background resolver would."""
        due = self.scheduler.next_due_ns()
        while due is not None and (due < mono if strict else due <= mono):
            nxt = self.last_refresh + self.refresh_interval_ns
            while self.auto_refresh and nxt < due:
                self.clock.advance_to(max(self.clock.monotonic_ns(), nxt))
                self.refresh_fees(nxt)
                nxt = self.last_refresh + self.refresh_interval_ns
            self.clock.advance_to(max(self.clock.monotonic_ns(), due + late_ns))
            self._tick_fees(due)
            self.scheduler.fire_due(due, self.clock.monotonic_ns())
            due = self.scheduler.next_due_ns()
        nxt = self.last_refresh + self.refresh_interval_ns
        while self.auto_refresh and nxt <= mono:
            self.clock.advance_to(max(self.clock.monotonic_ns(), nxt))
            self.refresh_fees(nxt)
            nxt = self.last_refresh + self.refresh_interval_ns
        self.clock.advance_to(max(mono, self.clock.monotonic_ns()))

    def feed(self, feed_id: str, payload: str, mono: int, late_ns: int = 0) -> Any:
        self.advance(mono, late_ns, strict=True)  # same model time: atomic market/venue state first, then scheduled fills
        self.seq[feed_id] = self.seq.get(feed_id, 0) + 1
        raw = RawEnvelope(feed_id, f"{feed_id}-c1", mono, utc_iso(self.clock.utc_ns()), self.seq[feed_id], payload)
        return self.owner.on_raw(raw)

    def flush(self) -> None:
        self.writer.flush_sync()

    def rows(self, sql: str, params: tuple = ()) -> list[sqlite3.Row]:
        self.flush()
        return self.conn.execute(sql, params).fetchall()

    def fk_check(self) -> list[Any]:
        self.flush()
        return self.conn.execute("PRAGMA foreign_key_check").fetchall()

    def position(self):
        return next(iter(self.owner.positions.values()))

    def finish(self, mono: int) -> None:
        """End of recording: finalize the owner and persist the run end (ended_mono_ns) exactly like Application.run."""
        self.advance(mono)
        self.owner.finalize(mono)
        self.flush()
        self.conn.execute("UPDATE runs SET status='COMPLETED', ended_utc=?, ended_mono_ns=?, end_balance_lamports=? WHERE run_id=?", (utc_iso(0), mono, self.owner.portfolio.cash_lamports + self.owner.portfolio.reserved_lamports, self.run_id))

    def report(self) -> dict[str, Any]:
        from darkbot.reporting import Reporter
        self.flush()
        return Reporter(self.conn).build(self.run_id)


# --------------------------------------------------------------------------- scenario helpers

def create_payload(mint: str = MINT_A, creator: str = CREATOR, sig: str = "sigC", slot: int = 100, **kw: Any) -> str:
    return logs_notification(sig, slot, create_frame_logs(mint, creator, **kw))


def trade_after(h: Harness, mint: str, user: str, sol: int, tok: int, is_buy: bool, sig: str, slot: int) -> str:
    """Frame for a trade applied on top of the current RAM state (no counterfactual reaction to our own paper trade)."""
    st = h.owner.reducer.states[mint]
    if is_buy:
        vsol, vtok, rtok = st.virtual_sol_reserves + sol, st.virtual_token_reserves - tok, st.real_token_reserves - tok
    else:
        vsol, vtok, rtok = st.virtual_sol_reserves - sol, st.virtual_token_reserves + tok, st.real_token_reserves + tok
    return logs_notification(sig, slot, trade_frame_logs(mint, sol, tok, is_buy, user, vsol, vtok, rtok))


def pump_frame(h: Harness, mint: str, sol: int, sig: str, slot: int, user: str = BUYER) -> str:
    st = h.owner.reducer.states[mint]
    tok = sol * st.virtual_token_reserves // (st.virtual_sol_reserves + sol)
    return trade_after(h, mint, user, sol, tok, True, sig, slot)


def dump_frame(h: Harness, mint: str, tok: int, sig: str, slot: int, user: str = BUYER2) -> str:
    st = h.owner.reducer.states[mint]
    sol = tok * st.virtual_sol_reserves // (st.virtual_token_reserves + tok)
    return trade_after(h, mint, user, sol, tok, False, sig, slot)


def creator_sell_frame(h: Harness, mint: str, sig: str, slot: int, tok: int = 5_000_000_000_000) -> str:
    return dump_frame(h, mint, tok, sig, slot, user=CREATOR)


def open_position(h: Harness, mono: int = T0 + 1_000_000, mint: str = MINT_A, feed: str = "helius_standard", fill_at: int | None = None):
    h.feed(feed, create_payload(mint), mono)
    h.advance(fill_at if fill_at is not None else mono)
    return h.owner.positions[h.owner.position_by_mint[mint]]


def ledger_kinds(h: Harness) -> list[str]:
    return [r["kind"] for r in h.rows("select kind from ledger order by mono_ns, ledger_entry_id")]


def attempts(h: Harness) -> list[dict[str, Any]]:
    return [dict(r) for r in h.rows("select * from execution_attempts order by submit_mono_ns, attempt_id")]

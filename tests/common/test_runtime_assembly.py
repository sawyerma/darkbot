"""Phase A (BSC handoff §21.2/§5): runtime assembly — Solana profile executable, BSC profiles fail explicitly,
DI seam preserves legacy StateOwner behaviour and the existing replay `engine` injection."""

from __future__ import annotations

import unittest

from darkbot.clock import LiveClock, ReplayClock, ReplayScheduler
from darkbot.solana_pump.config import build_runtime_config, load_json_strict
from darkbot.solana_pump.execution import QuoteEngine
from darkbot.solana_pump.market import MarketReducer, PumpDecoder
from darkbot.solana_pump.paper import PaperExecutionAdapter, StateOwner
from darkbot.solana_pump.replay import digest, load_replay_input, run_replay
from darkbot.runtime.assembly import MarketProfileNotRunnable, RuntimeCallbacks, RuntimeComponents, assemble_runtime, build_feeds, effective_market_profile_id, resolve_market_profile
from darkbot.solana_pump.storage import Recorder
from tests.helpers import BUYER, MINT_A, ROOT, T0, Harness, base_config, create_payload, creator_sell_frame, fee_basis, pump_frame


def _profile_cfg(profile_id):
    raw = load_json_strict(ROOT / "config" / "solana_pump" / "phase1.json")
    return build_runtime_config(ROOT / "config" / "solana_pump" / "phase1.json", {**raw, "market_profile_id": profile_id})


class RuntimeAssemblyTests(unittest.TestCase):
    def test_legacy_config_assembles_solana_pump_components(self):
        cfg = base_config()
        self.assertEqual(effective_market_profile_id(cfg), "solana-mainnet:pumpfun")
        self.assertEqual(resolve_market_profile(cfg).display_name, "Solana / Pump.fun")
        comps = assemble_runtime(cfg=cfg, clock=LiveClock(), run_id="run", callbacks=None)
        self.assertIsInstance(comps, RuntimeComponents)
        self.assertEqual((comps.chain.identity.chain_id, comps.venue.identity.venue_id), ("solana-mainnet", "pumpfun"))
        self.assertIsInstance(comps.decoder, PumpDecoder)
        self.assertIsInstance(comps.reducer, MarketReducer)
        self.assertIsInstance(comps.quote_engine, QuoteEngine)
        self.assertEqual(comps.reducer.run_id, "run")
        self.assertEqual(comps.feeds, ())
        calls = []
        feeds = build_feeds(comps, cfg=cfg, clock=LiveClock(), callbacks=RuntimeCallbacks(on_raw=calls.append, on_health=lambda *a: None, on_error=lambda *a: None))
        self.assertEqual([f.feed.id for f in feeds], [fid for fid, st in comps.chain.feed_setup_status(cfg).items() if st == "STARTING"])
        self.assertEqual(assemble_runtime(cfg=cfg, clock=LiveClock(), run_id="run", callbacks=None, canonical_prefix="src").reducer.run_id, "src")

    def test_explicit_solana_profile_assembles(self):
        comps = assemble_runtime(cfg=_profile_cfg("solana-mainnet:pumpfun"), clock=LiveClock(), run_id="run", callbacks=None)
        self.assertEqual(comps.market_profile.profile_id, "solana-mainnet:pumpfun")

    def test_bsc_profiles_resolve_metadata_but_fail_runtime_construction(self):
        for pid, name in (("bsc-mainnet:flap", "BSC / Flap.sh"), ("bsc-mainnet:fourmeme", "BSC / Four.meme")):
            cfg = _profile_cfg(pid)
            profile = resolve_market_profile(cfg)  # metadata only
            self.assertEqual((profile.profile_id, profile.display_name, profile.status), (pid, name, "RESEARCH"))
            with self.assertRaises(MarketProfileNotRunnable):
                assemble_runtime(cfg=cfg, clock=LiveClock(), run_id="run", callbacks=None)
            with self.assertRaises(NotImplementedError):
                assemble_runtime(cfg=cfg, clock=LiveClock(), run_id="run", callbacks=None)
            # legacy fallback construction is Solana/Pump only: a StateOwner for a BSC profile must not silently decode Pump
            recorder = Recorder(cfg.storage["queue_capacity_records"], cfg.storage["priority_headroom_records"])
            clock, sched = ReplayClock("e", T0, 0), ReplayScheduler()
            adapter = PaperExecutionAdapter(clock, sched, "PAPER", False, 0, 0)
            with self.assertRaises(NotImplementedError):
                StateOwner(cfg, "run", clock, sched, recorder, adapter, lambda: fee_basis(cfg, T0).basis(), "PAPER")

    def test_venue_chain_mismatch_rejected(self):
        from darkbot.runtime import assembly
        from darkbot.venues.base import VenueIdentity

        class WrongChainVenue:
            identity = VenueIdentity("pumpfun", "bsc-mainnet", "x")

        original = assembly.load_venue
        assembly.load_venue = lambda vid: WrongChainVenue()
        try:
            with self.assertRaises(ValueError) as ctx:
                assembly.assemble_runtime(cfg=base_config(), clock=LiveClock(), run_id="run", callbacks=None)
            self.assertIn("venue/chain mismatch", str(ctx.exception))
        finally:
            assembly.load_venue = original

    def test_injected_components_are_used_directly_and_fallback_is_identical(self):
        cfg = base_config()
        comps = assemble_runtime(cfg=cfg, clock=LiveClock(), run_id="run", callbacks=None)
        h = Harness()  # legacy construction (no injection)
        self.addCleanup(h.conn.close)
        recorder = Recorder(cfg.storage["queue_capacity_records"], cfg.storage["priority_headroom_records"])
        clock, sched = ReplayClock("epoch_test", T0, 1_700_000_000_000_000_000), ReplayScheduler()
        adapter = PaperExecutionAdapter(clock, sched, "PAPER", False, 0, 0)
        fees = fee_basis(cfg, T0)
        owner = StateOwner(cfg, "run", clock, sched, recorder, adapter, fees.basis, "PAPER", engine=comps.quote_engine, decoder=comps.decoder, reducer=comps.reducer, market_profile=comps.market_profile)
        self.assertIs(owner.decoder, comps.decoder)
        self.assertIs(owner.reducer, comps.reducer)
        self.assertIs(owner.engine, comps.quote_engine)
        self.assertIs(owner.normalizer.decoder, comps.decoder)
        self.assertEqual(owner.market_profile.profile_id, "solana-mainnet:pumpfun")
        self.assertIsNone(h.owner.market_profile)
        self.assertEqual(type(owner.decoder), type(h.owner.decoder))
        self.assertEqual(type(owner.reducer), type(h.owner.reducer))
        self.assertEqual(owner.decoder.layout_hash, h.owner.decoder.layout_hash)
        self.assertEqual(owner.engine.policy, h.owner.engine.policy)

    def test_injected_and_legacy_state_owner_produce_identical_replay_digest(self):
        # same recording through (a) legacy fallback construction and (b) assembled Pump.fun components -> identical RAM digest
        h = Harness()
        self.addCleanup(h.conn.close)
        h.feed("helius_standard", create_payload(MINT_A), T0 + 1_000_000)
        h.advance(T0 + 1_000_000)
        h.feed("helius_standard", pump_frame(h, MINT_A, 500_000_000, "sig2", 101, user=BUYER), T0 + 400_000_000)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "sig3", 102), T0 + 900_000_000)
        h.finish(T0 + 2_000_000_000)
        data = load_replay_input(h.conn, h.run_id, h.cfg)
        legacy = run_replay(h.conn, data, h.cfg, "rep_legacy", "camp", persist=False)
        self.assertGreaterEqual(len(legacy.digest["positions"]), 1)
        cfg = h.cfg
        comps = assemble_runtime(cfg=cfg, clock=LiveClock(), run_id="rep_inj", callbacks=None, canonical_prefix=h.run_id)
        clock = ReplayClock("replay_rep_inj", data.start_mono_ns, data.start_utc_ns)
        sched = ReplayScheduler()
        recorder = Recorder(cfg.storage["queue_capacity_records"], cfg.storage["priority_headroom_records"])
        adapter = PaperExecutionAdapter(clock, sched, "PAPER", False, 0, 0, lateness_provider=lambda aid: 0)
        fees = fee_basis(cfg, T0)
        owner = StateOwner(cfg, "rep_inj", clock, sched, recorder, adapter, fees.basis, "PAPER", canonical_prefix=h.run_id, persist_observations=False, compute_features=False, engine=comps.quote_engine, decoder=comps.decoder, reducer=comps.reducer, market_profile=comps.market_profile)
        owner.on_fee_snapshots([s for s in (fees.protocol, fees.network, fees.rent, fees.priority_policy) if s is not None])
        for f in cfg.feeds:
            owner.on_feed_health(f.id, True, data.start_mono_ns)
        from darkbot.solana_pump.domain import RawEnvelope
        for receive, seq, feed_id, conn_id, utc, payload in data.observations:
            while (due := sched.next_due_ns()) is not None and due < receive:
                clock.advance_to(max(clock.monotonic_ns(), due))
                sched.fire_due(due, clock.monotonic_ns())
            clock.advance_to(max(clock.monotonic_ns(), receive))
            owner.on_raw(RawEnvelope(feed_id, conn_id, receive, utc, seq, payload))
        while (due := sched.next_due_ns()) is not None and due <= data.end_mono_ns:
            clock.advance_to(max(clock.monotonic_ns(), due))
            sched.fire_due(due, clock.monotonic_ns())
        clock.advance_to(max(clock.monotonic_ns(), data.end_mono_ns))
        owner.finalize(clock.monotonic_ns())
        self.assertEqual(digest(owner), legacy.digest)

    def test_replay_keeps_its_own_engine_injection_and_stays_offline(self):
        import inspect
        from darkbot import replay
        src = inspect.getsource(replay.run_replay)
        self.assertIn("engine=engine", src)  # existing replay injection preserved
        self.assertNotIn("assemble_runtime", src)  # replay is not forced through the generic live assembly path
        self.assertNotIn("http_rpc", src)


if __name__ == "__main__":
    unittest.main()

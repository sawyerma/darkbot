"""Phase A (BSC handoff §21.2): venue registry — Pump.fun executable, Flap/Four.meme metadata only, unknown rejected."""

from __future__ import annotations

import unittest

from darkbot.solana_pump.execution import QuoteEngine
from darkbot.solana_pump.market import MarketReducer, PumpDecoder
from darkbot.venues.base import VenueIdentity
from darkbot.solana_pump.venue.venue import PumpFunVenueAdapter
from darkbot.venues.registry import UnknownVenue, VenueNotImplemented, load_venue, venue_identity
from tests.solana_pump.helpers import CREATOR, MINT_A, Harness, create_payload, T0


class VenueRegistryTests(unittest.TestCase):
    def test_unknown_venue_rejected(self):
        with self.assertRaises(UnknownVenue):
            load_venue("raydium")
        with self.assertRaises(ValueError):
            venue_identity("BSC2")

    def test_flap_and_fourmeme_are_metadata_only(self):
        self.assertEqual(venue_identity("flap"), VenueIdentity("flap", "bsc-mainnet", "Flap.sh"))
        self.assertEqual(venue_identity("fourmeme"), VenueIdentity("fourmeme", "bsc-mainnet", "Four.meme"))
        for vid in ("flap", "fourmeme"):
            with self.assertRaises(VenueNotImplemented):
                load_venue(vid)
            with self.assertRaises(NotImplementedError):
                load_venue(vid)

    def test_no_flap_or_fourmeme_package_exists_in_phase_a(self):
        import importlib
        for mod in ("darkbot.venues.flap", "darkbot.venues.fourmeme", "darkbot.chains.evm", "darkbot.chains.bsc"):
            with self.assertRaises(ModuleNotFoundError):
                importlib.import_module(mod)

    def test_pumpfun_wraps_existing_components(self):
        h = Harness()
        self.addCleanup(h.conn.close)
        venue = load_venue("pumpfun")
        self.assertIsInstance(venue, PumpFunVenueAdapter)
        self.assertEqual(venue.identity, VenueIdentity("pumpfun", "solana-mainnet", "Pump.fun"))
        decoder = venue.build_decoder(cfg=h.cfg)
        reducer = venue.build_reducer(cfg=h.cfg, run_id="prefix")
        engine = venue.build_quote_engine(cfg=h.cfg)
        self.assertIsInstance(decoder, PumpDecoder)
        self.assertIsInstance(reducer, MarketReducer)
        self.assertIsInstance(engine, QuoteEngine)
        # exactly the StateOwner legacy construction
        own = h.owner.reducer
        self.assertEqual(reducer.run_id, "prefix")
        self.assertEqual((reducer.history_per_mint, reducer.max_tracked_mints, reducer.trade_log_per_mint, reducer.event_points_max), (own.history_per_mint, own.max_tracked_mints, own.trade_log_per_mint, own.event_points_max))
        self.assertEqual((reducer.signature_history_max, reducer.dedup_keys_per_mint, reducer.wsol, reducer.native_sentinel), (own.signature_history_max, own.dedup_keys_per_mint, own.wsol, own.native_sentinel))
        self.assertEqual(decoder.layout_hash, h.owner.decoder.layout_hash)
        self.assertEqual(engine.policy, h.owner.engine.policy)

    def test_actionable_launch_and_creator_match_state_owner_predicate(self):
        h = Harness()
        self.addCleanup(h.conn.close)
        venue = load_venue("pumpfun")
        h.feed("helius_standard", create_payload(MINT_A), T0 + 1_000_000)
        st = h.owner.reducer.states[MINT_A]
        self.assertTrue(venue.is_actionable_launch(st))
        self.assertEqual(venue.creator_of(st), CREATOR)
        self.assertEqual(h.owner.counters.creates_actionable, 1)


if __name__ == "__main__":
    unittest.main()

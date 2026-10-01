"""Phase A (BSC handoff §7.1/§21.2): generic identity/amount mapping for Solana without renaming any legacy field or column.

asset_id = mint, tx_id = signature, block_ref = slot, native_amount_base_units = sol_amount, native = SOL / 9 decimals.
Domain dataclasses and database columns are unchanged (additive generic columns are deferred to a later milestone).
"""

from __future__ import annotations

import json
import unittest

from darkbot.chains.registry import chain_identity, load_chain
from darkbot.solana_pump.domain import LAMPORTS_PER_SOL, NormalizedEvent, NormalizedEnvelope, RawEnvelope
from darkbot.venues.registry import load_venue
from tests.solana_pump.helpers import BUYER, MINT_A, T0, Harness, create_payload, pump_frame


class MultichainDomainTests(unittest.TestCase):
    def test_native_unit_metadata(self):
        sol = chain_identity("solana-mainnet")
        self.assertEqual((sol.native_symbol, sol.native_decimals, 10 ** sol.native_decimals), ("SOL", 9, LAMPORTS_PER_SOL))
        bnb = chain_identity("bsc-mainnet")
        self.assertEqual((bnb.family, bnb.native_symbol, bnb.native_decimals), ("evm", "BNB", 18))

    def test_solana_tx_id_and_block_ref_from_raw(self):
        chain = load_chain("solana-mainnet")
        payload = create_payload(MINT_A, sig="sigXYZ", slot=4242)
        raw = RawEnvelope("helius_standard", "c1", T0, "1970-01-01T00:00:00Z", 1, payload)
        self.assertEqual(chain.tx_id_from_raw(raw), "sigXYZ")
        self.assertEqual(chain.block_ref_from_raw(raw), 4242)
        self.assertIsNone(chain.tx_id_from_raw(RawEnvelope("f", "c", T0, "", 1, "{not json")))
        self.assertIsNone(chain.block_ref_from_raw(RawEnvelope("f", "c", T0, "", 1, json.dumps({"id": 1, "result": 7}))))

    def test_pumpfun_asset_id_and_native_amount_mapping_against_decoded_events(self):
        h = Harness()
        self.addCleanup(h.conn.close)
        venue = load_venue("pumpfun")
        chain = load_chain("solana-mainnet")
        h.feed("helius_standard", create_payload(MINT_A, sig="sigC", slot=100), T0 + 1_000_000)
        h.advance(T0 + 1_000_000)
        payload = pump_frame(h, MINT_A, 123_456_789, "sigB", 101, user=BUYER)
        raw = RawEnvelope("helius_standard", "c1", T0 + 2_000_000, "1970-01-01T00:00:00Z", 2, payload)
        env = h.owner.normalizer.normalize(raw, "logsSubscribe", T0 + 2_000_000)
        self.assertIsInstance(env, NormalizedEnvelope)
        buy = next(e for e in env.events if e.event_type.value == "BUY")
        self.assertIsInstance(buy, NormalizedEvent)
        self.assertEqual(venue.asset_id_of(buy), MINT_A)
        self.assertEqual(venue.asset_id_of(h.owner.reducer.states[MINT_A]), MINT_A)
        self.assertEqual(venue.native_amount_base_units_of(buy), 123_456_789)
        self.assertEqual(chain.tx_id_from_raw(raw), env.signature)
        self.assertEqual(chain.block_ref_from_raw(raw), env.slot)
        # legacy fields untouched
        self.assertEqual(buy.mint, MINT_A)
        self.assertEqual(buy.sol_amount, 123_456_789)


if __name__ == "__main__":
    unittest.main()

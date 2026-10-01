"""Phase A (BSC handoff §21.2): chain registry — Solana executable, BSC metadata only, unknown rejected."""

from __future__ import annotations

import unittest

from darkbot.solana_pump.capture import FeedAdapter
from darkbot.chains.base import ChainIdentity
from darkbot.chains.registry import CHAIN_IDENTITIES, ChainNotImplemented, UnknownChain, chain_identity, load_chain
from darkbot.solana.chain import SolanaChainAdapter
from darkbot.clock import LiveClock
from darkbot.solana_pump.fees import FeeResolver
from tests.solana_pump.helpers import base_config


class ChainRegistryTests(unittest.TestCase):
    def test_unknown_chain_rejected(self):
        with self.assertRaises(UnknownChain):
            load_chain("ethereum-mainnet")
        with self.assertRaises(ValueError):
            chain_identity("BSC1")

    def test_bsc_is_metadata_only_and_fails_explicitly(self):
        ident = chain_identity("bsc-mainnet")
        self.assertEqual(ident, ChainIdentity("bsc-mainnet", "evm", "BNB", 18))
        with self.assertRaises(ChainNotImplemented):
            load_chain("bsc-mainnet")
        with self.assertRaises(NotImplementedError):
            load_chain("bsc-mainnet")

    def test_solana_resolves_executable_adapter(self):
        chain = load_chain("solana-mainnet")
        self.assertIsInstance(chain, SolanaChainAdapter)
        self.assertEqual(chain.identity, ChainIdentity("solana-mainnet", "solana", "SOL", 9))
        self.assertEqual(chain.identity, CHAIN_IDENTITIES["solana-mainnet"])

    def test_solana_wraps_existing_fee_resolver_and_feeds(self):
        cfg = base_config()
        chain = load_chain("solana-mainnet")
        resolver = chain.build_fee_resolver(cfg=cfg, clock=LiveClock(), run_id="run")
        self.assertIsInstance(resolver, FeeResolver)  # existing implementation, not a copy
        self.assertEqual(resolver.run_id, "run")
        status = chain.feed_setup_status(cfg)
        self.assertEqual(set(status), {f.id for f in cfg.feeds})
        calls = []
        on_raw = calls.append
        feeds = chain.build_feed_adapters(cfg=cfg, clock=LiveClock(), on_raw=on_raw, on_health=lambda *a: None, on_error=lambda *a: None)
        self.assertTrue(all(isinstance(f, FeedAdapter) for f in feeds))
        self.assertEqual([f.feed.id for f in feeds], [fid for fid, st in status.items() if st == "STARTING"])
        for f in feeds:
            self.assertIs(f.on_raw, on_raw)  # consumer callback passed through directly: no adapter-introduced hop


if __name__ == "__main__":
    unittest.main()

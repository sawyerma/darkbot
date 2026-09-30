"""Phase A (BSC handoff §21.2): market profile registry + legacy config-hash compatibility."""

from __future__ import annotations

import json
import unittest
from pathlib import Path

from darkbot.config import ConfigError, build_runtime_config, load_json_strict, load_runtime_config
from darkbot.runtime.profiles import DEFAULT_MARKET_PROFILE_ID, MARKET_PROFILES, UnknownMarketProfile, load_market_profile, market_profile_catalog
from tests.helpers import ROOT, base_config

GOLDEN = ROOT / "tests" / "fixtures" / "config_hashes_baseline_05971e4.json"  # hashes frozen BEFORE the Phase-A change at baseline 05971e4


class MarketProfileRegistryTests(unittest.TestCase):
    def test_three_profiles_registered_with_stable_ids(self):
        self.assertEqual(set(MARKET_PROFILES), {"solana-mainnet:pumpfun", "bsc-mainnet:flap", "bsc-mainnet:fourmeme"})
        for pid, p in MARKET_PROFILES.items():
            self.assertEqual(p.profile_id, pid)
            self.assertEqual(pid, f"{p.chain_id}:{p.venue_id}")

    def test_solana_ready_bsc_research_metadata_only(self):
        self.assertEqual(load_market_profile("solana-mainnet:pumpfun").status, "READY")
        self.assertTrue(load_market_profile("solana-mainnet:pumpfun").runnable)
        for pid, name in (("bsc-mainnet:flap", "BSC / Flap.sh"), ("bsc-mainnet:fourmeme", "BSC / Four.meme")):
            p = load_market_profile(pid)
            self.assertEqual((p.status, p.display_name, p.chain_id), ("RESEARCH", name, "bsc-mainnet"))
            self.assertFalse(p.runnable)

    def test_unknown_profile_rejected(self):
        with self.assertRaises(UnknownMarketProfile):
            load_market_profile("bsc-mainnet:unknown")
        with self.assertRaises(ValueError):
            load_market_profile("BSC1")

    def test_catalog_exposes_stable_ids_and_status(self):
        cat = {c["id"]: c for c in market_profile_catalog()}
        self.assertEqual(cat["bsc-mainnet:flap"]["status"], "RESEARCH")
        self.assertEqual(cat["solana-mainnet:pumpfun"]["display_name"], "Solana / Pump.fun")


class LegacyConfigCompatibilityTests(unittest.TestCase):
    def test_legacy_config_without_profile_resolves_to_solana_pumpfun_without_mutating_raw(self):
        cfg = base_config()
        self.assertNotIn("market_profile_id", cfg.raw)
        self.assertNotIn("market_profile_id", cfg.canonical)
        self.assertFalse(cfg.has_explicit_market_profile)
        self.assertEqual(cfg.market_profile_id, DEFAULT_MARKET_PROFILE_ID)
        self.assertEqual(cfg.market_profile_id, "solana-mainnet:pumpfun")
        self.assertNotIn("market_profile_id", cfg.snapshot_kinds()["RUNTIME_CONFIG"][1])

    def test_all_shipped_configs_keep_their_pre_phase_a_hashes(self):
        golden = json.loads(GOLDEN.read_text())
        self.assertEqual(len(golden), 9)  # all 9 runtime configs shipped at baseline 05971e4 (research*.json are not runtime configs)
        for name, hashes in golden.items():
            cfg = load_runtime_config(ROOT / "config" / "solana_pump" / name)
            with self.subTest(config=name):
                self.assertEqual(cfg.market_profile_id, "solana-mainnet:pumpfun")
                for kind, (h, _content) in cfg.snapshot_kinds().items():
                    self.assertEqual(h, hashes[kind], f"{name}: {kind} hash changed")
                self.assertEqual(len(cfg.canonical), hashes["canonical_sha256_len"])

    def test_explicit_profile_participates_in_config_hash(self):
        raw = load_json_strict(ROOT / "config" / "solana_pump" / "phase1.json")
        legacy = build_runtime_config(ROOT / "config" / "solana_pump" / "phase1.json", raw)
        explicit = build_runtime_config(ROOT / "config" / "solana_pump" / "phase1.json", {**raw, "market_profile_id": "solana-mainnet:pumpfun"})
        self.assertTrue(explicit.has_explicit_market_profile)
        self.assertEqual(explicit.market_profile_id, legacy.market_profile_id)
        self.assertNotEqual(explicit.config_hash, legacy.config_hash)  # explicit metadata is part of the newly authored config's hash binding
        self.assertEqual(explicit.policy_hash, legacy.policy_hash)  # policy/cost bindings untouched
        self.assertEqual(explicit.cost_model_hash, legacy.cost_model_hash)
        flap = build_runtime_config(ROOT / "config" / "solana_pump" / "phase1.json", {**raw, "market_profile_id": "bsc-mainnet:flap"})
        self.assertEqual(flap.market_profile_id, "bsc-mainnet:flap")
        self.assertNotEqual(flap.config_hash, explicit.config_hash)  # config hash changes when the market profile changes

    def test_unknown_explicit_profile_is_a_config_error(self):
        raw = load_json_strict(ROOT / "config" / "solana_pump" / "phase1.json")
        with self.assertRaises(ConfigError):
            build_runtime_config(ROOT / "config" / "solana_pump" / "phase1.json", {**raw, "market_profile_id": "bsc-mainnet:unknown"})
        with self.assertRaises(ConfigError):
            build_runtime_config(ROOT / "config" / "solana_pump" / "phase1.json", {**raw, "market_profile_id": ""})


if __name__ == "__main__":
    unittest.main()

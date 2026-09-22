"""Causal fee refresh regressions. All RPC calls are deterministic stubs."""
import asyncio
from dataclasses import replace
import unittest
from urllib.error import URLError
from unittest.mock import patch

from darkbot.domain import CostBasis, CostEvidence, NS_PER_MS
from darkbot.fees import FEE_KINDS, FeeCache, FeeResolver, select_cost_basis
from darkbot.replay import digest, load_replay_input, run_replay
from tests.helpers import Harness, base_config, create_payload, fee_basis, T0


def failure(snap, at, *, transient=True, status="RPC_ERROR"):
    return replace(snap, fee_snapshot_id=snap.fee_snapshot_id + "_failure", received_mono_ns=at,
                   validation_status=status, evidence=CostEvidence.UNKNOWN,
                   raw_values={"error": "URLError" if transient else "ValueError", "transient_acquisition_failure": transient})


def as_cache(basis):
    return FeeCache(**{k: getattr(basis, k) for k in FEE_KINDS.values()})


class FeeRefreshTests(unittest.TestCase):
    def test_edge_analysis_uses_same_atomic_causal_evidence_selection(self):
        from darkbot.edge_analysis import _basis_at
        old = fee_basis(base_config(), T0).basis()
        failed = failure(old.protocol, T0 + 50 * NS_PER_MS)
        invalid = failure(old.protocol, T0 + 100 * NS_PER_MS, transient=False, status="INVALID_OWNER")
        snapshots = [getattr(old, k) for k in FEE_KINDS.values()] + [failed, invalid]
        self.assertIs(_basis_at(snapshots, T0 + 75 * NS_PER_MS).protocol, old.protocol)
        self.assertIs(_basis_at(snapshots, T0 + 100 * NS_PER_MS).protocol, invalid)
        self.assertIsNone(_basis_at(snapshots, T0 - 1).protocol)

    def test_transient_retains_original_identity_and_expires_without_renewal(self):
        old = fee_basis(base_config(), T0).basis()
        old = replace(old, protocol=replace(old.protocol, max_age_ns=100 * NS_PER_MS))
        failed = failure(old.protocol, T0 + 50 * NS_PER_MS)
        retained = select_cost_basis(old, [failed])
        self.assertIs(retained.protocol, old.protocol)
        self.assertTrue(retained.protocol.is_fresh(T0 + 100 * NS_PER_MS))
        self.assertFalse(retained.protocol.is_fresh(T0 + 101 * NS_PER_MS))
        late = failure(old.protocol, T0 + 101 * NS_PER_MS)
        self.assertIs(select_cost_basis(retained, [late]).protocol, late)

    def test_authoritative_invalidations_never_resurrect_prior_good_data(self):
        old = fee_basis(base_config(), T0).basis()
        for status in ("INVALID_OWNER", "LAYOUT_UNVERIFIED", "RPC_ERROR"):
            invalid = failure(old.protocol, T0 + NS_PER_MS, transient=False, status=status)
            current = select_cost_basis(old, [invalid])
            self.assertIs(current.protocol, invalid)
            retry = failure(old.protocol, T0 + 2 * NS_PER_MS)
            self.assertIs(select_cost_basis(current, [retry]).protocol, retry)
        other_provider = replace(failure(old.protocol, T0 + NS_PER_MS), source_provider="OTHER")
        self.assertIs(select_cost_basis(old, [other_provider]).protocol, other_provider)

    def test_priority_change_with_failed_network_retains_old_coherent_pair(self):
        old = fee_basis(base_config(), T0).basis()
        priority = replace(old.priority_policy, fee_snapshot_id="changed_priority", received_mono_ns=T0 + NS_PER_MS,
                           normalized_fee_components={"compute_unit_price_micro_lamports": 20_000, "compute_unit_limit": 200_000})
        failed_net = failure(old.network, T0 + NS_PER_MS)
        selected = select_cost_basis(old, [priority, failed_net])
        self.assertIs(selected.network, old.network)
        self.assertIs(selected.priority_policy, old.priority_policy)
        invalid = replace(failed_net, raw_values={"transient_acquisition_failure": False}, validation_status="FEE_MESSAGE_UNVERIFIED")
        self.assertIs(select_cost_basis(old, [priority, invalid]).network, invalid)

    def test_resolver_classifies_transport_separately_from_invalid_data(self):
        h = Harness()
        self.addCleanup(h.conn.close)
        def timeout(*args):
            raise URLError("offline")
        resolver = FeeResolver(h.cfg, timeout, h.clock, "probe", "FIXTURE")
        self.assertTrue(resolver.resolve_priority().raw_values["transient_acquisition_failure"])
        resolver = FeeResolver(h.cfg, lambda *args: [{"prioritizationFee": "invalid"}], h.clock, "probe", "FIXTURE")
        self.assertFalse(resolver.resolve_priority().raw_values["transient_acquisition_failure"])

    def test_retry_backoff_is_per_family_and_resets_on_success(self):
        h = Harness()
        self.addCleanup(h.conn.close)
        resolver = FeeResolver(h.cfg, None, h.clock, "probe", "FIXTURE")
        old = h.fees.basis()
        failed = failure(old.protocol, T0 + NS_PER_MS)
        self.assertEqual(resolver._next_refresh_delay_ms("protocol", [failed]), 1000)
        self.assertEqual(resolver._next_refresh_delay_ms("protocol", [failed]), 2000)
        self.assertEqual(resolver._next_refresh_delay_ms("rent", [failure(old.rent, T0 + NS_PER_MS)]), 1000)
        self.assertEqual(resolver._next_refresh_delay_ms("protocol", [old.protocol]), h.cfg.costs["snapshot_refresh_policy"]["protocol_refresh_interval_ms"])
        self.assertEqual(resolver._next_refresh_delay_ms("protocol", [failed]), 1000)
        authoritative = failure(old.protocol, T0 + 2 * NS_PER_MS, transient=False, status="INVALID_OWNER")
        self.assertEqual(resolver._next_refresh_delay_ms("protocol", [authoritative]), h.cfg.costs["snapshot_refresh_policy"]["protocol_refresh_interval_ms"])

    def test_network_priority_retry_as_one_family(self):
        h = Harness()
        self.addCleanup(h.conn.close)
        resolver = FeeResolver(h.cfg, None, h.clock, "probe", "FIXTURE")
        old = h.fees.basis()
        failed_priority = failure(old.priority_policy, T0 + NS_PER_MS)
        failed_network = failure(old.network, T0 + NS_PER_MS, status="PRIORITY_UNKNOWN")
        self.assertEqual(resolver._next_refresh_delay_ms("network_pair", [failed_priority, failed_network]), 1000)
        self.assertEqual(resolver._next_refresh_delay_ms("network_pair", [failed_priority, failed_network]), 2000)
        self.assertEqual(resolver._next_refresh_delay_ms("network_pair", [old.priority_policy, old.network]), h.cfg.costs["snapshot_refresh_policy"]["network_fee_refresh_interval_ms"])

    def test_rent_partial_transport_failure_retries_but_does_not_retain_combined_evidence(self):
        h = Harness()
        self.addCleanup(h.conn.close)
        calls = {"n": 0}
        def rpc(_method, _params):
            calls["n"] += 1
            if calls["n"] == 1:
                return 2_039_280
            raise URLError("offline")
        resolver = FeeResolver(h.cfg, rpc, h.clock, "probe", "FIXTURE")
        snap = resolver.resolve_rent()
        self.assertEqual(snap.validation_status, "ACCOUNT_RENT_UNVERIFIED")
        self.assertFalse(snap.raw_values["transient_acquisition_failure"])
        self.assertTrue(snap.raw_values["retryable_acquisition_failure"])
        self.assertEqual(resolver._next_refresh_delay_ms("rent", [snap]), 1000)

    def test_run_background_retries_failed_rent_before_hourly_interval(self):
        h = Harness()
        self.addCleanup(h.conn.close)
        old = h.fees.basis()
        resolver = FeeResolver(h.cfg, None, h.clock, "probe", "FIXTURE")
        resolver.retry_backoff_ms = (1,)
        rent_calls = {"n": 0}
        received = []

        def rent():
            rent_calls["n"] += 1
            if rent_calls["n"] == 1:
                return failure(old.rent, T0, transient=True)
            return replace(old.rent, fee_snapshot_id="rent_recovered")

        async def run():
            stop = asyncio.Event()
            def callback(batch):
                received.extend(batch)
                if rent_calls["n"] >= 2 and any(s.source_type == "RENT_EXEMPTION" and s.validation_status == "VALID" for s in batch):
                    stop.set()
            with patch.object(resolver, "resolve_protocol", return_value=old.protocol), \
                 patch.object(resolver, "resolve_priority", return_value=old.priority_policy), \
                 patch.object(resolver, "resolve_network", return_value=old.network), \
                 patch.object(resolver, "resolve_rent", side_effect=rent):
                await asyncio.wait_for(resolver.run_background(callback, stop), timeout=1.0)

        asyncio.run(run())
        self.assertGreaterEqual(rent_calls["n"], 2)
        self.assertTrue(any(s.source_type == "RENT_EXEMPTION" and s.validation_status == "VALID" for s in received))

    def test_live_publication_keeps_raw_failure_but_cache_retains_good_pair(self):
        h = Harness()
        self.addCleanup(h.conn.close)
        old = h.fees.basis()
        resolver = FeeResolver(h.cfg, None, h.clock, "probe", "FIXTURE")
        resolver.cache = h.fees
        failed_priority = failure(old.priority_policy, T0)
        failed_net = failure(old.network, T0, status="PRIORITY_UNKNOWN")
        received = []
        async def run():
            stop = asyncio.Event()
            def callback(batch):
                received.extend(batch)
                stop.set()
            with patch.object(resolver, "resolve_protocol", return_value=old.protocol), patch.object(resolver, "resolve_priority", return_value=failed_priority), patch.object(resolver, "resolve_network", return_value=failed_net), patch.object(resolver, "resolve_rent", return_value=old.rent):
                await resolver.run_background(callback, stop)
        asyncio.run(run())
        self.assertIs(resolver.cache.network, old.network)
        self.assertIs(resolver.cache.priority_policy, old.priority_policy)
        self.assertTrue(any(s.validation_status == "RPC_ERROR" for s in received))

    def scenario(self, *, expire=False, authoritative=False, late=False):
        cfg = base_config(latency_model={"entry_delay_ms": 100, "exit_delay_ms": 0})
        cache = fee_basis(cfg, T0)
        if expire:
            cache.protocol = replace(cache.protocol, max_age_ns=80 * NS_PER_MS)
            # Replay derives max age from the declared config, so freeze the same limit.
            refresh = dict(cfg.costs["snapshot_refresh_policy"], protocol_max_age_ms=80, protocol_refresh_interval_ms=40)
            cfg = cfg.with_overrides(costs={"snapshot_refresh_policy": refresh})
            cache = fee_basis(cfg, T0)
        h = Harness(cfg, fees=cache)
        self.addCleanup(h.conn.close)
        start = T0 + NS_PER_MS
        h.feed("helius_standard", create_payload(), start)
        at = start + (150 if late else 50) * NS_PER_MS
        h.clock.advance_to(at)  # late callback deliberately has not fired
        failed = failure(cache.protocol, at, transient=not authoritative, status="INVALID_OWNER" if authoritative else "RPC_ERROR")
        h.recorder.try_append(("fee_snapshot", (failed, h.run_id)))
        h.fees = as_cache(select_cost_basis(h.fees.basis(), [failed]))
        h.owner.on_fee_snapshots([failed])
        h.advance(start + 100 * NS_PER_MS)
        return h, start, failed

    def test_raw_failure_persists_and_live_replay_share_fill_outcome(self):
        h, start, failed = self.scenario()
        self.assertEqual(h.owner.counters.entry_fills, 1)
        h.finish(start + 110 * NS_PER_MS)
        self.assertEqual(h.rows("SELECT validation_status FROM fee_snapshots WHERE fee_snapshot_id=?", (failed.fee_snapshot_id,))[0][0], "RPC_ERROR")
        data = load_replay_input(h.conn, h.run_id, h.cfg)
        replay = run_replay(h.conn, data, h.cfg, "parity", "camp", persist=False)
        self.assertEqual(replay.digest, digest(h.owner))
        attempt = h.rows("SELECT fee_snapshot_id FROM execution_attempts WHERE side='BUY'")[0]
        self.assertNotEqual(attempt[0], failed.fee_snapshot_id)

    def test_expiry_and_authoritative_failure_block_pending_fill(self):
        for kwargs in ({"expire": True}, {"authoritative": True}):
            with self.subTest(kwargs=kwargs):
                h, start, _ = self.scenario(**kwargs)
                self.assertEqual(h.owner.counters.entry_fills, 0)
                self.assertEqual(h.rows("SELECT status FROM execution_attempts WHERE side='BUY'")[0][0], "UNKNOWN")

    def test_failure_after_modeled_fill_cannot_change_late_callback(self):
        h, _, failed = self.scenario(authoritative=True, late=True)
        self.assertEqual(h.owner.counters.entry_fills, 1)
        attempt = h.rows("SELECT fee_snapshot_id,status FROM execution_attempts WHERE side='BUY'")[0]
        self.assertEqual(attempt["status"], "FILLED")
        self.assertNotEqual(attempt["fee_snapshot_id"], failed.fee_snapshot_id)

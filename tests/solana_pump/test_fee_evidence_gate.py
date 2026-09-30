"""Economic fee/profile gate: a PAPER/CAPTURE run must not silently run without a complete fresh cost basis."""
from __future__ import annotations

import unittest
from types import SimpleNamespace

from darkbot.solana_pump.domain import CostBasis, CostEvidence
from darkbot.main import Application, run_capture, run_paper
from darkbot.solana_pump.config import ConfigError
from tests.helpers import ROOT

S = 1_000_000_000


class FakeSnapshot:
    def __init__(self, status="VALID", evidence=CostEvidence.MEASURED, fresh=True):
        self.validation_status, self.evidence, self._fresh = status, evidence, fresh

    def is_fresh(self, _now):
        return self._fresh


def bare_app(basis, *, deadline_ms=90_000, outage_ms=90_000, economic_ready=False):
    app = Application.__new__(Application)
    app.require_economic_within_ms = deadline_ms
    app.max_economic_outage_ms = outage_ms
    app._economic_gate_start_mono_ns = 1_000
    app._economic_lost_since_mono_ns = None
    app._economic_seen_ready = False
    app.resolver = SimpleNamespace(cache=SimpleNamespace(basis=lambda: basis))
    app.clock = SimpleNamespace(monotonic_ns=lambda: 1_000)
    app.owner = SimpleNamespace(capital_initialized=True)
    app.economic_evidence_ready = lambda _now=None: economic_ready
    return app


class FeeEvidenceGateTests(unittest.TestCase):
    def test_fee_status_reports_all_components(self):
        basis = CostBasis(protocol=FakeSnapshot(), network=None, rent=FakeSnapshot(fresh=False), priority_policy=FakeSnapshot(status="RPC_ERROR"))
        self.assertEqual(bare_app(basis).fee_status(), {"protocol": "VALID", "priority_policy": "RPC_ERROR", "network": "MISSING", "rent": "STALE"})

    def test_fee_status_reports_unknown_evidence(self):
        basis = CostBasis(protocol=FakeSnapshot(evidence=CostEvidence.UNKNOWN), network=FakeSnapshot(), rent=FakeSnapshot(), priority_policy=FakeSnapshot())
        self.assertEqual(bare_app(basis).fee_status()["protocol"], "UNKNOWN_EVIDENCE")

    def test_gate_does_not_fire_before_deadline(self):
        app = bare_app(CostBasis(None, None, None, None))
        self.assertIsNone(app.economic_gate_violation(1_000 + 89_999 * 1_000_000))

    def test_gate_fires_at_deadline_when_not_economic(self):
        app = bare_app(CostBasis(None, None, None, None))
        result = app.economic_gate_violation(1_000 + 90_000 * 1_000_000)
        self.assertTrue(result.startswith("FEE_EVIDENCE_NOT_ECONOMIC:"))
        for part in ("protocol=MISSING", "priority_policy=MISSING", "network=MISSING", "rent=MISSING", "capital_initialized=True"):
            self.assertIn(part, result)

    def test_gate_does_not_fire_when_economic_ready(self):
        snap = FakeSnapshot()
        app = bare_app(CostBasis(snap, snap, snap, snap), economic_ready=True)
        self.assertIsNone(app.economic_gate_violation(1_000 + 500_000 * 1_000_000))

    def test_gate_disabled_preserves_old_behavior(self):
        app = bare_app(CostBasis(None, None, None, None), deadline_ms=None)
        self.assertIsNone(app.economic_gate_violation(10**18))

    def test_later_outage_uses_separate_tolerance_and_resets_after_recovery(self):
        """Startup and later-outage tolerances are independent; separate incidents never accumulate."""
        snap = FakeSnapshot()
        app = bare_app(CostBasis(snap, snap, snap, snap), deadline_ms=90_000, outage_ms=30_000)
        ready = {"v": True}
        app.economic_evidence_ready = lambda _now=None: ready["v"]
        t = 1_000 + 3_600 * S
        self.assertIsNone(app.economic_gate_violation(t))                       # economic for an hour
        ready["v"] = False
        self.assertIsNone(app.economic_gate_violation(t + 5 * S))               # first outage begins here
        self.assertIsNone(app.economic_gate_violation(t + 34 * S))              # 29 s: still tolerated
        ready["v"] = True
        self.assertIsNone(app.economic_gate_violation(t + 35 * S))              # recovery resets timer
        ready["v"] = False
        self.assertIsNone(app.economic_gate_violation(t + 40 * S))              # second outage begins here
        self.assertIsNone(app.economic_gate_violation(t + 69 * S))
        violation = app.economic_gate_violation(t + 71 * S)                     # 31 s continuous outage -> stop
        self.assertIsNotNone(violation)
        self.assertIn("phase=outage", violation)
        self.assertIn("not_economic_for_s=31", violation)

    def test_run_helpers_forward_separate_gate_values_and_reject_nonpositive(self):
        import darkbot.main as main_module
        captured = {}

        class FakeApplication:
            def __init__(self, _cfg, _db, _campaign, mode="PAPER", max_duration_ms=None, **kwargs):
                captured[mode] = (
                    kwargs.get("require_economic_within_ms"),
                    kwargs.get("max_economic_outage_ms"),
                )

            async def run(self):
                return {}
        original = main_module.Application
        main_module.Application = FakeApplication
        try:
            run_paper(
                ROOT / "config/solana_pump/strategy_c_pre_pump_01.json", ROOT / "data/solana_pump/x.sqlite", "test",
                max_duration_minutes=1, require_economic_within_s=120, max_economic_outage_s=30,
            )
            # If omitted, outage tolerance intentionally falls back to the startup value.
            run_capture(
                ROOT / "config/solana_pump/strategy_c_pre_pump_01.json", ROOT / "data/solana_pump/x.sqlite", "test",
                max_duration_minutes=1, require_economic_within_s=90,
            )
            with self.assertRaises(ConfigError):
                run_capture(ROOT / "config/solana_pump/phase1.json", ROOT / "data/solana_pump/x.sqlite", "test", max_duration_minutes=1, require_economic_within_s=0)
            with self.assertRaises(ConfigError):
                run_capture(ROOT / "config/solana_pump/phase1.json", ROOT / "data/solana_pump/x.sqlite", "test", max_duration_minutes=1, max_economic_outage_s=0)
        finally:
            main_module.Application = original
        self.assertEqual(captured, {"PAPER": (120_000, 30_000), "CAPTURE": (90_000, 90_000)})


if __name__ == "__main__":
    unittest.main()

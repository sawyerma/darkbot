"""Regression: preserved local historical runs (names only, read-only) still resolve by id through the run locator."""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'fixtures'))
import p0_paths  # shared paths; puts project_0 on sys.path
import unittest
from operations.verification import audit_market_quick as a

REPO=p0_paths.REPO


class HistoricalRuns(unittest.TestCase):
    def test_historical_local_runs_still_resolve_by_id_read_only(self):
        roots=[REPO/'project_0'/r for r in ('data/econ4h','data/market') if (REPO/'project_0'/r).is_dir()]
        names=[x.name for root in roots for x in sorted(root.iterdir()) if x.is_dir()][:3]
        if not names:self.skipTest('no local historical runs on this machine')
        for name in names:
            self.assertTrue(a.find_run(name).is_dir())


if __name__=='__main__':
    unittest.main(verbosity=2)

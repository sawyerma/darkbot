#!/usr/bin/env python3
"""Feature-matrix regression: `darkbot-runner` declares `default = []` (the health engine is the
optional `crate_health` feature; only `p0-longtail` requires it). A workspace build always unifies
`crate_health` into the runner (the reporter depends on it with that feature), so only an isolated
package build proves the library, its binaries and tests still compile without it. Offline only
(`--offline --locked`); no network, no Mainnet.
"""
import os
import shutil
import subprocess
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'fixtures'))
import p0_paths  # shared paths; puts project_0 on sys.path

NATIVE = p0_paths.PROJECT / 'native'


class RunnerFeatureMatrix(unittest.TestCase):
    def check(self, *features):
        if not shutil.which('cargo'):
            self.skipTest('cargo unavailable')
        env = dict(os.environ, CARGO_BUILD_JOBS=os.environ.get('CARGO_BUILD_JOBS', '2'))
        cmd = ['cargo', 'check', '--offline', '--locked', '-p', 'darkbot-runner', '--all-targets', *features]
        p = subprocess.run(cmd, cwd=NATIVE, env=env, capture_output=True, text=True, timeout=1800)
        errors = [l for l in p.stderr.splitlines() if l.startswith('error')]
        self.assertEqual(p.returncode, 0, f"{' '.join(cmd)}: {errors[:10]}")

    def test_runner_compiles_without_the_health_engine(self):
        self.check()

    def test_runner_compiles_with_txv1_without_the_health_engine(self):
        self.check('--features', 'txv1')


if __name__ == '__main__':
    unittest.main()

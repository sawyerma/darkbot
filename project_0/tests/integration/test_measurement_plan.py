#!/usr/bin/env python3
"""Offline P1 CLI/launcher regression. Uses a dummy signer and a fake capture binary.
Run AFTER building release p0-measurement-plan; never contacts RPC or starts Shadow.
"""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'fixtures'))
import p0_paths  # shared paths; puts project_0 on sys.path
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = p0_paths.REPO
SCRIPTS = p0_paths.SCRIPTS
PLAN = Path(os.environ.get('P0_TEST_PLAN_BINARY', ROOT / 'project_0/native/target/release/p0-measurement-plan'))


clean_env = p0_paths.clean_env


def call_plan(**values):
    return subprocess.run([str(PLAN)], env={**clean_env(), **values}, capture_output=True, text=True, check=False)


class MeasurementCli(unittest.TestCase):
    def test_release_duration_matrix(self):
        for duration, seconds in [('10m', 600), ('1h', 3600), ('5h', 18000), ('24h', 86400), ('25h',90000), ('61s', 61), ('1s', 1)]:
            with self.subTest(duration=duration):
                p = call_plan(P0_MEASUREMENT_DURATION=duration)
                self.assertEqual(p.returncode, 0, p.stderr)
                plan = json.loads(p.stdout)
                self.assertEqual(plan['duration_seconds'], seconds)
                self.assertEqual(plan['snapshot_offsets_ns'], [seconds * 10**9 * i // 4 for i in range(1, 5)])
                with tempfile.TemporaryDirectory() as folder:
                    path = Path(folder) / 'plan.json'
                    path.write_text(p.stdout)
                    loaded = call_plan(P0_MEASUREMENT_PLAN=str(path))
                    self.assertEqual(loaded.returncode, 0, loaded.stderr)
                    self.assertEqual(json.loads(loaded.stdout), plan)

    def test_invalid_and_conflicting_input(self):
        for duration in ['0s', '18446744073709551615h', '1.5h', '1', 'é', '1🦀']:
            self.assertNotEqual(call_plan(P0_MEASUREMENT_DURATION=duration).returncode, 0)
        self.assertNotEqual(call_plan(P0_MEASUREMENT_DURATION='10m', P0_ECON_SNAPSHOT_SECS='3600').returncode, 0)
        self.assertNotEqual(call_plan(P0_MEASUREMENT_DURATION='10m', P0_MARKET_DURATION_SECS='300').returncode, 0)
        old = call_plan(P0_ECON_DURATION_SECS='14400', P0_ECON_SNAPSHOT_SECS='3600')
        self.assertEqual(old.returncode, 0, old.stderr)

    def test_help_and_required_argument_do_not_start_run(self):
        for args, expected in [(['--help'], 0), ([], 2), (['--duration'], 2), (['--duration', '10m', '--duration', '1h'], 2)]:
            p = subprocess.run([str(SCRIPTS / 'test.sh'), *args], env=clean_env(), capture_output=True)
            self.assertEqual(p.returncode, expected)

    def test_launcher_uses_persisted_plan_and_no_total_capture_timebox(self):
        with tempfile.TemporaryDirectory(prefix='p0-p1-launcher-') as folder:
            tmp = Path(folder)
            target = tmp / 'target/release'
            target.mkdir(parents=True)
            (target / 'p0-measurement-plan').symlink_to(PLAN.resolve())
            capture = target / 'p0-longtail'
            capture.write_text('#!/usr/bin/env bash\nset -eu\n'
                               'cp "$P0_MEASUREMENT_PLAN" "$P0_ECON_OUT_DIR/$P0_ECON_RUN_ID/consumed_plan.json"\n'
                               'python3 - "$P0_ECON_OUT_DIR/$P0_ECON_RUN_ID" "$P0_ECON_DURATION_SECS" <<\'MOCK\'\n'
                               'import json,sys,pathlib,datetime\n'
                               'r=pathlib.Path(sys.argv[1]); (r/"market").mkdir()\n'
                               'n=int(sys.argv[2]); end=datetime.datetime.fromtimestamp(100+n,datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")\n'
                               '(r/"market/summary.json").write_text(json.dumps({"observation_started_at":"1970-01-01T00:01:40Z","observation_ended_at":end,"configured_duration_seconds":n,"capture_clock_complete":True,"live_coverage_status":"LIVE_COVERAGE_COMPLETE"}))\n'
                               'MOCK\n'
                               'printf "%s:%s:%s" "$P0_ECON_DURATION_SECS" "$P0_LIVE_ENABLED" "$P0_SUBMISSION_ENABLED"\n')
            capture.chmod(0o700)
            mock = tmp / 'mock'
            mock.mkdir()
            cargo = mock / 'cargo'
            cargo.write_text('#!/usr/bin/env bash\nexit 0\n')
            cargo.chmod(0o700)
            # Storage preflight is tested with ample mock disk; no data allocation.
            (mock / 'sitecustomize.py').write_text('import shutil\nshutil.disk_usage = lambda path: (type("Disk", (), {"free": 2**50}))()\n')
            # Public wallet configuration only. Dummy secret bytes are never used to sign.
            public = subprocess.check_output(['bash', '-c', 'source "$1"; printf "%s" "$P0_EXPECTED_PUBKEY"', 'test', str(ROOT / 'project_0/config/wallet.public.env')], text=True)
            alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
            n = 0
            for char in public:
                n = n * 58 + alphabet.index(char)
            key = tmp / 'dummy.json'
            key.write_text(json.dumps([0]*32 + list(n.to_bytes(32, 'big'))))
            key.chmod(0o600)
            env = {**clean_env(), 'P0_ENV_FILE': str(tmp / 'absent.env'),
                   'P0_RPC_URL': 'http://127.0.0.1:1', 'P0_WS_URL': 'ws://127.0.0.1:1',
                   'P0_KEYPAIR_PATH': str(key), 'P0_MAX_SLIPPAGE_BPS': '50',
                   'P0_ECON_OUT_DIR': str(tmp / 'runs'),
                   'CARGO_TARGET_DIR': str(target.parent), 'PATH': str(mock) + os.pathsep + os.environ['PATH'],
                   'PYTHONPATH': str(mock)}
            for duration, seconds in [('10m', 600), ('61s', 61)]:
                p = subprocess.run([str(SCRIPTS / 'test.sh'), '--duration', duration], cwd=tmp,
                                   env={**env, 'P0_ECON_RUN_ID': duration}, capture_output=True, text=True, timeout=20)
                self.assertEqual(p.returncode, 0, p.stderr)
                self.assertIn(f'{seconds}:false:false', p.stdout)
                run = tmp / 'runs' / duration
                plan = json.loads((run / 'measurement_plan.json').read_text())
                self.assertEqual(plan, json.loads((run / 'consumed_plan.json').read_text()))
                self.assertEqual(plan['duration_seconds'], seconds)
                self.assertEqual(json.loads((run / 'storage_preflight.json').read_text())['duration_seconds'], seconds)
                self.assertFalse((run / 'quick_timebox_report.json').exists())
            rejected = subprocess.run([str(SCRIPTS / 'test.sh'), '--duration', '10m'], env={**env, 'P0_ECON_RUN_MODE': 'quick'}, capture_output=True)
            self.assertEqual(rejected.returncode, 2)


if __name__ == '__main__':
    unittest.main(verbosity=2)

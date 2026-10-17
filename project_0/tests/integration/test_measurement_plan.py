#!/usr/bin/env python3
"""Offline P1 CLI/launcher regression with the real release binaries, real cargo and the real launcher.
Run AFTER building release p0-measurement-plan and p0-longtail. Endpoints are closed local ports and the key file holds only
the public half, so the real capture fails closed at startup; nothing contacts RPC, signs or submits.
"""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'fixtures'))
import p0_paths  # shared paths; puts project_0 on sys.path
import json
import os
from pathlib import Path
import shutil
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
        for duration, seconds in [('10m', 600), ('1h', 3600), ('5h', 18000), ('24h', 86400), ('25h',90000), ('61s', 61), ('1s', 1), ('2d', 172800)]:
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
        for duration in ['0s', '18446744073709551615h', '1.5h', '1', 'é', '1🦀', 'inf', 'until-stopped', '0d']:
            self.assertNotEqual(call_plan(P0_MEASUREMENT_DURATION=duration).returncode, 0)
        self.assertNotEqual(call_plan(P0_MEASUREMENT_DURATION='10m', P0_ECON_SNAPSHOT_SECS='3600').returncode, 0)
        self.assertNotEqual(call_plan(P0_MEASUREMENT_DURATION='10m', P0_MARKET_DURATION_SECS='300').returncode, 0)
        old = call_plan(P0_ECON_DURATION_SECS='14400', P0_ECON_SNAPSHOT_SECS='3600')
        self.assertEqual(old.returncode, 0, old.stderr)

    def test_help_and_required_argument_do_not_start_run(self):
        for args, expected in [(['--help'], 0), ([], 2), (['--duration'], 2), (['--duration', '10m', '--duration', '1h'], 2)]:
            p = subprocess.run([str(SCRIPTS / 'test.sh'), *args], env=clean_env(), capture_output=True)
            self.assertEqual(p.returncode, expected)

    def test_launch_never_recurses(self):
        # p0ctl marks its launcher child; inside it neither `p0ctl run start` nor `capture.sh econ4h` starts again.
        marked = {**clean_env(), 'P0_CTL_LAUNCH_RUN_ID': 'outer'}
        p = subprocess.run([str(SCRIPTS / 'capture.sh'), 'econ4h', '--duration', '61s'], env=marked, capture_output=True, text=True, timeout=60)
        self.assertEqual(p.returncode, 2)
        self.assertIn('RECURSIVE_LAUNCH: capture.sh econ4h inside the launcher of run outer', p.stderr)
        ctl = ROOT / 'project_0/native/target/release/p0ctl'
        if not ctl.exists():
            self.skipTest('NOT_RUN: release p0ctl not built')
        p = subprocess.run([str(ctl), '--project', str(ROOT / 'project_0'), 'run', 'start', '--duration', '61s'], env=marked,
                           capture_output=True, text=True, timeout=60)
        self.assertEqual(p.returncode, 2)
        self.assertIn('RECURSIVE_LAUNCH: run start inside the launcher of run outer', p.stderr)
        self.assertNotIn('BOOT AUDIT', p.stdout)

    def test_launcher_uses_persisted_plan_and_no_total_capture_timebox(self):
        release = ROOT / 'project_0/native/target/release'
        if not all((release / b).exists() for b in ('p0-longtail', 'p0-measurement-plan', 'p0ctl')):
            self.skipTest('NOT_RUN: release binaries not built')
        with tempfile.TemporaryDirectory(prefix='p0-p1-launcher-') as folder:
            tmp = Path(folder)
            if shutil.disk_usage(tmp).free < 8 * 2**30:
                self.skipTest('NOT_RUN: launcher storage preflight needs 8 GiB free')
            # Public wallet configuration only. The secret half is zero, so the real signer can never sign.
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
                   'P0_ECON_OUT_DIR': str(tmp / 'runs'), 'CARGO_TARGET_DIR': str(ROOT / 'project_0/native/target')}
            ctl = release / 'p0ctl'
            # OFFLINE_READY_NETWORK_NOT_RUN is no Mainnet release: without the operator's approval of exactly the
            # audited state nothing starts; the token is the one `p0ctl doctor` prints for this state.
            doctor = subprocess.run([str(ctl), '--project', str(ROOT / 'project_0'), 'doctor'], env=env, capture_output=True, text=True, timeout=60)
            self.assertIn('verdict=OFFLINE_READY_NETWORK_NOT_RUN', doctor.stdout)
            token = doctor.stdout.split('token=', 1)[1].split()[0]
            for approval in ([], ['--mainnet-approved', '0' * 16], ['--mainnet-approved', token, '--mainnet-approved', token]):
                p = subprocess.run([str(SCRIPTS / 'test.sh'), '--duration', '61s', *approval], cwd=tmp,
                                   env={**env, 'P0_ECON_RUN_ID': 'unapproved'}, capture_output=True, text=True, timeout=120)
                self.assertEqual(p.returncode, 2, p.stderr[-2000:])
                self.assertIn('exactly one --mainnet-approved' if len(approval) > 2 else f'MAINNET_APPROVAL_REQUIRED: boot verdict OFFLINE_READY_NETWORK_NOT_RUN is no Mainnet release', p.stderr)
                self.assertNotIn('[p0ctl] run unapproved started', p.stdout)
                self.assertFalse((tmp / 'runs').exists())
            approved = ['--mainnet-approved', token]
            for duration, seconds, relative in [('10m', 600, False), ('61s', 61, False), ('61s', 61, True)]:
                run_id = 'relative_config' if relative else duration
                command = [str(SCRIPTS / 'test.sh'), '--duration', duration, *approved]
                if relative:
                    # Native text config, relative to the caller's directory (the launcher changes directory).
                    (tmp / 'relative.conf').write_text('# operator override\nrpc-rps = 1\n')
                    command += ['--', '--config', 'relative.conf']
                p = subprocess.run(command, cwd=tmp,
                                   env={**env, 'P0_ECON_RUN_ID': run_id}, capture_output=True, text=True, timeout=600)
                self.assertNotIn('No such file or directory', p.stderr)
                self.assertNotIn('Traceback', p.stderr)
                self.assertEqual(p.returncode, 2, p.stderr[-2000:])  # no network: the real capture fails closed at startup
                self.assertIn(f'approved by the operator for readiness token {token}', p.stdout)
                self.assertIn(f'Capture: {seconds}s', p.stdout)
                run = tmp / 'runs' / run_id
                # JFI-03/PERSIST-04: the productive run writes no JSON/JSONL; the legacy views exist only after the
                # explicit offline export.
                self.assertEqual([str(f) for f in run.rglob('*') if f.suffix in ('.json', '.jsonl')], [])
                export = subprocess.run([str(ctl), '--project', str(ROOT / 'project_0'), 'export', '--run', str(run)],
                                        capture_output=True, text=True, timeout=60)
                self.assertEqual(export.returncode, 0, export.stderr)
                plan = json.loads((run / 'measurement_plan.json').read_text())
                manifest = json.loads((run / 'measurement_manifest.json').read_text())  # typed record of the real p0-longtail
                if relative:
                    operational = manifest['operational_config']
                    self.assertEqual((operational['limits']['rpc_rps'], operational['sources']['rpc_rps']), (1, 'config_file'))
                self.assertEqual(manifest['plan'], plan)  # the binary consumed exactly the persisted plan
                self.assertEqual((manifest['live_enabled'], manifest['submission_enabled']), (False, False))
                self.assertEqual(plan['duration_seconds'], seconds)
                self.assertEqual(json.loads((run / 'storage_preflight.json').read_text())['duration_seconds'], seconds)
                self.assertIn('C0_E1_WARMUP_FAILED', (run / 'runtime.log').read_text())
                self.assertFalse((run / 'quick_timebox_report.json').exists())
            rejected = subprocess.run([str(SCRIPTS / 'test.sh'), '--duration', '10m', *approved], cwd=tmp,
                                      env={**env, 'P0_ECON_RUN_MODE': 'quick', 'P0_ECON_RUN_ID': 'quick'}, capture_output=True, timeout=60)
            self.assertEqual(rejected.returncode, 2)
            self.assertFalse((tmp / 'runs/quick').exists())
            # p0ctl drives the same launcher: offline boot audit, run id and owner lock handed over,
            # child exit reported; the lock is released afterwards and records the last owner.
            if ctl.exists():
                lock = tmp / 'runtime.lock'
                p = subprocess.run([str(ctl), '--project', str(ROOT / 'project_0'), 'run', 'start', '--duration', '61s', *approved], cwd=tmp,
                                   env={**env, 'P0_ECON_RUN_ID': 'p0ctl', 'P0_RUNTIME_LOCK': str(lock)},
                                   capture_output=True, text=True, timeout=600)
                self.assertEqual(p.returncode, 2, p.stderr[-2000:])
                self.assertIn('PROJECT 0 BOOT AUDIT scope=offline', p.stdout)
                self.assertIn('[p0ctl] run p0ctl started', p.stdout)
                self.assertIn('[p0ctl] run p0ctl ended', p.stdout)
                # Explicit P0_ECON_OUT_DIR keeps layout V1: the native capacity report sits at the run root.
                self.assertTrue((tmp / 'runs/p0ctl/capacity_preflight.p0j').exists(), p.stdout[-3000:])
                self.assertIn('run_id=p0ctl', lock.read_text())
                status = subprocess.run([str(ctl), '--project', str(ROOT / 'project_0'), 'run', 'stop'], env={**env, 'P0_RUNTIME_LOCK': str(lock)},
                                        capture_output=True, text=True, timeout=60)
                self.assertEqual((status.returncode, status.stdout.strip()), (0, 'NOT_RUNNING: nothing to stop'))

if __name__ == '__main__':
    unittest.main(verbosity=2)

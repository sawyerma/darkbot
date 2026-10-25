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


def call_plan(out, **values):
    """The typed plan journal at `out`; stdout carries only the duration in seconds (no JSON)."""
    return subprocess.run([str(PLAN), '--out', str(out)], env={**clean_env(), **values}, capture_output=True, text=True, check=False)


class MeasurementCli(unittest.TestCase):
    def test_release_duration_matrix(self):
        for duration, seconds in [('10m', 600), ('1h', 3600), ('5h', 18000), ('24h', 86400), ('25h',90000), ('61s', 61), ('1s', 1), ('2d', 172800)]:
            with self.subTest(duration=duration), tempfile.TemporaryDirectory() as folder:
                path = Path(folder) / 'plan.p0j'
                p = call_plan(path, P0_MEASUREMENT_DURATION=duration)
                self.assertEqual(p.returncode, 0, p.stderr)
                self.assertEqual(p.stdout.strip(), str(seconds))
                # The typed plan journal is the runtime input: loading it gives the same plan.
                loaded = call_plan(Path(folder) / 'again.p0j', P0_MEASUREMENT_PLAN=str(path))
                self.assertEqual(loaded.returncode, 0, loaded.stderr)
                self.assertEqual(loaded.stdout.strip(), str(seconds))
        with tempfile.TemporaryDirectory() as folder:
            # JFI-01/P0-PROD-CTRL-02: no JSON output and no JSON plan input.
            self.assertEqual(subprocess.run([str(PLAN)], env={**clean_env(), 'P0_MEASUREMENT_DURATION': '10m'},
                                            capture_output=True).returncode, 2)
            json_plan = Path(folder) / 'plan.json'
            json_plan.write_text('{}')
            refused = call_plan(Path(folder) / 'x.p0j', P0_MEASUREMENT_PLAN=str(json_plan))
            self.assertEqual(refused.returncode, 2)
            self.assertIn('MEASUREMENT_PLAN_NOT_TYPED', refused.stderr)

    def test_invalid_and_conflicting_input(self):
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder) / 'plan.p0j'
            for duration in ['0s', '18446744073709551615h', '1.5h', '1', 'é', '1🦀', 'inf', 'until-stopped', '0d']:
                self.assertNotEqual(call_plan(out, P0_MEASUREMENT_DURATION=duration).returncode, 0)
                self.assertFalse(out.exists())
            self.assertNotEqual(call_plan(out, P0_MEASUREMENT_DURATION='10m', P0_ECON_SNAPSHOT_SECS='3600').returncode, 0)
            self.assertNotEqual(call_plan(out, P0_MEASUREMENT_DURATION='10m', P0_MARKET_DURATION_SECS='300').returncode, 0)
            old = call_plan(out, P0_ECON_DURATION_SECS='14400', P0_ECON_SNAPSHOT_SECS='3600')
            self.assertEqual(old.returncode, 0, old.stderr)
            self.assertEqual(old.stdout.strip(), '14400')

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

    def test_shadow_starts_only_through_the_controller(self):
        # F-01 (CTRL-01/03): the Shadow launcher refuses to run without p0ctl's launch marker, `capture.sh shadow`
        # never recurses inside a launcher, and the runtime refuses an argument-less Shadow start outright.
        p = subprocess.run([str(SCRIPTS / 'capture.sh'), 'shadow-launcher', '--duration', '3m'], env=clean_env(),
                           capture_output=True, text=True, timeout=60)
        self.assertEqual(p.returncode, 2)
        self.assertIn('LAUNCHER_REQUIRES_P0CTL', p.stderr)
        marked = {**clean_env(), 'P0_CTL_LAUNCH_RUN_ID': 'outer'}
        p = subprocess.run([str(SCRIPTS / 'capture.sh'), 'shadow', '--duration', '3m'], env=marked,
                           capture_output=True, text=True, timeout=60)
        self.assertEqual(p.returncode, 2)
        self.assertIn('RECURSIVE_LAUNCH: capture.sh shadow inside the launcher of run outer', p.stderr)
        runtime = ROOT / 'project_0/native/target/release/p0-longtail'
        if not runtime.exists():
            self.skipTest('NOT_RUN: release p0-longtail not built')
        p = subprocess.run([str(runtime)], env={**clean_env(), 'P0_MODE': 'shadow'}, capture_output=True, text=True, timeout=60)
        self.assertEqual(p.returncode, 2)
        self.assertIn('SHADOW_REQUIRES_CONTROLLER', p.stderr)

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
                self.assertIn('exactly one --mainnet-approved' if len(approval) > 2 else 'BOOT_NETWORK_AUDIT_REQUIRED', p.stderr)
                self.assertNotIn('[p0ctl] run unapproved started', p.stdout)
                self.assertFalse((tmp / 'runs').exists())
            approved = ['--mainnet-approved', token]
            for duration, seconds, relative in [('10m', 600, False), ('61s', 61, False), ('61s', 61, True)]:
                run_id = 'relative_config' if relative else duration
                command = [str(SCRIPTS / 'capture.sh'), 'econ4h-launcher', '--duration', duration]
                if relative:
                    # Native text config, relative to the caller's directory (the launcher changes directory).
                    (tmp / 'relative.conf').write_text('# operator override\nrpc-rps = 1\n')
                    command += ['--', '--config', 'relative.conf']
                p = subprocess.run(command, cwd=tmp,
                                   env={**env, 'P0_ECON_RUN_ID': run_id, 'P0_CTL_LAUNCH_RUN_ID': run_id}, capture_output=True, text=True, timeout=600)
                self.assertNotIn('No such file or directory', p.stderr)
                self.assertNotIn('Traceback', p.stderr)
                # X1: without the approved network audit record the launched runtime is refused after its local
                # manifest, before either owner starts (no network contact).
                self.assertEqual(p.returncode, 2, p.stderr[-2000:])
                # Started as p0ctl's launcher child (P0_CTL_LAUNCH_RUN_ID); run start itself requires the
                # network boot audit since e42162e, asserted above (BOOT_NETWORK_AUDIT_REQUIRED).
                self.assertIn(f'Capture: {seconds}s', p.stdout)
                run = tmp / 'runs' / run_id
                # JFI-01/03, P0-PROD-CTRL-02: the productive run writes no JSON/JSONL and has no JSON export; its
                # typed records are read natively (`p0ctl run status`).
                self.assertEqual([str(f) for f in run.rglob('*') if f.suffix in ('.json', '.jsonl')], [])
                export = subprocess.run([str(ctl), '--project', str(ROOT / 'project_0'), 'export', '--run', str(run)],
                                        capture_output=True, text=True, timeout=60)
                self.assertNotEqual(export.returncode, 0)
                status = subprocess.run([str(ctl), '--project', str(ROOT / 'project_0'), 'run', 'status', '--run', str(run)],
                                        env=env, capture_output=True, text=True, timeout=60)
                self.assertEqual(status.returncode, 0, status.stderr)
                # The typed manifest of the real p0-longtail: the binary consumed exactly the persisted plan.
                self.assertIn(f'plan_duration={seconds}s plan_bound=yes live_enabled=false submission_enabled=false', status.stdout)
                if relative:
                    self.assertIn('rpc_rps=1 source=config_file', status.stdout)
                self.assertIn(f'duration={seconds}s', status.stdout)  # the typed capacity report
                self.assertEqual([str(f) for f in run.rglob('*') if f.suffix in ('.json', '.jsonl')], [])
                log = (run / 'logs' / 'runtime.log').read_text()  # layout V2: every new econ4h run (review 1)
                self.assertIn('C0_E1_ADMISSION_REFUSED BOOT_NETWORK_AUDIT_MISSING', log)
                self.assertNotIn('C0_E1_WARMUP_FAILED', log)
                self.assertFalse((run / 'quick_timebox_report.json').exists())
            rejected = subprocess.run([str(SCRIPTS / 'test.sh'), '--duration', '10m', *approved], cwd=tmp,
                                      env={**env, 'P0_ECON_RUN_MODE': 'quick', 'P0_ECON_RUN_ID': 'quick'}, capture_output=True, timeout=60)
            self.assertEqual(rejected.returncode, 2)
            self.assertFalse((tmp / 'runs/quick').exists())
            # Offline approval alone may no longer start a network run; no runtime owner is created.
            if ctl.exists():
                lock = tmp / 'runtime.lock'
                p = subprocess.run([str(ctl), '--project', str(ROOT / 'project_0'), 'run', 'start', '--duration', '61s', *approved], cwd=tmp,
                                   env={**env, 'P0_ECON_RUN_ID': 'p0ctl', 'P0_RUNTIME_LOCK': str(lock)},
                                   capture_output=True, text=True, timeout=600)
                self.assertEqual(p.returncode, 2, p.stderr[-2000:])
                self.assertIn('PROJECT 0 BOOT AUDIT scope=offline', p.stdout)
                self.assertIn('BOOT_NETWORK_AUDIT_REQUIRED', p.stderr)
                self.assertNotIn('[p0ctl] run p0ctl started', p.stdout)
                self.assertFalse((tmp / 'runs/p0ctl').exists())
                status = subprocess.run([str(ctl), '--project', str(ROOT / 'project_0'), 'run', 'stop'], env={**env, 'P0_RUNTIME_LOCK': str(lock)},
                                        capture_output=True, text=True, timeout=60)
                self.assertEqual((status.returncode, status.stdout.strip()), (0, 'NOT_RUNNING: nothing to stop'))

if __name__ == '__main__':
    unittest.main(verbosity=2)

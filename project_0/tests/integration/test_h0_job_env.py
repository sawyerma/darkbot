"""H0 (review M3): a GUI-started H0 job reads the operator env files of the capture.

`darkbotctl p0 job start h0` exports them as `P0_ENV_FILE` to the deployed p0ctl (which loads
them before the request); other job kinds are started unchanged. Isolated: a private
DARKBOT_TEST_DIR and a recording stand-in for the deployed p0ctl, no launchd, no network.
"""
import os
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[3]


class H0JobEnv(unittest.TestCase):
    def test_only_an_h0_start_gets_the_capture_env_files(self):
        with tempfile.TemporaryDirectory(prefix='darkbot-test.', dir='/private/tmp') as tmp:
            work = pathlib.Path(tmp)
            (work / '.isolated').touch()
            p0ctl = work / 'app' / 'current' / 'p0ctl'
            p0ctl.parent.mkdir(parents=True)
            p0ctl.write_text('#!/bin/sh\necho "P0_ENV_FILE=${P0_ENV_FILE-unset} P0_PROJECT_DIR=$P0_PROJECT_DIR ARGS=$*"\n')
            p0ctl.chmod(0o755)
            env = {k: v for k, v in os.environ.items() if not k.startswith(('P0_', 'DARKBOT_'))}
            env.update(DARKBOT_TEST_DIR=str(work), DARKBOT_TEST_PORT='18999')

            def run(*args):
                p = subprocess.run([str(ROOT / 'ops/darkbotctl'), 'p0', *args], env=env,
                                   text=True, capture_output=True, timeout=60)
                self.assertEqual(p.returncode, 0, p.stderr)
                return p.stdout.strip()

            h0 = run('job', 'start', 'h0', '--max-seconds', '60', '--max-ram-bytes', '1')
            self.assertIn(f'P0_ENV_FILE={work}/capture.env ', h0)
            self.assertIn(f'P0_PROJECT_DIR={work}/app ', h0)
            self.assertTrue(h0.endswith('ARGS=job start h0 --max-seconds 60 --max-ram-bytes 1'), h0)
            backtest = run('job', 'start', 'backtest', '--run', 'r')
            self.assertIn('P0_ENV_FILE=unset ', backtest)
            status = run('job', 'status')
            self.assertIn('P0_ENV_FILE=unset ', status)


if __name__ == '__main__':
    unittest.main()

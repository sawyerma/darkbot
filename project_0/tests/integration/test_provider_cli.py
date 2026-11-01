"""DEC-18: the release `p0ctl` has no test credential store — `P0_TEST_CREDENTIAL_STORE_DIR` is
ignored there (the store exists only in a debug `test-support` build), so a credential needs the
system's approved store; without one (any system but macOS) it fails closed."""
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tests.fixtures import p0_paths

P0CTL = p0_paths.PROJECT / 'native' / 'target' / 'release' / 'p0ctl'


class ReleaseCredentialStore(unittest.TestCase):
    @unittest.skipIf(sys.platform == 'darwin', 'the release build would reach the real macOS keychain')
    def test_the_release_build_ignores_the_test_store(self):
        self.assertTrue(P0CTL.exists(), 'release p0ctl required')
        with tempfile.TemporaryDirectory() as d:
            store = Path(d) / 'store'
            env = p0_paths.clean_env()
            env['P0_TEST_CREDENTIAL_STORE_DIR'] = str(store)
            p = subprocess.run(
                [str(P0CTL), '--project', d, 'provider', 'credential', 'set', '--credential', 'helius', '--request-id', 'r1'],
                input=b'SECRET-VALUE\n', capture_output=True, env=env, timeout=60)
            self.assertNotEqual(p.returncode, 0)
            self.assertIn(b'CREDENTIAL_STORE_UNAVAILABLE', p.stderr)
            self.assertNotIn(b'SECRET-VALUE', p.stdout + p.stderr)
            self.assertFalse(store.exists(), 'no test store in a release build')


if __name__ == '__main__':
    unittest.main()

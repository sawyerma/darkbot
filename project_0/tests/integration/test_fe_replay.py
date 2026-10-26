"""Real release CLI checks: offline replay cannot activate RPC or LIVE."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'fixtures'))
import p0_paths
import subprocess
import tempfile
import unittest


class OfflineReplay(unittest.TestCase):
    def call(self, *args):
        binary = p0_paths.RELEASE / 'p0-fe-qualify'
        self.assertTrue(binary.is_file(), 'release p0-fe-qualify required')
        return subprocess.run([str(binary), *args], cwd=tempfile.gettempdir(),
                              env=p0_paths.clean_env(), capture_output=True, text=True, timeout=20)

    def test_replay_without_rpc_url_reaches_only_local_evidence_reader(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'out'
            result = self.call('--replay', directory, '--output', str(output), '--fe-mode', 'full')
            self.assertEqual(result.returncode, 2)
            # A14: without the binary journal the legacy read files are not a fallback.
            self.assertIn('FE_REPLAY_JOURNAL_MISSING', result.stderr)
            self.assertNotIn('RPC_URL_MISSING', result.stderr)
            self.assertFalse(output.exists())
            legacy = self.call('--replay', directory, '--legacy-reads', '--output', str(output), '--fe-mode', 'full')
            self.assertEqual(legacy.returncode, 2)
            # DEC-09: the historical build finds no read file; the standard build has no legacy reader.
            self.assertTrue(any(code in legacy.stderr for code in ('FE_REPLAY_READ_MISSING', 'HISTORICAL_READER_NOT_BUILT')),
                            legacy.stderr)
            self.assertNotIn('RPC_URL_MISSING', legacy.stderr)
            self.assertFalse(output.exists())

    def test_replay_and_execute_are_mutually_exclusive_before_rpc(self):
        result = self.call('--execute', '--replay', '/nonexistent', '--output', '/nonexistent/out', '--fe-mode', 'full')
        self.assertEqual(result.returncode, 2)
        self.assertIn('FE_EXACTLY_ONE_OF_EXECUTE_OR_REPLAY', result.stderr)

    def test_unsupported_funding_scope_is_not_silently_selected(self):
        result = self.call('--replay', '/nonexistent', '--output', '/nonexistent/out', '--fe-mode', 'full', '--funding-class', 'JIT_SHORTFALL')
        self.assertEqual(result.returncode, 2)
        self.assertIn('FE_ONE_CLASS_MUST_BE_ZERO_PREFUND_OR_NO_SWAP', result.stderr)

    def test_preflight_shell_dispatches_real_replay_without_provider_or_env(self):
        with tempfile.TemporaryDirectory() as directory:
            env = p0_paths.clean_env()
            env['P0_ENV_FILE'] = str(Path(directory) / 'absent.env')
            env['PYTHONDONTWRITEBYTECODE'] = '1'
            result = subprocess.run(['bash', str(p0_paths.SCRIPTS / 'preflight.sh'), '--replay', directory,
                                     '--output', str(Path(directory) / 'out'), '--fe-mode', 'full'],
                                    cwd=directory, env=env, capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 2)
            self.assertIn('FE_REPLAY_JOURNAL_MISSING', result.stderr)
            self.assertNotIn('FREE_ONLY_PROVIDER_QUALIFICATION_REQUIRED', result.stderr)

#!/usr/bin/env python3
"""FE-04/FE-07 terminal commands of the release `p0ctl` (offline: no network, no signer, no LIVE).

`fe04 accounting` reads the FE-04 journals of a wallet and prints the finalized accounting as
text; `fe04 archive` refuses without a finished attempt and never creates a journal.
"""
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'fixtures'))
import p0_paths  # shared paths; puts project_0 on sys.path

P0CTL = p0_paths.PROJECT / 'native' / 'target' / 'release' / 'p0ctl'
WALLET = '11111111111111111111111111111111'


def run(*args):
    return subprocess.run([str(P0CTL), *args], capture_output=True, text=True, timeout=60)


class Fe04Cli(unittest.TestCase):
    def setUp(self):
        self.assertTrue(P0CTL.exists(), 'release p0ctl required')

    def test_accounting_of_no_attempt_is_no_trade_as_text(self):
        with tempfile.TemporaryDirectory() as d:
            p = run('fe04', 'accounting', '--journal-dir', d, '--wallet', WALLET)
            self.assertEqual(p.returncode, 0, p.stderr)
            lines = p.stdout.splitlines()
            self.assertIn('attempts=0 archived=0 landed_attempts=0 settled_exits=0', lines)
            self.assertIn('verdict=NO_TRADE', lines)
            self.assertTrue(lines[-1].startswith('note=offline accounting'))
            self.assertFalse(p.stdout.lstrip().startswith('{'), 'text, not JSON')

    def test_archive_without_attempt_is_refused_and_creates_nothing(self):
        with tempfile.TemporaryDirectory() as d:
            p = run('fe04', 'archive', '--journal-dir', d, '--wallet', WALLET)
            self.assertEqual(p.returncode, 2)
            self.assertIn('ARCHIVE_NO_ATTEMPT', p.stderr)
            self.assertEqual(list(Path(d).iterdir()), [])

    def test_missing_arguments_print_the_usage(self):
        p = run('fe04', 'accounting')
        self.assertEqual(p.returncode, 2)
        self.assertIn('usage: p0ctl fe04 accounting|archive', p.stderr)


if __name__ == '__main__':
    unittest.main()

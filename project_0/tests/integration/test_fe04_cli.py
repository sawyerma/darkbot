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

    def test_approval_show_reports_the_wallet_history_and_a_spent_approval(self):
        import json
        approval = {
            'schema': 1, 'authorized': True, 'approval_id': 'pilot-1', 'code_sha': 'a' * 40,
            'binary_sha256': 'b' * 64, 'wallet': WALLET, 'program': WALLET,
            'program_data_hash': 'c' * 64, 'funding_class': 'NO_SWAP_INVENTORY',
            'allowed_targets': [WALLET], 'expires_unix_s': 4_000_000_000,
            'max_state_age_ms': 500, 'max_capital_usd_micro': 1, 'max_total_sol_debit_lamports': 1,
            'max_tip_lamports': 0, 'max_rent_lamports': 0, 'max_loss_usd_micro': 1,
            'min_net_usd_micro': 1, 'max_slippage_bps': 1,
        }
        with tempfile.TemporaryDirectory() as d:
            f = Path(d) / 'approval.json'
            f.write_text(json.dumps(approval))
            journals = Path(d) / 'fe04'
            p = run('approval', 'show', str(f), '--journal-dir', str(journals))
            self.assertIn('approval_consumed: false', p.stdout, p.stderr)
            self.assertIn('active_attempt: none', p.stdout)
            self.assertIn('history_verdict: NO_TRADE', p.stdout)
            self.assertEqual(p.returncode, 2, 'test approval binds no real code/binary')
            spent = journals / 'attempts' / WALLET
            spent.mkdir(parents=True)
            (spent / 'pilot-1.p0j').write_bytes(b'archived')
            p = run('approval', 'show', str(f), '--journal-dir', str(journals))
            self.assertIn('approval_consumed: true', p.stdout)
            self.assertEqual(p.returncode, 2)

    def test_missing_arguments_print_the_usage(self):
        p = run('fe04', 'accounting')
        self.assertEqual(p.returncode, 2)
        self.assertIn('usage: p0ctl fe04 accounting|archive', p.stderr)


if __name__ == '__main__':
    unittest.main()

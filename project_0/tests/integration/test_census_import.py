#!/usr/bin/env python3
"""JFI-04 census import through the release `p0-journal` (offline, no network).

`import KIND JOURNAL` decodes each `key=value` stdin row into the typed census row and writes
typed frames (schema 2); a JSON row fails the import and leaves only an INCOMPLETE journal.
"""
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'fixtures'))
import p0_paths  # shared paths; puts project_0 on sys.path

JOURNAL = p0_paths.PROJECT / 'native' / 'target' / 'release' / 'p0-journal'
KEY = '11111111111111111111111111111112'
PROGRAM = 'MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA'


def run(*args, stdin=''):
    return subprocess.run([str(JOURNAL), *args], input=stdin, capture_output=True, text=True,
                          timeout=60)


class CensusImport(unittest.TestCase):
    def setUp(self):
        self.assertTrue(JOURNAL.exists(), 'release p0-journal required')

    def test_rows_become_typed_frames_and_a_text_summary(self):
        rows = (f'pubkey={KEY} lamports=1 owner={PROGRAM} discriminator=AQIDBAUGBwg= space=1864 slot=5\n'
                f'pubkey={PROGRAM} lamports=2 owner={PROGRAM} discriminator=/////////w== space=- slot=5\n')
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'accounts.p0j'
            p = run('import', '900', str(path), stdin=rows)
            self.assertEqual(p.returncode, 0, p.stderr)
            head, *fields = p.stdout.split()
            summary = dict(f.split('=', 1) for f in fields)
            self.assertEqual(head, 'C0_CENSUS_IMPORT')
            self.assertEqual((summary['frames'], summary['status']), ('2', 'COMPLETE'))
            frames = run('frames', str(path))
            self.assertEqual(frames.returncode, 0, frames.stderr)
            census = [line for line in frames.stdout.splitlines() if ' kind=900 ' in line]
            self.assertEqual(len(census), 2)
            self.assertTrue(all(' schema=2 ' in line and ' slot=Some(5) ' in line for line in census))

    def test_a_json_row_is_refused_and_the_journal_is_incomplete(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'accounts.p0j'
            p = run('import', '900', str(path), stdin='{"pubkey":"%s","lamports":1}\n' % KEY)
            self.assertEqual(p.returncode, 2)
            self.assertIn('IMPORT_ROW_FORMAT', p.stderr)
            verify = run('verify', str(path))
            self.assertIn('status=INCOMPLETE ', verify.stdout)  # text report (no JSON)

    def test_runtime_kinds_are_not_importable(self):
        with tempfile.TemporaryDirectory() as d:
            p = run('import', '702', str(Path(d) / 'x.p0j'), stdin='')
            self.assertEqual(p.returncode, 2)
            self.assertIn('KIND_NOT_IMPORTABLE', p.stderr)


if __name__ == '__main__':
    unittest.main()

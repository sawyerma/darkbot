"""DEC-09: the standard production binary carries no historical JSON reader.

The historical readers (schema-1 value-tree JSON views, legacy renders, pre-binary FE-01/FE-04
imports, the A4 parity differential) are compiled only with the opt-in, non-default Cargo feature
`historical`. Checked on the source: every JSON use of a historical-only (H) file lies in code the
standard build does not compile (`#[cfg(feature = "historical")]` or a test), historical-only
binaries require the feature, the feature is opt-in and no production launcher enables it. The
standard build's refusals and its native seal check are Rust tests (`dec09_*`).
"""
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import test_jfi04_json_inventory as inventory  # noqa: E402  (shared scanner and pin)

CRATES = inventory.CRATES
PROJECT = CRATES.parents[1]
# Binaries that read historical sources only.
HISTORICAL_BINS = {'runtime/src/bin/p0-parity.rs'}
# Mixed files: the external wire decoders stay in the standard build, the re-encoders do not.
STANDARD_MIXED = {'solana/src/chain.rs': 9}


class HistoricalFeature(unittest.TestCase):
    def test_standard_build_compiles_no_historical_json_reader(self):
        standard = inventory.scan(CRATES, standard=True)
        for path, (_, categories) in inventory.PINNED.items():
            if 'H' not in categories:
                continue
            if categories == 'H' and path not in HISTORICAL_BINS:
                self.assertEqual(standard.get(path, 0), 0, path)
            else:
                self.assertTrue(path in HISTORICAL_BINS or path in STANDARD_MIXED, path)
        for path, count in STANDARD_MIXED.items():
            self.assertEqual(standard.get(path, 0), count, path)

    def test_historical_only_binaries_require_the_feature(self):
        cargo = (CRATES / 'runtime/Cargo.toml').read_text()
        for path in HISTORICAL_BINS:
            name = Path(path).stem
            block = re.search(r'\[\[bin\]\]\nname = "%s"\npath = "src/bin/%s"\nrequired-features = \["historical"\]'
                              % (re.escape(name), re.escape(Path(path).name)), cargo)
            self.assertIsNotNone(block, path)

    def test_the_feature_is_opt_in_and_no_production_launcher_enables_it(self):
        for crate in ('runtime', 'solana'):
            text = (CRATES / crate / 'Cargo.toml').read_text()
            self.assertRegex(text, r'\ndefault = \[\]\n', crate)
            self.assertIn('\nhistorical = [', text, crate)
        builds = 0
        for script in sorted((PROJECT / 'scripts').glob('*.sh')):
            for line in script.read_text().splitlines():
                if re.search(r'cargo (build|run)\b', line):
                    builds += 1
                    self.assertNotIn('historical', line, f'{script.name}: {line}')
        self.assertGreater(builds, 0)


if __name__ == '__main__':
    unittest.main()

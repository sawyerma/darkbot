"""D11 (P0-PROD-CTRL-08): a value the runtime owns has one default, the runtime's.

The launchers pass an operator value through but never set their own default for a key of the
parameter registry (`runtime_config.rs` `parameters!`) or of `runtime_config::defaults`'
launcher-wide waits — two defaults drifted apart before (recovery catch-up: Rust 30 s, the econ4h
launcher 120 s, Shadow 30 s).
"""
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
CONFIG = ROOT / 'native/crates/solana/src/runtime_config.rs'
SCRIPTS = ROOT / 'scripts'


def runtime_owned_keys():
    text = CONFIG.read_text()
    block = text[text.index('parameters! {'):]
    block = block[:block.index('\n}')]
    keys = set(re.findall(r'"((?:P0|RAYON)_[A-Z0-9_]+)";', block))
    keys |= {'P0_RECOVERY_ROOT_CATCHUP_SECS', 'P0_RECOVERY_ROOT_STALL_SECS',
             'P0_RECOVERY_ROOT_HARD_MAX_SECS'}
    return keys


class SingleDefault(unittest.TestCase):
    def test_launchers_set_no_default_for_a_runtime_owned_key(self):
        keys = runtime_owned_keys()
        self.assertIn('P0_INGRESS_BUFFER', keys)
        found = []
        for script in sorted(SCRIPTS.glob('*.sh')):
            for n, line in enumerate(script.read_text().splitlines(), 1):
                for key in re.findall(r'\$\{([A-Z0-9_]+):-', line):
                    if key in keys:
                        found.append(f'{script.name}:{n} {key}')
        self.assertEqual(found, [])


if __name__ == '__main__':
    unittest.main()

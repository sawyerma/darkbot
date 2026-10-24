#!/usr/bin/env python3
"""A23-09 / JFI-04: the production JSON callsite inventory of the native crates, pinned.

Counts every production use of `serde_json::…`, `json!(` and — in files that import it — bare
`Value` (outside `#[cfg(test)]` modules and test-only files) per file. Each file carries its
categories traced to their real readers (evidence: docs/evidence/BUGS_20261024/a23_09_jfi04_inventory.md
and its 24.10. re-audit):

  W  external wire (Solana JSON-RPC/WSS, Jupiter, Jito)    R  offline render / stdout / export
  H  historical reader or explicit offline import          C  operator config/approval file input
  I  INTERNAL candidate: production JSON as the runtime's own data model or record

The pin is the progress counter: a new file, a changed count or a category change fails until
this table and the evidence are updated together. Offline, stdlib only.
"""
import os
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'fixtures'))
import p0_paths  # shared paths; puts project_0 on sys.path

CRATES = p0_paths.PROJECT / 'native' / 'crates'

# Categories traced to the real readers (24.10. re-audit after the false-green pin: labels had been
# given by the producer's intent; review agent A and the main agent traced every R/H/C file).
PINNED = {
    'runtime/src/analytics/econ.rs': (2, 'I'),  # opportunity_view: JSON view for the Python readers (F4)
    'runtime/src/analytics/econ_canonical.rs': (2, 'I'),  # joined_value view (F4); JSONL replay removed
    'runtime/src/analytics/econ_docs.rs': (12, 'I'),  # legacy_json views (F4)
    'runtime/src/analytics/journal.rs': (24, 'HI'),  # legacy_value of historical frames; export_views (F4)
    'runtime/src/analytics/measurement.rs': (11, 'I'),  # legacy_json views (F4); JSON plan fallback removed (F9, 25.10.)
    'runtime/src/analytics/payload.rs': (1, 'I'),  # legacy_view (F4)
    'runtime/src/analytics/renders.rs': (14, 'I'),  # JSON renders of native journals (F4)
    'runtime/src/bin/p0-fe-qualify.rs': (2, 'I'),  # JSON config / provider evidence input (F6, F13)
    'runtime/src/bin/p0-journal.rs': (8, 'I'),  # renders stdout parsed by Python (F12)
    'runtime/src/bin/p0-parity.rs': (4, 'H'),
    'runtime/src/bin/p0-probe-costs.rs': (12, 'WR'),
    'runtime/src/bin/p0-strict-verify.rs': (1, 'I'),  # --compat job.json from Python (F5)
    'runtime/src/bin/p0ctl.rs': (2, 'C'),  # operator approval file (JSON format: F14)
    'runtime/src/execution/one_shot.rs': (7, 'I'),  # automatic <wallet>.json import in the money path (F8)
    'runtime/src/market/deployment.rs': (1, 'W'),
    'runtime/src/market/observer.rs': (1, 'W'),
    'runtime/src/market/rows.rs': (34, 'I'),  # legacy_json views (F4)
    'runtime/src/market/summary.rs': (4, 'I'),  # legacy_json view (F4)
    'runtime/src/market/winner.rs': (3, 'W'),
    'runtime/src/market_gap.rs': (3, 'W'),
    'runtime/src/market_reconcile.rs': (3, 'W'),
    'runtime/src/market_truth.rs': (2, 'I'),  # human-readable JSON form for the views (F4)
    'runtime/src/market_verification.rs': (19, 'WI'),  # --compat render (F5), JSON in the RPC cache (F10)
    'runtime/src/orchestration/boot_network.rs': (12, 'W'),
    'runtime/src/orchestration/fe_replay.rs': (3, 'H'),
    'runtime/src/orchestration/provider_evidence.rs': (1, 'C'),  # measured operator file (F13)
    'runtime/src/orchestration/startup.rs': (1, 'I'),  # --export-environment JSON round trip (F6)
    'solana/examples/p0-gap-ws-proxy.rs': (10, 'W'),
    'solana/src/bin/p0-probe-v1.rs': (16, 'WR'),
    'solana/src/chain.rs': (40, 'WI'),  # encode_* re-creates JSON for the views (F4)
    'solana/src/ingress/mod.rs': (5, 'W'),
    'solana/src/ingress/ws_rpc.rs': (14, 'W'),
    'solana/src/provider.rs': (11, 'W'),
    'solana/src/runtime_config.rs': (25, 'I'),  # JSON config export/decode round trip (F6)
    'solana/src/state/snapshot.rs': (4, 'W'),
    'solana/src/submit/jito.rs': (17, 'W'),
    'solana/src/submit/rpc.rs': (9, 'W'),
}

# JFI-04 is REOPENED (24.10.): productive internal JSON still exists in these files. Each migration
# removes its file here; the list may only shrink. Never re-label a file to empty it.
OPEN_INTERNAL = sorted(f for f, (_, c) in PINNED.items() if 'I' in c)
EXPECTED_OPEN_INTERNAL = [
    'runtime/src/analytics/econ.rs',
    'runtime/src/analytics/econ_canonical.rs',
    'runtime/src/analytics/econ_docs.rs',
    'runtime/src/analytics/journal.rs',
    'runtime/src/analytics/measurement.rs',
    'runtime/src/analytics/payload.rs',
    'runtime/src/analytics/renders.rs',
    'runtime/src/bin/p0-fe-qualify.rs',
    'runtime/src/bin/p0-journal.rs',
    'runtime/src/bin/p0-strict-verify.rs',
    'runtime/src/execution/one_shot.rs',
    'runtime/src/market/rows.rs',
    'runtime/src/market/summary.rs',
    'runtime/src/market_truth.rs',
    'runtime/src/market_verification.rs',
    'runtime/src/orchestration/startup.rs',
    'solana/src/chain.rs',
    'solana/src/runtime_config.rs',
]

USE_VALUE = re.compile(r'^\s*use serde_json::(Value\b|\{[^}]*\bValue\b)')
JSON = re.compile(r'serde_json::|json!\(')
BARE_VALUE = re.compile(r'(?<![:\w])Value\b')
TEST_MOD = re.compile(r'^\s*(pub(\(crate\))? )?mod \w+ *\{')
TEST_DECL = re.compile(r'^\s*(pub(\(crate\))? )?mod (\w+);')


def test_module_files(root):
    """Files declared `#[cfg(test)] mod x;` (test-only modules in their own file)."""
    out = set()
    for d, _, files in os.walk(root):
        for f in files:
            if not f.endswith('.rs'):
                continue
            lines = (Path(d) / f).read_text(encoding='utf-8').split('\n')
            for i in range(1, len(lines)):
                m = TEST_DECL.match(lines[i])
                if m and lines[i - 1].strip() == '#[cfg(test)]':
                    out.add(Path(d) / (m.group(3) + '.rs'))
    return out


def scan(root):
    skip = test_module_files(root)
    found = {}
    for d, _, files in os.walk(root):
        rel = Path(d).relative_to(root).as_posix()
        if '/tests' in '/' + rel or rel.startswith('target') or '/target' in rel:
            continue
        for f in files:
            path = Path(d) / f
            if not f.endswith('.rs') or f.endswith('_tests.rs') or path in skip:
                continue
            lines = path.read_text(encoding='utf-8').split('\n')
            bare = any(USE_VALUE.match(l) for l in lines)
            n, depth, test_until = 0, 0, None
            for i, line in enumerate(lines):
                code = '' if line.strip().startswith('//') else line.split('//')[0]
                if test_until is None and i and TEST_MOD.match(line) \
                        and lines[i - 1].strip() == '#[cfg(test)]':
                    test_until = depth
                if test_until is None and not USE_VALUE.match(line):
                    n += len(JSON.findall(code))
                    if bare:
                        n += len(BARE_VALUE.findall(code))
                depth += code.count('{') - code.count('}')
                if test_until is not None and depth <= test_until:
                    test_until = None
            if n:
                found[path.relative_to(root).as_posix()] = n
    return found


class JsonInventory(unittest.TestCase):
    def test_production_json_callsites_match_the_pinned_inventory(self):
        found = scan(CRATES)
        new = sorted(set(found) - set(PINNED))
        gone = sorted(set(PINNED) - set(found))
        changed = sorted(
            f'{f}: {PINNED[f][0]} -> {found[f]}'
            for f in set(found) & set(PINNED)
            if found[f] != PINNED[f][0]
        )
        self.assertEqual(
            (new, gone, changed), ([], [], []),
            'JFI-04 inventory drift: classify new/changed JSON uses, update PINNED and the evidence',
        )

    def test_categories_are_known_and_internal_candidates_are_counted(self):
        for f, (_, cats) in PINNED.items():
            self.assertTrue(cats and set(cats) <= set('WRHCI'), f)
        # JFI-04 is not closed: the open internal files are listed, never hidden (see above).
        self.assertEqual(OPEN_INTERNAL, EXPECTED_OPEN_INTERNAL)
        self.assertEqual(sum(n for n, _ in PINNED.values()), 340)


if __name__ == '__main__':
    unittest.main()

"""DEC-14: the canonical register in BUGS.md §22 holds exactly one status row per source ID.

Positive: the committed register passes `project_0/scripts/bugs_register.py`. Negative: a missing
row, a duplicate row, an unknown token, a checkbox/status contradiction, GEGENBELEG for an original
ticket, an unprefixed fehlstellen ID (FS collision), an unknown ID and a changed frozen source are
each refused.
"""
import importlib.util
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('bugs_register', ROOT / 'project_0/scripts/bugs_register.py')
reg = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reg)
BUGS = (ROOT / 'project_0/docs/BUGS.md').read_text()


def row(rid: str) -> str:
    m = re.search(rf'^\| `{re.escape(rid)}` \|.*$', BUGS, re.M)
    assert m, rid
    return m.group(0)


def errors(bugs: str, sources=None) -> list[str]:
    return reg.check(ROOT, bugs=bugs, sources=sources)[0]


class Register(unittest.TestCase):
    def test_committed_register_is_complete_and_consistent(self):
        found, stats = reg.check(ROOT)
        self.assertEqual(found, [])
        self.assertEqual({k: sum(v.values()) for k, v in stats.items() if k != 'p0'},
                         {'original': 148, 'zq': 33, 'fs': 85, 'v2': 15})
        self.assertGreaterEqual(sum(stats['p0'].values()), 5)

    def test_a_missing_row_is_refused(self):
        self.assertIn('FS-A01: no canonical row', errors(BUGS.replace(row('FS-A01') + '\n', '')))

    def test_a_duplicate_row_is_refused(self):
        line = row('Z7')
        self.assertIn('Z7: 2 canonical rows (exactly one required)', errors(BUGS.replace(line, line + '\n' + line)))

    def test_an_unknown_status_token_is_refused(self):
        line = row('V2-12')
        bad = line.replace('| BLOCKED_MESSUNG |', '| BLOCKED |')
        self.assertNotEqual(bad, line)
        self.assertIn("V2-12: unknown status 'BLOCKED'", errors(BUGS.replace(line, bad)))

    def test_a_checkbox_status_contradiction_is_refused(self):
        line = row('H1')  # original checkbox [x]
        bad = line.replace('| ERLEDIGT_LOKAL |', '| OFFEN |')
        self.assertIn('H1: checkbox [x] but status OFFEN', errors(BUGS.replace(line, bad)))
        line = row('M5')  # original checkbox [ ]
        bad = line.replace('| BLOCKED_MESSUNG |', '| ERLEDIGT_LOKAL |')
        self.assertIn('M5: checkbox [ ] but status ERLEDIGT_LOKAL', errors(BUGS.replace(line, bad)))

    def test_gegenbeleg_is_refused_for_an_original_ticket(self):
        line = row('H2')
        bad = line.replace('| ERLEDIGT_LOKAL |', '| GEGENBELEG |')
        self.assertIn('H2: GEGENBELEG is not allowed for an original ticket', errors(BUGS.replace(line, bad)))

    def test_an_unprefixed_fehlstellen_id_collides(self):
        line = row('FS-C08')
        found = errors(BUGS.replace(line, line.replace('`FS-C08`', '`C08`')))
        self.assertIn('C08: canonical row for an unknown ID (fehlstellen IDs carry the FS- prefix)', found)
        self.assertIn('FS-C08: no canonical row', found)

    def test_an_unknown_id_is_refused(self):
        line = row('P0-001')
        self.assertIn('Q99: canonical row for an unknown ID', errors(BUGS.replace(line, line + '\n' + line.replace('P0-001', 'Q99'))))

    def test_a_changed_frozen_source_is_refused(self):
        sources = {p: (ROOT / p).read_bytes() for p in reg.FROZEN}
        sources['project_0/docs/bug_v2.md'] += b'\n'
        self.assertTrue(any(e.startswith('frozen source changed: project_0/docs/bug_v2.md') for e in errors(BUGS, sources)))

    def test_definitions_are_told_apart_from_references(self):
        # Mentions such as "Bezug: B1" or cross-references never create an original ticket.
        self.assertEqual(len(reg.original_tickets(BUGS)), 148)
        self.assertNotIn('ID', reg.original_tickets(BUGS))


if __name__ == '__main__':
    unittest.main()

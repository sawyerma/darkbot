#!/usr/bin/env python3
"""DEC-14 register check: every source ID has exactly one canonical status row in BUGS.md §22.

Sources (definitions, never mere references):
  * BUGS.md §§2–21: the 148 original tickets (`### <ID> — …` with their first checkbox);
  * bugs_b.md: Z1–Z19 and Q1–Q14 (`### Zn — …` headings or bold first table cells);
  * fehlstellen_v1.md: 85 IDs, canonically prefixed `FS-` (table first cells with combined IDs
    expanded, the §6 counter-evidence list E01–E10, the §11 U rows, §11.4 `F10` as `FS-§11.4-F10`);
  * bug_v2.md: V2-01–V2-15 (`### V2-nn — …`);
  * new IDs `P0-nnn` (BUGS.md §23).
The canonical rows live in BUGS.md §22.1: `| `ID` | STATUS | origin | rest | section | evidence |`.

Checks: completeness and uniqueness per group, known status tokens, the checkbox mapping of the
148 originals, FS-prefix collisions, unknown row IDs, and the frozen sources' pinned blob SHAs.
Run: `python3 project_0/scripts/bugs_register.py [--repo ROOT]` (exit 1 with the violations).
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import re
import subprocess
import sys
from pathlib import Path

STATUSES = (
    'ERLEDIGT_LOKAL', 'OFFEN', 'TEILWEISE', 'BLOCKED_EXTERNAL', 'BLOCKED_MESSUNG',
    'REVIEW_REQUIRED', 'WIDERLEGT', 'DUPLIKAT', 'GEGENBELEG',
)
DONE = {'ERLEDIGT_LOKAL', 'WIDERLEGT', 'DUPLIKAT'}
NOT_DONE = {'OFFEN', 'TEILWEISE', 'BLOCKED_EXTERNAL', 'BLOCKED_MESSUNG', 'REVIEW_REQUIRED'}
# N-3: the frozen source reports, pinned by their git blob SHA (DEC-14 consolidation).
FROZEN = {
    'project_0/docs/bugs_b.md': '9e447f334d637c6cef031a20598b8ef0ba255aec',
    'project_0/docs/fehlstellen_v1.md': 'bbad5d713b145cd52f6f2b5e7b46b1b8b205a5c0',
    'project_0/docs/bug_v2.md': '477bf1af9865ca502cfd9b052f6881c9a79d3c0f',
}
EXPECTED = {'original': 148, 'zq': 33, 'fs': 85, 'v2': 15}
ORIGINAL_ID = re.compile(r'^(?:[HMNLBCDEFXRV]\d+|A23-\d\d|P1B-T1|CI-N1|P6MT-J1|P6MT-R1)$')
P0_ID = re.compile(r'^P0-\d{3}$')
ROW = re.compile(r'^\| `(?P<id>[^`]+)` \| (?P<status>[^|]+?) \| (?P<origin>[^|]*?) \| (?P<rest>[^|]*?) \|'
                 r' (?P<section>[^|]*?) \| (?P<evidence>[^|]*?) \|\s*$')


def blob_sha(data: bytes) -> str:
    return hashlib.sha1(b'blob %d\0' % len(data) + data).hexdigest()


def original_tickets(bugs: str) -> dict[str, str]:
    """ID → first checkbox ('x' or ' ') of every ticket definition in §§2–21."""
    body = bugs[:bugs.index('\n## 22. ')]
    heads = [(m.start(), m.group(1)) for m in re.finditer(r'^### (\S+)', body, re.M)]
    out: dict[str, str] = {}
    for i, (pos, tid) in enumerate(heads):
        if not ORIGINAL_ID.match(tid):
            continue
        end = heads[i + 1][0] if i + 1 < len(heads) else len(body)
        box = re.search(r'^- \[( |x)\]', body[pos:end], re.M)
        if tid in out:
            out[tid + '#DUP'] = box.group(1) if box else '?'
        out[tid] = box.group(1) if box else '?'
    return out


def zq_ids(text: str) -> set[str]:
    ids = set(re.findall(r'^### ([ZQ]\d+) —', text, re.M))
    ids |= set(re.findall(r'^\| \*\*([ZQ]\d+)\*\* \|', text, re.M))
    return ids


def _expand(cell: str) -> list[str]:
    cell = cell.strip().strip('*').strip()
    out = []
    for part in cell.split('/'):
        m = re.match(r'^([A-Z])(\d\d)–\1?(\d\d)$', part)
        if m:
            out += [f'{m.group(1)}{n:02d}' for n in range(int(m.group(2)), int(m.group(3)) + 1)]
        else:
            out.append(part)
    return out


def fs_ids(text: str) -> set[str]:
    ids: set[str] = set()
    for cell in re.findall(r'^\| ([^|]+?) \|', text, re.M):
        for part in _expand(cell):
            if re.match(r'^(?:[ABCD]\d\d|F-\d\d|U-\d+)$', part):
                ids.add(f'FS-{part}')
    if re.search(r'^\| F10 \(JSON-RPC-Cache\) \|', text, re.M):
        ids.add('FS-§11.4-F10')
    counter = text[text.index('## 6. Audit-Block E'):]
    counter = counter[:counter.index('\n---')]
    ids |= {f'FS-{e}' for e in re.findall(r'\b(E\d\d)\b', counter)}
    return ids


def v2_ids(text: str) -> set[str]:
    return set(re.findall(r'^### (V2-\d\d) —', text, re.M))


def canonical_rows(bugs: str) -> list[dict[str, str]]:
    section = bugs[bugs.index('\n## 22. '):]
    end = section.find('\n## 23. ')
    section = section if end < 0 else section[:end]
    rows = []
    for line in section.splitlines():
        m = ROW.match(line)
        if m and not m.group('id').startswith('ID'):
            rows.append({k: v.strip() for k, v in m.groupdict().items()})
    return rows


def check(root: Path, bugs: str | None = None, sources: dict[str, bytes] | None = None) -> tuple[list[str], dict]:
    """The violations (empty: the register is consistent) and the statistics."""
    errors: list[str] = []
    bugs = bugs if bugs is not None else (root / 'project_0/docs/BUGS.md').read_text()
    sources = sources if sources is not None else {p: (root / p).read_bytes() for p in FROZEN}
    for path, pinned in FROZEN.items():
        if blob_sha(sources[path]) != pinned:
            errors.append(f'frozen source changed: {path} blob {blob_sha(sources[path])} != pinned {pinned}')
    text = {p: b.decode() for p, b in sources.items()}
    originals = original_tickets(bugs)
    groups = {
        'original': set(originals),
        'zq': zq_ids(text['project_0/docs/bugs_b.md']),
        'fs': fs_ids(text['project_0/docs/fehlstellen_v1.md']),
        'v2': v2_ids(text['project_0/docs/bug_v2.md']),
    }
    for name, ids in groups.items():
        dup = [i for i in ids if i.endswith('#DUP')]
        if dup:
            errors.append(f'{name}: duplicate definitions {sorted(dup)}')
        if len(ids) != EXPECTED[name]:
            errors.append(f'{name}: {len(ids)} source IDs, expected {EXPECTED[name]}')
    known = set().union(*groups.values())
    rows = canonical_rows(bugs)
    seen = collections.Counter(r['id'] for r in rows)
    for rid, n in seen.items():
        if n > 1:
            errors.append(f'{rid}: {n} canonical rows (exactly one required)')
        if rid not in known and not P0_ID.match(rid):
            hint = ' (fehlstellen IDs carry the FS- prefix)' if f'FS-{rid}' in groups['fs'] else ''
            errors.append(f'{rid}: canonical row for an unknown ID{hint}')
    for rid in sorted(known - set(seen)):
        errors.append(f'{rid}: no canonical row')
    status = {}
    for r in rows:
        token = r['status']
        if token not in STATUSES:
            errors.append(f"{r['id']}: unknown status {token!r}")
            continue
        status[r['id']] = token
        if not r['origin']:
            errors.append(f"{r['id']}: origin missing")
    for tid, box in originals.items():
        if tid.endswith('#DUP') or tid not in status:
            continue
        token = status[tid]
        if token == 'GEGENBELEG':
            errors.append(f'{tid}: GEGENBELEG is not allowed for an original ticket')
        elif box == 'x' and token not in DONE:
            errors.append(f'{tid}: checkbox [x] but status {token}')
        elif box == ' ' and token not in NOT_DONE:
            errors.append(f'{tid}: checkbox [ ] but status {token}')
        elif box not in ('x', ' '):
            errors.append(f'{tid}: no checkbox in its original section')
    for rid, token in status.items():
        if P0_ID.match(rid) and token == 'GEGENBELEG':
            errors.append(f'{rid}: GEGENBELEG is not allowed for a P0 ticket')
    stats: dict = {name: collections.Counter() for name in groups}
    stats['p0'] = collections.Counter()
    for rid, token in status.items():
        name = next((n for n, ids in groups.items() if rid in ids), 'p0' if P0_ID.match(rid) else None)
        if name:
            stats[name][token] += 1
    return errors, stats


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[2])
    args = ap.parse_args()
    errors, stats = check(args.repo)
    try:
        sha = subprocess.check_output(['git', '-C', str(args.repo), 'rev-parse', 'HEAD'], text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        sha = 'UNKNOWN'
    print(f'register at {sha}')
    for name in ('original', 'zq', 'fs', 'v2', 'p0'):
        counts = stats[name]
        print(f"  {name:8} {sum(counts.values()):3}  " + ', '.join(f'{k} {counts[k]}' for k in STATUSES if counts[k]))
    for e in errors:
        print(f'VIOLATION {e}', file=sys.stderr)
    return 1 if errors else 0


if __name__ == '__main__':
    sys.exit(main())

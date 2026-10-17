#!/usr/bin/env python3
"""Ticket 3 A4 offline differential: the pinned Python verifier decoder against the Rust decode.

Both sides read the same historical provider bytes (read-only): `getTransaction` results and the
raw `full_slot_reconciliation` / `logsSubscribe` payloads of historical market runs. The Python
side imports `operations.verification.audit_market_quick` unchanged and uses its `detect` and
`decode_logs`; the Rust side is `p0-parity`. Lines are compared field by field. Offline
qualification only; no RPC, no production path, nothing is written into runs.

usage: parity_differential.py RUST_BINARY tx FILE... -- rows FILE...
"""
import json
import subprocess
import sys
from pathlib import Path

if __package__ in (None, ''):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from operations.verification import audit_market_quick as a


def opt(v):
    return '-' if v is None else str(v)


def tx_line(raw, slot):
    r = a.detect(raw, slot)
    status = 'UNKNOWN' if r['success'] is None else ('SUCCESS' if r['success'] else 'FAILED')
    markers = sorted(f"{i['marker']}@{i['position'][0]}.{opt(i['position'][1])}"
                     for i in r.get('instruction_evidence', []) if i.get('marker'))
    events = sorted(f"{e['type']}:{e['target']}:{e['actor']}:{e['event_index']}:{e.get('seized_asset_native_exact') or '-'}"
                    for e in r['events'])
    return (f"TX {r['signature']} {slot} status={status} fee={opt(r.get('fee_lamports'))} "
            f"priority={opt(r.get('priority_fee_lamports'))} version={opt(r.get('version'))} "
            f"payer={opt(r.get('fee_payer'))} markers={','.join(markers)} events={';'.join(events)} "
            f"class={r['classification']}")


def logs_line(raw, signature, slot):
    v = raw.get('value') or {}
    status = 'UNKNOWN' if 'err' not in v else ('SUCCESS' if v['err'] is None else 'FAILED')
    events, errors = a.decode_logs(v.get('logs') or [])
    ev = sorted(f"{e['type']}:{e['target']}:{e['actor']}" for e in events)
    truncated = 'true' if 'LOGS_TRUNCATED' in errors else 'false'
    return f"LOGS {signature} {slot} status={status} truncated={truncated} events={';'.join(ev)}"


def python_lines(tx_files, row_files):
    out = []
    for f in tx_files:
        raw = json.loads(Path(f).read_text())
        out.append(tx_line(raw, raw.get('slot')))
    for f in row_files:
        for line in Path(f).read_text().splitlines():
            try:
                row = json.loads(line)
            except ValueError:
                # A killed historical capture can end in a partial line: reported, kept as is.
                out.append(f'TRUNCATED_LINE {f}')
                continue
            if row.get('source') == 'full_slot_reconciliation':
                out.append(tx_line(row['raw'], row['slot']))
            elif row.get('source') == 'logsSubscribe':
                out.append(logs_line(row['raw'], row['signature'], row['slot']))
    return out


def fields(line):
    head, *rest = line.split(' ', 3)
    tokens = line.split(' ')
    return tokens[:3], dict(t.split('=', 1) for t in tokens[3:] if '=' in t)


def main(argv):
    if len(argv) < 2 or 'tx' not in argv:
        print(__doc__, file=sys.stderr)
        return 2
    binary, rest = argv[0], argv[1:]
    split = rest.index('--') if '--' in rest else len(rest)
    tx_files, row_files = rest[1:split], rest[split + 2:] if split < len(rest) else []
    rust = subprocess.run([binary, 'tx', *tx_files], capture_output=True, text=True, check=True).stdout.splitlines()
    if row_files:
        rust += subprocess.run([binary, 'rows', *row_files], capture_output=True, text=True, check=True).stdout.splitlines()
    py = python_lines(tx_files, row_files)
    if len(rust) != len(py):
        print(f'LINE_COUNT_MISMATCH rust={len(rust)} python={len(py)}')
        return 1
    by_field, mismatches, kinds = {}, [], {}
    for r, p in zip(rust, py):
        (rk, pk) = (r.split(' ')[0], p.split(' ')[0])
        kinds[rk] = kinds.get(rk, 0) + 1
        (rid, rf), (pid, pf) = fields(r), fields(p)
        diff = sorted(k for k in set(rf) | set(pf) if rf.get(k) != pf.get(k))
        if rid != pid:
            diff.insert(0, 'identity')
        for k in diff:
            by_field[k] = by_field.get(k, 0) + 1
        if diff:
            mismatches.append((diff, r, p))
    print(f'compared={len(rust)} kinds={kinds} matched={len(rust) - len(mismatches)} mismatched={len(mismatches)}')
    print('mismatches_by_field=' + ','.join(f'{k}:{v}' for k, v in sorted(by_field.items())))
    for diff, r, p in mismatches[:60]:
        print('DIFF ' + ','.join(diff))
        print('  rust   ' + r)
        print('  python ' + p)
    return 0 if not mismatches else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))

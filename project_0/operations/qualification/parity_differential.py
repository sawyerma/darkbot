#!/usr/bin/env python3
"""Ticket 3 A4 offline differential: the pinned Python verifier decoder against the Rust decode.

Both sides read the same provider bytes (read-only): historical `getTransaction` results, the raw
`full_slot_reconciliation` / `logsSubscribe` payloads of historical market runs, and documented
mutations of real historical transactions (missing meta/err, failed, undecodable error,
truncated/missing logs, missing loaded addresses or inner instructions). The Python side imports
`operations.verification.audit_market_quick` unchanged and decodes independently with its
`detect` and `decode_logs`; the Rust side is `p0-parity`. Lines keep execution/log order.
Every compared field must agree; a difference is accepted only by an explicit rule below (each
rule states why); anything else fails. All differences are printed, none is capped.
Offline qualification only: no RPC, no production path, nothing is written into runs.

usage: parity_differential.py RUST_BINARY tx FILE... [-- rows FILE...] [-- mut FILE...]
       parity_differential.py RUST_BINARY index tx FILE... [-- rows FILE...] [-- mut FILE...]

DEC-08 `index`: the INDEX verifier's reading of the same sources against the pinned verifier's
own — per transaction `detect` (classification, program events per kind, events) against
`index::classify`, per live logs notification the import's LIVE_PAYLOAD events (`decode_logs`,
kept only without errors) against `index::live_payload`. Accepted differences are the I-rules
in `explain_index`; anything else fails.
"""
import copy
import json
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path

if __package__ in (None, ''):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from operations.verification import audit_market_quick as a

MUTATIONS = {
    'meta_missing': lambda t: t.pop('meta'),
    'err_missing': lambda t: t['meta'].pop('err'),
    'failed': lambda t: t['meta'].__setitem__('err', {'InstructionError': [0, {'Custom': 1}]}),
    'err_undecodable': lambda t: t['meta'].__setitem__('err', {'FutureVariant': {'x': 1}}),
    'logs_truncated': lambda t: t['meta']['logMessages'].append('Log truncated'),
    'logs_missing': lambda t: t['meta'].pop('logMessages'),
    'loaded_missing': lambda t: t['meta'].pop('loadedAddresses'),
    'inner_missing': lambda t: t['meta'].pop('innerInstructions'),
}


def opt(v):
    return '-' if v is None else str(v)


def ordered(items):
    seen, out = set(), []
    for i in items:
        if i not in seen:
            seen.add(i)
            out.append(i)
    return out


def tx_line(raw, slot):
    r = a.detect(raw, slot)
    status = 'UNKNOWN' if r['success'] is None else ('SUCCESS' if r['success'] else 'FAILED')
    markers = [f"{i['marker']}@{i['position'][0]}.{opt(i['position'][1])}"
               for i in r.get('instruction_evidence', []) if i.get('marker')]
    related = [f"{i['related_type']}@{i['position'][0]}.{opt(i['position'][1])}:{opt(i.get('target'))}:{opt(i.get('actor'))}"
               for i in r.get('instruction_evidence', []) if i.get('related_type')]
    events = [f"{e['type']}:{e['target']}:{e['actor']}:{e['event_index']}:{e.get('seized_asset_native_exact') or '-'}"
              for e in r['events']]
    sigs = (raw.get('transaction') or {}).get('signatures')
    return (f"TX {r['signature']} {slot} status={status} fee={opt(r.get('fee_lamports'))} "
            f"priority={opt(r.get('priority_fee_lamports'))} version={opt(r.get('version'))} "
            f"payer={opt(r.get('fee_payer'))} sigs={len(sigs) if isinstance(sigs, list) else '-'} "
            f"markers={','.join(markers)} related={','.join(related)} events={';'.join(events)} class={r['classification']} "
            f"reasons={','.join(ordered(r['unknown_reasons']))}")


def logs_line(raw, signature, slot):
    v = raw.get('value') or {}
    status = 'UNKNOWN' if 'err' not in v else ('SUCCESS' if v['err'] is None else 'FAILED')
    events, errors = a.decode_logs(v.get('logs') or [])
    # Same grouping as the Rust line: receivership events, then the other classes, each in log
    # order (a stable partition, not a sort).
    ev = [e for e in events if e['type'] == 'RECEIVERSHIP'] + [e for e in events if e['type'] != 'RECEIVERSHIP']
    truncated = 'true' if 'LOGS_TRUNCATED' in errors else 'false'
    return (f"LOGS {signature} {slot} status={status} truncated={truncated} "
            f"events={';'.join(e['type'] + ':' + e['target'] + ':' + e['actor'] for e in ev)} "
            f"reasons={','.join(ordered(errors))}")


def mutate(files, out):
    made = []
    for f in files:
        base = json.loads(Path(f).read_text())
        for name, change in MUTATIONS.items():
            t = copy.deepcopy(base)
            try:
                change(t)
            except (KeyError, AttributeError):
                continue  # the field does not exist in this transaction
            p = Path(out) / f"{Path(f).stem}.{name}.json"
            p.write_text(json.dumps(t))
            made.append((str(p), f"{Path(f).stem[:12]}…/{name}"))
    return made


def python_side(tx, rows):
    out = []
    for f, _ in tx:
        raw = json.loads(Path(f).read_text())
        out.append(tx_line(raw, raw.get('slot')))
    for f in rows:
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


def parse(line):
    t = line.split(' ')
    return t[:3], dict(x.split('=', 1) for x in t[3:] if '=' in x)


def num(v):
    return int(v) if v not in (None, '-', '') else None


def explain(diff, r, p):
    """Accepted differences; each names the reason and requires everything else to agree.
    E1 applies per field (it can combine with one other rule); the others need an exact match."""
    d, notes = set(diff), []
    rr = r.get('reasons', '').split(',')
    if ('priority' in d and p.get('priority') == '-' and r.get('version') == '1'
            and None not in (num(r.get('priority')), num(r.get('fee')), num(r.get('sigs')))
            and num(r['fee']) == num(r['priority']) + 5000 * num(r['sigs'])):
        d.discard('priority')
        notes.append('E1_PINNED_PYTHON_V1_PRIORITY_UNKNOWN (Rust = v1 transactionConfig = meta.fee - signature fees)')
    unknown_both = (r.get('status') == p.get('status') == 'UNKNOWN' and r.get('class') == p.get('class') == 'UNKNOWN'
                    and not r.get('events') and not p.get('events'))
    if not d:
        pass
    elif (r.get('status') == 'UNKNOWN' and 'TX_ERROR_UNDECODABLE' in rr and p.get('status') == 'FAILED'
            and not r.get('events') and not p.get('events') and d <= {'status', 'class'}):
        notes.append('E2_UNDECODABLE_ERROR_IS_UNKNOWN_BY_CONTRACT (never success on either side)')
    elif (r.get('class') == 'UNKNOWN' and 'INNER_INSTRUCTIONS_UNAVAILABLE' in rr
            and p.get('class') == 'CLASSIFIED' and d <= {'class', 'events', 'related'}
            and ('related' not in d or r.get('related') == 'UNKNOWN')):
        notes.append('E3_RUST_FAILS_CLOSED_WITHOUT_INNER_INSTRUCTIONS (CPI coverage unprovable)')
    elif (p.get('class') == 'UNKNOWN' and 'LOGS_TRUNCATED' in p.get('reasons', '').split(',')
            and r.get('class') == 'CLASSIFIED' and d == {'class'}):
        notes.append('E4_TRUNCATED_LOGS_QUALIFIED_BY_INSTRUCTIONS (identical events; r6 instruction parity)')
    elif (unknown_both and ('TX_META_MISSING' in rr or 'TX_META_STATUS_MISSING' in rr)
            and d <= {'fee', 'priority', 'payer', 'markers', 'version', 'related'}
            and all(p.get(k) in ('-', '') for k in d)):
        notes.append('E5_STATUS_UNKNOWN_WITH_MESSAGE_FACTS (Python stops before meta/message facts; status, class and events agree)')
    elif (d == {'related'} and r.get('related') == 'UNKNOWN' and p.get('related') == ''
            and r.get('class') == p.get('class') == 'UNKNOWN' and not r.get('events') and not p.get('events')
            and not r.get('markers') and not p.get('markers') and 'LOADED_ADDRESSES_MISSING' in rr):
        notes.append('E6_RELATED_UNKNOWN_WITHOUT_ACCOUNT_KEYS (Rust cannot prove absence; Python stops before instruction evidence)')
    else:
        return None
    return ' + '.join(notes)


def index_tx_line(raw, slot):
    r = a.detect(raw, slot)
    counts = ','.join(str((r.get('program_events') or {}).get(k, 0)) for k in a.KINDS)
    events = ';'.join(f"{e['type']}:{e['target']}:{e['actor']}:{e['event_index']}" for e in r['events'])
    return (f"INDEX {r['signature']} {slot} class={r['classification']} program_events={counts} "
            f"events={events} reasons={','.join(ordered(r['unknown_reasons']))}")


def payload_line(row):
    # The pinned import (`Audit.import_bot.transaction`): a successful live row's logs, decoded
    # without any error, else no LIVE_PAYLOAD event.
    raw = row.get('raw') or {}
    value = raw.get('value', raw)
    logs = value.get('logs') or (value.get('meta') or {}).get('logMessages')
    text = 'NONE'
    if row.get('success') is True and isinstance(logs, list):
        events, errors = a.decode_logs(logs)
        if not errors:
            text = ';'.join(f"{e['type']}:{e['target']}:{e['actor']}:{e['event_index']}" for e in events)
    return f"PAYLOAD {row['signature']} {row['slot']} events={text}"


def python_index_side(tx, rows):
    out = []
    for f, _ in tx:
        raw = json.loads(Path(f).read_text())
        out.append(index_tx_line(raw, raw.get('slot')))
    for f in rows:
        for line in Path(f).read_text().splitlines():
            try:
                row = json.loads(line)
            except ValueError:
                out.append(f'TRUNCATED_LINE {f}')
                continue
            if row.get('source') == 'full_slot_reconciliation':
                out.append(index_tx_line(row['raw'], row['slot']))
            elif row.get('source') == 'logsSubscribe':
                out.append(payload_line(row))
    return out


def explain_index(diff, r, p):
    """Accepted INDEX differences (each names its reason; everything else must agree). The
    A4 rules carry over where the classification differs for the same documented cause."""
    d = set(diff)
    rr, pr = r.get('reasons', '').split(','), p.get('reasons', '').split(',')
    if (d == {'class'} and r.get('class') == 'UNKNOWN' and 'TX_ERROR_UNDECODABLE' in rr
            and p.get('class') == 'FAILED_TRANSACTION' and not r.get('events') and not p.get('events')):
        return 'I2=E2_UNDECODABLE_ERROR_IS_UNKNOWN_BY_CONTRACT (never success; no events on either side)'
    if (r.get('class') == 'UNKNOWN' and 'INNER_INSTRUCTIONS_UNAVAILABLE' in rr and p.get('class') == 'CLASSIFIED'
            and d <= {'class', 'events'} and not r.get('events')):
        return 'I3=E3_RUST_FAILS_CLOSED_WITHOUT_INNER_INSTRUCTIONS (CPI coverage unprovable; observed counts agree)'
    if (d == {'class'} and p.get('class') == 'UNKNOWN' and 'LOGS_TRUNCATED' in pr
            and r.get('class') == 'CLASSIFIED'):
        return 'I4=E4_TRUNCATED_LOGS_QUALIFIED_BY_INSTRUCTIONS (identical events and counts; r6 instruction parity)'
    if (d == {'program_events'} and r.get('class') == p.get('class') == 'UNKNOWN'
            and not r.get('events') and not p.get('events') and 'LOADED_ADDRESSES_MISSING' in rr
            and pr == ['TRANSACTION_LAYOUT_OR_PAYLOAD_UNKNOWN'] and p.get('program_events') == '0,0,0,0'):
        return ('I6_PYTHON_STOPS_BEFORE_THE_LOGS (successful transaction without its loaded addresses: the pinned '
                'verifier stops at key resolution; the native INDEX reads the program-attributed log events as observed only, never qualified)')
    if (d == {'events'} and r.get('class') == p.get('class') == 'UNKNOWN' and not r.get('events')):
        return 'I5_UNQUALIFIED_EVENTS_ARE_OBSERVED_ONLY (both UNKNOWN, counts agree; the native INDEX never compares unqualified events)'
    return None


def main_index(binary, rest):
    sections, current = {'tx': [], 'rows': [], 'mut': []}, None
    for x in rest:
        if x in sections:
            current = x
        elif x != '--':
            sections[current].append(x)
    with tempfile.TemporaryDirectory(prefix='p0-index-parity-') as tmp:
        tx = [(f, Path(f).stem[:12] + '…') for f in sections['tx']] + mutate(sections['mut'], tmp)
        args = [binary, 'index', 'tx', *[f for f, _ in tx]]
        if sections['rows']:
            args += ['--', 'rows', *sections['rows']]
        rust = subprocess.run(args, capture_output=True, text=True, check=True).stdout.splitlines()
        py = python_index_side(tx, sections['rows'])
    labels = [l for _, l in tx] + ['rows'] * (len(py) - len(tx))
    if len(rust) != len(py):
        print(f'LINE_COUNT_MISMATCH rust={len(rust)} python={len(py)}')
        return 1
    kinds, explained, unexplained, by_field = Counter(), Counter(), [], Counter()
    for label, r_line, p_line in zip(labels, rust, py):
        kinds[r_line.split(' ')[0]] += 1
        (rid, r), (pid, p) = parse(r_line), parse(p_line)
        diff = sorted(k for k in set(r) | set(p) if k != 'reasons' and r.get(k) != p.get(k))
        if rid != pid:
            diff.insert(0, 'identity')
        if not diff:
            continue
        for k in diff:
            by_field[k] += 1
        why = explain_index(diff, r, p) if rid == pid else None
        if why:
            explained[why] += 1
        else:
            unexplained.append((label, diff, r_line, p_line))
    print(f'compared={len(rust)} kinds={dict(kinds)} identical={len(rust) - sum(explained.values()) - len(unexplained)} '
          f'explained={sum(explained.values())} unexplained={len(unexplained)}')
    print('differences_by_field=' + ','.join(f'{k}:{v}' for k, v in sorted(by_field.items())))
    for why, n in sorted(explained.items()):
        print(f'rule {n:5d} {why}')
    for label, diff, r_line, p_line in unexplained:
        print(f'UNEXPLAINED {label} {",".join(diff)}')
        print('  rust   ' + r_line)
        print('  python ' + p_line)
    return 0 if not unexplained else 1


def main(argv):
    if len(argv) >= 2 and argv[1] == 'index':
        return main_index(argv[0], argv[2:])
    if len(argv) < 2 or 'tx' not in argv:
        print(__doc__, file=sys.stderr)
        return 2
    binary, rest = argv[0], argv[1:]
    sections, current = {'tx': [], 'rows': [], 'mut': []}, None
    for x in rest:
        if x in sections:
            current = x
        elif x != '--':
            sections[current].append(x)
    with tempfile.TemporaryDirectory(prefix='p0-parity-') as tmp:
        tx = [(f, Path(f).stem[:12] + '…') for f in sections['tx']] + mutate(sections['mut'], tmp)
        rust = subprocess.run([binary, 'tx', *[f for f, _ in tx]], capture_output=True, text=True, check=True).stdout.splitlines()
        if sections['rows']:
            rust += subprocess.run([binary, 'rows', *sections['rows']], capture_output=True, text=True, check=True).stdout.splitlines()
        py = python_side(tx, sections['rows'])
    labels = [l for _, l in tx] + ['rows'] * (len(py) - len(tx))
    if len(rust) != len(py):
        print(f'LINE_COUNT_MISMATCH rust={len(rust)} python={len(py)}')
        return 1
    kinds, explained, unexplained, by_field = Counter(), Counter(), [], Counter()
    for label, r_line, p_line in zip(labels, rust, py):
        kinds[r_line.split(' ')[0]] += 1
        (rid, r), (pid, p) = parse(r_line), parse(p_line)
        diff = sorted(k for k in set(r) | set(p) if k != 'reasons' and r.get(k) != p.get(k))
        if rid != pid:
            diff.insert(0, 'identity')
        if not diff:
            continue
        for k in diff:
            by_field[k] += 1
        why = explain(diff, r, p) if rid == pid else None
        if why:
            explained[why] += 1
            print(f'EXPLAINED {label} {",".join(diff)} :: {why}')
        else:
            unexplained.append((label, diff, r_line, p_line))
        print('  rust   ' + r_line)
        print('  python ' + p_line)
    print(f'compared={len(rust)} kinds={dict(kinds)} identical={len(rust) - sum(explained.values()) - len(unexplained)} '
          f'explained={sum(explained.values())} unexplained={len(unexplained)}')
    print('differences_by_field=' + ','.join(f'{k}:{v}' for k, v in sorted(by_field.items())))
    for why, n in sorted(explained.items()):
        print(f'rule {n:5d} {why}')
    for label, diff, r_line, p_line in unexplained:
        print(f'UNEXPLAINED {label} {",".join(diff)}')
        print('  rust   ' + r_line)
        print('  python ' + p_line)
    return 0 if not unexplained else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))

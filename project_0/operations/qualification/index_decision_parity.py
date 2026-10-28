#!/usr/bin/env python3
"""DEC-08 decision parity: the pinned verifier's INDEX decision over a run and fixture provider
answers, printed as canonical lines for comparison with the native INDEX result.

The pinned `operations.verification.audit_market_quick` is imported unchanged and runs its own
`Audit.execute()` — import of the capture's historical views, pagination, transaction fetch,
`compare` and `report` — against an RPC whose opener answers from fixture files (no network):
`signatures_head.json`, `signatures_<before>.json`, `transaction_<signature>.json` (absent:
`null`), `error_<signature>.json` (a JSON-RPC error object). Its output goes to OUTPUT_DIR;
nothing is written into the run. Offline qualification only, never a production path.

usage: index_decision_parity.py RUN PROVIDER_DIR MAX_PAGES OUTPUT_DIR
"""
import json
import sys
from pathlib import Path

if __package__ in (None, ''):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from operations.verification import audit_market_quick as a


class Response:
    def __init__(self, body):
        self.body = body

    def read(self, n):
        return self.body[:n]

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


def opener(provider):
    def answer(request, timeout=None):
        q = json.loads(request.data)
        method, params = q['method'], q['params']
        if method == 'getSignaturesForAddress':
            name = 'signatures_' + (params[1].get('before') or 'head')
        else:
            name = 'transaction_' + params[0]
            error = provider / ('error_' + params[0] + '.json')
            if error.exists():
                body = {'jsonrpc': '2.0', 'id': q['id'], 'error': json.loads(error.read_text())}
                return Response(json.dumps(body).encode())
        path = provider / (name + '.json')
        result = json.loads(path.read_text()) if path.exists() else None
        return Response(json.dumps({'jsonrpc': '2.0', 'id': q['id'], 'result': result}).encode())
    return answer


def flag(v):
    return 'true' if v else 'false'


def lines(summary, out):
    yield (f"STATUS status={summary['status']} reason={summary['reason'] or 'none'} "
           f"incomplete={','.join(summary['incomplete_reasons'])}")
    yield (f"SCOPE crossed_start={flag(summary['crossed_start'])} head_after_end={flag(summary['head_after_end'])} "
           f"pages={summary['pages_persisted']} indexed={summary['indexed_signatures']} "
           f"in_window={summary['in_window_transactions']} unresolved={summary['unresolved_transactions']} "
           f"unknown={summary['unknown_transactions']}")
    yield 'QUALIFIED ' + ' '.join(f"{k}={summary['qualified_liquidations'][k]}" for k in a.KINDS)
    yield 'OBSERVED ' + ' '.join(f"{k}={summary['observed_events_including_unknown_transactions'][k]}" for k in a.KINDS)
    yield 'COUNTS ' + ','.join(f'{k}={v}' for k, v in sorted(summary['comparison_counts'].items()))
    yield f"ZERO proven={flag(summary['zero_liquidations_proven_within_index_scope'])}"
    only = sorted((r['signature'], r['slot']) for r in map(json.loads, (out / 'bot_only.jsonl').read_text().splitlines()))
    yield 'BOT_ONLY ' + ','.join(f'{s}:{n}' for s, n in only)
    compared = []
    for line in (out / 'comparison.jsonl').read_text().splitlines():
        c = json.loads(line)['darkbot']
        if not c['events']:
            continue
        events = ';'.join(
            f"{e['event']['event_index']}:{e['event']['type']}:{e['detection']}:{e['shadow_status']}:"
            + '|'.join(sorted(f"{m['source']}={m['status']}" for m in e['comparisons']))
            for e in c['events'])
        compared.append((c['signature'], c['slot'],
                         f"COMPARE {c['signature']} {c['slot']} tx={c['transaction_status']} "
                         f"coverage={c['live_coverage']} class={c['classification']} events={events}"))
    for _, _, text in sorted(compared):
        yield text


def main(argv):
    if len(argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    run, provider, pages, out = Path(argv[0]), Path(argv[1]), int(argv[2]), Path(argv[3])
    rpc = a.RPC('fixture://provider', rps=1000, seconds=60, opener=opener(provider))
    summary = a.Audit(run, rpc, max_pages=pages, seconds=60, output=out).execute()
    for line in lines(summary, out):
        print(line)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))

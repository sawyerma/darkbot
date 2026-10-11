#!/usr/bin/env python3
"""Independent read-only Solana finalized signature-index audit of a timeboxed Project_0 capture.

Does not import or call the DarkBot market decoder. Counts actual program
instructions and program-emitted event discriminators from raw finalized blocks.
Results are written into the selected run directory, never into bot ledgers.
No wallet, signer, or submission required. RPC URL is never printed or saved.
"""
import argparse
import base64
import collections
import hashlib
import json
import os
from pathlib import Path
import sys
import time
import urllib.error
import urllib.request

PROGRAM = 'MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA'
ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
INSTRUCTION_TYPES = {
    'lending_account_liquidate': 'CLASSIC',
    'end_liquidation': 'RECEIVERSHIP_END',
    'start_liquidation': 'RECEIVERSHIP_START',
    'end_deleverage': 'FORCED_DELEVERAGE',
    'lending_pool_handle_bankruptcy': 'BANKRUPTCY_SETTLEMENT',
}
EVENT_DISCRIMINATORS = {
    bytes([166, 160, 249, 154, 183, 39, 23, 242]): 'CLASSIC',
    bytes([40, 131, 224, 220, 151, 83, 24, 230]): 'RECEIVERSHIP',
    bytes([161, 8, 108, 204, 209, 198, 12, 30]): 'FORCED_DELEVERAGE',
    bytes([166, 77, 41, 140, 36, 94, 10, 57]): 'BANKRUPTCY_SETTLEMENT',
}
DISC = {hashlib.sha256(('global:' + s).encode()).digest()[:8]: typ
        for s, typ in INSTRUCTION_TYPES.items()}


def base58_decode(text):
    number = 0
    for char in text:
        number = number * 58 + ALPHABET.index(char)
    payload = number.to_bytes((number.bit_length() + 7) // 8, 'big') if number else b''
    return b'\0' * (len(text) - len(text.lstrip('1'))) + payload


class RPC:
    def __init__(self, url, rps, timeout):
        self.url, self.interval, self.timeout, self.last = url, 1.0 / rps, timeout, 0.0
        self.calls = 0

    def call(self, method, params):
        for retry in range(6):
            delay = self.last + self.interval - time.monotonic()
            if delay > 0:
                time.sleep(delay)
            self.last = time.monotonic()
            payload = json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': method, 'params': params}).encode()
            req = urllib.request.Request(self.url, payload,
                                         {'Content-Type': 'application/json'}, method='POST')
            try:
                with urllib.request.urlopen(req, timeout=self.timeout) as response:
                    body = json.load(response)
                if 'error' in body:
                    err = str(body['error'])
                    if any(s in err.lower() for s in ('429', 'rate limit', 'too many', '-32005')):
                        raise RuntimeError('RPC_RATE_LIMIT')
                    raise RuntimeError('RPC_' + method + '_ERROR: ' + err[:160])
                self.calls += 1
                return body['result']
            except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError, RuntimeError) as exc:
                code = getattr(exc, 'code', None)
                transient = code in (429, 500, 502, 503, 504) or isinstance(exc, (urllib.error.URLError, TimeoutError)) or str(exc) == 'RPC_RATE_LIMIT'
                if retry == 5 or not transient:
                    raise RuntimeError(f'{method}: RPC request failed (HTTP {code or "transport"}); no proof obtained') from None
                time.sleep(min(2 ** retry, 16))
        raise AssertionError('unreachable')


def resolve_keys(tx):
    message = tx['transaction']['message']
    statics = message.get('accountKeys', [])
    keys = [x if isinstance(x, str) else x.get('pubkey', '') for x in statics]
    for kind in ('writable', 'readonly'):
        keys.extend(tx.get('meta', {}).get('loadedAddresses', {}).get(kind, []) or [])
    return keys


def iter_instructions(tx, keys):
    message = tx['transaction']['message']
    outer = message.get('instructions', [])
    groups = tx.get('meta', {}).get('innerInstructions') or []
    by_index = {g['index']: g.get('instructions', []) for g in groups}
    for index, ix in enumerate(outer):
        yield ix, keys, 'outer'
        for inner in by_index.get(index, []):
            yield inner, keys, 'inner'


def detect(tx, slot):
    keys = resolve_keys(tx)
    signature = tx['transaction'].get('signatures', ['UNKNOWN'])[0]
    relevant = False
    instruction_counts = collections.Counter()
    undecodable = 0
    for ix, _keys, scope in iter_instructions(tx, keys):
        idx = ix.get('programIdIndex')
        prog = keys[idx] if isinstance(idx, int) and idx < len(keys) else ix.get('programId')
        if prog != PROGRAM:
            continue
        relevant = True
        try:
            data = base58_decode(ix['data'])
            if len(data) < 8:
                raise ValueError('short discriminator')
            name = DISC.get(data[:8])
            if name:
                instruction_counts[name] += 1
        except (KeyError, ValueError, TypeError):
            undecodable += 1
    if not relevant:
        return None
    success = tx.get('meta', {}).get('err') is None
    event_counts = collections.Counter()
    malformed_program_data = 0
    # Require Marginfi's own execution frame to avoid foreign program log spoofing.
    frames = []
    for line in tx.get('meta', {}).get('logMessages') or []:
        if line.startswith('Program '):
            parts = line.split(' ')
            if len(parts) >= 3 and parts[2] == 'invoke':
                frames.append(parts[1])
            elif len(parts) >= 3 and (parts[2] == 'success' or parts[2].startswith('failed')):
                if frames and frames[-1] == parts[1]:
                    frames.pop()
        if line.startswith('Program data: ') and frames and frames[-1] == PROGRAM:
            try:
                raw = base64.b64decode(line[14:].strip(), validate=True)
                kind = EVENT_DISCRIMINATORS.get(raw[:8])
                if kind and success:
                    event_counts[kind] += 1
            except Exception:
                pass
    return {'slot': slot, 'signature': signature, 'success': success,
            'instructions': dict(instruction_counts),
            'program_events': dict(event_counts),
            'undecodable_instructions': undecodable,
            'malformed_program_data': malformed_program_data}



def quick_main():
    ap = argparse.ArgumentParser(description="Read-only independent Marginfi signature audit")
    ap.add_argument("run_dir", nargs="?", type=Path,
                    default=Path("project_0/data/econ4h/market10m_20261011T003528Z"))
    ap.add_argument("--rpc-url", default=(os.environ.get("P0_AUDIT_RPC_URL")
        or os.environ.get("P0_RPC_URL")
        or ("https://mainnet.helius-rpc.com/?api-key=" + os.environ["HELIUS_API_KEY"]
            if os.environ.get("HELIUS_API_KEY") else None)))
    ap.add_argument("--rps", type=float, default=2.0)
    ap.add_argument("--max-pages", type=int, default=50)
    ap.add_argument("--timeout", type=int, default=30)
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        # Checks positive events, failed transactions, foreign-program emission.
        test_tx = {"transaction":{"signatures":["TEST"],"message":{"accountKeys":[PROGRAM],
            "instructions":[{"programIdIndex":0,"data":"1","accounts":[]}]}},
            "meta":{"err":None,"loadedAddresses":{"writable":[],"readonly":[]},
                "innerInstructions":[],"logMessages":[]}}
        for discriminator, kind in EVENT_DISCRIMINATORS.items():
            payload = base64.b64encode(discriminator + bytes(80)).decode()
            test_tx["meta"]["logMessages"] = [
                "Program "+PROGRAM+" invoke [1]", "Program data: "+payload,
                "Program "+PROGRAM+" success"]
            assert detect(test_tx, 1)["program_events"].get(kind) == 1, kind
            test_tx["meta"]["err"] = {"InstructionError":[0,"Custom"]}
            assert detect(test_tx, 1)["program_events"].get(kind, 0) == 0, kind
            test_tx["meta"]["err"] = None
            test_tx["meta"]["logMessages"] = [
                "Program 11111111111111111111111111111111 invoke [1]",
                "Program data: "+payload,
                "Program 11111111111111111111111111111111 success"]
            assert detect(test_tx, 1)["program_events"].get(kind, 0) == 0, kind
        print("SELF_TEST=PASS all-four events, failed and foreign program excluded")
        return 0
    if not args.rpc_url:
        ap.error("Set P0_AUDIT_RPC_URL or P0_RPC_URL/HELIUS_API_KEY (no wallet needed)")
    if not (0 < args.rps <= 10 and 1 <= args.max_pages <= 500):
        ap.error("--rps must be 0..10 and --max-pages 1..500")
    report_path = args.run_dir/"quick_timebox_report.json"
    ledger_path = args.run_dir/"market"/"transactions.jsonl"
    if not report_path.is_file() or not ledger_path.is_file():
        ap.error("Quick report or market/transactions.jsonl missing: "+str(args.run_dir))
    quick = json.loads(report_path.read_text())
    lo, hi = quick.get("observed_slot_min"), quick.get("observed_slot_max")
    if not isinstance(lo, int) or not isinstance(hi, int) or lo <= 0 or hi < lo:
        ap.error("Missing or invalid captured slot boundaries")
    saved = []
    with ledger_path.open(encoding="utf-8") as f:
        for line_no, raw in enumerate(f, 1):
            try:
                rec = json.loads(raw)
            except json.JSONDecodeError:
                print("WARNING: possible truncated final JSONL row",line_no)
                break
            if rec.get("signature") and isinstance(rec.get("slot"), int):
                saved.append(rec)
    bot_signatures = {row["signature"] for row in saved}
    if len(bot_signatures) != len(saved):
        raise SystemExit("FAIL: repeated signatures in saved bot ledger")
    print("AUDIT_SLOTS=",lo,hi,"BOT_SAVED_TX=",len(saved),
          "BOT_LAST_LIVE_TX=",quick.get("market_live_last_observed",{}).get("tx"), flush=True)
    rpc = RPC(args.rpc_url, args.rps, args.timeout)
    indexed, before, crossed = {}, None, False
    for page in range(args.max_pages):
        options = {"commitment":"finalized","limit":1000}
        if before:
            options["before"] = before
        batch = rpc.call("getSignaturesForAddress", [PROGRAM, options])
        if not isinstance(batch, list):
            raise SystemExit("FAIL: RPC signature batch not list")
        prev_slot = None
        for row in batch:
            slot, signature = row.get("slot"), row.get("signature")
            if not isinstance(slot, int) or not isinstance(signature, str):
                raise SystemExit("FAIL: invalid signature-index item")
            if prev_slot is not None and slot > prev_slot:
                raise SystemExit("FAIL: reverse slot order violated")
            prev_slot = slot
            if lo <= slot <= hi:
                indexed[signature] = slot
        print("INDEX_PAGE=",page+1,"rows=",len(batch),"found_in_window=",len(indexed),
              "oldest_slot=",batch[-1].get("slot") if batch else None, flush=True)
        if batch and batch[-1]["slot"] < lo:
            crossed = True
            break
        if not batch or len(batch)<1000:
            break
        before = batch[-1]["signature"]
    if not crossed:
        raise SystemExit("UNVERIFIED: index pagination did not cross window start. "
                         "Increase --max-pages or use archival RPC. Not a zero-liquidation result.")
    external = []
    for n,(signature,slot) in enumerate(sorted(indexed.items(),key=lambda kv:kv[1]),1):
        tx = rpc.call("getTransaction", [signature, {"encoding":"json",
                       "commitment":"finalized","maxSupportedTransactionVersion":1}])
        if not isinstance(tx,dict):
            raise SystemExit("UNVERIFIED: finalized getTransaction not available for "+signature)
        event = detect(tx,slot)
        if event is None or event["signature"]!=signature:
            raise SystemExit("UNVERIFIED: program/key/signature mismatch for "+signature)
        external.append(event)
        if n%10==0 or n==len(indexed):
            print("FETCHED=",n,"/",len(indexed),flush=True)
    instruction_counts, event_counts = collections.Counter(), collections.Counter()
    liquidations = []
    for tx in external:
        if tx["success"]:
            instruction_counts.update(tx["instructions"])
            event_counts.update(tx["program_events"])
            if tx["instructions"] or tx["program_events"]:
                liquidations.append(tx)
    missing_in_bot = [tx for tx in external if tx["signature"] not in bot_signatures]
    # First and last slots are partial observations; interior is stronger evidence.
    missing_interior = [tx for tx in missing_in_bot if lo < tx["slot"] < hi]
    external_signatures = {tx["signature"] for tx in external}
    bot_only = [tx for tx in saved if tx["signature"] not in external_signatures]
    last = quick.get("market_live_last_observed",{})
    mapping = {"CLASSIC":"provisional_classic",
               "RECEIVERSHIP":"provisional_receivership",
               "FORCED_DELEVERAGE":"provisional_deleverage",
               "BANKRUPTCY_SETTLEMENT":"provisional_bankruptcy"}
    discrepancies = []
    for kind,field in mapping.items():
        if field in last and int(last[field]) < event_counts[kind]:
            discrepancies.append("BOT_EVENT_UNDERCOUNT_"+kind)
    if missing_interior:
        discrepancies.append("MISSING_BOT_SIGNATURES_INTERIOR")
    summary = {
        "method":"INDEPENDENT_FINALIZED_ADDRESS_INDEX",
        "range":{"lo":lo,"hi":hi,"partial_boundary_slots":True},
        "scope_warning":"Address-index audit, NOT full block reconciliation",
        "rpc_finalized_tx":len(external),
        "bot_saved_tx":len(saved),
        "bot_live_last_count":last.get("tx"),
        "rpc_event_counts":dict(event_counts),
        "rpc_liquidation_instruction_counts":dict(instruction_counts),
        "liquidations":liquidations,
        "missing_bot":missing_in_bot,
        "missing_bot_interior":missing_interior,
        "bot_only":bot_only,
        "discrepancies":discrepancies,
        "rpc_calls":rpc.calls,
        "qualification":"DISCREPANCY" if discrepancies else "INDEX_SCOPE_MATCH_ONLY",
        "note":"Zero in both does NOT prove whole-chain absence; full-slot proof separately required."
    }
    output = args.run_dir/"market"/"quick_independent_crosscheck.json"
    output.write_text(json.dumps(summary,indent=2)+"\n",encoding="utf-8")
    print("\n=== INDEPENDENT MARGINFI CROSSCHECK ===")
    print("RPC_FINALIZED_TX=",len(external),"BOT_SAVED_TX=",len(saved),
          "BOT_LIVE_TX=",last.get("tx"))
    print("RPC_LIQUIDATION_INSTRUCTIONS=",dict(instruction_counts))
    print("RPC_LIQUIDATION_EVENTS=",dict(event_counts))
    print("MISSING_BOT_INTERIOR=",len(missing_interior),"BOT_ONLY=",len(bot_only))
    for tx in liquidations:
        print("LIQUIDATION:",tx["slot"],tx["signature"],
              "instructions=",tx["instructions"],"events=",tx["program_events"],
              "https://solscan.io/tx/"+tx["signature"])
    for tx in missing_interior[:20]:
        print("MISSING_BOT:",tx["slot"],tx["signature"])
    print("RESULT=","DISCREPANCY" if discrepancies else "INDEX_SCOPE_MATCH_ONLY",
          "EVIDENCE=",output)
    return 1 if discrepancies else 0

if __name__ == "__main__":
    sys.exit(quick_main())

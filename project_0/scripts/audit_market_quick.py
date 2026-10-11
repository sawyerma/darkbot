#!/usr/bin/env python3
"""Independent read-only Solana finalized signature-index audit of a Project_0 capture.

Does not import or call the DarkBot market decoder. Counts actual program
instructions and program-emitted event discriminators from finalized getTransaction payloads. No full-block download.
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
import sqlite3
import struct
import math
import re
import datetime as dt
import decimal
import random
import fcntl
import resource
import email.utils
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

# Layout authority: marginfi-v2 33c67987a661f59c6683a65485ebb4f0ef030b77,
# programs/marginfi/src/events.rs. Independent Python interpretation, never calls bot decoder.
SCHEMA = 'C0-E1/INDEX/1'
NS = 10**9
KINDS = ('CLASSIC', 'RECEIVERSHIP', 'FORCED_DELEVERAGE', 'BANKRUPTCY_SETTLEMENT')
COMPLETION = {'CLASSIC': 'CLASSIC', 'RECEIVERSHIP_END': 'RECEIVERSHIP',
              'FORCED_DELEVERAGE': 'FORCED_DELEVERAGE', 'BANKRUPTCY_SETTLEMENT': 'BANKRUPTCY_SETTLEMENT'}


def base58_encode(raw):
    n, out = int.from_bytes(raw, 'big'), ''
    while n:
        n, rem = divmod(n, 58)
        out = ALPHABET[rem] + out
    return '1' * (len(raw) - len(raw.lstrip(b'\0'))) + out


class AuditError(Exception):
    """Only static error codes cross the CLI boundary; never include provider text."""


class RPC:
    def __init__(self, url, rps=2, timeout=15, attempts=3, max_calls=2000, seconds=300,
                 max_bytes=256*1024*1024, opener=None, sleep=time.sleep, clock=time.monotonic):
        self.url, self.interval, self.timeout = url, 1/rps, timeout
        self.attempts, self.max_calls, self.max_bytes = attempts, max_calls, max_bytes
        self.opener, self.sleep, self.clock = opener or urllib.request.urlopen, sleep, clock
        self.deadline, self.last = clock()+seconds, -1e30
        self.calls, self.bytes_received, self.bytes_sent = 0, 0, 0
        self.methods = collections.Counter()

    def pause(self, seconds):
        if self.clock()+seconds >= self.deadline:
            raise AuditError('RPC_TIME_BUDGET')
        self.sleep(seconds)

    def call(self, method, params):
        if method not in ('getSignaturesForAddress', 'getTransaction'):
            raise AuditError('INDEX_METHOD_FORBIDDEN')
        for attempt in range(self.attempts):
            if self.calls >= self.max_calls:
                raise AuditError('RPC_CALL_BUDGET')
            if self.bytes_received >= self.max_bytes:
                raise AuditError('RPC_BYTE_BUDGET')
            self.pause(max(0, self.last+self.interval-self.clock()))
            self.last = self.clock()
            data = json.dumps({'jsonrpc':'2.0','id':1,'method':method,'params':params}).encode()
            req = urllib.request.Request(self.url, data, {'Content-Type':'application/json'}, method='POST')
            retry_after = None
            self.calls += 1
            self.methods[method] += 1
            self.bytes_sent += len(data)
            try:
                with self.opener(req, timeout=min(self.timeout, self.deadline-self.clock())) as response:
                    limit = min(16*1024*1024, self.max_bytes-self.bytes_received)
                    raw = response.read(limit+1)
                    self.bytes_received += len(raw)
                    if len(raw) > limit:
                        raise AuditError('RPC_RESPONSE_OR_BYTE_BUDGET')
                    body = json.loads(raw)
                if not isinstance(body, dict):
                    raise AuditError('RPC_MALFORMED_RESPONSE')
                if 'error' in body:
                    error = body['error']
                    code = error.get('code') if isinstance(error,dict) else None
                    # Inspect text only to categorize; never persist/print it.
                    text = str(error).lower()
                    if code in (429,-32005) or any(x in text for x in ('429','rate limit','too many')):
                        reason = 'RPC_RATE_LIMIT'
                    elif code == -32015:
                        raise AuditError('RPC_UNSUPPORTED_TX_VERSION')
                    else:
                        raise AuditError('RPC_PROVIDER_ERROR')
                elif 'result' not in body:
                    raise AuditError('RPC_MALFORMED_RESPONSE')
                else:
                    return body['result']
            except urllib.error.HTTPError as exc:
                reason = 'RPC_RATE_LIMIT' if exc.code == 429 else 'RPC_HTTP_ERROR'
                if exc.code not in (429,500,502,503,504):
                    raise AuditError(reason) from None
                header = exc.headers.get('Retry-After') if exc.headers else None
                if header:
                    try:
                        retry_after = float(header)
                    except ValueError:
                        try:
                            retry_after = max(0,email.utils.parsedate_to_datetime(header).timestamp()-time.time())
                        except (ValueError,TypeError,OverflowError):
                            raise AuditError('RPC_INVALID_RETRY_AFTER') from None
                    if not math.isfinite(retry_after) or retry_after < 0:
                        raise AuditError('RPC_INVALID_RETRY_AFTER')
                exc.close()
            except (urllib.error.URLError, TimeoutError, OSError):
                reason = 'RPC_TRANSPORT'
            except (ValueError,KeyError,TypeError):
                raise AuditError('RPC_MALFORMED_RESPONSE') from None
            if attempt+1 == self.attempts:
                raise AuditError(reason)
            self.pause(max(retry_after or 0, min(2**attempt,8)+random.random()/4))
        raise AssertionError('unreachable')


class Cursor:
    def __init__(self, raw):
        self.raw, self.pos = raw, 8

    def take(self, size):
        end = self.pos+size
        if end > len(self.raw):
            raise ValueError('EVENT_LENGTH')
        value, self.pos = self.raw[self.pos:end], end
        return value

    def key(self):
        return base58_encode(self.take(32))

    def number(self):
        value = struct.unpack('<d', self.take(8))[0]
        if not math.isfinite(value):
            raise ValueError('EVENT_NONFINITE')
        return value

    def header(self):
        option = self.take(1)[0]
        if option not in (0,1):
            raise ValueError('EVENT_OPTION_TAG')
        signer = self.key() if option else None
        account = self.key()
        self.key(); self.key()  # authority/group retained in raw payload
        return signer, account


def decode_payload(raw):
    kind = EVENT_DISCRIMINATORS.get(raw[:8])
    if kind is None:
        if len(raw) < 8:
            raise ValueError('EVENT_DISCRIMINATOR_MISSING')
        return None
    c = Cursor(raw)
    result = {'type':kind,'seized_value_usd':None,'repaid_value_usd':None,
              'protocol_fee_lamports':None,'native_evidence':None}
    if kind in ('RECEIVERSHIP','FORCED_DELEVERAGE'):
        result.update(target=c.key(),actor=c.key(),seized_value_usd=c.number(),repaid_value_usd=c.number())
        if kind == 'RECEIVERSHIP':
            result['protocol_fee_lamports'] = int.from_bytes(c.take(4),'little')
    elif kind == 'CLASSIC':
        signer, account = c.header()
        if signer is None:
            raise ValueError('EVENT_SIGNER_MISSING')
        result.update(actor=signer,liquidator_account=account,target=c.key())
        c.key()
        result.update(asset_bank=c.key(),asset_mint=c.key(),liability_bank=c.key(),liability_mint=c.key())
        health = [c.number(),c.number()]
        pre, post = [c.number() for _ in range(5)], [c.number() for _ in range(5)]
        result['native_evidence'] = {'pre_balances':pre,'post_balances':post,'health':health,
                                    'provenance':'event f64 native balances; no historical prices imputed'}
    else:
        signer, account = c.header()
        if signer is None:
            raise ValueError('EVENT_SIGNER_MISSING')
        result.update(actor=signer,target=account,liability_bank=c.key(),liability_mint=c.key())
        result['native_evidence'] = dict(zip(('bad_debt','covered_amount','socialized_amount'),[c.number() for _ in range(3)]))
    if c.pos != len(raw):
        raise ValueError('EVENT_LAYOUT_TRAILING_BYTES')
    result['payload_sha256'] = hashlib.sha256(raw).hexdigest()
    return result


def resolve_keys(tx):
    message = tx['transaction']['message']
    statics = message['accountKeys']
    keys = [x if isinstance(x,str) else x['pubkey'] for x in statics]
    loaded = tx['meta'].get('loadedAddresses')
    if message.get('addressTableLookups') and not isinstance(loaded,dict):
        raise ValueError('LUT_ADDRESSES_MISSING')
    # encoding=json is required. jsonParsed keys already containing lookups are rejected.
    if any(isinstance(x,dict) and x.get('source') == 'lookupTable' for x in statics):
        raise ValueError('JSON_PARSED_KEYS_UNSUPPORTED')
    for kind in ('writable','readonly'):
        keys.extend((loaded or {}).get(kind,[]) or [])
    return keys


def iter_instructions(tx, keys):
    outer = tx['transaction']['message']['instructions']
    groups = tx['meta'].get('innerInstructions') or []
    by_index = {}
    for g in groups:
        if g['index'] in by_index or not 0 <= g['index'] < len(outer):
            raise ValueError('INNER_INSTRUCTION_INDEX')
        by_index[g['index']] = g['instructions']
    for index, ix in enumerate(outer):
        yield ix, keys, (index,None)
        for inner_index, ix in enumerate(by_index.get(index,[])):
            yield ix, keys, (index,inner_index)


def program_of(ix, keys):
    idx = ix.get('programIdIndex')
    if idx is None:
        return ix.get('programId')
    if type(idx) is not int or not 0 <= idx < len(keys):
        raise ValueError('PROGRAM_ID_INDEX')
    return keys[idx]


def decode_logs(logs, instructions=None):
    events, errors, stack = [], [], []
    cursor = 0
    for log_index, line in enumerate(logs):
        if not isinstance(line,str):
            errors.append('LOG_LAYOUT');continue
        if 'Log truncated' in line:
            errors.append('LOGS_TRUNCATED')
        invoke = re.fullmatch(r'Program (\w+) invoke \[(\d+)\]',line)
        end = re.fullmatch(r'Program (\w+) (success|failed:.*)',line)
        if invoke:
            program, depth = invoke[1], int(invoke[2])
            if depth != len(stack)+1:
                errors.append('LOG_FRAME_DEPTH')
            ix = None
            if program == PROGRAM and instructions is not None:
                if cursor >= len(instructions):
                    errors.append('INSTRUCTION_LOG_FRAME_MISMATCH')
                else:
                    ix = instructions[cursor]
                    cursor += 1
            stack.append((program,ix))
        elif end:
            if not stack or stack[-1][0] != end[1]:
                errors.append('LOG_FRAME_END')
            elif end[2] != 'success':
                # A caught failed CPI must not contribute rolled-back event effects.
                _, failed_ix = stack.pop()
                if end[1] == PROGRAM:
                    for event in events:
                        if event.get('instruction_position') == (failed_ix or {}).get('position'):
                            event['rolled_back'] = True
            else:
                stack.pop()
        elif line.startswith('Program data: ') and stack and stack[-1][0] == PROGRAM:
            try:
                ev = decode_payload(base64.b64decode(line[14:].strip(),validate=True))
                if ev:
                    ix = stack[-1][1]
                    ev.update(log_index=log_index,event_index=len(events),
                              instruction_position=ix['position'] if ix else None)
                    if ix and COMPLETION.get(ix.get('kind')) != ev['type']:
                        errors.append('INSTRUCTION_EVENT_TYPE_MISMATCH')
                    events.append(ev)
            except (ValueError,TypeError,struct.error):
                errors.append('MALFORMED_PROGRAM_EVENT')
    if stack:
        errors.append('LOG_FRAME_UNCLOSED')
    return [e for e in events if not e.get('rolled_back')], errors


def detect(tx, slot):
    """Preserve original discriminator/frame audit, adding strict payload and identity evidence."""
    errors, instructions, counts = [], [], collections.Counter()
    signature = (tx.get('transaction') or {}).get('signatures',[None])[0]
    result = {'slot':slot,'signature':signature,'success':None,'instructions':{},'program_events':{},
              'events':[],'unknown_reasons':errors,'classification':'UNKNOWN'}
    try:
        meta = tx['meta']
        if not isinstance(meta,dict) or 'err' not in meta:
            raise ValueError('META_MISSING')
        result['success'] = meta['err'] is None
        result['fee_lamports'] = meta.get('fee')
        result['version'] = tx.get('version','legacy')
        if result['version'] not in ('legacy',0,1):
            raise ValueError('UNSUPPORTED_TX_VERSION')
        keys = resolve_keys(tx)
        result['fee_payer'] = keys[0] if keys else None
        result['lamport_balances'] = {'pre':meta.get('preBalances'),'post':meta.get('postBalances')}
        result['token_balances'] = {'pre':meta.get('preTokenBalances'),'post':meta.get('postTokenBalances')}
        for ix, _, position in iter_instructions(tx,keys):
            if program_of(ix,keys) != PROGRAM:
                continue
            raw = base58_decode(ix['data'])
            if len(raw) < 8:
                raise ValueError('SHORT_INSTRUCTION')
            kind = DISC.get(raw[:8])
            item = {'position':position,'kind':kind,'discriminator':raw[:8].hex()}
            if kind:
                counts[kind] += 1
                if kind == 'CLASSIC':
                    if len(raw) != 18:
                        raise ValueError('CLASSIC_INSTRUCTION_LAYOUT')
                    item['asset_amount_native_exact'] = str(int.from_bytes(raw[8:16],'little'))
            instructions.append(item)
        result['instructions'] = dict(counts)
        result['instruction_evidence'] = instructions
        if not result['success']:
            result['classification'] = 'FAILED_TRANSACTION'
            return result
        logs = meta.get('logMessages')
        if not isinstance(logs,list):
            raise ValueError('LOGS_MISSING')
        events, log_errors = decode_logs(logs,instructions)
        errors.extend(log_errors)
        event_counts = collections.Counter(e['type'] for e in events)
        for kind, expected in COMPLETION.items():
            if counts[kind] != event_counts[expected]:
                errors.append('COMPLETION_EVENT_COUNT_MISMATCH_'+expected)
        if instructions and not logs:
            errors.append('LOGS_MISSING')
        for event in events:
            event.update(signature=signature,slot=slot,transaction_index=tx.get('transactionIndex'),
                         source='INDEPENDENT_FINALIZED_GET_TRANSACTION',
                         solscan='https://solscan.io/tx/'+str(signature))
        result.update(events=events,program_events=dict(event_counts))
        result['classification'] = 'UNKNOWN' if errors else 'CLASSIFIED'
    except (ValueError,KeyError,IndexError,TypeError,struct.error):
        errors.append('TRANSACTION_LAYOUT_OR_PAYLOAD_UNKNOWN')
    result['undecodable_instructions'] = len(errors)
    return result


def utc_ns(text):
    # datetime truncates beyond microseconds; preserve P1's exact nanoseconds.
    match = re.fullmatch(r'(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)(?:\.(\d{1,9}))?Z',text or '')
    if not match:
        raise AuditError('UTC_FORMAT')
    try:
        seconds = int(dt.datetime.fromisoformat(match[1]+'+00:00').timestamp())
    except ValueError:
        raise AuditError('UTC_FORMAT') from None
    return seconds*NS+int((match[2] or '').ljust(9,'0') or '0')


def iso_ns(value):
    return dt.datetime.fromtimestamp(value//NS,dt.timezone.utc).strftime('%Y-%m-%dT%H:%M:%S')+f'.{value%NS:09d}Z'


def rows(path, offset=0):
    """Bounded raw JSONL; original files are never written. Offset is a byte cursor."""
    with path.open('rb') as f:
        f.seek(offset)
        while True:
            raw = f.readline(16*1024*1024+1)
            if not raw:
                break
            if len(raw) > 16*1024*1024 or not raw.endswith(b'\n'):
                raise AuditError('LOCAL_JSONL_TRUNCATED_OR_OVERSIZE')
            try:
                row = json.loads(raw)
            except (ValueError,UnicodeError):
                raise AuditError('LOCAL_JSONL_INVALID') from None
            yield row,f.tell()


def atomic_json(path, value):
    tmp = path.with_suffix(path.suffix+'.tmp')
    with tmp.open('w') as f:
        json.dump(value,f,indent=2,allow_nan=False);f.write('\n');f.flush();os.fsync(f.fileno())
    os.replace(tmp,path)


def read_window(run):
    manifest = run/'measurement_manifest.json'
    summary_path = run/'market/summary.json'
    if not summary_path.exists():
        summary_path = run/'summary.json'
    sources = {}
    summary = json.loads(summary_path.read_text()) if summary_path.exists() else {}
    if manifest.exists():
        m = json.loads(manifest.read_text())
        if m.get('schema') != 'C0-E1/MeasurementManifest/1':
            raise AuditError('MANIFEST_SCHEMA_UNKNOWN')
        captures = [r['details'] for r,_ in rows(run/'measurement_phases.jsonl') if r.get('phase') == 'CAPTURE']
        if len(captures) != 1:
            raise AuditError('CAPTURE_WINDOW_NOT_UNIQUE')
        capture = captures[0]
        start, end = capture['capture_start_unix_ns'], capture['capture_cutoff_unix_ns']
        if end-start != m['plan']['duration_seconds']*NS:
            raise AuditError('PLAN_WINDOW_MISMATCH')
        sources['measurement_manifest_sha256'] = hashlib.sha256(manifest.read_bytes()).hexdigest()
        provenance = 'P1_MANIFEST_AND_CAPTURE_PHASE'
    else:
        start = utc_ns(summary.get('observation_started_at'))
        actual_end = utc_ns(summary.get('observation_ended_at'))
        duration = summary.get('configured_duration_seconds')
        if not isinstance(duration,(int,float)) or not math.isfinite(duration) or duration <= 0:
            raise AuditError('LEGACY_REQUESTED_DURATION_MISSING')
        end = start+int(decimal.Decimal(str(duration))*NS)
        if actual_end > end+NS:
            raise AuditError('LEGACY_TIME_CONFLICT')
        provenance = 'LEGACY_START_PLUS_REQUESTED_DURATION; actual end retained separately'
    if type(start) is not int or type(end) is not int or not 0 < end-start <= 86400*NS:
        raise AuditError('INVALID_WINDOW')
    if summary_path.exists():
        sources['summary_sha256'] = hashlib.sha256(summary_path.read_bytes()).hexdigest()
    return {'start_ns':start,'end_ns':end,'start_utc':iso_ns(start),'end_utc':iso_ns(end),
            'actual_observation_ended_at':summary.get('observation_ended_at'),
            'provenance':provenance,'blocktime_semantics':'integer chain blockTime in [start,end); not local T_rx; boundary seconds uncertain',
            'source_identity':sources}


class Audit:
    def __init__(self, run, rpc, max_pages=50, seconds=300, clock=time.monotonic):
        self.run, self.rpc, self.max_pages, self.clock = Path(run), rpc, max_pages, clock
        self.deadline = clock()+seconds
        self.window = read_window(self.run)
        self.out = self.run/'verification/index_v1'
        self.out.mkdir(parents=True,exist_ok=True)
        self.lock = (self.out/'writer.lock').open('a')
        try:
            fcntl.flock(self.lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:
            self.lock.close();raise AuditError('VERIFIER_ALREADY_RUNNING') from None
        self.db = sqlite3.connect(self.out/'checkpoint.sqlite3')
        self.db.execute('PRAGMA journal_mode=WAL')
        self.db.execute('PRAGMA cache_size=-4096')
        self.db.execute('PRAGMA temp_store=FILE')
        self.db.executescript('''
          CREATE TABLE IF NOT EXISTS state(key TEXT PRIMARY KEY,value TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS signatures(sig TEXT PRIMARY KEY,slot INTEGER,bt INTEGER,err TEXT,status TEXT DEFAULT 'pending',detail TEXT);
          CREATE TABLE IF NOT EXISTS bot_tx(sig TEXT,slot INTEGER,source TEXT,PRIMARY KEY(sig,slot,source));
          CREATE TABLE IF NOT EXISTS bot_events(sig TEXT,slot INTEGER,source TEXT,idx INTEGER,kind TEXT,data TEXT,PRIMARY KEY(sig,slot,source,idx,kind));
          CREATE TABLE IF NOT EXISTS gaps(start_ns INTEGER,end_ns INTEGER,source TEXT);
          CREATE TABLE IF NOT EXISTS input_cursor(path TEXT PRIMARY KEY,size INTEGER,mtime INTEGER,offset INTEGER);
          CREATE TABLE IF NOT EXISTS shadow(sig TEXT,slot INTEGER,target TEXT,data TEXT,PRIMARY KEY(sig,slot,target));
        ''')
        identity = {'schema':SCHEMA,'run_id':self.run.name,'window':self.window,
                    'decoder_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
        previous = self.get('identity')
        if previous is not None and previous != identity:
            self.close();raise AuditError('RESUME_IDENTITY_MISMATCH')
        self.set('identity',identity);self.db.commit()
        for name in ('pages','transactions','attempts'):
            (self.out/name).mkdir(exist_ok=True)

    def close(self):
        self.db.close();fcntl.flock(self.lock,fcntl.LOCK_UN);self.lock.close()

    def get(self,key,default=None):
        r=self.db.execute('SELECT value FROM state WHERE key=?',(key,)).fetchone()
        return json.loads(r[0]) if r else default

    def set(self,key,value):
        self.db.execute('INSERT OR REPLACE INTO state VALUES(?,?)',(key,json.dumps(value,allow_nan=False)))

    def budget(self):
        if self.clock() >= self.deadline:
            raise AuditError('VERIFICATION_TIME_BUDGET')

    def raw(self,kind,key,fetch):
        path=self.out/kind/(hashlib.sha256(key.encode()).hexdigest()+'.json')
        if path.exists():
            return json.loads(path.read_text())
        value=fetch()
        if value is not None:  # null/pruned responses must be retryable on the next invocation.
            atomic_json(path,value)
        return value

    def paginate(self):
        if self.get('crossed_start',False):
            return
        before=self.get('before')
        previous_slot=self.get('last_slot')
        for _ in range(self.max_pages):
            self.budget()
            options={'commitment':'finalized','limit':1000}
            if before:
                options['before']=before
            batch=self.raw('pages',before or 'HEAD',lambda:self.rpc.call('getSignaturesForAddress',[PROGRAM,options]))
            if not isinstance(batch,list):
                raise AuditError('INDEX_PAGE_LAYOUT')
            if not batch:
                raise AuditError('INDEX_HISTORY_EXHAUSTED_BEFORE_START')
            crossed=False
            with self.db:
                for row in batch:
                    slot,sig,bt=row.get('slot'),row.get('signature'),row.get('blockTime')
                    if type(slot) is not int or slot < 0 or not isinstance(sig,str) or not sig or 'err' not in row:
                        raise AuditError('INDEX_ROW_LAYOUT')
                    if previous_slot is not None and slot>previous_slot:
                        raise AuditError('INDEX_SLOT_ORDER')
                    previous_slot=slot
                    if bt is not None and type(bt) is not int:
                        raise AuditError('BLOCKTIME_LAYOUT')
                    old=self.db.execute('SELECT slot,bt,err FROM signatures WHERE sig=?',(sig,)).fetchone()
                    values=(slot,bt,json.dumps(row['err'],sort_keys=True))
                    if old and old != values:
                        raise AuditError('INDEX_DUPLICATE_CONFLICT')
                    self.db.execute('INSERT OR IGNORE INTO signatures(sig,slot,bt,err) VALUES(?,?,?,?)',(sig,*values))
                    if bt is not None and bt*NS<self.window['start_ns']:
                        crossed=True
                next_before=batch[-1]['signature']
                if next_before==before:
                    raise AuditError('INDEX_CURSOR_STALLED')
                self.set('before',next_before);self.set('last_slot',previous_slot)
                self.set('pages',self.get('pages',0)+1)
                # Only stop after a full page is persisted. Null times are resolved separately.
                self.set('crossed_start',crossed)
            before=next_before
            if crossed:
                return
        raise AuditError('INDEX_PAGE_BUDGET')

    def fetch_transactions(self):
        # Paginated rows on disk; failed + successful are fetched, no liquidation pre-filter.
        cursor=self.db.execute("SELECT sig,slot,bt,err FROM signatures WHERE status != 'done'")
        for sig,slot,bt,err in cursor:
            self.budget()
            if bt is not None and not self.window['start_ns'] <= bt*NS < self.window['end_ns']:
                # Boundary seconds remain fetched (coarse chain time vs precise local time).
                if bt not in (self.window['start_ns']//NS,self.window['end_ns']//NS):
                    with self.db:
                        self.db.execute("UPDATE signatures SET status='done',detail=? WHERE sig=?",(json.dumps({'membership':'OUTSIDE'}),sig))
                    continue
            tx=self.raw('transactions',sig,lambda:self.rpc.call('getTransaction',[sig,{'encoding':'json','commitment':'finalized','maxSupportedTransactionVersion':1}]))
            if tx is None:
                with self.db:
                    self.db.execute("UPDATE signatures SET status='missing',detail=? WHERE sig=?",(json.dumps({'membership':'UNKNOWN','error':'TRANSACTION_UNAVAILABLE'}),sig))
                continue
            if not isinstance(tx,dict) or tx.get('slot')!=slot or (tx.get('transaction') or {}).get('signatures',[None])[0]!=sig:
                raise AuditError('TRANSACTION_IDENTITY_MISMATCH')
            tx_time=tx.get('blockTime')
            if tx_time is not None and type(tx_time) is not int:
                raise AuditError('BLOCKTIME_LAYOUT')
            if bt is not None and tx_time is not None and bt!=tx_time:
                raise AuditError('INDEX_TX_TIME_MISMATCH')
            if not isinstance(tx.get('meta'),dict) or tx['meta'].get('err','MISSING')!=json.loads(err):
                raise AuditError('INDEX_TX_STATUS_MISMATCH')
            resolved_time=tx_time if tx_time is not None else bt
            membership='UNKNOWN' if resolved_time is None else ('IN_WINDOW' if self.window['start_ns']<=resolved_time*NS<self.window['end_ns'] else 'OUTSIDE')
            result=detect(tx,slot)
            result.update(membership=membership,block_time=resolved_time,
                          boundary_second=resolved_time in (self.window['start_ns']//NS,self.window['end_ns']//NS))
            with self.db:
                self.db.execute("UPDATE signatures SET status='done',detail=? WHERE sig=?",(json.dumps(result,allow_nan=False),sig))

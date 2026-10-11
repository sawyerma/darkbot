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


# Audited pinned program surface (including generic entry points), fail closed on upgrades.
KNOWN_INSTRUCTIONS = {hashlib.sha256(("global:"+name).encode()).digest()[:8] for name in """
marginfi_group_initialize
marginfi_group_configure
lending_pool_add_bank
lending_pool_add_bank_with_seed
lending_pool_clone_bank
lending_pool_add_bank_permissionless
lending_pool_backfill_bank_is_t22_flag
lending_pool_backfill_staked_bank_validator_vote_account
disable_staked_oracles
enable_staked_oracle_onramp
lending_pool_configure_bank
lending_pool_configure_bank_interest_only
lending_pool_configure_bank_limits_only
lending_pool_force_tokenless_repay_complete
lending_pool_clear_circuit_breaker
lending_pool_configure_bank_oracle
lending_pool_configure_bank_oracle_scope
lending_pool_set_oracle_price
lending_pool_init_same_asset_emode_registry
lending_pool_set_bank_same_asset_emode_eligibility
lending_pool_configure_bank_emode
lending_pool_clone_emode
lending_pool_emissions_deposit
lending_pool_handle_bankruptcy
super_admin_withdraw
super_admin_deposit
marginfi_account_initialize
marginfi_account_init_liq_record
marginfi_account_close_liq_record
marginfi_account_initialize_pda
marginfi_account_place_order
marginfi_account_close_order
marginfi_account_keeper_close_order
marginfi_account_set_keeper_close_flags
marginfi_account_start_execute_order
marginfi_account_end_execute_order
lending_account_deposit
lending_account_repay
lending_account_withdraw
lending_account_borrow
lending_account_close_balance
lending_account_liquidate
lending_account_start_flashloan
lending_account_end_flashloan
marginfi_account_update_emissions_destination_account
lending_pool_accrue_bank_interest
lending_pool_resize_group_account
resize_global_fee_state
lending_pool_collect_bank_fees
lending_pool_withdraw_fees
lending_pool_withdraw_fees_permissionless
lending_pool_update_fees_destination_account
lending_pool_withdraw_insurance
lending_pool_close_bank
transfer_to_new_account
transfer_to_new_account_pda
marginfi_account_set_freeze
marginfi_account_close
admin_close_account
lending_account_pulse_health
sync_indexer_flags
lending_pool_pulse_bank_price_cache
init_global_fee_state
edit_global_fee_state
propagate_fee_state
config_group_fee
init_staked_settings
edit_staked_settings
propagate_staked_settings
start_liquidation
end_liquidation
start_deleverage
end_deleverage
panic_pause
panic_unpause
panic_unpause_permissionless
init_bank_metadata
write_bank_metadata
write_bank_metadata_pre_init
configure_deleverage_withdrawal_limit
update_deleverage_withdrawals
configure_bank_rate_limits
configure_group_rate_limits
update_group_rate_limiter
purge_deleverage_balance
kamino_init_obligation
kamino_deposit
kamino_withdraw
lending_pool_add_bank_kamino
kamino_harvest_reward
lending_pool_add_bank_drift
drift_init_user
drift_deposit
drift_withdraw
drift_harvest_reward
drift_claim_bad_debt
lending_pool_add_bank_solend
solend_init_obligation
solend_deposit
solend_withdraw
lending_pool_add_bank_juplend
juplend_init_position
juplend_deposit
juplend_withdraw
""".split()}
KNOWN_EVENTS = {hashlib.sha256(("event:"+name).encode()).digest()[:8] for name in """
MarginfiGroupCreateEvent
MarginfiGroupConfigureEvent
LendingPoolBankCreateEvent
LendingPoolBankConfigureEvent
LendingPoolBankConfigureOracleEvent
LendingPoolBankSetOraclePriceEvent
LendingPoolBankSetSameAssetEmodeEligibilityEvent
LendingPoolBankConfigureFrozenEvent
EditStakedSettingsEvent
LendingPoolBankAccrueInterestEvent
LendingPoolBankCollectFeesEvent
LendingPoolBankHandleBankruptcyEvent
DriftClaimBadDebtEvent
LendingPoolSuperAdminWithdrawEvent
LendingPoolSuperAdminDepositEvent
MarginfiAccountCreateEvent
LendingAccountDepositEvent
LendingAccountRepayEvent
LendingAccountBorrowEvent
LendingAccountWithdrawEvent
LendingAccountLiquidateEvent
MarginfiAccountTransferToNewAccount
MarginfiAccountFreezeEvent
MarginfiAccountPlaceOrderEvent
MarginfiAccountCloseOrderEvent
KeeperCloseOrderEvent
SetKeeperCloseFlagsEvent
AdminCloseAccountEvent
HealthPulseEvent
LiquidationReceiverEvent
DeleverageEvent
RateLimitFlowEvent
DeleverageWithdrawFlowEvent
CircuitBreakerTrippedEvent
CircuitBreakerClearedEvent
CircuitBreakerAutoBrokenEvent
""".split()}


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
        if len(raw) < 8 or raw[:8] not in KNOWN_EVENTS:
            raise ValueError('UNAUDITED_EVENT_DISCRIMINATOR')
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
            stack.append((program,ix,len(events)))
        elif end:
            if not stack or stack[-1][0] != end[1]:
                errors.append('LOG_FRAME_END')
            elif end[2] != 'success':
                # A caught failed CPI must not contribute rolled-back event effects.
                _, _, first_event = stack.pop()
                del events[first_event:]
                errors.append('CAUGHT_CPI_FAILURE_EFFECTS_UNCERTAIN')
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
                    if ix and (ix.get('target') != ev['target'] or ix.get('actor') != ev['actor']):
                        errors.append('INSTRUCTION_EVENT_ACCOUNT_MISMATCH')
                    if ix:
                        ev['seized_asset_native_exact'] = ix.get('asset_amount_native_exact')
                    events.append(ev)
            except (ValueError,TypeError,struct.error):
                errors.append('MALFORMED_PROGRAM_EVENT')
    if stack:
        errors.append('LOG_FRAME_UNCLOSED')
    return [e for e in events if not e.get('rolled_back')], errors


def detect(tx, slot):
    """Preserve original discriminator/frame audit, adding strict payload and identity evidence."""
    errors, instructions, counts = [], [], collections.Counter()
    signatures = (tx.get('transaction') or {}).get('signatures') or [None]
    signature = signatures[0]
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
            if raw[:8] not in KNOWN_INSTRUCTIONS:
                errors.append('UNAUDITED_INSTRUCTION')
            kind = DISC.get(raw[:8])
            item = {'position':position,'kind':kind,'discriminator':raw[:8].hex()}
            if kind:
                counts[kind] += 1
                if kind in COMPLETION:
                    ti,ai={'CLASSIC':(5,4),'RECEIVERSHIP_END':(0,3),
                           'FORCED_DELEVERAGE':(0,3),'BANKRUPTCY_SETTLEMENT':(3,1)}[kind]
                    accounts=ix['accounts']
                    def account_at(index):
                        n=accounts[index]
                        if type(n) is not int or not 0<=n<len(keys):
                            raise ValueError('ACCOUNT_INDEX')
                        return keys[n]
                    item.update(target=account_at(ti),actor=account_at(ai))
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
        self.set('identity',identity)
        if rpc is not None:
            binding=hashlib.sha256(getattr(rpc,'url','OFFLINE_FIXTURE').encode()).hexdigest()
            if self.get('provider_binding') not in (None,binding):
                self.close();raise AuditError('RESUME_PROVIDER_CHANGED')
            self.set('provider_binding',binding)
        self.db.commit()
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
            if not isinstance(batch,list) or len(batch)>1000:
                raise AuditError('INDEX_PAGE_LAYOUT')
            if not batch:
                raise AuditError('INDEX_HISTORY_EXHAUSTED_BEFORE_START')
            crossed=True
            page_times=[r.get("blockTime") for r in batch if isinstance(r,dict) and type(r.get("blockTime")) is int]
            if before is None:
                self.set("head_after_end",bool(page_times and max(page_times)*NS >= self.window["end_ns"]))
            with self.db:
                for row in batch:
                    if not isinstance(row,dict):
                        raise AuditError('INDEX_ROW_LAYOUT')
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
                    if bt is None or bt*NS >= self.window['start_ns']:
                        crossed=False
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
            try:
                tx=self.raw('transactions',sig,lambda:self.rpc.call('getTransaction',[sig,{'encoding':'json','commitment':'finalized','maxSupportedTransactionVersion':1}]))
            except AuditError as exc:
                if str(exc)!='RPC_UNSUPPORTED_TX_VERSION':
                    raise
                with self.db:
                    self.db.execute("UPDATE signatures SET status='unsupported',detail=? WHERE sig=?",(json.dumps({'membership':'UNKNOWN','error':str(exc)}),sig))
                continue
            if tx is None:
                with self.db:
                    self.db.execute("UPDATE signatures SET status='missing',detail=? WHERE sig=?",(json.dumps({'membership':'UNKNOWN','error':'TRANSACTION_UNAVAILABLE'}),sig))
                continue
            if not isinstance(tx,dict) or tx.get('slot')!=slot or ((tx.get('transaction') or {}).get('signatures') or [None])[0]!=sig:
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

    def import_file(self,path,consume):
        if not path.exists():
            return
        name=str(path.relative_to(self.run))
        stat=path.stat()
        old=self.db.execute('SELECT size,mtime,offset FROM input_cursor WHERE path=?',(name,)).fetchone()
        if old and old[:2]!=(stat.st_size,stat.st_mtime_ns):
            raise AuditError('LOCAL_INPUT_CHANGED_DURING_RESUME')
        offset=old[2] if old else 0
        for row,next_offset in rows(path,offset):
            self.budget()
            with self.db:
                consume(row)
                self.db.execute('INSERT OR REPLACE INTO input_cursor VALUES(?,?,?,?)',(name,stat.st_size,stat.st_mtime_ns,next_offset))
        with self.db:
            self.db.execute('INSERT OR REPLACE INTO input_cursor VALUES(?,?,?,?)',(name,stat.st_size,stat.st_mtime_ns,stat.st_size))

    def import_bot(self):
        market=self.run/'market' if (self.run/'market').is_dir() else self.run
        def transaction(row):
            sig,slot,source=row.get('signature'),row.get('slot'),row.get('source','UNKNOWN')
            if not sig or type(slot) is not int:
                raise AuditError('BOT_TRANSACTION_IDENTITY')
            self.db.execute('INSERT OR IGNORE INTO bot_tx VALUES(?,?,?)',(sig,slot,source))
            if source in ('logsSubscribe','transactionSubscribe'):
                raw=row.get('raw') or {}
                value=raw.get('value',raw)
                logs=value.get('logs') or (value.get('meta') or {}).get('logMessages')
                if row.get('success') is True and isinstance(logs,list):
                    events,errors=decode_logs(logs)
                    if not errors:
                        for ev in events:
                            event({**ev,'signature':sig,'slot':slot},'LIVE_PAYLOAD')
        def event(row,source):
            kind=row.get('liquidation_type',row.get('type','RECEIVERSHIP' if 'receiver' in row else None))
            if kind not in KINDS:
                return
            index=row.get('event_index')
            if not row.get('signature') or type(row.get('slot')) is not int or type(index) is not int:
                raise AuditError('BOT_EVENT_IDENTITY')
            self.db.execute('INSERT OR REPLACE INTO bot_events VALUES(?,?,?,?,?,?)',
                (row['signature'],row['slot'],source,index,kind,json.dumps(row)))
        def coverage(row):
            name=row.get('event');at=row.get('timestamp_utc');detail=row.get('detail') or {}
            if at:
                at=utc_ns(at)
            if name=='MARKET_SLOT_BOUNDARY_EVIDENCE' and at is not None:
                self.set('first_live_ns',self.get('first_live_ns',at));self.set('last_live_ns',at)
                self.set('last_live_slot',row.get('slot'))
                pending=self.get('gap_open')
                ack=self.get('reconnect_ack')
                if pending is not None and ack is not None and at>=ack:
                    self.db.execute('INSERT INTO gaps VALUES(?,?,?)',(pending,at,'GAP_THEN_ACK_AND_SLOT'))
                    self.set('gap_open',None);self.set('reconnect_ack',None)
            elif name=='GAP' and row.get('counted',True) and at is not None:
                if self.get('gap_open') is None:
                    self.set('gap_open',at)
                self.set('reconnect_ack',None)
            elif name in ('RECONNECT','RECONNECTED') and at is not None:
                self.set('reconnect_ack',at)
        def shadow(row):
            # Existing final materialized economics, not an alternate simulation/economics path.
            wrapped=row.get('join',row)
            market_row=wrapped.get('market') or {}
            our=wrapped.get('our')
            winner=row.get('winner') or {}
            if not isinstance(winner,dict):
                winner={}
            sig=market_row.get('signature') or winner.get('signature')
            slot=market_row.get('slot') or winner.get('slot')
            target=market_row.get('target') or row.get('target')
            if sig and type(slot) is int and target and our:
                evidence={'market':market_row,'our':our,'strong_join':row.get('strong_join',False),
                          'race':row.get('race'),'capital_bucket_membership_usd':row.get('capital_bucket_membership_usd')}
                self.db.execute('INSERT OR REPLACE INTO shadow VALUES(?,?,?,?)',(sig,slot,target,json.dumps(evidence)))
        self.import_file(market/'transactions.jsonl',transaction)
        self.import_file(market/'liquidations.jsonl',lambda r:event(r,'BOT_LIVE' if r.get('source')=='live' else 'BOT_BACKFILL'))
        # Existing canonical final join contains canonical truth records (distinct from live).
        self.import_file(self.run/'canonical_market_shadow.jsonl',lambda r:(event(r.get('join',r).get('market') or {},'BOT_CANONICAL'),shadow(r)))
        self.import_file(self.run/'opportunity_economics.jsonl',shadow)
        self.import_file(market/'coverage.jsonl',coverage)
        self.set('bot_import_complete',True);self.db.commit()

    def coverage_at(self,bt,boundary):
        if bt is None or boundary:
            return 'TIME_BOUNDARY_UNKNOWN'
        at=bt*NS
        pending=self.get('gap_open')
        # Blocktime has second precision and is not the local receive clock. Keep boundary guards.
        if pending is not None and at>pending+NS:
            return 'PROVEN_WEBSOCKET_OUTAGE_INTERVAL'
        gap=self.db.execute('SELECT start_ns,end_ns FROM gaps WHERE start_ns<? AND end_ns>? LIMIT 1',(at-NS,at+NS)).fetchone()
        if gap:
            return 'PROVEN_WEBSOCKET_OUTAGE_INTERVAL'
        first,last=self.get('first_live_ns'),self.get('last_live_ns')
        if first is not None and last is not None and first+NS<at<last-NS:
            if self.db.execute('SELECT 1 FROM gaps WHERE start_ns<=? AND end_ns>=? LIMIT 1',(at+NS,at-NS)).fetchone():
                return 'TIME_BOUNDARY_UNKNOWN'
            return 'WITHIN_OBSERVED_SLOT_INTERVAL'
        return 'LIVE_COVERAGE_UNPROVEN'

    def compare(self,result):
        sig,slot=result['signature'],result['slot']
        live=bool(self.db.execute("SELECT 1 FROM bot_tx WHERE sig=? AND slot=? AND source IN ('logsSubscribe','transactionSubscribe')",(sig,slot)).fetchone())
        same_sig=bool(self.db.execute("SELECT 1 FROM bot_tx WHERE sig=? AND source IN ('logsSubscribe','transactionSubscribe')",(sig,)).fetchone())
        coverage=self.coverage_at(result.get('block_time'),result.get('boundary_second'))
        tx_status='MATCH' if live else ('SLOT_MISMATCH' if same_sig else 'MISSING_LIVE')
        comparisons=[]
        for ev in result.get('events',[]):
            candidates=self.db.execute('SELECT source,idx,kind,data FROM bot_events WHERE sig=? AND slot=?',(sig,slot))
            matched=[]
            for source,idx,kind,data in candidates:
                bot=json.loads(data)
                # Canonical event_index is all-kind order; old live receiver index is receiver-only.
                position=idx==ev['event_index'] if source in ('BOT_CANONICAL','LIVE_PAYLOAD') else (
                    ev['type']=='RECEIVERSHIP' and idx==sum(e['type']=='RECEIVERSHIP' for e in result['events'][:ev['event_index']]))
                if not position:
                    continue
                same_kind=kind==ev['type']
                target=bot.get('target')
                actor=bot.get('actor',bot.get('liquidator',bot.get('receiver')))
                mismatch=[];unknown=[]
                for key,legacy in [('seized_value_usd','seized_equity_usd'),('repaid_value_usd','repaid_equity_usd'),('protocol_fee_lamports','protocol_flat_fee_lamports')]:
                    a,b=ev.get(key),bot.get(key,bot.get(legacy))
                    if a is None or b is None:
                        unknown.append(key)
                    elif a!=b:
                        mismatch.append(key)
                if ev.get('native_evidence') is not None:
                    if source=='LIVE_PAYLOAD':
                        if bot.get('native_evidence')!=ev['native_evidence']:
                            mismatch.append('native_evidence')
                    else:
                        unknown.append('native_amount_semantics')
                status='MATCH'
                if not same_kind:
                    status='TYPE_MISMATCH'
                elif target!=ev['target'] or actor!=ev['actor']:
                    status='TARGET_OR_ACTOR_MISMATCH'
                elif mismatch:
                    status='AMOUNT_MISMATCH'
                matched.append({'source':source,'status':status,'unknown_amounts':unknown,'different_amounts':mismatch})
            bot_live=[m for m in matched if m['source']=='BOT_LIVE']
            if not live:
                detection={'PROVEN_WEBSOCKET_OUTAGE_INTERVAL':'MISSING_DURING_OUTAGE',
                           'WITHIN_OBSERVED_SLOT_INTERVAL':'MISSING_LIQUIDATION_WITHIN_OBSERVED_INTERVAL'}.get(coverage,'MISSING_LIVE_LIQUIDATION_COVERAGE_UNPROVEN')
            elif bot_live:
                detection=bot_live[0]['status']
            elif ev['type']=='RECEIVERSHIP':
                detection='MISSING_LIQUIDATION_RECORD_WITH_LIVE_TX'
            else:
                detection='LIVE_PAYLOAD_PRESENT_DETECTION_UNPROVEN' if any(m['source']=='LIVE_PAYLOAD' for m in matched) else 'DETECTION_UNPROVEN'
            shadow=self.db.execute('SELECT data FROM shadow WHERE sig=? AND slot=? AND target=?',(sig,slot,ev['target'])).fetchone()
            shadow_data=json.loads(shadow[0]) if shadow else None
            exact=False
            if shadow_data:
                m=shadow_data['market']
                exact=m.get('event_index')==ev['event_index'] and m.get('liquidation_type',m.get('type'))==ev['type']
            comparisons.append({'event':ev,'detection':detection,'comparisons':matched,
                'shadow_status':'SHADOW_MATCH' if exact else ('SHADOW_IDENTITY_UNKNOWN' if shadow_data else 'SHADOW_NO_EVIDENCE'),
                'shadow_evidence':shadow_data,'historical_reconstruction_is_live_race_evidence':False})
        return {'signature':sig,'slot':slot,'transaction_status':tx_status,'live_coverage':coverage,
                'classification':result['classification'],'events':comparisons,
                'shadow_scope':'existing materialized canonical_market_shadow/opportunity_economics; absence is not proof no raw evaluation existed'}

    def report(self,reason):
        counts=collections.Counter();observed=collections.Counter();statuses=collections.Counter();unknown=0;missing=0;in_window=0
        events_path=self.out/'comparison.jsonl'
        temporary=events_path.with_suffix('.jsonl.tmp')
        with temporary.open('w') as stream:
            for status,detail in self.db.execute('SELECT status,detail FROM signatures'):
                if status!='done':
                    missing+=1;continue
                row=json.loads(detail)
                if row.get('membership')=='UNKNOWN':
                    unknown+=1;continue
                if row.get('membership')!='IN_WINDOW':
                    continue
                in_window+=1
                observed.update(row.get('program_events',{}))
                if row.get('classification')=='UNKNOWN':
                    unknown+=1
                elif row.get('classification')=='CLASSIFIED':
                    counts.update(row.get('program_events',{}))
                comparison=self.compare(row)
                statuses.update([comparison['transaction_status']])
                statuses.update(e['detection'] for e in comparison['events'])
                stream.write(json.dumps({'historical':row,'darkbot':comparison},allow_nan=False)+'\n')
            stream.flush();os.fsync(stream.fileno())
        os.replace(temporary,events_path)
        bot_only=self.out/'bot_only.jsonl'
        with bot_only.with_suffix('.jsonl.tmp').open('w') as f:
            for sig,slot in self.db.execute("SELECT DISTINCT b.sig,b.slot FROM bot_tx b LEFT JOIN signatures s ON b.sig=s.sig AND b.slot=s.slot WHERE b.source IN ('logsSubscribe','transactionSubscribe') AND s.sig IS NULL"):
                statuses['BOT_ONLY_INDEX_MISSING']+=1
                f.write(json.dumps({'signature':sig,'slot':slot,'status':'INDEX_MISSING_OR_INCOMPLETE; local receive window differs from chain time'})+'\n')
        os.replace(bot_only.with_suffix('.jsonl.tmp'),bot_only)
        complete=self.get('crossed_start',False) and self.get('head_after_end',False) and missing==0 and unknown==0 and reason is None
        summary={'schema':SCHEMA,'run_id':self.run.name,'window':self.window,
            'status':'INDEX_VERIFIED' if complete else 'HISTORICAL_INCOMPLETE',
            'canonical_verified':False,'c0_e1_master_pass':False,'live_ready':False,
            'reason':reason,'incomplete_reasons':([reason] if reason else []) + ([] if self.get('crossed_start',False) else ['INDEX_START_NOT_CROSSED']) + ([] if self.get('head_after_end',False) else ['INDEX_HEAD_BEFORE_END']) + (['HISTORICAL_PAYLOAD_UNKNOWN'] if unknown or missing else []),'crossed_start':self.get('crossed_start',False),'head_after_end':self.get('head_after_end',False),
            'pages_persisted':self.get('pages',0),'indexed_signatures':self.db.execute('SELECT count(*) FROM signatures').fetchone()[0],
            'in_window_transactions':in_window,'unresolved_transactions':missing,'unknown_transactions':unknown,
            'qualified_liquidations':{k:counts[k] for k in KINDS},'observed_events_including_unknown_transactions':{k:observed[k] for k in KINDS},
            'zero_liquidations_proven_within_index_scope':complete and not counts,
            'comparison_counts':dict(statuses),'bot_import_complete':self.get('bot_import_complete',False),
            'live_coverage':{'first_slot_evidence_utc':iso_ns(self.get('first_live_ns')) if self.get('first_live_ns') is not None else None,
                'last_slot_evidence_utc':iso_ns(self.get('last_live_ns')) if self.get('last_live_ns') is not None else None,
                'last_live_slot':self.get('last_live_slot'),'open_gap_start_utc':iso_ns(self.get('gap_open')) if self.get('gap_open') is not None else None,
                'tail_without_slot_evidence_seconds':(self.window['end_ns']-self.get('last_live_ns'))/NS if self.get('last_live_ns') is not None else None},
            'rpc_this_attempt':{'calls':self.rpc.calls if self.rpc else 0,'methods':dict(self.rpc.methods) if self.rpc else {},
                'response_bytes':self.rpc.bytes_received if self.rpc else 0,'request_bytes':self.rpc.bytes_sent if self.rpc else 0},
            'provider_identity':'opaque SHA256 binding retained in checkpoint; credential/endpoint changes reject mixed-source resume',
            'provider_assumptions':['finalized address index pagination supplied by configured provider; no independent provider crosscheck',
                'address-index LUT-only/CPI delivery completeness unproven; INDEX_VERIFIED is conditional index-scope evidence',
                'blockTime is coarse chain evidence, not local receive/decision time; boundary seconds not classified as live misses'],
            'economic_non_claims':['No realized PnL; no historical race/speed/profit reconstruction',
                'meta.fee retained per transaction; not multiplied by event count; absent cost/price/amount remains null',
                'No full-block downloads; transaction_index unknown unless actually supplied by provider'],
            'resources':{'cpu_seconds':time.process_time(),'max_rss_platform_units':resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                'verification_disk_bytes':sum(p.stat().st_size for p in self.out.rglob('*') if p.is_file())}}
        cumulative= {'calls':0,'response_bytes':0,'request_bytes':0}
        for path in (self.out/'attempts').glob('*.json'):
            previous=json.loads(path.read_text()).get('rpc_this_attempt',{})
            for key in cumulative:
                cumulative[key]+=previous.get(key,0)
        for key in cumulative:
            cumulative[key]+=summary['rpc_this_attempt'][key]
        summary['rpc_all_attempts']=cumulative
        stamp=str(time.time_ns())
        atomic_json(self.out/'attempts'/f'{stamp}.json',summary)
        atomic_json(self.out/'summary.json',summary)
        report=(f"{summary['status']} — {self.run.name}\n{self.window['start_utc']} → {self.window['end_utc']}\n"
                f"Reason: {reason or 'none'}; pages: {summary['pages_persisted']}; transactions in window: {in_window}\n"
                f"Qualified events: {json.dumps(summary['qualified_liquidations'])}\n"
                f"Unknown transactions: {unknown}; unresolved: {missing}\n"
                f"DarkBot: {json.dumps(dict(statuses))}\nRPC: {json.dumps(summary['rpc_this_attempt'])}\n"
                "INDEX scope only. CANONICAL_VERIFIED=false; C0_E1_MASTER_PASS=false. UNKNOWN is never zero.\n"
                "Historical chain data does not establish that DarkBot could have traded an event in time.\n")
        (self.out/'report.txt').write_text(report)
        return summary

    def execute(self):
        reason=None
        try:
            self.import_bot()
            if self.rpc is None:
                raise AuditError('RPC_ACCESS_MISSING')
            # Continue pending TX work even when page budget was exhausted last time.
            try:
                self.paginate()
            except AuditError as exc:
                reason=str(exc)
                if reason not in ('INDEX_PAGE_BUDGET','INDEX_HISTORY_EXHAUSTED_BEFORE_START'):
                    raise
            self.fetch_transactions()
        except AuditError as exc:
            reason=str(exc)
        except KeyboardInterrupt:
            reason='INTERRUPTED_RESUMABLE'
        except (ValueError,KeyError,TypeError,IndexError,AttributeError):
            reason='MALFORMED_EVIDENCE_INCOMPLETE'
        self.db.commit()
        return self.report(reason)


def find_run(value):
    direct=Path(value).expanduser()
    root=Path(__file__).resolve().parents[1]
    candidates=[direct] if direct.is_dir() else [root/'data/econ4h'/value,root/'data/market'/value]
    candidates=[p.resolve() for p in candidates if p.is_dir()]
    if len(candidates)!=1:
        raise AuditError('RUN_NOT_FOUND_OR_AMBIGUOUS')
    return candidates[0]


def quick_main(argv=None):
    ap=argparse.ArgumentParser(description='Independent, resumable finalized Marginfi UTC index verification; read-only RPC')
    ap.add_argument('run_dir',nargs='?')
    ap.add_argument('--run')
    ap.add_argument('--mode',choices=['index'],default='index',help='strict remains the existing Rust qualification; crosscheck not claimed')
    ap.add_argument('--rps',type=float,default=2)
    ap.add_argument('--max-pages',type=int,default=50,help='per invocation; exhaustion is INCOMPLETE, cursor resumes')
    ap.add_argument('--max-calls',type=int,default=2000)
    ap.add_argument('--max-seconds',type=int,default=300)
    ap.add_argument('--timeout',type=int,default=15)
    ap.add_argument('--attempts',type=int,default=3)
    ap.add_argument('--max-bytes',type=int,default=256*1024*1024)
    ap.add_argument('--offline',action='store_true',help='import local evidence and write missing-RPC status without network')
    ap.add_argument('--self-test',action='store_true')
    args=ap.parse_args(argv)
    if args.self_test:
        import unittest
        suite=unittest.defaultTestLoader.discover(str(Path(__file__).parent),'test_audit_market_quick.py')
        return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 2
    if not args.run and not args.run_dir:
        ap.error('--run or legacy positional run path required')
    if args.run and args.run_dir:
        ap.error('choose --run or positional path, not both')
    if not (0<args.rps<=10 and 1<=args.max_pages<=500 and 1<=args.max_calls<=100000 and 1<=args.max_seconds<=3600
            and 1<=args.timeout<=60 and 1<=args.attempts<=5 and 1024<=args.max_bytes<=2**30):
        ap.error('invalid bounded RPC settings; see --help')
    try:
        run=find_run(args.run or args.run_dir)
        url=os.environ.get('P0_AUDIT_RPC_URL') or os.environ.get('P0_RPC_URL')
        if not url and os.environ.get('HELIUS_API_KEY'):
            url='https://mainnet.helius-rpc.com/?api-key='+os.environ['HELIUS_API_KEY']
        rpc=RPC(url,args.rps,args.timeout,args.attempts,args.max_calls,args.max_seconds,args.max_bytes) if url and not args.offline else None
        audit=Audit(run,rpc,args.max_pages,args.max_seconds)
        try:
            summary=audit.execute()
            print((audit.out/'report.txt').read_text(),end='')
            print('Evidence:',audit.out)
            return 0 if summary['status']=='INDEX_VERIFIED' else 2
        finally:
            audit.close()
    except (AuditError,OSError,ValueError,KeyError,TypeError) as exc:
        # Provider/environment text must never reach terminal or persisted reports.
        print('HISTORICAL_INCOMPLETE:',str(exc) if isinstance(exc,AuditError) else 'LOCAL_INPUT_OR_IO_ERROR',file=sys.stderr)
        return 2


if __name__=='__main__':
    sys.exit(quick_main())

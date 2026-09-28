"""Offline evidence collector. Only allowlisted read-only Ethereum RPC methods.

Not a trading runtime, signer, benchmark, or a Gate-0 PASS oracle.
Preserves requests/responses; never retries a value-bearing operation (none exist).
"""
import argparse
import datetime as dt
import hashlib
import json
import pathlib
import time
import urllib.request

ALLOWED = frozenset({
    'eth_chainId', 'web3_clientVersion', 'eth_getBlockByNumber',
    'eth_getBlockByHash', 'eth_getCode', 'eth_getStorageAt', 'eth_getLogs',
    'eth_getTransactionByHash', 'eth_getTransactionReceipt',
    'eth_getBlockReceipts', 'web3_sha3', 'eth_call',
})
ADDRESSES = {
    'flap': '0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0',
    'four': '0x5c952063c7fc8610ffdb798152d69f0b9550762b',
}
SLOTS = {
    'implementation': '0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc',
    'beacon': '0xa3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6cb3582b35133d50',
}


class Recorder:
    def __init__(self, url, out):
        self.url, self.out = url, pathlib.Path(out)
        self.out.mkdir(parents=True, exist_ok=False)
        self.seq = 0

    def batch(self, calls, single=False):
        for method, params in calls:
            if method not in ALLOWED:
                raise ValueError('Forbidden RPC method: ' + method)
        request = [dict(jsonrpc='2.0', id=i+1, method=m, params=p)
                   for i, (m, p) in enumerate(calls)]
        if single:
            if len(request) != 1:
                raise ValueError('Single request mode requires exactly one call')
            request = request[0]
        self.seq += 1
        stem = self.out / f'{self.seq:03d}'
        stem.with_suffix('.request.json').write_text(json.dumps(request, indent=2)+'\n')
        started = dt.datetime.now(dt.timezone.utc).isoformat()
        t = time.monotonic_ns()
        req = urllib.request.Request(self.url, data=json.dumps(request).encode(),
              headers={'Content-Type': 'application/json', 'User-Agent': 'DarkBot-G0-ReadOnly/1'})
        try:
            with urllib.request.urlopen(req, timeout=25) as response:
                raw = response.read(40_000_001)
                if len(raw) > 40_000_000:
                    raise ValueError('Response exceeds evidence capture bound')
                status = response.status
            stem.with_suffix('.response.json').write_bytes(raw)
            elapsed = time.monotonic_ns() - t
            stem.with_suffix('.meta.json').write_text(json.dumps(dict(
                endpoint=self.url, started_utc=started, http_status=status,
                elapsed_ns=elapsed, response_sha256=hashlib.sha256(raw).hexdigest(),
                timing_use='RPC diagnostic only; not BUY latency'), indent=2)+'\n')
            data = json.loads(raw)
            if single and isinstance(data, dict):
                data = [data]
            if not isinstance(data, list):
                raise ValueError('Batch response is not a list: '+str(data)[:300])
            by_id = {x['id']: x for x in data}
            if len(by_id) != len(data) or set(by_id) != set(range(1,len(calls)+1)):
                raise ValueError('Unexpected/missing/duplicate response IDs')
            return [by_id[i+1] for i in range(len(calls))]
        except Exception as exc:
            stem.with_suffix('.error.txt').write_text(type(exc).__name__+': '+str(exc)+'\n')
            raise


def result(r):
    if 'error' in r:
        raise ValueError(str(r['error']))
    return r['result']


def freeze(rec):
    initial = rec.batch([('eth_chainId', []), ('eth_getBlockByNumber', ['finalized',False])])
    if int(result(initial[0]),16) != 56:
        raise ValueError('Wrong chain')
    block = result(initial[1])
    tag = block['number']
    (rec.out/'reference_block.json').write_text(json.dumps(block,indent=2)+'\n')
    calls=[]; labels=[]
    for venue,address in ADDRESSES.items():
        calls.append(('eth_getCode',[address,tag])); labels.append((venue,'code',address))
        for name,slot in SLOTS.items():
            calls.append(('eth_getStorageAt',[address,slot,tag])); labels.append((venue,name,address))
    replies=rec.batch(calls)
    summary={'block_number':int(tag,16), 'block_hash':block['hash'], 'contracts':{}, 'errors':[]}
    impl_calls=[]; impl_labels=[]
    for (venue,kind,address),reply in zip(labels,replies):
        item=summary['contracts'].setdefault(venue,{'address':address})
        if 'error' in reply:
            summary['errors'].append(dict(venue=venue,kind=kind,error=reply['error'])); continue
        value=result(reply)
        if kind=='code':
            code=bytes.fromhex(value[2:]); item['code_bytes']=len(code)
            item['code_sha256']=hashlib.sha256(code).hexdigest()
            (rec.out/f'{venue}.code.hex').write_text(value+'\n')
            if not code: summary['errors'].append(dict(venue=venue,kind=kind,error='EMPTY_CODE'))
        else:
            item[kind+'_slot']=value
            if int(value,16):
                target='0x'+value[-40:]; item[kind+'_address']=target
                impl_calls.append(('eth_getCode',[target,tag])); impl_labels.append((venue,kind,target))
    if impl_calls:
        for (venue,kind,address),reply in zip(impl_labels,rec.batch(impl_calls)):
            if 'error' in reply:
                summary['errors'].append(dict(venue=venue,kind=kind+'_code',error=reply['error'])); continue
            code=bytes.fromhex(result(reply)[2:])
            (rec.out/f'{venue}.{kind}.code.hex').write_text(result(reply)+'\n')
            summary['contracts'][venue][kind+'_code_bytes']=len(code)
            summary['contracts'][venue][kind+'_code_sha256']=hashlib.sha256(code).hexdigest()
            if not code: summary['errors'].append(dict(venue=venue,kind=kind+'_code',error='EMPTY_CODE'))
    check=result(rec.batch([('eth_getBlockByNumber',[tag,False])])[0])
    summary['block_hash_rechecked']=check['hash']==block['hash']
    summary['status']='READ_EVIDENCE_CAPTURED' if not summary['errors'] and summary['block_hash_rechecked'] else 'INCOMPLETE'
    summary['scope']='Bytecode/standard-slot snapshot only; source/ABI/launch semantics NOT verified'
    (rec.out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(json.dumps(summary,indent=2),flush=True)


def scan(rec, count):
    if not 1 <= count <= 512:
        raise ValueError('Scan bound must be 1..512 blocks')
    replies=rec.batch([('eth_chainId',[]),('eth_getBlockByNumber',['finalized',False])])
    if int(result(replies[0]),16)!=56:
        raise ValueError('Wrong chain')
    anchor=result(replies[1]); top=int(anchor['number'],16)
    matches=[]; examined=0; previous=anchor; launch=None
    for offset in range(0,count,16):
        numbers=list(range(top-offset,max(top-count,top-offset-16),-1))
        blocks=rec.batch([('eth_getBlockByNumber',[hex(n),True]) for n in numbers])
        for n,reply in zip(numbers,blocks):
            block=result(reply)
            if int(block['number'],16)!=n:
                raise ValueError('Wrong block height')
            if n==top:
                if block['hash']!=anchor['hash']: raise ValueError('Anchor changed')
            elif previous['parentHash']!=block['hash']:
                raise ValueError('Disconnected block ancestry')
            previous=block; examined+=1
            for tx in block['transactions']:
                destination=(tx.get('to') or '').lower()
                if destination in ADDRESSES.values():
                    matches.append(dict(block_number=n,block_hash=block['hash'],tx=tx))
                    if destination==ADDRESSES['four'] and tx['input'].startswith('0x519ebb10'):
                        launch=matches[-1]
        print(json.dumps(dict(blocks_examined=examined,venue_calls=len(matches),four_create_candidate=launch is not None)),flush=True)
        if launch is not None:
            break
    (rec.out/'venue_calls.json').write_text(json.dumps(matches,indent=2)+'\n')
    summary=dict(anchor_number=top,anchor_hash=anchor['hash'],blocks_examined=examined,
                 direct_venue_calls=len(matches),ancestry_checked=True,
                 scope='Bounded direct-to-contract scan only; NOT complete launch census or pending coverage')
    if launch:
        receipt=result(rec.batch([('eth_getTransactionReceipt',[launch['tx']['hash']])])[0])
        if receipt['blockHash']!=launch['block_hash'] or receipt['transactionHash']!=launch['tx']['hash']:
            raise ValueError('Receipt identity mismatch')
        (rec.out/'four_create_transaction.json').write_text(json.dumps(launch,indent=2)+'\n')
        (rec.out/'four_create_receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
        summary.update(candidate_tx=launch['tx']['hash'],receipt_status=receipt['status'])
    (rec.out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(json.dumps(summary,indent=2),flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--url',required=True)
    parser.add_argument('--out',required=True)
    parser.add_argument('--requests',help='JSON array containing read-only method/params')
    parser.add_argument('--single',action='store_true',help='Send one allowlisted call without a batch envelope')
    parser.add_argument('--scan-blocks',type=int,help='Bounded canonical scan, no pending subscription')
    args=parser.parse_args()
    rec=Recorder(args.url,args.out)
    if args.requests and args.scan_blocks:
        raise ValueError('Choose one operation')
    if args.scan_blocks:
        scan(rec,args.scan_blocks)
    elif args.requests:
        requests=json.loads(pathlib.Path(args.requests).read_text())
        replies=rec.batch([(r['method'],r['params']) for r in requests],single=args.single)
        print(json.dumps([{'id':r['id'],'error':r.get('error'),
            'result_type':type(r.get('result')).__name__} for r in replies]),flush=True)
    else:
        freeze(rec)

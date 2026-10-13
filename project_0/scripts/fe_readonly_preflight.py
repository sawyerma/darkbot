#!/usr/bin/env python3
"""FE-01 bounded deployment/target observation using the existing P3 RPC transport.
Requires explicit user permission to run --execute. No WSS, signer, build or send.
Observation is not protocol/source parity, simulation or PILOT_READY.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import time
import signal
import audit_market_quick as audit
import measurement_contract as contract

LOADER='BPFLoaderUpgradeab1e11111111111111111111111'
TARGETS=('A45dVtuyLccxs1xpbVJsTNh7AdtAotAZi4UxEcyMQDMS','64vknRpPQgoyLDvro5GZtYk39odDtkLQ99Ye71T3vzEk')

def account(value,owner,tag=None):
    if not isinstance(value,dict) or value.get('owner')!=owner:raise audit.AuditError('ACCOUNT_OWNER_MISMATCH')
    data=value.get('data')
    if not isinstance(data,list) or len(data)!=2 or data[1]!='base64':raise audit.AuditError('ACCOUNT_ENCODING_UNKNOWN')
    try:raw=base64.b64decode(data[0],validate=True)
    except ValueError:raise audit.AuditError('ACCOUNT_DATA_INVALID') from None
    if tag is not None and (len(raw)<4 or int.from_bytes(raw[:4],'little')!=tag):raise audit.AuditError('LOADER_LAYOUT_UNKNOWN')
    return raw


def probe(rpc):
    program=rpc.call('getAccountInfo',[audit.PROGRAM,{'encoding':'base64','commitment':'finalized'}])
    raw=account(program.get('value'),LOADER,2)
    if len(raw)!=36 or program['value'].get('executable') is not True:raise audit.AuditError('PROGRAM_LAYOUT_UNKNOWN')
    pd=audit.base58_encode(raw[4:]);slot=program['context']['slot']
    data=rpc.call('getAccountInfo',[pd,{'encoding':'base64','commitment':'finalized','minContextSlot':slot}])
    pd_raw=account(data.get('value'),LOADER,3)
    if len(pd_raw)<45 or pd_raw[12] not in (0,1):raise audit.AuditError('PROGRAMDATA_LAYOUT_UNKNOWN')
    if type(data['context']['slot']) is not int or data['context']['slot']<slot:raise audit.AuditError('RPC_CONTEXT_REGRESSION')
    targets=rpc.call('getMultipleAccounts',[list(TARGETS),{'encoding':'base64','commitment':'finalized','minContextSlot':data['context']['slot']}])
    if type(targets['context']['slot']) is not int or targets['context']['slot']<data['context']['slot']:raise audit.AuditError('RPC_CONTEXT_REGRESSION')
    if len(targets['value'])!=len(TARGETS):raise audit.AuditError('TARGET_RESPONSE_INCOMPLETE')
    rows=[]
    for target,value in zip(TARGETS,targets['value']):
        if value is None:rows.append({'account':target,'status':'MISSING'});continue
        raw=account(value,audit.PROGRAM)
        rows.append({'account':target,'status':'OBSERVED_NOT_HEALTH_QUALIFIED','bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
    return {'schema':'Project0/FE01Observation/1','status':'OBSERVED_NOT_QUALIFIED','program':audit.PROGRAM,
        'program_data':pd,'program_data_sha256':hashlib.sha256(pd_raw).hexdigest(),
        'deployment_slot':int.from_bytes(pd_raw[4:12],'little'),
        'context_slots':[program['context']['slot'],data['context']['slot'],targets['context']['slot']],
        'targets':rows,'pilot_ready':False,'simulation':'NOT_RUN','source_parity':'UNKNOWN',
        'live_enabled':False,'submission_enabled':False}


def main(argv=None):
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--execute',action='store_true');p.add_argument('--output',type=Path,required=True)
    args=p.parse_args(argv)
    if not args.execute:p.error('--execute only after explicit permission; at most 3 calls / 30s / 16MiB / 1rps')
    if args.output.exists():p.error('output exists; preserve prior evidence')
    url=contract.endpoint()
    if not url:p.error('RPC endpoint missing in local environment')
    rpc=audit.RPC(url,1,8,1,3,30,16*1024*1024,allowed_methods={'getAccountInfo','getMultipleAccounts'})
    def deadline(signum,frame):raise audit.AuditError('RPC_TIME_BUDGET')
    signal.signal(signal.SIGALRM,deadline);signal.alarm(30)
    try:result=probe(rpc)
    except (audit.AuditError,OSError,ValueError,KeyError,TypeError) as exc:
        result={'status':'EXTERNAL_BLOCKED','reason':str(exc) if isinstance(exc,audit.AuditError) else 'LOCAL_OR_PROVIDER_DATA_INVALID','pilot_ready':False,'live_enabled':False,'submission_enabled':False}
    finally:signal.alarm(0)
    result['rpc_actual']={'calls':rpc.calls,'bytes_sent':rpc.bytes_sent,'bytes_received':rpc.bytes_received,'methods':dict(rpc.methods)}
    result['observed_at_unix_s']=int(time.time());result['request_budget']=3
    contract.immutable(args.output,result);print(json.dumps(result,indent=2));return 0 if result['status']=='OBSERVED_NOT_QUALIFIED' else 2

if __name__=='__main__':raise SystemExit(main())

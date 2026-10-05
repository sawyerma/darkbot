"""Read-only public transaction evidence; RPC URL is read from environment, never saved.
Usage: P0_RPC_URL=... python3 collect_history.py OUTPUT_DIRECTORY
No signing, simulation or submission methods are called.
"""
import os,sys,json,urllib.request,hashlib,base64,struct,time,datetime
from pathlib import Path
url=os.environ['P0_RPC_URL']; out=Path(sys.argv[1]); out.mkdir(parents=True,exist_ok=True)
program='MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA'
def rpc(method,params):
 req=urllib.request.Request(url,json.dumps({'jsonrpc':'2.0','id':1,'method':method,'params':params}).encode(),{'Content-Type':'application/json'})
 for attempt in range(4):
  try:
   v=json.load(urllib.request.urlopen(req,timeout=45))
   if 'error' in v: raise RuntimeError(str(v['error'].get('code')))
   return v.get('result')
  except Exception:
   if attempt==3: raise RuntimeError('RPC failed: '+method)
   time.sleep(1+attempt)
alpha='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
def b58(b):
 n=int.from_bytes(b,'big'); s=''
 while n: n,r=divmod(n,58); s=alpha[r]+s
 return '1'*(len(b)-len(b.lstrip(b'\0')))+s
D=hashlib.sha256(b'event:LiquidationReceiverEvent').digest()[:8]
# Select recent real events through on-chain LiquidationRecord entries. This avoids
# mistaking price-cache cranks (most program traffic) for liquidation transactions.
r=rpc('getProgramAccounts',[program,{'encoding':'base64','commitment':'finalized','withContext':True,
 'filters':[{'memcmp':{'offset':0,'bytes':'X3QXhFnS9aI=','encoding':'base64'}}],
 'dataSlice':{'offset':0,'length':328}}])
records=[]
for item in r['value']:
 data=base64.b64decode(item['account']['data'][0])
 if len(data)<328:continue
 ts=max(struct.unpack_from('<q',data,136+i*48+24)[0] for i in range(4))
 if ts>0:records.append({'record':item['pubkey'],'target':b58(data[40:72]),'last_liquidation_unix_s':ts})
records.sort(key=lambda x:x['last_liquidation_unix_s'],reverse=True)
(out/'recent_liquidation_records.json').write_text(json.dumps({'context':r['context'],'record_count':len(r['value']),'selected':records[:20]},indent=2))
sigs=[]; seen=set()
for record in records[:12]:
 batch=rpc('getSignaturesForAddress',[record['record'],{'limit':8,'commitment':'finalized'}])
 for sig in batch:
  if sig['signature'] not in seen:sigs.append(sig);seen.add(sig['signature'])
 time.sleep(0.3)
(out/'selected_signatures.json').write_text(json.dumps(sigs,indent=2))
print('selected records',len(records),'candidate signatures',len(sigs),flush=True)
found=[]; inspected=0; failures=[]
for s in sigs:
 if s['err'] is not None:continue
 inspected+=1
 try:t=rpc('getTransaction',[s['signature'],{'encoding':'json','commitment':'finalized','maxSupportedTransactionVersion':1}])
 except RuntimeError:failures.append(s['signature']);continue
 if not t:continue
 events=[]
 for log in t['meta'].get('logMessages') or []:
  if log.startswith('Program data: '):
   try:b=base64.b64decode(log[14:])
   except Exception:continue
   if b[:8]==D and len(b)>=92:
    seized,repaid,flat=struct.unpack_from('<ddI',b,72)
    events.append({'target':b58(b[8:40]),'receiver':b58(b[40:72]),'seized_equity_usd':seized,'repaid_equity_usd':repaid,'protocol_flat_fee_lamports':flat})
 if events:
  (out/(s['signature']+'.json')).write_text(json.dumps(t,indent=2))
  found.append({'signature':s['signature'],'slot':t['slot'],'blockTime':t['blockTime'],'fee_lamports':t['meta']['fee'],'events':events})
  print('liquidation',len(found),s['signature'],events,flush=True)
 if len(found)>=6:break
 if inspected%10==0: print('inspected',inspected,flush=True)
(out/'history_summary.json').write_text(json.dumps({'retrieved_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'program':program,'signatures_scanned':len(sigs),'successful_transactions_inspected':inspected,'fetch_failures':failures,'events':found,'historical_account_prestate_available':False},indent=2))
print('DONE',inspected,len(found),flush=True)

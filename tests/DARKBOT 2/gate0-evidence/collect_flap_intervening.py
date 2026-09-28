"""Read-only G0.2 state-transition evidence. URL is read hidden and never persisted."""
import datetime,getpass,hashlib,json,pathlib,time,urllib.request,urllib.error
ROOT=pathlib.Path(__file__).resolve().parent
OUT=ROOT/'2026-09-28-g02-archive-witness-quicknode-153754'/'intervening_raw'
OUT.mkdir(exist_ok=True)
endpoint=getpass.getpass('Archive RPC (hidden): ')
prefix=json.loads((ROOT/'2026-09-28-g02-archive-witness/transaction_prefix.json').read_text())
first=json.loads((OUT.parent/'first_buy_prestate.json').read_text())['result']
def rpc(name,method,params):
 request={'jsonrpc':'2.0','id':1,'method':method,'params':params}
 reqpath=OUT/(name+'.request.json');respath=OUT/(name+'.response.json')
 if respath.exists():
  assert json.loads(reqpath.read_text())==request
  old=json.loads(respath.read_text())
  if 'result' in old and not old.get('error'):return old['result']
 reqpath.write_text(json.dumps(request,indent=2)+'\n')
 started=datetime.datetime.now(datetime.timezone.utc).isoformat()
 for attempt in range(3):
  try:
   req=urllib.request.Request(endpoint,data=json.dumps(request).encode(),headers={'Content-Type':'application/json'})
   with urllib.request.urlopen(req,timeout=60) as response:raw=response.read(40000001)
   if len(raw)>40000000:raise RuntimeError('Response bound exceeded')
   if endpoint.encode() in raw or endpoint.rstrip('/').split('/')[-1].encode() in raw:raise RuntimeError('Response contains endpoint credential; not persisted')
   data=json.loads(raw)
   respath.write_bytes(raw)
   (OUT/(name+'.meta.json')).write_text(json.dumps({'provider':'QuickNode','started_utc':started,'sha256':hashlib.sha256(raw).hexdigest()},indent=2)+'\n')
   if data.get('error'):raise RuntimeError(json.dumps(data['error']))
   assert 'result' in data
   return data['result']
  except urllib.error.HTTPError as e:
   if e.code==429 and attempt<2:time.sleep(2**attempt);continue
   raise RuntimeError('HTTP '+str(e.code)+' '+str(e.reason)) from None
  except urllib.error.URLError:raise RuntimeError('RPC connection failed (URL redacted)') from None
 raise RuntimeError('Retry limit')
assert int(rpc('chain_id','eth_chainId',[]),16)==56
headers={}
for number in [124415356,124415357,124415358]:headers[number]=rpc('block_'+str(number),'eth_getBlockByNumber',[hex(number),False])
assert headers[124415357]['parentHash']==headers[124415356]['hash']
assert headers[124415358]['parentHash']==headers[124415357]['hash']
assert len(headers[124415357]['transactions'])==51
for tx in prefix:
 b=headers[tx['block_number']];assert b['hash']==tx['block_hash'];assert b['transactions'][tx['tx_index']]==tx['tx_hash']
print('Chain, Blockhashes und 65 TX-Identitäten bestätigt.',flush=True)
for i,tx in enumerate(prefix[:-1]):
 name='diff_'+str(tx['block_number'])+'_'+str(tx['tx_index']).zfill(3)
 result=rpc(name,'debug_traceTransaction',[tx['tx_hash'],{'tracer':'prestateTracer','tracerConfig':{'diffMode':True},'timeout':'30s'}])
 assert isinstance(result,dict) and set(result)=={'pre','post'}
 if i%10==0 or i==65:print(str(i+1)+'/66 State-Diffs gesichert (Launch + 65 Zwischen-TXs).',flush=True)
# Parent block baseline for every state item actually touched by First BUY.
# EIP-1898 hash reference prevents accidental block-number/reorg ambiguity.
tag={'blockHash':headers[124415356]['hash'],'requireCanonical':True}
for i,(address,account) in enumerate(first.items()):
 label=address[2:]
 for kind,method in [('code','eth_getCode'),('balance','eth_getBalance'),('nonce','eth_getTransactionCount')]:rpc('parent_'+label+'_'+kind,method,[address,tag])
 for key in account.get('storage',{}):rpc('parent_'+label+'_slot_'+key[2:],'eth_getStorageAt',[address,key,tag])
 if i%5==0 or i==len(first)-1:print(str(i+1)+'/'+str(len(first))+' Parent-State-Accounts gesichert.',flush=True)
rpc('launch_block_end_fee_account_balance','eth_getBalance',['0xfffffffffffffffffffffffffffffffffffffffe',{'blockHash':headers[124415357]['hash'],'requireCanonical':True}])
for number in [124415356,124415357,124415358]:assert rpc('recheck_'+str(number),'eth_getBlockByNumber',[hex(number),False])['hash']==headers[number]['hash']
(OUT/'sha256.json').write_text(json.dumps({p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(OUT.glob('*.response.json'))},indent=2)+'\n')
print('Read-only Capture vollständig; Raw-Antworten und Hashes gespeichert.',flush=True)

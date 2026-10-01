"""Bounded read-only archive closure; credentials held in memory, never serialized."""
import json,pathlib,getpass,urllib.request,urllib.error,hashlib,datetime,subprocess
R=pathlib.Path(__file__).resolve().parents[2];O=R/'gate0/evidence/g03_activation_raw';O.mkdir(exist_ok=True)
endpoint=getpass.getpass('Archive endpoint (hidden): ')
def rpc(name,method,params):
 assert method in {'eth_chainId','eth_getCode','eth_getBalance','eth_getTransactionCount','eth_getStorageAt','debug_traceTransaction'}
 req={'jsonrpc':'2.0','id':1,'method':method,'params':params};p=O/(name+'.response.json');q=O/(name+'.request.json')
 if p.exists():
  assert json.loads(q.read_text())==req
  d=json.loads(p.read_text());assert not d.get('error');return d['result']
 q.write_text(json.dumps(req,indent=2)+'\n')
 try:
  with urllib.request.urlopen(urllib.request.Request(endpoint,data=json.dumps(req).encode(),headers={'Content-Type':'application/json'}),timeout=45) as r:raw=r.read(40000001)
 except urllib.error.HTTPError as e:raise RuntimeError('HTTP '+str(e.code)) from None
 except urllib.error.URLError:raise RuntimeError('Network failure; endpoint redacted') from None
 assert len(raw)<=40000000
 assert endpoint.encode() not in raw and endpoint.rstrip('/').split('/')[-1].encode() not in raw
 d=json.loads(raw);p.write_bytes(raw);(O/(name+'.meta.json')).write_text(json.dumps({'sha256':hashlib.sha256(raw).hexdigest(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'provider':'QuickNode'},indent=2)+'\n')
 if d.get('error'):raise RuntimeError(json.dumps(d['error']))
 return d['result']
assert int(rpc('chain','eth_chainId',[]),16)==56
blockHash='0x854055a293db2a4c69d4827e00654bc3be75d0484b1f4258a8056380672d1104';tag={'blockHash':blockHash,'requireCanonical':True}
f=R/'gate0/evidence/g03_parent_closure.json';state=json.loads(f.read_text())['state'] if f.exists() else {}
for phase in ['after','before']:
 for attempt in range(40):
  f.write_text(json.dumps({'block_hash':blockHash,'state':state},indent=2)+'\n')
  r=subprocess.run(['node',str(R/'gate0/tools/probe_sequence.cjs')]+(['--before'] if phase=='before' else []),capture_output=True,text=True);assert r.returncode==0,r.stderr
  d=json.loads(r.stdout);err=d['exception'];print('Closure',phase,attempt,err,'frames',d['frames'],flush=True)
  if (phase=='after' and not err) or (phase=='before' and err=='revert'):break
  if err.startswith('UNWITNESSED_CODE '):
   a=err.split()[1];v=state.setdefault(a,{'storage':{}})
   for key,method in [('code','eth_getCode'),('balance','eth_getBalance'),('nonce','eth_getTransactionCount')]:v[key]=rpc('parent_'+a[2:]+'_'+key,method,[a,tag])
  elif err.startswith('UNWITNESSED_STORAGE '):
   _,a,s=err.split();state.setdefault(a,{'storage':{}})['storage'][s]=rpc('parent_'+a[2:]+'_slot_'+s[2:],'eth_getStorageAt',[a,s,tag])
  else:raise RuntimeError('Actual execution error after witnessed closure: '+str(err))
 else:raise RuntimeError('Bound exceeded')
for label,tx in [('358_016','0x73716f5fffab9493483d8eae7c1b170bc4d33feaa8346bb07d5c6a0d91d2595b'),('359_017','0x6f723464dbb64ba0d5e6f468b3a411161e7861baa6214dc7d38938b5a46514b6')]:
 for kind,opts in [('diff',{'tracer':'prestateTracer','tracerConfig':{'diffMode':True},'timeout':'30s'}),('calls',{'tracer':'callTracer','timeout':'30s'})]:rpc(label+'_'+kind,'debug_traceTransaction',[tx,opts])
(O/'sha256.json').write_text(json.dumps({p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(O.glob('*.response.json'))},indent=2)+'\n')
print('Read-only closure and candidate transitions frozen.',flush=True)

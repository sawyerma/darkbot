"""Read-only call traces for the three frozen Flap transactions only."""
import argparse,json,urllib.request,pathlib,hashlib,time
import read_rpc
p=argparse.ArgumentParser();p.add_argument('--url',required=True);p.add_argument('--out',required=True);p.add_argument('--single',action='store_true');a=p.parse_args()
read_rpc.ALLOWED=frozenset({'debug_traceTransaction'})
hashes=['0x5b8cc9893c403a66d4f43bd73ed5c69227807661a58d1d6b5611def3f1d8352c','0x189585bec2fbfac6d2ff03455e1137b6c938bd17f495895e4e076b808d926800','0x8aff738f6fff1302e057609661acf39eb2ebbf8ac13f1bd69dbc0835bb570f6b']
if a.single:
 out=pathlib.Path(a.out);out.mkdir(exist_ok=False);r=[]
 for i,h in enumerate(hashes,1):
  req=dict(jsonrpc='2.0',id=i,method='debug_traceTransaction',params=[h,{'tracer':'callTracer','timeout':'10s'}])
  stem=out/f'{i:03d}';stem.with_suffix('.request.json').write_text(json.dumps(req,indent=2)+'\n')
  try:
   q=urllib.request.Request(a.url,data=json.dumps(req).encode(),headers={'Content-Type':'application/json','User-Agent':'DarkBot-G02-readonly/1'})
   with urllib.request.urlopen(q,timeout=25) as response: raw=response.read(40000000)
   stem.with_suffix('.response.json').write_bytes(raw)
   stem.with_suffix('.meta.json').write_text(json.dumps(dict(endpoint=a.url,sha256=hashlib.sha256(raw).hexdigest()))+'\n')
   r.append(json.loads(raw))
  except Exception as e:
   stem.with_suffix('.error.txt').write_text(str(e)+'\n');r.append(dict(id=i,error=str(e)))
else:
 r=read_rpc.Recorder(a.url,a.out).batch([('debug_traceTransaction',[h,{'tracer':'callTracer','timeout':'10s'}]) for h in hashes])
print(json.dumps([{'id':x['id'],'error':x.get('error'),'result_present':'result' in x} for x in r],indent=2))

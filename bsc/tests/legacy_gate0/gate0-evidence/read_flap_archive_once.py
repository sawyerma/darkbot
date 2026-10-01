"""One read-only historical BUY call-trace probe. Preserves HTTP error bodies."""
import argparse,datetime,hashlib,json,pathlib,urllib.request,urllib.error
p=argparse.ArgumentParser();p.add_argument('--url',required=True);p.add_argument('--out',required=True);a=p.parse_args()
out=pathlib.Path(a.out);out.mkdir(exist_ok=False)
request={'jsonrpc':'2.0','id':1,'method':'debug_traceTransaction','params':['0x189585bec2fbfac6d2ff03455e1137b6c938bd17f495895e4e076b808d926800',{'tracer':'callTracer','timeout':'20s'}]}
(out/'request.json').write_text(json.dumps(request,indent=2)+'\n')
meta={'endpoint':a.url,'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'purpose':'historical execution evidence; no send/sign'}
req=urllib.request.Request(a.url,data=json.dumps(request).encode(),headers={'Content-Type':'application/json','User-Agent':'DarkBot-ArchiveRead/1'})
try:
 with urllib.request.urlopen(req,timeout=25) as r:raw=r.read(40000001);meta['http_status']=r.status
 if len(raw)>40000000:raise ValueError('Response size bound exceeded')
 (out/'response.json').write_bytes(raw);data=json.loads(raw);meta['rpc_error']=data.get('error');meta['trace_present']=isinstance(data.get('result'),dict)
 if isinstance(data.get('result'),dict):meta['top_level_to']=data['result'].get('to')
 meta['sha256']=hashlib.sha256(raw).hexdigest()
except urllib.error.HTTPError as e:
 raw=e.read(1000000);(out/'http_error.body').write_bytes(raw);meta.update(http_status=e.code,error=str(e),sha256=hashlib.sha256(raw).hexdigest(),body_preview=raw.decode(errors='replace')[:1500])
except Exception as e:meta['error']=str(e)
(out/'result.json').write_text(json.dumps(meta,indent=2)+'\n')
print(json.dumps(meta,indent=2))

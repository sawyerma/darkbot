"""Read-only archive capability probes for the frozen historical Flap BUY."""
import argparse,concurrent.futures,datetime,hashlib,json,pathlib,time,urllib.request,urllib.error
parser=argparse.ArgumentParser();parser.add_argument('--out',default='2026-09-28-g02-archive-probe');args=parser.parse_args()
P=pathlib.Path(__file__).parent/args.out
P.mkdir(exist_ok=False)
TX='0x189585bec2fbfac6d2ff03455e1137b6c938bd17f495895e4e076b808d926800'
CALLS=[('eth_chainId',[]),('eth_getCode',['0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0',hex(124415356)]),('debug_traceTransaction',[TX,{'tracer':'callTracer','timeout':'10s'}])]
ENDPOINTS={'nariox':'https://bsc-dataseed.nariox.org','bnb_public':'https://bsc-dataseed-public.bnbchain.org'}
def probe(item):
 name,url=item;out=P/name;out.mkdir();rows=[]
 for i,(method,params) in enumerate(CALLS,1):
  req=dict(jsonrpc='2.0',id=i,method=method,params=params)
  (out/f'{i:03d}.request.json').write_text(json.dumps(req,indent=2)+'\n')
  started=datetime.datetime.now(datetime.timezone.utc).isoformat();t=time.monotonic_ns()
  row=dict(method=method,started_utc=started)
  try:
   request=urllib.request.Request(url,data=json.dumps(req).encode(),headers={'Content-Type':'application/json','User-Agent':'DarkBot-G02-ArchiveRead/1'})
   with urllib.request.urlopen(request,timeout=20) as response:raw=response.read(40000001);status=response.status
   if len(raw)>40000000:raise ValueError('Evidence size bound exceeded')
   (out/f'{i:03d}.response.json').write_bytes(raw);data=json.loads(raw)
   row.update(http_status=status,response_sha256=hashlib.sha256(raw).hexdigest(),rpc_error=data.get('error'))
   if data.get('id')!=i:row['identity_error']='Unexpected response ID'
   elif 'error' not in data:
    result=data.get('result');row['result_present']=result is not None
    if method=='eth_chainId':row['chain_id']=int(result,16)
    elif method=='eth_getCode':row['code_bytes']=len(bytes.fromhex(result[2:]))
    else:row['trace_type']=type(result).__name__;row['trace_error']=result.get('error') if isinstance(result,dict) else None
  except urllib.error.HTTPError as e:
   raw=e.read(1000000);(out/f'{i:03d}.http_error.body').write_bytes(raw);row.update(http_error=e.code,error=str(e))
  except Exception as e:row['error']=str(e)
  row['diagnostic_elapsed_ns']=time.monotonic_ns()-t;rows.append(row)
  if i==1 and row.get('chain_id')!=56:break
  if i==2 and 'nodename nor servname' in row.get('error',''):break
  (out/f'{i:03d}.meta.json').write_text(json.dumps(row,indent=2)+'\n')
 result=dict(endpoint=url,calls=rows,usable=False)
 if len(rows)==3:result['usable']=rows[0].get('chain_id')==56 and rows[1].get('code_bytes',0)>0 and rows[2].get('result_present',False) and not rows[2].get('trace_error')
 (out/'summary.json').write_text(json.dumps(result,indent=2)+'\n');return result
rows=list(concurrent.futures.ThreadPoolExecutor(max_workers=2).map(probe,ENDPOINTS.items()))
(P/'summary.json').write_text(json.dumps(rows,indent=2)+'\n')
print(json.dumps(rows,indent=2))

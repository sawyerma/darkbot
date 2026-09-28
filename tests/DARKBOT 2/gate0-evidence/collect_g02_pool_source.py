"""Bounded public contract/source downloads for offline G0.2 analysis."""
import concurrent.futures, hashlib, json, pathlib, urllib.request
OUT=pathlib.Path(__file__).parent/'2026-09-28-g02-economics'/'sources'
OUT.mkdir(exist_ok=True)
urls={'IPancakeV3PoolEvents.sol':'https://raw.githubusercontent.com/pancakeswap/pancake-v3-contracts/main/projects/v3-core/contracts/interfaces/pool/IPancakeV3PoolEvents.sol'}

def get(item):
 name,url=item
 try:
  req=urllib.request.Request(url,headers={'User-Agent':'DarkBot-G02-readonly/1'})
  with urllib.request.urlopen(req,timeout=25) as r: raw=r.read(12000000);status=r.status
  (OUT/name).write_bytes(raw)
  result=dict(file=name,url=url,status=status,bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
 except Exception as e: result=dict(file=name,url=url,error=str(e))
 return result
results=list(concurrent.futures.ThreadPoolExecutor(max_workers=4).map(get,urls.items()))
(OUT/'pancake_manifest.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results,indent=2))

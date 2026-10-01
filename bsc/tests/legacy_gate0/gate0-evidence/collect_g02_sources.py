"""Bounded public contract/source downloads for offline G0.2 analysis."""
import concurrent.futures, hashlib, json, pathlib, urllib.request
OUT=pathlib.Path(__file__).parent/'2026-09-28-g02-economics'/'sources'
OUT.mkdir(exist_ok=True)
urls={
 'router_bscscan.html':'https://bscscan.com/address/0x43dd5eebeebead726fbc84e2511567d9827ae1bb#code',
 'portal_impl_bscscan.html':'https://bscscan.com/address/0x27a0f9da7f5bead616fe0bf02bf0c793bece4e20#code',
 'router_sourcify.json':'https://sourcify.dev/server/v2/contract/56/0x43dd5eebeebead726fbc84e2511567d9827ae1bb?fields=all',
 'portal_sourcify.json':'https://sourcify.dev/server/v2/contract/56/0x27a0f9da7f5bead616fe0bf02bf0c793bece4e20?fields=all',
 'portal_source.sol':'https://raw.githubusercontent.com/incjanta/Portal/bed46ef0/src/Portal.sol',
 'official_IPortal.sol':'https://raw.githubusercontent.com/flap-sh/FlapVaultExample/5949cc7eb99bcb5ac5f679cc710ae456e627f12a/src/flap/IPortal.sol',
}
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
(OUT/'manifest.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results,indent=2))

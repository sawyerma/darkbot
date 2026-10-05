"""Read-only dispatch probe, unsigned simulation; intentionally no venue accounts.
An account error proves dispatch recognition, NOT a successful refresh.
No private key or sendTransaction call.
"""
import json,urllib.request,base64,hashlib,os,sys
from pathlib import Path
A='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
def dec(s):
 n=0
 for c in s:n=n*58+A.index(c)
 return b'\0'*(len(s)-len(s.lstrip('1')))+n.to_bytes((n.bit_length()+7)//8,'big')
def rpc(m,p):
 req=urllib.request.Request(os.environ.get('P0_RPC_URL','https://api.mainnet-beta.solana.com'),json.dumps({'jsonrpc':'2.0','id':1,'method':m,'params':p}).encode(),{'Content-Type':'application/json'})
 return json.load(urllib.request.urlopen(req,timeout=30))
h=rpc('getLatestBlockhash',[{'commitment':'confirmed'}])['result']['value']['blockhash']
payer='GqRxBgvH94QKFJQVyDv82fK6jxaKrcyjyjSPkN7ue3zd'; program='dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH'
disc=hashlib.sha256(b'global:update_spot_market_cumulative_interest').digest()[:8]
message=bytes([1,0,1,2])+dec(payer)+dec(program)+dec(h)+bytes([1,1,0,8])+disc
wire=bytes([1])+bytes(64)+message
r=rpc('simulateTransaction',[base64.b64encode(wire).decode(),{'encoding':'base64','sigVerify':False,'commitment':'confirmed'}])
Path(sys.argv[1]).write_text(json.dumps({'program':program,'discriminator':list(disc),'purpose':'dispatch-only; deliberately missing venue accounts; unsigned simulation','response':r},indent=2))
print(json.dumps(r))

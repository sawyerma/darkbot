"""Decode public transaction facts. Token deltas are not silently marked USD PnL."""
import json,struct,hashlib,sys,datetime
from pathlib import Path
root=Path(sys.argv[1]); A='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
def dec(s):
 n=0
 for c in s:n=n*58+A.index(c)
 return b'\0'*(len(s)-len(s.lstrip('1')))+n.to_bytes((n.bit_length()+7)//8,'big')
names=['start_liquidation','end_liquidation','lending_account_withdraw','lending_account_repay','kamino_withdraw','drift_withdraw','juplend_withdraw','init_liquidation_record']
discs={hashlib.sha256(('global:'+n).encode()).digest()[:8]:n for n in names}
summary=json.loads((root/'history_summary.json').read_text()); rows=[]
for e in summary['events']:
 sig=e['signature'];t=json.loads((root/(sig+'.json')).read_text());m=t['meta'];msg=t['transaction']['message'];keys=msg['accountKeys']+m.get('loadedAddresses',{}).get('writable',[])+m.get('loadedAddresses',{}).get('readonly',[])
 receiver=e['events'][0]['receiver']; ownerdelta={}; token_deltas={}; ixout=[]; transfers=[];cu_limit=None;cu_price=None
 for sign,balances in [(-1,m.get('preTokenBalances',[])),(1,m.get('postTokenBalances',[]))]:
  for b in balances:
   if b.get('owner')!=receiver:continue
   mint=b['mint'];v=b['uiTokenAmount'];token_deltas[mint]=token_deltas.get(mint,0)+sign*int(v['amount']);ownerdelta[mint]=v['decimals']
 for ix in msg['instructions']:
  b=dec(ix['data']);program=keys[ix['programIdIndex']];accounts=[keys[i] for i in ix['accounts']];name=discs.get(b[:8],b[:8].hex())
  ixout.append({'program':program,'name':name,'accounts':accounts,'data_hex':b.hex(),'amount_u64':int.from_bytes(b[8:16],'little') if len(b)>=16 and name.endswith(('withdraw','repay')) else None})
  if program=='ComputeBudget111111111111111111111111111111':
   if b[0]==2:cu_limit=int.from_bytes(b[1:5],'little')
   if b[0]==3:cu_price=int.from_bytes(b[1:9],'little')
 allix=msg['instructions']+[i for block in m.get('innerInstructions',[]) for i in block['instructions']]
 for ix in allix:
  b=dec(ix.get('data',''));program=keys[ix['programIdIndex']]
  if program=='11111111111111111111111111111111' and b[:4]==b'\x02\0\0\0':transfers.append({'source':keys[ix['accounts'][0]],'destination':keys[ix['accounts'][1]],'lamports':int.from_bytes(b[4:12],'little')})
 idx=keys.index(receiver);row={**e,'timestamp_utc':datetime.datetime.fromtimestamp(t['blockTime'],datetime.timezone.utc).isoformat(),'fee_payer':keys[0],'receiver_lamport_delta':m['postBalances'][idx]-m['preBalances'][idx],'receiver_token_deltas':{k:{'native':v,'decimals':ownerdelta[k]} for k,v in token_deltas.items()},'cu_limit':cu_limit,'cu_price_micro_lamports':cu_price,'priority_fee_from_explicit_budget':(cu_limit*cu_price+999999)//1000000 if cu_limit is not None and cu_price is not None else None,'system_transfers':transfers,'instructions':ixout,'complete_realized_usd_pnl':None,'historical_darkbot_classification':'UNKNOWN','historical_prestate_missing':True}
 rows.append(row)
 print(sig,row['timestamp_utc'],'fee',m['fee'],'lamports',row['receiver_lamport_delta'],'tokens',row['receiver_token_deltas'])
 print('ix',[(x['name'],x['amount_u64'],x['accounts'][:4]) for x in ixout if x['name'] in names]);print('transfers',transfers)
(root/'history_analysis.json').write_text(json.dumps(rows,indent=2))

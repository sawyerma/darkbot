"""Offline verification of the canonical 65-TX interval and relevant state replay."""
import copy,hashlib,json,pathlib
ROOT=pathlib.Path(__file__).resolve().parent
P=ROOT/'2026-09-28-g02-archive-witness-quicknode-153754';RAW=P/'intervening_raw'
def read(p):return json.loads(p.read_text())
def write(name,obj):(P/name).write_text(json.dumps(obj,indent=2)+'\n')
def num(v):return int(v,16) if isinstance(v,str) else v
def slot(n):return '0x'+format(n,'064x')
def main():
 hashes=read(RAW/'sha256.json')
 def result(name):
  p=RAW/(name+'.response.json');assert hashlib.sha256(p.read_bytes()).hexdigest()==hashes[p.name]
  d=read(p);assert not d.get('error');return d['result']
 prefix=read(ROOT/'2026-09-28-g02-archive-witness/transaction_prefix.json')
 first=read(P/'first_buy_prestate.json')['result']
 six_hashes=read(P/'sha256.json')
 for name,digest in six_hashes.items():assert hashlib.sha256((P/name).read_bytes()).hexdigest()==digest
 target={a:set(x.get('storage',{})) for a,x in first.items()}
 scope=set(first)
 for role in ['launch','direct_buy']:scope.update(read(P/(role+'_prestate.json'))['result'])
 headers={n:result('block_'+str(n)) for n in [124415356,124415357,124415358]}
 assert num(result('chain_id'))==56
 for n,b in headers.items():
  assert num(b['number'])==n
  assert result('recheck_'+str(n))['hash']==b['hash']
 assert headers[124415357]['parentHash']==headers[124415356]['hash']
 assert headers[124415358]['parentHash']==headers[124415357]['hash']
 assert len(headers[124415357]['transactions'])==51
 assert len(prefix)==67 and len([x for x in prefix if x['role']=='intervening'])==65
 for tx in prefix:
  b=headers[tx['block_number']];assert b['hash']==tx['block_hash'];assert b['transactions'][tx['tx_index']]==tx['tx_hash']
 state={}
 for address,account in first.items():
  stem='parent_'+address[2:]
  state[address]={field:result(stem+'_'+field) for field in ['code','balance','nonce']}
  state[address]['storage']={key:result(stem+'_slot_'+key[2:]) for key in account.get('storage',{})}
  for reqpath in RAW.glob(stem+'*.request.json'):
   req=read(reqpath);assert req['params'][0]==address
   assert req['params'][-1]=={'blockHash':headers[124415356]['hash'],'requireCanonical':True}
 initial=copy.deepcopy(state)
 rows=[];prechecks=0;boundary_adjustments=[]
 for tx in prefix[:-1]:
  if tx['block_number']==124415358 and tx['tx_index']==0:
   address='0xfffffffffffffffffffffffffffffffffffffffe'
   old=state[address]['balance'];new=result('launch_block_end_fee_account_balance')
   boundary_adjustments.append({'account':address,'field':'balance','after_last_traced_tx':old,'canonical_block_end':new,
    'authority':'launch_block_end_fee_account_balance.response.json','cause':'Difference outside captured per-TX diffs; protocol accounting explanation not independently replayed',
    'quote_input_effect':'None: native balance of this account is not a curve/pool/config input'})
   state[address]['balance']=new
  name='diff_'+str(tx['block_number'])+'_'+str(tx['tx_index']).zfill(3);d=result(name)
  request=read(RAW/(name+'.request.json'))
  assert request['method']=='debug_traceTransaction' and request['params'][0]==tx['tx_hash']
  assert request['params'][1]['tracer']=='prestateTracer' and request['params'][1]['tracerConfig']=={'diffMode':True}
  assert set(d)=={'pre','post'}
  changes=[]
  for address in sorted(set(d['pre'])|set(d['post'])):
   before=d['pre'].get(address,{});after=d['post'].get(address,{})
   created=address not in d['pre'];deleted=address not in d['post']
   local=state.get(address)
   if local:
    for field,value in before.items():
     if field=='storage':
      for key,v in value.items():
       if key in target[address]:assert num(local['storage'][key])==num(v),(tx['tx_hash'],address,key,'prestate mismatch');prechecks+=1
     elif field in ['balance','nonce','code']:
      assert (local[field].lower()==value.lower() if field=='code' else num(local[field])==num(value)),(tx['tx_hash'],address,field,'prestate mismatch')
      prechecks+=1
   for field in ['code','balance','nonce']:
    empty='0x' if field=='code' else '0x0'
    old=before.get(field,empty)
    new=empty if deleted else after.get(field,old)
    same=(old==new if field=='code' else num(old)==num(new))
    if same:continue
    # Only actual CALL execution/state influences are claimed. Nonce and incidental
    # recipient balances are retained in the diff but do not change curve arithmetic.
    relevant=address in target and field=='code'
    changes.append({'contract':address,'field':field,'slot':None,'before':old,'after':new,
                    'in_launch_first_direct_dependency_accounts':address in scope,
                    'first_buy_read_dependency':address in target,'buy_quote_relevant':relevant,
                    'effect':'Runtime code change: recompute semantics' if relevant else 'Account bookkeeping/native value; not a curve-input mutation'})
    if local:local[field]=new
   for key in sorted(set(before.get('storage',{}))|set(after.get('storage',{}))):
    old=before.get('storage',{}).get(key,slot(0));new=after.get('storage',{}).get(key,slot(0))
    if num(old)==num(new):continue
    relevant=key in target.get(address,set())
    changes.append({'contract':address,'field':'storage','slot':key,'before':old,'after':new,
                    'in_launch_first_direct_dependency_accounts':address in scope,
                    'first_buy_read_dependency':relevant,'buy_quote_relevant':relevant,
                    'effect':'Update witnessed first-BUY read slot and rerun quote' if relevant else 'Slot not read by historical first-BUY execution; no effect on its frozen-path quote'})
    if local and relevant:
     if created:assert num(local['storage'][key])==0
     # Geth diffMode: pre-only slot was cleared; post-only slot was zero before.
     if key not in before.get('storage',{}):assert num(local['storage'][key])==0,(tx['tx_hash'],address,key,'nonzero new slot')
     local['storage'][key]=new
   if local and deleted:
    local.update(code='0x',balance='0x0',nonce='0x0');local['storage']={k:slot(0) for k in target[address]}
  relevant=[x for x in changes if x['buy_quote_relevant']]
  rows.append({**tx,'raw_state_diff':str((RAW/(name+'.response.json')).relative_to(ROOT)),
               'all_changed_accounts':sorted(set(d['pre'])|set(d['post'])),
               'affected_contracts':sorted({x['contract'] for x in changes}),
               'buy_relevant':bool(relevant),'first_buy_quote_influence':'RECOMPUTE' if relevant else 'NONE_FOR_OBSERVED_READ_SET',
               'changes':changes,'relevant_change_count':len(relevant),
               'classification_scope':'Complete committed diff; relevance to actual first-BUY read/code dependencies, not all hypothetical future modes'})
  if tx['role']=='launch':post_launch=copy.deepcopy(state)
 checks=[]
 for address,account in first.items():
  for field in ['code','balance','nonce']:
   expected=account.get(field,'0x' if field=='code' else 0)
   actual=state[address][field]
   same=actual==expected if field=='code' else num(actual)==num(expected)
   assert same,(address,field,'final state mismatch',actual,expected)
   checks.append({'address':address,'field':field,'match':True})
  for key,expected in account.get('storage',{}).items():
   assert num(state[address]['storage'][key])==num(expected),(address,key,'final storage mismatch')
   checks.append({'address':address,'field':'storage','slot':key,'match':True})
 intervening=[x for x in rows if x['role']=='intervening'];assert len(intervening)==65
 output={'status':'ALL_65_COMMITTED_DIFFS_REVIEWED','scope':'Historical first-BUY executed path; does not independently establish all pending initialization/extension semantics',
  'count':65,'buy_relevant_count':sum(x['buy_relevant'] for x in intervening),'transactions':intervening,
  'unknown_transaction_classifications':0,'parent_block_hash':headers[124415356]['hash'],
  'prestate_consistency_checks':prechecks,'final_accounts_matched':len(first),'final_slots_matched':sum(map(len,target.values())),
  'block_boundary_adjustments':boundary_adjustments,
  'final_state_checks':checks,'launch_diff':rows[0],
  'baseline_first_buy_read_set':initial,'post_launch_first_buy_read_set':post_launch,'replayed_first_buy_prestate':state,
  'notes':['Pre/post diff records committed changes; transient reverted writes are not persisted and cannot alter later TX state.',
           'All changed native balances/nonces are recorded separately; they are not equated with curve input changes.',
           'Matching actual first-BUY state is not by itself a pending-only initialization proof.']}
 write('intervening_transactions.json',output)
 write('replayed_first_buy_prestate.json',{'jsonrpc':'2.0','id':1,'result':state})
 print(json.dumps({k:output[k] for k in ['status','count','buy_relevant_count','prestate_consistency_checks','final_accounts_matched','final_slots_matched']},indent=2))
 return output
if __name__=='__main__':main()

"""Offline G0.2 audit. Exit 0 = full gate PASS, 2 = evidence PARTIAL, 1 = failed check.
No network, private keys, transaction construction, signing or submission.
"""
import hashlib,json,pathlib,subprocess,sys
ROOT=pathlib.Path(__file__).resolve().parent
P=ROOT/'2026-09-28-g02-economics'
OLD=ROOT/'2026-09-27-g01'
def read(p):return json.loads(p.read_text())
def write(name,obj):(P/name).write_text(json.dumps(obj,indent=2)+'\n')
def hxint(x):return int(x,16)

def main():
 subprocess.run(['node',str(ROOT/'decode_flap_economics.cjs')],check=True,stdout=subprocess.DEVNULL)
 abi=read(P/'abi_decode.json');receipts=read(P/'decoded_receipts.json');timeline=read(P/'decoded_timeline.json')
 rpc={r['id']:r for r in read(P/'state_rpc/001.response.json')}
 assert abi['version']=='6.15.0'
 assert rpc[5]['result'][:10]==abi['swap_selector']=='0xef7ec2e7'
 assert abi['create2']=='0xabe1fe16cf6bc48c9524713054efe6e954b17777'
 assert int(rpc[1]['result'],16)==56 and int(rpc[4]['result'],16)==8
 portal=abi['direct']['to'].lower();token=abi['create2'];doge='0xba2ae424d960c26247dd6c32edc70b295c744c43'
 def logs(r,name):return [l for l in r['decoded_logs'] if l['decoded'] and l['decoded']['name']==name]
 def ev(r,name):return [l['decoded']['args'] for l in logs(r,name) if l['address'].lower()==portal]
 def transfers(r,asset):return [l['decoded']['args'] for l in logs(r,'Transfer') if l['address'].lower()==asset]
 for key in ['router','direct']:
  tx=abi[key];r=receipts[key]
  assert tx['hash']==r['transactionHash'] and tx['blockHash']==r['blockHash'] and r['status']=='0x1'
  assert tx['to'].lower()==r['to'].lower() and tx['from'].lower()==r['from'].lower()
  assert hxint(tx['gasPrice'])==hxint(r['effectiveGasPrice'])
 c=ev(receipts['launch'],'TokenCurveSetV2')[0];r,h,k=(int(c[x]) for x in ['r','h','k'])
 threshold=int(ev(receipts['launch'],'TokenDexSupplyThreshSet')[0]['dexSupplyThresh'])
 def supply(reserve):
  denom=r+reserve*10**10
  return 10**27+h-(k*10**18+denom-1)//denom
 assert supply(0)==0
 reserve=0;circulating=0;steps=[]
 for tx in timeline:
  bs=[b for b in ev(tx,'TokenBought') if b['token'].lower()==token]
  if not bs:continue
  assert len(bs)==1
  b=bs[0];q=int(b['eth']);fee=int(b['fee']);tax=sum(int(b['amount']) for b in ev(tx,'TaxV2OnBondingCurvePaid') if b['token'].lower()==token)
  ds=transfers(tx,doge)
  received=sum(int(t['value']) for t in ds if t['to'].lower()==portal)
  paid=sum(int(t['value']) for t in ds if t['from'].lower()==portal)
  assert received==q and paid==fee+tax
  before=reserve;reserve+=q-fee-tax
  predicted=supply(reserve)-circulating;actual=int(b['amount'])
  assert predicted==actual
  post=int(ev(tx,'FlapTokenCirculatingSupplyChanged')[0]['newSupply'])
  assert post==circulating+actual==supply(reserve)
  virtual=[l['decoded']['args'] for l in logs(tx,'Sync') if l['address']=='0x3a632ecc01a45b8002d76c15e172e71c79412003'][-1]
  assert int(virtual['reserve0'])==10**27+h-post
  assert int(virtual['reserve1'])==r//10**10+reserve
  simple_floor_prediction=supply(before+q-2*(q//100))-circulating
  steps.append(dict(tx=tx['transactionHash'],quote_input=q,protocol_fee_observed=fee,tax_observed=tax,
                    fee_floor_100bps=q//100,tax_minus_floor=tax-q//100,post_reserve=reserve,post_circulating=post,
                    actual_output=actual,conditional_expected_output=predicted,conditional_difference=predicted-actual,
                    naive_two_floor_fees_difference=simple_floor_prediction-actual,
                    authority='Conditional on observed r/h/k and actual fee/tax transfers; NOT pending-only proof'))
  circulating=post
 assert len(steps)==7 and steps[1]['tx']==abi['router']['hash'] and steps[5]['tx']==abi['direct']['hash']
 # Final recipient is independently established through ERC20 Transfer logs.
 router=abi['router']['to'].lower();buyer=abi['router']['from'].lower()
 rt=transfers(receipts['router'],token)
 incoming=sum(int(t['value']) for t in rt if t['from'].lower()==portal and t['to'].lower()==router)
 outgoing=sum(int(t['value']) for t in rt if t['from'].lower()==router and t['to'].lower()==buyer)
 assert incoming==outgoing==steps[1]['actual_output']
 direct_received=sum(int(t['value']) for t in transfers(receipts['direct'],token) if t['from'].lower()==portal and t['to'].lower()==abi['direct']['from'].lower())
 assert direct_received==steps[5]['actual_output']
 assert int(abi['direct']['decoded']['inputAmount'])==hxint(abi['direct']['value'])==150000000000000000
 assert int(abi['direct']['decoded']['minOutputAmount'])<=direct_received
 # Router V2 arithmetic: state reconstructed from this receipt, NOT an independent pre-state.
 pair='0xac109c8025f272414fd9e2faa805a583708a017f'
 sw=next(l['decoded']['args'] for l in logs(receipts['router'],'Swap') if l['address']==pair)
 sync=next(l['decoded']['args'] for l in logs(receipts['router'],'Sync') if l['address']==pair)
 amount_in=int(sw['amount1In']);amount_out=int(sw['amount0Out'])
 before0=int(sync['reserve0'])+amount_out;before1=int(sync['reserve1'])-amount_in
 path_hex=abi['router']['segments'][2]['words'][7][2:]+abi['router']['segments'][2]['words'][8][2:]
 path_bytes=bytes.fromhex(path_hex)[:43]
 multiplier=int.from_bytes(path_bytes[20:23],'big')
 assert multiplier==9975 and path_bytes[:20].hex()=='bb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c'
 assert path_bytes[23:].hex()==doge[2:]
 v2_expected=amount_in*multiplier*before0//(before1*10000+amount_in*multiplier)
 assert v2_expected==amount_out==steps[1]['quote_input']
 native=hxint(abi['router']['value']);router_fee=native-amount_in
 assert native==30000000000000000 and router_fee==150000000000000
 # This fee amount also exists independently in the router's first fee-event payload.
 first_data=receipts['router']['logs'][0]['data'];assert hxint(first_data)==router_fee
 state=dict(token=token,quote_token=doge,quote_decimals=8,launch_quote=steps[0]['quote_input'],
            reserve_after_launch=steps[0]['post_reserve'],circulating_after_launch=steps[0]['post_circulating'],
            r=r,h=h,k=k,dex_threshold=threshold,launch_params=abi['launch']['params'],
            reconstruction='EXACT conditional on historical r/h/k and fee/tax amounts from launch logs',
            pending_only_derivation='NOT_PROVEN',extension_semantics='UNKNOWN',historical_fee_exemption='UNKNOWN')
 write('initial_state_reconstruction.json',state)
 v2=dict(pair=pair,input_wei=amount_in,pool_fee_multiplier_from_calldata=multiplier,
         inferred_pre_reserve_doge=before0,inferred_pre_reserve_wbnb=before1,
         conditional_output=v2_expected,actual_output=amount_out,difference=0,
         independent_prestate=False,limitation='Pre-reserves use target Swap output; arithmetic consistency only',
         router_native_fee=router_fee,pool_fee_wei_under_9975_model=amount_in*25//10000)
 qwords=[int(rpc[14]['result'][i:i+64],16) for i in range(2,len(rpc[14]['result']),64)]
 assert qwords==[1,41,41,7,0]
 costs={key:dict(msg_value_wei=hxint(abi[key]['value']),gas_used=hxint(receipts[key]['gasUsed']),
         effective_gas_price_wei=hxint(receipts[key]['effectiveGasPrice']),gas_cost_wei=int(receipts[key]['gas_cost_wei']),
         gross_value_plus_gas_wei=hxint(abi[key]['value'])+int(receipts[key]['gas_cost_wei']),
         net_native_debit='UNKNOWN until internal value/refund trace checked') for key in ['router','direct']}
 v3=next(l['decoded']['args'] for l in logs(receipts['direct'],'Swap') if l['address']=='0xce6160bb594fc055c943f59de92cee30b8c6b32c')
 assert int(v3['amount0'])==-steps[5]['quote_input'] and int(v3['amount1'])==hxint(abi['direct']['value'])
 pool_fee={x['id']:int(x['result'],16) for x in read(P/'pool_fee_rpc_retry/001.response.json')}
 matrix=read(P/'buy_field_matrix.json')
 assert len(matrix)>=14 and all(all(k in x for k in ['source','earliest','signature_bound','classification','signal_time_residual','invalidation']) for x in matrix)
 write('economics_reproduction.json',dict(full_pending_only_expected_output=None,full_difference=None,
       status='PARTIAL',conditional_curve_steps=steps,router_v2=v2,costs=costs,
       direct_v3_event=v3,direct_pool_fee_latest_ppm=pool_fee[1],direct_pool_tick_spacing_latest=pool_fee[2],
       router_final_recipient=buyer,router_recipient_output=outgoing,direct_recipient_output=direct_received,
       current_quote_config_words=qwords,current_quote_config_mode_7_semantics='UNRESOLVED; saved interface defines 0..6',
       direct_internal_conversion='DOGE inflow from ce6160... observed; complete V3 state/ticks/fee-mode not reproduced'))
 tests=[
  [1,'FAIL','Full router command semantics/internal Portal call not traced; envelope and transfer chain verified'],
  [2,'FAIL','Direct ABI, selector, output and visible flows PASS; complete fee/tax/conversion semantics incomplete'],
  [3,'PASS','Comparison records verified differences and explicit unknowns; no unsupported winner'],
  [4,'FAIL','Historical state numerically reconstructed; pending-only r/h/k/config/extension provenance missing'],
  [5,'FAIL','Router V2 arithmetic consistent; prestate not independent; direct conversion unresolved'],
  [6,'FAIL','Visible charges reconciled; fee rounding/exemption semantics not completely proven'],
  [7,'FAIL','Seven conditional outputs match exactly; full independent pending-only first-BUY reproduction missing'],
  [8,'PASS','minOutputAmount classified UNKNOWN with exact missing dependencies'],
  [9,'PASS','Field matrix records sources, signature binding, residuals and invalidation'],
  [10,'PASS','G0.2_FLAP_PARTIAL; no G0.3 authorization']]
 # Fixed evidence requirements stay false until a new audit actually proves them.
 requirements=dict(create2=True,vetted_selector=True,router_command_semantics=False,
    pending_initial_state=False,independent_conversion_prestate=False,fee_rounding_source=False,
    full_first_buy_reproduction=False,hotpath_dependencies_fully_classified=False)
 status='G0.2_FLAP_READY_FOR_G0.3' if all(requirements.values()) else 'G0.2_FLAP_PARTIAL'
 summary=dict(status=status,tests=[dict(test=t,status=s,reason=r) for t,s,r in tests],requirements=requirements,
              minOutputAmount='UNKNOWN',conditional_first_buy_difference=0,
              full_independent_first_buy_difference=None,
              next_test='Archive-backed transaction-index-correct execution witness: replay launch 124415357:0 through first BUY 124415358:15 with call tree, read prestate/code and intervening state changes; verify exact Portal selector, quote mode and fee rounding.')
 write('gate_result.json',summary)
 files=[p for p in P.rglob('*') if p.is_file() and p.name!='evidence_sha256.json' and not p.name.endswith('.stdout.json')]
 files += [ROOT/'decode_flap_economics.cjs', ROOT/'verify_flap_g02_economics.py', ROOT/'abi-tools/package-lock.json']
 files += [OLD/n for n in ['flap_fixture_rpc/001.response.json','flap_buy_order_receipts_rpc/001.response.json','flap_first_external_buy_rpc/001.response.json','flap_token_runtime_evidence.json','flap_v6_fixture_decoded.json']]
 write('evidence_sha256.json',{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(files)})
 print(json.dumps(summary,indent=2))
 return 0 if all(requirements.values()) else 2

if __name__=='__main__':
 try:sys.exit(main())
 except Exception as exc:
  print(type(exc).__name__+': '+str(exc),file=sys.stderr);raise

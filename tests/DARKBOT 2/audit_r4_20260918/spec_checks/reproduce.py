from pathlib import Path
import sys, json, copy, tempfile
from decimal import Decimal
from types import SimpleNamespace
BASE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BASE/'snapshot'))
from tests.helpers import *
from darkbot.backtest import *
from darkbot.config import *
from darkbot.execution import *
from darkbot.domain import *
from darkbot.reporting import Reporter
from darkbot.market import b58encode
from darkbot.replay import load_replay_input
out={}; cfg=base_config(); r=load_research_config(ROOT/'config/research.json')
def save():
 (BASE/'spec_checks/results.json').write_text(json.dumps(out,indent=2,default=str))
base={'variant_id':'base','split':'test','target':'1.5','entry_delay_ms':5,'exit_delay_ms':5,'fee_multiplier':'1.0','adverse_bps':0,'reject_rate':'0','portfolio_limit':1,'source_run_id':'A','bootstrap':{'ci_low':1,'blocks':30,'status':'OK'},'unresolved':0,'status':'POSITIVE_UNDER_MODEL_PENDING_STRESS'}
stress={**base,'variant_id':'stress','fee_multiplier':'2.0'}
rows=[copy.deepcopy(base)]+[{**copy.deepcopy(stress),'source_run_id':str(i),'variant_id':'SAME_STRESS'} for i in range(11)]
apply_stress_gate(rows,'1.5',11)
out['gate_duplicate_stress_across_recordings']={'base_status':rows[0]['status'],'unique_stress_scenarios':1,'required_stress_scenarios':11}
v=Variant('test','1.5',5,5,'1.0',0,1,False)
unknown=variant_status(v,'test',{'status':'OK','blocks':30,'ci_low':1},[SimpleNamespace(entry_economic_cost_lamports=None)],r,cfg)
rows=[{**copy.deepcopy(base),'status':unknown,'unresolved':1}]
for fm in r['network_fee_multipliers']:
 for ab in r['additional_adverse_price_bps']:
  for rr in ('0',r['conservative_exit_reject_rate']):
   if (fm,ab,rr)==('1.0',0,'0'): continue
   rows.append({**copy.deepcopy(base),'variant_id':f'{fm}_{ab}_{rr}','fee_multiplier':fm,'adverse_bps':ab,'reject_rate':rr})
apply_stress_gate(rows,'1.5',11)
out['gate_unknown_entry_exposure']={'pre_status':unknown,'post_status':rows[0]['status'],'unresolved':rows[0]['unresolved']}
rows=[copy.deepcopy(base),{**copy.deepcopy(stress),'bootstrap':{'ci_low':1,'blocks':30,'status':'OK'},'status':'UNQUALIFIED_COSTS'}]
apply_stress_gate(rows,'1.5',1)
out['gate_unqualified_stress']={'base_status':rows[0]['status'],'stress_status':rows[1]['status']}
save()
h=Harness();open_position(h);h.finish(T0+2_000_000)
before=load_replay_input(h.conn,h.run_id,h.cfg)
h.conn.execute("UPDATE feed_observations SET raw_payload=replace(raw_payload,'sigC','ALTERED_SIG') WHERE raw_payload IS NOT NULL")
after=load_replay_input(h.conn,h.run_id,h.cfg)
out['dataset_raw_content_change']={'hash_equal':before.dataset_hash==after.dataset_hash,'loaded_raw_changed':before.observations!=after.observations,'new_payload_contains_altered_sig':any('ALTERED_SIG' in x[-1] for x in after.observations)}
v1=Variant('v1','1.5',5,5,'1.0',0,1,False,'0');v2=Variant('v2','1.5',5,5,'2.0',300,1,False,'.10')
c1=variant_config(cfg,v1,1);c2=variant_config(cfg,v2,1)
out['stress_policy_hash_same']={'config':c1.config_hash==c2.config_hash,'cost_model':c1.cost_model_hash==c2.cost_model_hash,'quote_model':c1.quote_model_hash==c2.quote_model_hash}
# Exact run duration: last frame is not the end of the run.
h=Harness(); p=open_position(h);h.feed('helius_standard',creator_sell_frame(h,MINT_A,'cs',101),T0+2_000_000);h.finish(T0+10_000_000_000)
e=h.report()['economic']
a=attempts(h)
busy=p.close_mono_ns-a[0]['submit_mono_ns']
span=(T0+10_000_000_000)-(T0+1_000_000)
out['utilization_wrong_denominator']={'reported':e['capital_utilization_fraction'],'expected_using_recorded_end':busy/span,'busy_ns':busy,'recorded_span_ns':span}
# Correct UTC close is Sep19 00:00:05; first event came 15s after run start.
out['daily_utc_anchor_shift']={'reported':Reporter._daily([{'close_mono_ns':T0+20_000_000_000,'entry_decision_id':'D','net_pnl_lamports':123}],{'started_utc':'2026-09-18T23:59:45Z'},T0+15_000_000_000,{}),'expected_day':'2026-09-19'}
# Baseline-close selection branch lacks positive flag and never writes freeze.
rows=[{'variant_id':'best','split':'validation','target':'1.75','entry_delay_ms':5,'exit_delay_ms':5,'fee_multiplier':'1.0','adverse_bps':0,'reject_rate':'0','bootstrap':{'ci_low':100}}, {'variant_id':'base','split':'validation','target':'1.5','entry_delay_ms':5,'exit_delay_ms':5,'fee_multiplier':'1.0','adverse_bps':0,'reject_rate':'0','bootstrap':{'ci_low':95}}]
sel=select_candidate(rows)
out['baseline_preference_drops_positive_flag']={'selection':sel,'would_freeze':bool(sel.get('positive'))}
save()
# Persisted locked -> unlocked workflow trips over its own train runs before reaching freeze lookup/test.
with tempfile.TemporaryDirectory() as d:
 h=Harness(db_path=str(Path(d)/'r.sqlite'));h.auto_refresh=False
 for i in range(6):
  m=b58encode(f'z{i}'.encode().ljust(32,b'\0'));t=T0+i*4_000_000_000_000
  h.refresh_fees(t)
  h.feed('helius_standard',create_payload(m,sig=f's{i}',slot=100+i),t)
  h.feed('helius_standard',creator_sell_frame(h,m,f'c{i}',200+i),t+5_000_000)
 h.finish(T0+6*4_000_000_000_000)
 rc=copy.deepcopy(r);p=Path(d)/'research.json';p.write_text(json.dumps(rc))
 first=run_research(h.conn,'camp',h.cfg,p,Path(d)/'out1',persist=True,max_variants=2)
 # Install exactly the freeze record first locked evaluation would write with a positive candidate.
 dk=first['dataset_hash'];fk=config_hash({'campaign':'camp','dataset':dk,'kind':'POLICY_FREEZE'})
 upsert_config_snapshot(h.conn,'POLICY_FREEZE',fk,{'campaign':'camp','dataset':dk,'selected_target':'1.5','stage':'VALIDATION'},'audit',utc_iso(0))
 rc['test_split_unlocked']=True;p.write_text(json.dumps(rc))
 try:
  second=run_research(h.conn,'camp',h.cfg,p,Path(d)/'out2',persist=True,max_variants=2)
  result='SUCCEEDED'
 except Exception as e: result=f'{type(e).__name__}: {e}'
 out['persisted_freeze_unlock_workflow']={'first_run_succeeded':True,'freeze_exists':True,'unlock_result':result,'test_runs_persisted':h.conn.execute("select count(*) from research_variants where split='test'").fetchone()[0]}
save();print(json.dumps(out,indent=2,default=str))

from pathlib import Path
import sys, json, copy
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
from darkbot.replay import load_replay_input

out={}
cfg=base_config()
r=load_research_config(ROOT/'config/research.json')
# S02: all fees refreshed, but no new reserves for 590 seconds.
h=Harness(); p=open_position(h); h.advance(p.timeout_due_mono_ns)
a=attempts(h)[-1]
out['stale_reserve_timeout']={k:a[k] for k in ('status','side','proxy_age_ns','cost_evidence_status')}
out['stale_reserve_timeout']['configured_max_ns']=cfg.execution['proxy_state_max_age_ms']*10**6
# S04: missing frozen candidate is optional; known-insufficient stress block sample qualifies base.
base={'variant_id':'base','split':'test','target':'1.5','entry_delay_ms':5,'exit_delay_ms':5,'fee_multiplier':'1.0','adverse_bps':0,'reject_rate':'0','portfolio_limit':1,'source_run_id':'A','bootstrap':{'ci_low':1,'blocks':30,'status':'OK'},'unresolved':0,'status':'POSITIVE_UNDER_MODEL_PENDING_STRESS'}
stress={**base,'variant_id':'stress','fee_multiplier':'2.0','bootstrap':{'ci_low':1,'blocks':2,'status':'OK'},'status':'INSUFFICIENT_DATA'}
rows=copy.deepcopy([base,stress]);apply_stress_gate(rows,None,1)
out['gate_no_candidate_and_insufficient_stress']=[{'status':x['status'],'blocks':x['bootstrap']['blocks']} for x in rows]
# Cross-source duplicates satisfy the count despite identical stress dimension.
rows=[copy.deepcopy(base)]+[{**copy.deepcopy(stress),'source_run_id':str(i),'status':'POSITIVE_UNDER_MODEL_PENDING_STRESS'} for i in range(11)]
apply_stress_gate(rows,'1.5',11)
out['gate_duplicate_stress_across_recordings']=rows[0]['status']
# UNKNOWN possible entry exposure with no known entry cost passes variant pre-gate.
v=Variant('test','1.5',5,5,'1.0',0,1,False)
out['gate_unknown_entry_exposure']=variant_status(v,'test',{'status':'OK','blocks':30,'ci_low':1},[SimpleNamespace(entry_economic_cost_lamports=None)],r,cfg)
# Exact fee mutation not reflected in dataset hash (no rehash or integrity validation).
h=Harness();open_position(h);h.finish(T0+2_000_000)
before=load_replay_input(h.conn,h.run_id,h.cfg).dataset_hash
h.conn.execute("UPDATE fee_snapshots SET normalized_fee_components=json_set(normalized_fee_components,'$.protocol_fee_bps',1000) WHERE source_type='PROTOCOL_GLOBAL_ACCOUNT'")
after=load_replay_input(h.conn,h.run_id,h.cfg).dataset_hash
out['dataset_fee_content_change']={'hash_equal':before==after,'loaded_bps':load_replay_input(h.conn,h.run_id,h.cfg).fee_snapshots[0].normalized_fee_components.get('protocol_fee_bps')}
raw=copy.deepcopy(h.cfg.raw);raw['protocol']['create_event_tail_layouts'][0]['verification_status']='UNVERIFIED_CHANGED'
cfg2=build_runtime_config(h.cfg.path,raw)
out['dataset_decoder_layout_change_same_version']={'runtime_hash_changed':h.cfg.config_hash!=cfg2.config_hash,'dataset_hash_equal':load_replay_input(h.conn,h.run_id,h.cfg).dataset_hash==load_replay_input(h.conn,h.run_id,cfg2).dataset_hash}
# Legacy provider status captured after original cutoff / feature ordering.
h=Harness();open_position(h)
h.owner.on_feed_health('helius_standard',False,T0+1_200_000_000,'late')
h.owner.on_feed_health('existing_rpc',False,T0+1_200_000_000,'late')
h.advance(T0+3_000_000_000)
fr=[dict(x) for x in h.rows('select feature_set_id,features_json,cutoff_mono_ns,quality_status from feature_snapshots')]
out['legacy_future_gap']=[{'set':x['feature_set_id'],'cutoff_offset_ms':(x['cutoff_mono_ns']-T0)//10**6,'gap':json.loads(x['features_json'])['provider_gap_flag'],'quality':x['quality_status']} for x in fr[:2]]
out['feature_key_order']=list(json.loads(fr[0]['features_json']))[:4]
# Hold-time / last observed span fails as utilization even one failed attempt.
h=Harness(base_config(latency_model={'entry_delay_ms':20}))
h.feed('helius_standard',create_payload(),T0+1_000_000)
h.feed('helius_standard',pump_frame(h,MINT_A,20_000_000_000,'pump',101),T0+5_000_000)
h.advance(T0+30_000_000)
out['utilization_over_one']=h.report()['economic']['capital_utilization_fraction']
# Fee rule updates before fill: actual q uses new network, attempt columns remain old.
from dataclasses import replace
h=Harness(base_config(latency_model={'entry_delay_ms':20}));h.feed('helius_standard',create_payload(),T0+1_000_000)
n=h.fees.network
new=replace(n,fee_snapshot_id=n.fee_snapshot_id+'_new',received_mono_ns=T0+2_000_000,normalized_fee_components={**n.normalized_fee_components,'priority_fee_lamports':15000,'network_fee_total_lamports':20000})
h.fees.network=new;h.recorder.try_append(('fee_snapshot',(new,h.run_id)));h.owner.on_fee_snapshots([new]);h.advance(T0+30_000_000)
a=attempts(h)[0];fq=h.rows('select * from quote_snapshots where quote_snapshot_id=?',(a['fill_quote_snapshot_id'],))[0]
out['fee_attempt_columns_inconsistent']={'status':a['status'],'attempt_network_fee':a['network_fee_total_lamports'],'fill_quote_network_fee':fq['network_fee_total_lamports'],'referenced_fee_snapshot':a['network_fee_snapshot_id']}
# Stress changes costs after slippage comparison.
h=Harness();p=open_position(h);st=h.owner.reducer.states[MINT_A];curve=h.owner._curve(st)
eng=QuoteEngine(ExecutionPolicy.from_config(h.cfg),Decimal(1),1000)
expected=eng.quote_buy(curve,h.fees.basis(),h.clock.monotonic_ns(),'audit',st.coverage_state)
ev=evaluate_fill(eng,expected,curve,h.fees.basis(),h.clock.monotonic_ns(),'audit',st.coverage_state,0)
out['stress_exceeds_slippage_limit']={'status':ev.status.value,'max_input':expected.max_input_base_units,'filled_debit':ev.actual.swap_wallet_debit_lamports,'slippage_bps':cfg.execution['slippage_bps'],'adverse_bps':1000}
# stress cost hashes unchanged
v1=Variant('v1','1.5',5,5,'1.0',0,1,False,'0');v2=Variant('v2','1.5',5,5,'2.0',300,1,False,'.10')
c1=variant_config(cfg,v1,1);c2=variant_config(cfg,v2,1)
out['stress_policy_hash_same']={'config':c1.config_hash==c2.config_hash,'cost_model':c1.cost_model_hash==c2.cost_model_hash,'quote_model':c1.quote_model_hash==c2.quote_model_hash}
print(json.dumps(out,indent=2,default=str));(BASE/'spec_checks/results.json').write_text(json.dumps(out,indent=2,default=str))

import sys, json, base64
from pathlib import Path
from decimal import Decimal
from dataclasses import replace
ROOT=Path(__file__).resolve().parents[1]/'snapshot'
sys.path.insert(0,str(ROOT))
from tests.helpers import *
from darkbot.backtest import Variant, apply_stress_gate, register_variants
from darkbot.reporting import Reporter
from darkbot.replay import load_replay_input
from darkbot.domain import RawEnvelope, CostEvidence
from darkbot.market import b58decode, b58encode
out={}
# A complete second feed must still be actionable after a partial first copy.
h=Harness(); logs=create_frame_logs(MINT_A,CREATOR); partial=logs[:-1]+['Program data: '+base64.b64encode(b'UNKNOWN!junk').decode(),logs[-1]]
h.feed('helius_standard',logs_notification('same',100,partial),T0+1_000_000)
h.feed('existing_rpc',logs_notification('same',100,logs),T0+2_000_000)
out['partial_then_complete']={'positions':len(h.owner.positions),'duplicates':h.owner.counters.duplicates,'stored':[dict(r) for r in h.rows('select completeness,validation_status from feed_observations')]}
assert not h.owner.positions
# Even fully OBSERVED coverage must honor configured reserve freshness.
h=Harness(); p=open_position(h); h.advance(p.timeout_due_mono_ns)
a=attempts(h)[-1]; out['stale_timeout']={k:a[k] for k in ['status','side','proxy_age_ns','cost_evidence_status']}; out['stale_timeout']['configured_proxy_max_age_ms']=h.cfg.execution['proxy_state_max_age_ms']
assert a['status']=='FILLED' and a['proxy_age_ns']>h.cfg.execution['proxy_state_max_age_ms']*1_000_000
# A future state cannot prove a historical unknown fill.
h=Harness(base_config(latency_model={'entry_delay_ms':10})); h.feed('helius_standard',create_payload(),T0+1_000_000)
h.owner.on_feed_health('helius_standard',False,T0+5_000_000); h.owner.on_feed_health('existing_rpc',False,T0+5_000_000)
h.advance(T0+11_000_000); before=h.position().state.value
h.owner.on_feed_health('helius_standard',True,T0+20_000_000)
h.feed('helius_standard',pump_frame(h,MINT_A,1,'later',101),T0+21_000_000)
a=attempts(h)[0]; out['future_reconciliation']={'before':before,'after':h.position().state.value,**{k:a[k] for k in ['modeled_fill_mono_ns','proxy_receive_mono_ns','fill_commitment']}}
assert a['proxy_receive_mono_ns']>a['modeled_fill_mono_ns'] and h.position().state.value=='OPEN'
# Fee rule changes known before the modeled fill are ignored (frozen basis at submit).
h=Harness(base_config(latency_model={'entry_delay_ms':10})); h.feed('helius_standard',create_payload(),T0+1_000_000)
s=replace(h.fees.protocol,fee_snapshot_id='run_fee_protocol_new',received_mono_ns=T0+5_000_000,normalized_fee_components={'protocol_fee_bps':1000,'creator_fee_bps':30,'other_protocol_fee_bps':0}); h.fees.protocol=s; h.recorder.try_append(('fee_snapshot',(s,h.run_id)))
h.advance(T0+11_000_000); a=attempts(h)[0]
out['fee_change_before_fill']={k:a[k] for k in ['status','fee_snapshot_id','protocol_fee_lamports','entry_curve_input_lamports']}; out['fee_change_before_fill']['current_protocol_bps']=1000
assert a['status']=='FILLED' and a['fee_snapshot_id']=='run_fee_protocol'
# Hashing only payloads does not freeze economic replay inputs.
r1=load_replay_input(h.conn,'run',h.cfg); h.conn.execute("UPDATE fee_snapshots SET normalized_fee_components=? WHERE fee_snapshot_id='run_fee_protocol'",(json.dumps({'protocol_fee_bps':9999,'creator_fee_bps':30,'other_protocol_fee_bps':0}),)); r2=load_replay_input(h.conn,'run',h.cfg)
out['dataset_hash_fee_mutation']={'same_hash':r1.dataset_hash==r2.dataset_hash}; assert r1.dataset_hash==r2.dataset_hash
# Report qualifier has no mandatory forward/stress gates.
cfg=base_config(latency_model={'entry_delay_ms':5,'exit_delay_ms':5})
econ={'cost_evidence_counts':{'MEASURED':60},'unresolved_exposure_entry_cost_lamports':0,'resolved_intents':30,'expectancy_per_intent_lamports':100}
qual={'ineligible_by_reason':{},'canonical_gap_events':0,'positions_with_canonical_gap':[],'migration_or_curve_complete_exposures':[],'recorder_overflows':0,'internal_errors':0,'run_quality_status':'OK'}
fees=[{'source_type':x,'validation_status':'VALID'} for x in ['PROTOCOL_GLOBAL_ACCOUNT','NETWORK_FEE','RENT_EXEMPTION','PRIORITY_POLICY']]
s,_=Reporter._qualify(cfg.raw,econ,qual,fees,[],[],{'blocks':30,'ci_low':1})
out['report_no_forward_or_stress']=s.value; assert s.value=='POSITIVE_UNDER_MODEL'
# An exposed, disqualified stress sibling must not pass the qualification gate.
base={'split':'test','target':'1.5','entry_delay_ms':5,'exit_delay_ms':5,'portfolio_limit':1}
rows=[{**base,'fee_multiplier':'1.0','adverse_bps':0,'status':'POSITIVE_UNDER_MODEL_PENDING_STRESS','bootstrap':{'ci_low':10}}, {**base,'fee_multiplier':'2.0','adverse_bps':300,'status':'UNQUALIFIED_DATA_UNRESOLVED_EXPOSURE','unresolved':1,'bootstrap':{'ci_low':1}}]
apply_stress_gate(rows); out['unqualified_stress_accepted']=rows; assert rows[0]['status']=='POSITIVE_UNDER_MODEL'
# Configured independent delay arrays do not generate independent scenarios.
research=json.loads((ROOT/'config/research.json').read_text()); variants=register_variants(research,cfg)
out['research_delay_pairs']=sorted(set((v.entry_delay_ms,v.exit_delay_ms) for v in variants)); assert not any(v.entry_delay_ms!=v.exit_delay_ms for v in variants)
# Unsupported token program still admitted despite token_2022_supported=false.
raw=base_config().raw
raw['protocol']['create_event_tail_layouts']=[{'layout_id':'TEST_EXPLICIT_QUOTE','tail_fields':[['quote_mint','pubkey'],['token_program','pubkey']],'quote_mint_field':'quote_mint','implied_quote_asset':None,'verification_status':'VERIFIED_TEST','token_program':None}]
cfg=build_runtime_config(ROOT/'config/phase1.json',raw)
token2022='TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb'
h=Harness(cfg); h.feed('helius_standard',create_payload(tail=b58decode(cfg.protocol['wsol_mint'])+b58decode(token2022)),T0+1_000_000); h.advance(T0+1_000_000)
out['unsupported_token2022']={'configured_supported':cfg.execution['token_account_policy']['token_2022_supported'],'positions':len(h.owner.positions),'state':h.position().state.value}; assert h.position().state.value=='OPEN'
# Explicit foreign quote still gets persisted in SOL reserve columns.
h=Harness(cfg); foreign=b58encode(b'USDC_TEST'.ljust(32,b'\0')); h.feed('helius_standard',create_payload(tail=b58decode(foreign)+b58decode(token2022)),T0+1_000_000)
r=h.rows("select quote_scope_status,virtual_sol_reserves from market_events where event_type='CREATE'")[0]
out['foreign_quote_sol_columns']=dict(r); assert r['virtual_sol_reserves'] is not None and r['quote_scope_status']=='UNSUPPORTED_QUOTE'
# Feature key order and missing skipped launch snapshots.
h=Harness(); open_position(h); h.feed('helius_standard',create_payload(MINT_B,sig='B',slot=101),T0+2_000_000); h.advance(T0+1_100_000_000)
rows=h.rows('select mint,features_json from feature_snapshots'); out['features']={'mints':sorted(set(r['mint'] for r in rows)),'first_keys':list(json.loads(rows[0]['features_json']))[:3],'expected_first_keys':['buy_count','sell_count','buy_volume_sol']}; assert MINT_B not in {r['mint'] for r in rows}
print(json.dumps(out,indent=2,default=str))

from pathlib import Path
import sys,json,tempfile,copy
from decimal import Decimal
BASE=Path(__file__).resolve().parents[1];sys.path.insert(0,str(BASE/'snapshot'))
from tests.helpers import *
from darkbot.backtest import *
from darkbot.market import b58encode
from darkbot.replay import load_replay_input
out={}
r=load_research_config(ROOT/'config/research.json')
# Two fee-paid failures recorded in different hourly blocks become one research CI block.
h=Harness(base_config(latency_model={'entry_delay_ms':20}));h.auto_refresh=False
mints=set()
for i in range(2):
 m=b58encode(f'fail{i}'.encode().ljust(32,b'\0'));mints.add(m);t=T0+i*4_000_000_000_000
 h.refresh_fees(t);h.feed('helius_standard',create_payload(m,sig=f'C{i}',slot=100+i*2),t)
 h.feed('helius_standard',pump_frame(h,m,20_000_000_000,f'P{i}',101+i*2),t+5_000_000)
 h.advance(t+30_000_000)
h.finish(t+40_000_000)
rin=load_replay_input(h.conn,'run',h.cfg)
v=Variant('failed','1.5',20,0,'1.0',0,1,False)
res=evaluate_variant(h.conn,rin,h.cfg,v,mints,'camp',r,'train',False)
orig=[dict(x) for x in h.rows('select state,open_mono_ns,net_pnl_lamports from positions')]
out['failed_intents_bootstrap_collapsed']={'original_positions':orig,'actual_research_blocks':res['bootstrap']['blocks'],'resolved':res['resolved'],'expected_decision_blocks':2}
# Direct loop structure: two non-overlapping recordings split independently yields earlier test than later train.
block=r['block_length_ms']*1_000_000
runs={'early':[(f'A{i}',T0+i*4_000_000_000_000) for i in range(6)],'late':[(f'B{i}',T0+100_000_000_000_000+i*4_000_000_000_000) for i in range(6)]}
classified={}
for key,creates in runs.items():
 splits=chronological_split(creates,[Decimal(x) for x in r['chronological_split']],embargo_ns(h.cfg,r),block)
 classified[key]={s:[t for m,t in creates if m in chosen] for s,chosen in splits.items()}
out['campaign_chronology_leak']={'split_times':classified,'earliest_late_train':min(classified['late']['train']),'latest_early_test':max(classified['early']['test']),'test_precedes_training':max(classified['early']['test'])<min(classified['late']['train'])}
# Bounds prove fixed embargo cannot use actual unresolved exposure: function has no positions input.
print(json.dumps(out,indent=2));(BASE/'spec_checks/split_results.json').write_text(json.dumps(out,indent=2))

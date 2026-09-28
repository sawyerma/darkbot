import sys, json, tempfile
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'snapshot'))
from tests.helpers import *
from darkbot.backtest import evaluate_variant,Variant
from darkbot.replay import load_replay_input
cfg=base_config(); research=json.loads((ROOT/'config/research.json').read_text()); v=Variant('same','1.5',0,0,'1.0',0,1,False)
with tempfile.TemporaryDirectory() as d:
 db=str(Path(d)/'x.sqlite')
 h1=Harness(cfg,run_id='source1',db_path=db); open_position(h1); h1.finish(T0+3_000_000)
 h2=Harness(cfg,run_id='source2',db_path=db); open_position(h2,mint=MINT_B); h2.finish(T0+3_000_000)
 a=evaluate_variant(h2.conn,load_replay_input(h2.conn,'source1',cfg),cfg,v,{MINT_A},'camp',research,'train',True)
 b=evaluate_variant(h2.conn,load_replay_input(h2.conn,'source2',cfg),cfg,v,{MINT_B},'camp',research,'train',True)
 print(json.dumps({'source1_output_run':a['run_id'],'source2_output_run':b['run_id'],'db_runs':[dict(r) for r in h2.conn.execute("select run_id,parent_run_id from runs where mode='RESEARCH_REPLAY'")],'positions':[dict(r) for r in h2.conn.execute("select run_id,mint from positions where run_id like 'research_%'")]},indent=2))
 assert a['run_id']==b['run_id']

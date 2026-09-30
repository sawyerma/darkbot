import json,sys,time,sqlite3,threading
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]/'snapshot'
sys.path.insert(0,str(ROOT))
from tests.helpers import *
from darkbot.storage import Recorder,SqliteWriter,RowRecord
from darkbot.market import event_discriminator,b58encode
from darkbot.domain import PositionState
out={}
# Known failed tx before scheduled fill must invalidate its proxy.
h=Harness(base_config(latency_model={'entry_delay_ms':20}))
h.feed('helius_standard',create_payload(),T0+1_000_000)
h.feed('existing_rpc',logs_notification('sigC',100,[],err={'InstructionError':[0,'Custom']}),T0+5_000_000)
h.advance(T0+30_000_000)
a=attempts(h)[0]
out['reorg_before_fill']={'quote_uncertain':h.owner.reducer.states[MINT_A].quote_uncertain,'status':a['status'],'position':h.position().state.value,'gap_flag':h.position().canonical_provider_gap_flag,'proxy_signature':a['proxy_signature']}
# Unknown relevant event while sole feed is covering mint -> no coverage degradation.
h=Harness(base_config(latency_model={'entry_delay_ms':20}))
h.owner.on_feed_health('existing_rpc',False,T0,'off')
h.feed('helius_standard',create_payload(),T0+1_000_000)
unknown=[f'Program {PROGRAM} invoke [1]','Program data: '+base64.b64encode(event_discriminator('NewUnknownMarketEvent')+b'x'*40).decode(),f'Program {PROGRAM} success']
h.feed('helius_standard',logs_notification('unknown',101,unknown),T0+5_000_000)
h.advance(T0+30_000_000)
a=attempts(h)[0]
h.finish(T0+31_000_000)
out['decode_gap_before_fill']={'decode_errors':h.owner.counters.decode_errors,'status':a['status'],'coverage':h.owner.reducer.states[MINT_A].coverage_state.value,'position_gap':h.position().canonical_provider_gap_flag,'quality':[dict(r) for r in h.rows("select kind,canonical_gap from quality_events where kind='DECODE_GAP_SUMMARY'")]}
# Late feature job uses current V1 connectivity; truncation is not marked.
h=Harness(base_config(storage={'trade_log_per_mint':2}))
open_position(h)
for i in range(3):
 h.feed('helius_standard',pump_frame(h,MINT_A,10_000_000,f'buy{i}',101+i),T0+(10+i)*1_000_000)
h.owner.on_feed_health('helius_standard',False,T0+1_200_000_000,'off')
h.owner.on_feed_health('existing_rpc',False,T0+1_200_000_000,'off')
h.advance(T0+2_000_000_000)
rows=[dict(r) for r in h.rows('select feature_set_id,cutoff_mono_ns,features_json,quality_status from feature_snapshots')]
out['features']=[{'version':r['feature_set_id'],'cutoff_offset':r['cutoff_mono_ns']-(T0+1_000_000),'buy_count':json.loads(r['features_json'])['buy_count'],'provider_gap':json.loads(r['features_json'])['provider_gap_flag'],'quality':r['quality_status']} for r in rows if r['cutoff_mono_ns'] in (T0+1_000_000,T0+1_001_000_000)]
# Closed positions permanently protect their trackers even when over max.
h=Harness(base_config(storage={'max_tracked_mints':1,'inactive_mint_retention_ms':1}))
for i in range(3):
 m=_pubkey('closed'+str(i)) if '_pubkey' in globals() else b58encode(('closed'+str(i)).encode().ljust(32,b'\0'))
 h.feed('helius_standard',create_payload(m,sig=f'c{i}',slot=100+i*2),T0+(i*10+1)*1_000_000)
 h.feed('helius_standard',creator_sell_frame(h,m,f's{i}',101+i*2),T0+(i*10+2)*1_000_000)
 h.advance(T0+(i*10+3)*1_000_000)
removed=h.owner.maybe_evict(T0+10**15)
out['closed_trackers']={'states':len(h.owner.reducer.states),'configured_max':1,'removed':removed,'position_states':[p.state.value for p in h.owner.positions.values()],'slots':h.owner.occupied_slots(),'adapter_submitted_retained':len(h.adapter.submitted)}
# Missing FK target, empty queue, stop: retained batch spins forever; stop silently returns.
c=sqlite3.connect(':memory:',isolation_level=None,check_same_thread=False);c.execute('PRAGMA foreign_keys=ON');c.execute('create table parents(id text primary key)');c.execute('create table children(id text primary key,p text references parents(id) deferrable initially deferred)')
r=Recorder(10); r.try_append(RowRecord('children',{'id':'orphan','p':'never-arrives'})); errs=[]
w=SqliteWriter(c,r,1,1,errs.append,1,2);w.start();time.sleep(.01);w.stop(.05)
out['writer_orphan_fk_shutdown']={'thread_alive':w._thread.is_alive(),'failures':w.failures,'disk_error':w.disk_error,'error_callbacks':errs,'queue_depth':r.depth()}
print(json.dumps(out,indent=2))
Path(__file__).with_name('results.json').write_text(json.dumps(out,indent=2))

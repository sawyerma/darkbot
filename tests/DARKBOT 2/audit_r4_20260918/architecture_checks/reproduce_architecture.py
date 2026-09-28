from pathlib import Path
import sys, json, base64, tempfile, threading, asyncio
from dataclasses import replace
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'snapshot'))
from tests.helpers import *
from darkbot.storage import RowRecord
from darkbot.market import event_discriminator, b58encode
from darkbot.domain import PositionState
from darkbot.fees import FeeResolver
results = {}
# Unattributable PARTIAL has no bounded trim, unlike UNDECODABLE branch.
h = Harness(base_config(storage={'fee_history_max': 4}), economic=False)
line = 'Program data: '+base64.b64encode(event_discriminator('FutureEvent')+b'x'*40).decode()
for i in range(100):
    h.feed('helius_standard', logs_notification(f'u{i}', i, [f'Program {PROGRAM} invoke [1]', line, f'Program {PROGRAM} success']), T0+i*1000)
results['partial_points_bound'] = {'configured_bound': h.owner.reducer.event_points_max, 'actual_points':len(h.owner.reducer.undecodable_points)}
# A long-lived active token retains every dedup and applied signature despite state/log bounds.
h = Harness(base_config(strategy={'target_net_multiple':'100'}, storage={'state_history_per_mint':2, 'trade_log_per_mint':2,'max_tracked_mints':1}))
open_position(h)
for i in range(200):
    h.feed('helius_standard', pump_frame(h, MINT_A, 1, f't{i}', 101+i), T0+2_000_000+i*1000)
h.owner.maybe_evict(T0+3_000_000)
st=h.owner.reducer.states[MINT_A]
results['active_mint_dedup_bound']={'state_history':len(st.state_history),'trade_log':len(st.trade_log),'seen_keys':len(st.seen_keys),'applied_signatures':len(h.owner.reducer.applied_signatures),'position_state':h.position().state.value}
# Historical positions and submit handles remain after their mint trackers are correctly evicted.
h=Harness()
for i in range(30):
    mint=b58encode(f'mint{i}'.encode().ljust(32,b'\0'))
    t=T0+1_000_000+i*3_000_000
    h.feed('helius_standard',create_payload(mint=mint,sig=f'c{i}',slot=100+i*2),t)
    h.advance(t)
    h.feed('helius_standard',creator_sell_frame(h,mint,f's{i}',101+i*2),t+1_000_000)
    h.advance(t+1_000_000)
    h.flush()
end=T0+2_000_000_000_000
h.advance(end)
h.owner.maybe_evict(end)
results['retained_after_eviction']={'mint_states':len(h.owner.reducer.states),'positions':len(h.owner.positions),'closed':sum(p.state==PositionState.CLOSED for p in h.owner.positions.values()),'adapter_handles':len(h.adapter.submitted),'attempt_index':len(h.owner.attempts_by_position),'occupied_slots':h.owner.occupied_slots()}
# Config permits fee_history_max small enough to discard still-valid slow-refresh components.
h=Harness(base_config(storage={'fee_history_max':4}))
for i in range(10):
    s=replace(h.fees.network, fee_snapshot_id=f'network{i}', received_mono_ns=T0+(i+1)*1000)
    h.owner.on_fee_snapshots([s])
basis=h.owner.basis_at(T0+20_000)
results['fee_history_component_loss']={'cache_complete':h.fees.basis().complete,'history_basis_complete':basis.complete,'rent_cache_fresh':h.fees.rent.is_fresh(T0+20_000),'rent_history_present':basis.rent is not None,'history_length':len(h.owner.fee_history)}
# Verify quarantine preserves errors but clips raw records and invokes callback on writer thread.
with tempfile.TemporaryDirectory() as d:
    h=Harness()
    h.flush()
    sentinel='UNIQUE_LAST_BYTES_123456'
    q=Path(d)/'quarantine.jsonl'
    callbacks=[]
    main_tid=threading.get_ident()
    h.recorder.try_append(RowRecord('quality_events',{'quality_event_id':'big','run_id':'missing','start_mono_ns':1,'kind':'TEST','severity':'CRITICAL','scope':'RUN','details':'x'*10_000+sentinel,'canonical_gap':True}))
    w=SqliteWriter(h.conn,h.recorder,2,1,lambda msg:callbacks.append({'thread_id':threading.get_ident(),'message':msg}),locked_retries=1,fk_merge_max_batches=1,quarantine_path=q)
    w.start(); w.stop(timeout_s=2)
    data=q.read_text()
    results['quarantine']={'terminated':not w._thread.is_alive(),'quarantined_records':w.quarantined_records,'bytes':len(data),'sentinel_preserved':sentinel in data,'callback_on_owner_thread':callbacks[0]['thread_id']==main_tid,'callbacks':callbacks}
# Real background refresh publishes cache entries before publishing the same snapshots to StateOwner.
async def fee_publish_race():
    h=Harness()
    resolver=FeeResolver(h.cfg,None,h.clock,'background','test')
    resolver.cache=h.fees
    h.owner.basis_getter=resolver.cache.basis
    # getter captures this object; background mutates its component attributes.
    new_protocol=replace(h.fees.protocol,fee_snapshot_id='new-protocol',received_mono_ns=T0+10_000,normalized_fee_components={'protocol_fee_bps':1000,'creator_fee_bps':30,'other_protocol_fee_bps':0})
    entered=threading.Event(); release=threading.Event()
    resolver.resolve_protocol=lambda:new_protocol
    def block_priority():
        entered.set(); release.wait(2); return h.fees.priority_policy
    resolver.resolve_priority=block_priority
    resolver.resolve_network=lambda priority:h.fees.network
    resolver.resolve_rent=lambda:h.fees.rent
    stop=asyncio.Event()
    seen=[]
    def publish(batch):
        seen.extend(batch);h.owner.on_fee_snapshots(batch)
    task=asyncio.create_task(resolver.run_background(publish,stop))
    for _ in range(100):
        if entered.is_set():break
        await asyncio.sleep(.005)
    result={'resolver_protocol':resolver.cache.protocol.fee_snapshot_id,'owner_model_protocol':h.owner.basis_at(T0+20_000).protocol.fee_snapshot_id,'published_snapshots':len(seen)}
    stop.set();release.set();await task
    return result
results['fee_publication_race']=asyncio.run(fee_publish_race())
print(json.dumps(results,indent=2))
(ROOT/'architecture_checks'/'results.json').write_text(json.dumps(results,indent=2)+'\n')

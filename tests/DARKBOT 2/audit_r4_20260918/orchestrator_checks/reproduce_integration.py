"""Independent r4 integration counterexamples. No production edits."""
import base64
import json
import sys
from pathlib import Path

BASE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BASE / 'snapshot'))
from tests.helpers import *
from darkbot.replay import load_replay_input, run_replay
from darkbot.market import event_discriminator

out = {}
# A partial atomic transaction must not buy from the state before its bundled buy.
h = Harness()
full = create_frame_logs(MINT_A, CREATOR, initial_buy=(5_000_000_000, 100_000_000_000_000))
partial = list(full)
partial.insert(-1, 'Program data: ' + base64.b64encode(event_discriminator('UnknownSibling') + b'x'*40).decode())
h.feed('existing_rpc', logs_notification('atomic', 100, partial), T0+1_000_000)
h.feed('helius_standard', logs_notification('atomic', 100, full), T0+2_000_000)
h.advance(T0+3_000_000)
st = h.owner.reducer.states[MINT_A]
a = attempts(h)[0]
out['partial_atomic_upgrade'] = {'expected_vsol': V_SOL0+5_000_000_000, 'actual_vsol': st.virtual_sol_reserves,
    'expected_vtok': V_TOK0-100_000_000_000_000, 'actual_vtok': st.virtual_token_reserves,
    'status': a['status'], 'quote_state_event_id': a['quote_state_event_id'], 'proxy_event_id': a['proxy_event_id'],
    'stored_events': [dict(r) for r in h.rows('SELECT event_type,virtual_sol_reserves,quality_status FROM market_events ORDER BY instruction_index')]}

# A later disconnect must not retrospectively alter a fill before that disconnect.
h = Harness(base_config(latency_model={'entry_delay_ms':20}))
h.feed('helius_standard', create_payload(), T0+1_000_000)
h.advance(T0+22_000_000)
h.clock.advance_to(T0+30_000_000)
h.owner.on_feed_health('helius_standard', False, T0+30_000_000, 'closed')
h.owner.on_feed_health('existing_rpc', False, T0+30_000_000, 'closed')
h.feed('pumpportal', json.dumps({'txType':'create','mint':MINT_B,'signature':'hint'}), T0+50_000_000)
h.finish(T0+60_000_000)
rin = load_replay_input(h.conn, h.run_id, h.cfg)
replay = run_replay(h.conn, rin, h.cfg, 'replay_gap', 'camp', persist=False)
out['future_gap_changes_earlier_replay_fill'] = {'paper_state': h.position().state.value,
    'paper_fill_status': attempts(h)[0]['status'], 'paper_fill_time': attempts(h)[0]['modeled_fill_mono_ns'],
    'gap_start': T0+30_000_000, 'replay_state': next(iter(replay.owner.positions.values())).state.value,
    'replay_quality': next(iter(replay.owner.positions.values())).quality_status}

# Invalid Global discriminator remains accepted once the operator pins the layout.
from darkbot.fees import FeeResolver
from darkbot.clock import ReplayClock
cfg = base_config()
ga = dict(cfg.protocol['global_account']); ga['verification_status']='VERIFIED_AUDIT_LAYOUT'
cfg = cfg.with_overrides(protocol={'global_account':ga})
values=[(name, CREATOR if kind=='pubkey' else True if kind=='bool' else 100) for name,kind in ga['layout_after_discriminator']]
payload=b'WRONGDIS'+encode_fields(values,ga['layout_after_discriminator'])
def rpc(method, params):
    return {'context':{'slot':1},'value':{'owner':cfg.protocol['program_id'],'data':[base64.b64encode(payload).decode(),'base64']}}
snap=FeeResolver(cfg,rpc,ReplayClock('audit',T0,0),'audit','FIXTURE').resolve_protocol()
out['wrong_global_discriminator']={'status':snap.validation_status,'evidence':snap.evidence.value}

# D04's live SQL should see the creator sell the runtime already recognized.
h=Harness();open_position(h)
h.feed('helius_standard',creator_sell_frame(h,MINT_A,'cs',101),T0+2_000_000);h.advance(T0+3_000_000)
out['doctor_creator_sell_count']={'runtime_detected':h.owner.reducer.states[MINT_A].creator_sell_event_id is not None,
    'doctor_count':h.rows("SELECT COUNT(*) FROM market_events WHERE event_type='SELL' AND actor=creator")[0][0]}

text=json.dumps(out,indent=2);print(text)
Path(__file__).with_name('integration_results.json').write_text(text)

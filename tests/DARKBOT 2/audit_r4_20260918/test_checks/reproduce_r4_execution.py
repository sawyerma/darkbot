import json,sys,base64
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"snapshot"))
from tests.helpers import *
from darkbot.domain import FeeSnapshot,ExecutionOutcome
from darkbot.fees import FeeCache
from darkbot.config import build_runtime_config
R={}
def save():
 t=json.dumps(R,indent=2);Path(__file__).with_name("execution_results.json").write_text(t);print(t)
# Unknown nonempty TradeEvent tail: a new layout is still executable as SOL.
h=Harness();pos=open_position(h);st=h.owner.reducer.states[MINT_A]
logs=trade_frame_logs(MINT_A,5_000_000_000,100,True,BUYER,60_000_000_000,st.virtual_token_reserves,st.real_token_reserves,tail=b"unknown-layout-v999")
r=h.feed("helius_standard",logs_notification("unknown-tail",101,logs),T0+2_000_000);h.advance(T0+3_000_000)
R["unknown_tail"]={"completeness":r.envelope.completeness.value,"tail":r.events[0].event.extra,"state":pos.state.value,"vsol":st.virtual_sol_reserves,"attempts":[{"side":a["side"],"status":a["status"]} for a in attempts(h)]}
# Changed quote mint on same canonical TradeEvent not in dedup fingerprint.
c=base_config();raw=json.loads(json.dumps(c.raw));raw["protocol"]["events"]["TradeEvent"]["tail_layouts"]=[{"layout_id":"pinned-quote","tail_fields":[["quote_mint","pubkey"]],"quote_mint_field":"quote_mint","verification_status":"VERIFIED_TEST"}];cfg=build_runtime_config(c.path,raw)
h=Harness(cfg);pos=open_position(h);st=h.owner.reducer.states[MINT_A]
def quote_copy(quote):
 tail=encode_fields([("quote_mint",quote)],[["quote_mint","pubkey"]]);return logs_notification("same-event",101,trade_frame_logs(MINT_A,1,1,True,BUYER,st.virtual_sol_reserves,st.virtual_token_reserves,st.real_token_reserves,tail=tail))
h.feed("helius_standard",quote_copy(cfg.protocol["wsol_mint"]),T0+2_000_000)
r=h.feed("existing_rpc",quote_copy(BUYER2),T0+3_000_000)
R["duplicate_changed_quote"]={"validation":r.events[0].validation.value,"quote_uncertain":st.quote_uncertain,"quote_scope":st.quote_scope_status.value,"position":pos.state.value}
# Ordinary CONTENT_MISMATCH on an OPEN mint does not touch it, thus does not set UNRESOLVED.
h=Harness();pos=open_position(h);r=h.feed("existing_rpc",create_payload(vsol=31_000_000_000),T0+2_000_000)
R["open_conflict"]={"quote_uncertain":h.owner.reducer.states[MINT_A].quote_uncertain,"touched":r.touched,"position":pos.state.value,"unresolved":pos.unresolved_reason}
# Multi-mint successful transaction then FAILED_TX: only last mint is invalidated.
h=Harness(base_config(latency_model={"entry_delay_ms":50}));logs=create_frame_logs(MINT_A,CREATOR)+create_frame_logs(MINT_B,CREATOR);h.feed("helius_standard",logs_notification("multi",100,logs),T0+1_000_000);h.feed("existing_rpc",logs_notification("multi",100,[],err={"InstructionError":[0,"Custom"]}),T0+10_000_000);h.advance(T0+60_000_000)
R["multi_mint_reorg"]={m:{"quote_uncertain":h.owner.reducer.states[m].quote_uncertain,"position":h.owner.positions[h.owner.position_by_mint[m]].state.value if m in h.owner.position_by_mint else None} for m in (MINT_A,MINT_B)}
# Fee is updated while pending; then price moves enough to reject.
h=Harness(base_config(latency_model={"entry_delay_ms":50}));h.feed("helius_standard",create_payload(),T0+1_000_000)
s=h.fees.network;new=FeeSnapshot(**{**{k:getattr(s,k) for k in s.__slots__},"fee_snapshot_id":"new_network","received_mono_ns":T0+5_000_000,"normalized_fee_components":{**s.normalized_fee_components,"base_fee_lamports":9000,"network_fee_total_lamports":11000}})
h.fees=FeeCache(h.fees.protocol,new,h.fees.rent,h.fees.priority_policy);h.recorder.try_append(("fee_snapshot",(new,h.run_id)));h.owner.on_fee_snapshots([new]);h.feed("helius_standard",pump_frame(h,MINT_A,20_000_000_000,"big",101),T0+10_000_000);h.advance(T0+60_000_000)
a=attempts(h)[0];R["rejected_fill_current_fee"]={k:a[k] for k in ("status","failure_reason","network_fee_snapshot_id","network_fee_total_lamports","fees_charged_lamports")};R["rejected_fill_current_fee"]["expected_current_fee"]=11000
# Callback after later conflict: conflict did not exist at modeled fill, still back-propagates.
h=Harness(base_config(latency_model={"entry_delay_ms":10}));h.feed("helius_standard",create_payload(),T0+1_000_000);handle=h.adapter.submitted[0];h.clock.advance_to(T0+20_000_000)
h.owner.on_raw(RawEnvelope("existing_rpc","existing_rpc-c1",T0+20_000_000,utc_iso(h.clock.utc_ns()),1,create_payload(vsol=31_000_000_000)))
h.owner.on_outcome(ExecutionOutcome(handle.attempt_id,handle.intent,handle.submit_mono_ns,handle.scheduled_fill_mono_ns,T0+30_000_000));a=attempts(h)[0]
R["future_conflict_backpropagates"]={k:a[k] for k in ("status","failure_reason","modeled_fill_mono_ns","callback_actual_mono_ns")};R["future_conflict_backpropagates"]["conflict_observed_mono_ns"]=T0+20_000_000
save()

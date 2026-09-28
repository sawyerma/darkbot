import json,sys,base64
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"snapshot"))
from tests.helpers import *
from darkbot.fees import FeeCache
from darkbot.domain import FeeSnapshot
from darkbot.config import build_runtime_config
results={}
# Old reserves with automatically refreshed fees.
h=Harness(); pos=open_position(h); h.advance(T0+591_000_000_000)
a=attempts(h)[-1]; results["old_reserves"]={"state":pos.state.value,"side":a["side"],"status":a["status"],"proxy_age_ns":a["proxy_age_ns"],"limit_ms":h.cfg.execution["proxy_state_max_age_ms"]}
# Costs become invalid while an entry is already pending.
h=Harness(base_config(latency_model={"entry_delay_ms":20}));h.feed("helius_standard",create_payload(),T0+1_000_000)
s=h.fees.protocol
bad=FeeSnapshot(**{**{k:getattr(s,k) for k in s.__slots__},"fee_snapshot_id":"bad","received_mono_ns":T0+5_000_000,"validation_status":"RPC_ERROR"})
h.fees=FeeCache(bad,h.fees.network,h.fees.rent,h.fees.priority_policy);h.recorder.try_append(("fee_snapshot",(bad,h.run_id)));h.owner.on_fee_snapshots([bad]);h.advance(T0+30_000_000)
a=attempts(h)[0];results["invalid_fee_pending"]={k:a[k] for k in ["status","failure_reason","fees_charged_lamports","cost_evidence_status","fee_reconciliation_status"]};results["invalid_fee_pending"]["position"]=h.position().state.value
# Conflicting create arrives while entry pending.
h=Harness(base_config(latency_model={"entry_delay_ms":20}));h.feed("helius_standard",create_payload(),T0+1_000_000);h.feed("existing_rpc",create_payload(vsol=31_000_000_000),T0+5_000_000);h.advance(T0+30_000_000)
results["conflict_pending"]={"uncertain":h.owner.reducer.states[MINT_A].quote_uncertain,"state":h.position().state.value,"attempt":attempts(h)[0]["status"]}
# Verified trade tail explicitly foreign after SOL CREATE.
c=base_config();raw=json.loads(json.dumps(c.raw));raw["protocol"]["events"]["TradeEvent"]["tail_layouts"].append({"layout_id":"foreign_test","tail_fields":[["quote_mint","pubkey"]],"quote_mint_field":"quote_mint","verification_status":"VERIFIED_TEST"});cfg=build_runtime_config(c.path,raw)
h=Harness(cfg);open_position(h)
tail=encode_fields([("quote_mint",BUYER2)],[["quote_mint","pubkey"]]);logs=trade_frame_logs(MINT_A,2_000_000_000,1,True,BUYER,99_000_000_000,V_TOK0,R_TOK0,tail=tail)
h.feed("helius_standard",logs_notification("foreign",101,logs),T0+2_000_000);h.advance(T0+3_000_000)
st=h.owner.reducer.states[MINT_A];results["foreign_trade"]={"uncertain":st.quote_uncertain,"vsol":st.virtual_sol_reserves,"attempts":[{"side":a["side"],"status":a["status"]} for a in attempts(h)],"stored":dict(h.rows("select virtual_sol_reserves,quote_mint_raw,quote_scope_status from market_events where signature='foreign'")[0])}
# Network fee changes while pending; compare persisted attempt and actual ledger.
h=Harness(base_config(latency_model={"entry_delay_ms":20}));h.feed("helius_standard",create_payload(),T0+1_000_000)
s=h.fees.network;norm={**s.normalized_fee_components,"base_fee_lamports":9000,"network_fee_total_lamports":11000}
new=FeeSnapshot(**{**{k:getattr(s,k) for k in s.__slots__},"fee_snapshot_id":"new_network","received_mono_ns":T0+5_000_000,"normalized_fee_components":norm})
h.fees=FeeCache(h.fees.protocol,new,h.fees.rent,h.fees.priority_policy);h.recorder.try_append(("fee_snapshot",(new,h.run_id)));h.owner.on_fee_snapshots([new]);h.advance(T0+30_000_000)
a=attempts(h)[0];fq=dict(h.rows("select * from quote_snapshots where quote_snapshot_id=?",(a["fill_quote_snapshot_id"],))[0]);results["fee_provenance"]={"attempt_network_id":a["network_fee_snapshot_id"],"attempt_network_fee":a["network_fee_total_lamports"],"fill_quote_network_fee":fq["network_fee_total_lamports"],"position_network_fee":h.position().network_fee_lamports}
# Partial -> complete canonical event remains non-actionable in stored record.
h=Harness();logs=create_frame_logs(MINT_A,CREATOR);logs.insert(3,"Program data: "+base64.b64encode(event_discriminator("FutureEvent")+b"x"*40).decode());h.feed("existing_rpc",logs_notification("s",1,logs),T0+1_000_000);h.feed("helius_standard",create_payload(sig="s",slot=1),T0+2_000_000);h.advance(T0+3_000_000)
results["partial_persisted"]={"positions":len(h.owner.positions),"create":dict(h.rows("select actionable_mono_ns,winner_feed_id,first_seen_mono_ns from market_events where event_type='CREATE'")[0])}
text=json.dumps(results,indent=2);print(text);Path(__file__).with_name("execution_results.json").write_text(text)


import { useState } from "react";
import type { LiquidatorDraft, Project0Tab, StrategyKind } from "../model/contracts";
import { validateDraft } from "../hooks/useFleetDrafts";
import { DetailRow, Section, Notice } from "../components/PanelPrimitives";
type Fleet={drafts:LiquidatorDraft[];selected:LiquidatorDraft|null;selectedId:string|null;
 select:(id:string|null)=>void;update:(id:string,patch:Partial<LiquidatorDraft>)=>void;
 create:()=>void;duplicate:()=>void;remove:()=>void};
export function LiquidatorPanel({fleet,goto}:{fleet:Fleet;goto:(t:Project0Tab)=>void}) {
 const [message,setMessage]=useState("");const d=fleet.selected;
 const patch=(p:Partial<LiquidatorDraft>)=>{if(d)fleet.update(d.id,p)};
 const toggle=(k:StrategyKind,v:boolean)=>{if(d)patch({enabledStrategies:v?Array.from(new Set([...d.enabledStrategies,k])):d.enabledStrategies.filter(x=>x!==k)})};
 return <><Section title="Liquidator fleet" aside={<span className="p0-badge">Execution locked · V0</span>}>
 <p className="p0-quiet">Shared market state · Independent bot drafts · No live instances.</p>
 <DetailRow title="Running liquidators" value="0 · LIVE not authorized"/>
 <DetailRow title="Available working capital" value="— USD · Not connected"/>
 <DetailRow title="Realized net PnL" value="— USD · Not proven"/></Section>
 <Section title="Bots & strategies" aside={<button className="p0-secondary" onClick={()=>fleet.create()}>+ Add bot</button>}>
 <div className="p0-fleet"><div><p className="p0-quiet">Liquidators · {fleet.drafts.length} drafts</p>
 <div role="listbox" aria-label="Bot drafts">{fleet.drafts.map(x=><button key={x.id} role="option" aria-selected={x.id===fleet.selectedId}
 className={"p0-botrow "+(x.id===fleet.selectedId?"p0-selected":"")}
 onClick={()=>{fleet.select(x.id);setMessage("")}}><strong>{x.name}</strong>
 <small>{x.enabledStrategies.join(" + ")||"None"} · Draft</small></button>)}</div>
 <div className="p0-actions"><button className="p0-secondary" disabled={!d} onClick={fleet.duplicate}>Duplicate</button>
 <button className="p0-secondary" disabled={!d} onClick={fleet.remove}>Remove</button></div></div>
 <div>{d?<><div className="p0-row"><h2>{d.name}</h2><span className="p0-badge">Draft</span></div>
 <div className="p0-formgrid"><label>Name<input value={d.name} onChange={e=>patch({name:e.target.value})}/></label>
 <label>Strategy origin<select disabled value={d.origin}><option value="manual">Custom draft</option><option value="analytics_unverified">Analytics candidate · unqualified</option></select></label></div>
 <p className="p0-label">Liquidation strategies</p><div className="p0-formgrid">
 {(["CLASSIC","RECEIVERSHIP"] as const).map(k=><label key={k} className="p0-check">
 <input type="checkbox" checked={d.enabledStrategies.includes(k)} onChange={e=>toggle(k,e.target.checked)}/>{k==="CLASSIC"?"Classic":"Receivership"}</label>)}</div>
 <div className="p0-formgrid">
 <label>Capital limit · USD<input type="number" min="0" value={d.capitalUsd} onChange={e=>patch({capitalUsd:e.target.value})}/></label>
 <label>Minimum net profit · USD<input type="number" min="0" value={d.minimumNetUsd} onChange={e=>patch({minimumNetUsd:e.target.value})}/></label>
 <label>Fee limit · USD<input type="number" min="0" value={d.maximumFeeUsd} onChange={e=>patch({maximumFeeUsd:e.target.value})}/></label>
 <label>Maximum in-flight<input type="number" min="1" value={d.maximumInFlight} onChange={e=>patch({maximumInFlight:e.target.value})}/></label></div>
 <DetailRow title="Execution status" value="Not authorized"/>
 <div className="p0-actions"><button className="p0-secondary" onClick={()=>setMessage(validateDraft(d)??"Valid UI draft. Not saved to backend.")}>Validate draft</button><button disabled className="p0-primary">Start selected bot</button></div></>:<p className="p0-quiet">Add a bot to configure its strategy.</p>}</div></div>
 {message?<Notice>{message}</Notice>:null}
 <p className="p0-note">Drafts exist in memory only. Rust fleet scheduling, admission, wallet reservations and durable profile storage are not implemented.</p></Section>
 <Section title="Parallel operation" aside={<span className="p0-quiet">Target architecture</span>}>
 <DetailRow title="Shared market capture & Shadow" value="One unfiltered stream · Planned"/>
 <DetailRow title="Per-bot results & latency" value="Unmeasured"/>
 <DetailRow title="Execution arbiter" value="Not qualified"/>
 <DetailRow title="Background replay / optimize" value="Resource-isolated · Planned"/>
 <div className="p0-actions"><button className="p0-secondary" onClick={()=>goto("capture")}>Capture →</button>
 <button className="p0-secondary" onClick={()=>goto("analytics")}>Analytics →</button><button disabled className="p0-primary">Start fleet</button></div></Section></>;
}
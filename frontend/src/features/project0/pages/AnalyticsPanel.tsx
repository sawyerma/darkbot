import { useState } from "react";
import type { LiquidatorDraft, Project0Tab } from "../model/contracts";
import { Section, DetailRow, TabStrip, Notice } from "../components/PanelPrimitives";
type AnalTab="report"|"backtest"|"optimize";
const tabs:[{value:AnalTab;label:string},{value:AnalTab;label:string},{value:AnalTab;label:string}]=[
 {value:"report",label:"Report"},{value:"backtest",label:"Backtest"},{value:"optimize",label:"Optimize"}];
export function AnalyticsPanel({createDraft,goto}:{createDraft:(x?:Pick<LiquidatorDraft,"capitalUsd"|"minimumNetUsd">)=>void;goto:(x:Project0Tab)=>void}){
 const [tab,setTab]=useState<AnalTab>("report");const [capital,setCapital]=useState("");const [minimumNet,setMinimumNet]=useState("");
 const [msg,setMsg]=useState("");
 return <Section title="Runs & analysis">
 <label>Recorded run<select disabled><option>No journal connected</option></select></label>
 <TabStrip minor label="Analysis mode" options={tabs} value={tab} onChange={setTab}/>
 {tab==="report"?<><DetailRow title="Market liquidations" value="—"/>
 <DetailRow title="Executable Shadow opportunities" value="—"/><DetailRow title="Known Shadow net edge" value="— USD"/>
 <DetailRow title="Realized net PnL" value="— USD"/><p className="p0-note">Report filters are under Capture. No journal loaded.</p></>:null}
 {tab==="backtest"?<><p>Replay immutable recorded evidence with as-of historical inputs.</p>
 <DetailRow title="Replay source" value="Not connected"/><button disabled className="p0-primary">Run backtest</button></>:null}
 {tab==="optimize"?<><div className="p0-formgrid"><label>Capital · USD<input type="number" value={capital} onChange={e=>setCapital(e.target.value)}/></label>
 <label>Minimum net edge · USD<input type="number" min="0" value={minimumNet} onChange={e=>setMinimumNet(e.target.value)}/></label></div>
 <DetailRow title="Funding and fee scenarios" value="Backend controlled"/><DetailRow title="Holdout / uncertainty" value="Requires run data"/>
 <div className="p0-actions"><button disabled className="p0-primary">Optimize</button><button className="p0-secondary"
 onClick={()=>{if(!/^\d+(?:\.\d{1,6})?$/.test(capital)||Number(capital)<=0||!/^\d+(?:\.\d{1,6})?$/.test(minimumNet)){
 setMsg("Enter a positive capital and nonnegative net edge.");return}
 createDraft({capitalUsd:capital,minimumNetUsd:minimumNet});goto("liquidator");}}>Add as bot draft →</button></div>
 <p className="p0-note">Only typed input values are copied to a draft. No optimization or replay has run.</p></>:null}
 {msg?<Notice>{msg}</Notice>:null}</Section>;
}
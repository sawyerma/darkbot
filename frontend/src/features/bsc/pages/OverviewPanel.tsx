import { Line, Section } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectStatus, useBscStore } from "../store/bscStore";
import { EvidenceRows, PublicRows, timestamp, ValueState } from "./EvidenceRows";
import { ReadinessActions } from "../ReadinessActions";
import type { BscTab } from "../BscPage";
export function OverviewPanel({goto}:{goto:(t:BscTab)=>void}) {
 const {projection:s,status}=useBscStore(selectStatus);
 return <>
 <Section title="Source snapshot"><Line label="Source quality" value={status}/><Line label="Data as of" value={timestamp(s?.dataAsOfUnixS)}/><Line label="Source commit" value={s?.sourceCommit||"—"}/><Line label="Update source" value="darkbotctl bsc sync" hint="Run in the original repository terminal; existing snapshots remain dated."/></Section>
 <Section title="Qualification gates">{s?.gates.map(g=><Line key={g.gateId} label={g.gateId} value={<ValueState state={g.state}/>} hint={g.reasonCodes.join(" · ")}/>)}</Section>
 <Section title="Pre-Live Readiness · read-only"><Line label="Runtime readiness" value="UNKNOWN" hint="No BSC execution engine is running"/><Line label="Pre-Live result" value={<ValueState state={s?.readiness?.verdict}/>}/><Line label="Funding" value={s?.readiness?.fundingVerdict||"UNKNOWN"}/><EvidenceRows evidence={s?.readiness?.evidence}/><PublicRows rows={s?.readiness?.checks}/><ReadinessActions/></Section>
 <Section title="Guard"><Line label="Guard" value={<ValueState state={s?.guard?.guardState}/>}/><Line label="Compiled hash" value={s?.guard?.frozenApprovedSha256||"—"}/><Line label="Current contract" value={s?.guard?.contractPresent? s.guard.contractPath : "No active v4 contract"}/><Line label="LIVE" value={<ValueState state={s?.liveState}/>}/></Section>
 <Section title="Portal"><Line label="Bound → observed" value={`${s?.readiness?.portalVersionBound||"—"} → ${s?.readiness?.portalVersionObserved||"—"}`}/><Line label="Implementation bound" value={s?.readiness?.portalImplBound||"—"}/><Line label="Implementation observed" value={s?.readiness?.portalImplObserved||"—"}/></Section>
 <Section title="Builder"><Line label="Documentation fetched" value={timestamp(s?.builder?.refreshObservedUnixS)}/><Line label="Expires" value={timestamp(s?.builder?.refreshExpiresUnixS)}/><PublicRows rows={s?.builder?.floors}/><EvidenceRows evidence={s?.builder?.probe}/>{(["price","noMerge","positionFirst","inclusion"] as const).map(k=><Line key={k} label={k} value={<ValueState state={s?.builder?.[k]}/>}/>)}</Section>
 <Section title="Witness"><Line label="Source ready" value={s?.witness?.sourceReady===undefined?"UNKNOWN":String(s.witness.sourceReady)}/><Line label="T0 → BUY" value={s?.witness?.t0ToBuyState||"UNKNOWN"}/></Section>
 {s?.warnings.map(w=><Notice key={w}>{w}</Notice>)}
 <div className="g-actions"><button className="g-secondary" onClick={()=>goto("launch")}>Bound plan</button><button className="g-secondary" onClick={()=>goto("monitoring")}>Monitoring</button></div>
 <Notice>Read-only observations. A connected reporter, successful readiness check or ARMED guard never grants LIVE permission.</Notice>
 </>;
}

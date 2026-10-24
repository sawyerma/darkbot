import { Line } from "@/shared/ui/graphite/Rows";
import type { Evidence,PublicField } from "../model/types";
import { statusTone } from "../model/status";
export function timestamp(s:bigint|undefined) {return s && s>0n && s<8640000000000n ? new Date(Number(s)*1000).toLocaleString() : "UNKNOWN";}
export function ValueState({state}:{state:string|undefined}) {return <span className="g-badge bsc-state" data-tone={statusTone(state??"UNKNOWN")}>{state||"UNKNOWN"}</span>;}
export function PublicRows({rows}:{rows:PublicField[]|undefined}) {return <>{rows?.map((f,i)=><Line key={`${f.name}:${i}`} label={f.name} value={f.value||"—"} hint={f.hint}/> )}</>;}
export function EvidenceRows({evidence}:{evidence:Evidence|undefined}) {return <><Line label="Evidence" value={evidence?.path||"—"}/><Line label="SHA-256" value={evidence?.sha256||"—"}/><Line label="Evidence observed" value={timestamp(evidence?.observedUnixS)}/></>;}

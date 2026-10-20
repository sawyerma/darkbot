import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line,Section } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectMonitoring,useBscStore } from "../store/bscStore";
import { EvidenceRows,PublicRows,ValueState } from "./EvidenceRows";
export function MonitoringPanel(){const quality=useTopic(TopicId.BSC_MONITORING_STATE);const {projection:m}=useBscStore(selectMonitoring);return <><Section title="G0.8 verification"><Line label="Source quality" value={quality}/><Line label="Result" value={<ValueState state={m?.state}/>}/><EvidenceRows evidence={m?.evidence}/><PublicRows rows={m?.results}/></Section><Notice>Submission acknowledgement is not acquisition. No timing, inclusion or profit is inferred.</Notice></>;}

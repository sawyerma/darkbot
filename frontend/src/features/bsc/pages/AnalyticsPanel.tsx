import { Line,Section } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectStatus,useBscStore } from "../store/bscStore";
import { EvidenceRows,PublicRows,ValueState } from "./EvidenceRows";
export function AnalyticsPanel(){const {projection:s}=useBscStore(selectStatus);return <><Section title="Historical DRY evidence"><Line label="DRY result" value={<ValueState state={s?.dry?.state}/>}/><Line label="Current portal validity" value={s?.dry?.validForCurrentPortal===undefined?"UNKNOWN":String(s.dry.validForCurrentPortal)}/><EvidenceRows evidence={s?.dry?.evidence}/><PublicRows rows={s?.dry?.parameters}/></Section><Notice>Historical simulation is not live profit. DRY cannot be started from this interface.</Notice></>;}

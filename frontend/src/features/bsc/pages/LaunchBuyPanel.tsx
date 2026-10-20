import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line,Section } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectLaunch,useBscStore } from "../store/bscStore";
import { EvidenceRows,PublicRows } from "./EvidenceRows";
export function LaunchBuyPanel(){ const quality=useTopic(TopicId.BSC_LAUNCH_STATE);const {projection:l}=useBscStore(selectLaunch);return <><Section title="Bound v4 plan · read-only"><Line label="Source quality" value={quality}/><Line label="Contract" value={l?.contractPresent?"Present":"No active v4 contract"}/><EvidenceRows evidence={l?.evidence}/><PublicRows rows={l?.contractFields}/></Section><Section title="Observed runtime context"><PublicRows rows={l?.runtimeFields}/>{!l?.runtimeFields.length?<Line label="Runtime context" value="UNKNOWN"/>:null}</Section><Notice>{l?.reason||"No bound evidence"}. DRY, BUY, arm, probe and send are unavailable.</Notice></>;}

import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectMonitoring, selectStatus, useBscStore } from "../store/bscStore";
import { EvidenceRows, PublicRows, timestamp, ValueState } from "./EvidenceRows";
import { PlacedSection } from "./PlacedSection";
import { useNativeRecords } from "./useNativeRecords";

// The planned independent signal inputs and venues (BSC plan v9): no native owner before G0.7 → G0.8.
const SOURCES = ["BSC_INGRESS_GETH", "BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_DRPC", "BSC_INGRESS_BLOCKREQ", "BSC_INGRESS_48CLUB"];
const VENUES = ["FLAP", "FOUR_CLASSIC", "OPENFOUR"];
const FUTURE = "No native source/venue owner before G0.7 → G0.8 (C1/C2); requested/effective modes stay disabled";

/** Monitoring in the fixed §13 order: sources → venues → builder → G0.7 → G0.8 → chain → timing. */
export function MonitoringPanel() {
  const quality = useTopic(TopicId.BSC_MONITORING_STATE);
  const { projection: m } = useBscStore(selectMonitoring);
  const { projection: s } = useBscStore(selectStatus);
  const records = useNativeRecords();
  const b = s?.builder;
  const g07 = s?.gates.find(g => g.gateId === "G0.7");
  return <>
    <Line label="Monitoring source quality" value={<ValueState state={quality} />} hint="bsc.monitoring.state transport" />
    <PlacedSection panel="monitoring" section={1} records={records}>
      {SOURCES.map(id => <Line key={id} label={id} value={<ValueState state="FUTURE_PHASE_BLOCKED" />} hint={FUTURE} />)}
    </PlacedSection>
    <PlacedSection panel="monitoring" section={2} records={records}>
      {VENUES.map(id => <Line key={id} label={id} value={<ValueState state="FUTURE_PHASE_BLOCKED" />} hint={FUTURE} />)}
    </PlacedSection>
    <PlacedSection panel="monitoring" section={3} records={records}>
      <Line label="Builder documentation fetched" value={timestamp(b?.refreshObservedUnixS)} />
      <Line label="Documentation expires" value={timestamp(b?.refreshExpiresUnixS)} />
      <PublicRows rows={b?.floors} />
      {(["price", "noMerge", "positionFirst", "inclusion"] as const).map(k => <Line key={k} label={`Empirical ${k}`} value={<ValueState state={b?.[k]} />} />)}
      <EvidenceRows evidence={b?.probe} />
    </PlacedSection>
    <PlacedSection panel="monitoring" section={4} records={records}>
      <Line label="G0.7 classification" value={<ValueState state={g07?.state} />} hint={g07?.reasonCodes.join(" · ")} />
      <EvidenceRows evidence={g07?.evidence} />
    </PlacedSection>
    <PlacedSection panel="monitoring" section={5} records={records}>
      <Line label="G0.8 verdict" value={<ValueState state={m?.state} />} />
      <EvidenceRows evidence={m?.evidence} />
    </PlacedSection>
    <PlacedSection panel="monitoring" section={6} records={records} />
    <PlacedSection panel="monitoring" section={7} records={records}>
      <Line label="Producer witness source ready" value={s?.witness?.sourceReady === undefined ? "UNKNOWN" : String(s.witness.sourceReady)} />
      <Line label="T0 → BUY" value={<ValueState state={s?.witness?.t0ToBuyState} />} hint="Only an original producer witness can prove timing" />
      <EvidenceRows evidence={s?.witness?.evidence} />
    </PlacedSection>
    <Notice>Submission acknowledgement is not acquisition. No timing, inclusion or profit is inferred.</Notice>
  </>;
}

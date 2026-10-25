import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectMonitoring, selectStatus, useBscStore } from "../store/bscStore";
import { EvidenceRows, PublicRows, timestamp, ValueState } from "./EvidenceRows";
import { PlacedSection } from "./PlacedSection";
import { useNativeRecords } from "./useNativeRecords";
import { sourceControlView } from "../model/sources";
import { SourceActions } from "../SourceActions";

// The planned venues (BSC plan v9): no native venue owner yet (phases D/E).
const VENUES = ["FLAP", "FOUR_CLASSIC", "OPENFOUR"];
const FUTURE = "No native venue owner before phases D/E; requested/effective modes stay disabled";

/** Monitoring in the fixed §13 order: sources → venues → builder → G0.7 → G0.8 → chain → timing. */
export function MonitoringPanel() {
  const quality = useTopic(TopicId.BSC_MONITORING_STATE);
  const { projection: m } = useBscStore(selectMonitoring);
  const { projection: s } = useBscStore(selectStatus);
  const records = useNativeRecords();
  const sources = sourceControlView(records);
  const b = s?.builder;
  const g07 = s?.gates.find(g => g.gateId === "G0.7");
  const g08 = s?.gates.find(g => g.gateId === "G0.8");
  return <>
    <Line label="Monitoring source quality" value={<ValueState state={quality} />} hint="bsc.monitoring.state transport" />
    <PlacedSection panel="monitoring" section={1} records={records}>
      <Line label="Source modes generation" value={sources.available ? sources.generation.toString() : sources.reason} hint="Native bsc_runtime source owner; requested vs. effective, LIVE stays BLOCKED" />
      <Line label="BSC runtime" value={<ValueState state={sources.runtime.state} />} hint={`${sources.runtime.mode} · launches ${sources.runtime.launches} · journal ${sources.runtime.journal}`} />
      {sources.rows.map(row => <Line key={row.id} label={row.id} value={<><ValueState state={row.effective} /> <ValueState state={row.health} /></>}
        hint={`requested ${row.requested}${row.blockReason ? ` · ${row.blockReason}` : ""}${row.counters ? ` · ${row.counters}` : ""}`} />)}
      <SourceActions view={sources} />
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
      <Line label="G0.8 verdict" value={<ValueState state={m?.state} />} hint={g08?.reasonCodes.join(" · ")} />
      <EvidenceRows evidence={m?.evidence} />
    </PlacedSection>
    <PlacedSection panel="monitoring" section={6} records={records}>
      <Line label="Canonical follower" value={<ValueState state={sources.runtime.canonical?.state ?? "NOT_CONFIGURED"} />}
        hint={sources.runtime.canonical ? `head ${sources.runtime.canonical.head} · age ${sources.runtime.canonical.headAgeMs} ms · ${sources.runtime.canonical.chain}` : "No canonical endpoint in the runtime configuration: no G0.4 denominator"} />
      {sources.runtime.canonical && <Line label="Canonical launches (G0.4 denominator)" value={sources.runtime.canonical.launches} hint="Orphaned launches are taken out; Four token fields only where the event layout is proven" />}
      {sources.runtime.canonical && <Line label="Pending sighting vs. block (Flap)" value={sources.runtime.canonical.join} hint="Local monotonic time of the first verified sighting vs. the follower's first header; no evaluation before the G0.4 criteria are confirmed" />}
    </PlacedSection>
    <PlacedSection panel="monitoring" section={7} records={records}>
      <Line label="Producer witness source ready" value={s?.witness?.sourceReady === undefined ? "UNKNOWN" : String(s.witness.sourceReady)} />
      <Line label="T0 → BUY" value={<ValueState state={s?.witness?.t0ToBuyState} />} hint="Only an original producer witness can prove timing" />
      <EvidenceRows evidence={s?.witness?.evidence} />
    </PlacedSection>
    <Notice>Submission acknowledgement is not acquisition. No timing, inclusion or profit is inferred.</Notice>
  </>;
}

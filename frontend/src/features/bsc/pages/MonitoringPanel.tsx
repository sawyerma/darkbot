import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectMonitoring, selectStatus, useBscStore } from "../store/bscStore";
import { EvidenceRows, PublicRows, timestamp, ValueState } from "./EvidenceRows";
import { PlacedSection } from "./PlacedSection";

/** What each switchable component does (its owner's rule shows in the effective mode). */
const COMPONENT_HINTS: Record<string, string> = {
  BSC_STATE_C2: "C2 canonical/pending state owner; SHADOW observes only, LIVE stays BLOCKED until G0.8 PASS and evaluated G0.4",
  BSC_EXEC: "C3 execution path; SHADOW runs it to the PAPER sink (PAPER identity, never sent), LIVE stays BLOCKED (economic LIVE not authorized)",
};
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
      {sources.runtime.capture && <Line label="BSC capture" value={<ValueState state={sources.runtime.capture.state} />} hint={sources.runtime.capture.detail} />}
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
      {sources.components.map(row => <Line key={row.id} label={row.id} value={<ValueState state={row.effective} />}
        hint={`requested ${row.requested}${row.blockReason ? ` · ${row.blockReason}` : ""} · ${COMPONENT_HINTS[row.id] ?? ""}`} />)}
      <SourceActions view={sources} rows={sources.components} testId="bsc-component" />
      <Line label="Canonical follower" value={<ValueState state={sources.runtime.canonical?.state ?? "NOT_CONFIGURED"} />}
        hint={sources.runtime.canonical ? `head ${sources.runtime.canonical.head} · age ${sources.runtime.canonical.headAgeMs} ms · ${sources.runtime.canonical.chain}` : "No canonical endpoint in the runtime configuration: no G0.4 denominator"} />
      {sources.runtime.canonical && <Line label="Canonical launches (G0.4 denominator)" value={sources.runtime.canonical.launches} hint="Orphaned launches are taken out; Four.meme Classic and OpenFour through the Phase E decoder; token derived pending = the signed launch bytes name the canonical token before execution" />}
      {sources.runtime.canonical && <Line label="Pending sighting vs. block time (Flap)" value={sources.runtime.canonical.join} hint="First verified sighting (local wall-clock estimate) vs. the block's own ms time; header fetch lag bounds the clock offset; no evaluation before the G0.4 criteria are confirmed" />}
      <Line label="C2 chain state" value={<ValueState state={sources.runtime.chainState?.coherent ?? "NOT_RUNNING"} />}
        hint={sources.runtime.chainState ? `head ${sources.runtime.chainState.head} · ${sources.runtime.chainState.held}` : "BSC_STATE_C2 is not effective SHADOW (or no canonical endpoint): no canonical/pending state owner runs"} />
      {sources.runtime.chainState && <Line label="C2 canonical and pending effects" value={sources.runtime.chainState.effects} hint="Applied by the one native owner from the follower's complete blocks and the launch/own-account sightings; proofs only for watched own accounts" />}
      {sources.runtime.chainState && <Line label="C2 refused, overflowed, unknown" value={sources.runtime.chainState.faults} hint="Every refused or overflowed input is counted; unknown never becomes zero" />}
      {sources.runtime.chainState && <Line label="C2 restart" value={sources.runtime.chainState.checkpoint} hint="The retained canonical window is checkpointed in the runtime pass; at start it is restored and own accounts stay quarantined until a nonce fact reconciles them" />}
      <Line label="C3 PAPER path" value={<ValueState state={sources.runtime.exec?.state ?? "NOT_CONFIGURED"} />}
        hint={sources.runtime.exec ? sources.runtime.exec.detail : "No BSC_EXEC_PAPER_IDENTITY in the runtime configuration"} />
      {sources.runtime.exec && <Line label="C3 path to the PAPER sink" value={sources.runtime.exec.path} hint="Production decoder, C2 snapshot, admission, build/sign and lane; the sink has no transport, the signed bytes stay local" />}
      {sources.runtime.exec && <Line label="C3 local latency" value={sources.runtime.exec.latency} hint="Local production-code cost on this host (T_shadow_sink), never the transport boundary T_send" />}
    </PlacedSection>
    <PlacedSection panel="monitoring" section={7} records={records}>
      <Line label="Producer witness source ready" value={s?.witness?.sourceReady === undefined ? "UNKNOWN" : String(s.witness.sourceReady)} />
      <Line label="T0 → BUY" value={<ValueState state={s?.witness?.t0ToBuyState} />} hint="Only an original producer witness can prove timing" />
      <EvidenceRows evidence={s?.witness?.evidence} />
    </PlacedSection>
    <Notice>Submission acknowledgement is not acquisition. No timing, inclusion or profit is inferred.</Notice>
  </>;
}

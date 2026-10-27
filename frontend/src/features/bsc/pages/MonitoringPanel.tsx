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
  BSC_EXEC: "C3 execution path; SHADOW runs it to the PAPER sink for Flap launches while BSC_VENUE_FLAP is SHADOW (PAPER identity, never sent), LIVE stays BLOCKED (economic LIVE)",
  BSC_STRATEGY_C: "Strategy C on canonical Flap events while BSC_VENUE_FLAP is SHADOW; SHADOW keeps a PAPER ledger (nothing signed or sent), LIVE stays BLOCKED (economic LIVE)",
};
import { useNativeRecords } from "./useNativeRecords";
import { sourceControlView } from "../model/sources";
import { SourceActions } from "../SourceActions";

/** What each venue's mode does at the native owner (plan v9 §2.3). */
const VENUE_HINTS: Record<string, string> = {
  BSC_VENUE_FLAP: "SHADOW admits Flap launches to the C3 PAPER path and Strategy C (with BSC_EXEC / BSC_STRATEGY_C SHADOW); OFF admits none; LIVE stays BLOCKED",
  BSC_VENUE_FOUR_CLASSIC: "Recognized and decoded (Phase E); no economics or execution path to admit to yet (FOUR_EXECUTION_PATH_MISSING); LIVE stays BLOCKED",
  BSC_VENUE_OPENFOUR: "Recognized and decoded (Phase E); no economics or execution path to admit to yet (FOUR_EXECUTION_PATH_MISSING); LIVE stays BLOCKED",
};

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
      {sources.venues.map(row => <Line key={row.id} label={row.id} value={<ValueState state={row.effective} />}
        hint={`requested ${row.requested}${row.blockReason ? ` · ${row.blockReason}` : ""} · ${VENUE_HINTS[row.id] ?? ""}${row.receipt ? ` · last set by ${row.receipt}` : ""}`} />)}
      <SourceActions view={sources} rows={sources.venues} testId="bsc-venue" action="bsc.venue.mode.set" />
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
      {sources.runtime.canonical && <Line label="Canonical launches (G0.4 denominator)" value={sources.runtime.canonical.launches} hint="Orphaned launches are taken out; Four.meme Classic and OpenFour through the Phase E decoder; derivable from the launch bytes = the signed launch bytes name the canonical token; actionable before the block = that and the launch was seen in full pending before its block" />}
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
      {sources.runtime.exec && <Line label="Strategy C PAPER path" value={sources.runtime.exec.strategy} hint="Phase I: a PASS is a BUY, an exit trigger an approval and SELL, admitted on the strategy's exact curve and C2's canonical snapshot, signed by the PAPER identity, through lanes and the local node's single-raw plan to the PAPER sink; nothing is sent" />}
      <Line label="Strategy C" value={<ValueState state={sources.runtime.strategy?.state ?? "NOT_RUNNING"} />}
        hint={sources.runtime.strategy ? sources.runtime.strategy.detail : "BSC_STRATEGY_C is not effective SHADOW (or no canonical endpoint): no strategy runs"} />
      {sources.runtime.strategy && <Line label="Strategy C decisions" value={sources.runtime.strategy.decisions} hint="Typed filters over the observation window of each canonical Flap launch; a gap is DATA_INCOMPLETE, never a pass" />}
      {sources.runtime.strategy && <Line label="Strategy C PAPER ledger" value={sources.runtime.strategy.paper} hint="Entries and exits valued by the exact Portal curve with the frozen gas evidence, after every cost; an open position whose curve stopped being exact stays unresolved, never valued. Not a proven outcome" />}
    </PlacedSection>
    <PlacedSection panel="monitoring" section={7} records={records}>
      <Line label="Producer witness source ready" value={s?.witness?.sourceReady === undefined ? "UNKNOWN" : String(s.witness.sourceReady)} />
      <Line label="T0 → BUY" value={<ValueState state={s?.witness?.t0ToBuyState} />} hint="Only an original producer witness can prove timing" />
      <EvidenceRows evidence={s?.witness?.evidence} />
    </PlacedSection>
    <Notice>Submission acknowledgement is not acquisition. No timing, inclusion or profit is inferred.</Notice>
  </>;
}

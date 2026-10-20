import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line, Section } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectLaunch, selectStatus, useBscStore } from "../store/bscStore";
import { ReadinessActions } from "../ReadinessActions";
import { EvidenceRows, PublicRows, ValueState } from "./EvidenceRows";
import { FilteredEvidence } from "./FilteredEvidence";

export function LaunchBuyPanel() {
  const quality = useTopic(TopicId.BSC_LAUNCH_STATE);
  const { projection: launch } = useBscStore(selectLaunch);
  const { projection: status, status: statusQuality } = useBscStore(selectStatus);
  return <>
    <Section title="Controlled BUY test · G0.7 → G0.8">
      <Line label="Status source quality" value={statusQuality} />
      <Line label="Pre-Live Readiness" value={<ValueState state={status?.readiness?.verdict} />} />
      <Line label="Compiled guard" value={<ValueState state={status?.guard?.guardState} />} />
      <Line label="Contract approval" value={status?.guard?.approved === undefined ? "UNKNOWN" : String(status.guard.approved)} />
      <Line label="Execution ready" value={status?.guard?.executionReady === undefined ? "UNKNOWN" : String(status.guard.executionReady)} />
      <Line label="Attempt ID" value={status?.guard?.attemptId || "—"} />
      <Line label="Maximum attempts" value={status?.guard?.maxAttempts?.toString() ?? "UNKNOWN"} />
      {status?.gates.filter(gate => gate.gateId === "G0.7" || gate.gateId === "G0.8").map(gate =>
        <Line key={gate.gateId} label={gate.gateId} value={<ValueState state={gate.state} />} hint={gate.reasonCodes.join(" · ")} />)}
      <Notice>Sequence: portal requalification → bound v4 contract → current readiness → explicit approval of the exact contract → one G0.7 attempt → G0.8 acquisition, ordering and cost verification.</Notice>
    </Section>
    <Section title="Readiness check · no signing or sending">
      <ReadinessActions />
      <EvidenceRows evidence={status?.readiness?.evidence} />
      <FilteredEvidence rows={status?.readiness?.checks} label="Filter readiness checks" />
    </Section>
    <Section title="Bound v4 plan · read-only">
      <Line label="Plan source quality" value={quality} />
      <Line label="Contract" value={launch?.contractPresent ? "Present" : "No active v4 contract"} />
      <EvidenceRows evidence={launch?.evidence} />
      <FilteredEvidence rows={launch?.contractFields} label="Filter contract fields" />
    </Section>
    <Section title="Observed runtime context">
      <PublicRows rows={launch?.runtimeFields} />
      {!launch?.runtimeFields.length ? <Line label="Runtime context" value="UNKNOWN" /> : null}
    </Section>
    <Notice>{launch?.reason || "No bound evidence"}. BUY control is not implemented. DRY, arm, probe and send remain unavailable. Filters only change the displayed fields; signed limits and execution rules remain bound to native evidence.</Notice>
  </>;
}

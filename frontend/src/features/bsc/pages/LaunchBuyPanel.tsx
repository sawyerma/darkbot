import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line, Section } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectLaunch, selectStatus, useBscStore } from "../store/bscStore";
import { ReadinessActions } from "../ReadinessActions";
import { EvidenceRows, PublicRows, ValueState } from "./EvidenceRows";
import { FilteredEvidence } from "./FilteredEvidence";
import { MISSING_INTEGRATION, PLANNED_SCOPES, PLANNED_SOURCES, candidateControlView } from "../model/candidate";
import { CandidateActions } from "../CandidateActions";
import { useCandidateControl } from "@/core/realtime/candidateControl";

export function LaunchBuyPanel() {
  const quality = useTopic(TopicId.BSC_LAUNCH_STATE);
  const { projection: launch, status: launchQuality } = useBscStore(selectLaunch);
  const { projection: status, status: statusQuality } = useBscStore(selectStatus);
  const { busy } = useCandidateControl();
  const candidate = candidateControlView({ fields: launch?.runtimeFields, guardState: status?.guard?.guardState, statusOk: launchQuality === "LIVE" && statusQuality === "LIVE", busy });
  const runtime = launch?.runtimeFields.filter(f => !f.name.startsWith("candidate."));
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
    <Section title="G0.7 – Self-Launch-Testkandidat">
      <Line label="Candidate control" value={<ValueState state={candidate.reason} />} />
      <CandidateActions view={candidate} />
      {candidate.rows.map(row => <Line key={row.label} label={row.label} value={row.value} />)}
      {candidate.readiness.map(row => <Line key={row.profile} label={`Readiness ${row.profile}`} value={<ValueState state={row.value} />} />)}
      {candidate.request.map(row => <Line key={row.label} label={row.label} value={row.value} />)}
      {PLANNED_SCOPES.map(scope => <Line key={scope.id} label={`${scope.id} · ${scope.label}`} value={scope.state} />)}
      {PLANNED_SOURCES.map(source => <Line key={source.id} label={`Source ${source.label}`} value={source.state} />)}
      <Notice>{candidate.available ? null : <>{MISSING_INTEGRATION} </>}The native offline manager (g07_candidate) is the only authority: a request becomes effective only after its native apply at the next desktop sync, and switching never grants approval, signing or sending.</Notice>
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
      <PublicRows rows={runtime} />
      {!runtime?.length ? <Line label="Runtime context" value="UNKNOWN" /> : null}
    </Section>
    <Notice>{launch?.reason || "No bound evidence"}. BUY control is not implemented. DRY, arm, probe and send remain unavailable. Filters only change the displayed fields; signed limits and execution rules remain bound to native evidence.</Notice>
  </>;
}

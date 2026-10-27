import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectLaunch, selectStatus, useBscStore } from "../store/bscStore";
import { ReadinessActions } from "../ReadinessActions";
import { EvidenceRows, PublicRows, ValueState } from "./EvidenceRows";
import { FilteredEvidence } from "./FilteredEvidence";
import { PlacedSection } from "./PlacedSection";
import { useNativeRecords } from "./useNativeRecords";
import { MISSING_INTEGRATION, candidateControlView } from "../model/candidate";
import { labelOf } from "../model/placement";
import { CandidateActions } from "../CandidateActions";
import { useCandidateControl } from "@/core/realtime/candidateControl";
import { buyLimitRow, economicRows, switchRow, type SwitchRow } from "../model/switches";
import { RUNTIME_CHOICES, SwitchActions } from "../SwitchActions";
import { BuyLimitActions } from "../BuyLimitActions";

/** The economic switches of one section, with what each permits and the request that set it. */
function EconomicSwitches({ rows, testId }: { rows: SwitchRow[]; testId: string }) {
  return <>
    {rows.map(row => <Line key={row.id} label={`${row.label} switch`} value={<ValueState state={row.effective} />}
      hint={`requested ${row.requested}${row.blockReason ? ` · ${row.blockReason}` : ""} · ${row.hint} · generation ${row.available ? row.generation.toString() : row.reason} · last set by ${row.receipt}`} />)}
    <SwitchActions rows={rows} testId={testId} action="bsc.economic.set" />
  </>;
}

const flag = (v: boolean | undefined) => v === undefined ? "UNKNOWN" : String(v);

/** Launch / BUY in the fixed §13 order: candidate → path → quote → DRY → V5 → budget → attempt → readiness. */
export function LaunchBuyPanel() {
  const quality = useTopic(TopicId.BSC_LAUNCH_STATE);
  useTopic(TopicId.BSC_MONITORING_STATE); // G0.7 hash bindings and attempt records
  const { projection: launch, status: launchQuality } = useBscStore(selectLaunch);
  const { projection: status, status: statusQuality } = useBscStore(selectStatus);
  const records = useNativeRecords();
  const { busy, awaiting } = useCandidateControl();
  // Locked from the click until the native mirror shows the recorded request (then its own state locks).
  const candidate = candidateControlView({ fields: launch?.runtimeFields, guardState: status?.guard?.guardState, statusOk: launchQuality === "LIVE" && statusQuality === "LIVE", busy: busy || awaiting !== null });
  const mirror = (launch?.runtimeFields ?? []).map(f => ({ name: labelOf(f.name, ""), value: f.value || "—", hint: f.name }));
  const r = status?.readiness;
  const g = status?.guard;
  const economic = economicRows(launch?.economicControls);
  const runtimeMode = switchRow("BSC_RUNTIME_MODE", launch?.runtimeMode);
  const limit = buyLimitRow(launch?.buyAttemptLimit);
  const pick = (...ids: string[]) => economic.filter(row => ids.includes(row.id));
  return <>
    <Line label="Plan source quality" value={<ValueState state={quality} />} hint="bsc.launch.state transport" />
    <PlacedSection panel="launch" section={1} records={records}>
      <Line label="Candidate control" value={<ValueState state={candidate.reason} />} />
      <CandidateActions view={candidate} />
      {candidate.rows.map(row => <Line key={row.label} label={row.label} value={row.value} />)}
      {candidate.readiness.map(row => <Line key={row.profile} label={`Readiness ${row.profile}`} value={<ValueState state={row.value} />} />)}
      {candidate.request.map(row => <Line key={row.label} label={row.label} value={row.value} />)}
      {mirror.length > 12 ? <FilteredEvidence rows={mirror} label="Filter candidate mirror" /> : <PublicRows rows={mirror} />}
      <Notice>{candidate.available ? null : <>{MISSING_INTEGRATION} </>}The native offline manager (g07_candidate) is the only authority: a request becomes effective only after its native apply at the next desktop sync, and switching never grants approval, signing or sending.</Notice>
    </PlacedSection>
    <PlacedSection panel="launch" section={2} records={records}>
      <Line label="Portal version bound → observed" value={`${r?.portalVersionBound || "—"} → ${r?.portalVersionObserved || "—"}`} />
      <Line label="Implementation bound → observed" value={`${r?.portalImplBound || "—"} → ${r?.portalImplObserved || "—"}`} />
      <Line label="Implementation code changed" value={flag(r?.codeHashChanged)} />
    </PlacedSection>
    <PlacedSection panel="launch" section={3} records={records} />
    <PlacedSection panel="launch" section={4} records={records}>
      <Line label="DRY result" value={<ValueState state={status?.dry?.state} />} />
      <Line label="Valid for current portal" value={flag(status?.dry?.validForCurrentPortal)} />
      <Line label="DRY run" value={status?.dry?.dryId || "—"} />
      <EvidenceRows evidence={status?.dry?.evidence} />
    </PlacedSection>
    <PlacedSection panel="launch" section={5} records={records}>
      <Line label="Compiled guard hash" value={g?.frozenApprovedSha256 || "—"} />
      <Line label="Contract" value={launch?.contractPresent ? "Present" : "No active V5 contract"} hint={g?.contractPath} />
      <Line label="Contract SHA-256" value={g?.contractSha256 || "—"} />
      <Line label="Contract note" value={launch?.reason || "—"} />
      <EvidenceRows evidence={launch?.evidence} />
      <EconomicSwitches rows={pick("BSC_ECON_SIGN", "BSC_ECON_ARM", "BSC_ECON_LIVE")} testId="bsc-economic-authorization" />
      <Line label="Runtime mode" value={<ValueState state={runtimeMode.effective} />}
        hint={`requested ${runtimeMode.requested}${runtimeMode.blockReason ? ` · ${runtimeMode.blockReason}` : ""} · ${runtimeMode.hint} · generation ${runtimeMode.available ? runtimeMode.generation.toString() : runtimeMode.reason} · last set by ${runtimeMode.receipt}`} />
      <SwitchActions rows={[runtimeMode]} testId="bsc-runtime-mode" action="bsc.runtime.mode.set" modes={RUNTIME_CHOICES} />
    </PlacedSection>
    <PlacedSection panel="launch" section={6} records={records} />
    <PlacedSection panel="launch" section={7} records={records}>
      <Line label="Attempt ID" value={g?.attemptId || "—"} />
      <Line label="Maximum attempts" value={g?.maxAttempts?.toString() ?? "UNKNOWN"} />
      <EconomicSwitches rows={pick("BSC_ECON_SEND", "BSC_ECON_ATTEMPT")} testId="bsc-economic-attempt" />
      <Line label="BUY attempts per UTC day" value={limit.available ? limit.perDay.toString() : limit.summary}
        hint={`${limit.summary} · generation ${limit.generation.toString()} · last set by ${limit.receipt} · every attempt that may send counts (sent or unclear too); 0 allows none; never a permission to execute`} />
      <BuyLimitActions row={limit} />
    </PlacedSection>
    <PlacedSection panel="launch" section={8} records={records}>
      <ReadinessActions />
      <Line label="Pre-Live Readiness" value={<ValueState state={r?.verdict} />} />
      <Line label="Readiness mode" value={<ValueState state={r?.mode || undefined} />} />
      <Line label="Selected candidate binding" value={<ValueState state={r?.candidateBinding || undefined} />} />
      <Line label="Head block" value={r?.headBlock?.toString() ?? "—"} />
      <EvidenceRows evidence={r?.evidence} />
      <FilteredEvidence rows={r?.checks} label="Filter readiness checks" />
    </PlacedSection>
    <Notice>No action here signs, arms, probes or sends. The economic switches are native permissions of the bsc_runtime owner (the same as darkbotctl bsc source request), DISABLED initially and after every restart; the G0.7 attempt needs both wallets, SIGN, ARM, SEND and Attempt ENABLED besides the compiled guard, the V5 contract and the attempt authority. Filters only change the displayed fields; signed limits and execution rules remain bound to native evidence.</Notice>
  </>;
}

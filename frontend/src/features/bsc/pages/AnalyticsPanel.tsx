import { useState } from "react";
import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectMonitoring, selectStatus, useBscStore } from "../store/bscStore";
import { EvidenceRows, timestamp, ValueState } from "./EvidenceRows";
import { FilteredEvidence } from "./FilteredEvidence";
import { PlacedSection } from "./PlacedSection";
import { useNativeRecords } from "./useNativeRecords";
import { shortSchema } from "../model/placement";

/**
 * Analytics in the fixed §13 order. Reports only: each native value has its primary place here or
 * in Monitoring, never both; no DRY result is shown as profit.
 */
export function AnalyticsPanel() {
  const { projection: status, status: sourceQuality } = useBscStore(selectStatus);
  const { projection: monitoring } = useBscStore(selectMonitoring);
  const monitoringQuality = useTopic(TopicId.BSC_MONITORING_STATE);
  useTopic(TopicId.BSC_LAUNCH_STATE); // record evidence of the qualification records
  const records = useNativeRecords();
  const [report, setReport] = useState("dry");
  const gate = (id: string) => status?.gates.find(g => g.gateId === id);
  const evidence = records.map(r => ({ name: shortSchema(r.schema), value: `${r.provenance} · ${r.evidence?.sha256 || "—"}`, hint: `${r.evidence?.path || "—"} · observed ${timestamp(r.evidence?.observedUnixS)}` }));
  return <>
    <PlacedSection panel="analytics" section={1} records={records}>
      <Line label="DRY run" value={status?.dry?.dryId || "—"} hint={status?.dry?.evidence?.path} />
      <Line label="G0.7 attempt evidence" value={gate("G0.7")?.evidence?.path || "NOT_EXECUTED"} />
      <Line label="G0.8 evidence" value={monitoring?.evidence?.path || "NOT_EXECUTED"} />
    </PlacedSection>
    <PlacedSection panel="analytics" section={2} records={records} />
    <PlacedSection panel="analytics" section={3} records={records}>
      <Line label="Qualified source-race latencies (p50/p95/p99, samples, clock)" value={<ValueState state="FUTURE_PHASE_BLOCKED" />} hint="No native measurement producer before C3 / Phase F" />
    </PlacedSection>
    <PlacedSection panel="analytics" section={4} records={records} />
    <PlacedSection panel="analytics" section={5} records={records} />
    <PlacedSection panel="analytics" section={6} records={records}>
      <Line label="G0.8 verdict" value={<ValueState state={monitoring?.state} />} hint="Acquisition proof in Monitoring; no realized PnL without original evidence" />
    </PlacedSection>
    <PlacedSection panel="analytics" section={7} records={records}>
      <div className="bsc-filter">
        <label htmlFor="bsc-report-source">Report</label>
        <select id="bsc-report-source" value={report} onChange={event => setReport(event.target.value)}>
          <option value="dry">Historical DRY</option>
          <option value="g08">G0.8 acquisition, ordering and costs</option>
        </select>
      </div>
      {report === "dry" ? <>
        <Line label="Source quality" value={sourceQuality} />
        <Line label="DRY result" value={<ValueState state={status?.dry?.state} />} hint="All DRY fields: Launch / BUY section 4" />
        <Line label="Current portal validity" value={status?.dry?.validForCurrentPortal === undefined ? "UNKNOWN" : String(status.dry.validForCurrentPortal)} />
        <EvidenceRows evidence={status?.dry?.evidence} />
      </> : <>
        <Line label="Source quality" value={monitoringQuality} />
        <Line label="Native verdict" value={<ValueState state={monitoring?.state} />} hint="All G0.8 fields: Monitoring sections 5–7 and the cost/coverage sections above" />
        <EvidenceRows evidence={monitoring?.evidence} />
      </>}
    </PlacedSection>
    <PlacedSection panel="analytics" section={8} records={records}>
      <FilteredEvidence rows={evidence} label="Filter evidence records" />
    </PlacedSection>
    <Notice>Latest provided evidence only. Filters affect this report view, not capture or BUY execution. Historical simulation is not live profit. Missing acquisition or fee evidence remains UNKNOWN. DRY cannot be started here.</Notice>
  </>;
}

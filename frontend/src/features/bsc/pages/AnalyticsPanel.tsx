import { useState } from "react";
import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line, Section } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectMonitoring, selectStatus, useBscStore } from "../store/bscStore";
import { EvidenceRows, ValueState } from "./EvidenceRows";
import { FilteredEvidence } from "./FilteredEvidence";

export function AnalyticsPanel() {
  const { projection: status, status: sourceQuality } = useBscStore(selectStatus);
  const { projection: monitoring } = useBscStore(selectMonitoring);
  const monitoringQuality = useTopic(TopicId.BSC_MONITORING_STATE);
  const [report, setReport] = useState("dry");
  return <>
    <div className="bsc-filter">
      <label htmlFor="bsc-report-source">Report</label>
      <select id="bsc-report-source" value={report} onChange={event => setReport(event.target.value)}>
        <option value="dry">Historical DRY</option>
        <option value="g08">G0.8 acquisition, ordering and costs</option>
      </select>
    </div>
    {report === "dry" ? <Section title="Historical DRY evidence">
      <Line label="Source quality" value={sourceQuality} />
      <Line label="DRY result" value={<ValueState state={status?.dry?.state} />} />
      <Line label="Current portal validity" value={status?.dry?.validForCurrentPortal === undefined ? "UNKNOWN" : String(status.dry.validForCurrentPortal)} />
      <EvidenceRows evidence={status?.dry?.evidence} />
      <FilteredEvidence key="dry" rows={status?.dry?.parameters} label="Filter DRY fields" />
    </Section> : <Section title="G0.8 acquisition, ordering and costs">
      <Line label="Source quality" value={monitoringQuality} />
      <Line label="Native verdict" value={<ValueState state={monitoring?.state} />} />
      <EvidenceRows evidence={monitoring?.evidence} />
      <FilteredEvidence key="g08" rows={monitoring?.results} label="Filter G0.8 fields" />
    </Section>}
    <Notice>Latest provided evidence only. Filters affect this report view, not capture or BUY execution. Historical simulation is not live profit. Missing acquisition or fee evidence remains UNKNOWN. DRY cannot be started here.</Notice>
  </>;
}

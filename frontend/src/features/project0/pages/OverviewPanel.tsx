import type { MonitorSnapshot, Project0Tab } from "../model/contracts";
import type { CaptureView } from "../model/status";
import type { TopicStatus } from "@/core/realtime";
import { DetailRow, Section, UnknownMetric } from "../components/PanelPrimitives";
import { usdMicroDisplay } from "../model/format";
import { fmtAgeNs, orDash } from "../model/statusFormat";
import { counterOf, phaseLabel } from "../model/statusSelectors";

export function OverviewPanel({ monitor, capture, topicStatus, authorization, goto }: { monitor: MonitorSnapshot; capture: CaptureView | null; topicStatus: TopicStatus; authorization: string; goto: (tab: Project0Tab) => void }) {
  const t = capture?.transport ?? null;
  const stream = t?.connected == null ? "Unknown"
    : t.connected ? `Connected · ${capture?.run?.wsHost ?? "—"}` : "Disconnected";
  return <><Section title="System status"><div className="p0-metrics">
    {monitor.rpcP95Ms != null
      ? <div className="p0-metric"><span>RPC latency · p95</span><b>{monitor.rpcP95Ms} ms</b></div>
      : <UnknownMetric label="RPC latency · p95" unit="ms"/>}
    {monitor.decisionP95Ms != null
      ? <div className="p0-metric"><span>Decision latency · p95</span><b>{monitor.decisionP95Ms} ms</b></div>
      : <UnknownMetric label="Decision latency · p95" unit="ms"/>}
    {t?.lastFrameAgeNs != null
      ? <div className="p0-metric"><span>Stream freshness</span><b>{fmtAgeNs(t.lastFrameAgeNs)}</b></div>
      : <UnknownMetric label="Stream freshness" unit="ms"/>}
    <div className="p0-metric"><span>Realized net PnL</span><b>{usdMicroDisplay(monitor.finance?.realizedNetPnlUsdMicro)}</b></div>
  </div><DetailRow title="Total capital · USD" value={usdMicroDisplay(monitor.finance?.totalCapitalUsdMicro)}/>
  <p className="p0-note">{monitor.finance ? `Finance ${monitor.finance.status} · slot ${monitor.finance.asOfSlot}. Unknown values stay —.` : capture ? `Last reported stream values · ${topicStatus}. No owner finance received.` : "No measured values. Click Health for detailed diagnostics."}</p></Section>
  <Section title="Liquidator" aside={<span className="p0-badge">Locked</span>}>
    <DetailRow title="Execution" value={`Disabled · ${authorization}`}/><DetailRow title="Available capital" value={usdMicroDisplay(monitor.finance?.availableCapitalUsdMicro)}/>
    <button className="p0-secondary" onClick={() => goto("liquidator")}>Open Liquidator →</button></Section>
  <Section className="p0-live-section" title="Capture" aside={<span className="p0-badge">{phaseLabel(capture)}</span>}>
    <DetailRow title="Market stream" value={stream}/>
    <DetailRow title="Unique transactions / recorded gaps" value={capture?.market ? `${orDash(counterOf(capture, "unique_transactions"))} / ${orDash(counterOf(capture, "gap_count"))}` : "— / —"}/>
    <button className="p0-secondary" onClick={() => goto("capture")}>Open Capture →</button></Section>
  <Section title="Analytics" aside={<span className="p0-quiet">No stored run loaded</span>}>
    <DetailRow title="Replay / Optimization" value="Offline jobs · Not connected"/>
    <button className="p0-secondary" onClick={() => goto("analytics")}>Open Analytics →</button></Section></>;
}

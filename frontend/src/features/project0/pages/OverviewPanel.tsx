import type { MonitorSnapshot, Project0Tab } from "../model/contracts";
import type { CaptureView } from "../model/status";
import { analyticsSummary, type AnalyticsView } from "../model/analytics";
import { programPinStatus } from "../model/owner";
import type { TopicStatus } from "@/core/realtime";
import { DetailRow, Section, UnknownMetric } from "../components/PanelPrimitives";
import { usdMicroDisplay } from "../model/format";
import { fmtAgeNs, orDash } from "../model/statusFormat";
import { counterOf, phaseLabel } from "../model/statusSelectors";

export function OverviewPanel({ monitor, financeStatus, capture, topicStatus, authorization, analytics, analyticsStatus, goto }: { monitor: MonitorSnapshot; financeStatus: TopicStatus; capture: CaptureView | null; topicStatus: TopicStatus; authorization: string; analytics: AnalyticsView | null; analyticsStatus: TopicStatus; goto: (tab: Project0Tab) => void }) {
  const pin = programPinStatus(monitor.programPin, monitor.programUpgradeLatched);
  const jobs = analyticsSummary(analytics, analyticsStatus);
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
  <p className="p0-note">{monitor.finance ? `Finance ${monitor.finance.status} · slot ${monitor.finance.asOfSlot} · ${financeStatus}. Unknown values stay —.` : capture ? `Last reported stream values · ${topicStatus}. Owner finance · ${financeStatus}.` : `No measured values · owner finance ${financeStatus}. Click Health for detailed diagnostics.`}</p></Section>
  <Section title="Liquidator" aside={<span className="p0-badge">Locked</span>}>
    <DetailRow title="Execution" value={`Disabled · ${authorization}`}/><DetailRow title="Available capital" value={usdMicroDisplay(monitor.finance?.availableCapitalUsdMicro)}/>
    <DetailRow title={`Program pin · ${pin.status}`} value={pin.detail}/>
    <button className="p0-secondary" onClick={() => goto("liquidator")}>Open Liquidator →</button></Section>
  <Section className="p0-live-section" title="Capture" aside={<span className="p0-badge">{phaseLabel(capture)}</span>}>
    <DetailRow title="Market stream" value={stream}/>
    <DetailRow title="Unique transactions / recorded gaps" value={capture?.market ? `${orDash(counterOf(capture, "unique_transactions"))} / ${orDash(counterOf(capture, "gap_count"))}` : "— / —"}/>
    <button className="p0-secondary" onClick={() => goto("capture")}>Open Capture →</button></Section>
  <Section title="Analytics" aside={<span className="p0-badge">{analyticsStatus}</span>}>
    <DetailRow title="Backtest / optimizer jobs" value={jobs}/>
    <button className="p0-secondary" onClick={() => goto("analytics")}>Open Analytics →</button></Section></>;
}

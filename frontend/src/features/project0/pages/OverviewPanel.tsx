import type { MonitorSnapshot, Project0Tab } from "../model/contracts";
import { DetailRow, Section, UnknownMetric } from "../components/PanelPrimitives";
import { usdMicroDisplay } from "../model/format";
export function OverviewPanel({monitor,goto}:{monitor:MonitorSnapshot;goto:(tab:Project0Tab)=>void}) {
 return <><Section title="System status"><div className="p0-metrics">
 <UnknownMetric label="RPC latency · p95" unit="ms"/><UnknownMetric label="Decision latency · p95" unit="ms"/>
 <UnknownMetric label="Stream freshness" unit="ms"/>
 <div className="p0-metric"><span>Realized net PnL</span><b>{usdMicroDisplay(monitor.finance?.realizedNetPnlUsdMicro)}</b></div>
 </div><DetailRow title="Total capital · USD" value={usdMicroDisplay(monitor.finance?.totalCapitalUsdMicro)}/>
 <p className="p0-note">No measured values. Click Health for detailed diagnostics.</p></Section>
 <Section title="Liquidator" aside={<span className="p0-badge">Locked</span>}>
 <DetailRow title="Execution" value="Disabled · Not authorized"/><DetailRow title="Available capital" value="Not connected"/>
 <button className="p0-secondary" onClick={()=>goto("liquidator")}>Open Liquidator →</button></Section>
 <Section title="Capture" aside={<span className="p0-badge">Stopped</span>}>
 <DetailRow title="Market stream" value="Not connected"/><DetailRow title="Raw events / open gaps" value="— / —"/>
 <button className="p0-secondary" onClick={()=>goto("capture")}>Open Capture →</button></Section>
 <Section title="Analytics" aside={<span className="p0-quiet">No stored run loaded</span>}>
 <DetailRow title="Replay / Optimization" value="Offline jobs · Not connected"/>
 <button className="p0-secondary" onClick={()=>goto("analytics")}>Open Analytics →</button></Section></>;
}
import { useEffect, useState } from "react";
import type { Project0Tab } from "./model/contracts";
import { UNCONNECTED_MONITOR } from "./model/contracts";
import { useFleetDrafts } from "./hooks/useFleetDrafts";
import { TabStrip } from "./components/PanelPrimitives";
import { HealthSection } from "./components/HealthSection";
import { ensureProject0Adapter } from "./api/project0Adapter";
import { OverviewPanel } from "./pages/OverviewPanel";
import { LiquidatorPanel } from "./pages/LiquidatorPanel";
import { CapturePanel } from "./pages/CapturePanel";
import { AnalyticsPanel } from "./pages/AnalyticsPanel";
import { WalletsPanel } from "./pages/WalletsPanel";
import "./styles/graphite.css";

/**
 * Feature composition only:
 *   - UI tabs and ephemeral draft edits stay here.
 *   - Rust owns real runs, economic decisions, wallets, authorizations and jobs.
 *   - The API boundary is intentionally NOT wired: no Project_0 control server yet.
 * Never turn this page into a second runtime / fake market data provider.
 */
const TABS: ReadonlyArray<{ value: Project0Tab; label: string }> = [
  { value: "overview", label: "Overview" },
  { value: "liquidator", label: "Liquidator" },
  { value: "capture", label: "Capture" },
  { value: "analytics", label: "Analytics" },
  { value: "wallets", label: "Wallets" },
];

export default function Project0Page() {
  const [tab, setTab] = useState<Project0Tab>("overview");
  const [healthOpen, setHealthOpen] = useState(false);
  const fleet = useFleetDrafts();
  useEffect(() => { ensureProject0Adapter(); }, []);

  return <div className="p0-graphite"><div className="p0-panel">
    <div className="p0-heading">
      <div><h1>Project 0 Control</h1><p>DarkBot · Marginfi Liquidator</p></div>
      <button type="button" className="p0-badge" aria-expanded={healthOpen}
        onClick={() => setHealthOpen(v => !v)}>● &nbsp; Health · Unknown &nbsp;›</button>
    </div>
    <TabStrip label="Project 0 areas" options={TABS} value={tab} onChange={setTab}/>
    {healthOpen ? <HealthSection/> : null}
    {tab === "overview" ? <OverviewPanel monitor={UNCONNECTED_MONITOR} goto={setTab}/> : null}
    {tab === "liquidator" ? <LiquidatorPanel fleet={fleet} goto={setTab}/> : null}
    {tab === "capture" ? <CapturePanel/> : null}
    {tab === "analytics" ? <AnalyticsPanel createDraft={fleet.create} goto={setTab}/> : null}
    {tab === "wallets" ? <WalletsPanel finance={UNCONNECTED_MONITOR.finance}/> : null}
    <footer className="p0-footer">Graphite V0 · Unconnected UI shell · No live submissions or transfers</footer>
  </div></div>;
}

import { useEffect, useState } from "react";
import type { Project0Tab } from "./model/contracts";
import { UNCONNECTED_MONITOR } from "./model/contracts";
import { useFleetDrafts } from "./hooks/useFleetDrafts";
import { TabStrip } from "./components/PanelPrimitives";
import { HealthSection } from "./components/HealthSection";
import { ensureProject0Adapter } from "./api/project0Adapter";
import { useNowMs } from "./hooks/useStatusClock";
import { executionAuthorization, healthLabel } from "./model/statusSelectors";
import { selectCapture, selectSystem, useProject0Store } from "./store/project0Store";
import { OverviewPanel } from "./pages/OverviewPanel";
import { LiquidatorPanel } from "./pages/LiquidatorPanel";
import { CapturePanel } from "./pages/CapturePanel";
import { AnalyticsPanel } from "./pages/AnalyticsPanel";
import { WalletsPanel } from "./pages/WalletsPanel";
import "./styles/graphite.css";
import "./styles/status.css";

/**
 * Feature composition only:
 * - UI tabs and ephemeral draft edits stay here.
 * - Rust owns real runs, economic decisions, wallets, authorizations and jobs.
 * - Read-only status arrives via the gateway; Capture reuses the one capture control client.
 * Never turn this page into a second runtime / fake market data provider.
 */
const TABS: ReadonlyArray<{ value: Project0Tab; label: string }> = [
  { value: "overview", label: "Overview" },
  { value: "liquidator", label: "Liquidator" },
  { value: "capture", label: "Capture" },
  { value: "analytics", label: "Analytics" },
  { value: "wallets", label: "Wallets" },
];

function HealthBadge({ open, onToggle }: { open: boolean; onToggle: () => void }) {
  const capture = useProject0Store(selectCapture);
  const progress = useProject0Store(s => s.streamProgress);
  const nowMs = useNowMs();
  const label = healthLabel(capture.topicStatus, capture.value, { nowMs, ...progress });
  return <button type="button" className="p0-badge" aria-expanded={open}
    onClick={onToggle}>● &nbsp; Health · {label} &nbsp;›</button>;
}

export default function Project0Page() {
  const [tab, setTab] = useState<Project0Tab>("overview");
  const [healthOpen, setHealthOpen] = useState(false);
  const fleet = useFleetDrafts();
  const capture = useProject0Store(selectCapture);
  const system = useProject0Store(selectSystem);
  const authorization = executionAuthorization(system.value, system.topicStatus);
  useEffect(() => { ensureProject0Adapter(); }, []);
  return <div className="p0-graphite"><div className="p0-panel">
    <div className="p0-heading">
      <div><h1>Project 0 Control</h1><p>DarkBot · Marginfi Liquidator</p></div>
      <HealthBadge open={healthOpen} onToggle={() => setHealthOpen(v => !v)}/>
    </div>
    <TabStrip label="Project 0 areas" options={TABS} value={tab} onChange={setTab}/>
    {healthOpen ? <HealthSection/> : null}
    {tab === "overview" ? <OverviewPanel monitor={UNCONNECTED_MONITOR} capture={capture.value} topicStatus={capture.topicStatus} authorization={authorization} goto={setTab}/> : null}
    {tab === "liquidator" ? <LiquidatorPanel fleet={fleet} authorization={authorization} goto={setTab}/> : null}
    {tab === "capture" ? <CapturePanel capture={capture.value} topicStatus={capture.topicStatus}/> : null}
    {tab === "analytics" ? <AnalyticsPanel createDraft={fleet.create} goto={setTab}/> : null}
    {tab === "wallets" ? <WalletsPanel finance={UNCONNECTED_MONITOR.finance}/> : null}
    <footer className="p0-footer">Graphite V0 · Read-only status via gateway · No live submissions or transfers</footer>
  </div></div>;
}

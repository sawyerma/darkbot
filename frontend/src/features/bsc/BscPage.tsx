import { statusTone } from "./model/status";
import "./bsc.css";

import { useEffect, useState } from "react";
import { GraphiteShell } from "@/shared/ui/graphite/GraphiteShell";
import { Tabs } from "@/shared/ui/graphite/Tabs";
import { StatusBadge, type NoticeState } from "@/shared/ui/graphite/Notice";
import { ensureBscAdapter } from "./api/bscAdapter";
import { selectGuardState, selectStatusQuality, useBscStore } from "./store/bscStore";
import { OverviewPanel } from "./pages/OverviewPanel";
import { LaunchBuyPanel } from "./pages/LaunchBuyPanel";
import { MonitoringPanel } from "./pages/MonitoringPanel";
import { AnalyticsPanel } from "./pages/AnalyticsPanel";
import { WalletsPanel } from "./pages/WalletsPanel";

/**
 * BSC Birth Entry – Feature-Komposition.
 * - Read-only projections and a separate readiness job; no BUY controller or signer.
 * - Die BSC-Engine (bsc/engine) ist alleinige Autorität für G0.7/G0.8, Guard und Wallets.
 * - bsc.status ist über die Route gebunden (projectRegistry); Panels binden ihre Topics selbst.
 */

export type BscTab = "overview" | "launch" | "monitoring" | "analytics" | "wallets";
export type BscViewMode = "basic" | "advanced";

const TABS: ReadonlyArray<{ key: BscTab; label: string }> = [
  { key: "overview", label: "Overview" },
  { key: "launch", label: "Launch / BUY" },
  { key: "monitoring", label: "Monitoring" },
  { key: "analytics", label: "Analytics" },
  { key: "wallets", label: "Wallets" },
];

function toNotice(topicStatus: string): NoticeState {
  switch (topicStatus) {
    // Transport-LIVE bedeutet nicht, dass Trading freigegeben ist.
    case "LIVE": return "READY";
    case "STALE": return "STALE";
    case "DETACHED": return "DETACHED";
    case "INCOMPLETE": return "INCOMPLETE";
    case "UNAVAILABLE":
    case "ERROR": return "BLOCKED";
    case "IDLE":
    case "SUBSCRIBING": return "NOT_CONNECTED";
    default: return "UNKNOWN";
  }
}

export default function BscPage() {
  const [tab, setTab] = useState<BscTab>("overview");
  const [viewMode, setViewMode] = useState<BscViewMode>("basic");
  const guard = useBscStore(selectGuardState);
  const statusTopic = useBscStore(selectStatusQuality);

  useEffect(() => {
    ensureBscAdapter();
  }, []);

  return (
    <GraphiteShell
      title="BSC Control"
      subtitle="DarkBot · Birth Entry"
      right={
        <span className="bsc-header-actions">
          <StatusBadge state={toNotice(statusTopic)} detail="Nur BSC-Datenverbindung, keine Trading-Freigabe" />
          <span className="g-badge bsc-state" data-tone={statusTone(guard)} data-testid="bsc-guard">Guard · {guard}</span>
          <span className="bsc-view-switch" role="group" aria-label="BSC detail level">
            <button type="button" aria-pressed={viewMode === "basic"} onClick={() => setViewMode("basic")}>Basic</button>
            <button type="button" aria-pressed={viewMode === "advanced"} onClick={() => setViewMode("advanced")}>Advanced</button>
          </span>
        </span>
      }
      footer="BSC · Readiness control and evidence · BUY execution unavailable"
    >
      <Tabs items={TABS} value={tab} onChange={setTab} ariaLabel="BSC areas" />
      <p className="bsc-view-description" role="status">{viewMode === "basic" ? "Übersicht: wichtige Zustände, Sperrgründe und verfügbare Bedienung." : "Technische Ansicht: native Evidenz, Prüffelder und vollständige Diagnosen."}</p>
      {tab === "overview" ? <OverviewPanel goto={setTab} viewMode={viewMode} /> : null}
      {tab === "launch" ? <LaunchBuyPanel viewMode={viewMode} /> : null}
      {tab === "monitoring" ? <MonitoringPanel viewMode={viewMode} /> : null}
      {tab === "analytics" ? <AnalyticsPanel viewMode={viewMode} /> : null}
      {tab === "wallets" ? <WalletsPanel viewMode={viewMode} /> : null}
    </GraphiteShell>
  );
}

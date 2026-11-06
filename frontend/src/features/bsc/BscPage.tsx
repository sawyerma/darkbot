// Graphite tokens (incl. the V8 palette) and base rows, formerly loaded through GraphiteShell.
import "@/shared/ui/graphite/graphite.css";
import "./bsc.css";

import { useCallback, useEffect, useMemo, useState } from "react";
import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { useRealtimeStatus } from "@/core/realtime/useRealtimeStatus";
import { ensureBscAdapter } from "./api/bscAdapter";
import { selectLive, selectStatus, useBscStore } from "./store/bscStore";
import { useNativeRecords } from "./pages/useNativeRecords";
import { healthView } from "./model/health";
import { Icon, IconSprite, type IconName } from "./pages/V8";
import { HealthPill, HealthWindow } from "./pages/HealthWindow";
import { SettingsDrawer } from "./pages/SettingsDrawer";
import { GuideDrawer } from "./pages/GuideDrawer";
import { OverviewPanel } from "./pages/OverviewPanel";
import { TrainingPanel } from "./pages/TrainingPanel";
import { LivePanel } from "./pages/LivePanel";
import { WalletsPanel } from "./pages/WalletsPanel";
import { useNowMs } from "./pages/useNowMs";

/**
 * BSC – Graphite V8 (06.11.2026, FE-V8-01): frontend/design/reference/bsc_graphite_v8_approved.html
 * in the existing React app. Four areas Overview | Training | Live | Wallets, the Health pill top
 * right with its own window, the settings drawer with the native controls and evidence the V8
 * areas do not show. Every value is the native owner's (bsc.* STATE topics); every control is a
 * request the owner applies or refuses; nothing is signed or sent by the browser.
 */
export type BscView = "overview" | "training" | "live" | "wallets";
const NAV: ReadonlyArray<{ id: BscView; title: string; icon: IconName }> = [
  { id: "overview", title: "Overview", icon: "home" },
  { id: "training", title: "Training", icon: "training" },
  { id: "live", title: "Live", icon: "live" },
  { id: "wallets", title: "Wallets", icon: "wallet" },
];
type Layer = "health" | "settings" | "guide" | null;

export default function BscPage() {
  const [view, setView] = useState<BscView>("overview");
  const [layer, setLayer] = useState<Layer>(null);
  useEffect(() => { ensureBscAdapter(); }, []);
  // The header's Health pill reads the runtime records and the LIVE owner on every area.
  useTopic(TopicId.BSC_MONITORING_STATE);
  useTopic(TopicId.BSC_LIVE_STATE);
  const gateway = useRealtimeStatus();
  const { projection: status, status: statusTopic } = useBscStore(selectStatus);
  const { projection: live } = useBscStore(selectLive);
  const records = useNativeRecords();
  const nowMs = useNowMs();
  const health = useMemo(() => healthView({ gateway, statusTopic, status, live, records, nowMs }), [gateway, statusTopic, status, live, records, nowMs]);
  const close = useCallback(() => setLayer(null), []);
  const goto = useCallback((v: BscView) => { setView(v); setLayer(null); window.scrollTo({ top: 0, behavior: "instant" }); }, []);
  const title = NAV.find(n => n.id === view)?.title ?? "Overview";

  return <div className="bsc-v8" data-testid="bsc-v8">
    <IconSprite />
    <div className="layout">
      <aside className="sidebar">
        <div className="brand"><div className="brandmark">D<span className="v8-brand-dot">·</span></div><div><strong>DarkBot</strong><span>BSC · Trading dashboard</span></div></div>
        <nav className="nav" aria-label="BSC areas">
          {NAV.map(n => <button key={n.id} type="button" className={view === n.id ? "selected" : undefined} aria-current={view === n.id ? "page" : undefined}
            data-testid={`bsc-nav-${n.id}`} onClick={() => goto(n.id)}><Icon name={n.icon} />{n.title}</button>)}
        </nav>
        <div className="sidebar-bottom"><button type="button" className="help" onClick={() => setLayer("guide")}><Icon name="info" size="s" />How it works</button></div>
      </aside>
      <div className="main">
        <header className="top">
          <div className="path"><span>DarkBot</span><Icon name="chevron" size="s" /><span>BSC</span><Icon name="chevron" size="s" /><strong>{title}</strong></div>
          <div className="top-right">
            <HealthPill view={health} onOpen={() => setLayer("health")} />
            <button type="button" className="round" title="Settings" aria-label="Settings" data-testid="bsc-settings-open" onClick={() => setLayer("settings")}><Icon name="settings" size="s" /></button>
          </div>
        </header>
        <section className="wrap" id="bsc-content" aria-label="BSC area">
          {view === "overview" ? <OverviewPanel goto={goto} /> : null}
          {view === "training" ? <TrainingPanel goto={goto} /> : null}
          {view === "live" ? <LivePanel /> : null}
          {view === "wallets" ? <WalletsPanel /> : null}
        </section>
      </div>
    </div>
    {layer === "health" ? <HealthWindow view={health} onClose={close} /> : null}
    {layer === "settings" ? <SettingsDrawer onClose={close} /> : null}
    {layer === "guide" ? <GuideDrawer onClose={close} /> : null}
  </div>;
}

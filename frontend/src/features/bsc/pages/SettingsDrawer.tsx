import { TopicId } from "@/core/realtime";
import { useTopicStatus } from "@/core/realtime/hooks";
import { selectStatus, useBscStore } from "../store/bscStore";
import { timestamp } from "./EvidenceRows";
import { DetailLine, Drawer, StateBadge } from "./V8";
import { SystemEvidence } from "./SystemEvidence";
import { LaunchBuyPanel } from "./LaunchBuyPanel";
import { MonitoringPanel } from "./MonitoringPanel";
import { AnalyticsPanel } from "./AnalyticsPanel";

const TOPICS: ReadonlyArray<[TopicId, string, string]> = [
  [TopicId.BSC_STATUS, "System", "BscStatus · bsc.status"],
  [TopicId.BSC_LAUNCH_STATE, "Launches", "BscLaunchState · bsc.launch.state"],
  [TopicId.BSC_MONITORING_STATE, "Monitoring", "BscMonitoringState · bsc.monitoring.state"],
  [TopicId.BSC_WALLETS_STATE, "Wallets", "BscWalletsState · bsc.wallets.state"],
  [TopicId.BSC_LIVE_STATE, "Live", "BscLiveState · bsc.live.state"],
];

function TopicLine({ topic, name, wire }: { topic: TopicId; name: string; wire: string }) {
  const status = useTopicStatus(topic);
  return <DetailLine label={`${name} · ${wire}`} value={<StateBadge state={status === "LIVE" ? "RUNNING" : status} label={status} />} testId={`bsc-topic-${topic}`} />;
}

/**
 * The reference's Settings drawer ("Data connections"), in production also the home of every
 * native control and evidence section the four V8 areas do not show (FE-V8-01: moved, never
 * deleted): the system and evidence sections, the G0.7 readiness and candidate controls with the
 * economic switches and the runtime mode, the source/venue/component modes and runtime detail,
 * and the reports. Each section keeps its native placement (bsc-fields.json) and its owner request.
 */
export function SettingsDrawer({ onClose }: { onClose: () => void }) {
  const { projection: s } = useBscStore(selectStatus);
  return <Drawer title="Settings" kicker="DATA CONNECTIONS · NATIVE CONTROLS" wide onClose={onClose} testId="bsc-settings"
    footer={<button type="button" className="btn primary" onClick={onClose}>Close</button>}>
    <p className="small">The frontend reads the authorized Protobuf status topics of the gateway; every switch below is a request the native owner applies or refuses.</p>
    <div className="subcard mt">
      {TOPICS.map(([t, name, wire]) => <TopicLine key={t} topic={t} name={name} wire={wire} />)}
      <DetailLine label="Source commit" value={s?.sourceCommit || "—"} />
      <DetailLine label="Data as of" value={timestamp(s?.dataAsOfUnixS)} />
    </div>
    <p className="tiny mt">Historical trade pages, training profiles and wallet transfers use the bounded query of the gateway once their native owners answer (BSC-V8-01…17).</p>
    <section className="v8-section-group" data-testid="bsc-settings-system"><h3>System &amp; evidence</h3><SystemEvidence /></section>
    <section className="v8-section-group" data-testid="bsc-settings-launch"><h3>Launch / BUY · G0.7 readiness and switches</h3><LaunchBuyPanel /></section>
    <section className="v8-section-group" data-testid="bsc-settings-monitoring"><h3>Sources, venues, components and runtime</h3><MonitoringPanel sourceControls={false} /></section>
    <section className="v8-section-group" data-testid="bsc-settings-reports"><h3>Reports &amp; evidence records</h3><AnalyticsPanel /></section>
  </Drawer>;
}

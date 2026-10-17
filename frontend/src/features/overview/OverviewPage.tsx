import { useEffect } from "react";
import { PROJECT_REGISTRY } from "@/app/projectRegistry";
import { useRealtimeStatus } from "@/core/realtime/useRealtimeStatus";
import { useGatewayReason } from "@/core/realtime/hooks";
import { GraphiteShell } from "@/shared/ui/graphite/GraphiteShell";
import { Line, Section, DASH } from "@/shared/ui/graphite/Rows";
import { Notice, StatusBadge, type NoticeState } from "@/shared/ui/graphite/Notice";
import { ensureOverviewAdapter } from "./api/overviewAdapter";
import { ProjectEntry } from "./ProjectEntry";
import { selectComputedAt, selectConnected, selectGlobal, selectReason, selectStatus, useOverviewStore } from "./overviewStore";
import { formatAge, formatUsdMicro, formatUsdMicroSigned } from "./model/format";
import "./overview.css";

// Globale Overview: abonniert ausschließlich overview.aggregates (über die Route gebunden).
// Keine Wallet-, Bot- oder Orderstreams. Keine globalen Start/BUY/Send-Funktionen.

const CARDS = PROJECT_REGISTRY.filter((p) => p.key !== "overview");

function gatewayNotice(s: string): NoticeState {
  switch (s) {
    case "OPEN": return "READY";
    case "STALE": return "STALE";
    case "BLOCKED": return "BLOCKED";
    case "CONNECTING":
    case "HANDSHAKE": return "NOT_CONNECTED";
    default: return "NOT_CONNECTED";
  }
}

export default function OverviewPage() {
  const gatewayStatus = useRealtimeStatus();
  const gatewayReason = useGatewayReason();
  const topicStatus = useOverviewStore(selectStatus);
  const topicReason = useOverviewStore(selectReason);
  const fresh = topicStatus === "LIVE";
  const global = useOverviewStore(selectGlobal);
  const connected = useOverviewStore(selectConnected);
  const computedAt = useOverviewStore(selectComputedAt);

  useEffect(() => {
    ensureOverviewAdapter();
  }, []);

  return (
    <GraphiteShell
      title="DarkBot Overview"
      subtitle="One frontend · isolated backend domains"
      right={<StatusBadge state={gatewayNotice(gatewayStatus)} detail={gatewayReason || undefined} />}
      footer="Read-only aggregates · no global trading controls"
    >
      <Section title="Projects">
        <div className="ov-cards">
          {CARDS.map((reg) => (
            <ProjectEntry key={reg.key} reg={reg} />
          ))}
        </div>
      </Section>

      <Section title="Global finance">
        {fresh && global?.available ? (
          <>
            <Line label="Net PnL (all projects)" value={formatUsdMicroSigned(global.netPnlUsdMicro)} />
            <Line label="Total capital" value={formatUsdMicro(global.totalCapitalUsdMicro)} />
          </>
        ) : (
          <Line label="Aggregate" value="Aggregate unavailable" hint={fresh ? (global?.reason || "no compatible finalized valuation across projects") : "topic not current"} />
        )}
      </Section>

      <Section title="Control plane">
        <Line label="Gateway" value={gatewayStatus} hint={gatewayReason || undefined} />
        <Line label="overview.aggregates" value={topicStatus} hint={topicReason || undefined} />
        <Line label="Projects connected" value={!fresh || connected === null ? DASH : String(connected)} />
        <Line label="Computed" value={fresh ? formatAge(Date.now(), computedAt) : DASH} />
      </Section>

      <Notice>
        Values appear only from qualified backend sources. Unknown is shown as {DASH}, never as zero. Readiness is not a LIVE authorization.
      </Notice>
    </GraphiteShell>
  );
}

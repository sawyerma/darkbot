import { Fragment, useEffect } from "react";
import { PROJECT_REGISTRY } from "@/app/projectRegistry";
import { useRealtimeStatus } from "@/core/realtime/useRealtimeStatus";
import { useGatewayReason } from "@/core/realtime/hooks";
import { GraphiteShell } from "@/shared/ui/graphite/GraphiteShell";
import { Section } from "@/shared/ui/graphite/Rows";
import { StatusBadge, type NoticeState } from "@/shared/ui/graphite/Notice";
import { ensureOverviewAdapter } from "./api/overviewAdapter";
import { ProjectEntry } from "./ProjectEntry";
import { selectReason, selectStatus, selectView, useOverviewStore } from "./overviewStore";
import { financeRows, systemRows, type OverviewRow } from "./model/overviewRows";
import { useNowMs } from "./useNowMs";
import "./overview.css";

// Overview subscribes only to overview.aggregates. No wallet/order streams or global trading controls.
const CARDS = PROJECT_REGISTRY.filter((project) => project.key !== "overview");

function gatewayNotice(status: string): NoticeState {
  switch (status) {
    case "OPEN": return "READY";
    case "STALE": return "STALE";
    case "BLOCKED": return "BLOCKED";
    case "CONNECTING":
    case "HANDSHAKE": return "NOT_CONNECTED";
    default: return "NOT_CONNECTED";
  }
}

function OverviewTile({ title, rows, testId }: { title: string; rows: readonly OverviewRow[]; testId: string }) {
  return (
    <section className="ov-tile" aria-label={title} data-testid={testId}>
      <h3>{title}</h3>
      <dl className="ov-facts">
        {rows.map((row) => (
          <Fragment key={row.label}>
            <dt>{row.label}</dt>
            <dd>{row.value}{row.note ? <small className="ov-note">{row.note}</small> : null}</dd>
          </Fragment>
        ))}
      </dl>
    </section>
  );
}

function SystemTile() {
  const gatewayStatus = useRealtimeStatus();
  const gatewayReason = useGatewayReason();
  const topicStatus = useOverviewStore(selectStatus);
  const topicReason = useOverviewStore(selectReason);
  const view = useOverviewStore(selectView);
  const nowMs = useNowMs();
  return <OverviewTile title="System & Control Plane" testId="overview-system"
    rows={systemRows({ gatewayStatus, gatewayReason, topicStatus, topicReason,
      view, projectCount: CARDS.length, nowMs })} />;
}

function FinanceTile() {
  const topicStatus = useOverviewStore(selectStatus);
  const view = useOverviewStore(selectView);
  return <OverviewTile title="Global Finance" testId="overview-finance"
    rows={financeRows({ topicStatus, view, projectCount: CARDS.length })} />;
}

export default function OverviewPage() {
  const gatewayStatus = useRealtimeStatus();
  const gatewayReason = useGatewayReason();

  useEffect(() => { ensureOverviewAdapter(); }, []);

  return (
    <GraphiteShell
      title="DarkBot Overview"
      subtitle="One frontend · isolated backend domains"
      right={<StatusBadge state={gatewayNotice(gatewayStatus)} detail={gatewayReason || undefined} />}
      footer="Read-only aggregates · no global trading controls"
    >
      <Section title="Projects">
        <div className="ov-cards">
          {CARDS.map((project) => <ProjectEntry key={project.key} reg={project} />)}
        </div>
      </Section>
      <div className="ov-tiles">
        <SystemTile />
        <FinanceTile />
      </div>
    </GraphiteShell>
  );
}

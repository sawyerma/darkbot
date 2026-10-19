import { Fragment, useEffect, type ReactNode } from "react";
import { PROJECT_REGISTRY } from "@/app/projectRegistry";
import { useRealtimeStatus } from "@/core/realtime/useRealtimeStatus";
import { useGatewayReason } from "@/core/realtime/hooks";
import { GraphiteShell } from "@/shared/ui/graphite/GraphiteShell";
import { Section } from "@/shared/ui/graphite/Rows";
import { StatusBadge, type NoticeState } from "@/shared/ui/graphite/Notice";
import { ensureOverviewAdapter } from "./api/overviewAdapter";
import { ProjectEntry } from "./ProjectEntry";
import { DisabledAction } from "./OverviewControls";
import { selectReason, selectStatus, selectView, useOverviewStore } from "./overviewStore";
import { financeRows, systemRows, type OverviewRow } from "./model/overviewRows";
import { GLOBAL_CONTROLS } from "./model/lifecycle";
import { useNowMs } from "./useNowMs";
import "./overview.css";

// Overview subscribes only to overview.aggregates. No wallet/order streams, no trading controls.
// Layout per FRONTEND_PHASENPLAN §10 (F7-OVR): three project cards, two equal tiles below.
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

function OverviewTile({ title, rows, testId, note, actions }: {
  title: string; rows: readonly OverviewRow[]; testId: string; note?: string; actions?: ReactNode;
}) {
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
      {note ? <p className="ov-tile-note">{note}</p> : null}
      {actions ? <div className="ov-tile-actions" role="group" aria-label={`${title} actions`}>{actions}</div> : null}
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
  return (
    <OverviewTile title="System & Control Plane" testId="overview-system"
      rows={systemRows({ gatewayStatus, gatewayReason, topicStatus, topicReason, view, projectCount: CARDS.length, nowMs })}
      actions={<>
        <DisabledAction label="Stop All" control={GLOBAL_CONTROLS.stopAll} testId="overview-stop-all" />
        <DisabledAction label="Show All Logs" control={GLOBAL_CONTROLS.showAllLogs} testId="overview-show-all-logs" />
      </>} />
  );
}

function FinanceTile() {
  const topicStatus = useOverviewStore(selectStatus);
  const view = useOverviewStore(selectView);
  return (
    <OverviewTile title="Global Finance Aggregate" testId="overview-finance"
      rows={financeRows({ topicStatus, view, projectCount: CARDS.length })}
      note="Only qualified, non-duplicated finance data is eligible." />
  );
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

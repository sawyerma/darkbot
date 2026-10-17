import { useNavigate } from "react-router-dom";
import type { ProjectRegistration } from "@/app/projectRegistry";
import { StatusBadge, type NoticeState } from "@/shared/ui/graphite/Notice";
import { DASH } from "@/shared/ui/graphite/Rows";
import { selectProject, useOverviewStore } from "./overviewStore";
import { formatCount, formatLatency, formatUsdMicroSigned } from "./model/format";

// Eine Projektkarte aus der Registry. Daten ausschließlich aus overview.aggregates;
// ohne Daten bleibt alles "—". Klick öffnet die Projektroute.

function sourceNotice(s: string | undefined): NoticeState {
  switch (s) {
    case "CONNECTED": return "READY";
    case "STALE": return "STALE";
    case "DETACHED": return "DETACHED";
    case "NOT_CONNECTED": return "NOT_CONNECTED";
    default: return "UNKNOWN";
  }
}

export function ProjectEntry({ reg }: { reg: ProjectRegistration }) {
  const navigate = useNavigate();
  const quality = useOverviewStore(s => s.status);
  const cached = useOverviewStore(selectProject(reg.projectId));
  const summary = quality === "LIVE" ? cached : null;

  return (
    <button
      type="button"
      className="ov-card"
      onClick={() => navigate(reg.route)}
      aria-label={`Open ${reg.label}`}
      data-testid={`project-card-${reg.key}`}
    >
      <div className="g-row">
        <h3>{reg.label}</h3>
        <StatusBadge state={sourceNotice(summary?.source)} detail={summary?.sourceId} />
      </div>
      <p>{summary?.domain ?? DASH}</p>
      <dl className="ov-facts">
        <dt>Readiness</dt>
        <dd>{summary?.readiness ?? DASH}</dd>
        <dt>Execution authorization</dt>
        <dd>{summary?.liveAuthorized == null ? DASH : summary.liveAuthorized ? "Engine authorized" : "Not authorized"}</dd>
        <dt>Bots / jobs / captures</dt>
        <dd>{`${formatCount(summary?.activeBots ?? null)} / ${formatCount(summary?.activeJobs ?? null)} / ${formatCount(summary?.activeCaptures ?? null)}`}</dd>
        <dt>RPC p95</dt>
        <dd>{formatLatency(summary?.rpcLatency ?? null)}</dd>
        <dt>Realized net PnL</dt>
        <dd>{formatUsdMicroSigned(summary?.finance?.realizedNetPnlUsdMicro ?? null)}</dd>
      </dl>
      {summary && summary.warnings.length > 0 ? (
        <p className="ov-warn">{summary.warnings.slice(0, 2).join(" · ")}</p>
      ) : null}
    </button>
  );
}

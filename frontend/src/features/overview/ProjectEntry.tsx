import {useReadinessControl} from "@/core/realtime/readinessControl";
import { useCaptureControl } from "@/core/realtime/captureControl";
import { useId } from "react";
import { Link } from "react-router-dom";
import type { ProjectRegistration } from "@/app/projectRegistry";
import { StatusBadge, type NoticeState } from "@/shared/ui/graphite/Notice";
import { DASH } from "@/shared/ui/graphite/Rows";
import { selectProject, selectStatus, useOverviewStore } from "./overviewStore";
import { formatCount, formatLatency, formatUsdMicroSigned } from "./model/format";
import { captureIndicator, processIndicator, projectControls } from "./model/lifecycle";
import { CommandAction, DisabledAction, LogsAction, ProcessLine } from "./OverviewControls";

// One project card from the registry. Metrics come only from overview.aggregates; without data
// everything stays "—". Navigation is a link in the title; lifecycle controls are sibling buttons,
// never nested in the link and never triggered by navigating.

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
  const quality = useOverviewStore(selectStatus);
  const cached = useOverviewStore(selectProject(reg.projectId));
  const summary = quality === "LIVE" ? cached : null;
  const titleId = useId();
  // Capture lifecycle comes from launchd, independently of the reporter feed.
  const capture = useCaptureControl(s => s.process);
  const readiness = useReadinessControl(s=>s.process);
  const owned = reg.key==="bsc"?readiness:capture;
  const process = reg.key === "project0" ? captureIndicator(capture) : reg.key==="bsc"?captureIndicator(readiness):processIndicator(null);
  const controls = projectControls(reg.key, process);

  return (
    <article className="ov-card" aria-labelledby={titleId} data-testid={`project-card-${reg.key}`} data-project={reg.key} data-capture-state={process.state}>
      <div className="ov-card-head">
        <h3 id={titleId}>
          <Link to={reg.route} title={`Open ${reg.label}`} data-testid={`project-link-${reg.key}`}>{reg.label}</Link>
        </h3>
        <StatusBadge state={(reg.key === "project0" || reg.key==="bsc") && process.state === "FAILED" ? "FAILED" : sourceNotice(summary?.source)} detail={(reg.key === "project0" || reg.key==="bsc") && process.state === "FAILED" ? process.reason : summary?.sourceId} />
      </div>
      <p className="ov-domain">{summary?.domain ?? DASH}</p>
      <ProcessLine indicator={process} testId={`project-process-${reg.key}`} />
      <dl className="ov-facts">
        <dt>Readiness</dt>
        <dd>{summary?.readiness ?? DASH}</dd>
        <dt>Execution authorization</dt>
        <dd>{summary?.liveAuthorized == null ? DASH : summary.liveAuthorized ? "Engine authorized" : "Not authorized"}</dd>
        <dt>Bots / jobs / captures</dt>
        <dd>{`${formatCount(summary?.activeBots ?? null)} / ${formatCount(summary?.activeJobs ?? null)} / ${formatCount(reg.key === "project0" ? (["RUNNING", "STOPPING"].includes(process.state) ? 1 : ["STOPPED", "FAILED"].includes(process.state) ? 0 : null) : summary?.activeCaptures ?? null)}`}</dd>
        <dt>RPC p95</dt>
        <dd>{formatLatency(summary?.rpcLatency ?? null)}</dd>
        <dt>Realized net PnL</dt>
        <dd>{formatUsdMicroSigned(summary?.finance?.realizedNetPnlUsdMicro ?? null)}</dd>
      </dl>
      {summary && summary.warnings.length > 0 ? (
        <p className="ov-warn">{summary.warnings.slice(0, 2).join(" · ")}</p>
      ) : null}
      <div className="ov-actions" role="group" aria-label={`${reg.label} controls`}>
        {controls.startStop.enabled && owned?.controller_available
          ? <CommandAction control={controls.startStop} testId={`project-startstop-${reg.key}`} />
          : <DisabledAction label="Start / Stop" control={{ enabled: false, reason: "No available controller or capture is stopping" }} testId={`project-startstop-${reg.key}`} />}
        {controls.logs.enabled ? <LogsAction label="Logs" source={reg.key==="bsc"?"bsc":"project0"} testId={`project-logs-${reg.key}`} /> : <DisabledAction label="Logs" control={{ enabled: false, reason: controls.logs.reason }} testId={`project-logs-${reg.key}`} />}
      </div>
      {reg.key === "project0" ? <p className="ov-note">Gateway läuft unabhängig weiter</p> : null}
    </article>
  );
}

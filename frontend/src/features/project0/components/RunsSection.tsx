import type { TopicStatus } from "@/core/realtime";
import type { AnalyticsView } from "../model/analytics";
import { DetailRow, Section } from "./PanelPrimitives";

/**
 * A12/F-03: the recorded runs with their post-capture state as the native records hold it
 * (project0.analytics.jobs): layout, capture completion and the newest qualification record.
 * Nothing is derived from file existence here; a run without a record shows none.
 */
export function RunsSection({ analytics, topicStatus }: { analytics: AnalyticsView | null; topicStatus: TopicStatus }) {
  const runs = analytics?.runs ?? [];
  return <Section title="Recorded runs · post-capture" aside={<span className="p0-badge">{topicStatus}</span>}>
    {topicStatus !== "LIVE" && analytics ? <p className="p0-quiet">Last reported runs ({topicStatus}); not current.</p> : null}
    {runs.length === 0 ? <p className="p0-quiet">{analytics ? "No runs under the project's run roots." : "No run list received (a Project 0 process publishes it)."}</p> : null}
    {runs.map(r => {
      const q = r.qualification;
      const qualification = q == null ? "no qualification record"
        : q.error ? `qualification unreadable · ${q.error}`
        : `${q.verdict} · verification ${q.verification}${q.failed.length ? ` · failed ${q.failed.join(", ")}` : ""}`;
      return <DetailRow key={`${r.root}/${r.runId}`} title={`${r.runId}${r.running ? " · running" : ""}`}
        value={`${r.layout} · ${r.captureComplete ? "capture complete" : "capture not complete"} · ${qualification}`}/>;
    })}
  </Section>;
}

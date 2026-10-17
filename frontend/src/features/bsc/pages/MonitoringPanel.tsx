import { StartPolicy, TopicId } from "@/core/realtime";
import { useTopic, useTopicReason } from "@/core/realtime/hooks";
import { Line, Section, orDash } from "@/shared/ui/graphite/Rows";
import { Notice, StatusBadge, type NoticeState } from "@/shared/ui/graphite/Notice";
import { formatUnixNs, formatWei, shortId } from "../model/format";
import { selectEvents, selectMonitoring, useBscStore } from "../store/bscStore";

const EVENT_OPTS = { startPolicy: StartPolicy.LIVE_FROM_BOUNDARY };

function feedNotice(status: string): NoticeState {
  if (status === "LIVE") return "LIVE";
  if (status === "CATCHING_UP") return "STALE";
  if (status === "INCOMPLETE") return "INCOMPLETE";
  if (status === "UNAVAILABLE" || status === "ERROR") return "BLOCKED";
  return "NOT_CONNECTED";
}

export function MonitoringPanel() {
  useTopic(TopicId.BSC_MONITORING_STATE);
  const feedStatus = useTopic(TopicId.BSC_EXECUTION_EVENTS, EVENT_OPTS);
  const feedReason = useTopicReason(TopicId.BSC_EXECUTION_EVENTS);
  const { projection: m } = useBscStore(selectMonitoring);
  const events = useBscStore(selectEvents);

  return (
    <>
      <Section title="Chain evidence">
        <Line label="Witness" value={orDash(m?.witness)} />
        <Line label="Chain reconcile" value={m?.chainReconcile ?? "—"} />
        <Line label="Receipts" value={m?.receiptsCount === null || m?.receiptsCount === undefined ? "—" : String(m.receiptsCount)} />
        <Line label="Canonical ordering" value={orDash(m?.canonicalOrdering)} />
        <Line label="Fees observed" value={formatWei(m?.feesObservedWei ?? null)} />
        <Line label="Last inclusion block" value={orDash(m?.lastInclusionBlock)} />
      </Section>

      <Section title="Execution events">
        <Line
          label="Feed"
          value={<StatusBadge state={feedNotice(feedStatus)} detail={feedReason} />}
          hint={feedReason || undefined}
        />
        {events.items.length === 0 ? (
          <Line label={feedStatus === "LIVE" ? "No events received" : "Event stream not qualified"} value="—" />
        ) : (
          events.items.slice(0, 50).map((row) => (
            <Line
              key={`${row.meta.epoch}:${row.meta.cursor.toString()}:${row.meta.eventId}`}
              label={
                <>
                  <span className="g-badge">{row.phase}</span> {shortId(row.meta.eventId)}
                </>
              }
              value={formatUnixNs(row.meta.committedAtUnixNs)}
              hint={`cursor ${row.meta.cursor.toString()} · ${shortId(row.meta.evidenceRef, 10, 4)}`}
            />
          ))
        )}
      </Section>

      <Notice>
        Events are displayed only after the engine's durable commit. ARCHIVE marks journal catch-up; LIVE marks the stream after CatchupComplete. INCOMPLETE is never shown as complete.
      </Notice>
    </>
  );
}

import { ProjectId, TopicClass, TopicId, type TopicDescriptor } from "../generated/darkbot/control/v1/topics_pb";

// Compile-Time-Bindung Topic -> Klasse/Projekt/Payload. Muss mit der TopicDescriptor-Tabelle
// übereinstimmen, die das Gateway im Welcome sendet. payloadType bleibt leer, bis F0-VAR die
// Domänenprojektion liefert und der Adapter sie decodieren kann; solange ist available=false.

export interface TopicBinding {
  readonly topic: TopicId;
  readonly wireName: string;
  readonly cls: TopicClass;
  readonly project: ProjectId;
  readonly payloadType: string;
  readonly available: boolean;
}

const b = (
  topic: TopicId,
  wireName: string,
  cls: TopicClass,
  project: ProjectId,
  payloadType = "",
): TopicBinding => ({ topic, wireName, cls, project, payloadType, available: payloadType !== "" });

export const TOPIC_TABLE: readonly TopicBinding[] = [
  b(TopicId.OVERVIEW_AGGREGATES, "overview.aggregates", TopicClass.STATE, ProjectId.UNSPECIFIED, "darkbot.control.v1.OverviewAggregates"),

  b(TopicId.PROJECT0_STATUS, "project0.status", TopicClass.STATE, ProjectId.PROJECT_0, "darkbot.project0.v1.SystemStatus"),
  b(TopicId.PROJECT0_FINANCE_SUMMARY, "project0.finance.summary", TopicClass.STATE, ProjectId.PROJECT_0),
  b(TopicId.PROJECT0_LIQUIDATOR_STATE, "project0.liquidator.state", TopicClass.STATE, ProjectId.PROJECT_0),
  b(TopicId.PROJECT0_CAPTURE_STATE, "project0.capture.state", TopicClass.STATE, ProjectId.PROJECT_0, "darkbot.project0.v1.CaptureState"),
  b(TopicId.PROJECT0_WALLETS_STATE, "project0.wallets.state", TopicClass.STATE, ProjectId.PROJECT_0),
  b(TopicId.PROJECT0_ANALYTICS_JOBS, "project0.analytics.jobs", TopicClass.STATE, ProjectId.PROJECT_0),
  b(TopicId.PROJECT0_EXECUTION_EVENTS, "project0.execution.events", TopicClass.EVENT, ProjectId.PROJECT_0),
  b(TopicId.PROJECT0_CAPTURE_EVENTS, "project0.capture.events", TopicClass.EVENT, ProjectId.PROJECT_0),

  b(TopicId.BSC_STATUS, "bsc.status", TopicClass.STATE, ProjectId.BSC),
  b(TopicId.BSC_LAUNCH_STATE, "bsc.launch.state", TopicClass.STATE, ProjectId.BSC),
  b(TopicId.BSC_MONITORING_STATE, "bsc.monitoring.state", TopicClass.STATE, ProjectId.BSC),
  b(TopicId.BSC_WALLETS_STATE, "bsc.wallets.state", TopicClass.STATE, ProjectId.BSC),
  b(TopicId.BSC_EXECUTION_EVENTS, "bsc.execution.events", TopicClass.EVENT, ProjectId.BSC),

  b(TopicId.SOLANA_STATUS, "solana.status", TopicClass.STATE, ProjectId.SOLANA),
];

const BY_ID = new Map<TopicId, TopicBinding>(TOPIC_TABLE.map((t) => [t.topic, t]));

export function bindingOf(topic: TopicId): TopicBinding | undefined {
  return BY_ID.get(topic);
}

export function wireNameOf(topic: TopicId): string {
  return BY_ID.get(topic)?.wireName ?? `topic#${topic}`;
}

export type DescriptorCheck =
  | { ok: true }
  | { ok: false; reason: "UNKNOWN_TOPIC" | "CLASS_MISMATCH" | "PROJECT_MISMATCH" | "PAYLOAD_TYPE_MISMATCH" | "SERVER_UNAVAILABLE" | "CLIENT_UNAVAILABLE" };

// Prüft einen Server-Descriptor gegen die lokale Tabelle. Jede Abweichung macht das Topic
// lokal UNAVAILABLE; es wird dann nie abonniert (kein Fake-Adapter, kein Subscribe-Loop).
export function checkDescriptor(d: TopicDescriptor): DescriptorCheck {
  const local = BY_ID.get(d.topic);
  if (!local) return { ok: false, reason: "UNKNOWN_TOPIC" };
  if (local.cls !== d.class) return { ok: false, reason: "CLASS_MISMATCH" };
  if (local.project !== d.project) return { ok: false, reason: "PROJECT_MISMATCH" };
  if (!d.available) return { ok: false, reason: "SERVER_UNAVAILABLE" };
  if (!local.available) return { ok: false, reason: "CLIENT_UNAVAILABLE" };
  if (local.payloadType !== d.payloadType) return { ok: false, reason: "PAYLOAD_TYPE_MISMATCH" };
  return { ok: true };
}

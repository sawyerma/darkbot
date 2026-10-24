import { TopicId, topics, type EventMeta, type StateMeta } from "@/core/realtime";
import { useBscStore, type BscStateKey } from "../store/bscStore";
import type { BscLaunchView, BscMonitoringView, BscStatusView, BscWalletsView } from "../model/types";
import { applyReadinessJob } from "@/core/realtime/readinessControl";
import { candidateMirrorShows } from "@/core/realtime/candidateControl";

import { fromBinary } from "@bufbuild/protobuf";
import { BscStatusSchema, BscLaunchStateSchema, BscMonitoringStateSchema, BscWalletsStateSchema } from "@/core/generated/darkbot/bsc/v1/status_pb";
// Contract: frontend/contracts/bsc-fields.json. Decoding never computes a trading gate.
export const decodeStatus = (bytes:Uint8Array): BscStatusView => fromBinary(BscStatusSchema,bytes);
export const decodeLaunch = (bytes:Uint8Array): BscLaunchView => fromBinary(BscLaunchStateSchema,bytes);
export const decodeMonitoring = (bytes:Uint8Array): BscMonitoringView => fromBinary(BscMonitoringStateSchema,bytes);
export const decodeWallets = (bytes:Uint8Array): BscWalletsView => fromBinary(BscWalletsStateSchema,bytes);

const STATE_BINDINGS: ReadonlyArray<{ topic: TopicId; key: BscStateKey; decode: (b: Uint8Array) => unknown }> = [
  { topic: TopicId.BSC_STATUS, key: "status", decode: decodeStatus },
  { topic: TopicId.BSC_LAUNCH_STATE, key: "launch", decode: decodeLaunch },
  { topic: TopicId.BSC_MONITORING_STATE, key: "monitoring", decode: decodeMonitoring },
  { topic: TopicId.BSC_WALLETS_STATE, key: "wallets", decode: decodeWallets },
];

let started = false;

// Idempotent. Wird von BscPage beim ersten Mount aufgerufen; Listener leben für die App-Lebensdauer.
export function ensureBscAdapter(): void {
  if (started) return;
  started = true;
  for (const b of STATE_BINDINGS) {
    topics.onTopicStatus(b.topic, (status, reason) => {
      useBscStore.getState().setTopicStatus(b.key, status, reason);
      // Readiness process truth only from the live status push; no HTTP polling fallback.
      if (b.key === "status" && status !== "LIVE") applyReadinessJob("bsc", null);
    });
    topics.onState(b.topic, (bytes: Uint8Array, meta: StateMeta) => {
      const projection = b.decode(bytes);
      useBscStore.getState().applyState(b.key, meta, projection as never);
      if (b.key === "status") applyReadinessJob("bsc", (projection as BscStatusView).jobs.find(j => j.job === "readiness") ?? null);
      if (b.key === "launch") candidateMirrorShows((projection as BscLaunchView).runtimeFields.find(f => f.name === "candidate.request.request_id")?.value);
    });
  }

  topics.onTopicStatus(TopicId.BSC_EXECUTION_EVENTS, (status, reason) =>
    useBscStore.getState().setTopicStatus("executionEvents", status, reason),
  );
  // Keine Execution-Ereignisse aus unverifiziertem Transport-Metadata konstruieren.
  topics.onEvent(TopicId.BSC_EXECUTION_EVENTS, (_bytes: Uint8Array, _meta: EventMeta) => {
    throw new Error("BSC_EVENT_SCHEMA_UNAVAILABLE");
  });
}

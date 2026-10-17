import { TopicId, topics, type EventMeta, type StateMeta } from "@/core/realtime";
import { useBscStore, type BscStateKey } from "../store/bscStore";
import type { BscLaunchView, BscMonitoringView, BscStatusView, BscWalletsView } from "../model/types";

// Einzige Brücke Transport -> BSC-Store. Hier und nur hier dürfen später die generierten
// darkbot.bsc.v1-Typen importiert werden (ESLint-Boundary Regel 1).
//
// F0-VAR-GATE: darkbot.bsc.v1 fehlt. Decoder verweigern die Verarbeitung
// statt einen Null-Snapshot als erfolgreiches LIVE-Frame zu quittieren.

function unavailable(_bytes: Uint8Array): never {
  throw new Error("BSC_DOMAIN_SCHEMA_UNAVAILABLE: requires qualified F0-VAR protobuf adapter");
}
const decodeStatus: (_: Uint8Array) => BscStatusView = unavailable;
const decodeLaunch: (_: Uint8Array) => BscLaunchView = unavailable;
const decodeMonitoring: (_: Uint8Array) => BscMonitoringView = unavailable;
const decodeWallets: (_: Uint8Array) => BscWalletsView = unavailable;

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
    topics.onTopicStatus(b.topic, (status, reason) => useBscStore.getState().setTopicStatus(b.key, status, reason));
    topics.onState(b.topic, (bytes: Uint8Array, meta: StateMeta) => {
      const projection = b.decode(bytes);
      useBscStore.getState().applyState(b.key, meta, projection as never);
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

import { useEffect, useSyncExternalStore } from "react";
import type { TopicId } from "../generated/darkbot/control/v1/topics_pb";
import type { ConnectionStatus } from "./GatewayConnection";
import type { TopicStatus } from "./TopicState";
import type { AcquireOptions } from "./TopicSubscriptions";
import { gateway, topics } from "./index";

// Primitive Snapshots (Strings) => referenzstabil, kein getSnapshot-Loop.

export function useGatewayStatus(): ConnectionStatus {
  return useSyncExternalStore(
    (cb) => gateway.onStatus(() => cb()),
    () => gateway.getInfo().status,
    () => "IDLE" as ConnectionStatus,
  );
}

export function useGatewayReason(): string {
  return useSyncExternalStore(
    (cb) => gateway.onStatus(() => cb()),
    () => gateway.getInfo().reason,
    () => "",
  );
}

// Hält die Topic-Subscription für die Lebensdauer der Komponente (refcount + grace).
export function useTopic(topic: TopicId, opts?: AcquireOptions): TopicStatus {
  useEffect(() => topics.acquire(topic, opts), [topic, opts?.startPolicy]);
  return useTopicStatus(topic);
}

export function useTopicStatus(topic: TopicId): TopicStatus {
  return useSyncExternalStore(
    (cb) => topics.onTopicStatus(topic, () => cb()),
    () => topics.statusOf(topic),
    () => "IDLE" as TopicStatus,
  );
}

export function useTopicReason(topic: TopicId): string {
  return useSyncExternalStore(
    (cb) => topics.onTopicStatus(topic, () => cb()),
    () => topics.reasonOf(topic),
    () => "",
  );
}

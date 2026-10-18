import { fromBinary } from "@bufbuild/protobuf";
import { StatusProjectionSchema, type StatusProjection } from "@/core/generated/darkbot/project0/v1/status_pb";
import { TopicId, topics, type StateMeta } from "@/core/realtime";
import type { Project0StatusView } from "../model/status";
import { useProject0Store } from "../store/project0Store";

export function toStatusView(p: StatusProjection): Project0StatusView {
  return {
    runId: p.runId,
    lastObservedSlot: p.lastObservedSlot ?? null,
    lastFrameAgeNs: p.lastFrameAgeNs ?? null,
    gapCount: p.gapCount ?? null,
    reconnectCount: p.reconnectCount ?? null,
    liquidationEvents: p.liquidationEvents ?? null,
    queueBytes: p.queueBytes ?? null,
    lagFrames: p.lagFrames ?? null,
    marketVerdict: p.marketVerdict,
  };
}
let started = false;
export function ensureProject0Adapter(): void {
  if (started) return;
  started = true;
  topics.onTopicStatus(TopicId.PROJECT0_STATUS, (status, reason) =>
    useProject0Store.getState().setStatusTopic(status, reason),
  );
  topics.onState(TopicId.PROJECT0_STATUS, (bytes: Uint8Array, meta: StateMeta) => {
    // Throwing on invalid protobuf prevents registry from acknowledging this state.
    useProject0Store.getState().applyStatus(toStatusView(fromBinary(StatusProjectionSchema, bytes)), meta);
  });
}

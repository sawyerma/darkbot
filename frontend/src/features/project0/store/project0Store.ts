import { create } from "zustand";
import type { StateMeta, TopicStatus } from "@/core/realtime";
import type { CaptureView, SystemView } from "../model/status";

import { NO_STREAM_PROGRESS, nextStreamProgress, type StreamProgressState } from "../model/streamFreshness";

export interface TopicSlice<T> {
  value: T | null;
  meta: StateMeta | null;
  topicStatus: TopicStatus;
  reason: string;
  /** Browser clock when the applied state arrived from the gateway. */
  receivedAtMs: number | null;
}

function empty<T>(): TopicSlice<T> {
  return { value: null, meta: null, topicStatus: "IDLE", reason: "", receivedAtMs: null };
}

interface Project0Store {
  streamProgress: StreamProgressState;
  system: TopicSlice<SystemView>;
  capture: TopicSlice<CaptureView>;
  applySystem: (value: SystemView, meta: StateMeta, receivedAtMs: number) => void;
  applyCapture: (value: CaptureView, meta: StateMeta, receivedAtMs: number) => void;
  setSystemTopic: (topicStatus: TopicStatus, reason: string) => void;
  setCaptureTopic: (topicStatus: TopicStatus, reason: string) => void;
}

export const useProject0Store = create<Project0Store>()((set) => ({
  streamProgress: NO_STREAM_PROGRESS,
  system: empty<SystemView>(),
  capture: empty<CaptureView>(),
  applySystem: (value, meta, receivedAtMs) =>
    set((s) => ({ system: { ...s.system, value, meta, receivedAtMs } })),
  applyCapture: (value, meta, receivedAtMs) =>
    set((s) => ({ capture: { ...s.capture, value, meta, receivedAtMs },
      streamProgress: nextStreamProgress(s.streamProgress, value.transport?.applicationFrames ?? null,
        receivedAtMs, JSON.stringify([value.run?.runId ?? "", meta.sourceId, String(meta.generation)])),
    })),
  setSystemTopic: (topicStatus, reason) => set((s) => ({ system: { ...s.system, topicStatus, reason } })),
  setCaptureTopic: (topicStatus, reason) => set((s) => ({ capture: { ...s.capture, topicStatus, reason } })),
}));

export const selectSystem = (s: Project0Store) => s.system;
export const selectCapture = (s: Project0Store) => s.capture;

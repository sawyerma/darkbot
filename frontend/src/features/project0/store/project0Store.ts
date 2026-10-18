import { create } from "zustand";
import type { StateMeta, TopicStatus } from "@/core/realtime";
import type { Project0StatusView } from "../model/status";
export interface StatusSlice {
  projection: Project0StatusView | null;
  meta: StateMeta | null;
  topicStatus: TopicStatus;
  reason: string;
}
interface Project0Store {
  status: StatusSlice;
  applyStatus: (projection: Project0StatusView, meta: StateMeta) => void;
  setStatusTopic: (topicStatus: TopicStatus, reason: string) => void;
}
export const useProject0Store = create<Project0Store>()((set) => ({
  status: { projection: null, meta: null, topicStatus: "IDLE", reason: "" },
  applyStatus: (projection, meta) => set((s) => ({ status: { ...s.status, projection, meta } })),
  setStatusTopic: (topicStatus, reason) => set((s) => ({ status: { ...s.status, topicStatus, reason } })),
}));
export const selectStatus = (s: Project0Store) => s.status;

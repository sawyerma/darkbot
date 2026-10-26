import { create } from "zustand";
import type { StateMeta, TopicStatus } from "@/core/realtime";
import type { CaptureView, SystemView } from "../model/status";
import type { FinanceSnapshot } from "../model/contracts";
import type { LiquidatorView, WalletsView } from "../model/owner";
import type { AnalyticsView } from "../model/analytics";

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

export type OwnerTopic = "liquidator" | "wallets" | "finance" | "analytics";
type OwnerValue = LiquidatorView | WalletsView | FinanceSnapshot | AnalyticsView;

interface Project0Store {
  streamProgress: StreamProgressState;
  system: TopicSlice<SystemView>;
  capture: TopicSlice<CaptureView>;
  /** The state owner's projection (topics 12, 14, 11). */
  liquidator: TopicSlice<LiquidatorView>;
  wallets: TopicSlice<WalletsView>;
  finance: TopicSlice<FinanceSnapshot>;
  /** The analytics job controller's store (topic 15). */
  analytics: TopicSlice<AnalyticsView>;
  applySystem: (value: SystemView, meta: StateMeta, receivedAtMs: number) => void;
  applyOwner: (topic: OwnerTopic, value: OwnerValue, meta: StateMeta, receivedAtMs: number) => void;
  setOwnerTopic: (topic: OwnerTopic, topicStatus: TopicStatus, reason: string) => void;
  applyCapture: (value: CaptureView, meta: StateMeta, receivedAtMs: number) => void;
  /** P0-012: every received capture state's frame counter, before any render batching. */
  observeStream: (frames: bigint | null, identity: string, receivedAtMs: number) => void;
  setSystemTopic: (topicStatus: TopicStatus, reason: string) => void;
  setCaptureTopic: (topicStatus: TopicStatus, reason: string) => void;
}

export const useProject0Store = create<Project0Store>()((set) => ({
  streamProgress: NO_STREAM_PROGRESS,
  system: empty<SystemView>(),
  capture: empty<CaptureView>(),
  liquidator: empty<LiquidatorView>(),
  wallets: empty<WalletsView>(),
  finance: empty<FinanceSnapshot>(),
  analytics: empty<AnalyticsView>(),
  applyOwner: (topic, value, meta, receivedAtMs) =>
    set((s) => ({ [topic]: { ...s[topic], value, meta, receivedAtMs } }) as Partial<Project0Store>),
  setOwnerTopic: (topic, topicStatus, reason) =>
    set((s) => ({ [topic]: { ...s[topic], topicStatus, reason } }) as Partial<Project0Store>),
  applySystem: (value, meta, receivedAtMs) =>
    set((s) => ({ system: { ...s.system, value, meta, receivedAtMs } })),
  applyCapture: (value, meta, receivedAtMs) =>
    set((s) => ({ capture: { ...s.capture, value, meta, receivedAtMs } })),
  observeStream: (frames, identity, receivedAtMs) =>
    set((s) => {
      const next = nextStreamProgress(s.streamProgress, frames, receivedAtMs, identity);
      return next === s.streamProgress ? s : { streamProgress: next };
    }),
  setSystemTopic: (topicStatus, reason) => set((s) => ({ system: { ...s.system, topicStatus, reason } })),
  setCaptureTopic: (topicStatus, reason) => set((s) => ({ capture: { ...s.capture, topicStatus, reason } })),
}));

export const selectSystem = (s: Project0Store) => s.system;
export const selectCapture = (s: Project0Store) => s.capture;
export const selectLiquidator = (s: Project0Store) => s.liquidator;
export const selectWallets = (s: Project0Store) => s.wallets;
export const selectFinance = (s: Project0Store) => s.finance;
export const selectAnalytics = (s: Project0Store) => s.analytics;

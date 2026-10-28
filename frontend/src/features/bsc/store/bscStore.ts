import { create } from "zustand";
import type { EventMeta, StateMeta, TopicStatus } from "@/core/realtime";
import type { BscLaunchView, BscLiveView, BscMonitoringView, BscStatusView, BscWalletsView, NativeRecordView } from "../model/types";

// Isolierter BSC-Store. Kein Import aus project0/*, keine Economics, keine Commands.
// Jede STATE-Topic ist ein Slice mit Projektion (null = nicht decodiert/UNKNOWN) plus Transport-Meta.

export interface TopicSlice<T> {
  projection: T | null;
  meta: StateMeta | null;
  status: TopicStatus;
  reason: string;
}

export interface BscEventRow {
  meta: EventMeta;
  phase: "ARCHIVE" | "LIVE";
  receivedAtMs: number;
}

interface EventSlice {
  items: BscEventRow[];
  status: TopicStatus;
  reason: string;
}

type Projections = {
  status: BscStatusView;
  launch: BscLaunchView;
  monitoring: BscMonitoringView;
  wallets: BscWalletsView;
  live: BscLiveView;
};

export type BscStateKey = keyof Projections;
export type BscTopicKey = BscStateKey | "executionEvents";

const MAX_EVENT_ROWS = 200;

const emptySlice = <T>(): TopicSlice<T> => ({ projection: null, meta: null, status: "IDLE", reason: "" });

export interface BscStore {
  status: TopicSlice<BscStatusView>;
  launch: TopicSlice<BscLaunchView>;
  monitoring: TopicSlice<BscMonitoringView>;
  wallets: TopicSlice<BscWalletsView>;
  live: TopicSlice<BscLiveView>;
  executionEvents: EventSlice;

  applyState: <K extends BscStateKey>(key: K, meta: StateMeta, projection: Projections[K] | null) => void;
  setTopicStatus: (key: BscTopicKey, status: TopicStatus, reason: string) => void;
  pushEvent: (row: BscEventRow) => void;
  reset: () => void;
}

const initial = () => ({
  status: emptySlice<BscStatusView>(),
  launch: emptySlice<BscLaunchView>(),
  monitoring: emptySlice<BscMonitoringView>(),
  wallets: emptySlice<BscWalletsView>(),
  live: emptySlice<BscLiveView>(),
  executionEvents: { items: [] as BscEventRow[], status: "IDLE" as TopicStatus, reason: "" },
});

export const useBscStore = create<BscStore>()((set) => ({
  ...initial(),

  applyState: (key, meta, projection) =>
    set((s) => ({ [key]: { ...s[key], meta, projection } }) as Partial<BscStore>),

  setTopicStatus: (key, status, reason) =>
    set((s) => ({ [key]: { ...s[key], status, reason } }) as Partial<BscStore>),

  pushEvent: (row) =>
    set((s) => {
      const items = [row, ...s.executionEvents.items];
      if (items.length > MAX_EVENT_ROWS) items.length = MAX_EVENT_ROWS;
      return { executionEvents: { ...s.executionEvents, items } };
    }),

  reset: () => set(initial()),
}));

// Selektoren: primitive oder stabile Teilobjekte, damit nur betroffene Komponenten rendern.
export const selectStatus = (s: BscStore) => s.status;
export const selectLaunch = (s: BscStore) => s.launch;
export const selectMonitoring = (s: BscStore) => s.monitoring;
export const selectWallets = (s: BscStore) => s.wallets;
export const selectLive = (s: BscStore) => s.live;
export const selectEvents = (s: BscStore) => s.executionEvents;
export const selectGuardState = (s: BscStore) => s.status.projection?.guard?.guardState ?? "UNKNOWN";
export const selectStatusQuality = (s: BscStore) => s.status.status;
const NO_RECORDS: readonly NativeRecordView[] = [];
// Stable references: a panel re-renders only when a records slice actually changed.
export const selectLaunchRecords = (s: BscStore) => s.launch.projection?.records ?? NO_RECORDS;
export const selectMonitoringRecords = (s: BscStore) => s.monitoring.projection?.records ?? NO_RECORDS;

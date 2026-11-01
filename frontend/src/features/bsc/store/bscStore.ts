import { create } from "zustand";
import type { StateMeta, TopicStatus } from "@/core/realtime";
import type { BscLaunchView, BscLiveView, BscMonitoringView, BscStatusView, BscWalletsView, NativeRecordView } from "../model/types";

// Isolierter BSC-Store. Kein Import aus project0/*, keine Economics, keine Commands.
// Jede STATE-Topic ist ein Slice mit Projektion (null = nicht decodiert/UNKNOWN) plus Transport-Meta.

export interface TopicSlice<T> {
  projection: T | null;
  meta: StateMeta | null;
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

const emptySlice = <T>(): TopicSlice<T> => ({ projection: null, meta: null, status: "IDLE", reason: "" });

export interface BscStore {
  status: TopicSlice<BscStatusView>;
  launch: TopicSlice<BscLaunchView>;
  monitoring: TopicSlice<BscMonitoringView>;
  wallets: TopicSlice<BscWalletsView>;
  live: TopicSlice<BscLiveView>;

  applyState: <K extends BscStateKey>(key: K, meta: StateMeta, projection: Projections[K] | null) => void;
  setTopicStatus: (key: BscStateKey, status: TopicStatus, reason: string) => void;
}

export const useBscStore = create<BscStore>()((set) => ({
  status: emptySlice<BscStatusView>(),
  launch: emptySlice<BscLaunchView>(),
  monitoring: emptySlice<BscMonitoringView>(),
  wallets: emptySlice<BscWalletsView>(),
  live: emptySlice<BscLiveView>(),

  applyState: (key, meta, projection) =>
    set((s) => ({ [key]: { ...s[key], meta, projection } }) as Partial<BscStore>),

  setTopicStatus: (key, status, reason) =>
    set((s) => ({ [key]: { ...s[key], status, reason } }) as Partial<BscStore>),
}));

// Selektoren: primitive oder stabile Teilobjekte, damit nur betroffene Komponenten rendern.
export const selectStatus = (s: BscStore) => s.status;
export const selectLaunch = (s: BscStore) => s.launch;
export const selectMonitoring = (s: BscStore) => s.monitoring;
export const selectWallets = (s: BscStore) => s.wallets;
export const selectLive = (s: BscStore) => s.live;
export const selectGuardState = (s: BscStore) => s.status.projection?.guard?.guardState ?? "UNKNOWN";
export const selectStatusQuality = (s: BscStore) => s.status.status;
const NO_RECORDS: readonly NativeRecordView[] = [];
// Stable references: a panel re-renders only when a records slice actually changed.
export const selectLaunchRecords = (s: BscStore) => s.launch.projection?.records ?? NO_RECORDS;
export const selectMonitoringRecords = (s: BscStore) => s.monitoring.projection?.records ?? NO_RECORDS;

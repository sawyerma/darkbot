import { useMemo } from "react";
import type { NativeRecordView } from "../model/types";
import { strategySettingsView, withSettingsState, type StrategySettingsView } from "../model/strategySettings";
import { useBscQuery } from "./useBscQuery";

/** The Strategy C settings of the snapshot with the owner's settings state (BSC-V8-04, one
 *  `strategySettings` query while `ask`): asked once per view and again only when the snapshot's
 *  stored generation, the running generation or the strategy's state changes; never polled. */
export function useStrategySettings(records: readonly NativeRecordView[], warnings: readonly string[], nowMs: number, ask = true): StrategySettingsView {
  const view = useMemo(() => strategySettingsView(records, warnings, nowMs), [records, warnings, nowMs]);
  const asked = ask && view.available;
  const key = asked ? `strategy-settings:${view.generation.toString()}:${view.startedGeneration?.toString() ?? "-"}:${view.state}` : "strategy-settings:none";
  const q = useBscQuery(key, asked ? { case: "strategySettings", value: { includeFeatures: true } } : null);
  return useMemo(() => withSettingsState(view, q), [view, q]);
}

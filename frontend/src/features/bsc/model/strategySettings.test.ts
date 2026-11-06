import { describe, expect, it } from "vitest";
import { RUNTIME_SCHEMA, SOURCE_SCHEMA } from "./sources";
import { SETTINGS_SCHEMA, STRATEGY_SETTINGS, strategySettingsView } from "./strategySettings";
import type { NativeRecordView } from "./types";

// TEST_ONLY records: the owner's stored settings, the runtime status of a running strategy.
const field = (name: string, value: string) => ({ name, value, kind: 7, unit: "" });
const owner: NativeRecordView = { schema: SOURCE_SCHEMA, provenance: "NATIVE_CREATED", fields: [field("generation", "3")] };
const stored = (generation: string, rows: [string, string][]): NativeRecordView => ({
  schema: SETTINGS_SCHEMA, provenance: "NATIVE_CREATED",
  fields: [field("generation", generation), field("request_id", "r-7"),
    ...rows.flatMap(([id, value], i) => [field(`settings[${i}].id`, id), field(`settings[${i}].value`, value)])],
});
const NOW = 1_800_000_000_000;
const runtime = (started: string | undefined, rows: [string, string][], observed = NOW): NativeRecordView => ({
  schema: RUNTIME_SCHEMA, provenance: "NATIVE_CREATED",
  fields: [field("_provenance.observed_unix_ms", String(observed)), field("status_interval_ms", "1000"), field("status_stale_intervals", "3"),
    field("strategy[0].state", "SHADOW"),
    ...(started === undefined ? [] : [field("strategy[0].settings_generation", started)]),
    ...rows.flatMap(([id, value], i) => [field(`strategy[0].settings[${i}].id`, id), field(`strategy[0].settings[${i}].value`, value)])],
});
const row = (v: ReturnType<typeof strategySettingsView>, id: string) => v.rows.find(r => r.id === id)!;

describe("Strategy C settings (frontend package 06.11.)", () => {
  it("offers the user's scope, stored or the file's, at the stored generation", () => {
    const v = strategySettingsView([owner, stored("4", [["BSC_STRATEGY_C_MAX_OPEN", "3"], ["BSC_STRATEGY_C_FILTERS", ""]])], [], NOW);
    expect(v.rows.map(r => r.id)).toEqual(STRATEGY_SETTINGS.map(k => k.id));
    expect(v).toMatchObject({ available: true, generation: 4n, requestId: "r-7", running: false, state: "UNKNOWN" });
    expect(row(v, "BSC_STRATEGY_C_MAX_OPEN")).toMatchObject({ stored: "3", effective: undefined, pending: false });
    // An empty value is a value, never the file's.
    expect(row(v, "BSC_STRATEGY_C_FILTERS").stored).toBe("");
    expect(row(v, "BSC_STRATEGY_C_HOLD_MS").stored).toBeUndefined();
    expect(row(v, "BSC_STRATEGY_C_CREATOR_SELL_EXIT").choices).toEqual(["ENABLED", "DISABLED"]);
    // None stored yet: generation 0, still requestable.
    expect(strategySettingsView([owner], [], NOW)).toMatchObject({ available: true, generation: 0n });
  });

  it("shows what the running strategy took and what waits for the next start", () => {
    const records = [owner, stored("5", [["BSC_STRATEGY_C_MAX_OPEN", "4"]]),
      runtime("4", [["BSC_STRATEGY_C_MAX_OPEN", "3"], ["BSC_STRATEGY_C_HOLD_MS", "600000"]])];
    const v = strategySettingsView(records, [], NOW);
    expect(v).toMatchObject({ running: true, startedGeneration: 4n, state: "SHADOW" });
    expect(row(v, "BSC_STRATEGY_C_MAX_OPEN")).toMatchObject({ stored: "4", effective: "3", pending: true });
    expect(row(v, "BSC_STRATEGY_C_HOLD_MS")).toMatchObject({ stored: undefined, effective: "600000", pending: false });
    // A stale runtime status names no running strategy.
    const stale = strategySettingsView([owner, runtime("4", [["BSC_STRATEGY_C_MAX_OPEN", "3"]], NOW - 10_000)], [], NOW);
    expect(stale).toMatchObject({ running: false, state: "UNKNOWN" });
    expect(row(stale, "BSC_STRATEGY_C_MAX_OPEN").effective).toBeUndefined();
  });

  it("requests nothing without the owner or with an unreadable generation", () => {
    expect(strategySettingsView([], [], NOW)).toMatchObject({ available: false, reason: "BACKEND_UNAVAILABLE" });
    expect(strategySettingsView([owner, stored("x", [])], [], NOW)).toMatchObject({ available: false, reason: "BACKEND_INVALID", generation: 0n });
    // Review LOW 3: a store the reporter could not read is never an empty one.
    expect(strategySettingsView([owner], ["STRATEGY_SETTINGS_INVALID"], NOW)).toMatchObject({ available: false, reason: "STRATEGY_SETTINGS_INVALID" });
  });
});

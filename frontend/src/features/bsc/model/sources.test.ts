import { describe, expect, it } from "vitest";
import { BACKEND_UNAVAILABLE, SOURCE_IDS, SOURCE_SCHEMA, sourceControlView } from "./sources";
import type { NativeRecordView } from "./types";

const record = (generation: string, rows: [string, string, string, string?][]): NativeRecordView => ({
  schema: SOURCE_SCHEMA, provenance: "NATIVE_CREATED",
  fields: [{ name: "generation", value: generation, kind: 2, unit: "" },
    ...rows.flatMap(([id, requested, effective, reason], i) => [
      { name: `sources[${i}].source_id`, value: id, kind: 7, unit: "" },
      { name: `sources[${i}].requested`, value: requested, kind: 7, unit: "" },
      { name: `sources[${i}].effective`, value: effective, kind: 7, unit: "" },
      ...(reason ? [{ name: `sources[${i}].block_reason`, value: reason, kind: 7, unit: "" }] : []),
    ])],
});

const all = (over: Record<string, [string, string, string?]> = {}) =>
  SOURCE_IDS.map(id => [id, ...(over[id] ?? ["OFF", "OFF"])] as [string, string, string, string?]);

describe("BSC source control", () => {
  it("without the native owner's record nothing can be requested", () => {
    const v = sourceControlView([]);
    expect([v.available, v.reason]).toEqual([false, BACKEND_UNAVAILABLE]);
  });
  it("shows the owner's requested and effective modes with the generation", () => {
    const v = sourceControlView([record("3", all({ BSC_INGRESS_DRPC: ["LIVE", "BLOCKED", "ECONOMIC_LIVE_NOT_AUTHORIZED"], BSC_INGRESS_PUBLICNODE: ["SHADOW", "SHADOW"] }))]);
    expect(v.available).toBe(true);
    expect(v.generation).toBe(3n);
    expect(v.rows.find(r => r.id === "BSC_INGRESS_DRPC")).toEqual({ id: "BSC_INGRESS_DRPC", requested: "LIVE", effective: "BLOCKED", blockReason: "ECONOMIC_LIVE_NOT_AUTHORIZED" });
    expect(v.rows.map(r => r.id)).toEqual([...SOURCE_IDS]);
  });
  it("a record without all five sources or a generation is invalid, never partial", () => {
    expect(sourceControlView([record("x", all())]).reason).toBe("BACKEND_INVALID");
    expect(sourceControlView([record("1", all().slice(1))]).reason).toBe("BACKEND_INVALID");
  });
});

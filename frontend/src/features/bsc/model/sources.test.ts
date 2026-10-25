import { describe, expect, it } from "vitest";
import { BACKEND_UNAVAILABLE, RUNTIME_SCHEMA, SOURCE_IDS, SOURCE_SCHEMA, runtimeView, sourceControlView } from "./sources";
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
    expect(v.rows.find(r => r.id === "BSC_INGRESS_DRPC")).toMatchObject({ id: "BSC_INGRESS_DRPC", requested: "LIVE", effective: "BLOCKED", blockReason: "ECONOMIC_LIVE_NOT_AUTHORIZED", health: "NOT_RUNNING" });
    expect(v.runtime.state).toBe("NOT_RUNNING");
    expect(v.rows.map(r => r.id)).toEqual([...SOURCE_IDS]);
  });
  it("a record without all five sources or a generation is invalid, never partial", () => {
    expect(sourceControlView([record("x", all())]).reason).toBe("BACKEND_INVALID");
    expect(sourceControlView([record("1", all().slice(1))]).reason).toBe("BACKEND_INVALID");
  });
  it("takes health only from a fresh runtime status, a stale one is STALE, never HEALTHY", () => {
    const status = (observed: number): NativeRecordView => ({ schema: RUNTIME_SCHEMA, provenance: "NATIVE_CREATED", fields: [
      { name: "_provenance.observed_unix_ms", value: String(observed), kind: 2, unit: "unix_milliseconds" },
      { name: "mode", value: "READ_ONLY", kind: 7, unit: "" }, { name: "launches", value: "4", kind: 2, unit: "" },
      { name: "sources[0].source_id", value: "BSC_INGRESS_PUBLICNODE", kind: 7, unit: "" },
      { name: "sources[0].health", value: "HEALTHY", kind: 7, unit: "" }, { name: "sources[0].full", value: "12", kind: 2, unit: "" }] });
    const modes = record("1", all({ BSC_INGRESS_PUBLICNODE: ["SHADOW", "SHADOW"] }));
    const fresh = sourceControlView([modes, status(10_000)], 11_000, 5000);
    expect(fresh.runtime).toMatchObject({ state: "RUNNING", mode: "READ_ONLY", launches: "4" });
    expect(fresh.rows.find(r => r.id === "BSC_INGRESS_PUBLICNODE")?.health).toBe("HEALTHY");
    const stale = sourceControlView([modes, status(10_000)], 20_000, 5000);
    expect(stale.runtime.state).toBe("STALE");
    expect(stale.rows.find(r => r.id === "BSC_INGRESS_PUBLICNODE")?.health).toBe("STALE");
  });
  it("shows the canonical follower only when the runtime reports it, stale as STALE", () => {
    const status = (observed: number, canonical: boolean): NativeRecordView => ({ schema: RUNTIME_SCHEMA, provenance: "NATIVE_CREATED", fields: [
      { name: "_provenance.observed_unix_ms", value: String(observed), kind: 2, unit: "unix_milliseconds" },
      ...(canonical ? [{ name: "canonical[0].state", value: "FOLLOWING", kind: 7, unit: "" }, { name: "canonical[0].head", value: "100", kind: 2, unit: "" },
        { name: "canonical[0].flap_launches", value: "3", kind: 2, unit: "" }, { name: "canonical[0].seen_full_before", value: "2", kind: 2, unit: "" }] : [])] });
    expect(runtimeView([status(10_000, false)], 11_000, 5000).view.canonical).toBeUndefined();
    const c = runtimeView([status(10_000, true)], 11_000, 5000).view.canonical;
    expect(c).toMatchObject({ state: "FOLLOWING", head: "100", headAgeMs: "UNKNOWN" });
    expect(c?.launches).toContain("Flap 3");
    expect(c?.join).toContain("full before block 2");
    expect(runtimeView([status(10_000, true)], 20_000, 5000).view.canonical?.state).toBe("STALE");
  });
});

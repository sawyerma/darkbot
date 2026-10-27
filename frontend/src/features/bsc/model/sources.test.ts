import { describe, expect, it } from "vitest";
import { BACKEND_UNAVAILABLE, NOT_RECORDED, RUNTIME_SCHEMA, SOURCE_IDS, SOURCE_SCHEMA, runtimeView, sourceControlView } from "./sources";
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

const withComponents = (base: NativeRecordView, rows: [string, string, string, string?][]): NativeRecordView => ({
  ...base, fields: [...base.fields, ...rows.flatMap(([id, requested, effective, reason], i) => [
    { name: `components[${i}].component_id`, value: id, kind: 7, unit: "" },
    { name: `components[${i}].requested`, value: requested, kind: 7, unit: "" },
    { name: `components[${i}].effective`, value: effective, kind: 7, unit: "" },
    ...(reason ? [{ name: `components[${i}].block_reason`, value: reason, kind: 7, unit: "" }] : []),
  ])],
});

describe("BSC component control (C2 state)", () => {
  it("shows the owner's component modes; LIVE stays BLOCKED with its reason", () => {
    const v = sourceControlView([withComponents(record("4", all()), [["BSC_STATE_C2", "LIVE", "BLOCKED", "C2_OPERATIONAL_NOT_AUTHORIZED"]])]);
    expect(v.components[0]).toEqual({ id: "BSC_STATE_C2", requested: "LIVE", effective: "BLOCKED", blockReason: "C2_OPERATIONAL_NOT_AUTHORIZED", health: "", counters: "" });
    // A component the record does not name is NOT_RECORDED, never assumed OFF.
    expect(v.components[1]).toMatchObject({ id: "BSC_EXEC", requested: NOT_RECORDED, effective: NOT_RECORDED });
    const exec = sourceControlView([withComponents(record("5", all()), [["BSC_STATE_C2", "SHADOW", "SHADOW"], ["BSC_EXEC", "LIVE", "BLOCKED", "ECONOMIC_LIVE_NOT_AUTHORIZED"]])]);
    expect(exec.components[1]).toMatchObject({ id: "BSC_EXEC", requested: "LIVE", effective: "BLOCKED", blockReason: "ECONOMIC_LIVE_NOT_AUTHORIZED" });
    // A generation from before Strategy C does not name it: NOT_RECORDED.
    expect(exec.components[2]).toMatchObject({ id: "BSC_STRATEGY_C", requested: NOT_RECORDED, effective: NOT_RECORDED });
    const strategy = sourceControlView([withComponents(record("6", all()), [["BSC_STATE_C2", "SHADOW", "SHADOW"], ["BSC_EXEC", "OFF", "OFF"], ["BSC_STRATEGY_C", "LIVE", "BLOCKED", "ECONOMIC_LIVE_NOT_AUTHORIZED"]])]);
    expect(strategy.components[2]).toMatchObject({ id: "BSC_STRATEGY_C", requested: "LIVE", effective: "BLOCKED", blockReason: "ECONOMIC_LIVE_NOT_AUTHORIZED" });
  });
  it("a generation from before components is NOT_RECORDED, never assumed", () => {
    expect(sourceControlView([record("1", all())]).components[0]).toMatchObject({ requested: NOT_RECORDED, effective: NOT_RECORDED });
  });
  it("an unknown or repeated component makes the record invalid", () => {
    expect(sourceControlView([withComponents(record("1", all()), [["BSC_STATE_X", "OFF", "OFF"]])]).reason).toBe("BACKEND_INVALID");
    expect(sourceControlView([withComponents(record("1", all()), [["BSC_STATE_C2", "OFF", "OFF"], ["BSC_STATE_C2", "OFF", "OFF"]])]).reason).toBe("BACKEND_INVALID");
  });
});

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
      { name: "status_interval_ms", value: "1000", kind: 2, unit: "milliseconds" },
      { name: "mode", value: "READ_ONLY", kind: 7, unit: "" }, { name: "launches", value: "4", kind: 2, unit: "" },
      { name: "sources[0].source_id", value: "BSC_INGRESS_PUBLICNODE", kind: 7, unit: "" },
      { name: "sources[0].health", value: "HEALTHY", kind: 7, unit: "" }, { name: "sources[0].full", value: "12", kind: 2, unit: "" }] });
    const modes = record("1", all({ BSC_INGRESS_PUBLICNODE: ["SHADOW", "SHADOW"] }));
    const fresh = sourceControlView([modes, status(10_000)], 11_000);
    expect(fresh.runtime).toMatchObject({ state: "RUNNING", mode: "READ_ONLY", launches: "4" });
    expect(fresh.rows.find(r => r.id === "BSC_INGRESS_PUBLICNODE")?.health).toBe("HEALTHY");
    // Stale after STALE_AFTER_INTERVALS rewrites of the record's own interval (3 × 1 s).
    const stale = sourceControlView([modes, status(10_000)], 13_001);
    expect(stale.runtime.state).toBe("STALE");
    expect(stale.rows.find(r => r.id === "BSC_INGRESS_PUBLICNODE")?.health).toBe("STALE");
  });
  it("without its own rewrite interval a runtime status is never fresh", () => {
    const record: NativeRecordView = { schema: RUNTIME_SCHEMA, provenance: "NATIVE_CREATED", fields: [
      { name: "_provenance.observed_unix_ms", value: "10000", kind: 2, unit: "unix_milliseconds" }] };
    expect(runtimeView([record], 10_001).view.state).toBe("STALE");
  });
  it("shows the canonical follower only when the runtime reports it, stale as STALE", () => {
    const status = (observed: number, canonical: boolean): NativeRecordView => ({ schema: RUNTIME_SCHEMA, provenance: "NATIVE_CREATED", fields: [
      { name: "_provenance.observed_unix_ms", value: String(observed), kind: 2, unit: "unix_milliseconds" },
      { name: "status_interval_ms", value: "1000", kind: 2, unit: "milliseconds" },
      ...(canonical ? [{ name: "canonical[0].state", value: "FOLLOWING", kind: 7, unit: "" }, { name: "canonical[0].head", value: "100", kind: 2, unit: "" },
        { name: "canonical[0].flap_launches", value: "3", kind: 2, unit: "" }, { name: "canonical[0].seen_full_before", value: "2", kind: 2, unit: "" },
        { name: "canonical[0].four_deployment", value: "MATCH at block 129994638", kind: 7, unit: "" }] : [])] });
    expect(runtimeView([status(10_000, false)], 11_000).view.canonical).toBeUndefined();
    const c = runtimeView([status(10_000, true)], 11_000).view.canonical;
    expect(c).toMatchObject({ state: "FOLLOWING", head: "100", headAgeMs: "UNKNOWN" });
    expect(c?.launches).toContain("Flap 3");
    // Phase E review L3: the Four drift guard's verdict.
    expect(c?.launches).toContain("Four deployment MATCH at block 129994638");
    expect(c?.join).toContain("full before block time 2");
    expect(runtimeView([status(10_000, true)], 20_000).view.canonical?.state).toBe("STALE");
  });

  it("shows the C2 chain state only while its owner runs, coherent from the native bool, stale as STALE", () => {
    const status = (observed: number, state: boolean, coherent = "true"): NativeRecordView => ({ schema: RUNTIME_SCHEMA, provenance: "NATIVE_CREATED", fields: [
      { name: "_provenance.observed_unix_ms", value: String(observed), kind: 2, unit: "" }, { name: "status_interval_ms", value: "1000", kind: 2, unit: "" },
      ...(state ? [{ name: "state[0].coherent", value: coherent, kind: 1, unit: "" }, { name: "state[0].retained_blocks", value: "64", kind: 2, unit: "" },
        { name: "state[0].last_conflict", value: "DISCONTINUITY", kind: 7, unit: "" }, { name: "state[0].last_refused", value: "MALFORMED_BLOCK", kind: 7, unit: "" }, { name: "state[0].checkpoint_restore", value: "RESTORED", kind: 7, unit: "" }] : [])] });
    expect(runtimeView([status(10_000, false)], 11_000).view.chainState).toBeUndefined();
    const v = runtimeView([status(10_000, true)], 11_000).view.chainState;
    expect(v?.coherent).toBe("COHERENT");
    expect(v?.head).toBe("UNANCHORED");
    expect(v?.held).toContain("retained blocks 64");
    expect(v?.faults).toContain("(last DISCONTINUITY)");
    expect(v?.faults).toContain("refused inputs ? (last MALFORMED_BLOCK)");
    expect(v?.checkpoint).toContain("restore RESTORED");
    expect(runtimeView([status(10_000, true, "false")], 11_000).view.chainState?.coherent).toBe("NOT_COHERENT");
    expect(runtimeView([status(10_000, true)], 20_000).view.chainState?.coherent).toBe("STALE");
  });

  it("shows the C3 PAPER path only while configured, never claiming a transport", () => {
    const status = (observed: number, exec: boolean, latency = false): NativeRecordView => ({ schema: RUNTIME_SCHEMA, provenance: "NATIVE_CREATED", fields: [
      { name: "_provenance.observed_unix_ms", value: String(observed), kind: 2, unit: "" }, { name: "status_interval_ms", value: "1000", kind: 2, unit: "" },
      ...(exec ? [{ name: "exec[0].state", value: "ECONOMICS_UNQUALIFIED", kind: 7, unit: "" }, { name: "exec[0].reason", value: "Portal v5.27.0 is decode-only", kind: 7, unit: "" },
        { name: "exec[0].lanes_armed", value: "64", kind: 2, unit: "" }, { name: "exec[0].handoffs", value: "0", kind: 2, unit: "" },
        { name: "exec[0].sender_paths", value: "C3+STRATEGY_C", kind: 7, unit: "" }, { name: "exec[0].refusals[0]", value: "NONCE_TAKEN=1", kind: 7, unit: "" }] : []),
      ...(latency ? [{ name: "exec[0].latency_p50_ns", value: "116250", kind: 2, unit: "" }] : [])] });
    expect(runtimeView([status(10_000, false)], 11_000).view.exec).toBeUndefined();
    const v = runtimeView([status(10_000, true)], 11_000).view.exec;
    expect(v?.state).toBe("ECONOMICS_UNQUALIFIED");
    expect(v?.detail).toContain("decode-only");
    expect(v?.path).toContain("to the PAPER sink 0");
    // Phase I gate M2: every C3 refusal and the paths on the PAPER sender.
    expect(v?.path).toContain("every refusal (NONCE_TAKEN=1)");
    expect(v?.path).toContain("paths on the PAPER sender C3+STRATEGY_C");
    expect(v?.latency).toBe("no PAPER BUY yet");
    expect(v?.strategy).toContain("Strategy C intents ? · at the PAPER sink ?");
    expect(runtimeView([status(10_000, true, true)], 11_000).view.exec?.latency).toContain("not T_send");
    expect(runtimeView([status(10_000, true)], 20_000).view.exec?.state).toBe("STALE");
  });

  it("shows Strategy C's decisions and PAPER ledger only while it runs, a refusal with its reason", () => {
    const status = (observed: number, state?: string, reason?: string): NativeRecordView => ({ schema: RUNTIME_SCHEMA, provenance: "NATIVE_CREATED", fields: [
      { name: "_provenance.observed_unix_ms", value: String(observed), kind: 2, unit: "" }, { name: "status_interval_ms", value: "1000", kind: 2, unit: "" },
      ...(state ? [{ name: "strategy[0].state", value: state, kind: 7, unit: "" }, { name: "strategy[0].launches", value: "3", kind: 2, unit: "" },
        { name: "strategy[0].passed", value: "1", kind: 2, unit: "" }, { name: "strategy[0].entered", value: "1", kind: 2, unit: "" },
        { name: "strategy[0].loss_wei", value: "120000000000000", kind: 9, unit: "" }] : []),
      ...(reason ? [{ name: "strategy[0].reason", value: reason, kind: 7, unit: "" }] : [])] });
    expect(runtimeView([status(10_000)], 11_000).view.strategy).toBeUndefined();
    const v = runtimeView([status(10_000, "SHADOW")], 11_000).view.strategy;
    expect(v?.state).toBe("SHADOW");
    expect(v?.decisions).toContain("launches 3 · passed 1");
    expect(v?.paper).toContain("entered 1");
    expect(v?.paper).toContain("(120000000000000 wei)");
    expect(v?.paper).toContain("open ? (not valued)");
    expect(v?.detail).not.toContain("undefined");
    const refused = runtimeView([status(10_000, "GAS_EVIDENCE_REFUSED", "flap_gas_20261026.txt: missing")], 11_000).view.strategy;
    expect(refused?.detail).toContain("flap_gas_20261026.txt: missing");
    expect(runtimeView([status(10_000, "SHADOW")], 20_000).view.strategy?.state).toBe("STALE");
  });

  it("shows the segmented capture's storage health from the runtime, a durable sequence only once synced", () => {
    const status = (observed: number, capture: boolean, durable = false): NativeRecordView => ({ schema: RUNTIME_SCHEMA, provenance: "NATIVE_CREATED", fields: [
      { name: "_provenance.observed_unix_ms", value: String(observed), kind: 2, unit: "" }, { name: "status_interval_ms", value: "1000", kind: 2, unit: "" },
      { name: "journal_lost", value: "1", kind: 2, unit: "" },
      ...(capture ? [{ name: "capture[0].state", value: "HEALTHY", kind: 7, unit: "" }, { name: "capture[0].records", value: "42", kind: 2, unit: "" },
        { name: "capture[0].segments_sealed", value: "2", kind: 2, unit: "" }, { name: "capture[0].budget_peak_bytes", value: "900", kind: 2, unit: "" },
        { name: "capture[0].budget_max_bytes", value: "67108864", kind: 2, unit: "" }] : []),
      ...(durable ? [{ name: "capture[0].durable_sequence", value: "41", kind: 2, unit: "" }] : [])] });
    expect(runtimeView([status(10_000, false)], 11_000).view.capture).toBeUndefined();
    const v = runtimeView([status(10_000, true)], 11_000).view.capture;
    expect(v?.state).toBe("HEALTHY");
    expect(v?.detail).toContain("records 42");
    expect(v?.detail).toContain("sealed segments 2");
    expect(v?.detail).toContain("durable to sequence none yet");
    expect(v?.detail).toContain("lost 1 at the recorder");
    expect(v?.detail).toContain("peak 900");
    expect(runtimeView([status(10_000, true, true)], 11_000).view.capture?.detail).toContain("durable to sequence 41");
    expect(runtimeView([status(10_000, true)], 20_000).view.capture?.state).toBe("STALE");
  });
});

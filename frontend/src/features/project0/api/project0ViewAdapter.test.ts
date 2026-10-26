import { describe, expect, it } from "vitest";
import captureHex from "../../../../../protocol/golden/project0/capture_state.hex?raw";
import systemHex from "../../../../../protocol/golden/project0/system_status.hex?raw";
import { USED_COUNTERS } from "../model/status";
import { parameterText } from "../model/statusFormat";
import { captureJsonOf, decodeCaptureState, decodeSystemStatus, toCaptureView, toSystemView, type SystemMessage } from "./project0ViewAdapter";

function bytes(hex: string): Uint8Array {
  const t = hex.trim();
  const out = new Uint8Array(t.length / 2);
  for (let i = 0; i < out.length; i += 1) out[i] = Number.parseInt(t.slice(i * 2, i * 2 + 2), 16);
  return out;
}

describe("Project 0 wire contract (golden frames written by the Rust reporter)", () => {
  const capture = toCaptureView(decodeCaptureState(bytes(captureHex)));
  const system = toSystemView(decodeSystemStatus(bytes(systemHex)));

  it("decodes every capture block", () => {
    expect(capture.run?.runId).toBe("golden-run");
    expect(capture.phase?.phase).toBe("CAPTURE");
    expect(capture.transport?.lastCode).toBe("WSS_HTTP_429");
    expect(capture.transport?.reconnects).toBe(1); // A01
    expect(capture.writer?.diskFreeBytes).toBe(10n);
    expect(capture.writer?.queuePeakBytes).toBe(18_446_744_073_709_551_615n);
    expect(capture.assessment?.verdict).toBe("MARKET_DATA_INCOMPLETE");
    // F-01: the provider budget of the reporter's golden frame, unrepresentable cooldown kept null.
    expect(capture.provider?.requests).toBe(21n);
    expect(capture.provider?.http429Count).toBe(3n);
    expect(capture.provider?.bytes).toBe(18_446_744_073_709_551_615n);
    expect(capture.provider?.cooldownCause).toBe("RETRY_AFTER_UNREPRESENTABLE");
    expect(capture.provider?.cooldownRemainingMs).toBeNull();
    expect(capture.provider?.haltCode).toBe("PROVIDER_BUDGET_POISONED");
  });

  it("preserves missing blocks and rejects malformed wire bytes", () => {
    const empty = toCaptureView(decodeCaptureState(new Uint8Array()));
    expect(Object.values(empty).every((block) => block === null)).toBe(true);
    expect(() => decodeCaptureState(new Uint8Array([0xff]))).toThrow();
    expect(() => decodeSystemStatus(new Uint8Array([0xff]))).toThrow();
  });

  it("knows every counter name the GUI reads", () => {
    for (const name of USED_COUNTERS) expect(capture.market?.counters.has(name), name).toBe(true);
  });

  it("decodes the operational parameter registry", () => {
    expect(system.parameterCount).toBeGreaterThan(0);
    expect(system.activated).toBe(false);
    // D01: a former literal arrives as a typed parameter with unit and source.
    expect(system.parameters.get("blockhash_max_age_ms")).toEqual({ value: 30_000n, unit: "ms", source: "builtin_default",
      defaultValue: 30_000n, minimum: 1n, maximum: 4_294_967_295n, provisional: null, changeRule: "restart" });
    expect(system.parameters.size).toBe(system.parameterCount);
  });

  it("DEC-05: states the range, the open measurement question and the owner parameters", () => {
    const workers = system.parameters.get("workers")!;
    expect(workers.provisional).toBe("B-2");
    expect(parameterText(workers)).toBe("1 threads · builtin_default · default 1 · 1…4294967295 · restart · provisional, measurement open (B-2)");
    expect(system.parameters.get("max_slippage_bps")).toMatchObject({ value: 0n, unit: "bps", minimum: 0n, maximum: 10_000n, provisional: null });
    expect(system.parameters.get("recovery_root_catchup_seconds")).toMatchObject({ value: 120n, minimum: 30n, maximum: 300n });
    expect(system.settings.size).toBe(0);
    const m = decodeSystemStatus(bytes(systemHex));
    type Effective = NonNullable<NonNullable<SystemMessage["operational"]>["effective"]>;
    m.operational!.effective = { profile: "performance", sources: { profile: "mode:econ4h", threshold_trigger: "environment" },
      diagnostics: false, thresholdTrigger: true, c0Census: false, sourceGeyser: "off", sourceGeyserProvider: "publicnode",
      liveAuthorized: false, submissionAuthorized: false } as Partial<Effective> as Effective;
    const activated = toSystemView(m);
    expect([activated.profile, activated.profileSource]).toEqual(["performance", "mode:econ4h"]);
    expect(activated.settings.get("threshold_trigger")).toEqual({ value: "true", source: "environment" });
    expect(activated.settings.get("source_geyser")).toEqual({ value: "off", source: "" });
  });
});

describe("All values never shows a local filesystem path", () => {
  const json = (runDir: string) => {
    const m = decodeCaptureState(bytes(captureHex));
    m.run!.runDir = runDir;
    return captureJsonOf(m);
  };
  it("keeps only the last segment of an absolute run directory", () => {
    expect((json("/Users/someone/DarkBot/project_0/data/runs/r1").run as Record<string, unknown>).run_dir).toBe("…/r1");
    expect((json("C:\\Users\\someone\\runs\\r1").run as Record<string, unknown>).run_dir).toBe("…/r1");
    expect((json("").run as Record<string, unknown>).run_dir).toBe("");
    expect((json("runs/r1").run as Record<string, unknown>).run_dir).toBe("runs/r1");
  });
});

import { describe, expect, it } from "vitest";
import captureHex from "../../../../../protocol/golden/project0/capture_state.hex?raw";
import systemHex from "../../../../../protocol/golden/project0/system_status.hex?raw";
import { USED_COUNTERS } from "../model/status";
import { decodeCaptureState, decodeSystemStatus, toCaptureView, toSystemView } from "./project0ViewAdapter";

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
    expect(capture.writer?.diskFreeBytes).toBe(10n);
    expect(capture.writer?.queuePeakBytes).toBe(18_446_744_073_709_551_615n);
    expect(capture.assessment?.verdict).toBe("MARKET_DATA_INCOMPLETE");
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
  });
});

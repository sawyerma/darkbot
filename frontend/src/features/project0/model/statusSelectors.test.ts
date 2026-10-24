import { describe, expect, it } from "vitest";
import type { CaptureView, CapturePhaseName, SystemView, TransportView } from "./status";
import { executionAuthorization, healthLabel } from "./statusSelectors";
import { STREAM_STALE_AFTER_MS } from "./streamFreshness";

function view(phase: CapturePhaseName, transport: Partial<TransportView> = {}, counters: Record<string, bigint> = {}, endResult = ""): CaptureView {
  return {
    run: null, assessment: null, writer: null,
    phase: { phase, changedUnixMs: 0n, streamReadyUnixS: null, stopReason: "", drainComplete: null, writeError: null, endResult },
    transport: { connected: true, lastFrameAgeNs: null, applicationFrames: 1n, lastSlot: null,
      lastState: "CAPTURING", lastCode: "APPLICATION_SLOT_PROGRESS", lastAttempt: 1, ...transport },
    market: { observing: true, streamReady: true, counters: new Map(Object.entries(counters)) },
  };
}
const NOW = 1_000_000;

describe("healthLabel", () => {
  it("calls a stream live only with recent counter growth, and never valid", () => {
    expect(healthLabel("LIVE", view("CAPTURE"), { nowMs: NOW, progressAtMs: NOW - 1_000, firstSeenAtMs: NOW - 9_000 })).toBe("Live · unverified");
  });
  it("detects a stream that stopped delivering, although `connected` stays true", () => {
    const clock = { nowMs: NOW, progressAtMs: NOW - STREAM_STALE_AFTER_MS - 1, firstSeenAtMs: NOW - 60_000 };
    expect(healthLabel("LIVE", view("CAPTURE"), clock)).toBe("Solana stream stale");
  });
  it("never reports a value of unknown age as fresh after a reload", () => {
    expect(healthLabel("LIVE", view("CAPTURE"), { nowMs: NOW, progressAtMs: null, firstSeenAtMs: NOW - 1_000 })).toBe("Checking stream");
    expect(healthLabel("LIVE", view("CAPTURE"), { nowMs: NOW, progressAtMs: null, firstSeenAtMs: NOW - STREAM_STALE_AFTER_MS - 1 })).toBe("Solana stream stale");
  });
  it("shows reconnects and the provider limit without flickering", () => {
    expect(healthLabel("LIVE", view("CAPTURE", { lastState: "DEGRADED", lastCode: "WSS_HTTP_429" }))).toBe("Reconnecting · WSS_HTTP_429");
    expect(healthLabel("LIVE", view("WAITING_FOR_STREAM", { lastState: "RECOVERING", lastCode: "CONNECT", lastAttempt: 3 }, { transport_http_429: 3n })))
      .toBe("Provider rate limit · HTTP 429 ×3");
  });
  it("names old snapshots and finished runs as such", () => {
    expect(healthLabel("DETACHED", view("CAPTURE"))).toBe("Capture process gone · last known");
    expect(healthLabel("DETACHED", view("ENDED", {}, {}, "CAPTURE_COMPLETE"))).toBe("Last run · Completed · no capture running");
    expect(healthLabel("SUBSCRIBING" as never, null)).toBe("No capture data");
  });
});

describe("execution authorization is the engine's statement, never a fixed GUI claim", () => {
  const system = (liveAuthorized: boolean | null, submissionAuthorized: boolean | null): SystemView =>
    ({ activated: liveAuthorized != null, profile: null, parameterCount: 1, liveAuthorized, submissionAuthorized });
  it("is unknown without a live effective record", () => {
    expect(executionAuthorization(null, "LIVE")).toBe("Authorization unknown");
    expect(executionAuthorization(system(null, null), "LIVE")).toBe("Authorization unknown");
    for (const status of ["STALE", "DETACHED", "UNAVAILABLE", "ERROR", "IDLE"] as const)
      expect(executionAuthorization(system(false, false), status)).toBe("Authorization unknown");
  });
  it("needs both LIVE and submission authorization", () => {
    expect(executionAuthorization(system(false, false), "LIVE")).toBe("Not authorized");
    expect(executionAuthorization(system(true, false), "LIVE")).toBe("Not authorized");
    expect(executionAuthorization(system(true, true), "LIVE")).toBe("Authorized by engine");
  });
});

import { describe, expect, it } from "vitest";
import { decodeCaptureState, toCaptureView } from "../api/project0ViewAdapter";
import captureHex from "../../../../../protocol/golden/project0/capture_state.hex?raw";
import { healthLabel, phaseLabel } from "./statusSelectors";

const capture = () => toCaptureView(decodeCaptureState(Uint8Array.from(
  captureHex.trim().match(/../g)!.map((byte) => Number.parseInt(byte, 16)),
)));

describe("capture health", () => {
  it("does not invent a stopped run or live connection", () => {
    expect(phaseLabel(null)).toBe("Unknown");
    expect(healthLabel("UNAVAILABLE", null)).toBe("Unavailable");
    const c = capture();
    c.assessment = null;
    expect(healthLabel("LIVE", c)).toBe("Checking");
    for (const status of ["IDLE", "SUBSCRIBING", "CATCHING_UP"] as const) {
      expect(healthLabel(status, c)).toBe("Unknown");
    }
  });
  it("keeps stale and failed topics from appearing live", () => {
    const c = capture();
    expect(healthLabel("LIVE", c)).toBe("Degraded");
    expect(healthLabel("STALE", c)).toBe("Stale");
    expect(healthLabel("DETACHED", c)).toBe("Source detached");
    expect(healthLabel("ERROR", c)).toBe("Error");
    expect(healthLabel("UNAVAILABLE", c)).toBe("Unavailable");
    c.assessment!.coverageComplete = true;
    c.assessment!.decodeComplete = true;
    expect(healthLabel("LIVE", c)).toBe("Live");
    c.transport!.connected = false;
    expect(healthLabel("LIVE", c)).toBe("Degraded");
  });
  it("distinguishes lifecycle and terminal result from connection quality", () => {
    const c = capture();
    c.phase!.phase = "WAITING_FOR_STREAM";
    expect(healthLabel("LIVE", c)).toBe("Waiting for stream");
    c.phase!.phase = "FINALIZE";
    expect(healthLabel("LIVE", c)).toBe("Finishing");
    c.phase!.phase = "ENDED";
    c.phase!.endResult = "CAPTURE_COMPLETE";
    expect(healthLabel("LIVE", c)).toBe("Completed");
    expect(healthLabel("DETACHED", c)).toBe("Completed · Source detached");
    c.phase!.endResult = "MARKET_CAPTURE_INCOMPLETE: OPERATOR_STOP";
    expect(healthLabel("LIVE", c)).toBe("Incomplete");
  });
});

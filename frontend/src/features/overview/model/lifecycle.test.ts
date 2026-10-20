import { describe, expect, it } from "vitest";
import { captureIndicator, processIndicator, projectControls, stopAllControl } from "./lifecycle";
describe("supervised capture controls", () => {
  it("unknown is never inferred from feed connectivity", () => {
    expect(captureIndicator(null).state).toBe("UNKNOWN");
    expect(processIndicator({ state: "RUNNING", runId: "a" }, "STALE").state).toBe("UNKNOWN");
  });
  it("exposes all real process states and only allows qualified actions", () => {
    for (const state of ["RUNNING", "STOPPED", "STOPPING", "FAILED", "UNKNOWN"] as const) {
      const indicator = { state, label: state, reason: "launchd" };
      expect(projectControls("project0", indicator).startStop.enabled).toBe(["RUNNING", "STOPPED", "FAILED"].includes(state));
      expect(stopAllControl(indicator).enabled).toBe(state === "RUNNING");
      expect(projectControls("bsc", indicator).startStop.enabled).toBe(["RUNNING", "STOPPED", "FAILED"].includes(state));
      expect(projectControls("bsc", indicator).logs.enabled).toBe(true);
      for (const key of ["solana"] as const) {
        expect(projectControls(key, indicator).startStop.enabled).toBe(false);
        expect(projectControls(key, indicator).logs.enabled).toBe(false);
      }
    }
  });
});

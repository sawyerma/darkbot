import { describe, expect, it } from "vitest";
import { GLOBAL_CONTROLS, processIndicator, projectControls, type DisabledControl } from "./lifecycle";

describe("F7-OVR lifecycle model", () => {
  it("never derives a process state from nothing, a stale source or an unidentified run", () => {
    expect(processIndicator(null).state).toBe("UNKNOWN");
    expect(processIndicator({ state: "RUNNING", runId: "r1" }, "STALE").state).toBe("UNKNOWN");
    expect(processIndicator({ state: "RUNNING", runId: "r1" }, "DETACHED").state).toBe("UNKNOWN");
    expect(processIndicator({ state: "RUNNING", runId: "" }, "LIVE").state).toBe("UNKNOWN");
  });

  it("shows a supervisor state only from a live, identified source (future F7-OVR-B path)", () => {
    expect(processIndicator({ state: "RUNNING", runId: "r1" }, "LIVE")).toEqual(
      { state: "RUNNING", label: "Process · RUNNING", reason: "Run r1" });
  });

  it("keeps every project control disabled with an honest, project-specific reason", () => {
    for (const key of ["project0", "bsc", "solana"] as const) {
      const c = projectControls(key);
      expect(c.startStop.enabled).toBe(false);
      expect(c.logs.enabled).toBe(false);
      expect(c.startStop.reason.length).toBeGreaterThan(10);
    }
    expect(projectControls("project0").startStop.reason).not.toBe(projectControls("bsc").startStop.reason);
  });

  it("offers exactly Stop All and Show All Logs globally, both disabled, and no Start Selected", () => {
    expect(Object.keys(GLOBAL_CONTROLS)).toEqual(["stopAll", "showAllLogs"]);
    expect(GLOBAL_CONTROLS.stopAll.enabled).toBe(false);
    expect(GLOBAL_CONTROLS.showAllLogs.enabled).toBe(false);
  });

  it("makes an enabled control a type error", () => {
    // @ts-expect-error enabling requires the F6 command contract, not a flag flip
    const enabled: DisabledControl = { enabled: true, reason: "x" };
    expect(enabled).toBeDefined();
  });
});

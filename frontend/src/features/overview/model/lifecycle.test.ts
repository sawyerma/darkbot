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
      // DEC-07: only the Stop of a running capture; a start is the boot flow's (Capture tab).
      expect(projectControls("project0", indicator).startStop.enabled).toBe(state === "RUNNING");
      if (state === "STOPPED" || state === "FAILED") {
        const c = projectControls("project0", indicator).startStop;
        expect(c.enabled ? "" : c.reason).toMatch(/^BOOT_AUDIT_REQUIRED/);
      }
      expect(stopAllControl(indicator).enabled).toBe(state === "RUNNING");
      expect(projectControls("bsc", indicator).startStop.enabled).toBe(["RUNNING", "STOPPED", "FAILED"].includes(state));
      expect(projectControls("bsc", indicator).logs.enabled).toBe(true);
      for (const key of ["solana"] as const) {
        expect(projectControls(key, indicator).startStop.enabled).toBe(false);
        expect(projectControls(key, indicator).logs.enabled).toBe(false);
      }
    }
  });
  it("names every effect of darkbotctl capture stop: autostart suspended, BSC readiness also stopped", () => {
    const running = { state: "RUNNING", label: "RUNNING", reason: "launchd" } as const;
    for (const control of [projectControls("project0", running).startStop, stopAllControl(running)]) {
      const confirm = control.enabled ? control.confirm : undefined;
      expect(confirm).toMatch(/BSC-Readiness/);
      expect(confirm).toMatch(/Autostart/);
      expect(confirm).not.toMatch(/ausschließlich Project 0/);
    }
  });
});

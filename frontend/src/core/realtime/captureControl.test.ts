import { describe, expect, it } from "vitest";
import { captureButtons, type CaptureProcess, type ProcessState } from "./captureControl";

const process = (state: ProcessState, controller_available = true): CaptureProcess =>
  ({ state, run_id: "", pid: null, reason: "", observed_unix_ms: 0, controller_available });

describe("Project 0 capture buttons follow the observed process only", () => {
  it("starts only a stopped or failed capture and stops only a running one", () => {
    expect(captureButtons(process("STOPPED"), false)).toMatchObject({ start: { enabled: true }, stop: { enabled: false } });
    expect(captureButtons(process("FAILED"), false)).toMatchObject({ start: { enabled: true }, stop: { enabled: false } });
    expect(captureButtons(process("RUNNING"), false)).toMatchObject({ start: { enabled: false }, stop: { enabled: true } });
  });
  it("never offers a command while state, controller or a previous command is unresolved", () => {
    for (const [p, busy] of [[null, false], [process("UNKNOWN"), false], [process("STOPPING"), false],
      [process("STOPPED", false), false], [process("RUNNING", false), false], [process("STOPPED"), true], [process("RUNNING"), true]] as const) {
      const b = captureButtons(p, busy);
      expect([b.start.enabled, b.stop.enabled]).toEqual([false, false]);
      expect(b.start.reason).not.toBe("");
    }
  });
});

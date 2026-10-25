import { describe, expect, it } from "vitest";
import { checkStatus } from "./checks";

describe("BSC native readiness checks", () => {
  const checks = Object.freeze([
    Object.freeze({ name: "CLOCK_CONFIG", value: "BLOCKED · Attempt clock_id and boot_id must be configured before arming" }),
    Object.freeze({ name: "ATTEMPT_AUTHORITY", value: "STOP · A shared authority of this attempt already exists on origin" }),
  ]);
  it("reads exactly the native status of one check and never invents one", () => {
    expect(checkStatus(checks, "ATTEMPT_AUTHORITY")).toBe("STOP");
    expect(checkStatus(checks, "CLOCK_CONFIG")).toBe("BLOCKED");
    expect(checkStatus(checks, "ATTEMPT_LATCH")).toBeUndefined();
    expect(checkStatus(undefined, "ATTEMPT_AUTHORITY")).toBeUndefined();
  });
});

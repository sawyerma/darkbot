import { describe, expect, it } from "vitest";
import { WelcomeStatus } from "../generated/darkbot/control/v1/stream_pb";
import { shouldReloadForSkew } from "./GatewayConnection";

describe("version skew handling", () => {
  it("reloads exactly once per server schema", () => {
    expect(shouldReloadForSkew(WelcomeStatus.REJECTED_SCHEMA, "new", null)).toBe(true);
    expect(shouldReloadForSkew(WelcomeStatus.REJECTED_SCHEMA, "new", "new")).toBe(false);
    expect(shouldReloadForSkew(WelcomeStatus.REJECTED_MAJOR, "newer", "new")).toBe(true);
  });
  it("never reloads for an accepted handshake", () => {
    expect(shouldReloadForSkew(WelcomeStatus.ACCEPTED, "x", null)).toBe(false);
  });
});

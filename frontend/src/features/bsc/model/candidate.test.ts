import { describe, expect, it } from "vitest";
import {
  BACKEND_UNAVAILABLE,
  PLANNED_SCOPES,
  PLANNED_SOURCES,
  candidateControlView,
} from "./candidate";

describe("G0.7 candidate control without a native backend", () => {
  const view = candidateControlView("UNARMED");

  it("offers no switch that could appear successful", () => {
    expect(view.available).toBe(false);
    expect(view.reason).toBe(BACKEND_UNAVAILABLE);
    expect(view.options.map(o => [o.profile, o.enabled])).toEqual([
      ["SELF_A", false],
      ["SELF_B", false],
    ]);
  });

  it("invents no candidate value, salt or qualification", () => {
    const values = [...view.rows, ...view.readiness].map(r => r.value);
    for (const value of values) {
      expect(value).not.toMatch(/0x[0-9a-f]{40,64}/i);
      expect(["QUALIFIED", "PASS_READ_ONLY", "PASS"]).not.toContain(value);
    }
    expect(view.rows.find(r => r.label === "Compiled guard")?.value).toBe("UNARMED");
    expect(candidateControlView(undefined).rows.at(-1)?.value).toBe("UNKNOWN");
  });

  it("shows FOREIGN_AUTO and the five sources only as planned", () => {
    expect(PLANNED_SCOPES.map(s => s.id)).toEqual(["FOREIGN_AUTO"]);
    expect(PLANNED_SOURCES.map(s => s.id)).toEqual([
      "BSC_INGRESS_GETH",
      "PUBLICNODE",
      "DRPC",
      "BLOCKREQ",
      "48CLUB",
    ]);
    for (const control of [...PLANNED_SCOPES, ...PLANNED_SOURCES]) {
      expect(control.state).toMatch(/^PLANNED/);
    }
  });
});

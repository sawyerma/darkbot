import { describe, expect, it } from "vitest";
import { BACKEND_UNAVAILABLE, candidateControlView } from "./candidate";

const mirror = (extra: Record<string, string> = {}) =>
  Object.entries({
    "candidate.backend": "AVAILABLE",
    "candidate.selection.generation": "2",
    "candidate.selection.profile": "SELF_A",
    "candidate.selection.candidate_id": "0xaa",
    "candidate.SELF_A.state": "PRESENT",
    "candidate.SELF_A.candidate_id": "0xaa",
    "candidate.SELF_A.token": "0xE51C…7777",
    "candidate.SELF_A.qualification": "UNQUALIFIED",
    "candidate.SELF_A.readiness": "STALE",
    "candidate.SELF_B.state": "PRESENT",
    "candidate.SELF_B.token": "0xB…7777",
    "candidate.SELF_B.qualification": "UNQUALIFIED",
    "candidate.SELF_B.readiness": "NOT_SELECTED",
    "candidate.request.state": "NONE",
    ...extra,
  }).map(([name, value]) => ({ name, value }));

const live = { guardState: "UNARMED", statusOk: true, busy: false };

describe("G0.7 candidate control", () => {
  it("without a native backend offers no switch that could appear successful", () => {
    for (const fields of [undefined, [], [{ name: "other", value: "x" }]]) {
      const view = candidateControlView({ fields, ...live });
      expect([view.available, view.reason]).toEqual([false, BACKEND_UNAVAILABLE]);
      expect(view.options.every(o => !o.enabled)).toBe(true);
      expect(view.rows.find(r => r.label === "Compiled guard")?.value).toBe("UNARMED");
      expect(view.rows.some(r => r.value === "QUALIFIED")).toBe(false);
    }
    const invalid = candidateControlView({ fields: [{ name: "candidate.backend", value: "INVALID" }], ...live });
    expect([invalid.available, invalid.reason]).toEqual([false, "BACKEND_INVALID"]);
  });

  it("shows the native selection and enables only a real alternative", () => {
    const view = candidateControlView({ fields: mirror(), ...live });
    expect(view.available).toBe(true);
    expect(view.generation).toBe(2n);
    expect(view.options.map(o => [o.profile, o.enabled, o.why])).toEqual([
      ["SELF_A", false, "ALREADY_SELECTED"],
      ["SELF_B", true, ""],
    ]);
    expect(view.rows.find(r => r.label === "Selected profile")?.value).toBe("SELF_A · generation 2");
    expect(view.rows.find(r => r.label === "Qualification")?.value).toBe("UNQUALIFIED");
    expect(view.readiness).toEqual([{ profile: "SELF_A", value: "STALE" }, { profile: "SELF_B", value: "NOT_SELECTED" }]);
  });

  it("keeps the selector disabled in every unsafe or undecided state", () => {
    const disabled = (input: Parameters<typeof candidateControlView>[0]) =>
      candidateControlView(input).options.find(o => o.profile === "SELF_B");
    expect(disabled({ fields: mirror(), ...live, guardState: "ARMED" })?.why).toBe("GUARD_NOT_UNARMED");
    expect(disabled({ fields: mirror(), ...live, guardState: undefined })?.enabled).toBe(false);
    expect(disabled({ fields: mirror(), ...live, statusOk: false })?.why).toBe("STATUS_NOT_CURRENT");
    expect(disabled({ fields: mirror(), ...live, busy: true })?.why).toBe("REQUEST_IN_FLIGHT");
    expect(disabled({ fields: mirror({ "candidate.request.state": "PENDING_DESKTOP_SYNC" }), ...live })?.why).toBe("REQUEST_PENDING_DESKTOP_SYNC");
    expect(disabled({ fields: mirror({ "candidate.SELF_B.state": "ABSENT" }), ...live })?.why).toBe("CANDIDATE_ABSENT");
    expect(candidateControlView({ fields: mirror({ "candidate.selection.generation": "x" }), ...live }).available).toBe(false);
  });

  it("shows a request as requested and the native decision separately", () => {
    const view = candidateControlView({ fields: mirror({ "candidate.request.state": "REJECTED", "candidate.request.profile": "SELF_B", "candidate.request.expected_generation": "1", "candidate.request.detail": "stale selection" }), ...live });
    expect(view.request.map(r => r.value)).toEqual(["REJECTED · SELF_B @ generation 1", "stale selection"]);
    expect(view.rows.find(r => r.label === "Selected profile")?.value).toBe("SELF_A · generation 2");
  });

});

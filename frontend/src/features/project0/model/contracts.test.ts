import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { REPORT_MARKERS, type LiquidatorDraft } from "./contracts";
import { usdMicroDisplay } from "./format";
import { validateDraft } from "../hooks/useFleetDrafts";

const draft = (over: Partial<LiquidatorDraft> = {}): LiquidatorDraft => ({
  id: "d1", name: "Classic 1", origin: "manual", enabledStrategies: ["CLASSIC"],
  capitalUsd: "100", minimumNetUsd: "0", maximumFeeUsd: "0.5", maximumInFlight: "1", ...over,
});

describe("Project_0 frontend contracts", () => {
  it("shows exactly the six native report markers (measurement_contract.py), in native order", () => {
    const py = readFileSync(new URL("../../../../../project_0/operations/cli/measurement_contract.py", import.meta.url), "utf8");
    const native = /^MARKERS=\(([^)]*)\)/m.exec(py)?.[1]?.match(/'([a-z_]+)'/g)?.map(s => s.slice(1, -1));
    expect(REPORT_MARKERS.map(m => m.id)).toEqual(native);
  });
  it("accepts only canonical signed USD micro and never turns unknown into 0", () => {
    expect(usdMicroDisplay("-1234567")).toBe("-1.23 USD");
    expect(usdMicroDisplay("0")).toBe("0.00 USD");
    for (const v of ["-0", "007", "1.5", "", null, undefined]) expect(usdMicroDisplay(v)).toBe("— USD");
  });
  it("validates browser-local drafts without treating them as executable", () => {
    expect(validateDraft(draft())).toBeNull();
    expect(validateDraft(draft({ name: " " }))).toMatch(/name/);
    expect(validateDraft(draft({ enabledStrategies: [] }))).toMatch(/strategy/);
    expect(validateDraft(draft({ capitalUsd: "0" }))).toMatch(/Capital/);
    expect(validateDraft(draft({ minimumNetUsd: "-1" }))).toMatch(/nonnegative/);
    expect(validateDraft(draft({ maximumFeeUsd: "1.1234567" }))).toMatch(/Fee/);
    expect(validateDraft(draft({ maximumInFlight: "0" }))).toMatch(/positive integer/);
  });
});

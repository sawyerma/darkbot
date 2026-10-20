import { describe, expect, it } from "vitest";
import { filterReportFields } from "./reportFilters";

describe("BSC report filters", () => {
  const rows = Object.freeze([
    Object.freeze({ name: "transactions.buy_value_wei", value: "100000000000000000000000000001" }),
    Object.freeze({ name: "CURRENT_DEPLOYMENT_DRIFT", value: "STOP · portal_version: v5.24.0 -> v5.25.0" }),
  ]);
  it("filters names and native values without converting amounts or changing evidence", () => {
    expect(filterReportFields(rows, " BUY_VALUE ")).toEqual([rows[0]]);
    expect(filterReportFields(rows, "v5.25.0")).toEqual([rows[1]]);
    expect(filterReportFields(rows, "does not exist")).toEqual([]);
    expect(filterReportFields(rows, "")).toBe(rows);
    expect(rows[0]?.value).toBe("100000000000000000000000000001");
  });
});

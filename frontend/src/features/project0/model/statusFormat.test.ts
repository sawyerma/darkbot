import { describe, expect, it } from "vitest";
import { fmtAgeNs, fmtBytesExact, fmtEtaS, formatField } from "./statusFormat";

describe("status units and missing measurements", () => {
  it("keeps uint64 values exact", () => {
    expect(fmtBytesExact(18_446_744_073_709_551_615n)).toBe("18,446,744,073,709,551,615 B");
    expect(formatField("counter", "18446744073709551615")).toBe("18,446,744,073,709,551,615");
  });
  it("distinguishes absent forecasts from measured zero", () => {
    expect(fmtEtaS(null)).toBe("not available");
    expect(fmtEtaS(0)).toBe("0 s");
    expect(fmtAgeNs(null)).toBe("—");
    expect(fmtAgeNs(0n)).toBe("0 ns");
    expect(formatField("connected", false)).toBe("no");
  });
});

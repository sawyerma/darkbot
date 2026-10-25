import { describe, expect, it } from "vitest";
import type { ProviderView } from "./status";
import { fmtAgeNs, fmtBytesExact, fmtClockMs, fmtEtaS, fmtProvider, formatField } from "./statusFormat";

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

describe("provider budget (F-01)", () => {
  const budget: ProviderView = {
    sampledUnixMs: 1n, scope: "PROCESS_SOLANA_PROVIDER_ACCOUNT", requests: 21n, http429Count: 3n, inFlight: 1n, queued: 2n,
    maxRps: 10n, effectiveRps: 2.5, maxConcurrency: 4n, burst: 5n, bytes: 7n, cooldownCause: "NONE",
    cooldownRemainingMs: 0n, haltCode: null, quotaQualified: false,
  };
  it("shows the counters and an admitting budget", () => {
    expect(fmtProvider(budget)).toBe("21 requests · HTTP 429 3 · 1 in flight / 2 queued · 2.5 / 10 rps · no cooldown · admitting");
  });
  it("never shows an unrepresentable cooldown as 0 and names a halt", () => {
    const halted = { ...budget, cooldownCause: "RETRY_AFTER_UNREPRESENTABLE", cooldownRemainingMs: null, haltCode: "PROVIDER_BUDGET_POISONED" };
    expect(fmtProvider(halted)).toContain("cooldown RETRY_AFTER_UNREPRESENTABLE unbounded");
    expect(fmtProvider(halted)).toContain("halted PROVIDER_BUDGET_POISONED");
    expect(fmtProvider({ ...budget, cooldownCause: "SERVER_RETRY_AFTER", cooldownRemainingMs: 1500n })).toContain("cooldown SERVER_RETRY_AFTER 1500 ms");
    expect(fmtProvider(null)).toBe("Not reported");
  });
});

describe("engine clock stamps (D08)", () => {
  it("never shows an unavailable engine clock as 1970", () => {
    expect(fmtClockMs(0n)).toBe("clock unavailable");
    expect(fmtClockMs(1_760_000_000_123n)).toMatch(/\.123$/);
  });
});

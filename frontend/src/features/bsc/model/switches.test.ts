import { describe, expect, it } from "vitest";
import { ECONOMIC_SWITCHES, SWITCHES_UNAVAILABLE, buyLimitRow, economicRows, limitInputOk, summary, switchRow } from "./switches";
import type { ControlSwitchView } from "./types";

const sw = (id: string, over: Partial<ControlSwitchView> = {}): ControlSwitchView =>
  ({ id, requested: "DISABLED", effective: "DISABLED", blockReason: "", generation: 4n, requestId: "", ...over });

describe("BSC wallet and economic switches (plan 27.10.)", () => {
  it("without the owner's state a switch is never requestable", () => {
    const row = switchRow("BSC_WALLET_LAUNCHER", undefined);
    expect([row.available, row.effective, row.reason]).toEqual([false, SWITCHES_UNAVAILABLE, SWITCHES_UNAVAILABLE]);
  });
  it("shows requested, effective, the CAS generation and the request that set it", () => {
    const row = switchRow("BSC_WALLET_BUYER", sw("BSC_WALLET_BUYER", { requested: "ENABLED", effective: "ENABLED", requestId: "abc", setGeneration: 3n }));
    expect(row).toMatchObject({ label: "Buyer wallet", requested: "ENABLED", effective: "ENABLED", generation: 4n, receipt: "abc @ generation 3", available: true });
    expect(switchRow("BSC_WALLET_BUYER", sw("BSC_WALLET_BUYER")).receipt).toBe("never set");
  });
  it("an ENABLED switch of an earlier boot shows BLOCKED with its reason, never ENABLED", () => {
    const row = switchRow("BSC_ECON_SEND", sw("BSC_ECON_SEND", { requested: "ENABLED", effective: "BLOCKED", blockReason: "RESET_BY_RESTART" }));
    expect([row.effective, row.blockReason, row.available]).toEqual(["BLOCKED", "RESET_BY_RESTART", true]);
    expect(summary([row])).toBe("SEND BLOCKED (RESET_BY_RESTART)");
  });
  it("an unreadable owner state is not requestable", () => {
    const row = switchRow("BSC_ECON_SIGN", sw("BSC_ECON_SIGN", { requested: "UNKNOWN", effective: "BLOCKED", blockReason: "CONTROL_STATE_INVALID", generation: 0n }));
    expect([row.available, row.reason]).toEqual([false, "CONTROL_STATE_INVALID"]);
  });
  it("lists the five economic switches in their fixed order, missing ones unavailable", () => {
    const rows = economicRows([sw("BSC_ECON_LIVE"), sw("BSC_ECON_SIGN")]);
    expect(rows.map(r => r.id)).toEqual([...ECONOMIC_SWITCHES]);
    expect(rows.map(r => r.available)).toEqual([true, false, false, false, true]);
  });
  it("the runtime mode takes its four modes; anything else is not requestable", () => {
    const row = switchRow("BSC_RUNTIME_MODE", sw("BSC_RUNTIME_MODE", { requested: "PAPER", effective: "PAPER" }));
    expect([row.label, row.available, row.effective]).toEqual(["Runtime mode", true, "PAPER"]);
    expect(switchRow("BSC_RUNTIME_MODE", sw("BSC_RUNTIME_MODE", { requested: "LIVE", effective: "BLOCKED", blockReason: "ECONOMIC_LIVE_DISABLED" })).blockReason).toBe("ECONOMIC_LIVE_DISABLED");
    expect(switchRow("BSC_RUNTIME_MODE", sw("BSC_RUNTIME_MODE", { requested: "UNKNOWN" })).available).toBe(false);
  });
  it("the daily BUY limit shows its use on the UTC day and accepts only what the owner takes", () => {
    expect(buyLimitRow(undefined)).toMatchObject({ available: false, summary: SWITCHES_UNAVAILABLE });
    const row = buyLimitRow({ perDay: 3n, dayUtc: "2026-10-27", used: 1n, remaining: 2n, generation: 9n, requestId: "lim", setGeneration: 8n });
    expect(row).toMatchObject({ available: true, summary: "1 of 3 used on 2026-10-27 (UTC) · 2 remaining", receipt: "lim @ generation 8" });
    for (const ok of ["0", "10", "4294967295"]) expect(limitInputOk(ok)).toBe(true);
    for (const bad of ["", "-1", "01", "1.5", "x", "4294967296", " 3"]) expect(limitInputOk(bad)).toBe(false);
  });
});

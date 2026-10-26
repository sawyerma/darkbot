import { describe, expect, it } from "vitest";
import liquidatorHex from "../../../../../protocol/golden/project0/liquidator_state.hex?raw";
import walletsHex from "../../../../../protocol/golden/project0/wallets_state.hex?raw";
import walletsNonLandingHex from "../../../../../protocol/golden/project0/wallets_state_non_landing.hex?raw";
import financeHex from "../../../../../protocol/golden/project0/finance_summary.hex?raw";
import { ownerMonitor, programPinStatus, unusableByReason } from "../model/owner";
import {
  decodeFinanceSummary,
  decodeLiquidatorState,
  decodeWalletsState,
  toFinanceSnapshot,
  toLiquidatorView,
  toWalletsView,
} from "./ownerAdapter";

function bytes(hex: string): Uint8Array {
  const t = hex.trim();
  const out = new Uint8Array(t.length / 2);
  for (let i = 0; i < out.length; i += 1) out[i] = Number.parseInt(t.slice(i * 2, i * 2 + 2), 16);
  return out;
}

describe("Project 0 owner wire contract (golden frames written by the Rust reporter)", () => {
  const liquidator = toLiquidatorView(decodeLiquidatorState(bytes(liquidatorHex)));
  const wallets = toWalletsView(decodeWalletsState(bytes(walletsHex)));
  const finance = toFinanceSnapshot(decodeFinanceSummary(bytes(financeHex)));

  it("decodes the owner engine, trigger, pipeline, safety and gates", () => {
    expect(liquidator.runId).toBe("golden-run");
    expect(liquidator.engine?.runState).toBe("READY");
    expect(liquidator.engine?.reconnects).toBe(1n);
    expect(liquidator.engine?.counters.get("same_slot_unversioned_refused")).toBe(2n);
    expect(liquidator.trigger?.lanes).toEqual([1n, 2n, 3n]);
    // DEC-01: early blockhash refusals and the parking of their candidates.
    expect(liquidator.trigger).toMatchObject({ blockhashRefused: 6n, blockhashParked: 1n, blockhashRearmed: 4n,
      blockhashSuperseded: 1n, blockhashRefreshRequests: 2n });
    expect(liquidator.pipeline?.losses).toEqual([{ reason: "ROUTING", count: 2n }]);
    expect(liquidator.pipeline?.simulationP95Ns).toBeNull();
    expect(liquidator.safety?.maxTotalCapitalUsdMicro).toBe("100000000");
    expect(liquidator.capabilities[0]).toMatchObject({ name: "LIVE", effective: "BLOCKED" });
    // B01: the rebalancing plan (planned, never traded) as the owner published it.
    expect(liquidator.rebalance?.mode).toBe("OBSERVE");
    expect(liquidator.rebalance?.trades[0]).toMatchObject({ sell: "SOL", amountIn: 3_000_000_000n, minimumOut: 450_000_000n, costUsdMicro: 1_250_000n });
    expect(liquidator.rebalance?.blocked[0]).toEqual({ asset: "PREHELD", reason: "INVENTORY_BALANCE_UNKNOWN" });
    // S1-02: the applied source controls and the owner's last decision.
    expect(liquidator.sources.map((s) => [s.unit, s.mode, s.canonical])).toEqual([["geyser:publicnode", "observe", false], ["wss", "active", true]]);
    expect(liquidator.lastSourceDecision).toEqual({ seq: 3n, request: "geyser:publicnode active", applied: false, detail: "SOURCE_ACTIVE_REQUIRES_QUALIFICATION" });
    expect(liquidator.sourceConfigVersion).toBe(3n);
    // DEC-13: LIVE requested, effective BLOCKED; the decision and the CAS version.
    expect(liquidator.capabilities.find((c) => c.name === "LIVE")).toMatchObject({ requested: "LIVE", effective: "BLOCKED" });
    expect(liquidator.capabilityConfigVersion).toBe(1n);
    expect(liquidator.lastCapabilityDecision).toMatchObject({ seq: 1n, request: "LIVE LIVE", applied: true });
    // DEC-10: the approval status as the engine projected it (golden: bound to another commit).
    expect(liquidator.approval).toMatchObject({ state: "BINDING_MISMATCH", reason: "code_sha", kind: "attempt",
      approvalId: "pilot-1", maxCapitalUsdMicro: 10_000_000n, allowedTargets: 1n });
  });

  it("keeps unknown finance unknown and one snapshot id across the wallet topics", () => {
    expect(finance.totalCapitalUsdMicro).toBeNull();
    expect(finance.availableCapitalUsdMicro).toBeNull();
    expect(finance.withdrawableProfitUsdMicro).toBeNull();
    expect(finance.reservedCapitalUsdMicro).toBe("0");
    // Review E4: a read journal without reconciled balances is PARTIAL, never VERIFIED.
    expect(finance.status).toBe("PARTIAL");
    expect(wallets.attempts).toBe(0n);
    expect(liquidator.endResult).toBe("");
    expect(finance.snapshotId).toBe(wallets.financeSnapshotId);
    expect(wallets.policyReserves).toEqual([{ asset: "SOL", units: 50_000_000n }]);
  });

  it("DEC-03: decodes the N-2 non-landing status; none without a held attempt", () => {
    expect(wallets.nonLanding).toBeNull();
    const held = toWalletsView(decodeWalletsState(bytes(walletsNonLandingHex)));
    expect(held.heldAttempt).toBe("UNKNOWN");
    expect(held.nonLanding).toEqual({
      subject: "ATTEMPT",
      resultClass: "VETO_CONTRADICTION_OPEN",
      alarm: true,
      sources: [{ operator: "op-a", class: "SIGHTING" }, { operator: "op-b", class: "UNUSABLE_RATE_LIMITED" }],
      cResult: "NEGATIVE",
      holdMs: 42_000n,
      calls: 20n,
      bytes: 1_234_567n,
      reads: 3n,
    });
    expect(unusableByReason([
      { operator: "a", class: "UNUSABLE_RATE_LIMITED" }, { operator: "b", class: "USABLE" },
      { operator: "c", class: "UNUSABLE_PRUNED" }, { operator: "d", class: "UNUSABLE_RATE_LIMITED" },
    ])).toEqual([{ reason: "PRUNED", count: 1 }, { reason: "RATE_LIMITED", count: 2 }]);
  });

  it("fills the existing Overview monitor slots without inventing a sample", () => {
    const m = ownerMonitor(liquidator, finance);
    expect(m.rpcP95Ms).toBe(180);
    expect(m.decisionP95Ms).toBe(4);
    expect(ownerMonitor(null, null)).toEqual({ rpcP95Ms: null, decisionP95Ms: null, streamAgeMs: null, finance: null, programPin: null, programUpgradeLatched: null });
  });

  it("shows the Program-Data-Pin as BLOCKED with both identities, never as a pass (§6)", () => {
    const pin = liquidator.engine?.programPin ?? null;
    expect(pin).toMatchObject({ verdict: "NOT_REFERENCE", deploymentSlot: 371_000_000n, sourceTag: "mrgn-0.1.12-rc2", referenceSourceTag: "mrgn-0.1.11-rc1" });
    const m = ownerMonitor(liquidator, finance);
    const s = programPinStatus(m.programPin, m.programUpgradeLatched);
    expect(s.status).toBe("BLOCKED");
    expect(s.detail).toContain("NOT_REFERENCE · upgrade latched");
    expect(s.detail).toContain("qualified mrgn-0.1.11-rc1");
    expect(programPinStatus(null, null).status).toBe("UNKNOWN");
    expect(programPinStatus(pin && { ...pin, verdict: "REFERENCE" }, false).status).toBe("REFERENCE");
    expect(programPinStatus(pin && { ...pin, verdict: "REFERENCE" }, true).status).toBe("BLOCKED");
  });

  it("rejects malformed wire bytes", () => {
    expect(() => decodeLiquidatorState(new Uint8Array([0xff]))).toThrow();
    expect(() => decodeFinanceSummary(new Uint8Array([0xff]))).toThrow();
  });
});

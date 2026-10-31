import { describe, expect, it } from "vitest";
import liquidatorHex from "../../../../../protocol/golden/project0/liquidator_state.hex?raw";
import walletsHex from "../../../../../protocol/golden/project0/wallets_state.hex?raw";
import walletsNonLandingHex from "../../../../../protocol/golden/project0/wallets_state_non_landing.hex?raw";
import financeHex from "../../../../../protocol/golden/project0/finance_summary.hex?raw";
import { admissionLine, economicBlock, isEconomic, liveGates, ownerMonitor, programPinStatus, unusableByReason } from "../model/owner";
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
    // DEC-06: the threshold mode (ACTIVE requested, OBSERVE while N is undecided) and its comparison.
    expect(liquidator.trigger?.threshold).toMatchObject({ requested: "ACTIVE", requestedSource: "controller seq 3",
      effective: "OBSERVE", blockReason: "B8_UNDECIDED", generation: 4n, minTicks: null, ticks: 1_200n, streak: 800n,
      hits: 9n, misses: 1n, extra: 14n, indexTracked: 83_000n, observeBudgetPpm: 20_000n });
    expect(liquidator.trigger?.threshold?.sweepDetect).toEqual({ count: 10n, p50: 4_000_000n, p95: 9_000_000n, p99: 12_000_000n, max: 12_500_000n });
    expect(liquidator.trigger?.threshold?.thresholdDetect.p50).toBe(60_000n);
    expect(liquidator.trigger?.threshold?.observeOwner).toEqual({ count: 0n, p50: null, p95: null, p99: null, max: null });
    expect(liquidator.capabilities.find((c) => c.name === "THRESHOLD_TRIGGER")).toMatchObject({ requested: "ACTIVE", effective: "OBSERVE", blockReason: "B8_UNDECIDED" });
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
    // DEC-13b: without FE-06 the admission is OFF with LIVE's block reason, nothing admitted.
    expect(liquidator.admission).toMatchObject({ state: "OFF", blockReason: "FE-06/P8-LT: LIVE is not authorized (B10)",
      submitted: 0n, refused: 0n, unknown: 0n, exitsSubmitted: 0n });
    expect(admissionLine(liquidator.admission!)).toBe("OFF · FE-06/P8-LT: LIVE is not authorized (B10)"
      + " · attempts 0 submitted / 0 refused / 0 unknown · simulations 0 reused / 0 again / 0 refused"
      + " · exits 0 submitted / 0 refused / 0 unknown");
    expect(admissionLine({ ...liquidator.admission!, state: "BLOCKED", blockReason: "FE04_ATTEMPT_HELD:SUBMITTED",
      submitted: 1n, refused: 2n, lastRefusal: "FE04_FEE_UNKNOWN", simulationReused: 1n,
      lastReuseRefusal: "SIM_REUSE_SLOT", exitsRefused: 1n, lastExitRefusal: "EXIT_BLOCKHASH_STALE" }))
      .toBe("BLOCKED · FE04_ATTEMPT_HELD:SUBMITTED · attempts 1 submitted / 2 refused / 0 unknown (last: FE04_FEE_UNKNOWN)"
        + " · simulations 1 reused / 0 again / 0 refused (last: SIM_REUSE_SLOT)"
        + " · exits 0 submitted / 1 refused / 0 unknown (last: EXIT_BLOCKHASH_STALE)");
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
    // DEC-04: the B hold mode and the held attempt's A, B and finalized flows.
    expect(wallets.holdB).toBeNull();
    expect(held.holdB).toMatchObject({
      requested: "ACTIVE", effective: "OBSERVE", n: null, evidenceExact: 1n,
      inventoryReservedUsdMicro: 1_100n, capitalInUseUsdMicro: 5_000n,
      attempt: {
        mode: "OBSERVE", aWallet: [{ asset: "SOL", units: 20n }], aInventoryUsdMicro: 1_100n,
        reservedInventoryUsdMicro: 1_100n, bClass: "EXACT", bKind: "PLAIN", bInventoryUsdMicro: 100n,
        flows: [
          { asset: "LIAB", reserve: 0n, residualMax: null, actual: 150n, underestimated: false },
          { asset: "SOL", reserve: 20n, residualMax: 0n, actual: -21n, underestimated: true },
        ],
        aFlows: [{ asset: "SOL", reserve: 20n, residualMax: 0n, actual: -21n, underestimated: true }],
        settled: true, actualInventoryUsdMicro: 90n,
      },
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

  it("P8-CTRL: the owner epoch, every economic switch, the signer with its roles and FE-06 reach the view", () => {
    expect(liquidator.capabilityEpoch).toBe(2n);
    expect(liquidator.capabilities.filter((c) => isEconomic(c.name)).map((c) => [c.name, c.effective])).toEqual([
      ["ECON_LIVE", "DISABLED"], ["ECON_ARM", "DISABLED"], ["ECON_SIGN", "ENABLED"], ["ECON_SEND", "DISABLED"], ["ECON_ATTEMPT", "DISABLED"],
      ["WALLET:11111111111111111111111111111112", "ENABLED"],
    ]);
    expect(liquidator.capabilities.find((c) => c.name.startsWith("WALLET:"))?.roles).toEqual(["LIQUIDATOR", "FEE_PAYER"]);
    expect(liquidator.capabilities.find((c) => c.name === "LIVE")?.blockReason)
      .toBe("ECON_LIVE DISABLED (missing too: ECON_ARM DISABLED; ECON_SEND DISABLED; ECON_ATTEMPT DISABLED; FE-06/P8-LT: LIVE is not authorized (B10))");
    // No grant (unknown usage stays null, never 0) and the one scope previewed for this epoch.
    expect(liquidator.fe06).toMatchObject({ grantDigest: null, inForce: false, validFromUnixS: null, attemptsToday: null, attemptsGrant: null });
    expect(liquidator.fe06?.blockReason).toMatch(/^FE06_OPERATOR_AUTHORIZATION_UNDECIDED: /);
    expect(liquidator.fe06?.previews).toEqual([{ digest: "d".repeat(64), scopeMode: "ALL", ownerEpoch: 2n, validUntilUnixS: 1_760_003_600n, terminal: "PROJECT 0 FE-06 SCOPE digest=dddd…" }]);
    // The checklist names what the owner reports, gate by gate; it decides nothing.
    expect(liveGates(liquidator).map((g) => `${g.gate}:${g.met ? "met" : "missing"}`)).toEqual([
      "LIVE requested:met", "ECON_LIVE:missing", "ECON_ARM:missing", "ECON_SIGN:met", "ECON_SEND:missing", "ECON_ATTEMPT:missing",
      "Signers:met", "FE-06 grant:missing", "FE-04 approval:missing", "Program pin:missing", "Safety:met",
    ]);
    expect(liveGates({ ...liquidator, capabilities: [], fe06: null }).find((g) => g.gate === "Signers")).toEqual({ gate: "Signers", met: false, detail: "no configured signer" });
  });

  it("P8-CTRL.8: an economic request is never sent from a state that is not current or without its version and epoch", () => {
    const ok = { busy: false, live: true, version: 1n, epoch: 2n };
    expect(economicBlock(ok)).toBe("");
    expect(economicBlock({ ...ok, live: false })).toBe("Runtime owner not current");
    expect(economicBlock({ ...ok, busy: true })).toBe("Command in progress");
    expect(economicBlock({ ...ok, version: null })).toMatch(/^Capability configuration version unknown/);
    expect(economicBlock({ ...ok, epoch: null })).toMatch(/^Owner epoch unknown/);
  });
});

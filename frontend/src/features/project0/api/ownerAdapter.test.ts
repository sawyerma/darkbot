import { describe, expect, it } from "vitest";
import liquidatorHex from "../../../../../protocol/golden/project0/liquidator_state.hex?raw";
import walletsHex from "../../../../../protocol/golden/project0/wallets_state.hex?raw";
import financeHex from "../../../../../protocol/golden/project0/finance_summary.hex?raw";
import { ownerMonitor } from "../model/owner";
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
    expect(liquidator.pipeline?.losses).toEqual([{ reason: "ROUTING", count: 2n }]);
    expect(liquidator.pipeline?.simulationP95Ns).toBeNull();
    expect(liquidator.safety?.maxTotalCapitalUsdMicro).toBe("100000000");
    expect(liquidator.capabilities[0]).toMatchObject({ name: "LIVE", effective: "BLOCKED" });
    // B01: the rebalancing plan (planned, never traded) as the owner published it.
    expect(liquidator.rebalance?.mode).toBe("OBSERVE");
    expect(liquidator.rebalance?.trades[0]).toMatchObject({ sell: "SOL", amountIn: 3_000_000_000n, minimumOut: 450_000_000n, costUsdMicro: 1_250_000n });
    expect(liquidator.rebalance?.blocked[0]).toEqual({ asset: "PREHELD", reason: "INVENTORY_BALANCE_UNKNOWN" });
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

  it("fills the existing Overview monitor slots without inventing a sample", () => {
    const m = ownerMonitor(liquidator, finance);
    expect(m.rpcP95Ms).toBe(180);
    expect(m.decisionP95Ms).toBe(4);
    expect(ownerMonitor(null, null)).toEqual({ rpcP95Ms: null, decisionP95Ms: null, streamAgeMs: null, finance: null });
  });

  it("rejects malformed wire bytes", () => {
    expect(() => decodeLiquidatorState(new Uint8Array([0xff]))).toThrow();
    expect(() => decodeFinanceSummary(new Uint8Array([0xff]))).toThrow();
  });
});

import { describe, expect, it } from "vitest";
import { conditionRows, grantScope, liveIndicator, liveRequestOf, scopeSha256, scopeText, signedBnb, totalInputOk, weiOk } from "./live";
import type { BscLiveView } from "./types";

// The native owner's own output (`bsc_runtime live scope` with these flags): text and digest.
const OWNER_TEXT = "darkbot.bsc.live-grant.v1\nvenue=BSC_VENUE_FLAP\nstrategies=C3+STRATEGY_C\n"
  + "sources=BSC_INGRESS_PUBLICNODE,BSC_INGRESS_DRPC\nadapters=CLUB48_BUILDER_V1\n"
  + "max_input_wei=1000000000000000\nmax_spend_wei=20000000000000000\nmax_attempts=2\nexpires_unix_ms=1793000000000\n"
  // BSC3-069: the execution risk (`--max-buy-slippage-bps 300 ... --max-sell-gas-limit 1500000`).
  + "max_buy_slippage_bps=300\nmax_exit_slippage_bps=500\nmax_gas_price_wei=1000000000\n"
  + "max_buy_gas_limit=3000000\nmax_approve_gas_limit=100000\nmax_sell_gas_limit=1500000\n"
  // 31.10. order: the limits (`--max-open-positions 3 ... --min-buy-interval-ms 60000`).
  + "max_open_positions=3\nmax_realized_loss_wei=5000000000000000\n"
  + "max_daily_realized_loss_wei=2000000000000000\nmin_buy_interval_ms=60000\n";
const OWNER_SHA = "567cec77f585f80ab0ab9408bcdb2ccc7db7ba2f3d70d770c379195dea6afceb";
// The same scope as an EXIT-ONLY grant (`... --exit-only`, BSC3-058).
const OWNER_EXIT_SHA = "e012c2fd97397c20f75972c2435989cac4ed0843260534b7d020a812f25a2162";

describe("BSC LIVE tab (plan 28.10.)", () => {
  it("builds the owner's canonical scope text and digest byte for byte", async () => {
    const scope = {
      venue: "BSC_VENUE_FLAP", strategies: "C3+STRATEGY_C", sources: ["BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_DRPC"],
      adapters: ["CLUB48_BUILDER_V1"], maxInputWei: "1000000000000000", maxSpendWei: "20000000000000000",
      maxAttempts: 2, expiresUnixMs: 1793000000000n, exitOnly: false,
      risk: {
        maxBuySlippageBps: 300, maxExitSlippageBps: 500, maxGasPriceWei: 1000000000n,
        maxBuyGasLimit: 3000000n, maxApproveGasLimit: 100000n, maxSellGasLimit: 1500000n,
      },
      limits: {
        maxOpenPositions: 3, maxRealizedLossWei: "5000000000000000",
        maxDailyRealizedLossWei: "2000000000000000", minBuyIntervalMs: 60000n,
      },
    };
    expect(scopeText(scope)).toBe(OWNER_TEXT);
    expect(await scopeSha256(scopeText(scope))).toBe(OWNER_SHA);
    const exits = { ...scope, exitOnly: true };
    expect(scopeText(exits)).toBe(`${OWNER_TEXT}exit_only=true\n`);
    expect(await scopeSha256(scopeText(exits))).toBe(OWNER_EXIT_SHA);
  });

  it("checks a grant form as the owner does and dates its expiry", () => {
    const form = { strategies: "C3", sources: ["BSC_INGRESS_PUBLICNODE"], adapters: ["CLUB48_BUILDER_V1"], maxInputWei: "1000", maxSpendWei: "5000", maxAttempts: "2", hours: "4", exitOnly: false,
      buySlippageBps: "300", exitSlippageBps: "500", gasPriceWei: "1000000000", buyGasLimit: "3000000", approveGasLimit: "100000", sellGasLimit: "1500000",
      maxOpenPositions: "3", maxRealizedLossWei: "5000", maxDailyRealizedLossWei: "2000", minBuyIntervalMs: "0" };
    const grantable = ["BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_DRPC"];
    const ok = grantScope(form, 1_000, grantable);
    expect("scope" in ok && ok.scope.expiresUnixMs).toBe(1_000n + 4n * 3_600_000n);
    expect("scope" in ok && ok.scope.venue).toBe("BSC_VENUE_FLAP");
    for (const bad of [
      { ...form, strategies: "ALL" },
      { ...form, sources: [] },
      { ...form, sources: ["BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_PUBLICNODE"] },
      { ...form, adapters: ["BLOCKRAZOR_RPC_V1"] },
      { ...form, maxInputWei: "0" },
      { ...form, maxInputWei: "01" },
      { ...form, maxSpendWei: "999" },
      { ...form, maxAttempts: "11" },
      // BSC3-069: a slippage of 100 %, no gas price, a gas limit below a transfer's, a non-decimal.
      { ...form, buySlippageBps: "10000" },
      { ...form, exitSlippageBps: "" },
      { ...form, gasPriceWei: "0" },
      { ...form, sellGasLimit: "20999" },
      { ...form, buyGasLimit: "20999" },
      { ...form, approveGasLimit: "20999" },
      { ...form, gasPriceWei: "18446744073709551616" },
      { ...form, approveGasLimit: "1e5" },
      { ...form, maxAttempts: "0" },
      { ...form, hours: "0" },
      // 31.10. order: no position, no loss bound, a day above the grant's, a non-decimal interval.
      { ...form, maxOpenPositions: "0" },
      { ...form, maxRealizedLossWei: "0" },
      { ...form, maxDailyRealizedLossWei: "0" },
      { ...form, maxDailyRealizedLossWei: "5001" },
      { ...form, minBuyIntervalMs: "1e3" },
      { ...form, minBuyIntervalMs: "" },
      // Audit E 1.6: a source the projection does not name grantable (no adapter endpoint).
      { ...form, sources: ["BSC_INGRESS_GETH"] },
      { ...form, sources: ["BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_48CLUB"] },
    ]) expect("error" in grantScope(bad, 1_000, grantable)).toBe(true);
    // Before a choice the form names the first grantable source; without any it offers none.
    const unchosen = grantScope({ ...form, sources: null }, 1_000, grantable);
    expect("scope" in unchosen && unchosen.scope.sources).toEqual(["BSC_INGRESS_PUBLICNODE"]);
    expect("error" in grantScope(form, 1_000, [])).toBe(true);
    expect("error" in grantScope({ ...form, sources: null }, 1_000, [])).toBe(true);
    expect(weiOk("340282366920938463463374607431768211455")).toBe(true);
    expect(weiOk("340282366920938463463374607431768211456")).toBe(false);
    expect([totalInputOk("10"), totalInputOk("0"), totalInputOk("11"), totalInputOk("01")]).toEqual([true, true, false, false]);
  });

  it("lists the owner's conditions with its reasons and the requests that meet the scope", () => {
    const live = { preconditions: [{ id: "ECON_SEND", met: false, reason: "ECON_SEND_DISABLED" }, { id: "LIVE_GRANT", met: true, reason: "" }] } as unknown as BscLiveView;
    const rows = conditionRows(live);
    expect(rows.map(r => [r.label, r.met, r.reason])).toEqual([["SEND ENABLED", false, "ECON_SEND_DISABLED"], ["LIVE grant of a confirmed scope", true, ""]]);
    expect(conditionRows(null)).toEqual([]);
    const row = (id: string) => ({ id, requested: "OFF", effective: "OFF", blockReason: "", generation: 1n, requestId: "" });
    expect(liveRequestOf(row("BSC_RUNTIME_MODE")).action).toBe("bsc.runtime.mode.set");
    expect(liveRequestOf(row("BSC_VENUE_FLAP")).action).toBe("bsc.venue.mode.set");
    expect(liveRequestOf(row("BSC_EXEC")).action).toBe("bsc.source.mode.set");
    expect(signedBnb("-1500000000000000")).toBe("−0.0015 BNB");
    expect(signedBnb("2000000000000000000")).toBe("2 BNB");
  });

  it("is the one LIVE indicator: the owner's gate and the LIVE path from bsc.live.state (Audit E 4.10)", () => {
    const live = (gate: string, pathState: string, blockers: string[] = []) => ({ gate, pathState, blockers }) as unknown as BscLiveView;
    expect(liveIndicator(null).state).toBe("UNKNOWN");
    expect(liveIndicator(live("UNKNOWN", "NOT_RUNNING")).state).toBe("UNKNOWN");
    expect(liveIndicator(live("CLEAR", "ARMED")).state).toBe("LIVE");
    expect(liveIndicator(live("CLEAR", "PROBING")).state).toBe("BLOCKED");
    const blocked = liveIndicator(live("BLOCKED", "ARMED", ["LIVE_GRANT_MISSING"]));
    expect([blocked.state, blocked.detail]).toEqual(["BLOCKED", "gate BLOCKED (first: LIVE_GRANT_MISSING) · LIVE path ARMED"]);
  });
});

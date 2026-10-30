import { describe, expect, it } from "vitest";
import { conditionRows, grantScope, liveRequestOf, scopeSha256, scopeText, signedBnb, totalInputOk, weiOk } from "./live";
import type { BscLiveView } from "./types";

// The native owner's own output (`bsc_runtime live scope` with these flags): text and digest.
const OWNER_TEXT = "darkbot.bsc.live-grant.v1\nvenue=BSC_VENUE_FLAP\nstrategies=C3+STRATEGY_C\n"
  + "sources=BSC_INGRESS_PUBLICNODE,BSC_INGRESS_DRPC\nadapters=CLUB48_BUILDER_V1\n"
  + "max_input_wei=1000000000000000\nmax_spend_wei=20000000000000000\nmax_attempts=2\nexpires_unix_ms=1793000000000\n"
  // BSC3-069: the execution risk (`--max-buy-slippage-bps 300 ... --max-sell-gas-limit 1500000`).
  + "max_buy_slippage_bps=300\nmax_exit_slippage_bps=500\nmax_gas_price_wei=1000000000\n"
  + "max_buy_gas_limit=3000000\nmax_approve_gas_limit=100000\nmax_sell_gas_limit=1500000\n";
const OWNER_SHA = "ad1a8f82ec3b81acd6410b40402fd2865e5d0cfff5e117778a40fd24cc3ad65a";
// The same scope as an EXIT-ONLY grant (`... --exit-only`, BSC3-058).
const OWNER_EXIT_SHA = "dc80d35ca0a75070d6cb079513b9c5d6a026fe238d25417d570c1c44dcf19c65";

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
    };
    expect(scopeText(scope)).toBe(OWNER_TEXT);
    expect(await scopeSha256(scopeText(scope))).toBe(OWNER_SHA);
    const exits = { ...scope, exitOnly: true };
    expect(scopeText(exits)).toBe(`${OWNER_TEXT}exit_only=true\n`);
    expect(await scopeSha256(scopeText(exits))).toBe(OWNER_EXIT_SHA);
  });

  it("checks a grant form as the owner does and dates its expiry", () => {
    const form = { strategies: "C3", sources: ["BSC_INGRESS_PUBLICNODE"], adapters: ["CLUB48_BUILDER_V1"], maxInputWei: "1000", maxSpendWei: "5000", maxAttempts: "2", hours: "4", exitOnly: false,
      buySlippageBps: "300", exitSlippageBps: "500", gasPriceWei: "1000000000", buyGasLimit: "3000000", approveGasLimit: "100000", sellGasLimit: "1500000" };
    const ok = grantScope(form, 1_000);
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
    ]) expect("error" in grantScope(bad, 1_000)).toBe(true);
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
});

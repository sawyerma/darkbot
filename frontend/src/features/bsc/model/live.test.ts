import { describe, expect, it } from "vitest";
import { ATTEMPT_COLUMNS, COUNTERS_MEANING, NO_EVALUATION, evaluationOf, attemptCells, campaignBeyondState, campaignRows, exitDeciderRows, recoveryRows, conditionRows, grantScope, liveIndicator, liveRequestOf, scopeSha256, scopeText, signedBnb, totalInputOk, weiOk } from "./live";
import type { BscLiveView, LiveAttemptView, LiveCampaignView } from "./types";

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

  it("shows who decides the open lots' exits and the receipt recovery as the runtime names them (REV.1.0 R1/R3b)", () => {
    // TEST_ONLY values, each its own.
    const l = { exitUndecided: ["0xu1", "0xu2"], stranded: ["0xs1 LIVE_POSITION_OFF_CURVE"], lotReadFaults: 3n, lotReadFault: "curve fault",
      recoveryPending: ["0xc1 RECOVERY_NOT_FINAL", "0xd5 RECOVERY_FAILED"], recovered: 5n, recoveryFaults: 7n, recoveryFault: "recovery fault" } as unknown as BscLiveView;
    expect(exitDeciderRows(l).map(r => [r.label, r.value])).toEqual([
      ["Exit undecided", "0xu1 · 0xu2"], ["Stranded lots", "0xs1 LIVE_POSITION_OFF_CURVE"], ["Curve read faults", "3"]]);
    expect(exitDeciderRows(l)[2]?.hint).toBe("curve fault");
    const r = recoveryRows(l);
    expect(r.row.value).toBe("5 resolved · 2 pending · 7 failed reads");
    expect(r.row.hint).toBe("recovery fault");
    expect(r.pending).toEqual([["0xc1", "RECOVERY_NOT_FINAL"], ["0xd5", "RECOVERY_FAILED"]]);
    // Nothing named: no value is made up.
    const none = recoveryRows(undefined);
    expect([none.row.value, none.pending]).toEqual(["0 resolved · 0 pending · 0 failed reads", []]);
    expect(exitDeciderRows(undefined).map(x => x.value)).toEqual(["—", "—", "0"]);
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

  it("shows the campaign's counts as recorded, every rate with its denominator (REV.1.0 R2b)", () => {
    // Every count its own value: a swapped field is visible.
    const c = { verdict: "CAMPAIGN_OPEN", releaseEvidence: "RELEASE_EVIDENCE_NOT_READY", reasons: [], planned: 11n, observed: 41n, selected: 31n, admitted: 5n, skipped: 37n,
      written: 9n, acquired: 8n, reverted: 2n, dropped: 3n, late: 4n, unresolved: 6n, passed: 12n, failed: 13n, undecided: 14n, consecutiveMisses: 15n, beforePrecommit: 16n,
      unverifiable: 17n, coveredS: 3600n, uncoveredS: 400n, coveredPpm: 900000n, skipReasons: ["HASH_ONLY=7"], cells: [], manifestSha256: "", precommitSha256: "", buildSha256: "",
      evaluatedUnixMs: 1n, recordSha256: "" } as LiveCampaignView;
    const rows = Object.fromEntries(campaignRows(c).map(r => [r.label, r.value]));
    expect(rows).toEqual({
      "Attempts written": "9/11 planned", "Acquired": "8/9", "Passed · failed · undecided": "12/9 · 13/9 · 14/9",
      "Reverted · dropped · late · unresolved": "2/9 · 3/9 · 4/9 · 6/9", "Selected launches": "31/41", "Skipped launches": "37/41", "Admitted by the owner": "5",
      "Longest run of misses": "15", "Before the commitment": "16", "Unverifiable launches": "17", "Coverage": "3600 s covered · 400 s not",
    });
    expect(campaignRows(c).find(r => r.label === "Skipped launches")?.hint).toBe("HASH_ONLY=7");
    expect(campaignRows(c).find(r => r.label === "Coverage")?.hint).toContain("900000 ppm");
    expect(NO_EVALUATION).toBe("keine Bewertung");
  });

  it("finds the reporter's warning that the campaign section is beyond the state, no other (REV.1.0 R2b re-check F3)", () => {
    const beyond = "LIVE_CAMPAIGN_BEYOND_STATE 3 records, 2 warnings";
    expect(campaignBeyondState(["LIVE_ATTEMPT_INVALID 1, first x", beyond])).toBe(beyond);
    expect(campaignBeyondState(["LIVE_CAMPAIGN_BEYOND_STATEX 1", "LIVE_CAMPAIGN_VERDICT_INVALID"])).toBeUndefined();
  });

  it("shows every recorded field of an attempt, the counters as counts with their meaning (REV.1.0 R2b review)", () => {
    const tx = `0x${"01".repeat(32)}`;
    // TEST_ONLY values, every field its own: a swapped field is visible.
    const a: LiveAttemptView = { ordinal: 1n, tx, kind: "C3_BUY", cell: "EXTERNAL_FULL_LAUNCH", state: "INCLUDED", verdict: "TEST_VERDICT", acquisition: "PASS", canonical: "FAIL",
      timing: "UNKNOWN", placement: "NOT_APPLICABLE", capture: "TEST_CAPTURE", local: "TEST_LOCAL", economics: "TEST_ECONOMICS", reasons: ["TIMING_WITNESS_MISSING", "PLACEMENT_LATE"],
      window: "INCOMPLETE", windowWhy: "WINDOW_NOT_FINAL", trades: 7n, buys: 5n, sells: 2n, firstExternalPrice: "250", priceVsFirstExternalPpm: 800000n,
      ingressToSendNs: 900n, selectToSendNs: 500n, sendToAckNs: 8000n, attempts: 21n, acquired: 31n, passed: 41n, failed: 51n, undecided: 61n,
      adapters: [{ builder: "CLUB48_BUILDER_V1", outcome: "SENT", answer: "ACCEPTED", adapter: 7n, boundary: "WRITE", qualified: true, tWriteCheckNs: 1800n, tBoundaryNs: 1905n, tQueueNs: 1950n, tAnswerNs: 9950n }],
      attempt: `0x${"a0".repeat(32)}`, run: 101n, token: `0x${"ab".repeat(20)}`, launchHash: `0x${"1a".repeat(32)}`, trigger: "BSC_INGRESS_PUBLICNODE",
      blockNumber: 130000010n, blockHash: `0x${"b1".repeat(32)}`, blockMs: 1793000000000n, tokens: "5000", ethWei: "1000", gasWei: "21", ownPrice: "200", placed: "LATE",
      launchIndex: 4n, ownIndex: 6n, buysBefore: 1n, clockId: 2n, tIngressNs: 1000n, tSelectNs: 1400n, tSendNs: 1900n, tAckNs: 9900n, deadlineMisses: 3n, windowMs: 15000n,
      launchBlock: 130000000n, launchMs: 1792999990000n, windowEndBlock: 130000020n, t0Exact: false, firstExternalBuyTx: `0x${"e1".repeat(32)}`, firstExternalBuyBlock: 130000001n,
      firstExternalBuyIndex: 8n, buyWei: "9000", sellWei: "3000", evaluatedUnixMs: 1793000050000n };
    const cells = attemptCells(a, 1793000050000n);
    expect(cells).toHaveLength(ATTEMPT_COLUMNS.length);
    expect(Object.fromEntries(ATTEMPT_COLUMNS.map((h, i) => [h, cells[i]]))).toEqual({
      "#": "1",
      "Tx · attempt · run": `${tx} · attempt 0x${"a0".repeat(32)} · run 101`,
      "Kind · cell": "C3_BUY · EXTERNAL_FULL_LAUNCH",
      "Token · launch · trigger": `0x${"ab".repeat(20)} · launch 0x${"1a".repeat(32)} · trigger BSC_INGRESS_PUBLICNODE`,
      "Block": `130000010 · 0x${"b1".repeat(32)} · 2026-10-26T07:33:20.000Z`,
      "State": "INCLUDED",
      "Verdict": "TEST_VERDICT",
      "Components": "acquisition PASS · canonical FAIL · timing UNKNOWN · placement NOT_APPLICABLE · capture TEST_CAPTURE · local TEST_LOCAL · economics TEST_ECONOMICS",
      "Tokens · cost": "tokens 5000 · input 1000 wei · gas 21 wei",
      "Placement": "LATE · launch index 4 · own index 6 · 1 BUYs between",
      "15-s window": "INCOMPLETE (WINDOW_NOT_FINAL) · 15000 ms from block 130000000 (2026-10-26T07:33:10.000Z) to 130000020 · trades 7 (buys 5 9000 wei, sells 2 3000 wei)",
      "Price vs first external BUY": `own 200 · first external 250 (wei per 10^18) · 800000 ppm · first external BUY 0x${"e1".repeat(32)} (block 130000001, index 8)`,
      "Local timing": "T0 UNKNOWN · clock 2 · ingress 1000 ns · select 1400 ns · send 1900 ns · ack 9900 ns · ingress→send 900 ns · select→send 500 ns · send→ack 8000 ns · deadline misses 3",
      // Review L8: C3 verdict counts, never a share of the attempts.
      "Counters": "acquired 31/21 LIVE BUYs · C3 verdicts so far: 41 passed · 51 failed · 61 undecided",
      "Adapters": "CLUB48_BUILDER_V1 (adapter 7) SENT ACCEPTED · WRITE qualified · write-check 1800 ns · boundary 1905 ns · queue 1950 ns · answer 9950 ns",
      "Reasons": "TIMING_WITNESS_MISSING · PLACEMENT_LATE",
      "Evaluated": "2026-10-26T07:34:10.000Z",
    });
    expect(COUNTERS_MEANING).toContain("passed · failed · undecided zählen nur die Verdikte der C3-BUYs");
    // Review M4: a record of another evaluation is marked: against the verdict's, without one
    // against the newest attempt's.
    expect(attemptCells(a, 1793000060000n).at(-1)).toBe("2026-10-26T07:34:10.000Z · of another evaluation");
    const newer = { ...a, evaluatedUnixMs: 1793000070000n };
    expect(evaluationOf(undefined, [a, newer, a])).toBe(1793000070000n);
    expect(evaluationOf({ evaluatedUnixMs: 1793000050000n } as LiveCampaignView, [a, newer])).toBe(1793000050000n);
    expect(evaluationOf(undefined, [])).toBeUndefined();
    expect(attemptCells(a).at(-1)).toBe("2026-10-26T07:34:10.000Z");
    // The window as written, the exact T0 only when recorded, nothing where the record names nothing.
    const bare: LiveAttemptView = { ...a, window: "COMPLETE", windowWhy: "", t0Exact: true, run: undefined, launchHash: "", trigger: "", blockNumber: undefined, blockHash: "",
      blockMs: undefined, tokens: undefined, ethWei: undefined, gasWei: undefined, placed: "", adapters: [], reasons: [], ownPrice: undefined, firstExternalPrice: undefined,
      priceVsFirstExternalPpm: undefined, firstExternalBuyTx: "", firstExternalBuyBlock: undefined, firstExternalBuyIndex: undefined, launchMs: 9000000000000000n };
    const b = attemptCells(bare);
    expect([b[1], b[3], b[4], b[8], b[9], b[11], b[14], b[15]]).toEqual([`${tx} · attempt 0x${"a0".repeat(32)} · run —`, `0x${"ab".repeat(20)} · launch — · trigger —`, "not included",
      "tokens — · input — · gas —", "—", "own — · first external — (wei per 10^18) · — · first external BUY — (block —, index —)", "—", "—"]);
    expect(b[10]).toBe("COMPLETE · 15000 ms from block 130000000 (9000000000000000 ms) to 130000020 · trades 7 (buys 5 9000 wei, sells 2 3000 wei)");
    expect(b[12]).toMatch(/^T0 exact \(recorded\) · /);
    // Review L9: the record's word, never a reading of whether it names a reason.
    expect(attemptCells({ ...a, windowWhy: "" })[10]).toMatch(/^INCOMPLETE · 15000 ms/);
  });
});

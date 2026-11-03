import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { ATTEMPT_COLUMNS, COUNTERS_MEANING, NO_EVALUATION, builderPolicyText, divergenceText, evaluationOf, attemptCells, campaignBeyondState, campaignRows, exitDeciderRows, heldPositions, operatorDecisions, outcomeRows, pathRows, presenceText, qualificationAgeText, recoveryRows, conditionRows, grantChoices, grantScope, liveIndicator, liveRequestOf, scopeSha256, scopeText, signedBnb, totalInputOk, weiOk } from "./live";
import type { BscLiveView, LiveAttemptView, LiveCampaignView } from "./types";

// Audit E 4.8: the owner's grant vector (bsc/engine/crates/execution/tests/fixtures/live_grant/), which
// the native owner's test `the_grant_vector_is_the_owners` reads too: its canonical scope text and the
// variants it refuses.
const VECTOR = new URL("../../../../../bsc/engine/crates/execution/tests/fixtures/live_grant/", import.meta.url);
const OWNER_TEXT = readFileSync(new URL("scope.txt", VECTOR), "utf8");
const REFUSED = readFileSync(new URL("refused.txt", VECTOR), "utf8").split("\n").filter(l => l && !l.startsWith("#"));
// The owner's grant choices as bsc.live.state projects them (status::tests checks the projection).
const CHOICES = { venues: ["BSC_VENUE_FLAP"], strategies: ["C3", "STRATEGY_C", "C3+STRATEGY_C"], adapters: ["CLUB48_BUILDER_V1", "BLOCKRAZOR_BUILDER_V1"] };
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
        maxBuyGasLimit: 3000000n, maxApproveGasLimit: 100000n, maxSellGasLimit: 1500000n, outlierAck: false,
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
    // REV.1.0 R4: a risk confirmed as an outlier says so after its bounds, as the owner's text does.
    const acked = { ...scope, risk: { ...scope.risk, outlierAck: true } };
    expect(scopeText(acked)).toBe(OWNER_TEXT.replace("max_sell_gas_limit=1500000\n", "max_sell_gas_limit=1500000\nrisk_outlier_ack=YES\n"));
  });

  it("refuses every variant the owner refuses (the shared grant vector)", () => {
    // The form of OWNER_TEXT's scope; each refused key maps to its form field.
    const form = { strategies: "C3+STRATEGY_C", sources: ["BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_DRPC"], adapters: ["CLUB48_BUILDER_V1"],
      maxInputWei: "1000000000000000", maxSpendWei: "20000000000000000", maxAttempts: "2", hours: "1", exitOnly: false,
      buySlippageBps: "300", exitSlippageBps: "500", gasPriceWei: "1000000000", buyGasLimit: "3000000", approveGasLimit: "100000", sellGasLimit: "1500000", outlierAck: false,
      maxOpenPositions: "3", maxRealizedLossWei: "5000000000000000", maxDailyRealizedLossWei: "2000000000000000", minBuyIntervalMs: "60000" };
    const grantable = ["BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_DRPC"];
    const ok = grantScope(form, 1_000, grantable, 10, CHOICES);
    expect("scope" in ok && scopeText({ ...ok.scope, expiresUnixMs: 1793000000000n })).toBe(OWNER_TEXT);
    const field: Record<string, string> = {
      max_input_wei: "maxInputWei", max_spend_wei: "maxSpendWei", max_attempts: "maxAttempts",
      max_buy_slippage_bps: "buySlippageBps", max_exit_slippage_bps: "exitSlippageBps", max_gas_price_wei: "gasPriceWei",
      max_buy_gas_limit: "buyGasLimit", max_approve_gas_limit: "approveGasLimit", max_sell_gas_limit: "sellGasLimit",
      max_open_positions: "maxOpenPositions", max_realized_loss_wei: "maxRealizedLossWei", max_daily_realized_loss_wei: "maxDailyRealizedLossWei",
    };
    expect(REFUSED.length).toBe(17);
    for (const line of REFUSED) {
      const [key, value] = [line.slice(0, line.indexOf("=")), line.slice(line.indexOf("=") + 1)];
      const list = value ? value.split(",") : [];
      const into = field[key];
      const list_key = key === "sources" || key === "adapters";
      expect(list_key || into !== undefined, line).toBe(true);
      const bad = list_key ? { ...form, [key]: list } : { ...form, [into ?? key]: value };
      expect("error" in grantScope(bad, 1_000, grantable, 10, CHOICES), line).toBe(true);
    }
  });

  it("checks a grant form as the owner does and dates its expiry", () => {
    const form = { strategies: "C3", sources: ["BSC_INGRESS_PUBLICNODE"], adapters: ["CLUB48_BUILDER_V1"], maxInputWei: "1000", maxSpendWei: "5000", maxAttempts: "2", hours: "4", exitOnly: false,
      buySlippageBps: "300", exitSlippageBps: "500", gasPriceWei: "1000000000", buyGasLimit: "3000000", approveGasLimit: "100000", sellGasLimit: "1500000", outlierAck: true,
      maxOpenPositions: "3", maxRealizedLossWei: "5000", maxDailyRealizedLossWei: "2000", minBuyIntervalMs: "0" };
    const grantable = ["BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_DRPC"];
    const ok = grantScope(form, 1_000, grantable, 10, CHOICES);
    expect("scope" in ok && ok.scope.expiresUnixMs).toBe(1_000n + 4n * 3_600_000n);
    expect("scope" in ok && ok.scope.venue).toBe("BSC_VENUE_FLAP");
    expect("scope" in ok && ok.scope.risk.outlierAck).toBe(true);
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
    ]) expect("error" in grantScope(bad, 1_000, grantable, 10, CHOICES)).toBe(true);
    // Before a choice the form names the first grantable source; without any it offers none.
    const unchosen = grantScope({ ...form, sources: null }, 1_000, grantable, 10, CHOICES);
    expect("scope" in unchosen && unchosen.scope.sources).toEqual(["BSC_INGRESS_PUBLICNODE"]);
    expect("error" in grantScope(form, 1_000, [], 10, CHOICES)).toBe(true);
    expect("error" in grantScope({ ...form, sources: null }, 1_000, [], 10, CHOICES)).toBe(true);
    // Audit E 3.1: a lowered maximum bounds the attempts; without one no grant is formed.
    expect("scope" in grantScope(form, 1_000, grantable, 2, CHOICES)).toBe(true);
    expect("error" in grantScope({ ...form, maxAttempts: "3" }, 1_000, grantable, 2, CHOICES)).toBe(true);
    expect("error" in grantScope(form, 1_000, grantable, undefined, CHOICES)).toBe(true);
    // Audit E 4.9: without the owner's projected choices nothing is grantable.
    expect(grantScope(form, 1_000, grantable, 10, grantChoices(null))).toEqual({ error: "no venue the native owner can grant (bsc.live.state)" });
    expect(weiOk("340282366920938463463374607431768211455")).toBe(true);
    expect(weiOk("340282366920938463463374607431768211456")).toBe(false);
    expect([totalInputOk("10", 10), totalInputOk("0", 10), totalInputOk("11", 10), totalInputOk("01", 10)]).toEqual([true, true, false, false]);
    // Audit E 3.1: the configured maximum bounds it, none without one.
    expect([totalInputOk("5", 5), totalInputOk("6", 5), totalInputOk("0", undefined)]).toEqual([true, false, false]);
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

  it("shows the LIVE path's counters, the second source, the presence and the qualification's age as projected (REV.1.0 R10)", () => {
    const l = {
      nextNonce: 41n, armedGeneration: 6n, refusals: ["OWNER=2", "PLAN=1"], responses: ["ACCEPTED=3"], probes: 16n,
      included: 11n, reverted: 12n, replaced: 13n, finalized: 14n, orphaned: 15n,
      latencySamples: 9n, latencyP50Ns: 100n, latencyP99Ns: 200n, latencyMaxNs: 300n,
      divergenceQuarantine: true, confirmState: "DISAGREES", presenceAnchor: "0xAB", presenceSigner: "0xab",
      grant: { riskScopeSha256: "r1" }, qualification: "VERIFIED", qualificationRiskScopeSha256: "r1",
      qualificationObservedUnixMs: 0n, qualificationMaxAgeMs: 86400000n,
      positions: [{ state: "OPEN" }, { state: "EXITING" }, { state: "PENDING" }, { state: "CLOSED" }, { state: "FAILED" }, { state: "WRITTEN_OFF" }],
    } as unknown as BscLiveView;
    expect(pathRows(l).map(r => r.value)).toEqual(["41", "grant generation 6", "OWNER=2 · PLAN=1", "16", "p50 100 ns · p99 200 ns · max 300 ns (9 samples)"]);
    expect(outcomeRows(l).map(r => r.value)).toEqual(["ACCEPTED=3", "11 included · 12 reverted · 13 replaced · 14 final · 15 orphaned"]);
    expect(divergenceText(l)).toBe("second source DISAGREES · quarantine ON · every send refused, exits too, while it DISAGREES");
    expect(presenceText(l)).toBe("anchor 0xAB · the grant is signed by it");
    expect(qualificationAgeText(l)).toBe("oldest evidence 1970-01-01T00:00:00.000Z · max age 86400000 ms · risk decision for this grant's risk scope");
    // A revoke ends the exits of the held positions (pending, open, exiting).
    expect(heldPositions(l)).toBe(3);
    // Another signer, an unsigned grant, OFF; another risk scope, no age bound.
    const other = { ...l, presenceSigner: "0xcd", qualificationRiskScopeSha256: "r2", qualificationMaxAgeMs: 0n } as BscLiveView;
    expect(presenceText(other)).toBe("anchor 0xAB · the grant is signed by 0xcd, not the anchor: BUYs refused");
    expect(qualificationAgeText(other)).toContain("no max age · risk decision for another grant risk scope");
    expect(presenceText({ ...l, presenceSigner: "" } as BscLiveView)).toContain("unsigned: BUYs refused (LIVE_PRESENCE_UNSIGNED)");
    expect(presenceText({ ...l, presenceAnchor: "" } as BscLiveView)).toBe("OFF · BUYs need no presence signature");
    // Nothing projected: nothing made up.
    expect([divergenceText(undefined), presenceText(undefined), qualificationAgeText(undefined), heldPositions(undefined)]).toEqual([undefined, undefined, undefined, 0]);
    expect(pathRows(undefined).map(r => r.value)).toEqual(["—", "—", "—", "—", "no sample"]);
    expect(outcomeRows(undefined)[1]?.value).toBe("0 included · 0 reverted · 0 replaced · 0 final · 0 orphaned");
    expect(qualificationAgeText({ ...l, qualification: "NONE" } as BscLiveView)).toBeUndefined();
  });

  it("offers the operator's decisions at the ledger's version, exit-now and the burn locked unless their switch is ON (REV.1.0 R3b)", () => {
    const l = (ledgerVersion: bigint | undefined, operatorExit: boolean | undefined, nonceBurn: boolean | undefined) =>
      ({ ledgerVersion, operatorExit, nonceBurn }) as unknown as BscLiveView;
    const on = operatorDecisions(l(8n, true, true));
    expect([on.version, on.exitNow.locked, on.burn.locked, on.row.value]).toEqual([8n, false, false, "ledger version 8 · exit-now ON · nonce burn ON"]);
    const off = operatorDecisions(l(8n, false, false));
    expect([off.exitNow, off.burn]).toEqual([
      { locked: true, why: "gesperrt: BSC_EXEC_LIVE_OPERATOR_EXIT ist OFF" }, { locked: true, why: "gesperrt: BSC_EXEC_LIVE_NONCE_BURN ist OFF" }]);
    // Unknown (no readable configuration, an unreadable ledger): locked, no version made up.
    const unknown = operatorDecisions(undefined);
    expect([unknown.version, unknown.exitNow.locked, unknown.burn.locked]).toEqual([undefined, true, true]);
    expect(unknown.exitNow.why).toContain("unbekannt");
    expect(unknown.row.value).toBe("ledger version — (unknown) · exit-now locked · nonce burn locked");
  });

  it("shows why the bound builder policy refuses LIVE BUYs and until when it is fresh (REV.1.0 R4)", () => {
    const l = (builderPolicyRefusal: string | undefined, builderPolicyFreshUntilUnixMs: bigint | undefined) =>
      ({ builderPolicyRefusal, builderPolicyFreshUntilUnixMs }) as unknown as BscLiveView;
    expect(builderPolicyText(l(undefined, 1_793_000_000_000n))).toBe("holds · fresh until 2026-10-26T07:33:20.000Z");
    expect(builderPolicyText(l("LIVE_BUILDER_POLICY_STALE: x", 1_793_000_000_000n))).toBe("BUYs refused: LIVE_BUILDER_POLICY_STALE: x · fresh until 2026-10-26T07:33:20.000Z");
    // An import: never fresh, no time made up.
    expect(builderPolicyText(l("LIVE_BUILDER_POLICY_STALE: y", undefined))).toBe("BUYs refused: LIVE_BUILDER_POLICY_STALE: y");
    expect([builderPolicyText(l(undefined, undefined)), builderPolicyText(null)]).toEqual([undefined, undefined]);
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

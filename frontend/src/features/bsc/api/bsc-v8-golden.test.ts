import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { classify } from "@/core/realtime/bscQuery";
import { Availability, DataQuality, ExitReason, QueryRefusal, ReplayVerdict, TimeSource, TradePlacement, TradeStatus, TransferStatus, TrainingRunStatus, TrainingRunStopKind, PolicyUnit, WalletRole, V8_CONTRACTS, decodeV8, encodeV8, type V8ContractName } from "./bscV8Adapter";

// BSC-V8-09 / FE-V8-17: the prepared V8 contracts, Rust↔TS. gateway/tests/bsc_v8_goldens.rs
// encodes the bytes with prost; here protobuf-es decodes them, the values survive exactly (u64
// and i64 limits, >u128 signed wei text, absent vs present zero) and re-encode byte for byte.
const dir = new URL("../../../../../protocol/golden/bsc/v8/", import.meta.url);
const golden = (name: string) => Uint8Array.from(Buffer.from(readFileSync(new URL(`${name}.hex`, dir), "utf8").trim(), "hex"));
const U256_MAX = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
const U64_MAX = 18446744073709551615n;
const I64_MIN = -9223372036854775808n;

describe("prepared BSC V8 contracts (Rust golden bytes)", () => {
  it("has a fixture for every contract and no stray fixture", () => {
    const files = readdirSync(dir).filter(f => f.endsWith(".hex")).map(f => f.slice(0, -4)).sort();
    expect(files).toEqual(Object.keys(V8_CONTRACTS).sort());
  });

  it("re-encodes every fixture byte for byte", () => {
    for (const name of Object.keys(V8_CONTRACTS) as V8ContractName[]) {
      const bytes = golden(name);
      expect(Buffer.from(encodeV8(name, decodeV8(name, bytes))).toString("hex"), name).toBe(Buffer.from(bytes).toString("hex"));
    }
  });

  it("keeps the trade row's exact values and its absent ones absent", () => {
    const page = decodeV8("trade_page", golden("trade_page"));
    expect(page.result.case).toBe("trades");
    if (page.result.case !== "trades") return;
    const [closed, open] = page.result.value.rows;
    if (!closed || !open) throw new Error("trade_page: two rows expected");
    expect(closed.status).toBe(TradeStatus.CLOSED);
    expect(closed.pnlWei).toBe(`-${U256_MAX}`);
    expect(closed.roiPpm).toBe(I64_MIN);
    expect(closed.leadBeforeBlockUs).toBe(I64_MIN);
    expect(closed.detectToSendNs).toBe(U64_MAX);
    expect(closed.sendToAckNs).toBeUndefined();
    expect(closed.holdMs).toBe(0n);
    expect(closed.proceedsUnknown).toBe(true);
    expect(closed.placement).toBe(TradePlacement.FIRST);
    expect(closed.exitReason).toBe(ExitReason.TARGET);
    expect(closed.buyTime?.source).toBe(TimeSource.BLOCK_HEADER);
    expect(closed.buyTime?.blockNumber).toBe(U64_MAX);
    expect(open.pnlWei).toBeUndefined();
    expect(open.roiPpm).toBeUndefined();
    expect(open.sellTime?.unixMs).toBeUndefined();
    expect(open.quality).toBe(DataQuality.UNRESOLVED);
    expect(open.qualityReasons).toEqual(["GAS_UPPER_BOUND", "CLOCK_SUSPECT"]);
    expect(page.result.value.counts?.writtenOff).toBe(U64_MAX);
    // The row names the trigger (LiveAttempt.trigger), fees absent with their reason, an adopted lot its evidence.
    expect(closed.triggerSource).toBe("BSC_INGRESS_GETH");
    expect(closed.feesWei).toBeUndefined();
    expect(closed.feesReason).toBe("FEES_NOT_ATTRIBUTED");
    expect([open.adopted, open.evidenceSha256]).toEqual([true, "ee".repeat(32)]);
    expect(page.result.value.realizedNetPnlWei).toBe(`-${U256_MAX}`);
    expect(page.result.value.burnGasWei).toBeUndefined();
  });

  it("embeds the ledger's own transaction and attempt records in the trade detail", () => {
    const d = decodeV8("trade_detail", golden("trade_detail"));
    const tx = d.transactions[0];
    expect(tx?.tx).toMatchObject({ intent: "BUY", state: "INCLUDED", nonce: U64_MAX, blockNumber: 0n, source: "RECEIPT" });
    expect(tx?.orphaned).toBe(true);
    expect(d.attempt).toMatchObject({ kind: "STRATEGY_BUY", clockId: U64_MAX, tSendNs: U64_MAX, tokens: U256_MAX, t0Exact: false });
    expect(d.attempt?.tAckNs).toBeUndefined();
    expect(d.exitTerms).toMatchObject({ ratioPlaces: 18, targetNetMultiple: "1.50", stopNetMultiple: "0.80", holdMs: U64_MAX });
    expect(d.settingsGeneration).toBe(U64_MAX);
  });

  it("keeps the 15 filters with their bounds and the read-only first-buyer window", () => {
    const s = decodeV8("strategy_settings_state", golden("strategy_settings_state"));
    expect(s.stored?.filters.map(f => f.feature)).toHaveLength(15);
    expect(s.stored?.filters.find(f => f.feature === "net_buy_flow_bnb")?.min).toBeUndefined();
    expect(s.stored?.filters.find(f => f.feature === "buy_count")).toMatchObject({ enabled: true, min: "3" });
    expect(s.stored?.filters.find(f => f.feature === "curve_progress_pct")?.enabled).toBe(false);
    expect(s.stored?.inputWei).toBe(U256_MAX);
    expect(s.stored?.observationMs).toBe(0n);
    expect(s.stored?.firstBuyerWindow).toBe(100);
    expect(s.stored?.fillDelayBlocks).toBeUndefined();
    expect(s.running).toBeUndefined();
    expect(s.storedRevision).toBe(U64_MAX);
    // The effective values (read-only ones included) and the native parser bounds.
    expect(s.effective?.fillDelayBlocks).toBe(0);
    expect(s.settingSpecs.map(x => [x.key, x.min, x.max])).toEqual([["BSC_STRATEGY_C_INPUT_WEI", "1", U256_MAX], ["BSC_STRATEGY_C_FIRST_BUYER_WINDOW", "1", undefined]]);
    // A command carries the settable values only (no ratio places, fill delay or gas price).
    const all = decodeV8("strategy_set_all_command", golden("strategy_set_all_command"));
    expect(all.values?.firstBuyerWindow).toBe(100);
    expect(Object.keys(all.values ?? {})).not.toContain("ratioPlaces");
    expect(decodeV8("live_strategy_select_command", golden("live_strategy_select_command")).expectedSettingsGeneration).toBe(U64_MAX);
  });

  it("sends a module revision whole and clones any kept revision under a new name (BSC-V8-01)", () => {
    const created = decodeV8("training_profile_create_command", golden("training_profile_create_command"));
    expect([created.name, created.venues, created.values?.filters.length, created.values?.firstBuyerWindow]).toEqual(["Flap fast", ["BSC_VENUE_FLAP", "BSC_VENUE_OPENFOUR"], 15, 100]);
    expect(created.values?.inputWei).toBeDefined();
    const updated = decodeV8("training_profile_update_command", golden("training_profile_update_command"));
    expect([updated.profileId, updated.name, updated.venues]).toEqual(["p000001", "Flap fast", ["BSC_VENUE_FLAP"]]);
    const cloned = decodeV8("training_profile_clone_command", golden("training_profile_clone_command"));
    expect([cloned.sourceProfileId, cloned.sourceRevision, cloned.name]).toEqual(["p000001", U64_MAX, "Flap fast (copy)"]);
    const deleted = decodeV8("training_profile_delete_command", golden("training_profile_delete_command"));
    expect([deleted.profileId, deleted.archive]).toEqual(["p000001", true]);
  });

  it("keeps an unknown transfer unknown and an absent fee absent", () => {
    const t = decodeV8("treasury_state", golden("treasury_state"));
    expect(t.profitPolicy?.lastTransfer?.status).toBe(TransferStatus.UNKNOWN);
    expect(t.profitPolicy?.lastTransfer?.receipt).toBeUndefined();
    expect(t.profitPolicy?.lastTransfer?.feeEstimateWei).toBe("21000000000000");
    expect(t.profitPolicy?.lastTransfer?.triggerId).toBe("settle-7");
    expect(t.payoutPolicy?.lastPayout?.status).toBe(TransferStatus.CONFIRMED);
    expect(t.payoutPolicy?.lastPayout?.receipt).toMatchObject({ feeWei: "0", blockNumber: U64_MAX, gasUsed: 21000n });
    expect(t.payoutPolicy?.lastPayout?.maxBlock).toBe(U64_MAX);
    expect(t.accounts[0]?.balance?.reservedWei).toBeUndefined();
    expect(t.accounts[0]?.balance?.availableWei).toBe("0");
    expect(t.provenance?.quality).toBe(DataQuality.STALE);
  });

  it("separates the accounts and the two independent policies", () => {
    const t = decodeV8("treasury_state", golden("treasury_state"));
    expect(t.accounts.map(a => a.role)).toEqual([WalletRole.MAIN, WalletRole.BUYER, WalletRole.LAUNCHER]);
    const buyer = t.accounts[1];
    expect(buyer?.realizedNetPnlWei).toBe(`-${U256_MAX}`);
    expect(buyer?.untransferredProfitWei).toBe("0");
    expect(buyer?.profitReceivedWei).toBeUndefined();
    expect(t.accounts[0]?.otherCapitalWei).toBe("0");
    expect(t.accounts[2]?.balance).toBeUndefined();
    expect(t.profitPolicy?.amount?.amount).toEqual({ case: "shareBps", value: 10000 });
    expect(t.profitPolicy?.toThresholdWei).toBeUndefined();
    expect(t.payoutPolicy?.amount?.amount).toEqual({ case: "fixedWei", value: "0" });
    expect(t.payoutPolicy?.unit).toBe(PolicyUnit.BNB);
    expect(t.payoutPolicy?.thresholdEur).toBeUndefined();
    expect(t.eurQuote).toMatchObject({ currency: "EUR", fiatPerBnb: "612.40", quality: DataQuality.STALE, maxAgeMs: 60000n });
    expect(t.eurQuote?.observed?.blockNumber).toBeUndefined();
    expect(t.transferFeeEstimateWei).toBe("21000000000000");
    expect(t.treasuryVersion).toBe(U64_MAX);
    // The accounts' switches, the authorizations and the owner's bounds.
    expect(t.accounts.map(a => [a.control?.id, a.control?.effective, a.reason])).toEqual([["BSC_WALLET_MAIN", "ENABLED", ""], ["BSC_WALLET_BUYER", "BLOCKED", ""], [undefined, undefined, "NOT_CONFIGURED"]]);
    expect(t.profitPolicy?.authorization).toMatchObject({ expiresUnixMs: U64_MAX, usedTotalWei: "0", blockReason: "" });
    expect(t.profitPolicy?.netSinceLastSettlementWei).toBe(`-${U256_MAX}`);
    expect(t.payoutPolicy?.authorization?.blockReason).toBe("RESET_BY_RESTART");
    expect(t.authorizationBounds).toMatchObject({ maxPerTransferWei: "1000000000000000000", maxValidityMs: 604800000n });
    expect(t.destinations.map(d => [d.destinationId, d.confirmed, d.inUse])).toEqual([["d1", true, false], ["d2", false, false]]);
    const eur = decodeV8("payout_policy_set_command", golden("payout_policy_set_command"));
    expect(eur.unit).toBe(PolicyUnit.EUR);
    expect(eur.thresholdEur).toBe("1000.5");
    expect(eur.amount?.amount).toEqual({ case: "fixedEur", value: "0.01" });
    expect(eur.authorization?.maxTotalWei).toBe("5000000000000000000");
    expect(t.decidedRequestIds).toEqual(["pol-b-1", "auth-1"]);
    expect(decodeV8("profit_policy_set_command", golden("profit_policy_set_command")).authorization).toMatchObject({ maxPerTransferWei: "1", maxTotalWei: U256_MAX, scopeSha256: expect.stringMatching(/^[0-9a-f]{64}$/) });
    // A manual transfer: a prepare per path, then an authorization of the DRAFT's digest.
    const fund = decodeV8("transfer_prepare_fund_command", golden("transfer_prepare_fund_command"));
    expect(fund.transfer.case).toBe("fundTrading");
    expect(fund.transfer.value).toMatchObject({ amountWei: U256_MAX, buyerAddress: `0x${"61".repeat(20)}` });
    expect(decodeV8("transfer_prepare_payout_command", golden("transfer_prepare_payout_command")).transfer.case).toBe("payout");
    expect(decodeV8("transfer_authorize_command", golden("transfer_authorize_command")).scopeSha256).toBe(decodeV8("transfer_draft", golden("transfer_draft")).scopeSha256);
    expect(decodeV8("destination_confirm_command", golden("destination_confirm_command"))).toMatchObject({ destinationId: "d2", address: `0x${"93".repeat(20)}` });
  });

  it("keeps the run plans and an unresolved net result absent", () => {
    const p = decodeV8("training_run_page", golden("training_run_page"));
    expect(p.runs.map(r => r.plan?.stopKind)).toEqual([TrainingRunStopKind.UNTIL_STOPPED, TrainingRunStopKind.DURATION]);
    expect(p.runs[0]?.result?.netWei).toBeUndefined();
    expect(p.runs[1]?.result?.netWei).toBe("-1");
    expect(p.runs[1]?.plan?.maxLaunches).toBe(0n);
    expect(p.runs[1]?.status).toBe(TrainingRunStatus.INTERRUPTED);
    expect([p.runs[1]?.replaySource, p.runs[1]?.result?.replayVerdict, p.runs[1]?.result?.configMatch]).toEqual(["rec-run-1", ReplayVerdict.DIFFERS, false]);
    expect(p.runs[0]?.result?.configMatch).toBeUndefined();
    expect(p.runs[0]?.result?.cancelled).toBe(U64_MAX);
    const r = decodeV8("replay_source_page", golden("replay_source_page"));
    expect(r.sources.map(x => [x.sourceId, x.kind, x.records])).toEqual([["rec-run-1", "RECORDED_RUN", U64_MAX]]);
  });

  it("reads NOT_IMPLEMENTED as such", () => {
    expect(decodeV8("query_not_implemented", golden("query_not_implemented")).availability).toBe(Availability.NOT_IMPLEMENTED);
    expect(decodeV8("watch_not_implemented", golden("watch_not_implemented")).availability).toBe(Availability.NOT_IMPLEMENTED);
  });
});

describe("bounded BSC query transport (no route yet)", () => {
  const pb = "application/x-protobuf";
  it("treats the missing route as NOT_IMPLEMENTED and never as data", () => {
    expect(classify(405, "text/plain", null)).toMatchObject({ kind: "NOT_IMPLEMENTED" });
    expect(classify(404, "text/plain", null)).toMatchObject({ kind: "NOT_IMPLEMENTED" });
    expect(classify(503, null, null)).toMatchObject({ kind: "UNAVAILABLE", reason: "HTTP 503" });
    // The SPA fallback's HTML is never decoded as an answer.
    expect(classify(200, "text/html", new TextEncoder().encode("<!doctype html>"))).toMatchObject({ kind: "UNAVAILABLE", reason: "QUERY_ANSWER_NOT_PROTOBUF" });
  });
  it("passes the owner's NOT_IMPLEMENTED and its result through", () => {
    expect(classify(200, pb, golden("query_not_implemented"))).toEqual({ kind: "NOT_IMPLEMENTED", reason: "BSC-V8-10" });
    // A refusal is never a page: its reason, no result.
    expect(decodeV8("query_refused", golden("query_refused")).refusal).toBe(QueryRefusal.CURSOR_STALE);
    expect(classify(200, pb, golden("query_refused"))).toEqual({ kind: "UNAVAILABLE", reason: "QUERY_REFUSED CURSOR_STALE (SNAPSHOT_COMPACTED)" });
    const answer = classify(200, pb, golden("trade_page"));
    expect(answer.kind).toBe("ANSWER");
    if (answer.kind === "ANSWER") expect(answer.result.case).toBe("trades");
  });
});

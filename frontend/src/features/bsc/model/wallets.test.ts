import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { DataQuality, PolicyUnit, TransferStatus, WalletMovementKind, decodeV8, v8Params } from "../api/bscV8Adapter";
import {
  accountViews, bpsToPercent, economics, eurQualified, externalAddress, lastOf, openOf, payoutCommand, payoutDisable, payoutForm, percentToBps,
  positiveWei, profitCommand, profitDisable, profitForm, remaining, seriesScopeId, transferRows, treasuryOf, versionText,
} from "./wallets";
import type { BscWalletsView } from "./types";
import { bnb } from "../pages/V8";

// FE-V8-13…16: the Wallets model against the Rust golden treasury state (prost bytes), the
// operator's input checks and the transfer states. No owner is simulated: an absent owner is a
// NOT_IMPLEMENTED query, and every value comes from the golden bytes or the native projection.
const dir = new URL("../../../../../protocol/golden/bsc/v8/", import.meta.url);
const golden = (name: string) => Uint8Array.from(Buffer.from(readFileSync(new URL(`${name}.hex`, dir), "utf8").trim(), "hex"));
const U256_MAX = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
const MAIN = "0x4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d";
const EXTERNAL = "0x8282828282828282828282828282828282828282";
const BUYER = "0x6161616161616161616161616161616161616161";
const treasury = () => decodeV8("treasury_state", golden("treasury_state"));
const answer = () => treasuryOf({ kind: "ANSWER", requestId: "r", result: { case: "treasury", value: treasury() } });
const projection: BscWalletsView = { expectedAddress: BUYER, walletStatus: "READY", launcherAddress: "0x1111111111111111111111111111111111111111", buyerAddress: BUYER, chainId: 56n,
  launcherBalanceWei: "5", buyerBalanceWei: "7", observedUnixS: 1n, source: "READINESS_SNAPSHOT", liveBalanceWei: "9", liveNextNonce: 3n };

describe("V8 Wallets: accounts from their native owners", () => {
  it("disables everything owner-bound while the treasury owner does not answer", () => {
    const t = treasuryOf({ kind: "NOT_IMPLEMENTED", reason: "HTTP 405" });
    expect(t).toEqual({ ok: false, reason: expect.stringContaining("Not implemented yet") });
    const a = accountViews(projection, t, null);
    expect(a.main.state).toBe("NOT_CONFIGURED");
    expect(a.main.balanceWei).toBeUndefined();
    expect(a.external.address).toBe("");
    // Trading account and launcher stay on the existing projection: LIVE probe first, never summed.
    expect(a.buyer.balanceWei).toBe("9");
    expect(a.buyer.source).toContain("LIVE probe");
    expect(a.buyer.nonce).toBe("3");
    expect(a.launcher.balanceWei).toBe("5");
    expect(economics(t, null).realizedSource).toBe("Not reported");
  });
  it("refuses an answer that is not AVAILABLE or not the treasury", () => {
    const s = decodeV8("treasury_state", v8Params("treasury_state", { availability: 2, unavailableReason: "MAIN_KEY_MISSING" }));
    expect(treasuryOf({ kind: "ANSWER", requestId: "r", result: { case: "treasury", value: s } })).toEqual({ ok: false, reason: "UNAVAILABLE: MAIN_KEY_MISSING" });
    expect(treasuryOf({ kind: "ANSWER", requestId: "r", result: { case: undefined, value: undefined } }).ok).toBe(false);
    expect(treasuryOf({ kind: "LOADING" }).ok).toBe(false);
  });
  it("takes Main, Trading and the launcher from the treasury owner once it answers", () => {
    const t = answer();
    const a = accountViews(projection, t, null);
    expect(a.main.address).toBe(MAIN);
    expect(a.main.balanceWei).toBe(U256_MAX);
    expect(a.main.availableWei).toBe("0");
    expect(a.main.reservedWei).toBeUndefined();
    expect(a.main.quality).toBe("STALE");
    expect(a.buyer.balanceWei).toBe("1");
    expect(a.buyer.state).toBe("LOCKED");
    expect(a.buyer.reason).toBe("ECONOMIC_LIVE_DISABLED");
    expect(a.launcher.state).toBe("NOT_CONFIGURED");
    expect(a.external.address).toBe(EXTERNAL);
    expect(a.external.state).toBe("READY");
    const e = economics(t, null);
    expect(e.realizedNetPnlWei).toBe(`-${U256_MAX}`);
    expect(e.realizedSource).toContain("Treasury owner");
    expect(e.profitReceivedWei).toBe(U256_MAX);
    expect(e.otherCapitalWei).toBe("0");
    expect(seriesScopeId("main")).toBe("WALLET_ROLE_MAIN");
  });
});

describe("V8 Wallets: transfers once, open ones block the next", () => {
  it("lists every request once with its newest state, newest first", () => {
    const rows = transferRows(treasury());
    expect(rows.map(r => r.t.requestId)).toEqual(["auto-a-1", "auto-b-1"]);
    expect(rows.map(r => r.label)).toEqual(["Unknown · the chain decides", "Confirmed"]);
    expect(rows.every(r => !r.duplicateTrigger)).toBe(true);
    expect(lastOf(rows, [WalletMovementKind.PAYOUT_AUTO])?.t.blockNumber).toBe(18446744073709551615n);
    // UNKNOWN is never sent twice: the next profit transfer waits for the chain.
    expect(openOf(rows, [WalletMovementKind.PROFIT_TRANSFER])?.t.requestId).toBe("auto-a-1");
    expect(openOf(rows, [WalletMovementKind.PAYOUT_AUTO])).toBeUndefined();
  });
  it("labels every lifecycle state and flags two requests on one trigger", () => {
    const s = treasury();
    const base = s.transfers[0];
    if (!base) throw new Error("golden has a transfer");
    const states = [TransferStatus.PENDING, TransferStatus.SENT, TransferStatus.CONFIRMED, TransferStatus.FAILED, TransferStatus.UNKNOWN];
    s.transfers = states.map((status, i) => ({ ...base, requestId: `r${i}`, commandId: `c${i}`, status, updatedUnixMs: BigInt(10 - i), triggerId: i < 2 ? "same" : `t${i}` }));
    s.profitPolicy = undefined; s.payoutPolicy = undefined;
    const rows = transferRows(s);
    expect(rows.map(r => r.label)).toEqual(["Pending", "Sent", "Confirmed", "Failed", "Unknown · the chain decides"]);
    expect(rows.map(r => r.duplicateTrigger)).toEqual([true, true, false, false, false]);
    expect(rows.map(r => r.tone)).toEqual(["warn", "blue", "good", "bad", "warn"]);
    // Open is everything not decided: an unspecified or unknown future state blocks too.
    s.transfers = [{ ...base, commandId: "c9", status: TransferStatus.UNSPECIFIED }];
    expect(openOf(transferRows(s), [base.kind])).toBeDefined();
    s.transfers = [{ ...base, commandId: "c9", status: 99 as TransferStatus }];
    expect(openOf(transferRows(s), [base.kind])).toBeDefined();
    s.transfers = [{ ...base, commandId: "c9", status: TransferStatus.FAILED }];
    expect(openOf(transferRows(s), [base.kind])).toBeUndefined();
    // One intent is one transfer: a newer state of the same intent replaces the older one.
    s.transfers = [{ ...base, commandId: "c1", requestId: "x", status: TransferStatus.PENDING, updatedUnixMs: 1n }, { ...base, commandId: "c1", requestId: "y", status: TransferStatus.CONFIRMED, updatedUnixMs: 2n }];
    expect(transferRows(s).map(r => r.label)).toEqual(["Confirmed"]);
  });
});

describe("V8 Wallets: operator input, exact and checked", () => {
  it("shows BNB exactly, all 18 places included", () => {
    expect(bnb("1", 18)).toBe("0.000000000000000001");
    expect(bnb(U256_MAX, 18)).toBe("115792089237316195423570985008687907853269984665640564039457.584007913129639935");
    expect(bnb("-1500000000000000000", 2)).toBe("-1.50");
    expect(bnb("1", 0)).toBe("0");
  });
  it("accepts exact BNB amounts only", () => {
    expect(positiveWei(" ", "Amount")).toEqual({ ok: false, error: "Amount: enter a BNB amount" });
    expect(positiveWei("0.000000000000000001", "Amount")).toEqual({ ok: true, value: "1" });
    expect(positiveWei("0", "Amount")).toEqual({ ok: false, error: "Amount must be above 0" });
    expect(positiveWei("1e18", "Amount").ok).toBe(false);
    expect(positiveWei("-1", "Amount").ok).toBe(false);
    expect(positiveWei(`${"9".repeat(60)}`, "Amount")).toEqual({ ok: false, error: "Amount is beyond the chain's uint256 range" });
    expect(remaining("100", "60", "40")).toEqual({ ok: true, value: "0" });
    expect(remaining("100", "61", "40")).toEqual({ ok: false, error: "Amount and fee exceed the free balance" });
    expect(remaining(undefined, "1", "1").ok).toBe(false);
    expect(remaining("100", "1", undefined).ok).toBe(false);
  });
  it("checks an external address against format, zero and DarkBot's own accounts", () => {
    expect(externalAddress(EXTERNAL, [MAIN, BUYER])).toEqual({ ok: true, value: EXTERNAL });
    expect(externalAddress("0x82", []).ok).toBe(false);
    expect(externalAddress(`0x${"0".repeat(40)}`, []).ok).toBe(false);
    expect(externalAddress(MAIN.toUpperCase().replace("0X", "0x"), [MAIN])).toEqual({ ok: false, error: "This is one of DarkBot's own accounts, not an external wallet" });
  });
  it("converts shares to basis points and back", () => {
    expect(percentToBps("100")).toEqual({ ok: true, value: 10000 });
    expect(percentToBps("0.01")).toEqual({ ok: true, value: 1 });
    expect(percentToBps("12,5")).toEqual({ ok: true, value: 1250 });
    expect(percentToBps("0").ok).toBe(false);
    expect(percentToBps("100.01").ok).toBe(false);
    expect(percentToBps("1.234").ok).toBe(false);
    expect(bpsToPercent(1250)).toBe("12.5");
    expect(bpsToPercent(1)).toBe("0.01");
    expect(bpsToPercent(10000)).toBe("100");
  });
});

describe("V8 Wallets: two independent policies", () => {
  it("round-trips policy A from the stored version into its exact set command", () => {
    const p = treasury().profitPolicy;
    const f = profitForm(p);
    expect(f).toEqual({ enabled: true, threshold: "0.1", mode: "share", amount: "100", minCapital: weiToBnbMax(), reserve: "0" });
    const cmd = profitCommand(f);
    if (!cmd.ok) throw new Error(cmd.error);
    expect(cmd.value).toEqual({ enabled: true, thresholdRealizedNetProfitWei: "100000000000000000", amount: { amount: { case: "shareBps", value: 10000 } }, buyerMinCapitalWei: U256_MAX, buyerReserveWei: "0" });
    expect(decodeV8("profit_policy_set_command", v8Params("profit_policy_set_command", cmd.value)).thresholdRealizedNetProfitWei).toBe("100000000000000000");
    expect(profitCommand({ ...f, mode: "fixed", amount: "0" })).toEqual({ ok: false, error: "Transfer amount must be above 0" });
    expect(versionText(p)).toBe("v18446744073709551615 · effective");
  });
  it("refuses EUR without a qualified fresh native quote and an unconfirmed recipient", () => {
    const s = treasury();
    const p = s.payoutPolicy;
    const f = payoutForm(p);
    expect(f).toEqual({ enabled: false, destinationId: "d1", unit: "BNB", threshold: weiToBnbMax(), mode: "fixed", amount: "0", retain: "0.25" });
    expect(payoutCommand(f, s.destinations, false)).toEqual({ ok: false, error: "Payout amount must be above 0" });
    expect(versionText(p)).toBe("Stored v1 · effective v0");
    const ok = payoutCommand({ ...f, amount: "0.5" }, s.destinations, false);
    if (!ok.ok) throw new Error(ok.error);
    expect(ok.value.amount).toEqual({ amount: { case: "fixedWei", value: "500000000000000000" } });
    expect(ok.value.unit).toBe(PolicyUnit.BNB);
    expect(payoutCommand({ ...f, unit: "EUR", amount: "10", threshold: "1000" }, s.destinations, false)).toEqual({ ok: false, error: "EUR needs a qualified, fresh native BNB/EUR price; use BNB" });
    const eur = payoutCommand({ ...f, unit: "EUR", amount: "10.5", threshold: "1000" }, s.destinations, true);
    if (!eur.ok) throw new Error(eur.error);
    expect(eur.value).toMatchObject({ thresholdEur: "1000", thresholdAvailableMainWei: "", amount: { amount: { case: "fixedEur", value: "10.5" } } });
    expect(payoutCommand({ ...f, unit: "EUR", amount: "0", threshold: "1000" }, s.destinations, true)).toEqual({ ok: false, error: "Payout amount must be above 0" });
    expect(payoutCommand({ ...f, amount: "1" }, s.destinations, false, [EXTERNAL])).toEqual({ ok: false, error: "This is one of DarkBot's own accounts, not an external wallet" });
    expect(ok.value.destinationAddress).toBe(EXTERNAL);
    const unconfirmed = s.destinations.map(d => ({ ...d, confirmed: false }));
    expect(payoutCommand({ ...f, amount: "1" }, unconfirmed, false)).toEqual({ ok: false, error: "The external address is not confirmed" });
    expect(payoutCommand({ ...f, destinationId: "", amount: "1" }, s.destinations, false)).toEqual({ ok: false, error: "Choose a saved external address" });
  });
  it("qualifies a BNB/EUR quote only as the owner verified it, never by the browser clock", () => {
    const s = treasury();
    const q = s.eurQuote;
    if (!q) throw new Error("golden has a quote");
    expect(q.quality).toBe(DataQuality.STALE);
    expect(eurQualified(s)).toBe(false);
    q.quality = DataQuality.VERIFIED;
    expect(eurQualified(s)).toBe(true);
    q.fiatPerBnb = "0";
    expect(eurQualified(s)).toBe(false);
    q.fiatPerBnb = "612.40"; q.currency = "USD";
    expect(eurQualified(s)).toBe(false);
    expect(eurQualified(undefined)).toBe(false);
  });
  it("disables a stored policy with its stored values, nothing re-validated", () => {
    const s = treasury();
    const a = s.profitPolicy, b = s.payoutPolicy;
    if (!a || !b) throw new Error("golden has both policies");
    const da = profitDisable(a).value;
    expect(da).toMatchObject({ enabled: false, thresholdRealizedNetProfitWei: "100000000000000000", buyerMinCapitalWei: U256_MAX });
    expect(decodeV8("profit_policy_set_command", v8Params("profit_policy_set_command", da)).amount?.amount).toEqual({ case: "shareBps", value: 10000 });
    // Policy B's stored fixed 0 would not validate; a disable sends it unchanged.
    const db = payoutDisable(b).value;
    expect(decodeV8("payout_policy_set_command", v8Params("payout_policy_set_command", db))).toMatchObject({ enabled: false, destinationAddress: EXTERNAL, retainWei: "250000000000000000" });
  });
});

function weiToBnbMax() { return "115792089237316195423570985008687907853269984665640564039457.584007913129639935"; }

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { DataQuality, PolicyUnit, TransferStatus, WalletMovementKind, decodeV8, v8Params } from "../api/bscV8Adapter";
import {
  accountViews, authorizationOf, bpsToPercent, chainProblem, draftFrom, draftProblem, economics, eurQualified, eurText, externalAddress, lastOf, openOf, payoutCommand, payoutDisable,
  payoutForm, payoutKey, payoutScopeText, percentToBps, positiveWei, profitCommand, profitDisable, profitForm, profitKey, profitScopeText, remaining, seriesScopeId, transferRows,
  treasuryOf, usable, versionText,
} from "./wallets";
import { WalletRole } from "../api/bscV8Adapter";
import { scopeSha256 } from "./live";
import { createHash } from "node:crypto";
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
/** Policy B's command on the golden Main chain (56). */
const payout = (f: Parameters<typeof payoutCommand>[0], d: Parameters<typeof payoutCommand>[1], eur: boolean, own: readonly string[] = []) => payoutCommand(f, d, eur, own, 56n);
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
    // The account state is its native switch: unknown without it, never a guessed READY.
    expect(a.buyer.state).toBe("UNKNOWN");
    const switched = accountViews({ ...projection, buyerControl: { id: "BSC_WALLET_BUYER", requested: "ENABLED", effective: "BLOCKED", blockReason: "RESET_BY_RESTART", generation: 1n, requestId: "" } }, t, null);
    expect([switched.buyer.state, switched.buyer.reason]).toEqual(["BLOCKED", "RESET_BY_RESTART"]);
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
    // Each account's state is its native wallet switch (BSC_WALLET_MAIN, BSC_WALLET_BUYER).
    expect([a.main.state, a.main.reason, usable(a.main)]).toEqual(["ENABLED", "", true]);
    expect([a.buyer.state, a.buyer.reason, usable(a.buyer)]).toEqual(["BLOCKED", "RESET_BY_RESTART", false]);
    expect([a.launcher.state, a.launcher.reason]).toEqual(["NOT_CONFIGURED", "NOT_CONFIGURED"]);
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
    expect(lastOf(rows, [WalletMovementKind.PAYOUT_AUTO])?.t.receipt?.blockNumber).toBe(18446744073709551615n);
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
    // A DRAFT awaits its authorization: open (no second prepare); an EXPIRED one is decided, never sent.
    s.transfers = [{ ...base, commandId: "c9", status: TransferStatus.DRAFT }];
    expect(openOf(transferRows(s), [base.kind])?.label).toBe("Prepared · awaits authorization");
    s.transfers = [{ ...base, commandId: "c9", status: TransferStatus.EXPIRED }];
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
    expect(f).toEqual({ enabled: true, threshold: "0.1", mode: "share", amount: "100", minCapital: weiToBnbMax(), reserve: "0", validHours: "", maxPerTransfer: weiToBnbMax(), maxTotal: weiToBnbMax() });
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
    expect(f).toEqual({ enabled: false, destinationId: "d1", unit: "BNB", threshold: weiToBnbMax(), mode: "fixed", amount: "0", retain: "0.25", validHours: "", maxPerTransfer: weiToBnbMax(), maxTotal: weiToBnbMax() });
    expect(payout(f, s.destinations, false)).toEqual({ ok: false, error: "Payout amount must be above 0" });
    expect(versionText(p)).toBe("Stored v1 · effective v0");
    const ok = payout({ ...f, amount: "0.5" }, s.destinations, false);
    if (!ok.ok) throw new Error(ok.error);
    expect(ok.value.amount).toEqual({ amount: { case: "fixedWei", value: "500000000000000000" } });
    expect(ok.value.unit).toBe(PolicyUnit.BNB);
    expect(payout({ ...f, unit: "EUR", amount: "10", threshold: "1000" }, s.destinations, false)).toEqual({ ok: false, error: "EUR needs a qualified, fresh native BNB/EUR price; use BNB" });
    const eur = payout({ ...f, unit: "EUR", amount: "10.5", threshold: "1000" }, s.destinations, true);
    if (!eur.ok) throw new Error(eur.error);
    expect(eur.value).toMatchObject({ thresholdEur: "1000", thresholdAvailableMainWei: "", amount: { amount: { case: "fixedEur", value: "10.5" } } });
    expect(payout({ ...f, unit: "EUR", amount: "0", threshold: "1000" }, s.destinations, true)).toEqual({ ok: false, error: "Payout amount must be above 0" });
    expect(payout({ ...f, amount: "1" }, s.destinations, false, [EXTERNAL])).toEqual({ ok: false, error: "This is one of DarkBot's own accounts, not an external wallet" });
    expect(ok.value.destinationAddress).toBe(EXTERNAL);
    const unconfirmed = s.destinations.map(d => ({ ...d, confirmed: false }));
    expect(payout({ ...f, amount: "1" }, unconfirmed, false)).toEqual({ ok: false, error: "The external address is not confirmed" });
    expect(payout({ ...f, destinationId: "", amount: "1" }, s.destinations, false)).toEqual({ ok: false, error: "Choose a saved external address" });
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

describe("V8 Wallets: authorizations within the owner's bounds, canonical scope texts", () => {
  const bounds = () => treasury().authorizationBounds;
  it("bounds an enable's authorization by the owner's configuration", () => {
    const f = { validHours: "24", maxPerTransfer: "0.5", maxTotal: "2" };
    expect(authorizationOf(f, bounds())).toEqual({ ok: true, value: { validityMs: 86_400_000n, maxPerTransferWei: "500000000000000000", maxTotalWei: "2000000000000000000" } });
    expect(authorizationOf(f, undefined)).toEqual({ ok: false, error: "The owner reports no authorization bounds" });
    expect(authorizationOf({ ...f, validHours: "0" }, bounds()).ok).toBe(false);
    expect(authorizationOf({ ...f, validHours: "1.5" }, bounds()).ok).toBe(false);
    expect(authorizationOf({ ...f, validHours: "169" }, bounds())).toEqual({ ok: false, error: "Valid for at most 168 h (the owner's bound)" });
    expect(authorizationOf({ ...f, maxPerTransfer: "1.000000000000000001" }, bounds())).toEqual({ ok: false, error: "Most per transfer is above the owner's bound of 1 BNB" });
    expect(authorizationOf({ ...f, maxTotal: "5.1" }, bounds())).toEqual({ ok: false, error: "Most in total is above the owner's bound of 5 BNB" });
    expect(authorizationOf({ ...f, maxTotal: "0.4" }, bounds())).toEqual({ ok: false, error: "Most in total is below most per transfer" });
  });
  it("writes policy A's and B's canonical scope text byte for byte, its digest the SHA-256 of it", async () => {
    const s = treasury();
    const a = profitCommand(profitForm(s.profitPolicy));
    const auth = authorizationOf({ validHours: "24", maxPerTransfer: "0.5", maxTotal: "2" }, s.authorizationBounds);
    if (!a.ok || !auth.ok) throw new Error("golden policy A and bounds validate");
    const text = profitScopeText(56n, BUYER.toUpperCase().replace("0X", "0x"), MAIN, a.value, auth.value, 1_793_000_000_000n);
    expect(text).toBe("darkbot.bsc.wallet-profit-policy.v1\nchain_id=56\n"
      + `from=${BUYER}\nto=${MAIN}\nenabled=true\nthreshold_realized_net_profit_wei=100000000000000000\namount=share_bps:10000\n`
      + `buyer_min_capital_wei=${U256_MAX}\nbuyer_reserve_wei=0\nmax_per_transfer_wei=500000000000000000\nmax_total_wei=2000000000000000000\nexpires_unix_ms=1793000000000\n`);
    expect(await scopeSha256(text)).toBe(createHash("sha256").update(text, "utf8").digest("hex"));
    const b = payout({ ...payoutForm(s.payoutPolicy), enabled: true, amount: "0.3" }, s.destinations, false);
    if (!b.ok) throw new Error(b.error);
    expect(payoutScopeText(56n, MAIN, b.value, auth.value, 7n)).toBe("darkbot.bsc.wallet-payout-policy.v1\nchain_id=56\n"
      + `from=${MAIN}\nto=${EXTERNAL}\ndestination_id=d1\nenabled=true\nunit=BNB\nthreshold_available_main_wei=${U256_MAX}\n`
      + "amount=fixed_wei:300000000000000000\nretain_wei=250000000000000000\nmax_per_transfer_wei=500000000000000000\nmax_total_wei=2000000000000000000\nexpires_unix_ms=7\n");
    const eur = payout({ ...payoutForm(s.payoutPolicy), enabled: true, unit: "EUR", threshold: "1000", amount: "10.5" }, s.destinations, true);
    if (!eur.ok) throw new Error(eur.error);
    expect(payoutScopeText(56n, MAIN, eur.value, auth.value, 7n)).toContain("unit=EUR\nthreshold_eur=1000\namount=fixed_eur:10.5\n");
  });
});

describe("V8 Wallets: the shared golden vectors and the DRAFT check", () => {
  const text = (name: string) => readFileSync(new URL(`${name}.txt`, dir), "utf8");
  const sha = (t: string) => createHash("sha256").update(t, "utf8").digest("hex");
  it("builds both policies' scope texts from the Rust golden commands byte for byte, their SHA-256 in the command", () => {
    const a = decodeV8("profit_policy_set_command", golden("profit_policy_set_command"));
    const av = a.authorization;
    if (!av || a.amount?.amount.case !== "shareBps") throw new Error("golden policy A carries a share and an authorization");
    const limits = { validityMs: 0n, maxPerTransferWei: av.maxPerTransferWei, maxTotalWei: av.maxTotalWei };
    const cmdA = { enabled: a.enabled, thresholdRealizedNetProfitWei: a.thresholdRealizedNetProfitWei, amount: { amount: { case: "shareBps" as const, value: a.amount.amount.value } }, buyerMinCapitalWei: a.buyerMinCapitalWei, buyerReserveWei: a.buyerReserveWei };
    expect(profitScopeText(56n, BUYER, MAIN, cmdA, limits, av.expiresUnixMs)).toBe(text("scope_profit_policy"));
    expect(sha(text("scope_profit_policy"))).toBe(av.scopeSha256);
    const b = decodeV8("payout_policy_set_command", golden("payout_policy_set_command"));
    const bv = b.authorization;
    if (!bv || b.amount?.amount.case !== "fixedEur") throw new Error("golden policy B carries a fixed EUR amount and an authorization");
    const cmdB = { enabled: b.enabled, destinationId: b.destinationId, destinationAddress: b.destinationAddress, unit: b.unit, thresholdAvailableMainWei: b.thresholdAvailableMainWei, thresholdEur: b.thresholdEur,
      amount: { amount: { case: "fixedEur" as const, value: b.amount.amount.value } }, retainWei: b.retainWei };
    expect(payoutScopeText(56n, MAIN, cmdB, { validityMs: 0n, maxPerTransferWei: bv.maxPerTransferWei, maxTotalWei: bv.maxTotalWei }, bv.expiresUnixMs)).toBe(text("scope_payout_policy"));
    expect(sha(text("scope_payout_policy"))).toBe(bv.scopeSha256);
    // The command carries EUR in the canonical form the text names.
    expect(eurText("01000.50", "x")).toEqual({ ok: true, value: "1000.5" });
    expect(eurText("1000,00", "x")).toEqual({ ok: true, value: "1000" });
    expect(eurText("0.01", "x")).toEqual({ ok: true, value: "0.01" });
    expect(eurText("0.00", "x", true).ok).toBe(false);
  });
  it("authorizes a DRAFT only when its text matches its fields and this path", async () => {
    const d = decodeV8("transfer_draft", golden("transfer_draft"));
    expect(d.scopeText).toBe(text("scope_wallet_transfer"));
    expect(await scopeSha256(d.scopeText)).toBe(d.scopeSha256);
    expect(draftProblem(d, WalletMovementKind.CAPITAL_FUNDING, MAIN, BUYER)).toBeNull();
    expect(draftProblem(d, WalletMovementKind.PAYOUT_MANUAL, MAIN, EXTERNAL)).toBe("The draft is not one of this path");
    expect(draftProblem({ ...d, commandId: "" }, WalletMovementKind.CAPITAL_FUNDING, MAIN, BUYER)).toBe("The owner names no draft id");
    expect(draftProblem({ ...d, scopeText: d.scopeText.replace("wallet-transfer.v1", "wallet-transfer.v2") }, WalletMovementKind.CAPITAL_FUNDING, MAIN, BUYER)).toBe("The owner's scope text is not a manual transfer's");
    // A text that sends elsewhere than its fields and the path say is never authorized.
    expect(draftProblem({ ...d, scopeText: d.scopeText.replace(`to=${BUYER}`, `to=${EXTERNAL}`) }, WalletMovementKind.CAPITAL_FUNDING, MAIN, BUYER)).toBe(`The owner's scope text says to=${EXTERNAL}, the draft ${BUYER}`);
    expect(draftProblem({ ...d, amountWei: "1" }, WalletMovementKind.CAPITAL_FUNDING, MAIN, BUYER)).toContain("amount_wei=500000000000000000");
    expect(draftProblem({ ...d, destinationAddress: EXTERNAL }, WalletMovementKind.CAPITAL_FUNDING, MAIN, BUYER)).toBe("The draft's addresses are not this path's");
    expect(draftProblem(d, WalletMovementKind.CAPITAL_FUNDING, MAIN, undefined)).toBe("The receiver of this path is not known");
    // The DRAFT holds Main's next nonce: it is Main's open draft whatever path asks.
    const s = treasury();
    s.transfers = [d];
    expect(draftFrom(transferRows(s), WalletRole.MAIN)?.t.commandId).toBe("draft-f-1");
    expect(draftFrom(transferRows(s), WalletRole.BUYER)).toBeUndefined();
  });
  it("compares a policy edit by its exact meaning, refuses another chain than Main's", () => {
    const s = treasury();
    const a = profitForm(s.profitPolicy);
    expect(profitKey({ ...a, threshold: "0.10", reserve: "0.0" }, false)).toBe(profitKey(a, false));
    expect(profitKey({ ...a, threshold: "0.2" }, false)).not.toBe(profitKey(a, false));
    expect(profitKey({ ...a, validHours: "24" }, false)).toBe(profitKey(a, false));
    expect(profitKey({ ...a, validHours: "24" }, true)).not.toBe(profitKey(a, true));
    const b = payoutForm(s.payoutPolicy);
    expect(payoutKey({ ...b, retain: "0.250" }, true)).toBe(payoutKey(b, true));
    expect(chainProblem(56n, 56n)).toBeNull();
    expect(chainProblem(97n, 56n)).toBe("The address is saved for chain 97, Main is on chain 56");
    expect(payoutCommand({ ...b, amount: "1" }, s.destinations, false, [], 97n)).toEqual({ ok: false, error: "The address is saved for chain 56, Main is on chain 97" });
  });
});

function weiToBnbMax() { return "115792089237316195423570985008687907853269984665640564039457.584007913129639935"; }

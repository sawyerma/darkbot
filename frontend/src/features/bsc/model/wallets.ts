// V8 Wallets (FE-V8-13…16, Bindings §14.1/§14.4 M5): the four accounts, the four money paths and
// the two independent automatic policies, read from the native owners only. The browser validates
// what the operator typed (exact BNB → wei, a BSC address, a share) and previews what a request
// would leave; the native treasury owner alone decides, signs, sends and settles.
import {
  Availability, ChainTxStatus, DataQuality, PolicyUnit, TransferStatus, WalletMovementKind, WalletRole,
  type MainPayoutPolicy, type ProfitTransferPolicy, type WalletAccount, type WalletAuthorizationBounds, type WalletDestination, type WalletTransferState, type WalletTreasuryState,
} from "../api/bscV8Adapter";
import type { BscLiveView, BscWalletsView, ControlSwitchView } from "./types";
import { bnbToWei, weiToBnb } from "./strategyForm";
import type { BscQueryOutcome } from "@/core/realtime/bscQuery";

type QueryState = { kind: "IDLE" } | { kind: "LOADING" } | BscQueryOutcome;

export type AccountId = "main" | "buyer" | "external" | "launcher";
export const ACCOUNT_NAME: Record<AccountId, string> = { main: "Main account", buyer: "Trading account", external: "External wallet", launcher: "G0.7 test launcher" };
export const ROLE_OF: Record<AccountId, WalletRole> = { main: WalletRole.MAIN, buyer: WalletRole.BUYER, external: WalletRole.EXTERNAL, launcher: WalletRole.LAUNCHER };
const ID_OF = new Map<WalletRole, AccountId>(Object.entries(ROLE_OF).map(([k, v]) => [v, k as AccountId]));
export const accountOf = (role: WalletRole): AccountId | undefined => ID_OF.get(role);
/** The scope id of one wallet's performance series (common.proto: its WalletRole name). */
export const seriesScopeId = (id: AccountId) => `WALLET_ROLE_${WalletRole[ROLE_OF[id]]}`;

const OWNER = "the native BSC treasury owner (BSC-V8-13…17)";
/** The treasury as its native owner answered, or why there is none. */
export type Treasury = { ok: true; state: WalletTreasuryState } | { ok: false; reason: string };
export function treasuryOf(q: QueryState): Treasury {
  if (q.kind === "IDLE") return { ok: false, reason: "Not loaded" };
  if (q.kind === "LOADING") return { ok: false, reason: "Loading…" };
  if (q.kind !== "ANSWER") return { ok: false, reason: q.kind === "NOT_IMPLEMENTED" ? `Not implemented yet: ${OWNER} (${q.reason})` : `Unavailable: ${q.reason}` };
  if (q.result.case !== "treasury") return { ok: false, reason: `The owner answered ${q.result.case ?? "nothing"}, not the treasury` };
  const state = q.result.value;
  if (state.availability !== Availability.AVAILABLE) return { ok: false, reason: `${Availability[state.availability] ?? "UNKNOWN"}: ${state.unavailableReason || OWNER}` };
  return { ok: true, state };
}

export interface AccountView {
  id: AccountId; name: string; address: string; chainId?: bigint;
  balanceWei?: string; reservedWei?: string; availableWei?: string;
  /** Who reported the balance and how it is proven. */
  source: string; quality: string;
  /** The account's wallet switch as the owner holds it (ENABLED, DISABLED, BLOCKED, UNKNOWN), or
   *  NOT_CONFIGURED; an external wallet CONFIRMED or UNCONFIRMED. With the owner's reason. */
  state: string; reason: string;
  nonce?: string; ownerKey: boolean; account?: WalletAccount;
}
const DIGITS = /^[0-9]+$/;
const digits = (v: string | undefined) => v !== undefined && DIGITS.test(v) ? v : undefined;

/** The account's state: its native switch, else NOT_CONFIGURED without an address, else UNKNOWN. */
const switchState = (address: string, control: ControlSwitchView | undefined) => control ? control.effective || "UNKNOWN" : address ? "UNKNOWN" : "NOT_CONFIGURED";
const reasons = (...r: (string | undefined)[]) => r.filter(Boolean).join(" · ");
/** Whether the owner can send from this account now: its switch ENABLED, no owner reason, its key held. */
export const usable = (a: AccountView) => a.state === "ENABLED" && !a.reason && a.ownerKey;

function fromTreasury(id: AccountId, a: WalletAccount): AccountView {
  const b = a.balance;
  return {
    id, name: ACCOUNT_NAME[id], address: a.address, chainId: a.chainId || undefined,
    balanceWei: digits(b?.balanceWei), reservedWei: digits(b?.reservedWei), availableWei: digits(b?.availableWei),
    source: b ? `${b.source || "TREASURY_OWNER"}${b.blockNumber !== undefined ? ` · block ${b.blockNumber}` : ""}` : "TREASURY_OWNER",
    quality: b ? DataQuality[b.quality] ?? "UNKNOWN" : "UNKNOWN",
    state: switchState(a.address, a.control), reason: reasons(a.reason, a.control?.blockReason), nonce: a.nextNonce?.toString(), ownerKey: a.ownerKey, account: a,
  };
}

/** The accounts: the treasury owner's when it answers; until then the Trading account and the
 *  launcher from the existing native wallet projection (`bsc.wallets.state`, the LIVE probe first),
 *  the Main account and the external wallet not configured. Never summed. */
export function accountViews(w: BscWalletsView | null, t: Treasury, live: BscLiveView | null): Record<AccountId, AccountView> {
  const of = (id: AccountId) => t.ok ? t.state.accounts.find(a => a.role === ROLE_OF[id]) : undefined;
  const none = (id: AccountId, reason: string): AccountView => ({ id, name: ACCOUNT_NAME[id], address: "", source: "—", quality: "UNKNOWN", state: "NOT_CONFIGURED", reason, ownerKey: false });
  const main = of("main");
  const buyer = of("buyer");
  const launcher = of("launcher");
  const projection = (id: "buyer" | "launcher"): AccountView => {
    if (!w) return none(id, "No wallet projection");
    const probe = id === "buyer" && w.liveBalanceWei !== undefined;
    const balance = id === "buyer" ? (w.liveBalanceWei ?? w.buyerBalanceWei) : w.launcherBalanceWei;
    const address = id === "buyer" ? w.buyerAddress : w.launcherAddress;
    const control = id === "buyer" ? w.buyerControl : w.launcherControl;
    return {
      id, name: ACCOUNT_NAME[id], address, chainId: w.chainId, balanceWei: digits(balance),
      source: probe ? "LIVE probe · current runtime status" : `${w.source || "UNKNOWN"} snapshot${w.observedBlock !== undefined ? ` · block ${w.observedBlock}` : ""}`,
      quality: probe ? "LIVE" : w.source ? "SNAPSHOT" : "UNKNOWN",
      state: switchState(address, control), reason: address ? control?.blockReason ?? "" : "No address reported",
      nonce: id === "buyer" ? (w.liveNextNonce ?? live?.nextNonce)?.toString() : undefined, ownerKey: !!address,
    };
  };
  const dest = t.ok ? externalDestination(t.state) : undefined;
  return {
    main: main ? fromTreasury("main", main) : none("main", t.ok ? "The treasury owner has no Main account configured" : t.reason),
    buyer: buyer ? fromTreasury("buyer", buyer) : projection("buyer"),
    launcher: launcher ? fromTreasury("launcher", launcher) : projection("launcher"),
    external: dest ? { id: "external", name: ACCOUNT_NAME.external, address: dest.address, chainId: dest.chainId, source: `Saved destination · ${dest.label || dest.destinationId}`,
      quality: dest.confirmed ? "CONFIRMED" : "UNCONFIRMED", state: dest.confirmed ? "READY" : "UNCONFIRMED", reason: dest.confirmed ? "" : "Not confirmed by the operator", ownerKey: false }
      : none("external", t.ok ? "No external address saved" : t.reason),
  };
}

/** The destination policy B pays to, else the first confirmed, else the first saved. */
export function externalDestination(s: WalletTreasuryState): WalletDestination | undefined {
  const byId = s.payoutPolicy?.destinationId ? s.destinations.find(d => d.destinationId === s.payoutPolicy?.destinationId) : undefined;
  return byId ?? s.destinations.find(d => d.confirmed) ?? s.destinations[0];
}

export interface Economics {
  /** Realized net trading PnL (signed wei) and who reports it. */
  realizedNetPnlWei?: string; realizedSource: string; lowerBound: boolean;
  capitalBasisWei?: string; transferredWei?: string; untransferredWei?: string;
  profitReceivedWei?: string; otherCapitalWei?: string;
}
/** The Trading and Main economics, kept apart (Main may hold capital: never shown as profit). */
export function economics(t: Treasury, live: BscLiveView | null): Economics {
  const buyer = t.ok ? t.state.accounts.find(a => a.role === WalletRole.BUYER) : undefined;
  const main = t.ok ? t.state.accounts.find(a => a.role === WalletRole.MAIN) : undefined;
  const owner = buyer?.realizedNetPnlWei;
  return {
    realizedNetPnlWei: owner ?? live?.realizedPnlWei,
    realizedSource: owner !== undefined ? "Treasury owner (closed, chain-confirmed lots)" : live ? "LIVE ledger" : "Not reported",
    lowerBound: owner === undefined && !!live?.positions.some(p => p.proceedsUnknown),
    capitalBasisWei: buyer?.capitalBasisWei, transferredWei: buyer?.transferredProfitWei, untransferredWei: buyer?.untransferredProfitWei,
    profitReceivedWei: main?.profitReceivedWei, otherCapitalWei: main?.otherCapitalWei,
  };
}

// ---------------------------------------------------------------- transfers
export type Tone = "" | "good" | "bad" | "warn" | "blue";
export const TRANSFER_LABEL: Record<TransferStatus, { label: string; tone: Tone }> = {
  [TransferStatus.UNSPECIFIED]: { label: "Not reported", tone: "warn" },
  [TransferStatus.DRAFT]: { label: "Prepared · awaits authorization", tone: "blue" },
  [TransferStatus.PENDING]: { label: "Pending", tone: "warn" },
  [TransferStatus.SENT]: { label: "Sent", tone: "blue" },
  [TransferStatus.CONFIRMED]: { label: "Confirmed", tone: "good" },
  [TransferStatus.FAILED]: { label: "Failed", tone: "bad" },
  [TransferStatus.UNKNOWN]: { label: "Unknown · the chain decides", tone: "warn" },
  [TransferStatus.EXPIRED]: { label: "Expired · never sent", tone: "" },
};
export const transferLabel = (s: TransferStatus) => TRANSFER_LABEL[s] ?? TRANSFER_LABEL[TransferStatus.UNSPECIFIED];
export const CHAIN_LABEL: Record<ChainTxStatus, string> = {
  [ChainTxStatus.UNSPECIFIED]: "Not reported", [ChainTxStatus.PENDING]: "Pending", [ChainTxStatus.INCLUDED]: "Included",
  [ChainTxStatus.FINAL]: "Final", [ChainTxStatus.REVERTED]: "Reverted", [ChainTxStatus.REPLACED]: "Replaced",
  [ChainTxStatus.ORPHANED]: "Orphaned", [ChainTxStatus.UNKNOWN]: "Unknown",
};
export const MOVEMENT_LABEL: Record<WalletMovementKind, string> = {
  [WalletMovementKind.UNSPECIFIED]: "Not reported",
  [WalletMovementKind.DEPOSIT]: "Deposit",
  [WalletMovementKind.CAPITAL_FUNDING]: "Funding · Main → Trading",
  [WalletMovementKind.PROFIT_TRANSFER]: "Profit transfer · Trading → Main",
  [WalletMovementKind.PAYOUT_MANUAL]: "Payout · Main → external",
  [WalletMovementKind.PAYOUT_AUTO]: "Automatic payout · Main → external",
  [WalletMovementKind.TRADE_BUY]: "Trade buy",
  [WalletMovementKind.TRADE_SELL]: "Trade sell",
  [WalletMovementKind.FEE]: "Fee",
  [WalletMovementKind.EXTERNAL_OUT]: "Outgoing · not by DarkBot",
  [WalletMovementKind.UNATTRIBUTED]: "Unattributed balance change",
  [WalletMovementKind.CAPITAL_RETURN]: "Capital return · Trading → Main",
  [WalletMovementKind.LAUNCH]: "G0.7 launch",
  [WalletMovementKind.NONCE_BURN]: "Nonce burn",
};
export const movementLabel = (k: WalletMovementKind) => MOVEMENT_LABEL[k] ?? MOVEMENT_LABEL[WalletMovementKind.UNSPECIFIED];
export type MovementFilter = "all" | "deposit" | "funding" | "profit" | "payout" | "trades" | "fees" | "other";
export const MOVEMENT_FILTERS: ReadonlyArray<[MovementFilter, string, WalletMovementKind[]]> = [
  ["all", "All", []],
  ["deposit", "Deposits", [WalletMovementKind.DEPOSIT]],
  ["funding", "Capital · Main ↔ Trading", [WalletMovementKind.CAPITAL_FUNDING, WalletMovementKind.CAPITAL_RETURN]],
  ["profit", "Profit transfers · Trading → Main", [WalletMovementKind.PROFIT_TRANSFER]],
  ["payout", "Payouts · Main → external", [WalletMovementKind.PAYOUT_MANUAL, WalletMovementKind.PAYOUT_AUTO]],
  ["trades", "Trades", [WalletMovementKind.TRADE_BUY, WalletMovementKind.TRADE_SELL]],
  ["fees", "Fees and nonce burns", [WalletMovementKind.FEE, WalletMovementKind.NONCE_BURN]],
  ["other", "Launches, outside and unattributed", [WalletMovementKind.LAUNCH, WalletMovementKind.EXTERNAL_OUT, WalletMovementKind.UNATTRIBUTED]],
];

export interface TransferRow { t: WalletTransferState; label: string; tone: Tone; path: string; duplicateTrigger: boolean }
/** Every transfer the owner reports once (the list and both policies' last transfer, newest state
 *  per request), newest first; two requests on one economic trigger are flagged, never hidden. */
export function transferRows(s: WalletTreasuryState): TransferRow[] {
  const byRequest = new Map<string, WalletTransferState>();
  for (const t of [...s.transfers, s.profitPolicy?.lastTransfer, s.payoutPolicy?.lastPayout]) {
    if (!t) continue;
    // The owner's durable intent first: one intent is one transfer, whatever request created it.
    const key = t.commandId || t.requestId || t.txHash;
    const seen = byRequest.get(key);
    if (!seen || t.updatedUnixMs > seen.updatedUnixMs) byRequest.set(key, t);
  }
  const rows = [...byRequest.values()].sort((a, b) => (b.updatedUnixMs > a.updatedUnixMs ? 1 : b.updatedUnixMs < a.updatedUnixMs ? -1 : 0));
  const triggers = new Map<string, number>();
  for (const t of rows) if (t.triggerId) triggers.set(`${t.kind}:${t.triggerId}`, (triggers.get(`${t.kind}:${t.triggerId}`) ?? 0) + 1);
  return rows.map(t => ({ t, ...transferLabel(t.status), path: movementLabel(t.kind), duplicateTrigger: !!t.triggerId && (triggers.get(`${t.kind}:${t.triggerId}`) ?? 0) > 1 }));
}
/** The newest transfer of one path. */
export const lastOf = (rows: readonly TransferRow[], kinds: readonly WalletMovementKind[]) => rows.find(r => kinds.includes(r.t.kind));
/** The decided states; every other one (DRAFT, PENDING, SENT, UNKNOWN, unspecified, a future value) is open. */
const DECIDED = new Set<TransferStatus>([TransferStatus.CONFIRMED, TransferStatus.FAILED, TransferStatus.EXPIRED]);
/** A transfer of this path not decided yet (a DRAFT awaiting its authorization included): the next
 *  one waits (no double transfer). */
export const openOf = (rows: readonly TransferRow[], kinds: readonly WalletMovementKind[]) =>
  rows.find(r => kinds.includes(r.t.kind) && !DECIDED.has(r.t.status));
/** The first line of a manual transfer's canonical scope text (wallet.proto). */
export const TRANSFER_SCOPE = "darkbot.bsc.wallet-transfer.v1";
/** A DRAFT holds its sending wallet's next nonce: while it is open nothing else leaves that wallet. */
export const draftFrom = (rows: readonly TransferRow[], role: WalletRole) => rows.find(r => r.t.status === TransferStatus.DRAFT && r.t.sourceRole === role);
const TRANSFER_KIND: Partial<Record<WalletMovementKind, string>> = { [WalletMovementKind.CAPITAL_FUNDING]: "CAPITAL_FUNDING", [WalletMovementKind.PAYOUT_MANUAL]: "PAYOUT_MANUAL" };
/** Why a DRAFT must not be authorized on this path, or null: its canonical text line by line against
 *  its own fields and the path's sender and receiver (wallet.proto darkbot.bsc.wallet-transfer.v1). */
export function draftProblem(t: WalletTransferState, kind: WalletMovementKind, from: string, to: string | undefined): string | null {
  if (!t.commandId) return "The owner names no draft id";
  if (t.status !== TransferStatus.DRAFT || t.kind !== kind) return "The draft is not one of this path";
  if (!to) return "The receiver of this path is not known";
  const lines = t.scopeText.split("\n");
  if (lines[0] !== TRANSFER_SCOPE || lines[lines.length - 1] !== "") return "The owner's scope text is not a manual transfer's";
  const want: Array<[string, string | undefined]> = [["kind", TRANSFER_KIND[kind]], ["chain_id", t.chainId.toString()], ["from", from.toLowerCase()], ["to", to.toLowerCase()],
    ["amount_wei", t.amountWei], ["max_fee_wei", undefined], ["nonce", t.nonce?.toString()], ["expires_unix_ms", t.expiresUnixMs?.toString()]];
  const got = lines.slice(1, -1);
  if (got.length !== want.length) return "The owner's scope text has other lines than a manual transfer's";
  for (const [i, [key, value]] of want.entries()) {
    const line = got[i] ?? "";
    if (!line.startsWith(`${key}=`)) return `The owner's scope text names ${line.split("=")[0] || "nothing"} where ${key} belongs`;
    const v = line.slice(key.length + 1);
    if (value === undefined ? !DIGITS.test(v) : v !== value) return `The owner's scope text says ${key}=${v}, the draft ${value ?? "a decimal"}`;
  }
  if (t.sourceAddress.toLowerCase() !== from.toLowerCase() || t.destinationAddress.toLowerCase() !== to.toLowerCase()) return "The draft's addresses are not this path's";
  return null;
}

// ---------------------------------------------------------------- operator input
const ADDRESS = /^0x[0-9a-fA-F]{40}$/;
const ZERO = /^0x0{40}$/;
type Ok<T> = { ok: true; value: T };
type Err = { ok: false; error: string };
const err = (error: string): Err => ({ ok: false, error });

/** The largest amount the chain carries (uint256). */
const U256_MAX = (1n << 256n) - 1n;
/** An exact BNB amount, 0 allowed, at most uint256 wei. */
export function wei(text: string, what: string): Ok<string> | Err {
  if (!text.trim()) return err(`${what}: enter a BNB amount`);
  const r = bnbToWei(text);
  if ("error" in r) return err(`${what}: ${r.error}`);
  return BigInt(r.wei) > U256_MAX ? err(`${what} is beyond the chain's uint256 range`) : { ok: true, value: r.wei };
}
/** An exact, positive BNB amount. */
export function positiveWei(text: string, what: string): Ok<string> | Err {
  const r = wei(text, what);
  return r.ok && r.value === "0" ? err(`${what} must be above 0`) : r;
}
/** Percent text (at most 2 places, above 0, at most 100) → basis points. */
export function percentToBps(text: string): Ok<number> | Err {
  const m = /^([0-9]{1,3})(?:[.,]([0-9]{1,2}))?$/.exec(text.trim());
  if (!m) return err(`${text} is no percentage with at most 2 places`);
  const bps = Number(m[1]) * 100 + Number((m[2] ?? "").padEnd(2, "0"));
  return bps < 1 || bps > 10_000 ? err("A share is above 0 % and at most 100 %") : { ok: true, value: bps };
}
export const bpsToPercent = (bps: number) => `${Math.floor(bps / 100)}${bps % 100 ? `.${String(bps % 100).padStart(2, "0").replace(/0$/, "")}` : ""}`;
/** A decimal EUR amount (at most 2 places) in its canonical form (wallet.proto: no leading zero, no
 *  trailing fraction zero, e.g. 1000, 10.5, 0.01); `positive` refuses 0. */
export function eurText(text: string, what: string, positive = false): Ok<string> | Err {
  const t = text.trim().replace(",", ".");
  if (!/^[0-9]+(?:\.[0-9]{1,2})?$/.test(t)) return err(`${what}: ${text} is no EUR amount with at most 2 places`);
  const [int = "", frac = ""] = t.split(".");
  const value = `${int.replace(/^0+(?=[0-9])/, "")}${frac.replace(/0+$/, "") ? `.${frac.replace(/0+$/, "")}` : ""}`;
  return positive && value === "0" ? err(`${what} must be above 0`) : { ok: true, value };
}
/** A BSC address of an external receiver: not zero and none of DarkBot's own accounts. */
export function externalAddress(text: string, own: readonly string[]): Ok<string> | Err {
  const a = text.trim();
  if (!ADDRESS.test(a)) return err("A BSC address is 0x and 40 hex digits");
  if (ZERO.test(a)) return err("The zero address receives nothing");
  if (own.some(o => o && o.toLowerCase() === a.toLowerCase())) return err("This is one of DarkBot's own accounts, not an external wallet");
  return { ok: true, value: a };
}
/** What `available` keeps after `amount` and at most `fee` (a preview: the owner decides). */
export function remaining(available: string | undefined, amount: string, fee: string | undefined): Ok<string> | Err {
  if (!digits(available)) return err("The free balance is not reported");
  if (!digits(fee)) return err("The fee estimate is not reported");
  const rest = BigInt(available as string) - BigInt(amount) - BigInt(fee as string);
  return rest < 0n ? err("Amount and fee exceed the free balance") : { ok: true, value: rest.toString() };
}

export type AmountMode = "fixed" | "share";
/** An enable's authorization as the operator types it (wallet.proto WalletAuthorizationScope). */
export interface AuthorizationForm { validHours: string; maxPerTransfer: string; maxTotal: string }
export interface ProfitPolicyForm extends AuthorizationForm { enabled: boolean; threshold: string; mode: AmountMode; amount: string; minCapital: string; reserve: string }
export interface PayoutPolicyForm extends AuthorizationForm { enabled: boolean; destinationId: string; unit: "BNB" | "EUR"; threshold: string; mode: AmountMode; amount: string; retain: string }
/** The stored authorization's limits as the start of a new one; its validity is always typed anew. */
const authorizationForm = (a: ProfitTransferPolicy["authorization"]): AuthorizationForm =>
  ({ validHours: "", maxPerTransfer: weiToBnb(a?.maxPerTransferWei ?? ""), maxTotal: weiToBnb(a?.maxTotalWei ?? "") });

const HOUR_MS = 3_600_000n;
export interface AuthorizationLimits { validityMs: bigint; maxPerTransferWei: string; maxTotalWei: string }
/** An enable's authorization within the owner's configured bounds, or the first error. */
export function authorizationOf(f: AuthorizationForm, bounds: WalletAuthorizationBounds | undefined): Ok<AuthorizationLimits> | Err {
  if (!bounds || !digits(bounds.maxPerTransferWei) || !digits(bounds.maxTotalWei)) return err("The owner reports no authorization bounds");
  if (!/^[1-9][0-9]{0,5}$/.test(f.validHours.trim())) return err("Valid for: whole hours above 0");
  const validityMs = BigInt(f.validHours.trim()) * HOUR_MS;
  if (validityMs > bounds.maxValidityMs) return err(`Valid for at most ${(bounds.maxValidityMs / HOUR_MS).toString()} h (the owner's bound)`);
  const per = positiveWei(f.maxPerTransfer, "Most per transfer");
  if (!per.ok) return per;
  if (BigInt(per.value) > BigInt(bounds.maxPerTransferWei)) return err(`Most per transfer is above the owner's bound of ${weiToBnb(bounds.maxPerTransferWei)} BNB`);
  const total = positiveWei(f.maxTotal, "Most in total");
  if (!total.ok) return total;
  if (BigInt(total.value) > BigInt(bounds.maxTotalWei)) return err(`Most in total is above the owner's bound of ${weiToBnb(bounds.maxTotalWei)} BNB`);
  if (BigInt(total.value) < BigInt(per.value)) return err("Most in total is below most per transfer");
  return { ok: true, value: { validityMs, maxPerTransferWei: per.value, maxTotalWei: total.value } };
}

const amountText = (a: ProfitTransferPolicy["amount"], eur = false): [AmountMode, string] => {
  const v = a?.amount;
  if (v?.case === "shareBps") return ["share", bpsToPercent(v.value)];
  if (v?.case === "fixedWei") return ["fixed", weiToBnb(v.value)];
  if (v?.case === "fixedEur" && eur) return ["fixed", v.value];
  return ["share", ""];
};
/** The stored policy A as the editor's start; empty and off before the owner ever stored one. */
export function profitForm(p: ProfitTransferPolicy | undefined): ProfitPolicyForm {
  const [mode, amount] = amountText(p?.amount);
  return { enabled: !!p?.enabled, threshold: weiToBnb(p?.thresholdRealizedNetProfitWei ?? ""), mode, amount, minCapital: weiToBnb(p?.buyerMinCapitalWei ?? ""), reserve: weiToBnb(p?.buyerReserveWei ?? ""), ...authorizationForm(p?.authorization) };
}
export function payoutForm(p: MainPayoutPolicy | undefined): PayoutPolicyForm {
  const eur = p?.unit === PolicyUnit.EUR;
  const [mode, amount] = amountText(p?.amount, eur);
  return { enabled: !!p?.enabled, destinationId: p?.destinationId ?? "", unit: eur ? "EUR" : "BNB", threshold: eur ? p?.thresholdEur ?? "" : weiToBnb(p?.thresholdAvailableMainWei ?? ""), mode, amount, retain: weiToBnb(p?.retainWei ?? ""), ...authorizationForm(p?.authorization) };
}
type AmountInit = { amount: { case: "fixedWei" | "fixedEur"; value: string } | { case: "shareBps"; value: number } };
function amountOf(mode: AmountMode, text: string, eur: boolean, what: string): Ok<AmountInit> | Err {
  if (mode === "share") { const b = percentToBps(text); return b.ok ? { ok: true, value: { amount: { case: "shareBps", value: b.value } } } : b; }
  if (eur) { const e = eurText(text, what, true); return e.ok ? { ok: true, value: { amount: { case: "fixedEur", value: e.value } } } : e; }
  const v = positiveWei(text, what);
  return v.ok ? { ok: true, value: { amount: { case: "fixedWei", value: v.value } } } : v;
}
/** Policy A's set command, or the first input error. */
export function profitCommand(f: ProfitPolicyForm) {
  const threshold = wei(f.threshold, "Profit threshold");
  if (!threshold.ok) return threshold;
  const amount = amountOf(f.mode, f.amount, false, "Transfer amount");
  if (!amount.ok) return amount;
  const minCapital = wei(f.minCapital, "Minimum trading capital");
  if (!minCapital.ok) return minCapital;
  const reserve = wei(f.reserve, "Exit and gas reserve");
  if (!reserve.ok) return reserve;
  return { ok: true as const, value: { enabled: f.enabled, thresholdRealizedNetProfitWei: threshold.value, amount: amount.value, buyerMinCapitalWei: minCapital.value, buyerReserveWei: reserve.value } };
}
/** Policy B's set command, or the first input error (EUR only with a qualified native quote; never
 *  to one of DarkBot's own accounts; only on Main's chain). */
export function payoutCommand(f: PayoutPolicyForm, destinations: readonly WalletDestination[], eurQualified: boolean, own: readonly string[] = [], mainChainId?: bigint) {
  const dest = destinations.find(d => d.destinationId === f.destinationId);
  if (!dest) return err("Choose a saved external address");
  if (!dest.confirmed) return err("The external address is not confirmed");
  const chain = chainProblem(dest.chainId, mainChainId);
  if (chain) return err(chain);
  const external = externalAddress(dest.address, own);
  if (!external.ok) return external;
  const eur = f.unit === "EUR";
  if (eur && !eurQualified) return err("EUR needs a qualified, fresh native BNB/EUR price; use BNB");
  const threshold = eur ? eurText(f.threshold, "Main balance threshold") : wei(f.threshold, "Main balance threshold");
  if (!threshold.ok) return threshold;
  const amount = amountOf(f.mode, f.amount, eur, "Payout amount");
  if (!amount.ok) return amount;
  const retain = wei(f.retain, "Main reserve");
  if (!retain.ok) return retain;
  return { ok: true as const, value: { enabled: f.enabled, destinationId: dest.destinationId, destinationAddress: dest.address, unit: eur ? PolicyUnit.EUR : PolicyUnit.BNB,
    thresholdAvailableMainWei: eur ? "" : threshold.value, thresholdEur: eur ? threshold.value : undefined, amount: amount.value, retainWei: retain.value } };
}
/** Disabling a stored policy A unchanged: its stored values as they are, nothing re-validated. */
export function profitDisable(p: ProfitTransferPolicy) {
  return { ok: true as const, value: { enabled: false, thresholdRealizedNetProfitWei: p.thresholdRealizedNetProfitWei, amount: p.amount, buyerMinCapitalWei: p.buyerMinCapitalWei, buyerReserveWei: p.buyerReserveWei } };
}
/** Disabling a stored policy B unchanged (also without a fresh EUR quote). */
export function payoutDisable(p: MainPayoutPolicy) {
  return { ok: true as const, value: { enabled: false, destinationId: p.destinationId, destinationAddress: p.destinationAddress, unit: p.unit,
    thresholdAvailableMainWei: p.thresholdAvailableMainWei, thresholdEur: p.thresholdEur, amount: p.amount, retainWei: p.retainWei } };
}
/** Whether the owner's BNB/EUR quote is qualified: VERIFIED by the owner (it marks a quote beyond
 *  its max age STALE), in EUR, with a price. The browser clock is not consulted. */
export function eurQualified(s: WalletTreasuryState | undefined): boolean {
  const q = s?.eurQuote;
  return !!q && q.quality === DataQuality.VERIFIED && q.currency === "EUR" && /^[0-9]+(\.[0-9]+)?$/.test(q.fiatPerBnb) && !/^0+(\.0+)?$/.test(q.fiatPerBnb);
}

/** Why a destination's chain cannot receive from Main (wallet.proto: Main's chain only), or null. */
export function chainProblem(chainId: bigint, mainChainId: bigint | undefined): string | null {
  if (mainChainId === undefined) return "The Main account reports no chain id";
  return chainId === mainChainId ? null : `The address is saved for chain ${chainId}, Main is on chain ${mainChainId}`;
}

/** The canonical meaning of a policy form (exact wei, bps, EUR), so that "0.50" and the stored "0.5"
 *  are the same edit; `withValidity` counts a typed validity as a change (a new authorization). */
const canonWei = (t: string) => { const r = wei(t, ""); return r.ok ? r.value : `?${t}`; };
const canonAmount = (mode: AmountMode, t: string, eur: boolean) => mode === "share" ? `bps:${(r => r.ok ? r.value : `?${t}`)(percentToBps(t))}` : eur ? `eur:${(r => r.ok ? r.value : `?${t}`)(eurText(t, ""))}` : `wei:${canonWei(t)}`;
export function profitKey(f: ProfitPolicyForm, withValidity: boolean): string {
  return [f.enabled, canonWei(f.threshold), canonAmount(f.mode, f.amount, false), canonWei(f.minCapital), canonWei(f.reserve), canonWei(f.maxPerTransfer), canonWei(f.maxTotal), withValidity ? f.validHours.trim() : ""].join("|");
}
export function payoutKey(f: PayoutPolicyForm, withValidity: boolean): string {
  const eur = f.unit === "EUR";
  return [f.enabled, f.destinationId, f.unit, eur ? (r => r.ok ? r.value : `?${f.threshold}`)(eurText(f.threshold, "")) : canonWei(f.threshold), canonAmount(f.mode, f.amount, eur), canonWei(f.retain),
    canonWei(f.maxPerTransfer), canonWei(f.maxTotal), withValidity ? f.validHours.trim() : ""].join("|");
}

/** Stored vs effective policy version, as the owner reports them. */
export function versionText(p: { policyVersion: bigint; effectiveVersion?: bigint } | undefined): string {
  if (!p) return "Never stored";
  if (p.effectiveVersion === undefined) return `Stored v${p.policyVersion} · effect not reported`;
  return p.effectiveVersion === p.policyVersion ? `v${p.policyVersion} · effective` : `Stored v${p.policyVersion} · effective v${p.effectiveVersion}`;
}

// ---------------------------------------------------------------- canonical scope texts
// wallet.proto: byte for byte the owner's text, lowercase 0x addresses, decimal integers, one
// `key=value\n` per line; its SHA-256 (model/live.ts scopeSha256) is what the operator confirms.
type AmountLine = AmountInit["amount"];
const amountLine = (a: AmountLine) => `${a.case === "shareBps" ? "share_bps" : a.case === "fixedWei" ? "fixed_wei" : "fixed_eur"}:${a.value}`;
const lines = (name: string, kv: ReadonlyArray<[string, string]>) => `${name}\n${kv.map(([k, v]) => `${k}=${v}\n`).join("")}`;
const authLines = (a: AuthorizationLimits, expiresUnixMs: bigint): Array<[string, string]> =>
  [["max_per_transfer_wei", a.maxPerTransferWei], ["max_total_wei", a.maxTotalWei], ["expires_unix_ms", expiresUnixMs.toString()]];
export type ProfitCommand = Extract<ReturnType<typeof profitCommand>, { ok: true }>["value"];
export type PayoutCommand = Extract<ReturnType<typeof payoutCommand>, { ok: true }>["value"];
/** Policy A's canonical scope text (darkbot.bsc.wallet-profit-policy.v1). */
export function profitScopeText(chainId: bigint, buyer: string, main: string, c: ProfitCommand, a: AuthorizationLimits, expiresUnixMs: bigint): string {
  return lines("darkbot.bsc.wallet-profit-policy.v1", [["chain_id", chainId.toString()], ["from", buyer.toLowerCase()], ["to", main.toLowerCase()], ["enabled", String(c.enabled)],
    ["threshold_realized_net_profit_wei", c.thresholdRealizedNetProfitWei], ["amount", amountLine(c.amount.amount)], ["buyer_min_capital_wei", c.buyerMinCapitalWei],
    ["buyer_reserve_wei", c.buyerReserveWei], ...authLines(a, expiresUnixMs)]);
}
/** Policy B's canonical scope text (darkbot.bsc.wallet-payout-policy.v1). */
export function payoutScopeText(chainId: bigint, main: string, c: PayoutCommand, a: AuthorizationLimits, expiresUnixMs: bigint): string {
  const eur = c.unit === PolicyUnit.EUR;
  return lines("darkbot.bsc.wallet-payout-policy.v1", [["chain_id", chainId.toString()], ["from", main.toLowerCase()], ["to", c.destinationAddress.toLowerCase()],
    ["destination_id", c.destinationId], ["enabled", String(c.enabled)], ["unit", eur ? "EUR" : "BNB"],
    eur ? ["threshold_eur", c.thresholdEur ?? ""] : ["threshold_available_main_wei", c.thresholdAvailableMainWei], ["amount", amountLine(c.amount.amount)],
    ["retain_wei", c.retainWei], ...authLines(a, expiresUnixMs)]);
}

// V8 Live trades (FE-V8-11, Bindings §14.1/§14.3 item 7): the one trade list from the bounded LIVE
// state (BscLiveState positions, transactions and evaluated attempts) until the native history
// query exists (BSC-V8-07…10). A value only when the owner's record names it for this lot: the
// attempt of the lot's BUY hash gives its trigger source, local send/ack timing, placement and BUY
// block time; venue, sell time, hold, lead before block, ROI and exit reason need the history
// projection and stay "—" here. The status label is a display of the ledger state and the sign
// of the native PnL only.
import type { BscLiveView, LiveAttemptView, LivePositionView, LiveTransactionView } from "./types";
import { MAX_DATE_MS } from "./live";

export const TRADE_COLUMNS = [
  { id: "token", label: "Token", short: "Token" },
  { id: "status", label: "Status", short: "Status" },
  { id: "market", label: "Market", short: "Market" },
  { id: "invest", label: "Investment", short: "Invest" },
  { id: "pnl", label: "Net PnL", short: "PnL" },
  { id: "roi", label: "Return (%)", short: "ROI" },
  { id: "buy", label: "Buy time", short: "Buy" },
  { id: "sell", label: "Sell time", short: "Sell" },
  { id: "hold", label: "Hold time", short: "Hold" },
  { id: "source", label: "First source", short: "Source" },
  { id: "lead", label: "Before block", short: "Lead" },
  { id: "send", label: "Detect → send", short: "Send" },
  { id: "ack", label: "Send → response", short: "Ack" },
  { id: "gas", label: "Network fee", short: "Gas" },
  { id: "placement", label: "Placement", short: "Order" },
  { id: "exit", label: "Exit reason", short: "Exit" },
  { id: "tx", label: "Buy transaction", short: "TX" },
] as const;
export type TradeColumn = (typeof TRADE_COLUMNS)[number]["id"];
export const TRADE_COL_DEFAULT: readonly TradeColumn[] = ["token", "status", "invest", "pnl", "buy", "sell", "source", "send"];
export const RIGHT_ALIGNED: ReadonlySet<TradeColumn> = new Set(["invest", "pnl", "roi", "lead", "send", "ack", "gas", "hold"]);
const IDS = new Set<string>(TRADE_COLUMNS.map(c => c.id));
/** A stored column list, valid ids only, each once, token always first-visible. */
export function safeColumns(ids: unknown): TradeColumn[] {
  const list = Array.isArray(ids) ? [...new Set(ids.filter((id): id is TradeColumn => typeof id === "string" && IDS.has(id)))] : [];
  return list.includes("token") ? list : ["token", ...list];
}

export type TradeLabel = "RUN" | "PEND" | "WIN" | "LOSS" | "DONE" | "FAIL" | "WOFF" | "UNKN";
export interface TradeView {
  lot: string; token: string; label: TradeLabel; state: string; component: string;
  /** Tokens the BUY received, decimal. */
  tokens: string;
  inputWei: string; proceedsWei?: string; gasWei: string; pnlWei?: string; proceedsUnknown: boolean;
  /** The evaluated attempt of this lot's BUY; absent without one. */
  attempt?: LiveAttemptView;
  /** The lot's transactions: its BUY, and the token's others only when no other lot of the token
   *  exists (the state names no lot per transaction). */
  transactions: LiveTransactionView[];
  /** Other lots of the same token exist: its later transactions are not attributable here. */
  sharedToken: boolean;
  decisions: string[];
  /** The BUY block's header time, unix ms (from the attempt). */
  buyMs?: bigint;
}

const isNeg = (w: string | undefined) => !!w && /^-[0-9]+$/.test(w) && !/^-0+$/.test(w);
const isPos = (w: string | undefined) => !!w && /^[0-9]+$/.test(w) && !/^0+$/.test(w);
/** RUN/PEND/WIN/LOSS from the ledger state and the native PnL's sign; a lower bound
 *  (proceeds unknown) is a WIN only when it is above zero, else UNKN; never 0-PnL as WIN. */
export function tradeLabel(p: LivePositionView): TradeLabel {
  if (p.state === "OPEN" || p.state === "EXITING") return "RUN";
  if (p.state === "PENDING") return "PEND";
  if (p.state === "FAILED") return "FAIL";
  // An operator's write-off: input and gas lost, a final state (never pending, never a normal SELL).
  if (p.state === "WRITTEN_OFF") return "WOFF";
  if (p.state !== "CLOSED") return "UNKN";
  if (isPos(p.pnlWei)) return "WIN";
  if (p.proceedsUnknown) return "UNKN";
  if (isNeg(p.pnlWei)) return "LOSS";
  return p.pnlWei === undefined ? "UNKN" : "DONE";
}
export const LABEL_TONE: Record<TradeLabel, "blue" | "warn" | "good" | "bad" | ""> = { RUN: "blue", PEND: "warn", WIN: "good", LOSS: "bad", DONE: "", FAIL: "bad", WOFF: "bad", UNKN: "warn" };
const ORDER: Record<TradeLabel, number> = { RUN: 0, PEND: 1, UNKN: 2, WIN: 3, LOSS: 3, DONE: 3, WOFF: 3, FAIL: 4 };

/** Every lot of the ledger, active first (RUN, PEND, UNKN), then the closed; newest BUY first
 *  inside a group where the attempt names its block time, else ledger order reversed. */
export function tradeViews(live: BscLiveView | null): TradeView[] {
  if (!live) return [];
  const byTx = new Map(live.attempts.map(a => [a.tx.toLowerCase(), a]));
  const rows = live.positions.map((p, i): TradeView & { i: number } => {
    const attempt = byTx.get(p.lot.toLowerCase());
    const sharedToken = live.positions.filter(q => q.token.toLowerCase() === p.token.toLowerCase()).length > 1;
    const own = live.transactions.filter(t => t.txHash.toLowerCase() === p.lot.toLowerCase() || (!sharedToken && t.token.toLowerCase() === p.token.toLowerCase()));
    return { i, lot: p.lot, token: p.token, tokens: p.tokens, sharedToken, label: tradeLabel(p), state: p.state, component: p.component, inputWei: p.inputWei,
      proceedsWei: p.state === "CLOSED" ? p.proceedsWei : undefined, gasWei: p.gasWei, pnlWei: p.pnlWei, proceedsUnknown: p.proceedsUnknown,
      attempt, transactions: own, decisions: p.decisions, buyMs: attempt?.blockMs };
  });
  rows.sort((a, b) => ORDER[a.label] - ORDER[b.label] || (a.buyMs !== undefined && b.buyMs !== undefined ? (b.buyMs > a.buyMs ? 1 : b.buyMs < a.buyMs ? -1 : 0) : b.i - a.i));
  return rows.map(({ i: _i, ...r }) => r);
}

export interface TradeFilter { search: string; status: "all" | "open" | "unknown" | "win" | "loss" | "closed" }
export function filterTrades(rows: readonly TradeView[], f: TradeFilter): TradeView[] {
  const q = f.search.trim().toLowerCase();
  return rows.filter(r => {
    const s = f.status === "all" || (f.status === "open" && r.label === "RUN") || (f.status === "unknown" && (r.label === "PEND" || r.label === "UNKN"))
      || (f.status === "win" && r.label === "WIN") || (f.status === "loss" && (r.label === "LOSS" || r.label === "FAIL" || r.label === "WOFF")) || (f.status === "closed" && (r.state === "CLOSED" || r.state === "WRITTEN_OFF"));
    return s && (!q || [r.token, r.lot, ...r.transactions.map(t => t.txHash)].some(v => v.toLowerCase().includes(q)));
  });
}
export const counts = (rows: readonly TradeView[]) => ({
  run: rows.filter(r => r.label === "RUN").length, pend: rows.filter(r => r.label === "PEND" || r.label === "UNKN").length,
  win: rows.filter(r => r.label === "WIN").length, loss: rows.filter(r => r.label === "LOSS" || r.label === "FAIL" || r.label === "WOFF").length,
});

/** ns as ms text with two places, exact integer arithmetic. */
export function nsToMs(ns: bigint | undefined): string {
  if (ns === undefined) return "—";
  const hundredths = (ns + 5_000n) / 10_000n;
  return `${hundredths / 100n}.${(hundredths % 100n).toString().padStart(2, "0")} ms`;
}
/** A unix-ms block time as time and date (UTC; the block header's own clock). */
export function blockClock(ms: bigint | undefined): { time: string; date: string } | null {
  if (ms === undefined || ms <= 0n || ms > MAX_DATE_MS) return null;
  const d = new Date(Number(ms));
  return { time: d.toISOString().slice(11, 19), date: d.toISOString().slice(0, 10) };
}
/** CSV of the shown rows (local export of what the table holds; values as displayed natively). */
export function tradesCsv(rows: readonly TradeView[]): string {
  const head = ["Token", "Lot", "Status", "Ledger state", "Input wei", "Proceeds wei", "Gas wei", "Net PnL wei", "Proceeds unknown", "First source", "Detect to send ns", "Send to ack ns", "Placement", "Buy block unix ms"];
  const cell = (v: string | bigint | boolean | undefined) => `"${String(v ?? "").split('"').join('""')}"`;
  return [head.map(cell).join(";"), ...rows.map(r => [r.token, r.lot, r.label, r.state, r.inputWei, r.proceedsWei, r.gasWei, r.pnlWei, r.proceedsUnknown, r.attempt?.trigger, r.attempt?.ingressToSendNs, r.attempt?.sendToAckNs, r.attempt?.placed, r.buyMs].map(cell).join(";"))].join("\n");
}

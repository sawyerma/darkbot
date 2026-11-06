// V8 Overview (FE-V8-03): the compact system status from the native projections, once. Nothing
// is estimated: an unreported metric is "—", a stale runtime status gives no signal age.
import type { BscLiveView, BscWalletsView, NativeRecordView } from "./types";
import { RUNTIME_SCHEMA, runtimeView } from "./sources";

export type TradingState = { label: "Trading ON" | "Trading OFF" | "Trading UNKNOWN"; tone: "good" | "" | "warn"; detail: string };

/** ON only while the owner's LIVE gate is CLEAR and the runtime's LIVE path is ARMED. */
export function tradingState(live: BscLiveView | null): TradingState {
  if (!live || live.gate === "UNKNOWN" || live.gate === "") return { label: "Trading UNKNOWN", tone: "warn", detail: "No readable LIVE owner state" };
  if (live.gate === "CLEAR" && live.pathState === "ARMED") return { label: "Trading ON", tone: "good", detail: live.grant?.exitOnly ? "Exits only (EXIT-ONLY grant)" : "LIVE path armed under the grant" };
  return { label: "Trading OFF", tone: "", detail: live.blockers[0] ?? live.pathReason ?? "LIVE gate blocked" };
}

/** The youngest last-event age of the sources a fresh runtime status reports, ms. */
export function lastSignalMs(records: readonly NativeRecordView[], nowMs: number): number | undefined {
  if (runtimeView(records, nowMs).view.state !== "RUNNING") return undefined;
  const f = new Map(records.find(r => r.schema === RUNTIME_SCHEMA)?.fields.map(x => [x.name, x.value]) ?? []);
  let best: number | undefined;
  for (let i = 0; f.has(`sources[${i}].source_id`); i++) {
    const v = f.get(`sources[${i}].last_event_age_ms`);
    if (v !== undefined && /^[0-9]+$/.test(v) && (best === undefined || Number(v) < best)) best = Number(v);
  }
  return best;
}

export interface Capital { totalWei?: string; detail: string }
/** The BSC wallets' balance: the LIVE buyer's at its last probe when a current status reports it,
 *  else the snapshot; one wallet named twice is counted once. Exact integer wei. */
export function capital(w: BscWalletsView | null): Capital {
  if (!w) return { detail: "No wallet projection" };
  const buyer = w.liveBalanceWei ?? w.buyerBalanceWei;
  const launcher = w.launcherBalanceWei;
  const same = !!w.launcherAddress && w.launcherAddress.toLowerCase() === w.buyerAddress.toLowerCase();
  const parts = same ? [buyer ?? launcher] : [launcher, buyer];
  if (parts.some(p => p === undefined || !/^[0-9]+$/.test(p))) return { detail: "A balance is not reported" };
  const total = parts.reduce((a, p) => a + BigInt(p as string), 0n);
  return { totalWei: total.toString(), detail: `${w.liveBalanceWei !== undefined ? "Buyer at the LIVE probe" : "Snapshot"}${same ? " · one wallet" : ""}` };
}

/** Released transactions the chain has not decided (RELEASED, UNKNOWN). */
export const pendingTransactions = (live: BscLiveView | null) => (live?.transactions ?? []).filter(t => t.state === "RELEASED" || t.state === "UNKNOWN").length;
/** Positions held now (OPEN, EXITING). */
export const openPositions = (live: BscLiveView | null) => (live?.positions ?? []).filter(p => p.state === "OPEN" || p.state === "EXITING").length;

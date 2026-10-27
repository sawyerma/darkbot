// The wallet and economic switches of the native bsc_runtime owner (plan
// 27.10.), as the reporter projects them typed: requested, effective in this
// boot session, block reason, generation (the CAS of the next request) and
// the request that last set each. Nothing is inferred locally; without the
// owner's state nothing can be requested.
import type { BuyAttemptLimitView, ControlSwitchView } from "./types";

export const WALLET_SWITCHES = ["BSC_WALLET_LAUNCHER", "BSC_WALLET_BUYER"] as const;
export const ECONOMIC_SWITCHES = ["BSC_ECON_SIGN", "BSC_ECON_ARM", "BSC_ECON_SEND", "BSC_ECON_ATTEMPT", "BSC_ECON_LIVE"] as const;
export const SWITCHES_UNAVAILABLE = "BACKEND_UNAVAILABLE";

/** What each switch permits; the Mainnet path still checks its own guard, V5, nonce, budget and attempt. */
export const SWITCH_TEXT: Record<string, { label: string; hint: string }> = {
  BSC_WALLET_LAUNCHER: { label: "Launcher wallet", hint: "Mainnet signing and sending with the launcher wallet (DRY SIGN, G0.7 launch)" },
  BSC_WALLET_BUYER: { label: "Buyer wallet", hint: "Mainnet signing and sending with the buyer wallet (DRY SIGN, G0.7 BUY)" },
  BSC_ECON_SIGN: { label: "SIGN", hint: "Mainnet wallet signatures (DRY SIGN, the G0.7 attempt)" },
  BSC_ECON_ARM: { label: "ARM", hint: "Use of an armed acceptance guard (compiled V5 hash) by the G0.7 attempt" },
  BSC_ECON_SEND: { label: "SEND", hint: "Mainnet submission: the G0.7 release and a live builder probe" },
  BSC_ECON_ATTEMPT: { label: "Attempt", hint: "Consuming the one-attempt authority of a G0.7 attempt" },
  BSC_ECON_LIVE: { label: "Economic LIVE", hint: "The global LIVE condition; every requested LIVE stays BLOCKED without an authorized LIVE executor (J)" },
  BSC_RUNTIME_MODE: { label: "Runtime mode", hint: "Global execution mode: SHADOW runs the Strategy C ledger (no signature), PAPER also the C3 path to the PAPER sink (PAPER identity, nothing sent); LIVE stays BLOCKED; replay is the offline job" },
};
/** The runtime mode's requestable modes; LIVE is recorded and stays BLOCKED natively. */
export const RUNTIME_MODES = ["OFF", "SHADOW", "PAPER", "LIVE"] as const;

export interface SwitchRow {
  id: string; label: string; hint: string;
  requested: string; effective: string; blockReason: string;
  /** The owner generation, the CAS of the next request. */
  generation: bigint;
  /** The request (or RESTART_RESET) that last set it, with its generation. */
  receipt: string;
  /** Requestable: the owner's readable state is projected. */
  available: boolean;
  reason: string;
}

/** One switch row from the typed projection; absent or unreadable is never requestable. */
export function switchRow(id: string, s: ControlSwitchView | undefined): SwitchRow {
  const text = SWITCH_TEXT[id] ?? { label: id, hint: "" };
  if (!s) return { id, ...text, requested: SWITCHES_UNAVAILABLE, effective: SWITCHES_UNAVAILABLE, blockReason: "", generation: 0n, receipt: "—", available: false, reason: SWITCHES_UNAVAILABLE };
  const readable = s.requested === "DISABLED" || s.requested === "ENABLED" || (id === "BSC_RUNTIME_MODE" && (RUNTIME_MODES as readonly string[]).includes(s.requested));
  return {
    id, ...text, requested: s.requested, effective: s.effective, blockReason: s.blockReason, generation: s.generation,
    receipt: s.requestId ? `${s.requestId} @ generation ${s.setGeneration?.toString() ?? "?"}` : "never set",
    available: readable, reason: readable ? "" : s.blockReason || "BACKEND_INVALID",
  };
}

/** The economic switches in their fixed order. */
export function economicRows(controls: readonly ControlSwitchView[] | undefined): SwitchRow[] {
  return ECONOMIC_SWITCHES.map(id => switchRow(id, controls?.find(c => c.id === id)));
}

/** One line for a summary: each switch with its effective state. */
export function summary(rows: readonly SwitchRow[]): string {
  return rows.map(r => `${r.label} ${r.effective}${r.blockReason ? ` (${r.blockReason})` : ""}`).join(" · ");
}

/** The daily BUY attempt limit (UTC day) with its use; absent without the owner's state. */
export interface BuyLimitRow { available: boolean; perDay: bigint; dayUtc: string; used: bigint; remaining: bigint; generation: bigint; receipt: string; summary: string }
export function buyLimitRow(l: BuyAttemptLimitView | undefined): BuyLimitRow {
  if (!l) return { available: false, perDay: 0n, dayUtc: "—", used: 0n, remaining: 0n, generation: 0n, receipt: "—", summary: SWITCHES_UNAVAILABLE };
  return {
    available: true, perDay: l.perDay, dayUtc: l.dayUtc, used: l.used, remaining: l.remaining, generation: l.generation,
    receipt: l.requestId ? `${l.requestId} @ generation ${l.setGeneration?.toString() ?? "?"}` : "never set",
    summary: `${l.used.toString()} of ${l.perDay.toString()} used on ${l.dayUtc} (UTC) · ${l.remaining.toString()} remaining`,
  };
}
/** A limit the native owner takes: a plain decimal within u32 (the owner checks again). */
export function limitInputOk(text: string): boolean {
  return /^(0|[1-9][0-9]{0,9})$/.test(text) && BigInt(text) <= 4294967295n;
}

/** A mode button is locked while the switch already holds that mode; an ENABLED of an earlier boot
 *  (effective BLOCKED RESET_BY_RESTART) can be requested again in this boot. */
export function modeHeld(row: SwitchRow, mode: string): boolean {
  return row.requested === mode && !(row.effective === "BLOCKED" && row.blockReason === "RESET_BY_RESTART");
}

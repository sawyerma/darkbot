// The BSC LIVE tab (plan 28.10.): what the native owner's LIVE gate needs, each condition with
// the owner's own reason, and the canonical scope of a grant exactly as the owner computes it
// (its SHA-256 is what the operator confirms). Nothing here decides LIVE: the owner admits every
// intent again at its dispatch.
import type { LiveGrantScope } from "@/core/realtime/sourceControl";
import type { BscLiveView, ControlSwitchView } from "./types";

export const LIVE_VENUES = ["BSC_VENUE_FLAP"] as const;
export const LIVE_STRATEGIES = ["C3", "STRATEGY_C", "C3+STRATEGY_C"] as const;
export const LIVE_SOURCES = ["BSC_INGRESS_GETH", "BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_DRPC", "BSC_INGRESS_BLOCKREQ", "BSC_INGRESS_48CLUB"] as const;
export const LIVE_ADAPTERS = ["CLUB48_BUILDER_V1", "BLOCKRAZOR_BUILDER_V1"] as const;
/** The campaign's limit: at most ten real BUY attempts (PHASENPLAN 28.10.); the owner checks again. */
export const MAX_ATTEMPTS_TOTAL = 10;

/** What each gate condition is and where an operator meets it. */
export const CONDITION_TEXT: Record<string, { label: string; where: string }> = {
  ECONOMIC_LIVE: { label: "Economic LIVE switch", where: "Start / Stop on this tab (BSC_ECON_LIVE)" },
  LIVE_GRANT: { label: "LIVE grant of a confirmed scope", where: "Grant on this tab" },
  WALLET_LAUNCHER: { label: "Launcher wallet ENABLED", where: "Wallets tab" },
  WALLET_BUYER: { label: "Buyer wallet ENABLED", where: "Wallets tab" },
  ECON_SIGN: { label: "SIGN ENABLED", where: "Launch / BUY tab" },
  ECON_ARM: { label: "ARM ENABLED", where: "Launch / BUY tab" },
  ECON_SEND: { label: "SEND ENABLED", where: "Launch / BUY tab" },
  ECON_ATTEMPT: { label: "Attempt ENABLED", where: "Launch / BUY tab" },
  RUNTIME_LIVE: { label: "Runtime mode LIVE requested", where: "Scope on this tab" },
  BUY_LIMIT_DAILY: { label: "Daily BUY attempts left", where: "Attempts on this tab" },
  BUY_LIMIT_TOTAL: { label: "Campaign BUY attempts left (at most 10)", where: "Attempts on this tab" },
  GRANT_ATTEMPTS: { label: "The grant's attempts left", where: "A new grant" },
  GRANT_SPEND: { label: "The grant's spend left", where: "A new grant" },
  GRANT_BUYS: { label: "The grant admits BUYs (not EXIT-ONLY)", where: "A new grant" },
  VENUE_QUALIFIED: { label: "Venue qualified", where: "LIVE qualification record (bsc_runtime live qualify)" },
  SOURCES_QUALIFIED: { label: "Sources qualified", where: "LIVE qualification record" },
  ADAPTERS_QUALIFIED: { label: "Adapters qualified", where: "LIVE qualification record" },
  ACQUISITION_PROVEN: { label: "G0.8 acquisition PASS", where: "LIVE qualification record" },
  RISK_DECISION: { label: "Operator's risk decision", where: "LIVE qualification record" },
};

export interface ConditionRow { id: string; label: string; where: string; met: boolean; reason: string }

/** The owner's checklist in its order; without the projection every condition is unknown. */
export function conditionRows(live: BscLiveView | null): ConditionRow[] {
  return (live?.preconditions ?? []).map(p => ({
    id: p.id, ...(CONDITION_TEXT[p.id] ?? { label: p.id, where: "" }), met: p.met, reason: p.reason,
  }));
}

/** The canonical scope text, byte for byte the owner's `LiveGrant::scope_text`. */
export function scopeText(s: LiveGrantScope): string {
  return "darkbot.bsc.live-grant.v1\n"
    + `venue=${s.venue}\nstrategies=${s.strategies}\nsources=${s.sources.join(",")}\nadapters=${s.adapters.join(",")}\n`
    + `max_input_wei=${s.maxInputWei}\nmax_spend_wei=${s.maxSpendWei}\nmax_attempts=${s.maxAttempts}\nexpires_unix_ms=${s.expiresUnixMs.toString()}\n`
    + (s.exitOnly ? "exit_only=true\n" : "");
}

/** SHA-256 of the scope text, lowercase hex: the digest the operator confirms. */
export async function scopeSha256(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return Array.from(new Uint8Array(digest), b => b.toString(16).padStart(2, "0")).join("");
}

const U128_MAX = (1n << 128n) - 1n;
/** A plain decimal wei amount within 128 bits (the owner's and the gateway's rule). */
export function weiOk(text: string): boolean {
  return /^(0|[1-9][0-9]{0,38})$/.test(text) && BigInt(text) <= U128_MAX;
}

/** The grant form as typed. */
export interface GrantForm {
  strategies: string; sources: string[]; adapters: string[];
  maxInputWei: string; maxSpendWei: string; maxAttempts: string; hours: string;
  exitOnly: boolean;
}

/** The scope of a grant form at `nowMs`, or why it is not one (the owner checks again). */
export function grantScope(f: GrantForm, nowMs: number): { scope: LiveGrantScope } | { error: string } {
  if (!(LIVE_STRATEGIES as readonly string[]).includes(f.strategies)) return { error: "strategies: C3, STRATEGY_C or C3+STRATEGY_C" };
  const unique = (xs: string[]) => new Set(xs).size === xs.length;
  if (!f.sources.length || !unique(f.sources) || !f.sources.every(s => (LIVE_SOURCES as readonly string[]).includes(s))) return { error: "at least one source, each once" };
  if (!f.adapters.length || !unique(f.adapters) || !f.adapters.every(a => (LIVE_ADAPTERS as readonly string[]).includes(a))) return { error: "at least one adapter, each once" };
  if (!weiOk(f.maxInputWei) || BigInt(f.maxInputWei) === 0n) return { error: "max input: a positive plain decimal in wei" };
  if (!weiOk(f.maxSpendWei) || BigInt(f.maxSpendWei) < BigInt(f.maxInputWei)) return { error: "max spend: a plain decimal in wei, at least the max input" };
  if (!/^([1-9]|10)$/.test(f.maxAttempts) || Number(f.maxAttempts) > MAX_ATTEMPTS_TOTAL) return { error: `attempts: 1..${MAX_ATTEMPTS_TOTAL}` };
  if (!/^[1-9][0-9]{0,3}$/.test(f.hours)) return { error: "validity: whole hours" };
  return {
    scope: {
      venue: LIVE_VENUES[0], strategies: f.strategies, sources: f.sources, adapters: f.adapters,
      maxInputWei: f.maxInputWei, maxSpendWei: f.maxSpendWei, maxAttempts: Number(f.maxAttempts),
      expiresUnixMs: BigInt(nowMs) + BigInt(f.hours) * 3_600_000n, exitOnly: f.exitOnly,
    },
  };
}

/** The total limit an operator may set: 0..10 (the owner checks again). */
export function totalInputOk(text: string): boolean {
  return /^([0-9]|10)$/.test(text);
}

/** The owner request that asks an entity of the scope for LIVE (its CAS is the owner generation). */
export function liveRequestOf(row: ControlSwitchView): { action: "bsc.runtime.mode.set" | "bsc.venue.mode.set" | "bsc.source.mode.set"; id: string } {
  if (row.id === "BSC_RUNTIME_MODE") return { action: "bsc.runtime.mode.set", id: row.id };
  if (row.id.startsWith("BSC_VENUE_")) return { action: "bsc.venue.mode.set", id: row.id };
  return { action: "bsc.source.mode.set", id: row.id };
}

/** A signed decimal wei amount as BNB with its sign. */
export function signedBnb(wei: string): string {
  if (!/^-?[0-9]+$/.test(wei)) return wei || "—";
  const negative = wei.startsWith("-");
  const digits = negative ? wei.slice(1) : wei;
  const whole = digits.length > 18 ? digits.slice(0, -18) : "0";
  const fraction = digits.padStart(18, "0").slice(-18).slice(0, 6).replace(/0+$/, "");
  return `${negative ? "−" : ""}${whole}${fraction ? `.${fraction}` : ""} BNB`;
}

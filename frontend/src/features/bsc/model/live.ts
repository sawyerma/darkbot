// The BSC LIVE tab (plan 28.10.): what the native owner's LIVE gate needs, each condition with
// the owner's own reason, and the canonical scope of a grant exactly as the owner computes it
// (its SHA-256 is what the operator confirms). Nothing here decides LIVE: the owner admits every
// intent again at its dispatch.
import type { LiveGrantScope } from "@/core/realtime/bscControl";
import type { BscLiveView, ControlSwitchView, LiveAttemptView, LiveCampaignView } from "./types";

export const LIVE_VENUES = ["BSC_VENUE_FLAP"] as const;
export const LIVE_STRATEGIES = ["C3", "STRATEGY_C", "C3+STRATEGY_C"] as const;
export const LIVE_ADAPTERS = ["CLUB48_BUILDER_V1", "BLOCKRAZOR_BUILDER_V1"] as const;
/** The campaign's limit: at most ten real BUY attempts (PHASENPLAN 28.10.); the owner checks again. */
export const MAX_ATTEMPTS_TOTAL = 10;

/** The one LIVE indicator (Audit E 4.10), from bsc.live.state alone: LIVE while the owner's gate
 *  is CLEAR and the runtime's LIVE path ARMED, UNKNOWN without the owner's state, else BLOCKED. */
export function liveIndicator(l: BscLiveView | null): { state: string; detail: string } {
  if (!l || l.gate === "UNKNOWN" || !l.gate) return { state: "UNKNOWN", detail: "bsc.live.state: no readable owner state" };
  const state = l.gate === "CLEAR" && l.pathState === "ARMED" ? "LIVE" : "BLOCKED";
  return { state, detail: `gate ${l.gate}${l.blockers[0] ? ` (first: ${l.blockers[0]})` : ""} · LIVE path ${l.pathState || "UNKNOWN"}` };
}

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
  GRANT_RISK: { label: "The grant names its execution risk and limits", where: "A new grant (slippage, gas price and limits; positions, losses, BUY interval)" },
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
    + `max_buy_slippage_bps=${s.risk.maxBuySlippageBps}\nmax_exit_slippage_bps=${s.risk.maxExitSlippageBps}\n`
    + `max_gas_price_wei=${s.risk.maxGasPriceWei.toString()}\nmax_buy_gas_limit=${s.risk.maxBuyGasLimit.toString()}\n`
    + `max_approve_gas_limit=${s.risk.maxApproveGasLimit.toString()}\nmax_sell_gas_limit=${s.risk.maxSellGasLimit.toString()}\n`
    + `max_open_positions=${s.limits.maxOpenPositions}\nmax_realized_loss_wei=${s.limits.maxRealizedLossWei}\n`
    + `max_daily_realized_loss_wei=${s.limits.maxDailyRealizedLossWei}\nmin_buy_interval_ms=${s.limits.minBuyIntervalMs.toString()}\n`
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

/** The grant form as typed. `sources` null: not chosen yet, the first grantable source. */
export interface GrantForm {
  strategies: string; sources: string[] | null; adapters: string[];
  maxInputWei: string; maxSpendWei: string; maxAttempts: string; hours: string;
  exitOnly: boolean;
  /** BSC3-069: the execution risk, as typed. */
  buySlippageBps: string; exitSlippageBps: string; gasPriceWei: string;
  buyGasLimit: string; approveGasLimit: string; sellGasLimit: string;
  /** 31.10. order: the positions and losses, as typed. */
  maxOpenPositions: string; maxRealizedLossWei: string; maxDailyRealizedLossWei: string; minBuyIntervalMs: string;
}

const U64_MAX = (1n << 64n) - 1n;
/** A plain decimal within 64 bits of at least `min`. */
function u64Ok(text: string, min: bigint): boolean {
  return /^(0|[1-9][0-9]{0,19})$/.test(text) && BigInt(text) >= min && BigInt(text) <= U64_MAX;
}
/** A slippage in basis points below 100 % (the owner's rule). */
function bpsOk(text: string): boolean {
  return /^(0|[1-9][0-9]{0,3})$/.test(text) && Number(text) < 10_000;
}
/** The least gas limit of a transaction (a plain transfer's). */
const MIN_GAS_LIMIT = 21_000n;

/** The sources a form names among `grantable` (bsc.live.state's grantable_sources, Audit E 1.6):
 *  the chosen ones, or before any choice the first grantable one. */
export function formSources(f: GrantForm, grantable: readonly string[]): string[] {
  return f.sources ?? grantable.slice(0, 1);
}

/** The scope of a grant form at `nowMs`, or why it is not one (the owner checks again). Only the
 *  native projection's grantable sources can be named. */
export function grantScope(f: GrantForm, nowMs: number, grantable: readonly string[]): { scope: LiveGrantScope } | { error: string } {
  if (!(LIVE_STRATEGIES as readonly string[]).includes(f.strategies)) return { error: "strategies: C3, STRATEGY_C or C3+STRATEGY_C" };
  const unique = (xs: string[]) => new Set(xs).size === xs.length;
  const sources = formSources(f, grantable);
  if (!grantable.length) return { error: "no source has an adapter endpoint in the runtime configuration" };
  if (!sources.length || !unique(sources) || !sources.every(s => grantable.includes(s))) return { error: "at least one grantable source, each once" };
  if (!f.adapters.length || !unique(f.adapters) || !f.adapters.every(a => (LIVE_ADAPTERS as readonly string[]).includes(a))) return { error: "at least one adapter, each once" };
  if (!weiOk(f.maxInputWei) || BigInt(f.maxInputWei) === 0n) return { error: "max input: a positive plain decimal in wei" };
  if (!weiOk(f.maxSpendWei) || BigInt(f.maxSpendWei) < BigInt(f.maxInputWei)) return { error: "max spend: a plain decimal in wei, at least the max input" };
  if (!/^([1-9]|10)$/.test(f.maxAttempts) || Number(f.maxAttempts) > MAX_ATTEMPTS_TOTAL) return { error: `attempts: 1..${MAX_ATTEMPTS_TOTAL}` };
  if (!/^[1-9][0-9]{0,3}$/.test(f.hours)) return { error: "validity: whole hours" };
  if (!bpsOk(f.buySlippageBps) || !bpsOk(f.exitSlippageBps)) return { error: "slippage: basis points below 10000" };
  if (!u64Ok(f.gasPriceWei, 1n)) return { error: "gas price: a positive plain decimal in wei" };
  if (![f.buyGasLimit, f.approveGasLimit, f.sellGasLimit].every(l => u64Ok(l, MIN_GAS_LIMIT))) return { error: `gas limits: at least ${MIN_GAS_LIMIT.toString()}` };
  if (!/^[1-9][0-9]{0,9}$/.test(f.maxOpenPositions) || Number(f.maxOpenPositions) > 4_294_967_295) return { error: "open positions: 1..4294967295" };
  if (!weiOk(f.maxRealizedLossWei) || BigInt(f.maxRealizedLossWei) === 0n) return { error: "realized loss: a positive plain decimal in wei" };
  if (!weiOk(f.maxDailyRealizedLossWei) || BigInt(f.maxDailyRealizedLossWei) === 0n || BigInt(f.maxDailyRealizedLossWei) > BigInt(f.maxRealizedLossWei)) return { error: "daily realized loss: a positive plain decimal in wei, within the grant's" };
  if (!u64Ok(f.minBuyIntervalMs, 0n)) return { error: "BUY interval: a plain decimal in milliseconds" };
  return {
    scope: {
      venue: LIVE_VENUES[0], strategies: f.strategies, sources, adapters: f.adapters,
      maxInputWei: f.maxInputWei, maxSpendWei: f.maxSpendWei, maxAttempts: Number(f.maxAttempts),
      expiresUnixMs: BigInt(nowMs) + BigInt(f.hours) * 3_600_000n, exitOnly: f.exitOnly,
      risk: {
        maxBuySlippageBps: Number(f.buySlippageBps), maxExitSlippageBps: Number(f.exitSlippageBps),
        maxGasPriceWei: BigInt(f.gasPriceWei), maxBuyGasLimit: BigInt(f.buyGasLimit),
        maxApproveGasLimit: BigInt(f.approveGasLimit), maxSellGasLimit: BigInt(f.sellGasLimit),
      },
      limits: {
        maxOpenPositions: Number(f.maxOpenPositions), maxRealizedLossWei: f.maxRealizedLossWei,
        maxDailyRealizedLossWei: f.maxDailyRealizedLossWei, minBuyIntervalMs: BigInt(f.minBuyIntervalMs),
      },
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

// ---- REV.1.0 R2b: the LIVE campaign verdict and its attempts (section 4) ----

/** Before `bsc_runtime live attempts` wrote a verdict there is no evaluation, never a value. */
export const NO_EVALUATION = "keine Bewertung";
/** What the verdict is: the campaign's evidence, never a release or a permission. */
export const CAMPAIGN_SCOPE = "Kampagnen-Evidenz allein (RELEASE_EVIDENCE_*): ein Release braucht zusätzlich Implementierungsmatrix, Runtime-Durchsetzung und LIVE-Freigabe";
/** The producer's T0 to T_buy is not measured locally: unknown unless a record says it is exact. */
export const T0_UNKNOWN = "T0→T_buy bleibt UNKNOWN ohne Producer-Witness";
/** Always true: the local times are the runtime's clock, never T0 to T_buy. */
export const LOCAL_NOT_T0 = "Die lokalen Zeiten sind die Uhr der Runtime (Ingress→Send), nie T0→T_buy";

export interface CountRow { label: string; value: string; hint: string }
const of = (n: bigint, d: bigint) => `${n.toString()}/${d.toString()}`;

/** The campaign's counts as the record holds them, every rate with its denominator. */
export function campaignRows(c: LiveCampaignView): CountRow[] {
  const w = c.written;
  return [
    { label: "Attempts written", value: `${of(w, c.planned)} planned`, hint: "C3 BUYs written after the commitment, of the enabled cells' minimum attempts" },
    { label: "Acquired", value: of(c.acquired, w), hint: "of the written attempts" },
    { label: "Passed · failed · undecided", value: `${of(c.passed, w)} · ${of(c.failed, w)} · ${of(c.undecided, w)}`, hint: "of the written attempts" },
    { label: "Reverted · dropped · late · unresolved", value: `${of(c.reverted, w)} · ${of(c.dropped, w)} · ${of(c.late, w)} · ${of(c.unresolved, w)}`, hint: "of the written attempts" },
    { label: "Selected launches", value: of(c.selected, c.observed), hint: "seen in full before their block, of the observed canonical C1-scope Flap launches" },
    { label: "Skipped launches", value: of(c.skipped, c.observed), hint: c.skipReasons.length ? c.skipReasons.join(" · ") : "of the observed launches" },
    { label: "Admitted by the owner", value: c.admitted.toString(), hint: "C3 BUYs the owner admitted, written and withdrawn" },
    { label: "Longest run of misses", value: c.consecutiveMisses.toString(), hint: "observed launches in a row without an attempt" },
    { label: "Before the commitment", value: c.beforePrecommit.toString(), hint: "C3 BUYs written before the manifest's commitment (not the campaign's)" },
    { label: "Unverifiable launches", value: c.unverifiable.toString(), hint: "launches whose transaction could not be verified (coverage unknown)" },
    { label: "Coverage", value: `${c.coveredS.toString()} s covered · ${c.uncoveredS.toString()} s not`, hint: `${c.coveredPpm.toString()} ppm of the campaign's span covered by sealed runs` },
  ];
}

/** ECMAScript's largest time value (ms). */
const MAX_DATE_MS = 8_640_000_000_000_000n;
/** A unix-ms time as ISO; beyond the date range its raw value (never a thrown error). */
export function unixMs(v: bigint | undefined): string {
  if (v === undefined) return "—";
  return v <= MAX_DATE_MS ? new Date(Number(v)).toISOString() : `${v.toString()} ms`;
}
/** What an attempt's counters count (REV.1.0 R2b review L8): counts, not rates of the attempts. */
export const COUNTERS_MEANING = "Je Versuch bis einschließlich ihm: acquired von allen LIVE-BUYs (C3 und Strategy C, auch vor dem Commitment); passed · failed · undecided zählen nur die Verdikte der C3-BUYs (ein Strategy-C-BUY hat keins, NOT_APPLICABLE)";
/** The campaign verdict record exists but is unreadable or not of its schema. */
export const VERDICT_INVALID = "LIVE_CAMPAIGN_VERDICT_INVALID";
/** The campaign section did not fit what the rest of the LIVE tab leaves of the state bound:
 *  none of it is shown, its records and warnings counted (REV.1.0 R2b re-check F3). */
export const CAMPAIGN_BEYOND_STATE = "LIVE_CAMPAIGN_BEYOND_STATE";
/** The reporter's warning that the campaign section is beyond the state, if any. */
export function campaignBeyondState(warnings: readonly string[]): string | undefined {
  return warnings.find(w => w.startsWith(`${CAMPAIGN_BEYOND_STATE} `));
}

/** The attempts table's columns, one per cell of {@link attemptCells}. */
export const ATTEMPT_COLUMNS = ["#", "Tx · attempt · run", "Kind · cell", "Token · launch · trigger", "Block", "State", "Verdict", "Components", "Tokens · cost",
  "Placement", "15-s window", "Price vs first external BUY", "Local timing", "Counters", "Adapters", "Reasons", "Evaluated"] as const;

/** The evaluation the attempts are of: the verdict's, without one the newest attempt's. */
export function evaluationOf(c: LiveCampaignView | undefined, attempts: readonly LiveAttemptView[]): bigint | undefined {
  return c?.evaluatedUnixMs ?? attempts.reduce<bigint | undefined>((m, a) => m === undefined || a.evaluatedUnixMs > m ? a.evaluatedUnixMs : m, undefined);
}

/** One attempt's row cells, as its record holds it ("—" where it names nothing); `evaluationMs`
 *  is {@link evaluationOf}, against which a record of another evaluation is marked. */
export function attemptCells(a: LiveAttemptView, evaluationMs?: bigint): string[] {
  const ns = (v: bigint | undefined) => v === undefined ? "—" : `${v.toString()} ns`;
  const n = (v: bigint | undefined) => v === undefined ? "—" : v.toString();
  const wei = (v: string | undefined) => v === undefined || v === "" ? "—" : `${v} wei`;
  const adapter = (x: LiveAttemptView["adapters"][number]) => `${x.builder || "UNKNOWN"} (adapter ${x.adapter.toString()}) ${x.outcome}${x.answer ? ` ${x.answer}` : ""} · ${x.boundary} ${x.qualified ? "qualified" : "not qualified"}`
    + ` · write-check ${ns(x.tWriteCheckNs)} · boundary ${ns(x.tBoundaryNs)} · queue ${ns(x.tQueueNs)} · answer ${ns(x.tAnswerNs)}`;
  return [
    a.ordinal.toString(),
    `${a.tx} · attempt ${a.attempt || "—"} · run ${n(a.run)}`,
    `${a.kind} · ${a.cell}`,
    `${a.token} · launch ${a.launchHash || "—"} · trigger ${a.trigger || "—"}`,
    a.blockNumber === undefined ? "not included" : `${a.blockNumber.toString()} · ${a.blockHash || "—"} · ${unixMs(a.blockMs)}`,
    a.state,
    a.verdict,
    `acquisition ${a.acquisition} · canonical ${a.canonical} · timing ${a.timing} · placement ${a.placement} · capture ${a.capture} · local ${a.local} · economics ${a.economics}`,
    `tokens ${a.tokens ?? "—"} · input ${wei(a.ethWei)} · gas ${wei(a.gasWei)}`,
    a.placed ? `${a.placed} · launch index ${n(a.launchIndex)} · own index ${n(a.ownIndex)} · ${n(a.buysBefore)} BUYs between` : "—",
    `${a.window}${a.windowWhy ? ` (${a.windowWhy})` : ""} · ${a.windowMs.toString()} ms from block ${n(a.launchBlock)} (${unixMs(a.launchMs)}) to ${n(a.windowEndBlock)}`
      + ` · trades ${a.trades.toString()} (buys ${a.buys.toString()} ${wei(a.buyWei)}, sells ${a.sells.toString()} ${wei(a.sellWei)})`,
    `own ${a.ownPrice ?? "—"} · first external ${a.firstExternalPrice ?? "—"} (wei per 10^18) · ${a.priceVsFirstExternalPpm === undefined ? "—" : `${a.priceVsFirstExternalPpm.toString()} ppm`}`
      + ` · first external BUY ${a.firstExternalBuyTx || "—"} (block ${n(a.firstExternalBuyBlock)}, index ${n(a.firstExternalBuyIndex)})`,
    `T0 ${a.t0Exact ? "exact (recorded)" : "UNKNOWN"} · clock ${n(a.clockId)} · ingress ${ns(a.tIngressNs)} · select ${ns(a.tSelectNs)} · send ${ns(a.tSendNs)} · ack ${ns(a.tAckNs)}`
      + ` · ingress→send ${ns(a.ingressToSendNs)} · select→send ${ns(a.selectToSendNs)} · send→ack ${ns(a.sendToAckNs)} · deadline misses ${n(a.deadlineMisses)}`,
    `acquired ${of(a.acquired, a.attempts)} LIVE BUYs · C3 verdicts so far: ${a.passed.toString()} passed · ${a.failed.toString()} failed · ${a.undecided.toString()} undecided`,
    a.adapters.length ? a.adapters.map(adapter).join(" · ") : "—",
    a.reasons.length ? a.reasons.join(" · ") : "—",
    `${unixMs(a.evaluatedUnixMs)}${evaluationMs !== undefined && evaluationMs !== a.evaluatedUnixMs ? " · of another evaluation" : ""}`,
  ];
}

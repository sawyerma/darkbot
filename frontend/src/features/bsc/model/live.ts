// The BSC LIVE tab (plan 28.10.): what the native owner's LIVE gate needs, each condition with
// the owner's own reason, and the canonical scope of a grant exactly as the owner computes it
// (its SHA-256 is what the operator confirms). Nothing here decides LIVE: the owner admits every
// intent again at its dispatch.
import type { LiveGrantScope } from "@/core/realtime/bscControl";
import type { BscLiveView, ControlSwitchView, LiveAttemptView, LiveCampaignView } from "./types";

/** Audit E 4.9: the grant's fixed choices as the native owner projects them (bsc.live.state's
 *  grant_venues/strategies/adapters); none without the projection, never a frontend list. */
export interface GrantChoices { venues: readonly string[]; strategies: readonly string[]; adapters: readonly string[] }
export function grantChoices(l: BscLiveView | null): GrantChoices {
  return { venues: l?.grantVenues ?? [], strategies: l?.grantStrategies ?? [], adapters: l?.grantAdapters ?? [] };
}

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
  BUY_LIMIT_TOTAL: { label: "Campaign BUY attempts left (at most BSC_BUY_ATTEMPTS_TOTAL_MAX)", where: "Attempts on this tab" },
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
    + (s.risk.outlierAck ? "risk_outlier_ack=YES\n" : "")
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

/** The grant form as typed. `strategies`, `sources`, `adapters` null: not chosen yet, the first
 *  the owner offers. */
export interface GrantForm {
  strategies: string | null; sources: string[] | null; adapters: string[] | null;
  maxInputWei: string; maxSpendWei: string; maxAttempts: string; hours: string;
  exitOnly: boolean;
  /** BSC3-069: the execution risk, as typed. */
  buySlippageBps: string; exitSlippageBps: string; gasPriceWei: string;
  buyGasLimit: string; approveGasLimit: string; sellGasLimit: string;
  /** REV.1.0 R4: the risk confirmed as an outlier (above the runtime's soft bounds). */
  outlierAck: boolean;
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
/** The strategy paths a form names, or before any choice the owner's first. */
export function formStrategies(f: GrantForm, c: GrantChoices): string {
  return f.strategies ?? c.strategies[0] ?? "";
}
/** The adapters a form names, or before any choice the owner's first. */
export function formAdapters(f: GrantForm, c: GrantChoices): string[] {
  return f.adapters ?? c.adapters.slice(0, 1);
}

/** The scope of a grant form at `nowMs`, or why it is not one (the owner checks again). Only the
 *  native projection's grantable sources can be named, at most `maxAttempts` attempts
 *  (bsc.live.state's buy_attempts_total_max, Audit E 3.1; none without it). */
export function grantScope(f: GrantForm, nowMs: number, grantable: readonly string[], maxAttempts: number | undefined, choices: GrantChoices): { scope: LiveGrantScope } | { error: string } {
  const venue = choices.venues[0];
  if (venue === undefined) return { error: "no venue the native owner can grant (bsc.live.state)" };
  const strategies = formStrategies(f, choices);
  if (!choices.strategies.includes(strategies)) return { error: `strategies: ${choices.strategies.join(", ") || "none projected"}` };
  const unique = (xs: string[]) => new Set(xs).size === xs.length;
  const sources = formSources(f, grantable);
  if (!grantable.length) return { error: "no source has an adapter endpoint in the runtime configuration" };
  if (!sources.length || !unique(sources) || !sources.every(s => grantable.includes(s))) return { error: "at least one grantable source, each once" };
  const adapters = formAdapters(f, choices);
  if (!adapters.length || !unique(adapters) || !adapters.every(a => choices.adapters.includes(a))) return { error: "at least one adapter, each once" };
  if (!weiOk(f.maxInputWei) || BigInt(f.maxInputWei) === 0n) return { error: "max input: a positive plain decimal in wei" };
  if (!weiOk(f.maxSpendWei) || BigInt(f.maxSpendWei) < BigInt(f.maxInputWei)) return { error: "max spend: a plain decimal in wei, at least the max input" };
  if (maxAttempts === undefined) return { error: "attempts: no maximum without a readable runtime configuration (BSC_BUY_ATTEMPTS_TOTAL_MAX)" };
  if (!/^[1-9][0-9]{0,9}$/.test(f.maxAttempts) || Number(f.maxAttempts) > maxAttempts) return { error: `attempts: 1..${maxAttempts}` };
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
      venue, strategies, sources, adapters,
      maxInputWei: f.maxInputWei, maxSpendWei: f.maxSpendWei, maxAttempts: Number(f.maxAttempts),
      expiresUnixMs: BigInt(nowMs) + BigInt(f.hours) * 3_600_000n, exitOnly: f.exitOnly,
      risk: {
        maxBuySlippageBps: Number(f.buySlippageBps), maxExitSlippageBps: Number(f.exitSlippageBps),
        maxGasPriceWei: BigInt(f.gasPriceWei), maxBuyGasLimit: BigInt(f.buyGasLimit),
        maxApproveGasLimit: BigInt(f.approveGasLimit), maxSellGasLimit: BigInt(f.sellGasLimit),
        outlierAck: f.outlierAck,
      },
      limits: {
        maxOpenPositions: Number(f.maxOpenPositions), maxRealizedLossWei: f.maxRealizedLossWei,
        maxDailyRealizedLossWei: f.maxDailyRealizedLossWei, minBuyIntervalMs: BigInt(f.minBuyIntervalMs),
      },
    },
  };
}

/** The total limit an operator may set: 0..`max` (buy_attempts_total_max; none without it; the
 *  owner checks again). */
export function totalInputOk(text: string, max: number | undefined): boolean {
  return max !== undefined && /^(0|[1-9][0-9]{0,9})$/.test(text) && Number(text) <= max;
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
export const MAX_DATE_MS = 8_640_000_000_000_000n;
/** A unix-ms time as ISO; beyond the date range its raw value (never a thrown error). */
export function unixMs(v: bigint | undefined): string {
  if (v === undefined) return "—";
  return v <= MAX_DATE_MS ? new Date(Number(v)).toISOString() : `${v.toString()} ms`;
}
/** REV.1.0 R4: the bound builder policy's line: why it refuses LIVE BUYs, else that it holds; until when it is fresh (none: unknown). */
export function builderPolicyText(l: BscLiveView | null): string | undefined {
  if (!l || (l.builderPolicyRefusal === undefined && l.builderPolicyFreshUntilUnixMs === undefined)) return undefined;
  const until = l.builderPolicyFreshUntilUnixMs === undefined ? "" : ` · fresh until ${unixMs(l.builderPolicyFreshUntilUnixMs)}`;
  return `${l.builderPolicyRefusal === undefined ? "holds" : `BUYs refused: ${l.builderPolicyRefusal}`}${until}`;
}
/** What an attempt's counters count (REV.1.0 R2b review L8): counts, not rates of the attempts. */
export const COUNTERS_MEANING = "Je Versuch bis einschließlich ihm: acquired von allen LIVE-BUYs (C3 und Strategy C, auch vor dem Commitment); passed · failed · undecided zählen nur die Verdikte der C3-BUYs (ein Strategy-C-BUY hat keins, NOT_APPLICABLE)";
/** One row of the LIVE tab: label, value and hint. */
export interface LiveRow { label: string; value: string; hint: string }

/** REV.1.0 R1: who decides the open lots' exits, as the runtime's status names them. */
export function exitDeciderRows(l: BscLiveView | null | undefined): LiveRow[] {
  const list = (v: readonly string[] | undefined) => v?.length ? v.join(" · ") : "—";
  return [
    { label: "Exit undecided", value: list(l?.exitUndecided), hint: "Open lots no strategy holds yet (their curve being read): no new BUY until each is held or stranded" },
    { label: "Stranded lots", value: list(l?.stranded), hint: "No strategy exit can take them (the reason named): the operator resumes, writes off or exits each" },
    { label: "Curve read faults", value: (l?.lotReadFaults ?? 0n).toString(), hint: l?.lotReadFault || "no failure" },
  ];
}

/** REV.1.0 R3b (BSC3-024/041): the runtime's receipt recovery of the ledger's unresolved
 *  transactions, and each one it has not resolved as `[tx, reason]`. */
export function recoveryRows(l: BscLiveView | null | undefined): { row: LiveRow; pending: [string, string][] } {
  const pending = (l?.recoveryPending ?? []).map(p => {
    const at = p.indexOf(" ");
    return (at < 0 ? [p, ""] : [p.slice(0, at), p.slice(at + 1)]) as [string, string];
  });
  return {
    row: {
      label: "Receipt recovery",
      value: `${(l?.recovered ?? 0n).toString()} resolved · ${pending.length} pending · ${(l?.recoveryFaults ?? 0n).toString()} failed reads`,
      hint: l?.recoveryFault || "Unresolved transactions the follower can no longer resolve, looked up off the loop: only a final block the second source confirms that holds the transaction (its receipt's) or its sender's at its nonce (where the nonce passes it)",
    },
    pending,
  };
}

/** R10 (Audit D): the positions a revoke leaves without an exit (pending, open or exiting). */
export function heldPositions(l: BscLiveView | null | undefined): number {
  return (l?.positions ?? []).filter(p => p.state === "PENDING" || p.state === "OPEN" || p.state === "EXITING").length;
}

/** REV.1.0 R10 (Audit D): the LIVE path as the runtime's status names it: the buyer's next nonce,
 *  the grant it is armed under, refusals by stage, probes and the release latency. */
export function pathRows(l: BscLiveView | null | undefined): LiveRow[] {
  const n = (v: bigint | undefined) => v === undefined ? "—" : v.toString();
  const ns = (v: bigint | undefined) => v === undefined ? "—" : `${v.toString()} ns`;
  return [
    { label: "Next nonce", value: n(l?.nextNonce), hint: "The buyer's next nonce at the LIVE path's last probe" },
    { label: "Armed under", value: l?.armedGeneration === undefined ? "—" : `grant generation ${l.armedGeneration.toString()}`, hint: "The owner generation of the grant the LIVE path is armed under" },
    { label: "Refusals by stage", value: l?.refusals.length ? l.refusals.join(" · ") : "—", hint: "Every refused intent, counted by the stage that refused it" },
    { label: "Probes", value: n(l?.probes), hint: "The LIVE path's read-only probes" },
    { label: "Release latency", value: l?.latencySamples ? `p50 ${ns(l.latencyP50Ns)} · p99 ${ns(l.latencyP99Ns)} · max ${ns(l.latencyMaxNs)} (${l.latencySamples.toString()} samples)` : "no sample",
      hint: "T_ingress → T_handoff of the LIVE releases: owner admitted, request written; no builder answer, no inclusion (BSC3-043)" },
  ];
}

/** R10: what the providers answered the LIVE releases and what C2's evidence did to its intents. */
export function outcomeRows(l: BscLiveView | null | undefined): LiveRow[] {
  const c = (v: bigint | undefined) => (v ?? 0n).toString();
  return [
    { label: "Provider answers", value: l?.responses.length ? l.responses.join(" · ") : "—", hint: "OUTCOME=n per release answer (BSC3-051)" },
    { label: "Chain evidence", value: `${c(l?.included)} included · ${c(l?.reverted)} reverted · ${c(l?.replaced)} replaced · ${c(l?.finalized)} final · ${c(l?.orphaned)} orphaned`, hint: "C2's evidence on the LIVE path's intents" },
  ];
}

/** R10 (R7, BSC3-056): the second source's verdict and the divergence quarantine (none: unknown). */
export function divergenceText(l: BscLiveView | null | undefined): string | undefined {
  if (!l || (l.divergenceQuarantine === undefined && !l.confirmState)) return undefined;
  const q = l.divergenceQuarantine === undefined ? "unknown" : l.divergenceQuarantine ? "ON · every send refused, exits too, while it DISAGREES" : "OFF · exits go on while it DISAGREES";
  return `second source ${l.confirmState || "—"} · quarantine ${q}`;
}

/** R10 (R9, BSC3-001): the presence anchor and who signed the grant's presence (none: unknown). */
export function presenceText(l: BscLiveView | null | undefined): string | undefined {
  if (!l || l.presenceAnchor === undefined) return undefined;
  if (l.presenceAnchor === "") return "OFF · BUYs need no presence signature";
  const signer = l.presenceSigner.toLowerCase();
  const signed = !l.grant ? "no grant" : signer === "" ? "the grant is unsigned: BUYs refused (LIVE_PRESENCE_UNSIGNED), exits go on"
    : signer === l.presenceAnchor.toLowerCase() ? "the grant is signed by it" : `the grant is signed by ${l.presenceSigner}, not the anchor: BUYs refused`;
  return `anchor ${l.presenceAnchor} · ${signed}`;
}

/** R10 (R8, BSC3-049): the qualification's age and the grant risk scope its risk decision names. */
export function qualificationAgeText(l: BscLiveView | null | undefined): string | undefined {
  if (!l || l.qualification !== "VERIFIED") return undefined;
  const bound = l.qualificationMaxAgeMs === undefined ? "max age unknown" : l.qualificationMaxAgeMs === 0n ? "no max age" : `max age ${l.qualificationMaxAgeMs.toString()} ms`;
  const scope = !l.qualificationRiskScopeSha256 ? "risk decision names no grant risk scope"
    : !l.grant ? `risk decision for ${l.qualificationRiskScopeSha256}`
      : l.qualificationRiskScopeSha256 === l.grant.riskScopeSha256 ? "risk decision for this grant's risk scope" : "risk decision for another grant risk scope";
  return `oldest evidence ${unixMs(l.qualificationObservedUnixMs)} · ${bound} · ${scope}`;
}

/** REV.1.0 R3b (Audit A): whether a switch locks a decision, and why (unknown locks as OFF does).
 *  Which lots and transactions take a decision is the ledger's own rule (`decisions`, `burnable`). */
export interface DecisionLock { locked: boolean; why: string }
/** The operator's decisions as the LIVE tab offers them: EXIT_NOW and the nonce burn locked unless
 *  their switch of the runtime configuration is ON; each at the ledger's version shown. */
export function operatorDecisions(l: BscLiveView | null | undefined): { exitNow: DecisionLock; burn: DecisionLock; version?: bigint; row: LiveRow } {
  const lock = (on: boolean | undefined, key: string): DecisionLock => on === true ? { locked: false, why: `${key} ON` }
    : { locked: true, why: on === false ? `gesperrt: ${key} ist OFF` : `gesperrt: ${key} unbekannt (keine lesbare Runtime-Konfiguration mit Executor)` };
  const exitNow = lock(l?.operatorExit, "BSC_EXEC_LIVE_OPERATOR_EXIT");
  const burn = lock(l?.nonceBurn, "BSC_EXEC_LIVE_NONCE_BURN");
  return {
    exitNow, burn, version: l?.ledgerVersion,
    row: {
      label: "Operator decisions",
      value: `ledger version ${l?.ledgerVersion?.toString() ?? "— (unknown)"} · exit-now ${exitNow.locked ? "locked" : "ON"} · nonce burn ${burn.locked ? "locked" : "ON"}`,
      hint: "Resume, write off or exit an open lot, burn an unresolved transaction's nonce: each at the ledger's version shown (a newer fact refuses it, LIVE_REQUEST_STALE)",
    },
  };
}

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

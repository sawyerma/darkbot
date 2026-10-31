import { ECON_SWITCHES, WALLET_PREFIX } from "@/core/realtime/analyticsControl";
import type { FinanceSnapshot, MonitorSnapshot, ProgramPinView } from "./contracts";

export { ECON_SWITCHES, WALLET_PREFIX };

/**
 * Views of the Shadow/econ4h state owner (topics project0.liquidator.state, .wallets.state,
 * .finance.summary). Values are what the Rust owner published; unknown stays null, never 0.
 */
export interface CapabilityView {
  name: string;
  requested: string;
  effective: string;
  blockReason: string;
  /** P8-CTRL.1: a signer's validated roles (WALLET:<pubkey> rows only). */
  roles: readonly string[];
}

/** P8-CTRL.1: an economic switch or a signer — shown and changed in their own rows. */
export function isEconomic(name: string): boolean {
  return (ECON_SWITCHES as readonly string[]).includes(name) || name.startsWith(WALLET_PREFIX);
}

export interface OwnerEngineView {
  runState: string;
  haltReason: string;
  ingressGeneration: bigint;
  reconnects: bigint;
  gapOpen: boolean;
  highestSlot: bigint;
  ingressHaltRaised: boolean;
  programUpgradeLatched: boolean;
  /** StateCore write counters by proto field name. */
  counters: ReadonlyMap<string, bigint>;
  /** §6: null before the owner read its ProgramData. */
  programPin: ProgramPinView | null;
}

/** §6 (CTRL-04/12): the Program-Data-Pin as an honest status — REFERENCE only for the qualified
 * deployment without a latched upgrade, BLOCKED otherwise (both identities named), UNKNOWN
 * without a current owner. */
export function programPinStatus(
  pin: ProgramPinView | null,
  latched: boolean | null,
): { status: "REFERENCE" | "BLOCKED" | "UNKNOWN"; detail: string } {
  if (!pin || latched == null) return { status: "UNKNOWN", detail: "No current owner has read its ProgramData" };
  const short = (h: string) => (h.length > 12 ? `${h.slice(0, 6)}…${h.slice(-4)}` : h);
  const observed = `observed ${short(pin.observedHash)} · slot ${pin.deploymentSlot}`;
  if (pin.verdict === "REFERENCE" && !latched) return { status: "REFERENCE", detail: `${pin.referenceSourceTag} · ${observed}` };
  return {
    status: "BLOCKED",
    detail: `${pin.verdict}${latched ? " · upgrade latched" : ""} · ${observed} · qualified ${pin.referenceSourceTag} ${short(pin.referenceHash)} · build ${pin.sourceTag}`,
  };
}

export interface OwnerTriggerView {
  mode: string;
  thresholdReady: boolean;
  lanes: readonly [bigint, bigint, bigint];
  detectQueue: bigint;
  indexedAccounts: bigint;
  suppressed: bigint;
  softcapSuppressed: bigint;
  /** DEC-01: early refusals (Build · BLOCKHASH_STALE), parked now, re-armed, superseded, refresh wake-ups. */
  blockhashRefused: bigint;
  blockhashParked: bigint;
  blockhashRearmed: bigint;
  blockhashSuperseded: bigint;
  blockhashRefreshRequests: bigint;
  /** DEC-06: the threshold mode; null from an owner before DEC-06. */
  threshold: OwnerThresholdView | null;
}

/** DEC-06: a latency distribution in ns; percentiles null without a sample (never 0). */
export interface LatencyView {
  count: bigint;
  p50: bigint | null;
  p95: bigint | null;
  p99: bigint | null;
  max: bigint | null;
}

/**
 * DEC-06: the threshold mode — requested (OFF|OBSERVE|ACTIVE, with its source), the owner's
 * effective mode (OFF|SWEEP|OBSERVE|ACTIVE), its block reason and generation; OBSERVE's comparison
 * with the sweep on the same ticks (hits, misses, extra wake-ups, detection times) and the cost of
 * both paths; ACTIVE's sweep cross-check. ACTIVE is no LIVE.
 */
export interface OwnerThresholdView {
  requested: string;
  requestedSource: string;
  effective: string;
  blockReason: string;
  generation: bigint;
  /** N (B-8); null while the user has not decided it. */
  minTicks: bigint | null;
  ticks: bigint;
  streak: bigint;
  uncomparableTicks: bigint;
  hits: bigint;
  misses: bigint;
  extra: bigint;
  sampledOut: bigint;
  fallbacks: bigint;
  indexTracked: bigint;
  indexFallback: bigint;
  indexBootstrapPending: bigint;
  indexEvaluations: bigint;
  crosscheckSamples: bigint;
  crosscheckSkipped: bigint;
  crosscheckStale: bigint;
  crosscheckMisses: bigint;
  sweepDetect: LatencyView;
  thresholdDetect: LatencyView;
  observeOwner: LatencyView;
  observeOwnerNsTotal: bigint;
  observeBudgetPpm: bigint;
  sweepHealthEvaluations: bigint;
  sweepOwnerNsTotal: bigint;
}

export interface OwnerPipelineView {
  evaluations: bigint;
  healthChecked: bigint;
  liquidatable: bigint;
  plansBuilt: bigint;
  plansRejected: bigint;
  simulationsPassed: bigint;
  simulationsFailed: bigint;
  shadowReady: bigint;
  shadowWins: bigint;
  losses: ReadonlyArray<{ reason: string; count: bigint }>;
  decisionP95Ns: bigint | null;
  feeQuoteP95Ns: bigint | null;
  simulationP95Ns: bigint | null;
}

export interface OwnerSafetyView {
  capitalInUseUsdMicro: string;
  maxTotalCapitalUsdMicro: string;
  maxDailyLossUsdMicro: string;
  maxExposurePerAttemptUsdMicro: string;
  maxConcurrentPositions: number;
  maxOutstandingAttempts: number;
  maxFailedAttemptsPerDay: number;
  haltReason: string;
  dayRealizedUsdMicro: string | null;
  dayFailedAttempts: number;
  dayAttempts: number;
}

/** B01: the owner's rebalancing plan in OBSERVE (planned, never traded). */
export interface RebalanceView {
  mode: string;
  /** null before the first plan. */
  atUnixMs: bigint | null;
  slot: bigint;
  trades: ReadonlyArray<{ sell: string; buy: string; amountIn: bigint; minimumOut: bigint; costUsdMicro: bigint }>;
  blocked: ReadonlyArray<{ asset: string; reason: string }>;
  quotesPending: bigint;
  budgetUsdMicro: bigint;
}

export interface LiquidatorView {
  runId: string;
  mode: string;
  publishedUnixMs: bigint;
  wallet: string;
  /** Empty while the owner runs; at its end RUN_COMPLETE or the terminal code. */
  endResult: string;
  engine: OwnerEngineView | null;
  trigger: OwnerTriggerView | null;
  pipeline: OwnerPipelineView | null;
  safety: OwnerSafetyView | null;
  capabilities: readonly CapabilityView[];
  /** null while rebalancing is OFF. */
  rebalance: RebalanceView | null;
  /** S1-02: every controlled data source with its applied control (the owner's arbiter). */
  sources: ReadonlyArray<{ unit: string; role: string; mode: string; qualification: string; generation: bigint; canonical: boolean }>;
  lastSourceDecision: { seq: bigint; request: string; applied: boolean; detail: string } | null;
  sourceStats: { observed: bigint; rejected: bigint; applied: bigint; refused: bigint } | null;
  /** CTRL-03: the source configuration version decided up to — the CAS a source change states;
   * null without source control. */
  sourceConfigVersion: bigint | null;
  /** DEC-10: the configured FE-04 approval, secret-free (never a LIVE authorization). */
  approval: ApprovalView | null;
  /** DEC-13: the capability configuration version decided up to — the CAS a capability request
   * states; null without capability control. */
  capabilityConfigVersion: bigint | null;
  /** DEC-13: the owner's last capability-control decision. */
  lastCapabilityDecision: { seq: bigint; request: string; applied: boolean; detail: string } | null;
  /** DEC-13b/DEC-02: the owner's FE-04 admission of LIVE candidates and exits (never LIVE itself:
   * that needs FE-06). */
  admission: AdmissionView | null;
  /** P8-CTRL.6: the owner epoch an economic request states (a new one at every owner start);
   * null without capability control. */
  capabilityEpoch: bigint | null;
  /** P8-CTRL.3/.5: null from an owner before P8-CTRL. */
  fe06: Fe06View | null;
}

/** DEC-13b/DEC-02: the owner's FE-04 admission state and this run's counters. */
export interface AdmissionView {
  /** OFF (LIVE not effective) | WAITING (no LIVE candidate yet) | READY | BLOCKED. */
  state: string;
  blockReason: string;
  submitted: bigint;
  refused: bigint;
  unknown: bigint;
  lastRefusal: string;
  simulationReused: bigint;
  simulationResimulated: bigint;
  simulationRefused: bigint;
  lastReuseRefusal: string;
  exitsSubmitted: bigint;
  exitsRefused: bigint;
  exitsUnknown: bigint;
  lastExitRefusal: string;
}

/** DEC-10: status of the FE-04 approval file the runtime is configured with. */
export interface ApprovalView {
  /** NONE | VALID | EXPIRED | CONSUMED | BLOCKED | INVALID | BINDING_MISMATCH. */
  state: string;
  reason: string;
  kind: string;
  approvalId: string;
  fundingClass: string;
  expiresUnixS: bigint;
  maxCapitalUsdMicro: bigint;
  maxLossUsdMicro: bigint;
  minNetUsdMicro: bigint;
  maxSlippageBps: bigint;
  allowedTargets: bigint;
}

export interface WalletsView {
  financeSnapshotId: string;
  wallet: string;
  journalRead: boolean;
  verdict: string;
  unknownReasons: readonly string[];
  /** Unknown (null) unless the journal was read. */
  attempts: bigint | null;
  landedAttempts: bigint | null;
  settledExits: bigint | null;
  feeLamports: bigint | null;
  openInventoryMarkUsdMicro: string | null;
  holds: ReadonlyArray<{ asset: string; units: bigint }>;
  policyReserves: ReadonlyArray<{ asset: string; units: bigint }>;
  heldAttempt: string;
  /** DEC-03: null without a held FE-04 journal (no attempt, nothing to classify). */
  nonLanding: NonLandingView | null;
  /** DEC-04: null without an owner projection of the B hold mode. */
  holdB: HoldBView | null;
}

/** DEC-04 (V7): the B hold mode, its evidence, Safety's reserved inventory and capital, and the
 * held attempt's A, B and finalized flows. */
export interface HoldBView {
  requested: string;
  effective: string;
  blockReason: string;
  /** The user's N (B-8); null while undecided. */
  n: bigint | null;
  evidenceExact: bigint;
  evidenceUnderestimated: bigint;
  evidenceUnqualified: bigint;
  inventoryReservedUsdMicro: bigint;
  capitalInUseUsdMicro: bigint;
  attempt: {
    mode: string;
    aWallet: ReadonlyArray<{ asset: string; units: bigint }>;
    aInventoryUsdMicro: bigint;
    reservedInventoryUsdMicro: bigint;
    bClass: string;
    bReason: string;
    bKind: string;
    bInventoryUsdMicro: bigint | null;
    flows: ReadonlyArray<HoldFlowView>;
    aFlows: ReadonlyArray<HoldFlowView>;
    settled: boolean;
    actualInventoryUsdMicro: bigint | null;
  } | null;
}

/** DEC-04: one asset of a prediction against the finalized delta (`residualMax` null: no bound in
 * units; `actual` null until settled or beyond int64). */
export interface HoldFlowView {
  asset: string;
  reserve: bigint;
  residualMax: bigint | null;
  actual: bigint | null;
  underestimated: boolean;
}

/** DEC-03 (N-2): the non-landing status of the held FE-04 attempt or its latest exit. */
export interface NonLandingView {
  /** ATTEMPT | EXIT. */
  subject: string;
  /** One of the five N-2 classes; NONE before the handoff. */
  resultClass: string;
  /** An open contradiction: holds stay, read again. */
  alarm: boolean;
  /** The latest read's B sources (operator, USABLE | UNUSABLE_<reason> | SIGHTING). */
  sources: ReadonlyArray<{ operator: string; class: string }>;
  /** The latest read's C result; empty without a read. */
  cResult: string;
  /** Unknown (null) without a hold or a read in this run. */
  holdMs: bigint | null;
  calls: bigint | null;
  bytes: bigint | null;
  reads: bigint;
}

/** The five N-2 result classes as the Wallets view names them. */
export const NON_LANDING_CLASSES: Readonly<Record<string, string>> = {
  B_C_CONFIRMED: "B+C confirmed",
  C_ALONE_QUALIFIED: "C alone qualified",
  VETO_LANDING_CONFIRMED: "Veto · landing confirmed",
  VETO_CONTRADICTION_OPEN: "Veto · contradiction open",
  UNKNOWN_INCOMPLETE: "UNKNOWN · incomplete",
};

/** Unusable B sources counted per reason (`UNUSABLE_<reason>`), in reason order. */
export function unusableByReason(sources: NonLandingView["sources"]): ReadonlyArray<{ reason: string; count: number }> {
  const counts = new Map<string, number>();
  for (const s of sources) {
    if (!s.class.startsWith("UNUSABLE_")) continue;
    const reason = s.class.slice("UNUSABLE_".length);
    counts.set(reason, (counts.get(reason) ?? 0) + 1);
  }
  return [...counts].sort(([a], [b]) => a.localeCompare(b)).map(([reason, count]) => ({ reason, count }));
}

/** Nanoseconds to whole milliseconds for the existing latency slots; null stays null. */
export function nsToMs(ns: bigint | null | undefined): number | null {
  return ns == null ? null : Number(ns / 1_000_000n);
}

/** The existing Overview monitor slots from the owner projection: RPC p95 is the slower of the
 * fee-quote and simulation p95 (conservative); without a sample it stays null. */
export function ownerMonitor(
  owner: LiquidatorView | null,
  finance: FinanceSnapshot | null,
): MonitorSnapshot {
  const p = owner?.pipeline ?? null;
  const rpc = [nsToMs(p?.feeQuoteP95Ns), nsToMs(p?.simulationP95Ns)].filter((v): v is number => v != null);
  return {
    rpcP95Ms: rpc.length ? Math.max(...rpc) : null,
    decisionP95Ms: nsToMs(p?.decisionP95Ns),
    streamAgeMs: null,
    finance,
    programPin: owner?.engine?.programPin ?? null,
    programUpgradeLatched: owner?.engine?.programUpgradeLatched ?? null,
  };
}

/** DEC-13b/DEC-02: one line of the admission state and this run's attempts, handoff simulations
 * and exits (the Liquidator tab). */
export function admissionLine(a: AdmissionView): string {
  const last = (reason: string) => (reason ? ` (last: ${reason})` : "");
  return `${a.state}${a.blockReason ? ` · ${a.blockReason}` : ""}`
    + ` · attempts ${a.submitted} submitted / ${a.refused} refused / ${a.unknown} unknown${last(a.lastRefusal)}`
    + ` · simulations ${a.simulationReused} reused / ${a.simulationResimulated} again / ${a.simulationRefused} refused${last(a.lastReuseRefusal)}`
    + ` · exits ${a.exitsSubmitted} submitted / ${a.exitsRefused} refused / ${a.exitsUnknown} unknown${last(a.lastExitRefusal)}`;
}

/** P8-CTRL.3/.5: the FE-06 grant in force (none: `grantDigest` null and the reason), its scope's
 * mode, validity, campaign and attempt limits, the attempts booked against them (null: the FE-04
 * journal unreadable or no clock), and the scopes previewed for this owner epoch. */
export interface Fe06View {
  grantDigest: string | null;
  scopeMode: string;
  validFromUnixS: bigint | null;
  validUntilUnixS: bigint | null;
  inForce: boolean;
  campaignId: string;
  maxAttemptsPerUtcDay: bigint;
  maxAttemptsCampaign: bigint;
  maxAttemptsGrant: bigint;
  attemptsToday: bigint | null;
  attemptsCampaign: bigint | null;
  attemptsGrant: bigint | null;
  blockReason: string;
  previews: ReadonlyArray<{ digest: string; scopeMode: string; ownerEpoch: bigint; validUntilUnixS: bigint; terminal: string }>;
}

/** P8-CTRL.8: why an economic request cannot be sent from this view (empty: it can). Never from
 * a state that is not current, and never without the version and the epoch the owner checks. */
export function economicBlock(s: { busy: boolean; live: boolean; version: bigint | null; epoch: bigint | null }): string {
  if (s.busy) return "Command in progress";
  if (!s.live) return "Runtime owner not current";
  if (s.version == null) return "Capability configuration version unknown (no capability control in this runtime)";
  if (s.epoch == null) return "Owner epoch unknown (an economic request states it)";
  return "";
}

/** P8-CTRL.2/.8: the LIVE checklist from the owner's projection — each gate the owner reports
 * with what it says. The checklist decides nothing: LIVE is what the owner made effective. */
export function liveGates(o: LiquidatorView): ReadonlyArray<{ gate: string; met: boolean; detail: string }> {
  const live = o.capabilities.find((c) => c.name === "LIVE") ?? null;
  const signers = o.capabilities.filter((c) => c.name.startsWith(WALLET_PREFIX));
  const enabled = signers.filter((c) => c.effective === "ENABLED").length;
  const pin = programPinStatus(o.engine?.programPin ?? null, o.engine?.programUpgradeLatched ?? null);
  return [
    { gate: "LIVE requested", met: live?.requested === "LIVE", detail: live ? `requested ${live.requested}` : "not reported" },
    ...ECON_SWITCHES.map((n) => {
      const c = o.capabilities.find((x) => x.name === n);
      return { gate: n, met: c?.effective === "ENABLED", detail: c?.effective ?? "not reported" };
    }),
    { gate: "Signers", met: signers.length > 0 && enabled === signers.length, detail: signers.length ? `${enabled} of ${signers.length} ENABLED` : "no configured signer" },
    { gate: "FE-06 grant", met: o.fe06?.inForce === true, detail: o.fe06 ? (o.fe06.grantDigest ?? o.fe06.blockReason) : "not reported" },
    { gate: "FE-04 approval", met: o.approval?.state === "VALID", detail: o.approval ? `${o.approval.state}${o.approval.reason ? ` (${o.approval.reason})` : ""}` : "not reported" },
    { gate: "Program pin", met: pin.status === "REFERENCE", detail: pin.status },
    { gate: "Safety", met: o.safety != null && o.safety.haltReason === "", detail: o.safety ? (o.safety.haltReason || "no halt") : "not reported" },
  ];
}

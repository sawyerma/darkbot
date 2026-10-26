import type { FinanceSnapshot, MonitorSnapshot, ProgramPinView } from "./contracts";

/**
 * Views of the Shadow/econ4h state owner (topics project0.liquidator.state, .wallets.state,
 * .finance.summary). Values are what the Rust owner published; unknown stays null, never 0.
 */
export interface CapabilityView {
  name: string;
  requested: string;
  effective: string;
  blockReason: string;
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

import type { FinanceSnapshot, MonitorSnapshot } from "./contracts";

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
}

export interface OwnerTriggerView {
  mode: string;
  thresholdReady: boolean;
  lanes: readonly [bigint, bigint, bigint];
  detectQueue: bigint;
  indexedAccounts: bigint;
  suppressed: bigint;
  softcapSuppressed: bigint;
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
  };
}

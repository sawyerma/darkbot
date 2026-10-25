import { fromBinary } from "@bufbuild/protobuf";
import {
  FinanceSummarySchema,
  LiquidatorStateSchema,
  StateCountersSchema,
  WalletsStateSchema,
  type FinanceSummary,
  type LiquidatorState,
  type WalletsState,
} from "@/core/generated/darkbot/project0/v1/owner_pb";
import type { FinanceSnapshot } from "../model/contracts";
import type { LiquidatorView, WalletsView } from "../model/owner";

/** Throws on invalid bytes, so the registry never acknowledges a broken state. */
export function decodeLiquidatorState(bytes: Uint8Array): LiquidatorState {
  return fromBinary(LiquidatorStateSchema, bytes);
}
export function decodeWalletsState(bytes: Uint8Array): WalletsState {
  return fromBinary(WalletsStateSchema, bytes);
}
export function decodeFinanceSummary(bytes: Uint8Array): FinanceSummary {
  return fromBinary(FinanceSummarySchema, bytes);
}

const decimal = (v: bigint | undefined): string | null => (v === undefined ? null : v.toString());
const optional = (v: bigint | undefined): bigint | null => (v === undefined ? null : v);

export function toLiquidatorView(m: LiquidatorState): LiquidatorView {
  const { run, engine, trigger, pipeline, safety } = m;
  const counters = new Map<string, bigint>();
  if (engine?.counters) {
    const values = engine.counters as unknown as Record<string, bigint>;
    for (const f of StateCountersSchema.fields) counters.set(f.name, values[f.localName] ?? 0n);
  }
  return {
    runId: run?.runId ?? "",
    mode: run?.mode ?? "",
    publishedUnixMs: run?.publishedUnixMs ?? 0n,
    wallet: run?.wallet ?? "",
    endResult: run?.endResult ?? "",
    engine: engine
      ? {
          runState: engine.runState,
          haltReason: engine.haltReason,
          ingressGeneration: engine.ingressGeneration,
          reconnects: engine.reconnects,
          gapOpen: engine.gapOpen,
          highestSlot: engine.highestSlot,
          ingressHaltRaised: engine.ingressHaltRaised,
          programUpgradeLatched: engine.programUpgradeLatched,
          counters,
        }
      : null,
    trigger: trigger
      ? {
          mode: trigger.mode,
          thresholdReady: trigger.thresholdReady,
          lanes: [trigger.laneL0, trigger.laneL1, trigger.laneL2],
          detectQueue: trigger.detectQueue,
          indexedAccounts: trigger.indexedAccounts,
          suppressed: trigger.suppressed,
          softcapSuppressed: trigger.softcapSuppressed,
        }
      : null,
    pipeline: pipeline
      ? {
          evaluations: pipeline.evaluations,
          healthChecked: pipeline.healthChecked,
          liquidatable: pipeline.liquidatable,
          plansBuilt: pipeline.plansBuilt,
          plansRejected: pipeline.plansRejected,
          simulationsPassed: pipeline.simulationsPassed,
          simulationsFailed: pipeline.simulationsFailed,
          shadowReady: pipeline.shadowReady,
          shadowWins: pipeline.shadowWins,
          losses: pipeline.losses.map((l) => ({ reason: l.reason, count: l.count })),
          decisionP95Ns: optional(pipeline.decisionP95Ns),
          feeQuoteP95Ns: optional(pipeline.feeQuoteP95Ns),
          simulationP95Ns: optional(pipeline.simulationP95Ns),
        }
      : null,
    safety: safety
      ? {
          capitalInUseUsdMicro: safety.capitalInUseUsdMicro.toString(),
          maxTotalCapitalUsdMicro: safety.maxTotalCapitalUsdMicro.toString(),
          maxDailyLossUsdMicro: safety.maxDailyLossUsdMicro.toString(),
          maxExposurePerAttemptUsdMicro: safety.maxExposurePerAttemptUsdMicro.toString(),
          maxConcurrentPositions: safety.maxConcurrentPositions,
          maxOutstandingAttempts: safety.maxOutstandingAttempts,
          maxFailedAttemptsPerDay: safety.maxFailedAttemptsPerDay,
          haltReason: safety.haltReason,
          dayRealizedUsdMicro: decimal(safety.dayRealizedUsdMicro),
          dayFailedAttempts: safety.dayFailedAttempts,
          dayAttempts: safety.dayAttempts,
        }
      : null,
    capabilities: m.capabilities.map((c) => ({
      name: c.name,
      requested: c.requested,
      effective: c.effective,
      blockReason: c.blockReason,
    })),
    rebalance: m.rebalance
      ? {
          mode: m.rebalance.mode,
          atUnixMs: m.rebalance.atUnixMs ?? null,
          slot: m.rebalance.slot,
          trades: m.rebalance.trades.map((t) => ({ sell: t.sell, buy: t.buy, amountIn: t.amountIn, minimumOut: t.minimumOut, costUsdMicro: t.costUsdMicro })),
          blocked: m.rebalance.blocked.map((b) => ({ asset: b.asset, reason: b.reason })),
          quotesPending: m.rebalance.quotesPending,
          budgetUsdMicro: m.rebalance.budgetUsdMicro,
        }
      : null,
  };
}

export function toWalletsView(m: WalletsState): WalletsView {
  const amounts = (xs: WalletsState["holds"]) => xs.map((a) => ({ asset: a.asset, units: a.units }));
  return {
    financeSnapshotId: m.financeSnapshotId,
    wallet: m.wallet,
    journalRead: m.journalRead,
    verdict: m.verdict,
    unknownReasons: [...m.unknownReasons],
    attempts: optional(m.attempts),
    landedAttempts: optional(m.landedAttempts),
    settledExits: optional(m.settledExits),
    feeLamports: optional(m.feeLamports),
    openInventoryMarkUsdMicro: decimal(m.openInventoryMarkUsdMicro),
    holds: amounts(m.holds),
    policyReserves: amounts(m.policyReserves),
    heldAttempt: m.heldAttempt,
  };
}

const STATUSES: ReadonlyArray<FinanceSnapshot["status"]> = ["VERIFIED", "PARTIAL", "UNKNOWN"];

/** The finance summary in the existing `FinanceSnapshot` contract of Overview and Wallets. */
export function toFinanceSnapshot(m: FinanceSummary): FinanceSnapshot {
  const status = STATUSES.find((s) => s === m.status) ?? "UNKNOWN";
  return {
    snapshotId: m.financeSnapshotId,
    asOfSlot: m.asOfSlot.toString(),
    totalCapitalUsdMicro: decimal(m.totalCapitalUsdMicro),
    availableCapitalUsdMicro: decimal(m.availableCapitalUsdMicro),
    reservedCapitalUsdMicro: decimal(m.reservedCapitalUsdMicro),
    realizedNetPnlUsdMicro: decimal(m.realizedNetPnlUsdMicro),
    withdrawableProfitUsdMicro: decimal(m.withdrawableProfitUsdMicro),
    transferredToTreasuryUsdMicro: decimal(m.transferredToTreasuryUsdMicro),
    status,
  };
}

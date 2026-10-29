import { fromBinary } from "@bufbuild/protobuf";
import {
  FinanceSummarySchema,
  LiquidatorStateSchema,
  StateCountersSchema,
  WalletsStateSchema,
  type FinanceSummary,
  type LatencyNs,
  type LiquidatorState,
  type WalletsState,
} from "@/core/generated/darkbot/project0/v1/owner_pb";
import type { FinanceSnapshot } from "../model/contracts";
import type { LatencyView, LiquidatorView, WalletsView } from "../model/owner";

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
const latency = (l: LatencyNs | undefined): LatencyView => ({
  count: l?.count ?? 0n, p50: optional(l?.p50), p95: optional(l?.p95), p99: optional(l?.p99), max: optional(l?.max),
});

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
          programPin: engine.programPin
            ? {
                verdict: engine.programPin.verdict,
                observedHash: engine.programPin.observedHash,
                deploymentSlot: engine.programPin.deploymentSlot,
                upgradeAuthority: engine.programPin.upgradeAuthority ?? null,
                sourceTag: engine.programPin.sourceTag,
                referenceSourceTag: engine.programPin.referenceSourceTag,
                referenceHash: engine.programPin.referenceHash,
              }
            : null,
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
          blockhashRefused: trigger.blockhashRefused,
          blockhashParked: trigger.blockhashParked,
          blockhashRearmed: trigger.blockhashRearmed,
          blockhashSuperseded: trigger.blockhashSuperseded,
          blockhashRefreshRequests: trigger.blockhashRefreshRequests,
          threshold: trigger.threshold
            ? {
                requested: trigger.threshold.requested,
                requestedSource: trigger.threshold.requestedSource,
                effective: trigger.threshold.effective,
                blockReason: trigger.threshold.blockReason,
                generation: trigger.threshold.generation,
                minTicks: optional(trigger.threshold.minTicks),
                ticks: trigger.threshold.ticks,
                streak: trigger.threshold.streak,
                uncomparableTicks: trigger.threshold.uncomparableTicks,
                hits: trigger.threshold.hits,
                misses: trigger.threshold.misses,
                extra: trigger.threshold.extra,
                sampledOut: trigger.threshold.sampledOut,
                fallbacks: trigger.threshold.fallbacks,
                indexTracked: trigger.threshold.indexTracked,
                indexFallback: trigger.threshold.indexFallback,
                indexBootstrapPending: trigger.threshold.indexBootstrapPending,
                indexEvaluations: trigger.threshold.indexEvaluations,
                crosscheckSamples: trigger.threshold.crosscheckSamples,
                crosscheckSkipped: trigger.threshold.crosscheckSkipped,
                crosscheckStale: trigger.threshold.crosscheckStale,
                crosscheckMisses: trigger.threshold.crosscheckMisses,
                sweepDetect: latency(trigger.threshold.sweepDetect),
                thresholdDetect: latency(trigger.threshold.thresholdDetect),
                observeOwner: latency(trigger.threshold.observeOwner),
                observeOwnerNsTotal: trigger.threshold.observeOwnerNsTotal,
                observeBudgetPpm: trigger.threshold.observeBudgetPpm,
                sweepHealthEvaluations: trigger.threshold.sweepHealthEvaluations,
                sweepOwnerNsTotal: trigger.threshold.sweepOwnerNsTotal,
              }
            : null,
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
    sources: m.sources.map((s) => ({ unit: s.unit, role: s.role, mode: s.mode, qualification: s.qualification, generation: s.generation, canonical: s.canonical })),
    lastSourceDecision: m.lastSourceDecision
      ? { seq: m.lastSourceDecision.seq, request: m.lastSourceDecision.request, applied: m.lastSourceDecision.applied, detail: m.lastSourceDecision.detail }
      : null,
    sourceConfigVersion: m.sourceConfigVersion ?? null,
    capabilityConfigVersion: m.capabilityConfigVersion ?? null,
    lastCapabilityDecision: m.lastCapabilityDecision
      ? { seq: m.lastCapabilityDecision.seq, request: m.lastCapabilityDecision.request, applied: m.lastCapabilityDecision.applied, detail: m.lastCapabilityDecision.detail }
      : null,
    approval: m.approval
      ? {
          state: m.approval.state, reason: m.approval.reason, kind: m.approval.kind, approvalId: m.approval.approvalId,
          fundingClass: m.approval.fundingClass, expiresUnixS: m.approval.expiresUnixS,
          maxCapitalUsdMicro: m.approval.maxCapitalUsdMicro, maxLossUsdMicro: m.approval.maxLossUsdMicro,
          minNetUsdMicro: m.approval.minNetUsdMicro, maxSlippageBps: m.approval.maxSlippageBps,
          allowedTargets: m.approval.allowedTargets,
        }
      : null,
    sourceStats: m.sourceStats
      ? { observed: m.sourceStats.observed, rejected: m.sourceStats.rejected, applied: m.sourceStats.applied, refused: m.sourceStats.refused }
      : null,
    admission: m.admission
      ? {
          state: m.admission.state, blockReason: m.admission.blockReason,
          submitted: m.admission.submitted, refused: m.admission.refused, unknown: m.admission.unknown,
          lastRefusal: m.admission.lastRefusal, simulationReused: m.admission.simulationReused,
          simulationResimulated: m.admission.simulationResimulated, simulationRefused: m.admission.simulationRefused,
          lastReuseRefusal: m.admission.lastReuseRefusal, exitsSubmitted: m.admission.exitsSubmitted,
          exitsRefused: m.admission.exitsRefused, exitsUnknown: m.admission.exitsUnknown,
          lastExitRefusal: m.admission.lastExitRefusal,
        }
      : null,
  };
}

/** DEC-04: one HoldFlow; an unbounded residual is null. */
function holdFlow(f: { asset: string; reserve: bigint; residualMax: bigint; actual?: bigint; underestimated: boolean; residualUnbounded: boolean }) {
  return {
    asset: f.asset,
    reserve: f.reserve,
    residualMax: f.residualUnbounded ? null : f.residualMax,
    actual: optional(f.actual),
    underestimated: f.underestimated,
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
    nonLanding: m.nonLanding
      ? {
        subject: m.nonLanding.subject,
        resultClass: m.nonLanding.resultClass,
        alarm: m.nonLanding.alarm,
        sources: m.nonLanding.sources.map((s) => ({ operator: s.operator, class: s.class })),
        cResult: m.nonLanding.cResult,
        holdMs: optional(m.nonLanding.holdMs),
        calls: optional(m.nonLanding.calls),
        bytes: optional(m.nonLanding.bytes),
        reads: m.nonLanding.reads,
      }
      : null,
    holdB: m.holdB
      ? {
        requested: m.holdB.requested,
        effective: m.holdB.effective,
        blockReason: m.holdB.blockReason,
        n: optional(m.holdB.n),
        evidenceExact: m.holdB.evidenceExact,
        evidenceUnderestimated: m.holdB.evidenceUnderestimated,
        evidenceUnqualified: m.holdB.evidenceUnqualified,
        inventoryReservedUsdMicro: m.holdB.inventoryReservedUsdMicro,
        capitalInUseUsdMicro: m.holdB.capitalInUseUsdMicro,
        attempt: m.holdB.attempt
          ? {
            mode: m.holdB.attempt.mode,
            aWallet: amounts(m.holdB.attempt.aWallet),
            aInventoryUsdMicro: m.holdB.attempt.aInventoryUsdMicro,
            bClass: m.holdB.attempt.bClass,
            bReason: m.holdB.attempt.bReason,
            bKind: m.holdB.attempt.bKind,
            bInventoryUsdMicro: optional(m.holdB.attempt.bInventoryUsdMicro),
            reservedInventoryUsdMicro: m.holdB.attempt.reservedInventoryUsdMicro,
            flows: m.holdB.attempt.flows.map(holdFlow),
            aFlows: m.holdB.attempt.aFlows.map(holdFlow),
            settled: m.holdB.attempt.settled,
            actualInventoryUsdMicro: optional(m.holdB.attempt.actualInventoryUsdMicro),
          }
          : null,
      }
      : null,
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

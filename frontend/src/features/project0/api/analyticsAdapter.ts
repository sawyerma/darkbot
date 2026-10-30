import { fromBinary } from "@bufbuild/protobuf";
import {
  AnalyticsJobsSchema,
  type AnalyticsJobs,
  type AutostartGrant,
  type BootFlow,
  type BootStep,
  type JobParams,
  type ShadowCounterfactual,
} from "@/core/generated/darkbot/project0/v1/analytics_pb";
import type { UntilStopped } from "@/core/generated/darkbot/project0/v1/analytics_pb";
import type { AutostartGrantView, BootFlowView, BootStepView, CounterfactualView, JobParamsView, AnalyticsView, UntilStoppedView } from "../model/analytics";

/** Throws on invalid bytes, so the registry never acknowledges a broken state. */
export function decodeAnalyticsJobs(bytes: Uint8Array): AnalyticsJobs {
  return fromBinary(AnalyticsJobsSchema, bytes);
}

/** A canonical decimal (Wire V1.3 §8) or null; anything else is unknown, never 0. */
const decimal = (d: { value: string } | undefined): string | null =>
  d !== undefined && /^(?:0|-?[1-9][0-9]*)$/.test(d.value) ? d.value : null;

const params = (p: JobParams | undefined): JobParamsView | null =>
  p
    ? { minProfitUsdMicro: decimal(p.minProfitUsdMicroSigned), capitalUsdMicro: [...p.capitalUsdMicro], funding: [...p.funding] }
    : null;

const counterfactual = (c: ShadowCounterfactual | undefined): CounterfactualView | null =>
  c
    ? {
        matched: c.matched,
        unknown: c.unknown,
        wins: c.wins,
        lostRaces: c.lostRaces,
        noChainEvent: c.noChainEvent,
        scenarios: c.scenarios.map((s) => ({
          capitalUsdMicro: s.capitalUsdMicro,
          wins: s.wins,
          failedAttempts: s.failedAttempts,
          failedCostUnknown: s.failedCostUnknown,
          netUsdMicro: decimal(s.netUsdMicroSigned),
        })),
        landing: c.landing,
        safety: c.safety,
      }
    : null;

export function toAnalyticsView(m: AnalyticsJobs): AnalyticsView {
  return {
    publishedUnixMs: m.publishedUnixMs,
    runnerBusy: m.runnerBusy,
    runnerError: m.runnerError,
    storeError: m.storeError,
    runs: m.runs.map((r) => ({
      runId: r.runId,
      root: r.root,
      layout: r.layout,
      captureComplete: r.captureComplete,
      qualification: r.qualification
        ? { verdict: r.qualification.verdict, verification: r.qualification.verification, failed: [...r.qualification.failed], error: r.qualification.error }
        : null,
      modifiedUnixMs: r.modifiedUnixMs ?? null,
      running: r.running,
      verification: r.verification.map((v) => ({
        method: v.method,
        specId: v.specId,
        attempts: v.attempts,
        running: v.running,
        status: v.status,
        reason: v.reason ?? null,
        resultId: v.resultId ?? null,
        offline: v.offline,
        providerBinding: v.providerBinding ?? null,
        checkpointReads: v.checkpointReads,
        calls: v.calls ?? null,
        bytes: v.bytes ?? null,
        callsTotal: v.callsTotal ?? null,
        bytesTotal: v.bytesTotal ?? null,
        progressUnit: v.progressUnit,
        progressDone: v.progressDone,
        progressTotal: v.progressTotal ?? null,
        qualified: v.qualified.map((c) => ({ name: c.name, count: c.count })),
        observed: v.observed.map((c) => ({ name: c.name, count: c.count })),
        markers: v.markers.map((c) => ({ name: c.name, count: c.count })),
        comparison: v.comparison.map((c) => ({ name: c.name, count: c.count })),
        transactions: v.transactions.map((c) => ({ name: c.name, count: c.count })),
        disagreements: v.disagreements ?? null,
        unresolved: v.unresolved ?? null,
        unknown: v.unknown ?? null,
      })),
    })),
    jobs: m.jobs.map((j) => ({
      jobId: j.jobId,
      kind: j.kind,
      run: j.run,
      state: j.state,
      code: j.code,
      requestedUnixMs: j.requestedUnixMs ?? null,
      finishedUnixMs: j.finishedUnixMs ?? null,
      maxSeconds: j.maxSeconds,
      maxRamBytes: j.maxRamBytes,
      params: params(j.params),
      verification: j.postCaptureSpec ? { chainVerify: j.postCaptureSpec.chainVerify, offline: j.postCaptureSpec.offline } : null,
      result:
        j.result.case === "backtest"
          ? {
              kind: "BACKTEST",
              resultId: j.result.value.resultId,
              liquidations: j.result.value.liquidations,
              rows: j.result.value.rows,
              attempts: j.result.value.attempts,
              counterfactual: counterfactual(j.result.value.counterfactual),
              realized: j.result.value.realized,
            }
          : j.result.case === "optimize"
            ? {
                kind: "OPTIMIZE",
                resultId: j.result.value.resultId,
                points: j.result.value.points,
                chosen: params(j.result.value.chosen),
                chosenObjectiveBps: decimal(j.result.value.chosenHoldout?.objectiveBpsSigned),
                baselineObjectiveBps: decimal(j.result.value.baselineHoldout?.objectiveBpsSigned),
                profile: j.result.value.profile,
              }
            : j.result.case === "postCapture"
              ? { kind: "POST_CAPTURE", verificationOk: j.result.value.verificationOk, reportOk: j.result.value.reportOk, verdict: j.result.value.verdict || null }
              : j.result.case === "h0"
                ? {
                    kind: "H0", resultId: j.result.value.resultId, windowStartUnixS: j.result.value.windowStartUnixS,
                    windowEndUnixS: j.result.value.windowEndUnixS, liquidations: j.result.value.liquidations,
                    duplicates: j.result.value.duplicates, runsVerified: j.result.value.runsVerified,
                    runsWithoutResult: j.result.value.runsWithoutResult, winners: j.result.value.winners,
                    topWinnerShareBps: j.result.value.topWinnerShareBps ?? null, bands: j.result.value.bands,
                    sampleStatus: j.result.value.sampleStatus, tags: j.result.value.tags,
                    competitive: j.result.value.competitive ?? null, transactions: j.result.value.transactions ?? null,
                    failedAttempts: j.result.value.failedAttempts ?? null, coveredSeconds: j.result.value.coveredSeconds ?? null,
                    bandsBounded: j.result.value.bandsBounded ?? null, bandsBelowOurMinProfit: j.result.value.bandsBelowOurMinProfit ?? null,
                    tipAccounts: j.result.value.tipAccounts ?? null,
                  }
                : null,
    })),
    boot: m.boot ? boot(m.boot) : null,
    bootError: m.bootError,
    untilStopped: untilStopped(m.untilStopped),
  };
}

const grant = (g: AutostartGrant | undefined): AutostartGrantView | null =>
  g
    ? {
        digest: g.digest, mode: g.mode, configSha256: g.configSha256, binarySha256: g.binarySha256, duration: g.duration,
        checkpointSeconds: g.checkpointSeconds, minimumRunwaySeconds: g.minimumRunwaySeconds, qualificationId: g.qualificationId,
        providerEvidenceId: g.providerEvidenceId, genesisHash: g.genesisHash, captureSource: g.captureSource, rpcRps: g.rpcRps,
        maxRequests: g.maxRequests, maxTotalMib: g.maxTotalMib, maxRuntimeSeconds: g.maxRuntimeSeconds, expiresUnixS: g.expiresUnixS,
        recordedUnixS: g.recordedUnixS,
      }
    : null;

/** DEC-16: unbounded runs as the stores hold them (an absent part is null). */
function untilStopped(u: UntilStopped | undefined): UntilStoppedView {
  const q = u?.qualification;
  const r = u?.run;
  return {
    autostart: grant(u?.autostart),
    preview: grant(u?.preview),
    qualification: q
      ? {
          id: q.id, mode: q.mode, checkpointSeconds: q.checkpointSeconds, minimumRunwaySeconds: q.minimumRunwaySeconds,
          complete: q.complete, open: [...q.open], evidenceId: q.evidenceId ?? null,
          evidenceValidUntilUnixS: q.evidenceValidUntilUnixS ?? null, qualifiedAtUnixS: q.qualifiedAtUnixS,
        }
      : null,
    run: r
      ? {
          runId: r.runId, qualificationId: r.qualificationId, startedUnixS: r.startedUnixS, checkpointSeconds: r.checkpointSeconds,
          windows: r.windows, lastVerdict: r.lastVerdict ?? null, lastDetail: r.lastDetail, lastEndedUnixS: r.lastEndedUnixS ?? null,
          halted: r.halted ?? null, sealed: r.sealed, complete: r.complete,
        }
      : null,
    errors: [...(u?.errors ?? [])],
  };
}

const step = (s: BootStep | undefined): BootStepView => ({
  state: s?.state || "NOT_RUN",
  code: s?.code ?? "",
  runId: s?.runId ?? null,
  startedUnixMs: s?.startedUnixMs ?? null,
  finishedUnixMs: s?.finishedUnixMs ?? null,
});

/** DEC-07: the boot controller's flow as it holds it (an absent step is NOT_RUN). */
function boot(b: BootFlow): BootFlowView {
  return {
    flowId: b.flowId,
    mode: b.mode,
    duration: b.duration,
    createdUnixMs: b.createdUnixMs ?? null,
    offlineVerdict: b.offlineVerdict,
    offlineToken: b.offlineToken,
    offlineBlocked: [...b.offlineBlocked],
    ttlSeconds: b.ttlSeconds,
    ttlSource: b.ttlSource,
    network: step(b.network),
    networkVerdict: b.networkVerdict ?? null,
    networkToken: b.networkToken ?? null,
    networkBlocked: [...b.networkBlocked],
    observedUnixS: b.observedUnixS ?? null,
    expiresUnixS: b.expiresUnixS ?? null,
    start: step(b.start),
  };
}

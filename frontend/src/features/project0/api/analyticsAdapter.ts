import { fromBinary } from "@bufbuild/protobuf";
import {
  AnalyticsJobsSchema,
  type AnalyticsJobs,
  type JobParams,
  type ShadowCounterfactual,
} from "@/core/generated/darkbot/project0/v1/analytics_pb";
import type { CounterfactualView, JobParamsView, AnalyticsView } from "../model/analytics";

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
            : null,
    })),
  };
}

/**
 * View of project0.analytics.jobs (B03/U-9): the runs a job can read and the native analytics
 * jobs, as the Rust job controller's store holds them. Amounts are exact decimal strings
 * (USD-micro); unknown stays null, never 0. A counterfactual Shadow result is never realized.
 */
export interface RunQualificationView {
  verdict: string;
  verification: string;
  failed: readonly string[];
  error: string;
}

export interface AnalyticsRunView {
  runId: string;
  root: string;
  /** NATIVE_V2 (a job can read it), NATIVE_V1 or HISTORICAL. */
  layout: string;
  captureComplete: boolean;
  qualification: RunQualificationView | null;
  modifiedUnixMs: bigint | null;
  running: boolean;
}

export interface JobParamsView {
  minProfitUsdMicro: string | null;
  capitalUsdMicro: readonly bigint[];
  funding: readonly string[];
}

export interface ScenarioView {
  capitalUsdMicro: bigint;
  wins: bigint;
  failedAttempts: bigint;
  failedCostUnknown: bigint;
  /** Wins minus failed costs; null when a failed cost is unknown. */
  netUsdMicro: string | null;
}

export interface CounterfactualView {
  matched: bigint;
  unknown: bigint;
  wins: bigint;
  lostRaces: bigint;
  noChainEvent: bigint;
  scenarios: readonly ScenarioView[];
  landing: string;
  safety: string;
}

export type JobResultView =
  | { kind: "BACKTEST"; resultId: string; liquidations: bigint; rows: bigint; attempts: bigint; counterfactual: CounterfactualView | null; realized: string }
  | { kind: "OPTIMIZE"; resultId: string; points: bigint; chosen: JobParamsView | null; chosenObjectiveBps: string | null; baselineObjectiveBps: string | null; profile: string }
  | { kind: "POST_CAPTURE"; verificationOk: boolean; reportOk: boolean; verdict: string | null };

export interface AnalyticsJobView {
  jobId: string;
  kind: string;
  run: string;
  /** QUEUED, RUNNING, INTERRUPTED, COMPLETED, FAILED or CANCELLED. */
  state: string;
  code: string;
  requestedUnixMs: bigint | null;
  finishedUnixMs: bigint | null;
  maxSeconds: bigint;
  maxRamBytes: bigint;
  params: JobParamsView | null;
  /** Post-capture jobs: the verification request. */
  verification: { chainVerify: string; offline: boolean } | null;
  result: JobResultView | null;
}

export interface AnalyticsView {
  publishedUnixMs: bigint;
  runnerBusy: boolean;
  runs: readonly AnalyticsRunView[];
  jobs: readonly AnalyticsJobView[];
  storeError: string;
}

/** Job states with an outcome (never run again). */
export const TERMINAL_JOB_STATES: ReadonlySet<string> = new Set(["COMPLETED", "FAILED", "CANCELLED"]);

/** Runs a backtest or optimization can read: native layout V2 with a recorded capture end. */
export function jobReadableRuns(view: AnalyticsView | null): readonly AnalyticsRunView[] {
  return (view?.runs ?? []).filter((r) => r.layout === "NATIVE_V2" && r.captureComplete && !r.running);
}

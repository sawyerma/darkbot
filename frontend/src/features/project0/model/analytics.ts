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
  /** Why the runner state is unknown; then runnerBusy is no fact. */
  runnerError: string;
  runs: readonly AnalyticsRunView[];
  jobs: readonly AnalyticsJobView[];
  storeError: string;
  /** DEC-07: the boot controller's flow (the active one, else the newest); null without one. */
  boot: BootFlowView | null;
  /** DEC-07: why the boot store was not (completely) readable; empty when read. */
  bootError: string;
}

/** DEC-07: one step of a boot flow (NOT_RUN, QUEUED, RUNNING, INTERRUPTED, COMPLETED, FAILED,
 *  CANCELLED). */
export interface BootStepView {
  state: string;
  code: string;
  runId: string | null;
  startedUnixMs: bigint | null;
  finishedUnixMs: bigint | null;
}

/** DEC-07 (N7): offline preflight → network approval → network audit → start approval → start;
 *  tokens are the 16 characters an approval names. */
export interface BootFlowView {
  flowId: string;
  mode: string;
  /** The run's duration the flow audits and starts (fixed at its offline step). */
  duration: string;
  createdUnixMs: bigint | null;
  offlineVerdict: string;
  offlineToken: string;
  offlineBlocked: readonly string[];
  /** The flow's TTL (fixed at its offline step) and that value's source. */
  ttlSeconds: bigint;
  ttlSource: string;
  network: BootStepView;
  networkVerdict: string | null;
  networkToken: string | null;
  networkBlocked: readonly string[];
  /** The network record's TTL window (unix seconds): its oldest read and its expiry. */
  observedUnixS: bigint | null;
  expiresUnixS: bigint | null;
  start: BootStepView;
}

/** The Overview's one-line job summary from topic 15: the runner state is unknown when the
 * controller says so, and a projection that is not LIVE is named as not current. */
export function analyticsSummary(a: AnalyticsView | null, status: string): string {
  if (!a) return `No analytics projection (${status})`;
  const count = (state: string) => a.jobs.filter(j => j.state === state).length;
  const runner = a.runnerError ? `runner unknown (${a.runnerError})` : a.runnerBusy ? "runner busy" : "runner idle";
  const current = status === "LIVE" ? "" : ` · ${status}, not current`;
  return `${runner} · ${count("RUNNING")} running · ${count("QUEUED")} queued · ${a.jobs.length} listed · ${a.runs.length} readable runs${current}`;
}

/** Job states with an outcome (never run again). */
export const TERMINAL_JOB_STATES: ReadonlySet<string> = new Set(["COMPLETED", "FAILED", "CANCELLED"]);

/**
 * Why a job command must not be issued now (empty: it may). Only a LIVE projection with a known
 * runner state is current; a stale list could name runs and jobs that are no longer so.
 */
export function jobCommandBlock(view: AnalyticsView | null, topicStatus: string, busy: boolean): string {
  if (busy) return "Command in progress";
  if (topicStatus !== "LIVE" || !view) return `Job status not current (${topicStatus}); no command is sent on a stale list`;
  if (view.runnerError) return `Runner state unknown: ${view.runnerError}`;
  if (view.runnerBusy) return "An analytics job is running (one at a time)";
  return "";
}

/** Runs a backtest or optimization can read: native layout V2 with a recorded capture end. */
export function jobReadableRuns(view: AnalyticsView | null): readonly AnalyticsRunView[] {
  return (view?.runs ?? []).filter((r) => r.layout === "NATIVE_V2" && r.captureComplete && !r.running);
}

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
  /** DEC-08: every chain verification method of the run (its typed verification records). */
  verification: readonly VerificationMethodView[];
}

/** A count under a bounded name (a liquidation kind, a marker or a status code). */
export interface NamedCountView { name: string; count: bigint }

/**
 * DEC-08: one chain verification method of a run — its spec with the newest attempt and its
 * newest finished result. Never a promotion (only strict reaches CANONICAL_VERIFIED); an
 * unknown count is null or an empty list, never 0.
 */
export interface VerificationMethodView {
  /** off | index | strict | crosscheck */
  method: string;
  specId: string;
  attempts: bigint;
  running: boolean;
  /** RUNNING, NOT_RUN, INDEX_VERIFIED, CROSSCHECK_VERIFIED, CANONICAL_VERIFIED or HISTORICAL_INCOMPLETE. */
  status: string;
  /** The newest attempt's block or incompleteness reason (block_reason). */
  reason: string | null;
  resultId: string | null;
  offline: boolean;
  /** SHA-256 of the provider endpoint(s), never the endpoint; null offline. */
  providerBinding: string | null;
  checkpointReads: bigint;
  calls: bigint | null;
  bytes: bigint | null;
  progressUnit: string;
  progressDone: bigint;
  progressTotal: bigint | null;
  qualified: readonly NamedCountView[];
  observed: readonly NamedCountView[];
  markers: readonly NamedCountView[];
  comparison: readonly NamedCountView[];
  disagreements: bigint | null;
  unresolved: bigint | null;
  unknown: bigint | null;
}

/** DEC-08: the Capture tab's line of one method: status, progress, provider, requests, bytes,
 *  checkpoint and block reason as the records hold them. */
export function methodLine(m: VerificationMethodView): string {
  const progress = `${m.progressDone}${m.progressTotal != null ? ` / ${m.progressTotal}` : ""} ${m.progressUnit}`;
  const provider = m.offline ? "checkpoint only" : m.providerBinding ? `provider ${m.providerBinding.slice(0, 12)}` : "no provider";
  return `${m.status}${m.reason ? ` (${m.reason})` : ""} · progress ${progress} · ${provider} · requests ${m.calls ?? "—"} · bytes ${m.bytes ?? "—"} · checkpoint ${m.checkpointReads} reads · ${m.attempts} attempt${m.attempts === 1n ? "" : "s"}`;
}

/** DEC-08: the methods the Analytics tab shows side by side, in their rank order. */
export const COMPARED_METHODS = ["index", "crosscheck", "strict"] as const;

/** One value per compared method (`—` for a method the run never ran). */
export function sideBySide(methods: readonly VerificationMethodView[], value: (m: VerificationMethodView) => string): string {
  return COMPARED_METHODS.map((name) => {
    const m = methods.find((x) => x.method === name);
    return `${name} ${m ? value(m) : "—"}`;
  }).join(" · ");
}

/** The count of `name` in a complete list; `—` when the list is unknown (empty). */
export function countOf(list: readonly NamedCountView[], name: string): string {
  if (list.length === 0) return "—";
  return (list.find((c) => c.name === name)?.count ?? 0n).toString();
}

/** The sum of the comparison counts whose name starts with one of `prefixes`; `—` when unknown. */
export function comparisonSum(m: VerificationMethodView, prefixes: readonly string[]): string {
  if (m.comparison.length === 0) return "—";
  return m.comparison.filter((c) => prefixes.some((p) => c.name.startsWith(p))).reduce((n, c) => n + c.count, 0n).toString();
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
  /** DEC-16: the standing autostart approval, the newest raw-capture qualification and run. */
  untilStopped: UntilStoppedView;
}

/** DEC-16: a standing autostart approval (in force or previewed) and the binding it was given for. */
export interface AutostartGrantView {
  digest: string;
  mode: string;
  configSha256: string;
  binarySha256: string;
  duration: string;
  checkpointSeconds: bigint;
  minimumRunwaySeconds: bigint;
  qualificationId: string;
  providerEvidenceId: string;
  genesisHash: string;
  captureSource: string;
  rpcRps: bigint;
  maxRequests: bigint;
  maxTotalMib: bigint;
  maxRuntimeSeconds: bigint;
  expiresUnixS: bigint;
  /** When it was granted, respectively previewed. */
  recordedUnixS: bigint;
}

/** DEC-16: a qualification of an unbounded run; `open` names every part that did not pass. */
export interface UntilStoppedQualificationView {
  id: string;
  mode: string;
  checkpointSeconds: bigint;
  minimumRunwaySeconds: bigint;
  complete: boolean;
  open: readonly string[];
  evidenceId: string | null;
  evidenceValidUntilUnixS: bigint | null;
  qualifiedAtUnixS: bigint;
}

/** DEC-16: an unbounded raw capture's windows journal as far as it is written. */
export interface UntilStoppedRunView {
  runId: string;
  qualificationId: string;
  startedUnixS: bigint;
  checkpointSeconds: bigint;
  windows: bigint;
  lastVerdict: string | null;
  lastDetail: string;
  lastEndedUnixS: bigint | null;
  halted: string | null;
  sealed: boolean;
  complete: boolean;
}

export interface UntilStoppedView {
  autostart: AutostartGrantView | null;
  preview: AutostartGrantView | null;
  qualification: UntilStoppedQualificationView | null;
  run: UntilStoppedRunView | null;
  errors: readonly string[];
}

/** DEC-16: the unbounded horizon of a boot flow (`p0ctl` and the gateway check it again). */
export const UNTIL_STOPPED = "until-stopped";

/** DEC-16: whole seconds in their largest exact unit (d, h, m, s) — the canonical text the
 *  controller binds an unbounded horizon by (`until-stopped:1h:1d`). */
export function durationText(seconds: bigint): string {
  for (const [unit, scale] of [["d", 86400n], ["h", 3600n], ["m", 60n]] as const) {
    if (seconds % scale === 0n) return `${seconds / scale}${unit}`;
  }
  return `${seconds}s`;
}

/** Whole seconds of a `p0ctl` duration (digits and s/m/h/d); null for anything else. */
export function durationSeconds(d: string): bigint | null {
  const m = /^([1-9]\d{0,5})([smhd])$/.exec(d.trim());
  if (!m) return null;
  const scale = { s: 1n, m: 60n, h: 3600n, d: 86400n }[m[2] as "s" | "m" | "h" | "d"];
  return BigInt(m[1]!) * scale;
}

/**
 * DEC-16: why "Until stopped" is locked for this boot (empty: it may be chosen). The start
 * admits it again natively (RunHorizon::admit against the run and the clock); this only shows
 * what the projection already decides: the newest qualification, of the chosen mode, every
 * part passed, the same window and runway, its provider evidence still fresh at `nowS`.
 */
export function untilStoppedBlock(v: { mode: string; window: string; runway: string; qualificationId: string; nowS: bigint },
  q: UntilStoppedQualificationView | null): string {
  const qualify = `p0ctl horizon qualify --mode ${v.mode} --checkpoint W --minimum-runway R`;
  if (!q) return `No qualification (${qualify})`;
  if (q.mode !== v.mode) return `The newest qualification ${q.id} is the ${q.mode} run's, not ${v.mode}'s (${qualify})`;
  if (v.qualificationId.trim() !== q.id) return `Qualification ${v.qualificationId.trim() || "—"} is not the newest one shown (${q.id}); only that one is checked here`;
  if (!q.complete) return `Qualification ${q.id} incomplete: ${q.open.join("; ")}`;
  const w = durationSeconds(v.window);
  const r = durationSeconds(v.runway);
  if (w === null || r === null) return "Window and runway: durations (s/m/h/d)";
  if (w !== q.checkpointSeconds || r !== q.minimumRunwaySeconds) return `Window/runway differ from the qualification (${q.checkpointSeconds} s / ${q.minimumRunwaySeconds} s)`;
  if (q.evidenceValidUntilUnixS === null || q.evidenceValidUntilUnixS < v.nowS) return `The qualification's provider evidence is no longer fresh (${q.evidenceValidUntilUnixS ?? "unknown"}); qualify again`;
  return "";
}

/** DEC-16: the command that stops the unbounded run shown in order (drain, seal, final status):
 *  an econ4h or Shadow run through its boot flow's running start step — only a flow that runs
 *  until stopped with the run's window (null: the running flow is another run) — the raw
 *  capture through the desktop capture control. */
export function untilStoppedStopAction(flow: { mode: string; duration: string; start: { state: string } } | null,
  run: { checkpointSeconds: bigint } | null): "project0.boot.cancel" | "project0.capture.stop" | null {
  if (flow != null && flow.mode !== "market" && flow.start.state === "RUNNING" && flow.duration.startsWith(`${UNTIL_STOPPED}:`)) {
    const window = durationSeconds(flow.duration.split(":")[1] ?? "");
    return window !== null && run !== null && window === run.checkpointSeconds ? "project0.boot.cancel" : null;
  }
  return "project0.capture.stop";
}

/** DEC-16: the final status of an unbounded run as its windows journal shows it. */
export function untilStoppedRunStatus(r: UntilStoppedRunView): string {
  if (!r.sealed) return r.halted ? `HALTING · ${r.halted} · drain and seal in progress` : "RUNNING";
  if (r.complete) return "SEALED_COMPLETE · ordered end, every window held";
  return `SEALED_INCOMPLETE${r.halted ? ` · HALT ${r.halted}` : " · not an ordered end"}`;
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

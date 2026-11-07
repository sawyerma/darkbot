// V8 Training runs (FE-V8-07/08, BSC-V8-02/03): the native run owner's runs of one module revision
// (bsc/query TrainingRuns, newest first, cursor pages) over a recorded runtime run (ReplaySources)
// and the two run commands. A run is a REPLAY: a start names the profile at its revision with
// exactly one bound and the recorded run; a stop names the run, no version. A run's counts are its
// end's: none while it is PENDING or RUNNING, never a zero for unknown.
import type { BscQueryOutcome } from "@/core/realtime/bscQuery";
import { Availability, DataQuality, TrainingRunMode, TrainingRunStatus, TrainingRunStopKind, type TrainingResult } from "../api/bscV8Adapter";
import { MAX_DATE_MS } from "./live";

type Pending = { kind: "IDLE" | "LOADING" };
type Answer = BscQueryOutcome | Pending;

export type PlanKind = "DURATION" | "LAUNCH_COUNT" | "UNTIL_STOPPED";
export interface RunView {
  id: string; profileId: string; revision: bigint; configSha256: string; mode: string; venues: readonly string[];
  plan: { kind: string; durationMs?: bigint; maxLaunches?: bigint };
  startedMs?: bigint; endedMs?: bigint;
  /** The owner's status name (PENDING, RUNNING, STOPPING, COMPLETED, STOPPED, FAILED, INTERRUPTED, UNKNOWN). */
  status: string; stopReason: string;
  /** PENDING, RUNNING or STOPPING: its job runs (one at a time; the module is in use). */
  active: boolean;
  launches: bigint; result?: TrainingResult;
  /** The run record's own quality (INCOMPLETE while it runs) and why. */
  quality: string; qualityReasons: readonly string[];
  requestId: string; replaySource: string;
}
export type RunsView =
  | { ok: true; runs: readonly RunView[]; nextCursor: string; hasMore: boolean }
  | { ok: false; reason?: string };

const ACTIVE = new Set(["PENDING", "RUNNING", "STOPPING"]);
const name = <E extends Record<number, string>>(e: E, v: number) => e[v] ?? String(v);

/** The owner's run page; not answered: `ok` false (the query's own reason names why). */
export function runsOf(q: Answer): RunsView {
  if (q.kind !== "ANSWER") return { ok: false };
  if (q.result.case !== "trainingRuns") return { ok: false, reason: `QUERY_ANSWER_MISMATCH ${q.result.case ?? ""}` };
  const p = q.result.value;
  if (p.availability !== Availability.AVAILABLE) return { ok: false, reason: p.unavailableReason || "OWNER_UNAVAILABLE" };
  return {
    ok: true, nextCursor: p.nextCursor, hasMore: p.hasMore,
    runs: p.runs.map(r => {
      const status = name(TrainingRunStatus, r.status);
      return {
        id: r.runId, profileId: r.profileId, revision: r.profileRevision, configSha256: r.configSha256, mode: name(TrainingRunMode, r.mode), venues: r.venues,
        plan: { kind: r.plan ? name(TrainingRunStopKind, r.plan.stopKind) : "UNSPECIFIED", durationMs: r.plan?.durationMs, maxLaunches: r.plan?.maxLaunches },
        startedMs: r.startedUnixMs, endedMs: r.endedUnixMs, status, stopReason: r.stopReason, active: ACTIVE.has(status),
        launches: r.launchesProcessed, result: r.result,
        quality: r.provenance ? name(DataQuality, r.provenance.quality) : "UNKNOWN", qualityReasons: r.provenance?.qualityReasons ?? [],
        requestId: r.requestId, replaySource: r.replaySource,
      };
    }),
  };
}

export interface SourceView {
  id: string; kind: string; fromMs?: bigint; toMs?: bigint;
  /** Its records; undefined while it has no manifest (still recording or cut off: unknown, not 0). */
  records?: bigint;
  quality: string; reasons: readonly string[]; configSha256: string;
}
export type SourcesView =
  | { ok: true; sources: readonly SourceView[]; nextCursor: string; hasMore: boolean }
  | { ok: false; reason?: string };

/** The owner's replay catalogue (recorded runtime runs, newest first). */
export function sourcesOf(q: Answer): SourcesView {
  if (q.kind !== "ANSWER") return { ok: false };
  if (q.result.case !== "replaySources") return { ok: false, reason: `QUERY_ANSWER_MISMATCH ${q.result.case ?? ""}` };
  const p = q.result.value;
  if (p.availability !== Availability.AVAILABLE) return { ok: false, reason: p.unavailableReason || "OWNER_UNAVAILABLE" };
  return {
    ok: true, nextCursor: p.nextCursor, hasMore: p.hasMore,
    sources: p.sources.map(s => ({
      id: s.sourceId, kind: s.kind, fromMs: s.from?.unixMs, toMs: s.to?.unixMs, configSha256: s.configSha256,
      records: s.qualityReasons.some(r => r.startsWith("NO_MANIFEST")) ? undefined : s.records,
      quality: name(DataQuality, s.quality), reasons: s.qualityReasons,
    })),
  };
}

/** A run plan as the operator enters it: the bound's kind, hours (DURATION) and launches. */
export interface PlanForm { kind: PlanKind; hours: string; launches: string }
/** FE-V8-07's offered start values (1 hour, 100 launches); the owner takes any positive bound. */
export const PLAN_START: PlanForm = { kind: "DURATION", hours: "1", launches: "100" };
const MS_PER_HOUR = 3_600_000n;
const U64_MAX = (1n << 64n) - 1n;

/** The plan's typed bound (exactly the field its kind names), or why it cannot be sent: hours are
 *  converted exactly to whole milliseconds of the recorded run's time, never rounded. */
export function planOf(f: PlanForm): { plan: { stopKind: TrainingRunStopKind; durationMs?: bigint; maxLaunches?: bigint } } | { error: string } {
  if (f.kind === "UNTIL_STOPPED") return { plan: { stopKind: TrainingRunStopKind.UNTIL_STOPPED } };
  if (f.kind === "LAUNCH_COUNT") {
    const t = f.launches.trim();
    if (!/^[0-9]+$/.test(t)) return { error: "Maximum launches is no whole number" };
    const n = BigInt(t);
    if (n === 0n) return { error: "Maximum launches must be at least 1" };
    if (n > U64_MAX) return { error: "Maximum launches is above the native maximum" };
    return { plan: { stopKind: TrainingRunStopKind.LAUNCH_COUNT, maxLaunches: n } };
  }
  const m = /^([0-9]+)(?:\.([0-9]+))?$/.exec(f.hours.trim());
  if (!m) return { error: "Duration is no number of hours" };
  const frac = m[2] ?? "";
  const scale = 10n ** BigInt(frac.length);
  const scaled = (BigInt(m[1] ?? "0") * scale + BigInt(frac || "0")) * MS_PER_HOUR;
  if (scaled % scale !== 0n) return { error: "Duration is no whole number of milliseconds" };
  const ms = scaled / scale;
  if (ms === 0n) return { error: "Duration must be above 0" };
  if (ms > U64_MAX) return { error: "Duration is above the native maximum" };
  return { plan: { stopKind: TrainingRunStopKind.DURATION, durationMs: ms } };
}

/** The params of a start (a REPLAY of the module's revision on its venues), or why not. */
export function startParams(profile: { id: string; venues: readonly string[] }, f: PlanForm, source: string):
  { profileId: string; mode: TrainingRunMode; venues: string[]; plan: { stopKind: TrainingRunStopKind; durationMs?: bigint; maxLaunches?: bigint }; replaySource: string } | { error: string } {
  if (!source) return { error: "Choose a recorded run to replay" };
  if (!profile.venues.length) return { error: "The module names no market" };
  const p = planOf(f);
  if ("error" in p) return p;
  return { profileId: profile.id, mode: TrainingRunMode.REPLAY, venues: [...profile.venues], plan: p.plan, replaySource: source };
}

/** The diagnostic reason a stop from here carries (TrainingRunStopCommand.reason). */
export const STOP_REASON = "operator stop (Training module)";

/** The run a decided start created (`outcome=APPLIED action=START run_id=<id>`), from its receipt. */
export function appliedRun(detail: string): string | undefined {
  if (!/(^|\s)outcome=APPLIED(\s|$)/.test(detail) || !/(^|\s)action=START(\s|$)/.test(detail)) return undefined;
  const id = /(^|\s)run_id=(\S+)/.exec(detail)?.[2];
  return id && id !== "-" ? id : undefined;
}

/** A duration in its largest whole unit. */
export function durationText(ms: bigint): string {
  if (ms > 0n && ms % MS_PER_HOUR === 0n) return `${ms / MS_PER_HOUR} h`;
  if (ms > 0n && ms % 60_000n === 0n) return `${ms / 60_000n} min`;
  if (ms > 0n && ms % 1_000n === 0n) return `${ms / 1_000n} s`;
  return `${ms} ms`;
}

/** What ends a run, in words. */
export function planText(plan: RunView["plan"]): string {
  if (plan.kind === "DURATION") return plan.durationMs === undefined ? "Duration (no bound)" : `${durationText(plan.durationMs)} of recorded time`;
  if (plan.kind === "LAUNCH_COUNT") return plan.maxLaunches === undefined ? "Launches (no bound)" : `${plan.maxLaunches} launches`;
  if (plan.kind === "UNTIL_STOPPED") return "Until stopped or the recording ends";
  return plan.kind;
}

const STATUS_LABEL: Record<string, string> = {
  PENDING: "Starting", RUNNING: "Test running", STOPPING: "Stopping", COMPLETED: "Test completed",
  STOPPED: "Stopped", FAILED: "Failed", INTERRUPTED: "Interrupted", UNKNOWN: "Unknown",
};
/** A run's status as the badge shows it, and its tone. */
export function statusBadge(status: string | undefined): { label: string; tone: "" | "good" | "warn" | "bad" } {
  if (status === undefined) return { label: "Ready", tone: "" };
  const label = STATUS_LABEL[status] ?? status;
  return { label, tone: status === "RUNNING" || status === "PENDING" ? "good" : status === "STOPPING" ? "warn" : status === "FAILED" || status === "INTERRUPTED" || status === "UNKNOWN" ? "bad" : "" };
}

/** The V8 result metrics of one run (FE-V8-08), each absent while unknown: no run, a run still
 *  active (its counts are its end's) or no result. Completed trades = the exits; the win rate =
 *  the closed positions with a gain over them (one decimal, truncated). The net stays absent while
 *  the owner leaves it unresolved. */
export interface RunMetrics { netWei?: string; completed?: bigint; winRate?: string; evaluated?: bigint; rejected?: bigint; dataIncomplete?: bigint }
export function runMetrics(r: RunView | undefined): RunMetrics {
  const t = r && !r.active ? r.result : undefined;
  if (!t) return {};
  const completed = t.exitsTarget + t.exitsStop + t.exitsCreatorSell + t.exitsTimeout;
  const tenths = completed > 0n ? (t.wins * 1000n) / completed : undefined;
  return {
    netWei: t.netWei, completed, evaluated: t.evaluated, rejected: t.rejected, dataIncomplete: t.dataIncomplete,
    winRate: tenths === undefined ? undefined : `${tenths / 10n}.${tenths % 10n} %`,
  };
}

/** A unix-ms time as `YYYY-MM-DD HH:MM` (UTC), or “—”. */
export const timeText = (ms: bigint | undefined) => ms === undefined ? "—" : ms > MAX_DATE_MS ? `unix ms ${ms}` : new Date(Number(ms)).toISOString().replace("T", " ").slice(0, 16);

/** A recorded span in whole hours and minutes (the remainder left out). */
export const spanText = (ms: bigint) => ms < 60_000n ? "under 1 min" : `${ms >= MS_PER_HOUR ? `${ms / MS_PER_HOUR} h ` : ""}${(ms % MS_PER_HOUR) / 60_000n} min`;

/** A recorded run as the choice shows it: its span and records (unknown while recording). */
export function sourceLabel(s: SourceView): string {
  const span = s.fromMs !== undefined && s.toMs !== undefined && s.toMs >= s.fromMs ? ` (${spanText(s.toMs - s.fromMs)})` : "";
  const records = s.records === undefined ? "still recording, records unknown" : `${s.records} records`;
  return `${timeText(s.fromMs)} – ${s.toMs === undefined ? "recording" : timeText(s.toMs)}${span} · ${records}${s.quality === "VERIFIED" ? "" : " · incomplete"}`;
}

/** The recorded run a start offers first: the newest complete one, else the newest. */
export function defaultSource(sources: readonly SourceView[]): string {
  return (sources.find(s => s.quality === "VERIFIED") ?? sources[0])?.id ?? "";
}

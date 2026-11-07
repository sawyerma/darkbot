import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { TrainingRunStopKind, decodeV8 } from "../api/bscV8Adapter";
import {
  PLAN_START, appliedRun, defaultSource, durationText, planOf, planText, runMetrics, runsOf, sourceLabel, sourcesOf, spanText, startParams, statusBadge, type RunView,
} from "./training";

const dir = new URL("../../../../../protocol/golden/bsc/v8/", import.meta.url);
const golden = (name: string) => Uint8Array.from(Buffer.from(readFileSync(new URL(`${name}.hex`, dir), "utf8").trim(), "hex"));

describe("training run plans (FE-V8-07)", () => {
  it("converts hours exactly to whole milliseconds and refuses what is no positive bound", () => {
    expect(planOf(PLAN_START)).toEqual({ plan: { stopKind: TrainingRunStopKind.DURATION, durationMs: 3_600_000n } });
    expect(planOf({ ...PLAN_START, hours: "0.25" })).toEqual({ plan: { stopKind: TrainingRunStopKind.DURATION, durationMs: 900_000n } });
    expect(planOf({ ...PLAN_START, hours: "0.000001" })).toEqual({ error: "Duration is no whole number of milliseconds" });
    expect(planOf({ ...PLAN_START, hours: "0" })).toEqual({ error: "Duration must be above 0" });
    expect(planOf({ ...PLAN_START, hours: "1,5" })).toEqual({ error: "Duration is no number of hours" });
    expect(planOf({ ...PLAN_START, hours: "5124095576030432" })).toEqual({ error: "Duration is above the native maximum" });
    expect(planOf({ ...PLAN_START, kind: "LAUNCH_COUNT" })).toEqual({ plan: { stopKind: TrainingRunStopKind.LAUNCH_COUNT, maxLaunches: 100n } });
    expect(planOf({ ...PLAN_START, kind: "LAUNCH_COUNT", launches: "0" })).toEqual({ error: "Maximum launches must be at least 1" });
    expect(planOf({ ...PLAN_START, kind: "LAUNCH_COUNT", launches: "18446744073709551616" })).toEqual({ error: "Maximum launches is above the native maximum" });
    expect(planOf({ ...PLAN_START, kind: "UNTIL_STOPPED", hours: "x" })).toEqual({ plan: { stopKind: TrainingRunStopKind.UNTIL_STOPPED } });
    expect(startParams({ id: "p000001", venues: ["BSC_VENUE_FLAP"] }, PLAN_START, "")).toEqual({ error: "Choose a recorded run to replay" });
    expect([durationText(5_400_000n), durationText(1_500n), planText({ kind: "LAUNCH_COUNT", maxLaunches: 7n }), spanText(3_780_123n), spanText(59_999n)])
      .toEqual(["90 min", "1500 ms", "7 launches", "1 h 3 min", "under 1 min"]);
  });

  it("reads a start's run only from an applied START receipt", () => {
    expect(appliedRun("outcome=APPLIED action=START run_id=t000002 request_id=r1\nrun=t000002 status=PENDING")).toBe("t000002");
    expect(appliedRun("outcome=REJECTED_ACTIVE action=START run_id=- request_id=r2 reason=t000001 runs")).toBeUndefined();
    expect(appliedRun("outcome=APPLIED action=STOP run_id=t000002 request_id=r3")).toBeUndefined();
  });
});

describe("training run results (FE-V8-08)", () => {
  const run = (over: Partial<RunView>): RunView => ({
    id: "t000001", profileId: "p000001", revision: 1n, configSha256: "", mode: "REPLAY", venues: [], plan: { kind: "UNTIL_STOPPED" },
    status: "COMPLETED", stopReason: "JOURNAL_END", active: false, launches: 9n, quality: "VERIFIED", qualityReasons: [], requestId: "r", replaySource: "s", ...over,
  });
  // run-1 of the golden page: every count 0, its net -1; the counts set here.
  const zero = decodeV8("training_run_page", golden("training_run_page")).runs[1]!.result!;
  const result = { ...zero, evaluated: 9n, rejected: 4n, wins: 2n, losses: 1n, exitsTarget: 2n, exitsStop: 1n, netWei: "-5" };

  it("shows a run's counts only at its end and a win rate over its exits", () => {
    expect(runMetrics(run({ result }))).toEqual({ netWei: "-5", completed: 3n, winRate: "66.6 %", evaluated: 9n, rejected: 4n, dataIncomplete: 0n });
    expect(runMetrics(run({ result, status: "RUNNING", active: true }))).toEqual({});
    expect(runMetrics(run({ result: zero })).winRate).toBeUndefined();
    expect(runMetrics(undefined)).toEqual({});
    expect([statusBadge("STOPPING"), statusBadge(undefined).label, statusBadge("INTERRUPTED").tone]).toEqual([{ label: "Stopping", tone: "warn" }, "Ready", "bad"]);
  });

  it("decodes the owner's pages and keeps a recording without manifest unknown", () => {
    const runs = runsOf({ kind: "ANSWER", requestId: "q", result: { case: "trainingRuns", value: decodeV8("training_run_page", golden("training_run_page")) } });
    if (!runs.ok) throw new Error("runs");
    expect(runs.runs.map(r => [r.id, r.status, r.active, r.plan.kind, r.result?.netWei])).toEqual([["run-2", "RUNNING", true, "UNTIL_STOPPED", undefined], ["run-1", "INTERRUPTED", false, "DURATION", "-1"]]);
    const sources = sourcesOf({ kind: "ANSWER", requestId: "q", result: { case: "replaySources", value: decodeV8("replay_source_page", golden("replay_source_page")) } });
    if (!sources.ok) throw new Error("sources");
    expect(sources.sources[0]?.records).toBeUndefined();
    expect(sourceLabel(sources.sources[0]!)).toContain("still recording, records unknown · incomplete");
    expect(defaultSource(sources.sources)).toBe("1794026420785");
    expect(runsOf({ kind: "ANSWER", requestId: "q", result: { case: "replaySources", value: decodeV8("replay_source_page", golden("replay_source_page")) } })).toEqual({ ok: false, reason: "QUERY_ANSWER_MISMATCH replaySources" });
  });
});

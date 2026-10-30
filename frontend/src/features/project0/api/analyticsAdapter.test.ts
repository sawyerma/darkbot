import { describe, expect, it } from "vitest";
import analyticsHex from "../../../../../protocol/golden/project0/analytics_jobs.hex?raw";
import { decodeAnalyticsJobs, toAnalyticsView } from "./analyticsAdapter";
import { countOf, countSum, durationText, h0Line, jobCommandBlock, jobReadableRuns, methodLine, sideBySide, untilStoppedBlock, untilStoppedRunStatus, untilStoppedStopAction } from "../model/analytics";

const bytes = (hex: string) => Uint8Array.from(hex.trim().match(/../g)!.map((h) => parseInt(h, 16)));

describe("project0.analytics.jobs (golden frame written by the Rust reporter)", () => {
  const view = toAnalyticsView(decodeAnalyticsJobs(bytes(analyticsHex)));

  it("DEC-07: maps the boot controller's newest flow — tokens, the flow's TTL and source, the TTL window, every step", () => {
    expect(view.boot).toMatchObject({
      flowId: "flow-1", mode: "econ4h", duration: "1h", offlineVerdict: "OFFLINE_READY_NETWORK_NOT_RUN", offlineToken: "0123456789abcdef",
      ttlSeconds: 90n, ttlSource: "cli", networkVerdict: "READY", networkToken: "fedcba9876543210",
      observedUnixS: 1_760_000_001n, expiresUnixS: 1_760_000_091n,
    });
    expect([view.boot?.network.state, view.boot?.start.state]).toEqual(["COMPLETED", "NOT_RUN"]);
    expect(view.boot?.start.runId).toBeNull();
    expect(view.bootError).toBe("");
  });

  it("decodes runs, jobs and their states as the store holds them", () => {
    expect(view.runnerBusy).toBe(true);
    expect(view.runs.map((r) => r.runId)).toEqual(["run-a", "run-b"]);
    expect(view.runs[0]?.qualification).toBeNull();
    expect(view.runs[1]?.qualification?.verdict).toBe("UNREADABLE");
    expect(view.runs[1]?.verification).toEqual([]);
    expect(view.jobs.map((j) => [j.jobId, j.kind, j.state])).toEqual([
      ["opt-1", "OPTIMIZE", "RUNNING"],
      ["bt-1", "BACKTEST", "COMPLETED"],
      ["pc-1", "POST_CAPTURE", "COMPLETED"],
      ["h0-1", "H0", "COMPLETED"],
      ["h0-0", "H0", "COMPLETED"],
    ]);
    // H0: every saved run (no run), its headline as the job store holds it.
    expect(view.jobs[3]?.run).toBe("");
    expect(view.jobs[3]?.result).toEqual({
      kind: "H0", resultId: "h0-1", windowStartUnixS: 1_757_408_000n, windowEndUnixS: 1_760_000_000n, liquidations: 12n,
      duplicates: 3n, runsVerified: 2n, runsWithoutResult: 1n, winners: 4n, topWinnerShareBps: 5_000n, bands: 2n,
      sampleStatus: "MIN_SAMPLE_UNDECIDED", tags: "BLOCKED: H0-04 (tags, take times, premium(t), our t_profitable) needs the 0.1.12 deployment",
      competitive: 10n, transactions: 11n, failedAttempts: 5n, coveredSeconds: 86_400n, bandsBounded: 1n,
      bandsBelowOurMinProfit: 1n, tipAccounts: 8n,
    });
    const r = view.jobs[3]!.result!;
    if (r.kind !== "H0") throw new Error("H0");
    expect(h0Line(r)).toBe("2025-09-09 … 2025-10-09 (covered 24.0 h) · 12 liquidations (10 competitive, 3 seen again) · 11 transactions"
      + " · runs 2 verified / 1 without · 4 winners · top 50.00 % · 5 failed attempts · 1/2 min-profit bands bounded (upper bounds; 1 below ours)"
      + " · tips 8 accounts · MIN_SAMPLE_UNDECIDED · tags BLOCKED: H0-04 (tags, take times, premium(t), our t_profitable) needs the 0.1.12 deployment");
    expect(h0Line({ ...r, topWinnerShareBps: null })).toContain("top —");
    expect(h0Line({ ...r, tipAccounts: 0n })).toContain("tips unknown");
    // A report of 9a41e808 (before the H0 review): its headline, the later counts unknown — never 0.
    const pre = view.jobs[4]!.result!;
    if (pre.kind !== "H0") throw new Error("H0");
    expect([pre.competitive, pre.transactions, pre.failedAttempts, pre.coveredSeconds, pre.bandsBounded, pre.bandsBelowOurMinProfit, pre.tipAccounts])
      .toEqual([null, null, null, null, null, null, null]);
    expect(h0Line(pre)).toBe("2025-08-28 … 2025-09-27 (covered — h) · 7 liquidations (— competitive, 0 seen again) · — transactions"
      + " · runs 1 verified / 0 without · 2 winners · top 70.00 % · — failed attempts · —/1 min-profit bands bounded (upper bounds; — below ours)"
      + " · tips — · MIN_SAMPLE_UNDECIDED · tags BLOCKED: H0-04 needs the 0.1.12 deployment");
    expect(view.jobs[2]?.verification).toEqual({ chainVerify: "strict", offline: true });
    expect(view.jobs[2]?.result).toEqual({ kind: "POST_CAPTURE", verificationOk: false, reportOk: true, verdict: null });
    expect(view.jobs[2]?.params).toBeNull();
    expect(view.jobs[0]?.result).toBeNull();
    expect(view.jobs[0]?.params?.minProfitUsdMicro).toBe("1500000");
  });

  it("DEC-08: decodes each chain verification method as recorded; unknown counts stay unknown", () => {
    const [index, crosscheck] = view.runs[0]!.verification;
    expect([index?.method, index?.status, index?.calls, index?.bytes, index?.progressTotal, index?.checkpointReads])
      .toEqual(["index", "INDEX_VERIFIED", 5n, 65_248n, null, 7n]);
    expect(countOf(index!.qualified, "RECEIVERSHIP")).toBe("1");
    expect(countOf(index!.markers, "end_liquidation")).toBe("2");
    expect(countSum(index!.comparison, ["MISSING_"])).toBe("1");
    expect(countOf(index!.comparison, "MATCH")).toBe("1");
    expect(countOf(index!.transactions, "MISSING_LIVE")).toBe("2");
    expect(countOf(index!.transactions, "BOT_ONLY_INDEX_MISSING")).toBe("1");
    expect([index?.callsTotal, index?.bytesTotal]).toEqual([9n, 98_000n]);
    expect(methodLine(index!)).toBe(`INDEX_VERIFIED · progress 2 pages · provider ${"c".repeat(12)} · requests 5 (9 all attempts) · bytes 65248 · checkpoint 7 reads · 2 attempts`);
    expect([crosscheck?.status, crosscheck?.reason, crosscheck?.calls, crosscheck?.disagreements]).toEqual(["NOT_RUN", "CROSSCHECK_SECOND_PROVIDER_MISSING", null, null]);
    expect(countOf(crosscheck!.qualified, "RECEIVERSHIP")).toBe("—");
    expect(sideBySide(view.runs[0]!.verification, (m) => m.status)).toBe("index INDEX_VERIFIED · crosscheck NOT_RUN · strict —");
  });

  it("keeps amounts exact and an unknown net null", () => {
    const r = view.jobs[1]?.result;
    if (r?.kind !== "BACKTEST") throw new Error("backtest expected");
    expect(r.liquidations).toBe(3n);
    expect(r.counterfactual?.scenarios[0]?.netUsdMicro).toBeNull();
    expect(r.counterfactual?.scenarios[1]?.netUsdMicro).toBe("-2000");
    expect(r.counterfactual?.landing).toBe("UNPROVEN");
  });

  it("offers only completed native runs that are not running", () => {
    expect(jobReadableRuns(view).map((r) => r.runId)).toEqual(["run-a", "run-b"]);
    expect(jobReadableRuns({ ...view, runs: view.runs.map((r) => ({ ...r, layout: "HISTORICAL" })) })).toEqual([]);
    expect(jobReadableRuns(null)).toEqual([]);
  });

  it("issues job commands on a LIVE list with a known idle runner only (review H1, M4)", () => {
    const idle = { ...view, runnerBusy: false };
    expect(jobCommandBlock(idle, "LIVE", false)).toBe("");
    expect(jobCommandBlock(idle, "DETACHED", false)).toMatch(/not current \(DETACHED\)/);
    expect(jobCommandBlock(null, "LIVE", false)).not.toBe("");
    expect(jobCommandBlock({ ...idle, runnerError: "JOB_STORE_IO: denied" }, "LIVE", false)).toMatch(/unknown/);
    expect(jobCommandBlock(view, "LIVE", false)).toMatch(/running/);
    expect(jobCommandBlock(idle, "LIVE", true)).toMatch(/in progress/);
  });

  it("DEC-16: maps the standing approval, its preview, the raw-capture qualification and the unbounded run", () => {
    const u = view.untilStopped;
    expect(u.autostart).toMatchObject({ mode: "market", duration: "until-stopped:1h:24h", checkpointSeconds: 3600n, minimumRunwaySeconds: 86400n,
      qualificationId: "0123456789abcdef", providerEvidenceId: "89abcdef01234567", captureSource: "logs", rpcRps: 5n, maxRequests: 10_000n,
      maxTotalMib: 512n, maxRuntimeSeconds: 604_800n, expiresUnixS: 1_760_600_000n, recordedUnixS: 1_760_000_100n });
    expect(u.autostart?.digest).toMatch(/^[0-9a-f]{16}$/);
    expect(u.preview?.maxRuntimeSeconds).toBe(86_400n);
    expect(u.preview?.digest).not.toBe(u.autostart?.digest);
    expect(u.qualification).toMatchObject({ id: "0123456789abcdef", mode: "market", complete: false, open: ["capacity: NOT_RUN · free disk unknown"] });
    expect(u.run).toMatchObject({ runId: "market-1", windows: 2n, lastVerdict: "CONTINUE", halted: null, sealed: true, complete: true });
    expect(untilStoppedRunStatus(u.run!)).toMatch(/^SEALED_COMPLETE/);
    expect(u.errors).toEqual([]);
  });

  it("DEC-16: 'Until stopped' stays locked, with its reason, until a complete fresh qualification of the same horizon", () => {
    const q = view.untilStopped.qualification!;
    const input = { mode: "market", window: "1h", runway: "24h", qualificationId: q.id, nowS: 1_760_000_500n };
    expect(untilStoppedBlock(input, q)).toMatch(/incomplete: capacity: NOT_RUN/);
    const complete = { ...q, complete: true, open: [] };
    expect(untilStoppedBlock(input, complete)).toBe("");
    // DEC-16 (stage 3): every mode runs until stopped, each with a qualification of its own mode.
    expect(untilStoppedBlock({ ...input, mode: "econ4h" }, complete)).toMatch(/is the market run's, not econ4h's/);
    expect(untilStoppedBlock({ ...input, mode: "shadow" }, { ...complete, mode: "shadow" })).toBe("");
    expect(untilStoppedBlock(input, null)).toMatch(/No qualification/);
    expect(untilStoppedBlock({ ...input, qualificationId: "fedcba9876543210" }, complete)).toMatch(/not the newest/);
    expect(untilStoppedBlock({ ...input, window: "30m" }, complete)).toMatch(/differ from the qualification/);
    expect(untilStoppedBlock({ ...input, window: "3600s", runway: "1d" }, complete)).toBe("");
    // The pre-filled text is the controller's canonical one.
    expect([durationText(3600n), durationText(86400n), durationText(90n), durationText(7200n)]).toEqual(["1h", "1d", "90s", "2h"]);
    expect(untilStoppedBlock({ ...input, nowS: 1_760_600_001n }, complete)).toMatch(/no longer fresh/);
    const halted = { ...view.untilStopped.run!, complete: false, halted: "UNTIL_STOPPED_PROVIDER_EVIDENCE_MISSING" };
    expect(untilStoppedRunStatus(halted)).toBe("SEALED_INCOMPLETE · HALT UNTIL_STOPPED_PROVIDER_EVIDENCE_MISSING");
    expect(untilStoppedRunStatus({ ...halted, sealed: false, halted: null })).toBe("RUNNING");
  });

  it("DEC-16 (stage 3): a running econ4h or Shadow run stops through its boot flow, the raw capture through the desktop control", () => {
    const flow = (mode: string, state: string, duration = "until-stopped:1h:1d") => ({ mode, duration, start: { state } });
    const run = { checkpointSeconds: 3600n };
    expect(untilStoppedStopAction(flow("shadow", "RUNNING"), run)).toBe("project0.boot.cancel");
    expect(untilStoppedStopAction(flow("econ4h", "RUNNING"), run)).toBe("project0.boot.cancel");
    expect(untilStoppedStopAction(flow("market", "RUNNING"), run)).toBe("project0.capture.stop");
    expect(untilStoppedStopAction(flow("shadow", "COMPLETED"), run)).toBe("project0.capture.stop");
    expect(untilStoppedStopAction(null, run)).toBe("project0.capture.stop");
    // Review L-2: a running bounded flow is no unbounded run; one with another window is another run.
    expect(untilStoppedStopAction(flow("shadow", "RUNNING", "1h"), run)).toBe("project0.capture.stop");
    expect(untilStoppedStopAction(flow("econ4h", "RUNNING", "until-stopped:2h:1d"), run)).toBeNull();
    expect(untilStoppedStopAction(flow("econ4h", "RUNNING"), null)).toBeNull();
  });

  it("rejects malformed wire bytes", () => {
    expect(() => decodeAnalyticsJobs(Uint8Array.from([0xff, 0xff, 0xff]))).toThrow();
  });
});

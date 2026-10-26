import { describe, expect, it } from "vitest";
import analyticsHex from "../../../../../protocol/golden/project0/analytics_jobs.hex?raw";
import { decodeAnalyticsJobs, toAnalyticsView } from "./analyticsAdapter";
import { jobCommandBlock, jobReadableRuns } from "../model/analytics";

const bytes = (hex: string) => Uint8Array.from(hex.trim().match(/../g)!.map((h) => parseInt(h, 16)));

describe("project0.analytics.jobs (golden frame written by the Rust reporter)", () => {
  const view = toAnalyticsView(decodeAnalyticsJobs(bytes(analyticsHex)));

  it("DEC-07: maps the boot controller's newest flow — tokens, the flow's TTL and source, the TTL window, every step", () => {
    expect(view.boot).toMatchObject({
      flowId: "flow-1", mode: "econ4h", offlineVerdict: "OFFLINE_READY_NETWORK_NOT_RUN", offlineToken: "0123456789abcdef",
      ttlSeconds: 90n, ttlSource: "cli", networkVerdict: "READY", networkToken: "fedcba9876543210",
      observedUnixS: 1_760_000_001n, expiresUnixS: 1_760_000_091n,
    });
    expect([view.boot?.network.state, view.boot?.start.state]).toEqual(["COMPLETED", "NOT_RUN"]);
    expect(view.boot?.start.runId).toBeNull();
  });

  it("decodes runs, jobs and their states as the store holds them", () => {
    expect(view.runnerBusy).toBe(true);
    expect(view.runs.map((r) => r.runId)).toEqual(["run-a", "run-b"]);
    expect(view.runs[0]?.qualification).toBeNull();
    expect(view.runs[1]?.qualification?.verdict).toBe("UNREADABLE");
    expect(view.jobs.map((j) => [j.jobId, j.kind, j.state])).toEqual([
      ["opt-1", "OPTIMIZE", "RUNNING"],
      ["bt-1", "BACKTEST", "COMPLETED"],
      ["pc-1", "POST_CAPTURE", "COMPLETED"],
    ]);
    expect(view.jobs[2]?.verification).toEqual({ chainVerify: "strict", offline: true });
    expect(view.jobs[2]?.result).toEqual({ kind: "POST_CAPTURE", verificationOk: false, reportOk: true, verdict: null });
    expect(view.jobs[2]?.params).toBeNull();
    expect(view.jobs[0]?.result).toBeNull();
    expect(view.jobs[0]?.params?.minProfitUsdMicro).toBe("1500000");
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

  it("rejects malformed wire bytes", () => {
    expect(() => decodeAnalyticsJobs(Uint8Array.from([0xff, 0xff, 0xff]))).toThrow();
  });
});

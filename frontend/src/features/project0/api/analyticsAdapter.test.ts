import { describe, expect, it } from "vitest";
import analyticsHex from "../../../../../protocol/golden/project0/analytics_jobs.hex?raw";
import { decodeAnalyticsJobs, toAnalyticsView } from "./analyticsAdapter";
import { jobCommandBlock, jobReadableRuns, untilStoppedBlock, untilStoppedRunStatus } from "../model/analytics";

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
    expect(untilStoppedBlock({ ...input, mode: "econ4h" }, complete)).toMatch(/RUN_HORIZON_UNTIL_STOPPED_GATED/);
    expect(untilStoppedBlock(input, null)).toMatch(/No qualification/);
    expect(untilStoppedBlock({ ...input, qualificationId: "fedcba9876543210" }, complete)).toMatch(/not the newest/);
    expect(untilStoppedBlock({ ...input, window: "30m" }, complete)).toMatch(/differ from the qualification/);
    expect(untilStoppedBlock({ ...input, window: "3600s", runway: "1d" }, complete)).toBe("");
    expect(untilStoppedBlock({ ...input, nowS: 1_760_600_001n }, complete)).toMatch(/no longer fresh/);
    const halted = { ...view.untilStopped.run!, complete: false, halted: "UNTIL_STOPPED_PROVIDER_EVIDENCE_MISSING" };
    expect(untilStoppedRunStatus(halted)).toBe("SEALED_INCOMPLETE · HALT UNTIL_STOPPED_PROVIDER_EVIDENCE_MISSING");
    expect(untilStoppedRunStatus({ ...halted, sealed: false, halted: null })).toBe("RUNNING");
  });

  it("rejects malformed wire bytes", () => {
    expect(() => decodeAnalyticsJobs(Uint8Array.from([0xff, 0xff, 0xff]))).toThrow();
  });
});

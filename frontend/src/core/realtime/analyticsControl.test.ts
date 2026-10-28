import { describe, it, expect, beforeEach, vi } from "vitest";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { CommandPhase, CommandRequestSchema, CommandResponseSchema, CommandStatus, CommandLookupResponseSchema } from "../generated/darkbot/control/v1/commands_pb";
import {
  AutostartGrantCommandSchema, AutostartPreviewCommandSchema, BacktestJobCommandSchema, BootNetworkCommandSchema, BootStartCommandSchema,
  OptimizeJobCommandSchema, PostCaptureJobCommandSchema,
} from "../generated/darkbot/project0/v1/analytics_pb";
import { SourceControlCommandSchema } from "../generated/darkbot/project0/v1/owner_pb";
import {
  analyticsCommand, autostartGrantCommand, autostartPreviewCommand, backtestCommand, bootNetworkCommand, bootStartCommand, optimizeCommand, recheck,
  sourceCommand, untilStoppedDuration, useAnalyticsControl, verifyCommand, type PointInput,
} from "./analyticsControl";

const session = () => new Response(null, { status: 200, headers: { "x-csrf-token": "csrf" } });
const accepted = (phase: CommandPhase, detail = "") => new Response(toBinary(CommandResponseSchema, create(CommandResponseSchema, { status: CommandStatus.ACCEPTED, phase, detail })));
const lookup = (phase: CommandPhase) => new Response(toBinary(CommandLookupResponseSchema, create(CommandLookupResponseSchema, { found: true, phase })));
const point: PointInput = { runId: "run-a", minProfitUsd: "1.5", capitalUsd: "100, 1000", funding: "all", maxSeconds: "60", maxRamMib: "64" };

describe("Analytics job commands", () => {
  beforeEach(() => useAnalyticsControl.setState({ busy: false, receipt: null }));

  it("types the operator's inputs, never with a hidden budget", () => {
    const ok = backtestCommand(point);
    if (!("params" in ok)) throw new Error(ok.problem);
    const c = fromBinary(BacktestJobCommandSchema, ok.params);
    expect([c.runId, c.minProfitUsd, c.capitalUsd, c.funding, c.maxSeconds, c.maxRamBytes]).toEqual(["run-a", "1.5", [100n, 1000n], [], 60n, 64n * 1_048_576n]);
    for (const bad of [{ runId: "" }, { minProfitUsd: "1e5" }, { capitalUsd: "0" }, { funding: "all;x" }, { maxSeconds: "" }, { maxRamMib: "-1" }]) {
      expect("problem" in backtestCommand({ ...point, ...bad })).toBe(true);
    }
    const o = optimizeCommand(point, { minProfitUsdGrid: "1.5,2", fundingSets: "all; NO_SWAP_INVENTORY", holdoutBps: "3000", maxPoints: "4", saveProfile: true });
    if (!("params" in o)) throw new Error(o.problem);
    const g = fromBinary(OptimizeJobCommandSchema, o.params);
    expect([g.baseline?.runId, g.minProfitUsdGrid, g.fundingSets.map((s) => s.classes), g.holdoutBps, g.maxPoints, g.saveProfile])
      .toEqual(["run-a", ["1.5", "2"], [[], ["NO_SWAP_INVENTORY"]], 3000n, 4n, true]);
  });

  it("types a verification off or strict from the cache, with an explicit budget", () => {
    const v = verifyCommand({ runId: "run-a", mode: "strict-cache", resume: true, maxSeconds: "60", maxRamMib: "1" });
    if (!("params" in v)) throw new Error(v.problem);
    const c = fromBinary(PostCaptureJobCommandSchema, v.params);
    expect([c.runId, c.chainVerify, c.offline, c.resumeVerification, c.maxSeconds, c.maxRamBytes]).toEqual(["run-a", "strict", true, true, 60n, 1_048_576n]);
    const off = verifyCommand({ runId: "run-a", mode: "off", resume: false, maxSeconds: "60", maxRamMib: "1" });
    if (!("params" in off)) throw new Error(off.problem);
    expect(fromBinary(PostCaptureJobCommandSchema, off.params).offline).toBe(false);
    expect("problem" in verifyCommand({ runId: "run-a", mode: "off", resume: false, maxSeconds: "", maxRamMib: "1" })).toBe(true);
  });

  it("DEC-08: types index and crosscheck from their checkpoints as strict (never online)", () => {
    for (const [mode, chain] of [["index-cache", "index"], ["crosscheck-cache", "crosscheck"]] as const) {
      const v = verifyCommand({ runId: "run-a", mode, resume: true, maxSeconds: "60", maxRamMib: "1" });
      if (!("params" in v)) throw new Error(v.problem);
      const c = fromBinary(PostCaptureJobCommandSchema, v.params);
      expect([c.chainVerify, c.offline, c.resumeVerification]).toEqual([chain, true, true]);
    }
  });

  it("DEC-07: names the network audit's provider evidence by its record id, never by a path", () => {
    const v = { flowId: "flow-1", token: "0123456789abcdef", genesisHash: "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d", captureSource: "logs" as const };
    for (const providerEvidence of ["/data/provider_evidence/89abcdef01234567.p0j", "relative.p0j", "89ABCDEF01234567", "89abcdef0123456", ""]) {
      expect(bootNetworkCommand({ ...v, providerEvidence })).toHaveProperty("problem");
    }
    const ok = bootNetworkCommand({ ...v, providerEvidence: " 89abcdef01234567 " });
    if (!("params" in ok)) throw Error(ok.problem);
    expect(fromBinary(BootNetworkCommandSchema, ok.params).providerEvidence).toBe("89abcdef01234567");
  });
  it("types a source mode request for known units only", () => {
    const ok = sourceCommand("geyser:publicnode", "observe");
    if (!("params" in ok)) throw new Error(ok.problem);
    const c = fromBinary(SourceControlCommandSchema, ok.params);
    expect([c.unit, c.mode]).toEqual(["geyser:publicnode", "observe"]);
    expect("problem" in sourceCommand("geyser:../x", "off")).toBe(true);
    expect("problem" in sourceCommand("", "off")).toBe(true);
  });

  it("posts one typed request whose id becomes the job id, and shows the native refusal", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(session()).mockResolvedValueOnce(accepted(CommandPhase.FAILED, "Native controller refused: JOB_RUNNER_BUSY"));
    vi.stubGlobal("fetch", fetch);
    const ok = backtestCommand(point);
    if (!("params" in ok)) throw new Error(ok.problem);
    await analyticsCommand("project0.analytics.backtest.start", { params: ok.params });
    const req = fromBinary(CommandRequestSchema, (fetch.mock.calls[1]?.[1] as RequestInit).body as Uint8Array);
    expect([req.action, req.resourceId, fromBinary(BacktestJobCommandSchema, req.params).runId]).toEqual(["project0.analytics.backtest.start", "", "run-a"]);
    expect(useAnalyticsControl.getState()).toMatchObject({ busy: false, receipt: { id: req.requestId, phase: "FAILED", detail: "Native controller refused: JOB_RUNNER_BUSY" } });
  });

  it("never locks the panel after an unknown outcome and looks it up again on request", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(session()).mockRejectedValueOnce(new TypeError("reset")).mockRejectedValueOnce(new TypeError("down"))
      .mockResolvedValueOnce(lookup(CommandPhase.COMPLETED));
    vi.stubGlobal("fetch", fetch);
    await analyticsCommand("project0.analytics.job.cancel", { resourceId: "job-1" });
    expect(useAnalyticsControl.getState()).toMatchObject({ busy: false, receipt: { phase: "UNKNOWN" } });
    await recheck();
    expect(fetch.mock.calls.filter(([, init]) => (init as RequestInit | undefined)?.method === "POST")).toHaveLength(1);
    expect(useAnalyticsControl.getState().receipt?.phase).toBe("COMPLETED");
  });

  it("looks a lost acknowledgement up once and never posts again", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(session()).mockRejectedValueOnce(new TypeError("reset")).mockResolvedValueOnce(lookup(CommandPhase.COMPLETED));
    vi.stubGlobal("fetch", fetch);
    await analyticsCommand("project0.analytics.job.cancel", { resourceId: "job-1" });
    expect(fetch.mock.calls.filter(([, init]) => (init as RequestInit | undefined)?.method === "POST")).toHaveLength(1);
    expect(useAnalyticsControl.getState()).toMatchObject({ busy: false, receipt: { phase: "COMPLETED", action: "project0.analytics.job.cancel" } });
  });
});

describe("DEC-16 unbounded boot and the standing autostart approval", () => {
  const GENESIS = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d";
  it("an unbounded start names its qualification; a fixed one sends none", () => {
    const flow = { flowId: "flow-1", token: "fedcba9876543210", captureSource: "logs" as const, chainVerify: "off" as const };
    expect(untilStoppedDuration("1h", "24h")).toBe("until-stopped:1h:24h");
    expect(untilStoppedDuration("1h", "")).toBeNull();
    expect(untilStoppedDuration("1x", "24h")).toBeNull();
    const missing = bootStartCommand({ ...flow, duration: "until-stopped:1h:24h" });
    expect("problem" in missing && missing.problem).toMatch(/qualification id/);
    const ok = bootStartCommand({ ...flow, duration: "until-stopped:1h:24h", qualification: "0123456789abcdef" });
    if (!("params" in ok)) throw new Error(ok.problem);
    expect(fromBinary(BootStartCommandSchema, ok.params).qualification).toBe("0123456789abcdef");
    const fixed = bootStartCommand({ ...flow, duration: "1h", qualification: "0123456789abcdef" });
    if (!("params" in fixed)) throw new Error(fixed.problem);
    expect(fromBinary(BootStartCommandSchema, fixed.params).qualification).toBeUndefined();
  });
  it("a preview carries the typed terms; a grant only the digest the preview showed", () => {
    const terms = { qualification: "0123456789abcdef", window: "1h", runway: "24h", genesisHash: GENESIS, maxRuntime: "7d", validFor: "30d", captureSource: "logs" as const };
    const ok = autostartPreviewCommand(terms);
    if (!("params" in ok)) throw new Error(ok.problem);
    const c = fromBinary(AutostartPreviewCommandSchema, ok.params);
    expect([c.qualification, c.checkpoint, c.minimumRunway, c.genesisHash, c.maxRuntime, c.validFor, c.captureSource])
      .toEqual(["0123456789abcdef", "1h", "24h", GENESIS, "7d", "30d", "logs"]);
    for (const broken of [{ qualification: "x" }, { window: "" }, { genesisHash: "0OIl" }, { maxRuntime: "" }, { validFor: "forever" }]) {
      expect("problem" in autostartPreviewCommand({ ...terms, ...broken }), JSON.stringify(broken)).toBe(true);
    }
    const grant = autostartGrantCommand("89abcdef01234567");
    if (!("params" in grant)) throw new Error(grant.problem);
    expect(fromBinary(AutostartGrantCommandSchema, grant.params).confirmDigest).toBe("89abcdef01234567");
    expect("problem" in autostartGrantCommand("")).toBe(true);
  });
});

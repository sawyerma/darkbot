import { describe, it, expect, beforeEach, vi } from "vitest";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { CommandPhase, CommandRequestSchema, CommandResponseSchema, CommandStatus, CommandLookupResponseSchema } from "../generated/darkbot/control/v1/commands_pb";
import { BacktestJobCommandSchema, OptimizeJobCommandSchema } from "../generated/darkbot/project0/v1/analytics_pb";
import { analyticsCommand, backtestCommand, optimizeCommand, useAnalyticsControl, type PointInput } from "./analyticsControl";

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

  it("looks a lost acknowledgement up once and never posts again", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(session()).mockRejectedValueOnce(new TypeError("reset")).mockResolvedValueOnce(lookup(CommandPhase.COMPLETED));
    vi.stubGlobal("fetch", fetch);
    await analyticsCommand("project0.analytics.job.cancel", { resourceId: "job-1" });
    expect(fetch.mock.calls.filter(([, init]) => (init as RequestInit | undefined)?.method === "POST")).toHaveLength(1);
    expect(useAnalyticsControl.getState()).toMatchObject({ busy: false, receipt: { phase: "COMPLETED", action: "project0.analytics.job.cancel" } });
  });
});

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { CommandLookupResponseSchema, CommandPhase, CommandRequestSchema, CommandResponseSchema, CommandStatus } from "../generated/darkbot/control/v1/commands_pb";
import {
  COMMAND_TIMEOUT_MS, READ_TIMEOUT_MS, applyReadinessJob, candidateCommand, candidateMirrorShows, liveDecisionCommand, ownerCommand, readinessCommand,
  useCandidateControl, useReadinessControl, useSourceControl, type LiveDecision,
} from "./bscControl";

const session = () => new Response(null, { status: 200, headers: { "x-csrf-token": "csrf" } });
const accepted = (phase: CommandPhase) => new Response(toBinary(CommandResponseSchema, create(CommandResponseSchema, { status: CommandStatus.ACCEPTED, phase, detail: "ledger" })));
const lookup = (phase: CommandPhase, detail = "lookup") => new Response(toBinary(CommandLookupResponseSchema, create(CommandLookupResponseSchema, { found: true, phase, detail })));
const calls = (f: ReturnType<typeof vi.fn>) => f.mock.calls.map(([url, init]) => `${(init as RequestInit | undefined)?.method ?? "GET"} ${String(url).split("?")[0]}`);

describe("BSC readiness control without browser polling", () => {
  beforeEach(() => {
    useReadinessControl.setState({ process: null, busy: false, receipt: null, sources: { bsc: null, overview: null }, awaiting: null });
  });
  it("takes process truth only from the pushed job and drops it with the source", () => {
    const fetch = vi.fn(); vi.stubGlobal("fetch", fetch);
    applyReadinessJob("bsc", { state: "STOPPED", launchId: "run-1", resultCode: "BLOCKED", controllerAvailable: true });
    expect(useReadinessControl.getState().process).toMatchObject({ state: "STOPPED", run_id: "run-1", controller_available: true });
    applyReadinessJob("bsc", null);
    expect(useReadinessControl.getState().process).toBeNull();
    expect(fetch).not.toHaveBeenCalled();
  });
  it("uses the POST phase, never a follow-up GET, and unlocks only when the push shows the new run", async () => {
    applyReadinessJob("bsc", { state: "STOPPED", launchId: "run-1", resultCode: "STOP", controllerAvailable: true });
    const fetch = vi.fn().mockResolvedValueOnce(session()).mockResolvedValueOnce(accepted(CommandPhase.COMPLETED)); vi.stubGlobal("fetch", fetch);
    await readinessCommand("bsc.readiness.start");
    expect(calls(fetch)).toEqual(["GET /api/v1/session", "POST /api/v1/command"]);
    expect(useReadinessControl.getState()).toMatchObject({ busy: true, receipt: { phase: "COMPLETED" } });
    await readinessCommand("bsc.readiness.start");
    expect(fetch).toHaveBeenCalledTimes(2);
    applyReadinessJob("bsc", { state: "RUNNING", launchId: "run-2", resultCode: "" });
    expect(useReadinessControl.getState().busy).toBe(false);
  });
  it("keeps PENDING_RECONCILE locked", async () => {
    applyReadinessJob("bsc", { state: "STOPPED", launchId: "run-1", resultCode: "STOP", controllerAvailable: true });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(session()).mockResolvedValueOnce(accepted(CommandPhase.PENDING_RECONCILE)));
    await readinessCommand("bsc.readiness.start");
    expect(useReadinessControl.getState()).toMatchObject({ busy: true, receipt: { phase: "PENDING_RECONCILE" } });
  });
  it("recovers a lost acknowledgement with exactly one lookup and never a second POST", async () => {
    applyReadinessJob("bsc", { state: "RUNNING", launchId: "run-1", resultCode: "", controllerAvailable: true });
    const fetch = vi.fn().mockResolvedValueOnce(session()).mockRejectedValueOnce(new TypeError("connection reset")).mockResolvedValueOnce(lookup(CommandPhase.PENDING)); vi.stubGlobal("fetch", fetch);
    await readinessCommand("bsc.readiness.stop");
    expect(calls(fetch)).toEqual(["GET /api/v1/session", "POST /api/v1/command", "GET /api/v1/command"]);
    expect(useReadinessControl.getState()).toMatchObject({ busy: true, receipt: { phase: "PENDING" } });
    applyReadinessJob("bsc", { state: "STOPPED", launchId: "run-1", resultCode: "STOPPED" });
    expect(useReadinessControl.getState().busy).toBe(false);
  });
});

describe("BSC candidate request", () => {
  beforeEach(() => useCandidateControl.setState({ busy: false, receipt: null, awaiting: null }));
  it("needs no GET after ACCEPTED and stays locked until the native mirror shows the request", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(session()).mockResolvedValueOnce(accepted(CommandPhase.COMPLETED)); vi.stubGlobal("fetch", fetch);
    await candidateCommand("SELF_B", 4n);
    expect(fetch).toHaveBeenCalledTimes(2);
    const { busy, awaiting, receipt } = useCandidateControl.getState();
    expect([busy, awaiting, receipt?.phase]).toEqual([false, receipt?.id, "COMPLETED"]);
    await candidateCommand("SELF_A", 4n);
    expect(fetch).toHaveBeenCalledTimes(2);
    candidateMirrorShows(receipt?.id);
    expect(useCandidateControl.getState().awaiting).toBeNull();
  });
  it("keeps a lost acknowledgement open when the lookup is still PENDING (N4)", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(session()).mockRejectedValueOnce(new TypeError("reset")).mockResolvedValueOnce(lookup(CommandPhase.PENDING_RECONCILE)); vi.stubGlobal("fetch", fetch);
    await candidateCommand("SELF_B", 4n);
    expect(fetch).toHaveBeenCalledTimes(3);
    expect(fetch.mock.calls.filter(([, init]) => (init as RequestInit | undefined)?.method === "POST")).toHaveLength(1);
    expect(useCandidateControl.getState()).toMatchObject({ busy: true, receipt: { phase: "PENDING_RECONCILE" } });
  });
});

describe("BSC owner requests (Audit E 4.12: the one BSC command client)", () => {
  beforeEach(() => useSourceControl.setState({ busy: false, receipt: null }));
  it("takes the ledger receipt line from one lookup after ACCEPTED, under its origin", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(session()).mockResolvedValueOnce(accepted(CommandPhase.COMPLETED))
      .mockResolvedValueOnce(lookup(CommandPhase.COMPLETED, "outcome=APPLIED source=BSC_INGRESS_DRPC")); vi.stubGlobal("fetch", fetch);
    await ownerCommand("bsc-source", "bsc.source.mode.set", "BSC_INGRESS_DRPC", "SHADOW", 4n);
    expect(calls(fetch)).toEqual(["GET /api/v1/session", "POST /api/v1/command", "GET /api/v1/command"]);
    expect(useSourceControl.getState()).toMatchObject({ busy: false, receipt: { phase: "COMPLETED", detail: "outcome=APPLIED source=BSC_INGRESS_DRPC", origin: "bsc-source" } });
  });
  it("keeps a lost acknowledgement locked when its one lookup fails, and never posts twice", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(session()).mockRejectedValueOnce(new TypeError("reset")).mockRejectedValueOnce(new TypeError("down")); vi.stubGlobal("fetch", fetch);
    await ownerCommand("bsc-live-econ", "bsc.economic.set", "BSC_ECON_LIVE", "DISABLED", 9n);
    expect(calls(fetch)).toEqual(["GET /api/v1/session", "POST /api/v1/command", "GET /api/v1/command"]);
    expect(useSourceControl.getState()).toMatchObject({ busy: true, receipt: { phase: "UNKNOWN", detail: "reset", origin: "bsc-live-econ" } });
  });
  it("sends the operator's LIVE ledger decisions at the state they were decided on, nothing else (REV.1.0 R3b)", async () => {
    const lot = "0x" + "ab".repeat(32);
    const cases: [LiveDecision, string, string, bigint][] = [
      [{ action: "bsc.live.lot", lot, decision: "WRITE_OFF", ledgerVersion: 7n }, "bsc.live.lot", `${lot}:WRITE_OFF`, 7n],
      [{ action: "bsc.live.nonce_burn", tx: lot, ledgerVersion: 8n }, "bsc.live.nonce_burn", lot, 8n],
      [{ action: "bsc.live.ledger.quarantine", seq: 4n, generation: 3n }, "bsc.live.ledger.quarantine", "4", 3n],
    ];
    for (const [decision, action, resource, version] of cases) {
      useSourceControl.setState({ busy: false, receipt: null });
      const fetch = vi.fn().mockResolvedValueOnce(session()).mockResolvedValueOnce(accepted(CommandPhase.COMPLETED))
        .mockResolvedValueOnce(lookup(CommandPhase.COMPLETED, "outcome=APPLIED")); vi.stubGlobal("fetch", fetch);
      await liveDecisionCommand("bsc-live-lot", decision);
      const [, init] = fetch.mock.calls.find(([, i]) => (i as RequestInit | undefined)?.method === "POST")!;
      const req = fromBinary(CommandRequestSchema, (init as RequestInit).body as Uint8Array);
      expect([req.action, req.resourceId, req.params.length, req.expected?.version]).toEqual([action, resource, 0, { case: "configVersion", value: version }]);
      expect(useSourceControl.getState().receipt).toMatchObject({ phase: "COMPLETED", detail: "outcome=APPLIED", origin: "bsc-live-lot" });
    }
  });
  it("is the only BSC command client", () => {
    const dir = new URL(".", import.meta.url);
    const clients = readdirSync(dir).filter(f => f.endsWith(".ts") && !f.endsWith(".test.ts"))
      .filter(f => { const s = readFileSync(new URL(f, dir), "utf8"); return s.includes("ProjectId.BSC") && s.includes("/api/v1/command"); });
    expect(clients).toEqual(["bscControl.ts"]);
  });
});

describe("BSC command timeouts (Audit E 3.8: one configured value)", () => {
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllEnvs(); vi.resetModules(); });
  const timeouts = async (load: () => Promise<typeof import("./bscControl")>) => {
    const spy = vi.spyOn(AbortSignal, "timeout");
    const m = await load();
    vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(session()).mockRejectedValueOnce(new TypeError("reset")).mockResolvedValueOnce(lookup(CommandPhase.PENDING)));
    m.useCandidateControl.setState({ busy: false, receipt: null, awaiting: null });
    await m.candidateCommand("SELF_A", 1n);
    return spy.mock.calls.map(([ms]) => ms);
  };
  it("waits longer than the gateway's controller bound and reads with a tenth of it", async () => {
    expect([COMMAND_TIMEOUT_MS, READ_TIMEOUT_MS]).toEqual([60_000, 6_000]);
    expect(await timeouts(async () => import("./bscControl"))).toEqual([6_000, 60_000, 6_000]);
  });
  it("takes the build's VITE_COMMAND_TIMEOUT_MS and derives the reads from it", async () => {
    vi.stubEnv("VITE_COMMAND_TIMEOUT_MS", "20000");
    vi.resetModules();
    expect(await timeouts(async () => import("./bscControl"))).toEqual([2_000, 20_000, 2_000]);
  });
});

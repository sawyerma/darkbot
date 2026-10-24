import { describe, it, expect, beforeEach, vi } from "vitest";
import { create, toBinary } from "@bufbuild/protobuf";
import { CommandLookupResponseSchema, CommandPhase, CommandResponseSchema, CommandStatus } from "../generated/darkbot/control/v1/commands_pb";
import { applyReadinessJob, readinessCommand, useReadinessControl } from "./readinessControl";

const session = () => new Response(null, { status: 200, headers: { "x-csrf-token": "csrf" } });
const accepted = (phase: CommandPhase) => new Response(toBinary(CommandResponseSchema, create(CommandResponseSchema, { status: CommandStatus.ACCEPTED, phase, detail: "ledger" })));
const lookup = (phase: CommandPhase) => new Response(toBinary(CommandLookupResponseSchema, create(CommandLookupResponseSchema, { found: true, phase, detail: "lookup" })));
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

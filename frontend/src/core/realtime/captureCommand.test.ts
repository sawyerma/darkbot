import { beforeEach, describe, expect, it, vi } from "vitest";
import { create, toBinary } from "@bufbuild/protobuf";
import { CommandLookupResponseSchema, CommandPhase, CommandResponseSchema, CommandStatus } from "../generated/darkbot/control/v1/commands_pb";
import { captureButtons, captureCommand, useCaptureControl, type CaptureProcess, type ProcessState } from "./captureControl";

const process = (state: ProcessState, controller_available = true): CaptureProcess =>
  ({ state, run_id: "", pid: null, reason: "", observed_unix_ms: 0, controller_available });

describe("Project 0 capture commands: ACCEPTED is not EXECUTED, nothing is replayed", () => {
  const session = () => new Response(null, { status: 200, headers: { "x-csrf-token": "csrf" } });
  const stopped = () => new Response(JSON.stringify(process("STOPPED")));
  const response = (status: CommandStatus, detail: string) => new Response(toBinary(CommandResponseSchema, create(CommandResponseSchema, { status, detail })));
  const lookup = (phase: CommandPhase) => new Response(toBinary(CommandLookupResponseSchema, create(CommandLookupResponseSchema, { found: true, phase, detail: "lookup" })));
  const calls = (f: ReturnType<typeof vi.fn>) => f.mock.calls.map(([url, init]) => `${(init as RequestInit | undefined)?.method ?? "GET"} ${String(url).split("?")[0]}`);
  beforeEach(() => { useCaptureControl.setState({ process: process("STOPPED"), busy: false, receipt: null }); });

  it("shows a refusal as REJECTED and asks the ledger nothing more", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(session()).mockResolvedValueOnce(response(CommandStatus.REJECTED, "Capture state unknown or stopping; no start issued")).mockResolvedValueOnce(stopped());
    vi.stubGlobal("fetch", fetch);
    await captureCommand("project0.capture.start");
    expect(calls(fetch)).toEqual(["GET /api/v1/session", "POST /api/v1/command", "GET /api/v1/project0/process"]);
    expect(useCaptureControl.getState()).toMatchObject({ busy: false, receipt: { phase: "REJECTED" }, process: { state: "STOPPED" } });
  });
  it("an accepted command stays PENDING until the ledger says otherwise", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(session()).mockResolvedValueOnce(response(CommandStatus.ACCEPTED, "")).mockResolvedValueOnce(lookup(CommandPhase.PENDING)).mockResolvedValueOnce(stopped());
    vi.stubGlobal("fetch", fetch);
    await captureCommand("project0.capture.start");
    expect(useCaptureControl.getState()).toMatchObject({ busy: true, receipt: { phase: "PENDING" }, process: { state: "STOPPED" } });
    expect(captureButtons(useCaptureControl.getState().process, useCaptureControl.getState().busy).start.enabled).toBe(false);
  });
  it("recovers a lost acknowledgement by lookup, never by a second POST", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(session()).mockRejectedValueOnce(new TypeError("connection reset")).mockResolvedValueOnce(lookup(CommandPhase.PENDING_RECONCILE)).mockResolvedValueOnce(stopped());
    vi.stubGlobal("fetch", fetch);
    await captureCommand("project0.capture.stop");
    expect(calls(fetch)).toEqual(["GET /api/v1/session", "POST /api/v1/command", "GET /api/v1/command", "GET /api/v1/project0/process"]);
    expect(useCaptureControl.getState().receipt).toMatchObject({ phase: "PENDING_RECONCILE" });
  });
  it("never sends while a command is in flight or before a session exists", async () => {
    useCaptureControl.setState({ busy: true });
    const fetch = vi.fn(); vi.stubGlobal("fetch", fetch);
    await captureCommand("project0.capture.start");
    expect(fetch).not.toHaveBeenCalled();
    useCaptureControl.setState({ busy: false });
    const refused = vi.fn().mockResolvedValueOnce(new Response(null, { status: 401 })).mockResolvedValueOnce(stopped());
    vi.stubGlobal("fetch", refused);
    await captureCommand("project0.capture.start");
    expect(calls(refused)).toEqual(["GET /api/v1/session", "GET /api/v1/project0/process"]);
    expect(useCaptureControl.getState()).toMatchObject({ busy: false, receipt: { phase: "FAILED" } });
  });
});

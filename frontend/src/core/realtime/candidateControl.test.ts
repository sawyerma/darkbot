import { describe, it, expect, beforeEach, vi } from "vitest";
import { create, toBinary } from "@bufbuild/protobuf";
import { CommandLookupResponseSchema, CommandPhase, CommandResponseSchema, CommandStatus } from "../generated/darkbot/control/v1/commands_pb";
import { candidateCommand, candidateMirrorShows, useCandidateControl } from "./candidateControl";

const session = () => new Response(null, { status: 200, headers: { "x-csrf-token": "csrf" } });
const accepted = (phase: CommandPhase) => new Response(toBinary(CommandResponseSchema, create(CommandResponseSchema, { status: CommandStatus.ACCEPTED, phase })));
const lookup = (phase: CommandPhase) => new Response(toBinary(CommandLookupResponseSchema, create(CommandLookupResponseSchema, { found: true, phase })));

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

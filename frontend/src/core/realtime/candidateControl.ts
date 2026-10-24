import { create as message, fromBinary, toBinary } from "@bufbuild/protobuf";
import { create } from "zustand";
import { CommandRequestSchema, CommandResponseSchema, CommandLookupResponseSchema, CommandStatus, CommandPhase } from "../generated/darkbot/control/v1/commands_pb";
import { ProjectId } from "../generated/darkbot/control/v1/topics_pb";
// G0.7 A/B candidate request (offline only). The gateway records it; the
// native store applies or rejects it at the next desktop sync. ACCEPTED is
// never an effective switch, approval, signature or send.
export type CandidateProfile = "SELF_A" | "SELF_B";
interface Receipt { id: string; phase: string; detail: string }
/** busy: no decided ledger phase yet. awaiting: recorded, the native mirror must still show it. */
interface CandidateControlState { busy: boolean; receipt: Receipt | null; awaiting: string | null }
export const useCandidateControl = create<CandidateControlState>(() => ({ busy: false, receipt: null, awaiting: null }));
const options: RequestInit = { credentials: "same-origin", cache: "no-store" };
async function csrf(): Promise<string> {
  const r = await fetch("/api/v1/session", { ...options, signal: AbortSignal.timeout(4000) });
  const token = r.headers.get("x-csrf-token");
  if (!r.ok || !token) throw new Error("Session unavailable");
  return token;
}
/** PENDING and PENDING_RECONCILE stay open and locked until the native decision is known. */
function decide(id: string, phase: CommandPhase, detail: string): void {
  const name = CommandPhase[phase] ?? "UNKNOWN";
  const open = phase === CommandPhase.PENDING || phase === CommandPhase.PENDING_RECONCILE;
  useCandidateControl.setState({ busy: open, awaiting: phase === CommandPhase.COMPLETED ? id : null, receipt: { id, phase: name, detail } });
}
/** The pushed native mirror shows this request: from now on its own state locks the selector. */
export function candidateMirrorShows(requestId: string | undefined): void {
  const { awaiting } = useCandidateControl.getState();
  if (awaiting && requestId === awaiting) useCandidateControl.setState({ awaiting: null });
}
/** One targeted lookup, only after a lost acknowledgement; never a polling loop. */
async function lookup(id: string): Promise<void> {
  const r = await fetch(`/api/v1/command?project=${ProjectId.BSC}&request_id=${encodeURIComponent(id)}`, { ...options, signal: AbortSignal.timeout(6000) });
  if (!r.ok) throw new Error(`Command lookup HTTP ${r.status}`);
  const c = fromBinary(CommandLookupResponseSchema, new Uint8Array(await r.arrayBuffer()));
  decide(id, c.found ? c.phase : CommandPhase.UNKNOWN, c.detail);
}
/** One request per user action; the generation is the CAS of the displayed native state. */
export async function candidateCommand(profile: CandidateProfile, generation: bigint): Promise<void> {
  const state = useCandidateControl.getState();
  if (state.busy || state.awaiting) return;
  const id = crypto.randomUUID();
  useCandidateControl.setState({ busy: true, awaiting: null, receipt: { id, phase: "SENDING", detail: profile } });
  let submitted = false;
  try {
    const token = await csrf();
    const body = toBinary(CommandRequestSchema, message(CommandRequestSchema, {
      project: ProjectId.BSC, requestId: id, action: "bsc.candidate.select", resourceId: profile,
      expected: { version: { case: "profileVersion", value: generation } }, issuedAtUnixNs: BigInt(Date.now()) * 1_000_000n,
    }));
    submitted = true;
    const r = await fetch("/api/v1/command", { ...options, method: "POST", headers: { "Content-Type": "application/x-protobuf", "X-CSRF-Token": token }, body, signal: AbortSignal.timeout(60000) });
    if (!r.ok) throw new Error(`Command HTTP ${r.status}`);
    const c = fromBinary(CommandResponseSchema, new Uint8Array(await r.arrayBuffer()));
    if (c.status !== CommandStatus.ACCEPTED) useCandidateControl.setState({ busy: false, receipt: { id, phase: "REJECTED", detail: c.detail } });
    else decide(id, c.phase, c.detail);
  } catch (error) {
    if (submitted) {
      try { await lookup(id); return; } catch { /* unknown outcome, never replay POST */ }
      useCandidateControl.setState({ busy: true, receipt: { id, phase: "UNKNOWN", detail: error instanceof Error ? error.message : String(error) } });
      return;
    }
    useCandidateControl.setState({ busy: false, receipt: { id, phase: "FAILED", detail: error instanceof Error ? error.message : String(error) } });
  }
}

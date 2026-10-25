import { create as message, fromBinary, toBinary } from "@bufbuild/protobuf";
import { create } from "zustand";
import { CommandRequestSchema, CommandResponseSchema, CommandLookupResponseSchema, CommandStatus, CommandPhase } from "../generated/darkbot/control/v1/commands_pb";
import { ProjectId } from "../generated/darkbot/control/v1/topics_pb";
// BSC source mode request (plan §5a.2, order V2 §5). The native bsc_runtime
// source owner applies or refuses it with the shown generation as CAS; its
// one-line receipt is the detail. A requested LIVE stays effective BLOCKED
// in the native owner: no request here can arm, sign or send.
export type SourceMode = "OFF" | "SHADOW" | "LIVE";
interface Receipt { id: string; phase: string; detail: string }
interface SourceControlState { busy: boolean; receipt: Receipt | null }
export const useSourceControl = create<SourceControlState>(() => ({ busy: false, receipt: null }));
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
  useSourceControl.setState({ busy: open, receipt: { id, phase: name, detail } });
}
/** One targeted lookup, only after a lost acknowledgement; never a polling loop. */
async function lookup(id: string): Promise<void> {
  const r = await fetch(`/api/v1/command?project=${ProjectId.BSC}&request_id=${encodeURIComponent(id)}`, { ...options, signal: AbortSignal.timeout(6000) });
  if (!r.ok) throw new Error(`Command lookup HTTP ${r.status}`);
  const c = fromBinary(CommandLookupResponseSchema, new Uint8Array(await r.arrayBuffer()));
  decide(id, c.found ? c.phase : CommandPhase.UNKNOWN, c.detail);
}
/** One request per user action; the generation is the CAS of the displayed native state. */
export async function sourceCommand(source: string, mode: SourceMode, generation: bigint): Promise<void> {
  if (useSourceControl.getState().busy) return;
  const id = crypto.randomUUID();
  useSourceControl.setState({ busy: true, receipt: { id, phase: "SENDING", detail: `${source} ${mode}` } });
  let submitted = false;
  try {
    const token = await csrf();
    const body = toBinary(CommandRequestSchema, message(CommandRequestSchema, {
      project: ProjectId.BSC, requestId: id, action: "bsc.source.mode.set", resourceId: `${source}:${mode}`,
      expected: { version: { case: "configVersion", value: generation } }, issuedAtUnixNs: BigInt(Date.now()) * 1_000_000n,
    }));
    submitted = true;
    const r = await fetch("/api/v1/command", { ...options, method: "POST", headers: { "Content-Type": "application/x-protobuf", "X-CSRF-Token": token }, body, signal: AbortSignal.timeout(60000) });
    if (!r.ok) throw new Error(`Command HTTP ${r.status}`);
    const c = fromBinary(CommandResponseSchema, new Uint8Array(await r.arrayBuffer()));
    if (c.status !== CommandStatus.ACCEPTED) { useSourceControl.setState({ busy: false, receipt: { id, phase: "REJECTED", detail: c.detail } }); return; }
    // The decided phase arrives with the response; its receipt line is the ledger detail.
    await lookup(id).catch(() => decide(id, c.phase, c.detail));
  } catch (error) {
    if (submitted) {
      try { await lookup(id); return; } catch { /* unknown outcome, never replay POST */ }
      useSourceControl.setState({ busy: true, receipt: { id, phase: "UNKNOWN", detail: error instanceof Error ? error.message : String(error) } });
      return;
    }
    useSourceControl.setState({ busy: false, receipt: { id, phase: "FAILED", detail: error instanceof Error ? error.message : String(error) } });
  }
}

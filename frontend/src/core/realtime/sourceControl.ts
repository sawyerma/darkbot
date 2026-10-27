import { create as message, fromBinary, toBinary } from "@bufbuild/protobuf";
import { create } from "zustand";
import { CommandRequestSchema, CommandResponseSchema, CommandLookupResponseSchema, CommandStatus, CommandPhase } from "../generated/darkbot/control/v1/commands_pb";
import { ProjectId } from "../generated/darkbot/control/v1/topics_pb";
// BSC owner requests (plan §5a.2, v9 §2, order V2 §5, plan 27.10.): source,
// component and venue modes and the wallet and economic switches of the one
// native bsc_runtime owner, which applies or refuses each with the shown
// generation as CAS; its one-line receipt is the detail. A requested LIVE
// stays effective BLOCKED in the native owner and an ENABLED switch is a
// permission only: no request here can arm, sign or send.
export type SourceMode = "OFF" | "SHADOW" | "LIVE";
export type SwitchMode = "DISABLED" | "ENABLED";
/** The allowlisted gateway actions of the owner and the CAS each states. */
export type OwnerAction = "bsc.source.mode.set" | "bsc.venue.mode.set" | "bsc.wallet.set" | "bsc.economic.set" | "bsc.runtime.mode.set" | "bsc.attempt.limit.set";
/** `origin` names the control group that sent it; each shows only its own receipt. */
interface Receipt { id: string; phase: string; detail: string; origin: string }
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
  const origin = useSourceControl.getState().receipt?.origin ?? "";
  useSourceControl.setState({ busy: open, receipt: { id, phase: name, detail, origin } });
}
/** One targeted lookup, only after a lost acknowledgement; never a polling loop. */
async function lookup(id: string): Promise<void> {
  const r = await fetch(`/api/v1/command?project=${ProjectId.BSC}&request_id=${encodeURIComponent(id)}`, { ...options, signal: AbortSignal.timeout(6000) });
  if (!r.ok) throw new Error(`Command lookup HTTP ${r.status}`);
  const c = fromBinary(CommandLookupResponseSchema, new Uint8Array(await r.arrayBuffer()));
  decide(id, c.found ? c.phase : CommandPhase.UNKNOWN, c.detail);
}
/** One request per user action; the generation is the CAS of the displayed native state
 *  (`walletPolicyVersion` for a wallet switch, `configVersion` otherwise). */
export async function ownerCommand(origin: string, action: OwnerAction, id: string, mode: string, generation: bigint): Promise<void> {
  if (useSourceControl.getState().busy) return;
  const requestId = crypto.randomUUID();
  useSourceControl.setState({ busy: true, receipt: { id: requestId, phase: "SENDING", detail: `${id} ${mode}`, origin } });
  let submitted = false;
  try {
    const token = await csrf();
    const version = action === "bsc.wallet.set"
      ? { case: "walletPolicyVersion" as const, value: generation }
      : { case: "configVersion" as const, value: generation };
    const body = toBinary(CommandRequestSchema, message(CommandRequestSchema, {
      project: ProjectId.BSC, requestId, action, resourceId: `${id}:${mode}`,
      expected: { version }, issuedAtUnixNs: BigInt(Date.now()) * 1_000_000n,
    }));
    submitted = true;
    const r = await fetch("/api/v1/command", { ...options, method: "POST", headers: { "Content-Type": "application/x-protobuf", "X-CSRF-Token": token }, body, signal: AbortSignal.timeout(60000) });
    if (!r.ok) throw new Error(`Command HTTP ${r.status}`);
    const c = fromBinary(CommandResponseSchema, new Uint8Array(await r.arrayBuffer()));
    if (c.status !== CommandStatus.ACCEPTED) { useSourceControl.setState({ busy: false, receipt: { id: requestId, phase: "REJECTED", detail: c.detail, origin } }); return; }
    // The decided phase arrives with the response; its receipt line is the ledger detail.
    await lookup(requestId).catch(() => decide(requestId, c.phase, c.detail));
  } catch (error) {
    if (submitted) {
      try { await lookup(requestId); return; } catch { /* unknown outcome, never replay POST */ }
      useSourceControl.setState({ busy: true, receipt: { id: requestId, phase: "UNKNOWN", detail: error instanceof Error ? error.message : String(error), origin } });
      return;
    }
    useSourceControl.setState({ busy: false, receipt: { id: requestId, phase: "FAILED", detail: error instanceof Error ? error.message : String(error), origin } });
  }
}
/** A source or component mode request (the existing action). */
export const sourceCommand = (source: string, mode: SourceMode, generation: bigint, origin = "bsc-source") =>
  ownerCommand(origin, "bsc.source.mode.set", source, mode, generation);

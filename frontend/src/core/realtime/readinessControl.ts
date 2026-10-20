import { create as message, fromBinary, toBinary } from "@bufbuild/protobuf";
import { create } from "zustand";
import { CommandRequestSchema, CommandResponseSchema, CommandLookupResponseSchema, CommandStatus, CommandPhase } from "../generated/darkbot/control/v1/commands_pb";
import { ProjectId } from "../generated/darkbot/control/v1/topics_pb";
export type ProcessState = "RUNNING" | "STOPPED" | "STOPPING" | "FAILED" | "UNKNOWN";
export interface ReadinessProcess { state: ProcessState; run_id: string; pid: number | null; reason: string; observed_unix_ms: number; controller_available: boolean }
export type ReadinessAction = "bsc.readiness.start" | "bsc.readiness.stop";
interface Receipt { id: string; phase: string; detail: string }
interface ControlState { process: ReadinessProcess | null; busy: boolean; receipt: Receipt | null }
export const useReadinessControl = create<ControlState>(() => ({ process: null, busy: false, receipt: null }));
const receiptKey = "darkbot.bsc.lastCommand";
const options: RequestInit = { credentials: "same-origin", cache: "no-store" };
async function bootstrap(): Promise<string> {
  const r = await fetch("/api/v1/session", { ...options, signal: AbortSignal.timeout(4000) });
  const token = r.headers.get("x-csrf-token");
  if (!r.ok || !token) throw new Error("Session unavailable");
  return token;
}
async function refreshProcess(): Promise<void> {
  try {
    const r = await fetch("/api/v1/bsc/process", { ...options, signal: AbortSignal.timeout(4000) });
    if (r.status === 401) await bootstrap();
    if (!r.ok) throw new Error(`Process status HTTP ${r.status}`);
    const p = await r.json() as ReadinessProcess;
    if (!["RUNNING", "STOPPED", "STOPPING", "FAILED", "UNKNOWN"].includes(p.state)) throw new Error("Invalid process status");
    useReadinessControl.setState({ process: p });
  } catch { useReadinessControl.setState({ process: null }); }
}
async function lookup(id: string): Promise<void> {
  const r = await fetch(`/api/v1/command?project=${ProjectId.BSC}&request_id=${encodeURIComponent(id)}`, { ...options, signal: AbortSignal.timeout(6000) });
  if (!r.ok) throw new Error(`Command lookup HTTP ${r.status}`);
  const c = fromBinary(CommandLookupResponseSchema, new Uint8Array(await r.arrayBuffer()));
  const phase = c.found ? CommandPhase[c.phase] ?? "UNKNOWN" : "UNKNOWN";
  useReadinessControl.setState({ receipt: { id, phase, detail: c.detail }, busy: c.found && c.phase === CommandPhase.PENDING });
}
/** One watcher per mounted Overview; process truth comes from authenticated launchd observation. */
export function watchReadinessControl(): () => void {
  let done = false; let timer: ReturnType<typeof setTimeout> | undefined;
  useReadinessControl.setState({ process: null });
  const tick = async () => {
    if (done) return;
    if (document.visibilityState === "visible") {
      await refreshProcess();
      const receipt = useReadinessControl.getState().receipt;
      if (receipt?.phase === "PENDING") {
        try { await lookup(receipt.id); } catch { /* retain command identity; retry lookup, never POST */ }
      }
    }
    if (!done) timer = setTimeout(() => void tick(), 2000);
  };
  void bootstrap().then(async () => {
    if (done) return;
    try { const id = sessionStorage.getItem(receiptKey); if (id) await lookup(id); } catch { /* storage/lookup may be unavailable */ }
    await tick();
  }).catch(() => { if (!done) timer = setTimeout(() => void tick(), 2000); });
  return () => { done = true; clearTimeout(timer); };
}
export async function readinessCommand(action: ReadinessAction): Promise<void> {
  if (useReadinessControl.getState().busy) return;
  const id = crypto.randomUUID();
  try { sessionStorage.setItem(receiptKey, id); } catch { /* commands also work without browser storage */ }
  useReadinessControl.setState({ busy: true, receipt: { id, phase: "SENDING", detail: action } });
  let submitted = false;
  try {
    const csrf = await bootstrap();
    const body = toBinary(CommandRequestSchema, message(CommandRequestSchema, { project: ProjectId.BSC, resourceId: useReadinessControl.getState().process?.run_id ?? "", requestId: id, action, issuedAtUnixNs: BigInt(Date.now()) * 1_000_000n }));
    submitted = true;
    const r = await fetch("/api/v1/command", { ...options, method: "POST", headers: { "Content-Type": "application/x-protobuf", "X-CSRF-Token": csrf }, body, signal: AbortSignal.timeout(60000) });
    if (!r.ok) throw new Error(`Command HTTP ${r.status}`);
    const c = fromBinary(CommandResponseSchema, new Uint8Array(await r.arrayBuffer()));
    if (c.status !== CommandStatus.ACCEPTED) {
      useReadinessControl.setState({ busy: false, receipt: { id, phase: "REJECTED", detail: c.detail } });
    } else { await lookup(id); }
  } catch (error) {
    if (submitted) {
      try { await lookup(id); await refreshProcess(); return; } catch { /* unknown outcome, never replay POST */ }
    }
    useReadinessControl.setState({ busy: false, receipt: { id, phase: submitted ? "UNKNOWN" : "FAILED", detail: error instanceof Error ? error.message : String(error) } });
  }
  await refreshProcess();
}

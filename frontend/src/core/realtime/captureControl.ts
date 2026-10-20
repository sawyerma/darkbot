import { create as message, fromBinary, toBinary } from "@bufbuild/protobuf";
import { create } from "zustand";
import { CommandRequestSchema, CommandResponseSchema, CommandLookupResponseSchema, CommandStatus, CommandPhase } from "../generated/darkbot/control/v1/commands_pb";
import { ProjectId } from "../generated/darkbot/control/v1/topics_pb";
export type ProcessState = "RUNNING" | "STOPPED" | "STOPPING" | "FAILED" | "UNKNOWN";
export interface CaptureProcess { state: ProcessState; run_id: string; pid: number | null; reason: string; observed_unix_ms: number; controller_available: boolean }
export type CaptureAction = "project0.capture.start" | "project0.capture.stop";
interface Receipt { id: string; phase: string; detail: string }
interface ControlState { process: CaptureProcess | null; busy: boolean; receipt: Receipt | null }
export const useCaptureControl = create<ControlState>(() => ({ process: null, busy: false, receipt: null }));
const receiptKey = "darkbot.project0.lastCommand";
const options: RequestInit = { credentials: "same-origin", cache: "no-store" };
async function bootstrap(): Promise<string> {
  const r = await fetch("/api/v1/session", { ...options, signal: AbortSignal.timeout(4000) });
  const token = r.headers.get("x-csrf-token");
  if (!r.ok || !token) throw new Error("Session unavailable");
  return token;
}
async function refreshProcess(): Promise<void> {
  try {
    const r = await fetch("/api/v1/project0/process", { ...options, signal: AbortSignal.timeout(4000) });
    if (r.status === 401) await bootstrap();
    if (!r.ok) throw new Error(`Process status HTTP ${r.status}`);
    const p = await r.json() as CaptureProcess;
    if (!["RUNNING", "STOPPED", "STOPPING", "FAILED", "UNKNOWN"].includes(p.state)) throw new Error("Invalid process status");
    useCaptureControl.setState({ process: p });
  } catch { useCaptureControl.setState({ process: null }); }
}
async function lookup(id: string): Promise<void> {
  const r = await fetch(`/api/v1/command?project=${ProjectId.PROJECT_0}&request_id=${encodeURIComponent(id)}`, { ...options, signal: AbortSignal.timeout(6000) });
  if (!r.ok) throw new Error(`Command lookup HTTP ${r.status}`);
  const c = fromBinary(CommandLookupResponseSchema, new Uint8Array(await r.arrayBuffer()));
  const phase = c.found ? CommandPhase[c.phase] ?? "UNKNOWN" : "UNKNOWN";
  useCaptureControl.setState({ receipt: { id, phase, detail: c.detail }, busy: c.found && c.phase === CommandPhase.PENDING });
}
/** One watcher per mounted Overview; process truth comes from authenticated launchd observation. */
export function watchCaptureControl(): () => void {
  let done = false; let timer: ReturnType<typeof setTimeout> | undefined;
  useCaptureControl.setState({ process: null });
  const tick = async () => {
    if (done) return;
    if (document.visibilityState === "visible") {
      await refreshProcess();
      const receipt = useCaptureControl.getState().receipt;
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
export async function captureCommand(action: CaptureAction): Promise<void> {
  if (useCaptureControl.getState().busy) return;
  const id = crypto.randomUUID();
  try { sessionStorage.setItem(receiptKey, id); } catch { /* commands also work without browser storage */ }
  useCaptureControl.setState({ busy: true, receipt: { id, phase: "SENDING", detail: action } });
  let submitted = false;
  try {
    const csrf = await bootstrap();
    const body = toBinary(CommandRequestSchema, message(CommandRequestSchema, { project: ProjectId.PROJECT_0, requestId: id, action, issuedAtUnixNs: BigInt(Date.now()) * 1_000_000n }));
    submitted = true;
    const r = await fetch("/api/v1/command", { ...options, method: "POST", headers: { "Content-Type": "application/x-protobuf", "X-CSRF-Token": csrf }, body, signal: AbortSignal.timeout(60000) });
    if (!r.ok) throw new Error(`Command HTTP ${r.status}`);
    const c = fromBinary(CommandResponseSchema, new Uint8Array(await r.arrayBuffer()));
    if (c.status !== CommandStatus.ACCEPTED) {
      useCaptureControl.setState({ busy: false, receipt: { id, phase: "REJECTED", detail: c.detail } });
    } else { await lookup(id); }
  } catch (error) {
    if (submitted) {
      try { await lookup(id); await refreshProcess(); return; } catch { /* unknown outcome, never replay POST */ }
    }
    useCaptureControl.setState({ busy: false, receipt: { id, phase: submitted ? "UNKNOWN" : "FAILED", detail: error instanceof Error ? error.message : String(error) } });
  }
  await refreshProcess();
}
export interface LogPart { source: string; text: string }
export async function captureLogs(source: "project0" | "bsc" | "all"): Promise<LogPart[]> {
  await bootstrap();
  const r = await fetch(`/api/v1/logs?source=${source}`, { ...options, signal: AbortSignal.timeout(5000) });
  if (!r.ok) throw new Error(`Logs HTTP ${r.status}`);
  return await r.json() as LogPart[];
}

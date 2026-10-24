import { create as message, fromBinary, toBinary } from "@bufbuild/protobuf";
import { create } from "zustand";
import { CommandRequestSchema, CommandResponseSchema, CommandLookupResponseSchema, CommandStatus, CommandPhase } from "../generated/darkbot/control/v1/commands_pb";
import { ProjectId } from "../generated/darkbot/control/v1/topics_pb";
export type ProcessState = "RUNNING" | "STOPPED" | "STOPPING" | "FAILED" | "UNKNOWN";
export interface ReadinessProcess { state: ProcessState; run_id: string; pid: number | null; reason: string; observed_unix_ms: number; controller_available: boolean }
export type ReadinessAction = "bsc.readiness.start" | "bsc.readiness.stop";
/** A supervisor job as pushed in bsc.status or overview.aggregates. */
export interface PushedJob { state: string; launchId: string; resultCode: string; controllerAvailable?: boolean }
export type JobSource = "bsc" | "overview";
interface Receipt { id: string; phase: string; detail: string }
/** The effect a decided command must show in the pushed job before the control unlocks. */
interface Awaiting { action: ReadinessAction; runId: string }
interface ControlState { process: ReadinessProcess | null; busy: boolean; receipt: Receipt | null; sources: Record<JobSource, ReadinessProcess | null>; awaiting: Awaiting | null }
export const useReadinessControl = create<ControlState>(() => ({ process: null, busy: false, receipt: null, sources: { bsc: null, overview: null }, awaiting: null }));
const receiptKey = "darkbot.bsc.lastCommand";
const options: RequestInit = { credentials: "same-origin", cache: "no-store" };
const STATES: ProcessState[] = ["RUNNING", "STOPPED", "STOPPING", "FAILED", "UNKNOWN"];
async function bootstrap(): Promise<string> {
  const r = await fetch("/api/v1/session", { ...options, signal: AbortSignal.timeout(4000) });
  const token = r.headers.get("x-csrf-token");
  if (!r.ok || !token) throw new Error("Session unavailable");
  return token;
}
function forget(): void { try { sessionStorage.removeItem(receiptKey); } catch { /* storage may be unavailable */ } }
/** Whether the pushed job shows the effect of the awaited command. */
function settled(a: Awaiting, p: ReadinessProcess | null): boolean {
  if (!p) return false;
  // A start is only offered from STOPPED/FAILED, so a running job or a new launch is its effect.
  return a.action === "bsc.readiness.start"
    ? p.run_id !== a.runId || p.state === "RUNNING" || p.state === "STOPPING"
    : p.run_id !== a.runId || p.state === "STOPPED" || p.state === "FAILED";
}
/**
 * Process truth comes only from the pushed supervisor job of a live topic (bsc.status on /bsc,
 * overview.aggregates on /); a stale, detached or missing source makes it UNKNOWN. No HTTP polling.
 */
export function applyReadinessJob(source: JobSource, job: PushedJob | null): void {
  const value: ReadinessProcess | null = job ? {
    state: STATES.includes(job.state as ProcessState) ? job.state as ProcessState : "UNKNOWN",
    run_id: job.launchId, pid: null, reason: `launchd readiness job${job.resultCode ? ` · result ${job.resultCode}` : ""}`,
    observed_unix_ms: Date.now(), controller_available: job.controllerAvailable === true,
  } : null;
  useReadinessControl.setState(s => {
    const sources = { ...s.sources, [source]: value };
    const process = sources.bsc ?? sources.overview;
    if (s.awaiting && settled(s.awaiting, process)) { forget(); return { sources, process, awaiting: null, busy: false }; }
    return { sources, process };
  });
}
/** Applies a decided ledger phase; open or undecided commands keep the control locked. */
function decide(id: string, phase: CommandPhase, detail: string, awaiting: Awaiting): void {
  const name = CommandPhase[phase] ?? "UNKNOWN";
  if (phase === CommandPhase.FAILED) { forget(); useReadinessControl.setState({ busy: false, awaiting: null, receipt: { id, phase: name, detail } }); return; }
  if (phase === CommandPhase.COMPLETED || phase === CommandPhase.PENDING || phase === CommandPhase.PENDING_RECONCILE) {
    useReadinessControl.setState(s => settled(awaiting, s.process) && phase !== CommandPhase.PENDING_RECONCILE
      ? (forget(), { busy: false, awaiting: null, receipt: { id, phase: name, detail } })
      : { busy: true, awaiting, receipt: { id, phase: name, detail } });
    return;
  }
  // Not recorded by the gateway: nothing was dispatched, the control is free again.
  forget(); useReadinessControl.setState({ busy: false, awaiting: null, receipt: { id, phase: name, detail } });
}
/** One targeted lookup, only for a lost acknowledgement or an unresolved command after reload. */
async function lookup(id: string, awaiting: Awaiting): Promise<void> {
  const r = await fetch(`/api/v1/command?project=${ProjectId.BSC}&request_id=${encodeURIComponent(id)}`, { ...options, signal: AbortSignal.timeout(6000) });
  if (!r.ok) throw new Error(`Command lookup HTTP ${r.status}`);
  const c = fromBinary(CommandLookupResponseSchema, new Uint8Array(await r.arrayBuffer()));
  decide(id, c.found ? c.phase : CommandPhase.UNKNOWN, c.detail, awaiting);
}
let recovered = false;
/** After a reload, recovers an unresolved command once; resolved commands are never looked up. */
export function recoverReadinessCommand(): void {
  if (recovered) return;
  recovered = true;
  let stored: Awaiting & { id: string } | null = null;
  try { const raw = sessionStorage.getItem(receiptKey); stored = raw ? JSON.parse(raw) as Awaiting & { id: string } : null; } catch { stored = null; }
  if (!stored?.id || useReadinessControl.getState().receipt) return;
  const { id, action, runId } = stored;
  useReadinessControl.setState({ busy: true, awaiting: { action, runId }, receipt: { id, phase: "RECOVERING", detail: action } });
  void bootstrap().then(() => lookup(id, { action, runId })).catch(error => useReadinessControl.setState({ busy: true, receipt: { id, phase: "UNKNOWN", detail: error instanceof Error ? error.message : String(error) } }));
}
export async function readinessCommand(action: ReadinessAction): Promise<void> {
  if (useReadinessControl.getState().busy) return;
  const id = crypto.randomUUID();
  const runId = useReadinessControl.getState().process?.run_id ?? "";
  const awaiting: Awaiting = { action, runId };
  try { sessionStorage.setItem(receiptKey, JSON.stringify({ id, ...awaiting })); } catch { /* commands also work without browser storage */ }
  useReadinessControl.setState({ busy: true, awaiting: null, receipt: { id, phase: "SENDING", detail: action } });
  let submitted = false;
  try {
    const csrf = await bootstrap();
    const body = toBinary(CommandRequestSchema, message(CommandRequestSchema, { project: ProjectId.BSC, resourceId: runId, requestId: id, action, issuedAtUnixNs: BigInt(Date.now()) * 1_000_000n }));
    submitted = true;
    const r = await fetch("/api/v1/command", { ...options, method: "POST", headers: { "Content-Type": "application/x-protobuf", "X-CSRF-Token": csrf }, body, signal: AbortSignal.timeout(60000) });
    if (!r.ok) throw new Error(`Command HTTP ${r.status}`);
    const c = fromBinary(CommandResponseSchema, new Uint8Array(await r.arrayBuffer()));
    if (c.status !== CommandStatus.ACCEPTED) { forget(); useReadinessControl.setState({ busy: false, receipt: { id, phase: "REJECTED", detail: c.detail } }); }
    else decide(id, c.phase, c.detail, awaiting);
  } catch (error) {
    if (submitted) {
      // Lost acknowledgement: one targeted lookup, never a second POST.
      try { await lookup(id, awaiting); return; } catch { /* outcome unknown; stays locked */ }
      useReadinessControl.setState({ busy: true, awaiting, receipt: { id, phase: "UNKNOWN", detail: error instanceof Error ? error.message : String(error) } });
      return;
    }
    forget(); useReadinessControl.setState({ busy: false, receipt: { id, phase: "FAILED", detail: error instanceof Error ? error.message : String(error) } });
  }
}

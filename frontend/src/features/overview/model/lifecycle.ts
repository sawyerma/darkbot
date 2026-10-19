import type { ProjectKey } from "@/app/projectRegistry";
import type { CaptureProcess, CaptureAction, ProcessState } from "@/core/realtime/captureControl";
export type { ProcessState };
export interface ProcessStatusSource { state: ProcessState; runId: string }
export interface ProcessIndicator { state: ProcessState; label: string; reason: string }
export function processIndicator(source: ProcessStatusSource | null, sourceTopicStatus = "IDLE"): ProcessIndicator {
  if (!source || sourceTopicStatus !== "LIVE" || !source.runId) return { state: "UNKNOWN", label: "Process · UNKNOWN", reason: "No fresh supervisor observation" };
  return { state: source.state, label: `Process · ${source.state}`, reason: `Run ${source.runId}` };
}
export function captureIndicator(p: CaptureProcess | null): ProcessIndicator {
  return p ? { state: p.state, label: `Process · ${p.state}`, reason: p.reason } : processIndicator(null);
}
export interface DisabledControl { readonly enabled: false; readonly reason: string }
export interface EnabledControl { readonly enabled: true; readonly label: string; readonly action: CaptureAction; readonly confirm?: string }
export type Control = DisabledControl | EnabledControl;
const NO_OWNER = "No qualified process controller for this project";
const STOP_CONFIRM = "Stop the Project 0 read-only capture? Its journal will be drained and sealed. The gateway remains running.";
export function projectControls(key: ProjectKey, process: ProcessIndicator = processIndicator(null)): { startStop: Control; logs: { enabled: boolean; reason: string } } {
  const logs = { enabled: key === "project0", reason: NO_OWNER };
  if (key !== "project0") return { startStop: { enabled: false, reason: NO_OWNER }, logs };
  if (process.state === "RUNNING") return { startStop: { enabled: true, label: "Stop", action: "project0.capture.stop", confirm: STOP_CONFIRM }, logs };
  if (process.state === "STOPPED" || process.state === "FAILED") return { startStop: { enabled: true, label: "Start", action: "project0.capture.start" }, logs };
  return { startStop: { enabled: false, reason: process.state === "STOPPING" ? "Journal drain and seal in progress" : "Process state unknown" }, logs };
}
export function stopAllControl(process: ProcessIndicator): Control {
  return process.state === "RUNNING" ? { enabled: true, label: "Stop All", action: "project0.capture.stop", confirm: STOP_CONFIRM + " Only Project 0 has a managed controller." } : { enabled: false, reason: "No managed capture running" };
}

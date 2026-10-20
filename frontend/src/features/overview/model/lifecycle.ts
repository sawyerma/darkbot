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
export interface EnabledControl { readonly enabled: true; readonly label: string; readonly action: CaptureAction | "bsc.readiness.start" | "bsc.readiness.stop"; readonly confirm?: string }
export type Control = DisabledControl | EnabledControl;
const NO_OWNER = "No qualified process controller for this project";
const STOP_CONFIRM = "Die laufende Aufzeichnung wird geordnet beendet und das Journal versiegelt. Das Cockpit bleibt geöffnet.";
export function projectControls(key: ProjectKey, process: ProcessIndicator = processIndicator(null)): { startStop: Control; logs: { enabled: boolean; reason: string } } {
  const logs = { enabled: key === "project0" || key === "bsc", reason: NO_OWNER };
  if(key==="bsc") {
    if(process.state==="RUNNING")return {startStop:{enabled:true,label:"Stop readiness",action:"bsc.readiness.stop",confirm:"Die laufende Readiness-Prüfung wird geordnet abgebrochen. Das Gateway bleibt aktiv."},logs};
    if(process.state==="STOPPED"||process.state==="FAILED")return {startStop:{enabled:true,label:"Run readiness",action:"bsc.readiness.start"},logs};
    return {startStop:{enabled:false,reason:"Readiness state unknown or stopping"},logs};
  }
  if (key !== "project0") return { startStop: { enabled: false, reason: NO_OWNER }, logs };
  if (process.state === "RUNNING") return { startStop: { enabled: true, label: "Stop", action: "project0.capture.stop", confirm: STOP_CONFIRM }, logs };
  if (process.state === "STOPPED" || process.state === "FAILED") return { startStop: { enabled: true, label: "Start", action: "project0.capture.start" }, logs };
  return { startStop: { enabled: false, reason: process.state === "STOPPING" ? "Journal drain and seal in progress" : "Process state unknown" }, logs };
}
export function stopAllControl(process: ProcessIndicator): Control {
  return process.state === "RUNNING" ? { enabled: true, label: "Stop All · Project 0", action: "project0.capture.stop", confirm: STOP_CONFIRM + " Dies betrifft ausschließlich Project 0. Das Gateway läuft unabhängig weiter." } : { enabled: false, reason: "No managed capture running" };
}

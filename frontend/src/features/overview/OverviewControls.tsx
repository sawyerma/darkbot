import "@/shared/ui/graphite/control-dialog.css";
import { readinessCommand,useReadinessControl } from "@/core/realtime/readinessControl";
import { captureCommand, captureLogs, useCaptureControl, type LogPart } from "@/core/realtime/captureControl";
import { useEffect, useId, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import type { DisabledControl, EnabledControl, ProcessIndicator } from "./model/lifecycle";

/** A real, inert button: disabled by type, with its reason as tooltip and for screen readers. */
export function DisabledAction({ label, control, testId }: { label: string; control: DisabledControl; testId: string }) {
  const reasonId = useId();
  return (
    <>
      <span className="ov-disabled-action" title={control.reason}>
        <button type="button" className="ov-btn" disabled
          aria-describedby={reasonId} data-testid={testId}>{label}</button>
      </span>
      <span id={reasonId} className="ov-sr-only">{control.reason}</span>
    </>
  );
}

export function ProcessLine({ indicator, testId }: { indicator: ProcessIndicator; testId: string }) {
  return (
    <div className="ov-process" data-state={indicator.state} title={indicator.reason} data-testid={testId}>
      <span className="ov-dot" aria-hidden="true" />
      <span>{indicator.label}</span>
    </div>
  );
}


export function CommandAction({ control, testId }: { control: EnabledControl; testId: string }) {
  const capture = useCaptureControl(); const readiness = useReadinessControl();
  const isBsc=control.action.startsWith("bsc.");
  const {busy,process}=isBsc?readiness:capture;
  const command=()=>isBsc?readinessCommand(control.action as "bsc.readiness.start"|"bsc.readiness.stop"):captureCommand(control.action as "project0.capture.start"|"project0.capture.stop");
  const [open, setOpen] = useState(false);
  const canStop = !busy && process?.state === "RUNNING" && Boolean(control.confirm);
  // A new supervisor observation or another command invalidates an open confirmation.
  useEffect(() => { if (!canStop) setOpen(false); }, [canStop]);
  if (!control.confirm) return <button type="button" className="ov-btn" disabled={busy} data-testid={testId}
    onClick={() => void command()}>{busy ? "Working…" : control.label}</button>;
  return <AlertDialog.Root open={open && canStop} onOpenChange={setOpen}>
    <AlertDialog.Trigger asChild>
      <button type="button" className="ov-btn" disabled={!canStop} data-testid={testId}>{busy ? "Working…" : control.label}</button>
    </AlertDialog.Trigger>
    <AlertDialog.Portal>
      <AlertDialog.Overlay className="ov-log-overlay" />
      <AlertDialog.Content className="ov-confirm-view">
        <p className="ov-confirm-context">DarkBot · {isBsc?"BSC":"Project 0"}</p>
        <AlertDialog.Title>{isBsc?"Readiness-Prüfung stoppen?":"Capture stoppen?"}</AlertDialog.Title>
        <AlertDialog.Description>{control.confirm}</AlertDialog.Description>
        <div className="ov-confirm-actions">
          <AlertDialog.Cancel asChild><button type="button" className="ov-btn">Abbrechen</button></AlertDialog.Cancel>
          <AlertDialog.Action asChild><button type="button" className="ov-btn ov-btn-confirm" disabled={!canStop} onClick={() => {
            setOpen(false);
            if (canStop) void command();
          }}>{isBsc?"Prüfung stoppen":"Capture stoppen"}</button></AlertDialog.Action>
        </div>
      </AlertDialog.Content>
    </AlertDialog.Portal>
  </AlertDialog.Root>;
}
export function CommandReceipt() {
  const receipt = useCaptureControl(s => s.receipt);
  return receipt ? <p role="status" className="ov-note" data-testid="capture-command-receipt">{receipt.phase} · {receipt.detail}<br/>Command {receipt.id}</p> : null;
}
export function LogsAction({ source, label, testId }: { source: "project0" | "bsc" | "all"; label: string; testId: string }) {
  const [open, setOpen] = useState(false);
  const [logs, setLogs] = useState<LogPart[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const refresh = async () => {
    setBusy(true); setError("");
    try { setLogs(await captureLogs(source)); } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  };
  return <Dialog.Root open={open} onOpenChange={setOpen}>
    <Dialog.Trigger asChild><button type="button" className="ov-btn" data-testid={testId} onClick={() => void refresh()}>{label}</button></Dialog.Trigger>
    <Dialog.Portal>
      <Dialog.Overlay className="ov-log-overlay" />
      <Dialog.Content className="ov-log-view">
        <Dialog.Title>{label}</Dialog.Title>
        <Dialog.Description>Latest 200 lines per log · provider URLs and credential lines redacted</Dialog.Description>
        <Dialog.Close asChild><button type="button" className="ov-btn">Close logs</button></Dialog.Close>{" "}
        <button type="button" className="ov-btn" disabled={busy} onClick={() => void refresh()}>Refresh logs</button>
        {busy ? <p role="status">Loading…</p> : null}{error ? <p role="alert">{error}</p> : null}
        {logs.map(log => <section key={log.source}><h4>{log.source === "project0" ? "Project 0" : log.source}</h4><pre>{log.text || "Log is empty"}</pre></section>)}
      </Dialog.Content>
    </Dialog.Portal>
  </Dialog.Root>;
}

export function StopManagedAction(){
 const p0=useCaptureControl(); const bsc=useReadinessControl(); const [open,setOpen]=useState(false);
 const busy=p0.busy||bsc.busy;const running=p0.process?.state==="RUNNING"||bsc.process?.state==="RUNNING";
 const stop=async()=>{setOpen(false);const jobs:Promise<void>[]=[];if(useCaptureControl.getState().process?.state==="RUNNING")jobs.push(captureCommand("project0.capture.stop"));if(useReadinessControl.getState().process?.state==="RUNNING")jobs.push(readinessCommand("bsc.readiness.stop"));await Promise.all(jobs);};
 return <AlertDialog.Root open={open&&running&&!busy} onOpenChange={setOpen}><AlertDialog.Trigger asChild><button className="ov-btn" disabled={busy||!running} data-testid="overview-stop-all">Stop All · P0 + BSC readiness</button></AlertDialog.Trigger><AlertDialog.Portal><AlertDialog.Overlay className="ov-log-overlay"/><AlertDialog.Content className="ov-confirm-view"><AlertDialog.Title>Verwaltete Aufzeichnungen stoppen?</AlertDialog.Title><AlertDialog.Description>Project 0 wird geordnet beendet und das Journal versiegelt. Eine laufende BSC-Readiness-Prüfung wird abgebrochen. Das Gateway und der Status-Reporter bleiben aktiv.</AlertDialog.Description><div className="ov-confirm-actions"><AlertDialog.Cancel asChild><button className="ov-btn">Abbrechen</button></AlertDialog.Cancel><AlertDialog.Action asChild><button className="ov-btn" onClick={()=>void stop()}>Aufzeichnungen stoppen</button></AlertDialog.Action></div></AlertDialog.Content></AlertDialog.Portal></AlertDialog.Root>;
}

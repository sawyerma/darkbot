import { captureCommand, captureLogs, useCaptureControl, type LogPart } from "@/core/realtime/captureControl";
import { useId, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
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
  const busy = useCaptureControl(s => s.busy);
  return <button type="button" className="ov-btn" disabled={busy} data-testid={testId} onClick={() => {
    if (!control.confirm || window.confirm(control.confirm)) void captureCommand(control.action);
  }}>{busy ? "Working…" : control.label}</button>;
}
export function CommandReceipt() {
  const receipt = useCaptureControl(s => s.receipt);
  return receipt ? <p role="status" className="ov-note" data-testid="capture-command-receipt">{receipt.phase} · {receipt.detail}<br/>Command {receipt.id}</p> : null;
}
export function LogsAction({ source, label, testId }: { source: "project0" | "all"; label: string; testId: string }) {
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
        {logs.map(log => <section key={log.source}><h4>{log.source === "project0" ? "Project 0" : "Gateway"}</h4><pre>{log.text || "Log is empty"}</pre></section>)}
      </Dialog.Content>
    </Dialog.Portal>
  </Dialog.Root>;
}

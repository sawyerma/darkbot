import "@/shared/ui/graphite/control-dialog.css";
import { useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { sourceCommand, useSourceControl, type SourceMode } from "@/core/realtime/sourceControl";
import type { SourceControlView, SourceRow } from "./model/sources";
// Source mode requests to the native bsc_runtime owner. Enabled only from the
// native projection; the confirmation states what the owner will do: LIVE is
// recorded as requested and stays effective BLOCKED, nothing is signed or sent.
const MODES: SourceMode[] = ["OFF", "SHADOW", "LIVE"];
/** Mode requests for `rows` (the sources by default, or the components) of the one native owner. */
export function SourceActions({ view, rows = view.rows, testId = "bsc-source" }: { view: SourceControlView; rows?: SourceRow[]; testId?: string }) {
  const { busy, receipt } = useSourceControl();
  const [target, setTarget] = useState<{ source: string; mode: SourceMode } | null>(null);
  return <>
    <div className="bsc-filter" data-testid={`${testId}-controls`}>
      {rows.map(row =>
        <span key={row.id} className="bsc-source-control">
          {MODES.map(mode =>
            <button key={mode} className="g-secondary" disabled={!view.available || busy || row.requested === mode}
              title={view.available ? `${row.id} → ${mode}` : view.reason}
              data-testid={`bsc-source-${row.id}-${mode}`} onClick={() => setTarget({ source: row.id, mode })}>{mode}</button>)}
        </span>)}
    </div>
    <AlertDialog.Root open={!!target} onOpenChange={open => { if (!open) setTarget(null); }}>
      <AlertDialog.Portal><AlertDialog.Overlay className="ov-log-overlay" /><AlertDialog.Content className="ov-confirm-view">
        <AlertDialog.Title>{target?.source} auf {target?.mode} setzen?</AlertDialog.Title>
        <AlertDialog.Description>{target?.mode === "LIVE"
          ? "LIVE wird als Wunsch gespeichert und bleibt nativ BLOCKED, solange keine wirtschaftliche bzw. operative Freigabe existiert. "
          : ""}Die Anfrage gilt für Generation {view.generation.toString()}; die native Quellenverwaltung wendet sie an oder lehnt sie ab. Sie erteilt keine Freigabe und löst weder Signatur noch Sendung aus.</AlertDialog.Description>
        <div className="ov-confirm-actions"><AlertDialog.Cancel asChild><button className="ov-btn">Abbrechen</button></AlertDialog.Cancel>
          <AlertDialog.Action asChild><button className="ov-btn" onClick={() => { if (target) void sourceCommand(target.source, target.mode, view.generation); setTarget(null); }}>Anfordern</button></AlertDialog.Action></div>
      </AlertDialog.Content></AlertDialog.Portal>
    </AlertDialog.Root>
    {receipt ? <p role="status" data-testid={`${testId}-receipt`}>{receipt.phase} · {receipt.detail}<br />Command {receipt.id}</p> : null}
  </>;
}

import "@/shared/ui/graphite/control-dialog.css";
import { useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { candidateCommand, useCandidateControl, type CandidateProfile } from "@/core/realtime/candidateControl";
import type { CandidateControlView } from "./model/candidate";
// Offline A/B candidate request. Enabled only from the native projection;
// the confirmation states that nothing is approved, signed or sent.
export function CandidateActions({ view }: { view: CandidateControlView }) {
  const { receipt } = useCandidateControl();
  const [target, setTarget] = useState<CandidateProfile | null>(null);
  const chosen = view.options.find(o => o.profile === target && o.enabled);
  return <>
    <div className="bsc-filter" data-testid="bsc-candidate-selector">
      {view.options.map(o =>
        <button key={o.profile} className="g-secondary" disabled={!o.enabled} title={o.why || undefined}
          data-testid={`bsc-candidate-request-${o.profile}`} onClick={() => setTarget(o.profile)}>Request {o.label}</button>)}
    </div>
    <AlertDialog.Root open={!!chosen} onOpenChange={open => { if (!open) setTarget(null); }}>
      <AlertDialog.Portal><AlertDialog.Overlay className="ov-log-overlay" /><AlertDialog.Content className="ov-confirm-view">
        <AlertDialog.Title>Kandidat {chosen?.label} anfordern?</AlertDialog.Title>
        <AlertDialog.Description>Die Anfrage gilt für Generation {view.generation.toString()} und wird erst beim nächsten Desktop-Sync von der nativen Kandidatenverwaltung angewendet oder abgelehnt. Sie erteilt keine Freigabe und löst weder Signatur noch Sendung aus.</AlertDialog.Description>
        <div className="ov-confirm-actions"><AlertDialog.Cancel asChild><button className="ov-btn">Abbrechen</button></AlertDialog.Cancel>
          <AlertDialog.Action asChild><button className="ov-btn" onClick={() => { if (chosen) void candidateCommand(chosen.profile, view.generation); setTarget(null); }}>Anfordern</button></AlertDialog.Action></div>
      </AlertDialog.Content></AlertDialog.Portal>
    </AlertDialog.Root>
    {receipt ? <p role="status">{receipt.phase} · {receipt.detail}<br />Command {receipt.id}</p> : null}
  </>;
}

import "@/shared/ui/graphite/control-dialog.css";
import { useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { ownerCommand, useSourceControl, type OwnerAction, type SwitchMode } from "@/core/realtime/sourceControl";
import { modeHeld, type SwitchRow } from "./model/switches";
// ENABLE/DISABLE requests for the wallet and economic switches of the one
// native bsc_runtime owner (plan 27.10.), the same request a terminal sends
// with `darkbotctl bsc source request`. A switch is a permission only: the
// confirmation says so; nothing here signs, arms or sends.
const MODES: { mode: SwitchMode; label: string }[] = [{ mode: "ENABLED", label: "ENABLE" }, { mode: "DISABLED", label: "DISABLE" }];
/** The runtime mode's choices (plan v9 §2.5); LIVE stays BLOCKED in the owner. */
export const RUNTIME_CHOICES = (["OFF", "SHADOW", "PAPER", "LIVE"] as const).map(mode => ({ mode, label: mode }));
const describe = (mode: string): string => ({
  ENABLED: "ENABLED ist nur die native Erlaubnis dieses Schalters: es wird nichts signiert oder gesendet, und kein anderer Schalter, kein LIVE und keine Freigabe wird dadurch aktiv. Jeder Mainnet-Pfad prüft zusätzlich Guard, V5-Vertrag, Nonce, Budget und Attempt-Autorität. Nach jedem Neustart ist der Schalter wieder DISABLED. ",
  DISABLED: "DISABLED sperrt jeden Mainnet-Pfad, der diesen Schalter braucht (auch im Terminal gestartete Runner), bei seiner nächsten Prüfung: der G0.7-Versuch prüft vor der Signatur, vor Tageslimit und Attempt-Autorität und vor dem Send-Head; nach dieser letzten Prüfung erreicht er den Release innerhalb der Head-Wartezeit. Bereits gesendete Transaktionen macht das nicht rückgängig. ",
  OFF: "OFF: weder das Strategy-C-Ledger noch der C3-PAPER-Pfad laufen. ",
  SHADOW: "SHADOW: Strategy C führt ihr PAPER-Ledger ohne Signatur; der C3-PAPER-Pfad bleibt aus. ",
  PAPER: "PAPER: zusätzlich der C3-Pfad bis zur PAPER-Senke (PAPER-Identität ohne Guthaben, nichts wird gesendet), soweit BSC_EXEC/BSC_STRATEGY_C und die Flap-Venue auf SHADOW stehen. ",
  LIVE: "LIVE wird als Wunsch gespeichert und bleibt nativ BLOCKED, solange keine wirtschaftliche Freigabe existiert. ",
} as Record<string, string>)[mode] ?? "";
export function SwitchActions({ rows, testId, action, modes = MODES }: { rows: SwitchRow[]; testId: string; action: Extract<OwnerAction, "bsc.wallet.set" | "bsc.economic.set" | "bsc.runtime.mode.set">; modes?: readonly { mode: string; label: string }[] }) {
  const { busy, receipt } = useSourceControl();
  const [target, setTarget] = useState<{ row: SwitchRow; mode: string } | null>(null);
  return <>
    <div className="bsc-filter" data-testid={`${testId}-controls`}>
      {rows.map(row =>
        <span key={row.id} className="bsc-source-control">
          {modes.map(({ mode, label }) =>
            <button key={mode} className="g-secondary" disabled={!row.available || busy || modeHeld(row, mode)}
              title={row.available ? `${row.id} → ${mode}` : row.reason}
              data-testid={`bsc-switch-${row.id}-${mode}`} onClick={() => setTarget({ row, mode })}>{row.label} {label}</button>)}
        </span>)}
    </div>
    <AlertDialog.Root open={!!target} onOpenChange={open => { if (!open) setTarget(null); }}>
      <AlertDialog.Portal><AlertDialog.Overlay className="ov-log-overlay" /><AlertDialog.Content className="ov-confirm-view">
        <AlertDialog.Title>{target?.row.label} ({target?.row.id}) auf {target?.mode} setzen?</AlertDialog.Title>
        <AlertDialog.Description>{describe(target?.mode ?? "")}
          Die Anfrage gilt für Generation {target?.row.generation.toString()}; die native Verwaltung wendet sie an oder lehnt sie ab.</AlertDialog.Description>
        <div className="ov-confirm-actions"><AlertDialog.Cancel asChild><button className="ov-btn">Abbrechen</button></AlertDialog.Cancel>
          <AlertDialog.Action asChild><button className="ov-btn" onClick={() => { if (target) void ownerCommand(testId, action, target.row.id, target.mode, target.row.generation); setTarget(null); }}>Anfordern</button></AlertDialog.Action></div>
      </AlertDialog.Content></AlertDialog.Portal>
    </AlertDialog.Root>
    {receipt && receipt.origin === testId ? <p role="status" data-testid={`${testId}-receipt`}>{receipt.phase} · {receipt.detail}<br />Command {receipt.id}</p> : null}
  </>;
}

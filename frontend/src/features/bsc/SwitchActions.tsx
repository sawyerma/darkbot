import "@/shared/ui/graphite/control-dialog.css";
import { useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { ownerCommand, useSourceControl, type OwnerAction, type SwitchMode } from "@/core/realtime/sourceControl";
import type { SwitchRow } from "./model/switches";
// ENABLE/DISABLE requests for the wallet and economic switches of the one
// native bsc_runtime owner (plan 27.10.), the same request a terminal sends
// with `darkbotctl bsc source request`. A switch is a permission only: the
// confirmation says so; nothing here signs, arms or sends.
const MODES: { mode: SwitchMode; label: string }[] = [{ mode: "ENABLED", label: "ENABLE" }, { mode: "DISABLED", label: "DISABLE" }];
export function SwitchActions({ rows, testId, action }: { rows: SwitchRow[]; testId: string; action: Extract<OwnerAction, "bsc.wallet.set" | "bsc.economic.set"> }) {
  const { busy, receipt } = useSourceControl();
  const [target, setTarget] = useState<{ row: SwitchRow; mode: SwitchMode } | null>(null);
  return <>
    <div className="bsc-filter" data-testid={`${testId}-controls`}>
      {rows.map(row =>
        <span key={row.id} className="bsc-source-control">
          {MODES.map(({ mode, label }) =>
            <button key={mode} className="g-secondary" disabled={!row.available || busy || row.requested === mode}
              title={row.available ? `${row.id} → ${mode}` : row.reason}
              data-testid={`bsc-switch-${row.id}-${mode}`} onClick={() => setTarget({ row, mode })}>{row.label} {label}</button>)}
        </span>)}
    </div>
    <AlertDialog.Root open={!!target} onOpenChange={open => { if (!open) setTarget(null); }}>
      <AlertDialog.Portal><AlertDialog.Overlay className="ov-log-overlay" /><AlertDialog.Content className="ov-confirm-view">
        <AlertDialog.Title>{target?.row.label} ({target?.row.id}) auf {target?.mode} setzen?</AlertDialog.Title>
        <AlertDialog.Description>{target?.mode === "ENABLED"
          ? "ENABLED ist nur die native Erlaubnis dieses Schalters: es wird nichts signiert oder gesendet, und kein anderer Schalter, kein LIVE und keine Freigabe wird dadurch aktiv. Jeder Mainnet-Pfad prüft zusätzlich Guard, V5-Vertrag, Nonce, Budget und Attempt-Autorität. Nach jedem Neustart ist der Schalter wieder DISABLED. "
          : "DISABLED sperrt ab sofort jeden Mainnet-Pfad, der diesen Schalter braucht (auch im Terminal gestartete Runner). Bereits gesendete Transaktionen macht das nicht rückgängig. "}
          Die Anfrage gilt für Generation {target?.row.generation.toString()}; die native Verwaltung wendet sie an oder lehnt sie ab.</AlertDialog.Description>
        <div className="ov-confirm-actions"><AlertDialog.Cancel asChild><button className="ov-btn">Abbrechen</button></AlertDialog.Cancel>
          <AlertDialog.Action asChild><button className="ov-btn" onClick={() => { if (target) void ownerCommand(testId, action, target.row.id, target.mode, target.row.generation); setTarget(null); }}>Anfordern</button></AlertDialog.Action></div>
      </AlertDialog.Content></AlertDialog.Portal>
    </AlertDialog.Root>
    {receipt && receipt.origin === testId ? <p role="status" data-testid={`${testId}-receipt`}>{receipt.phase} · {receipt.detail}<br />Command {receipt.id}</p> : null}
  </>;
}

import "@/shared/ui/graphite/control-dialog.css";
import { useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { ownerCommand, useSourceControl } from "@/core/realtime/sourceControl";
import { limitInputOk, type BuyLimitRow } from "./model/switches";
// The persisted daily BUY attempt limit of the native bsc_runtime owner (plan
// 27.10.), the same request as `darkbotctl bsc source request
// BSC_BUY_ATTEMPTS_PER_DAY <n>`. A limit is never a permission to execute.
const TEST_ID = "bsc-buy-limit";
export function BuyLimitActions({ row }: { row: BuyLimitRow }) {
  const { busy, receipt } = useSourceControl();
  const [text, setText] = useState("");
  const [open, setOpen] = useState(false);
  const ok = limitInputOk(text);
  return <>
    <div className="bsc-filter" data-testid={`${TEST_ID}-controls`}>
      <input inputMode="numeric" aria-label="BUY attempts per UTC day" placeholder="attempts / UTC day" value={text}
        onChange={e => setText(e.target.value.trim())} disabled={!row.available || busy} data-testid={`${TEST_ID}-input`} />
      <button className="g-secondary" disabled={!row.available || busy || !ok || (ok && BigInt(text) === row.perDay)}
        title={row.available ? "Set the daily limit" : "BACKEND_UNAVAILABLE"} data-testid={`${TEST_ID}-set`} onClick={() => setOpen(true)}>Set daily limit</button>
    </div>
    <AlertDialog.Root open={open} onOpenChange={setOpen}>
      <AlertDialog.Portal><AlertDialog.Overlay className="ov-log-overlay" /><AlertDialog.Content className="ov-confirm-view">
        <AlertDialog.Title>Tageslimit auf {text} BUY-Versuche (UTC-Tag) setzen?</AlertDialog.Title>
        <AlertDialog.Description>Das Limit zählt jeden wirtschaftlichen BUY-Versuch, der senden kann (auch unklare), bevor seine Autorität verbraucht wird; 0 erlaubt keinen. Es ist keine Ausführungsfreigabe: Wallets, SIGN/ARM/SEND/Attempt, Guard und V5 bleiben nötig. Die Anfrage gilt für Generation {row.generation.toString()}; die native Verwaltung wendet sie an oder lehnt sie ab.</AlertDialog.Description>
        <div className="ov-confirm-actions"><AlertDialog.Cancel asChild><button className="ov-btn">Abbrechen</button></AlertDialog.Cancel>
          <AlertDialog.Action asChild><button className="ov-btn" onClick={() => { if (ok) void ownerCommand(TEST_ID, "bsc.attempt.limit.set", "BSC_BUY_ATTEMPTS_PER_DAY", text, row.generation); setOpen(false); }}>Anfordern</button></AlertDialog.Action></div>
      </AlertDialog.Content></AlertDialog.Portal>
    </AlertDialog.Root>
    {receipt && receipt.origin === TEST_ID ? <p role="status" data-testid={`${TEST_ID}-receipt`}>{receipt.phase} · {receipt.detail}<br />Command {receipt.id}</p> : null}
  </>;
}

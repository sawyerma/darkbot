import "@/shared/ui/graphite/control-dialog.css";
import { useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { liveGrantCommand, ownerCommand, useSourceControl, type LiveGrantScope } from "@/core/realtime/sourceControl";
import { LIVE_ADAPTERS, LIVE_SOURCES, LIVE_STRATEGIES, grantScope, liveRequestOf, scopeSha256, scopeText, totalInputOk, type GrantForm } from "./model/live";
import type { ControlSwitchView } from "./model/types";
// The LIVE tab's requests (plan 28.10.), each the same request a terminal sends with `darkbotctl bsc
// live grant|revoke` or `darkbotctl bsc source request`, applied or refused by the one native
// bsc_runtime owner at the shown generation. A grant is never a send: the owner admits every LIVE
// intent again at its dispatch (switches, limits, the grant's budget, the verified qualification).

function Receipt({ testId }: { testId: string }) {
  const { receipt } = useSourceControl();
  return receipt && receipt.origin === testId ? <p role="status" data-testid={`${testId}-receipt`}>{receipt.phase} · {receipt.detail}<br />Command {receipt.id}</p> : null;
}

function Confirm({ open, title, children, onConfirm, onClose, testId }: { open: boolean; title: string; children: React.ReactNode; onConfirm: () => void; onClose: () => void; testId: string }) {
  return <AlertDialog.Root open={open} onOpenChange={o => { if (!o) onClose(); }}>
    <AlertDialog.Portal><AlertDialog.Overlay className="ov-log-overlay" /><AlertDialog.Content className="ov-confirm-view" data-testid={`${testId}-dialog`}>
      <AlertDialog.Title>{title}</AlertDialog.Title>
      <AlertDialog.Description asChild><div>{children}</div></AlertDialog.Description>
      <div className="ov-confirm-actions"><AlertDialog.Cancel asChild><button className="ov-btn">Abbrechen</button></AlertDialog.Cancel>
        <AlertDialog.Action asChild><button className="ov-btn" data-testid={`${testId}-confirm`} onClick={() => { onConfirm(); onClose(); }}>Anfordern</button></AlertDialog.Action></div>
    </AlertDialog.Content></AlertDialog.Portal>
  </AlertDialog.Root>;
}

const GRANT = "bsc-live-grant";
const EMPTY: GrantForm = { strategies: "C3", sources: ["BSC_INGRESS_PUBLICNODE"], adapters: ["CLUB48_BUILDER_V1"], maxInputWei: "", maxSpendWei: "", maxAttempts: "1", hours: "1" };

/** The grant of one scope: the form, then the exact canonical scope and its SHA-256 to confirm. */
export function LiveGrantActions({ generation, available }: { generation: bigint; available: boolean }) {
  const { busy } = useSourceControl();
  const [form, setForm] = useState<GrantForm>(EMPTY);
  const [shown, setShown] = useState<{ scope: LiveGrantScope; text: string; sha: string } | null>(null);
  const checked = grantScope(form, Date.now());
  const toggle = (list: string[], v: string) => list.includes(v) ? list.filter(x => x !== v) : [...list, v];
  const review = async () => {
    const now = grantScope(form, Date.now());
    if (!("scope" in now)) return;
    const text = scopeText(now.scope);
    setShown({ scope: now.scope, text, sha: await scopeSha256(text) });
  };
  return <>
    <div className="bsc-filter" data-testid={`${GRANT}-form`}>
      <select aria-label="Strategies" value={form.strategies} onChange={e => setForm({ ...form, strategies: e.target.value })} data-testid={`${GRANT}-strategies`}>
        {LIVE_STRATEGIES.map(s => <option key={s} value={s}>{s}</option>)}
      </select>
      {LIVE_SOURCES.map(s => <label key={s}><input type="checkbox" checked={form.sources.includes(s)} onChange={() => setForm({ ...form, sources: toggle(form.sources, s) })} data-testid={`${GRANT}-source-${s}`} />{s.replace("BSC_INGRESS_", "")}</label>)}
      {LIVE_ADAPTERS.map(a => <label key={a}><input type="checkbox" checked={form.adapters.includes(a)} onChange={() => setForm({ ...form, adapters: toggle(form.adapters, a) })} data-testid={`${GRANT}-adapter-${a}`} />{a}</label>)}
      <input inputMode="numeric" aria-label="Max BUY input (wei)" placeholder="max input wei" value={form.maxInputWei} onChange={e => setForm({ ...form, maxInputWei: e.target.value.trim() })} data-testid={`${GRANT}-max-input`} />
      <input inputMode="numeric" aria-label="Max spend (wei)" placeholder="max spend wei" value={form.maxSpendWei} onChange={e => setForm({ ...form, maxSpendWei: e.target.value.trim() })} data-testid={`${GRANT}-max-spend`} />
      <input inputMode="numeric" aria-label="BUY attempts" placeholder="attempts 1..10" value={form.maxAttempts} onChange={e => setForm({ ...form, maxAttempts: e.target.value.trim() })} data-testid={`${GRANT}-attempts`} />
      <input inputMode="numeric" aria-label="Valid for hours" placeholder="hours" value={form.hours} onChange={e => setForm({ ...form, hours: e.target.value.trim() })} data-testid={`${GRANT}-hours`} />
      <button className="g-secondary" disabled={!available || busy || "error" in checked} title={"error" in checked ? checked.error : `Grant at generation ${generation.toString()}`}
        data-testid={`${GRANT}-review`} onClick={() => void review()}>Grant LIVE…</button>
    </div>
    {"error" in checked ? <p className="g-muted" data-testid={`${GRANT}-error`}>{checked.error}</p> : null}
    <Confirm open={!!shown} testId={GRANT} title="Diesen LIVE-Umfang freigeben?" onClose={() => setShown(null)}
      onConfirm={() => { if (shown) void liveGrantCommand(GRANT, shown.scope, shown.sha, generation); }}>
      <p>Genau dieser Umfang wird freigegeben; die native Verwaltung berechnet den SHA-256 neu und lehnt jede Abweichung ab. Die Freigabe gilt nur in dieser Boot-Sitzung bis zum Ablauf und endet mit Widerruf, Neustart oder einer neuen Freigabe. Sie sendet nichts: jede LIVE-Absicht wird bei ihrem Versand erneut zugelassen (Schalter, Tages- und Gesamtlimit, Budget dieser Freigabe, verifizierte Qualifikation). Generation {generation.toString()}.</p>
      <pre data-testid={`${GRANT}-scope`}>{shown?.text}</pre>
      <p>scope_sha256 <code data-testid={`${GRANT}-sha`}>{shown?.sha}</code></p>
      <p>Ablauf: {shown ? new Date(Number(shown.scope.expiresUnixMs)).toISOString() : ""}</p>
    </Confirm>
    <Receipt testId={GRANT} />
  </>;
}

const REVOKE = "bsc-live-revoke";
/** Deactivate: the grant is revoked; the LIVE path ends at its next pass and drops its key. */
export function LiveRevokeActions({ generation, granted }: { generation: bigint; granted: boolean }) {
  const { busy } = useSourceControl();
  const [open, setOpen] = useState(false);
  return <>
    <button className="g-secondary" disabled={!granted || busy} data-testid={`${REVOKE}-request`} onClick={() => setOpen(true)}>Deactivate (revoke grant)</button>
    <Confirm open={open} testId={REVOKE} title="LIVE-Freigabe widerrufen?" onClose={() => setOpen(false)}
      onConfirm={() => void ownerCommand(REVOKE, "bsc.live.revoke", "BSC_LIVE_GRANT", "DISABLED", generation)}>
      <p>Der Widerruf beendet den LIVE-Pfad bei seinem nächsten Durchlauf; sein Schlüssel wird verworfen. Bereits gesendete Transaktionen und offene Positionen bleiben, wie die Kette sie entscheidet (Ledger). Generation {generation.toString()}.</p>
    </Confirm>
    <Receipt testId={REVOKE} />
  </>;
}

const TOTAL = "bsc-live-total";
/** The campaign's total BUY attempt limit (at most ten). */
export function TotalLimitActions({ generation, available, limit }: { generation: bigint; available: boolean; limit: bigint }) {
  const { busy } = useSourceControl();
  const [text, setText] = useState("");
  const [open, setOpen] = useState(false);
  const ok = totalInputOk(text);
  return <>
    <div className="bsc-filter" data-testid={`${TOTAL}-controls`}>
      <input inputMode="numeric" aria-label="Campaign BUY attempts" placeholder="0..10" value={text} onChange={e => setText(e.target.value.trim())} disabled={!available || busy} data-testid={`${TOTAL}-input`} />
      <button className="g-secondary" disabled={!available || busy || !ok || (ok && BigInt(text) === limit)} data-testid={`${TOTAL}-set`} onClick={() => setOpen(true)}>Set campaign limit</button>
    </div>
    <Confirm open={open} testId={TOTAL} title={`Gesamtlimit auf ${text} BUY-Versuche setzen?`} onClose={() => setOpen(false)}
      onConfirm={() => { if (ok) void ownerCommand(TOTAL, "bsc.attempt.total.set", "BSC_BUY_ATTEMPTS_TOTAL", text, generation); }}>
      <p>Höchstens zehn echte BUY-Versuche insgesamt, über jeden Tag und Neustart, unklare (UNKNOWN) eingeschlossen. Ein Limit ist keine Ausführungsfreigabe. Generation {generation.toString()}.</p>
    </Confirm>
    <Receipt testId={TOTAL} />
  </>;
}

const SCOPE = "bsc-live-scope";
/** Request LIVE for an entity of the scope (runtime, component, venue, source). */
export function ScopeLiveActions({ rows }: { rows: ControlSwitchView[] }) {
  const { busy } = useSourceControl();
  const [target, setTarget] = useState<ControlSwitchView | null>(null);
  return <>
    <div className="bsc-filter" data-testid={`${SCOPE}-controls`}>
      {rows.filter(r => r.requested !== "LIVE").map(r =>
        <button key={r.id} className="g-secondary" disabled={busy} data-testid={`${SCOPE}-${r.id}`} onClick={() => setTarget(r)}>{r.id} → LIVE</button>)}
    </div>
    <Confirm open={!!target} testId={SCOPE} title={`${target?.id ?? ""} auf LIVE setzen?`} onClose={() => setTarget(null)}
      onConfirm={() => { if (target) { const { action, id } = liveRequestOf(target); void ownerCommand(SCOPE, action, id, "LIVE", target.generation); } }}>
      <p>LIVE wird als Wunsch gespeichert und ist nur wirksam, solange das LIVE-Gate der nativen Verwaltung frei ist und die Freigabe diesen Teil des Umfangs nennt. Generation {target?.generation.toString()}.</p>
    </Confirm>
    <Receipt testId={SCOPE} />
  </>;
}

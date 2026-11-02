import "@/shared/ui/graphite/control-dialog.css";
import { useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { liveDecisionCommand, liveGrantCommand, ownerCommand, useSourceControl, type LiveGrantScope, type LotDecision } from "@/core/realtime/bscControl";
import { LIVE_ADAPTERS, LIVE_STRATEGIES, formSources, grantScope, liveRequestOf, scopeSha256, scopeText, totalInputOk, type DecisionLock, type GrantForm } from "./model/live";
import type { ControlSwitchView } from "./model/types";
// The LIVE tab's requests (plan 28.10.), each the same request a terminal sends with `darkbotctl bsc
// live grant|revoke` or `darkbotctl bsc source request`, applied or refused by the one native
// bsc_runtime owner at the shown generation. A grant is never a send: the owner admits every LIVE
// intent again at its dispatch (switches, limits, the grant's budget, the verified qualification).

export function Receipt({ testId }: { testId: string }) {
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
const EMPTY: GrantForm = { strategies: "C3", sources: null, adapters: ["CLUB48_BUILDER_V1"], maxInputWei: "", maxSpendWei: "", maxAttempts: "1", hours: "1", exitOnly: false,
  buySlippageBps: "", exitSlippageBps: "", gasPriceWei: "", buyGasLimit: "", approveGasLimit: "", sellGasLimit: "", outlierAck: false,
  maxOpenPositions: "", maxRealizedLossWei: "", maxDailyRealizedLossWei: "", minBuyIntervalMs: "" };
/** BSC3-069: the execution risk's inputs, each a bound the executor must stay within. */
const RISK_INPUTS: { key: "buySlippageBps" | "exitSlippageBps" | "gasPriceWei" | "buyGasLimit" | "approveGasLimit" | "sellGasLimit"; label: string; test: string }[] = [
  { key: "buySlippageBps", label: "Max BUY slippage (bps)", test: "buy-slippage" },
  { key: "exitSlippageBps", label: "Max exit slippage (bps)", test: "exit-slippage" },
  { key: "gasPriceWei", label: "Max gas price (wei)", test: "gas-price" },
  { key: "buyGasLimit", label: "Max BUY gas limit", test: "buy-gas" },
  { key: "approveGasLimit", label: "Max approval gas limit", test: "approve-gas" },
  { key: "sellGasLimit", label: "Max SELL gas limit", test: "sell-gas" },
];
/** 31.10. order: the positions and losses, each a bound no BUY goes beyond (exits go on). */
const LIMIT_INPUTS: { key: "maxOpenPositions" | "maxRealizedLossWei" | "maxDailyRealizedLossWei" | "minBuyIntervalMs"; label: string; test: string }[] = [
  { key: "maxOpenPositions", label: "Max open positions", test: "open-positions" },
  { key: "maxRealizedLossWei", label: "Max realized loss of the grant (wei)", test: "realized-loss" },
  { key: "maxDailyRealizedLossWei", label: "Max realized loss per UTC day (wei)", test: "daily-loss" },
  { key: "minBuyIntervalMs", label: "Min time between BUYs (ms)", test: "buy-interval" },
];

/** The grant of one scope: the form, then the exact canonical scope and its SHA-256 to confirm.
 *  Only the sources the native projection names grantable (an adapter endpoint) are offered. */
export function LiveGrantActions({ generation, available, grantable }: { generation: bigint; available: boolean; grantable: readonly string[] }) {
  const { busy } = useSourceControl();
  const [form, setForm] = useState<GrantForm>(EMPTY);
  const [shown, setShown] = useState<{ scope: LiveGrantScope; text: string; sha: string } | null>(null);
  const checked = grantScope(form, Date.now(), grantable);
  const sources = formSources(form, grantable);
  const toggle = (list: string[], v: string) => list.includes(v) ? list.filter(x => x !== v) : [...list, v];
  const review = async () => {
    const now = grantScope(form, Date.now(), grantable);
    if (!("scope" in now)) return;
    const text = scopeText(now.scope);
    setShown({ scope: now.scope, text, sha: await scopeSha256(text) });
  };
  return <>
    <div className="bsc-filter" data-testid={`${GRANT}-form`}>
      <select aria-label="Strategies" value={form.strategies} onChange={e => setForm({ ...form, strategies: e.target.value })} data-testid={`${GRANT}-strategies`}>
        {LIVE_STRATEGIES.map(s => <option key={s} value={s}>{s}</option>)}
      </select>
      {grantable.map(s => <label key={s}><input type="checkbox" checked={sources.includes(s)} onChange={() => setForm({ ...form, sources: toggle(sources, s) })} data-testid={`${GRANT}-source-${s}`} />{s.replace("BSC_INGRESS_", "")}</label>)}
      {LIVE_ADAPTERS.map(a => <label key={a}><input type="checkbox" checked={form.adapters.includes(a)} onChange={() => setForm({ ...form, adapters: toggle(form.adapters, a) })} data-testid={`${GRANT}-adapter-${a}`} />{a}</label>)}
      <input inputMode="numeric" aria-label="Max BUY input (wei)" placeholder="max input wei" value={form.maxInputWei} onChange={e => setForm({ ...form, maxInputWei: e.target.value.trim() })} data-testid={`${GRANT}-max-input`} />
      <input inputMode="numeric" aria-label="Max spend (wei)" placeholder="max spend wei" value={form.maxSpendWei} onChange={e => setForm({ ...form, maxSpendWei: e.target.value.trim() })} data-testid={`${GRANT}-max-spend`} />
      <input inputMode="numeric" aria-label="BUY attempts" placeholder="attempts 1..10" value={form.maxAttempts} onChange={e => setForm({ ...form, maxAttempts: e.target.value.trim() })} data-testid={`${GRANT}-attempts`} />
      <input inputMode="numeric" aria-label="Valid for hours" placeholder="hours" value={form.hours} onChange={e => setForm({ ...form, hours: e.target.value.trim() })} data-testid={`${GRANT}-hours`} />
      {RISK_INPUTS.map(r => <input key={r.key} inputMode="numeric" aria-label={r.label} placeholder={r.label} value={form[r.key]} onChange={e => setForm({ ...form, [r.key]: e.target.value.trim() })} data-testid={`${GRANT}-${r.test}`} />)}
      {LIMIT_INPUTS.map(r => <input key={r.key} inputMode="numeric" aria-label={r.label} placeholder={r.label} value={form[r.key]} onChange={e => setForm({ ...form, [r.key]: e.target.value.trim() })} data-testid={`${GRANT}-${r.test}`} />)}
      <label title="BSC3-058: only the exits of open positions, no BUY"><input type="checkbox" checked={form.exitOnly} onChange={() => setForm({ ...form, exitOnly: !form.exitOnly })} data-testid={`${GRANT}-exit-only`} />EXIT-ONLY</label>
      <label title="REV.1.0 R4 (BSC3-069): bounds above the runtime's soft bounds arm only confirmed as an outlier and with BSC_EXEC_LIVE_RISK_OUTLIER ON"><input type="checkbox" checked={form.outlierAck} onChange={() => setForm({ ...form, outlierAck: !form.outlierAck })} data-testid={`${GRANT}-outlier-ack`} />Confirm the risk as an outlier</label>
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

// REV.1.0 R3b (Audit A): the operator's decisions of the LIVE ledger, each the request a terminal
// sends with `darkbotctl bsc live lot|nonce-burn|ledger-quarantine`, at the state shown when its
// dialog opened (review H1: frozen there; a newer state is named, never sent); the native ledger
// writes its fact once (the runtime acts on it) or refuses (the receipt names why).
export const LIVE_LOT = "bsc-live-lot";
const DECISIONS: Record<LotDecision, { label: string; title: string; text: string }> = {
  RESUME: { label: "Resume", title: "Lot fortsetzen?", text: "Seine gestoppten Exits laufen weiter, seine Kurve wird neu gelesen." },
  WRITE_OFF: { label: "Write off", title: "Lot abschreiben?", text: "Es endet als verloren (Einsatz und Gas) und wird nie mehr verkauft." },
  EXIT_NOW: { label: "Exit now", title: "Exit jetzt anfordern?", text: "Die Strategie verkauft es jetzt zu ihrem Preis, über die Zulassung der nativen Verwaltung." },
};
const isDecision = (d: string): d is LotDecision => d in DECISIONS;
/** The ledger moved since the dialog opened: named, the frozen request is refused (LIVE_REQUEST_STALE). */
function Moved({ testId, now, then }: { testId: string; now: bigint | undefined; then: bigint }) {
  return now === then ? null : <p data-testid={`${testId}-moved`}>Das Ledger hat sich seither geändert (Version {now?.toString() ?? "—"}): diese Anfrage gilt Version {then.toString()} und wird abgelehnt; neu öffnen.</p>;
}
/** One open lot: the decisions the ledger takes of it now; exit-now locked unless
 *  BSC_EXEC_LIVE_OPERATOR_EXIT is ON. */
export function LiveLotActions({ lot, decisions, version, exitNow }: { lot: string; decisions: readonly string[]; version: bigint | undefined; exitNow: DecisionLock }) {
  const { busy } = useSourceControl();
  const [target, setTarget] = useState<{ decision: LotDecision; version: bigint } | null>(null);
  const shown = target ? DECISIONS[target.decision] : undefined;
  return <>
    {decisions.filter(isDecision).map(d => {
      const locked = d === "EXIT_NOW" && exitNow.locked;
      return <button key={d} className="g-secondary" disabled={busy || version === undefined || locked} data-testid={`${LIVE_LOT}-${d}`}
        aria-label={`${DECISIONS[d].label} lot ${lot}`} title={locked ? exitNow.why : `${DECISIONS[d].label} at ledger version ${version?.toString() ?? "—"}`}
        onClick={() => { if (version !== undefined) setTarget({ decision: d, version }); }}>{DECISIONS[d].label}</button>;
    })}
    <Confirm open={!!target} testId={LIVE_LOT} title={shown?.title ?? ""} onClose={() => setTarget(null)}
      onConfirm={() => { if (target) void liveDecisionCommand(LIVE_LOT, { action: "bsc.live.lot", lot, decision: target.decision, ledgerVersion: target.version }); }}>
      <p>{shown?.text} Lot <code>{lot}</code>, Ledger-Version {target?.version.toString()}: hat das Ledger danach einen neueren Fakt, lehnt die native Verwaltung ab (LIVE_REQUEST_STALE).</p>
      {target ? <Moved testId={LIVE_LOT} now={version} then={target.version} /> : null}
    </Confirm>
  </>;
}

export const LIVE_BURN = "bsc-live-burn";
/** One transaction the ledger takes a nonce burn of: locked unless BSC_EXEC_LIVE_NONCE_BURN is ON. */
export function LiveBurnAction({ tx, nonce, version, burn, requested }: { tx: string; nonce: bigint; version: bigint | undefined; burn: DecisionLock; requested: boolean }) {
  const { busy } = useSourceControl();
  const [at, setAt] = useState<bigint | null>(null);
  return <>
    <button className="g-secondary" disabled={busy || version === undefined || burn.locked || requested} data-testid={`${LIVE_BURN}-request`} aria-label={`Burn nonce ${nonce.toString()} of ${tx}`}
      title={requested ? "BURN_REQUESTED: the armed runtime burns it" : burn.locked ? burn.why : `Burn nonce ${nonce.toString()} at ledger version ${version?.toString() ?? "—"}`}
      onClick={() => { if (version !== undefined) setAt(version); }}>{requested ? "Burn requested" : "Burn nonce"}</button>
    <Confirm open={at !== null} testId={LIVE_BURN} title={`Nonce ${nonce.toString()} verbrennen?`} onClose={() => setAt(null)}
      onConfirm={() => { if (at !== null) void liveDecisionCommand(LIVE_BURN, { action: "bsc.live.nonce_burn", tx, ledgerVersion: at }); }}>
      <p>Ist der LIVE-Pfad bewaffnet und diese Nonce die nächste des Käufers, sendet er sich selbst nichts (0 BNB, 21 000 Gas zu <code>BSC_EXEC_GAS_PRICE_WEI</code>); landet das zuerst, ist <code>{tx}</code> ersetzt (REPLACED). Kein BUY geht davor. Ledger-Version {at?.toString()}.</p>
      {at !== null ? <Moved testId={LIVE_BURN} now={version} then={at} /> : null}
    </Confirm>
  </>;
}

export const LIVE_QUARANTINE = "bsc-live-quarantine";
/** The lowest ledger entry that does not decode: moved out at the owner generation (owner state
 *  needed: without it the native side refuses). */
export function LiveQuarantineAction({ seq, generation, available }: { seq: bigint | undefined; generation: bigint; available: boolean }) {
  const { busy } = useSourceControl();
  const [target, setTarget] = useState<{ seq: bigint; generation: bigint } | null>(null);
  if (seq === undefined) return null;
  return <>
    <button className="g-secondary" disabled={busy || !available} data-testid={`${LIVE_QUARANTINE}-request`} onClick={() => setTarget({ seq, generation })}>Quarantine ledger entry {seq.toString()}</button>
    <Confirm open={!!target} testId={LIVE_QUARANTINE} title={`Ledger-Eintrag ${target?.seq.toString() ?? ""} herausnehmen?`} onClose={() => setTarget(null)}
      onConfirm={() => { if (target) void liveDecisionCommand(LIVE_QUARANTINE, { action: "bsc.live.ledger.quarantine", ...target }); }}>
      <p>Der Eintrag, der nicht dekodiert, wird mit seinem Record verschoben; das Ledger liest wieder, wenn kein weiterer unlesbar ist. Was er sagte, ist die Rekonstruktion des Betreibers: BUYs aller Freigaben bis Generation {target?.generation.toString()} bleiben gesperrt, kein Slot geht mehr zurück.</p>
    </Confirm>
  </>;
}

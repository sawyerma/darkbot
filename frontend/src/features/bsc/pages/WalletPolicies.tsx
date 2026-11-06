import { useEffect, useId, useState } from "react";
import { useWalletControl, walletCommand } from "@/core/realtime/bscControl";
import { v8Params, WalletMovementKind } from "../api/bscV8Adapter";
import {
  bpsToPercent, eurQualified, lastOf, payoutCommand, payoutDisable, payoutForm, profitCommand, profitDisable, profitForm, versionText,
  type AmountMode, type Economics, type Treasury, type TransferRow,
} from "../model/wallets";
import { CommandReceipt, Locked, TransferLines, WalletConfirm, shortAddress } from "./WalletParts";
import { Badge, DetailLine, PanelHead, Wei, bnb } from "./V8";

const A = "bsc-wallet-policy-a";
const B = "bsc-wallet-policy-b";
const same = <T extends object>(a: T, b: T) => (Object.keys(a) as (keyof T)[]).every(k => a[k] === b[k]);
const BARE = { border: 0, margin: 0, padding: 0, minWidth: 0 } as const;

/** An edit of a stored policy, bound to the version it started from (its CAS). When the stored
 *  version moves under it, the edit is refused until Reset, unless the store now equals it (the
 *  operator's own request applied). Lives in this view only. */
function usePolicyDraft<F extends object>(stored: F, version: bigint | undefined) {
  const [draft, setDraft] = useState<{ form: F; base: bigint | undefined } | null>(null);
  const moved = !!draft && draft.base !== version;
  const applied = moved && !!draft && same(draft.form, stored);
  useEffect(() => { if (applied) setDraft(null); }, [applied]);
  return {
    form: draft?.form ?? stored, edited: !!draft, base: draft ? draft.base : version,
    moved: moved && !applied ? `The stored policy changed (v${draft?.base ?? 0n} → v${version ?? 0n}): Reset to edit the current one` : null,
    set: (patch: Partial<F>) => setDraft(d => ({ form: { ...(d?.form ?? stored), ...patch }, base: d ? d.base : version })),
    reset: () => setDraft(null),
  };
}

function Field({ id, label, value, onChange, unit, hint, check }: { id: string; label: string; value: string; onChange: (v: string) => void; unit: string; hint?: string; check: string }) {
  return <div className="field"><label htmlFor={id}>{label} · {unit}</label>
    <input id={id} className="input" inputMode="decimal" autoComplete="off" spellCheck={false} value={value} onChange={e => onChange(e.target.value)} data-testid={id} aria-describedby={`${check} ${id}-hint`} />
    {hint ? <small id={`${id}-hint`}>{hint}</small> : null}</div>;
}
function ModeSwitch({ id, mode, onChange, fixedLabel, labelId }: { id: string; mode: AmountMode; onChange: (m: AmountMode) => void; fixedLabel: string; labelId: string }) {
  return <div className="segment" role="group" aria-labelledby={labelId} data-testid={`${id}-mode`}>
    <button type="button" className={mode === "fixed" ? "active" : undefined} aria-pressed={mode === "fixed"} onClick={() => onChange("fixed")}>{fixedLabel}</button>
    <button type="button" className={mode === "share" ? "active" : undefined} aria-pressed={mode === "share"} onClick={() => onChange("share")}>Share %</button>
  </div>;
}
const stateBadge = (stored: boolean, enabled: boolean, testId: string) => <Badge label={!stored ? "Not stored" : enabled ? "Enabled" : "Disabled"} tone={!stored ? "warn" : enabled ? "good" : ""} testId={testId} />;

/** 4 · Policy A: Trading → Main on REALIZED NET trading profit (independent of policy B). */
export function ProfitPolicy({ t, eco, rows }: { t: Treasury; eco: Economics; rows: TransferRow[] }) {
  const { busy } = useWalletControl();
  const ids = useId();
  const p = t.ok ? t.state.profitPolicy : undefined;
  const stored = profitForm(p);
  const d = usePolicyDraft(stored, p?.policyVersion);
  const [ask, setAsk] = useState(false);
  const f = d.form;
  // Disabling a stored policy unchanged sends its stored values: nothing else has to validate again.
  const disableOnly = !!p?.enabled && !f.enabled && same({ ...f, enabled: true }, stored);
  const cmd = p && disableOnly ? profitDisable(p) : profitCommand(f);
  const lock = !t.ok ? t.reason : busy ? "Another wallet request is being decided" : d.moved;
  const block = lock ?? (!cmd.ok ? cmd.error : !d.edited || same(f, stored) ? "No change to save" : null);
  const send = () => {
    if (cmd.ok && !block) void walletCommand(A, "bsc.wallet.profit_policy.set", "", v8Params("profit_policy_set_command", cmd.value), d.base ?? 0n, `profit policy ${f.enabled ? "ENABLED" : "DISABLED"}`);
    setAsk(false);
  };
  const amount = cmd.ok ? cmd.value.amount?.amount : undefined;
  return <section className="card pad mtl" data-testid={A} aria-label="Automatic profit transfer">
    <PanelHead title="Automatic profit transfer · Trading → Main" extra={stateBadge(!!p, !!p?.enabled, `${A}-state`)} />
    <p className="small">Moves realized net trading profit (after losses, gas and fees, of closed, chain-confirmed lots) to the Main account once it passes the threshold. Never deposits, fundings, open or unknown PnL; never the Trading balance. Runs in the native owner, not in this browser.</p>
    <div className="grid wideleft mt">
      <fieldset disabled={!t.ok} style={BARE}>
        <label className="check-label"><input type="checkbox" checked={f.enabled} onChange={e => d.set({ enabled: e.target.checked })} data-testid={`${A}-enabled`} /> Enable automatic profit transfer</label>
        <div className="field-row mt">
          <Field id={`${A}-threshold`} check={`${A}-check`} label="Realized net profit threshold" unit="BNB" value={f.threshold} onChange={v => d.set({ threshold: v })} hint="Since the last transfer" />
          <div className="field"><span className="label" id={`${ids}-amount`}>Amount</span><ModeSwitch id={A} labelId={`${ids}-amount`} mode={f.mode} onChange={m => d.set({ mode: m, amount: "" })} fixedLabel="Fixed BNB" /></div>
        </div>
        <div className="field-row mt">
          <Field id={`${A}-amount`} check={`${A}-check`} label={f.mode === "share" ? "Share of the untransferred profit" : "Fixed amount"} unit={f.mode === "share" ? "%" : "BNB"} value={f.amount} onChange={v => d.set({ amount: v })} />
          <Field id={`${A}-min-capital`} check={`${A}-check`} label="Minimum trading capital" unit="BNB" value={f.minCapital} onChange={v => d.set({ minCapital: v })} hint="Always stays on Trading" />
        </div>
        <div className="field-row mt">
          <Field id={`${A}-reserve`} check={`${A}-check`} label="Exit and gas reserve" unit="BNB" value={f.reserve} onChange={v => d.set({ reserve: v })} hint="Always stays on Trading" />
          <div className="field"><span className="label">Policy version</span><div className="small" data-testid={`${A}-version`}>{versionText(p)}</div></div>
        </div>
        <p className="small mt" id={`${A}-check`} data-testid={`${A}-check`}>{!p && !d.edited ? "Not set: enter threshold, amount and reserves"
          : disableOnly ? `Disables the stored policy v${p?.policyVersion ?? 0n}; its values stay unchanged`
          : cmd.ok ? `Exact: threshold ${cmd.value.thresholdRealizedNetProfitWei} wei · ${amount?.case === "shareBps" ? `${bpsToPercent(amount.value)} %` : `${amount?.value ?? "—"} wei`} · keeps ${cmd.value.buyerMinCapitalWei} + ${cmd.value.buyerReserveWei} wei` : cmd.error}</p>
      </fieldset>
      <div className="subcard">
        <DetailLine label="Realized net PnL" value={eco.realizedNetPnlWei === undefined ? "—" : <Wei wei={eco.realizedNetPnlWei} places={6} signed />} />
        <DetailLine label="Already transferred" value={eco.transferredWei === undefined ? "—" : <Wei wei={eco.transferredWei} places={6} />} />
        <DetailLine label="Not yet transferred" value={p?.untransferredProfitWei === undefined ? "—" : <Wei wei={p.untransferredProfitWei} places={6} />} />
        <DetailLine label="Next threshold in" value={p?.toThresholdWei === undefined ? "—" : `${bnb(p.toThresholdWei, 6)} BNB`} />
        <DetailLine label="Settled up to" value={<span className="wallet-address">{p?.settlementCursor || "—"}</span>} />
        {p?.blockReason ? <DetailLine label="Not acting" value={p.blockReason} /> : null}
        <TransferLines row={lastOf(rows, [WalletMovementKind.PROFIT_TRANSFER])} empty={t.ok ? "None yet" : "—"} testId={`${A}-last`} />
      </div>
    </div>
    {lock ? <Locked reason={lock} testId={`${A}-locked`} /> : null}
    <div className="editor-footer"><span className="tiny">Independent of the automatic payout. Stored version applies once the owner reports it effective.</span>
      <div className="editor-actions"><button type="button" className="btn quiet" disabled={!d.edited} onClick={d.reset} data-testid={`${A}-reset`}>Reset</button>
        <button type="button" className="btn primary" disabled={!!block} title={block ?? undefined} data-testid={`${A}-save`} onClick={() => setAsk(true)}>Save profit transfer</button></div></div>
    <CommandReceipt origin={A} />
    <WalletConfirm open={ask} testId={`${A}-dialog`} title={`${f.enabled ? "Enable" : "Disable"} the automatic profit transfer?`} action="Request policy" onCancel={() => setAsk(false)} onConfirm={send}
      lines={disableOnly ? [["Policy", `stored v${p?.policyVersion ?? 0n}, values unchanged`]] : [["Trigger", `realized net profit ≥ ${f.threshold} BNB`], ["Amount", f.mode === "share" ? `${f.amount} % of the untransferred profit` : `${f.amount} BNB`], ["Keeps on Trading", `${f.minCapital} BNB capital + ${f.reserve} BNB reserve`], ["Version", `${d.base ?? 0n}`]]} />
  </section>;
}

/** 5 · Policy B: Main → external on the FREE Main balance (independent of policy A). */
export function PayoutPolicy({ t, rows, own }: { t: Treasury; rows: TransferRow[]; own: readonly string[] }) {
  const { busy } = useWalletControl();
  const ids = useId();
  const s = t.ok ? t.state : undefined;
  const p = s?.payoutPolicy;
  const stored = payoutForm(p);
  const d = usePolicyDraft(stored, p?.policyVersion);
  const [ask, setAsk] = useState(false);
  const f = d.form;
  const eur = eurQualified(s);
  const destinations = s?.destinations ?? [];
  const disableOnly = !!p?.enabled && !f.enabled && same({ ...f, enabled: true }, stored);
  const cmd = p && disableOnly ? payoutDisable(p) : payoutCommand(f, destinations, eur, own);
  const lock = !t.ok ? t.reason : busy ? "Another wallet request is being decided" : d.moved;
  const block = lock ?? (!cmd.ok ? cmd.error : !d.edited || same(f, stored) ? "No change to save" : null);
  const dest = destinations.find(x => x.destinationId === f.destinationId);
  const send = () => {
    if (cmd.ok && !block) void walletCommand(B, "bsc.wallet.payout_policy.set", "", v8Params("payout_policy_set_command", cmd.value), d.base ?? 0n, `payout policy ${f.enabled ? "ENABLED" : "DISABLED"}`);
    setAsk(false);
  };
  // A unit change never reinterprets a number: threshold and a fixed amount start empty.
  const unit = (u: "BNB" | "EUR") => { if (u !== f.unit) d.set({ unit: u, threshold: "", amount: f.mode === "fixed" ? "" : f.amount }); };
  return <section className="card pad mtl" data-testid={B} aria-label="Automatic payout">
    <PanelHead title="Automatic payout · Main → External" extra={stateBadge(!!p, !!p?.enabled, `${B}-state`)} />
    <p className="small">Pays out from the Main account to your saved external address once the free Main balance passes its own threshold: one payout per qualified balance event, an unchanged balance never pays again. Runs in the native owner, not in this browser.</p>
    <div className="grid wideleft mt">
      <fieldset disabled={!t.ok} style={BARE}>
        <label className="check-label"><input type="checkbox" checked={f.enabled} onChange={e => d.set({ enabled: e.target.checked })} data-testid={`${B}-enabled`} /> Enable automatic payout</label>
        <div className="field-row mt">
          <div className="field"><label htmlFor={`${B}-destination`}>Recipient · saved BSC address</label>
            <select id={`${B}-destination`} className="select" value={f.destinationId} onChange={e => d.set({ destinationId: e.target.value })} data-testid={`${B}-destination`}>
              <option value="">Choose…</option>
              {destinations.map(x => <option key={x.destinationId} value={x.destinationId} disabled={!x.confirmed}>{x.label || x.destinationId} · {shortAddress(x.address)}{x.confirmed ? "" : " · not confirmed"}</option>)}
            </select></div>
          <div className="field"><span className="label" id={`${ids}-unit`}>Unit</span>
            <div className="segment" role="group" aria-labelledby={`${ids}-unit`} data-testid={`${B}-unit`}>
              <button type="button" className={f.unit === "BNB" ? "active" : undefined} aria-pressed={f.unit === "BNB"} onClick={() => unit("BNB")}>BNB</button>
              <button type="button" className={f.unit === "EUR" ? "active" : undefined} aria-pressed={f.unit === "EUR"} disabled={!eur} title={eur ? undefined : "No qualified native BNB/EUR price"} onClick={() => unit("EUR")}>EUR</button>
            </div><small>{eur && s?.eurQuote ? `${s.eurQuote.fiatPerBnb} EUR/BNB · ${s.eurQuote.source}` : "EUR only with a qualified native BNB/EUR price"}</small></div>
        </div>
        <div className="field-row mt">
          <Field id={`${B}-threshold`} check={`${B}-check`} label="Free Main balance threshold" unit={f.unit} value={f.threshold} onChange={v => d.set({ threshold: v })} />
          <div className="field"><span className="label" id={`${ids}-amount`}>Amount</span><ModeSwitch id={B} labelId={`${ids}-amount`} mode={f.mode} onChange={m => d.set({ mode: m, amount: "" })} fixedLabel={`Fixed ${f.unit}`} /></div>
        </div>
        <div className="field-row mt">
          <Field id={`${B}-amount`} check={`${B}-check`} label={f.mode === "share" ? "Share of the free Main balance" : "Fixed payout"} unit={f.mode === "share" ? "%" : f.unit} value={f.amount} onChange={v => d.set({ amount: v })} />
          <Field id={`${B}-retain`} check={`${B}-check`} label="Main reserve" unit="BNB" value={f.retain} onChange={v => d.set({ retain: v })} hint="Gas and minimum: always stays on Main" />
        </div>
        <div className="field mt"><span className="label">Policy version</span><div className="small" data-testid={`${B}-version`}>{versionText(p)}</div></div>
        <p className="small mt" id={`${B}-check`} data-testid={`${B}-check`}>{!p && !d.edited ? "Not set: choose a recipient, threshold, amount and reserve"
          : disableOnly ? `Disables the stored policy v${p?.policyVersion ?? 0n}; its values stay unchanged`
          : cmd.ok ? `Exact: ${cmd.value.thresholdEur !== undefined ? `threshold ${cmd.value.thresholdEur} EUR` : `threshold ${cmd.value.thresholdAvailableMainWei} wei`} · keeps ${cmd.value.retainWei} wei` : cmd.error}</p>
      </fieldset>
      <div className="subcard">
        <DetailLine label="Free Main balance" value={p?.availableMainWei === undefined ? "—" : <Wei wei={p.availableMainWei} places={6} />} />
        <DetailLine label="Recipient" value={<span className="wallet-address">{p?.destinationAddress || dest?.address || "—"}</span>} />
        <DetailLine label="Last balance event" value={<span className="wallet-address">{p?.triggerCursor || "—"}</span>} />
        {p?.blockReason ? <DetailLine label="Not acting" value={p.blockReason} /> : null}
        <TransferLines row={lastOf(rows, [WalletMovementKind.PAYOUT_AUTO])} empty={t.ok ? "None yet" : "—"} testId={`${B}-last`} />
      </div>
    </div>
    {lock ? <Locked reason={lock} testId={`${B}-locked`} /> : null}
    <div className="editor-footer"><span className="tiny">Independent of the automatic profit transfer. On chain always BNB.</span>
      <div className="editor-actions"><button type="button" className="btn quiet" disabled={!d.edited} onClick={d.reset} data-testid={`${B}-reset`}>Reset</button>
        <button type="button" className="btn primary" disabled={!!block} title={block ?? undefined} data-testid={`${B}-save`} onClick={() => setAsk(true)}>Save payout rule</button></div></div>
    <CommandReceipt origin={B} />
    <WalletConfirm open={ask} testId={`${B}-dialog`} title={`${f.enabled ? "Enable" : "Disable"} the automatic payout?`} action="Request policy" onCancel={() => setAsk(false)} onConfirm={send}
      lines={disableOnly ? [["Policy", `stored v${p?.policyVersion ?? 0n}, values unchanged`]] : [["Recipient", dest ? `${dest.label || dest.destinationId} ${dest.address}` : "—"], ["Trigger", `free Main balance ≥ ${f.threshold} ${f.unit}`], ["Amount", f.mode === "share" ? `${f.amount} % of the free Main balance` : `${f.amount} ${f.unit}`], ["Keeps on Main", `${f.retain} BNB`], ["Version", `${d.base ?? 0n}`]]} />
  </section>;
}

import { useEffect, useId, useState, type ReactNode } from "react";
import { useWalletControl, walletCommand } from "@/core/realtime/bscControl";
import { v8Params, WalletMovementKind, WalletRole, type WalletAuthorization, type WalletTreasuryState } from "../api/bscV8Adapter";
import {
  authorizationOf, bpsToPercent, eurQualified, lastOf, payoutCommand, payoutDisable, payoutForm, payoutKey, payoutScopeText, profitCommand, profitDisable, profitForm, profitKey, profitScopeText, versionText,
  type AmountMode, type AuthorizationForm, type AuthorizationLimits, type Economics, type Treasury, type TransferRow,
} from "../model/wallets";
import { scopeSha256, unixMs } from "../model/live";
import { CommandReceipt, Locked, TransferLines, WalletConfirm, shortAddress } from "./WalletParts";
import { Badge, DetailLine, PanelHead, Wei, bnb } from "./V8";

const A = "bsc-wallet-policy-a";
const B = "bsc-wallet-policy-b";
const BARE = { border: 0, margin: 0, padding: 0, minWidth: 0 } as const;

/** An edit of a stored policy, bound to the version it started from (its CAS). When the stored
 *  version moves under it, the edit is refused until Reset, unless the store now means the same
 *  (the operator's own request applied; compared exactly, a validity is never stored). Lives in
 *  this view only. `key` is the form's canonical meaning. */
function usePolicyDraft<F extends AuthorizationForm>(stored: F, version: bigint | undefined, key: (f: F, withValidity: boolean) => string) {
  const [draft, setDraft] = useState<{ form: F; base: bigint | undefined } | null>(null);
  const moved = !!draft && draft.base !== version;
  const applied = moved && !!draft && key(draft.form, false) === key(stored, false);
  useEffect(() => { if (applied) setDraft(null); }, [applied]);
  return {
    form: draft?.form ?? stored, edited: !!draft, base: draft ? draft.base : version,
    /** Nothing to save: the same meaning as stored, and no new authorization typed. */
    unchanged: (f: F) => key(f, true) === key(stored, true),
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

/** The authorization inputs of an enable: validity, most per transfer and in total (owner bounds). */
function AuthorizationFields({ id, f, set, s }: { id: string; f: AuthorizationForm; set: (p: Partial<AuthorizationForm>) => void; s: WalletTreasuryState | undefined }) {
  const b = s?.authorizationBounds;
  return <div className="field-row mt">
    <Field id={`${id}-valid-hours`} check={`${id}-auth-check`} label="Authorization valid for" unit="h" value={f.validHours} onChange={v => set({ validHours: v })} hint={b ? `At most ${(b.maxValidityMs / 3_600_000n).toString()} h; ends at every restart` : "Owner bounds not reported"} />
    <Field id={`${id}-max-per-transfer`} check={`${id}-auth-check`} label="Most per transfer" unit="BNB" value={f.maxPerTransfer} onChange={v => set({ maxPerTransfer: v })} hint={b ? `Owner bound ${bnb(b.maxPerTransferWei, 6)} BNB` : undefined} />
    <Field id={`${id}-max-total`} check={`${id}-auth-check`} label="Most in total" unit="BNB" value={f.maxTotal} onChange={v => set({ maxTotal: v })} hint={b ? `Owner bound ${bnb(b.maxTotalWei, 6)} BNB` : undefined} />
  </div>;
}
/** The stored authorization as the owner holds it. */
const authorizationLine = (a: WalletAuthorization | undefined) => !a ? "None: the policy never acts"
  : `until ${unixMs(a.expiresUnixMs)} · ≤ ${bnb(a.maxPerTransferWei, 6)} BNB per transfer · ≤ ${bnb(a.maxTotalWei, 6)} BNB total · used ${bnb(a.usedTotalWei || "0", 6)} BNB${a.blockReason ? ` · ${a.blockReason}` : ""}`;
const authCheck = (auth: { ok: true; value: AuthorizationLimits } | { ok: false; error: string } | null) => auth === null ? "A disable needs no authorization"
  : auth.ok ? `Authorization: ${(auth.value.validityMs / 3_600_000n).toString()} h · ≤ ${auth.value.maxPerTransferWei} wei per transfer · ≤ ${auth.value.maxTotalWei} wei in total` : auth.error;
/** What the operator confirms: the canonical scope text, its digest and the expiry it names. */
type Shown = { text: string; sha: string; expires: bigint } | null;
const scopeLines = (shown: Shown): Array<[string, ReactNode]> => shown ? [["Valid until", unixMs(shown.expires)], ["Scope", <pre key="scope">{shown.text}</pre>], ["scope_sha256", shown.sha]] : [];

/** 4 · Policy A: Trading → Main on REALIZED NET trading profit (independent of policy B). */
export function ProfitPolicy({ t, eco, rows }: { t: Treasury; eco: Economics; rows: TransferRow[] }) {
  const { busy } = useWalletControl();
  const ids = useId();
  const s = t.ok ? t.state : undefined;
  const p = s?.profitPolicy;
  const stored = profitForm(p);
  const d = usePolicyDraft(stored, p?.policyVersion, profitKey);
  const [shown, setShown] = useState<Shown>(null);
  const [ask, setAsk] = useState(false);
  const f = d.form;
  // Disabling a stored policy unchanged sends its stored values: nothing else has to validate again.
  const disableOnly = !!p?.enabled && !f.enabled && d.unchanged({ ...f, enabled: true });
  const full = profitCommand(f);
  const cmd = p && disableOnly ? profitDisable(p) : full;
  const auth = f.enabled ? authorizationOf(f, s?.authorizationBounds) : null;
  const buyer = s?.accounts.find(a => a.role === WalletRole.BUYER);
  const main = s?.accounts.find(a => a.role === WalletRole.MAIN);
  const lock = !t.ok ? t.reason : busy ? "Another wallet request is being decided" : d.moved;
  const block = lock ?? (!cmd.ok ? cmd.error : auth && !auth.ok ? auth.error : f.enabled && (!buyer?.address || !main?.address || !main.chainId) ? "The owner reports no Trading or Main address" : !d.edited || d.unchanged(f) ? "No change to save" : null);
  const review = async () => {
    if (block) return;
    if (auth?.ok && full.ok && buyer && main) {
      const expires = BigInt(Date.now()) + auth.value.validityMs;
      const text = profitScopeText(main.chainId, buyer.address, main.address, full.value, auth.value, expires);
      setShown({ text, sha: await scopeSha256(text), expires });
    } else setShown(null);
    setAsk(true);
  };
  const send = () => {
    if (cmd.ok && !block && (!f.enabled || (shown && auth?.ok)))
      void walletCommand(A, "bsc.wallet.profit_policy.set", "", v8Params("profit_policy_set_command", { ...cmd.value,
        authorization: f.enabled && shown && auth?.ok ? { expiresUnixMs: shown.expires, maxPerTransferWei: auth.value.maxPerTransferWei, maxTotalWei: auth.value.maxTotalWei, scopeSha256: shown.sha } : undefined }),
      d.base ?? 0n, `profit policy ${f.enabled ? "ENABLED" : "DISABLED"}`);
    setAsk(false);
  };
  const amount = cmd.ok ? cmd.value.amount?.amount : undefined;
  return <section className="card pad mtl" data-testid={A} aria-label="Automatic profit transfer">
    <PanelHead title="Automatic profit transfer · Trading → Main" extra={stateBadge(!!p, !!p?.enabled, `${A}-state`)} />
    <p className="small">Moves realized net trading profit (after losses, gas and fees, of closed, chain-confirmed lots) to the Main account once it passes the threshold. Never deposits, fundings, open or unknown PnL; never the Trading balance. Runs in the native owner within your authorization, not in this browser.</p>
    <div className="grid wideleft mt">
      <fieldset disabled={!t.ok} style={BARE}>
        <label className="check-label"><input type="checkbox" checked={f.enabled} onChange={e => d.set({ enabled: e.target.checked })} data-testid={`${A}-enabled`} /> Enable automatic profit transfer</label>
        <div className="field-row mt">
          <Field id={`${A}-threshold`} check={`${A}-check`} label="Realized net profit threshold" unit="BNB" value={f.threshold} onChange={v => d.set({ threshold: v })} hint="Since the last settlement" />
          <div className="field"><span className="label" id={`${ids}-amount`}>Amount</span><ModeSwitch id={A} labelId={`${ids}-amount`} mode={f.mode} onChange={m => d.set({ mode: m, amount: "" })} fixedLabel="Fixed BNB" /></div>
        </div>
        <div className="field-row mt">
          <Field id={`${A}-amount`} check={`${A}-check`} label={f.mode === "share" ? "Share of the net profit since the last settlement" : "Fixed amount"} unit={f.mode === "share" ? "%" : "BNB"} value={f.amount} onChange={v => d.set({ amount: v })} />
          <Field id={`${A}-min-capital`} check={`${A}-check`} label="Minimum trading capital" unit="BNB" value={f.minCapital} onChange={v => d.set({ minCapital: v })} hint="Always stays on Trading" />
        </div>
        <div className="field-row mt">
          <Field id={`${A}-reserve`} check={`${A}-check`} label="Exit and gas reserve" unit="BNB" value={f.reserve} onChange={v => d.set({ reserve: v })} hint="Always stays on Trading" />
          <div className="field"><span className="label">Policy version</span><div className="small" data-testid={`${A}-version`}>{versionText(p)}</div></div>
        </div>
        {f.enabled ? <AuthorizationFields id={A} f={f} set={d.set} s={s} /> : null}
        <p className="small mt" id={`${A}-check`} data-testid={`${A}-check`}>{!p && !d.edited ? "Not set: enter threshold, amount, reserves and the authorization"
          : disableOnly ? `Disables the stored policy v${p?.policyVersion ?? 0n}; its values stay unchanged`
          : cmd.ok ? `Exact: threshold ${cmd.value.thresholdRealizedNetProfitWei} wei · ${amount?.case === "shareBps" ? `${bpsToPercent(amount.value)} %` : `${amount?.value ?? "—"} wei`} · keeps ${cmd.value.buyerMinCapitalWei} + ${cmd.value.buyerReserveWei} wei` : cmd.error}</p>
        <p className="small" id={`${A}-auth-check`} data-testid={`${A}-auth-check`}>{authCheck(auth)}</p>
      </fieldset>
      <div className="subcard">
        <DetailLine label="Realized net PnL" value={eco.realizedNetPnlWei === undefined ? "—" : <Wei wei={eco.realizedNetPnlWei} places={6} signed />} />
        <DetailLine label="Net since last settlement" value={p?.netSinceLastSettlementWei === undefined ? "—" : <Wei wei={p.netSinceLastSettlementWei} places={6} signed />} />
        <DetailLine label="Already transferred" value={eco.transferredWei === undefined ? "—" : <Wei wei={eco.transferredWei} places={6} />} />
        <DetailLine label="Not yet transferred" value={p?.untransferredProfitWei === undefined ? "—" : <Wei wei={p.untransferredProfitWei} places={6} />} />
        <DetailLine label="Next threshold in" value={p?.toThresholdWei === undefined ? "—" : `${bnb(p.toThresholdWei, 6)} BNB`} />
        <DetailLine label="Settled up to" value={<span className="wallet-address">{p?.settlementCursor || "—"}{p?.settledLedgerVersion !== undefined ? ` · ledger v${p.settledLedgerVersion}` : ""}{p?.finalityDepth !== undefined ? ` · ${p.finalityDepth} blocks deep` : ""}</span>} />
        <DetailLine label="Authorization" testId={`${A}-authorization`} value={authorizationLine(p?.authorization)} />
        {p?.blockReason ? <DetailLine label="Not acting" value={p.blockReason} /> : null}
        <TransferLines row={lastOf(rows, [WalletMovementKind.PROFIT_TRANSFER])} empty={t.ok ? "None yet" : "—"} testId={`${A}-last`} />
      </div>
    </div>
    {lock ? <Locked reason={lock} testId={`${A}-locked`} /> : null}
    <div className="editor-footer"><span className="tiny">Independent of the automatic payout. Stored version applies once the owner reports it effective.</span>
      <div className="editor-actions"><button type="button" className="btn quiet" disabled={!d.edited} onClick={d.reset} data-testid={`${A}-reset`}>Reset</button>
        <button type="button" className="btn primary" disabled={!!block} title={block ?? undefined} data-testid={`${A}-save`} onClick={() => { void review(); }}>Save profit transfer</button></div></div>
    <CommandReceipt origin={A} />
    <WalletConfirm open={ask} testId={`${A}-dialog`} title={`${f.enabled ? "Enable" : "Disable"} the automatic profit transfer?`} action="Request policy" onCancel={() => setAsk(false)} onConfirm={send}
      lines={disableOnly ? [["Policy", `stored v${p?.policyVersion ?? 0n}, values unchanged`]] : [["Trigger", `realized net profit ≥ ${f.threshold} BNB`], ["Amount", f.mode === "share" ? `${f.amount} % of the net profit since the last settlement` : `${f.amount} BNB`], ["Keeps on Trading", `${f.minCapital} BNB capital + ${f.reserve} BNB reserve`], ["Version", `${d.base ?? 0n}`], ...scopeLines(f.enabled ? shown : null)]} />
  </section>;
}

/** 5 · Policy B: Main → external on the FREE Main balance (independent of policy A). */
export function PayoutPolicy({ t, rows, own }: { t: Treasury; rows: TransferRow[]; own: readonly string[] }) {
  const { busy } = useWalletControl();
  const ids = useId();
  const s = t.ok ? t.state : undefined;
  const p = s?.payoutPolicy;
  const stored = payoutForm(p);
  const d = usePolicyDraft(stored, p?.policyVersion, payoutKey);
  const [shown, setShown] = useState<Shown>(null);
  const [ask, setAsk] = useState(false);
  const f = d.form;
  const eur = eurQualified(s);
  const destinations = s?.destinations ?? [];
  const main = s?.accounts.find(a => a.role === WalletRole.MAIN);
  const disableOnly = !!p?.enabled && !f.enabled && d.unchanged({ ...f, enabled: true });
  const full = payoutCommand(f, destinations, eur, own, main?.chainId);
  const cmd = p && disableOnly ? payoutDisable(p) : full;
  const auth = f.enabled ? authorizationOf(f, s?.authorizationBounds) : null;
  const lock = !t.ok ? t.reason : busy ? "Another wallet request is being decided" : d.moved;
  const block = lock ?? (!cmd.ok ? cmd.error : auth && !auth.ok ? auth.error : f.enabled && (!main?.address || !main.chainId) ? "The owner reports no Main address" : !d.edited || d.unchanged(f) ? "No change to save" : null);
  const dest = destinations.find(x => x.destinationId === f.destinationId);
  const review = async () => {
    if (block) return;
    if (auth?.ok && full.ok && main) {
      const expires = BigInt(Date.now()) + auth.value.validityMs;
      const text = payoutScopeText(main.chainId, main.address, full.value, auth.value, expires);
      setShown({ text, sha: await scopeSha256(text), expires });
    } else setShown(null);
    setAsk(true);
  };
  const send = () => {
    if (cmd.ok && !block && (!f.enabled || (shown && auth?.ok)))
      void walletCommand(B, "bsc.wallet.payout_policy.set", "", v8Params("payout_policy_set_command", { ...cmd.value,
        authorization: f.enabled && shown && auth?.ok ? { expiresUnixMs: shown.expires, maxPerTransferWei: auth.value.maxPerTransferWei, maxTotalWei: auth.value.maxTotalWei, scopeSha256: shown.sha } : undefined }),
      d.base ?? 0n, `payout policy ${f.enabled ? "ENABLED" : "DISABLED"}`);
    setAsk(false);
  };
  // A unit change never reinterprets a number: threshold and a fixed amount start empty.
  const unit = (u: "BNB" | "EUR") => { if (u !== f.unit) d.set({ unit: u, threshold: "", amount: f.mode === "fixed" ? "" : f.amount }); };
  return <section className="card pad mtl" data-testid={B} aria-label="Automatic payout">
    <PanelHead title="Automatic payout · Main → External" extra={stateBadge(!!p, !!p?.enabled, `${B}-state`)} />
    <p className="small">Pays out from the Main account to your saved external address once the free Main balance passes its own threshold: one payout per qualified balance event, an unchanged balance never pays again. Runs in the native owner within your authorization, not in this browser.</p>
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
          <Field id={`${B}-amount`} check={`${B}-check`} label={f.mode === "share" ? "Share of the free Main balance above the reserve" : "Fixed payout"} unit={f.mode === "share" ? "%" : f.unit} value={f.amount} onChange={v => d.set({ amount: v })} />
          <Field id={`${B}-retain`} check={`${B}-check`} label="Main reserve" unit="BNB" value={f.retain} onChange={v => d.set({ retain: v })} hint="Gas and minimum: always stays on Main" />
        </div>
        <div className="field mt"><span className="label">Policy version</span><div className="small" data-testid={`${B}-version`}>{versionText(p)}</div></div>
        {f.enabled ? <AuthorizationFields id={B} f={f} set={d.set} s={s} /> : null}
        <p className="small mt" id={`${B}-check`} data-testid={`${B}-check`}>{!p && !d.edited ? "Not set: choose a recipient, threshold, amount, reserve and the authorization"
          : disableOnly ? `Disables the stored policy v${p?.policyVersion ?? 0n}; its values stay unchanged`
          : cmd.ok ? `Exact: ${cmd.value.thresholdEur !== undefined ? `threshold ${cmd.value.thresholdEur} EUR` : `threshold ${cmd.value.thresholdAvailableMainWei} wei`} · keeps ${cmd.value.retainWei} wei` : cmd.error}</p>
        <p className="small" id={`${B}-auth-check`} data-testid={`${B}-auth-check`}>{authCheck(auth)}</p>
      </fieldset>
      <div className="subcard">
        <DetailLine label="Free Main balance" value={p?.availableMainWei === undefined ? "—" : <Wei wei={p.availableMainWei} places={6} />} />
        <DetailLine label="Recipient" value={<span className="wallet-address">{p?.destinationAddress || dest?.address || "—"}</span>} />
        <DetailLine label="Last balance event" value={<span className="wallet-address">{p?.triggerCursor || "—"}</span>} />
        <DetailLine label="Authorization" testId={`${B}-authorization`} value={authorizationLine(p?.authorization)} />
        {p?.blockReason ? <DetailLine label="Not acting" value={p.blockReason} /> : null}
        <TransferLines row={lastOf(rows, [WalletMovementKind.PAYOUT_AUTO])} empty={t.ok ? "None yet" : "—"} testId={`${B}-last`} />
      </div>
    </div>
    {lock ? <Locked reason={lock} testId={`${B}-locked`} /> : null}
    <div className="editor-footer"><span className="tiny">Independent of the automatic profit transfer. On chain always BNB.</span>
      <div className="editor-actions"><button type="button" className="btn quiet" disabled={!d.edited} onClick={d.reset} data-testid={`${B}-reset`}>Reset</button>
        <button type="button" className="btn primary" disabled={!!block} title={block ?? undefined} data-testid={`${B}-save`} onClick={() => { void review(); }}>Save payout rule</button></div></div>
    <CommandReceipt origin={B} />
    <WalletConfirm open={ask} testId={`${B}-dialog`} title={`${f.enabled ? "Enable" : "Disable"} the automatic payout?`} action="Request policy" onCancel={() => setAsk(false)} onConfirm={send}
      lines={disableOnly ? [["Policy", `stored v${p?.policyVersion ?? 0n}, values unchanged`]] : [["Recipient", dest ? `${dest.label || dest.destinationId} ${dest.address}` : "—"], ["Trigger", `free Main balance ≥ ${f.threshold} ${f.unit}`], ["Amount", f.mode === "share" ? `${f.amount} % of the free Main balance above the reserve` : `${f.amount} ${f.unit}`], ["Keeps on Main", `${f.retain} BNB`], ["Version", `${d.base ?? 0n}`], ...scopeLines(f.enabled ? shown : null)]} />
  </section>;
}

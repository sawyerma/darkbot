import { useEffect, useState } from "react";
import { useWalletControl, walletCommand } from "@/core/realtime/bscControl";
import { TransferStatus, v8Params, WalletMovementKind, WalletRole, type WalletDestination, type WalletTreasuryState } from "../api/bscV8Adapter";
import { chainProblem, draftFrom, draftProblem, externalAddress, lastOf, openOf, positiveWei, remaining, usable, type AccountView, type Treasury, type TransferRow } from "../model/wallets";
import { scopeSha256, unixMs } from "../model/live";
import { CommandReceipt, Locked, TransferLines, WalletConfirm, shortAddress } from "./WalletParts";
import { DetailLine, Drawer, PanelHead, Wei, bnb } from "./V8";

const FUND = "bsc-wallet-fund";
const PAYOUT = "bsc-wallet-send";
const DEST = "bsc-wallet-destination";
const PREPARE_NOTE = "One request to the native BSC treasury owner: it prepares the transfer (nonce, fee, what stays on Main, expiry) and sends nothing. You authorize the prepared transfer in a second step.";

/** What stands between the operator and any request from Main (the first reason), or null. */
function ownerLock(t: Treasury, main: AccountView, busy: boolean): string | null {
  if (!t.ok) return t.reason;
  if (!usable(main)) return `Main account ${main.state}${main.reason ? `: ${main.reason}` : ""}${main.ownerKey ? "" : " · the owner holds no key"}`;
  if (busy) return "Another wallet request is being decided";
  return null;
}
/** What still stands between the form and a prepare (the first reason), or null. A DRAFT from Main
 *  holds Main's next nonce: no other transfer leaves Main while it is open (wallet.proto). */
function blocker(t: Treasury, main: AccountView, busy: boolean, open: TransferRow | undefined, mainDraft: TransferRow | undefined, fee: string | undefined): string | null {
  if (!t.ok || !usable(main)) return ownerLock(t, main, busy);
  if (fee === undefined) return "The owner reports no fee estimate";
  if (open?.t.status === TransferStatus.DRAFT) return `A prepared transfer of this path awaits its authorization (request ${open.t.requestId})`;
  if (open) return `A transfer of this path is ${open.label.toLowerCase()} (request ${open.t.requestId}): the chain decides first, nothing is sent twice`;
  if (mainDraft) return `A prepared transfer from Main awaits its authorization (request ${mainDraft.t.requestId}): it holds Main's next nonce`;
  return ownerLock(t, main, busy);
}

function Amount({ id, label, value, onChange, check }: { id: string; label: string; value: string; onChange: (v: string) => void; check: { ok: true; value: string } | { ok: false; error: string } | null }) {
  return <div className="field mt"><label htmlFor={id}>{label}</label>
    <input id={id} className="input" inputMode="decimal" autoComplete="off" spellCheck={false} placeholder="0.00" value={value} onChange={e => onChange(e.target.value)} data-testid={id} aria-invalid={check?.ok === false} />
    <small data-testid={`${id}-check`}>{check === null ? "Exact BNB, at most 18 places" : check.ok ? `${check.value} wei` : check.error}</small></div>;
}

/** The owner's DRAFT: its canonical scope text, its digest checked again here, and one authorization
 *  naming it. Nothing is sent without it; after its expiry the owner sends nothing. */
function DraftAuthorize({ origin, row, from, to, lock, version }: { origin: string; row: TransferRow; from: string; to: string | undefined; lock: string | null; version: bigint }) {
  const t = row.t;
  const [digest, setDigest] = useState<{ text: string; sha: string } | null>(null);
  const [ask, setAsk] = useState(false);
  useEffect(() => {
    let live = true;
    void scopeSha256(t.scopeText).then(sha => { if (live) setDigest({ text: t.scopeText, sha }); });
    return () => { live = false; };
  }, [t.scopeText]);
  const sha = digest?.text === t.scopeText ? digest.sha : undefined;
  const problem = draftProblem(t, row.t.kind, from, to)
    ?? (sha === undefined ? "Checking the scope digest…"
    : sha !== t.scopeSha256 ? "The owner's scope text and its digest disagree: not authorized" : null);
  const block = lock ?? problem;
  const send = () => {
    if (!block) void walletCommand(origin, "bsc.wallet.transfer.authorize", t.commandId, v8Params("transfer_authorize_command", { scopeSha256: t.scopeSha256 }), version, `authorize ${t.commandId}`);
    setAsk(false);
  };
  return <div className="subcard mt" data-testid={`${origin}-draft`}>
    <strong>Prepared · awaits your authorization</strong>
    <DetailLine label="To" value={<span className="wallet-address">{t.destinationAddress || "—"}</span>} />
    <DetailLine label="Amount" value={<Wei wei={t.amountWei} places={18} />} />
    <DetailLine label="Nonce" value={t.nonce?.toString() ?? "—"} />
    <DetailLine label="Fee estimate" value={t.feeEstimateWei === undefined ? "— (owner)" : `${bnb(t.feeEstimateWei, 6)} BNB`} />
    <DetailLine label="Stays on Main" value={t.reserveAfterWei === undefined ? "—" : <Wei wei={t.reserveAfterWei} places={6} />} />
    <DetailLine label="Expires" value={unixMs(t.expiresUnixMs)} />
    <pre data-testid={`${origin}-draft-scope`}>{t.scopeText}</pre>
    <DetailLine label="scope_sha256" value={<span className="wallet-address" data-testid={`${origin}-draft-sha`}>{t.scopeSha256 || "—"}</span>} />
    {problem && !lock ? <Locked reason={problem} testId={`${origin}-draft-locked`} /> : null}
    <div className="wallet-tool-row"><button type="button" className="btn primary" disabled={!!block} title={block ?? undefined} data-testid={`${origin}-authorize`} onClick={() => setAsk(true)}>Authorize transfer</button></div>
    <WalletConfirm open={ask} testId={`${origin}-authorize-dialog`} title="Authorize this prepared transfer?" action="Authorize" onCancel={() => setAsk(false)} onConfirm={send}
      lines={[["Scope", <pre key="scope">{t.scopeText}</pre>], ["scope_sha256", t.scopeSha256], ["Draft", t.commandId], ["Treasury version", version.toString()]]} />
  </div>;
}

/** 3 · Manual transfers: Main → Trading (capital) and Main → external (payout); each a prepare and
 *  its authorization, two requests. */
export function WalletTransfers({ t, accounts, rows }: { t: Treasury; accounts: Record<"main" | "buyer" | "launcher" | "external", AccountView>; rows: TransferRow[] }) {
  const { busy } = useWalletControl();
  const s: WalletTreasuryState | undefined = t.ok ? t.state : undefined;
  // The funding target is the treasury owner's own Trading account, never the projection's.
  const target = s?.accounts.find(a => a.role === WalletRole.BUYER);
  const fee = s?.transferFeeEstimateWei;
  const main = accounts.main;
  const [fund, setFund] = useState("");
  const [pay, setPay] = useState("");
  const [destId, setDestId] = useState("");
  const [confirm, setConfirm] = useState<"fund" | "payout" | null>(null);
  const [destDrawer, setDestDrawer] = useState(false);
  const destinations = s?.destinations ?? [];
  const dest = destinations.find(d => d.destinationId === destId) ?? destinations.find(d => d.confirmed);
  const lock = ownerLock(t, main, busy);
  const version = s?.treasuryVersion ?? 0n;
  const mainDraft = draftFrom(rows, WalletRole.MAIN);

  const fundAmount = fund.trim() ? positiveWei(fund, "Amount") : null;
  const fundRest = fundAmount?.ok ? remaining(main.availableWei, fundAmount.value, fee) : null;
  const fundOpen = openOf(rows, [WalletMovementKind.CAPITAL_FUNDING]);
  // An owner reason is shown as a lock; an input reason stays at its field (the amount, the preview).
  const fundLock = blocker(t, main, busy, fundOpen, mainDraft, fee) ?? (!target?.address ? "The treasury owner reports no Trading account" : main.chainId === undefined ? "No chain id reported" : null);
  const fundBlock = fundLock ?? (!fundAmount ? "Enter an amount" : !fundAmount.ok ? fundAmount.error : fundRest && !fundRest.ok ? fundRest.error : null);

  const payAmount = pay.trim() ? positiveWei(pay, "Amount") : null;
  const payRest = payAmount?.ok ? remaining(main.availableWei, payAmount.value, fee) : null;
  const payOpen = openOf(rows, [WalletMovementKind.PAYOUT_MANUAL]);
  const own = [main.address, accounts.buyer.address, accounts.launcher.address];
  const destCheck = dest ? externalAddress(dest.address, own) : null;
  const payLock = blocker(t, main, busy, payOpen, mainDraft, fee)
    ?? (!dest ? "Save and confirm an external BSC address first" : !dest.confirmed ? "The external address is not confirmed" : destCheck && !destCheck.ok ? destCheck.error : chainProblem(dest.chainId, main.chainId));
  const payBlock = payLock ?? (!payAmount ? "Enter an amount" : !payAmount.ok ? payAmount.error : payRest && !payRest.ok ? payRest.error : null);

  const send = () => {
    if (confirm === "fund" && fundAmount?.ok && fee !== undefined && target && !fundBlock)
      void walletCommand(FUND, "bsc.wallet.transfer.prepare", "", v8Params("transfer_prepare_fund_command", { transfer: { case: "fundTrading", value: { amountWei: fundAmount.value, maxFeeWei: fee, chainId: main.chainId ?? 0n, buyerAddress: target.address } } }), version, `prepare funding ${bnb(fundAmount.value, 6)} BNB`);
    if (confirm === "payout" && payAmount?.ok && dest && fee !== undefined && !payBlock)
      void walletCommand(PAYOUT, "bsc.wallet.transfer.prepare", "", v8Params("transfer_prepare_payout_command", { transfer: { case: "payout", value: { destinationId: dest.destinationId, destinationAddress: dest.address, chainId: dest.chainId, amountWei: payAmount.value, maxFeeWei: fee } } }), version, `prepare payout ${bnb(payAmount.value, 6)} BNB to ${shortAddress(dest.address)}`);
    setConfirm(null);
  };
  const draft = (open: TransferRow | undefined, origin: string, to: string | undefined) => open?.t.status === TransferStatus.DRAFT ? <DraftAuthorize origin={origin} row={open} from={main.address} to={to} lock={lock} version={version} /> : null;
  // A payout DRAFT goes only to a saved, confirmed destination: its receiver is that one's address.
  const payDraftTo = destinations.find(d => d.confirmed && d.address.toLowerCase() === payOpen?.t.destinationAddress.toLowerCase())?.address;

  return <section className="card pad mtl" data-testid="bsc-wallet-manual" aria-label="Manual transfers">
    <PanelHead title="Manual transfers" extra={<span className="tiny">From the Main account · prepare, then authorize</span>} />
    <div className="grid cols2">
      <div className="subcard" data-testid={FUND}>
        <strong>Main → Trading · add capital</strong>
        <p className="small">Capital for trading, never counted as profit. The receiver is fixed: the Trading account of the native owner.</p>
        <DetailLine label="From · Main" value={main.balanceWei === undefined ? "—" : `${bnb(main.balanceWei, 6)} BNB · available ${main.availableWei === undefined ? "—" : bnb(main.availableWei, 6)}`} />
        <DetailLine label="To · Trading (fixed)" value={<span className="wallet-address">{target?.address || "—"}</span>} />
        <Amount id={`${FUND}-amount`} label="Amount · BNB" value={fund} onChange={setFund} check={fundAmount} />
        <DetailLine label="Fee estimate" value={fee === undefined ? "— (owner)" : `${bnb(fee, 6)} BNB`} />
        <DetailLine label="Main after transfer · preview" testId={`${FUND}-rest`} value={fundRest ? fundRest.ok ? <Wei wei={fundRest.value} places={6} /> : fundRest.error : "—"} />
        <TransferLines row={lastOf(rows, [WalletMovementKind.CAPITAL_FUNDING])} empty={t.ok ? "None yet" : "—"} testId={`${FUND}-last`} />
        {fundLock ? <Locked reason={fundLock} testId={`${FUND}-locked`} /> : null}
        <div className="wallet-tool-row"><button type="button" className="btn primary" disabled={!!fundBlock} title={fundBlock ?? undefined} data-testid={`${FUND}-review`} onClick={() => setConfirm("fund")}>Prepare funding</button></div>
        {draft(fundOpen, FUND, target?.address)}
        <CommandReceipt origin={FUND} />
      </div>
      <div className="subcard" data-testid={PAYOUT}>
        <strong>Main → External · send BNB</strong>
        <p className="small">To your MetaMask or another BSC wallet. DarkBot holds no MetaMask connection: the address only receives.</p>
        <DetailLine label="From · Main" value={main.balanceWei === undefined ? "—" : `${bnb(main.balanceWei, 6)} BNB · available ${main.availableWei === undefined ? "—" : bnb(main.availableWei, 6)}`} />
        <div className="field mt"><label htmlFor={`${PAYOUT}-destination`}>To · saved BSC address</label>
          <select id={`${PAYOUT}-destination`} className="select" value={dest?.destinationId ?? ""} onChange={e => setDestId(e.target.value)} disabled={!destinations.length} data-testid={`${PAYOUT}-destination`}>
            {destinations.length ? destinations.map(d => <option key={d.destinationId} value={d.destinationId} disabled={!d.confirmed}>{d.label || d.destinationId} · {shortAddress(d.address)}{d.confirmed ? "" : " · not confirmed"}</option>) : <option value="">No saved address</option>}
          </select>
          <small><button type="button" className="link" onClick={() => setDestDrawer(true)} data-testid={`${DEST}-open`}>Add, confirm or remove an address</button></small></div>
        <Amount id={`${PAYOUT}-amount`} label="Amount · BNB" value={pay} onChange={setPay} check={payAmount} />
        <DetailLine label="Fee estimate" value={fee === undefined ? "— (owner)" : `${bnb(fee, 6)} BNB`} />
        <DetailLine label="Main after payout · preview" testId={`${PAYOUT}-rest`} value={payRest ? payRest.ok ? <Wei wei={payRest.value} places={6} /> : payRest.error : "—"} />
        <TransferLines row={lastOf(rows, [WalletMovementKind.PAYOUT_MANUAL])} empty={t.ok ? "None yet" : "—"} testId={`${PAYOUT}-last`} />
        {payLock ? <Locked reason={payLock} testId={`${PAYOUT}-locked`} /> : null}
        <div className="wallet-tool-row"><button type="button" className="btn primary" disabled={!!payBlock} title={payBlock ?? undefined} data-testid={`${PAYOUT}-review`} onClick={() => setConfirm("payout")}>Prepare payout</button></div>
        {draft(payOpen, PAYOUT, payDraftTo)}
        <CommandReceipt origin={PAYOUT} />
      </div>
    </div>
    <p className="wallet-transfer-note">Previews only: the native owner checks balance, reserve and fee again and decides. The wallet switches are permissions, never transfers.</p>
    <WalletConfirm open={confirm === "fund"} testId={`${FUND}-dialog`} title="Prepare a funding of the Trading account?" action="Request prepare" note={PREPARE_NOTE} onCancel={() => setConfirm(null)} onConfirm={send}
      lines={[["From", `Main ${shortAddress(main.address)}`], ["To", `Trading ${target?.address ?? "—"}`], ["Amount", fundAmount?.ok ? `${bnb(fundAmount.value, 18)} BNB (${fundAmount.value} wei)` : "—"], ["Fee at most", fee === undefined ? "—" : `${bnb(fee, 18)} BNB`], ["Treasury version", version.toString()]]} />
    <WalletConfirm open={confirm === "payout"} testId={`${PAYOUT}-dialog`} title="Prepare a payout to the external wallet?" action="Request prepare" note={PREPARE_NOTE} onCancel={() => setConfirm(null)} onConfirm={send}
      lines={[["From", `Main ${shortAddress(main.address)}`], ["To", dest ? `${dest.label || dest.destinationId} ${dest.address}` : "—"], ["Amount", payAmount?.ok ? `${bnb(payAmount.value, 18)} BNB (${payAmount.value} wei)` : "—"], ["Fee at most", fee === undefined ? "—" : `${bnb(fee, 18)} BNB`], ["Treasury version", version.toString()]]} />
    {destDrawer ? <DestinationDrawer t={t} own={own} chainId={main.chainId} onClose={() => setDestDrawer(false)} /> : null}
  </section>;
}

/** Save an external BSC address (unconfirmed), confirm a saved one on its own, or remove one. */
function DestinationDrawer({ t, own, chainId, onClose }: { t: Treasury; own: string[]; chainId?: bigint; onClose: () => void }) {
  const { busy } = useWalletControl();
  const [label, setLabel] = useState("");
  const [address, setAddress] = useState("");
  const [checked, setChecked] = useState<string | null>(null);
  const [ask, setAsk] = useState<{ remove?: WalletDestination; confirm?: WalletDestination } | null>(null);
  const check = address.trim() ? externalAddress(address, own) : null;
  const lock = !t.ok ? t.reason : chainId === undefined ? "The Main account reports no chain id" : busy ? "Another wallet request is being decided" : null;
  const block = lock ?? (!label.trim() ? "Enter a label" : !check ? "Enter a BSC address" : !check.ok ? check.error : null);
  const version = t.ok ? t.state.treasuryVersion : 0n;
  const send = () => {
    if (ask?.remove && !lock && chainId !== undefined && !ask.remove.inUse)
      void walletCommand(DEST, "bsc.wallet.destination.set", ask.remove.destinationId, v8Params("destination_set_command", { remove: true, chainId }), version, `remove ${ask.remove.destinationId}`);
    else if (ask?.confirm && !lock && checked === ask.confirm.destinationId && ask.confirm.chainId === chainId)
      void walletCommand(DEST, "bsc.wallet.destination.confirm", ask.confirm.destinationId, v8Params("destination_confirm_command", { destinationId: ask.confirm.destinationId, address: ask.confirm.address }), version, `confirm ${shortAddress(ask.confirm.address)}`);
    else if (!ask?.remove && !ask?.confirm && !block && check?.ok && chainId !== undefined)
      void walletCommand(DEST, "bsc.wallet.destination.set", "", v8Params("destination_set_command", { label: label.trim(), address: check.value, chainId }), version, `save ${shortAddress(check.value)}`);
    setAsk(null);
  };
  return <Drawer title="External BSC address" kicker="WALLETS" onClose={onClose} testId={DEST}
    footer={<><button type="button" className="btn quiet" onClick={onClose}>Close</button><button type="button" className="btn primary" disabled={!!block} title={block ?? undefined} data-testid={`${DEST}-save`} onClick={() => setAsk({})}>Save address</button></>}>
    <p className="small">A receiving address on BNB Smart Chain (Main's chain {chainId?.toString() ?? "unknown"}), e.g. your MetaMask account. Only its address is stored, never a key; DarkBot does not connect to MetaMask. A saved address receives only after you confirm it on its own.</p>
    {t.ok ? t.state.destinations.map(d => <div className="subcard mt" key={d.destinationId} data-testid={`${DEST}-${d.destinationId}`}><strong>{d.label || d.destinationId}</strong>
      <div className="wallet-address" style={{ margin: "8px 0" }}>{d.address}</div><span className="tiny">{d.confirmed ? "Confirmed" : "Not confirmed"} · chain {d.chainId.toString()}{d.inUse ? " · the automatic payout pays to it" : ""}</span>
      {!d.confirmed ? <label className="check-label mt"><input type="checkbox" checked={checked === d.destinationId} onChange={e => setChecked(e.target.checked ? d.destinationId : null)} data-testid={`${DEST}-check-${d.destinationId}`} /> I checked this address in my wallet app on BNB Smart Chain</label> : null}
      <div className="wallet-tool-row">
        {!d.confirmed ? <button type="button" className="btn sm primary" disabled={!!lock || checked !== d.destinationId || d.chainId !== chainId} title={lock ?? (d.chainId !== chainId ? `Saved for chain ${d.chainId}, Main is on chain ${chainId}` : checked !== d.destinationId ? "Confirm that you checked the address" : undefined)} data-testid={`${DEST}-confirm-${d.destinationId}`} onClick={() => setAsk({ confirm: d })}>Confirm address</button> : null}
        <button type="button" className="btn sm quiet" disabled={!!lock || d.inUse} title={lock ?? (d.inUse ? "The automatic payout pays to it: choose another recipient or disable it first" : undefined)} data-testid={`${DEST}-remove-${d.destinationId}`} onClick={() => setAsk({ remove: d })}>Remove</button></div></div>) : null}
    <div className="field mt"><label htmlFor={`${DEST}-label`}>Label</label><input id={`${DEST}-label`} className="input" value={label} onChange={e => setLabel(e.target.value)} placeholder="MetaMask" data-testid={`${DEST}-label`} /></div>
    <div className="field mt"><label htmlFor={`${DEST}-address`}>BSC address</label><input id={`${DEST}-address`} className="input" value={address} onChange={e => setAddress(e.target.value)} placeholder="0x…" autoComplete="off" spellCheck={false} data-testid={`${DEST}-address`} aria-invalid={check?.ok === false} />
      <small data-testid={`${DEST}-check`}>{check === null ? "0x and 40 hex digits; the owner checksums it" : check.ok ? "Format valid; the owner checksums it" : check.error}</small></div>
    {lock ? <Locked reason={lock} testId={`${DEST}-locked`} /> : null}
    <CommandReceipt origin={DEST} />
    <WalletConfirm open={!!ask} testId={`${DEST}-dialog`} title={ask?.remove ? "Remove this address?" : ask?.confirm ? "Confirm this address?" : "Save this external address?"}
      action={ask?.remove ? "Request removal" : ask?.confirm ? "Request confirmation" : "Request save"} onCancel={() => setAsk(null)} onConfirm={send}
      lines={ask?.remove ? [["Destination", `${ask.remove.label || ask.remove.destinationId} ${ask.remove.address}`]]
        : ask?.confirm ? [["Destination", ask.confirm.label || ask.confirm.destinationId], ["Address", ask.confirm.address], ["Chain", ask.confirm.chainId.toString()]]
        : [["Label", label.trim()], ["Address", address.trim()], ["Chain", chainId?.toString() ?? "—"], ["Then", "saved unconfirmed: confirm it on its own before it receives"]]} />
  </Drawer>;
}

import { useState } from "react";
import { useWalletControl, walletCommand } from "@/core/realtime/bscControl";
import { v8Params, WalletMovementKind, WalletRole, type WalletTreasuryState } from "../api/bscV8Adapter";
import { externalAddress, lastOf, openOf, positiveWei, remaining, type AccountView, type Treasury, type TransferRow } from "../model/wallets";
import { CommandReceipt, Locked, TransferLines, WalletConfirm, shortAddress } from "./WalletParts";
import { DetailLine, Drawer, PanelHead, Wei, bnb } from "./V8";

const FUND = "bsc-wallet-fund";
const PAYOUT = "bsc-wallet-send";
const DEST = "bsc-wallet-destination";

/** What still stands between the form and one request (the first reason), or null. */
function blocker(t: Treasury, main: AccountView, busy: boolean, open: TransferRow | undefined, fee: string | undefined): string | null {
  if (!t.ok) return t.reason;
  if (main.state !== "READY" || !main.ownerKey) return `Main account ${main.state}${main.reason ? `: ${main.reason}` : ""}`;
  if (fee === undefined) return "The owner reports no fee estimate";
  if (open) return `A transfer of this path is ${open.label.toLowerCase()} (request ${open.t.requestId}): the chain decides first, nothing is sent twice`;
  if (busy) return "Another wallet request is being decided";
  return null;
}

function Amount({ id, label, value, onChange, check }: { id: string; label: string; value: string; onChange: (v: string) => void; check: { ok: true; value: string } | { ok: false; error: string } | null }) {
  return <div className="field mt"><label htmlFor={id}>{label}</label>
    <input id={id} className="input" inputMode="decimal" autoComplete="off" spellCheck={false} placeholder="0.00" value={value} onChange={e => onChange(e.target.value)} data-testid={id} aria-invalid={check?.ok === false} />
    <small data-testid={`${id}-check`}>{check === null ? "Exact BNB, at most 18 places" : check.ok ? `${check.value} wei` : check.error}</small></div>;
}

/** 3 · Manual transfers: Main → Trading (capital) and Main → external (payout), each its own request. */
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

  const fundAmount = fund.trim() ? positiveWei(fund, "Amount") : null;
  const fundRest = fundAmount?.ok ? remaining(main.availableWei, fundAmount.value, fee) : null;
  const fundOpen = openOf(rows, [WalletMovementKind.CAPITAL_FUNDING]);
  // An owner reason is shown as a lock; an input reason stays at its field (the amount, the preview).
  const fundLock = blocker(t, main, busy, fundOpen, fee) ?? (!target?.address ? "The treasury owner reports no Trading account" : main.chainId === undefined ? "No chain id reported" : null);
  const fundBlock = fundLock ?? (!fundAmount ? "Enter an amount" : !fundAmount.ok ? fundAmount.error : fundRest && !fundRest.ok ? fundRest.error : null);

  const payAmount = pay.trim() ? positiveWei(pay, "Amount") : null;
  const payRest = payAmount?.ok ? remaining(main.availableWei, payAmount.value, fee) : null;
  const payOpen = openOf(rows, [WalletMovementKind.PAYOUT_MANUAL]);
  const own = [main.address, accounts.buyer.address, accounts.launcher.address];
  const destCheck = dest ? externalAddress(dest.address, own) : null;
  const payLock = blocker(t, main, busy, payOpen, fee)
    ?? (!dest ? "Save and confirm an external BSC address first" : !dest.confirmed ? "The external address is not confirmed" : destCheck && !destCheck.ok ? destCheck.error : null);
  const payBlock = payLock ?? (!payAmount ? "Enter an amount" : !payAmount.ok ? payAmount.error : payRest && !payRest.ok ? payRest.error : null);

  const version = s?.treasuryVersion ?? 0n;
  const send = () => {
    if (confirm === "fund" && fundAmount?.ok && fee !== undefined && !fundBlock)
      void walletCommand(FUND, "bsc.wallet.fund_trading", "", v8Params("fund_trading_command", { amountWei: fundAmount.value, maxFeeWei: fee, chainId: main.chainId ?? 0n }), version, `fund Trading ${bnb(fundAmount.value, 6)} BNB`);
    if (confirm === "payout" && payAmount?.ok && dest && fee !== undefined && !payBlock)
      void walletCommand(PAYOUT, "bsc.wallet.payout", dest.destinationId, v8Params("payout_command", { destinationId: dest.destinationId, destinationAddress: dest.address, chainId: dest.chainId, amountWei: payAmount.value, maxFeeWei: fee }), version, `payout ${bnb(payAmount.value, 6)} BNB to ${shortAddress(dest.address)}`);
    setConfirm(null);
  };

  return <section className="card pad mtl" data-testid="bsc-wallet-manual" aria-label="Manual transfers">
    <PanelHead title="Manual transfers" extra={<span className="tiny">From the Main account · one request each</span>} />
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
        <div className="wallet-tool-row"><button type="button" className="btn primary" disabled={!!fundBlock} title={fundBlock ?? undefined} data-testid={`${FUND}-review`} onClick={() => setConfirm("fund")}>Review funding</button></div>
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
          <small><button type="button" className="link" onClick={() => setDestDrawer(true)} data-testid={`${DEST}-open`}>Add or remove an address</button></small></div>
        <Amount id={`${PAYOUT}-amount`} label="Amount · BNB" value={pay} onChange={setPay} check={payAmount} />
        <DetailLine label="Fee estimate" value={fee === undefined ? "— (owner)" : `${bnb(fee, 6)} BNB`} />
        <DetailLine label="Main after payout · preview" testId={`${PAYOUT}-rest`} value={payRest ? payRest.ok ? <Wei wei={payRest.value} places={6} /> : payRest.error : "—"} />
        <TransferLines row={lastOf(rows, [WalletMovementKind.PAYOUT_MANUAL])} empty={t.ok ? "None yet" : "—"} testId={`${PAYOUT}-last`} />
        {payLock ? <Locked reason={payLock} testId={`${PAYOUT}-locked`} /> : null}
        <div className="wallet-tool-row"><button type="button" className="btn primary" disabled={!!payBlock} title={payBlock ?? undefined} data-testid={`${PAYOUT}-review`} onClick={() => setConfirm("payout")}>Review payout</button></div>
        <CommandReceipt origin={PAYOUT} />
      </div>
    </div>
    <p className="wallet-transfer-note">Previews only: the native owner checks balance, reserve and fee again and decides. The wallet switches are permissions, never transfers.</p>
    <WalletConfirm open={confirm === "fund"} testId={`${FUND}-dialog`} title="Fund the Trading account?" action="Request funding" onCancel={() => setConfirm(null)} onConfirm={send}
      lines={[["From", `Main ${shortAddress(main.address)}`], ["To", `Trading ${target?.address ?? "—"}`], ["Amount", fundAmount?.ok ? `${bnb(fundAmount.value, 18)} BNB (${fundAmount.value} wei)` : "—"], ["Fee at most", fee === undefined ? "—" : `${bnb(fee, 18)} BNB`], ["Treasury version", version.toString()]]} />
    <WalletConfirm open={confirm === "payout"} testId={`${PAYOUT}-dialog`} title="Send BNB to the external wallet?" action="Request payout" onCancel={() => setConfirm(null)} onConfirm={send}
      lines={[["From", `Main ${shortAddress(main.address)}`], ["To", dest ? `${dest.label || dest.destinationId} ${dest.address}` : "—"], ["Amount", payAmount?.ok ? `${bnb(payAmount.value, 18)} BNB (${payAmount.value} wei)` : "—"], ["Fee at most", fee === undefined ? "—" : `${bnb(fee, 18)} BNB`], ["Treasury version", version.toString()]]} />
    {destDrawer ? <DestinationDrawer t={t} own={own} chainId={main.chainId ?? accounts.buyer.chainId} onClose={() => setDestDrawer(false)} /> : null}
  </section>;
}

/** Save (or remove) an external BSC address after the operator confirmed it. */
function DestinationDrawer({ t, own, chainId, onClose }: { t: Treasury; own: string[]; chainId?: bigint; onClose: () => void }) {
  const { busy } = useWalletControl();
  const [label, setLabel] = useState("");
  const [address, setAddress] = useState("");
  const [checked, setChecked] = useState(false);
  const [ask, setAsk] = useState<{ remove?: string } | null>(null);
  const check = address.trim() ? externalAddress(address, own) : null;
  const lock = !t.ok ? t.reason : chainId === undefined ? "No chain id reported" : busy ? "Another wallet request is being decided" : null;
  const block = lock ?? (!label.trim() ? "Enter a label" : !check ? "Enter a BSC address" : !check.ok ? check.error : !checked ? "Confirm that you checked the address" : null);
  const version = t.ok ? t.state.treasuryVersion : 0n;
  const send = () => {
    if (ask?.remove && !lock && chainId !== undefined) void walletCommand(DEST, "bsc.wallet.destination.set", ask.remove, v8Params("destination_set_command", { remove: true, chainId }), version, `remove ${ask.remove}`);
    else if (!ask?.remove && !block && check?.ok && chainId !== undefined) void walletCommand(DEST, "bsc.wallet.destination.set", "", v8Params("destination_set_command", { label: label.trim(), address: check.value, chainId, confirmed: true }), version, `save ${shortAddress(check.value)}`);
    setAsk(null);
  };
  return <Drawer title="External BSC address" kicker="WALLETS" onClose={onClose} testId={DEST}
    footer={<><button type="button" className="btn quiet" onClick={onClose}>Close</button><button type="button" className="btn primary" disabled={!!block} title={block ?? undefined} data-testid={`${DEST}-save`} onClick={() => setAsk({})}>Save address</button></>}>
    <p className="small">A receiving address on BNB Smart Chain (chain {chainId?.toString() ?? "unknown"}), e.g. your MetaMask account. Only its address is stored, never a key; DarkBot does not connect to MetaMask.</p>
    {t.ok ? t.state.destinations.map(d => <div className="subcard mt" key={d.destinationId}><strong>{d.label || d.destinationId}</strong>
      <div className="wallet-address" style={{ margin: "8px 0" }}>{d.address}</div><span className="tiny">{d.confirmed ? "Confirmed" : "Not confirmed"} · chain {d.chainId.toString()}</span>
      <div className="wallet-tool-row"><button type="button" className="btn sm quiet" disabled={!!lock} title={lock ?? undefined} onClick={() => setAsk({ remove: d.destinationId })}>Remove</button></div></div>) : null}
    <div className="field mt"><label htmlFor={`${DEST}-label`}>Label</label><input id={`${DEST}-label`} className="input" value={label} onChange={e => { setLabel(e.target.value); setChecked(false); }} placeholder="MetaMask" data-testid={`${DEST}-label`} /></div>
    <div className="field mt"><label htmlFor={`${DEST}-address`}>BSC address</label><input id={`${DEST}-address`} className="input" value={address} onChange={e => { setAddress(e.target.value); setChecked(false); }} placeholder="0x…" autoComplete="off" spellCheck={false} data-testid={`${DEST}-address`} aria-invalid={check?.ok === false} />
      <small data-testid={`${DEST}-check`}>{check === null ? "0x and 40 hex digits; the owner checksums it" : check.ok ? "Format valid; the owner checksums it" : check.error}</small></div>
    <label className="check-label mt"><input type="checkbox" checked={checked} onChange={e => setChecked(e.target.checked)} data-testid={`${DEST}-confirmed`} /> I checked this address in my wallet app on BNB Smart Chain</label>
    {lock ? <Locked reason={lock} testId={`${DEST}-locked`} /> : null}
    <CommandReceipt origin={DEST} />
    <WalletConfirm open={!!ask} testId={`${DEST}-dialog`} title={ask?.remove ? "Remove this address?" : "Save this external address?"} action={ask?.remove ? "Request removal" : "Request save"} onCancel={() => setAsk(null)} onConfirm={send}
      lines={ask?.remove ? [["Destination", ask.remove]] : [["Label", label.trim()], ["Address", address.trim()], ["Chain", chainId?.toString() ?? "—"]]} />
  </Drawer>;
}

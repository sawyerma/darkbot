import { useState } from "react";
import type { SwitchRow } from "../model/switches";
import type { AccountId, AccountView, Economics, Treasury } from "../model/wallets";
import type { BscWalletsView } from "../model/types";
import { SwitchActions } from "../SwitchActions";
import { ValueState } from "./EvidenceRows";
import { copyText, shortAddress } from "./WalletParts";
import { DetailLine, Icon, PanelHead, Stat, Wei, bnb, signTone, signedBnbText } from "./V8";

const ROLE_TEXT: Record<AccountId, string> = {
  main: "Treasury: holds capital and receives the trading profit. Its key is only at the native BSC owner.",
  buyer: "Trading capital: BUY and SELL, realized PnL. Its key is only at the native BSC owner.",
  external: "Your MetaMask or other BSC wallet: receives payouts only. DarkBot holds no connection to it and never its key.",
  launcher: "Controlled G0.7 test launches only: never part of a profit or payout path.",
};
const balanceText = (a: AccountView) => a.balanceWei === undefined ? "—" : `${bnb(a.balanceWei, 2)} BNB`;

function CopyAddress({ address }: { address: string }) {
  const [copied, setCopied] = useState(false);
  return <button type="button" className="link" disabled={!address} onClick={() => { void copyText(address).then(setCopied); }}><Icon name="copy" size="s" />{copied ? "Copied" : "Copy address"}</button>;
}

function AccountDetail({ a, w, switchRow }: { a: AccountView; w: BscWalletsView | null; switchRow?: SwitchRow }) {
  const required = a.id === "buyer" ? w?.requiredBuyerWei : a.id === "launcher" ? w?.requiredLauncherWei : undefined;
  return <div data-testid={`bsc-wallet-detail-${a.id}`}>
    <p className="small" style={{ margin: "12px 0 4px" }}>{ROLE_TEXT[a.id]}</p>
    <DetailLine label="Address" value={<span className="wallet-address">{a.address || "—"}</span>} />
    <DetailLine label="State" value={<span>{a.state}{a.reason ? <span className="tiny" style={{ display: "block" }}>{a.reason}</span> : null}</span>} />
    {a.id === "main" && a.account?.control ? <DetailLine label="Main switch" value={`${a.account.control.id} · requested ${a.account.control.requested || "—"} · generation ${a.account.control.generation.toString()}`} /> : null}
    <DetailLine label="Chain" value={a.chainId?.toString() ?? "—"} />
    <DetailLine label="Balance source" value={`${a.source} · ${a.quality}`} />
    <DetailLine label="Exact balance" value={<Wei wei={a.balanceWei} places={18} />} />
    {a.id === "buyer" && w?.liveBalanceWei !== undefined ? <DetailLine label="Snapshot balance" value={<Wei wei={w.buyerBalanceWei} places={6} />} /> : null}
    {a.id === "main" || a.id === "buyer" ? <DetailLine label="Next nonce" value={a.nonce ?? "— (not reported)"} /> : null}
    {required !== undefined ? <DetailLine label="Required balance" value={`${bnb(required)} BNB`} /> : null}
    {switchRow ? <>
      <DetailLine label={a.id === "buyer" ? "Allow trading" : "Allow test launches"} value={<ValueState state={switchRow.effective} />} />
      <SwitchActions rows={[switchRow]} testId={`bsc-wallet-${a.id}`} action="bsc.wallet.set" />
    </> : null}
    <CopyAddress address={a.address} />
  </div>;
}

/** 1 · Accounts: Main, Trading and the external wallet; the G0.7 launcher separately. */
export function WalletAccounts({ accounts, w, launcherSwitch, buyerSwitch, onReceive }: { accounts: Record<AccountId, AccountView>; w: BscWalletsView | null; launcherSwitch: SwitchRow; buyerSwitch: SwitchRow; onReceive: () => void }) {
  const [selected, setSelected] = useState<"main" | "buyer" | "external">("buyer");
  const pick = accounts[selected];
  const launcher = accounts.launcher;
  return <section className="card pad" data-testid="bsc-wallet-accounts" aria-label="Accounts">
    <PanelHead title="Accounts" extra={<button type="button" className="btn sm quiet" onClick={onReceive}>Deposit address</button>} />
    <div className="wallet-tabs v8-three">
      {(["main", "buyer", "external"] as const).map(id => <button key={id} type="button" className={`wallet-mini${selected === id ? " active" : ""}`} data-testid={`bsc-wallet-${id}`} aria-pressed={selected === id} onClick={() => setSelected(id)}>
        <span>{accounts[id].name}</span><b className="mono">{id === "external" ? accounts[id].quality === "UNKNOWN" ? "—" : accounts[id].quality : balanceText(accounts[id])}</b><span>{shortAddress(accounts[id].address)}</span></button>)}
    </div>
    <AccountDetail key={pick.id} a={pick} w={w} switchRow={selected === "buyer" ? buyerSwitch : undefined} />
    <div className="subcard mt" data-testid="bsc-wallet-launcher">
      <div className="topline"><strong>{launcher.name}</strong><b className="mono">{balanceText(launcher)}</b></div>
      <AccountDetail a={launcher} w={w} switchRow={launcherSwitch} />
    </div>
  </section>;
}

/** 2 · Balances: each account apart (never summed), Trading and Main economics kept apart. */
export function WalletBalances({ accounts, eco, treasury }: { accounts: Record<AccountId, AccountView>; eco: Economics; treasury: Treasury }) {
  const pnl = eco.realizedNetPnlWei;
  const ownerOnly = treasury.ok ? "Not reported by the treasury owner" : "Needs the native treasury owner";
  const v = (wei: string | undefined) => wei === undefined ? <span title={ownerOnly}>—</span> : <Wei wei={wei} places={6} />;
  return <section className="card pad mtl" data-testid="bsc-wallet-balances" aria-label="Balances">
    <PanelHead title="Balances" extra={<span className="tiny">Per account · never summed</span>} />
    <div className="grid cols4">
      <Stat label="Main account" value={balanceText(accounts.main)} note={accounts.main.balanceWei === undefined ? accounts.main.reason || "Not reported" : `${accounts.main.source}`} testId="bsc-wallets-balance-main" title={accounts.main.balanceWei ? `${accounts.main.balanceWei} wei` : undefined} />
      <Stat label="Trading account" value={balanceText(accounts.buyer)} note={accounts.buyer.source} testId="bsc-wallets-balance" title={accounts.buyer.balanceWei ? `${accounts.buyer.balanceWei} wei` : undefined} />
      <Stat label="Realized net PnL" value={pnl === undefined ? "—" : `${eco.lowerBound ? "≥ " : ""}${signedBnbText(pnl)}`} note={`${eco.realizedSource}${eco.lowerBound ? " · lower bound: a SELL without the Portal trade event" : ""}`} accent={signTone(pnl)} testId="bsc-wallets-pnl" title={pnl ? `${pnl} wei` : undefined} />
      <Stat label="Profit not yet transferred" value={eco.untransferredWei === undefined ? "—" : `${bnb(eco.untransferredWei, 3)} BNB`} note={eco.untransferredWei === undefined ? ownerOnly : "Realized, net, not yet on Main"} testId="bsc-wallets-untransferred" />
    </div>
    <div className="grid cols3 mt">
      {(["main", "buyer", "launcher"] as const).map(id => <div className="subcard" key={id} data-testid={`bsc-wallet-balance-${id}`}>
        <strong>{accounts[id].name}</strong>
        <DetailLine label="Confirmed" value={<Wei wei={accounts[id].balanceWei} places={6} />} />
        <DetailLine label="Reserved" value={v(accounts[id].reservedWei)} />
        <DetailLine label="Available" value={v(accounts[id].availableWei)} />
        <DetailLine label="Quality" value={accounts[id].quality} />
      </div>)}
    </div>
    <div className="grid cols2 mt">
      <div className="subcard" data-testid="bsc-wallet-economics-buyer"><strong>Trading account economics</strong>
        <DetailLine label="Capital basis" value={v(eco.capitalBasisWei)} />
        <DetailLine label="Realized net PnL" value={pnl === undefined ? "—" : <Wei wei={pnl} places={6} signed />} />
        <DetailLine label="Already transferred to Main" value={v(eco.transferredWei)} />
        <DetailLine label="Not yet transferred" value={v(eco.untransferredWei)} />
      </div>
      <div className="subcard" data-testid="bsc-wallet-economics-main"><strong>Main account economics</strong>
        <DetailLine label="Profit received from Trading" value={v(eco.profitReceivedWei)} />
        <DetailLine label="Other capital (deposits, returns)" value={v(eco.otherCapitalWei)} />
        <p className="tiny mt">Main may hold capital: its balance is never shown as trading profit, and an internal transfer is never counted twice.</p>
      </div>
    </div>
  </section>;
}

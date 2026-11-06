import { useState } from "react";
import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { selectLive, selectStatus, selectWallets, useBscStore } from "../store/bscStore";
import { switchRow, type SwitchRow } from "../model/switches";
import { Line } from "@/shared/ui/graphite/Rows";
import { capital } from "../model/overview";
import { SwitchActions } from "../SwitchActions";
import { PerformanceMetric, PerformanceScope, WalletMovementKind } from "../api/bscV8Adapter";
import { timestamp, ValueState } from "./EvidenceRows";
import { PlacedSection } from "./PlacedSection";
import { useNativeRecords } from "./useNativeRecords";
import { queryReason, useBscQuery } from "./useBscQuery";
import { DetailLine, Drawer, Icon, PanelHead, Stat, TopHeading, bnb, signTone, signedBnbText } from "./V8";

type WalletId = "launcher" | "buyer";

/** One wallet switch: effective state, what was requested, why, the CAS generation and the request
 *  that set it (the pre-V8 rows, kept: the operator sees which request, GUI or terminal, set it). */
function SwitchLines({ row }: { row: SwitchRow }) {
  return <>
    <Line label={`${row.label} switch`} value={<ValueState state={row.effective} />}
      hint={`requested ${row.requested}${row.blockReason ? ` · ${row.blockReason}` : ""} · ${row.hint}; DISABLED initially and after every restart`} />
    <Line label={`${row.label} switch generation`} value={row.available ? row.generation.toString() : row.reason} hint={`Native owner generation (CAS) · last set by ${row.receipt}`} />
  </>;
}
type Period = "7T" | "30T" | "All";
const PERIODS: ReadonlyArray<[Period, string]> = [["7T", "7 days"], ["30T", "30 days"], ["All", "All time"]];
const DAY_MS = 86_400_000;
const NO_TRANSFER_OWNER = "No native BSC transfer owner yet (BSC-V8-15/17): nothing can be sent from here.";
const NO_SWEEP_OWNER = "No native sweep policy owner yet (BSC-V8-16): no rule is stored or run.";

async function copy(text: string): Promise<boolean> {
  try { await navigator.clipboard.writeText(text); return true; } catch { return false; }
}

/** V8 Wallets (FE-V8-13…16): balances, the two wallet switches, history, transfers and the
 *  automatic transfer. Balances and switches are the native owner's; history, transfer and sweep
 *  have no native owner yet and say so (no draft, no simulated success). */
export function WalletsPanel() {
  const quality = useTopic(TopicId.BSC_WALLETS_STATE);
  useTopic(TopicId.BSC_LAUNCH_STATE); // preflight funding records (placed wallet evidence)
  const { projection: w } = useBscStore(selectWallets);
  const { projection: live } = useBscStore(selectLive);
  const { projection: s } = useBscStore(selectStatus);
  const records = useNativeRecords();
  const [selected, setSelected] = useState<WalletId | "all">("all");
  const [period, setPeriod] = useState<Period>("7T");
  const [chart, setChart] = useState<"balance" | "profit">("balance");
  const [movement, setMovement] = useState<"all" | "deposit" | "withdraw" | "profit">("all");
  const [layer, setLayer] = useState<"receive" | "send" | "sweep" | null>(null);
  const [copied, setCopied] = useState("");
  const cap = capital(w);
  const launcher = switchRow("BSC_WALLET_LAUNCHER", w?.launcherControl);
  const buyer = switchRow("BSC_WALLET_BUYER", w?.buyerControl);
  const wallets: { id: WalletId; name: string; address: string; balance?: string; source: string }[] = [
    { id: "launcher", name: "Funding wallet", address: w?.launcherAddress ?? "", balance: w?.launcherBalanceWei, source: w?.source || "UNKNOWN" },
    { id: "buyer", name: "Trading wallet", address: w?.buyerAddress ?? "", balance: w?.liveBalanceWei ?? w?.buyerBalanceWei, source: w?.liveBalanceWei !== undefined ? "LIVE probe" : w?.source || "UNKNOWN" },
  ];
  const from = period === "All" ? undefined : BigInt(Date.now() - (period === "7T" ? 7 : 30) * DAY_MS);
  const series = useBscQuery(`series:${chart}:${period}:${selected}`, { case: "series", value: {
    metric: chart === "balance" ? PerformanceMetric.WALLET_BALANCE : PerformanceMetric.WALLET_TRADING_PNL,
    scope: selected === "all" ? PerformanceScope.ALL_WALLETS : PerformanceScope.WALLET, scopeId: selected === "all" ? "" : selected, fromUnixMs: from } });
  const kinds = movement === "deposit" ? [WalletMovementKind.DEPOSIT] : movement === "withdraw" ? [WalletMovementKind.WITHDRAWAL] : movement === "profit" ? [WalletMovementKind.TRADE_BUY, WalletMovementKind.TRADE_SELL] : [];
  const movements = useBscQuery(`movements:${movement}:${selected}`, { case: "walletMovements", value: { kinds } });
  const pick = wallets.find(x => x.id === selected);
  const doCopy = (text: string) => { void copy(text).then(ok => setCopied(ok ? text : "")); };

  return <>
    <TopHeading title="Wallets" subtitle="Balances, transfers and PnL history." />
    <div className="grid cols2">
      <Stat label="Balance" value={cap.totalWei === undefined ? "—" : `${bnb(cap.totalWei, 2)} BNB`} note={`All wallets · ${cap.detail}`} testId="bsc-wallets-balance" />
      <Stat label="Realized net PnL" value={live ? signedBnbText(live.realizedPnlWei) : "—"} note="Net trading PnL (LIVE ledger)" accent={signTone(live?.realizedPnlWei)} testId="bsc-wallets-pnl" />
    </div>
    <div className="grid wideleft mtl">
      <div className="card pad">
        <PanelHead title="History" extra={<div className="segment" role="group" aria-label="Time range">{PERIODS.map(([v, t]) => <button key={v} type="button" className={period === v ? "active" : undefined} onClick={() => setPeriod(v)}>{t}</button>)}</div>} />
        <div className="segment" role="group" aria-label="Chart type" style={{ marginBottom: 13 }}>
          <button type="button" className={chart === "balance" ? "active" : undefined} onClick={() => setChart("balance")}>Balance</button>
          <button type="button" className={chart === "profit" ? "active" : undefined} onClick={() => setChart("profit")}>Profit &amp; loss</button>
        </div>
        <div className="empty" data-testid="bsc-wallet-history">{series.kind === "ANSWER" ? "No verified points in this period." : queryReason(series, "the native wallet ledger (BSC-V8-13/14)")}</div>
        <div className="chart-foot"><span>{chart === "balance" ? "Balance including transfers" : "Trading PnL excluding transfers"}</span><span>Native wallet ledger only</span></div>
      </div>
      <div className="card pad">
        <PanelHead title="My wallets" extra={<button type="button" className="btn sm quiet" onClick={() => setLayer("receive")}>Deposit address</button>} />
        <div className="wallet-tabs">
          {wallets.map(x => <button key={x.id} type="button" className={`wallet-mini${selected === x.id ? " active" : ""}`} data-testid={`bsc-wallet-${x.id}`} onClick={() => setSelected(x.id)}>
            <span>{x.name}</span><b className="mono">{x.balance === undefined ? "—" : `${bnb(x.balance, 2)} BNB`}</b><span>{x.address ? `${x.address.slice(0, 8)}…${x.address.slice(-6)}` : "No address"}</span></button>)}
        </div>
        <div className="mt"><button type="button" className="btn sm quiet" onClick={() => setSelected("all")}>All wallets</button></div>
        {pick ? <>
          <DetailLine label="Address" value={<span className="wallet-address">{pick.address || "—"}</span>} />
          <DetailLine label="Balance source" value={`${pick.source} · ${pick.id === "buyer" && w?.liveBalanceWei !== undefined ? "current runtime status" : timestamp(w?.observedUnixS)}`} />
          <DetailLine label={pick.id === "buyer" ? "Allow trading" : "Allow test launches"} value={<ValueState state={(pick.id === "buyer" ? buyer : launcher).effective} />} />
          <SwitchActions rows={[pick.id === "buyer" ? buyer : launcher]} testId={`bsc-wallet-${pick.id}`} action="bsc.wallet.set" />
          <DetailLine label="Required balance" value={(pick.id === "buyer" ? w?.requiredBuyerWei : w?.requiredLauncherWei) === undefined ? "—" : `${bnb(pick.id === "buyer" ? w?.requiredBuyerWei : w?.requiredLauncherWei)} BNB`} />
          {pick.id === "buyer" ? <DetailLine label="Next nonce" value={w?.liveNextNonce?.toString() ?? live?.nextNonce?.toString() ?? "— (not reported)"} /> : null}
          <button type="button" className="link" disabled={!pick.address} onClick={() => doCopy(pick.address)}><Icon name="copy" size="s" />{copied === pick.address && copied ? "Copied" : "Copy address"}</button>
        </> : null}
        <div className="v8-section-group">
          <DetailLine label="Wallet data" value={<ValueState state={quality} />} />
          <DetailLine label="Snapshot" value={`${w?.source || "UNKNOWN"} · block ${w?.observedBlock?.toString() ?? "—"} · ${timestamp(w?.observedUnixS)}`} />
          <DetailLine label="Funding verdict" value={<ValueState state={s?.readiness?.fundingVerdict || undefined} />} />
        </div>
      </div>
    </div>
    <div className="wallet-action-panel mtl">
      <div className="wallet-rule-summary"><div><strong>Send funds &amp; auto-transfer</strong><p className="small" style={{ margin: "5px 0 0" }}>To MetaMask or another BSC wallet. No auto-transfer rule (no native owner).</p></div>
        <div className="wallet-tool-row"><button type="button" className="btn quiet" onClick={() => setLayer("send")}>Send funds</button><button type="button" className="btn quiet" onClick={() => setLayer("sweep")}>Auto-transfer</button></div></div>
      <p className="wallet-transfer-note">Transfers need the native BSC transfer owner (BSC-V8-15…17); the wallet switches above are permissions, never transfers.</p>
    </div>
    <div className="card mtl">
      <div className="topline" style={{ padding: "20px 22px 0" }}><h2>Transfers</h2>
        <select className="select" aria-label="Filter transactions" value={movement} onChange={e => setMovement(e.target.value as typeof movement)}>
          <option value="all">All</option><option value="deposit">Deposits</option><option value="withdraw">Withdrawals</option><option value="profit">Trading PnL</option></select></div>
      <p className="small" style={{ margin: 0, padding: "0 22px 12px" }}>{movements.kind === "ANSWER" && movements.result.case === "walletMovements" ? `Deposited: ${bnb(movements.result.value.depositedWei, 2)} BNB · Withdrawn: ${bnb(movements.result.value.withdrawnWei, 2)} BNB` : "Deposited: — · Withdrawn: —"}</p>
      <div className="table-wrap"><table><thead><tr><th>Activity</th><th>Date</th><th>Wallet</th><th className="right">Amount</th></tr></thead>
        <tbody><tr><td colSpan={4}><div className="empty" data-testid="bsc-wallet-movements">{movements.kind === "ANSWER" ? "No matching transactions." : queryReason(movements, "the native wallet ledger (BSC-V8-13)")}</div></td></tr></tbody></table></div>
    </div>
    <div className="card pad mtl" data-testid="bsc-wallet-evidence">
      <PanelHead title="Wallet evidence" />
      <DetailLine label="Configured LIVE buyer" value={<span className="wallet-address">{w?.expectedAddress || "—"}</span>} />
      <DetailLine label="Wallet state" value={w?.walletStatus || "UNKNOWN"} />
      <DetailLine label="Chain" value={w?.chainId?.toString() || "—"} />
      <DetailLine label="Snapshot evidence" value={<span className="wallet-address">{w?.evidence?.path || "—"} · {w?.evidence?.sha256 || "—"}</span>} />
      <SwitchLines row={launcher} />
      <SwitchLines row={buyer} />
      {[1, 2, 3, 4, 5, 6, 7].map(n => <PlacedSection key={n} panel="wallets" section={n} records={records} />)}
    </div>
    {layer === "receive" ? <Drawer title="Receive BNB" kicker="WALLETS" onClose={() => setLayer(null)} footer={<button type="button" className="btn primary" onClick={() => setLayer(null)}>Close</button>}>
      <p className="small">Copy a wallet address for an incoming BSC transfer (chain {w?.chainId?.toString() ?? "unknown"}).</p>
      {wallets.map(x => <div className="subcard mt" key={x.id}><strong>{x.name}</strong><div className="mono" style={{ wordBreak: "break-all", fontSize: 12, margin: "10px 0" }}>{x.address || "No address reported"}</div>
        <button type="button" className="btn quiet" disabled={!x.address} onClick={() => doCopy(x.address)}>{copied === x.address && copied ? "Copied" : "Copy address"}</button></div>)}
    </Drawer> : null}
    {layer === "send" ? <Drawer title="Send BNB" kicker="TRANSFER" onClose={() => setLayer(null)} testId="bsc-wallet-send"
      footer={<><button type="button" className="btn quiet" onClick={() => setLayer(null)}>Cancel</button><button type="button" className="btn primary" disabled title={NO_TRANSFER_OWNER}>Review transfer</button></>}>
      <p className="small">Transfer to a MetaMask or other BSC address. DarkBot holds no MetaMask connection; the address is an external destination.</p>
      <div className="notice mt"><Icon name="lock" size="s" /><div>{NO_TRANSFER_OWNER}</div></div>
      <fieldset disabled style={{ border: 0, margin: 0, padding: 0, minWidth: 0 }}>
        <div className="field mt"><label>From wallet</label><select className="select">{wallets.map(x => <option key={x.id}>{x.name} · {x.balance === undefined ? "—" : bnb(x.balance)} BNB</option>)}</select></div>
        <div className="field mt"><label>Destination address · BSC</label><input className="input" placeholder="0x…" autoComplete="off" spellCheck={false} /></div>
        <div className="field mt"><label>Amount · BNB</label><input className="input" inputMode="decimal" placeholder="0.10" /></div>
      </fieldset>
    </Drawer> : null}
    {layer === "sweep" ? <Drawer title="Automatic transfer" kicker="RULE" onClose={() => setLayer(null)} testId="bsc-wallet-sweep"
      footer={<><button type="button" className="btn quiet" onClick={() => setLayer(null)}>Cancel</button><button type="button" className="btn primary" disabled title={NO_SWEEP_OWNER}>Save rule</button></>}>
      <p className="small">A threshold and reserve for a native wallet sweep: destination, trigger balance, amount kept.</p>
      <div className="notice mt"><Icon name="lock" size="s" /><div>{NO_SWEEP_OWNER}</div></div>
      <fieldset disabled style={{ border: 0, margin: 0, padding: 0, minWidth: 0 }}>
        <label className="check-label mt"><input type="checkbox" /> Enable rule</label>
        <div className="field mt"><label>Source wallet</label><select className="select">{wallets.map(x => <option key={x.id}>{x.name}</option>)}</select></div>
        <div className="field mt"><label>Destination BSC address</label><input className="input" placeholder="0x…" autoComplete="off" spellCheck={false} /></div>
        <div className="field-row mt"><div className="field"><label>Trigger balance · BNB</label><input className="input" inputMode="decimal" /></div><div className="field"><label>Keep in wallet · BNB</label><input className="input" inputMode="decimal" /></div></div>
      </fieldset>
    </Drawer> : null}
  </>;
}

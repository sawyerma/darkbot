import { useEffect, useState } from "react";
import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { releaseWalletRequest, useWalletControl } from "@/core/realtime/bscControl";
import { selectLive, selectStatus, selectWallets, useBscStore } from "../store/bscStore";
import { switchRow, type SwitchRow } from "../model/switches";
import { accountViews, economics, transferRows, treasuryOf, type AccountId } from "../model/wallets";
import { Line } from "@/shared/ui/graphite/Rows";
import { timestamp, ValueState } from "./EvidenceRows";
import { PlacedSection } from "./PlacedSection";
import { useNativeRecords } from "./useNativeRecords";
import { useBscQuery } from "./useBscQuery";
import { WalletAccounts, WalletBalances } from "./WalletAccounts";
import { WalletTransfers } from "./WalletTransfers";
import { PayoutPolicy, ProfitPolicy } from "./WalletPolicies";
import { WalletCharts, WalletMovements } from "./WalletHistory";
import { Locked, copyText } from "./WalletParts";
import { DetailLine, Drawer, PanelHead, TopHeading } from "./V8";

/** One wallet switch: effective state, what was requested, why, the CAS generation and the request
 *  that set it (the pre-V8 rows, kept: the operator sees which request, GUI or terminal, set it). */
function SwitchLines({ row }: { row: SwitchRow }) {
  return <>
    <Line label={`${row.label} switch`} value={<ValueState state={row.effective} />}
      hint={`requested ${row.requested}${row.blockReason ? ` · ${row.blockReason}` : ""} · ${row.hint}; DISABLED initially and after every restart`} />
    <Line label={`${row.label} switch generation`} value={row.available ? row.generation.toString() : row.reason} hint={`Native owner generation (CAS) · last set by ${row.receipt}`} />
  </>;
}

/** V8 Wallets (FE-V8-13…16, operator order 06.11.): Accounts → Balances → Manual transfers →
 *  Automatic profit transfer → Automatic payout → Transaction history → Balance & profit charts.
 *  Main account, transfers and both policies belong to the native BSC treasury owner; until it
 *  answers the bounded treasury query they are shown and disabled with its reason (no draft, no
 *  simulated success). The Trading account and the launcher keep the existing native projection. */
export function WalletsPanel() {
  const quality = useTopic(TopicId.BSC_WALLETS_STATE);
  useTopic(TopicId.BSC_LAUNCH_STATE); // preflight funding records (placed wallet evidence)
  const { projection: w } = useBscStore(selectWallets);
  const { projection: live } = useBscStore(selectLive);
  const { projection: s } = useBscStore(selectStatus);
  const records = useNativeRecords();
  const { receipt } = useWalletControl();
  const [refresh, setRefresh] = useState(0);
  const [step, setStep] = useState("");
  const [receive, setReceive] = useState(false);
  const [copied, setCopied] = useState("");
  // One treasury read per view, per explicit refresh and per decided wallet request (never while
  // one is being sent): no polling. History and charts follow the same steps.
  const decided = receipt && receipt.phase !== "SENDING" ? `${receipt.id}:${receipt.phase}` : null;
  useEffect(() => { if (decided) setStep(decided); }, [decided]);
  const epoch = `${refresh}:${step}`;
  const q = useBscQuery(`treasury:${epoch}`, { case: "treasury", value: { roles: [] } });
  const t = treasuryOf(q);
  // A request whose handoff was UNKNOWN unlocks once the owner's treasury shows it.
  const unknown = receipt?.phase === "UNKNOWN" ? receipt.id : null;
  const shown = !!unknown && t.ok && ([...t.state.transfers, t.state.profitPolicy?.lastTransfer, t.state.payoutPolicy?.lastPayout].some(x => x?.requestId === unknown)
    || t.state.profitPolicy?.requestId === unknown || t.state.payoutPolicy?.requestId === unknown);
  useEffect(() => { if (shown && receipt) releaseWalletRequest(receipt.id); }, [shown, receipt]);
  const accounts = accountViews(w, t, live);
  const eco = economics(t, live);
  const rows = t.ok ? transferRows(t.state) : [];
  const launcher = switchRow("BSC_WALLET_LAUNCHER", w?.launcherControl);
  const buyer = switchRow("BSC_WALLET_BUYER", w?.buyerControl);
  const doCopy = (text: string) => { void copyText(text).then(ok => setCopied(ok ? text : "")); };

  return <>
    <TopHeading title="Wallets" subtitle="Accounts, transfers, automatic rules and history."
      actions={<button type="button" className="btn sm quiet" onClick={() => setRefresh(n => n + 1)} data-testid="bsc-wallet-refresh">Refresh</button>} />
    {!t.ok ? <Locked reason={`Treasury owner: ${t.reason}. Trading account and launcher come from the native wallet projection; the Main account, every transfer and both automatic rules stay disabled.`} testId="bsc-wallet-owner" /> : null}
    <div className="mtl"><WalletAccounts accounts={accounts} w={w} launcherSwitch={launcher} buyerSwitch={buyer} onReceive={() => setReceive(true)} /></div>
    <WalletBalances accounts={accounts} eco={eco} treasury={t} />
    <WalletTransfers t={t} accounts={accounts} rows={rows} />
    <ProfitPolicy t={t} eco={eco} rows={rows} />
    <PayoutPolicy t={t} rows={rows} own={[accounts.main.address, accounts.buyer.address, accounts.launcher.address]} />
    <WalletMovements rows={rows} epoch={epoch} />
    <WalletCharts epoch={epoch} />
    <div className="card pad mtl" data-testid="bsc-wallet-evidence">
      <PanelHead title="Wallet evidence" />
      <DetailLine label="Wallet data" value={<ValueState state={quality} />} />
      <DetailLine label="Snapshot" value={`${w?.source || "UNKNOWN"} · block ${w?.observedBlock?.toString() ?? "—"} · ${timestamp(w?.observedUnixS)}`} />
      <DetailLine label="Funding verdict" value={<ValueState state={s?.readiness?.fundingVerdict || undefined} />} />
      <DetailLine label="Configured LIVE buyer" value={<span className="wallet-address">{w?.expectedAddress || "—"}</span>} />
      <DetailLine label="Wallet state" value={w?.walletStatus || "UNKNOWN"} />
      <DetailLine label="Chain" value={w?.chainId?.toString() || "—"} />
      <DetailLine label="Snapshot evidence" value={<span className="wallet-address">{w?.evidence?.path || "—"} · {w?.evidence?.sha256 || "—"}</span>} />
      <SwitchLines row={launcher} />
      <SwitchLines row={buyer} />
      {[1, 2, 3, 4, 5, 6, 7].map(n => <PlacedSection key={n} panel="wallets" section={n} records={records} />)}
    </div>
    {receive ? <Drawer title="Receive BNB" kicker="WALLETS" onClose={() => setReceive(false)} footer={<button type="button" className="btn primary" onClick={() => setReceive(false)}>Close</button>}>
      <p className="small">Copy an address for an incoming BSC transfer (chain {(accounts.main.chainId ?? w?.chainId)?.toString() ?? "unknown"}). BNB sent to an account is capital, never trading profit.</p>
      {(["main", "buyer", "launcher"] as AccountId[]).map(id => <div className="subcard mt" key={id}><strong>{accounts[id].name}</strong>
        <div className="mono" style={{ wordBreak: "break-all", fontSize: 12, margin: "10px 0" }}>{accounts[id].address || accounts[id].reason || "No address reported"}</div>
        <button type="button" className="btn quiet" disabled={!accounts[id].address} onClick={() => doCopy(accounts[id].address)}>{copied === accounts[id].address && copied ? "Copied" : "Copy address"}</button></div>)}
    </Drawer> : null}
  </>;
}

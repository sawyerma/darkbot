import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { selectLive, selectStatus, selectWallets, useBscStore } from "../store/bscStore";
import { capital, lastSignalMs, openPositions, pendingTransactions, pnlLowerBound, tradingState } from "../model/overview";
import { runtimeView } from "../model/sources";
import { strategySettingsView } from "../model/strategySettings";
import type { BscView } from "../BscPage";
import { useNativeRecords } from "./useNativeRecords";
import { useNowMs } from "./useNowMs";
import { Badge, Icon, PanelHead, SystemMetric, TopHeading, bnb, signTone, signedBnbText } from "./V8";

/** V8 Overview (FE-V8-03): one compact system status, the pending notice and the two jumps. */
export function OverviewPanel({ goto }: { goto: (v: BscView) => void }) {
  useTopic(TopicId.BSC_WALLETS_STATE);
  const { projection: live, status: liveTopic } = useBscStore(selectLive);
  const { projection: wallets } = useBscStore(selectWallets);
  const { projection: status } = useBscStore(selectStatus);
  const records = useNativeRecords();
  const nowMs = useNowMs();
  const trading = tradingState(live, liveTopic);
  const lowerBound = pnlLowerBound(live);
  const signal = lastSignalMs(records, nowMs);
  const cap = capital(wallets);
  const pending = pendingTransactions(live);
  const open = openPositions(live);
  const strategy = strategySettingsView(records, status?.warnings ?? [], nowMs);
  const runtime = runtimeView(records, nowMs).view;
  return <>
    <TopHeading title="Overview" subtitle="System status and quick access." />
    <div className="card" data-testid="bsc-overview-status">
      <div className="pad" style={{ paddingBottom: 4 }}><PanelHead title="System status" extra={<Badge label={trading.label} tone={trading.tone} testId="bsc-trading-state" />} /></div>
      <div className="system-strip" aria-label="System status">
        <SystemMetric label="RPC latency · p95" value="—" unit="ms" testId="bsc-metric-rpc" />
        <SystemMetric label="Decision latency · p95" value="—" unit="ms" testId="bsc-metric-decision" />
        <SystemMetric label="Last market signal" value={signal === undefined ? "—" : signal < 1000 ? `${signal}` : `${Math.round(signal / 1000)}`} unit={signal === undefined ? "s" : signal < 1000 ? "ms" : "s"} testId="bsc-metric-signal" />
        <SystemMetric label={lowerBound ? "Realized net PnL · lower bound" : "Realized net PnL"} value={live ? `${lowerBound ? "≥ " : ""}${signedBnbText(live.realizedPnlWei)}` : "—"} accent={signTone(live?.realizedPnlWei)} testId="bsc-metric-pnl" title={live ? `${live.realizedPnlWei} wei${lowerBound ? " (a SELL without the Portal trade event counts its proceeds as 0)" : ""}` : undefined} />
        <SystemMetric label="Total balance" value={cap.totalWei === undefined ? "—" : bnb(cap.totalWei, 2)} unit="BNB" testId="bsc-metric-balance" title={cap.totalWei ? `${cap.totalWei} wei` : undefined} />
      </div>
      <div className="tiny" style={{ padding: "0 24px 17px" }}>{runtime.state === "RUNNING" ? `Runtime ${runtime.mode}` : runtime.state === "STALE" ? "Runtime status stale" : "Runtime not reported"} · {cap.detail} · unreported latencies remain blank.</div>
    </div>
    {pending ? <div className="notice mt" style={{ alignItems: "center", justifyContent: "space-between", gap: 15 }} data-testid="bsc-pending-notice">
      <div className="start"><Icon name="info" size="s" /><span><strong>{pending} pending {pending === 1 ? "transaction" : "transactions"}</strong><span className="small" style={{ display: "block" }}>Review in trade history</span></span></div>
      <button type="button" className="link" onClick={() => goto("live")}>View <Icon name="chevron" size="s" /></button>
    </div> : null}
    <div className="grid cols2 mtl">
      <div className="card pad dashboard-jump"><div><h2>Live trading</h2><p>{trading.label === "Trading ON" ? "Trading running" : trading.label === "Trading OFF" ? "Currently off" : "State unknown"} · {live ? `${open} open ${open === 1 ? "position" : "positions"}` : "open positions —"}</p></div>
        <button type="button" className="btn quiet" onClick={() => goto("live")}>Open Live <Icon name="chevron" size="s" /></button></div>
      <div className="card pad dashboard-jump"><div><h2>Training</h2><p>Strategy C {strategy.state === "UNKNOWN" ? "state unknown" : strategy.state.toLowerCase()} · training modules not implemented</p></div>
        <button type="button" className="btn quiet" onClick={() => goto("training")}>Open Training <Icon name="chevron" size="s" /></button></div>
    </div>
  </>;
}

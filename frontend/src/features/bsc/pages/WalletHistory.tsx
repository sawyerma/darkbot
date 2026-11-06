import { useEffect, useState } from "react";
import { Availability, DataQuality, PerformanceMetric, PerformanceScope, WalletRole, type PerformanceSeries, type WalletMovement } from "../api/bscV8Adapter";
import { ACCOUNT_NAME, MOVEMENT_FILTERS, accountOf, movementLabel, seriesScopeId, type MovementFilter, type TransferRow } from "../model/wallets";
import { MAX_DATE_MS } from "../model/live";
import { queryReason, useBscQuery } from "./useBscQuery";
import { Badge, PanelHead, Wei, bnb } from "./V8";
import { chainLabel, shortAddress } from "./WalletParts";

const DAY_MS = 86_400_000;
type Period = "7T" | "30T" | "All";
const PERIODS: ReadonlyArray<[Period, string]> = [["7T", "7 days"], ["30T", "30 days"], ["All", "All time"]];
type AccountFilter = "all" | "main" | "buyer" | "launcher";
const ROLE_FILTER: Record<AccountFilter, WalletRole[]> = { all: [], main: [WalletRole.MAIN], buyer: [WalletRole.BUYER], launcher: [WalletRole.LAUNCHER] };
/** UTC minute of a unix-ms time; a value outside the JS date range is shown raw, never thrown. */
const time = (ms: bigint | undefined) => ms === undefined ? "—" : ms > MAX_DATE_MS ? `unix ms ${ms}` : new Date(Number(ms)).toISOString().replace("T", " ").slice(0, 16);

/** 6 · Transaction history: the owner's movements (cursor pages) and the transfers in flight. */
export function WalletMovements({ rows, epoch }: { rows: TransferRow[]; epoch: string }) {
  const [kind, setKind] = useState<MovementFilter>("all");
  const [account, setAccount] = useState<AccountFilter>("all");
  const [pages, setPages] = useState<string[]>([""]);
  const kinds = MOVEMENT_FILTERS.find(([k]) => k === kind)?.[2] ?? [];
  const cursor = pages[pages.length - 1] ?? "";
  const q = useBscQuery(`movements:${epoch}:${kind}:${account}:${cursor}`, { case: "walletMovements", value: { kinds, roles: ROLE_FILTER[account], cursor } });
  const page = q.kind === "ANSWER" && q.result.case === "walletMovements" ? q.result.value : undefined;
  const reset = (f: () => void) => { f(); setPages([""]); };
  // A new treasury step (refresh, decided request) starts again at the newest page: older cursors belong to the old snapshot.
  useEffect(() => { setPages([""]); }, [epoch]);
  return <section className="card mtl" data-testid="bsc-wallet-movements-card" aria-label="Transaction history">
    <div className="topline" style={{ padding: "20px 22px 0", flexWrap: "wrap", gap: 8 }}><h2>Transaction history</h2>
      <div className="wallet-tool-row" style={{ marginTop: 0 }}>
        <select className="select" aria-label="Filter by kind" value={kind} onChange={e => reset(() => setKind(e.target.value as MovementFilter))} data-testid="bsc-wallet-movement-kind">
          {MOVEMENT_FILTERS.map(([k, label]) => <option key={k} value={k}>{label}</option>)}</select>
        <select className="select" aria-label="Filter by account" value={account} onChange={e => reset(() => setAccount(e.target.value as AccountFilter))} data-testid="bsc-wallet-movement-account">
          <option value="all">All accounts</option><option value="main">{ACCOUNT_NAME.main}</option><option value="buyer">{ACCOUNT_NAME.buyer}</option><option value="launcher">{ACCOUNT_NAME.launcher}</option></select>
      </div></div>
    <p className="small" style={{ margin: 0, padding: "8px 22px 12px" }}>{page ? `Deposited: ${bnb(page.depositedWei, 3)} BNB · Paid out: ${bnb(page.paidOutWei, 3)} BNB · internal transfers counted once per account` : "Deposited: — · Paid out: —"}</p>
    {rows.length ? <div className="table-wrap" data-testid="bsc-wallet-transfers"><table><thead><tr><th>Transfer</th><th>Status</th><th className="right">Amount</th><th>Transaction</th><th>Updated</th></tr></thead>
      <tbody>{rows.map((r, i) => <tr key={`${r.t.commandId || r.t.requestId || r.t.txHash}:${i}`} data-status={r.label}><td>{r.path}{r.t.policyVersion !== undefined ? <span className="tiny" style={{ display: "block" }}>policy v{r.t.policyVersion.toString()}</span> : null}</td>
        <td><Badge label={r.label} tone={r.tone} />{r.duplicateTrigger ? <Badge label="Two requests on one trigger" tone="bad" /> : null}{r.t.failureReason ? <span className="tiny" style={{ display: "block" }}>{r.t.failureReason}</span> : null}</td>
        <td className="right mono"><Wei wei={r.t.amountWei} places={6} /></td><td className="wallet-address">{r.t.txHash ? shortAddress(r.t.txHash) : "not signed"}<span className="tiny" style={{ display: "block" }}>intent {r.t.commandId || "—"}</span></td><td>{time(r.t.updatedUnixMs)}</td></tr>)}</tbody></table></div> : null}
    <div className="table-wrap"><table><thead><tr><th>Activity</th><th>Date</th><th>Account</th><th>Counterparty</th><th className="right">Amount</th><th className="right">Fee</th><th>Status</th><th>Transaction</th></tr></thead>
      <tbody>{page?.movements.length ? page.movements.map(m => <MovementRow key={m.movementId} m={m} />)
        : <tr><td colSpan={8}><div className="empty" data-testid="bsc-wallet-movements">{q.kind === "ANSWER" ? "No matching transactions." : queryReason(q, "the native wallet ledger (BSC-V8-13/14)")}</div></td></tr>}</tbody></table></div>
    <div className="wallet-tool-row" style={{ padding: "0 22px 18px" }}>
      {pages.length > 1 ? <button type="button" className="btn sm quiet" onClick={() => setPages(pages.slice(0, -1))}>Newer</button> : null}
      <button type="button" className="btn sm quiet" disabled={!page?.hasMore || !page.nextCursor} onClick={() => page && setPages([...pages, page.nextCursor])} data-testid="bsc-wallet-movements-more">Older</button>
    </div>
  </section>;
}

function MovementRow({ m }: { m: WalletMovement }) {
  const who = accountOf(m.role);
  const other = accountOf(m.counterpartyRole);
  const negative = m.amountWei.startsWith("-");
  return <tr data-kind={m.kind}>
    <td>{movementLabel(m.kind)}{m.lot ? <span className="tiny" style={{ display: "block" }}>lot {shortAddress(m.lot)}</span> : null}</td>
    <td>{m.blockTime?.unixMs !== undefined ? time(m.blockTime.unixMs) : "—"}</td>
    <td>{who ? ACCOUNT_NAME[who] : "—"}</td>
    <td className="wallet-address">{other ? ACCOUNT_NAME[other] : ""}{m.counterpartyAddress ? ` ${shortAddress(m.counterpartyAddress)}` : ""}</td>
    <td className={`right mono ${negative ? "down" : "up"}`}><Wei wei={m.amountWei} places={6} signed /></td>
    <td className="right mono"><Wei wei={m.feeWei} places={6} /></td>
    <td>{chainLabel(m.status)}{m.quality !== DataQuality.VERIFIED ? <span className="tiny" style={{ display: "block" }}>{DataQuality[m.quality]}</span> : null}</td>
    <td className="wallet-address">{m.txHash ? shortAddress(m.txHash) : "—"}{m.blockNumber !== undefined ? <span className="tiny" style={{ display: "block" }}>block {m.blockNumber.toString()}</span> : null}</td>
  </tr>;
}

/** The owner's series as a line (display only: the exact values are in the history). */
export function SeriesChart({ series, label }: { series: PerformanceSeries; label: string }) {
  const pts = series.points.filter(p => p.at?.unixMs !== undefined && /^-?[0-9]+$/.test(p.valueWei)).map(p => ({ x: Number(p.at?.unixMs), y: Number(BigInt(p.valueWei)) / 1e18, wei: p.valueWei }));
  if (pts.length < 2) return <div className="empty">{pts.length ? "One verified point: no line yet." : "No verified points in this period."}</div>;
  const w = 650, h = 225, pad = 25;
  const xs = pts.map(p => p.x), ys = pts.map(p => p.y);
  const x0 = Math.min(...xs), x1 = Math.max(...xs) || x0 + 1, lo = Math.min(0, ...ys), hi = Math.max(...ys, lo + 1e-18);
  const X = (x: number) => pad + (w - 2 * pad) * (x - x0) / ((x1 - x0) || 1);
  const Y = (y: number) => h - 30 - (y - lo) / ((hi - lo) || 1) * (h - 58);
  const path = pts.map((p, i) => `${i ? "L" : "M"}${X(p.x).toFixed(2)} ${Y(p.y).toFixed(2)}`).join(" ");
  const last = pts[pts.length - 1];
  return <svg className="chart" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" role="img" aria-label={`${label}, ${pts.length} verified points`}>
    {[0, 0.5, 1].map(f => { const y = 23 + f * (h - 53); return <g key={f}><line x1={pad} y1={y} x2={w - pad} y2={y} /><text x={pad - 1} y={y - 7}>{(hi - (hi - lo) * f).toFixed(3)}</text></g>; })}
    <path d={path} className="curve" />{last ? <circle cx={X(last.x)} cy={Y(last.y)} r={5} className="point"><title>{bnb(last.wei, 6)} BNB</title></circle> : null}
    <text x={pad} y={h - 5}>{time(BigInt(x0))}</text><text x={w - pad} y={h - 5} textAnchor="end">{time(BigInt(x1))}</text>
  </svg>;
}

/** 7 · Balance and profit charts, per account (never summed). */
export function WalletCharts({ epoch }: { epoch: string }) {
  const [period, setPeriod] = useState<Period>("7T");
  const [chart, setChart] = useState<"balance" | "profit">("balance");
  const [account, setAccount] = useState<"main" | "buyer">("buyer");
  const target = chart === "profit" ? "buyer" : account;
  const from = period === "All" ? undefined : BigInt(Date.now() - (period === "7T" ? 7 : 30) * DAY_MS);
  const q = useBscQuery(`wallet-series:${epoch}:${chart}:${period}:${target}`, { case: "series", value: {
    metric: chart === "balance" ? PerformanceMetric.WALLET_BALANCE : PerformanceMetric.WALLET_TRADING_PNL,
    scope: PerformanceScope.WALLET, scopeId: seriesScopeId(target), fromUnixMs: from } });
  const series = q.kind === "ANSWER" && q.result.case === "series" ? q.result.value : undefined;
  const label = chart === "balance" ? `${ACCOUNT_NAME[target]} balance` : "Trading account realized net PnL";
  return <section className="card pad mtl" aria-label="Balance and profit charts">
    <PanelHead title="Balance & profit" extra={<div className="segment" role="group" aria-label="Time range">{PERIODS.map(([v, t]) => <button key={v} type="button" className={period === v ? "active" : undefined} aria-pressed={period === v} onClick={() => setPeriod(v)}>{t}</button>)}</div>} />
    <div className="wallet-tool-row" style={{ marginTop: 0, marginBottom: 13 }}>
      <div className="segment" role="group" aria-label="Chart type">
        <button type="button" className={chart === "balance" ? "active" : undefined} aria-pressed={chart === "balance"} onClick={() => setChart("balance")}>Balance</button>
        <button type="button" className={chart === "profit" ? "active" : undefined} aria-pressed={chart === "profit"} onClick={() => setChart("profit")}>Profit &amp; loss</button>
      </div>
      {chart === "balance" ? <div className="segment" role="group" aria-label="Account">
        <button type="button" className={account === "main" ? "active" : undefined} aria-pressed={account === "main"} onClick={() => setAccount("main")}>Main</button>
        <button type="button" className={account === "buyer" ? "active" : undefined} aria-pressed={account === "buyer"} onClick={() => setAccount("buyer")}>Trading</button>
      </div> : null}
    </div>
    <div data-testid="bsc-wallet-history">{series && series.availability === Availability.AVAILABLE ? <SeriesChart series={series} label={label} /> : <div className="empty">{q.kind === "ANSWER" ? series?.unavailableReason || "No series." : queryReason(q, "the native wallet ledger (BSC-V8-13/14)")}</div>}</div>
    <div className="chart-foot"><span>{chart === "balance" ? `${label} · including transfers` : "Realized net trading PnL · excluding transfers"}</span><span>Native wallet ledger only</span></div>
  </section>;
}

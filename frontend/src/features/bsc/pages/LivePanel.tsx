import "@/shared/ui/graphite/control-dialog.css";
import { useMemo, useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { strategySettingCommand, useSourceControl } from "@/core/realtime/bscControl";
import { selectLive, selectStatus, useBscStore } from "../store/bscStore";
import { tradingState } from "../model/overview";
import { sourceControlView } from "../model/sources";
import { strategySettingsView } from "../model/strategySettings";
import { changes, formOf, runningNotes, type StrategyForm } from "../model/strategyForm";
import { LABEL_TONE, RIGHT_ALIGNED, TRADE_COLUMNS, TRADE_COL_DEFAULT, blockClock, counts, filterTrades, nsToMs, safeColumns, tradeViews, tradesCsv, type TradeColumn, type TradeFilter, type TradeView } from "../model/trades";
import { detection, nsMs3, share, sourceLines, usToMs, type Market } from "../model/monitor";
import { SOURCE_STATUS, age, sourceName, venueName } from "../model/health";
import { LiveLotActions, Receipt, LIVE_LOT } from "../LiveActions";
import { operatorDecisions } from "../model/live";
import { SourceActions } from "../SourceActions";
import { TradeSort } from "../api/bscV8Adapter";
import { LiveSections } from "./LiveSections";
import { StrategyEditor } from "./StrategyEditor";
import { useNativeRecords } from "./useNativeRecords";
import { useNowMs } from "./useNowMs";
import { queryReason, useBscQuery } from "./useBscQuery";
import { Badge, DetailLine, Drawer, Icon, PanelHead, StateBadge, TopHeading, bnb, signTone, signedBnbText } from "./V8";

const STRATEGY = "bsc-live-strategy";
const COLUMNS_KEY = "bsc_trade_columns_v8";
const PAGE = 6;
const NO_PROFILES = "Training modules need their native profile owner (BSC-V8-01); Live uses the native Strategy C configuration.";
const ONE_AT_A_TIME = "Saving several settings at once needs the atomic set of the native owner (BSC-V8-04): save one change at a time.";

function loadColumns(): TradeColumn[] {
  try { const raw = localStorage.getItem(COLUMNS_KEY); return raw ? safeColumns(JSON.parse(raw)) : [...TRADE_COL_DEFAULT]; } catch { return [...TRADE_COL_DEFAULT]; }
}
function saveColumns(c: readonly TradeColumn[]) { try { localStorage.setItem(COLUMNS_KEY, JSON.stringify(c)); } catch { /* a UI preference only */ } }

function cell(t: TradeView, id: TradeColumn) {
  const a = t.attempt;
  const clock = blockClock(t.buyMs);
  switch (id) {
    case "token": return <><strong className="mono">{t.token.slice(0, 8)}…{t.token.slice(-4)}</strong><span className="tdmuted">{t.component === "BSC_STRATEGY_C" ? "Strategy C" : t.component === "BSC_EXEC" ? "C3" : t.component}</span></>;
    case "status": return <Badge label={t.label} tone={LABEL_TONE[t.label]} />;
    case "market": return "—";
    case "invest": return `${bnb(t.inputWei)} BNB`;
    case "pnl": return t.pnlWei === undefined ? "—" : `${t.proceedsUnknown ? "≥ " : ""}${signedBnbText(t.pnlWei)}`;
    case "roi": return "—";
    case "buy": return clock ? <>{clock.time}<span className="tdmuted">{clock.date}</span></> : "—";
    case "sell": return "—";
    case "hold": return "—";
    case "source": return a?.trigger ? sourceName(a.trigger) : "—";
    case "lead": return "—";
    case "send": return nsToMs(a?.ingressToSendNs);
    case "ack": return nsToMs(a?.sendToAckNs);
    case "gas": return `${bnb(t.gasWei)} BNB`;
    case "placement": return a?.placed || "—";
    case "exit": return "—";
    case "tx": return <span className="mono">{t.lot.slice(0, 10)}…</span>;
  }
}

/** V8 Live (FE-V8-09…12): the LIVE strip with its controls and limits, the strategy (the native
 *  Strategy C configuration, one change per request), the one trade list and the monitoring.
 *  Every control is the native owner's request (bsc.live.*, bsc.strategy.set, bsc.source.mode.set). */
export function LivePanel() {
  useTopic(TopicId.BSC_LIVE_STATE);
  useTopic(TopicId.BSC_MONITORING_STATE);
  const { projection: live } = useBscStore(selectLive);
  const { projection: status } = useBscStore(selectStatus);
  const records = useNativeRecords();
  const nowMs = useNowMs();
  const { busy } = useSourceControl();
  const [layer, setLayer] = useState<"controls" | "limits" | "columns" | "ledger" | { trade: string } | null>(null);
  const [mode, setMode] = useState<"module" | "manual">("manual");
  const trading = tradingState(live);
  const settings = useMemo(() => strategySettingsView(records, status?.warnings ?? [], nowMs), [records, status?.warnings, nowMs]);
  const native = useMemo(() => formOf(settings), [settings]);
  // The operator's draft over the native values: shown as the owner holds them until edited.
  const [draft, setDraft] = useState<StrategyForm | null>(null);
  const form = draft ?? native.form;
  const edited = draft !== null;
  const diff = changes(settings, form);
  const one = "changed" in diff && diff.changed.length === 1 ? diff.changed[0] : undefined;
  const [confirm, setConfirm] = useState<{ key: string; value: string; generation: bigint } | null>(null);
  const control = sourceControlView(records, nowMs);
  const venues = control.venues.filter(v => v.effective !== "OFF").map(v => venueName(v.id)).join(", ") || "No venue";

  const [columns, setColumns] = useState<TradeColumn[]>(loadColumns);
  const [colDraft, setColDraft] = useState<TradeColumn[]>(columns);
  const [filter, setFilter] = useState<TradeFilter>({ search: "", status: "all" });
  const [page, setPage] = useState(1);
  const [wantHistory, setWantHistory] = useState(0);
  const all = useMemo(() => tradeViews(live), [live]);
  const shownRows = filterTrades(all, filter);
  const visible = shownRows.slice(0, page * PAGE);
  const c = counts(shownRows);
  // The history is asked for on the operator's request only (one query per step, never per keystroke).
  const history = useBscQuery(`trades:${wantHistory}`, wantHistory ? { case: "trades", value: { search: filter.search, sort: TradeSort.ACTIVE_FIRST, limit: PAGE } } : null);
  const trade = layer && typeof layer === "object" ? all.find(t => t.lot === layer.trade) : undefined;
  const decisions = operatorDecisions(live);

  const [market, setMarket] = useState<Market>("flap");
  const d = detection(records, market);
  const sources = sourceLines(records, nowMs);
  const modeOf = (id: string) => control.rows.find(r => r.id === id);

  const exportCsv = () => {
    const blob = new Blob(["﻿" + tradesCsv(shownRows)], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob), a = document.createElement("a");
    a.href = url; a.download = "bsc-trades.csv"; a.click(); setTimeout(() => URL.revokeObjectURL(url), 500);
  };

  return <>
    <TopHeading title="Live trading" subtitle="Control trading, review transactions and monitor sources." />
    <section className="card live-strip" data-testid="bsc-live-strip">
      <div className="start"><span className={`symbol ${trading.label === "Trading ON" ? "" : "warn"}`}><Icon name="live" /></span>
        <div><strong>{trading.label}</strong><small>{trading.detail}</small></div></div>
      <div className="actions"><button type="button" className="btn sm quiet" onClick={() => setLayer("limits")} data-testid="bsc-live-limits-open">Limits</button>
        <button type="button" className={trading.label === "Trading ON" ? "btn quiet" : "btn primary"} onClick={() => setLayer("controls")} data-testid="bsc-live-controls-open">{trading.label === "Trading ON" ? "Controls" : "Start Live"}</button></div>
    </section>

    <div className="card editor-card mtl" data-testid="bsc-live-strategy">
      <div className="editor-heading"><div><h2>Trading strategy</h2><p className="small">Use a training module or enter custom settings.</p></div><StateBadge state={settings.state} label={`Strategy C · ${settings.state}`} /></div>
      <div className="live-mode-switch">
        <button type="button" className={`btn ${mode === "module" ? "primary" : "quiet"}`} disabled title={NO_PROFILES}>Use training module</button>
        <button type="button" className={`btn ${mode === "manual" ? "primary" : "quiet"}`} onClick={() => setMode("manual")}>Custom settings</button>
      </div>
      <StrategyEditor form={form} onChange={setDraft} disabled={!settings.available || busy} venues={venues} firstBuyerWindow="—" idPrefix={STRATEGY} notes={runningNotes(settings)} />
      {native.filterError ? <p className="tiny v8-unavailable">Stored filters unreadable here: {native.filterError}</p> : null}
      <div className="editor-footer">
        <span className="small" data-testid={`${STRATEGY}-generation`}>{!settings.available ? `Native owner ${settings.reason}` : `Saved generation ${settings.generation.toString()}${settings.running ? ` · ${settings.startedGeneration?.toString() ?? "?"} in effect` : " · applies at the next start"}`}
          {"error" in diff ? ` · ${diff.error}` : "changed" in diff && diff.changed.length > 1 ? ` · ${ONE_AT_A_TIME}` : ""}</span>
        <div className="editor-actions">
          <button type="button" className="btn quiet" disabled={!edited} onClick={() => setDraft(null)}>Reset</button>
          <button type="button" className="btn primary" disabled={!settings.available || busy || !one} title={one ? `${one.key} at generation ${settings.generation.toString()}` : "changed" in diff && diff.changed.length > 1 ? ONE_AT_A_TIME : "No change"}
            data-testid={`${STRATEGY}-save`} onClick={() => { if (one) setConfirm({ ...one, generation: settings.generation }); }}>Save custom settings</button>
        </div>
      </div>
      <Receipt testId={STRATEGY} />
    </div>

    <section id="live-history" className="card pad mtl" data-testid="bsc-live-trades">
      <PanelHead title="Trades" extra={<div className="actions">
        <button type="button" className="btn sm quiet" onClick={() => { setColDraft(columns); setLayer("columns"); }} data-testid="bsc-trade-columns"><Icon name="settings" size="s" /> Columns</button>
        <button type="button" className="btn sm quiet" onClick={() => setLayer("ledger")}>Receipts</button>
        <button type="button" className="btn sm quiet" disabled={!shownRows.length} onClick={exportCsv}><Icon name="download" size="s" /> Export</button></div>} />
      <div className="toolbar history-toolbar" style={{ padding: 0 }}>
        <label className="start grow" style={{ maxWidth: 320 }}><Icon name="search" size="s" /><input className="input search" value={filter.search} onChange={e => { setFilter({ ...filter, search: e.target.value }); setPage(1); }} placeholder="Find token or transaction" aria-label="Find trades" /></label>
        <select className="select" aria-label="Filter by status" value={filter.status} onChange={e => { setFilter({ ...filter, status: e.target.value as TradeFilter["status"] }); setPage(1); }}>
          <option value="all">All</option><option value="open">RUN</option><option value="unknown">PEND</option><option value="win">WIN</option><option value="loss">LOSS</option><option value="closed">DONE</option></select>
        <select className="select" aria-label="Filter by market" disabled title="Per-lot venue needs the native trade history (BSC-V8-07/10)"><option>All markets</option></select>
        <select className="select" aria-label="Filter by time" disabled title="Per-lot times need the native trade history (BSC-V8-07/10)"><option>All dates</option></select>
      </div>
      <div className="trade-summary"><span><i className="trade-dot open" />{c.run} RUN</span><span><i className="trade-dot unknown" />{c.pend} PEND</span><span><i className="trade-dot closed" />{c.win} WIN</span><span><i className="trade-dot loss" />{c.loss} LOSS</span><span className="tiny">Active first · current LIVE ledger</span></div>
      <div className="table-wrap trade-flow-wrap"><table className="trade-flow-table"><thead><tr>{columns.map(id => <th key={id} className={RIGHT_ALIGNED.has(id) ? "right" : undefined}>{TRADE_COLUMNS.find(x => x.id === id)?.short ?? id}</th>)}<th /></tr></thead>
        <tbody>{visible.length ? visible.map(t => <tr key={t.lot} data-open tabIndex={0} role="button" data-testid="bsc-trade-row" data-lot={t.lot} aria-label={`Trade ${t.token} ${t.label}`} onClick={() => setLayer({ trade: t.lot })} onKeyDown={e => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); setLayer({ trade: t.lot }); } }}>
          {columns.map(id => <td key={id} className={`trade-col-${id}${RIGHT_ALIGNED.has(id) ? " right nowrap" : ""}${id === "pnl" ? ` ${signTone(t.pnlWei)}` : ""}`} data-label={TRADE_COLUMNS.find(x => x.id === id)?.label}>{cell(t, id)}</td>)}
          <td className="trade-row-action"><Icon name="chevron" size="s" /></td></tr>)
          : <tr><td colSpan={columns.length + 1}><div className="empty">{live ? "No matching trades." : "No LIVE owner state."}</div></td></tr>}</tbody></table></div>
      <p className="tiny trade-footnote">*Lead is relative to the block header timestamp, not an exact block production time; per-trade venue, sell time, hold, lead, ROI and exit reason come with the native trade history. Click a trade for its transactions and decisions.</p>
      <div className="table-footer"><span>{visible.length} of {shownRows.length} trades · full history: {history.kind === "ANSWER" ? "available" : queryReason(history, "the native trade history (BSC-V8-07/10)")}</span>
        {visible.length < shownRows.length ? <button type="button" className="btn sm quiet" onClick={() => setPage(page + 1)}>Show more</button>
          : <button type="button" className="btn sm quiet" onClick={() => setWantHistory(wantHistory + 1)} data-testid="bsc-trade-history-more">Load history</button>}</div>
    </section>

    <section id="live-monitoring" className="card pad mtl" data-testid="bsc-live-monitoring">
      <div className="row monitor-head" style={{ justifyContent: "space-between", alignItems: "center", gap: 12, flexWrap: "wrap" }}>
        <div><h2>Market monitoring</h2><p className="small" style={{ margin: "3px 0 0" }}>Launch detection and WSS source comparison.</p></div>
        <label className="small monitor-choice">Market <select className="select" value={market} onChange={e => setMarket(e.target.value as Market)}><option value="flap">Flap</option><option value="four">Four.meme</option></select></label>
      </div>
      <div className="detection-strip" aria-label="Early detection statistics">
        <div className="detect-main"><span className="small">Verified before block</span><strong className="mono">{share(d.full, d.total)} <small>({d.full ?? "—"}/{d.total ?? "—"})</small></strong></div>
        <div><span>Hash only</span><strong>{d.hash ?? "—"}</strong></div><div><span>Late</span><strong>{d.after ?? "—"}</strong></div><div><span>Missed</span><strong>{d.missed ?? "—"}</strong></div>
        <div className="detect-leads"><span>Lead before block header{d.clockSuspect ? " · clock suspect" : ""}</span><strong className="mono">p50 {usToMs(d.p50Us)} <i>·</i> p95 {usToMs(d.p95Us)} <i>·</i> p99 {usToMs(d.p99Us)}</strong></div>
      </div>
      <div className="source-table-head"><h3>Signal sources and reception</h3><span className="small">{sources.filter(s => s.health === "HEALTHY").length}/{sources.length || control.rows.length} connected · First = first verified full sighting</span></div>
      <div className="table-wrap source-table-wrap"><table className="source-compact-table"><thead><tr><th>Source</th><th>State</th><th className="right">First</th><th className="right" title="Lag behind the first verified source">Behind first · p50</th><th className="right">Queue mean</th><th className="right">Dropped</th><th className="right">Last signal</th><th className="right">Mode</th></tr></thead>
        <tbody>{(sources.length ? sources : control.rows.map(r => ({ id: r.id, health: "UNKNOWN", fresh: false }) as const)).map(s => {
          const row = modeOf(s.id);
          const cls = s.health === "HEALTHY" ? "" : s.health === "OFF" ? "off" : "down";
          const lag = "lagP50Us" in s && s.lagP50Us !== undefined && !/^0+$/.test(s.lagP50Us) ? usToMs(s.lagP50Us) : "—";
          return <tr key={s.id} data-testid={`bsc-monitor-source-${s.id}`}><td><strong>{sourceName(s.id)}</strong></td><td><span className={`source-pill ${cls}`} title={s.health}><i className="source-led" />{s.health === "DISCONNECTED" ? "Down" : SOURCE_STATUS[s.health] ?? s.health}</span></td>
            <td className="right mono">{"first" in s ? s.first ?? "—" : "—"}</td><td className="right mono" title="Lag behind the first verified source; — for the first or without a later sighting">{lag}</td><td className="right mono">{"queueMeanNs" in s ? nsMs3(s.queueMeanNs) : "—"}</td>
            <td className="right mono">{"dropped" in s ? s.dropped ?? "—" : "—"}</td><td className="right mono">{"lastAgeMs" in s && s.lastAgeMs !== undefined ? age(s.lastAgeMs) : "—"}</td>
            <td className="right v8-mode-cell">{row ? <><span className="tiny" title={row.blockReason || undefined}>{row.effective}</span><SourceActions view={control} rows={[row]} receipt={false} /></> : "—"}</td></tr>;
        })}</tbody></table></div>
      <Receipt testId="bsc-source" />
      <p className="tiny monitor-footnote">{d.detail}. Source-to-source lag and pre-block lead are different measurements; the native census and runtime status provide every value.</p>
    </section>

    {layer === "controls" ? <Drawer title="Live controls" kicker="LIVE TRADING" wide onClose={() => setLayer(null)} testId="bsc-live-controls"
      footer={<button type="button" className="btn primary" onClick={() => setLayer(null)}>Close</button>}>
      <p className="small">The native owner's LIVE gate, the grant of one confirmed scope, its activation and the runtime's LIVE path. A grant sends nothing; every LIVE intent is admitted again at its dispatch.</p>
      <LiveSections only={[1, 2, 3, 6]} />
    </Drawer> : null}
    {layer === "limits" ? <Drawer title="Trading limits" kicker="LIVE" wide onClose={() => setLayer(null)} testId="bsc-live-limits-drawer"
      footer={<button type="button" className="btn primary" onClick={() => setLayer(null)}>Close</button>}>
      <p className="small">BUY attempt limits, spend and budget of the native owner; the grant's own limits and risk are set with the grant (Live controls).</p>
      <LiveSections only={[4, 5]} />
    </Drawer> : null}
    {layer === "ledger" ? <Drawer title="Receipts & ledger" kicker="LIVE LEDGER" wide onClose={() => setLayer(null)}
      footer={<button type="button" className="btn primary" onClick={() => setLayer(null)}>Close</button>}>
      <LiveSections only={[7, 8, 9]} positionsTable={false} />
    </Drawer> : null}
    {layer === "columns" ? <Drawer title="Trade columns" kicker="PERSONALIZE" wide onClose={() => setLayer(null)} testId="bsc-trade-columns-drawer"
      footer={<><button type="button" className="btn quiet" onClick={() => setColDraft([...TRADE_COL_DEFAULT])}>Defaults</button><button type="button" className="btn quiet" onClick={() => setLayer(null)}>Cancel</button>
        <button type="button" className="btn primary" data-testid="bsc-trade-columns-apply" onClick={() => { const next = safeColumns(colDraft); setColumns(next); saveColumns(next); setLayer(null); }}>Apply</button></>}>
      <p className="small">Choose which fields appear in the trade list. Order with the arrows; saved in this browser only (a display preference, never a trading parameter).</p>
      <div className="column-choices">{[...colDraft.map(id => TRADE_COLUMNS.find(c => c.id === id)!), ...TRADE_COLUMNS.filter(c => !colDraft.includes(c.id))].map(col => {
        const on = colDraft.includes(col.id), at = colDraft.indexOf(col.id);
        const move = (by: number) => { const n = [...colDraft]; const b = at + by; if (b < 0 || b >= n.length) return; [n[at], n[b]] = [n[b]!, n[at]!]; setColDraft(n); };
        return <div className="column-choice" key={col.id}><label className="check-label"><input type="checkbox" checked={on} disabled={col.id === "token"} data-testid={`bsc-col-${col.id}`}
          onChange={e => setColDraft(e.target.checked ? [...colDraft, col.id] : colDraft.filter(x => x !== col.id))} /><span>{col.label}</span></label>
          <div className="column-controls"><button type="button" aria-label={`Move ${col.label} up`} disabled={!on || at <= 0} onClick={() => move(-1)}>↑</button><button type="button" aria-label={`Move ${col.label} down`} disabled={!on || at === colDraft.length - 1} onClick={() => move(1)}>↓</button></div></div>;
      })}</div>
      <p className="tiny mt">Token is always visible.</p>
    </Drawer> : null}
    {trade ? <Drawer title={`${trade.token.slice(0, 10)}… · Trade`} kicker="TRADE DETAILS" onClose={() => setLayer(null)} testId="bsc-trade-detail"
      footer={<button type="button" className="btn primary" onClick={() => setLayer(null)}>Close</button>}>
      <div className="row"><Badge label={trade.label} tone={LABEL_TONE[trade.label]} /><span className="small">{trade.state}</span></div>
      <div className="subcard mt"><span className="small">Net PnL</span><div className={`big-figure ${signTone(trade.pnlWei)}`}>{trade.pnlWei === undefined ? "Not available yet" : `${trade.proceedsUnknown ? "≥ " : ""}${signedBnbText(trade.pnlWei)}`}</div>
        <span className="tiny">{trade.proceedsUnknown ? "SELL without the Portal's trade event: proceeds unknown, PnL a lower bound" : "After every recorded transaction cost"}</span></div>
      <hr className="rule" /><h3>Trade</h3>
      <DetailLine label="Token" value={<span className="mono">{trade.token}</span>} />
      <DetailLine label="Component" value={trade.component} />
      <DetailLine label="Buy block time" value={blockClock(trade.buyMs) ? `${blockClock(trade.buyMs)!.date} ${blockClock(trade.buyMs)!.time} UTC (block header)` : "—"} />
      <DetailLine label="Investment" value={`${bnb(trade.inputWei, 6)} BNB`} />
      <DetailLine label="Sell proceeds" value={trade.proceedsWei === undefined ? "—" : trade.proceedsUnknown ? "unknown" : `${bnb(trade.proceedsWei, 6)} BNB`} />
      <DetailLine label="Network fees" value={`${bnb(trade.gasWei, 6)} BNB`} />
      <h3 style={{ marginTop: 18 }}>Detection and execution</h3>
      <DetailLine label="First source" value={trade.attempt?.trigger ? sourceName(trade.attempt.trigger) : "—"} />
      <DetailLine label="Ingress → send" value={nsToMs(trade.attempt?.ingressToSendNs)} />
      <DetailLine label="Send → response" value={nsToMs(trade.attempt?.sendToAckNs)} />
      <DetailLine label="Placement" value={trade.attempt?.placed || "Not established"} />
      <DetailLine label="Timing verdict" value={trade.attempt?.timing || "—"} />
      <h3 style={{ marginTop: 18 }}>Decisions</h3>
      {trade.decisions.length ? <LiveLotActions lot={trade.lot} decisions={trade.decisions} version={decisions.version} exitNow={decisions.exitNow} /> : <p className="small">No operator decision the ledger takes of this lot now.</p>}
      <Receipt testId={LIVE_LOT} />
      <h3 style={{ marginTop: 18 }}>Transactions</h3>
      {trade.transactions.map(t => <div key={t.txHash} className="detail-line"><span className="muted">{t.intent} · nonce {t.nonce.toString()} · {t.state}{t.blockNumber !== undefined ? ` · block ${t.blockNumber.toString()}` : ""}{t.burnable || t.burnRequested ? " · nonce burn under Receipts" : ""}</span>
        <strong className="mono">{t.txHash.slice(0, 18)}…</strong></div>)}
      <DetailLine label="Lot (BUY transaction)" value={<span className="mono">{trade.lot}</span>} />
      <button type="button" className="link" onClick={() => { void navigator.clipboard.writeText(trade.lot).catch(() => undefined); }}><Icon name="copy" size="s" /> Copy transaction ID</button>
    </Drawer> : null}

    <AlertDialog.Root open={!!confirm} onOpenChange={o => { if (!o) setConfirm(null); }}>
      <AlertDialog.Portal><AlertDialog.Overlay className="ov-log-overlay" /><AlertDialog.Content className="ov-confirm-view" data-testid={`${STRATEGY}-dialog`}>
        <AlertDialog.Title>{confirm?.key} auf „{confirm?.value}“ setzen?</AlertDialog.Title>
        <AlertDialog.Description asChild><div><p>Die native Verwaltung prüft den Wert mit der Runtime-Konfiguration und speichert ihn sofort; wirksam wird er beim nächsten Start von Strategy C (Einschalten oder Neustart der Runtime). Echte Positionen behalten die Exit-Werte ihres Einstiegs. Generation {confirm?.generation.toString()}.</p>
          <p><code data-testid={`${STRATEGY}-request`}>{confirm ? `${confirm.key}=${confirm.value}` : ""}</code></p></div></AlertDialog.Description>
        <div className="ov-confirm-actions"><AlertDialog.Cancel asChild><button className="ov-btn">Abbrechen</button></AlertDialog.Cancel>
          <AlertDialog.Action asChild><button className="ov-btn" data-testid={`${STRATEGY}-confirm`} onClick={() => { if (confirm) { void strategySettingCommand(STRATEGY, confirm.key, confirm.value, confirm.generation); setDraft(null); } setConfirm(null); }}>Anfordern</button></AlertDialog.Action></div>
      </AlertDialog.Content></AlertDialog.Portal>
    </AlertDialog.Root>
  </>;
}

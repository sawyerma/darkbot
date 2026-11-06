import { useMemo, useState } from "react";
import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { selectStatus, useBscStore } from "../store/bscStore";
import { RUNTIME_SCHEMA, runtimeView, sourceControlView } from "../model/sources";
import { strategySettingsView } from "../model/strategySettings";
import { formOf } from "../model/strategyForm";
import { venueName } from "../model/health";
import { PerformanceMetric, PerformanceScope } from "../api/bscV8Adapter";
import type { BscView } from "../BscPage";
import { useNativeRecords } from "./useNativeRecords";
import { useNowMs } from "./useNowMs";
import { queryReason, useBscQuery } from "./useBscQuery";
import { StrategyEditor } from "./StrategyEditor";
import { PlacedSection } from "./PlacedSection";
import { Badge, DetailLine, Icon, PanelHead, StateBadge, TopHeading, bnb } from "./V8";

const NO_PROFILE_OWNER = "Training profiles need their native owner (BSC-V8-01): nothing is stored in the browser.";
const NO_RUN_OWNER = "Training runs need their native owner (BSC-V8-02): no test starts from here.";

/** V8 Training (FE-V8-04…08). The native profile and run owner does not exist yet: the module
 *  list shows the one native Strategy C configuration and its running PAPER/SHADOW results (the
 *  runtime status), saved modules and test runs stay visible but disabled with their reason. */
export function TrainingPanel({ goto }: { goto: (v: BscView) => void }) {
  useTopic(TopicId.BSC_MONITORING_STATE);
  const records = useNativeRecords();
  const nowMs = useNowMs();
  const { projection: status } = useBscStore(selectStatus);
  const [open, setOpen] = useState(false);
  const [period, setPeriod] = useState<"7T" | "30T" | "All">("7T");
  const [runMode, setRunMode] = useState<"duration" | "iterations" | "continuous">("duration");
  const profiles = useBscQuery("training_profiles", { case: "trainingProfiles", value: {} });
  const from = period === "All" ? undefined : BigInt(nowMs - (period === "7T" ? 7 : 30) * 86_400_000);
  const series = useBscQuery(`training-series:${period}`, { case: "series", value: { metric: PerformanceMetric.TRAINING_NET_RESULT, scope: PerformanceScope.TRAINING_PROFILE, fromUnixMs: from } });
  const settings = strategySettingsView(records, status?.warnings ?? [], nowMs);
  const { form } = useMemo(() => formOf(settings), [settings]);
  const runtime = runtimeView(records, nowMs).view;
  const venues = sourceControlView(records, nowMs).venues.filter(v => v.effective !== "OFF").map(v => venueName(v.id)).join(", ") || "No venue";
  const rt = new Map(records.find(r => r.schema === RUNTIME_SCHEMA)?.fields.map(x => [x.name, x.value]) ?? []);
  const fresh = runtime.state === "RUNNING";
  const y = (k: string) => fresh ? rt.get(`strategy[0].${k}`) : undefined;
  // Running only in a running mode; SETTINGS_REFUSED / GAS_EVIDENCE_REFUSED did not start it.
  const running = fresh && ["SHADOW", "PAPER", "LIVE"].includes(y("state") ?? "");
  const gains = y("gain_wei"), losses = y("loss_wei");

  if (open) return <>
    <button type="button" className="workspace-back" onClick={() => setOpen(false)}>‹ All modules</button>
    <TopHeading title="Strategy C · runtime configuration" subtitle={`${venues} · Paper trading · no real funds`}
      actions={<><button type="button" className="btn quiet" disabled title={NO_PROFILE_OWNER}>Duplicate</button><button type="button" className="btn quiet" disabled title={NO_PROFILE_OWNER}>Delete</button></>} />
    <div className="card workspace-hero" data-testid="bsc-training-result"><div className="row"><h2>Test result</h2><StateBadge state={fresh ? y("state") ?? "OFF" : runtime.state === "STALE" ? "STALE" : "UNKNOWN"} /></div>
      <div className="workspace-metrics mt">
        <div><small>Gains</small><b className="up">{gains === undefined ? "—" : `+${bnb(gains)} BNB`}</b></div>
        <div><small>Losses</small><b className="down">{losses === undefined ? "—" : `−${bnb(losses)} BNB`}</b></div>
        <div><small>Wins / losses</small><b>{y("gains") ?? "—"} / {y("losses") ?? "—"}</b></div>
        <div><small>Launches checked</small><b>{y("launches") ?? "—"}</b></div>
        <div><small>Rejected</small><b>{y("rejected") ?? "—"}</b></div>
        <div><small>Data incomplete</small><b>{y("data_incomplete") ?? "—"}</b></div>
      </div>
      <p className="tiny mt">{fresh ? `Passed ${y("passed") ?? "?"} · entered ${y("entered") ?? "?"} · exits target ${y("exits_target") ?? "?"}, stop ${y("exits_stop") ?? "?"}, creator sell ${y("exits_creator_sell") ?? "?"}, timeout ${y("exits_timeout") ?? "?"} · open ${y("open") ?? "?"} (not valued) · unresolved ${y("unresolved") ?? "?"}` : "No current runtime status: no result is shown."} The PAPER ledger of the running strategy; not a proven outcome.</p>
    </div>
    <section className="card editor-card mtl"><div className="editor-heading"><div><h2>Module settings</h2><p className="small">The one native Strategy C configuration; edit it in Live → Custom settings.</p></div><Badge label="15 filters available" /></div>
      <StrategyEditor form={form} onChange={() => undefined} disabled venues={venues} firstBuyerWindow="—" idPrefix="bsc-training" />
      <h3 style={{ fontSize: 14, margin: "22px 0 12px" }}>Test run</h3>
      <div className="test-plan"><div className="test-plan-row">
        <div className="field"><label htmlFor="bsc-run-mode">Stop test by</label><select id="bsc-run-mode" className="select" value={runMode} onChange={e => setRunMode(e.target.value as typeof runMode)}>
          <option value="duration">Duration</option><option value="iterations">Number of launches</option><option value="continuous">Until stopped</option></select></div>
        {runMode === "duration" ? <div className="field"><label>Duration · hours</label><input className="input" type="number" disabled /></div> : null}
        {runMode === "iterations" ? <div className="field"><label>Maximum launches processed</label><input className="input" type="number" disabled /></div> : null}
      </div><div className="module-run-meta">{NO_RUN_OWNER}</div></div>
      <div className="v8-section-group" data-testid="bsc-training-native-record"><PlacedSection panel="live" section={10} records={records}>
        <DetailLine label="Settings generation" value={`${settings.generation.toString()} stored${settings.running ? ` · ${settings.startedGeneration?.toString() ?? "?"} in effect` : ""}`} />
      </PlacedSection></div>
      <div className="editor-footer"><span className="small">{running ? "Strategy C runs in the runtime (its switch and the runtime mode, Settings)." : "Strategy C is not running."}</span>
        <div className="editor-actions"><button type="button" className="btn quiet" onClick={() => goto("live")}>Edit in Live</button><button type="button" className="btn primary" disabled title={NO_RUN_OWNER} data-testid="bsc-training-start">Start test</button></div></div>
    </section>
    <section className="card pad mtl"><PanelHead title="Performance over time" extra={<div className="segment" role="group" aria-label="Time range">{([["7T", "7 days"], ["30T", "30 days"], ["All", "All time"]] as const).map(([v, t]) => <button key={v} type="button" className={period === v ? "active" : undefined} onClick={() => setPeriod(v)}>{t}</button>)}</div>} />
      <div className="empty">{series.kind === "ANSWER" ? "No points yet." : queryReason(series, "the native training journal (BSC-V8-03)")}</div>
    </section>
  </>;

  return <>
    <TopHeading title="Training" subtitle="Modules for testing without real funds."
      actions={<button type="button" className="btn primary" disabled title={NO_PROFILE_OWNER} data-testid="bsc-training-new"><Icon name="plus" size="s" /> New module</button>} />
    <div className="row" style={{ marginBottom: 14 }}><span className="small">1 native configuration · {running ? "Strategy C running" : "No test running"}</span><span className="small">No real funds</span></div>
    <div className="module-list">
      <div className="card module" data-testid="bsc-training-native">
        <button type="button" className="module-card-open" aria-label="Open Strategy C module" onClick={() => setOpen(true)}><span className="symbol"><Icon name="training" /></span>
          <span className="grow"><span className="modname">Strategy C · runtime configuration</span><span className="modsub">{venues} · {running ? "Running (PAPER/SHADOW)" : fresh ? "Not running" : "Runtime not reported"}</span></span></button>
        <div className="modmetrics"><div className="metric"><b className="mono">{gains === undefined ? "—" : `+${bnb(gains)}`}</b><small>Gains · BNB</small></div><div className="metric"><b className="mono">{losses === undefined ? "—" : `−${bnb(losses)}`}</b><small>Losses · BNB</small></div></div>
        <div className="modactions"><button type="button" className="btn sm quiet" onClick={() => setOpen(true)}>Open <Icon name="chevron" size="s" /></button></div>
        <div className="module-quick-actions"><button type="button" className="square-action" disabled aria-label="Delete Strategy C module" title={NO_PROFILE_OWNER}><Icon name="trash" size="s" /></button></div>
      </div>
      <div className="card pad" data-testid="bsc-training-profiles"><div className="empty">{profiles.kind === "ANSWER" && profiles.result.case === "trainingProfiles" ? `${profiles.result.value.profiles.length} saved modules` : queryReason(profiles, "the native training profile owner (BSC-V8-01)")}</div></div>
    </div>
  </>;
}

import { useEffect, useMemo, useRef, useState } from "react";
import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { useSourceControl } from "@/core/realtime/bscControl";
import { selectStatus, useBscStore } from "../store/bscStore";
import { RUNTIME_SCHEMA, runtimeView, sourceControlView } from "../model/sources";
import { generationText } from "../model/strategySettings";
import { featureHints, fieldRanges, formOf } from "../model/strategyForm";
import { appliedProfile, profilesOf, type ProfilesView } from "../model/profiles";
import { Receipt } from "../LiveActions";
import { venueName } from "../model/health";
import { PerformanceMetric, PerformanceScope } from "../api/bscV8Adapter";
import type { BscView } from "../BscPage";
import { useNativeRecords } from "./useNativeRecords";
import { useNowMs } from "./useNowMs";
import { queryReason, useBscQuery, useKept } from "./useBscQuery";
import { StrategyEditor } from "./StrategyEditor";
import { DeleteModuleDialog, ModuleWorkspace, NewModuleDrawer, PROFILE } from "./TrainingModule";
import { RUN, useRuns } from "./TrainingRuns";
import { runMetrics, statusBadge } from "../model/training";
import { useStrategySettings } from "./useStrategySettings";
import { PlacedSection } from "./PlacedSection";
import { Badge, DetailLine, Icon, PanelHead, StateBadge, TopHeading, bnb } from "./V8";

const NO_PROFILE_OWNER = "Training modules need their native profile owner (BSC-V8-01): nothing is stored in the browser.";
const NATIVE_KEPT = "The runtime configuration is no saved module: it is edited in Live and never deleted.";
const NATIVE_NO_RUN = "A test replays a saved module: Duplicate saves this configuration as one, then start its test there.";

/** V8 Training (FE-V8-04…08). The module list shows the one native Strategy C configuration with
 *  its running PAPER/SHADOW results (the runtime status) and the native profile owner's modules
 *  (bsc/query TrainingProfiles: create, edit, duplicate, delete or archive at the owner's
 *  versions), each with its newest test run (TrainingRuns); a module's test runs (REPLAY) start,
 *  stop and show their results in the module (./TrainingRuns). */
export function TrainingPanel({ goto }: { goto: (v: BscView) => void }) {
  useTopic(TopicId.BSC_MONITORING_STATE);
  const records = useNativeRecords();
  const nowMs = useNowMs();
  const { projection: status } = useBscStore(selectStatus);
  // The open module: the native Strategy C configuration, or one of the owner's modules (its id).
  const [open, setOpen] = useState<"native" | string | null>(null);
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);
  const { busy, receipt } = useSourceControl();
  const [period, setPeriod] = useState<"7T" | "30T" | "All">("7T");
  // One catalog and run read per view and per decided module or run request (never while one is
  // sent): no polling.
  const decided = receipt && (receipt.origin === PROFILE || receipt.origin === RUN) && receipt.phase !== "SENDING" ? `${receipt.id}:${receipt.phase}` : null;
  const [step, setStep] = useState("");
  useEffect(() => { if (decided) setStep(decided); }, [decided]);
  const profiles = useBscQuery(`training_profiles:${step}`, { case: "trainingProfiles", value: { includeArchived: true } });
  // The last answered catalog stays shown while the next one loads (a save never closes the module).
  const catalog = useKept<ProfilesView>(useMemo(() => profilesOf(profiles), [profiles]), profiles.kind === "LOADING");
  // The list's results: the newest runs of every module (its first page), only while the list shows.
  const allRuns = useRuns(null, step, "", open === null).newest;
  const newestOf = (id: string) => allRuns.ok ? allRuns.runs.find(r => r.profileId === id) : undefined;
  const testing = allRuns.ok ? allRuns.runs.filter(r => r.active).length : 0;
  // A created or duplicated module opens once the owner applied it; a deleted one closes. Only a
  // request decided while this view is shown (an older receipt reopens nothing).
  const shownReceipt = useRef(receipt?.id);
  useEffect(() => {
    if (!receipt || receipt.id === shownReceipt.current) return;
    const id = receipt.origin === PROFILE ? appliedProfile(receipt.detail) : undefined;
    if (id && /\baction=(CREATE|CLONE)\b/.test(receipt.detail)) setOpen(id);
    if (id && /\baction=DELETE\b/.test(receipt.detail)) setOpen(o => o === id ? null : o);
  }, [receipt]);
  const from = period === "All" ? undefined : BigInt(nowMs - (period === "7T" ? 7 : 30) * 86_400_000);
  const series = useBscQuery(`training-series:${period}`, { case: "series", value: { metric: PerformanceMetric.TRAINING_NET_RESULT, scope: PerformanceScope.TRAINING_PROFILE, fromUnixMs: from } });
  // The settings state is asked for only while the module detail shows it.
  const settings = useStrategySettings(records, status?.warnings ?? [], nowMs, open !== null || creating);
  const { form } = useMemo(() => formOf(settings), [settings]);
  const runtime = runtimeView(records, nowMs).view;
  const control = sourceControlView(records, nowMs);
  const venues = control.venues.filter(v => v.effective !== "OFF").map(v => venueName(v.id)).join(", ") || "No venue";
  // The venues a module can name: the native venue list of the source-modes record.
  const offered = control.venues.map(v => v.id);
  const modules = catalog.ok ? catalog.profiles : [];
  const module = open && open !== "native" ? modules.find(p => p.id === open) : undefined;
  const seriesText = series.kind === "ANSWER" ? "No points yet." : queryReason(series, "the native training journal (BSC-V8-03)");
  const rt = new Map(records.find(r => r.schema === RUNTIME_SCHEMA)?.fields.map(x => [x.name, x.value]) ?? []);
  const fresh = runtime.state === "RUNNING";
  const y = (k: string) => fresh ? rt.get(`strategy[0].${k}`) : undefined;
  // Running only in a running mode; SETTINGS_REFUSED / GAS_EVIDENCE_REFUSED did not start it.
  const running = fresh && ["SHADOW", "PAPER", "LIVE"].includes(y("state") ?? "");
  const gains = y("gain_wei"), losses = y("loss_wei");

  const dialogs = <>
    {creating && catalog.ok ? <NewModuleDrawer all={modules} catalogVersion={catalog.catalogVersion} offered={offered} settings={settings} onClose={() => setCreating(false)}
      start={{ form, venues: control.venues.filter(v => v.effective !== "OFF").map(v => v.id), filters: settings.basis?.values.BSC_STRATEGY_C_FILTERS ?? "" }} /> : null}
    <DeleteModuleDialog profile={modules.find(p => p.id === deleting)} onClose={() => setDeleting(null)} />
  </>;

  if (module && catalog.ok) return <>
    <ModuleWorkspace key={`${module.id}:${module.revision.toString()}`} profile={module} all={modules} catalogVersion={catalog.catalogVersion} offered={offered} settings={settings}
      seriesText={seriesText} step={step} onBack={() => setOpen(null)} onDelete={() => setDeleting(module.id)} />
    {dialogs}
  </>;

  if (open === "native") return <>
    <button type="button" className="workspace-back" onClick={() => setOpen(null)}>‹ All modules</button>
    <TopHeading title="Strategy C · runtime configuration" subtitle={`${venues} · Paper trading · no real funds`}
      actions={<><button type="button" className="btn quiet" disabled={!catalog.ok || busy} title={catalog.ok ? "A new module from this configuration" : NO_PROFILE_OWNER} onClick={() => setCreating(true)} data-testid="bsc-training-native-duplicate">Duplicate</button>
        <button type="button" className="btn quiet" disabled title={NATIVE_KEPT}>Delete</button></>} />
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
      <StrategyEditor form={form} onChange={() => undefined} disabled venues={venues} idPrefix="bsc-training" ranges={fieldRanges(settings)} features={featureHints(settings)} />
      <h3 style={{ fontSize: 14, margin: "22px 0 12px" }}>Test run</h3>
      <div className="test-plan"><div className="module-run-meta" data-testid="bsc-training-native-run">{NATIVE_NO_RUN}</div></div>
      <div className="v8-section-group" data-testid="bsc-training-native-record"><PlacedSection panel="live" section={10} records={records}>
        <DetailLine label="Settings generation" value={generationText(settings)} />
      </PlacedSection></div>
      <div className="editor-footer"><span className="small">{running ? "Strategy C runs in the runtime (its switch and the runtime mode, Settings)." : "Strategy C is not running."}</span>
        <div className="editor-actions"><button type="button" className="btn quiet" onClick={() => goto("live")}>Edit in Live</button><button type="button" className="btn primary" disabled title={NATIVE_NO_RUN} data-testid="bsc-training-start">Start test</button></div></div>
    </section>
    <section className="card pad mtl"><PanelHead title="Performance over time" extra={<div className="segment" role="group" aria-label="Time range">{([["7T", "7 days"], ["30T", "30 days"], ["All", "All time"]] as const).map(([v, t]) => <button key={v} type="button" className={period === v ? "active" : undefined} onClick={() => setPeriod(v)}>{t}</button>)}</div>} />
      <div className="empty">{seriesText}</div>
    </section>
    {dialogs}
  </>;

  const catalogReason = catalog.ok ? "" : catalog.reason ? `Unavailable: ${catalog.reason}.` : queryReason(profiles, "the native training profile owner (BSC-V8-01)");
  return <>
    <TopHeading title="Training" subtitle="Modules for testing without real funds."
      actions={<button type="button" className="btn primary" disabled={!catalog.ok || busy} title={catalog.ok ? undefined : NO_PROFILE_OWNER} onClick={() => setCreating(true)} data-testid="bsc-training-new"><Icon name="plus" size="s" /> New module</button>} />
    <div className="row" style={{ marginBottom: 14 }}><span className="small">1 native configuration{catalog.ok ? ` · ${modules.length} module${modules.length === 1 ? "" : "s"}` : ""} · {testing ? `${testing} test running` : "No test running"}{running ? " · Strategy C running" : ""}</span><span className="small">No real funds</span></div>
    <Receipt testId={PROFILE} />
    <div className="module-list">
      <div className="card module" data-testid="bsc-training-native">
        <button type="button" className="module-card-open" aria-label="Open Strategy C module" onClick={() => setOpen("native")}><span className="symbol"><Icon name="training" /></span>
          <span className="grow"><span className="modname">Strategy C · runtime configuration</span><span className="modsub">{venues} · {running ? "Running (PAPER/SHADOW)" : fresh ? "Not running" : "Runtime not reported"}</span></span></button>
        <div className="modmetrics"><div className="metric"><b className="mono">{gains === undefined ? "—" : `+${bnb(gains)}`}</b><small>Gains · BNB</small></div><div className="metric"><b className="mono">{losses === undefined ? "—" : `−${bnb(losses)}`}</b><small>Losses · BNB</small></div></div>
        <div className="modactions"><button type="button" className="btn sm quiet" onClick={() => setOpen("native")}>Open <Icon name="chevron" size="s" /></button></div>
        <div className="module-quick-actions"><button type="button" className="square-action" disabled aria-label="Delete Strategy C module" title={NATIVE_KEPT}><Icon name="trash" size="s" /></button></div>
      </div>
      {modules.map(p => { const last = newestOf(p.id); const net = runMetrics(last).netWei; const using = p.inUse || !!last?.active; return <div className="card module" key={p.id} data-testid={`bsc-training-module-${p.id}`}>
        <button type="button" className="module-card-open" aria-label={`Open ${p.name} module`} onClick={() => setOpen(p.id)}><span className={`symbol ${using ? "warn" : p.status === "ARCHIVED" ? "blue" : ""}`}><Icon name="training" /></span>
          <span className="grow"><span className="modname">{p.name}</span><span className="modsub">{p.venues.map(venueName).join(", ")} · {p.status === "ARCHIVED" ? "Archived" : last?.active ? statusBadge(last.status).label : p.inUse ? `In use: ${p.inUseReason || "a run or LIVE"}` : last || p.runIds.length ? "Previously tested" : "Not yet tested"}</span></span></button>
        <div className="modmetrics"><div className="metric"><b className={`mono ${net?.startsWith("-") ? "down" : net && net !== "0" ? "up" : ""}`} title={last ? `Run ${last.id}` : allRuns.ok ? "Not yet tested" : "Runs not read"}>{net === undefined ? "—" : bnb(net, 6)}</b><small>Result · BNB</small></div></div>
        <div className="modactions"><button type="button" className="btn sm quiet" onClick={() => setOpen(p.id)}>Open <Icon name="chevron" size="s" /></button></div>
        <div className="module-quick-actions"><button type="button" className="square-action" disabled={busy || using} aria-label={`Delete ${p.name}`} title={using ? "In use: duplicate it to change it" : "Delete module"} onClick={() => setDeleting(p.id)}><Icon name="trash" size="s" /></button></div>
      </div>; })}
      {catalog.ok && modules.length ? null : <div className="card pad" data-testid="bsc-training-profiles"><div className="empty">{catalog.ok ? "No saved modules yet: New module saves one at the native profile owner." : catalogReason}</div></div>}
    </div>
    {dialogs}
  </>;
}

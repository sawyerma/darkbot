import { useMemo, useState } from "react";
import { trainingCommand, useSourceControl } from "@/core/realtime/bscControl";
import { v8Params } from "../api/bscV8Adapter";
import { Receipt } from "../LiveActions";
import type { ProfileView } from "../model/profiles";
import {
  PLAN_START, STOP_REASON, defaultSource, planText, runMetrics, runsOf, sourceLabel, sourcesOf, startParams, statusBadge, timeText,
  type PlanForm, type PlanKind, type RunView, type RunsView, type SourcesView,
} from "../model/training";
import { queryReason, useBscQuery, useKept } from "./useBscQuery";
import { Badge, PanelHead, bnb } from "./V8";

// V8 Training runs (FE-V8-07/08, BSC-V8-02/03): a module's REPLAY runs at the native run owner.
// One query per view, decided request, page step or Refresh (the owner publishes no run topic);
// a run's counts are its end's. Start replays the module's saved revision over one recorded
// runtime run; Stop asks the owner to end the run (its job seals the results).

export const RUN = "bsc-training-run";
export const NOT_REPLAY = "Needs the runtime's live inputs: the run owner replays recorded runs only (REPLAY).";
const RUNS_OWNER = "the native training run owner (BSC-V8-02)";

/** The runs of one module (`profileId`) or of every module (null): the newest page, and an older
 *  page while the operator steps back (`cursor`). `step` changes with every decided request;
 *  `ask` false sends nothing. */
export function useRuns(profileId: string | null, step: string, cursor = "", ask = true): { newest: RunsView; page: RunsView; text: string } {
  const id = profileId ?? "";
  const first = useBscQuery(ask ? `training_runs:${id}::${step}` : "training_runs:none", ask ? { case: "trainingRuns", value: { profileId: id, cursor: "", limit: 0 } } : null);
  const older = useBscQuery(`training_runs:${id}:${cursor}:${step}`, cursor ? { case: "trainingRuns", value: { profileId: id, cursor, limit: 0 } } : null);
  const newest = useKept(useMemo(() => runsOf(first), [first]), first.kind === "LOADING");
  const shown = useKept(useMemo(() => runsOf(older), [older]), older.kind === "LOADING");
  const page = cursor ? shown : newest;
  const q = cursor ? older : first;
  const text = page.ok ? "" : page.reason ? `Unavailable: ${page.reason}.` : queryReason(q, RUNS_OWNER);
  return { newest, page, text };
}

/** The recorded runs a start can replay (newest first, cursor pages). */
export function useSources(step: string, ask: boolean, cursor: string): SourcesView & { text: string } {
  const q = useBscQuery(`replay_sources:${cursor}:${step}`, ask ? { case: "replaySources", value: { cursor, limit: 0 } } : null);
  const view = useKept(useMemo(() => sourcesOf(q), [q]), q.kind === "LOADING");
  return { ...view, text: view.ok ? "" : view.reason ? `Unavailable: ${view.reason}.` : queryReason(q, "the native replay catalogue (BSC-V8-02)") };
}

/** The V8 “Test run” block: the mode (REPLAY only), the bound and the recorded run. */
export function RunPlanFields({ plan, onPlan, sources, source, onSource, onOlder, disabled, meta, idPrefix }: {
  plan: PlanForm; onPlan: (p: PlanForm) => void; sources: SourcesView & { text: string }; source: string; onSource: (id: string) => void;
  onOlder?: (() => void) | undefined; disabled: boolean; meta: string; idPrefix: string;
}) {
  const offered = sources.ok ? sources.sources : [];
  return <div className="test-plan" data-testid={`${idPrefix}-plan`}><div className="test-plan-row">
    <div className="field"><label htmlFor={`${idPrefix}-mode`}>Mode</label><select id={`${idPrefix}-mode`} className="select" value="REPLAY" disabled={disabled} onChange={() => undefined}>
      <option value="REPLAY">Replay a recorded run</option><option value="PAPER" disabled title={NOT_REPLAY}>Paper (live inputs)</option><option value="SHADOW" disabled title={NOT_REPLAY}>Shadow (live inputs)</option></select></div>
    <div className="field"><label htmlFor={`${idPrefix}-bound`}>Stop test by</label><select id={`${idPrefix}-bound`} className="select" value={plan.kind} disabled={disabled} onChange={e => onPlan({ ...plan, kind: e.target.value as PlanKind })} data-testid={`${idPrefix}-bound`}>
      <option value="DURATION">Duration</option><option value="LAUNCH_COUNT">Number of launches</option><option value="UNTIL_STOPPED">Until stopped</option></select></div>
    {plan.kind === "DURATION" ? <div className="field"><label htmlFor={`${idPrefix}-hours`}>Duration · hours of recording</label><input id={`${idPrefix}-hours`} className="input" inputMode="decimal" value={plan.hours} disabled={disabled} onChange={e => onPlan({ ...plan, hours: e.target.value })} data-testid={`${idPrefix}-hours`} /></div> : null}
    {plan.kind === "LAUNCH_COUNT" ? <div className="field"><label htmlFor={`${idPrefix}-launches`}>Maximum launches processed</label><input id={`${idPrefix}-launches`} className="input" inputMode="numeric" value={plan.launches} disabled={disabled} onChange={e => onPlan({ ...plan, launches: e.target.value })} data-testid={`${idPrefix}-launches`} /></div> : null}
    <div className="field" style={{ maxWidth: 360, flexBasis: 280 }}><label htmlFor={`${idPrefix}-source`}>Recorded run</label>
      <select id={`${idPrefix}-source`} className="select" value={source} disabled={disabled || !offered.length} onChange={e => onSource(e.target.value)} data-testid={`${idPrefix}-source`}>
        {offered.length ? offered.map(s => <option key={s.id} value={s.id}>{sourceLabel(s)}</option>) : <option value="">{sources.ok ? "No recorded run yet" : sources.text}</option>}</select></div>
    {onOlder ? <button type="button" className="btn sm quiet" disabled={disabled} onClick={onOlder}>Older recordings</button> : null}
  </div><div className="module-run-meta" data-testid={`${idPrefix}-run-state`}>{meta}</div></div>;
}

/** The V8 “Test result” hero of one run (the picked one, else the newest). */
export function RunHero({ run, text, testId }: { run: RunView | undefined; text: string; testId: string }) {
  const m = runMetrics(run);
  const badge = statusBadge(run?.status);
  const t = run && !run.active ? run.result : undefined;
  const net = m.netWei;
  const reasons = t?.qualityReasons.length ? t.qualityReasons : [];
  return <div className="card workspace-hero" data-testid={testId}><div className="row"><h2>Test result</h2><Badge label={badge.label} tone={badge.tone} testId={`${testId}-status`} /></div>
    <div className="workspace-metrics mt">
      <div><small>Net result</small><b className={net?.startsWith("-") ? "down" : net && net !== "0" ? "up" : ""} title={t && net === undefined ? "Unresolved: positions were followed or unresolved at the end" : undefined}>{net === undefined ? "—" : `${bnb(net, 6)} BNB`}</b></div>
      <div><small>Completed trades</small><b>{m.completed?.toString() ?? "—"}</b></div>
      <div><small>Win rate</small><b>{m.winRate ?? "—"}</b></div>
      <div><small>Launches checked</small><b>{m.evaluated?.toString() ?? "—"}</b></div>
      <div><small>Rejected</small><b>{m.rejected?.toString() ?? "—"}</b></div>
    </div>
    <p className="tiny mt" data-testid={`${testId}-detail`}>{run
      ? <>Run <span className="mono">{run.id}</span> · revision {run.revision.toString()} · {planText(run.plan)} · recorded run <span className="mono">{run.replaySource || "—"}</span>{run.stopReason ? ` · ${run.stopReason}` : ""}
        {run.active ? " · Counts come at the run's end: the owner reports none while it runs." : t
          ? ` · passed ${t.passed} · data incomplete ${t.dataIncomplete} · not tracked ${t.notTracked} · entered ${t.entered} · exits target ${t.exitsTarget}, stop ${t.exitsStop}, creator sell ${t.exitsCreatorSell}, timeout ${t.exitsTimeout} · open at end ${t.openAtEnd} · unresolved ${t.unresolved} · gains ${t.gainWei ? `${bnb(t.gainWei, 6)} BNB` : "—"} · losses ${t.lossWei ? `${bnb(t.lossWei, 6)} BNB` : "—"}`
          : " · No result recorded for this run (its counts were never taken or do not read)."}
        {reasons.length ? ` · Incomplete: ${reasons.join("; ")}` : ""}</>
      : text || "No test run yet: Start test replays this module over a recorded run, without real funds."}</p>
  </div>;
}

/** The module's runs (FE-V8-07/08): picking one shows its result in the hero. */
export function RunList({ runs, text, picked, onPick, onRefresh, onOlder, onNewer, testId }: {
  runs: RunsView; text: string; picked: string | undefined; onPick: (id: string) => void; onRefresh: () => void;
  onOlder: (() => void) | undefined; onNewer: (() => void) | undefined; testId: string;
}) {
  const rows = runs.ok ? runs.runs : [];
  return <section className="card mtl" data-testid={testId}>
    <div style={{ padding: "20px 22px 0" }}><PanelHead title="Test runs" extra={<button type="button" className="btn sm quiet" onClick={onRefresh} data-testid={`${testId}-refresh`}>Refresh</button>} /></div>
    <div className="table-wrap"><table><thead><tr><th>Run</th><th>Status</th><th>Plan</th><th>Recorded run</th><th>Started</th><th>Ended</th><th className="right">Launches</th><th className="right">Net · BNB</th></tr></thead>
      <tbody>{rows.length ? rows.map(r => { const b = statusBadge(r.status); const net = runMetrics(r).netWei; return <tr key={r.id} data-testid={`${testId}-row-${r.id}`} aria-selected={r.id === picked}>
        <td><button type="button" className="btn sm quiet" aria-pressed={r.id === picked} onClick={() => onPick(r.id)}>{r.id}</button><span className="tiny" style={{ display: "block" }}>revision {r.revision.toString()}</span></td>
        <td><Badge label={b.label} tone={b.tone} /></td><td>{planText(r.plan)}</td><td className="mono">{r.replaySource || "—"}</td>
        <td>{timeText(r.startedMs)}</td><td>{timeText(r.endedMs)}</td><td className="right mono">{r.active || !r.result ? "—" : r.launches.toString()}</td>
        <td className="right mono">{net === undefined ? "—" : bnb(net, 6)}</td></tr>; })
        : <tr><td colSpan={8}><div className="empty" data-testid={`${testId}-empty`}>{runs.ok ? "No test run yet." : text}</div></td></tr>}</tbody></table></div>
    <div className="wallet-tool-row" style={{ padding: "0 22px 18px" }}>
      {onNewer ? <button type="button" className="btn sm quiet" onClick={onNewer}>Newer</button> : null}
      <button type="button" className="btn sm quiet" disabled={!onOlder} onClick={onOlder} data-testid={`${testId}-older`}>Older</button>
    </div>
  </section>;
}

/** A module's run controls and their state: the plan, the recorded run, Start and Stop. */
export function useRunControls(profile: ProfileView, newest: RunView | undefined, step: string, blocked: string | undefined) {
  const { busy } = useSourceControl();
  const [plan, setPlan] = useState<PlanForm | null>(null);
  const [cursors, setCursors] = useState<string[]>([""]);
  const sources = useSources(step, true, cursors[cursors.length - 1] ?? "");
  const [chosen, setChosen] = useState<string | null>(null);
  const source = chosen ?? (sources.ok ? defaultSource(sources.sources) : "");
  const p = plan ?? PLAN_START;
  const params = startParams(profile, p, source);
  const active = newest?.active ? newest : undefined;
  const why = active ? `Test ${statusBadge(active.status).label.toLowerCase()} (run ${active.id}): stop it to change or start this module.`
    : blocked ?? ("error" in params ? params.error : `Start test replays revision ${profile.revision.toString()} on ${profile.venues.length} market${profile.venues.length === 1 ? "" : "s"} over recorded run ${source}: offline, no real funds; one test at a time.`);
  const start = () => {
    if ("error" in params || active || blocked) return;
    void trainingCommand(RUN, "bsc.training.run.start", profile.id, v8Params("training_run_start_command", params), profile.revision,
      `start ${profile.id} revision ${profile.revision.toString()} over ${source}`);
  };
  const stop = () => {
    if (!active || active.status === "STOPPING") return;
    void trainingCommand(RUN, "bsc.training.run.stop", active.id, v8Params("training_run_stop_command", { reason: STOP_REASON }), null, `stop ${active.id}`);
  };
  const olderSources = sources.ok && sources.hasMore && sources.nextCursor ? () => { setCursors([...cursors, sources.nextCursor]); setChosen(null); } : undefined;
  const fields = (disabled: boolean) => <RunPlanFields plan={p} onPlan={setPlan} sources={sources} source={source} onSource={setChosen} onOlder={olderSources}
    disabled={disabled || busy || !!active} meta={why} idPrefix={RUN} />;
  const button = active
    ? <button type="button" className="btn danger" disabled={busy || active.status === "STOPPING"} onClick={stop} data-testid={`${RUN}-stop`}>{active.status === "STOPPING" ? "Stopping…" : "Stop test"}</button>
    : <button type="button" className="btn primary" disabled={busy || !!blocked || "error" in params} title={blocked ?? ("error" in params ? params.error : undefined)} onClick={start} data-testid={`${RUN}-start`}>Start test</button>;
  return { fields, button, active, receipt: <Receipt testId={RUN} /> };
}

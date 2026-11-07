import "@/shared/ui/graphite/control-dialog.css";
import { useEffect, useRef, useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { profileCommand, useSourceControl } from "@/core/realtime/bscControl";
import { v8Params } from "../api/bscV8Adapter";
import { Receipt } from "../LiveActions";
import { copyName, marketOptions, moduleParams, type ModuleDraft, type ProfileView } from "../model/profiles";
import { featureHints, fieldRanges, filterOrder, formOfValues, sameForm, type StrategyForm } from "../model/strategyForm";
import { settableInit, type StrategySettingsView } from "../model/strategySettings";
import { venueName } from "../model/health";
import { StrategyEditor } from "./StrategyEditor";
import { RUN, RunHero, RunList, useRunControls, useRuns } from "./TrainingRuns";
import { appliedRun } from "../model/training";
import { Badge, Drawer, TopHeading } from "./V8";

// V8 Training modules (FE-V8-04, BSC-V8-01): the native profile owner's modules. A module is
// edited as a whole revision (name, market, every value) and saved at its revision; a new one and
// a duplicate at the catalog version. Its test runs (REPLAY) and their results are the native run
// owner's (FE-V8-07/08, BSC-V8-02/03, ./TrainingRuns).

export const PROFILE = "bsc-training-profile";
/** A new module is tested once saved (a run replays a saved revision). */
export const RUN_AFTER_SAVE = "A test replays a saved revision: save the module, then start its test in the module.";

const FILTERS_KEY = "BSC_STRATEGY_C_FILTERS";
const sameDraft = (a: ModuleDraft, b: ModuleDraft) => a.name === b.name && a.venues.join(",") === b.venues.join(",") && sameForm(a.form, b.form);
const venuesText = (venues: readonly string[]) => venues.map(venueName).join(", ") || "No market";

/** The module's settings form: name, market and the ten values with the 15 filters. */
function ModuleEditor({ draft, onChange, offered, settings, disabled, idPrefix }: {
  draft: ModuleDraft; onChange: (d: ModuleDraft) => void; offered: readonly string[]; settings: StrategySettingsView; disabled: boolean; idPrefix: string;
}) {
  const options = marketOptions(offered, draft.venues, venueName);
  return <StrategyEditor form={draft.form} onChange={(form: StrategyForm) => onChange({ ...draft, form })} disabled={disabled} venues={venuesText(draft.venues)} idPrefix={idPrefix}
    ranges={fieldRanges(settings)} features={featureHints(settings)}
    name={{ value: draft.name, onChange: name => onChange({ ...draft, name }) }}
    market={{ value: draft.venues.join(","), options, onChange: key => onChange({ ...draft, venues: options.find(o => o.key === key)?.venues ?? [] }) }} />;
}

/** One module (V8 trainingWorkspace): its test result, its settings at its revision with the test
 *  run, its runs, Duplicate and Delete. `step` changes with every decided module or run request. */
export function ModuleWorkspace({ profile, all, catalogVersion, offered, settings, seriesText, step, onBack, onDelete }: {
  profile: ProfileView; all: readonly ProfileView[]; catalogVersion: bigint; offered: readonly string[]; settings: StrategySettingsView;
  seriesText: string; step: string; onBack: () => void; onDelete: () => void;
}) {
  const { busy, receipt } = useSourceControl();
  const parsed = formOfValues(profile.values);
  const base: ModuleDraft = { name: profile.name, venues: profile.venues, form: parsed.form };
  const [draft, setDraft] = useState<ModuleDraft | null>(null);
  const d = draft ?? base;
  // The runs: re-read per decided request and per Refresh; older pages by their cursors.
  const [refresh, setRefresh] = useState(0);
  const [cursors, setCursors] = useState<string[]>([""]);
  const runStep = `${step}:${refresh}`;
  const runs = useRuns(profile.id, runStep, cursors[cursors.length - 1] ?? "");
  const newest = runs.newest.ok ? runs.newest.runs[0] : undefined;
  const [picked, setPicked] = useState<string | null>(null);
  // A start the owner applied while this module shows: its run in the hero, the newest page listed
  // (a receipt from before the module opened picks nothing).
  const shownReceipt = useRef(receipt?.id);
  useEffect(() => {
    if (!receipt || receipt.id === shownReceipt.current || receipt.origin !== RUN) return;
    const started = appliedRun(receipt.detail);
    if (started) { setPicked(started); setCursors([""]); }
  }, [receipt]);
  const shown = (picked && [...(runs.page.ok ? runs.page.runs : []), ...(runs.newest.ok ? runs.newest.runs : [])].find(r => r.id === picked)) || newest;
  // In use: by the profile owner's word, or a run of this module its job still runs (the owner
  // refuses an edit then; the lock only spares the request).
  const running = newest?.active ? newest : undefined;
  const locked = profile.inUse || !!running;
  const others = all.filter(p => p.id !== profile.id);
  const params = moduleParams(d, others, filterOrder(profile.values[FILTERS_KEY] ?? ""), settings.basis?.specs, settings.basis?.features);
  const changed = !sameDraft(d, base);
  const save = () => {
    if ("error" in params) return;
    void profileCommand(PROFILE, "bsc.training.profile.update", profile.id,
      v8Params("training_profile_update_command", { profileId: profile.id, name: params.name, venues: params.venues, values: settableInit(params.values) }),
      { case: "profileVersion", value: profile.revision }, `update ${profile.id} at revision ${profile.revision.toString()}`);
  };
  const duplicate = () => void profileCommand(PROFILE, "bsc.training.profile.clone", profile.id,
    v8Params("training_profile_clone_command", { sourceProfileId: profile.id, sourceRevision: profile.revision, name: copyName(profile.name, all) }),
    { case: "configVersion", value: catalogVersion }, `clone ${profile.id} revision ${profile.revision.toString()}`);
  const changedNote = changed ? "Save the module before its test: a test replays a saved revision." : undefined;
  const blocked = profile.status === "ARCHIVED" ? "Archived: duplicate it to test it." : profile.inUse && !running ? `In use: ${profile.inUseReason || "a run or LIVE"}.` : changedNote;
  const run = useRunControls(profile, newest, runStep, blocked);
  return <>
    <button type="button" className="workspace-back" onClick={onBack}>‹ All modules</button>
    <TopHeading title={profile.name} subtitle={`${venuesText(profile.venues)} · Paper trading · no real funds`}
      actions={<><button type="button" className="btn quiet" disabled={busy} onClick={duplicate} data-testid={`${PROFILE}-duplicate`}>Duplicate</button>
        <button type="button" className="btn quiet" disabled={busy || locked} title={locked ? "In use: duplicate it to change it" : undefined} onClick={onDelete} data-testid={`${PROFILE}-delete`}>Delete</button></>} />
    <RunHero run={shown} text={runs.text} testId={`${PROFILE}-result`} />
    <p className="tiny" style={{ margin: "8px 2px 0" }}>{profile.status === "ARCHIVED" ? "Archived · " : ""}Revision {profile.revision.toString()} · configuration <span className="mono">{profile.configSha256.slice(0, 12)}…</span></p>
    <section className="card editor-card mtl" data-testid={`${PROFILE}-settings`}>
      <div className="editor-heading"><div><h2>Module settings</h2><p className="small">Edit settings and filters directly, with no extra menus.</p></div>{locked ? <Badge label={running ? "Locked during test" : "Locked: in use"} tone="warn" /> : <Badge label="15 filters available" />}</div>
      <ModuleEditor draft={d} onChange={setDraft} offered={offered} settings={settings} disabled={locked || busy} idPrefix={PROFILE} />
      {parsed.filterError ? <p className="tiny v8-unavailable">Stored filters unreadable here: {parsed.filterError}</p> : null}
      <h3 style={{ fontSize: 14, margin: "22px 0 12px" }}>Test run</h3>
      {run.fields(profile.status === "ARCHIVED")}
      <div className="editor-footer"><span className="small" data-testid={`${PROFILE}-state`}>{running ? `Test ${running.status === "STOPPING" ? "stopping" : "running"} · stop to edit.` : locked ? "In use: duplicate it to change it."
        : !changed ? `Revision ${profile.revision.toString()} · changes apply to this module only.` : "error" in params ? params.error : `Saves revision ${(profile.revision + 1n).toString()} of this module.`}</span>
        <div className="editor-actions">{!locked ? <><button type="button" className="btn quiet" disabled={!draft} onClick={() => setDraft(null)}>Reset</button>
          <button type="button" className="btn quiet" disabled={busy || !changed || "error" in params} onClick={() => { save(); setDraft(null); }} data-testid={`${PROFILE}-save`}>Save</button></> : null}
          {run.button}</div></div>
      <Receipt testId={PROFILE} />
      {run.receipt}
    </section>
    <RunList runs={runs.page} text={runs.text} picked={shown?.id} onPick={setPicked} onRefresh={() => setRefresh(n => n + 1)} testId={`${PROFILE}-runs`}
      onOlder={runs.page.ok && runs.page.hasMore && runs.page.nextCursor ? () => { const next = runs.page.ok ? runs.page.nextCursor : ""; setCursors(c => [...c, next]); } : undefined}
      onNewer={cursors.length > 1 ? () => setCursors(c => c.slice(0, -1)) : undefined} />
    <section className="card pad mtl"><h2>Performance over time</h2><div className="empty">{seriesText}</div></section>
  </>;
}

/** V8 “New training module”: the current Strategy C configuration as its start, saved at the catalog version. */
export function NewModuleDrawer({ all, catalogVersion, offered, settings, start, onClose }: {
  all: readonly ProfileView[]; catalogVersion: bigint; offered: readonly string[]; settings: StrategySettingsView;
  /** The form and venues a new module starts from (the Strategy C configuration and its venues). */
  start: { form: StrategyForm; venues: readonly string[]; filters: string };
  onClose: () => void;
}) {
  const { busy } = useSourceControl();
  // Each part follows the start (the settings state may still load) until the operator edits it.
  const [name, setName] = useState("");
  const [venues, setVenues] = useState<readonly string[] | null>(null);
  const [form, setForm] = useState<StrategyForm | null>(null);
  const draft: ModuleDraft = { name, venues: venues ?? start.venues, form: form ?? start.form };
  const setDraft = (d: ModuleDraft) => {
    if (d.name !== draft.name) setName(d.name);
    if (d.venues !== draft.venues) setVenues(d.venues);
    if (d.form !== draft.form) setForm(d.form);
  };
  const params = moduleParams(draft, all, filterOrder(start.filters), settings.basis?.specs, settings.basis?.features);
  const create = () => {
    if ("error" in params) return;
    void profileCommand(PROFILE, "bsc.training.profile.create", "",
      v8Params("training_profile_create_command", { name: params.name, venues: params.venues, values: settableInit(params.values) }),
      { case: "configVersion", value: catalogVersion }, `create at catalog ${catalogVersion.toString()}`);
    onClose();
  };
  return <Drawer title="New training module" kicker="TRAINING" wide onClose={onClose} testId={`${PROFILE}-new`}
    footer={<><span className="small grow" data-testid={`${PROFILE}-new-state`}>{"error" in params ? params.error : "Saved by the native profile owner."}</span>
      <button type="button" className="btn quiet" onClick={onClose}>Cancel</button>
      <button type="button" className="btn primary" disabled={busy || "error" in params} onClick={create} data-testid={`${PROFILE}-create`}>Save</button></>}>
    <p className="small">Configure a paper strategy. All filters are visible. It starts from the current Strategy C configuration.</p>
    <ModuleEditor draft={draft} onChange={setDraft} offered={offered} settings={settings} disabled={busy} idPrefix={`${PROFILE}-new`} />
    <h3 style={{ fontSize: 14, margin: "20px 0 12px" }}>Test run</h3>
    <div className="test-plan"><div className="module-run-meta">{RUN_AFTER_SAVE}</div></div>
  </Drawer>;
}

/** V8 “Delete module?”: delete, or archive (kept with its runs, not selectable); never while in use. */
export function DeleteModuleDialog({ profile, onClose }: { profile: ProfileView | undefined; onClose: () => void }) {
  const send = (archive: boolean) => {
    if (!profile) return;
    void profileCommand(PROFILE, "bsc.training.profile.delete", profile.id, v8Params("training_profile_delete_command", { profileId: profile.id, archive }),
      { case: "profileVersion", value: profile.revision }, `${archive ? "archive" : "delete"} ${profile.id} at revision ${profile.revision.toString()}`);
    onClose();
  };
  return <AlertDialog.Root open={!!profile} onOpenChange={o => { if (!o) onClose(); }}>
    <AlertDialog.Portal><AlertDialog.Overlay className="ov-log-overlay" /><AlertDialog.Content className="ov-confirm-view" data-testid={`${PROFILE}-delete-dialog`}>
      <AlertDialog.Title>Delete module?</AlertDialog.Title>
      <AlertDialog.Description asChild><div><p>“{profile?.name}” is deleted by the native profile owner and leaves every list; other modules remain unchanged.
        Archive keeps it with its runs and results, no longer selectable.</p></div></AlertDialog.Description>
      <div className="ov-confirm-actions"><AlertDialog.Cancel asChild><button className="ov-btn">Cancel</button></AlertDialog.Cancel>
        <button className="ov-btn" disabled={profile?.status === "ARCHIVED"} title={profile?.status === "ARCHIVED" ? "Already archived" : undefined} onClick={() => send(true)} data-testid={`${PROFILE}-archive-confirm`}>Archive</button>
        <AlertDialog.Action asChild><button className="ov-btn" onClick={() => send(false)} data-testid={`${PROFILE}-delete-confirm`}>Delete module</button></AlertDialog.Action></div>
    </AlertDialog.Content></AlertDialog.Portal>
  </AlertDialog.Root>;
}

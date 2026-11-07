import "@/shared/ui/graphite/control-dialog.css";
import { useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { profileCommand, useSourceControl } from "@/core/realtime/bscControl";
import { v8Params } from "../api/bscV8Adapter";
import { Receipt } from "../LiveActions";
import { copyName, marketOptions, moduleParams, type ModuleDraft, type ProfileView } from "../model/profiles";
import { featureHints, fieldRanges, filterOrder, formOfValues, sameForm, type StrategyForm } from "../model/strategyForm";
import { settableInit, type StrategySettingsView } from "../model/strategySettings";
import { venueName } from "../model/health";
import { StrategyEditor } from "./StrategyEditor";
import { Badge, Drawer, TopHeading } from "./V8";

// V8 Training modules (FE-V8-04, BSC-V8-01): the native profile owner's modules. A module is
// edited as a whole revision (name, market, every value) and saved at its revision; a new one and
// a duplicate at the catalog version. Runs and results need the native run owner (BSC-V8-02/03).

export const PROFILE = "bsc-training-profile";
export const NO_RUN_OWNER = "Training runs need their native owner (BSC-V8-02): no test starts from here.";

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

/** One module (V8 trainingWorkspace): its result, its settings at its revision, Duplicate and Delete. */
export function ModuleWorkspace({ profile, all, catalogVersion, offered, settings, seriesText, onBack, onDelete }: {
  profile: ProfileView; all: readonly ProfileView[]; catalogVersion: bigint; offered: readonly string[]; settings: StrategySettingsView;
  seriesText: string; onBack: () => void; onDelete: () => void;
}) {
  const { busy } = useSourceControl();
  const parsed = formOfValues(profile.values);
  const base: ModuleDraft = { name: profile.name, venues: profile.venues, form: parsed.form };
  const [draft, setDraft] = useState<ModuleDraft | null>(null);
  const d = draft ?? base;
  const locked = profile.inUse;
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
  const status = profile.status === "ARCHIVED" ? "Archived" : locked ? `In use: ${profile.inUseReason || "a run or LIVE"}` : profile.runIds.length ? "Previously tested" : "Not yet tested";
  return <>
    <button type="button" className="workspace-back" onClick={onBack}>‹ All modules</button>
    <TopHeading title={profile.name} subtitle={`${venuesText(profile.venues)} · Paper trading · no real funds`}
      actions={<><button type="button" className="btn quiet" disabled={busy} onClick={duplicate} data-testid={`${PROFILE}-duplicate`}>Duplicate</button>
        <button type="button" className="btn quiet" disabled={busy || locked} title={locked ? "In use: duplicate it to change it" : undefined} onClick={onDelete} data-testid={`${PROFILE}-delete`}>Delete</button></>} />
    <div className="card workspace-hero" data-testid={`${PROFILE}-result`}><div className="row"><h2>Test result</h2><Badge label={status} tone={profile.status === "ARCHIVED" ? "" : locked ? "warn" : ""} /></div>
      <div className="workspace-metrics mt">
        <div><small>Net result</small><b>—</b></div><div><small>Completed trades</small><b>—</b></div><div><small>Win rate</small><b>—</b></div>
        <div><small>Launches checked</small><b>—</b></div><div><small>Rejected</small><b>—</b></div>
      </div>
      <p className="tiny mt">{NO_RUN_OWNER} Revision {profile.revision.toString()} · configuration <span className="mono">{profile.configSha256.slice(0, 12)}…</span></p>
    </div>
    <section className="card editor-card mtl" data-testid={`${PROFILE}-settings`}>
      <div className="editor-heading"><div><h2>Module settings</h2><p className="small">Edit settings and filters directly, with no extra menus.</p></div>{locked ? <Badge label="Locked: in use" tone="warn" /> : <Badge label="15 filters available" />}</div>
      <ModuleEditor draft={d} onChange={setDraft} offered={offered} settings={settings} disabled={locked || busy} idPrefix={PROFILE} />
      {parsed.filterError ? <p className="tiny v8-unavailable">Stored filters unreadable here: {parsed.filterError}</p> : null}
      <h3 style={{ fontSize: 14, margin: "22px 0 12px" }}>Test run</h3>
      <div className="test-plan"><div className="module-run-meta">{NO_RUN_OWNER}</div></div>
      <div className="editor-footer"><span className="small" data-testid={`${PROFILE}-state`}>{locked ? "In use: duplicate it to change it."
        : !changed ? `Revision ${profile.revision.toString()} · changes apply to this module only.` : "error" in params ? params.error : `Saves revision ${(profile.revision + 1n).toString()} of this module.`}</span>
        <div className="editor-actions"><button type="button" className="btn quiet" disabled={!draft} onClick={() => setDraft(null)}>Reset</button>
          <button type="button" className="btn quiet" disabled={locked || busy || !changed || "error" in params} onClick={() => { save(); setDraft(null); }} data-testid={`${PROFILE}-save`}>Save</button>
          <button type="button" className="btn primary" disabled title={NO_RUN_OWNER}>Start test</button></div></div>
      <Receipt testId={PROFILE} />
    </section>
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
    <div className="test-plan"><div className="module-run-meta">{NO_RUN_OWNER}</div></div>
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

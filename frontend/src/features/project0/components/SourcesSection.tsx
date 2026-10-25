import { useState } from "react";
import type { TopicStatus } from "@/core/realtime";
import { analyticsCommand, sourceCommand, useAnalyticsControl } from "@/core/realtime/analyticsControl";
import type { LiquidatorView } from "../model/owner";
import { DetailRow, Notice, Section } from "./PanelPrimitives";

type Mode = "off" | "observe" | "active";

/**
 * S1-02 (CTRL-07): the runtime owner's data sources as its arbiter holds them — applied mode,
 * qualification, generation, the canonical unit, the last control decision — and the mode change
 * through the same controller as `p0ctl source set` (the owner applies or refuses it in order;
 * ACTIVE needs a qualification record). A state that is not LIVE is never offered for a change.
 * The command reaches only a runtime owner of the app's data home; without one it fails
 * (SOURCE_NO_RUNTIME), and a run started from a terminal elsewhere is changed with `p0ctl source
 * set` in its own project (review 72b74fc1).
 */
export function SourcesSection({ owner, topicStatus }: { owner: LiquidatorView | null; topicStatus: TopicStatus }) {
  const { busy, receipt } = useAnalyticsControl();
  const units = owner?.sources ?? [];
  const [wanted, setWanted] = useState(""); const [mode, setMode] = useState<Mode>("observe"); const [msg, setMsg] = useState("");
  const unit = units.find(u => u.unit === wanted) ?? units[0] ?? null;
  const live = topicStatus === "LIVE";
  const version = owner?.sourceConfigVersion ?? null;
  const blocked = busy ? "Command in progress" : !live || !owner ? `Runtime owner not current (${topicStatus})`
    : version == null ? "Source configuration version unknown (no source control in this runtime)" : "";
  const apply = () => {
    const typed = sourceCommand(unit?.unit ?? "", mode);
    if ("problem" in typed) { setMsg(typed.problem); return; }
    setMsg("");
    // CTRL-03: the change states the version this view showed; a newer one is a conflict.
    void analyticsCommand("project0.source.set", { params: typed.params, configVersion: version ?? undefined });
  };
  const d = owner?.lastSourceDecision ?? null;
  return <Section title="Data sources · runtime owner" aside={<span className="p0-badge">{topicStatus}</span>}>
    {!owner ? <p className="p0-quiet">No Shadow/econ4h owner state received.</p> : null}
    {owner && !live ? <p className="p0-quiet">Last reported sources ({topicStatus}); not current.</p> : null}
    {units.map(u => <DetailRow key={u.unit} title={`${u.unit}${u.canonical ? " · canonical" : ""}`}
      value={`${u.mode} · ${u.qualification} · generation ${u.generation} · ${u.role}`}/>)}
    {d ? <DetailRow title={`Last decision · #${d.seq}`} value={`${d.applied ? "APPLIED" : "REFUSED"} · ${d.request} · ${d.detail}`}/> : null}
    {owner?.sourceStats ? <DetailRow title="Observed / rejected · changes applied / refused"
      value={`${owner.sourceStats.observed} / ${owner.sourceStats.rejected} · ${owner.sourceStats.applied} / ${owner.sourceStats.refused}`}/> : null}
    {units.length ? <>
      <div className="p0-formgrid">
        <label>Unit<select value={unit?.unit ?? ""} onChange={e => setWanted(e.target.value)}>{units.map(u => <option key={u.unit} value={u.unit}>{u.unit}</option>)}</select></label>
        <label>Mode<select value={mode} onChange={e => setMode(e.target.value as Mode)}>
          <option value="off">Off</option><option value="observe">Observe</option><option value="active">Active (needs a qualification)</option></select></label>
      </div>
      <div className="p0-actions"><button className="p0-secondary" disabled={blocked !== ""} title={blocked || undefined} onClick={apply}>Request mode</button></div>
      {blocked ? <p className="p0-quiet">{blocked}</p> : null}
      <p className="p0-quiet">Reaches the runtime owner of this app's data home only (none running: the command fails, SOURCE_NO_RUNTIME); a run started from a terminal elsewhere is changed there with <code>p0ctl source set</code>.</p>
    </> : null}
    {msg ? <Notice>{msg}</Notice> : null}
    {receipt?.action === "project0.source.set" ? <Notice><span data-testid="p0-source-command-receipt">Command {receipt.phase}{receipt.detail ? ` · ${receipt.detail}` : ""} · the owner's decision follows above</span></Notice> : null}
  </Section>;
}

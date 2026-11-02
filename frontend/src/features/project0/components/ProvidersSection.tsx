import { useState } from "react";
import type { TopicStatus } from "@/core/realtime";
import {
  analyticsCommand, providerCredentialCommand, providerDescriptorCommand, providerPriorityCommand,
  providerQualifyCommand, providerSelectCommand, useAnalyticsControl, type ProviderAction, type ProviderInput,
} from "@/core/realtime/analyticsControl";
import type { AnalyticsView, ProviderKind } from "../model/analytics";
import type { SystemView } from "../model/status";
import { effectiveProviders } from "../model/statusSelectors";
import { DetailRow, Notice, Section } from "./PanelPrimitives";

const EMPTY: ProviderInput = { id: "", kind: "", endpoint: "", auth: "", credential: "", priority: "", capacityRps: "", plan: "", entitlement: "", disabled: false };

/**
 * DEC-18 (P0-020): the provider registry of this tab's kinds (Capture: RPC, WSS, Geyser;
 * Liquidator: Jupiter, Jito) as the native store holds it — host, authentication name, credential
 * revision, plan, priority, capacity, binding, selection and why a provider cannot be selected —
 * beside the providers the running process is bound to (effective). Every change goes through the
 * same controller as `p0ctl provider …` with the registry version this view showed (CAS) and is
 * staged: effective at the next controlled start. A credential's key is typed here, sent once in
 * the command and never kept (the gateway hands it to the controller's standard input; the
 * macOS keychain holds it).
 */
export function ProvidersSection({ title, kinds, analytics, topicStatus, system, systemStatus }: {
  title: string; kinds: readonly ProviderKind[]; analytics: AnalyticsView | null; topicStatus: TopicStatus; system: SystemView | null; systemStatus: TopicStatus;
}) {
  const { busy, receipt } = useAnalyticsControl();
  const registry = analytics?.providers ?? null;
  const providers = (registry?.providers ?? []).filter(p => (kinds as readonly string[]).includes(p.kind));
  const used = new Set(providers.map(p => p.credential).filter(Boolean));
  // Review F7: a credential no provider references (set before its provider, left after a removal,
  // freshly imported) stays visible — the Capture tab (the RPC one) holds it.
  const referenced = new Set((registry?.providers ?? []).map(p => p.credential).filter(Boolean));
  const holdsUnreferenced = (kinds as readonly string[]).includes("rpc");
  const credentials = (registry?.credentials ?? []).filter(c => used.has(c.id) || (holdsUnreferenced && !referenced.has(c.id)));
  const [form, setForm] = useState<ProviderInput>({ ...EMPTY, kind: kinds[0] ?? "" });
  const [target, setTarget] = useState(""); const [priority, setPriority] = useState(""); const [evidence, setEvidence] = useState("");
  const [credential, setCredential] = useState(""); const [key, setKey] = useState(""); const [msg, setMsg] = useState("");
  const live = topicStatus === "LIVE";
  const blocked = busy ? "Command in progress" : !live || !registry ? `Provider registry not current (${topicStatus})` : "";
  const chosen = providers.find(p => p.id === target) ?? providers[0] ?? null;
  const send = (action: ProviderAction, typed: { params: Uint8Array } | { problem: string } | null, resourceId?: string) => {
    if (typed && "problem" in typed) { setMsg(typed.problem); return; }
    setMsg("");
    // The change states the registry version this view showed; a newer one is a conflict.
    void analyticsCommand(action, { params: typed?.params, resourceId, configVersion: registry?.version });
  };
  const field = (name: keyof ProviderInput, label: string, placeholder = "") =>
    <label>{label}<input value={String(form[name])} placeholder={placeholder} autoComplete="off" spellCheck={false}
      onChange={e => setForm({ ...form, [name]: e.target.value })}/></label>;
  const setKeyed = (action: "project0.provider.credential.set" | "project0.provider.credential.rotate") => {
    const typed = providerCredentialCommand(credential.trim(), key);
    setKey(""); // the key is never kept beyond this one command
    send(action, typed);
  };
  return <Section title={title} aside={<span className="p0-badge">{registry ? `registry v${registry.version} · ${topicStatus}` : topicStatus}</span>}>
    {analytics?.providersError ? <Notice>Provider registry unreadable: {analytics.providersError}</Notice> : null}
    {!registry ? <p className="p0-quiet">No provider registry received.</p> : null}
    {registry && !live ? <p className="p0-quiet">Last reported registry ({topicStatus}); not current.</p> : null}
    {providers.map(p => <DetailRow key={p.id} title={`${p.kind} · ${p.id}${p.selected ? " · selected" : ""}${p.enabled ? "" : " · disabled"}`}
      value={`${p.host} · ${p.auth}${p.credential ? ` · credential ${p.credential}` : ""} · ${p.plan} (${p.entitlement}) · priority ${p.priority}`
        + `${p.capacityRps != null ? ` · ${p.capacityRps} rps` : ""} · binding ${p.binding ? p.binding.slice(0, 16) : "— (credential revoked or unknown)"}`
        + `${p.qualification ? ` · qualified ${p.qualification}` : " · not qualified"}${p.selectionBlock ? ` · ${p.selectionBlock}` : ""}`}/>)}
    {registry && !providers.length ? <p className="p0-quiet">No {kinds.join("/")} provider in the registry.</p> : null}
    {credentials.map(c => <DetailRow key={c.id} title={`Credential · ${c.id}`} value={`revision ${c.revision} · ${c.state} · ${c.origin}${referenced.has(c.id) ? "" : " · no provider"}`}/>)}
    <DetailRow title="Running process · effective" value={effectiveProviders(system, systemStatus, kinds)}/>
    {registry ? <p className="p0-quiet">A change is staged: effective at the next controlled start ({registry.restartRule}); a running process keeps its providers.</p> : null}
    {providers.length ? <>
      <div className="p0-formgrid">
        <label>Provider<select value={chosen?.id ?? ""} onChange={e => setTarget(e.target.value)}>{providers.map(p => <option key={p.id} value={p.id}>{p.kind} · {p.id}</option>)}</select></label>
        <label>New priority<input value={priority} onChange={e => setPriority(e.target.value)} inputMode="numeric"/></label>
        <label>Evidence record<input value={evidence} placeholder="16 hex digits, or -" onChange={e => setEvidence(e.target.value)}/></label>
      </div>
      <div className="p0-actions">
        {chosen && chosen.kind !== "geyser" ? <button className="p0-secondary" disabled={blocked !== "" || chosen.selected} title={blocked || undefined}
          onClick={() => send("project0.provider.select", providerSelectCommand(chosen.kind, chosen.id))}>Select</button> : null}
        {chosen?.selected && chosen.kind !== "geyser" ? <button className="p0-secondary" disabled={blocked !== ""} onClick={() => send("project0.provider.unselect", null, chosen.kind)}>Unselect</button> : null}
        {chosen ? <button className="p0-secondary" disabled={blocked !== ""} onClick={() => send(chosen.enabled ? "project0.provider.disable" : "project0.provider.enable", null, chosen.id)}>{chosen.enabled ? "Disable" : "Enable"}</button> : null}
        {chosen ? <button className="p0-secondary" disabled={blocked !== ""} onClick={() => send("project0.provider.priority", providerPriorityCommand(chosen.id, priority))}>Set priority</button> : null}
        {chosen ? <button className="p0-secondary" disabled={blocked !== ""} onClick={() => send("project0.provider.qualify", providerQualifyCommand(chosen.id, evidence))}>Qualify</button> : null}
        {chosen ? <button className="p0-secondary" disabled={blocked !== ""} onClick={() => send("project0.provider.remove", null, chosen.id)}>Remove</button> : null}
      </div>
    </> : null}
    <p className="p0-label">Add or update a provider</p>
    <div className="p0-formgrid">
      {field("id", "Id")}
      <label>Kind<select value={form.kind} onChange={e => setForm({ ...form, kind: e.target.value })}>{kinds.map(k => <option key={k} value={k}>{k}</option>)}</select></label>
      {field("endpoint", "Endpoint", "https://… (no key in it)")}
      {field("auth", "Authentication", "none | query:NAME | header:NAME | path")}
      {field("credential", "Authenticates with")}
      <label>Plan<select value={form.plan} onChange={e => setForm({ ...form, plan: e.target.value })}><option value="">—</option><option value="free">Free</option><option value="unknown">Unknown (never selected)</option></select></label>
      {field("entitlement", "Plan name", "e.g. helius-free")}
      {field("priority", "Priority")}
      {field("capacityRps", "Capacity · rps")}
      <label className="p0-check"><input type="checkbox" checked={form.disabled} onChange={e => setForm({ ...form, disabled: e.target.checked })}/>Added disabled</label>
    </div>
    <div className="p0-actions">
      <button className="p0-secondary" disabled={blocked !== ""} title={blocked || undefined} onClick={() => send("project0.provider.add", providerDescriptorCommand(form, true))}>Add</button>
      <button className="p0-secondary" disabled={blocked !== ""} title={blocked || undefined} onClick={() => send("project0.provider.update", providerDescriptorCommand(form, false))}>Update</button>
    </div>
    <p className="p0-label">Credential · macOS keychain</p>
    <div className="p0-formgrid">
      <label>Credential<input value={credential} autoComplete="off" spellCheck={false} onChange={e => setCredential(e.target.value)}/></label>
      <label>Key<input type="password" value={key} autoComplete="off" spellCheck={false} data-testid="p0-provider-key" onChange={e => setKey(e.target.value)}/></label>
    </div>
    <div className="p0-actions">
      <button className="p0-secondary" disabled={blocked !== ""} onClick={() => setKeyed("project0.provider.credential.set")}>Set key</button>
      <button className="p0-secondary" disabled={blocked !== ""} onClick={() => setKeyed("project0.provider.credential.rotate")}>Rotate key</button>
      <button className="p0-secondary" disabled={blocked !== ""} onClick={() => send("project0.provider.credential.revoke", null, credential.trim())}>Revoke</button>
    </div>
    {blocked ? <p className="p0-quiet">{blocked}</p> : null}
    {msg ? <Notice>{msg}</Notice> : null}
    {receipt?.action.startsWith("project0.provider.") ? <Notice><span data-testid="p0-provider-command-receipt">Command {receipt.phase}{receipt.detail ? ` · ${receipt.detail}` : ""}</span></Notice> : null}
  </Section>;
}

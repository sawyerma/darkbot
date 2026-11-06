import { useRef } from "react";
import type { HealthEntry, HealthGroup, HealthView } from "../model/health";
import { Badge, Icon, useModalKeys } from "./V8";

/** FE-V8-02: the small Health pill top right (like Project_0's), opening the separate window. */
export function HealthPill({ view, onOpen }: { view: HealthView; onOpen: () => void }) {
  return <button type="button" className={`health-pill ${view.severity}`} title="View system health" data-testid="bsc-health-pill" onClick={onOpen}>
    <i className="status-lamp" /><span className="health-caption">Health · BSC</span><strong>{view.alerts} {view.alerts === 1 ? "alert" : "alerts"}</strong><Icon name="chevron" size="s" />
  </button>;
}

function Entry({ e }: { e: HealthEntry }) {
  return <div className="health-entry health-entry-rich" data-testid={`bsc-health-${e.id}`} data-state={e.state}>
    <div className="health-name"><i className={`status-lamp ${e.state === "unknown" ? "off" : e.state}`} /><div><strong>{e.name}</strong><small>{e.detail}</small></div></div>
    <span className={`health-val ${e.state === "down" || e.state === "update" ? "down" : e.state === "warn" ? "warn-text" : e.state === "unknown" || e.state === "off" ? "v8-health-grey" : ""}`}>{e.status}</span>
  </div>;
}
function Group({ g }: { g: HealthGroup }) {
  return <div className="health-group"><h3>{g.title}</h3><div className="health-rows">
    {g.entries.length ? g.entries.map(e => <Entry key={e.id} e={e} />) : <div className="health-entry health-entry-rich"><div className="health-name"><i className="status-lamp off" /><div><strong>Not reported</strong><small>No native record names any</small></div></div><span className="health-val v8-health-grey">UNKNOWN</span></div>}
  </div></div>;
}

/** The Health window: every connection and service at once, no submenus (the reference's V6). */
export function HealthWindow({ view, onClose }: { view: HealthView; onClose: () => void }) {
  const close = useRef<HTMLButtonElement>(null);
  const layer = useRef<HTMLElement>(null);
  useModalKeys(close, layer, onClose);
  return <div className="drawer-area health-overlay" data-testid="bsc-health-window">
    <button type="button" className="backdrop" aria-label="Close health window" onClick={onClose} />
    <section ref={layer} className="health-window" role="dialog" aria-modal="true" aria-labelledby="health-window-title" aria-describedby="health-window-intro">
      <header className="health-window-head"><div><div className="caps">DARKBOT · BSC</div><h2 id="health-window-title">Health · Connections & services</h2><div className="small" id="health-window-intro">All connections visible without submenus.</div></div>
        <div className="health-window-actions"><Badge label={view.alerts ? `${view.alerts} alerts` : view.severity === "ok" ? "All checked" : "Incomplete"} tone={view.alerts ? "bad" : view.severity === "ok" ? "good" : "warn"} />
          <button ref={close} type="button" className="round" aria-label="Close health window" onClick={onClose}><Icon name="x" size="s" /></button></div>
      </header>
      <div className="health-window-body"><p className="health-window-note">Native states as reported by the runtime, the gateway and the readiness evidence. Version-change monitoring of the portals (WATCH) is REV.2.0 and not active.</p>
        <div className="health-window-grid">{view.groups.map(g => <Group key={g.id} g={g} />)}</div>
      </div>
      <footer className="health-window-foot"><span>Green: healthy · Amber: degraded · Red: error or changed · Grey: off or not reported</span><button type="button" className="btn quiet sm" onClick={onClose}>Close</button></footer>
    </section>
  </div>;
}

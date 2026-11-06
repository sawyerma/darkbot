import { useEffect, useRef, type ReactNode } from "react";
import type React from "react";
import { statusTone } from "../model/status";

// V8 (FE-V8-01): the building blocks of frontend/design/reference/bsc_graphite_v8_approved.html in
// React: its icon sprite, badges, headings, stats, drawers and detail lines, with the reference's
// own class names (styled in bsc.css under .bsc-v8). No values here; every caller passes the
// native owner's or an explicit "—".

const SYMBOLS: Record<string, ReactNode> = {
  home: <><path d="m3 10 9-7 9 7v10H3z" /><path d="M9 20v-7h6v7" /></>,
  training: <><path d="M4 4h16v16H4zM4 11h16M11 4v16" /><path d="m14 15 2 2 4-4" /></>,
  live: <path d="M3 12h4l3-7 4 14 3-7h4" />,
  wallet: <><rect x="3" y="5" width="18" height="15" rx="2.5" /><path d="M3 9h18M16 14h2" /></>,
  chevron: <path d="m9 5 7 7-7 7" />,
  plus: <path d="M12 4v16M4 12h16" />,
  settings: <><path d="M4 7h16M4 17h16" /><circle cx="9" cy="7" r="3" className="v8-icon-knob" /><circle cx="16" cy="17" r="3" className="v8-icon-knob" /></>,
  info: <><circle cx="12" cy="12" r="9" /><path d="M12 11v5M12 8h.01" /></>,
  x: <path d="M5 5 19 19M19 5 5 19" />,
  search: <><circle cx="11" cy="11" r="7" /><path d="m16 16 5 5" /></>,
  copy: <><rect x="8" y="8" width="13" height="13" rx="2" /><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3" /></>,
  download: <path d="M12 3v12m-5-5 5 5 5-5M4 17v4h16v-4" />,
  trash: <path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14M10 11v7M14 11v7" />,
  lock: <><rect x="4" y="10" width="16" height="11" rx="2" /><path d="M8 10V7a4 4 0 1 1 8 0v3" /></>,
};
export type IconName = keyof typeof SYMBOLS;

/** The reference's sprite, once per page (its <symbol> ids). */
export function IconSprite() {
  return <svg xmlns="http://www.w3.org/2000/svg" style={{ display: "none" }} aria-hidden="true"><defs>
    {Object.entries(SYMBOLS).map(([id, body]) => <symbol key={id} id={`i-${id}`} viewBox="0 0 24 24">{body}</symbol>)}
  </defs></svg>;
}
export function Icon({ name, size }: { name: IconName; size?: "s" | "l" }) {
  return <svg className={`icon${size ? ` ${size}` : ""}`} aria-hidden="true"><use href={`#i-${name}`} /></svg>;
}

export type BadgeTone = "" | "good" | "bad" | "warn" | "blue";
export function Badge({ label, tone = "", testId }: { label: string; tone?: BadgeTone; testId?: string }) {
  return <span className={`badge${tone ? ` ${tone}` : ""}`} data-testid={testId}>{label}</span>;
}
/** A native state as a V8 badge: green only for the owner's qualified pass or running states. */
export function StateBadge({ state, label, testId }: { state: string | undefined; label?: string; testId?: string }) {
  const tone = statusTone(state ?? "UNKNOWN");
  return <Badge label={label ?? (state || "UNKNOWN")} tone={tone === "green" ? "good" : tone === "red" ? "bad" : tone === "amber" ? "warn" : ""} testId={testId} />;
}

export function TopHeading({ title, subtitle, actions }: { title: string; subtitle: string; actions?: ReactNode }) {
  return <div className="heading"><div><h1>{title}</h1><p>{subtitle}</p></div>{actions ? <div className="actions">{actions}</div> : null}</div>;
}
export function PanelHead({ title, extra }: { title: string; extra?: ReactNode }) {
  return <div className="topline"><h2>{title}</h2>{extra}</div>;
}
export function SystemMetric({ label, value, unit, accent, testId, title }: { label: string; value: string; unit?: string; accent?: "up" | "down" | ""; testId?: string; title?: string }) {
  return <div className="system-metric" data-testid={testId}><span className="name">{label}</span><strong className={`mono ${accent ?? ""}`} title={title}>{value} <span className="unit">{unit ?? ""}</span></strong></div>;
}
export function Stat({ label, value, note, accent, testId, title }: { label: string; value: string; note: string; accent?: "up" | "down" | ""; testId?: string; title?: string }) {
  return <div className="card stat" data-testid={testId}><span className="label">{label}</span><b className={`mono ${accent ?? ""}`} title={title}>{value}</b><small>{note}</small></div>;
}
/** An exact wei amount shown as BNB, the exact wei in its title. */
export function Wei({ wei, places, signed }: { wei: string | undefined; places?: number; signed?: boolean }) {
  return <span title={wei ? `${wei} wei` : undefined}>{signed ? signedBnbText(wei, places) : wei === undefined || wei === "" ? "—" : `${bnb(wei, places)} BNB`}</span>;
}
export function DetailLine({ label, value, testId }: { label: string; value: ReactNode; testId?: string }) {
  return <div className="detail-line" data-testid={testId}><span className="muted">{label}</span><strong>{value}</strong></div>;
}

/** A modal layer's keyboard: focus to `first` once on open and back on close, Escape closes (not
 *  while a confirmation dialog above it takes the key), Tab stays inside. Runs once per opening: a
 *  re-render (the page clock) never moves the focus. */
export function useModalKeys(first: React.RefObject<HTMLElement | null>, layer: React.RefObject<HTMLElement | null>, onClose: () => void) {
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    first.current?.focus();
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (e.defaultPrevented || document.querySelector("[role=alertdialog]")) return;
        close.current();
      } else if (e.key === "Tab" && layer.current) {
        const items = [...layer.current.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])")];
        const a = items[0], z = items[items.length - 1];
        if (!a || !z) return;
        if (e.shiftKey && document.activeElement === a) { e.preventDefault(); z.focus(); }
        else if (!e.shiftKey && document.activeElement === z) { e.preventDefault(); a.focus(); }
      }
    };
    document.addEventListener("keydown", key);
    return () => { document.removeEventListener("keydown", key); before?.focus?.(); };
  }, [first, layer]);
}

/** A side drawer of the reference: backdrop, header with kicker and title, content, footer.
 *  Escape and the backdrop close it; focus moves to its close button and returns on close. */
export function Drawer({ title, kicker, wide, footer, onClose, children, testId }: { title: string; kicker: string; wide?: boolean; footer?: ReactNode; onClose: () => void; children: ReactNode; testId?: string }) {
  const close = useRef<HTMLButtonElement>(null);
  const layer = useRef<HTMLElement>(null);
  useModalKeys(close, layer, onClose);
  return <div className="drawer-area" data-testid={testId}>
    <button type="button" className="backdrop" aria-label="Close" onClick={onClose} />
    <section ref={layer} className={`drawer${wide ? " wide" : ""}`} role="dialog" aria-modal="true" aria-label={title}>
      <header className="drawer-head"><div><span className="caps">{kicker}</span><h2>{title}</h2></div>
        <button ref={close} type="button" className="round" aria-label="Close" onClick={onClose}><Icon name="x" size="s" /></button></header>
      <div className="drawer-content">{children}</div>
      {footer ? <footer className="drawer-foot">{footer}</footer> : null}
    </section>
  </div>;
}

/** Exact decimal wei as BNB text, without floating point: integer division of the wei string.
 *  `places` digits are shown (truncated, never rounded up); callers show the full wei as a title. */
export function bnb(wei: string | undefined, places = 3): string {
  if (wei === undefined || wei === "") return "—";
  const m = /^(-?)([0-9]+)$/.exec(wei);
  if (!m) return wei;
  const [, sign, digits] = m;
  const padded = (digits ?? "0").padStart(19, "0");
  const whole = padded.slice(0, -18).replace(/^0+(?=\d)/, "");
  const frac = padded.slice(-18, -18 + places);
  return `${sign}${whole}${places ? `.${frac}` : ""}`;
}
/** Signed wei as "+x.xxx BNB" / "−x.xxx BNB" (the reference's amount()). */
export function signedBnbText(wei: string | undefined, places = 3): string {
  if (wei === undefined || wei === "") return "—";
  const text = bnb(wei, places);
  if (text === "—" || !/^-?[0-9]/.test(text)) return text;
  const zero = /^-?0\.?0*$/.test(text);
  return `${text.startsWith("-") ? "−" : zero ? "" : "+"}${text.replace(/^-/, "")} BNB`;
}
/** The tone of a signed wei amount. */
export const signTone = (wei: string | undefined): "up" | "down" | "" => !wei || !/^-?[0-9]+$/.test(wei) || /^-?0+$/.test(wei) ? "" : wei.startsWith("-") ? "down" : "up";
